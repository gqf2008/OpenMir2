//! **构造段 fold 与真流水（`MIR2_RNG_LOG`）互核**（`openmir2-ctor-draws-fold` 判据②加强半）。
//!
//! 本文件是**独立测试二进制**（`cargo test -p game-svr --test ctor_fold_rng_trace`）——`rng_trace`
//! 的开关是进程级全局（`OnceLock`），只有独占进程才不会被别的用例的抽数污染。
//!
//! 判据（一条）：开着与 C# 钩子同口径的 trace 跑一遍构造段 fold + 屏障 A，落盘流水（跳过两行头部）
//! **逐行**等于 [`CtorFoldLedger`] 的同序号条目：
//! - ctor 公共链的 `site` 必须落在 `crates/world/src/entity.rs:`（`consumer_groups.tsv` 的
//!   `construct` 组；`BaseAnimalCtorDraws::draw` 是 ctor 链的**唯一抽数点**）；
//! - 屏障 A（`ServerConf.cs:114` 的 `Random(10000)`）的 `site` 必须**外传到调用方**（该抽数点标了
//!   `#[track_caller]`）：生产 = `bins/game-svr/src/lib.rs`（`consumer_groups.tsv` 的
//!   `startup|rust|bins/game-svr/src/lib\.rs:` 行）⇒ 断言它**不落** `crates/world/src/`，
//!   否则结构一致性门会把它算成 `other` 组判红；
//! - `kind`/`arg`/`result` 逐列相等 ⇒ 账本**就是**真流水的一份符号化投影（site 换成 oracle 锚），
//!   真机对拍时两侧比较的就是这批行。
//!
//! 证据用法（把真实 trace 留给 `rng_ledger_diff.py` 复算）：
//! ```text
//! M2_CTORFOLD_TRACE_PATH=E:/tmp/ctorfold-rust.tsv \
//!   cargo test -p game-svr --test ctor_fold_rng_trace -- --nocapture
//! ```

use mir2_envir::MapLoader;
use mir2_shared::rng::RandomNumber;
use mir2_shared::rng_trace;
use mir2_world::{CtorFoldEntry, CtorFoldSite};

const SEED: i32 = 20240229;
const DEPLOYED_ROBOT_TXT: &str = ";机器人名称 脚本名称\r\n系统控制 AutoRunRobot\r\n";

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
        // 头部两行（`1\tINJECT_OK…` / `SEED_PROBE …`）与非 TSV 标记行跳过：seq 列必须可解析且
        // 至少 5 列（与 `attack_rng_ledger.py` 同判据）。
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

fn write_gbk(path: &std::path::Path, text: &str) {
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, &bytes[..]).unwrap();
}

