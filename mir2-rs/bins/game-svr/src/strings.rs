//! `String.conf` 读取（`M2GameSvr/string.conf`）—— 帧 42 `SM_USERNAME` 的**展示名模板**输入。
//!
//! 逐条照 oracle：
//! - 文件定位：`SystemShare.cs:187` = `Path.Combine(BasePath, ConfConst.StringFileName)`，
//!   `ConfConst.cs:8` 的常量是 `"string.conf"`；`BasePath` = exe 目录 = `Server.conf` 同目录。
//! - 读取语义：`StringConf.cs:7` 继承 `ConfigFile`，与 `Server.conf` 同一套 `Load()`
//!   （`ConfigFile.cs:256-340`）：**GB2312/GBK** 解码 + `Trim` + 行内 `;;` 注释 + 段/键
//!   `OrdinalIgnoreCase` + 首次出现为准。
//! - 缺文件：oracle `ConfigFile.Load` 只 `File.Create(...)` 后返回（沿用源码默认值）；
//!   故本模块对**缺文件**返回「全默认」，不当错误（调用方记 WRN）。
//! - 缺键：用 `MessageSettings` 源码默认值（`src/Modules/SystemModule/MessageSettings.cs:228-237`）。
//!
//! 只读帧 42 与 B4 `YouPoisoned` 用到的 10 个键（`[String]` 段），不做通用 INI。

use std::path::Path;

use anyhow::{Context, Result};

/// oracle `MessageSettings.HumanShowName`（`MessageSettings.cs:238`）。
pub const DEFAULT_HUMAN_SHOW_NAME: &str = "%chrname\\%guildname\\%dearname\\%mastername";
/// oracle `MessageSettings.RankLevelName`（`MessageSettings.cs:233`）。
pub const DEFAULT_RANK_LEVEL_NAME: &str = "{0}\\平民";
/// oracle `MessageSettings.WarrReNewName`（`MessageSettings.cs:230`）。
pub const DEFAULT_WARR_RE_NEW_NAME: &str = "%chrname\\*<圣>*";
/// oracle `MessageSettings.WizardReNewName`（`MessageSettings.cs:231`）。
pub const DEFAULT_WIZARD_RE_NEW_NAME: &str = "%chrname\\*<神>*";
/// oracle `MessageSettings.TaosReNewName`（`MessageSettings.cs:232`）。
pub const DEFAULT_TAOS_RE_NEW_NAME: &str = "%chrname\\*<尊>*";
/// oracle `MessageSettings.ManDearName`（`MessageSettings.cs:234`）。
pub const DEFAULT_MAN_DEAR_NAME: &str = "{0}的老公";
/// oracle `MessageSettings.WoManDearName`（`MessageSettings.cs:235`）。
pub const DEFAULT_WO_MAN_DEAR_NAME: &str = "{0}的妻子";
/// oracle `MessageSettings.MasterName`（`MessageSettings.cs:236`）。
pub const DEFAULT_MASTER_NAME: &str = "{0}的师傅";
/// oracle `MessageSettings.NoMasterName`（`MessageSettings.cs:237`）。
pub const DEFAULT_NO_MASTER_NAME: &str = "{0}的徒弟";
/// oracle `MessageSettings.CanotPickUpItem`（`MessageSettings.cs:221`）——`ClientPickUpItem` 里
/// “别人（或未过保护期）掉落物不可拾”的提示文本（`PlayObject.Base.cs:73`）。
pub const DEFAULT_CANOT_PICK_UP_ITEM: &str = "在一定时间以内无法捡起此物品!!!";
/// oracle `MessageSettings.YouPoisoned`（`MessageSettings.cs:64`）——**源码默认值**。
/// 现网四份 `String.conf` 均为 `YouPoisoned=您中毒了!!!`（`M2GameSvr/String.conf:31`），
/// 经 `StringConf.cs:76`（`GameShare.cs:91`）读入后覆盖本默认 ⇒ 线上体字节
/// `c4fad6d0b6bec1cb212121`（`SM_SYSMESSAGE(100)`，魔号 6/48 与 50-D2 的 STY 帧）。
pub const DEFAULT_YOU_POISONED: &str = "中毒了!!!";
/// `[String] GuildMsgPreFix`（`GameSvrConf.cs:1843`，缺省 `〖行会〗`）——`ShowPreFixMsg` 为真时
/// 行会消息（104）的前缀（`GuildInfo.cs:435-438`，**无空格**拼接）。
pub const DEFAULT_GUILD_MSG_PREFIX: &str = "〖行会〗";
/// `[Guild] GuildNotice`（`GameSvrConf.cs:1850`）——`<名>.txt` 公告段头。
pub const DEFAULT_GUILD_NOTICE: &str = "公告";
/// `[Guild] GuildWar`（`GameSvrConf.cs:1851`）——敌对行会段头。
pub const DEFAULT_GUILD_WAR: &str = "敌对行会";
/// `[Guild] GuildAll`（`GameSvrConf.cs:1852`）——联盟行会段头。
pub const DEFAULT_GUILD_ALL: &str = "联盟行会";
/// `[Guild] GuildMember`（`GameSvrConf.cs:1853`）——成员段头。
pub const DEFAULT_GUILD_MEMBER: &str = "行会成员";
/// `[Guild] GuildMemberRank`（`GameSvrConf.cs:1854`）——普通成员封号。
pub const DEFAULT_GUILD_MEMBER_RANK: &str = "行会成员";
/// `[Guild] GuildChief`（`GameSvrConf.cs:1855`）——掌门人封号。
pub const DEFAULT_GUILD_CHIEF: &str = "掌门人";

