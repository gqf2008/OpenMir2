//! T-141（卡 `hit-site`，tier=L）的**真 trace** 自证：走生产入口
//! [`World::attack_cmd`] → `World::attack_once` → `combat::attack_dir_grouped_poison`（Player 模式），
//! 断言流水里**每一条**取数都落在 `crates/world/src/combat.rs` 的**实际活栈行**，且站点集合
//! 恰为攻击链的六处（`atk_power_a` / `atk_power_b` / `hit_gate` / `hit_struck_damage` /
//! `attack_success` 的毒门与 `Random(5)`）——**没有被算成 noise 的取数**。
//!
//! 判据分两半（合起来才闭环）：
//! 1. 本测试（与 `MIR2_RNG_LOG` 同形的流水）：行号从 `crates/world/src/combat.rs` **现读**
//!    （不写死 1014/1150/1258），断言「落盘站点 == 源码当前活栈行」。注意本半是**自洽**判据：
//!    源码搬家后期望值同步重算，仍会绿；它咬的是「trace 打点位置/调用点归属」而不是分类表。
//! 2. `mir2-rs/tests/parity/m3/attack_rng_ledger.py selftest` 与
//!    `rng-trace-probe/rng_ledger_diff.py selftest`：同样**现读**源码行，断言该行命中
//!    `RUST_ATTACK_RE` / `CANON_RULES`。**表漏行或代码搬家未同步 ⇒ 那两条必红**（这才是
//!    「搬家变红」的那一半）。
//!
//! 独立测试二进制（`rng_trace` 的开关是进程级全局）：本文件**只有一个** `#[test]`，
//! 避免同进程并行用例污染流水。
//!
//! 证据用法（把真实 trace 留给 Python 账本复算）：
//! ```text
//! M2_HIT_DICE_TRACE_PATH=E:/tmp/hit-dice-rust.tsv \
//!   cargo test -p mir2-world --test hit_dice_rng_trace -- --nocapture
//! python -I mir2-rs/tests/parity/m3/attack_rng_ledger.py summary E:/tmp/hit-dice-rust.tsv --side rust
//! ```

use std::path::PathBuf;

use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::combat::AttackPoisonState;
use mir2_world::{Entity, World, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};

const SEED: i32 = 42;

/// 攻击链六处取数的**源码文本锚**（与 `crates/world/src/combat.rs` 逐字一致；行号现读）。
/// 第三列 = `rng_ledger_diff.py` 的 canonical 名（分类表口径，供人读）。
/// `result = base_power + rng.random_below(power + 1);` 在 `get_base_attack_power_player` 里
/// 出现两次（`Luck > 0` 的 else 支与 `Luck <= 0` 的 else 支，`combat.rs` 106/109 两行逐字相同）；
/// 生产用例走 `Luck <= 0` 支 ⇒ 用**下一行**的 oracle 注释 `if (Luck <= 0)` 唯一定位。
const ATK_POWER_A_NEIGHBOR: &str = "`if (Luck <= 0) if (Random(10 - _MAX(0, -Luck)) == 0)";

/// (源码文本锚, 唯一的下一行锚（None = 本行已唯一）, canonical)
const ATTACK_SITES: [(&str, Option<&str>, &str); 6] = [
    (
        "result = base_power + rng.random_below(power + 1);",
        Some(ATK_POWER_A_NEIGHBOR),
        "atk_power_a",
    ),
    (
        "if luck <= 0 && rng.random_below(10 - hutil::max_i32(0, -luck)) == 0 {",
        None,
        "atk_power_b",
    ),
    ("rng.random_byte(target.speed_point)", None, "hit_gate"),
    (
        "let n_rnd = ac_lo + rng.random_range(0, (ac_hi - ac_lo).abs() + 1);",
        None,
        "hit_struck_damage",
    ),
    (
        "rng.random_below(i32::from(p.anti_poison) + config.attack_poison_rate)",
        None,
        "attack_success(poison gate)",
    ),
    ("(rng.random_below(5) + 2) as u16", None, "attack_success"),
];

