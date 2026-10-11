//! 动作处理器（`ExecutionProcessingSys`）与 `ScriptEngine` 的动作 switch（1:1 移植）。
//!
//! 注意 C# 语义：注册表以**枚举值**为键。历史上解析器产出 `枚举值-1`（whitelist B-8/T-4），
//! 脚本写的命令会落到「前一个」成员的实现上；**B2/M4 已同批翻转**（`parser.rs` 现取字段序号，
//! 与 C# S3 修正后一致）⇒ 本模块按 CmdCode 直查即命中“这条命令自己的”处理器。

use crate::engine::{DynVar, DynVarKind, Engine, EngineError, VarVal};
use crate::model::{QuestActionInfo, QuestConditionInfo};

/// C# `Messages` 常量（`src/OpenMir2/Messages.cs`）。
pub const RM_MERCHANTDLGCLOSE: i32 = 10127;
pub const RM_MENU_OK: i32 = 10309;

/// C# `Grobal2.StringGoldName`（金币名）——`GotoLableTakeItem`/`GiveItem` 按它区分金币与物品。
pub const GOLD_NAME: &str = "金币";

/// C# `Messages` 常量（`src/OpenMir2/Messages.cs`）。
pub const SM_QUERYVALUE: i32 = 215;
pub const SM_OUTOFCONNECTION: i32 = 528;
pub const SM_QUERYITEMDLG: i32 = 623;
pub const RM_DURACHANGE: i32 = 10125;
pub const RM_PLAYDICE: i32 = 10500;
pub const RM_WINEXP: i32 = 8075;
pub const RM_ADJUST_BONUS: i32 = 8078;
pub const RM_SPACEMOVE_FIRE: i32 = 8062;

/// `MsgColor`（`src/Modules/SystemModule/Enums/MsgColor.cs`）。
pub const MSG_COLOR_RED: i32 = 0;
pub const MSG_COLOR_GREEN: i32 = 1;
pub const MSG_COLOR_BLUE: i32 = 2;
/// `MsgType`（`src/Modules/SystemModule/Enums/MsgType.cs`）。
pub const MSG_TYPE_HINT: i32 = 2;
pub const MSG_TYPE_SAY: i32 = 3;

/// `ItemLocation.Charm`（`src/OpenMir2/Enums/ItemLocation.cs`）。
pub const ITEM_LOCATION_CHARM: i32 = 12;

/// `Grobal2.MaxLevel`（`= byte.MaxValue`）。
pub const MAX_LEVEL: i32 = 255;

/// `s[0]`（C# 空串会抛 `IndexOutOfRangeException`；此处落 `'\0'` 走 default 分支，绝不 panic）。
fn first_char(s: &str) -> char {
    s.chars().next().unwrap_or('\0')
}

/// `HUtil32.MakeWord(ushort low, ushort high)` = `(ushort)(low | high << 8)`。
fn make_word(low: u16, high: u16) -> u16 {
    low | (high << 8)
}

/// `HUtil32.MakeLong(ushort low, ushort high)` = `low | high << 16`（int 语义，可带符号位）。
fn make_long(low: u16, high: u16) -> i32 {
    (low as i32) | ((high as i32) << 16)
}

/// `ItemLocation`（`src/OpenMir2/Enums/ItemLocation.cs`）——佩戴位号即 `UseItems` 下标。
mod item_location {
    pub const DRESS: i32 = 0;
    pub const WEAPON: i32 = 1;
    pub const NECKLACE: i32 = 3;
    pub const HELMET: i32 = 4;
    pub const ARMRINGL: i32 = 5;
    pub const ARMRINGR: i32 = 6;
    pub const RINGL: i32 = 7;
    pub const RINGR: i32 = 8;
    pub const BUJUK: i32 = 9;
    pub const BELT: i32 = 10;
    pub const BOOTS: i32 = 11;
    pub const CHARM: i32 = 12;
}

