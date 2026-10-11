//! 「空 `HomeMap` 的新角色 ⇒ 出生点地图」——oracle `WorldServer.cs:250-260` + `GetHomeInfo`
//! （`:1384-1410`）的 1:1 归因与实现。
//!
//! # 为什么需要它（缺陷现场）
//!
//! `-FixedNewChr` 建出的新角色，DBSrv 写库时 `MapName`/`HomeMap` **都是 NULL**
//! （`NewChrData` 只填 `ChrName/Account/Sex/Job/Hair`，其余用默认值；
//! `PlayDataStorage.Add.cs:94-108` 把 `hd.CurMap`/`hd.HomeMap` 原样写库）⇒
//! Rust 进图门拿 `CurMap=""` 去 `MapRegistry::require_id` 必然 `Err`，落
//! `[WRN] 拒绝进图 ... CurMap:（空）` ⇒ 客户端只收到 7 帧、无 `658`、`in_world=0`。
//!
//! oracle 的处置（`WorldServer.cs:248-260`，`GetHumData` 之后）：
//!
//! ```text
//! GetHumData(playObject, ref humanRcd);         // MapName = humData.CurMap; HomeMap = humData.HomeMap
//! if (string.IsNullOrEmpty(playObject.HomeMap)) // 新角色 HomeMap 也是空
//! {
//!     playObject.HomeMap = GetHomeInfo(playObject.Job, ref homeX, ref homeY);
//!     playObject.HomeX = homeX; playObject.HomeY = homeY;
//!     playObject.MapName = playObject.HomeMap;  // ← **忽略空 CurMap**，直接落到出生点
//!     playObject.CurrX = GetRandHomeX(playObject);  // = HomeX - 2 + Random(3)
//!     playObject.CurrY = GetRandHomeY(playObject);  // = HomeY - 2 + Random(3)
//!     if (playObject.Abil.Level == 0) { /* 新手属性；本片未接，登记在卡 */ }
//! }
//! ```
//!
//! ⇒ 空 `CurMap` **不是**拒绝条件；两侧不等价的是 Rust 缺了这条「空 `HomeMap` ⇒ 出生点」回落。
//!
//! `GetHomeInfo`（`WorldServer.cs:1384-1410`）：
//!
//! ```text
//! if (M2Share.StartPointList.Count > 0)
//! {
//!     if (Count > Config.StartPointSize) I = Random(Config.StartPointSize); else I = 0;
//!     result = M2Share.GetStartPointInfo(I, ref nX, ref nY);   // StartPointList[I] 的 MapName/X/Y
//! }
//! else { result = Config.HomeMap; nX = Config.HomeX; nY = Config.HomeY; }  // 注意 oracle 这里把 nY 写成 nX（笔误）；本片按 X/Y 各自取
//! ```
//!
//! # 确定性口径（本宿主对 `Random` 的处置）
//!
//! oracle 在 `StartPointList.Count > StartPointSize` 时取 `Random(StartPointSize)`；本宿主取 **`I = 0`**
//! （确定性，避免跨侧对拍 flaky）。理由：真实 `E:/MirServer/M2GameSvr/Envir/StartPoint.txt` 里
//! 前 `StartPointSize`(=2) 条的 `MapName` **都是 `0`** ⇒ **进图地图与随机取值无关**，只有出生坐标
//! 在前 `StartPointSize` 条之间漂移。取 0 与夹具实测一致（C# 侧新角色落库 `HomeMap=0 HomeX=289 HomeY=618`
//! = `StartPoint.txt` 第 0 条）。坐标的 `-2 + Random(3)` 同样按 oracle 下界取 `-2`（等价 `Random`=0）。

use std::path::Path;

use mir2_shared::hutil32::get_valid_str3;

/// 分词器：`LocalDB.TextSplitConst = { ' ', '\t' }`（`src/GameSrv/DB/LocalDB.cs:12`）。
const TEXT_SPLIT: [char; 2] = [' ', '\t'];

/// 一条出生点（`LocalDB.cs:853-863` 装配的 `StartPoint`；本接缝只用地图名 + 坐标）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartPoint {
    pub map_name: String,
    pub x: i16,
    pub y: i16,
}

