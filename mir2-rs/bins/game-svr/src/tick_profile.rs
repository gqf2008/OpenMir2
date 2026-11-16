//! **T10b**：`sim_loop` 宿主段计时（**默认关闭**；`MIR2_TICK_PROFILE=1` 开启）。
//!
//! 背景：T10 只量了 `World::tick`（11k 实体 63.97ms），而演示栈端到端 tick ≈ 2000ms ——
//! 32× 差额在**宿主段**（`bins/game-svr` 的 `sim_loop`：P21..P32、帧生成/会话输出、AOI 下行、
//! DB 轮询、踢人/存盘落地等）。本模块把这些段按**与 `crates/world-bevy` 的 `TickProfile` 同一口径**
//! （PascalCase 段名 + `(段, 累计 ns, 调用次数)` 三元组）计时，使两套探针能拼成一张 tick 分解表。
//!
//! ## 口径与拼接
//!
//! - 宿主段名 = `Host` + 语义（`HostInbound` / `HostWorldHead` / `HostAoiDownlink` …），
//!   与 World 的 `TickStage` 变体名（`SimDup` / `OutputAoi` / `TailVerifyMapTime` …）同为
//!   PascalCase，不重名。
//! - World 的 20 段在输出里带 `World.` 前缀（`World.SimDup` …），名字取自
//!   `TickStage` 的 `Debug`（与 `tick_bench --profile` 的打印逐字一致）。
//! - `HostWorldHead` = `tick_world_head_with_specials`（`World::tick_head` + 专用怪产仔/ref 排水）
//!   的整体墙钟；`HostWorldTail` = `World::tick_tail`。两者内部再由 World 探针按 `TickStage` 拆：
//!   `HostWorldHead + HostWorldTail - ΣWorld.*` = 专用怪产仔/ref 等宿主附加成本（每窗口打印一行对账）。
//! - `HostTickTotal` = 一次循环迭代的整墙钟（含末尾的节拍 sleep）；`HostSleep` 单列，
//!   「工作段合计」= `HostTickTotal - HostSleep`。`HostUnaccounted` = 整 tick 减去各段之和的残差
//!   （段边界之间没有热点代码时应在噪声级；>1% 说明有未分段的耗时块）。
//!
//! ## 关闭时零开销（代码级）
//!
//! 缺省（环境变量未设或 ≠ `"1"`）时：`enabled == false`，
//! - [`HostTickProfile::begin`] 读一次 `bool` 后直接返回 `None` —— **不取 `Instant`**；
//! - [`HostTickProfile::end`] / [`HostTickProfile::end_tick`] 只做一次 `Option` 判别即 `return`
//!   —— **不写任何状态、不分配、不查表**。
//!
//! 每 tick 的判别次数 = 段数 × 2（begin + end）+ 2（整 tick 的 begin/end_tick）。
//! 当前宿主 **17 段 ⇒ 36 次 `Option` 判别/tick**，与 World 探针同口径（T10 实测 45–55 次
//! `Option::is_some`，见 `docs/tick-hotspot-profile-20261113.md` §2）。开启时每段另有
//! 2 次 `Instant::now()`（≈20–30ns/次，34 次/tick ≈ 1µs），相对 200ms 节拍是 5e-6。
//!
//! 计时**只观测**：不写实体、不抽 RNG、不改帧面；开启/关闭做的工作逐字节相同
//! （与 `World` 侧 `tick_profile_is_opt_in_and_behavior_neutral` 同一类判据，见本文件测试）。
//!
//! ## 逐 tick「慢 tick」段快照（卡 `openmir2-host-segment-profiler-500`）
//!
//! 上面的窗口汇总只有 **50 tick 均值** + 逐 tick **总工作段**样本，回答不了
//! 「P99 尾部那一个 tick 的耗时花在哪一段」。第二道门 `MIR2_TICK_PROFILE_SLOW_MS=N`
//! （必须同时 `MIR2_TICK_PROFILE=1`）在**单个 tick 的工作段 ≥ N ms** 时打印该 tick 的
//! 段构成（段 → ms → 占该 tick 工作段的比例，另附该 tick 内记录的命名子探针）。
//!
//! - **默认关闭**：`MIR2_TICK_PROFILE_SLOW_MS` 缺省/空串 ⇒ 静默关；显式 `0` ⇒ 关但打一条可审计
//!   `DBG`；**非法值 ⇒ 关 + 一条含「变量名 + 原文值」的 `WRN`，不杀进程**（诊断子门口径，
//!   理由与对照见 `docs/tick-profile-slow-gate-20261116.md`）。关闭时 `end`/`probe` 只多一次
//!   `Option::is_some` 判别，`end_tick` 不产生快照；
//! - 值解析先 `trim()`（与 `clock.rs`/`seed.rs` 的「前后空白合法」同口径）：`" 100 "` = 100ms；
//! - 开启时每 tick 用一个**复用缓冲**（`Vec::clear` 后 push，容量稳定后不再分配）；
//! - 快照只打印「慢 tick」，不改变任何阈值/行为，也不进 `HostUnaccounted` 对账口径。
//!
//! ## `hear_targets_at` 命名子探针（卡 `openmir2-p1-3-hear-targets-probe`）
//!
//! [`chat::hear_targets_at`](crate::chat::hear_targets_at) 在 `HostInbound` 的消息处理深处
//! （`broadcast_ref_msg_opt`）被调用，**不携带** `&mut HostTickProfile` 句柄。若把句柄穿过
//! 20+ 层消息处理函数，只会扩大写集与借用面；因此这里用**线程局部累加器**
//! [`HEAR_TARGETS_TICK`]：慢 tick 门开启时由 [`HostTickProfile::from_env`] 在 sim 线程置位，
//! `hear_targets_at` 每次进来累加 `(ns, calls)`，[`HostTickProfile::end_tick`] 每 tick 一次性
//! 取走并作为命名子项 `hear_targets_at` 记入快照（与 `HostAoi*` 同形：名字 → ms → 占工作段%）。
//!
//! - **关闭态零开销**：主门未开或慢 tick 门未设时累加器为 `None`；[`hear_targets_probe_begin`]
//!   只读一次线程局部槽 + 一次 `Option` 判别即返回 `None`——**不取 `Instant`、不分配、不打日志**。
//! - **不改行为**：与既有子探针一样只累加数字，不进 `tick_sum_ns`；每 tick 末尾取走并清零。

#![allow(clippy::missing_const_for_thread_local)]

use std::cell::Cell;
use std::time::Instant;

/// 环境变量门：**只有字面 `"1"`** 才开启（`0`/`true`/空/缺省一律关；与起栈器的 fail-closed 口径同形）。
pub const ENV_TICK_PROFILE: &str = "MIR2_TICK_PROFILE";

/// **慢 tick 段快照**门（卡 `openmir2-host-segment-profiler-500`）：工作段 ≥ N ms 的 tick
/// 打印段构成。必须同时 `MIR2_TICK_PROFILE=1`；缺省/`0`/非数字 ⇒ 关（fail-closed）。
pub const ENV_TICK_PROFILE_SLOW_MS: &str = "MIR2_TICK_PROFILE_SLOW_MS";

/// 汇总窗口（**两臂先到者**）：50 tick 或 10s 墙钟。200ms 节拍下两者等价；
/// tick 被拖慢（演示栈端到端 ≈ 2000ms）时按墙钟臂切窗，60s 的观测窗内仍有多行汇总。
pub const REPORT_TICKS: u64 = 50;

/// 汇总窗口的墙钟臂（10s；用 `window_ns`（整 tick 之和）判，不额外取时钟）。
pub const REPORT_WINDOW_NS: u64 = 10_000_000_000;

/// `hear_targets_at` 命名子项的显示名（快照里逐字出现；与 [`chat::hear_targets_at`] 同名）。
pub const HEAR_TARGETS_PROBE_NAME: &str = "hear_targets_at";

thread_local! {
    /// `hear_targets_at` 的**本 tick 累加器**（`None` = 未启用）。
    ///
    /// - `None`：默认。主门 `MIR2_TICK_PROFILE` 未开，或慢 tick 门 `MIR2_TICK_PROFILE_SLOW_MS`
    ///   未设 ⇒ [`hear_targets_probe_begin`] 不取时钟、不分配；
    /// - `Some((累计 ns, 调用次数))`：慢 tick 门开启时由 [`HostTickProfile::from_env`] 在此线程置位，
    ///   每 tick 由 [`HostTickProfile::end_tick`] 取走并清零。
    ///
    /// 用线程局部而不是字段：`chat::hear_targets_at` 在消息处理深处调用，不携带
    /// `&mut HostTickProfile`；采集与消费都在 sim 线程（`from_env` 与 tick 循环同线程）。
    static HEAR_TARGETS_TICK: Cell<Option<(u64, u64)>> = const { Cell::new(None) };
}

/// 置位/清位 `hear_targets_at` 累加器（只在 `from_env` 与测试里调用）。
fn set_hear_targets_probe(active: bool) {
    HEAR_TARGETS_TICK.with(|c| c.set(if active { Some((0, 0)) } else { None }));
}

/// **测试专用**：直接置位/清位累加器（生产只经 [`HostTickProfile::from_env`]）。
#[cfg(test)]
pub(crate) fn set_hear_targets_probe_for_test(active: bool) {
    set_hear_targets_probe(active);
}

/// **测试专用**：取走并清零「探针时钟被调用次数」（证明关闭态没取时钟）。
#[cfg(test)]
pub(crate) fn probe_clock_calls_for_test() -> u64 {
    clock_calls::take()
}