/// oracle `MessageSettings.YouNeedLevelMsg`（`MessageSettings.cs:258`）——`!` 喊话等级门提示。
pub const DEFAULT_YOU_NEED_LEVEL_MSG: &str = "你的等级要在{0}级以上才能用此功能!!!";
/// oracle `MessageSettings.ThisMapDisableSendCyCyMsg`（`MessageSettings.cs:259`）——`boQUIZ` 图提示。
pub const DEFAULT_THIS_MAP_DISABLE_SEND_CYCY_MSG: &str = "本地图不允许喊话!!!";
/// oracle `MessageSettings.YouCanSendCyCyLaterMsg`（`MessageSettings.cs:260`）——10s 门提示。
pub const DEFAULT_YOU_CAN_SEND_CYCY_LATER_MSG: &str = "{0}秒后才可以再发文字!!!";

/// oracle `MessageSettings.AttackModeOfAll`（`MessageSettings.cs:39`；`StringConf.cs:54` 读键）——登录 `SysMsg` 攻击模式文案。
pub const DEFAULT_ATTACK_MODE_OF_ALL: &str = "[攻击模式: 全体攻击]";
/// `MessageSettings.AttackModeOfPeaceful`（`MessageSettings.cs:40`；`StringConf.cs:55` 读键）。
pub const DEFAULT_ATTACK_MODE_OF_PEACEFUL: &str = "[攻击模式: 和平攻击]";
/// `MessageSettings.AttackModeOfDear`（`MessageSettings.cs:41`）——**只作为源码默认值**：
/// oracle `StringConf.cs:53-61` 不读 `[String] AttackModeOfDear` 键（部署件也没有该键），
/// 故本解析器同样不读（Minor 1，2026-10-21）。
pub const DEFAULT_ATTACK_MODE_OF_DEAR: &str = "[攻击模式: 夫妻攻击]";
/// `MessageSettings.AttackModeOfMaster`（`MessageSettings.cs:42`；`StringConf.cs:59` 读键）。
pub const DEFAULT_ATTACK_MODE_OF_MASTER: &str = "[攻击模式: 师徒攻击]";
/// `MessageSettings.AttackModeOfGroup`（`MessageSettings.cs:43`；`StringConf.cs:56` 读键）。
pub const DEFAULT_ATTACK_MODE_OF_GROUP: &str = "[攻击模式: 编组攻击]";
/// `MessageSettings.AttackModeOfGuild`（`MessageSettings.cs:44`；`StringConf.cs:57` 读键）。
pub const DEFAULT_ATTACK_MODE_OF_GUILD: &str = "[攻击模式: 行会攻击]";
/// `MessageSettings.AttackModeOfRedWhite`（`MessageSettings.cs:45`；`StringConf.cs:58` 读键）。
pub const DEFAULT_ATTACK_MODE_OF_RED_WHITE: &str = "[攻击模式: 红名攻击]";
/// `MessageSettings.StartChangeAttackModeHelp`（`MessageSettings.cs:46`；`StringConf.cs:60` 读键）——登录第二条 `SysMsg`。
pub const DEFAULT_START_CHANGE_ATTACK_MODE_HELP: &str = "使用组合快捷键 CTRL-H 更改攻击模式...";
/// `MessageSettings.StartNoticeMsg`（`MessageSettings.cs:47`；`StringConf.cs:61` 读键）——`TestServer` 时的欢迎语（第三条）。
pub const DEFAULT_START_NOTICE_MSG: &str = "欢迎进入本服务器进行游戏...";
/// `MessageSettings.NowIsFreePlayMode`（`MessageSettings.cs:38`；`StringConf.cs:53` 读键）——`PayMent==3` 的第四条。
pub const DEFAULT_NOW_IS_FREE_PLAY_MODE: &str = "当前服务器运行于测试模式.";

