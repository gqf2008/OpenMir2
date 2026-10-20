//! `Envir/StartPoint.txt` 安全区/出生点配置解析 —— 1:1 复刻
//! `src/GameSrv/DB/LocalDB.cs:822-867`（`LoadStartPoint`）。
//!
//! 交付卡：`loader-fields`（数据加载器字段完整性，tier=R）。此前仓内**唯一**的
//! `StartPoint.txt` 读取在 `bins/game-svr/src/homemap.rs`，只取 `MapName/X/Y` 三列，
//! **丢弃** oracle 逐行读取的 `NotAllowSay/Range/Type/PkZone/PkFire` 五列
//! （`LocalDB.cs:844-862`）—— 本模块补齐这 5 个字段并给出完整 `StartPoint` 记录。
//!
//! ## oracle 行为逐条（行号按本仓库 `src/` 实读）
//!
//! - `:832` 路径 `M2Share.GetEnvirFilePath("StartPoint.txt")`；`:833` `File.Exists` 门
//!   （不存在 ⇒ 整段跳过，**不清表**）；
//! - `:835` 存在时先 `StartPointList.Clear()`；
//! - `:842` 逐行 `sLine = loadList[i].Trim()`（**先 trim 再判注释**）；
//! - `:843` 空行 / 首字符 `;` ⇒ 跳过；
//! - `:845-859` 8 次 `GetValidStr3(..., TextSplitConst)`：
//!   `MapName / cX / cY / allSay / range / type / zone / fire`，
//!   `TextSplitConst = { ' ', '\t' }`（`LocalDB.cs:12`，`RemoveEmptyEntries` 语义）；
//! - `:860` 入库门：`MapName`、`cX`、`cY` **三者非空**（其余列允许缺省）；
//! - `:862-863` 装配：`CurrX = StrToInt16(cX, 0)`、`CurrY = StrToInt16(cY, 0)`、
//!   `NotAllowSay = Convert.ToBoolean(StrToInt(allSay, 0))`（**非 0 即 true**，负数也为 true）、
//!   `Range/Type/PkZone/PkFire = StrToInt(列, 0)`；
//! - `StartPoint.Shape`（`src/OpenMir2/Data/StartPoint.cs`）在本加载器**没有赋值点**
//!   ⇒ 恒为 `0`，本模块照实给 0（不要臆造）。
//!
//! ## 部署件形态（只读核对 `E:/MirServer/M2GameSvr/Envir/StartPoint.txt`）
//!
//! 现网每行只有 3 列（`0    289  618`，GBK + CRLF）⇒ `allSay/range/type/zone/fire`
//! 全部缺省：`NotAllowSay=false`、其余 4 列 `0`。列补齐后同一文件的行为**逐字段不变**，
//! 只是把 oracle 读到的值显式承载下来（供后续 `MakeSafePkZone` / `InSafeZone` 接线）。
//!
//! ## 与 oracle 的建模差异（登记）
//!
//! - oracle 的 `StartPoint` 是可变 `record struct`，`M2Share.StartPointList` 是全局
//!   `List<StartPoint>`；本模块只交付**解析 + 记录 + 按索引取值**（`GetStartPointInfo`），
//!   世界侧（`HomeMap` 回落、安全区判定）由接线层消费（`bins/game-svr`，不在本片写集）。
//! - `Convert.ToBoolean(int)` 只有「非 0 ⇒ true」，与 `bool` 的 .NET 默认格式无关；
//!   不做字符串真值判断（那是 `ReadWriteBool` 的口径，别混用）。

use std::path::{Path, PathBuf};

use mir2_shared::hutil32::{get_valid_str3, str_to_int, str_to_int16};

use crate::error::EnvirError;
use crate::paths::envir_file_path;
use crate::text::decode_text;

/// `LocalDB.TextSplitConst = { ' ', '\t' }`（`LocalDB.cs:12`）。
pub const START_POINT_TEXT_SPLIT: [char; 2] = [' ', '\t'];

/// `M2Share.GetEnvirFilePath("StartPoint.txt")` 里的文件名常量（`LocalDB.cs:832`）。
pub const START_POINT_FILE_NAME: &str = "StartPoint.txt";

