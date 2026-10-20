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

use crate::msgqueue::NetPriorityQueue;
use mir2_protocol::{edcode, messages};

/// `HUtil32.MakeWord(low, high)`（`src/OpenMir2/HUtil32.cs:61-64`）：`(high << 8) | low`。
#[must_use]
pub fn make_word(low: u8, high: u8) -> u16 {
    (u16::from(high) << 8) | u16::from(low)
}

/// `HUtil32.MakeWord(ushort low, ushort high)`（`HUtil32.cs:66-69`）：`(ushort)(low | (high << 8))`。
///
/// C# 重载解析：实参是 `ushort`/`int` 时选**这个**重载（`ushort -> byte` 无隐式转换），
/// 低位字节的第 8 位会串进高字节（如 `MakeWord(0x0190, 0x0002) == 0x0390`）。
#[must_use]
pub fn make_word16(low: u16, high: u16) -> u16 {
    low | (high << 8)
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
///   `tail` = `EDCode.EncodeBuffer(ClientItem)` 的**线上字节**（原样追加，不再 `EncodeString`）；
///   构造在 `trade::add_item_tail`（`GetUpgradeStdItem`/`GetItemName`/`MakeIndex·Dura·DuraMax`/
///   `StdMode==50` 的 ` #Dura` / `StdModeMap` 的 `Desc[8]`⇒Shape 0/130）。
///   G-1 起全部发射点（卸下/换装、商店购买/打造/取回、地面拾取）都带尾；T-24 已收敛（残留
///   `GetUpgradeStdItem` 的 desc 差量与自定义名未建模，desc=0 时逐字节等价）。
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
pub fn sm_additem(actor_id: i32, tail: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_ADDITEM, 0, 0, 1);
    gm_data_body(header, tail)
}

/// `PlayObject.SendDelItems(UserItem)`（`PlayObject.Base.cs:1648-1665`）：
/// `MakeMessage(SM_DELITEM(202), ActorId, 0, 0, 1)` + 尾 `EDCode.EncodeBuffer(clientItem)`。
///
/// 与 [`sm_additem`] 的差别（逐行对照）：**无** `M2Share.StdModeMap` 的 `Desc[8]`⇒`Shape`
/// 0/130 改写（`SendDelItems` 不动 `Item.Shape`）；`StdMode == 50` 的名称 `" #" + Dura`
/// 后缀、`Series=1`、字段序同源。**立即帧**（oracle 走 `SendSocket` 直发，不进 `MsgQueue`）。
#[must_use]
pub fn sm_delitem(actor_id: i32, tail: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_DELITEM, 0, 0, 1);
    gm_data_body(header, tail)
}

/// `PlayObject.SendUpdateItem/WithLevel/ByJob`（`PlayObject.Base.cs:1683/1704/1725`）：
/// `MakeMessage(SM_UPDATEITEM(203), ActorId, 0, 0, 1)` + 尾 `EDCode.EncodeBuffer(clientItem)`。
///
/// 与 [`sm_delitem`] 同形；差别只有 ident。oracle 发点：
/// - `Operate.cs:462-478`：穿上衣服（`StdMode 10/11`）后按 `Shape` 选
///   `SendUpdateItem`（其它）/`SendUpdateItemWithLevel`（`Shape==9`）/
///   `SendUpdateItemByJob`（`Shape==10/11` 龙袍/王袍）；
/// - `PlayObject.cs:4131`：武器升级成功（`CheckWeaponUpgrade`，本仓未建模 ⇒ 登记）。
#[must_use]
pub fn sm_updateitem(actor_id: i32, tail: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_UPDATEITEM, 0, 0, 1);
    gm_data_body(header, tail)
}

/// `PlayObject.SendUseItems()`（`PlayObject.Base.cs:1019-1050`）：
/// `MakeMessage(SM_SENDUSEITEMS(621), 0, 0, 0, 0)` + 尾 `i + '/' + EncodeBuffer(ClientItem) + '/'`（逐槽）。
///
/// oracle 有 `if (!string.IsNullOrEmpty(sSendMsg))` 门 ⇒ 空尾时**一帧都不发**（调用点判空）。
#[must_use]
pub fn sm_senduseitems(tail: &[u8]) -> Vec<u8> {
    let header = command_header(0, messages::SM_SENDUSEITEMS, 0, 0, 0);
    gm_data_body(header, tail)
}

/// `PlayObject.SendUseMagic()`（`PlayObject.Base.cs:1052-1073`）：
/// `MakeMessage(SM_SENDMYMAGIC(211), 0, 0, 0, (short)MagicList.Count)` +
/// 尾 `EncodeBuffer(ClientMagic) + '/'`（逐条）。空列表 ⇒ 一帧都不发。
#[must_use]
pub fn sm_sendmymagic(count: u16, tail: &[u8]) -> Vec<u8> {
    let header = command_header(0, messages::SM_SENDMYMAGIC, 0, 0, count);
    gm_data_body(header, tail)
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

/// 冻结客户端布局开关（`MIR2_CLIENT_ABILITY_LAYOUT`）：`0|false|FALSE|False` ⇒ oracle 逐字节；
/// 未设/其它（含 `1`/`true`）⇒ **冻结客户端布局**（52 的 58B `TAbility` + 53 的 `TCharDesc` 尾
/// + 31 的 `SM_STRUCK` `param/tag` 拆 HP、尾 `Param1`=MaxHP）。
///
/// 这是全仓唯一真源：`bins/game-svr/src/main.rs::mir2_client_ability_layout()` 只是本函数的别名。
/// 真机约定（`openmir2-client-e2e-enter-gaps`，证据 `61-*`）：客户端 `MirClient@c61d912d`
/// 在 `Source/Common/Grobal2.pas:39` 写死 `HIGHHP = True` ⇒ 52/53/31 都必须按客户端记录布局发，
/// 否则客户端把能力/血量读成 0（HUD `0/0`、`CanRun(HP>=10)` 拒绝跑；受击后血条错乱）。
#[must_use]
pub fn client_layout_enabled() -> bool {
    !matches!(
        std::env::var("MIR2_CLIENT_ABILITY_LAYOUT").as_deref(),
        Ok("0") | Ok("false") | Ok("FALSE") | Ok("False")
    )
}

/// 帧 52/53 记录布局的**起栈自证行**（卡 `openmir2-client-e2e-enter-gaps` 片 ㊷）：
/// `CLIENT_ABILITY_LAYOUT layout=oracle-40|client-58`。
///
/// `main()` 启动时打一行，供起栈器
/// （`tools/m2-acceptance/start_rust_stack.ps1 -ClientAbilityLayout`）的
/// `RUST_CLIENT_ABILITY_LAYOUT_INJECTED=<0|1>` 行**对得上**（同 `SEED_PROBE` 口径）：
/// `0` ⇒ 本行必须 `layout=oracle-40`；`1` ⇒ `layout=client-58`。只读日志，不改任何出帧语义。
#[must_use]
pub fn client_layout_probe_line() -> &'static str {
    if client_layout_enabled() {
        "CLIENT_ABILITY_LAYOUT layout=client-58"
    } else {
        "CLIENT_ABILITY_LAYOUT layout=oracle-40"
    }
}

/// `SM_HEALTHSPELLCHANGED` 的**冻结客户端布局**（`HIGHHP=True`），与 oracle 的 header-only 帧
/// 只差**尾**：`EDCode.Encode(TCharDesc)`（12B）。
///
/// # 为什么必须有尾（真机根因）
///
/// 冻结客户端 `Source/MirClient/ClMain.pas:10220` 的 `SM_HEALTHSPELLCHANGED` 支在
/// `{$IF HIGHHP}`（`Grobal2.pas:39 = True`）下**不读 12B 命令头的 `param/tag/series`**，
/// 而是从**尾**解码 `SizeOf(TCharDesc)`：
///
/// ```pascal
/// DecodeBuffer(body, @desc, SizeOf(TCharDesc));
/// Actor.m_Abil.HP    := DWORD(desc.Feature);
/// Actor.m_Abil.MP    := DWORD(desc.Status);
/// Actor.m_Abil.MaxHP := DWORD(desc.StatusEx);
/// ```
///
/// `TCharDesc = packed record Feature: Integer; Status: Integer; StatusEx: Integer; end;`
/// （`Grobal2.pas:3515`）⇒ 尾 = `[HP:i32][MP:i32][MaxHP:i32]`（12B）。
///
/// oracle 的 53 是 `SendSocket(ClientMsg)` = **header-only**（`PlayObject.Socket.cs:77-96`，
/// `PackLength = CommandMessage.Size`）⇒ 客户端解出 `desc = 0` ⇒ `HP=0 / MP=0 / MaxHP=0`，
/// HUD `0/0`、跑不动（真机 verify2 实测：进图 `18/19` 之后一旦恢复/体力段发 53 即归零）。
/// **oracle 与冻结客户端不可同时满足** ⇒ 与 52 的 58B 同款 §6.0-3 故意偏离：默认以客户端为准。
///
/// 调用方：**生产路径一律用** [`sm_healthspellchanged_frame`]（本函数只在 `=0` 对拍夹具里显式选）。
#[must_use]
pub fn sm_healthspellchanged_client(recog: i32, hp: u16, mp: u16, max_hp: u16) -> Vec<u8> {
    let header = command_header(recog, messages::SM_HEALTHSPELLCHANGED, hp, mp, max_hp);
    // `TCharDesc`（`Grobal2.pas:3515`）：Feature=HP、Status=MP、StatusEx=MaxHP，各 `Integer`(i32)。
    let mut desc = [0u8; 12];
    desc[0..4].copy_from_slice(&i32::from(hp).to_le_bytes());
    desc[4..8].copy_from_slice(&i32::from(mp).to_le_bytes());
    desc[8..12].copy_from_slice(&i32::from(max_hp).to_le_bytes());
    gm_data_body(header, &edcode::encode(&desc))
}

/// 53 帧的**生产选路**（[`client_layout_enabled`]）：客户端布局（默认）⇒ [`sm_healthspellchanged_client`]；
/// `MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle header-only [`sm_healthspellchanged`]（逐字节对拍/金标准夹具）。
///
/// 与 52 的选路同源同开关（`bins/game-svr/src/enter.rs::AbilityInputs::client_ability_layout`）。
#[must_use]
pub fn sm_healthspellchanged_frame(recog: i32, hp: u16, mp: u16, max_hp: u16) -> Vec<u8> {
    if client_layout_enabled() {
        sm_healthspellchanged_client(recog, hp, mp, max_hp)
    } else {
        sm_healthspellchanged(recog, hp, mp, max_hp)
    }
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

/// **Lane B**：`RecalcAbilitys()` 尾段（`PlayObject.cs:2764-2779`）的直接帧事件。
///
/// 顺序即 oracle 调用序：先火球分支（`FlameRing ? AddItemSkill(1) : DelItemSkill(1)`），
/// 再治愈分支（`RecoveryRing ? AddItemSkill(2) : DelItemSkill(2)`）。
#[derive(Debug, Clone, PartialEq)]
pub enum MagicEvent {
    /// `SendDelMagic(userMagic)`（`PlayObject.Base.cs:2218-2222`）。
    Del { mag_idx: u16 },
    /// `SendAddMagic(userMagic)`（`PlayObject.Base.cs:2205-2216`）——`def` 即 `UserMagic.Magic`。
    Add { def: mir2_data::MagicInfo },
}

/// `PlayerObject.SendDelMagic`（`PlayObject.Base.cs:2218-2222`）：
/// `MakeMessage(SM_DELMAGIC, userMagic.MagIdx, 0, 0, 1)` + `SendSocket(ClientMsg)`（**无尾**）。
#[must_use]
pub fn sm_delmagic(mag_idx: u16) -> Vec<u8> {
    let header = command_header(i32::from(mag_idx), messages::SM_DELMAGIC, 0, 0, 1);
    gm_data_body(header, &[])
}

/// `PlayObject.SendAddMagic`（`PlayObject.Base.cs:2205-2216`）：
/// `MakeMessage(SM_ADDMAGIC, 0, 0, 0, 1)` + `EDCode.EncodeBuffer(ClientMagic)`。
///
/// `ClientMagic`（`src/OpenMir2/Packets/ClientPackets/ClientMagic.cs`）由 `AddItemSkill`
/// 新建的 `UserMagic` 填：`Key = 0`、`Level = 1`、`CurTrain = 0`、`Def = magic`
/// （`PlayObject.cs:3919-3926`）。
#[must_use]
pub fn sm_addmagic(def: &mir2_data::MagicInfo) -> Vec<u8> {
    let header = command_header(0, messages::SM_ADDMAGIC, 0, 0, 1);
    let tail = edcode::encode(&crate::itemwire::encode_client_magic(0, 1, 0, def));
    gm_data_body(header, &tail)
}

/// 把一个 [`MagicEvent`] 编成完整 `GM_DATA` body（212 无尾 / 210 带 `ClientMagic` 尾）。
#[must_use]
pub fn magic_event_frame(event: &MagicEvent) -> Vec<u8> {
    match event {
        MagicEvent::Del { mag_idx } => sm_delmagic(*mag_idx),
        MagicEvent::Add { def } => sm_addmagic(def),
    }
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
pub fn sm_sysmessage(recog: i32, f_color: u16, b_color: u16, msg: &[u8]) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_SYSMESSAGE,
        make_word16(f_color, b_color),
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

/// `SM_GHOST`(803)（`PlayObject.Operate.cs:32-33` 的 else 支）：
/// `SendDefMessage(SM_GHOST, baseObject.ActorId, x, y, 0)` → `MakeMessage(803, targetId, x, y, 0)` + **无尾**。
///
/// 语义：查询的目标**不在请求的 3×3 窗口内**时，服务端回「幽灵」提示（x/y 是**请求参数**原样回带）。
#[must_use]
pub fn sm_ghost(actor_id: i32, x: u16, y: u16) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_GHOST, x, y, 0);
    gm_data_body(header, &[])
}

/// `SM_SENDUSERSTATE`(751)（`PlayObject.Operate.cs:110-111` 的 `ClientQueryUserInformation`）：
/// `ClientMsg = MakeMessage(SM_SENDUSERSTATE, 0, 0, 0, 0)` + 尾 = `EDCode.EncodeBuffer(userState)`
/// ⇒ 线缆尾就是 `EncryptUtil.Encode(UserStateInfo.GetBuffer())`。`tail` 传**已编码**缓冲。
#[must_use]
pub fn sm_senduserstate(tail: &[u8]) -> Vec<u8> {
    let header = command_header(0, messages::SM_SENDUSERSTATE, 0, 0, 0);
    gm_data_body(header, tail)
}

/// `SM_CHANGENAMECOLOR`（`PlayObject.Message.cs:1677-1679` 的 `RM_CHANGENAMECOLOR` 观察者支）：
/// `SendDefMessage(SM_CHANGENAMECOLOR, processMsg.ActorId, GetChrColor(baseObject), 0, 0)`
/// ⇒ `MakeMessage(656, ActorId, color, 0, 0)` + **无尾**（`SendDefMessage` 的 5 参重载）。
///
/// 产者 = `RefNameColor()`（`BaseObject.cs:730-733`）经 `SendRefMsg(RM_CHANGENAMECOLOR,…)` 广播；
/// ⑨-3 的宝宝叛变（`PlayObject.Base.cs:351`）就是这条。
#[must_use]
pub fn sm_changenamecolor(actor_id: i32, name_color: u8) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_CHANGENAMECOLOR,
        u16::from(name_color),
        0,
        0,
    );
    gm_data_body(header, &[])
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
    anti_magic: u16,
    hit_point: u8,
    speed_point: u8,
    anti_poison: u8,
    poison_recover: u8,
    health_recover: u8,
    spell_recover: u8,
) -> Vec<u8> {
    // `Recog = MakeLong(MakeWord(anti_magic, 0), 0)`（`MakeLong(low, high) = (high << 16) | low`）。
    // `AntiMagic` 是 `ushort` ⇒ C# 重载解析选 `MakeWord(ushort, ushort)`，**完整 16 位**保留
    // （`Recog` 是 i32 不截断）；旧实现的 `lo_byte` 会丢高字节（G-6）。
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
/// **尾**（`SendSocket(ClientMsg, processMsg.Msg)`）：普通攻击的 `SendRefMsg(..., "")` 恒空；
/// `SM_HEAVYHIT` 由挖矿（`PlayObject.Base.cs:1586` 的 `PileStones`）命中时 `s1C = "1"` ⇒ 尾 `b"1"`。
/// 本仓世界侧尚未建模 `PileStones`（登记为不可达）；编码器/`tail` 参数先按 oracle 留口。
#[must_use]
pub fn sm_action_hit(ident: u16, actor_id: i32, x: u16, y: u16, dir: u8, tail: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, ident, x, y, u16::from(dir));
    gm_data_body(header, tail)
}

/// **`openmir2-m3-pet-slave-summon`（第②片）**：`RM_SPELL` → `SM_SPELL(17)`
/// （`PlayObject.Message.cs:1454-1459`）。
///
/// oracle `DoSpell` 第 159 行 `SendRefMsg(RM_SPELL, Magic.Effect, nTargetX, nTargetY, Magic.MagicId, "")`
/// 广播给 ±`SendRefMsgRange` 内、非自己的玩家（`BaseObject.Message.cs:383-445`）；每个收件人：
///
/// ```text
/// ClientMsg = MakeMessage(SM_SPELL, processMsg.ActorId, nParam1 /*=x*/, nParam2 /*=y*/, wParam /*=Effect*/)
/// SendSocket(ClientMsg, processMsg.nParam3.ToString())   // 尾 = MagicId 的 ASCII 原文（不再 Encode）
/// ```
#[must_use]
pub fn sm_spell(actor_id: i32, x: u16, y: u16, effect: u8, magic_id: u16) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_SPELL, x, y, u16::from(effect));
    gm_data_body(header, magic_id.to_string().as_bytes())
}

/// **`openmir2-m3-pet-slave-summon`（第②片）**：`RM_MAGICFIRE` → `SM_MAGICFIRE(638)`
/// （`PlayObject.Message.cs:1788-1793`）。
///
/// oracle `DoSpell` 共享尾 `SendRefMsg(RM_MAGICFIRE, 0, MakeWord(EffectType, Effect), MakeLong(x,y), targetActorId, "")`
/// （`MagicManager.cs:648-658`）。收件人：
///
/// ```text
/// ClientMsg = MakeMessage(SM_MAGICFIRE, ActorId, LoWord(nParam2) /*=x*/, HiWord(nParam2) /*=y*/, nParam1 /*=MakeWord(EffectType,Effect)*/)
/// sSendStr = EDCode.EncodeBuffer(BitConverter.GetBytes(nParam3 /*= targetActorId */), 4)
/// SendSocket(ClientMsg, sSendStr)
/// ```
#[must_use]
pub fn sm_magicfire(
    actor_id: i32,
    x: u16,
    y: u16,
    effect_type: u8,
    effect: u8,
    target_actor_id: i32,
) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_MAGICFIRE,
        x,
        y,
        make_word(effect_type, effect),
    );
    gm_data_body(header, &edcode::encode(&target_actor_id.to_le_bytes()))
}

/// **`openmir2-m3-pet-slave-summon`（第②片）**：`RM_MAGIC_LVEXP` → `SM_MAGIC_LVEXP(640)`
/// （`PlayObject.Message.cs:1800-1802`）。
///
/// oracle `DoSpell` 尾 `SendSelfDelayMsg(RM_MAGIC_LVEXP, 0, Magic.MagicId, Level, TranPoint, "", 1000)`
/// （`MagicManager.cs:659-669`）→ 自有队列 → 收件人：
/// `SendDefMessage(SM_MAGIC_LVEXP, nParam1 /*=MagicId*/, nParam2 /*=Level*/, LoWord(nParam3), HiWord(nParam3))`
/// （`nParam3 = TranPoint`）。客户端（BotSrv `RobotPlayer.Operate.cs:1039`）回读
/// `ClientGetMagicLvExp(Recog, Param, MakeLong(Tag, Series))` ⇒ `(MagicId, Level, TranPoint)`。
#[must_use]
pub fn sm_magic_lvexp(magic_id: u16, level: u8, tran_point: i32) -> Vec<u8> {
    let header = command_header(
        i32::from(magic_id),
        messages::SM_MAGIC_LVEXP,
        u16::from(level),
        crate::feature::lo_word(tran_point),
        crate::feature::hi_word(tran_point),
    );
    gm_data_body(header, &[])
}

