//! 行会状态机（`GuildInfo` / `GuildManager` 的会话态与成员/封号/公告/联盟/解散）
//! —— M3⑧-实现 4 第①片（卡 `openmir2-m3-guild-state`）。
//!
//! 盘点依据：`mir2-rs/tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md` §1.A / §1.B / §1.C / §1.L。
//! oracle 锚（行号均在本仓库 `src/` 内实测）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`GuildState::is_member`] | `GuildInfo.cs:161-179`（逐 rank 逐 member，**大小写敏感** `==`） |
//! | [`GuildState::get_rank_name`] | `GuildInfo.cs:485-509`（`string.Compare(..., OrdinalIgnoreCase)==0`，命中即回） |
//! | [`GuildState::get_chief_name`] | `GuildInfo.cs:511-523`（`RankList[0].MemberList[0]`） |
//! | [`GuildState::set_guild_info`] | `GuildInfo.cs:463-484`（`RankList.Count==0` ⇒ 建 rank 1 掌门 + 唯一成员） |
//! | [`GuildState::add_member`] | `GuildInfo.cs:623-652`（找/建 `RankNo==99`，`GuildMemberRank`） |
//! | [`GuildState::del_member`] | `GuildInfo.cs:653-679`（首个 `==` 命中移除 + `UpdateGuildFile`） |
//! | [`GuildState::cancel_guld`] | `GuildInfo.cs:680-698`（单 rank 单成员 + `OrdinalIgnoreCase` ⇒ `BackupGuildFile`） |
//! | [`GuildState::update_rank`] | `GuildInfo.cs:708-993`（返回码 `-1..-7`；**N3**：成功路径 `RankList = 空表`） |
//! | [`GuildState::ally_guild`] / [`GuildState::del_ally_guild`] / [`GuildState::is_ally_guild`] | `GuildInfo.cs:129-159`/`:1007-1019`（`GuildAllList.Add` 被注释 ⇒ **联盟恒空**） |
//! | [`GuildState::add_war_guild`] | `GuildInfo.cs:1020-1056`（**N2**：条件反置 ⇒ 未战不生效 / 已战追加重复） |
//! | [`GuildState::end_guild_war`] | `GuildInfo.cs:1058-1061` |
//! | [`GuildState::check_save_guild_file`] | `GuildInfo.cs:525-533`（`BoChanged && >30s` ⇒ 落盘） |
//! | [`GuildState::del_human_obj`] | `GuildInfo.cs:534-550`（**附**：`Compare(...)==1` 而非 `!=0`） |
//! | [`GuildState::count`] / [`GuildState::is_full`] | `GuildInfo.cs:1064-1072`（`Count = Σ MemberList.Count`，`IsFull = Count >= GuildMemberMaxLimit`） |
//! | [`GuildManager`] | `GuildManager.cs:19-167`（`AddGuild`/`DelGuild`/`FindGuild`/`MemberOfGuild`/`LoadGuildInfo`/`Run`） |
//! | [`check_guild_name`] | `M2Share.cs:529-545`（**N4**：循环 `i < Length-1` ⇒ 末字符不比黑名单） |
//! | [`GuildState::member_list_text`] | `PlayObject.Operate.cs:1283-1306`（`#RankNo/*RankName/` + 逐成员 + `/`，`>5000` 截断） |
//!
//! ## 照搬的 oracle 怪行为（硬约束 3，不是差异）
//!
//! 1. **N2 `AddWarGuild` 条件反置**（`GuildInfo.cs:1039`）：循环找到战条后把字段写进 **record struct 的副本**
//!    （不写回 `GuildWarList`），再判 `if (warGuild.WarTime > 0)`。⇒ 未在战 ⇒ 返回 `default`（`WarTick=0`）、
//!    **不生效**；已在战 ⇒ 副本 `WarTime>0` ⇒ **再追加一条重复**。本模块照搬（[`GuildState::add_war_guild`]）。
//! 2. **N3 `UpdateRank` 成功即清空成员表**（`GuildInfo.cs:970-971`）：`//RankList = guildRankList;` 被注释，
//!    实际 `RankList = rankListTmp`（空表）⇒ 成功路径把 `ranks` 置空。
//! 3. **N4 `CheckGuildName` 漏比末字符**（`M2Share.cs:536`）：循环上界 `Length-1`。
//! 4. **`DelHumanObj` 的 `Compare(...)==1`**（`GuildInfo.cs:540`）：只命中「大于」，等值/小于都不动。
//! 5. **`GuildAllList` 恒空**（`GuildInfo.cs:1016` 的 `Add` 被注释；加载分支 `:311` 同）：`ally_guild` 只落盘不登记。
//!
//! 这些将来若要「修好」，另开 `S-*` 卡，**不在本卡**。
//!
//! ## 本片范围（第①片）
//!
//! 只做 **离线 crates/world 状态机**：数据 + 全部成员/封号/公告/联盟/解散变换 + `GuildManager`，
//! 加 `check_guild_name` + 成员列表文本 + 构造级向量 + 改坏必红。
//! 明确不含：9 个 `CM_GUILD*` 收包派发、`SM_GUILD*` 帧、`MyGuild`/`GuildRankNo` 的角色侧接线、
//! 面板文本（`<KillGuilds>` 里 oracle 直接 `+` record struct 会打出 `WarGuild { Guild = … }` 字面量，
//! 属表现差异登记）、落盘 IO（走 `mir2-data` 的 `GuildStore`，由 第②片 接线）；这些是
//! [`openmir2-m3-guild-state`] 的第②片（写集含 `bins/game-svr`，等 owner 对 bins 串行序裁决）。
//!
//! ## 确定性
//!
//! 本模块**零 RNG、零墙钟**：时间全部来自调用方注入的 `now_tick`（`HUtil32.GetTickCount` 口径，
//! **i32** 与 oracle 的 `int` 一致；生产侧由 `world.clock_ms as i32` 提供）。遍历顺序 = `Vec` 序。

use std::cmp::Ordering;

use mir2_data::guild::{
    GuildConfValues, GuildFileBody, GuildRankRecord, GuildRecord, WarGuildRecord,
};
use mir2_shared::hutil32::{get_valid_str3, str_to_int16, to_upper};

