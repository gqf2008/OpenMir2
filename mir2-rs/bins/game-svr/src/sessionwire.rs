//! **接线批 A：会话/切图/装备光的出帧装配**（`crates/world` 的世界事件/session 状态 →
//! `SM_*` body 与路由类别）。与 `downlink`（战斗/AOI）、`movewire`（走位）、`pickwire`（拾取）同层：
//! 只做**纯装配**（无 I/O、无全局状态），供 `main.rs` 的 drain 段与进程级集成测试共用。
//!
//! 本模块覆盖批 A 里**有真实触发链**的帧；逐帧的 oracle 发点与接线状态见 rationale：
//!
//! | 帧 | oracle 发点 | 本模块 |
//! |---|---|---|
//! | `SM_CLEAROBJECTS(633)` | `BaseObject.cs:1133` / `PlayObject.cs:3772` | [`map_change_frames`]（`SendMsg` ⇒ [`Route::SelfOnly`]） |
//! | `SM_CHANGEMAP(634)` | `BaseObject.cs:1134` / `PlayObject.cs:3778` | [`map_change_frames`]（`SendMsg` ⇒ [`Route::SelfOnly`]） |
//! | `SM_SPACEMOVE_SHOW(801)`/`SHOW2(807)` | `BaseObject.cs:1137/1141` | [`map_change_frames`]（`SendRefMsg` ⇒ [`Route::Ref`]） |
//! | `SM_CHANGELIGHT(654)` | `PlayObject.cs:2884` | [`changelight_frame`]（`SendRefMsg` ⇒ [`Route::Ref`]） |
//! | `SM_PASSWORD(5009)` | `PlayObject.Base.cs:3207/3304` | [`password_frame`]（`SendMsg` ⇒ [`Route::SelfOnly`]） |

use crate::smframe;
use crate::smframe_ext;
use mir2_protocol::messages;
use mir2_world::session::MapChange;

/// `SystemShare.Config.nClientKey`（`GameSvrConf.cs:1886-1887`：先 `6534` 再覆盖成 `500`）。
///
/// **登记**：本仓 `config.rs` 未读该键（oracle 侧也无 `Server.conf` 读取点；纯代码常量）⇒
/// 按最终默认 `500` 钉死，防止将来改常量时静默漂移。
pub const N_CLIENT_KEY_DEFAULT: i32 = 500;

/// 帧的收件人集合分类（`BaseObject.Message.cs:383` 的 `SendRefMsg` vs `SendMsg`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// `SendMsg(...)`：只发自己（`SM_CLEAROBJECTS`/`SM_CHANGEMAP`/`SM_PASSWORD`）。
    SelfOnly,
    /// `SendRefMsg(...)`：以发帧者为中心 ±`SendRefMsgRange`(12) 格、`Race==Play && !Ghost` 的 actor
    /// （**含自己**）各一发（`SM_SPACEMOVE_SHOW`/`SM_CHANGELIGHT`）。
    ///
    /// 注：`SendRefMsg` 的观察者集合已在 `chat::hear_targets` 建模（`BaseObject.Message.cs:395-440`）。
    Ref,
}

/// 一条待下行帧：`GM_DATA` body（12B 头 + 尾）+ 路由类别。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emitted {
    pub body: Vec<u8>,
    pub route: Route,
}

