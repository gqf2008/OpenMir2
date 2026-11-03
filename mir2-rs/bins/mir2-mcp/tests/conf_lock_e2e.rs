//! `conf_set` 跨进程锁的端到端自测（卡 `openmir2-mcp-conf-set-lock`）。
//!
//! 真的起多个 `mir2-mcp` 子进程（stdio JSON-RPC），对同一个 `<root>/M2GameSvr/Server.conf`
//! 并发执行 `conf_set`。覆盖卡面 A1（并发互斥）、A2（锁冲突有界 + 结构化错误）、
//! A4（残留锁自愈 / 活锁不得被抢）。
//!
//! 用例只依赖公开行为（工具回包 + 盘上文件），不读 `conf.rs` 内部状态；并发窗口用生产代码里
//! 文档化的测试接缝 `MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS` 放大（未设置时为空操作）。

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-11-25";

/// 每个进程的写入轮数（卡面要求 ≥50）。
const ROUNDS_PER_PROCESS: usize = 50;
/// 测试接缝：rename 与回读之间停住多久（毫秒），放大互滚窗口。
const HOLD_AFTER_RENAME_MS: u64 = 60;

// ---------------------------------------------------------------------------
// MCP 子进程（与 mcp_e2e.rs 同形态的最小副本）
// ---------------------------------------------------------------------------