/// 行会消息前景色（`GameSvrConf.cs:1542` 初值 0xDB；读取点 `ServerConf.cs:335`）。
pub const GUILD_MSG_F_COLOR_DEFAULT: u8 = 0xDB;
/// 行会消息背景色（`GameSvrConf.cs:1543` 初值 0xFF；读取点 `ServerConf.cs:336`）。
pub const GUILD_MSG_B_COLOR_DEFAULT: u8 = 0xFF;
/// 行会召回时限（`GameSvrConf.cs:1701` 初值 180；读取点 `ServerConf.cs:479`）。
pub const GUILD_RECALL_TIME_DEFAULT: i32 = 180;
/// 封号名长度上限（`GameSvrConf.cs:1720` 初值 16；读取点 `ServerConf.cs:498`）。
pub const GUILD_RANK_NAME_LEN_DEFAULT: u8 = 16;
/// 掌门人封号（`GameSvrConf.cs:1855`，`SetGuildInfo` 用）。
pub const GUILD_CHIEF_DEFAULT: &str = "掌门人";
/// 普通成员封号（`GameSvrConf.cs:1854`，`AddMember` 用）。
pub const GUILD_MEMBER_RANK_DEFAULT: &str = "行会成员";
/// `GuildWarTime` 初值（`GameSvrConf.cs:1447`，注释「3 小时」）。
pub const GUILD_WAR_TIME_DEFAULT: i32 = 10_800_000;
/// `GuildMemberMaxLimit` 初值（`GameSvrConf.cs:1721`）。
pub const GUILD_MEMBER_MAX_LIMIT_DEFAULT: i32 = 200;
/// `GuildNameLen` 初值（`GameSvrConf.cs` 的 `GuildNameLen`，现网 `Server.conf` = 15）。
pub const GUILD_NAME_LEN_DEFAULT: u8 = 15;

/// `PlayObject.Operate.cs:1329` 的硬编码门：`MyGuild.RankList.Count < 400`。
pub const GUILD_RANK_LIST_LIMIT: usize = 400;
/// `GuildInfo.cs:527` 的落盘节流：`(GetTickCount() - DwSaveTick) > 30 * 1000`。
pub const GUILD_SAVE_THROTTLE_MS: i32 = 30_000;
/// `GuildInfo.cs:731` 的封号名硬截断：`if (sRankName.Length > 30) sRankName = sRankName[..30]`。
pub const GUILD_RANK_NAME_MAX: usize = 30;
/// `GuildInfo.cs:772` 的单行成员上限：`count >= 10` 即停（`UpdateRank` 的 `while` 内计数）。
pub const GUILD_RANK_MEMBERS_PER_LINE: usize = 10;
/// 普通成员封号号（`GuildInfo.cs:626/637`：`RankNo == 99`）。
pub const GUILD_RANK_NO_MEMBER: i16 = 99;
/// 掌门封号号（`GuildInfo.cs:467`：`RankNo = 1`）。
pub const GUILD_RANK_NO_CHIEF: i16 = 1;
/// `LoadGuildFile` 的分词符：`new char[] { ' ', ',' }`（`GuildInfo.cs:207`）。
pub const GUILD_SPLIT: [char; 2] = [' ', ','];

/// 行会配置（host 从 `crates/data` 的 `GuildCastleConfig` + `GuildConf` 组装；默认值 = `GameSvrConf` 初值）。
///
/// **O-1 三键**（⑧-2 reviewer 从 `ServerConf.cs` 转入本卡）：`guild_msg_f_color`（`:335`）、
/// `guild_msg_b_color`（`:336`）、`guild_recall_time`（`:479`）——⑧ 盘点 §1.K 未列，本卡显式建模。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildConfig {
    /// `[Setup] GuildWarTime`（`ServerConf.cs:644`；缺键默认 `GameSvrConf.cs:1447`）。
    pub guild_war_time: i32,
    /// `[Setup] GuildMemberMaxLimit`（`ServerConf.cs:500`）。
    pub guild_member_max_limit: i32,
    /// `[Setup] GuildRankNameLen`（`ServerConf.cs:498`）。
    pub guild_rank_name_len: u8,
    /// `[Setup] GuildNameLen`（`ServerConf.cs:499`）。
    pub guild_name_len: u8,
    /// `[String] GuildChief`（`GameSvrConf.cs:1855`；`ServerConf` 无读取点）。
    pub guild_chief: String,
    /// `[String] GuildMemberRank`（`GameSvrConf.cs:1854`；`ServerConf` 无读取点）。
    pub guild_member_rank: String,
    /// `[Setup] GuildMsgFColor`（`ServerConf.cs:335`）—— **O-1**。
    pub guild_msg_f_color: u8,
    /// `[Setup] GuildMsgBColor`（`ServerConf.cs:336`）—— **O-1**。
    pub guild_msg_b_color: u8,
    /// `[Setup] GuildRecallTime`（`ServerConf.cs:479`）—— **O-1**。
    pub guild_recall_time: i32,
}

impl Default for GuildConfig {
    fn default() -> Self {
        Self {
            guild_war_time: GUILD_WAR_TIME_DEFAULT,
            guild_member_max_limit: GUILD_MEMBER_MAX_LIMIT_DEFAULT,
            guild_rank_name_len: GUILD_RANK_NAME_LEN_DEFAULT,
            guild_name_len: GUILD_NAME_LEN_DEFAULT,
            guild_chief: GUILD_CHIEF_DEFAULT.to_string(),
            guild_member_rank: GUILD_MEMBER_RANK_DEFAULT.to_string(),
            guild_msg_f_color: GUILD_MSG_F_COLOR_DEFAULT,
            guild_msg_b_color: GUILD_MSG_B_COLOR_DEFAULT,
            guild_recall_time: GUILD_RECALL_TIME_DEFAULT,
        }
    }
}

/// `SystemModule.Data.GuildRank`（`GuildInfo.cs:5-10`）。`members` 只留成员名，
/// oracle 的 `IPlayerActor` 引用由宿主按名解析（第②片）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildRank {
    /// `RankNo`（`1` = 掌门、`99` = 普通成员）。
    pub rank_no: i16,
    /// `RankName`。
    pub rank_name: String,
    /// `MemberList[*].MemberName`。
    pub members: Vec<String>,
}

/// `SystemModule.Data.WarGuild`（`WarGuild.cs`；**record struct** ⇒ `AddWarGuild` 的赋值是副本语义）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WarGuild {
    /// `Guild.GuildName`（oracle 是 `IGuild` 引用，本模块用会名）。
    pub guild_name: String,
    /// `WarTick`。
    pub war_tick: i32,
    /// `WarTime`。
    pub war_time: i32,
}

