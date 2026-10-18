//! **A 类批（28 条）出站 `SM_*` 编码器** —— 新建模块，不改 `smframe.rs` 文件尾
//! （该文件尾被待落 B/D 两件占用；见卡 `openmir2-outbound-encoders-red-batch` 的争用面说明）。
//!
//! 逐条按 oracle `src/M2Server/Player/PlayObject.Message.cs` 的 `RM_*` 观察者支复刻：
//! 发射形状只有三种（`PlayObject.Socket.cs`）：
//! - `SendDefMessage(wIdent, nRecog, nParam, nTag, nSeries)`（`:17`）⇒ `MakeMessage` + `SendSocket(defMsg)`
//!   ⇒ **12B 命令头 + 无尾**（header-only）；
//! - `SendDefMessage(..., sMsg)`（`:27`）⇒ 非空 `sMsg` 追加 `EDCode.EncodeString(sMsg)`，空则不追加；
//! - `MakeMessage(...)` + `SendSocket(defMsg, sMsg)`（`:101`）⇒ 尾 = **调用方已编码好的 `sMsg`**（原样透传，
//!   本层只做「按 oracle 逐字段拼头 + 拼尾」，尾的 `Encode*` 由各自函数口径决定）。
//!
//! `MakeMessage(int msg, int recog, int param, int tag, int series)`（`src/OpenMir2/Messages.cs:846`）
//! ⇒ 头字段序恒为 `Recog(i32) + Ident(u16) + Param(u16) + Tag(u16) + Series(u16)`，小端
//! （复用 [`crate::smframe::command_header`]）。凡 oracle 形参是 `int` 而落在单个 `u16` 格上的，
//! 按 `(ushort)` 截断语义 `as u16`（与 `MakeMessage` 的强制转换逐位一致）。
//!
//! **发射点接线一律不做**（本批只交编码层；接线片另开，逐条状态见 rationale 的接线状态表）。
//!
//! | message | 常量 | oracle 行 | 形状 |
//! |---|---|---|---|
//! | [`sm_716`] | 716 | `PlayObject.Message.cs:1929` | header-only |
//! | [`sm_breakweapon`] | 1102 | `:2016` | header-only |
//! | [`sm_changeface`] | 1104 | `:2024` | 头 + `Encode(CharDesc 8B)` |
//! | [`sm_changelight`] | 654 | `:1840` | header-only |
//! | [`sm_changemap`] | 634 | `:1776` | 头 + `EncodeString(Msg)` |
//! | [`sm_charstatuschanged`] | 657 | `:1846` | header-only |
//! | [`sm_closedoor`] | 614 | `:1754` | header-only |
//! | [`sm_closehealth`] | 1101 | `:2013` | header-only（D-9 家族，无 HIGHHP 站 ⇒ 布局不变） |
//! | [`sm_hideevent`] | 805 | `:1993` | header-only |
//! | [`sm_instancehealguage`] | 1103 | `:2019` | D-9 双分支 |
//! | [`sm_itemshow`] | 610 | `:1745` | 头 + `EncodeString(Msg)`（T-19/T-25 ⇒ 接线登记） |
//! | [`sm_lampchangedura`] | 655 | `:1843` | header-only |
//! | [`sm_magicfire_fail`] | 639 | `:1795` | header-only |
//! | [`sm_makedrug_fail`] | 714 | `:1882` | header-only |
//! | [`sm_makedrug_success`] | 713 | `:1879` | header-only |
//! | [`sm_merchantdlgclose`] | 644 | `:1807` | header-only |
//! | [`sm_movemessage`] | 99 | `:1727` | 头 + `EncodeString(Msg)` |
//! | [`sm_mystatus`] | 708 | `:1950` | header-only |
//! | [`sm_opendoor_ok`] | 612 | `:1751` | header-only |
//! | [`sm_openhealth`] | 1100 | `:2010` | D-9 双分支 |
//! | [`sm_password`] | 5009 | `:2032` | header-only |
//! | [`sm_playdice`] | 8001 | `:2039` | 头 + `Encode(MessageBodyWL 16B)` + `EncodeString(Msg)` |
//! | [`sm_queryybdeal`] | 23023 | `:2054` | 头 + 尾原样 |
//! | [`sm_queryybsell`] | 23021 | `:2050` | 头 + 尾原样 |
//! | [`sm_reconnect`] | 802 | `:1990` | 头 + `EncodeString(Msg)` |
//! | [`sm_sendusermakedrugitemlist`] | 712 | `:1876` | 头 + `EncodeString(Msg)` |
//! | [`sm_showevent`] | 804 | `:1999` | 头 + `Encode(ShortMessage 4B)` |
//! | [`sm_whisper`] | 103 | `:1709` | 头 + `EncodeString(Msg)` |

use crate::smframe::{
    client_layout_enabled, command_header, gm_data_body, hi_word, lo_word, make_word,
};
use mir2_protocol::{edcode, messages};

/// `SM_716`(716)（`PlayObject.Message.cs:1929` 的 `RM_10205` 支）：
/// `SendDefMessage(SM_716, ActorId, nParam1, nParam2, nParam3)` ⇒ header-only（无尾）。
#[must_use]
pub fn sm_716(actor_id: i32, n_param1: i32, n_param2: i32, n_param3: i32) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_716,
        n_param1 as u16,
        n_param2 as u16,
        n_param3 as u16,
    );
    gm_data_body(header, &[])
}

