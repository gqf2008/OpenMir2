//! `M2GameSvr/Server.conf` 的只读视图 + 白名单写。
//!
//! 读侧口径逐条对应 oracle `src/OpenMir2/Common/ConfigFile.cs` 的 `Load`
//! （`src/Modules/SystemModule/Conf/ServerConf.cs` 的键来自它）：
//!
//! * 段名/键名 **`OrdinalIgnoreCase`**（C# 内层 `Dictionary` 也传了 `StringComparer.OrdinalIgnoreCase`）；
//! * 整行 `Trim`；`;` 开头整行注释；`/* … */` 段注释；行内 `;;` 之后截断；
//! * 键按**第一个** `=` 切分，键值各自 `Trim`；
//! * 同段同名键**首个生效**；**重复段名 ⇒ 整个加载中断**（C# `goto Label_02AE`）。
//!
//! 写侧刻意**不走** oracle 的 `Save()`：`ConfigFile.Save` 会按内存字典整文件重写（丢注释/重排），
//! 那是内容破坏。本工具只在原始字节上替换**目标行那一个值区间**，其余字节逐字节不变；
//! 写前备份、写时临时文件 + rename 原子替换、写后回读校验（见 [`conf_set`]）。
//!
//! **写面固定为 `<root>/M2GameSvr/Server.conf`**：工具没有 path 参数，因此不存在跨文件写入面。
//!
//! # 跨进程互斥（`conf_set` 的「读 → 备份 → rename → 回读 → 回滚」整段）
//!
//! HTTP 传输下每个 MCP 会话各建一个 `Mir2Mcp`，两个已鉴权客户端可以并发 `conf_set` 同一个
//! `Server.conf`。没有跨进程互斥时，两侧的「备份 + rename + 回读校验」会交错：A 的 rename 之后
//! B 又 rename，A 回读到 B 的值 ⇒ A 用自己**陈旧**的备份回滚，把 B 的成功写入覆盖掉（互滚覆盖）。
//! 同进程 `Mutex` 不够（HTTP 每会话一个实例），所以用**同目录锁文件**：
//!
//! * 锁文件 = 目标同目录的 `Server.conf.lock`，用 `OpenOptions::create_new(true)` 原子创建
//!   （Windows delete-pending 窗口里的 `ERROR_ACCESS_DENIED` 按瞬时竞态少量重试）；
//!   抢不到就**有界重试**（[`LOCK_RETRY_INTERVAL_MS`]），超过 `MIR2_MCP_CONF_LOCK_TIMEOUT_MS`
//!   （缺省 [`DEFAULT_LOCK_TIMEOUT_MS`]）返回新错误码 [`ConfError::LockConflict`] =
//!   `conf_lock_timeout`（不 panic、不改文件）。
//! * 锁覆盖 `conf_set` 的**无锁预检之后的整段临界区**：锁内重读 → 备份 → 临时文件 + rename →
//!   回读校验 → 必要时回滚。校验失败/IO 失败同样在锁内回滚后才释放。
//! * 锁文件内容 = `pid=<持有者 pid>` / `acq_ms=<获取时刻 unix 毫秒>` /
//!   `start=<持有者进程启动时刻 token>`（探测不到时省略）/ `nonce=<唯一串>`；
//!   正常结束由 [`ConfLock`] 的 `Drop` 删除（**只删 pid + acq_ms + start + nonce 逐字段
//!   等于自己那份**；不做子串匹配，避免误删内容里恰好含相同 nonce 片段的他锁）。
//! * **残留锁接管规则**（`try_take_over_stale`）三条同时成立才接管：
//!   1. 锁文件年龄 ≥ `MIR2_MCP_CONF_LOCK_STALE_MS`（缺省 [`DEFAULT_LOCK_STALE_MS`]；
//!      年龄取 `acq_ms`，缺失时退化为文件 mtime）；
//!   2. 持有者**确证已死**：pid 不存在，**或** pid 被复用（探测到的进程启动时刻与锁里记录的
//!      `start` 不相等）。判定采用「pid + start_time」双元组（Windows 用 `tasklist` 判存活 +
//!      `wmic`/PowerShell `Win32_Process.CreationDate` 取启动时刻；Unix 用
//!      `/proc/<pid>/stat` 的 starttime，无 `/proc` 时退化为 `ps -o lstart=`）。
//!      **探测不可用（None/Unknown）一律保守视为「活」⇒ 不接管**：宁可让一个超龄锁继续挡住
//!      写者直到超时，也不把不确定的锁抢走。缺 `start` 记录时同理（无法区分原持有者与复用
//!      pid 的新进程）。
//!   3. 接管用**同目录 rename 抢占**（原子，只有一个进程能成功），并把抢到的文件内容与
//!      判定时读到的字节逐字节比对；不一致说明撞上「刚释放/刚被他人接管」的竞态，放回去重试；
//!      放不回（原位置已被新锁占用）时按搬走文件**自己的身份**重新判定：确证已死才删，
//!      否则留作 `.<name>.stale.*` 交给后续清理（宁留残片也不误删活锁）。
//!      ⇒ **活进程的锁永不被抢**（规则 2），被强杀的进程留下的锁在超过时间阈值后可自愈。
//! * **崩溃残留清理**（`cleanup_residue`，每次成功取锁后顺手执行）：识别同目录的
//!   `.<name>.mir2-mcp.<pid>.<start>.<stamp>.<counter>.tmp` 与
//!   `.<name>.stale.<pid>.<start>.<stamp>.<counter>`，只删「确证持有者已死 + 年龄 ≥ stale
//!   阈值」的；存活进程持有（启动时刻一致）、探测不可用、身份不可判定的一律保留。
//! * `dry_run=true` **不取锁**：它是零写的纯预览（不备份、不落盘），取排他锁会让只读预演
//!   被写者阻塞、甚至在并发时返回新的锁错误码，从而改变既有 dry_run 行为；并发下它最多读到
//!   某一时刻的完整快照（rename 原子替换），不产生持久化副作用。
//! * 测试专用接缝：`MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS` 在 rename 与回读之间睡眠，
//!   只用于放大并发窗口复现互滚；`MIR2_MCP_CONF_TEST_HOLD_BEFORE_TAKEOVER_RENAME_MS` 在
//!   接管「读锁之后、rename 之前」睡眠，用于稳定复现三路竞态；
//!   `MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE` 在写完临时文件后注入失败（模拟崩溃，留下
//!   `.tmp` 残留）；`MIR2_MCP_CONF_TEST_PROCESS_PROBE` 强制覆盖进程探测（`dead` / `unknown` /
//!   `alive` / `alive:<token>`）。生产不设置 ⇒ 全部为空操作/真实探测，不改变任何语义。

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

/// CLI/运行配置：`--root` > env `MIR2_MCP_ROOT` > `E:/MirServer`。
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub root: PathBuf,
    /// 可选：gm_list 改为实读该 `gm_registry.json`（缺省用编译期嵌入副本）。
    pub registry_path: Option<PathBuf>,
    /// 可选：game-svr 本机 admin 通道地址（`gm_execute` 用）。
    ///
    /// 来源优先级：env `MIR2_MCP_ADMIN_ADDR` > 缺省 `127.0.0.1:<Server.conf [Admin] AdminPort>`。
    /// 两者都没有（`AdminPort` 缺省 0）⇒ `gm_execute` 返回结构化 `admin_not_configured`。
    pub admin_addr: Option<String>,
}

impl AppConfig {
    /// 默认 root（`E:/MirServer`）并读取环境变量覆盖。
    #[must_use]
    pub fn from_env() -> Self {
        let root = std::env::var_os("MIR2_MCP_ROOT")
            .map_or_else(|| PathBuf::from("E:/MirServer"), PathBuf::from);
        let admin_addr = std::env::var("MIR2_MCP_ADMIN_ADDR")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Self {
            root,
            registry_path: None,
            admin_addr,
        }
    }

    /// 固定写面：`<root>/M2GameSvr/Server.conf`。
    #[must_use]
    pub fn server_conf_path(&self) -> PathBuf {
        self.root.join("M2GameSvr").join("Server.conf")
    }
}

/// 白名单可写键的元数据。`live=false` 的键**永远拒绝写入**（见模块文档与 `docs/mcp-gm-server-20261101.md`）。
#[derive(Debug, Clone, Copy)]
pub struct WhitelistKey {
    /// 固定段名（`[Setup]`）。
    pub section: &'static str,
    /// 键名。
    pub key: &'static str,
    /// 该键在 oracle/Rust 里是否有消费点。
    pub live: bool,
    /// oracle 缺省值。
    pub default: i32,
    /// 本工具允许的下界（含）。
    pub min: i32,
    /// 本工具允许的上界（含）。
    pub max: i32,
    /// 语义说明（写进 `conf_keys` 输出与文档）。
    pub semantics: &'static str,
}

/// 白名单（卡面指定三键）。
pub const WHITELIST: &[WhitelistKey] = &[
    WhitelistKey {
        section: "Setup",
        key: "GenMonRate",
        live: true,
        default: 10,
        min: 1,
        max: 1000,
        semantics:
            "刷怪目标数除数：目标数 = max(1, round(max(1, Count) / (rate/10)))，值越大刷得越少；\
                    启动装载时生效（crates/world/src/mongen.rs gen_mod_count）",
    },
    WhitelistKey {
        section: "Setup",
        key: "MonsterPowerRate",
        live: true,
        default: 10,
        min: 1,
        max: 1000,
        semantics: "怪物模板能力缩放：HP/MP/AC/MAC/DC/DCMAX/MC/SC 按 rate/10 缩放\
                    （SabukWall/SabukDoor 的 HP 例外不乘）；怪物库装载时生效，重启才变",
    },
    WhitelistKey {
        section: "Setup",
        key: "ExpRate",
        live: false,
        default: 0,
        min: 0,
        max: 0,
        semantics: "oracle 无此键：Server.conf 没有 ExpRate，全服经验倍率不存在；\
                    每玩家倍率走脚本 KILLMONEXPRATE（docs/gm-operations-guide-20261031.md §4.4）。\
                    本工具列出但拒绝写入（写进去也不会被任何代码读取）",
    },
];

