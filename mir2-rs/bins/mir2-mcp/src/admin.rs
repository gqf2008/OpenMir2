//! game-svr 本机 admin 通道的最小客户端（卡 `openmir2-mcp-gm-execute`，T-273）。
//!
//! 协议：TCP 回环、**逐行 JSON**。请求 `{"chr":"<角色名>","cmd":"@..."}`；响应
//! `{"ok":bool,"error":?string,"frames":N,"note":?string}`（见 game-svr 侧
//! `AdminGmReply`）。本模块只做「连接 → 发一行 → 读一行 → 解析」，不做任何本地 GM 语义：
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
            Self::Connect(_) => "admin_connect_failed",
            Self::Timeout(_) => "admin_timeout",
            Self::Protocol(_) => "admin_protocol_error",
        }
    }

    /// 人读消息。
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::BadAddr(m) | Self::Connect(m) | Self::Timeout(m) | Self::Protocol(m) => m,
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

/// 向 `addr`（`host:port`）投一条 GM 命令并返回对端的 JSON 响应（原样透传）。
///
/// # Errors
///
/// 地址非法/连不上/超时/响应不是约定的 JSON 对象（含 `ok: bool`）。
pub fn execute(addr: &str, chr: &str, cmd: &str) -> Result<Value, AdminFailure> {
    let sock_addr = addr
        .to_socket_addrs()
        .map_err(|e| AdminFailure::BadAddr(format!("解析 admin 地址 {addr:?} 失败：{e}")))?
        .next()
        .ok_or_else(|| AdminFailure::BadAddr(format!("admin 地址 {addr:?} 无可用套接字")))?;
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
}