/// oracle `MessageSettings.RevivalRecoverMsg`（`MessageSettings.cs:226`）——P32 复活成功的
/// `SysMsg(..., Green, Hint)` 文本（`PlayObject.Message.cs:759`）。缺键回落源码默认。
pub const DEFAULT_REVIVAL_RECOVER_MSG: &str = "复活戒指生效，体力恢复.";

/// `[String]` 段里帧 42 用到的键（键名照 `StringConf.cs`，**不发明**）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringConf {
    human_show_name: Option<String>,
    rank_level_name: Option<String>,
    warr_re_new_name: Option<String>,
    wizard_re_new_name: Option<String>,
    taos_re_new_name: Option<String>,
    man_dear_name: Option<String>,
    wo_man_dear_name: Option<String>,
    master_name: Option<String>,
    no_master_name: Option<String>,
    canot_pick_up_item: Option<String>,
    you_poisoned: Option<String>,
    revival_recover: Option<String>,
    // `[Guild]` 六键（`StringConf.cs:30-35`）。
    guild_notice: Option<String>,
    guild_war: Option<String>,
    guild_all: Option<String>,
    guild_member: Option<String>,
    guild_member_rank: Option<String>,
    guild_chief: Option<String>,
    // `[String] GuildMsgPreFix`（`StringConf.cs:38`）。
    guild_msg_prefix: Option<String>,
    // 登录 `UserLogon` 的 3+1 条 `SysMsg` 文本（`PlayObject.cs:1342-1383/1417-1420`）。
    // oracle `StringConf.cs:53-61` 读 NowIsFreePlayMode/AttackModeOfAll/Peaceful/Group/Guild/
    // RedWhite/Master/StartChangeAttackModeHelp/StartNoticeMsg —— **不含 `AttackModeOfDear`**
    // （Minor 1：部署件无该键、oracle 不读，本解析器同样不读；模式 2 用源码默认值）。
    attack_mode_of_all: Option<String>,
    attack_mode_of_peaceful: Option<String>,
    attack_mode_of_master: Option<String>,
    attack_mode_of_group: Option<String>,
    attack_mode_of_guild: Option<String>,
    attack_mode_of_red_white: Option<String>,
    start_change_attack_mode_help: Option<String>,
    start_notice_msg: Option<String>,
    now_is_free_play_mode: Option<String>,
    // `[String]` 喊话三键（`StringConf.cs:257-259`）。
    you_need_level_msg: Option<String>,
    this_map_disable_send_cycy_msg: Option<String>,
    you_can_send_cycy_later_msg: Option<String>,
}

impl StringConf {
    /// 全默认（`String.conf` 缺失/none 时的取值）。
    #[must_use]
    pub fn defaults() -> Self {
        Self::default()
    }

    /// `[String] HumanShowName`。
    #[must_use]
    pub fn human_show_name(&self) -> &str {
        self.human_show_name
            .as_deref()
            .unwrap_or(DEFAULT_HUMAN_SHOW_NAME)
    }

    /// `[String] RankLevelName`。
    #[must_use]
    pub fn rank_level_name(&self) -> &str {
        self.rank_level_name
            .as_deref()
            .unwrap_or(DEFAULT_RANK_LEVEL_NAME)
    }

    /// `[String] WarrReNewName`。
    #[must_use]
    pub fn warr_re_new_name(&self) -> &str {
        self.warr_re_new_name
            .as_deref()
            .unwrap_or(DEFAULT_WARR_RE_NEW_NAME)
    }

