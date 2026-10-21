//! GameSvr 启动期「名单类」Envir 文本文件的**解析 + 注册表 + 装载**（offline 可验）。
//!
//! 依据：`docs/移植计划重审-20261015.md` §1 红名单 R0-5..R0-9 + UnbindList 升卡
//! （卡 `openmir2-envir-redlist-batch`）。oracle 锚（行号均在本仓库 `src/` 内实测）：
//!
//! | 文件 | oracle 加载点 | `GameApp` 装载点 | oracle 消费点 |
//! | --- | --- | --- | --- |
//! | `Envir/ItemNameList.txt` | `CustomItemSystem.LoadCustomItemName`（`M2Server/Items/CustomItemSystem.cs:80-112`） | `GameApp.cs:166` | `CustomItemSystem.GetItemName`（`:119-125`）→ 商人改名 `Merchant.cs:1785-1797` |
//! | `Envir/DisableSendMsgList.txt` | `M2Share.LoadDisableSendMsgList`（`M2Server/M2Share.cs:1355-1374`，**函数体整段注释**） | `GameApp.cs:167` | `PlayObject.cs:1448`（`FilterSendMsg`，getter 亦注释） |
//! | `Envir/DenyChrNameList.txt` | `M2Share.LoadDenyChrNameList`（`M2Share.cs:1626-1652`，**函数体整段注释**） | `GameApp.cs:108` | `PlayObject.Base.cs:3400`（拒登，getter 亦注释） |
//! | `Envir/GameLogItemNameList.txt` | `M2Share.LoadGameLogItemNameList`（`M2Share.cs:1512-1536`） | `GameApp.cs:105` | `MySqlDB.cs:87-90`（`GameLogGold`/`GameLogGameGold`/`GameLogGamePoint`） |
//! | `Envir/AdminList.txt` | `LocalDb.LoadAdminList`（`GameSrv/DB/LocalDB.cs:16-97`） | `GameApp.cs:209` | `WorldServer.GetHumPermission`（`GameSrv/Word/WorldServer.cs:1021-1039`）GM 判定 |
//! | `Envir/Robot.txt` | `RobotManage.LoadRobot`（`GameSrv/Robots/RobotManage.cs:21-54`） | `GameApp.cs:58`（`new RobotManage()`） | `AutoBot` 脚本宿主（未移植） |
//! | `Envir/UnbindList.txt` | `LocalDb.LoadUnbindList`（`DB/LocalDB.cs:872-915`） | `GameApp.cs:174` | `CM_EAT` StdMode 31 解包（`bins/game-svr/src/useitem.rs`，白名单 T-17） |
//!
//! ## 编码 / 行切分（与 `StringList.LoadFromFile` 同口径）
//!
//! oracle 全部经 `StringList.LoadFromFile`（`src/OpenMir2/Common/StringList.cs:276-291`）读入：
//! 默认 **gb2312**（有 BOM 时切 UTF-8/UTF-16，见 `:293-360`），逐行 `StreamReader.ReadLine()`。
//! 解码走 [`mir2_encoding::decode_string_list`]（BOM 感知：UTF-8 / UTF-16LE / UTF-16BE /
//! UTF-32BE；无 BOM ⇒ .NET cp936）。行切分用 `str::lines()`（剥 `\n` 与其前的 `\r`，
//! 与 `ReadLine` 对 CRLF/LF 等价）。
//!
//! ## 刻意偏离（登记，非"顺手修"）
//!
//! 1. **不实现「文件不存在 ⇒ `SaveToFile` 建空文件」写回**：`LoadCustomItemName`（`:107`）、
//!    `LoadGameLogItemNameList`（`:1534`）在缺文件时会 `SaveToFile` 一个空文件；本层只读
//!    （内容冻结约束，与 `monitems.rs`/`exps.rs` 同口径），缺文件一律返回 `Ok(None)`，不落盘。
//! 2. **`DisableSendMsgList` / `DenyChrNameList` 的装载/查询层照搬 oracle 的「无操作」**：
//!    oracle 主动代码里 `Load*`（`M2Share.cs:1357-1374`/`:1626-1652`）与 `Get*`
//!    （`M2Share.cs:1498-1510`/`:1654-1666`）函数体**整段被注释**，实际恒返回 `false`、不读文件、
//!    不入表。本片 `load_*_file` / `get_*` 逐字照搬该**无操作**行为（BUG-REPLICA，见各函数文档与
//!    测试 `*_load_is_faithful_noop`）；另提供 `parse_*`（oracle 注释掉的**意图语义**）供未来接线，
//!    但**不接主动路径**——要让它们真生效（拒登/禁言）须另开偏差卡（`D-*`）并接线登录/发言支。
//! 3. **`CaptureString` 越界照搬为上抛 `Err`**：`AdminList`/`UnbindList` 走 `GetValidStrCap`，
//!    行内出现前置 `"` 时 oracle 抛 `IndexOutOfRangeException`（`LoadAdminList`/`LoadUnbindList`
//!    调用点均无 try/catch ⇒ 启动期 fail-closed）。本层以 [`ListsError::CaptureString`] 显式上抛。
//! 4. **oracle 的 `\r`（单独 CR，非 CRLF）行切分未覆盖**：`ReadLine` 把单独 `\r` 也当换行，
//!    `str::lines()` 不会。部署件均为 CRLF/LF，差异不可达（登记）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mir2_shared::{hutil, hutil32::get_valid_str_cap};

