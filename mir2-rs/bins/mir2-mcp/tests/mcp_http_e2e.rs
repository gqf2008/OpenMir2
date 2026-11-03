//! Streamable HTTP 传输端到端测试（卡 `openmir2-mcp-streamable-http`）。
//!
//! 全部真起 `mir2-mcp` 子进程；HTTP 客户端用 std `TcpStream` 手写（HTTP/1.0 + SSE），
//! 不引入 reqwest/axum 等额外依赖。覆盖 A1–A7 的机器可判面：
//!
//! * A2 缺 token ⇒ rc=2 且不启动服务；
//! * A3 非回环 bind ⇒ rc=2 且不启动服务；
//! * A4 无 token / 错 token ⇒ 401，且**零次触达工具处理器**（用进程级调用计数器日志证明：
//!   鉴权通过后的首次工具调用必须是 `#1`）；
//! * A5 同参数 `gm_list`/`conf_get` 在 stdio 与 HTTP 下的 `structuredContent` canonical JSON 相等
//!   （sha256 也相等）；
//! * A6 HTTP 服务可从 `127.0.0.1:<port>` 访问（真实监听面的定位证据见 patch body 的 PowerShell 输出）；
//! * `--help` 反映两种传输与「stdio 为缺省」。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-11-25";
/// 测试 token（仅测试进程内使用；不写日志、不进 patch body）。
const TOKEN: &str = "mir2-mcp-http-e2e-token";
const EXE: &str = env!("CARGO_BIN_EXE_mir2-mcp");

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "mir2-mcp-http-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("M2GameSvr")).expect("mkdir");
        let mut conf = Vec::from(&b"\xEF\xBB\xBF"[..]);
        conf.extend_from_slice(
            b"[DataBase]\r\nDbType=MySQL\r\n\r\n[Setup]\r\nMonsterPowerRate=10\r\nGenMonRate=10\r\n",
        );
        std::fs::write(path.join("M2GameSvr").join("Server.conf"), conf).expect("写夹具");
        Self { path }
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 占一个回环端口再释放（不依赖固定端口，CI 可并行）。
fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral");
    listener.local_addr().expect("addr").port()
}

// ---------------------------------------------------------------------------
// 极简 HTTP 客户端（HTTP/1.0，响应以连接关闭定界）
// ---------------------------------------------------------------------------

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// SSE 的 `data:` 行里的 JSON-RPC 消息（空数据 / 非 JSON 行跳过）。
    fn sse_messages(&self) -> Vec<Value> {
        let text = String::from_utf8_lossy(&self.body);
        let mut messages = Vec::new();
        for line in text.lines() {
            let Some(rest) = line.strip_prefix("data:") else {
                continue;
            };
            let data = rest.trim();
            if data.is_empty() {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(data) {
                messages.push(value);
            }
        }
        messages
    }
}

fn find_crlf(data: &[u8]) -> Option<usize> {
    data.windows(2).position(|window| window == b"\r\n")
}

fn decode_chunked(mut raw: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    while let Some(end) = find_crlf(raw) {
        let header = String::from_utf8_lossy(&raw[..end]);
        let size_part = header.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_part, 16) else {
            break;
        };
        raw = &raw[end + 2..];
        if size == 0 {
            break;
        }
        let take = size.min(raw.len());
        body.extend_from_slice(&raw[..take]);
        if raw.len() < size + 2 {
            break;
        }
        raw = &raw[size + 2..];
    }
    body
}

fn parse_response(raw: &[u8]) -> HttpResponse {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("响应必须含头部终止符");
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .expect("状态行")
        .parse::<u16>()
        .expect("状态码");
    let mut headers = Vec::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.push((key.trim().to_string(), value.trim().to_string()));
        }
    }
    let chunked = headers.iter().any(|(key, value)| {
        key.eq_ignore_ascii_case("transfer-encoding")
            && value.to_ascii_lowercase().contains("chunked")
    });
    let body = if chunked {
        decode_chunked(&raw[split + 4..])
    } else {
        raw[split + 4..].to_vec()
    };
    HttpResponse {
        status,
        headers,
        body,
    }
}

