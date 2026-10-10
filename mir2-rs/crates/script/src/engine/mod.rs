//! 脚本求值引擎：控制流 + 派发（对应 `src/Modules/ScriptEngine/ScriptEngine.cs`）。
//!
//! 设计要点：
//! - 游戏世界通过 [`ScriptPlayer`] / [`ScriptNpc`] 两个 trait 注入，引擎自身不依赖具体实现；
//! - 派发键沿用 C# 语义：解析器产出的 `CmdCode`（普通命令 = 枚举值-1，见 whitelist B-8），
//!   注册表以枚举值为键 ⇒ 引擎按 CmdCode 直接查表；
//! - **未实现的处理器必须显式报错**（验收③）：[`EngineError::NotImplemented`] 计入
//!   [`Engine::errors`]，绝不静默跳过；
//! - C# 语义缺陷照搬（如 `goto` 落到 `EndQuest` 分支、`take` 落到 `Set` 处理器）。

use crate::model::{QuestActionInfo, QuestConditionInfo, ScriptInfo};

/// 玩家侧（脚本可见的最小面；随处理器移植逐步扩充）。
pub trait ScriptPlayer {
    /// `playerActor.LastNpc`
    fn last_npc(&self) -> i32;
    fn set_last_npc(&mut self, actor_id: i32);
    /// `playerActor.Script`（用脚本下标标识，`None` 表示 null）
    fn script_index(&self) -> Option<usize>;
    fn set_script_index(&mut self, idx: Option<usize>);
    /// `playerActor.ScriptGotoCount`
    fn script_goto_count(&self) -> i32;
    fn set_script_goto_count(&mut self, v: i32);
    /// `playerActor.GetQuestFalgStatus(flag)`
    fn quest_flag_status(&self, flag: i16) -> u8;
    /// `playerActor.Gold`
    fn gold(&self) -> i32;
    fn actor_id(&self) -> i32;
    /// `playerActor.Abil.Level`
    fn level(&self) -> i32;
    /// `RandomNumber.Random(value)`（`value <= 0` 时 C# 走 `Next()`）
    fn random(&mut self, value: i32) -> i32;
    /// `playerActor.SendMsg(...)`（ident 取 `Messages` 常量）
    fn send_msg(&mut self, ident: i32, param: i32, tag: i32, series: i32, msg: Option<&str>);
    /// `playerActor.MNVal[n]`（0..99）
    fn mn_val(&self, n: usize) -> i32;
    fn set_mn_val(&mut self, n: usize, v: i32);
    /// `playerActor.MDyVal[n]`
    fn mdy_val(&self, n: usize) -> i32;
    /// `playerActor.MNMval[n]`
    fn mnmval(&self, n: usize) -> i32;
    /// `playerActor.MNInteger[n]`
    fn mn_integer(&self, n: usize) -> i32;
    /// `playerActor.MSString[n]`
    fn ms_string(&self, n: usize) -> String;
    /// `playerActor.SetQuestUnitStatus(index, value)`
    fn set_quest_unit_status(&mut self, index: i32, value: i32);
    /// `playerActor.SetQuestFlagStatus(flag, value)`（位语义在实现侧，日志记原始实参）
    fn set_quest_flag_status(&mut self, flag: i32, value: i32);
    /// `playerActor.SetQuestUnitOpenStatus(index, value)`
    fn set_quest_unit_open_status(&mut self, index: i32, value: i32);
    /// 背包中该名字的物品件数（`CheckItemCount` 的 `ref nCount`）
    fn item_count(&self, name: &str) -> i32;
    /// `playerActor.SlaveList.Count`（宝宝/随从数量）
    fn slave_count(&self) -> i32;
    /// `playerActor.ItemList.Count`（背包件数上限判定用）
    fn bag_count(&self) -> i32;
    /// 是否佩戴着该位置的装备（`UseItems[...].Index > 0`）；位置名同 C#（"NECKLACE" 等）
    fn has_worn(&self, location: &str) -> bool;
    /// 把物品交给玩家（`SendAddItem`）
    fn give_item(&mut self, name: &str, count: i32);
    /// 从背包移除物品（`SendDelItems`）；返回实际移除件数
    fn remove_item(&mut self, name: &str, count: i32) -> i32;
    /// `normNpc.GetLineVariableText(playerActor, sMsg)` 的玩家侧部分；缺省实现原样返回
    fn line_variable_text(&mut self, msg: &str) -> String {
        msg.to_string()
    }
}

