//! res-svr 端到端测试：真起子进程 + std `TcpStream` 手写 HTTP（不引入 reqwest/axum）。
//!
//! 覆盖卡 A1–A6 的机器可判面：鉴权 403、方法闸门 405、路径穿越矩阵全 403、下载逐字节一致、
//! Range 206/416/忽略语义、清单 sha256 与缓存失效、限额 413/429（含并发闸门）、审计日志
//! 不含 token。测试 token 只出现在测试进程与被起子进程的**配置文件**里，不写任何日志断言。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// 测试专用 token（只在测试进程与子进程配置文件之间传递）。
const TOKEN: &str = "res-svr-e2e-TOKEN-0123456789abcdef";
const EXE: &str = env!("CARGO_BIN_EXE_res-svr");
/// 与服务端 `http::serve` 启动行前缀逐字一致。
const LISTENING_PREFIX: &str = "res-svr: 监听 http://";
const WAIT: Duration = Duration::from_secs(15);

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
            "res-svr-e2e-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        for dir in ["Data", "Map", "Wav"] {
            std::fs::create_dir_all(path.join(dir)).expect("建资产根");
        }
        let range: Vec<u8> = (0..1000u32).map(|i| (i % 256) as u8).collect();
        for (name, bytes) in [
            ("Data/a.wzl", b"abc".to_vec()),
            ("Data/b.wzx", b"de".to_vec()),
            ("Data/range.wzl", range),
            ("Data/big.wzl", vec![7u8; 4096]),
            ("Data/blocked.exe", b"nope".to_vec()),
            ("Map/m.map", b"MAP!".to_vec()),
            ("Wav/note.wav", b"RIFF".to_vec()),
        ] {
            std::fs::write(path.join(name), bytes).expect("写夹具文件");
        }
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn data(&self) -> PathBuf {
        self.path.join("Data")
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

struct Server {
    child: Child,
    addr: SocketAddr,
    lines: Arc<Mutex<Vec<String>>>,
    root: TempRoot,
    _conf_path: PathBuf,
}

impl Server {
    /// 起一个服务实例；`extra` 追加到 `[res-svr]` 段（限额覆盖）。
    fn start(tag: &str, extra: &str) -> Self {
        let root = TempRoot::new(tag);
        let conf_path = root.path().join("res-svr.conf");
        let conf = format!(
            "[res-svr]\r\nbind=127.0.0.1:0\r\ntoken={TOKEN}\r\n{extra}\r\n\
             [map]\r\nData={}\r\nMap={}\r\nWav={}\r\n\
             [ext]\r\nwhitelist=.wzl,.wzx,.wav,.mp3,.map,.mex,.wil,.wick\r\n",
            root.data().display(),
            root.path().join("Map").display(),
            root.path().join("Wav").display(),
        );
        std::fs::write(&conf_path, conf).expect("写配置");
        let mut child = Command::new(EXE)
            .arg("--conf")
            .arg(&conf_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("启动 res-svr");
        let stderr = child.stderr.take().expect("stderr 管道");
        let lines = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel::<()>();
        {
            let lines = Arc::clone(&lines);
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    lines.lock().expect("日志锁").push(line);
                    let _ = tx.send(());
                }
            });
        }
        let deadline = Instant::now() + WAIT;
        let addr = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "等待监听行超时：{lines:?}");
            rx.recv_timeout(remaining).expect("等待监听行超时");
            let found = {
                let lines = lines.lock().expect("日志锁");
                lines.iter().find_map(|line| parse_listening(line))
            };
            if let Some(addr) = found {
                break addr;
            }
        };
        Self {
            child,
            addr,
            lines,
            root,
            _conf_path: conf_path,
        }
    }

    fn bearer(&self) -> String {
        format!("Bearer {TOKEN}")
    }

    /// 有界等待日志行满足条件（返回当前全部行）。
    fn wait_for_log(&self, predicate: impl Fn(&str) -> bool, within: Duration) -> Vec<String> {
        let deadline = Instant::now() + within;
        loop {
            let lines = {
                let lines = self.lines.lock().expect("日志锁");
                lines.clone()
            };
            if lines.iter().any(|line| predicate(line)) {
                return lines;
            }
            assert!(
                Instant::now() < deadline,
                "等待日志行超时；现有日志：{lines:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn parse_listening(line: &str) -> Option<SocketAddr> {
    let rest = line.strip_prefix(LISTENING_PREFIX)?;
    let authority = rest.split('/').next()?;
    authority.parse().ok()
}

// ---------------------------------------------------------------------------
// 手写 HTTP 客户端
// ---------------------------------------------------------------------------

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    }

    fn content_length(&self) -> usize {
        self.header("content-length")
            .and_then(|value| value.parse::<usize>().ok())
            .expect("响应必须带合法 Content-Length")
    }
}

