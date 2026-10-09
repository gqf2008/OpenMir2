//! 存储 trait —— 镜像 C# `src/Storeages/DBSrv.Storage/` 四个接口。
//!
//! 设计取舍：
//! - C# 侧大量「返回码 + `ref bool success`」风格统一收敛为 `Result<_, StorageError>`；
//!   但**错误分支的触发条件必须与 C# 一致**（行为等价优先），不许把 C# 的静默失败改成报错，
//!   也不许把 C# 的报错吞掉。
//! - C# 接口是同步的（DBSrv 内存缓存 + 周期落库）；Rust 实现侧若用 sqlx 异步，
//!   由实现方在 DBSrv 边界做同步包装，**trait 本身保持同步语义**（与 C# 调用约定对齐）。

use crate::types::{CharacterDataInfo, MarketItem, PlayQuick, PlayerRecordData, QueryChr};
use crate::StorageError;

/// 角色记录存储（C# `IPlayRecordStorage`：角色索引表，DBSrv 内存索引 + 落库）。
pub trait PlayRecordStorage {
    /// 加载快速索引（`LoadQuickList`）。
    fn load_quick_list(&mut self) -> Result<(), StorageError>;
    /// 按角色名取索引（`Index`，C# 未命中返回 -1 ⇒ 这里返回 `Ok(None)`）。
    fn index(&self, name: &[u8]) -> Result<Option<i32>, StorageError>;
    /// 按索引取记录（`Get`）。
    fn get(&self, index: i32) -> Result<PlayerRecordData, StorageError>;
    /// 按账号列出角色（`FindByAccount`）。
    fn find_by_account(&self, account: &[u8]) -> Result<Vec<PlayQuick>, StorageError>;
    /// 账号下角色数量（`ChrCountOfAccount`）。
    fn chr_count_of_account(&self, account: &[u8]) -> Result<i32, StorageError>;
    /// 新增记录（`Add`）。
    fn add(&mut self, record: PlayerRecordData) -> Result<bool, StorageError>;
    /// 删除记录索引（`Delete`）。
    fn delete(&mut self, name: &[u8]) -> Result<bool, StorageError>;
    /// 更新记录（`Update`）。
    fn update(&mut self, index: i32, record: &mut PlayerRecordData) -> Result<bool, StorageError>;
}

/// 角色数据存储（C# `IPlayDataStorage`：完整角色存档读写）。
pub trait PlayDataStorage {
    /// 加载快速索引（`LoadQuickList`）。
    fn load_quick_list(&mut self) -> Result<(), StorageError>;
    /// 按名取完整存档（`Get(chrName, ...)`）。
    fn get_by_name(&self, chr_name: &[u8]) -> Result<CharacterDataInfo, StorageError>;
    /// 按索引取完整存档（`Get(nIndex, ...)`，C# 返回 int 状态码；语义对齐后错误走 `Err`）。
    fn get_by_index(&self, index: i32) -> Result<CharacterDataInfo, StorageError>;
    /// 取选角查询记录（`GetQryChar`）。
    fn get_query_char(&self, index: i32) -> Result<QueryChr, StorageError>;
    /// 更新存档（`Update`）。
    fn update(&mut self, chr_name: &[u8], record: &CharacterDataInfo)
        -> Result<bool, StorageError>;
    /// 更新选角查询记录（`UpdateQryChar`）。
    fn update_query_char(&mut self, index: i32, record: &QueryChr) -> Result<bool, StorageError>;
    /// 新增存档（`Add`）。
    fn add(&mut self, record: CharacterDataInfo) -> Result<bool, StorageError>;
    /// 按名删除（`Delete(sChrName)`）。
    fn delete_by_name(&mut self, chr_name: &[u8]) -> Result<bool, StorageError>;
    /// 记录总数（`Count`）。
    fn count(&self) -> Result<i32, StorageError>;
}

/// 内存缓存（C# `ICacheStorage`：DBSrv 玩家数据缓存）。
pub trait CacheStorage {
    /// 加入缓存。
    fn add(&mut self, chr_name: &[u8], data: CharacterDataInfo) -> Result<(), StorageError>;
    /// 取出缓存（未命中返回 `Ok(None)`）。
    fn get(&self, chr_name: &[u8]) -> Result<Option<CharacterDataInfo>, StorageError>;
    /// 删除缓存。
    fn delete(&mut self, chr_name: &[u8]) -> Result<(), StorageError>;
    /// 遍历全部缓存（`QueryCacheData`）。
    fn query_all(&self) -> Result<Vec<CharacterDataInfo>, StorageError>;
}

/// 拍卖行存储（C# `IMarketStorage`）。
pub trait MarketStorage {
    /// 按组查询在售列表（`QueryMarketItems`）。
    fn query_market_items(&self, group_id: u8) -> Result<Vec<MarketItem>, StorageError>;
    /// 查询某出售者在售数量（`QueryMarketItemsCount`）。
    fn query_market_items_count(&self, group_id: u8, sell_who: &[u8]) -> Result<i32, StorageError>;
    /// 保存条目（`SaveMarketItem`）。
    fn save_market_item(
        &mut self,
        item: &MarketItem,
        group_id: u8,
        server_index: u8,
    ) -> Result<bool, StorageError>;
    /// 搜索（`SearchMarketItems`；`item_type`/`item_set` 过滤语义与 C# 一致）。
    fn search_market_items(
        &self,
        group_id: u8,
        market_name: &[u8],
        sell_who: &[u8],
        item_name: &[u8],
        item_type: i16,
        item_set: u8,
    ) -> Result<Vec<MarketItem>, StorageError>;
}
