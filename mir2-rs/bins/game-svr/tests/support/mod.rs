//! 进程级 relay 测试共用的**无竞态端口 / 临时目录**工具（卡 `openmir2-m2-flaky-relay-testports`）。
//!
//! 两个 target（`bins/game-svr/tests/relay.rs`、`bins/game-gate/tests/relay.rs`）共用**同一份**
//! 实现：`game-gate` 用 `#[path = "../../game-svr/tests/support/mod.rs"] mod support;` 引入，
//! 避免两份随时会漂移的拷贝（判据口径的「唯一真源」）。
//!
//! ## 为什么不能只用 `free_port()`
//!
//! `free_port()` =「`bind(:0)` → 读端口 → drop」是 TOCTOU：端口一释放就回到 OS 临时端口池，
//! **同一 test binary 的并行用例**随后 `bind("127.0.0.1:0")`（假 GameSvr / 假 DBSvr / 假 IDS 等）
//! 就可能拿到同一个端口，把它抢走。实测症状（owner 在 master `f2eb43d6` 的**合并树**上跑
//! `cargo test --workspace --offline`，2026-10-13 05:12）：
//!
//! - `bins/game-gate/tests/relay.rs` 的 `relays_open_data_heartbeat_and_close` 与
//!   `bad_login_is_kicked_and_nothing_is_forwarded` **FAILED**，报错
//!   `通常每个套接字地址…只允许使用一次。(os error 10048)`；
//! - cargo 因 **fail-fast** 提前收工（只跑到 6 个 target）⇒ 后续 target 根本没跑，判据面被截断；
//! - 单跑 `cargo test -p game-gate --test relay` 却 **11/11 绿**，workspace 复跑也全绿。
//!
//! 交叉连线是怎么发生的：用例 A 的 `gate_port = free_port()` 释放了端口 P；并行用例 B 的
//! `bind(:0)` 假服务抢先绑上 P；用例 A 的网关子进程 bind P 失败（10048）退出，而用例 A 的客户端
//! `connect_with_retry(P)` 却连到了**用例 B 的假服务**上 —— 于是两边都读到不属于自己的数据/空日志。
//!
//! ## 口径（照已 approve 的 `openmir2-m2-flaky-relay-nowm@00af1c78` 样板）
//!
//! 1. **候选端口来自进程内原子计数器、且落在非临时段** `[PORT_LO, PORT_HI)`：Windows 动态端口段
//!    默认 `49152..65535`（本机 `netsh int ipv4 show dynamicport tcp` = Start 49152 / 16384）
//!    ⇒ 并行用例的 `bind(:0)` **不可能**返回我们的候选端口；原子计数器保证同进程内用例不互相重号。
//! 2. **就绪门**：端口是否真被**本用例的子进程** bind，以子进程自己在 `TcpListener::bind` **成功之后**
//!    打印的 `…[{addr}:{port}]已启动…` 为准（`game-svr` `main.rs:459` / `game-gate` `main.rs:134`）。
//!    没等到就绪标记（子进程退出 = bind 失败）⇒ 把子进程全文日志打进 panic，**fail-closed**；
//!    绝不再「先 connect 再赌」——那会连上别人的监听。
//! 3. **有界重试**：`next_port()` 会跳过已被外部进程占用的候选（端口段内最多试 `SPAN` 次）。
//! 4. **临时目录用例级唯一**：`pid + 原子计数`（同族 LESSON：时钟粒度不足 / 并行用例撞路径），
//!    并用 RAII 保证**断言 panic 的失败路径**也删目录。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

/// 候选端口段下界（避开 Windows 动态段 `49152..65535` 与 Linux 默认段 `32768..60999`）。
pub const PORT_LO: u16 = 20_000;
/// 候选端口段上界（开区间）——`32000 < 32768`，与两个平台的动态段都不相交。
pub const PORT_HI: u16 = 32_000;

static PORT_SEQ: AtomicU32 = AtomicU32::new(0);

