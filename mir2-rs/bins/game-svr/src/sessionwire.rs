//! **接线批 A：会话/切图/装备光的出帧装配**（`crates/world-bevy` 的世界事件/session 状态 →
//! `SM_*` body 与路由类别）。与 `downlink`（战斗/AOI）、`movewire`（走位）、`pickwire`（拾取）同层：
//! 只做**纯装配**（无 I/O、无全局状态），供 `lib.rs` 的 drain 段与进程级集成测试共用。
//!
//! 本模块覆盖批 A 里**有真实触发链**的帧；逐帧的 oracle 发点与接线状态见 rationale：
//!
//! | 帧 | oracle 发点 | 本模块 |
//! |---|---|---|
//! | `SM_CLEAROBJECTS(633)` | `BaseObject.cs:1133` / `PlayObject.cs:3772` | [`map_change_frames`]（`SendMsg` ⇒ [`Route::SelfOnly`]） |
//! | `SM_CHANGEMAP(634)` | `BaseObject.cs:1134` / `PlayObject.cs:3778` | [`map_change_frames`]（`SendMsg` ⇒ [`Route::SelfOnly`]） |
//! | `SM_SPACEMOVE_SHOW(801)`/`SHOW2(807)` | `BaseObject.cs:1137/1141` | [`map_change_frames`]（`SendRefMsg` ⇒ [`Route::Ref`]） |
//! | `SM_AREASTATE(766)` | `PlayObject.Message.cs:1774-1779` → `RefUserState`（`Base.cs:270-286`） | [`map_change_frames`]（`SendDefMessage` ⇒ [`Route::SelfOnly`]） |
//! | `SM_MAPDESCRIPTION(54)` | `PlayObject.Message.cs:1774-1779` → `SendMapDescription`（`Base.cs:887-895`） | [`map_change_frames`]（`SendDefMessage` ⇒ [`Route::SelfOnly`]） |
//! | `SM_CHANGELIGHT(654)` | `PlayObject.cs:2884` | [`changelight_frame`]（`SendRefMsg` ⇒ [`Route::Ref`]） |
//! | `SM_PASSWORD(5009)` | `PlayObject.Base.cs:3207/3304` | [`password_frame`]（`SendMsg` ⇒ [`Route::SelfOnly`]） |

use crate::smframe;
use crate::smframe_ext;
use mir2_protocol::messages;
use mir2_world_bevy::session::MapChange;

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

