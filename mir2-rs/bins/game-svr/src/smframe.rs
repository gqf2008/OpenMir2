//! M2 **S2b-4 续**：GameSvr → 客户端的 `SM_*` 入图帧**编码层**（`openmir2-m2-enter-world-frames`）。
//!
//! 判据来源是 M0 金标准 `tests/golden/session-20261010-c3-hook/capture.jsonl`（真 C# 服务端 + 真客户端，
//! `port=7200 && dir=s2c` 共 329 条）。这里先把**两条字节流水线**逐字节钉住，不掺任何"按公式重推"：
//!
//! ```text
//! GameSvr 侧（oracle PlayObject.Socket.cs:101-116 的 SendSocket(defMsg, sMsg)）：
//!   GM_DATA.body = CommandMessage(12B, 解码态) ++ 尾
//!   其中"尾"由各消息自己决定：多为 EDCode 编码后的 role body，少数是原样字面量（如 "<<<<<<"）。
//! 网关侧（oracle GameGate ClientSession.cs:897-920 的 ProcessServerPacket）：
//!   client = '#' ++ Encode(头 12B) ++ 尾(原样) ++ '!'
//! ```
//!
//! **为什么 `Encode(头)` 与"尾原样"能拼**：`encoded_len(12) == 16` 且 `16 % 4 == 0`，
//! 所以 `Encode` 的状态在头边界复位，网关只编码头、尾原样追加，客户端整段解码后仍然正确
//! （`mir2-rs/crates/protocol/src/edcode.rs` 的 `header_encodes_to_multiple_of_4` 锚这条）。
//!
//! **编码层已逐字节对上的 16 条**（金标准 s2c 帧号）：
//! `658`(#1) / `51`(#2) / `50`(#3) / `41`(#4) / `213`(#5) / `11029`(#6) / `42`(#7) / `766`(#8) /
//! `54`(#9) / `5008`(#10) / `1106`(#11) / `52`(#12) / `752`(#13) / `811`(#14) / `46`(#15) / `10`(#16)。
//! 每条都**从金标准的原始字节回读**校验（不按源码公式重推尾字节），尾的**编码口径**逐条照 oracle：
//! - `SendDefMessage(..., sMsg)` ⇒ 尾 = `EncodeString(sMsg)`（658/54/5008；11029 是 `EncodeString(EncodeMessage(cfg))` 双编码）；
//! - `SendSocket(defMsg, sMsg)` ⇒ 尾 = `sMsg` **原样**（52/42/10 的 `sMsg` 已是 `Encode*` 输出）；
//! - `811` 是 `Encode(A)+'/'+Encode(B)+'/'+Encode(C)` 三段拼（`SendSocket` 不再编码）；
//! - `766/752/46` 无尾；`213` 的模式在 **`Recog`**（不是 `Param`，见该函数注释）。
//!
//! **仍缺的运行时输入**（编码层可判，生产路径还取不到；登记在卡，由对应车道补）：
//! - 真地图：`54` 的 `Envir.MapDesc`、`51/46` 的 `DayBright()`、`766` 的地图 Flag（`openmir2-m2-map-loading`）；
//! - 玩家记录字段：`52` 的 `WAbil`、`752` 的 HitPoint/AntiMagic…、`41/50` 的 `GetFeatureToLong`、`52` 的 Gold/GameGold（`openmir2-m2-record-fields`）；
//! - 服务器配置：`11029` 的 `ClientConf`、`5008` 的元宝/金币名、`811` 的三段能力模板（配置层）。
//!   （`658` 的公告已在本片接线，见 `notice.rs`；不再是缺输入项。）
//!
//! 生产路径当前下发 `658`（公告，见 `notice.rs`）+ 输入齐备的 `SM_NEWMAP`（见 `main.rs`）；
//! 其余帧由 [`EnterWorldInputs::frames`] 在输入补齐后按同一顺序产出。

use mir2_protocol::{edcode, messages};

/// `HUtil32.MakeWord(low, high)`（`src/OpenMir2/HUtil32.cs:61-64`）：`(high << 8) | low`。
#[must_use]
pub fn make_word(low: u8, high: u8) -> u16 {
    (u16::from(high) << 8) | u16::from(low)
}

/// `CommandMessage`（`ClientMesaagePacket.cs:11-24`）的 12B 解码态布局：
/// `Recog(i32) + Ident(u16) + Param(u16) + Tag(u16) + Series(u16)`，小端。
#[must_use]
pub fn command_header(recog: i32, ident: u16, param: u16, tag: u16, series: u16) -> [u8; 12] {
    let mut b = [0u8; 12];
    b[0..4].copy_from_slice(&recog.to_le_bytes());
    b[4..6].copy_from_slice(&ident.to_le_bytes());
    b[6..8].copy_from_slice(&param.to_le_bytes());
    b[8..10].copy_from_slice(&tag.to_le_bytes());
    b[10..12].copy_from_slice(&series.to_le_bytes());
    b
}

/// `GM_DATA` 的 body = 12B 命令头 ++ 尾（尾的编码口径由各 `sm_*` 决定，见文件头）。
#[must_use]
pub fn gm_data_body(header: [u8; 12], tail: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(12 + tail.len());
    v.extend_from_slice(&header);
    v.extend_from_slice(tail);
    v
}

/// 网关把 `GM_DATA` body 变成客户端线缆字节（`GameGate/Services/ClientSession.cs:897-920`）：
/// `'#' ++ Encode(头 12B) ++ 尾(原样) ++ '!'`。
///
/// **范围（R4-4）**：只覆盖 oracle `ProcessServerPacket` 的 **`bufferLen >= 0` 支**（即 body 含
/// 12B 命令头的 `SM_*` 帧，本模块产出的全是这类）。`bufferLen < 0` 的**短包支**（`'#'+原样+'!'`、
/// 不 Encode 头）属 game-gate 的发送路径，**不在本模块实现**（另一张卡）；本函数对 <12B 直接
/// panic，拒绝被当作短包支误用。
///
/// # Panics
/// body 不足 12B 时 panic（本模块的自家构造器恒 >= 12）。
#[must_use]
pub fn client_frame(gm_data: &[u8]) -> Vec<u8> {
    assert!(
        gm_data.len() >= 12,
        "GM_DATA body 必须含 12B 命令头（短包支属 game-gate）"
    );
    let mut out = Vec::with_capacity(gm_data.len() + 6);
    out.push(b'#');
    out.extend_from_slice(&edcode::encode(&gm_data[..12]));
    out.extend_from_slice(&gm_data[12..]);
    out.push(b'!');
    out
}

/// `SM_NEWMAP`（`PlayObject.Message.cs:1681` 的 `RM_LOGON` 支）：
/// `MakeMessage(SM_NEWMAP, ActorId, CurrX, CurrY, DayBright())` + 尾 `EDCode.EncodeString(MapFileName)`。
///
/// `map_file_name` 取**原始字节**（现网地图名是 GB2312 字节流，本层不转码）。
#[must_use]
pub fn sm_newmap(
    recog: i32,
    cur_x: u16,
    cur_y: u16,
    day_bright: u16,
    map_file_name: &[u8],
) -> Vec<u8> {
    let header = command_header(recog, messages::SM_NEWMAP, cur_x, cur_y, day_bright);
    gm_data_body(header, &edcode::encode(map_file_name))
}

/// `SM_LOGON`（`PlayObject.cs:1130-1140` 的 `SendLogon`）：
/// `MakeMessage(SM_LOGON, ActorId, CurrX, CurrY, MakeWord(Dir, Light))` +
/// 尾 `EDCode.EncodePacket(MessageBodyWL)`（4×i32 = 16B）。
#[must_use]
pub fn sm_logon(
    recog: i32,
    cur_x: u16,
    cur_y: u16,
    dir: u8,
    light: u8,
    body_wl: [u8; 16],
) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_LOGON,
        cur_x,
        cur_y,
        make_word(dir, light),
    );
    gm_data_body(header, &edcode::encode(&body_wl))
}

/// `SM_FEATURECHANGED`（`PlayObject.cs:1145`）：
/// `MakeMessage(SM_FEATURECHANGED, ActorId, LoWord(nRecog), HiWord(nRecog), GetFeatureEx())`，无尾。
#[must_use]
pub fn sm_featurechanged(recog: i32, n_recog: i32, feature_ex: u16) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_FEATURECHANGED,
        lo_word(n_recog),
        hi_word(n_recog),
        feature_ex,
    );
    gm_data_body(header, &[])
}