struct McpProc {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: u64,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl McpProc {
    fn start(root: &Path, extra_env: &[(&str, String)]) -> Self {
        let exe = env!("CARGO_BIN_EXE_mir2-mcp");
        let mut command = Command::new(exe);
        command.arg("--root").arg(root);
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
            for line in BufReader::new(stdout).lines() {
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
        let deadline = Instant::now() + Duration::from_secs(60);
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

    fn initialize(&mut self) {
        self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "mir2-mcp-conf-lock-e2e", "version": "0"},
            }),
        );
        self.notify("notifications/initialized");
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

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

const FIXTURE: &[u8] = b"\xEF\xBB\xBF[DataBase]\r\nDbType=MySQL\r\n\r\n[Setup]\r\nMonsterPowerRate=10\r\nGenMonRate=10\r\n";

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(tag: &str) -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "mir2-mcp-conf-lock-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("M2GameSvr")).expect("mkdir");
        std::fs::write(path.join("M2GameSvr").join("Server.conf"), FIXTURE).expect("写夹具");
        Self { path }
    }

    fn conf_path(&self) -> PathBuf {
        self.path.join("M2GameSvr").join("Server.conf")
    }

    fn lock_path(&self) -> PathBuf {
        self.path.join("M2GameSvr").join("Server.conf.lock")
    }

    fn dir_entries(&self) -> Vec<String> {
        std::fs::read_dir(self.path.join("M2GameSvr"))
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    fn residue(&self) -> Vec<String> {
        self.dir_entries()
            .into_iter()
            .filter(|n| n.contains(".lock") || n.ends_with(".tmp") || n.contains(".stale"))
            .collect()
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn lock_body(pid: u32, acquired_ms: u64) -> String {
    format!("pid={pid}\nacq_ms={acquired_ms}\nnonce=test-{pid}-{acquired_ms}\n")
}

/// 从文件字节里取 `GenMonRate=` 的值（夹具格式固定）。
fn genmonrate(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .lines()
        .find_map(|line| line.trim().strip_prefix("GenMonRate="))
        .map(|v| v.trim().to_string())
        .expect("夹具里必须有 GenMonRate")
}

/// 按夹具把 `GenMonRate` 替换成 `value` 后的完整文件字节（A1 断言「完整结果」）。
fn fixture_with_rate(value: &str) -> Vec<u8> {
    String::from_utf8_lossy(FIXTURE)
        .replace("GenMonRate=10", &format!("GenMonRate={value}"))
        .into_bytes()
}

fn write_rounds(proc: &mut McpProc, start: u32) -> Vec<Value> {
    let mut out = Vec::with_capacity(ROUNDS_PER_PROCESS);
    for i in 0..ROUNDS_PER_PROCESS {
        let value = (start + u32::try_from(i).expect("轮数适合 u32")).to_string();
        let response = proc.call_tool(
            "conf_set",
            json!({"section": "Setup", "key": "GenMonRate", "value": value}),
        );
        out.push(response);
    }
    out
}

// ---------------------------------------------------------------------------
// A1：并发互斥
// ---------------------------------------------------------------------------

/// 两个进程各写 50 轮同一个键；锁必须让「读 → 备份 → rename → 回读 → 回滚」整段串行。
///
/// 断言：所有回包 ok/wrote=true 且 readback 等于请求值（无互滚错误）；最终文件逐字节等于
/// 某一轮完整结果；目录里没有 `.tmp` / `.lock` / `.stale` 残留。
#[test]
fn concurrent_conf_set_is_serialized_across_processes() {
    let root = TempRoot::new("concurrent");
    let env = [
        (
            "MIR2_MCP_CONF_LOCK_TIMEOUT_MS",
            (ROUNDS_PER_PROCESS as u64 * HOLD_AFTER_RENAME_MS * 4 + 10_000).to_string(),
        ),
        (
            "MIR2_MCP_CONF_TEST_HOLD_AFTER_RENAME_MS",
            HOLD_AFTER_RENAME_MS.to_string(),
        ),
    ];

    let mut first = McpProc::start(&root.path, &env);
    let mut second = McpProc::start(&root.path, &env);
    first.initialize();
    second.initialize();

    // 两段互不重叠的值域（白名单上界 1000）：最终值可以唯一指认「最后一个写者」。
    let a = std::thread::spawn(move || {
        let responses = write_rounds(&mut first, 101);
        first.shutdown();
        responses
    });
    let b = std::thread::spawn(move || {
        let responses = write_rounds(&mut second, 201);
        second.shutdown();
        responses
    });
    let mut responses = a.join().expect("A 进程线程");
    responses.extend(b.join().expect("B 进程线程"));

    assert_eq!(responses.len(), ROUNDS_PER_PROCESS * 2);
    for response in &responses {
        let body = structured(response);
        assert_eq!(body["ok"], true, "并发写不得失败：{response}");
        assert_eq!(body["wrote"], true, "{response}");
        assert_eq!(body["changed"], true, "{response}");
        let value = body["new_value"].as_str().expect("new_value");
        assert_eq!(
            body["readback_value"], value,
            "回读必须是自己写的值：{response}"
        );
    }

    let final_bytes = std::fs::read(root.conf_path()).expect("读最终文件");
    let final_value = genmonrate(&final_bytes);
    let final_num: u32 = final_value.parse().expect("值必须是整数");
    assert!(
        (101..=150).contains(&final_num) || (201..=250).contains(&final_num),
        "最终值必须来自某一轮的完整结果：{final_value}"
    );
    assert_eq!(
        final_bytes,
        fixture_with_rate(&final_value),
        "最终文件必须逐字节等于「夹具 + 某一个完整值」，不得交错"
    );

    let residue = root.residue();
    assert!(
        residue.is_empty(),
        "结束时不得有锁/临时文件残留：{residue:?}"
    );
}

// ---------------------------------------------------------------------------
// A2：锁冲突有界 + 结构化错误
// ---------------------------------------------------------------------------

/// 另一进程持有**活锁**（pid = 本测试进程，仍在运行）时，`conf_set` 必须在有界时间内返回
/// `conf_lock_timeout`，且不改文件、不删/改写他人的锁、不 panic（进程随后仍能正常服务）。
#[test]
fn held_lock_times_out_with_structured_error() {
    let root = TempRoot::new("held");
    let before = std::fs::read(root.conf_path()).expect("读写前");
    let lock = root.lock_path();
    let body = lock_body(std::process::id(), now_ms());
    std::fs::write(&lock, &body).expect("写活锁");
    let lock_before = std::fs::read(&lock).expect("读锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "400".to_string()),
            // 阈值远大于锁年龄 ⇒ 纯「抢不到」，与残留接管路径无关。
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "30000".to_string()),
        ],
    );
    proc.initialize();

    let started = Instant::now();
    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "77"}),
    );
    let elapsed = started.elapsed();

    assert_eq!(response["result"]["isError"], true, "{response}");
    let body = structured(&response);
    assert_eq!(body["code"], "conf_lock_timeout", "{response}");
    assert_eq!(body["ok"], false, "{response}");
    assert_eq!(body["wrote"], false, "{response}");
    assert_eq!(body["timeout_ms"], 400, "{response}");
    assert!(
        elapsed >= Duration::from_millis(300),
        "必须有界等待而不是立即失败：{elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "超时必须有界：{elapsed:?}"
    );

    assert_eq!(
        std::fs::read(root.conf_path()).expect("读文件"),
        before,
        "锁冲突路径不得改文件"
    );
    assert_eq!(
        std::fs::read(&lock).expect("读锁"),
        lock_before,
        "不得删除/改写他人持有的活锁"
    );

    // 进程仍存活且服务正常（没有 panic/退出）。
    let got = proc.call_tool("conf_get", json!({"section": "Setup", "key": "GenMonRate"}));
    assert_eq!(structured(&got)["value"], "10", "{got}");
    proc.shutdown();
}