/// `SM_BREAKWEAPON`(1102)（`PlayObject.Message.cs:2016` 的 `RM_BREAKWEAPON` 支）：
/// `SendDefMessage(SM_BREAKWEAPON, ActorId, 0, 0, 0)` ⇒ header-only。
/// `Recog` = 断武器的**角色** ActorId（客户端据此找 actor 播武器碎裂）。
#[must_use]
pub fn sm_breakweapon(actor_id: i32) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_BREAKWEAPON, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_CHANGEFACE`(1104)（`PlayObject.Message.cs:2024` 的 `RM_CHANGEFACE` 支）：
/// `MakeMessage(SM_CHANGEFACE, nParam1, LoWord(nParam2), HiWord(nParam2), 0)`
/// 追加尾 `EDCode.EncodePacket(CharDesc)`。
///
/// oracle 仅在 `nParam1 != 0 && nParam2 != 0` 时发（空则整帧不发）。
/// `CharDesc`（`src/OpenMir2/Packets/ClientPackets/CharDesc.cs`）= `{ Feature: i32; Status: i32 }`
/// ⇒ `SerializerUtil.Serialize` 出 8B 小端（`Feature` 在前、`Status` 在后），再整段 `Encode`。
/// 客户端 BotSrv 侧解析见 `src/BotSrv/Player/RobotPlayer.Operate.cs:719`（`DecodeBuffer<CharDesc>`）。
#[must_use]
pub fn sm_changeface(recog: i32, target_id: i32, feature: i32, status: i32) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_CHANGEFACE,
        lo_word(target_id),
        hi_word(target_id),
        0,
    );
    gm_data_body(header, &char_desc_tail(feature, status))
}

/// `SM_CHANGELIGHT`(654)（`PlayObject.Message.cs:1840` 的 `RM_CHANGELIGHT` 支）：
/// `SendDefMessage(SM_CHANGELIGHT, ActorId, baseObject.Light, (short)Config.nClientKey, 0)`
/// ⇒ header-only。`Param = Light`（`BaseObject.Light` 是 `byte`）、`Tag = (u16)nClientKey`。
#[must_use]
pub fn sm_changelight(actor_id: i32, light: u8, client_key: i32) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_CHANGELIGHT,
        u16::from(light),
        client_key as u16,
        0,
    );
    gm_data_body(header, &[])
}

/// `SM_CHANGEMAP`(634)（`PlayObject.Message.cs:1776` 的 `RM_CHANGEMAP` 支）：
/// `SendDefMessage(SM_CHANGEMAP, ActorId, CurrX, CurrY, DayBright(), processMsg.Msg)`
/// ⇒ 头 + 尾 `EDCode.EncodeString(Msg)`（`Msg` 为地图名等原始字节；本层不转 UTF-8）。
/// 观察者随后还有 `RefUserState`/`SendMapDescription`/`SendServerConfig`（另帧、不在本编码器内）。
#[must_use]
pub fn sm_changemap(actor_id: i32, cur_x: u16, cur_y: u16, day_bright: u16, msg: &[u8]) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_CHANGEMAP, cur_x, cur_y, day_bright);
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_CHARSTATUSCHANGED`(657)（`PlayObject.Message.cs:1846` 的 `RM_CHARSTATUSCHANGED` 支）：
/// `SendDefMessage(SM_CHARSTATUSCHANGED, ActorId, LoWord(nParam1), HiWord(nParam1), this.HitSpeed)`
/// ⇒ header-only（无尾）。`status` = oracle `nParam1`（`int`），按 `Lo/HiWord` 拆进 `Param/Tag`；
/// `Series = HitSpeed`（`CharacterObject.HitSpeed` 是 `ushort`）。
#[must_use]
pub fn sm_charstatuschanged(actor_id: i32, status: i32, hit_speed: u16) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_CHARSTATUSCHANGED,
        lo_word(status),
        hi_word(status),
        hit_speed,
    );
    gm_data_body(header, &[])
}

/// `SM_CLOSEDOOR`(614)（`PlayObject.Message.cs:1754` 的 `RM_DOORCLOSE` 支）：
/// `SendDefMessage(SM_CLOSEDOOR, 0, nParam1, nParam2, 0)` ⇒ header-only，`Recog` 恒 0。
/// **orphan 登记（接线首批，registered_not_wired）**：本帧与 sm_opendoor_ok 在 oracle 属**结构性不可达**—— CM_OPENDOOR 守卫写反（PlayObject.Operate.cs:363-376）⇒ 门永不开 ⇒ SendDoorStatus 永不出帧（见 tests/parity/m3/design-door-and-mapevent-20261016.md §2.2）；按 AGENTS.md 「不许硬接不可达的帧」⇒ 登记不接线。
#[must_use]
pub fn sm_closedoor(n_param1: i32, n_param2: i32) -> Vec<u8> {
    let header = command_header(
        0,
        messages::SM_CLOSEDOOR,
        n_param1 as u16,
        n_param2 as u16,
        0,
    );
    gm_data_body(header, &[])
}