impl Engine<'_, '_> {
    /// 已注册动作的派发。
    pub(crate) fn run_action(&mut self, code: i32, info: &QuestActionInfo, result: &mut bool) {
        let handler = crate::engine::dispatch::execution_handler(code).unwrap_or("");
        match handler {
            "ActionOfClose" => {
                // playerActor.SendMsg(normNpc, RM_MERCHANTDLGCLOSE, 0, normNpc.ActorId, 0, 0)
                //   ⇒ (wIdent, wParam, nParam1, nParam2, nParam3) = (10127, 0, ActorId, 0, 0)
                let npc_id = self.npc.actor_id();
                self.player
                    .send_msg(RM_MERCHANTDLGCLOSE, 0, npc_id, 0, 0, None);
            }
            "ActionOfMessageBox" => {
                // playerActor.SendMsg(normNpc, RM_MENU_OK, 0, playerActor.ActorId, 0, 0,
                //                     GetLineVariableText(playerActor, sParam1))
                //   ⇒ (wIdent, wParam, nParam1, nParam2, nParam3) = (10309, 0, ActorId, 0, 0)
                let actor_id = self.player.actor_id();
                let text = self.player.line_variable_text(&info.s_param1);
                self.player
                    .send_msg(RM_MENU_OK, 0, actor_id, 0, 0, Some(&text));
            }
            "ActionOfMobFireBurn" => {
                // C#：参数不全时 ScriptActionError(ExecutionCode.MobFireburn) 后返回
                let s_map = &info.s_param1;
                let nx = mir2_shared::hutil32::str_to_int16(&info.s_param2, -1);
                let ny = mir2_shared::hutil32::str_to_int16(&info.s_param3, -1);
                let n_type = mir2_shared::hutil32::str_to_int(&info.s_param4, -1);
                let n_time = mir2_shared::hutil32::str_to_int(&info.s_param5, -1);
                let n_point = mir2_shared::hutil32::str_to_int(&info.s_param6, -1);
                if s_map.is_empty() || nx < 0 || ny < 0 || n_type < 0 || n_time < 0 || n_point < 0 {
                    self.script_action_error("MobFireburn", info);
                    return;
                }
                // 参数完整时的地图效果依赖 MapMgr（批次 2 后续）
                self.errors.push(EngineError::NotImplemented {
                    kind: "action",
                    code,
                    handler: "ActionOfMobFireBurn(地图效果)",
                });
            }
            "ActionOfResetUnit" => {
                // C#: for (k = 0; k < nParam2; k++) SetQuestUnitStatus(nParam1 + k, 0)
                for k in 0..info.n_param2 {
                    self.player.set_quest_unit_status(info.n_param1 + k, 0);
                }
            }
            "ActionOfMapMove" => {
                // C# `ActionOfMapMove`：SendRefMsg(RM_SPACEMOVE_FIRE, …) + SpaceMove(sParam1, nParam2, nParam3, 0)
                self.player
                    .space_move(&info.s_param1, info.n_param2 as i16, info.n_param3 as i16);
            }
            "ActionOfGiveItem" => {
                // C# `GotoLableGiveItem(sParam1, nParam2)`：金币名 → IncGold(nParam2)；
                // 否则 nParam2 夹到 1..=50 后逐件入包（此处用 give_item 一次性交付，夹具日志口径同为 additem）。
                if info.s_param1.eq_ignore_ascii_case(GOLD_NAME) {
                    self.player.inc_gold(info.n_param2);
                } else {
                    let mut n = info.n_param2;
                    if !(1..=50).contains(&n) {
                        n = 1;
                    }
                    self.player.give_item(&info.s_param1, n);
                }
            }
            "ActionOfSet" => {
                // C#: int n28 = StrToInt(sParam1, 0); int n2C = StrToInt(sParam2, 0);
                //     playerActor.SetQuestFlagStatus(n28, n2C);
                let n28 = mir2_shared::hutil32::str_to_int(&info.s_param1, 0);
                let n2c = mir2_shared::hutil32::str_to_int(&info.s_param2, 0);
                self.player.set_quest_flag_status(n28, n2c);
            }
            "ActionOfReSet" => {
                // C#: for (k = 0; k < nParam2; k++) SetQuestFlagStatus(nParam1 + k, 0)
                for k in 0..info.n_param2 {
                    self.player.set_quest_flag_status(info.n_param1 + k, 0);
                }
            }
            "ActionOfSetOpen" => {
                let n28 = mir2_shared::hutil32::str_to_int(&info.s_param1, 0);
                let n2c = mir2_shared::hutil32::str_to_int(&info.s_param2, 0);
                self.player.set_quest_unit_open_status(n28, n2c);
            }
            "ActionOfSetUnit" => {
                let n28 = mir2_shared::hutil32::str_to_int(&info.s_param1, 0);
                let n2c = mir2_shared::hutil32::str_to_int(&info.s_param2, 0);
                self.player.set_quest_unit_status(n28, n2c);
            }
            // ---- B2/M4 动作批次（2026-10-11）----
            "ActionOfLineMsg" => {
                // C# `ActionOfLineMsg`：文本先 `%s`→玩家名、`%d`→NPC 名，再按 nParam1 分发
                let mut s_msg = self.player.line_variable_text(&info.s_param2);
                let chr_name = self.player.chr_name();
                let npc_name = self.npc.chr_name();
                s_msg = s_msg.replace("%s", &chr_name);
                s_msg = s_msg.replace("%d", &npc_name);
                match info.n_param1 {
                    0 => self.player.world_broadcast(&s_msg),
                    1 => self.player.world_broadcast(&format!("(*) {s_msg}")),
                    2 => self.player.world_broadcast(&format!("[{npc_name}]{s_msg}")),
                    3 => self.player.world_broadcast(&format!("[{chr_name}]{s_msg}")),
                    4 => self.npc.send_say_msg(&s_msg),
                    5 => self.player.sys_msg(&s_msg, MSG_COLOR_RED, MSG_TYPE_SAY),
                    6 => self.player.sys_msg(&s_msg, MSG_COLOR_GREEN, MSG_TYPE_SAY),
                    7 => self.player.sys_msg(&s_msg, MSG_COLOR_BLUE, MSG_TYPE_SAY),
                    8 => self.player.send_group_text(&s_msg),
                    // C# `MyGuild != null` 才发公会消息
                    9 => {
                        if self.player.guild_name().is_some() {
                            self.player.guild_send_msg(&s_msg);
                        }
                    }
                    _ => self.script_action_error("SendMsg", info),
                }
            }
            "ActionOfGameGold" => {
                // C# `ActionOfGameGold`：`=` 赋值 / `-` `+` 夹 0；变更后 `GameGoldChanged()`
                let old = self.player.game_gold();
                let mut n = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
                if n < 0 {
                    self.script_action_error("GameGold", info);
                    return;
                }
                match first_char(&info.s_param1) {
                    // C# 内层 `if (nGameGold >= 0)` 恒真
                    '=' => self.player.set_game_gold(n),
                    '-' => {
                        n = self.player.game_gold().wrapping_sub(n).max(0);
                        self.player.set_game_gold(n);
                    }
                    '+' => {
                        n = self.player.game_gold().wrapping_add(n).max(0);
                        self.player.set_game_gold(n);
                    }
                    _ => {}
                }
                if old != self.player.game_gold() {
                    self.player.game_gold_changed();
                }
            }
            "ActionOfGamePoint" => {
                // C# `ActionOfGamePoint`：字段与分支同 GameGold，末端同样调 `GameGoldChanged()`
                let old = self.player.game_point();
                let mut n = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
                if n < 0 {
                    self.script_action_error("GamePoint", info);
                    return;
                }
                match first_char(&info.s_param1) {
                    '=' => self.player.set_game_point(n),
                    '-' => {
                        n = self.player.game_point().wrapping_sub(n).max(0);
                        self.player.set_game_point(n);
                    }
                    '+' => {
                        n = self.player.game_point().wrapping_add(n).max(0);
                        self.player.set_game_point(n);
                    }
                    _ => {}
                }
                if old != self.player.game_point() {
                    self.player.game_gold_changed();
                }
            }
            "ActionOfChangeCreditPoint" => {
                // C# 先把 sParam2 收成 `byte`（`(byte)StrToInt(…)`，截断后**恒 >= 0**，
                // 故原文 `if (nCreditPoint < 0) ScriptActionError` 分支不可达，此处省略）
                let n = mir2_shared::hutil32::str_to_int(&info.s_param2, -1) as u8;
                match first_char(&info.s_param1) {
                    '=' => {
                        // 内层 `nCreditPoint >= 0` 恒真、`> byte.MaxValue` 恒假
                        self.player.set_credit_point(n as i32);
                    }
                    '-' => {
                        if self.player.credit_point() > n as i32 {
                            let v = self.player.credit_point() - n as i32;
                            self.player.set_credit_point(v);
                        } else {
                            self.player.set_credit_point(0);
                        }
                    }
                    '+' => {
                        if self.player.credit_point() + n as i32 > 255 {
                            self.player.set_credit_point(255);
                        } else {
                            let v = self.player.credit_point() + n as i32;
                            self.player.set_credit_point(v);
                        }
                    }
                    _ => self.script_action_error("CreditPoint", info),
                }
            }
            "ActionOfChangeExp" => {
                // C# `ActionOfChangeExp`：`Abil` 是引用类型，`abil.Exp` 直接改在玩家身上
                let n_exp = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
                if n_exp < 0 {
                    self.script_action_error("ChangeExp", info);
                    return;
                }
                match first_char(&info.s_param1) {
                    '=' => {
                        if n_exp > 0 {
                            self.player.set_abil_exp(n_exp);
                        }
                    }
                    '-' => {
                        if self.player.abil_exp() > n_exp {
                            let v = self.player.abil_exp() - n_exp;
                            self.player.set_abil_exp(v);
                        } else {
                            self.player.set_abil_exp(0);
                        }
                    }
                    '+' => {
                        // C# 溢出夹取（unchecked 语义），照搬其两段比较
                        let cur = self.player.abil_exp();
                        let d = if cur >= n_exp {
                            if cur.wrapping_sub(n_exp) > i32::MAX.wrapping_sub(cur) {
                                i32::MAX.wrapping_sub(cur)
                            } else {
                                n_exp
                            }
                        } else if n_exp.wrapping_sub(cur) > i32::MAX.wrapping_sub(n_exp) {
                            i32::MAX.wrapping_sub(n_exp)
                        } else {
                            n_exp
                        };
                        self.player.set_abil_exp(cur.wrapping_add(d));
                        // C# `SendMsg(playerActor, RM_WINEXP, 0, dwInt, 0, 0)`
                        self.player.send_msg(RM_WINEXP, 0, d, 0, 0, None);
                    }
                    _ => {}
                }
            }
            "ActionOfChangeLevel" => {
                let old_level = self.player.level();
                let n_level = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
                if n_level < 0 {
                    self.script_action_error("ChangeLevel", info);
                    return;
                }
                let cur = self.player.level();
                let mut chg_ok = false;
                let new_level = match first_char(&info.s_param1) {
                    '=' => {
                        if n_level > 0 && n_level <= MAX_LEVEL {
                            chg_ok = true;
                            n_level
                        } else {
                            cur
                        }
                    }
                    '-' => {
                        chg_ok = true;
                        cur.wrapping_sub(n_level).clamp(0, MAX_LEVEL)
                    }
                    '+' => {
                        chg_ok = true;
                        cur.wrapping_add(n_level).clamp(0, MAX_LEVEL)
                    }
                    _ => cur,
                };
                if chg_ok {
                    self.player.set_abil_level(new_level);
                    self.player.has_level_up(old_level);
                }
            }
            "ActionOfChangePkPoint" => {
                let old_pk_level = self.player.pvp_level();
                let n_pk = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
                if n_pk < 0 {
                    self.script_action_error("ChangePkPoint", info);
                    return;
                }
                match first_char(&info.s_param1) {
                    // C# 内层 `nPkPoint >= 0` 恒真
                    '=' => self.player.set_pk_point(n_pk),
                    '-' => {
                        let v = self.player.pk_point().wrapping_sub(n_pk).max(0);
                        self.player.set_pk_point(v);
                    }
                    '+' => {
                        let v = self.player.pk_point().wrapping_add(n_pk).max(0);
                        self.player.set_pk_point(v);
                    }
                    _ => {}
                }
                if old_pk_level != self.player.pvp_level() {
                    self.player.ref_name_color();
                }
            }
            "ActionOfChangeHairStyle" => {
                let n_hair = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
                if !info.s_param1.is_empty() && n_hair >= 0 {
                    self.player.set_hair(n_hair as u8);
                    self.player.feature_changed();
                } else {
                    self.script_action_error("Hairstyle", info);
                }
            }
            "ActionOfHumanHp" | "ActionOfHumanMp" => {
                // C# `ActionOfHumanHp` / `ActionOfHumanMp`：同构，只换字段与提示串
                let is_hp = handler == "ActionOfHumanHp";
                let n = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
                if n < 0 {
                    self.script_action_error(if is_hp { "HumanHp" } else { "HumanMp" }, info);
                    return;
                }
                let cur = if is_hp {
                    self.player.abil_hp()
                } else {
                    self.player.abil_mp()
                };
                // C# `(ushort)nHp` 截断；字段是 `ushort`，运算后回写同样按 16 位截断
                let low = (n as u16) as i32;
                let new = match first_char(&info.s_param1) {
                    '=' => low,
                    '-' => {
                        // 注意 C# 比较的是 `WAbil.HP/MP`（观察者快照），不是 Abil
                        let w = if is_hp {
                            self.player.wabil_hp()
                        } else {
                            self.player.wabil_mp()
                        };
                        if w >= n {
                            (cur.wrapping_sub(low)) & 0xFFFF
                        } else {
                            0
                        }
                    }
                    '+' => {
                        let v = (cur.wrapping_add(low)) & 0xFFFF;
                        // C# 判断写在 `WAbil.HP > WAbil.MaxHP` 上（原文如此）
                        let (w, max) = if is_hp {
                            (self.player.wabil_hp(), self.player.wabil_max_hp())
                        } else {
                            (self.player.wabil_mp(), self.player.wabil_max_mp())
                        };
                        if w > max {
                            max
                        } else {
                            v
                        }
                    }
                    _ => cur,
                };
                if is_hp {
                    self.player.set_abil_hp(new);
                } else {
                    self.player.set_abil_mp(new);
                }
                if self.player.config_show_script_action_msg() {
                    let max = if is_hp {
                        self.player.wabil_max_hp()
                    } else {
                        self.player.wabil_max_mp()
                    };
                    let msg = if is_hp {
                        format!("当前HP值为:{max}")
                    } else {
                        format!("当前MP值为:{max}")
                    };
                    self.player.sys_msg(&msg, MSG_COLOR_GREEN, MSG_TYPE_HINT);
                }
            }
            "ActionOfKick" => {
                // C# `ActionOfKick`：三个标志位置真
                self.player.set_kick_flags();
            }
            "ActionOfKill" => {
                let n_mode = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
                if (0..=3).contains(&n_mode) {
                    match n_mode {
                        1 => {
                            self.player.set_no_item(true);
                            self.player.die();
                        }
                        2 => {
                            self.player.set_last_hiter_npc();
                            self.player.die();
                        }
                        3 => {
                            self.player.set_no_item(true);
                            self.player.set_last_hiter_npc();
                            self.player.die();
                        }
                        // 0（default 分支）
                        _ => self.player.die(),
                    }
                } else {
                    self.script_action_error("Kill", info);
                }
            }
            "ActionOfMission" => {
                if !info.s_param1.is_empty() && info.n_param2 > 0 && info.n_param3 > 0 {
                    self.player
                        .set_mission(&info.s_param1, info.n_param2, info.n_param3);
                } else {
                    self.script_action_error("Mission", info);
                }
            }
            "ActionOfMobPlace" => {
                // C# 方法体：`nX/nY/nCount/nRange` 全为 0 ⇒ `for (i = 0; i < nCount; i++)` 不执行，
                // 无任何副作用（原文如此，照搬）
            }
            "ActionOfMap" => {
                // C# `ActionOfMap`：先 `SendRefMsg(RM_SPACEMOVE_FIRE, 0,0,0,0, "")` 再 `MapRandomMove(map, 0)`
                self.player.send_ref_msg(RM_SPACEMOVE_FIRE, 0, 0, 0, 0, "");
                self.player.map_random_move(&info.s_param1, 0);
            }
            "ActionOfClearDelayGoto" => {
                self.player.clear_delay_goto();
            }
            "ActionOfBreakTimereCall" => {
                self.player.set_is_time_recall(false);
            }
            "ActionOfTimereCall" => {
                // C#：`TimeRecallTick = GetTickCount() + nParam1 * 60 * 1000`
                let tick = self.player.tick_count();
                let map = self.player.map_name();
                let (x, y) = self.player.curr_pos();
                let recall_tick = tick.wrapping_add(info.n_param1.wrapping_mul(60_000));
                self.player.set_time_recall(&map, x, y, recall_tick);
            }
            "ActionOfDelayCall" => {
                let tick = self.player.tick_count();
                let delay = info.n_param1.max(1);
                // C#：先按行变量文本取数，取不到（==0）再走 `GetValValue`
                let text = self.player.line_variable_text(&info.s_param1);
                let mut m_delay = mir2_shared::hutil32::str_to_int(&text, 0);
                if m_delay == 0 {
                    if let Some(v) = self.get_val_value_int(&info.s_param1) {
                        m_delay = v;
                    }
                }
                let goto_tick = if m_delay > 0 {
                    tick.wrapping_add(m_delay)
                } else {
                    tick.wrapping_add(info.n_param1)
                };
                let npc_id = self.npc.actor_id();
                self.player
                    .set_delay_call(delay, &info.s_param2, npc_id, goto_tick);
            }
            "ActionOfSetScriptFlag" => {
                let n_where = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
                let bo_flag = mir2_shared::hutil32::str_to_int(&info.s_param2, -1) == 1;
                match n_where {
                    0 => self.player.set_bo_send_msg_flag(bo_flag),
                    1 => self.player.set_bo_change_item_name_flag(bo_flag),
                    _ => self.script_action_error("SetscriptFlag", info),
                }
            }
            "ActionOfSetSendMsgFlag" => {
                self.player.set_bo_send_msg_flag(true);
            }
            "ActionOfAutoGetExp" => {
                // C# `SETAUTOGETEXP 时间 点数 是否安全区 地图号`
                let n_time = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
                let n_point = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
                // C# `sParam3[1] == '1'`（空串/单字符会抛异常；此处按 false 处理）
                let bo_safe = info.s_param3.chars().nth(1) == Some('1');
                let s_map = info.s_param4.clone();
                // 原文：只有 sMap 为空时才去 FindMap（非空时 envir 恒为 null）
                let envir = s_map.is_empty() && self.player.map_exists(&s_map);
                if n_time <= 0 || n_point <= 0 || (s_map.is_empty() && !envir) {
                    self.script_action_error("SetautogetExp", info);
                    return;
                }
                let envir_map = if envir { Some(s_map.as_str()) } else { None };
                self.player.set_auto_get_exp(
                    bo_safe,
                    envir_map,
                    n_time.wrapping_mul(1000),
                    n_point,
                );
            }
            "ActionOfOffLine" => {
                let n_time = mir2_shared::hutil32::str_to_int(&info.s_param1, 5);
                let n_point = mir2_shared::hutil32::str_to_int(&info.s_param2, 500);
                let n_kick = mir2_shared::hutil32::str_to_int(&info.s_param3, 1440 * 15);
                let tick = self.player.tick_count();
                let envir = self.player.envir_map_name();
                self.player.set_offline(
                    envir.as_deref(),
                    n_time.wrapping_mul(1000),
                    n_point,
                    tick.wrapping_add(n_kick.wrapping_mul(60_000)),
                );
                self.player
                    .send_def_message(SM_OUTOFCONNECTION, 0, 0, 0, 0, "");
            }
            "ActionOfGmExecute" => {
                // C#：`Self` → 玩家名；`@参数1 …参数6`；临时把 Permission 提到 10 执行一行命令
                let mut s_param2 = info.s_param2.clone();
                if s_param2.eq_ignore_ascii_case("Self") {
                    s_param2 = self.player.chr_name();
                }
                let s_data = format!(
                    "@{} {} {} {} {} {}",
                    info.s_param1,
                    s_param2,
                    info.s_param3,
                    info.s_param4,
                    info.s_param5,
                    info.s_param6
                );
                let old_permission = self.player.permission();
                self.player.set_permission(10);
                self.player.process_user_line_msg(&s_data);
                self.player.set_permission(old_permission);
            }
            "ActionOfRepairAllItem" => {
                let mut has_item = false;
                let len = self.player.use_items_len();
                for i in 0..len {
                    if self.player.use_item_index(i) <= 0 {
                        continue;
                    }
                    let name = self.player.use_item_std_name(i);
                    // C# `!(i != ItemLocation.Charm)` ⇒ 宝石位禁止修理
                    if i == ITEM_LOCATION_CHARM {
                        let msg = format!("{name} 禁止修理...");
                        self.player.sys_msg(&msg, MSG_COLOR_RED, MSG_TYPE_HINT);
                        continue;
                    }
                    let dura_max = self.player.use_item_dura_max(i);
                    self.player.set_use_item_dura(i, dura_max);
                    // C# `SendMsg(normNpc, RM_DURACHANGE, (short)i, Dura, DuraMax, 0)`
                    self.player
                        .send_msg(RM_DURACHANGE, i, dura_max, dura_max, 0, None);
                    has_item = true;
                }
                if has_item {
                    // 原文如此（"身上的的"）
                    self.player.sys_msg(
                        "您身上的的装备修复成功了...",
                        MSG_COLOR_BLUE,
                        MSG_TYPE_HINT,
                    );
                }
            }
            "ActionOfRestReNewLevel" => {
                self.player.set_re_level(0);
                self.player.has_level_up(0);
            }
            "ActionOfReNewLevel" => {
                // C# 先把 sParam1/sParam2 收成 `byte`（截断后恒 >= 0，原文的两个分支不可达）
                let n_re_level = mir2_shared::hutil32::str_to_int(&info.s_param1, -1) as u8;
                let n_level = mir2_shared::hutil32::str_to_int(&info.s_param2, -1) as u8;
                let n_bonus = mir2_shared::hutil32::str_to_int(&info.s_param3, -1);
                if n_bonus < 0 {
                    self.script_action_error("Renewlevel", info);
                    return;
                }
                let cur = self.player.re_level();
                if cur + n_re_level as i32 <= 255 {
                    self.player.set_re_level(cur + n_re_level as i32);
                    // C#：`nLevel > 0` 改等级与 `ReNewLevelClearExp` 清经验两段均已被注释 ⇒ 无行为
                    let _ = n_level;
                    let _ = self.player.config_renewlevel_clear_exp();
                    let bonus = self.player.bonus_point();
                    self.player.set_bonus_point(bonus.wrapping_add(n_bonus));
                    // C# `SendMsg(Messages.RM_ADJUST_BONUS, 0, 0, 0, 0)`（无 baseObject 重载）
                    self.player.send_msg(RM_ADJUST_BONUS, 0, 0, 0, 0, None);
                    self.player.has_level_up(0);
                    self.player.ref_show_name();
                }
            }
            "ActionOfChangeMode" => {
                let n_mode = info.n_param1;
                let bo_open = mir2_shared::hutil32::str_to_int(&info.s_param2, -1) == 1;
                if (1..=3).contains(&n_mode) {
                    match n_mode {
                        1 => {
                            self.player.set_admin_mode(bo_open);
                            let msg = if self.player.admin_mode() {
                                "已进入管理员模式"
                            } else {
                                "已退出管理员模式"
                            };
                            self.player.sys_msg(msg, MSG_COLOR_GREEN, MSG_TYPE_HINT);
                        }
                        2 => {
                            self.player.set_super_man(bo_open);
                            let msg = if self.player.super_man() {
                                "已进入无敌模式"
                            } else {
                                "已退出无敌模式"
                            };
                            self.player.sys_msg(msg, MSG_COLOR_GREEN, MSG_TYPE_HINT);
                        }
                        _ => {
                            self.player.set_ob_mode(bo_open);
                            let msg = if self.player.ob_mode() {
                                "已进入隐身模式"
                            } else {
                                "已退出隐身模式"
                            };
                            self.player.sys_msg(msg, MSG_COLOR_GREEN, MSG_TYPE_HINT);
                        }
                    }
                } else {
                    self.script_action_error("ChangeMode", info);
                }
            }
            "ActionOfQueryValue" => {
                // C# `ActionOfQueryValue`：ValLabel < 100 → 0；ValType > 3 → 0
                let mut val_label = info.n_param1;
                if val_label < 100 {
                    val_label = 0;
                }
                let mut val_type = info.n_param2 as u8;
                if val_type as i32 > 3 {
                    val_type = 0;
                }
                let val_len = info.n_param3.max(1) as u16;
                let npc_type = if info.s_param6.eq_ignore_ascii_case("QF") {
                    1
                } else if info.s_param6.eq_ignore_ascii_case("QM") {
                    2
                } else {
                    0
                };
                self.player
                    .set_val_query(val_label as u8, val_type, &info.s_param4, npc_type);
                let hint = if info.s_param5.is_empty() {
                    "请输入：".to_string()
                } else {
                    info.s_param5.clone()
                };
                let tag = make_word(val_type as u16, val_len) as i32;
                self.player
                    .send_def_message(SM_QUERYVALUE, 0, tag, 0, 0, &hint);
            }
            "ActionOfQueryItemDlg" => {
                let take = info.n_param3 != 0;
                self.player.set_dlg_query(take, &info.s_param2);
                let hint = if info.s_param1.is_empty() {
                    "请输入:".to_string()
                } else {
                    info.s_param1.clone()
                };
                let npc_id = self.npc.actor_id();
                self.player
                    .send_def_message(SM_QUERYITEMDLG, npc_id, 0, 0, 0, &hint);
            }
            "ActionOfUpgradeDlgItem" => {
                // C# 方法体为空（原文如此）
            }
            "ActionOfPlayDice" => {
                // C# `ActionOfPlayDice`：把 MDyVal[0..9] 打包进 3 个 DWORD 一并下发
                self.player.set_play_dice_label(&info.s_param2);
                let v: Vec<u16> = (0..10).map(|i| self.player.mdy_val(i) as u16).collect();
                let p2 = make_long(make_word(v[0], v[1]), make_word(v[2], v[3]));
                let p3 = make_long(make_word(v[4], v[5]), make_word(v[6], v[7]));
                let p4 = make_long(make_word(v[8], v[9]), 0);
                self.player.send_msg(
                    RM_PLAYDICE,
                    info.n_param1 as i16 as i32,
                    p2,
                    p3,
                    p4,
                    Some(&info.s_param2),
                );
            }
            "ActionOfAddSkill" => {
                let n_level = 3.min(mir2_shared::hutil32::str_to_int(&info.s_param2, 0));
                match self.player.find_magic(&info.s_param1) {
                    Some((magic_id, magic_name)) => {
                        if !self.player.is_training_skill(magic_id) {
                            self.player
                                .learn_magic(magic_id, &magic_name, n_level as u8);
                            if self.player.config_show_script_action_msg() {
                                let msg = format!("{magic_name}练习成功。");
                                self.player.sys_msg(&msg, MSG_COLOR_GREEN, MSG_TYPE_HINT);
                            }
                        }
                    }
                    None => self.script_action_error("AddSkill", info),
                }
            }
            // C# `ActionOfAddNameList` / `ActionOfDelNameList` / `ActionOfDelUseDateList` /
            // `ActionOfDelAccountList` 都落到 `ActionOfAddList`/`ActionOfDelList`（两者方法体逐字相同）
            "ActionOfAddNameList" | "ActionOfDelNameList" | "ActionOfDelUseDateList" => {
                let name = self.player.chr_name();
                let rel = format!("{}{}", self.npc.path(), info.s_param1);
                self.del_from_string_list(&name, &rel);
            }
            "ActionOfDelAccountList" => {
                let account = self.player.user_account();
                let rel = format!("{}{}", self.npc.path(), info.s_param1);
                self.del_from_string_list(&account, &rel);
            }
            // ---- B2/M4 var 家族 / 剩余动作批次（2026-10-11）----
            "ActionOfMovData" => self.action_of_mov_data(info),
            "ActionOfMovrData" => self.action_of_movr_data(info),
            "ActionOfIncInteger" => self.action_of_inc_integer(info),
            "ActionOfDecInteger" => self.action_of_dec_integer(info),
            "ActionOfMulData" => self.action_of_mul_data(info),
            "ActionOfVar" => self.action_of_var(info),
            "ActionOfLoadVar" => self.action_of_load_var(info),
            "ActionOfSaveVar" => self.action_of_save_var(info),
            "ActionOfCalcVar" => self.action_of_calc_var(info),
            "ActionOfUpgradeItemsEx" => self.action_of_upgrade_items_ex(info),
            "ActionOfMarry" => self.action_of_marry(info),
            "ActionOfUnMarry" => self.action_of_un_marry(info),
            "ActionOfAddUseDateList" => self.action_of_add_use_date_list(info),
            "ActionOfClearMapMon" => self.action_of_clear_map_mon(info),
            "ActionOfMonGenEx" => self.action_of_mon_gen_ex(info),
            other => {
                self.errors.push(EngineError::NotImplemented {
                    kind: "action",
                    code,
                    handler: if other.is_empty() {
                        "(unknown)"
                    } else {
                        "(static)"
                    },
                });
            }
        }
        let _ = result;
    }

