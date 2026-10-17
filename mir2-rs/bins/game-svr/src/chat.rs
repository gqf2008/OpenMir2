//! `CM_SAY`(3030) 的**普通说话**面（最小切片）。
//!
//! 移植源（逐条照抄，不按常识重推）：
//!
//! - `PlayObject.Message.cs:1394-1396`：`case Messages.CM_SAY: ProcessUserLineMsg(processMsg.Msg)`；
//! - `PlayObject.Chat.cs:229-...`：`ProcessUserLineMsg` —— 不以 `@` 开头 ⇒ `ProcessSayMsg(sData)`；
//! - `PlayObject.Chat.cs:106-...`：`ProcessSayMsg` —— `/`（私聊）、`!`（喊话/组队/行会）走别的支；
//!   普通行落 `BaseObject.Base.cs:475-479` 的
//!   `SendRefMsg(RM_HEAR, 0, btHearMsgFColor, btHearMsgBColor, 0, sChrName + ':' + sMsg)`；
//! - `BaseObject.Message.cs:383-440`：`SendRefMsg` —— 以自己为中心 ±`SendRefMsgRange`(12) 格的
//!   方格内、`!Ghost && Race==Play` 的 actor 各来一发（**含说话者自己**：oracle 不排除 self，
//!   客户端正是靠这条回声渲染自己发出去的话）；
//! - `PlayObject.Message.cs:1705-1707`：`RM_HEAR` ⇒ `MakeMessage(SM_HEAR, ActorId, MakeWord(FColor,
//!   BColor), 0, 1)` + 尾 `EncodeString(processMsg.Msg)`。
//!
//! **本切片未接（登记，见 `docs/M2-聊天-普通说话-20261012.md`）**：喊话/组队/行会 `!`、
//! `@` 命令系统、GM 红字、`SayMsgTime/SayMsgCount/DisableSayMsg` 频率限制、`FilterSendMsg`、
//! `DenySayMsgList`、地图 `boNOCHAT`、`SayMsgMaxLen` 截断、`@@加速处理` 特判、怪物 `WantRefMsg` 支；
//! `SendRefMsgTick` 的 500ms 收件人缓存也**未接** ⇒ 本切片恒走 oracle 的**冷路径**（`<= 12` 格）；
//! oracle 缓存命中时改用 `< 11` 格，两者只在**恰好 11/12 格**的近邻上收件人集合不同（连发才命中缓存）。
//!
//! **已接（orphan 编码器接线首批）**：私聊 `/`（[`parse_whisper`]/[`whisper_text`]）
//! ⇒ `SM_WHISPER(103)`，发射点见 `main.rs::handle_whisper`。

use mir2_world::{World, ACTOR_RACE_PLAY};

/// `Messages.CM_SAY`（`Messages.cs:147`）。
pub const CM_SAY: u16 = 3030;

/// `[Setup] SendRefMsgRange`（`GameSvrConf.cs:1503` 缺省 12；现网 `Server.conf` 同为 12）。
///
/// 本切片按常量取用（配置读取登记为后续项，见模块头）；现网与缺省一致 ⇒ 行为等价。
pub const SEND_REF_MSG_RANGE: i32 = 12;

/// `PlayObject.ProcessUserLineMsg` 的"不以 `@` 开头"这一支；`ProcessSayMsg` 再按首字符分流，
/// 本切片只接**非频道**的普通行（`/`、`!`、空行都不接，交给后续片）。
#[must_use]
pub fn is_plain_say(line: &str) -> bool {
    !line.is_empty() && !line.starts_with('@') && !line.starts_with('/') && !line.starts_with('!')
}

/// `PlayObject.Chat.cs:157-166` 的 `/` 私聊支：
///
/// ```text
/// case '/':
///     sText = sData[1..];
///     sText = HUtil32.GetValidStr3(sText, ref sParam1, ' ');
///     if (!FilterSendMsg) Whisper(sParam1, sText);
///     return;
/// ```
///
/// 返回 `Some((目标名, 正文))`；首字符非 `/` ⇒ `None`。切分语义走 [`mir2_shared::hutil32::get_valid_str3`]
/// （与 oracle `.NET Split(div, 2, RemoveEmptyEntries)` 逐例对齐，连续分隔符不留进返回值）。
///
/// `FilterSendMsg` 支（只发自己 `RM_HEAR`）未建模 ⇒ 登记（属 V1-3 已登记片，非本批）。
#[must_use]
pub fn parse_whisper(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('/')?;
    Some(mir2_shared::hutil32::get_valid_str3(rest, &[' ']))
}

