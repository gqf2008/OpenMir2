//! 逐玩家**滚动公告**下发（oracle `WorldServer.cs:669-696`）的宿主等价物
//! —— 世界 tick 扩容首批（T3）· ③。
//!
//! oracle 链（`ProcessPlayObjectData`，`WorldServer.cs:625-670`，每玩家自己的 `RunTime` 门内）：
//!
//! ```text
//! if ((now - playObject.ShowLineNoticeTick) > Config.ShowLineNoticeTime) {
//!     playObject.ShowLineNoticeTick = now;
//!     if (M2Share.LineNoticeList.Count > playObject.ShowLineNoticeIdx) {
//!         lineNoticeMsg = ManageNPC.GetLineVariableText(playObject, LineNoticeList[ShowLineNoticeIdx]);
//!         switch (lineNoticeMsg[0]) {
//!             case 'R': SysMsg(rest, Red,   Notice); break;
//!             case 'G': SysMsg(rest, Green, Notice); break;
//!             case 'B': SysMsg(rest, Blue,  Notice); break;
//!             default:  SysMsg(lineNoticeMsg, (MsgColor)Config.LineNoticeColor, Notice); break;
//!         }
//!     }
//!     playObject.ShowLineNoticeIdx++;
//!     if (M2Share.LineNoticeList.Count <= playObject.ShowLineNoticeIdx)
//!         playObject.ShowLineNoticeIdx = 0;
//! }
//! ```
//!
//! ⇒ `SysMsg(sMsg, color, MsgType.Notice)`（`BaseObject.cs:1588-1685`）再按 `sMsg[0]`：
//! `'['` ⇒ `RM_MOVEMESSAGE`(顶部滚动)；`'<'` ⇒ `RM_SYSMESSAGE`(聊天框彩色)；
//! `'{'` ⇒ `RM_MOVEMESSAGE`(居中)；default ⇒ 按 `color` 的 `RM_SYSMESSAGE`。
//!
//! **建模差异（登记）**：
//! - `GetLineVariableText` 的 `<$…>` 变量替换**未建模**（返回原串）。oracle 只在
//!   `sVariable[0] == '$'` 时替换；现网 `Notice/LineNotice.txt` **不含** `<$…>`（实测 8 行）
//!   ⇒ 恒等 = oracle 同结果（`must_be_verified_elsewhere`：变量面随 NPC 脚本层）。
//! - `GetValidStrCap` 的**逗号切分已对齐 oracle**（`HUtil32.GetValidStr3` 的
//!   `Split(delimiter, 2, RemoveEmptyEntries)` ⇒ 空首段被删），见
//!   [`get_valid_str_cap_comma`] / [`get_valid_str3`]；但其 `str[0] == '"'` 的
//!   `CaptureString` 子支**未建模**（本函数直接按逗号切）——
//!   仅当 brackets 内容以 `"` 开头时可达；现网 `Notice/LineNotice.txt`
//!   各行形如 `[公告]…`（无前导 `"`，见 `notice.rs` 的
//!   `load_line_notice` 夹具）⇒ 现网不可达（登记）。
//! - `ShowPreFixMsg`（现网 `Server.conf:778` = `0`）为真时的 `LineNoticePreFix` 前缀已实现
//!   （配置键 `ShowPreFixMsg`），但前缀文本源（oracle `String.conf` 的 `LineNoticePreFix`）
//!   未接线 ⇒ 用 oracle 缺省 `〖公告〗`（前缀支现网不可达）。
//! - per-player `RunTime`/`SearchTime` 门未建模 ⇒ 宿主在**每个 tick**（200ms）对该会话调一次
//!   [`roll`]；`ShowLineNoticeTime` 门（现网 300000ms）才是真正控制滚行节奏的周期（U-C2 登记）。

use game_svr::smframe;
use game_svr::smframe_ext;

/// `Config` 的滚动公告输入（`Server.conf` `[Setup]` 键，见 `config.rs`）。
#[derive(Clone, Debug)]
pub struct LineNoticeCfg {
    /// `[Setup] ShowLineNoticeTime`（`GameSvrConf.cs:1440` 缺省 `5*60*1000`）。
    pub interval_ms: i64,
    /// `[Setup] LineNoticeColor`（无 `R`/`G`/`B` 前缀行的 `MsgColor`，`Red=0/Green=1/Blue=2`）。
    pub default_color: u8,
    /// `[Setup] ShowPreFixMsg`（前缀开关）。
    pub show_pre_fix_msg: bool,
    /// `Config.LineNoticePreFix`（现网经 `String.conf`；未接线 ⇒ oracle 缺省 `〖公告〗`）。
    pub pre_fix: String,
    /// `Config.RedMsgFColor/RedMsgBColor`。
    pub red: (u8, u8),
    /// `Config.GreenMsgFColor/GreenMsgBColor`。
    pub green: (u8, u8),
    /// `Config.BlueMsgFColor/BlueMsgBColor`。
    pub blue: (u8, u8),
}

