//! T-141（卡 `hit-site`，tier=L）的**真 trace** 自证：普通怪 `_Attack` 的命中骰
//! （`random_byte(target.speed_point)`）在 Rust 活栈落 `crates/world/src/combat.rs:<live>`，
//! 必须与 Python 分类表 `RUST_ATTACK_RE` 的登记行一致（否则 `attack_rng_ledger.py` /
//! `rng_ledger_diff.py` 会把它算成 noise/未覆盖）。
//!
//! 判据分两半（合起来才闭环）：
//! 1. 本测试（与 `MIR2_RNG_LOG` 同形的流水）：走**生产入口** `attack_with_grouped`，落盘站点
//!    必须**恰有一条** = `crates/world/src/combat.rs:<源码里 random_byte(target.speed_point) 的行>`。
//!    行号从 `src/combat.rs` **现读**（不写死 1014/1018）⇒ 命中骰搬家时本条直接点名「期望 vs 实际」。
//! 2. `mir2-rs/tests/parity/m3/attack_rng_ledger.py selftest` 与
//!    `rng-trace-probe/rng_ledger_diff.py selftest`：同一条源码行必须命中 `RUST_ATTACK_RE`
//!    （归 `attack`；canonical = `hit_gate`）。表漏行 ⇒ 那两条红。
//!
//! 独立测试二进制（`rng_trace` 的开关是进程级全局）：本文件**只有一个** `#[test]`，
//! 避免同进程并行用例污染流水。
//!
//! 证据用法（把真实 trace 留给 Python 账本复算）：
//! ```text
//! M2_HIT_DICE_TRACE_PATH=E:/tmp/hit-dice-rust.tsv \
//!   cargo test -p mir2-world --test hit_dice_rng_trace -- --nocapture
//! ```

use std::path::PathBuf;

use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::{
    attack_with_grouped, AttackPowerMode, CombatConfig, Entity, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY,
};

const SEED: i32 = 42;
/// 调用点唯一标识（与 `combat.rs` 里 `_Attack` 命中骰的源码文本逐字一致）。
const HIT_DICE_NEEDLE: &str = "rng.random_byte(target.speed_point)";

#[test]
fn attack_type_hit_dice_lands_on_the_source_live_line() {
    // 给了 `M2_HIT_DICE_TRACE_PATH` ⇒ 保留流水作证据；缺省落临时目录并跑完删除。
    let from_env = std::env::var("M2_HIT_DICE_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-hit-dice-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    // 单一世界随机流（同种子 ⇒ 取数次序可复算）。
    let mut rng = RandomNumber::with_seed(SEED);
    let cfg = CombatConfig::default();
    // 怪物攻方走基版 `IsAttackTarget`（不依赖组队表）；目标是玩家 ⇒ proper target。
    let mut attacker = Entity::new(1, "a".to_string(), 0, 0);
    attacker.race = ACTOR_RACE_MONSTER;
    attacker.dc = 0x0A0A; // Lo=Hi=10 ⇒ 攻击力恒 10，必进伤害结算
    attacker.hit_point = 100;
    let mut target = Entity::new(2, "t".to_string(), 0, 0);
    target.race = ACTOR_RACE_PLAY;
    target.hp = 100;
    target.max_hp = 100;
    target.hit_point = 10; // 准确度非 0 ⇒ 命中骰 `RandomByte(SpeedPoint)` 必抽
    target.speed_point = 10;
    target.ac = 0;

    let out = attack_with_grouped(
        &mut rng,
        &mut attacker,
        &mut target,
        None,
        &cfg,
        AttackPowerMode::Base,
        &[],
    );
    // `random_byte(10) ∈ [0,10)`，`100 < v` 恒假 ⇒ 命中骰必走且 hit=true（不是靠种子碰运气）。
    assert!(out.hit, "命中骰必须被抽且通过（hit=true）");
    assert!(out.damage > 0, "必须走伤害结算（damage>0）");

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("HIT_DICE_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");

    // 源码活栈行：从 combat.rs 现读（不写死 1014/1018）。
    let combat_src =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/combat.rs"))
            .expect("读 crates/world/src/combat.rs");
    let live_lines: Vec<usize> = combat_src
        .lines()
        .enumerate()
        .filter(|(_, ln)| ln.contains(HIT_DICE_NEEDLE))
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(
        live_lines.len(),
        1,
        "combat.rs 中 `{HIT_DICE_NEEDLE}` 应恰有 1 处：{live_lines:?}"
    );
    let expected = format!("crates/world/src/combat.rs:{}", live_lines[0]);
    let hits = norm.matches(&expected).count();
    assert_eq!(
        hits, 1,
        "`_Attack` 命中骰应恰落一条站点 `{expected}`（实际 {hits} 条）。\n流水:\n{txt}"
    );
    assert!(
        !norm.contains("crates/world/src/combat.rs:1018"),
        "当前活栈不应再落旧行号 1018（`6a774ea79` 之前的历史站点）:\n{txt}"
    );
}