    /// `ScriptEngine.GotoLableQuestActionProcess` 的 switch 分支（未注册动作的落点）。
    /// 返回 false 表示 `break`。
    pub(crate) fn run_switch_action(&mut self, code: i32, info: &QuestActionInfo) -> bool {
        let Some(case) = crate::engine::dispatch::engine_switch_execution(code) else {
            // C# switch 未命中：无行为（静默，1:1）
            return true;
        };
        match case {
            "Break" => {
                // C#: case ExecutionCode.Break: result = false;
                return false;
            }
            "Param1" | "Param2" | "Param3" | "Param4" => {
                // C# 只把参数读进未使用的局部变量
            }
            "Map" | "MapMove" => {
                self.goto_params.send_say_msg = true;
            }
            "PlayDice" => {
                self.goto_params.send_say_msg = true;
            }
            "AddBatch" | "BatchDelay" | "BatchMove" | "CheckUserDate" | "GetDlgItemValue"
            | "TakeDlgItem" => {
                // C# 侧实现为空/已注释
            }
            "GoQuest" => {
                // GoToQuest(playerActor, normNpc, nParam1)
                let n_quest = info.n_param1;
                let idx = self
                    .npc
                    .script_list()
                    .iter()
                    .position(|s| s.quest_count == n_quest);
                if let Some(i) = idx {
                    self.player.set_script_index(Some(i));
                    self.player.set_last_npc(self.npc.actor_id());
                    self.goto_label("@main", false);
                }
            }
            "EndQuest" => {
                self.player.set_script_index(None);
            }
            "Goto" => {
                // JmpToLable：先计数，超限报错（C# `[脚本死循环]`）
                let cnt = self.player.script_goto_count() + 1;
                self.player.set_script_goto_count(cnt);
                if cnt > self.npc.script_goto_count_limit() {
                    self.errors.push(EngineError::GotoCountLimit {
                        npc: self.npc.chr_name(),
                        map: self.npc.map_name(),
                        x: self.npc.curr_x(),
                        y: self.npc.curr_y(),
                        label: info.s_param1.clone(),
                    });
                    return false;
                }
                self.goto_label(&info.s_param1, false);
            }
            "Take" => {
                // C# `GotoLableTakeItem(playerActor, sParam1, nParam2, ItemName)`：
                // 金币名 → DecGold(nParam2)；否则按名从背包移除 nParam2 件。
                if info.s_param1.eq_ignore_ascii_case(GOLD_NAME) {
                    self.player.dec_gold(info.n_param2);
                } else {
                    self.player.remove_item(&info.s_param1, info.n_param2);
                }
            }
            "Takew" => {
                // C# `GotoLableTakeWItem(playerActor, sParam1, nParam2)`：先按前缀摘佩戴位，
                // 未命中再按标准名遍历所有佩戴位摘下至多 nParam2 件
                let name = info.s_param1.clone();
                let done = self.take_worn_by_prefix(&name);
                if !done {
                    let mut left = info.n_param2;
                    let len = self.player.use_items_len();
                    for i in 0..len {
                        if left <= 0 {
                            break;
                        }
                        if self.player.use_item_index(i) > 0 {
                            let std_name = self.player.use_item_std_name(i);
                            if std_name.eq_ignore_ascii_case(&name) {
                                self.player.send_del_use_item(i);
                                self.player.set_use_item_index(i, 0);
                                left -= 1;
                            }
                        }
                    }
                }
            }
            "TakecheckItem" => {
                // `GotoLableTakeCheckItem` —— 批次 2 后续（语料未使用）
                self.errors.push(EngineError::NotImplemented {
                    kind: "action",
                    code,
                    handler: "GotoLableTakeCheckItem",
                });
            }
            _ => {
                // 生成表里出现未覆盖的 case：显式报错而非静默
                self.errors.push(EngineError::NotImplemented {
                    kind: "action-switch",
                    code,
                    handler: case,
                });
            }
        }
        true
    }

