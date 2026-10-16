//! 入站派发 P0 批（R1 前六条）的**纯函数接缝**（`CM_QUERYUSERNAME` / `CM_QUERYUSERSTATE`）。
//!
//! 本模块只承载与「世界/会话状态」解耦的两件可被单测直接钉住的事：
//! 1. `CM_QUERYUSERNAME` 的 `CretInNearXy` 几何判定（oracle `PlayObject.Base.cs:984-1010`）；
//! 2. `CM_QUERYUSERSTATE` 的 `UserStateInfo` 线缆缓冲编码（oracle
//!    `src/OpenMir2/Packets/ClientPackets/UserStateInfo.cs:29-49`）。
//!
//! 派发分支、早退门与出帧在 `main.rs`（`handle_query_user_name_cmd` 等）；本模块只做纯逻辑，
//! 以便 `cargo test -p game-svr --lib` 直接钉住字段序/几何，不依赖 tokio/网关。

use mir2_data::StdItem;
use mir2_world::World;

use crate::itemwire::{encode_client_item, write_ascii_string, ClientItem, ITEM_NAME_LEN};

/// oracle `PlayObject.CretInNearXy(targeBaseObject, nX, nY)`（`PlayObject.Base.cs:984-1010`）。
///
/// 语义：在**本方所在图**上、以 `(x,y)` 为中心的 3×3 格内逐格扫 `ObjList`；对每个
/// `CellObjId > 0 && ActorObject` 的格子对象，等价于取 `ActorMgr.Get(CellObjId)`——
/// 若该对象 `!= null && !Ghost` 且**就是目标**即命中。
///
/// - `map_id` = 本方（调用者）当前所在图（oracle `this.Envir`）；
/// - `target_id` = 待判目标的 `ActorId`；目标不在世界 ⇒ 永不命中
///   （oracle 侧 `ClientQueryUserName` 已在前一行 `baseObject == null` 早退）。
///
/// 逐格扫描（而非只比坐标）是**照抄 oracle 结构**：格子不可用（越界/不可走）时该格被跳过，
/// 等价于「目标虽在坐标窗口内但不在这张图的格子里 ⇒ 不命中」。
#[must_use]
pub fn cetin_near_xy(world: &World, map_id: usize, x: i32, y: i32, target_id: i32) -> bool {
    let Some(map) = world.maps.get(map_id) else {
        return false;
    };
    for cx in (x - 1)..=(x + 1) {
        for cy in (y - 1)..=(y + 1) {
            let (cell, ok) = map.get_cell_info(cx, cy);
            if !ok || !cell.is_available() {
                continue;
            }
            for o in &cell.obj_list {
                if o.cell_obj_id > 0
                    && o.actor_object
                    && o.cell_obj_id == target_id
                    && world.entities.get(o.cell_obj_id).is_some_and(|t| !t.ghost)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// 一个 `ClientItem` 的**空槽**缓冲（oracle `UserStateInfo.nullItemBuff`，
/// `UserStateInfo.cs:17`：`new ClientItem().GetBuffer()`）——默认模板（全 0）+ 空名 + 空 `Reference`
/// + 空 `Desc`。
#[must_use]
pub fn null_item_buffer() -> Vec<u8> {
    let tpl = StdItem::default();
    encode_client_item(&ClientItem {
        tpl: &tpl,
        name: String::new(),
        make_index: 0,
        dura: 0,
        dura_max: 0,
        desc: [0; ITEM_NAME_LEN],
    })
}

/// `UserStateInfo.WritePacket`（`UserStateInfo.cs:29-49`）的 1:1 编码。
///
/// 字段序（LSB 起）：
/// ```text
/// Feature            (int32, 4B)
/// WriteAsciiString(UserName,      14)   // 1B 长度 + 14B
/// (byte)0
/// NameColor          (int32, 4B)
/// WriteAsciiString(GuildName,     20)   // 1B 长度 + 20B
/// WriteAsciiString(GuildRankName, 14)   // 1B 长度 + 14B
/// 13 × UseItems[i].GetBuffer()          // 空槽 = 空缓冲
/// ExistLover         (bool → 1B)
/// WriteAsciiString(LoverName,     14)
/// ```
///
/// `use_item_segments` 必须是 **13** 段（调用方保证：有件 = `encode_client_item`，空槽 = 空缓冲）。
/// **登记**：`ChangeItemWithLevel`（`Dress` 槽按等级改名）/`ChangeItemByJob`（按职业改属性）两个
/// 变换未建模（`UserStateInfo.cs` 之外，`PlayObject.Operate.cs:95-102`）；Dress/Weapon 槽带件时
/// 与 oracle 存在差异，空背包（13 空槽）路径逐字节一致。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_user_state_buffer(
    feature: i32,
    user_name: &str,
    name_color: i32,
    guild_name: &str,
    guild_rank_name: &str,
    use_item_segments: &[Vec<u8>],
    exist_lover: bool,
    lover_name: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&feature.to_le_bytes());
    write_ascii_string(&mut out, user_name, 14);
    out.push(0);
    out.extend_from_slice(&name_color.to_le_bytes());
    write_ascii_string(&mut out, guild_name, 20);
    write_ascii_string(&mut out, guild_rank_name, 14);
    for seg in use_item_segments {
        out.extend_from_slice(seg);
    }
    out.push(u8::from(exist_lover));
    write_ascii_string(&mut out, lover_name, 14);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_world::Entity;

    /// `null_item_buffer` 的长度锚（`ClientItem.WritePacket` 的定长形态）：改坏任一字段宽度/序必红。
    #[test]
    fn null_item_buffer_is_fixed_124_bytes() {
        assert_eq!(
            null_item_buffer().len(),
            124,
            "null ClientItem 缓冲长度（oracle `new ClientItem().GetBuffer()`）"
        );
    }

    /// `UserStateInfo` 的字段序与各定长串宽度——改坏字段序/宽度必红。
    #[test]
    fn user_state_buffer_layout_matches_oracle_field_order() {
        let item_len = null_item_buffer().len();
        let segs = vec![null_item_buffer(); 13];
        let feature = 0x0403_0201i32;
        let buf = build_user_state_buffer(feature, "AB", 255, "G", "R", &segs, false, "");

        // Feature (i32 @ 0)
        assert_eq!(&buf[0..4], &feature.to_le_bytes(), "Feature 起点/宽度");
        // UserName: 1B 长度(=2) @4 + 14B 定长 @5..19
        assert_eq!(buf[4], 2, "UserName 长度字节");
        assert_eq!(&buf[5..7], b"AB", "UserName 内容");
        assert_eq!(&buf[7..19], &[0u8; 12], "UserName 补零到 14");
        // (byte)0 @19
        assert_eq!(buf[19], 0, "UserName 与 NameColor 之间的保留 0");
        // NameColor (i32 @ 20)
        assert_eq!(&buf[20..24], &255i32.to_le_bytes(), "NameColor @20");
        // GuildName: 1B 长度(=1) @24 + 20B @25..45
        assert_eq!(buf[24], 1, "GuildName 长度字节");
        assert_eq!(buf[25], b'G', "GuildName 内容");
        // GuildRankName: 1B 长度(=1) @45 + 14B @46..60
        assert_eq!(buf[45], 1, "GuildRankName 长度字节");
        assert_eq!(buf[46], b'R', "GuildRankName 内容");
        // 13 段物品缓冲 @60
        let items_start = 60;
        assert_eq!(
            buf.len(),
            items_start + 13 * item_len + 1 + 15,
            "总长 = 60 + 13×item + ExistLover(1) + LoverName(1+14)"
        );
        // ExistLover (bool → 1B) 在物品段之后
        assert_eq!(buf[items_start + 13 * item_len], 0, "ExistLover=0");
    }

    /// 非空尾串（LoverName/GuildName）的长度字节按 oracle `WriteAsciiString` 口径。
    #[test]
    fn user_state_buffer_ascii_len_bytes_for_nonempty_tails() {
        let buf = build_user_state_buffer(
            0,
            "Hero",
            1,
            "GuildX",
            "Rank",
            &vec![null_item_buffer(); 13],
            true,
            "Dear",
        );
        assert_eq!(buf[4], 4, "UserName='Hero' → 长度 4");
        assert_eq!(buf[24], 6, "GuildName='GuildX' → 长度 6");
        assert_eq!(buf[45], 4, "GuildRankName='Rank' → 长度 4");
        // ExistLover=1
        let item_len = null_item_buffer().len();
        assert_eq!(buf[60 + 13 * item_len], 1, "ExistLover=true → 1");
        // LoverName 紧随其后
        assert_eq!(buf[60 + 13 * item_len + 1], 4, "LoverName='Dear' → 长度 4");
    }

    /// `CretInNearXy`：以 (x,y) 为心的 3×3 命中、出窗不命中、异图不命中、Ghost 目标不命中。
    ///
    /// 红检：把窗口从 3×3 改成任意其它范围、或删掉 `!ghost` / `map_id` 判定 ⇒ 必红。
    #[test]
    fn cetin_near_xy_window_and_ghost_and_map() {
        let mut world = World::with_maps(vec![(40, 40), (40, 40)]);
        // 目标在 (11,10)：落在以 (10,10) 为心的 3×3 内。
        world.open_session("t");
        world
            .enter_map("t", Entity::new(0, "t", 11, 10), 0)
            .expect("进图");
        let target = world.session("t").unwrap().actor_id.unwrap();
        // 目标已在 (11,10) ⇒ 心 (10,10) 命中。
        assert!(cetin_near_xy(&world, 0, 10, 10, target), "3×3 窗口内应命中");
        assert!(cetin_near_xy(&world, 0, 11, 10, target), "目标自身格命中");
        // 心 (9,10)：窗口 = x∈[8,10]，不含 11 ⇒ 不命中。
        assert!(
            !cetin_near_xy(&world, 0, 9, 10, target),
            "出 3×3 窗口不命中"
        );
        // 异图（map_id=1）⇒ 不命中。
        assert!(!cetin_near_xy(&world, 1, 10, 10, target), "异图不命中");
        // 不存在的目标 ⇒ 不命中。
        assert!(!cetin_near_xy(&world, 0, 10, 10, 9999), "目标不存在不命中");
        // 目标变 Ghost ⇒ 不命中。
        world.entities.get_mut(target).unwrap().ghost = true;
        assert!(
            !cetin_near_xy(&world, 0, 10, 10, target),
            "Ghost 目标不命中"
        );
    }
}
