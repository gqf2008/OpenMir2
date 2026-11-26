//! 卡 `openmir2-warrskill-entry` 要求 4 + 卡 `openmir2-mg52-56-explicit-cases`：`ClientSpellXY` 的
//! **7 个显式 `case` 臂**（12/25/26/27/34/38/43；56 因 oracle `MagicArr[56]` 越界崩溃不入对拍家族，
//! 见卡 `openmir2-magic-56-oracle-index-out-of-range`）。
//!
//! oracle 出处（`src/M2Server/Player/PlayObject.Attack.cs`，行号按 oracle 文件）：
//! `case SKILL_ERGUM` `:1120`、`SKILL_BANWOL` `:1136`、`SKILL_FIRESWORD` `:1168`、
//! `SKILL_MOOTEBO` `:1187`、`SKILL_CROSSMOON` `:1218`、`SKILL_TWINBLADE` `:1234`、`SKILL_43` `:1253`。
//! 这 6 个魔号**不进 `default` 支、不调 `DoSpell`**；3/4/7（同样在 `IsWarrSkill` 名单里）走
//! `default`（`:1269`）⇒ 战士门 ⇒ `RM_MAGICFIREFAIL` ⇒ `SM_MAGICFIRE_FAIL(639)`，由
//! `lib.rs::handle_spell_cmd_with_magic` 既有的 `default_arm && is_warr_skill` 块负责（本模块不碰）。
//!
//! **收件人面（oracle 口径；比较器逐收件人比）**
//! - 12/25/34 的绿字 `SysMsg(..., Green)` ⇒ `SendMsg(RM_SYSMESSAGE, …)` 进**自己**的队列 ⇒
//!   `SM_SYSMESSAGE(100)` **只给自己**；
//! - 26/38 的绿/红字同样只给自己；53（`HealthSpellChanged`）经 `SendUpdateMsg` 只给自己
//!   （`ShowHp` 时另有 `SendRefMsg` 广播，沿用既有 `show_hp` 口径）；
//! - 27 的 `SendRefMsg(RM_RUSHKUNG)` ⇒ `SM_RUSHKUNG(7)` 给 ±`SendRefMsgRange` 内所有 `Play`
//!   （**含自己**，`PlayObject.Message.cs:1527/1538-1541` 的复合条件恒真）；
//! - `SendSocket("+LNG"/"+ULNG"/"+WID"/"+UWID"/"+CRS"/"+UCRS"/"+FIR"/"+TWN")` 短包只给自己。
//!
//! **帧的线上序**（golden 的 `Sent` 序）：短包是直发、先到；队列帧按 `(byte)ident` 出队
//! （53=68 < SM_RUSHKUNG 的 RM 低字节 169 < SM100=252）。故本模块按**线上序**直接发：
//! 短包 → 53 → RUSHKUNG → SM100。
//!
//! **已登记偏差（本模块）**：27 的 `DoMotaebo` **真支**（推挤移动/`SM_RUSH`/`SM_STRUCK`/自伤/
//! 训练尾 `Random(3)+1`）未建模——golden 只有退化输入（`Dir=(byte)nTargetX=101` ⇒ 前方格=自己脚下
//! ⇒ 假支）；合法方向变体排在 `MagicGolden/Program.cs` 释放之后。走到真支条件时**记 WRN**，
//! 仍按退化支发 RUSHKUNG/红字（不伪造推挤帧）。
use crate::downlink;
use crate::magics::MagicTable;
use crate::{OnlineRegistry, SessionGround, SimToGate};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// oracle `SendRefMsg` 的 `(byte)wIdent` 优先级（`MsgQueue` 出队序，低字节先）。
const P_PUSH: u8 = (mir2_protocol::messages::RM_PUSH & 0xFF) as u8; // 156
const P_RUSH: u8 = (mir2_protocol::messages::RM_RUSH & 0xFF) as u8; // 168
const P_RUSHKUNG: u8 = (mir2_protocol::messages::RM_RUSHKUNG & 0xFF) as u8; // 169
const P_STRUCK: u8 = (mir2_protocol::messages::RM_STRUCK & 0xFF) as u8; // 82

