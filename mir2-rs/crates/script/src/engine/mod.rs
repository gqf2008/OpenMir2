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
    /// `IPlayerActor.ChrName`（结婚/离婚文案用）
    pub chr_name: String,
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
    /// `playerActor.GetQuestUnitStatus(flag)`（`ConditionCheckUnit` 的取数口）。
    ///
    /// 位语义见 [`crate::engine::quest_state::QuestBitmap`]；默认返回 0 —— 引擎本身不持有任务态，
    /// 真世界/夹具按需覆写。
    fn quest_unit_status(&self, _flag: i16) -> u8 {
        0
    }
    /// `playerActor.GetQuestUnitOpenStatus(flag)`（`ConditionOfCheckOpen` 的取数口）。
    fn quest_unit_open_status(&self, _flag: i16) -> u8 {
        0
    }
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
    /// `playerActor.SendMsg([baseObject,] wIdent, wParam, nParam1, nParam2, nParam3[, sMsg])`
    /// （`IActor.SendMsg` 重载；ident 取 `Messages` 常量）。
    ///
    /// 五个数值参数一个都不能少：flow 夹具按实参位次逐个打印
    /// （`msg <wIdent> <wParam> <nParam1> <nParam2> <nParam3> <sMsg>`），
    /// 少一位就会把 `nParam3` 与文本挤到错误的列上。
    fn send_msg(
        &mut self,
        ident: i32,
        w_param: i32,
        n_param1: i32,
        n_param2: i32,
        n_param3: i32,
        msg: Option<&str>,
    );
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

    // ---- B2/M4 var 家族批次（2026-10-11）：编号槽位 setter 与 `DynamicVar` 存储 ----
    // 说明：默认实现为空（世界适配层尚未接上）；引擎侧 `SetMovDataValNameValue` /
    // `SetMovDataDynamicVarValue` 走这些方法写回。

    /// `SystemShare.Config.GlobalVal[n] = v`
    fn set_global_val(&mut self, _n: usize, _v: i32) {}
    /// `SystemShare.Config.GlobaDyMval[n] = v`
    fn set_globa_dy_mval(&mut self, _n: usize, _v: i32) {}
    /// `SystemShare.Config.GlobalAVal[n] = v`
    fn set_global_a_val(&mut self, _n: usize, _v: String) {}
    /// `playerActor.MDyVal[n] = v`
    fn set_mdy_val(&mut self, _n: usize, _v: i32) {}
    /// `playerActor.MNMval[n] = v`
    fn set_mnmval(&mut self, _n: usize, _v: i32) {}
    /// `playerActor.MNInteger[n] = v`
    fn set_mn_integer(&mut self, _n: usize, _v: i32) {}
    /// `playerActor.MSString[n] = v`
    fn set_ms_string(&mut self, _n: usize, _v: String) {}
    /// `playerActor.ServerIntVal[n] = v`
    fn set_server_int_val(&mut self, _n: usize, _v: i32) {}
    /// `playerActor.ServerStrVal[n] = v`
    fn set_server_str_val(&mut self, _n: usize, _v: String) {}

    /// `GetDynamicVarMap(playerActor, kind)` 取变量值（`TryGetValue` 语义）。
    fn dyn_var_get(&self, _kind: DynVarKind, _name: &str) -> Option<DynVar> {
        None
    }
    /// 写回**已存在**的变量（`SetMovDataDynamicVarValue` 的 `TryGetValue` 命中分支）；
    /// 返回 false 表示变量不存在（或类型不匹配），调用方据此报错。
    fn dyn_var_set(&mut self, _kind: DynVarKind, _name: &str, _v: DynVar) -> bool {
        false
    }
    /// `VAR` 声明：不存在则新增，返回是否新增（已存在返回 false）。
    fn dyn_var_insert(&mut self, _kind: DynVarKind, _name: &str, _v: DynVar) -> bool {
        false
    }

    // ---- B2/M4 动作批次（2026-10-11）：非 var 家族动作处理器所需的世界面 ----
    // 说明：默认实现一律取「世界缺省」（0 / 空 / false / 空实现），与 C# flow 夹具
    // （DispatchProxy 未实现的方法回默认值且不落日志）一致；真世界适配层随之接上后覆写。

    /// `HUtil32.GetTickCount()`（世界时钟，毫秒）
    fn tick_count(&self) -> i32 {
        0
    }
    /// `playerActor.SysMsg(msg, color, msgType)`（`MsgColor` / `MsgType` 取值见本模块常量）
    fn sys_msg(&mut self, _msg: &str, _color: i32, _msg_type: i32) {}
    /// `SystemShare.WorldEngine.SendBroadCastMsg(msg, msgType)`（`MsgType` 取值见本模块常量：
    /// LINE MSG 0-3 为 `System`、其余广播为 `Say`；世界适配层据此选择广播类型）
    fn world_broadcast(&mut self, _msg: &str, _msg_type: i32) {}
    /// `playerActor.SendGroupText(msg)`
    fn send_group_text(&mut self, _msg: &str) {}
    /// `playerActor.GroupOwner`（0 = 无组）。
    ///
    /// 供 §1.O 扩展面脚本读侧使用：`ConditionOfCheckGroupCount`
    /// （`ConditionProcessingSys.cs:2670`）/ `ConditionOfIsGroupMaster`（`:4466`）/
    /// `ActionOfGroupRecall`（`ExecutionProcessingSys.cs:3918`）。默认 0 = 无组。
    fn group_owner(&self) -> i32 {
        0
    }
    /// `playerActor.GroupMembers` 各成员的 `Job`（`PlayerJob`：Warrior=0 / Wizard=1 /
    /// Taoist=2）。oracle 只有**团长自己**持有这份列表（`PlayObject.Operate.cs:834-835/875`），
    /// 非团长为空 ⇒ 默认空表与 oracle 一致。供 `ConditionOfCheckGroupClass`
    /// （`ConditionProcessingSys.cs:262`）读侧使用。
    fn group_member_jobs(&self) -> Vec<i32> {
        Vec::new()
    }
    /// `playerActor.MyGuild.SendGuildMsg(msg)`（仅在 `MyGuild != null` 时调用）
    fn guild_send_msg(&mut self, _msg: &str) {}
    /// `playerActor.SendDefMessage(ident, param, tag1, tag2, series[, msg])`
    fn send_def_message(
        &mut self,
        _ident: i32,
        _param: i32,
        _tag1: i32,
        _tag2: i32,
        _series: i32,
        _msg: &str,
    ) {
    }
    /// `playerActor.SendRefMsg(ident, p1, p2, p3, p4, msg)`
    fn send_ref_msg(&mut self, _ident: i32, _p1: i32, _p2: i32, _p3: i32, _p4: i32, _msg: &str) {}
    /// `playerActor.MapRandomMove(map, range)`
    fn map_random_move(&mut self, _map: &str, _range: i32) {}

    /// `playerActor.GameGold = v`
    fn set_game_gold(&mut self, _v: i32) {}
    /// `playerActor.GameGoldChanged()`（`GameGold` 与 `GamePoint` 变更后共用）
    fn game_gold_changed(&mut self) {}
    /// `playerActor.GamePoint = v`
    fn set_game_point(&mut self, _v: i32) {}
    /// `playerActor.CreditPoint = v`（C# 字段类型 `byte`）
    fn set_credit_point(&mut self, _v: i32) {}
    /// `playerActor.Abil.Exp`
    fn abil_exp(&self) -> i32 {
        0
    }
    fn set_abil_exp(&mut self, _v: i32) {}
    /// `playerActor.Abil.Level = v`
    fn set_abil_level(&mut self, _v: i32) {}
    /// `playerActor.HasLevelUp(oldLevel)`
    fn has_level_up(&mut self, _old_level: i32) {}
    /// `playerActor.Abil.HP`（C# 字段类型 `ushort`，取值 0..=65535）
    fn abil_hp(&self) -> i32 {
        0
    }
    fn set_abil_hp(&mut self, _v: i32) {}
    /// `playerActor.Abil.MP`（C# 字段类型 `ushort`）
    fn abil_mp(&self) -> i32 {
        0
    }
    fn set_abil_mp(&mut self, _v: i32) {}
    /// `playerActor.WAbil.HP`
    fn wabil_hp(&self) -> i32 {
        0
    }
    /// `playerActor.WAbil.MaxHP`
    fn wabil_max_hp(&self) -> i32 {
        0
    }
    /// `playerActor.WAbil.MP`
    fn wabil_mp(&self) -> i32 {
        0
    }
    /// `playerActor.WAbil.MaxMP`
    fn wabil_max_mp(&self) -> i32 {
        0
    }
    /// `playerActor.PkPoint`
    fn pk_point(&self) -> i32 {
        0
    }
    fn set_pk_point(&mut self, _v: i32) {}
    /// `playerActor.PvpLevel()`（C# 返回 `byte`）
    fn pvp_level(&self) -> i32 {
        0
    }
    /// `playerActor.RefNameColor()`
    fn ref_name_color(&mut self) {}
    /// `playerActor.RefShowName()`
    fn ref_show_name(&mut self) {}
    /// `playerActor.Hair = v`（C# 字段类型 `byte`）
    fn set_hair(&mut self, _v: u8) {}
    /// `playerActor.FeatureChanged()`
    fn feature_changed(&mut self) {}
    /// `playerActor.BonusPoint`
    fn bonus_point(&self) -> i32 {
        0
    }
    fn set_bonus_point(&mut self, _v: i32) {}
    /// `playerActor.ReLevel = v`（C# 字段类型 `byte`）
    fn set_re_level(&mut self, _v: i32) {}
    /// `SystemShare.Config.ShowScriptActionMsg`
    fn config_show_script_action_msg(&self) -> bool {
        false
    }
    /// `SystemShare.Config.ReNewLevelClearExp`（C# 分支体已注释，仅读配置）
    fn config_renewlevel_clear_exp(&self) -> bool {
        false
    }

    /// **PL-3 / P27**：`ActionOfKillMonExpRate`（`ExecutionProcessingSys.cs:5449-5464`）写
    /// `playerActor.KillMonExpRate = nRate; KillMonExpRateTime = nTime;`。
    fn set_kill_mon_exp_rate(&mut self, _rate: i32, _time: i32) {}
    /// **PL-3 / P27**：`ActionOfPowerRate`（`ExecutionProcessingSys.cs:5539-5554`）写
    /// `playerActor.PowerRate = nRate; PowerRateTime = nTime;`。
    fn set_power_rate(&mut self, _rate: i32, _time: i32) {}
    /// `playerActor.AdminMode`
    fn admin_mode(&self) -> bool {
        false
    }
    fn set_admin_mode(&mut self, _v: bool) {}
    /// `playerActor.SuperMan`
    fn super_man(&self) -> bool {
        false
    }
    fn set_super_man(&mut self, _v: bool) {}
    /// `playerActor.ObMode`
    fn ob_mode(&self) -> bool {
        false
    }
    fn set_ob_mode(&mut self, _v: bool) {}
    /// `playerActor.BoSendMsgFlag = v`
    fn set_bo_send_msg_flag(&mut self, _v: bool) {}
    /// `playerActor.BoChangeItemNameFlag = v`
    fn set_bo_change_item_name_flag(&mut self, _v: bool) {}
    /// `playerActor.BoKickFlag = BoReconnection = BoSoftClose = true`
    fn set_kick_flags(&mut self) {}
    /// `playerActor.Permission = v`（C# 字段类型 `byte`）
    fn set_permission(&mut self, _v: i32) {}
    /// `playerActor.ProcessUserLineMsg(line)`
    fn process_user_line_msg(&mut self, _line: &str) {}
    /// `playerActor.NoItem = v`
    fn set_no_item(&mut self, _v: bool) {}
    /// `playerActor.Die()`
    fn die(&mut self) {}
    /// `playerActor.SetLastHiter(normNpc)`
    fn set_last_hiter_npc(&mut self) {}
    /// `SystemShare.MissionMap / MissionX / MissionY`（全局任务坐标）
    fn set_mission(&mut self, _map: &str, _x: i32, _y: i32) {}

    /// `IsTimeGoto = false; TimeGotoLable = ""; TimeGotoNpc = null`
    fn clear_delay_goto(&mut self) {}
    /// `playerActor.IsTimeRecall = v`
    fn set_is_time_recall(&mut self, _v: bool) {}
    /// `IsTimeRecall = true; TimeRecallMoveMap/X/Y = 当前位置; TimeRecallTick = tick`
    fn set_time_recall(&mut self, _map: &str, _x: i32, _y: i32, _tick: i32) {}
    /// `ActionOfDelayCall`：`DelayCall / DelayCallLabel / DelayCallTick / IsDelayCall /
    /// DelayCallNpc` + `IsTimeGoto = true; TimeGotoTick; TimeGotoLable; TimeGotoNpc = normNpc`
    fn set_delay_call(&mut self, _delay: i32, _label: &str, _npc_id: i32, _goto_tick: i32) {}

    /// `ValLabel / ValType / GotoNpcLabel / ValNpcType`（`ActionOfQueryValue`）
    fn set_val_query(
        &mut self,
        _val_label: u8,
        _val_type: u8,
        _goto_npc_label: &str,
        _val_npc_type: i32,
    ) {
    }
    /// `TakeDlgItem / GotoNpcLabel`（`ActionOfQueryItemDlg`）
    fn set_dlg_query(&mut self, _take_dlg_item: bool, _goto_npc_label: &str) {}
    /// `playerActor.PlayDiceLabel = v`
    fn set_play_dice_label(&mut self, _label: &str) {}

    /// `AutoGetExpInSafeZone / AutoGetExpEnvir / AutoGetExpTime / AutoGetExpPoint`
    fn set_auto_get_exp(
        &mut self,
        _safe_zone: bool,
        _envir_map: Option<&str>,
        _time_ms: i32,
        _point: i32,
    ) {
    }
    /// `AutoGetExp*`（挂机口径）+ `OffLineFlag = true` + `KickOffLineTick`
    fn set_offline(
        &mut self,
        _envir_map: Option<&str>,
        _time_ms: i32,
        _point: i32,
        _kick_tick: i32,
    ) {
    }

    /// `SystemShare.WorldEngine.FindMagic(name)` → `(MagicId, MagicName)`
    fn find_magic(&self, _name: &str) -> Option<(i32, String)> {
        None
    }
    /// `playerActor.IsTrainingSkill(magicId)`
    fn is_training_skill(&self, _magic_id: i32) -> bool {
        false
    }
    /// `MagicList.Add(userMagic) + SendAddMagic(userMagic) + RecalcAbilitys()`
    /// （`UserMagic.Level = (byte)nLevel`，由实现侧按 `u8` 取用）
    fn learn_magic(&mut self, _magic_id: i32, _magic_name: &str, _level: u8) {}

    /// `playerActor.UseItems.Length`
    fn use_items_len(&self) -> i32 {
        0
    }
    /// `playerActor.UseItems[i].Index = v`
    fn set_use_item_index(&mut self, _i: i32, _v: i32) {}
    /// `SystemShare.ItemSystem.GetStdItemName(UseItems[i].Index)`
    fn use_item_std_name(&self, _i: i32) -> String {
        String::new()
    }
    /// `SystemShare.ItemSystem.GetStdItem(UseItems[i].Index) != null`
    fn use_item_std_item_present(&self, i: i32) -> bool {
        self.use_item_index(i) > 0
    }
    /// `playerActor.SendDelItems(UseItems[i])`
    fn send_del_use_item(&mut self, _i: i32) {}
    /// `playerActor.UseItems[i].Dura`
    fn use_item_dura(&self, _i: i32) -> i32 {
        0
    }
    fn set_use_item_dura(&mut self, _i: i32, _v: i32) {}
    /// `playerActor.UseItems[i].DuraMax`
    fn use_item_dura_max(&self, _i: i32) -> i32 {
        0
    }

    // ---- B2/M4 var 家族 / 剩余动作批次（2026-10-11）：世界面钩子 ----
    // 约定：默认实现为「世界缺省」（no-op / 0 / false），与 C# flow 夹具（DispatchProxy
    // 回默认值且不落日志）口径一致；真世界适配层接入后覆写。

    /// `playerActor.UseItems[i].Desc[attr]`（属性附加点）
    fn use_item_desc(&self, _i: i32, _attr: i32) -> i32 {
        0
    }
    /// `playerActor.UseItems[i].Desc[attr] = v`
    fn set_use_item_desc(&mut self, _i: i32, _attr: i32, _v: i32) {}
    /// `playerActor.UseItems[i].DuraMax = v`
    fn set_use_item_dura_max(&mut self, _i: i32, _v: i32) {}
    /// `playerActor.SendUpdateItem(UseItems[i])`
    fn send_update_item(&mut self, _i: i32) {}
    /// `SystemShare.WorldEngine.RegenMonsterByName(map, x, y, name) != null`
    fn regen_monster(&mut self, _map: &str, _x: i32, _y: i32, _name: &str) -> bool {
        false
    }
    /// `ActionOfClearMapMon`：把地图上的怪物（排除 Master / NoClearMonList）置 NoItem + MakeGhost
    fn clear_map_monsters(&mut self, _map: &str) {}
    /// `DateTime.Today` 的 `ToString()` 文本（`ActionOfAddUseDateList` 用）
    fn today_string(&self) -> String {
        String::new()
    }

    // ---- Marry / UnMarry 的自身侧与对方（pose）侧钩子 ----
    /// `playerActor.IsStartMarry`
    fn is_start_marry(&self) -> bool {
        false
    }
    fn set_is_start_marry(&mut self, _v: bool) {}
    /// `playerActor.IsStartUnMarry`
    fn is_start_un_marry(&self) -> bool {
        false
    }
    fn set_is_start_un_marry(&mut self, _v: bool) {}
    /// `playerActor.DearName = v`
    fn set_dear_name(&mut self, _v: &str) {}
    /// `playerActor.MarryCount++`
    fn inc_marry_count(&mut self) {}
    /// `playerActor.GetPoseCreate().ActorId`
    fn pose_actor_id(&self) -> i32 {
        0
    }
    /// `poseHuman.DearName`
    fn pose_dear_name(&self) -> String {
        String::new()
    }
    fn set_pose_dear_name(&mut self, _v: &str) {}
    fn pose_is_start_marry(&self) -> bool {
        false
    }
    fn set_pose_is_start_marry(&mut self, _v: bool) {}
    fn pose_is_start_un_marry(&self) -> bool {
        false
    }
    fn set_pose_is_start_un_marry(&mut self, _v: bool) {}
    fn inc_pose_marry_count(&mut self) {}
    fn pose_ref_show_name(&mut self) {}
    /// `normNpc.GotoLable(poseHuman, label, false)`
    fn pose_goto_label(&mut self, _label: &str) {}
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
    /// `normNpc.SendSayMsg(msg)`（`ActionOfLineMsg` 的 nParam1=4 分支）
    fn send_say_msg(&mut self, _msg: &str) {}
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
    /// **异常式中止信号**：C# 的 `ActionOfAutoGetExp` 在 `sParam3[1]` 越界时抛
    /// `IndexOutOfRangeException`，异常从 `ScriptEngine.GotoLable` 逃出 ⇒ **整个 GotoLable
    /// 中止**（当前动作列表后续动作、后续 procedure、以及 `sSendMsg` 说词都不执行）。
    /// Rust 不 panic，用本字段表达同一次展开：动作处理器置 `true`，[`Engine::run_actions`]
    /// 与 [`Engine::goto_label`] 见旗即返回（whitelist B-108）。
    pub abort_goto: bool,
}

