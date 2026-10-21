//! 入图帧的**世界/配置层输入装配**（`openmir2-m2-enter-world-frames` 的判据②「生产路径」层）。
//!
//! 编码层（`game_svr::smframe::EnterWorldInputs::frames()`）已逐字节对上 M0 金标准；本模块
//! 只做一件事：把 **Envir 真地图的 `Envir.Flag`/`MapDesc`**（`crates/envir`，`openmir2-m2-envir-inputs`）
//! 与 **`Server.conf` 配置层**（同卡）以及**角色记录全字段**（`record.rs`，`openmir2-m2-record-fields`）
//! 装配成 `EnterWorldInputs` 的 `Some` 字段。缺输入的帧**保持 `None`（跳过并登记），绝不猜尾字节**。
//!
//! 逐条对照 oracle（字段序与门都按行号，不按公式重推）：
//! - `766 SM_AREASTATE` ← `RefUserState()`（`PlayObject.Base.cs:270-286`）：`n8 = FightZone|1 | SafeArea|2 | InGuildWarArea|4`；
//! - `54 SM_MAPDESCRIPTION` ← `SendMapDescription()`（`:887-895`）：`nMusicid = Flag.Music ? Flag.MusicId : -1`，尾 = `MapDesc`；
//! - `46 SM_DAYCHANGING` ← `RM_DAYCHANGING`（`PlayObject.Message.cs:1741`）：`Bright = M2Share.GameTime`，`DayBright()`（`Base.cs:244-269`）；
//! - `11029 SM_SERVERCONFIG` ← `SendServerConfig()`（`Base.cs:941-971`）：`SoftVersionDateEx == 0` 时**直接 return**；
//! - `5008 SM_GAMEGOLDNAME` ← `SendGoldInfo(true)`（`Base.cs:921-939`）：同样门 `SoftVersionDateEx == 0`；
//! - `1106 SM_VERSION_FAIL` ← `RM_LOGON`（`Message.cs:1690-1692`）：**无门**，尾恒 `"<<<<<<"`，CRC 来自 `Config`（恒 0）；
//! - `811 SM_ADJUST_BONUS` ← `SendAdjustBonus()`（`Base.cs:1193-1210`）：按 `Job` 取 `Config.BonusAbilofXxx` / 角色 `BonusAbil` / `Config.NakedAbilofXxx`。
//!
//! **本片已接线**：`42 SM_USERNAME`（`GetShowName()` + `String.conf` 模板）、`46 SM_DAYCHANGING`
//! （`Bright = M2Share.GameTime` 按**真实本地小时**算，`SystemProcessor.cs:98-134` 的 `GetGameTime` 1:1），
//! 以及 `50 SM_LOGON` / `41 SM_FEATURECHANGED` / `213 SM_ATTACKMODE` **同组**三条
//! （`SendLogon()`，`PlayObject.cs:1128-1147`；`GetFeatureToLong`/`GetFeatureEx` 复用已合入的
//! `game_svr::feature`）。
//!
//! **本片已接线（52/752）**：`52 SM_ABILITY` / `752 SM_SUBABILITY`——发帧点 = 登录后
//! `PlayObject.cs:1385-1386`（`SendPriorityMsg(RM_ABILITY/RM_SUBABILITY)` → `Message.cs:1732-1735` /
//! `:1935`），输入由 **已合入的 `game_svr::ability::recalc_abilitys`**（`PlayObject.RecalcAbilitys()` 的
//! 纯函数等价物，`openmir2-m3-bag-equip-alignment` 交付）产出：`WAbil` → `record::encode_ability` →
//! `smframe::sm_ability`；`SubAbility::to_frame_bytes()` → `smframe::sm_subability`。
//!
//! **仍缺输入（本片登记，不接线）**：
//! - `50/41` 的 `UseItems[Dress/Weapon].StdItem.Shape` **已接线**（查表只有一份：`game_svr::ability`
//!   的 `StdItemCatalog`，由 `crate::items::catalog_from` 从直连 MySQL 读到的 `stditems` 顺序表构造，
//!   1-based + 空名按缺）；仅 `CharStatus` 的 buff 部分（`ActorBuffSystem`）未移植，会话初生恒 0
//!   （金标准 #3 印证）。
//! - `10 SM_TURN`：oracle **对自身不发的**（`PlayObject.cs:3234` 的 `if (baseObject != this)` 门）；
//!   金标准 #16 的 `Recog` 是**同屏另一个 actor**（msg = `稻草人/255`）而非本会话 `ActorId`
//!   ⇒ 这条帧属**世界侧 AOI/同屏 actor 展示**路径，不在「自身进世界」这条链上（台账 §3 已更正）。
//!
//! `GameTimeChanged()`（`PlayObject.Base.cs:824-830`，游戏时间变化时广播 `RM_DAYCHANGING`）的
//! **触发点**属世界 tick 侧（未接，登记）；本片只实现登录时那一条 46。

use mir2_envir::{GameSvrConf, MapInfoFlag, NakedAbility as ConfNakedAbility};

use crate::record::{Ability, MagicRcd, NakedAbility, ServerUserItem};
use crate::strings::StringConf;
use game_svr::ability::{self, ItemCatalog, ItemInstance, RecalcInputs};
use game_svr::feature::{feature_ex, human_feature, make_long, make_word};
use game_svr::smframe::EnterWorldInputs;

/// 装配入图帧额外输入所需的世界/配置层数据（`None` = 该层未就绪，对应帧**跳过并登记**）。
#[derive(Clone, Copy, Default)]
pub struct WorldFrameInputs<'a> {
    /// 当前地图的 `Envir.Flag`（`MapRegistry::flag_of(map_id)`；无 Envir 时 `None`）。
    pub flag: Option<&'a MapInfoFlag>,
    /// 当前地图的 `Envir.MapDesc`（`MapRegistry::map_desc_of(map_id)`）。
    pub map_desc: Option<&'a str>,
    /// `Server.conf` 的配置层（`GameSvrConf::load`；读不到时 `None`）。
    pub conf: Option<&'a GameSvrConf>,
    /// 角色记录的 `BonusAbil`（帧 811 的 B 段；`CharacterData.bonus_abil`）。
    pub bonus_abil: Option<&'a NakedAbility>,
    /// oracle `PlayObject.SoftVersionDateEx`（= `LoadDBInfo.SoftVersionDate`，客户端登录版本）。
    /// `0` ⇒ 帧 11029/5008 不发（`Base.cs:930/943` 的 `if (SoftVersionDateEx == 0) return;`）。
    pub soft_version_date: i32,
    /// 帧 5008 的 `GameGold`（角色记录 `game_gold`）。
    pub game_gold: i32,
    /// 帧 5008 的 `GamePoint`（角色记录 `game_point`）。
    pub game_point: i32,
    /// 帧 811 的 `Job`（角色记录 `job`，`PlayerJob`：0 战士 / 1 法师 / 2 道士）。
    pub job: u8,
    /// 帧 811 的 `BonusPoint`（角色记录 `bonus_point`）。
    pub bonus_point: i32,
    /// oracle `M2Share.GameTime`（世界时钟）。未实现 ⇒ `None`。
    pub game_time: Option<u8>,
    /// `String.conf` 的展示名模板（帧 42 的 `GetShowName()`，见 [`show_name`]）。
    pub strings: Option<&'a StringConf>,
    /// 帧 42 的展示名输入（`None` ⇒ 帧 42 跳过并登记）。
    pub show_name: Option<ShowNameInputs<'a>>,
    /// 帧 50/41/213 的输入（`PlayObject.SendLogon()`，`PlayObject.cs:1128-1147` 同组连发三条）。
    /// `None`（未解析出角色记录全字段）⇒ 三条同组跳过并登记，绝不单发半组。
    pub logon: Option<LogonInputs>,
    /// 帧 52/752 的输入（`PlayObject.cs:1385-1386` 登录后的 `SendPriorityMsg(RM_ABILITY/RM_SUBABILITY)`）。
    /// `None` ⇒ 两条同组跳过并登记（`RecalcAbilitys` 的输入接缝未就绪）。
    pub ability: Option<AbilityInputs<'a>>,
    /// `[Setup] GreenMsgFColor/GreenMsgBColor`（登录 `SysMsg(..., MsgColor.Green, ...)` 的
    /// `Param = make_word(f, b)`；`Server.conf` 现网 219/255 ⇒ `Param=0xFFDB`）。
    pub green_msg: (u8, u8),
    /// `[Server] TestServer`（`ServerConf.cs:23`）：第三条登录 `SysMsg`（`StartNoticeMsg`）的门。
    pub test_server: bool,
    /// `[Server] ServiceMode`（`ServerConf.cs:27`）：与 `TestServer` 一同把 `PayMent` 覆盖成 3。
    pub service_mode: bool,
    /// `LoadDBInfo.PayMent`（`PlayObject.cs:1145-1148` 覆盖前的值；`TestServer||ServiceMode` ⇒ 3）。
    pub pay_ment: i32,
}

/// `PlayObject.RecalcAbilitys()`（`BaseObject.Abil.cs:15` + `PlayObject.cs:2008`）的一次输入。
///
/// 装备/卸下层的纯函数是**已合入的** [`game_svr::ability::recalc_abilitys`]（m3bag 交付），本结构只
/// 负责把**角色记录 + `stditems` 目录**接进去（`brief-m2svr-next.md` §18：禁止复制第二份实现）。
#[derive(Clone, Copy)]
pub struct AbilityInputs<'a> {
    /// `stditems` 查表目录（`crate::items::catalog_from`；oracle `SystemShare.ItemSystem`）。
    pub catalog: &'a dyn ItemCatalog,
    /// `BaseObject.Abil`（角色记录 `CharacterData.abil`）。
    pub abil: &'a Ability,
    /// `PlayObject.BonusAbil`（角色记录 `CharacterData.bonus_abil`）。
    pub bonus_abil: &'a NakedAbility,
    /// `Config.BonusAbilofXxx.Hit`（`RecalcHitSpeed` 的除数；见 [`bonus_tick`]）。
    pub bonus_tick_hit: u8,
    /// `Config.BonusAbilofXxx.Speed`（同上）。
    pub bonus_tick_speed: i32,
    /// `UseItems[13]`（角色记录 `hum_items`）。
    pub use_items: Option<&'a [Option<ServerUserItem>]>,
    /// 背包物品（角色记录 `bag_items`；只用于 `WAbil.Weight`）。
    pub bag_items: Option<&'a [Option<ServerUserItem>]>,
    /// `PlayObject.MagicList`（角色记录 `magic`）：`SM_SENDMYMAGIC(211)` 的逐条输入。
    pub magics: Option<&'a [Option<MagicRcd>]>,
    /// `M2Share.MagicList` 查表面（`magics::load_magics`）：`ClientMagic.Def` 的模板。
    pub magic_table: Option<&'a game_svr::magics::MagicTable>,
    /// `Gold`（`SM_ABILITY` 的 `Recog`）。
    pub gold: i32,
    /// 帧 52 尾的布局：`true` ⇒ 冻结客户端的 58B `TAbility`（`HIGHHP=True`，
    /// [`crate::record::encode_ability_client`]）；`false` ⇒ oracle 的 40B `EncodeMessage(WAbil)`。
    /// 由 `MIR2_CLIENT_ABILITY_LAYOUT` 决定（**默认 `true`** = 冻结客户端可玩；`=0|false` ⇒
    /// oracle 逐字节，供金标准/对拍夹具）。
    pub client_ability_layout: bool,
}

/// `PlayObject.SendLogon()`（`PlayObject.cs:1128-1147`）一次产三条帧所需的输入。
#[derive(Debug, Clone, Copy, Default)]
pub struct LogonInputs {
    /// `GetFeatureToLong()` / `GetFeatureEx()` 的输入（帧 50 的 `Param1` + 帧 41 的 `nRecog`/`Series`）。
    pub feature: FeatureInputs,
    /// `PlayObject.CharStatus`（帧 50 的 `Param2`；`GetCharStatus()`）。
    pub char_status: i32,
    /// `PlayObject.AllowGroup`（帧 50 的 `Tag1` 门；`WorldServer.cs:1146` = 记录 `allow_group > 0`）。
    pub allow_group: bool,
    /// `PlayObject.AttatckMode`（帧 213 的 `Recog`；`WorldServer.cs:1147` = 记录 `attatck_mode`）。
    pub attack_mode: u8,
}

/// `PlayObject.GetFeature(null)`（`PlayObject.cs:3393`）与 `CharacterObject.GetFeatureEx()`
/// （`CharacterObject.cs:146`）的输入；纯函数实现复用已合入的 [`game_svr::feature`]。
#[derive(Debug, Clone, Copy, Default)]
pub struct FeatureInputs {
    /// `UseItems[ItemLocation::Dress]` 的 `StdItem.Shape`（未穿 / `Index == 0` / `GetStdItem == null` ⇒ `None`）。
    pub dress_shape: Option<u8>,
    /// `UseItems[ItemLocation::Weapon]` 的 `StdItem.Shape`。
    pub weapon_shape: Option<u8>,
    /// `Gender`（记录 `sex`；`PlayerGender`：0 `Man` / 1 `WoMan`）。
    pub gender: u8,
    /// `Hair`（记录 `hair`）。
    pub hair: u8,
    /// `CharacterObject.OnHorse`。
    pub on_horse: bool,
    /// `CharacterObject.HorseType`。
    pub horse_type: u8,
    /// `CharacterObject.DressEffType`。
    pub dress_eff_type: u8,
}

