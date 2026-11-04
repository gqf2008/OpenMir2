//! **T-277**（卡 `openmir2-t282-mount-server`）坐骑服务端偏差：马牌识别、骑乘状态、`SM_MOUNTRIDE`。
//!
//! # 为什么是一条偏差（而不是 oracle 等价）
//!
//! C# oracle 里 `HorseType` 全仓只有声明与读取、**没有赋值点**，也没有任何马类物品
//! （`stditems.StdMode in (50,51,53)` 计数 0）；`@OnHorse` 的红字「骑马必须先戴上马牌!!!」
//! 是唯一可达支。本卡按客户端侧（MirClientZig T286）约定补：
//!
//! - 新增独立帧 `SM_MOUNTRIDE(1118)`（**不复用** `feature_ex` 低字节——冻结 Delphi 客户端把
//!   该低字节当**武器特效索引**，见 `Grobal2.pas` 的 `Horsefeature = Lobyte(LoWord(feature_ex))`）；
//! - 马牌物品：`StdMode = 18`（冻结客户端 `ClFunc.pas` 的 `18: Result := U_HORSE`）、
//!   `Shape = mount_id`；**不扩** `characters_item.Position=15`（C# oracle 读侧 `HumItems[15]`
//!   会越界抛异常），马牌留在背包，`@OnHorse` 从背包取；
//! - 骑乘态**不进入** `feature_ex`：帧面 `615.Param`/`41.Series` 与骑乘前逐字节相同（A4）。
//!
//! # mount_id 取值口径（与卡面文字的一处收敛，见 whitelist T-277）
//!
//! 卡面写「`Shape = mount_id (0..11)`」又写「帧 `mount_id = 0` 表示下马」——二者在 0 上冲突。
//! 实现取 **`mount_id ∈ 1..=12`、`0 = 下马`**；12 个 Mount 库按 `mount_id - 1` 映射到
//! `Crystal.Assets/ClientData/Mount/00.Lib .. 11.Lib`。客户端侧已回执接受此口径
//! （卡线程 comment `3652251b`；T286 merge `165079c`）。跨端向量由本文件的
//! [`MOUNT_LIBRARIES`] / [`mount_library_of`] 与对应用例钉死。
//!
//! # 帧格式
//!
//! `GM_DATA` body = 12B 命令头 ++ 3B 尾：
//! - 头：`Recog = actor_id`、`Ident = SM_MOUNTRIDE(1118)`、`Param = Tag = Series = 0`；
//! - 尾：`[1] mount_id`（0 = 下马）++ `[2] actor_id`（u16 小端，Word 截断）。
//!
//! 可见性 = 同地图 ±`SendRefMsgRange` 内的玩家（与 `SendRefMsg` 口径一致），不发给无关玩家。

use crate::ability::ItemCatalog;
use crate::bag::BagItem;

/// 新增坐骑帧号。取 1118：`Messages.cs` 的 SM/DBR 常量表无此值；冻结 Delphi 客户端无该
/// ident 分支（落 `Actor.SendMsg` 的默认空支，不报错、不产生特效）；Zig 客户端侧由 T286 解析。
pub const SM_MOUNTRIDE: u16 = 1118;

/// 马牌物品的 `stditems.StdMode`（冻结客户端 `ClFunc.pas` 的 `18: Result := U_HORSE`）。
pub const HORSE_ITEM_STD_MODE: u8 = 18;

/// `mount_id` 下界（1 = 第 1 个 Mount 库 `00.Lib`）；0 保留给「下马」。
pub const MOUNT_ID_MIN: u8 = 1;
/// `mount_id` 上界（12 = `11.Lib`）。
pub const MOUNT_ID_MAX: u8 = 12;

/// **T-277 / T286 跨端契约**：`mount_id 1..=12` 对应的 Mount 库文件，库索引 = `mount_id - 1`。
/// 服务端不发送文件名；本表只作为双方对齐的单一事实源，供契约用例与排障使用。
pub const MOUNT_LIBRARIES: [&str; 12] = [
    "00.Lib", "01.Lib", "02.Lib", "03.Lib", "04.Lib", "05.Lib", "06.Lib", "07.Lib", "08.Lib",
    "09.Lib", "10.Lib", "11.Lib",
];

/// `mount_id` → 客户端 Mount 库文件名；`0 = 下马`、越界 ⇒ `None`。
#[must_use]
pub fn mount_library_of(mount_id: u8) -> Option<&'static str> {
    let index = usize::from(mount_id.checked_sub(1)?);
    MOUNT_LIBRARIES.get(index).copied()
}

/// 物品模板 `Index` → `mount_id`。判定 = `StdMode == 18` **且** `Shape ∈ 1..=12`。
///
/// 模板缺失 / 非马牌 / `Shape` 越界 ⇒ `None`（呼叫方按「没有马牌」处理）。
#[must_use]
pub fn mount_id_of_index(catalog: &dyn ItemCatalog, index: u16) -> Option<u8> {
    let std = catalog.get(index)?;
    if std.std_mode != HORSE_ITEM_STD_MODE {
        return None;
    }
    if (MOUNT_ID_MIN..=MOUNT_ID_MAX).contains(&std.shape) {
        Some(std.shape)
    } else {
        None
    }
}