/// `SM_ATTACKMODE`（`PlayObject.cs:1146`）：`SendDefMessage(SM_ATTACKMODE, (byte)AttatckMode, 0, 0, 0)`，无尾。
///
/// ⚠️ 攻击模式落在 **`Recog`**（`SendDefMessage` 的第 2 参 = `nRecog`），不是 `Param`——
/// `MakeMessage(ident, recog, param, tag, series)`（`Messages.cs:846-857`）。
/// 金标准 #5 恰好全 0，看不出差别；这里按 oracle 字段序钉死。
#[must_use]
pub fn sm_attackmode(mode: u8) -> Vec<u8> {
    let header = command_header(i32::from(mode), messages::SM_ATTACKMODE, 0, 0, 0);
    gm_data_body(header, &[])
}

/// **S3 后半段（第二格）**：`CM_PICKUP`/`CM_DROPITEM` 的回帧与地面/背包延时通知的 `SM_*` 编码。
///
/// 逐条口径（oracle 行号见 `## pickup/drop 接线` 的作者 comment `d552d2a2`）：
/// - `SM_ADDITEM`（`PlayObject.Base.cs:225-227`）：头 = `MakeMessage(SM_ADDITEM, ActorId, 0, 0, 1)`；
///   oracle 随后 `SendSocket(ClientMsg, EDCode.EncodeBuffer(clientItem))`——**带一条 `ClientItem` 尾**。
///   ⚠️ 本函数当前只产 **header-only**：`ClientItem` 的构造（`GetUpgradeStdItem` /
///   `CustomItemSystem.GetItemName` / `MakeIndex·Dura·DuraMax` / `StdMode==50` 的名称 ` #Dura` 后缀 /
///   `StdModeMap` 的 `Desc[8]`⇒Shape 0/130）属物品**展示层**，尚未移植 ⇒ **已登记为已知缺失
///   T-24**（`mir2-rs/tests/parity/whitelist.md`）。接线层 `pickwire::immediate_frame` 对“拾取普通
///   物品成功”恒交 header-only；**在其实现前不得把真机 `SM_ADDITEM` 字节对拍判 GREEN**。
/// - `SM_ITEMHIDE`（`PlayObject.Message.cs:1748`）：`SendDefMessage(SM_ITEMHIDE, nParam1, nParam2, nParam3, 0)`
///   ⇒ `Recog=nParam1`(=物品 `ItemId`)、`Param=nParam2`(=x)、`Tag=nParam3`(=y)、`Series=0`。
/// - `SM_WEIGHTCHANGED`（`:1760`）：`SendDefMessage(SM_WEIGHTCHANGED, WAbil.Weight, WAbil.WearWeight, WAbil.HandWeight, chk)`
///   ⇒ `Recog=Weight`、`Param=WearWeight`、`Tag=HandWeight`、`Series=((W+A+H)^0x3A5F)^0x1F35^0xaa21`
///   （**用当下 `WAbil`**，不是 `RM_WEIGHTCHANGED` 的字段）。
/// - `SM_GOLDCHANGED`（`:1834`）：`SendDefMessage(SM_GOLDCHANGED, Gold, LoWord(GameGold), HiWord(GameGold), 0)`。
/// - `SM_MENU_OK`（`:1953`）：`SendDefMessage(SM_MENU_OK, processMsg.nParam1, 0, 0, 0, processMsg.Msg)`。
/// - `SM_DROPITEM_SUCCESS/FAIL`（`PlayObject.Message.cs:801-810`）：
///   `SendDefMessage(ident, nParam1, 0, 0, 0, processMsg.Msg)` ⇒ 尾 = `EncodeString(请求的 Msg)`。
#[must_use]
pub fn sm_additem(actor_id: i32) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_ADDITEM, 0, 0, 1);
    gm_data_body(header, &[])
}

/// 见 [`sm_additem`] 段的口径表。
#[must_use]
pub fn sm_itemhide(item_id: i32, x: i32, y: i32) -> Vec<u8> {
    let header = command_header(item_id, messages::SM_ITEMHIDE, x as u16, y as u16, 0);
    gm_data_body(header, &[])
}

/// 见 [`sm_additem`] 段的口径表。按 oracle 逐字：`((W+A+H) ^ 0x3A5F) ^ 0x1F35 ^ 0xaa21`（`u16` 回写）。
#[must_use]
pub fn sm_weightchanged(weight: u16, wear_weight: u8, hand_weight: u8) -> Vec<u8> {
    let sum = i32::from(weight) + i32::from(wear_weight) + i32::from(hand_weight);
    let chk = ((sum ^ 0x3A5F) ^ 0x1F35) ^ 0xaa21;
    let header = command_header(
        i32::from(weight),
        messages::SM_WEIGHTCHANGED,
        u16::from(wear_weight),
        u16::from(hand_weight),
        chk as u16,
    );
    gm_data_body(header, &[])
}

/// `SM_HEALTHSPELLCHANGED`（`PlayObject.Message.cs:1736-1739` 的 `case RM_HEALTHSPELLCHANGED`）：
/// `MakeMessage(SM_HEALTHSPELLCHANGED, processMsg.ActorId, WAbil.HP, WAbil.MP, WAbil.MaxHP)` + `SendSocket`
/// ⇒ `Recog=ActorId`、`Param=HP`、`Tag=MP`、`Series=MaxHP`（无尾）。
///
/// 调用点：`BaseObject.IncHealthSpell`（`BaseObject.cs:704`）的 `HealthSpellChanged()` ⇒ `SendMsg(RM_HEALTHSPELLCHANGED)`
/// （`CM_EAT` 的 `StdMode 0` 回帧，`PlayObject.Base.cs:2047`）。
#[must_use]
pub fn sm_healthspellchanged(recog: i32, hp: u16, mp: u16, max_hp: u16) -> Vec<u8> {
    let header = command_header(recog, messages::SM_HEALTHSPELLCHANGED, hp, mp, max_hp);
    gm_data_body(header, &[])
}

/// `SM_WINEXP`（`PlayObject.Message.cs:1666-1669` 的 `case RM_WINEXP`）：
/// `MakeMessage(SM_WINEXP, Abil.Exp, LoWord(processMsg.nParam1), HiWord(processMsg.nParam1), 0)`
/// + `SendSocket`（无尾）。发点是 `PlayObject.Base.cs:153` 的 `GetExp`（`SendMsg(RM_WINEXP,0,dwExp,0,0)`）。
///
/// ⇒ `Recog = 加完后的 Abil.Exp`（**总量**，不是本次增量）、`Param = LoWord(dwExp)`、
/// `Tag = HiWord(dwExp)`、`Series = 0`。
#[must_use]
pub fn sm_winexp(total_exp: i32, delta: i32) -> Vec<u8> {
    let header = command_header(
        total_exp,
        messages::SM_WINEXP,
        lo_word(delta),
        hi_word(delta),
        0,
    );
    gm_data_body(header, &[])
}

/// `SM_LEVELUP`（`PlayObject.Message.cs:1670-1671` 的 `case RM_LEVELUP`）：
/// `MakeMessage(SM_LEVELUP, Abil.Exp, Abil.Level, 0, 0)` + `SendSocket`（无尾）。
/// 发点 = `PlayObject.cs:5247` 的 `HasLevelUp`（`SendMsg(RM_LEVELUP, 0, Abil.Exp, 0, 0)`）。
///
/// ⇒ `Recog = 加完后的 Abil.Exp`（**总量**）、`Param = Abil.Level`、`Tag/Series = 0`、无尾。
#[must_use]
pub fn sm_levelup(exp: i32, level: u8) -> Vec<u8> {
    let header = command_header(exp, messages::SM_LEVELUP, u16::from(level), 0, 0);
    gm_data_body(header, &[])
}

/// 见 [`sm_additem`] 段的口径表。
#[must_use]
pub fn sm_goldchanged(gold: i32, game_gold: i32) -> Vec<u8> {
    let header = command_header(
        gold,
        messages::SM_GOLDCHANGED,
        lo_word(game_gold),
        hi_word(game_gold),
        0,
    );
    gm_data_body(header, &[])
}

/// 见 [`sm_additem`] 段的口径表。`msg` 是 oracle `processMsg.Msg` 的原文（本接缝多数为空）。
#[must_use]
pub fn sm_menu_ok(recog: i32, msg: &[u8]) -> Vec<u8> {
    let header = command_header(recog, messages::SM_MENU_OK, 0, 0, 0);
    gm_data_body(header, &edcode::encode(msg))
}

