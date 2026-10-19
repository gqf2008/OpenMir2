//! **G1 行会会话层：GameSvr → 客户端的行会 `SM_*` 帧编码层**（新模块，纯函数，不接线）。
//!
//! 范围 = 简报 `E:/tmp/lane3-guild-brief.md` §2 的帧清单里**oracle 真有发点**的 15 条：
//!
//! | ident | 函数 | oracle 发点 | 头 | 尾 |
//! |---|---|---|---|---|
//! | 750 | [`sm_guild_changename_750`] | `PlayObject.Base.cs:1625-1635`（`SendChangeGuildName`） | `0,0,0,0` | 有会 `EncodeString("{GuildName}/{GuildRankName}")`；无会**纯头** |
//! | 753 | [`sm_guild_open_dlg_753`] | `PlayObject.Operate.cs:1269-1270` | `0,0,0,Series=1` | `EncodeString(panel)` |
//! | 754 | [`sm_guild_open_dlg_fail_754`] | `PlayObject.Operate.cs:1274` | 全 0 | 无 |
//! | 756 | [`sm_guild_member_list_756`] | `PlayObject.Operate.cs:1303-1304` | `0,0,0,Series=1` | `EncodeString(text)` |
//! | 757 | [`sm_guild_add_member_ok_757`] | `PlayObject.Operate.cs:1361` | 全 0 | 无 |
//! | 758 | [`sm_guild_add_member_fail_758`] | `PlayObject.Operate.cs:1365` | `Recog=nC(1..5)` | 无 |
//! | 759 | [`sm_guild_del_member_ok_759`] | `PlayObject.Operate.cs:1418` | 全 0 | 无 |
//! | 760 | [`sm_guild_del_member_fail_760`] | `PlayObject.Operate.cs:1422` | `Recog=nC(1..4)` | 无 |
//! | 761 | [`sm_guild_rank_update_fail_761`] | `PlayObject.Operate.cs:1460` | `Recog=nC(<=-2)` | 无 |
//! | 762 | [`sm_buildguild_ok_762`] | `PlayObject.Message.cs:1937-1939` | 全 0 | 无 |
//! | 763 | [`sm_buildguild_fail_763`] | `PlayObject.Message.cs:1940-1942` | `Recog=result(<0)` | 无 |
//! | 768 | [`sm_guild_make_ally_ok_768`] | `PlayObject.Operate.cs:1511` | 全 0 | 无 |
//! | 769 | [`sm_guild_make_ally_fail_769`] | `PlayObject.Operate.cs:1515` **和 `:1554`** | `Recog=n8(0..-4)` | 无 |
//! | 770 | [`sm_guild_break_ally_ok_770`] | `PlayObject.Operate.cs:1550` | 全 0 | 无 |
//! | 104 | [`sm_guild_message_104`] | `PlayObject.Message.cs:1720-1722`（`RM_GUILDMESSAGE` 支） | `Recog=收件人 ActorId`、`Param=MakeWord(F,B)`、`Series=1` | `EncodeString(prefix+text)` |
//!
//! **§2 的第 16 行 `SM_GUILDBREAKALLY_FAIL(771)` 在本模块故意没有编码函数**：oracle 全仓无发点
//! （简报 §2 表「无发点」、§1.2 #9「771 永不发」）；BreakAlly 失败走的是 **`769` 且 `Recog=0`**
//! （`PlayObject.Operate.cs:1554`）。判据 [`tests::sm_771_has_no_encoder_and_never_appears`] 锁死这一点。
//!
//! ## 三条容易写错的口径（简报 §0）
//!
//! 1. **750 只由 `RM_CHANGEGUILDNAME` 触发**：该 RM 只在 `GuildInfo.GetRankName`（`GuildInfo.cs:499`，
//!    上线物化时）与 `PlayObject.RefRankInfo`（`PlayObject.Base.cs:2655-2660`）里 `SendMsg`。
//!    ⇒ 无行会**登录**时一条 750 都不发。编码层用 [`rm_changename_750`] 表达这个 RM 门：
//!    `None`（MyGuild 为空且无 RefRankInfo 调用）⇒ **不出帧**；有会 ⇒ 出帧。
//!    [`sm_guild_changename_750`] 本身复刻 `SendChangeGuildName`：无会时仍是**纯头**（供
//!    「MyGuild 已清空后 `RefRankInfo(0, "")`」路径用）。
//! 2. **尾串口径 = `EDCode.EncodeString`**（`src/OpenMir2/EDcode.cs:175-185`）：
//!    `HUtil32.GetBytes` = GB2312/GBK 字节 → `EncryptUtil.Encode` ⇒ [`encode_string`]。
//!    本模块统一走 [`game_svr::downlink::gbk`] + [`edcode::encode`]，不另写一份编码。
//! 3. **`Series=1` 的帧只有 753 / 756 / 104**（oracle `MakeMessage(..., 1)` 末参）；
//!    其余行会帧末参均为 0。750 也**不是** `Series=1`（`SendDefMessage(..., 0, 0, 0, 0, msg)`）。
//!
//! ## 「尾为空是否仍发」的两类门（本模块据此返回 `Option` 或恒 `Vec`）
//!
//! - `SendDefMessage(wIdent, ..., sMsg)`（`PlayObject.Socket.cs:27-45`）：`sMsg` 为空**仍发纯头**。
//!   ⇒ 750（无会）/ 762 / 763 等恒返回 `Vec`。
//! - `MakeMessage(...) + SendSocket(defMsg, sMsg)`（`PlayObject.Socket.cs:101-116`）：
//!   `sMsg` 为空则**整帧不发**（`:108-111` 早退）。⇒ 753 / 756 / 104 在尾编码为空时返回 `None`。
//!
//! 编码器**不做派发、不做会话态**（G2）；输入文本（753 面板、756 成员列表、104 行会消息）由调用方
//! 按 oracle 构造好（面板/成员列表构造器属 G2 的 `crates/world`；104 的前缀规则见
//! `GuildInfo.cs:435-438` 的 `ShowPreFixMsg`/`GuildMsgPreFix`）。

