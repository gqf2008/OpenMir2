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
//! **频道入口（本批 `openmir2-channel-chat`）**：`!!` 组队、`!~` 行会、`!` 喊话
//! ⇒ [`parse_channel`] + [`channel_text`]/[`cry_text`]/[`cry_targets`]/[`shout_decision`]；
//! 发射点在 `main.rs::handle_say`。逐条照 `PlayObject.Chat.cs:163-208`：
//! - `!!` ⇒ `SendGroupText(ChrName + ": " + sText)`（`PlayObject.cs:5185-5197`，前缀
//!   `GroupMsgPreFix` 由世界侧 [`mir2_world::World::party_send_group_text`] 拼）；
//! - `!~` **且** `MyGuild != null` ⇒ `MyGuild.SendGuildMsg(...)`（`GuildInfo.cs:431-456`）；
//!   无行会时 `case '~' when MyGuild != null` 不命中 ⇒ 落**喊话**支（正文以 `~` 开头）；
//! - 其余 `!` ⇒ 喊话：`boQUIZ` 门 → 10s 门（**严格 `>`**）→ 等级门
//!   （`Abil.Level <= CanShoutMsgLevel`）→ `CryCry(..., nWide=50, ...)`。
//!
//! **登记/偏差（见模块 `main.rs` 与 `whitelist.md` T-118）**：oracle 的 `CryCry` 对每个收件人调
//! `SendMsg(null, RM_CRY, …)`，而 `BaseObject.Message.cs:154` 直接解引用 `baseObject.ActorId`
//! ⇒ 空引用被 `ProcessSayMsg` 的 catch 吞掉（字面行为 = 不出任何帧）；本仓按 `CryCry` 的**意图**
//! （源 = null ⇒ `Recog=0`）投递。`BanShout` 唯一写点是 GM 命令/跨服 `SwitchData`（未移植）
//! ⇒ 恒 true（`PlayObject.cs:1006`），本仓不建模该门。
//!
//! **仍未接（登记）**：`@` 命令系统、GM 红字、`SayMsgTime/SayMsgCount/DisableSayMsg` 频率限制、
//! `FilterSendMsg`、`DenySayMsgList`、地图 `boNOCHAT`、`SayMsgMaxLen` 截断、`@@加速处理` 特判、
//! 怪物 `WantRefMsg` 支；`SS_208` 跨服无接收端（T-113）。`SendRefMsgTick` 的 500ms 收件人缓存
//! 也**未接** ⇒ 普通说话恒走 oracle 的**冷路径**（`<= 12` 格）；缓存命中时改用 `< 11` 格，
//! 两者只在**恰好 11/12 格**的近邻上收件人集合不同（连发才命中缓存）。
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

/// 喊话的 10s 门（`PlayObject.Chat.cs:183`：`(GetTickCount() - ShoutMsgTick) > 10 * 1000`，**严格大于**）。
pub const SHOUT_INTERVAL_MS: i64 = 10_000;

/// `CryCry(..., nWide=50, ...)` 的喊话半径（`PlayObject.Chat.cs:199`）。判定是
/// `|dx| < nWide && |dy| < nWide`（`src/GameSrv/Word/WorldServer.cs:755-764`，**严格小于**，
/// 与 `SendRefMsg` 的 `<=` 不同）。
pub const SEND_CRY_RANGE: i32 = 50;

