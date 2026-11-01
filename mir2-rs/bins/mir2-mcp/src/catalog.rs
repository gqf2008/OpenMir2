//! GM 命令目录（185 条）：把仓库内三份源数据合成一份只读目录。
//!
//! | 字段 | 来源 |
//! |---|---|
//! | 现网名 / 权限 / 参数个数 / 实现态 | `bins/game-svr/src/gmcmd.rs` 的 `COMMANDS` 常量表 |
//! | 族 / 代码类 / 出厂名 / 效应 / 主要帧 | `tests/parity/m3/design-gm-commands-20261023.md` §4.3 |
//! | 中文描述 / 帮助文本（oracle `[Command]` 属性） | `src/Modules/GameCommand/**/*.cs`（build.rs 提取） |
//! | 总条数 / 违例明细 | `tests/parity/coverage/gm_registry.json` |
//!
//! 三份源码在**编译期**用 `include_str!` 打进二进制（`mir2-mcp` 可脱离仓库运行），
//! 运行期解析并做交叉校验：`gmcmd` 表 185 条 ⇔ 设计件 §4.3 表 185 行（名字集合相等），
//! 且 `gm_list` 调用时还与 registry 的 `summary.table_total` 比对。
//!
//! 为什么不直接读 registry 的 `commands` 数组：该数组只收 **`Imp::Unimplemented`** 的 9 条
//! （`gm_registry.py` 的 `rows` 循环显式 `if r["imp"] != "Unimplemented": continue`），
//! 全量 185 条只在 `summary.table_total` / `crosscheck.model_total` 里出现。

use std::collections::BTreeMap;

use serde_json::Value;

/// Rust 命令表（185 条），编译期嵌入。
pub const GMCMD_SRC: &str = include_str!("../../../bins/game-svr/src/gmcmd.rs");
/// 设计件 §4.3 全量清点表，编译期嵌入。
pub const DESIGN_DOC: &str =
    include_str!("../../../tests/parity/m3/design-gm-commands-20261023.md");
/// GM 可达性登记表，编译期嵌入（`--registry` 可覆盖为实读文件）。
pub const REGISTRY_JSON: &str = include_str!("../../../tests/parity/coverage/gm_registry.json");
/// build.rs 从 C# `[Command]` 属性提取的 `出厂名 -> {desc, help}`。
const CSHARP_ATTRS: &str = include_str!(concat!(env!("OUT_DIR"), "/gm_csharp_attrs.json"));

/// 目录中的一条命令。
#[derive(Debug, Clone)]
pub struct GmEntry {
    /// 现网拼写（部署 `Command.conf` 覆盖后的 `CommandMaps` key）。
    pub name: String,
    /// 设计件分族（如「传送/召唤」）。
    pub family: String,
    /// oracle 命令类名（如 `TingCommand`）。
    pub handler_class: String,
    /// 出厂名（`[Command]` 属性名 / `Command.conf` 键）；与 `name` 不同即被配置改写过。
    pub factory_name: String,
    /// 生效 `PermissionMin`（`GameCommand.cs:37` 的门）。
    pub perm_min: u32,
    /// `Execute` 形参个数。
    pub params: u32,
    /// Rust `Imp` 变体名。
    pub imp: String,
    /// `imp != Unimplemented`。
    pub implemented: bool,
    /// 设计件「效应」列（N/S/F/C/W/B/D 组合）。
    pub effect: String,
    /// 设计件「帧」列。
    pub frames: String,
    /// oracle `[Command]` 第二参（描述）。
    pub desc: String,
    /// oracle `[Command]` 第三参（帮助文本；非字符串则为空）。
    pub help: String,
}

/// 全量命令目录。
#[derive(Debug)]
pub struct Catalog {
    pub entries: Vec<GmEntry>,
}