/// 见 [`sm_additem`] 段的口径表。`msg` = 请求里带回的物品名（`CM_DROPITEM` 的尾，原样传入后 `EncodeString`）。
#[must_use]
pub fn sm_dropitem_reply(success: bool, make_index: i32, msg: &[u8]) -> Vec<u8> {
    let ident = if success {
        messages::SM_DROPITEM_SUCCESS
    } else {
        messages::SM_DROPITEM_FAIL
    };
    let header = command_header(make_index, ident, 0, 0, 0);
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_OUTOFCONNECTION`（`PlayObject.cs:1084-1086` 的 `RunNotice` 踢人支）：
/// `SendDefMessage(SM_OUTOFCONNECTION, 0, 0, 0, 0)` ⇒ 12B 命令头 + **无尾**（header-only）。
///
/// 场景（`openmir2-m2-session-lifecycle` 的"踢旧"）：同一角色重复进世界时
/// `WorldServer.KickOnlineUser`（`WorldServer.cs:1052-1064`）给**旧** PlayObject 置 `BoKickFlag`，
/// 旧对象下一 tick 在 `RunNotice`（`PlayObject.cs:1082-1090`）里先收到本帧、再 `MakeGhost()`。
#[must_use]
pub fn sm_outofconnection() -> Vec<u8> {
    let header = command_header(0, messages::SM_OUTOFCONNECTION, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_VERSION_FAIL`（`PlayObject.Message.cs:1690-1692`）：
/// `MakeMessage(SM_VERSION_FAIL, ClientFile1_CRC, LoWord(ClientFile2_CRC), HiWord(ClientFile2_CRC), 0)`
/// + 尾 `"<<<<<<"`（**原样字面量**，不是 EDCode 编码——金标准里捕获工具把它解码成了垃圾，但这 6 字节在线缆上就是原样）。
///
/// **R4-3**：第 1 参是 oracle 的 `ClientFile1_CRC`（落在 `Recog`），**不是** `ActorId`；
/// 接线时按 `Config.ClientFile1_CRC` 传，勿传 `ActorId`（金标准 #11 两值都 0，测试看不出）。
#[must_use]
pub fn sm_version_fail(client_file1_crc: i32, client_file2_crc: i32) -> Vec<u8> {
    let header = command_header(
        client_file1_crc,
        messages::SM_VERSION_FAIL,
        lo_word(client_file2_crc),
        hi_word(client_file2_crc),
        0,
    );
    gm_data_body(header, b"<<<<<<")
}

/// `SM_SENDNOTICE`（`PlayObject.cs:1064-1076` 的 `SendNotice`）：
/// `SendDefMessage(SM_SENDNOTICE, 2000, 0, 0, 0, sNoticeMsg)` ⇒ `Recog` **恒 2000**，
/// 尾 = `EncodeString(公告文本)`（文本由 `M2Share.NoticeMgr.GetNoticeMsg` 读出，逐行以
/// `\x20\x1B\x1B` 相连且**最后一行后也跟一个分隔符**；加载口径见 `notice.rs`）。
///
/// **R1-1**：`notice` 为空也**照发**——oracle 空 `sMsg` 走 `else SendSocket(ClientMsg)`，
/// 仍写 12B 命令头（即 header-only 658）；空尾时本函数等价 `gm_data_body(header, &[])`。
#[must_use]
pub fn sm_sendnotice(notice: &[u8]) -> Vec<u8> {
    let header = command_header(2000, messages::SM_SENDNOTICE, 0, 0, 0);
    gm_data_body(header, &edcode::encode(notice))
}

/// `SM_SYSMESSAGE`（`BaseObject.SysMsg`，`BaseObject.cs:1588-1685`）：
/// `SendMsg(RM_SYSMESSAGE, 0, fColor, bColor, 0, sMsg)` ⇒ `PlayObject.Message.cs:1715-1718`
/// 把它们翻成 `MakeMessage(SM_SYSMESSAGE, ActorId, MakeWord(fColor, bColor), 0, 1)`
/// + 尾 `EDCode.EncodeString(sMsg)`。
///
/// 参数口径：
/// - `recog` = **说话对象的 ActorId**（`processMsg.ActorId`；账号级顶号时 = 旧 PlayObject 的 ActorId）；
/// - `f_color`/`b_color` = oracle `SystemShare.Config.RedMsgFColor/RedMsgBColor`
///   （`BaseObject.cs:1678` 的 `MsgColor.Red` 支），落在 `Param` = `make_word(f, b)`；
/// - `series` 恒 1（`MakeMessage` 的末参，见 `PlayObject.Message.cs:1715`）。
///
/// 场景（`openmir2-m2-session-lifecycle`）：账号级顶号（`TCPNetChannel.KickUser`，`TCPNetChannel.cs:229/262`）。
#[must_use]
pub fn sm_sysmessage(recog: i32, f_color: u8, b_color: u8, msg: &[u8]) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_SYSMESSAGE,
        make_word(f_color, b_color),
        0,
        1,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_SERVERCONFIG`（`PlayObject.Base.cs:941-971` 的 `SendServerConfig`）：
/// `sMsg = EDCode.EncodeMessage(SystemShare.Config.ClientConf)`，再 `SendDefMessage(SM_SERVERCONFIG, 0, 0, 0, 0, sMsg)`
/// ⇒ 尾 = `EncodeString(EncodeMessage(ClientConf))`（**双重编码**；金标准 #6：线缆 31B ⇒ 单解 23B ⇒ 双解 17B）。
///
/// 入参是 `ClientConf` 的**序列化原文**（配置对象在 Rust 侧尚未落地，登记）。
#[must_use]
pub fn sm_serverconfig(client_conf: &[u8]) -> Vec<u8> {
    let header = command_header(0, messages::SM_SERVERCONFIG, 0, 0, 0);
    gm_data_body(header, &edcode::encode(&edcode::encode(client_conf)))
}

/// `SM_USERNAME`（`PlayObject.Operate.cs:18-34` 的 `ClientQueryUserName`）：
/// `MakeMessage(SM_USERNAME, targetId, nameColor, 0, 0)` + 尾 = `EncodeString(baseObject.GetShowName())`。
/// `SendSocket(defMsg, EncodeString(...))` 把已编码串**原样**追加 ⇒ 线缆尾就是 `EncodeString(name)`。
#[must_use]
pub fn sm_username(target_id: i32, name_color: u8, show_name: &[u8]) -> Vec<u8> {
    let header = command_header(
        target_id,
        messages::SM_USERNAME,
        u16::from(name_color),
        0,
        0,
    );
    gm_data_body(header, &edcode::encode(show_name))
}

/// `SM_AREASTATE`（`PlayObject.Base.cs:270-286` 的 `RefUserState`）：
/// `SendDefMessage(SM_AREASTATE, n8, 0, 0, 0)`，无尾。`n8` 是地图 Flag 位或
/// （`FightZone|1`、`SafeArea|2`、`InGuildWarArea|4`），落在 `Recog`。
#[must_use]
pub fn sm_areastate(n8: u16) -> Vec<u8> {
    let header = command_header(i32::from(n8), messages::SM_AREASTATE, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_MAPDESCRIPTION`（`PlayObject.Base.cs:887-895` 的 `SendMapDescription`）：
/// `SendDefMessage(SM_MAPDESCRIPTION, nMusicid, 0, 0, 0, Envir.MapDesc)` ⇒ 尾 = `EncodeString(MapDesc)`。
/// `nMusicid = Envir.Flag.Music ? Envir.Flag.MusicId : -1`（金标准 #9 的 `Recog == 0xFFFF_FFFF` = -1）。
#[must_use]
pub fn sm_mapdescription(n_musicid: i32, map_desc: &[u8]) -> Vec<u8> {
    let header = command_header(n_musicid, messages::SM_MAPDESCRIPTION, 0, 0, 0);
    gm_data_body(header, &edcode::encode(map_desc))
}

/// `SM_GAMEGOLDNAME`（`PlayObject.Base.cs:921-939` 的 `SendGoldInfo`）：
/// `SendDefMessage(SM_GAMEGOLDNAME, GameGold, LoWord(GamePoint), HiWord(GamePoint), 0, sMsg)`
/// ⇒ 尾 = `EncodeString(sMsg)`（`boSendName` 时 = `元宝名 + '\r' + 金币名`）。
#[must_use]
pub fn sm_gamegoldname(game_gold: i32, game_point: i32, msg: &[u8]) -> Vec<u8> {
    let header = command_header(
        game_gold,
        messages::SM_GAMEGOLDNAME,
        lo_word(game_point),
        hi_word(game_point),
        0,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_ABILITY`（`PlayObject.Message.cs:1673-1674` 的 `RM_LEVELUP` 支 / `:1732-1734` 的 `RM_ABILITY` 支）：
/// `MakeMessage(SM_ABILITY, Gold, MakeWord((byte)Job, 99), LoWord(GameGold), HiWord(GameGold))`
/// 尾 = `EncodeMessage(WAbil)`。`EncodeMessage` 返回的已是编码串，`SendSocket` 原样追加
/// ⇒ 线缆尾 = `Encode(WAbil 序列化原文)`（金标准 #12：线缆 54B ⇒ 解出 40B）。
#[must_use]
pub fn sm_ability(gold: i32, job: u8, game_gold: i32, wabil: &[u8]) -> Vec<u8> {
    let header = command_header(
        gold,
        messages::SM_ABILITY,
        make_word(job, 99),
        lo_word(game_gold),
        hi_word(game_gold),
    );
    gm_data_body(header, &edcode::encode(wabil))
}

/// `SM_SUBABILITY`（`PlayObject.Message.cs:1675` / `:1935`）：
/// `SendDefMessage(SM_SUBABILITY, MakeLong(MakeWord(AntiMagic, 0), 0),
///  MakeWord(HitPoint, SpeedPoint), MakeWord(AntiPoison, PoisonRecover), MakeWord(HealthRecover, SpellRecover))`，无尾。
///
/// 各字段在 oracle 里是 `ushort`，但 `MakeWord`/`MakeLong` 只取低字节 ⇒ 线上每项一条 `u8` 通道。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_subability(
    anti_magic: u8,
    hit_point: u8,
    speed_point: u8,
    anti_poison: u8,
    poison_recover: u8,
    health_recover: u8,
    spell_recover: u8,
) -> Vec<u8> {
    // `Recog = MakeLong(MakeWord(anti_magic, 0), 0)`（`MakeLong(low, high) = (high << 16) | low`）
    let header = command_header(
        i32::from(anti_magic),
        messages::SM_SUBABILITY,
        make_word(hit_point, speed_point),
        make_word(anti_poison, poison_recover),
        make_word(health_recover, spell_recover),
    );
    gm_data_body(header, &[])
}

/// `SM_ADJUST_BONUS`（`PlayObject.Base.cs:1193-1210` 的 `ADJUST_BONUS`/`RefBonusPoint`）：
/// `MakeMessage(SM_ADJUST_BONUS, BonusPoint, 0, 0, 0)` + 尾 = `sMsg = EncodeMessage(A) + '/' + EncodeMessage(B) + '/' + EncodeMessage(C)`
/// （`SendSocket(defMsg, sMsg)` **不再编码**）。A/B/C 按职业取
/// `Config.BonusAbilofXxx` / 玩家 `BonusAbil` / `Config.NakedAbilofXxx`。
/// 金标准 #14：线缆 83B = 3 段各 27B（解码各 20B）。
#[must_use]
pub fn sm_adjust_bonus(bonus_point: i32, a: &[u8], b: &[u8], c: &[u8]) -> Vec<u8> {
    let header = command_header(bonus_point, messages::SM_ADJUST_BONUS, 0, 0, 0);
    let mut tail = edcode::encode(a);
    tail.push(b'/');
    tail.extend_from_slice(&edcode::encode(b));
    tail.push(b'/');
    tail.extend_from_slice(&edcode::encode(c));
    gm_data_body(header, &tail)
}

/// `SM_DAYCHANGING`（`PlayObject.Message.cs:1741-1742` 的 `RM_DAYCHANGING`）：
/// `MakeMessage(SM_DAYCHANGING, 0, Bright, DayBright(), 0)`，无尾。
#[must_use]
pub fn sm_daychanging(bright: u16, day_bright: u16) -> Vec<u8> {
    let header = command_header(0, messages::SM_DAYCHANGING, bright, day_bright, 0);
    gm_data_body(header, &[])
}

/// `SM_TURN`（`PlayObject.Message.cs:1552` 的 `RM_TURN` 支）：
/// `MakeMessage(SM_TURN, ActorId, nParam1, nParam2, MakeWord((ushort)wParam, baseObject.Light))`
/// 尾 = `EncodePacket(CharDesc)`（`CharDesc` = `Feature(i32) + Status(i32)` = 8B）；
/// 若 `processMsg.Msg` 非空，再**直接相接** `EncodeString($"{Msg}/{nObjCount}")`。
///
/// ⚠️ 两段各自编码后直接拼，前段 8B 使编码状态未复位（`8 % 3 != 0`）⇒ 客户端整段解码会错位，
/// 这是 oracle 的既有形态（非本层引入）。金标准 #16：线缆 25B = `Encode(8B)`(11) + `Encode(10B)`(14)。
#[must_use]
pub fn sm_turn(
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    light: u8,
    char_desc: &[u8],
    msg: &[u8],
) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_TURN, x, y, make_word(dir, light));
    let mut tail = edcode::encode(char_desc);
    if !msg.is_empty() {
        tail.extend_from_slice(&edcode::encode(msg));
    }
    gm_data_body(header, &tail)
}

/// `SM_WALK`(11) / `SM_RUN`(13) / `SM_HORSERUN`(5010)（`PlayObject.Message.cs:1403-1432` 的
/// `RM_WALK`/`RM_RUN`/`RM_HORSERUN` 支）：
///
/// ```text
/// ClientMsg = MakeMessage(SM_*, processMsg.ActorId, nParam1 /*=x*/, nParam2 /*=y*/,
///                         MakeWord((ushort)processMsg.wParam /*=Dir*/, baseObject.Light))
/// SendSocket(ClientMsg, EDCode.EncodePacket(CharDesc))   // = Encode(8B Feature+Status)
/// ```
///
/// **与 `SM_TURN` 的差别**：没有紧随的 `SM_FEATURECHANGED`，尾也没有 `ShowName/Color`（只有
/// `Encode(CharDesc)`）；`x`/`y` 是**移动后**的 `CurrX/CurrY`（oracle `SendRefMsg(RM_*, Dir, CurrX,
/// CurrY, 0, "")` 在 `CurrX/CurrY` 更新之后）。
#[must_use]
pub fn sm_move(
    ident: u16,
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    light: u8,
    char_desc: &[u8],
) -> Vec<u8> {
    let header = command_header(actor_id, ident, x, y, make_word(dir, light));
    gm_data_body(header, &edcode::encode(char_desc))
}

/// `SM_WALK`(11)：见 [`sm_move`]。
#[must_use]
pub fn sm_walk(actor_id: i32, x: u16, y: u16, dir: u8, light: u8, char_desc: &[u8]) -> Vec<u8> {
    sm_move(messages::SM_WALK, actor_id, x, y, dir, light, char_desc)
}

/// `SM_RUN`(13)：见 [`sm_move`]。
#[must_use]
pub fn sm_run(actor_id: i32, x: u16, y: u16, dir: u8, light: u8, char_desc: &[u8]) -> Vec<u8> {
    sm_move(messages::SM_RUN, actor_id, x, y, dir, light, char_desc)
}

/// `SM_HORSERUN`(5010)：见 [`sm_move`]。
#[must_use]
pub fn sm_horserun(actor_id: i32, x: u16, y: u16, dir: u8, light: u8, char_desc: &[u8]) -> Vec<u8> {
    sm_move(messages::SM_HORSERUN, actor_id, x, y, dir, light, char_desc)
}

/// **`openmir2-m3-attack-magic-wiring`（攻击回帧）**：`RM_HIT` 族（`PlayObject.Message.cs:1433-1521`）
/// 的动作帧编码。
///
/// `SM_HIT`(14)/`SM_HEAVYHIT`(15)/`SM_BIGHIT`(16)/`SM_POWERHIT`(18)/`SM_LONGHIT`(19)/
/// `SM_WIDEHIT`(24)/`SM_CRSHIT`(25)/`SM_TWINHIT`(26)/`SM_FIREHIT`(8) 共用一条形状：
/// `MakeMessage(SM_*, processMsg.ActorId, nParam1 /*= 攻方 CurrX */, nParam2 /*= 攻方 CurrY */, wParam /*= Dir */)`，
/// **无尾**（`SendSocket(ClientMsg)`；`SM_HEAVYHIT` 的尾 = `processMsg.Msg`，而攻击路径的
/// `SendRefMsg(..., "")` 恒空 ⇒ 本函数不产尾）。
#[must_use]
pub fn sm_action_hit(ident: u16, actor_id: i32, x: u16, y: u16, dir: u8) -> Vec<u8> {
    let header = command_header(actor_id, ident, x, y, u16::from(dir));
    gm_data_body(header, &[])
}

/// `SM_STRUCK`(31)/`SM_STRUCK_MAG`：受击者**自己**收到的受击帧
/// （`PlayObject.Message.cs:1562-1628` 的 `RM_STRUCK` 支）。
///
/// 头 = `MakeMessage(SM_STRUCK, ActorId /*= 受击者*/, WAbil.HP, WAbil.MaxHP, wParam /*= 本次伤害*/)`；
/// 尾 = `EDCode.EncodePacket(MessageBodyWL{ Param1=GetFeature(), Param2=CharStatus, Tag1=nParam3 /*= 命中者*/, Tag2=(RM_STRUCK_MAG?1:0) })`。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_struck(
    actor_id: i32,
    hp: u16,
    max_hp: u16,
    damage: u16,
    feature: i32,
    status: i32,
    hitter_id: i32,
    mag: bool,
) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_STRUCK, hp, max_hp, damage);
    let mut wl = [0u8; 16];
    wl[0..4].copy_from_slice(&feature.to_le_bytes());
    wl[4..8].copy_from_slice(&status.to_le_bytes());
    wl[8..12].copy_from_slice(&hitter_id.to_le_bytes());
    wl[12..16].copy_from_slice(&i32::from(mag).to_le_bytes());
    gm_data_body(header, &edcode::encode(&wl))
}