// ---------------------------------------------------------------------------
// A4：残留锁自愈 / 活锁不得被抢
// ---------------------------------------------------------------------------

/// 模拟「持有者被强杀留下锁文件」：pid 已死 + 时间戳超过 stale 阈值 ⇒ 后续 `conf_set`
/// 接管旧锁并成功写入，结束后锁文件被正常释放。
#[test]
fn dead_holder_lock_is_reclaimed_after_timeout() {
    let root = TempRoot::new("dead-holder");
    let dead = dead_pid();
    let lock = root.lock_path();
    std::fs::write(&lock, lock_body(dead, now_ms().saturating_sub(10_000))).expect("写残留锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "4000".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "500".to_string()),
        ],
    );
    proc.initialize();

    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "55"}),
    );
    let body = structured(&response);
    assert_eq!(body["ok"], true, "死锁必须可自愈：{response}");
    assert_eq!(body["wrote"], true, "{response}");
    assert_eq!(body["readback_value"], "55", "{response}");
    assert_eq!(
        genmonrate(&std::fs::read(root.conf_path()).expect("读")),
        "55"
    );
    assert!(!lock.exists(), "接管成功并写完后锁文件应被释放");
    assert!(root.residue().is_empty(), "残留：{:?}", root.residue());

    proc.shutdown();
}

/// 负例（证明「进程是否还活着」真的参与判定）：锁文件很老（远超 stale 阈值），但持有者
/// **仍存活** ⇒ 接管方必须超时失败，绝不能抢走活锁。
#[test]
fn live_holder_lock_is_never_stolen_even_if_old() {
    let root = TempRoot::new("live-holder");
    let before = std::fs::read(root.conf_path()).expect("读写前");
    let lock = root.lock_path();
    let body = lock_body(std::process::id(), now_ms().saturating_sub(60_000));
    std::fs::write(&lock, &body).expect("写老但活的锁");
    let lock_before = std::fs::read(&lock).expect("读锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "400".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "200".to_string()),
        ],
    );
    proc.initialize();

    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "66"}),
    );
    let body = structured(&response);
    assert_eq!(body["code"], "conf_lock_timeout", "{response}");
    assert_eq!(body["wrote"], false, "{response}");
    assert_eq!(
        std::fs::read(root.conf_path()).expect("读"),
        before,
        "活锁场景不得改文件"
    );
    assert_eq!(
        std::fs::read(&lock).expect("读锁"),
        lock_before,
        "活锁不得被接管/改写"
    );

    proc.shutdown();
}

// ---------------------------------------------------------------------------
// 跟进卡 `openmir2-mcp-conf-lock-edge-cases`：探测不可用 / pid 复用 / 残留清理
// ---------------------------------------------------------------------------

/// 在目录里按「名字含 `contains` 且以 `suffix` 结尾」找第一个文件。
fn find_residue(dir: &Path, contains: &str, suffix: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            name.contains(contains) && name.ends_with(suffix)
        })
}

/// A1：进程探测不可用（`Unknown`）⇒ 保守视为「活」，**不得**接管超龄残留锁。
///
/// 锁里的 pid 已死、年龄远超阈值；唯一让接管失效的是「探测不可用」。把 Unknown 改判成
/// Dead 会让本用例变红（变异自证见 patch body）。
#[test]
fn unknown_process_probe_never_takes_over_stale_lock() {
    let root = TempRoot::new("probe-unknown");
    let before = std::fs::read(root.conf_path()).expect("读写前");
    let lock = root.lock_path();
    let body = lock_body(dead_pid(), now_ms().saturating_sub(60_000));
    std::fs::write(&lock, &body).expect("写残留锁");
    let lock_before = std::fs::read(&lock).expect("读锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "400".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "100".to_string()),
            ("MIR2_MCP_CONF_TEST_PROCESS_PROBE", "unknown".to_string()),
        ],
    );
    proc.initialize();

    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "71"}),
    );
    assert_eq!(
        structured(&response)["code"],
        "conf_lock_timeout",
        "探测不可用时必须保守放行不了（不得接管）：{response}"
    );
    assert_eq!(
        std::fs::read(&lock).expect("读锁"),
        lock_before,
        "探测不可用时不得接管/改写残留锁"
    );
    assert_eq!(
        std::fs::read(root.conf_path()).expect("读文件"),
        before,
        "探测不可用时不得改文件"
    );

    proc.shutdown();
}