/// 取走本 tick 的 `hear_targets_at` 累计（并清零、保持启用态）。未启用 ⇒ `None`。
fn take_hear_targets_tick() -> Option<(u64, u64)> {
    HEAR_TARGETS_TICK.with(|c| {
        let prev = c.get()?;
        c.set(Some((0, 0)));
        Some(prev)
    })
}

/// **`hear_targets_at` 子探针起点**。未启用 ⇒ 一次线程局部槽读取 + `Option` 判别 ⇒ `None`：
/// **不取时钟、不分配、不打日志**（与既有慢门同口径）。
#[must_use]
#[inline]
pub fn hear_targets_probe_begin() -> Option<Instant> {
    HEAR_TARGETS_TICK.with(|c| c.get().map(|_| probe_clock_now()))
}

/// 探针时钟：**`hear_targets_at` 子探针路径上唯一的 `Instant::now()` 入口**。
///
/// 测试构建里顺带累加一个线程局部计数（[`probe_clock_calls_for_test`]），用于证明关闭态
/// [`hear_targets_probe_begin`] **根本没有调用时钟**——「不落行」只能证明没记日志，不能证明
/// 没取时钟。非测试构建下是 `Instant::now()` 的直接内联包装，无额外成本。
#[inline]
fn probe_clock_now() -> Instant {
    #[cfg(test)]
    clock_calls::bump();
    Instant::now()
}

/// 测试专用时钟计数（只在 [`probe_clock_now`] 的测试构建里累加）。
#[cfg(test)]
mod clock_calls {
    use std::cell::Cell;
    thread_local! {
        static CALLS: Cell<u64> = const { Cell::new(0) };
    }
    pub(super) fn bump() {
        CALLS.with(|c| c.set(c.get().saturating_add(1)));
    }
    /// 取走并清零计数（测试用）。
    pub(super) fn take() -> u64 {
        CALLS.with(|c| c.replace(0))
    }
}

/// **`hear_targets_at` 子探针终点**（`at == None` ⇒ 直接返回，不碰线程局部状态）。
#[inline]
pub fn hear_targets_probe_end(at: Option<Instant>) {
    let Some(at) = at else { return };
    let ns = nanos_since(at);
    HEAR_TARGETS_TICK.with(|c| {
        if let Some((total, calls)) = c.get() {
            c.set(Some((total.saturating_add(ns), calls.saturating_add(1))));
        }
    });
}

