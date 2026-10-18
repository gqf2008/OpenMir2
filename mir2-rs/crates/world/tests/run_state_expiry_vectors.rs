//! `Run` 状态到期（卡 `openmir2-run-gap-monster-state-expiry`）的构造级判据。
//!
//! 覆盖 oracle 四处**严格大于**到期门：
//! - `AnimalObject.Run` 的 HolySeize（`src/M2Server/Actor/AnimalObject.cs:142-149`）；
//! - `MonsterObject.Run` 的 CrazyMode（`src/M2Server/Monster/MonsterObject.cs:353-356`）；
//! - `PlayObject.Run` 的 FireHitSkill 20s（`src/M2Server/Player/PlayObject.Message.cs:54-58`）；
//! - `PlayObject.Run` 的 TwinHitSkill 60s（`:60-64`）。
//!
//! 每项至少两条：到点生效 + 恰在边界（差值 == 间隔）不生效。FireHit 另断言
//! 「到期后攻击力回落」（`get_attack_power_hit` 的 mode 7 不再翻倍）与事件序（`SysMsg` → `+UFIR`）。
//!
//! 全部使用注入时钟（`world.clock_ms`）推进；不抽随机数、不需要真机。

use mir2_world::combat::{get_attack_power_hit, AttackHitState};
use mir2_world::entity::{Entity, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};
use mir2_world::monai::{animal_run, monster_ai_tick, open_crazy_mode};
use mir2_world::{RunStateExpiry, World};

fn monster(world: &mut World) -> i32 {
    let mut e = Entity::new(0, "稻草人", 10, 10);
    e.race = ACTOR_RACE_MONSTER;
    e.hp = 60;
    e.max_hp = 60;
    world.add_entity(e)
}

fn player(world: &mut World) -> i32 {
    let mut e = Entity::new(0, "m2r", 12, 12);
    e.race = ACTOR_RACE_PLAY;
    e.hp = 100;
    e.max_hp = 100;
    world.add_entity(e)
}

// ---- ① AnimalObject HolySeize 到期（严格 `>`）-----------------------------------------------

#[test]
fn holy_seize_expiry_strictly_after_interval() {
    let mut w = World::new(32, 32);
    let id = monster(&mut w);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.ai.holy_seize = true;
        e.ai.holy_seize_tick = 1_000;
        e.ai.holy_seize_interval = 1_000;
    }
    // `now - tick == interval` ⇒ 不清（oracle 严格大于）。
    w.clock_ms = 2_000;
    animal_run(&mut w, id);
    assert!(
        w.entities.get(id).unwrap().ai.holy_seize,
        "等于间隔不清（严格大于）"
    );
    // 多 1ms ⇒ 清。
    w.clock_ms = 2_001;
    animal_run(&mut w, id);
    assert!(!w.entities.get(id).unwrap().ai.holy_seize, "过间隔即清");
}

#[test]
fn holy_seize_expiry_runs_through_monster_ai_tick() {
    let mut w = World::new(32, 32);
    let id = monster(&mut w);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.ai.holy_seize = true;
        e.ai.holy_seize_tick = 1_000;
        e.ai.holy_seize_interval = 1_000;
        // `Think()` 的 3s 门会 `base.Run(); return;` 提前跳过 `MonsterObject.Run` 尾的
        // CheckRoyalty/CrazyMode；把它压在门上，确保走到普通出口（HolySeize 到期在
        // `AnimalObject.Run`，无论哪个出口都会执行）。
        e.ai.think_tick = 2_001;
    }
    w.clock_ms = 2_001;
    monster_ai_tick(&mut w);
    assert!(
        !w.entities.get(id).unwrap().ai.holy_seize,
        "monster_ai_tick → animal_run（base.Run()）清定身"
    );
}

// ---- ② MonsterObject CrazyMode 到期 + OpenCrazyMode 写点 -----------------------------------

#[test]
fn crazy_mode_expiry_strictly_after_interval() {
    let mut w = World::new(32, 32);
    let id = monster(&mut w);
    w.clock_ms = 1_000;
    open_crazy_mode(&mut w, id, 1); // interval = 1000ms
    {
        let e = w.entities.get_mut(id).unwrap();
        assert!(e.ai.crazy_mode, "OpenCrazyMode 置态");
        assert_eq!(e.ai.crazy_mode_tick, 1_000, "CrazyModeTick = now");
        assert_eq!(e.ai.crazy_mode_interval, 1_000, "nTime * 1000");
        // 避免 `Think()` 早退（见上）。
        e.ai.think_tick = 2_000;
    }
    // 边界：差值 == interval ⇒ 不清。
    w.clock_ms = 2_000;
    monster_ai_tick(&mut w);
    assert!(
        w.entities.get(id).unwrap().ai.crazy_mode,
        "等于间隔不清（严格大于）"
    );
    // 过 1ms ⇒ 清（普通出口的 `CheckRoyalty()` 之后）。
    w.clock_ms = 2_001;
    if let Some(e) = w.entities.get_mut(id) {
        e.ai.think_tick = 2_001;
    }
    monster_ai_tick(&mut w);
    assert!(!w.entities.get(id).unwrap().ai.crazy_mode, "过间隔即清");
}

#[test]
fn crazy_mode_expiry_runs_on_non_movable_path() {
    // oracle：外层 `CanMove() && ...` 为假时仍落 `CheckRoyalty(); CrazyMode 到期; base.Run()`。
    let mut w = World::new(32, 32);
    let id = monster(&mut w);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.stone_mode = true; // `!StoneMode` 门失败 ⇒ 不可移动路径
        e.ai.crazy_mode = true;
        e.ai.crazy_mode_tick = 1_000;
        e.ai.crazy_mode_interval = 1_000;
    }
    w.clock_ms = 2_001;
    monster_ai_tick(&mut w);
    assert!(
        !w.entities.get(id).unwrap().ai.crazy_mode,
        "不可移动路径也要到期（oracle 落 CheckRoyalty 后）"
    );
}

