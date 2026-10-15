//! `Envir/Npcs.txt` 固定管理 NPC 配置解析 —— 1:1 复刻 `src/GameSrv/DB/LocalDB.cs:655-713`
//! （`LoadNpcs`）。
//!
//! 交付卡：`openmir2-npcs-fixed-loading`（红名单 R0-1，P0 内容加载缺口；解析半）。
//! 放置半在 `mir2-rs/bins/game-svr/src/npcs.rs`。
//!
//! oracle 行为逐条（行号按 `LocalDB.cs`）：
//! - `:664` 主文件路径 `M2Share.GetEnvirFilePath("Npcs.txt")`（宿主侧文件半）；
//! - `:665-668` `File.Exists` 门 + `StringList.LoadFromFile`（GBK；宿主侧文件半）；
//! - `:671-673` 行过滤：`sData = line.Trim()`；空行与首字符 `;` 的行跳过；
//! - `:674` `GetValidStrCap(sData, ref chrName, TextSplitConst)` —— 分隔符 `{ ' ', '\t' }`
//!   （`LocalDB.cs:12`）；首字符为 `"` 时走 `HUtil32.CaptureString` 引号捕获；
//! - `:675-678` `chrName` 首字符为 `"` 时 `ArrestStringEx(chrName, "\"", "\"", ref chrName)` 去引号；
//! - `:679-684` 逐列 `GetValidStr3`：`type / mapName / cX / cY / flag / appr`（同 `{ ' ', '\t' }`）；
//! - `:685` 入库门：`chrName`、`mapName`、`appr` 三者非空（**`type`/`cX`/`cY`/`flag` 允许缺省**）；
//! - `:688-698` `switch (StrToInt(type, 0))`：`0`⇒`Merchant` / `1`⇒`GuildOfficial` /
//!   `2`⇒`CastleOfficial`；**其余值 `npc` 保持 `null` ⇒ 整行跳过（不 AddQuestNpc）**；
//! - `:700-708` 逐字段：`MapName`=mapName、`CurrX`=`StrToInt16(cX,0)`、`CurrY`=`StrToInt16(cY,0)`、
//!   `ChrName`=chrName、`NpcFlag`=`StrToInt16(flag,0)`、`Appr`=`(ushort)StrToInt(appr,0)`——
//!   随后 `SystemShare.WorldEngine.AddQuestNpc(npc)`（`:708`）→ `QuestNpcList.Add`（`WorldServer.cs:1924-1927`）；
//! - `:655-713` 整个方法**无 try/catch**：解析异常（如 `GetValidStrCap` 的 `CaptureString`
//!   越界）会穿透 `LoadNpcs` ⇒ 本模块以逐行 `Result` 承载，宿主按 oracle 语义消费。
//!
//! oracle **不**在解析层校验地图存在（`NpCinitialize` 的 `FindMap` 门在 `WorldServer.Merchant.cs:39-63`
//! 的挂图阶段）⇒ 本模块同样不过滤。

use mir2_shared::hutil32::{
    arrest_string_ex_ref, get_valid_str3, get_valid_str_cap, str_to_int, str_to_int16, ArrestWrite,
};

use crate::error::EnvirError;

/// C# `LocalDB.TextSplitConst = { ' ', '\t' }`（`LocalDB.cs:12`）。
pub const NPC_TEXT_SPLIT: [char; 2] = [' ', '\t'];

/// `LoadNpcs` 的 `switch (StrToInt(type, 0))`（`LocalDB.cs:688-698`）选出的 NPC 类别。
///
/// 三个取值恰好覆盖 oracle 的 `case 0/1/2`；其它值 oracle 让 `npc` 保持 `null` ⇒ 整行跳过
/// （不在本枚举内表达，见 [`parse_npc_line`] 返回 `None`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum NpcType {
    /// `case 0` ⇒ `new Merchant()`（`LocalDB.cs:690-692`）。
    Merchant = 0,
    /// `case 1` ⇒ `new GuildOfficial()`（`LocalDB.cs:693-695`）。
    GuildOfficial = 1,
    /// `case 2` ⇒ `new CastleOfficial()`（`LocalDB.cs:696-698`）。
    CastleOfficial = 2,
}

