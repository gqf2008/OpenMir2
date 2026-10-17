//! **P3-B3 取数站点自证**（魔号 16/18/19/28/32/40）。
//!
//! 判据（合同 §5 与 B2 trace 同口径）：
//! - B3 的新增取数必须落在 `crates/world/src/magic_effect.rs`（`consumer_groups.tsv` 的
//!   `magic_effect` 组）；**不得**出现在 `crates/world/src/magic.rs:`（会被 `RUST_ATTACK_RE`
//!   整文件吞成 `attack` 组）或 `crates/world/src/world.rs:`（世界路径）。
//!
//! ```text
//! M2_P3B3_TRACE_PATH=E:/tmp/p3b3-rust.tsv \
//!   cargo test -p game-svr --test p3b3_rng_trace -- --nocapture
//! ```

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::entity::{ACTOR_RACE_ANIMAL, ACTOR_RACE_PLAY};
use mir2_world::Entity;

const SEED: i32 = 20261017;

fn magic(id: u16) -> MagicInfo {
    MagicInfo {
        magic_id: id,
        power: 0,
        max_power: 0,
        def_power: 0,
        def_max_power: 0,
        train_lv: 3,
        ..MagicInfo::default()
    }
}

fn player(id: i32, name: &str, x: i32, y: i32) -> Entity {
    let mut e = Entity::new(id, name.to_string(), x, y);
    e.race = ACTOR_RACE_PLAY;
    e.level = 60;
    e
}

fn monster(id: i32, name: &str, x: i32, y: i32) -> Entity {
    let mut e = Entity::new(id, name.to_string(), x, y);
    e.race = ACTOR_RACE_ANIMAL;
    e.level = 30;
    e.life_attrib = 1;
    e.ai.animal = true;
    e.ai.meat_quality = 100;
    e
}

#[test]
fn p3b3_draws_land_in_magic_effect_only() {
    let from_env = std::env::var("M2_P3B3_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-p3b3-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    let mut w = mir2_world::World::new(64, 64);
    let caster = w.add_entity(player(0, "caster", 10, 10));
    let friend = w.add_entity(player(0, "friend", 11, 10));
    // 16 的 3×3 目标（3 只合格 Animal）。
    let m1 = w.add_entity(monster(0, "m1", 11, 11));
    let _m2 = w.add_entity(monster(0, "m2", 12, 11));
    let _m3 = w.add_entity(monster(0, "m3", 12, 12));
    // 18 的贴身怪（TargetCret=caster）与远怪（坐标短路）。
    let mut near = monster(0, "near", 11, 10);
    near.ai.target_creature = Some(caster);
    let near = w.add_entity(near);
    let mut far = monster(0, "far", 15, 10);
    far.ai.target_creature = Some(caster);
    let _far = w.add_entity(far);
    // 32 的目标。
    let undead = w.add_entity(monster(0, "undead", 11, 12));
    w.tick();
    w.rng = RandomNumber::with_seed(SEED);

    let m16 = magic(16);
    let m18 = magic(18);
    let m19 = magic(19);
    let m28 = magic(28);
    let m32 = magic(32);
    let _m40 = magic(40);
    let sc = 0x0A05;

    let _ = w.cast_holy_curtain_spell(caster, &m16, 2, sc, 12, 12);
    let _ = w.cast_private_transparent_spell(caster, &m18, 2, sc);
    let _ = w.cast_group_transparent_spell(caster, &m19, 2, sc, 10, 10);
    let _ = w.cast_show_hp_spell(caster, &m28, 2, sc, Some(undead));
    let _ = w.cast_turn_undead_spell(caster, &m32, 2, Some(undead), 50);
    let _ = w.cast_unamyounsul_spell(caster, 2, None);
    // 19 的投递 tick（`RM_TRANSPARENT` 的 `R(2)`）。
    w.clock_ms += 800;
    let _ = w.deliver_self_delay_msgs();
    let _ = (friend, m1, near);

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("P3B3_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");
    assert!(
        norm.contains("crates/world/src/magic_effect.rs:"),
        "B3 取数必须落到 magic_effect.rs:\n{txt}"
    );
    assert!(
        !norm.contains("crates/world/src/magic.rs:"),
        "不得把取数放回 magic.rs（会被整文件吞成 attack）:\n{txt}"
    );
    assert!(
        !norm.contains("crates/world/src/world.rs:"),
        "不得在 world.rs 直接取数（站点组会漂）:\n{txt}"
    );
    for line in norm.lines() {
        if line.is_empty() || line.starts_with("SEED_PROBE") || line.contains("INJECT_OK") {
            continue;
        }
        assert!(
            line.contains("crates/world/src/magic_effect.rs:"),
            "B3 站点只允许 magic_effect.rs: {line}"
        );
    }
}