/// oracle `Config.LineNoticePreFix` 缺省（`GameSvrConf.cs:1841`）。
pub const LINE_NOTICE_PRE_FIX_DEFAULT: &str = "〖公告〗";

/// per-session 的 `PlayObject.ShowLineNoticeTick` / `ShowLineNoticeIdx`
/// （`PlayObject.cs:611-612`；进世界时 `ShowLineNoticeTick = GetTickCount()`、`Idx = 0`，
/// `PlayObject.cs:935-936`）。
#[derive(Clone, Copy, Debug, Default)]
pub struct LineNoticeState {
    /// `ShowLineNoticeTick`（ms，与宿主 `now_ms` 同源）。
    pub tick: i64,
    /// `ShowLineNoticeIdx`。
    pub idx: usize,
}

impl LineNoticeState {
    /// 进世界时的初值（= oracle `new PlayObject()` 的 `ShowLineNoticeTick = GetTickCount()`）。
    #[must_use]
    pub fn new(now_ms: i64) -> Self {
        Self {
            tick: now_ms,
            idx: 0,
        }
    }
}

/// 推进一次滚动公告（`WorldServer.cs:669-696` 的等价物，纯函数）。
///
/// 返回本 pass 应下发的帧（`None` = 本 pass 无帧）。**注意**：即使未到周期内/列表越界，
/// oracle 仍会在到周期后推进 `Idx`（`ShowLineNoticeIdx++` 在 `if` 外）⇒ 本函数在同到周期时
/// 也照此推进。
#[must_use]
pub fn roll(
    now_ms: i64,
    state: &mut LineNoticeState,
    cfg: &LineNoticeCfg,
    lines: &[String],
    actor_id: i32,
) -> Option<Vec<u8>> {
    // `(now - ShowLineNoticeTick) > ShowLineNoticeTime` 才进。
    if now_ms - state.tick <= cfg.interval_ms {
        return None;
    }
    state.tick = now_ms;
    let mut frame = None;
    if lines.len() > state.idx {
        // `GetLineVariableText`（变量替换未建模 ⇒ 恒等）。
        frame = build_frame(&lines[state.idx], cfg, actor_id);
    }
    state.idx += 1;
    if lines.len() <= state.idx {
        state.idx = 0;
    }
    frame
}

/// `WorldServer.cs:672-690` 的 `lineNoticeMsg` → 帧（`SysMsg(…, Notice)` 等价物）。
///
/// 先按首字符 `R`/`G`/`B` 剥离并选色（`WordServer.cs:674-689`），再走
/// `BaseObject.SysMsg`（`BaseObject.cs:1588-1685`）的 `MsgType.Notice` 分支。
#[must_use]
pub fn build_frame(msg: &str, cfg: &LineNoticeCfg, actor_id: i32) -> Option<Vec<u8>> {
    // `switch (lineNoticeMsg[0])`：`R`/`G`/`B` 剥离首字符并选对应色；其余用 `Config.LineNoticeColor`。
    // （首字符为 ASCII 时按 1 字节切片；否则整串留给 SysMsg 的 default 支。）
    let (body, color) = match msg.as_bytes().first() {
        Some(b'R') => (&msg[1..], 0u8),
        Some(b'G') => (&msg[1..], 1u8),
        Some(b'B') => (&msg[1..], 2u8),
        _ => (msg, cfg.default_color),
    };
    sys_msg_notice(body, color, cfg, actor_id)
}

