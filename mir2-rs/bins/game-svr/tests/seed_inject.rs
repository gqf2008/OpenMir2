//! `MIR2_RNG_SEED` 同种子注入的**进程级**用例（M3 **T-30**）。
//!
//! 判据（口径 = C# 侧 `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`）：
//! 1. 设 `MIR2_RNG_SEED=42` ⇒ 进程日志出现与 C# 钩子同形的 `SEED_PROBE` 行，8 项与 C# 实测逐项相同；
//! 2. **不设** ⇒ 日志**没有** `SEED_PROBE` 行（生产路径一字不改）；
//! 3. 设非法值 ⇒ 进程**非零退出**且错误信息点名 `MIR2_RNG_SEED`（fail-closed，不许静默降级）。
//!
//! 抗 flaky / **防挂死**（owner finding `owner-t30-hang-evidence-20261012.md`：非法种子用例曾把
//! 整条工具链挂死 9 分钟并泄漏 `game-svr`）：
//! - 临时目录 `pid + 原子计数` 用例级唯一；子进程 stdout 落盘有可见性延迟 ⇒ 一律**有超时地轮询**；
//! - **每个进程级用例都带界**：等退出、读 probe 都是 `tokio::time::timeout`，**没有无界的
//!   `Command::output()`**；到界/失败一律**因子进程树 kill**（Windows `taskkill /T /F /PID`）——
//!   `kill_on_drop` 只覆盖本进程 spawn 的**直接子进程**，被间接拉起的服务进程要单独收；
//! - 每个用例结束断言**不留本用例的 `game-svr`**（按本用例唯一临时目录名过滤，避免误伤并行用例 /
//!   其它车道的进程）。
//!
//! 根因（诊断结论，原始件 `rust-seed-injection-20261012/owner-t30-hang-diagnosis.txt`）：
//! 生产路径的 fail-closed 是**对的**（环境带 `MIR2_RNG_SEED=not-an-int` 时实测 rc=1、~1s）；挂死
//! 发生在**子进程忽略 / 未应用非法种子**（例如审计时把 `lib.rs` 的 `seed::env_seed()` 变异成 `None` 的「忽略环境变数」
//! 红检仍挂着、或 env 未送达）时——`invalid_seed_env_exits_nonzero` 用了**无界的 `.output()`**，
//! 子进程一进 DB 重连循环就永远不返回。修的是**用例的健壮性**，不是生产路径。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

// 共用「无竞态端口」工具（卡 `openmir2-m2-flaky-relay-testports`）：与 `tests/relay.rs` 同一份
// 实现，避免每个 target 各写一份会漂移的探测即释放端口分配。
#[allow(dead_code)] // 共用模块：另一个 target 的条目在本 target 未用
mod support;

/// C# 钩子实测的 `SEED_PROBE`（`MIR2_RNG_SEED=42`）——与
/// `mir2-rs/tests/parity/m3/RNG同种子与记录回放方案.md` §二「实测证据」一致。
const SEED42_PROBE: &str =
    "SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307";

/// 非法种子的**有界退出上限 N**：oracle 侧 `int.TryParse` 失败 ⇒ `Fail(...) ⇒ Environment.Exit(97)`
/// （`StartupHook.cs:46/48`）是**立即**的；Rust 侧 `main()` 在 `config::load` 之后、任何监听/线程之前
/// `seed::env_seed()?` 失败 ⇒ 实测 **~1s**（rc=1）。取 **10s** = 实测的 ~10× 余量（兼顾慢机/CI），
/// 到界即判红 —— 与 `tests/relay.rs::missing_worldmaps_and_envir_exits_nonzero` 同款上限。
const INVALID_SEED_EXIT_BOUND: Duration = Duration::from_secs(10);

/// 就绪/探针等待上限（正常 <1s 出现；见 `relay.rs` 同族经验）。
const READY_BOUND: Duration = Duration::from_secs(20);

static TEMP_DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique_temp_dir(tag: &str) -> PathBuf {
    let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("gs-seed-{tag}-{}-{seq}", std::process::id()))
}