/// **`openmir2-m3-pet-slave-summon`（第②片）**：`RM_DURACHANGE` → `SM_DURACHANGE(642)`
/// （`PlayObject.Message.cs:1803-1805`）。
///
/// oracle `MagicBase.UseAmulet`（`MagicBase.cs:118-121`）
/// `SendMsg(this, RM_DURACHANGE, idx, Dura, DuraMax, 0)` → 自有队列 → 收件人：
/// `SendDefMessage(SM_DURACHANGE, nParam1 /*=Dura*/, wParam /*=idx*/, LoWord(nParam2), HiWord(nParam2))`
/// （`nParam2 = DuraMax`）。BotSrv（`RobotPlayer.Operate.cs:1042`）回读
/// `ClientGetDuraChange(Param /*=idx*/, Recog /*=Dura*/, MakeLong(Tag, Series) /*=DuraMax*/)`。
#[must_use]
pub fn sm_durachange(dura: u16, slot: u8, dura_max: u16) -> Vec<u8> {
    let header = command_header(
        i32::from(dura),
        messages::SM_DURACHANGE,
        u16::from(slot),
        dura_max,
        0,
    );
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

/// `SM_STRUCK`(31) 的**冻结客户端 `HIGHHP` 布局**（生产默认；`HIGHHP` 家族第三帧）。
///
/// 冻结客户端 `Source/MirClient/ClMain.pas:10238` 的 `SM_STRUCK` 支在 `{$IF HIGHHP}`
/// （`Source/Common/Grobal2.pas:39 = True`，`MirClient@c61d912d`）下按**另一种布局**读 HP/MaxHP：
///
/// ```pascal
/// DecodeBuffer(body, @wl, SizeOf(TMessageBodyWL));
/// ...
/// {$IF HIGHHP}
///   Actor.m_Abil.HP    := DWORD(Makelong(msg.param, msg.tag));  // param=LoWord(HP), tag=HiWord(HP)
///   Actor.m_Abil.MaxHP := DWORD(wl.lParam1);                    // MaxHP 在**尾** Param1
/// {$ELSE}
///   Actor.m_Abil.HP    := msg.param;
///   Actor.m_Abil.MaxHP := msg.tag;
/// {$IFEND}
/// ```
///
/// ⇒ 客户端布局 = 头 `(Recog=受击者, param=LoWord(HP), tag=HiWord(HP), series=伤害)`
/// + 尾 `Encode(MessageBodyWL{ Param1=MaxHP, Param2=CharStatus, Tag1=命中者, Tag2=mag })`。
///
/// 与 oracle [`sm_struck`]（`PlayObject.Message.cs:1601`：`param=HP, tag=MaxHP, 尾.Param1=feature`）
/// 只差 HP 的拆分位置与尾 `Param1` 的取值。`feature` 在 `HIGHHP` 下**不被客户端消费**：
/// `ClMain.pas:10266` 把 `wl.lParam1` 当 `nFeature` 传给 `TActor.UpdateMsg`，而
/// `TActor.SendMsg` 的 `SM_STRUCK` 支（`Actor.pas:1833-1839`）只取 `nSound`（→ `m_nHiterCode`），
/// `nFeature` 被 `Actor.m_Abil.MaxHP := wl.lParam1` 顶掉 ⇒ 客户端布局不再携带 feature。
///
/// **缺口后果**（`213-highhp-family-audit.txt` §4）：oracle 布局下客户端算出
/// `HP = HP | (MaxHP << 16)`、`MaxHP = feature` ⇒ 受击后 HUD 血条错乱，且该错值一直留到
/// 下一条 52/53/升级/换装覆盖（`RM_STRUCK` 支不回 53）。
///
/// 调用方：**生产路径一律用** [`sm_struck_frame`]（本函数只在 `=0` 对拍夹具里显式选）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_struck_client(
    actor_id: i32,
    hp: u16,
    max_hp: u16,
    damage: u16,
    status: i32,
    hitter_id: i32,
    mag: bool,
) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_STRUCK,
        lo_word(i32::from(hp)),
        hi_word(i32::from(hp)),
        damage,
    );
    // `MessageBodyWL`（`ClMain.pas:10240` 解 16B）：`Param1=MaxHP`（HIGHHP 支读这个）、
    // `Param2=CharStatus`、`Tag1=命中者 ActorId`、`Tag2=(RM_STRUCK_MAG ? 1 : 0)`。
    let mut wl = [0u8; 16];
    wl[0..4].copy_from_slice(&i32::from(max_hp).to_le_bytes());
    wl[4..8].copy_from_slice(&status.to_le_bytes());
    wl[8..12].copy_from_slice(&hitter_id.to_le_bytes());
    wl[12..16].copy_from_slice(&i32::from(mag).to_le_bytes());
    gm_data_body(header, &edcode::encode(&wl))
}

/// 31 帧的**生产选路**（[`client_layout_enabled`]）：客户端布局（默认）⇒ [`sm_struck_client`]；
/// `MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle 逐字节 [`sm_struck`]（跨侧对拍/金标准夹具）。
///
/// 与 52/53 的选路同源同开关。**跨侧判据约定**：任何按 oracle 字段位解 `SM_STRUCK`
/// （`param`=HP、`tag`=MaxHP、尾 `Param1`=feature）的夹具（如 m3 ②③ 的 `attack_diff.py`）
/// 必须显式 pin `MIR2_CLIENT_ABILITY_LAYOUT=0`（同 52/53 的既有约定，见该卡 README §结论 #6）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_struck_frame(
    actor_id: i32,
    hp: u16,
    max_hp: u16,
    damage: u16,
    feature: i32,
    status: i32,
    hitter_id: i32,
    mag: bool,
) -> Vec<u8> {
    if client_layout_enabled() {
        sm_struck_client(actor_id, hp, max_hp, damage, status, hitter_id, mag)
    } else {
        sm_struck(
            actor_id, hp, max_hp, damage, feature, status, hitter_id, mag,
        )
    }
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

/// `SM_MOVEFAIL`(28) 的 **oracle 逐字节布局**（`PlayObject.Message.cs:1468-1474` 的
/// `RM_MOVEFAIL` 支）：
///
/// ```text
/// ClientMsg = MakeMessage(SM_MOVEFAIL, ActorId, CurrX, CurrY, Dir);
/// CharDesc movefailmessage = default;
/// movefailmessage.Feature = baseObject.GetFeatureToLong();
/// movefailmessage.Status  = baseObject.CharStatus;
/// SendSocket(ClientMsg, EDCode.EncodePacket(movefailmessage));
/// ```
///
/// 头 = `(Recog=ActorId, Param=CurrX, Tag=CurrY, Series=Dir)`；
/// 尾 = `Encode(8B CharDesc{Feature:i32, Status:i32})`
/// （`src/OpenMir2/Packets/ClientPackets/CharDesc.cs`：`public int Feature; public int Status;` ⇒ 8B，
/// 与同文件的 `sm_death` 尾同结构）。
///
/// # 字段归属（易错点·观察者集合）
///
/// `Operate`（`PlayObject.Message.cs:768-776`）先把 `baseObject = SystemShare.ActorMgr.Get(processMsg.ActorId)`
/// —— 即 **发送者**（`SendRefMsg` 经 `SendMsg(this, …)` 把 `SendMessage.ActorId` 置成发送者，
/// `BaseObject.Message.cs:98-158/383-476`）。于是本 `case` 里：
///
/// - **头** 的 `ActorId`/`CurrX`/`CurrY`/`Dir` 是 **收件人自己**（`this`）——同一 `RM_MOVEFAIL`
///   发给每个观察者，各自产出的帧头都不同（都是「自己的 ActorId + 自己的坐标/朝向」）；
/// - **尾** 的 `Feature`/`Status` 取自 **`baseObject` = 发送者**（`GetFeatureToLong()` / `CharStatus`），
///   与帧头 **不同源**。
///
/// 生产选路见 [`sm_movefail_frame`]（**默认走本函数**）；本函数也是跨侧逐字节对拍口径。
///
/// 发射点：`ClientChangeDir`/`ClientWalkXY`/`ClientRunXY`/`ClientHorseRunXY` 的失败/超速支
/// （`PlayObject.Message.cs:850/1018/1038/1069/1089/1099/1128/1148/1176/1196/1202…`，
/// 均为 `SendRefMsg(RM_MOVEFAIL, 0, 0, 0, 0, "")`）。**本层未接线**（登记项，见 rationale）。
#[must_use]
pub fn sm_movefail(actor_id: i32, x: u16, y: u16, dir: u8, feature: i32, status: i32) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_MOVEFAIL, x, y, u16::from(dir));
    // `CharDesc`：`Feature(i32 LE) + Status(i32 LE)` = 8B（与 `sm_death`/`SM_WALK` 同结构）。
    let mut desc = [0u8; 8];
    desc[0..4].copy_from_slice(&feature.to_le_bytes());
    desc[4..8].copy_from_slice(&status.to_le_bytes());
    gm_data_body(header, &edcode::encode(&desc))
}

/// `SM_MOVEFAIL`(28) 的 **冻结客户端布局**：与 oracle 只差 **尾** —— `Encode(12B TCharDesc)`。
///
/// 冻结客户端 `Source/MirClient/ClMain.pas:9939-9949` 的 `SM_MOVEFAIL` 支：
///
/// ```pascal
/// SM_MOVEFAIL:
///   begin
///     ActionFailed;
///     ActionLock := False;
///     frmMain.RecalcAutoMovePath();
///     DecodeBuffer(body, @desc, SizeOf(TCharDesc));   // 12B
///     ActionFailLock := False;
///     g_PlayScene.SendMsg(SM_TURN, msg.Recog, msg.param { x } , msg.tag { y } ,
///       msg.series { dir } , desc.Feature, desc.Status, '', desc.StatusEx);
///   end;
/// ```
///
/// `TCharDesc = packed record Feature: Integer; Status: Integer; StatusEx: Integer; end;`
/// （`Source/Common/Grobal2.pas:3515`）⇒ 客户端从尾按 **12B** 解码
/// （`DecodeBuffer`，`Source/Common/EDcode.pas:894-903`：`DeCodeBuf(...)` 后 `Move(EncBuf, buf^, bufsize)`）。
///
/// # 何时才需要它（与 D-9 家族的边界 —— 默认 **不用**）
///
/// 28 **不是** D-9 家族成员：`tests/parity/whitelist.md` 的 D-9 只含 52/53/31/1103，依据是客户端
/// `{$IF HIGHHP}` 站点 10225/10269/10338/10365（`Grobal2.pas:39 = HIGHHP True`）——
/// `SM_MOVEFAIL` 支（`ClMain.pas:9939`）**不在** `{$IF HIGHHP}` 内，不受 `HIGHHP` 支配。
///
/// 客户端读 12B，但 oracle 的 8B 尾恰好覆盖客户端真正消费的两个字段：
/// `DeCodeBuf` 把 8B 解出后 `Move(..., 12)` 再抄 4B（越界读到的旧栈），灌进 `desc.StatusEx`；
/// 而 `desc.Feature`/`desc.Status`（`SM_TURN` 的 Feature/State）**正确**，`desc.StatusEx` 只被当作
/// `IPInfo` → `Actor.m_nIPowerLvl`（`PlayScn.pas:3675-3677`，9th 实参）。⇒ **功能面不要求 12B**。
///
/// 故 28 生产默认 = oracle（见 [`sm_movefail_frame`]）；本函数只在显式开关
/// `MIR2_CLIENT_MOVEFAIL_LAYOUT`（默认关）打开时参与选路，偏离登记 **D-10**。
///
/// `status_ex` = 客户端 `StatusEx`（本仓 buff/状态扩展位未移植 ⇒ 调用方传 0）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_movefail_client(
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    feature: i32,
    status: i32,
    status_ex: i32,
) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_MOVEFAIL, x, y, u16::from(dir));
    // `TCharDesc`（`Grobal2.pas:3515`）：`[Feature:i32][Status:i32][StatusEx:i32]` = 12B。
    let mut desc = [0u8; 12];
    desc[0..4].copy_from_slice(&feature.to_le_bytes());
    desc[4..8].copy_from_slice(&status.to_le_bytes());
    desc[8..12].copy_from_slice(&status_ex.to_le_bytes());
    gm_data_body(header, &edcode::encode(&desc))
}

/// `MIR2_CLIENT_MOVEFAIL_LAYOUT` 的 **纯解析（吃 env 原文、不读 env）**：
/// `Some("1"|"true"|"TRUE"|"True")` ⇒ `true`；`None`（未设）/其它 ⇒ `false`
/// —— **「开关未设 ⇒ oracle」这个默认值的唯一真源**。
///
/// 把「开关读取」与「选路」[`movefail_route`] 解耦成纯函数，正是为了让**默认值**可被
/// **无 env、确定性** 地钉住（默认被翻成 `true` ⇒ 断言必红，不受测试线程并发影响）。
#[must_use]
fn movefail_layout_flag(raw: Option<&str>) -> bool {
    matches!(raw, Some("1") | Some("true") | Some("TRUE") | Some("True"))
}

/// `MIR2_CLIENT_MOVEFAIL_LAYOUT`（**默认关**）：`1|true|TRUE|True` ⇒ 28 生产帧走冻结客户端 12B
/// [`sm_movefail_client`]（偏离登记 D-10）；未设/其它 ⇒ oracle 逐字节 [`sm_movefail`]（**默认**）。
///
/// **env 读取隔离在本层**（本开关唯一读 env 之处）；解析 [`movefail_layout_flag`] 与选路
/// [`movefail_route`] 都是纯函数。
///
/// **不并入 D-9 的 `MIR2_CLIENT_ABILITY_LAYOUT`**：28 不在 D-9 家族（见 [`sm_movefail_client`]），
/// 复用同一开关 = 把 28 的生产默认绑到 D-9 口径上（正是 v4 判阻塞的点）。
#[must_use]
pub fn client_movefail_layout_enabled() -> bool {
    movefail_layout_flag(std::env::var("MIR2_CLIENT_MOVEFAIL_LAYOUT").ok().as_deref())
}

/// 28 帧选路的 **纯函数接缝**（吃 `bool`、**不读 env**）：生产入口 [`sm_movefail_frame`] 与
/// 单向牙测试共用。目的是让「默认走错布局 / 两支对调」可被 **确定性** 变异杀死（不依赖进程 env，
/// 不受测试线程并发影响）。
#[allow(clippy::too_many_arguments)]
fn movefail_route(
    client_layout: bool,
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    feature: i32,
    status: i32,
    status_ex: i32,
) -> Vec<u8> {
    if client_layout {
        sm_movefail_client(actor_id, x, y, dir, feature, status, status_ex)
    } else {
        sm_movefail(actor_id, x, y, dir, feature, status)
    }
}

/// 28 帧的 **生产选路**：**默认 = oracle 逐字节** [`sm_movefail`]（8B `CharDesc` 尾）。
/// 仅当显式打开 `MIR2_CLIENT_MOVEFAIL_LAYOUT`（默认关）才走冻结客户端 12B [`sm_movefail_client`]。
///
/// （v4 对抗验证判阻塞项：本函数此前默认走客户端布局，越出 D-9 家族且未登记 ⇒ 默认已回 oracle。）
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_movefail_frame(
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    feature: i32,
    status: i32,
    status_ex: i32,
) -> Vec<u8> {
    movefail_route(
        client_movefail_layout_enabled(),
        actor_id,
        x,
        y,
        dir,
        feature,
        status,
        status_ex,
    )
}

/// `SM_DISAPPEAR`(30)（`PlayObject.Message.cs:1651-1654` 的 `RM_DISAPPEAR` 支）：
///
/// ```text
/// ClientMsg = MakeMessage(SM_DISAPPEAR, processMsg.ActorId, 0, 0, 0);
/// SendSocket(ClientMsg);      // 无尾
/// ```
///
/// 头 = `(Recog=消失对象 ActorId, Param=0, Tag=0, Series=0)`，**无尾**。
///
/// 两个发射点，**观察者集合都归到「消失对象的 ActorId」**：
/// - `PlayObject.Run` 的 `VisibleFlag.Hidden` 支（`PlayObject.cs:3210-3219`）——
///   `SendMsg(baseObject, RM_DISAPPEAR, 0, 0, 0, 0)`；`SendMsg(IActor baseObject, …)`
///   （`BaseObject.Message.cs:98-158`）把 `SendMessage.ActorId` 置为 `baseObject.ActorId`
///   ⇒ 观察者自己的会话产出该帧，`Recog` = 消失对象；
/// - `BaseObject.DisappearA`（`BaseObject.cs:1521-1524`）——`SendRefMsg(RM_DISAPPEAR, 0, 0, 0, 0, "")`
///   广播给同图观察者，收件人 `case` 用 `processMsg.ActorId` ⇒ 同样 `Recog` = 消失对象。
///
/// 守卫：前者仅 `!baseObject.FixedHideMode && !baseObject.Ghost` 才发（防重复占带宽）。
/// 客户端（`ClMain.pas:10534-10539`）把 30 与 `SM_HIDE`/`SM_GHOST` 同支处理：**不读尾**，
/// `msg.Recog == 自己` 时忽略。⇒ oracle 与冻结客户端布局 **一致**，无需双分支。
/// 金标准 `c-line-baseline-c2-20261010` 的 15 条 30 帧（全 `Recog != 0`、`param/tag/series=0`、
/// `body_len=0`）逐字节锚在本模块测试里。
#[must_use]
pub fn sm_disappear(actor_id: i32) -> Vec<u8> {
    gm_data_body(
        command_header(actor_id, messages::SM_DISAPPEAR, 0, 0, 0),
        &[],
    )
}

/// `SM_CLEAROBJECTS`(633)（`PlayObject.Message.cs:1765-1773` 的 `RM_CLEAROBJECTS` 支）：
///
/// ```text
/// VisibleEvents.Clear();
/// for (int i = 0; i < VisibleItems.Count; i++) VisibleItems[i] = null;
/// VisibleItems.Clear();
/// SendDefMessage(SM_CLEAROBJECTS, 0, 0, 0, 0);   // 无尾
/// ```
///
/// 头 = `(Recog=0, Param=0, Tag=0, Series=0)`，**无尾**。
///
/// 发射点（**自己发给自己**，进图/切图清屏；两个点都用 5 参 `SendMsg(int wIdent, …)` ⇒
/// `SendMessage.ActorId = this.ActorId`，`BaseObject.Message.cs:34-95`）：
/// - `PlayObject.cs:3772`（`SpaceMove`/换图，清 `VisibleItems/Events/Actors` 后）；
/// - `BaseObject.cs:1133`（`SpaceMoveGet`，`AddMapObject` 后）。
///
/// 前置清空动作（`VisibleEvents`/`VisibleItems`/`VisibleActors`）是 **观察者侧视图清理**，
/// 属世界/会话状态，不在编码层。客户端（`ClMain.pas:10845-10849`）只置 `g_boMapMoving := True`
/// ⇒ 不读任何字段；oracle 与冻结客户端布局 **一致**，无需双分支。
#[must_use]
pub fn sm_clearobjects() -> Vec<u8> {
    gm_data_body(command_header(0, messages::SM_CLEAROBJECTS, 0, 0, 0), &[])
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
/// `SM_MERCHANTDLGCLOSE(644)`（`PlayObject.Message.cs:1806-1808` 的 `RM_MERCHANTDLGCLOSE` 支）：
/// `SendDefMessage(SM_MERCHANTDLGCLOSE, processMsg.nParam1, 0, 0, 0)`。
///
/// 触发方 `GuildOfficial.UserSelect` 的 `@exit` 支（`GuildOfficial.cs:84`）发
/// `SendMsg(this, RM_MERCHANTDLGCLOSE, 0, ActorId, 0, 0)` ⇒ `Recog` = **NPC** 的 `ActorId`
/// （不是玩家自己），无尾。
#[must_use]
pub fn sm_merchantdlgclose(npc_actor_id: i32) -> Vec<u8> {
    gm_data_body(
        command_header(npc_actor_id, messages::SM_MERCHANTDLGCLOSE, 0, 0, 0),
        &[],
    )
}

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
    /// `AntiMagic` 保留完整 16 位（G-6）；其余 6 项是低字节语义。
    pub sub_ability: Option<[u16; 7]>,
    /// `SM_ADJUST_BONUS` = `(BonusPoint, A, B, C)`（三段能力模板）。
    pub adjust_bonus: Option<AdjustBonusParts>,
    /// `SM_DAYCHANGING` = `(Bright, DayBright)`。
    pub day_changing: Option<(u16, u16)>,
    /// `SM_TURN` = `(ActorId, X, Y, Dir, Light, CharDesc 序列化原文, msg)`。
    pub turn: Option<TurnFrame>,
    /// `SM_SENDUSEITEMS(621)` 的**已编码尾** `i + '/' + EncodeBuffer(ClientItem) + '/'`（逐槽）。
    /// `None`/空 = oracle 的 `if (!string.IsNullOrEmpty(sSendMsg))` 门 ⇒ 不产帧。
    pub use_items_tail: Option<Vec<u8>>,
    /// `SM_SENDMYMAGIC(211)` = `(MagicList.Count, 已编码尾 EncodeBuffer(ClientMagic)+'/')`。
    /// `None`/空 = 不产帧。
    pub magic: Option<(u16, Vec<u8>)>,
    /// **Lane B**：`UserLogon → RecalcAbilitys()` 尾段直接发出的 212/210 事件
    /// （`PlayObject.cs:2764-2779`）。帧在 `658` 之后、登录消息队列出队（`51` 等）之前逐条产出；
    /// 真机实测序 `658 → 212 → 51`。
    pub magic_journal: Vec<MagicEvent>,
    /// `SM_CHANGELIGHT(654)` = `(Light, Config.nClientKey)`（`Message.cs:1840`；`Recog=ActorId`）。
    /// 来自 `RM_LOGON` 里排队的 `SendMsg(RM_CHANGELIGHT, 0,0,0,0)`（`Message.cs:1682`）。
    /// `None` ⇒ 跳过并登记（等待世界时钟/AOI 侧的 654 另有通路）。
    pub changelight: Option<(u8, i32)>,
    /// **G2b**：`SM_CHANGEGUILDNAME(750)` 的**已编码帧**（`guild_frames::login_changename_750` 产出）。
    ///
    /// oracle `PlayObject.cs:1391-1398`：`MyGuild = MemberOfGuild(ChrName)`；仅当 `MyGuild != null`
    /// 才 `GetRankName` ⇒ `playObject.SendMsg(playObject, RM_CHANGEGUILDNAME, ...)`（优先级
    /// `(byte)RM_CHANGEGUILDNAME = 61`）。**无会 ⇒ `None`，队列不占堆位**（简报 §0.4 / §3.1）。
    ///
    /// 属 `game_svr` lib ⇒ 只收已编码字节，不依赖 bin 侧的 `guild_frames` 模块。
    pub guild_changename: Option<Vec<u8>>,
    /// 登录 `UserLogon` 的 3+1 条 `SysMsg` 正文（GBK 原文；入队序见 [`LoginSysMsgs`]）。
    pub login_sys_msgs: LoginSysMsgs,
    /// `[Server] TestServer`（`ServerConf.cs:23`）：第三条 `SysMsg`（`StartNoticeMsg`）的入队门
    /// （`PlayObject.cs:1369-1373`）。
    pub test_server: bool,
    /// `PayMent == 3`（`TestServer || ServiceMode` 覆盖后的值，`PlayObject.cs:1145-1148`）：
    /// 第四条 `SysMsg`（`NowIsFreePlayMode`）的入队门（`:1417-1420`）。
    pub pay_ment_three: bool,
}

