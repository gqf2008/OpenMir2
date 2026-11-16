//! **入口三段计数**（卡 `openmir2-game-svr-entry-triple`）。
//!
//! 背景（2026-11-17 仪表态 1000 档）：DBSrv 侧 `525_sent=1000 525_fail=0` 已把入口第一段闭合，
//! 客户端 `in_world 975/1000`，剩下 **25** 个卡在「525 之后、进世界之前」。这一段服务端此前
//! **没有任何观测**（`658_sent = enter_ok` 的理由不成立：658 只说明发了 `SM_SENDNOTICE`）。
//!
//! 本模块只做**旁路计数**，三个原子量与三个调用点一一对应：
//!
//! | 计数 | 调用点（唯一） | 语义 |
//! | --- | --- | --- |
//! | [`LOGON_FRAMES_SEEN`] | `GateEvent::LoadDbRequest` 分支 | 网关侧认证通过、取角色进世界的登录事件到达 |
//! | [`ENTER_ATTEMPT`] | `DBR_LOADHUMANRCD(1100) recog==1` 回包分支入口 | 拿到角色数据、开始进世界流程 |
//! | [`ENTER_OK`] | `world.enter_map_at(..)` 返回 `Ok(id)` 分支 | 真正进入世界（会话 + 实体落图） |
//!
//! 相邻两两之差即缺口：
//! - `logon_frames_seen - enter_attempt`：收到了但没走到进世界（取数失败/重排队/重复登录/踢出）；
//! - `enter_attempt - enter_ok`：开始进世界但失败（解析失败/准入拒绝/落图失败）。
//!
//! **不改任何协议/回包/业务分支**；打印沿用 gate 的 `ENTRY_TELEMETRY` 单行风格，带 `svc=game-svr`
//! 便于与 gate 行区分。每 `PRINT_EVERY` 次事件打印一行（用 `fetch_add` 的返回值判档，
//! 不要打印时再读全局量——多线程下会漏档或重复打）。
//!
//! 计数语义的一个诚实边界：`RequeueSaving`（角色数据保存中）会把同一次登录**重新入队**，
//! 其后的新一次尝试会让 `enter_attempt` 超出 `logon_frames_seen`。所以
//! `logon ≥ attempt ≥ ok` 只在「无重排队」时成立；若读数违背它，先数日志里的
//! 「角色数据保存中,稍后重新读取」。

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// 汇总打印的事件间隔（每 N 次事件打印一行；0 = 从不打印）。
const PRINT_EVERY: u64 = 100;

/// 网关侧登录事件到达数（`GateEvent::LoadDbRequest`）。
pub static LOGON_FRAMES_SEEN: AtomicU64 = AtomicU64::new(0);
/// 开始进世界流程的次数（`DBR_LOADHUMANRCD(1100) recog==1` 回包分支入口）。
pub static ENTER_ATTEMPT: AtomicU64 = AtomicU64::new(0);
/// 真正进入世界的次数（`world.enter_map_at(..)` 返回 `Ok`）。
pub static ENTER_OK: AtomicU64 = AtomicU64::new(0);

fn bump(counter: &AtomicU64) -> Option<String> {
    let n = counter.fetch_add(1, Relaxed) + 1;
    if PRINT_EVERY == 0 || !n.is_multiple_of(PRINT_EVERY) {
        return None;
    }
    Some(summary_line())
}

/// 网关侧登录事件 +1；该次触达整百 ⇒ 返回一行汇总（调用方负责 `glog!`）。
#[must_use]
pub fn record_logon() -> Option<String> {
    bump(&LOGON_FRAMES_SEEN)
}

/// 进世界尝试 +1；该次触达整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_attempt() -> Option<String> {
    bump(&ENTER_ATTEMPT)
}

/// 进世界成功 +1；该次触达整百 ⇒ 返回一行汇总。
#[must_use]
pub fn record_ok() -> Option<String> {
    bump(&ENTER_OK)
}

/// 三段计数的一行汇总（字段名固定，供日志侧机判）。
#[must_use]
pub fn summary_line() -> String {
    format!(
        "ENTRY_TELEMETRY svc=game-svr logon_frames_seen={} enter_attempt={} enter_ok={}",
        LOGON_FRAMES_SEEN.load(Relaxed),
        ENTER_ATTEMPT.load(Relaxed),
        ENTER_OK.load(Relaxed)
    )
}

/// 进程级计数器被并行用例共享 ⇒ 会互相改数的用例必须串行（否则断言「另一个计数不变」会被
/// 同模块/别模块的用例 bump 掉，表现为 flaky 而不是真红）。接线牙不碰计数器，无需加锁。
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// 整百才打印：0 / 半档不打印，第 100 次触达返回 Summary；计数器本身每次 +1。
    #[test]
    fn print_only_on_exact_multiples() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 计数器是进程级全局量：用「差值」而不是绝对值（其它用例/并行测试也会动它们）。
        let before = LOGON_FRAMES_SEEN.load(Ordering::Relaxed);
        let mut printed = 0;
        for _ in 0..PRINT_EVERY {
            if record_logon().is_some() {
                printed += 1;
            }
        }
        let after = LOGON_FRAMES_SEEN.load(Ordering::Relaxed);
        assert_eq!(after, before + PRINT_EVERY, "每次调用必须 +1");
        assert_eq!(
            printed, 1,
            "PRINT_EVERY 次里只该在第 {PRINT_EVERY} 次打印一行（打印判档必须用 fetch_add 的返回值）"
        );
    }

    /// 汇总行把三个字段名都写出来（缺字段 = 读数时看不出是 0 还是没接）。
    #[test]
    fn summary_line_carries_every_field() {
        let line = summary_line();
        assert!(line.starts_with("ENTRY_TELEMETRY svc=game-svr "), "{line}");
        for key in ["logon_frames_seen=", "enter_attempt=", "enter_ok="] {
            assert!(line.contains(key), "汇总行缺字段 {key}: {line}");
        }
    }

    /// 三个计数器互相独立：动一个不许动另一个（同族「删任一计数 ⇒ 对应断言红」的前提）。
    #[test]
    fn counters_are_independent() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = LOGON_FRAMES_SEEN.load(Ordering::Relaxed);
        let b = ENTER_ATTEMPT.load(Ordering::Relaxed);
        let c = ENTER_OK.load(Ordering::Relaxed);
        let _ = record_attempt();
        assert_eq!(
            LOGON_FRAMES_SEEN.load(Ordering::Relaxed),
            a,
            "attempt 不得动 logon"
        );
        assert_eq!(ENTER_ATTEMPT.load(Ordering::Relaxed), b + 1);
        assert_eq!(ENTER_OK.load(Ordering::Relaxed), c, "attempt 不得动 ok");
    }
}
