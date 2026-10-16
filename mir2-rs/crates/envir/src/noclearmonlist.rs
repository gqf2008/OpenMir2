//! `Envir/NoClearMonList.txt` 不清除怪物名单 —— 解析 + 注册表。
//!
//! 1:1 复刻 `src/M2Server/M2Share.cs:1746-1766`（`LoadNoClearMonList`）；
//! `src/Modules/SystemModule/SystemShare.cs:1658-1678` 为同形副本（同一个 `Load*` 调用点
//! `GameApp.cs:109` 走的是 `M2Share` 那份，见下）。
//!
//! 交付卡：`openmir2-envir-redlist-batch`（内容对账门 §4-1 报红缺口：
//! `GameApp.cs:109 M2Share.LoadNoClearMonList()` ∧ 部署有件 ∧ Rust 无实现）。
//!
//! # oracle 行为逐条（行号按 `M2Share.cs`）
//!
//! - `:1749` 路径 `GetEnvirFilePath("NoClearMonList.txt")`
//!   = `Path.Combine(BasePath, Config.EnvirDir, "NoClearMonList.txt")`（`M2Share.cs:1970-1977`）；
//! - `:1751` `File.Exists(sFileName)` 门：
//!   - **存在** ⇒ `:1753` `NoClearMonLIst.Clear()`；`:1754` `StringList.LoadFromFile`（GBK，带 BOM 探测）；
//!     `:1755-1758` 逐行 `NoClearMonLIst.Add(LoadList[i].Trim())`；`:1759` `result = true`。
//!   - **不存在** ⇒ `:1763` `LoadList.SaveToFile(sFileName)`。`StringList.SaveToFile`
//!     （`StringList.cs:262-274`）首行 `GetEncoding(fileName, …)` → `new FileStream(fileName, FileMode.Open)`
//!     ⇒ **文件不存在时抛 `FileNotFoundException`**，而 `LoadNoClearMonList` **无 try/catch** ⇒ 异常穿透。
//!     ⇒ **缺文件不是「空名单」，而是抛异常**（照搬并登记，见 [`EnvirError::NoClearMonListMissing`]）。
//! - `NoClearMonLIst` 本身在 `GameApp.cs:47` 由 `new List<string>()` 初始化（先于 `:109` 的加载），
//!   故 `:1753` 的 `Clear()` 不会 NRE。
//!
//! # `StringList.LoadFromFile` 的行语义（`src/OpenMir2/Common/StringList.cs:276-291`）
//!
//! `if (!File.Exists) return; Clear(); while (sr.Peek() >= 0) AppendText(sr.ReadLine());`
//! `StreamReader.ReadLine` 以 `\r\n` / `\r` / `\n` 断行；行尾终止符**不**入内容；末尾终止符
//! **不**产生幻影空行；**空行产生 `""` 条目**。本模块据此**保留空行**（不做任何过滤），
//! 与 `MiniMap.txt` / `Npcs.txt` 不同——那两者有显式的空行/`;` 注释过滤，本文件**没有**。
//!
//! # 消费点（不在本片，登记给世界层接线）
//!
//! `ExecutionProcessingSys.cs:2983-3001` 的 `ActionOfClearMapMon` 用
//! `SystemShare.GetNoClearMonList(mon.ChrName)` 决定跳过名单内的怪物；但
//! `SystemShare.GetNoClearMonList`（`SystemShare.cs:1694-1708`）**整段被注释、恒返回 `false`**
//! ⇒ 名单对排除逻辑**当前实为 no-op**。本片只交付「解析 + 注册表 + 名字查询」；
//! 是否恢复排除语义，留给后续接线片按 §1 硬约束③（行为等价优先于修 bug）单独裁决并登记
//! （照搬 `false` = 保持 no-op；或由 owner 裁决恢复排除）。

use std::path::{Path, PathBuf};

use crate::error::EnvirError;
use crate::text::decode_text;

/// `M2Share.GetEnvirFilePath("NoClearMonList.txt")` 里的文件名常量（`M2Share.cs:1749`）。
pub const NO_CLEAR_MON_LIST_FILE_NAME: &str = "NoClearMonList.txt";

/// `path_combine(BasePath, EnvirDir, "NoClearMonList.txt")` —— `GetEnvirFilePath` 单参重载
/// （`M2Share.cs:1970-1977`；本文件名不以 `..` 开头，走非 `..` 分支）。
#[must_use]
pub fn no_clear_mon_list_path(base_path: &Path, envir_dir: &str) -> PathBuf {
    crate::paths::path_combine(&[
        base_path,
        Path::new(envir_dir),
        Path::new(NO_CLEAR_MON_LIST_FILE_NAME),
    ])
}

