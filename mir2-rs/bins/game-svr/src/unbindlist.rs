//! `Envir/UnbindList.txt` 的**宿主装载半 + 查表半**（卡 `openmir2-unbindlist-consumer`）。
//!
//! 解析半在 `crates/data/src/lists.rs`（`LocalDb.LoadUnbindList`，`DB/LocalDB.cs:872-915` 的逐行
//! 1:1，含 `GetValidStrCap` 的越界语义）；本模块只做「文件名字面量 + `File.Exists` 门 + GBK 解码」
//! 的宿主侧接线（与 [`crate::adminlist`] 同款），外加 oracle `M2Share.UnbindList` 的查表半。
//!
//! ## oracle 事实（逐条实读）
//!
//! 1. **装载**：`GameApp.cs:173-179`：
//!    `LogService.Info("正在加载捆装物品信息..."); nCode = GameShare.LocalDb.LoadUnbindList();`
//!    `if (nCode < 0) { LogService.Info($"加载捆装物品信息失败!!! Code: {nCode}"); return; }`
//!    ⇒ 失败会让 `GameApp.Initialize` 提前返回（进程不再继续起服）；成功后
//!    `LogService.Info("加载捆装物品信息成功...")`。
//! 2. **表**：`M2Share.UnbindList = new Dictionary<int, string>`（`M2Share.cs:85`），
//!    键 = `StdItem.Shape`（`int`），值 = 解包物品名（`LoadUnbindList` 的 `TryAdd`）。
//! 3. **查表**：`ClientUseItemsGetUnbindItemName(int nShape)`
//!    （`PlayObject.Operate.cs:578-580`）：
//!    `return M2Share.UnbindList.TryGetValue(nShape, out string result) ? result : string.Empty;`
//!    ⇒ **未命中 = 空串**（不是 null、不是抛错）。
//! 4. **消费**：`ClientUseItems` 的 `StdMode == 31 && AniCount == 0` 支
//!    （`PlayObject.Operate.cs:657-666`）→ `ClientUseItemsGetUnBindItems(name, 6)`（`:582-599`）：
//!    逐件 `CopyToUserItemFromName(name, ref item)`，**失败即 break**（空名命中不到模板 ⇒ 0 件，
//!    但解包件本身照吃、`SM_EAT_OK` 照发）。
//!
//! ## 本片边界（登记）
//!
//! - oracle 在 `nCode < 0` 时**终止起服**；本宿主的装载门与 [`crate::adminlist`] 同款：记 ERR 后
//!   以**空表**继续（部署件 `UnbindList.txt` 全为合法行 ⇒ 该支不可达；malformed 文件的差异登记）。
//! - 行切分缺口沿用 `crates/data/src/lists.rs` 模块头「刻意偏离 4」（孤立 `\r` 不切行；部署件为
//!   `\r\n` ⇒ 不可达）。

use std::collections::BTreeMap;

use mir2_envir::paths::envir_file_path;
use mir2_envir::MapLoader;

/// `M2Share.GetEnvirFilePath("UnbindList.txt")`（`DB/LocalDB.cs:879`）的文件名。
///
/// **就地保留字面量**：recon 内容门（`mir2-rs/tests/parity/coverage/recon_gates.py content`）的
/// 「同一 src 文件里出现文件名字面量 + 装载标记 + 装载点被 server 二进制引用」判据要求字面量
/// 落在本文件（数据层的 `mir2_data::lists::UNBIND_LIST_FILE` 是另一份同值常量）。
pub const UNBIND_LIST_FILE: &str = "UnbindList.txt";

/// `M2Share.UnbindList`（`M2Share.cs:85`）的 Rust 承载：`StdItem.Shape` → 解包物品名。
///
/// 装载顺序 = 文件行序（`BTreeMap` 保持键序，仅供日志/用例稳定打印；oracle `Dictionary` 的
/// 枚举序不参与任何行为）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnbindList {
    /// 解包物品 id（`StdItem.Shape`）→ 物品名。
    pub map: BTreeMap<i32, String>,
}

impl UnbindList {
    /// 表行数（日志口径：`UnbindList.Count`）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// 空表（oracle `Count == 0`）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `ClientUseItemsGetUnbindItemName`（`PlayObject.Operate.cs:578-580`）的逐行 1:1：
    /// 命中返回该行的物品名；**未命中返回空串**（`TryGetValue` 假 ⇒ `string.Empty`）。
    #[must_use]
    pub fn get_unbind_item_name(&self, shape: i32) -> &str {
        self.map.get(&shape).map_or("", String::as_str)
    }
}

