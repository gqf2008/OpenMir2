//! `CM_PICKUP`(1001) / `CM_DROPITEM`(1000) 的**接线层**（owner §23.1/§25.1/§26）：
//! 把 `ground::*` 纯函数产出的 `Frame` / `QueuedNotice` 翻成经网关下发的 `SM_*` body
//! （`GM_DATA.body = 12B 命令头 ++ 尾`），并把 world 的只读地面视图装配成 `ground::GroundMap`。
//!
//! 口径逐条照 oracle（作者 comment `d552d2a2`；行号）：
//!
//! ```text
//! CM_DROPITEM（PlayObject.Message.cs:801-810）
//!   if (ClientDropItem(Msg, nParam1))  SendDefMessage(SM_DROPITEM_SUCCESS, nParam1,0,0,0, Msg)
//!   else                               SendDefMessage(SM_DROPITEM_FAIL,    nParam1,0,0,0, Msg)
//!   ⇒ Recog = make_index{nParam1}；尾 = EncodeString(请求的 Msg)（`ground::DropOutcome` 只给 12B 头，
//!     尾由本层补）。
//! CM_PICKUP（:811-816）
//!   先判 CurrX==nParam2 && CurrY==nParam3 才 ClientPickUpItem()（**位置门在派发层**，见 main.rs）。
//! 延时通知 → SM（全部收件人=自己）
//!   RM_ITEMHIDE(8085)      → SM_ITEMHIDE(611)      recog=nParam1 param=nParam2 tag=nParam3 series=0
//!   RM_WEIGHTCHANGED(10115)→ SM_WEIGHTCHANGED(622) 用**当下 WAbil**（Weight/WearWeight/HandWeight + 校验和）
//!   RM_GOLDCHANGED(10136)  → SM_GOLDCHANGED(653)   Gold + LoWord/HiWord(GameGold)
//!   RM_SYSMESSAGE(11004)   → SM_SYSMESSAGE(100)    recog=ActorId, param=MakeWord(nParam1,nParam2), series=1, 尾=EncodeString(msg)
//!   RM_MENU_OK(10309)      → SM_MENU_OK(767)        recog=nParam1, 尾=processMsg.Msg（本接缝为空）
//! ```
//!
//! # 已知缺口（登记，不静默）
//!
//! 1. `RM_SYSMESSAGE` 的文本（oracle `SysMsg(MessageSettings.CanotPickUpItem, Red, Hint)`）由
//!    `MessageSettings` 配置提供，`ground::pick_up` 的 `QueuedNotice` **不带文本** ⇒
//!    本层用 [`WireSession::sys_msg`] 显式注入；缺该输入时**跳过**（不编造空文本）。
//! 2. 装配地面物品时 `BagItem.dura/dura_max/desc` 不在 world 只读视图里
//!    （`FloorItemMeta` 只记 `kind/index/count/item_id/of_base_object`）⇒ 取 `BagItem` 默认值。
//!    要逐字节复原需扩 world 记账（下一片，需 owner 放行写分片）。
//! 3. `CM_DROPITEM` 的成功写回（`Envir.AddItemToMap`）在本接缝下恒失败（`MapItem.ItemId == 0`，
//!    见 `ground.rs` 模块头 `T-19`）⇒ 不加地面；一旦 oracle 补上 `ItemId`，写回属下一片。

use crate::ability::{Ability, ItemCatalog};
use crate::bag::{BagItem, Frame};
use crate::ground::{
    GroundItem, GroundMap, RM_GOLDCHANGED, RM_ITEMHIDE, RM_MENU_OK, RM_SYSMESSAGE,
    RM_WEIGHTCHANGED, SM_ADDITEM, SM_DROPITEM_FAIL, SM_DROPITEM_SUCCESS,
};
use crate::smframe;
use crate::useitem::{my_status, QueuedNotice, RM_HEALTHSPELLCHANGED, RM_MYSTATUS};
use mir2_world::{FloorItemKind, FloorItemView};

