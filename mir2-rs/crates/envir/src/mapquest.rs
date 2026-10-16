//! `Envir/MapQuest.txt` 任务配置 + 隐藏任务 NPC 加载 —— 1:1 复刻
//! `src/GameSrv/DB/LocalDB.cs:341`（`LoadMapQuest`）+ `:192`/`:236`/`:293`
//! （`QFunctionNpc`/`QMangeNpc`/`RobotNpc`）+ `src/GameSrv/Maps/MapQuestManager.cs:10`
//! （`CreateQuest`）。
//!
//! 交付卡：`openmir2-m3-quest-load`（M3⑥-实现 1）。盘点：
//! `mir2-rs/tests/parity/m3/M3⑥任务现状盘点报告-20261013.md` §1.A。
//!
//! ## oracle 链（行号已逐条在 `src/` 实读复核）
//!
//! | 步骤 | 行号 |
//! | --- | --- |
//! | 启动调 `LoadMapQuest` | `GameSrv/GameApp.cs:184` |
//! | 主文件 `M2Share.GetEnvirFilePath("MapQuest.txt")` | `LocalDB.cs:352` |
//! | 7 列解析（`{ ' ', '\t' }`，见 `LocalDB.cs:12`） | `LocalDB.cs:361-376` |
//! | 入库门：`sMap`/`sMonName`/`sQuest` 非空 && `MapMgr.FindMap(sMap) != null` | `LocalDB.cs:377-403` |
//! | 建 `MapQuestInfo` + 隐藏 `Merchant` → `AddQuestNpc` | `MapQuestManager.cs:10-55` |
//! | 隐藏 NPC `QMangeNpc`/`QFunctionNpc`/`RobotNpc` | `LocalDB.cs:236` / `:192` / `:293` |
//!
//! ## 三条**刻意保留的 oracle 语义**（§1 硬约束③：行为等价优先于修 bug）
//!
//! 1. **`MapQuestInfo` 没有 `Quest` 字段**（`src/OpenMir2/Data/MapQuestInfo.cs`）：
//!    脚本名 `sQuest` **只**进隐藏 `Merchant.ChrName`，不进 `MapQuestInfo` ⇒ 本模块的
//!    [`MapQuestInfo`] 也不带 `quest` 字段（不要"补"一个）。
//! 2. **`sQuest == "*"` 时先清空再当 NPC 名**（`MapQuestManager.cs:35-38` 在 `:45` 之前）⇒
//!    `ChrName` 会是空串（不是 `"*"`）；照搬。
//! 3. **`nValue > 1` 被钳到 1，但 `nValue < 0` 不钳**（`MapQuestManager.cs:20-23`）；
//!    **`nFlag < 0` 直接失败返回 false**（`:12-15`）。
//!
//! ## 建模差异（登记）：本模块**只读**，不写 Envir 数据目录
//!
//! oracle 在 `QManage.txt`/`QFunction-0.txt`/`RobotManage.txt` 缺失时会 `CreateDirectory` +
//! `SaveToFile` **生成模板文件**（`LocalDB.cs:238-257` / `:197-204` / `:298-302`），随后
//! `File.Exists` 为真 ⇒ 仍建 NPC。Rust 侧**冻结数据目录**（硬约束②：不改数据文件格式/内容），
//! 故本模块**不落盘**：当文件缺失时仍产出同形的 NPC 记录（因为 oracle 会补建成功），并把
//! 「oracle 会写什么」放进 [`MapQuestLoad::hidden_files_created`]（纯数据，不触碰磁盘）。
//! 仅在「父目录不可创建/不可写」这种 oracle 会走 `catch` 的异常路径上两侧才会不同 —— 部署件
//! 三个目录都存在，该路径不可达。

use std::path::PathBuf;

use mir2_shared::hutil32::{
    arrest_string_ex_ref, compare_lstr, get_valid_str3, str_to_int, ArrestWrite,
};