// ---- ③ FireHitSkill 20s 到期 ----------------------------------------------------------------

#[test]
fn fire_hit_expiry_strictly_after_20s_and_event_order() {
    let mut w = World::new(32, 32);
    let id = player(&mut w);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.fire_hit_skill = true;
        e.latest_fire_hit_tick = 1_000;
    }
    // 边界：20_000 == 20*1000 ⇒ 不清。
    w.clock_ms = 21_000;
    assert!(w.expire_player_run_state().is_empty());
    assert!(w.entities.get(id).unwrap().fire_hit_skill);
    // 过 1ms ⇒ 清；`SysMsg(SpiritsGone)` 先于 `SendSocket("+UFIR")`。
    w.clock_ms = 21_001;
    assert_eq!(
        w.expire_player_run_state(),
        vec![
            (id, RunStateExpiry::SpiritsGone),
            (id, RunStateExpiry::FireHitSocket),
        ]
    );
    assert!(!w.entities.get(id).unwrap().fire_hit_skill);
    // 清一次后不重复发事件。
    assert!(w.expire_player_run_state().is_empty());
}

#[test]
fn fire_hit_expiry_falls_back_attack_power() {
    let mut w = World::new(32, 32);
    let id = player(&mut w);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.fire_hit_skill = true;
        e.latest_fire_hit_tick = 1_000;
    }
    // 到期前：mode 7 走 `nPower + Round(nPower/100*HitDouble*10)`（250 + 200 = 450）。
    let before = w.entities.get(id).unwrap();
    let mut state = AttackHitState {
        fire_hit_skill: before.fire_hit_skill,
        hit_double: 10,
        ..Default::default()
    };
    let (power_before, can_before) = get_attack_power_hit(7, 250, true, &mut state);
    assert_eq!((power_before, can_before), (450, true), "烈火加成生效");

    // 到期后：标志清除 ⇒ 同一 mode 7 不翻倍、不置 canHit（攻击力回落）。
    w.clock_ms = 21_001;
    let _ = w.expire_player_run_state();
    let after = w.entities.get(id).unwrap();
    let mut state = AttackHitState {
        fire_hit_skill: after.fire_hit_skill,
        hit_double: 10,
        ..Default::default()
    };
    let (power_after, can_after) = get_attack_power_hit(7, 250, true, &mut state);
    assert_eq!(
        (power_after, can_after),
        (250, false),
        "到期后回落基础攻击力"
    );
    assert!(power_after < power_before);
}

// ---- ④ TwinHitSkill 60s 到期 ----------------------------------------------------------------

#[test]
fn twin_hit_expiry_strictly_after_60s() {
    let mut w = World::new(32, 32);
    let id = player(&mut w);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.twin_hit_skill = true;
        e.latest_twin_hit_tick = 1_000;
    }
    // 边界：60_000 == 60*1000 ⇒ 不清。
    w.clock_ms = 61_000;
    assert!(w.expire_player_run_state().is_empty());
    assert!(w.entities.get(id).unwrap().twin_hit_skill);
    // 过 1ms ⇒ 清；双龙只有 `SendSocket("+UTWN")`（无 SysMsg）。
    w.clock_ms = 61_001;
    assert_eq!(
        w.expire_player_run_state(),
        vec![(id, RunStateExpiry::TwinHitSocket)]
    );
    assert!(!w.entities.get(id).unwrap().twin_hit_skill);
}

#[test]
fn both_expiries_keep_oracle_event_order() {
    let mut w = World::new(32, 32);
    let id = player(&mut w);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.fire_hit_skill = true;
        e.latest_fire_hit_tick = 1_000;
        e.twin_hit_skill = true;
        e.latest_twin_hit_tick = 1_000;
    }
    w.clock_ms = 61_001;
    assert_eq!(
        w.expire_player_run_state(),
        vec![
            (id, RunStateExpiry::SpiritsGone),
            (id, RunStateExpiry::FireHitSocket),
            (id, RunStateExpiry::TwinHitSocket),
        ]
    );
}

#[test]
fn expiry_is_player_only_and_world_tick_records_it() {
    let mut w = World::new(32, 32);
    let pid = player(&mut w);
    let mid = monster(&mut w);
    {
        let e = w.entities.get_mut(pid).unwrap();
        e.fire_hit_skill = true;
        e.latest_fire_hit_tick = 0;
        e.twin_hit_skill = true;
        e.latest_twin_hit_tick = 0;
    }
    // 非玩家即使字段被置位也不清（oracle 的到期段在 `PlayObject.Run`）。
    {
        let e = w.entities.get_mut(mid).unwrap();
        e.fire_hit_skill = true;
        e.twin_hit_skill = true;
    }
    w.clock_ms = 60_000; // tick 内部 +200 ⇒ 60_200（> 20s 且 > 60s）
    let rec = w.tick();
    assert_eq!(
        rec.run_state_expiries,
        vec![
            (pid, RunStateExpiry::SpiritsGone),
            (pid, RunStateExpiry::FireHitSocket),
            (pid, RunStateExpiry::TwinHitSocket),
        ],
        "world tick 记录到期事件（槽位顺序）"
    );
    assert!(!w.entities.get(pid).unwrap().fire_hit_skill);
    assert!(!w.entities.get(pid).unwrap().twin_hit_skill);
    assert!(
        w.entities.get(mid).unwrap().fire_hit_skill,
        "怪物不走 PlayObject.Run 到期段"
    );
}