/// `SM_CLOSEHEALTH`(1101)（`PlayObject.Message.cs:2013` 的 `RM_CLOSEHEALTH` 支）：
/// `SendDefMessage(SM_CLOSEHEALTH, ActorId, 0, 0, 0)` ⇒ header-only。
///
/// **D-9 家族说明**：本条与 1100/1103 同族（卡 `openmir2-outbound-encoders-red-batch` 的 triage
/// 列入 HIGHHP 家族），但**冻结客户端没有本条的 `{$IF HIGHHP}` 站点**——`MirClient@c61d912d` 的
/// `ClMain.pas` 全部 HIGHHP 站点 = `:10225`(53)/`:10269`(31)/`:10338`(1100)/`:10365`(1103)
/// （审计 `213-highhp-family-audit.txt` §1/§3），`SM_CLOSEHEALTH` 支（`ClMain.pas:10353-10361`）
/// 只置 `Actor.m_boOpenHealth := False`、**不读任何 HP 字段**；且本帧载荷全 0 ⇒ **两种布局逐字节相同**
/// ⇒ 单一编码器即可（无需 `MIR2_CLIENT_ABILITY_LAYOUT` 选路）。见 rationale 的登记项。
#[must_use]
pub fn sm_closehealth(actor_id: i32) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_CLOSEHEALTH, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_HIDEEVENT`(805)（`PlayObject.Message.cs:1993` 的 `RM_HIDEEVENT` 支）：
/// `SendDefMessage(SM_HIDEEVENT, nParam1, wParam, nParam2, nParam3)` ⇒ header-only（无尾）。
/// **orphan 登记（接线首批，registered_not_wired）**：本帧与 sm_showevent 在 oracle 只由 MapEvent 系统（SystemShare.EventMgr + EventProcessor 20ms）在可见变化时发（PlayObject.cs:3278/3284）；本仓 crates/world 未建模 MapEvent（仅 CellType::MapEvent 枚举占位）⇒ 无生产发射点，登记不接线（见 tests/parity/m3/design-door-and-mapevent-20261016.md §0）。
#[must_use]
pub fn sm_hideevent(n_param1: i32, w_param: i32, n_param2: i32, n_param3: i32) -> Vec<u8> {
    let header = command_header(
        n_param1,
        messages::SM_HIDEEVENT,
        w_param as u16,
        n_param2 as u16,
        n_param3 as u16,
    );
    gm_data_body(header, &[])
}

/// `SM_INSTANCEHEALGUAGE`(1103)（`PlayObject.Message.cs:2019` 的 `RM_ABILSEEHEALGAUGE` 支）的
/// **oracle 布局**：`SendDefMessage(SM_INSTANCEHEALGUAGE, ActorId, WAbil.HP, WAbil.MaxHP, 0)`
/// ⇒ header-only（`MIR2_CLIENT_ABILITY_LAYOUT=0` 时用，跨侧逐字节对拍）。
#[must_use]
pub fn sm_instancehealguage(actor_id: i32, hp: u16, max_hp: u16) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_INSTANCEHEALGUAGE, hp, max_hp, 0);
    gm_data_body(header, &[])
}

/// `SM_INSTANCEHEALGUAGE`(1103) 的**冻结客户端 `HIGHHP` 布局**（生产默认；D-9 家族第四帧）。
///
/// 冻结客户端 `Source/MirClient/ClMain.pas:10365` 的 `SM_INSTANCEHEALGUAGE` 支在 `{$IF HIGHHP}`
///（`Source/Common/Grobal2.pas:39 = True`）下：
/// ```pascal
/// DecodeBuffer(body, @sMsg, SizeOf(TShortMessage));
/// Actor.m_Abil.HP    := DWORD(Makelong(msg.param, msg.tag));      // HP = tag<<16 | param
/// Actor.m_Abil.MaxHP := DWORD(Makelong(sMsg.ident, sMsg.wMsg));   // MaxHP = wMsg<<16 | ident
/// ```
/// ⇒ 客户端布局 = 头 `(Recog=ActorId, param=LoWord(HP), tag=HiWord(HP), series=0)`
/// 追加尾 `Encode(TShortMessage{ Ident=LoWord(MaxHP), wMsg=HiWord(MaxHP) })`（4B）。
/// 与 oracle [`sm_instancehealguage`]（`param=HP, tag=MaxHP, 无尾`）字段位与尾都不同。
#[must_use]
pub fn sm_instancehealguage_client(actor_id: i32, hp: u16, max_hp: u16) -> Vec<u8> {
    let hp32 = i32::from(hp);
    let max_hp32 = i32::from(max_hp);
    let header = command_header(
        actor_id,
        messages::SM_INSTANCEHEALGUAGE,
        lo_word(hp32),
        hi_word(hp32),
        0,
    );
    gm_data_body(
        header,
        &short_message_tail(lo_word(max_hp32), hi_word(max_hp32)),
    )
}

/// 1103 的**生产选路**（[`client_layout_enabled`]，与 52/53/31 同源同开关）：
/// 客户端布局（默认）⇒ [`sm_instancehealguage_client`]；`MIR2_CLIENT_ABILITY_LAYOUT=0`
/// ⇒ oracle 逐字节 [`sm_instancehealguage`]。
#[must_use]
pub fn sm_instancehealguage_frame(actor_id: i32, hp: u16, max_hp: u16) -> Vec<u8> {
    if client_layout_enabled() {
        sm_instancehealguage_client(actor_id, hp, max_hp)
    } else {
        sm_instancehealguage(actor_id, hp, max_hp)
    }
}