    /// 条件侧的 `ScriptEngine` switch（未注册条件的落点）。
    pub(crate) fn run_unregistered_condition(
        &mut self,
        code: i32,
        info: &QuestConditionInfo,
        result: &mut bool,
    ) -> bool {
        let Some(case) = crate::engine::dispatch::engine_switch_condition(code) else {
            return true; // C# switch 未命中：无行为
        };
        match case {
            // CheckGotoLableItemW(playerActor, sParam1, nParam2)：先判佩戴位（前缀比较，
            // 对应 C# `HUtil32.CompareLStr(sItemType, "[NECKLACE]", 4)`），否则查背包件数。
            "CHECKITEMW" => {
                let name = info.s_param1.as_str();
                let worn = worn_location_of(name).map(|loc| self.player.has_worn(loc));
                let found = match worn {
                    // 佩戴位分支在 C# 里直接 return，不再做件数判断
                    Some(v) => v,
                    None => {
                        let cnt = self.player.item_count(name);
                        cnt > 0 && cnt >= info.n_param2
                    }
                };
                if !found {
                    *result = false;
                }
            }
            // `case (int)ConditionCode.CHECKDIEMON`：`GotoLable_QuestCheckCondition_CheckDieMon`
            "CHECKDIEMON" => {
                *result = info.s_param1.is_empty()
                    || self.player.last_hiter_name().as_deref() == Some(info.s_param1.as_str());
            }
            // `case (int)ConditionCode.CHECKKILLPLAYMON`：目标名比较
            "CHECKKILLPLAYMON" => {
                *result = info.s_param1.is_empty()
                    || self.player.target_name().as_deref() == Some(info.s_param1.as_str());
            }
            // `case (int)ConditionCode.CHECKRANDOMNO`：C# 只记一条 TODO 错误，不改变 result
            "CHECKRANDOMNO" => {
                self.errors.push(EngineError::ScriptError {
                    text: "TODO nSC_CHECKRANDOMNO...".to_string(),
                });
            }
            // `case (int)ConditionCode.CHECKDURAEVA`：`Round(nMaxDura / n1C / 1000.0) < nParam2`
            "CHECKDURAEVA" => {
                let (n1c, n_max_dura, _n_dura) = self.player.quest_check_item(&info.s_param1);
                if n1c > 0 {
                    let avg = (n_max_dura / n1c) as f64 / 1000.0;
                    if super::conditions::round_away(avg) < info.n_param2 {
                        *result = false;
                    }
                } else {
                    *result = false;
                }
            }
            // `case (int)ConditionCode.KILLBYHUM`：最后击杀者不是玩家 ⇒ false
            "KILLBYHUM" => {
                if let Some(race) = self.player.last_hiter_race() {
                    if race != crate::engine::ACTOR_RACE_PLAY {
                        *result = false;
                    }
                }
            }
            // `case (int)ConditionCode.KILLBYMON`：最后击杀者是玩家 ⇒ false
            "KILLBYMON" => {
                if let Some(race) = self.player.last_hiter_race() {
                    if race == crate::engine::ACTOR_RACE_PLAY {
                        *result = false;
                    }
                }
            }
            // `case (int)ConditionCode.CHECKINSAFEZONE`
            "CHECKINSAFEZONE" => {
                if !self.player.in_safe_zone() {
                    *result = false;
                }
            }
            // `case (int)ConditionCode.ISTAKEITEM`：与 `GotoLabParams.ItemName` 比较
            "ISTAKEITEM" => {
                if self.goto_params.item_name != info.s_param1 {
                    *result = false;
                }
            }
            // `case (int)ConditionCode.SCHECKDEATHPLAYMON`：`CheckKillMon2` 恒 true
            "SCHECKDEATHPLAYMON" => {}
            other => {
                self.errors.push(EngineError::NotImplemented {
                    kind: "condition-switch",
                    code,
                    handler: other,
                });
            }
        }
        true
    }

    /// `GotoLableTakeWItem` 的前缀分支：命中佩戴位且该位非空 ⇒ `SendDelItems` + `Index = 0` 并返回 true。
    ///
    /// C# 原文用 `compn = 4` 的前缀比较逐条判断；**注意** `"[U_BUJUK]"` / `"[U_BELT]"` / `"[U_BOOTS]"`
    /// 前 4 个字符同为 `"[U_B"`，故任何 `[U_B*` 物品都会先落 Bujuk 分支（原文如此，照搬）。
    fn take_worn_by_prefix(&mut self, item_name: &str) -> bool {
        use item_location::*;
        const SLOTS: [(i32, &str); 12] = [
            (NECKLACE, "[NECKLACE]"),
            (RINGL, "[RING]"),
            (RINGR, "[RING]"),
            (ARMRINGL, "[ARMRING]"),
            (ARMRINGR, "[ARMRING]"),
            (WEAPON, "[WEAPON]"),
            (HELMET, "[HELMET]"),
            (DRESS, "[DRESS]"),
            (BUJUK, "[U_BUJUK]"),
            (BELT, "[U_BELT]"),
            (BOOTS, "[U_BOOTS]"),
            (CHARM, "[U_CHARM]"),
        ];
        for (slot, loc) in SLOTS {
            if mir2_shared::hutil32::compare_lstr_prefix(item_name, loc, 4)
                && self.player.use_item_index(slot) > 0
            {
                // C# 先 `SendDelItems`（取的是摘下前的 `Index`）再置 0
                self.player.send_del_use_item(slot);
                self.player.set_use_item_index(slot, 0);
                return true;
            }
        }
        false
    }

    // ==== B2/M4 var 家族批次（2026-10-11）====

    /// `ActionOfMovData`（`MOV`）。`GetMovDataType` 的 0..7 分支 1:1 移植。
    fn action_of_mov_data(&mut self, info: &QuestActionInfo) {
        let s_param1 = self.arrest_str_param(&info.s_param1);
        let s_param2 = self.arrest_str_param(&info.s_param2);
        let s_param3 = self.arrest_str_param(&info.s_param3);
        if s_param1.is_empty() {
            self.script_action_error("Mov", info);
            return;
        }
        match self.get_mov_data_type(info) {
            0 => match self.get_mov_data_human_info_value(&s_param3) {
                Some(v) => {
                    let (sv, nv, dt) = mov_parts(&v);
                    let resolved = self.dyn_var_resolve(&s_param1);
                    let ok = match resolved {
                        Some((kind, _)) => {
                            self.set_mov_data_dynamic_var_value(kind, &s_param2, &sv, nv, dt)
                        }
                        None => false,
                    };
                    if !ok {
                        let msg = format!("变量{}不存在，变量类型:{}", s_param1, s_param2);
                        self.script_action_error_msg(&msg, "Mov", info);
                    }
                }
                None => self.script_action_error("Mov", info),
            },
            1 => match self.get_mov_data_val_name_value(&s_param3) {
                Some(v) => {
                    let (sv, nv, dt) = mov_parts(&v);
                    let resolved = self.dyn_var_resolve(&s_param1);
                    let ok = match resolved {
                        Some((kind, _)) => {
                            self.set_mov_data_dynamic_var_value(kind, &s_param2, &sv, nv, dt)
                        }
                        None => false,
                    };
                    if !ok {
                        let msg = format!("变量{}不存在，变量类型:{}", s_param1, s_param2);
                        self.script_action_error_msg(&msg, "Mov", info);
                    }
                }
                None => self.script_action_error("Mov", info),
            },
            2 | 3 => {
                let dt = if self.get_mov_data_type(info) == 2 {
                    1
                } else {
                    0
                };
                let resolved = self.dyn_var_resolve(&s_param1);
                let ok = match resolved {
                    Some((kind, _)) => self.set_mov_data_dynamic_var_value(
                        kind,
                        &s_param2,
                        &info.s_param3,
                        info.n_param3,
                        dt,
                    ),
                    None => false,
                };
                if !ok {
                    let msg = format!("变量{}不存在，变量类型:{}", s_param1, s_param2);
                    self.script_action_error_msg(&msg, "Mov", info);
                }
            }
            4 | 5 => {
                let got = if self.get_mov_data_type(info) == 4 {
                    self.get_mov_data_human_info_value(&s_param2)
                } else {
                    self.get_mov_data_val_name_value(&s_param2)
                };
                match got {
                    Some(v) => {
                        let (sv, nv, dt) = mov_parts(&v);
                        if !self.set_val_name_value(&s_param1, &sv, nv, dt) {
                            self.script_action_error("Mov", info);
                        }
                    }
                    None => self.script_action_error("Mov", info),
                }
            }
            6 => {
                let kind = self.dyn_var_resolve(&s_param2).map(|(k, _)| k);
                let got = kind.and_then(|k| self.get_mov_data_dynamic_var_value(k, &s_param3));
                match got {
                    Some(v) => {
                        let (sv, nv, dt) = mov_parts(&v);
                        if !self.set_val_name_value(&s_param1, &sv, nv, dt) {
                            self.script_action_error("Mov", info);
                        }
                    }
                    None => {
                        let msg = format!("变量{}不存在，变量类型:{}", s_param2, s_param3);
                        self.script_action_error_msg(&msg, "Mov", info);
                    }
                }
            }
            7 => match self.get_mov_data_val_name_value(&s_param1) {
                Some(v) => {
                    let (_, _, dt0) = mov_parts(&v);
                    let is_human_info =
                        s_param2.starts_with('<') && s_param2.chars().nth(1) == Some('$');
                    let (sv, dt) = if is_human_info {
                        match self.get_mov_data_human_info_value(&s_param2) {
                            Some(hv) => {
                                let (mut hsv, _hnv, hdt) = mov_parts(&hv);
                                if let Some(i) = s_param2.find('\\') {
                                    let chars: Vec<char> = s_param2.chars().collect();
                                    if i >= 1 {
                                        let suffix: String = chars[i - 1..].iter().collect();
                                        hsv = format!("{}{}", hsv, suffix);
                                    }
                                }
                                (hsv, hdt)
                            }
                            None => {
                                self.script_action_error("Mov", info);
                                return;
                            }
                        }
                    } else {
                        (info.s_param2.clone(), dt0)
                    };
                    let _ = self.set_val_name_value(&s_param1, &sv, info.n_param2, dt);
                }
                None => self.script_action_error("Mov", info),
            },
            _ => self.script_action_error("Mov", info),
        }
    }

