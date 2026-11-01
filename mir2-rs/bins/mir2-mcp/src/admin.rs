//! game-svr 本机 admin 通道的最小客户端（卡 `openmir2-mcp-gm-execute`，T-273）。
//!
//! 协议：TCP 回环、**逐行 JSON**。请求 `{"chr":"<角色名>","cmd":"@..."}`；响应
//! `{"ok":bool,"error":?string,"frames":N,"note":?string}`（见 game-svr 侧
//! `AdminGmReply`）。`frames` = 本次调用的**直接应答帧数**（`SysMsg` + 未知命令回落 +
//! 直发帧），**不含**事件末 `command_event_tail_gm_drain` 落地的副作用广播帧（刷怪/清除/203/709/41…）。
//! 目标地址解析后必须是**回环**，否则在连接前返回结构化 `admin_addr_not_loopback`。
//! 本模块只做「连接 → 发一行 → 读一行 → 解析」，不做任何本地 GM 语义：
//! 命令解析/执行全部在 game-svr 的同一执行核心（`gmcmd::execute` + 事件末 drain）。
//!
//! 失败面全部结构化：未配置/连不上/超时/协议不符都映射到稳定 `code`，不 panic。

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{json, Value};

/// 建立连接的超时（本机回环，2s 足够；连不上 = 通道没开）。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// 单次请求/响应的读写超时（game-svr 侧回包超时是 5s，这里留同等量级冗余）。
pub const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// admin 调用失败的分类（`code` 是机器可判的稳定标识）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminFailure {
    /// 地址无法解析（例如 `MIR2_MCP_ADMIN_ADDR` 写坏）。
    BadAddr(String),
    /// 地址解析成功但**不是回环**（`MIR2_MCP_ADMIN_ADDR` 指向本机之外）。客户端侧纵深防御：
    /// 服务端 accept 层同样只接受回环对端，这里在连接前拒绝。
    NotLoopback(String),
    /// 连不上（通道未开 / 端口错 / game-svr 未起）。
    Connect(String),
    /// 读写超时。
    Timeout(String),
    /// 对端回的不是约定的 JSON 形状。
    Protocol(String),
}

impl AdminFailure {
    /// 稳定错误码。
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadAddr(_) => "admin_bad_addr",
            Self::NotLoopback(_) => "admin_addr_not_loopback",
            Self::Connect(_) => "admin_connect_failed",
            Self::Timeout(_) => "admin_timeout",
            Self::Protocol(_) => "admin_protocol_error",
        }
    }

    /// 人读消息。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::BadAddr(m)
            | Self::NotLoopback(m)
            | Self::Connect(m)
            | Self::Timeout(m)
            | Self::Protocol(m) => m,
        }
    }

    /// `gm_execute` 的结构化错误体（`isError` + `code/message`）。
    #[must_use]
    pub fn to_json(&self, addr: &str, chr: &str, cmd: &str) -> Value {
        json!({
            "ok": false,
            "code": self.code(),
            "message": self.message(),
            "admin_addr": addr,
            "chr": chr,
            "cmd": cmd,
            "wrote": false,
        })
    }
}

/// 解析 `addr`（`host:port`）并只取回**环地址**（卡 `openmir2-mcp-admin-review-minors` F4）。
///
/// 客户端侧纵深防御：`MIR2_MCP_ADMIN_ADDR` 来自环境变量，可能被指到任意主机。服务端
/// （game-svr accept 层）同样只接受回环对端，这里在**连接前**拒绝，把失败稳定成结构化
/// `admin_addr_not_loopback`。同名解析出多个地址（IPv4/IPv6/混合）⇒ 取**第一个回环地址**；
/// 一个回环地址都没有才拒绝。
fn resolve_loopback(addr: &str) -> Result<std::net::SocketAddr, AdminFailure> {
    let addrs: Vec<std::net::SocketAddr> = addr
        .to_socket_addrs()
        .map_err(|e| AdminFailure::BadAddr(format!("解析 admin 地址 {addr:?} 失败：{e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(AdminFailure::BadAddr(format!(
            "admin 地址 {addr:?} 无可用套接字"
        )));
    }
    addrs
        .into_iter()
        .find(|candidate| candidate.ip().is_loopback())
        .ok_or_else(|| {
            AdminFailure::NotLoopback(format!(
                "admin 地址 {addr:?} 不是回环地址（admin 通道只在本机回环监听）"
            ))
        })
}