/// 宿主段计时器（缺省关闭；`MIR2_TICK_PROFILE=1` 开启）。
///
/// 一条记录 = `(段名, 累计 ns, 调用次数)`，与 `mir2_world_bevy::world::TickProfile` 的
/// `stages()` 三元组同形。
#[derive(Debug, Default)]
pub struct HostTickProfile {
    enabled: bool,
    /// `(段, 累计 ns, 调用次数)`，按首次记录顺序；窗口结束时整体取走（`take_window`）。
    rows: Vec<(&'static str, u64, u64)>,
    /// 命名**子探针** `(名, 累计 ns, 调用次数)`：只观测、**不进** `tick_sum_ns`
    /// （故不改变 `HostUnaccounted` 的对账口径），语义同 World `TickProfile::probes`。
    ///
    /// 用于把一个聚合段拆开（本批：`HostAoiDownlink` 的可见集选择/帧构造/入队/分发各支）。
    probes: Vec<(&'static str, u64, u64)>,
    /// 当前窗口已完成的 tick 数。
    window_ticks: u64,
    /// 当前窗口 `HostTickTotal` 之和。
    window_ns: u64,
    /// 本 tick 已计入各段的 ns 之和（算 `HostUnaccounted` 用；`end_tick` 末尾清零）。
    tick_sum_ns: u64,
    /// **卡 `openmir2-simmonsterai-profile`**：本 tick 的 `HostSleep` ns（`end_tick` 用它算工作段）。
    tick_sleep_ns: u64,
    /// 逐 tick 的**工作段** ns 样本（`HostTickTotal − HostSleep`）。tick 级分位数用；
    /// 每窗口保留最近 [`SAMPLE_CAP`] 个（防止长跑无界增长）。
    work_samples: Vec<u64>,
    /// 自开启起累计完成的 tick 数（诊断用）。
    total_ticks: u64,
    /// **慢 tick 快照门**：工作段 ≥ 该 ns 时输出段构成；`None` = 关（缺省）。
    /// 只有 `enabled && MIR2_TICK_PROFILE_SLOW_MS=N>0` 才为 `Some`。
    slow_threshold_ns: Option<u64>,
    /// 本 tick 的**段**记录（仅在慢 tick 门开启时填充；`end_tick` 末尾 `clear` 复用容量）。
    tick_rows: Vec<(&'static str, u64)>,
    /// 本 tick 的**命名子探针**记录（同上）。
    tick_probe_rows: Vec<(&'static str, u64)>,
    /// 最近一个慢 tick 的快照（`end_tick` 生成，`sim_loop` 每 tick 用 [`Self::take_slow_tick`] 取走）。
    pending_slow: Option<SlowTick>,
}

/// 单个慢 tick 的段快照（由 [`HostTickProfile::end_tick`] 在工作段 ≥ 阈值时生成，
/// [`HostTickProfile::take_slow_tick`] 取走）。
///
/// 打印由调用方（`sim_loop`）通过 [`SlowTick::log`] 完成——`end_tick` 本身不写日志，
/// 便于单测直接断言快照内容（而不是抓 stderr）。
#[derive(Debug, Clone, Default)]
pub struct SlowTick {
    /// 1-based tick 序号（`total_ticks + 1`）。
    pub tick: u64,
    /// 整 tick 墙钟（含 sleep）。
    pub total_ns: u64,
    /// 本 tick 的 `HostSleep`。
    pub sleep_ns: u64,
    /// 工作段 = `total_ns − sleep_ns`（判阈值的口径）。
    pub work_ns: u64,
    /// 未分段的残差（`total − Σ段`，含 sleep；与窗口 `HostUnaccounted` 同口径）。
    pub unaccounted_ns: u64,
    /// 本 tick 各段 `(段名, ns)`（含 `HostSleep`，调用方自己排除）。
    pub segments: Vec<(&'static str, u64)>,
    /// 本 tick 内记录的命名子探针 `(名, ns)`（窗口汇总口径之外的逐 tick 值）。
    pub probes: Vec<(&'static str, u64)>,
}

impl SlowTick {
    /// 打印成一行 `[TICKPROF] SLOW …`：段按耗时降序，百分比以**工作段**为分母。
    pub fn log(&self) {
        let work = self.work_ns.max(1) as f64;
        let mut segs: Vec<(&'static str, u64)> = self
            .segments
            .iter()
            .copied()
            .filter(|(n, _)| *n != "HostSleep")
            .collect();
        segs.sort_by_key(|(_, ns)| std::cmp::Reverse(*ns));
        let mut line = String::new();
        for (name, ns) in &segs {
            line.push_str(&format!(
                " {name}={:.3}({:.1}%)",
                *ns as f64 / 1e6,
                *ns as f64 / work * 100.0
            ));
        }
        let mut probes = self.probes.clone();
        probes.sort_by_key(|(_, ns)| std::cmp::Reverse(*ns));
        let probe_text = if probes.is_empty() {
            String::new()
        } else {
            let mut s = String::from(" | probes");
            for (name, ns) in &probes {
                s.push_str(&format!(
                    " {name}={:.3}({:.1}%)",
                    *ns as f64 / 1e6,
                    *ns as f64 / work * 100.0
                ));
            }
            s
        };
        crate::glog!(
            "DBG",
            "[TICKPROF] SLOW tick#{} work={:.3}ms total={:.3}ms sleep={:.3}ms unacct={:.3}ms ({:.1}%){}{}",
            self.tick,
            self.work_ns as f64 / 1e6,
            self.total_ns as f64 / 1e6,
            self.sleep_ns as f64 / 1e6,
            self.unaccounted_ns as f64 / 1e6,
            self.unaccounted_ns as f64 / work * 100.0,
            line,
            probe_text
        );
    }
}

/// 逐 tick 样本上限（每窗口最多保留这么多，丢最旧）。生产窗口是 50 tick；取 256 让
/// 测量 harness 跑 100+ tick 时仍装得下。
pub const SAMPLE_CAP: usize = 256;

/// 一个窗口的宿主汇总（由 [`HostTickProfile::take_window`] 产出）。
#[derive(Debug, Clone, Default)]
pub struct HostWindow {
    pub ticks: u64,
    pub total_ns: u64,
    pub rows: Vec<(&'static str, u64, u64)>,
    /// 命名子探针（与 `rows` 同形；同窗口取走）。
    pub probes: Vec<(&'static str, u64, u64)>,
    /// **卡 `openmir2-simmonsterai-profile`**：本窗口的逐 tick 工作段 ns 样本（tick 级分位数用）。
    pub work_samples: Vec<u64>,
}

/// World 探针的**增量**快照（累计值之差）。字段与 `TickProfile` 同序同名。
#[derive(Debug, Clone, Default)]
pub struct WorldDelta {
    /// 本窗口内 `World::tick_head` 的入口次数（= World 探针记的 tick 数）。
    pub ticks: u64,
    /// `(TickStage 名, 增量 ns, 增量调用次数)`
    pub stages: Vec<(String, u64, u64)>,
    /// `(探针名, 增量 ns, 增量调用次数)`
    pub probes: Vec<(String, u64, u64)>,
    /// **卡 `openmir2-simmonsterai-profile`**：本窗口的逐 tick `SimMonsterAi` ns 样本
    /// （tick 级分位数；不是 50-tick 桶）。
    pub sim_ai_tick_ns: Vec<u64>,
    /// `SimMonsterAi` 内部探针的**本窗口累计**（`crate::monai` 子探针）。
    pub monai: mir2_world_bevy::monai::MonaiProbe,
    /// 本窗口内 `monai` 覆盖的 tick 数。
    pub monai_ticks: u64,
}

/// 跨窗口保存的 World 累计快照（用于算增量；World 探针本身不 reset）。
#[derive(Debug, Clone, Default)]
pub struct WorldSnapshot {
    ticks: u64,
    stages: Vec<(String, u64, u64)>,
    probes: Vec<(String, u64, u64)>,
    /// 已消费到的 `TickProfile::sim_ai_samples_total`（单调计数；环形截断时也正确）。
    sim_ai_total: u64,
    /// 已消费的 `monai` 累计（用于算窗口增量）。
    monai: mir2_world_bevy::monai::MonaiProbe,
    monai_ticks: u64,
}

/// 环境变量取值 → 门控：只有 `"1"` 为真（其余一律关，含 `"true"`/`"0"`/空）。
#[must_use]
pub fn enabled_from(value: Option<&str>) -> bool {
    value == Some("1")
}

/// 慢 tick 阈值（ms）→ ns：**先 `trim()` 再解析**（口径对齐 `clock.rs::parse_clock` /
/// `seed.rs::parse_seed` 的「前后空白合法」），只接受纯十进制正整数。
///
/// `None`/空串/空白串/`0`/负数/小数/非数字/ms→ns 溢出 ⇒ `None`（本函数只回答「是不是合法阈值」；
/// 「非法」与「显式关闭」的区分由 [`slow_gate_from_env_value`] 负责）。
#[must_use]
pub fn slow_threshold_ns_from(value: Option<&str>) -> Option<u64> {
    let text = value?.trim();
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let ms: u64 = text.parse().ok()?;
    if ms == 0 {
        return None;
    }
    ms.checked_mul(1_000_000)
}

/// 慢 tick 诊断子门的环境判定结果（纯函数，便于单测；日志由 [`HostTickProfile::from_env`] 打）。
///
/// **口径**（见 `docs/tick-profile-slow-gate-20261116.md`）：
/// - 诊断子门**不因可选开关 typo 杀进程**，但**绝不静默降级**：非法值 ⇒ 门关闭 + 一条
///   含「变量名 + 原文值」的 `WRN`（与 `rng_trace` 的 `[ERROR]+trace 关闭`同族）；
/// - 行为性注入（`MIR2_CLOCK_MS` / `MIR2_RNG_SEED`）非法 ⇒ `Err`/fail-closed 退出，因为
///   带着默认值跑会产出**看似有效**的错误结果；诊断子门默认关、不影响任何行为，故降一级。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlowGateEnv {
    /// 变数未设或空串 ⇒ 静默关（与 `parse_clock` 的 `None`/`""` 同口径）。
    Unset,
    /// 显式 `0` ⇒ 关，但打一条可审计的 `DBG`（不当作非法）。
    ExplicitOff,
    /// 合法正整数毫秒 ⇒ 开（值为 ns）。
    On(u64),
    /// 非法值（含原始文本）⇒ 关 + `WRN`。
    Invalid(String),
}

impl SlowGateEnv {
    /// 门是否开（只有 [`Self::On`] 给出阈值）。
    #[must_use]
    pub fn threshold_ns(&self) -> Option<u64> {
        match self {
            Self::On(ns) => Some(*ns),
            _ => None,
        }
    }

    /// 非法值 ⇒ 必须用 `WRN` 打出的文案（含变量名与**原文值**，机器可抓）。
    #[must_use]
    pub fn warn_message(&self) -> Option<String> {
        match self {
            Self::Invalid(raw) => Some(format!(
                "{ENV_TICK_PROFILE_SLOW_MS}={raw:?} 非法（期望正整数毫秒；空白会被 trim；0=显式关闭），慢 tick 快照已关闭"
            )),
            _ => None,
        }
    }

    /// 显式 `0` ⇒ 可审计的 `DBG` 文案（不当作非法，也不静默）。
    #[must_use]
    pub fn notice_message(&self) -> Option<String> {
        match self {
            Self::ExplicitOff => Some(format!(
                "{ENV_TICK_PROFILE_SLOW_MS}=0 显式关闭慢 tick 快照（不是非法值）"
            )),
            _ => None,
        }
    }
}

/// 按环境变量原文判定慢 tick 子门（纯函数）。
///
/// - `None` / `Some("")` ⇒ [`SlowGateEnv::Unset`]（与 `clock.rs` 的空串口径一致）；
/// - 只含空白的串 ⇒ [`SlowGateEnv::Invalid`]（`clock.rs` 对空白串也走 `parse` 失败）；
/// - `trim()` 后为 `0` ⇒ [`SlowGateEnv::ExplicitOff`]；
/// - `trim()` 后是合法正整数毫秒 ⇒ [`SlowGateEnv::On`]；
/// - 其余 ⇒ [`SlowGateEnv::Invalid`]（保留**未 trim 的原文**供日志抓取）。
#[must_use]
pub fn slow_gate_from_env_value(value: Option<&str>) -> SlowGateEnv {
    let Some(raw) = value else {
        return SlowGateEnv::Unset;
    };
    if raw.is_empty() {
        return SlowGateEnv::Unset;
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return SlowGateEnv::Invalid(raw.to_string());
    }
    if trimmed == "0" {
        return SlowGateEnv::ExplicitOff;
    }
    match slow_threshold_ns_from(Some(raw)) {
        Some(ns) => SlowGateEnv::On(ns),
        None => SlowGateEnv::Invalid(raw.to_string()),
    }
}

impl HostTickProfile {
    /// 读环境变量建计时器（**整个进程只读一次**；tick 路径上不再读环境）。
    ///
    /// 慢 tick 快照门（`MIR2_TICK_PROFILE_SLOW_MS`）**只有主门开启时才解析**——主门关闭时
    /// 快照无从谈起（没有段计时），阈值恒 `None`。
    ///
    /// 非法值 ⇒ **启动时一条 `WRN`（含变量名+原文）+ 门保持关闭，不杀进程**（诊断子门口径，
    /// 见 [`SlowGateEnv`] 与 `docs/tick-profile-slow-gate-20261116.md`）。
    #[must_use]
    pub fn from_env() -> Self {
        let value = std::env::var(ENV_TICK_PROFILE).ok();
        let enabled = enabled_from(value.as_deref());
        let slow = if enabled {
            let slow_value = std::env::var(ENV_TICK_PROFILE_SLOW_MS).ok();
            let gate = slow_gate_from_env_value(slow_value.as_deref());
            if let Some(msg) = gate.warn_message() {
                crate::glog!("WRN", "{msg}");
            }
            if let Some(msg) = gate.notice_message() {
                crate::glog!("DBG", "{msg}");
            }
            gate.threshold_ns()
        } else {
            None
        };
        // `hear_targets_at` 子探针只服务慢 tick 快照 ⇒ 与 SLOW 门同开同关（关闭态零开销）。
        set_hear_targets_probe(enabled && slow.is_some());
        Self {
            enabled,
            slow_threshold_ns: slow,
            ..Self::default()
        }
    }

    /// 慢 tick 快照门是否开启（阈值 ns）。
    #[must_use]
    #[inline]
    pub fn slow_threshold_ns(&self) -> Option<u64> {
        self.slow_threshold_ns
    }

    /// **测量 harness 专用**：同 [`Self::with_enabled`]，但带慢 tick 阈值（ns）。
    #[cfg(test)]
    #[must_use]
    pub fn with_slow_threshold_ns(threshold_ns: u64) -> Self {
        Self {
            enabled: true,
            slow_threshold_ns: Some(threshold_ns),
            ..Self::default()
        }
    }

    /// 本 tick 已记录的段数（测试用：验证关闭态不落行）。
    #[cfg(test)]
    #[must_use]
    pub fn tick_rows(&self) -> &[(&'static str, u64)] {
        &self.tick_rows
    }

    /// 本 tick 已记录的子探针数（测试用）。
    #[cfg(test)]
    #[must_use]
    pub fn tick_probe_rows(&self) -> &[(&'static str, u64)] {
        &self.tick_probe_rows
    }

    /// 门控是否开启。
    #[must_use]
    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// **测量 harness 专用**：不读环境变量直接建开启/关闭两臂（`docs/host-aoi-profile-20261114.md`
    /// 的同进程 A/B 用；生产路径只走 [`Self::from_env`]）。
    #[cfg(test)]
    #[must_use]
    pub fn with_enabled(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    /// 已记录的子探针 `(名, 累计 ns, 调用次数)`（只读；窗口未取走时）。测量 harness 用。
    #[cfg(test)]
    #[must_use]
    pub fn probes(&self) -> &[(&'static str, u64, u64)] {
        &self.probes
    }

    /// 段起点。关闭时：一次 `bool` 判别 ⇒ `None`（**不取 `Instant`**）。
    #[must_use]
    #[inline]
    pub fn begin(&self) -> Option<Instant> {
        if self.enabled {
            Some(Instant::now())
        } else {
            None
        }
    }

    /// 段终点。关闭时（`at` 为 `None`）：一次 `Option` 判别 ⇒ 直接返回（不写状态）。
    #[inline]
    pub fn end(&mut self, name: &'static str, at: Option<Instant>) {
        let Some(at) = at else { return };
        let ns = nanos_since(at);
        if name == "HostSleep" {
            self.tick_sleep_ns = ns;
        }
        self.add(name, ns);
        self.tick_sum_ns = self.tick_sum_ns.saturating_add(ns);
        // 慢 tick 快照：只有第二道门开启时才保留逐 tick 行（关闭态一次 `Option` 判别）。
        if self.slow_threshold_ns.is_some() {
            self.tick_rows.push((name, ns));
        }
    }

    /// 命名**子探针**终点。与 [`Self::end`] 同样的关闭态口径（`at == None` ⇒ 一次 `Option`
    /// 判别后直接返回），但**不写 `tick_sum_ns`** ⇒ 子探针与父段可以嵌套计时而不让
    /// `HostUnaccounted` 出现负残差。
    #[inline]
    pub fn probe(&mut self, name: &'static str, at: Option<Instant>) {
        let Some(at) = at else { return };
        self.record_probe_ns(name, nanos_since(at), 1);
    }

    /// 记一条命名子探针（`calls` 次调用合计 `ns`）。窗口累计与逐 tick 行同口径；
    /// 慢 tick 门关闭时不保留逐 tick 行（一次 `Option` 判别）。
    fn record_probe_ns(&mut self, name: &'static str, ns: u64, calls: u64) {
        match self.probes.iter_mut().find(|(n, _, _)| *n == name) {
            Some((_, total, c)) => {
                *total = total.saturating_add(ns);
                *c = c.saturating_add(calls);
            }
            None => self.probes.push((name, ns, calls)),
        }
        if self.slow_threshold_ns.is_some() {
            self.tick_probe_rows.push((name, ns));
        }
    }

    /// 整 tick 终点：记 `HostTickTotal` + 残差 `HostUnaccounted`，并推进窗口计数。
    ///
    /// **卡 `openmir2-simmonsterai-profile`**：同时把**本 tick 工作段**（`total − HostSleep`）
    /// 追加到 `work_samples`（tick 级分位数；不是 50-tick 桶近似）。超过 [`SAMPLE_CAP`] 丢最旧。
    ///
    /// **卡 `openmir2-host-segment-profiler-500`**：慢 tick 门开启且工作段 ≥ 阈值时，把本 tick
    /// 的段快照暂存到 `pending_slow`（由 [`Self::take_slow_tick`] 取走打印）；否则什么都不留
    /// 并清空逐 tick 缓冲。关闭态：一次 `Option` 判别 ⇒ 直接返回（不取时钟、不分配）。
    ///
    /// 返回值是 `()`：`end_tick` 的既有调用点（含测量 harness）**不需要改**；慢 tick 输出是
    /// 可选的旁路，不取走也不影响窗口计数。
    #[inline]
    pub fn end_tick(&mut self, at: Option<Instant>) {
        let Some(at) = at else { return };
        let total = nanos_since(at);
        self.add("HostTickTotal", total);
        let sum = self.tick_sum_ns;
        let unaccounted = total.saturating_sub(sum);
        if total > sum {
            self.add("HostUnaccounted", total - sum);
        }
        let sleep = self.tick_sleep_ns;
        let work = total.saturating_sub(sleep);
        if self.work_samples.len() >= SAMPLE_CAP {
            self.work_samples.remove(0);
        }
        self.work_samples.push(work);
        let tick = self.total_ticks + 1;
        self.tick_sleep_ns = 0;
        self.tick_sum_ns = 0;
        self.window_ticks += 1;
        self.window_ns = self.window_ns.saturating_add(total);
        self.total_ticks += 1;
        // **`hear_targets_at` 命名子项**：慢 tick 门开启时，把线程局部累加器收进本 tick 的探针行
        // （`chat::hear_targets_at` 在消息处理深处调用，不携带 `&mut HostTickProfile`）。
        // 未启用慢门 ⇒ 累加器为 `None`，一次 `Option` 判别后什么都不做（不取时钟、不分配）。
        if self.slow_threshold_ns.is_some() {
            if let Some((ns, calls)) = take_hear_targets_tick() {
                if calls > 0 {
                    self.record_probe_ns(HEAR_TARGETS_PROBE_NAME, ns, calls);
                }
            }
        }
        self.pending_slow = match self.slow_threshold_ns {
            Some(threshold) if work >= threshold => Some(SlowTick {
                tick,
                total_ns: total,
                sleep_ns: sleep,
                work_ns: work,
                unaccounted_ns: unaccounted,
                segments: std::mem::take(&mut self.tick_rows),
                probes: std::mem::take(&mut self.tick_probe_rows),
            }),
            _ => None,
        };
        // 未构成慢 tick（或没有慢 tick 门）⇒ 本 tick 行必须丢弃，不能跨 tick 累积。
        self.tick_rows.clear();
        self.tick_probe_rows.clear();
    }

    /// 取走最近一个慢 tick 的快照（每 tick 由 `sim_loop` 调用一次；没有 ⇒ `None`）。
    #[inline]
    pub fn take_slow_tick(&mut self) -> Option<SlowTick> {
        self.pending_slow.take()
    }

    /// **卡 `openmir2-simmonsterai-profile`**：逐 tick 工作段样本（ns，旧→新）。测量 harness 用。
    #[cfg(test)]
    #[must_use]
    pub fn work_samples(&self) -> &[u64] {
        &self.work_samples
    }

    /// 窗口是否满（`REPORT_TICKS` tick 或 `REPORT_WINDOW_NS` 墙钟，先到者）。
    ///
    /// 关闭时恒 `false`（**只读一次 `bool`**，不取时钟）。
    #[must_use]
    #[inline]
    pub fn due(&self) -> bool {
        self.enabled && (self.window_ticks >= REPORT_TICKS || self.window_ns >= REPORT_WINDOW_NS)
    }

    /// 取走当前窗口（并清零窗口计数）；窗口为空 ⇒ `None`。
    pub fn take_window(&mut self) -> Option<HostWindow> {
        if self.window_ticks == 0 {
            return None;
        }
        let window = HostWindow {
            ticks: self.window_ticks,
            total_ns: self.window_ns,
            rows: std::mem::take(&mut self.rows),
            probes: std::mem::take(&mut self.probes),
            work_samples: std::mem::take(&mut self.work_samples),
        };
        self.window_ticks = 0;
        self.window_ns = 0;
        Some(window)
    }

    fn add(&mut self, name: &'static str, ns: u64) {
        match self.rows.iter_mut().find(|(n, _, _)| *n == name) {
            Some((_, total, calls)) => {
                *total = total.saturating_add(ns);
                *calls += 1;
            }
            None => self.rows.push((name, ns, 1)),
        }
    }
}

fn nanos_since(at: Instant) -> u64 {
    u64::try_from(at.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// **最近秩分位数**（ns）：与 `tick_bench` 同口径 `s[ceil(p/100·len) − 1]`。
///
/// **逐 tick 样本**，不是 50-tick 桶近似（卡 `openmir2-simmonsterai-profile` 的 opc 要求）。
#[must_use]
pub fn percentile_ns(samples: &[u64], p: f64) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    let mut s = samples.to_vec();
    s.sort_unstable();
    let idx = ((p / 100.0) * s.len() as f64).ceil() as usize;
    s[idx.clamp(1, s.len()) - 1]
}

/// 一行分位数摘要：`n/p50/p95/p99/max/mean`（ns → ms）。
fn log_quantiles(label: &str, samples: &[u64]) {
    if samples.is_empty() {
        return;
    }
    let n = samples.len() as f64;
    let mean_ns = samples.iter().map(|v| *v as f64).sum::<f64>() / n;
    let max = *samples.iter().max().unwrap_or(&0);
    let ms = |v: u64| v as f64 / 1e6;
    crate::glog!(
        "DBG",
        "[TICKPROF]   {label} 逐 tick 分位数（n={:.0}）：p50 {:.3} / p95 {:.3} / p99 {:.3} / max {:.3} / mean {:.3} ms",
        n,
        ms(percentile_ns(samples, 50.0)),
        ms(percentile_ns(samples, 95.0)),
        ms(percentile_ns(samples, 99.0)),
        ms(max),
        mean_ns / 1e6
    );
}

/// 子探针起点（`Option<&mut HostTickProfile>` 版）：`None` 或无门控 ⇒ `None`（**不取时钟**）。
///
/// 生产 `sim_loop` 恒传 `Some(&mut host_prof)`；门控关闭时 `begin()` 的 `enabled == false`
/// 短路 ⇒ 与既有段探针同一关闭态口径（`bool` 判别 + `Option` 判别，无 `Instant`/分配/写状态）。
#[inline]
#[must_use]
pub fn probe_begin(prof: &Option<&mut HostTickProfile>) -> Option<Instant> {
    match prof {
        Some(p) => p.begin(),
        None => None,
    }
}

/// 子探针终点（见 [`probe_begin`]）：`None`/关闭态 ⇒ 一次判别后返回，不写任何状态。
#[inline]
pub fn probe_end(prof: &mut Option<&mut HostTickProfile>, name: &'static str, at: Option<Instant>) {
    if let Some(p) = prof.as_deref_mut() {
        p.probe(name, at);
    }
}

/// 把 World 探针的累计值转成本窗口增量（`last` 被更新为当前值；探针不 reset）。
#[must_use]
pub fn world_delta(
    profile: Option<&mir2_world_bevy::world::TickProfile>,
    last: &mut WorldSnapshot,
) -> Option<WorldDelta> {
    let p = profile?;
    let stages = diff_rows(
        p.stages()
            .iter()
            .map(|(s, n, c)| (format!("{s:?}"), *n, *c)),
        &mut last.stages,
    );
    let probes = diff_rows(
        p.probes()
            .iter()
            .map(|(s, n, c)| ((*s).to_string(), *n, *c)),
        &mut last.probes,
    );
    let ticks = p.ticks.saturating_sub(last.ticks);
    last.ticks = p.ticks;
    // **卡 `openmir2-simmonsterai-profile`**：逐 tick `SimMonsterAi` 样本增量 + 子探针累计增量。
    let new_samples = p.sim_ai_samples_total.saturating_sub(last.sim_ai_total);
    last.sim_ai_total = p.sim_ai_samples_total;
    // 环形缓冲可能把本轮新增的样本裁掉一部分 ⇒ 至多取现有条数。
    let take = usize::try_from(new_samples)
        .unwrap_or(usize::MAX)
        .min(p.sim_ai_tick_ns.len());
    let sim_ai_tick_ns = p.sim_ai_tick_ns[p.sim_ai_tick_ns.len() - take..].to_vec();
    let monai = p.monai.delta_since(&last.monai);
    last.monai = p.monai.clone();
    let monai_ticks = p.monai_ticks.saturating_sub(last.monai_ticks);
    last.monai_ticks = p.monai_ticks;
    Some(WorldDelta {
        ticks,
        stages,
        probes,
        sim_ai_tick_ns,
        monai,
        monai_ticks,
    })
}

fn diff_rows(
    current: impl Iterator<Item = (String, u64, u64)>,
    last: &mut Vec<(String, u64, u64)>,
) -> Vec<(String, u64, u64)> {
    let mut out = Vec::new();
    let mut next = Vec::new();
    for (name, ns, calls) in current {
        // 首窗（`last` 里还没有这个名字）必须是**全量**，否则第一次汇总会全 0。
        let (dns, dcalls) = match last.iter().find(|(n, _, _)| *n == name) {
            Some((_, n0, c0)) => (ns.saturating_sub(*n0), calls.saturating_sub(*c0)),
            None => (ns, calls),
        };
        out.push((name.clone(), dns, dcalls));
        next.push((name, ns, calls));
    }
    *last = next;
    out
}

/// 打印一个窗口的**完整 tick 分解表**（宿主段 + World 内部分段）。
///
/// 一行一个段：`ms/tick`、占整 tick 的百分比、调用次数。所有行都是 `glog!("DBG", …)`，
/// 只在门控开启时被调用。
pub fn log_window(host: &HostWindow, world: Option<&WorldDelta>, entities: usize) {
    let ticks = host.ticks.max(1) as f64;
    let total_ms = host.total_ns as f64 / 1e6 / ticks;
    crate::glog!(
        "DBG",
        "[TICKPROF] 窗口 {} tick：整 tick 均值 {:.2} ms/tick（墙钟合计 {:.2}s，实体内 {}，门控 MIR2_TICK_PROFILE=1）",
        host.ticks,
        total_ms,
        host.total_ns as f64 / 1e9,
        entities
    );
    let mut rows = host.rows.clone();
    rows.sort_by_key(|(_, ns, _)| std::cmp::Reverse(*ns));
    let sleep_ns = rows
        .iter()
        .find(|(n, _, _)| *n == "HostSleep")
        .map_or(0, |(_, ns, _)| *ns);
    let total_ns = host.total_ns.max(1) as f64;
    for (name, ns, calls) in &rows {
        let ms = *ns as f64 / 1e6 / ticks;
        let pct = *ns as f64 / total_ns * 100.0;
        crate::glog!(
            "DBG",
            "[TICKPROF]   {:<22} {:>9.3} ms/tick {:>7.2}% tick  ({} 次)",
            name,
            ms,
            pct,
            calls / host.ticks.max(1)
        );
    }
    let work_ns = host.total_ns.saturating_sub(sleep_ns);
    // **子探针**（本批：HostAoiDownlink 的细分）。它们是**父段内部**的嵌套计时，
    // 不进 `tick_sum_ns` ⇒ 不影响上面的残差对账；占比按整 tick 算，可与父段直接相减。
    if !host.probes.is_empty() {
        let mut probes = host.probes.clone();
        probes.sort_by_key(|(_, ns, _)| std::cmp::Reverse(*ns));
        crate::glog!(
            "DBG",
            "[TICKPROF] 宿主命名子探针（默认关闭；MIR2_TICK_PROFILE=1 时每窗口输出）："
        );
        for (name, ns, calls) in &probes {
            crate::glog!(
                "DBG",
                "[TICKPROF]   {:<26} {:>9.3} ms/tick {:>7.2}% tick  ({} 次/tick)",
                name,
                *ns as f64 / 1e6 / ticks,
                *ns as f64 / total_ns * 100.0,
                calls / host.ticks.max(1)
            );
        }
    }
    crate::glog!(
        "DBG",
        "[TICKPROF] 工作段合计（= HostTickTotal - HostSleep）= {:.2} ms/tick，占比 {:.2}%",
        work_ns as f64 / 1e6 / ticks,
        work_ns as f64 / total_ns * 100.0
    );
    // **逐 tick 分位数**（不是 50-tick 桶近似；卡 `openmir2-simmonsterai-profile`）。
    log_quantiles("HostTickTotal 工作段", &host.work_samples);
    let Some(world) = world else {
        crate::glog!(
            "DBG",
            "[TICKPROF] World 探针未开启（world.tick_profile() = None）"
        );
        return;
    };
    let w_ticks = world.ticks.max(1) as f64;
    let w_sum: u64 = world.stages.iter().map(|(_, ns, _)| *ns).sum();
    crate::glog!(
        "DBG",
        "[TICKPROF] World::tick 内部（World 探针 {} tick，段合计 {:.3} ms/tick）：",
        world.ticks,
        w_sum as f64 / 1e6 / w_ticks
    );
    let mut stages = world.stages.clone();
    stages.sort_by_key(|(_, ns, _)| std::cmp::Reverse(*ns));
    for (name, ns, calls) in &stages {
        crate::glog!(
            "DBG",
            "[TICKPROF]   World.{:<22} {:>9.3} ms/tick {:>7.2}% World  {:>7.2}% tick  ({} 次/tick)",
            name,
            *ns as f64 / 1e6 / w_ticks,
            *ns as f64 / w_sum.max(1) as f64 * 100.0,
            *ns as f64 / total_ns * 100.0,
            calls / world.ticks.max(1)
        );
    }
    // **卡 `openmir2-simmonsterai-profile`**：`SimMonsterAi` 的逐 tick 分位数 + 内部子项。
    // 逐 tick 样本来自 `TickProfile::sim_ai_tick_ns`（每个 tick 一个值，**不是** 50-tick 桶）。
    log_quantiles("World.SimMonsterAi", &world.sim_ai_tick_ns);
    if world.monai_ticks > 0 {
        let mt = world.monai_ticks as f64;
        let m = &world.monai;
        let loop_ns = m.loop_ns as f64;
        crate::glog!(
            "DBG",
            "[TICKPROF]   SimMonsterAi 结构分解（{} tick 累计；collect=遍历, gate=门判定模型, loop=门+AI 体）：",
            world.monai_ticks
        );
        crate::glog!(
            "DBG",
            "[TICKPROF]     collect {:.3} ms/tick；gate {:.3} ms/tick；loop {:.3} ms/tick ⇒ AI 体（loop−gate）{:.3} ms/tick",
            m.collect_ns as f64 / 1e6 / mt,
            m.gate_ns as f64 / 1e6 / mt,
            m.loop_ns as f64 / 1e6 / mt,
            (loop_ns - m.gate_ns as f64).max(0.0) / 1e6 / mt
        );
        let per = |v: u64| v as f64 / mt;
        crate::glog!(
            "DBG",
            "[TICKPROF]     counts/tick：slot {:.0}，scanned {:.0}（inactive {:.0}），throttle-skip {:.0}，ran {:.0}（dead {:.0} / frozen {:.0} / no-target {:.0} / idle-wander {:.0} / walk-due {:.0} / target {:.0}）",
            per(m.slot_total),
            per(m.scanned),
            per(m.scanned_inactive),
            per(m.skipped_throttle),
            per(m.ran),
            per(m.ran_dead),
            per(m.ran_frozen),
            per(m.ran_no_target),
            per(m.ran_idle_wander),
            per(m.ran_walk_due),
            per(m.ran_target)
        );
    }
    // 对账：宿主量到的 World 两段 vs World 自己量到的段合计（差 = 专用怪产仔/ref 排水等宿主附加）。
    let host_world_ns: u64 = host
        .rows
        .iter()
        .filter(|(n, _, _)| *n == "HostWorldHead" || *n == "HostWorldTail")
        .map(|(_, ns, _)| *ns)
        .sum();
    if host_world_ns != 0 {
        crate::glog!(
            "DBG",
            "[TICKPROF] 对账：HostWorldHead+HostWorldTail = {:.3} ms/tick；ΣWorld.* = {:.3} ms/tick；宿主附加（专用怪产仔/ref 等）= {:.3} ms/tick",
            host_world_ns as f64 / 1e6 / ticks,
            w_sum as f64 / 1e6 / ticks,
            host_world_ns as f64 / 1e6 / ticks - w_sum as f64 / 1e6 / ticks
        );
    }
    if !world.probes.is_empty() {
        let mut probes = world.probes.clone();
        probes.sort_by_key(|(_, ns, _)| std::cmp::Reverse(*ns));
        crate::glog!("DBG", "[TICKPROF] World 命名探针（ms/tick）：");
        for (name, ns, calls) in &probes {
            crate::glog!(
                "DBG",
                "[TICKPROF]   Probe.{} = {:.4} ms/tick ({} 次/tick)",
                name,
                *ns as f64 / 1e6 / w_ticks,
                calls / world.ticks.max(1)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// **A1 牙（关闭即零工作）**：缺省 `enabled == false`；`begin` 不取时钟（恒 `None`）；
    /// `end`/`end_tick` 收到 `None` 不得写任何状态；`due`/`take_window` 恒空。
    ///
    /// 变异：把 `end` 的 `let Some(at) = at else { return }` 删掉（对 `None` 也累计）⇒
    /// `rows` 非空 ⇒ 红；把 `enabled` 缺省改 `true` ⇒ `begin().is_some()` ⇒ 红。
    #[test]
    fn disabled_profile_does_no_work() {
        let mut p = HostTickProfile::default();
        assert!(!p.enabled(), "缺省必须关闭");
        assert!(p.begin().is_none(), "关闭时 begin 不得取时钟");
        for _ in 0..100 {
            p.end("HostInbound", None);
            p.end_tick(None);
            assert!(p.tick_rows().is_empty(), "关闭态不得保留逐 tick 行");
            assert!(p.take_slow_tick().is_none(), "关闭态不得产生慢 tick 快照");
        }
        assert!(p.rows.is_empty(), "关闭时不得累计任何段");
        assert_eq!(p.window_ticks, 0);
        assert_eq!(p.total_ticks, 0);
        assert!(!p.due());
        assert!(p.take_window().is_none());
    }

    /// **子探针关闭态牙**：`probe(None)` 与 `probe_begin(None)`/`probe_begin(&None)` 都不得
    /// 写状态、不得取时钟；关闭态的 `probe_end` 也不得落行。
    ///
    /// 变异：把 `probe` 的 `let Some(at) = at else { return }` 删掉 ⇒ `probes` 非空 ⇒ 红；
    /// 把 `probe` 改成恒 `Instant::now()` ⇒ `probe_begin(&None).is_some()` ⇒ 红。
    #[test]
    fn disabled_probe_does_no_work() {
        let mut p = HostTickProfile::default();
        for _ in 0..100 {
            p.probe("HostAoiAppearSelect", None);
        }
        assert!(p.probes.is_empty(), "关闭态不得记录任何子探针");
        assert_eq!(p.tick_sum_ns, 0, "子探针不得写 tick_sum_ns");
        let none: Option<&mut HostTickProfile> = None;
        assert!(probe_begin(&none).is_none(), "None 句柄不得取时钟");
        let mut off = HostTickProfile::default();
        let mut off_opt: Option<&mut HostTickProfile> = Some(&mut off);
        assert!(probe_begin(&off_opt).is_none(), "关闭态 begin 不得取时钟");
        probe_end(&mut off_opt, "X", None);
        let off_ref = off_opt.expect("Some");
        assert!(off_ref.probes.is_empty());
    }

    /// **子探针不吃残差**：开启态记录 `probe` 后 `tick_sum_ns` 不变（父段与子探针可嵌套）。
    /// 窗口取走后 `probes` 清零。
    #[test]
    fn probe_does_not_change_residual_and_window_takes_probes() {
        let mut p = HostTickProfile {
            enabled: true,
            ..HostTickProfile::default()
        };
        p.tick_sum_ns = 123;
        let at = p.begin();
        assert!(at.is_some());
        p.probe("HostAoiAppearBuild", at);
        assert_eq!(p.tick_sum_ns, 123, "probe 不得改 tick_sum_ns");
        assert_eq!(p.probes.len(), 1);
        assert!(p.probes[0].1 > 0, "累计 ns 必须 > 0");
        // 手工推进窗口（不依赖真实 tick）。
        p.window_ticks = 1;
        p.window_ns = 1;
        let w = p.take_window().expect("窗口非空");
        assert_eq!(w.probes.len(), 1);
        assert!(p.probes.is_empty(), "取走窗口后子探针必须清零");
    }

    /// **`hear_targets_at` 子探针关闭态零开销**：累加器未置位（主门关 / 慢门未设）时，
    /// `hear_targets_probe_begin` 恒 `None`，且**探针时钟调用计数必须为 0**（不只是「不落行」）；
    /// `probe_end(None)` 不写状态；即使 Slow 门开着，`end_tick` 也拿不到任何 `hear_targets_at` 行。
    ///
    /// 变异：把 `hear_targets_probe_begin` 改成恒 `Some(probe_clock_now())` ⇒ 时钟计数断言红。
    #[test]
    fn hear_targets_probe_off_does_no_work() {
        set_hear_targets_probe_for_test(false);
        let _ = probe_clock_calls_for_test(); // 清掉同线程可能残留的计数
        for _ in 0..100 {
            assert!(
                hear_targets_probe_begin().is_none(),
                "未启用时不得取时钟（关闭态零开销）"
            );
            hear_targets_probe_end(None);
        }
        assert_eq!(
            probe_clock_calls_for_test(),
            0,
            "关闭态 begin 100 次后探针时钟调用次数必须仍为 0（证明没调 Instant::now）"
        );
        assert!(take_hear_targets_tick().is_none(), "未启用时不得有累计");
        // 主门开启、慢门关闭（既有 `with_enabled(true)` 形态）也不得采集、不得取时钟：
        let mut p = HostTickProfile::with_enabled(true);
        let slot = p.begin().expect("开启态");
        for _ in 0..10 {
            let at = hear_targets_probe_begin();
            assert!(at.is_none(), "慢门关闭时不得取时钟");
            hear_targets_probe_end(at);
        }
        assert_eq!(
            probe_clock_calls_for_test(),
            0,
            "慢门关闭时也不得调用探针时钟"
        );
        p.end_tick(Some(slot));
        assert!(
            !p.probes()
                .iter()
                .any(|(n, _, _)| *n == HEAR_TARGETS_PROBE_NAME),
            "慢门关闭时不得出现 hear_targets_at 行"
        );
        // 负对照：置位后必须真的计到 1，否则上面的 0 也可能是「计数器没接线」。
        set_hear_targets_probe_for_test(true);
        let at = hear_targets_probe_begin();
        assert!(at.is_some());
        assert_eq!(
            probe_clock_calls_for_test(),
            1,
            "置位后 begin 必须调用探针时钟（证明计数有接线）"
        );
        hear_targets_probe_end(at);
        set_hear_targets_probe_for_test(false);
        let _ = probe_clock_calls_for_test();
    }

    /// **跨 tick 不累积**：tick1 调用 10 次、tick2 一次都不调用 ⇒ tick2 的慢快照里
    /// **不得出现** `hear_targets_at` 行（更不能把 tick1 的 10 次/耗时再算一遍）。
    ///
    /// 变异：删掉 `take_hear_targets_tick` 里的 `c.set(Some((0, 0)))`（end_tick 取走后不清零）
    /// ⇒ tick2 再次取到 tick1 的累计并落行 ⇒ 本用例红。
    #[test]
    fn hear_targets_probe_does_not_accumulate_across_ticks() {
        set_hear_targets_probe_for_test(true);
        // tick 1：10 次调用。阈值 1ns ⇒ 两个 tick 都判慢。
        let mut p = HostTickProfile::with_slow_threshold_ns(1);
        let slot = p.begin().expect("开启态");
        for _ in 0..10 {
            let at = hear_targets_probe_begin().expect("已置位");
            std::thread::sleep(Duration::from_micros(50));
            hear_targets_probe_end(Some(at));
        }
        p.end_tick(Some(slot));
        let t1 = p.take_slow_tick().expect("tick1 必须是慢 tick");
        assert!(
            t1.probes.iter().any(|(n, _)| *n == HEAR_TARGETS_PROBE_NAME),
            "tick1 有 10 次调用 ⇒ 必须有该项"
        );
        // tick 2：零调用。显式睡 1ms 让本 tick 的**工作段**可测（否则 0ns < 1ns 阈值 ⇒ 无快照）。
        let slot = p.begin().expect("开启态");
        std::thread::sleep(Duration::from_millis(1));
        p.end_tick(Some(slot));
        let t2 = p.take_slow_tick().expect("tick2 必须是慢 tick");
        assert!(
            !t2.probes.iter().any(|(n, _)| *n == HEAR_TARGETS_PROBE_NAME),
            "tick2 零调用 ⇒ 不得出现该项（清零缺失时这里会是 tick1 的 10 次累计）"
        );
        let win = p
            .probes()
            .iter()
            .find(|(n, _, _)| *n == HEAR_TARGETS_PROBE_NAME)
            .expect("窗口累计必须含 tick1 的调用");
        assert_eq!(
            win.2, 10,
            "窗口累计只能是 tick1 的 10 次（tick2 不得重复计入）"
        );
        set_hear_targets_probe_for_test(false);
    }

    /// **`hear_targets_at` 子探针开启态**：累加 `(ns, calls)`，`end_tick` 一次性收进慢 tick
    /// 快照的 probes 与窗口 probes；取走后本 tick 累加器清零（不跨 tick 累积）。
    ///
    /// 变异：删掉 `end_tick` 里的 `take_hear_targets_tick()` 接线 ⇒ `slow.probes` 为空 ⇒ 红；
    /// 把 `record_probe_ns` 的 `calls.saturating_add(calls)` 改成 `+= 1` ⇒ 调用次数断言红。
    #[test]
    fn hear_targets_probe_accumulates_into_slow_snapshot() {
        set_hear_targets_probe_for_test(true);
        let mut p = HostTickProfile::with_slow_threshold_ns(1); // 1ns ⇒ 必判慢 tick
        let slot = p.begin().expect("开启态");
        for _ in 0..3 {
            let at = hear_targets_probe_begin().expect("已置位");
            std::thread::sleep(Duration::from_millis(1));
            hear_targets_probe_end(Some(at));
        }
        p.end_tick(Some(slot));
        set_hear_targets_probe_for_test(false);
        let slow = p.take_slow_tick().expect("必须产生慢 tick 快照");
        let row = slow
            .probes
            .iter()
            .find(|(n, _)| *n == HEAR_TARGETS_PROBE_NAME)
            .expect("hear_targets_at 必须进慢 tick 快照");
        assert!(row.1 >= 3_000_000, "3 次各睡 1ms，累计必须 ≥ 3ms");
        let win = p
            .probes()
            .iter()
            .find(|(n, _, _)| *n == HEAR_TARGETS_PROBE_NAME)
            .expect("窗口累计必须含 hear_targets_at");
        assert_eq!(win.2, 3, "调用次数必须按真实调用累计");
        assert!(win.1 >= 3_000_000);
        // 取走后清零（同一 tick 不重复计入下一个快照）。
        assert!(
            take_hear_targets_tick().is_none(),
            "测试已清位 ⇒ 不得残留累计"
        );
    }

    /// **环境门**：只有字面 `"1"` 开启；`"true"`/`"0"`/空/缺省一律关（fail-closed）。
    #[test]
    fn env_gate_accepts_only_literal_one() {
        assert!(enabled_from(Some("1")));
        for v in [Some("0"), Some("true"), Some(""), Some(" 1"), None] {
            assert!(!enabled_from(v), "{v:?} 不得开启门控");
        }
    }

    /// **慢 tick 阈值解析**：先 `trim()`（对齐 `clock.rs`/`seed.rs` 的前后空白合法），再只接受
    /// 纯十进制正整数（ms → ns）；`0`/空/空白串/符号/小数/非数字/溢出一律 `None`。
    ///
    /// 变异：把 `if ms == 0` 删掉 ⇒ `Some("0")` 得到 `Some(0)` ⇒ 每秒 tick 都打印 ⇒ 红；
    /// 把 `checked_mul` 改成 `*` ⇒ 溢出 panic/回绕 ⇒ 红；
    /// 删掉 `trim()` ⇒ `" 100"` 从 `Some` 变 `None` ⇒ 红。
    #[test]
    fn slow_threshold_parse_trims_then_fail_closed() {
        assert_eq!(slow_threshold_ns_from(Some("1")), Some(1_000_000));
        assert_eq!(slow_threshold_ns_from(Some("100")), Some(100_000_000));
        assert_eq!(
            slow_threshold_ns_from(Some(" 100 ")),
            Some(100_000_000),
            "前后空白必须被 trim（与 clock/seed 同口径）"
        );
        for v in [
            Some("0"),
            Some(""),
            Some("   "),
            Some("-1"),
            Some("+1"),
            Some("1.5"),
            Some("abc"),
            None,
        ] {
            assert_eq!(slow_threshold_ns_from(v), None, "{v:?} 必须关闭慢 tick 门");
        }
        assert_eq!(
            slow_threshold_ns_from(Some("18446744073709551615")),
            None,
            "ms→ns 溢出必须关（checked_mul）"
        );
    }

    /// **非法值不静默降级**（复审 P2）：非法原文 ⇒ 门关 + 必须给出含「变量名 + 原文值」的 WARN；
    /// 未设/空串 ⇒ 静默关（与 `clock.rs` 的 `None`/`""` 同口径）。
    ///
    /// 变异：把 `slow_gate_from_env_value` 末尾的 `None => SlowGateEnv::Invalid(raw.to_string())`
    /// 改成 `None => SlowGateEnv::Unset`（删掉 WARN 分支）⇒ 本用例红。
    #[test]
    fn slow_gate_invalid_value_warns_and_stays_closed() {
        for raw in ["10O", "1.5", "-1", "+1", "100ms", "   ", "０"] {
            let gate = slow_gate_from_env_value(Some(raw));
            assert_eq!(
                gate.threshold_ns(),
                None,
                "{raw:?} 非法值必须关闭慢 tick 快照（不杀进程、不开门）"
            );
            let warn = gate.warn_message().expect("非法值必须给出 WARN 文案");
            assert!(
                warn.contains(ENV_TICK_PROFILE_SLOW_MS),
                "WARN 必须含变量名：{warn}"
            );
            assert!(warn.contains(raw), "WARN 必须含原文值：{warn}");
            assert!(
                gate.notice_message().is_none(),
                "非法值不得混入 0 的 DBG 文案"
            );
        }
        // 未设 / 空串：与 clock/seed 同口径 —— 静默关（不可审计，但也不算非法）。
        for v in [None, Some("")] {
            let gate = slow_gate_from_env_value(v);
            assert_eq!(gate, SlowGateEnv::Unset, "{v:?} 必须是 Unset");
            assert!(gate.warn_message().is_none());
            assert!(gate.notice_message().is_none());
        }
    }

    /// **显式 `0` = 关闭但不是非法**：门关、无 WARN、有一条含变量名的可审计 DBG 文案。
    ///
    /// 变异：把 `if trimmed == "0"` 分支删掉 ⇒ 落到 `_ => Invalid` ⇒ WARN 非空 ⇒ 红。
    #[test]
    fn slow_gate_zero_is_explicit_off_with_audit_line() {
        let gate = slow_gate_from_env_value(Some("0"));
        assert_eq!(gate.threshold_ns(), None, "0 必须关门");
        assert!(gate.warn_message().is_none(), "0 不是非法值，不得 WARN");
        let notice = gate.notice_message().expect("0 必须有可审计文案");
        assert!(
            notice.contains(ENV_TICK_PROFILE_SLOW_MS),
            "审计文案必须含变量名：{notice}"
        );
        assert_eq!(
            slow_gate_from_env_value(Some(" 0 ")),
            SlowGateEnv::ExplicitOff,
            "trim 后 0 也算显式关闭"
        );
    }

    /// **空白与 clock/seed 对齐**：`" 100 "` ⇒ `On(100ms)`；仅空白 ⇒ `Invalid`（`clock.rs` 对
    /// 空白串同样走 parse 失败，只有**空串**才当未设）。
    ///
    /// 变异：删掉 `slow_threshold_ns_from` 的 `trim()` ⇒ `" 100 "` 变 `Invalid` ⇒ 红。
    #[test]
    fn slow_gate_trims_like_clock_and_seed() {
        let gate = slow_gate_from_env_value(Some(" 100 "));
        assert_eq!(gate, SlowGateEnv::On(100_000_000));
        assert_eq!(gate.threshold_ns(), Some(100_000_000));
        assert!(gate.warn_message().is_none());
        assert!(matches!(
            slow_gate_from_env_value(Some(" \t ")),
            SlowGateEnv::Invalid(_)
        ));
    }

    /// **启动路径（`from_env`）实际打 WARN**：非法值 ⇒ 门关；`--nocapture` 下可见实际输出行
    /// `… [WRN] MIR2_TICK_PROFILE_SLOW_MS="10O" 非法（…），慢 tick 快照已关闭`。
    ///
    /// 本用例只断言「门关」（日志内容由 `slow_gate_invalid_value_warns_and_stays_closed` 钉住，
    /// 删 WARN 分支会让那条用例红）；这里用环境变量走真实入口，作为人工/脚本证据。
    #[test]
    fn from_env_warns_on_invalid_slow_value() {
        std::env::set_var(ENV_TICK_PROFILE, "1");
        std::env::set_var(ENV_TICK_PROFILE_SLOW_MS, "10O");
        let p = HostTickProfile::from_env();
        // 先恢复环境再断言，避免用例失败时把 env 留给同进程的其它用例。
        std::env::remove_var(ENV_TICK_PROFILE_SLOW_MS);
        std::env::remove_var(ENV_TICK_PROFILE);
        let enabled = p.enabled();
        let slow = p.slow_threshold_ns();
        assert!(enabled, "主门=1 必须开启");
        assert!(slow.is_none(), "非法 SLOW 值必须关门（不杀进程）");
    }

    /// **慢 tick 关闭态零开销牙**：主门开启但 `MIR2_TICK_PROFILE_SLOW_MS` 未设
    /// （`slow_threshold_ns == None`）时，`end`/`probe` 不得保留逐 tick 行，
    /// `end_tick` 不得产生快照。
    ///
    /// 变异：删掉 `end` 里的 `if self.slow_threshold_ns.is_some()` 守卫（无条件 push）⇒
    /// 第一个 tick 内 `tick_rows` 非空 ⇒ 红；删掉 `probe` 的同名守卫 ⇒ `tick_probe_rows` 非空 ⇒ 红；
    /// 把 `end_tick` 的 `match` 改成恒 `Some(...)` ⇒ `take_slow_tick` 非 None ⇒ 红。
    #[test]
    fn slow_gate_off_records_no_per_tick_rows() {
        let mut p = HostTickProfile::with_enabled(true);
        for _ in 0..10 {
            let slot = p.begin().expect("开启态");
            let t = p.begin().expect("开启态");
            p.end("HostInbound", Some(t));
            assert!(
                p.tick_rows().is_empty(),
                "慢 tick 门关闭时不得保留逐 tick 段行"
            );
            let t = p.begin().expect("开启态");
            p.probe("HostAoiAppearSelect", Some(t));
            assert!(
                p.tick_probe_rows().is_empty(),
                "慢 tick 门关闭时不得保留逐 tick 子探针行"
            );
            p.end_tick(Some(slot));
            assert!(p.take_slow_tick().is_none(), "慢 tick 门关闭时不得产生快照");
        }
    }

    /// **慢 tick 快照内容**：只有工作段 ≥ 阈值的 tick 产生快照；段与子探针都带进快照，
    /// 且**未达阈值的 tick 行必须清空**（不跨 tick 累积）。
    ///
    /// 变异：把 `_ => None` 分支换成恒 `Some` ⇒ 第一个快 tick 也出快照 ⇒ 红；
    /// 把 `tick_rows.clear()` 删掉 ⇒ 第二个 tick 的快照里混入上一个 tick 的段 ⇒ 红。
    #[test]
    fn slow_gate_snapshot_only_over_threshold() {
        // 阈值 1s：快 tick 远不到 ⇒ 无快照，且逐 tick 行被清空。
        let mut p = HostTickProfile::with_slow_threshold_ns(1_000_000_000);
        let slot = p.begin().expect("开启态");
        let t = p.begin().expect("开启态");
        p.end("HostInbound", Some(t));
        let t = p.begin().expect("开启态");
        p.probe("HostAoiAppearSelect", Some(t));
        p.end_tick(Some(slot));
        assert!(p.take_slow_tick().is_none(), "未达阈值不得产生慢 tick 快照");
        assert!(
            p.tick_rows().is_empty(),
            "未产生快照的 tick 行必须清空（不跨 tick 累积）"
        );
        assert!(p.tick_probe_rows().is_empty());

        // 阈值 1ms：睡 3ms 的段 ⇒ 工作段 ≥ 1ms ⇒ 有快照。
        let mut p = HostTickProfile::with_slow_threshold_ns(1_000_000);
        let slot = p.begin().expect("开启态");
        let t = p.begin().expect("开启态");
        std::thread::sleep(Duration::from_millis(3));
        p.end("HostInbound", Some(t));
        let t = p.begin().expect("开启态");
        p.probe("HostAoiAppearSelect", Some(t));
        p.end_tick(Some(slot));
        let slow = p.take_slow_tick().expect("慢 tick 必须产生快照");
        assert_eq!(slow.tick, 1, "1-based tick 序号");
        assert!(slow.work_ns >= 1_000_000, "工作段必须 ≥ 阈值");
        assert!(slow.total_ns >= slow.work_ns, "total 必须 ≥ work");
        assert!(
            slow.segments
                .iter()
                .any(|(n, ns)| *n == "HostInbound" && *ns >= 1_000_000),
            "计时段必须进快照（且带真实 ns）"
        );
        assert!(
            slow.probes.iter().any(|(n, _)| *n == "HostAoiAppearSelect"),
            "命名子探针必须进快照"
        );
        assert_eq!(p.tick_rows().len(), 0, "快照取走后缓冲必须清空");
        assert_eq!(p.tick_probe_rows().len(), 0);
        assert!(
            p.take_slow_tick().is_none(),
            "快照只能取走一次（不重复打印）"
        );
    }

    /// **开启态累计 + 窗口**：段累计与调用次数、整 tick 残差、窗口取走与清零。
    #[test]
    fn enabled_profile_accumulates_and_takes_window() {
        let mut p = HostTickProfile {
            enabled: true,
            ..HostTickProfile::default()
        };
        for i in 0..REPORT_TICKS {
            let tick = p.begin();
            assert!(tick.is_some(), "开启态 begin 必须有时钟");
            let seg = p.begin();
            std::thread::sleep(Duration::from_micros(50));
            p.end("HostInbound", seg);
            if i == 0 {
                p.end_tick(tick);
                assert!(p.take_slow_tick().is_none(), "慢 tick 门缺省关闭");
                assert_eq!(p.window_ticks, 1);
                assert!(!p.due(), "1 tick 不得触发窗口");
                // 首轮：HostInbound 之外的耗时记进残差。
                assert!(
                    p.rows.iter().any(|(n, _, _)| *n == "HostUnaccounted"),
                    "整 tick 必须有残差行（HostTickTotal - 各段）"
                );
                continue;
            }
            p.end_tick(tick);
            assert!(p.take_slow_tick().is_none(), "慢 tick 门缺省关闭");
        }
        assert!(p.due(), "满 {REPORT_TICKS} tick 必须到期");
        let w = p.take_window().expect("窗口必须非空");
        assert_eq!(w.ticks, REPORT_TICKS);
        assert!(w.total_ns > 0);
        let inbound = w
            .rows
            .iter()
            .find(|(n, _, _)| *n == "HostInbound")
            .expect("HostInbound 必须被记录");
        assert_eq!(inbound.2, REPORT_TICKS, "段调用次数 = 每 tick 一次");
        assert!(inbound.1 > 0, "累计 ns 必须 > 0");
        assert!(p.rows.is_empty(), "取走窗口后本窗口段表必须清零");
        assert!(!p.due(), "取走后窗口重新计数");
        assert_eq!(p.total_ticks, REPORT_TICKS);
    }

    /// **墙钟臂**：tick 数远未到 `REPORT_TICKS`、但窗口墙钟已满 10s ⇒ 也必须到期
    /// （演示栈端到端 ≈ 2000ms/tick 时，50 tick 窗口要 100s，60s 观测拿不到一行）。
    #[test]
    fn due_has_a_wall_clock_arm() {
        let mut p = HostTickProfile {
            enabled: true,
            ..HostTickProfile::default()
        };
        p.window_ticks = 1;
        p.window_ns = REPORT_WINDOW_NS - 1;
        assert!(!p.due(), "1 tick + 墙钟未满不得到期");
        p.window_ns = REPORT_WINDOW_NS;
        assert!(p.due(), "墙钟满 10s 必须到期（即使只有 1 tick）");
        // 关闭态：即便窗口计数也满，`due` 仍恒假（不打印、不取时钟）。
        p.enabled = false;
        assert!(!p.due());
    }

    /// **World 增量**：同一份累计跑两次窗口，第二次应全 0（不重复计入）。
    #[test]
    fn world_delta_is_incremental() {
        // 合成累计：不依赖真 World（首窗全量、次窗只看增量；两档都断言）。
        let mut last: Vec<(String, u64, u64)> = Vec::new();
        let rows = diff_rows([("SimDup".to_string(), 500, 50)].into_iter(), &mut last);
        assert_eq!(rows[0].1, 500, "首窗 = 全量");
        assert_eq!(rows[0].2, 50);
        let rows2 = diff_rows([("SimDup".to_string(), 800, 80)].into_iter(), &mut last);
        assert_eq!(rows2[0].1, 300, "次窗只看增量");
        assert_eq!(rows2[0].2, 30);
        let rows3 = diff_rows([("SimDup".to_string(), 0, 0)].into_iter(), &mut last);
        assert_eq!(rows3[0].1, 0, "计数回退必须饱和为 0（不 panic、不重计）");
    }

    /// **卡 `openmir2-simmonsterai-profile` 牙**：`end_tick` 把**逐 tick 工作段**
    /// （`HostTickTotal − HostSleep`）记进 `work_samples`，`take_window` 把它带走。
    ///
    /// 变异：删掉 `end` 里 `self.tick_sleep_ns = ns` ⇒ work 恒等于 total（含 sleep）⇒ 红；
    /// 删掉 `end_tick` 的 push ⇒ 长度 0 ⇒ 红。
    #[test]
    fn end_tick_records_work_segment_per_tick() {
        let mut p = HostTickProfile::with_enabled(true);
        let slot = p.begin().unwrap();
        let t = p.begin().unwrap();
        std::thread::sleep(Duration::from_millis(2));
        p.end("HostWorldHead", Some(t));
        let t = p.begin().unwrap();
        std::thread::sleep(Duration::from_millis(6));
        p.end("HostSleep", Some(t));
        p.end_tick(Some(slot));
        assert_eq!(p.work_samples().len(), 1, "每 tick 一条工作段样本");
        let total = p
            .rows
            .iter()
            .find(|(n, _, _)| *n == "HostTickTotal")
            .expect("必须记 HostTickTotal")
            .1;
        let sleep = p
            .rows
            .iter()
            .find(|(n, _, _)| *n == "HostSleep")
            .expect("必须记 HostSleep")
            .1;
        assert_eq!(p.work_samples()[0], total - sleep, "工作段 = total − sleep");
        assert!(
            p.work_samples()[0] > 0 && p.work_samples()[0] < total,
            "工作段必须小于整 tick（sleep 被排除）"
        );
        let w = p.take_window().expect("窗口非空");
        assert_eq!(w.work_samples.len(), 1, "take_window 必须带走样本");
        assert!(p.work_samples().is_empty(), "取走后清零");
    }

    /// **逐 tick 分位数口径**：最近秩 `s[ceil(p/100·n)−1]`，输入乱序也要先排序；
    /// 空集返回 0（退化形态不 panic）。
    #[test]
    fn percentile_ns_uses_nearest_rank() {
        let s = [5u64, 1, 3, 2, 4];
        assert_eq!(percentile_ns(&s, 50.0), 3);
        assert_eq!(percentile_ns(&s, 95.0), 5);
        assert_eq!(percentile_ns(&s, 0.0), 1, "p0 = 最小值");
        assert_eq!(percentile_ns(&[42], 99.0), 42);
        assert_eq!(percentile_ns(&[], 50.0), 0, "空集退化 ⇒ 0");
    }

    /// **逐 tick 样本的窗口增量**：第二次 `world_delta` 只取**新增**样本（游标），
    /// `monai` 累计取差值。变异：把 `last.sim_ai_total = p.sim_ai_samples_total` 删掉
    /// ⇒ 第二次仍返回全量 ⇒ 红。
    #[test]
    fn world_delta_carries_per_tick_samples_without_repeating() {
        let mut p = mir2_world_bevy::world::TickProfile::default();
        p.sim_ai_tick_ns = vec![100, 200];
        p.sim_ai_samples_total = 2;
        p.monai_ticks = 2;
        p.monai.scanned = 10;
        let mut last = WorldSnapshot::default();
        let d1 = world_delta(Some(&p), &mut last).expect("profile 存在");
        assert_eq!(d1.sim_ai_tick_ns, vec![100, 200], "首窗全量");
        assert_eq!(d1.monai.scanned, 10, "首窗 monai 全量");
        assert_eq!(d1.monai_ticks, 2);

        p.sim_ai_tick_ns.push(300);
        p.sim_ai_samples_total = 3;
        p.monai_ticks = 3;
        p.monai.scanned = 15;
        let d2 = world_delta(Some(&p), &mut last).expect("profile 存在");
        assert_eq!(d2.sim_ai_tick_ns, vec![300], "次窗只取新增样本（不重复）");
        assert_eq!(d2.monai.scanned, 5, "monai 取差值");
        assert_eq!(d2.monai_ticks, 1);
    }
}
