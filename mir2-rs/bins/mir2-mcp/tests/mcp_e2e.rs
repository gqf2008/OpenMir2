//! MCP 协议层端到端自测：真的起 `mir2-mcp` 子进程，走 stdio 上的 JSON-RPC。
//!
//! 覆盖卡面 M1-2（initialize + tools/list ≥6 + tools/call）与 M1-3/M1-5 的端到端面：
//! `gm_list` 条数断言（= gm_registry.json 的 table_total）、`conf_get`/`conf_set`（dry_run 零写、
//! 真实写的备份/回读）、`gm_execute` 的结构化错误，以及经 game-svr 本机 admin 通道
//! （进程内假通道）的原样透传。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-11-25";

struct McpProc {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: u64,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl McpProc {
    fn start(root: Option<&Path>, env_root: Option<&Path>) -> Self {
        Self::start_with_env(root, env_root, &[])
    }

    /// 同 [`Self::start`]，额外注入环境变量（`gm_execute` 的 `MIR2_MCP_ADMIN_ADDR` 等）。
    fn start_with_env(
        root: Option<&Path>,
        env_root: Option<&Path>,
        extra_env: &[(&str, String)],
    ) -> Self {
        let exe = env!("CARGO_BIN_EXE_mir2-mcp");
        let mut command = Command::new(exe);
        if let Some(root) = root {
            command.arg("--root").arg(root);
        }
        if let Some(env_root) = env_root {
            command.env("MIR2_MCP_ROOT", env_root);
        }
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn mir2-mcp");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let lines = BufReader::new(stdout).lines();
            for line in lines {
                let Ok(line) = line else { break };
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

    fn notify(&mut self, method: &str) {
        self.send(&json!({"jsonrpc": "2.0", "method": method}));
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.wait_for_id(id)
    }

    fn wait_for_id(&mut self, id: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "等 JSON-RPC 响应 id={id} 超时");
            let line = self
                .rx
                .recv_timeout(remaining)
                .unwrap_or_else(|_| panic!("等 JSON-RPC 响应 id={id} 超时"));
            let value: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("服务端输出不是 JSON：{e}；原文：{line}"));
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }

    fn shutdown(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => {
                    assert!(status.success(), "子进程非零退出：{status}");
                    break;
                }
                None => {
                    if Instant::now() >= deadline {
                        let _ = self.child.kill();
                        let _ = self.child.wait();
                        panic!("关闭 stdin 后子进程 30s 内未退出");
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for McpProc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "mir2-mcp-e2e-{tag}-{}-{}",
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

    fn conf_path(&self) -> PathBuf {
        self.path.join("M2GameSvr").join("Server.conf")
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn structured(response: &Value) -> &Value {
    response
        .get("result")
        .and_then(|r| r.get("structuredContent"))
        .unwrap_or_else(|| panic!("响应缺 result.structuredContent：{response}"))
}

#[test]
fn mcp_stdio_end_to_end() {
    let root = TempRoot::new("main");
    let mut proc = McpProc::start(Some(&root.path), None);

    // 1) initialize
    let init = proc.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "mir2-mcp-e2e", "version": "0"},
        }),
    );
    let result = init.get("result").expect("initialize 必须有 result");
    assert_eq!(
        result.get("protocolVersion").and_then(Value::as_str),
        Some(PROTOCOL_VERSION)
    );
    assert!(result.get("capabilities").is_some());
    proc.notify("notifications/initialized");

    // 2) tools/list：六个工具全在
    let listed = proc.request("tools/list", json!({}));
    let tools = listed["result"]["tools"]
        .as_array()
        .expect("tools 必须是数组");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str))
        .collect();
    for expected in [
        "gm_list",
        "gm_help",
        "conf_get",
        "conf_keys",
        "conf_set",
        "gm_execute",
    ] {
        assert!(
            names.contains(&expected),
            "tools/list 缺 {expected}：{names:?}"
        );
    }
    assert!(tools.len() >= 6, "工具数必须 ≥6，实际 {}", tools.len());

    // 3) tools/call gm_list：条数断言 = gm_registry.json 的 table_total
    let list = proc.call_tool("gm_list", json!({}));
    let body = structured(&list);
    assert_eq!(body["catalog_total"], 185);
    assert_eq!(body["table_total"], 185);
    assert_eq!(
        body["commands"].as_array().expect("commands 数组").len(),
        185
    );
    assert!(body["summary"]["table_imp_breakdown"]["Implemented"]
        .as_u64()
        .is_some_and(|n| n > 0));
    let tingwa = body["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Tingwa")
        .expect("Tingwa 必须在目录里");
    assert_eq!(tingwa["family"], "传送/召唤");
    assert_eq!(tingwa["implemented"], true);