/// A2：pid 仍活但**启动时刻与锁里记录的不一致** ⇒ 判定 pid 复用（死痕迹），必须可回收。
#[test]
fn reused_pid_lock_is_reclaimed_when_start_time_differs() {
    let root = TempRoot::new("pid-reuse");
    let lock = root.lock_path();
    // 探测固定返回 `alive:999`；锁里记录的是另一时刻 `1` ⇒ 不是同一个进程。
    std::fs::write(
        &lock,
        format!(
            "pid={}\nacq_ms={}\nstart=1\nnonce=x\n",
            std::process::id(),
            now_ms().saturating_sub(10_000)
        ),
    )
    .expect("写复用 pid 的残留锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "4000".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "500".to_string()),
            ("MIR2_MCP_CONF_TEST_PROCESS_PROBE", "alive:999".to_string()),
        ],
    );
    proc.initialize();

    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "72"}),
    );
    let body = structured(&response);
    assert_eq!(body["ok"], true, "pid 复用的死痕迹必须可回收：{response}");
    assert_eq!(body["wrote"], true, "{response}");
    assert_eq!(body["readback_value"], "72", "{response}");
    assert!(!lock.exists(), "接管成功并写完后锁文件应被释放");

    proc.shutdown();
}

/// A2 负例：pid + 启动时刻都匹配 = 原持有者仍活 ⇒ 即使超龄也不得抢。
#[test]
fn live_holder_with_matching_start_time_is_not_stolen() {
    let root = TempRoot::new("pid-live-match");
    let before = std::fs::read(root.conf_path()).expect("读写前");
    let lock = root.lock_path();
    std::fs::write(
        &lock,
        format!(
            "pid={}\nacq_ms={}\nstart=999\nnonce=x\n",
            std::process::id(),
            now_ms().saturating_sub(60_000)
        ),
    )
    .expect("写活锁");
    let lock_before = std::fs::read(&lock).expect("读锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "400".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "100".to_string()),
            ("MIR2_MCP_CONF_TEST_PROCESS_PROBE", "alive:999".to_string()),
        ],
    );
    proc.initialize();

    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "73"}),
    );
    assert_eq!(
        structured(&response)["code"],
        "conf_lock_timeout",
        "启动时刻一致 ⇒ 是原持有者，不得接管：{response}"
    );
    assert_eq!(
        std::fs::read(&lock).expect("读锁"),
        lock_before,
        "活锁不得被接管/改写"
    );
    assert_eq!(std::fs::read(root.conf_path()).expect("读文件"), before);

    proc.shutdown();
}

/// A3：崩溃残留清理——死痕迹（pid 死 / pid 复用）的 `.tmp`、`.stale.*` 在下次写成功时
/// 被清掉；**活进程持有**的 `.tmp` 不得删（持有者退出后下一轮再清）。
#[test]
fn crash_residue_is_cleaned_but_live_holder_residue_is_kept() {
    let root = TempRoot::new("residue");
    let conf_dir = root.path.join("M2GameSvr");
    let conf_name = "Server.conf";
    let old = now_ms().saturating_sub(10_000);

    // 1) 活进程持有的 `.tmp`：崩溃接缝让子进程写完 tmp 后失败，子进程继续存活。
    let mut owner = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_TEST_FAIL_AFTER_TMP_WRITE", "1".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "300".to_string()),
        ],
    );
    owner.initialize();
    let response = owner.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "81"}),
    );
    assert_eq!(response["result"]["isError"], true, "{response}");
    let live_tmp =
        find_residue(&conf_dir, ".mir2-mcp.", ".tmp").expect("崩溃接缝必须留下 .tmp 残留");

    // 2) 伪造两类「确证已死」的残留：死 pid；以及 pid 活（本测试进程）但启动时刻不匹配。
    let dead = dead_pid();
    let dead_tmp = conf_dir.join(format!(".{conf_name}.mir2-mcp.{dead}.1.{old}.0.tmp"));
    std::fs::write(&dead_tmp, b"junk").expect("伪造死 pid .tmp");
    let reused_tmp = conf_dir.join(format!(
        ".{conf_name}.mir2-mcp.{}.1.{old}.0.tmp",
        std::process::id()
    ));
    std::fs::write(&reused_tmp, b"junk").expect("伪造 pid 复用 .tmp");
    let dead_stale = conf_dir.join(format!(".{conf_name}.lock.stale.{dead}.1.{old}.0"));
    // 内容也写 `start=1`：即使 pid 被后续进程复用，启动时刻不匹配仍判死痕迹。
    std::fs::write(
        &dead_stale,
        format!("pid={dead}\nacq_ms={old}\nstart=1\nnonce=x\n"),
    )
    .expect("伪造死 pid .stale");

    // 让活进程那份残留超过 stale 阈值（否则「保留」可能只是因为年龄不够）。
    std::thread::sleep(Duration::from_millis(500));

    // 3) 另一个进程写一次：清理死痕迹，但活进程持有的 tmp 保留。
    let mut cleaner = McpProc::start(
        &root.path,
        &[("MIR2_MCP_CONF_LOCK_STALE_MS", "300".to_string())],
    );
    cleaner.initialize();
    let response = cleaner.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "82"}),
    );
    assert_eq!(
        structured(&response)["ok"],
        true,
        "清理轮写成功：{response}"
    );
    assert!(!dead_tmp.exists(), "死 pid 的 .tmp 必须被清理");
    assert!(!reused_tmp.exists(), "pid 复用的 .tmp 必须被清理");
    assert!(!dead_stale.exists(), "死 pid 的 .stale 必须被清理");
    assert!(live_tmp.exists(), "活进程持有的 .tmp 不得删除");

    // 4) 持有者退出 ⇒ 变成死痕迹 ⇒ 下一轮写清掉。
    owner.shutdown();
    std::thread::sleep(Duration::from_millis(500));
    let response = cleaner.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "83"}),
    );
    assert_eq!(
        structured(&response)["ok"],
        true,
        "第二轮写成功：{response}"
    );
    assert!(!live_tmp.exists(), "持有者退出后 .tmp 必须被清理");
    cleaner.shutdown();

    let leftovers = root.residue();
    assert!(
        leftovers.is_empty(),
        "目录里只剩目标文件，残留：{leftovers:?}"
    );
}

