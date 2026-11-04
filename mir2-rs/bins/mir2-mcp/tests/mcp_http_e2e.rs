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
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
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

/// 启动行前缀（与服务端 `http::serve` 的 eprintln! 逐字一致）。
const LISTENING_PREFIX: &str = "mir2-mcp: HTTP 传输监听 ";

/// 从启动行解析实际监听地址：`mir2-mcp: HTTP 传输监听 http://127.0.0.1:<port>/mcp（…）`。
///
/// 只认回环地址（服务端本就只 bind 回环；解析层再拒一次，避免测试把非回环当成自己人）。
fn parse_listening_addr(line: &str) -> Option<SocketAddr> {
    let rest = line.strip_prefix(LISTENING_PREFIX)?;
    let url = rest.strip_prefix("http://")?;
    let authority = url.split('/').next()?;
    let addr: SocketAddr = authority.parse().ok()?;
    addr.ip().is_loopback().then_some(addr)
}

/// 等启动行出现并解析出实际端口（子进程 `--bind ...:0`，端口由系统分配）。
fn wait_for_listening_addr(rx: &Receiver<String>, within: Duration) -> SocketAddr {
    let deadline = Instant::now() + within;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "等待启动行超时");
        let line = rx.recv_timeout(remaining).expect("等待启动行超时");
        if let Some(addr) = parse_listening_addr(&line) {
            return addr;
        }
    }
}

/// 取一个「当前空闲」的回环端口——**只用于启动前就被拒绝的负例**（子进程在 bind 之前
/// 以 rc=2 退出，因此不存在「取端口 → 子进程 bind」的竞争窗口；用例还会立刻把它重新绑回来
/// 证明没有残留监听）。
///
/// 真正要监听的服务一律用 `--bind 127.0.0.1:0` 并由启动行解析实际端口（见
/// [`HttpServer::start_with`]）：那是「先绑后放」TOCTOU 的正解。
fn ephemeral_port() -> u16 {
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
    http_request_with_host(addr, &addr.to_string(), authorization, session, body)
}

/// 同 [`http_request`]，但 `Host` 头可指定（用于 Host 白名单的负例/正例）。
fn http_request_with_host(
    addr: SocketAddr,
    host: &str,
    authorization: Option<&str>,
    session: Option<&str>,
    body: &str,
) -> HttpResponse {
    http_request_with_origin(addr, host, None, authorization, session, body)
}