    /// `ActionOfMovrData`（`MOVR`）：取随机值赋给编号变量。
    fn action_of_movr_data(&mut self, info: &QuestActionInfo) {
        let n14 = if mir2_shared::hutil32::compare_lstr_prefix(&info.s_param1, "<$STR(", 6) {
            let s34 = match mir2_shared::hutil32::arrest_string_ex_ref(&info.s_param1, '(', ')').0 {
                mir2_shared::hutil32::ArrestWrite::Value(v) => v,
                mir2_shared::hutil32::ArrestWrite::Unchanged => String::new(),
            };
            crate::engine::vars::get_val_name_no(&s34)
        } else {
            crate::engine::vars::get_val_name_no(&info.s_param1)
        };
        if n14 < 0 {
            self.script_action_error("Movr", info);
            return;
        }
        let val = if info.n_param3 > info.n_param2 {
            info.n_param2
                .wrapping_add(self.player.random(info.n_param3 - info.n_param2))
        } else {
            self.player.random(info.n_param2)
        };
        match n14 {
            0..=99 => {
                self.player.set_mn_val(n14 as usize, val);
            }
            100..=119 => {
                self.player.set_global_val((n14 - 100) as usize, val);
            }
            200..=299 => {
                self.player.set_mdy_val((n14 - 200) as usize, val);
            }
            300..=399 => {
                self.player.set_mnmval((n14 - 300) as usize, val);
            }
            400..=499 => {
                self.player.set_globa_dy_mval((n14 - 400) as usize, val);
            }
            500..=599 => {
                self.player.set_mn_integer((n14 - 500) as usize, val);
            }
            800..=1199 => {
                self.player.set_global_val((n14 - 700) as usize, val);
            }
            _ => self.script_action_error("Movr", info),
        }
    }

    /// `ActionOfIncInteger`（`INC`）：编号槽位 / `DynamicVar` 自增。
    fn action_of_inc_integer(&mut self, info: &QuestActionInfo) {
        let s_param1 = self.arrest_str_param(&info.s_param1);
        let s_param2 = self.arrest_str_param(&info.s_param2);
        let s_param3 = self.arrest_str_param(&info.s_param3);
        if s_param1.is_empty() || s_param2.is_empty() {
            self.script_action_error("Inc", info);
            return;
        }
        if !s_param3.is_empty() {
            if !Self::is_var_number(&s_param1) && Self::is_var_number(&s_param2) {
                let Some((kind, _)) = self.dyn_var_resolve(&s_param2) else {
                    let msg = format!(
                        "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                        s_param2
                    );
                    self.script_action_error_msg(&msg, "Inc", info);
                    return;
                };
                let (n3c, s01) = match self.player.dyn_var_get(kind, &s_param3) {
                    Some(DynVar::Integer(x)) => (x, String::new()),
                    Some(DynVar::Str(s)) => (0, s),
                    None => {
                        let msg = format!("变量{}不存在，变量类型:{}", s_param3, s_param2);
                        self.script_action_error_msg(&msg, "Inc", info);
                        return;
                    }
                };
                let n14 = crate::engine::vars::get_val_name_no(&s_param1);
                if n14 < 0 {
                    self.script_action_error("Inc", info);
                    return;
                }
                self.inc_slot_by_no(n14, n3c, &s01, info);
                return;
            }
            if Self::is_var_number(&s_param1) && !Self::is_var_number(&s_param2) {
                // C# `!IsStringNumber(sParam3)` 恒假（IsStringNumber 恒真）⇒ n3c = nParam3、s01 = ""
                let n3c = info.n_param3;
                let s01 = String::new();
                let Some((kind, _)) = self.dyn_var_resolve(&s_param1) else {
                    let msg = format!(
                        "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                        s_param1
                    );
                    self.script_action_error_msg(&msg, "Inc", info);
                    return;
                };
                match self.player.dyn_var_get(kind, &s_param2) {
                    Some(DynVar::Integer(x)) => {
                        let nv = if n3c > 1 {
                            x.wrapping_add(n3c)
                        } else {
                            x.wrapping_add(1)
                        };
                        self.player
                            .dyn_var_set(kind, &s_param2, DynVar::Integer(nv));
                    }
                    Some(DynVar::Str(s)) => {
                        self.player.dyn_var_set(
                            kind,
                            &s_param2,
                            DynVar::Str(format!("{}{}", s, s01)),
                        );
                    }
                    None => {
                        let msg = format!("变量{}不存在，变量类型:{}", s_param2, s_param1);
                        self.script_action_error_msg(&msg, "Inc", info);
                        return;
                    }
                }
                return;
            }
            // n10 == 0 永真
            self.script_action_error("Inc", info);
            return;
        }
        // 无 sParam3：`!IsStringNumber` 恒假 ⇒ n3c = nParam2、s01 = ""
        let n3c = info.n_param2;
        let n14 = crate::engine::vars::get_val_name_no(&s_param1);
        if n14 < 0 {
            self.script_action_error("Inc", info);
            return;
        }
        self.inc_slot_by_no(n14, n3c, "", info);
    }

    /// `INC` 的编号槽位自增（含 600/700/1200 的字符串拼接与越界报错）。
    fn inc_slot_by_no(&mut self, n14: i32, n3c: i32, s01: &str, info: &QuestActionInfo) {
        let inc = |old: i32| {
            if n3c > 1 {
                old.wrapping_add(n3c)
            } else {
                old.wrapping_add(1)
            }
        };
        match n14 {
            0..=99 => {
                let v = inc(self.player.mn_val(n14 as usize));
                self.player.set_mn_val(n14 as usize, v);
            }
            100..=199 => {
                let i = (n14 - 100) as usize;
                let v = inc(self.player.global_val(i));
                self.player.set_global_val(i, v);
            }
            200..=299 => {
                let i = (n14 - 200) as usize;
                let v = inc(self.player.mdy_val(i));
                self.player.set_mdy_val(i, v);
            }
            300..=399 => {
                let i = (n14 - 300) as usize;
                let v = inc(self.player.mnmval(i));
                self.player.set_mnmval(i, v);
            }
            400..=499 => {
                let i = (n14 - 400) as usize;
                let v = inc(self.player.globa_dy_mval(i));
                self.player.set_globa_dy_mval(i, v);
            }
            500..=599 => {
                let i = (n14 - 500) as usize;
                let v = inc(self.player.mn_integer(i));
                self.player.set_mn_integer(i, v);
            }
            600..=699 => {
                let i = (n14 - 600) as usize;
                let old = self.player.ms_string(i);
                self.player.set_ms_string(i, format!("{}{}", old, s01));
            }
            700..=799 => {
                let i = (n14 - 700) as usize;
                let old = self.player.global_a_val(i);
                self.player.set_global_a_val(i, format!("{}{}", old, s01));
            }
            800..=1199 => {
                let i = (n14 - 700) as usize;
                let v = inc(self.player.global_val(i));
                self.player.set_global_val(i, v);
            }
            1200..=1599 => {
                let i = (n14 - 1100) as usize;
                let old = self.player.global_a_val(i);
                self.player.set_global_a_val(i, format!("{}{}", old, s01));
            }
            _ => self.script_action_error("Inc", info),
        }
    }

    /// `ActionOfDecInteger`（`DEC`）：编号槽位 / `DynamicVar` 自减。
    fn action_of_dec_integer(&mut self, info: &QuestActionInfo) {
        let s_param1 = self.arrest_str_param(&info.s_param1);
        let s_param2 = self.arrest_str_param(&info.s_param2);
        let s_param3 = self.arrest_str_param(&info.s_param3);
        if s_param1.is_empty() || s_param2.is_empty() {
            self.script_action_error("Dec", info);
            return;
        }
        if !s_param3.is_empty() {
            if !Self::is_var_number(&s_param1) && Self::is_var_number(&s_param2) {
                let Some((kind, _)) = self.dyn_var_resolve(&s_param2) else {
                    let msg = format!(
                        "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                        s_param2
                    );
                    self.script_action_error_msg(&msg, "Dec", info);
                    return;
                };
                let (n3c, s01) = match self.player.dyn_var_get(kind, &s_param3) {
                    Some(DynVar::Integer(x)) => (x, String::new()),
                    Some(DynVar::Str(s)) => (0, s),
                    None => {
                        let msg = format!("变量{}不存在，变量类型:{}", s_param3, s_param2);
                        self.script_action_error_msg(&msg, "Dec", info);
                        return;
                    }
                };
                let n14 = crate::engine::vars::get_val_name_no(&s_param1);
                if n14 < 0 {
                    self.script_action_error("Dec", info);
                    return;
                }
                self.dec_slot_by_no(n14, n3c, &s01, info);
                return;
            }
            if Self::is_var_number(&s_param1) && !Self::is_var_number(&s_param2) {
                let n3c = info.n_param3;
                let s01 = String::new();
                let Some((kind, _)) = self.dyn_var_resolve(&s_param1) else {
                    let msg = format!(
                        "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                        s_param1
                    );
                    self.script_action_error_msg(&msg, "Dec", info);
                    return;
                };
                match self.player.dyn_var_get(kind, &s_param2) {
                    Some(DynVar::Integer(x)) => {
                        let nv = if n3c > 1 {
                            x.wrapping_sub(n3c)
                        } else {
                            x.wrapping_sub(1)
                        };
                        self.player
                            .dyn_var_set(kind, &s_param2, DynVar::Integer(nv));
                    }
                    Some(DynVar::Str(s)) => {
                        if let Some(ns) = cs_remove_string(&s, &s01, true) {
                            self.player.dyn_var_set(kind, &s_param2, DynVar::Str(ns));
                        }
                    }
                    None => {
                        let msg = format!("变量{}不存在，变量类型:{}", s_param2, s_param1);
                        self.script_action_error_msg(&msg, "Dec", info);
                        return;
                    }
                }
                return;
            }
            self.script_action_error("Dec", info);
            return;
        }
        let n3c = info.n_param2;
        let n14 = crate::engine::vars::get_val_name_no(&s_param1);
        if n14 < 0 {
            self.script_action_error("Dec", info);
            return;
        }
        self.dec_slot_by_no(n14, n3c, "", info);
    }

