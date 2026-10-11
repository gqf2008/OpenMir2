//! 内存缓存（C# `DBSrv.Storage.Impl.CacheStorageService`，`ICacheStorage` 的 MySQL 无关实现）。
//!
//! 逐条对齐 C#：
//! - 键比较**大小写不敏感**（C# `ConcurrentDictionary<string,…>(StringComparer.OrdinalIgnoreCase)`，
//!   这里用 `gbk::fold_key` 做 ASCII 折叠，与 DBSrv 其余查表键同口径）；
//! - **`Add` 只在键已存在时替换**（`if (_cacheMap.ContainsKey(sChrName)) _cacheMap[sChrName] = …`）。
//!   这是 C# 的既有行为：缓存由空开始、没有任何路径往里 `TryAdd`，因此 **`Add` 对不存在的键是 no-op**，
//!   缓存实际恒为空（见 `docs/M1-S4b-DBSrv-6000-交付说明.md` 的 oracle 记录）。如实照搬，不许顺手修。
//! - `Get` 未命中返回 `(false, None)`；`Delete` 命中才删。

use std::collections::HashMap;

use mir2_storage::playerdata::CharacterDataInfo;

use crate::gbk::fold_key;

/// 玩家存档内存缓存（C# `_cacheMap`）。
#[derive(Default)]
pub struct CacheStore {
    /// 折叠键（`ChrName`）→ 存档。
    map: HashMap<Vec<u8>, CharacterDataInfo>,
}

impl CacheStore {
    /// C# `Add`：**仅当键已存在时替换**（既有怪行为，见模块文档）。
    pub fn add(&mut self, chr_name: &[u8], data: CharacterDataInfo) {
        let key = fold_key(chr_name);
        if self.map.contains_key(&key) {
            self.map.insert(key, data);
        }
    }

    /// C# `Get(chrName, out exist)`。
    #[must_use]
    pub fn get(&self, chr_name: &[u8]) -> Option<CharacterDataInfo> {
        self.map.get(&fold_key(chr_name)).cloned()
    }

    /// C# `Delete`。
    pub fn delete(&mut self, chr_name: &[u8]) {
        self.map.remove(&fold_key(chr_name));
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
        c.add(b"aaa", info("aaa"));
        assert_eq!(c.len(), 0, "C# 只在键已存在时替换 ⇒ 首次 Add 必须不写入");
        assert!(c.get(b"aaa").is_none());
    }

    /// 已存在的键才会被替换（构造一个已存在的键来验证替换语义）。
    #[test]
    fn add_replaces_existing_key_case_insensitively() {
        let mut c = CacheStore::default();
        c.map.insert(fold_key(b"aaa"), info("aaa"));
        c.add(b"AAA", info("AAA"));
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(b"aaa").unwrap().header.name.as_deref(), Some("AAA"));
        c.delete(b"AaA");
        assert_eq!(c.len(), 0);
    }

    /// `query_all` 是值快照。
    #[test]
    fn query_all_snapshots_values() {
        let mut c = CacheStore::default();
        c.map.insert(fold_key(b"a"), info("a"));
        c.map.insert(fold_key(b"b"), info("b"));
        assert_eq!(c.query_all().len(), 2);
    }
}