/// 接线层在一次 pickup/drop 处理里需要读到的**会话态**（oracle `PlayObject` 上的当下值）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireSession {
    /// `BaseObject.ActorId`（`SM_SYSMESSAGE` 的 `Recog`）。
    pub actor_id: i32,
    /// `BaseObject.Gold`（`SM_GOLDCHANGED` 用**当下值**）。
    pub gold: i32,
    /// `BaseObject.GameGold`（`SM_GOLDCHANGED` 的 LoWord/HiWord）。
    pub game_gold: i32,
    /// `BaseObject.WAbil`（`SM_WEIGHTCHANGED` 用**当下值**）。
    pub wabil: Ability,
    /// `MessageSettings.CanotPickUpItem` 的 GBK 原文；缺 ⇒ `RM_SYSMESSAGE` 不产出（缺口 1）。
    pub sys_msg: Option<Vec<u8>>,
    /// `PlayObject.HungerStatus`（`PlayObject.cs:225`）——`RM_MYSTATUS` 的 `GetMyStatus()`
    /// （`PlayObject.Base.cs:1183`）读它（**制药/食药片 WB**：`CM_EAT` StdMode 1 的跨档出帧）。
    pub hunger_status: i32,
}

/// 把一条立即帧（[`Frame`]）翻成 `SM_*` body；未知 ident ⇒ `None`（调用方记 WRN）。
#[must_use]
pub fn immediate_frame(frame: &Frame, drop_msg: &[u8]) -> Option<Vec<u8>> {
    match frame.ident {
        SM_ADDITEM => Some(smframe::sm_additem(frame.recog)),
        SM_DROPITEM_SUCCESS => Some(smframe::sm_dropitem_reply(true, frame.recog, drop_msg)),
        SM_DROPITEM_FAIL => Some(smframe::sm_dropitem_reply(false, frame.recog, drop_msg)),
        _ => None,
    }
}

/// 把一条延时通知翻成 `SM_*` body；未知 ident ⇒ `None`。
///
/// `RM_SYSMESSAGE` 缺 [`WireSession::sys_msg`] 时也返回 `None`（缺口 1）。
#[must_use]
pub fn notice_frame(n: &QueuedNotice, session: &WireSession) -> Option<Vec<u8>> {
    match n.ident {
        RM_ITEMHIDE => Some(smframe::sm_itemhide(n.n_param1, n.n_param2, n.n_param3)),
        RM_WEIGHTCHANGED => Some(smframe::sm_weightchanged(
            session.wabil.weight,
            session.wabil.wear_weight,
            session.wabil.hand_weight,
        )),
        // `CM_EAT` 的 `StdMode 0` 回帧（`BaseObject.IncHealthSpell` 尾部 `HealthSpellChanged()`，
        // `BaseObject.cs:704` ⇒ `SendMsg(RM_HEALTHSPELLCHANGED)`，`PlayObject.Message.cs:1736-1739`）。
        // 布局：生产默认冻结客户端（`TCharDesc` 尾），`MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle header-only。
        RM_HEALTHSPELLCHANGED => Some(smframe::sm_healthspellchanged_frame(
            session.actor_id,
            session.wabil.hp,
            session.wabil.mp,
            session.wabil.max_hp,
        )),
        RM_GOLDCHANGED => Some(smframe::sm_goldchanged(session.gold, session.game_gold)),
        RM_SYSMESSAGE => session.sys_msg.as_deref().map(|msg| {
            smframe::sm_sysmessage(
                session.actor_id,
                u8::try_from(n.n_param1).unwrap_or(0),
                u8::try_from(n.n_param2).unwrap_or(0),
                msg,
            )
        }),
        RM_MENU_OK => Some(smframe::sm_menu_ok(n.n_param1, &[])),
        // `RM_MYSTATUS`（`PlayObject.Base.cs:291` 的 `RefMyStatus()`）⇒
        // `SendDefMessage(SM_MYSTATUS, 0, (short)GetMyStatus(), 0, 0)`（`PlayObject.Message.cs:1950`）。
        // `GetMyStatus()` = `min(HungerStatus/1000, 4)` 的**当下**值（`PlayObject.Base.cs:1183`）。
        RM_MYSTATUS => Some(crate::smframe_ext::sm_mystatus(my_status(
            session.hunger_status,
        ))),
        _ => None,
    }
}