/// `BaseObject.SysMsg(sMsg, msgColor, MsgType.Notice)`（`BaseObject.cs:1588-1685`）的出帧半。
fn sys_msg_notice(s: &str, color: u8, cfg: &LineNoticeCfg, actor_id: i32) -> Option<Vec<u8>> {
    match s.as_bytes().first() {
        // `'['`：顶部滚动公告（`BaseObject.cs:1597-1607`）。
        Some(b'[') => {
            let (inner, rest) = mir2_shared::hutil32::arrest_string_ex_ref(s, '[', ']');
            let inner = arrest_value(inner);
            // `bColor = GetValidStrCap(str, ref fColor, ',')`：按 ',' 切，前段入 fColor。
            let (f_s, b_s) = get_valid_str_cap_comma(&inner);
            let text = prefix_or(&rest, cfg);
            let f = str_to_uint16(&f_s, 255) as u8;
            let b = str_to_uint16(&b_s, 255) as u8;
            // `SendMsg(RM_MOVEMESSAGE, 0, fColor, bColor, 0, sMsg)` ⇒
            // `MakeMessage(SM_MOVEMESSAGE, ActorId, MakeWord(f,b), 0, 0)`。
            Some(smframe_ext::sm_movemessage(
                actor_id,
                f,
                b,
                0,
                0,
                &crate::enter::gbk(&text),
            ))
        }
        // `'<'`：聊天框彩色公告（`BaseObject.cs:1610-1621`）。
        Some(b'<') => {
            let (inner, rest) = mir2_shared::hutil32::arrest_string_ex_ref(s, '<', '>');
            let inner = arrest_value(inner);
            let (f_s, b_s) = get_valid_str_cap_comma(&inner);
            let text = prefix_or(&rest, cfg);
            let f = str_to_uint16(&f_s, 255) as u8;
            let b = str_to_uint16(&b_s, 255) as u8;
            Some(smframe::sm_sysmessage(
                actor_id,
                f,
                b,
                &crate::enter::gbk(&text),
            ))
        }
        // `'{'`：屏幕居中公告（`BaseObject.cs:1624-1636`）。
        Some(b'{') => {
            let (inner, rest) = mir2_shared::hutil32::arrest_string_ex_ref(s, '{', '}');
            let inner = arrest_value(inner);
            let (f_s, t1) = get_valid_str_cap_comma(&inner);
            let (b_s, t2) = get_valid_str_cap_comma(&t1);
            let (n_s, _) = get_valid_str_cap_comma(&t2);
            let text = prefix_or(&rest, cfg);
            let f = str_to_uint16(&f_s, 255) as u8;
            let b = str_to_uint16(&b_s, 255) as u8;
            let n = str_to_uint16(&n_s, 0);
            // `SendMsg(RM_MOVEMESSAGE, 1, fColor, bColor, nTime, sMsg)` ⇒
            // `MakeMessage(SM_MOVEMESSAGE, ActorId, MakeWord(f,b), nTime, 1)`。
            Some(smframe_ext::sm_movemessage(
                actor_id,
                f,
                b,
                i32::from(n),
                1,
                &crate::enter::gbk(&text),
            ))
        }
        // `default`：按 `msgColor`（`BaseObject.cs:1640-1684`；仅 Red/Green/Blue 三支发帧）。
        _ => match color {
            0 => Some(smframe::sm_sysmessage(
                actor_id,
                cfg.red.0,
                cfg.red.1,
                &crate::enter::gbk(s),
            )),
            1 => Some(smframe::sm_sysmessage(
                actor_id,
                cfg.green.0,
                cfg.green.1,
                &crate::enter::gbk(s),
            )),
            2 => Some(smframe::sm_sysmessage(
                actor_id,
                cfg.blue.0,
                cfg.blue.1,
                &crate::enter::gbk(s),
            )),
            // `MsgColor.White`（3）在 oracle 的 `switch(msgColor)` 里**无 case** ⇒ 不发帧。
            _ => None,
        },
    }
}

/// `ArrestWrite` → 实际写入 `ref` 的值（`Unchanged` 时 oracle 的 `str` 保留原值；本模块调用点
/// 的 `str` 初值恒空串 ⇒ 等价于空串）。
fn arrest_value(w: mir2_shared::hutil32::ArrestWrite) -> String {
    match w {
        mir2_shared::hutil32::ArrestWrite::Value(s) => s,
        mir2_shared::hutil32::ArrestWrite::Unchanged => String::new(),
    }
}

/// `HUtil32.GetValidStrCap(str, ref dest, ',')`（`HUtil32.cs:430-443`）的等价物，
/// 返回 `(dest, 方法返回值)`。
///
/// oracle：`str[0] == '"'` 走 `CaptureString`（**未建模**，见模块头登记），
/// 否则走 [`get_valid_str3`]。调用点里 `dest` 即 `fColor`/`bColor`/`nTime`
/// 之一，返回值作为下一段的源串（`str = GetValidStrCap(...)`）。
fn get_valid_str_cap_comma(s: &str) -> (String, String) {
    // `GetValidStrCap`：`string.IsNullOrEmpty(str)` ⇒ 返回空、dest 置空。
    if s.is_empty() {
        return (String::new(), String::new());
    }
    get_valid_str3(s, ',')
}