/// ④ 并发接管：每轮留一个「死持有者 + 超龄」的锁，再让多个进程并发抢；N 轮后目录里
/// 只剩 `Server.conf`（没有 `.stale.*` / `.tmp` / `.lock` 残留）。
#[test]
fn concurrent_stale_takeover_leaves_only_target_file() {
    let root = TempRoot::new("takeover-rounds");
    const ROUNDS: usize = 5;
    const WRITERS: usize = 3;
    let env = [
        ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "8000".to_string()),
        ("MIR2_MCP_CONF_LOCK_STALE_MS", "100".to_string()),
    ];

    for round in 0..ROUNDS {
        // 死持有者 + 超龄；`start=1` 保证即使 pid 被新进程复用也判成死痕迹。
        let dead = dead_pid();
        let old = now_ms().saturating_sub(5_000);
        std::fs::write(
            root.lock_path(),
            format!("pid={dead}\nacq_ms={old}\nstart=1\nnonce=dead-{round}\n"),
        )
        .expect("写死锁");
        std::thread::sleep(Duration::from_millis(150));

        let writers: Vec<McpProc> = (0..WRITERS)
            .map(|_| {
                let mut proc = McpProc::start(&root.path, &env);
                proc.initialize();
                proc
            })
            .collect();
        let handles: Vec<_> = writers
            .into_iter()
            .enumerate()
            .map(|(idx, mut proc)| {
                let value = (101 + round * WRITERS + idx).to_string();
                std::thread::spawn(move || {
                    let response = proc.call_tool(
                        "conf_set",
                        json!({"section": "Setup", "key": "GenMonRate", "value": value}),
                    );
                    proc.shutdown();
                    response
                })
            })
            .collect();
        for handle in handles {
            let response = handle.join().expect("writer thread");
            let body = structured(&response);
            assert_eq!(body["ok"], true, "第 {round} 轮接管写必须成功：{response}");
        }

        let residue = root.residue();
        assert!(
            residue.is_empty(),
            "第 {round} 轮并发接管后不得有残留：{residue:?}"
        );
    }

    let mut entries = root.dir_entries();
    entries.sort();
    assert_eq!(
        entries,
        vec!["Server.conf".to_string()],
        "目录里只剩目标文件"
    );
}

