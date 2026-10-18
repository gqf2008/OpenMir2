//! 卡 `openmir2-run-gap-monster-specials`（tier=R）：构造级判据。
//!
//! oracle：
//! - `src/M2Server/Monster/Monsters/BeeQueen.cs`
//! - `src/M2Server/Monster/Monsters/SpiderHouseMonster.cs`
//! - `src/M2Server/Monster/Monsters/Clone.cs`
//!
//! 每个用例的 `oracle` 行号在断言旁注明；牙（mutation）见本卡报告。

use mir2_shared::rng::RandomNumber;
use mir2_world::entity::{Entity, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};
use mir2_world::slave::{RM_DOOPENHEALTH, RM_MAKEHOLYSEIZEMODE, RM_POISON};
use mir2_world::special_monster::{
    adopt_special_child, apply_ctor_fields, clone_operate, operate_special, special_monster_run,
    special_spawn_request, SpecialSpawner, CHILD_LIMIT, RACE_BEE_QUEEN, RACE_CLONE_MONSTER,
    RACE_SPIDER_HOUSE, RM_DELAYPUSHED, RM_MAGSTRUCK, RM_RANDOMSPACEMOVE, RM_REFMESSAGE, RM_SPELL,
    RM_STRUCK, RM_ZEN_BEE, ZEN_BEE_DELAY_MS,
};
use mir2_world::World;

/// `RegenMonsterByName` 的 mock：记录调用，成功时造一只 bee/子怪实体。
#[derive(Default)]
struct MockSpawner {
    calls: Vec<(usize, String, i16, i16)>,
    fail: bool,
}

impl SpecialSpawner for MockSpawner {
    fn regen_monster_by_name(
        &mut self,
        world: &mut World,
        _rng: &mut RandomNumber,
        map_id: usize,
        name: &str,
        x: i16,
        y: i16,
    ) -> Option<i32> {
        self.calls.push((map_id, name.to_string(), x, y));
        if self.fail {
            return None;
        }
        let mut e = Entity::new(0, name.to_string(), i32::from(x), i32::from(y));
        e.race = 125; // `ActorRace.MonsterBee`
        e.map_id = map_id;
        Some(world.add_entity(e))
    }
}

fn special_owner(world: &mut World, race: u8, x: i32, y: i32) -> i32 {
    let mut e = Entity::new(0, "special".to_string(), x, y);
    e.race = race;
    e.hp = 100;
    e.max_hp = 100;
    world.add_entity(e)
}

fn plain_child(world: &mut World, x: i32, y: i32) -> i32 {
    let mut e = Entity::new(0, "bee".to_string(), x, y);
    e.race = 125;
    world.add_entity(e)
}

// ---- ctor 字段（不抽数部分）-------------------------------------------------------------

/// `BeeQueen.cs:15-19`：`ViewRange=9`、`SearchTick=now`、`StickMode=true`；
/// `SpiderHouseMonster.cs:15-19`：`ViewRange=9`、**`SearchTick=0`**、`StickMode=true`；
/// `Clone.cs:9-12`：只改 `SearchTime`（`SearchTick` 保持 Base 值）。
#[test]
fn ctor_fields_match_oracle_per_race() {
    let mut bee = Entity::new(0, "bee".to_string(), 1, 1);
    bee.race = RACE_BEE_QUEEN;
    bee.ai.search_tick = 77;
    apply_ctor_fields(&mut bee, 5000);
    assert_eq!(bee.view_range, 9, "BeeQueen ViewRange=9");
    assert!(bee.stick_mode, "BeeQueen StickMode=true");
    assert_eq!(bee.ai.search_tick, 5000, "BeeQueen SearchTick=now");

    let mut spider = Entity::new(0, "spider".to_string(), 1, 1);
    spider.race = RACE_SPIDER_HOUSE;
    spider.ai.search_tick = 77;
    apply_ctor_fields(&mut spider, 5000);
    assert_eq!(spider.view_range, 9, "SpiderHouse ViewRange=9");
    assert!(spider.stick_mode, "SpiderHouse StickMode=true");
    assert_eq!(
        spider.ai.search_tick, 0,
        "SpiderHouse ctor 显式 SearchTick=0"
    );

    let mut clone = Entity::new(0, "clone".to_string(), 1, 1);
    clone.race = RACE_CLONE_MONSTER;
    clone.ai.search_tick = 77;
    apply_ctor_fields(&mut clone, 5000);
    assert_eq!(clone.view_range, 5, "Clone 不改 ViewRange（Base=5）");
    assert!(!clone.stick_mode, "Clone 不改 StickMode");
    assert_eq!(clone.ai.search_tick, 5000, "Clone SearchTick 保持 Base=now");
}

