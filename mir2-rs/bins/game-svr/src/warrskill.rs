//! 卡 `openmir2-warrskill-entry` 要求 4：`ClientSpellXY` 的 **6 个战士技独立 `case` 臂**
//! （12/25/26/27/34/38）。
//!
//! oracle 出处（`src/M2Server/Player/PlayObject.Attack.cs`，行号按 oracle 文件）：
//! `case SKILL_ERGUM` `:1120`、`SKILL_BANWOL` `:1136`、`SKILL_FIRESWORD` `:1168`、
//! `SKILL_MOOTEBO` `:1187`、`SKILL_CROSSMOON` `:1218`、`SKILL_TWINBLADE` `:1234`。
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
}

/// 魔号 → `case` 臂；其余魔号（含 3/4/7 与 43/56）返回 `None`。
#[must_use]
pub(crate) fn arm(magic_id: u16) -> Option<Arm> {
    match magic_id {
        crate::skill_ability::SKILL_ERGUM => Some(Arm::Ergum),
        crate::skill_ability::SKILL_BANWOL => Some(Arm::Banwol),
        crate::skill_ability::SKILL_FIRESWORD => Some(Arm::Firesword),
        crate::skill_ability::SKILL_MOOTEBO => Some(Arm::Moteebo),
        crate::skill_ability::SKILL_CROSSMOON => Some(Arm::Crossmoon),
        crate::skill_ability::SKILL_TWINBLADE => Some(Arm::Twinblade),
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

/// `AllowTwinHitSkill` 的文案是**字面量**（`PlayObject.Base.cs:670` 直接写死，不走 `[String]`）。
const TWIN_HIT_TEXT: &str = "twin hit skill charged";

/// `SendSocket(string)`：短包只给自己（`SimToGate::SendRaw`）。
fn send_short(
    online: &OnlineRegistry,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    chr_name: &str,
    payload: &[u8],
) {
    let Some(me) = online.iter().find(|e| e.chr_name == chr_name) else {
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
    world: &mut mir2_world_bevy::World,
    online: &OnlineRegistry,
    gates: &HashMap<u8, mpsc::UnboundedSender<SimToGate>>,
    displays: &downlink::DisplayTable,
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
        // `case MagicConst.SKILL_MOOTEBO`（`:1187-1217`）：3s 门内只置 `result = true`（零帧）。
        Arm::Moteebo => {
            if now - sg.magic.do_motaebo_tick <= 3 * 1000 {
                return true;
            }
            sg.magic.do_motaebo_tick = now;
            // `Dir = (byte)nTargetX`（oracle 把目标 X 当方向）。
            let dir = (target_x & 0xFF) as u8;
            let n = spell_point(magic_table, magic_id, magic_level);
            if sg.state.wabil.mp >= n {
                if n > 0 {
                    damage_spell_and_53(sg, world, online, gates, chr_name, n);
                }
                // `DoMotaebo(Dir, Level)`：退化支 = `GetFrontPosition` 对非法方向不改坐标
                // （`BaseObject.cs:977-982` 先写 `CurrX/CurrY` 再 switch）⇒ 前方格 = 自己脚下；
                // `GetPoseCreate` 取到施法者本人 ⇒ `DoMotaeboCanMotaebo` 的
                // `Abil.Level > 自己等级` 为假 ⇒ break（`n28 = 0`、`baseObject34 = null`），
                // 但 `bo35` 仍真 ⇒ `RM_RUSHKUNG(Dir, CurrX, CurrY)` + 红字（`:1500-1505`）。
                let (cur_x, cur_y) = world
                    .entities
                    .get(actor_id)
                    .map_or((0i32, 0i32), |e| (e.x, e.y));
                // **真支未建模（登记偏差）**：前方格若有别的对象且等级更低，oracle 会推挤/伤害/
                // 训练（`DoMotaebo` 的 `result = true` 分支）；本宿主只支持退化支 ⇒ WRN。
                let front_other = world
                    .entities
                    .iter_send_ref_targets()
                    .any(|t| t.id != actor_id && t.x == cur_x && t.y == cur_y);
                if front_other {
                    crate::glog!(
                        "WRN",
                        "CM_SPELL 27：前方格另有对象 ⇒ DoMotaebo 真支（推挤/伤害/训练尾）未建模（卡 openmir2-warrskill-entry，登记偏差）；按退化支发 RUSHKUNG"
                    );
                }
                if let Some(d) = displays.get(actor_id) {
                    let body = crate::smframe::sm_move(
                        mir2_protocol::messages::SM_RUSHKUNG,
                        actor_id,
                        cur_x as u16,
                        cur_y as u16,
                        dir,
                        d.light,
                        &downlink::char_desc(d.feature, d.status),
                    );
                    crate::broadcast_ref_msg_with_self(world, online, gates, actor_id, &[body]);
                } else {
                    crate::glog!(
                        "WRN",
                        "CM_SPELL 27：actor={actor_id} 无展示表 ⇒ SM_RUSHKUNG 无法编码（登记）"
                    );
                }
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
