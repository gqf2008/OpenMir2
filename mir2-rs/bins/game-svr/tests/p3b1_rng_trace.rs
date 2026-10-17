//! **P3-B1 取数站点自证**（魔号 1/5/11/13/44/45）。
//!
//! 本片新增的效果体取数（抗魔骰 / MPow / GetPower / 推挤）全部落在
//! `crates/world/src/magic_effect.rs`（`consumer_groups.tsv` 的 `magic_effect` 组）。
//! `GetAttackPower` 的抽数仍落 `crates/world/src/combat.rs`（两侧按攻击组登记）。
//!
//! 判据：trace 中**不得**出现 `crates/world/src/magic.rs:`（RUST_ATTACK_RE 会把整文件
//! 吞成 attack 组）；`magic_effect.rs` 必须真的出现（否则说明效果体没接上）。
//!
//! ```text
//! M2_P3B1_TRACE_PATH=E:/tmp/p3b1-rust.tsv \
//!   cargo test -p game-svr --test p3b1_rng_trace -- --nocapture
//! ```

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::entity::{ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};
use mir2_world::Entity;

const SEED: i32 = 20261017;

#[test]
fn p3b1_draws_land_in_magic_effect_not_magic_rs() {
    let from_env = std::env::var("M2_P3B1_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-p3b1-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    let mut w = mir2_world::World::new(64, 64);
    let mut c = Entity::new(0, "caster".to_string(), 10, 10);
    c.race = ACTOR_RACE_PLAY;
    c.level = 5;
    let caster = w.add_entity(c);
    let mut t = Entity::new(0, "mob".to_string(), 11, 10);
    t.race = ACTOR_RACE_MONSTER;
    t.level = 1;
    t.life_attrib = 1; // LA_UNDEAD
    let target = w.add_entity(t);
    w.tick();
    w.rng = RandomNumber::with_seed(SEED);

    let magic = MagicInfo {
        power: 5,
        max_power: 10,
        def_power: 2,
        def_max_power: 8,
        train_lv: 3,
        ..MagicInfo::default()
    };

    // 1 / 5（MC 口径；anti_magic=0 ⇒ 骰门必过）
    for _ in 0..2 {
        let _ = w.cast_fireball_spell(caster, &magic, 1, Some(target), 11, 10, 0, 0);
    }
    // 11 / 45（MC 口径 + 不死 ×1.5）
    for _ in 0..2 {
        let _ = w.cast_lightening_spell(caster, &magic, 1, Some(target), 0, 0);
    }
    // 13（SC 口径）
    let _ = w.cast_firecharm_spell(caster, &magic, 1, Some(target), 11, 10, 0, 0);
    // 44（MC 口径 + 推挤两抽；caster 5 级 > target 1 级）
    let _ = w.cast_hb_fireball_spell(caster, &magic, 1, Some(target), 11, 10, 0, 0);

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("P3B1_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");
    assert!(
        norm.contains("crates/world/src/magic_effect.rs:"),
        "B1 取数必须落到 magic_effect.rs:\n{txt}"
    );
    assert!(
        !norm.contains("crates/world/src/magic.rs:"),
        "不得把取数放回 magic.rs（会被整文件吞成 attack）:\n{txt}"
    );
    for line in norm.lines() {
        if line.is_empty() || line.starts_with("SEED_PROBE") || line.contains("INJECT_OK") {
            continue;
        }
        assert!(
            line.contains("crates/world/src/magic_effect.rs:")
                || line.contains("crates/world/src/combat.rs:"),
            "B1 站点只允许 magic_effect.rs（效果取数）或 combat.rs（GetAttackPower 攻击组）: {line}"
        );
    }
}