/// 与 `tests/relay.rs` 同形的最小合法 `Server.conf`（`[Server]` 段必需键齐；`WorldMaps` 用占位避免
/// 依赖真 Envir）。
fn write_conf(dir: &std::path::Path) -> std::path::PathBuf {
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

/// **因子进程树 kill**（T-30 finding ②）：Windows 用 `taskkill /T /F /PID`，非 Windows 退化到 `kill`。
///
/// `kill_on_drop` 只保证**直接子进程**被收；进程树里的间接服务进程必须显式收，否则会泄漏
/// （finding 里正是 `game-svr.exe` 存活 9 分钟没人收）。kill 失败（进程已退出/PID 不存在）无害。
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

/// **「用例结束不留本用例的 `game-svr`」断言**（T-30 finding ②）：按本用例唯一临时目录名过滤，
/// 给 tree-kill 一点收敛时间（最多 ~5s）。
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
async fn wait_log_contains(log: &std::path::Path, needle: &str, timeout: Duration) -> String {
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

/// 判据 1：设种子 ⇒ 生产路径注入并打出与 C# 逐项相同的参照序列。
#[tokio::test]
async fn seed_env_injection_logs_csharp_probe() {
    let dir = unique_temp_dir("set");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env("MIR2_RNG_SEED", "42")
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
    // 读 probe 有超时（READY_BOUND）；到界后照常 tree-kill + 断言红。
    let text = wait_log_contains(&log, "SEED_PROBE", READY_BOUND).await;
    kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
    assert_no_leftover_game_svr(&dir).await;
    assert!(
        text.contains("RNG 同种子注入：seed=42"),
        "应有同种子注入行；实得日志：\n{text}"
    );
    assert!(
        text.contains(SEED42_PROBE),
        "参照序列必须与 C# 钩子的 SEED_PROBE 逐项相同；实得日志：\n{text}"
    );
}

/// 判据 2：不设变数 ⇒ 生产路径一字不改（无注入行）。
#[tokio::test]
async fn unset_seed_env_is_inert() {
    let dir = unique_temp_dir("unset");
    let conf = write_conf(&dir);
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env_remove("MIR2_RNG_SEED")
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
        !text.contains("SEED_PROBE") && !text.contains("RNG 同种子注入"),
        "未设变数时不得出现注入行（生产路径一字不改）；实得日志：\n{text}"
    );
}

/// 判据 3：非法值 ⇒ **≤N 秒内**非零退出 + 点名变数（fail-closed），且**不留 `game-svr`**。
///
/// **有界**是这条判据的一半：到界未退出一律判红（因子进程树 kill），不许像 T-30 finding 那样挂死
/// 9 分钟并泄漏服务进程。子进程 stdout/stderr、退出码、耗时都进断言消息（证据四件套的「原始输出」）。
#[tokio::test]
async fn invalid_seed_env_exits_nonzero() {
    let dir = unique_temp_dir("bad");
    let conf = write_conf(&dir);
    // 用**文件**而非管道收 stdout/stderr：管道在句柄继承竞态下可能迟迟等不到 EOF ⇒ 文件不会。
    let log = dir.join("stdout.txt");
    let out = std::fs::File::create(&log).unwrap();
    let err = out.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env("MIR2_RNG_SEED", "not-an-int")
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
    let status = match tokio::time::timeout(INVALID_SEED_EXIT_BOUND, child.wait()).await {
        Ok(Ok(status)) => Some(status),
        Ok(Err(e)) => panic!("等待 game-svr 退出失败：{e}"),
        Err(_) => None,
    };
    let elapsed = started.elapsed();
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    if status.is_none() {
        // 有界兜底：因子进程树 kill + 收尸 ⇒ 再报红（含耗时与子进程原始输出）。
        kill_tree_and_reap(&mut child, pid, Duration::from_secs(5)).await;
        assert_no_leftover_game_svr(&dir).await;
        panic!(
            "非法 MIR2_RNG_SEED 必须 ≤{INVALID_SEED_EXIT_BOUND:?} 退出（oracle `int.TryParse` 失败 \
             ⇒ `Environment.Exit(97)` 立即；Rust 实测 ~1s）；到界 {elapsed:?} 仍未退出 \
             ⇒ 已因子进程树 kill（防挂死/防泄漏）。子进程原始输出：\n{text}"
        );
    }
    let code = status.unwrap().code();
    assert_no_leftover_game_svr(&dir).await;
    println!("[T-30 诊断] invalid MIR2_RNG_SEED ⇒ exit={code:?} elapsed={elapsed:?}\n{text}");
    assert!(
        code.is_some_and(|c| c != 0),
        "非法 MIR2_RNG_SEED 必须非零退出（oracle: TryParse 失败 ⇒ Environment.Exit(97)）；\
         实得 exit={code:?}，耗时 {elapsed:?}；子进程原始输出：\n{text}"
    );
    assert!(
        text.contains("MIR2_RNG_SEED"),
        "错误信息应点名 MIR2_RNG_SEED；实得：\n{text}"
    );
}
