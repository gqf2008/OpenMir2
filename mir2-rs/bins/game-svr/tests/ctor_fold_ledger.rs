//! **构造段 fold 账本判据**（`openmir2-ctor-draws-fold` 判据②）——「给定固定种子 + 一份最小
//! `Envir` 语料 ⇒ 账本前 N 项的 `site`/`order`/`kind` 与 oracle **逐项**相等」，期望表**写在本用例里**。
//!
//! ## 语料（最小，但每处门都踩到）
//!
//! | 文件 | 内容 | 站点 |
//! | --- | --- | --- |
//! | `Envir/Robot.txt` | 部署件逐字节复刻（注释行 + **1 有效行**） | `RobotObject` × 1 |
//! | `Envir/MapInfo.txt` | 2 个 section、共 **3 个 `CHECKQUEST`**（其中一行两个） | `MapCheckQuest` × 3 |
//! | `Envir/MapQuest.txt` | **2 有效行**（+1 注释行） | `MapQuestRow` × 2 |
//! | `Envir/{MapQuest_def/QManage.txt, Market_Def/QFunction-0.txt, Robot_def/}` | 三只隐藏 NPC 件/目录都在 | `HiddenQManage`/`HiddenQFunction`/`HiddenRobotManage` |
//!
//! ## 判据（按 oracle 三段 + 两条屏障）
//!
//! 1. **段①**（`GameApp` ctor：`GameApp.cs:58`/`:85-87`）4 站点 × 4 抽 ⇒ `site`/`order`/`kind`/`arg`
//!    逐项 = [`EXPECTED_SEG_ONE`]（字面表），`result` 逐项 = 同种子独立流；
//! 2. **屏障 A**（`AppService.cs:87` → `ServerConf.cs:114` 的 `Random(10000)`，**1 抽，本片未建模**）：
//!    本用例用**第二条** oracle 对齐流显式抽出它，并断言 Rust 侧**没有**这一抽（⇒ 段② 起两侧差 1，
//!    这是**登记在案**的未做项，见 `crates/world/src/ctor_fold.rs` 模块头）——
//!    **将来补上屏障 A 时本断言必红**，即本用例也是「补屏障 A」那条线的改坏必红牙；
//! 3. **段②**（`GameApp.cs:124` ⇒ `Maps.cs:454` 的 `CHECKQUEST`）3 站点 × 4 抽 ⇒ 逐项 = [`EXPECTED_SEG_TWO`]；
//! 4. **屏障 B**（`GameApp.cs:155` `InitializeMonster`）：本用例插一抽占位（真机是宿主
//!    `MonGenHost::start` 的 `Random(limit)`），断言段③ 在它**之后**；
//! 5. **段③**（`GameApp.cs:184`：M1 行 → L2/L1/L3）5 站点 × 4 抽 ⇒ 逐项 = [`EXPECTED_SEG_THREE`]；
//! 6. **改坏必红**：见 [`two_line_robot_file_shifts_the_whole_ledger`]（语料改 2 行 ⇒ 从第 4 条起
//!    全部错位）与 `crates/world/src/ctor_fold.rs` 的单测（删站点 / 调次序 / 挪屏障 A）。

use std::path::PathBuf;

use mir2_envir::MapLoader;
use mir2_shared::rng::RandomNumber;
use mir2_world::{CtorFoldEntry, CtorFoldSite, CTOR_FOLD_BARRIER_A_DRAWS, CTOR_FOLD_DRAW_KIND};