fn http_request(
    addr: SocketAddr,
    authorization: Option<&str>,
    session: Option<&str>,
    body: &str,
) -> HttpResponse {
    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("连接 HTTP 服务");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("读超时");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("写超时");
    let mut request = format!(
        "POST /mcp HTTP/1.0\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(authorization) = authorization {
        request.push_str(&format!("Authorization: {authorization}\r\n"));
    }
    if let Some(session) = session {
        request.push_str(&format!("Mcp-Session-Id: {session}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).expect("写请求");
    stream.flush().expect("flush");

    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => raw.extend_from_slice(&buffer[..read]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(error) => panic!("读响应失败：{error}"),
        }
    }
    parse_response(&raw)
}

// ---------------------------------------------------------------------------
// HTTP 服务子进程
// ---------------------------------------------------------------------------

struct HttpServer {
    child: Child,
    addr: SocketAddr,
    stderr_rx: Receiver<String>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl HttpServer {
    fn start(root: &Path, port: u16, trace: bool) -> Self {
        let mut command = Command::new(EXE);
        command
            .arg("--root")
            .arg(root)
            .arg("--transport")
            .arg("http")
            .arg("--bind")
            .arg(format!("127.0.0.1:{port}"))
            .env("MIR2_MCP_HTTP_TOKEN", TOKEN)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if trace {
            command.env("MIR2_MCP_TOOL_TRACE", "1");
        }
        let mut child = command.spawn().expect("启动 HTTP 服务");
        let stderr = child.stderr.take().expect("stderr");
        let (tx, stderr_rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("addr");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                break;
            }
            assert!(Instant::now() < deadline, "HTTP 服务 10s 内未监听 {addr}");
            std::thread::sleep(Duration::from_millis(20));
        }
        Self {
            child,
            addr,
            stderr_rx,
            reader: Some(reader),
        }
    }

    /// 收集已到达的「工具调用」日志行（**丢弃**其它 stderr 行）。
    fn drain_trace(&self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Ok(line) = self.stderr_rx.try_recv() {
            if line.starts_with("mir2-mcp-tool-invoked:") {
                lines.push(line);
            }
        }
        lines
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

// ---------------------------------------------------------------------------
// MCP over HTTP 会话
// ---------------------------------------------------------------------------

struct HttpMcp {
    addr: SocketAddr,
    session: Option<String>,
    next_id: u64,
}

impl HttpMcp {
    fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            session: None,
            next_id: 1,
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn post(&self, authorization: Option<&str>, message: &Value) -> HttpResponse {
        let body = serde_json::to_string(message).expect("serialize");
        http_request(self.addr, authorization, self.session.as_deref(), &body)
    }

    fn initialize(&mut self) {
        let id = self.next_id();
        let auth = format!("Bearer {TOKEN}");
        let response = self.post(
            Some(auth.as_str()),
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "initialize",
                "params": {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "mir2-mcp-http-e2e", "version": "0"},
                },
            }),
        );
        assert_eq!(response.status, 200, "initialize 状态");
        self.session = response.header("mcp-session-id").map(str::to_string);
        assert!(self.session.is_some(), "initialize 必须回 Mcp-Session-Id");
        let messages = response.sse_messages();
        assert!(
            message_with_id(&messages, id).get("result").is_some(),
            "initialize 必须有 result：{messages:?}"
        );
    }

    fn notify_initialized(&self) {
        let auth = format!("Bearer {TOKEN}");
        let response = self.post(
            Some(auth.as_str()),
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        );
        assert!(
            response.status == 200 || response.status == 202,
            "initialized 状态：{}",
            response.status
        );
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let id = self.next_id();
        let auth = format!("Bearer {TOKEN}");
        let response = self.post(
            Some(auth.as_str()),
            &json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "tools/call",
                "params": {"name": name, "arguments": arguments},
            }),
        );
        assert_eq!(response.status, 200, "tools/call 状态");
        let messages = response.sse_messages();
        message_with_id(&messages, id).clone()
    }
}

fn message_with_id(messages: &[Value], id: u64) -> &Value {
    messages
        .iter()
        .find(|message| message.get("id").and_then(Value::as_u64) == Some(id))
        .unwrap_or_else(|| panic!("缺 id={id} 的 JSON-RPC 响应：{messages:?}"))
}

fn structured(message: &Value) -> &Value {
    message
        .get("result")
        .and_then(|result| result.get("structuredContent"))
        .unwrap_or_else(|| panic!("响应缺 result.structuredContent：{message}"))
}

fn canonical_sha256(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::to_string(value).expect("canonical JSON");
    let digest = Sha256::digest(canonical.as_bytes());
    hex::encode(digest)
}

// ---------------------------------------------------------------------------
// stdio 子进程（等价性基准）
// ---------------------------------------------------------------------------

