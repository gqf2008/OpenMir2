//! 测试专用**观测缝**（卡 `openmir2-test-attack-log`）：`MIR2_LOG_ATTACKS`。
//!
//! 为什么需要：服务端收攻击帧**原本没有任何日志**（`attackwire.rs` 与 combat 路径零 `glog`），
//! 真机验证「骑乘中仍能攻击」时，客户端侧的 `[攻击] 发送攻击帧` 只能证明**客户端发了**，
//! 证明不了**服务端收到了并进入攻击路径**。本缝在受理点补一行可判据日志。
//!
//! 口径（与 `MIR2_TEST_PERMISSION` / `MIR2_RNG_SEED` / `MIR2_CLOCK_MS` 同族）：
//! - **默认关**：不设/空串/`0` ⇒ 不打日志，行为与改动前一字不差；
//! - **启动即固定**：只在启动读一次（`OnceLock`），运行中不可改；
//! - **fail-closed**：只接受 `0`/`1`，其余（含 `true`、`2`、`-1`、带空白）⇒ 启动失败。

/// 环境变量名。
pub const LOG_ATTACKS_ENV: &str = "MIR2_LOG_ATTACKS";

static LOG_ATTACKS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// 解析取值：`None`/空串/`0` ⇒ `false`；`1` ⇒ `true`；其余 ⇒ 错误（**不 trim**，避免假注入）。
fn parse_log_attacks(raw: Option<&str>) -> Result<bool, String> {
    match raw {
        None | Some("") | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(s) => Err(format!(
            "{LOG_ATTACKS_ENV} 非法：{s:?}（只接受 0/1，空串=关闭）"
        )),
    }
}

/// 安装（进程内一次性）。
///
/// # Errors
/// 取值非法，或已安装过。
pub fn install_log_attacks(raw: Option<&str>) -> Result<bool, String> {
    let v = parse_log_attacks(raw)?;
    LOG_ATTACKS
        .set(v)
        .map_err(|_| format!("{LOG_ATTACKS_ENV} 只能安装一次"))?;
    Ok(v)
}

/// 从进程环境读取并安装（`run_inner` 唯一入口）。
///
/// 用 `var_os` + `to_str`：非 UTF-8 值必须判错，不能与「未设」混同（与同族缝一致）。
///
/// # Errors
/// 取值非法、含非 UTF-8 字节，或已安装过。
pub fn install_log_attacks_from_env() -> Result<bool, String> {
    match std::env::var_os(LOG_ATTACKS_ENV) {
        None => install_log_attacks(None),
        Some(os) => match os.to_str() {
            Some(s) => install_log_attacks(Some(s)),
            None => Err(format!(
                "{LOG_ATTACKS_ENV} 含非 UTF-8 字节（拒绝静默当未设）"
            )),
        },
    }
}

/// 是否记录攻击受理日志（未安装 ⇒ `false`）。
#[must_use]
pub fn attack_log_enabled() -> bool {
    *LOG_ATTACKS.get().unwrap_or(&false)
}

/// **卡 `openmir2-ids-keepalive-timeout`**：`MIR2_LOG_IDS`——LoginSrv 链路的观测缝。
///
/// 背景：旧实例 2h45m 内 LoginSrv 判了 119 次「游戏服务器响应超时」，而同窗口 game-svr 又在持续
/// 收到 `SS_KEEPALIVE(104)`（该消息只由 LoginSrv 在**处理完 SS_SERVERINFO 后**发出）——
/// 「已刷新」与「被判超时」同时成立，必须拿到「每次发送/每次断链/链路代次」的可判事实。
///
/// 口径与同族缝一致：默认关（未设/空串/`0` ⇒ 不打）；只接受 `0`/`1`；启动一次性；非法值与非
/// UTF-8 ⇒ 启动失败。
pub const LOG_IDS_ENV: &str = "MIR2_LOG_IDS";

static LOG_IDS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// 解析取值：`None`/空串/`0` ⇒ `false`；`1` ⇒ `true`；其余（含带空白）⇒ 错误。
fn parse_log_ids(raw: Option<&str>) -> Result<bool, String> {
    match raw {
        None | Some("") | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(s) => Err(format!(
            "{LOG_IDS_ENV} 非法：{s:?}（只接受 0/1，空串=关闭）"
        )),
    }
}

/// 安装（进程内一次性）。
///
/// # Errors
/// 取值非法，或已安装过。
pub fn install_log_ids(raw: Option<&str>) -> Result<bool, String> {
    let v = parse_log_ids(raw)?;
    LOG_IDS
        .set(v)
        .map_err(|_| format!("{LOG_IDS_ENV} 只能安装一次"))?;
    Ok(v)
}