/// `auth_value` 是完整的 `Authorization` 头值（如 `Bearer <token>`）或 `None`。
fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    auth_value: Option<&str>,
    extra_headers: &[(&str, &str)],
) -> HttpResponse {
    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("连接 res-svr");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("设置读超时");
    let mut raw_request =
        format!("{method} {path} HTTP/1.1\r\nHost: res-svr-test\r\nConnection: close\r\n");
    if let Some(value) = auth_value {
        raw_request.push_str(&format!("Authorization: {value}\r\n"));
    }
    for (name, value) in extra_headers {
        raw_request.push_str(&format!("{name}: {value}\r\n"));
    }
    raw_request.push_str("\r\n");
    stream
        .write_all(raw_request.as_bytes())
        .expect("写请求失败");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("读响应失败");
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> HttpResponse {
    let Some(header_end) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
        panic!(
            "响应缺少头结束标记；原始字节：{:?}",
            String::from_utf8_lossy(raw)
        );
    };
    let header_end = header_end + 4;
    let head = String::from_utf8_lossy(&raw[..header_end]);
    let mut lines = head.split("\r\n");
    let status_line = lines.next().expect("状态行");
    let status: u16 = status_line
        .split(' ')
        .nth(1)
        .expect("状态码")
        .parse()
        .expect("状态码是数字");
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    HttpResponse {
        status,
        headers,
        body: raw[header_end..].to_vec(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// 取清单里指定条目的 `sha256`（顺带断言清单本身可解析且条目存在）。
fn index_sha(server: &Server, path: &str) -> String {
    let response = request(
        server.addr,
        "GET",
        "/res/index.json",
        Some(&server.bearer()),
        &[],
    );
    assert_eq!(response.status, 200, "清单请求必须 200");
    let parsed: serde_json::Value =
        serde_json::from_slice(&response.body).expect("清单是合法 JSON");
    parsed["files"]
        .as_array()
        .expect("files 是数组")
        .iter()
        .find(|file| file["path"] == path)
        .unwrap_or_else(|| panic!("清单里必须有 {path}"))["sha256"]
        .as_str()
        .expect("sha256 是字符串")
        .to_string()
}

// ---------------------------------------------------------------------------
// A2：鉴权
// ---------------------------------------------------------------------------

#[test]
fn no_or_wrong_token_is_403() {
    let server = Server::start("auth", "");
    for path in ["/res/index.json", "/res/file/Data/a.wzl"] {
        let response = request(server.addr, "GET", path, None, &[]);
        assert_eq!(response.status, 403, "缺失 token 必须 403：{path}");
    }

    let bearer = server.bearer();
    let response = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    assert_eq!(response.status, 200, "正确 token 必须放行");

    // 等长但逐字节不同（常量时间比较的行为用例）。
    let mut wrong = TOKEN.as_bytes().to_vec();
    let last = wrong.len() - 1;
    wrong[last] = if wrong[last] == b'x' { b'y' } else { b'x' };
    let wrong = String::from_utf8(wrong).expect("仍是 ASCII");
    assert_eq!(wrong.len(), TOKEN.len());
    let wrong_bearer = format!("Bearer {wrong}");
    assert_eq!(
        request(
            server.addr,
            "GET",
            "/res/index.json",
            Some(&wrong_bearer),
            &[]
        )
        .status,
        403
    );

    // 长度不同。
    let longer_bearer = format!("Bearer {TOKEN}0");
    assert_eq!(
        request(
            server.addr,
            "GET",
            "/res/index.json",
            Some(&longer_bearer),
            &[]
        )
        .status,
        403
    );

    // 非 Bearer scheme。
    let basic = format!("Basic {TOKEN}");
    assert_eq!(
        request(server.addr, "GET", "/res/index.json", Some(&basic), &[]).status,
        403
    );

    // HEAD 无 token 同样 403（鉴权先于方法/路由）。
    assert_eq!(
        request(server.addr, "HEAD", "/res/index.json", None, &[]).status,
        403
    );

    // 重复 Authorization 头：**首值优先**（hyper `HeaderMap::get` 取第一个值）。
    // 这条用例钉住「取最后一个值」的变异（reviewer-26 M5）。
    let correct_first = request(
        server.addr,
        "GET",
        "/res/index.json",
        Some(&bearer),
        &[("Authorization", wrong_bearer.as_str())],
    );
    assert_eq!(correct_first.status, 200, "正确头在前 ⇒ 200");
    let wrong_first = request(
        server.addr,
        "GET",
        "/res/index.json",
        Some(&wrong_bearer),
        &[("Authorization", bearer.as_str())],
    );
    assert_eq!(wrong_first.status, 403, "错误头在前 ⇒ 403（首值优先）");
}

// ---------------------------------------------------------------------------
// A1：方法白名单
// ---------------------------------------------------------------------------

#[test]
fn non_get_head_methods_are_405() {
    let server = Server::start("methods", "");
    let bearer = server.bearer();
    for method in ["POST", "PUT", "DELETE", "PATCH"] {
        let response = request(server.addr, method, "/res/index.json", Some(&bearer), &[]);
        assert_eq!(response.status, 405, "{method} 必须 405");
        assert_eq!(response.header("allow"), Some("GET, HEAD"));
    }
    // 鉴权先于方法闸门：无 token 的 POST ⇒ 403 而不是 405。
    assert_eq!(
        request(server.addr, "POST", "/res/index.json", None, &[]).status,
        403
    );
}

// ---------------------------------------------------------------------------
// A4：路径穿越矩阵
// ---------------------------------------------------------------------------

#[test]
fn traversal_and_absolute_paths_are_403() {
    let server = Server::start("traversal", "");
    let bearer = server.bearer();
    let hostile = [
        // 逃出映射根
        "/res/file/Data/../../GameSrv.exe",
        "/res/file/Data/%2e%2e/%2e%2e/GameSrv.exe",
        "/res/file/Data%5c..%5c..%5cGameSrv.exe",
        // 逃出又回来：必须仍 403（M2 变异的靶子：去掉 `..` 检查后这里会 200）
        "/res/file/Data/../Data/a.wzl",
        "/res/file/Data/%2e%2e/Data/a.wzl",
        // 绝对路径 / UNC / 盘符
        "/res/file//etc/passwd",
        "/res/file/%2fetc/passwd",
        "/res/file/C:/Windows/win.ini",
        "/res/file//host/share",
        // 尾点 / 尾空格
        "/res/file/Data/a.wzl.",
        "/res/file/Data/a.wzl%20",
        // 中间组件尾点 / 尾空格：逐段检查，不是只查末段（reviewer-26 M4）
        "/res/file/Data/sub%20/b.wzx",
        "/res/file/Data/sub./b.wzx",
        // NUL / 控制字符
        "/res/file/Data/a.wzl%00.txt",
        "/res/file/Data/a.wzl%0a.txt",
        // Windows 保留设备名
        "/res/file/NUL",
        "/res/file/Data/NUL.wzl",
        // ADS
        "/res/file/Data/a.wzl:ads",
        // 扩展白名单拒绝（文件真实存在 ⇒ 403 而非 404）
        "/res/file/Data/blocked.exe",
        // 点组件
        "/res/file/Data/./a.wzl",
    ];
    for path in hostile {
        let response = request(server.addr, "GET", path, Some(&bearer), &[]);
        assert_eq!(
            response.status,
            403,
            "恶意路径必须 403：{path}；实际响应：{:?}",
            String::from_utf8_lossy(&response.body)
        );
    }
}

#[test]
fn not_found_is_404() {
    let server = Server::start("notfound", "");
    let bearer = server.bearer();
    for path in [
        "/res/file/Data/missing.wzl",
        "/res/file/Other/a.wzl",
        "/res/nope",
        "/res/index.json/",
    ] {
        let response = request(server.addr, "GET", path, Some(&bearer), &[]);
        assert_eq!(response.status, 404, "{path} 应 404");
    }
    // 无扩展名的路径由白名单策略拒绝（不是 404）。
    assert_eq!(
        request(server.addr, "GET", "/res/file/Data/sub", Some(&bearer), &[]).status,
        403
    );
}

// ---------------------------------------------------------------------------
// A4：下载与 HEAD
// ---------------------------------------------------------------------------

#[test]
fn download_is_byte_exact_with_content_length() {
    let server = Server::start("download", "");
    let bearer = server.bearer();
    let response = request(
        server.addr,
        "GET",
        "/res/file/Data/a.wzl",
        Some(&bearer),
        &[],
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.content_length(), 3);
    assert_eq!(response.body, b"abc");
    assert_eq!(response.header("accept-ranges"), Some("bytes"));
    assert_eq!(
        response.header("content-type"),
        Some("application/octet-stream")
    );

    // 子目录 + 大小写不敏感前缀。
    let response = request(
        server.addr,
        "GET",
        "/res/file/data/A.WZL",
        Some(&bearer),
        &[],
    );
    assert_eq!(response.status, 200, "前缀/文件名大小写不敏感");
    assert_eq!(response.body, b"abc");

    // HEAD：头与 GET 一致，体为空。
    let response = request(
        server.addr,
        "HEAD",
        "/res/file/Data/a.wzl",
        Some(&bearer),
        &[],
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.content_length(), 3);
    assert!(response.body.is_empty(), "HEAD 响应不得有体");
}

// ---------------------------------------------------------------------------
// A4：Range
// ---------------------------------------------------------------------------

#[test]
fn range_semantics() {
    let server = Server::start("range", "");
    let bearer = server.bearer();
    let source: Vec<u8> = (0..1000u32).map(|i| (i % 256) as u8).collect();

    let response = request(
        server.addr,
        "GET",
        "/res/file/Data/range.wzl",
        Some(&bearer),
        &[("Range", "bytes=0-99")],
    );
    assert_eq!(response.status, 206);
    assert_eq!(response.content_length(), 100);
    assert_eq!(response.header("content-range"), Some("bytes 0-99/1000"));
    assert_eq!(response.body, source[0..=99]);

    // 终点越界 ⇒ 饱和到 size-1。
    let response = request(
        server.addr,
        "GET",
        "/res/file/Data/range.wzl",
        Some(&bearer),
        &[("Range", "bytes=990-2000")],
    );
    assert_eq!(response.status, 206);
    assert_eq!(response.content_length(), 10);
    assert_eq!(response.header("content-range"), Some("bytes 990-999/1000"));
    assert_eq!(response.body, source[990..=999]);

    // 开放段。
    let response = request(
        server.addr,
        "GET",
        "/res/file/Data/range.wzl",
        Some(&bearer),
        &[("Range", "bytes=5-")],
    );
    assert_eq!(response.status, 206);
    assert_eq!(response.content_length(), 995);
    assert_eq!(response.body, source[5..]);

    // 起点越界 ⇒ 416 + `bytes */size`。
    let response = request(
        server.addr,
        "GET",
        "/res/file/Data/range.wzl",
        Some(&bearer),
        &[("Range", "bytes=1000-")],
    );
    assert_eq!(response.status, 416);
    assert_eq!(response.header("content-range"), Some("bytes */1000"));

    // 多段 / 后缀 / 语法非法 ⇒ 忽略，回 200 全量。
    for ignored in ["bytes=0-1,5-6", "bytes=-10", "bytes=abc"] {
        let response = request(
            server.addr,
            "GET",
            "/res/file/Data/range.wzl",
            Some(&bearer),
            &[("Range", ignored)],
        );
        assert_eq!(response.status, 200, "{ignored} 应被忽略");
        assert_eq!(response.content_length(), 1000);
        assert_eq!(response.body, source);
    }

    // HEAD + Range：头正确、体为空。
    let response = request(
        server.addr,
        "HEAD",
        "/res/file/Data/range.wzl",
        Some(&bearer),
        &[("Range", "bytes=0-99")],
    );
    assert_eq!(response.status, 206);
    assert_eq!(response.content_length(), 100);
    assert!(response.body.is_empty());
}

// ---------------------------------------------------------------------------
// A3：清单与哈希缓存
// ---------------------------------------------------------------------------

#[test]
fn index_lists_files_and_cache_invalidates_on_change() {
    let server = Server::start("index", "");
    let bearer = server.bearer();

    let response = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));
    let parsed: serde_json::Value =
        serde_json::from_slice(&response.body).expect("清单是合法 JSON");
    let files = parsed["files"].as_array().expect("files 是数组").clone();
    let paths: Vec<&str> = files
        .iter()
        .map(|file| file["path"].as_str().expect("path 是字符串"))
        .collect();
    assert_eq!(
        paths,
        vec![
            "Data/a.wzl",
            "Data/b.wzx",
            "Data/big.wzl",
            "Data/range.wzl",
            "Map/m.map",
            "Wav/note.wav",
        ],
        "清单按路径排序，且不含非白名单扩展"
    );
    let a = files
        .iter()
        .find(|file| file["path"] == "Data/a.wzl")
        .expect("a.wzl 条目");
    assert_eq!(a["size"].as_u64(), Some(3));
    assert_eq!(
        a["sha256"].as_str(),
        Some(sha256_hex(b"abc").as_str()),
        "sha256 必须与被哈希内容一致"
    );
    let big = files
        .iter()
        .find(|file| file["path"] == "Data/big.wzl")
        .expect("big.wzl 条目");
    assert_eq!(big["size"].as_u64(), Some(4096));

    // 改内容（size 变）⇒ (size, mtime) 缓存失效、重算。
    std::fs::write(server.root.data().join("a.wzl"), b"abcXY").expect("改夹具");
    let response = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    assert_eq!(response.status, 200);
    let parsed: serde_json::Value =
        serde_json::from_slice(&response.body).expect("清单是合法 JSON");
    let a = parsed["files"]
        .as_array()
        .expect("files 是数组")
        .iter()
        .find(|file| file["path"] == "Data/a.wzl")
        .expect("a.wzl 条目")
        .clone();
    assert_eq!(a["size"].as_u64(), Some(5), "改内容后 size 必须更新");
    assert_eq!(
        a["sha256"].as_str(),
        Some(sha256_hex(b"abcXY").as_str()),
        "缓存必须失效重算"
    );
}

// ---------------------------------------------------------------------------
// 加固 F1：清单缓存 TTL（同尺寸同 mtime 重写）
// ---------------------------------------------------------------------------

#[test]
fn index_cache_ttl_refreshes_same_size_same_mtime_rewrite() {
    // TTL 取 3s：首次请求到第二次请求之间的耗时远小于它，「TTL 内给旧值」是确定性的；
    // 睡 3.3s 后必超 TTL，必须重算出新 sha。
    let server = Server::start(
        "cachettl",
        "index_cache_ttl_ms=3000\r\nrate_limit_rps=1000\r\nrate_limit_burst=1000\r\n",
    );
    let path = server.root.data().join("a.wzl");
    let original_mtime = std::fs::metadata(&path)
        .expect("取 a.wzl 元数据")
        .modified()
        .expect("本平台必须给出 mtime");
    assert_eq!(index_sha(&server, "Data/a.wzl"), sha256_hex(b"abc"));

    // 同尺寸原地重写并把 mtime 还原成旧值 ⇒ (size, mtime) 指纹完全不变。
    std::fs::write(&path, b"xyz").expect("同尺寸改写");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("打开待改 mtime 的文件")
        .set_modified(original_mtime)
        .expect("还原 mtime");
    let after = std::fs::metadata(&path).expect("取改写后元数据");
    assert_eq!(after.len(), 3, "夹具必须同尺寸");
    assert_eq!(
        after.modified().ok(),
        Some(original_mtime),
        "夹具必须同 mtime"
    );

    // TTL 内：允许（这里断言必然）返回旧 sha —— 正是 F1 的现象，TTL 给它上界。
    assert_eq!(
        index_sha(&server, "Data/a.wzl"),
        sha256_hex(b"abc"),
        "TTL 内允许旧值（缓存命中）"
    );

    // 超过 TTL：必须重算，返回新 sha。
    thread::sleep(Duration::from_millis(3_300));
    assert_eq!(
        index_sha(&server, "Data/a.wzl"),
        sha256_hex(b"xyz"),
        "超过 TTL 必须重算"
    );
}

// ---------------------------------------------------------------------------
// A5：限额
// ---------------------------------------------------------------------------

#[test]
fn oversized_file_is_413() {
    let server = Server::start("maxsize", "max_file_size=1024\r\n");
    let bearer = server.bearer();
    let response = request(
        server.addr,
        "GET",
        "/res/file/Data/big.wzl",
        Some(&bearer),
        &[],
    );
    assert_eq!(response.status, 413);
    // 限额内的文件仍可下载。
    let response = request(
        server.addr,
        "GET",
        "/res/file/Data/a.wzl",
        Some(&bearer),
        &[],
    );
    assert_eq!(response.status, 200);
    // 清单仍列出大文件（是否超限是下载面的判断）。
    let response = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    let parsed: serde_json::Value = serde_json::from_slice(&response.body).expect("JSON");
    assert!(parsed["files"]
        .as_array()
        .expect("数组")
        .iter()
        .any(|file| file["path"] == "Data/big.wzl"));
}

#[test]
fn rate_limit_returns_429() {
    let server = Server::start("ratelimit", "rate_limit_rps=1\r\nrate_limit_burst=2\r\n");
    let bearer = server.bearer();
    let first = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    let second = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    assert_eq!(first.status, 200);
    assert_eq!(second.status, 200);
    let third = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    assert_eq!(third.status, 429, "突发额度 2 用尽后必须 429");
    assert_eq!(third.header("retry-after"), Some("1"));
}

#[test]
fn connection_limit_returns_429_and_recovers() {
    let server = Server::start(
        "concurrency",
        "max_connections=1\r\nrate_limit_rps=1000\r\nrate_limit_burst=1000\r\n",
    );
    let bearer = server.bearer();
    // 空闲连接占住唯一名额（连接级许可在连接关闭前一直持有）。
    let idle =
        TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).expect("连接空闲连接");
    thread::sleep(Duration::from_millis(150));
    let rejected = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    assert_eq!(rejected.status, 429, "并发上限 1 时第二条连接必须 429");
    assert_eq!(
        String::from_utf8_lossy(&rejected.body).trim(),
        "too many connections"
    );

    // 有界等待：关闭空闲连接后名额释放，随后请求恢复 200。
    drop(idle);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut status = 0;
    while Instant::now() < deadline {
        status = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]).status;
        if status == 200 {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(status, 200, "空闲连接关闭后并发名额必须释放");
}