impl<'n, 'p> Engine<'n, 'p> {
    pub fn new(npc: &'n mut dyn ScriptNpc, player: &'p mut dyn ScriptPlayer) -> Self {
        Self {
            npc,
            player,
            goto_params: GotoLabParams::default(),
            errors: Vec::new(),
            current_script: None,
            abort_goto: false,
        }
    }

    /// `ScriptEngine.GotoLable`（1:1 移植，含 C# 的选脚本顺序与 break 语义）。
    pub fn goto_label(&mut self, label: &str, bo_ext_jmp: bool) {
        // 上一次（嵌套）GotoLable 的异常中止不得泄漏到本次调用。
        self.abort_goto = false;
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
                    // C# 的越界异常会从 GotoLable 逃出 ⇒ 后续 procedure 与说词都不执行。
                    if self.abort_goto {
                        return;
                    }
                    break;
                }
                if self.goto_params.send_say_msg {
                    let msg = self.player.line_variable_text(&s_send_msg);
                    self.npc.send_say(&msg, true);
                }
            } else {
                s_send_msg.push_str(&proc_.s_else_say_msg);
                if !self.run_actions(&proc_.else_action_list) {
                    if self.abort_goto {
                        return;
                    }
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
        self.script_action_error_msg("", cmd_member, info);
    }

    /// C# `ScriptActionError(normNpc, playerActor, sErrMsg, questActionInfo, sCmd)`：
    /// `sErrMsg` 非空时插在 "[脚本错误] " 之后（var 家族的「变量…不存在」等文本）。
    pub(crate) fn script_action_error_msg(
        &mut self,
        err_msg: &str,
        cmd_member: &str,
        info: &QuestActionInfo,
    ) {
        let msg = format!(
            "[脚本错误] {} 脚本命令:{} NPC名称:{} 地图:{}({}:{}) 参数1:{} 参数2:{} 参数3:{} 参数4:{} 参数5:{} 参数6:{}",
            err_msg,
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

    // ---- B2/M4 var 家族批次（2026-10-11）：`ProcessingBase` 取值/写值层 ----

    /// `HUtil32.IsVarNumber`：`HUMAN`/`GUILD`/`GLOBAL` 前缀判断。
    pub(crate) fn is_var_number(s: &str) -> bool {
        use mir2_shared::hutil32::compare_lstr_prefix as p;
        p(s, "HUMAN", 5) || p(s, "GUILD", 5) || p(s, "GLOBAL", 6)
    }

    /// C# `HUtil32.ArrestStringEx(s, "(", ")", ref sParam)`（仅当以 `<$STR(` 开头）。
    pub(crate) fn arrest_str_param(&self, s: &str) -> String {
        if mir2_shared::hutil32::compare_lstr_prefix(s, "<$STR(", 6) {
            match mir2_shared::hutil32::arrest_string_ex_ref(s, '(', ')').0 {
                mir2_shared::hutil32::ArrestWrite::Value(v) => v,
                mir2_shared::hutil32::ArrestWrite::Unchanged => String::new(),
            }
        } else {
            s.to_string()
        }
    }

    /// `ProcessingBase.GetMovDataValNameValue`：编号槽位取值（命中 `(0,99)…(1200,1599)`）。
    pub(crate) fn get_mov_data_val_name_value(&self, name: &str) -> Option<VarVal> {
        let n100 = crate::engine::vars::get_val_name_no(name);
        if n100 < 0 {
            return None;
        }
        let p = &*self.player;
        let v = match n100 {
            0..=99 => VarVal::Int(p.mn_val(n100 as usize)),
            100..=199 => VarVal::Int(p.global_val((n100 - 100) as usize)),
            200..=299 => VarVal::Int(p.mdy_val((n100 - 200) as usize)),
            300..=399 => VarVal::Int(p.mnmval((n100 - 300) as usize)),
            400..=499 => VarVal::Int(p.globa_dy_mval((n100 - 400) as usize)),
            500..=599 => VarVal::Int(p.mn_integer((n100 - 500) as usize)),
            600..=699 => VarVal::Str(p.ms_string((n100 - 600) as usize)),
            700..=799 => VarVal::Str(p.global_a_val((n100 - 700) as usize)),
            800..=1199 => VarVal::Int(p.global_val((n100 - 700) as usize)),
            1200..=1599 => VarVal::Str(p.global_a_val((n100 - 1100) as usize)),
            _ => return None,
        };
        Some(v)
    }

    /// `ProcessingBase.SetMovDataValNameValue`：按 `nDataType`（1 数值 / 0 文本）写回编号槽位。
    pub(crate) fn set_val_name_value(
        &mut self,
        name: &str,
        s_value: &str,
        n_value: i32,
        n_data_type: i32,
    ) -> bool {
        let n100 = crate::engine::vars::get_val_name_no(name);
        if n100 < 0 {
            return false;
        }
        match n_data_type {
            1 => match n100 {
                0..=99 => self.player.set_mn_val(n100 as usize, n_value),
                100..=199 => self.player.set_global_val((n100 - 100) as usize, n_value),
                200..=299 => self.player.set_mdy_val((n100 - 200) as usize, n_value),
                300..=399 => self.player.set_mnmval((n100 - 300) as usize, n_value),
                400..=499 => self
                    .player
                    .set_globa_dy_mval((n100 - 400) as usize, n_value),
                500..=599 => self.player.set_mn_integer((n100 - 500) as usize, n_value),
                600..=699 => self
                    .player
                    .set_ms_string((n100 - 600) as usize, n_value.to_string()),
                700..=799 => self
                    .player
                    .set_global_a_val((n100 - 700) as usize, n_value.to_string()),
                800..=1199 => self.player.set_global_val((n100 - 700) as usize, n_value),
                1200..=1599 => self
                    .player
                    .set_global_a_val((n100 - 1100) as usize, n_value.to_string()),
                _ => return false,
            },
            0 => {
                let iv = mir2_shared::hutil32::str_to_int(s_value, 0);
                match n100 {
                    0..=99 => self.player.set_mn_val(n100 as usize, iv),
                    100..=199 => self.player.set_global_val((n100 - 100) as usize, iv),
                    200..=299 => self.player.set_mdy_val((n100 - 200) as usize, iv),
                    300..=399 => self.player.set_mnmval((n100 - 300) as usize, iv),
                    400..=499 => self.player.set_globa_dy_mval((n100 - 400) as usize, iv),
                    500..=599 => self.player.set_mn_integer((n100 - 500) as usize, iv),
                    600..=699 => self
                        .player
                        .set_ms_string((n100 - 600) as usize, s_value.to_string()),
                    700..=799 => self
                        .player
                        .set_global_a_val((n100 - 700) as usize, s_value.to_string()),
                    800..=1199 => self.player.set_global_val((n100 - 700) as usize, iv),
                    1200..=1599 => self
                        .player
                        .set_global_a_val((n100 - 1100) as usize, s_value.to_string()),
                    _ => return false,
                }
            }
            _ => return false,
        }
        true
    }

    /// `GetDynamicVarMap` 的 null 判定 + `sName` 解析：`(kind, sName)`。
    /// `GUILD` 且无行会 ⇒ `None`（C# 返回 null）。
    pub(crate) fn dyn_var_resolve(&self, s_type: &str) -> Option<(DynVarKind, String)> {
        let kind = crate::engine::vars::dyn_var_kind_of(s_type)?;
        let name = match kind {
            DynVarKind::Human => self.player.chr_name(),
            DynVarKind::Guild => self.player.guild_name()?,
            DynVarKind::Global => "GLOBAL".to_string(),
            DynVarKind::Account => self.player.user_account(),
        };
        Some((kind, name))
    }

    /// `ProcessingBase.GetMovDataDynamicVarValue`。
    pub(crate) fn get_mov_data_dynamic_var_value(
        &self,
        kind: DynVarKind,
        name: &str,
    ) -> Option<VarVal> {
        match self.player.dyn_var_get(kind, name)? {
            DynVar::Integer(v) => Some(VarVal::Int(v)),
            DynVar::Str(s) => Some(VarVal::Str(s)),
        }
    }

    /// `ProcessingBase.SetMovDataDynamicVarValue`（类型匹配才写回）。
    pub(crate) fn set_mov_data_dynamic_var_value(
        &mut self,
        kind: DynVarKind,
        name: &str,
        s_value: &str,
        n_value: i32,
        n_data_type: i32,
    ) -> bool {
        match self.player.dyn_var_get(kind, name) {
            Some(DynVar::Integer(_)) if n_data_type == 1 => {
                self.player
                    .dyn_var_set(kind, name, DynVar::Integer(n_value))
            }
            Some(DynVar::Str(_)) => {
                self.player
                    .dyn_var_set(kind, name, DynVar::Str(s_value.to_string()))
            }
            _ => false,
        }
    }

    /// `ProcessingBase.GetMovDataType`（`MOV` 的 0..7 数据源判别）。
    pub(crate) fn get_mov_data_type(&self, info: &QuestActionInfo) -> i32 {
        let s_param1 = self.arrest_str_param(&info.s_param1);
        let s_param2 = self.arrest_str_param(&info.s_param2);
        let s_param3 = self.arrest_str_param(&info.s_param3);
        if Self::is_var_number(&s_param1) {
            if !s_param3.is_empty() && s_param3.starts_with('<') && s_param3.ends_with('>') {
                return 0;
            }
            if !s_param3.is_empty() && crate::engine::vars::get_val_name_no(&s_param3) >= 0 {
                return 1;
            }
            if !s_param3.is_empty() && mir2_shared::hutil32::is_string_number(&s_param3) {
                return 2;
            }
            return 3;
        }
        let n01 = crate::engine::vars::get_val_name_no(&s_param1);
        if n01 >= 0 {
            if !s_param2.is_empty() && s_param2.starts_with('<') && s_param2.ends_with('>') {
                return 4;
            }
            if !s_param2.is_empty() && crate::engine::vars::get_val_name_no(&s_param2) >= 0 {
                return 5;
            }
            if !s_param2.is_empty() && Self::is_var_number(&s_param2) {
                return 6;
            }
            return 7;
        }
        -1
    }

    /// `ProcessingBase.GetMovDataHumanInfoValue` 的引擎侧移植。
    ///
    /// 已覆盖语料用到的固定变量（`$HP`/`$MP`/`$MAXHP`/`$MAXMP`/`$LEVEL`/…）以及
    /// `$HUMAN(...)`/`$GLOBAL(...)`/`$ACCOUNT(...)` 三类动态变量；
    /// 其余固定变量（依赖世界未接入的 `WAbil.AC/DC/…`、在线人数、时间等）返回未命中
    /// —— 与 C# flow 夹具（DispatchProxy 回默认值）口径一致，接入世界适配层后再补齐。
    /// 注意：`$STR(...)`/`$H.HP` 在 C# 原文里也**不在**该 switch 内（见 ProcessingBase.cs:35-566），
    /// 故同样返回未命中。
    pub(crate) fn get_mov_data_human_info_value(&self, variable: &str) -> Option<VarVal> {
        if variable.is_empty() {
            return None;
        }
        let s10 = match mir2_shared::hutil32::arrest_string_ex_ref(variable, '<', '>').0 {
            mir2_shared::hutil32::ArrestWrite::Value(v) => v,
            mir2_shared::hutil32::ArrestWrite::Unchanged => return None,
        };
        if s10.is_empty() {
            return None;
        }
        let s_variable = s10.as_str();
        let fixed = match s_variable {
            "$USERNAME" => Some(VarVal::Str(self.player.chr_name())),
            "$X" => Some(VarVal::Int(self.player.curr_pos().0)),
            "$Y" => Some(VarVal::Int(self.player.curr_pos().1)),
            "$MAP" => Some(VarVal::Str(
                self.player.envir_map_name().unwrap_or_default(),
            )),
            "$GUILDNAME" => Some(VarVal::Str(
                self.player.guild_name().unwrap_or_else(|| "无".to_string()),
            )),
            "$RELEVEL" => Some(VarVal::Int(self.player.re_level())),
            "$LEVEL" => Some(VarVal::Int(self.player.level())),
            "$HP" => Some(VarVal::Int(self.player.wabil_hp())),
            "$MAXHP" => Some(VarVal::Int(self.player.wabil_max_hp())),
            "$MP" => Some(VarVal::Int(self.player.wabil_mp())),
            "$MAXMP" => Some(VarVal::Int(self.player.wabil_max_mp())),
            "$EXP" => Some(VarVal::Int(self.player.abil_exp())),
            "$PKPOINT" => Some(VarVal::Int(self.player.pk_point())),
            "$CREDITPOINT" => Some(VarVal::Int(self.player.credit_point())),
            "$GOLDCOUNT" => Some(VarVal::Int(self.player.gold())),
            "$GAMEGOLD" => Some(VarVal::Int(self.player.game_gold())),
            "$GAMEPOINT" => Some(VarVal::Int(self.player.game_point())),
            _ => None,
        };
        if fixed.is_some() {
            return fixed;
        }
        if mir2_shared::hutil32::compare_lstr_prefix(s_variable, "$GLOBAL", 6) {
            let name = match mir2_shared::hutil32::arrest_string_ex_ref(s_variable, '(', ')').0 {
                mir2_shared::hutil32::ArrestWrite::Value(v) => v,
                mir2_shared::hutil32::ArrestWrite::Unchanged => String::new(),
            };
            return self.get_mov_data_dynamic_var_value(DynVarKind::Global, &name);
        }
        if mir2_shared::hutil32::compare_lstr_prefix(s_variable, "$HUMAN", 6)
            || mir2_shared::hutil32::compare_lstr_prefix(s_variable, "$ACCOUNT", 8)
        {
            // 注意：C# 原文 `$ACCOUNT` 亦读 `playerActor.DynamicVarMap`（非 Account 表）
            let name = match mir2_shared::hutil32::arrest_string_ex_ref(s_variable, '(', ')').0 {
                mir2_shared::hutil32::ArrestWrite::Value(v) => v,
                mir2_shared::hutil32::ArrestWrite::Unchanged => String::new(),
            };
            return self.get_mov_data_dynamic_var_value(DynVarKind::Human, &name);
        }
        None
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
                if self.abort_goto {
                    // C# 异常从 GotoLable 逃出：本动作列表的后续动作也不再执行。
                    return false;
                }
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
pub mod quest_state;
pub mod vars;

pub use vars::VarLookup;
pub use vars::{DynVar, DynVarKind, VarVal};