/// 从进程环境读取并安装（`run_inner` 唯一入口）。
///
/// # Errors
/// 取值非法、含非 UTF-8 字节，或已安装过。
pub fn install_log_ids_from_env() -> Result<bool, String> {
    match std::env::var_os(LOG_IDS_ENV) {
        None => install_log_ids(None),
        Some(os) => match os.to_str() {
            Some(s) => install_log_ids(Some(s)),
            None => Err(format!("{LOG_IDS_ENV} 含非 UTF-8 字节（拒绝静默当未设）")),
        },
    }
}

/// 是否记录 LoginSrv 链路日志（未安装 ⇒ `false`）。
#[must_use]
pub fn ids_log_enabled() -> bool {
    *LOG_IDS.get().unwrap_or(&false)
}

/// **临时观测缝（走位对齐用）**：`MIR2_LOG_MOVES`。默认关、只接受 `0`/`1`、启动一次性。
///
/// 用途：客户端报「骑马/跑动后攻击被拒（位置不一致）」时，需要把「服务端每收一条走位命令 →
/// 接受/拒绝 → 落地坐标」与客户端序列对齐。口径与同族缝一致。
pub const LOG_MOVES_ENV: &str = "MIR2_LOG_MOVES";

static LOG_MOVES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// 解析取值：`None`/空串/`0` ⇒ `false`；`1` ⇒ `true`；其余（含带空白）⇒ 错误。
fn parse_log_moves(raw: Option<&str>) -> Result<bool, String> {
    match raw {
        None | Some("") | Some("0") => Ok(false),
        Some("1") => Ok(true),
        Some(s) => Err(format!(
            "{LOG_MOVES_ENV} 非法：{s:?}（只接受 0/1，空串=关闭）"
        )),
    }
}

/// 安装（进程内一次性）。
///
/// # Errors
/// 取值非法，或已安装过。
pub fn install_log_moves(raw: Option<&str>) -> Result<bool, String> {
    let v = parse_log_moves(raw)?;
    LOG_MOVES
        .set(v)
        .map_err(|_| format!("{LOG_MOVES_ENV} 只能安装一次"))?;
    Ok(v)
}

/// 从进程环境读取并安装（`run_inner` 唯一入口）。
///
/// # Errors
/// 取值非法、含非 UTF-8 字节，或已安装过。
pub fn install_log_moves_from_env() -> Result<bool, String> {
    match std::env::var_os(LOG_MOVES_ENV) {
        None => install_log_moves(None),
        Some(os) => match os.to_str() {
            Some(s) => install_log_moves(Some(s)),
            None => Err(format!("{LOG_MOVES_ENV} 含非 UTF-8 字节（拒绝静默当未设）")),
        },
    }
}

/// 是否记录走位日志（未安装 ⇒ `false`）。
#[must_use]
pub fn move_log_enabled() -> bool {
    *LOG_MOVES.get().unwrap_or(&false)
}

// ---------------------------------------------------------------------------
// 世界就绪等待上界 + 测试用决策延迟（卡 `openmir2-quests-parity` §10.8）
// ---------------------------------------------------------------------------

/// 网关连接等待世界就绪的上界（毫秒）。缺省 30000（opc-07 2026-11-20 裁定：挂起必须有界）。
pub const GATE_READY_WAIT_MS_ENV: &str = "MIR2_GATE_READY_WAIT_MS";

/// 缺省上界：30s（与既有网关判定周期同量级；oracle 在同窗口直接拒连，本仓挂起以保启动时序）。
pub const DEFAULT_GATE_READY_WAIT_MS: u64 = 30_000;

/// 测试缝：把 sim 的「世界就绪/中止」决策点延迟 N 毫秒（缺省 `0` = 不延迟）。
///
/// 只供进程级用例构造「Ready 晚于上界」的场景（与 `MIR2_CLOCK_MS` / `MIR2_TEST_PERMISSION`
/// 同族：默认关、启动即固定、非法值 fail-closed）。
pub const TEST_WORLD_READY_DELAY_MS_ENV: &str = "MIR2_TEST_WORLD_READY_DELAY_MS";

static WORLD_READY_DELAY_MS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

fn parse_ms(name: &str, raw: Option<&str>, default_ms: u64) -> Result<u64, String> {
    match raw {
        None | Some("") => Ok(default_ms),
        Some(s) => s
            .parse::<u64>()
            .map_err(|_| format!("{name} 非法：{s:?}（只接受十进制毫秒；空串=缺省）")),
    }
}

