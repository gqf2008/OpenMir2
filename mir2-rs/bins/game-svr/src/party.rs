//! 组队（party）接缝 —— M3⑦-实现 2（卡 `openmir2-m3-party-group-state`）。
//!
//! 世界侧的状态机在 [`mir2_world::party`]（`crates/world/src/party.rs`）；本模块只做两件事：
//! ① 把 `GateConfig` 的 `[Setup]` 值 + 缺省文案装成 [`mir2_world::PartyConfig`]；
//! ② 把一条 [`PartyEvent`] 编成 `(收件人 ActorId, GM_DATA body)`（**纯函数**，无 I/O）。
//! 按 `[GateIdx, Socket]` 路由由 `lib.rs` 的派发层完成（与既有 handler 同口径）。
//!
//! 帧映射与 oracle 锚点见 [`Event`] 的每个分支注释；编码在 [`crate::smframe`]。

use mir2_world::{PartyConfig, PartyEvent};

use crate::downlink::gbk;
use crate::smframe;

/// 从 `Server.conf` 的 `[Setup]` 值装世界侧 [`PartyConfig`]。
///
/// 入参由 `lib.rs` 从 `config::GateConfig` 取（`config` 是**二进制私有模块**，
/// 本 lib 模块不直接依赖它）。
///
/// **登记**：`[String] GroupMsgPreFix`/`JoinGroupMsg`（`StringConf.cs:39/223`）本片不读 `Server.conf`
/// 的 `[String]` 段（本仓尚无该段读取器）；部署 `Server.conf` 未覆盖它们 ⇒ 用源码缺省
/// （`〖组队〗` / `{0} 已加入小组.`），与 oracle 一致。
#[must_use]
pub fn world_config(
    group_members_max: u8,
    group_msg_f_color: u8,
    group_msg_b_color: u8,
    red_msg_f_color: u8,
    red_msg_b_color: u8,
) -> PartyConfig {
    PartyConfig {
        group_members_max,
        group_msg_f_color,
        group_msg_b_color,
        red_msg_f_color,
        red_msg_b_color,
        ..PartyConfig::default()
    }
}