    // 4) tools/call conf_get（tempdir 假 root）
    let got = proc.call_tool("conf_get", json!({"section": "setup", "key": "genmonrate"}));
    let body = structured(&got);
    assert_eq!(body["found"], true);
    assert_eq!(body["value"], "10");
    assert_eq!(body["ok"], true);

    // 5) tools/call gm_help
    let help = proc.call_tool("gm_help", json!({"name": "清理垃圾"}));
    let body = structured(&help);
    assert_eq!(body["handler_class"], "ClearBagItemCommand");
    assert_eq!(body["desc"], "清理包裹物品");

    // 6) tools/call conf_set dry_run：零写
    let before = std::fs::read(root.conf_path()).expect("read conf");
    let dry = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "20", "dry_run": true}),
    );
    let body = structured(&dry);
    assert_eq!(body["ok"], true);
    assert_eq!(body["dry_run"], true);
    assert_eq!(body["wrote"], false);
    assert_eq!(body["changed"], true);
    assert_eq!(body["before_line"], "GenMonRate=10");
    assert_eq!(body["after_line"], "GenMonRate=20");
    assert_eq!(
        std::fs::read(root.conf_path()).expect("read conf"),
        before,
        "dry_run 零写"
    );

    // 7) tools/call conf_set 真写：备份 + 回读
    let set = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "25"}),
    );
    let body = structured(&set);
    assert_eq!(body["ok"], true);
    assert_eq!(body["wrote"], true);
    assert_eq!(body["changed"], true);
    assert_eq!(body["readback_value"], "25");
    assert_ne!(body["before_sha"], body["after_sha"]);
    let backup = body["backup_path"].as_str().expect("backup_path");
    assert!(Path::new(backup).exists(), "备份文件必须存在：{backup}");
    assert_eq!(
        std::fs::read(backup).expect("read backup"),
        before,
        "备份 = 写前内容"
    );

    let after = proc.call_tool("conf_get", json!({"section": "Setup", "key": "GenMonRate"}));
    assert_eq!(structured(&after)["value"], "25");

    // 8) tools/call gm_execute：本夹具未配置 [Admin] AdminPort ⇒ 结构化 admin_not_configured
    //    （isError，不 panic、不伪造执行）。
    let exec = proc.call_tool("gm_execute", json!({"chr": "aaa", "cmd": "@时间"}));
    assert_eq!(exec["result"]["isError"], true);
    let body = structured(&exec);
    assert_eq!(body["code"], "admin_not_configured");
    assert_eq!(body["ok"], false);
    assert_eq!(body["wrote"], false);

    proc.shutdown();
}

/// root 解析：`--root` > env `MIR2_MCP_ROOT`（卡面「默认 E:/MirServer」不测——那会读真机生产文件）。
#[test]
fn root_flag_wins_over_env_and_env_is_used_without_flag() {
    let flag_root = TempRoot::new("prec-flag"); // GenMonRate=10
    let env_root = TempRoot::new("prec-env");
    let mut env_conf = Vec::from(&b"\xEF\xBB\xBF"[..]);
    env_conf.extend_from_slice(b"[Setup]\r\nGenMonRate=77\r\n");
    std::fs::write(env_root.conf_path(), env_conf).expect("写 env 夹具");

    let mut with_flag = McpProc::start(Some(&flag_root.path), Some(&env_root.path));
    assert_eq!(lookup_rate(&mut with_flag), "10", "--root 必须压过 env");
    with_flag.shutdown();

    let mut with_env = McpProc::start(None, Some(&env_root.path));
    assert_eq!(lookup_rate(&mut with_env), "77", "无 --root 时读 env");
    with_env.shutdown();
}

fn lookup_rate(proc: &mut McpProc) -> String {
    proc.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "mir2-mcp-e2e", "version": "0"},
        }),
    );
    proc.notify("notifications/initialized");
    let got = proc.call_tool("conf_get", json!({"section": "Setup", "key": "GenMonRate"}));
    structured(&got)["value"]
        .as_str()
        .expect("value 必须是字符串")
        .to_string()
}

