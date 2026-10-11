//! 轻量记录类型（字段口径以 `sql/mir2_db.sql` 的既有表结构为准，不改表）。
//!
//! **角色完整存档**（C# `CharacterDataInfo`/`CharacterData`/`PlayerRecordData` 等）在
//! [`crate::playerdata`]：那一组与 C# MemoryPack 逐字节兼容，是本 crate 里字段最多的记录类型。

/// 选角列表条目（C# `PlayQuick`）。
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PlayQuick {
    /// 角色名（GB2312 字节）。
    pub name: Vec<u8>,
    /// 选择序号（C# `characters_indexes.SelectID` 兼作状态标记，勿按名字猜语义）。
    pub select_id: i32,
}

/// 查询角色结果（C# `QueryChr`）。
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct QueryChr {
    /// 角色名（GB2312 字节）。
    pub name: Vec<u8>,
}

/// 拍卖行条目（C# `MarketItem`）。
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct MarketItem {
    /// 物品名（GB2312 字节）。
    pub item_name: Vec<u8>,
    /// 出售者。
    pub sell_who: Vec<u8>,
}
