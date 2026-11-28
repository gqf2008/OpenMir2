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
/// oracle `MessageSettings.ThrustingOn`（`MessageSettings.cs:48`）——`[String] ThrustingOn` 缺键默认。
pub const DEFAULT_THRUSTING_ON: &str = "启用刺杀剑法";
/// oracle `MessageSettings.HalfMoonOn`（`MessageSettings.cs:50`）——`[String] HalfMoonOn` 缺键默认。
pub const DEFAULT_HALF_MOON_ON: &str = "开启半月弯刀";
/// oracle `MessageSettings.ThrustingOff`（`MessageSettings.cs:49`）。
pub const DEFAULT_THRUSTING_OFF: &str = "关闭刺杀剑法";
/// oracle `MessageSettings.HalfMoonOff`（`MessageSettings.cs:51`）。
pub const DEFAULT_HALF_MOON_OFF: &str = "关闭半月弯刀";
/// oracle `MessageSettings.CrsHitOn`（`MessageSettings.cs:52`）。
pub const DEFAULT_CRS_HIT_ON: &str = "开启光风斩";
/// oracle `MessageSettings.CrsHitOff`（`MessageSettings.cs:53`）。
pub const DEFAULT_CRS_HIT_OFF: &str = "关闭光风斩";
/// oracle `MessageSettings.FireSpiritsSummoned`（`MessageSettings.cs:58`）。
pub const DEFAULT_FIRE_SPIRITS_SUMMONED: &str = "召唤烈火精灵成功...";
/// oracle `MessageSettings.FireSpiritsFail`（`MessageSettings.cs:59`）。
pub const DEFAULT_FIRE_SPIRITS_FAIL: &str = "召唤烈火精灵失败";
/// oracle `MessageSettings.MateDoTooweak`（`MessageSettings.cs:61`）。
pub const DEFAULT_MATE_DO_TOOWEAK: &str = "冲撞力不够!!!";
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
/// **G3**：`[String] OnlineUserFull`（在线人数门第 1 条；缺键回落 `MessageSettings.cs:36`）。
pub const DEFAULT_ONLINE_USER_FULL: &str = "可允许的玩家数量已满";
/// **G1/G3**：`[String] ForceDisConnect`（强制断开收尾；缺键回落 `MessageSettings.cs:33`）。
pub const DEFAULT_FORCE_DISCONNECT: &str = "连接被强行中断!!!";
/// **G1**：`[String] ClientSoftVersionError`（版本门第 1 条；缺键回落 `MessageSettings.cs:31`）。
pub const DEFAULT_CLIENT_SOFT_VERSION_ERROR: &str = "游戏版本错误!!!";
/// **G1**：`[String] DownLoadNewClientSoft`（版本门第 2 条；缺键回落 `MessageSettings.cs:32`）。
pub const DEFAULT_DOWNLOAD_NEW_CLIENT_SOFT: &str = "请到网站上下载最新版本游戏客户端软件。";
/// **G2**：`[String] ClientSoftVersionTooOld`（蓝字；缺键回落 `MessageSettings.cs:34`）。
pub const DEFAULT_CLIENT_SOFT_VERSION_TOO_OLD: &str =
    "您现在使用的客户端软件版本太老了，大量的游戏效果新将无法使用。";
/// **G2**：`[String] DownLoadAndUseNewClient`（红字；缺键回落 `MessageSettings.cs:35`）。
pub const DEFAULT_DOWNLOAD_AND_USE_NEW_CLIENT: &str =
    "为了更好的进行游戏，请下载最新的客户端软件!!!";
/// **G4**：`[String] YouNowIsTryPlayMode`（体验模式第 1 条红字；缺键回落
/// `MessageSettings.cs:37`）。注意：G4 踢支两条是**硬编码**不走 MessageSettings。
pub const DEFAULT_YOU_NOW_IS_TRY_PLAY_MODE: &str =
    "你现在处于测试中，你可以在七级以前使用，但是会限制你的一些功能.";

/// oracle `MessageSettings.RevivalRecoverMsg`（`MessageSettings.cs:226`）——P32 复活成功的
/// `SysMsg(..., Green, Hint)` 文本（`PlayObject.Message.cs:759`）。缺键回落源码默认。
pub const DEFAULT_REVIVAL_RECOVER_MSG: &str = "复活戒指生效，体力恢复.";