/// 一次 pickup/drop 的完整下行序列 = **立即帧（按 `ground::*` 内序）** 再 **延时通知（按出队序）**。
#[must_use]
pub fn outcome_frames(
    immediate: &[Frame],
    notices: &[QueuedNotice],
    session: &WireSession,
    drop_msg: &[u8],
) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(immediate.len() + notices.len());
    for f in immediate {
        if let Some(b) = immediate_frame(f, drop_msg) {
            out.push(b);
        }
    }
    for n in notices {
        if let Some(b) = notice_frame(n, session) {
            out.push(b);
        }
    }
    out
}

/// 用 world 的只读视图（owner §25.1 (A)）装配 `ground::GroundMap`。
///
/// `width`/`height` = 该图的真尺寸（`World::maps[map_id]`）；视图坐标是绝对坐标。
/// 不在视图里的格子留空。字段缺口见模块头「已知缺口 2」。
#[must_use]
pub fn build_ground_map(
    view: &[FloorItemView],
    map_id: usize,
    width: i32,
    height: i32,
    catalog: &dyn ItemCatalog,
) -> GroundMap {
    let mut map = GroundMap::new(width, height);
    for v in view.iter().filter(|v| v.map_id == map_id) {
        let item = match v.kind {
            FloorItemKind::Gold => {
                GroundItem::gold(v.item_id, v.count, v.of_base_object, v.can_pick_up_tick)
            }
            FloorItemKind::Item => {
                let (name, looks, ani_count) = catalog.get(v.index).map_or_else(
                    || (String::new(), 0u16, 0u8),
                    |t| (t.name.clone(), t.looks, t.ani_count),
                );
                GroundItem {
                    item_id: v.item_id,
                    name,
                    looks,
                    ani_count,
                    reserved: 0,
                    count: v.count,
                    drop_base_object: 0,
                    of_base_object: v.of_base_object,
                    can_pick_up_tick: v.can_pick_up_tick,
                    // 缺口 2：dura/dura_max/desc 不在只读视图里 ⇒ 默认值。
                    user_item: Some(BagItem {
                        make_index: v.item_id,
                        index: v.index,
                        dura: 0,
                        dura_max: 0,
                        desc: [0; 14],
                    }),
                }
            }
        };
        let _ = map.put_item(v.x, v.y, item);
    }
    map
}

