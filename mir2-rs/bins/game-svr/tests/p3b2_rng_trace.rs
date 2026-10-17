//! **P3-B2 取数站点自证**（魔号 2/29/23/24/33/47）。
//!
//! 判据（合同 §3/§6 与 B1 trace 同口径）：
//! - 新增取数必须落在 `crates/world/src/magic_effect.rs`（`consumer_groups.tsv` 的
//!   `magic_effect` 组）；`MSD`/`STD`/`GetAttackPower` 的抽数落 `crates/world/src/combat.rs`
//!   （attack 组，两侧按攻击组登记）。
//! - trace 中**不得**出现 `crates/world/src/magic.rs:`（`RUST_ATTACK_RE` 会把整文件吞成
//!   `attack` 组），也不得出现 `crates/world/src/world.rs:`（会把交付抽误归世界路径）。
//!
//! ```text
//! M2_P3B2_TRACE_PATH=E:/tmp/p3b2-rust.tsv \
//!   cargo test -p game-svr --test p3b2_rng_trace -- --nocapture
//! ```

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::entity::{ACTOR_RACE_ANIMAL, ACTOR_RACE_PLAY};
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

#[test]
fn p3b2_draws_land_in_magic_effect_and_combat_only() {
    let from_env = std::env::var("M2_P3B2_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-p3b2-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    let mut w = mir2_world::World::new(64, 64);
    let mut c = Entity::new(0, "caster".to_string(), 10, 10);
    c.race = ACTOR_RACE_PLAY;
    c.level = 50;
    let caster = w.add_entity(c);
    // 治疗目标（玩家友军）。
    let mut p = Entity::new(0, "friend".to_string(), 12, 12);
    p.race = ACTOR_RACE_PLAY;
    p.hp = 50;
    p.max_hp = 1000;
    let friend = w.add_entity(p);
    // AoE 目标（怪物，MAC 非 0 ⇒ MSD 有骰；Animal ⇒ R(300)）。
    let mut m = Entity::new(0, "mob".to_string(), 12, 12);
    m.race = ACTOR_RACE_ANIMAL;
    m.level = 30;
    m.hp = 1000;
    m.max_hp = 1000;
    m.mac = 0x0A00;
    m.ai.animal = true;
    let mob = w.add_entity(m);
    w.tick();
    w.rng = RandomNumber::with_seed(SEED);

    let sc = 0x0C04;
    let mc = 0x0F05;
    let m2 = magic(2, 14, 20, 0, 0);
    let m29 = magic(29, 10, 10, 4, 4);
    let m23 = magic(23, 8, 8, 6, 6);
    let m24 = magic(24, 10, 30, 10, 30);
    let m33 = magic(33, 12, 12, 12, 12);
    let m47 = magic(47, 10, 30, 10, 30);

    let _ = w.cast_healing_spell(caster, &m2, 3, Some(friend), 12, 12, sc);
    let _ = w.cast_big_healing_spell(caster, &m29, 3, Some(friend), 12, 12, sc);
    let _ = w.cast_big_explosion_spell(caster, &m23, 3, Some(mob), 12, 12, 1, mc);
    let _ = w.cast_elec_blizzard_spell(caster, &m24, 3, Some(mob), 2, mc);
    let _ = w.cast_big_explosion_spell(caster, &m33, 3, Some(mob), 12, 12, 2, mc);
    let _ = w.cast_big_explosion_spell(caster, &m47, 3, Some(mob), 12, 12, 2, mc);
    // 交付段（R(1000) / MSD / R(300)）。
    let _ = w.deliver_mag_struck();

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("P3B2_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");
    assert!(
        norm.contains("crates/world/src/magic_effect.rs:"),
        "B2 取数必须落到 magic_effect.rs:\n{txt}"
    );
    assert!(
        norm.contains("crates/world/src/combat.rs:"),
        "MSD/STD 必须落到 combat.rs:\n{txt}"
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
            "B2 站点只允许 magic_effect.rs / combat.rs: {line}"
        );
    }
}