#![allow(dead_code)] // G1 只交编码层；bin 副本暂无调用点（派发属 G2）。

use game_svr::downlink::gbk;
use game_svr::smframe::{command_header, gm_data_body, make_word};
use mir2_protocol::{edcode, messages};

/// `EDCode.EncodeString(s)`（`src/OpenMir2/EDcode.cs:175-185`）：
/// `HUtil32.GetBytes`（GB2312/GBK）→ `EncryptUtil.Encode`。
///
/// 复用既有实现：`downlink::gbk`（`encoding_rs::GBK`）+ `edcode::encode`（逐字节金标准已另有判据）。
fn encode_string(s: &str) -> Vec<u8> {
    edcode::encode(&gbk(s))
}

/// `SM_CHANGEGUILDNAME(750)`：`PlayObject.Base.cs:1625-1635` 的 `SendChangeGuildName`。
///
/// - 有会：`SendDefMessage(SM_CHANGEGUILDNAME, 0, 0, 0, 0, GuildName + '/' + GuildRankName)`
///   ⇒ 尾 `EncodeString("{GuildName}/{GuildRankName}")`；
/// - 无会：`SendDefMessage(SM_CHANGEGUILDNAME, 0, 0, 0, 0)` ⇒ **纯头**（`sMsg` 为空时
///   `SendDefMessage` 仍发纯头，见 `PlayObject.Socket.cs:33-44`）。
///
/// **无会登录不发 750** 的门由 [`rm_changename_750`] 表达，不在本函数里。
#[must_use]
pub fn sm_guild_changename_750(guild_name: Option<&str>, rank_name: &str) -> Vec<u8> {
    let header = command_header(0, messages::SM_CHANGEGUILDNAME, 0, 0, 0);
    match guild_name {
        Some(name) => gm_data_body(header, &encode_string(&format!("{name}/{rank_name}"))),
        None => gm_data_body(header, &[]),
    }
}

/// **登录路径**的 750 触发门（简报 §0.3）：`GuildInfo.GetRankName`（`GuildInfo.cs:485-509`）
/// 只在 `MyGuild != null` 时 `SendMsg(RM_CHANGEGUILDNAME)` ⇒ **无会登录一条 750 都不发**。
///
/// ⚠ 这不是「RM 一律不出 750」：`PlayObject.RefRankInfo`（`PlayObject.Base.cs:2655-2660`）
/// **无条件** `SendMsg(RM_CHANGEGUILDNAME)`；被踢/掌门自删/解散三条路径都是「先清空
/// `MyGuild` 再调 RefRankInfo」⇒ `SendChangeGuildName` 因 `MyGuild==null` 发**纯头 750**
/// （`Operate.cs:1383-1384/1403-1404`、`GuildInfo.cs:605-606`）。那些路径**直接**
/// 调 [`sm_guild_changename_750`]（`None` 时它本就产纯头），不要经过本函数。
///
/// - `guild_name == None` ⇒ `None`（登录无会不发）；
/// - 有会 ⇒ 出 [`sm_guild_changename_750`] 的帧。
#[must_use]
pub fn login_changename_750(guild_name: Option<&str>, rank_name: &str) -> Option<Vec<u8>> {
    guild_name.map(|_| sm_guild_changename_750(guild_name, rank_name))
}