struct StdioMcp {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: u64,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl StdioMcp {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(EXE)
            .arg("--root")
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("启动 stdio 服务");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            rx,
            next_id: 1,
            reader: Some(reader),
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin 未关闭");
        writeln!(stdin, "{message}").expect("写 JSON-RPC");
        stdin.flush().expect("flush");
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "等 stdio 响应 id={id} 超时");
            let line = self
                .rx
                .recv_timeout(remaining)
                .unwrap_or_else(|_| panic!("等 stdio 响应 id={id} 超时"));
            let value: Value = serde_json::from_str(&line).expect("服务端输出必须是 JSON");
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }

    fn initialize(&mut self) {
        let init = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "mir2-mcp-http-e2e", "version": "0"},
            }),
        );
        assert!(
            init.get("result").is_some(),
            "stdio initialize 必须有 result"
        );
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    }
}

impl Drop for StdioMcp {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

// ---------------------------------------------------------------------------
// A2 / A3：启动前拒绝（rc=2，不创建监听）
// ---------------------------------------------------------------------------

/// 跑一次注定失败启动的二进制，断言 rc=2、stderr 点名原因、且**没有常驻监听**。
fn assert_startup_rejected(args: &[&str], env: &[(&str, &str)], keyword: &str) {
    let mut command = Command::new(EXE);
    command
        .args(args)
        .env_remove("MIR2_MCP_HTTP_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("运行 mir2-mcp");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "必须以 rc=2 拒绝启动；stderr：{stderr}"
    );
    assert!(
        stderr.contains(keyword),
        "stderr 必须点名 `{keyword}`：{stderr}"
    );
    assert!(
        !stderr.contains(TOKEN),
        "token 绝不能出现在 stderr：{stderr}"
    );
    // 进程已退出；若能立刻重新占用端口，说明它没有留下监听（也无残留 LISTEN）。
    let addr: SocketAddr = args
        .iter()
        .position(|arg| *arg == "--bind")
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse::<SocketAddr>().ok())
        .unwrap_or_else(|| "127.0.0.1:7950".parse().expect("default bind"));
    if addr.ip().is_loopback() {
        let reclaimed = TcpListener::bind(addr);
        assert!(reclaimed.is_ok(), "拒绝启动后端口 {addr} 必须可重新监听");
    }
}

#[test]
fn http_transport_rejects_missing_token() {
    // A2：无 env、无 --token-file ⇒ rc=2，stderr 点名 token，不监听。
    let port = free_port();
    let bind = format!("127.0.0.1:{port}");
    assert_startup_rejected(
        &["--transport", "http", "--bind", bind.as_str()],
        &[],
        "token",
    );
}

#[test]
fn http_transport_rejects_empty_token_file() {
    let port = free_port();
    let bind = format!("127.0.0.1:{port}");
    let token_file = std::env::temp_dir().join(format!(
        "mir2-mcp-empty-token-{}-{port}.txt",
        std::process::id()
    ));
    std::fs::write(&token_file, "  \r\n\t").expect("写空 token 夹具");
    let path = token_file.to_str().expect("utf8 path").to_string();
    assert_startup_rejected(
        &[
            "--transport",
            "http",
            "--bind",
            bind.as_str(),
            "--token-file",
            path.as_str(),
        ],
        &[],
        "token",
    );
    let _ = std::fs::remove_file(&token_file);
}

#[test]
fn http_transport_rejects_non_loopback_bind() {
    // A3：非回环 ⇒ rc=2，stderr 点名回环，不监听。
    let port = free_port();
    let bind = format!("0.0.0.0:{port}");
    assert_startup_rejected(
        &["--transport", "http", "--bind", bind.as_str()],
        &[("MIR2_MCP_HTTP_TOKEN", TOKEN)],
        "回环",
    );
    let hostname = format!("localhost:{port}");
    assert_startup_rejected(
        &["--transport", "http", "--bind", hostname.as_str()],
        &[("MIR2_MCP_HTTP_TOKEN", TOKEN)],
        "IP 字面量",
    );
}

// ---------------------------------------------------------------------------
// A4：401 且零触达工具处理器
// ---------------------------------------------------------------------------