/// oracle `MessageSettings.CanotUseDrugOnThisMap`（`MessageSettings.cs:91`；`StringConf.cs:103` 读键
/// `CanotUseDrugOnThisMap`）——`EatItems` 的禁药守卫 `SysMsg(Red)` 文本。
pub const DEFAULT_CANOT_USE_DRUG_ON_THIS_MAP: &str = "此地图不允许使用任何药品!!!";
/// oracle `MessageSettings.WeaponRepairSuccess`（`MessageSettings.cs:199`；`StringConf.cs:209` 读键）
/// ——`RepairWeapon`/`SuperRepairWeapon` 的 `SysMsg(Green)` 文本（IT-14/IT-15）。
pub const DEFAULT_WEAPON_REPAIR_SUCCESS: &str = "武器修复成功...";
/// oracle `MessageSettings.WeaptonMakeLuck`（`MessageSettings.cs:211`；`StringConf.cs:219` 读键
/// `WeaptonMakeLuck`）——祝福油「加幸运/消诅咒成功」的 `SysMsg(Green)` 文本（IT-12）。
pub const DEFAULT_WEAPTON_MAKE_LUCK: &str = "武器被加幸运了...";
/// oracle `MessageSettings.WeaptonNotMakeLuck`（`MessageSettings.cs:212`；`StringConf.cs:220`）
/// ——祝福油未中（`SysMsg(Green)`，IT-12）。
pub const DEFAULT_WEAPTON_NOT_MAKE_LUCK: &str = "无效!!!";
/// oracle `MessageSettings.TheWeaponIsCursed`（`MessageSettings.cs:213`；`StringConf.cs:221`）
/// ——祝福油诅咒支（`SysMsg(Red)`，IT-12）。
pub const DEFAULT_THE_WEAPON_IS_CURSED: &str = "您的武器被诅咒了。";
/// oracle `MessageSettings.WinLottery1Msg..=WinLottery6Msg`（`MessageSettings.cs:204-209`；
/// `StringConf.cs:212-217` 逐键读），idx0 = `WinLottery1Msg`（一等奖）。
pub const DEFAULT_WIN_LOTTERY_MSGS: [&str; 6] = [
    "祝贺您，中了一等奖。",
    "祝贺您，中了二等奖。",
    "祝贺您，中了三等奖。",
    "祝贺您，中了四等奖。",
    "祝贺您，中了五等奖。",
    "祝贺您，中了六等奖。",
];
/// oracle `MessageSettings.NotWinLotteryMsg`（`MessageSettings.cs:210`；`StringConf.cs:218` 读键）
/// ——未中/闸闭的 `SysMsg(Red)` 文本（IT-16）。
pub const DEFAULT_NOT_WIN_LOTTERY_MSG: &str = "等下次机会吧!!!";