pub(crate) enum Arm {
    /// 12 `SKILL_ERGUM`（`Attack.cs:1120-1135`）。
    Ergum,
    /// 25 `SKILL_BANWOL`（`:1136-1151`）。
    Banwol,
    /// 26 `SKILL_FIRESWORD`（`:1168-1186`）。
    Firesword,
    /// 27 `SKILL_MOOTEBO`（`:1187-1217`）。
    Moteebo,
    /// 34 `SKILL_CROSSMOON`（`:1218-1233`）。
    Crossmoon,
    /// 38 `SKILL_TWINBLADE`（`:1234-1252`）。
    Twinblade,
    /// 43 `SKILL_43`（`:1253-1266`）：`Skill43OnOff` + `"+CID"/"+UCID"`。
    Skill43,
}

/// 魔号 → `case` 臂；其余魔号（3/4/7 走 `default`）返回 `None`。
#[must_use]
pub(crate) fn arm(magic_id: u16) -> Option<Arm> {
    match magic_id {
        crate::skill_ability::SKILL_ERGUM => Some(Arm::Ergum),
        crate::skill_ability::SKILL_BANWOL => Some(Arm::Banwol),
        crate::skill_ability::SKILL_FIRESWORD => Some(Arm::Firesword),
        crate::skill_ability::SKILL_MOOTEBO => Some(Arm::Moteebo),
        crate::skill_ability::SKILL_CROSSMOON => Some(Arm::Crossmoon),
        crate::skill_ability::SKILL_TWINBLADE => Some(Arm::Twinblade),
        crate::skill_ability::SKILL_43 => Some(Arm::Skill43),
        _ => None,
    }
}

const SHORT_LNG: &[u8] = b"+LNG";
const SHORT_ULNG: &[u8] = b"+ULNG";
const SHORT_WID: &[u8] = b"+WID";
const SHORT_UWID: &[u8] = b"+UWID";
const SHORT_CRS: &[u8] = b"+CRS";
const SHORT_UCRS: &[u8] = b"+UCRS";
const SHORT_FIR: &[u8] = b"+FIR";
const SHORT_TWN: &[u8] = b"+TWN";
const SHORT_CID: &[u8] = b"+CID";
const SHORT_UCID: &[u8] = b"+UCID";

/// 43 的开关文案：`Skill43OnOff` 的**字面量**（`PlayObject.Base.cs`："开启破空剑"/"关闭破空剑"）。
/// **源码默认 vs 部署**：`StringConf` 不读 `RedHalfMoonOn/Off`、部署 `String.conf` 也无该键；
/// 43/56 共用这两条串，**只能**来自源码默认（同类第二次，第一次是 `AmyOunsulPoint`）。
/// 驱动必须把文案记进 Fixture 并断言尾串，防默认串被改而 golden 静默漂移。
const SKILL43_ON_TEXT: &str = "开启破空剑";
const SKILL43_OFF_TEXT: &str = "关闭破空剑";

/// `AllowTwinHitSkill` 的文案是**字面量**（`PlayObject.Base.cs:670` 直接写死，不走 `[String]`）。
const TWIN_HIT_TEXT: &str = "twin hit skill charged";

/// `SendSocket(string)`：短包只给自己（`SimToGate::SendRaw`）。
fn send_short(
    online: &OnlineRegistry,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    chr_name: &str,
    payload: &[u8],
) {
    let Some(me) = online.find_linear_chr(chr_name) else {
        return;
    };
    let Some(tx) = crate::route_to_gate(gates, me.gate_idx, me.socket, "战士技短包") else {
        return;
    };
    let _ = tx.send(SimToGate::SendRaw {
        socket: me.socket,
        payload: payload.to_vec(),
    });
}

