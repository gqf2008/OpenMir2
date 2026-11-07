//! oracle `PlanesMessage.ProcessData` 的等价物（G11 / A2-3）。
//!
//! # oracle 的实际语义
//!
//! `ProcessData` 是 41 个 case 的 `switch`（40 个 ident 有处理器 + 242 空处理；
//! `ISM_LM_DELETE = 226` 有常量但**没有 case**）。把 32 个 handler 逐个读完后，
//! **没有任何一个产生可观察的下游副作用**：
//!
//! | 形态 | 方法数 | 说明 |
//! |---|---:|---|
//! | 函数体为空 / 只有注释 | 11 | 如 `ServerHeartMessage`、`MsgGetSysopMsg` |
//! | 只有局部变量赋值（含 `x = Body`） | 17 | 赋给从未被读的局部量 |
//! | 调了 `GetValidStr3` 但结果丢弃 | 3 | 205 `MsgGetAddGuild`、209 `MsgGetChatProhibition`、214–221 `MsgGetUserMgr` |
//! | 直接 `throw` | 1 | 211 `MsgGetChangeCastleOwner` |
//!
//! 因此 `ProcessData` 在这个 oracle 里的**唯一可观察行为**就是 [`Effect::Throws`]（211）——
//! 而它的异常会被 `DecodeSocStr` 的 `catch` 吞掉，表现为**该批剩余帧一并丢弃**。
//!
//! 205 / 209 / 214–221 的 `GetValidStr3` 调用**确实执行**（不在 `/* */` 里），只是结果无人消费。
//! 本模块把它们提供成纯函数，供 A4 真正接线时复用，并用测试按 oracle 语义钉住。
//!
//! # 一个容易搞错的点
//!
//! A1 设计文档 §3 的「body 字段」列有 20 多个 ident 写了字段名——那些**全部来自注释掉的代码**，
//! 运行时不会执行。本模块以**源码里实际执行的语句**为准，设计文档已同步更正。

use mir2_protocol::messages as msg;

use crate::get_valid_str3;

/// oracle `ProcessData` 对某 ident 的**可观察效果**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// `switch` 命中，且无任何可观察副作用（空实现 / 局部赋值 / 解析后丢弃）。
    Handled,
    /// `switch` 未命中。C# 的 `switch` 没有 `default`，未命中即什么都不做。
    /// 与 [`Effect::Handled`] **可观察等价**，分开只是为了核对覆盖面。
    Unknown,
    /// `throw new Exception(...)`：`MsgGetChangeCastleOwner`（211）。
    Throws,
}

/// `Handled` 的 ident（40 个：41 个 case 减去抛异常的 211）。
const HANDLED: &[u16] = &[
    msg::ISM_GROUPSERVERHEART,
    msg::ISM_USERSERVERCHANGE,
    msg::ISM_CHANGESERVERRECIEVEOK,
    msg::ISM_USERLOGON,
    msg::ISM_USERLOGOUT,
    msg::ISM_WHISPER,
    msg::ISM_GMWHISPER,
    msg::ISM_LM_WHISPER,
    msg::ISM_SYSOPMSG,
    msg::ISM_ADDGUILD,
    msg::ISM_DELGUILD,
    msg::ISM_RELOADGUILD,
    msg::ISM_GUILDMSG,
    msg::ISM_GUILDWAR,
    msg::ISM_CHATPROHIBITION,
    msg::ISM_CHATPROHIBITIONCANCEL,
    msg::ISM_RELOADCASTLEINFO,
    msg::ISM_RELOADADMIN,
    msg::ISM_MARKETOPEN,
    msg::ISM_MARKETCLOSE,
    msg::ISM_RELOADCHATLOG,
    msg::ISM_USER_INFO,
    msg::ISM_FRIEND_INFO,
    msg::ISM_FRIEND_DELETE,
    msg::ISM_FRIEND_OPEN,
    msg::ISM_FRIEND_CLOSE,
    msg::ISM_FRIEND_RESULT,
    msg::ISM_TAG_SEND,
    msg::ISM_TAG_RESULT,
    msg::ISM_RELOADMAKEITEMLIST,
    msg::ISM_GUILDMEMBER_RECALL,
    msg::ISM_RELOADGUILDAGIT,
    msg::ISM_LM_LOGIN,
    msg::ISM_LM_LOGOUT,
    msg::ISM_LM_LOGIN_REPLY,
    msg::ISM_LM_KILLED_MSG,
    msg::ISM_RECALL,
    msg::ISM_REQUEST_RECALL,
    msg::ISM_REQUEST_LOVERRECALL,
    msg::ISM_GRUOPMESSAGE,
];