use crate::error::EnvirError;
use crate::paths::envir_file_path;
use crate::registry::MapLoader;
use crate::text::decode_text;

/// C# `LocalDB.TextSplitConst = { ' ', '\t' }`（`LocalDB.cs:12`）。
pub const TEXT_SPLIT_CONST: [char; 2] = [' ', '\t'];

/// 隐藏脚本 NPC 的归属列表（oracle `AddQuestNpc` vs `AddMerchant`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HiddenNpcTarget {
    /// `WorldServer.AddQuestNpc`（`WorldServer.cs:1924`）→ `QuestNpcList`。
    QuestNpcList,
    /// `WorldServer.AddMerchant`（`WorldServer.cs:1572`）→ `MerchantList`。
    MerchantList,
}

/// `Merchant`/`NormNpc`/`BaseObject` 在**配置加载层**可对拍字段的子集
/// （`src/GameSrv/Npc/Merchant.cs`、`src/M2Server/Npc/NormNpc.cs:23-37`、`BaseObject.cs:24/89`）。
///
/// 世界侧运行时字段（`GoodsList`/`ScriptList`/`ProcessRefillIndex`…）由实现卡按需承载，
/// 本 crate 只落与 `MapQuest.txt`/隐藏 NPC 创建有关的常量列。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestNpcRecord {
    /// `MapName`（oracle 恒 `"0"`）。
    pub map_name: String,
    /// `CurrX`（oracle 恒 0）。
    pub curr_x: i16,
    /// `CurrY`（oracle 恒 0）。
    pub curr_y: i16,
    /// `ChrName`（`MapQuest.txt` 的 `sQuest`，或 `"QManage"`/`"QFunction"`/`"RobotManage"`）。
    pub chr_name: String,
    /// `NormNpc.NpcFlag`（`short`，oracle 恒 0）。
    pub npc_flag: i16,
    /// `BaseObject.Appr`（`ushort`，oracle 恒 0）。
    pub appr: u16,
    /// `NormNpc.FilePath`（`"MapQuest_def"`/`"Market_Def"`/`"Robot_def"`）。
    pub file_path: String,
    /// `Merchant.ScriptName`（仅 `QFunction` 设 `"QFunction"`，其余为 `null`）。
    pub script_name: Option<String>,
    /// `NormNpc.IsHide`（oracle 恒 `true`）。
    pub is_hide: bool,
    /// `NormNpc.IsQuest`（oracle 恒 `false`）。
    pub is_quest: bool,
}

impl QuestNpcRecord {
    /// `MapQuestManager.CreateQuest`（`:40-51`）里的 `new Merchant{…}` 形态。
    fn new_quest_npc(chr_name: String) -> Self {
        QuestNpcRecord {
            map_name: "0".to_string(),
            curr_x: 0,
            curr_y: 0,
            chr_name,
            npc_flag: 0,
            appr: 0,
            file_path: "MapQuest_def".to_string(),
            script_name: None,
            is_hide: true,
            is_quest: false,
        }
    }
}

/// `src/OpenMir2/Data/MapQuestInfo.cs`（注意**没有** `Quest` 字段）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapQuestInfo {
    pub flag: i32,
    pub value: i32,
    pub mon_name: String,
    pub item_name: String,
    pub grouped: bool,
    /// 对应的隐藏 `Merchant`（`mapQuest.NPC = questMerchant`，`:53`）。
    pub npc: QuestNpcRecord,
}