/// 背包里第一块马牌的 `mount_id`（按 `item_list` 顺序，确定性；不依赖 map 迭代序）。
///
/// 「持有马牌 = 骑乘」是 T-277 的持久化载体：马牌行本身由既有 `characters_bagitem` 存档路径落库，
/// 登录/重连时用本函数**重算** `HorseType`（与 T-279 的 `dress_eff_from_hum_items` 同法，不新增列）。
#[must_use]
pub fn held_mount_id<'a>(
    catalog: &dyn ItemCatalog,
    items: impl IntoIterator<Item = &'a BagItem>,
) -> Option<u8> {
    items
        .into_iter()
        .find_map(|it| mount_id_of_index(catalog, it.index))
}

/// `SM_MOUNTRIDE` 的 `GM_DATA` body（12B 头 ++ 3B 尾，见模块头）。
#[must_use]
pub fn sm_mountride_body(actor_id: i32, mount_id: u8) -> Vec<u8> {
    let header = crate::smframe::command_header(actor_id, SM_MOUNTRIDE, 0, 0, 0);
    let mut body = crate::smframe::gm_data_body(header, &[]);
    body.push(mount_id);
    body.extend_from_slice(&((actor_id & 0xFFFF) as u16).to_le_bytes());
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::StdItemCatalog;
    use mir2_data::models::StdItem;

    fn catalog_with(index: u16, std_mode: u8, shape: u8) -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        c.insert(
            index,
            StdItem {
                name: "马牌".into(),
                std_mode,
                shape,
                ..StdItem::default()
            },
        );
        c
    }

    /// `mount_id_of_index` 的正/负例（StdMode/Shape 两个谓词都要有牙）。
    #[test]
    fn mount_id_requires_stdmode_18_and_shape_in_range() {
        let ok = catalog_with(1001, HORSE_ITEM_STD_MODE, 1);
        assert_eq!(mount_id_of_index(&ok, 1001), Some(1));
        let top = catalog_with(1012, HORSE_ITEM_STD_MODE, 12);
        assert_eq!(mount_id_of_index(&top, 1012), Some(12));
        // 负例：非马牌 StdMode。
        let wrong_mode = catalog_with(1001, 17, 1);
        assert_eq!(mount_id_of_index(&wrong_mode, 1001), None);
        // 负例：Shape=0（下马保留值）与 Shape=13（越界）。
        assert_eq!(
            mount_id_of_index(&catalog_with(1001, HORSE_ITEM_STD_MODE, 0), 1001),
            None
        );
        assert_eq!(
            mount_id_of_index(&catalog_with(1001, HORSE_ITEM_STD_MODE, 13), 1001),
            None
        );
        // 负例：模板缺失。
        assert_eq!(mount_id_of_index(&ok, 9999), None);
    }

    /// `held_mount_id` 取**第一块**马牌（顺序确定性），跳过非马牌。
    #[test]
    fn held_mount_id_takes_first_plate_in_bag_order() {
        let cat = catalog_with(1001, HORSE_ITEM_STD_MODE, 3);
        let bag = vec![
            BagItem::new(1, 42),   // 非马牌
            BagItem::new(2, 1001), // 马牌 #3
            BagItem::new(3, 1001), // 马牌 #3（第二块不改变结果）
        ];
        assert_eq!(held_mount_id(&cat, &bag), Some(3));
        assert_eq!(held_mount_id(&cat, &[]), None);
        assert_eq!(held_mount_id(&cat, &[BagItem::new(1, 42)]), None);
    }

    /// **F3 / T286 跨端契约向量**：`Shape 1 → mount_id 1 → 00.Lib`、
    /// `Shape 12 → mount_id 12 → 11.Lib`；帧 `mount_id = 0` 只表示下马、不对应任何库。
    ///
    /// 红检：把库索引从 `id - 1` 改成 `id` ⇒ 两端向量断言红；把 0 映射到 `00.Lib` ⇒
    /// `mount_library_of(0) == None` 与下马帧断言红。
    #[test]
    fn mount_contract_vectors_match_t286() {
        let low = catalog_with(1001, HORSE_ITEM_STD_MODE, 1);
        let low_id = mount_id_of_index(&low, 1001).expect("Shape 1 是合法马牌");
        assert_eq!(low_id, 1);
        assert_eq!(mount_library_of(low_id), Some("00.Lib"));

        let high = catalog_with(1012, HORSE_ITEM_STD_MODE, 12);
        let high_id = mount_id_of_index(&high, 1012).expect("Shape 12 是合法马牌");
        assert_eq!(high_id, 12);
        assert_eq!(mount_library_of(high_id), Some("11.Lib"));

        assert_eq!(mount_library_of(0), None, "0 只表示下马，不对应 00.Lib");
        assert_eq!(mount_library_of(13), None, "越界没有库");
        let down = sm_mountride_body(1, 0);
        assert_eq!(down[12], 0, "帧 0 = 下马");
    }

    /// 帧体逐字节：12B 头（Recog/Ident/0/0/0）++ `[mount_id][actor u16 LE]`。
    #[test]
    fn sm_mountride_body_layout() {
        let body = sm_mountride_body(0x0001_0002, 7);
        assert_eq!(body.len(), 15);
        assert_eq!(
            i32::from_le_bytes(body[0..4].try_into().unwrap()),
            0x0001_0002
        );
        assert_eq!(u16::from_le_bytes([body[4], body[5]]), SM_MOUNTRIDE);
        assert_eq!(&body[6..12], &[0, 0, 0, 0, 0, 0]);
        assert_eq!(body[12], 7, "尾 [1] mount_id");
        assert_eq!(
            u16::from_le_bytes([body[13], body[14]]),
            2,
            "尾 [2] actor_id 低 16 位 LE"
        );
        // 下马帧 mount_id=0。
        let down = sm_mountride_body(5, 0);
        assert_eq!(down[12], 0);
    }
}
