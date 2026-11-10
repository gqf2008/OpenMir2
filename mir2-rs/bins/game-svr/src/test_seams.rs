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
    }
}