/// `LocalDB.TextSplitConst`（`DB/LocalDB.cs:12`）= `{' ', '\t'}`。
const TEXT_SPLIT_CONST: [char; 2] = [' ', '\t'];
/// `LocalDB.MonsterSpitConst`（`DB/LocalDB.cs:13`）= `{' ', '/', '\t'}`（`Robot.txt` 用）。
const MONSTER_SPIT_CONST: [char; 3] = [' ', '/', '\t'];
/// `LocalDB.divider`（`DB/LocalDB.cs:14`）= `{"/", "\\", " ", "\t"}`（`AdminList` 用；
/// 每个元素取首字符 ⇒ 等价于 '/'、反斜杠、空格、制表符四者）。
const ADMIN_DIVIDER: [char; 4] = ['/', 0x5C_u8 as char, ' ', '\t'];

/// 装载/解析错误。
#[derive(Debug, thiserror::Error)]
pub enum ListsError {
    /// 文件存在（`File.Exists` 真）但读取失败（权限/句柄/IO）。
    #[error("读取 {path} 失败: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// 行内前置 `"` 触发 `HUtil32.CaptureString` 越界（对应 C# `IndexOutOfRangeException`）。
    #[error("名单文件行解析越界（CaptureString，对应 C# IndexOutOfRangeException）：{line}")]
    CaptureString { line: String },
}

// ===========================================================================
// 文件名常量（`M2Share.GetEnvirFilePath("…")` 的实参，逐字照搬大小写）
// ===========================================================================

/// `LoadCustomItemName` 的文件名（`CustomItemSystem.cs:85`）。
pub const ITEM_NAME_LIST_FILE: &str = "ItemNameList.txt";
/// `LoadDisableSendMsgList` 的文件名（`M2Share.cs:1359`）。
pub const DISABLE_SEND_MSG_LIST_FILE: &str = "DisableSendMsgList.txt";
/// `LoadDenyChrNameList` 的文件名（`M2Share.cs:1629`）。
pub const DENY_CHR_NAME_LIST_FILE: &str = "DenyChrNameList.txt";
/// `LoadGameLogItemNameList` 的文件名（`M2Share.cs:1515`）。
pub const GAME_LOG_ITEM_NAME_LIST_FILE: &str = "GameLogItemNameList.txt";
/// `LoadAdminList` 的文件名（`DB/LocalDB.cs:21`，注意 oracle 用的是**大写 A**）。
pub const ADMIN_LIST_FILE: &str = "AdminList.txt";
/// `LoadRobot` 的文件名（`Robots/RobotManage.cs:25`）。
pub const ROBOT_FILE: &str = "Robot.txt";
/// `LoadUnbindList` 的文件名（`DB/LocalDB.cs:879`）。
pub const UNBIND_LIST_FILE: &str = "UnbindList.txt";

// ===========================================================================
// 1. ItemNameList.txt —— `CustomItemSystem.LoadCustomItemName`（:80-112）
// ===========================================================================

/// `src/OpenMir2/Data/ItemName.cs` 的 `ItemName`（`nMakeIndex`/`nItemIndex`/`sItemName`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemName {
    pub make_index: i32,
    pub item_index: i32,
    pub item_name: String,
}

/// `LoadCustomItemName`（`CustomItemSystem.cs:80-112`）的解析循环。
///
/// 逐行：`line.Trim()` → 三次 `GetValidStr3({' ', '\t'})`（`sMakeIndex`/`sItemIndex`/`sItemName`）
/// → `StrToInt(…, -1)`；`nMakeIndex < 0 || nItemIndex < 0` 跳过。**无**空行/`;` 跳过（靠整型解析
/// 失败自然落空），**不**去重。
#[must_use]
pub fn parse_item_name_list(text: &str) -> Vec<ItemName> {
    let mut list = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        let (make_s, rest) = hutil::get_valid_str3(line, &TEXT_SPLIT_CONST);
        let (item_s, rest) = hutil::get_valid_str3(&rest, &TEXT_SPLIT_CONST);
        let (name_s, _) = hutil::get_valid_str3(&rest, &TEXT_SPLIT_CONST);
        let make_index = hutil::str_to_int(&make_s, -1);
        let item_index = hutil::str_to_int(&item_s, -1);
        if make_index < 0 || item_index < 0 {
            continue;
        }
        list.push(ItemName {
            make_index,
            item_index,
            item_name: name_s,
        });
    }
    list
}

/// `CustomItemSystem.GetCustomItemName`（`:26-41`）：**倒序**找首个 `MakeIndex && ItemIndex`
/// 相同的条目，返回其 `sItemName`；找不到返回空串。
#[must_use]
pub fn get_custom_item_name(list: &[ItemName], make_index: i32, item_index: i32) -> String {
    for item in list.iter().rev() {
        if item.make_index == make_index && item.item_index == item_index {
            return item.item_name.clone();
        }
    }
    String::new()
}

/// `CustomItemSystem.AddCustomItemName`（`:43-62`）：已存在同 `(MakeIndex, ItemIndex)` ⇒ `false`
/// 不加；否则追加并返回 `true`。
pub fn add_custom_item_name(
    list: &mut Vec<ItemName>,
    make_index: i32,
    item_index: i32,
    item_name: &str,
) -> bool {
    if list
        .iter()
        .any(|i| i.make_index == make_index && i.item_index == item_index)
    {
        return false;
    }
    list.push(ItemName {
        make_index,
        item_index,
        item_name: item_name.to_string(),
    });
    true
}