/// `src/OpenMir2/Data/StartPoint.cs` 的 1:1 读侧记录（`LoadStartPoint` 实际赋值的 8 列 +
/// 恒 0 的 `Shape`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartPoint {
    /// `StartPoint.MapName`（入库门要求非空）。
    pub map_name: String,
    /// `StartPoint.CurrX`（`StrToInt16(cX, 0)`）。
    pub x: i16,
    /// `StartPoint.CurrY`（`StrToInt16(cY, 0)`）。
    pub y: i16,
    /// `StartPoint.NotAllowSay`（`Convert.ToBoolean(StrToInt(allSay, 0))`）。
    pub not_allow_say: bool,
    /// `StartPoint.Range`（`StrToInt(range, 0)`；`MakeSafePkZone` 的格子半径）。
    pub range: i32,
    /// `StartPoint.Type`（`StrToInt(type, 0)`；`MakeSafePkZone` 的 `SafeEvent` 类型）。
    pub r#type: i32,
    /// `StartPoint.PkZone`（`StrToInt(zone, 0)`）。
    pub pk_zone: i32,
    /// `StartPoint.PkFire`（`StrToInt(fire, 0)`）。
    pub pk_fire: i32,
    /// `StartPoint.Shape` —— `LoadStartPoint` **无赋值点** ⇒ 恒 `0`（登记，不臆造）。
    pub shape: u8,
}

