//! `Envir/Merchant.txt`（部署件盘上文件名 `MerChant.txt`）交易商人配置加载 ——
//! 1:1 复刻 `src/GameSrv/DB/LocalDB.cs:412`（`LoadMerchant`）+ `src/Modules/ScriptEngine/ScriptParsers.cs:683-970`
//! 的商人脚本头（`%PRICERATE` / `+StdMode` / `(...)` 命令位）与 `[goods]` 区。
//!
//! 交付卡：`openmir2-m3-merchant-load`（M3⑤-实现 1）。oracle 调用链见
//! `mir2-rs/tests/parity/m3/M3⑤NPC商店现状盘点报告-20261013.md` §1.A / §1.B。
//!
//! ## 逐字段来源（行号按本仓库 `src/` 实读）
//!
//! - 主文件路径 `M2Share.GetEnvirFilePath("Merchant.txt")`：`LocalDB.cs:424`；
//! - 逐行 `Trim()` + `;` 注释跳过：`LocalDB.cs:431-433`；
//! - 10 列解析 `ScriptName/MapName/X/Y/Name/Flag/Appr/IsCastle/CanMove/MoveTime`：
//!   `LocalDB.cs:434-447`（分隔符 `TextSplitConst = { ' ', '\t' }`，见 `:12`）；
//! - `Name` 以 `"` 开头时 `ArrestStringEx`：`LocalDB.cs:439-442`；
//! - 入库校验 `ScriptName` / `MapName` / `Appr` 非空：`LocalDB.cs:448`；
//! - `X/Y` = `StrToInt16(列, 0)`、`NpcFlag` = `StrToInt16(Flag, 0)`、
//!   `Appr` = `(ushort)StrToInt(Appr, 0)`、`MoveTime` = `StrToInt(MoveTime, 0)`：`:450-460`；
//! - `CastleMerchant = StrToInt(IsCastle, 0) != 0`：`:461-464`；
//! - `BoCanMove = StrToInt(CanMove, 0) != 0 && MoveTime > 0`：`:465-468`；
//! - 商人脚本路径 = `Market_Def/<ScriptName>-<MapName>.txt`
//!   （`Merchant.LoadMerchantScript()` → `ScriptParsers.LoadScriptFile(this, "Market_Def", ScriptName+"-"+MapName, true)`，
//!   `src/GameSrv/Npc/Merchant.cs:1089-1095`；`boFlag=true` 才收头信息）；
//! - `%PRICERATE`（`>= 55` 才写入，否则保持构造默认 `100`）：`ScriptParsers.cs:683-692`；
//! - `+StdMode`（`>= 0` 入 `ItemTypeList`）：`ScriptParsers.cs:693-702`；
//! - `(...)` 命令位（`IsBuy`…`IsYbDeal`）：`ScriptParsers.cs:703-780`；
//! - `[goods]` 条目 `物品名 数量 补货分钟` → `RefillGoodsList`：`ScriptParsers.cs:946-970`；
//! - `Merchant()` 构造默认 `PriceRate = 100`：`src/GameSrv/Npc/Merchant.cs:86`。
//!
//! ## 建模说明（登记）
//!
//! - **脚本头 / `[goods]` 的解析复用 `crates/script` 的 [`ScriptParsers::load_script_file`]**
//!   （本模块只做「按商人记录逐条取脚本 → 把 [`LoadOutcome`] 的四个头字段搬到记录上」）。
//! - **`SystemShare.CanSellItem` 是常量 `true`**：`src/Modules/SystemModule/SystemShare.cs:841-857`
//!   的 `DisableSellOffList` 检查整段被注释 ⇒ `[goods]` 的 `RefillGoodsList` **不做过滤**，
//!   与解析器收集的 `refill_goods` 等价（本模块不额外过滤）。
//! - **`Initialize` / 挂图 / 幽灵化不在本卡**：oracle 在 `MerchantInitialize`
//!   （`src/GameSrv/Word/WorldServer.Merchant.cs:8-32`）里 `FindMap(MapName) == null` 时把商人从
//!   `MerchantList` 移除、地图书存在才 `LoadMerchantScript()`。本模块只产出**配置记录**并逐条
//!   尝试加载脚本；地图存在性与实体生命周期（`Ghost`/`AddtoMapSuccess`）留给 ⑤-2
//!   `openmir2-m3-merchant-entity-tick`。
//! - **`GoodsList`（在售货架）不在加载期填**：oracle 只在 `Merchant.Run()` 的 5 分钟补货门后
//!   由 `RefillGoods()` 从 `RefillGoodsList` 生成（`Merchant.cs:112/209`），属 ⑤-2。
//! - **`rename_rand`**：`ScriptParsers` 对重名 label 调 `SystemShare.RandomNumber.GetRandomNumber(1, 200)`
//!   （`ScriptParsers.cs:857`）。本模块不持有随机源，故由调用方注入（oracle 语义下的随机源在世界层）；
//!   该随机值只影响内部 label 名，不影响本记录暴露的四个头字段。

