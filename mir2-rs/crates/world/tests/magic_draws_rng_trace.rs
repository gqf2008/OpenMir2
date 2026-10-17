//! 方案 C（卡 `openmir2-magic-draws-move`）的**取数站点**自证。
//!
//! 1. **静态**：`crates/world/src/magic.rs` 不得再含任何取数形态（`random_below` /
//!    `random_byte` / `random_range` / `random()` / `.rng.`）——原 21 抽已按语义
//!    **等价搬出**（攻击语义 → `combat.rs`；魔号效果语义 → `magic_effect.rs`）。
//!    把任一抽数改回 `magic.rs` ⇒ 本测试红。
//! 2. **真 trace**：同种子跑 buff（无极真气 36）/ 群体（`MagGroupDeDing` 39）/
//!    圣言（`MagTurnUndead` 32）/ 诱惑（`MagTamming` 20）/ 魔号 8（抗拒火环）/
//!    魔号 21（瞬息移动），落盘站点必须只出现在 `combat.rs` / `magic_effect.rs` /
//!    `merchant.rs` / `space_move.rs`，且 `magic.rs` 站点数为 **0**。
//!
//! 证据用法（把真实 trace 留给 `rng_ledger_diff.py groups --require-closed` 复算，
//! 含/不含 `--no-attack` 都必须 closed）：
//! ```text
//! M2_MAGIC_DRAWS_TRACE_PATH=E:/tmp/magic-draws-rust.tsv \
//!   cargo test -p mir2-world --test magic_draws_rng_trace -- --nocapture
//! ```
//!
//! 独立测试二进制（`rng_trace` 的开关是进程级全局）：本文件**只有一个** `#[test]`，
//! 避免同进程并行用例污染流水。

use std::path::PathBuf;

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::entity::{VisibleBaseObject, VisibleFlag};
use mir2_world::magic::{
    group_melee_damage, mag_tamming, mag_turn_undead, power_uenhancer, TammingInput,
    TurnUndeadInput, LA_UNDEAD,
};
use mir2_world::{AttackPowerMode, CellAttribute, Entity, MapGrid, World, ACTOR_RACE_PLAY};

const SEED: i32 = 42;

