//! `Exps.conf` 解析，移植源：
//! - `src/Modules/SystemModule/Conf/ExpsConf.cs`（`LoadConfig`）
//! - `src/OpenMir2/Common/ConfigFile.cs`（ini 读取语义）
//!
//! 与 C# 的差异（刻意）：C# `ConfigFile` 对「缺键/值为 0」会**写回文件**；
//! Rust 侧只读，不写回（内容冻结约束，且对拍用夹具副本）。

use std::collections::HashMap;
use std::path::Path;

use mir2_shared::hutil;

use crate::models::MAX_CHANGE_LEVEL;

/// C# `ConfigFile` 的 ini 解析（`Load`，GB2312 编码、大小写不敏感、
/// `;` 行注释、`/* */` 段注释、`;;` 行内注释、键重复取首个）。
#[derive(Debug, Default)]
pub struct IniFile {
    /// section -> (key -> value)，键比较均大小写不敏感（存小写）。
    sections: HashMap<String, HashMap<String, String>>,
}

impl IniFile {
    /// 从 GBK（GB2312）字节加载。解析规则逐条对应 `ConfigFile.Load`。
    pub fn from_gbk_bytes(bytes: &[u8]) -> Self {
        let (text, _, _) = encoding_rs::GBK.decode(bytes);
        let mut ini = IniFile::default();
        let mut large_comment = false;
        let mut is_cur_sec_comment = false;
        let mut cur_sec: Option<String> = None;
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.starts_with(';') {
                continue;
            }
            if line.is_empty() {
                continue;
            }
            if line.starts_with("/*") {
                large_comment = true;
                continue;
            }
            if large_comment {
                if line.starts_with("*/") {
                    large_comment = false;
                }
                continue;
            }
            // C#: (str.Length < 2) || ((str[0] != ';') || (str[1] != ';')) 才继续——
            // 即以 ";;" 开头的行在此被跳过（前面 starts_with(';') 已覆盖）
            if let Some(sec) = section_name(line) {
                if sec.len() >= 2 && sec.starts_with(";;") {
                    is_cur_sec_comment = true;
                    continue;
                }
                is_cur_sec_comment = false;
                let key = sec.to_lowercase();
                if ini.sections.contains_key(&key) {
                    // C#: 段重复直接结束加载（goto Label_02AE）
                    break;
                }
                ini.sections.insert(key.clone(), HashMap::new());
                cur_sec = Some(key);
                continue;
            }
            if is_cur_sec_comment {
                continue;
            }
            // 行内 ";;" 注释截断
            let line = match line.find(";;") {
                Some(idx) => line[..idx].trim(),
                None => line,
            };
            let sec_key = cur_sec.clone().unwrap_or_default();
            let sec = ini.sections.entry(sec_key).or_default();
            if let Some((k, v)) = split_key_val(line) {
                sec.entry(k).or_insert(v);
            }
        }
        ini
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_lowercase())
            .and_then(|sec| sec.get(&key.to_lowercase()))
            .map(String::as_str)
    }

    /// C# `ReadWriteString` 的读侧（缺键/空值给默认值；不写回）。
    pub fn read_string<'a>(&'a self, section: &str, key: &str, def: &'a str) -> &'a str {
        match self.get(section, key) {
            Some(v) if !v.is_empty() => v,
            _ => def,
        }
    }

    /// C# `ReadWriteInteger` 的读侧。
    pub fn read_integer(&self, section: &str, key: &str, def: i32) -> i32 {
        match self.get(section, key) {
            Some(v) if !v.is_empty() => v.trim().parse::<i32>().unwrap_or(def),
            _ => def,
        }
    }

    /// C# `ReadBool`：`是` / `YES` / `1` / `TRUE`（大写后比较）。
    pub fn read_bool(&self, section: &str, key: &str, def: bool) -> bool {
        match self.get(section, key) {
            Some(v) if !v.is_empty() => {
                let up = v.to_uppercase();
                up == "是" || up == "YES" || up == "1" || up == "TRUE"
            }
            _ => def,
        }
    }
}

/// C# `GetSecString`：`[xxx]` 取段名；非 `[` 开头返回 None。
fn section_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let end = rest.find(']')?;
    if end == 0 {
        return None;
    }
    Some(rest[..end].trim())
}

/// C# `SplitKeyVal`：按第一个 `=` 切，键值各自 trim；无 `=` 返回 None。
fn split_key_val(line: &str) -> Option<(String, String)> {
    let pos = line.find('=')?;
    Some((
        line[..pos].trim().to_lowercase(),
        line[pos + 1..].trim().to_string(),
    ))
}