    /// `[String] WizardReNewName`。
    #[must_use]
    pub fn wizard_re_new_name(&self) -> &str {
        self.wizard_re_new_name
            .as_deref()
            .unwrap_or(DEFAULT_WIZARD_RE_NEW_NAME)
    }

    /// `[String] TaosReNewName`。
    #[must_use]
    pub fn taos_re_new_name(&self) -> &str {
        self.taos_re_new_name
            .as_deref()
            .unwrap_or(DEFAULT_TAOS_RE_NEW_NAME)
    }

    /// `[String] ManDearName`。
    #[must_use]
    pub fn man_dear_name(&self) -> &str {
        self.man_dear_name
            .as_deref()
            .unwrap_or(DEFAULT_MAN_DEAR_NAME)
    }

    /// `[String] WoManDearName`。
    #[must_use]
    pub fn wo_man_dear_name(&self) -> &str {
        self.wo_man_dear_name
            .as_deref()
            .unwrap_or(DEFAULT_WO_MAN_DEAR_NAME)
    }

    /// `[String] MasterName`。
    #[must_use]
    pub fn master_name(&self) -> &str {
        self.master_name.as_deref().unwrap_or(DEFAULT_MASTER_NAME)
    }

    /// `[String] NoMasterName`。
    #[must_use]
    pub fn no_master_name(&self) -> &str {
        self.no_master_name
            .as_deref()
            .unwrap_or(DEFAULT_NO_MASTER_NAME)
    }

    /// `[String] CanotPickUpItem`（“无法捡起此物品”提示）。
    #[must_use]
    pub fn canot_pick_up_item(&self) -> &str {
        self.canot_pick_up_item
            .as_deref()
            .unwrap_or(DEFAULT_CANOT_PICK_UP_ITEM)
    }

    /// `[String] YouPoisoned`（`SysMsg(YouPoisoned, Red, Hint)` 的文本）。
    #[must_use]
    pub fn you_poisoned(&self) -> &str {
        self.you_poisoned.as_deref().unwrap_or(DEFAULT_YOU_POISONED)
    }

    /// `[Guild] GuildNotice`（缺键回落 `GameSvrConf.cs:1850`）。
    #[must_use]
    pub fn guild_notice(&self) -> &str {
        self.guild_notice.as_deref().unwrap_or(DEFAULT_GUILD_NOTICE)
    }

    /// `[Guild] GuildWar`。
    #[must_use]
    pub fn guild_war(&self) -> &str {
        self.guild_war.as_deref().unwrap_or(DEFAULT_GUILD_WAR)
    }

    /// `[Guild] GuildAll`。
    #[must_use]
    pub fn guild_all(&self) -> &str {
        self.guild_all.as_deref().unwrap_or(DEFAULT_GUILD_ALL)
    }

    /// `[Guild] GuildMember`。
    #[must_use]
    pub fn guild_member(&self) -> &str {
        self.guild_member.as_deref().unwrap_or(DEFAULT_GUILD_MEMBER)
    }

    /// `[Guild] GuildMemberRank`（`AddMember` 的普通成员封号）。
    #[must_use]
    pub fn guild_member_rank(&self) -> &str {
        self.guild_member_rank
            .as_deref()
            .unwrap_or(DEFAULT_GUILD_MEMBER_RANK)
    }

    /// `[Guild] GuildChief`（`SetGuildInfo` 的掌门封号）。
    #[must_use]
    pub fn guild_chief(&self) -> &str {
        self.guild_chief.as_deref().unwrap_or(DEFAULT_GUILD_CHIEF)
    }

    /// `[String] GuildMsgPreFix`（`ShowPreFixMsg` 为真时 104 的前缀，缺省 `〖行会〗`）。
    #[must_use]
    pub fn guild_msg_prefix(&self) -> &str {
        self.guild_msg_prefix
            .as_deref()
            .unwrap_or(DEFAULT_GUILD_MSG_PREFIX)
    }

    /// `[String] YouNeedLevelSendMsg`（`!` 喊话等级门；缺键回落 `MessageSettings.cs:258`）。
    #[must_use]
    pub fn you_need_level_msg(&self) -> &str {
        self.you_need_level_msg
            .as_deref()
            .unwrap_or(DEFAULT_YOU_NEED_LEVEL_MSG)
    }