/// 生成路径（`spawn_monster_precomputed`）在 `seed_from_spawn` 之后落 ctor 字段；
/// `SearchTime` 必须原样保留构造抽的值（`Random(1500)+2500` / `+1500` 在宿主侧抽）。
#[test]
fn spawn_precomputed_applies_ctor_fields_and_keeps_search_time() {
    let mut w = World::new(32, 32);
    w.clock_ms = 50_000;
    let ctor = |search_time: i32| mir2_world::MonsterCtorDraws {
        search_time,
        ..Default::default()
    };
    let bee = w
        .spawn_monster_precomputed(
            "蜂后",
            5,
            5,
            0,
            RACE_BEE_QUEEN,
            1,
            1,
            100,
            0,
            0,
            0,
            0,
            0,
            0,
            vec![],
            ctor(4100),
            2,
            false,
        )
        .expect("bee spawn");
    let e = w.entities.get(bee).unwrap();
    assert_eq!(e.ai.search_time, 4100, "构造抽的 SearchTime 原样落库");
    assert_eq!(e.ai.search_tick, 50_000);
    assert_eq!(e.view_range, 9);
    assert!(e.stick_mode);

    let spider = w
        .spawn_monster_precomputed(
            "蜘蛛巢",
            6,
            5,
            0,
            RACE_SPIDER_HOUSE,
            1,
            1,
            100,
            0,
            0,
            0,
            0,
            0,
            0,
            vec![],
            ctor(4200),
            2,
            false,
        )
        .expect("spider spawn");
    let e = w.entities.get(spider).unwrap();
    assert_eq!(e.ai.search_time, 4200);
    assert_eq!(e.ai.search_tick, 0, "SpiderHouse SearchTick=0");
    assert_eq!(e.view_range, 9);
    assert!(e.stick_mode);

    let clone = w
        .spawn_monster_precomputed(
            "分身",
            7,
            5,
            0,
            RACE_CLONE_MONSTER,
            1,
            1,
            100,
            0,
            0,
            0,
            0,
            0,
            0,
            vec![],
            ctor(3300),
            2,
            false,
        )
        .expect("clone spawn");
    let e = w.entities.get(clone).unwrap();
    assert_eq!(e.ai.search_time, 3300);
    assert_eq!(e.ai.search_tick, 50_000);
}

// ---- BeeQueen/SpiderHouse Run -------------------------------------------------------------

/// `(now - WalkTick) >= WalkSpeed` —— **>=**：1399 不动，1400 置 `WalkTick=now`。
#[test]
fn run_walk_tick_boundary_is_ge_not_gt() {
    let mut w = World::new(32, 32);
    let id = special_owner(&mut w, RACE_BEE_QUEEN, 5, 5);
    w.entities.get_mut(id).unwrap().ai.walk_tick = 0;
    w.entities.get_mut(id).unwrap().ai.attack_tick = 0;
    w.entities.get_mut(id).unwrap().ai.walk_speed = 1400;
    w.entities.get_mut(id).unwrap().ai.next_hit_time = 1_000_000; // 攻门不参与本判据

    w.clock_ms = 1399;
    assert!(special_monster_run(&mut w, id));
    assert_eq!(
        w.entities.get(id).unwrap().ai.walk_tick,
        0,
        "1399 < 1400 不动"
    );

    w.clock_ms = 1400;
    assert!(special_monster_run(&mut w, id));
    assert_eq!(
        w.entities.get(id).unwrap().ai.walk_tick,
        1400,
        "恰好 1400 必须到点（>=）"
    );
}

/// `(now - AttackTick) >= NextHitTime` —— **>=**：差 1999 不动，2000 置 `AttackTick=now`。
#[test]
fn run_attack_tick_boundary_is_ge_not_gt() {
    let mut w = World::new(32, 32);
    let id = special_owner(&mut w, RACE_BEE_QUEEN, 5, 5);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.ai.walk_tick = 0;
        e.ai.walk_speed = 1400;
        e.ai.attack_tick = 3000;
        e.ai.next_hit_time = 2000;
    }
    w.clock_ms = 4999; // 4999-3000 = 1999
    special_monster_run(&mut w, id);
    assert_eq!(
        w.entities.get(id).unwrap().ai.attack_tick,
        3000,
        "1999 < 2000 不动"
    );

    // 上一跑把 WalkTick 置为 4999 ⇒ 第二跑先把 WalkTick 归零，使攻门可再求值。
    w.entities.get_mut(id).unwrap().ai.walk_tick = 0;
    w.clock_ms = 5000; // 5000-3000 = 2000
    special_monster_run(&mut w, id);
    assert_eq!(
        w.entities.get(id).unwrap().ai.attack_tick,
        5000,
        "恰好 2000 必须到点（>=）"
    );
}