/// `Exps.conf` 的 `[Exp]` 段（`SystemShare.Config` 中 D 线相关字段）。
#[derive(Clone, Debug)]
pub struct ExpsConfig {
    /// C# `SystemShare.Config.NeedExps`（`int[1000]`）。
    ///
    /// 逐级语义照 `ExpsConf.LoadConfig`：`Level{i}` 缺失或解析为 0 时，
    /// C# 走 `OldNeedExps[i]`（恒全 0）→ 最终 0；故缺失级 = 0。
    pub need_exps: [i32; MAX_CHANGE_LEVEL],
    pub limit_exp_level: i32,
    pub limit_exp_value: i32,
    pub kill_mon_exp_multiple: i32,
    pub high_level_kill_mon_fix_exp: bool,
    pub high_level_group_fix_exp: bool,
    pub use_fix_exp: bool,
    pub mon_del_hp_to_exp: bool,
    pub base_exp: i32,
    pub add_exp: i32,
    pub mon_hp_to_exp_level: i32,
    pub mon_hp_to_exp_max: i32,
}

impl ExpsConfig {
    /// 从 ini 读取，对应 `ExpsConf.LoadConfig`（默认值取 `GameSvrConf` 初始值）。
    pub fn from_ini(ini: &IniFile) -> Self {
        let mut need_exps = [0i32; MAX_CHANGE_LEVEL];
        for (i, slot) in need_exps.iter_mut().enumerate() {
            let key = format!("Level{i}");
            let load_string = ini.read_string("Exp", &key, "");
            let load_integer = hutil::str_to_int(load_string, 0);
            if load_integer != 0 {
                *slot = load_integer;
            }
            // load_integer == 0 分支：C# 回退 OldNeedExps[i]（恒 0），结果即 0
        }
        ExpsConfig {
            need_exps,
            // 默认值与 GameSvrConf.cs:1828-1839 一致（缺键时生效）
            limit_exp_level: ini.read_integer("Exp", "LimitExpLevel", 1000),
            limit_exp_value: ini.read_integer("Exp", "LimitExpValue", 1),
            kill_mon_exp_multiple: ini.read_integer("Exp", "KillMonExpMultiple", 1),
            high_level_kill_mon_fix_exp: ini.read_bool("Exp", "HighLevelKillMonFixExp", false),
            high_level_group_fix_exp: ini.read_bool("Exp", "HighLevelGroupFixExp", true),
            use_fix_exp: ini.read_bool("Exp", "UseFixExp", true),
            mon_del_hp_to_exp: ini.read_bool("Exp", "MonDelHptoExp", false),
            base_exp: ini.read_integer("Exp", "BaseExp", 100_000_000),
            add_exp: ini.read_integer("Exp", "AddExp", 1_000_000),
            mon_hp_to_exp_level: ini.read_integer("Exp", "MonHptoExpLevel", 100),
            mon_hp_to_exp_max: ini.read_integer("Exp", "MonHptoExpmax", 1),
        }
    }

    /// 从 GBK 文件路径加载。
    pub fn from_file(path: &Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_ini(&IniFile::from_gbk_bytes(&bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_basic() {
        let ini = IniFile::from_gbk_bytes(b"[Exp]\nLevel1=10\nlevel2=15 ;x\n; comment\nLevel3=\n");
        assert_eq!(ini.read_integer("Exp", "Level1", 0), 10);
        // 键大小写不敏感；行内 " ;x" 不是 ";;" 注释 → 值含 "15 ;x" → TryParse 失败?
        // C# 值是 "15 ;x"，int.TryParse 失败 → default。注意这一语义！
        assert_eq!(ini.read_integer("Exp", "level2", -1), -1);
        assert_eq!(ini.read_integer("Exp", "Level3", 7), 7); // 空值 → default
        assert_eq!(ini.read_integer("Exp", "Missing", 9), 9);
    }

    #[test]
    fn ini_inline_comment() {
        let ini = IniFile::from_gbk_bytes(b"[Exp]\nA=1 ;; note\n");
        assert_eq!(ini.read_integer("Exp", "A", 0), 1);
    }

    #[test]
    fn ini_bool_parse() {
        // "是" 的 GBK 编码为 0xCA 0xC7
        let ini = IniFile::from_gbk_bytes(b"[Exp]\nA=1\nB=yes\nC=true\nD=\xCA\xC7\nE=0\n");
        assert!(ini.read_bool("Exp", "A", false));
        assert!(ini.read_bool("Exp", "B", false));
        assert!(ini.read_bool("Exp", "C", false));
        assert!(ini.read_bool("Exp", "D", false));
        assert!(!ini.read_bool("Exp", "E", true));
        assert!(ini.read_bool("Exp", "Missing", true));
    }

    #[test]
    fn need_exps_missing_is_zero() {
        let ini = IniFile::from_gbk_bytes(b"[Exp]\nLevel1=100\n");
        let cfg = ExpsConfig::from_ini(&ini);
        assert_eq!(cfg.need_exps[0], 0);
        assert_eq!(cfg.need_exps[1], 100);
        assert_eq!(cfg.need_exps[2], 0);
    }
}