/// oracle `MessageSettings.CanotUseItemMsg`（`MessageSettings.cs:129`；`StringConf.cs:154` 读键
/// `CanotUseItemMsg`）——`ClientUseItems` 的 `BoCanUseItem` 守卫（`Operate.cs:685`）。
pub const DEFAULT_CANOT_USE_ITEM_MSG: &str = "当前无法进行此操作!!!";
/// oracle `MessageSettings.CanotDropGoldMsg`（`MessageSettings.cs:126`；`StringConf.cs:151` 读键）
/// ——`ControlDropItem && nGold < CanDropGold` 守卫。
pub const DEFAULT_CANOT_DROP_GOLD_MSG: &str = "太少的金币不允许扔在地上!!!";
/// oracle `MessageSettings.CanotDropInSafeZoneMsg`（`MessageSettings.cs:127`；`StringConf.cs:152` 读键）
/// ——`ClientDropItem/ClientDropGold` 的安全区守卫。
pub const DEFAULT_CANOT_DROP_IN_SAFE_ZONE_MSG: &str = "安全区不允许扔东西在地上!!!";
/// oracle `MessageSettings.CanotDropItemMsg`（`MessageSettings.cs:128`；`StringConf.cs:153` 读键）
/// ——`!IsCanDrop || Envir.Flag.NoThrowItem` 守卫。
pub const DEFAULT_CANOT_DROP_ITEM_MSG: &str = "当前无法进行此操作!!!";

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
    /// `[String] ThrustingOn`（`MessageSettings.ThrustingOn`，`ThrustingOnOff(true)` 的绿字）。
    thrusting_on: Option<String>,
    /// `[String] HalfMoonOn`（`MessageSettings.HalfMoonOn`，`HalfMoonOnOff(true)` 的绿字）。
    half_moon_on: Option<String>,
    /// 卡 `openmir2-warrskill-entry`：`ClientSpellXY` 6 个战士技 case 的文案（`[String]` 同名键）。
    thrusting_off: Option<String>,
    half_moon_off: Option<String>,
    crs_hit_on: Option<String>,
    crs_hit_off: Option<String>,
    fire_spirits_summoned: Option<String>,
    fire_spirits_fail: Option<String>,
    mate_do_tooweak: Option<String>,
    revival_recover: Option<String>,
    // 卡 `openmir2-runtime-sysmsg-text-frames`：带文本的 `SysMsg` 帧（禁药/修复油/彩票）。
    canot_use_drug_on_this_map: Option<String>,
    weapon_repair_success: Option<String>,
    win_lottery_msg1: Option<String>,
    win_lottery_msg2: Option<String>,
    win_lottery_msg3: Option<String>,
    win_lottery_msg4: Option<String>,
    win_lottery_msg5: Option<String>,
    win_lottery_msg6: Option<String>,
    not_win_lottery_msg: Option<String>,
    // 卡 `openmir2-runtime-sysmsg-text-frames`（续）：祝福油三条（IT-12）。
    weapton_make_luck: Option<String>,
    weapton_not_make_luck: Option<String>,
    the_weapon_is_cursed: Option<String>,
    // 卡 `openmir2-menu-ok-text`：`RM_MENU_OK` 的尾文案（四个生产者）。
    canot_use_item_msg: Option<String>,
    canot_drop_gold_msg: Option<String>,
    canot_drop_in_safe_zone_msg: Option<String>,
    canot_drop_item_msg: Option<String>,
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
    // **G3（`PlayObject.cs:1376-1383`）**：在线人数门的两条红字
    // （`StringConf.cs:48/51` 读键，键名与 `MessageSettings` 静态字段同源）。
    online_user_full: Option<String>,
    force_disconnect: Option<String>,
    // **G1（`PlayObject.cs:1325-1327`）**：版本门的前两条红字
    // （`StringConf.cs:46/47` 读键；第 3 条 ForceDisConnect 复用上面已备键）。
    client_soft_version_error: Option<String>,
    download_new_client_soft: Option<String>,
    // **G2（`PlayObject.cs:1334-1336`）**：老客户端分支的蓝+红两条
    // （`StringConf.cs:49/50` 读键；踢支三条复用 G1 已备键）。
    client_soft_version_too_old: Option<String>,
    download_and_use_new_client: Option<String>,
    // **G4（`PlayObject.cs:1407`）**：体验模式提示（`StringConf.cs:52` 读键；
    // 踢支两条是 oracle 源码硬编码，不走本表）。
    you_now_is_try_play_mode: Option<String>,
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

    /// `[String] CanotUseDrugOnThisMap`（`StringConf.cs:103`；`MessageSettings.cs:91`）。
    #[must_use]
    pub fn canot_use_drug_on_this_map(&self) -> &str {
        self.canot_use_drug_on_this_map
            .as_deref()
            .unwrap_or(DEFAULT_CANOT_USE_DRUG_ON_THIS_MAP)
    }

    /// `[String] WeaponRepairSuccess`（`StringConf.cs:209`；`MessageSettings.cs:199`）。
    #[must_use]
    pub fn weapon_repair_success(&self) -> &str {
        self.weapon_repair_success
            .as_deref()
            .unwrap_or(DEFAULT_WEAPON_REPAIR_SUCCESS)
    }

    /// `[String] WeaptonMakeLuck`（`StringConf.cs:219`；`MessageSettings.cs:211`）。
    #[must_use]
    pub fn weapton_make_luck(&self) -> &str {
        self.weapton_make_luck
            .as_deref()
            .unwrap_or(DEFAULT_WEAPTON_MAKE_LUCK)
    }

    /// `[String] WeaptonNotMakeLuck`（`StringConf.cs:220`；`MessageSettings.cs:212`）。
    #[must_use]
    pub fn weapton_not_make_luck(&self) -> &str {
        self.weapton_not_make_luck
            .as_deref()
            .unwrap_or(DEFAULT_WEAPTON_NOT_MAKE_LUCK)
    }

    /// `[String] TheWeaponIsCursed`（`StringConf.cs:221`；`MessageSettings.cs:213`）。
    #[must_use]
    pub fn the_weapon_is_cursed(&self) -> &str {
        self.the_weapon_is_cursed
            .as_deref()
            .unwrap_or(DEFAULT_THE_WEAPON_IS_CURSED)
    }

    /// `[String] WinLottery1Msg..=WinLottery6Msg`（`StringConf.cs:212-217`），idx0 = 一等奖。
    /// 逐键缺省：缺哪个用哪个的源码默认（`MessageSettings.cs:204-209`）。
    #[must_use]
    pub fn win_lottery_msgs(&self) -> [&str; 6] {
        let slots = [
            &self.win_lottery_msg1,
            &self.win_lottery_msg2,
            &self.win_lottery_msg3,
            &self.win_lottery_msg4,
            &self.win_lottery_msg5,
            &self.win_lottery_msg6,
        ];
        // 显式写成 `[&str; 6]`（不是从 `'static` 常量推出的 `[&'static str; 6]`）：
        // 命中键时要把 `&self` 生命周期的值填进去。
        let mut out: [&str; 6] = DEFAULT_WIN_LOTTERY_MSGS;
        for (slot, default) in slots.iter().zip(out.iter_mut()) {
            if let Some(v) = slot.as_deref() {
                *default = v;
            }
        }
        out
    }

    /// `[String] NotWinLotteryMsg`（`StringConf.cs:218`；`MessageSettings.cs:210`）。
    #[must_use]
    pub fn not_win_lottery_msg(&self) -> &str {
        self.not_win_lottery_msg
            .as_deref()
            .unwrap_or(DEFAULT_NOT_WIN_LOTTERY_MSG)
    }

    /// `[String] ThrustingOn`（`ThrustingOnOff(true)` 的绿字，IT-17a 学习刺杀剑术）。
    #[must_use]
    pub fn thrusting_on(&self) -> &str {
        self.thrusting_on.as_deref().unwrap_or(DEFAULT_THRUSTING_ON)
    }

    /// `[String] HalfMoonOn`（`HalfMoonOnOff(true)` 的绿字，IT-17a 学习半月弯刀）。
    #[must_use]
    pub fn half_moon_on(&self) -> &str {
        self.half_moon_on.as_deref().unwrap_or(DEFAULT_HALF_MOON_ON)
    }

    /// `[String] ThrustingOff`（`ThrustingOnOff(false)` 的绿字）。
    #[must_use]
    pub fn thrusting_off(&self) -> &str {
        self.thrusting_off
            .as_deref()
            .unwrap_or(DEFAULT_THRUSTING_OFF)
    }

    /// `[String] HalfMoonOff`。
    #[must_use]
    pub fn half_moon_off(&self) -> &str {
        self.half_moon_off
            .as_deref()
            .unwrap_or(DEFAULT_HALF_MOON_OFF)
    }

    /// `[String] CrsHitOn`（34 逐日剑法开）。
    #[must_use]
    pub fn crs_hit_on(&self) -> &str {
        self.crs_hit_on.as_deref().unwrap_or(DEFAULT_CRS_HIT_ON)
    }

    /// `[String] CrsHitOff`（34 逐日剑法关）。
    #[must_use]
    pub fn crs_hit_off(&self) -> &str {
        self.crs_hit_off.as_deref().unwrap_or(DEFAULT_CRS_HIT_OFF)
    }

    /// `[String] FireSpiritsSummoned`（26 烈火剑法充能成功）。
    #[must_use]
    pub fn fire_spirits_summoned(&self) -> &str {
        self.fire_spirits_summoned
            .as_deref()
            .unwrap_or(DEFAULT_FIRE_SPIRITS_SUMMONED)
    }

    /// `[String] FireSpiritsFail`（26 的 10s 门内失败）。
    #[must_use]
    pub fn fire_spirits_fail(&self) -> &str {
        self.fire_spirits_fail
            .as_deref()
            .unwrap_or(DEFAULT_FIRE_SPIRITS_FAIL)
    }

    /// `[String] MateDoTooweak`（27 冲撞失败的红字）。
    #[must_use]
    pub fn mate_do_tooweak(&self) -> &str {
        self.mate_do_tooweak
            .as_deref()
            .unwrap_or(DEFAULT_MATE_DO_TOOWEAK)
    }

    /// `[String] CanotUseItemMsg`（`StringConf.cs:154`；`MessageSettings.cs:129`）。
    #[must_use]
    pub fn canot_use_item_msg(&self) -> &str {
        self.canot_use_item_msg
            .as_deref()
            .unwrap_or(DEFAULT_CANOT_USE_ITEM_MSG)
    }

    /// `[String] CanotDropGoldMsg`（`StringConf.cs:151`）。
    #[must_use]
    pub fn canot_drop_gold_msg(&self) -> &str {
        self.canot_drop_gold_msg
            .as_deref()
            .unwrap_or(DEFAULT_CANOT_DROP_GOLD_MSG)
    }

    /// `[String] CanotDropInSafeZoneMsg`（`StringConf.cs:152`）。
    #[must_use]
    pub fn canot_drop_in_safe_zone_msg(&self) -> &str {
        self.canot_drop_in_safe_zone_msg
            .as_deref()
            .unwrap_or(DEFAULT_CANOT_DROP_IN_SAFE_ZONE_MSG)
    }

    /// `[String] CanotDropItemMsg`（`StringConf.cs:153`）。
    #[must_use]
    pub fn canot_drop_item_msg(&self) -> &str {
        self.canot_drop_item_msg
            .as_deref()
            .unwrap_or(DEFAULT_CANOT_DROP_ITEM_MSG)
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

    /// `[String] OnlineUserFull`（G3 在线人数门第 1 条；缺键回落 `MessageSettings.cs:36`）。
    #[must_use]
    pub fn online_user_full(&self) -> &str {
        self.online_user_full
            .as_deref()
            .unwrap_or(DEFAULT_ONLINE_USER_FULL)
    }

    /// `[String] ForceDisConnect`（G1/G3 的强制断开收尾；缺键回落 `MessageSettings.cs:33`）。
    #[must_use]
    pub fn force_disconnect(&self) -> &str {
        self.force_disconnect
            .as_deref()
            .unwrap_or(DEFAULT_FORCE_DISCONNECT)
    }

    /// `[String] ClientSoftVersionError`（G1 第 1 条；缺键回落 `MessageSettings.cs:31`）。
    #[must_use]
    pub fn client_soft_version_error(&self) -> &str {
        self.client_soft_version_error
            .as_deref()
            .unwrap_or(DEFAULT_CLIENT_SOFT_VERSION_ERROR)
    }

    /// `[String] DownLoadNewClientSoft`（G1 第 2 条；缺键回落 `MessageSettings.cs:32`）。
    #[must_use]
    pub fn download_new_client_soft(&self) -> &str {
        self.download_new_client_soft
            .as_deref()
            .unwrap_or(DEFAULT_DOWNLOAD_NEW_CLIENT_SOFT)
    }

    /// `[String] ClientSoftVersionTooOld`（G2 蓝字；缺键回落 `MessageSettings.cs:34`）。
    #[must_use]
    pub fn client_soft_version_too_old(&self) -> &str {
        self.client_soft_version_too_old
            .as_deref()
            .unwrap_or(DEFAULT_CLIENT_SOFT_VERSION_TOO_OLD)
    }

    /// `[String] DownLoadAndUseNewClient`（G2 红字；缺键回落 `MessageSettings.cs:35`）。
    #[must_use]
    pub fn download_and_use_new_client(&self) -> &str {
        self.download_and_use_new_client
            .as_deref()
            .unwrap_or(DEFAULT_DOWNLOAD_AND_USE_NEW_CLIENT)
    }

    /// `[String] YouNowIsTryPlayMode`（G4 第 1 条；缺键回落 `MessageSettings.cs:37`）。
    #[must_use]
    pub fn you_now_is_try_play_mode(&self) -> &str {
        self.you_now_is_try_play_mode
            .as_deref()
            .unwrap_or(DEFAULT_YOU_NOW_IS_TRY_PLAY_MODE)
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
/// `mir2_script::cp936::decode_config_text`（E-6）。
///
/// 旧实现的 `encoding_rs::GBK.decode` **自带 BOM 嗅探**（UTF-8 / UTF-16LE / UTF-16BE），
/// BOM 面本就一致；真实差异在 ① cp936 精确字符集（`0xFF→U+F8F5`、`A8 BF→U+E7C8`、
/// 非法双字节对吞成 `?`）与 ② `FF FE 00 00` 的 UTF-32LE 分支（encoding_rs 无 UTF-32 嗅探，
/// 会按 UTF-16LE 解）。牙见 `cp936_divergences_and_configfile_utf32le_model`。
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
            (Section::String, "THRUSTINGON") => &mut conf.thrusting_on,
            (Section::String, "HALFMOONON") => &mut conf.half_moon_on,
            (Section::String, "THRUSTINGOFF") => &mut conf.thrusting_off,
            (Section::String, "HALFMOONOFF") => &mut conf.half_moon_off,
            (Section::String, "CRSHITON") => &mut conf.crs_hit_on,
            (Section::String, "CRSHITOFF") => &mut conf.crs_hit_off,
            (Section::String, "FIRESPIRITSSUMMONED") => &mut conf.fire_spirits_summoned,
            (Section::String, "FIRESPIRITSFAIL") => &mut conf.fire_spirits_fail,
            (Section::String, "MATEDOTOWEAK") => &mut conf.mate_do_tooweak,
            (Section::String, "REVIVALRECOVERMSG") => &mut conf.revival_recover,
            (Section::String, "CANOTUSEDRUGONTHISMAP") => &mut conf.canot_use_drug_on_this_map,
            (Section::String, "WEAPONREPAIRSUCCESS") => &mut conf.weapon_repair_success,
            (Section::String, "WINLOTTERY1MSG") => &mut conf.win_lottery_msg1,
            (Section::String, "WINLOTTERY2MSG") => &mut conf.win_lottery_msg2,
            (Section::String, "WINLOTTERY3MSG") => &mut conf.win_lottery_msg3,
            (Section::String, "WINLOTTERY4MSG") => &mut conf.win_lottery_msg4,
            (Section::String, "WINLOTTERY5MSG") => &mut conf.win_lottery_msg5,
            (Section::String, "WINLOTTERY6MSG") => &mut conf.win_lottery_msg6,
            (Section::String, "NOTWINLOTTERYMSG") => &mut conf.not_win_lottery_msg,
            (Section::String, "WEAPTONMAKELUCK") => &mut conf.weapton_make_luck,
            (Section::String, "WEAPTONNOTMAKELUCK") => &mut conf.weapton_not_make_luck,
            (Section::String, "THEWEAPONISCURSED") => &mut conf.the_weapon_is_cursed,
            (Section::String, "CANOTUSEITEMMSG") => &mut conf.canot_use_item_msg,
            (Section::String, "CANOTDROPGOLDMSG") => &mut conf.canot_drop_gold_msg,
            (Section::String, "CANOTDROPINSAFEZONEMSG") => &mut conf.canot_drop_in_safe_zone_msg,
            (Section::String, "CANOTDROPITEMMSG") => &mut conf.canot_drop_item_msg,
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
            (Section::String, "ONLINEUSERFULL") => &mut conf.online_user_full,
            (Section::String, "FORCEDISCONNECT") => &mut conf.force_disconnect,
            (Section::String, "CLIENTSOFTVERSIONERROR") => &mut conf.client_soft_version_error,
            (Section::String, "DOWNLOADNEWCLIENTSOFT") => &mut conf.download_new_client_soft,
            (Section::String, "CLIENTSOFTVERSIONTOOOLD") => &mut conf.client_soft_version_too_old,
            (Section::String, "DOWNLOADANDUSENEWCLIENT") => &mut conf.download_and_use_new_client,
            (Section::String, "YOUNOWISTRYPLAYMODE") => &mut conf.you_now_is_try_play_mode,
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
    /// UTF-16LE BOM 都必须读出同一组中文值。
    ///
    /// 注意：本用例**不是**回退牙——旧的 `encoding_rs::GBK.decode` 自带 BOM 嗅探
    /// （UTF-8 / UTF-16LE / UTF-16BE），这三份夹具在旧实现下同样绿。真牙见
    /// [`cp936_divergences_and_configfile_utf32le_model`]。
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

    /// **E-6 真牙（cp936 精确字符集 + 两套 BOM 模型）**。
    ///
    /// 1. `0xFF → U+F8F5`、`A8 BF → U+E7C8`：.NET cp936 的定向映射；旧实现的
    ///    `encoding_rs::GBK.decode` 分别给 `U+FFFD` / `U+01F9` ⇒ 回退必红。
    /// 2. `FF FE 00 00 41 00 00 00`：`ConfigFile.Load`（本文件走 `decode_config_text`）
    ///    按 **UTF-32LE** 解出 `A`；`StringList.LoadFromFile`（`decode_string_list`）按
    ///    **UTF-16LE** 解出 `U+0000 A U+0000`。两套模型不可混用；`encoding_rs` 没有
    ///    UTF-32 嗅探（会按 UTF-16LE）⇒ 回退亦红。
    #[test]
    fn cp936_divergences_and_configfile_utf32le_model() {
        let c = parse(b"[String]\nYouPoisoned=\xff\xa8\xbf\n");
        assert_eq!(
            c.you_poisoned(),
            "\u{F8F5}\u{E7C8}",
            "cp936：0xFF=U+F8F5、A8 BF=U+E7C8（WHATWG 给 U+FFFD / U+01F9）"
        );

        let raw = [0xFF, 0xFE, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00];
        assert_eq!(
            mir2_script::cp936::decode_config_text(&raw),
            "A",
            "ConfigFile.Load：FF FE 00 00 = UTF-32LE"
        );
        assert_eq!(
            mir2_script::cp936::decode_string_list(&raw),
            "\u{0000}A\u{0000}",
            "StringList.LoadFromFile：FF FE 00 00 = UTF-16LE（U+0000 U+0041 U+0000）"
        );

        // 本文件实际加载器口径（ConfigFile / UTF-32LE）必须端到端解析；
        // 若按 StringList 的 UTF-16LE 模型读，键全被 NUL 打断 ⇒ 落回源码默认值。
        let text = "[String]\nYouPoisoned=测试值\n";
        let mut utf32le = vec![0xFF, 0xFE, 0x00, 0x00];
        for ch in text.chars() {
            utf32le.extend_from_slice(&u32::from(ch).to_le_bytes());
        }
        let c = parse(&utf32le);
        assert_eq!(
            c.you_poisoned(),
            "测试值",
            "String.conf 走 ConfigFile.Load 的 UTF-32LE 分支（不是 StringList 的 UTF-16LE）"
        );
    }

    /// 卡 `openmir2-runtime-sysmsg-text-frames`：`[String]` 的 `SysMsg` 文本键——
    /// 键名逐字照 `StringConf.cs:103`（`CanotUseDrugOnThisMap`）/`:209`（`WeaponRepairSuccess`）/
    /// `:212-217`（`WinLottery1Msg..6Msg`）/`:218`（`NotWinLotteryMsg`）；缺键 = `MessageSettings.cs`
    /// 的源码默认（`:91` / `:199` / `:204-209` / `:210`）。
    ///
    /// 输入字节是 **Windows cp936 的实际编码**（由独立来源生成，不是本仓编码器自证）
    /// ⇒ 同时校验 cp936 解码；末尾再断言 `gbk()` 编回的是**同一串字节**（卡 §3-3
    /// 「编码路径只有一条」：配置层 decode 一次、出帧 encode 一次，无中转）。
    #[test]
    fn parses_sysmsg_text_keys_from_cp936_and_falls_back_per_key() {
        let repair_gbk: &[u8] = b"\xce\xe4\xc6\xf7\xd0\xde\xb8\xb4\xb3\xc9\xb9\xa6...";
        let drug_gbk: &[u8] =
            b"\xb4\xcb\xb5\xd8\xcd\xbc\xb2\xbb\xd4\xca\xd0\xed\xca\xb9\xd3\xc3\xc8\xce\xba\xce\xd2\xa9\xc6\xb7!!!";
        let win3_gbk: &[u8] =
            b"\xd7\xa3\xba\xd8\xc4\xfa\xa3\xac\xd6\xd0\xc1\xcb\xc8\xfd\xb5\xc8\xbd\xb1\xa1\xa3";
        let not_win_gbk: &[u8] = b"\xb5\xc8\xcf\xc2\xb4\xce\xbb\xfa\xbb\xe1\xb0\xc9!!!";

        let mut bytes = b"[String]\nWeaponRepairSuccess=".to_vec();
        bytes.extend_from_slice(repair_gbk);
        bytes.extend_from_slice(b"\nCanotUseDrugOnThisMap=");
        bytes.extend_from_slice(drug_gbk);
        bytes.extend_from_slice(b"\nWinLottery3Msg=");
        bytes.extend_from_slice(win3_gbk);
        bytes.extend_from_slice(b"\nNotWinLotteryMsg=");
        bytes.extend_from_slice(not_win_gbk);
        bytes.push(b'\n');

        let c = parse(&bytes);
        assert_eq!(c.weapon_repair_success(), "武器修复成功...");
        assert_eq!(
            c.canot_use_drug_on_this_map(),
            "此地图不允许使用任何药品!!!"
        );
        assert_eq!(c.not_win_lottery_msg(), "等下次机会吧!!!");
        // 逐键缺省：只有 3 等奖给了值 ⇒ 其余 5 档各自回落默认（不是整组一起回）。
        assert_eq!(
            c.win_lottery_msgs(),
            [
                DEFAULT_WIN_LOTTERY_MSGS[0],
                DEFAULT_WIN_LOTTERY_MSGS[1],
                "祝贺您，中了三等奖。",
                DEFAULT_WIN_LOTTERY_MSGS[3],
                DEFAULT_WIN_LOTTERY_MSGS[4],
                DEFAULT_WIN_LOTTERY_MSGS[5],
            ]
        );
        // 编码路径只有一条：读进来的 cp936 字节，出帧时 `gbk()` 编回**同一串**。
        assert_eq!(
            crate::downlink::gbk(c.weapon_repair_success()),
            repair_gbk,
            "cp936 → String → cp936 往返逐字节相同"
        );

        // 缺键用例：全缺 ⇒ 全默认（等于 `MessageSettings` 源码默认值）。
        let d = parse(b"[String]\n");
        assert_eq!(d.weapon_repair_success(), "武器修复成功...");
        assert_eq!(
            d.canot_use_drug_on_this_map(),
            "此地图不允许使用任何药品!!!"
        );
        assert_eq!(d.not_win_lottery_msg(), "等下次机会吧!!!");
        assert_eq!(d.win_lottery_msgs(), DEFAULT_WIN_LOTTERY_MSGS);
        assert_eq!(
            d.win_lottery_msgs()[5],
            "祝贺您，中了六等奖。",
            "缺键默认与 `MessageSettings.cs:209` 逐字一致"
        );
        // **缺键路径也走同一串字节**：源码默认值（Rust 字面量）经 `gbk()` 编出的必须是
        // oracle 会写上线缆的同一串 cp936（不是"字面量又转了一次"的另一种字节）。
        assert_eq!(
            crate::downlink::gbk(d.weapon_repair_success()),
            repair_gbk,
            "缺键默认值的 cp936 字节与部署值同串"
        );
    }

    /// **G3 文本键牙**：`[String] OnlineUserFull/ForceDisConnect` 逐键解析 + 逐键回落
    /// （`StringConf.cs:51/48` 读键；缺键回落 `MessageSettings.cs:36/33` 源码默认）。
    ///
    /// 红检（实测）：删 parse 里任一对应臂 ⇒ 该键断言红；把两臂接反 ⇒ 两断言同时红。
    #[test]
    fn parses_g3_deny_keys_and_falls_back_per_key() {
        // cp936 实编码（独立来源生成，同 `parses_sysmsg_text_keys…` 的口径）。
        let full_gbk: &[u8] =
            b"\xbf\xc9\xd4\xca\xd0\xed\xb5\xc4\xcd\xe6\xbc\xd2\xca\xfd\xc1\xbf\xd2\xd1\xc2\xfa";
        let kick_gbk: &[u8] = b"\xc1\xac\xbd\xd3\xb1\xbb\xc7\xbf\xd0\xd0\xd6\xd0\xb6\xcf!!!";

        let mut bytes = b"[String]".to_vec();
        bytes.extend_from_slice(b"\nOnlineUserFull=");
        bytes.extend_from_slice(full_gbk);
        bytes.extend_from_slice(b"\nForceDisConnect=");
        bytes.extend_from_slice(kick_gbk);
        bytes.push(b'\n');
        let c = parse(&bytes);
        assert_eq!(c.online_user_full(), "可允许的玩家数量已满");
        assert_eq!(c.force_disconnect(), "连接被强行中断!!!");
        // 编码路径只有一条：读进来的 cp936 字节，出帧 `gbk()` 编回同一串。
        assert_eq!(crate::downlink::gbk(c.online_user_full()), full_gbk);
        assert_eq!(crate::downlink::gbk(c.force_disconnect()), kick_gbk);

        // 缺键 ⇒ 全默认（等于 `MessageSettings` 源码默认值）。
        let d = parse(b"[String]".to_vec().as_slice());
        assert_eq!(d.online_user_full(), "可允许的玩家数量已满");
        assert_eq!(d.force_disconnect(), "连接被强行中断!!!");
    }

    /// **G1 文本键牙**：`[String] ClientSoftVersionError/DownLoadNewClientSoft` 逐键解析
    /// + 逐键回落（`StringConf.cs:46/47` 读键；缺键回落 `MessageSettings.cs:31/32`）。
    ///
    /// 红检：删 parse 任一对应臂 ⇒ 该键断言红；两臂接反 ⇒ 两断言同红。
    #[test]
    fn parses_g1_version_keys_and_falls_back_per_key() {
        // cp936 实编码（独立来源生成，同 `parses_sysmsg_text_keys…` 的口径）。
        let err_gbk: &[u8] = b"\xd3\xce\xcf\xb7\xb0\xe6\xb1\xbe\xb4\xed\xce\xf3\x21\x21\x21";
        let dl_gbk: &[u8] = b"\xc7\xeb\xb5\xbd\xcd\xf8\xd5\xbe\xc9\xcf\xcf\xc2\xd4\xd8\xd7\xee\xd0\xc2\xb0\xe6\xb1\xbe\xd3\xce\xcf\xb7\xbf\xcd\xbb\xa7\xb6\xcb\xc8\xed\xbc\xfe\xa1\xa3";
        let mut bytes = b"[String]".to_vec();
        bytes.extend_from_slice(b"\nClientSoftVersionError=");
        bytes.extend_from_slice(err_gbk);
        bytes.extend_from_slice(b"\nDownLoadNewClientSoft=");
        bytes.extend_from_slice(dl_gbk);
        bytes.push(b'\n');
        let c = parse(&bytes);
        assert_eq!(c.client_soft_version_error(), "游戏版本错误!!!");
        assert_eq!(
            c.download_new_client_soft(),
            "请到网站上下载最新版本游戏客户端软件。"
        );
        assert_eq!(crate::downlink::gbk(c.client_soft_version_error()), err_gbk);
        assert_eq!(crate::downlink::gbk(c.download_new_client_soft()), dl_gbk);
        let d = parse(b"[String]".to_vec().as_slice());
        assert_eq!(d.client_soft_version_error(), "游戏版本错误!!!");
        assert_eq!(
            d.download_new_client_soft(),
            "请到网站上下载最新版本游戏客户端软件。"
        );
    }
    /// **G2 文本键牙**：`[String] ClientSoftVersionTooOld/DownLoadAndUseNewClient` 逐键解析
    /// + 逐键回落（`StringConf.cs:49/50` 读键；缺键回落 `MessageSettings.cs:34/35`）。
    ///
    /// 红检：删 parse 任一臂 ⇒ 该键断言红。
    #[test]
    fn parses_g2_old_client_keys_and_falls_back_per_key() {
        let too_gbk: &[u8] = b"\xc4\xfa\xcf\xd6\xd4\xda\xca\xb9\xd3\xc3\xb5\xc4\xbf\xcd\xbb\xa7\xb6\xcb\xc8\xed\xbc\xfe\xb0\xe6\xb1\xbe\xcc\xab\xc0\xcf\xc1\xcb\xa3\xac\xb4\xf3\xc1\xbf\xb5\xc4\xd3\xce\xcf\xb7\xd0\xa7\xb9\xfb\xd0\xc2\xbd\xab\xce\xde\xb7\xa8\xca\xb9\xd3\xc3\xa1\xa3";
        let dl_gbk: &[u8] = b"\xce\xaa\xc1\xcb\xb8\xfc\xba\xc3\xb5\xc4\xbd\xf8\xd0\xd0\xd3\xce\xcf\xb7\xa3\xac\xc7\xeb\xcf\xc2\xd4\xd8\xd7\xee\xd0\xc2\xb5\xc4\xbf\xcd\xbb\xa7\xb6\xcb\xc8\xed\xbc\xfe\x21\x21\x21";
        let mut bytes = b"[String]".to_vec();
        bytes.extend_from_slice(b"\nClientSoftVersionTooOld=");
        bytes.extend_from_slice(too_gbk);
        bytes.extend_from_slice(b"\nDownLoadAndUseNewClient=");
        bytes.extend_from_slice(dl_gbk);
        bytes.push(b'\n');
        let c = parse(&bytes);
        assert_eq!(
            c.client_soft_version_too_old(),
            "您现在使用的客户端软件版本太老了，大量的游戏效果新将无法使用。"
        );
        assert_eq!(
            c.download_and_use_new_client(),
            "为了更好的进行游戏，请下载最新的客户端软件!!!"
        );
        assert_eq!(
            crate::downlink::gbk(c.client_soft_version_too_old()),
            too_gbk
        );
        assert_eq!(
            crate::downlink::gbk(c.download_and_use_new_client()),
            dl_gbk
        );
        let d = parse(b"[String]".to_vec().as_slice());
        assert_eq!(
            d.client_soft_version_too_old(),
            crate::strings::DEFAULT_CLIENT_SOFT_VERSION_TOO_OLD
        );
        assert_eq!(
            d.download_and_use_new_client(),
            crate::strings::DEFAULT_DOWNLOAD_AND_USE_NEW_CLIENT
        );
    }
}