/// `BeeQueen.cs:23-27`：`BeeList.Count >= 15` ⇒ 直接返回（不 `SendRefMsg`、不 `SendSelfDelayMsg`）。
#[test]
fn make_child_stops_at_15() {
    let mut w = World::new(32, 32);
    let id = special_owner(&mut w, RACE_BEE_QUEEN, 5, 5);
    let target = plain_child(&mut w, 6, 5);
    let children: Vec<i32> = (0..CHILD_LIMIT)
        .map(|i| plain_child(&mut w, 10 + i as i32, 10))
        .collect();
    {
        let e = w.entities.get_mut(id).unwrap();
        e.ai.target_creature = Some(target);
        e.ai.walk_tick = 0;
        e.ai.attack_tick = 0;
        e.ai.walk_speed = 1400;
        e.ai.next_hit_time = 2000;
        e.special_children = children;
    }
    w.clock_ms = 5000;
    let before_refs = w.pending_special_ref_msgs.len();
    let before_delays = w.self_delay_msgs.len();
    special_monster_run(&mut w, id);
    assert_eq!(
        w.pending_special_ref_msgs.len(),
        before_refs,
        "15 只不再发 RM_HIT"
    );
    assert_eq!(
        w.self_delay_msgs.len(),
        before_delays,
        "15 只不再排 RM_ZEN_BEE"
    );
    assert_eq!(
        w.entities.get(id).unwrap().special_children.len(),
        CHILD_LIMIT
    );
}

/// 14 只时仍产仔：恰一条 `RM_HIT` + 一条 `RM_ZEN_BEE`（500ms）。
#[test]
fn make_child_below_limit_sends_hit_and_delay() {
    let mut w = World::new(32, 32);
    let id = special_owner(&mut w, RACE_BEE_QUEEN, 5, 5);
    let target = plain_child(&mut w, 6, 5);
    let children: Vec<i32> = (0..(CHILD_LIMIT - 1))
        .map(|i| plain_child(&mut w, 10 + i as i32, 20))
        .collect();
    {
        let e = w.entities.get_mut(id).unwrap();
        e.ai.target_creature = Some(target);
        e.ai.walk_tick = 0;
        e.ai.attack_tick = 0;
        e.ai.walk_speed = 1400;
        e.ai.next_hit_time = 2000;
        e.special_children = children;
    }
    w.clock_ms = 5000;
    special_monster_run(&mut w, id);
    let refs = w.take_special_ref_msgs();
    assert_eq!(refs.len(), 1, "一条 RM_HIT");
    assert_eq!(refs[0].ident, 10004);
    assert_eq!((refs[0].x, refs[0].y), (5, 5));
    assert_eq!(w.self_delay_msgs.len(), 1, "一条 RM_ZEN_BEE");
    assert_eq!(w.self_delay_msgs[0].ident, RM_ZEN_BEE);
    assert_eq!(
        w.self_delay_msgs[0].deliver_at_ms, 5500,
        "DeliveryTime = now + 500"
    );
    assert_eq!(w.self_delay_msgs[0].deliver_at_ms - 5000, ZEN_BEE_DELAY_MS);
}

/// `BeeQueen.cs:63-71`：倒序遍历移除 `Death || Ghost`（实体已不存在同样移除），
/// 其余项**保序**。
#[test]
fn child_list_cleanup_removes_dead_ghost_and_missing_in_reverse() {
    let mut w = World::new(32, 32);
    let id = special_owner(&mut w, RACE_BEE_QUEEN, 5, 5);
    let alive1 = plain_child(&mut w, 1, 1);
    let dead = plain_child(&mut w, 2, 1);
    let ghost = plain_child(&mut w, 3, 1);
    let missing = plain_child(&mut w, 4, 1);
    let alive2 = plain_child(&mut w, 5, 1);
    w.entities.get_mut(dead).unwrap().death = true;
    w.entities.get_mut(ghost).unwrap().ghost = true;
    w.entities.remove(missing);
    {
        let e = w.entities.get_mut(id).unwrap();
        e.special_children = vec![alive1, dead, ghost, missing, alive2];
        e.ai.walk_tick = 0;
        e.ai.attack_tick = 0;
        e.ai.walk_speed = 1400;
        e.ai.next_hit_time = 1_000_000; // 不进产仔段：只验清理
    }
    w.clock_ms = 1400;
    special_monster_run(&mut w, id);
    assert_eq!(
        w.entities.get(id).unwrap().special_children,
        vec![alive1, alive2],
        "倒序移除 Death/Ghost/缺失，剩余保序"
    );
}

