//! **L2-4 段 4：`(网关线号, socket) → ZoneId` 的分派表**
//!
//! 入站 demux 的唯一会话键是 `(line, socket)`：`socket` 在**不同网关线之间会重复**
//! （见 `crate::GateEvent::Command` 的文档与 oracle `ChannelMessageHandler.cs`），单用 socket
//! 会把别的线的会话路由进本区。
//!
//! # 生命周期（唯一口径）
//!
//! - `GM_OPEN` ⇒ [`ZoneRouter::opened`]：插入 `(line, socket) → 区 0`；
//! - 显式分配（段 6 的测试分配缝 / 将来的登录归属）⇒ [`ZoneRouter::assign`]；
//! - `GM_CLOSE` ⇒ [`ZoneRouter::remove`]；
//! - `CertificationPending` **不改表**：那是认证前的丢帧分支，不是会话结束。
//!
//! # 裁决（Leader 2026-11-22）
//!
//! - **未命中一律区 0**（登录在区 0；fail-closed 到主机，不把事件丢进虚空）。
//! - **重复键 = last-wins 覆盖**，但计 `zone_router_overwrites`：键已存在说明生命周期里缺了
//!   一次 `Closed`，这本身就是「玩家可能被路由到错区」的诊断信号，静默覆盖不可接受。
//! - **`socket <= 0` 拒绝**（不是活客户端句柄）并计 `zone_router_rejected_socket`；表不变，
//!   `lookup` 对该键仍是区 0。
//!
//! 计数器走 [`crate::perf_counters`]：探针关闭时 `bump` 是纯 no-op（一次 relaxed 读）。
//!
//! # 接线状态
//!
//! **段 6a 已接**：`run_inner` 的 `MIR2_ZONES` 分支起 `m2-zone-demux` 线程，`route_zone_target`
//! 是本表唯一的判定点（`Opened` 登记 / `Command` 查询 / `Closed` **先查后删**）。
//! `MIR2_ZONES` 未设（N=1）时本表没有生产者：玩家全部落在区 0，原单线程路径逐字节不变。
//! 不校验区号上界——布局的区数在启动门（`zone_wiring::plan_zone_mode`）才存在。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::perf_counters;
use crate::zone::ZoneId;

/// 未命中（未登记 / 已注销 / 从未分配）时落的区：登录与既有单线程路径都在这里。
pub const HOST_ZONE: ZoneId = ZoneId::new(0);

/// [`ZoneRouter::opened`] / [`ZoneRouter::assign`] 被拒绝的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketRejected {
    /// 网关线号。
    pub line: u8,
    /// 被拒的 socket（`<= 0`）。
    pub socket: i32,
}

impl fmt::Display for SocketRejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "网关线 {} 的 socket={} 不是活客户端句柄（要求 > 0），拒绝分配",
            self.line, self.socket
        )
    }
}

impl std::error::Error for SocketRejected {}

/// `(line, socket) → ZoneId` 分派表（段 6 接进 `run_inner`；本段只有构造与查询）。
#[derive(Debug, Default)]
pub struct ZoneRouter {
    /// 键 = `(网关线号, 客户端句柄)`；值 = 该会话当前所在区。
    map: Mutex<HashMap<(u8, i32), ZoneId>>,
}

impl ZoneRouter {
    /// 空表（所有键查得区 0）。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 查会话所在区；**未命中一律 [`HOST_ZONE`]**（不返回 `Option`，调用方没有「漏路由」支）。
    #[must_use]
    pub fn lookup(&self, line: u8, socket: i32) -> ZoneId {
        self.lock()
            .get(&(line, socket))
            .copied()
            .unwrap_or(HOST_ZONE)
    }

    /// `GM_OPEN`：登记会话，初始区 0。重复键 last-wins 并计 `zone_router_overwrites`。
    ///
    /// # Errors
    /// `socket <= 0` ⇒ [`SocketRejected`]（计 `zone_router_rejected_socket`，表不变）。
    pub fn opened(&self, line: u8, socket: i32) -> Result<ZoneId, SocketRejected> {
        self.insert(line, socket, HOST_ZONE)
    }