/// `AddWarGuild` 的返回（含 oracle 在此函数里发出的两条 `SendGuildMsg` 文案）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddWarGuildOutcome {
    /// oracle 返回值（`default` = `war_tick=0`/`war_time=0`）。
    pub result: WarGuild,
    /// `SendGuildMsg` 文案（顺序 = oracle 调用序）。
    pub messages: Vec<String>,
}

/// `GuildManager.Run` 的一行产出（某行会对某敌会到期结束）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildWarEnded {
    /// 本会名。
    pub guild: String,
    /// 敌会名。
    pub enemy: String,
    /// `EndGuildWar` 文案（`SendGuildMsg`）。
    pub message: String,
}

/// 一个行会的会话态（`GuildInfo` 的内存字段子集）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildState {
    /// `GuildName`。
    pub name: String,
    /// `NoticeList`。
    pub notices: Vec<String>,
    /// `GuildWarList`。
    pub wars: Vec<WarGuild>,
    /// `GuildAllList`（会名；oracle 恒空，见模块头 4）。
    pub allies: Vec<String>,
    /// `RankList`。
    pub ranks: Vec<GuildRank>,
    /// `BoChanged`。
    pub bo_changed: bool,
    /// `DwSaveTick`。
    pub dw_save_tick: i32,
    /// `BuildPoint`。
    pub build_point: i32,
    /// `Aurae`。
    pub aurae: i32,
    /// `Stability`。
    pub stability: i32,
    /// `Flourishing`。
    pub flourishing: i32,
    /// `ChiefItemCount`。
    pub chief_item_count: i32,
}

