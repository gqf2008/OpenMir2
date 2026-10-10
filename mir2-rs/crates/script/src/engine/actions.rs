//! 动作处理器（`ExecutionProcessingSys`）与 `ScriptEngine` 的动作 switch（1:1 移植）。
//!
//! 注意 C# 语义：注册表以**枚举值**为键，解析器产出的 CmdCode 对普通命令是枚举值-1，
//! 因此脚本写的命令常落到「前一个」成员的实现上（whitelist B-8）。本模块按 CmdCode 直查，
//! 保持同样的落点。

use crate::engine::{Engine, EngineError};
use crate::model::{QuestActionInfo, QuestConditionInfo};

/// C# `Messages` 常量（`src/OpenMir2/Messages.cs`）。
pub const RM_MERCHANTDLGCLOSE: i32 = 10127;
pub const RM_MENU_OK: i32 = 10309;

impl Engine<'_, '_> {
    /// 已注册动作的派发。
    pub(crate) fn run_action(&mut self, code: i32, info: &QuestActionInfo, result: &mut bool) {
        let handler = crate::engine::dispatch::execution_handler(code).unwrap_or("");
        match handler {
            "ActionOfClose" => {
                // playerActor.SendMsg(normNpc, RM_MERCHANTDLGCLOSE, 0, normNpc.ActorId, 0, 0)
                let npc_id = self.npc.actor_id();
                self.player
                    .send_msg(RM_MERCHANTDLGCLOSE, npc_id, 0, 0, None);
            }
            "ActionOfMessageBox" => {
                // playerActor.SendMsg(normNpc, RM_MENU_OK, 0, playerActor.ActorId, 0, 0,
                //                     GetLineVariableText(playerActor, sParam1))
                let actor_id = self.player.actor_id();
                let text = self.player.line_variable_text(&info.s_param1);
                self.player
                    .send_msg(RM_MENU_OK, actor_id, 0, 0, Some(&text));
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
            "ActionOfSet" => {
                // TODO(批次 2 后续)：Set 变量写入（SetMovDataValNameValue 系）。
                self.errors.push(EngineError::NotImplemented {
                    kind: "action",
                    code,
                    handler: "ActionOfSet",
                });
            }
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
            "Take" | "Takew" | "TakecheckItem" => {
                // 物品移除路径（GotoLableTakeItem / GotoLableTakeWItem）——批次 2 后续
                self.errors.push(EngineError::NotImplemented {
                    kind: "action",
                    code,
                    handler: match case {
                        "Take" => "GotoLableTakeItem",
                        "Takew" => "GotoLableTakeWItem",
                        _ => "GotoLableTakeCheckItem",
                    },
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
            // 其余分支依赖世界状态（最后击杀者/目标/安全区/背包细节），批次 2 后续移植
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
