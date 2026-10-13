//! 仓库会话态与存取档口径（卡 `openmir2-m3-storage-persist`，M3⑤-实现 5 第①片）。
//!
//! 本片只做 oracle 的「**仓库会话态**（`StorageItemList` + 39 上限 + 取回锁）+ **存档落库**」两条：
//! 会话里的仓库列表、读档/存档往返、`IsCanGetBackItem` 锁、39 上限常量。**两条 `CM_*` 收包派发
//! 与帧面**（`CM_USERSTORAGEITEM(1031)`/`CM_USERTAKEBACKSTORAGEITEM(1032)` + `SM_STORAGE_OK`/
//! `SM_SAVEITEMLIST` …）归 ⑤-4 第②片（`bins/game-svr/src/shop.rs` 模块头已登记该归属）。
//!
//! # oracle 锚点（全部 `src/` 行号）
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`StorageState::new`]（列表空 + 锁开） | `PlayObject.cs:878`（`StorageItemList = new List<UserItem>()`）、`:956`（`IsCanGetBackItem = true`） |
//! | [`StorageState::can_store`]（39 上限） | `PlayObject.Operate.cs:1629`（`StorageItemList.Count < 39`） |
//! | [`StorageState::is_can_get_back_item`]（取回锁） | `PlayObject.cs:741`；`PlayObject.Operate.cs:1669`（`!IsCanGetBackItem` ⇒ 直接 return、无帧） |
//! | [`load_from_record`]（读档） | `WorldServer.cs:1213-1227`：跳过 `null` 槽；仅 `Index > 0`；`ToClientItem()`；保持数组次序 |
//! | [`storage_items_for_save`]（存档） | `WorldServer.cs:1359-1376`：新 `ServerUserItem[50]`；`StorageItemList[i]` → 槽 i；空槽填 `HUtil32.DelfautItem` |
//! | 落库（差量表） | `Storeages/DBSrv.Storage.MySQL/PlayDataStorage.Save.cs:357-360`（`characters_storageitem`） |
//!
//! `MakeSaveRcd`（`WorldServer.cs:1231`）的第一行是 `CharacterData playerInfo = humanRcd.Data;`
//! 而 `humanRcd` 是**新建**的 `CharacterDataInfo` ⇒ `StorageItems` 恒为 `new CharacterData()`
//! 里的 `new ServerUserItem[50]`（`PlayerDataInfo.cs:117`）⇒ 存档数组长度**恒 50**（不是记录原长），
//! 且每次存档都从空槽开始、不会被上一次的旧物品顶掉（[`storage_items_for_save`] 照此实现）。
//!
//! # 本片未建模（登记，不静默跳过）
//! - `IsCanGetBackItem` 的密码锁降级：oracle 在 `PasswordLockSystem && IsPasswordLocked` 时按
//!   `Config.LockGetBackItemAction` 把它置 false（`PlayObject.cs:1451-1454`，由 `@SetPassword`/
//!   `@Lock`/`@Unlock` 切换，`PlayObject.Chat.cs:264/324`）。本宿主未装载密码锁脚本与配置
//!   ⇒ 会话恒初生 true（= oracle 未加锁态），锁为 false 的路径**不可达**（登记）。
//! - `SendSaveItemList` 的分页下行（每页 50，`SM_SAVEITEMLIST`，`PlayObject.Base.cs:1590`）：
//!   帧面归 ⑤-4 第②片。

use crate::record::ServerUserItem;
use crate::{default_server_item, server_item_from_bag};
use game_svr::bag::BagItem;

/// oracle `PlayObject.Operate.cs:1629` 的字面量：`StorageItemList.Count < 39` 才允许存入。
///
/// 本片的命令层（`CM_USERSTORAGEITEM`）归 ⑤-4 第②片 ⇒ 生产路径暂无消费方。
#[allow(dead_code)]
pub const STORAGE_MAX_ITEMS: usize = 39;

/// oracle `WorldServer.cs:1362` / `PlayerDataInfo.cs:117` 的存档数组长度：`new ServerUserItem[50]`。
pub const STORAGE_SAVE_SLOTS: usize = 50;

/// `PlayObject` 上的仓库会话态（`StorageItemList` + `IsCanGetBackItem`）。
///
/// 与 `PlayObject` 同生命周期：进世界时读档建立、断线/`MakeGhost` 时随会话移除。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageState {
    /// `PlayObject.StorageItemList`（`IList<UserItem>`，顺序敏感；元素为客户端物品子集）。
    pub items: Vec<BagItem>,
    /// `PlayObject.IsCanGetBackItem`（true = 可取回；false = 仓库已加锁）。
    pub is_can_get_back_item: bool,
}

