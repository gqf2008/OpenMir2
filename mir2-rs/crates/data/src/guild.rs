//! 行会文件加载/落盘 —— `GuildManager.LoadGuildInfo` / `SaveGuildList` +
//! `GuildInfo.LoadGuild` / `LoadGuildFile` / `SaveGuildInfoFile` / `SaveGuildFile` +
//! `GuildConf`（`<名>.ini`）的读侧移植（M3⑧-实现 3）。
//!
//! 移植源（行号均在本仓库 `src/` 内实测）：
//! - `src/M2Server/Guild/GuildManager.cs:78-110`（`LoadGuildInfo`）、`:124-143`（`SaveGuildList`）
//! - `src/M2Server/Guild/GuildInfo.cs:195-199`（`LoadGuild`）、`:209-346`（`LoadGuildFile`）、
//!   `:366-431`（`SaveGuildInfoFile` / `SaveGuildFile`）
//! - `src/M2Server/Guild/GuildConf.cs:8-17`（ctor + `LoadConfig`）与 `src/OpenMir2/Common/ConfigFile.cs`
//! - `src/OpenMir2/Common/StringList.cs:260-274`（`SaveToFile`）、`:276-291`（`LoadFromFile`）、
//!   `:293-301`（`GetEncoding`）；`src/OpenMir2/HUtil32.cs:391-402`（`GetValidStr3`）
//!
//! 盘点依据：`openmir2-m3-guild-castle-align` 的
//! [`M3⑧公会攻城现状盘点报告-20261013.md`](../../../tests/parity/m3/M3⑧公会攻城现状盘点报告-20261013.md) §1.A。
//!
//! ## 照搬的 oracle 行为（都有真身向量锁死，见 `tests/guild_store_vectors.rs`）
//!
//! 1. **落盘不新建文件**：`StringList.SaveToFile` 先 `GetEncoding(fileName, …)` →
//!    `new FileStream(fileName, FileMode.Open)` ⇒ **目标文件不存在时抛异常**，被
//!    `SaveGuildFile` / `SaveGuildList` 的 `try/catch` 吞掉 ⇒ 新行会的 `<名>.txt` 实际**写不出去**。
//!    本模块用 [`GuildStoreError::MissingSaveTarget`] 表达（**不**代 oracle 修好）。
//! 2. **`<名>.ini` 只会被建成空文件**：`GuildConf` ctor 里 `Load()` 对不存在的文件
//!    `File.Create(FileName).Close()`；而 `SaveGuildConfig` 只写 ini 缓存、**从不 `Save()`**
//!    ⇒ 五键永远不落盘。本模块只提供读侧 + 建空文件副作用，**没有**写盘路径。
//! 3. **行会战 `WarTime` 加载恒为 0**：`LoadGuildFile` 的行会战分支把剩余时长读进**上一行残留**的
//!    `s20`（该段从不赋值，初值 `string.Empty`）⇒ `HUtil32.StrToInt("") == 0`。本模块照搬
//!    「跨行可变 `s20`」。
//! 4. **联盟表恒空**：`case 3` 的 `//GuildAllList.Add(guild)` 与 `Operate.cs:1016` 的同类行都被注释
//!    ⇒ 加载时解析出来的联盟名**被丢弃**（保存时 `GuildAllList` 也恒空）。本模块解析该段但**不落入
//!    `allies`**，并在保存侧保留 `+<名>` 的写法（不可达分支的形态）。
//! 5. **职名截断只在 `s24.Length > 30` 时发生**，且截到 `GuildRankNameLen`（现网默认 16）。
//! 6. 文本一律 **GBK 字节** + `WriteLine` 的 **CRLF**；`.ini` 读取走 `ConfigFile.Load`
//!    （`StreamReader(..., Encoding.GetEncoding("GB2312"))`，构造参数默认 `detectEncodingFromByteOrderMarks: true`）。

use std::path::{Path, PathBuf};

use crate::exps::IniFile;

