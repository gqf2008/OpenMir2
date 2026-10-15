//! `Envir/MiniMap.txt` 小地图索引表解析 —— 1:1 复刻 `src/GameSrv/Maps/Maps.cs:387-414`（`LoadMinMap`）。
//!
//! 交付卡：`openmir2-envir-redlist-batch`（R0-2 装载半）。
//!
//! oracle 行为逐条（行号按 `Maps.cs`）：
//! - `:393` 文件名 `M2Share.GetEnvirFilePath("MiniMap.txt")`；`:394` `File.Exists` 门（无文件 ⇒ 不动表）；
//! - `:396` 命中即 `MiniMapList.Clear()`；
//! - `:399-402` 逐行：`tStr = tMapList[i]`（**不 Trim**）；空行与首字符 `';'` 跳过；
//! - `:404` `GetValidStr3(tStr, ref sMapNo, HUtil32.Separator)`，`Separator = { ' ', ',', '\t' }`；
//! - `:405` `GetValidStr3(tStr, ref sMapIdx, HUtil32.Separator)`；
//! - `:406` `nIdx = (short)StrToInt(sMapIdx, 0)`（**short 截断**照搬；解析失败取默认 0）；
//! - `:407` 入库门 `nIdx > 0`（`<= 0` ⇒ 既不报错也不入库）；
//! - `:409-413` 重复键（OrdinalIgnoreCase）⇒ `LogService.Error` 后 `continue`（**先到保留、后到丢弃**）；
//! - `:414` `TryAdd(sMapNo, nIdx)`。
//!
//! 键比较口径 = `ConcurrentDictionary<string, short>(StringComparer.OrdinalIgnoreCase)`
//! （`GameApp.cs:27`）⇒ **大小写不敏感**（地图名都是 ASCII，用 `eq_ignore_ascii_case` 等价）。
//!
//! 消费点（**不在本片**，登记）：`MapManager.cs:92` 在地图创建时
//! `MiniMapList.TryGetValue(envirnoment.MapName, out minMap)` ⇒ `envirnoment.MinMap`；
//! 下发帧 `CM_WANTMINIMAP(1033)` → `ClientGetMinMap()` → `SM_READMINIMAP_OK/FAIL`
//! （R1 红名单，Rust 侧未实现）⇒ 本片只做「解析 + 注册表 + 宿主装载」。

use mir2_shared::hutil32::{get_valid_str3, str_to_int};

/// `HUtil32.Separator`（`src/OpenMir2/HUtil32.cs:13`）：`{ ' ', ',', '\t' }`。
pub const MINIMAP_SEPARATOR: [char; 3] = [' ', ',', '\t'];

/// 一条小地图配置：地图名 → 小地图索引。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MiniMapEntry {
    pub map_name: String,
    pub index: i16,
}

/// `M2Share.MiniMapList`（`ConcurrentDictionary<string, short>(OrdinalIgnoreCase)`）的等价物。
///
/// 保序（按 `MiniMap.txt` 行序）；查找大小写不敏感（ASCII）；重复键先到保留、后到丢弃。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MiniMapRegistry {
    entries: Vec<MiniMapEntry>,
}

impl MiniMapRegistry {
    /// 已入库条数（`MiniMapList.Count`）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 空表（`MiniMapList.Count == 0`）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `TryGetValue(mapName, out minMap)`（OrdinalIgnoreCase）。
    #[must_use]
    pub fn get(&self, map_name: &str) -> Option<i16> {
        self.entries
            .iter()
            .find(|e| e.map_name.eq_ignore_ascii_case(map_name))
            .map(|e| e.index)
    }

    /// `ContainsKey(mapName)`（OrdinalIgnoreCase）。
    #[must_use]
    pub fn contains(&self, map_name: &str) -> bool {
        self.get(map_name).is_some()
    }

    /// 按入库顺序遍历。
    pub fn iter(&self) -> impl Iterator<Item = &MiniMapEntry> {
        self.entries.iter()
    }

    /// 入库（内部用）。返回是否真的插入（重复则 `false`，照 oracle `continue`）。
    fn try_add(&mut self, entry: MiniMapEntry) -> bool {
        if self.contains(&entry.map_name) {
            return false;
        }
        self.entries.push(entry);
        true
    }
}

/// 解析一行（`Maps.cs:401-415`）。
///
/// 空行 / 首字符 `';'` / `nIdx <= 0` ⇒ `None`（不入库）。
/// **不 Trim**（与 oracle `tStr = tMapList[i]` 一致；`GetValidStr3` 的 RemoveEmptyEntries
/// 语义自会跳过前导分隔符）。
#[must_use]
pub fn parse_minimap_line(line: &str) -> Option<MiniMapEntry> {
    // `:402`：`if (!string.IsNullOrEmpty(tStr) && tStr[0] != ';')`
    if line.is_empty() || line.starts_with(';') {
        return None;
    }
    // `:404` MapNo（`{ ' ', ',', '\t' }`）。
    let (map_no, rest) = get_valid_str3(line, &MINIMAP_SEPARATOR);
    // `:405` MapIdx（同分隔符集合）。
    let (map_idx, _) = get_valid_str3(&rest, &MINIMAP_SEPARATOR);
    // `:406` `(short)StrToInt(sMapIdx, 0)` —— short 截断照搬。
    let index = str_to_int(&map_idx, 0) as i16;
    // `:407` 入库门。
    if index > 0 {
        Some(MiniMapEntry {
            map_name: map_no,
            index,
        })
    } else {
        None
    }
}