impl Default for StorageState {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageState {
    /// `PlayObject.cs:878` + `:956`：空列表 + 锁开。
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            is_can_get_back_item: true,
        }
    }

    /// `PlayObject.Operate.cs:1629`：`StorageItemList.Count < 39`。
    ///
    /// 生产消费方是 ⑤-4 第②片的 `CM_USERSTORAGEITEM`（本片先钉住口径与边界）。
    #[allow(dead_code)]
    #[must_use]
    pub fn can_store(&self) -> bool {
        self.items.len() < STORAGE_MAX_ITEMS
    }
}

/// `ServerUserItem.ToClientItem()`（`ServerUserItem.cs:35-50`）→ 本宿主的 `BagItem` 子集。
///
/// `ToClientItem` 只搬 `MakeIndex/Index/Dura/DuraMax/Desc`（`ColorR/G/B`/`Prefix` 在本子集外）；
/// `Desc` 在本宿主是定长 14 字节（`UserItem()` 构造 `new byte[14]`，`ClientUserItem.cs:33`），
/// 记录里 `Desc` 缺失（`null`）时按未赋值形态补零。
#[must_use]
pub fn bag_item_from_server(it: &ServerUserItem) -> BagItem {
    let mut desc = [0u8; 14];
    if let Some(d) = it.desc.as_deref() {
        let n = d.len().min(desc.len());
        desc[..n].copy_from_slice(&d[..n]);
    }
    BagItem {
        make_index: it.make_index,
        index: it.index,
        dura: it.dura,
        dura_max: it.dura_max,
        desc,
    }
}

/// `WorldServer.cs:1213-1227`：`humanRcd.Data.StorageItems` → `playObject.StorageItemList`。
///
/// - `storage_items == None`（`StorageItems == null`）⇒ 空列表；
/// - 逐槽：`null` 槽 `continue`；`Index > 0` 才 `Add(ToClientItem())`（`Index == 0` 的空槽被丢弃）；
/// - **保持数组次序**（先出现的先入列表）。
#[must_use]
pub fn storage_items_from_record(storage_items: Option<&[Option<ServerUserItem>]>) -> Vec<BagItem> {
    let mut out = Vec::new();
    for cell in storage_items.unwrap_or(&[]) {
        let Some(it) = cell.as_ref() else { continue };
        if it.index > 0 {
            out.push(bag_item_from_server(it));
        }
    }
    out
}

/// `PlayObject` 进世界时的仓库会话态：读档列表 + 初生锁开（`PlayObject.cs:878/956`）。
#[must_use]
pub fn load_from_record(storage_items: Option<&[Option<ServerUserItem>]>) -> StorageState {
    StorageState {
        items: storage_items_from_record(storage_items),
        is_can_get_back_item: true,
    }
}

