//! `MIR2_RNG_SEED` 同种子注入的**进程级**用例（M3 **T-30**）。
//!
//! 判据（口径 = C# 侧 `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`）：
//! 1. 设 `MIR2_RNG_SEED=42` ⇒ 进程日志出现与 C# 钩子同形的 `SEED_PROBE` 行，8 项与 C# 实测逐项相同；
//! 2. **不设** ⇒ 日志**没有** `SEED_PROBE` 行（生产路径一字不改）；
//! 3. 设非法值 ⇒ 进程**非零退出**且错误信息点名 `MIR2_RNG_SEED`（fail-closed，不许静默降级）。
//!
//! 抗 flaky（照 `tests/relay.rs` 的教训）：临时目录 `pid + 原子计数` 用例级唯一；子进程 stdout
//! 落盘有可见性延迟 ⇒ 一律**有超时地轮询**，不做一次性读取。

use std::time::Duration;

use tokio::process::Command;

/// C# 钩子实测的 `SEED_PROBE`（`MIR2_RNG_SEED=42`）——与
/// `mir2-rs/tests/parity/m3/RNG同种子与记录回放方案.md` §二「实测证据」一致。
const SEED42_PROBE: &str =
    "SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307";

static TEMP_DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn unique_temp_dir(tag: &str) -> std::path::PathBuf {
    let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("gs-seed-{tag}-{}-{seq}", std::process::id()))
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
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
            free_port(),
            free_port(),
            free_port()
        ),
    )
    .unwrap();
    conf
}

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
    let text = wait_log_contains(&log, "SEED_PROBE", Duration::from_secs(20)).await;
    child.kill().await.ok();
    child.wait().await.ok();
    std::fs::remove_dir_all(&dir).ok();
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
    // 先等到监听就绪行（sim 线程在此之前已起），再留一个短 settle 读全文。
    let ready = wait_log_contains(&log, "已启动", Duration::from_secs(20)).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    child.kill().await.ok();
    child.wait().await.ok();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        ready.contains("已启动"),
        "进程应正常起来；实得日志：\n{ready}"
    );
    assert!(
        !text.contains("SEED_PROBE") && !text.contains("RNG 同种子注入"),
        "未设变数时不得出现注入行（生产路径一字不改）；实得日志：\n{text}"
    );
}

/// 判据 3：非法值 ⇒ 非零退出 + 点名变数（fail-closed）。
#[tokio::test]
async fn invalid_seed_env_exits_nonzero() {
    let dir = unique_temp_dir("bad");
    let conf = write_conf(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_game-svr"))
        .arg(&conf)
        .env("MIR2_RNG_SEED", "not-an-int")
        .output()
        .await
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        !out.status.success(),
        "非法 MIR2_RNG_SEED 必须非零退出（不许静默降级）"
    );
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("MIR2_RNG_SEED"),
        "错误信息应点名 MIR2_RNG_SEED；实得：\n{text}"
    );
}