/// 同 [`http_request_with_host`]，并可注入 `Origin` 头（用于 Origin 403 的证据）。
fn http_request_with_origin(
    addr: SocketAddr,
    host: &str,
    origin: Option<&str>,
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
        "POST /mcp HTTP/1.0\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(authorization) = authorization {
        request.push_str(&format!("Authorization: {authorization}\r\n"));
    }
    if let Some(origin) = origin {
        request.push_str(&format!("Origin: {origin}\r\n"));
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
    fn start(root: &Path, trace: bool) -> Self {
        Self::start_with(root, "127.0.0.1", &[], trace)
    }

    /// 真起子进程：`--bind <ip>:0` 由系统分配端口，再解析 stderr 启动行取实际端口。
    ///
    /// 这样测试内不再需要「先 bind 取端口 → 释放 → 交给子进程 bind」的窗口，
    /// 并发跑用例时不会互相抢端口（`os error 10048`）。
    fn start_with(root: &Path, bind_ip: &str, extra_args: &[&str], trace: bool) -> Self {
        let mut command = Command::new(EXE);
        command
            .arg("--root")
            .arg(root)
            .arg("--transport")
            .arg("http")
            .arg("--bind")
            .arg(format!("{bind_ip}:0"))
            .args(extra_args)
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
        let addr = wait_for_listening_addr(&stderr_rx, Duration::from_secs(10));
        // 启动行在 `TcpListener::bind`（= bind + listen）之后才打印，因此能解析出地址
        // 就说明端口已在监听——不要再额外连一次探活：那会占用一个并发连接名额，
        // 让 `--max-connections 2` 这类用例多出一个瞬时「幽灵连接」。
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

    /// `initialize` 握手，返回该响应的 `result` 对象（供跨 bind 地址的逐字节比较）。
    fn initialize(&mut self) -> Value {
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
        let message = message_with_id(&messages, id);
        let result = message
            .get("result")
            .unwrap_or_else(|| panic!("initialize 必须有 result：{messages:?}"));
        assert!(result.is_object(), "initialize 的 result 必须是对象");
        result.clone()
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

/// 一次「本应被拒绝的启动」的观测（rc + stderr）。
///
/// 把观测与判据拆开，是为了让**拒绝路径本身**可测：用例可以把一条真实生产格式的
/// 启动行喂进来，走完 `assert_startup_rejected` 所用的同一判据（见
/// `startup_rejection_path_rejects_real_listening_line`）——牙打在接线之后，
/// 而不是只证明 helper 函数自己会红。
struct StartupObservation {
    code: Option<i32>,
    stderr: String,
}

/// 从 `--bind <value>` 取监听地址；只接受可解析成 `SocketAddr` 的字面量。
///
/// 解析失败返回 `None`（如 `localhost:7950`）。调用方据此**跳过**端口探测与回收：
/// 解析失败的正确语义是「不知道要探哪个端口」，不是「探缺省端口」。
fn bind_addr_from_args(args: &[&str]) -> Option<SocketAddr> {
    args.windows(2)
        .find(|pair| pair[0] == "--bind")
        .and_then(|pair| pair[1].parse::<SocketAddr>().ok())
}

/// 需要做端口探测的目标：`--bind` 可解析，且是「本机可控地址」（回环 / 通配）。
///
/// 具名非回环地址不能当 oracle：本机路由/中间盒可能对任意非本地地址回 SYN
/// （实测 192.0.2.1 / 198.51.100.1 / 203.0.113.1 的任意端口都 connect 成功），
/// 而真实 LAN 主机同端口在听又会假红——两个方向都不可靠。
///
/// 历史坑（Info-3）：这里曾在解析失败时回落 `127.0.0.1:7950`（服务端缺省端口）。
/// 7950 在 CI / 他人机器上可能被占用，回落值会把「拒绝启动」误判成「仍在监听」的假红。
fn probe_target_from_args(args: &[&str]) -> Option<SocketAddr> {
    let addr = bind_addr_from_args(args)?;
    (addr.ip().is_loopback() || addr.ip().is_unspecified()).then_some(addr)
}

/// 拒绝启动的**全部结论性判据**：rc=2、stderr 点名原因、无 token、无监听启动行。
///
/// 这是「判启动行」的唯一显式接线：`assert_startup_rejected` 只调用本函数一次。
/// 删掉本函数里的任何一条判据（尤其 `assert_no_listening_startup_line`）都会让
/// `startup_rejection_path_rejects_real_listening_line` 与
/// `startup_rejection_path_invokes_listening_line_criterion` 变红。
fn assert_startup_rejection(observation: &StartupObservation, keyword: &str) {
    let stderr = &observation.stderr;
    assert_eq!(
        observation.code,
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
    // 「确实没在监听」的机器判据（不再只看 rc=2 + stderr）。
    // 启动行只在 `TcpListener::bind` 成功后打印，因此它缺席 = 从未进入监听。
    assert_no_listening_startup_line(stderr);
}

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
    let observation = StartupObservation {
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    // 显式接线：拒绝路径的全部结论性判据（含「无监听启动行」）都在这里。
    assert_startup_rejection(&observation, keyword);
    // 端口探测只对可解析的「本机可控地址」做；解析失败不探、也不回落缺省 7950。
    if let Some(addr) = probe_target_from_args(args) {
        assert_port_not_listening(addr, probe_addr_for(addr));
    }
}

thread_local! {
    /// 「无监听启动行」判据的调用计数（S2 接线牙）。
    ///
    /// 合成牙 `listening_startup_line_is_rejected_by_the_criterion` 只证明 helper 自身能红，
    /// 不证明拒绝路径真的调用了它。计数按线程隔离，避免并行用例互相污染。
    static LISTENING_LINE_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// 当前线程上「无监听启动行」判据已被调用多少次（供接线牙读）。
fn listening_line_check_count() -> usize {
    LISTENING_LINE_CHECKS.with(|count| count.get())
}

/// 拒绝启动的进程不得打印监听启动行（该行只在 `TcpListener::bind` 成功后打印）。
fn assert_no_listening_startup_line(stderr: &str) {
    LISTENING_LINE_CHECKS.with(|count| count.set(count.get() + 1));
    assert!(
        !stderr.contains("HTTP 传输监听"),
        "拒绝启动的进程不得打印监听启动行（证明从未 bind 成功）：{stderr}"
    );
}

/// 按地址族选探针：通配地址用**同族**回环（Windows 默认 v6only 下 `[::]:P` 不响应
/// `127.0.0.1:P` 的连接），其余地址（回环/具名 IP）用地址本体。
fn probe_addr_for(bind_addr: SocketAddr) -> SocketAddr {
    let port = bind_addr.port();
    match bind_addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port)
        }
        _ => bind_addr,
    }
}

/// 机器判据：`bind_addr` 上没有监听面——`probe` 连不上，且回环/通配地址可重新 bind。
fn assert_port_not_listening(bind_addr: SocketAddr, probe: SocketAddr) {
    if TcpStream::connect_timeout(&probe, Duration::from_millis(750)).is_ok() {
        panic!("拒绝启动后 {probe} 仍可连接：说明进程确实监听了 {bind_addr}");
    }
    if probe.ip().is_loopback() || bind_addr.ip().is_unspecified() {
        let reclaimed = TcpListener::bind(bind_addr);
        assert!(reclaimed.is_ok(), "拒绝启动后 {bind_addr} 必须可重新监听");
    }
}

#[test]
fn http_transport_rejects_missing_token() {
    // A2：无 env、无 --token-file ⇒ rc=2，stderr 点名 token，不监听。
    let port = ephemeral_port();
    let bind = format!("127.0.0.1:{port}");
    assert_startup_rejected(
        &["--transport", "http", "--bind", bind.as_str()],
        &[],
        "token",
    );
}

#[test]
fn http_transport_rejects_empty_token_file() {
    let port = ephemeral_port();
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
    let port = ephemeral_port();
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
    // Minor-1：具名非回环改用 RFC 5737 TEST-NET-1（192.0.2.0/24 文档保留地址），
    // 不再用可能真实存在的私网地址（192.168.1.10）。该地址只作为「非回环字面量」
    // 的输入；是否真的监听由启动行判据判定，不对它做 connect 探测
    // （见 `assert_startup_rejected`：本机中间盒会对 TEST-NET 任意端口回 SYN）。
    let reserved = format!("192.0.2.1:{port}");
    assert_startup_rejected(
        &["--transport", "http", "--bind", reserved.as_str()],
        &[("MIR2_MCP_HTTP_TOKEN", TOKEN)],
        "回环",
    );
}

#[test]
fn http_transport_rejects_ipv6_wildcard_bind() {
    // Minor-2：`[::]:P` 也必须拒绝启动；探针按地址族走 `[::1]:P`
    // （Windows 默认 v6only 下 `127.0.0.1:P` 探不到 `[::]:P`）。
    let port = ephemeral_port();
    let bind = format!("[::]:{port}");
    assert_startup_rejected(
        &["--transport", "http", "--bind", bind.as_str()],
        &[("MIR2_MCP_HTTP_TOKEN", TOKEN)],
        "回环",
    );
}

// ---------------------------------------------------------------------------
// 探针判据的正例牙（Minor-1 / Minor-2）
// ---------------------------------------------------------------------------

/// 纯函数判据：通配地址按族选回环探针，其余地址用本体。
#[test]
fn probe_addr_is_family_matched_for_wildcards() {
    let port = 4242_u16;
    let v4_wildcard: SocketAddr = format!("0.0.0.0:{port}").parse().expect("v4 wildcard");
    let v6_wildcard: SocketAddr = format!("[::]:{port}").parse().expect("v6 wildcard");
    assert_eq!(
        probe_addr_for(v4_wildcard),
        format!("127.0.0.1:{port}")
            .parse::<SocketAddr>()
            .expect("v4 lo")
    );
    assert_eq!(
        probe_addr_for(v6_wildcard),
        format!("[::1]:{port}")
            .parse::<SocketAddr>()
            .expect("v6 lo")
    );
    let loopback: SocketAddr = format!("127.0.0.2:{port}").parse().expect("loopback");
    assert_eq!(probe_addr_for(loopback), loopback);
    let named: SocketAddr = format!("192.0.2.1:{port}").parse().expect("named");
    assert_eq!(probe_addr_for(named), named);
}

/// 合成夹具：探针地址上确有监听 ⇒ 判据判红；撤掉监听 ⇒ 同一判据不假红（反向对照）。
///
/// 证明「非回环改用 TEST-NET」不是把牙删掉换绿：只要探针地址真有监听，
/// `assert_port_not_listening` 仍会 panic。
///
/// 监听用 `0.0.0.0:P`（`127.0.0.1:P` 能连上），而 `bind_addr` 故意用另一条空闲回环
/// `127.0.0.2:Q`——这样「回收 bind 失败」不会替 connect 判据兜底，删掉 connect 断言时
/// 本用例必然红（牙只钉 connect 判据）。
#[test]
fn startup_rejection_probe_detects_live_listener() {
    let listener = TcpListener::bind("0.0.0.0:0").expect("bind 合成监听");
    let live_port = listener.local_addr().expect("local_addr").port();
    let probe: SocketAddr = format!("127.0.0.1:{live_port}").parse().expect("probe");
    let free = TcpListener::bind("127.0.0.2:0").expect("bind 空闲回环");
    let bind_addr = free.local_addr().expect("local_addr");
    drop(free);
    let outcome = std::panic::catch_unwind(move || assert_port_not_listening(bind_addr, probe));
    assert!(outcome.is_err(), "探针 {probe} 确有监听时，判据必须判红");
    drop(listener);
    assert_port_not_listening(bind_addr, probe);
}

/// 合成夹具：stderr 里出现监听启动行 ⇒ 结论性判据判红；普通错误行 ⇒ 放行。
///
/// 具名非回环地址不再做 connect 探测后，「无监听启动行」就是该分支的唯一 oracle，
/// 本用例证明它仍能判出「服务真的监听了」（不是放宽判据换绿）。
#[test]
fn listening_startup_line_is_rejected_by_the_criterion() {
    let fake_listening = "mir2-mcp: HTTP 传输监听 http://127.0.0.1:1234/mcp（只回环）";
    let outcome =
        std::panic::catch_unwind(move || assert_no_listening_startup_line(fake_listening));
    assert!(outcome.is_err(), "启动行存在时，结论性判据必须判红");
    assert_no_listening_startup_line("mir2-mcp: --bind 不是回环地址；拒绝启动");
}

/// 合成夹具：`[::]:P` 的探针必须是 `[::1]:P`，且「探错地址族」必须由**判据本身**判红。
///
/// 夹具在 `[::1]:P` 上**显式**监听（不是 `[::]:P`）：这样无论平台 dual-stack 默认如何，
/// 只有 `[::1]:P` 连得上、`127.0.0.1:P` 连不上。`bind_addr` 换成另一条**空闲**回环，
/// 回收 bind 一定成功——判据外的回收不能再替 connect 判据兜底（原用例的漏洞正是这点：
/// 探针退回 v4 + 删掉 `assert_eq` 后，回收 `[::]:P` 失败仍能让用例"绿"，注释里
/// 「退回 v4 会因没探到而失败」不成立）。
///
/// 需要 IPv6 栈（`[::1]`）：环境无 IPv6 时 `bind [::1]:0` 会**显式失败**，
/// 不静默跳过；在无 IPv6 的 CI 上运行请加 `#[ignore = "需要 IPv6 栈"]` 并注明理由
/// （本机 E2E 已在有 IPv6 的 Windows 上实跑，见 patch body）。
#[test]
fn ipv6_wildcard_probe_uses_ipv6_loopback() {
    let listener = TcpListener::bind("[::1]:0").expect("环境需支持 IPv6：bind [::1]:0");
    let port = listener.local_addr().expect("local_addr").port();
    let wildcard: SocketAddr = format!("[::]:{port}").parse().expect("wildcard addr");
    let probe = probe_addr_for(wildcard);
    assert_eq!(
        probe.ip(),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
        "[::]:P 的探针必须是 [::1]:P（Windows v6only 下 127.0.0.1:P 探不到）"
    );

    // 判据外的回收必须成功，否则「探错地址族」会被回收分支兜住。
    let free = TcpListener::bind("127.0.0.2:0").expect("bind 空闲回环");
    let bind_addr = free.local_addr().expect("local_addr");
    drop(free);

    let outcome = std::panic::catch_unwind(move || assert_port_not_listening(bind_addr, probe));
    assert!(
        outcome.is_err(),
        "[::1]:{port} 确有监听时，探针必须判红（探错地址族 ⇒ 本断言红）"
    );

    // 反向对照：撤掉监听后同一判据不得假红（证明牙不是恒红）。
    drop(listener);
    assert_port_not_listening(bind_addr, probe);
}

/// S2 接线牙（调用计数）：拒绝启动的**真实路径**必须调用「无监听启动行」判据。
///
/// 合成牙只证明 helper 自身能红；本用例跑一次真实拒绝启动（缺 token），
/// 用调用计数证明 `assert_startup_rejected` 的拒绝路径确实调用了判据。
/// 删掉接线（或把它挪到不执行的支路）⇒ 计数不增长 ⇒ 红。
#[test]
fn startup_rejection_path_invokes_listening_line_criterion() {
    let port = ephemeral_port();
    let bind = format!("127.0.0.1:{port}");
    let before = listening_line_check_count();
    assert_startup_rejected(
        &["--transport", "http", "--bind", bind.as_str()],
        &[],
        "token",
    );
    assert_eq!(
        listening_line_check_count(),
        before + 1,
        "拒绝启动路径必须调用一次「无监听启动行」判据（删接线 ⇒ 本用例红）"
    );
}

/// S2 接线牙（真生产启动行）：把真实生产格式的监听启动行喂进拒绝路径的**同一判据**，
/// 断言它判红；去掉启动行后同一判据不假红（反向对照）。
///
/// 与上一条用例合起来：上面钉「接线存在」，本用例钉「接线后的判据仍然生效」。
#[test]
fn startup_rejection_path_rejects_real_listening_line() {
    // rc=2 + 关键字 + 无 token 全部成立，唯一不满足的是「无监听启动行」。
    let with_listening_line = StartupObservation {
        code: Some(2),
        stderr: format!(
            "{LISTENING_PREFIX}http://127.0.0.1:1234/mcp（只回环，Bearer token 已配置）\n\
             mir2-mcp: --bind 不是回环地址；拒绝启动"
        ),
    };
    let outcome =
        std::panic::catch_unwind(|| assert_startup_rejection(&with_listening_line, "回环"));
    assert!(
        outcome.is_err(),
        "带真实生产监听启动行的拒绝观测必须判红（判据未接线或已被放宽）"
    );

    // 反向对照：同一条观测去掉启动行 ⇒ 不假红。
    let without_listening_line = StartupObservation {
        code: Some(2),
        stderr: "mir2-mcp: --bind 不是回环地址；拒绝启动".to_string(),
    };
    assert_startup_rejection(&without_listening_line, "回环");
}

/// `assert_port_not_listening` 的「回收 bind」分支负例：
/// 探针连不上、但 `bind_addr` 被占用 ⇒ 判据必须判红（删掉回收断言 ⇒ 本用例红）。
///
/// 夹具把 probe 放在**另一条**空闲回环上：connect 判据不会先红，
/// 唯一能判红的就是回收 bind 分支。
#[test]
fn port_probe_rejects_when_reclaimed_bind_fails() {
    // bind_addr 被另一个监听者占用。
    let occupied = TcpListener::bind("127.0.0.2:0").expect("占位监听");
    let bind_addr = occupied.local_addr().expect("local_addr");
    // probe 是一条确定空闲的回环端口（先 bind 取端口再释放）。
    let free = TcpListener::bind("127.0.0.3:0").expect("取空闲端口");
    let probe = free.local_addr().expect("local_addr");
    drop(free);
    assert!(
        TcpStream::connect_timeout(&probe, Duration::from_millis(250)).is_err(),
        "夹具前提：{probe} 必须没有监听"
    );

    let outcome = std::panic::catch_unwind(move || assert_port_not_listening(bind_addr, probe));
    assert!(
        outcome.is_err(),
        "{bind_addr} 被占用时，回收 bind 判据必须判红（删掉该断言 ⇒ 本用例红）"
    );

    // 反向对照：释放占位后同一判据不假红。
    drop(occupied);
    assert_port_not_listening(bind_addr, probe);
}

/// helper 回落（Info-3）：`--bind` 解析失败时不得产生探测目标，更不得回落缺省 7950。
#[test]
fn unparseable_bind_has_no_probe_target() {
    assert_eq!(
        bind_addr_from_args(&["--transport", "http", "--bind", "localhost:7950"]),
        None,
        "`localhost:P` 不是 SocketAddr 字面量"
    );
    assert_eq!(
        probe_target_from_args(&["--transport", "http", "--bind", "localhost:7950"]),
        None,
        "解析失败 ⇒ 不探测（绝不回落缺省 7950）"
    );
    assert_eq!(
        probe_target_from_args(&["--transport", "http", "--bind", "192.0.2.1:7950"]),
        None,
        "具名非回环地址不做端口探测"
    );
    assert_eq!(
        probe_target_from_args(&["--transport", "http", "--bind", "127.0.0.1:7950"]),
        Some("127.0.0.1:7950".parse().expect("addr"))
    );
}

/// helper 回落（Info-3）行为牙：7950 **确有监听** 时，`--bind localhost:P` 的拒绝启动
/// 不得被误判成「仍在监听」。
///
/// 旧实现解析失败后回落 `127.0.0.1:7950` 并探测：7950 被占就会假红（rc=2 本已成立）。
/// 新实现跳过端口探测，只保留与端口无关的判据。
#[test]
fn unparseable_bind_is_not_judged_against_occupied_7950() {
    // 夹具：127.0.0.1:7950 必须处于监听状态。优先自己绑定；若已被别人占用（同样满足前提），
    // 用 connect 自检确认。两种情况都能让「旧回落探测」判红。
    let owned = TcpListener::bind(("127.0.0.1", 7950)).ok();
    assert!(
        TcpStream::connect_timeout(
            &"127.0.0.1:7950".parse().expect("addr"),
            Duration::from_millis(750)
        )
        .is_ok(),
        "夹具前提：127.0.0.1:7950 必须处于监听状态"
    );

    let port = ephemeral_port();
    let bind = format!("localhost:{port}");
    assert_startup_rejected(
        &["--transport", "http", "--bind", bind.as_str()],
        &[("MIR2_MCP_HTTP_TOKEN", TOKEN)],
        "IP 字面量",
    );
    drop(owned);
}

// ---------------------------------------------------------------------------
// A4：401 且零触达工具处理器
// ---------------------------------------------------------------------------

#[test]
fn http_auth_rejects_and_never_touches_tool_handlers() {
    let root = TempRoot::new("auth");
    let server = HttpServer::start(&root.path, true);

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
    let server = HttpServer::start(&root.path, false);

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
        "--max-connections",
        "--header-timeout-ms",
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

// ---------------------------------------------------------------------------
// F1：bind 的 IP 字面量必须进 allowed_hosts（127.0.0.2 不再每请求 403）
// ---------------------------------------------------------------------------

#[test]
fn loopback_bind_ip_is_accepted_by_host_validation() {
    let root = TempRoot::new("hostbind");
    // 127.0.0.2 与 127.0.0.1 同属 127.0.0.0/8：修好前 127.0.0.2 能监听但每个请求都 403。
    let server_alt = HttpServer::start_with(&root.path, "127.0.0.2", &[], false);
    let server_std = HttpServer::start_with(&root.path, "127.0.0.1", &[], false);
    assert_eq!(
        server_alt.addr.ip().to_string(),
        "127.0.0.2",
        "必须真的从 127.0.0.2 监听"
    );
    assert_eq!(server_std.addr.ip().to_string(), "127.0.0.1");

    let mut alt = HttpMcp::new(server_alt.addr);
    let alt_init = alt.initialize();
    alt.notify_initialized();
    let alt_list = alt.call_tool("gm_list", json!({}));

    let mut std = HttpMcp::new(server_std.addr);
    let std_init = std.initialize();
    std.notify_initialized();
    let std_list = std.call_tool("gm_list", json!({}));

    assert_eq!(
        serde_json::to_string(&alt_init).expect("canonical"),
        serde_json::to_string(&std_init).expect("canonical"),
        "两个回环 bind 的 initialize result 必须逐字节相等"
    );
    assert_eq!(
        serde_json::to_string(structured(&alt_list)).expect("canonical"),
        serde_json::to_string(structured(&std_list)).expect("canonical"),
        "跨 bind 地址的 structuredContent 必须逐字节相等"
    );

    // 负例：不在白名单里的 Host 仍必须 403——不能靠「清空 allowed_hosts」蒙混过关。
    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
    }))
    .expect("serialize");
    let auth = format!("Bearer {TOKEN}");
    let evil = http_request_with_host(
        server_alt.addr,
        "evil.example",
        Some(auth.as_str()),
        alt.session.as_deref(),
        &body,
    );
    assert_eq!(
        evil.status, 403,
        "非白名单 Host 必须被拒（Host 校验不能放宽）"
    );
}

