//! GM `@` 命令面（oracle `src/Modules/GameCommand/**`）：**第一批 = 表驱动派发 + 现网可达 6 条**。
//!
//! 设计件：`mir2-rs/tests/parity/m3/design-gm-commands-20261023.md`（§1 分发链 / §2 可达性 / §4.2 判据 /
//! §6 批次 G1）。G1 落地派发链 + 5 条命令体；批次 2/3 再落 48 条（见下）。剩余 132 条留在
//! `Imp::Unimplemented`，等 G4-G6。
//!
//! ## 分发链（逐行照 oracle）
//!
//! `CM_SAY(3030)` ⇒ `PlayObject.Chat.cs:363-400`：`sData.StartsWith("@")` 命中 ⇒
//! `M2Share.CommandSystem.Execute(this, sData)`（`CommandSystem.cs:125-196`）：
//!
//! 1. [`parse_command`] = `ExtractCommandAndParameters`（`CommandSystem.cs:228-250`）：`Trim()` ⇒ 去首
//!    `@` ⇒ 按**单个 ASCII 空格**切首 token、参数取首个空格之后再 `Trim()`。
//! 2. `CommandMaps`（`OrdinalIgnoreCase`，`CommandSystem.cs:16`）查找 ⇒ [`COMMANDS`] 线性扫。
//! 3. 命中 ⇒ `GameCommand.Handle`（`GameCommand.cs:28-58`）：`PlayerActor.Permission < PermissionMin`
//!    ⇒ 返回 `权限不够!!!`；否则按 `Execute` 形参个数三选一派发。
//! 4. 未命中 ⇒ **先**发一条 `SysMsg("未知命令: {command} {parameters}", Red, Hint)`，`Execute` 返回
//!    `false` ⇒ `PlayObject.Chat.cs:400` 再补第二条（见 [`incorrect_msg`]）。
//! 5. `Execute` 返回 `true`（命中，含返回空串）⇒ 整条 `CM_SAY` 结束，**不**走回落。
//!
//! ## 命令表口径（`Command.conf` 覆盖 + `[RegisterCommand]`）
//!
//! 名字/权限不是代码里的 `[Command]` 名，而是三层叠加（`CommandSystem.cs:39-113`）：
//! `GameCmdConf.LoadConfig`（`GameCmdConf.cs`）按固定 key 读 `Command.conf` 的 `[Command]`/`[Permission]`
//! 覆盖 `GameCommands` 字段的构造默认值；`RegisterCustomCommand` 只收带 `[RegisterCommand]` 的字段，
//! 键 = 对应类的 `[Command]` 名；`RegisterCommandGroups` 再把该值覆盖回类属性（**名字与 PermissionMin**）。
//! 未登记的 25 条直接用类属性（L1）。
//!
//! [`COMMANDS`] 是**按部署 `M2GameSvr/Command.conf` 算好的常量表**（185 行 = `Commands/` 下 185 个
//! 命令类，不含内建 `help`/`commands`）。生成口径与逐行对照见
//! `mir2-rs/tests/parity/m3/design-gm-commands-20261023.md` §4.3；`@sadasd`/`@df56dfs`/`@天地合一`
//! 这类混淆名与 `Level0`/`ChangeMasterName` 等「配置缺键回落构造默认名」都已按 oracle 解析结果落表
//! （配置死键照抄，见设计件 §3.2/§3.3 —— **不修**，属内容冻结）。
//!
//! ## 现网可达性（设计件 §2）
//!
//! `Permission` 的唯一生产来源 `GetHumPermission` 被注释（`PlayObject.cs:1188-1194`）、`AdminList.txt`
//! 缺 IP 段被丢弃、`StartPermission=0`、部署是 Release ⇒ **现网 `Permission` 恒 0**。185 条里 perm 0
//! 恰好 5 条，加内建 `@help` 共 6 条可达；其余 180 条一律停在 `权限不够!!!`（Red 255/56）。
//! [`GmSession::permission`] 因此保持 0（设计件 D-1 建议 (a)：1:1 复刻这个「锁死」）。
//!
//! **脚本可达（`GMEXECUTE`）**：脚本动作 `ActionOfGmExecute` 先把 `Permission` 临时置 10、
//! 执行 `ProcessUserLineMsg`、再 `finally` 还原（`ExecutionProcessingSys.cs:4938-4947`，抬权
//! 唯一地址 = `crates/script/src/engine/actions.rs` 的 `PermissionGuard`）。宿主跳线由
//! `npc::NpcPlayer` 覆写 `set_permission`/`process_user_line_msg` 接上（卡
//! `openmir2-gm-script-host-seam`，T-210）⇒ `0 < PermissionMin ≤ 10` 的命令**经脚本**可达
//! （`coverage/gm_registry.py` 的第 4 态 `script-reachable`）；客户端直呼仍恒 0。
//!
//! ## 批次 2/3（本模块第二批，tier=L）
//!
//! 落地设计件 §4.4 的第 4 族（重载/管理 14 条）、第 5 族（封禁名单 9 条）、第 11 族（桩/测试 8 条，
//! 其中 `@AddRobotPlay` 已在 G1）。三族共 31 条，除 `@AddRobotPlay` 外全是 `PermissionMin = 10`
//! ⇒ **客户端直呼**恒停在 `权限不够!!!`（脚本 `GMEXECUTE` 抬权后可达，见上「脚本可达」）；
//! 用例里直接构造 `permission = 10` 进命令体。
//! **oracle 注释掉的逻辑照抄 no-op**（设计件 §4.5 #7）；宿主未接项逐条 `log_note` 并登记
//! `mir2-rs/tests/parity/whitelist.md` T-161..T-164。
//!
//! ## 批次 3（本模块第三批，tier=R）：传送/召唤族 17 条 + `@传送`
//!
//! 落地设计件 §4.3 第 11–27 行（`RecallHuman`/`ReGotoHuman`/`Ting`/`SuperTing`/`PositionMove`/
//! `MapMove`/`MapMoveHuman`/`MoveMobTo`/`RecallMob`/`SignMapMove`/`GroupRecall`/`GuildRecall`/
//! `MasterRecall`/`DearRecall`/`AllowDearRecall`/`AllowGuildRecall`/`AllowMasterRecall`）
//! 与设计件 §6 G3 点名的 `UserMoveXY`（现网名 `传送`；§4.3 归「其它」族，本批一并落）。
//! 共 18 条，全部 `PermissionMin = 10` ⇒ 现网 `Permission` 恒 0，生产路径一律 `权限不够!!!`。
//!
//! **移动/RNG 一律不伪造**（设计件 §4.4 第 2 族 + 本卡口径）：宿主能力**已经存在**
//! （`World::base_object_move_player`/`cast_space_move_spell_with`、`space_move_get_rand_xy`、
//! `guardlist::regen_monster_by_name`、`magic::front_position`），但 GM 派发只有 `&GmWorld`
//! 内的 `&World` 只读视图，**没有 `&mut World` 通道** ⇒ 命令体只保留 oracle 会发的 `SysMsg`
//! （这些支多数本来就是**零文本**），移动与抽数副作用逐条 `log_note` 并登记 `whitelist.md`
//! T-166..T-171。地图存在性/`MapDesc`/`Flag`、在线目标、坐标、行会掌门、攻城区域走
//! [`GmWorld`] 的 `maps`/`world` 只读视图（宿主已有能力，不另造）。

use chrono::Local;

/// oracle `CommandHelp.GameCommandPermissionTooLow`（`CommandHelp.cs`）。
pub const PERMISSION_TOO_LOW: &str = "权限不够!!!";
/// oracle `CommandHelp.NowCurrDateTime`（`CommandHelp.cs`）。
pub const NOW_CURR_DATETIME: &str = "当前日期时间: ";
/// `HelpCommandGroup.Fallback`（`CommandSystem.cs:281-307`）。
pub const HELP_USAGE: &str = "usage: help <command>";
/// `PrvMsgCommand` 的 `Command.CommandHelp` = `命令格式: @{Name} {Help}`（`CommandAttribute.cs:67`，
/// `Help = CommandHelp.GameCommandPrvMsgHelpMsg = "人物名称"`，`Name` 已被配置覆盖为 `拒绝`）。
pub const PRV_MSG_HELP: &str = "命令格式: @拒绝 人物名称";
/// `CreateRobotPlayCommand` 的 `Command.CommandHelp`（未登记 ⇒ `Name = AddRobotPlay`，
/// `Help = "数量 地图 X Y"`）。
pub const ADD_ROBOT_HELP: &str = "命令格式: @AddRobotPlay 数量 地图 X Y";

// ===== 批次 2/3 的 `Command.CommandHelp` 常量 =====
//
// 口径：`命令格式: @{Name} {Help}`（`CommandAttribute.cs` 的 `CommandHelp`），`Name` 已被
// `command.conf` 覆盖成**现网拼写**（`CommandSystem.cs:100-107`），`Help` 取类属性原文。
/// `DelDenyAccountLogonCommand`：`[Command("DelDenyAccountLogon", "", "登录帐号", 10)]`，现网名 `-jid`。
pub const DEL_DENY_ACCOUNT_HELP: &str = "命令格式: @-jid 登录帐号";
/// `DelDenyChrNameLogonCommand`：Help `人物名称`，现网名 `DelDenyChrNameLogon`。
pub const DEL_DENY_CHRNAME_HELP: &str = "命令格式: @DelDenyChrNameLogon 人物名称";
/// `DelDenyIPaddrLogonCommand`：Help `IP地址`，现网名 `-jip`。
pub const DEL_DENY_IP_HELP: &str = "命令格式: @-jip IP地址";
/// `DenyAccountLogonCommand`：Help `登录帐号 是否永久封(0,1)`，现网名 `+jid`。
pub const DENY_ACCOUNT_HELP: &str = "命令格式: @+jid 登录帐号 是否永久封(0,1)";
/// `DenyChrNameLogonCommand`：Help `人物名称 是否永久封(0,1)`，现网名 `DenyChrNameLogon`。
pub const DENY_CHRNAME_HELP: &str = "命令格式: @DenyChrNameLogon 人物名称 是否永久封(0,1)";
/// `DenyIPaddrLogonCommand`：Help `IP地址 是否永久封(0,1)`，现网名 `+jip`。
pub const DENY_IP_HELP: &str = "命令格式: @+jip IP地址 是否永久封(0,1)";
/// `ReloadGuildCommand`：`string.Format(GameCommandParamUnKnow, Name, "行会名称")`。
pub const RELOAD_GUILD_HELP: &str = "命令格式: @ReloadGuild 行会名称";
/// `FireBurnCommand`：`[Command("FireBurn", "", 10)]` ⇒ `Help` 为 null ⇒ 尾随一个空格。
pub const FIRE_BURN_HELP: &str = "命令格式: @FireBurn ";

/// `MobFireBurnCommand` 的帮助文本模板：`CommandHelp.GameCommandMobFireBurnHelpMsg` =
/// `"命令格式: {0} {1} {2} {3} {4} {5} {6}"`（`CommandHelp.cs:104`），实参 = `Command.Name`
/// + 六个原始串。注意**没有 `@` 前缀**、分隔符是**单个**空格，空槽位 ⇒ 连续/尾随空格照原样。
pub const MOB_FIRE_BURN_NAME: &str = "MobFireBurn";

// ===== 批次 3（传送/召唤族）的 `Command.CommandHelp` 与 SysMsg 文本常量 =====
//
// 口径：`命令格式: @{Name} {Help}`（`CommandAttribute.cs:53`），`Name` 已被 `Command.conf`
// 覆盖成现网拼写（`CommandSystem.cs:100-107`）；其余文本取 `CommandHelp` 原文，逐字节照抄。
/// `MapMoveCommand`：`Help = CommandHelp.GameCommandMoveHelpMsg = "地图号"`，现网名 `f`。
pub const MAP_MOVE_HELP: &str = "命令格式: @f 地图号";
/// `MapMoveHumanCommand`：`Help = "源地图  目标地图"`（**两个空格**），现网名 `集体传送`。
pub const MAP_MOVE_HUMAN_HELP: &str = "命令格式: @集体传送 源地图  目标地图";
/// `PositionMoveCommand`：`Help = "地图号 座标X 座标Y"`，现网名 `ff`。
pub const POSITION_MOVE_HELP: &str = "命令格式: @ff 地图号 座标X 座标Y";
/// `RecallHumanCommand`：`Help = CommandHelp.GameCommandPrvMsgHelpMsg = "人物名称"`（未登记 ⇒ 英文名）。
pub const RECALL_HUMAN_HELP: &str = "命令格式: @RecallHuman 人物名称";
/// `ReGotoHumanCommand`：`Help = "人物名称"`（未登记 ⇒ 英文名）。
pub const REGOTO_HUMAN_HELP: &str = "命令格式: @ReGotoHuman 人物名称";
/// `TingCommand`：`Help = "人物名称"`，现网名 `Tingwa`。
pub const TING_HELP: &str = "命令格式: @Tingwa 人物名称";
/// `SuperTingCommand`：`Help = "人物名称 范围(0-10)"`，现网名 `SuperTingwa`。
pub const SUPER_TING_HELP: &str = "命令格式: @SuperTingwa 人物名称 范围(0-10)";
/// `RecallMobCommand`：`Help = "怪物名称 数量 等级(0-7) 叛变时间(分钟) 是否自动变色（0、1）固定颜色（1-7）"`，现网名 `bb`。
pub const RECALL_MOB_HELP: &str =
    "命令格式: @bb 怪物名称 数量 等级(0-7) 叛变时间(分钟) 是否自动变色（0、1）固定颜色（1-7）";
/// `MoveMobToCommand`：`Help = "怪物名称 原地图 原X 原Y 新地图 新X 新Y"`（未登记 ⇒ 英文名）。
pub const MOVE_MOB_TO_HELP: &str = "命令格式: @MoveMobTo 怪物名称 原地图 原X 原Y 新地图 新X 新Y";
/// `CommandHelp.EnableDearRecall`。
pub const ENABLE_DEAR_RECALL: &str = "允许夫妻传送!!!";
/// `CommandHelp.DisableDearRecall`。
pub const DISABLE_DEAR_RECALL: &str = "禁止夫妻传送!!!";
/// `CommandHelp.EnableGuildRecall`。
pub const ENABLE_GUILD_RECALL: &str = "[允许行会合一]";
/// `CommandHelp.DisableGuildRecall`。
pub const DISABLE_GUILD_RECALL: &str = "[禁止行会合一]";
/// `CommandHelp.EnableMasterRecall`。
pub const ENABLE_MASTER_RECALL: &str = "允许师徒传送!!!";
/// `CommandHelp.DisableMasterRecall`。
pub const DISABLE_MASTER_RECALL: &str = "禁止师徒传送!!!";

// ===== 族 T · 会话开关/骑马（卡 `openmir2-gm-family-t-toggle-horse`，tier=L）的 SysMsg 正文 =====
//
// 文本取 `CommandHelp.cs:114-121` 与两个命令体的**内联字面量**（DisableFilterCommand.cs:15/19、
// TakeOnHorseCommand.cs:15），逐字节照抄——注意两组 Ban 族的常量名与语义是**反的**：
// `if (BanShout) SysMsg(EnableShoutMsg)` ⇒ `BanShout=true`（拒聊位开）显示的却是「允许群聊」，
// oracle 就这么写，1:1 照抄、不「修」。
/// `CommandHelp.EnableDealMsg`（`CommandHelp.cs:116`；`AllowDeal=true` 时发）。
pub const ENABLE_DEAL_MSG: &str = "[允许交易]";
/// `CommandHelp.DisableDealMsg`（`CommandHelp.cs:117`；`AllowDeal=false` 时发）。
pub const DISABLE_DEAL_MSG: &str = "[禁止交易]";
/// `CommandHelp.EnableShoutMsg`（`CommandHelp.cs:114`；`BanShout=true` 时发——常量名与状态语义相反，照抄）。
pub const ENABLE_SHOUT_MSG: &str = "[允许群聊]";
/// `CommandHelp.DisableShoutMsg`（`CommandHelp.cs:115`；`BanShout=false` 时发）。
pub const DISABLE_SHOUT_MSG: &str = "[禁止群聊]";
/// `CommandHelp.EnableGuildChat`（`CommandHelp.cs:118`；`BanGuildChat=true` 时发——同上相反，照抄）。
pub const ENABLE_GUILD_CHAT: &str = "[允许行会聊天]";
/// `CommandHelp.DisableGuildChat`（`CommandHelp.cs:119`；`BanGuildChat=false` 时发）。
pub const DISABLE_GUILD_CHAT: &str = "[禁止行会聊天]";
/// `CommandHelp.EnableJoinGuild`（`CommandHelp.cs:120`；`AllowGuild=true` 时发）。
pub const ENABLE_JOIN_GUILD: &str = "[允许加入行会]";
/// `CommandHelp.DisableJoinGuild`（`CommandHelp.cs:121`；`AllowGuild=false` 时发）。
pub const DISABLE_JOIN_GUILD: &str = "[禁止加入行会]";
/// `DisableFilterCommand.cs:15` 内联字面量（`SystemShare.FilterWord` 翻为 true 时发）。
pub const DISABLE_FILTER_ENABLED: &str = "已启用文字过滤。";
/// `DisableFilterCommand.cs:19` 内联字面量（翻为 false 时发）。部署启动 `GameApp.cs:70` 置 true
/// ⇒ 现网第一条回「已禁止文字过滤。」。
pub const DISABLE_FILTER_DISABLED: &str = "已禁止文字过滤。";
/// `TakeOnHorseCommand.cs:15` 内联字面量（`HorseType == 0` 的红字）。oracle `HorseType`
/// 全仓**无赋值点**（`feature.rs` 模块文档 §4 实证）⇒ 现网恒 0 ⇒ 本红字是**唯一可达支**。
pub const ON_HORSE_NEED_TAG: &str = "骑马必须先戴上马牌!!!";

// ===== 族 Q · 查询/展示（卡 `openmir2-gm-family-q-query-display`，tier=L）的 SysMsg 正文 =====
//
// 文本取 `CommandHelp.cs` / 命令体内联字面量，逐字节照抄。`{n}` 是 .NET `string.Format`
// 占位——输出由 [`dotnet_format`] 单遍替换（值原样插入，不递归展开）。
/// `CommandHelp.NowNotOnLineOrOnOtherServer`（`CommandHelp.cs:18`；离线红字）。
pub const NOW_NOT_ON_LINE_FMT: &str = "{0} 现在不在线，或在其它服务器上!!!";
/// `CommandHelp.GameCommandMobCountMapNotFound`（`CommandHelp.cs:20`）。
pub const MOB_COUNT_MAP_NOT_FOUND: &str = "指定的地图不存在!!!";
/// `CommandHelp.GameCommandMobCountMonsterCount`（`CommandHelp.cs:21`；**全角冒号**；
/// HumanCount 复用它计玩家数——oracle 就这么写）。
pub const MOB_COUNT_MONSTER_COUNT_FMT: &str = "怪物数量：{0}";
/// `HumanCountCommand` 的 `Command.CommandHelp`（`[Command]` Help=`地图号`；现网名 `HumanCountwa`）。
pub const Q_HELP_HUMAN_COUNT: &str = "命令格式: @HumanCountwa 地图号";
/// `MobCountCommand` 的 `Command.CommandHelp`（Help=`地图号`；现网名 `MobCountwa`）。
pub const Q_HELP_MOB_COUNT: &str = "命令格式: @MobCountwa 地图号";
/// `ShowMapModeCommand` 的 `Command.CommandHelp`（Help=`地图号`；现网名 `ShowMapMode`）。
pub const Q_HELP_SHOW_MAP_MODE: &str = "命令格式: @ShowMapMode 地图号";
/// `ShowHumanFlagCommand` 的 `Command.CommandHelp`（Help=`人物名称 标识号`；现网名 `showflag`）。
pub const Q_HELP_SHOW_FLAG: &str = "命令格式: @showflag 人物名称 标识号";
/// `HumanLocalCommand` 的 `Command.CommandHelp`（Help=`人物名称`；现网名 `IP`）。
pub const Q_HELP_HUMAN_LOCAL: &str = "命令格式: @IP 人物名称";
/// `HumanInfoCommand` 的 `Command.CommandHelp`（Help=`人物名称`——与 HumanLocal 同一条 Help
/// 常量 `GameCommandHumanLocalHelpMsg`；现网名 `信息`）。
pub const Q_HELP_HUMAN_INFO: &str = "命令格式: @信息 人物名称";
/// `ShowUseItemInfoCommand` 的 `Command.CommandHelp`（Help=`人物名称`；现网名 `Showuseiteminfo`）。
pub const Q_HELP_SHOW_USE_ITEM: &str = "命令格式: @Showuseiteminfo 人物名称";
/// `CommandHelp.GameCommandMapInfoMsg`（`CommandHelp.cs:39`）。
pub const Q_MAP_INFO_NAME_FMT: &str = "地图名称: {0}({1})";
/// `CommandHelp.GameCommandMapInfoSizeMsg`（`CommandHelp.cs:40`）。
pub const Q_MAP_INFO_SIZE_FMT: &str = "地图大小: X({0}) Y({1})";
/// `CommandHelp.GameCommandShowHumanFlagONMsg`（`CommandHelp.cs:52`）。
pub const Q_FLAG_ON_FMT: &str = "{0}: [{1}] = ON";
/// `CommandHelp.GameCommandShowHumanFlagOFFMsg`（`CommandHelp.cs:53`）。
pub const Q_FLAG_OFF_FMT: &str = "{0}: [{1}] = OFF";
/// `CommandHelp.GameCommandLotteryTicketMsg`（`CommandHelp.cs:65`；**尾随一个空格**）。
pub const LOTTERY_MSG_FMT: &str =
    "已中彩票数:{0} 未中彩票数:{1} 一等奖:{2} 二等奖:{3} 三等奖:{4} 四等奖:{5} 五等奖:{6} 六等奖:{7} ";
/// `ShowUseItemInfoCommand.cs:33` 的格式串字面量——.NET `string.Format` 不认识 `%s/%d`
/// ⇒ **原样输出、逐件相同**（内容冻结怪癖，不许「修」）。
pub const Q_SHOW_USE_ITEM_LITERAL: &str = "%s[%s]IDX[%d]系列号[%d]持久[%d-%d]";

// ===== 族 X1 · 禁言名单（卡 `openmir2-gm-family-x1-denywhisper`，tier=L）的 SysMsg 正文 =====
//
// 文本取 `CommandHelp.cs` / 命令体内联字面量，逐字节照抄。注意（review F1）：触表的四个
// 命令体运行期 NRE ⇒ 零帧，源文本恒字/绿字**真机不可达**——以下常量里可达面仅
// +jy/-jy 两条红帮助与 ViewWhisper 两帧（见族 section 注）。
/// `DisableSendMsgCommand` 的 `Command.CommandHelp`（Help=`人物名称`；现网名 `+jy`）。
pub const X1_HELP_DISABLE_SEND_MSG: &str = "命令格式: @+jy 人物名称";
/// `EnableSendMsgCommand` 的 `Command.CommandHelp`（Help=`人物名称`；现网名 `-jy`）。
pub const X1_HELP_ENABLE_SEND_MSG: &str = "命令格式: @-jy 人物名称";
/// `ViewWhisperCommand` 的 `Command.CommandHelp`——Help=`GameCommandViewWhisperHelpMsg`
/// = `" 人物名称"`（**带前导空格**，`CommandHelp.cs:91`）⇒ 拼出**双空格**
/// `@查看私聊  人物名称`（`CommandAttribute.cs:53` 的 `$"@{Name} {Help}"`）。
pub const X1_HELP_VIEW_WHISPER: &str = "命令格式: @查看私聊  人物名称";
/// `CommandHelp.GameCommandViewWhisperMsg1`（`CommandHelp.cs:92`；toggle 回 Msg1）。
pub const X1_VIEW_WHISPER_STOP_FMT: &str = "已停止侦听{0}的私聊信息...";
/// `CommandHelp.GameCommandViewWhisperMsg2`（`CommandHelp.cs:93`；toggle 开 Msg2）。
pub const X1_VIEW_WHISPER_START_FMT: &str = "正在侦听{0}的私聊信息...";

// ===== 批次 4（数值/角色族，设计件 §4.3 第 72–115 行，44 条）的 CommandHelp 常量 =====
//
// 口径同上：`命令格式: @{现网名} {Help}`（`CommandAttribute.cs:53`）；`Help` 为空时**末尾空格保留**
// （与 `FIRE_BURN_HELP` 同形）。现网名取 `Command.conf` 覆盖后的值（设计件 §4.3「现网名」列）。
pub const ADD_GAME_POINT_HELP: &str = "命令格式: @AddGamePoint 人物名称  金币数量";
pub const ADD_GOLD_HELP: &str = "命令格式: @加钱 人物名称  金币数量";
pub const ADJUEST_EXP_HELP: &str = "命令格式: @调整经验 物名称 经验值";
pub const ADJUEST_LEVEL_HELP: &str = "命令格式: @1级 人物名称 等级";
pub const ADJUST_EXP_HELP: &str = "命令格式: @AdjustExp ";
pub const BIND_USE_ITEM_HELP: &str = "命令格式: @Binduseitem 人物名称 物品类型 绑定方法";
/// `CommandHelp.GameCommandBindUseItemNoItemMsg`（`CommandHelp.cs:102`）：在线目标该佩戴位
/// 空槽（`UserItem.Index == 0`）的红字（`BindUseItemCommand.cs:53-59`）。
pub const BIND_NO_ITEM_FMT: &str = "{0}的{1}没有戴物品!!!";
pub const BONU_POINT_HELP: &str = "命令格式: @调整属性 人物名称 属性点数(不输入为查看点数)";
pub const CHANGE_ADMIN_MODE_HELP: &str = "命令格式: @1 ";
pub const CHANGE_ATTACK_MODE_HELP: &str = "命令格式: @AttackMode ";
pub const CHANGE_DEAR_NAME_HELP: &str = "命令格式: @伴侣变更 人物名称 配偶名称(如果为 无 则清除)";
pub const CHANGE_GENDER_HELP: &str = "命令格式: @改变性别 人物名称 性别(男、女)";
pub const CHANGE_ITEM_NAME_HELP: &str = "命令格式: @ChangeItemName 物品编号 物品ID号 物品名称";
pub const CHANGE_JOB_HELP: &str = "命令格式: @职业变更  人物名称 职业类型(Warr Wizard Taos)";
pub const CHANGE_LEVEL_HELP: &str = "命令格式: @Level0 等级(1-65535)";
pub const CHANGE_MASTER_NAME_HELP: &str =
    "命令格式: @ChangeMasterName 人物名称 师徒名称(如果为 无 则清除)";
pub const CHANGE_OB_MODE_HELP: &str = "命令格式: @2 ";
pub const CHANGE_SALVE_STATUS_HELP: &str = "命令格式: @Rest ";
pub const CHANGE_SUPER_MAN_MODE_HELP: &str = "命令格式: @3 ";
pub const CHANGE_USER_FULL_HELP: &str = "命令格式: @CHGUSERFULL 人数";
pub const CHANGE_ZEN_FAST_STEP_HELP: &str = "命令格式: @CHGZENFASTSTEP 速度";
pub const CREDIT_POINT_HELP: &str = "命令格式: @调整声望 人物名称 控制符(+,-,=) 声望点数(0-255)";
pub const DEL_BONU_POINT_HELP: &str = "命令格式: @DelBonusPoint 人物名称";
pub const DELETE_ITEM_HELP: &str = "命令格式: @删除物品 人物名称 物品名称 数量";
pub const DEL_GAME_GOLD_HELP: &str = "命令格式: @DelGamePoint 人物名称 数量";
pub const DEL_GOLD_HELP: &str = "命令格式: @减钱 人物名称 数量";
pub const FREE_PENALTY_HELP: &str = "命令格式: @清除PK 人物名称";
pub const GAME_DIAMOND_HELP: &str = "命令格式: @GameDiaMond ";
pub const GAME_GIRD_HELP: &str = "命令格式: @GameGird ";
pub const GAME_GLORY_HELP: &str = "命令格式: @GameGlory ";
pub const GAME_GOLD_HELP: &str =
    "命令格式: @VV调整元宝  人物名称 控制符(+,-,=) 游戏币(1-200000000)";
pub const GAME_POINT_HELP: &str =
    "命令格式: @调整泡点 人物名称 控制符(+,-,=) 游戏点数(1-100000000)";
pub const GET_USER_ITEMS_HELP: &str = "命令格式: @GetUserItems 人物名称 物品名称 数量 类型(0,1,2)";
pub const HAIR_HELP: &str = "命令格式: @改变发型 人物名称 类型值";
pub const HUNGER_HELP: &str = "命令格式: @Hunger 人物名称 能量值";
pub const INC_PK_POINT_HELP: &str = "命令格式: @增加PK 人物名称 PK点数";
pub const LUCK_POINT_HELP: &str = "命令格式: @LuckyPoint 人物名称 控制符 幸运点数";
pub const PK_POINT_HELP: &str = "命令格式: @查看PK 人物名称";
pub const REFINE_WEAPON_HELP: &str = "命令格式: @RefineWeaponwa 攻击力 魔法力 道术 准确度";
pub const RE_NEW_LEVEL_HELP: &str = "命令格式: @转生 人物名称 点数(为空则查看)";
pub const REST_BONU_POINT_HELP: &str = "命令格式: @RestBonuPoint 人物名称";
pub const TRAINING_HELP: &str = "命令格式: @+jn ";
pub const TRAINING_MAGIC_HELP: &str = "命令格式: @TrainingMagic 人物名称  技能名称 修炼等级(0-3)";
pub const TRAINING_SKILL_HELP: &str = "命令格式: @增加技能 人物名称  技能名称 修炼等级(0-3)";
pub const UNBIND_USE_ITEM_HELP: &str = "命令格式: @UnBindUseItem ";

// ---- 批次 5 · 脚本可达族（卡 `openmir2-gm-impl-*`；`GMEXECUTE` 抬权可达，tier=R）----
/// `ShutupCommand` 的参数校验回执：`string.Format(CommandHelp.GameCommandParamUnKnow,
/// Command.Name, CommandHelp.GameCommandShutupHelpMsg)`。
///
/// `CommandHelp.cs:28` 的 `GameCommandShutupHelpMsg = "人物名称  时间长度(分钟)"`（**两个**空格，
/// GBK 线上编码时同样保留两个 0x20）；`Name` 被部署 `Command.conf` 覆盖为 `禁言`
/// （`conf[Command].Shutup=禁言`）。
pub const SHUTUP_HELP: &str = "命令格式: @禁言 人物名称  时间长度(分钟)";

/// `ShutupReleaseCommand` 唯一的可观测帧：`Command.CommandHelp`（`CommandAttribute.cs:56-58`）
/// = `命令格式: @{Name} {Help}`。`Name` 被部署 `Command.conf` 的
/// `[Command] ReleaseShutup=取消禁言` 覆盖，`Help` = `CommandHelp.GameCommandShutupReleaseHelpMsg`
/// = 「人物名称」（`CommandHelp.cs:41`）。
pub const SHUTUP_RELEASE_HELP: &str = "命令格式: @取消禁言 人物名称";

/// `ShutupReleaseCommand` 的 `bool.Parse` 失败路径（被 `PlayObject.Chat.cs:402-406` 吞掉 ⇒ 零帧）。
const NOTE_SHUTUP_RELEASE_BOOL_PARSE: &str =
    "oracle `bool.Parse(sParam2)` FormatException ⇒ 被 PlayObject.Chat.cs 的 catch 吞掉 ⇒ 零帧";

// ---- 批次 4 的 SysMsg 正文（`MessageSettings` / `CommandHelp` 原文；`String.ini` 未随部署提供 ⇒ 取
// ---- `SystemModule/MessageSettings.cs` 的构造默认值，见 T-191）----
pub const USER_FULL_TITLE: &str = "设置服务器最高上线人数。";
pub const ZEN_FAST_STEP_TITLE: &str = "设置怪物行动速度。";
pub const ITEM_NAME_OCCUPIED: &str = "此物品，已经设置了其它的名称!!!";
pub const ADMIN_MODE_ON: &str = "已进入管理员模式";
pub const ADMIN_MODE_OFF: &str = "已退出管理员模式";
pub const OBSERVER_MODE_ON: &str = "已进入隐身模式";
pub const OBSERVER_MODE_OFF: &str = "已退出隐身模式";
pub const SUPERMAN_MODE_ON: &str = "已进入无敌模式";
pub const SUPERMAN_MODE_OFF: &str = "已退出无敌模式";
pub const PET_REST: &str = "下属：休息";
pub const PET_ATTACK: &str = "下属：攻击";
pub const ATTACK_MODE_ALL: &str = "[攻击模式: 全体攻击]";
pub const ATTACK_MODE_PEACEFUL: &str = "[攻击模式: 和平攻击]";
pub const ATTACK_MODE_DEAR: &str = "[攻击模式: 夫妻攻击]";
pub const ATTACK_MODE_MASTER: &str = "[攻击模式: 师徒攻击]";
pub const ATTACK_MODE_GROUP: &str = "[攻击模式: 编组攻击]";
pub const ATTACK_MODE_GUILD: &str = "[攻击模式: 行会攻击]";
pub const ATTACK_MODE_RED_WHITE: &str = "[攻击模式: 红名攻击]";

// ---- 族 P · 密码锁门（卡 `openmir2-gm-family-p-passwordlock`，tier=L）的 SysMsg 正文 ----
/// `MessageSettings.NoPasswordLockSystemMsg`（`MessageSettings.cs:99` 构造缺省；
/// 部署 `String.conf:75` 覆盖为**同一文本**；GBK 线上字节逐对：游`D3CE` 戏`CFB7` 密`C3DC`
/// 码`C2EB` 保`B1A3` 护`BBA4` 系`CFB5` 统`CDB3` 还`BBB9` 没`C3BB` 有`D3D0` 启`C6F4` 用`D3C3`
/// 加三个 ASCII `!`(0x21)，共 29 字节）。
/// 密码锁族 7 命令在部署 `PasswordLockSystem=0` 下的恒定红字。
pub const NO_PASSWORD_LOCK_SYSTEM_MSG: &str = "游戏密码保护系统还没有启用!!!";

/// 内建命令名（`CommandSystem.cs:255-307` 的两个嵌套类，**不属于** 185 个命令类）。
pub const BUILTIN_HELP: &str = "help";
pub const BUILTIN_COMMANDS: &str = "commands";

/// 命令体实现标签。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Imp {
    /// 内建 `HelpCommandGroup`：**重写 `Handle`、不过权限门**。
    Help,
    /// 内建 `CommandsCommandGroup`：未重写 `Handle` ⇒ 反射 `TargetParameterCountException` ⇒ 零帧。
    Commands,
    /// `DataCommand`（现网 `@时间`）。
    Date,
    /// `PrvMsgCommand`（现网 `@拒绝`）。
    PrvMsg,
    /// `AllowMsgCommand`（现网 `@拒绝私聊`）。
    AllowMsg,
    /// `LockLoginCommand`（现网 `@LockLogin`）。
    LockLogin,
    /// `CreateRobotPlayCommand`（现网 `@AddRobotPlay`；加机器人代码整段被注释 ⇒ 只留 `?` 帮助）。
    AddRobotPlay,

    // ---- 批次 2/3 · 重载/管理（14，设计件 §4.4 第 4 族）----
    /// `LoadAdminCommand`（`@LoadAdmin`）。
    LoadAdmin,
    /// `NpcScriptCommand`（`@NpcScript`）。
    NpcScript,
    /// `ReloadAbuseCommand`（`@ReloadAbuse`；空体）。
    ReloadAbuse,
    /// `ReLoadAdminCommand`（`@重读GM`）。
    ReLoadAdmin,
    /// `ReloadAllGuildCommand`（`@ReloadGuildAll`）。
    ReloadAllGuild,
    /// `ReloadGameItemCommand`（`@ReloadItemDB`）。
    ReloadItemDb,
    /// `ReloadGuildCommand`（`@ReloadGuild`）。
    ReloadGuild,
    /// `ReloadLineNoticeCommand`（`@重读公告`）。
    ReloadLineNotice,
    /// `ReloadMagicDbCommand`（`@ReloadMagicDB`）。
    ReloadMagicDb,
    /// `ReloadManageCommand`（`@重读登陆`）。
    ReloadManage,
    /// `ReloadMonItemsCommand`（`@重读爆率`）。
    ReloadMonItems,
    /// `ReloadNpcCommand`（`@重读商人`；体整段被注释）。
    ReloadNpc,
    /// `ReloadRobotCommand`（`@重读机器人脚本`）。
    ReloadRobot,
    /// `ReloadRobotManageCommand`（`@重读机器人配置`）。
    ReloadRobotManage,

    // ---- 批次 2/3 · 封禁名单（9，设计件 §4.4 第 5 族）----
    /// `DelDenyAccountLogonCommand`（`@-jid`）。
    DelDenyAccount,
    /// `DelDenyChrNameLogonCommand`（`@DelDenyChrNameLogon`）。
    DelDenyChrName,
    /// `DelDenyIPaddrLogonCommand`（`@-jip`）。
    DelDenyIp,
    /// `DenyAccountLogonCommand`（`@+jid`）。
    DenyAccount,
    /// `DenyChrNameLogonCommand`（`@DenyChrNameLogon`）。
    DenyChrName,
    /// `DenyIPaddrLogonCommand`（`@+jip`；含 `@params[3]` 两参越界怪癖）。
    DenyIp,
    /// `ShowDenyAccountLogonCommand`（`@+-jid`）。
    ShowDenyAccount,
    /// `ShowDenyChrNameLogonCommand`（`@ShowDenyChrNameLogon`；注意文件名是 Char、**命令名是 Chr**）。
    ShowDenyChrName,
    /// `ShowDenyIPaddrLogonCommand`（`@+-jip`）。
    ShowDenyIp,

    // ---- 批次 2/3 · 桩/测试（7 新增，设计件 §4.4 第 11 族；`@AddRobotPlay` 见上）----
    /// `TestFireCommand`（`@TestFire`；只往 `EventMgr` 入事件）。
    TestFire,
    /// `TestGetBagItemsCommand`（`@TestGetBagItems`）。
    TestGetBagItems,
    /// `TestSpeedModeCommand`（`@TestSpeedMode`）。
    TestSpeedMode,
    /// `TestStatusCommand`（`@TestStatus`）。
    TestStatus,
    /// `UserCmdCommand`（`@UserCmd`；体整段被注释）。
    UserCmd,
    /// `BackStepCommand`（`@Backstep`）。
    BackStep,
    /// `FireBurnCommand`（`@FireBurn`）。
    FireBurn,

    /// `StartQuestCommand`（现网 `@开始提问`）：`SendQuestMsg` 被注释 ⇒ 非空名零帧；
    /// 空名 ⇒ 红字帮助。
    StartQuest,
    /// `MobFireBurnCommand`（现网 `@MobFireBurn`）：六参校验 + `MapMgr.FindMap`；
    /// 地图命中 ⇒ `FireBurnEvent` 入 `EventMgr`（宿主未接 ⇒ 零帧）；未命中 ⇒ 红字。
    MobFireBurn,
    /// `SmakeItemCommand`（现网 `@改属性`）：原地改佩戴位属性（`Desc[n]` / `Dura` / `DuraMax`）+
    /// `RecalcAbilitys()` + `SendUpdateItem()`；空槽 ⇒ 一条红字。
    SmakeItem,
    /// `ClearBagItemCommand`（现网 `@清理垃圾`）：清空目标 `ItemList` 并给**目标**发
    /// `SM_DELITEMS(709)`（`名/makeindex/` 串，`Series` = 件数）；空列表 ⇒ 零帧。
    ClearBagItem,

    // ---- 批次 3 · 传送/召唤（17 + `@传送`，设计件 §4.3 第 11–27 行 / §6 G3）----
    /// `AllowDearRecallCommand`（现网 `@dfsgf567`）。
    AllowDearRecall,
    /// `AllowGuildRecallCommand`（现网 `@sadasd`；类名与出厂名都是小写 `Allowguildrecall`）。
    AllowGuildRecall,
    /// `AllowMasterRecallCommand`（现网 `@fdgh56j`）。
    AllowMasterRecall,
    /// `DearRecallCommond`（现网 `@dsfsdt546`；类名拼写 `Commond` 照抄）。
    DearRecall,
    /// `GroupRecallCommand`（现网 `@天地合一`）。
    GroupRecall,
    /// `GuildRecallCommand`（现网 `@df56dfs`；成员遍历整段注释 ⇒ no-op）。
    GuildRecall,
    /// `MapMoveCommand`（现网 `@f`）。
    MapMove,
    /// `MapMoveHumanCommand`（现网 `@集体传送`）。
    MapMoveHuman,
    /// `MasterRecallCommand`（现网 `@jkt76`）。
    MasterRecall,
    /// `MoveMobToCommand`（现网 `@MoveMobTo`；两个移动支整段注释）。
    MoveMobTo,
    /// `PositionMoveCommand`（现网 `@ff`）。
    PositionMove,
    /// `RecallHumanCommand`（现网 `@RecallHuman`；判空用 `sHumanName[1]`，与别处 `[0]` 不一致）。
    RecallHuman,
    /// `RecallMobCommand`（现网 `@bb`）。
    RecallMob,
    /// `ReGotoHumanCommand`（现网 `@ReGotoHuman`）。
    ReGotoHuman,
    /// `SignMapMoveCommand`（现网 `@SignMapMove`；体整段注释 ⇒ no-op）。
    SignMapMove,
    /// `SuperTingCommand`（现网 `@SuperTingwa`）。
    SuperTing,
    /// `TingCommand`（现网 `@Tingwa`）。
    Ting,
    /// `UserMoveXyCommand`（现网 `@传送`；§4.3 归「其它」，§6 G3 点名本批）。
    UserMoveXy,

    // ---- 批次 4 · 数值/角色族（44，设计件 §4.3 第 72–115 行）----
    /// `AddGameGoldCommand`（现网 `@AddGamePoint`）。
    AddGameGold,
    /// `AddGoldCommand`（现网 `@加钱`）。
    AddGold,
    /// `AdjuestExpCommand`（现网 `@调整经验`）。
    AdjuestExp,
    /// `AdjuestLevelCommand`（现网 `@1级`）。
    AdjuestLevel,
    /// `AdjustExpCommand`（现网 `@AdjustExp`；体只有权限自检）。
    AdjustExp,
    /// `BindUseItemCommand`（现网 `@Binduseitem`）。
    BindUseItem,
    /// `BonuPointCommand`（现网 `@调整属性`）。
    BonuPoint,
    /// `ChangeAdminModeCommand`（现网 `@1`）。
    ChangeAdminMode,
    /// `ChangeAttackModeCommand`（现网 `@AttackMode`）。
    ChangeAttackMode,
    /// `ChangeDearNameCommand`（现网 `@伴侣变更`）。
    ChangeDearName,
    /// `ChangeGenderCommand`（现网 `@改变性别`）。
    ChangeGender,
    /// `ChangeItemNameCommand`（现网 `@ChangeItemName`）。
    ChangeItemName,
    /// `ChangeJobCommand`（现网 `@职业变更`）。
    ChangeJob,
    /// `ChangeLevelCommand`（现网 `@Level0`）。
    ChangeLevel,
    /// `ChangeMasterNameCommand`（现网 `@ChangeMasterName`：conf 无键 ⇒ 取类属性默认名）。
    ChangeMasterName,
    /// `ChangeObModeCommand`（现网 `@2`）。
    ChangeObMode,
    /// `ChangeSalveStatusCommand`（现网 `@Rest`）。
    ChangeSalveStatus,
    /// `ChangeSuperManModeCommand`（现网 `@3`）。
    ChangeSuperManMode,
    /// `ChangeUserFullCommand`（现网 `@CHGUSERFULL`）。
    ChangeUserFull,
    /// `ChangeZenFastStepCommand`（现网 `@CHGZENFASTSTEP`）。
    ChangeZenFastStep,
    /// `CreditPointCommand`（现网 `@调整声望`）。
    CreditPoint,
    /// `DelBonuPointCommand`（现网 `@DelBonusPoint`）。
    DelBonuPoint,
    /// `DeleteItemCommand`（现网 `@删除物品`）。
    DeleteItem,
    /// `DelGameGoldCommand`（现网 `@DelGamePoint`）。
    DelGameGold,
    /// `DelGoldCommand`（现网 `@减钱`）。
    DelGold,
    /// `FreePenaltyCommand`（现网 `@清除PK`）。
    FreePenalty,
    /// `GameDiaMondCommand`（现网 `@GameDiaMond`；体为空）。
    GameDiamond,
    /// `GameGirdCommand`（现网 `@GameGird`；体为空）。
    GameGird,
    /// `GameGloryCommand`（现网 `@GameGlory`；体为空）。
    GameGlory,
    /// `GameGoldCommand`（现网 `@VV调整元宝`）。
    GameGold,
    /// `GamePointCommand`（现网 `@调整泡点`）。
    GamePoint,
    /// `GetUserItemsCommand`（现网 `@GetUserItems`）。
    GetUserItems,
    /// `HairCommand`（现网 `@改变发型`）。
    Hair,
    /// `HungerCommand`（现网 `@Hunger`）。
    Hunger,
    /// `IncPkPointCommand`（现网 `@增加PK`）。
    IncPkPoint,
    /// `LuckPointCommand`（现网 `@LuckyPoint`）。
    LuckPoint,
    /// `PKpointCommand`（现网 `@查看PK`）。
    PkPoint,
    /// `RefineWeaponCommand`（现网 `@RefineWeaponwa`）。
    RefineWeapon,
    /// `ReNewLevelCommand`（现网 `@转生`）。
    ReNewLevel,
    /// `RestBonuPointCommand`（现网 `@RestBonuPoint`）。
    RestBonuPoint,
    /// `TrainingCommand`（现网 `@+jn`；体只有权限自检）。
    Training,
    /// `TrainingMagicCommand`（现网 `@TrainingMagic`）。
    TrainingMagic,
    /// `TrainingSkillCommand`（现网 `@增加技能`）。
    TrainingSkill,
    /// `UnBindUseItemCommand`（现网 `@UnBindUseItem`；体整段注释）。
    UnBindUseItem,

    // ---- 批次 5 · 脚本可达族（卡 `openmir2-gm-impl-shutup`，tier=R）----
    /// `ShutupCommand`（现网 `@禁言`；`DenySayMsgList` 写入整段被注释 ⇒ 只发一条 SysMsg）。
    Shutup,

    /// `ShowEffectCommand`（现网 `@ShowEffect`，`Command.conf` 无覆盖 ⇒ 英文名）：oracle 命令体
    /// **整段被注释** ⇒ 任何参数都零帧、零副作用（最小 no-op 实现）。
    ShowEffect,

    /// `KickHumanCommand`（现网 `@VV踢人`）：置目标的 `BoKickFlag/BoEmergencyClose`；
    /// 目标离线 ⇒ 一条 Red/Hint；空名 ⇒ **静默 return**（零帧）。
    KickHuman,

    /// `ShutupReleaseCommand`（现网 `@取消禁言`）：`IsNullOrEmpty(x) || !IsNullOrEmpty(x)` 恒真
    /// ⇒ 永远只回帮助文本；名单移除整段被注释；第二参非 `true`/`false` ⇒ `bool.Parse` 抛
    /// `FormatException` 被 `PlayObject.Chat.cs:402-406` 吞掉 ⇒ 零帧。
    ShutupRelease,

    /// 未接的 80 条（185 − G1 5 条 − 批次 2/3 30 条 − 批次 3 18 条 − 批次 4 44 条 − 批次 5 8 条）：
    /// 现网全被权限门挡住，命令体留 G4-G6。
    // ---- 批次 5A · 行会/城堡族 A 片（6，卡 `openmir2-gm-batch-5a-guild-impl`；
    //      Leader 裁定 `9e7d2227`）----
    //
    // **权限与生效名都以 `GameCommands.cs` 构造 + `Command.conf` 为准**（不是 `[Command]` 属性）：
    // `Authally/Auth/AuthCancel` 的 `[Command(...)]` 写的是 0，但 `GameCommands.cs:371-373`
    // 的 `new GameCmd { PerMissionMin = 10 }` 覆盖成 **10**；`GameCmdConf.cs:16-18` 又把三条的
    // `CmdName` 覆盖成中文（`Command.conf [Command]`：`AuthAlly=允许联盟` / `Auth=联盟` /
    // `AuthCancel=取消联盟`）。`SbkDoorControlCommand` 注册在字段 **`SbkDoor`**（`GameCommands.cs:314`），
    // 生效名 `@SbkDoor`。
    /// `AnnouncementCommand`（`AnnouncementCommand.cs:8`；perm 10）；命令体 `:14-41` **整段被注释** ⇒ 真 no-op。
    Announcement,
    /// `AuthCommand`（`AuthCommand.cs:12-17`；生效名 `@联盟`，perm 10）：
    /// `if (IsGuildMaster()) ClientGuildAlly();` —— 非会长 ⇒ 零帧；会长路径未接（T-263）。
    Auth,
    /// `AuthallyCommand`（`AuthallyCommand.cs:9-23`；生效名 `@允许联盟`，perm 10）：
    /// `if (IsGuildMaster()) { /* 翻转 EnableAuthAlly + 两条 SysMsg 全被注释 */ } return;` ⇒ 真 no-op。
    Authally,
    /// `AuthCancelCommand`（`AuthCancelCommand.cs:12-21`；生效名 `@取消联盟`，perm 10）：
    /// 空参 ⇒ return；非会长 ⇒ return；会长路径未接（T-263）。
    AuthCancel,
    /// `GuildWarCommand`（`GuildWarCommand.cs:5`；perm 10）：体只有 `if (Permission < 6) return;` ⇒ 零帧。
    GuildWar,
    /// `SbkDoorControlCommand`（`SbkDoorControlCommand.cs:5`；注册名 `@SbkDoor`，perm 10）：空体 ⇒ 真 no-op。
    SbkDoorControl,

    // ---- 族 P · 密码锁门（7，卡 `openmir2-gm-family-p-passwordlock`，tier=L）----
    /// 密码锁 7 命令（`@hjhgj67` SetPassWord / `@sadsa54` UnPasswWord / `@ghjhg6` ChgpassWord /
    /// `@hjhg556` UnlockStorage / `@gfh678dfg` UnLock / `@fdgfdg56` Lock / `@PasswordLock`）：
    /// 部署 `PasswordLockSystem=0` ⇒ 命令体第一句恒回红字 `NoPasswordLockSystemMsg`，
    /// 零状态、零额外帧；启用分支（RM_PASSWORD 状态机）现网不可达 ⇒ `log_note` 登记。
    PasswordLockGate,

    // ---- 族 T · 会话开关/骑马（7，卡 `openmir2-gm-family-t-toggle-horse`，tier=L）----
    /// `LetTradeCommand`（现网 `@交易`）：翻转 `AllowDeal`（ctor true，`PlayObject.cs:1008`），
    /// 绿字 `[允许交易]`/`[禁止交易]` 二选一。
    LetTrade,
    /// `LetGuildCommand`（现网 `@加入门派`）：翻转 `AllowGuild`（ctor false，`PlayObject.cs:903`），
    /// 绿字 `[允许加入行会]`/`[禁止加入行会]`。
    LetGuild,
    /// `LetShoutCommand`（现网 `@拒绝组队聊天`）：翻转 `BanShout`（ctor true，`PlayObject.cs:1006`），
    /// 绿字 `[允许群聊]`/`[禁止群聊]`（常量名与状态语义相反，照抄）。
    LetShout,
    /// `BanGuildChatCommand`（现网 `@禁止行会信息`）：翻转 `BanGuildChat`（ctor true，
    /// `PlayObject.cs:1007`），绿字 `[允许行会聊天]`/`[禁止行会聊天]`（同上相反，照抄）。
    BanGuildChat,
    /// `TakeOffHorseCommand`（现网 `@OffHorse`）：未骑马静默零帧；骑马 ⇒ `OnHorse=false` +
    /// `FeatureChanged()` 外观帧（`pending_feature_changed`，宿主广播 `SM_FEATURECHANGED(41)`）。
    TakeOffHorse,
    /// `TakeOnHorseCommand`（现网 `@OnHorse`）：已骑马静默；`HorseType=0`（现网恒值，无赋值点）
    /// ⇒ 红字「骑马必须先戴上马牌!!!」；上马 ⇒ `OnHorse=true` + 外观帧 + `@OnHorse` NPC 脚本
    /// 跳线（宿主接缝，只登记不实现脚本面）。
    TakeOnHorse,
    /// `DisableFilterCommand`（现网 `@DisableFilter`）：翻转**全局** `SystemShare.FilterWord`
    /// （部署启动 true，`GameApp.cs:70`）+ 绿字「已启用文字过滤。」/「已禁止文字过滤。」。
    DisableFilter,

    // ---- 族 Q · 查询/展示（10，卡 `openmir2-gm-family-q-query-display`，tier=L）----
    /// `HumanCountCommand`（现网 `@HumanCountwa`）：地图不存在红字 `指定的地图不存在!!!`；
    /// 否则绿字 `怪物数量：{n}`（复用 MobCount 文案——oracle 就这么写）计**在线玩家**数。
    HumanCount,
    /// `MobCountCommand`（现网 `@MobCountwa`）：同上门；绿字计**怪物**数。
    MobCount,
    /// `MapInfoCommand`（现网 `@Map`；类名/出厂名都是 `MapInfo`，conf 键 `Map=Map`）：
    /// **直取 `@params[2]`** ⇒ 实参 <3 抛越界被吞 ⇒ 零帧；足参 ⇒ 格子 `标志: Walk` 绿字、
    /// 有对象再补 `对象数: N` 绿字；一切非可走格（HighWall/LowWall/越界）⇒ `GetCellInfo`
    /// success=false ⇒ 红字 `取地图单元信息失败: {图名}`；图不存在**静默**（无 else）；
    /// `nX<0 || nY<0 || 图名空` ⇒ 绿字「请按正确格式输入: Map 地图号 X Y」（注意是**绿**字）。
    MapInfo,
    /// `ShowMapInfoCommand`（现网 `@MapInfo`）：两条绿字——当前图 `地图名称: 名(描述)` +
    /// `地图大小: X(W) Y(H)`。
    ShowMapInfo,
    /// `ShowMapModeCommand`（现网 `@ShowMapMode`；conf 键是死键拼写 `ShoweMapMode`）：地图不存在
    /// 红字 `{图名} 不存在!!!`；否则蓝字 `地图模式: ` + `GetEnvirInfo()` 长串（TRUE/FALSE 布尔）。
    ShowMapMode,
    /// `ShowHumanFlagCommand`（现网 `@showflag`；conf 键 `Flag=showflag`）：空名/`?` 名红字帮助；
    /// 离线红字 `{名} 现在不在线，或在其它服务器上!!!`；在线 ⇒ 目标 QuestFlag 位阵读源在宿主
    /// 未建模（恒零值模型）⇒ 恒走 OFF 绿字支（ON 支结构保留）。
    ShowFlag,
    /// `HumanLocalCommand`（现网 `@IP`）：**判空恒真怪癖**（`IsNullOrEmpty(s) || !IsNullOrEmpty(s)`
    /// ⇒ 任何输入同答红字帮助；`GetPlayObject` 之后的支全部不可达）。
    HumanLocal,
    /// `HumanInfoCommand`（现网 `@信息`）：空名/`?` 名红字帮助；离线红字；**在线零帧**
    /// （`GetBaseObjectInfo` 的 SysMsg 被注释）。
    HumanInfo,
    /// `ShowUseItemInfoCommand`（现网 `@Showuseiteminfo`）：**判空下标怪癖**（`sHumanName[1]`，
    /// 单字符名越界被吞 ⇒ 零帧；[1]=='?' 红字帮助）；离线红字；在线 ⇒ 逐**非空佩戴位**发
    /// **字面**蓝字 `%s[%s]IDX[%d]系列号[%d]持久[%d-%d]`（.NET `string.Format` 无 `{n}` 占位
    /// ⇒ 原样输出、逐件相同）。
    ShowUseItemInfo,
    /// `LotteryTicketCommandL`（现网 `@LotteryTicket`；类名带尾 L）：一条绿字彩票 8 配置值
    /// （部署全 0：`GameSvrConf.cs:1688-1695` ctor 0 + `GameApp.cs:75-81` 启动清零 + 部署
    /// `Server.conf` 无这些键），格式串尾**带一个空格**。
    LotteryTicket,

    // ---- 族 X1 · 禁言名单（5，卡 `openmir2-gm-family-x1-denywhisper`，tier=L）----
    // **运行层真相（review F1，Leader 复核成立）**：`SystemShare.DisableSendMsgList`
    // （`SystemShare.cs:103`）与 `SystemShare.DenySayMsgList`（`:64`）**声明即 null、全仓
    // 零赋值**（初始化只在 M2Share 孪生字段，GameApp.cs:26/38）⇒ 触表的四个命令体真机
    // NRE、被 `PlayObject.Chat.cs:402` 顶层 catch 吞 ⇒ **零帧**——源文本的恒字/绿字分支
    // 真机不可达（前一轮误按源文本建模，已改零帧+log_note）。空名红帮助（+jy/-jy）与
    // ViewWhisper 全支（只触已初始化的 WorldEngine 与 actor 字段）不受影响。
    /// `DisableSendMsgCommand`（现网 `@+jy`）：空名红帮助可达；非空 `:26` 在线置目标
    /// `FilterSendMsg=true`（真实副作用、非帧）后 `:28` 对 null 表 Add 抛 NRE ⇒ 零帧
    /// （绿字/Add/落盘全部不可达）。
    DisableSendMsg,
    /// `DisableSendMsgListCommand`（现网 `@+-jy`）：首句读 null 表 Count 即 NRE ⇒ 恒零帧
    /// （源文本红/蓝两支真机均不可达）。
    DisableSendMsgListCmd,
    /// `EnableSendMsgCommand`（现网 `@-jy`）：空名红帮助可达；非空 `:26` for-init 读 null
    /// 表即 NRE ⇒ 零帧（源文本恒红「没有被禁言!!!」真机不可达）。
    EnableSendMsg,
    /// `ShutupListCommand`（现网 `@查看禁言`）：首句 `EnterCriticalSection(null)` 即 NRE ⇒
    /// 恒零帧（源文本恒绿「禁言列表为空!!!」真机不可达；对照 Chat.cs:137 走 M2Share 孪生
    /// 字段非 null ⇒ 聊天禁言门无害）。
    ShutupList,
    /// `ViewWhisperCommand`（现网 `@查看私聊`）：`sChrName[1]` 判空下标怪癖（单字符名越界
    /// ⇒ 零帧）；离线红；在线 toggle 说话者侧 `listening_to` 镜像（= 目标 `WhisperHuman` 的
    /// 等价观测面）⇒ 两条绿字。**已登记差异（review F3）**：oracle 目标重登 = 新 PlayObject
    /// ⇒ `WhisperHuman = null` ⇒ GM 再 toggle 出 Msg2；rust `listening_to` 持名不变 ⇒ 出
    /// Msg1（GM-only、零玩法影响）。
    ViewWhisper,
    Unimplemented,
}

/// 一条命令的生效定义（名字/权限已按 `Command.conf` 覆盖解析）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandDef {
    /// 现网拼写（`CommandMaps` 的 key，`OrdinalIgnoreCase`）。
    pub name: &'static str,
    /// `Command.PermissionMin`（`GameCommand.cs:37` 的门）。
    pub perm_min: u8,
    /// `Execute` 形参个数（`GameCommand.cs:99-103` 的三选一）。
    pub params: u8,
    pub imp: Imp,
}

/// 消息颜色（只承载 `BaseObject.SysMsg` 用到的三支；`MsgColor.Hear/Whisper` 走各自的帧）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GmColor {
    Red,
    Green,
    Blue,
}

/// 一条待发 `SysMsg`：色 + 正文（UTF-8 原文；线上由调用方 `gbk_encode`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GmMsg {
    pub color: GmColor,
    pub text: String,
}

/// GM 命令对**权威 `Entity`** 的一次写入（卡 `openmir2-gm-admin-mode-entity-seam`）。
///
/// oracle 的这些开关与 `PlayObject` 是**同一份字段**（`BaseObject.cs:145/157/263/267`、
/// `CharacterObject.cs:44`），引擎各处直接读；本仓把它们拆成了 `GmSession`（镜像，命令体读改写）
/// 与 `Entity`（权威，引擎读）两份，于是命令只翻转镜像 ⇒ `@1/@2/@3/@Rest/@AttackMode`
/// 回执发了、战斗/AOI/怪物 AI 却看不到。
///
/// 本类型是「把权威值落到实体」的最小差量：每个命令只填自己拥有的字段（`Some`），
/// 其余保持 `None`（别的命令/别的写点不归它管，避免把陈旧镜像盖回去）。
/// [`GmSession::pending_use_item`] 的载荷：**改完后的整槽**（`PlayObject.UseItems[n]`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmUseItemWrite {
    /// 佩戴位下标（`SmakeItemCommand.cs:22` 的 `nWhere`，守卫 `0..=12`）。
    pub slot: usize,
    /// `UserItem.Index`（模板查表用；未改）。
    pub index: u16,
    /// `UserItem.MakeIndex`（文案与 `SM_UPDATEITEM` 用；未改）。
    pub make_index: i32,
    /// 改后的 `UserItem.Dura`。
    pub dura: u16,
    /// 改后的 `UserItem.DuraMax`。
    pub dura_max: u16,
    /// 改后的 `UserItem.Desc`（14 字节）。
    pub desc: [u8; 14],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GmEntityState {
    /// 目标实体 = `GmWorld::self_entity()`（`WorldEngine.GetPlayObject(speaker)`）的 `Entity.id`。
    pub actor_id: i32,
    /// `@1` 翻转 `BaseObject.AdminMode`（`ChangeAdminModeCommand.cs:16`）。
    pub admin_mode: Option<bool>,
    /// `@2` 翻转 `BaseObject.ObMode`（`ChangeObModeCommand.cs:22`）。
    pub ob_mode: Option<bool>,
    /// `@3` 翻转 `BaseObject.SuperMan`（`ChangeSuperManModeCommand.cs:17`）。
    pub super_man: Option<bool>,
    /// `@Rest` 翻转 `BaseObject.SlaveRelax`（`ChangeSalveStatusCommand.cs:20`）。
    pub slave_relax: Option<bool>,
    /// `@AttackMode` 写回 `PlayObject.AttatckMode`（`ChangeAttackModeCommand.cs:18-40`）。
    pub attack_mode: Option<u8>,
    /// `@Level0` 写回 `PlayObject.Abil.Level`（`ChangeLevelCommand.cs:23`：
    /// `(byte)HUtil32._MIN(MessageSettings.MAXUPLEVEL(255), nLevel)`，**unchecked 强转**）。
    /// 卡 `openmir2-gm-target-state-write`。
    pub level: Option<u8>,
}

impl GmEntityState {
    /// 把权威值落到世界实体。调用点在**命令处理完之后、同一事件内**（宿主 `lib.rs` 的
    /// `CM_SAY` 支），因此早于本 tick 的 `world.tick()`——与 oracle「命令体直接写
    /// `PlayObject` 字段」的可观察次序一致（不需要额外一拍）。
    ///
    /// 实体不在场（`self_entity()` 为 `None` 时根本不会生成差量）⇒ 本函数兜底为 no-op。
    pub fn apply(&self, world: &mut mir2_world::World) {
        let Some(e) = world.entities.get_mut(self.actor_id) else {
            return;
        };
        if let Some(v) = self.admin_mode {
            e.admin_mode = v;
        }
        if let Some(v) = self.ob_mode {
            e.ob_mode = v;
        }
        if let Some(v) = self.super_man {
            e.super_man = v;
        }
        if let Some(v) = self.slave_relax {
            e.slave_relax = v;
        }
        if let Some(v) = self.attack_mode {
            e.attack_mode = v;
        }
        if let Some(v) = self.level {
            e.level = v;
        }
    }
}

/// GM 命令体发出的**非 `SysMsg`** 帧（oracle 里走 `SendDefMessage`/`SendRefMsg` 的那几张）。
///
/// 命令体只产出**语义**，wire 编码留给宿主（`bins/game-svr/src/smframe.rs`）——与
/// [`GmMsg`]「命名字段 + 宿主编码」同一层分工。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GmFrame {
    /// `SM_ATTACKMODE(213)`：`Recog` = 新模式（`smframe::sm_attackmode`）。
    AttackMode(u8),
}

/// 会话内 GM 状态（`PlayObject` 的同生命周期字段；随会话建/删）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GmSession {
    /// `PlayObject.Permission`（`PlayObject.cs:58`，默认 0）。现网无任何生产写点（设计件 §2）。
    pub permission: u8,
    /// `PlayObject.HearWhisper`（`PlayObject.cs:1005` ctor = `true`）。
    pub hear_whisper: bool,
    /// `PlayObject.LockWhisperList`（`PlayObject.cs:879` ctor = 空表；`@拒绝` 只加不删）。
    pub lock_whisper_list: Vec<String>,
    /// `PlayObject.IsLockLogon`（`PlayObject.cs:1026` ctor = **true**）。
    pub is_lock_logon: bool,
    /// `PlayObject.IsLockLogoned`（`PlayObject.cs:1027` ctor = false）。
    pub is_lock_logoned: bool,
    /// `PlayObject.TestSpeedMode`（`BaseObject` 字段，缺省 false；`@TestSpeedMode` 翻转）。
    pub test_speed_mode: bool,

    // ---- 批次 3 · 传送/召唤族（`PlayObject` 同生命周期字段；ctor 缺省见 `Default`）----
    /// `PlayObject.CanDearRecall`（`@dfsgf567` 翻转）。
    pub can_dear_recall: bool,
    /// `PlayObject.AllowGuildReCall`（`@sadasd` 翻转；记录 `characters.AllowGuildReCall` 登录写入）。
    pub allow_guild_re_call: bool,
    /// `PlayObject.CanMasterRecall`（`@fdgh56j` 翻转）。
    pub can_master_recall: bool,
    /// `PlayObject.RecallSuite`（`PlayObject.cs:510`；唯一写点 `:2530`，`@天地合一` 的门之一）。
    pub recall_suite: bool,
    /// `PlayObject.IsMaster`（记录 `is_master`；`@jkt76` 的首门）。
    pub is_master: bool,
    /// `PlayObject.GuildMove`（`PlayObject.cs:151`；脚本 `GuildMove` 置位，`@df56dfs` 的门之一）。
    pub guild_move: bool,
    /// `PlayObject.Teleport`（装备派生；本仓未建模该派生 ⇒ 缺省 false，见 T-169）。
    pub teleport: bool,
    /// `PlayObject.DearName`（记录 `dear_name`；空 = 未婚）。
    pub dear_name: String,
    /// `PlayObject.Gender`（记录 `sex`；0 = 男，非 0 = 女——只用于夫妻文案）。
    pub gender: u8,
    /// `PlayObject.DearRecallTick`（ms）。
    pub dear_recall_tick: i64,
    /// `PlayObject.MasterRecallTick`（ms）。
    pub master_recall_tick: i64,
    /// `PlayObject.TeleportTick`（ms）。
    pub teleport_tick: i64,
    /// `PlayObject.GroupRcallTick`（ms；ctor 不赋值 ⇒ 0）。
    pub group_rcall_tick: i64,
    /// `PlayObject.GroupRcallTime`（秒；记录 `group_rcall_time`，ctor 缺省 0）。
    pub group_rcall_time: i16,
    /// `PlayObject.MasterList` 的**在线**徒弟名（本仓无师徒在线表 ⇒ 恒空，T-168）。
    pub master_list: Vec<String>,
    // ---- 批次 4 · 数值/角色族（`PlayObject` 同生命周期字段）----
    /// `PlayObject.AdminMode`（`@1` 翻转）。
    pub admin_mode: bool,
    /// `PlayObject.ObMode`（`@2` 翻转）。
    pub ob_mode: bool,
    /// `PlayObject.SuperMan`（`@3` 翻转）。
    pub super_man: bool,
    /// `PlayObject.SlaveRelax`（`@Rest` 翻转）。
    pub slave_relax: bool,
    /// `PlayObject.AttatckMode`（`@AttackMode`；`AttackMode` 枚举 0..=6）。
    pub attack_mode: u8,
    /// `PlayObject.ReLevel`（`@转生` 只写「发起者自己」时的镜像；其余情况保持登录值）。
    pub re_level: u8,
    /// **待落地的权威 `Entity` 写**（卡 `openmir2-gm-admin-mode-entity-seam`）。
    ///
    /// 本结构是 oracle `PlayObject` 字段的**镜像**；引擎（战斗/AOI/怪物 AI）读的是
    /// `crates/world/src/entity.rs` 的 `Entity` 同名字段。命令体只写镜像 ⇒ 回执发了、
    /// 行为不变。故 `@1/@2/@3/@Rest/@AttackMode` 在翻转镜像的同时把权威值装进本缓冲，
    /// 由宿主在**同一事件内**（`lib.rs` 的 `CM_SAY` 支）取走并 [`GmEntityState::apply`]，
    /// 仍在 `world.tick()` 之前。每次命令都会整体覆盖本字段（不会残留上一次的值）。
    pub pending_entity: Option<GmEntityState>,
    /// **待落地的踢人目标**（卡 `openmir2-gm-impl-kick`，`KickHumanCommand.cs:26-31`）。
    ///
    /// oracle 命令体直接写**目标** `PlayObject.BoKickFlag/BoEmergencyClose`；本仓命令体是纯函数、
    /// `GmWorld` 只读 ⇒ 命令体把目标 `ChrName` 装进本缓冲（与 [`GmSession::pending_entity`] 同法），
    /// 宿主（`lib.rs` 的 sim 循环）在 `drain_kicks` 之前收口：该支先发
    /// `SM_OUTOFCONNECTION(528)` 再 `KickUser` + 存档 + `MakeGhost`（`PlayObject.Message.cs:367-378`）。
    pub pending_kick: Option<String>,
    /// **待落地的佩戴位写**（卡 `openmir2-gm-impl-smake`，`SmakeItemCommand.cs:28-61`）。
    ///
    /// oracle 命令体直接改**目标（= 说话者）自己的** `PlayObject.UseItems[n]` 再
    /// `RecalcAbilitys()` + `SendUpdateItem()`；本仓命令体纯、`GmWorld` 只读 ⇒ 命令体把
    /// 「改完后的整槽」装进本缓冲（与 `pending_entity` 同法），宿主在同一事件内落地
    /// （写会话 `use_items` + 实体 `UseItems`、重算 WAbil、发 `SM_UPDATEITEM(203)`）。
    pub pending_use_item: Option<GmUseItemWrite>,
    /// **待落地的清包目标**（卡 `openmir2-gm-impl-clearbag`，`ClearBagItemCommand.cs:31-52`）：
    /// 目标 `ChrName`。oracle 命令体直接清目标 `ItemList` 并给目标发 `RM_SENDDELITEMLIST`；
    /// 本仓命令体纯 ⇒ 只登记目标，宿主收口（空包 ⇒ 不发帧，与 oracle 同）。
    pub pending_clear_bag: Option<String>,
    /// **`@Level0` 的 `HasLevelUp(1)` 待落地**（卡 `openmir2-gm-target-state-write`）：
    /// oracle 命令体写完 `Abil.Level` 后当场调 `HasLevelUp(1)`（`PlayObject.cs:5242-5252`）
    /// ⇒ `MaxExp = GetLevelExp(level)` + `RecalcLevelAbilitys()` + `RecalcAbilitys()` +
    /// `SendMsg(RM_LEVELUP)`（泵发 `SM_LEVELUP`/`SM_ABILITY`/`SM_SUBABILITY` 三条直连帧）。
    /// 本仓命令体纯 ⇒ 宿主在同一事件内按本标记落地。
    pub pending_level_up: Option<u8>,

    // ---- 族 T · 会话开关/骑马（`PlayObject` 同生命周期字段；ctor 缺省见 `Default`）----
    /// `PlayObject.AllowDeal`（ctor true，`PlayObject.cs:1008`；`@交易` 翻转）。
    pub allow_deal: bool,
    /// `PlayObject.AllowGuild`（ctor false，`PlayObject.cs:903`；`@加入门派` 翻转）。
    pub allow_guild: bool,
    /// `PlayObject.BanShout`（ctor true，`PlayObject.cs:1006`；`@拒绝组队聊天` 翻转）。
    pub ban_shout: bool,
    /// `PlayObject.BanGuildChat`（ctor true，`PlayObject.cs:1007`；`@禁止行会信息` 翻转）。
    pub ban_guild_chat: bool,
    /// `PlayObject.OnHorse`（ctor false，`PlayObject.cs:988`；`@OnHorse`/`@OffHorse` 翻转）。
    pub on_horse: bool,
    /// `PlayObject.HorseType`（`CharacterObject.cs:52`；oracle **全仓无赋值点** ⇒ 恒 0 ⇒
    /// `@OnHorse` 的红字「骑马必须先戴上马牌!!!」是唯一可达支；测试可构造非 0 进上马支）。
    pub horse_type: u8,
    /// **待落地的 `FeatureChanged()` 外观帧**（oracle `BaseObject.cs:1509-1512`：
    /// `SendRefMsg(RM_FEATURECHANGED, GetFeatureEx(), GetFeatureToLong(), …)` ⇒ 视距广播
    /// `SM_FEATURECHANGED(41)`）。`@OnHorse`/`@OffHorse` 翻转 `on_horse` 后置位；宿主在
    /// 同一事件内按「翻转后的会话态」现算 feature/feature_ex 并走
    /// [`crate::downlink`] 的视距广播（与换装路径同一收口，见 lib.rs `apply_pending_gm_feature_changed`）。
    pub pending_feature_changed: bool,
    /// **族 X1**：`@查看私聊` 的监听位镜像——oracle 写**目标** `WhisperHuman = PlayerActor`
    /// （谁在监听我；`ViewWhisperCommand.cs:33-39`）。本仓写面消费点（私聊回声路由）未接
    /// ⇒ 该位唯一可观测面 = 本命令的 toggle 回执，故以**说话者侧** `listening_to` 承载
    /// （`Some(目标名)` = 正在监听该目标）：单命令驱动流下与 oracle 逐支等价；
    /// 多听者覆盖差异（oracle 目标仅一个监听位）登记于 [`Imp::ViewWhisper`] 文档。
    pub listening_to: Option<String>,
}

impl Default for GmSession {
    fn default() -> Self {
        Self {
            permission: 0,
            hear_whisper: true,
            lock_whisper_list: Vec::new(),
            is_lock_logon: true,
            is_lock_logoned: false,
            test_speed_mode: false,
            can_dear_recall: false,
            allow_guild_re_call: false,
            can_master_recall: false,
            recall_suite: false,
            is_master: false,
            guild_move: false,
            teleport: false,
            dear_name: String::new(),
            gender: 0,
            dear_recall_tick: 0,
            master_recall_tick: 0,
            teleport_tick: 0,
            group_rcall_tick: 0,
            group_rcall_time: 0,
            master_list: Vec::new(),
            admin_mode: false,
            ob_mode: false,
            super_man: false,
            slave_relax: false,
            attack_mode: 0,
            re_level: 0,
            pending_entity: None,
            pending_kick: None,
            pending_use_item: None,
            pending_clear_bag: None,
            pending_level_up: None,
            allow_deal: true,
            allow_guild: false,
            ban_shout: true,
            ban_guild_chat: true,
            on_horse: false,
            horse_type: 0,
            pending_feature_changed: false,
            listening_to: None,
        }
    }
}

impl GmSession {
    /// 登录期把 `GetHumData` 写进 `PlayObject` 的几项装进会话（其余保持 ctor 缺省）。
    ///
    /// 口径：oracle 的 `DearName/Gender/IsMaster/AllowGuildReCall/GroupRcallTime` 由角色记录
    /// 装入（`WorldServer.cs:279-350` + `PlayObject` 字段），本仓由登录分支从
    /// `record::CharacterData` 传进来（`lib.rs` 一处构造点）。
    ///
    /// 另两项同样在登录时从 `GetHumData` 装入：`AttatckMode = humData.AttatckMode`
    /// （`WorldServer.cs:1148`）、`ReLevel = humData.ReLevel`（`:1133`）——对应
    /// `record::CharacterData.attatck_mode`/`.re_level`，不装载会让 `@AttackMode` 的
    /// 连加序列整体偏移、`@转生` 印出的发起者 `ReLevel` 为 0。
    #[must_use]
    pub fn from_character(
        dear_name: &str,
        gender: u8,
        is_master: bool,
        allow_guild_re_call: bool,
        group_rcall_time: i16,
        attack_mode: u8,
        re_level: u8,
    ) -> Self {
        Self {
            dear_name: dear_name.to_string(),
            gender,
            is_master,
            allow_guild_re_call,
            group_rcall_time,
            attack_mode,
            re_level,
            ..Self::default()
        }
    }
}

impl GmSession {
    /// 生成「本次命令要落到权威 `Entity`」的**空差量**（只带目标 `actor_id`）。
    ///
    /// 调用点在命令翻转镜像之后：`self_entity()` 为空（发起者不在世界实体表）⇒ `None`，
    /// 即不伪造写入——oracle 里命令对象与 `PlayObject` 是同一引用，不存在这种情形；
    /// 本仓只有「会话在、实体不在」的边角（未进世界/已 Ghost），此时命令仍回执但不写实体。
    fn entity_write(&self, world: &GmWorld<'_>) -> Option<GmEntityState> {
        // 优先用宿主给出的说话者 `ActorId`（oracle 的命令对象就是这个 `PlayObject`，Ghost 与否
        // 都写同一字段）；`0`（测试/无在线表）时回落到按名查找。实体不在表里 ⇒ `apply` 是 no-op。
        let actor_id = if world.speaker_id > 0 {
            world.speaker_id
        } else {
            world.self_entity()?.id
        };
        Some(GmEntityState {
            actor_id,
            ..GmEntityState::default()
        })
    }

    /// `@1/@2/@3/@Rest/@AttackMode` 共用：把「翻转/算完之后」的权威值装进待落地差量。
    fn stash(&mut self, state: Option<GmEntityState>, f: impl FnOnce(&mut GmEntityState)) {
        if let Some(mut st) = state {
            f(&mut st);
            self.pending_entity = Some(st);
        }
    }
}

/// 命令体要读的配置（`SystemShare.Config` / `SystemShare` 启动态的等价物）。
///
/// 除 `lock_human_login`（真配置解析）外都是**启动期常量**（批次 2/3 与族 P）：oracle 部署的
/// 取值在字段注释里写明出处，[`Default`] 就取部署值（重载/管理族与密码锁门族的分支正是由它们决定）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GmConfig {
    /// `Config.LockHumanLogin`（`ServerConf.cs:574` 读 `[Setup] PasswordLockHumanLogin`；
    /// `GameSvrConf.cs:1493` 缺省 false，部署 = 0）。
    pub lock_human_login: bool,
    /// `Config.PasswordLockSystem`（`ServerConf.cs:570` 读 `[Setup] PasswordLockSystem`；
    /// `GameSvrConf.cs:1489` 缺省 false，部署 `Server.conf:382` = 0）：密码锁族 7 命令
    /// （`@hjhgj67`/`@sadsa54`/`@ghjhg6`/`@hjhg556`/`@gfh678dfg`/`@fdgfdg56`/`@PasswordLock`）
    /// 的命令体第一道门。部署恒 false ⇒ 恒回红字 [`NO_PASSWORD_LOCK_SYSTEM_MSG`]。
    /// 本仓不解析 `[Setup]`（部署值即常量，族 P 卡面口径；启用分支现网不可达）。
    pub password_lock_system: bool,
    /// `Config.WinLotteryCount`（`ServerConf.cs:645` 读 `[Setup] WinLotteryCount`；
    /// `GameSvrConf.cs:1688` ctor 0、`GameApp.cs:75` 启动再清零；部署 `Server.conf` **无此键**
    /// ⇒ 0）。`@LotteryTicket` 的 8 配置值之一。
    pub win_lottery_count: i32,
    /// `Config.NoWinLotteryCount`（同上口径 ⇒ 0）。
    pub no_win_lottery_count: i32,
    /// `Config.WinLotteryLevel1..6`（`ServerConf.cs:647-652`；ctor/启动双清零 + 无部署键
    /// ⇒ 全 0）。格式串 `GameCommandLotteryTicketMsg` 的 {2}..{7}。
    pub win_lottery_level1: i32,
    /// 见 [`Self::win_lottery_level1`]。
    pub win_lottery_level2: i32,
    /// 见 [`Self::win_lottery_level1`]。
    pub win_lottery_level3: i32,
    /// 见 [`Self::win_lottery_level1`]。
    pub win_lottery_level4: i32,
    /// 见 [`Self::win_lottery_level1`]。
    pub win_lottery_level5: i32,
    /// 见 [`Self::win_lottery_level1`]。
    pub win_lottery_level6: i32,
    /// `SystemShare.ServerIndex`（`SystemShare.cs:45` 缺省 0）。全仓**无写点**
    /// （只有 `AppService.cs:211` 读）⇒ 部署恒 0 = 主服 ⇒ `ReloadGuild`/`ReloadGuildAll` 永远走成功支。
    pub server_index: u8,
    /// `SystemShare.ManageNPC != null`（`GameApp.cs:85` 启动即 `new Merchant()` ⇒ 部署 true）
    /// 与 `SystemShare.FunctionNPC != null`（`:87` ⇒ true）。
    pub manage_npc_present: bool,
    /// 见 [`GmConfig::manage_npc_present`]（`FunctionNPC`）。
    pub function_npc_present: bool,
    /// `SystemShare.RobotNPC != null`（`GameApp.cs:86` ⇒ true）。
    pub robot_npc_present: bool,
    /// `File.Exists(SystemShare.GetNoticeFilePath("LineNotice.txt"))`：部署件
    /// `E:/MirServer/M2GameSvr/Notice/linenotice.txt` 存在（Windows 大小写不敏感）⇒ true。
    pub notice_file_exists: bool,
    /// `SystemShare.Config.GroupRecallTime`（`ServerConf.cs:480` 读 `[Setup] GroupRecallTime`；
    /// `GameSvrConf.cs:1702` 源码缺省 180）。部署值 = 5。
    pub group_recall_time: i16,
    /// `SystemShare.Config.GuildRecallTime`（`ServerConf.cs:479`；源码缺省 180）。部署值 = 600。
    pub guild_recall_time: i32,
    /// `SystemShare.Config.dwUserMoveTime`（`UserMoveXY` 冷却，秒）。部署值 = 10。
    pub user_move_time: i32,
    /// `[Setup] UserMoveCanDupObj`（`CanWalkOfItem` 的 `boFlag`：true 忽略格上角色）。部署值 = 1。
    pub user_move_can_dup_obj: bool,
    /// `[Setup] UserMoveCanOnItem`（`CanWalkOfItem` 的 `boItem`：true 忽略格上物品）。部署值 = 1。
    pub user_move_can_on_item: bool,
}

impl Default for GmConfig {
    fn default() -> Self {
        Self {
            lock_human_login: false,
            password_lock_system: false,
            win_lottery_count: 0,
            no_win_lottery_count: 0,
            win_lottery_level1: 0,
            win_lottery_level2: 0,
            win_lottery_level3: 0,
            win_lottery_level4: 0,
            win_lottery_level5: 0,
            win_lottery_level6: 0,
            server_index: 0,
            manage_npc_present: true,
            function_npc_present: true,
            robot_npc_present: true,
            notice_file_exists: true,
            group_recall_time: 5,
            guild_recall_time: 600,
            user_move_time: 10,
            user_move_can_dup_obj: true,
            user_move_can_on_item: true,
        }
    }
}

/// 一次派发的结果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GmResult {
    /// `CommandSystem.Execute` 的返回值（命中即 true ⇒ 不再走 `PlayObject.Chat.cs:372-400` 回落）。
    pub found: bool,
    /// `ExtractCommandAndParameters` 是否成功（失败时 oracle 不发任何帧，也没有回落）。
    pub parsed: bool,
    /// 依序要发的 `SysMsg(_, _, Hint)`。
    pub msgs: Vec<GmMsg>,
    /// 依序要发的**非 `SysMsg`** 帧（oracle `SendDefMessage`/`SendRefMsg` 面；宿主负责编码/路由）。
    pub frames: Vec<GmFrame>,
    /// oracle 侧被 `ProcessUserLineMsg` 的 catch 吞掉的路径 / 本批未接的登记（调用方记日志，不静默）。
    pub log_note: Option<&'static str>,
}

impl GmResult {
    fn one(color: GmColor, text: String) -> Self {
        Self {
            found: true,
            parsed: true,
            msgs: vec![GmMsg { color, text }],
            frames: Vec::new(),
            log_note: None,
        }
    }

    fn empty() -> Self {
        Self {
            found: true,
            parsed: true,
            msgs: Vec::new(),
            frames: Vec::new(),
            log_note: None,
        }
    }
}

/// `ExtractCommandAndParameters`（`CommandSystem.cs:228-250`）。
///
/// - `Trim()` 后为空 / 首字符非 `@` ⇒ `None`（oracle 返回 false，无帧）。
/// - 命令名 = 去掉首个 `@` 后按**单个 ASCII 空格**切出的首段（`@a\tb` 的命令名含制表符；
///   不用 `:`/`,`/`\t` 切）。
/// - 参数 = 首个空格之后的全部内容再 `Trim()`（无空格 ⇒ 空串；内部连续空格**保留**）。
pub fn parse_command(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let rest = line.strip_prefix('@')?;
    let command = rest.split(' ').next().unwrap_or("");
    let parameters = match rest.find(' ') {
        Some(i) => rest[i + 1..].trim(),
        None => "",
    };
    Some((command, parameters))
}

/// `CommandMaps` 查找（`StringComparer.OrdinalIgnoreCase`；表内名字只有 ASCII 与中文，
/// 对它们「ASCII 大小写折叠 + 其余精确」与 `OrdinalIgnoreCase` 等价，见设计件 D-6）。
pub fn find(name: &str) -> Option<&'static CommandDef> {
    COMMANDS.iter().find(|d| d.name.eq_ignore_ascii_case(name))
}

/// `PlayObject.Chat.cs:368-400` 的未知命令回落**第二条**帧的正文。
///
/// `perm < 2` 时 `sCMD` 保持空串 ⇒ `@此命令不正确，或没有足够的权限!!!`；`perm >= 2` 且
/// `sData.Length > 2` 时 `sCMD` = `GetValidStr3(sData, [' ', ':', ',', '\t'])` 的首个非空段
/// （**含前导 `@`**）。`perm >= 6 && sData[2] == GMRedMsgCmd('!')` 的红字广播支未接（现网 Permission
/// 恒 0 ⇒ 不可达；设计件 D-3 归 G6）。
pub fn incorrect_msg(permission: u8, s_data: &str) -> String {
    let mut s_cmd = String::new();
    if permission >= 2 && s_data.encode_utf16().count() > 2 {
        s_cmd = s_data
            .split([' ', ':', ',', '\t'])
            .find(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
    }
    format!("@{s_cmd}此命令不正确，或没有足够的权限!!!")
}

/// `HelpCommandGroup.Handle`（`CommandSystem.cs:294-307`）：空参回 usage，否则按前两段拼 `Unknown command`。
fn help_text(parameters: &str) -> String {
    if parameters.is_empty() {
        return HELP_USAGE.to_string();
    }
    let params: Vec<&str> = parameters.split(' ').collect();
    let group = params[0];
    let command = params.get(1).copied().unwrap_or("");
    format!("Unknown command: {group} {command}")
}

/// `PrvMsgCommand.Execute`（`Commands/PrvMsgCommand.cs`）。
///
/// 去重/删除分支在 oracle 里整段被注释 ⇒ **只加不删**。`sHumanName[1]` 是 UTF-16 下标：名字只有一个
/// 字符时抛 `IndexOutOfRangeException`，被 `ProcessUserLineMsg` 的 catch 吞掉 ⇒ **零帧**（设计件 §4.5 #2）。
fn prv_msg(parameters: &str, sess: &mut GmSession) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let human = params.first().copied().unwrap_or("");
    if human.is_empty() {
        return GmResult::one(GmColor::Red, PRV_MSG_HELP.to_string());
    }
    let mut units = human.encode_utf16();
    units.next();
    let Some(second) = units.next() else {
        return GmResult {
            found: true,
            parsed: true,
            msgs: Vec::new(),
            frames: Vec::new(),
            log_note: Some(
                "PrvMsg: sHumanName[1] 越界（oracle 抛 IndexOutOfRangeException，被 catch 吞）",
            ),
        };
    };
    if second == u16::from(b'?') {
        return GmResult::one(GmColor::Red, PRV_MSG_HELP.to_string());
    }
    sess.lock_whisper_list.push(human.to_string());
    GmResult::one(GmColor::Green, format!("{human} 已被加入禁止私聊列表..."))
}

/// `AllowMsgCommand.Execute`（`Commands/AllowMsgCommand.cs`）：翻转 `HearWhisper`，Green。
fn allow_msg(sess: &mut GmSession) -> GmResult {
    sess.hear_whisper = !sess.hear_whisper;
    let text = if sess.hear_whisper {
        "[允许私聊]"
    } else {
        "[禁止私聊]"
    };
    GmResult::one(GmColor::Green, text.to_string())
}

/// `LockLoginCommand.Execute`（`Commands/LockLoginCommand.cs`）。
fn lock_login(sess: &mut GmSession, cfg: &GmConfig) -> GmResult {
    if !cfg.lock_human_login {
        return GmResult::one(GmColor::Red, "本服务器还没有启用登录锁功能!!!".to_string());
    }
    if sess.is_lock_logon && !sess.is_lock_logoned {
        return GmResult::one(
            GmColor::Red,
            "您还没有打开登录锁或还没有设置锁密码!!!".to_string(),
        );
    }
    sess.is_lock_logon = !sess.is_lock_logon;
    let text = if sess.is_lock_logon {
        "已开启登录锁"
    } else {
        "已关闭登录锁"
    };
    GmResult::one(GmColor::Green, text.to_string())
}

/// `CreateRobotPlayCommand.Execute`（`Commands/CreateRobotPlayCommand.cs`）：除 `?` 帮助外**无任何副作用**
/// （加机器人的循环整段被注释，`userCount` 算出来没用）。
fn add_robot_play(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    if params.first().copied().unwrap_or("") == "?" {
        return GmResult::one(GmColor::Red, ADD_ROBOT_HELP.to_string());
    }
    GmResult::empty()
}

// ============================================================================================
// 批次 2/3：重载/管理（14）· 封禁名单（9）· 桩/测试（7 + G1 的 `@AddRobotPlay`）
//
// 逐条 1:1 对照 `src/Modules/GameCommand/Commands/*.cs`。**oracle 注释掉的逻辑照抄 no-op**
// （设计件 §4.5 #7）。本仓没有的宿主（物品库/魔法库/爆率/NPC 脚本/公告/行会/机器人/事件队列）
// 一律「回 oracle 同一句 `SysMsg` + `log_note` 登记」，**不伪造任何重载动作**（任务书要求）。
// ============================================================================================

/// `HUtil32.StrToInt`（`HUtil32.cs:359-362`）：`int.TryParse` 失败取缺省值。
///
/// `NumberStyles.Integer` = 允许前后空白 + 一个前导符号 + 十进制数字；不接受小数点/十六进制/
/// 千分位 ⇒ 与 `str::parse::<i32>` 同口径（`+5`/`-5` 都收）。唯一残余是 `trim` 的空白集合
/// （.NET `char.IsWhiteSpace` vs Unicode `White_Space`；本批参数都是 ASCII，不可观测）。
fn str_to_int(s: &str, def: i32) -> i32 {
    s.trim().parse::<i32>().unwrap_or(def)
}

/// 命令体要读的世界侧只读状态（oracle `SystemShare` 的等价物）。
///
/// 批次 2/3 只用 `guilds`；批次 3（传送/召唤）再加三项**只读视图**（宿主已有能力，不另造）：
/// - `maps` = `SystemShare.MapMgr`（`FindMap` / `MapDesc` / `Flag`）；
/// - `world` = `WorldEngine` 的实体表（`GetPlayObject` / 坐标 / 城堡表）；
/// - `speaker`/`now_ms` = `PlayerActor.ChrName` 与 `HUtil32.GetTickCount()`。
#[derive(Clone, Copy)]
pub struct GmWorld<'a> {
    /// `SystemShare.GuildMgr`；`FindGuild` 是 `GuildName == sGuildName` **精确**比较
    /// （`GuildManager.cs:66-76`，不是 `OrdinalIgnoreCase`）。
    pub guilds: &'a mir2_world::GuildManager,
    /// `M2Share.MapMgr` 的等价物；`None` = S1 占位世界（无 Envir 注册表）。
    pub maps: Option<&'a mir2_envir::MapRegistry>,
    /// `SystemShare.WorldEngine` 的实体表（`GetPlayObject`/坐标/`CastleList`）；`None` = 未接线。
    pub world: Option<&'a mir2_world::World>,
    /// 命令发起者 `ChrName`（`@天地合一` 的 `GroupOwner == ActorId` 自比用）。
    pub speaker: &'a str,
    /// 命令发起者的 `ActorId`（`OnlineEntry.actor_id` / `SessionGround.actor_id`）＝ oracle
    /// 「命令对象就是那个 `PlayObject`」的等价物。`0` = 宿主未接线（测试/无在线表）。
    ///
    /// 管理态写穿用它定位实体（卡 `openmir2-gm-admin-mode-entity-seam`，review `2c5cb955` m-2）：
    /// 按名查找 `play_by_name` 会滤掉 `Ghost`，而 oracle 直接写那个对象、Ghost 与否都写同一字段。
    pub speaker_id: i32,
    /// `HUtil32.GetTickCount()` 的等价物（ms；冷却门的 `now`）。
    pub now_ms: i64,
    /// `SystemShare.FilterWord`（**全局可变位**，`SystemShare.cs:157` 声明、部署启动
    /// `GameApp.cs:70` 置 true；`@DisableFilter` 翻转）的宿主侧共享单元。
    ///
    /// oracle 是进程级单例（不是 `PlayObject` 字段）⇒ 用 `Cell` 内可变共享引用承载，
    /// 跨会话/跨事件同一份；`None` = 构造处未注入（测试/`ScriptGmEnv::bridge` 的脚本
    /// GMEXECUTE 路径）⇒ 命令体走 `log_note` 登记零帧（不静默伪造翻转）。
    pub filter_word: Option<&'a std::cell::Cell<bool>>,
}

impl<'a> GmWorld<'a> {
    /// 空视图（只带行会表）：测试与「无 Envir / 无世界」的调用方用。
    #[must_use]
    pub fn new(guilds: &'a mir2_world::GuildManager) -> Self {
        Self {
            guilds,
            maps: None,
            world: None,
            speaker: "",
            speaker_id: 0,
            now_ms: 0,
            filter_word: None,
        }
    }

    #[must_use]
    pub fn with_maps(mut self, maps: &'a mir2_envir::MapRegistry) -> Self {
        self.maps = Some(maps);
        self
    }

    #[must_use]
    pub fn with_world(mut self, world: &'a mir2_world::World) -> Self {
        self.world = Some(world);
        self
    }

    #[must_use]
    pub fn with_speaker(mut self, speaker: &'a str) -> Self {
        self.speaker = speaker;
        self
    }

    #[must_use]
    pub fn with_now(mut self, now_ms: i64) -> Self {
        self.now_ms = now_ms;
        self
    }

    /// 注入宿主的 `SystemShare.FilterWord` 全局单元（`@DisableFilter` 用）。
    #[must_use]
    pub fn with_filter_word(mut self, filter_word: &'a std::cell::Cell<bool>) -> Self {
        self.filter_word = Some(filter_word);
        self
    }

    /// `WorldEngine.GetPlayObject`（`WorldServer.cs:937-954`）：`PlayObjectList` 顺序扫，
    /// `ChrName` **OrdinalIgnoreCase** 命中首个、且 `!Ghost`。本仓实体表含怪物 ⇒ 加 `Race == Play` 门。
    fn play_by_name(&self, name: &str) -> Option<&'a mir2_world::Entity> {
        let world = self.world?;
        world.entities.iter().find(|e| {
            e.race == mir2_world::ACTOR_RACE_PLAY && !e.ghost && e.name.eq_ignore_ascii_case(name)
        })
    }

    fn entity(&self, id: i32) -> Option<&'a mir2_world::Entity> {
        self.world?.entities.get(id)
    }

    /// `PlayObject.UseItems[n]`（**说话者自己**的佩戴位；`SmakeItemCommand.cs:28` 的读源）。
    fn self_use_item(&self, slot: usize) -> Option<&'a mir2_world::entity::UseItemSlot> {
        self.self_entity()?.use_items.get(slot)
    }

    fn self_entity(&self) -> Option<&'a mir2_world::Entity> {
        if self.speaker.is_empty() {
            return None;
        }
        self.play_by_name(self.speaker)
    }

    fn self_pos(&self) -> Option<(usize, i32, i32)> {
        let e = self.self_entity()?;
        Some((e.map_id, e.x, e.y))
    }

    fn map_name(&self, map_id: usize) -> Option<&'a str> {
        self.maps?.name_of(map_id)
    }

    fn flag_of(&self, map_id: usize) -> Option<&'a mir2_envir::MapInfoFlag> {
        self.maps?.flag_of(map_id)
    }

    /// `PlayerActor.Envir.Flag`（发起者当前地图的 `Flag`）。
    fn self_flag(&self) -> Option<&'a mir2_envir::MapInfoFlag> {
        let e = self.self_entity()?;
        self.flag_of(e.map_id)
    }

    /// `Envir.CanWalk(nX, nY, true)`（`Envirnoment.cs:442-451`）：`GetCellInfo` 成功 ⇔
    /// 界内**且** `Valid`（`boFlag=true` 忽略格上对象）。`None` = 世界图未接线/尺寸不符。
    fn can_walk(&self, map_id: usize, x: i32, y: i32) -> Option<bool> {
        let world = self.world?;
        let grid = world.maps.get(map_id)?;
        Some(grid.cell_match(x, y) && grid.cell_valid(x, y))
    }

    /// `Envir.CanWalkOfItem(nX, nY, boFlag, boItem)`（`Envirnoment.cs:502-540`）。
    ///
    /// `boFlag=true` 忽略格上角色、`boItem=true` 忽略格上物品（部署两者都是 1 ⇒ 恒 true）。
    /// `None` = 需要 `FixedHideMode/ObMode/HoldPlace` 等本仓未建模的 actor 状态。
    fn can_walk_of_item(
        &self,
        map_id: usize,
        x: i32,
        y: i32,
        ignore_actor: bool,
        ignore_item: bool,
    ) -> Option<bool> {
        let world = self.world?;
        let grid = world.maps.get(map_id)?;
        let (cell, success) = grid.get_cell_info(x, y);
        if success && cell.valid() && cell.is_available() {
            for obj in &cell.obj_list {
                if !ignore_actor && obj.cell_obj_id > 0 && obj.actor_object {
                    return None;
                }
                if !ignore_item && obj.cell_type == mir2_world::CellType::Item {
                    return Some(false);
                }
            }
        }
        Some(true)
    }

    /// `PlayObject.IsGuildMaster()`（`PlayObject.cs:4402-4405`）：`MyGuild != null && GuildRankNo == 1`。
    /// 本仓行会会话态不在 `GmWorld` ⇒ 用 `GuildMgr` 的成员表等价求：首个含该角色的行会里 `RankNo == 1`。
    fn is_guild_master(&self, chr_name: &str) -> bool {
        let Some(guild_name) = self.guilds.member_of_guild(chr_name) else {
            return false;
        };
        self.guilds
            .find_guild(guild_name)
            .and_then(|g| g.get_rank_name(chr_name))
            .is_some_and(|(rank_no, _)| rank_no == 1)
    }
}

/// 零帧 + 一条 `log_note`：oracle 抛异常被 `PlayObject.Chat.cs` 的 catch 吞掉，或宿主未接。
fn no_frames(note: &'static str) -> GmResult {
    GmResult {
        found: true,
        parsed: true,
        msgs: Vec::new(),
        frames: Vec::new(),
        log_note: Some(note),
    }
}

/// `AuthCommand.Execute`（`AuthCommand.cs:12-17`）：
/// `if (PlayerActor.IsGuildMaster()) PlayerActor.ClientGuildAlly();`
///
/// - 非会长 ⇒ oracle 不进 `ClientGuildAlly` ⇒ **零帧**（本函数照抄）；
/// - 会长 ⇒ `ClientGuildAlly`（`PlayObject.Operate.cs:1465-1523`）会发
///   `SM_GUILDMAKEALLY_OK` / `SM_GUILDMAKEALLY_FAIL(n8)`（n8 = -1..-4），成功支还
///   `SendServerGroupMsg(SS_207)` + 行会消息 + 行会态改写 ⇒ 依赖**可写行会态**与 G11 跨服，
///   宿主未建模（whitelist **T-263**，本臂只留 `log_note`）。
fn auth(world: &GmWorld<'_>) -> GmResult {
    if world.is_guild_master(world.speaker) {
        return no_frames(
            "Auth 会长路径：ClientGuildAlly（Operate.cs:1465-1523，SM_GUILDMAKEALLY_OK/_FAIL(n8) + SS_207 + 行会态改写）宿主未接（whitelist T-263）",
        );
    }
    no_frames("Auth 非会长 ⇒ oracle 不进 ClientGuildAlly ⇒ 零帧（AuthCommand.cs:14）")
}

/// `AuthallyCommand.Execute`（`AuthallyCommand.cs:9-23`）：
/// `if (PlayerActor.IsGuildMaster()) { /* 翻转 EnableAuthAlly + 两条 SysMsg 全被注释 */ } return;`
/// ⇒ **两条分支都零帧**（真 no-op）；这里仍按 `IsGuildMaster` 分两条 note，便于窗口定位。
fn authally(world: &GmWorld<'_>) -> GmResult {
    if world.is_guild_master(world.speaker) {
        return no_frames(
            "Authally 会长支：EnableAuthAlly 翻转与两条 SysMsg 在 AuthallyCommand.cs:12-21 全被注释 ⇒ 零帧",
        );
    }
    no_frames("Authally 非会长 ⇒ AuthallyCommand.cs:12 不成立 ⇒ return（零帧）")
}

/// `AuthCancelCommand.Execute`（`AuthCancelCommand.cs:12-21`）：
/// ```text
/// if (@params == null || @params.Length <= 0) return;
/// if (PlayerActor.IsGuildMaster()) PlayerActor.ClientGuildBreakAlly(@params[0]);
/// ```
/// 空参 / 非会长 ⇒ **零帧**；会长 ⇒ `ClientGuildBreakAlly`（`PlayObject.Operate.cs:1525-1560`）
/// 发 `SM_GUILDBREAKALLY_OK` / `SM_GUILDMAKEALLY_FAIL(0)` + `SS_207` + 行会态改写 ⇒ 未建模（T-263）。
fn auth_cancel(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    if parameters.trim().is_empty() {
        return no_frames("AuthCancel 空参 ⇒ AuthCancelCommand.cs:14-17 return（零帧）");
    }
    if world.is_guild_master(world.speaker) {
        return no_frames(
            "AuthCancel 会长路径：ClientGuildBreakAlly（Operate.cs:1525-1560，SM_GUILDBREAKALLY_OK / SM_GUILDMAKEALLY_FAIL + SS_207 + 行会态改写）宿主未接（whitelist T-263）",
        );
    }
    no_frames("AuthCancel 非会长 ⇒ oracle return ⇒ 零帧（AuthCancelCommand.cs:18-21）")
}

/// 单条 `SysMsg` + `log_note`（oracle 文本保持逐字节）。
fn one_note(color: GmColor, text: impl Into<String>, note: &'static str) -> GmResult {
    GmResult {
        found: true,
        parsed: true,
        msgs: vec![GmMsg {
            color,
            text: text.into(),
        }],
        frames: Vec::new(),
        log_note: Some(note),
    }
}

// ---------------------------- 族 P · 密码锁门（7，卡 `openmir2-gm-family-p-passwordlock`）----------------------------
//
// oracle：`SetPassWordCommand.cs`（6 类同文件：SetPassWord `:17` / UnPassWord `:44` /
// ChgpassWord `:70` / UnlockStorage `:103` / UnLock `:143` / Lock `:183` 同一句配置门）+
// `PasswordLockCommand.cs:21`。部署 `Server.conf:382 PasswordLockSystem=0`
// （`GameSvrConf.cs:1489` 缺省 false，`ServerConf.cs:570` 读 `[Setup]`）⇒ 过权限门
// （7 条全 perm 10：`GameCommands.cs:381-388` 构造缺省，`[Permission]` 段对 7 条都无键）后
// 命令体第一句恒 `SysMsg(NoPasswordLockSystemMsg, Red, Hint)` —— 零状态、零额外帧。
//
// 三层名链（`CommandSystem.cs:39-113`）：`Command.conf [Command]` 把 6 条改成混淆现网名
// （`StorageSetPassword=hjhgj67` / `StorageUserClearPassword=sadsa54` / `StorageChgPassword=ghjhg6` /
// `StorageUnLock=hjhg556` / `PasswordUnLock=gfh678dfg` / `StorageLock=fdgfdg56`），
// `PasswordLock=PasswordLock` 不改名。
//
// **启用分支不实现**（卡面 forbidden：RM_PASSWORD 设/改/解密状态机 + `Lock` 的
// `IsPasswordLocked`/`IsCanGetBackItem` 写面）：部署门恒 0 ⇒ 该面对现网不可达，只 `log_note` 登记。

/// 密码锁族 7 命令的共享命令体：配置门红字 + 不可达启用支登记。
///
/// 各类命令体逐行：`if (!SystemShare.Config.PasswordLockSystem) { SysMsg(...Red...); return; }`
/// 之后才是各自的状态机——部署取值恒 false ⇒ 状态机段现网不可达。
fn password_lock_gate(parameters: &str, cfg: &GmConfig) -> GmResult {
    // `PasswordLockCommand.cs:17-19` 的前置静默支（**仅** PasswordLock 类有；其余 6 类无此支）：
    // `if (@params == null || @params.Length <= 0) return;`。oracle `GameCommand.Handle`
    // （`GameCommand.cs:46`）case 2 恒 `parameters.Split(' ')`，而 C# `"".Split(' ')` 也返回
    // 长度 1 的数组（`[""]`）⇒ `@params.Length <= 0` 对客户端路径**恒假**（死分支照抄）。
    // Rust `split` 同样恒产 ≥ 1 段 ⇒ 本支永不命中——两侧由同一语言事实证明死支。
    if parameters.split(' ').next().is_none() {
        return GmResult::empty();
    }
    if !cfg.password_lock_system {
        return red(NO_PASSWORD_LOCK_SYSTEM_MSG);
    }
    no_frames(
        "密码锁启用分支未实现：PasswordLockSystem=true 时 oracle 走 RM_PASSWORD 设/改/解密状态机与 IsPasswordLocked/IsCanGetBackItem 写面（SetPassWordCommand.cs / PasswordLockCommand.cs）；部署 Server.conf:382 PasswordLockSystem=0 ⇒ 现网不可达（卡 openmir2-gm-family-p-passwordlock forbidden）",
    )
}

// ---------------------------- 族 T · 会话开关/骑马（7，卡 `openmir2-gm-family-t-toggle-horse`）----------------------------
//
// oracle：四条 toggle（LetTrade/LetGuild/LetShout/BanGuildChat，体形全同：翻转布尔 +
// `if (X) SysMsg(EnableXxxMsg, Green) else SysMsg(DisableXxxMsg, Green)`）+ 两条骑马
// （TakeOffHorse/TakeOnHorse）+ `DisableFilterCommand`（全局 `SystemShare.FilterWord`）。
// 7 条全是 `Execute(IPlayerActor)` **单参**（`GameCommand.Handle` case 1：不拆参数串）；
// 生效名/权限三层链：`Command.conf [Command]`（LetShout=拒绝组队聊天 / LetTrade=交易 /
// LetGuild=加入门派 / BanGuildChat=禁止行会信息 / TakeOnHorse=OnHorse / TakeOffHorse=OffHorse /
// DisableFilter 不改名），perm 全 10（`GameCommands.cs:366-368/370/399-400/460` 构造缺省；
// `[Permission]` 段只有 DisableFilter=10，与其缺省相同）。
//
// **常量名与状态语义相反**（oracle 原样）：`if (BanShout) → EnableShoutMsg`、
// `if (BanGuildChat) → EnableGuildChat` —— 「拒聊位开」显示「允许群聊」。1:1 照抄不修。
//
// 消费点（`PlayObject.Operate` 交易门/加会门等）语义本卡**不改**（卡面 forbidden）：
// 命令体只翻转会话镜像 + 回执。

/// `LetTradeCommand.Execute`（`LetTradeCommand.cs:10-22`，现网 `@交易`）。
fn let_trade(sess: &mut GmSession) -> GmResult {
    sess.allow_deal = !sess.allow_deal;
    green(if sess.allow_deal {
        ENABLE_DEAL_MSG
    } else {
        DISABLE_DEAL_MSG
    })
}

/// `LetGuildCommand.Execute`（`LetGuildCommand.cs:10-23`，现网 `@加入门派`）。
fn let_guild(sess: &mut GmSession) -> GmResult {
    sess.allow_guild = !sess.allow_guild;
    green(if sess.allow_guild {
        ENABLE_JOIN_GUILD
    } else {
        DISABLE_JOIN_GUILD
    })
}

/// `LetShoutCommand.Execute`（`LetShoutCommand.cs:10-22`，现网 `@拒绝组队聊天`）。
fn let_shout(sess: &mut GmSession) -> GmResult {
    sess.ban_shout = !sess.ban_shout;
    green(if sess.ban_shout {
        ENABLE_SHOUT_MSG
    } else {
        DISABLE_SHOUT_MSG
    })
}

/// `BanGuildChatCommand.Execute`（`BanGuildChatCommand.cs:10-22`，现网 `@禁止行会信息`）。
fn ban_guild_chat(sess: &mut GmSession) -> GmResult {
    sess.ban_guild_chat = !sess.ban_guild_chat;
    green(if sess.ban_guild_chat {
        ENABLE_GUILD_CHAT
    } else {
        DISABLE_GUILD_CHAT
    })
}

/// `TakeOffHorseCommand.Execute`（`TakeOffHorseCommand.cs:10-19`，现网 `@OffHorse`）：
/// `if (!OnHorse) return;`（静默零帧）⇒ `OnHorse = false; FeatureChanged();`。
/// `FeatureChanged()`（`BaseObject.cs:1509-1512`）= `SendRefMsg(RM_FEATURECHANGED,
/// GetFeatureEx(), GetFeatureToLong(), …)` ⇒ 视距广播 `SM_FEATURECHANGED(41)`；本仓命令体纯
/// ⇒ 翻转镜像后置 [`GmSession::pending_feature_changed`]，宿主同一事件内广播。
fn take_off_horse(sess: &mut GmSession) -> GmResult {
    if !sess.on_horse {
        return GmResult::empty();
    }
    sess.on_horse = false;
    sess.pending_feature_changed = true;
    GmResult::empty()
}

/// `TakeOnHorseCommand.Execute`（`TakeOnHorseCommand.cs:12-28`，现网 `@OnHorse`）：
/// `if (OnHorse) return;`（已骑马静默）⇒ `HorseType == 0` 红字（现网唯一可达支：
/// oracle `HorseType` 全仓无赋值点，见 `feature.rs` §4）⇒ 上马支 `OnHorse = true;
/// FeatureChanged(); if (OnHorse) SystemShare.FunctionNPC.GotoLable(PlayerActor, "@OnHorse", false)`。
/// 跳线是**宿主接缝**（部署 `FunctionNPC` 在场 ⇒ oracle 无 NRE；脚本面归脚本卡，本卡 forbidden
/// 只登记）。
fn take_on_horse(sess: &mut GmSession) -> GmResult {
    if sess.on_horse {
        return GmResult::empty();
    }
    if sess.horse_type == 0 {
        return red(ON_HORSE_NEED_TAG);
    }
    sess.on_horse = true;
    sess.pending_feature_changed = true;
    no_frames(
        "TakeOnHorse：FeatureChanged 外观帧已置 pending（宿主同一事件内广播 SM_FEATURECHANGED）；随后 FunctionNPC.GotoLable(PlayerActor,\"@OnHorse\",false) 脚本跳线未实现（卡 openmir2-gm-family-t-toggle-horse forbidden：脚本面归脚本卡；部署 FunctionNPC 在场 ⇒ oracle 该调用无 NRE）",
    )
}

/// `DisableFilterCommand.Execute`（`DisableFilterCommand.cs:12-22`，现网 `@DisableFilter`）：
/// `SystemShare.FilterWord = !SystemShare.FilterWord`——**全局位**（进程单例，跨会话同一份），
/// 不是 `PlayObject` 字段。部署启动 true（`GameApp.cs:70`）⇒ 现网第一条回「已禁止文字过滤。」。
/// 全局单元由宿主注入（[`GmWorld::filter_word`]）；未注入 ⇒ 登记零帧（不伪造翻转）。
fn disable_filter(world: &GmWorld<'_>) -> GmResult {
    match world.filter_word {
        Some(cell) => {
            let v = !cell.get();
            cell.set(v);
            green(if v {
                DISABLE_FILTER_ENABLED
            } else {
                DISABLE_FILTER_DISABLED
            })
        }
        None => no_frames(
            "DisableFilter：SystemShare.FilterWord 全局位未注入（构造处未接全局单元）⇒ 零帧登记（不伪造翻转）",
        ),
    }
}

// ---------------------------- 族 Q · 查询/展示（10，卡 `openmir2-gm-family-q-query-display`）----------------------------
//
// oracle：HumanCount/MobCount/MapInfo(Map 类)/ShowMapMode/ShowHumanFlag/HumanLocal/HumanInfo/
// ShowUseItemInfo 八条 `Execute(string[], IPlayerActor)`（case 2 拆参）+ ShowMapInfo/
// LotteryTicketCommandL 两条 `Execute(IPlayerActor)`（case 1 不拆参）。
// 现网名三层链：`HumanCount=HumanCountwa` / `MobCount=MobCountwa` / `Map=Map`（类名 MapInfo）/
// `MapInfo=MapInfo`（类 ShowMapInfo）/ `ShoweMapMode=ShowMapMode`（conf 键是死键拼写）/
// `Flag=showflag` / `HumanLocal=IP` / `Info=信息` / `LotteryTicket=LotteryTicket`（出厂名
// `Namecolor` 被覆盖）/ ShowUseItemInfo 无键 ⇒ 出厂名 `Showuseiteminfo`。perm 全 10
// （`[Permission]` HumanLocal/MobCount/HumanCount/Map/LotteryTicket/MapInfo/ShoweMapMode/Info/Flag
// 与构造缺省一致；ShowUseItemInfo 无键 ⇒ 构造 10）。**只读命令：不写任何状态**（卡面 forbidden）。
//
// 四个怪癖逐条照抄（不许「修」，内容冻结）：① IP 判空恒真；② 信息在线零帧（SysMsg 被注释）；
// ③ Map 直取 `@params[2]` 越界零帧 + 图不存在/格子无效静默；④ Showuseiteminfo `sHumanName[1]`
// 单字符名越界零帧 + `%s/%d` 格式串字面输出。

/// .NET `string.Format(template, args)` 的 {n} 占位替换：**单遍扫描**，值原样插入、
/// 不再参与后续展开（与 C# 语义一致——值里带 `{1}` 不会被二次替换）。
fn dotnet_format(template: &str, args: &[String]) -> String {
    let b = template.as_bytes();
    let mut out =
        String::with_capacity(template.len() + args.iter().map(String::len).sum::<usize>());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            let mut j = i + 1;
            let mut idx = 0usize;
            let mut has_digit = false;
            while j < b.len() && b[j].is_ascii_digit() {
                idx = idx * 10 + usize::from(b[j] - b'0');
                j += 1;
                has_digit = true;
            }
            if has_digit && j < b.len() && b[j] == b'}' && idx < args.len() {
                out.push_str(&args[idx]);
                i = j + 1;
                continue;
            }
        }
        // 非占位（含 `{` 后不合法）按字面拷贝——C# 里模板写坏会抛 FormatException 被吞，
        // 但本仓全部模板都是常量、占位合法，这条字面支只为健全性。
        let ch_len = template[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&template[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// `CellAttribute` 枚举名的 C# `ToString()` 字面（`标志: {name}` 用）。
/// `GetCellInfo` success 门要求 `Valid`（= Walk）⇒ 命令实际只可能输出 `Walk`；
/// 另两名为结构完备保留。
fn cell_attribute_name(a: mir2_world::CellAttribute) -> &'static str {
    match a {
        mir2_world::CellAttribute::Walk => "Walk",
        mir2_world::CellAttribute::HighWall => "HighWall",
        mir2_world::CellAttribute::LowWall => "LowWall",
    }
}

/// `MapManager.FindMap(sMapName)` 的两级判定：`Some(None)` = 注册表在、图不在（oracle
/// `FindMap` 落空 ⇒ 调用方走「图不存在」红字/静默）；`None` = 注册表未注入（构造缺口 ⇒
/// 调用方按 [`NOTE_NO_MAP_REGISTRY`] 登记零帧，不当「图不存在」）；`Some(Some(id))` = 命中。
fn find_map_registered(world: &GmWorld<'_>, name: &str) -> Option<Option<usize>> {
    let reg = world.maps.as_ref()?;
    Some(reg.id_of(name))
}

/// `WorldServer.GetMapHuman(sMapName)`（`WorldServer.cs:1678-1694`）：!Death && !Ghost &&
/// `Envir == envir` 的 PlayObjectList 计数。世界未注入 ⇒ `None`（[`NOTE_NO_WORLD`]）。
fn map_human_count(world: &GmWorld<'_>, map_id: usize) -> Option<i32> {
    let w = world.world?;
    Some(
        w.entities
            .iter()
            .filter(|e| {
                e.race == mir2_world::ACTOR_RACE_PLAY && !e.death && !e.ghost && e.map_id == map_id
            })
            .count() as i32,
    )
}

/// `WorldServer.GetMapMonster(envir, null)`（`WorldServer.cs:1639-1677`）：!Death && !Ghost &&
/// `Envir == envir` 的 MonGen CertList（怪物）计数。宿主怪物是普通实体 ⇒ 按
/// `race >= ACTOR_RACE_ANIMAL`（oracle `Envirnoment.AddObject` 的怪物判据同式）过滤镜像；
/// NPC（10/15）天然排除。
fn map_monster_count(world: &GmWorld<'_>, map_id: usize) -> Option<i32> {
    let w = world.world?;
    Some(
        w.entities
            .iter()
            .filter(|e| {
                e.race >= mir2_world::ACTOR_RACE_ANIMAL
                    && !e.death
                    && !e.ghost
                    && e.map_id == map_id
            })
            .count() as i32,
    )
}

/// 目标 `GetQuestFalgStatus(nFlag)`（`PlayObject.cs:3966-3989`：`nFlag-1` → 位
/// `128 >> (n-1)%8` 于字节 `(n-1)/8`）的**读源接缝**。宿主未建模 `QuestFlag` 位阵
/// （族 F 的 setflag/setopen/setunit 尚未落地、无任何写点）⇒ 恒零值模型：任何目标/任何
/// 位号读 0（= 新角色位阵全零语义）。族 F 落地时把本函数换成真位阵读，`showflag` 的
/// ON 支随之可达——命令体结构已按 oracle 写全，不随接缝改动。
fn target_quest_flag_status(_target: &mir2_world::Entity, _n_flag: i32) -> i32 {
    0
}

/// `HumanCountCommand.Execute`（`HumanCountCommand.cs:13-33`，现网 `@HumanCountwa`）。
fn human_count(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let s_map = split_params(parameters).first().copied().unwrap_or("");
    if s_map.is_empty() {
        return red(Q_HELP_HUMAN_COUNT);
    }
    let Some(map_id) = find_map_registered(world, s_map) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let Some(map_id) = map_id else {
        return red(MOB_COUNT_MAP_NOT_FOUND);
    };
    let Some(n) = map_human_count(world, map_id) else {
        return no_frames(NOTE_NO_WORLD);
    };
    green(dotnet_format(MOB_COUNT_MONSTER_COUNT_FMT, &[n.to_string()]))
}

/// `MobCountCommand.Execute`（`MobCountCommand.cs:13-33`，现网 `@MobCountwa`）。
fn mob_count(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let s_map = split_params(parameters).first().copied().unwrap_or("");
    if s_map.is_empty() {
        return red(Q_HELP_MOB_COUNT);
    }
    let Some(map_id) = find_map_registered(world, s_map) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let Some(map_id) = map_id else {
        return red(MOB_COUNT_MAP_NOT_FOUND);
    };
    let Some(n) = map_monster_count(world, map_id) else {
        return no_frames(NOTE_NO_WORLD);
    };
    green(dotnet_format(MOB_COUNT_MONSTER_COUNT_FMT, &[n.to_string()]))
}

/// `MapInfoCommand.Execute`（`MapInfoCommand.cs:12-46`，现网 `@Map`）。
///
/// 支结构（含怪癖）：`sMap = @params[2]` **直取下标** ⇒ 实参 <3 抛越界被吞 ⇒ 零帧；
/// `!IsNullOrEmpty(sMap) && nX>=0 && nY>=0` 不成立 ⇒ **绿**字格式提示（Name=现网名 `Map`）；
/// 图不存在 ⇒ **静默**（无 else；`IsValidCell` 的 false 臂不可达——success ⇒ Walk ⇒ 永非
/// LowWall，见 [`q_map`] 内注）；`GetCellInfo` success ⇒ 绿字 `标志: {Attribute}`（success 门
/// 要求 Valid ⇒ 恒 `Walk`）+ `IsAvailable` 再补绿字 `对象数: {Count}`；失败（HighWall/
/// LowWall/越界）⇒ 红字 `取地图单元信息失败: {图名}`。
fn q_map(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let Some(&s_map) = params.get(2) else {
        return no_frames(NOTE_INDEX_OUT_OF_RANGE);
    };
    let n_x = str_to_int16(params.first().copied().unwrap_or(""), 0);
    let n_y = str_to_int16(params.get(1).copied().unwrap_or(""), 0);
    if s_map.is_empty() || n_x < 0 || n_y < 0 {
        return green("请按正确格式输入: Map 地图号 X Y");
    }
    let Some(map_id) = find_map_registered(world, s_map) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let Some(map_id) = map_id else {
        // oracle：`map == null` 时该 if 整支不进、也没有 else ⇒ 静默。
        return GmResult::empty();
    };
    let Some(w) = world.world else {
        return no_frames(NOTE_NO_WORLD);
    };
    let Some(grid) = w.maps.get(map_id) else {
        return no_frames(NOTE_NO_WORLD);
    };
    // oracle 判据逐行（`Envirnoment.cs:1422-1429/322-335` + `MapCellInfo.cs:19`）：
    // `Valid` = `Attribute == Walk` ⇒ `GetCellInfo` 对「越界或非 Walk」一律 `success=false`；
    // `IsValidCell` 的 false 臂（`success && Attribute==LowWall`）因此**不可达**（success ⇒
    // Walk ⇒ 永非 LowWall）⇒ `@Map` 的静默支只由 `FindMap == null` 到达；其余一切非可走格
    // （HighWall/LowWall/越界）都走 `success=false` ⇒ 红字失败。
    let (cell, success) = grid.get_cell_info(i32::from(n_x), i32::from(n_y));
    if !success {
        return red(format!("取地图单元信息失败: {s_map}"));
    }
    let mut r = GmResult::one(
        GmColor::Green,
        format!("标志: {}", cell_attribute_name(cell.attribute)),
    );
    if cell.is_available() {
        r.msgs.push(GmMsg {
            color: GmColor::Green,
            text: format!("对象数: {}", cell.count()),
        });
    }
    r
}

/// `ShowMapInfoCommand.Execute`（`ShowMapInfoCommand.cs:10-17`，现网 `@MapInfo`）：
/// 说话者当前图的两条绿字。
fn show_map_info(world: &GmWorld<'_>) -> GmResult {
    let Some((map_id, _, _)) = world.self_pos() else {
        return no_frames(NOTE_NO_WORLD);
    };
    let Some(reg) = world.maps else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let Some(name) = reg.name_of(map_id).map(str::to_string) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let desc = reg.map_desc_of(map_id).unwrap_or_default().to_string();
    let Some(map_file) = reg.maps().get(map_id) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    GmResult {
        found: true,
        parsed: true,
        msgs: vec![
            GmMsg {
                color: GmColor::Green,
                text: dotnet_format(Q_MAP_INFO_NAME_FMT, &[name, desc]),
            },
            GmMsg {
                color: GmColor::Green,
                text: dotnet_format(
                    Q_MAP_INFO_SIZE_FMT,
                    &[map_file.width.to_string(), map_file.height.to_string()],
                ),
            },
        ],
        frames: Vec::new(),
        log_note: None,
    }
}

/// `Envirnoment.GetEnvirInfo()`（`Envirnoment.cs:1432-1448`）逐段重建：
/// `HUtil32.BoolToStr` = `TRUE`/`FALSE`；`EXPRATE` 的第二槽是 `Flag.ExpRate / 100`（**整数除**）。
fn get_envir_info(name: &str, desc: &str, f: &mir2_envir::MapInfoFlag) -> String {
    let bt = |v: bool| if v { "TRUE" } else { "FALSE" };
    format!(
        "Map:{name}({desc}) DAY:{0} DARK:{1} SAFE:{2} FIGHT:{3} FIGHT3:{4} QUIZ:{5} \
         NORECONNECT:{6}({7}) MUSIC:{8}({9}) EXPRATE:{10}({11}) PKWINLEVEL:{12}({13}) \
         PKLOSTLEVEL:{14}({15}) PKWINEXP:{16}({17}) \
         PKLOSTEXP:{18}({19}) DECHP:{20}({21}/{22}) INCHP:{23}({24}/{25}) \
         DECGAMEGOLD:{26}({27}/{28}) INCGAMEGOLD:{29}({30}/{31}) INCGAMEPOINT:{32}({33}/{34}) \
         RUNHUMAN:{35} RUNMON:{36} NEEDHOLE:{37} NORECALL:{38} \
         NOGUILDRECALL:{39} NODEARRECALL:{40} NOMASTERRECALL:{41} NODRUG:{42} MINE:{43} \
         MINE2:{44} NODROPITEM:{45} NOTHROWITEM:{46} NOPOSITIONMOVE:{47} NOHORSE:{48} \
         NOHUMNOMON:{49} NOCHAT:{50}",
        bt(f.day_light),
        bt(f.bo_darkness),
        bt(f.safe_area),
        bt(f.fight_zone),
        bt(f.fight3_zone),
        bt(f.bo_quiz),
        bt(f.bo_noreconnect),
        f.s_no_reconnect_map,
        bt(f.music),
        f.music_id,
        bt(f.bo_exprate),
        f.exp_rate / 100,
        bt(f.bo_pkwinlevel),
        f.n_pkwinlevel,
        bt(f.bo_pklostlevel),
        f.n_pklostlevel,
        bt(f.bo_pkwinexp),
        f.n_pkwinexp,
        bt(f.bo_pklostexp),
        f.n_pklostexp,
        bt(f.bo_dechp),
        f.n_dechptime,
        f.n_dechppoint,
        bt(f.bo_inchp),
        f.n_inchptime,
        f.n_inchppoint,
        bt(f.bo_decgamegold),
        f.n_decgamegoldtime,
        f.n_decgamegold,
        bt(f.bo_incgamegold),
        f.n_incgamegoldtime,
        f.n_incgamegold,
        bt(f.bo_incgamepoint),
        f.n_incgamepointtime,
        f.n_incgamepoint,
        bt(f.run_human),
        bt(f.run_mon),
        bt(f.bo_needhole),
        bt(f.no_re_call),
        bt(f.no_guild_re_call),
        bt(f.bo_nodearrecall),
        bt(f.master_re_call),
        bt(f.bo_nodrug),
        bt(f.mine),
        bt(f.bo_mine2),
        bt(f.no_drop_item),
        bt(f.no_throw_item),
        bt(f.bo_nopositionmove),
        bt(f.no_horse),
        bt(f.bo_nohumnomon),
        bt(f.bo_nochat),
    )
}

/// `ShowMapModeCommand.Execute`（`ShowMapModeCommand.cs:13-35`，现网 `@ShowMapMode`）。
fn show_map_mode(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let s_map = split_params(parameters).first().copied().unwrap_or("");
    if s_map.is_empty() {
        return red(Q_HELP_SHOW_MAP_MODE);
    }
    let Some(map_id) = find_map_registered(world, s_map) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let Some(map_id) = map_id else {
        return red(format!("{s_map} 不存在!!!"));
    };
    let Some(reg) = world.maps else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let Some(name) = reg.name_of(map_id) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let desc = reg.map_desc_of(map_id).unwrap_or_default();
    let Some(flag) = reg.flag_of(map_id) else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    blue(format!("地图模式: {}", get_envir_info(name, desc, flag)))
}

/// `ShowHumanFlagCommand.Execute`（`ShowHumanFlagCommand.cs:14-41`，现网 `@showflag`）。
fn show_flag(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let s_human = params.first().copied().unwrap_or("");
    let s_flag = params.get(1).copied().unwrap_or("");
    if s_human.is_empty() || s_human.starts_with('?') {
        return red(Q_HELP_SHOW_FLAG);
    }
    let Some(target) = world.play_by_name(s_human) else {
        return red(dotnet_format(NOW_NOT_ON_LINE_FMT, &[s_human.to_string()]));
    };
    let n_flag = str_to_int(s_flag, 0);
    // 读源接缝：宿主位阵恒零值模型 ⇒ 恒走 OFF 支（ON 支结构保留，见 seam 文档）。
    let status = target_quest_flag_status(target, n_flag);
    if status == 1 {
        green(dotnet_format(
            Q_FLAG_ON_FMT,
            &[target.name.clone(), n_flag.to_string()],
        ))
    } else {
        green(dotnet_format(
            Q_FLAG_OFF_FMT,
            &[target.name.clone(), n_flag.to_string()],
        ))
    }
}

/// `HumanLocalCommand.Execute`（`HumanLocalCommand.cs:14-36`，现网 `@IP`）：
/// **判空恒真怪癖**——`IsNullOrEmpty(sHumanName) || !IsNullOrEmpty(sHumanName)` 恒成立
/// ⇒ 任何输入同答红字帮助；查在线/IP 支全部不可达（照抄不修）。
fn human_local(_world: &GmWorld<'_>) -> GmResult {
    red(Q_HELP_HUMAN_LOCAL)
}

/// `HumanInfoCommand.Execute`（`HumanInfoCommand.cs:14-36`，现网 `@信息`）：
/// 空名/`?` 名红字帮助；离线红字；**在线零帧**（末行 SysMsg 被注释——非注释是没写，
/// 1:1 只留静默）。
fn human_info(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let s_human = split_params(parameters).first().copied().unwrap_or("");
    if s_human.is_empty() || s_human.starts_with('?') {
        return red(Q_HELP_HUMAN_INFO);
    }
    if world.play_by_name(s_human).is_none() {
        return red(dotnet_format(NOW_NOT_ON_LINE_FMT, &[s_human.to_string()]));
    }
    GmResult::empty()
}

/// `ShowUseItemInfoCommand.Execute`（`ShowUseItemInfoCommand.cs:13-41`，现网 `@Showuseiteminfo`）：
/// `sHumanName[1]` 判空下标怪癖（单字符名越界 ⇒ 零帧）+ 离线红字 + 在线逐**非空佩戴位**
/// 发**字面**蓝字（.NET `string.Format` 无 `{n}` 占位 ⇒ 每件同文）。
fn show_use_item_info(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let s_human = split_params(parameters).first().copied().unwrap_or("");
    if s_human.is_empty() {
        return red(Q_HELP_SHOW_USE_ITEM);
    }
    match second_utf16_is_question(s_human) {
        Some(true) => return red(Q_HELP_SHOW_USE_ITEM),
        Some(false) => {}
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
    }
    let Some(target) = world.play_by_name(s_human) else {
        return red(dotnet_format(NOW_NOT_ON_LINE_FMT, &[s_human.to_string()]));
    };
    let mut r = GmResult::empty();
    for slot in &target.use_items {
        if slot.index == 0 {
            continue;
        }
        r.msgs.push(GmMsg {
            color: GmColor::Blue,
            text: Q_SHOW_USE_ITEM_LITERAL.to_string(),
        });
    }
    r
}

/// `LotteryTicketCommandL.Execute`（`LotteryTicketCommand.cs:11-19`，现网 `@LotteryTicket`）：
/// 一条绿字 8 配置值（部署全 0），格式串尾随一个空格照抄。
fn lottery_ticket(cfg: &GmConfig) -> GmResult {
    green(dotnet_format(
        LOTTERY_MSG_FMT,
        &[
            cfg.win_lottery_count.to_string(),
            cfg.no_win_lottery_count.to_string(),
            cfg.win_lottery_level1.to_string(),
            cfg.win_lottery_level2.to_string(),
            cfg.win_lottery_level3.to_string(),
            cfg.win_lottery_level4.to_string(),
            cfg.win_lottery_level5.to_string(),
            cfg.win_lottery_level6.to_string(),
        ],
    ))
}

// ---------------------------- 族 X1 · 禁言名单（5，卡 `openmir2-gm-family-x1-denywhisper`）----------------------------
//
// oracle：DisableSendMsgCommand（现网 `+jy`）/ EnableSendMsgCommand（`-jy`）/
// DisableSendMsgListCommand（`+-jy`）/ ShutupListCommand（`查看禁言`）+ ViewWhisperCommand
// （`查看私聊`）。perm 全 10（`[Permission]` DisableSendMsg/EnableSendMsg/DisableSendMsgList/
// ShutupList/ViewWhisper 与构造缺省一致）。
//
// **运行层真相（review F1，Leader 复核成立）**：`SystemShare.DisableSendMsgList`
// （`SystemShare.cs:103`）与 `SystemShare.DenySayMsgList`（`:64`）**声明即 null、全仓零赋值**
// （初始化只在 M2Share 孪生字段，`GameApp.cs:26/38`）⇒ 触表的四个命令体真机 NRE、被
// `PlayObject.Chat.cs:402` 顶层 catch 吞 ⇒ **零帧**——源文本的恒字/绿字分支真机不可达。
// 前一轮误按源文本建模恒字帧，rework 改零帧 + log_note（`NOTE_X1_NULL_TABLE` 范式）。
// 对照：聊天禁言门（`PlayObject.Chat.cs:137`）走 **M2Share** 孪生字段（非 null 空表）⇒ 无害；
// `ViewWhisper` 只触已初始化的 WorldEngine（`GameApp.cs:93`）与 actor 字段 ⇒ 不受影响。
//
// 名单注入链整体不接（rework ③）：命令体不再读任何名单容器 ⇒ `GmWorld` 不建模
// `DisableSendMsgList`/`DenySayMsgList`（`DisableSendMsgList.txt` 的读/写面在 oracle 侧
// 亦为全注释/落盘 forbidden，无注入价值）。

/// `DisableSendMsgCommand.Execute`（`DisableSendMsgCommand.cs:12-33`，现网 `@+jy`）。
///
/// 空名红帮助可达（`:20-23` 不触表）；非空：`:26` 在线置**目标** `FilterSendMsg = true`
/// （WorldEngine 已初始化 ⇒ 可达、**真实副作用、非帧**）⇒ `:28` 对 null 表 Add 抛 NRE ⇒
/// `PlayObject.Chat.cs:402` 吞 ⇒ **零帧**（绿字/名单 Add/Save 落盘全部不可达）。
fn disable_send_msg(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let s_human = split_params(parameters).first().copied().unwrap_or("");
    if s_human.is_empty() {
        return red(X1_HELP_DISABLE_SEND_MSG);
    }
    if world.play_by_name(s_human).is_some() {
        no_frames(
            "+jy：:26 已置目标 FilterSendMsg=true（真实副作用、非帧）⇒ :28 SystemShare.DisableSendMsgList.Add 对 null 表抛 NRE ⇒ Chat.cs:402 顶层 catch 吞 ⇒ 零帧（review F1；SystemShare.cs:103 声明即 null 全仓零赋值）",
        )
    } else {
        no_frames(
            "+jy：目标离线 ⇒ :25-27 不置位 ⇒ :28 SystemShare.DisableSendMsgList.Add 对 null 表抛 NRE ⇒ Chat.cs:402 顶层 catch 吞 ⇒ 零帧（review F1）",
        )
    }
}

/// `DisableSendMsgListCommand.Execute`（`DisableSendMsgListCommand.cs:11-29`，现网 `@+-jy`）：
/// 首句 `:13` 读 `SystemShare.DisableSendMsgList.Count` 对 null 即 NRE ⇒ **恒零帧**
/// （源文本「禁言列表为空!!!」/「禁言列表:」两支真机均不可达）。
fn disable_send_msg_list(_world: &GmWorld<'_>) -> GmResult {
    no_frames(NOTE_X1_NULL_TABLE)
}

/// `EnableSendMsgCommand.Execute`（`EnableSendMsgCommand.cs:13-40`，现网 `@-jy`）：
/// 空名红帮助可达；非空 `:26` for-init 读 null 表 Count 即 NRE ⇒ **零帧**
/// （源文本恒红「{名} 没有被禁言!!!」真机不可达）。
fn enable_send_msg(parameters: &str) -> GmResult {
    let s_human = split_params(parameters).first().copied().unwrap_or("");
    if s_human.is_empty() {
        return red(X1_HELP_ENABLE_SEND_MSG);
    }
    no_frames(NOTE_X1_NULL_TABLE)
}

/// `ShutupListCommand.Execute`（`ShutupListCommand.cs:13-40`，现网 `@查看禁言`）：
/// 首句 `EnterCriticalSection(SystemShare.DenySayMsgList)` 对 null 即 NRE ⇒ **恒零帧**
/// （源文本恒绿「禁言列表为空!!!」真机不可达；对照 `PlayObject.Chat.cs:137` 走 M2Share
/// 孪生字段非 null ⇒ 聊天禁言门无害——同镜头盲区已开 follow-up 卡
/// `openmir2-gm-systemshare-null-sweep`，含批次 5 `@禁言` 同病）。
fn shutup_list(_world: &GmWorld<'_>) -> GmResult {
    no_frames(NOTE_X1_NULL_TABLE)
}

/// `ViewWhisperCommand.Execute`（`ViewWhisperCommand.cs:14-42`，现网 `@查看私聊`）：
/// 只触已初始化的 WorldEngine 与 actor 字段 ⇒ 全支可达（review A4 成立）：`sChrName[1]`
/// 判空下标怪癖（单字符名越界被吞 ⇒ 零帧）；离线红 NowNotOnLine；在线按 `listening_to`
/// 镜像 toggle ⇒ Msg2「正在侦听…」（开）/ Msg1「已停止侦听…」（关）。
fn view_whisper(parameters: &str, world: &GmWorld<'_>, sess: &mut GmSession) -> GmResult {
    let s_chr = split_params(parameters).first().copied().unwrap_or("");
    if s_chr.is_empty() {
        return red(X1_HELP_VIEW_WHISPER);
    }
    match second_utf16_is_question(s_chr) {
        Some(true) => return red(X1_HELP_VIEW_WHISPER),
        Some(false) => {}
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
    }
    if world.play_by_name(s_chr).is_none() {
        return red(dotnet_format(NOW_NOT_ON_LINE_FMT, &[s_chr.to_string()]));
    }
    if sess.listening_to.as_deref() == Some(s_chr) {
        sess.listening_to = None;
        green(dotnet_format(
            X1_VIEW_WHISPER_STOP_FMT,
            &[s_chr.to_string()],
        ))
    } else {
        sess.listening_to = Some(s_chr.to_string());
        green(dotnet_format(
            X1_VIEW_WHISPER_START_FMT,
            &[s_chr.to_string()],
        ))
    }
}

// ---------------------------- 宿主未接 / oracle 注释的登记文案 ----------------------------
const NOTE_EVENT_MGR: &str =
    "FireBurnEvent+SystemShare.EventMgr 宿主未接 ⇒ 未入事件队列（oracle 帧面同样零帧）";
const NOTE_CHAR_PUSHED: &str =
    "CharPushed 宿主未接 ⇒ 未推挤、未发生 RandomByte(nType) 抽数（tier 若接线须按 R 重评）";
const NOTE_STATUS: &str =
    "StatusTimeArr/CharStatus/StatusChanged 宿主未接 ⇒ 无状态写入、无状态帧（oracle 帧面另有状态广播）";
const NOTE_RELOAD_ADMIN: &str =
    "ReLoadAdmin: WorldEngine.SendServerGroupMsg(213) 宿主未接（单区无 server-group），只回 oracle 文本";
const NOTE_RELOAD_ALL_GUILD: &str =
    "ReloadAllGuild: GuildMgr.LoadGuildInfo 宿主未接，只回 oracle 文本（行会不会被重读）";
const NOTE_RELOAD_GUILD: &str =
    "ReloadGuild: guild.LoadGuild() 宿主未接，只回 oracle 文本（行会不会被重读）";
const NOTE_NOTICE: &str =
    "ReloadLineNotice: LoadLineNotice/LineNoticeList 宿主未接，结果取部署件存在性（Notice/linenotice.txt）";
const NOTE_MANAGE: &str =
    "ReloadManage: ManageNPC/FunctionNPC 的 ClearScript+LoadNPCScript 宿主未接，只回 oracle 文本";
const NOTE_ROBOT_MANAGE: &str =
    "ReloadRobotManage: RobotNPC 的 ClearScript+LoadNPCScript 宿主未接，只回 oracle 文本";

// ---- 传送/召唤族的宿主未接与 oracle 越界登记（whitelist.md T-166..T-171）----
const NOTE_INDEX_OUT_OF_RANGE: &str =
    "oracle 下标越界（`sHumanName[1]`/`@params[0..6]`）被 PlayObject.Chat.cs 的 catch 吞掉 ⇒ 零帧";
/// **扫盲卡 `openmir2-gm-systemshare-null-sweep`**（X1 F1 同镜头系统化推广）：`SystemShare`
/// 声明即 null 的集合字段（穷举见卡上清单：DisableSendMsgList:103 / DenySayMsgList:64 /
/// ItemBind*/Deny*List 等共 29 个 NULL-FOREVER）⇒ 命令体触表即 NRE、被
/// `PlayObject.Chat.cs:402` 顶层 catch 吞 ⇒ 零帧。已修复：批次 5 `@禁言`（X1 后扫盲第一例）。
const NOTE_SYSTEMSHARE_NULL_TABLE: &str =
    "oracle SystemShare 集合字段声明即 null 全仓零赋值 ⇒ 命令体触表即 NRE、被 PlayObject.Chat.cs:402 顶层 catch 吞 ⇒ 零帧（扫盲卡 openmir2-gm-systemshare-null-sweep；源文本分支真机不可达）";
const NOTE_NO_WORLD: &str =
    "GmWorld 未注入在线实体表/世界（S1 占位世界）⇒ 无法判定目标在线、坐标或地图 Flag";

/// `SmakeItemCommand.cs:77`：空槽位的红字回执（`CommandHelp.GamecommandSuperMakeHelpMsg`）。
pub const SMAKE_NO_ITEM_HELP: &str = "身上没指定物品!!!";

/// `SmakeItemCommand.cs:26`：三参越界 ⇒ **静默 return**（oracle 无 else 支）。
const NOTE_SMAKE_RANGE: &str = "SmakeItem：nWhere/nValueType/nValue 越界 ⇒ 静默 return（零帧）";

/// `SmakeItemCommand.cs:36-43`：`nValueType == 16`（吸伤）支**不可达**——`:26` 的范围守卫
/// 只放行 `0..=15`。照抄时省略，登记在此。
const NOTE_SMAKE_ABSORB_DEAD: &str =
    "SmakeItem：nValueType==16 吸伤支被 :26 的 0..=15 守卫挡住（死支）";
/// `ClearBagItemCommand` 的帮助文本：`Command.CommandHelp` = `命令格式: @清理垃圾 人物名称`
/// （`Name` 被 `Command.conf` 的 `[Command] ClearBag=清理垃圾` 覆盖，`Help` = 「人物名称」）。
pub const CLEAR_BAG_HELP: &str = "命令格式: @清理垃圾 人物名称";

/// `ClearBagItemCommand.cs:47-52`：命中且目标背包非空 ⇒ 宿主清包 + 给**目标**发
/// `SM_DELITEMS(709)`（`Recog=0`、`Series`=件数、尾 = `EncodeString("名/makeindex/…")`）。
const NOTE_CLEAR_BAG_PENDING: &str =
    "ClearBagItem：已登记清包目标（空包 ⇒ 不发帧，与 oracle 同；非空由宿主清包并发 709）";

/// `ShowEffectCommand.cs:20-70`：命令体**整段被注释** ⇒ 任何参数都零帧、零副作用。
const NOTE_SHOW_EFFECT_ALL_COMMENTED: &str =
    "ShowEffect：oracle 命令体 20-70 行整段注释 ⇒ 零帧、零副作用（最小 no-op）";
/// `StartQuestCommand` 的帮助文本：`Command.CommandHelp` = `命令格式: @开始提问 问答名称`
/// （`Name` 被 `Command.conf` 的 `[Command] StartQuest=开始提问` 覆盖，`Help` = 「问答名称」）。
pub const START_QUEST_HELP: &str = "命令格式: @开始提问 问答名称";

/// `StartQuestCommand.cs:22`：`SendQuestMsg` 已被注释 ⇒ 非空名零帧、零副作用。
const NOTE_START_QUEST_MSG_COMMENTED: &str =
    "StartQuest：oracle :22 的 WorldEngine.SendQuestMsg 被注释 ⇒ 零帧、零副作用";

/// `KickHumanCommand.cs:21-24`：空名 **静默 return**（oracle 无任何帧，连帮助文本都不发）。
const NOTE_KICK_HUMAN_EMPTY_NAME: &str = "KickHuman：空名字 ⇒ oracle 静默 return（零帧）";

/// `KickHumanCommand.cs:26-31`：命中 ⇒ 只置目标 `BoKickFlag/BoEmergencyClose`，命令本身零回帧；
/// 踢的落地（`SM_OUTOFCONNECTION(528)` + `MakeGhost`）由宿主 `drain_kicks` 在 tick 内收口。
const NOTE_KICK_HUMAN_PENDING: &str =
    "KickHuman：已置目标 BoKickFlag/BoEmergencyClose（命令无回帧，宿主 drain_kicks 落地）";
const NOTE_NO_MAP_REGISTRY: &str =
    "GmWorld 未注入 Envir 地图注册表 ⇒ 无法执行 MapMgr.FindMap（oracle 按 FindMap 结果分支）";
const NOTE_X1_NULL_TABLE: &str =
    "X1 禁言名单命令体：SystemShare.DisableSendMsgList/DenySayMsgList 声明即 null（SystemShare.cs:103/:64）全仓零赋值 ⇒ 触表即 NRE、被 PlayObject.Chat.cs:402 顶层 catch 吞 ⇒ 零帧（review F1，Leader 复核成立；源文本恒字/绿字分支真机不可达）";
const NOTE_RECALL_HUMAN: &str =
    "RecallHuman: GetPlayObject/GetRecallXy/SpaceMove 未接 ⇒ 不移动、不抽 SpaceMoveGetRandXY（oracle 本支零文本）";
const NOTE_MAP_RANDOM_MOVE: &str =
    "MapRandomMove/SpaceMove 未接 ⇒ 不移动、不抽 Random(Width-nEgdey-1)/Random(Height-nEgdey-1)（oracle 本支零文本）";
const NOTE_MAP_RANDOM_MOVE_HOME: &str =
    "MapRandomMove(HomeMap)/SpaceMove 未接 ⇒ 不移动、不抽 RNG；HomeMap 本仓未建模（oracle 本支零文本）";
const NOTE_SPACE_MOVE: &str =
    "SpaceMove 未接 ⇒ 不换图、不抽 SpaceMoveGetRandXY 移位环（oracle 本支零 SysMsg，另有 RM_CLEAROBJECTS/RM_CHANGEMAP/RM_SPACEMOVE_SHOW*）";
const NOTE_TARGET_CAN_DEAR_RECALL: &str =
    "DearHuman.CanDearRecall 是对方会话字段，GmWorld 读不到 ⇒ 无法判定召唤/拒绝支（DearRecallTick 已按 oracle 先置）";
const NOTE_MASTER_RECALL_TARGET: &str =
    "MasterList[*].CanMasterRecall 是对方会话字段，GmWorld 读不到 ⇒ 无法判定召唤/拒绝支";
const NOTE_RECALL_MOB_SPAWN: &str =
    "GetFrontPosition/RegenMonsterByName/SlaveList 未接 ⇒ 不刷宝宝、不写 Master/SlaveExpLevel（oracle 本支零文本）";
const NOTE_MOVE_MOB_TO: &str =
    "MoveMobTo: 两个移动支（含 GetMapRangeMonster/SpaceMove）整段注释 ⇒ oracle 本身就是 no-op";
const NOTE_SIGN_MAP_MOVE: &str = "SignMapMove: oracle 命令体整段注释 ⇒ no-op（零帧）";

// ---------------------------- 封禁名单族（设计件 §4.4 第 5 族）----------------------------
//
// 三张表的**增/删/查/存循环在 oracle 里全部被注释**（`Deny*Command.cs` 的 for 体、
// `M2Share.SaveDenyAccountList` 的写盘体、`LoadDenyAccountList`/`LoadDenyChrNameList`/
// `LoadDenyIPAddrList` 的读入体），所以启动后三表恒空、`boDelete` 恒 false。本仓不建这三张表
// （宿主未接，`whitelist.md` T-162），文本按 oracle 原文发。

/// `DelDenyAccountLogonCommand.Execute`（`DelDenyAccountLogonCommand.cs:12-44`，现网 `@-jid`）。
///
/// 空名红帮助可达（:19-22 不触表）；非空 `:25` for-init 读 null 表即 NRE ⇒ 零帧
/// （源文本绿「{名} 没有被禁止登录。」（:41）真机不可达）。
fn del_deny_account(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let account = params.first().copied().unwrap_or("");
    if account.is_empty() {
        return GmResult::one(GmColor::Red, DEL_DENY_ACCOUNT_HELP.to_string());
    }
    // oracle 真机零帧（扫盲卡 openmir2-gm-systemshare-null-sweep）：:25 for-init 读
    // SystemShare.DenyAccountList（SystemShare.cs:143 声明即 null 全仓零赋值）即 NRE ⇒
    // 被 PlayObject.Chat.cs:402 顶层 catch 吞 ⇒ :41 绿字真机不可达（循环体本就全注释）。
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

/// `DelDenyChrNameLogonCommand`（现网 `@DelDenyChrNameLogon`）。
fn del_deny_chr_name(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let chr_name = params.first().copied().unwrap_or("");
    if chr_name.is_empty() {
        return GmResult::one(GmColor::Red, DEL_DENY_CHRNAME_HELP.to_string());
    }
    // oracle 真机零帧：:26 for-init 读 SystemShare.DenyChrNameList（SystemShare.cs:139
    // 声明即 null 全仓零赋值）即 NRE ⇒ 绿字真机不可达（循环体本就全注释）。
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

/// `DelDenyIPaddrLogonCommand`（`DelDenyIPaddrLogonCommand.cs:12-53`，现网 `@-jip`）：
/// 空名红帮助可达（:19-22）；非空 `:26` 倒序 for-init 读 null 表即 NRE ⇒ 零帧
/// （源文本绿「{名} 没有被禁止登录。」（:50）真机不可达；循环体本就全注释）。
fn del_deny_ip(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let ip = params.first().copied().unwrap_or("");
    if ip.is_empty() {
        return GmResult::one(GmColor::Red, DEL_DENY_IP_HELP.to_string());
    }
    // oracle 真机零帧：:26 for-init 读 SystemShare.DenyIPAddrList（SystemShare.cs:135
    // 声明即 null 全仓零赋值）即 NRE ⇒ 绿字真机不可达。
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

/// `DenyAccountLogonCommand`（现网 `@+jid`）：`sFixDeny[0] == '1'` 走「永久」支。
///
/// 两支的 `g_DenyAccountList.Add(...)` 都被注释；永久支的 `SaveDenyAccountList()` 函数体
/// （写文件）也整段被注释 ⇒ **纯文本命令**，无落盘。
fn deny_account(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let account = params.first().copied().unwrap_or("");
    if account.is_empty() {
        return GmResult::one(GmColor::Red, DENY_ACCOUNT_HELP.to_string());
    }
    let fix_deny = params.get(1).copied().unwrap_or("");
    if fix_deny.starts_with('1') {
        GmResult::one(GmColor::Green, format!("{account}已加入禁止登录帐号列表"))
    } else {
        GmResult::one(
            GmColor::Green,
            format!("{account}已加入临时禁止登录帐号列表"),
        )
    }
}

/// `DenyChrNameLogonCommand`（现网 `@DenyChrNameLogon`）。
fn deny_chr_name(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let chr_name = params.first().copied().unwrap_or("");
    if chr_name.is_empty() {
        return GmResult::one(GmColor::Red, DENY_CHRNAME_HELP.to_string());
    }
    let fix_deny = params.get(1).copied().unwrap_or("");
    if fix_deny.starts_with('1') {
        GmResult::one(GmColor::Green, format!("{chr_name}已加入禁止人物列表"))
    } else {
        GmResult::one(GmColor::Green, format!("{chr_name}已加入临时禁止人物列表"))
    }
}

/// `DenyIPaddrLogonCommand`（现网 `@+jip`）。
///
/// **oracle 怪癖**（设计件 §4.5 #1）：取出的是 `@params.Length > 1 ? @params[3] : ""` ——
/// 2 或 3 个参数时 `@params[3]` 越界 ⇒ `IndexOutOfRangeException`，经反射包成
/// `TargetInvocationException`，被 `PlayObject.ProcessUserLineMsg` 的 catch 吞掉 ⇒ **零帧**。
/// 注意越界发生在「`sIPaddr` 判空」**之前**，照抄这个次序。
fn deny_ip(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let ip = params.first().copied().unwrap_or("");
    let fix_deny = if params.len() > 1 {
        if params.len() <= 3 {
            return no_frames(
                "DenyIPaddrLogon: @params[3] 越界（oracle IndexOutOfRangeException 被 catch 吞）⇒ 零帧",
            );
        }
        params[3]
    } else {
        ""
    };
    if ip.is_empty() {
        return GmResult::one(GmColor::Red, DENY_IP_HELP.to_string());
    }
    if fix_deny.starts_with('1') {
        GmResult::one(GmColor::Green, format!("{ip}已加入禁止登录IP列表"))
    } else {
        GmResult::one(GmColor::Green, format!("{ip}已加入临时禁止登录IP列表"))
    }
}

/// `ShowDenyAccountLogonCommand`（现网 `@+-jid`）。
///
/// `:13` `Permission < 6` 静默返回（外门 10 已过 ⇒ 这条内门对 <10 调用方不可达）；
/// `:19` 读 `SystemShare.DenyAccountList`（null）⇒ NRE 零帧（扫盲卡；源文本绿
/// 「禁止登录帐号列表为空。」真机不可达）。
fn show_deny_account(sess: &GmSession) -> GmResult {
    if sess.permission < 6 {
        return GmResult::empty();
    }
    // oracle 真机零帧：:19 读 SystemShare.DenyAccountList（SystemShare.cs:143 声明即 null
    // 全仓零赋值）即 NRE ⇒ 绿字真机不可达（:24 循环体本就全注释；:13 perm<6 静默门为
    // oracle 原样，<10 调用方被外门挡 ⇒ 修复覆盖 ≥10 的全部调用方）。
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

/// `ShowDenyChrNameLogonCommand`（现网 `@ShowDenyChrNameLogon`）：无权限二次校验，表空即提示。
fn show_deny_chr_name() -> GmResult {
    // oracle 真机零帧：:15 直接读 SystemShare.DenyChrNameList（SystemShare.cs:139 声明即
    // null 全仓零赋值）即 NRE ⇒ 绿字真机不可达（无 perm<6 内门，体首即触表）。
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

/// `ShowDenyIPaddrLogonCommand`（现网 `@+-jip`）：`:16` 读 `SystemShare.DenyIPAddrList`
/// （null）⇒ NRE 零帧（扫盲卡）；源文本表空提示**沿用角色文案**
/// （`禁止登录角色列表为空。`）是 oracle 的复制粘贴怪癖，随之真机不可达。
fn show_deny_ip() -> GmResult {
    // oracle 真机零帧：:16 读 SystemShare.DenyIPAddrList（SystemShare.cs:135 声明即 null
    // 全仓零赋值）即 NRE ⇒ 绿字真机不可达（表空提示沿用角色文案的复制粘贴怪癖随之不可达）。
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

// ---------------------------- 重载/管理族（设计件 §4.4 第 4 族）----------------------------

/// `LoadAdminCommand`（现网 `@LoadAdmin`）：`LocalDB.LoadAdminList()` 与
/// `SendServerGroupMsg(213, ...)` **两条都被注释** ⇒ 纯文本。
/// 体内的 `Permission < 6` 在门 10 之后是死支，1:1 照抄。
fn load_admin(sess: &GmSession) -> GmResult {
    if sess.permission < 6 {
        return GmResult::empty();
    }
    GmResult::one(GmColor::Green, "管理员列表重新加载成功...".to_string())
}

/// `ReLoadAdminCommand`（现网 `@重读GM`）：`LocalDb.LoadAdminList()` 被注释，但
/// `WorldEngine.SendServerGroupMsg(213, ServerIndex, "")` **是活的** ⇒ 宿主未接残余。
fn re_load_admin() -> GmResult {
    one_note(
        GmColor::Green,
        "管理员列表重新加载成功...",
        NOTE_RELOAD_ADMIN,
    )
}

/// `ReloadAbuseCommand`（现网 `@ReloadAbuse`）：`Execute` 空体。
fn reload_abuse() -> GmResult {
    GmResult::empty()
}

/// `ReloadAllGuildCommand`（现网 `@ReloadGuildAll`）：主服门 + `GuildMgr.LoadGuildInfo()`，
/// 成功后回**红字**（oracle 用 `MsgColor.Red`，怪癖照抄）。
fn reload_all_guild(cfg: &GmConfig) -> GmResult {
    if cfg.server_index != 0 {
        return GmResult::one(
            GmColor::Red,
            "此命令只能在主游戏服务器上执行!!!".to_string(),
        );
    }
    one_note(GmColor::Red, "重新加载行会信息完成.", NOTE_RELOAD_ALL_GUILD)
}

/// `ReloadGameItemCommand`（现网 `@ReloadItemDB`）：`LoadItemsDB()` 被注释 ⇒ 纯文本。
fn reload_item_db() -> GmResult {
    GmResult::one(GmColor::Green, "物品数据库重新加载完成。".to_string())
}

/// `ReloadMagicDbCommand`（现网 `@ReloadMagicDB`）：`LoadMagicDB()` 被注释 ⇒ 纯文本。
fn reload_magic_db() -> GmResult {
    GmResult::one(GmColor::Green, "魔法数据库重新加载完成。".to_string())
}

/// `ReloadGuildCommand`（现网 `@ReloadGuild`）：空名 ⇒ 帮助；非主服 ⇒ 拒绝；
/// `FindGuild` 未命中 ⇒ `未找到行会{name}!!!`；命中 ⇒ `guild.LoadGuild()` + 红字成功。
fn reload_guild(parameters: &str, cfg: &GmConfig, world: &GmWorld<'_>) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return GmResult::one(GmColor::Red, RELOAD_GUILD_HELP.to_string());
    }
    if cfg.server_index != 0 {
        return GmResult::one(
            GmColor::Red,
            "此命令只能在主游戏服务器上执行!!!".to_string(),
        );
    }
    if world.guilds.find_guild(name).is_none() {
        return GmResult::one(GmColor::Red, format!("未找到行会{name}!!!"));
    }
    one_note(
        GmColor::Red,
        format!("行会{name}重加载成功..."),
        NOTE_RELOAD_GUILD,
    )
}

/// `ReloadLineNoticeCommand`（现网 `@重读公告`）：`LoadLineNotice(Notice/LineNotice.txt)`
/// 的返回值选帧。部署件 `M2GameSvr/Notice/linenotice.txt` 存在 ⇒ oracle 走成功支。
fn reload_line_notice(cfg: &GmConfig) -> GmResult {
    if cfg.notice_file_exists {
        one_note(GmColor::Green, "重新加载公告设置信息完成。", NOTE_NOTICE)
    } else {
        one_note(GmColor::Red, "重新加载公告设置信息失败!!!", NOTE_NOTICE)
    }
}

/// `ReloadManageCommand`（现网 `@重读登陆`）：两条独立提示，**失败支也是 Green**（怪癖）。
fn reload_manage(cfg: &GmConfig) -> GmResult {
    let mut r = GmResult::empty();
    r.msgs.push(GmMsg {
        color: GmColor::Green,
        text: if cfg.manage_npc_present {
            "重新加载登录脚本完成..."
        } else {
            "重新加载登录脚本失败..."
        }
        .to_string(),
    });
    r.msgs.push(GmMsg {
        color: GmColor::Green,
        text: if cfg.function_npc_present {
            "重新加载功能脚本完成..."
        } else {
            "重新加载功能脚本失败..."
        }
        .to_string(),
    });
    r.log_note = Some(NOTE_MANAGE);
    r
}

/// `ReloadMonItemsCommand`（现网 `@重读爆率`）：try 体整段被注释 ⇒ 恒成功文本，catch 不可达。
fn reload_mon_items() -> GmResult {
    GmResult::one(GmColor::Green, "怪物爆物品列表重加载完成...".to_string())
}

/// `ReloadNpcCommand`（现网 `@重读商人`）：`sParam` 取出来没用，整段逻辑被注释 ⇒ 零帧零副作用。
fn reload_npc() -> GmResult {
    GmResult::empty()
}

/// `ReloadRobotCommand`（现网 `@重读机器人脚本`）：`RobotMgr.ReLoadRobot()` 被注释 ⇒ 纯文本。
fn reload_robot() -> GmResult {
    GmResult::one(GmColor::Green, "重新加载机器人配置完成...".to_string())
}

/// `ReloadRobotManageCommand`（现网 `@重读机器人配置`）：`RobotNPC != null` 分支。
fn reload_robot_manage(cfg: &GmConfig) -> GmResult {
    if cfg.robot_npc_present {
        one_note(
            GmColor::Green,
            "重新加载机器人专用脚本完成...",
            NOTE_ROBOT_MANAGE,
        )
    } else {
        one_note(
            GmColor::Green,
            "重新加载机器人专用脚本失败...",
            NOTE_ROBOT_MANAGE,
        )
    }
}

/// `NpcScriptCommand`（现网 `@NpcScript`）：`GetPoseCreate()` 的返回**没有任何使用**
/// （定位/文件读取/逐行回显整段被注释），`sScriptFileName` 恒空 ⇒ `File.Exists("") == false`
/// ⇒ **零帧**。
fn npc_script() -> GmResult {
    no_frames(
        "NpcScript: 定位与脚本重读整段被注释（sScriptFileName 恒空 ⇒ File.Exists false）⇒ 零帧",
    )
}

// ---------------------------- 桩/测试族（设计件 §4.4 第 11 族）----------------------------

/// `TestFireCommand`（现网 `@TestFire`）：绕角色画一圈 `FireBurnEvent` 入 `EventMgr`，
/// **不发任何 `SysMsg`**。宿主未接 ⇒ 只登记。
fn test_fire() -> GmResult {
    no_frames(NOTE_EVENT_MGR)
}

/// `TestGetBagItemsCommand`（现网 `@TestGetBagItems`）。
///
/// 两个怪癖照抄：① `SysMsgGetBagUseItems` 被注释 ⇒ 四个值恒 0；
/// ② `string.Format("DC:%d SC:%d MC:%d DURA:%d", ...)` 里**没有 `{n}` 占位符** ⇒ 原样回这串字面量。
fn test_get_bag_items() -> GmResult {
    GmResult::one(GmColor::Blue, "DC:%d SC:%d MC:%d DURA:%d".to_string())
}

/// `TestSpeedModeCommand`（现网 `@TestSpeedMode`）：翻转 `TestSpeedMode`，Red。
fn test_speed_mode(sess: &mut GmSession) -> GmResult {
    sess.test_speed_mode = !sess.test_speed_mode;
    let text = if sess.test_speed_mode {
        "开启速度测试模式"
    } else {
        "关闭速度测试模式"
    };
    GmResult::one(GmColor::Red, text.to_string())
}

/// `TestStatusCommand`（现网 `@TestStatus`）。
///
/// `PlayerActor.StatusTimeArr[nType] = (ushort)(nTime * 1000)`：`StatusTimeArr` 是
/// `new ushort[15]`（`BaseObject.cs:394`），`nType` **没有任何范围校验**（校验被注释）⇒
/// `nType < 0 || nType > 14` 抛 `IndexOutOfRangeException` ⇒ catch 吞 ⇒ **零帧**。
/// 合法下标才回 `状态编号:{nType} 时间长度: {nTime} 秒`（Green）。
fn test_status(parameters: &str, sess: &GmSession) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let n_type = str_to_int(params.first().copied().unwrap_or(""), 0);
    let n_time = str_to_int(params.get(1).copied().unwrap_or(""), 0);
    if sess.permission < 6 {
        // 门是 10 ⇒ 死支，照抄。
        return GmResult::empty();
    }
    if !(0..15).contains(&n_type) {
        return no_frames(
            "TestStatus: StatusTimeArr[nType] 越界（oracle IndexOutOfRangeException 被 catch 吞）⇒ 零帧",
        );
    }
    one_note(
        GmColor::Green,
        format!("状态编号:{n_type} 时间长度: {n_time} 秒"),
        NOTE_STATUS,
    )
}

/// `UserCmdCommand`（现网 `@UserCmd`）：`sLable` 取出来后，`GotoLable` 三段**全部被注释**
/// ⇒ 零帧零副作用。
fn user_cmd() -> GmResult {
    no_frames("UserCmd: FunctionNPC.GotoLable 三段全被注释 ⇒ 零帧")
}

/// `BackStepCommand`（现网 `@Backstep`）：`byte` 强转 + `_MIN(_, 8)` 后调 `CharPushed`。
///
/// `nType == 0` 走 `GetBackDir(Dir)`（**不抽数**）；否则先 `RandomByte(nType)` 再推。
/// 本仓无该 GM 推挤宿主 ⇒ 不抽数、零帧（`whitelist.md` T-163：若接线，RNG 面按 R 重评）。
fn back_step(parameters: &str, sess: &GmSession) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    // `(byte)HUtil32.StrToInt(...)`：int → byte 是 **unchecked**（C# 默认上下文）⇒ 截低 8 位。
    let _n_type = (str_to_int(params.first().copied().unwrap_or(""), 0) as u8).min(8);
    let _n_count = str_to_int(params.get(1).copied().unwrap_or(""), 0) as u8;
    if sess.permission < 6 {
        return GmResult::empty();
    }
    no_frames(NOTE_CHAR_PUSHED)
}

/// `FireBurnCommand`（现网 `@FireBurn`）：任一参数为 0 ⇒ 红字帮助（`Help` 为 null，尾随空格）；
/// 否则在脚下加 `FireBurnEvent`（宿主未接）⇒ 零帧。
fn fire_burn(parameters: &str, sess: &GmSession) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let n_int = str_to_int(params.first().copied().unwrap_or(""), 0);
    let n_time = str_to_int(params.get(1).copied().unwrap_or(""), 0);
    let n_n = str_to_int(params.get(2).copied().unwrap_or(""), 0);
    if sess.permission < 6 {
        // 门是 10 ⇒ 死支，照抄（oracle 在参数解析之后、零值检查之前）。
        return GmResult::empty();
    }
    if n_int == 0 || n_time == 0 || n_n == 0 {
        return GmResult::one(GmColor::Red, FIRE_BURN_HELP.to_string());
    }
    no_frames(NOTE_EVENT_MGR)
}

/// `StartQuestCommand.Execute`（`src/Modules/GameCommand/Commands/StartQuestCommand.cs:10-23`，
/// 现网 `@开始提问`，`Command.conf` 的 `[Command] StartQuest=开始提问` / `[Permission] StartQuest=10`）。
///
/// **实际执行行**：
/// - `:12-15` `@params == null` 死分支；
/// - `:16` `sQuestName = @params[0]`（缺参 = 空串）；
/// - `:17-21` 空名 ⇒ 红字 `Command.CommandHelp`（`命令格式: @开始提问 问答名称`）；
/// - `:22` `SystemShare.WorldEngine.SendQuestMsg(sQuestName);` **整行被注释** ⇒ 非空名
///   **零帧、零副作用**（照抄空实现并登记，不臆造问题系统）。
fn start_quest(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(START_QUEST_HELP);
    }
    no_frames(NOTE_START_QUEST_MSG_COMMENTED)
}

/// `MobFireBurnCommand.Execute`（`src/Modules/GameCommand/Commands/MobFireBurnCommand.cs:17-60`，
/// 现网 `@MobFireBurn`；`Command.conf` **没有** `[Command] MobFireBurn` 覆盖 ⇒ 注册名保持英文
/// `MobFireBurn`，部署脚本写 `MOBFIREBURN` 靠 `CommandMaps` 的 `OrdinalIgnoreCase` 命中）。
///
/// **实际执行行**：
/// - `:19` `if (@params == null) return;`——`case 2` 传 `Split(' ')`（不返回 null）⇒ 死分支；
/// - `:23-28` 取六个原始串（地图/坐标 X/Y/光环效果/持续时间/未知）；
/// - `:29-33` `IsNullOrEmpty(sMap) || sMap != "" && sMap[1] == '?'`：空地图 ⇒ 红字帮助；
///   **单码元地图名 ⇒ `sMap[1]` 抛 `IndexOutOfRangeException`，被 `PlayObject.Chat.cs:402-406`
///   吞掉 ⇒ 零帧**（构造级分支；**部署形态不走这条**：`ActionOfGmExecute` 直接拼
///   `sParam1..6`、不调 `GetLineVariableText`，`<$…>` 在动作参数里原样（T-165）⇒ 实参是
///   `<$CURRENTMAP>`（13 码元，`sMap[1]` 是 `$`，不越界），`StrToInt("<$CURRENTX>", -1) = -1`
///   ⇒ 落 `:43` 的 `nX < 0` 帮助支）；
///   第 2 个 UTF-16 码元是 `?` ⇒ 红字帮助；
/// - `:34-38` `(short)StrToInt(sX,-1)` / `(short)sY` / `(byte)sType` / `int sTime` / `int sPoint`；
/// - `:39-42` `nPoint < 0 ⇒ 1`（⇒ `:43` 的 `nPoint < 0` 恒假）；`(byte)` 的 `nType < 0` 也恒假；
/// - `:43-48` 其余非法 ⇒ 红字帮助（模板 7 槽，空槽 ⇒ 连续/尾随空格）；
/// - `:49-58` `MapMgr.FindMap`：命中 ⇒ 临时改 `PlayerActor.Envir`、`FireBurnEvent` 入
///   `EventMgr`、还原 `Envir`，**不发帧**（宿主未接 ⇒ 与 `@FireBurn` 同一支 [`NOTE_EVENT_MGR`]）；
/// - `:59` 未命中 ⇒ `string.Format("地图{0} 不存在", Command.Name, sMap)`——模板**只有一个**
///   占位符 ⇒ 实参 `sMap` 被丢弃，文案是 `地图MobFireBurn 不存在`（**BUG-REPLICA**，照抄）。
fn mob_fire_burn(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let s_map = params.first().copied().unwrap_or("");
    let s_x = params.get(1).copied().unwrap_or("");
    let s_y = params.get(2).copied().unwrap_or("");
    let s_type = params.get(3).copied().unwrap_or("");
    let s_time = params.get(4).copied().unwrap_or("");
    let s_point = params.get(5).copied().unwrap_or("");
    let help = || {
        red(format!(
            "命令格式: {MOB_FIRE_BURN_NAME} {s_map} {s_x} {s_y} {s_type} {s_time} {s_point}"
        ))
    };
    // `:29`：`IsNullOrEmpty(sMap)` ⇒ 帮助（此支不求值 `sMap[1]`）。
    if s_map.is_empty() {
        return help();
    }
    match second_utf16_is_question(s_map) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return help(),
        Some(false) => {}
    }
    // `:34-38`：C# 的 `(short)`/`(byte)` 是 **unchecked** 截断 ⇒ Rust `as`。
    let n_x = str_to_int(s_x, -1) as i16;
    let n_y = str_to_int(s_y, -1) as i16;
    let n_type = str_to_int(s_type, -1) as u8;
    let n_time = str_to_int(s_time, -1);
    let n_point = str_to_int(s_point, -1);
    let n_point = if n_point < 0 { 1 } else { n_point };
    // `nType < 0`（byte 恒 ≥ 0）与 `nPoint < 0`（上一行已抬到 1）是死条件，照抄时省略。
    let _ = (n_type, n_point);
    if n_x < 0 || n_y < 0 || n_time < 0 {
        return help();
    }
    // `:49` `MapMgr.FindMap(sMap)`：未接地图注册表时按既有口径登记（不伪造「地图不存在」）。
    let Some(maps) = world.maps else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    if maps.map_by_name(s_map).is_some() {
        // `:52-57`：`FireBurnEvent(PlayerActor, nX, nY, nType, nTime*1000, nPoint)` 入
        // `SystemShare.EventMgr`（临时改/还原 `PlayerActor.Envir`）。宿主未接事件队列 ⇒ 零帧。
        return no_frames(NOTE_EVENT_MGR);
    }
    // `:59`：模板只有 `{0}` ⇒ `Command.Name` 被填、`sMap` 被忽略 ⇒ 文案是命令名（BUG-REPLICA）。
    red(format!("地图{MOB_FIRE_BURN_NAME} 不存在"))
}

/// `SmakeItemCommand.Execute`（`src/Modules/GameCommand/Commands/SmakeItemCommand.cs:16-80`，
/// 现网 `@改属性`，`Command.conf` 的 `[Command] SuperMake=改属性` / `[Permission] SuperMake=10`）。
///
/// **实际执行行**：
/// - `:22-24` `nWhere`/`nValueType`/`nValue`（`StrToInt(_, 0)`，缺参 = 0）；
/// - `:26` 范围守卫 `0<=nWhere<=12 && 0<=nValueType<=15 && 0<=nValue<=255`，不满足 ⇒ **静默
///   return**（无 else、无回执）；`nValueType <= 15` 让 `:36` 的 `nValueType == 16` 支**不可达**；
/// - `:28` 该槽 `Index > 0` 才继续；**空槽 ⇒ 红字 `身上没指定物品!!!`**（`:77`）；
/// - `:30-34` 模板查不到 ⇒ 静默 return（本仓 `GmWorld` 无物品表 ⇒ 该支登记在 T-* 行）；
/// - `:35` `Desc[9] = (byte)_MIN(255, Desc[9] + 1)`（**无条件**累积升级次数）；
/// - `:44-55` `nValueType` 14 ⇒ `Dura = (ushort)(_MIN(65, nValue) * 1000)`；15 ⇒ `DuraMax` 同式；
/// - `:56-59` 否则 `Desc[nValueType] = (byte)nValue`（`nValueType` 已 ≤13）；
/// - `:60` `RecalcAbilitys()`；`:61` `SendUpdateItem(UseItems[nWhere])`（`SM_UPDATEITEM(203)`）；
/// - `:62-69` 蓝字回执 `"{Index}-{MakeIndex} {Dura}/{DuraMax} {Desc0}/{1}/…/{13}"`
///   （`ItemAttr.WeaponUpgrade = 10` ⇒ 13 个 `Desc` 槽按 0..=13 全列）；
/// - `:70-73` `Config.ShowMakeItemMsg` ⇒ 只写服务端日志（无帧）。
///
/// 落地：命令体只算「改完后的整槽」并装进 [`GmSession::pending_use_item`]；宿主
/// （`lib.rs::apply_pending_gm_use_item`）写会话 + 实体、重算 WAbil、发 `SM_UPDATEITEM(203)`。
fn smake_item(parameters: &str, sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let n_where = str_to_int(params.first().copied().unwrap_or(""), 0);
    let n_value_type = str_to_int(params.get(1).copied().unwrap_or(""), 0);
    let n_value = str_to_int(params.get(2).copied().unwrap_or(""), 0);
    // `:26`：越界 ⇒ 静默 return（无 else）。
    if !(0..=12).contains(&n_where)
        || !(0..=15).contains(&n_value_type)
        || !(0..=255).contains(&n_value)
    {
        return no_frames(NOTE_SMAKE_RANGE);
    }
    if world.world.is_none() {
        return no_frames(NOTE_NO_WORLD);
    }
    let Some(item) = world.self_use_item(n_where as usize) else {
        return no_frames(NOTE_NO_WORLD);
    };
    // `:28`：空槽 ⇒ 一条红字（固定文案，不是帮助格式）。
    if item.index == 0 {
        return red(SMAKE_NO_ITEM_HELP);
    }
    // `:30-34`：模板查不到 ⇒ 静默（`GmWorld` 无物品表，登记为 T-* 残余）。
    let mut desc = item.desc;
    let mut dura = item.dura;
    let mut dura_max = item.dura_max;
    // `:35`：无条件累积升级次数（`byte` 语义 ⇒ 饱和）。
    desc[9] = desc[9].saturating_add(1);
    // `:36-43` 的 `nValueType == 16` 支不可达（守卫 ≤15）——省略并登记。
    // `:44-59`：14/15 先 `_MIN(65, _)` 再 ×1000；其余写 `Desc[n]`。
    match n_value_type {
        14 => dura = (n_value.min(65) * 1000) as u16,
        15 => dura_max = (n_value.min(65) * 1000) as u16,
        t => desc[t as usize] = n_value as u8,
    }
    let _ = NOTE_SMAKE_ABSORB_DEAD;
    sess.pending_use_item = Some(GmUseItemWrite {
        slot: n_where as usize,
        index: item.index,
        make_index: item.make_index,
        dura,
        dura_max,
        desc,
    });
    // `:62-69`：蓝字回执（`Desc` 0..=13 全列）。
    let dump = (0..14)
        .map(|i| desc[i].to_string())
        .collect::<Vec<_>>()
        .join("/");
    GmResult::one(
        GmColor::Blue,
        format!(
            "{}-{} {}/{} {dump}",
            item.index, item.make_index, dura, dura_max
        ),
    )
}

/// `ClearBagItemCommand.Execute`（`src/Modules/GameCommand/Commands/ClearBagItemCommand.cs:13-54`，
/// 现网 `@清理垃圾`，`Command.conf` 的 `[Command] ClearBag=清理垃圾` / `[Permission] ClearBag=10`）。
///
/// **实际执行行**：
/// - `:15-18` `@params == null` 死分支；
/// - `:19` `sHumanName = @params[0]`（缺参 = 空串）；
/// - `:20-24` `IsNullOrEmpty(sHumanName) || … && sHumanName[0] == '?'`（下标 **0**，不越界）
///   ⇒ 红字 `Command.CommandHelp`（`命令格式: @清理垃圾 人物名称`）；
/// - `:25-30` `GetPlayObject` 未命中 ⇒ 红字 `"{0} 现在不在线，或在其它服务器上!!!"`；
/// - `:31-46` 目标 `ItemList.Count > 0` ⇒ **倒序**收集 `DeleteItem{ItemName, MakeIndex}` 并逐件
///   `RemoveAt`，最后 `Clear()`；
/// - `:47-52` 列表非空 ⇒ `ActorMgr.AddOhter(Sequence(), delList)` + 目标
///   `SendMsg(RM_SENDDELITEMLIST, 0, objectId, 0, 0)` ⇒ 泵（`PlayObject.Message.cs:1870-1874`）
///   `SendDelItemList`：`MakeMessage(SM_DELITEMS, 0, 0, 0, Count)` + `EncodeString("名/make/…")`。
///   **空背包 ⇒ 完全静默**（`delList` 保持 null）。
///
/// 落地：命令体只登记目标 `ChrName`（宿主看得到会话背包，空包判断与清包/发帧都在宿主
/// `apply_pending_gm_clear_bag`）。
fn clear_bag_item(parameters: &str, sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() || name.starts_with('?') {
        return red(CLEAR_BAG_HELP);
    }
    if world.world.is_none() {
        return no_frames(NOTE_NO_WORLD);
    }
    if let Some(entity) = world.play_by_name(name) {
        // 复审 important-2：存**实体规范名**。`play_by_name` 是 OrdinalIgnoreCase（与 oracle
        // `GetPlayObject` 同口径），但宿主清包 / 709 路由都按 `chr_name` **精确**匹配 ⇒ 存命令行
        // 输入的大小写会「命令说命中、宿主找不到目标」（静默不清包且 pending 被消费丢失）。
        sess.pending_clear_bag = Some(entity.name.clone());
        no_frames(NOTE_CLEAR_BAG_PENDING)
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

// ---------------------------- 传送/召唤族（设计件 §4.4 第 2 族）----------------------------
//
// 口径：全部 `PermissionMin = 10`（现网 `Permission` 恒 0 ⇒ 生产一律 `权限不够!!!`）。
// 命令体逐行照 oracle；**移动/RNG 一律不伪造**：空间移动/重刷能力已在 `crates/world`
// （`base_object_move_player`/`cast_space_move_spell_with`/`space_move_get_rand_xy`/
// `regen_monster_by_name`），缺的是 GM 派发的 `&mut World` 通道（`GetRecallXy` 全仓无）⇒
// 只保留 oracle 会发的 `SysMsg`（多数支本来就是零文本），并逐条 `log_note` +
// `whitelist.md` T-166..T-171。

/// `parameters.Split(' ')`（`GameCommand.cs:44`）：**保留空项**，与 `split_whitespace` 不同。
fn split_params(parameters: &str) -> Vec<&str> {
    parameters.split(' ').collect()
}

fn red(text: impl Into<String>) -> GmResult {
    GmResult::one(GmColor::Red, text.into())
}

fn green(text: impl Into<String>) -> GmResult {
    GmResult::one(GmColor::Green, text.into())
}

/// 蓝字单条 SysMsg（`ShowMapMode`/`@Showuseiteminfo` 的信息面）。
fn blue(text: impl Into<String>) -> GmResult {
    GmResult::one(GmColor::Blue, text.into())
}

/// `!string.IsNullOrEmpty(s) && s[1] == '?'` 的逐 UTF-16 码元等价。
///
/// `Some(true)` = 命中 `?`；`Some(false)` = 不命中；`None` = C# `s[1]` 越界
/// （`IndexOutOfRangeException` 由 `PlayObject.Chat.cs` 外层 catch 吞掉 ⇒ **零帧**）。
fn second_utf16_is_question(s: &str) -> Option<bool> {
    if s.is_empty() {
        return Some(false);
    }
    s.encode_utf16().nth(1).map(|u| u == u16::from(b'?'))
}

/// `HUtil32.StrToInt16`：`short.TryParse`（前后的 .NET 空白近似为 `trim`），失败取 `def`。
fn str_to_int16(s: &str, def: i16) -> i16 {
    s.trim().parse::<i16>().unwrap_or(def)
}

/// `AllowDearRecallCommand`（现网 `@dfsgf567`）：翻转 `CanDearRecall`，Blue。
fn allow_dear_recall(sess: &mut GmSession) -> GmResult {
    sess.can_dear_recall = !sess.can_dear_recall;
    let text = if sess.can_dear_recall {
        ENABLE_DEAR_RECALL
    } else {
        DISABLE_DEAR_RECALL
    };
    GmResult::one(GmColor::Blue, text.to_string())
}

/// `AllowGuildRecallCommand`（现网 `@sadasd`）：翻转 `AllowGuildReCall`，Green（`[允许行会合一]`）。
fn allow_guild_recall(sess: &mut GmSession) -> GmResult {
    sess.allow_guild_re_call = !sess.allow_guild_re_call;
    let text = if sess.allow_guild_re_call {
        ENABLE_GUILD_RECALL
    } else {
        DISABLE_GUILD_RECALL
    };
    GmResult::one(GmColor::Green, text.to_string())
}

/// `AllowMasterRecallCommand`（现网 `@fdgh56j`）：翻转 `CanMasterRecall`，Blue。
fn allow_master_recall(sess: &mut GmSession) -> GmResult {
    sess.can_master_recall = !sess.can_master_recall;
    let text = if sess.can_master_recall {
        ENABLE_MASTER_RECALL
    } else {
        DISABLE_MASTER_RECALL
    };
    GmResult::one(GmColor::Blue, text.to_string())
}

/// `DearRecallCommond`（现网 `@dsfsdt546`）：把配偶召到身边。
///
/// 门序照 oracle：未婚 → 地图禁夫妻传送 → 配偶不在线（按 `Gender` 分文案）→ 10s 冷却。
/// 末端 `DearHuman.CanDearRecall` 是**对方会话**字段，`GmWorld` 读不到 ⇒ 残余（T-167）。
fn dear_recall(sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    if sess.dear_name.is_empty() {
        return red("你没有结婚!!!");
    }
    let Some(flag) = world.self_flag() else {
        return no_frames(NOTE_NO_WORLD);
    };
    if flag.bo_nodearrecall {
        return red("本地图禁止夫妻传送!!!");
    }
    // `PlayerActor.DearHuman == null`：本仓 `Entity.dear_human` 生产路径无写点（T-167）。
    let dear = world
        .self_entity()
        .and_then(|e| e.dear_human)
        .and_then(|id| world.entity(id));
    let Some(dear) = dear else {
        let text = if sess.gender == 0 {
            "你的老婆不在线!!!"
        } else {
            "你的老公不在线!!!"
        };
        return red(text);
    };
    if world.now_ms - sess.dear_recall_tick < 10000 {
        return red("稍等会才能再次使用此功能!!!");
    }
    sess.dear_recall_tick = world.now_ms;
    let _ = dear;
    no_frames(NOTE_TARGET_CAN_DEAR_RECALL)
}

/// `GroupRecallCommand`（现网 `@天地合一`）：团长把组员召到身边。
fn group_recall(sess: &mut GmSession, cfg: &GmConfig, world: &GmWorld<'_>) -> GmResult {
    if !sess.recall_suite && sess.permission < 6 {
        return red("您现在还无法使用此功能!!!");
    }
    // `short dwValue = (short)((GetTickCount() - GroupRcallTick) / 1000);`
    let dw = ((world.now_ms - sess.group_rcall_tick) / 1000) as i16;
    sess.group_rcall_tick += i64::from(dw) * 1000;
    if sess.permission >= 6 {
        sess.group_rcall_time = 0;
    }
    if sess.group_rcall_time > dw {
        sess.group_rcall_time = sess.group_rcall_time.wrapping_sub(dw);
    } else {
        sess.group_rcall_time = 0;
    }
    if sess.group_rcall_time != 0 {
        return red(format!(
            "{} 秒之后才可以再使用此功能!!!",
            sess.group_rcall_time
        ));
    }
    let Some(me) = world.self_entity() else {
        return no_frames(NOTE_NO_WORLD);
    };
    let mut msgs = Vec::new();
    let mut recalled = false;
    if me.group_owner == me.id {
        for &member_id in &me.group_members {
            let Some(member) = world.entity(member_id) else {
                continue;
            };
            if member.allow_group_re_call {
                if world.flag_of(member.map_id).is_some_and(|f| f.no_re_call) {
                    msgs.push(GmMsg {
                        color: GmColor::Red,
                        text: format!("{} 所在的地图不允许传送。", member.name),
                    });
                } else {
                    recalled = true;
                }
            } else {
                msgs.push(GmMsg {
                    color: GmColor::Red,
                    text: format!("{} 不允许天地合一!!!", member.name),
                });
            }
        }
        sess.group_rcall_tick = world.now_ms;
        sess.group_rcall_time = cfg.group_recall_time;
    }
    GmResult {
        found: true,
        parsed: true,
        msgs,
        frames: Vec::new(),
        log_note: recalled.then_some(NOTE_RECALL_HUMAN),
    }
}

/// `GuildRecallCommand`（现网 `@df56dfs`）：成员遍历在 oracle 里**整段注释** ⇒ 恒「已传送0个成员」。
fn guild_recall(sess: &mut GmSession, cfg: &GmConfig, world: &GmWorld<'_>) -> GmResult {
    if !sess.guild_move && sess.permission < 6 {
        return red("您现在还无法使用此功能!!!");
    }
    if !world.is_guild_master(world.speaker) {
        return red("行会掌门人才可以使用此功能!!!");
    }
    let Some(flag) = world.self_flag() else {
        return no_frames(NOTE_NO_WORLD);
    };
    if flag.no_guild_re_call {
        return red("本地图不允许使用此功能!!!");
    }
    let Some((map_id, x, y)) = world.self_pos() else {
        return no_frames(NOTE_NO_WORLD);
    };
    let Some(w) = world.world else {
        return no_frames(NOTE_NO_WORLD);
    };
    // `CastleMgr.InCastleWarArea(PlayerActor)`：首个命中的城堡，`UnderWar` ⇒ 拒绝。
    if w.castles
        .iter()
        .find(|c| c.in_castle_war_area(Some(map_id), x, y))
        .is_some_and(|c| c.under_war)
    {
        return red("攻城区域不允许使用此功能!!!");
    }
    let dw = ((world.now_ms - sess.group_rcall_tick) / 1000) as i32;
    sess.group_rcall_tick += i64::from(dw) * 1000;
    if sess.permission >= 6 {
        sess.group_rcall_time = 0;
    }
    if i32::from(sess.group_rcall_time) > dw {
        sess.group_rcall_time = sess.group_rcall_time.wrapping_sub(dw as i16);
    } else {
        sess.group_rcall_time = 0;
    }
    if sess.group_rcall_time > 0 {
        return red(format!(
            "{} 秒之后才可以再使用此功能!!!",
            sess.group_rcall_time
        ));
    }
    sess.group_rcall_tick = world.now_ms;
    sess.group_rcall_time = cfg.guild_recall_time as i16;
    green("已传送0个成员，0个成员未被传送。")
}

/// `MapMoveCommand`（现网 `@f`）：在指定地图随机移动自己。
fn map_move(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let map = params.first().copied().unwrap_or("");
    if map.is_empty() {
        return red(MAP_MOVE_HELP);
    }
    let Some(maps) = world.maps else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    if maps.id_of(map).is_none() {
        return red(format!("{map} 此地图号不存在!!!"));
    }
    // `Permission >= PermissionMin || CanMoveMap` 恒真（`CanMoveMap` 的函数体注释，`M2Share.cs:915-927`）。
    no_frames(NOTE_MAP_RANDOM_MOVE)
}

/// `MapMoveHumanCommand`（现网 `@集体传送`）：把源地图全部玩家随机移到目标地图。
fn map_move_human(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let src = params.first().copied().unwrap_or("");
    let den = params.get(1).copied().unwrap_or("");
    if den.is_empty() || src.is_empty() || src.starts_with('?') {
        return red(MAP_MOVE_HUMAN_HELP);
    }
    let Some(maps) = world.maps else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    if maps.id_of(src).is_none() {
        return red(format!("地图{src}不存在!!!"));
    }
    if maps.id_of(den).is_none() {
        return red(format!("地图{den}不存在!!!"));
    }
    no_frames(NOTE_MAP_RANDOM_MOVE)
}

/// `MasterRecallCommand`（现网 `@jkt76`）：师父把在线徒弟召到身边。
fn master_recall(sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    if !sess.is_master {
        return red("只能师父才能使用此功能!!!");
    }
    if sess.master_list.is_empty() {
        return red("你的徒弟一个都不在线!!!");
    }
    if world.now_ms - sess.master_recall_tick < 10000 {
        return red("稍等一会才能再次使用此功能!!!");
    }
    // oracle 只读 MasterRecallTick、**不写**；末端 `CanMasterRecall` 是对方会话字段 ⇒ 残余（T-168）。
    no_frames(NOTE_MASTER_RECALL_TARGET)
}

/// `MoveMobToCommand`（现网 `@MoveMobTo`）：两个移动支整段注释 ⇒ 参数合法时 no-op。
fn move_mob_to(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    // 命令体无条件取 `@params[0..6]`：不足 7 段 ⇒ `IndexOutOfRangeException` 被外层 catch 吞 ⇒ 零帧。
    if params.len() < 7 {
        return no_frames(NOTE_INDEX_OUT_OF_RANGE);
    }
    let (mon, old_map, new_map) = (params[0], params[1], params[2]);
    if mon.is_empty() || old_map.is_empty() || new_map.is_empty() || mon.starts_with('?') {
        return red(MOVE_MOB_TO_HELP);
    }
    // `MapMgr.FindMap` 两支都只影响已注释的移动支 ⇒ 帧面恒零。
    no_frames(NOTE_MOVE_MOB_TO)
}

/// `PositionMoveCommand`（现网 `@ff`）：移动到指定地图坐标。
fn position_move(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let map = params.first().copied().unwrap_or("");
    let s_x = params.get(1).copied().unwrap_or("");
    let s_y = params.get(2).copied().unwrap_or("");
    if map.is_empty() || s_x.is_empty() || s_y.is_empty() || map.starts_with('?') {
        return red(POSITION_MOVE_HELP);
    }
    let Some(maps) = world.maps else {
        return no_frames(NOTE_NO_MAP_REGISTRY);
    };
    let Some(map_id) = maps.id_of(map) else {
        // `FindMap == null` ⇒ oracle 什么也不做。
        return GmResult::empty();
    };
    let n_x = str_to_int16(s_x, 0);
    let n_y = str_to_int16(s_y, 0);
    match world.can_walk(map_id, i32::from(n_x), i32::from(n_y)) {
        None => no_frames(NOTE_NO_MAP_REGISTRY),
        Some(true) => no_frames(NOTE_SPACE_MOVE),
        Some(false) => green(format!("无法移动到地图: {map} X:{s_x} Y:{s_y}")),
    }
}

/// `RecallHumanCommand`（现网 `@RecallHuman`）：把指定人物召到身边（判空用 `sHumanName[1]`）。
fn recall_human(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(RECALL_HUMAN_HELP);
    }
    match second_utf16_is_question(name) {
        None => no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => red(RECALL_HUMAN_HELP),
        // 离线支的 SysMsg 在 oracle 里被注释；在线支只有移动 ⇒ 帧面恒零。
        Some(false) => no_frames(NOTE_RECALL_HUMAN),
    }
}

/// `RecallMobCommand`（现网 `@bb`）：召唤指定怪物为宠物。
fn recall_mob(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let mon = params.first().copied().unwrap_or("");
    if mon.is_empty() || mon.starts_with('?') {
        return red(RECALL_MOB_HELP);
    }
    // `nCount/nLevel/nTick/nAutoChangeColor/nFixColor` 只喂已注释的宝宝字段与刷怪支 ⇒ 帧面恒零。
    no_frames(NOTE_RECALL_MOB_SPAWN)
}

/// `ReGotoHumanCommand`（现网 `@ReGotoHuman`）：飞到指定玩家身边。
fn regoto_human(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(REGOTO_HUMAN_HELP);
    }
    match second_utf16_is_question(name) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return red(REGOTO_HUMAN_HELP),
        Some(false) => {}
    }
    if world.world.is_none() {
        return no_frames(NOTE_NO_WORLD);
    }
    if world.play_by_name(name).is_none() {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    no_frames(NOTE_SPACE_MOVE)
}

/// `SignMapMoveCommand`（现网 `@SignMapMove`）：oracle 体**整段注释** ⇒ no-op。
fn sign_map_move() -> GmResult {
    no_frames(NOTE_SIGN_MAP_MOVE)
}

/// `SuperTingCommand`（现网 `@SuperTingwa`）：随机传送指定玩家与他身边的人。
fn super_ting(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let range = params.get(1).copied().unwrap_or("");
    if range.is_empty() || name.is_empty() {
        return red(SUPER_TING_HELP);
    }
    match second_utf16_is_question(name) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return red(SUPER_TING_HELP),
        Some(false) => {}
    }
    // `nRange = HUtil32._MAX(10, HUtil32.StrToInt(sRange, 2))` 只用于已未接的范围扫描。
    if world.world.is_none() {
        return no_frames(NOTE_NO_WORLD);
    }
    if world.play_by_name(name).is_some() {
        no_frames(NOTE_MAP_RANDOM_MOVE_HOME)
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `TingCommand`（现网 `@Tingwa`）：把指定人物随机传送。
fn ting(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(TING_HELP);
    }
    match second_utf16_is_question(name) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return red(TING_HELP),
        Some(false) => {}
    }
    if world.world.is_none() {
        return no_frames(NOTE_NO_WORLD);
    }
    if world.play_by_name(name).is_some() {
        no_frames(NOTE_MAP_RANDOM_MOVE_HOME)
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `UserMoveXyCommand`（现网 `@传送`）：移动地图指定坐标（需要戴传送装备）。
fn user_move_xy(
    parameters: &str,
    sess: &mut GmSession,
    cfg: &GmConfig,
    world: &GmWorld<'_>,
) -> GmResult {
    let params = split_params(parameters);
    let s_x = params.first().copied().unwrap_or("");
    let s_y = params.get(1).copied().unwrap_or("");
    if !sess.teleport {
        return red("您现在还无法使用此功能!!!");
    }
    let n_x = str_to_int16(s_x, -1);
    let n_y = str_to_int16(s_y, -1);
    let Some(flag) = world.self_flag() else {
        return no_frames(NOTE_NO_WORLD);
    };
    if flag.bo_nopositionmove {
        return red("此地图禁止使用此命令!!!");
    }
    let Some((map_id, _, _)) = world.self_pos() else {
        return no_frames(NOTE_NO_WORLD);
    };
    let walk = world.can_walk_of_item(
        map_id,
        i32::from(n_x),
        i32::from(n_y),
        cfg.user_move_can_dup_obj,
        cfg.user_move_can_on_item,
    );
    match walk {
        None => no_frames(NOTE_NO_WORLD),
        Some(false) => {
            let map_name = world.map_name(map_id).unwrap_or("");
            green(format!("无法移动到地图: {map_name} X:{s_x} Y:{s_y}"))
        }
        Some(true) => {
            // oracle `UserMoveXYCommand.cs:31`：门是**原始毫秒**
            // （`GetTickCount() - TeleportTick > dwUserMoveTime * 1000`），
            // 只有红字里的秒数是整秒截断（`:35` 的 `/ 1000`）。先把 elapsed 截成秒再比
            // ⇒ UserMoveTime*1000+1..999 ms 段 oracle 放行、Rust 拒绝（Important-1）。
            let elapsed_ms = world.now_ms - sess.teleport_tick;
            if elapsed_ms > i64::from(cfg.user_move_time) * 1000 {
                sess.teleport_tick = world.now_ms;
                no_frames(NOTE_SPACE_MOVE)
            } else {
                red(format!(
                    "{}秒之后才可以再使用此功能!!!",
                    i64::from(cfg.user_move_time) - elapsed_ms / 1000
                ))
            }
        }
    }
}

/// `CommandSystem.Execute(PlayerActor, line)` 的等价物（**不含** `PlayObject.Chat.cs` 的第二条回落帧）。
///
/// 调用方约定：`parsed == false` ⇒ 无帧；`found == false && parsed` ⇒ 先发 [`GmResult::msgs`]，
/// 再发 [`incorrect_msg`]。
pub fn execute(sess: &mut GmSession, cfg: &GmConfig, world: &GmWorld<'_>, line: &str) -> GmResult {
    // 「待落地权威写」一次性：先清空，只有**本次**命中的命令体真的翻转了镜像才重新装填。
    // 否则一条被权限门/未知命令挡下的 `@…` 会把上一条命令的差量顺带落地。
    sess.pending_entity = None;
    sess.pending_kick = None;
    sess.pending_use_item = None;
    sess.pending_clear_bag = None;
    sess.pending_feature_changed = false;
    let Some((command, parameters)) = parse_command(line) else {
        return GmResult::default();
    };
    // 内建 `@help`：`HelpCommandGroup.Handle` 重写，**不过权限门**。
    if command.eq_ignore_ascii_case(BUILTIN_HELP) {
        return GmResult::one(GmColor::Red, help_text(parameters));
    }
    // 内建 `@commands`：未重写 `Handle` ⇒ `default:` 支用 2 个实参调零形参 `Execute()`
    // ⇒ `TargetParameterCountException` 被 catch 吞掉 ⇒ 零帧（设计件 §1.3）。
    if command.eq_ignore_ascii_case(BUILTIN_COMMANDS) {
        return GmResult {
            found: true,
            parsed: true,
            msgs: Vec::new(),
            frames: Vec::new(),
            log_note: Some("commands: TargetParameterCountException（oracle 无回帧）"),
        };
    }
    let Some(def) = find(command) else {
        // 未命中 ⇒ 无条件先发一条（`CommandSystem.cs:185-194`），`found = false` 交给回落补第二条。
        return GmResult {
            found: false,
            parsed: true,
            msgs: vec![GmMsg {
                color: GmColor::Red,
                text: format!("未知命令: {command} {parameters}"),
            }],
            frames: Vec::new(),
            log_note: None,
        };
    };
    // `GameCommand.Handle` 的权限门（`GameCommand.cs:37`）：`Permission < PermissionMin` ⇒ 红字。
    if sess.permission < def.perm_min {
        return GmResult::one(GmColor::Red, PERMISSION_TOO_LOW.to_string());
    }
    match def.imp {
        Imp::Help | Imp::Commands => unreachable!("内建命令在表外处理"),
        Imp::Date => GmResult::one(
            GmColor::Blue,
            format!(
                "{NOW_CURR_DATETIME}{}",
                Local::now().format("%Y-%m-%d %H:%M:%S")
            ),
        ),
        Imp::PrvMsg => prv_msg(parameters, sess),
        Imp::AllowMsg => allow_msg(sess),
        Imp::LockLogin => lock_login(sess, cfg),
        Imp::AddRobotPlay => add_robot_play(parameters),

        // ---- 批次 2/3 · 重载/管理（14）----
        Imp::LoadAdmin => load_admin(sess),
        Imp::NpcScript => npc_script(),
        Imp::ReloadAbuse => reload_abuse(),
        Imp::ReLoadAdmin => re_load_admin(),
        Imp::ReloadAllGuild => reload_all_guild(cfg),
        Imp::ReloadItemDb => reload_item_db(),
        Imp::ReloadGuild => reload_guild(parameters, cfg, world),
        Imp::ReloadLineNotice => reload_line_notice(cfg),
        Imp::ReloadMagicDb => reload_magic_db(),
        Imp::ReloadManage => reload_manage(cfg),
        Imp::ReloadMonItems => reload_mon_items(),
        Imp::ReloadNpc => reload_npc(),
        Imp::ReloadRobot => reload_robot(),
        Imp::ReloadRobotManage => reload_robot_manage(cfg),

        // ---- 批次 2/3 · 封禁名单（9）----
        Imp::DelDenyAccount => del_deny_account(parameters),
        Imp::DelDenyChrName => del_deny_chr_name(parameters),
        Imp::DelDenyIp => del_deny_ip(parameters),
        Imp::DenyAccount => deny_account(parameters),
        Imp::DenyChrName => deny_chr_name(parameters),
        Imp::DenyIp => deny_ip(parameters),
        Imp::ShowDenyAccount => show_deny_account(sess),
        Imp::ShowDenyChrName => show_deny_chr_name(),
        Imp::ShowDenyIp => show_deny_ip(),

        // ---- 批次 2/3 · 桩/测试（7）----
        Imp::TestFire => test_fire(),
        Imp::TestGetBagItems => test_get_bag_items(),
        Imp::TestSpeedMode => test_speed_mode(sess),
        Imp::TestStatus => test_status(parameters, sess),
        Imp::UserCmd => user_cmd(),
        Imp::BackStep => back_step(parameters, sess),
        Imp::FireBurn => fire_burn(parameters, sess),
        Imp::StartQuest => start_quest(parameters),
        Imp::MobFireBurn => mob_fire_burn(parameters, world),
        Imp::SmakeItem => smake_item(parameters, sess, world),
        Imp::ClearBagItem => clear_bag_item(parameters, sess, world),

        // ---- 批次 3 · 传送/召唤（17 + `@传送`）----
        Imp::AllowDearRecall => allow_dear_recall(sess),
        Imp::AllowGuildRecall => allow_guild_recall(sess),
        Imp::AllowMasterRecall => allow_master_recall(sess),
        Imp::DearRecall => dear_recall(sess, world),
        Imp::GroupRecall => group_recall(sess, cfg, world),
        Imp::GuildRecall => guild_recall(sess, cfg, world),
        Imp::MapMove => map_move(parameters, world),
        Imp::MapMoveHuman => map_move_human(parameters, world),
        Imp::MasterRecall => master_recall(sess, world),
        Imp::MoveMobTo => move_mob_to(parameters),
        Imp::PositionMove => position_move(parameters, world),
        Imp::RecallHuman => recall_human(parameters),
        Imp::RecallMob => recall_mob(parameters),
        Imp::ReGotoHuman => regoto_human(parameters, world),
        Imp::SignMapMove => sign_map_move(),
        Imp::SuperTing => super_ting(parameters, world),
        Imp::Ting => ting(parameters, world),
        Imp::UserMoveXy => user_move_xy(parameters, sess, cfg, world),

        // ---- 批次 4 · 数值/角色族（44）----
        Imp::AddGameGold => add_game_gold(parameters, world),
        Imp::AddGold => add_gold(parameters, world),
        Imp::AdjuestExp => adjuest_exp(parameters, world),
        Imp::AdjuestLevel => adjuest_level(parameters, world),
        Imp::AdjustExp => adjust_exp(),
        Imp::BindUseItem => bind_use_item(parameters, world),
        Imp::BonuPoint => bonu_point(parameters, world),
        Imp::ChangeAdminMode => change_admin_mode(sess, world),
        Imp::ChangeAttackMode => change_attack_mode(sess, world),
        Imp::ChangeDearName => change_dear_name(parameters, world),
        Imp::ChangeGender => change_gender(parameters, world),
        Imp::ChangeItemName => change_item_name(parameters),
        Imp::ChangeJob => change_job(parameters, world),
        Imp::ChangeLevel => change_level(parameters, sess, world),
        Imp::ChangeMasterName => change_master_name(parameters, world),
        Imp::ChangeObMode => change_ob_mode(sess, world),
        Imp::ChangeSalveStatus => change_salve_status(sess, world),
        Imp::ChangeSuperManMode => change_super_man_mode(sess, world),
        Imp::ChangeUserFull => change_user_full(parameters),
        Imp::ChangeZenFastStep => change_zen_fast_step(parameters),
        Imp::CreditPoint => credit_point(parameters, world),
        Imp::DelBonuPoint => del_bonu_point(parameters, world),
        Imp::DeleteItem => delete_item(parameters, world),
        Imp::DelGameGold => del_game_gold(parameters, world),
        Imp::DelGold => del_gold(parameters, world),
        Imp::FreePenalty => free_penalty(parameters, world),
        Imp::GameDiamond => game_diamond(),
        Imp::GameGird => game_gird(),
        Imp::GameGlory => game_glory(),
        Imp::GameGold => game_gold(parameters, world),
        Imp::GamePoint => game_point(parameters, world),
        Imp::GetUserItems => get_user_items(parameters, world),
        Imp::Hair => hair(parameters, world),
        Imp::Hunger => hunger(parameters, world),
        Imp::IncPkPoint => inc_pk_point(parameters, world),
        Imp::LuckPoint => luck_point(parameters, world),
        Imp::PkPoint => pk_point(parameters),
        Imp::RefineWeapon => refine_weapon(parameters),
        Imp::ReNewLevel => re_new_level(parameters, sess, world),
        Imp::RestBonuPoint => rest_bonu_point(parameters, world),
        Imp::Training => training(),
        Imp::TrainingMagic => training_magic(parameters, world),
        Imp::TrainingSkill => training_skill(parameters, world),
        Imp::UnBindUseItem => unbind_use_item(),

        // ---- 批次 5 · 脚本可达族（`GMEXECUTE` 抬权可达，tier=R）----
        Imp::Shutup => shutup(parameters),
        Imp::ShutupRelease => shutup_release(parameters),
        Imp::ShowEffect => show_effect(),

        Imp::KickHuman => kick_human(parameters, sess, world),

        // ---- 批次 5A · 行会/城堡族 A 片（6，卡 `openmir2-gm-batch-5a-guild-impl`）----
        Imp::Announcement => no_frames(
            "Announcement：AnnouncementCommand.cs:14-41 命令体整段被注释 ⇒ 真 no-op（零帧）",
        ),
        Imp::Auth => auth(world),
        Imp::Authally => authally(world),
        Imp::AuthCancel => auth_cancel(parameters, world),
        Imp::GuildWar => no_frames(
            "GuildWar：GuildWarCommand.cs:12-15 体只有 if (Permission < 6) return ⇒ 过权限门后零帧",
        ),
        Imp::SbkDoorControl => {
            no_frames("SbkDoorControl：SbkDoorControlCommand.cs:7-9 空体 ⇒ 真 no-op（零帧）")
        }
        // ---- 族 P · 密码锁门（7，卡 `openmir2-gm-family-p-passwordlock`）----
        Imp::PasswordLockGate => password_lock_gate(parameters, cfg),

        // ---- 族 T · 会话开关/骑马（7，卡 `openmir2-gm-family-t-toggle-horse`）----
        Imp::LetTrade => let_trade(sess),
        Imp::LetGuild => let_guild(sess),
        Imp::LetShout => let_shout(sess),
        Imp::BanGuildChat => ban_guild_chat(sess),
        Imp::TakeOffHorse => take_off_horse(sess),
        Imp::TakeOnHorse => take_on_horse(sess),
        Imp::DisableFilter => disable_filter(world),

        // ---- 族 Q · 查询/展示（10，卡 `openmir2-gm-family-q-query-display`）----
        Imp::HumanCount => human_count(parameters, world),
        Imp::MobCount => mob_count(parameters, world),
        Imp::MapInfo => q_map(parameters, world),
        Imp::ShowMapInfo => show_map_info(world),
        Imp::ShowMapMode => show_map_mode(parameters, world),
        Imp::ShowFlag => show_flag(parameters, world),
        Imp::HumanLocal => human_local(world),
        Imp::HumanInfo => human_info(parameters, world),
        Imp::ShowUseItemInfo => show_use_item_info(parameters, world),
        Imp::LotteryTicket => lottery_ticket(cfg),

        // ---- 族 X1 · 禁言名单（5，卡 `openmir2-gm-family-x1-denywhisper`）----
        Imp::DisableSendMsg => disable_send_msg(parameters, world),
        Imp::DisableSendMsgListCmd => disable_send_msg_list(world),
        Imp::EnableSendMsg => enable_send_msg(parameters),
        Imp::ShutupList => shutup_list(world),
        Imp::ViewWhisper => view_whisper(parameters, world, sess),
        Imp::Unimplemented => GmResult {
            found: true,
            parsed: true,
            msgs: Vec::new(),
            frames: Vec::new(),
            log_note: Some("GM 命令体未实现（G1 + 批次 2/3 之外；现网权限门先挡）"),
        },
    }
}

/// `ShutupCommand.Execute`（`ShutupCommand.cs:15-57`，现网 `@禁言`）。
///
/// **实际执行行**（逐行核过 `src/Modules/GameCommand/Commands/ShutupCommand.cs`）：
/// - `:21-22` 取 `sHumanName`/`sTime`（缺参补空串）；
/// - `:23-28` 参数校验：`sTime` 空 **或** `sHumanName` 空 **或** `sHumanName[1] == '?'`
///   ⇒ `命令格式: @禁言 人物名称  时间长度(分钟)`（Red/Hint）并 `return`。注意 C# `||`
///   **短路**：`sTime` 为空时根本不访问 `sHumanName[1]`；`sHumanName` 只有一个 UTF-16 码元
///   时 `sHumanName[1]` 抛 `IndexOutOfRangeException`，被 `PlayObject.Chat.cs:402-406` 的
///   catch 吞掉 ⇒ **零帧**；
/// - `:29` `dwTime = (uint)HUtil32.StrToInt(sTime, 5)`（`int.TryParse` 失败取 5；负值按
///   `unchecked` 回转成 uint，例如 `-1` ⇒ `4294967295`）；
/// - `:30` `EnterCriticalSection(SystemShare.DenySayMsgList)` 对 **null 表**（`SystemShare.cs:64`
///   声明即 null 全仓零赋值，扫盲卡实证）即 NRE ⇒ 被 `PlayObject.Chat.cs:402` 顶层 catch 吞
///   ⇒ **零帧**——`:56` 的红字 SysMsg（`GameCommandShutupHumanMsg`，发给调用者）真机不可达；
///   `:33-50` 的名单写段本就整段注释。本仓按真机行为落地：校验过 ⇒ 零帧 + `log_note`。
fn shutup(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let human = params.first().copied().unwrap_or("");
    let time = params.get(1).copied().unwrap_or("");
    // C# `||` 短路：`sTime` 空 ⇒ 直接回执，不访问 `sHumanName[1]`。
    if time.is_empty() || human.is_empty() {
        return red(SHUTUP_HELP);
    }
    // `!string.IsNullOrEmpty(sHumanName) && sHumanName[1] == '?'`（UTF-16 下标）。
    match second_utf16_is_question(human) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return red(SHUTUP_HELP),
        Some(false) => {}
    }
    // `(uint)HUtil32.StrToInt(sTime, 5)`：int → uint 的 unchecked 回转用 `as`（:29）。
    let _dw_time = str_to_int(time, 5) as u32;
    // oracle 真机零帧（reviewer-21 X1 审查同镜头盲区，扫盲卡 openmir2-gm-systemshare-null-sweep）：
    // :30 `EnterCriticalSection(SystemShare.DenySayMsgList)` 对 null 表即 NRE ⇒
    // 被 PlayObject.Chat.cs:402 顶层 catch 吞 ⇒ :56 的红字 SysMsg 真机不可达
    // （DenySayMsgList=SystemShare.cs:64 声明即 null 全仓零赋值；:33-50 名单写段亦全注释）。
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

/// `ShowEffectCommand.Execute`（`src/Modules/GameCommand/Commands/ShowEffectCommand.cs:12-71`，
/// 现网 `@ShowEffect`；`Command.conf` 里**没有** `[Command] ShowEffect` 覆盖 ⇒ 注册名保持英文
/// `ShowEffect`，查表 `OrdinalIgnoreCase` ⇒ 部署脚本的 `Gmexecute showeffect <n>` 能命中）。
///
/// **实际执行行**：
/// - `:14-17` `if (@params == null) return;`——`GameCommand.Handle` 的 `case 2` 传
///   `parameters.Split(' ')`（不返回 null）⇒ 死分支；
/// - `:18/:19` 取 `sEffect`/`sTime`，**两者之后都没被使用**；
/// - `:20-70` 的参数校验、79-85 烟花（`TFlowerEvent`）、87/89/90 的 `RM_MYSHOW`、
///   `EventManager.AddEvent` **全部被注释** ⇒ 函数体落下即返回。
///
/// ⇒ online 行为 = **任何参数都零帧、零副作用**（BUG-REPLICA / 空实现，不修）；按卡面要求只落
/// 「逐字节等价」的最小 no-op 并登记，不臆造烟花特效或 `RM_MYSHOW` 帧。
fn show_effect() -> GmResult {
    no_frames(NOTE_SHOW_EFFECT_ALL_COMMENTED)
}

/// `ShutupReleaseCommand.Execute`（`src/Modules/GameCommand/Commands/ShutupReleaseCommand.cs:12-49`，
/// 现网 `@取消禁言`，`Command.conf` 的 `[Command] ReleaseShutup=取消禁言` 覆盖了名字）。
///
/// **实际执行行**（逐行核过 oracle）：
/// - `:14` `if (@params == null) return;`——`GameCommand.Handle` 的 `case 2` 传的是
///   `parameters.Split(' ')`，`Split` **不返回 null** ⇒ 死分支（只有 `default:` 支传 null，
///   本命令的 `Execute` 是两参签名，走 `case 2`）；
/// - `:18` `sHumanName = @params.Length > 0 ? @params[0] : ""`；
/// - `:19` `boAll = @params.Length > 1 ? bool.Parse(@params[1]) : false`——**先于**下面的
///   恒真分支求值：第二段不是 `true`/`false`（`""`、`1`、`yes`…）⇒ `FormatException`
///   冒泡出 `Execute`（`MethodInfo.Invoke` 的 `TargetInvocationException` 由
///   `PlayObject.Chat.cs:402-406` 的整段 catch 吞掉）⇒ **零帧**；
/// - `:20` `if (string.IsNullOrEmpty(sHumanName) || !string.IsNullOrEmpty(sHumanName))`——
///   `A || !A` **恒真** ⇒ 无条件 `SysMsg(Command.CommandHelp, Red, Hint)` 后 `return`；
///   `Command.CommandHelp` = `$"命令格式: @{Name} {Help}"`（`CommandAttribute.cs:56-58`），
///   `Name` 被部署 `Command.conf` 覆盖为 `取消禁言`、`Help` = `人物名称`；
/// - `:25-48` `EnterCriticalSection/LeaveCriticalSection(DenySayMsgList)` 之间对
///   `DenySayMsgList` 的 `ContainsKey`/`Remove`/两处 `SysMsg` **整段被注释** ⇒ 不清禁言、
///   不通知目标、不落库；`boAll` 只被注释代码引用 ⇒ **解析了但无副作用**。
///
/// 因此本命令的 online 行为 = 「永远一条 Red/Hint 帮助文本」，`取消禁言 <名字> true` 与
/// `<名字> false` 完全同帧；这正是 oracle 现状（BUG-REPLICA，不修）。
fn shutup_release(parameters: &str) -> GmResult {
    // `GameCommand.Handle` `case 2`：`parameters.Split(' ')`（**保留**空段：
    // `parse_command` 只 trim 两端，内部连续空格会在 `params` 里留下 `""`）。
    let params = split_params(parameters);
    let human = params.first().copied().unwrap_or("");
    // `:20` 的 `bool.Parse(@params[1])` 先求值；失败即零帧（oracle 的异常被 catch 吞掉）。
    if let Some(second) = params.get(1) {
        if parse_csharp_bool(second).is_none() {
            return no_frames(NOTE_SHUTUP_RELEASE_BOOL_PARSE);
        }
    }
    // `:20` 条件恒真（`A || !A`）；`:22-55` 整段被注释 ⇒ 唯一的可观测行为是这条帮助文本。
    let _ = human;
    red(SHUTUP_RELEASE_HELP)
}

/// `bool.Parse`（`System.Boolean.Parse`）：先 `Trim()`，再只认 `True`/`False`（大小写不敏感）；
/// 其余（含空串）抛 `FormatException`。返回 `None` = 抛异常。
fn parse_csharp_bool(s: &str) -> Option<bool> {
    let t = s.trim();
    if t.eq_ignore_ascii_case("true") {
        Some(true)
    } else if t.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// `KickHumanCommand.Execute`（`src/Modules/GameCommand/Commands/KickHumanCommand.cs:10-40`，
/// 现网 `@VV踢人`，`Command.conf` 的 `[Command] Kick=VV踢人` / `[Permission] Kick=10`）。
///
/// **实际执行行**：
/// - `:16` `if (@params == null) return;`——`GameCommand.Handle` 的 `case 2` 传
///   `parameters.Split(' ')`（不返回 null）⇒ 死分支；
/// - `:20` `sHumName = @params.Length > 0 ? @params[0] : ""`；
/// - `:21-24` `string.IsNullOrEmpty(sHumName)` ⇒ **静默 return**（无帮助文本、无帧）；
/// - `:25` `WorldEngine.GetPlayObject(sHumName)`（`WorldServer.cs:937-954`：`ChrName`
///   OrdinalIgnoreCase 命中首个、且 `!Ghost`）；
/// - `:26-31` 命中 ⇒ `mIPlayerActor.BoKickFlag = true; BoEmergencyClose = true;`
///   （两行 `m_boPlayOffLine/m_boNotOnlineAddExp` 被注释）；
/// - `:32-38` 未命中 ⇒ `SysMsg("{0} 现在不在线，或在其它服务器上!!!", Red, Hint)`。
///
/// 落地：`BoKickFlag` 支由宿主 `drain_kicks` 收口（`PlayObject.Message.cs:367-378`：先
/// `SendDefMessage(SM_OUTOFCONNECTION, 0, 0, 0, 0)`、再 `MakeGhost()`）；命令体只把目标名字装进
/// [`GmSession::pending_kick`]（与 `pending_entity` 同法：命令体纯、宿主落账）。
fn kick_human(parameters: &str, sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    // `:21-24`：空名 ⇒ 静默 return（oracle 这里**没有**任何 SysMsg，连帮助文本都不发）。
    if name.is_empty() {
        return no_frames(NOTE_KICK_HUMAN_EMPTY_NAME);
    }
    // `WorldEngine.GetPlayObject` 需要实体表；未接世界时按既有口径登记（不伪造「不在线」）。
    if world.world.is_none() {
        return no_frames(NOTE_NO_WORLD);
    }
    if let Some(entity) = world.play_by_name(name) {
        // `:26-31`：置 `BoKickFlag`/`BoEmergencyClose`，**无回执**（oracle 不发任何 SysMsg）。
        //
        // 复审 important-1：登记的必须是**实体规范名**。宿主 `online_take` 用
        // `e.chr_name == chr` 精确匹配，而 `GetPlayObject` 是 OrdinalIgnoreCase ⇒ 若存命令行
        // 输入的大小写（如 `ViCtIm`），命令说命中、宿主却在在线表里找不到（只留 WRN、不发 528）。
        sess.pending_kick = Some(entity.name.clone());
        no_frames(NOTE_KICK_HUMAN_PENDING)
    } else {
        // `:32-38`：`NowNotOnLineOrOnOtherServer`（与 `@VV调整元宝` 同一条文案）。
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

// 卡 `openmir2-gm-script-host-seam`（T-210）订正：原先这里还有一个 `execute_script_line`，
// 自己把 `sess.permission` 抬到 10 再还原——那是**第二份**抬权语义（oracle 的抬权在脚本动作
// `ActionOfGmExecute`，`ExecutionProcessingSys.cs:4938-4947`，不在命令派发层）。跳线接通后
// 两条路径会在同一次执行里抬/还两次 ⇒ 本卡**删除**该门面，抬权只保留
// `crates/script/src/engine/actions.rs` 的 `PermissionGuard`（oracle 同址、`Drop` 还原）。
// 脚本侧入口现在是宿主 `npc::NpcPlayer::process_user_line_msg`（直接调 [`execute`]）。

/// 生效命令表：185 行 = `src/Modules/GameCommand/Commands/` 下的 185 个 `GameCommand` 子类
/// （**不含**内建 `help`/`commands`）。`name`/`perm_min` 已按部署 `Command.conf` 叠加
/// `[RegisterCommand]` 覆盖后的现网值；`params` = `Execute` 形参个数。
///
/// 判据（`gmcmd::tests`）：长度 185、名字按 `eq_ignore_ascii_case` 唯一、perm 0 恰好 5 条
/// （`时间`/`拒绝`/`拒绝私聊`/`LockLogin`/`AddRobotPlay`）。
pub const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "+-jid",
        perm_min: 10,
        params: 1,
        imp: Imp::ShowDenyAccount,
    },
    CommandDef {
        name: "+-jip",
        perm_min: 10,
        params: 1,
        imp: Imp::ShowDenyIp,
    },
    CommandDef {
        name: "+-jy",
        perm_min: 10,
        params: 1,
        imp: Imp::DisableSendMsgListCmd,
    },
    CommandDef {
        name: "+jid",
        perm_min: 10,
        params: 2,
        imp: Imp::DenyAccount,
    },
    CommandDef {
        name: "+jip",
        perm_min: 10,
        params: 2,
        imp: Imp::DenyIp,
    },
    CommandDef {
        name: "+jn",
        perm_min: 10,
        params: 1,
        imp: Imp::Training,
    },
    CommandDef {
        name: "+jy",
        perm_min: 10,
        params: 2,
        imp: Imp::DisableSendMsg,
    },
    CommandDef {
        name: "-jid",
        perm_min: 10,
        params: 2,
        imp: Imp::DelDenyAccount,
    },
    CommandDef {
        name: "-jip",
        perm_min: 10,
        params: 2,
        imp: Imp::DelDenyIp,
    },
    CommandDef {
        name: "-jy",
        perm_min: 10,
        params: 2,
        imp: Imp::EnableSendMsg,
    },
    CommandDef {
        name: "1",
        perm_min: 10,
        params: 1,
        imp: Imp::ChangeAdminMode,
    },
    CommandDef {
        name: "1级",
        perm_min: 10,
        params: 2,
        imp: Imp::AdjuestLevel,
    },
    CommandDef {
        name: "2",
        perm_min: 10,
        params: 1,
        imp: Imp::ChangeObMode,
    },
    CommandDef {
        name: "3",
        perm_min: 10,
        params: 1,
        imp: Imp::ChangeSuperManMode,
    },
    CommandDef {
        name: "AddGamePoint",
        perm_min: 10,
        params: 2,
        imp: Imp::AddGameGold,
    },
    CommandDef {
        name: "AddRobotPlay",
        perm_min: 0,
        params: 2,
        imp: Imp::AddRobotPlay,
    },
    CommandDef {
        name: "AdjustExp",
        perm_min: 10,
        params: 2,
        imp: Imp::AdjustExp,
    },
    CommandDef {
        name: "Announcement",
        perm_min: 10,
        params: 2,
        imp: Imp::Announcement,
    },
    CommandDef {
        name: "AttackMode",
        perm_min: 10,
        params: 1,
        imp: Imp::ChangeAttackMode,
    },
    CommandDef {
        name: "Backstep",
        perm_min: 10,
        params: 2,
        imp: Imp::BackStep,
    },
    CommandDef {
        name: "bb",
        perm_min: 10,
        params: 2,
        imp: Imp::RecallMob,
    },
    CommandDef {
        name: "Binduseitem",
        perm_min: 10,
        params: 2,
        imp: Imp::BindUseItem,
    },
    CommandDef {
        name: "ChangeItemName",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeItemName,
    },
    CommandDef {
        name: "CHGUSERFULL",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeUserFull,
    },
    CommandDef {
        name: "CHGZENFASTSTEP",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeZenFastStep,
    },
    CommandDef {
        name: "ClearCopyItem",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ClearItemMap",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ContestPoint",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "DelBonusPoint",
        perm_min: 10,
        params: 2,
        imp: Imp::DelBonuPoint,
    },
    CommandDef {
        name: "DelDenyChrNameLogon",
        perm_min: 10,
        params: 2,
        imp: Imp::DelDenyChrName,
    },
    CommandDef {
        name: "DelGamePoint",
        perm_min: 10,
        params: 2,
        imp: Imp::DelGameGold,
    },
    CommandDef {
        name: "DelNpc",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "DenyChrNameLogon",
        perm_min: 10,
        params: 2,
        imp: Imp::DenyChrName,
    },
    CommandDef {
        name: "df56dfs",
        perm_min: 10,
        params: 1,
        imp: Imp::GuildRecall,
    },
    CommandDef {
        name: "dfsgf567",
        perm_min: 10,
        params: 2,
        imp: Imp::AllowDearRecall,
    },
    CommandDef {
        name: "DisableFilter",
        perm_min: 10,
        params: 1,
        imp: Imp::DisableFilter,
    },
    CommandDef {
        name: "dsfsdt546",
        perm_min: 10,
        params: 1,
        imp: Imp::DearRecall,
    },
    CommandDef {
        name: "EndContest",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "f",
        perm_min: 10,
        params: 2,
        imp: Imp::MapMove,
    },
    CommandDef {
        name: "fdgfdg56",
        perm_min: 10,
        params: 2,
        imp: Imp::PasswordLockGate,
    },
    CommandDef {
        name: "fdgh56j",
        perm_min: 10,
        params: 1,
        imp: Imp::AllowMasterRecall,
    },
    CommandDef {
        name: "fdht7ed",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ff",
        perm_min: 10,
        params: 2,
        imp: Imp::PositionMove,
    },
    CommandDef {
        name: "FireBurn",
        perm_min: 10,
        params: 2,
        imp: Imp::FireBurn,
    },
    CommandDef {
        name: "GameDiaMond",
        perm_min: 10,
        params: 2,
        imp: Imp::GameDiamond,
    },
    CommandDef {
        name: "GameGird",
        perm_min: 10,
        params: 2,
        imp: Imp::GameGird,
    },
    CommandDef {
        name: "GameGlory",
        perm_min: 10,
        params: 2,
        imp: Imp::GameGlory,
    },
    CommandDef {
        name: "GetUserItems",
        perm_min: 10,
        params: 2,
        imp: Imp::GetUserItems,
    },
    CommandDef {
        name: "gfh678dfg",
        perm_min: 10,
        params: 2,
        imp: Imp::PasswordLockGate,
    },
    CommandDef {
        name: "ghjhg6",
        perm_min: 10,
        params: 2,
        imp: Imp::PasswordLockGate,
    },
    CommandDef {
        name: "GiveMine",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "GuildWar",
        perm_min: 10,
        params: 1,
        imp: Imp::GuildWar,
    },
    CommandDef {
        name: "hjhg556",
        perm_min: 10,
        params: 2,
        imp: Imp::PasswordLockGate,
    },
    CommandDef {
        name: "hjhgj67",
        perm_min: 10,
        params: 2,
        imp: Imp::PasswordLockGate,
    },
    CommandDef {
        name: "HumanCountwa",
        perm_min: 10,
        params: 2,
        imp: Imp::HumanCount,
    },
    CommandDef {
        name: "Hunger",
        perm_min: 10,
        params: 2,
        imp: Imp::Hunger,
    },
    CommandDef {
        name: "IP",
        perm_min: 10,
        params: 2,
        imp: Imp::HumanLocal,
    },
    CommandDef {
        name: "jkt76",
        perm_min: 10,
        params: 1,
        imp: Imp::MasterRecall,
    },
    CommandDef {
        name: "Level0",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeLevel,
    },
    CommandDef {
        name: "LoadAdmin",
        perm_min: 10,
        params: 1,
        imp: Imp::LoadAdmin,
    },
    CommandDef {
        name: "LockLogin",
        perm_min: 0,
        params: 1,
        imp: Imp::LockLogin,
    },
    CommandDef {
        name: "LotteryTicket",
        perm_min: 10,
        params: 1,
        imp: Imp::LotteryTicket,
    },
    CommandDef {
        name: "LuckyPoint",
        perm_min: 10,
        params: 2,
        imp: Imp::LuckPoint,
    },
    CommandDef {
        name: "Map",
        perm_min: 10,
        params: 2,
        imp: Imp::MapInfo,
    },
    CommandDef {
        name: "MapInfo",
        perm_min: 10,
        params: 1,
        imp: Imp::ShowMapInfo,
    },
    CommandDef {
        // oracle 活名 = **类属性默认名** `ChangeMasterName`（`ChangeMasterNameCommand.cs:10`；
        // conf `[Command]` 无此键，`GameCmdConf.cs:313-314` 读不到 ⇒ 回落默认名）。
        // 部署 conf 里 `ChangeMasterrName=MasterName`（`:178`）是**拼错的死键**、
        // `ChangeMasterName=10`（`:349`）只是权限键 ⇒ 两者都不构成命令名。卡 `openmir2-gm-command-name-typos`。
        name: "ChangeMasterName",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeMasterName,
    },
    CommandDef {
        name: "MemberFuncEx",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Mission",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "MobCountwa",
        perm_min: 10,
        params: 2,
        imp: Imp::MobCount,
    },
    CommandDef {
        name: "MobFireBurn",
        perm_min: 10,
        params: 2,
        imp: Imp::MobFireBurn,
    },
    CommandDef {
        name: "MobNpc",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "MobPlace",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "MoveMobTo",
        perm_min: 10,
        params: 2,
        imp: Imp::MoveMobTo,
    },
    CommandDef {
        name: "NpcScript",
        perm_min: 10,
        params: 1,
        imp: Imp::NpcScript,
    },
    CommandDef {
        name: "OffHorse",
        perm_min: 10,
        params: 1,
        imp: Imp::TakeOffHorse,
    },
    CommandDef {
        name: "OnHorse",
        perm_min: 10,
        params: 1,
        imp: Imp::TakeOnHorse,
    },
    CommandDef {
        name: "PasswordLock",
        perm_min: 10,
        params: 2,
        imp: Imp::PasswordLockGate,
    },
    CommandDef {
        name: "RecallHuman",
        perm_min: 10,
        params: 2,
        imp: Imp::RecallHuman,
    },
    CommandDef {
        name: "ReCallMobEx",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Reconnection",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "RefineWeaponwa",
        perm_min: 10,
        params: 2,
        imp: Imp::RefineWeapon,
    },
    CommandDef {
        name: "ReGotoHuman",
        perm_min: 10,
        params: 2,
        imp: Imp::ReGotoHuman,
    },
    CommandDef {
        name: "ReloadAbuse",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadAbuse,
    },
    CommandDef {
        name: "ReloadGuild",
        perm_min: 10,
        params: 2,
        imp: Imp::ReloadGuild,
    },
    CommandDef {
        name: "ReloadGuildAll",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadAllGuild,
    },
    CommandDef {
        name: "ReloadItemDB",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadItemDb,
    },
    CommandDef {
        name: "ReloadMagicDB",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadMagicDb,
    },
    CommandDef {
        name: "Rest",
        perm_min: 10,
        params: 1,
        imp: Imp::ChangeSalveStatus,
    },
    CommandDef {
        name: "RestBonuPoint",
        perm_min: 10,
        params: 2,
        imp: Imp::RestBonuPoint,
    },
    CommandDef {
        name: "s",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "SabukWallGold",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "sadasd",
        perm_min: 10,
        params: 1,
        imp: Imp::AllowGuildRecall,
    },
    CommandDef {
        name: "sadsa54",
        perm_min: 10,
        params: 2,
        imp: Imp::PasswordLockGate,
    },
    CommandDef {
        name: "SbkDoor",
        perm_min: 10,
        params: 1,
        imp: Imp::SbkDoorControl,
    },
    CommandDef {
        name: "sdasdert",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ServerInfo",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "setflag",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "SetMapMode",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "setopen",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "setunit",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        // oracle 活名 = `ShowDenyChrNameLogon`（`ShowDenyCharNameLogonCommand.cs:7` 的命令名，
        // **文件名是 Char、命令名是 Chr**；`GameCmdConf.cs:339-340` 同）。conf 里的
        // `ShowDenyCharNameLogon=+-jww`（`:191`）是死键（名字与权限都不读）。
        // 卡 `openmir2-gm-command-name-typos`。
        name: "ShowDenyChrNameLogon",
        perm_min: 10,
        params: 1,
        imp: Imp::ShowDenyChrName,
    },
    CommandDef {
        name: "ShowEffect",
        perm_min: 10,
        params: 2,
        imp: Imp::ShowEffect,
    },
    CommandDef {
        name: "showflag",
        perm_min: 10,
        params: 2,
        imp: Imp::ShowFlag,
    },
    CommandDef {
        name: "ShowMapMode",
        perm_min: 10,
        params: 2,
        imp: Imp::ShowMapMode,
    },
    CommandDef {
        name: "showopen",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "showunit",
        perm_min: 10,
        params: 0,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Showuseiteminfo",
        perm_min: 10,
        params: 2,
        imp: Imp::ShowUseItemInfo,
    },
    CommandDef {
        name: "SignMapMove",
        perm_min: 10,
        params: 1,
        imp: Imp::SignMapMove,
    },
    CommandDef {
        name: "StartContest",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "SuperTingwa",
        perm_min: 10,
        params: 2,
        imp: Imp::SuperTing,
    },
    CommandDef {
        name: "TestFire",
        perm_min: 10,
        params: 2,
        imp: Imp::TestFire,
    },
    CommandDef {
        name: "TestGetBagItems",
        perm_min: 10,
        params: 1,
        imp: Imp::TestGetBagItems,
    },
    CommandDef {
        name: "TestSpeedMode",
        perm_min: 10,
        params: 1,
        imp: Imp::TestSpeedMode,
    },
    CommandDef {
        name: "TestStatus",
        perm_min: 10,
        params: 2,
        imp: Imp::TestStatus,
    },
    CommandDef {
        name: "Tingwa",
        perm_min: 10,
        params: 2,
        imp: Imp::Ting,
    },
    CommandDef {
        name: "Total",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "TrainingMagic",
        perm_min: 10,
        params: 2,
        imp: Imp::TrainingMagic,
    },
    CommandDef {
        name: "UnBindUseItem",
        perm_min: 10,
        params: 2,
        imp: Imp::UnBindUseItem,
    },
    CommandDef {
        name: "UserCmd",
        perm_min: 10,
        params: 2,
        imp: Imp::UserCmd,
    },
    CommandDef {
        name: "vv杀死",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "VV给予权限",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "VV调整元宝",
        perm_min: 10,
        params: 2,
        imp: Imp::GameGold,
    },
    CommandDef {
        name: "VV踢人",
        perm_min: 10,
        params: 2,
        imp: Imp::KickHuman,
    },
    CommandDef {
        name: "who",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "交易",
        perm_min: 10,
        params: 1,
        imp: Imp::LetTrade,
    },
    CommandDef {
        name: "传送",
        perm_min: 10,
        params: 2,
        imp: Imp::UserMoveXy,
    },
    CommandDef {
        name: "伴侣变更",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeDearName,
    },
    CommandDef {
        name: "信息",
        perm_min: 10,
        params: 2,
        imp: Imp::HumanInfo,
    },
    CommandDef {
        name: "停止叛变",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "允许天地合一",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "允许联盟",
        perm_min: 10,
        params: 1,
        imp: Imp::Authally,
    },
    CommandDef {
        name: "减钱",
        perm_min: 10,
        params: 2,
        imp: Imp::DelGold,
    },
    CommandDef {
        name: "创建行会",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "删除技能",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "删除物品",
        perm_min: 10,
        params: 2,
        imp: Imp::DeleteItem,
    },
    CommandDef {
        name: "删除行会",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "加入门派",
        perm_min: 10,
        params: 1,
        imp: Imp::LetGuild,
    },
    CommandDef {
        name: "加钱",
        perm_min: 10,
        params: 2,
        imp: Imp::AddGold,
    },
    CommandDef {
        name: "取消禁言",
        perm_min: 10,
        params: 2,
        imp: Imp::ShutupRelease,
    },
    CommandDef {
        name: "取消联盟",
        perm_min: 10,
        params: 2,
        imp: Imp::AuthCancel,
    },
    CommandDef {
        name: "增加PK",
        perm_min: 10,
        params: 2,
        imp: Imp::IncPkPoint,
    },
    CommandDef {
        name: "增加技能",
        perm_min: 10,
        params: 2,
        imp: Imp::TrainingSkill,
    },
    CommandDef {
        name: "复活",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "天地合一",
        perm_min: 10,
        params: 1,
        imp: Imp::GroupRecall,
    },
    CommandDef {
        name: "开始提问",
        perm_min: 10,
        params: 2,
        imp: Imp::StartQuest,
    },
    CommandDef {
        name: "开始攻城",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "怪",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "拒绝",
        perm_min: 0,
        params: 2,
        imp: Imp::PrvMsg,
    },
    CommandDef {
        name: "拒绝私聊",
        perm_min: 0,
        params: 1,
        imp: Imp::AllowMsg,
    },
    CommandDef {
        name: "拒绝组队聊天",
        perm_min: 10,
        params: 1,
        imp: Imp::LetShout,
    },
    CommandDef {
        name: "探测",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "改变发型",
        perm_min: 10,
        params: 2,
        imp: Imp::Hair,
    },
    CommandDef {
        name: "改变性别",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeGender,
    },
    CommandDef {
        name: "改属性",
        perm_min: 10,
        params: 2,
        imp: Imp::SmakeItem,
    },
    CommandDef {
        name: "时间",
        perm_min: 0,
        params: 1,
        imp: Imp::Date,
    },
    CommandDef {
        name: "是我",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "查看PK",
        perm_min: 10,
        params: 2,
        imp: Imp::PkPoint,
    },
    CommandDef {
        name: "查看周围",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "查看禁言",
        perm_min: 10,
        params: 1,
        imp: Imp::ShutupList,
    },
    CommandDef {
        name: "查看私聊",
        perm_min: 10,
        params: 2,
        imp: Imp::ViewWhisper,
    },
    CommandDef {
        name: "沙城",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "清理垃圾",
        perm_min: 10,
        params: 2,
        imp: Imp::ClearBagItem,
    },
    CommandDef {
        name: "清除PK",
        perm_min: 10,
        params: 2,
        imp: Imp::FreePenalty,
    },
    CommandDef {
        name: "清除仓库密码",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "清除任务",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "清除怪物",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "祈祷生效",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "禁止行会信息",
        perm_min: 10,
        params: 1,
        imp: Imp::BanGuildChat,
    },
    CommandDef {
        name: "禁言",
        perm_min: 10,
        params: 2,
        imp: Imp::Shutup,
    },
    CommandDef {
        name: "职业变更",
        perm_min: 10,
        params: 2,
        imp: Imp::ChangeJob,
    },
    CommandDef {
        name: "联盟",
        perm_min: 10,
        params: 1,
        imp: Imp::Auth,
    },
    CommandDef {
        name: "调整声望",
        perm_min: 10,
        params: 2,
        imp: Imp::CreditPoint,
    },
    CommandDef {
        name: "调整属性",
        perm_min: 10,
        params: 2,
        imp: Imp::BonuPoint,
    },
    CommandDef {
        name: "调整泡点",
        perm_min: 10,
        params: 2,
        imp: Imp::GamePoint,
    },
    CommandDef {
        name: "调整经验",
        perm_min: 10,
        params: 2,
        imp: Imp::AdjuestExp,
    },
    CommandDef {
        name: "转生",
        perm_min: 10,
        params: 2,
        imp: Imp::ReNewLevel,
    },
    CommandDef {
        name: "退出门派",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "重读GM",
        perm_min: 10,
        params: 1,
        imp: Imp::ReLoadAdmin,
    },
    CommandDef {
        name: "重读公告",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadLineNotice,
    },
    CommandDef {
        name: "重读商人",
        perm_min: 10,
        params: 2,
        imp: Imp::ReloadNpc,
    },
    CommandDef {
        name: "重读机器人脚本",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadRobot,
    },
    CommandDef {
        name: "重读机器人配置",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadRobotManage,
    },
    CommandDef {
        name: "重读爆率",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadMonItems,
    },
    CommandDef {
        name: "重读登陆",
        perm_min: 10,
        params: 1,
        imp: Imp::ReloadManage,
    },
    CommandDef {
        name: "集体传送",
        perm_min: 10,
        params: 2,
        imp: Imp::MapMoveHuman,
    },
];

const NOTE_GM4_NOOP: &str = "批次 4：oracle 命令体为空/只有权限自检 ⇒ no-op（零帧）";

// ============================================================================================
// 批次 4：数值/角色族 44 条（设计件 §4.3 第 72–115 行；tier=R）
//
// 本批的统一前提（逐条登记 T-186..T-191，见 `mir2-rs/tests/parity/whitelist.md`）：
//
// 1. **目标数值态缺口**：`GmWorld` 只有 `&World` 只读视图。`Entity` **确实携带**
//    `gold/level/exp/pk_point/luck/use_items` 等字段，但玩家 `Entity.gold` 生产无写点（恒 0，
//    读它＝伪造数值），`level/exp` 可读而本批要写的 `gold/game_gold/...` 无写通道；真值是
//    `SessionGround` 的 `state.gold`/`game_gold`/`state.item_list`，未暴露给 `GmWorld`。
//    凡「目标在线」之后要读这些值的分支（含 `X的金币已增加{n}.` 里会被 `GoldMax` 夹取的 `n`），一律
//    `log_note` 登记，**不伪造**（任务书：不得伪造即时落库；**禁止**读 `Entity.gold`）。
// 2. **目标侧 SysMsg 缺口**：oracle 里 `mIPlayerActor.SysMsg(...)`（发给**其它角色**）本仓无
//    通道——`GmResult.msgs` 只承载发起者自己的帧 ⇒ 只出发起者那条，目标那条登记。
// 3. **可变副作用缺口**：`GmWorld` 无 `&mut World` ⇒ 所有写值（金币/等级/属性/背包/技能/配置）
//    登记，不留假实现。
//
// 与宿主能力无关的部分（参数解析、空参/`?` 前缀/越界/控制符集合/`StrToInt` 缺省值/异常路径）
// 必须 1:1，逐行照抄对应 `.cs`。
// ============================================================================================

const NOTE_CHANGE_LEVEL_PENDING: &str =
    "@Level0：命令体只登记等级写（写实体 + HasLevelUp 语义由宿主同事件落地）；oracle 命令体自身零 SysMsg、零帧";
const NOTE_GM4_TARGET_STATE: &str =
    "批次 4：目标角色数值态无读取/写回通道（真值在 SessionGround；Entity.gold 恒 0 不可读，T-186）⇒ 效果与依赖该值的文本未接";
const NOTE_GM4_TARGET_SYSMSG: &str =
    "批次 4：发给其它角色的 SysMsg 无通道（T-187）⇒ 只出 GM 侧那条";
const NOTE_GM4_BAG: &str = "批次 4：背包/物品绑定宿主未接（T-188）";
const NOTE_GM4_MAGIC: &str = "批次 4：魔法库/技能表宿主未接（T-189）";
const NOTE_GM4_CONFIG: &str = "批次 4：Config 运行期写点未接（T-190）";
const NOTE_GM4_EXCEPTION: &str =
    "批次 4：oracle 在此抛异常（被 ProcessUserLineMsg 的 catch 吞掉）⇒ 零帧";
const NOTE_GM4_FRONT_SERVER: &str =
    "批次 4：FindOtherServerUser（跨服在线表）宿主未接（T-190）⇒ 按「不在线」支";

/// 两条 SysMsg 的便捷构造（`Vec<GmMsg>` 顺序即发送顺序）。
fn two_msgs(first: (GmColor, String), second: (GmColor, String)) -> GmResult {
    GmResult {
        found: true,
        parsed: true,
        msgs: vec![
            GmMsg {
                color: first.0,
                text: first.1,
            },
            GmMsg {
                color: second.0,
                text: second.1,
            },
        ],
        frames: Vec::new(),
        log_note: None,
    }
}

fn noted(mut result: GmResult, note: &'static str) -> GmResult {
    result.log_note = Some(note);
    result
}

/// 目标是否在线（`WorldEngine.GetPlayObject`）。
fn target_online(world: &GmWorld<'_>, name: &str) -> bool {
    world.play_by_name(name).is_some()
}

/// `AddGameGoldCommand`（现网 `@AddGamePoint`）。
fn add_game_gold(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let point = params.get(1).map_or(0, |s| str_to_int(s, 0));
    // oracle 体另有 `Permission < 6 ⇒ return`：`perm_min = 10` 的门先挡，与该判断同效。
    if name.is_empty() || point <= 0 {
        return red(ADD_GAME_POINT_HELP);
    }
    if target_online(world, name) {
        // 在线支：「游戏点已增加{n}.」+ GM 侧「{name}的游戏点已增加{n}.」，n 依赖目标 GameGold。
        no_frames(NOTE_GM4_TARGET_STATE)
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `AddGoldCommand`（现网 `@加钱`）。
fn add_gold(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let count = params.get(1).map_or(0, |s| str_to_int(s, 0));
    if name.is_empty() || count <= 0 {
        return red(ADD_GOLD_HELP);
    }
    if target_online(world, name) {
        // 在线支：`Gold + n < GoldMax` 才全额，否则夹取到 `GoldMax - Gold` ⇒ 文本依赖目标 Gold。
        no_frames(NOTE_GM4_TARGET_STATE)
    } else {
        // `FindOtherServerUser` 命中支发绿字「{name} 现在{n}号服务器上」；跨服表未接（T-190）。
        noted(
            green(format!("{name} 现在不在线，等其上线时金币将自动增加")),
            NOTE_GM4_FRONT_SERVER,
        )
    }
}

/// `AdjuestExpCommand`（现网 `@调整经验`）。
fn adjuest_exp(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(ADJUEST_EXP_HELP);
    }
    if target_online(world, name) {
        // 文本恒定（Green），但 `Exp` 写值与 `HasLevelUp` 未接。
        noted(
            green(format!("{name} 经验调整完成。")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `AdjuestLevelCommand`（现网 `@1级`）。
fn adjuest_level(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(ADJUEST_LEVEL_HELP);
    }
    if target_online(world, name) {
        // 注意 oracle 不读第二个参数来设等级：只 `HasLevelUp(1)`（升级一拍）。
        noted(
            green(format!("{name} 等级调整完成。")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `AdjustExpCommand`（现网 `@AdjustExp`）：命令体只有 `Permission < 6 ⇒ return` ⇒ 零帧。
fn adjust_exp() -> GmResult {
    no_frames(NOTE_GM4_NOOP)
}

/// `SystemShare.GetUseItemIdx`（`SystemShare.cs:1834-1891`）——**编译期常量表**：13 个
/// `MessageSettings` const 装备位名（`MessageSettings.cs:15-27`），`String.Compare(...,
/// OrdinalIgnoreCase) == 0` 逐名比 ⇒ 无任何宿主依赖（review F1 订正：此前误判「宿主未接
/// 恒 -1」，导致合法槽名输入类被整类判成无偏差）。未知名 ⇒ -1。
fn use_item_idx(name: &str) -> i32 {
    if name.eq_ignore_ascii_case("衣服") {
        0
    } else if name.eq_ignore_ascii_case("武器") {
        1
    } else if name.eq_ignore_ascii_case("照明物") {
        2
    } else if name.eq_ignore_ascii_case("项链") {
        3
    } else if name.eq_ignore_ascii_case("头盔") {
        4
    } else if name.eq_ignore_ascii_case("左手镯") {
        5
    } else if name.eq_ignore_ascii_case("右手镯") {
        6
    } else if name.eq_ignore_ascii_case("左戒指") {
        7
    } else if name.eq_ignore_ascii_case("右戒指") {
        8
    } else if name.eq_ignore_ascii_case("物品") {
        9
    } else if name.eq_ignore_ascii_case("腰带") {
        10
    } else if name.eq_ignore_ascii_case("鞋子") {
        11
    } else if name.eq_ignore_ascii_case("宝石") {
        12
    } else {
        -1
    }
}

/// `BindUseItemCommand`（现网 `@Binduseitem`）。
fn bind_use_item(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    // oracle `@params == null || @params.Length <= 0`（`BindUseItemCommand.cs:15`）在真实调用路径下
    // **永假**：`GameCommand.Handle` 恒传 `parameters.Split(' ')`（`GameCommand.cs:46`），而
    // `ExtractCommandAndParameters` 对无参命令给 `string.Empty`（`CommandSystem.cs:228-250`），
    // .NET `"".Split(' ')` = `[""]`（长度 1）⇒ 无参 `@Binduseitem` 继续走下面的门，
    // 由 `GetUseItemIdx("") = -1` 落到红字 CommandHelp。故**不写**该早退（写了会让无参路径零帧）。
    let name = params.first().copied().unwrap_or("");
    let item_name = params.get(1).copied().unwrap_or("");
    let bind_type = params.get(2).copied().unwrap_or("");
    // `GetUseItemIdx(sItem)`：编译期常量表（见 [`use_item_idx`]），合法槽名可达。
    let item_idx = use_item_idx(item_name);
    let bind = if bind_type.eq_ignore_ascii_case("帐号") {
        0
    } else if bind_type.eq_ignore_ascii_case("人物") {
        1
    } else if bind_type.eq_ignore_ascii_case("IP") {
        2
    } else if bind_type.eq_ignore_ascii_case("死亡") {
        3
    } else {
        -1
    };
    if item_idx < 0 || bind < 0 || name.is_empty() {
        return red(BIND_USE_ITEM_HELP);
    }
    // `sHumanName[1]` 是上面 `||` 链的最后一项（`BindUseItemCommand.cs:42`）：只有前面全假才求值
    // ⇒ 单 UTF-16 码元名在这里抛 `IndexOutOfRangeException`（外层 catch ⇒ 零帧）。
    match second_utf16_is_question(name) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return red(BIND_USE_ITEM_HELP),
        Some(false) => {}
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // 在线：`userItem = mIPlayerActor.UseItems[nItem]`（`BindUseItemCommand.cs:53`）——
    // 空槽（`Index == 0`）⇒ 红 `{名}的{槽}没有戴物品!!!`（:53-59）；
    // 已穿戴 ⇒ `switch (nBind)` 各 case 触 `SystemShare.ItemBind*`（`SystemShare.cs:112-115`
    // 声明即 null 全仓零赋值）⇒ :73 起 `EnterCriticalSection(null)` NRE ⇒ 被
    // `PlayObject.Chat.cs:402` 顶层 catch 吞 ⇒ **零帧**（扫盲卡 review F1 第 8 条偏差，
    // 合法槽名输入类；批次 4 stub 既有面）。
    let Some(target) = world.play_by_name(name) else {
        return red(dotnet_format(NOW_NOT_ON_LINE_FMT, &[name.to_string()]));
    };
    let slot_present = usize::try_from(item_idx)
        .ok()
        .and_then(|i| target.use_items.get(i))
        .is_some_and(|slot| slot.index != 0);
    if !slot_present {
        return red(dotnet_format(
            BIND_NO_ITEM_FMT,
            &[name.to_string(), item_name.to_string()],
        ));
    }
    no_frames(NOTE_SYSTEMSHARE_NULL_TABLE)
}

/// `BonuPointCommand`（现网 `@调整属性`）。
fn bonu_point(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let count = params.get(1).map_or(0, |s| str_to_int(s, 0));
    if name.is_empty() {
        return red(BONU_POINT_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // `count > 0` ⇒ `BonusPoint = nCount` + `RM_ADJUST_BONUS`（零文本）；
    // `count <= 0` ⇒ 查询支文本要 `BonusPoint` 与九项 `BonusAbil`。两者都要目标数值态。
    let _ = count;
    no_frames(NOTE_GM4_TARGET_STATE)
}

/// `ChangeAdminModeCommand`（现网 `@1`）：翻转 `AdminMode`，绿字。
fn change_admin_mode(sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let pending = sess.entity_write(world);
    sess.admin_mode = !sess.admin_mode;
    let on = sess.admin_mode;
    sess.stash(pending, |st| st.admin_mode = Some(on));
    green(if on { ADMIN_MODE_ON } else { ADMIN_MODE_OFF })
}

/// `ChangeAttackModeCommand`（现网 `@AttackMode`）：oracle 连加两次（照抄），再发 `SM_ATTACKMODE`。
fn change_attack_mode(sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    const HAM_PKATTACK: u8 = 6;
    let mut mode = sess.attack_mode;
    if mode >= HAM_PKATTACK {
        mode = 0;
    } else if mode < HAM_PKATTACK {
        mode += 1;
    } else {
        mode = 0;
    }
    if mode < HAM_PKATTACK {
        mode += 1;
    } else {
        mode = 0;
    }
    let pending = sess.entity_write(world);
    sess.attack_mode = mode;
    sess.stash(pending, |st| st.attack_mode = Some(mode));
    let text = match mode {
        0 => ATTACK_MODE_ALL,
        1 => ATTACK_MODE_PEACEFUL,
        2 => ATTACK_MODE_DEAR,
        3 => ATTACK_MODE_MASTER,
        4 => ATTACK_MODE_GROUP,
        5 => ATTACK_MODE_GUILD,
        _ => ATTACK_MODE_RED_WHITE,
    };
    // 第二张帧 `SendDefMessage(SM_ATTACKMODE, (byte)AttatckMode, 0, 0, 0)`
    // （`ChangeAttackModeCommand.cs:65`）：**排在 `SysMsg` 之后**（oracle 同序），
    // 模式值与本命令写进 `Entity` 的权威值同源。
    let mut result = green(text);
    result.frames.push(GmFrame::AttackMode(mode));
    result
}

/// `ChangeDearNameCommand`（现网 `@伴侣变更`）。
fn change_dear_name(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let dear = params.get(1).copied().unwrap_or("");
    if name.is_empty() || dear.is_empty() {
        return red(CHANGE_DEAR_NAME_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    if dear.eq_ignore_ascii_case("无") {
        noted(
            green(format!("{name} 的配偶名清除成功。")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        noted(
            green(format!("{name} 的配偶名更改成功。")),
            NOTE_GM4_TARGET_STATE,
        )
    }
}

/// `ChangeGenderCommand`（现网 `@改变性别`）。
fn change_gender(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let sex = params.get(1).copied().unwrap_or("");
    // oracle 是**大小写敏感**的 `==`（`Man`/`WoMan`），不是 OrdinalIgnoreCase。
    let n_sex = if sex == "Man" || sex == "男" || sex == "0" {
        0
    } else if sex == "WoMan" || sex == "女" || sex == "1" {
        1
    } else {
        -1
    };
    if name.is_empty() || n_sex == -1 {
        return red(CHANGE_GENDER_HELP);
    }
    if target_online(world, name) {
        // 「已改变 / 未改变!!!」取决于目标当前 `Gender` ⇒ 宿主不可读。
        no_frames(NOTE_GM4_TARGET_STATE)
    } else {
        red(format!("{name}没有在线!!!"))
    }
}

/// `ChangeItemNameCommand`（现网 `@ChangeItemName`）：生效写入整段被注释 ⇒ 恒定红字。
fn change_item_name(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let make_index = params.first().copied().unwrap_or("");
    let item_index = params.get(1).copied().unwrap_or("");
    let item_name = params.get(2).copied().unwrap_or("");
    if make_index.is_empty() || item_index.is_empty() || item_name.is_empty() {
        return red(CHANGE_ITEM_NAME_HELP);
    }
    let n_make = str_to_int(make_index, -1);
    let n_item = str_to_int(item_index, -1);
    if n_make <= 0 || n_item < 0 {
        return red(CHANGE_ITEM_NAME_HELP);
    }
    red(ITEM_NAME_OCCUPIED)
}

/// `ChangeJobCommand`（现网 `@职业变更`）。
fn change_job(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let job = params.get(1).copied().unwrap_or("");
    if name.is_empty() || job.is_empty() {
        return red(CHANGE_JOB_HELP);
    }
    if target_online(world, name) {
        // 目标侧「职业更改成功。」无通道；职业写值未接。
        noted(
            green(format!("{name} 的职业更改成功。")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `ChangeLevelCommand`（现网 `@Level0`）：只改自己等级，无可观察帧。
/// `ChangeLevelCommand`（现网 `@Level0`，`ChangeLevelCommand.cs:10-33`）。
///
/// oracle 逐行：`nLevel = HUtil32.StrToInt(sParam1, 1)`（`:21`，`int.TryParse` 失败才用 1）；
/// `Abil.Level = (byte)HUtil32._MIN(MessageSettings.MAXUPLEVEL(255), nLevel)`（`:23`，**unchecked
/// 强转** ⇒ `-1` 得 255、`-300` 得 212，**不得**先钳 0）；随后 `HasLevelUp(1)`（`:24`）；
/// 命令体自身**不发任何 SysMsg/帧**（只写控制台日志，`ShowMakeItemMsg` 门内）。
///
/// 本仓命令体纯：只把等级装进 `pending_entity.level`（写实体）与 `pending_level_up`
/// （宿主落地 `HasLevelUp` 语义 + `RM_LEVELUP` 三帧）。
fn change_level(parameters: &str, sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let n_level = str_to_int(params.first().copied().unwrap_or(""), 1);
    // `(byte)HUtil32._MIN(255, n)`：`min` 之后再 `as u8`，负数按二进制补码截断（= C# unchecked）。
    let level = n_level.min(255) as u8;
    let pending = sess.entity_write(world);
    sess.stash(pending, |st| st.level = Some(level));
    sess.pending_level_up = Some(level);
    no_frames(NOTE_CHANGE_LEVEL_PENDING)
}

/// `ChangeMasterNameCommand`（现网 `@ChangeMasterName`）。
fn change_master_name(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let master = params.get(1).copied().unwrap_or("");
    if name.is_empty() || master.is_empty() {
        return red(CHANGE_MASTER_NAME_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    if master.eq_ignore_ascii_case("无") {
        noted(
            green(format!("{name} 的师徒名清除成功。")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        noted(
            green(format!("{name} 的师徒名更改成功。")),
            NOTE_GM4_TARGET_STATE,
        )
    }
}

/// `ChangeObModeCommand`（现网 `@2`）：翻转 `ObMode`；进入时先发 `RM_DISAPPEAR`（本仓未接）。
fn change_ob_mode(sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let pending = sess.entity_write(world);
    sess.ob_mode = !sess.ob_mode;
    let on = sess.ob_mode;
    sess.stash(pending, |st| st.ob_mode = Some(on));
    let result = green(if on {
        OBSERVER_MODE_ON
    } else {
        OBSERVER_MODE_OFF
    });
    if on {
        // 进入隐身时 oracle 还先发 `RM_DISAPPEAR`（`ChangeObModeCommand.cs:20`，"解决隐身有影子问题"）；
        // 本仓该帧仍无通道（T-187），这一项**只登记不实现**——权威 `ObMode` 已写进 `Entity`。
        noted(result, NOTE_GM4_TARGET_SYSMSG)
    } else {
        result
    }
}

/// `ChangeSalveStatusCommand`（现网 `@Rest`）：`SlaveList == null ⇒ return`，否则翻转 `SlaveRelax`。
fn change_salve_status(sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    // 本仓 `Entity::slave_list` 是 `Vec`，永不 `null`；`None` 视图按空表处理。
    let pending = sess.entity_write(world);
    sess.slave_relax = !sess.slave_relax;
    let on = sess.slave_relax;
    sess.stash(pending, |st| st.slave_relax = Some(on));
    let has_slaves = world
        .self_entity()
        .is_some_and(|e| !e.slave_list.is_empty());
    if !has_slaves {
        return GmResult::empty();
    }
    green(if on { PET_REST } else { PET_ATTACK })
}

/// `ChangeSuperManModeCommand`（现网 `@3`）：翻转 `SuperMan`，绿字。
fn change_super_man_mode(sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let pending = sess.entity_write(world);
    sess.super_man = !sess.super_man;
    let on = sess.super_man;
    sess.stash(pending, |st| st.super_man = Some(on));
    green(if on {
        SUPERMAN_MODE_ON
    } else {
        SUPERMAN_MODE_OFF
    })
}

/// `ChangeUserFullCommand`（现网 `@CHGUSERFULL`）。
///
/// **oracle 古怪的门**：`if (IsNullOrEmpty(sUserCount) || nCount < 1 || !IsNullOrEmpty(sUserCount))`
/// —— 第三个子句与前两个互补 ⇒ 条件**恒真**，命令体永远走「两句红字」，`Config.UserFull` 永不被写。
/// 照抄（设计件 §4.5 #7：oracle 怪癖原样搬）。
fn change_user_full(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let user_count = params.first().copied().unwrap_or("");
    let n_count = str_to_int(user_count, -1);
    if user_count.is_empty() || n_count < 1 || !user_count.is_empty() {
        return two_msgs(
            (GmColor::Red, USER_FULL_TITLE.to_string()),
            (GmColor::Red, CHANGE_USER_FULL_HELP.to_string()),
        );
    }
    noted(
        green(format!("服务器上线人数限制: {n_count}")),
        NOTE_GM4_CONFIG,
    )
}

/// `ChangeZenFastStepCommand`（现网 `@CHGZENFASTSTEP`）：同上恒真门。
fn change_zen_fast_step(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let fast_step = params.first().copied().unwrap_or("");
    let n_step = str_to_int(fast_step, -1);
    if fast_step.is_empty() || n_step < 1 || !fast_step.is_empty() {
        return two_msgs(
            (GmColor::Red, ZEN_FAST_STEP_TITLE.to_string()),
            (GmColor::Red, CHANGE_ZEN_FAST_STEP_HELP.to_string()),
        );
    }
    noted(green(format!("怪物行动速度: {n_step}")), NOTE_GM4_CONFIG)
}

/// `CreditPointCommand`（现网 `@调整声望`）。
fn credit_point(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let ctr_raw = params.get(1).copied().unwrap_or("");
    let point = params.get(2).map_or(0, |s| str_to_int(s, 0));
    let ctr = ctr_raw.chars().next().unwrap_or('1');
    if name.is_empty() || !matches!(ctr, '=' | '+' | '-') || point < 0 || name.starts_with('?') {
        return red(CREDIT_POINT_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // 两条文本都要目标**改动后**的 CreditPoint（byte 截断）⇒ 宿主不可读。
    no_frames(NOTE_GM4_TARGET_STATE)
}

/// `DelBonuPointCommand`（现网 `@DelBonusPoint`）。
fn del_bonu_point(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(DEL_BONU_POINT_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // 目标侧「分配点数已清除!!!」无通道；`BonusPoint = 0` 未接。
    noted(
        green(format!("{name} 的分配点数已清除.")),
        NOTE_GM4_TARGET_SYSMSG,
    )
}

/// `DeleteItemCommand`（现网 `@删除物品`）：遍历目标背包名字匹配删除。
fn delete_item(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let item = params.get(1).copied().unwrap_or("");
    if name.is_empty() || item.is_empty() {
        return red(DELETE_ITEM_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    noted(GmResult::empty(), NOTE_GM4_BAG)
}

/// `DelGameGoldCommand`（现网 `@DelGamePoint`）。
fn del_game_gold(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let point = params.get(1).map_or(0, |s| str_to_int(s, 0));
    if name.is_empty() || point <= 0 {
        return red(DEL_GAME_GOLD_HELP);
    }
    if target_online(world, name) {
        no_frames(NOTE_GM4_TARGET_STATE)
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `DelGoldCommand`（现网 `@减钱`）。
fn del_gold(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let count = params.get(1).map_or(0, |s| str_to_int(s, 0));
    // 第一道门**静默**返回（无 SysMsg）——照抄。
    if name.is_empty() || count <= 0 {
        return GmResult::empty();
    }
    if target_online(world, name) {
        no_frames(NOTE_GM4_TARGET_STATE)
    } else {
        // 注意与 `@加钱` 的离线文案不同：无空格、且是「减少」。
        noted(
            green(format!("{name}现在不在线，等其上线时金币将自动减少")),
            NOTE_GM4_FRONT_SERVER,
        )
    }
}

/// `FreePenaltyCommand`（现网 `@清除PK`）。
fn free_penalty(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.starts_with('?') {
        return red(FREE_PENALTY_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // 目标侧「你的PK值已经被清除...」无通道；`PkPoint = 0` 未接。
    noted(
        green(format!("{name}的PK值已经被清除...")),
        NOTE_GM4_TARGET_SYSMSG,
    )
}

/// `GameDiaMondCommand`（现网 `@GameDiaMond`）：`Execute` 体为空 ⇒ 零帧。
fn game_diamond() -> GmResult {
    no_frames(NOTE_GM4_NOOP)
}

/// `GameGirdCommand`（现网 `@GameGird`）：`Execute` 体为空 ⇒ 零帧。
fn game_gird() -> GmResult {
    no_frames(NOTE_GM4_NOOP)
}

/// `GameGloryCommand`（现网 `@GameGlory`）：`Execute` 体为空 ⇒ 零帧。
fn game_glory() -> GmResult {
    no_frames(NOTE_GM4_NOOP)
}

/// `GameGoldCommand`（现网 `@VV调整元宝`）。
fn game_gold(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let ctr_raw = params.get(1).copied().unwrap_or("");
    let gold = params.get(2).map_or(0, |s| str_to_int(s, 0));
    let ctr = ctr_raw.chars().next().unwrap_or('1');
    if name.is_empty() || !matches!(ctr, '=' | '+' | '-') || !(0..=200_000_000).contains(&gold) {
        return red(GAME_GOLD_HELP);
    }
    // `sHumanName[1] == '?'` 是 oracle `||` 链的**最后一项**（`GameGoldCommand.cs:30`）：前面全假才求值
    // ⇒ 单 UTF-16 码元名抛 `IndexOutOfRangeException`（外层 catch ⇒ 零帧），不能当成「不是 ?」。
    match second_utf16_is_question(name) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return red(GAME_GOLD_HELP),
        Some(false) => {}
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    no_frames(NOTE_GM4_TARGET_STATE)
}

/// `GamePointCommand`（现网 `@调整泡点`）。
///
/// 两处 oracle 怪癖照抄：① `Convert.ToUInt16` 解析失败**抛异常**（被外层 catch 吞掉 ⇒ 零帧）；
/// ② 取值 switch 用 `sCtr[1]`（下标 1）⇒ 控制符只有 1 个字符时**抛越界**（同样零帧）。
fn game_point(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let ctr_raw = params.get(1).copied().unwrap_or("");
    let ctr = ctr_raw.chars().next().unwrap_or('1');
    // oracle 先做 `Convert.ToUInt16(@params[2])`（`GamePointCommand.cs:24`）再做空名早退：
    // 空串/负数/超 65535 都抛异常（被外层 catch 吞掉 ⇒ 零帧），**包括**尾随空格产生的空 token。
    let point = match params.get(2).copied() {
        Some(s) => match s.trim().parse::<u16>() {
            Ok(v) => i32::from(v),
            Err(_) => return no_frames(NOTE_GM4_EXCEPTION),
        },
        None => 0,
    };
    if name.is_empty() {
        return GmResult::empty();
    }
    if !matches!(ctr, '=' | '+' | '-') || point > 100_000_000 {
        return red(GAME_POINT_HELP);
    }
    // `sHumanName[1] == '?'` 是 oracle `||` 链的最后一项（`GamePointCommand.cs:33-34`）。
    match second_utf16_is_question(name) {
        None => return no_frames(NOTE_INDEX_OUT_OF_RANGE),
        Some(true) => return red(GAME_POINT_HELP),
        Some(false) => {}
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // `switch (sCtr[1])`：控制符不足 2 个 UTF-16 单元 ⇒ 越界异常 ⇒ 零帧（无 SysMsg）。
    if ctr_raw.encode_utf16().count() < 2 {
        return no_frames(NOTE_GM4_EXCEPTION);
    }
    no_frames(NOTE_GM4_TARGET_STATE)
}

/// `GetUserItemsCommand`（现网 `@GetUserItems`）：在目标身上找物品并复制到自己背包。
fn get_user_items(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let item = params.get(1).copied().unwrap_or("");
    let count = params.get(2).copied().unwrap_or("");
    let kind = params.get(3).copied().unwrap_or("");
    if name.is_empty() || item.is_empty() || count.is_empty() || kind.is_empty() {
        return red(GET_USER_ITEMS_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    noted(GmResult::empty(), NOTE_GM4_BAG)
}

/// `HairCommand`（现网 `@改变发型`）。
fn hair(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let value = params.get(1).map_or(0, |s| str_to_int(s, 0));
    if name.is_empty() || value < 0 {
        return red(HAIR_HELP);
    }
    if target_online(world, name) {
        noted(
            green(format!("{name} 的头发已改变。")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        red(format!("{name} 现在不在线，或在其它服务器上!!!"))
    }
}

/// `HungerCommand`（现网 `@Hunger`）。
fn hunger(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let point = params.get(1).map_or(-1, |s| str_to_int(s, 0));
    // oracle 体另有 `Permission < 6 ⇒ return`：`perm_min = 10` 的门先挡。
    if name.is_empty() || point < 0 {
        return red(HUNGER_HELP);
    }
    if target_online(world, name) {
        // 目标 `RM_MYSTATUS`/`RefMyStatus` 未接，`HungerStatus` 写值未接。
        noted(
            green(format!("{name} 的能量值已改变。")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        red(format!("{name}没有在线!!!"))
    }
}

/// `IncPkPointCommand`（现网 `@增加PK`）。
fn inc_pk_point(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let point = params.get(1).map_or(0, |s| str_to_int(s, 0));
    if name.is_empty() {
        return red(INC_PK_POINT_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    if point > 0 {
        noted(
            green(format!("{name}的PK值已增加{point}点...")),
            NOTE_GM4_TARGET_STATE,
        )
    } else {
        noted(
            green(format!("{name}的PK值已减少{}点...", -point)),
            NOTE_GM4_TARGET_STATE,
        )
    }
}

/// `LuckPointCommand`（现网 `@LuckyPoint`）。
fn luck_point(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let ctr = params.get(1).copied().unwrap_or("");
    let point = params.get(2).copied().unwrap_or("");
    if name.is_empty() || name.starts_with('?') {
        return red(LUCK_POINT_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    if ctr.is_empty() {
        // 文本要 `BodyLuckLevel`/`BodyLuck`/`Luck` 三项 ⇒ 宿主不可读。
        return no_frames(NOTE_GM4_TARGET_STATE);
    }
    // `=`/`-`/`+` 三支只改值、不发帧。
    let _ = str_to_int(point, 0);
    no_frames(NOTE_GM4_TARGET_STATE)
}

/// `PKpointCommand`（现网 `@查看PK`）。
///
/// **oracle 古怪的门**：`if (!IsNullOrEmpty(sHumanName)) { help; return; }` —— 给了名字就恒红字；
/// 只有**空名字**才往下走，而 `GetPlayObject("")` 必然 `null` ⇒ 恒落「(空名) 现在不在线…」。
fn pk_point(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if !name.is_empty() {
        return red(PK_POINT_HELP);
    }
    red(format!("{name} 现在不在线，或在其它服务器上!!!"))
}

/// `RefineWeaponCommand`（现网 `@RefineWeaponwa`）：改自己武器 Desc，无 SysMsg。
fn refine_weapon(parameters: &str) -> GmResult {
    let params = split_params(parameters);
    let dc = params.first().map_or(0, |s| str_to_int(s, 0));
    let mc = params.get(1).map_or(0, |s| str_to_int(s, 0));
    let sc = params.get(2).map_or(0, |s| str_to_int(s, 0));
    let hit = params.get(3).map_or(0, |s| str_to_int(s, 0));
    let _ = (dc, mc, sc, hit);
    if dc + mc + sc > 10 {
        return GmResult::empty();
    }
    // `UseItems[Weapon]` 空/Index<=0 ⇒ 静默；否则写 Desc + `SendUpdateItem` + 重算属性（全未接）。
    no_frames(NOTE_GM4_BAG)
}

/// `ReNewLevelCommand`（现网 `@转生`）。
///
/// oracle 打印的是**发起者**的 `ReLevel`（`PlayerActor.ReLevel`），不是目标的；发起者给自己改转生时
/// 两值同名同体。本仓用 `GmSession::re_level` 复刻发起者字段。
fn re_new_level(parameters: &str, sess: &mut GmSession, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let level = params.get(1).copied().unwrap_or("");
    if name.is_empty() || name.starts_with('?') {
        return red(RE_NEW_LEVEL_HELP);
    }
    let n_level = str_to_int(level, -1);
    if !target_online(world, name) {
        return red(format!("{name} 没在线上!!!"));
    }
    if (0..=255).contains(&n_level) && name.eq_ignore_ascii_case(world.speaker) {
        // 只有「给自己改」时发起者字段才真的变化（其余情况 oracle 打印的仍是原值）。
        sess.re_level = u8::try_from(n_level).unwrap_or(sess.re_level);
    }
    noted(
        green(format!("{name} 的转生等级为 {}", sess.re_level)),
        NOTE_GM4_TARGET_STATE,
    )
}

/// `RestBonuPointCommand`（现网 `@RestBonuPoint`）。
fn rest_bonu_point(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    if name.is_empty() {
        return red(REST_BONU_POINT_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // 目标侧「分配点数已复位!!!」无通道；`BonusPoint`/`BonusAbil` 读写未接。
    noted(
        green(format!("{name} 的分配点数已复位.")),
        NOTE_GM4_TARGET_SYSMSG,
    )
}

/// `TrainingCommand`（现网 `@+jn`）：命令体只有 `Permission < 6 ⇒ return` ⇒ 零帧。
fn training() -> GmResult {
    no_frames(NOTE_GM4_NOOP)
}

/// `TrainingMagicCommand`（现网 `@TrainingMagic`）。
fn training_magic(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let skill = params.get(1).copied().unwrap_or("");
    let level = params.get(2).map_or(0, |s| str_to_int(s, 0));
    if name.starts_with('?')
        || name.is_empty()
        || skill.is_empty()
        || level < 0
        || !(0..=3).contains(&level)
    {
        return red(TRAINING_MAGIC_HELP);
    }
    if !target_online(world, name) {
        return red(format!("{name} 现在不在线，或在其它服务器上!!!"));
    }
    // `FindMagic` 与目标 `MagicList` 宿主未接 ⇒「技能名称不正确!!!」/「已修炼过!!!」判不出。
    noted(GmResult::empty(), NOTE_GM4_MAGIC)
}

/// `TrainingSkillCommand`（现网 `@增加技能`）。
///
/// oracle 的循环体只做 `userMagic = MagicList[i]`（`UserMagic` 是引用类型，循环后什么都没改）
/// ⇒ **零帧、零副作用**；只有离线支有帧。
fn training_skill(parameters: &str, world: &GmWorld<'_>) -> GmResult {
    let params = split_params(parameters);
    let name = params.first().copied().unwrap_or("");
    let skill = params.get(1).copied().unwrap_or("");
    let level = params.get(2).map_or(0, |s| str_to_int(s, 0));
    if name.is_empty() || skill.is_empty() || level <= 0 {
        return red(TRAINING_SKILL_HELP);
    }
    if !target_online(world, name) {
        // 注意：本条离线文案与 `CommandHelp.NowNotOnLineOrOnOtherServer` **不同**
        // （无「现在」、只有两个 `!`、名字后无空格）。
        return red(format!("{name}不在线，或在其它服务器上!!"));
    }
    no_frames(NOTE_GM4_MAGIC)
}

/// `UnBindUseItemCommand`（现网 `@UnBindUseItem`）：命令体整段注释 ⇒ 零帧。
fn unbind_use_item() -> GmResult {
    no_frames(NOTE_GM4_NOOP)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(line: &str) -> (GmResult, GmSession) {
        run_with(&GmConfig::default(), line)
    }

    fn run_with(cfg: &GmConfig, line: &str) -> (GmResult, GmSession) {
        let mut sess = GmSession::default();
        let r = exec(&mut sess, cfg, line);
        (r, sess)
    }

    /// 批次 2/3 的命令全是 `PermissionMin = 10`（现网 `Permission` 恒 0 ⇒ 不可达），
    /// 用例里显式构造 `permission = 10` 才进得了命令体。
    fn run_gm(line: &str) -> (GmResult, GmSession) {
        run_gm_with(&GmConfig::default(), line)
    }

    fn run_gm_with(cfg: &GmConfig, line: &str) -> (GmResult, GmSession) {
        let mut sess = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        let r = exec(&mut sess, cfg, line);
        (r, sess)
    }

    /// 每个用例新建一个空 `GuildManager`（`GmWorld` 的借用源）。
    fn exec(sess: &mut GmSession, cfg: &GmConfig, line: &str) -> GmResult {
        let guilds = mir2_world::GuildManager::new();
        let world = GmWorld::new(&guilds);
        execute(sess, cfg, &world, line)
    }

    /// 批次 5A（卡 `openmir2-gm-batch-5a-guild-impl`）：**6 条全部 perm 10**
    /// （`GameCommands.cs:371-373/526` 的 `PerMissionMin = 10` 覆盖 `[Command]` 属性的 0）。
    /// 4 条真 no-op（Announcement / 允许联盟 / GuildWar / SbkDoor）+ 联盟 / 取消联盟 的非会长路径。
    #[test]
    fn batch5a_noops_permission_gate_and_zero_frames() {
        // 6 条都是 perm 10：权限不足 ⇒ 恰好一条红字，且零帧
        for cmd in [
            "@Announcement",
            "@联盟",
            "@允许联盟",
            "@取消联盟",
            "@GuildWar",
            "@SbkDoor",
        ] {
            let (r, _) = run(cmd);
            assert_eq!(texts(&r), vec![(GmColor::Red, PERMISSION_TOO_LOW)], "{cmd}");
            assert!(r.frames.is_empty(), "{cmd}");
        }
        // 权限满足：4 条真 no-op + 联盟/取消联盟 的非会长路径 ⇒ 零帧零消息
        for cmd in [
            "@Announcement",
            "@允许联盟",
            "@GuildWar",
            "@SbkDoor",
            "@联盟",
            "@取消联盟",
            "@取消联盟 某会",
        ] {
            let (r, _) = run_gm(cmd);
            assert!(r.found && r.parsed, "{cmd}");
            assert!(
                r.msgs.is_empty() && r.frames.is_empty(),
                "{cmd} 应零帧零消息"
            );
            assert!(r.log_note.is_some(), "{cmd} 必须留 oracle 出处 note");
        }
    }

    /// 表项级：批次 5A 的 6 条必须在 `COMMANDS` 里且 **imp 不是 `Unimplemented`**、
    /// **perm 10**（生效口径以 `GameCommands.cs` 构造 + `Command.conf` 为准）。
    /// 牙：任一条回退 `Unimplemented` ⇒ 本用例红。
    #[test]
    fn batch5a_table_entries_are_wired() {
        for name in [
            "Announcement",
            "联盟",
            "允许联盟",
            "取消联盟",
            "GuildWar",
            "SbkDoor",
        ] {
            let def = find(name).unwrap_or_else(|| panic!("{name} 不在 COMMANDS 表"));
            assert_ne!(
                def.imp,
                Imp::Unimplemented,
                "{name} 仍是 Unimplemented（应为批次 5A 的接线实现）"
            );
            assert_eq!(def.perm_min, 10, "{name} 的 PermissionMin");
        }
    }

    /// `联盟` / `取消联盟` 的**会长支**：oracle 会进 `ClientGuildAlly`/`ClientGuildBreakAlly`
    /// （帧 + `SS_207`），宿主未建模 ⇒ 只留 note（whitelist T-263），不许静默当等价。
    #[test]
    fn batch5a_guild_master_path_is_registered_not_silent() {
        let mut gm = mir2_world::GuildManager::new();
        assert!(gm.add_guild("测试会", "boss", &mir2_world::GuildConfig::default()));
        let world = GmWorld {
            guilds: &gm,
            maps: None,
            world: None,
            speaker: "boss",
            speaker_id: 7,
            now_ms: 0,
            filter_word: None,
        };
        assert!(world.is_guild_master("boss"), "chief 必须是会长（rank 1）");
        let mut sess = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        for (cmd, needle) in [
            ("@联盟", "ClientGuildAlly"),
            ("@取消联盟 测试会", "ClientGuildBreakAlly"),
        ] {
            let r = execute(&mut sess, &GmConfig::default(), &world, cmd);
            assert!(
                r.msgs.is_empty() && r.frames.is_empty(),
                "{cmd} 会长支只留 note"
            );
            let note = r.log_note.unwrap_or("");
            assert!(
                note.contains(needle) && note.contains("T-263"),
                "{cmd} note={note}"
            );
        }
    }

    fn texts(r: &GmResult) -> Vec<(GmColor, &str)> {
        r.msgs.iter().map(|m| (m.color, m.text.as_str())).collect()
    }

    /// `ExtractCommandAndParameters`（`CommandSystem.cs:228-250`）的 6 条推论逐条钉住。
    #[test]
    fn parse_command_matches_extract_command_and_parameters() {
        assert_eq!(parse_command("@时间"), Some(("时间", "")));
        assert_eq!(parse_command("  @时间  "), Some(("时间", "")));
        // `@` 后紧跟空格 ⇒ 命令名为空串（未知命令）。
        assert_eq!(parse_command("@ 时间"), Some(("", "时间")));
        // 只有 ASCII 空格分隔：制表符留在命令名里。
        assert_eq!(parse_command("@时间\t1"), Some(("时间\t1", "")));
        // 参数取首个空格后整段再 Trim；内部连续空格保留。
        assert_eq!(parse_command("@时间  1"), Some(("时间", "1")));
        assert_eq!(parse_command("@Make 木剑  1"), Some(("Make", "木剑  1")));
        assert_eq!(parse_command("@"), Some(("", "")));
        assert_eq!(parse_command("@时间 "), Some(("时间", "")));
        // `:`/`,` 不是分隔符。
        assert_eq!(parse_command("@天地合一:1"), Some(("天地合一:1", "")));
        assert_eq!(parse_command("@a,b:c"), Some(("a,b:c", "")));
        assert_eq!(parse_command("not a command"), None);
        assert_eq!(parse_command("   "), None);
    }

    /// 表完整性：185 行、名字唯一、现网 perm 0 恰好 5 条（设计件 §2.4 / §7.1 #1）。
    #[test]
    fn command_table_is_the_185_class_oracle_set() {
        assert_eq!(COMMANDS.len(), 185);
        let mut seen: Vec<&str> = Vec::new();
        for def in COMMANDS {
            assert!(
                !seen.iter().any(|n| n.eq_ignore_ascii_case(def.name)),
                "重复命令名: {}",
                def.name
            );
            seen.push(def.name);
        }
        let mut perm0: Vec<&str> = COMMANDS
            .iter()
            .filter(|d| d.perm_min == 0)
            .map(|d| d.name)
            .collect();
        perm0.sort_unstable();
        assert_eq!(
            perm0,
            vec!["AddRobotPlay", "LockLogin", "拒绝", "拒绝私聊", "时间"]
        );
        // 其余 180 条全是 perm 10（现网 `Permission` 恒 0 ⇒ 一律 `权限不够!!!`）。
        assert_eq!(COMMANDS.iter().filter(|d| d.perm_min == 10).count(), 180);
    }

    /// `@sadasd`/`@df56dfs` 等混淆名是配置覆盖的结果，英文出厂名在现网**不**命中。
    #[test]
    fn config_overridden_names_win_and_factory_names_do_not() {
        let sadasd = find("sadasd").expect("@sadasd = AllowGuildRecallCommand 的现网名");
        assert_eq!(sadasd.imp, Imp::AllowGuildRecall);
        assert_eq!(sadasd.perm_min, 10);
        assert!(
            find("AllowGuildRecall").is_none(),
            "出厂名 Allowguildrecall 不注册"
        );
        assert_eq!(find("df56dfs").unwrap().perm_min, 10);
        assert!(find("GuildRecall").is_none());
        // 大小写不敏感（`OrdinalIgnoreCase`）。
        assert_eq!(find("LOCKLOGIN").unwrap().name, "LockLogin");
        assert_eq!(find("locklogin").unwrap().perm_min, 0);
    }

    /// `@help`：空参回 usage，否则 `Unknown command: {group} {command}`；**不受权限门约束**。
    #[test]
    fn help_usage_and_unknown_command_text() {
        let (r, _) = run("@help");
        assert_eq!(texts(&r), vec![(GmColor::Red, "usage: help <command>")]);
        assert!(r.found);
        let (r, _) = run("@help foo bar");
        assert_eq!(texts(&r), vec![(GmColor::Red, "Unknown command: foo bar")]);
        // 只取前两段，第三段丢弃。
        let (r, _) = run("@help foo bar baz");
        assert_eq!(texts(&r), vec![(GmColor::Red, "Unknown command: foo bar")]);
        // 只有一段 ⇒ 后半为空串（行尾保留空格）。
        let (r, _) = run("@help foo");
        assert_eq!(texts(&r), vec![(GmColor::Red, "Unknown command: foo ")]);
        // 内建 `@help` 不在 185 表里。
        assert!(find("help").is_none());
    }

    /// 权限门：180 条非 0 权限命令在 perm 0 一律 `权限不够!!!`（Red）。
    #[test]
    fn permission_gate_blocks_non_zero_commands() {
        for line in ["@交易", "@天地合一", "@sadasd", "@df56dfs", "@加入门派"] {
            let (r, _) = run(line);
            assert!(r.found, "{line} 应命中命令表（走回落就说明名字错了）");
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, "权限不够!!!")],
                "{line} 应只回一条权限不足"
            );
        }
    }

    /// `@时间`：Blue + `当前日期时间: yyyy-MM-dd HH:mm:ss`。
    #[test]
    fn date_is_blue_with_formatted_local_time() {
        let (r, _) = run("@时间");
        assert_eq!(r.msgs.len(), 1);
        assert_eq!(r.msgs[0].color, GmColor::Blue);
        let text = &r.msgs[0].text;
        let stamp = text.strip_prefix("当前日期时间: ").expect("前缀固定");
        assert_eq!(stamp.len(), 19, "yyyy-MM-dd HH:mm:ss");
        let b = stamp.as_bytes();
        assert_eq!(&b[4..5], b"-");
        assert_eq!(&b[7..8], b"-");
        assert_eq!(&b[10..11], b" ");
        assert_eq!(&b[13..14], b":");
        assert_eq!(&b[16..17], b":");
    }

    /// `@拒绝`：只加不删（重复调用不查重）；单字符名字触 oracle 越界异常 ⇒ 零帧。
    #[test]
    fn prv_msg_appends_only_and_single_char_name_throws() {
        let (r, sess) = run("@拒绝 zhangsan");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Green, "zhangsan 已被加入禁止私聊列表...")]
        );
        assert_eq!(sess.lock_whisper_list, vec!["zhangsan".to_string()]);
        let (r, sess) = run("@拒绝 zhangsan");
        assert_eq!(sess.lock_whisper_list, vec!["zhangsan".to_string()]);
        assert_eq!(r.msgs.len(), 1);
        // 无参 ⇒ 空名字 ⇒ 帮助（Red）。
        let (r, _) = run("@拒绝");
        assert_eq!(texts(&r), vec![(GmColor::Red, "命令格式: @拒绝 人物名称")]);
        // 第二字符 `?` ⇒ 帮助。
        let (r, sess) = run("@拒绝 a?");
        assert_eq!(texts(&r), vec![(GmColor::Red, "命令格式: @拒绝 人物名称")]);
        assert!(sess.lock_whisper_list.is_empty());
        // 单字符 ⇒ oracle `sHumanName[1]` 越界 ⇒ found 但零帧。
        let (r, sess) = run("@拒绝 a");
        assert!(r.found);
        assert!(r.msgs.is_empty());
        assert!(sess.lock_whisper_list.is_empty());
        assert!(r.log_note.is_some());
    }

    /// `@拒绝私聊`：翻转 `HearWhisper`（ctor = true），Green。
    #[test]
    fn allow_msg_toggles_hear_whisper() {
        let mut sess = GmSession::default();
        assert!(sess.hear_whisper);
        let r = exec(&mut sess, &GmConfig::default(), "@拒绝私聊");
        assert_eq!(texts(&r), vec![(GmColor::Green, "[禁止私聊]")]);
        assert!(!sess.hear_whisper);
        let r = exec(&mut sess, &GmConfig::default(), "@拒绝私聊");
        assert_eq!(texts(&r), vec![(GmColor::Green, "[允许私聊]")]);
        assert!(sess.hear_whisper);
    }

    /// `@LockLogin`：部署 `PasswordLockHumanLogin=0` ⇒ 恒回红字，`IsLockLogon` 不变。
    #[test]
    fn lock_login_reads_config_gate() {
        let mut sess = GmSession::default();
        let r = exec(
            &mut sess,
            &GmConfig {
                lock_human_login: false,
                ..GmConfig::default()
            },
            "@LockLogin",
        );
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "本服务器还没有启用登录锁功能!!!")]
        );
        assert!(sess.is_lock_logon, "配置未开 ⇒ 不翻转");
        // 配置开了但 ctor `IsLockLogon=true`/`IsLockLogoned=false` ⇒ 第二道红字。
        let r = exec(
            &mut sess,
            &GmConfig {
                lock_human_login: true,
                ..GmConfig::default()
            },
            "@LockLogin",
        );
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "您还没有打开登录锁或还没有设置锁密码!!!")]
        );
        assert!(sess.is_lock_logon);
        // 已设置锁密码 ⇒ 才翻转。
        sess.is_lock_logoned = true;
        let r = exec(
            &mut sess,
            &GmConfig {
                lock_human_login: true,
                ..GmConfig::default()
            },
            "@LockLogin",
        );
        assert_eq!(texts(&r), vec![(GmColor::Green, "已关闭登录锁")]);
        assert!(!sess.is_lock_logon);
    }

    /// `@AddRobotPlay`：`?` 出帮助；其余参数零帧零副作用。
    #[test]
    fn add_robot_play_is_a_noop_except_question_mark() {
        let (r, _) = run("@AddRobotPlay ?");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: @AddRobotPlay 数量 地图 X Y")]
        );
        for line in [
            "@AddRobotPlay",
            "@AddRobotPlay 5",
            "@AddRobotPlay 5 0 100 100",
        ] {
            let (r, _) = run(line);
            assert!(r.found);
            assert!(r.msgs.is_empty(), "{line} 应无回帧");
        }
    }

    /// 未知命令：先一条 `未知命令: {command} {parameters}`（`found=false`），再由回落补第二条。
    #[test]
    fn unknown_command_emits_first_frame_and_fallback_text() {
        let (r, _) = run("@nosuchcmd arg1");
        assert!(!r.found);
        assert_eq!(texts(&r), vec![(GmColor::Red, "未知命令: nosuchcmd arg1")]);
        assert_eq!(
            incorrect_msg(0, "@nosuchcmd arg1"),
            "@此命令不正确，或没有足够的权限!!!"
        );
        // 无参数时正文以空格结尾（`format!` 固定插一个空格）。
        let (r, _) = run("@nosuchcmd");
        assert_eq!(texts(&r), vec![(GmColor::Red, "未知命令: nosuchcmd ")]);
        // `@` 单独 ⇒ 命令名空串。
        let (r, _) = run("@");
        assert_eq!(texts(&r), vec![(GmColor::Red, "未知命令:  ")]);
        // perm >= 2 支（现网不可达，但语义要钉住）：`sCMD` 含前导 `@`。
        assert_eq!(
            incorrect_msg(2, "@nosuchcmd arg1"),
            "@@nosuchcmd此命令不正确，或没有足够的权限!!!"
        );
    }

    /// 内建 `@commands`：oracle 反射异常 ⇒ 零帧、`found=true`（不落未知命令回落）。
    #[test]
    fn commands_builtin_produces_no_frames() {
        let (r, _) = run("@commands");
        assert!(r.found);
        assert!(r.msgs.is_empty());
        assert!(r.log_note.is_some());
        assert!(find("commands").is_none());
    }

    // ==================== 批次 2/3（重载/管理 · 封禁名单 · 桩/测试）====================

    /// `HUtil32.StrToInt` = `int.TryParse` 失败取缺省（`NumberStyles.Integer`）。
    #[test]
    fn str_to_int_matches_tryparse_integer_style() {
        assert_eq!(str_to_int("12", 0), 12);
        assert_eq!(str_to_int(" 7 ", 0), 7);
        assert_eq!(str_to_int("+3", 0), 3);
        assert_eq!(str_to_int("-4", 0), -4);
        // `TryParse` 失败 ⇒ 缺省值（不是 0，缺省由调用方给）。
        assert_eq!(str_to_int("", 99), 99);
        assert_eq!(str_to_int("x", 99), 99);
        assert_eq!(str_to_int("1.5", 99), 99);
        assert_eq!(str_to_int("0x10", 99), 99);
        assert_eq!(str_to_int("1 2", 99), 99);
        // 溢出：`int.TryParse` 失败 ⇒ 缺省（Rust `parse` 同样失败）。
        assert_eq!(str_to_int("2147483648", 99), 99);
    }

    /// 批次 2/3 + 批次 3 + 批次 4 + 批次 5（含卡 `openmir2-gm-impl-showeffect` 的 `ShowEffect`、
    /// 卡 `openmir2-gm-impl-startquest` 的 `StartQuest`、卡 `openmir2-gm-impl-kick` 的 `VV踢人`、
    /// 卡 `openmir2-gm-impl-mobfireburn` 的 `MobFireBurn`、卡 `openmir2-gm-impl-smake` 的 `SmakeItem`、
    /// 卡 `openmir2-gm-impl-clearbag` 的 `ClearBagItem`）
    /// 以及批次 5A（卡 `openmir2-gm-batch-5a-guild-impl` 的 6 条行会族）、
    /// 族 P（卡 `openmir2-gm-family-p-passwordlock` 的 7 条密码锁门）、
    /// 族 T（卡 `openmir2-gm-family-t-toggle-horse` 的 7 条会话开关/骑马）与
    /// 族 Q（卡 `openmir2-gm-family-q-query-display` 的 10 条查询/展示）与
    /// 族 X1（卡 `openmir2-gm-family-x1-denywhisper` 的 5 条禁言名单）
    /// 的命令体接上（`Unimplemented` 从 180 降到 45），且命令名 → 实现标签固定。
    #[test]
    fn batch2_table_wires_thirty_command_bodies() {
        assert_eq!(
            COMMANDS
                .iter()
                .filter(|d| d.imp != Imp::Unimplemented)
                .count(),
            140,
            "140 = G1 的 5 条 + 批次 2/3 30 条 + 批次 3（传送/召唤）18 条 + 批次 4（数值/角色）44 条 + 批次 5（禁言 1 + ShowEffect 1 + StartQuest 1 + VV踢人 1 + MobFireBurn 1 + SmakeItem 1 + ClearBagItem 1 + 取消禁言 1）8 条 + 批次 5A（Announcement/联盟/允许联盟/取消联盟/GuildWar/SbkDoor）6 条 + 族 P（密码锁门 7）+ 族 T（会话开关/骑马 7）+ 族 Q（查询/展示 10）+ 族 X1（禁言名单 5）"
        );
        assert_eq!(
            COMMANDS
                .iter()
                .filter(|d| d.imp == Imp::Unimplemented)
                .count(),
            45,
            "185 - 140"
        );
        let cases: &[(&str, Imp)] = &[
            ("LoadAdmin", Imp::LoadAdmin),
            ("NpcScript", Imp::NpcScript),
            ("ReloadAbuse", Imp::ReloadAbuse),
            ("重读GM", Imp::ReLoadAdmin),
            ("ReloadGuildAll", Imp::ReloadAllGuild),
            ("ReloadItemDB", Imp::ReloadItemDb),
            ("ReloadGuild", Imp::ReloadGuild),
            ("重读公告", Imp::ReloadLineNotice),
            ("ReloadMagicDB", Imp::ReloadMagicDb),
            ("重读登陆", Imp::ReloadManage),
            ("重读爆率", Imp::ReloadMonItems),
            ("重读商人", Imp::ReloadNpc),
            ("重读机器人脚本", Imp::ReloadRobot),
            ("重读机器人配置", Imp::ReloadRobotManage),
            ("-jid", Imp::DelDenyAccount),
            ("DelDenyChrNameLogon", Imp::DelDenyChrName),
            ("-jip", Imp::DelDenyIp),
            ("+jid", Imp::DenyAccount),
            ("DenyChrNameLogon", Imp::DenyChrName),
            ("+jip", Imp::DenyIp),
            ("+-jid", Imp::ShowDenyAccount),
            ("ShowDenyChrNameLogon", Imp::ShowDenyChrName),
            ("+-jip", Imp::ShowDenyIp),
            ("Backstep", Imp::BackStep),
            ("TestFire", Imp::TestFire),
            ("TestGetBagItems", Imp::TestGetBagItems),
            ("TestSpeedMode", Imp::TestSpeedMode),
            ("TestStatus", Imp::TestStatus),
            ("UserCmd", Imp::UserCmd),
            ("FireBurn", Imp::FireBurn),
        ];
        assert_eq!(cases.len(), 30);
        for (name, imp) in cases {
            let def = find(name).unwrap_or_else(|| panic!("{name} 必须在命令表里"));
            assert_eq!(def.imp, *imp, "{name} 的实现标签");
            assert_eq!(def.perm_min, 10, "{name} 现网权限 = 10");
        }
    }

    /// 卡 `openmir2-gm-command-name-typos`：名字必须与 oracle **活名**一致，且**不得**把
    /// conf 死键拼写留作别名（负例是承重的——只有正例的话「加个别名」也能全绿）。
    ///
    /// 四条牙（正负各二）：
    /// ① `@ChangeMasterName` 命中（未接线世界 ⇒ 目标离线红字，**不是**未知命令）；
    /// ② `@MasterName` **未知命令**（改名前会命中）；
    /// ③ `@ShowDenyChrNameLogon` 命中（绿字「禁止登录角色列表为空。」）；
    /// ④ `@ShowDenyCharNameLogon` **未知命令**。
    ///
    /// 变异：任一名字改回死键拼写 ⇒ 对应正/负牙互换而红。
    #[test]
    fn command_names_use_oracle_live_keys_without_dead_key_aliases() {
        // 表级：两个活名在表、两个死键拼写**不在**表。
        let has = |n: &str| COMMANDS.iter().any(|c| c.name.eq_ignore_ascii_case(n));
        assert!(has("ChangeMasterName"), "oracle 活名（类属性默认名）");
        assert!(!has("MasterName"), "死键拼写不得作为别名");
        assert!(
            has("ShowDenyChrNameLogon"),
            "oracle 活名（文件名 Char / 命令名 Chr）"
        );
        assert!(!has("ShowDenyCharNameLogon"), "死键拼写不得作为别名");

        // 派发级：正例命中、负例落未知命令。
        let (r, _) = run_gm("@ChangeMasterName aaa 无");
        assert!(r.found, "@ChangeMasterName 必须被识别");
        assert!(
            !texts(&r).iter().any(|(_, t)| t.contains("未知命令")),
            "{:?}",
            texts(&r)
        );
        assert_eq!(texts(&r)[0].1, "aaa 现在不在线，或在其它服务器上!!!");

        let (r, _) = run_gm("@MasterName aaa 无");
        assert!(!r.found, "@MasterName 必须不可达（无别名）");
        assert!(
            texts(&r).iter().any(|(_, t)| t.contains("未知命令")),
            "{:?}",
            texts(&r)
        );

        let (r, _) = run_gm("@ShowDenyChrNameLogon");
        assert!(r.found);
        // 真机零帧（扫盲卡）：Count 读 SystemShare.DenyChrNameList（声明即 null）⇒ NRE ⇒
        // 绿字不可达；note 登记（无 perm<6 内门，体首即触表）。
        assert!(r.msgs.is_empty() && r.frames.is_empty());
        assert!(r.log_note.expect("必须登记").contains("声明即 null"));

        let (r, _) = run_gm("@ShowDenyCharNameLogon");
        assert!(!r.found);
        assert!(
            texts(&r).iter().any(|(_, t)| t.contains("未知命令")),
            "{:?}",
            texts(&r)
        );
    }

    /// 封禁名单族 9 条：可达支文本逐字节 + 触表六命令零帧（扫盲卡
    /// `openmir2-gm-systemshare-null-sweep`，review F1 同款运行层真相——Del 三条的 for-init
    /// 与 Show 三条的 Count 读 SystemShare.Deny*List 声明即 null（SystemShare.cs:135/:139/:143）
    /// ⇒ NRE 被 Chat.cs:402 顶层 catch 吞 ⇒ 零帧；+jid 三条只调 Save 方法（全注释体）⇒
    /// 绿字可达不变；`+jip` 4 参 `@params[3]` 越界零帧不变，见下一条用例）。
    #[test]
    fn deny_family_texts_match_oracle() {
        use GmColor::*;
        let cases: &[(&str, GmColor, &str)] = &[
            ("@-jid", Red, DEL_DENY_ACCOUNT_HELP),
            ("@DelDenyChrNameLogon", Red, DEL_DENY_CHRNAME_HELP),
            ("@-jip", Red, DEL_DENY_IP_HELP),
            ("@+jid", Red, DENY_ACCOUNT_HELP),
            ("@+jid acct 1", Green, "acct已加入禁止登录帐号列表"),
            ("@+jid acct 0", Green, "acct已加入临时禁止登录帐号列表"),
            ("@+jid acct", Green, "acct已加入临时禁止登录帐号列表"),
            ("@DenyChrNameLogon", Red, DENY_CHRNAME_HELP),
            ("@DenyChrNameLogon 张三 1", Green, "张三已加入禁止人物列表"),
            (
                "@DenyChrNameLogon 张三 0",
                Green,
                "张三已加入临时禁止人物列表",
            ),
            ("@+jip", Red, DENY_IP_HELP),
            ("@+jip 1.2.3.4", Green, "1.2.3.4已加入临时禁止登录IP列表"),
            // 4 参才轮到 `@params[3]`（2/3 参抛越界，见下一条用例）。
            ("@+jip 1.2.3.4 a b 1", Green, "1.2.3.4已加入禁止登录IP列表"),
            (
                "@+jip 1.2.3.4 a b 0",
                Green,
                "1.2.3.4已加入临时禁止登录IP列表",
            ),
        ];
        for (line, color, text) in cases {
            let (r, _) = run_gm(line);
            assert_eq!(texts(&r), vec![(*color, *text)], "{line}");
            assert!(r.found && r.parsed, "{line}");
        }
        // 触表六命令：真机 NRE ⇒ 零帧 + log_note（SYSTEMSHARE_NULL_TABLE）。
        for line in [
            "@-jid acct",
            "@DelDenyChrNameLogon 张三",
            "@-jip 1.2.3.4",
            "@+-jid",
            "@ShowDenyChrNameLogon",
            "@+-jip",
        ] {
            let (r, _) = run_gm(line);
            assert!(r.found && r.parsed, "{line} 命中命令体");
            assert!(
                r.msgs.is_empty() && r.frames.is_empty(),
                "{line} 触表应零帧"
            );
            let note = r.log_note.expect("{line} 必须登记 NRE 零帧");
            assert!(
                note.contains("声明即 null") && note.contains("零帧"),
                "{note}"
            );
        }
    }

    /// `@+jip` 的越界怪癖：2/3 个参数 ⇒ `@params[3]` 抛 `IndexOutOfRangeException`（catch 吞）⇒ 零帧。
    #[test]
    fn deny_ip_two_or_three_params_throw_like_oracle() {
        for line in ["@+jip 1.2.3.4 x", "@+jip 1.2.3.4 x y"] {
            let (r, _) = run_gm(line);
            assert!(r.found, "{line}");
            assert!(r.msgs.is_empty(), "{line} 应零帧（越界被 catch 吞）");
            assert!(r.log_note.is_some(), "{line} 要留痕");
        }
        // 1 参不越界：走 `""` 支。
        let (r, _) = run_gm("@+jip 1.2.3.4");
        assert_eq!(r.msgs.len(), 1);
        // 0 参：`@params == [""]` ⇒ 帮助（越界发生在判空之前，但 1 参不越界）。
        let (r, _) = run_gm("@+jip");
        assert_eq!(texts(&r), vec![(GmColor::Red, DENY_IP_HELP)]);
    }

    /// 重载/管理 14 条：文本 + 主服门 + 宿主开关分支。
    #[test]
    fn reload_family_texts_and_branches() {
        use GmColor::*;
        let cases: &[(&str, GmColor, &str)] = &[
            // oracle 里 `LoadAdminList`/`SendServerGroupMsg` 均被注释 ⇒ 纯文本。
            ("@LoadAdmin", Green, "管理员列表重新加载成功..."),
            ("@重读GM", Green, "管理员列表重新加载成功..."),
            ("@ReloadGuildAll", Red, "重新加载行会信息完成."),
            ("@ReloadItemDB", Green, "物品数据库重新加载完成。"),
            ("@ReloadMagicDB", Green, "魔法数据库重新加载完成。"),
            ("@ReloadGuild", Red, RELOAD_GUILD_HELP),
            ("@ReloadGuild 血盟", Red, "未找到行会血盟!!!"),
            ("@重读公告", Green, "重新加载公告设置信息完成。"),
            ("@重读爆率", Green, "怪物爆物品列表重加载完成..."),
            ("@重读机器人脚本", Green, "重新加载机器人配置完成..."),
            ("@重读机器人配置", Green, "重新加载机器人专用脚本完成..."),
        ];
        for (line, color, text) in cases {
            let (r, _) = run_gm(line);
            assert_eq!(texts(&r), vec![(*color, *text)], "{line}");
        }
        // 零帧两条：`@ReloadAbuse` 空体、`@NpcScript` 整段注释（`File.Exists("")` 恒 false）。
        for line in ["@ReloadAbuse", "@NpcScript", "@重读商人"] {
            let (r, _) = run_gm(line);
            assert!(r.found && r.msgs.is_empty(), "{line} 应零帧");
        }
        // `@重读登陆`：两条 Green，顺序固定（登录脚本 → 功能脚本）。
        let (r, _) = run_gm("@重读登陆");
        assert_eq!(
            texts(&r),
            vec![
                (Green, "重新加载登录脚本完成..."),
                (Green, "重新加载功能脚本完成..."),
            ]
        );
        // 失败支也是 Green（oracle 怪癖）。
        let cfg = GmConfig {
            manage_npc_present: false,
            function_npc_present: false,
            ..GmConfig::default()
        };
        let (r, _) = run_gm_with(&cfg, "@重读登陆");
        assert_eq!(
            texts(&r),
            vec![
                (Green, "重新加载登录脚本失败..."),
                (Green, "重新加载功能脚本失败..."),
            ]
        );
        let cfg = GmConfig {
            robot_npc_present: false,
            ..GmConfig::default()
        };
        let (r, _) = run_gm_with(&cfg, "@重读机器人配置");
        assert_eq!(texts(&r), vec![(Green, "重新加载机器人专用脚本失败...")]);
        let cfg = GmConfig {
            notice_file_exists: false,
            ..GmConfig::default()
        };
        let (r, _) = run_gm_with(&cfg, "@重读公告");
        assert_eq!(texts(&r), vec![(Red, "重新加载公告设置信息失败!!!")]);
        // 主服门（部署 `ServerIndex = 0`，测试里构造 1 覆盖拒绝支）。
        let cfg = GmConfig {
            server_index: 1,
            ..GmConfig::default()
        };
        for line in ["@ReloadGuildAll", "@ReloadGuild 血盟"] {
            let (r, _) = run_gm_with(&cfg, line);
            assert_eq!(
                texts(&r),
                vec![(Red, "此命令只能在主游戏服务器上执行!!!")],
                "{line}"
            );
        }
    }

    /// `@ReloadGuild` 命中支：`FindGuild` 精确匹配 ⇒ `行会{name}重加载成功...`（Red）。
    #[test]
    fn reload_guild_found_uses_exact_name_match() {
        let mut gm = mir2_world::GuildManager::new();
        gm.guilds.push(mir2_world::GuildState::new("血盟"));
        let world = GmWorld::new(&gm);
        let mut sess = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        let r = execute(&mut sess, &GmConfig::default(), &world, "@ReloadGuild 血盟");
        assert_eq!(texts(&r), vec![(GmColor::Red, "行会血盟重加载成功...")]);
        assert!(r.log_note.is_some(), "LoadGuild 宿主未接要留痕");
        // 大小写不同 = 不同会名（oracle `GuildName ==` 精确），但这里中文无大小写；
        // 用 ASCII 会名钉住「不是 OrdinalIgnoreCase」。
        let mut gm = mir2_world::GuildManager::new();
        gm.guilds.push(mir2_world::GuildState::new("Blood"));
        let world = GmWorld::new(&gm);
        let r = execute(
            &mut sess,
            &GmConfig::default(),
            &world,
            "@ReloadGuild blood",
        );
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "未找到行会blood!!!")],
            "FindGuild 是 `==` 精确匹配，不是 OrdinalIgnoreCase"
        );
    }

    /// 桩/测试 7 条：文本、翻转、越界零帧、字面量 `%d` 怪癖。
    #[test]
    fn stub_family_texts_match_oracle() {
        use GmColor::*;
        // `TestGetBagItems`：`string.Format` 无 `{n}` 占位符 ⇒ 原文照发。
        let (r, _) = run_gm("@TestGetBagItems");
        assert_eq!(texts(&r), vec![(Blue, "DC:%d SC:%d MC:%d DURA:%d")]);
        // `TestSpeedMode`：翻转 + Red。
        let (r, sess) = run_gm("@TestSpeedMode");
        assert_eq!(texts(&r), vec![(Red, "开启速度测试模式")]);
        assert!(sess.test_speed_mode);
        let mut sess = sess;
        let r = exec(&mut sess, &GmConfig::default(), "@TestSpeedMode");
        assert_eq!(texts(&r), vec![(Red, "关闭速度测试模式")]);
        assert!(!sess.test_speed_mode);
        // `TestStatus`：合法下标回文本；越界（含负数）抛异常被吞 ⇒ 零帧。
        let (r, _) = run_gm("@TestStatus 3 60");
        assert_eq!(texts(&r), vec![(Green, "状态编号:3 时间长度: 60 秒")]);
        let (r, _) = run_gm("@TestStatus 14 -5");
        assert_eq!(texts(&r), vec![(Green, "状态编号:14 时间长度: -5 秒")]);
        for line in ["@TestStatus 15 1", "@TestStatus -1 1", "@TestStatus 99 1"] {
            let (r, _) = run_gm(line);
            assert!(r.found && r.msgs.is_empty(), "{line} 应零帧（数组越界）");
            assert!(r.log_note.is_some());
        }
        // 零帧：TestFire / UserCmd / Backstep / FireBurn（有参数）。
        for line in [
            "@TestFire 3 1 5 10",
            "@UserCmd anylabel",
            "@Backstep 3 5",
            "@Backstep",
            "@FireBurn 1 1 1",
        ] {
            let (r, _) = run_gm(line);
            assert!(r.found && r.msgs.is_empty(), "{line} 应零帧");
        }
        // `FireBurn` 参数有 0 ⇒ 红字帮助，且 Help 为 null ⇒ 正文带尾随空格。
        let (r, _) = run_gm("@FireBurn 1 0 1");
        assert_eq!(texts(&r), vec![(Red, "命令格式: @FireBurn ")]);
        assert!(FIRE_BURN_HELP.ends_with(' '));
    }

    // ==================== 批次 3：传送/召唤族（17 + `@传送`）====================

    /// 小地图夹具：20×20 全平地 `.map`（逻辑名 `map`，`map_id = 0`）。
    ///
    /// 与 `castle/tests.rs` 的夹具同口径：写临时 `.map` 头 + 全 0 格，再 `MapRegistry::from_entries`。
    struct MapFixture {
        base: std::path::PathBuf,
        registry: mir2_envir::MapRegistry,
    }

    impl Drop for MapFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    static MAP_FIXTURE_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    fn map_fixture(flags: mir2_envir::MapInfoFlag) -> MapFixture {
        let base = std::env::temp_dir().join(format!(
            "gs-gmcmd-{}-{}",
            std::process::id(),
            MAP_FIXTURE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let (w, h) = (20_i16, 20_i16);
        let mut bytes = vec![0u8; mir2_envir::HEADER_BYTES];
        bytes[0..2].copy_from_slice(&w.to_le_bytes());
        bytes[2..4].copy_from_slice(&h.to_le_bytes());
        bytes.resize(
            mir2_envir::HEADER_BYTES
                + usize::try_from(w).unwrap()
                    * usize::try_from(h).unwrap()
                    * mir2_envir::CELL_BYTES,
            0,
        );
        std::fs::create_dir_all(base.join("Map")).unwrap();
        std::fs::write(base.join("Map").join("0.map"), &bytes).unwrap();
        let loader = mir2_envir::MapLoader::new(base.clone(), "Map", "Envir");
        let registry = mir2_envir::MapRegistry::from_entries(
            loader,
            vec![mir2_envir::MapInfoEntry {
                map_name: "map".to_string(),
                map_file_name: "0".to_string(),
                desc: "测试地图".to_string(),
                server_index: 0,
                flag: flags,
            }],
        );
        assert_eq!(registry.id_of("map"), Some(0));
        MapFixture { base, registry }
    }

    /// 20×20 单图世界 + `gm`(5,5) / `bob`(6,6) 两个玩家。
    fn gm_world() -> mir2_world::World {
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        w.add_entity(mir2_world::Entity::new(0, "gm", 5, 5));
        w.add_entity(mir2_world::Entity::new(0, "bob", 6, 6));
        w
    }

    fn gm_session() -> GmSession {
        GmSession {
            permission: 10,
            ..GmSession::default()
        }
    }

    /// 批次 3 的 18 条全部接线：表内 `imp` 不是 `Unimplemented`，且 perm 0 一律 `权限不够!!!`。
    #[test]
    fn teleport_family_table_wires_eighteen_bodies() {
        let expected = [
            ("dfsgf567", Imp::AllowDearRecall),
            ("sadasd", Imp::AllowGuildRecall),
            ("fdgh56j", Imp::AllowMasterRecall),
            ("dsfsdt546", Imp::DearRecall),
            ("天地合一", Imp::GroupRecall),
            ("df56dfs", Imp::GuildRecall),
            ("f", Imp::MapMove),
            ("集体传送", Imp::MapMoveHuman),
            ("jkt76", Imp::MasterRecall),
            ("MoveMobTo", Imp::MoveMobTo),
            ("ff", Imp::PositionMove),
            ("RecallHuman", Imp::RecallHuman),
            ("bb", Imp::RecallMob),
            ("ReGotoHuman", Imp::ReGotoHuman),
            ("SignMapMove", Imp::SignMapMove),
            ("SuperTingwa", Imp::SuperTing),
            ("Tingwa", Imp::Ting),
            ("传送", Imp::UserMoveXy),
        ];
        for (name, imp) in expected {
            let def = find(name).unwrap_or_else(|| panic!("{name} 必须在 185 表内"));
            assert_eq!(def.perm_min, 10, "{name}");
            assert_eq!(def.imp, imp, "{name}");
            let (r, _) = run(&format!("@{name}"));
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, "权限不够!!!")],
                "{name} 在 perm 0 必须被权限门挡住"
            );
        }
    }

    /// `@dfsgf567`/`@sadasd`/`@fdgh56j`：翻会话开关，颜色与文本照 oracle。
    #[test]
    fn allow_recall_commands_toggle_session_flags() {
        let guilds = mir2_world::GuildManager::new();
        let view = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        assert!(!sess.can_dear_recall);
        let r = execute(&mut sess, &cfg, &view, "@dfsgf567");
        assert_eq!(texts(&r), vec![(GmColor::Blue, "允许夫妻传送!!!")]);
        assert!(sess.can_dear_recall);
        let r = execute(&mut sess, &cfg, &view, "@dfsgf567");
        assert_eq!(texts(&r), vec![(GmColor::Blue, "禁止夫妻传送!!!")]);
        assert!(!sess.can_dear_recall);

        assert!(!sess.allow_guild_re_call);
        let r = execute(&mut sess, &cfg, &view, "@sadasd");
        assert_eq!(texts(&r), vec![(GmColor::Green, "[允许行会合一]")]);
        let r = execute(&mut sess, &cfg, &view, "@sadasd");
        assert_eq!(texts(&r), vec![(GmColor::Green, "[禁止行会合一]")]);

        assert!(!sess.can_master_recall);
        let r = execute(&mut sess, &cfg, &view, "@fdgh56j");
        assert_eq!(texts(&r), vec![(GmColor::Blue, "允许师徒传送!!!")]);
        let r = execute(&mut sess, &cfg, &view, "@fdgh56j");
        assert_eq!(texts(&r), vec![(GmColor::Blue, "禁止师徒传送!!!")]);
    }

    /// `@dsfsdt546`：未婚 → 地图禁 → 配偶不在线（按 `Gender` 分文案）三道门照 oracle。
    #[test]
    fn dear_recall_matches_oracle_branches() {
        let f = map_fixture(mir2_envir::MapInfoFlag::default());
        let f_no = map_fixture(mir2_envir::MapInfoFlag {
            bo_nodearrecall: true,
            ..mir2_envir::MapInfoFlag::default()
        });
        let w = gm_world();
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // 未婚（`string.IsNullOrEmpty(DearName)`）⇒ 第一条文案。
        let view = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm");
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@dsfsdt546")),
            vec![(GmColor::Red, "你没有结婚!!!")]
        );
        // 已婚 + 地图 `NODEARRECALL`。
        sess.dear_name = "spouse".to_string();
        let view = GmWorld::new(&guilds)
            .with_maps(&f_no.registry)
            .with_world(&w)
            .with_speaker("gm");
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@dsfsdt546")),
            vec![(GmColor::Red, "本地图禁止夫妻传送!!!")]
        );
        // `DearHuman == null`：男 ⇒ 老婆、女 ⇒ 老公（部署 `Entity.dear_human` 无写点）。
        let view = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm");
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@dsfsdt546")),
            vec![(GmColor::Red, "你的老婆不在线!!!")]
        );
        sess.gender = 1;
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@dsfsdt546")),
            vec![(GmColor::Red, "你的老公不在线!!!")]
        );
        // 未注入地图/世界 ⇒ 留痕零帧（不伪造「不在线」）。
        let mut sess2 = gm_session();
        sess2.dear_name = "spouse".to_string();
        let bare = GmWorld::new(&guilds).with_speaker("gm");
        let r = execute(&mut sess2, &cfg, &bare, "@dsfsdt546");
        assert!(r.msgs.is_empty());
        assert!(r.log_note.is_some());
    }

    /// `@ff` / `@f` / `@集体传送`：帮助、地图不存在、可走/不可走、成功零帧。
    #[test]
    fn map_and_position_move_texts() {
        let f = map_fixture(mir2_envir::MapInfoFlag::default());
        let w = gm_world();
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm");
        // PositionMove：帮助（缺参/`?` 首字符）。
        for line in ["@ff", "@ff map", "@ff map 1", "@ff ? 1 1"] {
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, line)),
                vec![(GmColor::Red, POSITION_MOVE_HELP)],
                "{line}"
            );
        }
        // `FindMap == null` ⇒ 零帧（oracle 什么也不做）。
        let r = execute(&mut sess, &cfg, &view, "@ff nope 1 1");
        assert!(r.msgs.is_empty() && r.log_note.is_none(), "{r:?}");
        // 可走 ⇒ SpaceMove 未接 ⇒ 零帧 + 留痕。
        let r = execute(&mut sess, &cfg, &view, "@ff map 5 5");
        assert!(r.msgs.is_empty());
        assert!(r.log_note.is_some());
        // 越界 = 不可走 ⇒ Green（正文用**原字符串** X/Y）。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@ff map 25 25")),
            vec![(GmColor::Green, "无法移动到地图: map X:25 Y:25")]
        );
        // MapMove。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@f")),
            vec![(GmColor::Red, MAP_MOVE_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@f nope")),
            vec![(GmColor::Red, "nope 此地图号不存在!!!")]
        );
        let r = execute(&mut sess, &cfg, &view, "@f map");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
        // MapMoveHuman：帮助（两个空格）、两张图都要存在。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@集体传送")),
            vec![(GmColor::Red, MAP_MOVE_HUMAN_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@集体传送 map nope")),
            vec![(GmColor::Red, "地图nope不存在!!!")]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@集体传送 nope map")),
            vec![(GmColor::Red, "地图nope不存在!!!")]
        );
        let r = execute(&mut sess, &cfg, &view, "@集体传送 map map");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
        // 未注入注册表 ⇒ 零帧留痕（不伪造「地图不存在」）。
        let bare = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
        let r = execute(&mut sess, &cfg, &bare, "@f map");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
    }

    /// `@ReGotoHuman` / `@Tingwa` / `@SuperTingwa` / `@RecallHuman`：在线判定、离线文案、`[1]` 越界怪癖。
    #[test]
    fn ting_family_online_checks_and_quirks() {
        let w = gm_world();
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let f = map_fixture(mir2_envir::MapInfoFlag::default());
        let view = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm");
        for line in [
            "@ReGotoHuman bob",
            "@Tingwa bob",
            "@SuperTingwa bob 5",
            "@RecallHuman bob",
        ] {
            let r = execute(&mut sess, &cfg, &view, line);
            assert!(r.msgs.is_empty(), "{line} 在线支应零帧");
            assert!(r.log_note.is_some(), "{line} 必须留痕");
        }
        for (line, text) in [
            (
                "@ReGotoHuman alice",
                "alice 现在不在线，或在其它服务器上!!!",
            ),
            ("@Tingwa alice", "alice 现在不在线，或在其它服务器上!!!"),
            (
                "@SuperTingwa alice 5",
                "alice 现在不在线，或在其它服务器上!!!",
            ),
        ] {
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, line)),
                vec![(GmColor::Red, text)],
                "{line}"
            );
        }
        // 帮助与 `?`。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@ReGotoHuman")),
            vec![(GmColor::Red, REGOTO_HUMAN_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@ReGotoHuman a?")),
            vec![(GmColor::Red, REGOTO_HUMAN_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@Tingwa")),
            vec![(GmColor::Red, TING_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@SuperTingwa bob")),
            vec![(GmColor::Red, SUPER_TING_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@RecallHuman")),
            vec![(GmColor::Red, RECALL_HUMAN_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@RecallHuman a?")),
            vec![(GmColor::Red, RECALL_HUMAN_HELP)]
        );
        // 单字符名字：`sHumanName[1]` 越界 ⇒ 零帧（四条命令都有此怪癖）。
        for line in [
            "@RecallHuman a",
            "@ReGotoHuman a",
            "@Tingwa a",
            "@SuperTingwa a 5",
        ] {
            let r = execute(&mut sess, &cfg, &view, line);
            assert!(r.msgs.is_empty(), "{line} 单字符必须零帧");
            assert!(
                r.log_note.is_some_and(|n| n.contains("越界")),
                "{line}: {:?}",
                r.log_note
            );
        }
    }

    /// `@jkt76`：师父门、徒弟列表空、10s 冷却、末端对方字段残余。
    #[test]
    fn master_recall_branches() {
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_now(20000);
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@jkt76")),
            vec![(GmColor::Red, "只能师父才能使用此功能!!!")]
        );
        sess.is_master = true;
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@jkt76")),
            vec![(GmColor::Red, "你的徒弟一个都不在线!!!")]
        );
        sess.master_list = vec!["pupil".to_string()];
        sess.master_recall_tick = 15000;
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@jkt76")),
            vec![(GmColor::Red, "稍等一会才能再次使用此功能!!!")]
        );
        sess.master_recall_tick = 0;
        let r = execute(&mut sess, &cfg, &view, "@jkt76");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
        assert_eq!(sess.master_recall_tick, 0, "oracle 只读不写");
    }

    /// `@bb` / `@MoveMobTo` / `@SignMapMove`：帮助、参数越界、oracle 自身 no-op。
    #[test]
    fn recall_mob_move_mob_to_and_sign_map_move_frames() {
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds);
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@bb")),
            vec![(GmColor::Red, RECALL_MOB_HELP)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@bb ?")),
            vec![(GmColor::Red, RECALL_MOB_HELP)]
        );
        let r = execute(&mut sess, &cfg, &view, "@bb 鸡 1");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
        // MoveMobTo：不足 7 段 ⇒ oracle 下标越界 ⇒ 零帧。
        for line in ["@MoveMobTo", "@MoveMobTo 鸡", "@MoveMobTo 鸡 map 1 1 map 2"] {
            let r = execute(&mut sess, &cfg, &view, line);
            assert!(r.msgs.is_empty(), "{line} 应零帧");
            assert!(
                r.log_note.is_some_and(|n| n.contains("越界")),
                "{line}: {:?}",
                r.log_note
            );
        }
        assert_eq!(
            texts(&execute(
                &mut sess,
                &cfg,
                &view,
                "@MoveMobTo ? map 1 1 map 2 2"
            )),
            vec![(GmColor::Red, MOVE_MOB_TO_HELP)]
        );
        let r = execute(&mut sess, &cfg, &view, "@MoveMobTo 鸡 map 1 1 map 2 2");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
        // SignMapMove：oracle 体整段注释 ⇒ no-op。
        let r = execute(&mut sess, &cfg, &view, "@SignMapMove");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
    }

    /// `@天地合一`：团长自比、成员允许门、冷却与 `GroupRecallTime` 复位。
    #[test]
    fn group_recall_member_gate_and_cooldown() {
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        let gm_id = w.add_entity(mir2_world::Entity::new(0, "gm", 5, 5));
        let bob_id = w.add_entity(mir2_world::Entity::new(0, "bob", 6, 6));
        w.entities.get_mut(gm_id).unwrap().group_owner = gm_id;
        w.entities.get_mut(gm_id).unwrap().group_members = vec![gm_id];
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        {
            let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
            // `GroupMembers` 含团长自己；其 `AllowGroupReCall` 缺省 false ⇒ 自报「不允许天地合一」。
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, "@天地合一")),
                vec![(GmColor::Red, "gm 不允许天地合一!!!")]
            );
            assert_eq!(sess.group_rcall_time, 5, "Config.GroupRecallTime 部署值");
            // oracle：`Permission >= 6` 时先 `GroupRcallTime = 0` ⇒ perm 10 下冷却支
            // （`> dwValue` 的红字）**不可达**；第二次调用仍走团长支。
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, "@天地合一")),
                vec![(GmColor::Red, "gm 不允许天地合一!!!")]
            );
        }
        // 允许传送 + 地图无 `NORECALL` ⇒ RecallHuman 未接 ⇒ 零帧 + 留痕。
        sess.group_rcall_time = 0;
        {
            // 允许标记走**登录生产入口**（`WorldServer.cs:1159` 的 `GetHumData` 装载），
            // 不手工置位实体字段——手工置位曾掩盖「登录不装载 `AllowGroupReCall`」的缺口
            // （Important-2）。
            let flags = crate::record::CharacterData {
                allow_group_re_call: true,
                ..crate::record::CharacterData::default()
            };
            let me = w.entities.get_mut(gm_id).unwrap();
            crate::apply_login_record_scalars(me, Some(&flags));
            me.group_members = vec![gm_id, bob_id];
        }
        {
            let flags = crate::record::CharacterData {
                allow_group_re_call: true,
                ..crate::record::CharacterData::default()
            };
            let bob = w.entities.get_mut(bob_id).unwrap();
            crate::apply_login_record_scalars(bob, Some(&flags));
        }
        {
            let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
            let r = execute(&mut sess, &cfg, &view, "@天地合一");
            assert!(r.msgs.is_empty() && r.log_note.is_some());
        }
        // 非团长：静默（不置 tick/time）。
        sess.group_rcall_time = 0;
        w.entities.get_mut(gm_id).unwrap().group_owner = bob_id;
        let before = sess.group_rcall_tick;
        {
            let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
            let r = execute(&mut sess, &cfg, &view, "@天地合一");
            assert!(r.msgs.is_empty() && r.log_note.is_none());
        }
        assert_eq!(sess.group_rcall_time, 0);
        assert!(sess.group_rcall_tick >= before);
    }

    /// `@df56dfs`：掌门门、地图 `NOGUILDRECALL`、攻城区域、成员遍历 no-op、`GuildRecallTime` 冷却。
    #[test]
    fn guild_recall_master_map_castle_and_noop_member_loop() {
        let f = map_fixture(mir2_envir::MapInfoFlag::default());
        let f_no = map_fixture(mir2_envir::MapInfoFlag {
            no_guild_re_call: true,
            ..mir2_envir::MapInfoFlag::default()
        });
        let w = gm_world();
        let mut guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // 无行会 ⇒ 不是掌门。
        {
            let view = GmWorld::new(&guilds)
                .with_maps(&f.registry)
                .with_world(&w)
                .with_speaker("gm");
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, "@df56dfs")),
                vec![(GmColor::Red, "行会掌门人才可以使用此功能!!!")]
            );
        }
        assert!(guilds.add_guild("G1", "gm", &mir2_world::GuildConfig::default()));
        // 成员遍历在 oracle 里注释 ⇒ 恒 0/0。
        {
            let view = GmWorld::new(&guilds)
                .with_maps(&f.registry)
                .with_world(&w)
                .with_speaker("gm");
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, "@df56dfs")),
                vec![(GmColor::Green, "已传送0个成员，0个成员未被传送。")]
            );
            assert_eq!(sess.group_rcall_time, 600, "Config.GuildRecallTime 部署值");
            // oracle：`Permission >= 6` 时先 `GroupRcallTime = 0` ⇒ perm 10 下冷却红字不可达。
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, "@df56dfs")),
                vec![(GmColor::Green, "已传送0个成员，0个成员未被传送。")]
            );
        }
        // 地图 `NOGUILDRECALL`。
        sess.group_rcall_time = 0;
        let view_no = GmWorld::new(&guilds)
            .with_maps(&f_no.registry)
            .with_world(&w)
            .with_speaker("gm");
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view_no, "@df56dfs")),
            vec![(GmColor::Red, "本地图不允许使用此功能!!!")]
        );
        // 攻城区域（首个命中的城堡 `UnderWar`）。
        let mut w_war = gm_world();
        let mut castle =
            mir2_world::UserCastle::new("沙巴克", mir2_world::CastleSiegeConfig::default());
        castle.castle_map_id = Some(0);
        castle.home_x = 5;
        castle.home_y = 5;
        castle.war_range_x = 100;
        castle.war_range_y = 100;
        castle.under_war = true;
        w_war.castles.push(castle);
        sess.group_rcall_time = 0;
        let view_war = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w_war)
            .with_speaker("gm");
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view_war, "@df56dfs")),
            vec![(GmColor::Red, "攻城区域不允许使用此功能!!!")]
        );
    }

    /// `@传送`：传送装备门、地图禁移、`UserMoveTime` 冷却、物品阻挡、成功零帧。
    #[test]
    fn user_move_xy_gates_and_texts() {
        let f = map_fixture(mir2_envir::MapInfoFlag::default());
        let f_no = map_fixture(mir2_envir::MapInfoFlag {
            bo_nopositionmove: true,
            ..mir2_envir::MapInfoFlag::default()
        });
        let w = gm_world();
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        assert_eq!(
            texts(&execute(
                &mut sess,
                &cfg,
                &GmWorld::new(&guilds)
                    .with_maps(&f.registry)
                    .with_world(&w)
                    .with_speaker("gm"),
                "@传送 5 5"
            )),
            vec![(GmColor::Red, "您现在还无法使用此功能!!!")]
        );
        sess.teleport = true;
        // 冷却：now=5000、tick=0 ⇒ 10 - 5 = 5。
        let view = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm")
            .with_now(5000);
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@传送 5 5")),
            vec![(GmColor::Red, "5秒之后才可以再使用此功能!!!")]
        );
        // 过冷却 ⇒ SpaceMove 未接 ⇒ 零帧 + 留痕，且 tick 前移。
        let view_ok = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm")
            .with_now(20000);
        let r = execute(&mut sess, &cfg, &view_ok, "@传送 5 5");
        assert!(r.msgs.is_empty() && r.log_note.is_some());
        assert_eq!(sess.teleport_tick, 20000);
        // **毫秒口径边界（Important-1）**：tick=0、now=10500 ⇒ 原始毫秒 `10500 > 10000`
        // ⇒ oracle 放行（`UserMoveXYCommand.cs:31`）；旧实现先 `/1000` 截成 10 秒 ⇒
        // `10 > 10` 为假 ⇒ 误发红字且不推进 tick。红检：把门改回秒口径 ⇒ 本断言红。
        sess.teleport_tick = 0;
        let view_edge = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm")
            .with_now(10500);
        let r = execute(&mut sess, &cfg, &view_edge, "@传送 5 5");
        assert!(
            r.msgs.is_empty() && r.log_note.is_some(),
            "10500ms > UserMoveTime*1000 ⇒ 放行"
        );
        assert_eq!(sess.teleport_tick, 10500, "放行支推进 TeleportTick");
        // 恰好 10000ms：`>` 严格 ⇒ 仍拒；红字秒数 = `10 - 10000/1000` = 0。
        sess.teleport_tick = 0;
        let view_eq = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w)
            .with_speaker("gm")
            .with_now(10000);
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view_eq, "@传送 5 5")),
            vec![(GmColor::Red, "0秒之后才可以再使用此功能!!!")]
        );
        assert_eq!(sess.teleport_tick, 0, "拒绝支不推进 tick");
        // 地图 `NOPOSITIONMOVE`。
        let view_no = GmWorld::new(&guilds)
            .with_maps(&f_no.registry)
            .with_world(&w)
            .with_speaker("gm");
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view_no, "@传送 5 5")),
            vec![(GmColor::Red, "此地图禁止使用此命令!!!")]
        );
        // 格上物品 + `UserMoveCanOnItem=0` ⇒ Green（正文用当前地图名 + 原字符串坐标）。
        let mut w_item = gm_world();
        w_item.maps[0].add_map_object(3, 3, mir2_world::CellType::Item, 999, 0);
        let cfg_block = GmConfig {
            user_move_can_on_item: false,
            ..GmConfig::default()
        };
        let view_item = GmWorld::new(&guilds)
            .with_maps(&f.registry)
            .with_world(&w_item)
            .with_speaker("gm");
        assert_eq!(
            texts(&execute(&mut sess, &cfg_block, &view_item, "@传送 3 3")),
            vec![(GmColor::Green, "无法移动到地图: map X:3 Y:3")]
        );
    }

    // ===================== 批次 4 · 数值/角色族（44 条） =====================

    /// 牙 ①：`@CHGUSERFULL` 的恒真门 —— 任何输入都只出两句红字，`Config.UserFull` 永不写。
    /// 若有人「修好」这个条件，本用例立刻红。
    #[test]
    fn change_user_full_is_always_the_two_red_messages() {
        for line in [
            "@CHGUSERFULL",
            "@CHGUSERFULL 100",
            "@CHGUSERFULL 0",
            "@CHGUSERFULL abc",
            "@CHGUSERFULL -5",
        ] {
            let (r, _) = run_gm(line);
            assert_eq!(
                texts(&r),
                vec![
                    (GmColor::Red, "设置服务器最高上线人数。"),
                    (GmColor::Red, "命令格式: @CHGUSERFULL 人数"),
                ],
                "line={line}"
            );
        }
    }

    /// 牙 ②：`@查看PK` 的反向门 —— 给名字恒红字帮助；空名才落到「不在线」（且名字位为空 ⇒ 前导空格）。
    #[test]
    fn pk_point_inverted_guard() {
        let (r, _) = run_gm("@查看PK 张三");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: @查看PK 人物名称")]
        );
        let (r, _) = run_gm("@查看PK");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, " 现在不在线，或在其它服务器上!!!")]
        );
    }

    /// 牙 ③：`@AttackMode` 的双加号（照抄 oracle 连加两次）——序列必须是 2,4,6,1,3,5,0,2。
    #[test]
    fn change_attack_mode_double_increment_sequence() {
        let mut sess = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        let guilds = mir2_world::GuildManager::new();
        let world = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        let expected = [
            "[攻击模式: 夫妻攻击]",
            "[攻击模式: 编组攻击]",
            "[攻击模式: 红名攻击]",
            "[攻击模式: 和平攻击]",
            "[攻击模式: 师徒攻击]",
            "[攻击模式: 行会攻击]",
            "[攻击模式: 全体攻击]",
            "[攻击模式: 夫妻攻击]",
        ];
        for want in expected {
            let r = execute(&mut sess, &cfg, &world, "@AttackMode");
            assert_eq!(texts(&r), vec![(GmColor::Green, want)]);
        }
    }

    /// 牙 ④：空 `Help` 的 help 文本**末尾空格**必须保留（`Command.CommandHelp` 的 `@ {Help}` 拼接）。
    #[test]
    fn empty_help_keeps_trailing_space() {
        let (r, _) = run_gm("@1级");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: @1级 人物名称 等级")]
        );
        // 空体的命令**不发**帮助（oracle 体为空）；末尾空格口径由常量本身钉住。
        let (r, _) = run_gm("@GameDiaMond");
        assert!(r.msgs.is_empty());
        assert!(CHANGE_ADMIN_MODE_HELP.ends_with(' '));
        assert!(GAME_DIAMOND_HELP.ends_with(' '));
        assert!(TRAINING_HELP.ends_with(' '));
        let (r, _) = run_gm("@RefineWeaponwa 20 0 0 0");
        assert!(r.msgs.is_empty(), "dc+mc+sc>10 ⇒ 静默");
    }

    /// 门禁：`@加钱` 在现网 `Permission = 0` 下不可达（红字），perm 10 才进命令体。
    #[test]
    fn add_gold_permission_gate_and_offline_text() {
        let (r, _) = run("@加钱 张三 100");
        assert_eq!(texts(&r), vec![(GmColor::Red, "权限不够!!!")]);

        let (r, _) = run_gm("@加钱");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: @加钱 人物名称  金币数量")]
        );

        // 空世界 ⇒ `GetPlayObject` 必空 ⇒ 离线支（绿字，与 `@减钱` 文案不同）。
        let (r, _) = run_gm("@加钱 张三 100");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Green, "张三 现在不在线，等其上线时金币将自动增加")]
        );
        let (r, _) = run_gm("@减钱 张三 100");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Green, "张三现在不在线，等其上线时金币将自动减少")]
        );
    }

    /// `@减钱` 的第一道门**静默**返回（oracle 此处没有 SysMsg）。
    #[test]
    fn del_gold_first_guard_is_silent() {
        let (r, _) = run_gm("@减钱 张三 0");
        assert!(r.msgs.is_empty() && r.found);
        let (r, _) = run_gm("@减钱");
        assert!(r.msgs.is_empty() && r.found);
    }

    /// `@ChangeItemName` 的生效写入整段被注释 ⇒ 门过了恒红字。
    #[test]
    fn change_item_name_always_occupied() {
        let (r, _) = run_gm("@ChangeItemName 1 2 木剑");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "此物品，已经设置了其它的名称!!!")]
        );
        let (r, _) = run_gm("@ChangeItemName 0 2 木剑");
        assert_eq!(
            texts(&r),
            vec![(
                GmColor::Red,
                "命令格式: @ChangeItemName 物品编号 物品ID号 物品名称"
            )]
        );
    }

    /// 三个自翻转开关：`@1`（管理员）/`@2`（隐身）/`@3`（无敌）。
    #[test]
    fn session_mode_toggles() {
        let mut sess = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        let guilds = mir2_world::GuildManager::new();
        let world = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &world, "@1")),
            vec![(GmColor::Green, "已进入管理员模式")]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &world, "@1")),
            vec![(GmColor::Green, "已退出管理员模式")]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &world, "@2")),
            vec![(GmColor::Green, "已进入隐身模式")]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &world, "@2")),
            vec![(GmColor::Green, "已退出隐身模式")]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &world, "@3")),
            vec![(GmColor::Green, "已进入无敌模式")]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &world, "@3")),
            vec![(GmColor::Green, "已退出无敌模式")]
        );
    }

    /// `@Rest`：无宝宝 ⇒ 翻转 `SlaveRelax` 但不发帧。
    #[test]
    fn change_salve_status_silent_without_slaves() {
        let (r, sess) = run_gm("@Rest");
        assert!(r.msgs.is_empty());
        assert!(sess.slave_relax);
    }

    /// 离线文案的三处**不一致**必须逐字保留。
    #[test]
    fn offline_texts_are_not_uniform() {
        let (r, _) = run_gm("@改变性别 张三 男");
        assert_eq!(texts(&r), vec![(GmColor::Red, "张三没有在线!!!")]);
        let (r, _) = run_gm("@转生 张三");
        assert_eq!(texts(&r), vec![(GmColor::Red, "张三 没在线上!!!")]);
        let (r, _) = run_gm("@增加技能 张三 火球术 2");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "张三不在线，或在其它服务器上!!")]
        );
        let (r, _) = run_gm("@调整声望 张三 + 10");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "张三 现在不在线，或在其它服务器上!!!")]
        );
    }

    /// `@调整泡点` 的两处异常路径 ⇒ 零帧（oracle 抛异常被外层 catch 吞掉）。
    #[test]
    fn game_point_exception_paths_are_silent() {
        let (r, _) = run_gm("@调整泡点 张三 + abc");
        assert!(r.msgs.is_empty());
        assert!(r.log_note.is_some());
        // 控制符只有一个字符 ⇒ `sCtr[1]` 越界（在线支之前先要过 `target_online`，空世界落离线支）
        let (r, _) = run_gm("@调整泡点 张三 = 10");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "张三 现在不在线，或在其它服务器上!!!")]
        );
    }

    /// I-2：无参 `@Binduseitem` 在 oracle 下**不是**零帧——`ExtractCommandAndParameters`
    /// 给 `string.Empty`，`Split(' ')` 得 `[""]`（长度 1）⇒ 继续走门，
    /// `GetUseItemIdx("") = -1` ⇒ 红字 CommandHelp。
    #[test]
    fn bind_use_item_empty_params_is_red_help() {
        // 无参：`"".Split(' ') = [""]` ⇒ 走到门 ⇒ 空名/未知名槽/未知名绑定方法 ⇒ 红帮助
        // （不是「无参特判」）。
        let (r, _) = run_gm("@Binduseitem");
        assert_eq!(texts(&r), vec![(GmColor::Red, BIND_USE_ITEM_HELP)]);
        // 未知名槽/绑定方法 ⇒ 同一红帮助门。
        for line in [
            "@Binduseitem 张三 未知槽 帐号",
            "@Binduseitem 张三 项链 未知类型",
        ] {
            let (r, _) = run_gm(line);
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, BIND_USE_ITEM_HELP)],
                "{line}"
            );
        }
    }

    /// **F1（review ac8c3ab2，扫盲卡第 8 条偏差）**：`@Binduseitem` 合法槽名输入类——
    /// `GetUseItemIdx` 是**编译期常量表**（`SystemShare.cs:1834-1891` + `MessageSettings.cs:15-27`
    /// 13 个 const 名），门后三支：① 离线 ⇒ 红 `{名} 现在不在线，或在其它服务器上!!!`；
    /// ② 在线空槽（`UseItems[n].Index == 0`）⇒ 红 `{名}的{槽}没有戴物品!!!`（`CommandHelp.cs:102`）；
    /// ③ 在线已穿戴 ⇒ `switch(nBind)` 触 `SystemShare.ItemBind*`（`SystemShare.cs:112-115`
    /// 声明即 null）⇒ `EnterCriticalSection(null)` NRE ⇒ **零帧** + NOTE。
    /// 13 名全表逐名核（全部解析 ⇒ 离线红字，不是帮助）。
    /// 变异红：M-F1a（表漏 项链 ⇒ 该名回帮助）⇒ 13 名牙红；M-F1b（已穿戴支补文本）⇒ 零帧牙红。
    #[test]
    fn bind_use_item_legal_slot_three_branches() {
        use mir2_world::Entity;
        let names = [
            ("衣服", 0),
            ("武器", 1),
            ("照明物", 2),
            ("项链", 3),
            ("头盔", 4),
            ("左手镯", 5),
            ("右手镯", 6),
            ("左戒指", 7),
            ("右戒指", 8),
            ("物品", 9),
            ("腰带", 10),
            ("鞋子", 11),
            ("宝石", 12),
        ];
        // 13 名全表：用「离线目标」区分「槽名解析成功」（⇒ 离线红）与「未知名」（⇒ 帮助）。
        for (slot_name, idx) in names {
            let (r, _) = run_gm(&format!("@Binduseitem 张三 {slot_name} 帐号"));
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, "张三 现在不在线，或在其它服务器上!!!")],
                "{slot_name}（槽位 {idx}）必须解析成功 ⇒ 离线红，不是帮助"
            );
        }
        // 在线三支：bob 在 world 里。
        let (fx, mut w, _gm_id, bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // ② 空槽（默认全零）⇒ 红 NoItemMsg。
        {
            let view = q_view(&fx, &w, &guilds);
            let r = execute(&mut sess, &cfg, &view, "@Binduseitem bob 项链 帐号");
            assert_eq!(texts(&r), vec![(GmColor::Red, "bob的项链没有戴物品!!!")]);
        }
        // ③ 已穿戴（UseItems[3].Index = 15）⇒ switch 触 null 表 ⇒ 零帧 + NOTE。
        {
            let bob = w.entities.get_mut(bob_id).unwrap();
            bob.use_items[3].index = 15;
        }
        let view = q_view(&fx, &w, &guilds);
        let r = execute(&mut sess, &cfg, &view, "@Binduseitem bob 项链 帐号");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "已穿戴支真机零帧");
        let note = r.log_note.expect("零帧必须登记 NRE 触点");
        assert!(
            note.contains("声明即 null") && note.contains("零帧"),
            "{note}"
        );
        // 其余绑定方法（人物/IP/死亡）同样触表 ⇒ 零帧。
        for bind_type in ["人物", "IP", "死亡"] {
            let r = execute(
                &mut sess,
                &cfg,
                &view,
                &format!("@Binduseitem bob 项链 {bind_type}"),
            );
            assert!(
                r.msgs.is_empty() && r.frames.is_empty(),
                "{bind_type} 应零帧"
            );
            assert!(r.log_note.is_some());
        }
        // 对照：已穿戴的「衣服」槽若为空格 ⇒ 红 NoItemMsg（不是绑定表零帧）。
        let r = execute(&mut sess, &cfg, &view, "@Binduseitem bob 衣服 帐号");
        assert_eq!(texts(&r), vec![(GmColor::Red, "bob的衣服没有戴物品!!!")]);
        // 目标 Entity 字段未受污染（只读命令）。
        let e: &Entity = w.entities.get(bob_id).unwrap();
        assert_eq!(e.use_items[3].index, 15);
    }

    /// I-3：单 UTF-16 码元名 ⇒ oracle `sHumanName[1]` 抛 `IndexOutOfRangeException`
    /// （`PlayObject.Chat.cs` 外层 catch 吞掉 ⇒ 零帧），不能当成「不是 ?」落离线红字。
    /// 求值顺序：先 ctr/数值门，再 `sHumanName[1]`。
    #[test]
    fn one_code_unit_name_throws_before_offline_check() {
        let (r, _) = run_gm("@VV调整元宝 X = 10");
        assert!(r.msgs.is_empty(), "{:?}", r.msgs);
        assert!(r.log_note.is_some());
        let (r, _) = run_gm("@调整泡点 X = 10");
        assert!(r.msgs.is_empty(), "{:?}", r.msgs);
        assert!(r.log_note.is_some());
        // 对照 A：两码元名走到离线红字（零帧来自越界，不是门）。
        let (r, _) = run_gm("@VV调整元宝 XY = 10");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "XY 现在不在线，或在其它服务器上!!!")]
        );
        // 对照 B：前面门先红 ⇒ 即使在单码元名上也不抛越界（求值顺序）。
        let (r, _) = run_gm("@VV调整元宝 X ! 10");
        assert_eq!(texts(&r), vec![(GmColor::Red, GAME_GOLD_HELP)]);
    }

    /// I-4：登录装载 `AttatckMode`（`WorldServer.cs:1148`）与 `ReLevel`（`:1133`）。
    #[test]
    fn gm_session_carries_attack_mode_and_re_level() {
        let mut sess = GmSession::from_character("", 0, false, false, 0, 3, 7);
        assert_eq!(sess.attack_mode, 3);
        assert_eq!(sess.re_level, 7);
        sess.permission = 10;
        let guilds = mir2_world::GuildManager::new();
        let world = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        // 从持久值 3 起算：3 → 4 → 5（`@AttackMode` 连加两次）⇒ 行会攻击。
        let r = execute(&mut sess, &cfg, &world, "@AttackMode");
        assert_eq!(texts(&r), vec![(GmColor::Green, "[攻击模式: 行会攻击]")]);
        assert_eq!(sess.attack_mode, 5);
    }

    /// 44 条命令全部接上（不再落 `Imp::Unimplemented`），且派发可达。
    #[test]
    fn batch4_commands_are_wired() {
        let names = [
            "AddGamePoint",
            "加钱",
            "调整经验",
            "1级",
            "AdjustExp",
            "Binduseitem",
            "调整属性",
            "1",
            "AttackMode",
            "伴侣变更",
            "改变性别",
            "ChangeItemName",
            "职业变更",
            "Level0",
            "ChangeMasterName",
            "2",
            "Rest",
            "3",
            "CHGUSERFULL",
            "CHGZENFASTSTEP",
            "调整声望",
            "DelBonusPoint",
            "删除物品",
            "DelGamePoint",
            "减钱",
            "清除PK",
            "GameDiaMond",
            "GameGird",
            "GameGlory",
            "VV调整元宝",
            "调整泡点",
            "GetUserItems",
            "改变发型",
            "Hunger",
            "增加PK",
            "LuckyPoint",
            "查看PK",
            "RefineWeaponwa",
            "转生",
            "RestBonuPoint",
            "+jn",
            "TrainingMagic",
            "增加技能",
            "UnBindUseItem",
        ];
        for name in names {
            let def = find(name).unwrap_or_else(|| panic!("命令表缺 {name}"));
            assert_ne!(def.imp, Imp::Unimplemented, "{name} 仍是大杂烩占位");
        }
        // 抽样派发：`@AdjustExp`（体只有权限自检）⇒ 零帧零 note 之外的东西。
        let (r, _) = run_gm("@AdjustExp");
        assert!(r.msgs.is_empty() && r.found);
    }

    // ==================== 批次 5 · 脚本可达族（`@禁言`，卡 openmir2-gm-impl-shutup，tier=R）=========

    /// 命令表：`禁言` = `ShutupCommand`，`PermissionMin=10`，`Execute` 形参 2。
    #[test]
    fn shutup_is_wired_in_the_command_table() {
        let def = find("禁言").expect("@禁言 在 185 行表里");
        assert_eq!(def.imp, Imp::Shutup);
        assert_eq!(def.perm_min, 10);
        assert_eq!(def.params, 2);
        // 帮助文本逐字节：`人物名称` 与 `时间长度(分钟)` 之间是**两个** ASCII 空格
        // （`CommandHelp.cs:28`），现网名 `禁言`（`conf[Command].Shutup`）。
        assert_eq!(
            SHUTUP_HELP, "命令格式: @禁言 人物名称  时间长度(分钟)",
            "`GameCommandParamUnKnow` + `GameCommandShutupHelpMsg` 的拼装产物"
        );
    }

    /// ① 正常触发：**零帧**（扫盲卡 `openmir2-gm-systemshare-null-sweep`，X1 F1 同款
    /// 运行层真相）。oracle 源文本 `ShutupCommand.cs:56` 有红字 SysMsg
    /// （`GameCommandShutupHumanMsg`），但 `:30` `EnterCriticalSection(SystemShare.DenySayMsgList)`
    /// 对 **null 表**（`SystemShare.cs:64` 声明即 null 全仓零赋值）即 NRE ⇒ 被
    /// `PlayObject.Chat.cs:402` 顶层 catch 吞 ⇒ `:56` 真机不可达 ⇒ 任何合法参数都零帧、
    /// 零副作用。牙：把 `:30` 的 NRE 触点改成不触表（即删 Enter）⇒ 本用例仍绿——
    /// 变异是改回发红字（源文本建模）⇒ 本用例红。
    #[test]
    fn shutup_enters_null_table_and_stays_zero_frames() {
        let (r, _) = run_gm("@禁言 zhangsan 30");
        assert!(r.found && r.parsed, "命中命令体，不走未知命令回落");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "真机零帧");
        let note = r.log_note.expect("零帧必须登记 NRE 触点");
        assert!(
            note.contains("声明即 null") && note.contains("零帧"),
            "{note}"
        );
        // 合法参数三形态同答（校验支在 Enter 之前 ⇒ 帮助文本不受影响，见 ② 用例）。
        for line in ["@禁言 bob 1", "@禁言 玩家 99999"] {
            let (r2, _) = run_gm(line);
            assert!(r2.msgs.is_empty() && r2.frames.is_empty(), "{line} 应零帧");
        }
    }

    /// ② 参数校验与越界怪癖（`ShutupCommand.cs:21-29`）。
    ///
    /// 顺序：`sTime` 空 或 `sHumanName` 空 或 `sHumanName[1] == '?'` ⇒ 帮助；C# `||` 短路
    /// ⇒ `sTime` 空时**不会**触发 `sHumanName[1]` 的越界异常。
    #[test]
    fn shutup_param_validation_and_single_char_name_quirk() {
        // 缺时间 / 缺名字。
        assert_eq!(
            texts(&run_gm("@禁言 zhangsan").0),
            vec![(GmColor::Red, SHUTUP_HELP)]
        );
        assert_eq!(texts(&run_gm("@禁言").0), vec![(GmColor::Red, SHUTUP_HELP)]);
        // `String.Split(' ')` 保留空段 ⇒ 双空格把 `sTime` 变成空串（不是取后面的 30）。
        assert_eq!(
            texts(&run_gm("@禁言 zhangsan  30").0),
            vec![(GmColor::Red, SHUTUP_HELP)]
        );
        // 第 2 个 UTF-16 码元是 '?' ⇒ 帮助。
        assert_eq!(
            texts(&run_gm("@禁言 x? 30").0),
            vec![(GmColor::Red, SHUTUP_HELP)]
        );
        // 单字符名字 + 非空时间 ⇒ `sHumanName[1]` 越界，被 `ProcessUserLineMsg` 的 catch
        // 吞掉 ⇒ 零帧（`ShutupCommand.cs:24` 的 `sHumanName[1]`，不是 `[0]`）。
        let (r, _) = run_gm("@禁言 x 30");
        assert!(r.found, "命令已命中（异常发生在命令体内部）");
        assert!(r.msgs.is_empty(), "越界 ⇒ 零帧");
        assert!(r.frames.is_empty());
        assert_eq!(r.log_note, Some(NOTE_INDEX_OUT_OF_RANGE));
        // 但 `sTime` 为空时短路 ⇒ 不碰 `sHumanName[1]`，正常回执。
        assert_eq!(
            texts(&run_gm("@禁言 x").0),
            vec![(GmColor::Red, SHUTUP_HELP)]
        );
        // 时间不可解析 / 负值回转 / 多余参数——全部在 `:30` Enter(null) NRE 之后 ⇒ 真机零帧
        // （源文本的红字回执真机不可达；`StrToInt` 缺省 5 与 uint 回转逻辑随之不可达）。
        for line in [
            "@禁言 zhangsan abc",
            "@禁言 zhangsan -1",
            "@禁言 zhangsan 30 extra",
        ] {
            let (r, _) = run_gm(line);
            assert!(r.msgs.is_empty() && r.frames.is_empty(), "{line} 应零帧");
            assert!(r.log_note.is_some(), "{line} 必须登记 NRE 触点");
        }
    }

    /// ③ 权限门不过：命令体**零贡献**，只有 `CommandSystem.Execute` 的通用 `权限不够!!!`。
    ///
    /// 口径说明：oracle `GameCommand.Handle:37-40` 返回 `GameCommandPermissionTooLow`，由
    /// `CommandSystem.cs:180-195` 作为**一条** Red SysMsg 下发——这不是命令体的帧，所以
    /// 「零帧」应读作「命令体零帧、零副作用」；这里按 oracle 真实帧面断言恰好这一条。
    #[test]
    fn shutup_permission_gate_blocks_body() {
        let (r, sess) = run("@禁言 zhangsan 30");
        assert!(r.found && r.parsed);
        assert_eq!(texts(&r), vec![(GmColor::Red, PERMISSION_TOO_LOW)]);
        assert!(r.frames.is_empty());
        assert!(r.log_note.is_none());
        assert_eq!(sess.permission, 0, "被挡下时权限不变");
        // 命令体文本不得出现（没有「已被禁言」回执）。
        assert!(r.msgs.iter().all(|m| !m.text.contains("已被禁言")));
    }

    /// ④ 新角色登录路径：`Gmexecute 禁言 self 10000`（`QuestDiary/游戏登陆/登陆脚本.txt:16`
    /// ← `MapQuest_def/QManage.txt:37` #CALL ← `PlayObject.cs:1440` 的 `@Login`，`#IF ISNEWHUMAN`）。
    ///
    /// `ActionOfGmExecute`（`ExecutionProcessingSys.cs:4925-4947`）拼出
    /// `"@{sParam1} {sParam2} {sParam3} {sParam4} {sParam5} {sParam6}"`（`sParam2=Self` ⇒ 角色名，
    /// 尾部三个空参 ⇒ 三个尾随空格），再把 `Permission` 临时置 10 执行、随后还原。
    ///
    /// 本用例走**宿主跳线**（`npc::NpcPlayer` 覆写的 `set_permission`/`process_user_line_msg`，
    /// 卡 `openmir2-gm-script-host-seam`）：不抬权 ⇒ 权限门；抬权 ⇒ **零帧**（扫盲卡：
    /// `@禁言` 触 null 表 NRE ⇒ 被顶层 catch 吞 ⇒ 红字回执真机不可达；notes 有登记）；
    /// 执行后 `Permission` 还原。`@Login` 经真脚本引擎的端到端用例在
    /// `npc.rs` 的 `gmexecute_*` 组（含 `finally` 还原与目标解析）。
    #[test]
    fn shutup_script_gmexecute_path_through_npc_player_seam() {
        use mir2_script::engine::ScriptPlayer;
        let guilds = mir2_world::GuildManager::new();
        let world = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        let mut sess = GmSession::default();
        assert_eq!(sess.permission, 0, "现网 Permission 的来源被锁死为 0");
        // 与 `ActionOfGmExecute` 同形的 sData（尾随 3 个空格由空参产生）。
        let s_data = format!("@{} {} {} {} {} {}", "禁言", "新人", "10000", "", "", "");
        assert_eq!(s_data, "@禁言 新人 10000   ");
        // 不抬权：落到权限门（这正是牙 #2 的反证：删掉抬权 ⇒ 这里变成 `权限不够!!!`）。
        let gated = execute(&mut sess, &cfg, &world, &s_data);
        assert_eq!(texts(&gated), vec![(GmColor::Red, PERMISSION_TOO_LOW)]);
        let mut rng = |_v: i32| 0;
        {
            let mut player = crate::npc::NpcPlayer::empty(&mut rng);
            player.chr_name = "新人".to_string();
            player.gm = Some(crate::npc::NpcGm {
                sess: &mut sess,
                cfg: &cfg,
                world,
                pwd_busy: false,
            });
            // `ActionOfGmExecute` 的抬权（oracle 同址；等价的两步）。
            player.set_permission(10);
            player.process_user_line_msg(&s_data);
            player.set_permission(0);
            assert!(
                player.frames.is_empty(),
                "脚本路同样零帧：命令体 :30 Enter(null) NRE ⇒ 被顶层 catch 吞 ⇒ 红字回执真机不可达"
            );
            assert!(
                player
                    .gm_notes
                    .iter()
                    .any(|n| n.contains("声明即 null") && n.contains("零帧")),
                "脚本路零帧必须登记：{:?}",
                player.gm_notes
            );
            assert!(player.gm_direct_frames.is_empty(), "`@禁言` 无直发帧");
        }
        assert_eq!(sess.permission, 0, "抬权是临时的，执行完必须还原");
    }

    /// 卡 `openmir2-gm-impl-smake`：表项（现网名 `改属性` = `conf[Command].SuperMake`）。
    #[test]
    fn smake_item_is_wired_in_the_command_table() {
        let def = find("改属性").expect("@改属性 在 185 行表里");
        assert_eq!(def.imp, Imp::SmakeItem);
        assert_eq!(def.perm_min, 10, "`[Permission] SuperMake=10`");
        assert_eq!(
            def.params, 2,
            "`Execute(string[] @params, IPlayerActor)` ⇒ case 2"
        );
    }

    /// 卡 `openmir2-gm-impl-clearbag`：表项（现网名 `清理垃圾` = `conf[Command].ClearBag`）。
    #[test]
    fn clear_bag_item_is_wired_in_the_command_table() {
        let def = find("清理垃圾").expect("@清理垃圾 在 185 行表里");
        assert_eq!(def.imp, Imp::ClearBagItem);
        assert_eq!(def.perm_min, 10, "`[Permission] ClearBag=10`");
        assert_eq!(def.params, 2);
    }

    /// `:20-24`：空名 / 首字符 `?` ⇒ 红字帮助（**字面量**把关，不拿 `CLEAR_BAG_HELP` 自证）
    /// + GBK 字节（复审 minor）。
    #[test]
    fn clear_bag_item_reports_help_for_empty_or_question() {
        let help = "命令格式: @清理垃圾 人物名称";
        for line in ["@清理垃圾", "@清理垃圾 ?", "@清理垃圾 ?abc"] {
            let (r, sess) = run_gm(line);
            assert_eq!(texts(&r), vec![(GmColor::Red, help)], "{line}");
            assert!(sess.pending_clear_bag.is_none(), "{line}");
        }
        let (r, _) = run_gm("@清理垃圾");
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0xc3, 0xfc, 0xc1, 0xee, 0xb8, 0xf1, 0xca, 0xbd, 0x3a, 0x20,
                0x40, // 命令格式:␠@
                0xc7, 0xe5, 0xc0, 0xed, 0xc0, 0xac, 0xbb, 0xf8, // 清理垃圾
                0x20, 0xc8, 0xcb, 0xce, 0xef, 0xc3, 0xfb, 0xb3, 0xc6, // ␠人物名称
            ],
            "帮助文本的 GBK 字节逐字节等价"
        );
    }

    /// `:25-30`：目标不在线 ⇒ 一条 Red「…现在不在线，或在其它服务器上!!!」。
    #[test]
    fn clear_bag_item_offline_target_reports_not_online() {
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        let mut e = mir2_world::Entity::new(0, "victim", 5, 5);
        e.race = mir2_world::ACTOR_RACE_PLAY;
        w.add_entity(e);
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
        let r = execute(&mut sess, &cfg, &view, "@清理垃圾 nobody");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "nobody 现在不在线，或在其它服务器上!!!")]
        );
        assert!(sess.pending_clear_bag.is_none());
        // 线上 GBK 字节（复审 minor）：ASCII 名 + GBK 文案。
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0x6e, 0x6f, 0x62, 0x6f, 0x64, 0x79, 0x20, // "nobody "
                0xcf, 0xd6, 0xd4, 0xda, 0xb2, 0xbb, 0xd4, 0xda, 0xcf, 0xdf, // 现在不在线
                0xa3, 0xac, 0xbb, 0xf2, 0xd4, 0xda, 0xc6, 0xe4, 0xcb, 0xfc, // ，或在其它
                0xb7, 0xfe, 0xce, 0xf1, 0xc6, 0xf7, 0xc9, 0xcf, // 服务器上
                0x21, 0x21, 0x21, // !!!
            ]
        );
        // 命中 ⇒ 零帧 + 登记**实体规范名**（复审 important-2：宿主精确匹配）。
        let r = execute(&mut sess, &cfg, &view, "@清理垃圾 VICTIM");
        assert!(r.msgs.is_empty(), "命中 ⇒ 命令本身零帧");
        assert_eq!(sess.pending_clear_bag.as_deref(), Some("victim"));
        // **部署形态**（`假人上线NPC-jiaren.txt:19`）：`GMEXECUTE 清理垃圾 <$USERNAME>` 的
        // 参数**不展开**（T-165）⇒ 按字面量查不到人 ⇒ 一条红字、零 709、零清包。
        let r = execute(&mut sess, &cfg, &view, "@清理垃圾 <$USERNAME>");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "<$USERNAME> 现在不在线，或在其它服务器上!!!")],
            "部署字面量形态 ⇒ 不在线支（不是清包）"
        );
        assert!(sess.pending_clear_bag.is_none());
    }

    /// 卡 `openmir2-gm-impl-showeffect`：表项（`Command.conf` 无覆盖 ⇒ 注册名保持英文
    /// `ShowEffect`；部署脚本写 `showeffect` 靠 `OrdinalIgnoreCase` 命中）。
    #[test]
    fn show_effect_is_wired_in_the_command_table() {
        let def = find("showeffect").expect("@ShowEffect 在 185 行表里（大小写不敏感）");
        assert_eq!(def.name, "ShowEffect");
        assert_eq!(def.imp, Imp::ShowEffect);
        assert_eq!(def.perm_min, 10);
        assert_eq!(
            def.params, 2,
            "`Execute(string[] @params, IPlayerActor)` ⇒ case 2"
        );
    }

    /// 卡 `openmir2-gm-impl-kick`：表项（现网名 `VV踢人` = `Command.conf` 的 `[Command] Kick`）。
    #[test]
    fn kick_human_is_wired_in_the_command_table() {
        let def = find("VV踢人").expect("@VV踢人 在 185 行表里");
        assert_eq!(def.imp, Imp::KickHuman);
        assert_eq!(def.perm_min, 10, "`[Permission] Kick=10`");
        assert_eq!(
            def.params, 2,
            "`Execute(string[] @params, IPlayerActor)` ⇒ case 2"
        );
    }

    /// 卡 `openmir2-gm-impl-mobfireburn`：表项（`Command.conf` 无覆盖 ⇒ 注册名英文
    /// `MobFireBurn`；部署脚本写 `MOBFIREBURN` 靠 `OrdinalIgnoreCase` 命中）。
    #[test]
    fn mob_fire_burn_is_wired_in_the_command_table() {
        let def = find("MOBFIREBURN").expect("@MobFireBurn 在 185 行表里（大小写不敏感）");
        assert_eq!(def.name, "MobFireBurn");
        assert_eq!(def.imp, Imp::MobFireBurn);
        assert_eq!(def.perm_min, 10);
        assert_eq!(
            def.params, 2,
            "`Execute(string[] @params, IPlayerActor)` ⇒ case 2"
        );
    }

    /// 造一个说话者实体（`GmWorld::self_entity` 的来源），可带一个佩戴件。
    fn smake_world_with_slot(
        slot: usize,
        item: mir2_world::entity::UseItemSlot,
    ) -> (mir2_world::World, mir2_world::GuildManager) {
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        let mut e = mir2_world::Entity::new(0, "gm", 5, 5);
        e.race = mir2_world::ACTOR_RACE_PLAY;
        if slot < e.use_items.len() {
            e.use_items[slot] = item;
        }
        w.add_entity(e);
        (w, mir2_world::GuildManager::new())
    }

    /// `:26` 三参范围守卫：任一越界 ⇒ **静默 return**（零帧、无副作用）。
    #[test]
    fn smake_item_out_of_range_is_silent() {
        for line in [
            "@改属性 13 1 1",
            "@改属性 1 16 1",
            "@改属性 1 1 256",
            "@改属性 -1 1 1",
        ] {
            let (r, sess) = run_gm(line);
            assert!(r.found && r.parsed, "{line}: 命中命令体");
            assert!(r.msgs.is_empty(), "{line}: 越界 ⇒ 零帧");
            assert!(sess.pending_use_item.is_none(), "{line}: 不得登记佩戴位写");
        }
    }

    /// `:28` 空槽 ⇒ 一条红字固定文案（不是帮助格式）。
    #[test]
    fn smake_item_empty_slot_reports_red_text() {
        let (w, guilds) = smake_world_with_slot(1, Default::default());
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
        let r = execute(&mut sess, &cfg, &view, "@改属性 1 2 7");
        assert_eq!(texts(&r), vec![(GmColor::Red, SMAKE_NO_ITEM_HELP)]);
        assert!(sess.pending_use_item.is_none());
    }

    /// 命中：`Desc[9]` 无条件 +1、`Desc[nValueType] = nValue`；蓝字回执逐字节
    /// （`{Index}-{MakeIndex} {Dura}/{DuraMax} {Desc0}/…/{Desc13}`）。
    #[test]
    fn smake_item_writes_desc_and_reports_blue_dump() {
        let item = mir2_world::entity::UseItemSlot {
            make_index: 77,
            index: 5,
            dura: 100,
            dura_max: 200,
            desc: [0; 14],
        };
        let (w, guilds) = smake_world_with_slot(1, item);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
        let r = execute(&mut sess, &cfg, &view, "@改属性 1 2 7");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Blue, "5-77 100/200 0/0/7/0/0/0/0/0/0/1/0/0/0/0")],
            "Desc[2]=7、Desc[9] 0→1；其余槽原样"
        );
        let pend = sess.pending_use_item.expect("命中 ⇒ 必须登记佩戴位写");
        assert_eq!(pend.slot, 1);
        assert_eq!((pend.index, pend.make_index), (5, 77));
        assert_eq!((pend.dura, pend.dura_max), (100, 200));
        assert_eq!(pend.desc[2], 7);
        assert_eq!(pend.desc[9], 1);
    }

    /// `:44-55`：`nValueType` 14 ⇒ `Dura = _MIN(65, nValue) * 1000`；15 ⇒ `DuraMax` 同式。
    #[test]
    fn smake_item_types_14_and_15_set_durability() {
        let item = mir2_world::entity::UseItemSlot {
            make_index: 9,
            index: 3,
            dura: 1,
            dura_max: 2,
            desc: [0; 14],
        };
        let cfg = GmConfig::default();
        for (args, dura, dura_max) in [("14 100", 65_000_u16, 2_u16), ("15 7", 1_u16, 7_000)] {
            let (w, guilds) = smake_world_with_slot(2, item);
            let mut sess = gm_session();
            let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
            let r = execute(&mut sess, &cfg, &view, &format!("@改属性 2 {args}"));
            assert!(!r.msgs.is_empty(), "{args}: 命中 ⇒ 蓝字回执");
            let pend = sess.pending_use_item.expect("必须登记");
            assert_eq!((pend.dura, pend.dura_max), (dura, dura_max), "{args}");
            assert_eq!(pend.desc[9], 1, "{args}: 升级次数无条件 +1");
        }
    }

    /// oracle 命令体 `:20-70` **整段被注释** ⇒ 任何参数（含部署脚本的 78/79/80）都零帧、
    /// 零副作用；`found` 仍为 true（不走未知命令回落）。
    #[test]
    fn show_effect_is_zero_frames_for_every_parameter() {
        for line in [
            "@ShowEffect",
            "@ShowEffect 78",
            "@ShowEffect 78 4000",
            "@showeffect 87",
            "@ShowEffect abc def ghi",
        ] {
            let (r, sess) = run_gm(line);
            assert!(r.found && r.parsed, "{line}: 命中命令体");
            assert!(r.msgs.is_empty(), "{line}: 零帧");
            assert!(r.frames.is_empty(), "{line}: 无直发帧");
            assert_eq!(
                r.log_note,
                Some(NOTE_SHOW_EFFECT_ALL_COMMENTED),
                "{line}: 空实现必须登记"
            );
            assert_eq!(sess.permission, 10, "{line}: 命令体不得改权限（无副作用）");
        }
    }

    /// 权限门先于命令体：`Permission < 10` ⇒ 一条红字 `权限不够!!!`。
    #[test]
    fn show_effect_permission_gate_blocks_body() {
        let mut sess = GmSession {
            permission: 9,
            ..GmSession::default()
        };
        let cfg = GmConfig::default();
        let r = exec(&mut sess, &cfg, "@ShowEffect 78");
        assert_eq!(texts(&r), vec![(GmColor::Red, "权限不够!!!")]);
        sess.permission = 10;
        let r = exec(&mut sess, &cfg, "@ShowEffect 78");
        assert!(r.msgs.is_empty(), "抬权后仍是空实现");
    }

    /// 部署脚本路（`Market_Def/QFunction-0.txt` 的 `@StdModeFunc14`：
    /// `Gmexecute showeffect 78/79/80`）经 `NpcPlayer` 的 GMEXECUTE 跳线执行 ⇒ **零帧**
    /// （oracle 命令体整段注释；此前这段是 `Imp::Unimplemented`，现在仍是零帧但已登记）。
    #[test]
    fn show_effect_script_gmexecute_path_is_zero_frames() {
        use mir2_script::engine::ScriptPlayer;
        let guilds = mir2_world::GuildManager::new();
        let view = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        let mut sess = GmSession::default();
        let s_data = format!("@{} {} {} {} {} {}", "showeffect", "78", "", "", "", "");
        let mut rng = |_v: i32| 0;
        {
            let mut player = crate::npc::NpcPlayer::empty(&mut rng);
            player.chr_name = "新人".to_string();
            player.gm = Some(crate::npc::NpcGm {
                sess: &mut sess,
                cfg: &cfg,
                world: view,
                pwd_busy: false,
            });
            player.set_permission(10);
            player.process_user_line_msg(&s_data);
            player.set_permission(0);
            assert!(player.frames.is_empty(), "空实现 ⇒ 脚本侧零帧");
            assert!(player.gm_direct_frames.is_empty());
            assert!(
                player.gm_notes.iter().any(|n| n.contains("整段注释")),
                "空实现路径必须登记：{:?}",
                player.gm_notes
            );
        }
        assert_eq!(sess.permission, 0, "抬权临时生效并还原");
    }

    /// 卡 `openmir2-gm-impl-shutup-release`：表项（名字被 `Command.conf` 覆盖为 `取消禁言`）。
    #[test]
    fn shutup_release_is_wired_in_the_command_table() {
        let def = find("取消禁言").expect("@取消禁言 在 185 行表里");
        assert_eq!(def.imp, Imp::ShutupRelease);
        assert_eq!(def.perm_min, 10, "`[Permission] ReleaseShutup=10`");
        assert_eq!(
            def.params, 2,
            "`Execute(string[] @params, IPlayerActor)` ⇒ case 2"
        );
        // `Command.CommandHelp` = `命令格式: @{Name} {Help}`；`Name` 被 `[Command] ReleaseShutup`
        // 覆盖为 `取消禁言`（`Command.conf:67`），`Help` = `CommandHelp.cs:41` 的「人物名称」。
        assert_eq!(SHUTUP_RELEASE_HELP, "命令格式: @取消禁言 人物名称");
    }

    /// 恒真分支：无论带不带名字、`boAll` 是 `true` 还是 `false`，都**只回同一条**帮助文本。
    ///
    /// oracle `ShutupReleaseCommand.cs:20` 的 `IsNullOrEmpty(x) || !IsNullOrEmpty(x)` 恒真；
    /// `:22-55` 对 `DenySayMsgList` 的操作整段被注释 ⇒ `boAll` 只被注释代码引用。
    #[test]
    fn shutup_release_always_returns_the_same_help_text() {
        for line in [
            "@取消禁言",
            "@取消禁言 zhangsan",
            "@取消禁言 zhangsan true",
            "@取消禁言 zhangsan false",
            // 只有 ASCII 空格分段（`Split(' ')`）：制表符留在**同一段**里 ⇒ 没有第二参，
            // 不触发 `bool.Parse` ⇒ 同样落帮助文本。
            "@取消禁言 zhangsan	true",
        ] {
            let (r, _) = run_gm(line);
            assert!(r.found && r.parsed, "{line}: 命中命令体");
            // 与字面量比对（不用常量比常量）：文本一旦被改写，本用例必须红。
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, "命令格式: @取消禁言 人物名称")],
                "{line}: 恒真分支 ⇒ 恰好一条 Red/Hint 帮助文本"
            );
            assert!(r.frames.is_empty(), "{line}: 本命令只发 SysMsg");
            assert!(r.log_note.is_none(), "{line}: 正常路径不得有登记");
        }
    }

    /// 第二段不是 `true`/`false` ⇒ `bool.Parse` 抛 `FormatException`，被
    /// `PlayObject.Chat.cs:402-406` 的 catch 吞掉 ⇒ **零帧**（`found` 仍为 true，不走回落）。
    ///
    /// 含「内部连续空格」情形：`parse_command` 只 Trim 两端 ⇒ `params[1]` 是空串 ⇒ 同样抛。
    #[test]
    fn shutup_release_bool_parse_failure_is_zero_frames() {
        for line in [
            "@取消禁言 zhangsan yes",
            "@取消禁言 zhangsan 1",
            "@取消禁言 zhangsan TRUE2",
            "@取消禁言 zhangsan  true",
        ] {
            let (r, _) = run_gm(line);
            assert!(
                r.found && r.parsed,
                "{line}: 命令已命中（只是命令体抛异常）"
            );
            assert!(r.msgs.is_empty(), "{line}: 异常被 catch 吞掉 ⇒ 零帧");
            assert!(r.frames.is_empty(), "{line}: 无其它帧");
            assert_eq!(
                r.log_note,
                Some(NOTE_SHUTUP_RELEASE_BOOL_PARSE),
                "{line}: 吞异常路径必须登记"
            );
        }
        // 对照：空白被 `bool.Parse` 的 `Trim()` 吃掉的 ` true ` 仍是合法值 ⇒ 帮助文本。
        let (r, _) = run_gm("@取消禁言 zhangsan 	true ");
        assert_eq!(texts(&r), vec![(GmColor::Red, SHUTUP_RELEASE_HELP)]);
    }

    /// **F3（review `01007b5ee8`）**：帮助文本的**线上 GBK 字节**逐字节断言（与兄弟卡
    /// `shutup_sends_exactly_one_red_sysmsg_bytes` 同口径）。
    ///
    /// `SysMsg` 在线上按 GB2312/GBK 编码（`HUtil32.GetString` 语义）；UTF-8 文本相同不能
    /// 证明线上字节相同（例如「格」在 GBK 是 `B8F1`，UTF-8 是 `E6 A0 BC`）。牙：把常量里的
    /// 「式」改成同音异形字 ⇒ 本用例红。
    #[test]
    fn shutup_release_help_is_gbk_bytes_on_the_wire() {
        let (r, _) = run_gm("@取消禁言 zhangsan");
        assert!(r.found && r.parsed, "命中命令体，不走未知命令回落");
        assert_eq!(r.msgs.len(), 1, "恰好一条 SysMsg");
        assert_eq!(r.msgs[0].color, GmColor::Red);
        assert_eq!(r.msgs[0].text, SHUTUP_RELEASE_HELP);
        assert!(r.frames.is_empty(), "本命令只发 SysMsg，无其它帧");
        assert!(r.log_note.is_none(), "正常支没有吞异常/未接项");
        // 线上编码：逐字符 GBK 双字节 + 三个 ASCII（': '、'@'、' '）。
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0xc3, 0xfc, // 命
                0xc1, 0xee, // 令
                0xb8, 0xf1, // 格
                0xca, 0xbd, // 式
                0x3a, 0x20, // ": "
                0x40, // '@'
                0xc8, 0xa1, // 取
                0xcf, 0xfb, // 消
                0xbd, 0xfb, // 禁
                0xd1, 0xd4, // 言
                0x20, // ' '
                0xc8, 0xcb, // 人
                0xce, 0xef, // 物
                0xc3, 0xfb, // 名
                0xb3, 0xc6, // 称
            ]
        );
    }

    /// 权限门先于命令体（`GameCommand.cs:37-40`）：`Permission < 10` ⇒ 红字 `权限不够!!!`，
    /// 连帮助文本都不出。
    #[test]
    fn shutup_release_permission_gate_blocks_body() {
        let mut sess = GmSession {
            permission: 9,
            ..GmSession::default()
        };
        let cfg = GmConfig::default();
        let r = exec(&mut sess, &cfg, "@取消禁言 zhangsan");
        assert_eq!(texts(&r), vec![(GmColor::Red, "权限不够!!!")]);
        sess.permission = 10;
        let r = exec(&mut sess, &cfg, "@取消禁言 zhangsan");
        assert_eq!(texts(&r), vec![(GmColor::Red, SHUTUP_RELEASE_HELP)]);
    }

    /// 卡 `openmir2-gm-impl-shutup-release`：部署脚本路（`Market_Def/功能NPC/比武-GA2.txt:43`
    /// 的 `Gmexecute 取消禁言 self`）经 `NpcPlayer` 的 GMEXECUTE 跳线抬权执行后，
    /// 唯一可观测帧 = 帮助文本（oracle `ShutupReleaseCommand.cs:20` 的恒真分支）。
    ///
    /// 同时钉住 `bool.Parse` 怪癖：脚本写成 `取消禁言 新人 1` ⇒ 第二参不是 true/false
    /// ⇒ 命令体抛异常被 catch 吞掉 ⇒ **零帧**（连帮助文本都不出）。
    #[test]
    fn shutup_release_script_gmexecute_path_through_npc_player_seam() {
        use mir2_script::engine::ScriptPlayer;
        let guilds = mir2_world::GuildManager::new();
        let world = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        let mut sess = GmSession::default();
        // `ActionOfGmExecute` 同形：`@<cmd> <p1..p6>`，尾随空参产生 3 个空格。
        let s_data = format!("@{} {} {} {} {} {}", "取消禁言", "新人", "", "", "", "");
        assert_eq!(s_data, "@取消禁言 新人    ");
        let mut rng = |_v: i32| 0;
        {
            let mut player = crate::npc::NpcPlayer::empty(&mut rng);
            player.chr_name = "新人".to_string();
            player.gm = Some(crate::npc::NpcGm {
                sess: &mut sess,
                cfg: &cfg,
                world,
                pwd_busy: false,
            });
            player.set_permission(10);
            player.process_user_line_msg(&s_data);
            player.set_permission(0);
            assert_eq!(player.frames.len(), 1, "恒真分支 ⇒ 恰好一条 SysMsg");
            assert_eq!(
                player.frames[0].msg.as_deref(),
                Some(SHUTUP_RELEASE_HELP),
                "帮助文本逐字节（部署名 `取消禁言`）"
            );
            assert_eq!(
                (player.frames[0].n_param1, player.frames[0].n_param2),
                (0xFF, 0x38),
                "Red/Hint 的部署色词"
            );
            assert!(player.gm_direct_frames.is_empty(), "本命令无直发帧");
        }
        // `bool.Parse` 怪癖：第二参 `1` ⇒ 异常被吞 ⇒ 脚本路径**零帧**。
        let bad = format!("@{} {} {} {} {} {}", "取消禁言", "新人", "1", "", "", "");
        {
            let mut player = crate::npc::NpcPlayer::empty(&mut rng);
            player.chr_name = "新人".to_string();
            player.gm = Some(crate::npc::NpcGm {
                sess: &mut sess,
                cfg: &cfg,
                world,
                pwd_busy: false,
            });
            player.set_permission(10);
            player.process_user_line_msg(&bad);
            player.set_permission(0);
            assert!(player.frames.is_empty(), "bool.Parse 失败 ⇒ 零帧");
            assert!(player.gm_direct_frames.is_empty());
            assert!(
                player.gm_notes.iter().any(|n| n.contains("bool.Parse")),
                "吞异常路径必须登记：{:?}",
                player.gm_notes
            );
        }
        assert_eq!(sess.permission, 0, "抬权临时生效并还原");
    }

    /// 卡 `openmir2-gm-impl-startquest`：表项（现网名 `开始提问` = `conf[Command].StartQuest`）。
    #[test]
    fn start_quest_is_wired_in_the_command_table() {
        let def = find("开始提问").expect("@开始提问 在 185 行表里");
        assert_eq!(def.imp, Imp::StartQuest);
        assert_eq!(def.perm_min, 10, "`[Permission] StartQuest=10`");
        assert_eq!(def.params, 2);
        assert_eq!(START_QUEST_HELP, "命令格式: @开始提问 问答名称");
    }

    /// `:17-21` 空名 ⇒ 红字帮助；`:22` 的 `SendQuestMsg` 被注释 ⇒ 非空名零帧 + 登记。
    #[test]
    fn start_quest_help_and_commented_noop() {
        for line in ["@开始提问", "@开始提问  "] {
            let (r, _) = run_gm(line);
            assert_eq!(texts(&r), vec![(GmColor::Red, START_QUEST_HELP)], "{line}");
        }
        // GBK 线上字节：帮助文案必须逐字节等同 oracle（「文本相等」不等于「线上字节相等」；
        // 同族用例见 `@禁言`）。28 字节 = 命令格式(8) + ": @"(3) + 开始提问(8) + ' '(1) + 问答名称(8)。
        let (r, _) = run_gm("@开始提问");
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0xc3, 0xfc, 0xc1, 0xee, 0xb8, 0xf1, 0xca, 0xbd, // 命令格式
                0x3a, 0x20, 0x40, // ": @"
                0xbf, 0xaa, 0xca, 0xbc, 0xcc, 0xe1, 0xce, 0xca, // 开始提问
                0x20, // ' '
                0xce, 0xca, 0xb4, 0xf0, 0xc3, 0xfb, 0xb3, 0xc6, // 问答名称
            ]
        );
        for line in ["@开始提问 quiz", "@开始提问 第一题 extra"] {
            let (r, sess) = run_gm(line);
            assert!(
                r.msgs.is_empty(),
                "{line}: 非空名 ⇒ 零帧（SendQuestMsg 被注释）"
            );
            assert!(r.frames.is_empty());
            assert_eq!(r.log_note, Some(NOTE_START_QUEST_MSG_COMMENTED), "{line}");
            assert_eq!(sess.permission, 10, "{line}: 无副作用");
        }
        let mut sess = GmSession {
            permission: 9,
            ..GmSession::default()
        };
        let cfg = GmConfig::default();
        let r = exec(&mut sess, &cfg, "@开始提问 quiz");
        assert_eq!(texts(&r), vec![(GmColor::Red, "权限不够!!!")]);
    }

    /// 空名字 ⇒ oracle `:21-24` **静默 return**（零帧、无帮助文本），且不得留下踢人目标。
    #[test]
    fn kick_human_empty_name_is_silent_zero_frames() {
        for line in ["@VV踢人", "@VV踢人  "] {
            let (r, sess) = run_gm(line);
            assert!(r.found && r.parsed, "{line}: 命中命令体");
            assert!(r.msgs.is_empty(), "{line}: 静默 return ⇒ 零帧");
            assert!(r.frames.is_empty());
            assert!(sess.pending_kick.is_none(), "{line}: 不得登记踢人目标");
        }
    }

    /// 目标不在线 ⇒ 一条 Red/Hint `"{name} 现在不在线，或在其它服务器上!!!"`（`:32-38`）。
    #[test]
    fn kick_human_offline_target_reports_not_online() {
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        let mut e = mir2_world::Entity::new(0, "victim", 5, 5);
        e.race = mir2_world::ACTOR_RACE_PLAY;
        w.add_entity(e);
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
        let r = execute(&mut sess, &cfg, &view, "@VV踢人 nobody");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "nobody 现在不在线，或在其它服务器上!!!")]
        );
        // 线上编码（`gbk_encode`）后的字节逐字节锚定（复审 minor）：ASCII + GBK 双字节。
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0x6e, 0x6f, 0x62, 0x6f, 0x64, 0x79, 0x20, // "nobody "
                0xcf, 0xd6, // 现
                0xd4, 0xda, // 在
                0xb2, 0xbb, // 不
                0xd4, 0xda, // 在
                0xcf, 0xdf, // 线
                0xa3, 0xac, // ，
                0xbb, 0xf2, // 或
                0xd4, 0xda, // 在
                0xc6, 0xe4, // 其
                0xcb, 0xfc, // 它
                0xb7, 0xfe, // 服
                0xce, 0xf1, // 务
                0xc6, 0xf7, // 器
                0xc9, 0xcf, // 上
                0x21, 0x21, 0x21, // !!!
            ],
            "SysMsg 文本的 GBK 字节逐字节等价"
        );
        assert!(sess.pending_kick.is_none(), "未命中不得登记目标");
        // 大小写不敏感（`GetPlayObject` 的 `OrdinalIgnoreCase`）⇒ 命中 ⇒ 零帧 + 登记。
        let r = execute(&mut sess, &cfg, &view, "@VV踢人 VICTIM");
        assert!(r.msgs.is_empty(), "命中目标 ⇒ 命令无回帧");
        // 复审 important-1：登记的是实体规范名（`victim`），不是输入串（`VICTIM`）——
        // 宿主 `online_take` 精确匹配，存输入串会「说命中但踢不到」。
        assert_eq!(sess.pending_kick.as_deref(), Some("victim"));
    }

    /// 命中 ⇒ 零帧 + `pending_kick` = 目标名；`Ghost` 玩家按 oracle 口径**不算在线**
    /// （`GetPlayObject` 的 `!Ghost` 门）。
    #[test]
    fn kick_human_online_target_sets_pending_kick_without_frames() {
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        for (name, ghost) in [("alive", false), ("gone", true)] {
            let mut e = mir2_world::Entity::new(0, name, 5, 5);
            e.race = mir2_world::ACTOR_RACE_PLAY;
            e.ghost = ghost;
            w.add_entity(e);
        }
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_world(&w).with_speaker("gm");
        let r = execute(&mut sess, &cfg, &view, "@VV踢人 alive");
        assert!(r.msgs.is_empty(), "命中 ⇒ 无回帧");
        assert!(r.frames.is_empty());
        assert_eq!(sess.pending_kick.as_deref(), Some("alive"));
        // Ghost ⇒ `GetPlayObject` 返回 null ⇒ 走「不在线」支。
        let r = execute(&mut sess, &cfg, &view, "@VV踢人 gone");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "gone 现在不在线，或在其它服务器上!!!")]
        );
        assert!(sess.pending_kick.is_none());
    }

    /// 未注入世界 ⇒ 登记零帧（不伪造「不在线」），与 `@dsfsdt546` 等同一口径。
    #[test]
    fn kick_human_without_world_is_registered_no_world() {
        let (r, sess) = run_gm("@VV踢人 victim");
        assert!(r.msgs.is_empty());
        assert_eq!(r.log_note, Some(NOTE_NO_WORLD));
        assert!(sess.pending_kick.is_none());
    }

    /// 脚本路（部署 `Market_Def/功能NPC/桃源-N109.txt:49` 的 `Gmexecute VV踢人 <名>`）：
    /// 经 `NpcPlayer` 的 GMEXECUTE 跳线执行后，目标名落在**会话**缓冲上（宿主 tick 内收口）。
    #[test]
    fn kick_human_script_gmexecute_path_sets_pending_kick() {
        use mir2_script::engine::ScriptPlayer;
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        let mut e = mir2_world::Entity::new(0, "victim", 5, 5);
        e.race = mir2_world::ACTOR_RACE_PLAY;
        w.add_entity(e);
        let guilds = mir2_world::GuildManager::new();
        let view = GmWorld::new(&guilds).with_world(&w);
        let cfg = GmConfig::default();
        let mut sess = GmSession::default();
        // `ActionOfGmExecute` 同形：`@<cmd> <p1..p6>`，尾随空参产生 3 个空格。
        let s_data = format!("@{} {} {} {} {} {}", "VV踢人", "victim", "", "", "", "");
        let mut rng = |_v: i32| 0;
        {
            let mut player = crate::npc::NpcPlayer::empty(&mut rng);
            player.chr_name = "新人".to_string();
            player.gm = Some(crate::npc::NpcGm {
                sess: &mut sess,
                cfg: &cfg,
                world: view,
                pwd_busy: false,
            });
            player.set_permission(10);
            player.process_user_line_msg(&s_data);
            player.set_permission(0);
            assert!(player.frames.is_empty(), "命中目标 ⇒ 脚本侧零帧");
            assert!(player.gm_direct_frames.is_empty());
        }
        assert_eq!(sess.pending_kick.as_deref(), Some("victim"));
        assert_eq!(sess.permission, 0, "抬权临时生效并还原");
    }

    /// 帮助文本逐字节：`"命令格式: {0} {1} {2} {3} {4} {5} {6}"` 的七个槽 —— **没有 `@` 前缀**、
    /// 单空格分隔、空槽照原样（缺省全空 ⇒ 6 个尾随空格）。
    #[test]
    fn mob_fire_burn_empty_map_reports_help_with_trailing_slots() {
        let (r, _) = run_gm("@MobFireBurn");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: MobFireBurn      ")],
            "六个空槽 ⇒ 六个尾随空格"
        );
        // GBK 字节断言（复审 minor）：`命令格式` 8 字节 + `: MobFireBurn` + 6 个尾随空格。
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0xc3, 0xfc, 0xc1, 0xee, 0xb8, 0xf1, 0xca, 0xbd, 0x3a,
                0x20, // 命令格式:␠
                0x4d, 0x6f, 0x62, 0x46, 0x69, 0x72, 0x65, 0x42, 0x75, 0x72,
                0x6e, // MobFireBurn
                0x20, 0x20, 0x20, 0x20, 0x20, 0x20, // 六个尾随空格
            ]
        );
        // 第 2 个 UTF-16 码元是 `?` ⇒ 帮助（原始串原样回填）。
        let (r, _) = run_gm("@MobFireBurn m? 10 10 5 600 0");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: MobFireBurn m? 10 10 5 600 0")]
        );
    }

    /// **构造级分支**（不是部署形态）：单码元地图名 ⇒ oracle `:29` 的 `sMap[1]` 抛
    /// `IndexOutOfRangeException`，被 `PlayObject.Chat.cs` 的 catch 吞掉 ⇒ **零帧**。
    ///
    /// 复审 important-1：部署脚本传的是 `<$CURRENTMAP>` **字面量**（`ActionOfGmExecute` 不展开
    /// 动作参数，T-165），不是 `0`/`3` 这类单字符名 ⇒ 现网**不走**这条；部署形态见下一条用例。
    #[test]
    fn mob_fire_burn_single_char_map_is_zero_frames_construct_only() {
        for line in [
            "@MobFireBurn 3 329 329 3 60 0",
            "@MOBFIREBURN 0 330 330 5 600 0",
        ] {
            let (r, _) = run_gm(line);
            assert!(r.found && r.parsed, "{line}: 命中命令体");
            assert!(r.msgs.is_empty(), "{line}: `sMap[1]` 越界 ⇒ 零帧");
            assert!(r.frames.is_empty());
            assert_eq!(r.log_note, Some(NOTE_INDEX_OUT_OF_RANGE), "{line}");
        }
    }

    /// **部署形态**（复审 important-1）：`Gmexecute MOBFIREBURN <$CURRENTMAP> <$CURRENTX>
    /// <$CURRENTY> 5 600 0`。`ActionOfGmExecute` 直接拼 `sParam1..6`、不调
    /// `GetLineVariableText`（`ExecutionProcessingSys.cs:4933`；T-165 同源）⇒ 实参是字面量：
    /// `sMap` = `<$CURRENTMAP>`（13 码元，`sMap[1] == '$'`，不越界）；
    /// `StrToInt("<$CURRENTX>", -1) = -1` ⇒ 命中 `:43` 的 `nX < 0` ⇒ **一条红字帮助**
    /// （回填六个原始串，含 `<$…>` 字面量），不是「零帧」。
    #[test]
    fn mob_fire_burn_deployment_literals_report_help() {
        let line = "@MOBFIREBURN <$CURRENTMAP> <$CURRENTX> <$CURRENTY> 5 600 0";
        let (r, _) = run_gm(line);
        assert!(r.found && r.parsed, "命中命令体");
        assert_eq!(
            texts(&r),
            vec![(
                GmColor::Red,
                "命令格式: MobFireBurn <$CURRENTMAP> <$CURRENTX> <$CURRENTY> 5 600 0"
            )],
            "部署字面量 ⇒ 一条红字帮助（不是零帧）"
        );
        assert!(r.frames.is_empty(), "帮助支没有其它帧");
        // 线上 GBK 字节：帮助模板 + 六个字面量**原样**（`<$…>` 不被展开）。
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0xc3, 0xfc, 0xc1, 0xee, 0xb8, 0xf1, 0xca, 0xbd, 0x3a,
                0x20, // 命令格式:␠
                0x4d, 0x6f, 0x62, 0x46, 0x69, 0x72, 0x65, 0x42, 0x75, 0x72,
                0x6e, // MobFireBurn
                0x20, 0x3c, 0x24, 0x43, 0x55, 0x52, 0x52, 0x45, 0x4e, 0x54, 0x4d, 0x41, 0x50,
                0x3e, // <$CURRENTMAP>
                0x20, 0x3c, 0x24, 0x43, 0x55, 0x52, 0x52, 0x45, 0x4e, 0x54, 0x58, 0x3e,
                // <$CURRENTX>
                0x20, 0x3c, 0x24, 0x43, 0x55, 0x52, 0x52, 0x45, 0x4e, 0x54, 0x59, 0x3e,
                // <$CURRENTY>
                0x20, 0x35, 0x20, 0x36, 0x30, 0x30, 0x20, 0x30, // ␠5␠600␠0
            ],
            "部署形态帮助文本的 GBK 字节逐字节等价"
        );
    }

    /// 数值校验：`(short)`/`(byte)` unchecked 截断后任一 < 0 ⇒ 帮助（回填**原始串**）。
    #[test]
    fn mob_fire_burn_bad_coords_report_help() {
        // 非数字 X ⇒ `StrToInt(..., -1)` = -1 ⇒ 帮助。
        let (r, _) = run_gm("@MobFireBurn map abc 10 5 600 0");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: MobFireBurn map abc 10 5 600 0")]
        );
        // `(short)` 截断：40000 ⇒ -25536 ⇒ 负 ⇒ 帮助（C# unchecked 口径）。
        let (r, _) = run_gm("@MobFireBurn map 40000 10 5 600 0");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: MobFireBurn map 40000 10 5 600 0")]
        );
        // `(byte)` 截断：`sType` 非数字 ⇒ (byte)-1 = 255 ⇒ **不**触发 `nType < 0`（死条件）。
        // 时间非数字 ⇒ -1 ⇒ 帮助。
        let (r, _) = run_gm("@MobFireBurn map 10 10 zzz bad 0");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "命令格式: MobFireBurn map 10 10 zzz bad 0")]
        );
    }

    /// 地图命中 ⇒ `FireBurnEvent` 入 `EventMgr`（宿主未接）⇒ **零帧** + 登记；缺省 `nPoint`
    /// 为负 ⇒ 抬到 1 后仍合法（`nPoint < 0` 是死条件）。
    #[test]
    fn mob_fire_burn_existing_map_registers_event_and_stays_silent() {
        let f = map_fixture(mir2_envir::MapInfoFlag::default());
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_maps(&f.registry);
        for line in ["@MobFireBurn map 5 5 5 600 0", "@MobFireBurn map 5 5 5 600"] {
            let r = execute(&mut sess, &cfg, &view, line);
            assert!(r.msgs.is_empty(), "{line}: 命中地图 ⇒ 零帧");
            assert!(r.frames.is_empty());
            assert_eq!(r.log_note, Some(NOTE_EVENT_MGR), "{line}");
        }
    }

    /// 地图不存在 ⇒ `string.Format("地图{0} 不存在", Command.Name, sMap)` 的**单占位符怪癖**
    /// ⇒ 文案填的是**命令名**、地图名被丢弃（BUG-REPLICA）。
    #[test]
    fn mob_fire_burn_map_not_found_prints_command_name() {
        let f = map_fixture(mir2_envir::MapInfoFlag::default());
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let view = GmWorld::new(&guilds).with_maps(&f.registry);
        let r = execute(&mut sess, &cfg, &view, "@MobFireBurn nowhere 5 5 5 600 0");
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "地图MobFireBurn 不存在")],
            "模板只有 {{0}} ⇒ 地图名不入文案（oracle 原样）"
        );
        // GBK 字节断言（复审 minor）：`地图` 4 字节 + 命令名 + ` 不存在`。
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0xb5, 0xd8, 0xcd, 0xbc, // 地图
                0x4d, 0x6f, 0x62, 0x46, 0x69, 0x72, 0x65, 0x42, 0x75, 0x72,
                0x6e, // MobFireBurn
                0x20, 0xb2, 0xbb, 0xb4, 0xe6, 0xd4, 0xda, // ␠不存在
            ]
        );
        // 未接地图注册表 ⇒ 登记零帧（不伪造「地图不存在」）。
        let bare = GmWorld::new(&guilds);
        let r = execute(&mut sess, &cfg, &bare, "@MobFireBurn map 5 5 5 600 0");
        assert!(r.msgs.is_empty());
        assert_eq!(r.log_note, Some(NOTE_NO_MAP_REGISTRY));
    }

    /// 脚本路（部署 `Market_Def/QFunction-0.txt:156/163` 的 `@StdModeFunc15/16`：
    /// `Gmexecute MOBFIREBURN <$CURRENTMAP> <$CURRENTX> <$CURRENTY> 5 600 0`）经
    /// `NpcPlayer` 的 GMEXECUTE 跳线执行 ⇒ 实参是**字面量**（T-165）⇒ **一条红字帮助**。
    #[test]
    fn mob_fire_burn_script_gmexecute_path_reports_help() {
        use mir2_script::engine::ScriptPlayer;
        let guilds = mir2_world::GuildManager::new();
        let view = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        let mut sess = GmSession::default();
        // `ActionOfGmExecute` 同形：`@<cmd> <p1..p6>`，六参全给；部署实参是**字面量**
        // `<$CURRENTMAP>/<$CURRENTX>/<$CURRENTY>`（GMEXECUTE 不展开动作参数，T-165）。
        let s_data = "@MOBFIREBURN <$CURRENTMAP> <$CURRENTX> <$CURRENTY> 5 600 0";
        let mut rng = |_v: i32| 0;
        {
            let mut player = crate::npc::NpcPlayer::empty(&mut rng);
            player.chr_name = "新人".to_string();
            player.gm = Some(crate::npc::NpcGm {
                sess: &mut sess,
                cfg: &cfg,
                world: view,
                pwd_busy: false,
            });
            player.set_permission(10);
            player.process_user_line_msg(s_data);
            player.set_permission(0);
            assert_eq!(
                player.frames.len(),
                1,
                "部署字面量 ⇒ 脚本侧一条红字帮助（复审 important-1：不是零帧）"
            );
            assert_eq!(
                player.frames[0].msg.as_deref(),
                Some("命令格式: MobFireBurn <$CURRENTMAP> <$CURRENTX> <$CURRENTY> 5 600 0")
            );
            assert!(player.gm_direct_frames.is_empty());
            assert!(
                player.gm_notes.is_empty(),
                "帮助支不登记越界：{:?}",
                player.gm_notes
            );
        }
        assert_eq!(sess.permission, 0, "抬权临时生效并还原");
    }

    // ---------------------------- 族 P · 密码锁门（7，卡 `openmir2-gm-family-p-passwordlock`）----------------------------

    /// 族 P 的 7 条现网名全部在 `COMMANDS` 表且 **imp = [`Imp::PasswordLockGate`]、
    /// **perm 10**（`GameCommands.cs:381-388` 构造缺省；`[Permission]` 段对 7 条都无键）。
    /// 牙：任一条回退 `Unimplemented` / 改名 / 改权限 ⇒ 本用例红。
    #[test]
    fn password_lock_family_table_entries_are_wired() {
        for name in [
            "hjhgj67",
            "sadsa54",
            "ghjhg6",
            "hjhg556",
            "gfh678dfg",
            "fdgfdg56",
            "PasswordLock",
        ] {
            let def = find(name).unwrap_or_else(|| panic!("{name} 不在 COMMANDS 表"));
            assert_eq!(def.imp, Imp::PasswordLockGate, "{name} 应是密码锁门实现");
            assert_eq!(def.perm_min, 10, "{name} 的 PermissionMin");
            assert_eq!(def.params, 2, "{name} 的 Execute 形参个数");
        }
        // 出厂英文名不注册（`CommandMaps` 键 = 配置覆盖后的现网名；`PasswordLock=PasswordLock`
        // 不改名 ⇒ 两名同名，不在本对照里）。忽略大小写口径与 `CommandMaps` 同（Ordinal）。
        for factory in [
            "SetPassWord",
            "UnPassWord",
            "ChgpassWordCommand",
            "UnlockStorage",
            "UnLock",
            "Lock",
        ] {
            assert!(
                find(factory).is_none(),
                "{factory} 出厂名不该命中（conf 覆盖后只认现网名）"
            );
        }
    }

    /// **T1（卡 `openmir2-gm-family-p-passwordlock`）**：部署 `PasswordLockSystem=0`
    /// （`Server.conf:382`）⇒ 7 条命令 perm=10 抬权后**恰好一条** Red `SysMsg` 且正文
    /// 逐字节 = `NO_PASSWORD_LOCK_SYSTEM_MSG`（`MessageSettings.cs:99` / `String.conf:75`）；
    /// `found=true`（不走未知命令回落）、零额外帧、无吞异常登记。带参与不带参同答
    /// （`GameCommand.Handle` case 2 的 `@params` 与各命令体首句无关）。
    ///
    /// 变异红：删 [`password_lock_gate`] 的配置门 ⇒ 全体走 `log_note` 臂，`msgs` 空 ⇒ 红。
    #[test]
    fn password_lock_gate_deployment_reports_exactly_one_red_sysmsg() {
        let cfg = GmConfig::default();
        assert!(
            !cfg.password_lock_system,
            "部署缺省即门关（Server.conf:382）"
        );
        for name in [
            "hjhgj67",
            "sadsa54",
            "ghjhg6",
            "hjhg556",
            "gfh678dfg",
            "fdgfdg56",
            "PasswordLock",
        ] {
            for line in [format!("@{name}"), format!("@{name} 参数1 参数2")] {
                let (r, _) = run_gm_with(&cfg, &line);
                assert!(r.found && r.parsed, "{line} 必须命中命令体");
                assert_eq!(
                    texts(&r),
                    vec![(GmColor::Red, NO_PASSWORD_LOCK_SYSTEM_MSG)],
                    "{line}"
                );
                assert!(r.frames.is_empty(), "{line} 不得有额外帧");
                assert!(r.log_note.is_none(), "{line} 门红字支不登记 note");
            }
        }
        // `@PasswordLock` 的前置静默支是**死分支**：oracle `GameCommand.Handle`（case 2）恒
        // `parameters.Split(' ')` ⇒ `"".Split(' ')` 也长 1 ⇒ `@params.Length <= 0` 恒假。
        // 上面第二组循环已覆盖带参形态；这里补「不带参」单独断言与其它 6 条同答。
        let (r, _) = run_gm_with(&cfg, "@PasswordLock");
        assert_eq!(texts(&r), vec![(GmColor::Red, NO_PASSWORD_LOCK_SYSTEM_MSG)]);
    }

    /// **T1 线上字节面**：门红字的 GBK 编码逐字节断言（与 `shutup_release_help_is_gbk_bytes_
    /// on_the_wire` 同口径）——13 个汉字各一对 GBK + 三个 ASCII `!`(0x21)，与部署
    /// `String.conf:75` 的原始字节一致。牙：常量里任何一个字被换 ⇒ 红。
    #[test]
    fn password_lock_msg_is_gbk_bytes_on_the_wire() {
        let (r, _) = run_gm("@hjhgj67");
        assert_eq!(r.msgs.len(), 1, "恰好一条 SysMsg");
        assert_eq!(
            crate::downlink::gbk(&r.msgs[0].text),
            vec![
                0xd3, 0xce, // 游
                0xcf, 0xb7, // 戏
                0xc3, 0xdc, // 密
                0xc2, 0xeb, // 码
                0xb1, 0xa3, // 保
                0xbb, 0xa4, // 护
                0xcf, 0xb5, // 系
                0xcd, 0xb3, // 统
                0xbb, 0xb9, // 还
                0xc3, 0xbb, // 没
                0xd3, 0xd0, // 有
                0xc6, 0xf4, // 启
                0xd3, 0xc3, // 用
                0x21, 0x21, 0x21, // !!!
            ]
        );
    }

    /// **T2（卡 `openmir2-gm-family-p-passwordlock`）**：零副作用——执行前后整个
    /// [`GmSession`] 逐字段相等（除构造时抬的 `permission`），门关（部署缺省）与门开
    /// （`password_lock_system=true` 的不可达臂）都一样；门开时多一条 `log_note` 登记
    /// 启用分支未实现、但仍零帧零消息。零额外帧在 T1 已断言，这里补状态面。
    ///
    /// 变异红：伪实现启用分支（翻转任意会话位/发任何帧）⇒ 门开组红。
    #[test]
    fn password_lock_gate_zero_side_effects() {
        let closed = GmConfig::default();
        let open = GmConfig {
            password_lock_system: true,
            ..GmConfig::default()
        };
        for (label, cfg) in [("门关=部署", &closed), ("门开=不可达臂", &open)] {
            for name in [
                "hjhgj67",
                "sadsa54",
                "ghjhg6",
                "hjhg556",
                "gfh678dfg",
                "fdgfdg56",
                "PasswordLock",
            ] {
                for line in [format!("@{name}"), format!("@{name} 参数")] {
                    let (r, sess) = run_gm_with(cfg, &line);
                    assert_eq!(
                        sess,
                        GmSession {
                            permission: 10,
                            ..GmSession::default()
                        },
                        "{label} {line} 不得改任何会话状态"
                    );
                    assert!(r.frames.is_empty(), "{label} {line} 零额外帧");
                    assert_eq!(
                        r.log_note.is_some(),
                        cfg.password_lock_system,
                        "{label} {line}：只有门开的不可达臂登记 note"
                    );
                }
            }
        }
    }

    /// 权限门先于命令体（`GameCommand.cs:37-40`）：`Permission < 10` ⇒ 恰一条红字
    /// `权限不够!!!`，会话零变化——7 条全是 perm 10（`GameCommands.cs:381-388`）。
    #[test]
    fn password_lock_family_permission_gate_blocks_body() {
        for name in [
            "hjhgj67",
            "sadsa54",
            "ghjhg6",
            "hjhg556",
            "gfh678dfg",
            "fdgfdg56",
            "PasswordLock",
        ] {
            let (r, sess) = run(&format!("@{name}"));
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, PERMISSION_TOO_LOW)],
                "{name}"
            );
            assert!(r.frames.is_empty(), "{name}");
            assert_eq!(
                sess,
                GmSession::default(),
                "{name} 被权限门挡下后零状态变化"
            );
        }
    }

    // ---------------------------- 族 T · 会话开关/骑马（7，卡 `openmir2-gm-family-t-toggle-horse`）----------------------------

    /// 族 T 的 7 条现网名全部在 `COMMANDS` 表且 **imp = 族 T 对应变体**、**perm 10**、
    /// **params 1**（oracle `Execute(IPlayerActor)` 单参 ⇒ `GameCommand.Handle` case 1；
    /// perm = `GameCommands.cs:366-368/370/399-400/460` 构造缺省，`[Permission]` 段只有
    /// `DisableFilter=10` 与缺省同值）。牙：任一条回退 `Unimplemented` / 改名 / 改权限 ⇒ 红。
    #[test]
    fn t_family_table_entries_are_wired() {
        for (name, imp) in [
            ("交易", Imp::LetTrade),
            ("加入门派", Imp::LetGuild),
            ("拒绝组队聊天", Imp::LetShout),
            ("禁止行会信息", Imp::BanGuildChat),
            ("OffHorse", Imp::TakeOffHorse),
            ("OnHorse", Imp::TakeOnHorse),
            ("DisableFilter", Imp::DisableFilter),
        ] {
            let def = find(name).unwrap_or_else(|| panic!("{name} 不在 COMMANDS 表"));
            assert_eq!(def.imp, imp, "{name} 的实现标签");
            assert_eq!(def.perm_min, 10, "{name} 的 PermissionMin");
            assert_eq!(def.params, 1, "{name} 的 Execute 形参个数（单参 case 1）");
        }
        // 出厂英文名不注册（`CommandMaps` 键 = 配置覆盖后的现网名；`DisableFilter` 不改名）。
        for factory in [
            "LetTrade",
            "LetGuild",
            "LetShout",
            "BanGuildChat",
            "TakeOnHorse",
            "TakeOffHorse",
        ] {
            assert!(
                find(factory).is_none(),
                "{factory} 出厂名不该命中（conf 覆盖后只认现网名）"
            );
        }
    }

    /// **A1（卡 `openmir2-gm-family-t-toggle-horse`）**：四条 toggle 从 ctor 缺省出发翻两次
    /// **回到初值**，绿字逐字节（`CommandHelp.cs:114-121`）：
    /// `@交易` [禁止交易]→[允许交易]（ctor AllowDeal=true）；`@加入门派` [允许加入行会]→
    /// [禁止加入行会]（ctor false）；`@拒绝组队聊天` [禁止群聊]→[允许群聊]（ctor BanShout=true，
    /// **常量名与状态语义相反**照抄）；`@禁止行会信息` [禁止行会聊天]→[允许行会聊天]（ctor true）。
    /// 命中即零帧、无 note。变异红：删翻转 / 绿字二选一条件写反 ⇒ 序列断言红。
    #[test]
    fn t_family_toggles_round_trip_with_byte_exact_texts() {
        // (命令, 字段 getter, 第一翻预期, 第二翻预期)
        for (name, get, first, second) in [
            (
                "@交易",
                (|s: &GmSession| s.allow_deal) as fn(&GmSession) -> bool,
                DISABLE_DEAL_MSG,
                ENABLE_DEAL_MSG,
            ),
            (
                "@加入门派",
                (|s: &GmSession| s.allow_guild) as fn(&GmSession) -> bool,
                ENABLE_JOIN_GUILD,
                DISABLE_JOIN_GUILD,
            ),
            (
                "@拒绝组队聊天",
                (|s: &GmSession| s.ban_shout) as fn(&GmSession) -> bool,
                DISABLE_SHOUT_MSG,
                ENABLE_SHOUT_MSG,
            ),
            (
                "@禁止行会信息",
                (|s: &GmSession| s.ban_guild_chat) as fn(&GmSession) -> bool,
                DISABLE_GUILD_CHAT,
                ENABLE_GUILD_CHAT,
            ),
        ] {
            // 单会话连敲两次（oracle `PlayObject` 同生命周期字段；`run_gm` 每次新建会话 ⇒ 用 `exec`）。
            let mut sess = GmSession {
                permission: 10,
                ..GmSession::default()
            };
            let initial = get(&sess);
            let r1 = exec(&mut sess, &GmConfig::default(), name);
            assert!(r1.found && r1.parsed, "{name} 必须命中命令体");
            assert_eq!(texts(&r1), vec![(GmColor::Green, first)], "{name} 第一翻");
            assert!(r1.frames.is_empty() && r1.log_note.is_none(), "{name}");
            assert_eq!(get(&sess), !initial, "{name} 第一翻应翻转");
            let r2 = exec(&mut sess, &GmConfig::default(), name);
            assert_eq!(texts(&r2), vec![(GmColor::Green, second)], "{name} 第二翻");
            // A1：两次翻回初值。
            assert_eq!(get(&sess), initial, "{name} 翻两次应回初值");
            assert!(r2.frames.is_empty() && r2.log_note.is_none(), "{name}");
        }
    }

    /// 带参不影响 toggle（`Execute(IPlayerActor)` 单参 ⇒ `GameCommand.Handle` case 1 根本
    /// 不拆参数串）：`@交易 任意参数` 与 `@交易` 同答、同状态面。
    #[test]
    fn t_family_toggles_ignore_parameters() {
        for line in ["@交易 abc", "@交易 ", "@加入门派 x y"] {
            let (r, _) = run_gm(line);
            assert!(r.found && r.parsed, "{line}");
            assert_eq!(r.msgs.len(), 1, "{line} 恰一条绿字");
            assert_eq!(r.msgs[0].color, GmColor::Green, "{line}");
        }
    }

    /// **A2（卡 `openmir2-gm-family-t-toggle-horse`）**：`@OffHorse`——未骑马（ctor）
    /// **静默零帧**、会话零变化（`TakeOffHorseCommand.cs:12-14` 的 `if (!OnHorse) return;`）；
    /// 构造骑马态 ⇒ `OnHorse=false` + **外观帧标记**（`pending_feature_changed`，宿主广播
    /// `SM_FEATURECHANGED(41)`）+ 零 SysMsg（oracle 下马支没有文本）；再敲一次回静默。
    /// 变异红：丢 `pending_feature_changed` 置位 ⇒ 本用例红。
    #[test]
    fn t_family_off_horse_silent_when_unmounted_and_frames_when_mounted() {
        // 未骑马（ctor）：静默零帧零副作用。
        let (r, sess) = run_gm("@OffHorse");
        assert!(r.found && r.parsed);
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "未骑马应静默");
        assert!(r.log_note.is_none());
        assert_eq!(
            sess,
            GmSession {
                permission: 10,
                ..GmSession::default()
            },
            "未骑马零状态变化"
        );
        // 骑马态：翻转 + 外观帧标记。
        let mut sess = GmSession {
            permission: 10,
            on_horse: true,
            ..GmSession::default()
        };
        let r = exec(&mut sess, &GmConfig::default(), "@OffHorse");
        assert!(r.msgs.is_empty(), "下马支 oracle 无文本");
        assert!(!sess.on_horse, "OnHorse 应翻为 false");
        assert!(
            sess.pending_feature_changed,
            "FeatureChanged 外观帧必须登记 pending（缺失 ⇒ 宿主不广播 41）"
        );
        assert!(r.found && r.parsed);
        // 已下马：再敲回静默、无重复标记。
        let r = exec(&mut sess, &GmConfig::default(), "@OffHorse");
        assert!(r.msgs.is_empty());
        assert!(!sess.pending_feature_changed, "第二次已静默，不重置标记");
    }

    /// **A2**：`@OnHorse` 三支——① 已骑马（构造）静默零帧；② `HorseType=0`（现网恒值，
    /// oracle 全仓无赋值点）红字「骑马必须先戴上马牌!!!」逐字节；③ 构造 `HorseType≠0`
    /// 上马：`OnHorse=true` + 外观帧标记 + `@OnHorse` 脚本跳线以 `log_note` 登记接缝
    /// （卡面 forbidden：脚本面归脚本卡）。变异红：丢 pending 置位 / 删红字门 ⇒ 本用例红。
    #[test]
    fn t_family_on_horse_three_branches() {
        // ① 已骑马：静默（`if (OnHorse) return;`）。
        let mut sess = GmSession {
            permission: 10,
            on_horse: true,
            ..GmSession::default()
        };
        let r = exec(&mut sess, &GmConfig::default(), "@OnHorse");
        assert!(r.msgs.is_empty() && r.log_note.is_none(), "已骑马应静默");
        assert!(!sess.pending_feature_changed, "已骑马不登记帧");
        // ② HorseType=0（ctor）：红字唯一可达支。
        let (r, sess) = run_gm("@OnHorse");
        assert_eq!(texts(&r), vec![(GmColor::Red, ON_HORSE_NEED_TAG)]);
        assert!(
            !sess.on_horse && !sess.pending_feature_changed,
            "红字支零副作用"
        );
        // ③ 上马支（构造 HorseType≠0；现网不可达，支结构 1:1）。
        let mut sess = GmSession {
            permission: 10,
            horse_type: 5,
            ..GmSession::default()
        };
        let r = exec(&mut sess, &GmConfig::default(), "@OnHorse");
        assert!(r.msgs.is_empty(), "上马支 oracle 无 SysMsg");
        assert!(sess.on_horse, "应翻为 true");
        assert!(sess.pending_feature_changed, "外观帧必须登记 pending");
        let note = r.log_note.expect("上马支必须登记 @OnHorse 跳线接缝");
        assert!(
            note.contains("@OnHorse") && note.contains("GotoLable"),
            "{note}"
        );
        // 上马后再敲：已骑马静默、无重复标记。
        let r = exec(&mut sess, &GmConfig::default(), "@OnHorse");
        assert!(r.msgs.is_empty() && r.log_note.is_none());
        assert!(!sess.pending_feature_changed, "第二次已静默");
    }

    /// **A3（卡 `openmir2-gm-family-t-toggle-horse`）**：`DisableFilter` 翻的是**全局位**
    /// （`SystemShare.FilterWord`，进程单例）——注入共享 `Cell`：跨**两个会话**同一份
    /// （部署启动 true ⇒ 第一条回「已禁止文字过滤。」）；翻两次回初值；未注入（退化形态）
    /// ⇒ `log_note` 登记零帧、Cell 不可达。变异红：删全局翻转（只回文本不写位）⇒ 本用例红。
    #[test]
    fn t_family_disable_filter_flips_shared_global_bit() {
        let filter_word = std::cell::Cell::new(true); // 部署启动值（GameApp.cs:70）
        let guilds = mir2_world::GuildManager::new();
        let world = GmWorld::new(&guilds).with_filter_word(&filter_word);
        let mut sess_a = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        // 会话 A 第一敲：全局 true→false ⇒ 「已禁止文字过滤。」。
        let r = execute(&mut sess_a, &GmConfig::default(), &world, "@DisableFilter");
        assert_eq!(texts(&r), vec![(GmColor::Green, DISABLE_FILTER_DISABLED)]);
        assert!(!filter_word.get(), "全局位应翻为 false");
        // **跨会话同一份**：会话 B 看到的是 A 翻过的全局（oracle SystemShare 单例语义）。
        let mut sess_b = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        let r = execute(&mut sess_b, &GmConfig::default(), &world, "@DisableFilter");
        assert_eq!(texts(&r), vec![(GmColor::Green, DISABLE_FILTER_ENABLED)]);
        assert!(filter_word.get(), "第二敲应翻回 true");
        // A1 口径：翻两次回初值；会话态零变化（命令不写任何 PlayObject 字段）。
        assert_eq!(sess_a, sess_b, "命令体不写会话状态");
        assert_eq!(
            sess_a,
            GmSession {
                permission: 10,
                ..GmSession::default()
            }
        );
        // 退化形态：未注入全局单元（如脚本 GMEXECUTE 桥）⇒ 登记零帧，不伪造翻转。
        let bare = GmWorld::new(&guilds);
        let mut sess = GmSession {
            permission: 10,
            ..GmSession::default()
        };
        let r = execute(&mut sess, &GmConfig::default(), &bare, "@DisableFilter");
        assert!(r.msgs.is_empty(), "未注入 ⇒ 零帧登记");
        let note = r.log_note.expect("未注入必须登记，不许静默");
        assert!(note.contains("FilterWord"), "{note}");
    }

    /// 族 T 权限门（`GameCommand.cs:37-40`）：`Permission < 10` ⇒ 恰一条红字 `权限不够!!!`，
    /// 会话零变化——7 条全 perm 10。
    #[test]
    fn t_family_permission_gate_blocks_body() {
        for name in [
            "交易",
            "加入门派",
            "拒绝组队聊天",
            "禁止行会信息",
            "OffHorse",
            "OnHorse",
            "DisableFilter",
        ] {
            let (r, sess) = run(&format!("@{name}"));
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, PERMISSION_TOO_LOW)],
                "{name}"
            );
            assert!(r.frames.is_empty(), "{name}");
            assert_eq!(sess, GmSession::default(), "{name} 零状态变化");
        }
    }

    // ---------------------------- 族 Q · 查询/展示（10，卡 `openmir2-gm-family-q-query-display`）----------------------------

    /// 族 Q 夹具：`map_fixture`（图名 `map`/描述 `测试地图`/20×20，默认 Flag）+ 双玩家世界，
    /// 返回 (fixture, world, gm_id, bob_id)。**只读命令**：命令体不得改任何状态（卡面 forbidden）。
    fn q_fixture() -> (MapFixture, mir2_world::World, i32, i32) {
        let fx = map_fixture(mir2_envir::MapInfoFlag::default());
        let mut w = mir2_world::World::with_maps(vec![(20, 20)]);
        let gm_id = w.add_entity(mir2_world::Entity::new(0, "gm", 5, 5));
        let bob_id = w.add_entity(mir2_world::Entity::new(0, "bob", 6, 6));
        (fx, w, gm_id, bob_id)
    }

    fn q_view<'a>(
        fx: &'a MapFixture,
        w: &'a mir2_world::World,
        guilds: &'a mir2_world::GuildManager,
    ) -> GmWorld<'a> {
        GmWorld::new(guilds)
            .with_maps(&fx.registry)
            .with_world(w)
            .with_speaker("gm")
    }

    /// 10 条全部接线：现网名/perm 10/params（8 条 case 2 双参 + `MapInfo`/`LotteryTicket`
    /// case 1 单参）；出厂名不注册（含 LotteryTicket 的出厂默认 `Namecolor`）。
    /// 牙：任一条回退 `Unimplemented` / 改名 / 改权限 ⇒ 红。
    #[test]
    fn q_family_table_entries_are_wired() {
        for (name, imp, params) in [
            ("HumanCountwa", Imp::HumanCount, 2),
            ("MobCountwa", Imp::MobCount, 2),
            ("Map", Imp::MapInfo, 2),
            ("MapInfo", Imp::ShowMapInfo, 1),
            ("ShowMapMode", Imp::ShowMapMode, 2),
            ("showflag", Imp::ShowFlag, 2),
            ("IP", Imp::HumanLocal, 2),
            ("信息", Imp::HumanInfo, 2),
            ("Showuseiteminfo", Imp::ShowUseItemInfo, 2),
            ("LotteryTicket", Imp::LotteryTicket, 1),
        ] {
            let def = find(name).unwrap_or_else(|| panic!("{name} 不在 COMMANDS 表"));
            assert_eq!(def.imp, imp, "{name} 的实现标签");
            assert_eq!(def.perm_min, 10, "{name} 的 PermissionMin");
            assert_eq!(def.params, params, "{name} 的 Execute 形参个数");
        }
        for factory in [
            "HumanCount",
            "MobCount",
            "ShowHumanFlag",
            "HumanLocal",
            "HumanInfo",
            "ShowUseItem",
            "ShowMapInfo",
            "Namecolor",
        ] {
            assert!(
                find(factory).is_none(),
                "{factory} 出厂名不该命中（conf 覆盖后只认现网名）"
            );
        }
    }

    /// **A6**：`@HumanCountwa`/`@MobCountwa`——空参红字帮助、图不存在红字
    /// `指定的地图不存在!!!`（HumanCount/MobCount **共用**这一条文本）、命中绿字
    /// `怪物数量：{n}`（**全角冒号**；HumanCount 复用 MobCount 文案计**玩家**数——oracle 原样）。
    /// 死亡/Ghost 排除；怪物按 `race >= ACTOR_RACE_ANIMAL` 计。
    #[test]
    fn q_family_counts_help_not_found_and_green_counts() {
        let (fx, mut w, gm_id, bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let exec_at = |w: &mir2_world::World,
                       fx: &MapFixture,
                       guilds: &mir2_world::GuildManager,
                       sess: &mut GmSession,
                       line: &str|
         -> GmResult {
            let view = q_view(fx, w, guilds);
            execute(sess, &cfg, &view, line)
        };
        // 空参 ⇒ 各自红字帮助（两个现网名两份帮助文本）。
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@HumanCountwa")),
            vec![(GmColor::Red, Q_HELP_HUMAN_COUNT)]
        );
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@MobCountwa")),
            vec![(GmColor::Red, Q_HELP_MOB_COUNT)]
        );
        // 图不存在 ⇒ 红字（两类共用）。
        assert_eq!(
            texts(&exec_at(
                &w,
                &fx,
                &guilds,
                &mut sess,
                "@HumanCountwa nowhere"
            )),
            vec![(GmColor::Red, MOB_COUNT_MAP_NOT_FOUND)]
        );
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@MobCountwa nowhere")),
            vec![(GmColor::Red, MOB_COUNT_MAP_NOT_FOUND)]
        );
        // 命中：gm+bob 两个 Play 实体 ⇒ 玩家数 2；怪物 0。
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@HumanCountwa map")),
            vec![(GmColor::Green, "怪物数量：2")]
        );
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@MobCountwa map")),
            vec![(GmColor::Green, "怪物数量：0")]
        );
        // 怪物实体：race >= ACTOR_RACE_ANIMAL ⇒ MobCount +1；HumanCount 不变。
        let mut mon = mir2_world::Entity::new(0, "怪甲", 7, 7);
        mon.race = mir2_world::ACTOR_RACE_MONSTER;
        w.add_entity(mon);
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@MobCountwa map")),
            vec![(GmColor::Green, "怪物数量：1")]
        );
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@HumanCountwa map")),
            vec![(GmColor::Green, "怪物数量：2")]
        );
        // 死亡/Ghost 排除：bob 死亡 + gm Ghost ⇒ 玩家数 0；怪甲存活 ⇒ 怪物数仍 1；
        // 怪乙 Ghost ⇒ 不计。
        w.entities.get_mut(bob_id).unwrap().death = true;
        w.entities.get_mut(gm_id).unwrap().ghost = true;
        let mut mon2 = mir2_world::Entity::new(0, "怪乙", 8, 8);
        mon2.race = mir2_world::ACTOR_RACE_MONSTER;
        let mon2_id = w.add_entity(mon2);
        w.entities.get_mut(mon2_id).unwrap().ghost = true;
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@HumanCountwa map")),
            vec![(GmColor::Green, "怪物数量：0")]
        );
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@MobCountwa map")),
            vec![(GmColor::Green, "怪物数量：1")]
        );
    }

    /// **A3**：`@Map`（MapInfoCommand）三怪癖——① 实参 <3 ⇒ `@params[2]` 直取越界被吞
    /// ⇒ **零帧**（有 note 登记）；② 图不存在/格子无效（界内 LowWall）⇒ **静默**（无 else，
    /// 零帧零 note）；界外 ⇒ `IsValidCell` 仍 true ⇒ `GetCellInfo` 失败 ⇒ **红字**失败文本；
    /// ③ `nX<0/nY<0/图名空` ⇒ **绿**字格式提示（Name=现网名 `Map`）。命中 ⇒ 绿字
    /// `标志: Walk`（success 门要求 Valid）+ 有对象再补绿字 `对象数: N`。
    /// 变异红：M3（越界支改安全取参）⇒ 本用例红。
    #[test]
    fn q_family_map_param_quirks() {
        let (fx, mut w, _gm_id, _bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        let exec_at = |w: &mir2_world::World,
                       fx: &MapFixture,
                       guilds: &mir2_world::GuildManager,
                       sess: &mut GmSession,
                       line: &str|
         -> GmResult {
            let view = q_view(fx, w, guilds);
            execute(sess, &cfg, &view, line)
        };
        // ① 越界零帧（0/1/2 个实参都一样：oracle 第一句就直取 [2]）。
        for line in ["@Map", "@Map 5", "@Map 5 5"] {
            let r = exec_at(&w, &fx, &guilds, &mut sess, line);
            assert!(r.found && r.parsed, "{line}");
            assert!(r.msgs.is_empty() && r.frames.is_empty(), "{line} 应零帧");
            assert!(r.log_note.is_some(), "{line} 越界必须登记");
        }
        // ② 图不存在 ⇒ 静默零帧零 note。
        let r = exec_at(&w, &fx, &guilds, &mut sess, "@Map 1 1 nowhere");
        assert!(r.msgs.is_empty() && r.log_note.is_none(), "图不存在应静默");
        // ②' 界内 LowWall ⇒ GetCellInfo success=false ⇒ **红字**失败（oracle IsValidCell
        // 的 false 臂不可达：success ⇒ Walk ⇒ 永非 LowWall；静默只由图不存在到达）。
        w.maps
            .get_mut(0)
            .expect("map 0")
            .set_map_xy_flag(12, 12, false);
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@Map 12 12 map")),
            vec![(GmColor::Red, "取地图单元信息失败: map")]
        );
        // ②'' 界外（20×20 图取 500,500）⇒ IsValidCell true（越界不算 LowWall）⇒ 红字失败。
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@Map 500 500 map")),
            vec![(GmColor::Red, "取地图单元信息失败: map")]
        );
        // ③ 负坐标 ⇒ 绿字格式提示（不是红字）。
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@Map -1 5 map")),
            vec![(GmColor::Green, "请按正确格式输入: Map 地图号 X Y")]
        );
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@Map 5 -1 map")),
            vec![(GmColor::Green, "请按正确格式输入: Map 地图号 X Y")]
        );
        // ① 命中：空格（10,10 无人）⇒ 恰一条绿字 `标志: Walk`（无对象数）。
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@Map 10 10 map")),
            vec![(GmColor::Green, "标志: Walk")]
        );
        // ①' 有对象 ⇒ 两条绿字（标志 + 对象数）。
        w.maps
            .get_mut(0)
            .expect("map 0")
            .add_map_object(5, 5, mir2_world::CellType::Play, 777, 0);
        assert_eq!(
            texts(&exec_at(&w, &fx, &guilds, &mut sess, "@Map 5 5 map")),
            vec![
                (GmColor::Green, "标志: Walk"),
                (GmColor::Green, "对象数: 1")
            ]
        );
    }

    /// **A6**：`@MapInfo`（说话者当前图两条绿字）与 `@ShowMapMode`（不存在红字 / 命中蓝字
    /// `GetEnvirInfo` 长串——默认 Flag 全 FALSE/0、`NORECONNECT:FALSE()` 空括号、
    /// `EXPRATE:FALSE(0)` 整除）逐字节。
    #[test]
    fn q_family_show_map_info_and_mode() {
        let (fx, w, _gm_id, _bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let view = q_view(&fx, &w, &guilds);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // MapInfo：两条绿字，顺序固定。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@MapInfo")),
            vec![
                (GmColor::Green, "地图名称: map(测试地图)"),
                (GmColor::Green, "地图大小: X(20) Y(20)")
            ]
        );
        // 说话者不在世界 ⇒ NOTE_NO_WORLD 登记零帧（构造缺口，不伪造当前图）。
        let bare_view = GmWorld::new(&guilds).with_maps(&fx.registry);
        let r = execute(&mut sess, &cfg, &bare_view, "@MapInfo");
        assert!(r.msgs.is_empty());
        assert!(r.log_note.is_some());
        // ShowMapMode：空参红帮助 / 不存在红字 / 命中蓝字长串。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@ShowMapMode")),
            vec![(GmColor::Red, Q_HELP_SHOW_MAP_MODE)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@ShowMapMode nowhere")),
            vec![(GmColor::Red, "nowhere 不存在!!!")]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@ShowMapMode map")),
            vec![(
                GmColor::Blue,
                "地图模式: Map:map(测试地图) DAY:FALSE DARK:FALSE SAFE:FALSE FIGHT:FALSE \
                 FIGHT3:FALSE QUIZ:FALSE NORECONNECT:FALSE() MUSIC:FALSE(0) EXPRATE:FALSE(0) \
                 PKWINLEVEL:FALSE(0) PKLOSTLEVEL:FALSE(0) PKWINEXP:FALSE(0) PKLOSTEXP:FALSE(0) \
                 DECHP:FALSE(0/0) INCHP:FALSE(0/0) DECGAMEGOLD:FALSE(0/0) INCGAMEGOLD:FALSE(0/0) \
                 INCGAMEPOINT:FALSE(0/0) RUNHUMAN:FALSE RUNMON:FALSE NEEDHOLE:FALSE NORECALL:FALSE \
                 NOGUILDRECALL:FALSE NODEARRECALL:FALSE NOMASTERRECALL:FALSE NODRUG:FALSE \
                 MINE:FALSE MINE2:FALSE NODROPITEM:FALSE NOTHROWITEM:FALSE NOPOSITIONMOVE:FALSE \
                 NOHORSE:FALSE NOHUMNOMON:FALSE NOCHAT:FALSE"
            )]
        );
    }

    /// **A1 + A2**：`@IP` **判空恒真怪癖**——任何输入（空/有参/问号/在线名）同答红字帮助；
    /// 变异红：M1 改回正常判空 ⇒ 非「帮助」支可达 ⇒ 本用例红。
    /// `@信息`：空名/`?` 名红帮助、离线红格式串、**在线零帧**（SysMsg 被注释）；
    /// 变异红：M2 在线支补发文本 ⇒ 本用例红。
    #[test]
    fn q_family_ip_always_help_and_info_online_silent() {
        let (fx, w, _gm_id, _bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let view = q_view(&fx, &w, &guilds);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // A1：恒红帮助。
        for line in ["@IP", "@IP bob", "@IP ?", "@IP 任何输入"] {
            assert_eq!(
                texts(&execute(&mut sess, &cfg, &view, line)),
                vec![(GmColor::Red, Q_HELP_HUMAN_LOCAL)],
                "{line} 恒真判空 ⇒ 同答帮助"
            );
            let r = execute(&mut sess, &cfg, &view, line);
            assert!(r.frames.is_empty() && r.log_note.is_none(), "{line}");
        }
        // A2：信息四支。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@信息")),
            vec![(GmColor::Red, Q_HELP_HUMAN_INFO)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@信息 ?gm")),
            vec![(GmColor::Red, Q_HELP_HUMAN_INFO)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@信息 nowhere")),
            vec![(GmColor::Red, "nowhere 现在不在线，或在其它服务器上!!!")]
        );
        // 在线 ⇒ 零帧零状态（SysMsg 被注释 ⇒ 1:1 静默）。
        let r = execute(&mut sess, &cfg, &view, "@信息 bob");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "在线应零帧");
        assert!(r.log_note.is_none(), "被注释支不是未接项");
        assert_eq!(
            sess,
            GmSession {
                permission: 10,
                ..GmSession::default()
            },
            "查询命令零状态（卡面 forbidden）"
        );
    }

    /// **A4**：`@Showuseiteminfo` 判空下标怪癖——单字符名（`sHumanName[1]` 越界）⇒ 零帧；
    /// `a?`（[1]=='?'）红帮助；`?a`（[1]=='a'）不命中帮助 ⇒ 走在线查找 ⇒ 离线红；
    /// 在线空佩戴位零帧；非空位逐件**字面**蓝字 `%s[%s]IDX[%d]系列号[%d]持久[%d-%d]`
    /// （.NET 无 `{n}` 占位 ⇒ 原样输出，Index==0 的槽跳过）。变异红：M4 改 [1]→[0] 或
    /// 字面改真替换 ⇒ 本用例红。
    #[test]
    fn q_family_show_use_item_info_quirks() {
        let (fx, mut w, _gm_id, bob_id) = q_fixture();
        {
            let bob = w.entities.get_mut(bob_id).unwrap();
            bob.use_items[3].index = 15;
            bob.use_items[3].make_index = 12345;
            bob.use_items[3].dura = 100;
            bob.use_items[3].dura_max = 200;
            bob.use_items[7].index = 42; // 第二件
            bob.use_items[9].index = 0; // Index==0 ⇒ 跳过
        }
        let guilds = mir2_world::GuildManager::new();
        let view = q_view(&fx, &w, &guilds);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // 空名红帮助。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@Showuseiteminfo")),
            vec![(GmColor::Red, Q_HELP_SHOW_USE_ITEM)]
        );
        // 单字符名 ⇒ 越界零帧（有 note 登记）。
        let r = execute(&mut sess, &cfg, &view, "@Showuseiteminfo a");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "单字符名应零帧");
        assert!(r.log_note.is_some(), "越界必须登记");
        // `a?`：[1]=='?' ⇒ 红帮助。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@Showuseiteminfo a?")),
            vec![(GmColor::Red, Q_HELP_SHOW_USE_ITEM)]
        );
        // `?a`：[1]=='a' ⇒ 不命中帮助 ⇒ 在线查找落空 ⇒ 离线红。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@Showuseiteminfo ?a")),
            vec![(GmColor::Red, "?a 现在不在线，或在其它服务器上!!!")]
        );
        // 在线 bob：2 个非空位 ⇒ 恰 2 条**字面**蓝字（逐件同文）。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@Showuseiteminfo bob")),
            vec![
                (GmColor::Blue, Q_SHOW_USE_ITEM_LITERAL),
                (GmColor::Blue, Q_SHOW_USE_ITEM_LITERAL)
            ]
        );
        let r = execute(&mut sess, &cfg, &view, "@Showuseiteminfo bob");
        assert!(r.log_note.is_none(), "字面输出不是未接项");
    }

    /// **A5 + A6**：`@showflag` 红绿回执（空/`?` 帮助、离线红格式串、在线 OFF 绿字——
    /// 目标位阵恒零值模型）+ `@LotteryTicket` 绿字 8 配置值（部署全 0 + 构造非 0 按序，
    /// 尾随空格逐字节）。
    #[test]
    fn q_family_showflag_and_lottery() {
        let (fx, w, _gm_id, _bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let view = q_view(&fx, &w, &guilds);
        let mut sess = gm_session();
        // showflag 四支。
        assert_eq!(
            texts(&execute(
                &mut sess,
                &GmConfig::default(),
                &view,
                "@showflag"
            )),
            vec![(GmColor::Red, Q_HELP_SHOW_FLAG)]
        );
        assert_eq!(
            texts(&execute(
                &mut sess,
                &GmConfig::default(),
                &view,
                "@showflag ?x 1"
            )),
            vec![(GmColor::Red, Q_HELP_SHOW_FLAG)]
        );
        assert_eq!(
            texts(&execute(
                &mut sess,
                &GmConfig::default(),
                &view,
                "@showflag nowhere 5"
            )),
            vec![(GmColor::Red, "nowhere 现在不在线，或在其它服务器上!!!")]
        );
        // 在线（位阵恒零值模型）⇒ OFF，标识号按 StrToInt 原值回显（7 ⇒ [7]；非法 ⇒ 0）。
        for (arg, want) in [
            ("7", "bob: [7] = OFF"),
            ("0", "bob: [0] = OFF"),
            ("abc", "bob: [0] = OFF"),
        ] {
            assert_eq!(
                texts(&execute(
                    &mut sess,
                    &GmConfig::default(),
                    &view,
                    &format!("@showflag bob {arg}")
                )),
                vec![(GmColor::Green, want)],
                "{arg}"
            );
        }
        // LotteryTicket：部署全 0（格式串尾随一个空格）。
        let (r, _) = run_gm("@LotteryTicket");
        assert_eq!(
            texts(&r),
            vec![(
                GmColor::Green,
                "已中彩票数:0 未中彩票数:0 一等奖:0 二等奖:0 三等奖:0 四等奖:0 五等奖:0 六等奖:0 "
            )]
        );
        // 构造非 0：8 值按 {0}..{7} 顺序。
        let cfg = GmConfig {
            win_lottery_count: 11,
            no_win_lottery_count: 22,
            win_lottery_level1: 1,
            win_lottery_level2: 2,
            win_lottery_level3: 3,
            win_lottery_level4: 4,
            win_lottery_level5: 5,
            win_lottery_level6: 6,
            ..GmConfig::default()
        };
        let (r, _) = run_gm_with(&cfg, "@LotteryTicket");
        assert_eq!(
            texts(&r),
            vec![(
                GmColor::Green,
                "已中彩票数:11 未中彩票数:22 一等奖:1 二等奖:2 三等奖:3 四等奖:4 五等奖:5 六等奖:6 "
            )]
        );
    }

    /// 族 Q 权限门（10 条全 perm 10）：`Permission < 10` ⇒ 恰一条红字 `权限不够!!!`。
    #[test]
    fn q_family_permission_gate_blocks_body() {
        for name in [
            "HumanCountwa",
            "MobCountwa",
            "Map",
            "MapInfo",
            "ShowMapMode",
            "showflag",
            "IP",
            "信息",
            "Showuseiteminfo",
            "LotteryTicket",
        ] {
            let (r, sess) = run(&format!("@{name}"));
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, PERMISSION_TOO_LOW)],
                "{name}"
            );
            assert!(r.frames.is_empty(), "{name}");
            assert_eq!(sess, GmSession::default(), "{name} 零状态变化");
        }
    }

    /// **退化形态**：注册表未注入（`GmWorld::new`）≠「图不存在」——`@HumanCountwa`/
    /// `@ShowMapMode`/`@Map` 都按 [`NOTE_NO_MAP_REGISTRY`] 登记零帧，不当图不存在红字。
    #[test]
    fn q_family_registry_not_injected_is_not_map_not_found() {
        let guilds = mir2_world::GuildManager::new();
        let view = GmWorld::new(&guilds);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        for line in ["@HumanCountwa map", "@ShowMapMode map", "@Map 1 1 map"] {
            let r = execute(&mut sess, &cfg, &view, line);
            assert!(r.msgs.is_empty(), "{line} 未注入 ⇒ 零帧");
            let note = r.log_note.expect("{line} 必须登记");
            assert!(note.contains("地图注册表"), "{line}: {note}");
        }
    }

    // ---------------------------- 族 X1 · 禁言名单（5，卡 `openmir2-gm-family-x1-denywhisper`）----------------------------

    /// 5 条全部接线：现网名/perm 10/params（`+-jy`/`查看禁言` 单参，其余双参）；
    /// 出厂名不注册。牙：任一条回退 `Unimplemented` / 改名 / 改权限 ⇒ 红。
    #[test]
    fn x1_family_table_entries_are_wired() {
        for (name, imp, params) in [
            ("+jy", Imp::DisableSendMsg, 2),
            ("+-jy", Imp::DisableSendMsgListCmd, 1),
            ("-jy", Imp::EnableSendMsg, 2),
            ("查看禁言", Imp::ShutupList, 1),
            ("查看私聊", Imp::ViewWhisper, 2),
        ] {
            let def = find(name).unwrap_or_else(|| panic!("{name} 不在 COMMANDS 表"));
            assert_eq!(def.imp, imp, "{name} 的实现标签");
            assert_eq!(def.perm_min, 10, "{name} 的 PermissionMin");
            assert_eq!(def.params, params, "{name} 的 Execute 形参个数");
        }
        for factory in [
            "DisableSendMsg",
            "EnableSendMsg",
            "DisableSendMsgList",
            "ShutupList",
            "ViewWhisper",
        ] {
            assert!(
                find(factory).is_none(),
                "{factory} 出厂名不该命中（conf 覆盖后只认现网名）"
            );
        }
    }

    /// **A1+A2+A3 rework（review F1）**：四命令体运行期触 null 表 ⇒ NRE 被
    /// `PlayObject.Chat.cs:402` 顶层 catch 吞 ⇒ **零帧**——源文本恒字/绿字真机不可达；
    /// 空名红帮助（+jy/-jy）不触表仍可达；+jy 非空在在线目标上有真实副作用
    /// `FilterSendMsg=true`（非帧，note 记语义）。会话全程零变化（查询面，卡面 forbidden）。
    /// 变异红：M-R1（+jy 非空补绿字）/ M-R2（-jy 非空补恒红文本）/ M-R3（+-jy 补红蓝支）/
    /// M-R4（查看禁言补绿字）⇒ 各自零帧断言红。
    #[test]
    fn x1_null_table_zero_frames() {
        let (fx, w, _gm_id, _bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let view = q_view(&fx, &w, &guilds);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // +jy：空名红帮助（不触表 ⇒ 可达）。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@+jy")),
            vec![(GmColor::Red, X1_HELP_DISABLE_SEND_MSG)]
        );
        // +jy 非空（在线 bob）⇒ 零帧；note 记 FilterSendMsg 置位语义（rework ②）。
        let r = execute(&mut sess, &cfg, &view, "@+jy bob");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "+jy 非空应零帧");
        let note = r.log_note.expect("+jy 必须登记 NRE 零帧");
        assert!(
            note.contains("FilterSendMsg") && note.contains("NRE"),
            "{note}"
        );
        // +jy 非空（离线 nowhere）⇒ 零帧（不置位语义单独成 note）。
        let r = execute(&mut sess, &cfg, &view, "@+jy nowhere");
        assert!(r.msgs.is_empty() && r.frames.is_empty());
        let note = r.log_note.expect("离线支必须登记");
        assert!(note.contains("离线") && note.contains("不置位"), "{note}");
        // -jy：空名红帮助（不触表）；非空零帧（恒红文本真机不可达）。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@-jy")),
            vec![(GmColor::Red, X1_HELP_ENABLE_SEND_MSG)]
        );
        let r = execute(&mut sess, &cfg, &view, "@-jy bob");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "-jy 非空应零帧");
        assert!(r.log_note.expect("-jy 必须登记").contains("null"));
        // +-jy：恒零帧（首句即触 null 表）。
        let r = execute(&mut sess, &cfg, &view, "@+-jy");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "+-jy 应零帧");
        assert!(r.log_note.expect("+-jy 必须登记").contains("null"));
        // 查看禁言：恒零帧（EnterCriticalSection(null) 即 NRE）。
        let r = execute(&mut sess, &cfg, &view, "@查看禁言");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "查看禁言 应零帧");
        assert!(r.log_note.expect("查看禁言 必须登记").contains("null"));
        // 会话零变化（四条命令体不写任何会话字段）。
        assert_eq!(
            sess,
            GmSession {
                permission: 10,
                ..GmSession::default()
            }
        );
    }

    /// **A4**：`@查看私聊` 四怪癖/分支——空名红帮助（**双空格**帮助串逐字节）；单字符名
    /// `sChrName[1]` 越界 ⇒ 零帧；`a?` 红帮助 / `?a` 离线红；在线 toggle `listening_to`
    /// 镜像 ⇒ Msg2「正在侦听…」→ Msg1「已停止侦听…」→ Msg2（与 oracle `WhisperHuman`
    /// 比对同构）。变异红：M-X4（[1]→[0]）⇒ 零帧用例红；M-X6（丢 Msg1 分支）⇒ 红。
    #[test]
    fn x1_view_whisper_quirks() {
        let (fx, w, _gm_id, _bob_id) = q_fixture();
        let guilds = mir2_world::GuildManager::new();
        let view = q_view(&fx, &w, &guilds);
        let cfg = GmConfig::default();
        let mut sess = gm_session();
        // 空名 ⇒ 红帮助（Help=" 人物名称" 带前导空格 ⇒ 双空格）。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@查看私聊")),
            vec![(GmColor::Red, X1_HELP_VIEW_WHISPER)]
        );
        assert_eq!(X1_HELP_VIEW_WHISPER, "命令格式: @查看私聊  人物名称");
        // 单字符名 ⇒ 越界零帧（有 note 登记）。
        let r = execute(&mut sess, &cfg, &view, "@查看私聊 a");
        assert!(r.msgs.is_empty() && r.frames.is_empty(), "单字符名应零帧");
        assert!(r.log_note.is_some(), "越界必须登记");
        // `a?`：[1]=='?' ⇒ 红帮助；`?a`：[1]=='a' ⇒ 不命中 ⇒ 离线红。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@查看私聊 a?")),
            vec![(GmColor::Red, X1_HELP_VIEW_WHISPER)]
        );
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@查看私聊 ?a")),
            vec![(GmColor::Red, "?a 现在不在线，或在其它服务器上!!!")]
        );
        // 在线 toggle：Msg2（开）→ Msg1（关）→ Msg2（再开）；镜像位随动、其余会话态零变化。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@查看私聊 bob")),
            vec![(GmColor::Green, "正在侦听bob的私聊信息...")]
        );
        assert_eq!(sess.listening_to.as_deref(), Some("bob"));
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@查看私聊 bob")),
            vec![(GmColor::Green, "已停止侦听bob的私聊信息...")]
        );
        assert_eq!(sess.listening_to, None);
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@查看私聊 bob")),
            vec![(GmColor::Green, "正在侦听bob的私聊信息...")]
        );
        assert_eq!(sess.listening_to.as_deref(), Some("bob"));
        // 换目标 ⇒ oracle 目标 WhisperHuman != 发起者 ⇒ 仍 Msg2（开）。
        assert_eq!(
            texts(&execute(&mut sess, &cfg, &view, "@查看私聊 gm")),
            vec![(GmColor::Green, "正在侦听gm的私聊信息...")]
        );
        assert_eq!(sess.listening_to.as_deref(), Some("gm"));
        // 会话其余字段零变化（只 listening_to 动）。
        assert_eq!(
            sess,
            GmSession {
                permission: 10,
                listening_to: Some("gm".to_string()),
                ..GmSession::default()
            }
        );
    }

    /// 族 X1 权限门（5 条全 perm 10）：`Permission < 10` ⇒ 恰一条红字 `权限不够!!!`。
    #[test]
    fn x1_family_permission_gate_blocks_body() {
        for name in ["+jy", "+-jy", "-jy", "查看禁言", "查看私聊"] {
            let (r, sess) = run(&format!("@{name}"));
            assert_eq!(
                texts(&r),
                vec![(GmColor::Red, PERMISSION_TOO_LOW)],
                "{name}"
            );
            assert!(r.frames.is_empty(), "{name}");
            assert_eq!(sess, GmSession::default(), "{name} 零状态变化");
        }
    }
}