/// `CustomItemSystem.DelCustomItemName`（`:64-75`）：删**首个**匹配的条目（oracle 删后即 `return`）。
pub fn del_custom_item_name(list: &mut Vec<ItemName>, make_index: i32, item_index: i32) -> bool {
    if let Some(pos) = list
        .iter()
        .position(|i| i.make_index == make_index && i.item_index == item_index)
    {
        list.remove(pos);
        true
    } else {
        false
    }
}

/// `LoadCustomItemName` 的文件半：`File.Exists` 门 + `StringList.LoadFromFile`（GBK）+ 解析。
///
/// `Ok(None)` = 文件不存在（oracle 走 `SaveToFile` 建空文件——本层不落盘，见模块头偏离 1）。
///
/// # Errors
/// 仅 [`ListsError::Io`]（读失败）。
pub fn load_item_name_list_file(path: &Path) -> Result<Option<Vec<ItemName>>, ListsError> {
    Ok(read_gbk_if_file(path)?.map(|t| parse_item_name_list(&t)))
}

// ===========================================================================
// 2. DisableSendMsgList.txt —— `M2Share.LoadDisableSendMsgList`（:1355-1374，注释）
// ===========================================================================

/// oracle `LoadDisableSendMsgList` **被注释掉的意图语义**（`M2Share.cs:1360-1373`）：
/// 逐行 `Trim()` 入表（**不**跳空行、**不**跳 `;`）。
///
/// **警告**：本函数**不在主动装载路径上**——oracle 主动代码把这整段注释了（见
/// [`load_disable_send_msg_list_file`]）。仅供未来接线（须先开偏差卡）。
#[must_use]
pub fn parse_disable_send_msg_list(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().to_string()).collect()
}

/// oracle `LoadDisableSendMsgList`（`M2Share.cs:1355-1374`）：**函数体整段注释**。
///
/// 主动行为 = 恒返回 `false`、**不读文件**、不动 `DisableSendMsgList`（`M2Share.cs:117` 的
/// 静态表）。本函数逐字照搬该无操作行为（BUG-REPLICA）——即便 `path` 存在也不加载。
///
/// 红检（两向牙）：把本函数改成"真读文件"（如 `read_gbk_if_file(...).is_some()`）⇒
/// [`tests::disable_send_msg_load_is_faithful_noop`] 必红。
#[must_use]
pub fn load_disable_send_msg_list_file(_path: &Path) -> bool {
    false
}

/// oracle `GetDisableSendMsgList`（`M2Share.cs:1498-1510`）：**函数体整段注释**，恒 `false`。
///
/// 消费点 `PlayObject.cs:1448`（`FilterSendMsg = M2Share.GetDisableSendMsgList(ChrName)`）⇒ oracle
/// 从不因该名单禁言。本函数照搬恒 `false`（BUG-REPLICA）。
#[must_use]
pub fn get_disable_send_msg_list(_chr_name: &str) -> bool {
    false
}

// ===========================================================================
// 3. DenyChrNameList.txt —— `M2Share.LoadDenyChrNameList`（:1626-1652，注释）
// ===========================================================================

/// oracle `LoadDenyChrNameList` **被注释掉的意图语义**（`M2Share.cs:1631-1650`）：
/// 逐行 `Trim()` 入表。
///
/// **警告**：本函数**不在主动装载路径上**（oracle 整段注释，见
/// [`load_deny_chr_name_list_file`]）。仅供未来接线（须先开偏差卡）。
#[must_use]
pub fn parse_deny_chr_name_list(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().to_string()).collect()
}

/// oracle `LoadDenyChrNameList`（`M2Share.cs:1626-1652`）：**函数体整段注释**，恒 `false`、
/// **不读文件**、不动 `DenyChrNameList`（`M2Share.cs:153`）。照搬（BUG-REPLICA）。
///
/// 红检（两向牙）：改成"真读文件" ⇒ [`tests::deny_chr_name_load_is_faithful_noop`] 必红。
#[must_use]
pub fn load_deny_chr_name_list_file(_path: &Path) -> bool {
    false
}

/// oracle `GetDenyChrNameList`（`M2Share.cs:1654-1666`）：**函数体整段注释**，恒 `false`。
///
/// 消费点 `PlayObject.Base.cs:3399-3404`（拒登）⇒ oracle 从不因该名单拒登。照搬（BUG-REPLICA）。
#[must_use]
pub fn get_deny_chr_name_list(_chr_name: &str) -> bool {
    false
}

// ===========================================================================
// 4. GameLogItemNameList.txt —— `M2Share.LoadGameLogItemNameList`（:1512-1536）
// ===========================================================================

/// `LoadGameLogItemNameList`（`M2Share.cs:1512-1536`）的解析。
///
/// 特例：**整文件恰 1 行且该行以 `*` 开头** ⇒ 列表 = `["*"]`（通配，见 [`game_logs_item`]）；
/// 否则逐行 `Trim()` 入表。文件行为空 ⇒ 空表。
#[must_use]
pub fn parse_game_log_item_name_list(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() == 1 && lines[0].starts_with('*') {
        return vec!["*".to_string()];
    }
    lines.iter().map(|l| l.trim().to_string()).collect()
}