/// 卡 `openmir2-mcp-gm-execute`（T-273）：`gm_execute` 的默认地址来自
/// `Server.conf [Admin] AdminPort`；请求 `{chr,cmd}` 原样发给通道，响应原样透传。
#[test]
fn gm_execute_roundtrips_through_admin_channel() {
    use std::net::TcpListener;

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fake admin");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        let (mut stream, peer) = listener.accept().expect("accept");
        assert!(peer.ip().is_loopback(), "客户端必须来自回环：{peer}");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("读请求");
        let req: Value = serde_json::from_str(line.trim()).expect("请求必须是 JSON");
        assert_eq!(req["chr"], "aaa", "{req}");
        assert_eq!(req["cmd"], "@时间", "{req}");
        writeln!(
            stream,
            "{}",
            json!({
                "ok": true,
                "error": Value::Null,
                "frames": 1,
                "note": "当前日期时间: 2026-11-01 00:00:00",
            })
        )
        .expect("写响应");
        stream.flush().expect("flush");
    });

    let root = TempRoot::new("admin-ok");
    // 默认地址 = 127.0.0.1:<Server.conf [Admin] AdminPort>（不设 MIR2_MCP_ADMIN_ADDR）。
    let mut conf = Vec::from(&b"\xEF\xBB\xBF"[..]);
    conf.extend_from_slice(
        format!("[Setup]\r\nGenMonRate=10\r\n[Admin]\r\nAdminPort={port}\r\n").as_bytes(),
    );
    std::fs::write(root.conf_path(), conf).expect("写 [Admin] 夹具");

    let mut proc = McpProc::start(Some(&root.path), None);
    proc.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "mir2-mcp-e2e", "version": "0"},
        }),
    );
    proc.notify("notifications/initialized");
    let got = proc.call_tool("gm_execute", json!({"chr": "aaa", "cmd": "@时间"}));
    let body = structured(&got);
    assert_eq!(body["ok"], true, "{got}");
    assert_eq!(body["frames"], 1, "{got}");
    assert_eq!(body["chr"], "aaa");
    assert_eq!(body["cmd"], "@时间");
    assert!(
        body["note"]
            .as_str()
            .is_some_and(|n| n.contains("当前日期时间")),
        "{got}"
    );
    proc.shutdown();
    server.join().expect("fake admin 线程");
}

/// 失败面全是结构化错误码：缺 `chr` / 非 `@` 命令行 / 通道连不上（不 panic、不静默）。
#[test]
fn gm_execute_errors_are_structured() {
    // TCP 端口 0 只用于 bind 时请求内核分配临时端口，不能作为已分配端口被监听；
    // 直接连接它可稳定走 Connect 失败，不做「先占用再释放」的 TOCTOU 端口猜测。
    let addr = "127.0.0.1:0".to_string();

    let root = TempRoot::new("admin-err");
    let mut proc =
        McpProc::start_with_env(Some(&root.path), None, &[("MIR2_MCP_ADMIN_ADDR", addr)]);
    proc.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "mir2-mcp-e2e", "version": "0"},
        }),
    );
    proc.notify("notifications/initialized");

    let missing = proc.call_tool("gm_execute", json!({"cmd": "@时间"}));
    assert_eq!(missing["result"]["isError"], true, "{missing}");
    assert_eq!(structured(&missing)["code"], "missing_chr");

    let bad_line = proc.call_tool("gm_execute", json!({"chr": "aaa", "cmd": "hello"}));
    assert_eq!(structured(&bad_line)["code"], "invalid_command");

    let unreachable = proc.call_tool("gm_execute", json!({"chr": "aaa", "cmd": "@时间"}));
    assert_eq!(unreachable["result"]["isError"], true, "{unreachable}");
    assert_eq!(structured(&unreachable)["code"], "admin_connect_failed");

    proc.shutdown();
}

/// F4（卡 `openmir2-mcp-admin-review-minors`）：`MIR2_MCP_ADMIN_ADDR` 指向非回环地址时，
/// `gm_execute` 必须在**连接前**结构化拒绝（`admin_addr_not_loopback`），不 panic、不伪造执行。
///
/// `192.0.2.1` 是 TEST-NET-1（RFC 5737），永不指向真实主机。
///
/// 变异自证：删掉客户端回环校验（`admin::resolve_loopback` 的 `.find(|c| c.ip().is_loopback())`
/// 换成 `.next()`）⇒ 会真去连 192.0.2.1 ⇒ 错误码变成 `admin_timeout`
/// （本机实测；不同网络下也可能是 `admin_connect_failed`）⇒ 本用例红。
#[test]
fn gm_execute_rejects_non_loopback_admin_addr() {
    let root = TempRoot::new("admin-nonloopback");
    let mut proc = McpProc::start_with_env(
        Some(&root.path),
        None,
        &[("MIR2_MCP_ADMIN_ADDR", "192.0.2.1:7946".to_string())],
    );
    proc.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "mir2-mcp-e2e", "version": "0"},
        }),
    );
    proc.notify("notifications/initialized");

    let got = proc.call_tool("gm_execute", json!({"chr": "aaa", "cmd": "@时间"}));
    assert_eq!(got["result"]["isError"], true, "{got}");
    let body = structured(&got);
    assert_eq!(body["code"], "admin_addr_not_loopback", "{got}");
    assert_eq!(body["ok"], false, "{got}");
    assert_eq!(body["wrote"], false, "{got}");
    assert_eq!(body["admin_addr"], "192.0.2.1:7946", "{got}");

    proc.shutdown();
}