    /// `[String] ThisMapDisableSendCyCyMsg`（`boQUIZ` 图；缺键回落 `MessageSettings.cs:259`）。
    #[must_use]
    pub fn this_map_disable_send_cycy_msg(&self) -> &str {
        self.this_map_disable_send_cycy_msg
            .as_deref()
            .unwrap_or(DEFAULT_THIS_MAP_DISABLE_SEND_CYCY_MSG)
    }

    /// `[String] YouCanSendCyCyLaterMsg`（10s 门；缺键回落 `MessageSettings.cs:260`）。
    #[must_use]
    pub fn you_can_send_cycy_later_msg(&self) -> &str {
        self.you_can_send_cycy_later_msg
            .as_deref()
            .unwrap_or(DEFAULT_YOU_CAN_SEND_CYCY_LATER_MSG)
    }

    /// `[String] RevivalRecoverMsg`（P32 复活提示；缺键回落 `MessageSettings.cs:226`）。
    #[must_use]
    pub fn revival_recover(&self) -> &str {
        self.revival_recover
            .as_deref()
            .unwrap_or(DEFAULT_REVIVAL_RECOVER_MSG)
    }

    /// `[String] AttackModeOfXxx`（`PlayObject.cs:1342-1371` 的 `switch (AttatckMode)`）：
    /// `mode` = oracle `AttatckMode`（`AttackMode` 枚举 `HAM_ALL=0 … HAM_PKATTACK=6`）。
    /// 枚举外 ⇒ `None`（oracle `switch` 无 `default` ⇒ **一条都不发**，见 `PlayObject.cs:1371`）。
    #[must_use]
    pub fn attack_mode_msg(&self, mode: u8) -> Option<&str> {
        // `AttatckMode.HAM_DEAR(2)`：oracle `StringConf.cs:53-61` **不读** `AttackModeOfDear`
        // ⇒ 恒用 `MessageSettings.cs:41` 的源码默认值（Minor 1；部署件无该键）。
        if mode == 2 {
            return Some(DEFAULT_ATTACK_MODE_OF_DEAR);
        }
        let (slot, default) = match mode {
            0 => (&self.attack_mode_of_all, DEFAULT_ATTACK_MODE_OF_ALL),
            1 => (
                &self.attack_mode_of_peaceful,
                DEFAULT_ATTACK_MODE_OF_PEACEFUL,
            ),
            3 => (&self.attack_mode_of_master, DEFAULT_ATTACK_MODE_OF_MASTER),
            4 => (&self.attack_mode_of_group, DEFAULT_ATTACK_MODE_OF_GROUP),
            5 => (&self.attack_mode_of_guild, DEFAULT_ATTACK_MODE_OF_GUILD),
            6 => (
                &self.attack_mode_of_red_white,
                DEFAULT_ATTACK_MODE_OF_RED_WHITE,
            ),
            _ => return None,
        };
        Some(slot.as_deref().unwrap_or(default))
    }

    /// `[String] StartChangeAttackModeHelp`（`MessageSettings.cs:46`；`StringConf.cs:60`）。
    #[must_use]
    pub fn start_change_attack_mode_help(&self) -> &str {
        self.start_change_attack_mode_help
            .as_deref()
            .unwrap_or(DEFAULT_START_CHANGE_ATTACK_MODE_HELP)
    }

    /// `[String] StartNoticeMsg`（`MessageSettings.cs:47`；`StringConf.cs:61`；仅 `TestServer` 发）。
    #[must_use]
    pub fn start_notice_msg(&self) -> &str {
        self.start_notice_msg
            .as_deref()
            .unwrap_or(DEFAULT_START_NOTICE_MSG)
    }

    /// `[String] NowIsFreePlayMode`（`MessageSettings.cs:38`；`StringConf.cs:53`；仅 `PayMent==3` 发）。
    #[must_use]
    pub fn now_is_free_play_mode(&self) -> &str {
        self.now_is_free_play_mode
            .as_deref()
            .unwrap_or(DEFAULT_NOW_IS_FREE_PLAY_MODE)
    }
}