/// `Throws` 的 ident（1 个）。
const THROWS: &[u16] = &[msg::ISM_CHANGECASTLEOWNER];

/// 某 ident 在 oracle `ProcessData` 里的效果。
///
/// `ident` 是 `i32`（帧里的值由 `StrToInt` 得到，可为负）：负值或大于 `u16::MAX`
/// 一律 [`Effect::Unknown`]。
#[must_use]
pub fn effect_of(ident: i32) -> Effect {
    let Ok(id) = u16::try_from(ident) else {
        return Effect::Unknown;
    };
    if THROWS.contains(&id) {
        Effect::Throws
    } else if HANDLED.contains(&id) {
        Effect::Handled
    } else {
        Effect::Unknown
    }
}

/// `ProcessData` 的失败：只有 211 一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    /// `ISM_CHANGECASTLEOWNER`（211）：oracle 直接 `throw`。
    ///
    /// 调用方（`DecodeSocStr` 的 try/catch）会**丢弃该批剩余帧**；`PlanesServer` 静默，
    /// `PlanesClient` 额外写 `LogService.Error`。这两条差异归 A3。
    ChangeCastleOwner,
}

/// oracle `PlanesMessage.ProcessData(ident, serverNum, body)` 的等价物。
///
/// `server_num` 不参与判定（oracle 里它只作为未使用形参传下去），故不接收。
///
/// # Errors
/// 仅 [`ProcessError::ChangeCastleOwner`]。
pub fn process(ident: i32) -> Result<(), ProcessError> {
    match effect_of(ident) {
        Effect::Throws => Err(ProcessError::ChangeCastleOwner),
        Effect::Handled | Effect::Unknown => Ok(()),
    }
}

/// `MsgGetAddGuild`（205）执行的解析：`(gname, mname)`，`mname` 是余量。
///
/// oracle 里两者都无人消费（`GuildMgr.AddGuild` 被注释），这里保留原始语义供 A4 复用。
#[must_use]
pub fn add_guild_fields(body: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut gname = Vec::new();
    let mname = get_valid_str3(body, &mut gname);
    (gname, mname)
}

/// `MsgGetChatProhibition`（209）执行的解析：`(whostr, minstr, 余量)`。
///
/// oracle 只判空 `whostr` 后什么都没做（`CmdShutup` 两行都被注释）。
#[must_use]
pub fn chat_prohibition_fields(body: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut whostr = Vec::new();
    let rest = get_valid_str3(body, &mut whostr);
    let mut minstr = Vec::new();
    let rest = get_valid_str3(&rest, &mut minstr);
    (whostr, minstr, rest)
}