/// 取一个**进程内唯一、且落在非临时段**的候选端口。
///
/// 「bind 后立刻 drop」只用来**筛掉已被外部进程占用**的候选（有界重试）；真正的就绪契约是
/// [`wait_child_ready`] 看到的子进程 bind 成功标记 —— 见本文件头部口径 ①/②。
pub fn next_port() -> u16 {
    let span = u32::from(PORT_HI - PORT_LO);
    // 进程内原子唯一 + 按 pid 错开起点：即便 cargo 并行跑多个 test binary，两个进程也不会从
    // 同一段位置起步；若候选恰被外部（含另一个 test binary）占用，`bind` 失败就换下一个。
    // `17` 与 `span` 互素 ⇒ `seq` 在 `0..span` 内取到的偏移两两不同。
    let base = std::process::id().wrapping_mul(977);
    for _ in 0..span {
        let seq = PORT_SEQ.fetch_add(1, Ordering::Relaxed);
        let off = base.wrapping_add(seq.wrapping_mul(17)) % span;
        let port = PORT_LO + u16::try_from(off).expect("段内偏移必落 u16");
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    panic!("测试端口段 [{PORT_LO},{PORT_HI}) 已全被占用；检查是否有残留 game-svr/game-gate 进程");
}

static TEMP_DIR_SEQ: AtomicU64 = AtomicU64::new(0);

/// 用例级唯一临时目录：`pid + 原子计数`（同族 LESSON；不要只靠 pid 或时钟）。
pub fn unique_temp_dir(tag: &str) -> PathBuf {
    let seq = TEMP_DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{tag}-{}-{seq}", std::process::id()))
}

/// RAII 清理：无论用例正常结束还是**断言 panic**（展开）都清掉临时目录。
pub struct TempDirGuard(pub PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// 有超时地轮询日志直到出现 `needle`，返回**最后一次**读到的全文。
///
/// 子进程 stdout 重定向到文件后，`println!` 与测试的 `read_to_string` 之间存在可见性延迟
/// ⇒ 绝不能用「连接成功后立刻读一次再断言」；超时后把全文交给调用方打进断言消息。
pub async fn wait_log_contains(log: &Path, needle: &str, timeout: Duration) -> String {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        if text.contains(needle) || tokio::time::Instant::now() >= deadline {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 等子进程日志出现就绪标记 `:{port}]已启动`（**该行在 `TcpListener::bind` 成功之后才打印**）；
/// 若子进程在此之前退出（bind 失败 / 配置错）或超时，则提前返回当时的全文。
///
/// 返回全文里**是否含 marker** 即「就绪与否」的判据；调用方据此 `panic`（fail-closed）或换端口重试。
pub async fn wait_child_ready(
    proc: &mut tokio::process::Child,
    log: &Path,
    port: u16,
    timeout: Duration,
) -> String {
    let marker = format!(":{port}]已启动");
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        if text.contains(&marker) {
            return text;
        }
        if matches!(proc.try_wait(), Ok(Some(_))) {
            return text;
        }
        if tokio::time::Instant::now() >= deadline {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// 断言子进程确实 bind 成功（就绪门）；否则把全文日志打进 panic，**fail-closed**。
pub fn require_ready(text: &str, port: u16) {
    let marker = format!(":{port}]已启动");
    assert!(
        text.contains(&marker),
        "子进程未在超时内 bind 成功（端口 {port} 被外部进程占用？）⇒ fail-closed；子进程日志：\n{text}"
    );
}

#[cfg(test)]
mod teeth {
    use super::{next_port, PORT_HI, PORT_LO};

    /// **改坏必红**：候选端口必须落在与 OS 动态段不相交的 `[PORT_LO, PORT_HI)`。
    ///
    /// 把 `next_port` 退回旧的 `free_port()`（`bind(:0)` → 取值 → drop）⇒ 返回值进入 OS 动态段，
    /// 本条立即红。
    #[test]
    fn ports_live_outside_the_os_dynamic_range() {
        for _ in 0..64 {
            let p = next_port();
            assert!(
                (PORT_LO..PORT_HI).contains(&p),
                "候选端口 {p} 必须落在与动态段不相交的 [{PORT_LO},{PORT_HI})，\
                 否则并行用例的 bind(:0) 会把它抢走"
            );
        }
    }

    /// **改坏必红**：同进程内（含并行线程）不得重号。旧口径只靠 OS 探测，原子计数器缺失时本用例会红。
    #[test]
    fn ports_are_unique_across_parallel_callers() {
        use std::sync::Mutex;
        let got = Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..32 {
                scope.spawn(|| {
                    let mut local = Vec::new();
                    for _ in 0..8 {
                        local.push(next_port());
                    }
                    got.lock().unwrap().extend(local);
                });
            }
        });
        let got = got.into_inner().unwrap();
        let mut uniq = got.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(
            uniq.len(),
            got.len(),
            "并行调用 must 不重号：{} 次调用只有 {} 个唯一端口",
            got.len(),
            uniq.len()
        );
    }

    /// 旧口径可复现失败：`free_port()` 释放的端口**未被任何东西保护**，紧接着的 bind（＝子进程
    /// 稍后的 bind）会拿到 `AddrInUse`——这正是 owner 实测 `os error 10048` 的机制。
    /// 新口径把候选放进与动态段不相交的固定段，`bind(:0)` 拿不到它。
    #[test]
    fn legacy_probe_release_port_is_left_unprotected() {
        // 旧口径（本地重实现，勿在生产用）。
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let legacy = l.local_addr().unwrap().port();
        drop(l);
        // 释放后：任一同进程的 bind(:0) / 显式 bind 都能把它占了。
        let stolen = std::net::TcpListener::bind(("127.0.0.1", legacy)).unwrap();
        let err = std::net::TcpListener::bind(("127.0.0.1", legacy)).unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::AddrInUse,
            "旧口径释放的端口可被抢走 ⇒ 子进程 bind 报 AddrInUse（Windows 10048）"
        );
        drop(stolen);
        // 新口径：候选不在动态段，故 `bind(:0)` 不可能把它交给并行用例。
        let p = next_port();
        assert!(
            !(49152..).contains(&p),
            "新候选 {p} 不应落在 Windows 动态段"
        );
    }
}