/// `PlayObject.RecalcAbilitys()` 的一次求值（`ability::recalc_abilitys` 的接线，**不复制第二份实现**）：
/// 把 [`AbilityInputs`] 转成 [`ability::RecalcOutput`]。`apply` 与本模块外的会话态
/// （如 `CM_PICKUP`/`CM_DROPITEM` 的 `GroundState.wabil`）共用本函数。
#[must_use]
pub fn recalc_for_session(a: &AbilityInputs<'_>, job: u8) -> ability::RecalcOutput {
    let use_items = to_instances(a.use_items);
    let bag = bag_indices(a.bag_items);
    ability::recalc_abilitys(
        a.catalog,
        &RecalcInputs {
            job,
            abil: a.abil,
            bonus_abil: a.bonus_abil,
            bonus_tick_hit: a.bonus_tick_hit,
            bonus_tick_speed: a.bonus_tick_speed,
            // `WAbil.HP/MP` 是**调用前**的当前值，被 `RecalcAbilitys` 保留（`BaseObject.Abil.cs:21-24`）。
            cur_hp: a.abil.hp,
            cur_mp: a.abil.mp,
            use_items: &use_items,
            bag: &bag,
        },
    )
}

/// **Lane B**：`RecalcAbilitys()` 主循环里 Ringl/Ringr 槽的 `Shape 115/116` 命中
/// （`PlayObject.cs:2226-2249`；`Dura==0` 在 `:2105-2120` 已 `continue`）。
///
/// 登录期在构造完整 `AbilityInputs` 之外需要这个轻量判定，以驱动 `Del/AddItemSkill` 尾段。
#[must_use]
pub fn ring_hits(
    catalog: &dyn ItemCatalog,
    use_items: Option<&[Option<ServerUserItem>]>,
) -> (bool, bool) {
    ability::ring_flags(catalog, &to_instances(use_items))
}

/// `PlayObject.GetShowName()`（`PlayObject.cs:3300-3388`）的输入。
#[derive(Debug, Clone, Copy)]
pub struct ShowNameInputs<'a> {
    /// `ChrName`（记录 `chr_name`）。
    pub chr_name: &'a str,
    /// `ReLevel`（记录 `re_level`）。
    pub re_level: u8,
    /// `Job`（记录 `job`；`PlayerJob`：0 战士 / 1 法师 / 2 道士）。
    pub job: u8,
    /// `Gender`（记录 `sex`；`PlayerGender`：0 `Man` / 1 `WoMan`）。
    pub sex: u8,
    /// `DearName`（记录 `dear_name`）。
    pub dear_name: &'a str,
    /// `MasterName`（记录 `master_name`）。
    pub master_name: &'a str,
    /// `IsMaster`（记录 `is_master`）。
    pub is_master: bool,
    /// `SystemShare.Config.ShowRankLevelName`（`Server.conf [Setup]`）。
    pub show_rank_level_name: bool,
}

/// `GetChrColor()`（`PlayObject.cs:3585-3656`）对**玩家**的返回值。
///
/// oracle 分支的输入（PK 红名 / 行会关系 / 城堡战区域）在本骨架都不触发：
/// - `result = baseObject.NameColor`，而 `NameColor` **只在构造器赋过 255**
///   （`BaseObject.cs:385`；全仓 grep `NameColor =` 仅此一处）⇒ 恒 `255`；
/// - `PvpFlag`/`GetGuildRelation`/`Envir.Flag.Fight3Zone`/`CastleMgr.InCastleWarArea` 仅改 `result`，
///   在「新号/无行会/无城堡」形态下全部落空（本骨架无 PK 红名/行会/城堡系统）。
///
/// 金标准 #7 的 `Param == 255` 印证。若今后接入红名/行会，需在此补 oracle 的分支。
pub const PLAY_NAME_COLOR: u8 = 255;

/// `DateTime.Now.Hour` → `M2Share.GameTime`（`SystemProcessor.cs:98-134` 的 `GetGameTime`，1:1）。
///
/// `1`=白天、`2`=日落、`0`=日出、`3`=夜晚；24 小时全覆盖（`_` 支兜住 0/1/2/3/12/13/14/22
/// 以及非法入参 ≥24）。
#[must_use]
pub fn game_time_from_hour(hour: u32) -> u8 {
    match hour {
        5..=10 | 16..=19 | 21 => 1,
        11 | 20 | 23 => 2,
        4 | 15 => 0,
        _ => 3,
    }
}

/// 真实本地时钟的 `GameTime`。
///
/// oracle 的 `M2Share.GameTime` 由 `SystemProcessor` 每 50ms 用 `DateTime.Now.Hour` 刷新
/// （`SystemProcessor.cs:48/98-134`），登录时读到的就是这个桶值 ⇒ 这里按同一口径读**真实本地时钟**
/// （不是假时钟；`chrono::Local` 走系统时区）。每次登录算一次与 oracle 的 50ms 刷新在小时桶上等价。
#[must_use]
pub fn game_time_now() -> u8 {
    let hour = chrono::Timelike::hour(&chrono::Local::now());
    game_time_from_hour(hour)
}

/// `.NET` `string.Format`（单参数形态）的**最小 1:1**：处理 `{0}`、`{{`/`}}`；
/// 其余复合格式（`{0,5}` 对齐 / `{0:X}` 格式说明符 / 尾随参数越界）在 oracle 侧会抛
/// `FormatException`（`GetShowName` 的整个 `try` 被 catch ⇒ 返回空串）。
///
/// 现网 `String.conf` 的 `RankLevelName=%s\` 等值**不含花括号** ⇒ `string.Format` 原样返回
/// （`%s` 不是占位符），本函数按同一结果返回。
fn dotnet_format_1(fmt: &str, arg: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if chars.peek() == Some(&'{') {
                    chars.next();
                    out.push('{');
                    continue;
                }
                let mut idx = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(d) if d.is_ascii_digit() => idx.push(d),
                        _ => return None, // 对齐/格式说明符/未闭合 ⇒ 同 oracle 抛错
                    }
                }
                if idx != "0" {
                    return None; // 只有 0 号参数（尾随参数越界 ⇒ FormatException）
                }
                out.push_str(arg);
            }
            '}' => {
                if chars.peek() == Some(&'}') {
                    chars.next();
                    out.push('}');
                } else {
                    return None;
                }
            }
            other => out.push(other),
        }
    }
    Some(out)
}

/// `PlayObject.GetShowName()`（`PlayObject.cs:3300-3388`）的 1:1。
///
/// 本骨架无行会系统 ⇒ `MyGuild == null` ⇒ `sGuildName` 恒空（`ShowGuildName` 分支不可达，登记）。
/// `Format` 抛错时 oracle 的 `catch` 让 `result` 保持空串 ⇒ 这里同样返回空串。
#[must_use]
pub fn show_name(cfg: &StringConf, input: &ShowNameInputs<'_>) -> String {
    let s_chr_name = if input.show_rank_level_name {
        match dotnet_format_1(cfg.rank_level_name(), input.chr_name) {
            Some(s) => s,
            None => return String::new(),
        }
    } else if input.re_level > 0 {
        // `switch (Job)` 无 `default` ⇒ 不在 {0,1,2} 时 `sChrName` 保持初值空串。
        match input.job {
            0 => cfg.warr_re_new_name().replace("%chrname", input.chr_name),
            1 => cfg.wizard_re_new_name().replace("%chrname", input.chr_name),
            2 => cfg.taos_re_new_name().replace("%chrname", input.chr_name),
            _ => String::new(),
        }
    } else {
        input.chr_name.to_string()
    };
    let s_dear_name = if input.dear_name.is_empty() {
        String::new()
    } else {
        let tpl = if input.sex == 0 {
            cfg.man_dear_name()
        } else {
            cfg.wo_man_dear_name()
        };
        match dotnet_format_1(tpl, input.dear_name) {
            Some(s) => s,
            None => return String::new(),
        }
    };
    let s_master_name = if input.master_name.is_empty() {
        String::new()
    } else {
        let tpl = if input.is_master {
            cfg.master_name()
        } else {
            cfg.no_master_name()
        };
        match dotnet_format_1(tpl, input.master_name) {
            Some(s) => s,
            None => return String::new(),
        }
    };
    cfg.human_show_name()
        .replace("%chrname", &s_chr_name)
        .replace("%guildname", "")
        .replace("%dearname", &s_dear_name)
        .replace("%mastername", &s_master_name)
}

/// `PlayObject.DayBright()`（`PlayObject.Base.cs:244-269`）的 1:1。
///
/// `Envir.Flag.DayLight` ⇒ 0；`Envir.Flag.boDarkness` ⇒ 1；否则按 `Bright`（= `M2Share.GameTime`）
/// 映射（1→0、3→1、其它→2）。`game_time = None`（世界时钟未实现）⇒ `None`（调用方记 WRN 并用既有占位值）。
#[must_use]
pub fn day_bright(flag: &MapInfoFlag, game_time: Option<u8>) -> Option<u16> {
    if flag.day_light {
        return Some(0);
    }
    if flag.bo_darkness {
        return Some(1);
    }
    game_time.map(|bright| match bright {
        1 => 0,
        3 => 1,
        _ => 2,
    })
}

/// `RefUserState()`（`PlayObject.Base.cs:270-286`）的 `n8`。
///
/// `InGuildWarArea` 在本骨架恒 `false`（无行会战系统）⇒ 只算 `FightZone|1`、`SafeArea|2`。
#[must_use]
pub fn area_state(flag: &MapInfoFlag) -> u16 {
    let mut n8 = 0u16;
    if flag.fight_zone {
        n8 |= 1;
    }
    if flag.safe_area {
        n8 |= 2;
    }
    n8
}

/// `SendMapDescription()`（`PlayObject.Base.cs:887-895`）的 `nMusicid`。
#[must_use]
pub fn music_id(flag: &MapInfoFlag) -> i32 {
    if flag.music {
        flag.music_id
    } else {
        -1
    }
}

/// 线上文本一律按 **GB2312/GBK 字节**（`HUtil32.GetBytes` = `Encoding.GetEncoding("gb2312")`，
/// `EDCode.EncodeString` 编码的就是它）——本层不做 UTF-8 线上转换。
#[must_use]
pub fn gbk(s: &str) -> Vec<u8> {
    game_svr::downlink::gbk(s)
}

/// `SendGoldInfo(true)`（`PlayObject.Base.cs:921-939`）的 `sMsg = GameGoldName + '\r' + GamePointName`。
#[must_use]
pub fn game_gold_msg(conf: &GameSvrConf) -> Vec<u8> {
    gbk(&format!(
        "{}\r{}",
        conf.game_gold_name, conf.game_point_name
    ))
}

/// 角色记录 `NakedAbility`（`record.rs`）→ 配置层 `NakedAbility`（同字段，仅类型不同）。
#[must_use]
pub fn conf_naked(n: &NakedAbility) -> ConfNakedAbility {
    ConfNakedAbility {
        dc: n.dc,
        mc: n.mc,
        sc: n.sc,
        ac: n.ac,
        mac: n.mac,
        hp: n.hp,
        mp: n.mp,
        hit: n.hit,
        speed: n.speed,
        reserved: n.reserved,
    }
}

/// 记录 `hum_items` 里的 `ItemLocation::Dress` / `ItemLocation::Weapon` 槽位下标
/// （`src/OpenMir2/Enums/ItemLocation.cs`：`Dress = 0`、`Weapon = 1`）。
pub const ITEM_LOCATION_DRESS: usize = 0;
pub const ITEM_LOCATION_WEAPON: usize = 1;

/// `UseItems[ItemLocation::Dress/Weapon]` 的 `StdItem.Shape`——`PlayObject.GetFeature()`
/// （`PlayObject.cs:3393-3423`）的两段输入。
///
/// 逐行对照 oracle 的分支：
/// - 槽为 `null` / `.Index <= 0` ⇒ 该段计 0（`None`）；
/// - `.Index > 0` 且 `GetStdItem(Index) != null` ⇒ 返回 `StdItem.Shape`（`Some(shape)`）；
/// - `.Index > 0` 但 `GetStdItem(Index) == null`（越界/空表/空名）⇒ 计 0（`None`）+ WRN。
///
/// `catalog` = 启动时从 `stditems` 读入的模板表构造的 [`ItemCatalog`]（`crate::items::catalog_from`）；
/// 空目录（加载失败/无配置）与 oracle「物品系统为空」同形 ⇒ 所有装备槽计 0
/// （`GetStdItem == null` 分支，**1:1**，非偏差）。
#[must_use]
pub fn feature_shapes(
    hum_items: Option<&[Option<ServerUserItem>]>,
    catalog: &dyn ItemCatalog,
) -> (Option<u8>, Option<u8>, Vec<String>) {
    let index_of = |slot: usize| -> u16 {
        hum_items
            .and_then(|items| items.get(slot))
            .and_then(|item| item.as_ref())
            .map_or(0, |item| item.index)
    };
    let mut warns = Vec::new();
    let mut resolve = |slot: usize, label: &str| -> Option<u8> {
        let index = index_of(slot);
        if index == 0 {
            return None;
        }
        match catalog.shape(index) {
            Some(shape) => Some(shape),
            None => {
                warns.push(format!(
                    "帧 50/41 的 {label} 槽有装备（Index={index}）但 stditems 无对应记录 ⇒ Shape 按 \
                      oracle `GetStdItem == null` 分支计 0（1:1；空表 = 未加载/无配置）"
                ));
                None
            }
        }
    };
    let dress = resolve(ITEM_LOCATION_DRESS, "Dress");
    let weapon = resolve(ITEM_LOCATION_WEAPON, "Weapon");
    (dress, weapon, warns)
}