    /// `DEC` 的编号槽位自减（字符串型走 C# 原文的 `Substring` 移除；越界时跳过）。
    fn dec_slot_by_no(&mut self, n14: i32, n3c: i32, s01: &str, info: &QuestActionInfo) {
        let dec = |old: i32| {
            if n3c > 1 {
                old.wrapping_sub(n3c)
            } else {
                old.wrapping_sub(1)
            }
        };
        match n14 {
            0..=99 => {
                let v = dec(self.player.mn_val(n14 as usize));
                self.player.set_mn_val(n14 as usize, v);
            }
            100..=199 => {
                let i = (n14 - 100) as usize;
                let v = dec(self.player.global_val(i));
                self.player.set_global_val(i, v);
            }
            200..=299 => {
                let i = (n14 - 200) as usize;
                let v = dec(self.player.mdy_val(i));
                self.player.set_mdy_val(i, v);
            }
            300..=399 => {
                let i = (n14 - 300) as usize;
                let v = dec(self.player.mnmval(i));
                self.player.set_mnmval(i, v);
            }
            400..=499 => {
                let i = (n14 - 400) as usize;
                let v = dec(self.player.globa_dy_mval(i));
                self.player.set_globa_dy_mval(i, v);
            }
            500..=599 => {
                let i = (n14 - 500) as usize;
                let v = dec(self.player.mn_integer(i));
                self.player.set_mn_integer(i, v);
            }
            600..=699 => {
                let i = (n14 - 600) as usize;
                let old = self.player.ms_string(i);
                if let Some(ns) = cs_remove_string(&old, s01, true) {
                    self.player.set_ms_string(i, ns);
                }
            }
            700..=799 => {
                let i = (n14 - 700) as usize;
                let old = self.player.global_a_val(i);
                if let Some(ns) = cs_remove_string(&old, s01, true) {
                    self.player.set_global_a_val(i, ns);
                }
            }
            800..=1199 => {
                let i = (n14 - 700) as usize;
                let v = dec(self.player.global_val(i));
                self.player.set_global_val(i, v);
            }
            1200..=1599 => {
                let i = (n14 - 1100) as usize;
                let old = self.player.global_a_val(i);
                if let Some(ns) = cs_remove_string(&old, s01, true) {
                    self.player.set_global_a_val(i, ns);
                }
            }
            _ => self.script_action_error("Dec", info),
        }
    }

    /// `ActionOfMulData`（`MUL`）：`sParam1 = sParam2 * sParam3`。
    fn action_of_mul_data(&mut self, info: &QuestActionInfo) {
        let n18 = self.mul_read(&info.s_param2, true, info);
        let n1c = self.mul_read(&info.s_param3, false, info);
        let n14 = if mir2_shared::hutil32::compare_lstr_prefix(&info.s_param1, "<$STR(", 6) {
            let s34 = match mir2_shared::hutil32::arrest_string_ex_ref(&info.s_param1, '(', ')').0 {
                mir2_shared::hutil32::ArrestWrite::Value(v) => v,
                mir2_shared::hutil32::ArrestWrite::Unchanged => String::new(),
            };
            crate::engine::vars::get_val_name_no(&s34)
        } else {
            crate::engine::vars::get_val_name_no(&info.s_param1)
        };
        let prod = n18.wrapping_mul(n1c);
        match n14 {
            0..=99 => self.player.set_mn_val(n14 as usize, prod),
            100..=199 => self.player.set_global_val((n14 - 100) as usize, prod),
            200..=299 => self.player.set_mdy_val((n14 - 200) as usize, prod),
            300..=399 => self.player.set_mnmval((n14 - 300) as usize, prod),
            400..=499 => self.player.set_globa_dy_mval((n14 - 400) as usize, prod),
            500..=599 => self.player.set_mn_integer((n14 - 500) as usize, prod),
            600..=699 => self
                .player
                .set_ms_string((n14 - 600) as usize, prod.to_string()),
            700..=799 => self
                .player
                .set_global_a_val((n14 - 700) as usize, prod.to_string()),
            800..=1199 => self.player.set_global_val((n14 - 700) as usize, prod),
            1200..=1599 => self
                .player
                .set_global_a_val((n14 - 1100) as usize, prod.to_string()),
            _ => {}
        }
    }

    /// `MUL` 一侧取值：先 `StrToInt(GetLineVariableText(x), -1)`，再按编号槽位取；
    /// `report_missing` 为真时，槽位未命中/越界都记 `ScriptActionError(Mul)`。
    fn mul_read(&mut self, text: &str, report_missing: bool, info: &QuestActionInfo) -> i32 {
        let direct = mir2_shared::hutil32::str_to_int(&self.player.line_variable_text(text), -1);
        if direct >= 0 {
            return direct;
        }
        let n14 = crate::engine::vars::get_val_name_no(text);
        if n14 < 0 {
            if report_missing {
                self.script_action_error("Mul", info);
            }
            return 0;
        }
        let v = match n14 {
            0..=99 => Some(self.player.mn_val(n14 as usize)),
            100..=199 => Some(self.player.global_val((n14 - 100) as usize)),
            200..=299 => Some(self.player.mdy_val((n14 - 200) as usize)),
            300..=399 => Some(self.player.mnmval((n14 - 300) as usize)),
            400..=499 => Some(self.player.globa_dy_mval((n14 - 400) as usize)),
            500..=599 => Some(self.player.mn_integer((n14 - 500) as usize)),
            600..=699 => Some(mir2_shared::hutil32::str_to_int(
                &self.player.ms_string((n14 - 600) as usize),
                1,
            )),
            700..=799 => Some(mir2_shared::hutil32::str_to_int(
                &self.player.global_a_val((n14 - 700) as usize),
                1,
            )),
            800..=1199 => Some(self.player.global_val((n14 - 700) as usize)),
            1200..=1599 => Some(mir2_shared::hutil32::str_to_int(
                &self.player.global_a_val((n14 - 1100) as usize),
                1,
            )),
            _ => None,
        };
        match v {
            Some(v) => v,
            None => {
                self.script_action_error("Mul", info);
                0
            }
        }
    }

    /// `ActionOfVar`（`VAR 数据类型 类型 变量名`）。
    fn action_of_var(&mut self, info: &QuestActionInfo) {
        let s_type = info.s_param2.clone();
        let s_var_name = info.s_param3.clone();
        let s_var_value = info.s_param4.clone();
        let is_int = info.s_param1.eq_ignore_ascii_case("Integer");
        let is_str = info.s_param1.eq_ignore_ascii_case("String");
        if !is_int && !is_str {
            self.script_action_error("Var", info);
            return;
        }
        if s_type.is_empty() || s_var_name.is_empty() {
            self.script_action_error("Var", info);
            return;
        }
        let value = if is_int {
            DynVar::Integer(mir2_shared::hutil32::str_to_int(&info.s_param4, 0))
        } else {
            DynVar::Str(s_var_value)
        };
        let Some((kind, _)) = self.dyn_var_resolve(&s_type) else {
            let msg = format!(
                "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                s_type
            );
            self.script_action_error_msg(&msg, "Var", info);
            return;
        };
        if self.player.dyn_var_get(kind, &s_var_name).is_some() {
            let msg = format!("变量{}已存在，变量类型:{}", s_var_name, s_type);
            self.script_action_error_msg(&msg, "Var", info);
        } else {
            self.player.dyn_var_insert(kind, &s_var_name, value);
        }
    }

    /// `ActionOfLoadVar`（`LOADVAR 类型 变量名 文件名`）。
    fn action_of_load_var(&mut self, info: &QuestActionInfo) {
        let s_type = info.s_param1.clone();
        let s_var_name = info.s_param2.clone();
        let file = trim_var_file(self.player.line_variable_text(&info.s_param3));
        let Some(path) = self.envir_file_path(&file) else {
            self.script_action_error("LoadVar", info);
            return;
        };
        if s_type.is_empty() || s_var_name.is_empty() || !path.is_file() {
            self.script_action_error("LoadVar", info);
            return;
        }
        let Some((kind, s_name)) = self.dyn_var_resolve(&s_type) else {
            let msg = format!(
                "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                s_type
            );
            self.script_action_error_msg(&msg, "Var", info);
            return;
        };
        let mut found = self.player.dyn_var_get(kind, &s_var_name).is_some();
        if !found {
            let str_val = conf_read_string(&path, &s_name, &s_var_name);
            if !str_val.is_empty() {
                self.player
                    .dyn_var_insert(kind, &s_var_name, DynVar::Str(str_val));
                found = true;
            }
        }
        if !found {
            let msg = format!("变量{}不存在，变量类型:{}", s_var_name, s_type);
            self.script_action_error_msg(&msg, "LoadVar", info);
        }
    }

    /// `ActionOfSaveVar`（`SAVEVAR 类型 变量名 文件名`）。
    ///
    /// C# `ConfFile.WriteInteger` 为空实现、`WriteString` 只改内存缓存且**从不 `Save()`**，
    /// 故可观察行为只有：类型/变量名/文件存在性校验 + 变量是否存在校验；不落盘（原文如此）。
    fn action_of_save_var(&mut self, info: &QuestActionInfo) {
        let s_type = info.s_param1.clone();
        let s_var_name = info.s_param2.clone();
        let file = trim_var_file(self.player.line_variable_text(&info.s_param3));
        let Some(path) = self.envir_file_path(&file) else {
            self.script_action_error("SaveVar", info);
            return;
        };
        if s_type.is_empty() || s_var_name.is_empty() || !path.is_file() {
            self.script_action_error("SaveVar", info);
            return;
        }
        let Some((kind, _)) = self.dyn_var_resolve(&s_type) else {
            let msg = format!(
                "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                s_type
            );
            self.script_action_error_msg(&msg, "Var", info);
            return;
        };
        if self.player.dyn_var_get(kind, &s_var_name).is_none() {
            let msg = format!("变量{}不存在，变量类型:{}", s_var_name, s_type);
            self.script_action_error_msg(&msg, "SaveVar", info);
        }
    }

    /// `ActionOfCalcVar`（`CALCVAR 类型 变量名 操作符 值`）。
    /// 注意 C# 缺陷：`IsStringNumber` 恒真 ⇒ `sParam4` 永不展开；且读写**总是 HUMAN 表**。
    fn action_of_calc_var(&mut self, info: &QuestActionInfo) {
        let s_type = info.s_param1.clone();
        let s_var_name = info.s_param2.clone();
        let s_method = info.s_param3.clone();
        let s_var_value = info.s_param4.clone();
        let n_var_value = mir2_shared::hutil32::str_to_int(&info.s_param4, 0);
        if s_type.is_empty() || s_var_name.is_empty() || s_method.is_empty() {
            self.script_action_error("CalcVar", info);
            return;
        }
        let Some((_kind, _)) = self.dyn_var_resolve(&s_type) else {
            let msg = format!(
                "变量类型错误，错误类型:{} 当前支持类型(HUMAN、GUILD、GLOBAL)",
                s_type
            );
            self.script_action_error_msg(&msg, "CalcVar", info);
            return;
        };
        let c_method = first_char(&s_method);
        let mut found = false;
        match self.player.dyn_var_get(DynVarKind::Human, &s_var_name) {
            Some(DynVar::Integer(x)) => {
                let nv = match c_method {
                    '=' => n_var_value,
                    '+' => x.wrapping_add(n_var_value),
                    '-' => x.wrapping_sub(n_var_value),
                    '*' => x.wrapping_mul(n_var_value),
                    '/' => x.wrapping_div(n_var_value),
                    _ => x,
                };
                self.player
                    .dyn_var_set(DynVarKind::Human, &s_var_name, DynVar::Integer(nv));
                found = true;
            }
            Some(DynVar::Str(s)) => {
                let nv = match c_method {
                    '=' => s_var_value.clone(),
                    '+' => format!("{}{}", s, s_var_value),
                    _ => s.clone(),
                };
                self.player
                    .dyn_var_set(DynVarKind::Human, &s_var_name, DynVar::Str(nv));
                found = true;
            }
            None => {}
        }
        if !found {
            let msg = format!("变量{}不存在，变量类型:{}", s_var_name, s_type);
            self.script_action_error_msg(&msg, "CalcVar", info);
        }
    }

