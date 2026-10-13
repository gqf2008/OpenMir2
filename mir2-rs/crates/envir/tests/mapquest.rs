#![allow(clippy::doc_markdown)]
//! `MapQuest.txt` 加载的构造级/真机向量门（卡 `openmir2-m3-quest-load`，M3⑥-实现 1）。
//!
//! - `synthetic_vectors`：**常驻门禁**，覆盖 `M3⑥任务现状盘点报告` §1.A 列出的 7 列切分、
//!   `*` 通配、`[flag]` 剥离、`GROUP` 前缀比、引号剥离、越界/缺列失败、`result = -i`、
//!   隐藏 NPC 三段同形记录；与 `crates/envir/src/mapquest.rs` 内联用例互补（此处是**输入 → 期望**
//!   的显式向量表，供改坏必红与审计对照）。
//! - `deployment_mapquest`：`#[ignore]`（读仓外 `E:\MirServer`），由真机显式跑：
//!   ```text
//!   $env:MIR2_ENVIR_DIR='E:\MirServer\M2GameSvr\Envir'
//!   cargo test -p mir2-envir --test mapquest -- --ignored --nocapture
//!   ```

use std::path::{Path, PathBuf};

use mir2_envir::{MapLoader, MapQuestLine};

fn deployment_envir_dir() -> PathBuf {
    std::env::var("MIR2_ENVIR_DIR").map_or_else(
        |_| PathBuf::from(r"E:\MirServer\M2GameSvr\Envir"),
        PathBuf::from,
    )
}

fn temp_envir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mir2-mapquest-it-{}-{tag}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_gbk(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let (bytes, _, _) = encoding_rs::GBK.encode(text);
    std::fs::write(path, bytes.as_ref()).unwrap();
}

/// 部署件 `Envir/MapQuest.txt` 的唯一数据行（`E:/MirServer` 实测，见证据 `01-deployment-input.txt`）。
const DEPLOYED_LINE: &str = "Q001     [58]     0      骷髅精灵5     *               Q002";

#[test]
fn synthetic_vectors() {
    let any = |_: &str| true;

    // ① 部署首行逐字段。
    let MapQuestLine::Created(info) = mir2_envir::parse_map_quest_line(DEPLOYED_LINE, any).unwrap()
    else {
        panic!("部署首行应建任务");
    };
    assert_eq!(
        (
            info.flag,
            info.value,
            info.mon_name.as_str(),
            info.item_name.as_str(),
            info.grouped,
            info.npc.chr_name.as_str(),
        ),
        (58, 0, "骷髅精灵5", "", false, "Q002")
    );

    // ② `[flag]` 剥离 + `sItem == "*"` 清空 + `GROUP` 前缀比 + `nValue` 钳 1。
    let MapQuestLine::Created(info) =
        mir2_envir::parse_map_quest_line("Q001 [7] 5 怪 * 脚本 GROUPX", any).unwrap()
    else {
        panic!()
    };
    assert_eq!((info.flag, info.value, info.item_name.as_str()), (7, 1, ""));
    assert!(info.grouped, "CompareLStr 只比前 5 字符");

    // ③ 三列任一为空 ⇒ Failed。
    assert_eq!(
        mir2_envir::parse_map_quest_line("Q001 [7] 0 怪 *", any).unwrap(),
        MapQuestLine::Failed
    );

    // ④ 地图找不到 ⇒ Failed。
    assert_eq!(
        mir2_envir::parse_map_quest_line(DEPLOYED_LINE, |_| false).unwrap(),
        MapQuestLine::Failed
    );

    // ⑤ `nFlag < 0` ⇒ CreateQuest false ⇒ Failed。
    assert_eq!(
        mir2_envir::parse_map_quest_line("Q001 [-1] 0 怪 * 脚本", any).unwrap(),
        MapQuestLine::Failed
    );

    // ⑥ `sQuest == "*"` ⇒ NPC 名清空（oracle 缺陷 ②）。
    let MapQuestLine::Created(info) =
        mir2_envir::parse_map_quest_line("Q001 [7] 0 怪 * *", any).unwrap()
    else {
        panic!()
    };
    assert_eq!(info.npc.chr_name, "");
}