#[test]
fn attack_path_draws_land_on_the_source_live_lines() {
    // 给了 `M2_HIT_DICE_TRACE_PATH` ⇒ 保留流水作证据；缺省落临时目录并跑完删除。
    let from_env = std::env::var("M2_HIT_DICE_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-hit-dice-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);

    // 生产入口：玩家攻方（`AttackPowerMode::Player` ⇒ 完整 `GetAttackPower` override +
    // `_Attack` + `AttackSuccess`）+ 怪物守方（`PlayObject.StruckDamage` 的耐久段
    // `struck_damage_durability` 只对玩家目标抽数 ⇒ 本用例的攻击链恰为六处，与
    // `attack_rng_ledger.py segment` 的逐击形状一一对应）。攻方持麻痹态且目标 `AntiPoison=0`、
    // `AttackPosionRate=1` ⇒ 毒门两项都过、`Random(1)` 必抽（且必为 0）⇒ 毒门站点必落。
    let mut w = World::new(32, 32);
    w.rng = RandomNumber::with_seed(SEED);
    w.enable_buff_tick();
    w.combat_config.attack_poison_rate = 1;
    w.combat_config.attack_poison_time = 5;

    let mut attacker = Entity::new(1, "hero".to_string(), 10, 10);
    attacker.race = ACTOR_RACE_PLAY;
    attacker.dc = 10; // `Random(power+1)` 与 `Random(10)` 两抽都走
    attacker.hit_point = 100; // 命中门恒过（`random_byte(10) < 100`）
    let mut target = Entity::new(2, "foe".to_string(), 11, 10);
    target.race = ACTOR_RACE_MONSTER;
    target.hp = 100;
    target.max_hp = 100;
    target.hit_point = 10; // 非 0 ⇒ 抽 `RandomByte(SpeedPoint)`
    target.speed_point = 10;
    target.ac = 0; // `GetHitStruckDamage` 抽 `Random(1)` 恰一次
    let attacker_id = w.add_entity(attacker);
    let target_id = w.add_entity(target);
    w.set_attack_poison(
        attacker_id,
        AttackPoisonState {
            paralysis: true,
            un_paralysis: false,
            anti_poison: 0,
        },
    );

    // 先把 `Enter` 命令消化掉（World 构造/落图不取数），再开 trace 做攻击那一 tick。
    w.tick();
    rng_trace::enable(&path, SEED).expect("打开 rng trace");
    w.attack_cmd(attacker_id, target_id);
    let rec = w.tick();
    assert_eq!(rec.attacks.len(), 1, "本 tick 应恰有一次攻击结算: {rec:?}");
    let (_, _, outcome) = &rec.attacks[0];
    assert!(outcome.hit, "命中门必须通过（hit=true）");
    assert!(outcome.damage > 0, "必须走伤害结算（damage>0）");

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("HIT_DICE_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");

    // 源码活栈行：从 combat.rs 现读（不写死行号）。
    let combat_src =
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/combat.rs"))
            .expect("读 crates/world/src/combat.rs");
    let src_lines: Vec<&str> = combat_src.lines().collect();
    let mut expected: Vec<usize> = Vec::new();
    for (needle, next_needle, _canon) in ATTACK_SITES {
        let lines: Vec<usize> = src_lines
            .iter()
            .enumerate()
            .filter(|(i, ln)| {
                ln.contains(needle)
                    && next_needle
                        .is_none_or(|n| src_lines.get(i + 1).is_some_and(|l| l.contains(n)))
            })
            .map(|(i, _)| i + 1)
            .collect();
        assert_eq!(
            lines.len(),
            1,
            "combat.rs 中 `{needle}` 应恰有 1 处（实际行号 {lines:?}）"
        );
        expected.push(lines[0]);
    }
    expected.sort_unstable();

    // 流水里每条数据行的站点：必须全在 combat.rs，且行号多重集恰为上述六处。
    let mut actual: Vec<usize> = Vec::new();
    let mut non_combat: Vec<&str> = Vec::new();
    for line in norm.lines() {
        if line.is_empty() || line.starts_with("SEED_PROBE") || line.contains("INJECT_OK") {
            continue;
        }
        let site = line.split('\t').nth(4).unwrap_or("");
        if site.is_empty() {
            continue;
        }
        match site.strip_prefix("crates/world/src/combat.rs:") {
            Some(n) => actual.push(n.parse().expect("站点行号")),
            None => non_combat.push(site),
        }
    }
    actual.sort_unstable();
    assert!(
        non_combat.is_empty(),
        "攻击路径不得有 `combat.rs` 之外的取数（会被 Python 账本算成 noise）: {non_combat:?}\n流水:\n{txt}"
    );
    assert_eq!(
        actual, expected,
        "攻击路径站点应恰为源码活栈六处 {expected:?}，实际 {actual:?}。\n流水:\n{txt}"
    );

    // `1018` 从来不是命中骰站点：它是 `e7d432189`（magic-draws-move）时代
    // `attack_success_weapon_damage` 的 `Random(5)` 行；现行活栈已漂到 1150（见上）。
    assert!(
        !norm.contains("crates/world/src/combat.rs:1018"),
        "当前活栈不应再落 `attack_success_weapon_damage` 的旧行号 1018:\n{txt}"
    );
}