fn remove_if_exists(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

#[test]
fn magic_draws_land_outside_magic_rs() {
    // ---- 1. 静态守卫：magic.rs 不得再含取数形态 ----
    let magic_src =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/magic.rs"))
            .expect("读 crates/world/src/magic.rs");
    for pat in [
        "random_below",
        "random_byte",
        "random_range",
        "random()",
        ".rng.",
    ] {
        assert!(
            !magic_src.contains(pat),
            "magic.rs 不得再含取数形态 `{pat}`（方案 C：取数须在 combat.rs / magic_effect.rs）"
        );
    }

    // ---- 2. 真 trace ----
    let from_env = std::env::var("M2_MAGIC_DRAWS_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-magic-draws-rng-{}.tsv", std::process::id())),
    };
    remove_if_exists(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    // 单一世界随机流：下面所有函数调用共用（同种子 ⇒ 取数次序可复算）。
    let mut rng = RandomNumber::with_seed(SEED);

    // buff / 无极真气（36）：`Random(Level)`（combat.rs）+ GetPower13/GetAttackPower。
    let magic = MagicInfo {
        magic_id: 36,
        power: 1,
        max_power: 5,
        train_lv: 3,
        def_power: 0,
        def_max_power: 10,
        ..Default::default()
    };
    let (buff_power, _buff_time) = power_uenhancer(&mut rng, &magic, 3, 500, AttackPowerMode::Base);
    assert!(buff_power >= 4, "Level+1 起步（{buff_power}）");

    // 群体（39 MagGroupDeDing）：GetAttackPower + 无条件 `Random(SpeedPoint)`（combat.rs）。
    let mut attacker = Entity::new(1, "caster".to_string(), 0, 0);
    attacker.race = ACTOR_RACE_PLAY;
    attacker.dc = 0x0503;
    attacker.hit_point = 5;
    let mut target = Entity::new(2, "target".to_string(), 1, 0);
    target.speed_point = 10;
    let _ = group_melee_damage(&mut rng, &attacker, &target, AttackPowerMode::Base);

    // 圣言（32 MagTurnUndead）：Random(300)（AnimalObject.Struck 的 Animal 支）+ Random(2) + Random(100)。
    let turn = mag_turn_undead(
        &mut rng,
        &TurnUndeadInput {
            super_man: false,
            life_attrib: LA_UNDEAD,
            animal: true,
            caster_level: 50,
            target_level: 1,
            magic_level: 3,
            mag_turn_undead_level: 100,
            // 探针口径（旧向量）：进入 `Struck` 时 TargetCret == null；施法者是 proper target。
            target_creature_present: false,
            attack_dir_ok: false,
            caster_is_proper_target: true,
            target_is_animal_object: true,
        },
    );
    assert!(turn.target_targeted_by_caster, "圣言命中路径");
    assert!(turn.target_hp_zeroed && turn.result, "高等级施法者应即死");

    // 诱惑（20 MagTamming）：magic_level=3 ⇒ `Random(4-3)=Random(1)` 恒 0，必进分支。
    let tame = mag_tamming(
        &mut rng,
        &TammingInput {
            magic_level: 3,
            caster_level: 20,
            target_race: ACTOR_RACE_PLAY + 1,
            target_level: 1,
            target_abil_max_hp: 100,
            target_life_attrib: 0,
            target_no_tame: false,
            target_master_is_caster: true,
            target_master_present: false,
            target_hp: 100,
            walk_speed: 1000,
            next_hit_time: 1000,
            caster_slave_count: 0,
            cfg_target_level: 1,
            cfg_tamming_level: 50,
            cfg_tamming_count: 5,
            cfg_hp_rate: 100,
        },
    );
    assert!(tame.result, "已是施法者下属 ⇒ 直接成功");
    // 再跑几轮「非下属」路径，覆盖 Random(3)/Random(caster.Lv*2)/Random(14)/Random(20) 等站点。
    for n in 0..16 {
        let inp = TammingInput {
            magic_level: (n % 3) as u8 + 1,
            caster_level: 30,
            target_race: ACTOR_RACE_PLAY + 1,
            target_level: 1,
            target_abil_max_hp: 100 + n,
            target_life_attrib: if n % 2 == 0 { LA_UNDEAD } else { 0 },
            target_no_tame: false,
            target_master_is_caster: false,
            target_master_present: false,
            target_hp: 500,
            walk_speed: 2000,
            next_hit_time: 2000,
            caster_slave_count: 0,
            cfg_target_level: 1,
            cfg_tamming_level: 50,
            cfg_tamming_count: 5,
            cfg_hp_rate: 100,
        };
        let _ = mag_tamming(&mut rng, &inp);
    }

    // 魔号 8（抗拒火环）+ 魔号 21（瞬息移动）：走生产入口（world 层）。
    let mut w = World::new(64, 64);
    w.rng = RandomNumber::with_seed(SEED);
    // Home 图全墙 ⇒ `SpaceMoveGetRandXY` 必进取数分支（与既有 magic_effect 追踪用例同口径）。
    let mut home = MapGrid::new(64, 64);
    for x in 0..64 {
        for y in 0..64 {
            home.get_cell_info_mut(x, y)
                .0
                .set_attribute(CellAttribute::HighWall);
        }
    }
    w.maps.push(home);
    w.open_session("caster");
    let caster = {
        let mut e = Entity::new(0, "caster".to_string(), 10, 10);
        e.level = 20;
        w.enter_map("caster", e, 0).expect("进图")
    };
    let visible_target = {
        let mut e = Entity::new(0, "target".to_string(), 11, 10);
        e.level = 15;
        w.add_entity(e)
    };
    w.tick();
    w.entities.get_mut(caster).unwrap().visible_actors = vec![VisibleBaseObject {
        target: visible_target,
        flag: VisibleFlag::Show,
    }];
    let (pushed, _events) = w.cast_firewind_spell(caster, 1);
    println!("FIREWIND_RESULT pushed={pushed}");
    let out = w.cast_space_move_spell("caster", caster, 1, Some(1));
    println!("SPACEMOVE_OUTCOME {out:?}");

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        remove_if_exists(&path);
    }
    println!("MAGIC_DRAWS_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");

    // 站点归属：magic.rs 为 0；迁出的模块必须真的出现。
    for (tag, needle) in [
        ("magic_effect", "crates/world/src/magic_effect.rs:"),
        ("combat", "crates/world/src/combat.rs:"),
        ("merchant", "crates/world/src/merchant.rs:"),
        ("space_move", "crates/world/src/space_move.rs:"),
    ] {
        let n = norm.matches(needle).count();
        println!("MAGIC_DRAWS_SITE group={tag} count={n}");
        assert!(n > 0, "trace 缺少 `{needle}` 站点:\n{txt}");
    }
    assert!(
        !norm.contains("crates/world/src/magic.rs:"),
        "取数不得留在 magic.rs（方案 C 后应为 0）:\n{txt}"
    );
    assert!(
        pushed > 0,
        "seed={SEED} 应让火环门通过（否则缺 Random(2) 站点）"
    );
    assert!(out.result, "seed={SEED} 应让 21 的门通过");
}