/// 读 `String.conf` 并解析 `[String]` 段。
///
/// # Errors
/// 文件存在但读失败（权限/IO）才返回错误；**文件不存在**返回 [`StringConf::defaults`]
/// （照 oracle `ConfigFile.Load` 的「缺文件即建空文件并沿默认」语义）。
pub fn load(path: &Path) -> Result<StringConf> {
    if !path.exists() {
        return Ok(StringConf::defaults());
    }
    let bytes =
        std::fs::read(path).with_context(|| format!("读不到 String.conf {}", path.display()))?;
    Ok(parse(&bytes))
}

/// 解析 `[String]` 段（`ConfigFile.Load` 的 1:1 子集：BOM 探测 + cp936 + Trim + `;;` 行内注释 + 首次为准）。
///
/// 编码口径 = `Common/ConfigFile.cs:263` 的 `StreamReader(..., Encoding.GetEncoding("GB2312"))`：
/// GB2312 的 preamble 长度为 0 ⇒ `StreamReader` 做 BOM 探测（UTF-8 / UTF-16LE / UTF-16BE /
/// UTF-32LE `FF FE 00 00` / UTF-32BE），无 BOM 按 cp936。统一走
/// `mir2_script::cp936::decode_config_text`（E-6）；旧实现裸 `encoding_rs::GBK.decode`
/// 会把 UTF-8 BOM 的文件解成乱码。
#[must_use]
pub fn parse(bytes: &[u8]) -> StringConf {
    let text = mir2_script::cp936::decode_config_text(bytes);
    let mut conf = StringConf::defaults();
    // 段解析从「单 bool」改为三态（I-6）：`[String]` 与 `[Guild]` 各取各的键，
    // 其余段一律忽略。段名 `OrdinalIgnoreCase`（`ConfigFile.cs:13`）。
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        String,
        Guild,
        Other,
    }
    let mut section = Section::Other;
    let mut large_comment = false;
    for raw in text.lines() {
        let mut line = raw.trim();
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
        if line.starts_with(';') || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let sec = rest.split_once(']').map_or("", |(name, _)| name.trim());
            section = if sec.eq_ignore_ascii_case("String") {
                Section::String
            } else if sec.eq_ignore_ascii_case("Guild") {
                Section::Guild
            } else {
                Section::Other
            };
            continue;
        }
        if section == Section::Other {
            continue;
        }
        // 行内注释：`ConfigFile.cs:314-318` 取首个 `;;` 之前。
        if let Some(i) = line.find(";;") {
            line = line[..i].trim();
        }
        // `SplitKeyVal`：首个 `=`，且位置必须 > 0。
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty() {
            continue;
        }
        // `ConfigFile.cs:327`：同一键**首次出现为准**。
        let slot = match (section, k.to_ascii_uppercase().as_str()) {
            (Section::String, "HUMANSHOWNAME") => &mut conf.human_show_name,
            (Section::String, "RANKLEVELNAME") => &mut conf.rank_level_name,
            (Section::String, "WARRRENEWNAME") => &mut conf.warr_re_new_name,
            (Section::String, "WIZARDRENEWNAME") => &mut conf.wizard_re_new_name,
            (Section::String, "TAOSRENEWNAME") => &mut conf.taos_re_new_name,
            (Section::String, "MANDEARNAME") => &mut conf.man_dear_name,
            (Section::String, "WOMANDEARNAME") => &mut conf.wo_man_dear_name,
            (Section::String, "MASTERNAME") => &mut conf.master_name,
            (Section::String, "NOMASTERNAME") => &mut conf.no_master_name,
            (Section::String, "CANOTPICKUPITEM") => &mut conf.canot_pick_up_item,
            (Section::String, "YOUPOISONED") => &mut conf.you_poisoned,
            (Section::String, "REVIVALRECOVERMSG") => &mut conf.revival_recover,
            (Section::String, "GUILDMSGPREFIX") => &mut conf.guild_msg_prefix,
            (Section::String, "ATTACKMODEOFALL") => &mut conf.attack_mode_of_all,
            (Section::String, "ATTACKMODEOFPEACEFUL") => &mut conf.attack_mode_of_peaceful,
            (Section::String, "ATTACKMODEOFMASTER") => &mut conf.attack_mode_of_master,
            (Section::String, "ATTACKMODEOFGROUP") => &mut conf.attack_mode_of_group,
            (Section::String, "ATTACKMODEOFGUILD") => &mut conf.attack_mode_of_guild,
            (Section::String, "ATTACKMODEOFREDWHITE") => &mut conf.attack_mode_of_red_white,
            (Section::String, "STARTCHANGEATTACKMODEHELP") => {
                &mut conf.start_change_attack_mode_help
            }
            (Section::String, "STARTNOTICEMSG") => &mut conf.start_notice_msg,
            (Section::String, "NOWISFREEPLAYMODE") => &mut conf.now_is_free_play_mode,
            (Section::String, "YOUNEEDLEVELSENDMSG") => &mut conf.you_need_level_msg,
            (Section::String, "THISMAPDISABLESENDCYCYMSG") => {
                &mut conf.this_map_disable_send_cycy_msg
            }
            (Section::String, "YOUCANSENDCYCYLATERMSG") => &mut conf.you_can_send_cycy_later_msg,
            (Section::Guild, "GUILDNOTICE") => &mut conf.guild_notice,
            (Section::Guild, "GUILDWAR") => &mut conf.guild_war,
            (Section::Guild, "GUILDALL") => &mut conf.guild_all,
            (Section::Guild, "GUILDMEMBER") => &mut conf.guild_member,
            (Section::Guild, "GUILDMEMBERRANK") => &mut conf.guild_member_rank,
            (Section::Guild, "GUILDCHIEF") => &mut conf.guild_chief,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(v.to_string());
        }
    }
    conf
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 现网形态：`[String] RankLevelName=%s\` / `HumanShowName=%chrname\%guildname\%dearname`
    /// （`E:\MirServer\M2GameSvr\String.conf:228/233`，值里的 `\` 是**字面**反斜杠）。
    #[test]
    fn parses_deployed_get_show_name_keys() {
        let bytes =
            b"[String]\nRankLevelName=%s\\\nHumanShowName=%chrname\\%guildname\\%dearname\n\
                      [Guild]\nHumanShowName=SHOULD_NOT_WIN\n";
        let c = parse(bytes);
        assert_eq!(c.rank_level_name(), "%s\\");
        assert_eq!(
            c.human_show_name(),
            "%chrname\\%guildname\\%dearname",
            "只认 [String] 段"
        );
        assert_eq!(c.master_name(), DEFAULT_MASTER_NAME, "缺键 ⇒ 源码默认值");
    }

    /// 段名/键名 `OrdinalIgnoreCase`（`ConfigFile.cs:13`）；`;;` 行内注释照 oracle 截断。
    #[test]
    fn section_and_key_are_case_insensitive_and_inline_comment_stripped() {
        let bytes = b"[string]\nhumanshowname=A;;tail\nranklevelname=B ; ; not-comment\n";
        let c = parse(bytes);
        assert_eq!(c.human_show_name(), "A");
        assert_eq!(c.rank_level_name(), "B ; ; not-comment", "只认连续的 ;;");
    }

    /// 现网 `YouPoisoned=您中毒了!!!`（`M2GameSvr/String.conf:31`，GBK 字节
    /// `c4fa d6d0 b6be c1cb 2121 21`）⇒ 取值为中文；缺键回落 `MessageSettings.cs:64`
    /// 源码默认「中毒了!!!」。
    #[test]
    fn parses_deployed_you_poisoned_and_falls_back_to_message_default() {
        let c = parse(b"[String]\nYouPoisoned=\xc4\xfa\xd6\xd0\xb6\xbe\xc1\xcb!!!\n");
        assert_eq!(c.you_poisoned(), "您中毒了!!!");
        let d = parse(b"[String]\n");
        assert_eq!(d.you_poisoned(), DEFAULT_YOU_POISONED);
        assert_eq!(d.you_poisoned(), "中毒了!!!");
    }

    /// 红检：把 `[String]` 段判定改坏（`sec.eq_ignore_ascii_case` → `sec == "String"`）
    /// ⇒ 上面的小写用例必红；此处另钉「缺键回落默认值」。
    #[test]
    fn missing_keys_fall_back_to_oracle_defaults() {
        let c = parse(b"[String]\n");
        assert_eq!(c.rank_level_name(), "{0}\\平民");
        assert_eq!(
            c.revival_recover(),
            "复活戒指生效，体力恢复.",
            "缺键 ⇒ MessageSettings.cs:226 源码默认"
        );
        assert_eq!(
            c.human_show_name(),
            "%chrname\\%guildname\\%dearname\\%mastername"
        );
    }
    /// 现网 `String.conf:244-246` 的三键（GBK；提示文本里是 `%d` 而不是 `{0}`）。
    /// 缺键回落 `MessageSettings.cs:258-260` 的源码缺省，**不是空串**。
    #[test]
    fn parses_deployed_shout_messages() {
        let bytes = b"[String]
YouNeedLevelSendMsg=%d level
ThisMapDisableSendCyCyMsg=X
YouCanSendCyCyLaterMsg=%d s
";
        let c = parse(bytes);
        assert_eq!(c.you_need_level_msg(), "%d level");
        assert_eq!(c.this_map_disable_send_cycy_msg(), "X");
        assert_eq!(c.you_can_send_cycy_later_msg(), "%d s");
        let d = parse(
            b"[String]
Other=1
",
        );
        assert_eq!(d.you_need_level_msg(), DEFAULT_YOU_NEED_LEVEL_MSG);
        assert_eq!(
            d.this_map_disable_send_cycy_msg(),
            DEFAULT_THIS_MAP_DISABLE_SEND_CYCY_MSG
        );
        assert_eq!(
            d.you_can_send_cycy_later_msg(),
            DEFAULT_YOU_CAN_SEND_CYCY_LATER_MSG
        );
    }

    /// **Minor 1（2026-10-21）**：oracle `StringConf.cs:53-61` **不读** `[String] AttackModeOfDear`
    /// （部署件也没有该键）⇒ 本解析器不得读它；其余六个攻击模式键照读。
    ///
    /// 牙：把 `ATTACKMODEOFDEAR` 的映射加回 `parse` ⇒ 第一条断言红（`CUSTOM` 会覆盖默认值）。
    #[test]
    fn attack_mode_of_dear_key_is_not_read_like_oracle() {
        let c = parse(
            b"[String]\nAttackModeOfAll=A\nAttackModeOfPeaceful=P\nAttackModeOfDear=CUSTOM\n\
              AttackModeOfMaster=M\nAttackModeOfGroup=G\nAttackModeOfGuild=Gd\n\
              AttackModeOfRedWhite=R\n",
        );
        assert_eq!(
            c.attack_mode_msg(2),
            Some(DEFAULT_ATTACK_MODE_OF_DEAR),
            "oracle 不读 AttackModeOfDear ⇒ 恒用 MessageSettings.cs:41 默认值"
        );
        assert_eq!(c.attack_mode_msg(0), Some("A"));
        assert_eq!(c.attack_mode_msg(1), Some("P"));
        assert_eq!(c.attack_mode_msg(3), Some("M"));
        assert_eq!(c.attack_mode_msg(4), Some("G"));
        assert_eq!(c.attack_mode_msg(5), Some("Gd"));
        assert_eq!(c.attack_mode_msg(6), Some("R"));
        assert_eq!(c.attack_mode_msg(7), None, "枚举外 ⇒ 一条都不发");
    }

    /// **E-6 BOM 矩阵**：oracle `ConfigFile.Load`（`ConfigFile.cs:263`）的
    /// `StreamReader(..., GB2312)` 做 BOM 探测 ⇒ `String.conf` 的 GBK / UTF-8 BOM /
    /// UTF-16LE BOM 都必须读出同一组中文值。旧实现裸 `encoding_rs::GBK.decode`
    /// 会把 UTF-8 BOM 解成 `U+FEFF` 开头的乱码、UTF-16 解成乱码 ⇒ 本用例必红。
    #[test]
    fn bom_matrix_decodes_string_conf() {
        let text = "[String]\nYouPoisoned=您中毒了!!!\nGuildMsgPrefix=行会:\n";
        let gbk = mir2_script::cp936::encode_gb2312_dotnet(text);
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice(text.as_bytes());
        let mut utf16le = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            utf16le.extend_from_slice(&u.to_le_bytes());
        }
        for (name, bytes) in [
            ("gbk-no-bom", gbk),
            ("utf8-bom", utf8_bom),
            ("utf16le-bom", utf16le),
        ] {
            let c = parse(&bytes);
            assert_eq!(c.you_poisoned(), "您中毒了!!!", "{name}: YouPoisoned");
            assert_eq!(c.guild_msg_prefix(), "行会:", "{name}: GuildMsgPrefix");
        }
    }
}
