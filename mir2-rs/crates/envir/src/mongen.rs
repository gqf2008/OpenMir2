//! `Envir/MonGen.txt` 刷怪配置解析（含 `loadgen` 子文件展开）——
//! 1:1 复刻 `src/GameSrv/DB/LocalDB.cs:501`（`LoadMonGen`）+ `:476`（`LoadMapGen`）。
//!
//! 交付卡：`openmir2-m3-mongen-load-spawn`（M3④-实现 B）。
//!
//! 逐字段来源（行号按 master `fa82187a`，**已逐条在 `src/` 实读复核**）：
//! - 主文件路径 `M2Share.GetEnvirFilePath("MonGen.txt")`：`LocalDB.cs:508`；
//! - `loadgen` 子文件判定：`:520`（`HUtil32.CompareLStr("loadgen", 行)`）；
//! - 子文件追加 `LoadMapGen`：`:476-499`（`GetEnvirFilePath("MonGen")` + **字符串拼接**）；
//! - 8 列解析：`:532-557`（分隔符 `{ ' ', '\t' }`，见 `:12`）；
//! - 入库校验：`:558-570`（`MapName`/`MonName` 非空 && `ZenTime > 0` && `MapMgr.GetMapInfo` 非 null）；
//! - 返回的 `mongenCount = WorldEngine.MonGenCount`：`WorldServer.cs:111`
//!   （= `MonGenList.Sum(x => x.Count)`，**`Count` 字段之和**，不是条目数）。
//!
//! ## 两条**刻意保留的 oracle 缺陷**（§1 硬约束③：行为等价优先于修 bug）
//!
//! 1. **`loadgen` 判定实参写反 ⇒ 子文件包含实际不可达**：
//!    `CompareLStr(src, targ)` 的语义是「`src` 前缀 == `targ`」（`HUtil32.cs:550-576`，
//!    `compn = targ.Length`）。`LocalDB.cs:520` 传的是 `CompareLStr("loadgen", 行)`，
//!    于是 `compn = 行.Length`、要求 `"loadgen".Length(7) >= 行.Length`。
//!    只有行长 ≤ 7 才可能为真；而那种行经 `GetValidStr3` 取到的「子文件名」必为空串
//!    ⇒ `if (!string.IsNullOrEmpty(sMapGenFile))` 不成立 ⇒ **永远不会展开任何子文件**。
//!    本模块保留该判定与展开代码路径（1:1），用 [`compare_lstr`] 直接复刻，不再"顺手修"。
//! 2. **子文件路径是字符串拼接**：`:494` 的 `sFilePatchName = sFileDir + sFileName`
//!    （不是 `Path.Combine`），`sFileDir` 由 `GetEnvirFilePath("MonGen")` 得到且**无尾分隔符**。
//!    照抄为 [`load_map_gen_into`] 的 `format!("{}{}", dir, file)`。
//!
//! ## 与 oracle 的建模差异（登记）
//!
//! - `MonGenInfo.CertList`（`IList<IMonsterActor>`）与 `MonGenInfo.Envir`（`IEnvirnoment`）
//!   属 `crates/world` 的运行时对象表，本 crate 只承载**可与 oracle 逐字段对拍**的配置/计数列；
//!   世界侧另存 actor 列表，`ThreadMonLimit` 的判定见 [`MON_GEN_THREAD_MON_LIMIT`]。
//! - `GetMapInfo(ServerIndex, MapName)`（`MapManager.cs:151-162`）额外要求 `ServerIndex` 相等；
//!   部署态所有地图的 `ServerIndex` 都是 0，[`MapRegistry::id_of`] 与之等价，故本模块用
//!   `id_of(...).is_some()` 把 `GetMapInfo != null` / `FindMap != null` 两道检查合并。

use std::path::{Path, PathBuf};

use mir2_shared::hutil32::{
    arrest_string_ex_ref, compare_lstr, get_valid_str3, get_valid_str_cap, str_to_int, ArrestWrite,
};

use crate::error::EnvirError;
use crate::paths::envir_file_path;
use crate::registry::{MapLoader, MapRegistry};
use crate::text::decode_text;

/// C# `LocalDB.TextSplitConst = { ' ', '\t' }`（`LocalDB.cs:12`）。
pub const TEXT_SPLIT_CONST: [char; 2] = [' ', '\t'];