/// `StreamReader.ReadLine` 语义的行切分（见模块头）。
///
/// `\r\n` / `\r` / `\n` 均断行；行尾终止符不入行；末尾终止符不产生幻影空行；
/// 空行返回 `""`；无终止符的非空文本返回单条；空文本返回空表。
fn read_lines(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let n = bytes.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let start = i;
        while i < n && bytes[i] != b'\r' && bytes[i] != b'\n' {
            i += 1;
        }
        out.push(&text[start..i]);
        if i < n {
            if bytes[i] == b'\r' && i + 1 < n && bytes[i + 1] == b'\n' {
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    out
}

/// `LoadNoClearMonList` 的**解析半**：`StringList.LoadFromFile` + 逐行 `Trim()`（`M2Share.cs:1754-1758`）。
///
/// 返回按行序、**保留重复、保留空行**（`""`）的名单。`Trim()` 口径 = `str::trim()`
/// （Unicode `White_Space`，与 .NET `char.IsWhiteSpace` 的 Trim 集在本数据下等价；
/// 单调中日韩文本无空白，登记该近似）。
#[must_use]
pub fn parse_no_clear_mon_lines(file_text: &str) -> Vec<String> {
    read_lines(file_text)
        .into_iter()
        .map(|line| line.trim().to_string())
        .collect()
}

/// `M2Share.NoClearMonLIst`（`IList<string>`，`M2Share.cs:161`）的等价物。
///
/// 保序、保重复（`List.Add` 语义）；这里同时提供按名查询，供世界层「排除名单内怪物」消费。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoClearMonList {
    entries: Vec<String>,
}

impl NoClearMonList {
    /// 从已解码文本构建（解析半）。
    #[must_use]
    pub fn parse(file_text: &str) -> Self {
        Self {
            entries: parse_no_clear_mon_lines(file_text),
        }
    }

    /// 从字节构建：先按 `StringList.LoadFromFile` 的编码判定解码，再解析。
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::parse(&decode_text(bytes))
    }

    /// 名单条数（`NoClearMonLIst.Count`）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 空名单（`NoClearMonLIst.Count == 0`）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 按行序取名单切片。
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.entries
    }

    /// 按行序遍历。
    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.entries.iter()
    }

    /// 第 `index` 条（`NoClearMonLIst[index]`）。
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(String::as_str)
    }

    /// `SystemShare.GetNoClearMonList(sMonName)` 的等价查询：名字**精确**在名单内。
    ///
    /// oracle 原文为 `sMonName.CompareTo(list[i]) == 0`（`CurrentCulture` 比较，全等即 `0`）；
    /// 这里用精确相等（CJK 怪物名无 culture 折叠差异，登记该近似）。
    ///
    /// **注意**：该查询在 oracle 里**整段被注释、恒 false** ⇒ 排除逻辑实为 no-op（见模块头登记）。
    /// 本方法只提供「若恢复排除语义」所需的机制，**不**擅自改变消费点行为。
    #[must_use]
    pub fn contains(&self, mon_name: &str) -> bool {
        self.entries.iter().any(|e| e == mon_name)
    }
}

