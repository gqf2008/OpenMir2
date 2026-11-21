//! **L2-4 区 tick 看门狗（屏障活性兜底）**
//!
//! 背景：L2-4 的 N>1 字形是「每区 owner 线程跑 `phase1→snapshot→barrier→phase2`」。
//! `std::sync::Barrier` **没有超时接口**：任一区在 phase1/phase2 panic 或卡死 ⇒ 其余区
//! **永久阻塞在 `wait`** ⇒ 整机挂死而不是单区失败。
//!
//! 本模块按既有语义（「任一片失败整机 Abort」）兜底：
//! - 每区在**完成一个 tick 后**写一次心跳（`ZoneHeartbeats::completed`）；
//! - 独立线程周期性检查；任一区 `now - last_complete > threshold` ⇒ 打 ERR 并
//!   **abort 整个进程**；
//! - **不改 stdlib 语义**（不给 Barrier 加超时、不换实现）。
//!
//! 可测性：判定是纯函数（[`is_stalled`] / [`ZoneHeartbeats::stalled_zones`]），
//! 触发动作用**可注入的 aborter**（生产 = `std::process::abort`；测试 = 记录标志），
//! 因此「检测 + 触发路径」都能在测试里红检，而不必真的杀测试进程。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::zone::ZoneId;

/// 缺省 tick 超时（ms）。
///
/// **为什么是 5000**：交付口径是 tick P99 ≤ 200ms（`docs/10k-shard-design` O3）；5000 = **25×
/// 预算**。取 25× 的理由：①远大于正常 tick 与 GC/调度抖动（否则会把「慢」误判成「卡死」并
/// abort 整机）；②仍远小于玩家可感知的「服务器无响应」（5s 内触发兜底，不会让全服静默挂死
/// 几十秒）。本值是 O(1) 的安全阀，不是性能目标；真正的性能判据在 `region_barrier_us` 探针。
pub const DEFAULT_TICK_TIMEOUT_MS: u64 = 5_000;

/// 配置的环境变量名。
pub const TIMEOUT_ENV: &str = "MIR2_ZONE_TICK_TIMEOUT_MS";

/// 解析超时值（`None` = 未设置 ⇒ 缺省）。**纯函数**，单测直接覆盖。
///
/// **fail-closed**：设置了但不是非零整数 ⇒ `Err`（拒绝启动）；`0` 没有意义（不许关掉兜底）。
///
/// # Errors
/// 非法/空/0 ⇒ 错误文本。
pub fn parse_timeout(raw: Option<&str>) -> Result<u64, String> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_TICK_TIMEOUT_MS);
    };
    let text = raw.trim();
    if text.is_empty() {
        return Err(format!(
            "{TIMEOUT_ENV} 为空（缺省 {DEFAULT_TICK_TIMEOUT_MS}）"
        ));
    }
    let ms: u64 = text
        .parse()
        .map_err(|_| format!("{TIMEOUT_ENV}={raw} 不是非负整数"))?;
    if ms == 0 {
        return Err(format!(
            "{TIMEOUT_ENV}=0 没有意义（必须 >0，缺省 {DEFAULT_TICK_TIMEOUT_MS}）"
        ));
    }
    Ok(ms)
}

/// 读取 [`TIMEOUT_ENV`]（缺省 [`DEFAULT_TICK_TIMEOUT_MS`]）。
///
/// # Errors
/// 非 Unicode / 非法 / 空 / 0 ⇒ 错误文本（调用方拒绝启动）。
pub fn timeout_from_env() -> Result<u64, String> {
    match std::env::var_os(TIMEOUT_ENV) {
        None => Ok(DEFAULT_TICK_TIMEOUT_MS),
        Some(raw) => {
            let raw = raw
                .into_string()
                .map_err(|_| format!("{TIMEOUT_ENV} 不是合法 Unicode"))?;
            parse_timeout(Some(&raw))
        }
    }
}

/// 判定：`now - last` **严格大于** `threshold` ⇒ 卡死。
///
/// 用 `saturating_sub`（`now < last` 不 panic；时钟回拨视作 0 差值）。
#[must_use]
pub fn is_stalled(last_ms: u64, now_ms: u64, threshold_ms: u64) -> bool {
    now_ms.saturating_sub(last_ms) > threshold_ms
}