impl NpcType {
    /// `StrToInt(type, 0)` 后的 `switch` 派发；不在 `{0,1,2}` ⇒ `None`（oracle `npc == null`）。
    #[must_use]
    pub fn from_raw(raw: i32) -> Option<NpcType> {
        match raw {
            0 => Some(NpcType::Merchant),
            1 => Some(NpcType::GuildOfficial),
            2 => Some(NpcType::CastleOfficial),
            _ => None,
        }
    }
}

/// `Envir/Npcs.txt` 一行对应的**入库**记录（`INormNpc` 在加载层可对拍的字段子集）。
///
/// 与 oracle `Merchant`/`GuildOfficial`/`CastleOfficial` 的赋值逐条对应：
/// `ChrName`/`MapName`/`CurrX`/`CurrY`/`NpcFlag`/`Appr` + 类别（`npc_type`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcInfo {
    /// `NormNpc.ChrName`（`LocalDB.cs:705`）。
    pub chr_name: String,
    /// 类别（`switch` 分支选出的具体类）。
    pub npc_type: NpcType,
    /// `BaseObject.MapName`（`LocalDB.cs:702`）。
    pub map_name: String,
    /// `CurrX`（`StrToInt16(cX, 0)`，`LocalDB.cs:703`）。
    pub x: i16,
    /// `CurrY`（`StrToInt16(cY, 0)`，`LocalDB.cs:704`）。
    pub y: i16,
    /// `NormNpc.NpcFlag`（`StrToInt16(flag, 0)`，`LocalDB.cs:706`）。
    pub npc_flag: i16,
    /// `BaseObject.Appr`（`(ushort)StrToInt(appr, 0)`，`LocalDB.cs:707`）。
    pub appr: u16,
}

/// 解析 `Envir/Npcs.txt` 的**一行**（`LocalDB.cs:671-708`）。
///
/// 返回 `Ok(None)` 对应 oracle 不入库的行：空行 / `;` 注释 / `chrName`·`mapName`·`appr`
/// 有空的校验不过行（`:672`、`:685`）/ `type` 不在 `{0,1,2}` 的 `null` 支（`:688-698`）。
///
/// # Errors
/// `GetValidStrCap` 的 `chrName` 列走 `HUtil32.CaptureString` 的越界分支
/// （对应 C# `IndexOutOfRangeException` 穿透 `LoadNpcs`）；以 [`EnvirError::NpcsCaptureString`] 上抛。
pub fn parse_npc_line(line: &str) -> Result<Option<NpcInfo>, EnvirError> {
    // `:671` `sData = loadList[i].Trim()`。
    let trimmed = line.trim();
    // `:672` `if (!string.IsNullOrEmpty(sData) && sData[0] != ';')`。
    if trimmed.is_empty() || trimmed.starts_with(';') {
        return Ok(None);
    }

    // `:674` `GetValidStrCap(sData, ref chrName, TextSplitConst)`（分隔符 `{ ' ', '\t' }`）。
    let (mut chr_name, rest) =
        get_valid_str_cap(trimmed, &NPC_TEXT_SPLIT).map_err(|_| EnvirError::NpcsCaptureString {
            line: line.to_string(),
        })?;
    // `:675-678` 首字符为 `"` ⇒ `ArrestStringEx(chrName, "\"", "\"", ref chrName)`（三态写回）。
    if !chr_name.is_empty() && chr_name.starts_with('"') {
        if let (ArrestWrite::Value(v), _) = arrest_string_ex_ref(&chr_name, '"', '"') {
            chr_name = v;
        }
    }

    // `:679-684` 逐列 `GetValidStr3`。
    let (s_type, rest) = get_valid_str3(&rest, &NPC_TEXT_SPLIT);
    let (map_name, rest) = get_valid_str3(&rest, &NPC_TEXT_SPLIT);
    let (c_x, rest) = get_valid_str3(&rest, &NPC_TEXT_SPLIT);
    let (c_y, rest) = get_valid_str3(&rest, &NPC_TEXT_SPLIT);
    let (flag, rest) = get_valid_str3(&rest, &NPC_TEXT_SPLIT);
    let (appr, _) = get_valid_str3(&rest, &NPC_TEXT_SPLIT);

    // `:685` 入库门：chrName/mapName/appr 三者非空。
    if chr_name.is_empty() || map_name.is_empty() || appr.is_empty() {
        return Ok(None);
    }

    // `:688` `switch (HUtil32.StrToInt(type, 0))`；不在 {0,1,2} ⇒ `npc == null` ⇒ 跳过。
    let Some(npc_type) = NpcType::from_raw(str_to_int(&s_type, 0)) else {
        return Ok(None);
    };

    Ok(Some(NpcInfo {
        chr_name,
        npc_type,
        map_name,
        // `:703-704` `(short)StrToInt16(cX/cY, 0)`。
        x: str_to_int16(&c_x, 0),
        y: str_to_int16(&c_y, 0),
        // `:706` `(short)StrToInt16(flag, 0)`。
        npc_flag: str_to_int16(&flag, 0),
        // `:707` `(ushort)StrToInt(appr, 0)`。
        appr: str_to_int(&appr, 0) as u16,
    }))
}