/// `LoadNoClearMonList` 的**文件半**（`M2Share.cs:1751-1765`）。
///
/// # Errors
/// - 文件不存在 ⇒ [`EnvirError::NoClearMonListMissing`]（复刻 oracle `:1763` 走
///   `SaveToFile` 分支时 `FileNotFoundException` 的穿透；**不是**「空名单」）。
/// - 文件存在但读取失败 ⇒ [`EnvirError::Io`]。
pub fn load_no_clear_mon_list(path: &Path) -> Result<NoClearMonList, EnvirError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // `:1751` File.Exists == false ⇒ `:1763` SaveToFile ⇒ GetEncoding 的 FileStream(Open) 抛。
            return Err(EnvirError::NoClearMonListMissing(path.to_path_buf()));
        }
        Err(source) => {
            return Err(EnvirError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    Ok(NoClearMonList::from_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 部署件 `E:/MirServer/M2GameSvr/Envir/NoClearMonList.txt` 的原始 GBK 字节
    /// （54 字节，CRLF；sha256 `b241e89eb3287116a3ae6ec65978b26f48ea65b0ed5e81bbd4dddb20a409e427`）。
    /// 以字节常量随代码入库（不外挂文件、避免 GBK+CRLF 被 git 文本归一改字节）。
    fn deployed_bytes() -> Vec<u8> {
        vec![
            0xC9, 0xB3, 0xB0, 0xCD, 0xBF, 0xCB, 0xB3, 0xC7, 0xC3, 0xC5, 0x0D,
            0x0A, // 沙巴克城门
            0xC9, 0xB3, 0xB0, 0xCD, 0xBF, 0xCB, 0xD7, 0xF3, 0xB3, 0xC7, 0xC7, 0xBD, 0x0D,
            0x0A, // 沙巴克左城墙
            0xC9, 0xB3, 0xB0, 0xCD, 0xBF, 0xCB, 0xD6, 0xD0, 0xB3, 0xC7, 0xC7, 0xBD, 0x0D,
            0x0A, // 沙巴克中城墙
            0xC9, 0xB3, 0xB0, 0xCD, 0xBF, 0xCB, 0xD3, 0xD2, 0xB3, 0xC7, 0xC7, 0xBD, 0x0D,
            0x0A, // 沙巴克右城墙
        ]
    }

    /// 一次性临时文件路径（各测试用不同后缀，避免并发/复用冲突）。
    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "noclear-montest-{}-{}.txt",
            std::process::id(),
            tag
        ))
    }

    /// 部署件形态**逐字段**：4 行 → 4 条，名字、顺序逐一比对，无尾随幻影空行。
    #[test]
    fn parses_deployed_shape_field_by_field() {
        let reg = NoClearMonList::from_bytes(&deployed_bytes());
        assert_eq!(reg.len(), 4);
        assert!(!reg.is_empty());
        assert_eq!(
            reg.as_slice().to_vec(),
            vec![
                "沙巴克城门".to_string(),
                "沙巴克左城墙".to_string(),
                "沙巴克中城墙".to_string(),
                "沙巴克右城墙".to_string(),
            ],
            "逐字段：名字与行序"
        );
        assert_eq!(reg.get(0), Some("沙巴克城门"));
        assert_eq!(reg.get(3), Some("沙巴克右城墙"));
        assert_eq!(reg.get(4), None);
    }

    /// `<常量>` 与文件名字面量（内容对账门 §4-1 的 `probe = "NoClearMonList.txt"` 依赖它）。
    #[test]
    fn file_name_constant_pins_probe_literal() {
        assert_eq!(NO_CLEAR_MON_LIST_FILE_NAME, "NoClearMonList.txt");
    }

    /// `contains`：精确匹配（非大小写不敏感、非子串），对应 `CompareTo(..) == 0`。
    #[test]
    fn contains_is_exact_match() {
        let reg = NoClearMonList::from_bytes(&deployed_bytes());
        for name in ["沙巴克城门", "沙巴克左城墙", "沙巴克中城墙", "沙巴克右城墙"]
        {
            assert!(reg.contains(name), "应命中 {name}");
        }
        assert!(!reg.contains("沙巴克"), "子串不算命中");
        assert!(!reg.contains("沙巴克城"), "子串不算命中");
        assert!(!reg.contains("沙巴克右城"), "子串不算命中");
        assert!(!reg.contains("MONSTER"), "名单外");
        // ASCII 名字：大小写敏感（与 MiniMap 的 OrdinalIgnoreCase 不同）。
        let ascii = NoClearMonList::parse("Boss\r\n");
        assert!(ascii.contains("Boss"));
        assert!(!ascii.contains("boss"), "精确匹配，大小写敏感");
    }

    /// **牙齿**：空行**保留**为 `""` 条目（`StringList.LoadFromFile` 不滤空行；oracle 照收）。
    /// 若有人「顺手」过滤空行，此断言即红。
    #[test]
    fn blank_lines_kept_as_empty_entries() {
        let reg = NoClearMonList::parse("沙巴克城门\r\n\r\n沙巴克右城墙\r\n");
        assert_eq!(reg.len(), 3);
        assert_eq!(reg.get(1), Some(""));
        assert!(reg.contains(""), "空行 → \"\" 条目，名单里应能查到空串");
    }

    /// **牙齿**：本文件**没有**分号注释过滤（与 `MiniMap.txt` / `Npcs.txt` 不同）
    /// ⇒ 以 `;` 开头的行是**数据**，照样入库。
    #[test]
    fn no_comment_filter_semicolon_is_data() {
        let reg = NoClearMonList::parse(";这不是注释\r\n沙巴克城门\r\n");
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.get(0), Some(";这不是注释"));
    }

    /// 行尾终止符吸收：末尾 `\r\n` 不产生幻影空行；行内空白被 `Trim`。
    #[test]
    fn trailing_terminator_no_phantom_and_trim() {
        assert_eq!(
            parse_no_clear_mon_lines("  沙巴克城门  \r\n\t沙巴克右城墙\t\r\n"),
            vec!["沙巴克城门".to_string(), "沙巴克右城墙".to_string()]
        );
        // 无末尾终止符。
        assert_eq!(
            parse_no_clear_mon_lines("沙巴克城门"),
            vec!["沙巴克城门".to_string()]
        );
        // 纯终止符：单条空行。
        assert_eq!(parse_no_clear_mon_lines("\r\n"), vec![String::new()]);
        // 空文本 → 空表（无行）。
        assert!(parse_no_clear_mon_lines("").is_empty());
    }

    /// `ReadLine` 的三类终止符：`\r\n` / `\r` / `\n` 均可断行。
    #[test]
    fn accepts_crlf_cr_lf_terminators() {
        assert_eq!(
            parse_no_clear_mon_lines("a\nb\rc\r\nd"),
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]
        );
    }

    /// `List.Add` 语义：重复行**保留**（不先去重，与 MiniMap 的 `TryAdd` 不同）。
    #[test]
    fn duplicates_preserved() {
        let reg = NoClearMonList::parse("沙巴克城门\r\n沙巴克城门\r\n");
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.get(0), reg.get(1));
    }

    /// **缺文件边界**：不存在 ⇒ `Err(NoClearMonListMissing)`（oracle 走 `SaveToFile` 分支抛异常），
    /// **不是** `Ok(空名单)`。
    #[test]
    fn missing_file_is_error_not_empty_list() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        let err = load_no_clear_mon_list(&path).expect_err("缺文件必须报错");
        assert!(
            matches!(err, EnvirError::NoClearMonListMissing(_)),
            "应为 NoClearMonListMissing，实为 {err:?}"
        );
        // 再次强调：绝不是静默空名单。
        assert!(load_no_clear_mon_list(&path).is_err());
    }

    /// 文件半往返：把部署件原字节落盘后再 `load`，得到与解析半一致的名单。
    #[test]
    fn loads_real_bytes_from_disk() {
        let path = temp_path("roundtrip");
        std::fs::write(&path, deployed_bytes()).expect("写临时件");
        let reg = load_no_clear_mon_list(&path).expect("加载");
        let _ = std::fs::remove_file(&path);
        assert_eq!(reg.len(), 4);
        assert_eq!(reg.get(2), Some("沙巴克中城墙"));
        assert_eq!(reg, NoClearMonList::from_bytes(&deployed_bytes()));
    }

    /// **改坏必红（牙齿）**：把部署件第 2 行的「左」字节改成「右」后，解析结果必须与原件**不同**
    /// ——证明上面的逐字段断言对内容敏感（不是空转）。
    #[test]
    fn corrupted_fixture_changes_output() {
        let pristine = NoClearMonList::from_bytes(&deployed_bytes());
        let mut broken = deployed_bytes();
        // 第 2 行「左」(D7 F3) → 「右」(D3 D2)：第 2 行自偏移 12 起，其第 4 字在第 6/7 字节
        // ⇒ 绝对偏移 18/19。
        broken[18] = 0xD3;
        broken[19] = 0xD2;
        let mutated = NoClearMonList::from_bytes(&broken);
        assert_eq!(pristine.get(1), Some("沙巴克左城墙"));
        assert_eq!(mutated.get(1), Some("沙巴克右城墙"));
        assert_ne!(mutated, pristine, "改坏一位必须改变解析结果");
    }

    /// 截断的多字节 GBK 不 panic（`encoding_rs` 有损解码），只影响该行内容。
    #[test]
    fn truncated_multibyte_does_not_panic() {
        let mut broken = deployed_bytes();
        broken.truncate(11); // 砍掉第 1 行 CR 中的一半（10 字 + 半 CR）
        let reg = NoClearMonList::from_bytes(&broken);
        assert_eq!(reg.len(), 1, "只剩第 1 行");
        assert!(reg.get(0).unwrap().starts_with("沙巴克城门"));
    }
}