/// 整文件解析（`LoadMinMap` 的解析半）。
///
/// 返回 `(注册表, 被丢弃的重复地图名列表)`——后者用于宿主复刻 `:411` 的
/// `LogService.Error($"重复小地图配置信息[...]")`。
///
/// 行尾 CR 吸收（C# `StringList.LoadFromFile` 语义）。
#[must_use]
pub fn load_minimap(file_text: &str) -> (MiniMapRegistry, Vec<String>) {
    let mut registry = MiniMapRegistry::default();
    let mut duplicates = Vec::new();
    for line in file_text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(entry) = parse_minimap_line(line) {
            // `:409-413` 重复键 ⇒ 记一条日志（此处收集）并 continue。
            if !registry.try_add(entry.clone()) {
                duplicates.push(entry.map_name);
            }
        }
    }
    (registry, duplicates)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 部署件 `E:\MirServer\M2GameSvr\Envir\MiniMap.txt` 的真实行形态（`D001  1`，双空格）。
    #[test]
    fn parses_deployed_shape() {
        assert_eq!(
            parse_minimap_line("D001  1"),
            Some(MiniMapEntry {
                map_name: "D001".to_string(),
                index: 1
            })
        );
        // 尾部形态（`631  200` / `64   202`：名字数字长度不同）。
        assert_eq!(parse_minimap_line("631  200").unwrap().index, 200);
        assert_eq!(
            parse_minimap_line("64   202"),
            Some(MiniMapEntry {
                map_name: "64".to_string(),
                index: 202
            })
        );
        // 制表符/逗号变体（`Separator` 含 `,` 与 `\t`）。
        assert_eq!(
            parse_minimap_line("R001\t96").unwrap().index,
            96,
            "制表符也是分隔符"
        );
        assert_eq!(
            parse_minimap_line("D001,7"),
            Some(MiniMapEntry {
                map_name: "D001".to_string(),
                index: 7
            })
        );
    }

    /// 空行 / 注释行 / 无索引行 / 索引 <= 0 ⇒ `None`（不入库）。
    #[test]
    fn filters_empty_comment_and_nonpositive() {
        assert_eq!(parse_minimap_line(""), None);
        assert_eq!(parse_minimap_line(";comment"), None);
        assert_eq!(parse_minimap_line("D001"), None, "无索引 ⇒ sMapIdx 空 ⇒ 0");
        assert_eq!(
            parse_minimap_line("D001  0"),
            None,
            "索引 0 被 `nIdx > 0` 门拒绝"
        );
        assert_eq!(parse_minimap_line("D001  -5"), None, "负索引被拒");
        // 首字符空格 + `;`：oracle 的 `tStr[0] != ';'` 成立（' ' != ';'）⇒ 进解析；
        // 但首 token 是 `;` 开头的整串（`;` 不是分隔符）⇒ 无索引 ⇒ 仍被 `nIdx>0` 排除。
        assert_eq!(parse_minimap_line("  ;comment"), None);
    }

    /// `:406` `(short)` 截断照搬：70000 → 4464（仍 > 0 ⇒ 入库）；40000 → -25536（被门拒绝）。
    #[test]
    fn index_short_truncation_like_oracle() {
        assert_eq!(parse_minimap_line("D001  70000").unwrap().index, 4464);
        assert_eq!(parse_minimap_line("D001  40000"), None);
    }

    /// 整文件：空行混入 + CRLF 吸收 + 保序。
    #[test]
    fn load_filters_blanks_and_handles_crlf() {
        let text = "D001  1\r\nD002  2\r\n\r\n;c\r\nD003  3\r\n";
        let (reg, dups) = load_minimap(text);
        assert_eq!(reg.len(), 3);
        assert!(dups.is_empty());
        assert_eq!(
            reg.iter().map(|e| e.map_name.as_str()).collect::<Vec<_>>(),
            vec!["D001", "D002", "D003"],
            "保序"
        );
        assert_eq!(reg.get("D002"), Some(2));
    }

    /// `:409-413` 重复键（OrdinalIgnoreCase）⇒ 先到保留、后到丢弃 + 记一次重复名。
    #[test]
    fn duplicate_keys_keep_first_case_insensitively() {
        let text = "D001  1\r\nD001  9\r\n;大小写不同也算重复\r\nd001  5\r\n";
        let (reg, dups) = load_minimap(text);
        assert_eq!(reg.len(), 1, "三条只有一条入库");
        assert_eq!(reg.get("D001"), Some(1), "先到保留");
        assert_eq!(dups, vec!["D001".to_string(), "d001".to_string()]);
    }

    /// 大小写不敏感查找（`OrdinalIgnoreCase`）。
    #[test]
    fn lookup_is_ascii_case_insensitive() {
        let (reg, _) = load_minimap("F001  37\r\n");
        assert_eq!(reg.get("f001"), Some(37));
        assert_eq!(reg.get("F001"), Some(37));
        assert_eq!(reg.get("F002"), None);
        assert!(reg.contains("f001"));
    }
}