// ---- Operate(RM_ZEN_BEE) ------------------------------------------------------------------

/// `SpiderHouseMonster.cs:35-42`：坐标 `(CurrX, CurrY+1)` 且先判 `CanWalk(..., true)`；
/// `BeeQueen.cs:35-41`：坐标 `(CurrX, CurrY)`、无 CanWalk 门。
#[test]
fn spider_spawns_at_y_plus_one_and_requires_canwalk() {
    let mut w = World::new(32, 32);
    let spider = special_owner(&mut w, RACE_SPIDER_HOUSE, 10, 10);
    let mut sp = MockSpawner::default();
    assert_eq!(
        operate_special(&mut w, &mut sp, spider, RM_ZEN_BEE, 0, 0),
        Some(true)
    );
    assert_eq!(sp.calls.len(), 1);
    assert_eq!(sp.calls[0].0, 0);
    assert_eq!(sp.calls[0].1, "爆裂蜘蛛", "Config.Spider 缺省");
    assert_eq!((sp.calls[0].2, sp.calls[0].3), (10, 11), "CurrY + 1");

    // 下一格不可走 ⇒ 不造怪（`CanWalk(..., true)`）。
    let mut w2 = World::new(32, 32);
    let spider2 = special_owner(&mut w2, RACE_SPIDER_HOUSE, 10, 10);
    assert!(w2.maps[0].set_map_xy_flag(10, 11, false), "置墙");
    let mut sp2 = MockSpawner::default();
    assert_eq!(
        operate_special(&mut w2, &mut sp2, spider2, RM_ZEN_BEE, 0, 0),
        Some(true)
    );
    assert!(sp2.calls.is_empty(), "CanWalk 失败 ⇒ 零造怪");

    // BeeQueen：`(CurrX, CurrY)`，格不可走也照造。
    let mut w3 = World::new(32, 32);
    let bee = special_owner(&mut w3, RACE_BEE_QUEEN, 10, 10);
    assert!(w3.maps[0].set_map_xy_flag(10, 10, false));
    let mut sp3 = MockSpawner::default();
    assert_eq!(
        operate_special(&mut w3, &mut sp3, bee, RM_ZEN_BEE, 0, 0),
        Some(true)
    );
    assert_eq!(sp3.calls.len(), 1);
    assert_eq!((sp3.calls[0].2, sp3.calls[0].3), (10, 10), "Bee 用原格");
    assert_eq!(sp3.calls[0].1, "蝙蝠", "Config.Bee 缺省");
}

/// `bb.SetTargetCreat(TargetCret)` + `BeeList.Add(bb)`（`BeeQueen.cs:40-41`）。
#[test]
fn spawned_child_inherits_target_and_joins_list() {
    let mut w = World::new(32, 32);
    let bee = special_owner(&mut w, RACE_BEE_QUEEN, 10, 10);
    let enemy = {
        let mut e = Entity::new(0, "enemy".to_string(), 12, 10);
        e.race = ACTOR_RACE_PLAY;
        w.add_entity(e)
    };
    w.entities.get_mut(bee).unwrap().ai.target_creature = Some(enemy);
    let mut sp = MockSpawner::default();
    operate_special(&mut w, &mut sp, bee, RM_ZEN_BEE, 0, 0);
    let child = w.entities.get(bee).unwrap().special_children[0];
    assert_eq!(
        w.entities.get(child).unwrap().ai.target_creature,
        Some(enemy),
        "宝宝继承产仔者的当前目标"
    );
    assert_eq!(w.entities.get(child).unwrap().race, 125);
}

/// 造怪失败（`bb == null`）⇒ 不入表。
#[test]
fn spawn_failure_does_not_join_list() {
    let mut w = World::new(32, 32);
    let bee = special_owner(&mut w, RACE_BEE_QUEEN, 10, 10);
    let mut sp = MockSpawner {
        fail: true,
        ..Default::default()
    };
    assert_eq!(
        operate_special(&mut w, &mut sp, bee, RM_ZEN_BEE, 0, 0),
        Some(true)
    );
    assert!(w.entities.get(bee).unwrap().special_children.is_empty());
}