// ---------------------------------------------------------------------------
// Minor-1：带 Origin 的请求必须 403（把原先只靠人工实测的口径变成 e2e 断言）
// ---------------------------------------------------------------------------

#[test]
fn origin_header_is_rejected_with_403() {
    let root = TempRoot::new("origin");
    let server = HttpServer::start(&root.path, false);
    let auth = format!("Bearer {TOKEN}");
    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
    }))
    .expect("serialize");

    // 对照组：同一请求**不带 Origin** ⇒ token 已通过、落到会话校验（400），
    // 证明后面的 403 是 Origin 造成的，而不是鉴权/会话。
    let without_origin = http_request(server.addr, Some(auth.as_str()), None, &body);
    assert_eq!(
        without_origin.status, 400,
        "无 Origin 时应进到会话校验（400），而不是 403"
    );

    let evil = http_request_with_origin(
        server.addr,
        &server.addr.to_string(),
        Some("http://evil.example"),
        Some(auth.as_str()),
        None,
        &body,
    );
    assert_eq!(evil.status, 403, "带 Origin 的请求必须 403");
    let text = String::from_utf8_lossy(&evil.body);
    assert!(
        text.contains("Forbidden: Origin header is not allowed"),
        "响应体必须点名 Origin：{text:?}"
    );
}