/// 心跳与看门狗共用的**单调时钟**。
///
/// 区线程 beat 与看门狗 `check_once` 必须用**同一个 epoch**：否则 `now - last` 会是巨大值
/// 或负数（`saturating_sub` 只会把它变成 0，永远不会判卡死）。[`ZoneWatch`] 负责把同一个
/// 时钟分给两边。
#[derive(Clone, Copy, Debug)]
pub struct WatchdogClock(Instant);

impl WatchdogClock {
    /// 以当前时刻为 epoch。
    #[must_use]
    pub fn start() -> Self {
        Self(Instant::now())
    }

    /// 自 epoch 起的毫秒数（`u64` 饱和）。
    #[must_use]
    pub fn now_ms(self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// 每区「最近一次完成 tick」的时刻表（ms，单调时钟）。
#[derive(Debug)]
pub struct ZoneHeartbeats {
    slots: Vec<AtomicU64>,
}

impl ZoneHeartbeats {
    /// 以当前时刻初始化（避免一启动就判卡死）。
    #[must_use]
    pub fn new(zone_count: usize, now_ms: u64) -> Self {
        Self {
            slots: (0..zone_count).map(|_| AtomicU64::new(now_ms)).collect(),
        }
    }

    /// 区完成一个 tick（phase2 之后）时调用。
    ///
    /// # Panics
    /// `zone` 越界 ⇒ panic（编程错误）。
    pub fn completed(&self, zone: ZoneId, now_ms: u64) {
        self.slots[zone.index()].store(now_ms, Ordering::Relaxed);
    }

    /// 某区是否卡死。
    ///
    /// # Panics
    /// `zone` 越界 ⇒ panic。
    #[must_use]
    pub fn stalled(&self, zone: ZoneId, now_ms: u64, threshold_ms: u64) -> bool {
        is_stalled(
            self.slots[zone.index()].load(Ordering::Relaxed),
            now_ms,
            threshold_ms,
        )
    }

    /// 某区最近一次完成 tick 的时刻（ms；诊断/测试）。
    ///
    /// # Panics
    /// `zone` 越界 ⇒ panic。
    #[must_use]
    pub fn last_completed_ms(&self, zone: ZoneId) -> u64 {
        self.slots[zone.index()].load(Ordering::Relaxed)
    }

    /// 全部卡死的区（按区号升序；空 = 都健康）。
    #[must_use]
    pub fn stalled_zones(&self, now_ms: u64, threshold_ms: u64) -> Vec<ZoneId> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, last)| is_stalled(last.load(Ordering::Relaxed), now_ms, threshold_ms))
            .map(|(i, _)| ZoneId::new(u32::try_from(i).expect("区数 <= u32::MAX")))
            .collect()
    }
}

/// 看门狗本体：共享心跳表 + 阈值 + 可注入的 aborter。
pub struct TickWatchdog {
    heartbeats: Arc<ZoneHeartbeats>,
    threshold_ms: u64,
    aborter: Arc<dyn Fn() + Send + Sync>,
}

impl std::fmt::Debug for TickWatchdog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TickWatchdog")
            .field("threshold_ms", &self.threshold_ms)
            .finish()
    }
}

impl TickWatchdog {
    /// 生产构造：abort = `std::process::abort`（与「任一片失败整机 Abort」一致）。
    #[must_use]
    pub fn new(heartbeats: Arc<ZoneHeartbeats>, threshold_ms: u64) -> Self {
        Self {
            heartbeats,
            threshold_ms,
            aborter: Arc::new(|| std::process::abort()),
        }
    }

