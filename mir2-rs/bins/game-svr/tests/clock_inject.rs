//! `MIR2_CLOCK_MS` 可注入时钟的**进程级**用例（M3 横切前置 `openmir2-m3-clock-injection`，Rust 半）。
//!
//! 判据（口径 = `src/clock.rs` 模块头）：
//! 1. 设 `MIR2_CLOCK_MS=<n>` ⇒ 进程日志出现 `CLOCK_PROBE origin=<n>`（启动处一条 + 世界建好后
//!    `world_clock_ms=<n>` 一条 ⇒ 证明 `now_ms()` 与 `world.clock_ms` 接的是**同一**注入）；
//! 2. **不设** ⇒ 日志**没有** `CLOCK_PROBE`（生产路径一字不改，`world.clock_ms` 仍从 0 起）；
//! 3. 设非法值 ⇒ 进程**≤N 秒内非零退出**且错误信息点名 `MIR2_CLOCK_MS`（fail-closed，不许静默降级）。
//!
//! 抗 flaky / 防挂死（照 `tests/seed_inject.rs` 的 T-30 口径）：
//! - 临时目录 `pid + 原子计数` 用例级唯一；子进程 stdout 落**文件**（不用管道）⇒ 不存在等不到 EOF；
//! - 等 probe / 等退出一律**有界轮询**；到界/失败一律**因子进程树 kill**（Windows `taskkill /T /F /PID`）；
//! - 每个用例结束断言**不留本用例的 `game-svr`**（按本用例唯一临时目录名过滤）。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

#[allow(dead_code)] // 共用模块：另一个 target 的条目在本 target 未用
mod support;

/// 注入起点（毫秒；约 2023-11-14，避开 `SystemTime` 的 0 值便于区分「注入 vs 未注入」）。
const CLOCK_ORIGIN: i64 = 1_700_000_000_000;

/// 非法值的**有界退出上限 N**：`main` 在起监听之前 `?` 传播 ⇒ 立即非零退出；取 10s = 实测 ~10× 余量。
const INVALID_CLOCK_EXIT_BOUND: Duration = Duration::from_secs(10);

/// 就绪/探针等待上限（正常 <1s 出现）。
const READY_BOUND: Duration = Duration::from_secs(20);

static TEMP_DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique_temp_dir(tag: &str) -> PathBuf {
    let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("gs-clock-{tag}-{}-{seq}", std::process::id()))
}

/// 与 `tests/relay.rs` / `tests/seed_inject.rs` 同形的最小合法 `Server.conf`。
fn write_conf(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let conf = dir.join("Server.conf");
    std::fs::write(
        &conf,
        format!(
            "[Server]\nGateAddr=127.0.0.1\nGatePort={}\nCheckBlock=12288\n\
             IDSAddr=127.0.0.1\nIDSPort={}\nDBAddr=127.0.0.1\nDBPort={}\nWorldMaps=64x64\n",
            support::next_port(),
            support::next_port(),
            support::next_port()
        ),
    )
    .unwrap();
    conf
}

/// **因子进程树 kill**：Windows 用 `taskkill /T /F /PID`，非 Windows 退化到 `kill`。
fn kill_process_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// 用例级兜底：**断言 panic（展开）路径**也要因子进程树 kill + 清临时目录。
#[must_use]
struct ProcGuard {
    pid: u32,
    dir: PathBuf,
}