// ---------------------------------------------------------------------------
// F2：请求头读超时（慢 header 连接被关闭，正常请求不受影响）
// ---------------------------------------------------------------------------

#[test]
fn slow_header_connection_is_closed_within_timeout() {
    let root = TempRoot::new("slowheader");
    let server = HttpServer::start_with(
        &root.path,
        "127.0.0.1",
        &["--header-timeout-ms", "300"],
        false,
    );

    let mut slow = TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).expect("连接");
    slow.set_read_timeout(Some(Duration::from_secs(10)))
        .expect("读超时");
    slow.write_all(format!("POST /mcp HTTP/1.1\r\nHost: {}\r\n", server.addr).as_bytes())
        .expect("写半截请求头");
    slow.flush().expect("flush");
    let started = Instant::now();
    // 必须读到 **EOF**（或等价的 RST）才算「连接已关闭」：只读到任意字节
    // （408/400 响应体）不能证明服务端关掉了连接。
    let mut buffer = [0u8; 64];
    let mut received = 0usize;
    loop {
        match slow.read(&mut buffer) {
            Ok(0) => break, // EOF：服务端已关闭连接
            Ok(read) => received += read,
            Err(error)
                if error.kind() == std::io::ErrorKind::ConnectionReset
                    || error.kind() == std::io::ErrorKind::ConnectionAborted =>
            {
                break; // RST 是「关闭」的等价形态
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                panic!(
                    "300ms 请求头读超时窗口内连接必须被关闭，但读到 {received} 字节后仍在等待：{error}"
                );
            }
            Err(error) => panic!("读慢 header 连接失败：{error}"),
        }
    }
    let elapsed = started.elapsed();
    // 只卡宽松下界（「不是瞬间被关」）；**不卡墙钟上界**（负载会让上界 flaky）。
    assert!(
        elapsed >= Duration::from_millis(1),
        "下界（宽松）：连接不是瞬间被关：{elapsed:?}"
    );

    // 同一进程内紧邻跑一次完整握手：超时只针对慢 header，不影响正常请求。
    let mut client = HttpMcp::new(server.addr);
    client.initialize();
    client.notify_initialized();
    let listed = client.call_tool("gm_list", json!({}));
    assert_eq!(
        structured(&listed).get("ok").and_then(Value::as_bool),
        Some(true)
    );
}