#[test]
fn http_auth_rejects_and_never_touches_tool_handlers() {
    let root = TempRoot::new("auth");
    let port = free_port();
    let server = HttpServer::start(&root.path, port, true);

    let mut client = HttpMcp::new(server.addr);
    client.initialize();
    client.notify_initialized();

    // 无 Authorization：携带**有效 session**的 tools/call —— 若鉴权被绕过，它会真的进处理器。
    let no_token = http_request(
        server.addr,
        None,
        client.session.as_deref(),
        &serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 100,
            "method": "tools/call",
            "params": {"name": "gm_list", "arguments": {}},
        }))
        .expect("serialize"),
    );
    assert_eq!(no_token.status, 401, "无 token 必须 401");
    assert!(
        no_token.body.is_empty(),
        "401 响应体不得含细节：{:?}",
        no_token.body
    );

    // 错 token：同样带有效 session。
    let wrong = http_request(
        server.addr,
        Some("Bearer definitely-wrong"),
        client.session.as_deref(),
        &serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 101,
            "method": "tools/call",
            "params": {"name": "gm_list", "arguments": {}},
        }))
        .expect("serialize"),
    );
    assert_eq!(wrong.status, 401, "错 token 必须 401");
    assert!(wrong.body.is_empty(), "401 响应体不得含细节");

    // 正确 token 的两次工具调用：计数器必须从 #1 连续到 #2。
    let listed = client.call_tool("gm_list", json!({}));
    assert_eq!(
        structured(&listed).get("ok").and_then(Value::as_bool),
        Some(true)
    );
    let helped = client.call_tool("gm_help", json!({"name": "Tingwa"}));
    assert_eq!(
        structured(&helped).get("ok").and_then(Value::as_bool),
        Some(true)
    );

    // 等待至少 2 条调用日志（确定性条件：两次成功调用各产生一条）。
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut trace = server.drain_trace();
    while trace.len() < 2 {
        assert!(Instant::now() < deadline, "等待工具调用日志超时：{trace:?}");
        std::thread::sleep(Duration::from_millis(20));
        trace.extend(server.drain_trace());
    }
    // 允许 reader 线程把剩余行送完（两次调用都已在响应前写完）。
    std::thread::sleep(Duration::from_millis(300));
    trace.extend(server.drain_trace());

    assert_eq!(
        trace,
        vec![
            "mir2-mcp-tool-invoked: gm_list #1".to_string(),
            "mir2-mcp-tool-invoked: gm_help #2".to_string(),
        ],
        "401 请求必须零触达：首次工具调用只能是 #1"
    );
}

// ---------------------------------------------------------------------------
// A5 / A6：两种传输的 structuredContent 逐字节等价
// ---------------------------------------------------------------------------

#[test]
fn http_and_stdio_structured_content_are_byte_identical() {
    let root = TempRoot::new("equiv");
    let port = free_port();
    let server = HttpServer::start(&root.path, port, false);

    // HTTP：真实监听面必须可经 127.0.0.1 访问（A6 的机器可判部分）。
    let mut http = HttpMcp::new(server.addr);
    http.initialize();
    http.notify_initialized();
    let http_list = http.call_tool("gm_list", json!({}));
    let http_conf = http.call_tool("conf_get", json!({"section": "Setup", "key": "GenMonRate"}));

    // stdio：同一 root、同参数。
    let mut stdio = StdioMcp::start(&root.path);
    stdio.initialize();
    let stdio_list = stdio.call_tool("gm_list", json!({}));
    let stdio_conf = stdio.call_tool("conf_get", json!({"section": "Setup", "key": "GenMonRate"}));

    for (name, http_value, stdio_value) in [
        ("gm_list", structured(&http_list), structured(&stdio_list)),
        ("conf_get", structured(&http_conf), structured(&stdio_conf)),
    ] {
        assert_eq!(
            serde_json::to_string(http_value).expect("canonical"),
            serde_json::to_string(stdio_value).expect("canonical"),
            "{name} 的 structuredContent canonical JSON 必须逐字节相等"
        );
        assert_eq!(
            canonical_sha256(http_value),
            canonical_sha256(stdio_value),
            "{name} 的 canonical sha256 必须相等"
        );
    }

    // 顺带钉住工具语义没被传输层改动（与既有 stdio e2e 同口径）。
    assert_eq!(
        structured(&http_list)
            .get("catalog_total")
            .and_then(Value::as_u64),
        Some(185)
    );
    assert_eq!(
        structured(&http_conf).get("value").and_then(Value::as_str),
        Some("10")
    );
}

// ---------------------------------------------------------------------------
// CLI：help 反映新参数与 stdio 缺省
// ---------------------------------------------------------------------------

#[test]
fn help_documents_both_transports() {
    let output = Command::new(EXE)
        .arg("--help")
        .output()
        .expect("运行 --help");
    assert!(output.status.success(), "--help 必须 rc=0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "--transport",
        "stdio",
        "http",
        "--bind",
        "127.0.0.1:7950",
        "--token-file",
    ] {
        assert!(
            stdout.contains(expected),
            "--help 必须提到 `{expected}`：{stdout}"
        );
    }
    assert!(
        stdout.contains("缺省"),
        "--help 必须说明 stdio 是缺省传输：{stdout}"
    );
}