impl Drop for ProcGuard {
    fn drop(&mut self) {
        kill_process_tree(self.pid);
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// 是否还有带本用例 `tag` 的 `game-svr` 进程（Windows 用 `wmic` 读命令行；无 `wmic` 时保守返回
/// `false`，不制造假红）。
#[cfg(windows)]
fn game_svr_leftover(tag: &str) -> bool {
    let Ok(out) = std::process::Command::new("wmic")
        .args([
            "process",
            "where",
            "name='game-svr.exe'",
            "get",
            "CommandLine",
            "/format:list",
        ])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout).contains(tag)
}

#[cfg(not(windows))]
fn game_svr_leftover(_tag: &str) -> bool {
    false
}

/// **「用例结束不留本用例的 `game-svr`」断言**（给 tree-kill 一点收敛时间，最多 ~5s）。
async fn assert_no_leftover_game_svr(dir: &Path) {
    let tag = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if tag.is_empty() {
        return;
    }
    for _ in 0..25 {
        if !game_svr_leftover(&tag) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("用例结束仍残留本用例的 game-svr（tag={tag}）——tree-kill 未收干净");
}

/// 有超时地轮询日志直到出现 `needle`，返回**最后一次**读到的全文（可见性延迟 ⇒ 不做一次性读取）。
async fn wait_log_contains(log: &Path, needle: &str, timeout: Duration) -> String {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        if text.contains(needle) || tokio::time::Instant::now() >= deadline {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// **有界收尸**：因子进程树 kill 后再 `wait()` 收直接子进程（best-effort，最多 `reap` 时间）。
async fn kill_tree_and_reap(child: &mut tokio::process::Child, pid: u32, reap: Duration) {
    kill_process_tree(pid);
    let _ = tokio::time::timeout(reap, child.wait()).await;
    child.kill().await.ok();
}

/// 判据 1：设起点 ⇒ 启动处与 `sim_loop` 各打一条 `CLOCK_PROBE origin=<n>`（`world_clock_ms` 见第 2 条）。
#[tokio::test]
async fn clock_env_injection_logs_probe() {
    let dir = unique_temp_dir("set");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env("MIR2_CLOCK_MS", CLOCK_ORIGIN.to_string())
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap_or(0);
    let _guard = ProcGuard {
        pid,
        dir: dir.clone(),
    };
    let text = wait_log_contains(&log, "world_clock_ms=", READY_BOUND).await;
    kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
    assert_no_leftover_game_svr(&dir).await;
    assert!(
        text.contains(&format!("CLOCK_PROBE origin={CLOCK_ORIGIN}")),
        "应打出 CLOCK_PROBE origin={CLOCK_ORIGIN}；实得日志：\n{text}"
    );
    assert!(
        text.contains(&format!("world_clock_ms={CLOCK_ORIGIN}")),
        "world.clock_ms 必须以同一 origin 起（未接注入 ⇒ 本断言必红）；实得日志：\n{text}"
    );
}

/// 判据 2：不设 ⇒ 生产路径一字不改（无 `CLOCK_PROBE`；`world.clock_ms` 不从注入起点起）。
#[tokio::test]
async fn unset_clock_env_is_inert() {
    let dir = unique_temp_dir("unset");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env_remove("MIR2_CLOCK_MS")
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap_or(0);
    let _guard = ProcGuard {
        pid,
        dir: dir.clone(),
    };
    // 先等到监听就绪行（sim 线程在此之前已起），再留一个短 settle 读全文。
    let ready = wait_log_contains(&log, "已启动", READY_BOUND).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
    assert_no_leftover_game_svr(&dir).await;
    assert!(
        ready.contains("已启动"),
        "进程应正常起来；实得日志：\n{ready}"
    );
    assert!(
        !text.contains("CLOCK_PROBE") && !text.contains("时钟注入"),
        "未设变数时不得出现注入行（生产路径一字不改）；实得日志：\n{text}"
    );
}

/// 判据 3：非法值 ⇒ **≤N 秒内**非零退出 + 点名变数（fail-closed），且**不留 `game-svr`**。
#[tokio::test]
async fn invalid_clock_env_exits_nonzero() {
    let dir = unique_temp_dir("bad");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env("MIR2_CLOCK_MS", "not-an-int")
        .stdout(out)
        .stderr(err)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap_or(0);
    let _guard = ProcGuard {
        pid,
        dir: dir.clone(),
    };
    let started = tokio::time::Instant::now();
    let status = match tokio::time::timeout(INVALID_CLOCK_EXIT_BOUND, child.wait()).await {
        Ok(Ok(status)) => Some(status),
        Ok(Err(e)) => panic!("等待 game-svr 退出失败：{e}"),
        Err(_) => None,
    };
    let elapsed = started.elapsed();
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    if status.is_none() {
        kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
        assert_no_leftover_game_svr(&dir).await;
        panic!(
            "非法 MIR2_CLOCK_MS 必须 ≤{INVALID_CLOCK_EXIT_BOUND:?} 退出；到界 {elapsed:?} 仍未退出 \
             ⇒ 已因子进程树 kill（防挂死/防泄漏）。子进程原始输出：\n{text}"
        );
    }
    let code = status.unwrap().code();
    assert_no_leftover_game_svr(&dir).await;
    println!(
        "[clock-injection 诊断] invalid MIR2_CLOCK_MS ⇒ exit={code:?} elapsed={elapsed:?}\n{text}"
    );
    assert!(
        code.is_some_and(|c| c != 0),
        "非法 MIR2_CLOCK_MS 必须非零退出；实得 exit={code:?}，耗时 {elapsed:?}；子进程原始输出：\n{text}"
    );
    assert!(
        text.contains("MIR2_CLOCK_MS"),
        "错误信息应点名 MIR2_CLOCK_MS；实得：\n{text}"
    );
}