// ---------------------------------------------------------------------------
// F2：并发连接上限（--max-connections 2：第 3 条被立即关闭，前 2 条仍可用）
// ---------------------------------------------------------------------------

/// 在（HTTP/1.1，缺省 keep-alive）连接上发一条完整请求。
fn send_http11_request(stream: &mut TcpStream, addr: SocketAddr, body: &str) {
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("写请求");
    stream.flush().expect("flush");
}

/// 在 keep-alive 连接上读一条完整响应（按 Content-Length 定界；无长度则读到 EOF/超时）。
fn read_one_response(stream: &mut TcpStream) -> HttpResponse {
    let mut raw: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..split]).to_string();
            let content_length = head.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            });
            let body_start = split + 4;
            if let Some(length) = content_length {
                if raw.len() >= body_start + length {
                    break;
                }
            } else if head.lines().any(|line| {
                line.to_ascii_lowercase()
                    .contains("transfer-encoding: chunked")
            }) && raw[body_start..]
                .windows(5)
                .any(|window| window == b"0\r\n\r\n")
            {
                break;
            }
        }
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => raw.extend_from_slice(&buffer[..read]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::ConnectionReset
                    || error.kind() == std::io::ErrorKind::ConnectionAborted =>
            {
                // 无长度响应用「关闭连接」定界时，读侧看到的是 RST/EOF。
                break;
            }
            Err(error) => panic!("读响应失败：{error}"),
        }
    }
    parse_response(&raw)
}