/// `World::switch_map`/`switch_map_space` 的一次落地 → `SM_CLEAROBJECTS(633)` →
/// `SM_CHANGEMAP(634)` [+ `SM_SPACEMOVE_SHOW(801)`/`SHOW2(807)`]（oracle 序）。
///
/// oracle 链：`BaseObject.SpaceMove`（`BaseObject.cs:1130-1144`）落地后
/// `SendMsg(RM_CLEAROBJECTS)` → `SendMsg(RM_CHANGEMAP, …, MapFileName)` →
/// （若 `nInt` 分叉）`SendRefMsg(RM_SPACEMOVE_SHOW(|2), Dir, CurrX, CurrY, 0, "")`；
/// `PlayObject.EnterAnotherMap`（`PlayObject.cs:3772-3778`）只发前两条（`space_move_show == None`）。
///
/// 参数：`map_file_name` = 新图逻辑名（`Envir.MapFileName` 的 GBK 字节，本层不转 UTF-8）；
/// `day_bright` = `DayBright()`（新图）；`dir`/`light` = `SM_SPACEMOVE_SHOW` 的 `Series` 两个半字节；
/// `char_desc` = `Encode(CharDesc{Feature,Status})` 的**未编码** 8B（`sm_spacemove_show` 内再 Encode）。
#[must_use]
pub fn map_change_frames(
    ev: &MapChange,
    map_file_name: &[u8],
    day_bright: u16,
    dir: u8,
    light: u8,
    char_desc: &[u8],
) -> Vec<Emitted> {
    let mut out = Vec::with_capacity(3);
    out.push(Emitted {
        body: smframe::sm_clearobjects(),
        route: Route::SelfOnly,
    });
    out.push(Emitted {
        body: smframe_ext::sm_changemap(
            ev.actor_id,
            ev.x as u16,
            ev.y as u16,
            day_bright,
            map_file_name,
        ),
        route: Route::SelfOnly,
    });
    if let Some(show2) = ev.space_move_show {
        let ident = if show2 {
            messages::SM_SPACEMOVE_SHOW2
        } else {
            messages::SM_SPACEMOVE_SHOW
        };
        out.push(Emitted {
            body: smframe::sm_spacemove_show(
                ident,
                ev.actor_id,
                ev.x as u16,
                ev.y as u16,
                dir,
                light,
                char_desc,
                &[],
            ),
            route: Route::Ref,
        });
    }
    out
}

/// `SM_CHANGELIGHT(654)`（`PlayObject.Message.cs:1840` 的 `RM_CHANGELIGHT` 支）。
///
/// oracle 发点：`PlayObject.RecalcAbilitys`（`PlayObject.cs:2882-2885`）在 `oldlight != Light` 时
/// `SendRefMsg(RM_CHANGELIGHT, 0, 0, 0, 0, "")`（`wParam`/`nParam*` 全 0，`Msg` 空）⇒ 观察者侧
/// `SendDefMessage(SM_CHANGELIGHT, ActorId, baseObject.Light, (short)Config.nClientKey, 0)`。
#[must_use]
pub fn changelight_frame(actor_id: i32, light: u8, client_key: i32) -> Emitted {
    Emitted {
        body: smframe_ext::sm_changelight(actor_id, light, client_key),
        route: Route::Ref,
    }
}