/// 等待 marker 文件里出现某一行（按事件同步，而不是固定 sleep 对齐）。
fn wait_for_marker(path: &Path, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            if text.lines().any(|line| line.trim() == needle) {
                return;
            }
        }
        if Instant::now() >= deadline {
            let seen = std::fs::read_to_string(path).unwrap_or_default();
            panic!(
                "marker {} 未在 {timeout:?} 内出现 {needle:?}；已见：{seen:?}",
                path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn marker_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// ④ 三路竞态（放回成功路径）：判定「死锁」之后、rename 之前，原位置被**新锁**替换 ⇒
/// 搬走的是新锁，必须放回原处：不得删活锁，也不得留下 `.stale.*`。
///
/// A10 哨兵：必须看到 `hold-start`（接管者进入 rename 前窗口）与 `putback-restored`
/// （确实走到了「搬走替换锁 → 放回」分支）；任一个没出现，用例直接失败。
#[test]
fn takeover_race_restores_replacement_lock_without_residue() {
    let root = TempRoot::new("takeover-race");
    let marker = root.path.join("race-marker.log");
    let marker_arg = marker.display().to_string();
    let dead = dead_pid();
    let old = now_ms().saturating_sub(10_000);
    std::fs::write(
        root.lock_path(),
        format!("pid={dead}\nacq_ms={old}\nstart=1\nnonce=dead-race\n"),
    )
    .expect("写死锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "3000".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "100".to_string()),
            (
                "MIR2_MCP_CONF_TEST_HOLD_BEFORE_TAKEOVER_RENAME_MS",
                "500".to_string(),
            ),
            (
                "MIR2_MCP_CONF_TEST_TAKEOVER_RACE_MARKER",
                marker_arg.clone(),
            ),
        ],
    );
    proc.initialize();

    let handle = std::thread::spawn(move || {
        let response = proc.call_tool(
            "conf_set",
            json!({"section": "Setup", "key": "GenMonRate", "value": "74"}),
        );
        proc.shutdown();
        response
    });

    // 哨兵①：子进程已读完死锁、进入接缝睡眠；此刻把原位置的锁换成「活锁」。
    wait_for_marker(&marker, "hold-start", Duration::from_secs(15));
    let live_bytes = format!(
        "pid={}\nacq_ms={}\nnonce=live-replacement\n",
        std::process::id(),
        now_ms()
    )
    .into_bytes();
    std::fs::write(root.lock_path(), &live_bytes).expect("写活锁");

    // 哨兵②：确实走到了「搬走替换锁 → 放回」分支（没走到就失败）。
    wait_for_marker(&marker, "putback-restored", Duration::from_secs(15));

    let response = handle.join().expect("conf_set thread");
    assert_eq!(
        structured(&response)["code"],
        "conf_lock_timeout",
        "替换进来的活锁不得被接管：{response}"
    );
    assert_eq!(
        std::fs::read(root.lock_path()).expect("读锁"),
        live_bytes,
        "三路竞态搬走的新锁必须放回原处"
    );
    let stale: Vec<String> = root
        .dir_entries()
        .into_iter()
        .filter(|name| name.contains(".stale"))
        .collect();
    assert!(stale.is_empty(), "三路竞态不得留下 .stale.*：{stale:?}");
}

/// A1/A10：放回时 canonical 已被第三方活锁占用 ⇒ **必须放弃放回**，绝不 rename 覆盖。
/// 哨兵 `putback-abandoned` 证明走的是「canonical 被占」分支。
#[test]
fn takeover_race_never_overwrites_third_party_lock_on_putback() {
    let root = TempRoot::new("takeover-race-abandon");
    let marker = root.path.join("race-marker.log");
    let marker_arg = marker.display().to_string();
    let dead = dead_pid();
    let old = now_ms().saturating_sub(10_000);
    std::fs::write(
        root.lock_path(),
        format!("pid={dead}\nacq_ms={old}\nstart=1\nnonce=dead-race2\n"),
    )
    .expect("写死锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "3000".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "100".to_string()),
            (
                "MIR2_MCP_CONF_TEST_HOLD_BEFORE_TAKEOVER_RENAME_MS",
                "400".to_string(),
            ),
            (
                "MIR2_MCP_CONF_TEST_HOLD_BEFORE_PUTBACK_MS",
                "1200".to_string(),
            ),
            (
                "MIR2_MCP_CONF_TEST_TAKEOVER_RACE_MARKER",
                marker_arg.clone(),
            ),
        ],
    );
    proc.initialize();

    let handle = std::thread::spawn(move || {
        let response = proc.call_tool(
            "conf_set",
            json!({"section": "Setup", "key": "GenMonRate", "value": "75"}),
        );
        proc.shutdown();
        response
    });

    wait_for_marker(&marker, "hold-start", Duration::from_secs(15));
    let replacement = format!(
        "pid={}\nacq_ms={}\nnonce=replacement-live\n",
        std::process::id(),
        now_ms()
    )
    .into_bytes();
    std::fs::write(root.lock_path(), &replacement).expect("写替换活锁");

    // 接管者搬走替换锁后进入 putback 窗口；此时第三人抢占 canonical。
    wait_for_marker(&marker, "putback", Duration::from_secs(15));
    let third = format!(
        "pid={}\nacq_ms={}\nnonce=third-party-live\n",
        std::process::id(),
        now_ms()
    )
    .into_bytes();
    std::fs::write(root.lock_path(), &third).expect("写第三方活锁");
    wait_for_marker(&marker, "putback-abandoned", Duration::from_secs(15));

    let response = handle.join().expect("conf_set thread");
    assert_eq!(
        structured(&response)["code"],
        "conf_lock_timeout",
        "canonical 被占时不得覆盖，最终超时：{response}"
    );
    assert_eq!(
        std::fs::read(root.lock_path()).expect("读锁"),
        third,
        "放回绝不得覆盖 canonical 上的第三方活锁"
    );
    let text = marker_text(&marker);
    assert!(
        text.lines().any(|line| line.trim() == "putback-abandoned"),
        "必须走到「canonical 被占 ⇒ 放弃放回」分支：{text:?}"
    );
}