    /// **L2-5 ③ 1b**：`(line,socket)` 是否已经在表里。
    ///
    /// `lookup` 未命中会回落区 0，这里把「真的被 assign 到区 0」与「没 assign」区分开——
    /// 后者是登录落区漏接，必须计数（`zone_router_unassigned`），不能静默回落。
    #[must_use]
    pub fn lookup_registered(&self, line: u8, socket: i32) -> Option<ZoneId> {
        self.lock().get(&(line, socket)).copied()
    }

    /// 显式分配（段 6 的测试分配缝 / 登录落区）。重复键 last-wins 并计覆盖。
    ///
    /// # Errors
    /// `socket <= 0` ⇒ [`SocketRejected`]（计 `zone_router_rejected_socket`，表不变）。
    pub fn assign(&self, line: u8, socket: i32, zone: ZoneId) -> Result<(), SocketRejected> {
        self.insert(line, socket, zone).map(|_| ())
    }

    /// `GM_CLOSE`：删除该会话（幂等）。返回删除前是否存在（诊断用）。
    pub fn remove(&self, line: u8, socket: i32) -> bool {
        self.lock().remove(&(line, socket)).is_some()
    }

    /// 表内条目数（诊断/测试）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// 表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<(u8, i32), ZoneId>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn insert(&self, line: u8, socket: i32, zone: ZoneId) -> Result<ZoneId, SocketRejected> {
        if socket <= 0 {
            perf_counters::bump(&perf_counters::ZONE_ROUTER_REJECTED_SOCKET);
            return Err(SocketRejected { line, socket });
        }
        let previous = self.lock().insert((line, socket), zone);
        if previous.is_some() {
            perf_counters::bump(&perf_counters::ZONE_ROUTER_OVERWRITES);
        }
        Ok(zone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    /// 探针守卫：计数器只在探针启用时加数，所以反例必须临时启用它；**drop 时复位**——
    /// 中途 panic 也不会把全局 `ENABLED` 留给别的测试。
    ///
    /// 与 `perf_counters_fail_closed_and_disabled_noop` 共用 [`perf_counters::TEST_LOCK`]：
    /// 两边都开关同一个全局开关，不串行就会互相踩。
    struct Probe {
        _guard: MutexGuard<'static, ()>,
        path: std::path::PathBuf,
    }

    impl Probe {
        fn on(tag: &str) -> Self {
            let guard = perf_counters::TEST_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let path = std::env::temp_dir().join(format!(
                "l2_zone_router_{}_{}.tsv",
                std::process::id(),
                tag
            ));
            let _ = std::fs::remove_file(&path);
            perf_counters::init_with_path(&path).expect("探针文件可建");
            Self {
                _guard: guard,
                path,
            }
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            perf_counters::reset_for_tests();
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn zone(id: u32) -> ZoneId {
        ZoneId::new(id)
    }

    /// **正例 1**：已知键查到被分配到的区（不是 0 兜底），且线号/socket 任一不同都不命中。
    ///
    /// 红检：`lookup` 忽略表、恒返回 `HOST_ZONE` ⇒ 本用例红。
    #[test]
    fn assigned_key_routes_to_its_zone() {
        let router = ZoneRouter::new();
        router.assign(3, 700, zone(1)).expect("活句柄可分配");
        assert_eq!(router.lookup(3, 700), zone(1));
        assert_eq!(
            router.lookup(3, 701),
            HOST_ZONE,
            "同线另一个 socket 未登记 ⇒ 区 0"
        );
        assert_eq!(
            router.lookup(4, 700),
            HOST_ZONE,
            "同 socket 另一条线未登记 ⇒ 区 0"
        );
    }

    /// **正例 2**：空表 / 未知键一律区 0（fail-closed 到主机，不丢事件）；`lookup` 不插表。
    #[test]
    fn unknown_key_routes_to_host_zone() {
        let router = ZoneRouter::new();
        assert!(router.is_empty());
        assert_eq!(router.lookup(0, 1), HOST_ZONE);
        assert_eq!(router.lookup(255, i32::MAX), HOST_ZONE);
        assert_eq!(router.len(), 0, "lookup 不得插表");
    }

    /// **正例 3**：`Opened` 插区 0、`assign` 改区、`Closed`（remove）后回到区 0。
    #[test]
    fn close_removes_the_entry_and_falls_back_to_zone_zero() {
        let router = ZoneRouter::new();
        assert_eq!(router.opened(2, 33).expect("活句柄可登记"), HOST_ZONE);
        assert_eq!(router.lookup(2, 33), HOST_ZONE, "Opened 初始区 0");
        assert_eq!(router.len(), 1);
        router.assign(2, 33, zone(1)).expect("活句柄可分配");
        assert_eq!(router.lookup(2, 33), zone(1));
        assert!(router.remove(2, 33), "存在 ⇒ remove 返回 true");
        assert_eq!(router.lookup(2, 33), HOST_ZONE, "注销后必须回到区 0");
        assert!(!router.remove(2, 33), "重复 remove 幂等（返回 false）");
        assert!(router.is_empty());
    }

    /// **反例 1（重复键）**：同键跨区覆盖 ⇒ last-wins，且 `zone_router_overwrites` **相对增量** +1；
    /// 首次插入不算覆盖。
    ///
    /// 红检：删掉「键已存在」的判断（或不再 bump `ZONE_ROUTER_OVERWRITES`）⇒ 计数断言红。
    /// 退化形态：空表首次插入 ⇒ 计数器 Δ=0（覆盖判定必须只看「已有条目」）。
    #[test]
    fn duplicate_key_last_wins_and_counts_overwrite() {
        let _probe = Probe::on("overwrite");
        let before = perf_counters::ZONE_ROUTER_OVERWRITES.load(Ordering::Relaxed);
        let router = ZoneRouter::new();
        router.assign(5, 900, zone(1)).expect("首次分配");
        assert_eq!(
            perf_counters::ZONE_ROUTER_OVERWRITES.load(Ordering::Relaxed) - before,
            0,
            "首次插入不是覆盖，不得计数"
        );
        router.assign(5, 900, zone(2)).expect("同键跨区覆盖");
        assert_eq!(router.lookup(5, 900), zone(2), "last-wins：查到最后一次");
        assert_eq!(
            perf_counters::ZONE_ROUTER_OVERWRITES.load(Ordering::Relaxed) - before,
            1,
            "重复键必须计一次覆盖"
        );
        // `Opened` 的重复键同样计数（键已存在 = 生命周期缺 `Closed`），并把会话重置到区 0。
        router.opened(5, 900).expect("活句柄");
        assert_eq!(
            router.lookup(5, 900),
            HOST_ZONE,
            "Opened 是 last-wins（重置区 0）"
        );
        assert_eq!(
            perf_counters::ZONE_ROUTER_OVERWRITES.load(Ordering::Relaxed) - before,
            2,
            "Opened 覆盖同样计数"
        );
    }

    /// **反例 2（非法 socket）**：`socket <= 0` 的分配/登记被拒、表不变，且
    /// `zone_router_rejected_socket` **相对增量** +1/次。
    ///
    /// 红检：删掉 `socket <= 0` 的守卫 ⇒ `assign` 返回 Ok 且 `lookup` 得区 1 ⇒ 红。
    /// 退化形态：`socket = 1`（最小合法句柄）必须**不**被拒。
    #[test]
    fn non_positive_socket_is_rejected_and_counted() {
        let _probe = Probe::on("rejected");
        let before = perf_counters::ZONE_ROUTER_REJECTED_SOCKET.load(Ordering::Relaxed);
        let router = ZoneRouter::new();
        let err = router
            .assign(7, 0, zone(1))
            .expect_err("socket=0 不是活句柄");
        assert_eq!(err, SocketRejected { line: 7, socket: 0 });
        assert!(
            matches!(
                router.assign(7, -5, zone(1)),
                Err(SocketRejected { socket: -5, .. })
            ),
            "负 socket 同样拒绝"
        );
        assert!(router.opened(7, 0).is_err(), "Opened 同样拒绝非法句柄");
        assert_eq!(router.lookup(7, 0), HOST_ZONE, "被拒的分配不得落表");
        assert_eq!(router.lookup(7, -5), HOST_ZONE);
        assert_eq!(router.len(), 0, "拒绝路径不得插表");
        assert_eq!(
            perf_counters::ZONE_ROUTER_REJECTED_SOCKET.load(Ordering::Relaxed) - before,
            3,
            "三次拒绝各计一次"
        );
        router.assign(7, 1, zone(1)).expect("最小合法句柄可分配");
        assert_eq!(router.lookup(7, 1), zone(1));
    }
}
