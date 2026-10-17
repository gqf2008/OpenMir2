//! **P3-B5 取数站点自证**（魔号 9/10/22/36）。
//!
//! 判据（合同 §6.2 与 B2/B3/B4 trace 同口径）：
//! - 四魔号的**魔号取数**必须落在 `crates/world/src/magic_effect.rs`
//!   （`consumer_groups.tsv` 的 `magic_effect` 组）：`MPow`/`GetPower`/`GetPower13`/`GetRPow`/
//!   `GetAttackPower` 与新的 `power_uenhancer`/`scale_undead_attack`；
//! - 共享的 `GetMagStruckDamage`（投递者侧 + 目标侧）落 `crates/world/src/combat.rs`
//!   （与 B2/B4 同口径的 attack 组——合同 §5 指定「MSD 唯一取数点」）；
//! - trace 中**不得**出现 `crates/world/src/magic.rs:`（`RUST_ATTACK_RE` 会把整文件吞成
//!   `attack` 组），也不得出现 `crates/world/src/world.rs:`（世界路径）。
//!
//! ```text
//! M2_P3B5_TRACE_PATH=E:/tmp/p3b5-rust.tsv \
//!   cargo test -p game-svr --test p3b5_rng_trace -- --nocapture
//! ```

use mir2_data::models::MagicInfo;
use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::Entity;

const SEED: i32 = 20261017;

#[allow(clippy::too_many_arguments)]
fn magic(
    id: u16,
    effect: u8,
    effect_type: u8,
    spell: u16,
    power: u16,
    max_power: u16,
    def_power: u8,
    def_max_power: u8,
) -> MagicInfo {
    MagicInfo {
        magic_id: id,
        effect,
        effect_type,
        spell,
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
    e.hp = 100;
    e.max_hp = 100;
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
fn p3b5_draws_land_in_magic_effect_and_combat_only() {
    let from_env = std::env::var("M2_P3B5_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-p3b5-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    let mut w = mir2_world::World::new(64, 64);
    let caster = w.add_entity(player(0, "caster", 10, 10));
    let obs = w.add_entity(player(0, "obs", 11, 9));
    let _far = w.add_entity(player(0, "far", 40, 40));
    // 9/10 的扫描线 y=10：三个命中候选（M2 死格验证 `!Death` 过滤不抽）。
    let _m1 = w.add_entity(monster(0, "m1", 12, 10, 30));
    let _m2 = w.add_entity(monster(0, "m2", 15, 10, 30));
    let _m3 = w.add_entity(monster(0, "m3", 17, 10, 30));
    // 22 的事件受害者在中心格。
    let _v = w.add_entity(monster(0, "v", 10, 13, 30));
    w.tick();
    w.rng = RandomNumber::with_seed(SEED);

    let m9 = magic(9, 7, 5, 10, 14, 14, 6, 6);
    let m10 = magic(10, 8, 6, 25, 12, 12, 12, 12);
    let m22 = magic(22, 20, 4, 20, 3, 3, 3, 3);
    let m36 = magic(36, 1, 1, 14, 12, 24, 6, 6);

    let _ = w.cast_pass_through_spell(caster, &m9, 2, 14, 10, 0x0A06, 5, false);
    let _ = w.cast_pass_through_spell(caster, &m10, 2, 16, 10, 0x0A06, 8, true);
    let _ = w.cast_fire_wall_spell(caster, &m22, 2, 10, 13, 0x0A06, false);
    let _ = w.cast_uenhancer_spell(caster, &m36, 2, Some(obs), 0x0A06);
    // 投递 tick：RM_MAGSTRUCK 的 `R(1000)` + MSD；22 的事件 MINE 只有 MSD。
    w.clock_ms += 1000;
    let _ = w.deliver_self_delay_msgs();
    let _ = w.deliver_mag_struck();
    w.run_map_events();
    let _ = w.deliver_mag_struck();

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("P3B5_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");
    assert!(
        norm.contains("crates/world/src/magic_effect.rs:"),
        "B5 魔号取数必须落到 magic_effect.rs:\n{txt}"
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
            "B5 站点只允许 magic_effect.rs / combat.rs: {line}"
        );
    }
}