/// 非 `RM_ZEN_BEE` 的消息 ⇒ `None`（走 base.Operate）；非专用怪同样 `None`。
#[test]
fn operate_special_returns_none_for_base_cases() {
    let mut w = World::new(32, 32);
    let bee = special_owner(&mut w, RACE_BEE_QUEEN, 10, 10);
    let mut sp = MockSpawner::default();
    assert_eq!(
        operate_special(&mut w, &mut sp, bee, RM_STRUCK, 1, 0),
        None,
        "BeeQueen 的 RM_STRUCK 走 base.Operate"
    );
    let normal = {
        let mut e = Entity::new(0, "normal".to_string(), 3, 3);
        e.race = ACTOR_RACE_MONSTER;
        w.add_entity(e)
    };
    assert_eq!(
        operate_special(&mut w, &mut sp, normal, RM_ZEN_BEE, 0, 0),
        None,
        "普通怪无专用 Operate"
    );
}

// ---- CloneMonster.Operate -----------------------------------------------------------------

/// `Clone.cs:22-27`：`RM_STRUCK` ⇒ `Master.WAbil.MP -= (ushort)wParam`。
#[test]
fn clone_struck_deducts_wparam_from_master_mp() {
    let (mut w, master, clone) = clone_world(100);
    assert!(!clone_operate(&mut w, clone, RM_STRUCK, 30, 0));
    assert_eq!(w.entities.get(master).unwrap().mp, 70);
}

/// `Clone.cs:22-27`：`RM_MAGSTRUCK` 同样扣 `wParam`（已知发送点传 0）。
#[test]
fn clone_magstruck_deducts_wparam_from_master_mp() {
    let (mut w, master, clone) = clone_world(100);
    assert!(!clone_operate(&mut w, clone, RM_MAGSTRUCK, 40, 0));
    assert_eq!(w.entities.get(master).unwrap().mp, 60);
}

/// `Clone.cs:22-24`：`RM_SPELL` 扣 `(ushort)nParam3`（= `MagicId`），**不**扣 `wParam`。
#[test]
fn clone_spell_deducts_nparam3_from_master_mp() {
    let (mut w, master, clone) = clone_world(100);
    assert!(!clone_operate(&mut w, clone, RM_SPELL, 99, 17));
    assert_eq!(w.entities.get(master).unwrap().mp, 83);
}

/// `Clone.cs:20-23`：**先**判 `Master.WAbil.MP <= 0 ⇒ WAbil.HP = 0`，**再**扣 MP；
/// `ushort` 复合赋值是 unchecked 回绕（0 - 5 ⇒ 65531）。
#[test]
fn clone_master_mp_zero_kills_clone_then_wraps_mp() {
    let (mut w, master, clone) = clone_world(0);
    assert_eq!(w.entities.get(clone).unwrap().hp, 100);
    assert!(!clone_operate(&mut w, clone, RM_STRUCK, 5, 0));
    assert_eq!(w.entities.get(clone).unwrap().hp, 0, "MP<=0 ⇒ 分身 HP=0");
    assert_eq!(
        w.entities.get(master).unwrap().mp,
        65_531,
        "ushort 复合赋值回绕（0 - 5）"
    );
}

/// `Clone.cs:30`：恒 `return false`（不跑 `base.Operate`）——三个 ident 与任意其它 ident 都一样。
#[test]
fn clone_operate_always_returns_false() {
    let (mut w, _master, clone) = clone_world(100);
    for ident in [RM_STRUCK, RM_MAGSTRUCK, RM_SPELL, 4242, 0] {
        assert!(
            !clone_operate(&mut w, clone, ident, 9, 9),
            "ident {ident} 必须返回 false"
        );
    }
}

