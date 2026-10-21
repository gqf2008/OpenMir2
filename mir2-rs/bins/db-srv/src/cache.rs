//! 内存缓存（C# `DBSrv.Storage.Impl.CacheStorageService`，`ICacheStorage` 的 MySQL 无关实现）。
//!
//! 逐条对齐 C#：
//! - 键是**解码后的 String**，折叠用 `gbk::fold_text`（C# `ConcurrentDictionary<string,…>`
//!   `StringComparer.OrdinalIgnoreCase` 的已登记近似，D-2/B-103；E-7 要求名字键不走 GBK 字节）；
//! - **`Add` 只在键已存在时替换**（`if (_cacheMap.ContainsKey(sChrName)) _cacheMap[sChrName] = …`）。
//!   这是 C# 的既有行为：缓存由空开始、没有任何路径往里 `TryAdd`，因此 **`Add` 对不存在的键是 no-op**，
//!   缓存实际恒为空（见 `docs/M1-S4b-DBSrv-6000-交付说明.md` 的 oracle 记录）。如实照搬，不许顺手修。
//! - `Get` 未命中返回 `(false, None)`；`Delete` 命中才删。

use std::collections::HashMap;

use mir2_storage::playerdata::CharacterDataInfo;

use crate::gbk::fold_text;

/// 玩家存档内存缓存（C# `_cacheMap`）。
#[derive(Default)]
pub struct CacheStore {
    /// 折叠键（`ChrName` 的 String 折叠）→ 存档。
    map: HashMap<String, CharacterDataInfo>,
}

impl CacheStore {
    /// C# `Add`：**仅当键已存在时替换**（既有怪行为，见模块文档）。
    pub fn add(&mut self, chr_name: &str, data: CharacterDataInfo) {
        let key = fold_text(chr_name);
        if self.map.contains_key(&key) {
            self.map.insert(key, data);
        }
    }

    /// C# `Get(chrName, out exist)`。
    #[must_use]
    pub fn get(&self, chr_name: &str) -> Option<CharacterDataInfo> {
        self.map.get(&fold_text(chr_name)).cloned()
    }

    /// C# `Delete`。
    pub fn delete(&mut self, chr_name: &str) {
        self.map.remove(&fold_text(chr_name));
    }

    /// C# `QueryCacheData`：全部值的快照（C# 是 `ConcurrentDictionary.Values` 的枚举器，
    /// 顺序不确定；这里返回 `Vec` 快照，顺序同样不作契约）。
    #[must_use]
    pub fn query_all(&self) -> Vec<CharacterDataInfo> {
        self.map.values().cloned().collect()
    }

    /// 当前缓存条数（C# 无此 API；仅单测用）。
    #[cfg(test)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str) -> CharacterDataInfo {
        let mut v = CharacterDataInfo::csharp_new();
        v.header.set_name(Some(name.to_string()));
        v
    }

    /// C# `Add` 对**不存在的键**是 no-op（既有怪行为，红检锚点）。
    #[test]
    fn add_of_missing_key_is_noop() {
        let mut c = CacheStore::default();
        c.add("aaa", info("aaa"));
        assert_eq!(c.len(), 0, "C# 只在键已存在时替换 ⇒ 首次 Add 必须不写入");
        assert!(c.get("aaa").is_none());
    }

    /// 已存在的键才会被替换（构造一个已存在的键来验证替换语义）。
    #[test]
    fn add_replaces_existing_key_case_insensitively() {
        let mut c = CacheStore::default();
        c.map.insert(fold_text("aaa"), info("aaa"));
        c.add("AAA", info("AAA"));
        assert_eq!(c.len(), 1);
        assert_eq!(c.get("aaa").unwrap().header.name.as_deref(), Some("AAA"));
        c.delete("AaA");
        assert_eq!(c.len(), 0);
    }

    /// `query_all` 是值快照。
    #[test]
    fn query_all_snapshots_values() {
        let mut c = CacheStore::default();
        c.map.insert(fold_text("a"), info("a"));
        c.map.insert(fold_text("b"), info("b"));
        assert_eq!(c.query_all().len(), 2);
    }
}