    /// `ActionOfAddUseDateList`：把「名字 + 当天日期」追加到名单（去重，忽略大小写）。
    fn action_of_add_use_date_list(&mut self, info: &QuestActionInfo) {
        let hum_name = self.player.chr_name();
        let rel = format!("{}{}", self.npc.path(), info.s_param1);
        let Some(path) = self.envir_file_path(&rel) else {
            return;
        };
        let mut lines: Vec<String> = Vec::new();
        if let Ok(bytes) = std::fs::read(&path) {
            lines = crate::textfile::split_lines(&crate::textfile::decode_bytes(&bytes));
        }
        for line in &lines {
            let (first, _) = mir2_shared::hutil32::get_valid_str_cap(line.trim(), &[' ', '\t'])
                .unwrap_or((String::new(), String::new()));
            if first.eq_ignore_ascii_case(&hum_name) {
                return;
            }
        }
        lines.push(format!("{}    {}", hum_name, self.player.today_string()));
        let mut out = String::new();
        for line in &lines {
            out.push_str(line);
            out.push_str("\r\n");
        }
        let _ = std::fs::write(&path, crate::textfile::encode_gb2312(&out));
    }

    /// `ActionOfClearMapMon`：清指定地图怪物（Master 与 NoClearMonList 由世界层排除）。
    fn action_of_clear_map_mon(&mut self, info: &QuestActionInfo) {
        self.player.clear_map_monsters(&info.s_param1);
    }

    /// `ActionOfMonGenEx`（`MONGENEX 地图 X Y 名 范围 数量`）。
    fn action_of_mon_gen_ex(&mut self, info: &QuestActionInfo) {
        let s_map_name = info.s_param1.clone();
        let n_map_x = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
        let n_map_y = mir2_shared::hutil32::str_to_int(&info.s_param3, -1);
        let s_mon_name = info.s_param4.clone();
        let n_range = info.n_param5;
        let n_count = info.n_param6;
        if s_map_name.is_empty() || n_map_x <= 0 || n_map_y <= 0 || n_range <= 0 || n_count <= 0 {
            self.script_action_error("MonGenex", info);
            return;
        }
        for _ in 0..n_count {
            let n_rand_x = self.player.random(n_range * 2 + 1) + (n_map_x - n_range);
            let n_rand_y = self.player.random(n_range * 2 + 1) + (n_map_y - n_range);
            if !self
                .player
                .regen_monster(&s_map_name, n_rand_x, n_rand_y, &s_mon_name)
            {
                break;
            }
        }
    }

    /// `ActionOfUpgradeItemsEx`（`UPGRADEITEMSEX`）。
    fn action_of_upgrade_items_ex(&mut self, info: &QuestActionInfo) {
        let n_where = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
        let n_val_type = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
        let n_rate = mir2_shared::hutil32::str_to_int(&info.s_param3, -1);
        let n_point_in = mir2_shared::hutil32::str_to_int(&info.s_param4, -1);
        let n_upgrade_item_status = mir2_shared::hutil32::str_to_int(&info.s_param5, -1);
        let upper = self.player.use_items_len();
        if !(0..=14).contains(&n_val_type)
            || n_where < 0
            || n_where > upper
            || n_rate < 0
            || !(0..=255).contains(&n_point_in)
        {
            self.script_action_error("UpgradeItemSex", info);
            return;
        }
        let std_name = self.player.use_item_std_name(n_where);
        if self.player.use_item_index(n_where) <= 0
            || !self.player.use_item_std_item_present(n_where)
        {
            self.player
                .sys_msg("你身上没有戴指定物品!!!", MSG_COLOR_RED, MSG_TYPE_HINT);
            return;
        }
        let n_rate_point = self.player.random(n_rate * 10);
        let n_point = {
            let r = self.player.random(n_point_in);
            if r > 1 {
                r
            } else {
                1
            }
        };
        if !(0..=10).contains(&n_rate_point) {
            match n_upgrade_item_status {
                0 => self
                    .player
                    .sys_msg("装备升级未成功!!!", MSG_COLOR_RED, MSG_TYPE_HINT),
                1 => {
                    self.player.send_del_use_item(n_where);
                    self.player.set_use_item_index(n_where, 0);
                    self.player
                        .sys_msg("装备破碎!!!", MSG_COLOR_RED, MSG_TYPE_HINT);
                }
                2 => {
                    self.player.sys_msg(
                        "装备升级失败，装备属性恢复默认!!!",
                        MSG_COLOR_RED,
                        MSG_TYPE_HINT,
                    );
                    if n_val_type != 14 {
                        self.player.set_use_item_desc(n_where, n_val_type, 0);
                    }
                }
                _ => {}
            }
            return;
        }
        if n_val_type == 14 {
            let mut n_add_point = n_point.wrapping_mul(1000);
            let dura_max = self.player.use_item_dura_max(n_where);
            if dura_max + n_add_point > 65535 {
                n_add_point = 65535 - dura_max;
            }
            self.player
                .set_use_item_dura_max(n_where, dura_max + n_add_point);
        } else {
            let mut n_add_point = n_point;
            let desc = self.player.use_item_desc(n_where, n_val_type);
            if desc + n_add_point > 255 {
                n_add_point = 255 - desc;
            }
            self.player
                .set_use_item_desc(n_where, n_val_type, desc + n_add_point);
        }
        self.player.send_update_item(n_where);
        self.player
            .sys_msg("装备升级成功", MSG_COLOR_GREEN, MSG_TYPE_HINT);
        let descs: Vec<String> = (0..14)
            .map(|i| self.player.use_item_desc(n_where, i).to_string())
            .collect();
        let msg = format!(
            "{}: {}/{}-{}",
            std_name,
            self.player.use_item_dura(n_where),
            self.player.use_item_dura_max(n_where),
            descs.join("/")
        );
        self.player.sys_msg(&msg, MSG_COLOR_BLUE, MSG_TYPE_HINT);
    }

    /// `ActionOfMarry`（`MARRY`）。
    fn action_of_marry(&mut self, info: &QuestActionInfo) {
        if !self.player.dear_name().is_empty() {
            return;
        }
        let pose = self.player.pose();
        let Some(pose) = pose else {
            self.goto_label("@MarryCheckDir", false);
            return;
        };
        let npc_name = self.npc.chr_name();
        let me_name = self.player.chr_name();
        let pose_name = pose.chr_name.clone();
        if info.s_param1.is_empty() {
            if pose.race != crate::engine::ACTOR_RACE_PLAY {
                self.goto_label("@HumanTypeErr", false);
                return;
            }
            if pose.is_mutual {
                if self.player.gender() != pose.gender {
                    self.goto_label("@StartMarry", false);
                    self.player.pose_goto_label("@StartMarry");
                    if (self.player.gender() == crate::engine::GENDER_MAN
                        && pose.gender == crate::engine::GENDER_WOMAN)
                        || (self.player.gender() == crate::engine::GENDER_WOMAN
                            && pose.gender == crate::engine::GENDER_MAN)
                    {
                        let msg = format!(
                            "[{}]: {} 与 {} 的婚礼现在开始...",
                            npc_name, me_name, pose_name
                        );
                        self.player.world_broadcast(&msg);
                        let msg = format!(
                            "[{}]: {} 你愿意娶 {} 小姐为妻，并照顾她一生一世吗？",
                            npc_name, me_name, pose_name
                        );
                        self.player.world_broadcast(&msg);
                    }
                    self.player.set_is_start_marry(true);
                    self.player.set_pose_is_start_marry(true);
                } else {
                    self.player.pose_goto_label("@MarrySexErr");
                    self.goto_label("@MarrySexErr", false);
                }
            } else {
                self.goto_label("@MarryDirErr", false);
                self.player.pose_goto_label("@MarryCheckDir");
            }
            return;
        }
        if info.s_param1.eq_ignore_ascii_case("REQUESTMARRY")
            && self.player.is_start_marry()
            && self.player.pose_is_start_marry()
            && self.player.gender() == crate::engine::GENDER_MAN
            && pose.gender == crate::engine::GENDER_WOMAN
        {
            // C# 对 `{0}` 模板做 `%n/%s/%d` Replace（无命中）⇒ 原文广播
            let msg =
                "[{0}]: 我愿意!!!，{1} 小姐我会尽我一生的时间来照顾您，让您过上快乐美满的日子的。";
            self.player.world_broadcast(msg);
            let msg = "[{0}]: {1} 你愿意嫁给 {2} 先生为妻，并照顾他一生一世吗？";
            self.player.world_broadcast(msg);
            self.goto_label("@WateMarry", false);
            self.player.pose_goto_label("@RevMarry");
        }
        if info.s_param1.eq_ignore_ascii_case("RESPONSEMARRY")
            && self.player.gender() == crate::engine::GENDER_WOMAN
            && pose.gender == crate::engine::GENDER_MAN
        {
            if info.s_param2.eq_ignore_ascii_case("OK") {
                if self.player.is_start_marry() && self.player.pose_is_start_marry() {
                    let msg = format!(
                        "[{}]: 我愿意!!!，{} 先生我愿意让你来照顾我，保护我。",
                        npc_name, pose_name
                    );
                    self.player.world_broadcast(&msg);
                    let msg = format!(
                        "[{}]: 我宣布 {} 先生与 {} 小姐正式成为合法夫妻。",
                        npc_name, me_name, pose_name
                    );
                    self.player.world_broadcast(&msg);
                    self.goto_label("@EndMarry", false);
                    self.player.pose_goto_label("@EndMarry");
                    self.player.set_is_start_marry(false);
                    self.player.set_pose_is_start_marry(false);
                    self.player.set_dear_name(&pose_name);
                    self.player.set_pose_dear_name(&me_name);
                    self.player.ref_show_name();
                    self.player.pose_ref_show_name();
                }
            } else if self.player.is_start_marry() && self.player.pose_is_start_marry() {
                self.goto_label("@EndMarryFail", false);
                self.player.pose_goto_label("@EndMarryFail");
                self.player.set_is_start_marry(false);
                self.player.set_pose_is_start_marry(false);
                let msg = format!(
                    "[{}]: {} 你这个好色之徒，谁会愿意嫁给你呀!!!，癞蛤蟆想吃天鹅肉。",
                    npc_name, me_name
                );
                self.player.world_broadcast(&msg);
                let msg = format!(
                    "[{}]: 真是可惜，二个人这个时候才翻脸，你们培养好感情后再来找我吧!!!",
                    npc_name
                );
                self.player.world_broadcast(&msg);
            }
        }
    }