// ---------------------------------------------------------------------------
// A2：空锁宽限 / A5：探测缓存 / A7：拒绝路径零写 / A8：delete-pending 重试
// ---------------------------------------------------------------------------

/// A2：0 字节锁在宽限内不得被抢（可能是正在写元数据的活锁）。
#[test]
fn empty_lock_is_not_stolen_within_grace() {
    let root = TempRoot::new("empty-grace");
    let lock = root.lock_path();
    std::fs::write(&lock, b"").expect("写空锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "400".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "50".to_string()),
            ("MIR2_MCP_CONF_LOCK_EMPTY_GRACE_MS", "2000".to_string()),
        ],
    );
    proc.initialize();
    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "91"}),
    );
    assert_eq!(
        structured(&response)["code"],
        "conf_lock_timeout",
        "宽限内不得接管空锁：{response}"
    );
    assert_eq!(
        std::fs::read(&lock).expect("读锁"),
        b"",
        "宽限内空锁必须原样保留"
    );
    proc.shutdown();
}

/// A2：0 字节锁超过短宽限 ⇒ 判为崩溃残留，后续 conf_set 必须能恢复。
#[test]
fn empty_lock_is_reclaimed_after_grace() {
    let root = TempRoot::new("empty-reclaim");
    let lock = root.lock_path();
    std::fs::write(&lock, b"").expect("写空锁");
    std::thread::sleep(Duration::from_millis(400));

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "4000".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "50".to_string()),
            ("MIR2_MCP_CONF_LOCK_EMPTY_GRACE_MS", "150".to_string()),
        ],
    );
    proc.initialize();
    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "92"}),
    );
    let body = structured(&response);
    assert_eq!(body["ok"], true, "超龄空锁必须可回收：{response}");
    assert_eq!(body["wrote"], true, "{response}");
    assert_eq!(body["readback_value"], "92", "{response}");
    assert!(!lock.exists(), "回收后锁文件应被正常释放");
    proc.shutdown();
}

/// A5：等待循环里同一 pid 只探测一次（缓存），且 waited_ms 不超过 timeout + 一次探测上界。
#[test]
fn probe_results_are_cached_during_lock_wait() {
    let root = TempRoot::new("probe-cache");
    let log = root.path.join("probe.log");
    let lock = root.lock_path();
    // 活持有者 + 超龄：每轮都会进入残留判定，但探测应只发生一次。
    std::fs::write(
        &lock,
        lock_body(std::process::id(), now_ms().saturating_sub(60_000)),
    )
    .expect("写活锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "600".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "100".to_string()),
            ("MIR2_MCP_CONF_PROBE_TIMEOUT_MS", "1500".to_string()),
            ("MIR2_MCP_CONF_TEST_PROBE_LOG", log.display().to_string()),
        ],
    );
    proc.initialize();
    let started = Instant::now();
    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "93"}),
    );
    let elapsed = started.elapsed();

    assert_eq!(
        structured(&response)["code"],
        "conf_lock_timeout",
        "活锁不得被接管：{response}"
    );
    let waited = structured(&response)["waited_ms"]
        .as_u64()
        .expect("waited_ms");
    assert!(
        waited <= 600 + 1500,
        "等待必须有界（timeout + 一次探测上界）：waited_ms={waited}"
    );
    assert!(
        elapsed < Duration::from_millis(600 + 1500 + 1000),
        "整体墙钟必须有界：{elapsed:?}"
    );
    let text = marker_text(&log);
    let probes = text.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(probes, 1, "同一 pid 的探测必须缓存；日志：{text:?}");
    proc.shutdown();
}