/// `MapQuestManager.CreateQuest`（`:10-55`）的 1:1 移植。
///
/// 返回 `None` 对应该函数 `return false`（仅 `nFlag < 0`）；`Some` 对应 `return true`
/// （且附带已创建的 NPC 记录，供 `AddQuestNpc` 入 `QuestNpcList`）。
#[must_use]
pub fn create_quest(
    n_flag: i32,
    mut n_value: i32,
    s_mon_name: &str,
    s_item: &str,
    s_quest: &str,
    bo_grouped: bool,
) -> Option<MapQuestInfo> {
    // `:12-15`
    if n_flag < 0 {
        return None;
    }
    // `:20-23`（只钳上界）
    if n_value > 1 {
        n_value = 1;
    }
    // `:25-29`
    let mon_name = if s_mon_name == "*" {
        String::new()
    } else {
        s_mon_name.to_string()
    };
    // `:30-34`
    let item_name = if s_item == "*" {
        String::new()
    } else {
        s_item.to_string()
    };
    // `:35-38`（先清空，再当 NPC 名 —— oracle 缺陷 ②）
    let quest = if s_quest == "*" {
        String::new()
    } else {
        s_quest.to_string()
    };
    Some(MapQuestInfo {
        flag: n_flag,
        value: n_value,
        mon_name,
        item_name,
        grouped: bo_grouped,
        npc: QuestNpcRecord::new_quest_npc(quest),
    })
}

/// `LoadMapQuest` 内层循环对**一行**的判定结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapQuestLine {
    /// 空行 / `;` 注释 ⇒ oracle `if` 不成立，`result` 不变。
    Skipped,
    /// `CreateQuest` 返回 true（或等价成功）。
    Created(Box<MapQuestInfo>),
    /// 入 `else` / `CreateQuest` 返回 false ⇒ oracle `result = -i`。
    Failed,
}

/// `ArrestStringEx(s, after, before, ref s)` 的写回语义（仅 `Value` 时改写）。
fn arrest_in_place(write: ArrestWrite, target: &mut String) {
    if let ArrestWrite::Value(v) = write {
        *target = v;
    }
}