/// `M2Share.GetGameLogItemNameList`（`:1539-1555`）的等价物（返回 `== 1` 的布尔）。
///
/// `["*"]`（通配）⇒ 任何物品名皆 `true`；否则按 `string.Compare(…, OrdinalIgnoreCase)` 找。
/// OrdinalIgnoreCase 在「ASCII + GBK 汉字（无大小写折叠）」实语料上等价于
/// [`str::eq_ignore_ascii_case`]（同 `crates/world/src/mongen.rs:377-379` 的登记口径）。
#[must_use]
pub fn game_logs_item(list: &[String], item_name: &str) -> bool {
    if list.len() == 1 && list[0].starts_with('*') {
        return true;
    }
    list.iter().any(|x| x.eq_ignore_ascii_case(item_name))
}

/// `LoadGameLogItemNameList` 的文件半。`Ok(None)` = 文件不存在（oracle 走 `SaveToFile` 建空文件
/// ——本层不落盘，模块头偏离 1）。
///
/// # Errors
/// 仅 [`ListsError::Io`]（读失败）。
pub fn load_game_log_item_name_list_file(path: &Path) -> Result<Option<Vec<String>>, ListsError> {
    Ok(read_gbk_if_file(path)?.map(|t| parse_game_log_item_name_list(&t)))
}

// ===========================================================================
// 5. AdminList.txt —— `LocalDb.LoadAdminList`（:16-97）
// ===========================================================================

/// `src/OpenMir2/Data/AdminInfo.cs` 的 `AdminInfo`（`Level`/`ChrName`/`IPaddr`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminInfo {
    pub level: u8,
    pub chr_name: String,
    pub ipaddr: String,
}

/// `LocalDb.LoadAdminList`（`DB/LocalDB.cs:16-97`）的解析循环。
///
/// 逐行：空行/首字符 `;` 跳过；首字符定级 `'*'`→10、`'1'..'9'`→9..1，落 0 或未知 ⇒ 跳过；
/// 否则三次 `GetValidStrCap({'/', '\', ' ', '\t'})`（第一个 token 丢弃）取 `ChrName`/`IPaddr`，
/// 二者任一为空 ⇒ 跳过；否则收录 `AdminInfo`。
///
/// # Errors
/// 行触发 `CaptureString`（前置 `"`）时返回 [`ListsError::CaptureString`]——oracle 调用点
/// （`GameApp.cs:209`）无 try/catch，异常穿透 ⇒ 启动期 fail-closed。
pub fn parse_admin_list(text: &str) -> Result<Vec<AdminInfo>, ListsError> {
    let mut list = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let first = line.chars().next().unwrap_or('\0');
        let level: i32 = match first {
            '*' => 10,
            '1' => 9,
            '2' => 8,
            '3' => 7,
            '4' => 6,
            '5' => 5,
            '6' => 4,
            '7' => 3,
            '8' => 2,
            '9' => 1,
            _ => -1,
        };
        if level <= 0 {
            continue;
        }
        let cap = |s: &str| -> Result<(String, String), ListsError> {
            get_valid_str_cap(s, &ADMIN_DIVIDER).map_err(|_| ListsError::CaptureString {
                line: line.to_string(),
            })
        };
        let (_data, rest) = cap(line)?;
        let (chr_name, rest) = cap(&rest)?;
        let (ipaddr, _rest) = cap(&rest)?;
        if chr_name.is_empty() || ipaddr.is_empty() {
            continue;
        }
        list.push(AdminInfo {
            level: level as u8,
            chr_name,
            ipaddr,
        });
    }
    Ok(list)
}

/// `WorldServer.GetHumPermission`（`Word/WorldServer.cs:1021-1039`）的名表查找半边：按
/// `ChrName`（OrdinalIgnoreCase）找首个匹配，返回其 [`AdminInfo`]；找不到 `None`。
#[must_use]
pub fn lookup_admin<'a>(list: &'a [AdminInfo], user_name: &str) -> Option<&'a AdminInfo> {
    list.iter()
        .find(|a| a.chr_name.eq_ignore_ascii_case(user_name))
}

/// `LoadAdminList` 的文件半。`Ok(None)` = 文件不存在（oracle 直接 `return false`，无写回）。
///
/// # Errors
/// IO 失败或行级 `CaptureString`，见 [`ListsError`]。
pub fn load_admin_list_file(path: &Path) -> Result<Option<Vec<AdminInfo>>, ListsError> {
    match read_gbk_if_file(path)? {
        Some(t) => Ok(Some(parse_admin_list(&t)?)),
        None => Ok(None),
    }
}

// ===========================================================================
// 6. Robot.txt —— `RobotManage.LoadRobot`（:21-54）
// ===========================================================================

/// `RobotManage.LoadRobot` 里 `RobotObject` 的 `ChrName`/`ScriptFileName` 两字段
/// （`Robots/RobotManage.cs:47-49`；`LoadScript()` 属脚本宿主，未移植）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RobotRecord {
    pub chr_name: String,
    pub script_file_name: String,
}

/// `LoadRobot`（`RobotManage.cs:21-54`）的解析循环。
///
/// 逐行：空行/首字符 `;` 跳过；两次 `GetValidStr3({' ', '/', '\t'})` 取 `RobotName`/`ScriptFileName`，
/// 二者任一为空 ⇒ 跳过；否则收录 `RobotRecord`。
#[must_use]
pub fn parse_robot_list(text: &str) -> Vec<RobotRecord> {
    let mut list = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (chr_name, rest) = hutil::get_valid_str3(line, &MONSTER_SPIT_CONST);
        let (script_file_name, _) = hutil::get_valid_str3(&rest, &MONSTER_SPIT_CONST);
        if chr_name.is_empty() || script_file_name.is_empty() {
            continue;
        }
        list.push(RobotRecord {
            chr_name,
            script_file_name,
        });
    }
    list
}