/// `ProcessSayMsg` 的 `switch (sData[0])` 的 `'!'` 支（`PlayObject.Chat.cs:163-208`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelSay<'a> {
    /// `!!` ⇒ `SendGroupText(ChrName + ": " + sText)`（`:167-169`）。
    Group(&'a str),
    /// `!~` **且 `MyGuild != null`** ⇒ `MyGuild.SendGuildMsg(...)`（`:171-176`）。
    Guild(&'a str),
    /// 其余 `!`（**含无行会时的 `!~`**）⇒ 喊话（`:179-208`）。
    Shout(&'a str),
}

/// 解析 `!` 频道行；`has_guild` = `PlayObject.MyGuild != null`。
///
/// 逐条照 `PlayObject.Chat.cs:163-208`：`!!` 组队、有行会时的 `!~` 行会、其余 `!` 喊话
/// （无行会的 `!~` 落到喊话，正文以 `~` 开头——C# 的 `case '~' when MyGuild != null` 不命中）。
///
/// 单个 `!`（长度 1）：oracle `sData[1]` 抛 `IndexOutOfRangeException`，被 `ProcessSayMsg`
/// 的 catch 吞掉 ⇒ **无任何输出**；本函数返回 `None`（宿主记 DBG）。
#[must_use]
pub fn parse_channel(line: &str, has_guild: bool) -> Option<ChannelSay<'_>> {
    let mut chars = line.chars();
    if chars.next() != Some('!') {
        return None;
    }
    match chars.next() {
        Some('!') => Some(ChannelSay::Group(&line[2..])),
        Some('~') if has_guild => Some(ChannelSay::Guild(&line[2..])),
        Some(_) => Some(ChannelSay::Shout(&line[1..])),
        None => None,
    }
}

/// 组队/行会正文 `ChrName + ": " + sText`（`PlayObject.Chat.cs:168/174`；GBK 字节拼接）。
#[must_use]
pub fn channel_text(chr_name: &[u8], say: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(chr_name.len() + 2 + say.len());
    v.extend_from_slice(chr_name);
    v.extend_from_slice(b": ");
    v.extend_from_slice(say);
    v
}

/// 喊话正文 `"(!)" + ChrName + ": " + sText`（`PlayObject.Chat.cs:192`；GBK 字节拼接）。
#[must_use]
pub fn cry_text(chr_name: &[u8], say: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(3 + chr_name.len() + 2 + say.len());
    v.extend_from_slice(b"(!)");
    v.extend_from_slice(chr_name);
    v.extend_from_slice(b": ");
    v.extend_from_slice(say);
    v
}

/// `WorldServer.CryCry` 的收件人集合（`src/GameSrv/Word/WorldServer.cs:755-764`）：
/// 同图、`!Ghost`、`|dx| < range && |dy| < range`（**严格小于**）。`PlayObjectList` 只含玩家
/// ⇒ 这里用 `Race == Play` 过滤；`BanShout` 恒 true（见模块头登记）。**含说话者自己**。
#[must_use]
pub fn cry_targets(world: &World, speaker_id: i32, range: i32) -> Vec<i32> {
    let Some(speaker) = world.entities.get(speaker_id) else {
        return Vec::new();
    };
    let mut out: Vec<i32> = world
        .entities
        .iter()
        .filter(|e| {
            e.map_id == speaker.map_id
                && e.race == ACTOR_RACE_PLAY
                && !e.ghost
                && (e.x - speaker.x).abs() < range
                && (e.y - speaker.y).abs() < range
        })
        .map(|e| e.id)
        .collect();
    out.sort_unstable();
    out
}

/// 喊话支的判定结果（`PlayObject.Chat.cs:179-208` 的分支树，按 oracle 的判定**顺序**）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShoutDecision {
    /// 三门全过 ⇒ `ShoutMsgTick = now` 并广播 `RM_CRY`。
    Send,
    /// 10s 门未过 ⇒ `SysMsg(YouCanSendCyCyLaterMsg(剩余秒), Red, Hint)`；`0` 值 = 剩余秒。
    TooSoon(i32),
    /// `Abil.Level <= CanShoutMsgLevel` ⇒ `SysMsg(YouNeedLevelMsg(CanShoutMsgLevel+1), Red, Hint)`。
    LevelTooLow(i32),
    /// `Envir.Flag.boQUIZ` ⇒ `SysMsg(ThisMapDisableSendCyCyMsg, Red, Hint)`。
    MapDisable,
}

/// `ProcessSayMsg` 的 `!` 喊话三门（顺序照 oracle）：
/// `!boQUIZ` → `now - ShoutMsgTick > 10_000` → `Abil.Level > CanShoutMsgLevel`。
#[must_use]
pub fn shout_decision(
    now_ms: i64,
    last_tick: i64,
    level: u8,
    can_shout_level: i32,
    map_quiz: bool,
) -> ShoutDecision {
    if map_quiz {
        return ShoutDecision::MapDisable;
    }
    let elapsed = now_ms - last_tick;
    if elapsed > SHOUT_INTERVAL_MS {
        if i32::from(level) <= can_shout_level {
            ShoutDecision::LevelTooLow(can_shout_level + 1)
        } else {
            ShoutDecision::Send
        }
    } else {
        // `10 - (now - tick) / 1000`（C# int 除法，向零截断）。
        let secs = i32::try_from(elapsed / 1000).unwrap_or(10);
        ShoutDecision::TooSoon(10 - secs)
    }
}