/// 按 `OrdinalIgnoreCase` 查白名单。
#[must_use]
pub fn whitelist_lookup(section: &str, key: &str) -> Option<&'static WhitelistKey> {
    WHITELIST.iter().find(|w| {
        w.section.eq_ignore_ascii_case(section.trim()) && w.key.eq_ignore_ascii_case(key.trim())
    })
}

/// `conf_get` 的结果。
#[derive(Debug, Serialize)]
pub struct ConfRead {
    pub path: String,
    pub section: String,
    pub key: String,
    pub found: bool,
    pub value: Option<String>,
    /// 生效行号（1-based）。
    pub line: Option<usize>,
}

/// `conf_set` 的结果。
#[derive(Debug, Serialize)]
pub struct SetOutcome {
    /// 是否真的写了盘（`dry_run` 与 no-op 都是 false）。
    pub wrote: bool,
    pub dry_run: bool,
    /// 值是否变化。
    pub changed: bool,
    pub path: String,
    pub section: String,
    pub key: String,
    /// 生效行号（1-based）。
    pub line: usize,
    pub before_value: String,
    pub new_value: String,
    /// 写前文件 sha256。
    pub before_sha: String,
    /// 写后（或 dry-run 预览）文件 sha256；no-op 时等于 `before_sha`。
    pub after_sha: String,
    /// 备份绝对路径（真实写入时才有）。
    pub backup_path: Option<String>,
    /// 写后回读到的值（真实写入时才有）。
    pub readback_value: Option<String>,
    /// 变更前整行（不含行尾换行）。
    pub before_line: String,
    /// 变更后整行（dry-run 为预览）。
    pub after_line: String,
    pub semantics: &'static str,
}

/// 错误分类：`code` 是机器可判的稳定标识。
#[derive(Debug)]
pub enum ConfError {
    NotWhitelisted {
        section: String,
        key: String,
    },
    NotLiveKey {
        key: &'static str,
        reason: &'static str,
    },
    InvalidValue {
        value: String,
        reason: String,
    },
    ConfMissing {
        path: PathBuf,
    },
    KeyAbsent {
        section: String,
        key: String,
        path: PathBuf,
    },
    UnsupportedEncoding {
        path: PathBuf,
        why: &'static str,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    ReadbackMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
        restored: bool,
    },
    /// 跨进程锁在超时内抢不到（另一个 `conf_set` 正在写同一份 `Server.conf`）。
    LockConflict {
        path: PathBuf,
        lock_path: PathBuf,
        waited_ms: u64,
        timeout_ms: u64,
        stale_ms: u64,
        /// 锁文件里记的持有者 pid（读不到时为 `None`）。
        holder_pid: Option<u32>,
    },
}

impl ConfError {
    /// 稳定的机器可判错误码。
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotWhitelisted { .. } => "not_whitelisted",
            Self::NotLiveKey { .. } => "not_live_key",
            Self::InvalidValue { .. } => "invalid_value",
            Self::ConfMissing { .. } => "conf_missing",
            Self::KeyAbsent { .. } => "key_absent",
            Self::UnsupportedEncoding { .. } => "unsupported_encoding",
            Self::Io { .. } => "io_error",
            Self::ReadbackMismatch { .. } => "readback_mismatch",
            Self::LockConflict { .. } => "conf_lock_timeout",
        }
    }

    /// 结构化 JSON（`{"code","message",…上下文}`），供 MCP 工具回包。
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "ok": false,
            "code": self.code(),
            "message": self.to_string(),
            "wrote": false,
        });
        let object = value.as_object_mut().expect("json! 构造的是对象");
        match self {
            Self::NotWhitelisted { section, key } => {
                object.insert("section".into(), section.clone().into());
                object.insert("key".into(), key.clone().into());
                object.insert(
                    "allowed".into(),
                    WHITELIST
                        .iter()
                        .map(|w| format!("[{}] {}", w.section, w.key))
                        .collect::<Vec<_>>()
                        .into(),
                );
            }
            Self::NotLiveKey { key, reason } => {
                object.insert("key".into(), (*key).into());
                object.insert("reason".into(), (*reason).into());
            }
            Self::InvalidValue { value, reason } => {
                object.insert("value".into(), value.clone().into());
                object.insert("reason".into(), reason.clone().into());
            }
            Self::ConfMissing { path } => {
                object.insert("path".into(), path.display().to_string().into());
            }
            Self::KeyAbsent { section, key, path } => {
                object.insert("section".into(), section.clone().into());
                object.insert("key".into(), key.clone().into());
                object.insert("path".into(), path.display().to_string().into());
            }
            Self::UnsupportedEncoding { path, why } => {
                object.insert("path".into(), path.display().to_string().into());
                object.insert("why".into(), (*why).into());
            }
            Self::Io { path, source } => {
                object.insert("path".into(), path.display().to_string().into());
                object.insert("io".into(), source.to_string().into());
            }
            Self::ReadbackMismatch {
                path,
                expected,
                actual,
                restored,
            } => {
                object.insert("path".into(), path.display().to_string().into());
                object.insert("expected".into(), expected.clone().into());
                object.insert("actual".into(), actual.clone().into());
                object.insert("restored".into(), (*restored).into());
            }
            Self::LockConflict {
                path,
                lock_path,
                waited_ms,
                timeout_ms,
                stale_ms,
                holder_pid,
            } => {
                object.insert("path".into(), path.display().to_string().into());
                object.insert("lock_path".into(), lock_path.display().to_string().into());
                object.insert("waited_ms".into(), (*waited_ms).into());
                object.insert("timeout_ms".into(), (*timeout_ms).into());
                object.insert("stale_ms".into(), (*stale_ms).into());
                object.insert(
                    "holder_pid".into(),
                    holder_pid.map_or(serde_json::Value::Null, |pid| pid.into()),
                );
            }
        }
        value
    }
}

impl fmt::Display for ConfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotWhitelisted { section, key } => write!(
                f,
                "键 [{section}] {key} 不在白名单；允许：{}",
                WHITELIST
                    .iter()
                    .map(|w| format!("[{}] {}", w.section, w.key))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::NotLiveKey { key, reason } => write!(f, "键 {key} 不是活键，拒绝写入：{reason}"),
            Self::InvalidValue { value, reason } => write!(f, "值 {value:?} 非法：{reason}"),
            Self::ConfMissing { path } => write!(f, "配置文件不存在：{}", path.display()),
            Self::KeyAbsent { section, key, path } => write!(
                f,
                "{} 里没有生效键 [{section}] {key}（本工具只改既有键，不插入新键）",
                path.display()
            ),
            Self::UnsupportedEncoding { path, why } => {
                write!(f, "{} 编码不受支持：{why}", path.display())
            }
            Self::Io { path, source } => write!(f, "{} 读写失败：{source}", path.display()),
            Self::ReadbackMismatch {
                path,
                expected,
                actual,
                restored,
            } => write!(
                f,
                "{} 回读校验失败（期望 {expected:?}，读到 {actual:?}；已回滚={restored}）",
                path.display()
            ),
            Self::LockConflict {
                path,
                lock_path,
                waited_ms,
                timeout_ms,
                holder_pid,
                ..
            } => write!(
                f,
                "并发写冲突：{} 的跨进程锁 {} 在 {}ms 内未释放（超时 {}ms；持有者 pid={}）",
                path.display(),
                lock_path.display(),
                waited_ms,
                timeout_ms,
                holder_pid.map_or_else(|| "未知".to_string(), |pid| pid.to_string())
            ),
        }
    }
}

impl std::error::Error for ConfError {}

/// 读取 `Server.conf` 的一个键（任意段/键，只读）。
///
/// # Errors
/// 文件缺失/不可读/UTF-16 BOM。
pub fn conf_get(cfg: &AppConfig, section: &str, key: &str) -> Result<ConfRead, ConfError> {
    let path = cfg.server_conf_path();
    let bytes = read_conf(&path)?;
    let scanned = scan(&bytes)?;
    let found = find_entry(&scanned, section, key);
    Ok(ConfRead {
        path: path.display().to_string(),
        section: section.to_string(),
        key: key.to_string(),
        found: found.is_some(),
        value: found.map(|e| e.value.clone()),
        line: found.map(|e| e.line),
    })
}

/// `[Admin] AdminPort`（game-svr 本机 admin 通道端口）。
///
/// 缺键 / 空值 ⇒ `0`（与 game-svr `config.rs` 的缺省一致 = 不监听）。非 0 但非法 ⇒
/// `InvalidValue`（fail-closed，不猜端口）。
///
/// # Errors
///
/// `Server.conf` 缺失/读失败/编码不支持，或 `AdminPort` 不是 `0..=65535` 的整数。
pub fn admin_port(cfg: &AppConfig) -> Result<u16, ConfError> {
    let read = conf_get(cfg, "Admin", "AdminPort")?;
    let Some(raw) = read.value else {
        return Ok(0);
    };
    let v = raw.trim();
    if v.is_empty() {
        return Ok(0);
    }
    v.parse::<u16>().map_err(|_| ConfError::InvalidValue {
        value: v.to_string(),
        reason: "AdminPort 不是合法端口（0..=65535）".to_string(),
    })
}