#[test]
fn connection_limit_rejects_excess_and_keeps_existing_usable() {
    let root = TempRoot::new("conncap");
    let server =
        HttpServer::start_with(&root.path, "127.0.0.1", &["--max-connections", "2"], false);
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

    let mut first =
        TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).expect("连接 first");
    let mut second =
        TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).expect("连接 second");
    for stream in [&mut first, &mut second] {
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("读超时");
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .expect("写超时");
        send_http11_request(stream, server.addr, body);
    }
    // 两条都拿到响应 ⇒ 服务端确实在处理它们；HTTP/1.1 缺省 keep-alive ⇒ 各占一个并发名额。
    for stream in [&mut first, &mut second] {
        let response = read_one_response(stream);
        assert_eq!(response.status, 401, "无 token 必须 401");
    }

    // 第 3 条连接：必须被立即关闭（不是排队、不是回包）。
    let mut probe =
        TcpStream::connect_timeout(&server.addr, Duration::from_secs(2)).expect("连接 probe");
    probe
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("probe 读超时");
    let mut buffer = [0u8; 16];
    let probe_result = match probe.read(&mut buffer) {
        Ok(0) => Ok(()),
        Err(error)
            if error.kind() == std::io::ErrorKind::ConnectionReset
                || error.kind() == std::io::ErrorKind::ConnectionAborted =>
        {
            Ok(())
        }
        other => Err(other),
    };
    assert!(
        probe_result.is_ok(),
        "并发上限 2 已满时第 3 条连接必须被立即关闭：{probe_result:?}"
    );

    // 前 2 条仍可用：同一条连接上再走一次请求 ⇒ 仍 401（上限只拦新连接）。
    send_http11_request(&mut first, server.addr, body);
    let again = read_one_response(&mut first);
    assert_eq!(again.status, 401, "上限只拦新连接，既有连接必须继续可用");
}