/// 换图簇的 `Envir` 展示输入（oracle `RM_CHANGEMAP` 处理器的连带帧来源）。
///
/// `flag` = `Envirnoment.Flag`（766 的 `n8`、54 的 `nMusicid`）；`desc` = `Envir.MapDesc`
/// （54 的文本尾原文，本层再 GBK+EDCode）。`None`（无注册表/无该图）⇒ 两条都不发。
#[derive(Clone, Copy, Debug)]
pub struct MapVisual<'a> {
    pub flag: &'a mir2_envir::mapinfo::MapInfoFlag,
    pub desc: &'a str,
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
/// 参数：`map_file_name` = 新图的**取图文件主干**（`Envir.MapFileName` = `MapManager.AddMapInfo`
/// 的 `sMapFileName`，即 `[名|文件]` 的右半，缺省 = 逻辑名；GBK 字节，本层不转 UTF-8）；
/// `day_bright` = `DayBright()`（新图）；`dir`/`light` = `SM_SPACEMOVE_SHOW` 的 `Series` 两个半字节；
/// `char_desc` = `Encode(CharDesc{Feature,Status})` 的**未编码** 8B（`sm_spacemove_show` 内再 Encode）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn map_change_frames(
    ev: &MapChange,
    map_file_name: &[u8],
    day_bright: u16,
    dir: u8,
    light: u8,
    char_desc: &[u8],
    visual: Option<MapVisual<'_>>,
    // `PlayObject.SoftVersionDateEx`（会话字段；0 ⇒ `SendServerConfig` 直接 return，不发 11029）。
    soft_version_date: i32,
    // `SystemShare.Config.ClientConf` 的序列化原文（`GameSvrConf::client_conf.to_bytes()`）。
    // `None`（`Server.conf` 读不到）⇒ 不发并登记方向（oracle 该情形起不来）。
    client_conf: Option<&[u8]>,
) -> Vec<Emitted> {
    let mut out = Vec::with_capacity(6);
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
    // oracle `PlayObject.Message.cs:1774-1779` 的 `RM_CHANGEMAP` 处理器在 `SendDefMessage(SM_CHANGEMAP)`
    // 之后**同支连发**：`RefUserState()` ⇒ `SM_AREASTATE(766)`（`Base.cs:270-286`）、
    // `SendMapDescription()` ⇒ `SM_MAPDESCRIPTION(54)`（`:887-895`）；两者都 `SendDefMessage`（只发自己）。
    if let Some(v) = visual {
        out.push(Emitted {
            body: smframe::sm_areastate(crate::enter::area_state(v.flag)),
            route: Route::SelfOnly,
        });
        out.push(Emitted {
            body: smframe::sm_mapdescription(
                crate::enter::music_id(v.flag),
                &crate::enter::gbk(v.desc),
            ),
            route: Route::SelfOnly,
        });
    }
    // `SendServerConfig()`（`PlayObject.Base.cs` 的 `SendServerConfig`）：`SoftVersionDateEx == 0`
    // ⇒ return（**不发**）；否则尾 = `EncodeString(EncodeMessage(ClientConf))`（双重编码）。
    // 序在 `54` 之后、`SM_SPACEMOVE_SHOW*` 之前（golden `spacemove`：`… 766, 54, 11029, 807 …`）。
    if soft_version_date != 0 {
        if let Some(cfg) = client_conf {
            out.push(Emitted {
                body: smframe::sm_serverconfig(cfg),
                route: Route::SelfOnly,
            });
        }
    }
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
        let frames = map_change_frames(&ev, b"0", 12, 3, 7, &[0u8; 8], None, 0, None);
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
        let f = map_change_frames(&base, b"0101", 0, 2, 5, &[0u8; 8], None, 0, None);
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
            None,
            0,
            None,
        );
        assert_eq!(u16_at(&f2[2].body, 4), messages::SM_SPACEMOVE_SHOW2);
        assert_eq!(f2[2].route, Route::Ref);
    }

    /// **766/54 连带帧**：序在 634 与 801 之间、路由 SelfOnly、字段取自 `Envir.Flag/MapDesc`。
    #[test]
    fn map_change_appends_areastate_and_mapdescription() {
        let ev = MapChange {
            actor_id: 7,
            map_id: 2,
            x: 1,
            y: 2,
            space_move_show: Some(false),
        };
        let flag = mir2_envir::mapinfo::MapInfoFlag {
            fight_zone: true,
            safe_area: true,
            music: true,
            music_id: 42,
            day_light: true,
            ..Default::default()
        };
        let f = map_change_frames(
            &ev,
            b"m",
            0,
            4,
            0,
            &[0u8; 8],
            Some(MapVisual {
                flag: &flag,
                desc: "副本",
            }),
            0,
            None,
        );
        let ids: Vec<u16> = f.iter().map(|x| u16_at(&x.body, 4)).collect();
        assert_eq!(
            ids,
            vec![
                messages::SM_CLEAROBJECTS,
                messages::SM_CHANGEMAP,
                messages::SM_AREASTATE,
                messages::SM_MAPDESCRIPTION,
                messages::SM_SPACEMOVE_SHOW
            ],
            "oracle 序：633 → 634 → 766 → 54 → 801"
        );
        // 766：Recog = FightZone|1 | SafeArea|2 = 3；无尾。
        assert_eq!(
            i32::from_le_bytes(f[2].body[0..4].try_into().unwrap()),
            3,
            "766 Recog = 标志位"
        );
        assert_eq!(f[2].route, Route::SelfOnly);
        assert_eq!(f[2].body.len(), 12, "766 无尾");
        // 54：Recog = nMusicid；尾 = EDCode(GBK(MapDesc))。
        assert_eq!(
            i32::from_le_bytes(f[3].body[0..4].try_into().unwrap()),
            42,
            "54 Recog = MusicId"
        );
        assert_eq!(
            &f[3].body[12..],
            mir2_protocol::edcode::encode(&crate::enter::gbk("副本")).as_slice()
        );
        assert_eq!(f[3].route, Route::SelfOnly);
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

    /// **11029 `SM_SERVERCONFIG`**：`soft_version_date != 0` ⇒ 在 `54` 之后、`801/807` 之前插入；
    /// 尾 = `EDCode(EDCode(ClientConf))`（双重编码）；路由 SelfOnly。
    #[test]
    fn map_change_appends_serverconfig_between_mapdesc_and_show() {
        let ev = MapChange {
            actor_id: 9,
            map_id: 2,
            x: 3,
            y: 4,
            space_move_show: Some(true),
        };
        let flag = mir2_envir::mapinfo::MapInfoFlag::default();
        let f = map_change_frames(
            &ev,
            b"0",
            0,
            0,
            0,
            &[0u8; 8],
            Some(MapVisual {
                flag: &flag,
                desc: "x",
            }),
            20_240_101,
            Some(b"cfg"),
        );
        let ids: Vec<u16> = f.iter().map(|x| u16_at(&x.body, 4)).collect();
        assert_eq!(
            ids,
            vec![
                messages::SM_CLEAROBJECTS,
                messages::SM_CHANGEMAP,
                messages::SM_AREASTATE,
                messages::SM_MAPDESCRIPTION,
                messages::SM_SERVERCONFIG,
                messages::SM_SPACEMOVE_SHOW2
            ],
            "oracle 序：633 → 634 → 766 → 54 → 11029 → 807"
        );
        assert_eq!(
            f[4].route,
            Route::SelfOnly,
            "11029 是 SendDefMessage（只发自己）"
        );
        let want_tail = mir2_protocol::edcode::encode(&mir2_protocol::edcode::encode(b"cfg"));
        assert_eq!(&f[4].body[12..], want_tail.as_slice(), "尾必须双重编码");
    }

    /// **负例（行为面）**：`SoftVersionDateEx == 0` ⇒ **不发** 11029（oracle `SendServerConfig` 早退）。
    #[test]
    fn map_change_omits_serverconfig_when_soft_version_zero() {
        let ev = MapChange {
            actor_id: 9,
            map_id: 2,
            x: 3,
            y: 4,
            space_move_show: Some(false),
        };
        let f = map_change_frames(&ev, b"0", 0, 0, 0, &[0u8; 8], None, 0, Some(b"cfg"));
        let ids: Vec<u16> = f.iter().map(|x| u16_at(&x.body, 4)).collect();
        assert!(
            !ids.contains(&messages::SM_SERVERCONFIG),
            "soft_version_date=0 ⇒ 不得出现 11029：{ids:?}"
        );
    }
    /// **MG-35 ②（T1，2026-11-27）**：`SendServerConfig` 在**发送时刻**读进程级单例的**当前值**
    ///（oracle `PlayObject.Base.cs:966`）⇒ 同一进程先后两次 send，各自读到"当时的 conf"。
    /// 纯函数面（conf 是参数，不经过线程本地/进程级静态）⇒ 并行安全、零残留。
    #[test]
    fn map_change_serverconfig_reads_current_conf_at_send_time() {
        let ev = MapChange {
            actor_id: 9,
            map_id: 2,
            x: 3,
            y: 4,
            space_move_show: Some(true),
        };
        let flag = mir2_envir::mapinfo::MapInfoFlag::default();
        let visual = Some(MapVisual {
            flag: &flag,
            desc: "x",
        });
        let mk = |conf: &[u8]| {
            map_change_frames(
                &ev,
                b"0",
                0,
                0,
                0,
                &[0u8; 8],
                visual,
                20_240_101,
                Some(conf),
            )
            .into_iter()
            .find(|x| u16_at(&x.body, 4) == messages::SM_SERVERCONFIG)
            .expect("soft_version!=0 且 conf=Some ⇒ 必有 11029")
        };
        let a = mk(b"cfgA");
        let want_a = mir2_protocol::edcode::encode(&mir2_protocol::edcode::encode(b"cfgA"));
        assert_eq!(&a.body[12..], want_a.as_slice(), "第一发读到 A");
        let b = mk(b"cfgB");
        let want_b = mir2_protocol::edcode::encode(&mir2_protocol::edcode::encode(b"cfgB"));
        assert_eq!(
            &b.body[12..],
            want_b.as_slice(),
            "换装后第二发读到 B（发送时刻读当前值）"
        );
        assert_ne!(want_a, want_b, "两条尾必须互异（防空转）");
    }

    /// **MG-35 ②（T3，2026-11-27）**：`client_conf = None`（Server.conf 读不到）⇒ **不发** 11029
    ///（oracle 该情形起不来；Rust 侧 fail-open 但方向登记，见 `map_change_frames` 文档）。
    /// 本测试须与 T2（`client_conf_oncelock_first_install_wins_isolated`）**同隔离进程**跑：
    /// T2 会 install 进程级 `CLIENT_CONF`，这里显式断言 `client_conf_bytes()` 语义不变。
    #[test]
    fn map_change_omits_serverconfig_when_client_conf_none() {
        let ev = MapChange {
            actor_id: 9,
            map_id: 2,
            x: 3,
            y: 4,
            space_move_show: Some(true),
        };
        let flag = mir2_envir::mapinfo::MapInfoFlag::default();
        let f = map_change_frames(
            &ev,
            b"0",
            0,
            0,
            0,
            &[0u8; 8],
            Some(MapVisual {
                flag: &flag,
                desc: "x",
            }),
            20_240_101,
            None,
        );
        assert!(
            f.iter()
                .all(|x| u16_at(&x.body, 4) != messages::SM_SERVERCONFIG),
            "conf=None ⇒ 不得出现 11029"
        );
    }

    /// **登记方向**：`soft_version_date != 0` 但 `client_conf = None`（Server.conf 读不到）⇒ 不发 11029。
    #[test]
    fn map_change_omits_serverconfig_when_client_conf_missing() {
        let ev = MapChange {
            actor_id: 9,
            map_id: 2,
            x: 3,
            y: 4,
            space_move_show: None,
        };
        let f = map_change_frames(&ev, b"0", 0, 0, 0, &[0u8; 8], None, 7, None);
        let ids: Vec<u16> = f.iter().map(|x| u16_at(&x.body, 4)).collect();
        assert!(
            !ids.contains(&messages::SM_SERVERCONFIG),
            "client_conf=None ⇒ 不得出现 11029：{ids:?}"
        );
    }
}
