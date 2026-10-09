//! 记录类型占位（字段未冻结，随 D 线/M1 落地；`#[non_exhaustive]` 保证加字段不破坏签名）。
//!
//! 命名与 C# `DBSrv.Storage.Model` / `OpenMir2.Packets.ServerPackets` 对齐，
//! 字段口径以 `sql/mir2_db.sql` 的既有表结构为准（不改表）。

/// 角色记录（C# `PlayerRecordData`，角色索引表条目）。
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct PlayerRecordData {
    /// 角色名（GB2312 字节）。
    pub name: Vec<u8>,
    /// 所属账号。
    pub account: Vec<u8>,
}

/// 角色完整数据（C# `CharacterDataInfo`，含属性/物品/技能/状态等全部存档字段）。
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct CharacterDataInfo {
    /// 角色名（GB2312 字节）。
    pub name: Vec<u8>,
}

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