    /// 测试构造：注入 aborter（生产不得使用）。
    #[must_use]
    pub fn with_aborter(
        heartbeats: Arc<ZoneHeartbeats>,
        threshold_ms: u64,
        aborter: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            heartbeats,
            threshold_ms,
            aborter,
        }
    }

    /// 心跳表（区线程持有同一个 `Arc`）。
    #[must_use]
    pub fn heartbeats(&self) -> &Arc<ZoneHeartbeats> {
        &self.heartbeats
    }

    /// 检查一次：有区卡死 ⇒ 打 ERR、调用 aborter、返回该区；否则 `None`。
    ///
    /// 返回 `Some` 后生产 aborter 已 abort；测试 aborter 返回时本函数仍返回该区，便于断言。
    pub fn check_once(&self, now_ms: u64) -> Option<ZoneId> {
        let stalled = self.heartbeats.stalled_zones(now_ms, self.threshold_ms);
        let zone = stalled.first().copied()?;
        crate::glog!(
            "ERR",
            "区 {} 已 {}ms 未完成 tick（阈值 {}ms）⇒ abort 整个进程（屏障活性兜底；卡死区={stalled:?}）",
            zone,
            now_ms.saturating_sub(
                self.heartbeats
                    .slots
                    .get(zone.index())
                    .map_or(0, |s| s.load(Ordering::Relaxed))
            ),
            self.threshold_ms
        );
        (self.aborter)();
        Some(zone)
    }

    /// 起独立看门狗线程（`poll_ms` 建议 = `threshold/4`）。
    ///
    /// 生产 aborter 是 `process::abort`，线程不会从 `check_once` 返回；测试注入的 aborter
    /// 返回后本线程退出（避免测试里空转）。时钟自取（测试便利）；生产接线用
    /// [`Self::spawn_stoppable`] 与区线程共用同一个 [`WatchdogClock`]。
    #[must_use]
    pub fn spawn(self, poll_ms: u64) -> std::thread::JoinHandle<()> {
        self.spawn_stoppable(
            poll_ms,
            WatchdogClock::start(),
            Arc::new(AtomicBool::new(false)),
        )
    }

    /// 可停止版本：`clock` 与区线程共用（心跳/检查必须同 epoch）；`stop` 置真后线程在
    /// 一个 ≤50ms 的切片内退出（测试用；生产不设 stop，靠 abort 终止）。
    ///
    /// 检查周期 `poll_ms` 内按 ≤50ms 切片睡眠，因此长阈值也不会让 `join` 等整段阈值。
    #[must_use]
    pub fn spawn_stoppable(
        self,
        poll_ms: u64,
        clock: WatchdogClock,
        stop: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        let poll = Duration::from_millis(poll_ms.max(1));
        let step = poll.min(Duration::from_millis(50));
        std::thread::Builder::new()
            .name("m2-zone-watchdog".to_string())
            .spawn(move || {
                let mut since_check = Duration::ZERO;
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(step);
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    since_check += step;
                    if since_check < poll {
                        continue;
                    }
                    since_check = Duration::ZERO;
                    if self.check_once(clock.now_ms()).is_some() {
                        return;
                    }
                }
            })
            .expect("起区看门狗线程失败")
    }
}

/// **N>1** 的生产看门狗组：心跳表 + 共用时钟 + **按需 arm** 的看门狗线程。
///
/// N=1 时**永不 arm**（[`Self::watchdog_started`] == false）：单区走原单线程路径，区驱动
/// 零参与（与 `region_loop::ZoneDriverPlan::SingleThread` 是同一决定的两半）。
///
/// **为什么惰性 arm**：地图/ctor/MonGen 装载常超阈值（缺省 5s）。若建表即起线程，
/// 装载期"没有 tick"会被判成卡死 ⇒ 健康进程被启动期 abort。接线（段 6）由区 0 在
/// **第一轮 barrier2 之后**调 [`Self::arm`]；之后任区 phase2 卡死 ⇒ 心跳停滞 ⇒ 线程
/// abort 整机（既有「任一片失败整机 Abort」语义）。关服时 [`Self::disarm`] 让线程退出，
/// 避免存档期被误 abort。
pub struct ZoneWatch {
    zone_count: usize,
    threshold_ms: u64,
    aborter: Option<Arc<dyn Fn() + Send + Sync>>,
    stop: Arc<AtomicBool>,
    heartbeats: Arc<ZoneHeartbeats>,
    clock: WatchdogClock,
    /// arm 之后才有线程。
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl std::fmt::Debug for ZoneWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZoneWatch")
            .field("watchdog_started", &self.watchdog_started())
            .finish()
    }
}

impl ZoneWatch {
    /// 生产构造：跨阈值 ⇒ `process::abort`；N=1 ⇒ 永不 arm。
    #[must_use]
    pub fn start(zone_count: usize, threshold_ms: u64) -> Self {
        Self::start_with(
            zone_count,
            threshold_ms,
            None,
            Arc::new(AtomicBool::new(false)),
        )
    }

    /// 可注入构造：`aborter = None` ⇒ 生产 `process::abort`；`stop` 置真即退出线程。
    ///
    /// **N=1 永不 arm**：没有区线程可卡，也不该让单区路径有 abort 源。
    #[must_use]
    pub fn start_with(
        zone_count: usize,
        threshold_ms: u64,
        aborter: Option<Arc<dyn Fn() + Send + Sync>>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let clock = WatchdogClock::start();
        let heartbeats = Arc::new(ZoneHeartbeats::new(zone_count, clock.now_ms()));
        Self {
            zone_count,
            threshold_ms,
            aborter,
            stop,
            heartbeats,
            clock,
            thread: Mutex::new(None),
        }
    }

