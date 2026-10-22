//! GM `@` 命令面（oracle `src/Modules/GameCommand/**`）：**第一批 = 表驱动派发 + 现网可达 6 条**。
//!
//! 设计件：`mir2-rs/tests/parity/m3/design-gm-commands-20261023.md`（§1 分发链 / §2 可达性 / §4.2 判据 /
//! §6 批次 G1）。G1 落地派发链 + 5 条命令体；批次 2/3 再落 30 条（见下）。剩余 150 条留在
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
//! 这类混淆名与 `Level0`/`MasterName` 等「配置缺键回落构造默认名」都已按 oracle 解析结果落表
//! （配置死键照抄，见设计件 §3.2/§3.3 —— **不修**，属内容冻结）。
//!
//! ## 现网可达性（设计件 §2）
//!
//! `Permission` 的唯一生产来源 `GetHumPermission` 被注释（`PlayObject.cs:1188-1194`）、`AdminList.txt`
//! 缺 IP 段被丢弃、`StartPermission=0`、部署是 Release ⇒ **现网 `Permission` 恒 0**。185 条里 perm 0
//! 恰好 5 条，加内建 `@help` 共 6 条可达；其余 180 条一律停在 `权限不够!!!`（Red 255/56）。
//! [`GmSession::permission`] 因此保持 0（设计件 D-1 建议 (a)：1:1 复刻这个「锁死」）。
//!
//! ## 批次 2/3（本模块第二批，tier=L）
//!
//! 落地设计件 §4.4 的第 4 族（重载/管理 14 条）、第 5 族（封禁名单 9 条）、第 11 族（桩/测试 8 条，
//! 其中 `@AddRobotPlay` 已在 G1）。三族共 31 条，除 `@AddRobotPlay` 外全是 `PermissionMin = 10`
//! ⇒ 现网 `Permission` 恒 0，生产路径一律停在 `权限不够!!!`；命令体只在测试里用 `permission = 10` 构造。
//! **oracle 注释掉的逻辑照抄 no-op**（设计件 §4.5 #7）；宿主未接项逐条 `log_note` 并登记
//! `mir2-rs/tests/parity/whitelist.md` T-161..T-164。

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
    /// `ShowDenyChrNameLogonCommand`（`@ShowDenyCharNameLogon`）。
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

    /// 未接的 150 条（185 − G1 5 条 − 本批 30 条）：现网全被权限门挡住，命令体留 G4-G6。
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
        }
    }
}

/// 命令体要读的配置（`SystemShare.Config` / `SystemShare` 启动态的等价物）。
///
/// 除 `lock_human_login` 外都是批次 2/3 引入的**启动期常量**：oracle 部署的取值在字段注释里
/// 写明出处，[`Default`] 就取部署值（重载/管理族的分支正是由它们决定）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GmConfig {
    /// `Config.LockHumanLogin`（`ServerConf.cs:574` 读 `[Setup] PasswordLockHumanLogin`；
    /// `GameSvrConf.cs:1493` 缺省 false，部署 = 0）。
    pub lock_human_login: bool,
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
}