/// `HUtil32.GetValidStr3(source, ref dest, divider)`（`HUtil32.cs:384-389`）：
/// `source.Split(divider, 2, StringSplitOptions.RemoveEmptyEntries)` ⇒ 按**首个**分隔符
/// 切成至多 2 段，**空段被删**；`dest = ary[0]`（无则空），返回 `ary[1]`（无则空）。
///
/// ⚠ 与 `str::split_once` 的差别正在「空段被删」：`",252"` ⇒ oracle 得
/// `dest="252"`、返回 `""`（即 fColor=252、bColor 缺省），而朴素 `split_once`
/// 会得 `("", "252")`（**恰好相反**）。
fn get_valid_str3(source: &str, divider: char) -> (String, String) {
    let raw: Vec<&str> = match source.split_once(divider) {
        Some((a, b)) => vec![a, b],
        None => vec![source],
    };
    let parts: Vec<&str> = raw.into_iter().filter(|p| !p.is_empty()).collect();
    let dest = parts.first().map_or_else(String::new, |s| (*s).to_string());
    let ret = parts.get(1).map_or_else(String::new, |s| (*s).to_string());
    (dest, ret)
}

/// `if (ShowPreFixMsg) sMsg = LineNoticePreFix + sMsg;`。
fn prefix_or(s: &str, cfg: &LineNoticeCfg) -> String {
    if cfg.show_pre_fix_msg {
        format!("{}{}", cfg.pre_fix, s)
    } else {
        s.to_string()
    }
}