use std::path::PathBuf;

use mir2_script::{LocalFs, ScriptParsers};
use mir2_shared::hutil32::{
    arrest_string_ex_ref, get_valid_str3, str_to_int, str_to_int16, ArrestWrite,
};

use crate::error::EnvirError;
use crate::paths::envir_file_path;
use crate::text::decode_text;

/// C# `LocalDB.TextSplitConst = { ' ', '\t' }`（`LocalDB.cs:12`）。
pub const MERCHANT_TEXT_SPLIT: [char; 2] = [' ', '\t'];

/// 商人脚本子目录（`Merchant.LoadMerchantScript` 的 `SystemShare.ScriptParsers.LoadScriptFile` 第一实参）。
pub const MERCHANT_SCRIPT_DIR: &str = "Market_Def";

/// `Merchant.PriceRate` 的构造默认值（`Merchant.cs:86`）；`%PRICERATE` 低于
/// [`MERCHANT_PRICE_RATE_MIN`] 时不覆盖。
pub const MERCHANT_DEFAULT_PRICE_RATE: i32 = 100;

/// `%PRICERATE` 写入所需的阈值（`ScriptParsers.cs:687`：`nPriceRate >= 55`）。
pub const MERCHANT_PRICE_RATE_MIN: i32 = 55;

/// `Envir/Merchant.txt` 一行对应的商人配置记录（`IMerchant` 的可对拍列）。
///
/// 前 10 个字段来自 `LocalDB.LoadMerchant`（`:435-466`）；`flags` / `price_rate` /
/// `item_type_list` / `refill_goods` 来自该商人的 `Market_Def` 脚本头与 `[goods]` 区
/// （`ScriptParsers.cs:683-970`），脚本缺失时保持默认。
#[derive(Clone, Debug)]
pub struct MerchantRecord {
    /// 脚本名（`IMerchant.ScriptName`，可含 `/` 子目录，如 `封魔谷/恶魔修理师`）。
    pub script_name: String,
    /// 地图名（`IMerchant.MapName`）。
    pub map_name: String,
    /// `CurrX`（`StrToInt16(X, 0)`）。
    pub x: i16,
    /// `CurrY`（`StrToInt16(Y, 0)`）。
    pub y: i16,
    /// 显示名（`IMerchant.ChrName`；原字段带 `"` 时按 `ArrestStringEx` 取引号内）。
    pub chr_name: String,
    /// `IMerchant.NpcFlag`（`StrToInt16(Flag, 0)`）。
    pub npc_flag: i16,
    /// `IMerchant.Appr`（`(ushort)StrToInt(Appr, 0)`）。
    pub appr: u16,
    /// 沙巴克城堡商人（`StrToInt(IsCastle, 0) != 0`）。
    pub castle_merchant: bool,
    /// 可移动（`BoCanMove = StrToInt(CanMove, 0) != 0 && MoveTime > 0`）。
    pub can_move: bool,
    /// 移动间隔（`StrToInt(MoveTime, 0)`）。
    pub move_time: i32,
    /// 脚本头 `(...)` 命令位（`IsBuy`…`IsYbDeal`）。
    pub flags: mir2_script::MerchantFlags,
    /// 脚本 `%PRICERATE`（`>= 55` 生效）的**有效值**：无脚本/无该行/低于阈值时 = 100。
    pub price_rate: i32,
    /// 脚本 `+StdMode` 交易物品类型表（`ItemTypeList`）。
    pub item_type_list: Vec<i32>,
    /// 脚本 `[goods]` 补货表（`RefillGoodsList`）。
    pub refill_goods: Vec<mir2_script::Goods>,
}