/// `PlayObject.Chat.cs:57` 的私聊文本 `ChrName + "=> " + saystr`（**含** `=>` 后的一个空格；
/// GM 支 `:45` 同形）。GBK 字节拼接，本层不转 UTF-8。
#[must_use]
pub fn whisper_text(chr_name: &[u8], say: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(chr_name.len() + 3 + say.len());
    v.extend_from_slice(chr_name);
    v.extend_from_slice(b"=> ");
    v.extend_from_slice(say);
    v
}

/// `BaseObject.Base.cs:475-479` 的 `sChrName + ':' + sMsg`（GBK 字节拼接，不在本层做编码）。
#[must_use]
pub fn hear_text(chr_name: &[u8], say: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(chr_name.len() + 1 + say.len());
    v.extend_from_slice(chr_name);
    v.push(b':');
    v.extend_from_slice(say);
    v
}

/// `SendRefMsg` 的收件人集合（`BaseObject.Message.cs:395-440`）：
/// 以说话者为中心 ±`range` 格的方格内、`Race==Play && !Ghost` 的 actor，**含说话者自己**。
/// `ObMode || FixedHideMode` ⇒ 只发自己（`Message.cs:390-394`）。
///
/// 返回按 `ActorId` 升序（本项目要求顺序显式化；oracle 的格子扫描序不同，但收件人集合相同，
/// 帧内容逐字节相同）。
#[must_use]
pub fn hear_targets(world: &World, speaker_id: i32, range: i32) -> Vec<i32> {
    let Some(speaker) = world.entities.get(speaker_id) else {
        return Vec::new();
    };
    hear_targets_at(
        world,
        speaker_id,
        speaker.map_id,
        speaker.x,
        speaker.y,
        range,
    )
}

