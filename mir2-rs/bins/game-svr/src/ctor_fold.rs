//! **构造段 fold 的宿主装配**（`openmir2-ctor-draws-fold`）——把 `Envir` 语料算成
//! [`mir2_world::CtorFoldPlan`]，并按 oracle 次序落进 sim 线程的账本。
//!
//! ## 分工
//!
//! - **主线程**（本模块的 [`build_plan`]）：只读文件、算站点数（**不抽数**）；
//! - **sim 线程**（`main.rs` 的 `sim_loop`）：按 oracle 次序在**三个位置**各调一次
//!   `World::fold_ctor_draws_{game_app_ctor,load_map_info,load_map_quest}`（要 `world.rng` 同一条流）；
//!   位置之间是 oracle 的**屏障**：屏障 A（`ServerConf.cs:114` 的 1 抽，**本片未建模**）在
//!   段①之后、段②之前；屏障 B（`InitializeMonster` `:155` = 宿主 `MonGenHost::start`）在
//!   段②之后；屏障 C（`LoadMerchant` `:227`/`LoadNpcs` `:229`）在段③之后。
//!
//! | 站点 | 计数来源 | oracle 锚 |
//! | --- | --- | --- |
//! | `RobotObject` | `Envir/Robot.txt` 有效行数（`mir2_envir::robot`） | `GameApp.cs:58` → `RobotManage.cs:48` |
//! | `Orphan*Npc` | 恒 3 | `GameApp.cs:85/86/87` |
//! | `MapCheckQuest` | `MapInfo` 的 `CHECKQUEST` 出现次数（`MapLoader::count_map_info_check_quests`） | `GameApp.cs:124` → `Maps.cs:454` |
//! | `MapQuestRow` | `MapQuest.txt` 有效行数（`MapQuestLoad::quests.len()`） | `GameApp.cs:184` → `MapQuestManager.cs:40` |
//! | `Hidden*` | 隐藏脚本 NPC 中件/目录存在者（`MapQuestLoad::hidden_npcs`） | `GameApp.cs:184` 尾 → `LocalDB.cs:268/210/312` |
//!
//! ## 已核的「不抽数」邻居（本片登记，实读 oracle 源码）
//!
//! `GameApp.cs:88-93` 的其余启动期构造者**不抽 RNG**（⇒ 不构成构造段阻塞）：`ItemSystem`
//! （`GameItemSystem.cs:15-18` 只 `new List<StdItem>()`）、`GuildMgr`（`GuildManager.cs:14-17`）、
//! `CastleMgr`（`CastleManager.cs:16-19`）、`EventMgr`（`EventManager.cs:12-16`）各自 ctor 只建表、
//! `MapMgr` **无显式 ctor**（`MapManager.cs:8-18` 只有字段初始化器）、`WorldServer`
//! （`WorldServer.cs:63-95` 只建表 + 读 `GetTickCount`）。`GameApp.cs:55/56/57` 的
//! `GameCommandSystem`（`CommandSystem.cs:19-22` 只 `new GameCmdConf` → `ConfigFile.Load`，
//! 该文件 0 处 `RandomNumber`）、`AuthenticationService`（`AuthenticationService.cs:15-20`）、
//! `ScriptEngine`（`ScriptEngine.cs:30-36` 只装两张字典）同样不抽（与设计件 §4.4.2 一致）。
//! ❗**仍受阻塞的未做项（屏障 A）**：`AppService.cs:87` `GameShare.LoadConfig()` →
//! `GameShare.cs:90` `SystemShare.ServerConf.LoadConfig()` → `ServerConf.cs:114`
//! `SystemShare.Config.ItemNumber += SystemShare.RandomNumber.Random(10000)` ——**1 抽**，
//! 位于 `GameApp` ctor **之后**（`new GameApp(...)` 由 DI 在 `AppService` 构造时发生）、
//! `GameApp.Initialize`（`:124` M2）**之前**（`StartingAsync` 才 `LoadConfig()`）。
//! Rust 侧 `bins/game-svr/src/config.rs` 读同一键但**不抽** ⇒ 真机账本自段①（12 抽）之后的
//! **第 13 条**起即分歧。本片**只留位置、不复刻该抽** ⇒ **账本对齐在本片之后仍受此未做项阻塞**
//! （须另立条目；补上后段②/段③ 即整段对齐，因为相对次序已按 oracle 排好）。
//! 反向核对：`GameApp.cs`/`AppService.cs`/`AppServer.cs`/`TimedService.cs`/`ServerHost.cs`/
//! `FrnEngn.cs`/`GeneratorProcessor.cs` 全文 **0 处** `RandomNumber` ⇒ 启动段除上述屏障 A 外
//! 无其它 RNG 消费者（`GameShare.cs:83` 的 `GetRandpmRoute` 是运行期选网关，不在启动段）。

use mir2_envir::{MapLoader, QuestNpcRecord};
use mir2_world::{CtorFoldPlan, CtorFoldSite};