/// **期望表 · 段①（字面，逐站点一组）**：`(站点, kind, [4 抽的 arg])` —— 次序即 oracle 次序：
/// `GameApp.cs:58`（`Robot.txt` 1 有效行）→ `:85/86/87`（G1/G2/G3）。
const EXPECTED_SEG_ONE: [(CtorFoldSite, &str, [i64; 4]); 4] = [
    (
        CtorFoldSite::RobotObject,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::OrphanManageNpc,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::OrphanRobotNpc,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::OrphanFunctionNpc,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
];

/// **期望表 · 段②（字面）**：`GameApp.cs:124` `Map.LoadMapInfo()` 的 3 个 `CHECKQUEST` 行
/// （每个命令一次 `LoadMapQuest` ⇒ `Maps.cs:454` 一次 `new Merchant` ⇒ 4 抽）。
const EXPECTED_SEG_TWO: [(CtorFoldSite, &str, [i64; 4]); 3] = [
    (
        CtorFoldSite::MapCheckQuest,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::MapCheckQuest,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::MapCheckQuest,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
];

/// **期望表 · 段③（字面）**：`GameApp.cs:184` 的 M1（`MapQuest.txt` 2 有效行）后接
/// `:403/404/405` 的 L2/L1/L3 —— 三者次序 = oracle 调用序（`QMangeNpc` → `QFunctionNpc` →
/// `RobotNpc`），且**必须**晚于 `InitializeMonster`（`:155`）、**早于** `LoadMerchant`（`:227`）。
const EXPECTED_SEG_THREE: [(CtorFoldSite, &str, [i64; 4]); 5] = [
    (
        CtorFoldSite::MapQuestRow,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::MapQuestRow,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::HiddenQManage,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::HiddenQFunction,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
    (
        CtorFoldSite::HiddenRobotManage,
        CTOR_FOLD_DRAW_KIND,
        [1500, 2000, 3000, 3000],
    ),
];

/// 固定种子（与真机 `MIR2_RNG_SEED` 注入口同口径；本用例两侧都用它）。
const SEED: i32 = 20240229;

/// 屏障 A 的抽数上界（oracle `ServerConf.cs:114` 的 `Random(10000)`）。
const BARRIER_A_MOD: i32 = 10_000;
/// 屏障 B 槽位的占位抽数上界（oracle `WorldServer.MonGen.cs:66` 的 `Random(limit)`）。
const MON_GEN_SLOT_MOD: i32 = 6;

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn temp_base(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "gs-ctorfold-ledger-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Envir")).unwrap();
    dir
}

fn write_gbk(path: &std::path::Path, text: &str) {
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, &bytes[..]).unwrap();
}

/// 部署件 `E:/MirServer/M2GameSvr/Envir/Robot.txt` 的逐字节形态（43 字节；1 注释 + 1 有效行）。
const DEPLOYED_ROBOT_TXT: &str = ";机器人名称 脚本名称\r\n系统控制 AutoRunRobot\r\n";

/// 最小语料（`robot_extra_lines` = `Robot.txt` 里追加的有效行数，0 = 部署件原样）。
fn corpus(tag: &str, robot_extra_lines: usize) -> (PathBuf, MapLoader) {
    let base = temp_base(tag);
    let envir = base.join("Envir");
    let mut robot = DEPLOYED_ROBOT_TXT.to_string();
    for i in 0..robot_extra_lines {
        robot.push_str(&format!("机器人{} AutoRunRobot{i}\r\n", i + 2));
    }
    write_gbk(&envir.join("Robot.txt"), &robot);
    // 2 个 section、3 个 CHECKQUEST（第 1 行两个、第 2 行一个）。
    write_gbk(
        &envir.join("MapInfo.txt"),
        "[0137 边境村] CHECKQUEST(Q0001) CHECKQUEST(Q0002)\r\n[0 比奇省] CHECKQUEST(Q0003)\r\n",
    );
    // 2 有效行 + 1 注释行（6 列，照部署件 `Q001     [58]     0      骷髅精灵5     *               Q002`）。
    write_gbk(
        &envir.join("MapQuest.txt"),
        "0137 [58] 0 鸡 鸡血 Q0001\r\n0 [1] 1 鹿 鹿角 Q0002\r\n;注释行\r\n",
    );
    // 三只隐藏 NPC：两个文件 + 一个目录（都要 present）。
    write_gbk(&envir.join("MapQuest_def/QManage.txt"), "[@Login]\r\n");
    write_gbk(
        &envir.join("Market_Def/QFunction-0.txt"),
        "[@PlayLevelUp]\r\n",
    );
    std::fs::create_dir_all(envir.join("Robot_def")).unwrap();
    let loader = MapLoader::new(&base, "Map", "Envir");
    (base, loader)
}