/// `SM_ITEMSHOW`(610)（`PlayObject.Message.cs:1745` 的 `RM_ITEMSHOW` 支）：
/// `SendDefMessage(SM_ITEMSHOW, nParam1, nParam2, nParam3, wParam, processMsg.Msg)`
/// ⇒ 头 `(Recog=nParam1, Param=nParam2, Tag=nParam3, Series=wParam)` + 尾 `EncodeString(Msg)`。
///
/// **接线登记（T-19/T-25）**：oracle 全仓无 `MapItem.ItemId` 赋值点，`Envir.AddItemToMap` 首行
/// `if (mapItem.ItemId == 0) return false;` 恒真 ⇒ 死亡掉落/脚本掉落**恒不落地** ⇒ 本帧在 oracle 里
/// **不可达**（白名单 T-32 订正、对齐冻结 `src/`）。编码器照做（字节口径已钉），
/// **发射点接线不做**（登记为接线片；见 rationale 接线状态表）。
#[must_use]
pub fn sm_itemshow(
    n_param1: i32,
    n_param2: i32,
    n_param3: i32,
    w_param: i32,
    msg: &[u8],
) -> Vec<u8> {
    let header = command_header(
        n_param1,
        messages::SM_ITEMSHOW,
        n_param2 as u16,
        n_param3 as u16,
        w_param as u16,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_LAMPCHANGEDURA`(655)（`PlayObject.Message.cs:1843` 的 `RM_LAMPCHANGEDURA` 支）：
/// `SendDefMessage(SM_LAMPCHANGEDURA, nParam1, 0, 0, 0)` ⇒ header-only。
#[must_use]
pub fn sm_lampchangedura(n_param1: i32) -> Vec<u8> {
    let header = command_header(n_param1, messages::SM_LAMPCHANGEDURA, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_MAGICFIRE_FAIL`(639)（`PlayObject.Message.cs:1795` 的 `RM_MAGICFIREFAIL` 支）：
/// `SendDefMessage(SM_MAGICFIRE_FAIL, ActorId, 0, 0, 0)` ⇒ header-only（无尾）。
#[must_use]
pub fn sm_magicfire_fail(actor_id: i32) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_MAGICFIRE_FAIL, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_MAKEDRUG_FAIL`(714)（`PlayObject.Message.cs:1882` 的 `RM_MAKEDRUG_FAIL` 支）：
/// `SendDefMessage(SM_MAKEDRUG_FAIL, nParam1, 0, 0, 0)` ⇒ header-only。
#[must_use]
pub fn sm_makedrug_fail(n_param1: i32) -> Vec<u8> {
    let header = command_header(n_param1, messages::SM_MAKEDRUG_FAIL, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_MAKEDRUG_SUCCESS`(713)（`PlayObject.Message.cs:1879` 的 `RM_MAKEDRUG_SUCCESS` 支）：
/// `SendDefMessage(SM_MAKEDRUG_SUCCESS, nParam1, 0, 0, 0)` ⇒ header-only。
#[must_use]
pub fn sm_makedrug_success(n_param1: i32) -> Vec<u8> {
    let header = command_header(n_param1, messages::SM_MAKEDRUG_SUCCESS, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_MERCHANTDLGCLOSE`(644)（`PlayObject.Message.cs:1807` 的 `RM_MERCHANTDLGCLOSE` 支）：
/// `SendDefMessage(SM_MERCHANTDLGCLOSE, nParam1, 0, 0, 0)` ⇒ header-only。
#[must_use]
pub fn sm_merchantdlgclose(n_param1: i32) -> Vec<u8> {
    let header = command_header(n_param1, messages::SM_MERCHANTDLGCLOSE, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_MOVEMESSAGE`(99)（`PlayObject.Message.cs:1727` 的 `RM_MOVEMESSAGE` 支）：
/// `MakeMessage(SM_MOVEMESSAGE, ActorId, MakeWord(nParam1, nParam2), nParam3, wParam)`
/// 追加尾 `EDCode.EncodeString(Msg)`（同 `RM_HEAR` 家族共用 `SendSocket(ClientMsg, EncodeString(Msg))`，`:1731`）。
///
/// `MakeWord` 是 `ushort` 重载（`(ushort)(nParam1 | nParam2<<8)`，G-9）。
#[must_use]
pub fn sm_movemessage(
    actor_id: i32,
    n_param1: u16,
    n_param2: u16,
    n_param3: i32,
    w_param: i32,
    msg: &[u8],
) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_MOVEMESSAGE,
        // G-9：oracle 的 `MakeWord(ushort, ushort)`（`(ushort)(f | b<<8)`）。
        crate::smframe::make_word16(n_param1, n_param2),
        n_param3 as u16,
        w_param as u16,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_MYSTATUS`(708)（`PlayObject.Message.cs:1950` 的 `RM_MYSTATUS` 支）：
/// `SendDefMessage(SM_MYSTATUS, 0, (short)GetMyStatus(), 0, 0)` ⇒ header-only，`Recog` 恒 0、
/// `Param = (u16)(short)GetMyStatus()`（`PlayObject.Base.cs:1183` 返回 `int`，按 `(short)` 截断）。
#[must_use]
pub fn sm_mystatus(status: i32) -> Vec<u8> {
    let header = command_header(0, messages::SM_MYSTATUS, status as u16, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_OPENDOOR_OK`(612)（`PlayObject.Message.cs:1751` 的 `RM_DOOROPEN` 支）：
/// `SendDefMessage(SM_OPENDOOR_OK, 0, nParam1, nParam2, 0)` ⇒ header-only，`Recog` 恒 0。
#[must_use]
pub fn sm_opendoor_ok(n_param1: i32, n_param2: i32) -> Vec<u8> {
    let header = command_header(
        0,
        messages::SM_OPENDOOR_OK,
        n_param1 as u16,
        n_param2 as u16,
        0,
    );
    gm_data_body(header, &[])
}

/// `SM_OPENHEALTH`(1100)（`PlayObject.Message.cs:2010` 的 `RM_OPENHEALTH` 支）的 **oracle 布局**：
/// `SendDefMessage(SM_OPENHEALTH, ActorId, WAbil.HP, WAbil.MaxHP, 0)` ⇒ header-only。
#[must_use]
pub fn sm_openhealth(actor_id: i32, hp: u16, max_hp: u16) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_OPENHEALTH, hp, max_hp, 0);
    gm_data_body(header, &[])
}

/// `SM_OPENHEALTH`(1100) 的**冻结客户端 `HIGHHP` 布局**（生产默认）。
///
/// 冻结客户端 `Source/MirClient/ClMain.pas:10338` 的 `SM_OPENHEALTH` 支在 `{$IF HIGHHP}` 下：
/// ```pascal
/// DecodeBuffer(body, @sMsg, SizeOf(TShortMessage));
/// Actor.m_Abil.HP    := DWORD(Makelong(msg.param, msg.tag));
/// Actor.m_Abil.MaxHP := DWORD(Makelong(sMsg.ident, sMsg.wMsg));
/// ```
/// 与 [`sm_instancehealguage_client`] 同形（仅 ident 不同）。
#[must_use]
pub fn sm_openhealth_client(actor_id: i32, hp: u16, max_hp: u16) -> Vec<u8> {
    let hp32 = i32::from(hp);
    let max_hp32 = i32::from(max_hp);
    let header = command_header(
        actor_id,
        messages::SM_OPENHEALTH,
        lo_word(hp32),
        hi_word(hp32),
        0,
    );
    gm_data_body(
        header,
        &short_message_tail(lo_word(max_hp32), hi_word(max_hp32)),
    )
}

/// 1100 的**生产选路**（[`client_layout_enabled`]）：客户端布局（默认）⇒ [`sm_openhealth_client`]；
/// `MIR2_CLIENT_ABILITY_LAYOUT=0` ⇒ oracle 逐字节 [`sm_openhealth`]。
#[must_use]
pub fn sm_openhealth_frame(actor_id: i32, hp: u16, max_hp: u16) -> Vec<u8> {
    if client_layout_enabled() {
        sm_openhealth_client(actor_id, hp, max_hp)
    } else {
        sm_openhealth(actor_id, hp, max_hp)
    }
}

/// `SM_PASSWORD`(5009)（`PlayObject.Message.cs:2032` 的 `RM_PASSWORD` 支）：
/// `SendDefMessage(SM_PASSWORD, 0, 0, 0, 0)` ⇒ header-only，全 0。
#[must_use]
pub fn sm_password() -> Vec<u8> {
    let header = command_header(0, messages::SM_PASSWORD, 0, 0, 0);
    gm_data_body(header, &[])
}

/// `SM_PLAYDICE`(8001)（`PlayObject.Message.cs:2039` 的 `RM_PLAYDICE` 支）：
/// `MakeMessage(SM_PLAYDICE, ActorId, wParam, 0, 0)`
/// 追加尾 `EDCode.EncodePacket(MessageBodyWL) + EDCode.EncodeString(Msg)`（`:2045`，**两段直接拼**）。
///
/// `MessageBodyWL`（`src/OpenMir2/Packets/ServerPackets/MessageBodyWL.cs`，`Pack=1`）4×`int`：
/// `Param1=nParam1, Param2=nParam2, Tag1=nParam3, Tag2=0`（`Tag2` 未赋值 ⇒ 0）⇒ 16B 小端。
#[must_use]
pub fn sm_playdice(
    actor_id: i32,
    w_param: i32,
    n_param1: i32,
    n_param2: i32,
    n_param3: i32,
    msg: &[u8],
) -> Vec<u8> {
    let header = command_header(actor_id, messages::SM_PLAYDICE, w_param as u16, 0, 0);
    let mut tail = message_body_wl_tail(n_param1, n_param2, n_param3, 0);
    tail.extend_from_slice(&edcode::encode(msg));
    gm_data_body(header, &tail)
}

/// `SM_QUERYYBDEAL`(23023)（`PlayObject.Message.cs:2054` 的 `RM_QUERYYBDEAL` 支）：
/// `MakeMessage(SM_QUERYYBDEAL, nParam1, nParam2, nParam3, wParam)` + `SendSocket(ClientMsg, processMsg.Msg)`
/// ⇒ 尾 = **`processMsg.Msg` 原样**（oracle 不 Encode，`Msg` 已是编码好的出售列表文本）。
#[must_use]
pub fn sm_queryybdeal(
    n_param1: i32,
    n_param2: i32,
    n_param3: i32,
    w_param: i32,
    tail: &[u8],
) -> Vec<u8> {
    let header = command_header(
        n_param1,
        messages::SM_QUERYYBDEAL,
        n_param2 as u16,
        n_param3 as u16,
        w_param as u16,
    );
    gm_data_body(header, tail)
}

/// `SM_QUERYYBSELL`(23021)（`PlayObject.Message.cs:2050` 的 `RM_QUERYYBSELL` 支）：
/// `MakeMessage(SM_QUERYYBSELL, nParam1, nParam2, nParam3, wParam)` + `SendSocket(ClientMsg, processMsg.Msg)`
/// ⇒ 尾 = **`processMsg.Msg` 原样**（同 [`sm_queryybdeal`]）。
#[must_use]
pub fn sm_queryybsell(
    n_param1: i32,
    n_param2: i32,
    n_param3: i32,
    w_param: i32,
    tail: &[u8],
) -> Vec<u8> {
    let header = command_header(
        n_param1,
        messages::SM_QUERYYBSELL,
        n_param2 as u16,
        n_param3 as u16,
        w_param as u16,
    );
    gm_data_body(header, tail)
}

/// `SM_RECONNECT`(802)（`PlayObject.Message.cs:1990` 的 `RM_RECONNECTION` 支）：
/// `SendDefMessage(SM_RECONNECT, 0, 0, 0, 0, processMsg.Msg)` ⇒ 头 + 尾 `EncodeString(Msg)`
/// （`Recog/Param/Tag/Series` 全 0；空 `Msg` ⇒ 退化为 header-only）。
#[must_use]
pub fn sm_reconnect(msg: &[u8]) -> Vec<u8> {
    let header = command_header(0, messages::SM_RECONNECT, 0, 0, 0);
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_SENDUSERMAKEDRUGITEMLIST`(712)（`PlayObject.Message.cs:1876` 的 `RM_USERMAKEDRUGITEMLIST` 支）：
/// `SendDefMessage(SM_SENDUSERMAKEDRUGITEMLIST, nParam1, nParam2, 0, 0, processMsg.Msg)`
/// ⇒ 头 `(Recog=nParam1, Param=nParam2, Tag=0, Series=0)` + 尾 `EncodeString(Msg)`。
#[must_use]
pub fn sm_sendusermakedrugitemlist(n_param1: i32, n_param2: i32, msg: &[u8]) -> Vec<u8> {
    let header = command_header(
        n_param1,
        messages::SM_SENDUSERMAKEDRUGITEMLIST,
        n_param2 as u16,
        0,
        0,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `SM_SHOWEVENT`(804)（`PlayObject.Message.cs:1999` 的 `RM_SHOWEVENT` 支）：
/// `MakeMessage(SM_SHOWEVENT, nParam1, wParam, nParam2, nParam3)` + 尾 `EDCode.EncodePacket(ShortMessage)`。
///
/// oracle 组 `ShortMessage shortMessage = { Ident = HUtil32.HiWord(nParam2), wMsg = 0 }`（`:2002-2005`）；
/// `ShortMessage`（`src/OpenMir2/Packets/ClientPackets/ShortMessage.cs`）= `{ Ident: u16; wMsg: u16 }`
/// ⇒ 4B 小端，`Ident` 在前、`wMsg` 在后。
#[must_use]
pub fn sm_showevent(n_param1: i32, w_param: i32, n_param2: i32, n_param3: i32) -> Vec<u8> {
    let header = command_header(
        n_param1,
        messages::SM_SHOWEVENT,
        w_param as u16,
        n_param2 as u16,
        n_param3 as u16,
    );
    gm_data_body(header, &short_message_tail(hi_word(n_param2), 0))
}

/// `SM_WHISPER`(103)（`PlayObject.Message.cs:1709` 的 `RM_WHISPER` 支）：
/// `MakeMessage(SM_WHISPER, ActorId, MakeWord(nParam1, nParam2), 0, 1)`
/// 追加尾 `EDCode.EncodeString(Msg)`（`:1731` 共用 `SendSocket(ClientMsg, EncodeString(Msg))`）。
/// `Series` 恒 1；`MakeWord` 取颜色字节（同 [`crate::smframe::sm_hear`] 口径）。
#[must_use]
pub fn sm_whisper(actor_id: i32, n_param1: u8, n_param2: u8, msg: &[u8]) -> Vec<u8> {
    let header = command_header(
        actor_id,
        messages::SM_WHISPER,
        make_word(n_param1, n_param2),
        0,
        1,
    );
    gm_data_body(header, &edcode::encode(msg))
}

/// `CharDesc`（8B）的 `EDCode.EncodePacket` 尾：`[Feature:i32][Status:i32]` 小端后整段 `Encode`。
fn char_desc_tail(feature: i32, status: i32) -> Vec<u8> {
    let mut b = [0u8; 8];
    b[0..4].copy_from_slice(&feature.to_le_bytes());
    b[4..8].copy_from_slice(&status.to_le_bytes());
    edcode::encode(&b)
}

/// `MessageBodyWL`（16B）的 `EDCode.EncodePacket` 尾：`[Param1][Param2][Tag1][Tag2]` 各 `i32` 小端后整段 `Encode`。
fn message_body_wl_tail(param1: i32, param2: i32, tag1: i32, tag2: i32) -> Vec<u8> {
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&param1.to_le_bytes());
    b[4..8].copy_from_slice(&param2.to_le_bytes());
    b[8..12].copy_from_slice(&tag1.to_le_bytes());
    b[12..16].copy_from_slice(&tag2.to_le_bytes());
    edcode::encode(&b)
}

/// `TShortMessage`（4B）的 `EDCode.EncodePacket` 尾：`[Ident:u16][wMsg:u16]` 小端后整段 `Encode`。
fn short_message_tail(ident: u16, w_msg: u16) -> Vec<u8> {
    let mut b = [0u8; 4];
    b[0..2].copy_from_slice(&ident.to_le_bytes());
    b[2..4].copy_from_slice(&w_msg.to_le_bytes());
    edcode::encode(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一帧头 12B 的**自算 golden**：用字面 hex 钉死字段序（不引 `command_header`）。
    /// `recog` 小端 4B + `ident` 小端 2B + `param`/`tag`/`series` 各 2B。
    fn assert_header(body: &[u8], expect_hex12: &str) {
        assert!(body.len() >= 12);
        let want: Vec<u8> = (0..24)
            .step_by(2)
            .map(|i| u8::from_str_radix(&expect_hex12[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(&body[..12], want.as_slice(), "头 12B 自算 golden 不符");
    }

    // ---- 15 条 header-only（自算 golden 逐字节） ----------------------------------------------

    #[test]
    fn header_only_frames_self_computed_golden() {
        // sm_password: recog=0, ident=5009(0x1391), 其余 0。
        let b = sm_password();
        assert_eq!(b.len(), 12);
        assert_header(&b, "000000009113000000000000");

        // sm_magicfire_fail(0x0000_00c8=200): recog=200 -> c8 00 00 00, ident=639(0x027f)。
        let b = sm_magicfire_fail(200);
        assert_eq!(b.len(), 12);
        assert_header(&b, "c80000007f02000000000000");

        // sm_breakweapon: ident=1102(0x044e)。
        let b = sm_breakweapon(0x1122_3344);
        assert_eq!(b.len(), 12);
        assert_header(&b, "443322114e04000000000000");

        // sm_716(actor=1, 2,3,4): ident=716(0x02cc)。
        let b = sm_716(1, 2, 3, 4);
        assert_eq!(b.len(), 12);
        assert_header(&b, "01000000cc02020003000400");

        // sm_closedoor(recog=0, param=0x0007, tag=0x0009): ident=614(0x0266)。
        let b = sm_closedoor(7, 9);
        assert_eq!(b.len(), 12);
        assert_header(&b, "000000006602070009000000");

        // sm_opendoor_ok(recog=0, param=0x0007, tag=0x0009): ident=612(0x0264)。
        let b = sm_opendoor_ok(7, 9);
        assert_eq!(b.len(), 12);
        assert_header(&b, "000000006402070009000000");

        // sm_mystatus(status=5): recog=0, ident=708(0x02c4), param=5。
        let b = sm_mystatus(5);
        assert_eq!(b.len(), 12);
        assert_header(&b, "00000000c402050000000000");

        // sm_merchantdlgclose(recog=0x0000_0102=258): ident=644(0x0284)。
        let b = sm_merchantdlgclose(258);
        assert_eq!(b.len(), 12);
        assert_header(&b, "020100008402000000000000");

        // sm_lampchangedura(recog=0x0000_0203=515): ident=655(0x028f)。
        let b = sm_lampchangedura(515);
        assert_eq!(b.len(), 12);
        assert_header(&b, "030200008f02000000000000");

        // sm_makedrug_fail(recog=0x0000_00aa=170): ident=714(0x02ca)。
        let b = sm_makedrug_fail(170);
        assert_eq!(b.len(), 12);
        assert_header(&b, "aa000000ca02000000000000");

        // sm_makedrug_success(recog=0x0000_00aa=170): ident=713(0x02c9)。
        let b = sm_makedrug_success(170);
        assert_eq!(b.len(), 12);
        assert_header(&b, "aa000000c902000000000000");

        // sm_closehealth(actor=0x0303=771): ident=1101(0x044d)。
        let b = sm_closehealth(771);
        assert_eq!(b.len(), 12);
        assert_header(&b, "030300004d04000000000000");

        // sm_changelight(actor=1, light=6, client_key=0x0abc=2748): ident=654(0x028e)。
        let b = sm_changelight(1, 6, 0x0abc);
        assert_eq!(b.len(), 12);
        assert_header(&b, "010000008e020600bc0a0000");

        // sm_charstatuschanged(actor=1, status=0x0001_0002, hit_speed=7): ident=657(0x0291)。
        let b = sm_charstatuschanged(1, 0x0001_0002, 7);
        assert_eq!(b.len(), 12);
        assert_header(&b, "010000009102020001000700");

        // sm_hideevent(recog=0x10, wParam=0x11, nParam2=0x12, nParam3=0x13): ident=805(0x0325)。
        let b = sm_hideevent(0x10, 0x11, 0x12, 0x13);
        assert_eq!(b.len(), 12);
        assert_header(&b, "100000002503110012001300");
    }

    // ---- 带尾帧：头自算 golden + 尾字段逐项 ------------------------------------------------------

    #[test]
    fn sm_changemap_header_and_encode_string_tail() {
        let msg = encoding_rs::GBK.encode("0").0.into_owned();
        let b = sm_changemap(0x7c30_3998, 287, 618, 12, &msg);
        // ident=634(0x027a), param=287(0x011f), tag=618(0x026a), series=12(0x0c)。
        assert_header(&b, "9839307c7a021f016a020c00");
        assert_eq!(&b[12..], edcode::encode(&msg), "尾 = EncodeString(Msg)");
        assert_eq!(edcode::decode(&b[12..]), msg);
    }

    #[test]
    fn sm_itemshow_header_and_tail() {
        let msg = b"item/name";
        let b = sm_itemshow(0x0a0b_0c0d, 0x11, 0x22, 0x33, msg);
        assert_header(&b, "0d0c0b0a6202110022003300");
        assert_eq!(&b[12..], edcode::encode(msg));
    }

    #[test]
    fn sm_movemessage_header_makeword_and_tail() {
        let msg = b"hi";
        // recog=1, ident=99(0x63), param=MakeWord(0x0a,0x0b)=0x0b0a, tag=3, series=4。
        let b = sm_movemessage(1, 0x0a, 0x0b, 3, 4, msg);
        assert_header(&b, "0100000063000a0b03000400");
        assert_eq!(&b[12..], edcode::encode(msg));
    }

    #[test]
    fn sm_whisper_header_makeword_series1_and_tail() {
        let msg = b"whisper!";
        // recog=2, ident=103(0x67), param=MakeWord(1,2)=0x0201, series=1。
        let b = sm_whisper(2, 1, 2, msg);
        assert_header(&b, "020000006700010200000100");
        assert_eq!(&b[12..], edcode::encode(msg));
    }

    #[test]
    fn sm_reconnect_header_all_zero_and_tail() {
        let b = sm_reconnect(b"msg");
        // ident=802(0x0322)。
        assert_header(&b, "000000002203000000000000");
        assert_eq!(&b[12..], edcode::encode(b"msg"));
    }

    #[test]
    fn sm_sendusermakedrugitemlist_header_and_tail() {
        let b = sm_sendusermakedrugitemlist(0x0000_0101, 0x2222, b"drug");
        // ident=712(0x02c8)。
        assert_header(&b, "01010000c802222200000000");
        assert_eq!(&b[12..], edcode::encode(b"drug"));
    }

    #[test]
    fn sm_changeface_header_split_and_chardesc_tail() {
        // recog=0x0000_0b0c, target=0x0001_0002 -> param=0x0002, tag=0x0001, series=0。
        let b = sm_changeface(0x0000_0b0c, 0x0001_0002, 0x1122_3344, 0x5566_7788);
        assert_header(&b, "0c0b00005004020001000000");
        let mut desc = [0u8; 8];
        desc[0..4].copy_from_slice(&0x1122_3344_i32.to_le_bytes());
        desc[4..8].copy_from_slice(&0x5566_7788_i32.to_le_bytes());
        assert_eq!(&b[12..], edcode::encode(&desc), "尾 = Encode(CharDesc 8B)");
    }

    #[test]
    fn sm_playdice_header_wl_tail_then_string_tail() {
        let msg = b"dice";
        // actor=3, wParam=0x7788, ident=8001(0x1f41)。
        let b = sm_playdice(3, 0x7788, 0x0102_0304, 0x0506_0708, 0x090a_0b0c, msg);
        assert_header(&b, "03000000411f887700000000");
        let mut wl = [0u8; 16];
        wl[0..4].copy_from_slice(&0x0102_0304_i32.to_le_bytes());
        wl[4..8].copy_from_slice(&0x0506_0708_i32.to_le_bytes());
        wl[8..12].copy_from_slice(&0x090a_0b0c_i32.to_le_bytes());
        wl[12..16].copy_from_slice(&0_i32.to_le_bytes());
        let mut want = edcode::encode(&wl);
        want.extend_from_slice(&edcode::encode(msg));
        assert_eq!(
            &b[12..],
            want.as_slice(),
            "尾 = Encode(WL 16B) + EncodeString(Msg)"
        );
    }

    #[test]
    fn sm_showevent_header_and_short_message_tail() {
        // nParam1=0x21, wParam=0x22, nParam2=0x0003_0004 -> ident=HiWord=0x0003, nParam3=0x23。
        let b = sm_showevent(0x21, 0x22, 0x0003_0004, 0x23);
        assert_header(&b, "210000002403220004002300");
        let mut sm = [0u8; 4];
        sm[0..2].copy_from_slice(&0x0003_u16.to_le_bytes());
        sm[2..4].copy_from_slice(&0_u16.to_le_bytes());
        assert_eq!(
            &b[12..],
            edcode::encode(&sm),
            "尾 = Encode(ShortMessage 4B)"
        );
    }

    #[test]
    fn sm_queryybdeal_and_sell_tail_passthrough() {
        let tail = [0xde, 0xad, 0xbe, 0xef];
        let b = sm_queryybdeal(1, 2, 3, 4, &tail);
        // ident=23023(0x59ef)。
        assert_header(&b, "01000000ef59020003000400");
        assert_eq!(&b[12..], &tail, "尾原样透传");

        let b = sm_queryybsell(5, 6, 7, 8, &tail);
        // ident=23021(0x59ed)。
        assert_header(&b, "05000000ed59060007000800");
        assert_eq!(&b[12..], &tail, "尾原样透传");
    }

    // ---- D-9 家族（1100/1101/1103） ------------------------------------------------------------

    #[test]
    fn health_oracle_layout_is_header_only() {
        // 1100: recog=9, param=HP=18(0x12), tag=MaxHP=19(0x13), series=0。
        let b = sm_openhealth(9, 18, 19);
        assert_eq!(b.len(), 12);
        assert_header(&b, "090000004c04120013000000");

        // 1103: ident=1103(0x044f)。
        let b = sm_instancehealguage(9, 18, 19);
        assert_eq!(b.len(), 12);
        assert_header(&b, "090000004f04120013000000");
    }

    #[test]
    fn health_client_layout_splits_hp_and_carries_shortmessage_tail() {
        // 1100 client: param=LoWord(HP)=18, tag=HiWord(HP)=0, series=0; 尾={Ident=19, wMsg=0}。
        let b = sm_openhealth_client(9, 18, 19);
        assert_header(&b, "090000004c04120000000000");
        let mut sm = [0u8; 4];
        sm[0..2].copy_from_slice(&19_u16.to_le_bytes());
        assert_eq!(&b[12..], edcode::encode(&sm));

        // 1103 client: ident=1103(0x044f)。
        let b = sm_instancehealguage_client(9, 300, 500);
        assert_header(&b, "090000004f042c0100000000");
        let mut sm = [0u8; 4];
        sm[0..2].copy_from_slice(&500_u16.to_le_bytes());
        assert_eq!(&b[12..], edcode::encode(&sm));
    }

    #[test]
    fn health_frame_router_respects_layout_switch() {
        // 选路结果必须与 client_layout_enabled() 的分支一致（env 可能被外部 set）。
        let f = sm_openhealth_frame(9, 18, 19);
        if client_layout_enabled() {
            assert_eq!(f, sm_openhealth_client(9, 18, 19));
            assert_eq!(f.len(), 12 + edcode::encoded_len(4));
        } else {
            assert_eq!(f, sm_openhealth(9, 18, 19));
            assert_eq!(f.len(), 12);
        }
        let g = sm_instancehealguage_frame(9, 18, 19);
        if client_layout_enabled() {
            assert_eq!(g, sm_instancehealguage_client(9, 18, 19));
        } else {
            assert_eq!(g, sm_instancehealguage(9, 18, 19));
        }
    }

    // ---- 变异必红（改写实现后对应断言必红；此处自证「变异确实改变字节」） -----------------------------

    /// **红检 1（字段错位）**：`sm_changelight` 的 `param/tag` 对调必改变字节
    /// （把实现改成对调 ⇒ `header_only_frames_self_computed_golden` 中 changelight 那条必红）。
    #[test]
    fn mutation_redcheck_field_swap_differs() {
        let good = sm_changelight(1, 6, 0x0abc);
        let bad = gm_data_body(
            command_header(1, messages::SM_CHANGELIGHT, 0x0abc, 6, 0),
            &[],
        );
        assert_ne!(good, bad, "param/tag 对调必须改变字节");
        assert_ne!(&good[6..8], &bad[6..8]);
    }

    /// **红检 2（尾长错）**：`sm_changemap` 丢掉尾后的字节与正解不同。
    #[test]
    fn mutation_redcheck_missing_tail_differs() {
        let msg = b"0";
        let good = sm_changemap(1, 2, 3, 4, msg);
        let bad = gm_data_body(command_header(1, messages::SM_CHANGEMAP, 2, 3, 4), &[]);
        assert_ne!(good, bad, "去掉尾必须改变字节");
        assert_eq!(good.len(), bad.len() + edcode::encoded_len(msg.len()));
    }

    /// **红检 3（尾顺序错）**：`sm_playdice` 两段尾顺序对调后不同。
    #[test]
    fn mutation_redcheck_tail_order_differs() {
        let msg = b"d";
        let good = sm_playdice(1, 2, 3, 4, 5, msg);
        let mut wl = [0u8; 16];
        wl[0..4].copy_from_slice(&3_i32.to_le_bytes());
        wl[4..8].copy_from_slice(&4_i32.to_le_bytes());
        wl[8..12].copy_from_slice(&5_i32.to_le_bytes());
        let mut bad = edcode::encode(msg);
        bad.extend_from_slice(&edcode::encode(&wl));
        let bad_full = gm_data_body(command_header(1, messages::SM_PLAYDICE, 2, 0, 0), &bad);
        assert_ne!(good, bad_full, "两段尾顺序对调必须改变字节");
    }

    /// **红检 4（HiWord/LoWord 取错）**：`sm_showevent` 的 `ShortMessage.Ident` 取 `HiWord(nParam2)`。
    #[test]
    fn mutation_redcheck_hiword_differs() {
        let good = sm_showevent(1, 2, 0x0003_0004, 5);
        let bad = gm_data_body(command_header(1, messages::SM_SHOWEVENT, 2, 0x0004, 5), &{
            let mut sm = [0u8; 4];
            sm[0..2].copy_from_slice(&0x0004_u16.to_le_bytes()); // 错：LoWord
            edcode::encode(&sm)
        });
        assert_ne!(good, bad, "HiWord/LoWord 取错必须改变字节");
    }
}