/// A7：`key_absent` 与同值 no-op 在取锁前判定——持锁者存在时也返回原拒绝码，
/// 且不建/删/改写锁文件、不跑 cleanup。
#[test]
fn rejection_and_noop_paths_never_touch_lock() {
    let root = TempRoot::new("reject-lock");
    // 缺 MonsterPowerRate（白名单活键）用于 key_absent；GenMonRate 用于同值 no-op。
    std::fs::write(
        root.conf_path(),
        b"\xEF\xBB\xBF[Setup]\r\nGenMonRate=10\r\n",
    )
    .expect("写夹具");
    let lock = root.lock_path();
    let lock_bytes = lock_body(std::process::id(), now_ms()).into_bytes();
    std::fs::write(&lock, &lock_bytes).expect("写活锁");

    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "400".to_string()),
            ("MIR2_MCP_CONF_LOCK_STALE_MS", "30000".to_string()),
        ],
    );
    proc.initialize();

    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "MonsterPowerRate", "value": "20"}),
    );
    assert_eq!(
        structured(&response)["code"],
        "key_absent",
        "持锁者存在时 key_absent 也必须原样返回（不得 conf_lock_timeout）：{response}"
    );

    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "10"}),
    );
    let body = structured(&response);
    assert_eq!(body["ok"], true, "{response}");
    assert_eq!(body["wrote"], false, "同值必须 no-op：{response}");
    assert_eq!(body["changed"], false, "{response}");

    assert_eq!(
        std::fs::read(&lock).expect("读锁"),
        lock_bytes,
        "拒绝/只读路径不得创建、删除或改写锁"
    );
    proc.shutdown();
}

/// A8（Windows）：delete-pending 的 `ERROR_ACCESS_DENIED` 必须进入外层重试；
/// 窗口小于 timeout ⇒ 最终成功；窗口大于 timeout ⇒ `conf_lock_timeout`，绝不是 `io_error`。
#[cfg(windows)]
#[test]
fn delete_pending_access_denied_retries_until_success_or_timeout() {
    let root = TempRoot::new("delete-pending-ok");
    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "3000".to_string()),
            ("MIR2_MCP_CONF_TEST_CREATE_NEW_DENIED_MS", "300".to_string()),
        ],
    );
    proc.initialize();
    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "94"}),
    );
    let body = structured(&response);
    assert_eq!(
        body["ok"], true,
        "delete-pending 必须走外层重试最终成功（不得 io_error）：{response}"
    );
    assert_eq!(body["wrote"], true, "{response}");
    assert_eq!(body["readback_value"], "94", "{response}");
    proc.shutdown();

    let root = TempRoot::new("delete-pending-timeout");
    let mut proc = McpProc::start(
        &root.path,
        &[
            ("MIR2_MCP_CONF_LOCK_TIMEOUT_MS", "200".to_string()),
            (
                "MIR2_MCP_CONF_TEST_CREATE_NEW_DENIED_MS",
                "3000".to_string(),
            ),
        ],
    );
    proc.initialize();
    let response = proc.call_tool(
        "conf_set",
        json!({"section": "Setup", "key": "GenMonRate", "value": "95"}),
    );
    assert_eq!(
        structured(&response)["code"],
        "conf_lock_timeout",
        "delete-pending 持续超过 timeout 必须返回 conf_lock_timeout，而不是 io_error：{response}"
    );
    proc.shutdown();
}

// ---------------------------------------------------------------------------
// 辅助：拿一个「确定已死」的 pid
// ---------------------------------------------------------------------------

fn dead_pid() -> u32 {
    for _ in 0..8 {
        let mut child = {
            #[cfg(windows)]
            {
                Command::new("cmd")
                    .args(["/C", "exit", "0"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("spawn 短命子进程")
            }
            #[cfg(unix)]
            {
                Command::new("true")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("spawn true")
            }
        };
        let pid = child.id();
        let _ = child.wait();
        if !pid_is_alive(pid) {
            return pid;
        }
    }
    panic!("找不到确定的死 pid（连续 8 次短命子进程 pid 都被复用）");
}

fn pid_is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .expect("tasklist");
        let wanted = pid.to_string();
        String::from_utf8_lossy(&output.stdout).lines().any(|line| {
            line.split(',')
                .any(|field| field.trim().trim_matches('"') == wanted)
        })
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        false
    }
}