/// `SysMsg(text, color, Hint)` 的非公告支 ⇒ `SendMsg(RM_SYSMESSAGE, …)` **只给自己** ⇒ SM100。
fn send_sys(
    online: &OnlineRegistry,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    chr_name: &str,
    actor_id: i32,
    f_color: u8,
    b_color: u8,
    text: &str,
) {
    let body = crate::smframe::sm_sysmessage(
        actor_id,
        u16::from(f_color),
        u16::from(b_color),
        &downlink::gbk(text),
    );
    crate::send_self_frame(online, gates, chr_name, body);
}

/// `DamageSpell(n)` + `HealthSpellChanged()`（`BaseObject.cs:700-709`）：扣 MP（镜像进实体，
/// 见 `openmir2-mp-session-projection-clobber`），再发 53（自己；`ShowHp` 时另广播）。
fn damage_spell_and_53(
    sg: &mut SessionGround,
    world: &mut mir2_world_bevy::World,
    online: &OnlineRegistry,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    chr_name: &str,
    n: u16,
) {
    sg.state.wabil.mp = sg.state.wabil.mp.saturating_sub(n);
    if let Some(e) = world.entities.get_mut(sg.actor_id) {
        e.vitals.mp = sg.state.wabil.mp;
    }
    let body = crate::smframe::sm_healthspellchanged_frame(
        sg.actor_id,
        sg.state.wabil.hp,
        sg.state.wabil.mp,
        sg.state.wabil.max_hp,
    );
    crate::send_self_frame(online, gates, chr_name, body.clone());
    let show_hp = sg.show_hp;
    if show_hp {
        crate::broadcast_healthspellchanged(
            world,
            online,
            gates,
            sg.actor_id,
            &body,
            crate::chat::send_ref_msg_range(),
        );
    }
}

/// `GetSpellPoint(UserMagic)`（`PlayObject.Base.cs:1369-1372`）：魔表行缺失 ⇒ 0（oracle 侧
/// `userMagic.Magic == null` 会在 `:159` NPE、现实不可达 ⇒ 保守取 0）。
fn spell_point(magic_table: &MagicTable, magic_id: u16, magic_level: u8) -> u16 {
    magic_table.get(magic_id).map_or(0, |m| {
        mir2_world_bevy::magic::get_spell_point(m, magic_level)
    })
}