/// `SM_OPENGUILDDLG(753)`：`PlayObject.Operate.cs:1269-1270`（`ClientOpenGuildDlg` 的有会支）。
///
/// `MakeMessage(SM_OPENGUILDDLG, 0, 0, 0, 1)` + `SendSocket(defMsg, EDCode.EncodeString(panel))`。
/// `SendSocket(defMsg, sMsg)` 在 `sMsg` 为空时**整帧不发**（`PlayObject.Socket.cs:108-111`），
/// 故尾编码为空时返回 `None`。
///
/// `panel` 由 G2 的 `crates/world` 构造器按 `Operate.cs:1233-1267` 拼好（本函数只做编码）。
#[must_use]
pub fn sm_guild_open_dlg_753(panel: &str) -> Option<Vec<u8>> {
    let tail = encode_string(panel);
    if tail.is_empty() {
        return None;
    }
    let header = command_header(0, messages::SM_OPENGUILDDLG, 0, 0, 1);
    Some(gm_data_body(header, &tail))
}

/// `SM_OPENGUILDDLG_FAIL(754)`：`PlayObject.Operate.cs:1274`（`ClientOpenGuildDlg` 的无会支）。
///
/// `SendDefMessage(SM_OPENGUILDDLG_FAIL, 0, 0, 0, 0)` ⇒ `Recog=0, Param=0, Tag=0, Series=0`，无尾。
#[must_use]
pub fn sm_guild_open_dlg_fail_754() -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_OPENGUILDDLG_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_SENDGUILDMEMBERLIST(756)`：`PlayObject.Operate.cs:1303-1304`（`ClientGuildMemberList`）。
///
/// `MakeMessage(SM_SENDGUILDMEMBERLIST, 0, 0, 0, 1)` + `SendSocket(defMsg, EncodeString(text))`。
/// **无会时 oracle 在 `:1285-1288` 直接 return（零帧）**，本函数的 `None`（空尾）对应
/// 有会但文本为空（如 `RankList` 为空）时 `SendSocket` 的早退。
///
/// `text` 的既有生产源是 `GuildState::member_list_text`（`crates/world/src/guild.rs`，含
/// `>5000` UTF-16 门，逐字节已对）。
#[must_use]
pub fn sm_guild_member_list_756(text: &str) -> Option<Vec<u8>> {
    let tail = encode_string(text);
    if tail.is_empty() {
        return None;
    }
    let header = command_header(0, messages::SM_SENDGUILDMEMBERLIST, 0, 0, 1);
    Some(gm_data_body(header, &tail))
}

/// `SM_GUILDADDMEMBER_OK(757)`：`PlayObject.Operate.cs:1361`。
///
/// `SendDefMessage(SM_GUILDADDMEMBER_OK, 0, 0, 0, 0)` ⇒ 全 0、无尾。
#[must_use]
pub fn sm_guild_add_member_ok_757() -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_GUILDADDMEMBER_OK, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDADDMEMBER_FAIL(758)`：`PlayObject.Operate.cs:1365`。
///
/// `SendDefMessage(SM_GUILDADDMEMBER_FAIL, nC, 0, 0, 0)`，`nC` 是 C# `byte` 语义：
/// `1` 非掌门 / `2` 目标不在线或没面对 / `3` 已在本会 / `4` 已在别的会或 rank 数 >= 400 /
/// `5` `AllowGuild=false`（`Operate.cs:1309-1360`）。
#[must_use]
pub fn sm_guild_add_member_fail_758(n_c: u8) -> Vec<u8> {
    gm_data_body(
        command_header(i32::from(n_c), messages::SM_GUILDADDMEMBER_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDDELMEMBER_OK(759)`：`PlayObject.Operate.cs:1418`。
///
/// `SendDefMessage(SM_GUILDDELMEMBER_OK, 0, 0, 0, 0)` ⇒ 全 0、无尾。
#[must_use]
pub fn sm_guild_del_member_ok_759() -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_GUILDDELMEMBER_OK, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDDELMEMBER_FAIL(760)`：`PlayObject.Operate.cs:1422`。
///
/// `SendDefMessage(SM_GUILDDELMEMBER_FAIL, nC, 0, 0, 0)`：
/// `1` 非掌门 / `2` 不是本会成员 / `3` 自删但 `CancelGuld` 失败 / `4` `DelMember` 失败
/// （`Operate.cs:1369-1425`；自删成功时 `nC` 最终置 0，见 `:1407`）。
#[must_use]
pub fn sm_guild_del_member_fail_760(n_c: u8) -> Vec<u8> {
    gm_data_body(
        command_header(i32::from(n_c), messages::SM_GUILDDELMEMBER_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDRANKUPDATE_FAIL(761)`：`PlayObject.Operate.cs:1460`。
///
/// `SendDefMessage(SM_GUILDRANKUPDATE_FAIL, nC, 0, 0, 0)`，且 oracle 只在 `nC <= -2` 时发
/// （`nC == -1` 静默，`Operate.cs:1455-1461`）。`Recog` 是**有符号 i32**（`-2..-7`），
/// 不能按 `u16` 落格。
#[must_use]
pub fn sm_guild_rank_update_fail_761(n_c: i32) -> Vec<u8> {
    gm_data_body(
        command_header(n_c, messages::SM_GUILDRANKUPDATE_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_BUILDGUILD_OK(762)`：`PlayObject.Message.cs:1937-1939`（`RM_BUILDGUILD_OK` 支）。
///
/// `SendDefMessage(SM_BUILDGUILD_OK, 0, 0, 0, 0)` ⇒ 全 0、无尾。触发方是 NPC
/// `GuildOfficial.ReQuestBuildGuild`（`src/GameSrv/Npc/GuildOfficial.cs:163-171`）。
#[must_use]
pub fn sm_buildguild_ok_762() -> Vec<u8> {
    gm_data_body(command_header(0, messages::SM_BUILDGUILD_OK, 0, 0, 0), &[])
}

/// `SM_BUILDGUILD_FAIL(763)`：`PlayObject.Message.cs:1940-1942`（`RM_BUILDGUILD_FAIL` 支）。
///
/// `SendDefMessage(SM_BUILDGUILD_FAIL, processMsg.nParam1, 0, 0, 0)`，`nParam1` 是
/// `GuildOfficial.cs:169` 的 `SendMsg(this, RM_BUILDGUILD_FAIL, 0, result, 0, 0)`
/// 带过来的 result：`-1` 已有会 / `-2` 钱不足 / `-3` 无号角 / `-4` 空名或建会失败。
#[must_use]
pub fn sm_buildguild_fail_763(result: i32) -> Vec<u8> {
    gm_data_body(
        command_header(result, messages::SM_BUILDGUILD_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDMAKEALLY_OK(768)`：`PlayObject.Operate.cs:1511`。
///
/// `SendDefMessage(SM_GUILDMAKEALLY_OK, 0, 0, 0, 0)` ⇒ 全 0、无尾。
/// **当前部署不可达**：`GuildInfo.EnableAuthAlly` 恒 false（ctor 置 false，唯一取反
/// `AuthallyCommand.cs:13` 被注释）⇒ `ClientGuildAlly` 恒走 `n8=-4`。
#[must_use]
pub fn sm_guild_make_ally_ok_768() -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_GUILDMAKEALLY_OK, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDMAKEALLY_FAIL(769)`：`PlayObject.Operate.cs:1515`（Ally）**和 `:1554`**（BreakAlly）。
///
/// - Ally：`SendDefMessage(SM_GUILDMAKEALLY_FAIL, n8, 0, 0, 0)`，`n8 ∈ {-1,-2,-3,-4}`（初值 `-1`；当前恒 `-4`）；
/// - **BreakAlly 失败也发 769 且 `Recog=0`**（`:1554`，不是 771/`-1`）。
///
/// `n8` 是**有符号 i32**（负数必须完整进 `Recog`）。
#[must_use]
pub fn sm_guild_make_ally_fail_769(n8: i32) -> Vec<u8> {
    gm_data_body(
        command_header(n8, messages::SM_GUILDMAKEALLY_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDBREAKALLY_OK(770)`：`PlayObject.Operate.cs:1550`。
///
/// `SendDefMessage(SM_GUILDBREAKALLY_OK, 0, 0, 0, 0)` ⇒ 全 0、无尾。
/// **当前部署不可达**：联盟表恒空（`LoadGuild` 丢弃联盟行）⇒ `IsAllyGuild` 恒 false。
#[must_use]
pub fn sm_guild_break_ally_ok_770() -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_GUILDBREAKALLY_OK, 0, 0, 0),
        &[],
    )
}

/// `SM_GUILDMESSAGE(104)`：`PlayObject.Message.cs:1720-1722` 的 `RM_GUILDMESSAGE` 支。
///
/// `MakeMessage(SM_GUILDMESSAGE, processMsg.ActorId, MakeWord((ushort)nParam1, (ushort)nParam2), 0, 1)`
/// + `SendSocket(defMsg, EDCode.EncodeString(processMsg.Msg))`。
///
/// - `Recog` = **收件人自己**的 `ActorId`（`GuildInfo.SendGuildMsg` 用
///   `guildMember.SendMsg(guildMember, ...)`，`GuildInfo.cs:451`）；
/// - `Param = MakeWord(FColor, BColor)`（低字节 F、高字节 B，复用 [`make_word`]）；
/// - `Series = 1`；尾 = `EncodeString(text)`，`text` 已由调用方按 `ShowPreFixMsg` 拼好前缀；
/// - `SendSocket(defMsg, sMsg)` 在尾为空时整帧不发 ⇒ 空文本返回 `None`。
#[must_use]
pub fn sm_guild_message_104(
    recipient_actor_id: i32,
    f_color: u8,
    b_color: u8,
    text: &str,
) -> Option<Vec<u8>> {
    let tail = encode_string(text);
    if tail.is_empty() {
        return None;
    }
    let header = command_header(
        recipient_actor_id,
        messages::SM_GUILDMESSAGE,
        make_word(f_color, b_color),
        0,
        1,
    );
    Some(gm_data_body(header, &tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recog(b: &[u8]) -> i32 {
        i32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }

    fn ident(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[4], b[5]])
    }

    fn param(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[6], b[7]])
    }

    fn tag(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[8], b[9]])
    }

    fn series(b: &[u8]) -> u16 {
        u16::from_le_bytes([b[10], b[11]])
    }

    /// `CommandMessage` 12B 头的逐字节断言（小端）。
    fn assert_header(
        body: &[u8],
        exp_recog: i32,
        exp_ident: u16,
        exp_param: u16,
        exp_tag: u16,
        exp_series: u16,
    ) {
        assert_eq!(recog(body), exp_recog, "Recog(i32)");
        assert_eq!(ident(body), exp_ident, "Ident(u16)");
        assert_eq!(param(body), exp_param, "Param(u16)");
        assert_eq!(tag(body), exp_tag, "Tag(u16)");
        assert_eq!(series(body), exp_series, "Series(u16)");
        assert_eq!(
            &body[..12],
            &command_header(exp_recog, exp_ident, exp_param, exp_tag, exp_series)
        );
    }

    // ---- 750 ----

    /// 判据 G1-3：有会 750 尾 = `EncodeString("会名/封号")`（GBK 字节），头全 0。
    /// oracle：`PlayObject.Base.cs:1627-1630`。
    #[test]
    fn sm_changename_750_with_guild_tail_is_gbk_encode_string() {
        let body = sm_guild_changename_750(Some("测试会"), "掌门");
        assert_header(&body, 0, messages::SM_CHANGEGUILDNAME, 0, 0, 0);
        // oracle 的字符串字面量 = GuildName + '/' + GuildRankName；GBK 字节由 downlink::gbk 给。
        let mut expected_text = Vec::new();
        expected_text.extend_from_slice(&gbk("测试会/掌门"));
        assert_eq!(&body[12..], &edcode::encode(&expected_text)[..]);
        // 明确钉住 GBK 而不是 UTF-8：UTF-8 的「测试会」是 9 字节，GBK 是 6 字节。
        assert_eq!(gbk("测试会").len(), 6);
        // 尾的反解必须是 `会名/封号`（GBK 解码）。
        let decoded = edcode::decode(&body[12..]);
        assert_eq!(encoding_rs::GBK.decode(&decoded).0, "测试会/掌门");
    }

    /// 判据 G1-3（空尾/纯头态）：无会 750 = **纯头 12B**，仍发帧。
    /// oracle：`PlayObject.Base.cs:1631-1634` + `PlayObject.Socket.cs:33-44`（SendDefMessage 空尾仍发）。
    #[test]
    fn sm_changename_750_without_guild_is_header_only() {
        let body = sm_guild_changename_750(None, "");
        assert_header(&body, 0, messages::SM_CHANGEGUILDNAME, 0, 0, 0);
        assert_eq!(body.len(), 12, "无会 750 是纯头，不得带尾");
    }

    /// 判据 G1-3（§0.3 的**登录**门）：无会登录**一条 750 都不发**；有会才出帧。
    /// oracle：`GuildInfo.cs:499` 只在 MyGuild 非空时 SendMsg。
    ///
    /// 牙：把 `login_changename_750` 对 `None` 改成也出帧 ⇒ 本用例红。
    #[test]
    fn login_changename_750_skips_frame_without_guild() {
        assert_eq!(login_changename_750(None, ""), None);
        let body = login_changename_750(Some("会"), "成员").expect("有会必须有 750");
        assert_eq!(ident(&body), messages::SM_CHANGEGUILDNAME);
        // RefRankInfo 路径（被踢/自删/解散）直接调编码器：MyGuild 已清空时发**纯头**。
        let kicked = sm_guild_changename_750(None, "");
        assert_header(&kicked, 0, messages::SM_CHANGEGUILDNAME, 0, 0, 0);
        assert_eq!(kicked.len(), 12, "RefRankInfo 路径的无会 750 是纯头");
    }

    // ---- 753 / 754 ----

    /// 判据 G1-1：753 面板帧的完整字节：`Recog=0,Param=0,Tag=0,Series=1` + `EncodeString(panel)`。
    /// 面板文本按 oracle `Operate.cs:1233-1267` 的顺序与字面量构造（构造器属 G2，本测只取字面量）：
    /// `"{会名}\r \r" + "1\r" + "<Notice>\r" + 公告 + "<KillGuilds>\r" + 战表 +
    /// "<AllyGuilds>\r"`。战表项是 `WarGuild.ToString()` 的**原始**输出
    /// （`src/Modules/SystemModule/Data/WarGuild.cs:5-10`；`GuildInfo` 未重写 ToString）。
    #[test]
    fn sm_guild_open_dlg_753_panel_bytes_rank1_and_rank99() {
        let panel_rank1 = "测试会\r \r1\r<Notice>\r公告一\r公告二\r<KillGuilds>\r\
WarGuild { Guild = M2Server.Guild.GuildInfo, WarTick = 123, WarTime = 45 }\r\
<AllyGuilds>\r";
        let body = sm_guild_open_dlg_753(panel_rank1).expect("面板非空必须发");
        assert_header(&body, 0, messages::SM_OPENGUILDDLG, 0, 0, 1);
        // 逐字节：12B 头 + edcode::encode(GBK(panel))。
        let mut expected = Vec::new();
        expected.extend_from_slice(&command_header(0, 753, 0, 0, 1));
        expected.extend_from_slice(&edcode::encode(&gbk(panel_rank1)));
        assert_eq!(body, expected);
        // 反解确认 `\r \r`（第二行是一个空格）不被「美化」成 `\r\r`。
        let decoded = edcode::decode(&body[12..]);
        let text = encoding_rs::GBK.decode(&decoded).0;
        assert!(text.starts_with("测试会\r \r1\r<Notice>\r"), "got {text:?}");
        assert!(!text.starts_with("测试会\r\r"), "第二行必须是一个空格");

        // rank != 1 ⇒ 第三行是 '0'。
        let panel_rank99 = panel_rank1.replacen("\r \r1\r", "\r \r0\r", 1);
        let body99 = sm_guild_open_dlg_753(&panel_rank99).expect("面板非空必须发");
        assert_header(&body99, 0, messages::SM_OPENGUILDDLG, 0, 0, 1);
        let raw99 = edcode::decode(&body99[12..]);
        let text99 = encoding_rs::GBK.decode(&raw99).0;
        assert!(
            text99.starts_with("测试会\r \r0\r<Notice>\r"),
            "got {text99:?}"
        );
    }

    /// 判据：753 走 `SendSocket(defMsg, sMsg)`（`PlayObject.Socket.cs:101-116`），空尾**整帧不发**。
    #[test]
    fn sm_guild_open_dlg_753_empty_panel_sends_nothing() {
        assert_eq!(sm_guild_open_dlg_753(""), None);
    }

    /// 判据 G1-2（754）：无会 = 纯头全 0（`Operate.cs:1274`）。
    #[test]
    fn sm_guild_open_dlg_fail_754_is_header_only() {
        let body = sm_guild_open_dlg_fail_754();
        assert_header(&body, 0, messages::SM_OPENGUILDDLG_FAIL, 0, 0, 0);
        assert_eq!(body, command_header(0, 754, 0, 0, 0).to_vec());
    }

    // ---- 756 ----

    /// 判据 G1-4：756 成员列表：`Series=1` + `EncodeString(text)`（`Operate.cs:1303-1304`）。
    /// 文本用 `GuildState::member_list_text` 的 oracle 格式 `#1/*掌门/甲/`。
    #[test]
    fn sm_guild_member_list_756_series_one_and_gbk_tail() {
        let text = "#1/*掌门/甲/乙/";
        let body = sm_guild_member_list_756(text).expect("非空列表必须发");
        assert_header(&body, 0, messages::SM_SENDGUILDMEMBERLIST, 0, 0, 1);
        assert_eq!(&body[12..], &edcode::encode(&gbk(text))[..]);
        let raw = edcode::decode(&body[12..]);
        let text2 = encoding_rs::GBK.decode(&raw).0;
        assert_eq!(text2, text);
    }

    /// 判据（空尾/不发态）：756 的空文本在 `SendSocket(defMsg, sMsg)` 早退 ⇒ 零帧。
    /// oracle `Operate.cs:1285-1288` 的无会早退与本函数的空文本分支是两条不同的「零帧」。
    #[test]
    fn sm_guild_member_list_756_empty_sends_nothing() {
        assert_eq!(sm_guild_member_list_756(""), None);
    }

    // ---- 757 / 758 ----

    /// 判据 G1-2（757）：成功 = 纯头全 0（`Operate.cs:1361`）。
    #[test]
    fn sm_guild_add_member_ok_757_is_header_only() {
        let body = sm_guild_add_member_ok_757();
        assert_header(&body, 0, messages::SM_GUILDADDMEMBER_OK, 0, 0, 0);
        assert_eq!(body, command_header(0, 757, 0, 0, 0).to_vec());
    }

    /// 判据 G1-2（758）：`Recog` 取 `nC` 的 byte 语义（1..5），其余全 0（`Operate.cs:1365`）。
    #[test]
    fn sm_guild_add_member_fail_758_recog_is_byte_code() {
        for n in 1u8..=5 {
            let body = sm_guild_add_member_fail_758(n);
            assert_header(
                &body,
                i32::from(n),
                messages::SM_GUILDADDMEMBER_FAIL,
                0,
                0,
                0,
            );
            assert_eq!(body.len(), 12);
        }
    }

    // ---- 759 / 760 ----

    /// 判据 G1-2（759）：成功 = 纯头全 0（`Operate.cs:1418`）。
    #[test]
    fn sm_guild_del_member_ok_759_is_header_only() {
        let body = sm_guild_del_member_ok_759();
        assert_header(&body, 0, messages::SM_GUILDDELMEMBER_OK, 0, 0, 0);
        assert_eq!(body, command_header(0, 759, 0, 0, 0).to_vec());
    }

    /// 判据 G1-2（760）：`Recog` 取 `nC`（1..4），其余全 0（`Operate.cs:1422`）。
    #[test]
    fn sm_guild_del_member_fail_760_recog_is_byte_code() {
        for n in 1u8..=4 {
            let body = sm_guild_del_member_fail_760(n);
            assert_header(
                &body,
                i32::from(n),
                messages::SM_GUILDDELMEMBER_FAIL,
                0,
                0,
                0,
            );
            assert_eq!(body.len(), 12);
        }
    }

    // ---- 761 ----

    /// 判据 G1-2（761）：`Recog` 是**有符号** i32（-2..-7），不能按 u16 落格（`Operate.cs:1460`）。
    #[test]
    fn sm_guild_rank_update_fail_761_keeps_signed_recog() {
        for n in [-2i32, -3, -4, -5, -6, -7] {
            let body = sm_guild_rank_update_fail_761(n);
            assert_header(&body, n, messages::SM_GUILDRANKUPDATE_FAIL, 0, 0, 0);
            assert_eq!(body.len(), 12);
        }
        assert_eq!(
            &sm_guild_rank_update_fail_761(-2)[..4],
            &(-2i32).to_le_bytes()
        );
    }

    // ---- 762 / 763 ----

    /// 判据：762 建会成功 = 纯头全 0（`PlayObject.Message.cs:1937-1939`）。
    #[test]
    fn sm_buildguild_ok_762_is_header_only() {
        let body = sm_buildguild_ok_762();
        assert_header(&body, 0, messages::SM_BUILDGUILD_OK, 0, 0, 0);
        assert_eq!(body, command_header(0, 762, 0, 0, 0).to_vec());
    }

    /// 判据：763 建会失败 = `Recog=result`（负数）且无尾
    /// （`PlayObject.Message.cs:1940-1942`，result 来自 `GuildOfficial.cs:169` 的 nParam1）。
    #[test]
    fn sm_buildguild_fail_763_recog_is_negative_result() {
        for r in [-1i32, -2, -3, -4] {
            let body = sm_buildguild_fail_763(r);
            assert_header(&body, r, messages::SM_BUILDGUILD_FAIL, 0, 0, 0);
        }
    }

    // ---- 768 / 769 / 770 ----

    /// 判据：768 = 纯头全 0（`Operate.cs:1511`）；当前不可达（EnableAuthAlly 恒 false）。
    #[test]
    fn sm_guild_make_ally_ok_768_is_header_only() {
        let body = sm_guild_make_ally_ok_768();
        assert_header(&body, 0, messages::SM_GUILDMAKEALLY_OK, 0, 0, 0);
        assert_eq!(body, command_header(0, 768, 0, 0, 0).to_vec());
    }

    /// 判据 G1-2（769）：Ally 失败 `Recog=n8 ∈ {-1..-4}`；**BreakAlly 失败也是 769 且 Recog=0**
    /// （`Operate.cs:1554`），本函数是两条路径的唯一编码器。
    #[test]
    fn sm_guild_make_ally_fail_769_covers_breakally_zero() {
        for n in [-4i32, -3, -2, -1, 0] {
            let body = sm_guild_make_ally_fail_769(n);
            assert_header(&body, n, messages::SM_GUILDMAKEALLY_FAIL, 0, 0, 0);
            assert_eq!(body.len(), 12);
        }
    }

    /// 判据：770 = 纯头全 0（`Operate.cs:1550`）；当前不可达（联盟表恒空）。
    #[test]
    fn sm_guild_break_ally_ok_770_is_header_only() {
        let body = sm_guild_break_ally_ok_770();
        assert_header(&body, 0, messages::SM_GUILDBREAKALLY_OK, 0, 0, 0);
        assert_eq!(body, command_header(0, 770, 0, 0, 0).to_vec());
    }

    // ---- 104 ----

    /// 判据 G1-5：104 的 `Recog` = **收件人自己**的 ActorId（不是发送者）、
    /// `Param=MakeWord(F,B)`、`Series=1`、尾 = `EncodeString(text)`。
    /// oracle：`PlayObject.Message.cs:1720-1722` + `GuildInfo.cs:451`。
    #[test]
    fn sm_guild_message_104_recog_is_recipient_and_series_one() {
        let body = sm_guild_message_104(9001, 0x0A, 0x0B, "行会消息").expect("非空文本必须发");
        assert_header(
            &body,
            9001,
            messages::SM_GUILDMESSAGE,
            make_word(0x0A, 0x0B),
            0,
            1,
        );
        assert_eq!(&body[12..], &edcode::encode(&gbk("行会消息"))[..]);
        // 牙的对照：若误用发送者 id（这里用 0），Recog 断言会红。
        assert_ne!(recog(&body), 0, "Recog 必须是收件人 ActorId，不是发送者/0");
    }

    /// 判据（空尾/不发态）：104 走 `SendSocket(defMsg, sMsg)`，空文本整帧不发
    /// （`PlayObject.Socket.cs:108-111`）。
    #[test]
    fn sm_guild_message_104_empty_sends_nothing() {
        assert_eq!(sm_guild_message_104(1, 0, 0, ""), None);
    }

    // ---- 771 的反向判据 ----

    /// 判据 G1-2 的「771 永不发」：§2 第 16 行 `SM_GUILDBREAKALLY_FAIL(771)` **没有编码函数**，
    /// 且所有行会编码器在任何输入下都不产出 ident 771。
    /// oracle：全仓无 771 发点；BreakAlly 失败发 769/0（`Operate.cs:1554`）。
    #[test]
    fn sm_771_has_no_encoder_and_never_appears() {
        assert_eq!(
            messages::SM_GUILDBREAKALLY_FAIL,
            771,
            "常量存在但 oracle 不用"
        );
        let mut bodies: Vec<Vec<u8>> = vec![
            sm_guild_changename_750(None, ""),
            sm_guild_changename_750(Some("会"), "封"),
            sm_guild_open_dlg_753("p").unwrap(),
            sm_guild_open_dlg_fail_754(),
            sm_guild_member_list_756("t").unwrap(),
            sm_guild_add_member_ok_757(),
            sm_guild_add_member_fail_758(5),
            sm_guild_del_member_ok_759(),
            sm_guild_del_member_fail_760(4),
            sm_guild_rank_update_fail_761(-7),
            sm_buildguild_ok_762(),
            sm_buildguild_fail_763(-4),
            sm_guild_make_ally_ok_768(),
            sm_guild_make_ally_fail_769(0),
            sm_guild_make_ally_fail_769(-4),
            sm_guild_break_ally_ok_770(),
            sm_guild_message_104(1, 0, 0, "m").unwrap(),
        ];
        for b in bodies.drain(..) {
            assert_ne!(ident(&b), messages::SM_GUILDBREAKALLY_FAIL, "771 永不发");
        }
    }
}