impl Catalog {
    /// 从编译期嵌入的三份源数据加载。
    ///
    /// # Errors
    /// 任一源解析失败、或 gmcmd 表与设计件表的条数/名字集合不一致。
    pub fn load() -> Result<Self, String> {
        Self::load_from(GMCMD_SRC, DESIGN_DOC, CSHARP_ATTRS)
    }

    /// 解析指定的三份文本（自测可直接喂夹具）。
    ///
    /// # Errors
    /// 同 [`Catalog::load`]。
    pub fn load_from(gmcmd: &str, design: &str, csharp_attrs: &str) -> Result<Self, String> {
        let rust_table = parse_gmcmd(gmcmd)?;
        let design_rows = parse_design(design)?;
        if rust_table.len() != design_rows.len() {
            return Err(format!(
                "gmcmd 表 {} 条 ≠ 设计件 §4.3 {} 行",
                rust_table.len(),
                design_rows.len()
            ));
        }
        let mut by_name: BTreeMap<&str, &DesignRow> = BTreeMap::new();
        for row in &design_rows {
            if by_name.insert(row.name.as_str(), row).is_some() {
                return Err(format!("设计件 §4.3 现网名重复：{}", row.name));
            }
        }
        let attrs: BTreeMap<String, CsharpAttr> =
            serde_json::from_str(csharp_attrs).map_err(|e| format!("C# 属性表解析失败：{e}"))?;

        let mut entries = Vec::with_capacity(rust_table.len());
        for raw in rust_table {
            let row = by_name
                .remove(raw.name.as_str())
                .ok_or_else(|| format!("设计件 §4.3 缺 `{}` 行", raw.name))?;
            let attr = attrs
                .get(&row.factory_name)
                .ok_or_else(|| format!("C# `[Command]` 属性表缺出厂名 `{}`", row.factory_name))?;
            entries.push(GmEntry {
                name: raw.name,
                family: row.family.clone(),
                handler_class: row.handler_class.clone(),
                factory_name: row.factory_name.clone(),
                perm_min: raw.perm_min,
                params: raw.params,
                implemented: raw.imp != "Unimplemented",
                imp: raw.imp,
                effect: row.effect.clone(),
                frames: row.frames.clone(),
                desc: attr.desc.clone(),
                help: attr.help.clone(),
            });
        }
        if let Some(leftover) = by_name.keys().next() {
            return Err(format!("gmcmd 表缺设计件里的 `{leftover}`"));
        }
        Ok(Self { entries })
    }

    /// 按现网名查找（`OrdinalIgnoreCase`，与 `CommandMaps` 同口径）。
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&GmEntry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CsharpAttr {
    #[serde(default)]
    desc: String,
    #[serde(default)]
    help: String,
}

#[derive(Debug)]
struct RustRow {
    name: String,
    perm_min: u32,
    params: u32,
    imp: String,
}

#[derive(Debug)]
struct DesignRow {
    family: String,
    handler_class: String,
    name: String,
    factory_name: String,
    effect: String,
    frames: String,
}

/// 解析 `gmcmd.rs` 的 `COMMANDS` 常量表。
///
/// 表项字段间允许夹注释（`ShowDenyCharNameLogon` / `MasterName` 两处），因此按
/// `CommandDef {` 分块后逐字段查找，而不是逐行匹配。
fn parse_gmcmd(src: &str) -> Result<Vec<RustRow>, String> {
    const HEAD: &str = "pub const COMMANDS: &[CommandDef] = &[";
    let body_start = src.find(HEAD).ok_or("gmcmd.rs 未找到 COMMANDS 表")? + HEAD.len();
    let rest = &src[body_start..];
    let table_end = rest.find("\n];").ok_or("gmcmd.rs 的 COMMANDS 表未闭合")?;
    let table = &rest[..table_end];

    let mut rows = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = table[from..].find("CommandDef {") {
        let open = from + rel + "CommandDef {".len();
        let next = table[open..]
            .find("CommandDef {")
            .map_or(table.len(), |x| open + x);
        let body = &table[open..next];
        rows.push(parse_entry(body)?);
        from = open;
    }
    Ok(rows)
}

fn parse_entry(body: &str) -> Result<RustRow, String> {
    Ok(RustRow {
        name: field_str(body, "name:")?,
        perm_min: field_u32(body, "perm_min:")?,
        params: field_u32(body, "params:")?,
        imp: field_ident(body, "imp: Imp::")?,
    })
}

fn field_after<'a>(body: &'a str, key: &str) -> Result<&'a str, String> {
    let at = body
        .find(key)
        .ok_or_else(|| format!("COMMANDS 表项缺字段 `{key}`：{body:.80}"))?
        + key.len();
    Ok(body[at..].trim_start())
}