/// oracle `GameSvrConf` 的四个段头初值（`GameSvrConf.cs:1850-1853`；`ServerConf.LoadConfig` 无读取点）。
pub const ORACLE_GUILD_NOTICE: &str = "公告";
/// 行会战段头（`GameSvrConf.cs:1851`）。
pub const ORACLE_GUILD_WAR: &str = "敌对行会";
/// 联盟段头（`GameSvrConf.cs:1852`）。
pub const ORACLE_GUILD_ALL: &str = "联盟行会";
/// 成员段头（`GameSvrConf.cs:1853`）。
pub const ORACLE_GUILD_MEMBER: &str = "行会成员";
/// 掌门人封号（`GameSvrConf.cs:1855`，`SetGuildInfo` 用）。
pub const ORACLE_GUILD_CHIEF: &str = "掌门人";

/// `LoadGuildFile` 用的分词符：`new char[] { ' ', ',' }`（`GuildInfo.cs:207`）。
pub const GUILD_SPLIT: [char; 2] = [' ', ','];

/// 四个段头文案（oracle `SystemShare.Config.GuildNotice/GuildWar/GuildAll/GuildMember`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildFileSections {
    /// `公告` 段头。
    pub notice: String,
    /// `敌对行会` 段头。
    pub war: String,
    /// `联盟行会` 段头。
    pub all: String,
    /// `行会成员` 段头。
    pub member: String,
}

impl GuildFileSections {
    /// oracle `ServerConf.LoadConfig` 对这四键**无读取点** ⇒ 恒为 `GameSvrConf` 初值。
    #[must_use]
    pub fn oracle_defaults() -> Self {
        Self {
            notice: ORACLE_GUILD_NOTICE.to_string(),
            war: ORACLE_GUILD_WAR.to_string(),
            all: ORACLE_GUILD_ALL.to_string(),
            member: ORACLE_GUILD_MEMBER.to_string(),
        }
    }

    /// 从 ⑧-2 的 [`crate::guild_castle::GuildCastleConfig`] 取段头（同源字段）。
    #[must_use]
    pub fn from_config(cfg: &crate::guild_castle::GuildCastleConfig) -> Self {
        Self {
            notice: cfg.guild_notice.clone(),
            war: cfg.guild_war_header.clone(),
            all: cfg.guild_all.clone(),
            member: cfg.guild_member.clone(),
        }
    }
}

/// `<名>.ini` 的 `[Guild]` 五键读侧（`GuildConf.LoadConfig`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuildConfValues {
    /// `[Guild] BuildPoint`。
    pub build_point: i32,
    /// `[Guild] Aurae`。
    pub aurae: i32,
    /// `[Guild] Stability`。
    pub stability: i32,
    /// `[Guild] Flourishing`。
    pub flourishing: i32,
    /// `[Guild] ChiefItemCount`。
    pub chief_item_count: i32,
}

/// 一个封号（`SystemModule.Data.GuildRank`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildRankRecord {
    /// `RankNo`（`1` = 掌门）。
    pub rank_no: i16,
    /// `RankName`。
    pub rank_name: String,
    /// 成员名（Rust 侧只留名，`PlayObject` 引用由世界层持有）。
    pub members: Vec<String>,
}

/// 一条行会战（`SystemModule.Data.WarGuild`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WarGuildRecord {
    /// 敌对行会名（oracle 里是 `IGuild` 引用）。
    pub guild_name: String,
    /// `WarTick`（oracle 加载时置本刻 `GetTickCount()`）。
    pub war_tick: i32,
    /// `WarTime`（oracle 加载时**恒为 0**，见模块头 ③）。
    pub war_time: i32,
}

/// `<名>.txt` 四段内容（`GuildInfo` 的 `NoticeList` / `GuildWarList` / `GuildAllList` / `RankList`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildFileBody {
    /// 公告行（`NoticeList`）。
    pub notices: Vec<String>,
    /// 敌对行会（`GuildWarList`）。
    pub wars: Vec<WarGuildRecord>,
    /// 联盟（`GuildAllList`；oracle 恒空，见模块头 ④）。
    pub allies: Vec<String>,
    /// 封号 + 成员（`RankList`）。
    pub ranks: Vec<GuildRankRecord>,
}

/// `LoadGuild()` 的结果：`.ini` 五键 + `.txt` 四段。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildRecord {
    /// 行会名（= `<名>.txt` 的文件名前缀）。
    pub name: String,
    /// `<名>.ini` 读侧。
    pub conf: GuildConfValues,
    /// `<名>.txt` 四段。
    pub body: GuildFileBody,
}