/// NPC 侧。
pub trait ScriptNpc {
    fn actor_id(&self) -> i32;
    fn chr_name(&self) -> String;
    fn map_name(&self) -> String;
    fn curr_x(&self) -> i32;
    fn curr_y(&self) -> i32;
    fn script_list(&self) -> &Vec<ScriptInfo>;
    /// `GotoLableSendMerChantSayMsg`：把说词发给玩家（`priority=true` 为优先消息）
    fn send_say(&mut self, msg: &str, priority: bool);
    /// `ScriptGotoCountLimit`（`SystemShare.Config.ScriptGotoCountLimit`）
    fn script_goto_count_limit(&self) -> i32;
}

/// 运行时错误/显式报告项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// 未实现的处理器（验收③：必须显式报错并进入清单，不许静默跳过）
    NotImplemented {
        kind: &'static str,
        code: i32,
        handler: &'static str,
    },
    /// C# `[脚本死循环]`：goto 次数超限
    GotoCountLimit {
        npc: String,
        map: String,
        x: i32,
        y: i32,
        label: String,
    },
    /// 脚本变量层尚未覆盖的取值（`<$VAR>` 等）
    UnsupportedVariable { text: String },
    /// C# `LogService.Error` 的脚本错误文本，两侧需逐字一致
    ScriptError { text: String },
}

/// `GotoLabParams`（`ScriptEngine.cs` 同名结构）。
#[derive(Debug, Default, Clone)]
pub struct GotoLabParams {
    pub item_name: String,
    pub send_say_msg: bool,
}

/// 求值上下文。
pub struct Engine<'n, 'p> {
    pub npc: &'n mut dyn ScriptNpc,
    pub player: &'p mut dyn ScriptPlayer,
    pub goto_params: GotoLabParams,
    pub errors: Vec<EngineError>,
    /// 当前脚本下标（进入 GotoLable 后可能被改写）
    pub current_script: Option<usize>,
}

impl<'n, 'p> Engine<'n, 'p> {
    pub fn new(npc: &'n mut dyn ScriptNpc, player: &'p mut dyn ScriptPlayer) -> Self {
        Self {
            npc,
            player,
            goto_params: GotoLabParams::default(),
            errors: Vec::new(),
            current_script: None,
        }
    }

    /// `ScriptEngine.GotoLable`（1:1 移植，含 C# 的选脚本顺序与 break 语义）。
    pub fn goto_label(&mut self, label: &str, bo_ext_jmp: bool) {
        if self.player.last_npc() != self.npc.actor_id() {
            self.player.set_last_npc(0);
        }
        let mut script: Option<usize> = None;
        let count = self.npc.script_list().len();
        if label.eq_ignore_ascii_case("@main") {
            for i in 0..count {
                if self.npc.script_list()[i].contains_label(label) {
                    script = Some(i);
                    self.player.set_script_index(Some(i));
                    self.player.set_last_npc(self.npc.actor_id());
                    break;
                }
            }
        }
        if script.is_none() {
            if let Some(cur) = self.player.script_index() {
                // 从后往前找同一个脚本（C# 保留最后一个匹配，实际等价于 cur）
                for i in (0..count).rev() {
                    if i == cur {
                        script = Some(i);
                    }
                }
            }
            if script.is_none() {
                for i in (0..count).rev() {
                    if Self::check_quest_status(self.player, &self.npc.script_list()[i]) {
                        script = Some(i);
                        self.player.set_script_index(Some(i));
                        self.player.set_last_npc(self.npc.actor_id());
                    }
                }
            }
        }
        let Some(si) = script else { return };
        self.current_script = Some(si);
        let record = self.npc.script_list()[si].find_record(label).cloned();
        let Some(record) = record else { return };
        if bo_ext_jmp && !record.bo_ext_jmp {
            return;
        }
        let mut s_send_msg = String::new();
        self.goto_params = GotoLabParams::default();
        let proc_count = record.procedure_list.len();
        for i in 0..proc_count {
            let proc_ = record.procedure_list[i].clone();
            let ok = self.check_conditions(&proc_.condition_list);
            if ok {
                s_send_msg.push_str(&proc_.s_say_msg);
                if !self.run_actions(&proc_.action_list) {
                    break;
                }
                if self.goto_params.send_say_msg {
                    let msg = self.player.line_variable_text(&s_send_msg);
                    self.npc.send_say(&msg, true);
                }
            } else {
                s_send_msg.push_str(&proc_.s_else_say_msg);
                if !self.run_actions(&proc_.else_action_list) {
                    break;
                }
                if self.goto_params.send_say_msg {
                    let msg = self.player.line_variable_text(&s_send_msg);
                    self.npc.send_say(&msg, true);
                }
            }
        }
        if !s_send_msg.is_empty() {
            let msg = self.player.line_variable_text(&s_send_msg);
            self.npc.send_say(&msg, false);
        }
    }

