//! 进程级测试共用的**无竞态端口 / 就绪门**工具（卡 `openmir2-test-port-hygiene-2`）。
//!
//! 口径与 `mir2-rs/bins/game-svr/tests/support/mod.rs`（卡 `openmir2-m2-flaky-relay-testports`）
//! **逐行相同**：`PORT_LO`/`PORT_HI`/`PORT_SEQ`/`next_port()` 与那份样板源码一致，review 可直接
//! `diff` 对照。每个 crate 各自复制一份 —— 独立 test target 不跨 crate 引用 tests 模块。
//!
//! ## 为什么不能只用 `free_port()`
//!
//! `free_port()` =「`bind(:0)` → 读端口 → drop」是 TOCTOU：端口一释放就回到 OS 临时端口池，
//! 并行用例随后的 `bind("127.0.0.1:0")` 就可能拿到同一个端口把它抢走 ⇒ 子进程 bind 报
//! `AddrInUse`（Windows `os error 10048`），而客户端的 `connect_with_retry` 却连到了**别人的**
//! 监听上 —— 表现为偶发挂起 / 读到不属于自己的数据。
//!
//! ## 口径
//!
//! 1. **候选端口来自进程内原子计数器、且落在非临时段** `[PORT_LO, PORT_HI)`：Windows 动态段
//!    默认 `49152..65535` ⇒ 并行用例的 `bind(:0)` **不可能**返回我们的候选端口；原子计数器保证
//!    同进程内用例不互相重号。
//! 2. **就绪门**：端口是否真被**本用例的子进程** bind，以子进程在 `TcpListener::bind` 成功之后
//!    打印的 `…[{addr}:{port}]已启动…` 为准。没等到 ⇒ 把子进程全文日志打进 panic，**fail-closed**；
//!    绝不再「先 connect 再赌」——那会连上别人的监听。
//! 3. **有界重试**：`next_port()` 会跳过已被外部进程占用的候选（端口段内最多试 `SPAN` 次）。
//!
//! 本拷贝裁掉了样板里的 `unique_temp_dir`/`TempDirGuard`（本 crate 用例不用）与
//! `legacy_probe_release_port_is_left_unprotected` 一条牙（它靠抢 `bind(:0)` 释放的端口，
//! 复制到多个 target 只会放大 flake 面；`next_port` 的两条牙保留）。

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

/// 候选端口段下界（避开 Windows 动态段 `49152..65535` 与 Linux 默认段 `32768..60999`）。
pub const PORT_LO: u16 = 20_000;
/// 候选端口段上界（开区间）——`32000 < 32768`，与两个平台的动态段都不相交。
pub const PORT_HI: u16 = 32_000;

static PORT_SEQ: AtomicU32 = AtomicU32::new(0);

/// 取一个**进程内唯一、且落在非临时段**的候选端口。
///
/// 「bind 后立刻 drop」只用来**筛掉已被外部进程占用**的候选（有界重试）；真正的就绪契约是
/// [`gate_ready`] 看到的子进程 bind 成功标记 —— 见本文件头部口径 ①/②。
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
    panic!("测试端口段 [{PORT_LO},{PORT_HI}) 已全被占用；检查是否有残留服务进程");
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

/// 断言子进程确实 bind 成功（就绪门）；否则把全文日志打进 panic，**fail-closed**。
pub fn require_ready(text: &str, port: u16) {
    let marker = format!(":{port}]已启动");
    assert!(
        text.contains(&marker),
        "子进程未在超时内 bind 成功（端口 {port} 被外部进程占用？）⇒ fail-closed；子进程日志：\n{text}"
    );
}

/// 就绪门（一步到位）：等子进程日志出现 `:{port}]已启动`，超时即 panic 并带全文日志。
///
/// `:{port}]已启动` 是四个二进制共有的 bind-success 标记（`…[addr:port]已启动…`）。
pub async fn gate_ready(log: &Path, port: u16, timeout: Duration) {
    let text = wait_log_contains(log, &format!(":{port}]已启动"), timeout).await;
    require_ready(&text, port);
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
}