// ---------------------------------------------------------------------------
// 加固 F5：已建连接的空闲超时（慢读/不读响应不得永久占坑）
// ---------------------------------------------------------------------------

#[test]
fn idle_connection_times_out_and_releases_slot() {
    let server = Server::start(
        "idletimeout",
        "max_connections=1\r\nidle_timeout_ms=2500\r\nrate_limit_rps=1000\r\nrate_limit_burst=1000\r\n",
    );
    let bearer = server.bearer();

    // 第一条连接：发出完整请求但**不读响应**（keep-alive，不写 Connection: close）。
    let idle =
        TcpStream::connect_timeout(&server.addr, Duration::from_secs(5)).expect("连接空闲连接");
    idle.set_read_timeout(Some(Duration::from_secs(10)))
        .expect("设置读超时");
    let raw = format!(
        "GET /res/index.json HTTP/1.1\r\nHost: res-svr-test\r\nAuthorization: {bearer}\r\n\r\n"
    );
    (&idle).write_all(raw.as_bytes()).expect("写请求");

    // 名额被占期间：第二条连接必须 429（探针与超时都还没到）。
    thread::sleep(Duration::from_millis(150));
    let rejected = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    assert_eq!(rejected.status, 429, "空闲连接仍占名额时必须 429");

    // 有界等待：空闲超时（2.5s）后旧连接被关闭、名额释放，随后请求恢复 200。
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut status = 0;
    while Instant::now() < deadline {
        status = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]).status;
        if status == 200 {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(status, 200, "空闲超时必须关闭旧连接并释放并发名额");

    // 同一实例上正常的 GET/Range 不受空闲超时配置影响。
    let ranged = request(
        server.addr,
        "GET",
        "/res/file/Data/range.wzl",
        Some(&bearer),
        &[("Range", "bytes=0-9")],
    );
    assert_eq!(ranged.status, 206, "正常 Range 请求不受空闲超时影响");
    assert_eq!(ranged.content_length(), 10);
    drop(idle);
}

// ---------------------------------------------------------------------------
// A2：审计日志红线
// ---------------------------------------------------------------------------

#[test]
fn audit_log_never_contains_token() {
    let server = Server::start("audit", "");
    let bearer = server.bearer();
    let _ = request(server.addr, "GET", "/res/index.json", Some(&bearer), &[]);
    let _ = request(server.addr, "GET", "/res/file/Data/a.wzl", None, &[]);
    let _ = request(
        server.addr,
        "GET",
        "/res/file/Data/../../x.wzl",
        Some(&bearer),
        &[],
    );

    let lines = server.wait_for_log(
        |line| line.contains("res-svr audit"),
        Duration::from_secs(5),
    );
    let audit_lines: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("res-svr audit"))
        .collect();
    assert!(
        audit_lines.len() >= 3,
        "审计行必须真的被写出（不是静默关闭）：{lines:?}"
    );
    for line in &lines {
        assert!(
            !line.contains(TOKEN),
            "审计/服务日志不得出现 token 本体：{line}"
        );
        assert!(
            !line.to_ascii_lowercase().contains("authorization"),
            "审计/服务日志不得回显 Authorization 头：{line}"
        );
    }
}
