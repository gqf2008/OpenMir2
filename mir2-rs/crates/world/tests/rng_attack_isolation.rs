//! **M3②③ 活栈隔离口径（owner 2026-10-12 21:15，§6.0-3 owner 签字）**：
//! 攻击路径专用随机流的路由判据。
//!
//! 背景：在「全服共享随机源」下，oracle 自己都不可复现（并发消费者的噪声序列每次不同）
//! ⇒「同种子 ⇒ 逐 k 相同」**原则不可达成**。owner 裁决把 DoD② 改为「活栈隔离口径」：两侧把
//! **攻击路径的取数**接到各自一条**专用种子流**，其余消费者走原全局流；判据仍是真机
//! `attack_diff.py diff` 逐 k = 0。
//!
//! 本文件钉死 Rust 侧的三件事（缺一条都会让对侧 `attack_diff.py diff` 假绿/假红）：
//! 1. **关闭时**（缺省）攻击路径仍走全局 `rng`，专用流**一次都不被取数**（行为逐字节不变）；
//! 2. **开启时**攻击路径（`attack_once` 的 `attack_dir` 与 `attack_swing`）改走专用流，
//!    全局 `rng` **一次都不被攻击路径取数**；
//! 3. 专用流的**取数序列**（不是集合）与「同一 `attack_seed` 独立驱动 `attack_dir`」逐项相同
//!    —— 这同时钉死「站点/次序/次数」与专用流种子的两侧同口径。
//!
//! **改坏必红**：把 `World::attack_rng` 改成恒定 `&mut self.rng`（= 不隔离）⇒ 本文件
//! [`isolation_on_routes_attacks_to_dedicated_stream`] 与
//! [`attack_swing_is_routed_to_dedicated_stream`] 的「全局流未被取数」必红；
//! 把 `attack_seed` 改成恒等（`base_seed`）⇒ `mir2_shared::rng` 的
//! `attack_stream_is_a_distinct_reproducible_sequence` 必红。

use mir2_shared::rng::{attack_seed, RandomNumber};
use mir2_world::combat::{attack_dir, swing_power, AttackPowerMode, CombatConfig};
use mir2_world::{Entity, World, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};

const SEED: i32 = 20261012;
const DC: i32 = 0x1E14;

/// 取一个随机流接下来的 `n` 个值；`RandomNumber` 是 `Clone`，所以不动原流。
fn next_vals(mut rng: RandomNumber, n: usize) -> Vec<i32> {
    (0..n).map(|_| rng.random()).collect()
}

/// `attack_dir` 的独立对照流（与 `World` 内部同口径：玩家攻方 + 怪物目标）。
fn reference_attack_stream(seed: i32, rounds: usize) -> RandomNumber {
    let cfg = CombatConfig::default();
    let mut rng = RandomNumber::with_seed(seed);
    let mut a = player_attacker();
    let mut t = monster_target();
    for _ in 0..rounds {
        let _ = attack_dir(&mut rng, &mut a, &mut t, None, &cfg);
    }
    rng
}

fn player_attacker() -> Entity {
    let mut a = Entity::new(1, "hero", 10, 10);
    a.race = ACTOR_RACE_PLAY;
    a.dc = DC;
    a.hit_point = 20;
    a.luck = 0;
    a.power_rate = 100;
    a.hp = 400;
    a.max_hp = 400;
    a
}

fn monster_target() -> Entity {
    let mut t = Entity::new(2, "mon", 11, 10);
    t.race = ACTOR_RACE_MONSTER;
    t.ac = 0;
    t.hp = 60000;
    t.max_hp = 60000;
    t.hit_point = 0; // 不掷闪避，聚焦「取数是否走对流」
    t
}

/// 建一个「玩家攻方 + 怪物目标」都已进图的世界（与既有战斗用例同口径）。
fn setup_world() -> (World, i32, i32) {
    let mut world = World::new(32, 32);
    world.open_session("hero");
    let hero_id = world
        .enter_map("hero", player_attacker(), 0)
        .expect("玩家进图");
    let mon_id = world.add_entity(monster_target());
    world.tick(); // 处理 Enter 落图
    (world, hero_id, mon_id)
}

/// 关闭时（缺省）：攻击路径照旧走全局 `rng`，专用流**未被取数**。
#[test]
fn isolation_off_routes_attacks_to_global_stream() {
    let (mut world, hero_id, mon_id) = setup_world();
    assert!(
        !world.attack_isolation_enabled(),
        "缺省必须关闭隔离（生产/既有 golden 路径逐字节不变）"
    );
    let aseed = attack_seed(SEED);
    world.rng = RandomNumber::with_seed(SEED);
    world.rng_attack = RandomNumber::with_seed(aseed);

    world.attack_cmd(hero_id, mon_id);
    world.attack_cmd(hero_id, mon_id);
    let rec = world.tick();
    assert_eq!(rec.attacks.len(), 2, "两次攻击都应结算");

    // 全局流推进 = 独立 `attack_dir` 两条；专用流原封未动。
    let replay_global = next_vals(reference_attack_stream(SEED, 2), 8);
    assert_eq!(
        next_vals(world.rng.clone(), 8),
        replay_global,
        "关闭时攻击路径必须逐次消费全局 rng（行为逐字节不变）"
    );
    assert_eq!(
        next_vals(world.rng_attack.clone(), 8),
        next_vals(RandomNumber::with_seed(aseed), 8),
        "关闭时专用流一次都不许被取数"
    );
}

