//! **魔号 8/21 效果体的 Rust 活栈 RNG 站点自证**（卡 `openmir2-m3-firewind-spacemove-wire`）。
//!
//! 本文件是**独立测试二进制**（`cargo test -p game-svr --test magic_effect_rng_trace`）——
//! `rng_trace` 的开关是进程级全局（`OnceLock`），只有独占进程才不会被别的用例的取数污染。
//!
//! 判据：开着 `MIR2_RNG_LOG` 同口径的 trace，跑一次
//! [`mir2_world::World::cast_firewind_spell`]（魔号 8）与
//! [`mir2_world::World::cast_space_move_spell`]（魔号 21），落盘里必须出现三处**非攻击**站点：
//! - `crates/world/src/magic_effect.rs:<line>`（`Random(20)`/`Random(2)`/`Random(11)`）；
//! - `crates/world/src/merchant.rs:<line>`（`MapRandomMove` 两抽；C# site 亦归 `merchant` 组）；
//! - `crates/world/src/space_move.rs:<line>`（`SpaceMoveGetRandXY` 移位环）。
//!
//! 证据用法（把真实 trace 留给 `rng_ledger_diff.py groups --require-closed` 复算）：
//! ```text
//! M2_MAGIC_EFFECT_TRACE_PATH=E:/tmp/magic-effect-rust.tsv \
//!   cargo test -p game-svr --test magic_effect_rng_trace -- --nocapture
//! ```
//!
//! **为什么 home 图全墙**：`SpaceMoveGetRandXY` 只在「当前格不可走」时抽数；全走格会让移位环
//! 首查即中、`space_move.rs` 站点整段缺失（trace 无法证明该组）。全墙图 ⇒ 移位环必然进入
//! `else` 分支抽 `Random(Width)`/`Random(Height)`（oracle `BaseObject.cs:1064-1075` 同式）。

use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::entity::{VisibleBaseObject, VisibleFlag};
use mir2_world::{CellAttribute, MapGrid, World};

const SEED: i32 = 1;

#[test]
fn magic_effect_draws_record_non_attack_sites() {
    let from_env = std::env::var("M2_MAGIC_EFFECT_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            std::env::temp_dir().join(format!("gs-magic-effect-rng-{}.tsv", std::process::id()))
        }
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    let mut w = World::new(64, 64);
    w.rng = RandomNumber::with_seed(SEED);
    // Home 图：全墙 ⇒ `SpaceMoveGetRandXY` 必进取数分支。
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
        let mut e = mir2_world::Entity::new(0, "caster".to_string(), 10, 10);
        e.level = 20;
        w.enter_map("caster", e, 0).expect("进图")
    };
    let target = {
        let mut e = mir2_world::Entity::new(0, "target".to_string(), 11, 10);
        e.level = 15;
        w.add_entity(e)
    };
    w.tick();
    w.entities.get_mut(caster).unwrap().visible_actors = vec![VisibleBaseObject {
        target,
        flag: VisibleFlag::Show,
    }];
    // 魔号 8：至少 1 抽（Random(20)），门通过时再 1 抽（Random(2)）。
    let (pushed, _events) = w.cast_firewind_spell(caster, 1);
    println!("FIREWIND_RESULT pushed={pushed}");
    // 魔号 21：Random(11) + MapRandomMove 两抽 + 移位环（全墙 ⇒ 必有）。
    let out = w.cast_space_move_spell("caster", caster, 1, Some(1));
    println!("SPACEMOVE_OUTCOME {out:?}");

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("MAGIC_EFFECT_TRACE_PATH {path:?}");
    let norm = txt.replace('\\', "/");
    for (tag, needle) in [
        ("magic_effect", "crates/world/src/magic_effect.rs:"),
        ("merchant", "crates/world/src/merchant.rs:"),
        ("space_move", "crates/world/src/space_move.rs:"),
    ] {
        let n = norm.matches(needle).count();
        println!("MAGIC_EFFECT_SITE group={tag} count={n}");
        assert!(
            n > 0,
            "trace 缺少 `{needle}` 站点（consumer_groups.tsv 的 {tag} 组会判 UNCLASSIFIED）:\n{txt}"
        );
    }
    // 反牙：效果体站点**不得**落 `magic.rs`（`RUST_ATTACK_RE` 会把整个文件误归 attack 组）。
    assert!(
        !norm.contains("crates/world/src/magic.rs:"),
        "效果体取数不得留在 magic.rs（会被误归 attack 组）:\n{txt}"
    );
    assert!(
        pushed > 0,
        "seed={SEED} 应让火环门通过（否则 trace 无 Random(2)）"
    );
    assert!(out.result, "seed={SEED} 应让 21 的门通过");
}