/// `LocalDB.LoadStartPoint`（`src/GameSrv/DB/LocalDB.cs:822-867`）的 1:1 解析。
///
/// - `Trim()` 后空行、首字符 `;` 的行跳过；
/// - 每条按 `TextSplitConst` 顺序取 `MapName / CX / CY`（其余列 `allSay/range/type/zone/fire` 本接缝不用）；
/// - `MapName/CX/CY` 任一为空即丢弃（oracle 的 `if (!string.IsNullOrEmpty(...))`）。
#[must_use]
pub fn parse_start_points(text: &str) -> Vec<StartPoint> {
    let mut out = Vec::new();
    for raw in text.split('\n') {
        let no_cr = raw.strip_suffix('\r').unwrap_or(raw);
        let line = no_cr.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (map_name, rest) = get_valid_str3(line, &TEXT_SPLIT);
        let (cx, rest) = get_valid_str3(&rest, &TEXT_SPLIT);
        let (cy, _) = get_valid_str3(&rest, &TEXT_SPLIT);
        if !map_name.is_empty() && !cx.is_empty() && !cy.is_empty() {
            out.push(StartPoint {
                map_name,
                x: str_to_i16(&cx, 0),
                y: str_to_i16(&cy, 0),
            });
        }
    }
    out
}

/// `HUtil32.StrToInt16(str, def)`：`short.TryParse` 失败取 `def`（`HUtil32.cs`）。
fn str_to_i16(s: &str, def: i16) -> i16 {
    s.trim().parse::<i16>().unwrap_or(def)
}

/// 「空 `HomeMap` ⇒ 出生点」的静态输入（进程启动时读一次）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeFallback {
    /// `Envir/StartPoint.txt`（`M2Share.StartPointList`）。
    pub start_points: Vec<StartPoint>,
    /// `[Setup] StartPointSize`（`Config.StartPointSize`，oracle 缺省 2）。
    pub start_point_size: u8,
    /// `[Setup] HomeMap`（`Config.HomeMap`，oracle 缺省 `"0"`）。
    pub home_map: String,
    /// `[Setup] HomeX`（`Config.HomeX`，oracle 缺省 289）。
    pub home_x: i16,
    /// `[Setup] HomeY`（`Config.HomeY`，oracle 缺省 618）。
    pub home_y: i16,
}

impl HomeFallback {
    /// `WorldServer.GetHomeInfo(job, ref x, ref y)`（`:1384-1410`）。
    ///
    /// `StartPointList` 非空 ⇒ `StartPointList[0]`（见模块头「确定性口径」）；
    /// 空 ⇒ `Config.HomeMap/HomeX/HomeY`。
    #[must_use]
    pub fn get_home_info(&self) -> (String, i16, i16) {
        if let Some(sp) = self.start_points.first() {
            (sp.map_name.clone(), sp.x, sp.y)
        } else {
            (self.home_map.clone(), self.home_x, self.home_y)
        }
    }

    /// 解析后**到底会选哪一条**（用于启动日志与证据；与 [`Self::get_home_info`] 同源）。
    #[must_use]
    pub fn home_index(&self) -> Option<usize> {
        if self.start_points.is_empty() {
            None
        } else {
            // `get_home_info` 取第 0 条；oracle 在 `Count > StartPointSize` 时随机 —— 见模块头。
            Some(0)
        }
    }
}