fn field_str(body: &str, key: &str) -> Result<String, String> {
    let rest = field_after(body, key)?;
    let rest = rest
        .strip_prefix('"')
        .ok_or_else(|| format!("字段 `{key}` 不是字符串：{rest:.40}"))?;
    let end = rest
        .find('"')
        .ok_or_else(|| format!("字段 `{key}` 字符串未闭合"))?;
    Ok(rest[..end].to_string())
}

fn field_u32(body: &str, key: &str) -> Result<u32, String> {
    let rest = field_after(body, key)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits
        .parse()
        .map_err(|_| format!("字段 `{key}` 不是整数：{rest:.24}"))
}

fn field_ident(body: &str, key: &str) -> Result<String, String> {
    let rest = field_after(body, key)?;
    let ident: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() {
        return Err(format!("字段 `{key}` 缺标识符：{rest:.24}"));
    }
    Ok(ident)
}

/// 解析设计件 §4.3 的全量清点表（`| # | 族 | 代码类 | 现网名 | 出厂名 | 权限 | 参数 | 效应 | 帧 |`）。
fn parse_design(src: &str) -> Result<Vec<DesignRow>, String> {
    let start = src.find("### 4.3").ok_or("设计件未找到 §4.3 全量清点表")?;
    let mut rows = Vec::new();
    let mut started = false;
    for line in src[start..].lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            if started {
                break;
            }
            continue;
        }
        let cells: Vec<&str> = line.split('|').collect();
        if cells.len() < 10 {
            continue;
        }
        let index = cells[1].trim();
        if index.is_empty() || !index.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        started = true;
        rows.push(DesignRow {
            family: plain(cells[2]),
            handler_class: backticked(cells[3])?,
            name: backticked(cells[4])?,
            factory_name: backticked(cells[5])?,
            effect: plain(cells[8]),
            frames: plain(cells[9]),
        });
    }
    if rows.is_empty() {
        return Err("设计件 §4.3 表解析出 0 行".to_string());
    }
    Ok(rows)
}

fn plain(cell: &str) -> String {
    cell.trim().to_string()
}

fn backticked(cell: &str) -> Result<String, String> {
    let cell = cell.trim();
    cell.strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .map(ToString::to_string)
        .ok_or_else(|| format!("设计件 §4.3 单元格不是反引号包裹：{cell:?}"))
}

/// registry JSON 的读侧视图。
#[derive(Debug)]
pub struct Registry {
    pub format: String,
    pub table_total: u64,
    /// `summary` 原样。
    pub summary: Value,
    /// `commands` 数组按 `cmd` 索引（只有 `Unimplemented` 的 9 条）。
    pub details: BTreeMap<String, Value>,
}