/// `SM_NOWDEATH`(34)/`SM_DEATH`(32)：死亡广播
/// （`BaseObject.Base.cs:351` 的 `SendRefMsg(RM_DEATH, Dir, CurrX, CurrY, 1, "")` ⇒ 观察者 34；
/// `PlayObject.cs:3235` 的 `SendMsg(..., RM_DEATH, Dir, CurrX, CurrY, 0)` ⇒ 观察者 32）。
///
/// 头 = `MakeMessage(SM_*DEATH, ActorId /*= 死者*/, nParam1 /*= CurrX */, nParam2 /*= CurrY */, wParam /*= Dir */)`；
/// 尾 = `EDCode.EncodePacket(CharDesc)`（`Feature + Status`，8B）。
#[must_use]
pub fn sm_death(ident: u16, actor_id: i32, x: u16, y: u16, dir: u8, char_desc: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, ident, x, y, u16::from(dir));
    gm_data_body(header, &edcode::encode(char_desc))
}

/// `SM_ADJUST_BONUS`(811) 的三段能力模板 `(BonusPoint, A, B, C)`（各自 `EncodeMessage` 后用 `/` 拼）。
pub type AdjustBonusParts = (i32, Vec<u8>, Vec<u8>, Vec<u8>);

/// `SM_HEAR`(40)（`PlayObject.Message.cs:1705-1707` 的 `RM_HEAR` 支）：
/// `MakeMessage(SM_HEAR, processMsg.ActorId /*= 说话者 ActorId，`BaseObject.Message.cs:154`*/, `
/// `MakeWord((ushort)nParam1, (ushort)nParam2) /*= MakeWord(btHearMsgFColor, btHearMsgBColor)*/, 0, 1)`；
/// 尾 = `EncodeString(ChrName + ':' + sMsg)`。
#[must_use]
pub fn sm_hear(actor_id: i32, f_color: u8, b_color: u8, msg: &[u8]) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_HEAR,
        make_word(f_color, b_color),
        0,
        1,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_MERCHANTSAY`(643)（`PlayObject.Message.cs:1722-1724` 的 `RM_MERCHANTSAY` 支）：
/// `MakeMessage(SM_MERCHANTSAY, processMsg.ActorId, MakeWord((ushort)nParam1, (ushort)nParam2), 0, 1)`
/// + 尾 `EDCode.EncodeString(processMsg.Msg)`（`:1726` 的 `SendSocket(ClientMsg, EncodeString(...))`）。
///
/// `recog` = `processMsg.ActorId`，两条入队路径取值不同（`BaseObject.Message.cs`）：
/// - `SendPriorityMsg(..., High)`（`ScriptEngine.cs:993`）以 `this.ActorId` 入库 ⇒ **玩家** ActorId；
/// - `SendMsg(normNpc, ...)`（`ScriptEngine.cs:997`）以 `baseObject.ActorId` 入库 ⇒ **NPC** ActorId
///   （隐藏任务 NPC 不经地图，恒 0；见 `bins/game-svr/src/npc.rs`）。
///
/// `msg` = `normNpc.ChrName + '/' + sMsg` 的**原始字节**（GBK；本层不转 UTF-8）。
#[must_use]
pub fn sm_merchantsay(recog: i32, n_param1: u8, n_param2: u8, msg: &[u8]) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_MERCHANTSAY,
        make_word(n_param1, n_param2),
        0,
        1,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_TURN`(10) 的字段 `(ActorId, X, Y, Dir, Light, CharDesc 序列化原文, msg)`。