/// 登录 `UserLogon` 的 3+1 条 `SysMsg`（`PlayObject.cs:1342-1383` 的攻击模式/帮助/欢迎语 +
/// `:1417-1420` 的免费模式）。正文是 **GBK 字节**（协议层不转 UTF-8）。
///
/// `None` = 该槽的正文输入缺失（oracle 一定有文本；缺输入时**槽位仍占堆**、出队不产帧并登记）。
#[derive(Debug, Clone, Default)]
pub struct LoginSysMsgs {
    /// 槽 0：`switch (AttatckMode)` 选中的文案（枚举外 ⇒ `None` 且不占槽）。
    pub attack_mode: Option<Vec<u8>>,
    /// 槽 1：`MessageSettings.StartChangeAttackModeHelp`。
    pub change_help: Option<Vec<u8>>,
    /// 槽 2：`MessageSettings.StartNoticeMsg`（仅 `TestServer` 入队）。
    pub notice: Option<Vec<u8>>,
    /// 槽 3：`MessageSettings.NowIsFreePlayMode`（仅 `PayMent==3` 入队）。
    pub free_play: Option<Vec<u8>>,
    /// `MsgColor.Green` 的 `(GreenMsgFColor, GreenMsgBColor)`（`BaseObject.cs:1645-1652`；
    /// 落在 `SM_SYSMESSAGE.Param = make_word(f, b)`，`PlayObject.Message.cs:1715`）。
    pub green: (u8, u8),
}

impl LoginSysMsgs {
    /// 槽号 → 正文（`None` = 输入缺失）。
    #[must_use]
    pub fn text(&self, slot: u8) -> Option<&[u8]> {
        match slot {
            0 => self.attack_mode.as_deref(),
            1 => self.change_help.as_deref(),
            2 => self.notice.as_deref(),
            3 => self.free_play.as_deref(),
            _ => None,
        }
    }
}

/// 登录消息泵里的排队条目（`BaseObject.Initialize` + `PlayObject.UserLogon` 的全部
/// `MsgQueue.Enqueue` 点）。
///
/// 变体的**声明/入队序 = oracle 调用序**（不是出队序）：
///
/// 1. `RM_TURN(10001)` ← `Initialize→AddToMap→SendRefMsg(RM_TURN)`
///    （`BaseObject.cs:1370-1377`）。`SendRefMsg` 的格子扫描把**自己**也算作
///    `Race==Play` 的目标（`BaseObject.Message.cs:427-433`）⇒ 自己也被入队；帧在
///    `PlayObject.Message.cs:1528` 被 `processMsg.ActorId != ActorId` 门抑制 ⇒ **只占堆位**。
/// 2. `RM_LOGON(10050)`（`PlayObject.cs:1176`）→ High=0。
/// 3. `RecalcAbilitys()` 的两条 `RM_CHARSTATUSCHANGED(10139)`：`StatusChanged()`
///    （`SendRefMsg`，`PlayObject.cs:2875`）与 `SendUpdateMsg`（`:2876`）各一条，优先级
///    `(byte)10139 = 155`。`SendUpdateMsg` 的去重扫不到（队首是 LOGON）⇒ 两条都在。
/// 4. 三条 `SysMsg`（攻击模式 / CTRL-H 帮助 / `TestServer` 欢迎语，`:1320-1383`）⇒ 252。
/// 5. 六条 `SendPriorityMsg`（`:1385-1390`），**源码序**：
///    `52(High)`, `752(High)`, `811(Normal)`, `46(Normal)`, `621(High)`, `211(High)`。
/// 6. `RefShowName()→SendRefMsg(RM_USERNAME)`（`:1415` 附近；优先级 `(byte)8101 = 165`）
///    ⇒ 只占堆位（`RM_LOGON` 里 `ClientQueryUserName` 的直发 42 已由 `username` 输入产出）。
/// 7. `PayMent==3`（`TestServer || ServiceMode`）的第四条 `SysMsg`（`:1419`）⇒ 252。
///
/// `RM_CHANGELIGHT` 不在这里，它在 `RM_LOGON` 被**出队处理**时才入队
/// （`PlayObject.Message.cs:1683`，见 [`EnterWorldInputs::frames`]）。
#[derive(Debug, Clone, Copy)]
enum LoginQueued {
    /// `RM_TURN(10001)`：`Initialize→AddToMap→SendRefMsg`，优先级 `(byte)10001 = 17`；
    /// 自身 TURN 被 `ActorId != ActorId` 门抑制 ⇒ 只占堆位。
    Turn,
    /// `RM_LOGON`：`SendPriorityMsg(..., MessagePriority.High)` ⇒ 优先级 0。
    Logon,
    /// `RM_CHARSTATUSCHANGED(10139)`：`StatusChanged()` 与 `SendUpdateMsg` 各一条，优先级 155；
    /// 两条都会产 `SM_CHARSTATUSCHANGED(657)`，本输入集不建模 657 的尾 ⇒ 只占堆位。
    CharStatusChanged,
    /// 登录 `SysMsg(..., MsgColor, MsgType.Hint)` ⇒ `BaseObject.SysMsg` 走 `SendMsg(RM_SYSMESSAGE)`
    /// ⇒ 优先级 `(byte)RM_SYSMESSAGE = 252`。
    ///
    /// 槽号 = oracle **入队序**（`PlayObject.cs:1342-1383/1417-1420`）：
    /// 0 = 攻击模式文案（`switch (AttatckMode)`，枚举外不发 ⇒ 不占槽）、
    /// 1 = `StartChangeAttackModeHelp`（恒发）、2 = `StartNoticeMsg`（`TestServer`）、
    /// 3 = `NowIsFreePlayMode`（`PayMent==3`，在 `RefShowName` **之后**入队）。
    /// 出队时按槽号取 [`EnterWorldInputs::login_sys_msgs`] 的 GBK 正文产帧；
    /// 正文缺失仍占堆位、不产帧（登记，绝不因缺文本少占位）。
    SysMsg(u8),
    /// `RM_ABILITY(52)`：`SendPriorityMsg(..., High)` ⇒ 0。
    Ability,
    /// `RM_SUBABILITY(752)`：`SendPriorityMsg(..., High)` ⇒ 0。
    SubAbility,
    /// `RM_ADJUST_BONUS(811)`：`SendPriorityMsg(..., Normal)` ⇒ 1。
    AdjustBonus,
    /// `RM_DAYCHANGING(46)`：`SendPriorityMsg(..., Normal)` ⇒ 1。
    DayChanging,
    /// `RM_SENDUSEITEMS(621)`：`SendPriorityMsg(..., High)` ⇒ 0。
    SendUseItems,
    /// `RM_SENDMYMAGIC(211)`：`SendPriorityMsg(..., High)` ⇒ 0。
    SendMyMagic,
    /// **G2b**：`RM_CHANGEGUILDNAME(10301)`（`PlayObject.cs:1397` 的 `GetRankName` 内
    /// `SendMsg(playObject, RM_CHANGEGUILDNAME, ...)`）⇒ 优先级 `(byte)10301 = 61`。
    ///
    /// 入队位置：六条 `SendPriorityMsg`（52/752/811/46/621/211）之后、`RM_USERNAME` 之前
    /// （oracle `PlayObject.cs:1385-1390` → `:1391-1398` → `RefShowName`）。**仅 `MyGuild != null`
    /// 时入队**（无会不占堆位）；出队时直接产出 [`EnterWorldInputs::guild_changename`] 的字节。
    ChangeGuildName,
    /// `RM_USERNAME(8101)`：`RefShowName()→SendRefMsg`，优先级 165；只占堆位。
    Username,
    /// `RM_CHANGELIGHT(654)`：`SendMsg` ⇒ 优先级 `(byte)RM_CHANGELIGHT = 153`。
    ChangeLight,
    /// 其他 actor 的出现槽（本宿主由 AOI 侧下发帧，这里只占堆位）。
    /// 优先级随种类由 [`OtherAppear::priority`] 在入队时给出（`RM_TURN` 17 / `RM_DEATH` 37 /
    /// `RM_SKELETON` 40）；出队不产帧。
    OtherAppear,
}

/// `PlayObject.SearchViewRange` 对**其他** actor 入队的出现/死亡帧种类
/// （`PlayObject.cs:3220-3240`）。本宿主不在这里产帧（AOI 侧 [`crate::drain_aoi_downlink`]
/// 负责），只用于复刻真机 `.NET PriorityQueue` 的堆布局——等优先级/不同优先级的槽位数量
/// 会改变 `654` 之后 `42`/`100` 的出队序（同优先级不是 FIFO）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtherAppear {
    /// `SendMsg(baseObject, RM_TURN, ...)`（活着）：优先级 `(byte)10001 = 17`。
    Turn,
    /// `SendMsg(baseObject, RM_DEATH, ...)`（死亡且非 Skeleton）：`(byte)10021 = 37`。
    Death,
    /// `SendMsg(baseObject, RM_SKELETON, ...)`：`(byte)10024 = 40`。
    Skeleton,
}

impl OtherAppear {
    /// oracle 的入队优先级（`SendMsg(baseObject, ...)` 的 `(byte)wIdent`）。
    #[must_use]
    pub fn priority(self) -> u8 {
        match self {
            Self::Turn => (messages::RM_TURN & 0xFF) as u8,
            Self::Death => (messages::RM_DEATH & 0xFF) as u8,
            Self::Skeleton => (messages::RM_SKELETON & 0xFF) as u8,
        }
    }
}

/// `MessagePriority.High`（`src/OpenMir2/Enums/MessagePriority.cs:16`）= 0。
const PRIORITY_HIGH: u8 = 0;
/// `MessagePriority.Normal`（`src/OpenMir2/Enums/MessagePriority.cs:12`）= 1。
const PRIORITY_NORMAL: u8 = 1;
/// `(byte)RM_TURN` = 17（`SendRefMsg`→`SendMsg` 的入队优先级）。
const PRIORITY_RM_TURN: u8 = (messages::RM_TURN & 0xFF) as u8;
/// `(byte)RM_CHARSTATUSCHANGED` = 155。
const PRIORITY_RM_CHARSTATUSCHANGED: u8 = (messages::RM_CHARSTATUSCHANGED & 0xFF) as u8;
/// `(byte)RM_USERNAME` = 165。
const PRIORITY_RM_USERNAME: u8 = (messages::RM_USERNAME & 0xFF) as u8;
/// **G2b**：`(byte)RM_CHANGEGUILDNAME` = 61（`GuildInfo.cs:499` 的 `SendMsg` 入队优先级）。
///
/// 牙口径（评审员更正 2）：把它改成 **0**（750 跑到 46 前）或 **>=154**（750 跑到 654 后）
/// 才改变出队序；改成 1 或 152 **不会**改变出队序，故不能当牙。
const PRIORITY_RM_CHANGEGUILDNAME: u8 = (messages::RM_CHANGEGUILDNAME & 0xFF) as u8;

impl EnterWorldInputs {
    /// 按 oracle 的登录消息队列（.NET 8 `PriorityQueue<SendMessage, byte>`，**四叉最小堆**）
    /// 模拟完整入队/出队过程后产出帧序（**模型 D**，2026-10-19 三次真机抓包裁定）。
    ///
    /// # 入队（oracle 调用序，见 [`LoginQueued`] 的逐条出处）
    ///
    /// `RM_TURN(17)`（`Initialize→AddToMap→SendRefMsg`，自己也在扫描目标里）→
    /// `RM_LOGON(0)` → `RM_CHARSTATUSCHANGED(155)`×2（`StatusChanged` + `SendUpdateMsg`）→
    /// 三条 `SysMsg(252)` → 六条 `52(0),752(0),811(1),46(1),621(0),211(0)`（**源码序**）→
    /// `RM_USERNAME(165)`（`RefShowName`）→ 第四条 `SysMsg(252)`（`PayMent==3`）。
    /// `RM_LOGON` 出队处理时再入 `RM_CHANGELIGHT(654)`（`Message.cs:1682`，优先级
    /// `(byte)RM_CHANGELIGHT = 153`）。
    ///
    /// # 出队（**模型 D**，全部来自源码 + net8.0.26 实跑 BCL）
    ///
    /// 三次 C# 抓包（同一 oracle/角色 `aaa`，`E:/tmp/window/cliente2e/csharp/`）：
    /// - 无装备无技能（`s2c`/`s2c-w03`/`s2c-w04`）：`…,1106,52,752,811,46,654,657,657,42,100×4`
    /// - 有武器无有效技能（`s2c-621b`）：`…,1106,52,752,621,811,46,654,…`
    /// - 有武器 + 有效技能（`s2c-621c`）：`…,1106,52,752,621,211,811,46,654,657,657,42,100×4`
    ///
    /// 本函数按上面的**完整入队内容**跑四叉堆，出队序恰为
    /// `LOGON,52,752,621,211,811,46,TURN(无帧),654,657×2,42,100×4`，三种输入逐项命中。
    /// 关键在**队列内容**而不是改入队序：旧模型 C/C′ 漏掉了 `RM_TURN`（它在 `RM_LOGON`
    /// **之前**入队，改变了整棵树）与两条 `RM_CHARSTATUSCHANGED`、`RM_USERNAME`、第四条
    /// `SysMsg`；其中队尾条目决定 `RemoveRootNode` 回填根的元素，能把 `811/46` 的相对序
    /// 翻过来（这正是旧模型在「621/211 出帧」时预测错的原因）。
    ///
    /// ⚠ **前提**：`Config.TestServer=TRUE`（⇒ `PayMent=3`）给出 3+1 条 `SysMsg`。
    /// 条数变化会重排堆；非 `TestServer` 的部署尚未取样本（R 级窗口余项）。
    /// 带发光装备/`IsSpirit` 会额外插入 `RM_CHANGELIGHT(153)`/`RM_SPIRITSUITE(255)` 槽位。
    ///
    /// # 空槽仍占位
    ///
    /// 缺输入的六条**无条件入队**（oracle 的消息一定入队，本层只是缺编码输入）；空槽会改变
    /// 堆布局，故绝不能跳过入队。621/211 的「尾为空则不發」门只在出队时决定是否产帧。
    ///
    /// # 非队列帧
    ///
    /// `SM_TURN(10)` 属 AOI/出现帧路径（`PlayObject.cs:3240` 的 `RM_TURN`），不是登录队列成员；
    /// 金标准 capture 里它早于 654（seq 30 vs 62），故在 654 出队前插入。`658` 由
    /// `SendNotice` 在消息泵之前直发，恒第一条。
    ///
    /// 生产路径用 [`Self::frames_split`]：`RM_TURN`（优先级 17）的**其他 actor** 出现帧由世界侧
    /// AOI 在本批之后、`after` 之前下发（oracle 同一 `Run()` 里 17 < 153 的出队序）。本函数把
    /// 两段拼起来，供只关心「完整登录队列内容」的用例使用。
    #[must_use]
    pub fn frames(&self) -> Vec<Vec<u8>> {
        let (mut before, after) = self.frames_split_with_others(&[]);
        before.extend(after);
        before
    }

