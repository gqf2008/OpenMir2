//! **登录 PlayObject 构造批的 RNG 流水与账本分桶**（卡 `openmir2-player-ctor-rng-order` A3/A4）。
//!
//! 独立测试二进制：`rng_trace` 的开关是进程级全局（`OnceLock`）⇒ 只有独占进程才不会被别的
//! 用例的抽数污染（同 `bins/game-svr/tests/ctor_fold_rng_trace.rs` 的口径）。
//!
//! 机器可判三件事：
//! 1. 登录批 **恰 5 行**：`1500, 2000, 3000, 3000, 999999`，结果的符号化账本
//!    `batch = Login`（startup 批 = `batch = Startup`，同一条流首尾相接）；
//! 2. 前 4 抽的流水 `site` 与 startup **相同**（都落 `crates/world/src/entity.rs:` 的
//!    `BaseAnimalCtorDraws::draw`）⇒ **只靠 site 不能分桶**，必须靠 `CtorFoldLedger::batch`；
//! 3. 第 5 抽（`RandomNo`）的 site 落在 `crates/world/src/ctor_fold.rs:`（登录 fold 点）。

use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::{CtorDrawBatch, CtorFoldPlan, World, CTOR_FOLD_DRAW_KIND};

const SEED: i32 = 20261027;

/// 一行流水（`seq \t kind \t arg \t result \t site`）。
struct Row {
    kind: String,
    arg: i64,
    result: i64,
    site: String,
}

fn parse_rows(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split('\t').collect();
        // 头部两行（`1\tINJECT_OK…` / `SEED_PROBE …`）与非 TSV 标记行跳过。
        if cols.len() < 5 {
            continue;
        }
        let Ok(_seq) = cols[0].parse::<i64>() else {
            continue;
        };
        if cols[1] == "INJECT_OK" {
            continue;
        }
        out.push(Row {
            kind: cols[1].to_string(),
            arg: cols[2].parse().expect("arg 是整数"),
            result: cols[3].parse().expect("result 是整数"),
            site: cols[4].to_string(),
        });
    }
    out
}

fn is_entity_site(site: &str) -> bool {
    site.contains("crates/world/src/entity.rs:") || site.contains("crates\\world\\src\\entity.rs:")
}

fn is_ctor_fold_site(site: &str) -> bool {
    site.contains("crates/world/src/ctor_fold.rs:")
        || site.contains("crates\\world\\src\\ctor_fold.rs:")
}

#[test]
fn login_player_ctor_batch_is_five_draws_and_separately_bucketed() {
    let path =
        std::env::temp_dir().join(format!("parity-player-ctor-rng-{}.tsv", std::process::id()));
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    let mut world = World::with_maps(vec![(8, 8)]);
    world.rng = RandomNumber::with_seed(SEED);

    // startup 对照批：1 站点 = 4 抽（与登录批前 4 抽同一条 ctor 公共链）。
    let mut plan = CtorFoldPlan::new();
    plan.push_robot_objects(1);
    let startup = world.fold_ctor_draws_game_app_ctor(&plan);
    assert_eq!(startup.batch, CtorDrawBatch::Startup);
    assert_eq!(startup.len(), 4, "1 站点 × 4 抽");

    let login = world.fold_login_player_ctor_draws();
    assert_eq!(login.ledger.batch, CtorDrawBatch::Login);
    assert_eq!(login.ledger.len(), 5, "登录 PlayObject 构造恒 5 抽");

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    let _ = std::fs::remove_file(&path);
    let rows = parse_rows(&txt);
    assert_eq!(
        rows.len(),
        startup.len() + 5,
        "流水总量按批次可核算：startup {} + login 5",
        startup.len()
    );

    // ① startup 4 行：全部落 entity.rs（`BaseAnimalCtorDraws::draw` 唯一抽数点）。
    for (i, row) in rows[..startup.len()].iter().enumerate() {
        assert!(
            is_entity_site(&row.site),
            "startup 第 {i} 行 site 必须是 entity.rs：{}",
            row.site
        );
        assert_eq!(row.kind, startup.entries[i].kind);
        assert_eq!(row.arg, startup.entries[i].arg);
        assert_eq!(row.result, startup.entries[i].result);
    }

    // ② login 前 4 抽：site **与 startup 相同** ⇒ 只靠 site 不能分桶（靠 batch 字段）。
    let login_rows = &rows[startup.len()..];
    for (i, row) in login_rows[..4].iter().enumerate() {
        assert!(
            is_entity_site(&row.site),
            "login 第 {i} 抽 site 仍是 entity.rs（同 site 不能靠 site 分桶）：{}",
            row.site
        );
        assert_eq!(row.kind, CTOR_FOLD_DRAW_KIND);
        assert_eq!(row.arg, login.ledger.entries[i].arg);
        assert_eq!(row.result, login.ledger.entries[i].result);
    }

    // ③ 第 5 抽（`PlayObject.cs:1047` RandomNo）：site 落登录 fold 点 + 结果进实体。
    let last = &login_rows[4];
    assert!(
        is_ctor_fold_site(&last.site),
        "第 5 抽 site 必须落 ctor_fold.rs（登录 fold 点）：{}",
        last.site
    );
    assert_eq!(last.kind, CTOR_FOLD_DRAW_KIND);
    assert_eq!(last.arg, 999_999);
    assert_eq!(last.result, i64::from(login.random_no));
    assert_eq!(
        login.ledger.entries[4].result, last.result,
        "账本第 5 条 = 真流水第 5 行"
    );
}