impl GuildState {
    /// `new GuildInfo(sName)` 的字段初始化（`GuildInfo.cs:88-116`，`.ini` 副作用在 第②片）。
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            notices: Vec::new(),
            wars: Vec::new(),
            allies: Vec::new(),
            ranks: Vec::new(),
            bo_changed: false,
            dw_save_tick: 0,
            build_point: 0,
            aurae: 0,
            stability: 0,
            flourishing: 0,
            chief_item_count: 0,
        }
    }

    /// 由 ⑧-3 的 [`GuildRecord`]（`.ini` 五键 + `.txt` 四段）物化（`GuildInfo.LoadGuild`）。
    ///
    /// `wars` 的 `war_tick` 直接沿用解析层写入的 `now_tick`（`GuildInfo.cs:274` 加载时置本刻）。
    #[must_use]
    pub fn from_record(record: &GuildRecord) -> Self {
        let mut state = Self::new(record.name.clone());
        state.notices = record.body.notices.clone();
        state.wars = record
            .body
            .wars
            .iter()
            .map(|w| WarGuild {
                guild_name: w.guild_name.clone(),
                war_tick: w.war_tick,
                war_time: w.war_time,
            })
            .collect();
        state.allies = record.body.allies.clone();
        state.ranks = record
            .body
            .ranks
            .iter()
            .map(|r| GuildRank {
                rank_no: r.rank_no,
                rank_name: r.rank_name.clone(),
                members: r.members.clone(),
            })
            .collect();
        state.set_conf(&record.conf);
        state
    }

    /// 落盘侧：`GuildInfo.SaveGuildFile` 的四段输入（`war_tick` 携带当前值，序列化在 `mir2-data`）。
    #[must_use]
    pub fn to_body(&self) -> GuildFileBody {
        GuildFileBody {
            notices: self.notices.clone(),
            wars: self
                .wars
                .iter()
                .map(|w| WarGuildRecord {
                    guild_name: w.guild_name.clone(),
                    war_tick: w.war_tick,
                    war_time: w.war_time,
                })
                .collect(),
            allies: self.allies.clone(),
            ranks: self
                .ranks
                .iter()
                .map(|r| GuildRankRecord {
                    rank_no: r.rank_no,
                    rank_name: r.rank_name.clone(),
                    members: r.members.clone(),
                })
                .collect(),
        }
    }

    /// 把 `.ini` 五键写回（`GuildConf.LoadConfig(this)`，`GuildInfo.cs:203-205`）。
    pub fn set_conf(&mut self, conf: &GuildConfValues) {
        self.build_point = conf.build_point;
        self.aurae = conf.aurae;
        self.stability = conf.stability;
        self.flourishing = conf.flourishing;
        self.chief_item_count = conf.chief_item_count;
    }

    /// `.ini` 五键的当前值。
    #[must_use]
    pub fn conf(&self) -> GuildConfValues {
        GuildConfValues {
            build_point: self.build_point,
            aurae: self.aurae,
            stability: self.stability,
            flourishing: self.flourishing,
            chief_item_count: self.chief_item_count,
        }
    }

    /// `GuildInfo.cs:1064` 的 `Count` = `Σ RankList[*].MemberList.Count`。
    #[must_use]
    pub fn count(&self) -> usize {
        self.ranks.iter().map(|r| r.members.len()).sum()
    }

    /// `GuildInfo.cs:1069` 的 `IsFull` = `Count >= GuildMemberMaxLimit`。
    #[must_use]
    pub fn is_full(&self, cfg: &GuildConfig) -> bool {
        self.count() >= usize::try_from(cfg.guild_member_max_limit).unwrap_or(usize::MAX)
    }

    /// `GuildInfo.cs:161-179` 的 `IsMember`：逐 rank 逐 member，`MemberName == sName`（**大小写敏感**）。
    #[must_use]
    pub fn is_member(&self, name: &str) -> bool {
        for rank in &self.ranks {
            for m in &rank.members {
                if m.is_empty() {
                    continue;
                }
                if m == name {
                    return true;
                }
            }
        }
        false
    }

    /// `GuildInfo.cs:485-509` 的 `GetRankName`：`OrdinalIgnoreCase` 命中首个 ⇒ 返回 `(RankNo, RankName)`。
    #[must_use]
    pub fn get_rank_name(&self, name: &str) -> Option<(i16, String)> {
        for rank in &self.ranks {
            for m in &rank.members {
                if cmp_ignore_case(m, name) == Ordering::Equal {
                    return Some((rank.rank_no, rank.rank_name.clone()));
                }
            }
        }
        None
    }

    /// `GuildInfo.cs:511-523` 的 `GetChiefName`。
    #[must_use]
    pub fn get_chief_name(&self) -> String {
        self.ranks
            .first()
            .and_then(|r| r.members.first())
            .cloned()
            .unwrap_or_default()
    }

    /// `GuildInfo.cs:463-484` 的 `SetGuildInfo`：`ranks` 空 ⇒ 建 rank 1 掌门 + 唯一成员。
    ///
    /// oracle 末尾 `SaveGuildInfoFile()`；本片以 `bo_changed = true` 标记待落盘（IO 在 第②片）。
    pub fn set_guild_info(&mut self, chief: &str, cfg: &GuildConfig) {
        if self.ranks.is_empty() {
            self.ranks.push(GuildRank {
                rank_no: GUILD_RANK_NO_CHIEF,
                rank_name: cfg.guild_chief.clone(),
                members: vec![chief.to_string()],
            });
            self.bo_changed = true;
        }
    }

    /// `GuildInfo.cs:623-652` 的 `AddMember`：找 `RankNo==99`，无则建（`GuildMemberRank`）；追加成员 + `UpdateGuildFile`。
    pub fn add_member(&mut self, name: &str, cfg: &GuildConfig, now_tick: i32) {
        let idx = self
            .ranks
            .iter()
            .position(|r| r.rank_no == GUILD_RANK_NO_MEMBER);
        let idx = match idx {
            Some(i) => i,
            None => {
                self.ranks.push(GuildRank {
                    rank_no: GUILD_RANK_NO_MEMBER,
                    rank_name: cfg.guild_member_rank.clone(),
                    members: Vec::new(),
                });
                self.ranks.len() - 1
            }
        };
        self.ranks[idx].members.push(name.to_string());
        self.update_guild_file(now_tick);
    }

    /// `GuildInfo.cs:653-679` 的 `DelMember`：首个 `==` 命中移除；命中才 `UpdateGuildFile`。
    pub fn del_member(&mut self, name: &str, now_tick: i32) -> bool {
        let mut removed = false;
        'outer: for rank in &mut self.ranks {
            for j in 0..rank.members.len() {
                if rank.members[j] == name {
                    rank.members.remove(j);
                    removed = true;
                    break 'outer;
                }
            }
        }
        if removed {
            self.update_guild_file(now_tick);
        }
        removed
    }

    /// `GuildInfo.cs:680-698` 的 `CancelGuld`：单 rank 单成员且 `OrdinalIgnoreCase` 相等 ⇒ `BackupGuildFile`。
    pub fn cancel_guld(&mut self, name: &str) -> bool {
        if self.ranks.len() != 1 {
            return false;
        }
        let rank = &self.ranks[0];
        if rank.members.len() != 1 {
            return false;
        }
        if cmp_ignore_case(&rank.members[0], name) == Ordering::Equal {
            self.backup_guild_file();
            return true;
        }
        false
    }

    /// `GuildInfo.cs:534-550` 的 `DelHumanObj`（**照搬 `Compare(...)==1`**）：只命中「大于」。
    pub fn del_human_obj(&mut self, name: &str, now_tick: i32) -> bool {
        self.check_save_guild_file(now_tick);
        for rank in &mut self.ranks {
            for j in 0..rank.members.len() {
                if cmp_ignore_case(&rank.members[j], name) == Ordering::Greater {
                    rank.members[j] = String::new();
                    return true;
                }
            }
        }
        false
    }

    /// `GuildInfo.cs:525-533` 的 `CheckSaveGuildFile`：命中返回 `true`（调用方落盘）。
    pub fn check_save_guild_file(&mut self, now_tick: i32) -> bool {
        if self.bo_changed && now_tick.wrapping_sub(self.dw_save_tick) > GUILD_SAVE_THROTTLE_MS {
            self.bo_changed = false;
            return true;
        }
        false
    }

    /// `GuildInfo.cs:584-589` 的 `UpdateGuildFile`：`BoChanged := true`、`DwSaveTick := now`，随后立即落盘。
    pub fn update_guild_file(&mut self, now_tick: i32) {
        self.bo_changed = true;
        self.dw_save_tick = now_tick;
    }

    /// `GuildInfo.cs:591-621` 的 `BackupGuildFile`：清空 members/ranks/notices/wars/allies 后落盘。
    ///
    /// oracle 的 `.bak` 文件名（`GuildName + '.' + GetTickCount() + ".bak"`）由 第②片 处理。
    pub fn backup_guild_file(&mut self) {
        for rank in &mut self.ranks {
            rank.members.clear();
        }
        self.ranks.clear();
        self.notices.clear();
        self.wars.clear();
        self.allies.clear();
        self.update_guild_file(0);
    }

    /// `GuildInfo.cs:129-135` 的 `DelAllyGuild`（`GuildAllList.RemoveAt` + `SaveGuildInfoFile`）。
    pub fn del_ally_guild(&mut self, name: &str) -> bool {
        let mut result = false;
        for i in 0..self.allies.len() {
            if self.allies[i] == name {
                self.allies.remove(i);
                result = true;
                break;
            }
        }
        self.bo_changed = true;
        result
    }

    /// `GuildInfo.cs:146-149` 的 `IsAllyGuild`。
    #[must_use]
    pub fn is_ally_guild(&self, name: &str) -> bool {
        self.allies.iter().any(|a| a == name)
    }

    /// `GuildInfo.cs:181-190` 的 `IsWarGuild`。
    #[must_use]
    pub fn is_war_guild(&self, name: &str) -> bool {
        self.wars.iter().any(|w| w.guild_name == name)
    }

    /// `GuildInfo.cs:995-1005` 的 `IsNotWarGuild`。
    #[must_use]
    pub fn is_not_war_guild(&self, name: &str) -> bool {
        !self.is_war_guild(name)
    }

    /// `GuildInfo.cs:1007-1019` 的 `AllyGuild`：oracle 的 `GuildAllList.Add` 被注释 ⇒ 只落盘、不登记。
    pub fn ally_guild(&mut self, _name: &str) {
        self.bo_changed = true;
    }

    /// `GuildInfo.cs:1020-1056` 的 `AddWarGuild`（**N2 反逻辑照搬**，见模块头 1）。
    pub fn add_war_guild(
        &mut self,
        guild_name: &str,
        cfg: &GuildConfig,
        now_tick: i32,
    ) -> AddWarGuildOutcome {
        let mut result = WarGuild::default();
        let mut war_guild = WarGuild::default();
        let mut messages = Vec::new();
        if !self.is_ally_guild(guild_name) {
            for i in 0..self.wars.len() {
                if self.wars[i].guild_name == guild_name {
                    war_guild = self.wars[i].clone();
                    war_guild.war_tick = now_tick;
                    war_guild.war_time = cfg.guild_war_time;
                    messages.push(format!("***{guild_name}行会战争将持续三个小时。"));
                    break;
                }
            }
            if war_guild.war_time > 0 {
                war_guild = WarGuild {
                    guild_name: guild_name.to_string(),
                    war_tick: now_tick,
                    war_time: cfg.guild_war_time,
                };
                self.wars.push(war_guild.clone());
                messages.push(format!("***{guild_name}行会战争开始(三个小时)"));
            }
            result = war_guild;
        }
        self.update_guild_file(now_tick);
        AddWarGuildOutcome { result, messages }
    }

    /// `GuildInfo.cs:1058-1061` 的 `EndGuildWar` 文案。
    #[must_use]
    pub fn end_guild_war(guild_name: &str) -> String {
        format!("***{guild_name}行会战争结束")
    }

    /// `PlayObject.Operate.cs:1283-1306` 的成员列表文本（`SM_SENDGUILDMEMBERLIST` 的 payload）。
    #[must_use]
    pub fn member_list_text(&self) -> String {
        let mut out = String::new();
        for rank in &self.ranks {
            out.push('#');
            out.push_str(&rank.rank_no.to_string());
            out.push_str("/*");
            out.push_str(&rank.rank_name);
            out.push('/');
            for m in &rank.members {
                if out.chars().count() > 5000 {
                    break;
                }
                out.push_str(m);
                out.push('/');
            }
        }
        out
    }

    /// `GuildInfo.cs:708-993` 的 `UpdateRank`（返回码 `-1..-7`；**N3 成功即清空**）。
    ///
    /// `online(name)` = oracle 的 `SystemShare.WorldEngine.GetPlayObject(name) != null`。
    pub fn update_rank(
        &mut self,
        rank_data: &str,
        _cfg: &GuildConfig,
        now_tick: i32,
        online: impl Fn(&str) -> bool,
    ) -> i32 {
        let mut guild_rank_list: Vec<GuildRank> = Vec::new();
        let mut guild_rank: Option<GuildRank> = None;
        let mut remaining = rank_data.to_string();
        loop {
            if remaining.is_empty() {
                break;
            }
            let (mut rank_info, rest) = get_valid_str3(&remaining, &['\r']);
            remaining = rest;
            rank_info = rank_info.trim().to_string();
            if rank_info.is_empty() {
                continue;
            }
            if rank_info.starts_with('#') {
                let body = rank_info.get(1..).unwrap_or("").to_string();
                let (rank_no_s, rest2) = get_valid_str3(&body, &[' ', '<']);
                let (rank_name_s, _rest3) = get_valid_str3(&rest2, &['<', '>']);
                let mut rank_name = rank_name_s;
                if rank_name.chars().count() > GUILD_RANK_NAME_MAX {
                    rank_name = rank_name.chars().take(GUILD_RANK_NAME_MAX).collect();
                }
                if let Some(gr) = guild_rank.take() {
                    guild_rank_list.push(gr);
                }
                guild_rank = Some(GuildRank {
                    rank_no: str_to_int16(&rank_no_s, GUILD_RANK_NO_MEMBER),
                    rank_name: rank_name.trim().to_string(),
                    members: Vec::new(),
                });
                continue;
            }
            if guild_rank.is_none() {
                continue;
            }
            let mut count = 0usize;
            let mut rem = rank_info;
            loop {
                if rem.is_empty() {
                    break;
                }
                let (member_name, next) = get_valid_str3(&rem, &GUILD_SPLIT);
                rem = next;
                if !member_name.is_empty() {
                    let _ = online(&member_name);
                    if let Some(gr) = guild_rank.as_mut() {
                        gr.members.push(member_name);
                    }
                }
                count += 1;
                if count >= GUILD_RANK_MEMBERS_PER_LINE {
                    break;
                }
            }
        }
        if let Some(gr) = guild_rank.take() {
            guild_rank_list.push(gr);
        }

        // 未变化 ⇒ -1。
        if self.ranks.len() == guild_rank_list.len() {
            let mut unchanged = true;
            for (cur, new) in self.ranks.iter().zip(guild_rank_list.iter()) {
                if cur.rank_no == new.rank_no
                    && cur.rank_name == new.rank_name
                    && cur.members.len() == new.members.len()
                {
                    if cur
                        .members
                        .iter()
                        .zip(new.members.iter())
                        .any(|(a, b)| a != b)
                    {
                        unchanged = false;
                        break;
                    }
                } else {
                    unchanged = false;
                    break;
                }
            }
            if unchanged {
                return -1;
            }
        }

        // 掌门封号检查。
        let mut result = -2;
        if let Some(gr) = guild_rank_list.first() {
            if gr.rank_no == GUILD_RANK_NO_CHIEF {
                if !gr.rank_name.is_empty() {
                    result = 0;
                } else {
                    result = -3;
                }
            }
        }

        // 掌门人在线检查。
        if result == 0 {
            let gr = &guild_rank_list[0];
            if gr.members.len() <= 2 {
                let mut n28 = gr.members.len() as i32;
                for m in &gr.members {
                    if !online(m) {
                        n28 -= 1;
                        break;
                    }
                }
                if n28 <= 0 {
                    result = -5;
                }
            } else {
                result = -4;
            }
        }

        // 老成员是否都在新表里。
        if result == 0 {
            let mut n2c = 0i32;
            let mut n30 = 0i32;
            let mut ok = true;
            'outer_old: for rank in &self.ranks {
                for m in &rank.members {
                    n2c += 1;
                    let found = guild_rank_list
                        .iter()
                        .any(|r| r.members.iter().any(|x| x == m));
                    if !found {
                        result = -6;
                        ok = false;
                        break 'outer_old;
                    }
                }
            }
            if ok {
                for rank in &guild_rank_list {
                    for m in &rank.members {
                        n30 += 1;
                        let found = guild_rank_list
                            .iter()
                            .any(|r| r.members.iter().any(|x| x == m));
                        if !found {
                            result = -6;
                            ok = false;
                            break;
                        }
                    }
                    if !ok {
                        break;
                    }
                }
            }
            if result == 0 && n2c != n30 {
                result = -6;
            }
        }

        // 封号号重复/非法。
        if result == 0 {
            'outer_no: for (i, gr) in guild_rank_list.iter().enumerate() {
                let n28 = gr.rank_no;
                for other in guild_rank_list.iter().skip(i + 1) {
                    if other.rank_no == n28 || n28 <= 0 || n28 > 99 {
                        result = -7;
                        break 'outer_no;
                    }
                }
            }
        }

        if result == 0 {
            // N3：`RankList = rankListTmp`（空表）——guildRankList 被丢弃。
            self.ranks = Vec::new();
            self.update_guild_file(now_tick);
        }
        result
    }
}