/// [`hear_targets`] 的**显式坐标**版：收件人按传入的 `(map_id, x, y)` 计算，而不是实体的当前坐标。
///
/// 用于「实体坐标还没写回、但帧必须按该步位置发」的场合——典型是 `CharPushed` 的逐格
/// `SendRefMsg(RM_PUSH, …)`（`BaseObject.cs:871`，oracle 每推进一格立即发，收件人按**该格**算）。
#[must_use]
pub fn hear_targets_at(
    world: &World,
    speaker_id: i32,
    map_id: usize,
    x: i32,
    y: i32,
    range: i32,
) -> Vec<i32> {
    let Some(speaker) = world.entities.get(speaker_id) else {
        return Vec::new();
    };
    if speaker.ob_mode || speaker.fixed_hide_mode {
        return vec![speaker_id];
    }
    let mut out: Vec<i32> = world
        .entities
        .iter()
        .filter(|e| {
            e.map_id == map_id
                && e.race == ACTOR_RACE_PLAY
                && !e.ghost
                && (e.x - x).abs() <= range
                && (e.y - y).abs() <= range
        })
        .map(|e| e.id)
        .collect();
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_world::{Entity, World};

    fn put(world: &mut World, name: &str, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, name.to_string(), x, y);
        e.race = ACTOR_RACE_PLAY;
        world.add_entity(e)
    }

    #[test]
    fn plain_say_only() {
        assert!(is_plain_say("hello"));
        assert!(is_plain_say("你好"));
        assert!(!is_plain_say(""));
        assert!(!is_plain_say("@move 1 1")); // 命令系统（`ProcessUserLineMsg` 的 `@` 支）
        assert!(!is_plain_say("/whisper hi")); // 私聊
        assert!(!is_plain_say("!shout")); // 喊话/组队/行会
    }

    #[test]
    fn hear_text_is_name_colon_say() {
        assert_eq!(hear_text(b"aaa", b"hi"), b"aaa:hi".to_vec());
        assert_eq!(hear_text(b"", b"x"), b":x".to_vec());
    }

    /// `parse_whisper` 逐例对齐 oracle `GetValidStr3(sText, ref sParam1, ' ')`：
    /// `/目标名 正文` ⇒ `(目标名, 正文)`；`/目标名`（无正文）⇒ `(目标名, "")`；
    /// 连续空格不留进正文；非 `/` 开头 ⇒ `None`。
    #[test]
    fn parse_whisper_splits_name_and_text() {
        assert_eq!(
            parse_whisper("/bob hello world"),
            Some(("bob".to_string(), "hello world".to_string()))
        );
        assert_eq!(
            parse_whisper("/bob"),
            Some(("bob".to_string(), String::new()))
        );
        // 连续分隔符：oracle `.NET Split(div, 2, RemoveEmptyEntries)` ⇒ 正文不带前导空格。
        assert_eq!(
            parse_whisper("/bob  hi"),
            Some(("bob".to_string(), "hi".to_string()))
        );
        assert_eq!(parse_whisper("hello"), None);
        assert_eq!(parse_whisper(""), None);
    }

    #[test]
    fn whisper_text_is_name_arrow_space_say() {
        assert_eq!(whisper_text(b"bob", b"hi"), b"bob=> hi".to_vec());
        assert_eq!(whisper_text(b"", b""), b"=> ".to_vec());
    }

    /// 收件人 = ±range 格内的 Play 且非 Ghost，**含自己**；远者与怪物不收。
    #[test]
    fn hear_targets_include_self_and_near_players_only() {
        let mut world = World::new(100, 100);
        let a = put(&mut world, "a", 10, 10); // 说话者
        let b = put(&mut world, "b", 22, 22); // dx=dy=12 ⇒ 边界内
        let c = put(&mut world, "c", 23, 10); // dx=13 ⇒ 界外
                                              // 怪物（Race != Play）即便在格内也不收（本切片不接 WantRefMsg）
        let mut mon = Entity::new(0, "mon".to_string(), 11, 10);
        mon.race = mir2_world::ACTOR_RACE_MONSTER;
        let m = world.add_entity(mon);
        let mut ghost = Entity::new(0, "g".to_string(), 12, 10);
        ghost.ghost = true;
        let g = world.add_entity(ghost);

        let got = hear_targets(&world, a, SEND_REF_MSG_RANGE);
        assert_eq!(
            got,
            vec![a, b],
            "含自己 + 边界内 Play；界外 c / 怪物 / Ghost 不收"
        );
        assert!(!got.contains(&c) && !got.contains(&m) && !got.contains(&g));

        // ObMode ⇒ 只发自己（`Message.cs:390-394`）
        world.entities.get_mut(a).unwrap().ob_mode = true;
        assert_eq!(hear_targets(&world, a, SEND_REF_MSG_RANGE), vec![a]);
    }

    /// 显式坐标版：收件人按传入的 `(map_id,x,y)` 算，不看实体当前坐标。
    /// `CharPushed` 逐格发 `RM_PUSH` 时实体坐标尚未写回最终值，必须用该步坐标。
    #[test]
    fn hear_targets_at_uses_given_coords_not_entity_position() {
        let mut world = World::new(64, 64);
        let speaker = put(&mut world, "speaker", 10, 10);
        let observer = put(&mut world, "observer", 22, 10); // 距实体坐标 (10,10) 恰 12
        assert_eq!(
            hear_targets(&world, speaker, SEND_REF_MSG_RANGE),
            vec![speaker, observer],
            "实体坐标口径：observer 在 ±12 边界内"
        );
        // 显式坐标 (9,10)：observer 距离 13 ⇒ 出界（实体仍在 (10,10)，证明没有读实体坐标）。
        assert_eq!(
            hear_targets_at(&world, speaker, 0, 9, 10, SEND_REF_MSG_RANGE),
            vec![speaker],
            "显式坐标口径：observer 出界必须被剔除"
        );
        // 显式坐标 (11,10)：observer 距离 11 ⇒ 仍在（且含自己）。
        assert_eq!(
            hear_targets_at(&world, speaker, 0, 11, 10, SEND_REF_MSG_RANGE),
            vec![speaker, observer]
        );
    }

    #[test]
    fn hear_targets_unknown_speaker_is_empty() {
        let world = World::new(10, 10);
        assert!(hear_targets(&world, 999, SEND_REF_MSG_RANGE).is_empty());
    }
}