/// 保存动作的结果（区分「写了」与「因非 0 区而 no-op」）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    /// 已写盘。
    Written,
    /// `M2Share.ServerIndex != 0` ⇒ `SaveGuildList` 直接 `return`（`GuildManager.cs:126-129`）。
    SkippedNonZeroServerIndex,
}

/// 行会文件层的错误。
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GuildStoreError {
    /// `GuildManager.LoadGuildInfo` 的 `!File.Exists(GuildFile)` 分支（`:81/108`）——oracle 只记错误。
    #[error("行会信息文件未找到: {0}")]
    MissingListFile(PathBuf),
    /// `<名>.txt` 不存在 ⇒ `LoadGuildFile` 返回 `false`（`GuildInfo.cs:215-217`）。
    #[error("行会文件不存在: {0}")]
    MissingGuildFile(PathBuf),
    /// 保存目标不存在 ⇒ oracle `StringList.SaveToFile` 抛异常被吞（见模块头 ①）。
    #[error("保存目标不存在（oracle StringList.SaveToFile 打不开不存在的文件）: {0}")]
    MissingSaveTarget(PathBuf),
    /// 其它 IO 错误。
    #[error("io: {0}")]
    Io(String),
}

/// `LoadGuildInfo` 的结果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildInfoLoad {
    /// 存活会名（`GuildList` 的顺序）。
    pub names: Vec<String>,
    /// 存活行会的加载结果（与 `names` 同序）。
    pub guilds: Vec<GuildRecord>,
    /// 因 `LoadGuild()` 失败被移除的会名（oracle 逆序移除）。
    pub removed: Vec<String>,
}

/// 行会文件仓库：`GuildDir` / `GuildFile`（oracle `SystemShare.Config` 的两个路径项）。
#[derive(Clone, Debug)]
pub struct GuildStore {
    guild_dir: PathBuf,
    guild_file: PathBuf,
}

impl GuildStore {
    /// `guild_dir` = `Config.GuildDir`（`GuildBase\Guilds\`），`guild_file` = `Config.GuildFile`。
    pub fn new(guild_dir: impl Into<PathBuf>, guild_file: impl Into<PathBuf>) -> Self {
        Self {
            guild_dir: guild_dir.into(),
            guild_file: guild_file.into(),
        }
    }

    /// `Config.GuildDir`。
    #[must_use]
    pub fn guild_dir(&self) -> &Path {
        &self.guild_dir
    }

    /// `Config.GuildFile`。
    #[must_use]
    pub fn guild_file(&self) -> &Path {
        &self.guild_file
    }

    /// `<GuildDir>/<名>.txt`（`Path.Combine`）。
    #[must_use]
    pub fn guild_txt_path(&self, name: &str) -> PathBuf {
        self.guild_dir.join(format!("{name}.txt"))
    }

    /// `<GuildDir>/<名>.ini`（`GuildInfo` ctor 的 `Path.Combine(GuildDir, name + ".ini")`）。
    #[must_use]
    pub fn guild_conf_path(&self, name: &str) -> PathBuf {
        self.guild_dir.join(format!("{name}.ini"))
    }

    /// `GuildManager.LoadGuildInfo` 的读名单段（`:81-93`）：文件不存在 ⇒
    /// [`GuildStoreError::MissingListFile`]（oracle 记错误、会表为空）。
    ///
    /// # Errors
    /// 名单文件缺失或读取失败。
    pub fn load_guild_list(&self) -> Result<Vec<String>, GuildStoreError> {
        if !self.guild_file.exists() {
            return Err(GuildStoreError::MissingListFile(self.guild_file.clone()));
        }
        let bytes = std::fs::read(&self.guild_file).map_err(io_err)?;
        Ok(parse_guild_list(&bytes))
    }

    /// `GuildManager.SaveGuildList`（`:124-143`）：非 0 区 no-op；目标文件不存在 ⇒
    /// [`GuildStoreError::MissingSaveTarget`]（oracle 抛错被吞）。
    ///
    /// # Errors
    /// 目标文件缺失或写失败。
    pub fn save_guild_list(
        &self,
        names: &[String],
        server_index: i32,
    ) -> Result<SaveStatus, GuildStoreError> {
        if server_index != 0 {
            return Ok(SaveStatus::SkippedNonZeroServerIndex);
        }
        self.write_existing(&self.guild_file, &serialize_guild_list(names))
    }