/// `LocalDb.LoadUnbindList`（`DB/LocalDB.cs:872-915`）的**文件半**。
///
/// `M2Share.GetEnvirFilePath("UnbindList.txt")` = [`envir_file_path`]；`File.Exists` 假 ⇒
/// `Ok(None)`（oracle 不读文件、返回 0，表保持构造时的空表）。
///
/// 返回 `Err` 的两种情形：
/// - 读失败 / 行级 `CaptureString`（行内前置 `"` ⇒ oracle `IndexOutOfRangeException`，
///   `GameApp.cs:174` 调用点无 try/catch）；
/// - `LoadUnbindList` 返回负的 `nCode`（`n10 <= 0` 的行，`DB/LocalDB.cs:906-910`）——oracle 在
///   `GameApp.cs:175-179` 打印 `加载捆装物品信息失败!!! Code: {nCode}` 并 `return`（终止起服）。
///   调用方据此记 ERR / 停服（本仓见 `lib.rs` 的装载块；部署件无该行 ⇒ 不可达）。
///
/// # Errors
/// 见上。
pub fn load_unbind_list_file(loader: &MapLoader) -> Result<Option<UnbindList>, String> {
    let path = envir_file_path(&loader.base_path, &loader.envir_dir, "", UNBIND_LIST_FILE);
    match mir2_data::lists::load_unbind_list_file(&path) {
        Ok(Some(load)) => {
            if load.result < 0 {
                // oracle `GameApp.cs:177`：`LogService.Info($"加载捆装物品信息失败!!! Code: {nCode}")`
                // 随后 `return`（`Initialize` 提前结束）。
                return Err(format!(
                    "加载捆装物品信息失败!!! Code: {}（{}）",
                    load.result,
                    path.display()
                ));
            }
            Ok(Some(UnbindList { map: load.map }))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(format!("加载 {} 失败：{e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用例级唯一临时目录序号（照 `adminlist.rs`/`guardlist.rs` 的 pid+原子计数口径）。
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    struct Fixture {
        base: std::path::PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    impl Fixture {
        fn new() -> Self {
            let base = std::env::temp_dir().join(format!(
                "gs-unbindlist-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(base.join("Envir")).unwrap();
            Fixture { base }
        }

        fn loader(&self) -> MapLoader {
            MapLoader::new(self.base.clone(), "Map", "Envir")
        }

        fn write(&self, body: &[u8]) {
            std::fs::write(self.base.join("Envir").join(UNBIND_LIST_FILE), body).unwrap();
        }
    }

    /// 三态 ①：**无文件** ⇒ `Ok(None)`（oracle `File.Exists` 假 ⇒ 返回 0、表为空）。
    #[test]
    fn unbind_list_missing_is_none() {
        let f = Fixture::new();
        assert!(load_unbind_list_file(&f.loader()).unwrap().is_none());
    }

    /// 三态 ②：**空文件** ⇒ `Ok(Some(空表))`（文件在 ⇒ oracle 走装载、0 行）。
    #[test]
    fn unbind_list_empty_file_is_empty_table() {
        let f = Fixture::new();
        f.write(b"");
        let list = load_unbind_list_file(&f.loader()).unwrap().expect("Some");
        assert!(list.is_empty());
    }

    /// 正常行（GBK）：`100 解包物甲` / `7<TAB>解包物乙`；`;` 行跳过。
    ///
    /// GBK 字节：解=BD E2 包=B0 FC 物=CE EF 甲=BC D7 / 乙=D2 D2。
    #[test]
    fn unbind_list_normal_lines_map_shape_to_name() {
        let f = Fixture::new();
        f.write(b"100 \xbd\xe2\xb0\xfc\xce\xef\xbc\xd7\r\n;comment\r\n7\t\xbd\xe2\xb0\xfc\xce\xef\xd2\xd2\r\n");
        let list = load_unbind_list_file(&f.loader()).unwrap().expect("Some");
        assert_eq!(list.len(), 2, "';' 行跳过");
        assert_eq!(list.get_unbind_item_name(100), "解包物甲");
        assert_eq!(list.get_unbind_item_name(7), "解包物乙");
        // `ClientUseItemsGetUnbindItemName` 未命中 ⇒ 空串（不是 panic / 不是 None）。
        assert_eq!(list.get_unbind_item_name(8), "");
    }

    /// 重复键**保留首个**（`LoadUnbindList` 的 `TryAdd` 假 ⇒ Warn + continue）。
    #[test]
    fn unbind_list_duplicate_key_keeps_first() {
        let f = Fixture::new();
        f.write(b"5 \xbc\xd7\r\n5 \xd2\xd2\r\n");
        let list = load_unbind_list_file(&f.loader()).unwrap().expect("Some");
        assert_eq!(list.len(), 1);
        assert_eq!(list.get_unbind_item_name(5), "甲");
    }

    /// 首列 `<= 0` ⇒ `LoadUnbindList` 返回 `-i`；oracle `GameApp.cs:175-179` 据此**终止起服**
    /// ⇒ 本包装返回 `Err`（调用方记 ERR / 停服）。
    ///
    /// 红检：把 `load.result < 0` 分支删掉（当成正常表返回）⇒ 本用例必红。
    #[test]
    fn unbind_list_nonpositive_key_is_loader_error() {
        let f = Fixture::new();
        // 第 0 行合法；第 1 行首列 `-3` ⇒ n10 < 0 ⇒ result = -1 并 break
        // （若**第 0 行**非法，`result = -0 = 0` 与成功不可区分——`LocalDB.cs:906-910` 的既有缺陷，照搬。）
        f.write(b"100 \xbc\xd7\r\n-3 \xd2\xd2\r\n");
        let err = load_unbind_list_file(&f.loader()).unwrap_err();
        assert!(
            err.contains("Code: -1"),
            "负 nCode 必须原样进错误串（GameApp.cs:177）：{err}"
        );
    }

    /// 行内前置 `"` ⇒ `CaptureString`（oracle `IndexOutOfRangeException`）⇒ `Err`（不静默吞）。
    #[test]
    fn unbind_list_capture_string_is_error() {
        let f = Fixture::new();
        f.write(b"100 \"abc\r\n");
        assert!(load_unbind_list_file(&f.loader()).is_err());
    }
}