/// `RecalcHitSpeed()`（`PlayObject.cs:3664-3686`）里各职业取的 `Config.BonusAbilofXxx` 除数。
///
/// `Job ∉ {0,1,2}` 时 oracle 的 `switch` 无 `default`（`bonusTick` 保持 null、分支不命中）⇒ 基线恒
/// `DEFHIT/DEFSPEED`；此处返回战士模板（非 0，仅用于避免除零——该分支不消费它们）。
#[must_use]
pub fn bonus_tick(conf: &GameSvrConf, job: u8) -> (u8, i32) {
    let n = match job {
        1 => &conf.bonus_abil_wizard,
        2 => &conf.bonus_abil_taos,
        _ => &conf.bonus_abil_warr,
    };
    (n.hit, n.speed)
}

/// 角色记录 `ServerUserItem` 序列 → `RecalcAbilitys` 的 `UseItems`（本片只消费 `Index`/`Dura`）。
#[must_use]
fn to_instances(items: Option<&[Option<ServerUserItem>]>) -> Vec<Option<ItemInstance>> {
    items
        .unwrap_or(&[])
        .iter()
        .map(|cell| {
            cell.as_ref().map(|it| ItemInstance {
                index: it.index,
                dura: it.dura,
            })
        })
        .collect()
}

/// 背包物品（`bag_items`）的 `Index` 列表（`RecalcBagWeight` 只算模板重量）。
#[must_use]
fn bag_indices(items: Option<&[Option<ServerUserItem>]>) -> Vec<u16> {
    items
        .unwrap_or(&[])
        .iter()
        .filter_map(|cell| cell.as_ref().map(|it| it.index))
        .collect()
}

/// `SendAdjustBonus()`（`PlayObject.Base.cs:1193-1210`）的三段 `(A, B, C)`（各自 MemoryPack 原文，20B）。
///
/// - `A` = `Config.BonusAbilofXxx`；`B` = 角色 `BonusAbil`；`C` = `Config.NakedAbilofXxx`；
/// - `Job` 不在 `{0,1,2}` 或 `BonusAbil` 缺失 ⇒ `None`。oracle 此时 `sSendMsg` 保持空串，
///   `SendSocket(defMsg, "")` 在 `string.IsNullOrEmpty(sMsg)` 时**直接 return**（`PlayObject.Socket.cs:111-114`）
///   ⇒ **一帧都不发**，故 `None`（跳过）是 1:1，不是偏差。
#[must_use]
pub fn adjust_bonus_parts(
    conf: &GameSvrConf,
    job: u8,
    bonus_abil: Option<&NakedAbility>,
) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let player = bonus_abil?;
    let (a, c) = match job {
        0 => (&conf.bonus_abil_warr, &conf.naked_abil_warr),
        1 => (&conf.bonus_abil_wizard, &conf.naked_abil_wizard),
        2 => (&conf.bonus_abil_taos, &conf.naked_abil_taos),
        _ => return None,
    };
    Some((a.to_bytes(), conf_naked(player).to_bytes(), c.to_bytes()))
}

/// `PlayObject.SendUseItems()`（`PlayObject.Base.cs:1019-1050`）的尾：
/// 逐槽 `i + '/' + EncodeBuffer(ClientItem) + '/'`（`i` = 槽位十进制 ASCII）。
///
/// 顺序逐行照 oracle（`Base.cs:1024-1041`）：`GetUpgradeStdItem`（[`game_svr::bag::upgrade_std_item`]，
/// `Desc` 升级差量）→ `GetItemName` → `Dura/DuraMax/MakeIndex` → 服装槽
/// （`ItemLocation.Dress = 0`）`ChangeItemWithLevel` → **全部**物品 `ChangeItemByJob` →
/// `EncodeBuffer`（[`game_svr::itemwire::encode_client_item_prepared`]，模板已就绪）。
///
/// **差量必须先于展示层变换**：反过来（先变换再 `encode_client_item` 二次叠差量）会在已变换
/// 模板上再叠一次 `Desc` 差量，desc 非 0 时分叉（R 级；探针 `StdMode10/shape9/AC=w(3,250)/
/// Desc[0]=10/level=50`：oracle `AC=0x060C`，双叠 `0x0B0C`）。
///
/// `Ok(None)` = 没有可发项（oracle 的 `if (!string.IsNullOrEmpty(sSendMsg))` 门）。
fn send_use_items_tail(a: &AbilityInputs<'_>, job: u8) -> Result<Option<Vec<u8>>, String> {
    let Some(items) = a.use_items else {
        return Ok(None);
    };
    let mut tail: Vec<u8> = Vec::new();
    for (i, slot) in items
        .iter()
        .enumerate()
        .take(game_svr::downlink::USE_ITEM_SLOTS)
    {
        let Some(it) = slot else { continue };
        if it.index == 0 {
            continue;
        }
        let Some(tpl) = a.catalog.get(it.index) else {
            continue;
        };
        let mut desc = [0u8; game_svr::itemwire::ITEM_NAME_LEN];
        if let Some(d) = &it.desc {
            for (j, b) in d.iter().take(desc.len()).enumerate() {
                desc[j] = *b;
            }
        }
        // oracle 顺序第一步：`GetUpgradeStdItem`（模板 + `Desc` 差量）——必须先于
        // 下面的 `ChangeItemWithLevel/ByJob`（`Base.cs:1029-1041`）。
        let (mut owned, _) = game_svr::bag::upgrade_std_item(tpl, &desc);
        if i == ITEM_LOCATION_DRESS {
            game_svr::itemwire::change_item_with_level(&mut owned, a.abil.level);
        }
        game_svr::itemwire::change_item_by_job(&mut owned, job);
        let bag_item = game_svr::bag::BagItem {
            make_index: it.make_index,
            index: it.index,
            dura: it.dura,
            dura_max: it.dura_max,
            desc,
        };
        let name = game_svr::bag::item_display_name(a.catalog, &bag_item);
        let ci = game_svr::itemwire::ClientItem {
            tpl: &owned,
            name,
            make_index: it.make_index,
            dura: it.dura,
            dura_max: it.dura_max,
            desc,
        };
        tail.extend_from_slice(format!("{i}/").as_bytes());
        // `ci.tpl` 已是「升级差量 → 展示层变换」的成品 ⇒ prepared（不再二次叠差量）。
        tail.extend_from_slice(&mir2_protocol::edcode::encode(
            &game_svr::itemwire::encode_client_item_prepared(&ci),
        ));
        tail.push(b'/');
    }
    if tail.is_empty() {
        Ok(None)
    } else {
        Ok(Some(tail))
    }
}

/// `PlayObject.SendUseMagic()`（`PlayObject.Base.cs:1052-1073`）的 `(count, 尾)`：
/// 逐条 `EncodeBuffer(ClientMagic) + '/'`；`count = MagicList.Count`。
///
/// **空槽口径（oracle）**：DBSvr 取档时确实把 `MagicRcd[20]` 全槽填成默认值
/// （`PlayDataStorage.Query.cs:343-357` 的 `Magic[i] = new MagicRcd()`，`MagIdx = 0`，
/// 与 `bins/db-srv/src/playdata.rs:206` 一致，属 1:1 照搬），但 `WorldServer.cs:1192-1211`
/// 把它们装进 `PlayObject.MagicList` 时会 `FindMagic(MagIdx)`：**查不到模板的槽直接丢弃**。
/// 因此 `MagicList` 只含可解析条目，`count`（`series`）与尾长都只算**有效**条目——
/// 空槽既不占 `series`，也不产出垃圾条目。查表口径 = `FindMagic`（首行胜，见 [`MagicTable`]）。
///
/// 全部槽都无效（或记录/魔表缺失）⇒ `sSendMsg` 为空 ⇒ oracle 的
/// `if (!string.IsNullOrEmpty(sSendMsg))` 门 ⇒ **一帧都不发**（`None`；真机 `s2c-621b` 无 211）。
fn send_my_magic(a: &AbilityInputs<'_>) -> Option<(u16, Vec<u8>)> {
    let (magics, table) = (a.magics?, a.magic_table?);
    let mut tail: Vec<u8> = Vec::new();
    let mut count: u16 = 0;
    for m in magics.iter().flatten() {
        // oracle 装表门：`FindMagic(MagIdx) == null` 的槽不进 `MagicList`
        // （空槽 `MagIdx = 0`、真魔表无 0 行，即此支）。
        let Some(def) = table.get(m.mag_idx) else {
            continue;
        };
        tail.extend_from_slice(&mir2_protocol::edcode::encode(
            &game_svr::itemwire::encode_client_magic(m.magic_key, m.level, m.tran_point, def),
        ));
        tail.push(b'/');
        count = count.saturating_add(1);
    }
    if tail.is_empty() {
        None
    } else {
        Some((count, tail))
    }
}