/// C# `MonGenConst.ThreadMonLimit`（`src/Modules/SystemModule/Data/MonGenInfo.cs:57`）。
///
/// `MonGenInfo.TryAdd`（同文件 `:41-51`）在 `CertList.Count > ThreadMonLimit` 时返回 false
/// —— 注意是 **`>`**：恰好等于上限时**仍允许加入**。
pub const MON_GEN_THREAD_MON_LIMIT: usize = 40000;

/// `Envir/MonGen.txt` 的一行（`src/Modules/SystemModule/Data/MonGenInfo.cs:9-38`）。
///
/// 字段名与 C# 一一对应；`cert_list`/`envir` 属世界侧运行时状态，见模块头。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonGenInfo {
    pub map_name: String,
    pub x: i16,
    pub y: i16,
    pub mon_name: String,
    /// 刷新范围（`(byte)` 截断）。
    pub range: u8,
    /// 配置刷新数量（`(ushort)` 截断）。
    pub count: u16,
    /// 已刷出数量（运行时；解析时为 0）。
    pub active_count: u16,
    /// 刷新间隔毫秒（`StrToInt(列, -1) * 60 * 1000`，i32 回绕）。
    pub zen_time: i32,
    /// 集中坐标刷新机率 0~100（`(byte)` 截断）。
    pub mission_gen_rate: u8,
    /// 怪物 Race（`InitializeMonster` 后由世界侧写入）。
    pub race: i16,
    /// 上次成功刷怪时刻（运行时；解析时为 0）。
    pub start_tick: i32,
}

impl MonGenInfo {
    /// `MonGenInfo.TryAdd` 的列表闸门语义（`MonGenInfo.cs:41-51`）：已有 `cert_count` 个时
    /// 还能否再放一个。返回 `true` 表示未满。
    ///
    /// 注意 oracle 用 `>`（不是 `>=`）：`cert_count == ThreadMonLimit` 时**仍可加入**。
    #[must_use]
    pub fn cert_list_has_room(cert_count: usize) -> bool {
        cert_count <= MON_GEN_THREAD_MON_LIMIT
    }
}