/// `LocalDB.LoadMapQuest` 内层循环对一行的 1:1 移植（`LocalDB.cs:356-399`）。
///
/// `map_exists` 对应 oracle `SystemShare.MapMgr.FindMap(sMap) != null`
/// （部署态等于「MapInfo.txt 里加载成功过该图」）。
///
/// # Errors
/// 目前 `GetValidStr3` 路径不抛异常；保留 `Result` 以便未来 oracle 语义扩展。
pub fn parse_map_quest_line(
    line: &str,
    map_exists: impl Fn(&str) -> bool,
) -> Result<MapQuestLine, EnvirError> {
    // `:357`
    if line.is_empty() || line.starts_with(';') {
        return Ok(MapQuestLine::Skipped);
    }

    // `:362-376`
    let (s_map, rest) = get_valid_str3(line, &TEXT_SPLIT_CONST);
    let (s1c, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let (s20, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let (mut s_mon_name, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    // `:366-369`
    if !s_mon_name.is_empty() && s_mon_name.starts_with('"') {
        let (w, _) = arrest_string_ex_ref(&s_mon_name, '"', '"');
        arrest_in_place(w, &mut s_mon_name);
    }
    let (mut s_item, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    // `:371-374`
    if !s_item.is_empty() && s_item.starts_with('"') {
        let (w, _) = arrest_string_ex_ref(&s_item, '"', '"');
        arrest_in_place(w, &mut s_item);
    }
    let (s_quest, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let (s30, _rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);

    // `:377`：三列非空才继续；否则 `else { result = -i }`。
    if s_map.is_empty() || s_mon_name.is_empty() || s_quest.is_empty() {
        return Ok(MapQuestLine::Failed);
    }
    // `:379`：地图找不到 ⇒ `else { result = -i }`。
    if !map_exists(&s_map) {
        return Ok(MapQuestLine::Failed);
    }

    // `:381-385`
    let mut s34 = String::new();
    let (w, _) = arrest_string_ex_ref(&s1c, '[', ']');
    arrest_in_place(w, &mut s34);
    let n38 = str_to_int(&s34, 0);
    let n3c = str_to_int(&s20, 0);
    // `:386`
    let bo_grouped = compare_lstr(&s30, "GROUP");

    // `:387`：`CreateQuest` true ⇒ 入 `QuestList`（+ `AddQuestNpc`）；false ⇒ `result = -i`。
    match create_quest(n38, n3c, &s_mon_name, &s_item, &s_quest, bo_grouped) {
        Some(info) => Ok(MapQuestLine::Created(Box::new(info))),
        None => Ok(MapQuestLine::Failed),
    }
}

/// 隐藏脚本 NPC 缺失时 oracle 会写入的模板行（纯数据；本模块**不落盘**）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HiddenFileCreation {
    pub path: PathBuf,
    /// oracle `StringList.SaveToFile` 的逻辑行（不含行尾符）。
    pub lines: Vec<String>,
}

/// `LocalDB.LoadMapQuest`（`:341`）的聚合产物。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapQuestLoad {
    /// `QuestList`（`MapQuestManager.cs:54` 顺序）。
    pub quests: Vec<MapQuestInfo>,
    /// 隐藏任务 NPC 记录（含 `MapQuest.txt` 的 `sQuest` NPC + `QManage` + `RobotManage`，
    /// 按 `AddQuestNpc` 调用先后）。
    pub quest_npcs: Vec<QuestNpcRecord>,
    /// `QFunction` 记录（走 `AddMerchant`）。
    pub merchants: Vec<QuestNpcRecord>,
    /// **本仓增量（构造段 fold 用，`openmir2-ctor-draws-fold`）**：三个隐藏脚本 NPC 中
    /// **文件或目录存在**（`present`）的那些，按 [`HIDDEN_NPCS`] 的 oracle 次序
    /// （`QManage` → `QFunction` → `RobotManage`）列出。
    ///
    /// 与 [`Self::quest_npcs`]/[`Self::merchants`] 是**同一批记录的重排**（不新增语义）：
    /// 后两者按「进哪个列表」分桶，本字段按「`LocalDB.cs:403-405` 的调用次序」列出 —— 构造段
    /// 账本要的正是后者（每只 `new Merchant` 各 4 抽，`LocalDB.cs:268/210/312`）。
    pub hidden_npcs: Vec<QuestNpcRecord>,
    /// oracle `LoadMapQuest` 的返回值（1，或最后一条失败行的 `-i`）。
    pub result: i32,
    /// oracle 会 `SaveToFile` 的模板文件（本模块不写盘）。
    pub hidden_files_created: Vec<HiddenFileCreation>,
}

/// 隐藏脚本 NPC 规格（三个 `*Npc()` 函数）。
struct HiddenNpcSpec {
    dir: &'static str,
    file: &'static str,
    chr_name: &'static str,
    file_path: &'static str,
    script_name: Option<&'static str>,
    target: HiddenNpcTarget,
    template: &'static [&'static str],
}

/// `LocalDB.cs:236`（`QMangeNpc`）/ `:192`（`QFunctionNpc`）/ `:293`（`RobotNpc`）。
const HIDDEN_NPCS: [HiddenNpcSpec; 3] = [
    HiddenNpcSpec {
        dir: "MapQuest_def",
        file: "QManage.txt",
        chr_name: "QManage",
        file_path: "MapQuest_def",
        script_name: None,
        target: HiddenNpcTarget::QuestNpcList,
        // `LocalDB.cs:240-252`
        template: &[
            ";此脚为登录脚本，人物每次登录时都会执行此脚本，所有人物初始设置都可以放在此脚本中。",
            ";修改脚本内容，可用@ReloadManage命令重新加载该脚本，不须重启程序。",
            "[@Login]",
            "#if",
            "#act",
            ";设置10倍杀怪经验",
            ";CANGETEXP 1 10",
            "#say",
            "游戏登录脚本运行成功，欢迎进入本游戏!!!\\ \\",
            "<关闭/@exit> \\ \\",
            "登录脚本文件位于: \\",
            "@SCRIPT_PATH@\\",
            "脚本内容请自行按自己的要求修改。",
        ],
    },
    HiddenNpcSpec {
        dir: "Market_Def",
        file: "QFunction-0.txt",
        chr_name: "QFunction",
        file_path: "Market_Def",
        script_name: Some("QFunction"),
        target: HiddenNpcTarget::MerchantList,
        // `LocalDB.cs:197`
        template: &[";此脚为功能脚本，用于实现各种与脚本有关的功能"],
    },
    HiddenNpcSpec {
        dir: "Robot_def",
        file: "RobotManage.txt",
        chr_name: "RobotManage",
        file_path: "Robot_def",
        script_name: None,
        target: HiddenNpcTarget::QuestNpcList,
        // `LocalDB.cs:300`
        template: &[";此脚为机器人专用脚本，用于机器人处理功能用的脚本。"],
    },
];

impl MapLoader {
    /// `M2Share.GetEnvirFilePath("MapQuest.txt")`（`LocalDB.cs:352`）。
    #[must_use]
    pub fn map_quest_path(&self) -> PathBuf {
        envir_file_path(&self.base_path, &self.envir_dir, "", "MapQuest.txt")
    }

    /// `M2Share.GetEnvirFilePath("MapQuest_def")`（目录）。
    #[must_use]
    pub fn map_quest_def_dir(&self) -> PathBuf {
        envir_file_path(&self.base_path, &self.envir_dir, "MapQuest_def", "")
    }

    /// `LocalDB.LoadMapQuest`（`:341-405`）的 1:1 移植（含隐藏 NPC）。
    ///
    /// 与 oracle 一致：**无论 `MapQuest.txt` 是否存在都会走完隐藏 NPC 段**并返回 `result`
    /// （缺失 ⇒ 空 `QuestList`、`result = 1`）。
    ///
    /// `map_exists` 对应 `MapMgr.FindMap(sMap) != null`。
    ///
    /// # Errors
    /// 读取 `MapQuest.txt` 的 I/O 错误（非 `NotFound`）。
    pub fn load_map_quest(
        &self,
        map_exists: impl Fn(&str) -> bool,
    ) -> Result<MapQuestLoad, EnvirError> {
        let path = self.map_quest_path();
        let mut lines: Vec<String> = Vec::new();
        match std::fs::read(&path) {
            Ok(bytes) => {
                // `StringList.LoadFromFile` 的 `ReadLine` 口径（同 `mongen`）。
                lines.extend(decode_text(&bytes).lines().map(str::to_string));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(EnvirError::Io { path, source }),
        }

        let mut result: i32 = 1;
        let mut quests: Vec<MapQuestInfo> = Vec::new();
        let mut quest_npcs: Vec<QuestNpcRecord> = Vec::new();
        let mut merchants: Vec<QuestNpcRecord> = Vec::new();
        let mut hidden_npcs: Vec<QuestNpcRecord> = Vec::new();
        let mut hidden_files_created: Vec<HiddenFileCreation> = Vec::new();

        // `:358-401`
        for (i, line) in lines.iter().enumerate() {
            match parse_map_quest_line(line, &map_exists)? {
                MapQuestLine::Skipped => {}
                MapQuestLine::Created(info) => {
                    // `:52` `AddQuestNpc(questMerchant)`
                    quest_npcs.push(info.npc.clone());
                    quests.push(*info);
                }
                // `:389/397/401`：`result = -i`
                MapQuestLine::Failed => result = -(i as i32),
            }
        }

        // `:403-405`：`QMangeNpc(); QFunctionNpc(); RobotNpc();`
        for spec in &HIDDEN_NPCS {
            let file = envir_file_path(&self.base_path, &self.envir_dir, spec.dir, spec.file);
            let dir = envir_file_path(&self.base_path, &self.envir_dir, spec.dir, "");
            let present = file.exists() || dir.exists();
            if !file.exists() {
                // oracle：目录/文件缺失 ⇒ 建目录 + 写模板 ⇒ `File.Exists` 为真。
                // 本模块**冻结数据目录**：只登记「oracle 会写什么」，不落盘。
                hidden_files_created.push(HiddenFileCreation {
                    path: file.clone(),
                    lines: spec
                        .template
                        .iter()
                        .map(|l| {
                            if *l == "@SCRIPT_PATH@\\" {
                                // `LocalDB.cs:245`：`HUtil32.ReplaceChar(sScriptFile, '\\', '/')` + `'\\'`
                                format!("{}\\", file.display().to_string().replace('\\', "/"))
                            } else {
                                (*l).to_string()
                            }
                        })
                        .collect(),
                });
            }
            // 仅在异常路径（目录不可创建/不可写）oracle 才置 null；部署件三目录都存在 ⇒ 恒 present。
            if present {
                let record = QuestNpcRecord {
                    map_name: "0".to_string(),
                    curr_x: 0,
                    curr_y: 0,
                    chr_name: spec.chr_name.to_string(),
                    npc_flag: 0,
                    appr: 0,
                    file_path: spec.file_path.to_string(),
                    script_name: spec.script_name.map(str::to_string),
                    is_hide: true,
                    is_quest: false,
                };
                match spec.target {
                    HiddenNpcTarget::QuestNpcList => quest_npcs.push(record.clone()),
                    HiddenNpcTarget::MerchantList => merchants.push(record.clone()),
                }
                // 构造段次序（`LocalDB.cs:403-405`：`QMangeNpc(); QFunctionNpc(); RobotNpc();`）
                // = `HIDDEN_NPCS` 的数组序 ⇒ 本 vec 的次序即 oracle 的 `new` 次序。
                hidden_npcs.push(record);
            }
        }

        Ok(MapQuestLoad {
            quests,
            quest_npcs,
            merchants,
            hidden_npcs,
            result,
            hidden_files_created,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn any_map(_: &str) -> bool {
        true
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mir2-mapquest-test-{}-{}-{n}",
            std::process::id(),
            tag
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("清旧临时目录");
        }
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    fn write_gbk(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let (bytes, _, _) = encoding_rs::GBK.encode(text);
        std::fs::write(path, bytes.as_ref()).unwrap();
    }

    /// 部署件首行形态（`E:/MirServer/M2GameSvr/Envir/MapQuest.txt`，空格分隔）。
    const DEPLOYED_LINE: &str = "Q001     [58]     0      骷髅精灵5     *               Q002";

    #[test]
    fn parses_deployed_line() {
        let got = parse_map_quest_line(DEPLOYED_LINE, any_map).unwrap();
        let MapQuestLine::Created(info) = got else {
            panic!("部署首行应成功建任务");
        };
        assert_eq!(info.flag, 58);
        assert_eq!(info.value, 0);
        assert_eq!(info.mon_name, "骷髅精灵5");
        assert_eq!(info.item_name, "", "sItem == \"*\" ⇒ ItemName 清空");
        assert!(!info.grouped);
        assert_eq!(info.npc.chr_name, "Q002");
        assert_eq!(info.npc.file_path, "MapQuest_def");
        assert!(info.npc.is_hide && !info.npc.is_quest);
        assert_eq!(info.npc.map_name, "0");
    }

    #[test]
    fn skips_blank_and_comment() {
        assert_eq!(
            parse_map_quest_line("", any_map).unwrap(),
            MapQuestLine::Skipped
        );
        assert_eq!(
            parse_map_quest_line(";注释", any_map).unwrap(),
            MapQuestLine::Skipped
        );
        assert_eq!(
            parse_map_quest_line("   ", any_map).unwrap(),
            MapQuestLine::Failed,
            "全空格行非空 ⇒ 不进短路，三列校验失败"
        );
    }

    #[test]
    fn fails_when_map_missing() {
        let got = parse_map_quest_line(DEPLOYED_LINE, |_| false).unwrap();
        assert_eq!(got, MapQuestLine::Failed);
    }

    #[test]
    fn fails_when_required_column_empty() {
        // 缺 sQuest（只有 5 列）。
        let got = parse_map_quest_line("Q001 [58] 0 稻草人 *", any_map).unwrap();
        assert_eq!(got, MapQuestLine::Failed);
        // 缺 sMonName（第 4 列直接是 sItem）。
        let got = parse_map_quest_line("Q001 [58] 0 * Q002", any_map).unwrap();
        assert_eq!(got, MapQuestLine::Failed);
    }

    #[test]
    fn value_clamped_and_negative_flag_fails() {
        // nValue 2 → 1
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 [58] 2 骷髅精灵5 * Q002", any_map).unwrap()
        else {
            panic!()
        };
        assert_eq!(info.value, 1);
        // nValue 为负不钳
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 [58] -7 骷髅精灵5 * Q002", any_map).unwrap()
        else {
            panic!()
        };
        assert_eq!(info.value, -7);
        // nFlag 负 → CreateQuest false → Failed
        let got = parse_map_quest_line("Q001 [-1] 0 骷髅精灵5 * Q002", any_map).unwrap();
        assert_eq!(got, MapQuestLine::Failed);
    }

    #[test]
    fn flag_arrest_fallback_when_brackets_absent() {
        // s1C 不含 '[' ⇒ ArrestStringEx 写回空串 ⇒ flag = StrToInt("", 0) = 0
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 58 0 骷髅精灵5 * Q002", any_map).unwrap()
        else {
            panic!()
        };
        assert_eq!(info.flag, 0);
    }

    #[test]
    fn group_token_is_case_insensitive_prefix() {
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 [58] 0 骷髅精灵5 * Q002 group", any_map).unwrap()
        else {
            panic!()
        };
        assert!(info.grouped, "CompareLStr 大小写不敏感、按 targ 长度前缀比");
        // 前缀比："GROUPX" 也算命中（CompareLStr 只比前 5 字符）
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 [58] 0 骷髅精灵5 * Q002 GROUPX", any_map).unwrap()
        else {
            panic!()
        };
        assert!(info.grouped);
    }

    #[test]
    fn quest_wildcard_clears_npc_name() {
        // sQuest == "*" ⇒ 清空后再当 ChrName（oracle 缺陷 ②）
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 [58] 0 骷髅精灵5 * *", any_map).unwrap()
        else {
            panic!()
        };
        assert_eq!(info.npc.chr_name, "");
    }

    #[test]
    fn quoted_mon_and_item_are_unarrested() {
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 [58] 0 \"骷髅精灵5\" \"金创药(小量)\" Q002", any_map)
                .unwrap()
        else {
            panic!()
        };
        assert_eq!(info.mon_name, "骷髅精灵5");
        assert_eq!(info.item_name, "金创药(小量)");
    }

    #[test]
    fn unterminated_quote_is_left_unchanged() {
        // `"骷髅` 无闭合引号：ArrestStringEx findData=true 但找不到 before ⇒ **不改写**
        // （返回 `after+span`，ref 参数保持原样）⇒ mon_name 仍带前导引号。
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001 [58] 0 \"骷髅 精灵5\" * Q002", any_map).unwrap()
        else {
            panic!()
        };
        assert_eq!(info.mon_name, "\"骷髅");
    }

    #[test]
    fn tab_separated_line_is_split() {
        let MapQuestLine::Created(info) =
            parse_map_quest_line("Q001\t[58]\t0\t骷髅精灵5\t*\tQ002\tGROUP", any_map).unwrap()
        else {
            panic!()
        };
        assert_eq!(info.flag, 58);
        assert!(info.grouped);
    }

    #[test]
    fn create_quest_flag_zero_is_allowed() {
        let info = create_quest(0, 0, "*", "*", "Q002", false).unwrap();
        assert_eq!(info.flag, 0);
        assert_eq!(info.mon_name, "");
        assert_eq!(info.item_name, "");
        assert_eq!(info.npc.chr_name, "Q002");
    }

    #[test]
    fn load_result_is_negative_of_last_failed_index() {
        let base = temp_dir("result");
        let envir = base.join("Envir");
        // 行 0 正常、行 1 因地图缺失失败、行 2 注释、行 3 缺列失败 ⇒ result = -3
        write_gbk(
            &envir.join("MapQuest.txt"),
            "Q001 [58] 0 骷髅精灵5 * Q002\nQ404 [1] 0 怪 * QX\n;注释\nQ001 [1] 0\n",
        );
        let loader = MapLoader::new(&base, "Map", "Envir");
        let load = loader
            .load_map_quest(|name| name != "Q404")
            .expect("加载成功");
        assert_eq!(load.quests.len(), 1);
        assert_eq!(load.result, -3);
    }

    #[test]
    fn missing_map_quest_file_still_resolves_hidden_npcs() {
        let base = temp_dir("nofile");
        let envir = base.join("Envir");
        // 三个隐藏 NPC 目录都存在（Robot_def 空目录，业务件缺失）
        std::fs::create_dir_all(envir.join("MapQuest_def")).unwrap();
        std::fs::create_dir_all(envir.join("Market_Def")).unwrap();
        std::fs::create_dir_all(envir.join("Robot_def")).unwrap();
        write_gbk(&envir.join("MapQuest_def/QManage.txt"), "[@Login]\n");
        write_gbk(
            &envir.join("Market_Def/QFunction-0.txt"),
            "[@PlayLevelUp]\n",
        );
        let loader = MapLoader::new(&base, "Map", "Envir");
        let load = loader.load_map_quest(any_map).unwrap();
        assert_eq!(load.result, 1);
        assert!(load.quests.is_empty());
        // 顺序：QManage 先于 RobotManage（同属 QuestNpcList）
        assert_eq!(load.quest_npcs.len(), 2);
        assert_eq!(load.quest_npcs[0].chr_name, "QManage");
        assert_eq!(load.quest_npcs[1].chr_name, "RobotManage");
        assert_eq!(load.merchants.len(), 1);
        assert_eq!(load.merchants[0].chr_name, "QFunction");
        assert_eq!(load.merchants[0].script_name.as_deref(), Some("QFunction"));
        // RobotManage.txt 缺失 ⇒ 登记 oracle 会写的模板（但不落盘）
        assert_eq!(load.hidden_files_created.len(), 1);
        assert!(load.hidden_files_created[0]
            .path
            .ends_with("Robot_def/RobotManage.txt"));
        assert_eq!(
            load.hidden_files_created[0].lines,
            vec![";此脚为机器人专用脚本，用于机器人处理功能用的脚本。".to_string()]
        );
        assert!(
            !envir.join("Robot_def/RobotManage.txt").exists(),
            "不得落盘"
        );
    }

    #[test]
    fn hidden_npc_created_file_is_not_written_qmanage_template() {
        let base = temp_dir("template");
        let envir = base.join("Envir");
        std::fs::create_dir_all(envir.join("MapQuest_def")).unwrap();
        // 目录存在、文件缺失 ⇒ oracle 会写 QManage 模板
        let loader = MapLoader::new(&base, "Map", "Envir");
        let load = loader.load_map_quest(any_map).unwrap();
        let created = &load.hidden_files_created[0];
        assert!(created.path.ends_with("MapQuest_def/QManage.txt"));
        assert_eq!(created.lines[2], "[@Login]");
        assert_eq!(
            created.lines[8],
            "游戏登录脚本运行成功，欢迎进入本游戏!!!\\ \\"
        );
        assert!(!envir.join("MapQuest_def/QManage.txt").exists());
    }
}
