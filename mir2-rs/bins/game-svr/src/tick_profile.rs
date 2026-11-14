//! **T10b**：`sim_loop` 宿主段计时（**默认关闭**；`MIR2_TICK_PROFILE=1` 开启）。
//!
//! 背景：T10 只量了 `World::tick`（11k 实体 63.97ms），而演示栈端到端 tick ≈ 2000ms ——
//! 32× 差额在**宿主段**（`bins/game-svr` 的 `sim_loop`：P21..P32、帧生成/会话输出、AOI 下行、
//! DB 轮询、踢人/存盘落地等）。本模块把这些段按**与 `crates/world` 的 `TickProfile` 同一口径**
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

use std::time::Instant;

/// 环境变量门：**只有字面 `"1"`** 才开启（`0`/`true`/空/缺省一律关；与起栈器的 fail-closed 口径同形）。
pub const ENV_TICK_PROFILE: &str = "MIR2_TICK_PROFILE";

/// 汇总窗口（**两臂先到者**）：50 tick 或 10s 墙钟。200ms 节拍下两者等价；
/// tick 被拖慢（演示栈端到端 ≈ 2000ms）时按墙钟臂切窗，60s 的观测窗内仍有多行汇总。
pub const REPORT_TICKS: u64 = 50;

/// 汇总窗口的墙钟臂（10s；用 `window_ns`（整 tick 之和）判，不额外取时钟）。
pub const REPORT_WINDOW_NS: u64 = 10_000_000_000;

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
    /// 自开启起累计完成的 tick 数（诊断用）。
    total_ticks: u64,
}

/// 一个窗口的宿主汇总（由 [`HostTickProfile::take_window`] 产出）。
#[derive(Debug, Clone, Default)]
pub struct HostWindow {
    pub ticks: u64,
    pub total_ns: u64,
    pub rows: Vec<(&'static str, u64, u64)>,
    /// 命名子探针（与 `rows` 同形；同窗口取走）。
    pub probes: Vec<(&'static str, u64, u64)>,
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
}

/// 跨窗口保存的 World 累计快照（用于算增量；World 探针本身不 reset）。
#[derive(Debug, Clone, Default)]
pub struct WorldSnapshot {
    ticks: u64,
    stages: Vec<(String, u64, u64)>,
    probes: Vec<(String, u64, u64)>,
}

/// 环境变量取值 → 门控：只有 `"1"` 为真（其余一律关，含 `"true"`/`"0"`/空）。
#[must_use]
pub fn enabled_from(value: Option<&str>) -> bool {
    value == Some("1")
}

impl HostTickProfile {
    /// 读环境变量建计时器（**整个进程只读一次**；tick 路径上不再读环境）。
    #[must_use]
    pub fn from_env() -> Self {
        let value = std::env::var(ENV_TICK_PROFILE).ok();
        Self {
            enabled: enabled_from(value.as_deref()),
            ..Self::default()
        }
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
        self.add(name, ns);
        self.tick_sum_ns = self.tick_sum_ns.saturating_add(ns);
    }

    /// 命名**子探针**终点。与 [`Self::end`] 同样的关闭态口径（`at == None` ⇒ 一次 `Option`
    /// 判别后直接返回），但**不写 `tick_sum_ns`** ⇒ 子探针与父段可以嵌套计时而不让
    /// `HostUnaccounted` 出现负残差。
    #[inline]
    pub fn probe(&mut self, name: &'static str, at: Option<Instant>) {
        let Some(at) = at else { return };
        let ns = nanos_since(at);
        match self.probes.iter_mut().find(|(n, _, _)| *n == name) {
            Some((_, total, calls)) => {
                *total = total.saturating_add(ns);
                *calls += 1;
            }
            None => self.probes.push((name, ns, 1)),
        }
    }

    /// 整 tick 终点：记 `HostTickTotal` + 残差 `HostUnaccounted`，并推进窗口计数。
    ///
    /// 关闭时：一次 `Option` 判别 ⇒ 直接返回。
    #[inline]
    pub fn end_tick(&mut self, at: Option<Instant>) {
        let Some(at) = at else { return };
        let total = nanos_since(at);
        self.add("HostTickTotal", total);
        let sum = self.tick_sum_ns;
        if total > sum {
            self.add("HostUnaccounted", total - sum);
        }
        self.tick_sum_ns = 0;
        self.window_ticks += 1;
        self.window_ns = self.window_ns.saturating_add(total);
        self.total_ticks += 1;
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
    Some(WorldDelta {
        ticks,
        stages,
        probes,
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

    /// **环境门**：只有字面 `"1"` 开启；`"true"`/`"0"`/空/缺省一律关（fail-closed）。
    #[test]
    fn env_gate_accepts_only_literal_one() {
        assert!(enabled_from(Some("1")));
        for v in [Some("0"), Some("true"), Some(""), Some(" 1"), None] {
            assert!(!enabled_from(v), "{v:?} 不得开启门控");
        }
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
}