/// 白名单写（或 `dry_run` 预演）。
///
/// 顺序：白名单 → 活键 → 值域 → 文件 → 既有键 → before_sha → dry-run 就返回 →
/// 备份（返回路径）→ 临时文件 + rename 原子替换 → 回读校验（不一致则回滚备份）。
///
/// # Errors
/// 见 [`ConfError`]；任何拒绝路径都**不写盘**（`dry_run` 也一样）。
pub fn conf_set(
    cfg: &AppConfig,
    section: &str,
    key: &str,
    value: &str,
    dry_run: bool,
) -> Result<SetOutcome, ConfError> {
    conf_set_with_readback(cfg, section, key, value, dry_run, &|path: &Path| {
        fs::read(path)
    })
}

/// [`conf_set`] 的本体：`readback` 是回读接缝（生产传 `fs::read`，单测可注入错误/不符内容）。
fn conf_set_with_readback<F>(
    cfg: &AppConfig,
    section: &str,
    key: &str,
    value: &str,
    dry_run: bool,
    readback: &F,
) -> Result<SetOutcome, ConfError>
where
    F: Fn(&Path) -> io::Result<Vec<u8>>,
{
    let white = whitelist_lookup(section, key).ok_or_else(|| ConfError::NotWhitelisted {
        section: section.to_string(),
        key: key.to_string(),
    })?;
    if !white.live {
        return Err(ConfError::NotLiveKey {
            key: white.key,
            reason: white.semantics,
        });
    }
    let requested = value.trim();
    let parsed: i64 = requested.parse().map_err(|_| ConfError::InvalidValue {
        value: value.to_string(),
        reason: format!("{} 必须是十进制整数", white.key),
    })?;
    let parsed = i32::try_from(parsed).map_err(|_| ConfError::InvalidValue {
        value: value.to_string(),
        reason: "超出 i32 范围".to_string(),
    })?;
    if !(white.min..=white.max).contains(&parsed) {
        return Err(ConfError::InvalidValue {
            value: value.to_string(),
            reason: format!("{} 的允许范围是 {}..={}", white.key, white.min, white.max),
        });
    }
    let new_value = parsed.to_string();

    let path = cfg.server_conf_path();
    // 无锁预检读：`conf_missing` / `unsupported_encoding` 等既有错误码必须在创建锁文件之前
    // 返回（拒绝路径不碰根目录，不留下锁）。
    let probe = read_conf(&path)?;
    // `dry_run` 不取锁（理由见模块文档「跨进程互斥」末段）。
    let _lock = if dry_run {
        None
    } else {
        let lock = ConfLock::acquire(&path)?;
        // 持锁后顺手回收「确证已死 + 超龄」的崩溃残留（不误删活进程持有的临时文件）。
        cleanup_residue(&path, lock_options().stale_after);
        Some(lock)
    };
    // 非 dry-run 在锁内重读：备份 / rename / 回读 / 回滚必须基于同一份无竞态的输入。
    let bytes = if dry_run { probe } else { read_conf(&path)? };
    let scanned = scan(&bytes)?;
    let entry =
        find_entry(&scanned, white.section, white.key).ok_or_else(|| ConfError::KeyAbsent {
            section: white.section.to_string(),
            key: white.key.to_string(),
            path: path.clone(),
        })?;
    let before_sha = sha256_hex(&bytes);
    let before_line = line_text(&bytes, entry);
    let new_bytes = splice_value(&bytes, entry, new_value.as_bytes());
    let after_line = line_text(&new_bytes, entry);
    let preview_sha = sha256_hex(&new_bytes);

    if entry.value == new_value {
        return Ok(SetOutcome {
            wrote: false,
            dry_run,
            changed: false,
            path: path.display().to_string(),
            section: white.section.to_string(),
            key: white.key.to_string(),
            line: entry.line,
            before_value: entry.value.clone(),
            new_value,
            after_sha: before_sha.clone(),
            before_sha,
            backup_path: None,
            readback_value: None,
            before_line,
            after_line,
            semantics: white.semantics,
        });
    }

    if dry_run {
        return Ok(SetOutcome {
            wrote: false,
            dry_run: true,
            changed: true,
            path: path.display().to_string(),
            section: white.section.to_string(),
            key: white.key.to_string(),
            line: entry.line,
            before_value: entry.value.clone(),
            new_value,
            before_sha,
            after_sha: preview_sha,
            backup_path: None,
            readback_value: None,
            before_line,
            after_line,
            semantics: white.semantics,
        });
    }

    // 写前备份（返回路径）。
    let backup = write_backup(&path, &bytes)?;
    let backup_display = backup.display().to_string();

    atomic_replace(&path, &new_bytes).map_err(|source| ConfError::Io {
        path: path.clone(),
        source,
    })?;

    test_hold_after_rename();

    // 写后回读校验：文件 sha 与值都要一致；读失败或校验不符都要用备份回滚。
    let readback_bytes = match readback(&path) {
        Ok(bytes) => bytes,
        Err(source) => {
            let _ = fs::copy(&backup, &path);
            return Err(ConfError::Io {
                path: path.clone(),
                source,
            });
        }
    };
    let after_sha = sha256_hex(&readback_bytes);
    let readback_value = scan(&readback_bytes)
        .ok()
        .and_then(|s| find_entry(&s, white.section, white.key).map(|e| e.value.clone()))
        .unwrap_or_default();
    if after_sha != preview_sha || readback_value != new_value {
        let restored = fs::copy(&backup, &path).is_ok();
        return Err(ConfError::ReadbackMismatch {
            path,
            expected: new_value,
            actual: readback_value,
            restored,
        });
    }
    Ok(SetOutcome {
        wrote: true,
        dry_run: false,
        changed: true,
        path: path.display().to_string(),
        section: white.section.to_string(),
        key: white.key.to_string(),
        line: entry.line,
        before_value: entry.value.clone(),
        new_value: new_value.clone(),
        before_sha,
        after_sha,
        backup_path: Some(backup_display),
        readback_value: Some(readback_value),
        before_line,
        after_line,
        semantics: white.semantics,
    })
}

// ---------------------------------------------------------------------------
// 跨进程锁（`<dir>/Server.conf.lock`）
// ---------------------------------------------------------------------------

/// 抢锁的缺省上限（毫秒）：超过就返回 `conf_lock_timeout`，不无限等。
const DEFAULT_LOCK_TIMEOUT_MS: u64 = 5_000;
/// 判定「残留锁可接管」的缺省时间阈值（毫秒）。
const DEFAULT_LOCK_STALE_MS: u64 = 30_000;
/// 抢锁重试间隔（毫秒）。
const LOCK_RETRY_INTERVAL_MS: u64 = 25;

/// 锁参数（env 覆盖仅供运维/测试，缺省值见常量）。
#[derive(Debug, Clone, Copy)]
struct LockOptions {
    timeout: Duration,
    stale_after: Duration,
}