impl MerchantRecord {
    /// 该记录对应的脚本文件名（不含 `.txt`）：`<ScriptName>-<MapName>`。
    #[must_use]
    pub fn script_file_stem(&self) -> String {
        format!("{}-{}", self.script_name, self.map_name)
    }
}

/// 解析 `Envir/Merchant.txt` 的**一行**（未加载脚本，脚本头字段为默认值）。
///
/// 返回 `None` 对应 oracle `LoadMerchant` 不入库的行（空行 / `;` 注释 /
/// `ScriptName`/`MapName`/`Appr` 有空的校验不过行，`LocalDB.cs:431-447`）。
#[must_use]
pub fn parse_merchant_line(line: &str) -> Option<MerchantRecord> {
    let trimmed = line.trim();
    // `LocalDB.cs:431-434`：`Trim()` 后空行或首字符 `;` 跳过。
    if trimmed.is_empty() || trimmed.starts_with(';') {
        return None;
    }

    // `:435-440` ScriptName / MapName / X / Y / Name。
    let (script_name, rest) = get_valid_str3(trimmed, &MERCHANT_TEXT_SPLIT);
    let (map_name, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    let (s_x, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    let (s_y, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    let (mut chr_name, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    // `:441-444`：名字首字符为 `"` 时 `ArrestStringEx(sName, "\"", "\"", ref sName)`。
    if !chr_name.is_empty() && chr_name.starts_with('"') {
        if let (ArrestWrite::Value(v), _) = arrest_string_ex_ref(&chr_name, '"', '"') {
            chr_name = v;
        }
    }
    // `:445-446` Flag / Appr / IsCastle / CanMove / MoveTime。
    let (s_flag, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    let (s_appr, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    let (s_is_castle, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    let (s_can_move, rest) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);
    let (s_move_time, _) = get_valid_str3(&rest, &MERCHANT_TEXT_SPLIT);

    // `:447`：`if (!string.IsNullOrEmpty(sScript) && !string.IsNullOrEmpty(sMapName) && !string.IsNullOrEmpty(sAppr))`。
    if script_name.is_empty() || map_name.is_empty() || s_appr.is_empty() {
        return None;
    }

    // `:450-466`：类型截断/转换口径逐条照搬。
    let x = str_to_int16(&s_x, 0);
    let y = str_to_int16(&s_y, 0);
    let npc_flag = str_to_int16(&s_flag, 0);
    let appr = str_to_int(&s_appr, 0) as u16;
    let move_time = str_to_int(&s_move_time, 0);
    let castle_merchant = str_to_int(&s_is_castle, 0) != 0;
    let can_move = str_to_int(&s_can_move, 0) != 0 && move_time > 0;

    Some(MerchantRecord {
        script_name,
        map_name,
        x,
        y,
        chr_name,
        npc_flag,
        appr,
        castle_merchant,
        can_move,
        move_time,
        flags: mir2_script::MerchantFlags::default(),
        price_rate: MERCHANT_DEFAULT_PRICE_RATE,
        item_type_list: Vec::new(),
        refill_goods: Vec::new(),
    })
}

impl crate::registry::MapLoader {
    /// `M2Share.GetEnvirFilePath("Merchant.txt")`（`LocalDB.cs:427`）。
    ///
    /// 注意 oracle 字符串是 `"Merchant.txt"`（小写 `erchant`），部署件盘上文件名是
    /// `MerChant.txt`（Windows 大小写不敏感）——这里照抄 oracle 字面量，别改成盘上大小写。
    #[must_use]
    pub fn merchant_path(&self) -> PathBuf {
        envir_file_path(&self.base_path, &self.envir_dir, "", "Merchant.txt")
    }

    /// `LocalDB.LoadMerchant`（`:412-473`）+ 逐商人 `LoadMerchantScript`（`Merchant.cs:1089`）的
    /// 1:1 移植。
    ///
    /// 返回 `Ok(None)` 对应 oracle `File.Exists(sFileName) == false`（不进循环、无记录）；
    /// `Ok(Some(records))` 对应逐行入库并逐条加载 `Market_Def/<ScriptName>-<MapName>.txt`
    /// 的商人脚本头 / `[goods]`。脚本文件缺失时保持头字段默认（oracle `LogService.Error`
    /// 后不写字段，见 `ScriptParsers.cs:978`）。
    ///
    /// `rename_rand` 透传给 `crates/script` 的重名 label 改名（oracle
    /// `SystemShare.RandomNumber.GetRandomNumber(1, 200)`）。
    ///
    /// # Errors
    /// - 主文件存在但读取失败；
    /// - 某商人的脚本解析抛 `LoadPanic`（C# 侧异常穿透）。
    pub fn load_merchants(
        &self,
        rename_rand: &mut dyn FnMut(i32, i32) -> i32,
    ) -> Result<Option<Vec<MerchantRecord>>, EnvirError> {
        let path = self.merchant_path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(EnvirError::Io { path, source }),
        };
        let text = decode_text(&bytes);
        let lines: Vec<String> = text.lines().map(str::to_string).collect();

        let envir_root = self.base_path.join(&self.envir_dir);
        let parsers = ScriptParsers::new();
        let fs = LocalFs;

        let mut out = Vec::with_capacity(lines.len());
        for line in &lines {
            let Some(mut record) = parse_merchant_line(line) else {
                continue;
            };
            let stem = record.script_file_stem();
            match parsers.load_script_file(
                &fs,
                &envir_root,
                MERCHANT_SCRIPT_DIR,
                &stem,
                true,
                rename_rand,
            ) {
                Ok(Some(outcome)) => {
                    record.flags = outcome.merchant_flags;
                    record.price_rate = outcome.price_rate.unwrap_or(MERCHANT_DEFAULT_PRICE_RATE);
                    record.item_type_list = outcome.item_type_list;
                    record.refill_goods = outcome.refill_goods;
                }
                // oracle：`LogService.Error("Script file not found: ...")`，四个头字段保持默认。
                Ok(None) => {}
                Err(reason) => {
                    return Err(EnvirError::MerchantScriptLoad {
                        script: stem,
                        reason: reason.to_string(),
                    });
                }
            }
            out.push(record);
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 部署件 `Merchant.txt:55` 形态（`盟重土城/仓库 0145 9 12 仓库保管员 0 9 0`）。
    #[test]
    fn parses_deployed_space_shape() {
        let r = parse_merchant_line("盟重土城/仓库 0145 9 12 仓库保管员 0 9 0").expect("应入库");
        assert_eq!(r.script_name, "盟重土城/仓库");
        assert_eq!(r.map_name, "0145");
        assert_eq!((r.x, r.y), (9, 12));
        assert_eq!(r.chr_name, "仓库保管员");
        assert_eq!(r.npc_flag, 0);
        assert_eq!(r.appr, 9);
        assert!(!r.castle_merchant);
        // CanMove / MoveTime 缺列 ⇒ 0 / 0 ⇒ 不可移动。
        assert!(!r.can_move);
        assert_eq!(r.move_time, 0);
        // 未加载脚本 ⇒ 默认 PriceRate 100、空表、默认 flags。
        assert_eq!(r.price_rate, MERCHANT_DEFAULT_PRICE_RATE);
        assert!(r.item_type_list.is_empty());
        assert!(r.refill_goods.is_empty());
        assert!(!r.flags.is_buy && !r.flags.is_storage);
    }

    /// 部署件 `Merchant.txt:1` 形态：脚本名含 `/`、名字无引号、行尾多余 0 列被丢弃。
    #[test]
    fn parses_extra_trailing_columns() {
        let r = parse_merchant_line(
            "LEGM2.CC假人NPC/假人上线NPC\tjiaren\t16\t8\t假人上线点我\t0\t27\t0\t0\t0\t0\t0",
        )
        .expect("应入库");
        assert_eq!(r.script_name, "LEGM2.CC假人NPC/假人上线NPC");
        assert_eq!(r.map_name, "jiaren");
        assert_eq!((r.x, r.y), (16, 8));
        assert_eq!(r.appr, 27);
        assert_eq!(r.move_time, 0);
    }

    /// 名字是**单个**带 `"` 的 token 时，按 `ArrestStringEx` 取引号内（`LocalDB.cs:441-444`）。
    #[test]
    fn unquotes_single_token_name() {
        let r = parse_merchant_line("A/店 0 1 2 \"卫家店\" 0 3 0").expect("应入库");
        assert_eq!(r.chr_name, "卫家店");
    }

    /// **牙齿**：`GetValidStr3` 不感知引号（`HUtil32.cs:391`：纯 `Split(div, 2)`）⇒
    /// 名字里带空格时 oracle 会把它切断（`"某某 老板"` ⇒ `ChrName="某某`，后续列错位）。
    /// 若有人把分词"修"成引号感知，本用例必红。
    #[test]
    fn quoted_name_with_space_is_split_like_oracle() {
        let r = parse_merchant_line("A/店 0 1 2 \"某某 老板\" 0 3 0").expect("应入库");
        assert_eq!(r.chr_name, "\"某某");
        // 错位后：Flag=`老板"`（→0）、Appr=`0`、IsCastle=`3`（→true）、CanMove=`0`。
        assert_eq!(r.npc_flag, 0);
        assert_eq!(r.appr, 0);
        assert!(r.castle_merchant);
    }

    /// `CastleMerchant` / `BoCanMove` 的 oracle 判定（`LocalDB.cs:459-466`）。
    #[test]
    fn castle_and_move_flags() {
        let castle = parse_merchant_line("A/店 0 1 2 甲 0 3 1 0 0").unwrap();
        assert!(castle.castle_merchant);
        assert!(!castle.can_move);
        // CanMove=1 但 MoveTime=0 ⇒ 仍不可移动（`&& MoveTime > 0`）。
        let no_time = parse_merchant_line("A/店 0 1 2 甲 0 3 0 1 0").unwrap();
        assert!(!no_time.can_move);
        let movable = parse_merchant_line("A/店 0 1 2 甲 0 3 0 1 30").unwrap();
        assert!(movable.can_move);
        assert_eq!(movable.move_time, 30);
    }

    /// 空行 / `;` 注释 / `Appr` 缺列 ⇒ 不入库（`LocalDB.cs:431-447`）。
    #[test]
    fn skips_blank_comment_and_incomplete_rows() {
        assert!(parse_merchant_line("").is_none());
        assert!(parse_merchant_line("   ").is_none());
        assert!(parse_merchant_line(";比奇城").is_none());
        // 缺 Appr（第 7 列）：到 `0` 之后 Appr 为空 ⇒ 校验不过。
        assert!(parse_merchant_line("A/店 0 1 2 甲 0").is_none());
        // 没有第二个 token（MapName 空）⇒ 校验不过。
        assert!(parse_merchant_line("A/店").is_none());
    }
}