/// 整文件解析（`LoadNpcs` 的解析半）。
///
/// 行尾 CR 吸收（C# `StringList.LoadFromFile` 语义）；任何一行的 `GetValidStrCap` panic
/// 形态都以 `Err` 冒泡给调用方。
///
/// # Errors
/// 见 [`parse_npc_line`]。
pub fn load_npcs(file_text: &str) -> Result<Vec<NpcInfo>, EnvirError> {
    let mut out = Vec::new();
    for line in file_text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(info) = parse_npc_line(line)? {
            out.push(info);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 部署件 `E:\MirServer\M2GameSvr\Envir\Npcs.txt` 的三条真实行形态（GBK 解码后的中文名）。
    /// 第 1 行 = `比奇国王 1 0122 29 32 0 8`（type=1 GuildOfficial）；
    /// 第 2 行 = `沙巴克城堡管理 2 0150 7 16 0 8`（type=2 CastleOfficial）；
    /// 第 3 行 = `老人 0 3 669 338 0 1`（type=0 Merchant）。
    #[test]
    fn parses_three_deployed_rows_field_by_field() {
        let king = parse_npc_line("比奇国王           1     0122  29    32    0     8")
            .unwrap()
            .unwrap();
        assert_eq!(
            king,
            NpcInfo {
                chr_name: "比奇国王".to_string(),
                npc_type: NpcType::GuildOfficial,
                map_name: "0122".to_string(),
                x: 29,
                y: 32,
                npc_flag: 0,
                appr: 8,
            }
        );

        let master = parse_npc_line("沙巴克城堡管理           2     0150  7     16    0     8")
            .unwrap()
            .unwrap();
        assert_eq!(master.npc_type, NpcType::CastleOfficial);
        assert_eq!(master.map_name, "0150");
        assert_eq!((master.x, master.y), (7, 16));
        assert_eq!(master.appr, 8);

        let old = parse_npc_line("老人               0     3     669   338   0     1")
            .unwrap()
            .unwrap();
        assert_eq!(old.npc_type, NpcType::Merchant);
        assert_eq!(old.chr_name, "老人");
        assert_eq!(old.map_name, "3");
        assert_eq!((old.x, old.y), (669, 338));
        assert_eq!(old.appr, 1);
    }

    /// `:672` 行过滤：空行 / `;` 注释跳过（部署件头两行即注释）。
    #[test]
    fn skips_comment_and_empty_lines() {
        assert_eq!(parse_npc_line(";比奇 NPC 文件").unwrap(), None);
        assert_eq!(parse_npc_line("").unwrap(), None);
        assert_eq!(parse_npc_line("   ").unwrap(), None);
    }

    /// `:685` 入库门：`appr` 缺列 ⇒ 不入库（`mapName`/`chrName` 缺同理）。
    #[test]
    fn gate_requires_name_map_and_appr() {
        // 只有 6 列（缺 appr）⇒ appr 空 ⇒ 拒。
        assert_eq!(parse_npc_line("比奇国王 1 0122 29 32 0").unwrap(), None);
        // 只有名字（mapName 空）⇒ 拒。
        assert_eq!(parse_npc_line("比奇国王").unwrap(), None);
        // 行首有空白 ⇒ `GetValidStr3` 跳分隔符后 chrName="1"（非空）；但 appr 缺列 ⇒ 门拒。
        assert_eq!(parse_npc_line("   1 0122 29 32 0 8").unwrap(), None);
    }

    /// `:688-698` `switch`：`type` 不在 `{0,1,2}` ⇒ `npc == null` ⇒ 整行跳过；
    /// `type` 缺省/非数字 ⇒ `StrToInt(...,0)`=0 ⇒ `Merchant`。
    #[test]
    fn type_out_of_range_skips_but_default_zero_is_merchant() {
        assert_eq!(parse_npc_line("甲 3 0122 1 2 0 8").unwrap(), None);
        assert_eq!(parse_npc_line("甲 9 0122 1 2 0 8").unwrap(), None);
        assert_eq!(parse_npc_line("甲 -1 0122 1 2 0 8").unwrap(), None);
        // 非数字 type ⇒ StrToInt 默认 0 ⇒ Merchant。
        let r = parse_npc_line("甲 x 0122 1 2 0 8").unwrap().unwrap();
        assert_eq!(r.npc_type, NpcType::Merchant);
    }

    /// `:703-707` 数值列口径：`cX/cY` = `(short)StrToInt16(...,0)`；`appr` = `(ushort)StrToInt(...,0)`。
    /// `StrToInt16` 越界/非数字 ⇒ 0；`appr` 走 i32→u16 截断。
    #[test]
    fn numeric_columns_use_oracle_conversions() {
        // cX 非数字 ⇒ 0；cY 非数字 ⇒ 0。
        let r = parse_npc_line("甲 0 0122 左 右 0 8").unwrap().unwrap();
        assert_eq!((r.x, r.y), (0, 0));
        // appr 负数 ⇒ (ushort) 截断（-1 → 65535）。
        let r = parse_npc_line("甲 0 0122 1 2 0 -1").unwrap().unwrap();
        assert_eq!(r.appr, 65535);
    }

    /// **牙齿**：`chrName` 列用 `GetValidStrCap`（首字符引号 ⇒ CaptureString），
    /// 名字带引号且内含空格时按 oracle 的 capture 截断（不做"顺手修"）。
    /// 部署件三条均无引号 ⇒ 本形态实际不可达，钉住防改。
    #[test]
    fn quoted_name_replicates_capture_string_quirk() {
        let r = parse_npc_line("\"Big Boss\" 0 0122 1 2 0 8")
            .unwrap()
            .unwrap();
        // `"Big Boss"` 经 CaptureString：dest = source[st-1..et]（含开引号、遇首个空格截断）
        // ⇒ `"Bi`（与 `guardlist.rs` 的 `"Big Guard"` ⇒ `"Bi` 同形）。
        // 随后 ArrestStringEx(`"Bi`, '"','"') 无闭引号 ⇒ Unchanged ⇒ chr_name 保持 `"Bi`。
        assert_eq!(r.chr_name, "\"Bi");
        // 连锁错位：rest = source[et+1..] = ` Boss" 0 0122 1 2 0 8` 依次落
        // type/mapName/cX/cY/flag/appr。
        assert_eq!(r.npc_type, NpcType::Merchant); // type="Boss\"" → StrToInt 0
        assert_eq!(r.map_name, "0");
        assert_eq!((r.x, r.y), (122, 1));
    }

    /// 整文件：注释/空行混入 + CRLF 行尾吸收 + 逐行入库。
    #[test]
    fn load_npcs_filters_and_handles_crlf() {
        let text = ";头\r\n比奇国王 1 0122 29 32 0 8\r\n\r\n;中间\r\n老人 0 3 669 338 0 1\r\n";
        let list = load_npcs(text).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].chr_name, "比奇国王");
        assert_eq!(list[0].npc_type, NpcType::GuildOfficial);
        assert_eq!(list[1].chr_name, "老人");
    }
}
