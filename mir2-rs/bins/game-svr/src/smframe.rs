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
//! - 服务器配置：`11029` 的 `ClientConf`、`658` 的公告、`5008` 的元宝/金币名、`811` 的三段能力模板（配置层）。
//!
//! 生产路径当前只下发了输入齐备的 `SM_NEWMAP`（`enter_world_first_frames`，见 `main.rs`）；
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
/// # Panics
/// body 不足 12B 时 panic（调用方是自家构造器，恒 >= 12）。
#[must_use]
pub fn client_frame(gm_data: &[u8]) -> Vec<u8> {
    assert!(gm_data.len() >= 12, "GM_DATA body 必须含 12B 命令头");
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

/// `SM_VERSION_FAIL`（`PlayObject.Message.cs:1690-1692`）：
/// `MakeMessage(SM_VERSION_FAIL, ClientFile1_CRC, LoWord(ClientFile2_CRC), HiWord(ClientFile2_CRC), 0)`
/// + 尾 `"<<<<<<"`（**原样字面量**，不是 EDCode 编码——金标准里捕获工具把它解码成了垃圾，但这 6 字节在线缆上就是原样）。
#[must_use]
pub fn sm_version_fail(recog: i32, crc2: i32) -> Vec<u8> {
    let header = command_header(
        recog,
        messages::SM_VERSION_FAIL,
        lo_word(crc2),
        hi_word(crc2),
        0,
    );
    gm_data_body(header, b"<<<<<<")
}

/// `SM_SENDNOTICE`（`PlayObject.cs:1064-1076` 的 `SendNotice`）：
/// `SendDefMessage(SM_SENDNOTICE, 2000, 0, 0, 0, sNoticeMsg)` ⇒ `Recog` **恒 2000**，
/// 尾 = `EncodeString(公告文本)`（文本由 `M2Share.NoticeMgr.GetNoticeMsg` 读出，逐行以 `\x20\x1B` 相连；
/// 这里收文本原文，来源属配置层，登记）。
#[must_use]
pub fn sm_sendnotice(notice: &[u8]) -> Vec<u8> {
    let header = command_header(2000, messages::SM_SENDNOTICE, 0, 0, 0);
    gm_data_body(header, &edcode::encode(notice))
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

/// `SM_ADJUST_BONUS`(811) 的三段能力模板 `(BonusPoint, A, B, C)`（各自 `EncodeMessage` 后用 `/` 拼）。
pub type AdjustBonusParts = (i32, Vec<u8>, Vec<u8>, Vec<u8>);

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
    /// 登录前公告（`NoticeMgr`，属配置层）。
    pub notice: Option<Vec<u8>>,
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
        if let Some(notice) = &self.notice {
            out.push(sm_sendnotice(notice));
        }
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
            notice: Some(edcode::decode(&t0)),
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
}