    /// `GuildConf` 读侧（`GuildConf.cs:8-17` + `LoadConfig`）：
    /// `.ini` 不存在时按 oracle ctor 的 `Load()` **建一个空文件**（副作用照搬），返回初值 0。
    ///
    /// # Errors
    /// 建文件 / 读文件失败。
    pub fn load_guild_conf(&self, name: &str) -> Result<GuildConfValues, GuildStoreError> {
        let path = self.guild_conf_path(name);
        if !path.exists() {
            std::fs::File::create(&path).map_err(io_err)?;
        }
        let bytes = std::fs::read(&path).map_err(io_err)?;
        Ok(parse_guild_conf(&bytes))
    }

    /// `GuildInfo.LoadGuild`（`:195-199`）：先读 `.ini`，再解析 `<名>.txt`；
    /// `<名>.txt` 不存在 ⇒ [`GuildStoreError::MissingGuildFile`]（`.ini` 副作用已发生）。
    ///
    /// # Errors
    /// `<名>.txt` 缺失或 IO / 读取失败。
    pub fn load_guild(
        &self,
        name: &str,
        sections: &GuildFileSections,
        rank_name_len: u8,
        now_tick: i32,
        guild_exists: impl Fn(&str) -> bool,
    ) -> Result<GuildRecord, GuildStoreError> {
        let conf = self.load_guild_conf(name)?;
        let path = self.guild_txt_path(name);
        if !path.exists() {
            return Err(GuildStoreError::MissingGuildFile(path));
        }
        let bytes = std::fs::read(&path).map_err(io_err)?;
        let body = parse_guild_file(&bytes, sections, rank_name_len, now_tick, guild_exists);
        Ok(GuildRecord {
            name: name.to_string(),
            conf,
            body,
        })
    }

    /// `GuildInfo.SaveGuildInfoFile`（`:366-379`）：0 区写 `<名>.txt`；非 0 区写
    /// `<GuildDir>/<名>/.<ServerIndex>`（oracle 的怪路径，照搬）。
    ///
    /// # Errors
    /// 目标文件缺失（见模块头 ①）或写失败。
    pub fn save_guild(
        &self,
        guild: &GuildRecord,
        sections: &GuildFileSections,
        now_tick: i32,
        server_index: i32,
    ) -> Result<SaveStatus, GuildStoreError> {
        let target = if server_index == 0 {
            self.guild_txt_path(&guild.name)
        } else {
            self.guild_dir
                .join(&guild.name)
                .join(format!(".{server_index}"))
        };
        self.write_existing(
            &target,
            &serialize_guild_file(&guild.body, sections, now_tick),
        )
    }

    /// `GuildManager.LoadGuildInfo`（`:78-110`）：读名单 → 逐个 `new GuildInfo`（建 `.ini`）→
    /// **逆序**加载，失败者移除并回写名单（非 0 区不回写）。
    ///
    /// 逆序加载期间 `guild_exists` 必须查**当前存活集**（不是初始名单快照）：索引 > i 的失败者
    /// 已 `RemoveAt`，故引用了它们的行会战段**不入表**（`GuildManager.cs:67-77` 的 `FindGuild`
    /// 遍历当前 `GuildList`）。
    ///
    /// # Errors
    /// 名单文件缺失，或名单外 IO 错误。
    pub fn load_guild_info(
        &self,
        sections: &GuildFileSections,
        rank_name_len: u8,
        now_tick: i32,
        server_index: i32,
    ) -> Result<GuildInfoLoad, GuildStoreError> {
        let mut names = self.load_guild_list()?;
        // oracle 先把**所有**行会对象放进 GuildList（ctor 建 `.ini`），再逐个 LoadGuild。
        for name in &names {
            self.load_guild_conf(name)?;
        }
        let mut guilds: Vec<GuildRecord> = Vec::new();
        let mut removed: Vec<String> = Vec::new();
        for i in (0..names.len()).rev() {
            let name = names[i].clone();
            // `FindGuild` 遍历**当前** `GuildList`：逆序加载时索引 > i 的失败者已被移除，
            // 索引 <= i 的会尚未处理、也都在表里 ⇒ 存活集恰好是本刻的 `names`
            // （失败者已在上面各轮从 `names` 移除）。用初始快照会让「引用已移除会」的行会战
            // 条目误入表，与 oracle 分叉（reviewer-6 F-1）。
            let load_result = self.load_guild(&name, sections, rank_name_len, now_tick, |n| {
                names.iter().any(|x| x == n)
            });
            match load_result {
                Ok(record) => guilds.push(record),
                Err(GuildStoreError::MissingGuildFile(_)) => {
                    removed.push(name);
                    names.remove(i);
                    self.save_guild_list(&names, server_index)?;
                }
                Err(e) => return Err(e),
            }
        }
        guilds.sort_by_key(|g| {
            names
                .iter()
                .position(|n| n == &g.name)
                .unwrap_or(usize::MAX)
        });
        Ok(GuildInfoLoad {
            names,
            guilds,
            removed,
        })
    }