impl Default for GmConfig {
    fn default() -> Self {
        Self {
            lock_human_login: false,
            server_index: 0,
            manage_npc_present: true,
            function_npc_present: true,
            robot_npc_present: true,
            notice_file_exists: true,
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
    /// oracle 侧被 `ProcessUserLineMsg` 的 catch 吞掉的路径 / 本批未接的登记（调用方记日志，不静默）。
    pub log_note: Option<&'static str>,
}

impl GmResult {
    fn one(color: GmColor, text: String) -> Self {
        Self {
            found: true,
            parsed: true,
            msgs: vec![GmMsg { color, text }],
            log_note: None,
        }
    }

    fn empty() -> Self {
        Self {
            found: true,
            parsed: true,
            msgs: Vec::new(),
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
/// 本批只需要一项：`ReloadGuild` 的 `GuildMgr.FindGuild`。
#[derive(Clone, Copy)]
pub struct GmWorld<'a> {
    /// `SystemShare.GuildMgr`；`FindGuild` 是 `GuildName == sGuildName` **精确**比较
    /// （`GuildManager.cs:66-76`，不是 `OrdinalIgnoreCase`）。
    pub guilds: &'a mir2_world::GuildManager,
}

/// 零帧 + 一条 `log_note`：oracle 抛异常被 `PlayObject.Chat.cs` 的 catch 吞掉，或宿主未接。
fn no_frames(note: &'static str) -> GmResult {
    GmResult {
        found: true,
        parsed: true,
        msgs: Vec::new(),
        log_note: Some(note),
    }
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
        log_note: Some(note),
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

// ---------------------------- 封禁名单族（设计件 §4.4 第 5 族）----------------------------
//
// 三张表的**增/删/查/存循环在 oracle 里全部被注释**（`Deny*Command.cs` 的 for 体、
// `M2Share.SaveDenyAccountList` 的写盘体、`LoadDenyAccountList`/`LoadDenyChrNameList`/
// `LoadDenyIPAddrList` 的读入体），所以启动后三表恒空、`boDelete` 恒 false。本仓不建这三张表
// （宿主未接，`whitelist.md` T-162），文本按 oracle 原文发。

/// `DelDenyAccountLogonCommand`（现网 `@-jid`）。
fn del_deny_account(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let account = params.first().copied().unwrap_or("");
    if account.is_empty() {
        return GmResult::one(GmColor::Red, DEL_DENY_ACCOUNT_HELP.to_string());
    }
    // for 循环体（含命中提示 `已从禁止登录帐号列表中删除。`）整段被注释 ⇒ boDelete 恒 false。
    GmResult::one(GmColor::Green, format!("{account}没有被禁止登录。"))
}

/// `DelDenyChrNameLogonCommand`（现网 `@DelDenyChrNameLogon`）。
fn del_deny_chr_name(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let chr_name = params.first().copied().unwrap_or("");
    if chr_name.is_empty() {
        return GmResult::one(GmColor::Red, DEL_DENY_CHRNAME_HELP.to_string());
    }
    GmResult::one(GmColor::Green, format!("{chr_name}没有被禁止登录。"))
}

/// `DelDenyIPaddrLogonCommand`（现网 `@-jip`）：倒序循环体同样被注释 ⇒ 恒 false。
fn del_deny_ip(parameters: &str) -> GmResult {
    let params: Vec<&str> = parameters.split(' ').collect();
    let ip = params.first().copied().unwrap_or("");
    if ip.is_empty() {
        return GmResult::one(GmColor::Red, DEL_DENY_IP_HELP.to_string());
    }
    GmResult::one(GmColor::Green, format!("{ip}没有被禁止登录。"))
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
/// `Permission < 6` 静默返回（门是 10 ⇒ 死支）；表恒空 ⇒ 恒回「禁止登录帐号列表为空。」。
fn show_deny_account(sess: &GmSession) -> GmResult {
    if sess.permission < 6 {
        return GmResult::empty();
    }
    GmResult::one(GmColor::Green, "禁止登录帐号列表为空。".to_string())
}

/// `ShowDenyChrNameLogonCommand`（现网 `@ShowDenyCharNameLogon`）：无权限二次校验，表空即提示。
fn show_deny_chr_name() -> GmResult {
    GmResult::one(GmColor::Green, "禁止登录角色列表为空。".to_string())
}

/// `ShowDenyIPaddrLogonCommand`（现网 `@+-jip`）：表空提示**沿用角色文案**
/// （`禁止登录角色列表为空。`），是 oracle 的复制粘贴怪癖，照抄。
fn show_deny_ip() -> GmResult {
    GmResult::one(GmColor::Green, "禁止登录角色列表为空。".to_string())
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

/// `CommandSystem.Execute(PlayerActor, line)` 的等价物（**不含** `PlayObject.Chat.cs` 的第二条回落帧）。
///
/// 调用方约定：`parsed == false` ⇒ 无帧；`found == false && parsed` ⇒ 先发 [`GmResult::msgs`]，
/// 再发 [`incorrect_msg`]。
pub fn execute(sess: &mut GmSession, cfg: &GmConfig, world: &GmWorld<'_>, line: &str) -> GmResult {
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

        Imp::Unimplemented => GmResult {
            found: true,
            parsed: true,
            msgs: Vec::new(),
            log_note: Some("GM 命令体未实现（G1 + 批次 2/3 之外；现网权限门先挡）"),
        },
    }
}

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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "+jy",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "1",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "1级",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "2",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "3",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "AddGamePoint",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Announcement",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "AttackMode",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Binduseitem",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ChangeItemName",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "CHGUSERFULL",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "CHGZENFASTSTEP",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "dfsgf567",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "DisableFilter",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "dsfsdt546",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "fdgfdg56",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "fdgh56j",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "GameGird",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "GameGlory",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "GetUserItems",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "gfh678dfg",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ghjhg6",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "hjhg556",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "hjhgj67",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "HumanCountwa",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Hunger",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "IP",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "jkt76",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Level0",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "LuckyPoint",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "Map",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "MapInfo",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "MasterName",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "MobFireBurn",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "OnHorse",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "PasswordLock",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "RecallHuman",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ReGotoHuman",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "RestBonuPoint",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "sadsa54",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "SbkDoor",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        name: "ShowDenyCharNameLogon",
        perm_min: 10,
        params: 1,
        imp: Imp::ShowDenyChrName,
    },
    CommandDef {
        name: "ShowEffect",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "showflag",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ShowMapMode",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "SignMapMove",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "UnBindUseItem",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "VV踢人",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "传送",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "伴侣变更",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "信息",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "减钱",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "加钱",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "取消禁言",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "取消联盟",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "增加PK",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "增加技能",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "开始提问",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "改变性别",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "改属性",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "查看私聊",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "清除PK",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "禁言",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "职业变更",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "联盟",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "调整声望",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "调整属性",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "调整泡点",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "调整经验",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "转生",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
];

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
        let world = GmWorld { guilds: &guilds };
        execute(sess, cfg, &world, line)
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
        assert_eq!(sadasd.imp, Imp::Unimplemented);
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

    /// 本批 30 条命令体全部接上（`Unimplemented` 从 180 降到 150），且每条的命令名 → 实现标签固定。
    #[test]
    fn batch2_table_wires_thirty_command_bodies() {
        assert_eq!(
            COMMANDS
                .iter()
                .filter(|d| d.imp != Imp::Unimplemented)
                .count(),
            35,
            "35 = G1 的 5 条 + 本批 30 条"
        );
        assert_eq!(
            COMMANDS
                .iter()
                .filter(|d| d.imp == Imp::Unimplemented)
                .count(),
            150,
            "185 - 35"
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
            ("ShowDenyCharNameLogon", Imp::ShowDenyChrName),
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

    /// 封禁名单 9 条的文本（含「表恒空」与复制粘贴怪癖）。
    #[test]
    fn deny_family_texts_match_oracle() {
        use GmColor::*;
        let cases: &[(&str, GmColor, &str)] = &[
            ("@-jid", Red, DEL_DENY_ACCOUNT_HELP),
            ("@-jid acct", Green, "acct没有被禁止登录。"),
            ("@DelDenyChrNameLogon", Red, DEL_DENY_CHRNAME_HELP),
            ("@DelDenyChrNameLogon 张三", Green, "张三没有被禁止登录。"),
            ("@-jip", Red, DEL_DENY_IP_HELP),
            ("@-jip 1.2.3.4", Green, "1.2.3.4没有被禁止登录。"),
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
            // Show 族：三表恒空；`ShowDenyIPaddr` 的表空提示沿用角色文案。
            ("@+-jid", Green, "禁止登录帐号列表为空。"),
            ("@ShowDenyCharNameLogon", Green, "禁止登录角色列表为空。"),
            ("@+-jip", Green, "禁止登录角色列表为空。"),
        ];
        for (line, color, text) in cases {
            let (r, _) = run_gm(line);
            assert_eq!(texts(&r), vec![(*color, *text)], "{line}");
            assert!(r.found && r.parsed, "{line}");
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
        let world = GmWorld { guilds: &gm };
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
        let world = GmWorld { guilds: &gm };
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
}