    /// 启动看门狗线程（**幂等**；N=1 ⇒ 恒 `false`）。
    ///
    /// 由区 0 在**第一轮 barrier2 之后**调用（装载期不跑，见类型文档）。
    pub fn arm(&self) -> bool {
        if self.zone_count <= 1 {
            return false;
        }
        let mut slot = self.thread.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_some() {
            return true;
        }
        let watchdog = match self.aborter.as_ref() {
            Some(a) => TickWatchdog::with_aborter(
                Arc::clone(&self.heartbeats),
                self.threshold_ms,
                Arc::clone(a),
            ),
            None => TickWatchdog::new(Arc::clone(&self.heartbeats), self.threshold_ms),
        };
        *slot = Some(watchdog.spawn_stoppable(
            (self.threshold_ms / 4).max(1),
            self.clock,
            Arc::clone(&self.stop),
        ));
        true
    }

    /// 关服/退出：让看门狗线程自行结束（不得在存档期 abort）。
    pub fn disarm(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// 看门狗线程是否启动（arm 之后且 N>1 ⇒ `true`）。
    #[must_use]
    pub fn watchdog_started(&self) -> bool {
        self.thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// 心跳表（区线程 beat / 段 6 传给 `run_zone_tick`）。
    #[must_use]
    pub fn heartbeats(&self) -> &Arc<ZoneHeartbeats> {
        &self.heartbeats
    }

    /// 共用时钟（与看门狗同一 epoch）。
    #[must_use]
    pub fn clock(&self) -> WatchdogClock {
        self.clock
    }

    /// 当前时刻（与看门狗同一 epoch）。
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.clock.now_ms()
    }

    /// 区完成一个 tick（phase2 之后）。
    ///
    /// # Panics
    /// `zone` 越界 ⇒ panic（编程错误）。
    pub fn beat(&self, zone: ZoneId) {
        self.heartbeats.completed(zone, self.clock.now_ms());
    }

    /// 某区是否卡死（诊断/测试；生产由看门狗线程判）。
    ///
    /// # Panics
    /// `zone` 越界 ⇒ panic。
    #[must_use]
    pub fn stalled(&self, zone: ZoneId, threshold_ms: u64) -> bool {
        self.heartbeats
            .stalled(zone, self.clock.now_ms(), threshold_ms)
    }

    /// 取走线程句柄（测试 join 用；未 arm ⇒ `None`）。
    #[must_use]
    pub fn into_thread(self) -> Option<std::thread::JoinHandle<()>> {
        self.thread
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// **判定牙**：严格 `>`；边界（差值 == 阈值）不算卡死；时钟回拨不 panic。
    ///
    /// 红检：把 `>` 改成 `>=` ⇒ 边界断言红；去掉 `saturating_sub`（改普通减法）⇒
    /// `now < last` 用例 panic ⇒ 红。
    #[test]
    fn is_stalled_is_strict_and_saturating() {
        assert!(!is_stalled(100, 100 + 5_000, 5_000), "差值 == 阈值不算卡死");
        assert!(is_stalled(100, 100 + 5_001, 5_000));
        assert!(is_stalled(0, 1, 0), "阈值 0 时任何前进都判卡死");
        assert!(!is_stalled(0, 0, 0));
        assert!(!is_stalled(500, 100, 5_000), "时钟回拨不 panic、不判卡死");
    }

    /// **表牙**：只报真正超时的区，按区号升序；`completed` 会刷新心跳。
    #[test]
    fn heartbeats_track_only_stale_zones() {
        let hb = ZoneHeartbeats::new(3, 1_000);
        hb.completed(ZoneId::new(1), 9_000);
        let stalled = hb.stalled_zones(10_000, 5_000);
        assert_eq!(stalled, vec![ZoneId::new(0), ZoneId::new(2)]);
        assert!(!hb.stalled(ZoneId::new(1), 10_000, 5_000));
        hb.completed(ZoneId::new(2), 9_500);
        assert_eq!(hb.stalled_zones(10_000, 5_000), vec![ZoneId::new(0)]);
    }

    /// **触发路径牙**：注入 aborter，检查被调用一次且返回卡死区；健康时 aborter 不被调用。
    ///
    /// 红检：删掉 `check_once` 里的 `(self.aborter)()` ⇒ 计数断言红；把 `stalled_zones`
    /// 改成恒空 ⇒ 第一条断言红。
    #[test]
    fn check_once_calls_aborter_exactly_once_for_a_stalled_zone() {
        let hb = Arc::new(ZoneHeartbeats::new(2, 0));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&calls);
        let watchdog = TickWatchdog::with_aborter(
            Arc::clone(&hb),
            DEFAULT_TICK_TIMEOUT_MS,
            Arc::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
        assert_eq!(watchdog.check_once(1_000), None, "都健康 ⇒ None");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "健康时不得触发 abort");
        // 让区 1 完成、区 0 停留；时间推过阈值。
        hb.completed(ZoneId::new(1), DEFAULT_TICK_TIMEOUT_MS + 100);
        let now = DEFAULT_TICK_TIMEOUT_MS + 1;
        assert_eq!(watchdog.check_once(now), Some(ZoneId::new(0)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// **线程牙**：注入 aborter 的看门狗在超时后自行退出（不空转、不挂测试进程）。
    #[test]
    fn watchdog_thread_exits_after_detection() {
        let hb = Arc::new(ZoneHeartbeats::new(1, 0));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&calls);
        let watchdog = TickWatchdog::with_aborter(
            Arc::clone(&hb),
            20, // 20ms 阈值，便于测试
            Arc::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let handle = watchdog.spawn(5);
        handle.join().expect("看门狗线程必须在检测后退出");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "恰触发一次");
    }

    /// **配置牙**：缺省合法；空/0/负值/非整数 fail-closed（纯解析，不依赖外部环境）。
    ///
    /// 红检：把 `ms == 0` 的拒绝删掉 ⇒ `parse_timeout(Some("0"))` 变 Ok ⇒ 红。
    #[test]
    fn parse_timeout_is_fail_closed() {
        assert_eq!(
            parse_timeout(None).expect("未设置 ⇒ 缺省"),
            DEFAULT_TICK_TIMEOUT_MS
        );
        assert_eq!(parse_timeout(Some(" 5000 ")).expect("空白可修剪"), 5_000);
        assert_eq!(parse_timeout(Some("1")).expect("最小合法值"), 1);
        for bad in ["", "   ", "0", "-1", "x", "1.5"] {
            assert!(parse_timeout(Some(bad)).is_err(), "非法值必须拒绝：{bad:?}");
        }
    }

    /// **接线牙**：N=1 永不 arm；N>1 **arm 之前不建线程、arm 之后启动**；beat 后区不判卡死。
    ///
    /// 红检：把 `arm` 的 `zone_count <= 1` 判断删掉 ⇒ N=1 断言红；让 `arm` 不 spawn ⇒
    /// `watchdog_started` 断言红；删掉 `beat` 的 `completed` ⇒ `stalled` 断言红。
    #[test]
    fn zone_watch_starts_only_for_multi_zone_and_beats_advance() {
        let single = ZoneWatch::start(1, 60_000);
        assert!(!single.arm(), "N=1 永不 arm");
        assert!(
            !single.watchdog_started(),
            "N=1 走原单线程路径 ⇒ 不建看门狗线程"
        );
        assert!(single.into_thread().is_none());

        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&calls);
        let multi = ZoneWatch::start_with(
            2,
            60_000,
            Some(Arc::new(move || {
                c.fetch_add(1, Ordering::SeqCst);
            })),
            Arc::clone(&stop),
        );
        assert!(
            !multi.watchdog_started(),
            "arm 之前不得建线程（装载期不能误 abort）"
        );
        assert!(multi.arm(), "N>1 arm 成功");
        assert!(multi.arm(), "arm 幂等");
        assert!(multi.watchdog_started(), "arm 之后必须有看门狗线程");
        multi.beat(ZoneId::new(0));
        multi.beat(ZoneId::new(1));
        assert!(!multi.stalled(ZoneId::new(0), 60_000));
        assert!(!multi.stalled(ZoneId::new(1), 60_000));
        multi.disarm();
        multi
            .into_thread()
            .expect("N>1 有线程")
            .join()
            .expect("看门狗线程按 disarm 退出");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "健康心跳期间不得触发 abort"
        );
    }
}