    fn write_existing(&self, path: &Path, bytes: &[u8]) -> Result<SaveStatus, GuildStoreError> {
        if !path.exists() {
            return Err(GuildStoreError::MissingSaveTarget(path.to_path_buf()));
        }
        std::fs::write(path, bytes).map_err(io_err)?;
        Ok(SaveStatus::Written)
    }
}

fn io_err(e: std::io::Error) -> GuildStoreError {
    GuildStoreError::Io(e.to_string())
}

/// `HUtil32.GetValidStr3(str, ref dest, char[])`（`HUtil32.cs:391-402`）：
/// `str.Split(div, 2, RemoveEmptyEntries)` —— `dest` = 第一个非空 token，返回 = 从第二个非空
/// token 起到**串尾**（含其后的分隔符）的子串；无第二个 token ⇒ 空串。
///
/// 真身向量：`tests/parity/m3/guild-store-probe/vectors/guild_store_vectors.tsv` 的
/// `H_get_valid_str3` 组（含 `,,a,,b,,` ⇒ `b,,` 这类「余串含尾分隔符」的边界）。
#[must_use]
pub fn get_valid_str3(source: &str, dest: &mut String, splits: &[char]) -> String {
    let is_sep = |c: char| splits.contains(&c);
    let head = source.trim_start_matches(is_sep);
    let first_end = head.find(is_sep).unwrap_or(head.len());
    let first = &head[..first_end];
    *dest = first.to_string();
    if first.is_empty() {
        return String::new();
    }
    head[first_end..].trim_start_matches(is_sep).to_string()
}

