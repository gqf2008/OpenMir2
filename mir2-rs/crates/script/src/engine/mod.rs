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

/// `Grobal2.MaxBagItem`（`src/OpenMir2/Grobal2.cs`）。
pub const MAX_BAG_ITEM: i32 = 46;

/// `ActorRace.Play`（`src/OpenMir2/Enums/Race.cs`）。
pub const ACTOR_RACE_PLAY: i32 = 0;

/// `PlayerGender`（`src/OpenMir2/Enums/PlayerGender.cs`）。
pub const GENDER_MAN: i32 = 0;
pub const GENDER_WOMAN: i32 = 1;

/// `baseObject.GetPoseCreate()` 的快照（面向/重叠判定用）。
///
/// C# 里 `GetPoseCreate()` 返回 `IActor`，各处理器按需读 `Race / Gender / Abil.Level /
/// DearName / MasterName / IsMaster`，以及"对方是否也面向我"（`poseHuman.GetPoseCreate() == self`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PoseInfo {
    /// `IActor.Race`（`ActorRace.Play = 0`）
    pub race: i32,
    /// `IPlayerActor.Gender`（`PlayerGender.Man = 0 / WoMan = 1`）
    pub gender: i32,
    /// `IActor.Abil.Level`
    pub level: i32,
    /// `poseHuman.GetPoseCreate() == baseObject`（互相面对）
    pub is_mutual: bool,
    /// `IPlayerActor.DearName`
    pub dear_name: String,
    /// `IPlayerActor.MasterName`
    pub master_name: String,
    /// `IPlayerActor.IsMaster`
    pub is_master: bool,
}

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
    /// `IPlayerActor.DecGold`（`GotoLableTakeItem` 的金币分支）。
    fn dec_gold(&mut self, n: i32);
    /// `IPlayerActor.IncGold`（`GotoLableGiveItem` 的金币分支）。
    fn inc_gold(&mut self, n: i32);
    /// `IPlayerActor.SpaceMove`（`ActionOfMapMove`）。
    /// 默认空实现：flow 夹具的 C# 侧用 DispatchProxy，`SpaceMove`/`SendRefMsg` 不落日志，
    /// 故默认不产生可观察副作用（与两侧对拍口径一致）；真世界适配层可覆写。
    fn space_move(&mut self, _map: &str, _x: i16, _y: i16) {}
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

    // ---- B2/M4 条件批次新增（`ConditionProcessingSys`）----
    // 说明：世界服务尚未接入的取值一律以「世界缺省」返回（0 / 空 / false / None），
    // 与 C# flow 夹具（DispatchProxy 回默认值）一致；真世界适配层按 M2/M4 接上后覆写。

    /// `playerActor.ChrName`
    fn chr_name(&self) -> String {
        String::new()
    }
    /// `playerActor.IsEnoughBag()`：`ItemList.Count < Grobal2.MaxBagItem(46)`
    fn is_enough_bag(&self) -> bool {
        self.bag_count() < MAX_BAG_ITEM
    }
    /// `playerActor.Permission`
    fn permission(&self) -> i32 {
        0
    }
    /// `playerActor.Gender`（`PlayerGender`）
    fn gender(&self) -> i32 {
        GENDER_MAN
    }
    /// `playerActor.Job`（`PlayerJob`：Warrior=0 / Wizard=1 / Taoist=2）
    fn job(&self) -> i32 {
        0
    }
    /// `playerActor.GameGold`
    fn game_gold(&self) -> i32 {
        0
    }
    /// `playerActor.GamePoint`
    fn game_point(&self) -> i32 {
        0
    }
    /// `playerActor.CreditPoint`
    fn credit_point(&self) -> i32 {
        0
    }
    /// `playerActor.ReLevel`
    fn re_level(&self) -> i32 {
        0
    }
    /// `playerActor.MemberLevel`
    fn member_level(&self) -> i32 {
        0
    }
    /// `playerActor.BonusAbil` 九项之和 + `playerActor.BonusPoint`
    fn bonus_total_point(&self) -> i32 {
        0
    }
    /// `playerActor.DearName`
    fn dear_name(&self) -> String {
        String::new()
    }
    /// `playerActor.MasterName`
    fn master_name(&self) -> String {
        String::new()
    }
    /// `playerActor.IsMaster`
    fn is_master(&self) -> bool {
        false
    }
    /// `playerActor.MyGuild?.GuildName`
    fn guild_name(&self) -> Option<String> {
        None
    }
    /// `playerActor.MyGuild != null && playerActor.IsGuildMaster()`
    fn is_guild_master(&self) -> bool {
        false
    }
    /// `playerActor.IsNewHuman`
    fn is_new_human(&self) -> bool {
        false
    }
    /// `playerActor.MapName`
    fn map_name(&self) -> String {
        String::new()
    }
    /// `playerActor.UserAccount`
    fn user_account(&self) -> String {
        String::new()
    }
    /// `playerActor.CurrX / CurrY`
    fn curr_pos(&self) -> (i32, i32) {
        (0, 0)
    }
    /// `playerActor.UseItems.GetUpperBound(0)`（佩戴位最大下标；-1 = 无佩戴位表）
    fn use_items_upper_bound(&self) -> i32 {
        -1
    }
    /// `playerActor.UseItems[i].Index`（0 = 空位）
    fn use_item_index(&self, _where_: i32) -> i32 {
        0
    }
    /// `playerActor.UseItems[i].Desc` 各项之和（`None` = 空位 `Index == 0`）
    fn worn_item_add_value(&self, _where_: i32) -> Option<i32> {
        None
    }
    /// `playerActor.QuestCheckItem(name, ref n1C, ref nMaxDura, ref nDura)`
    /// 返回 `(n1C, nMaxDura, nDura)`（件数 / 最大持久 / 当前持久）
    fn quest_check_item(&self, _name: &str) -> (i32, i32, i32) {
        (0, 0, 0)
    }
    /// `playerActor.GetMagicInfo(name)?.Level`
    fn magic_level(&self, _name: &str) -> Option<i32> {
        None
    }
    /// `playerActor.InSafeZone()`
    fn in_safe_zone(&self) -> bool {
        false
    }
    /// `SystemShare.WorldEngine.GetPlayObject(name) != null`
    fn is_player_online(&self, _name: &str) -> bool {
        false
    }
    /// `SystemShare.WorldEngine.GetMapHuman(map)`
    fn map_human_count(&self, _map: &str) -> i32 {
        0
    }
    /// `SystemShare.MapMgr.FindMap(name) != null`
    fn map_exists(&self, _map: &str) -> bool {
        false
    }
    /// `playerActor.Envir` 对应的地图名（`None` = 不在任何地图）
    fn envir_map_name(&self) -> Option<String> {
        None
    }
    /// `playerActor.Envir.GetXYObjCount(x, y)`（`None` = 无地图）
    fn xy_obj_count(&self) -> Option<i32> {
        None
    }
    /// `playerActor.LastHiter?.Race`（最后一次攻击者；`None` = 无）
    fn last_hiter_race(&self) -> Option<i32> {
        None
    }
    /// `playerActor.LastHiter?.ChrName`（`None` = 无）
    fn last_hiter_name(&self) -> Option<String> {
        None
    }
    /// `playerActor.TargetCret?.ChrName`（`None` = 无目标）
    fn target_name(&self) -> Option<String> {
        None
    }
    /// `playerActor.GetPoseCreate()` 的快照（`None` = 无面向对象）
    fn pose(&self) -> Option<PoseInfo> {
        None
    }
    /// `SystemShare.Config.GlobalVal[n]`（全局数值变量，`G` 前缀）
    fn global_val(&self, _n: usize) -> i32 {
        0
    }
    /// `SystemShare.Config.GlobaDyMval[n]`
    fn globa_dy_mval(&self, _n: usize) -> i32 {
        0
    }
    /// `SystemShare.Config.GlobalAVal[n]`（全局文本变量，`A` 前缀）
    fn global_a_val(&self, _n: usize) -> String {
        String::new()
    }
    /// `playerActor.ServerIntVal[n]`
    fn server_int_val(&self, _n: usize) -> i32 {
        0
    }
    /// `playerActor.ServerStrVal[n]`
    fn server_str_val(&self, _n: usize) -> String {
        String::new()
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
    /// `normNpc.Path`（NPC 脚本所在目录，供文件列表类条件拼路径）
    fn path(&self) -> String {
        String::new()
    }
    /// Envir 根目录（host 注入）；`None` ⇒ 文件列表类条件按"文件不存在"处理并记错误
    fn envir_root(&self) -> Option<String> {
        None
    }
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

    /// C# `ConditionProcessingSys.ScriptConditionError(normNpc, baseObject, questConditionInfo, ConditionCode sCmd)`
    /// —— **注意是 `ConditionProcessingSys` 类内的本地重载**（不是 `ScriptEngine` 的那两个），
    /// 文本格式为 `[脚本参数不正确] Cmd:{枚举成员名} NPC名称:… 地图:… 座标:x:y 参数1:… 参数5:`
    /// （只到参数 5，且分隔用 "座标"/"Cmd"）。条件处理器的报错都走这一条。
    pub(crate) fn script_condition_error(&mut self, cmd_member: &str, info: &QuestConditionInfo) {
        let msg = format!(
            "[脚本参数不正确] Cmd:{cmd_member} NPC名称:{} 地图:{} 座标:{}:{} 参数1:{} 参数2:{} 参数3:{} 参数4:{} 参数5:{}",
            self.npc.chr_name(),
            self.npc.map_name(),
            self.npc.curr_x(),
            self.npc.curr_y(),
            info.s_param1,
            info.s_param2,
            info.s_param3,
            info.s_param4,
            info.s_param5
        );
        self.errors.push(EngineError::ScriptError { text: msg });
    }

    /// C# `ProcessingBase.GetValValue(playerActor, sMsg, ref int nValue)`：
    /// 按 `SystemShare.GetValNameNo` 的槽号区间取数值；未命中返回 `None`（C# `result = false`）。
    pub(crate) fn get_val_value_int(&self, name: &str) -> Option<i32> {
        if name.is_empty() {
            return None;
        }
        let n01 = crate::engine::vars::get_val_name_no(name);
        if n01 < 0 {
            return None;
        }
        let p = &*self.player;
        let v = match n01 {
            0..=99 => p.mn_val(n01 as usize),
            100..=199 => p.global_val((n01 - 100) as usize),
            200..=299 => p.mdy_val((n01 - 200) as usize),
            300..=399 => p.mnmval((n01 - 300) as usize),
            400..=499 => p.globa_dy_mval((n01 - 400) as usize),
            500..=599 => p.mn_integer((n01 - 500) as usize),
            600..=699 => mir2_shared::hutil32::str_to_int(&p.ms_string((n01 - 600) as usize), 0),
            700..=799 => mir2_shared::hutil32::str_to_int(&p.global_a_val((n01 - 700) as usize), 0),
            800..=1199 => p.global_val((n01 - 700) as usize),
            1200..=1599 => {
                mir2_shared::hutil32::str_to_int(&p.global_a_val((n01 - 1100) as usize), 0)
            }
            1600..=1699 => {
                mir2_shared::hutil32::str_to_int(&p.server_str_val((n01 - 1600) as usize), 0)
            }
            1700..=1799 => p.server_int_val((n01 - 1700) as usize),
            _ => return None,
        };
        Some(v)
    }

    /// C# `ProcessingBase.GetValValue(playerActor, sMsg, ref string sValue)`：
    /// 按槽号区间取文本变量；未命中返回 `None`（C# `result = false`，`sValue` 不变）。
    pub(crate) fn get_val_value_string(&self, name: &str) -> Option<String> {
        if name.is_empty() {
            return None;
        }
        let n01 = crate::engine::vars::get_val_name_no(name);
        if n01 < 0 {
            return None;
        }
        let p = &*self.player;
        let v = match n01 {
            600..=699 => p.ms_string((n01 - 600) as usize),
            700..=799 => p.global_a_val((n01 - 700) as usize),
            1200..=1599 => p.global_a_val((n01 - 1100) as usize),
            1600..=1699 => p.server_str_val((n01 - 1600) as usize),
            _ => return None,
        };
        Some(v)
    }

    /// C# `ProcessingBase.GotoLableCheckStringList(sHumName, sListFileName)`：
    /// 按 `SystemShare.GetEnvirFilePath` 解析出绝对路径 → 逐行 `Trim()` 后忽略大小写比对。
    ///
    /// 文件不存在时 C# 记 `LogService.Error("file not found => " + path)`（进 `err` 流）；
    /// 无 Envir 根（host 未注入）时按同一口径报"文件找不到"。
    pub(crate) fn check_string_list(&mut self, text: &str, rel: &str) -> bool {
        let abs = match self.npc.envir_root() {
            Some(root) => {
                let mut p = std::path::PathBuf::from(root);
                for part in rel.replace('\\', "/").split('/') {
                    if !part.is_empty() {
                        p.push(part);
                    }
                }
                p
            }
            None => std::path::PathBuf::from(rel),
        };
        let bytes = match std::fs::read(&abs) {
            Ok(b) => b,
            Err(_) => {
                let text = format!("file not found => {}", abs.display());
                self.errors.push(EngineError::ScriptError { text });
                return false;
            }
        };
        let content = crate::textfile::decode_bytes(&bytes);
        for line in crate::textfile::split_lines(&content) {
            if line.trim().eq_ignore_ascii_case(text) {
                return true;
            }
        }
        false
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
