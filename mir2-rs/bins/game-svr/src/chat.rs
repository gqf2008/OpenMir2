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
//! **本切片未接（登记，见 `docs/M2-聊天-普通说话-20261012.md`）**：私聊 `/`、喊话/组队/行会 `!`、
//! `@` 命令系统、GM 红字、`SayMsgTime/SayMsgCount/DisableSayMsg` 频率限制、`FilterSendMsg`、
//! `DenySayMsgList`、地图 `boNOCHAT`、`SayMsgMaxLen` 截断、`@@加速处理` 特判、怪物 `WantRefMsg` 支；
//! `SendRefMsgTick` 的 500ms 收件人缓存也**未接** ⇒ 本切片恒走 oracle 的**冷路径**（`<= 12` 格）；
//! oracle 缓存命中时改用 `< 11` 格，两者只在**恰好 11/12 格**的近邻上收件人集合不同（连发才命中缓存）。

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
    if speaker.ob_mode || speaker.fixed_hide_mode {
        return vec![speaker_id];
    }
    let (sx, sy, map) = (speaker.x, speaker.y, speaker.map_id);
    let mut out: Vec<i32> = world
        .entities
        .iter()
        .filter(|e| {
            e.map_id == map
                && e.race == ACTOR_RACE_PLAY
                && !e.ghost
                && (e.x - sx).abs() <= range
                && (e.y - sy).abs() <= range
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

    #[test]
    fn hear_targets_unknown_speaker_is_empty() {
        let world = World::new(10, 10);
        assert!(hear_targets(&world, 999, SEND_REF_MSG_RANGE).is_empty());
    }
}