fn expand<'a>(sites: &'a [(CtorFoldSite, &'a str, [i64; 4])]) -> Vec<(CtorFoldSite, &'a str, i64)> {
    let mut out = Vec::new();
    for (site, kind, args) in sites {
        for arg in args {
            out.push((*site, *kind, *arg));
        }
    }
    out
}

fn rows(entries: &[CtorFoldEntry]) -> Vec<(CtorFoldSite, &'static str, i64)> {
    entries.iter().map(|e| (e.site, e.kind, e.arg)).collect()
}

/// 逐项断言「账本的 `result` 列 = 独立流按同一 `arg` 抽出的原值」。
fn assert_results_follow_stream(entries: &[CtorFoldEntry], exp: &mut RandomNumber, label: &str) {
    for (i, e) in entries.iter().enumerate() {
        assert_eq!(
            e.result,
            i64::from(exp.random_below(e.arg as i32)),
            "{label} 第 {i} 条：result 必须等于独立流按同一 arg 的抽数（site={:?} arg={}）",
            e.site,
            e.arg
        );
    }
}

/// **判据②（本条是主判据）**：三段 + 两条屏障，`site`/`order`/`kind`/`arg` 逐项 = 期望表；
/// `result` 列逐项 = 同种子独立流；屏障 A 的**缺口**机器可判。
#[test]
fn ledger_prefix_matches_expected_table() {
    let (_base, loader) = corpus("main", 0);
    let plan = game_svr::ctor_fold::build_plan(&loader, |n| n == "0137" || n == "0")
        .expect("最小语料应能算出计划");
    assert_eq!(
        plan.game_app_ctor.len(),
        4,
        "1 Robot + 3 孤儿（GameApp ctor 段）"
    );
    assert_eq!(plan.load_map_info.len(), 3, "3 CHECKQUEST（:124 段）");
    assert_eq!(
        plan.load_map_quest.len(),
        5,
        "2 M1 行 + 3 隐藏 NPC（:184 段）"
    );

    let mut world = mir2_world::World::with_maps(vec![(20, 20)]);
    world.rng = RandomNumber::with_seed(SEED);
    // 与 world 同步的独立流（**不含**屏障 A —— Rust 侧不抽它）。
    let mut exp = RandomNumber::with_seed(SEED);

    // ---- 段①（`GameApp` ctor：`:58`/`:85-87`）----
    let seg1 = world.fold_ctor_draws_game_app_ctor(&plan);
    assert_eq!(seg1.len(), 16, "段① 4 站点 × 4 抽");
    assert_eq!(
        rows(&seg1.entries),
        expand(&EXPECTED_SEG_ONE),
        "段① site/order/kind/arg 逐项相等"
    );
    assert_results_follow_stream(&seg1.entries, &mut exp, "段①");

    // ---- 段②（`GameApp.cs:124` ⇒ `Maps.cs:454`）----
    // ⚠ Rust 侧段② **紧跟**段①（中间不抽），而 oracle 在两者之间有屏障 A 的 1 抽（下节断言）。
    let seg2 = world.fold_ctor_draws_load_map_info(&plan);
    assert_eq!(seg2.len(), 12, "段② 3 站点 × 4 抽");
    assert_eq!(
        rows(&seg2.entries),
        expand(&EXPECTED_SEG_TWO),
        "段② site/order/kind/arg 逐项相等"
    );
    assert_results_follow_stream(&seg2.entries, &mut exp, "段②");

    // ---- 屏障 A（`ServerConf.cs:114` 的 `Random(10000)`，1 抽，**本片未建模**）----
    assert_eq!(CTOR_FOLD_BARRIER_A_DRAWS, 1, "屏障 A = 1 抽");
    let oracle_seg2_first = {
        // oracle 对齐流（独立重建，不动 `exp`）：段① 的 16 抽 → 屏障 A 的 1 抽 → 段② 的首抽。
        let mut r = RandomNumber::with_seed(SEED);
        for e in &seg1.entries {
            let _ = r.random_below(e.arg as i32);
        }
        let barrier_a = r.random_below(BARRIER_A_MOD); // ← oracle 独有（Rust 未建模）
        assert!(
            (0..BARRIER_A_MOD).contains(&barrier_a),
            "屏障 A 是 Random(10000) 的一抽"
        );
        r.random_below(1500)
    };
    // **登记项（改坏必红）**：Rust 侧没有屏障 A 这一抽 ⇒ 段② 首抽与 oracle 的不同（差 1 抽，
    // 即段②/段③ 的**绝对**位置差 1；相对次序已按 oracle 排好）。将来补上屏障 A 时本断言必红
    // ⇒ 本用例就是「补屏障 A」那条线的牙，须同步更新本用例与 `crates/world/src/ctor_fold.rs` 的登记。
    assert_ne!(
        i64::from(oracle_seg2_first),
        seg2.entries[0].result,
        "屏障 A 未建模 ⇒ 段② 起与 oracle 差 1 抽（登记于 crates/world/src/ctor_fold.rs 模块头）"
    );

    // ---- 屏障 B（`GameApp.cs:155` `InitializeMonster` 槽位）----
    let mid = world.rng.random_below(MON_GEN_SLOT_MOD);
    assert_eq!(
        mid,
        exp.random_below(MON_GEN_SLOT_MOD),
        "InitializeMonster 槽位在段②之后（两侧同位置）"
    );

    // ---- 段③（`GameApp.cs:184`：M1 + L2/L1/L3）----
    let seg3 = world.fold_ctor_draws_load_map_quest(&plan);
    assert_eq!(seg3.len(), 20, "段③ 5 站点 × 4 抽");
    assert_eq!(
        rows(&seg3.entries),
        expand(&EXPECTED_SEG_THREE),
        "段③ site/order/kind/arg 逐项相等（M1 行 → QManage → QFunction → RobotManage）"
    );
    assert_results_follow_stream(&seg3.entries, &mut exp, "段③");

    // ---- 屏障 C（`LoadMerchant`/`LoadNpcs` L4/L5 槽位）----
    let l4 = world.rng.random_below(1500);
    assert_eq!(l4, exp.random_below(1500), "L4/L5 槽位在段③之后");
    assert_eq!(world.rng.random(), exp.random(), "Rust 侧全段流位置对齐");
}

/// **改坏必红（语料）**：`Robot.txt` 有效行 1 → 2 ⇒ 段① 4 抽变 8 抽、其后**全部错位**
/// ⇒ 与主判据的字面期望表在 **index 4** 首次分歧（证明计数是**行数 × 4**、不是常量 4）。
#[test]
fn two_line_robot_file_shifts_the_whole_ledger() {
    let (_base, loader) = corpus("robot2", 1);
    let plan =
        game_svr::ctor_fold::build_plan(&loader, |n| n == "0137" || n == "0").expect("计划成功");
    assert_eq!(plan.game_app_ctor.len(), 5, "2 有效行 + 3 孤儿 ⇒ 5 站点");
    assert_eq!(plan.total_draws(), 52, "(5 + 3 + 5) 站点 × 4 抽");

    let mut world = mir2_world::World::with_maps(vec![(20, 20)]);
    world.rng = RandomNumber::with_seed(SEED);
    let seg1 = world.fold_ctor_draws_game_app_ctor(&plan);
    let got = rows(&seg1.entries);
    let expect = expand(&EXPECTED_SEG_ONE);
    // 前 4 条（Robot.txt 第 1 行）仍相同；第 5 条起错位（多出来的第 2 行插在中间）。
    assert_eq!(got[..4], expect[..4], "第 1 行的 4 抽不受影响");
    let first_diff = got
        .iter()
        .zip(expect.iter())
        .position(|(a, b)| a != b)
        .expect("必须出现分歧");
    assert_eq!(first_diff, 4, "第 2 行插在第 5 条 ⇒ 首次分歧 index = 4");
}

/// **`MapQuest.txt` 有效行数 × 4**：多一行有效行 ⇒ 段③多 4 抽（同样会使其后错位）。
#[test]
fn map_quest_row_count_multiplies_phase_three() {
    let (base, loader) = corpus("mq2", 0);
    let one = game_svr::ctor_fold::build_plan(&loader, |_| true).expect("计划成功");
    write_gbk(
        &base.join("Envir/MapQuest.txt"),
        "0137 [58] 0 鸡 鸡血 Q0001\r\n0 [1] 1 鹿 鹿角 Q0002\r\n0137 [1] 1 鹿 鹿角 Q0003\r\n",
    );
    let two = game_svr::ctor_fold::build_plan(&loader, |_| true).expect("计划成功");
    assert_eq!(one.load_map_quest.len(), 5);
    assert_eq!(two.load_map_quest.len(), 6, "多 1 有效行 ⇒ 多 1 站点");
    assert_eq!(two.total_draws() - one.total_draws(), 4);
}