    /// 登录帧序的**两段切分**（生产路径）：`before` = 优先级 ≤ `(byte)RM_TURN`(=17) 的出队帧，
    /// `after` = 其后的一切（`654`/`657×2`/第二 `42`/`100×4`，含优先级 61 的 `750`）。
    ///
    /// oracle 的 `PlayObject.Run()` 在**同一次**消息泵里消费整个队列；`RM_TURN(17)` 的**其他 actor**
    /// 出现帧由观察者自己的 `SearchViewRange` 入队（`PlayObject.cs:3220-3240`），因此
    /// 654（153）恒在出现帧**之后**。本宿主的出现帧由世界 tick 的 [`crate::drain_aoi_downlink`]
    /// 下发 ⇒ 登录点先发 `before`，`after` 由 [`crate::drain_login_tail`] 在该会话首次
    /// `SearchViewRange` 的同 tick 补发（C# 实测 2026-10-22：`10/41 ×6 → 654 → 657,657 → 42 → 100×4`）。
    #[must_use]
    pub fn frames_split(&self) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        self.frames_split_with_others(&[])
    }

    /// [`Self::frames_split`] 的**真机堆布局**版本：`others` = 本 tick `SearchViewRange`
    /// 对其他 actor 入队的出现/死亡条目（顺序 = `VisibleActors` 顺序）。
    ///
    /// 这些条目在 `UserLogon` 的全部入队之后、`Run()` 的消息泵之前入队（`SearchViewRange` 在
    /// `WorldServer.cs:663-665` 先于 `playObject.Run()`），因此追加到队列尾部即可。它们**不产帧**
    /// （AOI 侧下发），但会改变堆形 ⇒ 决定 `654` 之后同优先级 `100×4` 的出队序。
    #[must_use]
    pub fn frames_split_with_others(&self, others: &[OtherAppear]) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
        let mut out = Vec::new();
        let mut split_at: Option<usize> = None;
        // **R1-1**：oracle `SendNotice` 无条件 `SendDefMessage(..., sNoticeMsg)`；即使
        // `sNoticeMsg` 为空也照发 header-only 658（`SendDefMessage` 的 `if (!string.IsNullOrEmpty)`
        // 只决定是否追加尾，空时走 `else SendSocket(ClientMsg)`）。故这里**不**用 `if let`。
        out.push(sm_sendnotice(&self.notice));
        // **Lane B**：`UserLogon()` 里 `RecalcAbilitys()` 的直接帧（212/210）先于队列出队
        // （真机 `658 → 212 → 51`）。顺序 = oracle 调用序（火球分支在前、治愈分支在后）。
        for event in &self.magic_journal {
            out.push(magic_event_frame(event));
        }

        // ---- 入队：按 oracle 调用序把 15 条消息压进四叉最小堆（LOGON 出队时再入 654）----
        // 顺序 = 源码执行序（见 `LoginQueued` 的逐条行号）。**不要**按「可观测序」重排：
        // 真机实拍的三段序完全由这个内容 + .NET 8 四叉堆决定。
        let mut queue: NetPriorityQueue<LoginQueued> = NetPriorityQueue::new();
        // ① Initialize→AddToMap→SendRefMsg(RM_TURN)：自己也在扫描目标里（帧被自身门抑制）。
        queue.enqueue(LoginQueued::Turn, PRIORITY_RM_TURN);
        // ② RM_LOGON（High）。
        queue.enqueue(LoginQueued::Logon, PRIORITY_HIGH);
        // ③ RecalcAbilitys：StatusChanged() 的 SendRefMsg 与 SendUpdateMsg 各一条 657。
        queue.enqueue(
            LoginQueued::CharStatusChanged,
            PRIORITY_RM_CHARSTATUSCHANGED,
        );
        queue.enqueue(
            LoginQueued::CharStatusChanged,
            PRIORITY_RM_CHARSTATUSCHANGED,
        );
        // ③b `RecalcAbilitys()` 的 `oldlight != Light` 支（`PlayObject.cs:2878-2885`）：
        //     登录时 `BaseObject.Light` 从 0 起（`BaseObject.cs:384` 的 ctor），而
        //     `Light = GetMyLight()`（装备发光 / 高等级基线 1）⇒ `Light != 0` 时**多入队一条**
        //     `RM_CHANGELIGHT(153)`（`SendRefMsg` 把玩家自己也扫进目标）。它与 `RM_LOGON` 出队时
        //     再入的那条（`Message.cs:1683`）**各发一帧 654**（两次 Param 相同 = 处理时的 `Light`）。
        //     金标准角色无光源（`Light=0`）⇒ 只有一条；带火把/高等级角色有两条。
        if self.light != 0 {
            queue.enqueue(
                LoginQueued::ChangeLight,
                (messages::RM_CHANGELIGHT & 0xFF) as u8,
            );
        }
        // ④ 登录 SysMsg（`PlayObject.cs:1342-1383`）：`switch (AttatckMode)` 的攻击模式文案
        //    （仅枚举 0..=6 时入队；`switch` 无 `default` ⇒ 枚举外一条都不发、不占槽）
        //    + `StartChangeAttackModeHelp`（恒发）+ `TestServer` 的 `StartNoticeMsg`。
        let prio_sysmsg = (messages::RM_SYSMESSAGE & 0xFF) as u8;
        if self.attack_mode.is_some_and(|m| m <= 6) {
            queue.enqueue(LoginQueued::SysMsg(0), prio_sysmsg);
        }
        queue.enqueue(LoginQueued::SysMsg(1), prio_sysmsg);
        if self.test_server {
            queue.enqueue(LoginQueued::SysMsg(2), prio_sysmsg);
        }
        // ⑤ 六条 SendPriorityMsg（`PlayObject.cs:1385-1390`），**源码序**：
        //    52,752,811,46,621,211。堆在同一内容下自然给出真机序 52,752,621,211,811,46。
        queue.enqueue(LoginQueued::Ability, PRIORITY_HIGH); // 52
        queue.enqueue(LoginQueued::SubAbility, PRIORITY_HIGH); // 752
        queue.enqueue(LoginQueued::AdjustBonus, PRIORITY_NORMAL); // 811
        queue.enqueue(LoginQueued::DayChanging, PRIORITY_NORMAL); // 46
        queue.enqueue(LoginQueued::SendUseItems, PRIORITY_HIGH); // 621
        queue.enqueue(LoginQueued::SendMyMagic, PRIORITY_HIGH); // 211

        // ⑤b **G2b**：`MyGuild != null` 才 `GetRankName` → `SendMsg(RM_CHANGEGUILDNAME)`
        //    （`PlayObject.cs:1391-1398`；`GuildInfo.cs:485-509`）。无会 ⇒ **不入队**
        //    （不占堆位；oracle 的 `GetRankName` 根本没被调用）。
        if self.guild_changename.is_some() {
            queue.enqueue(LoginQueued::ChangeGuildName, PRIORITY_RM_CHANGEGUILDNAME);
        }

        // ⑥ RefShowName()→SendRefMsg(RM_USERNAME)：自己也在可见人类列表里（只占堆位）。
        queue.enqueue(LoginQueued::Username, PRIORITY_RM_USERNAME);
        // ⑦ `PayMent==3`（`TestServer || ServiceMode`）的第四条 SysMsg：
        //    `NowIsFreePlayMode`（`PlayObject.cs:1417-1420`，位置在 `RefShowName` 之后）。
        if self.pay_ment_three {
            queue.enqueue(LoginQueued::SysMsg(3), prio_sysmsg);
        }

        // ⑧ 其他 actor 的出现/死亡槽（`SearchViewRange`，`PlayObject.cs:3220-3240`）：
        //    本 tick 的 `VisibleActors` 里 `Show` 且非自己的对象，按 oracle 顺序入队。
        for other in others {
            queue.enqueue(LoginQueued::OtherAppear, other.priority());
        }

        // ---- 出队：逐条按堆序处理 ----
        // 登录自身 `RM_TURN` 的帧在 oracle 被 `ActorId != ActorId` 门抑制；本输入集里 `turn`
        // 只用于金标准夹具的 AOI 形状复现 ⇒ 多名 `ChangeLight` 槽下也只插入一次。
        let mut turn_emitted = false;
        while let Some(priority) = queue.peek_priority() {
            // 切分点 = 第一条优先级 **大于** `(byte)RM_TURN` 的条目。`RM_TURN` 的其余条目
            // （其他 actor 的出现帧）由 AOI 侧补在同一位置，故 `> 17` 而非 `>= 17`。
            if split_at.is_none() && priority > PRIORITY_RM_TURN {
                split_at = Some(out.len());
            }
            let Some(item) = queue.dequeue() else {
                break;
            };
            match item {
                LoginQueued::Logon => {
                    // `RM_LOGON` 支（`PlayObject.Message.cs:1680-1692`）在**出队处理时**
                    // 直接 `SendSocket/SendDefMessage` 的帧，按 handler 内调用序产出。
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
                    // `PlayObject.Message.cs:1683`：`SendMsg(RM_CHANGELIGHT, 0,0,0,0)` 在
                    // **LOGON 处理点**入队（不是 `UserLogon` 的调用序里）⇒ 优先级 153。
                    queue.enqueue(
                        LoginQueued::ChangeLight,
                        (messages::RM_CHANGELIGHT & 0xFF) as u8,
                    );
                }
                LoginQueued::Ability => {
                    if let Some((gold, job, game_gold, wabil)) = &self.ability {
                        out.push(sm_ability(*gold, *job, *game_gold, wabil));
                    }
                }
                LoginQueued::SubAbility => {
                    if let Some([m, hp, sp, ap, pr, hr, sr]) = self.sub_ability {
                        // 其余 6 项是 `MakeWord(byte, byte)` 语义（低字节）；数组里已是低字节值。
                        out.push(sm_subability(
                            m,
                            u8::try_from(hp).unwrap_or(0),
                            u8::try_from(sp).unwrap_or(0),
                            u8::try_from(ap).unwrap_or(0),
                            u8::try_from(pr).unwrap_or(0),
                            u8::try_from(hr).unwrap_or(0),
                            u8::try_from(sr).unwrap_or(0),
                        ));
                    }
                }
                LoginQueued::AdjustBonus => {
                    if let Some((bonus, a, b, c)) = &self.adjust_bonus {
                        out.push(sm_adjust_bonus(*bonus, a, b, c));
                    }
                }
                LoginQueued::DayChanging => {
                    if let Some((bright, day)) = self.day_changing {
                        out.push(sm_daychanging(bright, day));
                    }
                }
                LoginQueued::SendUseItems => {
                    // `PlayObject.cs:1389`（`UserLogon`）：`RM_SENDUSEITEMS`；oracle 的
                    // 「尾为空则不發」门（`Base.cs:1046`）在出队时判定。
                    if let Some(tail) = &self.use_items_tail {
                        if !tail.is_empty() {
                            out.push(sm_senduseitems(tail));
                        }
                    }
                }
                LoginQueued::SendMyMagic => {
                    // `PlayObject.cs:1390`（`UserLogon`）：`RM_SENDMYMAGIC`；空尾门同 `Base.cs:1065`。
                    if let Some((count, tail)) = &self.magic {
                        if !tail.is_empty() {
                            out.push(sm_sendmymagic(*count, tail));
                        }
                    }
                }
                LoginQueued::ChangeLight => {
                    // `SM_TURN(10)` 属 AOI/出现帧路径（`PlayObject.cs:3240`），不是队列成员；
                    // 金标准 capture 里 10 早于 654（seq 30 vs 62）⇒ 在 654 出队前插入。
                    if !turn_emitted {
                        if let Some((id, x, y, d, light, desc, msg)) = &self.turn {
                            out.push(sm_turn(*id, *x, *y, *d, *light, desc, msg));
                            turn_emitted = true;
                        }
                    }
                    // `Message.cs:1840`：`SendDefMessage(SM_CHANGELIGHT, ActorId, Light, nClientKey, 0)`。
                    if let Some((light, client_key)) = self.changelight {
                        out.push(crate::smframe_ext::sm_changelight(
                            self.actor_id,
                            light,
                            client_key,
                        ));
                    }
                }
                LoginQueued::ChangeGuildName => {
                    // `RM_CHANGEGUILDNAME` 出队处理 = `SendChangeGuildName`（`PlayObject.Base.cs:1625-1635`）；
                    // 字节已由 `guild_frames::sm_guild_changename_750` 编码好（有会恒带尾）。
                    if let Some(body) = &self.guild_changename {
                        out.push(body.clone());
                    }
                }
                LoginQueued::Username => {
                    // `RefShowName()`（`PlayObject.cs:1415`，`BaseObject.cs:1174-1177`）
                    // `SendRefMsg(RM_USERNAME, 0,0,0,0, GetShowName())` ⇒ 出队时
                    // `SM_USERNAME(ActorId, GetChrColor(baseObject), 0, 0)` + `EncodeString(GetShowName())`
                    // （`PlayObject.Message.cs:1662-1666`）。与 `RM_LOGON` 里 `ClientQueryUserName`
                    // 直发的那条**同字段同字节** ⇒ 直接复用 `username` 输入。
                    if let Some((id, color, name)) = &self.username {
                        out.push(sm_username(*id, *color, name));
                    }
                }
                LoginQueued::SysMsg(slot) => {
                    // `RM_SYSMESSAGE` 出队（`PlayObject.Message.cs:1714-1718`）：
                    // `SM_SYSMESSAGE(ActorId, make_word(f, b), 0, 1)` + `EncodeString(正文)`。
                    // 正文缺失 ⇒ 不产帧（槽位已按 oracle 占堆）。
                    if let Some(msg) = self.login_sys_msgs.text(slot) {
                        let (f, b) = self.login_sys_msgs.green;
                        out.push(sm_sysmessage(
                            self.actor_id,
                            u16::from(f),
                            u16::from(b),
                            msg,
                        ));
                    }
                }
                // 以下两条只占堆位：
                // - 自身 RM_TURN 被 `ActorId != ActorId` 门抑制（`PlayObject.Message.cs:1528`）；
                // - 两条 RM_CHARSTATUSCHANGED 会产 657，但其字节（CharStatus/HitSpeed）未在本输入集建模
                //   （登记 `openmir2-login-frame-order`）。
                LoginQueued::Turn | LoginQueued::CharStatusChanged | LoginQueued::OtherAppear => {}
            }
        }
        let at = split_at.unwrap_or(out.len());
        let after = out.split_off(at);
        (out, after)
    }
}

/// 组队帧编码 —— M3⑦-实现 2（卡 `openmir2-m3-party-group-state`）。
///
/// 全部走 oracle `PlayObject.Socket.cs:17-25` 的 `SendDefMessage(ident, recog, param, tag, series)`：
/// 12B 命令头 +（带 `sMsg` 时）尾 = `EDCode.EncodeString(sMsg)`。
/// 组队帧的 `tag`/`series` 恒 0（唯一例外 `SM_SYSMESSAGE` 的 `series=1`，见 [`sm_sysmessage`]）。
///
/// `SM_GROUPMODECHANGED(659)`（`PlayObject.Message.cs:926/929`）：`SendDefMessage(659, 0, param, 0, 0)`，
/// `param` = 1（开组）/0（关组），无尾。
#[must_use]
pub fn sm_groupmodechanged(param: u16) -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_GROUPMODECHANGED, param, 0, 0),
        &[],
    )
}

/// `SM_CREATEGROUP_OK(660)`（`PlayObject.Operate.cs:840`），无尾。
#[must_use]
pub fn sm_creategroup_ok() -> Vec<u8> {
    gm_data_body(command_header(0, messages::SM_CREATEGROUP_OK, 0, 0, 0), &[])
}

/// `SM_CREATEGROUP_FAIL(661)`（`PlayObject.Operate.cs:816/820/825/830`；`recog ∈ {-1,-2,-3,-4}`），无尾。
#[must_use]
pub fn sm_creategroup_fail(recog: i32) -> Vec<u8> {
    gm_data_body(
        command_header(recog, messages::SM_CREATEGROUP_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_GROUPADDMEM_OK(662)`（`PlayObject.Operate.cs:878`），无尾。
#[must_use]
pub fn sm_groupaddmem_ok() -> Vec<u8> {
    gm_data_body(command_header(0, messages::SM_GROUPADDMEM_OK, 0, 0, 0), &[])
}

/// `SM_GROUPADDMEM_FAIL(664)`（`PlayObject.Operate.cs:852/857/861/866/871`；`recog ∈ {-1,-5,-2,-3,-4}`），无尾。
#[must_use]
pub fn sm_groupaddmem_fail(recog: i32) -> Vec<u8> {
    gm_data_body(
        command_header(recog, messages::SM_GROUPADDMEM_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_GROUPDELMEM_OK(663)`（`PlayObject.Operate.cs:904`）：`SendDefMessage(663, 0,0,0,0, name)`
/// ⇒ 尾 = `EncodeString(name)`。
#[must_use]
pub fn sm_groupdelmem_ok(name: &[u8]) -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_GROUPDELMEM_OK, 0, 0, 0),
        &edcode::encode(name),
    )
}

/// `SM_GROUPDELMEM_FAIL(665)`（`PlayObject.Operate.cs:890/894/900`；`recog ∈ {-1,-2,-3}`），无尾。
#[must_use]
pub fn sm_groupdelmem_fail(recog: i32) -> Vec<u8> {
    gm_data_body(
        command_header(recog, messages::SM_GROUPDELMEM_FAIL, 0, 0, 0),
        &[],
    )
}

/// `SM_GROUPCANCEL(666)`（`PlayObject.cs:5150` 的立即路径 / `PlayObject.Message.cs:1848-1850` 的
/// `RM_GROUPCANCEL` 出队路径），无尾。
#[must_use]
pub fn sm_groupcancel() -> Vec<u8> {
    gm_data_body(command_header(0, messages::SM_GROUPCANCEL, 0, 0, 0), &[])
}

/// `SM_GROUPMEMBERS(667)`（`PlayObject.Base.cs:1361-1366`）：`SendDefMessage(667, 0,0,0,0, sSendMsg)`
/// ⇒ 尾 = `EncodeString("a/b/c/")`（逐成员 `ChrName + '/'` 拼接）。
#[must_use]
pub fn sm_groupmembers(members_text: &[u8]) -> Vec<u8> {
    gm_data_body(
        command_header(0, messages::SM_GROUPMEMBERS, 0, 0, 0),
        &edcode::encode(members_text),
    )
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

// ---------------------------------------------------------------------------
// R2 余条：出站编码器（`openmir2-outenc-r2`）——9 个 `RM_*` 观察者支的 `SM_*` 帧**编码层**。
//
// 判据来源：oracle `PlayObject.Message.cs` 的 9 个 `case`（`RM_SKELETON`/`RM_ALIVE`/
// `RM_DIGUP`/`RM_DIGDOWN`/`RM_FLYAXE`/`RM_LIGHTING`/`RM_SPACEMOVE_FIRE`(+`FIRE2`)/
// `RM_SPACEMOVE_SHOW`(+`SHOW2`)）。头一律 `MakeMessage(ident, ActorId, nParam1, nParam2, wParam)`
// 或按 oracle 显式覆盖；尾的编码口径逐条照 oracle（见各函数注释）。
//
// 观察者集合（`RM_*` → `SendRefMsg`，`BaseObject.Message.cs:385-445`）：发帧者 `Envir` 上
// ±`SendRefMsgRange`(=12) 格内 `Race == Play` 的可见玩家；非 Play 目标仅 `RM_STRUCK`/
// `RM_HEAR`/`RM_DEATH` 收（本组 9 条**都不在其中** ⇒ 只发玩家）。少数发帧点走 `SendMsg`
// （单点自发，如 `PlayObject.cs:3231` 的 `RM_SKELETON`）。
//
// **生产接线状态：全部「未接（登记）」**——发帧点都挂在怪物 AI 帧（挖地/掷斧/闪电/瞬移）或
// 玩家瞬移路径上，本仓 `world` 尚未承载这些行为；帧面先备好，待世界事件落地后按
// `downlink`/`pickwire` 同款接缝接线（**本片不动生产路径，默认行为不变**）。
//
// **D-9 客户端布局核查（逐条证「客户端是否要求特殊布局」，结论全部 = 不要求，用 oracle 布局）**：
// 冻结客户端 `MirClient@c61d912d`（`Source/Common/Grobal2.pas:39` `HIGHHP = True`）：
// - `SM_SKELETON`/`SM_ALIVE`：`ClMain.pas:10082/10089` 从**尾**解 `SizeOf(TCharDesc)=12B`
//   （`Grobal2.pas:3515`：`Feature/Status/StatusEx`），但**只消费 `desc.Feature`(→`m_nFeature`)
//   与 `desc.Status`(→`m_nState`，`Actor.pas:1877`)**，`desc.StatusEx` 不被消费。oracle 尾 =
//   `EncodePacket(CharDesc)` = `{Feature,Status}` **8B**（`ClientPackets/CharDesc.cs`）：被消费两字段
//   落在前 8B，客户端多读的 4B（未消费）为栈残值、无副作用。**与现有 `sm_death` 同形**
//   （`SM_DEATH` 客户端也解 12B、oracle 也发 8B，现网一致）。
// - `SM_DIGUP`：`ClMain.pas:10541` 解 `SizeOf(TMessageBodyWL)=16B`，消费 `lParam1`(feature)/
//   `lParam2`(status)/`lTag1`(event id) ⇒ oracle 16B 布局匹配。
// - `SM_FLYAXE`：`ClMain.pas:10013` 解 `SizeOf(TMessageBodyW)=8B` ⇒ oracle 8B 布局匹配。
// - `SM_LIGHTING`：`ClMain.pas:10027` 解 `SizeOf(TMessageBodyWL)=16B` ⇒ oracle 16B 布局匹配。
// - `SM_DIGDOWN`/`SM_SPACEMOVE_HIDE`(`2`)：客户端不读尾（`ClMain.pas:10631`/`:9841` 仅透传头字段）
//   ⇒ oracle 无尾匹配。
// - `SM_SPACEMOVE_SHOW`(`2`)：`ClMain.pas:9858` 解 `SizeOf(TCharDesc)=12B` 并消费 `desc.Feature`/
//   `desc.Status`；名串段按 `GetCodeMsgSize(12*4/3)=16` 与 desc 段切分。oracle 尾 = 8B 编码（11 字符）
//   `[+ EncodeString(Msg+'/'+color) 当 Msg 非空]`：被消费字段仍在前 8B ⇒ 正常；且 oracle 全部
//   发帧点 `sMsg` 恒 `""` ⇒ 名串段恒不出现（与 oracle 同形）。

/// `SM_SKELETON`(33)（`PlayObject.Message.cs:1654-1660` 的 `RM_SKELETON` 支）：
/// `MakeMessage(SM_SKELETON, ActorId, nParam1 /*=X*/, nParam2 /*=Y*/, wParam /*=Dir*/)`
/// `+ EDCode.EncodePacket(CharDesc{ Feature=GetFeature(this), Status=CharStatus })`。
///
/// 发帧点：`PlayObject.Operate.cs:736` 的 `SendRefMsg(RM_SKELETON, Dir, CurrX, CurrY, 0, "")`
/// 与 `PlayObject.cs:3231` 的 `SendMsg(baseObject, RM_SKELETON, Dir, CurrX, CurrY, 0)` ⇒ 头 `(X, Y, Dir)`。
#[must_use]
pub fn sm_skeleton(actor_id: i32, x: u16, y: u16, dir: u8, char_desc: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_SKELETON, x, y, u16::from(dir));
    gm_data_body(header, &edcode::encode(char_desc))
}

/// `SM_ALIVE`(27)（`PlayObject.Message.cs:1883-1889` 的 `RM_ALIVE` 支）：与 [`sm_skeleton`] 同形，
/// 仅 ident 不同（头 `(X, Y, Dir)` + 8B `CharDesc` 编码尾）。
///
/// 发帧点：`BaseObject.Base.cs:357`（复活）/`CastleDoor.cs:114`/`WallStructure.cs:39` 的
/// `SendRefMsg(RM_ALIVE, Dir, CurrX, CurrY, 0, "")`。
///
/// **orphan 登记（接线首批，`deferred` / registered_not_wired）**：本帧**无生产发射点**。
/// 注意口径——`crates/world` **已有**复活判定的纯谓词 `monster_due_for_revive`
///（`mongen.rs`，钉 `Death && CanReAlive && Invisible && MonGen!=null &&
/// (now-ReAliveTick) > GetMonstersZenTime`），故缺的**不是** `CanReAlive`/`ReAliveTick` 本身，
/// 而是**复活状态机与生产消费者**：无 `ReAliveEx` 的落地动作、无 `RM_ALIVE` 的生产消费
///（`CastleDoor`/`WallStructure` 的门/墙重生同理未建模）⇒ 登记为 `deferred`，不硬接无发射点的帧。
#[must_use]
pub fn sm_alive(actor_id: i32, x: u16, y: u16, dir: u8, char_desc: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_ALIVE, x, y, u16::from(dir));
    gm_data_body(header, &edcode::encode(char_desc))
}