/// `CM_PICKUP` 的**位置门**（`PlayObject.Message.cs:811-816`）：`CurrX==nParam2 && CurrY==nParam3`。
///
/// 不满足时 oracle 直接不进 `ClientPickUpItem`。
#[must_use]
pub fn pickup_position_ok(curr_x: i32, curr_y: i32, param2: i32, param3: i32) -> bool {
    curr_x == param2 && curr_y == param3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::StdItemCatalog;
    use mir2_data::StdItem;
    use mir2_protocol::edcode;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/ground_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    fn catalog_from(doc: &Value) -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        for (k, v) in doc["templates"].as_object().unwrap() {
            c.insert(
                k.parse::<u16>().unwrap(),
                StdItem {
                    name: v["name"].as_str().unwrap().to_string(),
                    std_mode: v["std_mode"].as_u64().unwrap() as u8,
                    shape: v["shape"].as_u64().unwrap() as u8,
                    weight: v["weight"].as_u64().unwrap() as u8,
                    price: v["price"].as_i64().unwrap() as i32,
                    looks: v["looks"].as_u64().unwrap() as u16,
                    ani_count: v["ani_count"].as_u64().unwrap() as u8,
                    dura_max: v["dura_max"].as_u64().unwrap() as u16,
                    ..StdItem::default()
                },
            );
        }
        c
    }

    fn wire_ability() -> Ability {
        Ability {
            level: 1,
            reserved1: 0,
            ac: 0,
            mac: 0,
            dc: 0,
            mc: 0,
            sc: 0,
            hp: 0,
            mp: 0,
            max_hp: 0,
            max_mp: 0,
            reserved2: 0,
            exp_count: 0,
            exp_max_count: 0,
            exp: 0,
            max_exp: 0,
            weight: 100,
            max_weight: 0,
            wear_weight: 5,
            max_wear_weight: 0,
            hand_weight: 2,
            max_hand_weight: 0,
        }
    }

    fn session(actor_id: i32, gold: i32, game_gold: i32, sys_msg: Option<&[u8]>) -> WireSession {
        WireSession {
            actor_id,
            gold,
            game_gold,
            wabil: wire_ability(),
            sys_msg: sys_msg.map(<[u8]>::to_vec),
            hunger_status: 0,
        }
    }

    fn header(recog: i32, ident: u16, param: u16, tag: u16, series: u16) -> Vec<u8> {
        let mut v = vec![0u8; 12];
        v[0..4].copy_from_slice(&recog.to_le_bytes());
        v[4..6].copy_from_slice(&ident.to_le_bytes());
        v[6..8].copy_from_slice(&param.to_le_bytes());
        v[8..10].copy_from_slice(&tag.to_le_bytes());
        v[10..12].copy_from_slice(&series.to_le_bytes());
        v
    }

    /// 立即帧：`SM_DROPITEM_SUCCESS/FAIL`（尾 = `EncodeString(Msg)`）；`SM_ADDITEM` 当前为
    /// **header-only（已知缺失 T-24）**——oracle `SendAddItem`（`PlayObject.Base.cs:225-227`）
    /// 会追加 `EncodeBuffer(clientItem)` 尾，本函数不产该尾（物品展示层未移植）。本用例
    /// **把当前 header-only 钉住**，避免后续真机 pickup 把缺尾误当 parity。
    ///
    /// 红检：把 `sm_additem` 的 `series` 从 1 改成 0 ⇒ `add` 断言必红。
    #[test]
    fn immediate_frames_map_to_oracle_bytes() {
        let add = immediate_frame(
            &Frame {
                ident: SM_ADDITEM,
                recog: 0x1234,
                param: 0,
                tag: 0,
                series: 1,
            },
            b"",
        )
        .expect("SM_ADDITEM 必须可映射");
        assert_eq!(add, header(0x1234, 200, 0, 0, 1));

        let msg = b"ring";
        let mut expect_fail = header(70001, 601, 0, 0, 0);
        expect_fail.extend_from_slice(&edcode::encode(msg));
        let fail = immediate_frame(
            &Frame {
                ident: SM_DROPITEM_FAIL,
                recog: 70001,
                param: 0,
                tag: 0,
                series: 0,
            },
            msg,
        )
        .expect("SM_DROPITEM_FAIL 必须可映射");
        assert_eq!(fail, expect_fail);

        let mut expect_ok = header(70001, 600, 0, 0, 0);
        expect_ok.extend_from_slice(&edcode::encode(msg));
        let ok = immediate_frame(
            &Frame {
                ident: SM_DROPITEM_SUCCESS,
                recog: 70001,
                param: 0,
                tag: 0,
                series: 0,
            },
            msg,
        )
        .unwrap();
        assert_eq!(ok, expect_ok);
    }

    /// 五条 `RM_*` 的映射（`recog/param/tag/series` 落位逐字节）。
    ///
    /// 红检：把 `RM_GOLDCHANGED` 的 `series` 从 0 改成 1 ⇒ `gold` 断言必红。
    #[test]
    fn notices_map_to_oracle_frames() {
        let s = session(0x7C30_3998, 1000, 0x0001_0002, Some(b"cannot"));

        let itemhide = notice_frame(
            &QueuedNotice {
                ident: RM_ITEMHIDE,
                w_param: 0,
                n_param1: 70001,
                n_param2: 32,
                n_param3: 33,
            },
            &s,
        )
        .unwrap();
        assert_eq!(itemhide, header(70001, 611, 32, 33, 0));

        let weight = notice_frame(
            &QueuedNotice {
                ident: RM_WEIGHTCHANGED,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
            },
            &s,
        )
        .unwrap();
        let chk = ((107i32 ^ 0x3A5F) ^ 0x1F35) ^ 0xaa21;
        assert_eq!(weight, header(100, 622, 5, 2, chk as u16));

        let gold = notice_frame(
            &QueuedNotice {
                ident: RM_GOLDCHANGED,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
            },
            &s,
        )
        .unwrap();
        assert_eq!(gold, header(1000, 653, 0x0002, 0x0001, 0));

        let mut expect_sys = header(0x7C30_3998, 100, 0x00FF, 0, 1);
        expect_sys.extend_from_slice(&edcode::encode(b"cannot"));
        let sys = notice_frame(
            &QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 0xFF,
                n_param2: 0x00,
                n_param3: 0,
            },
            &s,
        )
        .unwrap();
        assert_eq!(sys, expect_sys);

        let menu = notice_frame(
            &QueuedNotice {
                ident: RM_MENU_OK,
                w_param: 0,
                n_param1: 4660,
                n_param2: 0,
                n_param3: 0,
            },
            &s,
        )
        .unwrap();
        assert_eq!(menu, header(4660, 767, 0, 0, 0));
    }

    /// **制药/食药片 WB**：`RM_MYSTATUS`（`PlayObject.Base.cs:291` 的 `RefMyStatus`）⇒
    /// `SM_MYSTATUS(708)`，`Param = (u16)(short)GetMyStatus()`、`Recog=0`，其中
    /// `GetMyStatus() = min(HungerStatus/1000, 4)`（当下值）。
    ///
    /// 变异必红：把 `Recog` 从 0 改成 `actor_id`、或不做 `/1000` 截断（直接用 HungerStatus）⇒ 必红。
    #[test]
    fn mystatus_maps_to_sm_mystatus_with_derived_status() {
        let mut s = session(0x7C30_3998, 0, 0, None);
        s.hunger_status = 4500; // ⇒ GetMyStatus() = 4
        let body = notice_frame(
            &QueuedNotice {
                ident: RM_MYSTATUS,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
            },
            &s,
        )
        .expect("RM_MYSTATUS 必须可映射");
        assert_eq!(body, header(0, 708, 4, 0, 0));
        assert_eq!(body, crate::smframe_ext::sm_mystatus(4));
        // 跨档下界（999 ⇒ 0）。
        s.hunger_status = 999;
        let low = notice_frame(
            &QueuedNotice {
                ident: RM_MYSTATUS,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
            },
            &s,
        )
        .unwrap();
        assert_eq!(low, header(0, 708, 0, 0, 0));
    }

    /// 缺 `sys_msg` ⇒ `RM_SYSMESSAGE` 不产出（缺口 1，不编造空文本）。
    #[test]
    fn sysmessage_without_text_is_skipped() {
        let s = session(1, 0, 0, None);
        assert!(notice_frame(
            &QueuedNotice {
                ident: RM_SYSMESSAGE,
                w_param: 0,
                n_param1: 0,
                n_param2: 0,
                n_param3: 0,
            },
            &s
        )
        .is_none());
    }

    /// 装配：只装目标图；金币 `item_id`/`count`/归属、物品模板名逐项对上 world 视图。
    ///
    /// 红检：把 `item_id: v.item_id` 改成 `v.item_id + 1` ⇒ 本用例必红。
    #[test]
    fn build_ground_map_matches_view() {
        let doc = fixture();
        let catalog = catalog_from(&doc);
        let view = vec![
            FloorItemView {
                map_id: 0,
                x: 32,
                y: 33,
                obj_id: 1234,
                kind: FloorItemKind::Item,
                index: 3001,
                count: 1,
                item_id: 70001,
                of_base_object: 0,
                can_pick_up_tick: 1_200_000,
            },
            FloorItemView {
                map_id: 0,
                x: 40,
                y: 41,
                obj_id: 7,
                kind: FloorItemKind::Gold,
                index: 0,
                count: 555,
                item_id: 7,
                of_base_object: 42,
                can_pick_up_tick: 1_200_000,
            },
            FloorItemView {
                map_id: 1,
                x: 1,
                y: 1,
                obj_id: 1,
                kind: FloorItemKind::Gold,
                index: 0,
                count: 1,
                item_id: 9,
                of_base_object: 0,
                can_pick_up_tick: 0,
            },
        ];
        let map = build_ground_map(&view, 0, 64, 64, &catalog);
        assert_eq!(map.item_count(), 2, "只装配 map_id=0 的两件");
        let it = map.get_item(32, 33).unwrap();
        assert_eq!(it.item_id, 70001);
        assert_eq!(it.count, 1);
        assert_eq!(it.name, "ring");
        assert_eq!(it.user_item.as_ref().unwrap().index, 3001);
        let g = map.get_item(40, 41).unwrap();
        assert_eq!(g.name, crate::ground::GOLD_NAME);
        assert_eq!(g.count, 555);
        assert_eq!(g.of_base_object, 42);
    }

    /// `CM_PICKUP` 的位置门（派发层口径）。
    #[test]
    fn pickup_position_gate() {
        assert!(pickup_position_ok(10, 20, 10, 20));
        assert!(!pickup_position_ok(10, 20, 11, 20));
    }
}