/// **F2 防回归（RM_STRUCK 屏弊）**：`base_object_run_monster` 出队 `RM_STRUCK` 时必须先判
/// 「是 Clone ⇒ `clone_operate` 并终止」——`Master.MP -= wParam` 生效，而
/// `AnimalObject.Operate` 的 `SetLastHiter`/`Struck` 都不执行（`Clone.cs:14-30` 恒 false；
/// `Clone.Struck` 本身也是空实现，`Clone.cs:57-63`）。
#[test]
fn clone_struck_shield_deducts_master_mp_not_hp() {
    use mir2_world::entity::StruckMsg;
    let (mut w, master, clone) = clone_world(100);
    let mut h = Entity::new(0, "hitter".to_string(), 6, 4);
    h.race = ACTOR_RACE_PLAY;
    let hitter = w.add_entity(h);
    w.entities.get_mut(clone).unwrap().ai.pending_struck = vec![StruckMsg {
        hiter: hitter,
        w_param: 30,
    }];
    mir2_world::monai::base_object_run_monster(&mut w, clone);
    assert_eq!(
        w.entities.get(master).unwrap().mp,
        70,
        "RM_STRUCK 屏弊 ⇒ Master.MP -= (ushort)wParam"
    );
    let c = w.entities.get(clone).unwrap();
    assert!(c.ai.pending_struck.is_empty(), "队列必须出队");
    assert_eq!(
        c.hp, 100,
        "Clone.Struck 空实现 + base.Operate 不跑 ⇒ 不扣 HP"
    );
    assert_eq!(c.last_hiter, None, "SetLastHiter 不得执行");
}

/// **F2（复验 Important）**：`CloneMonster.Operate`（`Clone.cs:14-30`）对**任意**消息都
/// `return false` ⇒ `RM_POISON` 的 `MakePosion` 不得执行（原缺陷：Clone 会真的中毒）。
/// 投递点 = `deliver_self_delay_msgs` 的 `RM_POISON` 支（`BaseObject.Base.cs:740-759`）。
#[test]
fn clone_swallows_poison_delivery() {
    let (mut w, master, clone) = clone_world(100);
    // `SendSelfDelayMsg(RM_POISON, type, nPower, caster, point, "", 1000)`。
    w.send_self_delay_msg_full(clone, RM_POISON, 1, 30, master, 7, 0);
    let outs = w.deliver_self_delay_msgs();
    assert_eq!(outs.len(), 1, "消息仍须出队（不能被 Ghost 门保留）");
    assert!(
        outs[0].poison.is_none(),
        "Clone 的 Operate 吞掉 RM_POISON ⇒ 不得产出 PoisonDelivery"
    );
    let c = w.entities.get(clone).unwrap();
    assert_eq!(c.green_poisoning_point, 0, "不得写 DoT 点");
    assert_eq!(c.char_status, 0, "不得置中毒 CharStatus");
    assert_eq!(c.ai.target_creature, None, "不得 SetTargetCreat");
}

/// **F2**：`RM_MAKEHOLYSEIZEMODE`（`AnimalObject.cs:322-328`）对 Clone 不生效。
#[test]
fn clone_swallows_make_holy_seize_mode_delivery() {
    let (mut w, _master, clone) = clone_world(100);
    w.send_self_delay_msg_full(clone, RM_MAKEHOLYSEIZEMODE, 1500, 0, 0, 0, 0);
    let outs = w.deliver_self_delay_msgs();
    assert_eq!(outs.len(), 1);
    assert!(outs[0].name_color.is_none(), "Clone 不得产出 RefNameColor");
    let c = w.entities.get(clone).unwrap();
    assert!(!c.ai.holy_seize, "不得进入神圣战甲模式");
    assert_eq!(c.ai.holy_seize_interval, 0);
}

/// **F2**：`RM_DOOPENHEALTH`（`BaseObject.cs:944-950`）对 Clone 不生效。
#[test]
fn clone_swallows_do_open_health_delivery() {
    let (mut w, _master, clone) = clone_world(100);
    w.send_self_delay_msg_full(clone, RM_DOOPENHEALTH, 0, 0, 0, 0, 0);
    let outs = w.deliver_self_delay_msgs();
    assert_eq!(outs.len(), 1);
    assert!(
        outs[0].open_health.is_none(),
        "Clone 不得产出 MakeOpenHealth"
    );
    assert!(!w.entities.get(clone).unwrap().show_hp, "不得置 ShowHp");
}