/// `.NET StreamReader.ReadLine()` 的行切分：`\r\n` / `\n` / `\r` 三种终止符，
/// 串尾无内容时不额外产出一行（`StringList.LoadFromFile` 的 `while (Peek() >= 0)` 口径）。
#[must_use]
pub fn split_lines_readline(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let (mut start, mut i) = (0usize, 0usize);
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(&text[start..i]);
                i += 1;
                if i < bytes.len() && bytes[i] == b'\n' {
                    i += 1;
                }
                start = i;
            }
            b'\n' => {
                out.push(&text[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        out.push(&text[start..]);
    }
    out
}

/// `GuildManager.LoadGuildInfo` 的名单读侧（`:85-93`）：GBK 解码、逐行 `Trim()`、跳过空行。
#[must_use]
pub fn parse_guild_list(bytes: &[u8]) -> Vec<String> {
    let (text, _, _) = encoding_rs::GBK.decode(bytes);
    split_lines_readline(&text)
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// `GuildManager.SaveGuildList` 的写侧（`:130-138` + `StringList.SaveToFile`）：GBK + CRLF。
#[must_use]
pub fn serialize_guild_list(names: &[String]) -> Vec<u8> {
    encode_lines_gbk(names)
}

/// `<名>.ini` 的 `[Guild]` 五键读侧（`GuildConf.LoadConfig`；`ConfigFile.Load` 口径，含 BOM 探测）。
#[must_use]
pub fn parse_guild_conf(bytes: &[u8]) -> GuildConfValues {
    let ini = IniFile::from_bytes(bytes);
    GuildConfValues {
        build_point: ini.read_integer("Guild", "BuildPoint", 0),
        aurae: ini.read_integer("Guild", "Aurae", 0),
        stability: ini.read_integer("Guild", "Stability", 0),
        flourishing: ini.read_integer("Guild", "Flourishing", 0),
        chief_item_count: ini.read_integer("Guild", "ChiefItemCount", 0),
    }
}

/// `GuildInfo.LoadGuildFile`（`:209-346`）的纯解析：四段、`+`/`#`、`;` 行、`s20` 残留（见模块头 ③）
/// 与联盟段丢弃（模块头 ④）。
///
/// `now_tick` = oracle 在本刻的 `HUtil32.GetTickCount()`（加载出的 `WarTick`）；
/// `guild_exists` = oracle 的 `SystemShare.GuildMgr.FindGuild(name) != null`（查**当前存活集**：
/// 逆序加载中已失败的会已从 `GuildList` 移除，故引用它们的行会战条目**不入表**；见 `load_guild_info`）。
#[must_use]
pub fn parse_guild_file(
    bytes: &[u8],
    sections: &GuildFileSections,
    rank_name_len: u8,
    now_tick: i32,
    guild_exists: impl Fn(&str) -> bool,
) -> GuildFileBody {
    let (text, _, _) = encoding_rs::GBK.decode(bytes);
    let mut body = GuildFileBody::default();
    let mut s1c = String::new();
    // oracle 的 `s20` 是**跨行可变**变量：行会战段（case 2）从不给它赋值，联盟段（case 3）才写。
    let mut s20 = String::new();
    let mut section: u8 = 0;
    let mut rank_no: i16 = 0;
    let mut rank_name = String::new();
    let mut current_rank: Option<usize> = None;

    for line in split_lines_readline(&text) {
        let mut s18 = line.to_string();
        if s18.is_empty() || s18.starts_with(';') {
            continue;
        }
        if !s18.starts_with('+') {
            if s18 == sections.notice {
                section = 1;
            }
            if s18 == sections.war {
                section = 2;
            }
            if s18 == sections.all {
                section = 3;
            }
            if s18 == sections.member {
                section = 4;
            }
            if s18.starts_with('#') {
                s18 = s18[1..].to_string();
                s18 = get_valid_str3(&s18, &mut s1c, &GUILD_SPLIT);
                rank_no = str_to_i16(&s1c, 0);
                rank_name = s18.trim().to_string();
                current_rank = None;
            }
            continue;
        }
        s18 = s18[1..].to_string();
        match section {
            1 => body.notices.push(s18),
            2 => {
                while !s18.is_empty() {
                    s18 = get_valid_str3(&s18, &mut s1c, &GUILD_SPLIT);
                    if s1c.is_empty() {
                        break;
                    }
                    if guild_exists(&s1c) {
                        body.wars.push(WarGuildRecord {
                            guild_name: s1c.clone(),
                            war_tick: now_tick,
                            // oracle 读的是残留 `s20`（本段恒空）⇒ 恒 0（模块头 ③）。
                            war_time: str_to_i32(s20.trim(), 0),
                        });
                    }
                }
            }
            3 => {
                while !s18.is_empty() {
                    s18 = get_valid_str3(&s18, &mut s1c, &GUILD_SPLIT);
                    s18 = get_valid_str3(&s18, &mut s20, &GUILD_SPLIT);
                    if s1c.is_empty() {
                        break;
                    }
                    // oracle `//GuildAllList.Add(guild);` 被注释 ⇒ 联盟名被丢弃（模块头 ④）。
                }
            }
            4 => {
                if rank_no <= 0 || rank_name.is_empty() {
                    continue;
                }
                if rank_name.chars().count() > 30 {
                    rank_name = rank_name.chars().take(usize::from(rank_name_len)).collect();
                }
                let idx = match current_rank {
                    Some(idx) => idx,
                    None => {
                        body.ranks.push(GuildRankRecord {
                            rank_no,
                            rank_name: rank_name.clone(),
                            members: Vec::new(),
                        });
                        let idx = body.ranks.len() - 1;
                        current_rank = Some(idx);
                        idx
                    }
                };
                while !s18.is_empty() {
                    s18 = get_valid_str3(&s18, &mut s1c, &GUILD_SPLIT);
                    if s1c.is_empty() {
                        break;
                    }
                    body.ranks[idx].members.push(s1c.clone());
                }
            }
            _ => {}
        }
    }
    body
}

/// `GuildInfo.SaveGuildFile`（`:384-431`）的纯序列化（GBK + CRLF，`StringList.SaveToFile` 口径）。
///
/// `now_tick` = 本刻 `HUtil32.GetTickCount()`（算剩余时长 `n14 = WarTime - (now_tick - WarTick)`；
/// `n14 <= 0` 的条目**跳过**）。
#[must_use]
pub fn serialize_guild_file(
    body: &GuildFileBody,
    sections: &GuildFileSections,
    now_tick: i32,
) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();
    lines.push(sections.notice.clone());
    for notice in &body.notices {
        lines.push(format!("+{notice}"));
    }
    lines.push(" ".to_string());
    lines.push(sections.war.clone());
    for war in &body.wars {
        // oracle：`long n14 = WarTime - (GetTickCount() - WarTick);` —— int 先算完再拓宽到 long。
        let n14 = i64::from(
            war.war_time
                .wrapping_sub(now_tick.wrapping_sub(war.war_tick)),
        );
        if n14 <= 0 {
            continue;
        }
        lines.push(format!("+{} {}", war.guild_name, n14));
    }
    lines.push(" ".to_string());
    lines.push(sections.all.clone());
    for ally in &body.allies {
        lines.push(format!("+{ally}"));
    }
    lines.push(" ".to_string());
    lines.push(sections.member.clone());
    for rank in &body.ranks {
        lines.push(format!("#{} {}", rank.rank_no, rank.rank_name));
        for member in &rank.members {
            lines.push(format!("+{member}"));
        }
    }
    encode_lines_gbk(&lines)
}

fn encode_lines_gbk(lines: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in lines {
        let (bytes, _, _) = encoding_rs::GBK.encode(line);
        out.extend_from_slice(&bytes);
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// `HUtil32.StrToInt`（`HUtil32.cs:359-362`）：`int.TryParse` 成功取值，否则默认值。
#[must_use]
pub fn str_to_i32(s: &str, def: i32) -> i32 {
    s.trim().parse::<i32>().unwrap_or(def)
}

/// `HUtil32.StrToInt16`（`HUtil32.cs:369-372`）：`short.TryParse` 成功取值，否则默认值。
#[must_use]
pub fn str_to_i16(s: &str, def: i16) -> i16 {
    s.trim().parse::<i16>().unwrap_or(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gbk(s: &str) -> Vec<u8> {
        let (bytes, _, _) = encoding_rs::GBK.encode(s);
        bytes.into_owned()
    }

    #[test]
    fn readline_split_matches_dotnet() {
        assert_eq!(split_lines_readline("a\r\nb"), vec!["a", "b"]);
        assert_eq!(split_lines_readline("a\r\n"), vec!["a"]);
        assert_eq!(split_lines_readline("a\n\n"), vec!["a", ""]);
        assert_eq!(split_lines_readline("a\rb"), vec!["a", "b"]);
        assert_eq!(split_lines_readline(""), Vec::<&str>::new());
        assert_eq!(split_lines_readline("\n"), vec![""]);
    }

    #[test]
    fn get_valid_str3_keeps_trailing_separators_in_rest() {
        let mut dest = String::new();
        assert_eq!(get_valid_str3(",,a,,b,,", &mut dest, &GUILD_SPLIT), "b,,");
        assert_eq!(dest, "a");
        let mut dest = String::new();
        assert_eq!(get_valid_str3("  a  b", &mut dest, &GUILD_SPLIT), "b");
        assert_eq!(dest, "a");
        let mut dest = String::new();
        assert_eq!(get_valid_str3("  ", &mut dest, &GUILD_SPLIT), "");
        assert_eq!(dest, "");
    }

    #[test]
    fn war_time_is_always_zero_on_load() {
        let body = parse_guild_file(
            &gbk("敌对行会\n+乙会 123456\n"),
            &GuildFileSections::oracle_defaults(),
            16,
            777,
            |name| name == "乙会",
        );
        assert_eq!(body.wars.len(), 1);
        assert_eq!(body.wars[0].war_time, 0, "oracle 读残留 s20 ⇒ 恒 0");
        assert_eq!(body.wars[0].war_tick, 777);
    }

    #[test]
    fn allies_section_is_discarded() {
        let body = parse_guild_file(
            &gbk("联盟行会\n+丙会\n"),
            &GuildFileSections::oracle_defaults(),
            16,
            0,
            |_| true,
        );
        assert!(body.allies.is_empty(), "oracle 的 Add 被注释 ⇒ 联盟恒空");
    }
}