/// 从进程环境读「等待世界就绪上界」（毫秒）。
///
/// # Errors
/// 非 UTF-8 或非法十进制。
pub fn gate_ready_wait_ms_from_env() -> Result<u64, String> {
    match std::env::var_os(GATE_READY_WAIT_MS_ENV) {
        None => Ok(DEFAULT_GATE_READY_WAIT_MS),
        Some(os) => {
            let s = os.to_str().ok_or_else(|| {
                format!("{GATE_READY_WAIT_MS_ENV} 含非 UTF-8 字节（拒绝静默当未设）")
            })?;
            parse_ms(GATE_READY_WAIT_MS_ENV, Some(s), DEFAULT_GATE_READY_WAIT_MS)
        }
    }
}

/// 安装测试缝「就绪决策延迟」（进程内一次性）。
///
/// # Errors
/// 非 UTF-8、非法十进制，或已安装过。
pub fn install_world_ready_delay_from_env() -> Result<u64, String> {
    let ms = match std::env::var_os(TEST_WORLD_READY_DELAY_MS_ENV) {
        None => 0,
        Some(os) => {
            let s = os.to_str().ok_or_else(|| {
                format!("{TEST_WORLD_READY_DELAY_MS_ENV} 含非 UTF-8 字节（拒绝静默当未设）")
            })?;
            parse_ms(TEST_WORLD_READY_DELAY_MS_ENV, Some(s), 0)?
        }
    };
    WORLD_READY_DELAY_MS
        .set(ms)
        .map_err(|_| format!("{TEST_WORLD_READY_DELAY_MS_ENV} 只能安装一次"))?;
    Ok(ms)
}

/// 就绪决策延迟（未安装 ⇒ `0`）。
#[must_use]
pub fn world_ready_delay() -> std::time::Duration {
    std::time::Duration::from_millis(*WORLD_READY_DELAY_MS.get().unwrap_or(&0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_contract() {
        assert_eq!(parse_log_attacks(None), Ok(false), "未设 ⇒ 关");
        assert_eq!(parse_log_attacks(Some("")), Ok(false), "空串 ⇒ 关");
        assert_eq!(parse_log_attacks(Some("0")), Ok(false));
        assert_eq!(parse_log_attacks(Some("1")), Ok(true));
        assert!(parse_log_attacks(Some("true")).is_err(), "只认 0/1");
        assert!(parse_log_attacks(Some("2")).is_err());
        assert!(parse_log_attacks(Some("-1")).is_err());
        assert!(parse_log_attacks(Some(" 1")).is_err(), "带空白不静默接受");
        assert!(!attack_log_enabled(), "本进程未安装 ⇒ 关");
    }

    #[test]
    fn parse_log_ids_contract() {
        assert_eq!(parse_log_ids(None), Ok(false), "未设 ⇒ 关");
        assert_eq!(parse_log_ids(Some("")), Ok(false), "空串 ⇒ 关");
        assert_eq!(parse_log_ids(Some("0")), Ok(false));
        assert_eq!(parse_log_ids(Some("1")), Ok(true));
        assert!(parse_log_ids(Some("true")).is_err());
        assert!(parse_log_ids(Some("2")).is_err());
        assert!(parse_log_ids(Some(" 1")).is_err(), "带空白不静默接受");
        assert!(!ids_log_enabled(), "本进程未安装 ⇒ 关");
        assert!(!move_log_enabled(), "走位缝同样默认关");
    }

    #[test]
    fn parse_ms_contract() {
        assert_eq!(parse_ms("X", None, 7), Ok(7), "未设 ⇒ 缺省");
        assert_eq!(parse_ms("X", Some(""), 7), Ok(7), "空串 ⇒ 缺省");
        assert_eq!(parse_ms("X", Some("0"), 7), Ok(0));
        assert_eq!(parse_ms("X", Some("250"), 7), Ok(250));
        assert!(parse_ms("X", Some("-1"), 7).is_err(), "负数非法");
        assert!(parse_ms("X", Some(" 1"), 7).is_err(), "带空白不静默接受");
        assert!(parse_ms("X", Some("1s"), 7).is_err(), "只认毫秒");
        assert_eq!(
            world_ready_delay(),
            std::time::Duration::ZERO,
            "本进程未安装 ⇒ 0"
        );
    }

    #[test]
    fn parse_log_moves_contract() {
        assert_eq!(parse_log_moves(None), Ok(false));
        assert_eq!(parse_log_moves(Some("")), Ok(false));
        assert_eq!(parse_log_moves(Some("0")), Ok(false));
        assert_eq!(parse_log_moves(Some("1")), Ok(true));
        assert!(parse_log_moves(Some("true")).is_err());
        assert!(parse_log_moves(Some("2")).is_err());
        assert!(parse_log_moves(Some(" 1")).is_err(), "带空白不静默接受");
    }
}