/// **F2 构造级判据（每类 ≥1 条「不生效」）**：本仓已建模/已登记的全部投递类
/// （`RM_POISON`/`RM_MAKEHOLYSEIZEMODE`/`RM_REFMESSAGE`/`RM_DOOPENHEALTH`/
/// `RM_RANDOMSPACEMOVE`/`RM_DELAYPUSHED`）投给 Clone ⇒ `Operate` 恒 `false` 且
/// **不改任何状态**（`base.Operate` 不跑）。
#[test]
fn clone_operate_swallows_every_modeled_delivery_class() {
    for ident in [
        RM_POISON,
        RM_MAKEHOLYSEIZEMODE,
        RM_REFMESSAGE,
        RM_DOOPENHEALTH,
        RM_RANDOMSPACEMOVE,
        RM_DELAYPUSHED,
    ] {
        let (mut w, master, clone) = clone_world(100);
        let mut sp = MockSpawner::default();
        assert_eq!(
            operate_special(&mut w, &mut sp, clone, ident, 30, 7),
            Some(false),
            "ident {ident} 必须走 Clone.Operate 并返回 false"
        );
        assert_eq!(
            w.entities.get(master).unwrap().mp,
            100,
            "ident {ident} 不在三 ident 内 ⇒ 不得扣主人 MP"
        );
        let c = w.entities.get(clone).unwrap();
        assert_eq!(c.hp, 100, "ident {ident} 不得改 HP");
        assert_eq!(c.char_status, 0, "ident {ident} 不得改 CharStatus");
        assert_eq!(c.ai.target_creature, None, "ident {ident} 不得置目标");
        assert!(!c.ai.holy_seize, "ident {ident} 不得进神圣模式");
        assert!(!c.show_hp, "ident {ident} 不得开血");
    }
}

/// 无 Master（或 Master 已不在仓库）⇒ 不改任何值，仍返回 false。
#[test]
fn clone_without_master_is_noop_but_false() {
    let mut w = World::new(16, 16);
    let mut e = Entity::new(0, "clone".to_string(), 3, 3);
    e.race = RACE_CLONE_MONSTER;
    e.hp = 80;
    let clone = w.add_entity(e);
    assert!(!clone_operate(&mut w, clone, RM_STRUCK, 30, 0));
    assert_eq!(w.entities.get(clone).unwrap().hp, 80);

    // Master 引用悬空（已从仓库移除）⇒ 等价 null。
    let (mut w2, master, clone2) = clone_world(100);
    w2.entities.remove(master);
    assert!(!clone_operate(&mut w2, clone2, RM_STRUCK, 30, 0));
}

/// `Operate` 分派对 Clone 返回 `Some(false)`，对 Bee/Spider 的 `RM_ZEN_BEE` 返回 `Some(true)`。
#[test]
fn operate_special_dispatch_contract() {
    let (mut w, _master, clone) = clone_world(100);
    let mut sp = MockSpawner::default();
    assert_eq!(
        operate_special(&mut w, &mut sp, clone, RM_STRUCK, 1, 0),
        Some(false),
        "Clone 的 Operate 恒 false"
    );
}

// ---- RM_ZEN_BEE 自投递队列 -----------------------------------------------------------------

/// 500ms 自投递在 `deliver_self_delay_msgs` 到点后进 `pending_special_spawns`，
/// `flush_special_spawns` 用宿主 spawner 造怪并落尾。
#[test]
fn zen_bee_delay_queues_then_flush_spawns_and_adopts() {
    let mut w = World::new(32, 32);
    let bee = special_owner(&mut w, RACE_BEE_QUEEN, 10, 10);
    let target = plain_child(&mut w, 12, 10);
    {
        let e = w.entities.get_mut(bee).unwrap();
        e.ai.target_creature = Some(target);
        e.ai.walk_tick = 0;
        e.ai.attack_tick = 0;
        e.ai.walk_speed = 1400;
        e.ai.next_hit_time = 2000;
    }
    w.clock_ms = 2000;
    special_monster_run(&mut w, bee);
    assert_eq!(w.self_delay_msgs.len(), 1);

    // 未到点 ⇒ 不投递。
    let early = w.deliver_self_delay_msgs();
    assert!(early.is_empty());
    assert_eq!(w.self_delay_msgs.len(), 1);
    assert!(w.pending_special_spawns.is_empty());

    // 到点 ⇒ 入产仔队列。
    w.clock_ms = 2500;
    let outs = w.deliver_self_delay_msgs();
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0].ident, RM_ZEN_BEE);
    let req = outs[0].zen_bee.as_ref().expect("zen_bee 请求");
    assert_eq!((req.x, req.y), (10, 10));
    assert_eq!(req.name, "蝙蝠");
    assert_eq!(w.pending_special_spawns.len(), 1);

    let mut sp = MockSpawner::default();
    let flushed = w.flush_special_spawns(&mut sp);
    assert_eq!(flushed.len(), 1);
    let child = flushed[0].child_id.expect("造怪成功");
    assert_eq!(w.entities.get(bee).unwrap().special_children, vec![child]);
    assert_eq!(
        w.entities.get(child).unwrap().ai.target_creature,
        Some(target)
    );
    assert!(w.pending_special_spawns.is_empty());
}