pub type TurnFrame = (i32, u16, u16, u8, u8, Vec<u8>, Vec<u8>);

/// 入图帧序的输入集合（`PlayObject.Message.cs:1680-1692` 的 `RM_LOGON` 支 + 紧随的
/// `UserLogon`/`RefMyStatus` 首轮）。每个字段 `Some` 时才产出对应帧；缺输入即**跳过并登记**，
/// 绝不猜尾字节。
#[derive(Debug, Clone, Default)]
pub struct EnterWorldInputs {
    /// 本会话的 `ActorId`（oracle `ActorMgr.GetNextIdentity()`，随机正数）。
    pub actor_id: i32,
    pub cur_x: u16,
    pub cur_y: u16,
    pub dir: u8,
    pub light: u8,
    pub day_bright: u16,
    /// 登录前公告（`NoticeMgr`，属配置层）。**R1-1**：空/缺公告 ⇒ 空字节，但仍发 header-only 658。
    pub notice: Vec<u8>,
    /// `SM_NEWMAP` 的地图文件名（玩家记录 `CurMap`）。
    pub map_file_name: Option<Vec<u8>>,
    /// `SM_LOGON` 的 `MessageBodyWL` = `(Param1=GetFeatureToLong, Param2=CharStatus, Tag1, Tag2)`。
    pub logon_body_wl: Option<(i32, i32, i32, i32)>,
    /// `SM_FEATURECHANGED` = `(nRecog=GetFeatureToLong, FeatureEx)`。
    pub feature: Option<(i32, u16)>,
    pub attack_mode: Option<u8>,
    /// `SM_SERVERCONFIG` 的 `ClientConf` 序列化原文。
    pub client_conf: Option<Vec<u8>>,
    /// `SM_USERNAME` = `(targetId, nameColor, showName)`。
    pub username: Option<(i32, u8, Vec<u8>)>,
    pub area_state: Option<u16>,
    /// `SM_MAPDESCRIPTION` = `(nMusicid, MapDesc)`。
    pub map_description: Option<(i32, Vec<u8>)>,
    /// `SM_GAMEGOLDNAME` = `(GameGold, GamePoint, msg)`。
    pub game_gold_name: Option<(i32, i32, Vec<u8>)>,
    /// `SM_VERSION_FAIL` = `(ClientFile1_CRC, ClientFile2_CRC)`。
    pub version_fail: Option<(i32, i32)>,
    /// `SM_ABILITY` = `(Gold, Job, GameGold, WAbil 序列化原文)`。
    pub ability: Option<(i32, u8, i32, Vec<u8>)>,
    /// `SM_SUBABILITY` = `(AntiMagic, HitPoint, SpeedPoint, AntiPoison, PoisonRecover, HealthRecover, SpellRecover)`。
    pub sub_ability: Option<[u8; 7]>,
    /// `SM_ADJUST_BONUS` = `(BonusPoint, A, B, C)`（三段能力模板）。
    pub adjust_bonus: Option<AdjustBonusParts>,
    /// `SM_DAYCHANGING` = `(Bright, DayBright)`。
    pub day_changing: Option<(u16, u16)>,
    /// `SM_TURN` = `(ActorId, X, Y, Dir, Light, CharDesc 序列化原文, msg)`。
    pub turn: Option<TurnFrame>,
}