/// `M2Share.cs:529-545` 的 `CheckGuildName`（**N4：循环上界 `Length-1`**）。
#[must_use]
pub fn check_guild_name(name: &str, guild_name_len: u8) -> bool {
    let cs: Vec<char> = name.chars().collect();
    if cs.len() > usize::from(guild_name_len) {
        return false;
    }
    let mut result = true;
    for &c in cs.iter().take(cs.len().saturating_sub(1)) {
        if c < '0'
            || matches!(
                c,
                '/' | '\\' | ':' | '*' | ' ' | '"' | '\'' | '<' | '|' | '?' | '>'
            )
        {
            result = false;
        }
    }
    result
}

/// `GuildManager`（`GuildManager.cs`）—— 会表 + 加载/落盘/`Run`。落盘 IO 由 第②片 接 `GuildStore`。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildManager {
    /// `GuildList`。
    pub guilds: Vec<GuildState>,
}

impl GuildManager {
    /// 空会表。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `GuildManager.cs:19-32` 的 `AddGuild`：`CheckGuildName && FindGuild == null` ⇒ 建会 + `SetGuildInfo`。
    pub fn add_guild(&mut self, name: &str, chief: &str, cfg: &GuildConfig, now_tick: i32) -> bool {
        if check_guild_name(name, cfg.guild_name_len) && self.find_guild(name).is_none() {
            let mut guild = GuildState::new(name);
            guild.set_guild_info(chief, cfg);
            guild.update_guild_file(now_tick);
            self.guilds.push(guild);
            return true;
        }
        false
    }

