//! `Envir/MapInfo.txt` 的地图名解析 —— 复刻 `src/GameSrv/Maps/Maps.cs:25-...` 与
//! `src/GameSrv/Maps/MapManager.cs:AddMapInfo`。
//!
//! 只做「地图名 → 取图文件名」这一段（map_id 的来源就是本文件里 section 出现的**先后顺序**）：
//! - section 行以**未 trim 的首字符 `[`** 开头；
//! - `HUtil32.ArrestStringEx(line, "[", "]", ref sMapName)` 取括号内容；
//! - `GetValidStrCap(content, Separator)`：`sMapName = 第一段`，返回 `sMapDesc = 其余`；
//! - `AddMapInfo` 再从 `sMapName` 里分出逻辑名与文件名：
//!   - 含 `|` ⇒ 逻辑名 = `|` 前，文件名 = `|` 后（`GetValidStr3(name, ref name, '|')`）；
//!   - 否则找 `<>`：有 ⇒ 文件名 = `<>` 内、逻辑名 = `>` 后的剩余；无 ⇒ 两者都取整段。
//!
//! 真实部署件的 10 条 `|` 行、以及 `[0147\t桃源]` 这类 TAB 分隔行都由本模块覆盖。

use mir2_shared::hutil32::{arrest_string_ex_ref, get_valid_str3, get_valid_str_cap, ArrestWrite};

/// `HUtil32.Separator = { ' ', ',', '\t' }`。
pub const SEPARATORS: [char; 3] = [' ', ',', '\t'];

/// 一条 MapInfo section 解析结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapInfoEntry {
    /// 逻辑地图名（`MapManager._mapList` 的键；角色记录 `CurMap`）。
    pub map_name: String,
    /// 取图文件名（不带 `.map`）。
    pub map_file_name: String,
    /// `MapManager` 的 `MapDesc`（`MapInfo` 第一段之后的内容，仅供诊断）。
    pub desc: String,
    /// `(byte)StrToInt(s4C, 0)`。
    pub server_index: u8,
}

/// 解析一行 MapInfo；不是 section 行返回 `None`。
///
/// `content` 首字符为 `"` 时 `GetValidStrCap` 走 `CaptureString`（C# 可能抛异常）——
/// 真实 MapInfo.txt 无此形态，这里按「跳过该行」处理并交给调用方登记。
#[must_use]
pub fn parse_section_line(line: &str) -> Option<MapInfoEntry> {
    if !line.starts_with('[') {
        return None;
    }
    let content = match arrest_string_ex_ref(line, '[', ']') {
        (ArrestWrite::Value(v), _) => v,
        (ArrestWrite::Unchanged, _) => return None,
    };
    let (map_name_raw, map_desc) = get_valid_str_cap(&content, &SEPARATORS).ok()?;
    if map_name_raw.is_empty() {
        return None;
    }
    // C#：s4C = GetValidStr3(sMapDesc, ref sMapDesc, Separator).Trim(); nServerIndex = (byte)StrToInt(s4C, 0)
    let (desc_head, s4c_raw) = get_valid_str3(&map_desc, &SEPARATORS);
    let s4c = s4c_raw.trim();
    let server_index = str_to_i32(s4c, 0) as u8;
    let (map_name, map_file_name) = split_map_name_and_file(&map_name_raw);
    Some(MapInfoEntry {
        map_name,
        map_file_name,
        desc: desc_head,
        server_index,
    })
}

/// `MapManager.AddMapInfo` 里逻辑名/文件名的拆分。
#[must_use]
pub fn split_map_name_and_file(name: &str) -> (String, String) {
    if name.contains('|') {
        let (before, after) = get_valid_str3(name, &['|']);
        return (before, after);
    }
    match arrest_string_ex_ref(name, '<', '>') {
        (ArrestWrite::Value(file_name), rest) if !file_name.is_empty() => (rest, file_name),
        _ => (name.to_string(), name.to_string()),
    }
}

/// `Maps.cs` 第一遍扫描的 `ConnectMapInfo` 指令行：返回其后第一个 token（子文件名）。
#[must_use]
pub fn connect_map_info_arg(line: &str) -> Option<String> {
    if !mir2_shared::hutil32::compare_lstr(line, "ConnectMapInfo") {
        return None;
    }
    let div = [' ', '\t'];
    // `HUtil32.GetValidStr3(loadList[count], ref sFlag, …)` 的**返回值**即子文件名。
    let (_, s_map_info_file) = get_valid_str3(line, &div);
    if s_map_info_file.is_empty() {
        None
    } else {
        Some(s_map_info_file)
    }
}

/// 逐行解析出所有 section（不做 `ConnectMapInfo` 展开；展开见 `MapLoader::load_map_info_entries`）。
#[must_use]
pub fn parse_map_info_lines(lines: &[String]) -> Vec<MapInfoEntry> {
    lines
        .iter()
        .filter_map(|line| parse_section_line(line))
        .collect()
}

fn str_to_i32(s: &str, def: i32) -> i32 {
    s.trim().parse::<i32>().unwrap_or(def)
}