/// 从 `Envir` 语料算出构造段 fold 计划（主线程；**不抽数**）。
///
/// `map_exists` = oracle `SystemShare.MapMgr.FindMap(sMap) != null` 门（生产传
/// `|n| reg.id_of(n).is_some()`）；它只影响 `MapQuest.txt` 的**有效行**计数（oracle 找不到地图
/// 的行走 `else { result = -i }`，**不建对象、不抽数**，`LocalDB.cs:379-401`）。
///
/// # Errors
/// 文件读失败，或 `MapQuestLoad.hidden_npcs` 出现未知 `ChrName`（表漂移 ⇒ **fail-closed**，
/// 不静默少抽）。
pub fn build_plan(
    loader: &MapLoader,
    map_exists: impl Fn(&str) -> bool,
) -> Result<CtorFoldPlan, String> {
    // 段①：`GameApp.cs:58` `new RobotManage()` → `LoadRobot`（`Robot.txt` 每有效行）。
    let robot = loader
        .load_robot()
        .map_err(|e| format!("读 Envir/Robot.txt 失败：{e}"))?;
    // 段②：`GameApp.cs:124` `Map.LoadMapInfo()` 里的 `CHECKQUEST` 行。
    let check_quests = loader
        .count_map_info_check_quests()
        .map_err(|e| format!("读 Envir/MapInfo.txt 失败：{e}"))?;
    // 段③：`GameApp.cs:184` `LocalDB.LoadMapQuest()`（M1 循环 + 尾三只隐藏 NPC）。
    let map_quest = loader
        .load_map_quest(&map_exists)
        .map_err(|e| format!("读 Envir/MapQuest.txt 失败：{e}"))?;

    let mut plan = CtorFoldPlan::new();
    plan.push_robot_objects(robot.as_ref().map_or(0, Vec::len));
    plan.push_orphan_merchants();
    plan.push_map_check_quests(check_quests);
    plan.push_map_quest_rows(map_quest.quests.len());
    for rec in &map_quest.hidden_npcs {
        plan.push_hidden_npc(hidden_site_of(rec).ok_or_else(|| {
            format!(
                "MapQuest 隐藏 NPC 的 ChrName={} 不在已知三只（QManage/QFunction/RobotManage）\
                 ⇒ 构造段 fold 无法定位站点（fail-closed）",
                rec.chr_name
            )
        })?);
    }
    Ok(plan)
}