/// `BaseObject.Format(str, par)`（`BaseObject.cs:2851`）的 1:1 子集：只替换 `{0}`。
///
/// C# 的 `string.Format` 对**没有** `{0}` 占位符的模板原样返回——现网 `String.conf:244-246`
/// 用的是 `%d`（`YouCanSendCyCyLaterMsg=%d秒后...`）⇒ oracle 的提示文本里 `%d` **不被替换**。
/// 本函数同样不碰 `%`。
#[must_use]
pub fn format_int(template: &str, value: i32) -> String {
    template.replace("{0}", &value.to_string())
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
    #[test]
    fn parse_channel_covers_all_branches() {
        assert_eq!(parse_channel("!!hi", false), Some(ChannelSay::Group("hi")));
        assert_eq!(parse_channel("!!", true), Some(ChannelSay::Group("")));
        // 有行会 ⇒ `!~` 走行会
        assert_eq!(parse_channel("!~hi", true), Some(ChannelSay::Guild("hi")));
        // 无行会 ⇒ `case '~' when MyGuild != null` 不命中，落喊话且正文保留 `~`
        assert_eq!(parse_channel("!~hi", false), Some(ChannelSay::Shout("~hi")));
        assert_eq!(parse_channel("!hi", true), Some(ChannelSay::Shout("hi")));
        assert_eq!(
            parse_channel("!", true),
            None,
            "oracle 单个 ! 下标越界被 catch"
        );
        assert_eq!(parse_channel("@move 1 1", true), None);
        assert_eq!(parse_channel("hi", true), None);
        assert_eq!(parse_channel("", true), None);
    }

    #[test]
    fn channel_and_cry_text_are_gbk_byte_concats() {
        assert_eq!(channel_text(b"aaa", b"hi"), b"aaa: hi".to_vec());
        assert_eq!(cry_text(b"aaa", b"hi"), b"(!)aaa: hi".to_vec());
        assert_eq!(cry_text(b"", b""), b"(!): ".to_vec());
    }

    /// `CryCry` 的半径是**严格 `< 50`**（与 [`hear_targets`] 的 `<= 12` 不同）；
    /// 同图 + 非 Ghost + 玩家；含自己。
    #[test]
    fn cry_targets_strict_range_50() {
        let mut world = World::new(200, 200);
        let a = put(&mut world, "a", 100, 100);
        let inside = put(&mut world, "in", 149, 100); // dx=49 ⇒ 收
        let edge = put(&mut world, "edge", 150, 100); // dx=50 ⇒ 不收（严格 <）
        let mut other_map = Entity::new(0, "map".to_string(), 100, 100);
        other_map.race = ACTOR_RACE_PLAY;
        other_map.map_id = 1;
        let om = world.add_entity(other_map);
        let mut ghost = Entity::new(0, "g".to_string(), 101, 100);
        ghost.race = ACTOR_RACE_PLAY;
        ghost.ghost = true;
        let g = world.add_entity(ghost);
        let mut mon = Entity::new(0, "mon".to_string(), 102, 100);
        mon.race = mir2_world::ACTOR_RACE_MONSTER;
        let m = world.add_entity(mon);

        let got = cry_targets(&world, a, SEND_CRY_RANGE);
        assert_eq!(
            got,
            vec![a, inside],
            "含自己 + dx=49；50/异图/Ghost/怪物不收"
        );
        assert!(
            !got.contains(&edge) && !got.contains(&om) && !got.contains(&g) && !got.contains(&m)
        );
        assert!(cry_targets(&world, 999, SEND_CRY_RANGE).is_empty());
    }

    /// 10s 门：`(now - tick) > 10_000` **严格大于**；剩余秒 = `10 - elapsed/1000`。
    /// 首次喊话 `ShoutMsgTick=0` ⇒ 服务器启动后 10s 内被拒（与 oracle 的 ctor 零值一致）。
    #[test]
    fn shout_decision_time_gate_is_strict_and_reports_remaining_seconds() {
        assert_eq!(
            shout_decision(0, 0, 40, 34, false),
            ShoutDecision::TooSoon(10)
        );
        assert_eq!(
            shout_decision(10_000, 0, 40, 34, false),
            ShoutDecision::TooSoon(0),
            "恰好 10.000s 仍不过（C# 是 `>`）"
        );
        assert_eq!(
            shout_decision(10_001, 0, 40, 34, false),
            ShoutDecision::Send
        );
        assert_eq!(
            shout_decision(29_999, 20_000, 40, 34, false),
            ShoutDecision::TooSoon(1),
            "9.999s / 1000 = 9 ⇒ 10-9 = 1"
        );
        assert_eq!(
            shout_decision(30_001, 20_000, 40, 34, false),
            ShoutDecision::Send
        );
    }

    /// 等级门在时间门**之后**：`Abil.Level <= CanShoutMsgLevel` ⇒ 提示 `CanShoutMsgLevel+1`；
    /// 等于也拒（`<=`）。
    #[test]
    fn shout_decision_level_gate_is_after_time_gate_and_inclusive() {
        assert_eq!(
            shout_decision(10_001, 0, 34, 34, false),
            ShoutDecision::LevelTooLow(35),
            "level == CanShoutMsgLevel ⇒ 拒"
        );
        assert_eq!(
            shout_decision(10_001, 0, 35, 34, false),
            ShoutDecision::Send
        );
        // 时间门未过时先报时间，不报等级。
        assert_eq!(
            shout_decision(5_000, 0, 1, 34, false),
            ShoutDecision::TooSoon(5)
        );
    }

    #[test]
    fn shout_decision_quiz_map_blocks_first() {
        assert_eq!(
            shout_decision(1_000_000, 0, 99, 0, true),
            ShoutDecision::MapDisable,
            "boQUIZ 优先于时间/等级门"
        );
    }

    #[test]
    fn format_int_substitutes_braces_not_percent() {
        assert_eq!(
            format_int("{0}秒后才可以再发文字!!!", 3),
            "3秒后才可以再发文字!!!"
        );
        assert_eq!(
            format_int("%d秒后才可以再发文字!!!", 3),
            "%d秒后才可以再发文字!!!",
            "现网 String.conf 的 `%d` 在 C# string.Format 下不被替换"
        );
        assert_eq!(format_int("no placeholder", 1), "no placeholder");
    }
}
