//! GM `@` 命令面（oracle `src/Modules/GameCommand/**`）：**第一批 = 表驱动派发 + 现网可达 6 条**。
//!
//! 设计件：`mir2-rs/tests/parity/m3/design-gm-commands-20261023.md`（§1 分发链 / §2 可达性 / §4.2 判据 /
//! §6 批次 G1）。本模块只做 G1 的写集，命令体未实现的 180 条留在 `Imp::Unimplemented`，等 G2-G6。
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
    /// 本批未接的 180 条：现网全被权限门挡住，命令体留 G2-G6。
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
}

impl Default for GmSession {
    fn default() -> Self {
        Self {
            permission: 0,
            hear_whisper: true,
            lock_whisper_list: Vec::new(),
            is_lock_logon: true,
            is_lock_logoned: false,
        }
    }
}

/// 命令体要读的配置（`SystemShare.Config` 的等价物）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GmConfig {
    /// `Config.LockHumanLogin`（`ServerConf.cs:574` 读 `[Setup] PasswordLockHumanLogin`；
    /// `GameSvrConf.cs:1493` 缺省 false，部署 = 0）。
    pub lock_human_login: bool,
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

/// `CommandSystem.Execute(PlayerActor, line)` 的等价物（**不含** `PlayObject.Chat.cs` 的第二条回落帧）。
///
/// 调用方约定：`parsed == false` ⇒ 无帧；`found == false && parsed` ⇒ 先发 [`GmResult::msgs`]，
/// 再发 [`incorrect_msg`]。
pub fn execute(sess: &mut GmSession, cfg: &GmConfig, line: &str) -> GmResult {
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
        Imp::Unimplemented => GmResult {
            found: true,
            parsed: true,
            msgs: Vec::new(),
            log_note: Some("GM 命令体未实现（G1 只接现网可达 6 条；现网权限门先挡）"),
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "+-jip",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "+jip",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "-jip",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ReloadGuild",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ReloadGuildAll",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ReloadItemDB",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "ReloadMagicDB",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "TestGetBagItems",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "TestSpeedMode",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "TestStatus",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
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
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "重读公告",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "重读商人",
        perm_min: 10,
        params: 2,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "重读机器人脚本",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "重读机器人配置",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "重读爆率",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
    },
    CommandDef {
        name: "重读登陆",
        perm_min: 10,
        params: 1,
        imp: Imp::Unimplemented,
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
        let mut sess = GmSession::default();
        let r = execute(&mut sess, &GmConfig::default(), line);
        (r, sess)
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
        let r = execute(&mut sess, &GmConfig::default(), "@拒绝私聊");
        assert_eq!(texts(&r), vec![(GmColor::Green, "[禁止私聊]")]);
        assert!(!sess.hear_whisper);
        let r = execute(&mut sess, &GmConfig::default(), "@拒绝私聊");
        assert_eq!(texts(&r), vec![(GmColor::Green, "[允许私聊]")]);
        assert!(sess.hear_whisper);
    }

    /// `@LockLogin`：部署 `PasswordLockHumanLogin=0` ⇒ 恒回红字，`IsLockLogon` 不变。
    #[test]
    fn lock_login_reads_config_gate() {
        let mut sess = GmSession::default();
        let r = execute(
            &mut sess,
            &GmConfig {
                lock_human_login: false,
            },
            "@LockLogin",
        );
        assert_eq!(
            texts(&r),
            vec![(GmColor::Red, "本服务器还没有启用登录锁功能!!!")]
        );
        assert!(sess.is_lock_logon, "配置未开 ⇒ 不翻转");
        // 配置开了但 ctor `IsLockLogon=true`/`IsLockLogoned=false` ⇒ 第二道红字。
        let r = execute(
            &mut sess,
            &GmConfig {
                lock_human_login: true,
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
        let r = execute(
            &mut sess,
            &GmConfig {
                lock_human_login: true,
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
}