/// `SM_PASSWORD(5009)`（`PlayObject.Message.cs:2032` 的 `RM_PASSWORD` 支）：
/// `SendDefMessage(SM_PASSWORD, 0, 0, 0, 0)` ⇒ header-only，只发自己。
#[must_use]
pub fn password_frame() -> Emitted {
    Emitted {
        body: smframe_ext::sm_password(),
        route: Route::SelfOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }

    /// 逐字节：`map_change_frames`（`EnterAnotherMap` 支，无 show）出 2 帧、序为
    /// CLEAROBJECTS → CHANGEMAP，头字段与 oracle 观测点一致。
    #[test]
    fn map_change_enter_another_map_two_frames_order_and_bytes() {
        let ev = MapChange {
            actor_id: 0x7c30_3998,
            map_id: 1,
            x: 287,
            y: 618,
            space_move_show: None,
        };
        let frames = map_change_frames(&ev, b"0", 12, 3, 7, &[0u8; 8]);
        assert_eq!(frames.len(), 2, "EnterAnotherMap：CLEAROBJECTS + CHANGEMAP");
        assert_eq!(frames[0].route, Route::SelfOnly);
        assert_eq!(frames[1].route, Route::SelfOnly);

        // CLEAROBJECTS：Recog=0/Ident=633(0x0279)/Param=0/Tag=0/Series=0，无尾。
        let c = &frames[0].body;
        assert_eq!(i32::from_le_bytes(c[0..4].try_into().unwrap()), 0);
        assert_eq!(u16_at(c, 4), messages::SM_CLEAROBJECTS);
        assert_eq!(u16_at(c, 6), 0);
        assert_eq!(u16_at(c, 8), 0);
        assert_eq!(u16_at(c, 10), 0);
        assert_eq!(c.len(), 12, "CLEAROBJECTS 无尾");

        // CHANGEMAP：Recog=ActorId / Ident=634 / Param=X / Tag=Y / Series=DayBright，尾=EncodeString("0")。
        let m = &frames[1].body;
        assert_eq!(i32::from_le_bytes(m[0..4].try_into().unwrap()), 0x7c30_3998);
        assert_eq!(u16_at(m, 4), messages::SM_CHANGEMAP);
        assert_eq!(u16_at(m, 6), 287);
        assert_eq!(u16_at(m, 8), 618);
        assert_eq!(u16_at(m, 10), 12);
        assert_eq!(&m[12..], mir2_protocol::edcode::encode(b"0").as_slice());
    }

    /// 逐字节：`SpaceMove` 支（`nInt != 1`）追加 `SM_SPACEMOVE_SHOW(801)`，路由 = `Ref`；
    /// `nInt == 1` ⇒ `SM_SPACEMOVE_SHOW2(807)`。
    #[test]
    fn map_change_space_move_appends_show_with_ref_route() {
        let base = MapChange {
            actor_id: 0x0000_0102,
            map_id: 3,
            x: 6,
            y: 9,
            space_move_show: Some(false),
        };
        let f = map_change_frames(&base, b"0101", 0, 2, 5, &[0u8; 8]);
        assert_eq!(f.len(), 3);
        assert_eq!(f[2].route, Route::Ref, "SHOW 是 SendRefMsg 广播");
        assert_eq!(u16_at(&f[2].body, 4), messages::SM_SPACEMOVE_SHOW);
        assert_eq!(u16_at(&f[2].body, 6), 6, "Param = X");
        assert_eq!(u16_at(&f[2].body, 8), 9, "Tag = Y");
        assert_eq!(f[2].body[10], 2, "Series 低字节 = Dir");
        assert_eq!(f[2].body[11], 5, "Series 高字节 = Light");

        let f2 = map_change_frames(
            &MapChange {
                space_move_show: Some(true),
                ..base
            },
            b"0101",
            0,
            2,
            5,
            &[0u8; 8],
        );
        assert_eq!(u16_at(&f2[2].body, 4), messages::SM_SPACEMOVE_SHOW2);
        assert_eq!(f2[2].route, Route::Ref);
    }

    /// `changelight_frame`：`SendDefMessage(SM_CHANGELIGHT, ActorId, Light, (u16)nClientKey, 0)`，
    /// 路由 = `Ref`。
    #[test]
    fn changelight_frame_bytes_and_ref_route() {
        let e = changelight_frame(0x0000_00aa, 6, N_CLIENT_KEY_DEFAULT);
        assert_eq!(e.route, Route::Ref);
        let b = &e.body;
        assert_eq!(i32::from_le_bytes(b[0..4].try_into().unwrap()), 0xaa);
        assert_eq!(u16_at(b, 4), messages::SM_CHANGELIGHT);
        assert_eq!(u16_at(b, 6), 6, "Param = Light");
        assert_eq!(u16_at(b, 8), 500, "Tag = nClientKey(500)");
        assert_eq!(u16_at(b, 10), 0);
        assert_eq!(b.len(), 12);
    }

    /// `password_frame`：全 0 头、`SelfOnly`。
    #[test]
    fn password_frame_all_zero_self_only() {
        let e = password_frame();
        assert_eq!(e.route, Route::SelfOnly);
        let b = &e.body;
        assert_eq!(b.len(), 12);
        assert_eq!(u16_at(b, 4), messages::SM_PASSWORD);
        assert!(
            b[0..4].iter().all(|&x| x == 0) && b[6..12].iter().all(|&x| x == 0),
            "SM_PASSWORD 除 ident 外全 0"
        );
    }
}