/// 把 [`WorldFrameInputs`] 装配进 `base`（`base` 里 `notice`/`map_file_name`/`actor_id`/坐标/方向
/// 由 `lib.rs` 从网关链路与角色记录填好）。**只填**本片有输入的那几条；其余保持 `None`。
///
/// 返回本步产生的 WRN 文案（调用方逐条 `glog!`，不静默）。
#[must_use]
pub fn apply(base: &mut EnterWorldInputs, wx: &WorldFrameInputs<'_>) -> Vec<String> {
    let mut warns = Vec::new();

    if let Some(flag) = wx.flag {
        // 帧 51 的 `Series` = `DayBright()`（`Message.cs:1681`）。
        match day_bright(flag, wx.game_time) {
            Some(d) => {
                base.day_bright = d;
                // 帧 46 `SM_DAYCHANGING`（`Message.cs:1741`）：`Bright = M2Share.GameTime`、`DayBright()`。
                // 两者都要求世界时钟（`Bright`），故 `game_time` 缺失时跳过并登记（不猜）。
                if let Some(gt) = wx.game_time {
                    base.day_changing = Some((u16::from(gt), d));
                }
            }
            None => warns.push(format!(
                "当前地图的 DayBright 依赖 GameTime（世界时钟未实现）⇒ 帧 51 的 Series 沿用占位 {}（登记）",
                base.day_bright
            )),
        }
        // 帧 766 `SM_AREASTATE`。
        base.area_state = Some(area_state(flag));
        // 帧 54 `SM_MAPDESCRIPTION`（尾 = `MapDesc`，GBK 字节）。
        if let Some(desc) = wx.map_desc {
            base.map_description = Some((music_id(flag), gbk(desc)));
        }
    }

    // 帧 42 `SM_USERNAME`（`Message.cs:1686` → `ClientQueryUserName(ActorId, CurrX, CurrY)`，
    // `PlayObject.Operate.cs:18-34`）：目标就是自己 ⇒ `CretInNearXy` 命中，尾 = `EncodeString(GetShowName())`。
    if let (Some(strings), Some(sn)) = (wx.strings, wx.show_name.as_ref()) {
        base.username = Some((base.actor_id, PLAY_NAME_COLOR, gbk(&show_name(strings, sn))));
    }

    // 帧 50/41/213（`PlayObject.SendLogon()`，`PlayObject.cs:1128-1147`）：oracle 在 `RM_LOGON`
    // 支里**同组连发三条**（`SM_LOGON` → `SM_FEATURECHANGED` → `SM_ATTACKMODE`）；`logon` 缺失时
    // 三条一起跳过（单发半组比不发更坏）。
    match wx.logon {
        Some(l) => {
            let feature = human_feature(
                l.feature.dress_shape,
                l.feature.weapon_shape,
                l.feature.gender,
                l.feature.hair,
            );
            let feature_ex = feature_ex(
                l.feature.on_horse,
                l.feature.horse_type,
                l.feature.dress_eff_type,
            );
            // `if (AllowGroup) messageBodyWl.Tag1 = MakeLong(MakeWord(1, 0), GetFeatureEx()); else Tag1 = 0;`
            let tag1 = if l.allow_group {
                make_long(make_word(1, 0), feature_ex)
            } else {
                0
            };
            base.logon_body_wl = Some((feature, l.char_status, tag1, 0));
            base.feature = Some((feature, feature_ex));
            base.attack_mode = Some(l.attack_mode);
            // `RM_LOGON` 里排队的 `SendMsg(RM_CHANGELIGHT, 0,0,0,0)`（`Message.cs:1682`）⇒
            // `SM_CHANGELIGHT(654)`：`Recog=ActorId`、`Param=Light`、`Tag=Config.nClientKey`
            // （`Message.cs:1840`）。金标准实测 `param=0`（该角色无光源）、`tag=500`。
            base.changelight = Some((
                base.light,
                game_svr::sessionwire::N_CLIENT_KEY_DEFAULT,
            ));
        }
        None => warns.push(
            "帧 50/41/213 输入不足（未解析出角色记录全字段 GetFeatureToLong/CharStatus/AttatckMode）\
             ⇒ 同组跳过（登记）"
                .to_string(),
        ),
    }

    // 登录 `UserLogon` 的 3+1 条 `SysMsg`（`PlayObject.cs:1342-1383` 的攻击模式/CTRL-H/欢迎语 +
    // `:1417-1420` 的免费模式）。**入队门**由 `test_server`/`pay_ment_three` 决定（槽位在
    // `EnterWorldInputs::frames_split` 里按 oracle 无条件 `SysMsg(...)` 调用占堆）；正文取自
    // `String.conf [String]`（键名与 `MessageSettings` 静态字段同源，缺键回落源码默认）。
    base.test_server = wx.test_server;
    // `PlayObject.cs:1145-1148`：`if (TestServer || ServiceMode) PayMent = 3;`
    let pay_ment = if wx.test_server || wx.service_mode {
        3
    } else {
        wx.pay_ment
    };
    base.pay_ment_three = pay_ment == 3;
    base.login_sys_msgs.green = wx.green_msg;
    if let Some(strings) = wx.strings {
        base.login_sys_msgs.change_help = Some(gbk(strings.start_change_attack_mode_help()));
        if wx.test_server {
            base.login_sys_msgs.notice = Some(gbk(strings.start_notice_msg()));
        }
        if pay_ment == 3 {
            base.login_sys_msgs.free_play = Some(gbk(strings.now_is_free_play_mode()));
        }
        if let Some(l) = wx.logon {
            base.login_sys_msgs.attack_mode = strings.attack_mode_msg(l.attack_mode).map(gbk);
        }
    }

    match wx.conf {
        Some(conf) => {
            // 门：`SoftVersionDateEx == 0 ⇒ SendServerConfig/SendGoldInfo 直接 return`。
            if wx.soft_version_date != 0 {
                base.client_conf = Some(conf.client_conf.to_bytes());
                base.game_gold_name = Some((wx.game_gold, wx.game_point, game_gold_msg(conf)));
            }
            // 帧 1106 无门（`RM_LOGON` 无条件发）。
            base.version_fail = Some((conf.client_file1_crc, conf.client_file2_crc));
            // 帧 811（`BonusPoint` 落在命令头 Recog，由 `sm_adjust_bonus` 取出）。
            match adjust_bonus_parts(conf, wx.job, wx.bonus_abil) {
                Some((a, b, c)) => base.adjust_bonus = Some((wx.bonus_point, a, b, c)),
                // oracle 的 `switch (Job)` 无 `default` ⇒ `sSendMsg` 保持空串；`SendAdjustBonus` 调的是
                // 2 参 `SendSocket(ClientMsg, sSendMsg)`，而该重载在 `string.IsNullOrEmpty(sMsg)` 时
                // **直接 return**（`PlayObject.Socket.cs:111-114`）⇒ **oracle 此时一帧都不发**。
                // 故这里的「跳过」就是 1:1（reviewer-1 的 O-1 口径「发 header-only 811」有误：那是
                // `SendDefMessage` 的空尾分支 `:37-40`，不是 `SendAdjustBonus` 用的 2 参 `SendSocket`）。
                // 真线上 `Job` 恒 ∈ {0,1,2}，实语料不可达（`openmir2-m2-enter-world-frames`）。
                None => warns.push(format!(
                    "帧 811 输入不足（Job={} 不在 {{0,1,2}} 或角色 BonusAbil 缺失）⇒ 跳过（oracle 同样不发，1:1）",
                    wx.job
                )),
            }
        }
        None => warns.push(
            "未加载 Server.conf（GameSvrConf）⇒ 帧 11029/1106/5008/811 跳过（登记）".to_string(),
        ),
    }

    // 帧 52 `SM_ABILITY` / 752 `SM_SUBABILITY`（`PlayObject.cs:1385-1386`）：登录后
    // `SendPriorityMsg(RM_ABILITY/RM_SUBABILITY)`。输入由**已合入的** `game_svr::ability::recalc_abilitys`
    // （= oracle `PlayObject.RecalcAbilitys()` / `BaseObject.Abil.cs:15`）从角色记录 `Abil` + `stditems`
    // 目录 + `BonusAbil` 产出；本层只负责接线（禁止复制第二份实现）。
    match wx.ability {
        Some(a) => {
            let out = recalc_for_session(&a, wx.job);
            // 52：`MakeMessage(SM_ABILITY, Gold, MakeWord((byte)Job, 99), LoWord(GameGold), HiWord(GameGold))`
            // 尾 = `EncodeMessage(WAbil)`（`Message.cs:1732-1734`）。`MakeWord(low, high)` ⇒
            // `Job` 在**低字节**、99 在**高字节**（G-12：旧注释写反了，字节值以代码为准）。
            // `client_ability_layout`（`MIR2_CLIENT_ABILITY_LAYOUT`，**默认开** = 冻结客户端 58B）
            // 为 `false` 时用 oracle 的 40B `EncodeMessage(WAbil)`（逐字节对拍）。
            let wabil_tail = if a.client_ability_layout {
                crate::record::encode_ability_client(&out.wabil)
            } else {
                crate::record::encode_ability(&out.wabil)
            };
            base.ability = Some((a.gold, wx.job, wx.game_gold, wabil_tail));
            // 752：7 个字段全在头（`Message.cs:1935`）。
            base.sub_ability = Some(out.sub.to_frame_bytes());
            // 621 `SM_SENDUSEITEMS` / 211 `SM_SENDMYMAGIC`（`PlayObject.cs:1389-1390`）。
            // 两条都有「尾为空 ⇒ 不发」门（`Base.cs:1046/1065`）。
            match send_use_items_tail(&a, wx.job) {
                Ok(tail) => base.use_items_tail = tail,
                Err(w) => warns.push(w),
            }
            base.magic = send_my_magic(&a);
            // `BaseObject.Light` / `PlayObject.GetMyLight()`：装备（右手火把等，`Index>0 &&
            // Dura>0`）的 `StdItem.Light` 最大值；`Level >= EFFECTIVE_HIGHLEVEL` 时基线 1。
            // `SM_LOGON` 的 `MakeWord(Dir, Light)` 与 `SM_CHANGELIGHT` 的 `Param` 都用它。
            if let Some(items) = a.use_items {
                let pairs: Vec<(u16, u16)> = items
                    .iter()
                    .map(|s| s.as_ref().map_or((0, 0), |it| (it.index, it.dura)))
                    .collect();
                base.light = game_svr::downlink::my_light(a.abil.level, &pairs, a.catalog);
                if let Some((_, key)) = base.changelight {
                    base.changelight = Some((base.light, key));
                }
            }
        }
        None => warns.push(
            "帧 52/752 输入不足（角色 Abil/BonusAbil 或 stditems 目录缺失）⇒ 两条同组跳过（登记）"
                .to_string(),
        ),
    }
    warns
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_data::StdItem;
    use mir2_envir::parse_section_line;
    use mir2_protocol::edcode;
    use mir2_protocol::frame::CommandMessage;

    /// `[0 比奇省] DAY`（现网 `Envir/MapInfo.txt:281`）。
    fn flag_day() -> (MapInfoFlag, String) {
        let e = parse_section_line("[0 比奇省] DAY").expect("section");
        (e.flag, e.desc)
    }

    fn rec_naked() -> NakedAbility {
        NakedAbility {
            dc: 1,
            mc: 2,
            sc: 3,
            ac: 4,
            mac: 5,
            hp: 6,
            mp: 7,
            hit: 8,
            speed: 9,
            reserved: 10,
        }
    }

    fn idents(frames: &[Vec<u8>]) -> Vec<u16> {
        frames
            .iter()
            .map(|b| CommandMessage::from_bytes(b).expect("12B 头").ident)
            .collect()
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// 帧 766/54 的输入：`[0 比奇省] DAY` ⇒ `n8=0`、`nMusicid=-1`、尾 = `比奇省` 的 GBK 字节。
    #[test]
    fn area_state_and_map_description_follow_real_map0_flag() {
        let (flag, desc) = flag_day();
        assert_eq!(area_state(&flag), 0, "[0 比奇省] 无 FIGHT/SAFE ⇒ n8=0");
        assert_eq!(music_id(&flag), -1, "无 MUSIC ⇒ nMusicid=-1");

        let mut base = EnterWorldInputs {
            day_bright: 0,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: Some(&flag),
            map_desc: Some(&desc),
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: None,
            ability: None,
            ..WorldFrameInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(
            warns.len(),
            3,
            "只该有「未加载 conf」+「50/41/213 无输入」+「52/752 无输入」：{warns:?}"
        );
        assert!(warns.iter().any(|w| w.contains("Server.conf")), "{warns:?}");
        assert_eq!(base.area_state, Some(0));
        assert_eq!(
            base.map_description,
            Some((-1, gbk("比奇省"))),
            "帧 54 = (nMusicid=-1, MapDesc 的 GBK 字节)"
        );
        let frames = base.frames();
        assert_eq!(idents(&frames), vec![658, 766, 54], "只装配了 flag 层");
        let cm766 = CommandMessage::from_bytes(&frames[1]).unwrap();
        assert_eq!(cm766.recog, 0, "766 的 n8 落在 Recog");
        let cm54 = CommandMessage::from_bytes(&frames[2]).unwrap();
        assert_eq!(cm54.recog, -1);
        assert_eq!(
            edcode::decode(&frames[2][12..]),
            gbk("比奇省"),
            "54 尾必须与 `Encode(GBK(\"比奇省\"))` 一致"
        );
    }

    /// 红检：`area_state` 的位映射反了（`|1`↔`|2`）⇒ 本用例必红。
    #[test]
    fn area_state_bits_are_fight1_safe2() {
        let both = parse_section_line("[X 战场] FIGHT SAFE DAY").unwrap().flag;
        assert_eq!(area_state(&both), 1 | 2, "FightZone|1、SafeArea|2");
        let only_safe = parse_section_line("[Y 安全区] SAFE").unwrap().flag;
        assert_eq!(area_state(&only_safe), 2, "SafeArea 只置 bit1");
        let only_fight = parse_section_line("[Z 战斗] FIGHT").unwrap().flag;
        assert_eq!(area_state(&only_fight), 1, "FightZone 只置 bit0");
    }

    /// 红检：`music_id` 的 `!music ⇒ -1` 回落被拿掉（恒用 `music_id`）⇒ 本用例必红。
    #[test]
    fn music_id_falls_back_to_minus_one() {
        let (no_music, _) = flag_day();
        assert!(!no_music.music);
        assert_eq!(music_id(&no_music), -1);
        let with_music = parse_section_line("[M 音乐] MUSIC(7)").unwrap().flag;
        assert!(with_music.music);
        assert_eq!(music_id(&with_music), 7);
    }

    /// 帧 11029/5008 的 **`SoftVersionDateEx == 0`** 门（`Base.cs:930/943`）；1106 无门；811 由 `Job` 选模板。
    #[test]
    fn soft_version_gate_and_config_frames() {
        let conf = GameSvrConf {
            game_gold_name: "元宝".to_string(),
            game_point_name: "游戏点".to_string(),
            ..GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n")
        };
        let rec_bonus = rec_naked();
        let wx = WorldFrameInputs {
            flag: None,
            map_desc: None,
            conf: Some(&conf),
            bonus_abil: Some(&rec_bonus),
            soft_version_date: 0, // 门关
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: None,
            ability: None,
            ..WorldFrameInputs::default()
        };
        let mut base = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(
            warns.len(),
            2,
            "只该有「50/41/213 无输入」+「52/752 无输入」：{warns:?}"
        );
        assert!(warns[0].contains("50/41/213"), "{warns:?}");
        assert!(base.client_conf.is_none(), "门关 ⇒ 11029 不发");
        assert!(base.game_gold_name.is_none(), "门关 ⇒ 5008 不发");
        assert_eq!(base.version_fail, Some((0, 0)), "1106 无门，CRC 恒 0");
        assert_eq!(idents(&base.frames()), vec![658, 1106, 811], "门关序列");

        // 门开 ⇒ 11029/5008 出现，且序为金标准的相对序（11029 在 5008 前、5008 在 1106 前）。
        let wx_open = WorldFrameInputs {
            soft_version_date: 120_020_522,
            ..wx
        };
        let mut base2 = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let warns2 = apply(&mut base2, &wx_open);
        assert_eq!(
            warns2.len(),
            2,
            "只该有「50/41/213 无输入」+「52/752 无输入」两条：{warns2:?}"
        );
        let frames = base2.frames();
        assert_eq!(
            idents(&frames),
            vec![658, 11029, 5008, 1106, 811],
            "门开序列"
        );
        let f5008 = frames
            .iter()
            .find(|b| CommandMessage::from_bytes(b).unwrap().ident == 5008)
            .expect("5008");
        assert_eq!(
            edcode::decode(&f5008[12..]),
            gbk("元宝\r游戏点"),
            "5008 尾 = 金币名 + '\\r' + 游戏点名"
        );
    }

    /// 红检：把 `game_gold_msg` 的 `'\r'` 改成 `'\n'` ⇒ 本用例必红。
    #[test]
    fn gold_msg_uses_carriage_return() {
        let conf = GameSvrConf {
            game_gold_name: "元宝".to_string(),
            game_point_name: "成长".to_string(),
            ..GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n")
        };
        assert_eq!(game_gold_msg(&conf), gbk("元宝\r成长"));
        assert!(game_gold_msg(&conf).contains(&0x0D), "分隔符必须是 0x0D");
        assert!(!game_gold_msg(&conf).contains(&0x0A));
    }

    /// 帧 811 按 `Job` 取模板：0 战士 / 1 法师 / 2 道士；越界/缺输入 ⇒ `None`（不猜）。
    #[test]
    fn adjust_bonus_selects_template_by_job() {
        let conf = GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n");
        let rec = NakedAbility {
            dc: 111,
            ..rec_naked()
        };
        let warr = adjust_bonus_parts(&conf, 0, Some(&rec)).expect("战士");
        assert_eq!(
            warr.0,
            conf.bonus_abil_warr.to_bytes(),
            "A = BonusAbilofWarr"
        );
        assert_eq!(warr.1, conf_naked(&rec).to_bytes(), "B = 角色 BonusAbil");
        assert_eq!(
            warr.2,
            conf.naked_abil_warr.to_bytes(),
            "C = NakedAbilofWarr"
        );
        let wiz = adjust_bonus_parts(&conf, 1, Some(&rec)).expect("法师");
        assert_eq!(wiz.0, conf.bonus_abil_wizard.to_bytes());
        assert_ne!(warr.0, wiz.0, "不同职业的 A 段模板不同");
        let tao = adjust_bonus_parts(&conf, 2, Some(&rec)).expect("道士");
        assert_eq!(tao.0, conf.bonus_abil_taos.to_bytes());
        assert!(
            adjust_bonus_parts(&conf, 9, Some(&rec)).is_none(),
            "越界 Job ⇒ 不猜"
        );
        assert!(
            adjust_bonus_parts(&conf, 0, None).is_none(),
            "缺 BonusAbil ⇒ 不猜"
        );
    }

    /// **生产路径端到端**：真全字段 oracle 记录（`player_data_full_oracle.txt` 的 `stage6`）+
    /// 真 `[0 比奇省] DAY` flag + 默认配置层 ⇒ `apply` + `frames()` 应产出金标准**相对序**的
    /// `658, 51, 11029, 766, 54, 5008, 1106`（缺 50/41/213/42/52/752/46/10 的输入；
    /// 该 oracle 记录的 `Job=3` 不在 `{0,1,2}` ⇒ 811 按登记跳过）。
    #[test]
    fn production_path_emits_world_and_config_frames_for_real_record() {
        let txt = include_str!("../tests/fixtures/player_data_full_oracle.txt");
        let hex = txt
            .lines()
            .find_map(|l| l.strip_prefix("stage6="))
            .expect("stage6 fixture");
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let rec = crate::record::parse_player_data(&bytes).expect("全字段 oracle 记录");
        assert_eq!(rec.data.job, 3, "fixture 的 Job=3（不在 {{0,1,2}}）");

        let (flag, desc) = flag_day();
        let conf = GameSvrConf::from_bytes(b"[Server]\nGatePort=5000\n");
        let mut base = EnterWorldInputs {
            actor_id: 0x7C30_3998,
            cur_x: u16::try_from(rec.data.cur_x).unwrap_or(0),
            cur_y: u16::try_from(rec.data.cur_y).unwrap_or(0),
            dir: rec.data.dir,
            notice: Vec::new(),
            map_file_name: Some(rec.data.cur_map.clone().unwrap_or_default().into_bytes()),
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: Some(&flag),
            map_desc: Some(&desc),
            conf: Some(&conf),
            bonus_abil: rec.data.bonus_abil.as_ref(),
            soft_version_date: 120_020_522,
            game_gold: rec.data.game_gold,
            game_point: rec.data.game_point,
            job: rec.data.job,
            bonus_point: rec.data.bonus_point,
            game_time: None,
            strings: None,
            show_name: None,
            logon: None,
            ability: None,
            ..WorldFrameInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(
            warns.len(),
            3,
            "811 跳过 + 50/41/213 无输入 + 52/752 无输入：{warns:?}"
        );
        assert!(warns.iter().any(|w| w.contains("811")), "{warns:?}");
        let frames = base.frames();
        assert_eq!(
            idents(&frames),
            vec![658, 51, 11029, 766, 54, 5008, 1106],
            "金标准相对序（子序列）"
        );
        // 766 的 n8 来自真 flag；54 的尾 = 真 MapDesc。
        let f766 = &frames[3];
        assert_eq!(CommandMessage::from_bytes(f766).unwrap().recog, 0);
        let f54 = &frames[4];
        let cm54 = CommandMessage::from_bytes(f54).unwrap();
        assert_eq!(cm54.recog, -1);
        assert_eq!(edcode::decode(&f54[12..]), gbk("比奇省"));
        // 5008 的尾 = 金币名（真 `Server.conf` 默认值）。
        let f5008 = &frames[5];
        assert_eq!(edcode::decode(&f5008[12..]), gbk("元宝\r游戏点"));
        // 1106 无尾 + `<<<<<<`。
        let f1106 = &frames[6];
        assert_eq!(&f1106[12..], b"<<<<<<");
    }

    /// 红检：`apply` 的 `soft_version_date == 0` 门或 flag/conf 装配被改坏 ⇒ 上面两个用例必红。
    /// `DayBright()`：`DAY` ⇒ 0、darkness ⇒ 1、否则依赖 `GameTime`（未实现 ⇒ `None` + WRN）。
    #[test]
    fn day_bright_matches_oracle() {
        let (day, _) = flag_day();
        assert_eq!(day_bright(&day, None), Some(0), "DAY ⇒ 0，与 GameTime 无关");
        let dark = parse_section_line("[D 暗] DARK").unwrap().flag;
        assert_eq!(day_bright(&dark, None), Some(1));
        let neutral = parse_section_line("[N 中立] NORANDOMMOVE").unwrap().flag;
        assert!(!neutral.day_light && !neutral.bo_darkness);
        assert_eq!(
            day_bright(&neutral, None),
            None,
            "无 DAY/darkness 且无 GameTime ⇒ 不猜"
        );
        assert_eq!(day_bright(&neutral, Some(1)), Some(0));
        assert_eq!(day_bright(&neutral, Some(3)), Some(1));
        assert_eq!(day_bright(&neutral, Some(2)), Some(2));

        // 有 flag 但 DayBright 依赖 GameTime ⇒ WRN（不静默），且沿用调用方占位值。
        let mut base = EnterWorldInputs {
            day_bright: 7,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: Some(&neutral),
            map_desc: None,
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: None,
            ability: None,
            ..WorldFrameInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(
            warns.len(),
            4,
            "DayBright + 未加载 conf + 50/41/213 无输入 + 52/752 无输入：{warns:?}"
        );
        assert_eq!(base.day_bright, 7, "不猜 ⇒ 沿用调用方给的占位值");
        assert_eq!(base.area_state, Some(0));
    }

    /// `SystemProcessor.GetGameTime()`（`SystemProcessor.cs:98-134`）的小时→`GameTime` 映射逐值钉死。
    ///
    /// 红检：把 `11 | 20 | 23 => 2` 改成 `=> 1` ⇒ 本用例必红。
    #[test]
    fn game_time_from_hour_matches_oracle_switch() {
        for h in [5u32, 6, 7, 8, 9, 10, 16, 17, 18, 19, 21] {
            assert_eq!(game_time_from_hour(h), 1, "白天: {h}");
        }
        for h in [11u32, 20, 23] {
            assert_eq!(game_time_from_hour(h), 2, "日落: {h}");
        }
        for h in [4u32, 15] {
            assert_eq!(game_time_from_hour(h), 0, "日出: {h}");
        }
        for h in [0u32, 1, 2, 3, 12, 13, 14, 22] {
            assert_eq!(game_time_from_hour(h), 3, "夜晚: {h}");
        }
        assert!(game_time_now() <= 3, "真实时钟必须落在 0..=3");
    }

    /// 现网 `String.conf`（`RankLevelName=%s\` / `HumanShowName=%chrname\%guildname\%dearname`）加上
    /// `Server.conf ShowRankLevelName=1`：`GetShowName()` 的 `%s` **不是 .NET 占位符**（照搬 oracle
    /// 的 `string.Format` 语义）⇒ 结果为字面量；与金标准 #7（seq 21）线缆逐字节相等。
    ///
    /// 红检：`PLAY_NAME_COLOR` 255→0、或 `show_name` 的替换串被改 ⇒ 本用例必红。
    #[test]
    fn show_name_matches_deployed_config_and_golden_42() {
        let strings = crate::strings::parse(
            b"[String]\nRankLevelName=%s\\\nHumanShowName=%chrname\\%guildname\\%dearname\n",
        );
        let sn = ShowNameInputs {
            chr_name: "bot",
            re_level: 0,
            job: 0,
            sex: 0,
            dear_name: "",
            master_name: "",
            is_master: false,
            show_rank_level_name: true,
        };
        let expected = r"%s\\\";
        assert_eq!(show_name(&strings, &sn), expected);
        assert_eq!(
            PLAY_NAME_COLOR, 255,
            "BaseObject.NameColor 默认值（BaseObject.cs:385）"
        );
        let body = game_svr::smframe::sm_username(
            0x7C30_3998,
            PLAY_NAME_COLOR,
            &gbk(&show_name(&strings, &sn)),
        );
        assert_eq!(
            game_svr::smframe::client_frame(&body),
            hex("2348615861705e685d5364685f6464686b5d736c7778783c21"),
            "金标准 #7 的 `42 SM_USERNAME`（seq 21）逐字节"
        );
    }

    /// `.NET string.Format(fmt, arg)` 的两个形态：`{0}` 替换（源码缺省模板）与无花括号原样（现网）。
    #[test]
    fn rank_level_name_follows_dotnet_string_format() {
        let c = crate::strings::parse(b"[String]\n");
        let sn = ShowNameInputs {
            chr_name: "英雄",
            re_level: 0,
            job: 0,
            sex: 0,
            dear_name: "",
            master_name: "",
            is_master: false,
            show_rank_level_name: true,
        };
        // 缺省 `RankLevelName={0}\平民`、`HumanShowName=%chrname\%guildname\%dearname\%mastername`。
        assert_eq!(show_name(&c, &sn), r"英雄\平民\\\");
        // `ShowRankLevelName=false` 且未转生 ⇒ 直接用 `ChrName`。
        let plain = ShowNameInputs {
            show_rank_level_name: false,
            ..sn
        };
        assert_eq!(show_name(&c, &plain), r"英雄\\\");
    }

    /// 帧 42/46 进生产路径：`game_time=Some(1)` + `[0 比奇省] DAY` ⇒ `day_changing=(1,0)`，
    /// `username=(ActorId, 255, GBK(name))`；`frames()` 里 42 在 766 前、46 在 54 后（金标准相对序）。
    #[test]
    fn username_and_daychanging_are_wired_in_order() {
        let (flag, desc) = flag_day();
        let strings = crate::strings::StringConf::defaults();
        let sn = ShowNameInputs {
            chr_name: "bot",
            re_level: 0,
            job: 0,
            sex: 0,
            dear_name: "",
            master_name: "",
            is_master: false,
            show_rank_level_name: false,
        };
        let mut base = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: Some(&flag),
            map_desc: Some(&desc),
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: Some(1),
            strings: Some(&strings),
            show_name: Some(sn),
            logon: None,
            ability: None,
            ..WorldFrameInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(
            warns.len(),
            3,
            "只该有「未加载 conf」+「50/41/213 无输入」+「52/752 无输入」三条：{warns:?}"
        );
        assert_eq!(
            base.day_changing,
            Some((1, 0)),
            "Bright=1；DAY ⇒ DayBright=0"
        );
        assert_eq!(base.username, Some((7, 255, gbk(r"bot\\\"))));
        assert_eq!(
            idents(&base.frames()),
            // 654 之后：`RefShowName` 的第二条 42（165）先于 `StartChangeAttackModeHelp`
            // 的 100（252）；本 fixture 无 logon ⇒ 攻击模式槽不占位、`TestServer/PayMent`
            // 缺省 false ⇒ 第 3/4 条 100 不入队。
            vec![658, 42, 766, 54, 46, 42, 100],
            "42 在 766 前、46 在 54 后；第二 42 在 100 前（金标准相对序）"
        );
    }

    /// **帧 50/41/213 同组接线**（`PlayObject.SendLogon()`，`PlayObject.cs:1128-1147`）：
    /// `logon: Some(..)` ⇒ 三条按金标准相对序产出，且字段序/位序逐字节钉死。
    ///
    /// 红检：把 `human_feature(dress, weapon, gender, hair)` 的 `dress`/`weapon` 两参对调 ⇒
    /// 本用例必红（`0x0705_0B00` → `0x030B_0500`）；把 `MakeWord(1, 0)` 改 `MakeWord(0, 1)`
    /// ⇒ `Tag1` 处必红。
    #[test]
    fn logon_group_is_wired_in_order_and_byte_exact() {
        let logon = LogonInputs {
            feature: FeatureInputs {
                dress_shape: Some(3),
                weapon_shape: Some(5),
                gender: 1,
                hair: 2,
                on_horse: true,
                horse_type: 9,
                dress_eff_type: 4,
            },
            char_status: 0x1122_3344,
            allow_group: true,
            attack_mode: 2,
        };
        // 逐值手算（oracle `GetFeature`/`GetFeatureEx`）：
        //   nDress  = (byte)(3*2) + 1 = 7
        //   nWeapon = 1 + (byte)(5*2) = 11
        //   nHair   = (byte)(2*2 + 1) = 5
        //   feature = MakeHumanFeature(0, 7, 11, 5) = MakeLong(MakeWord(0,11), MakeWord(5,7))
        //           = 0x0B00 | (0x0705 << 16) = 0x0705_0B00
        //   featureEx = MakeWord(9, 4) = 0x0409
        //   Tag1   = MakeLong(MakeWord(1,0), featureEx) = 0x0001 | (0x0409 << 16) = 0x0409_0001
        let mut base = EnterWorldInputs {
            actor_id: 7,
            cur_x: 10,
            cur_y: 20,
            dir: 2,
            // 金标准角色无光源（`Light=0`）⇒ 不触发 `RecalcAbilitys` 的额外 `RM_CHANGELIGHT`。
            light: 0,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: None,
            map_desc: None,
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: Some(logon),
            ability: None,
            ..WorldFrameInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert_eq!(
            warns.len(),
            2,
            "只该有「未加载 conf」一条 +「52/752 无输入」：{warns:?}"
        );
        assert_eq!(
            base.logon_body_wl,
            Some((0x0705_0B00, 0x1122_3344, 0x0409_0001, 0))
        );
        assert_eq!(base.feature, Some((0x0705_0B00, 0x0409)));
        assert_eq!(base.attack_mode, Some(2));

        let frames = base.frames();
        assert_eq!(
            idents(&frames),
            vec![658, 50, 41, 213, 654],
            "同组三条 + 658 + RM_LOGON 排队的 654"
        );
        // 654：`Recog=ActorId(7)`、`Param=Light(0)`、`Tag=500`、`Series=0`、无尾。
        let mut expect_654 = Vec::new();
        expect_654.extend_from_slice(&7i32.to_le_bytes());
        expect_654.extend_from_slice(&654u16.to_le_bytes());
        expect_654.extend_from_slice(&0u16.to_le_bytes()); // param = Light
        expect_654.extend_from_slice(&500u16.to_le_bytes()); // tag = nClientKey
        expect_654.extend_from_slice(&0u16.to_le_bytes());
        assert_eq!(frames[4], expect_654, "654 逐字节");
        // 帧 50 尾 = `EncodePacket(MessageBodyWL 16B)`。
        let wl = edcode::decode(&frames[1][12..]);
        assert_eq!(wl.len(), 16);
        assert_eq!(&wl[0..4], &0x0705_0B00i32.to_le_bytes());
        assert_eq!(&wl[4..8], &0x1122_3344i32.to_le_bytes());
        assert_eq!(&wl[8..12], &0x0409_0001i32.to_le_bytes());
        assert_eq!(&wl[12..16], &[0, 0, 0, 0]);
        // 帧 41 的 Param/Tag = Lo/HiWord(feature)、Series = featureEx。
        let cm41 = CommandMessage::from_bytes(&frames[2]).unwrap();
        assert_eq!((cm41.ident, cm41.recog), (41, 7));
        assert_eq!(
            (cm41.param, cm41.tag, cm41.series),
            (0x0B00, 0x0705, 0x0409)
        );
        // 帧 213 的模式在 Recog（`SendDefMessage` 的第 2 参）。
        let cm213 = CommandMessage::from_bytes(&frames[3]).unwrap();
        assert_eq!((cm213.ident, cm213.recog), (213, 2));

        // `AllowGroup=false` ⇒ `Tag1 = 0`（oracle 的 `else` 支）。
        let mut base2 = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let wx2 = WorldFrameInputs {
            logon: Some(LogonInputs {
                allow_group: false,
                ..logon
            }),
            ..wx
        };
        let _ = apply(&mut base2, &wx2);
        assert_eq!(
            base2.logon_body_wl.map(|w| w.2),
            Some(0),
            "AllowGroup=false ⇒ Tag1=0"
        );
    }

    /// **F-3（reviewer-2 `8508d321`）**：生产形态直连金标准——裸男（无 `hum_items` / 空 `stditems`）
    /// 经 `feature_shapes` 得 `(None, None)`，`apply()` 出的 50 body `Param1` 必须 = 金标准 seq17 的
    /// `0x0002_0000`（`Hair=1`/gender=0 ⇒ `hair_byte=2`），41 的 `Param/Tag/Series = 0/2/0`、213 的 Recog=0。
    #[test]
    fn naked_man_logon_matches_golden_50_bytes() {
        // 生产形态：无装备记录 + 空形状表 ⇒ 两段都计 0（oracle `GetStdItem == null` 分支）。
        let (dress, weapon, warns) =
            feature_shapes(None, &game_svr::ability::StdItemCatalog::new());
        assert_eq!((dress, weapon), (None, None));
        assert!(warns.is_empty(), "无装备 ⇒ 无 WRN：{warns:?}");
        let logon = LogonInputs {
            feature: FeatureInputs {
                dress_shape: dress,
                weapon_shape: weapon,
                gender: 0,
                hair: 1,
                on_horse: false,
                horse_type: 0,
                dress_eff_type: 0,
            },
            char_status: 0,
            allow_group: false,
            attack_mode: 0,
        };
        let mut base = EnterWorldInputs {
            actor_id: 7,
            cur_x: 291,
            cur_y: 666,
            dir: 4,
            light: 0,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: None,
            map_desc: None,
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: Some(logon),
            ability: None,
            ..WorldFrameInputs::default()
        };
        let _ = apply(&mut base, &wx);
        // 金标准 seq17：`Param1=0x0002_0000`（`MakeHumanFeature(0,0,0,2)`）、其余 0。
        assert_eq!(
            base.logon_body_wl,
            Some((0x0002_0000, 0, 0, 0)),
            "裸男 50 body 必须 = 金标准 seq17"
        );
        assert_eq!(base.feature, Some((0x0002_0000, 0)));
        let frames = base.frames();
        // 658 恒发（R1-1：空尾也发 header-only），随后同组 50/41/213，最后 RM_LOGON 的 654。
        assert_eq!(idents(&frames), vec![658, 50, 41, 213, 654]);
        // 金标准 seq18：41 的 `Param/Tag/Series = Lo/HiWord(feature)/featureEx = 0/2/0`。
        let cm41 = CommandMessage::from_bytes(&frames[2]).unwrap();
        assert_eq!((cm41.param, cm41.tag, cm41.series), (0, 2, 0));
        // 金标准 seq19：213 的 Recog=0。
        let cm213 = CommandMessage::from_bytes(&frames[3]).unwrap();
        assert_eq!(cm213.recog, 0);
    }

    /// `feature_shapes`：`Index == 0`（未穿）/ 无记录 ⇒ `None` 且无 WRN；`Index > 0`（带装备）⇒
    /// 从 `stditems` 表查 `Shape`（1-based）；查不到（越界/空表）⇒ 计 0 + WRN（oracle `GetStdItem == null`）。
    #[test]
    fn feature_shapes_resolves_shape_from_item_table() {
        let empty = ServerUserItem {
            make_index: 0,
            index: 0,
            dura: 0,
            dura_max: 0,
            desc: None,
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        };
        let worn = ServerUserItem {
            index: 2,
            ..empty.clone()
        };
        // `Index=1` ⇒ items[0]（木剑 shape=5）；`Index=2` ⇒ items[1]（布衣 shape=3）。
        let table = vec![
            StdItem {
                name: "木剑".into(),
                shape: 5,
                ..StdItem::default()
            },
            StdItem {
                name: "布衣".into(),
                shape: 3,
                ..StdItem::default()
            },
        ];
        // Dress 槽 Index=2（布衣 shape=3）、Weapon 槽空。
        let items = vec![Some(worn), Some(empty.clone())];
        let catalog = crate::items::catalog_from(&table);
        let (dress, weapon, warns) = feature_shapes(Some(&items), &catalog);
        assert_eq!(
            (dress, weapon),
            (Some(3), None),
            "Dress 从 stditems 查得 Shape=3"
        );
        assert!(warns.is_empty(), "命中不 WRN：{warns:?}");
        // 空目录 ⇒ 越界 ⇒ 计 0 + WRN（oracle 空物品系统分支）。
        let empty_catalog = game_svr::ability::StdItemCatalog::new();
        let (dress, weapon, warns) = feature_shapes(Some(&items), &empty_catalog);
        assert_eq!((dress, weapon), (None, None));
        assert_eq!(warns.len(), 1, "只有 Dress 槽带装备：{warns:?}");
        assert!(warns[0].contains("Index=2"), "{warns:?}");
        // 空槽不 WRN。
        let all_empty = vec![None, Some(empty)];
        let (_, _, warns) = feature_shapes(Some(&all_empty), &catalog);
        assert!(warns.is_empty(), "空槽不 WRN：{warns:?}");
        assert!(feature_shapes(None, &catalog).2.is_empty());
    }

    /// 帧 52/752 的生产路径接线（`brief-m2svr-next.md` §18）：`apply` 必须用**已合入的**
    /// `game_svr::ability::recalc_abilitys`（禁止第二份实现），且 52 的尾 = `record::encode_ability(WAbil)`、
    /// 752 的 7 字段 = `SubAbility::to_frame_bytes()`；两条的相对序（`52` → `752`）不变。
    #[test]
    fn ability_frames_follow_recalc_abilitys() {
        let table = vec![StdItem {
            name: "木剑".into(),
            shape: 5,
            weight: 4,
            ..StdItem::default()
        }];
        let catalog = crate::items::catalog_from(&table);
        // 金标准 #12 的裸男形态：level=1 战士，`Abil` 已含等级公式算出的值。
        let abil = Ability {
            level: 1,
            reserved1: 0,
            ac: 0,
            mac: 0,
            dc: 0x0101,
            mc: 0,
            sc: 0,
            hp: 15,
            mp: 15,
            max_hp: 19,
            max_mp: 15,
            reserved2: 0,
            exp_count: 0,
            exp_max_count: 0,
            exp: 0,
            max_exp: 10,
            weight: 0,
            max_weight: 50,
            wear_weight: 0,
            max_wear_weight: 15,
            hand_weight: 0,
            max_hand_weight: 12,
        };
        let bonus = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        };
        let use_items = vec![None; 13];
        let bag = vec![Some(ServerUserItem {
            make_index: 0,
            index: 1,
            dura: 10,
            dura_max: 10,
            desc: None,
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        })];
        let inputs = AbilityInputs {
            catalog: &catalog,
            abil: &abil,
            bonus_abil: &bonus,
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
            use_items: Some(&use_items),
            bag_items: Some(&bag),
            magics: None,
            magic_table: None,
            gold: 1234,
            client_ability_layout: false,
        };
        let mut base = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: None,
            map_desc: None,
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 56,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: None,
            ability: Some(inputs),
            ..WorldFrameInputs::default()
        };
        let _ = apply(&mut base, &wx);
        let out = ability::recalc_abilitys(
            &catalog,
            &RecalcInputs {
                job: 0,
                abil: &abil,
                bonus_abil: &bonus,
                bonus_tick_hit: 20,
                bonus_tick_speed: 35,
                cur_hp: 15,
                cur_mp: 15,
                use_items: &to_instances(Some(&use_items)),
                bag: &[1],
            },
        );
        let (gold, job, game_gold, wabil) = base.ability.clone().expect("52 必须接线");
        assert_eq!((gold, job, game_gold), (1234, 0, 56), "52 头字段");
        assert_eq!(
            wabil,
            crate::record::encode_ability(&out.wabil),
            "52 尾 = EncodeMessage(WAbil)"
        );
        assert_eq!(out.wabil.hp, 15, "cur_hp 来自记录 Abil（被保留）");
        assert_eq!(out.wabil.weight, 4, "WAbil.Weight 来自背包模板重量");
        assert_eq!(
            base.sub_ability,
            Some(out.sub.to_frame_bytes()),
            "752 = SubAbility 的 7 个单字节通道"
        );
        // 约简输入：`658` 直发；本 fixture 只给六条里的 52/752/46（621/211/811 的帧输入缺席，
        // 但槽位仍占堆位），且 `attack_mode`/`TestServer`/`PayMent` 都是缺省 ⇒ 只有
        // `StartChangeAttackModeHelp` 这 **1 条** SysMsg 槽、没有第 4 条。
        // 自身 `RM_TURN` 不入队——`AddToMap` 被 `FixedHideMode` 挡住（`smframe::frames` 头注）。
        //
        // 在这个**非真实**的约简内容下，四叉堆给出 `752,52`；真实登录至少还有攻击模式文案
        // 一条 SysMsg 槽（2 条 252），全量内容下恒为 `52,752`（真机 13 次抓包一致）——
        // 全槽序判据在 `smframe::tests::login_scenario_*` 与
        // `login_capture_appearance_slots_keep_811_before_46`。
        assert_eq!(
            idents(&base.frames()),
            vec![658, 752, 52],
            "约简输入（仅 1 条 SysMsg 槽）的堆序；真实全量登录序见 smframe 判据"
        );

        // `MIR2_CLIENT_ABILITY_LAYOUT`：冻结客户端 58B 布局（`HIGHHP=True`）——同一 `WAbil`、
        // `HP` 落在客户端读取的 offset 22。生产默认即此布局（上一个断言显式传 `false` 时 = oracle）。
        let client_inputs = AbilityInputs {
            client_ability_layout: true,
            ..inputs
        };
        let mut base_client = EnterWorldInputs {
            actor_id: 7,
            ..EnterWorldInputs::default()
        };
        let wx_client = WorldFrameInputs {
            ability: Some(client_inputs),
            ..wx
        };
        let _ = apply(&mut base_client, &wx_client);
        let (_, _, _, wabil_client) = base_client.ability.clone().expect("52 必须接线");
        assert_eq!(
            wabil_client,
            crate::record::encode_ability_client(&out.wabil),
            "打开开关时 52 尾 = 客户端 58B 记录布局"
        );
        assert_eq!(
            wabil_client.len(),
            58,
            "客户端 TAbility（HIGHHP=True）= 58B"
        );
        assert_eq!(
            i32::from_le_bytes([
                wabil_client[22],
                wabil_client[23],
                wabil_client[24],
                wabil_client[25]
            ]),
            i32::from(out.wabil.hp),
            "客户端从 offset 22 读到的 HP 必须等于重算后的 HP"
        );
    }

    /// **G-3**：`SM_SENDUSEITEMS(621)` / `SM_SENDMYMAGIC(211)` 的装配（逐槽/逐条 + 顺序）。
    ///
    /// 逐字节判据（固定输入 → 期望尾）：
    /// - 621 尾 = `"0/" + EncodeBuffer(ClientItem[slot0]) + "/" + "5/" + EncodeBuffer(ClientItem[slot5]) + "/"`；
    /// - 211 = `(count=1, EncodeBuffer(ClientMagic) + "/")`；
    /// - `frames()` 序在 46 之后、654 之前：`658, 621, 211, 654`。
    #[test]
    fn send_use_items_and_magic_tails_follow_oracle_layout() {
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            3001,
            StdItem {
                name: "ring".into(),
                std_mode: 23,
                dura_max: 0,
                // 右手火把类光源：验证 654 的 `Param` 来自 `GetMyLight()`（不是恒 0）。
                light: 3,
                ..StdItem::default()
            },
        );
        catalog.insert(
            3002,
            StdItem {
                name: "dress".into(),
                std_mode: 10,
                shape: 0,
                dura_max: 30,
                ..StdItem::default()
            },
        );
        let abil = Ability {
            level: 45,
            ..Ability::default()
        };
        let bonus = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 35,
            reserved: 0,
        };
        let mut use_items = vec![None; 13];
        use_items[0] = Some(ServerUserItem {
            make_index: 11,
            index: 3002,
            dura: 20,
            dura_max: 30,
            desc: None,
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        });
        use_items[5] = Some(ServerUserItem {
            make_index: 7,
            index: 3001,
            dura: 1,
            dura_max: 2,
            desc: None,
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        });
        let magic = mir2_data::MagicInfo {
            magic_id: 17,
            magic_name: "heal".into(),
            ..mir2_data::MagicInfo::default()
        };
        let table = game_svr::magics::MagicTable::new(vec![magic.clone()]);
        let magics = vec![Some(MagicRcd {
            mag_idx: 17,
            level: 2,
            magic_key: 0x41,
            tran_point: 100,
        })];
        let a = AbilityInputs {
            catalog: &catalog,
            abil: &abil,
            bonus_abil: &bonus,
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
            use_items: Some(&use_items),
            bag_items: None,
            magics: Some(&magics),
            magic_table: Some(&table),
            gold: 0,
            client_ability_layout: false,
        };
        let use_tail = send_use_items_tail(&a, 0).unwrap().expect("621 有尾");
        let item0 = game_svr::itemwire::ClientItem {
            tpl: catalog.get(3002).unwrap(),
            name: "dress".into(),
            make_index: 11,
            dura: 20,
            dura_max: 30,
            desc: [0; 14],
        };
        let item5 = game_svr::itemwire::ClientItem {
            tpl: catalog.get(3001).unwrap(),
            name: "ring".into(),
            make_index: 7,
            dura: 1,
            dura_max: 2,
            desc: [0; 14],
        };
        let mut expect = Vec::new();
        expect.extend_from_slice(b"0/");
        expect.extend_from_slice(&edcode::encode(&game_svr::itemwire::encode_client_item(
            &item0,
        )));
        expect.extend_from_slice(b"/5/");
        expect.extend_from_slice(&edcode::encode(&game_svr::itemwire::encode_client_item(
            &item5,
        )));
        expect.push(b'/');
        assert_eq!(
            use_tail, expect,
            "621 逐槽 i + '/' + EncodeBuffer(ClientItem) + '/'"
        );

        let (count, magic_tail) = send_my_magic(&a).expect("211 有尾");
        assert_eq!(count, 1, "series = MagicList.Count");
        let mut expect_magic = edcode::encode(&game_svr::itemwire::encode_client_magic(
            0x41, 2, 100, &magic,
        ));
        expect_magic.push(b'/');
        assert_eq!(magic_tail, expect_magic);

        let base = EnterWorldInputs {
            actor_id: 7,
            use_items_tail: Some(use_tail),
            magic: Some((count, magic_tail)),
            changelight: Some((0, game_svr::sessionwire::N_CLIENT_KEY_DEFAULT)),
            // 真实登录必有 `AttatckMode`（记录 byte）+ `TestServer=TRUE`（部署件）⇒ 4 条 SysMsg
            // 全部占堆；否则堆布局变化会把 621/211 的先后翻过来（与真机不符）。
            attack_mode: Some(0),
            test_server: true,
            pay_ment_three: true,
            ..EnterWorldInputs::default()
        };
        assert_eq!(
            idents(&base.frames()),
            // `attack_mode` 是真实登录必有的记录字段（同时产 213）；4 条 SysMsg 全占堆的
            // 布局下 621 仍先于 211，654 收尾。
            vec![658, 213, 621, 211, 654],
            "order: 621→211，654 收尾"
        );
    }

    /// **F1 牙（review 1 Important）**：`SendUseItems()` 的 oracle 顺序是
    /// `GetUpgradeStdItem` → `GetItemName` → `Dura/DuraMax/MakeIndex` →（服装槽）
    /// `ChangeItemWithLevel` → `ChangeItemByJob` → `EncodeBuffer`（`PlayObject.Base.cs:1029-1041`）。
    ///
    /// 只有 `Desc` 非 0 时「先升级再变换」与「先变换再升级（`encode_client_item` 二次叠差量）」
    /// 才分叉，故本用例用 desc 非 0 的翼装钉住顺序：
    /// 输入 `StdMode 10 / Shape 9 / AC = w(3,250) = 0xFA03 / Desc[0] = 10 / Abil.Level = 50`。
    ///
    /// 手算（期望全部为字面量，**不**由 `encode_client_item` 生成）：
    /// - `GetUpgradeStdItem`（StdMode 10）：`AC = MakeWord(Lo, _MIN(255, Hi+10)) = 0xFF03`；
    /// - `ChangeItemWithLevel`（Shape 9、Level ≥ 50）：
    ///   `AC = (ushort)(0xFF03 + MakeWord(9,7)) = (ushort)0x1060C = 0x060C`；
    ///   `DC = 0x0500 / MC = 0x0600 / SC = 0x0600 / MAC = 0x0402`。
    ///
    /// 牙：把 `send_use_items_tail` 的顺序改回旧的「先 `change_item_with_level/by_job`，
    /// 再用 `encode_client_item` 在已变换模板上叠差量」⇒ `AC = 0x0B0C`，本用例红。
    #[test]
    fn send_use_items_applies_upgrade_before_level_transform() {
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            3003,
            StdItem {
                name: "翼装".into(),
                std_mode: 10,
                shape: 9,
                ac: 0xFA03, // = w(3, 250)：Lo=3、Hi=250
                ..StdItem::default()
            },
        );
        let abil = Ability {
            level: 50,
            ..Ability::default()
        };
        let bonus = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        };
        let mut use_items = vec![None; 13];
        use_items[ITEM_LOCATION_DRESS] = Some(ServerUserItem {
            make_index: 21,
            index: 3003,
            dura: 7,
            dura_max: 30,
            desc: Some(vec![10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        });
        let a = AbilityInputs {
            catalog: &catalog,
            abil: &abil,
            bonus_abil: &bonus,
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
            use_items: Some(&use_items),
            bag_items: None,
            magics: None,
            magic_table: None,
            gold: 0,
            client_ability_layout: false,
        };
        let tail = send_use_items_tail(&a, 0).unwrap().expect("621 有尾");

        // 手算的「升级差量 → ChangeItemWithLevel」成品模板（字段全是字面量）。
        let expect_tpl = StdItem {
            name: "翼装".into(),
            std_mode: 10,
            shape: 9,
            ac: 0x060C,
            dc: 0x0500,
            mc: 0x0600,
            sc: 0x0600,
            mac: 0x0402,
            ..StdItem::default()
        };
        let expect_ci = game_svr::itemwire::ClientItem {
            tpl: &expect_tpl,
            name: "翼装".into(),
            make_index: 21,
            dura: 7,
            dura_max: 30,
            desc: [10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        };
        let mut expect = Vec::new();
        expect.extend_from_slice(b"0/");
        expect.extend_from_slice(&edcode::encode(
            &game_svr::itemwire::encode_client_item_prepared(&expect_ci),
        ));
        expect.push(b'/');
        assert_eq!(
            tail, expect,
            "621 逐槽 i + '/' + EncodeBuffer(ClientItem) + '/'（升级先于等级变换）"
        );

        // 直接钉住线缆里的 AC 两字节（解 0/ 与尾 / 后的 124B 体，AC 在 offset 26）。
        let body = edcode::decode(&tail[2..tail.len() - 1]);
        assert_eq!(
            body.len(),
            124,
            "ClientItem 体长（name 15 + 7 + looks 2 + DuraMax 2 + AC 2 + …）"
        );
        assert_eq!(
            (body[26], body[27]),
            (0x0C, 0x06),
            "AC 低/高字节 = 0x060C（先升级后 ChangeItemWithLevel）"
        );
        assert_ne!(
            (body[26], body[27]),
            (0x0C, 0x0B),
            "不是双叠值 0x0B0C（旧顺序会得到这个）"
        );
    }

    /// 真机 `s2c-621c` 的 211 帧**整帧**（`capture.jsonl` seq=17 的 `raw_hex`）：
    /// 玩家有 1 个有效技能（`MagicId=3`「基本剑术」、`UseKey='1'`、Level 0、CurTrain 0）。
    const CAPTURE_621C_211_RAW: &str =
        "236464686b645b686b6464686b6465686b6164686b6464686b64646b6b6464534951414e43413f624f516468676464686b6464686b6464686b6464676b6768786354646863645c69686464545b6564686b506568676467686b6464686b6464686b6464686b6464686b6464686b6464686b6464686b6464686b6464686b6464686b2f21";

    /// `sql/mir2_data.sql:70` 的 `magics` 第 3 行（`基本剑术`），字段映射 1:1 照
    /// `mir2_data::loaders::DataLoaders::load_magic_db`（`NeedL3` 复制进 `TrainLevel[3]`、
    /// `L3Train` 复制进 `MaxTrain[3]`、`TrainLv` 恒 3、`Descr` 为空 ⇒ 空串）。
    fn magic_3_from_sql() -> mir2_data::MagicInfo {
        mir2_data::MagicInfo {
            magic_id: 3,
            magic_name: "基本剑术".into(),
            effect_type: 0,
            effect: 0,
            spell: 0,
            power: 0,
            max_power: 0,
            job: 0,
            train_level: [7, 11, 16, 16],
            max_train: [200, 300, 500, 500],
            train_lv: 3,
            delay_time: 0,
            def_spell: 0,
            def_power: 0,
            def_max_power: 0,
            desc: String::new(),
        }
    }

    /// `playdata.rs:206` 的口径：DBSvr 恒填 20 个**非 `None`** 槽，空槽是 `MagicRcd` 的
    /// `MagIdx=0` 默认值。oracle 装 `MagicList` 时按 `FindMagic(MagIdx)` 过滤，故这些槽既不进
    /// `series`、也不产生载荷。
    fn empty_magic_slots() -> Vec<Option<MagicRcd>> {
        vec![
            Some(MagicRcd {
                mag_idx: 0,
                level: 0,
                magic_key: 0,
                tran_point: 0,
            });
            20
        ]
    }

    fn bonus_none() -> NakedAbility {
        NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        }
    }

    fn magic_inputs<'a>(
        slots: &'a [Option<MagicRcd>],
        table: &'a game_svr::magics::MagicTable,
        catalog: &'a game_svr::ability::StdItemCatalog,
        abil: &'a Ability,
        bonus: &'a NakedAbility,
    ) -> AbilityInputs<'a> {
        AbilityInputs {
            catalog,
            abil,
            bonus_abil: bonus,
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
            use_items: None,
            bag_items: None,
            magics: Some(slots),
            magic_table: Some(table),
            gold: 0,
            client_ability_layout: false,
        }
    }

    /// 判据 1：有 1 个有效技能 ⇒ 211 出现，且**整帧**逐字节等于真机 621c。
    #[test]
    fn one_valid_magic_211_matches_capture_621c_byte_for_byte() {
        let catalog = game_svr::ability::StdItemCatalog::new();
        let abil = Ability::default();
        let bonus = bonus_none();
        let table = game_svr::magics::MagicTable::new(vec![magic_3_from_sql()]);
        let mut slots = empty_magic_slots();
        slots[0] = Some(MagicRcd {
            mag_idx: 3,
            level: 0,
            magic_key: u16::from(b'1'),
            tran_point: 0,
        });
        let a = magic_inputs(&slots, &table, &catalog, &abil, &bonus);
        let (count, tail) = send_my_magic(&a).expect("1 个有效技能 ⇒ 211 必须发");
        assert_eq!(count, 1, "series = MagicList.Count（空槽不计）");
        let frame =
            game_svr::smframe::client_frame(&game_svr::smframe::sm_sendmymagic(count, &tail));
        assert_eq!(
            frame,
            hex(CAPTURE_621C_211_RAW),
            "211 整帧（'#' + Encode(头) + Encode(ClientMagic) + '/' + '!'）必须与 621c 逐字节相同"
        );

        let base = EnterWorldInputs {
            magic: Some((count, tail)),
            ..EnterWorldInputs::default()
        };
        assert!(
            idents(&base.frames()).contains(&211),
            "帧序里必须出现 211（621c 登录序 52,752,621,211,811,46）"
        );
    }

    /// 判据 2：无技能（20 个空槽）⇒ 211 不出现（与真机 621b 一致）。
    #[test]
    fn no_valid_magic_emits_no_211_like_capture_621b() {
        let catalog = game_svr::ability::StdItemCatalog::new();
        let abil = Ability::default();
        let bonus = bonus_none();
        let table = game_svr::magics::MagicTable::new(vec![magic_3_from_sql()]);
        let slots = empty_magic_slots();
        let a = magic_inputs(&slots, &table, &catalog, &abil, &bonus);
        assert!(
            send_my_magic(&a).is_none(),
            "空槽全部被 FindMagic 门滤掉 ⇒ sSendMsg 空 ⇒ 一帧都不发"
        );

        let mut base = EnterWorldInputs::default();
        let wx = WorldFrameInputs {
            flag: None,
            map_desc: None,
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: None,
            ability: Some(a),
            ..WorldFrameInputs::default()
        };
        let warns = apply(&mut base, &wx);
        assert!(base.magic.is_none(), "211 不得进入待发帧");
        assert!(
            !idents(&base.frames()).contains(&211),
            "621b 的 35 条 s2c 无 211 ⇒ 这里也不能有"
        );
        assert!(
            !warns.iter().any(|w| w.contains("211")),
            "空槽是 oracle 正常路径，不得登记成告警"
        );
    }

    /// 判据 3：2 条技能按槽序进载荷，`series=2`。
    #[test]
    fn two_magics_keep_slot_order() {
        let catalog = game_svr::ability::StdItemCatalog::new();
        let abil = Ability::default();
        let bonus = bonus_none();
        let fireball = mir2_data::MagicInfo {
            magic_id: 1,
            magic_name: "火球术".into(),
            effect_type: 1,
            effect: 1,
            spell: 4,
            power: 8,
            max_power: 8,
            job: 1,
            train_level: [7, 11, 16, 16],
            max_train: [200, 300, 500, 500],
            train_lv: 3,
            delay_time: 60,
            def_spell: 1,
            def_power: 2,
            def_max_power: 2,
            desc: String::new(),
        };
        let table = game_svr::magics::MagicTable::new(vec![magic_3_from_sql(), fireball.clone()]);
        let first = MagicRcd {
            mag_idx: 3,
            level: 0,
            magic_key: u16::from(b'1'),
            tran_point: 0,
        };
        let second = MagicRcd {
            mag_idx: 1,
            level: 2,
            magic_key: u16::from(b'2'),
            tran_point: 5,
        };
        let mut slots = empty_magic_slots();
        slots[0] = Some(first.clone());
        slots[1] = Some(second.clone());
        let a = magic_inputs(&slots, &table, &catalog, &abil, &bonus);
        let (count, tail) = send_my_magic(&a).expect("2 条技能 ⇒ 211");
        assert_eq!(count, 2);

        let mut expect = edcode::encode(&game_svr::itemwire::encode_client_magic(
            first.magic_key,
            first.level,
            first.tran_point,
            &magic_3_from_sql(),
        ));
        expect.push(b'/');
        expect.extend_from_slice(&edcode::encode(&game_svr::itemwire::encode_client_magic(
            second.magic_key,
            second.level,
            second.tran_point,
            &fireball,
        )));
        expect.push(b'/');
        assert_eq!(tail, expect, "槽序 = 载荷序（MagicList 按槽序装入）");
    }

    /// 判据 4：空槽混在**中间**不产生垃圾条目；查无模板的有效槽同样被装表门丢弃。
    #[test]
    fn empty_slots_in_the_middle_produce_no_garbage() {
        let catalog = game_svr::ability::StdItemCatalog::new();
        let abil = Ability::default();
        let bonus = bonus_none();
        let table = game_svr::magics::MagicTable::new(vec![magic_3_from_sql()]);
        let first = MagicRcd {
            mag_idx: 3,
            level: 0,
            magic_key: u16::from(b'1'),
            tran_point: 0,
        };
        let mut slots = empty_magic_slots();
        slots[7] = Some(first.clone());
        let a = magic_inputs(&slots, &table, &catalog, &abil, &bonus);
        let (count, tail) = send_my_magic(&a).expect("中间空槽不得把整帧吞掉");
        assert_eq!(count, 1, "series 只算有效条目");
        let mut expect = edcode::encode(&game_svr::itemwire::encode_client_magic(
            first.magic_key,
            first.level,
            first.tran_point,
            &magic_3_from_sql(),
        ));
        expect.push(b'/');
        assert_eq!(tail, expect, "载荷恰为 1 条有效技能（无 0 号空槽垃圾）");
        assert_eq!(tail.len(), 113, "1 条技能尾 = enc(84) + '/' = 113");

        // 有效技能 + 魔表查无此项的槽：后者按 oracle 装表门丢弃（不 Err、不占 series）。
        let mut slots2 = empty_magic_slots();
        slots2[0] = Some(first);
        slots2[1] = Some(MagicRcd {
            mag_idx: 999,
            level: 1,
            magic_key: u16::from(b'9'),
            tran_point: 0,
        });
        let a2 = magic_inputs(&slots2, &table, &catalog, &abil, &bonus);
        let (count2, tail2) = send_my_magic(&a2).expect("有效技能仍在 ⇒ 211");
        assert_eq!(count2, 1, "查无模板的槽同样不进 MagicList");
        assert_eq!(tail2, expect);
    }

    /// **G-7**：`apply` 用装备的 `StdItem.Light` 算 `BaseObject.Light`，同时喂
    /// `SM_LOGON` 的 `HiByte(series)` 与 `SM_CHANGELIGHT(654).Param`。
    #[test]
    fn apply_light_feeds_logon_series_and_654_param() {
        let mut catalog = game_svr::ability::StdItemCatalog::new();
        catalog.insert(
            3001,
            StdItem {
                name: "torch".into(),
                std_mode: 3,
                light: 3,
                ..StdItem::default()
            },
        );
        let abil = Ability {
            level: 10,
            ..Ability::default()
        };
        let bonus = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 35,
            reserved: 0,
        };
        let mut use_items = vec![None; 13];
        use_items[2] = Some(ServerUserItem {
            make_index: 7,
            index: 3001,
            dura: 1,
            dura_max: 2,
            desc: None,
            color_r: 0,
            color_g: 0,
            color_b: 0,
            prefix: None,
        });
        let a = AbilityInputs {
            catalog: &catalog,
            abil: &abil,
            bonus_abil: &bonus,
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
            use_items: Some(&use_items),
            bag_items: None,
            magics: None,
            magic_table: None,
            gold: 0,
            client_ability_layout: false,
        };
        let mut base = EnterWorldInputs {
            actor_id: 7,
            dir: 4,
            ..EnterWorldInputs::default()
        };
        let wx = WorldFrameInputs {
            flag: None,
            map_desc: None,
            conf: None,
            bonus_abil: None,
            soft_version_date: 0,
            game_gold: 0,
            game_point: 0,
            job: 0,
            bonus_point: 0,
            game_time: None,
            strings: None,
            show_name: None,
            logon: Some(LogonInputs::default()),
            ability: Some(a),
            ..WorldFrameInputs::default()
        };
        let _ = apply(&mut base, &wx);
        assert_eq!(base.light, 3, "右手火把 Light=3");
        assert_eq!(
            base.changelight,
            Some((3, game_svr::sessionwire::N_CLIENT_KEY_DEFAULT))
        );
        let frames = base.frames();
        let cm50 = CommandMessage::from_bytes(&frames[1]).unwrap();
        assert_eq!(cm50.series >> 8, 3, "SM_LOGON 的 HiByte(series)=Light");
        let cm654 = CommandMessage::from_bytes(frames.last().unwrap()).unwrap();
        assert_eq!(cm654.ident, 654);
        assert_eq!(
            (cm654.param, cm654.tag),
            (3, 500),
            "654 Param=Light/Tag=500"
        );
    }

    /// **G-11**：地图文件名走 GBK 字节（`EDCode.EncodeString` 对 `HUtil32.GetBytes(gb2312)`）。
    #[test]
    fn map_file_name_uses_gbk_not_utf8() {
        assert_eq!(gbk("地图"), vec![0xB5, 0xD8, 0xCD, 0xBC]);
        assert_ne!(
            gbk("地图"),
            "地图".as_bytes().to_vec(),
            "UTF-8 字节不是 oracle 口径"
        );
        assert_eq!(gbk("0"), b"0".to_vec(), "现网 ASCII 地图名不变");
    }

    /// **Lane B / 211 空槽修复**：DB 回包的 20 槽里常含 `MagIdx=0` 空槽与模板缺失项；
    /// oracle `GetHumData` 装载时已丢弃它们 ⇒ 211 只发有效条目、`Count` 只数有效项。
    /// 牙：恢复旧行为（遇到空槽整条跳过 / 把空槽计入 Count）⇒ 本用例红。
    #[test]
    fn send_my_magic_skips_empty_and_missing_slots() {
        let magic = mir2_data::MagicInfo {
            magic_id: 17,
            magic_name: "heal".into(),
            ..mir2_data::MagicInfo::default()
        };
        let table = game_svr::magics::MagicTable::new(vec![magic.clone()]);
        let empty = Some(MagicRcd {
            mag_idx: 0,
            level: 3,
            magic_key: 9,
            tran_point: 77,
        });
        let valid = Some(MagicRcd {
            mag_idx: 17,
            level: 2,
            magic_key: 0x41,
            tran_point: 100,
        });
        let missing = Some(MagicRcd {
            mag_idx: 9999,
            level: 1,
            magic_key: 1,
            tran_point: 2,
        });
        let magics = vec![None, empty, valid, None, missing];
        let abil = Ability {
            level: 1,
            ..Ability::default()
        };
        let bonus = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        };
        let a = AbilityInputs {
            catalog: &game_svr::ability::StdItemCatalog::new(),
            abil: &abil,
            bonus_abil: &bonus,
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
            use_items: None,
            bag_items: None,
            magics: Some(&magics),
            magic_table: Some(&table),
            gold: 0,
            client_ability_layout: false,
        };
        let (count, tail) = send_my_magic(&a).expect("211 有尾");
        assert_eq!(count, 1, "Count = MagicList.Count（只数有效项）");
        let mut expect = edcode::encode(&game_svr::itemwire::encode_client_magic(
            0x41, 2, 100, &magic,
        ));
        expect.push(b'/');
        assert_eq!(tail, expect, "只发有效条目（空槽/缺模板都不进 211）");
    }
}