/// `SM_DIGUP`(20)（`PlayObject.Message.cs:1890-1899` 的 `RM_DIGUP` 支）：
/// `MakeMessage(SM_DIGUP, ActorId, nParam1 /*=X*/, nParam2 /*=Y*/,
///              HUtil32.MakeWord((ushort)wParam /*=Dir*/, baseObject.Light))`
/// `+ EDCode.EncodePacket(MessageBodyWL{ Param1=GetFeature(this), Param2=CharStatus, Tag1, Tag2 })`。
///
/// ⚠️ oracle 先写 `digupMessage.Tag1 = processMsg.nParam3`、**立即覆盖** `Tag1 = 0`（`:1897-1898`）
/// ⇒ 尾 `Tag1` **恒 0**（怪行为照搬，不"修"）。
/// 发帧点：`CastleDoor.cs:53`/`DigOutZombi.cs:62`/`ElfMonster.cs:50`/`ElfWarriorMonster.cs:19,118`/
/// `ScultureMonster.cs:21`/`ScultureKingMonster.cs:29`/`StickMonster.cs:57`/`WallStructure.cs:79`/
/// `WhiteSkeleton.cs:36` 的 `SendRefMsg(RM_DIGUP, Dir, CurrX, CurrY, 0|nParam3, "")`。
#[must_use]
pub fn sm_digup(
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    light: u8,
    feature: i32,
    status: i32,
) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_DIGUP, x, y, make_word(dir, light));
    // `MessageBodyWL`：Tag1 被 oracle 覆盖为 0 ⇒ 尾 = `[Feature:i32][Status:i32][0][0]`。
    let mut wl = [0u8; 16];
    wl[0..4].copy_from_slice(&feature.to_le_bytes());
    wl[4..8].copy_from_slice(&status.to_le_bytes());
    gm_data_body(header, &edcode::encode(&wl))
}

/// `SM_DIGDOWN`(21)（`PlayObject.Message.cs:1900-1903` 的 `RM_DIGDOWN` 支）：
/// `MakeMessage(SM_DIGDOWN, ActorId, nParam1 /*=X*/, nParam2 /*=Y*/, 0)`，**无尾**
/// （`SendSocket(ClientMsg)`）。
///
/// 发帧点：`CastleDoor.cs:71`/`StickMonster.cs:62`/`ElfWarriorMonster.cs:148` 的
/// `SendRefMsg(RM_DIGDOWN, Dir, CurrX, CurrY, 0, "")`。
#[must_use]
pub fn sm_digdown(actor_id: i32, x: u16, y: u16) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_DIGDOWN, x, y, 0);
    gm_data_body(header, &[])
}

/// `SM_FLYAXE`(22)（`PlayObject.Message.cs:1904-1918` 的 `RM_FLYAXE` 支）：
/// `MakeMessage(SM_FLYAXE, ActorId, nParam1 /*=X*/, nParam2 /*=Y*/, wParam /*=Dir*/)`
/// `+ EDCode.EncodePacket(MessageBodyW{ Param1=target.CurrX, Param2=target.CurrY,
///                                      Tag1=LoWord(nParam3), Tag2=HiWord(nParam3) })`。
///
/// oracle 外层有 `if (SystemShare.ActorMgr.Get(processMsg.nParam3) != null)` 守卫——目标不存在
/// 则**不发帧**；本编码器只产帧面，守卫由调用方（世界的目标查找）负责。
/// 发帧点：`ArcherGuard.cs:36`/`DualAxeMonster.cs:29` 的
/// `SendRefMsg(RM_FLYAXE, Dir, CurrX, CurrY, targetObject.ActorId, "")`。
#[must_use]
pub fn sm_flyaxe(
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    target_x: u16,
    target_y: u16,
    target_id: i32,
) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_FLYAXE, x, y, u16::from(dir));
    let mut w = [0u8; 8];
    w[0..2].copy_from_slice(&target_x.to_le_bytes());
    w[2..4].copy_from_slice(&target_y.to_le_bytes());
    w[4..6].copy_from_slice(&lo_word(target_id).to_le_bytes());
    w[6..8].copy_from_slice(&hi_word(target_id).to_le_bytes());
    gm_data_body(header, &edcode::encode(&w))
}

/// `SM_LIGHTING`(23)（`PlayObject.Message.cs:1919-1930` 的 `RM_LIGHTING` 支）：
/// `MakeMessage(SM_LIGHTING, ActorId, nParam1 /*=X*/, nParam2 /*=Y*/, baseObject.Dir)`
/// `+ EDCode.EncodePacket(MessageBodyWL{ Param1=target.CurrX, Param2=target.CurrY,
///                                       Tag1=nParam3 /*=target ActorId*/, Tag2=processMsg.wParam })`。
///
/// ⚠️ 头 `Series` = **施法者自己的 `baseObject.Dir`**，`wParam` 落在尾 `Tag2`（不在头）——
/// 照 `:1927` 的实参序，别按常识把 `wParam` 放进 `Series`。
/// 发帧点：`Clone.cs:44`/`ElectronicScolpionMon.cs:30`/`LightingZombi.cs:22` 的
/// `SendRefMsg(RM_LIGHTING, 1, CurrX, CurrY, TargetCret.ActorId, "")`（`wParam` 恒 1）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_lighting(
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    target_x: i32,
    target_y: i32,
    target_id: i32,
    w_param: i32,
) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_LIGHTING, x, y, u16::from(dir));
    let mut wl = [0u8; 16];
    wl[0..4].copy_from_slice(&target_x.to_le_bytes());
    wl[4..8].copy_from_slice(&target_y.to_le_bytes());
    wl[8..12].copy_from_slice(&target_id.to_le_bytes());
    wl[12..16].copy_from_slice(&w_param.to_le_bytes());
    gm_data_body(header, &edcode::encode(&wl))
}