/// 解析 `Envir/StartPoint.txt` 的**一行**（`LocalDB.cs:842-863`）。
///
/// 返回 `None` 对应 oracle 不入库的行：空行 / `;` 注释 / `MapName`·`cX`·`cY` 任一为空。
#[must_use]
pub fn parse_start_point_line(line: &str) -> Option<StartPoint> {
    // `:842` `string sLine = loadList[i].Trim();`
    let s_line = line.trim();
    // `:843` `if (!string.IsNullOrEmpty(sLine) && sLine[0] != ';')`
    if s_line.is_empty() || s_line.starts_with(';') {
        return None;
    }
    // `:845-859` 8 次 `GetValidStr3(..., TextSplitConst)`（dest = 首段，返回 = 剩余）。
    let (map_name, rest) = get_valid_str3(s_line, &START_POINT_TEXT_SPLIT);
    let (c_x, rest) = get_valid_str3(&rest, &START_POINT_TEXT_SPLIT);
    let (c_y, rest) = get_valid_str3(&rest, &START_POINT_TEXT_SPLIT);
    let (all_say, rest) = get_valid_str3(&rest, &START_POINT_TEXT_SPLIT);
    let (range, rest) = get_valid_str3(&rest, &START_POINT_TEXT_SPLIT);
    let (r#type, rest) = get_valid_str3(&rest, &START_POINT_TEXT_SPLIT);
    let (zone, rest) = get_valid_str3(&rest, &START_POINT_TEXT_SPLIT);
    let (fire, _) = get_valid_str3(&rest, &START_POINT_TEXT_SPLIT);

    // `:860` 入库门：MapName / cX / cY 三者非空。
    if map_name.is_empty() || c_x.is_empty() || c_y.is_empty() {
        return None;
    }

    Some(StartPoint {
        map_name,
        // `:862` `CurrX/CurrY = StrToInt16(列, 0)`。
        x: str_to_int16(&c_x, 0),
        y: str_to_int16(&c_y, 0),
        // `:862` `NotAllowSay = Convert.ToBoolean(StrToInt(allSay, 0))`：非 0 ⇒ true。
        not_allow_say: str_to_int(&all_say, 0) != 0,
        range: str_to_int(&range, 0),
        r#type: str_to_int(&r#type, 0),
        pk_zone: str_to_int(&zone, 0),
        pk_fire: str_to_int(&fire, 0),
        shape: 0,
    })
}

/// 整文件解析（`LoadStartPoint` 的解析半）。
///
/// 行分离按 `StringList.LoadFromFile` 的 `StreamReader.ReadLine` 口径（`\r\n` / `\n`；
/// 单独 `\r` 的差异与 `crates/data::lists` 同款登记：部署件均为 CRLF/LF，不可达）。
#[must_use]
pub fn parse_start_points(text: &str) -> Vec<StartPoint> {
    text.lines().filter_map(parse_start_point_line).collect()
}

/// `M2Share.GetStartPointInfo(int nIndex, ref short nX, ref short nY)`
/// （`src/M2Server/M2Share.cs:841-857`）：
/// 索引越界或该条 `MapName` 为空 ⇒ `None`（调用方保持 `nX=nY=0`）。
#[must_use]
pub fn get_start_point_info(list: &[StartPoint], index: usize) -> Option<(&str, i16, i16)> {
    let point = list.get(index)?;
    if point.map_name.is_empty() {
        return None;
    }
    Some((point.map_name.as_str(), point.x, point.y))
}

/// `M2Share.GetEnvirFilePath("StartPoint.txt")`（单参数重载，`M2Share.cs:1970-1977`）。
#[must_use]
pub fn start_point_path(base_path: &Path, envir_dir: &str) -> PathBuf {
    envir_file_path(base_path, envir_dir, "", START_POINT_FILE_NAME)
}

/// `LoadStartPoint` 的文件半。
///
/// - 文件不存在（或同名路径是目录）⇒ `Ok(None)`（oracle `File.Exists` 为假 ⇒ 整段跳过，
///   `StartPointList` 保持原样；启动时为空表）；
/// - 存在 ⇒ `Ok(Some(解析结果))`（oracle 先 `Clear()` 再逐条 `Add`，零条也返回 `Some(vec![])`）。
///
/// # Errors
/// 文件存在但读取失败（权限/句柄/IO）⇒ [`EnvirError::Io`]（oracle `LoadFromFile` 抛出，
/// `LoadStartPoint` 无 try/catch ⇒ 启动期 fail-closed）。
pub fn load_start_points_if_exists(path: &Path) -> Result<Option<Vec<StartPoint>>, EnvirError> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|source| EnvirError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(parse_start_points(&decode_text(&bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **判据 SP-1**：部署件三列形态（`E:/MirServer/M2GameSvr/Envir/StartPoint.txt` 首两行）。
    ///
    /// 牙：删掉 `not_allow_say` 的解析行（恒 false）或把 `range` 读成常量 0 ⇒
    /// `parses_deployed_three_column_shape` 仍绿（部署件五列都为默认），但
    /// `parses_full_eight_column_line` 必红——两用例分工：部署形态锁「三列 ⇒ 默认值」，
    /// 全列形态锁「每列真的读到」。
    #[test]
    fn parses_deployed_three_column_shape() {
        let a = parse_start_point_line("0    289  618").unwrap();
        assert_eq!(
            a,
            StartPoint {
                map_name: "0".to_string(),
                x: 289,
                y: 618,
                not_allow_say: false,
                range: 0,
                r#type: 0,
                pk_zone: 0,
                pk_fire: 0,
                shape: 0,
            }
        );
        let b = parse_start_point_line("11\t178\t326").unwrap();
        assert_eq!((b.map_name.as_str(), b.x, b.y), ("11", 178, 326));
        // 部署件末行的空行/CRLF 由 `str::lines()` 吸收。
        assert!(parse_start_point_line("").is_none());
    }

    /// **判据 SP-2（五列字段完整性）**：8 列全给时每一列都落到对应字段。
    ///
    /// 牙：分别删掉 `not_allow_say` / `range` / `r#type` / `pk_zone` / `pk_fire` 任一解析行 ⇒ 本测必红。
    #[test]
    fn parses_full_eight_column_line() {
        let p = parse_start_point_line("3 644 290 1 7 2 13 5").unwrap();
        assert_eq!(p.map_name, "3");
        assert_eq!(p.x, 644);
        assert_eq!(p.y, 290);
        assert!(p.not_allow_say, "allSay=1 ⇒ Convert.ToBoolean(1)=true");
        assert_eq!(p.range, 7);
        assert_eq!(p.r#type, 2);
        assert_eq!(p.pk_zone, 13);
        assert_eq!(p.pk_fire, 5);
        assert_eq!(p.shape, 0, "LoadStartPoint 无 Shape 赋值点");

        // `NotAllowSay` 是 `Convert.ToBoolean`（非 0 即 true），不是 `ReadWriteBool` 真值表。
        let neg = parse_start_point_line("3 1 2 -1").unwrap();
        assert!(
            neg.not_allow_say,
            "负数非 0 ⇒ true（Convert.ToBoolean 口径）"
        );
        let zero = parse_start_point_line("3 1 2 0").unwrap();
        assert!(!zero.not_allow_say);
        let yes = parse_start_point_line("3 1 2 YES").unwrap();
        assert!(
            !yes.not_allow_say,
            "`YES` 走 StrToInt 失败 ⇒ 0 ⇒ false（与 ReadWriteBool 的 YES=true 不同）"
        );
    }

    /// **判据 SP-3**：`StrToInt16` 的截断/失败口径（X/Y 是 short）。
    #[test]
    fn xy_use_str_to_int16_defaults_and_short_range() {
        // 解析失败 ⇒ 0（`StrToInt16(列, 0)`），但列非空 ⇒ 仍入库。
        let p = parse_start_point_line("3 abc def").unwrap();
        assert_eq!((p.x, p.y), (0, 0));
        // short 越界 ⇒ TryParse 失败 ⇒ 0（不是截断！）。
        let q = parse_start_point_line("3 40000 70000").unwrap();
        assert_eq!((q.x, q.y), (0, 0));
        // 负坐标在 short 范围内照收（oracle 不做 >=0 过滤，本加载器与 Point/*.txt 不同）。
        let r = parse_start_point_line("3 -5 -6").unwrap();
        assert_eq!((r.x, r.y), (-5, -6));
    }

    /// **判据 SP-4**：入库门 = `MapName && cX && cY` 非空（其余列可缺省）。
    #[test]
    fn gate_requires_map_and_both_coords() {
        assert_eq!(parse_start_point_line("3 289"), None, "缺 cY ⇒ 丢弃");
        assert_eq!(parse_start_point_line("3"), None, "缺 cX/cY ⇒ 丢弃");
        assert_eq!(parse_start_point_line("289 618"), None, "缺 MapName ⇒ 丢弃");
        assert_eq!(parse_start_point_line(";0 289 618"), None, "; 注释");
        assert_eq!(parse_start_point_line(""), None);
        // 只有空白 ⇒ trim 后为空 ⇒ 丢弃。
        assert_eq!(parse_start_point_line("   "), None);
    }

    /// **判据 SP-5**：`GetStartPointInfo` 的越界/空名分支（`M2Share.cs:841-857`）。
    #[test]
    fn get_start_point_info_bounds_and_empty_map() {
        let list = vec![
            parse_start_point_line("0 289 618").unwrap(),
            StartPoint {
                map_name: String::new(),
                ..parse_start_point_line("0 1 1").unwrap()
            },
        ];
        assert_eq!(get_start_point_info(&list, 0), Some(("0", 289, 618)));
        assert_eq!(
            get_start_point_info(&list, 1),
            None,
            "MapName 空 ⇒ oracle 不改写 nX/nY、返回 \"\""
        );
        assert_eq!(get_start_point_info(&list, 2), None);
    }

    /// **判据 SP-6**：整文件解析（注释/空行/CRLF 混排，保序、保重复）。
    #[test]
    fn parses_whole_file_skips_comments_and_keeps_order() {
        let text = "; 安全区\r\n0    289  618\r\n\r\n2    503  483\r\n3 330 330 1 5 2 7 9\r\n";
        let list = parse_start_points(text);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].x, 289);
        assert_eq!(list[1].y, 483);
        assert!(list[2].not_allow_say);
        assert_eq!(list[2].range, 5);
        assert_eq!(list[2].pk_fire, 9);
    }

    /// **判据 SP-7（文件半）**：缺文件 ⇒ `Ok(None)`；存在 ⇒ 解析（GBK 只影响中文名，
    /// 本文件无中文，但仍走 `StringList` 口径解码）。
    #[test]
    fn loader_returns_none_when_missing_and_parses_when_present() {
        let dir = std::env::temp_dir().join(format!(
            "mir2-startpoint-test-{}-{}",
            std::process::id(),
            std::line!()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("StartPoint.txt");
        assert_eq!(load_start_points_if_exists(&path).unwrap(), None);
        std::fs::write(&path, b"0 289 618\r\n2 503 483 1 10 3 1 2\r\n").unwrap();
        let list = load_start_points_if_exists(&path).unwrap().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].r#type, 3);
        assert_eq!(list[1].range, 10);
        // 目录（非文件）⇒ `File.Exists` 为假 ⇒ None。
        assert_eq!(load_start_points_if_exists(&dir).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