/// 把一条组队事件编成 `(收件人 ActorId, GM_DATA body)`。
///
/// 逐分支对应 oracle：
/// - [`PartyEvent::GroupModeChanged`] ⇒ `SendDefMessage(SM_GROUPMODECHANGED, 0, param, 0, 0)`
///   （`PlayObject.Message.cs:926/929`）；
/// - [`PartyEvent::CreateGroupOk`]/[`PartyEvent::CreateGroupFail`] ⇒ `Operate.cs:840/816-830`；
/// - [`PartyEvent::AddMemberOk`]/[`PartyEvent::AddMemberFail`] ⇒ `Operate.cs:878/852-871`；
/// - [`PartyEvent::DelMemberOk`]（尾 `EncodeString(name)`）/ [`PartyEvent::DelMemberFail`]
///   ⇒ `Operate.cs:904/890-900`；
/// - [`PartyEvent::GroupMembers`]（尾 `EncodeString("a/b/c/")`）⇒ `PlayObject.Base.cs:1361-1366`；
/// - [`PartyEvent::GroupCancel`] ⇒ `PlayObject.cs:5150`（立即）/ `PlayObject.Message.cs:1848-1850`（排队）；
/// - [`PartyEvent::SysMessage`] ⇒ `SM_SYSMESSAGE(100)`（`PlayObject.Message.cs:1715-1724` 的
///   `RM_SYSMESSAGE`/`RM_GROUPMESSAGE` 出队映射）；
/// - [`PartyEvent::GotoLabel`] ⇒ **`None`**（不是帧；宿主 drain 时特判并执行 `QFunction` 脚本，
///   脚本说词另经 `RM_MERCHANTSAY` 下发，见 `mir2_world::party::PartyEvent::GotoLabel`）。
#[must_use]
pub fn event_body(ev: &PartyEvent) -> Option<(i32, Vec<u8>)> {
    match ev {
        PartyEvent::GotoLabel { .. } => None,
        PartyEvent::GroupModeChanged { target, param } => {
            Some((*target, smframe::sm_groupmodechanged(*param)))
        }
        PartyEvent::CreateGroupOk { target } => Some((*target, smframe::sm_creategroup_ok())),
        PartyEvent::CreateGroupFail { target, recog } => {
            Some((*target, smframe::sm_creategroup_fail(*recog)))
        }
        PartyEvent::AddMemberOk { target } => Some((*target, smframe::sm_groupaddmem_ok())),
        PartyEvent::AddMemberFail { target, recog } => {
            Some((*target, smframe::sm_groupaddmem_fail(*recog)))
        }
        PartyEvent::DelMemberOk { target, name } => {
            Some((*target, smframe::sm_groupdelmem_ok(&gbk(name))))
        }
        PartyEvent::DelMemberFail { target, recog } => {
            Some((*target, smframe::sm_groupdelmem_fail(*recog)))
        }
        PartyEvent::GroupMembers {
            target,
            members_text,
        } => Some((*target, smframe::sm_groupmembers(&gbk(members_text)))),
        PartyEvent::GroupCancel { target } => Some((*target, smframe::sm_groupcancel())),
        PartyEvent::SysMessage {
            target,
            recog,
            f_color,
            b_color,
            text,
        } => Some((
            *target,
            smframe::sm_sysmessage(*recog, u16::from(*f_color), u16::from(*b_color), &gbk(text)),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_protocol::{edcode, messages};

    fn decode_header(body: &[u8]) -> (i32, u16, u16) {
        // 尾是 EncodeString 后的字节，但头 12B 恒为明文（`gm_data_body` 直接拼明文头）。
        let recog = i32::from_le_bytes([body[0], body[1], body[2], body[3]]);
        let ident = u16::from_le_bytes([body[4], body[5]]);
        let param = u16::from_le_bytes([body[6], body[7]]);
        (recog, ident, param)
    }

    #[test]
    fn maps_each_event_to_oracle_frame_shape() {
        let (t, body) = event_body(&PartyEvent::GroupModeChanged {
            target: 7,
            param: 1,
        })
        .expect("帧事件必可编码");
        assert_eq!(t, 7);
        assert_eq!(decode_header(&body), (0, messages::SM_GROUPMODECHANGED, 1));
        assert_eq!(body.len(), 12); // header-only

        let (_, body) = event_body(&PartyEvent::CreateGroupOk { target: 7 }).expect("编码");
        assert_eq!(decode_header(&body), (0, messages::SM_CREATEGROUP_OK, 0));

        let (_, body) = event_body(&PartyEvent::CreateGroupFail {
            target: 7,
            recog: -4,
        })
        .expect("编码");
        assert_eq!(decode_header(&body), (-4, messages::SM_CREATEGROUP_FAIL, 0));

        let (_, body) = event_body(&PartyEvent::AddMemberFail {
            target: 7,
            recog: -5,
        })
        .expect("编码");
        assert_eq!(decode_header(&body), (-5, messages::SM_GROUPADDMEM_FAIL, 0));

        let (_, body) = event_body(&PartyEvent::GroupCancel { target: 7 }).expect("编码");
        assert_eq!(decode_header(&body), (0, messages::SM_GROUPCANCEL, 0));

        // DelMemberOk：尾 = EncodeString("T")
        let (_, body) = event_body(&PartyEvent::DelMemberOk {
            target: 7,
            name: "T".into(),
        })
        .expect("编码");
        assert_eq!(decode_header(&body), (0, messages::SM_GROUPDELMEM_OK, 0));
        assert_eq!(edcode::decode(&body[12..]), gbk("T"));

        // SysMessage：param = MakeWord(f,b)，series=1
        let (_, body) = event_body(&PartyEvent::SysMessage {
            target: 7,
            recog: 7,
            f_color: 0xC4,
            b_color: 0xFF,
            text: "hi".into(),
        })
        .expect("编码");
        let (recog, ident, param) = decode_header(&body);
        assert_eq!(recog, 7);
        assert_eq!(ident, messages::SM_SYSMESSAGE);
        assert_eq!(param, 0xFFC4);
        let series = u16::from_le_bytes([body[10], body[11]]);
        assert_eq!(series, 1);
        assert_eq!(edcode::decode(&body[12..]), gbk("hi"));

        // GotoLabel 不是帧 ⇒ `event_body` 返回 `None`（由宿主 drain 特判执行脚本）。
        assert_eq!(
            event_body(&PartyEvent::GotoLabel {
                target: 7,
                label: mir2_world::party::PARTY_LABEL_GROUP_CREATE,
            }),
            None
        );
    }
}