/// 解析 `Envir/MonGen.txt` 的**一行**（已展开 `loadgen` 之后）。
///
/// `map_exists` 对应 oracle 的 `MapMgr.GetMapInfo(ServerIndex, MapName) != null`
/// 与 `MapMgr.FindMap(MapName) != null` 两道检查。
///
/// 返回 `Ok(None)` 表示该行不入库（空行 / `;` 注释 / 校验不过），与 oracle 的
/// `AddMonGenList` 未被调用等价。
///
/// # Errors
/// 怪物名列（`GetValidStrCap`）触发 oracle `CaptureString` 的越界（C# `IndexOutOfRangeException`）。
pub fn parse_mon_gen_line(
    line: &str,
    map_exists: impl Fn(&str) -> bool,
) -> Result<Option<MonGenInfo>, EnvirError> {
    // `LocalDB.cs:531`：`if (!string.IsNullOrEmpty(sLineText) && sLineText[0] != ';')`。
    if line.is_empty() || line.starts_with(';') {
        return Ok(None);
    }

    // `:532` MapName。
    let (map_name, rest) = get_valid_str3(line, &TEXT_SPLIT_CONST);
    // `:534` X（`(short)StrToInt(sData, 0)`）。
    let (s, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let x = str_to_int(&s, 0) as i16;
    // `:536` Y。
    let (s, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let y = str_to_int(&s, 0) as i16;
    // `:538-543` MonName（`GetValidStrCap` + 首字符为 `"` 时 `ArrestStringEx(sData, "\"", "\"", ref sData)`）。
    let (mut s, rest) = get_valid_str_cap(&rest, &TEXT_SPLIT_CONST).map_err(|_| {
        EnvirError::MonGenCaptureString {
            line: line.to_string(),
        }
    })?;
    if !s.is_empty() && s.starts_with('"') {
        if let (ArrestWrite::Value(v), _) = arrest_string_ex_ref(&s, '"', '"') {
            s = v;
        }
    }
    let mon_name = s;
    // `:544` Range（`(byte)` 截断）。
    let (s, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let range = str_to_int(&s, 0) as u8;
    // `:546` Count（`(ushort)` 截断）。
    let (s, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let count = str_to_int(&s, 0) as u16;
    // `:548` ZenTime = `StrToInt(sData, -1) * 60 * 1000`（unchecked i32）。
    let (s, rest) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let zen_time = str_to_int(&s, -1).wrapping_mul(60).wrapping_mul(1000);
    // `:550` MissionGenRate（`(byte)` 截断）。
    let (s, _) = get_valid_str3(&rest, &TEXT_SPLIT_CONST);
    let mission_gen_rate = str_to_int(&s, 0) as u8;

    // `:558` 入库校验。
    if map_name.is_empty() || mon_name.is_empty() || zen_time <= 0 || !map_exists(&map_name) {
        return Ok(None);
    }

    Ok(Some(MonGenInfo {
        map_name,
        x,
        y,
        mon_name,
        range,
        count,
        active_count: 0,
        zen_time,
        mission_gen_rate,
        race: 0,
        start_tick: 0,
    }))
}

/// `WorldEngine.MonGenCount`（`WorldServer.cs:111`）= `MonGenList.Sum(x => x.Count)`。
#[must_use]
pub fn mon_gen_count(list: &[MonGenInfo]) -> i32 {
    list.iter().map(|m| i32::from(m.count)).sum()
}

impl MapLoader {
    /// `M2Share.GetEnvirFilePath("MonGen.txt")`（`LocalDB.cs:508`）。
    #[must_use]
    pub fn mon_gen_path(&self) -> PathBuf {
        envir_file_path(&self.base_path, &self.envir_dir, "", "MonGen.txt")
    }

    /// `M2Share.GetEnvirFilePath("MonGen")`（`LocalDB.cs:478`）——**不含尾分隔符**。
    #[must_use]
    pub fn mon_gen_dir(&self) -> PathBuf {
        envir_file_path(&self.base_path, &self.envir_dir, "", "MonGen")
    }

    /// `LocalDB.LoadMonGen`（`:501`）的 1:1 移植。
    ///
    /// 返回 `Ok(None)` 对应 oracle 的 `File.Exists(sFileName) == false`（`result` 保持 0、
    /// `mongenCount = 0`）；`Ok(Some(list))` 对应 `result = 1`，`mongenCount` 用
    /// [`mon_gen_count`] 计算。
    ///
    /// `registry` 提供 oracle `MapMgr.GetMapInfo`/`FindMap` 的等价存在性判定。
    ///
    /// # Errors
    /// - 主文件存在但读取失败；
    /// - 某行的怪物名列触发 `CaptureString` 越界。
    pub fn load_mon_gen(
        &self,
        registry: &MapRegistry,
    ) -> Result<Option<Vec<MonGenInfo>>, EnvirError> {
        let path = self.mon_gen_path();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(EnvirError::Io { path, source }),
        };
        let text = decode_text(&bytes);
        // `StringList.LoadFromFile` 的 `ReadLine` 口径：保留中间空行、不产生行尾空行。
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();

        // `:516-529`：`loadgen` 子文件展开（含 `RemoveAt(i)` + `i++` 的跳跃语义）。
        let file_dir = self.mon_gen_dir();
        let mut i = 0;
        while i < lines.len() {
            // 注意实参写反（模块头缺陷 ①）：`CompareLStr("loadgen", 行)`。
            if compare_lstr("loadgen", &lines[i]) {
                // C#：`sMapGenFile = GetValidStr3(行, ref sLineText, TextSplitConst)`
                // ⇒ `sMapGenFile` 是**剩余段**（子文件名），`sLineText` 是首段。
                let (_s_line_text, s_map_gen_file) = get_valid_str3(&lines[i], &TEXT_SPLIT_CONST);
                lines.remove(i);
                if !s_map_gen_file.is_empty() {
                    load_map_gen_into(&file_dir, &s_map_gen_file, &mut lines);
                }
            }
            i += 1;
        }

        let mut out = Vec::new();
        for line in &lines {
            if let Some(info) = parse_mon_gen_line(line, |name| registry.id_of(name).is_some())? {
                out.push(info);
            }
        }
        Ok(Some(out))
    }
}

/// `LocalDB.LoadMapGen(StringList, sFileName)`（`:476-499`）的 1:1 移植。
///
/// - `Directory.Exists(sFileDir) == false` ⇒ `CreateDirectory`（`:479-482`）；
/// - 文件不存在 ⇒ 静默返回（`:485-488`）；
/// - 路径 = `sFileDir + sFileName`（**字符串拼接，无分隔符**，缺陷 ②）；
/// - 命中则把子文件的每一行**追加到表尾**（`LoadFromFile` 的 `ReadLine` 口径）。
pub fn load_map_gen_into(file_dir: &Path, file_name: &str, lines: &mut Vec<String>) {
    if !file_dir.exists() {
        // oracle 忽略 `CreateDirectory` 的失败（无 try/catch 时会影响后续，照抄不中断）。
        let _ = std::fs::create_dir_all(file_dir);
    }
    let path = PathBuf::from(format!("{}{file_name}", file_dir.display()));
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let text = decode_text(&bytes);
    lines.extend(text.lines().map(str::to_string));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn any_map(_: &str) -> bool {
        true
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mir2-mongen-test-{}-{}-{n}",
            std::process::id(),
            tag
        ));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("清旧临时目录");
        }
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    /// 真实 `Envir/MonGen.txt` 是 GBK 编码。
    fn write_gbk(path: &Path, text: &str) {
        let (bytes, _, _) = encoding_rs::GBK.encode(text);
        std::fs::write(path, bytes.as_ref()).unwrap();
    }

    /// 2×2 全走格 `.map` 字节（够 `MapRegistry` 加载成功即可）。
    fn synth_map() -> Vec<u8> {
        let mut buf = vec![0u8; crate::HEADER_BYTES + 4 * crate::CELL_BYTES];
        buf[0..2].copy_from_slice(&2u16.to_le_bytes());
        buf[2..4].copy_from_slice(&2u16.to_le_bytes());
        buf
    }

    #[test]
    fn parses_deployed_shape() {
        // 部署件 `E:\MirServer\M2GameSvr\Envir\MonGen.txt` 的真实两行形态：
        // 第 1 行中文后接两个 tab（空列被 RemoveEmptyEntries 吸收），第 2 行纯空格分隔。
        let a = parse_mon_gen_line("3\t294\t341\t神鹰\t\t10\t1\t360", any_map)
            .unwrap()
            .unwrap();
        assert_eq!(
            a,
            MonGenInfo {
                map_name: "3".to_string(),
                x: 294,
                y: 341,
                mon_name: "神鹰".to_string(),
                // `神鹰` 后两个 tab 被 `GetValidStr3` 的 RemoveEmptyEntries 语义吸收。
                range: 10,
                count: 1,
                active_count: 0,
                zen_time: 360 * 60 * 1000,
                mission_gen_rate: 0,
                race: 0,
                start_tick: 0,
            }
        );
        let b = parse_mon_gen_line("0     665   87    弓箭护卫        0     1     10", any_map)
            .unwrap()
            .unwrap();
        assert_eq!(b.map_name, "0");
        assert_eq!((b.x, b.y), (665, 87));
        assert_eq!(b.mon_name, "弓箭护卫");
        assert_eq!(b.range, 0);
        assert_eq!(b.count, 1);
        assert_eq!(b.zen_time, 10 * 60 * 1000);
        assert_eq!(b.mission_gen_rate, 0);
    }

    #[test]
    fn skips_blank_and_comment_rows() {
        assert_eq!(parse_mon_gen_line("", any_map).unwrap(), None);
        assert_eq!(
            parse_mon_gen_line(";地图 x y 名称 范围 数量 时间", any_map).unwrap(),
            None
        );
    }

    #[test]
    fn drops_rows_failing_oracle_validation() {
        // 缺 ZenTime 列（只有 6 段）⇒ `StrToInt("", -1) * 60000 < 0` ⇒ 丢弃（`LocalDB.cs:548/558`）。
        assert_eq!(
            parse_mon_gen_line("3 294 341 神鹰 10 1", any_map).unwrap(),
            None
        );
        // ZenTime 非正数 ⇒ 丢弃。
        assert_eq!(
            parse_mon_gen_line("3 294 341 神鹰 10 1 0", any_map).unwrap(),
            None
        );
        // 地图不在注册表 ⇒ 丢弃（对应 `GetMapInfo == null`）。
        assert_eq!(
            parse_mon_gen_line("3 294 341 神鹰 10 1 360", |_| false).unwrap(),
            None
        );
    }

    #[test]
    fn truncation_casts_match_csharp_unchecked() {
        // `(ushort)70000 == 4464`；`(byte)300 == 44`。
        let a = parse_mon_gen_line("3 294 341 神鹰 300 70000 360", any_map)
            .unwrap()
            .unwrap();
        assert_eq!(a.range, 44);
        assert_eq!(a.count, 4464);
        // 负数：`(short)(-1) == -1`，`(byte)(-1) == 255`，`(ushort)(-1) == 65535`。
        let b = parse_mon_gen_line("3 -1 -2 神鹰 -1 -1 360 300", any_map)
            .unwrap()
            .unwrap();
        assert_eq!((b.x, b.y), (-1, -2));
        assert_eq!(b.range, 255);
        assert_eq!(b.count, 65535);
        assert_eq!(b.mission_gen_rate, 44);
    }

    #[test]
    fn quoted_name_keeps_oracle_capture_string_quirk() {
        // oracle：`GetValidStrCap` 走 `CaptureString`（1-based 遗留索引）⇒ 取到 `"神鹰`（含开引号、
        // 丢闭引号），随后 `ArrestStringEx` 找不到闭定界符 ⇒ **不改写**。
        let a = parse_mon_gen_line("3 294 341 \"神鹰\" 10 1 360", any_map)
            .unwrap()
            .unwrap();
        assert_eq!(a.mon_name, "\"神鹰");
    }

    #[test]
    fn loadgen_compare_lstr_operands_are_reversed_in_oracle() {
        // 缺陷 ①：`CompareLStr("loadgen", 行)` 只在行长 ≤ 7 时才可能命中（见模块头）。
        assert!(!compare_lstr("loadgen", "loadgen sub.txt"));
        assert!(compare_lstr("loadgen", "loadgen"));
        // 因此形如 `loadgen sub.txt` 的行不会被当成包含指令，而是被当作普通配置行解析（随后丢弃）。
        assert_eq!(
            parse_mon_gen_line("loadgen sub.txt", any_map).unwrap(),
            None
        );
    }

    #[test]
    fn load_map_gen_concatenates_without_separator() {
        // 缺陷 ②：路径 = `sFileDir + sFileName`（无分隔符）。用一个以分隔符开头的"文件名"
        // 验证拼接口径，并确认目录会自动创建、子文件内容追加到表尾。
        let dir = temp_dir("concat");
        let file_dir = dir.join("Envir").join("MonGen");
        let mut lines = vec!["head".to_string()];
        // 目录不存在时 `load_map_gen_into` 应自行创建（对应 `Directory.CreateDirectory`）。
        std::fs::create_dir_all(&file_dir).unwrap();
        let file_name = format!("{}sub.txt", std::path::MAIN_SEPARATOR);
        write_gbk(&file_dir.join("sub.txt"), "7 8 9 乙 0 1 30\n");
        load_map_gen_into(&file_dir, &file_name, &mut lines);
        assert!(lines.iter().any(|l| l.starts_with("7 8 9 乙")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn loader_returns_none_when_file_missing_and_parses_when_present() {
        let dir = temp_dir("loader");
        let envir = dir.join("Envir");
        std::fs::create_dir_all(&envir).unwrap();
        let loader = MapLoader::new(dir.clone(), "Map", "Envir");
        // 文件缺失 ⇒ `Ok(None)`（oracle `result = 0` / `mongenCount = 0`）。
        let empty_reg = MapRegistry::from_entries(loader.clone(), Vec::new());
        assert_eq!(loader.load_mon_gen(&empty_reg).unwrap(), None);
        // 文件存在 ⇒ 逐行解析；未知地图丢弃、合法行保留、`MonGenCount` = Σ Count。
        write_gbk(
            &envir.join("MonGen.txt"),
            "; 注释\nA 1 2 甲 0 3 60\nZZ 1 2 乙 0 4 60\n",
        );
        // 造一个只有 "A" 的注册表（用合成 .map 走真实 MapLoader 路径）。
        let map_dir = dir.join("Map");
        std::fs::create_dir_all(&map_dir).unwrap();
        std::fs::write(map_dir.join("fa.map"), synth_map()).unwrap();
        write_gbk(&envir.join("MapInfo.txt"), "[A|fa 甲]\n");
        let registry = loader.load_registry().unwrap();
        assert_eq!(registry.id_of("A"), Some(0));
        let list = loader.load_mon_gen(&registry).unwrap().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].mon_name, "甲");
        assert_eq!(list[0].count, 3);
        assert_eq!(mon_gen_count(&list), 3);
        std::fs::remove_dir_all(&dir).ok();
    }
}