/// `WorldServer.cs:1359-1376`（`MakeSaveRcd` 的 `StorageItems`）：会话仓库 → 存档数组。
///
/// 存档数组是**新建**的 `ServerUserItem[50]`（见模块头），每次存档从全空槽起：
/// `items[i]` 写槽 i（超出 50 ⇒ `break`），余下空槽填 `HUtil32.DelfautItem.ToServerItem()`
/// （= [`default_server_item`]，**非 `None`**；DBSvr `ComparerUserItem` 要求槽位非 null）。
#[must_use]
pub fn storage_items_for_save(items: &[BagItem]) -> Vec<Option<ServerUserItem>> {
    let mut out = vec![Some(default_server_item()); STORAGE_SAVE_SLOTS];
    for (i, it) in items.iter().take(STORAGE_SAVE_SLOTS).enumerate() {
        out[i] = Some(server_item_from_bag(it));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(make_index: i32, index: u16) -> ServerUserItem {
        ServerUserItem {
            make_index,
            index,
            dura: 1,
            dura_max: 2,
            desc: Some(vec![0u8; 14]),
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        }
    }

    /// 读档：跳过 `null` 槽、丢弃 `Index == 0` 的空槽、保持数组次序（`WorldServer.cs:1213-1227`）。
    #[test]
    fn load_skips_null_and_zero_index_keeps_order() {
        let record = vec![
            Some(item(101, 3)),
            None,
            Some(item(0, 0)), // 记录里的空槽（DelfautItem）：Index==0 ⇒ 不入列表
            Some(item(102, 5)),
        ];
        let list = storage_items_from_record(Some(&record));
        assert_eq!(
            list.iter().map(|i| i.make_index).collect::<Vec<_>>(),
            vec![101, 102],
            "只收 Index>0 且非 null 的槽，次序不变"
        );
        assert!(
            load_from_record(Some(&record)).is_can_get_back_item,
            "读档建立的会话锁开（PlayObject.cs:956）"
        );
    }

    /// `StorageItems == null` ⇒ 空列表（`WorldServer.cs:1215` 的 `if (storageItems != null)` 反支）。
    #[test]
    fn load_null_array_is_empty() {
        assert!(storage_items_from_record(None).is_empty());
        assert!(load_from_record(None).items.is_empty());
    }

    /// `ToClientItem` 只搬 `MakeIndex/Index/Dura/DuraMax/Desc`（`ServerUserItem.cs:35-50`）。
    #[test]
    fn loaded_item_carries_identity_and_durability() {
        let mut src = item(777, 42);
        src.dura = 11;
        src.dura_max = 13;
        src.desc = Some((1u8..=14u8).collect());
        let list = storage_items_from_record(Some(&[Some(src)]));
        assert_eq!(list.len(), 1);
        let got = &list[0];
        assert_eq!(
            (got.make_index, got.index, got.dura, got.dura_max),
            (777, 42, 11, 13)
        );
        assert_eq!(got.desc, [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
    }

    /// 存档：恒 50 槽、前 N 槽是会话物品、其余槽是 `DelfautItem`（非 None）——`WorldServer.cs:1359-1376`。
    #[test]
    fn save_is_fixed_fifty_slots_with_default_filler() {
        let items = vec![BagItem::new(9, 7), BagItem::new(10, 8)];
        let out = storage_items_for_save(&items);
        assert_eq!(out.len(), STORAGE_SAVE_SLOTS, "存档数组恒 50 槽");
        assert_eq!(out[0].as_ref().unwrap().make_index, 9);
        assert_eq!(out[1].as_ref().unwrap().make_index, 10);
        for (i, slot) in out.iter().enumerate().skip(2) {
            let d = slot.as_ref().expect("空槽填 DelfautItem，不许是 None");
            assert_eq!(
                (d.make_index, d.index, d.dura, d.dura_max),
                (0, 0, 0, 0),
                "槽 {i} 应为默认物品"
            );
        }
    }

    /// 存档长度不随会话物品数变化（避免「取回后旧物品留在尾槽」这类隐性分叉）：
    /// 50 件 vs 1 件的存档数组长度相同、尾槽都是默认物品。
    #[test]
    fn save_length_is_independent_of_item_count() {
        let fifty: Vec<BagItem> = (0..50).map(|i| BagItem::new(i, 1)).collect();
        let one = vec![BagItem::new(1, 1)];
        let a = storage_items_for_save(&fifty);
        let b = storage_items_for_save(&one);
        assert_eq!(a.len(), b.len());
        assert_eq!(a[49].as_ref().unwrap().make_index, 49);
        assert_eq!(b[1].as_ref().unwrap().make_index, 0);
        assert_eq!(b[49].as_ref().unwrap().make_index, 0);
    }

    /// 39 上限：`can_store` 在 0..=38 为 true、39 起为 false（`PlayObject.Operate.cs:1629`）。
    #[test]
    fn can_store_boundary_is_thirty_nine() {
        let mut st = StorageState::new();
        assert!(st.can_store(), "空仓库可存");
        st.items = (0..STORAGE_MAX_ITEMS as i32)
            .map(|i| BagItem::new(i, 1))
            .collect();
        assert_eq!(st.items.len(), 39);
        assert!(!st.can_store(), "满 39 不能再存");
        st.items.pop();
        assert!(st.can_store(), "38 件仍可存");
    }

    /// 会话初生：空列表 + 取回锁开（`PlayObject.cs:878/956`）。
    #[test]
    fn new_state_is_empty_and_unlocked() {
        let st = StorageState::new();
        assert!(st.items.is_empty());
        assert!(st.is_can_get_back_item, "初生 IsCanGetBackItem = true");
    }

    /// 读档 → 存档往返（同一条记录）逐字段不变（`ToClientItem`/`ToServerItem` 互逆的子集）。
    #[test]
    fn round_trip_load_then_save_preserves_items() {
        let record = vec![Some(item(1, 3)), Some(item(2, 4))];
        let list = storage_items_from_record(Some(&record));
        let out = storage_items_for_save(&list);
        for (i, src) in record.iter().enumerate() {
            let src = src.as_ref().unwrap();
            let got = out[i].as_ref().unwrap();
            assert_eq!(
                (got.make_index, got.index, got.dura, got.dura_max),
                (src.make_index, src.index, src.dura, src.dura_max),
                "槽 {i} 往返不变"
            );
            assert_eq!(got.desc.as_deref(), src.desc.as_deref());
        }
    }
}