/// 读取 env 覆盖（非法值按缺省处理，不让配置把工具打挂）。
fn lock_options() -> LockOptions {
    LockOptions {
        timeout: Duration::from_millis(env_u64(
            "MIR2_MCP_CONF_LOCK_TIMEOUT_MS",
            DEFAULT_LOCK_TIMEOUT_MS,
        )),
        stale_after: Duration::from_millis(env_u64(
            "MIR2_MCP_CONF_LOCK_STALE_MS",
            DEFAULT_LOCK_STALE_MS,
        )),
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// 锁文件路径：与 `Server.conf` 同目录（rename/删除都在同一卷，原子性有保证）。
fn lock_path_for(conf_path: &Path) -> PathBuf {
    let name = conf_path.file_name().map_or_else(
        || "Server.conf".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    conf_path.with_file_name(format!("{name}.lock"))
}

/// 锁文件里的持有者身份；`Drop` 只在**逐字段等于自己**时删除（不做子串匹配）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct LockIdentity {
    pid: u32,
    acquired_ms: u64,
    /// 持有者进程启动时刻 token（探测不到时为 `None`；跨进程可比较，用于识别 pid 复用）。
    start: Option<String>,
    nonce: String,
}

impl LockIdentity {
    /// 生成一份新的锁身份（`nonce` 含 pid/时刻/进程内计数器，保证唯一）。
    fn fresh() -> Self {
        let pid = std::process::id();
        let acquired_ms = now_ms();
        Self {
            pid,
            acquired_ms,
            start: own_process_start(),
            nonce: format!("{pid}-{acquired_ms}-{}", unique_counter()),
        }
    }

    /// 锁文件正文（行式 `key=value`；`start` 探测不到时不写）。
    fn to_body(&self) -> String {
        let mut body = format!("pid={}\nacq_ms={}\n", self.pid, self.acquired_ms);
        if let Some(start) = &self.start {
            body.push_str("start=");
            body.push_str(start);
            body.push('\n');
        }
        body.push_str("nonce=");
        body.push_str(&self.nonce);
        body.push('\n');
        body
    }
}

/// 已持有的锁；`Drop` 时**只删身份逐字段仍属于自己的那份**（被接管/误删保护）。
struct ConfLock {
    path: PathBuf,
    identity: LockIdentity,
}

impl ConfLock {
    /// `create_new` 抢锁，失败有界重试；超时返回 [`ConfError::LockConflict`]。
    fn acquire(conf_path: &Path) -> Result<Self, ConfError> {
        let lock_path = lock_path_for(conf_path);
        let opts = lock_options();
        let started = Instant::now();
        loop {
            match create_lock_file(&lock_path) {
                Ok(identity) => {
                    return Ok(Self {
                        path: lock_path,
                        identity,
                    })
                }
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                    // 先看是否是「持有者确证已死且超过时间阈值」的残留锁；是则原子接管后立刻重试。
                    if try_take_over_stale(&lock_path, opts.stale_after) {
                        continue;
                    }
                    let elapsed = started.elapsed();
                    if elapsed >= opts.timeout {
                        let holder_pid = fs::read(&lock_path)
                            .ok()
                            .and_then(|bytes| parse_lock_meta(&bytes).pid);
                        return Err(ConfError::LockConflict {
                            path: conf_path.to_path_buf(),
                            lock_path,
                            waited_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                            timeout_ms: u64::try_from(opts.timeout.as_millis()).unwrap_or(u64::MAX),
                            stale_ms: u64::try_from(opts.stale_after.as_millis())
                                .unwrap_or(u64::MAX),
                            holder_pid,
                        });
                    }
                    let remaining = opts.timeout.saturating_sub(elapsed);
                    let nap = Duration::from_millis(LOCK_RETRY_INTERVAL_MS).min(remaining);
                    if !nap.is_zero() {
                        std::thread::sleep(nap);
                    }
                }
                Err(source) => {
                    return Err(ConfError::Io {
                        path: lock_path,
                        source,
                    })
                }
            }
        }
    }
}

impl Drop for ConfLock {
    fn drop(&mut self) {
        // 逐字段精确匹配（pid + acq_ms + start + nonce）；只做子串匹配会让「内容里恰好含
        // 相同 nonce 片段」的他锁被误删。
        let own = fs::read(&self.path)
            .ok()
            .and_then(|bytes| lock_identity_from_meta(&parse_lock_meta(&bytes)))
            .is_some_and(|current| current == self.identity);
        if own {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// 原子创建锁文件并写入持有者元数据（pid / 获取时刻 / 进程启动时刻 / nonce）。
fn create_lock_file(lock_path: &Path) -> io::Result<LockIdentity> {
    let mut file = open_lock_file(lock_path)?;
    let identity = LockIdentity::fresh();
    if let Err(source) = file.write_all(identity.to_body().as_bytes()) {
        drop(file);
        let _ = fs::remove_file(lock_path);
        return Err(source);
    }
    let _ = file.sync_all();
    Ok(identity)
}

/// `create_new` 抢锁文件。
///
/// Windows 上「另一个进程刚删掉锁文件、句柄尚未全部关闭」的 delete-pending 窗口里，
/// `CREATE_NEW` 会返回 `ERROR_ACCESS_DENIED` 而不是 `AlreadyExists`。这是纯瞬时竞态，
/// 做少量有界重试后交给外层重试循环；真正的权限问题重试后仍失败，按原样返回 `io_error`
/// （错误码语义不变）。
fn open_lock_file(lock_path: &Path) -> io::Result<fs::File> {
    let mut last = None;
    for attempt in 0..4 {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(file) => return Ok(file),
            Err(source) => {
                let transient = is_transient_create_new_error(&source);
                last = Some(source);
                if !transient || attempt == 3 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
    Err(last.expect("open_lock_file 至少尝试过一次"))
}

/// `create_new` 的瞬时失败：Windows `ERROR_ACCESS_DENIED`（delete-pending）。
fn is_transient_create_new_error(source: &io::Error) -> bool {
    #[cfg(windows)]
    {
        source.raw_os_error() == Some(5)
    }
    #[cfg(not(windows))]
    {
        let _ = source;
        false
    }
}

/// 锁文件内容（只解析需要的字段）。
#[derive(Debug, Default, Clone)]
struct LockMeta {
    pid: Option<u32>,
    acquired_ms: Option<u64>,
    start: Option<String>,
    nonce: Option<String>,
}

fn parse_lock_meta(bytes: &[u8]) -> LockMeta {
    let mut meta = LockMeta::default();
    for line in String::from_utf8_lossy(bytes).lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("pid=") {
            meta.pid = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("acq_ms=") {
            meta.acquired_ms = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("start=") {
            let v = v.trim();
            meta.start = (!v.is_empty()).then(|| v.to_string());
        } else if let Some(v) = line.strip_prefix("nonce=") {
            let v = v.trim();
            meta.nonce = (!v.is_empty()).then(|| v.to_string());
        }
    }
    meta
}

/// 只有 pid / acq_ms / nonce 齐全才构成可比较身份（`start` 可缺）。
fn lock_identity_from_meta(meta: &LockMeta) -> Option<LockIdentity> {
    Some(LockIdentity {
        pid: meta.pid?,
        acquired_ms: meta.acquired_ms?,
        start: meta.start.clone(),
        nonce: meta.nonce.clone()?,
    })
}

/// 锁年龄（毫秒）：优先 `acq_ms`，缺失时退化为锁文件 mtime。
fn lock_age_ms(lock_path: &Path, meta: &LockMeta) -> Option<u64> {
    if let Some(ts) = meta.acquired_ms {
        return Some(now_ms().saturating_sub(ts));
    }
    file_age_ms(lock_path)
}

/// 文件年龄（毫秒）：按 mtime。
fn file_age_ms(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let ts = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(now_ms().saturating_sub(u64::try_from(ts.as_millis()).unwrap_or(u64::MAX)))
}

/// 持有者状态（保守判定）：只有**确证已死**才返回 `Dead`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HolderState {
    Alive,
    Dead,
    Unknown,
}

/// 「pid + 进程启动时刻」双元组判定持有者是否还活着。
///
/// * `pid` 不存在 ⇒ `Dead`；
/// * `pid` 存在但启动时刻与锁里记录的不一致 ⇒ pid 已被复用 ⇒ `Dead`；
/// * 探测不可用、`pid` 缺失、任一侧缺启动时刻 ⇒ `Unknown`（调用方按「活」处理，绝不接管）；
/// * 其余 ⇒ `Alive`。
fn holder_state(meta: &LockMeta) -> HolderState {
    let Some(pid) = meta.pid else {
        return HolderState::Unknown;
    };
    match probe_process(pid) {
        ProcessProbe::Dead => HolderState::Dead,
        ProcessProbe::Unknown => HolderState::Unknown,
        ProcessProbe::Alive { start } => match (meta.start.as_deref(), start.as_deref()) {
            // 两边都有启动时刻：相等 ⇒ 原持有者仍活；不等 ⇒ pid 已被复用（死痕迹）。
            (Some(recorded), Some(probed)) if recorded == probed => HolderState::Alive,
            (Some(_), Some(_)) => HolderState::Dead,
            // 任一侧缺启动时刻 ⇒ 无法区分原持有者与复用 pid 的新进程 ⇒ 保守按活。
            _ => HolderState::Alive,
        },
    }
}

/// 残留锁接管：**时间阈值 + 持有者确证已死**双条件；活锁/无法判定的锁永不接管。
///
/// 接管用同目录 `rename` 抢占（同一目录内 rename 是原子的，多个接管者只有一个成功），
/// 抢到后把文件内容与判定时读到的字节逐字节比对：不一致说明撞上「持有者刚释放 / 刚被他人
/// 接管」的竞态。此时**不无条件删除**（搬走的可能是新持有者的活锁）：按搬走文件自己的身份
/// 重新判定，确证已死且超龄才删；否则放回原处（放不回时，若原位置内容仍是同一份则删重复
/// 副本，否则留作 `.stale.*` 交给后续 [`cleanup_residue`]）。返回 `true` 表示「残留已被
/// 移走，可以立刻重新 `create_new`」。
fn try_take_over_stale(lock_path: &Path, stale_after: Duration) -> bool {
    let Ok(bytes) = fs::read(lock_path) else {
        return false;
    };
    let meta = parse_lock_meta(&bytes);
    let Some(age_ms) = lock_age_ms(lock_path, &meta) else {
        return false;
    };
    let stale_ms = u64::try_from(stale_after.as_millis()).unwrap_or(u64::MAX);
    if age_ms < stale_ms {
        return false; // 未到时间阈值：即使 pid 已死也先等（避免与「刚创建/刚释放」竞态）
    }
    // 只有确证已死才接管；Unknown（探测不可用/身份不可判定）保守按活处理。
    if holder_state(&meta) != HolderState::Dead {
        return false;
    }

    // 测试专用接缝：放大「读锁之后、rename 之前」的窗口，供三路竞态用例稳定复现。
    test_hold_before_takeover_rename();

    let name = lock_path.file_name().map_or_else(
        || "Server.conf.lock".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let stale_path = lock_path.with_file_name(format!(
        ".{name}.stale.{}.{}.{}.{}",
        meta.pid.unwrap_or(0),
        name_token(meta.start.as_deref()),
        now_ms(),
        unique_counter()
    ));
    if fs::rename(lock_path, &stale_path).is_err() {
        return false; // 别的接管者先抢到了
    }
    let moved = fs::read(&stale_path).unwrap_or_default();
    if moved == bytes {
        let _ = fs::remove_file(&stale_path);
        return true;
    }
    // 三路竞态：判定之后原位置被**新锁**替换，我们搬走的是另一份锁。按它自己的身份重判。
    let moved_meta = parse_lock_meta(&moved);
    let moved_dead = holder_state(&moved_meta) == HolderState::Dead
        && lock_age_ms(&stale_path, &moved_meta).is_some_and(|age| age >= stale_ms);
    if moved_dead {
        let _ = fs::remove_file(&stale_path);
        return true;
    }
    if fs::rename(&stale_path, lock_path).is_ok() {
        return false; // 放回原处，交给下一轮重新判定
    }
    // 原位置已被占：若内容与搬走的那份相同，它只是重复副本（活锁仍在 canonical 位置），
    // 删副本是安全的；否则留作 `.stale.*`，由后续清理按「确证已死 + 超龄」决定。
    if fs::read(lock_path).ok().as_deref() == Some(moved.as_slice()) {
        let _ = fs::remove_file(&stale_path);
    }
    false
}

/// 清理崩溃残留：`.<name>.mir2-mcp.<pid>.<start>.<stamp>.<counter>.tmp` 与
/// `.<name>.stale.<pid>.<start>.<stamp>.<counter>`。
///
/// 只删「确证持有者已死（pid 不存在或 pid 被复用）+ 年龄 ≥ stale 阈值」的残留；存活进程
/// 持有（启动时刻一致）、探测不可用、身份不可判定的一律保留（宁留残片也不误删正在用的文件）。
/// 调用方在**已持锁**时调用，且失败一律吞掉——清理绝不改变 `conf_set` 的成败与错误码。
fn cleanup_residue(conf_path: &Path, stale_after: Duration) {
    let lock_path = lock_path_for(conf_path);
    let Some(dir) = lock_path.parent() else {
        return;
    };
    let base = lock_path.file_name().map_or_else(
        || "Server.conf.lock".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let conf_name = base.strip_suffix(".lock").unwrap_or(&base);
    let tmp_prefix = format!(".{conf_name}.mir2-mcp.");
    let stale_prefix = format!(".{conf_name}.stale.");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(rest) = name
            .strip_prefix(&tmp_prefix)
            .and_then(|rest| rest.strip_suffix(".tmp"))
        {
            let Some((pid, start, stamp)) = residue_fields(rest) else {
                continue;
            };
            let age = stamp
                .map(|stamp| now_ms().saturating_sub(stamp))
                .or_else(|| file_age_ms(&path));
            if residue_reclaimable(Some(pid), start.as_deref(), age, stale_after) {
                let _ = fs::remove_file(&path);
            }
        } else if let Some(rest) = name.strip_prefix(&stale_prefix) {
            let (pid, start, age) = stale_residue_identity(&path, rest);
            if residue_reclaimable(pid, start.as_deref(), age, stale_after) {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// 残留名尾部的 `{pid}.{start}.{stamp}.{counter}` → `(pid, start, stamp)`。
fn residue_fields(rest: &str) -> Option<(u32, Option<String>, Option<u64>)> {
    let parts: Vec<&str> = rest.rsplitn(4, '.').collect();
    if parts.len() < 4 {
        return None;
    }
    let pid = parts[3].parse::<u32>().ok()?;
    let start = (parts[2] != "-").then(|| parts[2].to_string());
    Some((pid, start, parts[1].parse::<u64>().ok()))
}

/// `.stale.*` 的持有者身份：锁体（行式元数据）优先，名字兜底。
fn stale_residue_identity(path: &Path, rest: &str) -> (Option<u32>, Option<String>, Option<u64>) {
    if let Ok(bytes) = fs::read(path) {
        let meta = parse_lock_meta(&bytes);
        if let Some(pid) = meta.pid {
            let age = lock_age_ms(path, &meta).or_else(|| file_age_ms(path));
            return (Some(pid), meta.start, age);
        }
    }
    match residue_fields(rest) {
        Some((pid, start, stamp)) => {
            let age = stamp
                .map(|stamp| now_ms().saturating_sub(stamp))
                .or_else(|| file_age_ms(path));
            (Some(pid), start, age)
        }
        None => (None, None, file_age_ms(path)),
    }
}

/// 残留是否可删：必须先过年龄阈值，再**确证持有者已死**。
fn residue_reclaimable(
    pid: Option<u32>,
    recorded_start: Option<&str>,
    age_ms: Option<u64>,
    stale_after: Duration,
) -> bool {
    let Some(pid) = pid else {
        return false;
    };
    let Some(age_ms) = age_ms else {
        return false;
    };
    if age_ms < u64::try_from(stale_after.as_millis()).unwrap_or(u64::MAX) {
        return false;
    }
    match probe_process(pid) {
        ProcessProbe::Dead => true,
        ProcessProbe::Alive {
            start: Some(probed),
        } => recorded_start.is_some_and(|recorded| {
            name_token(Some(recorded)) != name_token(Some(probed.as_str()))
        }),
        ProcessProbe::Alive { start: None } | ProcessProbe::Unknown => false,
    }
}

/// 名字里允许的 token：无点、无空格；`None` 记作 `-`。
fn name_token(token: Option<&str>) -> String {
    match token {
        Some(token) => token
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect(),
        None => "-".to_string(),
    }
}

/// 进程探测结果。
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProcessProbe {
    /// 活着；`start` = 进程启动时刻 token（探测不到时为 `None`）。
    Alive { start: Option<String> },
    /// 确定不存在。
    Dead,
    /// 无法判定（探测工具不可用/输出不可解析）；调用方**保守视为活**，绝不据此接管/删除。
    Unknown,
}

/// 测试专用接缝：强制覆盖进程探测结果（生产不设置 ⇒ `None`，走真实探测）。
///
/// 取值：`dead` / `unknown` / `alive`（启动时刻未知）/ `alive:<token>`。
fn test_probe_override() -> Option<ProcessProbe> {
    let raw = std::env::var("MIR2_MCP_CONF_TEST_PROCESS_PROBE").ok()?;
    let raw = raw.trim();
    match raw {
        "dead" => Some(ProcessProbe::Dead),
        "unknown" => Some(ProcessProbe::Unknown),
        "alive" => Some(ProcessProbe::Alive { start: None }),
        _ => raw.strip_prefix("alive:").map(|token| ProcessProbe::Alive {
            start: Some(token.to_string()),
        }),
    }
}

/// 进程存活 + 启动时刻探测（Windows 用 `tasklist` + `wmic`/PowerShell；
/// Unix 用 `/proc/<pid>/stat`，无 `/proc` 时退化为 `ps -o lstart=`）。
fn probe_process(pid: u32) -> ProcessProbe {
    if let Some(forced) = test_probe_override() {
        return forced;
    }
    probe_process_impl(pid)
}

/// 本进程的启动时刻 token；只探测一次并缓存（探测失败缓存 `None`，即无法做 pid 复用判定）。
fn own_process_start() -> Option<String> {
    static OWN_START: OnceLock<Option<String>> = OnceLock::new();
    OWN_START
        .get_or_init(|| match probe_process(std::process::id()) {
            ProcessProbe::Alive { start } => start,
            ProcessProbe::Dead | ProcessProbe::Unknown => None,
        })
        .clone()
}

#[cfg(windows)]
fn probe_process_impl(pid: u32) -> ProcessProbe {
    match tasklist_process_alive(pid) {
        None => ProcessProbe::Unknown,
        Some(false) => ProcessProbe::Dead,
        Some(true) => ProcessProbe::Alive {
            start: windows_process_start(pid),
        },
    }
}

/// Windows 存活探测：`tasklist` CSV 里任一列等于 pid 即视为存在。
///
/// 命令起不来/输出不可解析 ⇒ `None`（无法判定），由调用方保守处理。
#[cfg(windows)]
fn tasklist_process_alive(pid: u32) -> Option<bool> {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let wanted = pid.to_string();
    let text = String::from_utf8_lossy(&output.stdout);
    Some(text.lines().any(|line| {
        line.split(',')
            .any(|field| field.trim().trim_matches('"') == wanted)
    }))
}

/// Windows 进程启动时刻 token：`wmic` 优先（快），不可用时退回 PowerShell CIM。
#[cfg(windows)]
fn windows_process_start(pid: u32) -> Option<String> {
    wmic_process_start(pid).or_else(|| powershell_process_start(pid))
}

#[cfg(windows)]
fn wmic_process_start(pid: u32) -> Option<String> {
    let output = std::process::Command::new("wmic")
        .args([
            "process",
            "where",
            &format!("ProcessId={pid}"),
            "get",
            "CreationDate",
            "/value",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let raw = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("CreationDate="))?;
    dmtf_to_epoch_secs(raw.trim()).map(|secs| secs.to_string())
}

#[cfg(windows)]
fn powershell_process_start(pid: u32) -> Option<String> {
    let script = format!(
        "$p=Get-CimInstance Win32_Process -Filter 'ProcessId={pid}'; \
         if($p){{$p.CreationDate.ToFileTimeUtc()}}"
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let ticks: u64 = text.trim().parse().ok()?;
    // FILETIME（100ns since 1601）→ Unix epoch 秒的换算常数。
    const FILETIME_UNIX_EPOCH_SECS: u64 = 11_644_473_600;
    Some(
        (ticks / 10_000_000)
            .saturating_sub(FILETIME_UNIX_EPOCH_SECS)
            .to_string(),
    )
}

/// DMTF/CIM 时间 `YYYYMMDDHHMMSS[.ffffff]±ooo`（offset 为分钟）→ 1970 epoch 秒。
#[cfg(windows)]
fn dmtf_to_epoch_secs(raw: &str) -> Option<i64> {
    let (main, rest) = raw.split_once('.').unwrap_or((raw, ""));
    let offset_min = match rest.find(['+', '-']) {
        Some(pos) => {
            let (_, offset) = rest.split_at(pos);
            let sign = if offset.starts_with('-') { -1_i64 } else { 1 };
            offset[1..].parse::<i64>().ok()? * sign
        }
        None => 0,
    };
    if main.len() != 14 || !main.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let y = main[0..4].parse::<i64>().ok()?;
    let mo = main[4..6].parse::<u32>().ok()?;
    let d = main[6..8].parse::<u32>().ok()?;
    let h = main[8..10].parse::<i64>().ok()?;
    let mi = main[10..12].parse::<i64>().ok()?;
    let s = main[12..14].parse::<i64>().ok()?;
    Some(days_from_civil(y, mo, d) * 86_400 + h * 3_600 + mi * 60 + s - offset_min * 60)
}

/// civil 日期 → 1970-01-01 起的天数（Howard Hinnant 算法）。
#[cfg(windows)]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(unix)]
fn probe_process_impl(pid: u32) -> ProcessProbe {
    if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
        // `comm`（字段 2）可能含空格/右括号，所以从最后一个 ')' 之后切：切出的 fields[0]
        // 是 state（总字段 3），starttime 是总字段 22 ⇒ fields[19]。
        let Some(pos) = stat.rfind(')') else {
            return ProcessProbe::Unknown;
        };
        return match stat[pos + 1..]
            .split_whitespace()
            .nth(19)
            .and_then(|v| v.parse::<u64>().ok())
        {
            Some(ticks) => ProcessProbe::Alive {
                start: Some(ticks.to_string()),
            },
            None => ProcessProbe::Unknown,
        };
    }
    if Path::new("/proc/self").exists() {
        // /proc 可用但这 pid 没有目录 ⇒ 进程不存在。
        return ProcessProbe::Dead;
    }
    // 非 Linux（无 /proc）：`ps -o lstart=` 兜底。
    match std::process::Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                ProcessProbe::Dead
            } else {
                ProcessProbe::Alive {
                    start: Some(fnv1a_token(&text)),
                }
            }
        }
        _ => ProcessProbe::Unknown,
    }
}

/// `lstart` 文本 → 固定长度 hex token（名字/锁体里只允许无点无空格的 token）。
#[cfg(unix)]
fn fnv1a_token(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("h{hash:016x}")
}

#[cfg(not(any(windows, unix)))]
fn probe_process_impl(pid: u32) -> ProcessProbe {
    let _ = pid;
    ProcessProbe::Unknown
}

/// 测试专用接缝：放大「rename 之后、回读之前」的窗口，供 A1 并发用例稳定复现互滚覆盖。
///
/// 生产不设置 `MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS` ⇒ 恒 0 ⇒ 空操作，不改变任何语义。
fn test_hold_after_rename() {
    let ms = env_u64("MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS", 0);
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// 测试专用接缝：放大 `try_take_over_stale` 里「读锁之后、rename 之前」的窗口，
/// 供三路竞态用例稳定复现「判定后原位置被新锁替换」。生产不设置 ⇒ 恒 0 ⇒ 空操作。
fn test_hold_before_takeover_rename() {
    let ms = env_u64("MIR2_MCP_CONF_TEST_HOLD_BEFORE_TAKEOVER_RENAME_MS", 0);
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

fn read_conf(path: &Path) -> Result<Vec<u8>, ConfError> {
    if !path.exists() {
        return Err(ConfError::ConfMissing {
            path: path.to_path_buf(),
        });
    }
    let bytes = fs::read(path).map_err(|source| ConfError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(ConfError::UnsupportedEncoding {
            path: path.to_path_buf(),
            why: "UTF-16（本工具按字节改写，只支持 UTF-8/GBK 类单字节换行文本）",
        });
    }
    Ok(bytes)
}

/// 一条生效键（值区间用原始字节偏移，便于逐字节替换）。
#[derive(Debug, Clone)]
struct Entry {
    section_lower: Vec<u8>,
    key_lower: Vec<u8>,
    value: String,
    line: usize,
    line_start: usize,
    line_end: usize,
    value_start: usize,
    value_end: usize,
}

#[derive(Debug, Default)]
struct Scan {
    entries: Vec<Entry>,
    /// 重复段名触发的加载中断（`ConfigFile.Load` 的 `goto Label_02AE`）。
    aborted: bool,
}

fn scan(bytes: &[u8]) -> Result<Scan, ConfError> {
    let mut out = Scan::default();
    let mut seen_sections: HashSet<Vec<u8>> = HashSet::new();
    let mut seen_keys: HashSet<(Vec<u8>, Vec<u8>)> = HashSet::new();
    let mut cur_sec: Vec<u8> = Vec::new();
    let mut large_comment = false;
    let mut is_cur_sec_comment = false;

    let mut line_start = 0usize;
    let mut line_no = 1usize;
    while line_start <= bytes.len() {
        let line_end = bytes[line_start..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |p| line_start + p);
        let mut scan_start = line_start;
        if line_no == 1 && bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            scan_start = 3.min(line_end);
        }
        let (s, e) = trim_range(bytes, scan_start, line_end);
        let text = &bytes[s..e];

        if !text.is_empty() && !text.starts_with(b";") {
            if text.starts_with(b"/*") {
                large_comment = true;
            } else if large_comment {
                if text.starts_with(b"*/") {
                    large_comment = false;
                }
            } else if let Some(sec) = section_name(text) {
                let sec = trim_slice(sec).to_vec();
                // C# `ConfigFile.cs:293-301`：只有 `sec.Length >= 2` 才判 `;;` 段注释并复位
                // `isCurSecComment`。单字符段名（如 `[a]`）**不复位**：段仍登记，但其后的键继续
                // 处在段注释状态而被跳过（`[;;Disabled]` → `[a]` → `Hidden=2` 不装载）。
                if sec.len() >= 2 && sec.starts_with(b";;") {
                    is_cur_sec_comment = true;
                } else {
                    if sec.len() >= 2 {
                        is_cur_sec_comment = false;
                    }
                    let low = ascii_lower(&sec);
                    if !seen_sections.insert(low.clone()) {
                        out.aborted = true;
                        break;
                    }
                    cur_sec = low;
                }
            } else if !is_cur_sec_comment {
                if let Some(entry) = parse_key_line(bytes, line_no, s, e, &cur_sec, &mut seen_keys)
                {
                    out.entries.push(entry);
                }
            }
        }

        if line_end == bytes.len() {
            break;
        }
        line_start = line_end + 1;
        line_no += 1;
    }
    Ok(out)
}

/// `GetSecString`：`[xxx]` → `xxx`；`pos > 0` 才算段名（`[` 后立刻 `]` 不算）。
fn section_name(text: &[u8]) -> Option<&[u8]> {
    if text.first() != Some(&b'[') {
        return None;
    }
    let pos = text.iter().position(|b| *b == b']')?;
    if pos == 0 {
        return None;
    }
    Some(&text[1..pos])
}

fn parse_key_line(
    bytes: &[u8],
    line_no: usize,
    s: usize,
    e: usize,
    cur_sec: &[u8],
    seen_keys: &mut HashSet<(Vec<u8>, Vec<u8>)>,
) -> Option<Entry> {
    // 行内 `;;` 之后是注释：C# 先截断再 SplitKeyVal/Trim。
    let cut = bytes[s..e]
        .windows(2)
        .position(|w| w == b";;")
        .map_or(e, |p| s + p);
    let (s2, e2) = trim_range(bytes, s, cut);
    let eq_rel = bytes[s2..e2].iter().position(|b| *b == b'=')?;
    if eq_rel == 0 {
        return None;
    }
    let eq = s2 + eq_rel;
    let (ks, ke) = trim_range(bytes, s2, eq);
    let (vs, ve) = trim_range(bytes, eq + 1, e2);
    let key_lower = ascii_lower(&bytes[ks..ke]);
    if !seen_keys.insert((cur_sec.to_vec(), key_lower)) {
        // C# 同段同名键只有首个生效。
        return None;
    }
    Some(Entry {
        section_lower: cur_sec.to_vec(),
        key_lower: ascii_lower(&bytes[ks..ke]),
        value: String::from_utf8_lossy(&bytes[vs..ve]).into_owned(),
        line: line_no,
        line_start: s,
        line_end: e,
        value_start: vs,
        value_end: ve,
    })
}

fn find_entry<'a>(scan: &'a Scan, section: &str, key: &str) -> Option<&'a Entry> {
    let section = ascii_lower(section.trim().as_bytes());
    let key = ascii_lower(key.trim().as_bytes());
    scan.entries
        .iter()
        .find(|e| e.section_lower == section && e.key_lower == key)
}

fn trim_range(bytes: &[u8], mut s: usize, mut e: usize) -> (usize, usize) {
    while s < e && bytes[s].is_ascii_whitespace() {
        s += 1;
    }
    while e > s && bytes[e - 1].is_ascii_whitespace() {
        e -= 1;
    }
    (s, e)
}

fn trim_slice(bytes: &[u8]) -> &[u8] {
    let (s, e) = trim_range(bytes, 0, bytes.len());
    &bytes[s..e]
}

fn ascii_lower(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(u8::to_ascii_lowercase).collect()
}

fn line_text(bytes: &[u8], entry: &Entry) -> String {
    String::from_utf8_lossy(&bytes[entry.line_start..entry.line_end]).into_owned()
}

fn splice_value(bytes: &[u8], entry: &Entry, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + value.len());
    out.extend_from_slice(&bytes[..entry.value_start]);
    out.extend_from_slice(value);
    out.extend_from_slice(&bytes[entry.value_end..]);
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

/// 备份目录：系统临时目录下的 `mir2-mcp-backups/`。
#[must_use]
pub fn backup_dir() -> PathBuf {
    std::env::temp_dir().join("mir2-mcp-backups")
}

fn write_backup(path: &Path, bytes: &[u8]) -> Result<PathBuf, ConfError> {
    let dir = backup_dir();
    fs::create_dir_all(&dir).map_err(|source| ConfError::Io {
        path: dir.clone(),
        source,
    })?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let name = path.file_name().map_or_else(
        || "Server.conf".to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    let target = dir.join(format!(
        "{name}.{stamp}.{}.{}.bak",
        std::process::id(),
        unique_counter()
    ));
    fs::write(&target, bytes).map_err(|source| ConfError::Io {
        path: target.clone(),
        source,
    })?;
    Ok(target)
}

fn unique_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// 临时文件 + `rename`（Windows 上 `MoveFileEx(REPLACE_EXISTING)`）原子替换。
///
/// 临时文件名带「pid + 进程启动时刻 + 创建时刻 + 计数器」，供 [`cleanup_residue`] 在后续
/// 运行中判定「持有者确证已死 + 超龄」后回收崩溃残留。
fn atomic_replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name().map_or_else(
        || "Server.conf".to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let tmp = dir.join(format!(
        ".{name}.mir2-mcp.{}.{}.{stamp}.{}.tmp",
        std::process::id(),
        name_token(own_process_start().as_deref()),
        unique_counter()
    ));
    fs::write(&tmp, bytes)?;
    if env_flag("MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE") {
        // 测试接缝：模拟「写完临时文件后崩溃」，把 `.tmp` 残留留在盘上供清理用例断言。
        return Err(io::Error::other(
            "injected failure after tmp write (test seam)",
        ));
    }
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// 环境变量开关：未设置 / 空串 / `0` / `false`（大小写不敏感）⇒ `false`。
fn env_flag(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        let value = value.trim();
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(tag: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let base = std::env::temp_dir().join(format!(
                "mir2-mcp-selftest-{tag}-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(base.join("M2GameSvr")).expect("mkdir");
            Self { path: base }
        }

        fn conf_path(&self) -> PathBuf {
            self.path.join("M2GameSvr").join("Server.conf")
        }

        fn write_conf(&self, body: &[u8]) {
            fs::write(self.conf_path(), body).expect("写夹具");
        }

        fn cfg(&self) -> AppConfig {
            AppConfig {
                root: self.path.clone(),
                registry_path: None,
                admin_addr: None,
            }
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn fixture() -> Vec<u8> {
        let mut bytes = Vec::from(&b"\xEF\xBB\xBF"[..]);
        bytes.extend_from_slice(
            b"[DataBase]\r\nDbType=MySQL\r\n\r\n[Setup]\r\nClientVersion=0\r\n\
MonsterPowerRate=10\r\n; comment line\r\nGenMonRate=10 ;;\xe6\x97\xa7\xe6\xb3\xa8\xe9\x87\x8a\r\n\
ProcessMonRandRate=5\r\n",
        );
        bytes
    }

    fn find_conf_sha(root: &Path) -> String {
        sha256_hex(&fs::read(root.join("M2GameSvr").join("Server.conf")).expect("read"))
    }

    #[test]
    fn conf_get_reads_case_insensitive_and_trims() {
        let root = TempRoot::new("get");
        root.write_conf(&fixture());
        let read = conf_get(&root.cfg(), "setup", "genmonrate").expect("get");
        assert!(read.found);
        assert_eq!(read.value.as_deref(), Some("10"));
        assert_eq!(read.line, Some(8));

        let missing = conf_get(&root.cfg(), "Setup", "NoSuchKey").expect("get");
        assert!(!missing.found);
        assert_eq!(missing.value, None);
    }

    #[test]
    fn conf_get_missing_file_is_typed_error() {
        let root = TempRoot::new("missing");
        let err = conf_get(&root.cfg(), "Setup", "GenMonRate").unwrap_err();
        assert_eq!(err.code(), "conf_missing");
        assert!(!root.conf_path().exists());
    }

    #[test]
    fn conf_set_roundtrip_backup_and_readback() {
        let root = TempRoot::new("set");
        root.write_conf(&fixture());
        let before_sha = find_conf_sha(&root.path);
        let before_bytes = fs::read(root.conf_path()).expect("read");

        let out = conf_set(&root.cfg(), "Setup", "GenMonRate", "20", false).expect("set");
        assert!(out.wrote);
        assert!(out.changed);
        assert_eq!(out.before_sha, before_sha);
        assert_ne!(out.after_sha, out.before_sha);
        assert_eq!(out.readback_value.as_deref(), Some("20"));
        assert_eq!(out.after_sha, find_conf_sha(&root.path));
        assert_eq!(
            find_entry(
                &scan(&fs::read(root.conf_path()).unwrap()).unwrap(),
                "Setup",
                "GenMonRate"
            )
            .unwrap()
            .value,
            "20"
        );

        // 备份存在且逐字节等于写前内容。
        let backup = PathBuf::from(out.backup_path.as_ref().expect("backup"));
        assert!(backup.exists(), "备份必须存在");
        assert_eq!(fs::read(&backup).expect("read backup"), before_bytes);

        // 行内 `;;` 注释与其余行逐字节保留（只有值区间变了）。
        let text = String::from_utf8_lossy(&fs::read(root.conf_path()).unwrap()).into_owned();
        assert!(text.contains("GenMonRate=20 ;;旧注释"));
        assert!(text.contains("MonsterPowerRate=10"));
        assert!(text.contains("; comment line"));

        // 备份恢复后 sha 回到写前值。
        fs::copy(&backup, root.conf_path()).expect("restore");
        assert_eq!(find_conf_sha(&root.path), before_sha, "备份恢复 sha 相等");
    }

    #[test]
    fn dry_run_is_zero_write() {
        let root = TempRoot::new("dryrun");
        root.write_conf(&fixture());
        let before_sha = find_conf_sha(&root.path);
        let before_tree = tree_sha(&root.path);

        let out = conf_set(&root.cfg(), "Setup", "MonsterPowerRate", "40", true).expect("dry");
        assert!(!out.wrote);
        assert!(out.dry_run);
        assert!(out.changed);
        assert_eq!(out.before_sha, before_sha);
        assert_ne!(out.after_sha, before_sha, "dry-run 仍给出预览 after_sha");
        assert!(out.backup_path.is_none());
        assert_eq!(find_conf_sha(&root.path), before_sha, "dry-run 不得改文件");
        assert_eq!(
            tree_sha(&root.path),
            before_tree,
            "dry-run 目录 sha 前后相等"
        );
        assert_eq!(out.before_line, "MonsterPowerRate=10");
        assert_eq!(out.after_line, "MonsterPowerRate=40");
    }

    #[test]
    fn rejects_non_whitelisted_and_other_section() {
        let root = TempRoot::new("reject-white");
        root.write_conf(&fixture());
        let before = tree_sha(&root.path);

        for (section, key) in [
            ("Setup", "ClientVersion"),
            ("DataBase", "DbType"),
            ("Setup", "MonsterPowerRate2"),
            ("", "GenMonRate"),
        ] {
            let err = conf_set(&root.cfg(), section, key, "1", false).unwrap_err();
            assert_eq!(err.code(), "not_whitelisted", "[{section}] {key}");
        }
        assert_eq!(tree_sha(&root.path), before, "拒绝路径零写");
    }

    #[test]
    fn rejects_non_live_key() {
        let root = TempRoot::new("reject-live");
        root.write_conf(&fixture());
        let before = tree_sha(&root.path);
        let err = conf_set(&root.cfg(), "Setup", "ExpRate", "200", false).unwrap_err();
        assert_eq!(err.code(), "not_live_key");
        let err = conf_set(&root.cfg(), "Setup", "ExpRate", "200", true).unwrap_err();
        assert_eq!(err.code(), "not_live_key");
        assert_eq!(tree_sha(&root.path), before, "拒绝路径零写");
    }

    #[test]
    fn rejects_illegal_values() {
        let root = TempRoot::new("reject-value");
        root.write_conf(&fixture());
        let before = tree_sha(&root.path);
        for value in ["", "abc", "10.5", "1e3", "0", "-3", "1000000000000"] {
            let err = conf_set(&root.cfg(), "Setup", "GenMonRate", value, false).unwrap_err();
            assert_eq!(err.code(), "invalid_value", "value={value:?}");
        }
        assert_eq!(tree_sha(&root.path), before, "拒绝路径零写");
    }

    #[test]
    fn rejects_absent_key_without_inserting() {
        let root = TempRoot::new("absent");
        root.write_conf(b"[Setup]\r\nGenMonRate=10\r\n");
        let before = tree_sha(&root.path);
        let err = conf_set(&root.cfg(), "Setup", "MonsterPowerRate", "20", false).unwrap_err();
        assert_eq!(err.code(), "key_absent");
        assert_eq!(tree_sha(&root.path), before);
    }

    #[test]
    fn handle_same_value_is_noop() {
        let root = TempRoot::new("noop");
        root.write_conf(&fixture());
        let before = find_conf_sha(&root.path);
        let out = conf_set(&root.cfg(), "Setup", "GenMonRate", "10", false).expect("noop");
        assert!(!out.wrote);
        assert!(!out.changed);
        assert_eq!(out.before_sha, out.after_sha);
        assert!(out.backup_path.is_none());
        assert_eq!(find_conf_sha(&root.path), before);
    }

    #[test]
    fn no_temp_files_left_behind() {
        let root = TempRoot::new("tmpclean");
        root.write_conf(&fixture());
        conf_set(&root.cfg(), "Setup", "GenMonRate", "30", false).expect("set");
        let leftovers: Vec<String> = fs::read_dir(root.path.join("M2GameSvr"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "临时文件残留：{leftovers:?}");
    }

    /// 跨进程锁：成功路径与回滚路径都必须在返回前删掉 `Server.conf.lock`（无残留）。
    #[test]
    fn conf_lock_is_released_on_success_and_rollback() {
        let root = TempRoot::new("lockclean");
        root.write_conf(&fixture());
        let lock = root.path.join("M2GameSvr").join("Server.conf.lock");

        conf_set(&root.cfg(), "Setup", "GenMonRate", "31", false).expect("set");
        assert!(!lock.exists(), "成功路径必须释放锁文件");

        let err = conf_set_with_readback(
            &root.cfg(),
            "Setup",
            "GenMonRate",
            "32",
            false,
            &|_path: &Path| Ok(b"not the conf".to_vec()),
        )
        .unwrap_err();
        assert_eq!(err.code(), "readback_mismatch");
        assert!(!lock.exists(), "回滚路径也必须释放锁文件");

        let leftovers: Vec<String> = fs::read_dir(root.path.join("M2GameSvr"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".lock") || n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "锁/临时文件残留：{leftovers:?}");
    }

    /// `dry_run` 不取锁：目录树（含隐藏文件）前后逐字节一致。
    #[test]
    fn dry_run_never_touches_lock_file() {
        let root = TempRoot::new("drylock");
        root.write_conf(&fixture());
        let before = tree_sha(&root.path);
        conf_set(&root.cfg(), "Setup", "GenMonRate", "33", true).expect("dry");
        assert_eq!(tree_sha(&root.path), before, "dry_run 目录 sha 前后相等");
        assert!(!root
            .path
            .join("M2GameSvr")
            .join("Server.conf.lock")
            .exists());
    }

    #[test]
    fn duplicate_section_aborts_like_oracle() {
        // C#：重复段名 ⇒ 整个 Load 中断，后面的键都没装载。
        let root = TempRoot::new("dupsec");
        root.write_conf(
            b"[Setup]\r\nMonsterPowerRate=10\r\n[Other]\r\nA=1\r\n[Setup]\r\nGenMonRate=10\r\n",
        );
        let read = conf_get(&root.cfg(), "Setup", "GenMonRate").expect("get");
        assert!(!read.found, "重复段之后不装载");
        let err = conf_set(&root.cfg(), "Setup", "GenMonRate", "20", false).unwrap_err();
        assert_eq!(err.code(), "key_absent");
    }

    #[test]
    fn blank_and_comment_lines_are_skipped() {
        let root = TempRoot::new("comments");
        root.write_conf(
            b"; header\r\n[Setup]\r\n/* block\r\nGenMonRate=99\r\n*/\r\nGenMonRate=10\r\n",
        );
        let read = conf_get(&root.cfg(), "Setup", "GenMonRate").expect("get");
        assert_eq!(read.value.as_deref(), Some("10"), "段注释里的键不生效");
        assert_eq!(read.line, Some(6));
    }

    /// C# `ConfigFile.cs:293-301`：`;;` 段注释只由 **长度 ≥2** 的段名复位；
    /// 单字符段名（`[a]`）登记段但保持注释状态 ⇒ 其后的键不装载。
    #[test]
    fn single_char_section_does_not_reset_section_comment_like_oracle() {
        let root = TempRoot::new("sec-comment");
        root.write_conf(
            b"[z]\r\nLoaded=9\r\n[;;Disabled]\r\n[a]\r\nHidden=2\r\n[bb]\r\nShown=3\r\n[;;Again]\r\n[c]\r\nHidden2=4\r\n",
        );

        // 长度 ≥2 的普通段名复位注释状态 ⇒ 键装载（负例的对照）。
        let shown = conf_get(&root.cfg(), "bb", "Shown").expect("get");
        assert!(shown.found, "≥2 字符段名必须复位段注释");
        assert_eq!(shown.value.as_deref(), Some("3"));

        // 单字符段名不复位 ⇒ 键仍被段注释挡掉。
        let hidden = conf_get(&root.cfg(), "a", "Hidden").expect("get");
        assert!(!hidden.found, "[;;Disabled] 后的 [a] 不得复位注释状态");

        // 第二次进入段注释后同样如此。
        let hidden2 = conf_get(&root.cfg(), "c", "Hidden2").expect("get");
        assert!(!hidden2.found, "[;;Again] 后的 [c] 不得复位注释状态");

        // 非注释状态下单字符段仍正常装载（复位不是「单字符段全禁」）。
        let loaded = conf_get(&root.cfg(), "z", "Loaded").expect("get");
        assert!(loaded.found, "无段注释时单字符段应正常装载");
        assert_eq!(loaded.value.as_deref(), Some("9"));
    }

    /// F2：回读内容与写前/预期不符 ⇒ 用备份回滚，盘上字节回到写前，报 `readback_mismatch`。
    #[test]
    fn readback_mismatch_rolls_back_from_backup() {
        let root = TempRoot::new("rb-mismatch");
        root.write_conf(&fixture());
        let before_bytes = fs::read(root.conf_path()).expect("read");
        let before_sha = find_conf_sha(&root.path);

        let err = conf_set_with_readback(
            &root.cfg(),
            "Setup",
            "GenMonRate",
            "42",
            false,
            &|_path: &Path| Ok(b"not the conf".to_vec()),
        )
        .unwrap_err();
        assert_eq!(err.code(), "readback_mismatch");
        match &err {
            ConfError::ReadbackMismatch { restored, .. } => assert!(*restored, "必须回滚成功"),
            other => panic!("错误变体不对：{other:?}"),
        }
        assert_eq!(
            fs::read(root.conf_path()).expect("read"),
            before_bytes,
            "回滚后盘上字节必须等于写前"
        );
        assert_eq!(find_conf_sha(&root.path), before_sha);
    }

    /// F2：回读 IO 失败同样走备份回滚（不能把写了一半的文件留在盘上）。
    #[test]
    fn readback_io_error_rolls_back() {
        let root = TempRoot::new("rb-io");
        root.write_conf(&fixture());
        let before_bytes = fs::read(root.conf_path()).expect("read");
        let before_sha = find_conf_sha(&root.path);

        let err = conf_set_with_readback(
            &root.cfg(),
            "Setup",
            "GenMonRate",
            "43",
            false,
            &|_path: &Path| Err(io::Error::other("injected readback failure")),
        )
        .unwrap_err();
        assert_eq!(err.code(), "io_error");
        assert_eq!(
            fs::read(root.conf_path()).expect("read"),
            before_bytes,
            "读失败分支也必须回滚到写前字节"
        );
        assert_eq!(find_conf_sha(&root.path), before_sha);
    }

    fn tree_sha(root: &Path) -> String {
        let mut files = Vec::new();
        collect_files(root, root, &mut files);
        files.sort();
        let mut hasher = Sha256::new();
        for (rel, bytes) in files {
            hasher.update(rel.as_bytes());
            hasher.update([0u8]);
            hasher.update(&bytes);
        }
        hex::encode(hasher.finalize())
    }

    fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_files(root, &path, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.push((rel, fs::read(&path).unwrap()));
            }
        }
    }

    /// 卡 `openmir2-mcp-gm-execute`（T-273）：`[Admin] AdminPort` 的读取口径。
    ///
    /// 缺段/缺键/空值 ⇒ 0（= game-svr 不监听）；显式端口 ⇒ 解析；非法值 ⇒ fail-closed。
    #[test]
    fn admin_port_reads_section_with_zero_default() {
        let root = TempRoot::new("admin-port");
        // 缺 [Admin] 段（文件存在）⇒ 0。
        root.write_conf(b"[Setup]\r\nGenMonRate=10\r\n");
        assert_eq!(admin_port(&root.cfg()).unwrap(), 0);
        // 空值 ⇒ 0（game-svr 同口径）。
        root.write_conf(b"[Setup]\r\nGenMonRate=10\r\n[Admin]\r\nAdminPort=\r\n");
        assert_eq!(admin_port(&root.cfg()).unwrap(), 0);
        // 显式端口（大小写不敏感的段/键）⇒ 解析。
        root.write_conf(b"[admin]\r\nadminport=7946\r\n");
        assert_eq!(admin_port(&root.cfg()).unwrap(), 7946);
        // 非法端口 ⇒ fail-closed。
        root.write_conf(b"[Admin]\r\nAdminPort=70000\r\n");
        let err = admin_port(&root.cfg()).expect_err("越界端口必须报错");
        assert_eq!(err.code(), "invalid_value");
    }

    /// A4：`Drop` 只删**逐字段等于自己**的锁（pid + acq_ms + start + nonce）。
    ///
    /// 负例：他锁内容里恰好含「自己 nonce」的子串（更长的 nonce、或在别的字段里出现），
    /// 子串匹配会误删，精确字段匹配必须保留。
    #[test]
    fn drop_keeps_foreign_lock_that_contains_our_nonce_substring() {
        let root = TempRoot::new("drop-exact");
        let lock = root.path.join("M2GameSvr").join("Server.conf.lock");
        let identity = LockIdentity {
            pid: 111,
            acquired_ms: 222,
            start: Some("333".to_string()),
            nonce: "999-222-7".to_string(),
        };
        let foreign = "pid=888\nacq_ms=777\nstart=666\n\
note=nonce=999-222-7-other\nnonce=999-222-7-other\n";
        fs::write(&lock, foreign).expect("写他锁");
        let before = fs::read(&lock).expect("读他锁");

        drop(ConfLock {
            path: lock.clone(),
            identity,
        });

        assert!(lock.exists(), "内容含相同 nonce 子串的他锁不得被删");
        assert_eq!(fs::read(&lock).expect("读他锁"), before);
    }

    /// 锁体是行式 `key=value`：解析只认字段名精确前缀，`note=nonce=…` 不算 `nonce` 字段。
    #[test]
    fn lock_meta_parses_exact_fields_only() {
        let meta = parse_lock_meta(b"pid=12\nacq_ms=34\nstart=56\nnonce=78\nnote=nonce=78\n");
        assert_eq!(meta.pid, Some(12));
        assert_eq!(meta.acquired_ms, Some(34));
        assert_eq!(meta.start.as_deref(), Some("56"));
        assert_eq!(meta.nonce.as_deref(), Some("78"));
    }

    /// DMTF/CIM 时间（`wmic ... get CreationDate`）→ 1970 epoch 秒。
    #[cfg(windows)]
    #[test]
    fn dmtf_timestamp_parses_to_epoch_seconds() {
        assert_eq!(dmtf_to_epoch_secs("19700101000000.000000+000"), Some(0));
        assert_eq!(dmtf_to_epoch_secs("19700101000101.000000+001"), Some(1));
        assert_eq!(dmtf_to_epoch_secs("19700101000000.123456+000"), Some(0));
        assert_eq!(
            dmtf_to_epoch_secs("20261101154902.883908+480"),
            Some(1_793_519_342)
        );
        assert_eq!(dmtf_to_epoch_secs("not-a-timestamp"), None);
    }
}