/// 向 `addr`（`host:port`）投一条 GM 命令并返回对端的 JSON 响应（原样透传）。
///
/// 目标必须是**回环**地址（[`resolve_loopback`]）；非回环 ⇒ [`AdminFailure::NotLoopback`]。
///
/// # Errors
///
/// 地址非法/非回环/连不上/超时/响应不是约定的 JSON 对象（含 `ok: bool`）。
pub fn execute(addr: &str, chr: &str, cmd: &str) -> Result<Value, AdminFailure> {
    let sock_addr = resolve_loopback(addr)?;
    let mut stream = TcpStream::connect_timeout(&sock_addr, CONNECT_TIMEOUT)
        .map_err(|e| AdminFailure::Connect(format!("连接 admin 通道 {addr} 失败：{e}")))?;
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_nodelay(true);

    let request = json!({ "chr": chr, "cmd": cmd });
    stream
        .write_all(request.to_string().as_bytes())
        .and_then(|()| stream.write_all(b"\n"))
        .and_then(|()| stream.flush())
        .map_err(|e| classify_io("写 admin 请求", e))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let n = reader
        .read_line(&mut line)
        .map_err(|e| classify_io("读 admin 响应", e))?;
    if n == 0 {
        return Err(AdminFailure::Protocol(
            "admin 通道在回包前关闭（对端未执行/已退出）".to_string(),
        ));
    }
    let value: Value = serde_json::from_str(line.trim()).map_err(|e| {
        AdminFailure::Protocol(format!(
            "admin 响应不是合法 JSON：{e}；原文={:?}",
            line.trim()
        ))
    })?;
    if !value.is_object() || !value.get("ok").is_some_and(Value::is_boolean) {
        return Err(AdminFailure::Protocol(format!(
            "admin 响应缺 `ok: bool`：{value}"
        )));
    }
    Ok(value)
}

/// `io::Error` → 稳定分类（Windows 的读超时常见 `TimedOut` 或 `WouldBlock`）。
fn classify_io(context: &str, error: std::io::Error) -> AdminFailure {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            AdminFailure::Timeout(format!("{context}超时：{error}"))
        }
        _ => AdminFailure::Connect(format!("{context}失败：{error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreachable_address_is_structured_connect_failure() {
        // 绑定后立刻关闭拿到一个「大概率无人监听」的本地端口，避免固定端口冲突。
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        drop(listener);
        let err = execute(&addr.to_string(), "aaa", "@时间").expect_err("必须失败");
        assert!(
            matches!(
                err,
                AdminFailure::Connect(_) | AdminFailure::Timeout(_) | AdminFailure::Protocol(_)
            ),
            "连不上必须结构化：{err:?}"
        );
        assert!(!err.code().is_empty());
    }

    #[test]
    fn bad_address_is_structured() {
        let err = execute("not-an-addr-without-port", "aaa", "@时间").expect_err("必须失败");
        assert!(matches!(err, AdminFailure::BadAddr(_)), "{err:?}");
        assert_eq!(err.code(), "admin_bad_addr");
    }

    /// F4：`MIR2_MCP_ADMIN_ADDR` 指向非回环地址时，客户端必须在**连接前**结构化拒绝。
    ///
    /// 用 TEST-NET-1（RFC 5737，`192.0.2.0/24`）保证不指向真实可达主机；错误变体是
    /// `NotLoopback`（而不是 `Connect`）本身就证明没有发出连接尝试。
    ///
    /// 变异自证：把 [`resolve_loopback`] 的 `.find(|c| c.ip().is_loopback())` 换成
    /// `.next()`（=删掉回环校验）⇒ 这里会变成 `Connect`/`Timeout` ⇒ 必红。
    #[test]
    fn non_loopback_admin_addr_is_rejected_before_connect() {
        let err = execute("192.0.2.1:7946", "aaa", "@时间").expect_err("必须失败");
        assert!(matches!(err, AdminFailure::NotLoopback(_)), "{err:?}");
        assert_eq!(err.code(), "admin_addr_not_loopback");
        assert!(
            err.message().contains("不是回环地址"),
            "人读消息必须说清原因：{err:?}"
        );
    }

    /// F4 判据面：回环字面量（v4/v6）放行，私网/公网/未指定地址拒绝。
    #[test]
    fn loopback_resolution_accepts_only_loopback_literals() {
        for addr in ["127.0.0.1:7946", "127.255.255.254:7946", "[::1]:7946"] {
            let resolved = resolve_loopback(addr).expect("回环地址必须放行");
            assert!(resolved.ip().is_loopback(), "{addr}");
        }
        for addr in [
            "192.168.1.10:7946",
            "10.0.0.5:7946",
            "8.8.8.8:7946",
            "0.0.0.0:7946",
            "[::]:7946",
            "192.0.2.1:7946",
        ] {
            assert!(
                matches!(resolve_loopback(addr), Err(AdminFailure::NotLoopback(_))),
                "非回环地址必须拒绝：{addr} ⇒ {:?}",
                resolve_loopback(addr)
            );
        }
    }
}