/// `LoadRobot` 的文件半。`Ok(None)` = 文件不存在（oracle `File.Exists` 假 ⇒ 直接 `return`）。
///
/// # Errors
/// 仅 [`ListsError::Io`]（读失败）。
pub fn load_robot_list_file(path: &Path) -> Result<Option<Vec<RobotRecord>>, ListsError> {
    Ok(read_gbk_if_file(path)?.map(|t| parse_robot_list(&t)))
}

// ===========================================================================
// 7. UnbindList.txt —— `LocalDb.LoadUnbindList`（:872-915）
// ===========================================================================

/// `LocalDb.LoadUnbindList`（`DB/LocalDB.cs:872-915`）的返回值 `(result, UnbindList)`。
///
/// - `result` = oracle 的 `int` 返回值：正常 `0`；遇到首列 `<= 0` 的行时 = `-i`（该行 **0-based**
///   索引的相反数）并 `break`（注意 `i == 0` 时 `-i == 0`，与成功不可区分——照搬）。
/// - `map` = `UnbindList`（解包物品 id → 物品名）；重复键**保留首个**（`TryAdd` 语义）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnbindListLoad {
    pub result: i32,
    pub map: BTreeMap<i32, String>,
}

/// `LoadUnbindList`（`DB/LocalDB.cs:872-915`）的解析循环。
///
/// 逐行：空行/首字符 `;` 跳过；`GetValidStr3({' ', '\t'})` 取 `sData`；`GetValidStrCap` 取
/// `sItemName`（前置 `"` 时再 `ArrestStringEx("\"","\"")`）；`n10 = StrToInt(sData, 0)`；
/// `n10 > 0` ⇒ `TryAdd(n10, sItemName)`（重复键 `Warn` + `continue`）；否则 `result = -i; break`。
///
/// # Errors
/// `GetValidStrCap` 触发 `CaptureString` 时返回 [`ListsError::CaptureString`]（oracle 调用点
/// `GameApp.cs:174` 无 try/catch）。部署件无前置 `"` ⇒ 实际不可达（登记）。
pub fn parse_unbind_list(text: &str) -> Result<UnbindListLoad, ListsError> {
    let mut out = UnbindListLoad::default();
    for (i, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (data_s, rest) = hutil::get_valid_str3(line, &TEXT_SPLIT_CONST);
        let (mut item_name, _rest) =
            get_valid_str_cap(&rest, &TEXT_SPLIT_CONST).map_err(|_| ListsError::CaptureString {
                line: line.to_string(),
            })?;
        if !item_name.is_empty() && item_name.starts_with('"') {
            // `ArrestStringEx(sItemName, "\"", "\"", ref sItemName)`：三态写回。
            let (write, _) = mir2_shared::hutil32::arrest_string_ex_ref(&item_name, '"', '"');
            if let mir2_shared::hutil32::ArrestWrite::Value(v) = write {
                item_name = v;
            }
        }
        let n10 = hutil::str_to_int(&data_s, 0);
        if n10 > 0 {
            // `TryAdd`：已存在则 `Warn` + `continue`（等价于保留首个、不覆盖）。
            out.map.entry(n10).or_insert(item_name);
        } else {
            out.result = -(i as i32);
            break;
        }
    }
    Ok(out)
}

/// `LoadUnbindList` 的文件半。`Ok(None)` = 文件不存在（oracle `File.Exists` 假 ⇒ 不读、无写回）。
///
/// # Errors
/// IO 失败或行级 `CaptureString`，见 [`ListsError`]。
pub fn load_unbind_list_file(path: &Path) -> Result<Option<UnbindListLoad>, ListsError> {
    match read_gbk_if_file(path)? {
        Some(t) => Ok(Some(parse_unbind_list(&t)?)),
        None => Ok(None),
    }
}

// ===========================================================================
// 公共文件半：File.Exists 门 + `StringList.LoadFromFile` 解码（BOM 感知）
// ===========================================================================