/// 给 `spawner` 的版本在投递点**同步**造怪（oracle `Operate` 同点），不入队列。
#[test]
fn zen_bee_delivery_with_spawner_is_synchronous() {
    let mut w = World::new(32, 32);
    let spider = special_owner(&mut w, RACE_SPIDER_HOUSE, 10, 10);
    {
        let e = w.entities.get_mut(spider).unwrap();
        e.ai.walk_tick = 0;
        e.ai.attack_tick = 0;
        e.ai.walk_speed = 1400;
        e.ai.next_hit_time = 2000;
    }
    w.clock_ms = 2000;
    special_monster_run(&mut w, spider);
    // Spider 无目标 ⇒ 不产仔；给一个目标再跑。
    let target = plain_child(&mut w, 12, 10);
    w.entities.get_mut(spider).unwrap().ai.target_creature = Some(target);
    w.entities.get_mut(spider).unwrap().ai.walk_tick = 0;
    w.entities.get_mut(spider).unwrap().ai.attack_tick = 0;
    w.clock_ms = 2001;
    special_monster_run(&mut w, spider);
    let due = w.self_delay_msgs.iter().map(|m| m.deliver_at_ms).min();
    assert!(due.is_some());
    w.clock_ms = due.unwrap();
    let mut sp = MockSpawner::default();
    let outs = w.deliver_self_delay_msgs_with(Some(&mut sp));
    assert_eq!(outs.len(), 1);
    assert!(outs[0].zen_bee.is_none(), "同步支不返回队列请求");
    assert_eq!(sp.calls.len(), 1);
    assert_eq!((sp.calls[0].2, sp.calls[0].3), (10, 11));
    assert_eq!(w.entities.get(spider).unwrap().special_children.len(), 1);
    assert!(w.pending_special_spawns.is_empty());
}

/// Ghost 的产仔者：oracle `ProcessMonsters` 跳过 Ghost ⇒ 自投递永不泵出；本骨架保留在队列。
#[test]
fn ghost_owner_zen_bee_message_stays_queued() {
    let mut w = World::new(32, 32);
    let bee = special_owner(&mut w, RACE_BEE_QUEEN, 10, 10);
    let target = plain_child(&mut w, 12, 10);
    w.entities.get_mut(bee).unwrap().ai.target_creature = Some(target);
    w.clock_ms = 0;
    w.send_self_delay_msg(bee, RM_ZEN_BEE, ZEN_BEE_DELAY_MS);
    w.entities.get_mut(bee).unwrap().ghost = true;
    w.clock_ms = 10_000;
    let outs = w.deliver_self_delay_msgs();
    assert!(outs.is_empty());
    assert_eq!(w.self_delay_msgs.len(), 1, "Ghost owner 的消息保留");
    assert!(w.pending_special_spawns.is_empty());
}

/// 世界默认 `Config.Bee/Spider` 与 `[Names]` 覆盖。
#[test]
fn special_names_default_and_override() {
    let mut w = World::new(8, 8);
    let bee = special_owner(&mut w, RACE_BEE_QUEEN, 2, 2);
    let req = special_spawn_request(&w, bee).expect("请求");
    assert_eq!(req.name, "蝙蝠");
    w.set_special_monster_names("蜂", "蛛");
    let req = special_spawn_request(&w, bee).expect("请求");
    assert_eq!(req.name, "蜂");
}

/// `adopt_special_child` 在主人无目标时仍入表（`BeeList.Add` 无条件）。
#[test]
fn adopt_without_target_still_joins_list() {
    let mut w = World::new(8, 8);
    let bee = special_owner(&mut w, RACE_BEE_QUEEN, 2, 2);
    let child = plain_child(&mut w, 2, 3);
    assert!(adopt_special_child(&mut w, bee, child));
    assert_eq!(w.entities.get(bee).unwrap().special_children, vec![child]);
    assert_eq!(w.entities.get(child).unwrap().ai.target_creature, None);
}

fn clone_world(master_mp: u16) -> (World, i32, i32) {
    let mut w = World::new(16, 16);
    let mut m = Entity::new(0, "master".to_string(), 4, 4);
    m.race = ACTOR_RACE_PLAY;
    m.mp = master_mp;
    m.max_mp = 100;
    let master = w.add_entity(m);
    let mut c = Entity::new(0, "clone".to_string(), 5, 4);
    c.race = RACE_CLONE_MONSTER;
    c.master = Some(master);
    c.hp = 100;
    c.max_hp = 100;
    let clone = w.add_entity(c);
    (w, master, clone)
}