/// 开启时：攻击路径改走专用流；全局 `rng` **未被攻击路径取数**（不可被识别为「碰巧相同」）。
#[test]
fn isolation_on_routes_attacks_to_dedicated_stream() {
    let (mut world, hero_id, mon_id) = setup_world();
    let aseed = attack_seed(SEED);
    world.rng = RandomNumber::with_seed(SEED);
    world.enable_attack_isolation(aseed);
    assert!(world.attack_isolation_enabled());

    world.attack_cmd(hero_id, mon_id);
    world.attack_cmd(hero_id, mon_id);
    let rec = world.tick();
    assert_eq!(rec.attacks.len(), 2);

    // 专用流推进 = 独立 `attack_dir` 两条（逐项，不只是次数）。
    let replay_attack = next_vals(reference_attack_stream(aseed, 2), 8);
    assert_eq!(
        next_vals(world.rng_attack.clone(), 8),
        replay_attack,
        "开启时攻击路径必须逐次消费专用流，且次序/次数与独立 attack_dir 逐项一致"
    );
    // 全局流原封未动 ⇒ 「逐 k=0」只可能来自专用流，不可能来自噪声流。
    assert_eq!(
        next_vals(world.rng.clone(), 8),
        next_vals(RandomNumber::with_seed(SEED), 8),
        "开启时全局 rng 不应被攻击路径消费（否则隔离口径没有仲裁力）"
    );
}

/// 无目标挥砍（`World::attack_swing`）同样走专用流。
#[test]
fn attack_swing_is_routed_to_dedicated_stream() {
    let (mut world, hero_id, _mon_id) = setup_world();
    let aseed = attack_seed(SEED);
    world.rng = RandomNumber::with_seed(SEED);
    world.enable_attack_isolation(aseed);

    let mode = AttackPowerMode::Player {
        luck: 0,
        power_rate: 100,
        bo_power_item: false,
        power_item: 0,
    };
    let s1 = world.attack_swing(hero_id).expect("挥砍 1");
    let s2 = world.attack_swing(hero_id).expect("挥砍 2");

    let mut replay = RandomNumber::with_seed(aseed);
    let r1 = swing_power(&mut replay, DC, mode);
    let r2 = swing_power(&mut replay, DC, mode);
    assert_eq!([s1, s2], [r1, r2], "attack_swing 必须与专用流口径逐值相同");
    assert_eq!(
        next_vals(world.rng_attack.clone(), 8),
        next_vals(replay.clone(), 8),
        "attack_swing 的取数必须落在专用流"
    );
    assert_eq!(
        next_vals(world.rng.clone(), 8),
        next_vals(RandomNumber::with_seed(SEED), 8),
        "attack_swing 不应消费全局 rng"
    );
}

/// 隔离开启后同种子两次 ⇒ 专用流与攻击结果**逐项可复现**（真机两相比对的前提）。
#[test]
fn isolation_on_is_reproducible_across_worlds() {
    let aseed = attack_seed(SEED);
    let run = || {
        let (mut world, hero_id, mon_id) = setup_world();
        world.rng = RandomNumber::with_seed(SEED);
        world.enable_attack_isolation(aseed);
        for _ in 0..3 {
            world.attack_cmd(hero_id, mon_id);
        }
        let rec = world.tick();
        let dmg: Vec<u16> = rec.attacks.iter().map(|(_, _, o)| o.damage).collect();
        (dmg, next_vals(world.rng_attack.clone(), 8))
    };
    let (dmg1, rng1) = run();
    let (dmg2, rng2) = run();
    assert_eq!(dmg1, dmg2, "同种子两次伤害序列必须逐项相同");
    assert_eq!(rng1, rng2, "同种子两次专用流序列必须逐项相同");
}

/// 专用流种子公式（两侧同口径）确实被 `World` 采用：显式种子 = `attack_seed(base)`。
#[test]
fn dedicated_stream_seed_matches_two_sided_formula() {
    let (mut world, _hero_id, _mon_id) = setup_world();
    world.enable_attack_isolation(attack_seed(SEED));
    assert_eq!(
        next_vals(world.rng_attack.clone(), 4),
        next_vals(RandomNumber::with_seed(attack_seed(SEED)), 4),
        "专用流必须用 attack_seed(base)=base+0x9E37 播种（两侧同公式）"
    );
}