impl EnterWorldInputs {
    /// 按 **M0 金标准的入图序**产出帧（缺输入的帧跳过，已产出的相对顺序不变）。
    ///
    /// 序：`658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46, 10`。
    #[must_use]
    pub fn frames(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        // **R1-1**：oracle `SendNotice` 无条件 `SendDefMessage(..., sNoticeMsg)`；即使
        // `sNoticeMsg` 为空也照发 header-only 658（`SendDefMessage` 的 `if (!string.IsNullOrEmpty)`
        // 只决定是否追加尾，空时走 `else SendSocket(ClientMsg)`）。故这里**不**用 `if let`。
        out.push(sm_sendnotice(&self.notice));
        if let Some(name) = &self.map_file_name {
            out.push(sm_newmap(
                self.actor_id,
                self.cur_x,
                self.cur_y,
                self.day_bright,
                name,
            ));
        }
        if let Some((p1, p2, t1, t2)) = self.logon_body_wl {
            let mut wl = [0u8; 16];
            wl[0..4].copy_from_slice(&p1.to_le_bytes());
            wl[4..8].copy_from_slice(&p2.to_le_bytes());
            wl[8..12].copy_from_slice(&t1.to_le_bytes());
            wl[12..16].copy_from_slice(&t2.to_le_bytes());
            out.push(sm_logon(
                self.actor_id,
                self.cur_x,
                self.cur_y,
                self.dir,
                self.light,
                wl,
            ));
        }
        if let Some((n, ex)) = self.feature {
            out.push(sm_featurechanged(self.actor_id, n, ex));
        }
        if let Some(mode) = self.attack_mode {
            out.push(sm_attackmode(mode));
        }
        if let Some(cfg) = &self.client_conf {
            out.push(sm_serverconfig(cfg));
        }
        if let Some((id, color, name)) = &self.username {
            out.push(sm_username(*id, *color, name));
        }
        if let Some(n8) = self.area_state {
            out.push(sm_areastate(n8));
        }
        if let Some((music, desc)) = &self.map_description {
            out.push(sm_mapdescription(*music, desc));
        }
        if let Some((gold, point, msg)) = &self.game_gold_name {
            out.push(sm_gamegoldname(*gold, *point, msg));
        }
        if let Some((crc1, crc2)) = self.version_fail {
            out.push(sm_version_fail(crc1, crc2));
        }
        if let Some((gold, job, game_gold, wabil)) = &self.ability {
            out.push(sm_ability(*gold, *job, *game_gold, wabil));
        }
        if let Some([m, hp, sp, ap, pr, hr, sr]) = self.sub_ability {
            out.push(sm_subability(m, hp, sp, ap, pr, hr, sr));
        }
        if let Some((bonus, a, b, c)) = &self.adjust_bonus {
            out.push(sm_adjust_bonus(*bonus, a, b, c));
        }
        if let Some((bright, day)) = self.day_changing {
            out.push(sm_daychanging(bright, day));
        }
        if let Some((id, x, y, d, light, desc, msg)) = &self.turn {
            out.push(sm_turn(*id, *x, *y, *d, *light, desc, msg));
        }
        out
    }
}

/// `HUtil32.LoWord`（低 16 位）。
#[must_use]
pub fn lo_word(v: i32) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