/// 执行一个 `case` 臂。调用方负责 `GetMagicInfo` 成功（`has_magic`）与公共前缀的守卫；
/// 本函数按 oracle 的行号逐支实现并**直接发帧**（线上序见模块头）。
///
/// 返回 oracle `ClientSpellXY` 的 `result`（这 6 支恒 `true`）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    arm: Arm,
    sg: &mut SessionGround,
    saves: &mut HashMap<String, crate::SavedHuman>,
    world: &mut mir2_world_bevy::World,
    online: &OnlineRegistry,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    displays: &mut downlink::DisplayTable,
    colors: &crate::MsgColors,
    strings: &crate::strings::StringConf,
    magic_level: u8,
    magic_table: &MagicTable,
    magic_id: u16,
    target_x: i32,
    chr_name: &str,
    now: i64,
) -> bool {
    let actor_id = sg.actor_id;
    match arm {
        // `case MagicConst.SKILL_ERGUM`（`Attack.cs:1120-1135`）：
        //   `if (!UseThrusting) { ThrustingOnOff(true); SendSocket("+LNG"); }`
        //   `else { ThrustingOnOff(false); SendSocket("+ULNG"); }`
        Arm::Ergum => {
            if !sg.magic.use_thrusting {
                sg.magic.use_thrusting = true;
                send_short(online, gates, chr_name, SHORT_LNG);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.thrusting_on(),
                );
            } else {
                sg.magic.use_thrusting = false;
                send_short(online, gates, chr_name, SHORT_ULNG);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.thrusting_off(),
                );
            }
        }
        // `case MagicConst.SKILL_BANWOL`（`:1136-1151`）：同 12，开关换 `UseHalfMoon`。
        Arm::Banwol => {
            if !sg.magic.use_half_moon {
                sg.magic.use_half_moon = true;
                send_short(online, gates, chr_name, SHORT_WID);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.half_moon_on(),
                );
            } else {
                sg.magic.use_half_moon = false;
                send_short(online, gates, chr_name, SHORT_UWID);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.half_moon_off(),
                );
            }
        }
        // `case MagicConst.SKILL_CROSSMOON`（`:1218-1233`）：同 12，开关换 `CrsHitkill`。
        Arm::Crossmoon => {
            if !sg.magic.crs_hitkill {
                sg.magic.crs_hitkill = true;
                send_short(online, gates, chr_name, SHORT_CRS);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.crs_hit_on(),
                );
            } else {
                sg.magic.crs_hitkill = false;
                send_short(online, gates, chr_name, SHORT_UCRS);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.crs_hit_off(),
                );
            }
        }
        // `case MagicConst.SKILL_43`（`:1253-1266`）：`Skill43OnOff` + `"+CID"/"+UCID"`。
        Arm::Skill43 => {
            if !sg.magic.mbo43_kill {
                sg.magic.mbo43_kill = true;
                send_short(online, gates, chr_name, SHORT_CID);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    SKILL43_ON_TEXT,
                );
            } else {
                sg.magic.mbo43_kill = false;
                send_short(online, gates, chr_name, SHORT_UCID);
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    SKILL43_OFF_TEXT,
                );
            }
        }
        // `case MagicConst.SKILL_FIRESWORD`（`:1168-1186`）：
        //   `AllowFireHitSkill()`（`PlayObject.Base.cs:653-664`）：10s 门内 ⇒ 红字、false；
        //   否则刷 tick + `FireHitSkill = true` + 绿字、true。true 支再按 MP 扣减 + 53 + `+FIR`。
        Arm::Firesword => {
            let allowed = if now - sg.magic.latest_fire_hit_tick > 10 * 1000 {
                sg.magic.latest_fire_hit_tick = now;
                sg.magic.fire_hit_skill = true;
                true
            } else {
                false
            };
            if !allowed {
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.red_f,
                    colors.red_b,
                    strings.fire_spirits_fail(),
                );
                return true;
            }
            let n = spell_point(magic_table, magic_id, magic_level);
            let mp = sg.state.wabil.mp;
            if mp >= n {
                // 短包直发先到；53 走自己的队列（68）⇒ 线上 `+FIR → 53 → SM100`。
                send_short(online, gates, chr_name, SHORT_FIR);
                if n > 0 {
                    damage_spell_and_53(sg, world, online, gates, chr_name, n);
                }
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.fire_spirits_summoned(),
                );
            } else {
                // MP 不足：绿字已发，无扣减/无 53/无 `+FIR`。
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    strings.fire_spirits_summoned(),
                );
            }
        }
        // `case MagicConst.SKILL_MOOTEBO`（`Attack.cs:1187-1217`）：3s 门内只置 `result = true`（零帧）；
        // 门外：`Dir = (byte)nTargetX` + GetSpellPoint/DamageSpell/53 + `DoMotaebo(Dir, Level)`
        // **真支**（`PlayObject.Base.cs:1390-1503` ⇒ world facade
        // [`mir2_world_bevy::World::do_motaebo`]：推挤/自移/伤害/自伤，含 `n24`/`n28` byte
        // 回绕 BUG-REPLICA）+ 调用点训练尾（`:1201-1214`，**严格 <** 门）。
        // 帧序：按执行序入 `SpellFrameBuf`，flush 按 per-收件人 .NET 四叉堆出队序还原
        // oracle `MsgQueue` 序（golden `warr-27-true.json` 的 `Sent` 逐帧一致）。
        Arm::Moteebo => {
            if now - sg.magic.do_motaebo_tick <= 3 * 1000 {
                return true;
            }
            sg.magic.do_motaebo_tick = now;
            // `Dir = (byte)nTargetX`（oracle 把目标 X 当方向；非法方向 ⇒ 前格 = 自己脚下 ⇒ 红字支）。
            let dir = (target_x & 0xFF) as u8;
            let n = spell_point(magic_table, magic_id, magic_level);
            if sg.state.wabil.mp >= n {
                if n > 0 {
                    damage_spell_and_53(sg, world, online, gates, chr_name, n);
                }
                let caster_abil_level = sg.state.wabil.level;
                let outcome = world.do_motaebo(actor_id, dir, magic_level, caster_abil_level);
                let mut buf = crate::SpellFrameBuf::new();
                for ev in &outcome.events {
                    match ev {
                        mir2_world_bevy::world::MotaeboEvent::Push(pe) => {
                            let Some(d) = displays.get(pe.actor_id) else {
                                crate::glog!(
                                    "WRN",
                                    "27 推挤：actor={} 无展示表 ⇒ 跳过 SM_BACKSTEP（登记）",
                                    pe.actor_id
                                );
                                continue;
                            };
                            if let Some(body) = downlink::backstep_frame(pe, d) {
                                // oracle `CharPushed` 每推进一格立即 `SendRefMsg(RM_PUSH)`
                                // ⇒ 收件人按**该步**坐标算（firewind 同款）。
                                let map_id =
                                    world.entities.get(pe.actor_id).map_or(0, |e| e.map_id);
                                buf.push_broadcast_cached_at(
                                    world,
                                    online,
                                    pe.actor_id,
                                    true,
                                    (map_id, pe.x, pe.y),
                                    P_PUSH,
                                    &body,
                                );
                            }
                            // `CharPushed` 收尾把 `Dir` 置为 `nBackDir`（推进 > 0）⇒ 展示表同步。
                            if let Some(e) = world.entities.get(pe.actor_id) {
                                crate::set_display_dir(displays, pe.actor_id, e.dir());
                            }
                        }
                        mir2_world_bevy::world::MotaeboEvent::Rush(re) => {
                            let Some(d) = displays.get(re.actor_id) else {
                                crate::glog!(
                                    "WRN",
                                    "27 自移：actor={} 无展示表 ⇒ 跳过 SM_RUSH/RUSHKUNG（登记）",
                                    re.actor_id
                                );
                                continue;
                            };
                            if let Some(body) = downlink::rush_frame(re, d) {
                                let prio =
                                    if re.frame == mir2_world_bevy::action::RushFrame::RushKung {
                                        P_RUSHKUNG
                                    } else {
                                        P_RUSH
                                    };
                                buf.push_broadcast_cached(
                                    world,
                                    online,
                                    re.actor_id,
                                    true,
                                    prio,
                                    &body,
                                );
                            }
                            if let Some(e) = world.entities.get(re.actor_id) {
                                crate::set_display_dir(displays, re.actor_id, e.dir());
                            }
                        }
                    }
                }
                for s in &outcome.struck {
                    // 31 帧：收件人 = 受击者视野内玩家（含受击者）；帧尾 = 受击者 CharDesc
                    // （golden 怪尾 16B `EncodePacket(CharDesc)`）。
                    let Some(d) = displays.get(s.target_id) else {
                        crate::glog!(
                            "WRN",
                            "27 受击：actor={} 无展示表 ⇒ 跳过 SM_STRUCK（登记）",
                            s.target_id
                        );
                        continue;
                    };
                    let body = crate::smframe::sm_struck_frame(
                        s.target_id,
                        s.hp,
                        s.max_hp,
                        s.damage,
                        d.feature,
                        d.status,
                        s.attacker_id,
                        false,
                    );
                    buf.push_broadcast_cached(world, online, s.target_id, true, P_STRUCK, &body);
                    // 自伤会话镜像（HP 权威在实体；`drain_combat_downlink` 的受害者同步同口径）。
                    if s.is_self {
                        sg.state.wabil.hp = s.hp;
                    }
                }
                buf.flush(online, gates);
                // bo35 红字：`SM_SYSMESSAGE(100)` 只给施法者；优先级 252 恒末位 ⇒ 堆 flush 之后直发。
                if outcome.bo35 {
                    send_sys(
                        online,
                        gates,
                        chr_name,
                        actor_id,
                        colors.red_f,
                        colors.red_b,
                        strings.mate_do_tooweak(),
                    );
                }
                // 训练尾（调用点 `Attack.cs:1201-1214`）：`result && Level<3 &&
                // TrainLevel[L] < Abil.Level`（**严格 <**——与 MagicManager 共享尾的 `<=`
                // 不同源，勿改共享件、勿共享实现）。
                if outcome.result && magic_level < 3 {
                    if let Some(mi) = magic_table.get(magic_id) {
                        if u32::from(mi.train_level[usize::from(magic_level)])
                            < u32::from(caster_abil_level)
                        {
                            // `TrainSkill(UserMagic, Random(3) + 1)`（收敛件；FastTrain 未装载 ⇒ false）
                            let pts =
                                mir2_world_bevy::magic_effect::spell_train_points(&mut world.rng);
                            let cur = saves
                                .get(chr_name)
                                .and_then(|sv| sv.record.data.magic.as_ref())
                                .and_then(|list| {
                                    list.iter().flatten().find(|m| m.mag_idx == magic_id)
                                })
                                .map_or(0, |m| m.tran_point);
                            let mut train = mir2_world_bevy::magic::SkillTrainState {
                                magic_id,
                                level: magic_level,
                                tran_point: cur,
                                train_lv: mi.train_lv,
                                train_level: mi.train_level,
                                max_train: mi.max_train,
                            };
                            train.tran_point += mir2_world_bevy::magic::train_skill(false, pts);
                            let up = mir2_world_bevy::magic::check_magic_level_up(&mut train);
                            if up.result {
                                // 升级：`CheckMagicLevelUp` 内部 `UpdateDelayMsg(…, 800)`
                                // （`PlayObject.cs:5216`，先删同 `(ident, MagicId)` 待投递）。
                                world.update_self_delay_msg_full(
                                    actor_id,
                                    mir2_world_bevy::slave::RM_MAGIC_LVEXP,
                                    0,
                                    i32::from(magic_id),
                                    i32::from(train.level),
                                    train.tran_point,
                                    800,
                                );
                            } else {
                                // 未升级：调用点 `SendSelfDelayMsg(…, 1000)`（`Attack.cs:1210`）。
                                world.send_self_delay_msg_full(
                                    actor_id,
                                    mir2_world_bevy::slave::RM_MAGIC_LVEXP,
                                    0,
                                    i32::from(magic_id),
                                    i32::from(train.level),
                                    train.tran_point,
                                    1000,
                                );
                            }
                            // 熟练度回写存档记录（`finish_spell_tail` 同口径）。
                            if let Some(rcd) = saves
                                .get_mut(chr_name)
                                .and_then(|sv| sv.record.data.magic.as_mut())
                                .and_then(|list| {
                                    list.iter_mut().flatten().find(|m| m.mag_idx == magic_id)
                                })
                            {
                                rcd.level = train.level;
                                rcd.tran_point = train.tran_point;
                            }
                        }
                    }
                }
            }
        }
        // `case MagicConst.SKILL_TWINBLADE`（`:1234-1252`）：`AllowTwinHitSkill()` 恒真
        // （刷 tick + `TwinHitSkill = true` + 绿字），随后同 26 的 MP 扣减 + 53 + `+TWN`。
        Arm::Twinblade => {
            sg.magic.latest_twin_hit_tick = now;
            sg.magic.twin_hit_skill = true;
            let n = spell_point(magic_table, magic_id, magic_level);
            let mp = sg.state.wabil.mp;
            if mp >= n {
                send_short(online, gates, chr_name, SHORT_TWN);
                if n > 0 {
                    damage_spell_and_53(sg, world, online, gates, chr_name, n);
                }
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    TWIN_HIT_TEXT,
                );
            } else {
                send_sys(
                    online,
                    gates,
                    chr_name,
                    actor_id,
                    colors.green_f,
                    colors.green_b,
                    TWIN_HIT_TEXT,
                );
            }
        }
    }
    true
}