/// `MsgGetUserMgr`（214–221，8 个 ident 共用）执行的解析：`(UserName, msgbody)`。
///
/// oracle 里 `UserMgrEngine.OnExternInterMsg` 被注释，两者都丢弃。
#[must_use]
pub fn user_mgr_fields(body: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut user_name = Vec::new();
    let msgbody = get_valid_str3(body, &mut user_name);
    (user_name, msgbody)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// oracle `ProcessData` 的 41 个 case，逐个点名（顺序与 `PlanesMessage.cs` 的 switch 一致）。
    const ORACLE_CASES: &[(u16, Effect)] = &[
        (msg::ISM_GROUPSERVERHEART, Effect::Handled),
        (msg::ISM_USERSERVERCHANGE, Effect::Handled),
        (msg::ISM_CHANGESERVERRECIEVEOK, Effect::Handled),
        (msg::ISM_USERLOGON, Effect::Handled),
        (msg::ISM_USERLOGOUT, Effect::Handled),
        (msg::ISM_WHISPER, Effect::Handled),
        (msg::ISM_GMWHISPER, Effect::Handled),
        (msg::ISM_LM_WHISPER, Effect::Handled),
        (msg::ISM_SYSOPMSG, Effect::Handled),
        (msg::ISM_ADDGUILD, Effect::Handled),
        (msg::ISM_DELGUILD, Effect::Handled),
        (msg::ISM_RELOADGUILD, Effect::Handled),
        (msg::ISM_GUILDMSG, Effect::Handled),
        (msg::ISM_GUILDWAR, Effect::Handled),
        (msg::ISM_CHATPROHIBITION, Effect::Handled),
        (msg::ISM_CHATPROHIBITIONCANCEL, Effect::Handled),
        (msg::ISM_CHANGECASTLEOWNER, Effect::Throws),
        (msg::ISM_RELOADCASTLEINFO, Effect::Handled),
        (msg::ISM_RELOADADMIN, Effect::Handled),
        (msg::ISM_MARKETOPEN, Effect::Handled),
        (msg::ISM_MARKETCLOSE, Effect::Handled),
        (msg::ISM_RELOADCHATLOG, Effect::Handled),
        (msg::ISM_USER_INFO, Effect::Handled),
        (msg::ISM_FRIEND_INFO, Effect::Handled),
        (msg::ISM_FRIEND_DELETE, Effect::Handled),
        (msg::ISM_FRIEND_OPEN, Effect::Handled),
        (msg::ISM_FRIEND_CLOSE, Effect::Handled),
        (msg::ISM_FRIEND_RESULT, Effect::Handled),
        (msg::ISM_TAG_SEND, Effect::Handled),
        (msg::ISM_TAG_RESULT, Effect::Handled),
        (msg::ISM_RELOADMAKEITEMLIST, Effect::Handled),
        (msg::ISM_GUILDMEMBER_RECALL, Effect::Handled),
        (msg::ISM_RELOADGUILDAGIT, Effect::Handled),
        (msg::ISM_LM_LOGIN, Effect::Handled),
        (msg::ISM_LM_LOGOUT, Effect::Handled),
        (msg::ISM_LM_LOGIN_REPLY, Effect::Handled),
        (msg::ISM_LM_KILLED_MSG, Effect::Handled),
        (msg::ISM_RECALL, Effect::Handled),
        (msg::ISM_REQUEST_RECALL, Effect::Handled),
        (msg::ISM_REQUEST_LOVERRECALL, Effect::Handled),
        (msg::ISM_GRUOPMESSAGE, Effect::Handled),
    ];

    #[test]
    fn dispatch_table_matches_oracle_switch() {
        // 41 个 case，恰好 1 个 Throws。
        assert_eq!(ORACLE_CASES.len(), 41);
        let throws = ORACLE_CASES
            .iter()
            .filter(|(_, e)| *e == Effect::Throws)
            .count();
        assert_eq!(throws, 1);
        for (ident, want) in ORACLE_CASES {
            assert_eq!(effect_of(i32::from(*ident)), *want, "ident {ident}");
            assert_eq!(
                process(i32::from(*ident)).is_err(),
                *want == Effect::Throws,
                "process({ident})"
            );
        }
        // 常量表与用例表集合相等，且各自无重复。
        let mut from_cases: Vec<u16> = ORACLE_CASES.iter().map(|(i, _)| *i).collect();
        from_cases.sort_unstable();
        from_cases.dedup();
        assert_eq!(
            from_cases.len(),
            ORACLE_CASES.len(),
            "ORACLE_CASES 有重复 ident"
        );
        let mut from_consts: Vec<u16> = HANDLED.iter().chain(THROWS).copied().collect();
        from_consts.sort_unstable();
        from_consts.dedup();
        assert_eq!(
            from_consts.len(),
            HANDLED.len() + THROWS.len(),
            "常量表有重复"
        );
        assert_eq!(from_consts, from_cases);
    }

    #[test]
    fn idents_without_case_are_unknown_and_noop() {
        // 226 ISM_LM_DELETE 有常量但没有 case；其余为任意值（含越界与负值）。
        for ident in [
            0,
            1,
            99,
            226,
            243,
            255,
            1000,
            -1,
            i32::from(i16::MAX),
            65_536,
        ] {
            assert_eq!(effect_of(ident), Effect::Unknown, "ident {ident}");
            assert_eq!(process(ident), Ok(()), "ident {ident}");
        }
    }

    #[test]
    fn add_guild_fields_follow_oracle_split() {
        // 期望值按 C# `GetValidStr3` 的语义推出（A2-2 已用探针对拍过该函数）；(gname, mname)
        let cases: &[(&[u8], &[u8], &[u8])] = &[
            (b"guild", b"guild", b""),
            (b"guild/msg", b"guild", b"msg"),
            (b"guild//msg", b"guild", b"msg"),
            (b"///msg", b"msg", b""),
            (b"a//b/c", b"a", b"b/c"),
            (b"", b"", b""),
            (b"/", b"", b""),
        ];
        for (body, want_g, want_m) in cases {
            let (g, m) = add_guild_fields(body);
            assert_eq!(g.as_slice(), *want_g, "gname for {body:?}");
            assert_eq!(m.as_slice(), *want_m, "mname for {body:?}");
        }
    }

    /// 四元组用例：(body, whostr, minstr, 余量)。
    type ProhibitionCase = (&'static [u8], &'static [u8], &'static [u8], &'static [u8]);

    #[test]
    fn chat_prohibition_fields_follow_oracle_split() {
        let cases: &[ProhibitionCase] = &[
            (b"who/10", b"who", b"10", b""),
            (b"who", b"who", b"", b""),
            (b"/who//10/x", b"who", b"10", b"x"),
            (b"who//10", b"who", b"10", b""),
            (b"", b"", b"", b""),
        ];
        for (body, want_w, want_m, want_rest) in cases {
            let (w, m, rest) = chat_prohibition_fields(body);
            assert_eq!(w.as_slice(), *want_w, "whostr for {body:?}");
            assert_eq!(m.as_slice(), *want_m, "minstr for {body:?}");
            assert_eq!(rest.as_slice(), *want_rest, "rest for {body:?}");
        }
    }

    #[test]
    fn user_mgr_fields_follow_oracle_split() {
        let cases: &[(&[u8], &[u8], &[u8])] = &[
            (b"alice", b"alice", b""),
            (b"alice/hello", b"alice", b"hello"),
            (b"alice//hello/world", b"alice", b"hello/world"),
            (b"//alice", b"alice", b""),
            (b"", b"", b""),
        ];
        for (body, want_u, want_rest) in cases {
            let (u, rest) = user_mgr_fields(body);
            assert_eq!(u.as_slice(), *want_u, "UserName for {body:?}");
            assert_eq!(rest.as_slice(), *want_rest, "rest for {body:?}");
        }
    }

    #[test]
    fn body_is_bytes_not_utf8() {
        // GB2312 的「老王」是 C0 CF CD F5 —— 本模块不得对它做任何编码转换。
        let body: &[u8] = &[0xC0, 0xCF, 0xCD, 0xF5, b'/', b'3'];
        let (user, rest) = user_mgr_fields(body);
        assert_eq!(user, vec![0xC0, 0xCF, 0xCD, 0xF5]);
        assert_eq!(rest, b"3".to_vec());
        let (g, m) = add_guild_fields(body);
        assert_eq!(g, vec![0xC0, 0xCF, 0xCD, 0xF5]);
        assert_eq!(m, b"3".to_vec());
    }
}