/// `HUtil32.HiWord`（高 16 位）。
#[must_use]
pub fn hi_word(v: i32) -> u16 {
    u16::try_from((v >> 16) & 0xFFFF).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// 金标准 `capture.jsonl`（7200/s2c）里这 5 条的**原始客户端线缆字节**（含 `#`/`!`）。
    /// 这里按原样钉进用例；`golden_constants_are_present_in_capture` 再回读文件核对，防止抄错/漂移。
    const G_NEWMAP: &str = "23486158617063685f6b65426f6664686b603f21";
    const G_LOGON: &str =
        "23486158617062685f6b65426f6664686b64646a6b6464686b6464686b6464686b6464686b643f21";
    const G_FEATURECHANGED: &str = "2348615861705d685d64646a6b6464686b21";
    const G_ATTACKMODE: &str = "236464686b6459686a6464686b6464686b21";
    const G_VERSION_FAIL: &str = "236464686b647a646b6464686b6464686b3c3c3c3c3c3c21";

    /// 该会话的 `ActorId`（金标准里 5 条帧的 `Recog` 都一样，属"会话相关随机 Id"，归一化时固定成这个值）。
    const G_ACTOR_ID: i32 = 0x7c30_3998;

    /// ① 一条端到端（编码层）：`SM_NEWMAP` 与金标准 #2 逐字节相等。
    #[test]
    fn sm_newmap_matches_golden_frame_2() {
        // 金标准：CurrX=287 / CurrY=618 / DayBright=0 / MapFileName="0"
        let body = sm_newmap(G_ACTOR_ID, 287, 618, 0, b"0");
        assert_eq!(body.len(), 12 + edcode::encoded_len(1));
        assert_eq!(client_frame(&body), hex(G_NEWMAP));
    }

    /// ①：`SM_LOGON` 与金标准 #3 逐字节相等（`MessageBodyWL` = 金标准解出的 16B）。
    #[test]
    fn sm_logon_matches_golden_frame_3() {
        let wl: [u8; 16] = hex("00000200000000000000000000000000").try_into().unwrap();
        let body = sm_logon(G_ACTOR_ID, 287, 618, 0, 0, wl);
        assert_eq!(body.len(), 12 + edcode::encoded_len(16));
        assert_eq!(client_frame(&body), hex(G_LOGON));
    }

    /// `SM_FEATURECHANGED` 与金标准 #4 逐字节相等（`nRecog = GetFeatureToLong() = 0x0002_0000`）。
    #[test]
    fn sm_featurechanged_matches_golden_frame_4() {
        let body = sm_featurechanged(G_ACTOR_ID, 0x0002_0000, 0);
        assert_eq!(body.len(), 12, "无尾 ⇒ PackLength 恰好 12");
        assert_eq!(client_frame(&body), hex(G_FEATURECHANGED));
    }

    /// `SM_ATTACKMODE` 与金标准 #5 逐字节相等（AttatckMode=0）。
    #[test]
    fn sm_attackmode_matches_golden_frame_5() {
        let body = sm_attackmode(0);
        assert_eq!(body.len(), 12);
        assert_eq!(client_frame(&body), hex(G_ATTACKMODE));
    }

    /// `SM_OUTOFCONNECTION`（528）是 header-only：`Recog=0`、12B、无尾。
    /// 字段序按 `MakeMessage(ident, recog, param, tag, series)` 钉死（`Recog` 在头里是小端 0）。
    #[test]
    fn sm_outofconnection_is_header_only_with_recog_zero() {
        let body = sm_outofconnection();
        assert_eq!(body.len(), 12, "无尾 ⇒ PackLength 恰好 12");
        assert_eq!(
            i32::from_le_bytes([body[0], body[1], body[2], body[3]]),
            0,
            "Recog"
        );
        assert_eq!(
            u16::from_le_bytes([body[4], body[5]]),
            messages::SM_OUTOFCONNECTION,
            "ident 必须是 SM_OUTOFCONNECTION(528)"
        );
        assert_eq!(&body[6..12], &[0u8; 6], "Param/Tag/Series 全 0");
        // 客户端线缆：`#` + Encode(头 12B) + 无尾 + `!`
        let wire = client_frame(&body);
        assert_eq!(wire.first(), Some(&b'#'));
        assert_eq!(wire.last(), Some(&b'!'));
        assert_eq!(wire.len(), 1 + edcode::encoded_len(12) + 1);
    }

    /// `SM_VERSION_FAIL` 与金标准 #11 逐字节相等：尾是原样 `"<<<<<<"`（6B），不是 EDCode 编码。
    #[test]
    fn sm_version_fail_matches_golden_frame_11() {
        let body = sm_version_fail(0, 0);
        assert_eq!(body.len(), 12 + 6);
        assert_eq!(client_frame(&body), hex(G_VERSION_FAIL));
        // 钉住"尾是原样"：客户端线缆里应出现 6 个 0x3c，而不是它们的 EDCode 编码。
        assert!(client_frame(&body).windows(6).any(|w| w == b"<<<<<<"));
    }

    /// 头 12B 的字节序锚点（改坏字段顺序必红）：`Recog` 小端在前 4B，接着 Ident/Param/Tag/Series。
    #[test]
    fn command_header_layout_is_little_endian() {
        let h = command_header(0x0102_0304, 0x0506, 0x0708, 0x090A, 0x0B0C);
        assert_eq!(h.as_slice(), hex("04030201060508070a090c0b").as_slice());
    }

    /// **M3⑥-实现 3 第②片**：`SM_MERCHANTSAY`(643) 的字段/尾编码锚点
    /// （`PlayObject.Message.cs:1722-1726`）：`Recog=processMsg.ActorId`、`Ident=643`、
    /// `Param=MakeWord(nParam1,nParam2)`、`Tag=0`、`Series=1`，尾 = `EncodeString(Msg)` 的 EDCode。
    ///
    /// 红检：把 `series` 从 1 改 0、或把尾从 `edcode::encode` 改成原样追加 ⇒ 本用例必红。
    #[test]
    fn sm_merchantsay_header_and_tail() {
        let msg = encoding_rs::GBK.encode("QManage/欢迎").0.into_owned();
        let body = sm_merchantsay(0x7c30_3998, 0x12, 0x34, &msg);
        assert_eq!(i32at(&body, 0), 0x7c30_3998, "Recog = processMsg.ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_MERCHANTSAY);
        assert_eq!(u16at12(&body, 6), make_word(0x12, 0x34), "Param");
        assert_eq!(u16at12(&body, 8), 0, "Tag");
        assert_eq!(u16at12(&body, 10), 1, "Series 恒 1");
        assert_eq!(&body[12..], edcode::encode(&msg), "尾 = EncodeString(Msg)");
        // 客户端线缆：`#` + Encode(头 12B) + 尾 + `!`
        let wire = client_frame(&body);
        assert_eq!(wire.first(), Some(&b'#'));
        assert_eq!(wire.last(), Some(&b'!'));
        let (head, wire_tail) = split_wire(&wire);
        assert_eq!(u16at(&head, 4), messages::SM_MERCHANTSAY);
        assert_eq!(edcode::decode(&wire_tail), msg, "尾解码后仍是 GBK 原文");
    }

    fn u16at12(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes([b[o], b[o + 1]])
    }

    /// 回读 M0 金标准文件，核对上面 5 个常量确实是其原始字节（防抄错、防漂移）。
    ///
    /// 文件路径从 `CARGO_MANIFEST_DIR`（`mir2-rs/bins/game-svr`）上溯到仓库根的 `tests/golden/...`。
    #[test]
    fn golden_constants_are_present_in_capture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let Ok(capture) = std::fs::read_to_string(path) else {
            panic!("金标准不存在：{path}（本测试要求它在仓库内，不许静默跳过）");
        };
        for (name, raw) in [
            ("SM_NEWMAP", G_NEWMAP),
            ("SM_LOGON", G_LOGON),
            ("SM_FEATURECHANGED", G_FEATURECHANGED),
            ("SM_ATTACKMODE", G_ATTACKMODE),
            ("SM_VERSION_FAIL", G_VERSION_FAIL),
        ] {
            assert!(
                capture.contains(&format!("\"raw_hex\":\"{raw}\"")),
                "{name} 的常量 {raw} 不在 M0 金标准里（抄错或金标准已漂移）"
            );
        }
    }

    // ---- S2b-4 续：其余 11 条 + 入图序判据② ----

    /// 从 M0 金标准取前 `n` 条 `port=7200 && dir=s2c` 帧：`(ident, 原始线缆字节)`。
    fn golden_s2c(n: usize) -> Vec<(u16, Vec<u8>)> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let capture =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("金标准不存在：{path}（{e}）"));
        let mut out = Vec::new();
        for line in capture.lines() {
            if !line.contains("\"dir\":\"s2c\"") || !line.contains("\"port\":7200") {
                continue;
            }
            let raw = between(line, "\"raw_hex\":\"", "\"");
            let ident: u16 = between(line, "\"ident\":", ",")
                .parse()
                .expect("ident 不是整数");
            out.push((ident, hex(raw)));
            if out.len() == n {
                break;
            }
        }
        assert_eq!(out.len(), n, "金标准里的 7200/s2c 不足 {n} 条");
        out
    }

    fn between<'a>(s: &'a str, a: &str, b: &str) -> &'a str {
        let i = s.find(a).expect("缺少左标记") + a.len();
        let j = s[i..].find(b).expect("缺少右标记") + i;
        &s[i..j]
    }

    /// 从 M0 金标准取**第一条** `port=7200 && dir=s2c && ident=<id>` 帧的原始线缆字节。
    fn golden_s2c_ident(ident: u16) -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let capture =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("金标准不存在：{path}（{e}）"));
        let needle = format!("\"ident\":{ident},");
        for line in capture.lines() {
            if !line.contains("\"dir\":\"s2c\"")
                || !line.contains("\"port\":7200")
                || !line.contains(&needle)
            {
                continue;
            }
            return hex(between(line, "\"raw_hex\":\"", "\""));
        }
        panic!("金标准里没有 7200/s2c ident={ident} 的帧");
    }

    /// **`openmir2-m2-session-lifecycle` ①**：`sm_sysmessage` 必须逐字节重建金标准里的
    /// `SM_SYSMESSAGE`（金标准 seq 66：`recog=0x7c303998`、`param=0xFFDB`、`series=1`、
    /// 尾 = GBK 的 `唯一客服QQ：…`）。
    ///
    /// 注：金标准那条是 `MsgColor.Green`（`param` 低字节 0xDB = `GreenMsgFColor`）；本函数的
    /// 账号级顶号用 `MsgColor.Red`（0xFF/0x38）——同一编码路径，字节布局由本用例钉住。
    ///
    /// 红检：把 `command_header` 的 `series` 改回 0，或把 `make_word(f, b)` 改成 `make_word(b, f)`
    /// ⇒ 本用例必红。
    #[test]
    fn sm_sysmessage_rebuilds_golden_frame_byte_for_byte() {
        let wire = golden_s2c_ident(messages::SM_SYSMESSAGE);
        let (head, wire_tail) = split_wire(&wire);
        assert_eq!(u16at(&head, 4), messages::SM_SYSMESSAGE);
        assert_eq!(u16at(&head, 10), 1, "series 恒 1（MakeMessage 末参）");
        let msg = edcode::decode(&wire_tail);
        assert_eq!(
            &*encoding_rs::GBK.decode(&msg).0,
            "唯一客服QQ：79311185 唯一担保QQ：79311185",
            "金标准尾文本"
        );
        // 从金标准头里取出 f/b（低字节/高字节），用它们重建：必须与金标准的 头+尾 逐字节相等。
        let param = u16at(&head, 6);
        let f = u8::try_from(param & 0xFF).unwrap();
        let b = u8::try_from(param >> 8).unwrap();
        let got = sm_sysmessage(recog(&head), f, b, &msg);
        let mut want = head.to_vec();
        want.extend_from_slice(&wire_tail);
        assert_eq!(got, want, "SM_SYSMESSAGE 必须逐字节重建金标准帧");
    }

    /// 拆一条客户端线缆帧：`'#' + Encode(头12B) + 尾 + '!'` ⇒ `(头解码态 12B, 线缆尾)`。
    fn split_wire(raw: &[u8]) -> ([u8; 12], Vec<u8>) {
        assert_eq!(raw[0], b'#', "帧首必须是 '#'");
        assert_eq!(raw[raw.len() - 1], b'!', "帧尾必须是 '!'");
        let head: [u8; 12] = edcode::decode(&raw[1..17])
            .try_into()
            .expect("头解码必须是 12B");
        (head, raw[17..raw.len() - 1].to_vec())
    }

    fn u16at(h: &[u8; 12], o: usize) -> u16 {
        u16::from_le_bytes([h[o], h[o + 1]])
    }

    fn recog(h: &[u8; 12]) -> i32 {
        i32::from_le_bytes([h[0], h[1], h[2], h[3]])
    }

    fn i32at(b: &[u8], o: usize) -> i32 {
        i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }

    /// 用**金标准的真字节**回填 `EnterWorldInputs`（尾输入直接取金标准的解码值；
    /// 不按源码公式重推——这是本仓的硬要求）。
    fn golden_inputs() -> EnterWorldInputs {
        let f = golden_s2c(16);
        let wire = |i: usize| split_wire(&f[i].1);
        let (_h0, t0) = wire(0); // 658
        let (h1, t1) = wire(1); // 51
        let (h2, t2) = wire(2); // 50
        let (h3, _) = wire(3); // 41
        let (h4, _) = wire(4); // 213
        let (_, t5) = wire(5); // 11029
        let (h6, t6) = wire(6); // 42
        let (h7, _) = wire(7); // 766
        let (h8, t8) = wire(8); // 54
        let (h9, t9) = wire(9); // 5008
        let (h10, _) = wire(10); // 1106
        let (h11, t11) = wire(11); // 52
        let (h12, _) = wire(12); // 752
        let (h13, t13) = wire(13); // 811
        let (h14, _) = wire(14); // 46
        let (h15, t15) = wire(15); // 10
        let parts: Vec<&[u8]> = t13.split(|b| *b == b'/').collect();
        assert_eq!(parts.len(), 3, "811 尾必须是三段用 '/' 拼接");
        let logon = edcode::decode(&t2);
        assert_eq!(logon.len(), 16, "SM_LOGON 尾 = MessageBodyWL 16B");
        EnterWorldInputs {
            actor_id: recog(&h1),
            cur_x: u16at(&h1, 6),
            cur_y: u16at(&h1, 8),
            day_bright: u16at(&h1, 10),
            dir: u8::try_from(u16at(&h2, 10) & 0xFF).unwrap(),
            light: u8::try_from(u16at(&h2, 10) >> 8).unwrap(),
            notice: edcode::decode(&t0),
            map_file_name: Some(edcode::decode(&t1)),
            logon_body_wl: Some((
                i32at(&logon, 0),
                i32at(&logon, 4),
                i32at(&logon, 8),
                i32at(&logon, 12),
            )),
            feature: Some((
                (i32::from(u16at(&h3, 8)) << 16) | i32::from(u16at(&h3, 6)),
                u16at(&h3, 10),
            )),
            attack_mode: Some(u8::try_from(recog(&h4)).unwrap()),
            client_conf: Some(edcode::decode(&edcode::decode(&t5))),
            username: Some((
                recog(&h6),
                u8::try_from(u16at(&h6, 6)).unwrap(),
                edcode::decode(&t6),
            )),
            area_state: Some(u16::try_from(recog(&h7)).unwrap()),
            map_description: Some((recog(&h8), edcode::decode(&t8))),
            game_gold_name: Some((
                recog(&h9),
                (i32::from(u16at(&h9, 8)) << 16) | i32::from(u16at(&h9, 6)),
                edcode::decode(&t9),
            )),
            version_fail: Some((
                recog(&h10),
                (i32::from(u16at(&h10, 8)) << 16) | i32::from(u16at(&h10, 6)),
            )),
            ability: Some((
                recog(&h11),
                u8::try_from(u16at(&h11, 6) & 0xFF).unwrap(),
                (i32::from(u16at(&h11, 10)) << 16) | i32::from(u16at(&h11, 8)),
                edcode::decode(&t11),
            )),
            sub_ability: Some([
                u8::try_from(recog(&h12)).unwrap(),
                u8::try_from(u16at(&h12, 6) & 0xFF).unwrap(),
                u8::try_from(u16at(&h12, 6) >> 8).unwrap(),
                u8::try_from(u16at(&h12, 8) & 0xFF).unwrap(),
                u8::try_from(u16at(&h12, 8) >> 8).unwrap(),
                u8::try_from(u16at(&h12, 10) & 0xFF).unwrap(),
                u8::try_from(u16at(&h12, 10) >> 8).unwrap(),
            ]),
            adjust_bonus: Some((
                recog(&h13),
                edcode::decode(parts[0]),
                edcode::decode(parts[1]),
                edcode::decode(parts[2]),
            )),
            day_changing: Some((u16at(&h14, 6), u16at(&h14, 8))),
            turn: Some((
                recog(&h15),
                u16at(&h15, 6),
                u16at(&h15, 8),
                u8::try_from(u16at(&h15, 10) & 0xFF).unwrap(),
                u8::try_from(u16at(&h15, 10) >> 8).unwrap(),
                edcode::decode(&t15[..11]),
                edcode::decode(&t15[11..]),
            )),
        }
    }

    /// 顺序判据②：入图序到 `SM_TURN` 与金标准**同序**，且每条**逐字节相等**（含长度）。
    #[test]
    fn enter_world_sequence_matches_golden_order_and_bytes() {
        let golden = golden_s2c(16);
        let built = golden_inputs().frames();
        let got_idents: Vec<u16> = built
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        let want_idents: Vec<u16> = golden.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            want_idents,
            vec![658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46, 10],
            "金标准前 16 条的 ident 序（本判据的期望值）"
        );
        assert_eq!(got_idents, want_idents, "Rust 侧产出的入图序必须同序");
        assert_eq!(built.len(), golden.len());
        for (i, (body, (ident, raw))) in built.iter().zip(&golden).enumerate() {
            assert_eq!(
                client_frame(body),
                *raw,
                "第 {} 条 ident={ident} 必须逐字节等于金标准（长度 {}）",
                i + 1,
                body.len()
            );
        }
    }

    /// **R1-1**：空/缺公告 ⇒ oracle **仍发一条 header-only 658**（`SendDefMessage` 的
    /// `if (!string.IsNullOrEmpty(sMsg))` 只决定是否追加尾；空走 `else SendSocket(ClientMsg)`，
    /// 照常写 12B 命令头）。这里把「空输入」钉死：帧仍在序列第 1 位、线缆恰为 12B 命令头 + 无尾。
    ///
    /// 红检：把 `frames()` 改回 `if !self.notice.is_empty()` 跳过 658 ⇒ 本用例必红。
    #[test]
    fn empty_notice_still_sends_header_only_658() {
        let inputs = EnterWorldInputs {
            notice: Vec::new(),
            ..Default::default()
        };
        let built = inputs.frames();
        assert_eq!(
            built.len(),
            1,
            "空公告下其余输入仍缺（不发 51/50 等），但 658 必须在序列第 1 位"
        );
        assert_eq!(built[0], sm_sendnotice(&[]), "空尾 658 == 纯 12B 命令头");
        let cm = mir2_protocol::frame::CommandMessage::from_bytes(&built[0]).unwrap();
        assert_eq!(cm.ident, messages::SM_SENDNOTICE);
        assert_eq!(cm.recog, 2000, "SendNotice 的 Recog 恒 2000");
        assert_eq!(built[0].len(), 12, "header-only：12B 命令头 + 无尾");
        // 线缆形态：`'#' + Encode(12B) + '!'`（无尾 ⇒ 头编码后直接收尾）。
        let wire = client_frame(&built[0]);
        assert_eq!(wire[0], b'#');
        assert_eq!(wire[wire.len() - 1], b'!');
        assert_eq!(&wire[1..wire.len() - 1], &edcode::encode(&built[0])[..]);
        assert_eq!(
            edcode::decode(&wire[1..wire.len() - 1]),
            built[0],
            "线缆解回头必须是同 12B"
        );
    }

    /// 新增 11 条的**逐条**定位断言（失败时能直接看出是哪条）。
    #[test]
    fn remaining_eleven_encoders_match_golden_frames() {
        let f = golden_s2c(16);
        let built = golden_inputs().frames();
        for (idx, ident) in [
            (0usize, 658u16),
            (5, 11029),
            (6, 42),
            (7, 766),
            (8, 54),
            (9, 5008),
            (11, 52),
            (12, 752),
            (13, 811),
            (14, 46),
            (15, 10),
        ] {
            assert_eq!(f[idx].0, ident, "金标准 #{idx} 的 ident");
            assert_eq!(
                client_frame(&built[idx]),
                f[idx].1,
                "ident={ident} 的编码必须逐字节等于金标准"
            );
        }
    }

    /// `SM_ATTACKMODE` 的模式在 **`Recog`**（oracle `Messages.cs:846-857` 的参序）；
    /// 金标准 #5 全 0 看不出，这里用非 0 值把字段位置钉死。
    #[test]
    fn attack_mode_goes_in_recog_not_param() {
        use mir2_protocol::frame::CommandMessage;
        let body = sm_attackmode(7);
        let cm = CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 7, "攻击模式必须在 Recog");
        assert_eq!(cm.ident, messages::SM_ATTACKMODE);
        assert_eq!(cm.param, 0);
        assert_eq!(cm.tag, 0);
        assert_eq!(cm.series, 0);
    }

    /// **R4-3**：`SM_VERSION_FAIL` 的第 1 参是 `ClientFile1_CRC`（在 `Recog`），第 2 参按 Lo/Hi 拆进
    /// `Param`/`Tag`；金标准 #11 全 0，这里用非 0 值钉死字段位。
    #[test]
    fn version_fail_field_layout() {
        use mir2_protocol::frame::CommandMessage;
        let body = sm_version_fail(0x1122_3344, 0x5566_7788);
        let cm = CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 0x1122_3344, "ClientFile1_CRC 必须在 Recog");
        assert_eq!(cm.param, 0x7788, "LoWord(ClientFile2_CRC)");
        assert_eq!(cm.tag, 0x5566, "HiWord(ClientFile2_CRC)");
        assert_eq!(cm.series, 0);
    }
}