// ---------------------------------------------------------------------------
// F3：启动行解析（端口 0 ⇒ 解析实际端口；退化形态必须拒绝）
// ---------------------------------------------------------------------------

#[test]
fn listening_line_parses_assigned_port() {
    let addr = parse_listening_addr(
        "mir2-mcp: HTTP 传输监听 http://127.0.0.1:53211/mcp（只回环，Bearer token 已配置；并发上限 64，请求头超时 10000ms）",
    )
    .expect("启动行必须能解析出端口");
    assert_eq!(addr, "127.0.0.1:53211".parse().expect("addr"));

    let ipv6 = parse_listening_addr(
        "mir2-mcp: HTTP 传输监听 http://[::1]:7950/mcp（只回环，Bearer token 已配置）",
    )
    .expect("IPv6 回环启动行也必须能解析");
    assert_eq!(ipv6.port(), 7950);
    assert!(ipv6.ip().is_loopback());

    // 退化形态：前缀不符 / 非回环 / 端口非法 ⇒ None（绝不把非回环当成自己人）。
    assert!(parse_listening_addr("mir2-mcp: 其它行").is_none());
    assert!(
        parse_listening_addr("mir2-mcp: HTTP 传输监听 http://0.0.0.0:7950/mcp").is_none(),
        "非回环地址必须拒绝"
    );
    assert!(
        parse_listening_addr("mir2-mcp: HTTP 传输监听 http://127.0.0.1:notaport/mcp").is_none(),
        "非法端口必须拒绝"
    );
    assert!(parse_listening_addr("mir2-mcp: HTTP 传输监听 https://127.0.0.1:1/mcp").is_none());
}

// ---------------------------------------------------------------------------
// F4：没有会话的 tools/list 是 400（文档自检不再写错成 200）
// ---------------------------------------------------------------------------

#[test]
fn tools_list_without_session_is_rejected_until_initialize() {
    let root = TempRoot::new("session");
    let server = HttpServer::start(&root.path, false);
    let auth = format!("Bearer {TOKEN}");
    let body = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
    }))
    .expect("serialize");

    // 带正确 token、但没有 Mcp-Session-Id（也没 initialize）⇒ 400 + 明确文案。
    let no_session = http_request(server.addr, Some(auth.as_str()), None, &body);
    assert_eq!(no_session.status, 400, "无会话的 tools/list 必须 400");
    let text = String::from_utf8_lossy(&no_session.body);
    assert!(
        text.contains("Session ID is required"),
        "响应体必须点名缺会话：{text:?}"
    );

    // 完整握手后同一调用 ⇒ 200（文档自检第 5 步）。
    let mut client = HttpMcp::new(server.addr);
    client.initialize();
    client.notify_initialized();
    let with_session = http_request(
        server.addr,
        Some(auth.as_str()),
        client.session.as_deref(),
        &body,
    );
    assert_eq!(with_session.status, 200, "带会话的 tools/list 必须 200");
}