impl Registry {
    /// 解析 `gm_registry.json`。
    ///
    /// # Errors
    /// JSON 非法、缺 `summary.table_total`、或格式戳不是 `gm-registry/2`。
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| format!("gm_registry.json 解析失败：{e}"))?;
        let format = value
            .get("format")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if format != "gm-registry/2" {
            return Err(format!(
                "gm_registry.json 格式戳不是 gm-registry/2（实际 `{format}`）"
            ));
        }
        let summary = value.get("summary").cloned().unwrap_or(Value::Null);
        let table_total = summary
            .get("table_total")
            .and_then(Value::as_u64)
            .ok_or("gm_registry.json 缺 summary.table_total")?;
        let mut details = BTreeMap::new();
        if let Some(Value::Array(rows)) = value.get("commands") {
            for row in rows {
                if let Some(cmd) = row.get("cmd").and_then(Value::as_str) {
                    details.insert(cmd.to_string(), row.clone());
                }
            }
        }
        Ok(Self {
            format,
            table_total,
            summary,
            details,
        })
    }
}

/// 目录的统计（仅自测用）。
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogCounts {
    pub total: usize,
    pub implemented: usize,
    pub unimplemented: usize,
    pub perm0: usize,
    pub families: usize,
}

#[cfg(test)]
impl Catalog {
    #[must_use]
    pub fn counts(&self) -> CatalogCounts {
        use std::collections::BTreeSet;
        let families: BTreeSet<&str> = self.entries.iter().map(|e| e.family.as_str()).collect();
        CatalogCounts {
            total: self.entries.len(),
            implemented: self.entries.iter().filter(|e| e.implemented).count(),
            unimplemented: self.entries.iter().filter(|e| !e.implemented).count(),
            perm0: self.entries.iter().filter(|e| e.perm_min == 0).count(),
            families: families.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::load().expect("嵌入源必须可加载")
    }

    #[test]
    fn embedded_catalog_has_185_entries() {
        let catalog = catalog();
        let counts = catalog.counts();
        assert_eq!(counts.total, 185, "gmcmd 表条数");
        assert_eq!(counts.implemented, 176, "Implemented");
        assert_eq!(counts.unimplemented, 9, "Unimplemented");
        assert_eq!(counts.perm0, 5, "perm 0 恰好 5 条");
    }

    #[test]
    fn catalog_entries_have_family_and_handler() {
        let catalog = catalog();
        for entry in &catalog.entries {
            assert!(!entry.family.is_empty(), "{} 缺族", entry.name);
            assert!(!entry.handler_class.is_empty(), "{} 缺代码类", entry.name);
            assert!(!entry.factory_name.is_empty(), "{} 缺出厂名", entry.name);
        }
    }

    #[test]
    fn catalog_matches_registry_table_total() {
        let catalog = catalog();
        let registry = Registry::parse(REGISTRY_JSON).expect("registry");
        assert_eq!(
            catalog.entries.len() as u64,
            registry.table_total,
            "目录条数必须等于 gm_registry.json 的 table_total"
        );
    }

    #[test]
    fn catalog_help_comes_from_csharp_attributes() {
        let catalog = catalog();
        let ting = catalog.find("Tingwa").expect("现网名 Tingwa");
        assert_eq!(ting.handler_class, "TingCommand");
        assert_eq!(ting.desc, "将指定人物随机传送(支持权限分配)");
        assert_eq!(ting.family, "传送/召唤");

        let clear = catalog.find("清理垃圾").expect("清理垃圾");
        assert_eq!(clear.desc, "清理包裹物品");
        assert_eq!(clear.help, "人物名称");
    }

    #[test]
    fn implemented_flag_tracks_imp_variant() {
        let catalog = catalog();
        let unimplemented: Vec<&str> = catalog
            .entries
            .iter()
            .filter(|e| !e.implemented)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(unimplemented.len(), 9);
        for name in &unimplemented {
            let entry = catalog.find(name).expect("命中");
            assert_eq!(entry.imp, "Unimplemented");
        }
        let time = catalog.find("时间").expect("时间");
        assert!(time.implemented);
        assert_eq!(time.perm_min, 0);
    }

    #[test]
    fn parser_rejects_mismatched_design_rows() {
        let attrs = "{}";
        let err = Catalog::load_from(GMCMD_SRC, "### 4.3\n", attrs).unwrap_err();
        assert!(err.contains("0 行"), "{err}");
    }
}