/// 读 `<base>/<envir_dir>/StartPoint.txt`；文件不存在/读失败 ⇒ 空出生点表
/// （oracle `LocalDB.LoadStartPoint` 的 `if (File.Exists)` 分支；此时 `GetHomeInfo` 落到 `Config.HomeMap`）。
///
/// 现网该文件是 **GBK**（首行注释 `;安全区/出生点`），照 oracle `StringList.LoadFromFile`（GB2312）
/// 用 [`mir2_envir::text::decode_text`]（BOM/UTF-16/GBK 判定）解码，不用 `read_to_string`（会因非 UTF-8 失败
/// 而静默拿到空表——这条是 2026-10-12 真机跑出的）。
#[must_use]
pub fn load_start_points(base: &Path, envir_dir: &str) -> Vec<StartPoint> {
    let path = base.join(envir_dir).join("StartPoint.txt");
    match std::fs::read(&path) {
        Ok(bytes) => parse_start_points(&mir2_envir::text::decode_text(&bytes)),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 现网 `E:/MirServer/M2GameSvr/Envir/StartPoint.txt` 的前几行（逐字）。
    const REAL_HEAD: &str = ";安全区/出生点\n0    289  618\n0    650  631\n0    330  266\n2    503  483\n3    330  330\n4    240  200\n5    141  337\n11   178  326\n";

    #[test]
    fn parse_real_head_matches_oracle_tokens() {
        let sp = parse_start_points(REAL_HEAD);
        assert_eq!(sp.len(), 8, "首行注释跳过：{sp:?}");
        assert_eq!(
            sp[0],
            StartPoint {
                map_name: "0".into(),
                x: 289,
                y: 618
            }
        );
        assert_eq!(
            sp[1],
            StartPoint {
                map_name: "0".into(),
                x: 650,
                y: 631
            }
        );
        assert_eq!(
            sp[7],
            StartPoint {
                map_name: "11".into(),
                x: 178,
                y: 326
            }
        );
    }

    /// 红检：把分隔符从 `{ ' ', '\t' }` 改成只认 `','` ⇒ 现网（空格分隔）解析出 0 条 ⇒ 必红。
    /// 这里用「显式喂逗号文本」做**正向对照**：同一行若只有逗号也应能分（oracle 的 `GetValidStr3`
    /// 只认给定分隔符，行内空格属于 token），从而把「解析器确实按 `' '/\t'` 分」钉住。
    #[test]
    fn parser_splits_on_space_and_tab_only() {
        let text = "0\t289 618\n";
        let sp = parse_start_points(text);
        assert_eq!(
            sp,
            vec![StartPoint {
                map_name: "0".into(),
                x: 289,
                y: 618
            }]
        );
    }

    #[test]
    fn skips_blank_comment_and_incomplete_lines() {
        let text = "\n; c\n0 289\n   \n1 2 3\n";
        let sp = parse_start_points(text);
        // `0 289` 缺 CY ⇒ 丢；`1 2 3` 合法。
        assert_eq!(
            sp,
            vec![StartPoint {
                map_name: "1".into(),
                x: 2,
                y: 3
            }]
        );
    }

    #[test]
    fn get_home_info_prefers_first_start_point() {
        let fb = HomeFallback {
            start_points: parse_start_points(REAL_HEAD),
            start_point_size: 2,
            home_map: "3".into(),
            home_x: 839,
            home_y: 668,
        };
        assert_eq!(fb.get_home_info(), ("0".to_string(), 289, 618));
        assert_eq!(fb.home_index(), Some(0));
    }

    #[test]
    fn get_home_info_falls_back_to_config_when_no_start_points() {
        let fb = HomeFallback {
            start_points: Vec::new(),
            start_point_size: 2,
            home_map: "3".into(),
            home_x: 839,
            home_y: 668,
        };
        assert_eq!(fb.get_home_info(), ("3".to_string(), 839, 668));
        assert_eq!(fb.home_index(), None);
    }

    /// 现网 `StartPoint.txt` 是 **GBK**（首行 `;安全区/出生点`）——`load_start_points` 必须解码成
    /// 8 条，否则空表 ⇒ 落到 `Config.HomeMap=3` 而不是夹具实测的 `StartPoint[0]=("0",289,618)`。
    /// 红检：把 `std::fs::read` + `decode_text` 改回 `read_to_string` ⇒ 本用例必红（真实 GBK 文件读失败）。
    #[test]
    fn load_start_points_decodes_gbk_real_file() {
        let dir = std::env::temp_dir().join(format!("gs-homemap-gbk-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Envir")).unwrap();
        // 首行 GBK `;安全区/出生点`（`b0 b2 c8 ab c7 f8`）+ 两条出生点。
        let mut bytes: Vec<u8> = vec![0x3B, 0xB0, 0xB2, 0xC8, 0xAB, 0xC7, 0xF8, 0x0D, 0x0A];
        bytes.extend_from_slice(b"0    289  618\r\n0    650  631\r\n");
        std::fs::write(dir.join("Envir").join("StartPoint.txt"), &bytes).unwrap();
        let sp = load_start_points(&dir, "Envir");
        assert_eq!(
            sp.len(),
            2,
            "GBK 文件必须解出 2 条（read_to_string 会得 0 条）：{sp:?}"
        );
        assert_eq!(
            sp[0],
            StartPoint {
                map_name: "0".into(),
                x: 289,
                y: 618
            }
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
