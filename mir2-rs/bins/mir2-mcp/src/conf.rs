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
//! * **拒绝路径与同值 no-op 在取锁之前判定**：`conf_missing` / `unsupported_encoding` /
//!   `key_absent` 以及「值本来就等于请求值」只读预检就返回，不建/删锁、不跑残留清理，也不会
//!   因为别的进程持锁而变成 `conf_lock_timeout`。
//! * 锁文件内容 = `pid=<持有者 pid>` / `acq_ms=<获取时刻 unix 毫秒>` /
//!   `start=<持有者进程启动时刻 token>`（探测不到时省略）/ `nonce=<唯一串>`；
//!   正常结束由 [`ConfLock`] 的 `Drop` 删除（**只删 pid + acq_ms + start + nonce 逐字段
//!   等于自己那份**；不做子串匹配，避免误删内容里恰好含相同 nonce 片段的他锁）。
//! * **残留锁接管规则**（`try_take_over_stale`）：
//!   1. 身份完整的锁：锁文件年龄 ≥ `MIR2_MCP_CONF_LOCK_STALE_MS`（缺省
//!      [`DEFAULT_LOCK_STALE_MS`]；年龄取 `acq_ms`，缺失时退化为文件 mtime）**且**持有者
//!      **确证已死**（pid 不存在，或 pid 被复用——探测到的启动时刻与锁里记录的 `start` 不等）。
//!   2. 空锁/半写锁（`create_new` 之后、pid/acq_ms/nonce 写盘之前崩溃留下的 0 字节或无
//!      `pid`/`nonce` 的文件）没有可比较的身份：只有年龄 ≥
//!      `MIR2_MCP_CONF_LOCK_EMPTY_GRACE_MS`（缺省 [`DEFAULT_EMPTY_LOCK_GRACE_MS`]）才回收。
//!      **宽限内绝不接管**。`create_new` 之后先写不含 `start` 的最小身份，慢探测
//!      （tasklist/wmic/PowerShell，最坏数秒）发生在锁体已是有效身份之后，**不再落进这段
//!      可回收窗口**；宽限判定因此不依赖任何探测结果。
//!   3. 判定采用「pid + start_time」双元组（Windows 用 `tasklist` 判存活 +
//!      `wmic`/PowerShell `Win32_Process.CreationDate` 取启动时刻；Unix 用
//!      `/proc/<pid>/stat` 的 starttime，无 `/proc` 时退化为 `ps -o lstart=`，并固定
//!      `LC_ALL=C` / `TZ=UTC` 保证 token 不随环境漂移）。`/proc` 读失败只有 `NotFound`
//!      （且 `/proc` 本身可用）才判死；EACCES / hidepid / PID namespace / 非 UTF-8 等读错误
//!      一律 `Unknown`。**探测不可用（Unknown）一律保守视为「活」⇒ 不接管**：宁可让一个
//!      超龄锁继续挡住写者直到超时，也不把不确定的锁抢走。
//!   4. 接管用**同目录 rename 抢占**（原子，只有一个进程能成功），并把抢到的文件内容与
//!      判定时读到的字节逐字节比对；不一致说明撞上「刚释放/刚被他人接管」的竞态。
//!      **放回时绝不 rename 覆盖 canonical 上的第三方锁**：先 `create_new` 原子占位，只有
//!      canonical 空闲才成功，再用自己的占位换回搬走的锁；canonical 已被占用则放弃放回，
//!      搬走的这份留作 `.<lock-name>.stale.*` 交给后续清理（宁留残片也不误删活锁）。
//!      ⇒ **活进程的锁永不被抢**，被强杀的进程留下的锁（含空锁）在超过各自阈值后可自愈。
//! * **探测有界且按 pid 缓存**：单次 `tasklist`/`wmic`/PowerShell/`ps` 探测有墙钟上界
//!   （`MIR2_MCP_CONF_PROBE_TIMEOUT_MS`，缺省 [`DEFAULT_PROBE_TIMEOUT_MS`]，超时 = Unknown），
//!   等待循环内同一 pid 只探测一次；deadline 在探测前后各检查一次。
//! * **崩溃残留清理**（`cleanup_residue`，每次成功取锁后顺手执行）：识别同目录的
//!   `.<conf-name>.mir2-mcp.<pid>.<start>.<stamp>.<counter>.tmp` 与
//!   `.<lock-name>.stale.<pid>.<token>.<stamp>.<counter>`（即 `Server.conf.lock.stale.*`），
//!   只删「确证持有者已死 + 年龄 ≥ stale 阈值」的；存活进程持有（启动时刻一致）、探测不可用、
//!   身份不可判定的一律保留。
//! * **释放失败可观测 + 自愈（A1/A2/A4）**：`ConfLock` 的 `Drop` 删除锁文件失败（典型原因：
//!   外部进程以**不含 `FILE_SHARE_DELETE`** 的句柄打开锁文件，Windows 返回 sharing
//!   violation）时不再静默吞掉：失败登记为「本进程自持待释放」记录，并发出结构化诊断
//!   （[`conf_lock_release_diagnostics`] 快照 + stderr 行
//!   `mir2-mcp conf_lock_diagnostic {json}`，字段含 `event`/`lock_path`/`pid`/`start`/
//!   `nonce`/`errno`/`error`）。下一次取锁（同一进程）在进入文件锁等待前先尝试回收该残留：
//!   只有锁体**逐字段等于**记录的身份（pid + acq_ms + start + nonce，即 A4 的「自己持有且
//!   身份可证」路径）才 `remove_file`；身份不可读/不符则绝不动。⇒ 外部句柄一关闭，下一次
//!   `conf_set` 即可在同一调用内自愈成功，不再等满 `MIR2_MCP_CONF_LOCK_TIMEOUT_MS`；
//!   外部句柄仍在时继续按活锁等待并最终 `conf_lock_timeout`（诊断已先行留下）。
//! * **进程内临界区互斥（A3）**：同一进程的多个 MCP 会话按**锁文件路径**用一把 `Mutex`
//!   串行化「取文件锁 → 写 → 释放」整段（同一 `Server.conf` 串行，不同 root 互不阻塞），
//!   等待同样受 timeout 约束；**不**把 pid 当重入凭证（同 pid 的活锁仍按活锁等待，绝不抢占）。
//! * **已知残余 TOCTOU（F2，登记为真机观测项）**：rename 接管法无法原子地「确认没变再搬走」。
//!   极端三路竞态（P 读到旧死锁 → B `create_new` 成功 → P 的 rename 搬走 B 的活锁 → C 在
//!   P 放回前 `create_new` 成功）下，P 按 A1 放弃放回，B 的活锁被 strand 在 `.stale`，
//!   而 B 仍自认为持有 ⇒ 短暂双持有者。按卡面既定选择「宁留残片也不误删活锁」不改接管协议；
//!   真机观测：并发冒烟后若出现 `.stale.*`，核对其身份是否属于仍在运行的进程（命中即该窗口）。
//! * **预检快照可能过期（F4）**：`key_absent` / 同值 no-op 在取锁前用无锁快照判定后直接返回；
//!   若并发在预检后把值改成别的值/新增该键，本次返回的是过期快照且零写。**写路径**一律以
//!   锁内重读为准（并保留锁内同值 no-op 兜底），不会基于过期快照写盘。
//! * `dry_run=true` **不取锁**：它是零写的纯预览（不备份、不落盘），取排他锁会让只读预演
//!   被写者阻塞、甚至在并发时返回新的锁错误码，从而改变既有 dry_run 行为；并发下它最多读到
//!   某一时刻的完整快照（rename 原子替换），不产生持久化副作用。
//! * 测试专用接缝（**只在 `cfg(debug_assertions)` 下生效，release 构建里全是空操作**）：
//!   `MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS` 在 rename 与回读之间睡眠，只用于放大并发窗口
//!   复现互滚；`MIR2_MCP_CONF_TEST_HOLD_BEFORE_TAKEOVER_RENAME_MS` 在接管「读锁之后、rename
//!   之前」睡眠；`MIR2_MCP_CONF_TEST_HOLD_BEFORE_PUTBACK_MS` 在「判定搬走的是替换锁之后、
//!   尝试放回之前」睡眠；`MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE` 在写完临时文件后注入失败；
//!   `MIR2_MCP_CONF_TEST_PROCESS_PROBE` 强制覆盖进程探测（`dead` / `unknown` / `alive` /
//!   `alive:<token>`）；`MIR2_MCP_CONF_TEST_PROBE_LOG` 记录真实探测（证明按 pid 缓存）；
//!   `MIR2_MCP_CONF_TEST_TAKEOVER_RACE_MARKER` 写竞态分支哨兵；
//!   `MIR2_MCP_CONF_TEST_OWN_START_DELAY_MS` 模拟慢的本进程启动时刻探测
//!   （`MIR2_MCP_CONF_TEST_OWN_START_MARKER` 写进入窗口的哨兵）；
//!   `MIR2_MCP_CONF_TEST_CREATE_NEW_DENIED_MS` 模拟 Windows delete-pending 的
//!   `ERROR_ACCESS_DENIED`。生产/release 不设置或设置了都无效果。

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};
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

    // 无锁预检读：`conf_missing` / `unsupported_encoding` / `key_absent` 以及同值 no-op
    // 全部在创建锁文件之前判定（A7：拒绝/只读路径不得建删锁、跑 cleanup，更不得因为别人
    // 持锁而返回 `conf_lock_timeout`）。
    let probe = read_conf(&path)?;
    let probe_scan = scan(&probe)?;
    let probe_entry =
        find_entry(&probe_scan, white.section, white.key).ok_or_else(|| ConfError::KeyAbsent {
            section: white.section.to_string(),
            key: white.key.to_string(),
            path: path.clone(),
        })?;
    let probe_before_sha = sha256_hex(&probe);
    let probe_before_line = line_text(&probe, probe_entry);

    if probe_entry.value == new_value {
        return Ok(SetOutcome {
            wrote: false,
            dry_run,
            changed: false,
            path: path.display().to_string(),
            section: white.section.to_string(),
            key: white.key.to_string(),
            line: probe_entry.line,
            before_value: probe_entry.value.clone(),
            new_value,
            after_sha: probe_before_sha.clone(),
            before_sha: probe_before_sha,
            backup_path: None,
            readback_value: None,
            before_line: probe_before_line.clone(),
            after_line: probe_before_line,
            semantics: white.semantics,
        });
    }

    if dry_run {
        // `dry_run` 不取锁（理由见模块文档「跨进程互斥」末段）。
        let preview = splice_value(&probe, probe_entry, new_value.as_bytes());
        return Ok(SetOutcome {
            wrote: false,
            dry_run: true,
            changed: true,
            path: path.display().to_string(),
            section: white.section.to_string(),
            key: white.key.to_string(),
            line: probe_entry.line,
            before_value: probe_entry.value.clone(),
            new_value,
            before_sha: probe_before_sha,
            after_sha: sha256_hex(&preview),
            backup_path: None,
            readback_value: None,
            before_line: probe_before_line,
            after_line: line_text_after_splice(probe_entry, &preview),
            semantics: white.semantics,
        });
    }

    // 非 dry-run 且确有变更：取锁，锁内重读后再写。
    let opts = lock_options();
    let _lock = ConfLock::acquire(&path)?;
    // 持锁后顺手回收「确证已死 + 超龄」的崩溃残留（不误删活进程持有的临时文件）。
    cleanup_residue(&path, &opts);
    // 备份 / rename / 回读 / 回滚必须基于同一份无竞态的输入。
    let bytes = read_conf(&path)?;
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
    let after_line = line_text_after_splice(entry, &new_bytes);
    let preview_sha = sha256_hex(&new_bytes);

    if entry.value == new_value {
        // 取锁前还不同值、锁内重读已变成同值（别人刚写过）：仍按 no-op 返回，不写盘。
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
            before_line: before_line.clone(),
            after_line: before_line,
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
/// 空/半写锁（无可解析持有者身份）的回收宽限（毫秒）。
///
/// A2/F1：`create_new` 后先写最小身份（pid/acq_ms/nonce）、再补 `start`，所以宽限只兜底
/// 「写最小身份之前崩溃」的微秒级窗口；任何进程探测都发生在锁体已是有效身份之后，
/// **宽限判定不依赖探测结果**（默认 2s 只是崩溃残留兜底，不再需要大于探测上界）。
const DEFAULT_EMPTY_LOCK_GRACE_MS: u64 = 2_000;
/// 单次进程探测命令的墙钟上界（毫秒）；超时按 `Unknown`（保守按活）处理。
const DEFAULT_PROBE_TIMEOUT_MS: u64 = 2_000;
/// 抢锁重试间隔（毫秒）。
const LOCK_RETRY_INTERVAL_MS: u64 = 25;

/// 锁参数（env 覆盖仅供运维/测试，缺省值见常量）。
#[derive(Debug, Clone, Copy)]
struct LockOptions {
    timeout: Duration,
    stale_after: Duration,
    empty_grace: Duration,
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
        empty_grace: Duration::from_millis(env_u64(
            "MIR2_MCP_CONF_LOCK_EMPTY_GRACE_MS",
            DEFAULT_EMPTY_LOCK_GRACE_MS,
        )),
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

/// `Duration` → 毫秒（溢出按 `u64::MAX`）。
fn millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

/// 单次进程探测命令的墙钟上界。
fn probe_timeout() -> Duration {
    Duration::from_millis(env_u64(
        "MIR2_MCP_CONF_PROBE_TIMEOUT_MS",
        DEFAULT_PROBE_TIMEOUT_MS,
    ))
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
    /// 最小可判定身份：pid + acq_ms + nonce（**不做任何探测**）。
    ///
    /// A2/F1：`create_new` 之后必须能立刻写出这三项——它们已构成
    /// [`lock_identity_from_meta`] 认可的完整身份。`start` 是可选字段，探测（tasklist /
    /// wmic / PowerShell，最坏数秒）放到锁体已是有效身份之后追加，绝不落在「空锁可被回收」
    /// 的窗口里。
    fn minimal() -> Self {
        let pid = std::process::id();
        let acquired_ms = now_ms();
        Self {
            pid,
            acquired_ms,
            start: None,
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
    /// 进程内会话互斥（A3）：跨会话串行化「取锁 → 写 → 释放」整段。
    ///
    /// 字段在 `Drop::drop` **之后**才释放，所以 `Drop` 的失败登记仍在该临界区内。
    /// 测试里直接构造 `ConfLock` 时为 `None`（不参与进程内互斥）。
    _process_guard: Option<MutexGuard<'static, ()>>,
}

impl ConfLock {
    /// `create_new` 抢锁，失败有界重试；超时返回 [`ConfError::LockConflict`]。
    ///
    /// A3：先拿**进程内互斥**再抢文件锁；同一进程的多个 MCP 会话因此不会同时进入临界区
    /// （也绝不把「同 pid」当作重入凭证）。
    /// A2/A4：进入等待前先尝试回收「本进程自己持有、上轮 `Drop` 删除失败」的残留锁。
    fn acquire(conf_path: &Path) -> Result<Self, ConfError> {
        let lock_path = lock_path_for(conf_path);
        let opts = lock_options();
        let started = Instant::now();
        // 探测结果按 pid 缓存：等待循环每轮都会做残留判定，但同一次抢锁对同一 pid 只探测一次。
        let mut probes = ProbeCache::default();
        let process_guard = match acquire_process_guard(&lock_path, started, &opts) {
            Ok(guard) => guard,
            Err(elapsed) => {
                return Err(lock_conflict(conf_path, &lock_path, elapsed, &opts));
            }
        };
        // A2/A4：外部句柄可能已经关闭——先自愈自己的残留，再进入常规抢锁/等待。
        let _ = self_heal_pending_release(&lock_path);
        loop {
            // deadline 在探测**之前**先查一次（探测本身可能有墙钟上界）。
            let elapsed = started.elapsed();
            if elapsed >= opts.timeout {
                return Err(lock_conflict(conf_path, &lock_path, elapsed, &opts));
            }
            match create_lock_file(&lock_path) {
                Ok(identity) => {
                    return Ok(Self {
                        path: lock_path,
                        identity,
                        _process_guard: Some(process_guard),
                    })
                }
                Err(source)
                    if source.kind() == io::ErrorKind::AlreadyExists
                        || is_transient_create_new_error(&source) =>
                {
                    // A2：外部句柄若已关闭，自愈会在本轮把本进程的残留删掉，下一次
                    // `create_new` 即成功；仍在占用时继续按活锁等待。
                    let healed = self_heal_pending_release(&lock_path);
                    if !healed {
                        // 先看是否是「持有者确证已死且超过时间阈值」的残留锁（或空锁超过宽限）；
                        // 是则原子接管后立刻重试。
                        let _ = try_take_over_stale(&lock_path, &opts, &mut probes);
                    }
                    // deadline 在探测**之后**再查一次：慢探测不能把等待拖成无界。
                    let elapsed = started.elapsed();
                    if elapsed >= opts.timeout {
                        return Err(lock_conflict(conf_path, &lock_path, elapsed, &opts));
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

/// 构造结构化 `conf_lock_timeout`（持有者 pid 读不到时为 `None`）。
fn lock_conflict(
    conf_path: &Path,
    lock_path: &Path,
    elapsed: Duration,
    opts: &LockOptions,
) -> ConfError {
    let holder_pid = fs::read(lock_path)
        .ok()
        .and_then(|bytes| parse_lock_meta(&bytes).pid);
    ConfError::LockConflict {
        path: conf_path.to_path_buf(),
        lock_path: lock_path.to_path_buf(),
        waited_ms: millis_u64(elapsed),
        timeout_ms: millis_u64(opts.timeout),
        stale_ms: millis_u64(opts.stale_after),
        holder_pid,
    }
}

impl Drop for ConfLock {
    fn drop(&mut self) {
        // 逐字段精确匹配（pid + acq_ms + start + nonce）；只做子串匹配会让「内容里恰好含
        // 相同 nonce 片段」的他锁被误删。
        let current = match fs::read(&self.path) {
            Ok(bytes) => lock_identity_from_meta(&parse_lock_meta(&bytes)),
            // 锁文件已不存在：无需释放，也不算失败。
            Err(source) if source.kind() == io::ErrorKind::NotFound => return,
            // 读不到锁体（例如外部句柄连 `FILE_SHARE_READ` 都不给）：无法逐字段证明是自己的，
            // **绝不盲删**；但要留下可观测信号并登记自愈记录，等身份可读时再回收。
            Err(source) => {
                register_release_failure(&self.path, &self.identity, "release_unverified", &source);
                return;
            }
        };
        if current.as_ref() != Some(&self.identity) {
            return;
        }
        if let Err(source) = fs::remove_file(&self.path) {
            // A1：释放失败**不再静默吞掉**；A2/A4：登记「本进程自持待释放」，
            // 下一次取锁先尝试自愈。
            register_release_failure(&self.path, &self.identity, "release_failed", &source);
        }
    }
}

// ---------------------------------------------------------------------------
// 释放失败可观测 + 自愈（A1 / A2 / A4）
// ---------------------------------------------------------------------------

/// 进程内诊断记录上限（环形丢弃最旧的；避免长跑进程无界增长）。
const MAX_LOCK_DIAGNOSTICS: usize = 32;

/// 一次锁释放诊断（结构化：路径 / 原因 / errno / 持有者身份 / 次数）。
///
/// 由 [`conf_lock_release_diagnostics`] 读取，同时以
/// `mir2-mcp conf_lock_diagnostic {json}` 的 stderr 行发出（生产可观测）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfLockDiagnostic {
    /// `release_failed` / `release_unverified` / `release_recovered` / `release_record_dropped`。
    pub event: &'static str,
    /// 锁文件路径（含被外部句柄占住时的那一份）。
    pub lock_path: String,
    /// 期望持有者 pid（本进程）。
    pub pid: u32,
    /// 期望持有者启动时刻 token（探测不到时为 `None`）。
    pub start: Option<String>,
    /// 期望持有者的唯一 nonce（自持身份可证的最后一环）。
    pub nonce: String,
    /// 失败的 OS errno（Windows sharing violation = 32；无 OS 错误时为 `None`）。
    pub errno: Option<i32>,
    /// OS 错误文本。
    pub error: String,
    /// 事件时刻（unix 毫秒）。
    pub at_ms: u64,
    /// 该残留被尝试回收的累计次数（首次登记为 1）。
    pub attempts: u64,
}

/// 本进程「已登记但还没删掉」的自持锁（按锁路径）。
#[derive(Debug, Clone)]
struct PendingRelease {
    identity: LockIdentity,
    attempts: u64,
    errno: Option<i32>,
    error: String,
}

fn pending_releases() -> &'static Mutex<HashMap<PathBuf, PendingRelease>> {
    static PENDING: OnceLock<Mutex<HashMap<PathBuf, PendingRelease>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn diagnostics_sink() -> &'static Mutex<Vec<ConfLockDiagnostic>> {
    static SINK: OnceLock<Mutex<Vec<ConfLockDiagnostic>>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(Vec::new()))
}

fn emit_diagnostic(diagnostic: ConfLockDiagnostic) {
    if let Ok(line) = serde_json::to_string(&diagnostic) {
        eprintln!("mir2-mcp conf_lock_diagnostic {line}");
    }
    let mut sink = diagnostics_sink()
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if sink.len() >= MAX_LOCK_DIAGNOSTICS {
        sink.remove(0);
    }
    sink.push(diagnostic);
}

/// 读取本进程累计的锁释放诊断（A1/A2/A4 的可观测通道；只读快照）。
///
/// 仅测试可见：生产侧消费者是 stderr 的结构化诊断行；二进制 crate 的 `pub` 不会自动
/// 免于 `dead_code`，所以这里显式按测试编译。
#[cfg(test)]
#[must_use]
pub fn conf_lock_release_diagnostics() -> Vec<ConfLockDiagnostic> {
    diagnostics_sink()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// 登记一次释放失败（结构化诊断 + 自愈记录）。
fn register_release_failure(
    lock_path: &Path,
    identity: &LockIdentity,
    event: &'static str,
    source: &io::Error,
) {
    let attempts = pending_releases()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(lock_path.to_path_buf())
        .and_modify(|pending| {
            if pending.identity != *identity {
                // 正常路径不会发生（身份来自本进程刚持有的锁）；以最新身份为准重登记。
                *pending = PendingRelease {
                    identity: identity.clone(),
                    attempts: 0,
                    errno: None,
                    error: String::new(),
                };
            }
            pending.attempts = pending.attempts.saturating_add(1);
            pending.errno = source.raw_os_error();
            pending.error = source.to_string();
        })
        .or_insert_with(|| PendingRelease {
            identity: identity.clone(),
            attempts: 1,
            errno: source.raw_os_error(),
            error: source.to_string(),
        })
        .attempts;
    emit_diagnostic(ConfLockDiagnostic {
        event,
        lock_path: lock_path.display().to_string(),
        pid: identity.pid,
        start: identity.start.clone(),
        nonce: identity.nonce.clone(),
        errno: source.raw_os_error(),
        error: source.to_string(),
        at_ms: now_ms(),
        attempts,
    });
}

/// 更新自愈记录的最近一次失败（不重复发诊断，避免等待循环每 25ms 刷一行）。
fn note_release_retry(lock_path: &Path, source: &io::Error) {
    if let Some(pending) = pending_releases()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_mut(lock_path)
    {
        pending.attempts = pending.attempts.saturating_add(1);
        pending.errno = source.raw_os_error();
        pending.error = source.to_string();
    }
}

/// 移除自愈记录并发出一次「已处理」诊断（`event` 说明为什么可以撤销记录）。
fn clear_pending_release(lock_path: &Path, event: &'static str, note: &str) {
    let Some(pending) = pending_releases()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(lock_path)
    else {
        return;
    };
    emit_diagnostic(ConfLockDiagnostic {
        event,
        lock_path: lock_path.display().to_string(),
        pid: pending.identity.pid,
        start: pending.identity.start,
        nonce: pending.identity.nonce,
        errno: pending.errno,
        error: note.to_string(),
        at_ms: now_ms(),
        attempts: pending.attempts,
    });
}

/// 尝试自愈「本进程自己持有、上一轮 `Drop` 删除失败」的残留锁（A2/A4）。
///
/// **判定规则（新增的明确路径）：自己持有且身份可证。** 只有锁体当前内容**逐字段等于**
/// 记录在案的身份（pid + acq_ms + start + nonce）才 `remove_file`；读不到 / 内容不符
/// （已被他进程接管或清掉）⇒ 绝不动，只撤销或保留记录。
///
/// 返回 `true` 表示残留已消失（删除成功或确认记录已失效），调用方可立刻重试 `create_new`。
fn self_heal_pending_release(lock_path: &Path) -> bool {
    let Some(pending) = pending_releases()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(lock_path)
        .cloned()
    else {
        return false;
    };
    let bytes = match fs::read(lock_path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            clear_pending_release(lock_path, "release_record_dropped", "锁文件已不存在");
            return true;
        }
        Err(source) => {
            // 外部句柄仍不给读：继续等（身份不可证 ⇒ 绝不盲删）。
            note_release_retry(lock_path, &source);
            return false;
        }
    };
    if lock_identity_from_meta(&parse_lock_meta(&bytes)).as_ref() != Some(&pending.identity) {
        // 锁体已不是我们那份：他进程接管了或已被回收；记录作废，绝不误删。
        clear_pending_release(lock_path, "release_record_dropped", "锁体身份已变化");
        return true;
    }
    match fs::remove_file(lock_path) {
        Ok(()) => {
            clear_pending_release(
                lock_path,
                "release_recovered",
                "外部句柄已释放，残留锁已回收",
            );
            true
        }
        Err(source) => {
            note_release_retry(lock_path, &source);
            false
        }
    }
}

/// 进程内会话互斥（A3）：**按锁文件路径**，同一进程的多个 MCP 会话串行进入
/// 「取文件锁 → 写 → 释放」整段。不同 `Server.conf`（不同 root）互不阻塞。
///
/// 每个不同路径泄漏一把 `'static` Mutex（进程生命周期内路径数极少：生产通常一个；
/// 测试每个 TempRoot 一个）。这样 `MutexGuard<'static, ()>` 才能随 [`ConfLock`] 存活到
/// `Drop` 之后，而无需自引用结构。
///
/// **不按 pid 判重入**：同 pid + 同进程的活锁仍按活锁等待（见 [`holder_state`]）。
fn process_critical_section(lock_path: &Path) -> &'static Mutex<()> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, &'static Mutex<()>>>> = OnceLock::new();
    let mut registry = REGISTRY
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(existing) = registry.get(lock_path) {
        return existing;
    }
    let created: &'static Mutex<()> = Box::leak(Box::new(Mutex::new(())));
    registry.insert(lock_path.to_path_buf(), created);
    created
}

/// 在 `opts.timeout` 内拿到该锁文件的进程内互斥；超时返回已等待时长。
fn acquire_process_guard(
    lock_path: &Path,
    started: Instant,
    opts: &LockOptions,
) -> Result<MutexGuard<'static, ()>, Duration> {
    loop {
        match process_critical_section(lock_path).try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) => {
                let elapsed = started.elapsed();
                if elapsed >= opts.timeout {
                    return Err(elapsed);
                }
                let remaining = opts.timeout.saturating_sub(elapsed);
                std::thread::sleep(Duration::from_millis(LOCK_RETRY_INTERVAL_MS).min(remaining));
            }
        }
    }
}
/// 原子创建锁文件并写入持有者元数据。
///
/// A2/F1：`create_new` 之后**立刻**写 pid/acq_ms/nonce（不含 `start`），空锁窗口只剩这一次
/// 小写入（微秒级，且不再包含任何进程探测）。`start` 在锁体已是有效身份之后追加（见
/// [`complete_lock_identity`]），因此即使 tasklist / wmic / PowerShell 很慢（各自有墙钟
/// 上界），也不会让活持有者的锁被并发方当成「0 字节崩溃残留」回收。
fn create_lock_file(lock_path: &Path) -> io::Result<LockIdentity> {
    let mut file = open_lock_file(lock_path)?;
    let identity = LockIdentity::minimal();
    if let Err(source) = file.write_all(identity.to_body().as_bytes()) {
        drop(file);
        let _ = fs::remove_file(lock_path);
        return Err(source);
    }
    let _ = file.sync_all();
    drop(file);
    Ok(complete_lock_identity(lock_path, identity))
}

/// 把可选的 `start=` 追加到锁体（A2/F1）。
///
/// 追加时锁体已经含 pid/acq_ms/nonce，是 [`lock_identity_from_meta`] 认可的有效身份：
/// 并发方最多判 `Alive`（缺 `start` ⇒ 保守按活），绝不会把这段慢探测当作空锁回收。
/// 探测/追加失败时保留最小身份即可（pid 复用判定退化为「缺 start ⇒ 按活」，安全）。
fn complete_lock_identity(lock_path: &Path, mut identity: LockIdentity) -> LockIdentity {
    let Some(start) = own_process_start() else {
        return identity;
    };
    let appended = fs::OpenOptions::new()
        .append(true)
        .open(lock_path)
        .and_then(|mut file| {
            writeln!(file, "start={start}")?;
            let _ = file.sync_all();
            Ok(())
        });
    // 以盘上最终内容为准（追加可能部分成功），让 `Drop` 的逐字段比较与磁盘一致。
    if appended.is_ok() {
        identity.start = Some(start);
    }
    if let Ok(bytes) = fs::read(lock_path) {
        if let Some(parsed) = lock_identity_from_meta(&parse_lock_meta(&bytes)) {
            if parsed.pid == identity.pid && parsed.nonce == identity.nonce {
                identity = parsed;
            }
        }
    }
    identity
}

/// `create_new` 抢锁文件。
///
/// Windows 上「另一个进程刚删掉锁文件、句柄尚未全部关闭」的 delete-pending 窗口里，
/// `CREATE_NEW` 会返回 `ERROR_ACCESS_DENIED` 而不是 `AlreadyExists`。这是纯瞬时竞态，
/// 做少量有界重试后**交给外层重试循环**（[`ConfLock::acquire`] 把
/// [`is_transient_create_new_error`] 也当作可重试，直到 timeout）；真正的权限问题会一直重试
/// 到 timeout 并返回 `conf_lock_timeout`，而不是直接 `io_error`。错误码语义不变。
fn open_lock_file(lock_path: &Path) -> io::Result<fs::File> {
    let mut last = None;
    for attempt in 0..4 {
        if test_create_new_denied() {
            // 测试接缝：模拟 delete-pending 的 `ERROR_ACCESS_DENIED`（debug-only）。
            return Err(io::Error::from_raw_os_error(5));
        }
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

/// 一次抢锁/清理调用内的进程探测缓存（按 pid）。
#[derive(Debug, Default)]
struct ProbeCache {
    by_pid: HashMap<u32, ProcessProbe>,
}

impl ProbeCache {
    fn probe(&mut self, pid: u32) -> ProcessProbe {
        if let Some(probe) = self.by_pid.get(&pid) {
            return probe.clone();
        }
        let probe = probe_process(pid);
        self.by_pid.insert(pid, probe.clone());
        probe
    }
}

/// 「pid + 进程启动时刻」双元组判定持有者是否还活着。
///
/// * `pid` 不存在 ⇒ `Dead`；
/// * `pid` 存在但启动时刻与锁里记录的不一致 ⇒ pid 已被复用 ⇒ `Dead`；
/// * 探测不可用、`pid` 缺失、任一侧缺启动时刻 ⇒ `Unknown`（调用方按「活」处理，绝不接管）；
/// * 其余 ⇒ `Alive`。
fn holder_state(meta: &LockMeta, probes: &mut ProbeCache) -> HolderState {
    let Some(pid) = meta.pid else {
        return HolderState::Unknown;
    };
    match probes.probe(pid) {
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
/// 空/半写锁没有身份，改用短宽限判定（A2）。
///
/// 接管用同目录 `rename` 抢占（同一目录内 rename 是原子的，多个接管者只有一个成功），
/// 抢到后把文件内容与判定时读到的字节逐字节比对：不一致说明撞上「持有者刚释放 / 刚被他人
/// 接管」的竞态。此时**不无条件删除**（搬走的可能是新持有者的活锁）：按搬走文件自己的身份
/// 重新判定，确证已死且超龄才删；否则放回原处——**放回绝不覆盖 canonical 上已被第三方占用
/// 的锁**（A1）：先 `create_new` 原子占位，只有 canonical 空闲才成功，再用自己的占位换回；
/// canonical 被占用则放弃放回，留作 `.stale.*` 交给后续 [`cleanup_residue`]。
/// 返回 `true` 表示「残留已被移走，可以立刻重新 `create_new`」。
fn try_take_over_stale(lock_path: &Path, opts: &LockOptions, probes: &mut ProbeCache) -> bool {
    let Ok(bytes) = fs::read(lock_path) else {
        return false;
    };
    let meta = parse_lock_meta(&bytes);
    let Some(age_ms) = lock_age_ms(lock_path, &meta) else {
        return false;
    };
    let stale_ms = millis_u64(opts.stale_after);
    let grace_ms = millis_u64(opts.empty_grace);
    let reclaimable = if lock_identity_from_meta(&meta).is_none() {
        // 空锁/半写锁：没有可比较的持有者身份。宽限内绝不回收（可能是刚 create_new、
        // 正在写元数据的活锁）；超过短宽限仍为空 ⇒ 崩溃残留。
        age_ms >= grace_ms
    } else {
        // 时间阈值 + 持有者确证已死，二者同时成立才接管；Unknown 保守按活（短路避免探测）。
        age_ms >= stale_ms && holder_state(&meta, probes) == HolderState::Dead
    };
    if !reclaimable {
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
    let moved_dead = lock_identity_from_meta(&moved_meta).is_some()
        && holder_state(&moved_meta, probes) == HolderState::Dead
        && lock_age_ms(&stale_path, &moved_meta).is_some_and(|age| age >= stale_ms);
    if moved_dead {
        let _ = fs::remove_file(&stale_path);
        return true;
    }

    // 放回：canonical 可能已被第三方活锁占用，**绝不 rename 覆盖**。先 `create_new` 占位，
    // 只有 canonical 空闲才成功；成功后才用自己的占位换回搬走/新写的那份。
    test_race_marker("putback");
    test_hold_before_putback();
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(placeholder) => {
            drop(placeholder);
            if fs::rename(&stale_path, lock_path).is_ok() {
                test_race_marker("putback-restored");
            } else {
                // 自己的占位 rename 失败：删掉空占位，stale 留给 cleanup_residue。
                let _ = fs::remove_file(lock_path);
                test_race_marker("putback-failed");
            }
        }
        Err(_) => {
            // canonical 已被占用：放弃放回，搬走的这份留作 `.stale.*` 交给清理。
            test_race_marker("putback-abandoned");
        }
    }
    false
}

/// 清理崩溃残留：`.<conf-name>.mir2-mcp.<pid>.<start>.<stamp>.<counter>.tmp` 与
/// `.<lock-name>.stale.<pid>.<token>.<stamp>.<counter>`（锁文件名带 `.lock`，即
/// `Server.conf.lock.stale.*`）。
///
/// 只删「确证持有者已死（pid 不存在或 pid 被复用）+ 年龄 ≥ stale 阈值」的残留；存活进程
/// 持有（启动时刻一致）、探测不可用、身份不可判定的一律保留（宁留残片也不误删正在用的文件）。
/// 调用方在**已持锁**时调用，且失败一律吞掉——清理绝不改变 `conf_set` 的成败与错误码。
fn cleanup_residue(conf_path: &Path, opts: &LockOptions) {
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
    // A6：真实残留名基于**锁文件名**（`stale_path` 由 `<锁名>.stale.*` 构造）。
    let stale_prefix = format!(".{base}.stale.");
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut probes = ProbeCache::default();
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
            if residue_reclaimable(Some(pid), start.as_deref(), age, opts, &mut probes) {
                let _ = fs::remove_file(&path);
            }
        } else if let Some(rest) = name.strip_prefix(&stale_prefix) {
            let (pid, start, age) = stale_residue_identity(&path, rest);
            if residue_reclaimable(pid, start.as_deref(), age, opts, &mut probes) {
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
    opts: &LockOptions,
    probes: &mut ProbeCache,
) -> bool {
    let Some(pid) = pid else {
        return false;
    };
    let Some(age_ms) = age_ms else {
        return false;
    };
    if age_ms < millis_u64(opts.stale_after) {
        return false;
    }
    match probes.probe(pid) {
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
    /// 无法判定（探测工具不可用/输出不可解析/探测超时）；调用方**保守视为活**。
    Unknown,
}

// ---------------------------------------------------------------------------
// 测试专用接缝：全部只在 `cfg(debug_assertions)` 下生效（release 构建里是空操作）。
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
fn test_env_u64(name: &str, default: u64) -> u64 {
    env_u64(name, default)
}

#[cfg(not(debug_assertions))]
fn test_env_u64(_name: &str, default: u64) -> u64 {
    default
}

#[cfg(debug_assertions)]
fn env_flag(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        let value = value.trim();
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

#[cfg(debug_assertions)]
fn test_env_flag(name: &str) -> bool {
    env_flag(name)
}

#[cfg(not(debug_assertions))]
fn test_env_flag(_name: &str) -> bool {
    false
}

/// 测试专用接缝：强制覆盖进程探测结果（生产不设置 ⇒ `None`，走真实探测）。
///
/// 取值：`dead` / `unknown` / `alive`（启动时刻未知）/ `alive:<token>`。
#[cfg(debug_assertions)]
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

#[cfg(not(debug_assertions))]
fn test_probe_override() -> Option<ProcessProbe> {
    None
}

/// 测试专用接缝：记录一次真实进程探测（证明等待循环按 pid 缓存）。
#[cfg(debug_assertions)]
fn test_probe_log(pid: u32) {
    if let Ok(path) = std::env::var("MIR2_MCP_CONF_TEST_PROBE_LOG") {
        if !path.trim().is_empty() {
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| writeln!(f, "{pid}"));
        }
    }
}

#[cfg(not(debug_assertions))]
fn test_probe_log(_pid: u32) {}

/// 测试专用接缝：把竞态分支的执行证据追加到 marker 文件（哨兵，证明用例确实走到该分支）。
#[cfg(debug_assertions)]
fn test_race_marker(line: &str) {
    if let Ok(path) = std::env::var("MIR2_MCP_CONF_TEST_TAKEOVER_RACE_MARKER") {
        if !path.trim().is_empty() {
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| writeln!(f, "{line}"));
        }
    }
}

#[cfg(not(debug_assertions))]
fn test_race_marker(_line: &str) {}

/// 测试专用接缝：模拟一次慢的「本进程启动时刻」探测（debug-only）。
/// 进入时先写 `own-start-delay` 哨兵，供用例按事件启动第二个写者。
#[cfg(debug_assertions)]
fn test_own_start_delay() {
    let ms = test_env_u64("MIR2_MCP_CONF_TEST_OWN_START_DELAY_MS", 0);
    if ms > 0 {
        test_own_start_marker("own-start-delay");
        std::thread::sleep(Duration::from_millis(ms));
    }
}

#[cfg(not(debug_assertions))]
fn test_own_start_delay() {}

/// 测试专用接缝：慢启动探测的哨兵文件。
#[cfg(debug_assertions)]
fn test_own_start_marker(line: &str) {
    if let Ok(path) = std::env::var("MIR2_MCP_CONF_TEST_OWN_START_MARKER") {
        if !path.trim().is_empty() {
            let _ = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| writeln!(f, "{line}"));
        }
    }
}

/// 测试专用接缝：模拟 Windows delete-pending 的 `ERROR_ACCESS_DENIED`（前 `MS` 毫秒）。
#[cfg(all(windows, debug_assertions))]
fn test_create_new_denied() -> bool {
    let ms = test_env_u64("MIR2_MCP_CONF_TEST_CREATE_NEW_DENIED_MS", 0);
    if ms == 0 {
        return false;
    }
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed() < Duration::from_millis(ms)
}

#[cfg(not(all(windows, debug_assertions)))]
fn test_create_new_denied() -> bool {
    false
}

/// 测试专用接缝：放大「rename 之后、回读之前」的窗口（生产不设置 ⇒ 空操作）。
fn test_hold_after_rename() {
    let ms = test_env_u64("MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS", 0);
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// 测试专用接缝：放大 `try_take_over_stale` 里「读锁之后、rename 之前」的窗口。
/// 进入时先写 `hold-start` 哨兵，让用例可以按事件（而不是固定 sleep）安排竞态。
fn test_hold_before_takeover_rename() {
    test_race_marker("hold-start");
    let ms = test_env_u64("MIR2_MCP_CONF_TEST_HOLD_BEFORE_TAKEOVER_RENAME_MS", 0);
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// 测试专用接缝：放大「判定搬走的是替换锁之后、尝试放回之前」的窗口。
fn test_hold_before_putback() {
    let ms = test_env_u64("MIR2_MCP_CONF_TEST_HOLD_BEFORE_PUTBACK_MS", 0);
    if ms > 0 {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// 进程存活 + 启动时刻探测（Windows 用 `tasklist` + `wmic`/PowerShell；
/// Unix 用 `/proc/<pid>/stat`，无 `/proc` 时退化为 `ps -o lstart=`）。
fn probe_process(pid: u32) -> ProcessProbe {
    if let Some(forced) = test_probe_override() {
        return forced;
    }
    test_probe_log(pid);
    probe_process_impl(pid)
}

/// 本进程的启动时刻 token；只探测一次并缓存（探测失败缓存 `None`，即无法做 pid 复用判定）。
fn own_process_start() -> Option<String> {
    static OWN_START: OnceLock<Option<String>> = OnceLock::new();
    OWN_START
        .get_or_init(|| {
            // 测试接缝：模拟慢探测（debug-only）。真实路径里这段探测发生在锁体已写出有效
            // 身份之后，因此慢探测不会把活锁暴露成「空锁」。
            test_own_start_delay();
            match probe_process(std::process::id()) {
                ProcessProbe::Alive { start } => start,
                ProcessProbe::Dead | ProcessProbe::Unknown => None,
            }
        })
        .clone()
}

/// 有界执行探测命令：超过 [`probe_timeout`] 直接 kill 并按 `Unknown` 处理（调用方保守按活）。
///
/// 命令本身不做复杂输出流重定向，只读自身 stdout；探测进程不会留下后台孙进程，因此
/// `wait_with_output` 不会因为孙进程持有 stdout 而无界等待（这一点与 memory
/// `subprocess-timeout-does-not-kill-tree` 的场景不同，命令集合固定且无派生）。
fn run_probe_command(mut command: std::process::Command) -> Option<std::process::Output> {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().ok()?;
    let deadline = Instant::now() + probe_timeout();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    child.wait_with_output().ok()
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
/// 命令起不来/输出不可解析/超时 ⇒ `None`（无法判定），由调用方保守处理。
#[cfg(windows)]
fn tasklist_process_alive(pid: u32) -> Option<bool> {
    let mut command = std::process::Command::new("tasklist");
    command.args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"]);
    let output = run_probe_command(command)?;
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
    let mut command = std::process::Command::new("wmic");
    command.args([
        "process",
        "where",
        &format!("ProcessId={pid}"),
        "get",
        "CreationDate",
        "/value",
    ]);
    let output = run_probe_command(command)?;
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
    let mut command = std::process::Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    let output = run_probe_command(command)?;
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

/// `/proc/<pid>/stat` 的探测结果：可继续按结果判定，或退化到 `ps`。
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Debug)]
enum ProcStatOutcome {
    Probe(ProcessProbe),
    FallbackToPs,
}

/// `/proc` 路径的探测核心（读函数与 `/proc` 可用性可注入，便于跨平台单测错误分类）。
///
/// A3：只有 `NotFound`（且 `/proc` 本身可用）才可能是「进程不存在」。EACCES / hidepid /
/// PID namespace / 非 UTF-8 等一律 `Unknown`——绝不能让读失败变成 `Dead` 去抢活锁。
#[cfg_attr(not(unix), allow(dead_code))]
fn probe_proc_stat_with<F>(pid: u32, proc_available: bool, read_stat: F) -> ProcStatOutcome
where
    F: FnOnce(u32) -> io::Result<String>,
{
    match read_stat(pid) {
        Ok(stat) => ProcStatOutcome::Probe(parse_proc_stat(&stat)),
        Err(source) => match source.kind() {
            io::ErrorKind::NotFound if proc_available => ProcStatOutcome::Probe(ProcessProbe::Dead),
            io::ErrorKind::NotFound => ProcStatOutcome::FallbackToPs,
            _ => ProcStatOutcome::Probe(ProcessProbe::Unknown),
        },
    }
}

/// `/proc/<pid>/stat` → 启动时刻 token。
///
/// `comm`（字段 2）可能含空格/右括号，所以从最后一个 `')'` 之后切：切出的 fields[0] 是
/// state（总字段 3），starttime 是总字段 22 ⇒ fields[19]。
#[cfg_attr(not(unix), allow(dead_code))]
fn parse_proc_stat(stat: &str) -> ProcessProbe {
    let Some(pos) = stat.rfind(')') else {
        return ProcessProbe::Unknown;
    };
    match stat[pos + 1..]
        .split_whitespace()
        .nth(19)
        .and_then(|v| v.parse::<u64>().ok())
    {
        Some(ticks) => ProcessProbe::Alive {
            start: Some(ticks.to_string()),
        },
        None => ProcessProbe::Unknown,
    }
}

#[cfg(unix)]
fn probe_process_impl(pid: u32) -> ProcessProbe {
    let proc_available = Path::new("/proc/self").exists();
    match probe_proc_stat_with(pid, proc_available, |p| {
        fs::read_to_string(format!("/proc/{p}/stat"))
    }) {
        ProcStatOutcome::Probe(state) => state,
        ProcStatOutcome::FallbackToPs => ps_lstart_probe(pid),
    }
}

/// 非 /proc Unix 的 `ps -o lstart=` 兜底。
///
/// `lstart` 是本地化文本，受 `LC_ALL` / `TZ` 影响；这里固定 `LC_ALL=C` 与 `TZ=UTC`，
/// 保证同一个活进程在任何宿主环境下产出同一个 token（A11），不会因环境差异被误判 `Dead`。
#[cfg(unix)]
fn ps_lstart_probe(pid: u32) -> ProcessProbe {
    let mut command = std::process::Command::new("ps");
    command.args(["-o", "lstart=", "-p", &pid.to_string()]);
    for (key, value) in ps_lstart_env() {
        command.env(key, value);
    }
    match run_probe_command(command) {
        Some(output) if output.status.success() => {
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

/// 非 /proc Unix 的 `ps` 调用必须固定的环境：`LC_ALL=C` + `TZ=UTC`（A11）。
#[cfg_attr(not(unix), allow(dead_code))]
fn ps_lstart_env() -> [(&'static str, &'static str); 2] {
    [("LC_ALL", "C"), ("TZ", "UTC")]
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

/// A4：`after_line` 必须按**拼接后**的缓冲区重新定位行边界。
///
/// 拼接点之前的字节（含 `line_start`）不变，所以行起点可复用；行尾必须在 `spliced` 里重新
/// 找下一个 `\n`（值增长/缩短都会移动行尾，末行无换行时缩短更会让拼接前的 `line_end` 越界）。
fn line_text_after_splice(entry: &Entry, spliced: &[u8]) -> String {
    let start = entry.line_start;
    let raw_end = spliced[start..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(spliced.len(), |p| start + p);
    let (s, e) = trim_range(spliced, start, raw_end);
    String::from_utf8_lossy(&spliced[s..e]).into_owned()
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
    if test_env_flag("MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE") {
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

    /// 外部句柄（Windows：不含 `FILE_SHARE_DELETE`）打开锁文件，模拟第三方占用。
    #[cfg(windows)]
    fn open_without_share_delete(path: &Path) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ(0x1) | FILE_SHARE_WRITE(0x2)：**不含** FILE_SHARE_DELETE(0x4)。
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(path)
            .expect("打开外部占住句柄")
    }

    /// A1/A2/A4：释放失败必须可观测；外部句柄关闭后下一次 conf_set 自愈自己的残留。
    #[cfg(windows)]
    #[test]
    fn release_failure_is_observable_and_next_set_self_heals() {
        let root = TempRoot::new("release-heal");
        root.write_conf(&fixture());
        let conf = root.conf_path();
        let lock_path = conf.with_file_name("Server.conf.lock");

        let live = ConfLock::acquire(&conf).expect("先取锁");
        let identity = live.identity.clone();
        let foreign = open_without_share_delete(&lock_path);
        drop(live);
        assert!(lock_path.exists(), "外部句柄占住时删除必须失败（锁残留）");

        // A1：结构化诊断（路径 / errno / 身份）必须留下，不得静默吞。
        let path_text = lock_path.display().to_string();
        let diags = conf_lock_release_diagnostics();
        let failed = diags
            .iter()
            .find(|d| d.event == "release_failed" && d.lock_path == path_text)
            .unwrap_or_else(|| panic!("缺 release_failed 诊断：{diags:?}"));
        assert_eq!(failed.pid, std::process::id());
        assert_eq!(failed.nonce, identity.nonce);
        assert_eq!(failed.errno, Some(32), "Windows sharing violation = 32");
        assert!(!failed.error.is_empty());

        // A4：残留锁的身份逐字段等于「本进程自己那份」（自愈路径的前提）。
        let residue = lock_identity_from_meta(&parse_lock_meta(&fs::read(&lock_path).unwrap()));
        assert_eq!(residue.as_ref(), Some(&identity));

        // A2：句柄关闭后，下一次 conf_set 必须自愈成功（无自愈路径会等满 timeout 后报错）。
        drop(foreign);
        let out = conf_set(&root.cfg(), "Setup", "GenMonRate", "44", false)
            .expect("外部句柄关闭后下一次 conf_set 必须自愈成功");
        assert!(out.wrote);
        assert!(!lock_path.exists(), "自愈后锁必须释放");

        let recovered = conf_lock_release_diagnostics()
            .into_iter()
            .any(|d| d.event == "release_recovered" && d.lock_path == path_text);
        assert!(recovered, "自愈路径必须留下 release_recovered 诊断");
    }

    /// A3：同进程两个会话 + 外部句柄 ⇒ 不得双持有者；同 pid 的活锁不得被当重入抢走。
    #[cfg(windows)]
    #[test]
    fn same_process_concurrent_session_never_double_holds() {
        let root = TempRoot::new("a3-no-double");
        root.write_conf(&fixture());
        let conf = root.conf_path();
        let lock_path = conf.with_file_name("Server.conf.lock");

        let live = ConfLock::acquire(&conf).expect("会话 A 取锁");
        let identity = live.identity.clone();
        let foreign = open_without_share_delete(&lock_path);

        // 会话 B（同进程）：A 仍持锁 ⇒ 必须超时，绝不因「同 pid」直接进临界区。
        let cfg = root.cfg();
        let err = conf_set(&cfg, "Setup", "GenMonRate", "45", false)
            .expect_err("会话 B 在 A 持锁期间必须超时");
        assert_eq!(err.code(), "conf_lock_timeout");
        let current = lock_identity_from_meta(&parse_lock_meta(&fs::read(&lock_path).unwrap()));
        assert_eq!(current.as_ref(), Some(&identity), "B 不得抢走/改写 A 的锁");

        drop(live); // 删除失败（外部句柄）⇒ 登记自愈
        drop(foreign);
        let out = conf_set(&cfg, "Setup", "GenMonRate", "46", false).expect("句柄关闭后自愈");
        assert!(out.wrote);
        assert!(!lock_path.exists());
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
            _process_guard: None,
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

    /// A4：`after_line` 按拼接后缓冲区重新定位——值增长、缩短、末行无换行缩短都必须
    /// 返回新行原文，且缩短用例不得 panic。
    #[test]
    fn after_line_relocates_after_splice_length_changes() {
        // 增长：10 → 100
        let root = TempRoot::new("after-grow");
        root.write_conf(&fixture());
        let out = conf_set(&root.cfg(), "Setup", "GenMonRate", "100", false).expect("grow");
        assert_eq!(out.before_line, "GenMonRate=10 ;;旧注释");
        assert_eq!(out.after_line, "GenMonRate=100 ;;旧注释");
        let text = String::from_utf8_lossy(&fs::read(root.conf_path()).unwrap()).into_owned();
        assert!(text.contains("GenMonRate=100 ;;旧注释"), "同行注释不得错位");
        assert!(text.contains("MonsterPowerRate=10"), "其它行不得被切到");

        // 缩短：100 → 1
        let root = TempRoot::new("after-shrink");
        root.write_conf(b"\xEF\xBB\xBF[Setup]\r\nMonsterPowerRate=100\r\nGenMonRate=10\r\n");
        let out = conf_set(&root.cfg(), "Setup", "MonsterPowerRate", "1", false).expect("shrink");
        assert_eq!(out.before_line, "MonsterPowerRate=100");
        assert_eq!(out.after_line, "MonsterPowerRate=1");

        // 末行无换行且缩短：拼接前的 line_end 会越过拼接后的缓冲区，必须不 panic、取新行。
        let root = TempRoot::new("after-last-shrink");
        root.write_conf(b"\xEF\xBB\xBF[Setup]\r\nMonsterPowerRate=100");
        let out = conf_set(&root.cfg(), "Setup", "MonsterPowerRate", "1", false)
            .expect("last-line shrink");
        assert_eq!(out.after_line, "MonsterPowerRate=1");
        assert_eq!(
            fs::read(root.conf_path()).unwrap(),
            b"\xEF\xBB\xBF[Setup]\r\nMonsterPowerRate=1".to_vec()
        );
    }

    /// A3：`/proc/<pid>/stat` 读失败的错误分类——只有 NotFound（且 /proc 可用）判 Dead，
    /// EACCES/hidepid/非 UTF-8 等一律 Unknown（绝不据此抢活锁）。
    #[test]
    fn proc_stat_read_errors_only_notfound_is_dead() {
        let outcome = probe_proc_stat_with(42, true, |_| {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert!(
            matches!(outcome, ProcStatOutcome::Probe(ProcessProbe::Unknown)),
            "EACCES 必须 Unknown，得到 {outcome:?}"
        );

        let outcome = probe_proc_stat_with(42, true, |_| {
            Err(io::Error::from(io::ErrorKind::InvalidData))
        });
        assert!(
            matches!(outcome, ProcStatOutcome::Probe(ProcessProbe::Unknown)),
            "非 UTF-8/无效数据必须 Unknown，得到 {outcome:?}"
        );

        let outcome =
            probe_proc_stat_with(42, true, |_| Err(io::Error::from(io::ErrorKind::NotFound)));
        assert!(
            matches!(outcome, ProcStatOutcome::Probe(ProcessProbe::Dead)),
            "/proc 可用且 NotFound 才能判 Dead，得到 {outcome:?}"
        );

        let outcome =
            probe_proc_stat_with(42, false, |_| Err(io::Error::from(io::ErrorKind::NotFound)));
        assert!(
            matches!(outcome, ProcStatOutcome::FallbackToPs),
            "/proc 不可用时必须退化 ps，得到 {outcome:?}"
        );
    }

    /// A3：`/proc/<pid>/stat` 解析取第 22 字段（`)` 之后的第 20 个，index 19）。
    #[test]
    fn proc_stat_parses_starttime_field() {
        let mut stat = String::from("123 (comm with ) paren) S");
        for _ in 0..18 {
            stat.push_str(" 0");
        }
        stat.push_str(" 777 1 2 3");
        assert_eq!(
            parse_proc_stat(&stat),
            ProcessProbe::Alive {
                start: Some("777".to_string())
            }
        );
        assert_eq!(parse_proc_stat("no closing paren"), ProcessProbe::Unknown);
        assert_eq!(parse_proc_stat("1 (x) S 0"), ProcessProbe::Unknown);
    }

    /// A11：非 /proc Unix 的 `ps` 调用固定 `LC_ALL=C` / `TZ=UTC`，token 不随宿主环境漂移。
    #[test]
    fn ps_fallback_pins_locale_and_timezone() {
        assert_eq!(ps_lstart_env(), [("LC_ALL", "C"), ("TZ", "UTC")]);
    }

    /// A11（Windows 路径）：DMTF 时间带 offset，同一时刻的不同 offset 表示归一到同一 epoch token。
    #[cfg(windows)]
    #[test]
    fn dmtf_token_is_timezone_environment_stable() {
        assert_eq!(
            dmtf_to_epoch_secs("20261101154902.000000+480"),
            dmtf_to_epoch_secs("20261101074902.000000+000"),
            "UTC+8 的 15:49:02 与 UTC 的 07:49:02 是同一时刻"
        );
        assert_eq!(
            dmtf_to_epoch_secs("20261101154902.000000+480"),
            dmtf_to_epoch_secs("20261101084902.000000+060"),
            "UTC+6 的 08:49:02 与 UTC 的 07:49:02 是同一时刻"
        );
    }

    /// A9：release 构建下所有 `MIR2_MCP_CONF_TEST_*` 接缝必须完全无效果。
    ///
    /// 只在 release 编译：用 `cargo test --release -p mir2-mcp --lib test_seams_are_inert_in_release` 跑。
    #[cfg(not(debug_assertions))]
    #[test]
    fn test_seams_are_inert_in_release() {
        std::env::set_var("MIR2_MCP_CONF_TEST_PROCESS_PROBE", "dead");
        assert_eq!(test_probe_override(), None, "release 下探测覆盖不得生效");
        std::env::set_var("MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE", "1");
        assert!(
            !test_env_flag("MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE"),
            "release 下注入失败接缝不得生效"
        );
        assert_eq!(
            test_env_u64("MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS", 0),
            0,
            "release 下睡眠接缝不得生效"
        );
        assert_eq!(
            test_env_u64("MIR2_MCP_CONF_TEST_OWN_START_DELAY_MS", 7),
            7,
            "release 下慢探测接缝不得生效"
        );
        assert!(
            !test_create_new_denied(),
            "release 下 delete-pending 接缝不得生效"
        );
        std::env::remove_var("MIR2_MCP_CONF_TEST_PROCESS_PROBE");
        std::env::remove_var("MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE");
    }
}