    /// `GuildManager.cs:34-52` 的 `DelGuild`：`OrdinalIgnoreCase` 命中；`RankList.Count > 1` ⇒ `break`（拒删）。
    pub fn del_guild(&mut self, name: &str) -> bool {
        for i in 0..self.guilds.len() {
            if cmp_ignore_case(&self.guilds[i].name, name) == Ordering::Equal {
                if self.guilds[i].ranks.len() > 1 {
                    break;
                }
                self.guilds[i].backup_guild_file();
                self.guilds.remove(i);
                return true;
            }
        }
        false
    }

    /// `GuildManager.cs:63-72` 的 `FindGuild`（**大小写敏感** `==`）。
    #[must_use]
    pub fn find_guild(&self, name: &str) -> Option<&GuildState> {
        self.guilds.iter().find(|g| g.name == name)
    }

    /// 可变版 `FindGuild`。
    pub fn find_guild_mut(&mut self, name: &str) -> Option<&mut GuildState> {
        self.guilds.iter_mut().find(|g| g.name == name)
    }

    /// `GuildManager.cs:110-120` 的 `MemberOfGuild`：首个 `IsMember` 命中。
    #[must_use]
    pub fn member_of_guild(&self, name: &str) -> Option<&str> {
        self.guilds
            .iter()
            .find(|g| g.is_member(name))
            .map(|g| g.name.as_str())
    }

    /// `GuildManager.cs:74-108` 的 `LoadGuildInfo`：已由 ⑧-3 解析出的存活记录物化进会表。
    pub fn load_guild_info(&mut self, records: &[GuildRecord]) {
        self.guilds = records.iter().map(GuildState::from_record).collect();
    }

    /// `GuildManager.cs:145-167` 的 `Run`：逐会倒序扫 `GuildWarList`，到期 `EndGuildWar` + 移除 + 落盘。
    pub fn run(&mut self, now_tick: i32) -> Vec<GuildWarEnded> {
        let mut ended = Vec::new();
        for guild in &mut self.guilds {
            let mut bo_changed = false;
            let mut j = guild.wars.len();
            while j > 0 {
                j -= 1;
                let war = &guild.wars[j];
                if now_tick.wrapping_sub(war.war_tick) > war.war_time {
                    ended.push(GuildWarEnded {
                        guild: guild.name.clone(),
                        enemy: war.guild_name.clone(),
                        message: GuildState::end_guild_war(&war.guild_name),
                    });
                    guild.wars.remove(j);
                    bo_changed = true;
                }
            }
            if bo_changed {
                guild.update_guild_file(now_tick);
            }
            guild.check_save_guild_file(now_tick);
        }
        ended
    }
}