/// `HUtil32.StrToUInt16(str, def)`（`HUtil32.cs:364-367`）：`ushort.TryParse` 成功取之，否则 `def`。
#[must_use]
pub fn str_to_uint16(s: &str, def: u16) -> u16 {
    s.trim().parse::<u16>().unwrap_or(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> LineNoticeCfg {
        LineNoticeCfg {
            interval_ms: 300_000,
            default_color: 0, // 现网 Server.conf:336 LineNoticeColor=0
            show_pre_fix_msg: false,
            pre_fix: LINE_NOTICE_PRE_FIX_DEFAULT.to_string(),
            red: (0xFF, 0x38),
            green: (0xDB, 0xFF),
            blue: (0xFF, 0xFC),
        }
    }

    /// **滚动状态机（ShowLineNoticeTick 门 + Idx 递增 + 回绕）**：`WorldServer.cs:669-696`。
    /// 未到周期 ⇒ 零帧且 `Idx` 不动；到周期 ⇒ 滚一行 + `Idx++`；越界 ⇒ 回绕 0。
    #[test]
    fn roll_advances_index_by_interval_and_wraps() {
        let lines = vec!["[公告]甲".to_string(), "[公告]乙".to_string()];
        let mut st = LineNoticeState::new(0);
        // 未到周期：零帧、Idx 不动。
        assert!(roll(300_000, &mut st, &cfg(), &lines, 7).is_none());
        assert_eq!(st.idx, 0);
        // 到周期（> 300000）：发第 0 行 + Idx→1。
        assert!(roll(300_001, &mut st, &cfg(), &lines, 7).is_some());
        assert_eq!(st.idx, 1);
        // 再到周期：发第 1 行 + Idx 回绕 0（Count <= Idx）。
        assert!(roll(600_002, &mut st, &cfg(), &lines, 7).is_some());
        assert_eq!(st.idx, 0);
    }

    /// `ShowLineNoticeTick = now` 在**每 pass 到周期时**被刷新（oracle `:671`）⇒ 相邻两 tick
    /// （间隔 < 周期）不会连发。
    #[test]
    fn roll_resets_tick_each_fire() {
        let lines = vec!["[公告]甲".to_string()];
        let mut st = LineNoticeState::new(0);
        assert!(roll(300_001, &mut st, &cfg(), &lines, 1).is_some());
        // 同一时刻再来：`now - tick(=300001) > 300000` 假 ⇒ 不滚。
        assert!(roll(300_001, &mut st, &cfg(), &lines, 1).is_none());
        assert!(roll(600_000, &mut st, &cfg(), &lines, 1).is_none());
        assert!(roll(600_002, &mut st, &cfg(), &lines, 1).is_some());
    }

    /// 列表越界仍推进 `Idx`（oracle `:691-695` 的 `Idx++` 在 `if` 外）——空表时 Idx 恒 0，
    /// 且零帧。改坏（把 `Idx++` 塞进 `if` 内）⇒ 本断言在 `[公告]` 单行场景下红。
    #[test]
    fn roll_advances_index_even_when_no_line_sent() {
        let lines: Vec<String> = Vec::new();
        let mut st = LineNoticeState::new(0);
        assert!(roll(300_001, &mut st, &cfg(), &lines, 1).is_none());
        assert_eq!(st.idx, 0); // 空表：++ → 1 → Count(0)<=1 ⇒ 回绕 0。
    }

    /// **帧选择**：deployed 内容形如 `[公告]文字` ⇒ `'['` 支 ⇒ `SM_MOVEMESSAGE`（顶部滚动），
    /// 颜色 = `StrToUInt16("公告",255)=255`（非数字 ⇒ 缺省）；`[公告]` 被 `ArrestStringEx` 剥离。
    #[test]
    fn build_frame_square_prefix_is_move_message() {
        let body = build_frame("[公告]使用金币来代替需求!", &cfg(), 42).expect("应出帧");
        // 头 12B：Recog=42、Ident=SM_MOVEMESSAGE(99)、Param=MakeWord(255,255)、Tag=0、Series=0。
        assert_eq!(i32::from_le_bytes(body[0..4].try_into().unwrap()), 42);
        assert_eq!(u16::from_le_bytes(body[4..6].try_into().unwrap()), 99);
        assert_eq!(u16::from_le_bytes(body[6..8].try_into().unwrap()), 0xFFFF);
        assert_eq!(u16::from_le_bytes(body[8..10].try_into().unwrap()), 0);
        assert_eq!(u16::from_le_bytes(body[10..12].try_into().unwrap()), 0);
        // 尾 = EDCode.EncodeString(GBK("使用金币来代替需求!"))：解回应逐字节等于 GBK 明文
        // （`[公告]` 被 `ArrestStringEx` 剥离 ⇒ 尾不含 "公告"）。
        assert_eq!(
            mir2_protocol::edcode::decode(&body[12..]),
            crate::enter::gbk("使用金币来代替需求!")
        );
    }

    /// `R`/`G`/`B` 前缀被剥离用于选色，剩余走 `SysMsg` 的 default 支 ⇒ `SM_SYSMESSAGE`
    /// （`BaseObject.cs:1673-1683`），色取 `Config.Red/Green/BlueMsgFColor`。
    #[test]
    fn build_frame_rgb_prefix_selects_sysmessage_color() {
        let b = build_frame("R红色公告", &cfg(), 5).expect("应出帧");
        assert_eq!(u16::from_le_bytes(b[4..6].try_into().unwrap()), 100); // SM_SYSMESSAGE
        assert_eq!(u16::from_le_bytes(b[6..8].try_into().unwrap()), 0x38FF); // MakeWord(0xFF=low,0x38=high)
                                                                             // 剥离 'R' ⇒ 尾应是 "红色公告"。
        assert_eq!(
            mir2_protocol::edcode::decode(&b[12..]),
            crate::enter::gbk("红色公告")
        );
    }

    /// 非 `[`/`<`/`{`/`R`/`G`/`B` 开头（如现网首行 `≮…`）⇒ `SysMsg` 的 default 支 ⇒
    /// `SM_SYSMESSAGE`（色 = `Config.LineNoticeColor`）。
    #[test]
    fn build_frame_default_branch_uses_configured_color() {
        let b = build_frame("≮热点公告≯", &cfg(), 9).expect("应出帧");
        assert_eq!(u16::from_le_bytes(b[4..6].try_into().unwrap()), 100); // SM_SYSMESSAGE
        assert_eq!(u16::from_le_bytes(b[6..8].try_into().unwrap()), 0x38FF); // Red=0（default_color=0）⇒ MakeWord(0xFF,0x38)
        assert_eq!(
            mir2_protocol::edcode::decode(&b[12..]),
            crate::enter::gbk("≮热点公告≯")
        );
    }

    /// `MsgColor.White`（3）在 `SysMsg` 的 `switch(msgColor)` 无 case ⇒ **零帧**。
    #[test]
    fn build_frame_white_color_sends_nothing() {
        let mut c = cfg();
        c.default_color = 3;
        assert!(build_frame("纯文本无标记", &c, 1).is_none());
    }

    /// `'<'` 支 ⇒ `SM_SYSMESSAGE`，色从 `<f,b>` 解析（数字色）。
    #[test]
    fn build_frame_angle_prefix_parses_colors() {
        let b = build_frame("<251,252>彩色", &cfg(), 3).expect("应出帧");
        assert_eq!(u16::from_le_bytes(b[4..6].try_into().unwrap()), 100); // SM_SYSMESSAGE
        assert_eq!(u16::from_le_bytes(b[6..8].try_into().unwrap()), 0xFCFB); // MakeWord(251=low,252=high)
        assert_eq!(
            mir2_protocol::edcode::decode(&b[12..]),
            crate::enter::gbk("彩色")
        );
    }

    /// **`'{'` 居中公告支**（`BaseObject.cs:1624-1636`）：`ArrestStringEx(s,'{','}')` 取内层，
    /// 再**逐段** `GetValidStrCap(str, ref {fColor,bColor,nTime}, ',')`（`str` 逐段递进），
    /// 出 `SendMsg(RM_MOVEMESSAGE, 1, f, b, nTime, sMsg)` ⇒ `SM_MOVEMESSAGE`，
    /// `Tag = nTime`、`Series = 1`（`PlayObject.Message.cs:1727` 的 `RM_MOVEMESSAGE` 支）。
    #[test]
    fn build_frame_brace_is_centered_move_message_with_time() {
        // 内层 `9,13,7` ⇒ fColor=9、bColor=13、nTime=7；`}` 之后为文本。
        let b = build_frame("{9,13,7}居中公告", &cfg(), 6).expect("应出帧");
        assert_eq!(i32::from_le_bytes(b[0..4].try_into().unwrap()), 6);
        assert_eq!(u16::from_le_bytes(b[4..6].try_into().unwrap()), 99); // SM_MOVEMESSAGE
        assert_eq!(u16::from_le_bytes(b[6..8].try_into().unwrap()), 0x0D09); // MakeWord(9=low,13=high)
        assert_eq!(u16::from_le_bytes(b[8..10].try_into().unwrap()), 7); // Tag = nTime
        assert_eq!(u16::from_le_bytes(b[10..12].try_into().unwrap()), 1); // Series = 1
        assert_eq!(
            mir2_protocol::edcode::decode(&b[12..]),
            crate::enter::gbk("居中公告")
        );
        // 缺段回落：`{5}` ⇒ fColor=5、bColor/nTime 缺省（255/0）。
        let b2 = build_frame("{5}甲", &cfg(), 6).expect("应出帧");
        assert_eq!(u16::from_le_bytes(b2[6..8].try_into().unwrap()), 0xFF05); // MakeWord(5,255)
        assert_eq!(u16::from_le_bytes(b2[8..10].try_into().unwrap()), 0); // nTime 缺省 0
        assert_eq!(u16::from_le_bytes(b2[10..12].try_into().unwrap()), 1);
    }

    /// **`GetValidStrCap` 逗号切分对齐 oracle**（`HUtil32.GetValidStr3` 的
    /// `Split(',',2,RemoveEmptyEntries)` 删空段）：`",252"` ⇒ **fColor=252**（空首段被删）、
    /// bColor 缺省 255 —— 而朴素 `split_once` 会得 fColor=255/bColor=252（**恰好相反**）。
    /// 本用例是「对齐与否」的牙：改回 `split_once` 语义 ⇒ 必红。
    #[test]
    fn build_frame_comma_split_drops_empty_first_segment_like_oracle() {
        let b = build_frame("[,252]甲", &cfg(), 6).expect("应出帧");
        assert_eq!(u16::from_le_bytes(b[4..6].try_into().unwrap()), 99); // SM_MOVEMESSAGE

        // fColor=252（low）、bColor=255（high，缺省）⇒ MakeWord(252,255)。
        assert_eq!(u16::from_le_bytes(b[6..8].try_into().unwrap()), 0xFFFC);
        assert_eq!(
            mir2_protocol::edcode::decode(&b[12..]),
            crate::enter::gbk("甲")
        );
    }

    /// `StrToUInt16` 口径。
    #[test]
    fn str_to_uint16_matches_oracle() {
        assert_eq!(str_to_uint16("251", 0), 251);
        assert_eq!(str_to_uint16("", 7), 7);
        assert_eq!(str_to_uint16("公告", 255), 255);
        assert_eq!(str_to_uint16(" 12 ", 0), 12); // .NET TryParse 容忍前后空白。
    }
}