/// `File.Exists` 门（等价 `Path::is_file`，同名目录亦视为不存在）+ 整文件解码。
///
/// 解码口径 = oracle `StringList.LoadFromFile`（`src/OpenMir2/Common/StringList.cs:276-291`）：
/// 先 `GetEncoding` 预判 BOM，再由 `StreamReader` 探测（UTF-8 / UTF-16LE / UTF-16BE /
/// UTF-32BE；`FF FE 00 00` 是 UTF-16LE），无 BOM ⇒ cp936。统一走
/// [`mir2_encoding::decode_string_list`]（回退目标是裸 [`mir2_encoding::decode`]：无 BOM 嗅探，
/// UTF-8 BOM 会被当作 cp936 双字节解；该用例是**既有行为补测**，不是本批牙）。
///
/// `Ok(None)` = 不存在（或同名目录）；`Err(Io)` = 存在但读失败（fail-closed，不静默当空）。
fn read_gbk_if_file(path: &Path) -> Result<Option<String>, ListsError> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|e| ListsError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(Some(mir2_encoding::decode_string_list(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GBK 编码夹具（部署件真实格式）。
    fn gbk(s: &str) -> Vec<u8> {
        mir2_encoding::encode_gb2312_dotnet(s)
    }

    /// 用例级唯一临时目录（照 `monitems.rs` 的 `temp_dir` + 进程号口径）。
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mir2-lists-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // --------------------------------------------------------------------
    // 1. ItemNameList
    // --------------------------------------------------------------------

    /// 部署件第 1 行形态（`1073741869<TAB>372<TAB>1000`）+ 中文名行逐字段。
    #[test]
    fn item_name_list_parses_tab_rows() {
        let text = "1073741869\t372\t1000\n26282853\t443\t一级魔法药\n\n  ;注释行\n";
        let list = parse_item_name_list(text);
        assert_eq!(list.len(), 2, "空行与 ; 注释行解析失败被跳过");
        assert_eq!(
            list[0],
            ItemName {
                make_index: 1073741869,
                item_index: 372,
                item_name: "1000".to_string(),
            }
        );
        assert_eq!(list[1].item_name, "一级魔法药");
        assert_eq!(list[1].make_index, 26282853);
        assert_eq!(list[1].item_index, 443);
    }

    /// `nMakeIndex < 0 || nItemIndex < 0` 的行全部跳过（缺列 / 非数字 / 负值）。
    ///
    /// 红检（改坏必红）：把 `str_to_int(&item_s, -1)` 的默认值改成 `0` ⇒ `"5"`（单列）行会被
    /// 当成 `item_index == 0` 收录 ⇒ `list.is_empty()` 断言必红。
    #[test]
    fn item_name_list_skips_non_numeric_rows() {
        let text = "5\n5 x\na b c\n-1 2 name\n1 -2 name\n";
        assert!(parse_item_name_list(text).is_empty());
    }

    /// 注册表三操作与 oracle `Get/Add/Del` 的次序语义。
    #[test]
    fn custom_item_name_registry_get_add_del() {
        let mut list = vec![
            ItemName {
                make_index: 7,
                item_index: 3,
                item_name: "旧名".to_string(),
            },
            ItemName {
                make_index: 7,
                item_index: 3,
                item_name: "新名".to_string(),
            },
        ];
        // Get：倒序 → 取后加进去的"新名"。
        assert_eq!(get_custom_item_name(&list, 7, 3), "新名");
        assert_eq!(get_custom_item_name(&list, 8, 3), "");
        // Add：已有同键 ⇒ false。
        assert!(!add_custom_item_name(&mut list, 7, 3, "x"));
        assert!(add_custom_item_name(&mut list, 9, 3, "九"));
        assert_eq!(get_custom_item_name(&list, 9, 3), "九");
        // Del：删正序首个匹配（"旧名"）⇒ 剩 "新名"。
        assert!(del_custom_item_name(&mut list, 7, 3));
        assert_eq!(get_custom_item_name(&list, 7, 3), "新名");
        assert!(!del_custom_item_name(&mut list, 8, 3));
    }

    /// 文件半：缺文件 ⇒ `Ok(None)`（不落盘建空文件）；存在 ⇒ GBK 解码逐字段。
    #[test]
    fn load_item_name_list_missing_is_none_then_reads_gbk() {
        let dir = temp_dir("itemname");
        let path = dir.join(ITEM_NAME_LIST_FILE);
        assert!(load_item_name_list_file(&path).unwrap().is_none());
        std::fs::write(&path, gbk("1\t2\t裁决之杖\n")).unwrap();
        let list = load_item_name_list_file(&path).unwrap().expect("Some");
        assert_eq!(
            list,
            vec![ItemName {
                make_index: 1,
                item_index: 2,
                item_name: "裁决之杖".to_string(),
            }]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --------------------------------------------------------------------
    // 4. GameLogItemNameList
    // --------------------------------------------------------------------

    /// 单行 `*` ⇒ 通配；lookup 对任何名字皆真。
    #[test]
    fn game_log_star_line_is_wildcard() {
        let list = parse_game_log_item_name_list("*\n");
        assert_eq!(list, vec!["*".to_string()]);
        assert!(game_logs_item(&list, "任意物品"));
        assert!(game_logs_item(&list, ""));
    }

    /// 多行 ⇒ 逐行 `Trim()`；lookup 为 OrdinalIgnoreCase。
    ///
    /// 红检（改坏必红）：把 `game_logs_item` 的 `eq_ignore_ascii_case` 改成大小写敏感 `==`
    /// ⇒ `"GOLD"` 用例（表里是 `"gold"`）必红。
    #[test]
    fn game_log_multi_line_lookup_is_ordinal_ignore_case() {
        let text = "金币\n  gold  \n海魂\n";
        let list = parse_game_log_item_name_list(text);
        assert_eq!(
            list,
            vec!["金币".to_string(), "gold".to_string(), "海魂".to_string()]
        );
        assert!(game_logs_item(&list, "GOLD"));
        assert!(game_logs_item(&list, "金币"));
        assert!(!game_logs_item(&list, "银币"));
    }

    /// 缺文件 ⇒ `Ok(None)`（oracle 走 `SaveToFile` 建空文件——本层不落盘）。
    #[test]
    fn load_game_log_missing_is_none() {
        let dir = temp_dir("gamelog");
        assert!(
            load_game_log_item_name_list_file(&dir.join(GAME_LOG_ITEM_NAME_LIST_FILE))
                .unwrap()
                .is_none()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --------------------------------------------------------------------
    // 5. AdminList
    // --------------------------------------------------------------------

    /// 首字符定级 `'*'`→10 / `'1'..'9'`→9..1；`ChrName`/`IPaddr` 逐字段；空字段跳过。
    #[test]
    fn admin_list_levels_and_fields() {
        let text = ";注释\n* charlie 10.0.0.1\n1 bob 127.0.0.1\n9 zoe 1.2.3.4\n0 skipme 1.1.1.1\n";
        let list = parse_admin_list(text).unwrap();
        assert_eq!(list.len(), 3, "';' 与首字符 '0'（level<=0）的行跳过");
        assert_eq!(
            list[0],
            AdminInfo {
                level: 10,
                chr_name: "charlie".to_string(),
                ipaddr: "10.0.0.1".to_string(),
            }
        );
        assert_eq!(list[1].level, 9);
        assert_eq!(list[1].chr_name, "bob");
        assert_eq!(list[2].level, 1);
        assert_eq!(list[2].ipaddr, "1.2.3.4");
    }

    /// 部署件实况：单行 `*<TAB><名字>`（无 IP）⇒ `sIPaddr` 空 ⇒ **整行被丢弃**（`AdminList` 为空）。
    ///
    /// 红检（改坏必红）：去掉 `if chr_name.is_empty() || ipaddr.is_empty() { continue; }` ⇒
    /// 本用例 `assert!(list.is_empty())` 必红。
    #[test]
    fn admin_list_deployed_line_without_ip_is_dropped() {
        let list = parse_admin_list("*\t管理员\n").unwrap();
        assert!(list.is_empty(), "缺 IP 列 ⇒ 丢弃（LocalDB.cs:81-83）");
    }

    /// 首字符定级后行内含前置 `"` ⇒ `CaptureString` 越界 ⇒ `Err`（oracle 调用点无 catch）。
    #[test]
    fn admin_list_capture_string_is_err() {
        let err = parse_admin_list("* \"name\n").unwrap_err();
        assert!(
            matches!(err, ListsError::CaptureString { .. }),
            "得 {err:?}"
        );
    }

    /// `GetHumPermission` 名表半边：OrdinalIgnoreCase 匹配 + 未命中 `None`。
    #[test]
    fn lookup_admin_is_ordinal_ignore_case() {
        let list = vec![AdminInfo {
            level: 10,
            chr_name: "GM".to_string(),
            ipaddr: "127.0.0.1".to_string(),
        }];
        assert_eq!(lookup_admin(&list, "gm").unwrap().level, 10);
        assert!(lookup_admin(&list, "nobody").is_none());
    }

    /// 文件半：缺文件 ⇒ `Ok(None)`；存在但无有效行 ⇒ `Ok(Some(空))`。
    #[test]
    fn load_admin_list_file_gates_on_existence() {
        let dir = temp_dir("admin");
        let path = dir.join(ADMIN_LIST_FILE);
        assert!(load_admin_list_file(&path).unwrap().is_none());
        std::fs::write(&path, gbk("*\t管理员\n")).unwrap();
        assert_eq!(load_admin_list_file(&path).unwrap(), Some(Vec::new()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --------------------------------------------------------------------
    // 6. Robot.txt
    // --------------------------------------------------------------------

    /// 部署件形态：`;注释` 跳过；`系统账号 AutoRunRobot` ⇒ (名, 脚本) 两字段。
    #[test]
    fn robot_list_parses_rows() {
        let text = ";自动挂机账号 脚本文件名\n系统账号 AutoRunRobot\n";
        let list = parse_robot_list(text);
        assert_eq!(
            list,
            vec![RobotRecord {
                chr_name: "系统账号".to_string(),
                script_file_name: "AutoRunRobot".to_string(),
            }]
        );
    }

    /// 缺第二列（脚本名）的行跳过。
    ///
    /// 红检（改坏必红）：去掉 `script_file_name.is_empty()` 守卫 ⇒ `"onlyname"` 行被收录 ⇒ 红。
    #[test]
    fn robot_list_skips_missing_script_field() {
        assert!(parse_robot_list("onlyname\n").is_empty());
    }

    /// 文件半：缺文件 ⇒ `Ok(None)`。
    #[test]
    fn load_robot_missing_is_none() {
        let dir = temp_dir("robot");
        assert!(load_robot_list_file(&dir.join(ROBOT_FILE))
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --------------------------------------------------------------------
    // 7. UnbindList
    // --------------------------------------------------------------------

    /// 部署件形态：`;注释` 跳过；`id<TAB>名字` 逐字段入 map；`result == 0`。
    #[test]
    fn unbind_list_parses_and_maps() {
        let text = ";解包物品文件\n100    强效金创药\n101\t强效魔法药\n";
        let load = parse_unbind_list(text).unwrap();
        assert_eq!(load.result, 0);
        assert_eq!(load.map.len(), 2);
        assert_eq!(load.map[&100], "强效金创药");
        assert_eq!(load.map[&101], "强效魔法药");
    }

    /// 首列 `<= 0` ⇒ `result = -i` 并 `break`（余下行不再处理）。
    ///
    /// 红检（改坏必红）：把 `out.result = -(i as i32); break;` 改成 `continue` ⇒ 后续 `102` 行
    /// 会被收录 ⇒ `map` 断言必红。
    #[test]
    fn unbind_list_nonpositive_key_returns_negative_and_breaks() {
        let text = "100 甲\nabc 乙\n102 丙\n";
        let load = parse_unbind_list(text).unwrap();
        assert_eq!(load.result, -1, "第 1 行（0-based）非法 ⇒ -1");
        assert_eq!(load.map.len(), 1);
        assert_eq!(load.map[&100], "甲");
        assert!(!load.map.contains_key(&102), "break 后不再处理");
    }

    /// 重复键 ⇒ 保留首个（`TryAdd` 失败只 `Warn` + `continue`）。
    #[test]
    fn unbind_list_duplicate_key_keeps_first() {
        let load = parse_unbind_list("100 甲\n100 乙\n").unwrap();
        assert_eq!(load.map[&100], "甲");
    }

    /// 前置 `"` 且无空格 ⇒ `CaptureString` 越界 ⇒ `Err`。
    #[test]
    fn unbind_list_capture_string_is_err() {
        let err = parse_unbind_list("100 \"abc\n").unwrap_err();
        assert!(
            matches!(err, ListsError::CaptureString { .. }),
            "得 {err:?}"
        );
    }

    /// 行内前置 `"` 且**有**尾随空格 ⇒ 走 `ArrestStringEx` 三态分支（照 `<"abc" 1>` 计算）
    /// 不报错（保持首个 token 数值语义）。
    #[test]
    fn unbind_list_quoted_name_goes_through_arrest() {
        // `100 "abc" 1` 的 rest = `"abc" 1`；GetValidStrCap → CaptureString 取 `"abc`，
        // 再 ArrestStringEx(`"abc`, '"', '"') → Unchanged（无闭合）⇒ 保留 `"abc`。
        let load = parse_unbind_list("100 \"abc\" 1\n").unwrap();
        assert_eq!(load.map[&100], "\"abc");
    }

    /// 文件半：缺文件 ⇒ `Ok(None)`；空文件 ⇒ `Ok(Some({result:0, 空 map}))`。
    #[test]
    fn load_unbind_missing_is_none_and_empty_is_some() {
        let dir = temp_dir("unbind");
        let path = dir.join(UNBIND_LIST_FILE);
        assert!(load_unbind_list_file(&path).unwrap().is_none());
        std::fs::write(&path, b"").unwrap();
        assert_eq!(
            load_unbind_list_file(&path).unwrap(),
            Some(UnbindListLoad::default())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // --------------------------------------------------------------------
    // 2/3. DisableSendMsgList / DenyChrNameList
    // --------------------------------------------------------------------

    /// oracle 注释掉的意图语义：逐行 `Trim()`（不跳空行/`;`）。
    #[test]
    fn disable_send_msg_parse_trims_each_line() {
        let list = parse_disable_send_msg_list("  非常规名  \n\n;x\n");
        assert_eq!(
            list,
            vec!["非常规名".to_string(), String::new(), ";x".to_string()]
        );
    }

    /// **BUG-REPLICA 两向牙**：oracle `LoadDisableSendMsgList` 函数体整段注释 ⇒ 即便文件存在
    /// 也恒 `false`、不加载。改成"真读文件" ⇒ 本用例必红。
    #[test]
    fn disable_send_msg_load_is_faithful_noop() {
        let dir = temp_dir("disablesend");
        let path = dir.join(DISABLE_SEND_MSG_LIST_FILE);
        assert!(!load_disable_send_msg_list_file(&path), "缺文件 false");
        std::fs::write(&path, gbk("任何名字\n")).unwrap();
        assert!(
            !load_disable_send_msg_list_file(&path),
            "oracle 注释 ⇒ 有文件也 false（BUG-REPLICA）"
        );
        assert!(!get_disable_send_msg_list("任何名字"), "getter 恒 false");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// oracle 注释掉的意图语义：逐行 `Trim()`。
    #[test]
    fn deny_chr_name_parse_trims_each_line() {
        let list = parse_deny_chr_name_list("  坏人  \n\n;x\n");
        assert_eq!(
            list,
            vec!["坏人".to_string(), String::new(), ";x".to_string()]
        );
    }

    /// **BUG-REPLICA 两向牙**：oracle `LoadDenyChrNameList` 函数体整段注释 ⇒ 恒 `false`、不加载。
    #[test]
    fn deny_chr_name_load_is_faithful_noop() {
        let dir = temp_dir("denychr");
        let path = dir.join(DENY_CHR_NAME_LIST_FILE);
        assert!(!load_deny_chr_name_list_file(&path), "缺文件 false");
        std::fs::write(&path, gbk("坏人\n")).unwrap();
        assert!(
            !load_deny_chr_name_list_file(&path),
            "oracle 注释 ⇒ 有文件也 false（BUG-REPLICA）"
        );
        assert!(!get_deny_chr_name_list("坏人"), "getter 恒 false");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// **E-6 BOM 矩阵**：oracle 全部经 `StringList.LoadFromFile`（`GetEncoding` + BOM 探测）。
    /// GBK / UTF-8 BOM / UTF-16LE 三份夹具必须解出同一物品名。回退目标是裸
    /// `mir2_encoding::decode`（无 BOM 嗅探 ⇒ UTF-8 BOM 被当 cp936 双字节解，物品名乱码）。
    /// 本用例是既有行为的补测，**不是**本批（E-6 加载器统一）的牙。
    #[test]
    fn load_item_name_list_bom_matrix() {
        let text = "1\t2\t裁决之杖\n";
        let gbk = gbk(text);
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice(text.as_bytes());
        let mut utf16le = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            utf16le.extend_from_slice(&u.to_le_bytes());
        }
        let dir = temp_dir("itemname-bom");
        let path = dir.join(ITEM_NAME_LIST_FILE);
        for (name, bytes) in [
            ("gbk", gbk),
            ("utf8-bom", utf8_bom),
            ("utf16le-bom", utf16le),
        ] {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            let list = load_item_name_list_file(&path)
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .expect("Some");
            assert_eq!(
                list,
                vec![ItemName {
                    make_index: 1,
                    item_index: 2,
                    item_name: "裁决之杖".to_string(),
                }],
                "{name}"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