    /// `ActionOfUnMarry`（`UNMARRY`）。
    fn action_of_un_marry(&mut self, info: &QuestActionInfo) {
        if self.player.dear_name().is_empty() {
            self.goto_label("@ExeMarryFail", false);
            return;
        }
        let pose = self.player.pose();
        if pose.is_none() {
            self.goto_label("@UnMarryCheckDir", false);
        }
        if let Some(pose) = pose.as_ref() {
            if info.s_param1.is_empty() {
                if pose.race != crate::engine::ACTOR_RACE_PLAY {
                    self.goto_label("@UnMarryTypeErr", false);
                    return;
                }
                if pose.is_mutual && self.player.dear_name() == pose.chr_name {
                    self.goto_label("@StartUnMarry", false);
                    self.player.pose_goto_label("@StartUnMarry");
                    return;
                }
            }
        }
        if info.s_param1.eq_ignore_ascii_case("REQUESTUNMARRY") {
            if info.s_param2.is_empty() {
                if pose.is_some() {
                    self.player.set_is_start_un_marry(true);
                    if self.player.is_start_un_marry() && self.player.pose_is_start_un_marry() {
                        let npc_name = self.npc.chr_name();
                        let me_name = self.player.chr_name();
                        let pose_name = pose
                            .as_ref()
                            .map(|p| p.chr_name.clone())
                            .unwrap_or_default();
                        let msg = format!(
                            "[{}]: 我宣布{} 与{}  正式脱离夫妻关系。",
                            npc_name, pose_name, me_name
                        );
                        self.player.world_broadcast(&msg);
                        self.player.set_dear_name("");
                        self.player.set_pose_dear_name("");
                        self.player.inc_marry_count();
                        self.player.inc_pose_marry_count();
                        self.player.set_is_start_un_marry(false);
                        self.player.set_pose_is_start_un_marry(false);
                        self.player.ref_show_name();
                        self.player.pose_ref_show_name();
                        self.goto_label("@UnMarryEnd", false);
                        self.player.pose_goto_label("@UnMarryEnd");
                    } else {
                        self.goto_label("@WateUnMarry", false);
                    }
                }
            } else if info.s_param2.eq_ignore_ascii_case("FORCE") {
                let npc_name = self.npc.chr_name();
                let me_name = self.player.chr_name();
                let dear = self.player.dear_name();
                let msg = format!(
                    "[{}]: 我宣布{} 与{}  已经正式脱离夫妻关系!!!",
                    npc_name, me_name, dear
                );
                self.player.world_broadcast(&msg);
                // C# `poseHuman = GetPlayObject(DearName)`：世界层未接入时按「对方不在线」
                // 走 else 分支（仅写 UnMarry.txt，此处不落盘，原文已注释）
                self.player.set_dear_name("");
                self.player.inc_marry_count();
                self.goto_label("@UnMarryEnd", false);
                self.player.ref_show_name();
            }
        }
    }

    /// C# `ActionOfDelList(value, fileName)`：读表（文件不存在 = 空表）→ 逐行 `Trim()` 后忽略大小写
    /// 比对 → **首个**命中行 `RemoveAt(i)` 并立即整表保存；未命中不写文件。
    ///
    /// 注意：`ActionOfAddList` 的方法体与它**逐字相同**（C# 复制粘贴缺陷），故
    /// `ADDNAMELIST` 与 `DELNAMELIST` 同语义，照搬不修（行为等价优先）。
    ///
    /// 已知边界（登记在 `tests/parity/evidence/B2/actions-batch-1.md`）：写回按
    /// `StringList.SaveToFile` 口径（gb2312/cp936 无 BOM + 每行 CRLF）；带 BOM 的输入文件
    /// 与 .NET best-fit 可编码性差异不保证逐字节一致。
    fn del_from_string_list(&mut self, value: &str, rel_path: &str) {
        let Some(path) = self.envir_file_path(rel_path) else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let lines = crate::textfile::split_lines(&crate::textfile::decode_bytes(&bytes));
        let Some(idx) = lines
            .iter()
            .position(|l| l.trim().eq_ignore_ascii_case(value))
        else {
            return;
        };
        let mut out = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i == idx {
                continue;
            }
            out.push_str(line);
            out.push_str("\r\n");
        }
        let _ = std::fs::write(&path, crate::textfile::encode_gb2312(&out));
    }

    /// `SystemShare.GetEnvirFilePath(filePath)`（单参口径）：`Path.Combine(BasePath, EnvirDir, filePath)`；
    /// `filePath` 以 `..` 开头时丢弃前 3 个字符（`"../x"` / `"..\x"` ⇒ `x`）。
    /// `None` ⇒ host 未注入 Envir 根（文件类动作按"文件不存在"处理，不写盘）。
    pub(crate) fn envir_file_path(&self, rel: &str) -> Option<std::path::PathBuf> {
        let root = self.npc.envir_root()?;
        let rel = if rel.starts_with("..") {
            rel.get(3..).unwrap_or("")
        } else {
            rel
        };
        let mut p = std::path::PathBuf::from(root);
        if !rel.is_empty() {
            p.push(rel);
        }
        Some(p)
    }
}

/// 物品类型前缀 → 佩戴位名（C# 只比较前 4 个字符）。
fn worn_location_of(item_type: &str) -> Option<&'static str> {
    const LOCATIONS: [&str; 9] = [
        "[NECKLACE]",
        "[RING]",
        "[ARMRING]",
        "[WEAPON]",
        "[HELMET]",
        "[BUJUK]",
        "[BELT]",
        "[BOOTS]",
        "[CHARM]",
    ];
    LOCATIONS
        .into_iter()
        .find(|loc| mir2_shared::hutil32::compare_lstr_prefix(item_type, loc, 4))
}

/// `VarVal` → `(sValue, nValue, nDataType)`（对应 C# `GetMovDataH...` ref 三元组）。
fn mov_parts(v: &VarVal) -> (String, i32, i32) {
    match v {
        VarVal::Int(n) => (String::new(), *n, 1),
        VarVal::Str(s) => (s.clone(), -1, 0),
    }
}

/// `LOADVAR`/`SAVEVAR` 的文件名前导 `\` 剔除（C# 顺序执行的三次判断）。
fn trim_var_file(mut s: String) -> String {
    if s.starts_with('\\') {
        s = s.chars().skip(1).collect();
    }
    if s.chars().nth(1) == Some('\\') {
        s = s.chars().skip(2).collect();
    }
    if s.chars().nth(2) == Some('\\') {
        s = s.chars().skip(3).collect();
    }
    s
}

/// C# `String.IndexOf(sub, OrdinalIgnoreCase)`（`-1` = 未命中）。
fn find_ci(hay: &[char], needle: &[char]) -> i32 {
    if needle.is_empty() {
        return 0;
    }
    if needle.len() > hay.len() {
        return -1;
    }
    for i in 0..=(hay.len() - needle.len()) {
        if (0..needle.len()).all(|j| {
            mir2_shared::hutil32::to_upper_char(hay[i + j])
                == mir2_shared::hutil32::to_upper_char(needle[j])
        }) {
            return i as i32;
        }
    }
    -1
}

/// `DEC` 的 C# 字符串移除（两种下标口径）。
///
/// `variant_a` = 编号槽位分支（`Processings:1710-1713`）：`s02 = s[1..n10]`、
/// `s03 = s.Substring(sub.len + n10, s.len)`；否则为动态变量分支（`Processings:1815-1817`）：
/// `s02 = s[..n10-1]`、`s03 = s.Substring(sub.len + n10 - 1, s.len)`。
/// C# 下标越界会抛异常（语料保证 s01 存在），此处越界返回 `None`（不改写）。
fn cs_remove_string(s: &str, sub: &str, variant_a: bool) -> Option<String> {
    let cs: Vec<char> = s.chars().collect();
    let sub_cs: Vec<char> = sub.chars().collect();
    let n10 = find_ci(&cs, &sub_cs) as i64;
    if variant_a {
        if n10 < 1 {
            return None;
        }
        let start3 = sub_cs.len() as i64 + n10;
        if start3 > cs.len() as i64 {
            return None;
        }
        let s02: String = cs[1..n10 as usize].iter().collect();
        let s03: String = cs[start3 as usize..].iter().collect();
        Some(format!("{}{}", s02, s03))
    } else {
        let end2 = n10 - 1;
        if end2 < 0 {
            return None;
        }
        let start3 = sub_cs.len() as i64 + n10 - 1;
        if start3 > cs.len() as i64 || end2 as usize > cs.len() {
            return None;
        }
        let s02: String = cs[..end2 as usize].iter().collect();
        let s03: String = cs[start3 as usize..].iter().collect();
        Some(format!("{}{}", s02, s03))
    }
}

/// C# `ConfFile.Load()` + `ReadString(section, key, "")` 的读侧（不含写盘）。
///
/// 解析规则逐条对应 `src/OpenMir2/Common/ConfFile.cs:199-284`：逐行 `Trim`、空行跳过、
/// `/* */` 块注释、`;;` 行注释与行内截断、`[section]`（重复段直接停止加载）、键重复取首个、
/// 键/段比较大小写不敏感。文件不存在时 `Load()` 会创建空文件并返回 false，读侧给 ""。
fn conf_read_string(path: &std::path::Path, section: &str, key: &str) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let text = crate::textfile::decode_bytes(&bytes);
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut large_comment = false;
    let mut is_cur_sec_comment = false;
    let mut cur: Option<usize> = None;
    for raw in crate::textfile::split_lines(&text) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("/*") {
            large_comment = true;
            continue;
        }
        if large_comment {
            if line.starts_with("*/") {
                large_comment = false;
            }
            continue;
        }
        let cs: Vec<char> = line.chars().collect();
        if cs.len() >= 2 && cs[0] == ';' && cs[1] == ';' {
            continue;
        }
        if let Some(sec) = conf_section_name(line) {
            let sec_cs: Vec<char> = sec.chars().collect();
            if sec_cs.len() >= 2 && sec_cs[0] == ';' && sec_cs[1] == ';' {
                is_cur_sec_comment = true;
                continue;
            }
            is_cur_sec_comment = false;
            let lower = sec.to_lowercase();
            if sections.iter().any(|(s, _)| *s == lower) {
                // C# 段重复 → `goto Label_02AE` 结束加载
                break;
            }
            sections.push((lower, Vec::new()));
            cur = Some(sections.len() - 1);
            continue;
        }
        if is_cur_sec_comment {
            continue;
        }
        let line = match line.find(";;") {
            Some(i) => &line[..i],
            None => line,
        };
        let idx = match cur {
            Some(i) => i,
            None => {
                if sections.iter().any(|(s, _)| s.is_empty()) {
                    sections.iter().position(|(s, _)| s.is_empty()).unwrap()
                } else {
                    sections.push((String::new(), Vec::new()));
                    sections.len() - 1
                }
            }
        };
        let line = line.trim();
        if let Some((k, v)) = conf_split_key_val(line) {
            let sec = &mut sections[idx].1;
            if !sec.iter().any(|(ek, _)| *ek == k) {
                sec.push((k, v));
            }
        }
    }
    let lower_sec = section.to_lowercase();
    let lower_key = key.to_lowercase();
    sections
        .iter()
        .find(|(s, _)| *s == lower_sec)
        .and_then(|(_, kvs)| {
            kvs.iter()
                .find(|(k, _)| *k == lower_key)
                .map(|(_, v)| v.clone())
        })
        .unwrap_or_default()
}

/// `ConfFile.GetSecString`：`[xxx]` 取段名（`]` 前内容 trim）；非 `[` 开头或无 `]` 返回 `None`。
fn conf_section_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('[')?;
    let end = rest.find(']')?;
    if end == 0 {
        return None;
    }
    Some(rest[..end].trim())
}

/// `ConfFile.SplitKeyVal`：第一个 `=` 切分，键值各自 trim（键转小写）；无 `=` 返回 `None`。
fn conf_split_key_val(line: &str) -> Option<(String, String)> {
    let pos = line.find('=')?;
    if pos == 0 {
        return None;
    }
    Some((
        line[..pos].trim().to_lowercase(),
        line[pos + 1..].trim().to_string(),
    ))
}
