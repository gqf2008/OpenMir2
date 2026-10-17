//! **P3-B4 取数站点自证**（魔号 6/48/50/42）。
//!
//! 判据（合同 §5 与 B2/B3 trace 同口径）：
//! - 毒族/群体的**魔号取数**必须落在 `crates/world/src/magic_effect.rs`
//!   （`consumer_groups.tsv` 的 `magic_effect` 组）；
//! - 共享的 `GetMagStruckDamage`（投递者侧 + 目标侧）落 `crates/world/src/combat.rs`
//!   （与 B2 同口径的 attack 组——合同 §5 指定「MSD 唯一取数点」）；
//! - trace 中**不得**出现 `crates/world/src/magic.rs:`（`RUST_ATTACK_RE` 会把整文件吞成
//!   `attack` 组），也不得出现 `crates/world/src/world.rs:`（世界路径）。
//!
//! ```text
//! M2_P3B4_TRACE_PATH=E:/tmp/p3b4-rust.tsv \
//!   cargo test -p game-svr --test p3b4_rng_trace -- --nocapture
//! ```

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::Entity;

const SEED: i32 = 20261017;

fn magic(id: u16, power: u16, max_power: u16, def_power: u8, def_max_power: u8) -> MagicInfo {
    MagicInfo {
        magic_id: id,
        power,
        max_power,
        def_power,
        def_max_power,
        train_lv: 3,
        ..MagicInfo::default()
    }
}

fn player(id: i32, name: &str, x: i32, y: i32) -> Entity {
    let mut e = Entity::new(id, name.to_string(), x, y);
    e.race = 0;
    e.level = 60;
    e
}

fn monster(id: i32, name: &str, x: i32, y: i32, level: u8) -> Entity {
    let mut e = Entity::new(id, name.to_string(), x, y);
    e.race = 80;
    e.level = level;
    e.hp = 100;
    e.max_hp = 100;
    e
}

#[test]
fn p3b4_draws_land_in_magic_effect_and_combat_only() {
    let from_env = std::env::var("M2_P3B4_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-p3b4-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    let mut w = mir2_world::World::new(64, 64);
    let caster = w.add_entity(player(0, "caster", 10, 10));
    let _obs = w.add_entity(player(0, "obs", 18, 10));
    let t6 = w.add_entity(monster(0, "t6", 11, 10, 10));
    let a1 = w.add_entity(monster(0, "a1", 11, 11, 10));
    let _a2 = w.add_entity(monster(0, "a2", 12, 11, 10));
    let _a3 = w.add_entity(monster(0, "a3", 12, 12, 10));
    let t50 = w.add_entity(monster(0, "t50", 11, 12, 10));
    let g1 = w.add_entity(monster(0, "g1", 11, 13, 10));
    let _g2 = w.add_entity(monster(0, "g2", 12, 13, 10));
    w.tick();
    w.rng = RandomNumber::with_seed(SEED);

    let m6 = magic(6, 0, 0, 0, 0);
    let m48 = magic(48, 1, 1, 0, 0);
    let m50 = magic(50, 1, 1, 0, 0);
    let m42 = magic(42, 0, 0, 0, 0);
    let cfg = mir2_world::magic_effect::MabMabeConfig::default();

    let _ = w.cast_poison_on_target(caster, &m6, 2, t6, 0x0A05, 1, &cfg);
    let _ = w.cast_group_poison_spell(caster, &m48, 2, 11, 11, 0x0A05, &cfg, |_| Some(1));
    let _ = w.cast_mab_mabe_spell(caster, &m50, 2, Some(t50), 11, 12, 0x0A05, &cfg);
    let _ = w.cast_group_lightening_spell(caster, &m42, 2, g1, 11, 13, 0x0A05);
    // 投递 tick：RM_POISON（零抽）+ RM_DELAYMAGIC（1 抽 MSD）+ RM_MAGSTRUCK（R(1000)+MSD）。
    w.clock_ms += 1000;
    let _ = w.deliver_self_delay_msgs();
    let _ = w.deliver_mag_struck();
    let _ = (a1,);

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("P3B4_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");
    assert!(
        norm.contains("crates/world/src/magic_effect.rs:"),
        "B4 魔号取数必须落到 magic_effect.rs:\n{txt}"
    );
    assert!(
        norm.contains("crates/world/src/combat.rs:"),
        "MSD 必须落到 combat.rs:\n{txt}"
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
            line.contains("crates/world/src/magic_effect.rs:")
                || line.contains("crates/world/src/combat.rs:"),
            "B4 站点只允许 magic_effect.rs / combat.rs: {line}"
        );
    }
}