#[test]
fn fold_ledger_row_by_row_equals_rng_trace() {
    let from_env = std::env::var("M2_CTORFOLD_TRACE_PATH").ok();
    let path = match from_env.as_deref() {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("gs-ctorfold-rng-{}.tsv", std::process::id())),
    };
    let _ = std::fs::remove_file(&path);
    rng_trace::enable(&path, SEED).expect("打开 rng trace");

    // 最小语料（同 `ctor_fold_ledger.rs` 的主语料；只放本用例要用的三件）。
    let base = std::env::temp_dir().join(format!("gs-ctorfold-trace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let envir = base.join("Envir");
    std::fs::create_dir_all(&envir).unwrap();
    write_gbk(&envir.join("Robot.txt"), DEPLOYED_ROBOT_TXT);
    write_gbk(
        &envir.join("MapInfo.txt"),
        "[0137 边境村] CHECKQUEST(Q0001)\r\n[0 比奇省] CHECKQUEST(Q0002) CHECKQUEST(Q0003)\r\n",
    );
    write_gbk(
        &envir.join("MapQuest.txt"),
        "0137 [58] 0 鸡 鸡血 Q0001\r\n0 [1] 1 鹿 鹿角 Q0002\r\n",
    );
    write_gbk(&envir.join("MapQuest_def/QManage.txt"), "[@Login]\r\n");
    write_gbk(
        &envir.join("Market_Def/QFunction-0.txt"),
        "[@PlayLevelUp]\r\n",
    );
    std::fs::create_dir_all(envir.join("Robot_def")).unwrap();
    let loader = MapLoader::new(&base, "Map", "Envir");

    let plan =
        game_svr::ctor_fold::build_plan(&loader, |n| n == "0137" || n == "0").expect("计划成功");
    // 段①（`GameApp` ctor）= 1 Robot + 3 孤儿；屏障 A = 1（`ServerConf.cs:114`，与语料无关）；
    // 段②（`:124`）= 3 `CHECKQUEST`（第 2 行两个）；段③（`:184`）= 2 M1 行 + 3 隐藏 NPC
    // ⇒ 本用例的账本共 12 站点 × 4 抽 + 屏障 A 1 抽 = 49 条。
    assert_eq!(plan.game_app_ctor.len(), 4, "1 Robot + 3 孤儿");
    assert_eq!(plan.barrier_a.len(), 1, "屏障 A（恒 1，与语料无关）");
    assert_eq!(plan.load_map_info.len(), 3, "3 CHECKQUEST");
    assert_eq!(plan.load_map_quest.len(), 5, "2 M1 行 + 3 隐藏 NPC");

    let mut world = mir2_world::World::with_maps(vec![(20, 20)]);
    world.rng = RandomNumber::with_seed(SEED);
    // 四段按 oracle 次序落账（段① → 屏障 A → 段② → 段③；屏障 B/C 本用例不涉及）。
    let seg1 = world.fold_ctor_draws_game_app_ctor(&plan);
    let barrier_a = world.fold_ctor_draw_item_number_barrier_a(&plan);
    let seg2 = world.fold_ctor_draws_load_map_info(&plan);
    let seg3 = world.fold_ctor_draws_load_map_quest(&plan);
    let ledger: Vec<&CtorFoldEntry> = seg1
        .entries
        .iter()
        .chain(barrier_a.entries.iter())
        .chain(seg2.entries.iter())
        .chain(seg3.entries.iter())
        .collect();

    let txt = std::fs::read_to_string(&path).expect("读 trace");
    if from_env.is_none() {
        let _ = std::fs::remove_file(&path);
    }
    println!("CTORFOLD_TRACE_PATH {path:?}");
    let rows = parse_rows(&txt);
    assert_eq!(
        rows.len(),
        ledger.len(),
        "流水行数必须等于账本条数（4 × 站点数 = {}；多/少都说明 fold 漏抽或多抽）",
        ledger.len()
    );
    for (i, (row, entry)) in rows.iter().zip(ledger.iter()).enumerate() {
        if entry.site == CtorFoldSite::BarrierAItemNumber {
            // 屏障 A 的抽数点标了 `#[track_caller]` ⇒ 站点**外传到调用方**（生产 =
            // `bins/game-svr/src/lib.rs`）；本用例在测试进程里调 ⇒ 记为**本测试文件**。
            // 唯一不许的是落回 `crates/world/src/`（那会让它掉出 `startup` 组、落 `other` 判红）。
            assert!(
                !row.site.contains("crates/world/src/")
                    && !row.site.contains("crates\\world\\src\\"),
                "第 {i} 行（屏障 A）site 必须外传到调用方（`#[track_caller]`）：{}",
                row.site
            );
        } else {
            assert!(
                row.site.contains("crates/world/src/entity.rs:")
                    || row.site.contains("crates\\world\\src\\entity.rs:"),
                "第 {i} 行 site 必须落在 ctor 链唯一抽数点 entity.rs（`construct` 组）：{}",
                row.site
            );
        }
        assert_eq!(row.kind, entry.kind, "第 {i} 行 kind");
        assert_eq!(row.arg, entry.arg, "第 {i} 行 arg");
        assert_eq!(
            row.result, entry.result,
            "第 {i} 行 result（site={:?}）",
            entry.site
        );
        let fold_site = format!("{:?}", entry.site);
        println!(
            "CTORFOLD_ROW {i}\t{}\t{}\t{}\t{}\t{}",
            row.kind, row.arg, row.result, row.site, fold_site
        );
    }
    // 站点序的机器可判摘要（人读证据用；逐项断言在上面的循环里）。
    let sites: Vec<CtorFoldSite> = ledger.iter().map(|e| e.site).collect();
    println!("CTORFOLD_SITES {sites:?}");
}