/// `string.Compare(a, b, StringComparison.OrdinalIgnoreCase)` 的 `Ordering` 口径。
///
/// oracle 逐 UTF-16 码元比（先 `ToUpper`）；本仓会名/成员名为 BMP，`String` 的字节序与
/// UTF-16 码元序一致，故用 [`to_upper`] 后比较。
fn cmp_ignore_case(a: &str, b: &str) -> Ordering {
    to_upper(a).cmp(&to_upper(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_data::guild::{
        GuildConfValues, GuildFileBody, GuildRankRecord, GuildRecord, WarGuildRecord,
    };

    fn cfg() -> GuildConfig {
        GuildConfig::default()
    }

    fn record(name: &str, ranks: Vec<GuildRankRecord>, wars: Vec<WarGuildRecord>) -> GuildRecord {
        GuildRecord {
            name: name.to_string(),
            conf: GuildConfValues::default(),
            body: GuildFileBody {
                notices: vec!["n1".into()],
                wars,
                allies: Vec::new(),
                ranks,
            },
        }
    }

    // ---- check_guild_name（N4） ----

    #[test]
    fn check_guild_name_rejects_over_length() {
        assert!(check_guild_name("123456789012345", 15)); // 15 字符 = 上限内
        assert!(!check_guild_name("1234567890123456", 15)); // 16 > 15
    }

    #[test]
    fn check_guild_name_rejects_forbidden_chars_except_last() {
        // 首字符非法 ⇒ false。
        assert!(!check_guild_name(" 会名", 15));
        assert!(!check_guild_name("/会名", 15));
        assert!(!check_guild_name("<>会名", 15));
        // N4：末字符非法但循环不比它 ⇒ true（照搬）。
        assert!(check_guild_name("会名>", 15));
        assert!(check_guild_name("会名 ", 15));
        // 单字符名：循环上界 0 ⇒ 恒 true（照搬）。
        assert!(check_guild_name(">", 15));
    }

    // ---- IsMember / GetRankName / GetChiefName ----

    #[test]
    fn is_member_is_case_sensitive_but_rank_name_is_not() {
        let mut g = GuildState::new("会");
        g.ranks.push(GuildRank {
            rank_no: 1,
            rank_name: "掌门人".into(),
            members: vec!["Alice".into()],
        });
        assert!(g.is_member("Alice"));
        assert!(!g.is_member("alice"), "IsMember 用 ==（大小写敏感）");
        assert_eq!(g.get_rank_name("alice"), Some((1, "掌门人".to_string())));
        assert_eq!(g.get_rank_name("ALICE"), Some((1, "掌门人".to_string())));
        assert_eq!(g.get_rank_name("Bob"), None);
    }

    #[test]
    fn chief_name_and_count() {
        let g = GuildState::new("会");
        assert_eq!(g.get_chief_name(), "");
        assert_eq!(g.count(), 0);
        let g2 = GuildState::from_record(&record(
            "会",
            vec![
                GuildRankRecord {
                    rank_no: 1,
                    rank_name: "掌门人".into(),
                    members: vec!["A".into()],
                },
                GuildRankRecord {
                    rank_no: 99,
                    rank_name: "行会成员".into(),
                    members: vec!["B".into(), "C".into()],
                },
            ],
            Vec::new(),
        ));
        assert_eq!(g2.get_chief_name(), "A");
        assert_eq!(g2.count(), 3);
        assert!(!g2.is_full(&GuildConfig {
            guild_member_max_limit: 4,
            ..cfg()
        }));
        assert!(g2.is_full(&GuildConfig {
            guild_member_max_limit: 3,
            ..cfg()
        }));
    }

    // ---- SetGuildInfo / AddMember / DelMember / CancelGuld ----

    #[test]
    fn set_guild_info_only_when_empty() {
        let mut g = GuildState::new("会");
        g.set_guild_info("A", &cfg());
        assert_eq!(g.ranks.len(), 1);
        assert_eq!(g.ranks[0].rank_no, 1);
        assert_eq!(g.ranks[0].rank_name, "掌门人");
        assert_eq!(g.ranks[0].members, vec!["A".to_string()]);
        g.set_guild_info("B", &cfg());
        assert_eq!(g.ranks[0].members, vec!["A".to_string()], "非空不改");
    }

    #[test]
    fn add_member_creates_rank_99() {
        let mut g = GuildState::new("会");
        g.set_guild_info("A", &cfg());
        g.add_member("B", &cfg(), 100);
        assert_eq!(g.ranks.len(), 2);
        assert_eq!(g.ranks[1].rank_no, 99);
        assert_eq!(g.ranks[1].rank_name, "行会成员");
        assert_eq!(g.ranks[1].members, vec!["B".to_string()]);
        assert!(g.bo_changed, "AddMember 调 UpdateGuildFile");
        assert_eq!(g.dw_save_tick, 100);
        g.add_member("C", &cfg(), 200);
        assert_eq!(g.ranks.len(), 2, "复用已有 99 封号");
        assert_eq!(g.ranks[1].members, vec!["B".to_string(), "C".to_string()]);
    }

    #[test]
    fn del_member_first_exact_match_only() {
        let mut g = GuildState::new("会");
        g.ranks.push(GuildRank {
            rank_no: 1,
            rank_name: "掌门人".into(),
            members: vec!["A".into()],
        });
        g.ranks.push(GuildRank {
            rank_no: 99,
            rank_name: "行会成员".into(),
            members: vec!["B".into(), "B".into()],
        });
        assert!(g.del_member("B", 10));
        assert_eq!(g.ranks[1].members, vec!["B".to_string()], "只删首个命中");
        assert!(!g.del_member("b", 20), "大小写敏感 ⇒ 不命中");
        assert!(g.del_member("A", 30));
        assert!(g.ranks[0].members.is_empty());
    }

    #[test]
    fn cancel_guld_requires_single_rank_single_member() {
        let mut g = GuildState::new("会");
        g.set_guild_info("A", &cfg());
        assert!(!g.cancel_guld("B"));
        assert!(g.cancel_guld("a"), "OrdinalIgnoreCase");
        assert!(g.ranks.is_empty(), "BackupGuildFile 清空");
        assert!(g.notices.is_empty());
    }

    #[test]
    fn del_human_obj_compare_equals_one_quirk() {
        let mut g = GuildState::new("会");
        g.ranks.push(GuildRank {
            rank_no: 1,
            rank_name: "掌门人".into(),
            members: vec!["b".into()],
        });
        // 等值 ⇒ Compare == 0 ⇒ 不动（照搬 oracle 附注）。
        assert!(!g.del_human_obj("b", 0));
        assert_eq!(g.ranks[0].members, vec!["b".to_string()]);
        // 小于目标 ⇒ Compare == -1 ⇒ 不动。
        assert!(!g.del_human_obj("c", 0));
        assert_eq!(g.ranks[0].members, vec!["b".to_string()]);
        // 大于目标 ⇒ Compare == 1 ⇒ 命中，置默认（空名占位）。
        assert!(g.del_human_obj("a", 0));
        assert_eq!(g.ranks[0].members, vec![String::new()]);
    }

    // ---- AddWarGuild（N2） ----

    #[test]
    fn add_war_guild_n2_not_in_war_is_noop() {
        let mut g = GuildState::new("会");
        let out = g.add_war_guild("敌", &cfg(), 500);
        assert!(g.wars.is_empty(), "N2：未在战 ⇒ 不生效");
        assert_eq!(out.result.war_tick, 0);
        assert_eq!(out.result.war_time, 0);
        assert!(out.messages.is_empty(), "未命中循环 ⇒ 不发消息");
        assert!(g.bo_changed);
    }

    #[test]
    fn add_war_guild_n2_already_at_war_duplicates() {
        let mut g = GuildState::new("会");
        g.wars.push(WarGuild {
            guild_name: "敌".into(),
            war_tick: 1,
            war_time: 100,
        });
        let out = g.add_war_guild("敌", &cfg(), 500);
        assert_eq!(g.wars.len(), 2, "N2：已在战 ⇒ 追加重复");
        assert_eq!(out.result.war_tick, 500);
        assert_eq!(out.result.war_time, 10_800_000);
        assert_eq!(out.messages.len(), 2);
        assert_eq!(out.messages[1], "***敌行会战争开始(三个小时)");
    }

    #[test]
    fn ally_guild_is_not_registered() {
        let mut g = GuildState::new("会");
        g.ally_guild("友");
        assert!(!g.is_ally_guild("友"), "GuildAllList.Add 被注释 ⇒ 联盟恒空");
        // DelAllyGuild 对不存在的项返回 false。
        assert!(!g.del_ally_guild("友"));
    }

    // ---- UpdateRank ----

    fn two_rank_guild() -> GuildState {
        let mut g = GuildState::new("会");
        g.ranks.push(GuildRank {
            rank_no: 1,
            rank_name: "掌门人".into(),
            members: vec!["A".into()],
        });
        g.ranks.push(GuildRank {
            rank_no: 99,
            rank_name: "行会成员".into(),
            members: vec!["B".into()],
        });
        g
    }

    #[test]
    fn update_rank_unchanged_returns_minus_one() {
        let mut g = two_rank_guild();
        let data = "#1 掌门人\rA\r#99 行会成员\rB\r";
        assert_eq!(g.update_rank(data, &cfg(), 0, |_| true), -1);
        assert_eq!(g.ranks.len(), 2, "未变化不改");
    }

    #[test]
    fn update_rank_success_clears_ranks_n3() {
        let mut g = two_rank_guild();
        let data = "#1 掌门人甲\rA\r#99 行会成员\rB\r";
        assert_eq!(g.update_rank(data, &cfg(), 7, |_| true), 0);
        assert!(g.ranks.is_empty(), "N3：成功路径 RankList 置空");
        assert!(g.bo_changed);
        assert_eq!(g.dw_save_tick, 7);
    }

    #[test]
    fn update_rank_no_chief_returns_minus_two_or_three() {
        let mut g = two_rank_guild();
        assert_eq!(g.update_rank("#99 行会成员\rB\r", &cfg(), 0, |_| true), -2);
        assert_eq!(g.update_rank("#1 \rA\r", &cfg(), 0, |_| true), -3);
    }

    #[test]
    fn update_rank_chief_member_count_gate() {
        let mut g = two_rank_guild();
        // 3 名掌门成员 ⇒ -4（Count > 2）。
        let data = "#1 掌门人\rA A2 A3\r";
        assert_eq!(g.update_rank(data, &cfg(), 0, |_| true), -4);
        // 1 名掌门成员且离线 ⇒ n28 = 1-1 = 0 ⇒ -5。
        assert_eq!(g.update_rank("#1 掌门人\rA\r", &cfg(), 0, |_| false), -5);
    }

    #[test]
    fn update_rank_missing_old_member_returns_minus_six() {
        let mut g = two_rank_guild();
        // 新表少了 B ⇒ -6。
        let data = "#1 掌门人\rA\r";
        assert_eq!(g.update_rank(data, &cfg(), 0, |_| true), -6);
    }

    #[test]
    fn update_rank_duplicate_rank_no_returns_minus_seven() {
        let mut g = two_rank_guild();
        let data = "#1 掌门人\rA\r#1 副掌门\rB\r";
        assert_eq!(g.update_rank(data, &cfg(), 0, |_| true), -7);
    }

    #[test]
    fn update_rank_parses_rank_name_with_angle_and_space() {
        let mut g = GuildState::new("会");
        g.ranks.push(GuildRank {
            rank_no: 1,
            rank_name: "旧名".into(),
            members: vec!["A".into()],
        });
        // `#1 <张三` ⇒ RankNo=1、RankName=「张三」（`<` 为分隔符）；成功。
        let data = "#1 <张三\rA\r";
        assert_eq!(g.update_rank(data, &cfg(), 0, |_| true), 0);
        assert!(g.ranks.is_empty());
    }

    // ---- GuildManager ----

    #[test]
    fn manager_add_del_find_and_case_sensitivity() {
        let mut m = GuildManager::new();
        assert!(m.add_guild("会甲", "A", &cfg(), 0));
        assert!(!m.add_guild("会甲", "B", &cfg(), 0), "重名（大小写敏感）");
        assert!(m.add_guild("会丙", "C", &cfg(), 0), "可建第二会");
        assert!(m.find_guild("会甲").is_some());
        assert!(m.find_guild("会甲").unwrap().is_member("A"));
        assert_eq!(m.member_of_guild("A"), Some("会甲"));
        // DelGuild：rankCount > 1 ⇒ 拒删。
        m.find_guild_mut("会甲").unwrap().add_member("B", &cfg(), 1);
        assert!(!m.del_guild("会甲"));
        // 单 rank ⇒ 可删（大小写不敏感）。
        assert!(m.del_guild("会丙"));
        assert!(m.find_guild("会丙").is_none());
    }

    #[test]
    fn manager_run_ends_expired_wars() {
        let mut m = GuildManager::new();
        m.add_guild("会甲", "A", &cfg(), 0);
        m.find_guild_mut("会甲").unwrap().wars.push(WarGuild {
            guild_name: "敌".into(),
            war_tick: 100,
            war_time: 50,
        });
        // now=140 ⇒ 140-100=40 <= 50 ⇒ 未完。
        assert!(m.run(140).is_empty());
        // now=151 ⇒ 51 > 50 ⇒ 结束。
        let ended = m.run(151);
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].enemy, "敌");
        assert_eq!(ended[0].message, "***敌行会战争结束");
        assert!(m.find_guild("会甲").unwrap().wars.is_empty());
    }

    // ---- 加载/落盘 往返 ----

    #[test]
    fn from_record_to_body_roundtrip() {
        let rec = record(
            "会",
            vec![GuildRankRecord {
                rank_no: 1,
                rank_name: "掌门人".into(),
                members: vec!["A".into(), "A2".into()],
            }],
            vec![WarGuildRecord {
                guild_name: "敌".into(),
                war_tick: 42,
                war_time: 0,
            }],
        );
        let g = GuildState::from_record(&rec);
        assert_eq!(g.count(), 2);
        assert_eq!(g.wars.len(), 1);
        let back = g.to_body();
        assert_eq!(back, rec.body);
    }
}