#[test]
fn load_orders_hidden_npcs_and_reports_result() {
    let base = temp_envir("order");
    let envir = base.join("Envir");
    // 两条任务行 + 一条失败行（缺列）⇒ result = -2（0-based 索引）。
    write_gbk(
        &envir.join("MapQuest.txt"),
        &format!("{DEPLOYED_LINE}\nQ404 [1] 1 怪 * 脚本\nQ005 [1] 0 怪 *\n"),
    );
    write_gbk(&envir.join("MapQuest_def/QManage.txt"), "[@Login]\n");
    write_gbk(
        &envir.join("Market_Def/QFunction-0.txt"),
        "[@PlayLevelUp]\n",
    );
    std::fs::create_dir_all(envir.join("Robot_def")).unwrap();
    let loader = MapLoader::new(&base, "Map", "Envir");
    let load = loader
        .load_map_quest(|name| name != "Q404")
        .expect("加载成功");
    assert_eq!(load.result, -2);
    assert_eq!(load.quests.len(), 1);
    // QuestNpcList：MapQuest.txt 的 NPC（Q002）→ QManage → RobotManage。
    let names: Vec<&str> = load
        .quest_npcs
        .iter()
        .map(|n| n.chr_name.as_str())
        .collect();
    assert_eq!(names, ["Q002", "QManage", "RobotManage"]);
    let m: Vec<&str> = load.merchants.iter().map(|n| n.chr_name.as_str()).collect();
    assert_eq!(m, ["QFunction"]);
}

#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_ENVIR_DIR 显式跑"]
fn deployment_mapquest() {
    let envir = deployment_envir_dir();
    assert!(
        envir.is_dir(),
        "真机 Envir 目录不存在：{}（用 MIR2_ENVIR_DIR 指定）",
        envir.display()
    );
    let base = envir.parent().expect("Envir 的父目录");
    let loader = MapLoader::new(base, "Map", "Envir");
    let registry = loader.load_registry().expect("MapInfo 加载");
    let load = loader
        .load_map_quest(|name| registry.id_of(name).is_some())
        .expect("MapQuest 加载");
    eprintln!(
        "MapQuest.txt: result={} quests={} quest_npcs={:?} merchants={:?}",
        load.result,
        load.quests.len(),
        load.quest_npcs
            .iter()
            .map(|n| n.chr_name.as_str())
            .collect::<Vec<_>>(),
        load.merchants
            .iter()
            .map(|n| n.chr_name.as_str())
            .collect::<Vec<_>>(),
    );
    assert_eq!(load.result, 1);
    assert_eq!(load.quests.len(), 1, "部署 MapQuest.txt 只有 Q001 一行");
    let q = &load.quests[0];
    assert_eq!(
        (
            q.flag,
            q.value,
            q.mon_name.as_str(),
            q.item_name.as_str(),
            q.grouped,
            q.npc.chr_name.as_str(),
        ),
        (58, 0, "骷髅精灵5", "", false, "Q002")
    );
    // QManage/QFunction 存在、RobotManage 由 oracle 补建 ⇒ 三段都在 QuestNpcList/MerchantList。
    assert!(load
        .quest_npcs
        .iter()
        .any(|n| n.chr_name == "QManage" && n.file_path == "MapQuest_def"));
    assert!(load
        .quest_npcs
        .iter()
        .any(|n| n.chr_name == "RobotManage" && n.file_path == "Robot_def"));
    assert!(load
        .merchants
        .iter()
        .any(|n| n.chr_name == "QFunction" && n.script_name.as_deref() == Some("QFunction")));
}