/// `SM_SPACEMOVE_HIDE`(800) / `SM_SPACEMOVE_HIDE2`(806)
/// （`PlayObject.Message.cs:1957-1966` 的 `RM_SPACEMOVE_FIRE`/`RM_SPACEMOVE_FIRE2` 支）：
/// `MakeMessage(SM_*, ActorId, 0, 0, 0)`，**无尾**。`ident` 由发帧点选
/// （`RM_SPACEMOVE_FIRE`→800、`RM_SPACEMOVE_FIRE2`→806）。
///
/// 发帧点：`PlayObject.Base.cs:2234/2241/2247/2308/3334`、`RunAway.cs:27`、`TeleMonster.cs:20`、
/// `Khazard.cs:28,42`、`MagicManager.cs:881` 的 `SendRefMsg(RM_SPACEMOVE_FIRE(|2), 0, 0, 0, 0, "")`。
#[must_use]
pub fn sm_spacemove_hide(ident: u16, actor_id: i32) -> Vec<u8> {
    let header = command_header(actor_id, ident, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_SPACEMOVE_SHOW`(801) / `SM_SPACEMOVE_SHOW2`(807)
/// （`PlayObject.Message.cs:1967-1985` 的 `RM_SPACEMOVE_SHOW`/`RM_SPACEMOVE_SHOW2` 支）：
/// `MakeMessage(SM_*, ActorId, nParam1 /*=X*/, nParam2 /*=Y*/,
///              HUtil32.MakeWord((ushort)wParam /*=Dir*/, baseObject.Light))`
/// `+ EDCode.EncodePacket(CharDesc{ Feature, Status })`
/// `[+ EDCode.EncodeString(processMsg.Msg + '/' + GetChrColor(this)) 当 Msg 非空]`。
///
/// 两段各自 `Encode` 后**直接相接**（前段 8B 使编码状态未复位——同 [`sm_turn`] 的既有形态，
/// 非本层引入；客户端按 `GetCodeMsgSize(16)` 切分，被消费字段仍在前 8B）。
/// `name_msg` = `(Msg + '/' + color)` 的**原始字节**（空则不加名串尾；oracle 发帧点恒传 `""`）。
/// 发帧点：`BaseObject.cs:1137/1141` 的 `SendRefMsg(RM_SPACEMOVE_SHOW(|2), Dir, CurrX, CurrY, 0, "")`。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn sm_spacemove_show(
    ident: u16,
    actor_id: i32,
    x: u16,
    y: u16,
    dir: u8,
    light: u8,
    char_desc: &[u8],
    name_msg: &[u8],
) -> Vec<u8> {
    let header = command_header(actor_id, ident, x, y, make_word(dir, light));
    let mut tail = edcode::encode(char_desc);
    if !name_msg.is_empty() {
        tail.extend_from_slice(&edcode::encode(name_msg));
    }
    gm_data_body(header, &tail)
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

    /// **M3⑨-3 帧面残余**：`SM_CHANGENAMECOLOR`(656) = `SendDefMessage(656, Recog=ActorId, Param=color, 0, 0)`
    /// （`PlayObject.Message.cs:1677-1679` 的 `RM_CHANGENAMECOLOR` 观察者支），**无尾**。
    ///
    /// 红检：把 `color` 从 `param` 挪到 `tag`/`series`、或给它加尾 ⇒ 本用例必红。
    #[test]
    fn sm_changenamecolor_header_only() {
        let body = sm_changenamecolor(0x7c30_3998, 0xAB);
        assert_eq!(i32at(&body, 0), 0x7c30_3998, "Recog = processMsg.ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_CHANGENAMECOLOR);
        assert_eq!(u16at12(&body, 6), 0xAB, "Param = GetChrColor(baseObject)");
        assert_eq!(u16at12(&body, 8), 0, "Tag");
        assert_eq!(u16at12(&body, 10), 0, "Series");
        assert_eq!(body.len(), 12, "SendDefMessage 5 参重载 ⇒ 无尾");
    }

    fn u16at12(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes([b[o], b[o + 1]])
    }

    /// **卡 `openmir2-client-e2e-enter-gaps` ①/② 真机根因（53 帧尾）**：冻结客户端
    /// `HIGHHP=True`（`Source/Common/Grobal2.pas:39`）的 `SM_HEALTHSPELLCHANGED` 支
    /// （`Source/MirClient/ClMain.pas:10220`）**不读 12B 头**，而是从**尾**解码
    /// `SizeOf(TCharDesc)`=12B（`Grobal2.pas:3515`）：`HP=desc.Feature / MP=desc.Status /
    /// MaxHP=desc.StatusEx`。oracle 的 header-only 帧（`PlayObject.Socket.cs:77-96`）会让客户端
    /// 解出全 0 ⇒ HUD `0/0`、`CanRun(HP>=10)` 拒绝跑（真机 verify2：进图 `18/19` 后一旦发 53 即归零）。
    ///
    /// 红检（改坏必红）：① 去掉尾（退回 [`sm_healthspellchanged`]）⇒ `desc.len() == 12` 必红；
    /// ② 把 `Feature`/`StatusEx` 写反（MaxHP 塞进 `Status`）⇒ 尾部回读断言必红。
    #[test]
    fn healthspellchanged_client_carries_tchardesc_tail() {
        let body = sm_healthspellchanged_client(0x0000_1234, 18, 15, 19);
        assert_eq!(i32at(&body, 0), 0x0000_1234, "Recog = processMsg.ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_HEALTHSPELLCHANGED);
        assert_eq!(u16at12(&body, 6), 18, "Param = HP");
        assert_eq!(u16at12(&body, 8), 15, "Tag = MP");
        assert_eq!(u16at12(&body, 10), 19, "Series = MaxHP");
        let desc = edcode::decode(&body[12..]);
        assert_eq!(
            desc.len(),
            12,
            "TCharDesc = 3 × Integer = 12B（HIGHHP 客户端要尾）"
        );
        assert_eq!(i32at(&desc, 0), 18, "Feature = HP");
        assert_eq!(i32at(&desc, 4), 15, "Status = MP");
        assert_eq!(i32at(&desc, 8), 19, "StatusEx = MaxHP");

        // oracle 路径仍 header-only；生产选路由 `client_layout_enabled()`（默认客户端布局）决定。
        assert_eq!(sm_healthspellchanged(0x0000_1234, 18, 15, 19).len(), 12);
        let chosen = sm_healthspellchanged_frame(0x0000_1234, 18, 15, 19);
        if client_layout_enabled() {
            assert_eq!(chosen, body, "默认（未设/其它）= 冻结客户端布局");
        } else {
            assert_eq!(
                chosen.len(),
                12,
                "MIR2_CLIENT_ABILITY_LAYOUT=0 ⇒ oracle header-only"
            );
        }
    }

    /// **HIGHHP 家族第三帧（31 `SM_STRUCK`，片「SM_STRUCK 客户端布局」）**：冻结客户端
    /// `ClMain.pas:10238` 的 `{$IF HIGHHP}` 支把 HP 读成 `Makelong(msg.param, msg.tag)`、
    /// MaxHP 读成尾 `MessageBodyWL.Param1` ⇒ 客户端布局 = 头 `(LoWord(HP), HiWord(HP), 伤害)`
    /// 与尾 `{Param1=MaxHP, Param2=status, Tag1=hitter, Tag2=mag}`；
    /// oracle [`sm_struck`] 是 `(HP, MaxHP, 伤害)` 与尾 `{Param1=feature, ...}`。
    ///
    /// 红检（改坏必红）：① `sm_struck_client` 的 `param/tag` 写回 `(hp, max_hp)` ⇒ 头断言必红；
    /// ② 尾 `Param1` 从 `MaxHP` 改回 `feature` ⇒ 尾回读断言必红；
    /// ③ `sm_struck_frame` 两支互换 ⇒ 末段选路断言必红。
    #[test]
    fn struck_client_splits_hp_and_puts_maxhp_in_tail() {
        let body = sm_struck_client(0x0000_1234, 19, 42, 7, 0x11, 0x22, false);
        assert_eq!(i32at(&body, 0), 0x0000_1234, "Recog = 受击者");
        assert_eq!(u16at12(&body, 4), messages::SM_STRUCK);
        assert_eq!(u16at12(&body, 6), 19, "Param = LoWord(HP)");
        assert_eq!(u16at12(&body, 8), 0, "Tag = HiWord(HP)（HP 是 u16 ⇒ 恒 0）");
        assert_eq!(u16at12(&body, 10), 7, "Series = 伤害");
        let wl = edcode::decode(&body[12..]);
        assert_eq!(wl.len(), 16, "尾 = Encode(16B MessageBodyWL)");
        assert_eq!(i32at(&wl, 0), 42, "Param1 = MaxHP（HIGHHP 支读这个）");
        assert_eq!(i32at(&wl, 4), 0x11, "Param2 = CharStatus");
        assert_eq!(i32at(&wl, 8), 0x22, "Tag1 = 命中者 ActorId");
        assert_eq!(i32at(&wl, 12), 0, "Tag2 = mag");

        // oracle 路径仍 = `(HP, MaxHP, 伤害)` + 尾 `Param1=feature`。
        let oracle = sm_struck(0x0000_1234, 19, 42, 7, 0x99, 0x11, 0x22, false);
        assert_eq!(u16at12(&oracle, 6), 19, "oracle Param = HP");
        assert_eq!(u16at12(&oracle, 8), 42, "oracle Tag = MaxHP");
        assert_eq!(
            i32at(&edcode::decode(&oracle[12..]), 0),
            0x99,
            "oracle Param1 = feature"
        );

        // 生产选路：默认（未设/其它）= 客户端布局；`MIR2_CLIENT_ABILITY_LAYOUT=0` = oracle 逐字节。
        let chosen = sm_struck_frame(0x0000_1234, 19, 42, 7, 0x99, 0x11, 0x22, false);
        if client_layout_enabled() {
            assert_eq!(chosen, body, "默认（未设/其它）= 冻结客户端布局");
        } else {
            assert_eq!(chosen, oracle, "=0 ⇒ oracle 逐字节");
        }
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

    /// 从金标准取**全部** `port=7200 && dir=s2c && ident=<id>` 帧的原始线缆字节（capture 顺序）。
    fn golden_s2c_ident_all(ident: u16) -> Vec<Vec<u8>> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tests/golden/session-20261010-c3-hook/capture.jsonl"
        );
        let capture =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("金标准不存在：{path}（{e}）"));
        let needle = format!("\"ident\":{ident},");
        let mut out = Vec::new();
        for line in capture.lines() {
            if !line.contains("\"dir\":\"s2c\"")
                || !line.contains("\"port\":7200")
                || !line.contains(&needle)
            {
                continue;
            }
            out.push(hex(between(line, "\"raw_hex\":\"", "\"")));
        }
        out
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
        let f = param & 0xFF;
        let b = param >> 8;
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
            magic_journal: Vec::new(),
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
                // `AntiMagic` = `Recog` 的完整 i32；其值域是 `ushort`（G-6）。
                u16::try_from(recog(&h12) & 0xFFFF).unwrap(),
                u16::from(u8::try_from(u16at(&h12, 6) & 0xFF).unwrap()),
                u16::from(u8::try_from(u16at(&h12, 6) >> 8).unwrap()),
                u16::from(u8::try_from(u16at(&h12, 8) & 0xFF).unwrap()),
                u16::from(u8::try_from(u16at(&h12, 8) >> 8).unwrap()),
                u16::from(u8::try_from(u16at(&h12, 10) & 0xFF).unwrap()),
                u16::from(u8::try_from(u16at(&h12, 10) >> 8).unwrap()),
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
            // 金标准人物无装备/无技能 ⇒ 621/211 的 oracle 门（尾为空不发）不产帧。
            use_items_tail: None,
            magic: None,
            // `RM_CHANGELIGHT`（capture seq 62）：`param=0`（= frame 50 的 `HiByte(series)`），
            // `tag=500`（`Config.nClientKey`）。
            changelight: Some((
                u8::try_from(u16at(&h2, 10) >> 8).unwrap(),
                crate::sessionwire::N_CLIENT_KEY_DEFAULT,
            )),
            // 金标准会话无行会：登录不发 750、队列不占堆位。
            guild_changename: None,
            // 金标准抓包是 `TestServer=TRUE` 部署（3 条 SysMsg + `PayMent==3` 的第 4 条）；
            // 正文缺输入 ⇒ 100 槽占位不产帧（本用例只比 ident 与已知帧字节）。
            login_sys_msgs: Default::default(),
            test_server: true,
            pay_ment_three: true,
        }
    }

    /// 顺序判据②（**模型 D**）：登录消息按 oracle 四叉堆出队；每条帧的**字节**仍逐字节等于金标准。
    ///
    /// 绝对序由 **2026-10-19 三次真机抓包**裁定（`docs/window-shrink-run-20261019.md` §6 F1 +
    /// `s2c-621b`/`s2c-621c`）：无装备无技能会话的前缀恒为 `…,1106,52,752,811,46`；
    /// 旧模型 C/C′ 漏掉了 `RM_TURN` 等队列条目，已按源码全量内容重建（见 `frames` 头注）。
    /// **capture 里出现的每一条帧字节不变**（按 ident 逐条比对金标准原始线缆）。
    #[test]
    fn enter_world_sequence_matches_model_d_and_golden_bytes() {
        let golden = golden_s2c(16);
        let built = golden_inputs().frames();
        let got_idents: Vec<u16> = built
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        // 模型 D：六条 + 654 的出队序 = 52, 752, 621, 211, 811, 46, 654
        // （金标准角色无装备/无技能 ⇒ 621/211 的门关，不产帧，但槽位仍参与堆布局）。
        // 2026-10-22 真机（`winplay0`）与 c3 金标准均含**第二条 42**（`RefShowName`，
        // `PlayObject.cs:1415`）：654 之后、100×4 之前。本 fixture 未给 SysMsg 正文 ⇒ 100 不产帧。
        assert_eq!(
            got_idents,
            vec![
                658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46, 10, 654, 42
            ],
            "模型 D 的逐帧 ident 序（真机实拍：52 在 752 前，四个 C# 会话一致）"
        );
        // capture 前 16 条 + 654：按 ident 取原始线缆，逐条比字节（顺序按模型 D，字节不变）。
        let mut capture_by_ident: Vec<(u16, &Vec<u8>)> =
            golden.iter().map(|(id, raw)| (*id, raw)).collect();
        let cl = golden_s2c_ident(messages::SM_CHANGELIGHT);
        capture_by_ident.push((messages::SM_CHANGELIGHT, &cl));
        assert_eq!(
            got_idents.len(),
            capture_by_ident.len() + 1,
            "模型 D 产出的帧数 = capture 可见帧数（17）+ 第二条 42（`RefShowName`，与首条同字节）"
        );
        for body in &built {
            let ident = u16::from_le_bytes([body[4], body[5]]);
            let want = capture_by_ident
                .iter()
                .find(|(id, _)| *id == ident)
                .unwrap_or_else(|| panic!("金标准里没有 ident={ident}"))
                .1;
            assert_eq!(
                &client_frame(body),
                want,
                "ident={ident} 必须逐字节等于金标准（长度 {}）",
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
    ///
    /// `built` 的次序按模型 D，故按 **ident** 取金标准帧，不按 capture 下标。
    #[test]
    fn remaining_eleven_encoders_match_golden_frames() {
        let f = golden_s2c(16);
        let built = golden_inputs().frames();
        for (capture_idx, ident) in [
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
            assert_eq!(f[capture_idx].0, ident, "金标准 #{capture_idx} 的 ident");
            let want = f
                .iter()
                .find(|(id, _)| *id == ident)
                .map(|(_, raw)| raw)
                .expect("金标准里有该 ident");
            let got = built
                .iter()
                .find(|b| u16::from_le_bytes([b[4], b[5]]) == ident)
                .expect("模型 D 产出里有该 ident");
            assert_eq!(
                &client_frame(got),
                want,
                "ident={ident} 的编码必须逐字节等于金标准"
            );
        }
    }

    /// **模型 D 全量顺序**（卡 `openmir2-login-frame-order`）：六条全部有输入时，
    /// 逐帧 ident 必须等于 oracle 四叉堆的出队序 `52, 752, 621, 211, 811, 46, 654`。
    ///
    /// 红检（三条牙，任一都让本用例红）：
    /// ① 去掉 `RM_LOGON` 入队（`frames()` 不再 `enqueue(LoginQueued::Logon, ...)`）⇒ 51/50/...
    ///    整组消失、654 也不再入队；
    /// ② 删掉承重槽位之一（实测：`Logon`、621/211 槽、`811/46`、`RM_USERNAME`、第四条 `SysMsg`、
    ///    或「两条 `RM_CHARSTATUSCHANGED` 全删」）——注意单独删 `RM_TURN` 或单独删一条 657 不改变
    ///    本用例的 High 组序（复验实测），不要把它写进红检清单
    ///    （模型 D 的堆布局依赖它们）⇒ High 组序改变；
    /// ③ 六条按可观测序手写（`52,752,621,211,811,46`）⇒ 与 oracle 的**入队**序不符
    ///    （源码序是 `52,752,811,46,621,211`）；本用例锁的是 `frames()` 的产物，
    ///    `smframe` 的构造用例另行钉死 621/211 出帧时的位置。
    #[test]
    fn login_frame_order_matches_model_d_queue() {
        let inputs = EnterWorldInputs {
            actor_id: 7,
            cur_x: 1,
            cur_y: 2,
            dir: 3,
            // 金标准角色无光源（`654.param=0`）⇒ 不触发 ③b 的第二条 RM_CHANGELIGHT。
            light: 0,
            day_bright: 5,
            notice: b"N".to_vec(),
            magic_journal: Vec::new(),
            map_file_name: Some(b"0".to_vec()),
            logon_body_wl: Some((1, 2, 3, 0)),
            feature: Some((1, 2)),
            attack_mode: Some(0),
            client_conf: Some(b"cfg".to_vec()),
            username: Some((7, 255, b"name".to_vec())),
            area_state: Some(0),
            map_description: Some((-1, b"desc".to_vec())),
            game_gold_name: Some((0, 0, b"gold".to_vec())),
            version_fail: Some((0, 0)),
            ability: Some((0, 0, 0, b"wabil".to_vec())),
            sub_ability: Some([0; 7]),
            adjust_bonus: Some((0, b"a".to_vec(), b"b".to_vec(), b"c".to_vec())),
            day_changing: Some((1, 0)),
            turn: Some((7, 1, 2, 3, 4, b"12345678".to_vec(), b"x".to_vec())),
            use_items_tail: Some(b"0/abc/".to_vec()),
            magic: Some((1, b"magic/".to_vec())),
            changelight: Some((0, 500)),
            guild_changename: None,
            login_sys_msgs: Default::default(),
            test_server: true,
            pay_ment_three: true,
        };
        let got: Vec<u16> = inputs
            .frames()
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        assert_eq!(
            got,
            vec![
                658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008,
                1106, // 直发 + RM_LOGON handler
                52, 752, 621, 211, 811,
                46, // 六条的堆序（High: 52,752,621,211；Normal: 811,46）
                10, 654, // 非队列 SM_TURN，随后 CHANGELIGHT
                42,  // `RefShowName` 的 RM_USERNAME 出队（165），与首条 42 同字节
            ],
            "模型 D：源码入队序 52,752,811,46,621,211 + 完整队列内容 → 出队 52,752,621,211,811,46"
        );
    }

    /// 三次真机抓包共用的 logon 输入（不含 `turn`：那三行的 ident 序里没有 `10`）。
    fn login_scenario_inputs(use_items: bool, magic: bool) -> EnterWorldInputs {
        EnterWorldInputs {
            actor_id: 7,
            cur_x: 1,
            cur_y: 2,
            dir: 3,
            // 金标准角色无光源（`654.param=0`）⇒ 不触发 ③b 的第二条 RM_CHANGELIGHT。
            light: 0,
            day_bright: 5,
            notice: b"N".to_vec(),
            magic_journal: Vec::new(),
            map_file_name: Some(b"0".to_vec()),
            logon_body_wl: Some((1, 2, 3, 0)),
            feature: Some((1, 2)),
            attack_mode: Some(0),
            client_conf: Some(b"cfg".to_vec()),
            username: Some((7, 255, b"name".to_vec())),
            area_state: Some(0),
            map_description: Some((-1, b"desc".to_vec())),
            game_gold_name: Some((0, 0, b"gold".to_vec())),
            version_fail: Some((0, 0)),
            ability: Some((0, 0, 0, b"wabil".to_vec())),
            sub_ability: Some([0; 7]),
            adjust_bonus: Some((0, b"a".to_vec(), b"b".to_vec(), b"c".to_vec())),
            day_changing: Some((1, 0)),
            turn: None,
            use_items_tail: use_items.then(|| b"0/abc/".to_vec()),
            magic: magic.then(|| (1_u16, b"magic/".to_vec())),
            changelight: Some((0, 500)),
            guild_changename: None,
            login_sys_msgs: Default::default(),
            test_server: true,
            pay_ment_three: true,
        }
    }

    /// 判据①（无装备无技能，`s2c`/`s2c-w03`/`s2c-w04`）：**至 654 为止**的登录队列序逐项等于真机；
    /// 654 之后的 `657,657`（CharStatus/HitSpeed 未建模）仍只占堆位，第二条 `42`（`RefShowName`）
    /// 已实现，`100×4` 需 `String.conf` 正文输入（本 fixture 未给 ⇒ 不产帧；见
    /// `login_sys_messages_match_csharp_capture_order_and_bytes`）。
    #[test]
    fn login_scenario_naked_matches_oracle_ident_sequence() {
        let got: Vec<u16> = login_scenario_inputs(false, false)
            .frames()
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        assert_eq!(
            got,
            vec![658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46, 654, 42],
            "无装备无技能：621/211 空尾门关（槽位仍占位），52,752,811,46,654"
        );
    }

    /// 判据②（有武器无有效技能，`s2c-621b`，另有 212 `SM_DELMAGIC` 走直发）：
    /// `52,752,621,811,46,654`。
    #[test]
    fn login_scenario_weapon_no_magic_matches_oracle_ident_sequence() {
        let got: Vec<u16> = login_scenario_inputs(true, false)
            .frames()
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        assert_eq!(
            got,
            vec![
                658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 621, 811, 46, 654,
                42
            ],
            "有武器无技能：621 出帧、211 空尾 ⇒ 52,752,621,811,46,654"
        );
    }

    /// 判据③（有武器 + 有效技能，`s2c-621c`）：`52,752,621,211,811,46,654`。
    #[test]
    fn login_scenario_weapon_with_magic_matches_oracle_ident_sequence() {
        let got: Vec<u16> = login_scenario_inputs(true, true)
            .frames()
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        assert_eq!(
            got,
            vec![
                658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 621, 211, 811, 46,
                654, 42
            ],
            "有武器有技能：621/211 都出帧 ⇒ 52,752,621,211,811,46,654,42（第二条 42 = RefShowName）"
        );
    }

    /// **G2b / T9**：有会登录的 750 必须落在 **46 之后、654 之前**，尾逐字节 = `EncodeString("会名/封号")`。
    ///
    /// 同时钉住评审员更正 2 的堆形：在正确入队位（六条之后、Username 之前）插入 p=61 的 750 会把
    /// `811/46` 的相对序**翻成 46 先于 811**（oracle 同一四叉堆的必然结果，不是本项目改序）。
    #[test]
    fn guild_login_750_between_46_and_654() {
        fn guild_750(name: &str, rank: &str) -> Vec<u8> {
            let header = command_header(0, messages::SM_CHANGEGUILDNAME, 0, 0, 0);
            gm_data_body(
                header,
                &edcode::encode(&crate::downlink::gbk(&format!("{name}/{rank}"))),
            )
        }
        let mut inputs = login_scenario_inputs(true, true);
        inputs.guild_changename = Some(guild_750("测试会", "行会成员"));
        let frames = inputs.frames();
        let ids: Vec<u16> = frames
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        let pos = |id: u16| {
            ids.iter()
                .position(|x| *x == id)
                .unwrap_or_else(|| panic!("缺帧 {id}: {ids:?}"))
        };
        assert!(
            pos(messages::SM_DAYCHANGING) < pos(messages::SM_CHANGEGUILDNAME),
            "750 必须在 46 之后: {ids:?}"
        );
        assert!(
            pos(messages::SM_CHANGEGUILDNAME) < pos(messages::SM_CHANGELIGHT),
            "750 必须在 654 之前: {ids:?}"
        );
        assert!(
            pos(messages::SM_ADJUST_BONUS) > pos(messages::SM_DAYCHANGING),
            "插入 750 后 811/46 翻转为 46 先于 811（oracle 堆形）: {ids:?}"
        );
        let wire = &frames[pos(messages::SM_CHANGEGUILDNAME)];
        assert_eq!(
            &wire[12..],
            &edcode::encode(&crate::downlink::gbk("测试会/行会成员"))[..],
            "750 尾 = EncodeString(会名/封号)"
        );
        // 评审员更正 2 的齿：优先级常量必须是 61；改成 0（到 46 前）或 >=154（到 654 后）
        // 上面两条顺序断言必红；改成 1 / 152 不改变出队序，故不是牙。
        assert_eq!(PRIORITY_RM_CHANGEGUILDNAME, 61);
    }

    /// **lane654 判据①（654 顺序）**：`frames_split` 的 `before` 段**不含** `654`，`after` 段以
    /// `654` 开头；15 帧入图前缀逐帧不变；插入会（优先级 61 的 `750`）后它落在 `after` 里、
    /// 仍在 `654` 之前。
    ///
    /// 红检（实测）：把 `frames_split_with_others` 的切分条件改成恒假 ⇒ `after` 为空、
    /// 654 落回 `before` ⇒ 本用例必红。
    #[test]
    fn frames_split_defers_changelight_until_after_appearance_slots() {
        let ids = |v: &Vec<Vec<u8>>| -> Vec<u16> {
            v.iter().map(|b| u16::from_le_bytes([b[4], b[5]])).collect()
        };
        let inputs = login_scenario_inputs(false, false);
        let (before, after) = inputs.frames_split();
        // 入图 15 帧前缀（2026-10-22 真机两侧完全一致）。
        assert_eq!(
            ids(&before),
            vec![658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46],
            "15 帧入图前缀不得回归"
        );
        assert!(
            !ids(&before).contains(&messages::SM_CHANGELIGHT),
            "654 必须离开登录即时段（它排在 RM_TURN 出现帧之后）"
        );
        assert_eq!(
            ids(&after),
            vec![messages::SM_CHANGELIGHT, messages::SM_USERNAME],
            "after 段 = 654 → RefShowName 的第二条 42"
        );

        // 有会：750（优先级 61）同属 after，且必须仍在 654 之前。
        let mut guild = login_scenario_inputs(true, true);
        {
            // 真 750 帧（12B 头，ident=10301）——`login_changename_750` 的字节形状。
            let mut f = command_header(0, messages::SM_CHANGEGUILDNAME, 0, 0, 0).to_vec();
            f.extend_from_slice(b"/");
            guild.guild_changename = Some(f);
        }
        let (_, after) = guild.frames_split();
        let a = ids(&after);
        let p750 = a
            .iter()
            .position(|x| *x == messages::SM_CHANGEGUILDNAME)
            .expect("750");
        let p654 = a
            .iter()
            .position(|x| *x == messages::SM_CHANGELIGHT)
            .expect("654");
        assert!(p750 < p654, "750(61) 必须先于 654(153): {a:?}");
    }

    /// **lane654 判据⑦（RecalcAbilitys 的 oldlight != Light 支）**：登录时 `Light != 0`
    /// （带发光装备 / 高等级基线）⇒ `RecalcAbilitys` 额外入队一条 `RM_CHANGELIGHT(153)`，
    /// 与 `RM_LOGON` 出队时那条**各发一帧** `654`（两条 Param 相同）。
    ///
    /// 红检（实测）：删掉 ③b 的 `if self.light != 0 { queue.enqueue(ChangeLight, ...) }`
    /// ⇒ 本用例的第二条 654 消失 ⇒ 必红。
    #[test]
    fn light_bearing_login_enqueues_extra_changelight_slot() {
        let mut inputs = login_scenario_inputs(false, false);
        inputs.light = 3;
        inputs.changelight = Some((3, crate::sessionwire::N_CLIENT_KEY_DEFAULT));
        let frames = inputs.frames();
        let cl: Vec<(&Vec<u8>, u16)> = frames
            .iter()
            .filter_map(|b| {
                let ident = u16::from_le_bytes([b[4], b[5]]);
                (ident == messages::SM_CHANGELIGHT).then_some((b, ident))
            })
            .collect();
        assert_eq!(
            cl.len(),
            2,
            "RecalcAbilitys 的 oldlight(0)!=Light(3) ⇒ 多一条 654"
        );
        for (body, _) in cl {
            let cm = mir2_protocol::frame::CommandMessage::from_bytes(body).unwrap();
            assert_eq!((cm.param, cm.tag), (3, 500), "两条 654 的 Param/Tag 相同");
        }
        // 无光源（金标准）⇒ 一条。
        let mut naked = login_scenario_inputs(false, false);
        naked.light = 0;
        naked.changelight = Some((0, crate::sessionwire::N_CLIENT_KEY_DEFAULT));
        assert_eq!(
            naked
                .frames()
                .iter()
                .filter(|b| u16::from_le_bytes([b[4], b[5]]) == messages::SM_CHANGELIGHT)
                .count(),
            1,
            "Light=0 ⇒ 只发 RM_LOGON 那条"
        );
    }

    /// **lane654 判据②（第二 42）**：`frames()` 里 `42` 恰出现两次，且两条**逐字节相同**
    /// （`RefShowName` 用与 `ClientQueryUserName` 相同的 `GetShowName()/GetChrColor`）。
    ///
    /// 红检（实测）：删掉 `LoginQueued::Username` 分支的 `out.push(sm_username(...))`
    /// ⇒ 第二次计数为 1 ⇒ 本用例必红。
    #[test]
    fn ref_show_name_emits_second_username_frame_byte_identical() {
        let frames = login_scenario_inputs(false, false).frames();
        let names: Vec<&Vec<u8>> = frames
            .iter()
            .filter(|b| u16::from_le_bytes([b[4], b[5]]) == messages::SM_USERNAME)
            .collect();
        assert_eq!(
            names.len(),
            2,
            "登录期 42 = ClientQueryUserName + RefShowName 两条"
        );
        assert_eq!(names[0], names[1], "两条 42 必须逐字节相同");
        // 位置：第二条在 654 之后（优先级 165 > 153）。
        let idents: Vec<u16> = frames
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        let first = idents.iter().position(|x| *x == 42).unwrap();
        let second = idents.iter().rposition(|x| *x == 42).unwrap();
        let cl = idents
            .iter()
            .position(|x| *x == messages::SM_CHANGELIGHT)
            .unwrap();
        assert!(first < cl && cl < second, "{idents:?}");
    }

    /// **lane654 判据③（100×4 条数与文本）**：LoginSysMsgs 四槽按 `String.conf` 正文产出
    /// `100×4`；四条正文的**集合**与金标准逐字节相同，`Param = make_word(GreenMsgFColor,
    /// GreenMsgBColor)`、`Series = 1`、`Recog = ActorId`。
    ///
    /// 入队序 = oracle 调用序（攻击模式 → CTRL-H 帮助 → `TestServer` 欢迎语 → `PayMent==3`）。
    /// **四条之间的相对序不在此钉**：它由观察者队列里**全部**同优先级（252）条目 + 堆形决定，
    /// 而本宿主不建模整条观察者队列（`msgqueue.rs` 模块头已登记 `not_verified`）。两次真机抓包
    /// 也不一致（c3：欢迎语→帮助→攻击模式→免费模式；2026-10-22：欢迎语→帮助→免费模式→攻击模式）。
    /// 本用例钉的是**条数 = 4**、**正文集合逐字节**、**字段形状**。
    ///
    /// 红检（实测）：把 `green` 的 `(f, b)` 对调 ⇒ `Param` 变 ⇒ 必红；把 `pay_ment_three`
    /// 入队门改成 `false`（只剩 3 条）⇒ 条数断言必红。
    #[test]
    fn login_sys_messages_match_csharp_capture_bytes_and_order() {
        let golden = golden_s2c_ident_all(messages::SM_SYSMESSAGE);
        assert_eq!(golden.len(), 4, "c3 金标准登录期 100 恰 4 条");
        let (head, _) = split_wire(&golden[0]);
        let actor = recog(&head);
        let param = u16at(&head, 6);
        let f = u8::try_from(param & 0xFF).unwrap();
        let b = u8::try_from(param >> 8).unwrap();

        let mut inputs = login_scenario_inputs(false, false);
        inputs.actor_id = actor;
        inputs.username = Some((actor, f, crate::downlink::gbk("名字")));
        inputs.login_sys_msgs = LoginSysMsgs {
            // 入队序 = oracle 调用序（部署件 `M2GameSvr/String.conf` 的 `[String]` 覆盖值）。
            attack_mode: Some(crate::downlink::gbk("[攻击模式: 全体攻击]")),
            change_help: Some(crate::downlink::gbk(
                "欢迎进入记忆传奇(月卡版)-官方网站：www.aresm2.com",
            )),
            notice: Some(crate::downlink::gbk(
                "唯一客服QQ：79311185 唯一担保QQ：79311185",
            )),
            free_play: Some(crate::downlink::gbk(
                "禁止使用任何外挂、加速、辅助、脱机，一经发现，封号封IP",
            )),
            green: (f, b),
        };
        // c3 抓包时同屏 16 个其他 actor（16 对 10/41）⇒ 16 个 RM_TURN(17) 槽位。
        let others = vec![OtherAppear::Turn; 16];
        let (_, after) = inputs.frames_split_with_others(&others);
        let idents: Vec<u16> = after
            .iter()
            .map(|x| u16::from_le_bytes([x[4], x[5]]))
            .collect();
        assert_eq!(idents[0], messages::SM_CHANGELIGHT, "after 以 654 开头");
        assert_eq!(idents[1], messages::SM_USERNAME, "654 之后是第二 42");
        assert_eq!(
            idents
                .iter()
                .filter(|x| **x == messages::SM_SYSMESSAGE)
                .count(),
            4,
            "100 恰 4 条: {idents:?}"
        );
        let mut got: Vec<Vec<u8>> = after
            .iter()
            .filter(|x| u16::from_le_bytes([x[4], x[5]]) == messages::SM_SYSMESSAGE)
            .map(|x| client_frame(x))
            .collect();
        let mut want: Vec<Vec<u8>> = golden.clone();
        got.sort();
        want.sort();
        assert_eq!(
            got, want,
            "四条 100 的 Param/Series/正文集合逐字节等于金标准"
        );

        // 字段形状（逐条）：Recog=ActorId、Series=1、Param=make_word(f,b)。
        for body in after
            .iter()
            .filter(|x| u16::from_le_bytes([x[4], x[5]]) == messages::SM_SYSMESSAGE)
        {
            let cm = mir2_protocol::frame::CommandMessage::from_bytes(body).unwrap();
            assert_eq!(cm.recog, actor);
            assert_eq!(cm.param, make_word16(u16::from(f), u16::from(b)));
            assert_eq!((cm.tag, cm.series), (0, 1));
        }
    }

    /// **lane654 判据④（出现槽数不影响 654 的切分位置与条数）**：本 tick 其他 actor 的
    /// `RM_TURN/RM_DEATH` 槽数从 0 到 24 变化时，
    /// ① `654` 恒为 `after` 第一条（切分点只由优先级 17 决定）；
    /// ② `after` 恒含 654 + 第二 42 + 100×4；
    /// ③ `100` 正文集合恒为四槽正文。
    ///
    /// 注：`before` 内 `811/46` 的相对序在 4 叉堆模型下随槽数翻转（N=0 与真机 811→46 一致；
    /// N≥1 模型给 46→811）。真实 oracle 的 `PriorityQueue` 是**二叉堆**（.NET 8），其前缀在
    /// 各槽数下恒为 `811→46`——本宿主登录前缀按 N=0 的既有模型下发，与两次真机抓包一致。
    /// 该差异只影响模型内部的 `before` 排序，不影响本用例钉的切分点/条数。
    ///
    /// 红检见判据①（切分条件恒假 ⇒ 654 落回 before）。
    #[test]
    fn appear_slot_count_does_not_move_split_or_drop_tail_frames() {
        for n in 0..=24usize {
            let mut inputs = login_scenario_inputs(true, true);
            inputs.login_sys_msgs = LoginSysMsgs {
                attack_mode: Some(b"A".to_vec()),
                change_help: Some(b"B".to_vec()),
                notice: Some(b"C".to_vec()),
                free_play: Some(b"D".to_vec()),
                green: (0xDB, 0xFF),
            };
            let (before, after) = inputs.frames_split_with_others(&vec![OtherAppear::Turn; n]);
            assert!(
                !before
                    .iter()
                    .any(|x| u16::from_le_bytes([x[4], x[5]]) == messages::SM_CHANGELIGHT),
                "槽数 {n}：654 不得留在 before"
            );
            let tail: Vec<u16> = after
                .iter()
                .map(|x| u16::from_le_bytes([x[4], x[5]]))
                .collect();
            assert_eq!(tail[0], messages::SM_CHANGELIGHT, "槽数 {n}");
            assert_eq!(tail[1], messages::SM_USERNAME, "槽数 {n}");
            assert_eq!(
                tail.iter()
                    .filter(|x| **x == messages::SM_SYSMESSAGE)
                    .count(),
                4,
                "槽数 {n}"
            );
            let mut bodies: Vec<String> = after
                .iter()
                .filter(|x| u16::from_le_bytes([x[4], x[5]]) == messages::SM_SYSMESSAGE)
                .map(|x| String::from_utf8(edcode::decode(&x[12..])).unwrap())
                .collect();
            bodies.sort();
            assert_eq!(bodies, vec!["A", "B", "C", "D"], "槽数 {n}");
        }
    }

    /// **G2b / T9 回归**：无会（`None`）时**不占堆位、不产 750**，整串帧与改动前逐字节相等
    /// （与 `login_scenario_weapon_with_magic_matches_oracle_ident_sequence` 的期望同源）。
    #[test]
    fn guild_login_without_guild_keeps_frame_stream_byte_identical() {
        let inputs = login_scenario_inputs(true, true);
        assert!(inputs.guild_changename.is_none());
        let frames = inputs.frames();
        let ids: Vec<u16> = frames
            .iter()
            .map(|b| u16::from_le_bytes([b[4], b[5]]))
            .collect();
        assert!(
            !ids.contains(&messages::SM_CHANGEGUILDNAME),
            "无会不得发 750"
        );
        assert_eq!(
            ids,
            vec![
                658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 621, 211, 811, 46,
                654, 42
            ],
            "无会路径的帧序必须一字不改（654 后跟随 `RefShowName` 的第二条 42）"
        );
        // 金标准里 654 之前的帧序也未被 750 模型影响。
        assert_eq!(ids.len(), 19);
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

    /// **⑨-2 第②片**：`SM_SPELL`（`PlayObject.Message.cs:1454-1459`）与 `SM_MAGICFIRE`
    /// （`:1788-1793`）的字段位与尾，照 oracle `MakeMessage`/`SendSocket` 的实参序。
    #[test]
    fn spell_and_magicfire_field_layout() {
        use mir2_protocol::frame::CommandMessage;
        let body = sm_spell(0x1122_3344, 0x0102, 0x0304, 7, 30);
        let cm = CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 0x1122_3344, "ActorId 在 Recog");
        assert_eq!(cm.ident, messages::SM_SPELL);
        assert_eq!(cm.param, 0x0102, "nTargetX 在 Param");
        assert_eq!(cm.tag, 0x0304, "nTargetY 在 Tag");
        assert_eq!(cm.series, 7, "Effect 在 Series");
        assert_eq!(
            &body[12..],
            b"30",
            "尾 = MagicId 的 ASCII 原文（`SendSocket(ClientMsg, nParam3.ToString())` 不再 Encode）"
        );

        let body = sm_magicfire(9, 0x1112, 0x1314, 5, 6, 0x0000_0021);
        let cm = CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 9, "ActorId 在 Recog");
        assert_eq!(cm.ident, messages::SM_MAGICFIRE);
        assert_eq!(cm.param, 0x1112, "nTargetX=LoWord(nParam2) 在 Param");
        assert_eq!(cm.tag, 0x1314, "nTargetY=HiWord(nParam2) 在 Tag");
        assert_eq!(
            cm.series, 0x0605,
            "MakeWord(EffectType=5, Effect=6) 在 Series"
        );
        assert_eq!(
            &body[12..],
            edcode::encode(&0x21_i32.to_le_bytes()),
            "尾 = Encode(4B LE targetActorId)"
        );
    }

    /// **⑨-2 第②片**：`SM_MAGIC_LVEXP` 与 `SM_DURACHANGE`（`PlayObject.Message.cs:1800-1805`）
    /// 的字段位（无尾）。
    #[test]
    fn magic_lvexp_and_durachange_field_layout() {
        use mir2_protocol::frame::CommandMessage;
        let body = sm_magic_lvexp(30, 2, 0x0001_0002);
        let cm = CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 30, "MagicId=nParam1 在 Recog");
        assert_eq!(cm.ident, messages::SM_MAGIC_LVEXP);
        assert_eq!(cm.param, 2, "Level=nParam2 在 Param");
        assert_eq!(cm.tag, 0x0002, "LoWord(TranPoint)");
        assert_eq!(cm.series, 0x0001, "HiWord(TranPoint)");
        assert!(body[12..].is_empty(), "无尾");

        let body = sm_durachange(0x00C8, 5, 0x2710);
        let cm = CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 0x00C8, "Dura=nParam1 在 Recog");
        assert_eq!(cm.ident, messages::SM_DURACHANGE);
        assert_eq!(cm.param, 5, "ItemLocation=wParam 在 Param");
        assert_eq!(cm.tag, 0x2710, "LoWord(DuraMax)");
        assert_eq!(cm.series, 0, "HiWord(DuraMax)");
        assert!(body[12..].is_empty(), "无尾");
    }

    /// **⑨-2 护身符扣光帧面**：`SM_DELITEM(202)` 的字段位与「尾原样透传」
    /// （`PlayObject.Base.cs:1648-1665` 的 `MakeMessage(SM_DELITEM, ActorId, 0, 0, 1)` + `SendSocket`）。
    #[test]
    fn delitem_field_layout_and_tail_passthrough() {
        use mir2_protocol::frame::CommandMessage;
        let tail = [0x11, 0x22, 0x33];
        let body = sm_delitem(0x0000_0007, &tail);
        let cm = CommandMessage::from_bytes(&body).unwrap();
        assert_eq!(cm.recog, 7, "ActorId 在 Recog");
        assert_eq!(cm.ident, messages::SM_DELITEM);
        assert_eq!((cm.param, cm.tag, cm.series), (0, 0, 1), "Series=1");
        // 尾 = `EDCode.EncodeBuffer(clientItem)`，本层原样透传（不在本函数编码）。
        assert_eq!(&body[12..], &tail, "尾原样透传");
    }

    /// **T4/R2 出站编码器**：`SM_MOVEFAIL`(28) 的 **oracle 逐字节布局**
    /// （`PlayObject.Message.cs:1468-1474`）：头 `(Recog=ActorId, Param=CurrX, Tag=CurrY, Series=Dir)`
    /// + 尾 `Encode(8B CharDesc{Feature,Status})`。
    ///
    /// 红检（改坏必红）：① `dir` 从 `Series` 挪到 `Param`/`Tag` ⇒ 头断言必红；
    /// ② 尾从 8B 改 12B ⇒ `desc.len() == 8` 必红；③ `Feature`/`Status` 写反 ⇒ 尾回读必红。
    #[test]
    fn sm_movefail_oracle_layout_is_8b_chardesc() {
        let body = sm_movefail(0x0000_1234, 287, 618, 5, 0x0002_00AB, 0x0000_0077);
        assert_eq!(i32at(&body, 0), 0x0000_1234, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_MOVEFAIL);
        assert_eq!(u16at12(&body, 6), 287, "Param = CurrX");
        assert_eq!(u16at12(&body, 8), 618, "Tag = CurrY");
        assert_eq!(u16at12(&body, 10), 5, "Series = Dir");
        let desc = edcode::decode(&body[12..]);
        assert_eq!(desc.len(), 8, "oracle 尾 = Encode(8B CharDesc)");
        assert_eq!(i32at(&desc, 0), 0x0002_00AB, "Feature = GetFeatureToLong()");
        assert_eq!(i32at(&desc, 4), 0x0000_0077, "Status = CharStatus");
    }

    /// **T4/R2 出站编码器**：`SM_MOVEFAIL`(28) 的 **冻结客户端布局**——尾 `Encode(12B TCharDesc)`。
    /// 冻结客户端 `ClMain.pas:9944` `DecodeBuffer(body, @desc, SizeOf(TCharDesc))` ⇒ 12B
    /// （`Grobal2.pas:3515`：`Feature/Status/StatusEx`）。
    ///
    /// 红检（改坏必红）：① 尾退回 8B ⇒ `desc.len() == 12` 必红；
    /// ② `status_ex` 塞进 `Status` 位 ⇒ 尾回读断言必红；③ `sm_movefail_frame` 两支互换 ⇒ 见路由用例。
    #[test]
    fn sm_movefail_client_carries_tchardesc_tail() {
        let body = sm_movefail_client(0x0000_1234, 287, 618, 5, 0x0002_00AB, 0x0000_0077, 0x99);
        assert_eq!(i32at(&body, 0), 0x0000_1234, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_MOVEFAIL);
        assert_eq!(u16at12(&body, 6), 287, "Param = CurrX");
        assert_eq!(u16at12(&body, 8), 618, "Tag = CurrY");
        assert_eq!(u16at12(&body, 10), 5, "Series = Dir");
        let desc = edcode::decode(&body[12..]);
        assert_eq!(desc.len(), 12, "TCharDesc = 3 × Integer = 12B");
        assert_eq!(i32at(&desc, 0), 0x0002_00AB, "Feature");
        assert_eq!(i32at(&desc, 4), 0x0000_0077, "Status");
        assert_eq!(i32at(&desc, 8), 0x99, "StatusEx");

        // oracle 路径仍 8B（并存，不改）。
        assert_eq!(
            sm_movefail(0x0000_1234, 287, 618, 5, 0x0002_00AB, 0x0000_0077).len(),
            12 + edcode::encoded_len(8)
        );
    }

    /// 测试期临时清空/改写一个进程 env 变量，`Drop` 时恢复
    /// （断言失败 panic 也不污染同进程其它用例；仅用于本文件**唯一**读该 env 的用例）。
    struct EnvGuard {
        key: &'static str,
        saved: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, val: Option<&str>) -> Self {
            let saved = std::env::var_os(key);
            match val {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
            Self { key, saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.saved.take() {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }

    /// **T4/R2 默认路由牙（v4 阻塞项；v5 修对照 v2 的 Important）**：
    /// 28 的生产默认 **必须 = oracle 8B**（28 不在 D-9 家族）。
    ///
    /// 三向：
    /// - 纯接缝 [`movefail_route`]：`false ⇒ oracle`、`true ⇒ client`（**对调两支 ⇒ 必红**）；
    /// - 纯解析 [`movefail_layout_flag`]：`None`（开关未设）⇒ `false`（**默认翻成 true ⇒ 必红**，
    ///   不读 env、不受测试线程并发影响）；
    /// - 生产入口 [`sm_movefail_frame`]：**清空 env** 后**无条件**断言字节 == oracle golden
    ///   （**把开关默认改成 true ⇒ 必红**）。
    ///
    /// # v5 修（对抗验证 v2 判 Important）
    ///
    /// 旧版把「生产入口 == oracle」断言门控在 `if !client_movefail_layout_enabled()` 里 ——
    /// 正好门控在它**本要防的那个开关自己身上**：默认被改成 `true` 时该 `if` 直接跳过、用例静默变绿
    /// （实测变异 MB1 全绿）。现改为：断言**不依赖开关函数**——先**清空该 env** 再无条件断言生产入口字节。
    #[test]
    fn sm_movefail_frame_defaults_to_oracle_layout() {
        let oracle = sm_movefail(0x0000_1234, 287, 618, 5, 0x0002_00AB, 0x0000_0077);
        let client = sm_movefail_client(0x0000_1234, 287, 618, 5, 0x0002_00AB, 0x0000_0077, 0x99);
        assert_ne!(oracle, client, "两支布局必须可区分（否则判据无牙）");
        assert_ne!(oracle.len(), client.len(), "8B vs 12B 尾长可区分");

        // 纯接缝（不读 env ⇒ 确定性）：分支对调/写错必红。
        assert_eq!(
            movefail_route(
                false,
                0x0000_1234,
                287,
                618,
                5,
                0x0002_00AB,
                0x0000_0077,
                0x99
            ),
            oracle,
            "client_layout=false ⇒ oracle 8B"
        );
        assert_eq!(
            movefail_route(
                true,
                0x0000_1234,
                287,
                618,
                5,
                0x0002_00AB,
                0x0000_0077,
                0x99
            ),
            client,
            "client_layout=true ⇒ client 12B"
        );

        // 纯解析：开关未设（None）⇒ 默认关（oracle）。把默认翻成 true ⇒ 必红（无 env、无并发）。
        assert!(
            !movefail_layout_flag(None),
            "28 开关未设 ⇒ 默认关（oracle）"
        );
        assert!(movefail_layout_flag(Some("1")), "显式 1 ⇒ 开");

        // 生产入口 · 默认向：**清空 env** 后**无条件**断言 == oracle golden
        // （不再用 `if !client_movefail_layout_enabled()` 门控 ⇒ 开关默认改成 true 时静默跳过）。
        {
            let _g = EnvGuard::set("MIR2_CLIENT_MOVEFAIL_LAYOUT", None);
            assert_eq!(
                sm_movefail_frame(0x0000_1234, 287, 618, 5, 0x0002_00AB, 0x0000_0077, 0x99),
                oracle,
                "28 生产默认（开关未设）必须 = oracle 8B（把开关默认改成 true ⇒ 必红）"
            );
        }

        // 生产入口 · 反向：显式打开开关 ⇒ 走 client 12B（证明开关活、两支经入口都可达）。
        {
            let _g = EnvGuard::set("MIR2_CLIENT_MOVEFAIL_LAYOUT", Some("1"));
            assert_eq!(
                sm_movefail_frame(0x0000_1234, 287, 618, 5, 0x0002_00AB, 0x0000_0077, 0x99),
                client,
                "开关=1 ⇒ 生产入口走 client 12B"
            );
        }
    }

    /// **T4/R2 出站编码器**：`SM_DISAPPEAR`(30) 与金标准 `c-line-baseline-c2-20261010`
    /// **帧 30 逐字节相等**（15 条样本之一的真字节，含 `#`/`!`）。
    /// 头 = `(Recog=消失对象 ActorId, Param/Tag/Series=0)`，无尾。
    ///
    /// 红检（改坏必红）：① 给 `Param` 塞值 ⇒ 逐字节必红；② 加任意尾 ⇒ 逐字节必红；
    /// ③ `Recog` 写成 0 或写反 ⇒ 逐字节必红。
    #[test]
    fn sm_disappear_matches_golden_frame_30() {
        // 金标准帧 30（`tests/golden/c-line-baseline-c2-20261010.jsonl`，seq=74，ident=30）：
        // recog=1558452193 / param=tag=series=0 / body_len=0。
        let body = sm_disappear(1558452193);
        assert_eq!(i32at(&body, 0), 1558452193, "Recog = 消失对象 ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_DISAPPEAR);
        assert_eq!(&body[6..12], &[0u8; 6], "Param/Tag/Series 全 0");
        assert_eq!(body.len(), 12, "无尾");
        assert_eq!(
            client_frame(&body),
            hex("234d6b445a786a685c6464686b6464686b21"),
            "客户端线缆 = '#' + Encode(12B 头) + '' + '!'（金标准 raw_hex 原样）"
        );
    }

    /// **T4/R2 出站编码器**：`SM_CLEAROBJECTS`(633) 头-only
    /// （`PlayObject.Message.cs:1765-1773`）：`(Recog=0, Param=0, Tag=0, Series=0)`，无尾。
    ///
    /// 红检（改坏必红）：① `Recog` 从 0 改成非 0 ⇒ 断言必红；② 尾非空 ⇒ 断言必红。
    #[test]
    fn sm_clearobjects_is_header_only_all_zero() {
        let body = sm_clearobjects();
        assert_eq!(body.len(), 12, "SendDefMessage 5 参重载 ⇒ 无尾");
        assert_eq!(i32at(&body, 0), 0, "Recog = 0");
        assert_eq!(u16at12(&body, 4), messages::SM_CLEAROBJECTS);
        assert_eq!(&body[6..12], &[0u8; 6], "Param/Tag/Series 全 0");
        let wire = client_frame(&body);
        assert_eq!(wire.first(), Some(&b'#'));
        assert_eq!(wire.last(), Some(&b'!'));
        assert_eq!(wire.len(), 1 + edcode::encoded_len(12) + 1);
    }

    /// 反向牙：`SM_DISAPPEAR` 若把 `Recog` 写错（0 或写死），金标准帧 30 的逐字节比对必红；
    /// 此处显式构造「错的帧」证明该断言有分辨力（无牙的判据不算判据）。
    #[test]
    fn sm_disappear_recog_must_be_target_not_zero() {
        let good = client_frame(&sm_disappear(1558452193));
        let wrong = client_frame(&sm_disappear(0));
        assert_ne!(
            good, wrong,
            "Recog=0 与 Recog=目标 必须可区分（否则判据无牙）"
        );
        assert_eq!(
            good,
            hex("234d6b445a786a685c6464686b6464686b21"),
            "仅 Recog=目标 的帧等于金标准"
        );
    }
    // ---- R2 余条：出站编码器 golden（逐字节，自算） ----
    //
    // 本组 golden **不是**取自被测函数：由一份**独立**实现（按 oracle C# `EncryptUtil.Encode`，
    // `src/OpenMir2/EncryptUtil.cs:184-216`，与 `edcode.rs` 同源而异写）重算出的**完整客户端线缆字节**
    // （`'#' + Encode(头12B) + 尾 + '!'`）。该独立生成器已用**真实 C3 金标准** 3 帧反证逐字节一致
    // （`SM_NEWMAP`(51) / `SM_LOGON`(50) / `SM_FEATURECHANGED`(41)），见 `golden_generator_selfcheck`。
    // 会话 `ActorId` 同本文件其余 golden：`0x7c30_3998`。
    const G_R2_ACTOR_ID: i32 = 0x7c30_3998;
    /// `CharDesc{ Feature=0x0002_0000, Status=0x0000_0005 }` 的 8B 原始字节（尾编码前）。
    const R2_DESC8: &str = "0000020005000000";

    const G_R2_SKELETON: &str = "2348615861705d685f6b65426f6664686a64646a6b6465686a64644b21";
    const G_R2_ALIVE: &str = "2348615861706b685d6b65426f6665686b64646a6b6465686a64644b21";
    const G_R2_DIGUP: &str =
        "23486158617068685e5c65783f65666b6b64646a6b6469686b6464686b6464686b6464686b643f21";
    const G_R2_DIGDOWN: &str = "23486158617069685e5c65783f6564686b21";
    const G_R2_FLYAXE: &str = "2348615861706a685e6c6460576466686a6d6461576467686b66644b21";
    const G_R2_LIGHTING: &str =
        "2348615861706b685e6c6460576467686b6d6468676455686964646b6b6466686b6564686b643f21";
    const G_R2_SPACEMOVE_HIDE: &str = "2348615861705c6b5f6464686b6464686b21";
    const G_R2_SPACEMOVE_HIDE2: &str = "2348615861705e6b5e6464686b6464686b21";
    const G_R2_SPACEMOVE_SHOW: &str = "2348615861705d6b5f6b65426f6664656b64646a6b6465686a64644b21";
    const G_R2_SPACEMOVE_SHOW2: &str = "2348615861705f6b5e6b65426f6664656b64646a6b6465686a64644b21";
    const G_R2_SPACEMOVE_SHOW_NAME: &str =
        "2348615861705d6b5f6b65426f6664656b64646a6b6465686a64644b554e4c3e5a5f556821";

    /// 独立生成器自证：用本组 golden 同款算法重算 3 条**真实** C3 金标准帧（`capture.jsonl` 的
    /// `SM_NEWMAP`(51)/`SM_LOGON`(50)/`SM_FEATURECHANGED`(41)）逐字节一致 ⇒ 本组 golden 可信。
    #[test]
    fn golden_generator_selfcheck() {
        let g = |ident, param, tag, series, tail: &[u8]| {
            let mut body = command_header(G_R2_ACTOR_ID, ident, param, tag, series).to_vec();
            body.extend_from_slice(tail);
            client_frame(&body)
        };
        assert_eq!(g(51, 287, 618, 0, &edcode::encode(b"0")), hex(G_NEWMAP));
        let wl: [u8; 16] = hex("00000200000000000000000000000000").try_into().unwrap();
        assert_eq!(
            g(50, 287, 618, make_word(0, 0), &edcode::encode(&wl)),
            hex(G_LOGON)
        );
        assert_eq!(
            g(41, 0x0000, 0x0002, 0, &[]),
            hex(G_FEATURECHANGED),
            "SM_FEATURECHANGED = 头(ActorId,0x0000,0x0002,0) 无尾"
        );
    }

    /// `SM_SKELETON`(33)：头 `(Recog=ActorId, Param=X, Tag=Y, Series=Dir)` + 尾 `Encode(8B CharDesc)`。
    ///
    /// 红检：① 头 `Series` 从 `dir` 改 `0`/`x`/`y` ⇒ 头断言必红；② 尾从 `Encode(8B)` 改 header-only ⇒ 尾断言必红。
    #[test]
    fn sm_skeleton_frame() {
        let desc = hex(R2_DESC8);
        let body = sm_skeleton(G_R2_ACTOR_ID, 287, 618, 4, &desc);
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_SKELETON);
        assert_eq!(u16at12(&body, 6), 287, "Param = X = nParam1");
        assert_eq!(u16at12(&body, 8), 618, "Tag = Y = nParam2");
        assert_eq!(u16at12(&body, 10), 4, "Series = Dir = wParam");
        assert_eq!(
            edcode::decode(&body[12..]),
            desc,
            "尾 = Encode(8B CharDesc)"
        );
        assert_eq!(client_frame(&body), hex(G_R2_SKELETON));
    }

    /// `SM_ALIVE`(27)：与 `SM_SKELETON` 同形，仅 ident 不同（`Series = Dir`）。
    #[test]
    fn sm_alive_frame() {
        let desc = hex(R2_DESC8);
        let body = sm_alive(G_R2_ACTOR_ID, 287, 618, 1, &desc);
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_ALIVE);
        assert_eq!(u16at12(&body, 6), 287, "Param = X");
        assert_eq!(u16at12(&body, 8), 618, "Tag = Y");
        assert_eq!(u16at12(&body, 10), 1, "Series = Dir");
        assert_eq!(
            edcode::decode(&body[12..]),
            desc,
            "尾 = Encode(8B CharDesc)"
        );
        assert_eq!(client_frame(&body), hex(G_R2_ALIVE));
    }

    /// `SM_DIGUP`(20)：头 `(ActorId, X, Y, MakeWord(Dir, Light))` +
    /// 尾 `Encode(16B MessageBodyWL{ Feature, Status, 0, 0 })`——**`Tag1` 恒 0**（oracle `:1897-1898` 覆盖）。
    ///
    /// 红检：① 尾 `Tag1`（第 8..12B）写成 `processMsg.nParam3`（非 0）⇒ 尾断言必红；
    /// ② 头 `Series` 漏 `Light`（只放 `dir`）⇒ 头断言必红。
    #[test]
    fn sm_digup_frame_has_zero_tag1() {
        let body = sm_digup(G_R2_ACTOR_ID, 300, 400, 2, 3, 0x0002_0000, 0x0000_0011);
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_DIGUP);
        assert_eq!(u16at12(&body, 6), 300, "Param = X");
        assert_eq!(u16at12(&body, 8), 400, "Tag = Y");
        assert_eq!(
            u16at12(&body, 10),
            make_word(2, 3),
            "Series = MakeWord(Dir, Light)"
        );
        let wl = edcode::decode(&body[12..]);
        assert_eq!(wl.len(), 16, "尾 = Encode(16B MessageBodyWL)");
        assert_eq!(i32at(&wl, 0), 0x0002_0000, "Param1 = GetFeature()");
        assert_eq!(i32at(&wl, 4), 0x0000_0011, "Param2 = CharStatus");
        assert_eq!(i32at(&wl, 8), 0, "Tag1 恒 0（oracle 第二赋值覆盖）");
        assert_eq!(i32at(&wl, 12), 0, "Tag2 恒 0");
        assert_eq!(client_frame(&body), hex(G_R2_DIGUP));
    }

    /// `SM_DIGDOWN`(21)：头 `(ActorId, X, Y, 0)`，**无尾**。
    ///
    /// 红检：给它加尾、或把 `Series` 置非 0 ⇒ 必红。
    #[test]
    fn sm_digdown_frame_header_only() {
        let body = sm_digdown(G_R2_ACTOR_ID, 300, 400);
        assert_eq!(body.len(), 12, "SendSocket(ClientMsg) 无尾 ⇒ 恰好 12B");
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_DIGDOWN);
        assert_eq!(u16at12(&body, 6), 300, "Param = X");
        assert_eq!(u16at12(&body, 8), 400, "Tag = Y");
        assert_eq!(u16at12(&body, 10), 0, "Series = 0");
        assert_eq!(client_frame(&body), hex(G_R2_DIGDOWN));
    }

    /// `SM_FLYAXE`(22)：头 `(ActorId, X, Y, Dir)` +
    /// 尾 `Encode(8B MessageBodyW{ targetX, targetY, LoWord(targetId), HiWord(targetId) })`。
    ///
    /// 红检：把尾 `Tag1/Tag2` 互换、或头 `Series` 从 `dir` 改别的 ⇒ 必红。
    #[test]
    fn sm_flyaxe_frame() {
        let tid = 0x0002_0003;
        let body = sm_flyaxe(G_R2_ACTOR_ID, 100, 200, 6, 101, 201, tid);
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_FLYAXE);
        assert_eq!(u16at12(&body, 6), 100, "Param = X");
        assert_eq!(u16at12(&body, 8), 200, "Tag = Y");
        assert_eq!(u16at12(&body, 10), 6, "Series = Dir");
        let w = edcode::decode(&body[12..]);
        assert_eq!(w.len(), 8, "尾 = Encode(8B MessageBodyW)");
        assert_eq!(
            u16::from_le_bytes([w[0], w[1]]),
            101,
            "Param1 = target.CurrX"
        );
        assert_eq!(
            u16::from_le_bytes([w[2], w[3]]),
            201,
            "Param2 = target.CurrY"
        );
        assert_eq!(
            u16::from_le_bytes([w[4], w[5]]),
            0x0003,
            "Tag1 = LoWord(targetId)"
        );
        assert_eq!(
            u16::from_le_bytes([w[6], w[7]]),
            0x0002,
            "Tag2 = HiWord(targetId)"
        );
        assert_eq!(client_frame(&body), hex(G_R2_FLYAXE));
    }

    /// `SM_LIGHTING`(23)：头 `Series = baseObject.Dir`（**施法者的 Dir**），
    /// 尾 `Encode(16B MessageBodyWL{ targetX, targetY, targetId, wParam })`。
    ///
    /// 红检：① 头 `Series` 写 `w_param`（放错位）⇒ 头断言必红；② 尾 `Tag2` 写 `dir` ⇒ 尾断言必红。
    #[test]
    fn sm_lighting_series_is_caster_dir() {
        let tid = 0x0002_0003;
        let body = sm_lighting(G_R2_ACTOR_ID, 100, 200, 3, 101, 201, tid, 1);
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_LIGHTING);
        assert_eq!(u16at12(&body, 6), 100, "Param = X");
        assert_eq!(u16at12(&body, 8), 200, "Tag = Y");
        assert_eq!(
            u16at12(&body, 10),
            3,
            "Series = baseObject.Dir（不是 wParam）"
        );
        let wl = edcode::decode(&body[12..]);
        assert_eq!(wl.len(), 16, "尾 = Encode(16B MessageBodyWL)");
        assert_eq!(i32at(&wl, 0), 101, "Param1 = target.CurrX");
        assert_eq!(i32at(&wl, 4), 201, "Param2 = target.CurrY");
        assert_eq!(i32at(&wl, 8), tid, "Tag1 = nParam3 = target ActorId");
        assert_eq!(i32at(&wl, 12), 1, "Tag2 = wParam");
        assert_eq!(client_frame(&body), hex(G_R2_LIGHTING));
    }

    /// `SM_SPACEMOVE_HIDE`(800) / `SM_SPACEMOVE_HIDE2`(806)：头 `(ActorId, 0, 0, 0)`，**无尾**。
    ///
    /// 红检：把 `ident` 传错（800↔806）或给它加尾 ⇒ 必红。
    #[test]
    fn sm_spacemove_hide_frame_header_only() {
        let body = sm_spacemove_hide(messages::SM_SPACEMOVE_HIDE, G_R2_ACTOR_ID);
        assert_eq!(body.len(), 12, "无尾 ⇒ 恰好 12B");
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_SPACEMOVE_HIDE);
        assert_eq!(&body[6..12], &[0u8; 6], "Param/Tag/Series 全 0");
        assert_eq!(client_frame(&body), hex(G_R2_SPACEMOVE_HIDE));

        let body2 = sm_spacemove_hide(messages::SM_SPACEMOVE_HIDE2, G_R2_ACTOR_ID);
        assert_eq!(u16at12(&body2, 4), messages::SM_SPACEMOVE_HIDE2);
        assert_eq!(client_frame(&body2), hex(G_R2_SPACEMOVE_HIDE2));
    }

    /// `SM_SPACEMOVE_SHOW`(801) / `SM_SPACEMOVE_SHOW2`(807)：头 `(ActorId, X, Y, MakeWord(Dir, Light))`
    /// + 尾 `Encode(8B CharDesc)`，`Msg` 非空时再接 `EncodeString(Msg+'/'+color)`。
    ///
    /// 红检：① 名串段从"两段拼接"改成不拼 ⇒ `_NAME` 用例必红；② `ident` 传错（801↔807）⇒ 必红。
    #[test]
    fn sm_spacemove_show_frame_with_and_without_name() {
        let desc = hex(R2_DESC8);
        let body = sm_spacemove_show(
            messages::SM_SPACEMOVE_SHOW,
            G_R2_ACTOR_ID,
            287,
            618,
            0,
            5,
            &desc,
            &[],
        );
        assert_eq!(i32at(&body, 0), G_R2_ACTOR_ID, "Recog = ActorId");
        assert_eq!(u16at12(&body, 4), messages::SM_SPACEMOVE_SHOW);
        assert_eq!(u16at12(&body, 6), 287, "Param = X");
        assert_eq!(u16at12(&body, 8), 618, "Tag = Y");
        assert_eq!(
            u16at12(&body, 10),
            make_word(0, 5),
            "Series = MakeWord(Dir, Light)"
        );
        assert_eq!(
            edcode::decode(&body[12..]),
            desc,
            "无 Msg ⇒ 尾仅 Encode(8B CharDesc)"
        );
        assert_eq!(client_frame(&body), hex(G_R2_SPACEMOVE_SHOW));

        let body2 = sm_spacemove_show(
            messages::SM_SPACEMOVE_SHOW2,
            G_R2_ACTOR_ID,
            287,
            618,
            0,
            5,
            &desc,
            &[],
        );
        assert_eq!(u16at12(&body2, 4), messages::SM_SPACEMOVE_SHOW2);
        assert_eq!(client_frame(&body2), hex(G_R2_SPACEMOVE_SHOW2));

        // `Msg` 非空：尾 = Encode(8B desc) ++ EncodeString(Msg+'/'+color)（两段直接相接）。
        let name = [0xCD_u8, 0xE6, 0xBC, 0xD2, b'/', b'5']; // "玩家/5" 的 GBK 字节
        let body3 = sm_spacemove_show(
            messages::SM_SPACEMOVE_SHOW,
            G_R2_ACTOR_ID,
            287,
            618,
            0,
            5,
            &desc,
            &name,
        );
        let mut want_tail = edcode::encode(&desc);
        want_tail.extend_from_slice(&edcode::encode(&name));
        assert_eq!(
            &body3[12..],
            want_tail.as_slice(),
            "尾 = 两段各自 Encode 后相接"
        );
        assert_eq!(client_frame(&body3), hex(G_R2_SPACEMOVE_SHOW_NAME));
    }

    // ---- 帧面审计 2026-10-18（G-1/G-2/G-3/G-5/G-6/G-9）----

    /// **G-1**：`SM_ADDITEM` 头 = `Recog=ActorId, Param/Tag=0, Series=1`，尾 = `EncodeBuffer(ClientItem)`。
    ///
    /// 红检：把 `sm_additem` 的 `tail` 参数去掉（回到 header-only）或 `series` 改 0 ⇒ 必红。
    #[test]
    fn sm_additem_header_and_client_item_tail() {
        let tail = vec![0xAAu8; 166]; // encode_len(124)=166；本用例只锁透传/字段位
        let body = sm_additem(0x0102_0304, &tail);
        assert_eq!(
            &body[..12],
            &command_header(0x0102_0304, messages::SM_ADDITEM, 0, 0, 1)
        );
        assert_eq!(body.len(), 12 + 166);
        assert_eq!(&body[12..], tail.as_slice());
    }

    /// **G-2**：`SM_UPDATEITEM(203)` = `sm_delitem` 同形（ident 203, series=1）+ `ClientItem` 尾。
    #[test]
    fn sm_updateitem_is_delitem_shape_with_ident_203() {
        let tail = vec![0x5Au8; 166];
        let body = sm_updateitem(0x1122_3344, &tail);
        assert_eq!(
            &body[..12],
            &command_header(0x1122_3344, messages::SM_UPDATEITEM, 0, 0, 1)
        );
        assert_eq!(&body[12..], tail.as_slice());
        // 与 202 只差 ident。
        let del = sm_delitem(0x1122_3344, &tail);
        assert_eq!(&body[6..], &del[6..]);
        assert_ne!(&body[4..6], &del[4..6]);
    }

    /// **G-3**：`SM_SENDUSEITEMS(621)` 头全 0；`SM_SENDMYMAGIC(211)` 的 `series=count`。
    #[test]
    fn sm_senduseitems_and_sendmymagic_headers() {
        let tail = b"0/xx/5/yy/";
        let b621 = sm_senduseitems(tail);
        assert_eq!(
            &b621[..12],
            &command_header(0, messages::SM_SENDUSEITEMS, 0, 0, 0)
        );
        assert_eq!(&b621[12..], tail.as_slice());

        let b211 = sm_sendmymagic(3, tail);
        assert_eq!(
            &b211[..12],
            &command_header(0, messages::SM_SENDMYMAGIC, 0, 0, 3)
        );
        assert_eq!(&b211[12..], tail.as_slice());
    }

    /// **G-5**：`SM_HEAVYHIT(15)` 的尾 = `processMsg.Msg` **原样**（挖矿命中 `"1"` ⇒ `31`）。
    #[test]
    fn sm_action_hit_heavyhit_tail_passthrough() {
        let body = sm_action_hit(messages::SM_HEAVYHIT, 1, 2, 3, 4, b"1");
        assert_eq!(body, vec![1, 0, 0, 0, 0x0F, 0x00, 2, 0, 3, 0, 4, 0, b'1']);
        assert_eq!(body.len(), 13);
        // 普通攻击（空尾）仍是 12B。
        assert_eq!(sm_action_hit(messages::SM_HIT, 1, 2, 3, 4, &[]).len(), 12);
    }

    /// **G-6**：`AntiMagic=300`（0x012C）必须完整进 `Recog`（旧实现 `lo_byte` ⇒ 44）。
    #[test]
    fn sm_subability_anti_magic_keeps_high_byte() {
        let body = sm_subability(300, 17, 15, 0, 0, 0, 0);
        // recog = 0x0000012C；ident=752(0x02F0)；param = MakeWord(17,15)=0x0F11。
        assert_eq!(
            body,
            vec![0x2C, 0x01, 0, 0, 0xF0, 0x02, 0x11, 0x0F, 0, 0, 0, 0]
        );
        // 回归：`AntiMagic<=255` 与旧值一致（金标准 #13 recog=1）。
        assert_eq!(&sm_subability(1, 5, 15, 0, 0, 0, 0)[..4], &[1, 0, 0, 0]);
    }

    /// **G-9**：`SM_SYSMESSAGE` 的颜色是 `MakeWord(ushort, ushort)`：`f` 的第 8 位会串进高字节。
    #[test]
    fn sm_sysmessage_color_u16_spills_like_oracle() {
        // MakeWord(0x0190, 0x0002) = 0x0190 | 0x0200 = 0x0390。
        let body = sm_sysmessage(1, 0x0190, 0x0002, &[]);
        assert_eq!(u16at12(&body, 6), 0x0390);
        // 配置色（<=255）不变：MakeWord(0xDB, 0xFF) = 0xFFDB（金标准）。
        assert_eq!(u16at12(&sm_sysmessage(1, 0xDB, 0xFF, &[]), 6), 0xFFDB);
    }

    /// **Lane B**：`SM_DELMAGIC(212)` 的命令头逐字节 —— `Recog=MagIdx`、`Param/Tag=0`、`Series=1`、
    /// **无尾**。真机 `s2c-621b` seq4 的 `raw_hex`（`2365…2121`，经网关 `client_frame`）解码后
    /// 即本 12B（牙 3：把 recog 写成 0 或 series 写成 0 ⇒ 本用例红）。
    #[test]
    fn sm_delmagic_header_matches_capture() {
        assert_eq!(
            sm_delmagic(1),
            vec![1, 0, 0, 0, 0xD4, 0x00, 0, 0, 0, 0, 1, 0],
            "212: Recog=1, Ident=0x00D4, Param=0, Tag=0, Series=1"
        );
        assert_eq!(sm_delmagic(2)[0], 2, "Recog 是 MagIdx 而非固定 1");
        assert_eq!(sm_delmagic(2)[10], 1, "Series 恒 1");
        // 真机原始线缆字节（`client_frame`）— 与抓包 raw_hex 完全一致。
        assert_eq!(crate::smframe::client_frame(&sm_delmagic(1)), {
            let mut v = vec![b'#'];
            v.extend_from_slice(&mir2_protocol::edcode::encode(&[
                1, 0, 0, 0, 0xD4, 0x00, 0, 0, 0, 0, 1, 0,
            ]));
            v.push(b'!');
            v
        });
    }

    /// **Lane B**：`SM_ADDMAGIC(210)` —— `Recog/Param/Tag=0`、`Series=1`，尾 =
    /// `EDCode.EncodeBuffer(ClientMagic)`（`Key=0, Level=1, CurTrain=0, Def=magic`）。
    /// 牙 6：漏 `edcode` / 漏 `encode_client_magic` 的 `Def` ⇒ 长度与尾字节断言必红。
    #[test]
    fn sm_addmagic_header_and_client_magic_tail() {
        let def = mir2_data::MagicInfo {
            magic_id: 1,
            magic_name: "火球术".into(),
            effect_type: 1,
            effect: 1,
            spell: 5,
            power: 3,
            max_power: 9,
            job: 0,
            train_level: [1, 2, 3, 4],
            max_train: [10, 20, 30, 40],
            train_lv: 3,
            delay_time: 700,
            def_spell: 0,
            def_power: 0,
            def_max_power: 0,
            desc: String::new(),
        };
        let body = sm_addmagic(&def);
        assert_eq!(&body[..12], &[0, 0, 0, 0, 0xD2, 0x00, 0, 0, 0, 0, 1, 0]);
        let client_magic = crate::itemwire::encode_client_magic(0, 1, 0, &def);
        assert_eq!(client_magic.len(), 84, "1+1+2+4 + MagicInfo 76");
        assert_eq!(body[12..], mir2_protocol::edcode::encode(&client_magic));
    }

    /// **Lane B**：入图帧序 —— 658 → `magic_journal`（212/210）→ 队列出队（51 …）。
    #[test]
    fn magic_journal_frames_follow_notice_and_precede_queue() {
        use mir2_protocol::frame::CommandMessage;
        let inputs = EnterWorldInputs {
            actor_id: 7,
            map_file_name: Some(b"0".to_vec()),
            logon_body_wl: Some((0, 0, 0, 0)),
            magic_journal: vec![MagicEvent::Del { mag_idx: 1 }],
            ..EnterWorldInputs::default()
        };
        let ids: Vec<u16> = inputs
            .frames()
            .iter()
            .map(|b| CommandMessage::from_bytes(b).unwrap().ident)
            .collect();
        assert_eq!(&ids[..3], &[658, 212, 51], "658 → 212 → 51");
    }
}