/// 隐藏脚本 NPC 的 `ChrName` → fold 站点（次序由 `MapQuestLoad::hidden_npcs` 保证 =
/// `LocalDB.cs:403/404/405` 的 `QMangeNpc`/`QFunctionNpc`/`RobotNpc`）。
#[must_use]
pub fn hidden_site_of(rec: &QuestNpcRecord) -> Option<CtorFoldSite> {
    match rec.chr_name.as_str() {
        "QManage" => Some(CtorFoldSite::HiddenQManage),
        "QFunction" => Some(CtorFoldSite::HiddenQFunction),
        "RobotManage" => Some(CtorFoldSite::HiddenRobotManage),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 用例级唯一临时目录（照 `guardlist.rs` 的 pid+原子计数口径）。
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("gs-ctorfold-{tag}-{}-{n}", std::process::id()));
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

    /// 最小 `Envir` 语料：`Robot.txt`（1 有效行）+ `MapInfo.txt`（2 个 `CHECKQUEST`）+
    /// `MapQuest.txt`（2 有效行）+ 三个隐藏 NPC 件 ⇒ 计划 = 1+3+2 站点（阶段①）+ 2+3 站点（阶段②）。
    fn minimal_corpus(tag: &str) -> (PathBuf, MapLoader) {
        let base = temp_dir(tag);
        let envir = base.join("Envir");
        // 部署件形态（43 字节）：第 1 行注释、第 2 行有效。
        write_gbk(
            &envir.join("Robot.txt"),
            ";机器人名称 脚本名称\r\n系统控制 AutoRunRobot\r\n",
        );
        write_gbk(
            &envir.join("MapInfo.txt"),
            "[0137 边境村] CHECKQUEST(Q0001)\r\n[0 比奇省] CHECKQUEST(Q0002)\r\n",
        );
        write_gbk(
            &envir.join("MapQuest.txt"),
            "0137 [58] 0 鸡 鸡血 Q0001\r\n0 [1] 1 鹿 鹿角 Q0002\r\n;注释行\r\n",
        );
        // 三只隐藏 NPC：目录/文件都在 ⇒ `present`（QManage/QFunction 有件、RobotManage 只有目录）。
        write_gbk(&envir.join("MapQuest_def/QManage.txt"), "[@Login]\r\n");
        write_gbk(
            &envir.join("Market_Def/QFunction-0.txt"),
            "[@PlayLevelUp]\r\n",
        );
        std::fs::create_dir_all(envir.join("Robot_def")).unwrap();
        let loader = MapLoader::new(&base, "Map", "Envir");
        (base, loader)
    }

    /// 语料 → 计划：站点数逐条等于语料事实（**行数 × 1**，不是常量）。
    #[test]
    fn plan_counts_match_minimal_corpus() {
        let (_base, loader) = minimal_corpus("plan");
        let plan = build_plan(&loader, |n| n == "0137" || n == "0").expect("计划成功");
        // 段①（`GameApp` ctor：`:58`/`:85-87`）
        assert_eq!(
            plan.game_app_ctor,
            vec![
                CtorFoldSite::RobotObject,       // Robot.txt 1 有效行（GameApp.cs:58）
                CtorFoldSite::OrphanManageNpc,   // GameApp.cs:85
                CtorFoldSite::OrphanRobotNpc,    // :86
                CtorFoldSite::OrphanFunctionNpc, // :87
            ]
        );
        // 段②（`GameApp.cs:124` ⇒ `Maps.cs:454`）
        assert_eq!(
            plan.load_map_info,
            vec![
                CtorFoldSite::MapCheckQuest, // MapInfo CHECKQUEST × 2
                CtorFoldSite::MapCheckQuest,
            ]
        );
        // 段③（`GameApp.cs:184`：M1 + L2/L1/L3）
        assert_eq!(
            plan.load_map_quest,
            vec![
                CtorFoldSite::MapQuestRow, // MapQuest.txt 2 有效行（M1）
                CtorFoldSite::MapQuestRow,
                CtorFoldSite::HiddenQManage,     // LocalDB.cs:268
                CtorFoldSite::HiddenQFunction,   // :210
                CtorFoldSite::HiddenRobotManage, // :312
            ]
        );
        assert_eq!(plan.total_draws(), 44, "(4 + 2 + 5) 站点 × 4 抽");
    }

    /// **`Robot.txt` 行数 × 4**：加一行有效行 ⇒ 头部多 4 抽（改坏必红：把计数写成常量就红）。
    #[test]
    fn robot_lines_multiply_head_draws() {
        let (base, loader) = minimal_corpus("robot2");
        let one = build_plan(&loader, |_| true).expect("计划成功");
        write_gbk(
            &base.join("Envir/Robot.txt"),
            ";机器人名称 脚本名称\r\n系统控制 AutoRunRobot\r\n机器人2 AutoRunRobot2\r\n",
        );
        let two = build_plan(&loader, |_| true).expect("计划成功");
        assert_eq!(one.game_app_ctor.len(), 4, "1 Robot + 3 孤儿");
        assert_eq!(two.game_app_ctor.len(), 5, "多 1 有效行 ⇒ 多 1 站点");
        assert_eq!(two.total_draws() - one.total_draws(), 4, "每行 4 抽");
    }

    /// **`Robot.txt` 缺失 ⇒ 0 站点**（oracle `File.Exists` 门 `RobotManage.cs:24-27`）。
    #[test]
    fn missing_robot_file_yields_no_robot_sites() {
        let (base, loader) = minimal_corpus("norobot");
        std::fs::remove_file(base.join("Envir/Robot.txt")).unwrap();
        let plan = build_plan(&loader, |_| true).expect("计划成功");
        assert!(!plan.game_app_ctor.contains(&CtorFoldSite::RobotObject));
        assert_eq!(plan.game_app_ctor.len(), 3, "只剩 3 孤儿");
        assert_eq!(plan.load_map_info.len(), 2, "CHECKQUEST 与 Robot.txt 无关");
    }

    /// **`MapQuest.txt` 的有效行门**：地图找不到的行**不建对象、不抽数**
    /// （`LocalDB.cs:379-401` 的 `else { result = -i }`）⇒ 站点数随 `map_exists` 变。
    #[test]
    fn map_quest_rows_follow_findmap_gate() {
        let (_base, loader) = minimal_corpus("mqgate");
        let both = build_plan(&loader, |_| true).expect("计划成功");
        let none = build_plan(&loader, |_| false).expect("计划成功");
        assert_eq!(both.load_map_quest.len(), 5, "2 行 + 3 隐藏");
        assert_eq!(none.load_map_quest.len(), 3, "FindMap 全丢 ⇒ 只剩 3 隐藏");
    }

    /// 隐藏 NPC 未知 `ChrName` ⇒ **fail-closed**（不静默少抽）——直接测映射函数。
    #[test]
    fn unknown_hidden_npc_name_fails_closed() {
        let rec = QuestNpcRecord {
            map_name: "0".to_string(),
            curr_x: 0,
            curr_y: 0,
            chr_name: "QUnknown".to_string(),
            npc_flag: 0,
            appr: 0,
            file_path: "MapQuest_def".to_string(),
            script_name: None,
            is_hide: true,
            is_quest: false,
        };
        assert_eq!(hidden_site_of(&rec), None);
    }
}