    /// `CheckGotoLableQuestStatus`。
    fn check_quest_status(player: &dyn ScriptPlayer, script: &ScriptInfo) -> bool {
        if !script.is_quest {
            return true;
        }
        for q in script.quest_info.iter().take(10) {
            // C# 里 nRandRage 走 RandomNumber.Random(n)，此处随机源由调用方注入；
            // 语料中该分支不可达（scriptType==1 死分支，见 whitelist B-13），保守按 0 处理。
            if q.n_rand_rage > 0 {
                return false;
            }
            if player.quest_flag_status(q.w_flag) != q.bt_value {
                return false;
            }
        }
        true
    }

    /// C# `ScriptActionError`：文本逐字符对齐（"[脚本错误] " 后为两空格）。
    pub(crate) fn script_action_error(&mut self, cmd_member: &str, info: &QuestActionInfo) {
        let msg = format!(
            "[脚本错误]  脚本命令:{} NPC名称:{} 地图:{}({}:{}) 参数1:{} 参数2:{} 参数3:{} 参数4:{} 参数5:{} 参数6:{}",
            cmd_member,
            self.npc.chr_name(),
            self.npc.map_name(),
            self.npc.curr_x(),
            self.npc.curr_y(),
            info.s_param1,
            info.s_param2,
            info.s_param3,
            info.s_param4,
            info.s_param5,
            info.s_param6
        );
        self.errors.push(EngineError::ScriptError { text: msg });
    }

    /// `GotoLableQuestCheckCondition`：条件全真才为真。
    pub fn check_conditions(&mut self, list: &[QuestConditionInfo]) -> bool {
        let mut result = true;
        for info in list {
            let code = info.cmd_code;
            if let Some(implemented) = crate::engine::dispatch::condition_status(code) {
                if implemented {
                    // C# `ConditionScript.Execute(...)` 后**立即 return**（只评第一条注册条件）
                    let mut ok = result;
                    self.run_condition(code, info, &mut ok);
                    return ok;
                }
                // C# 有注册但 Rust 未实现：显式报错（验收③），不静默跳过
                self.errors.push(EngineError::NotImplemented {
                    kind: "condition",
                    code,
                    handler: crate::engine::dispatch::condition_handler(code)
                        .unwrap_or("(unregistered)"),
                });
                return result;
            }
            // 未注册：C# 落 ScriptEngine switch 的固定几条，其余静默无行为（1:1）
            if !self.run_unregistered_condition(code, info, &mut result) {
                break;
            }
        }
        result
    }

    /// `GotoLableQuestActionProcess`：返回 false 表示 break（终止本过程后续动作）。
    pub fn run_actions(&mut self, list: &[QuestActionInfo]) -> bool {
        let result = true;
        for info in list {
            let code = info.n_cmd_code;
            if crate::engine::dispatch::execution_implemented(code) {
                // C# `GotoLableQuestActionProcess`：`if (IsRegister) { Execute(...); return result; }`
                // —— 执行**第一个已注册动作**后立即返回（后续动作不再执行）
                let mut ok = result;
                self.run_action(code, info, &mut ok);
                return ok;
            }
            if crate::engine::dispatch::execution_registered(code) {
                // C# 会执行该处理器；Rust 尚未实现 → 显式报错，并按 C# 的"执行后返回"语义停止
                self.errors.push(EngineError::NotImplemented {
                    kind: "action",
                    code,
                    handler: crate::engine::dispatch::execution_handler(code)
                        .unwrap_or("(unregistered)"),
                });
                return result;
            }
            // 未注册 → C# 的 ScriptEngine switch（含位移语义：case 常量是枚举值）
            if !self.run_switch_action(code, info) {
                return false;
            }
        }
        result
    }
}

pub mod actions;
pub mod conditions;
pub mod dispatch;
pub mod vars;

pub use vars::VarLookup;
