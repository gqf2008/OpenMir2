//! OpenMir2 公共工具。
//!
//! - [`rng`]：复刻 `System.Random`（固定种子走 legacy 减法算法）与
//!   `src/OpenMir2/RandomNumber.cs` 的包装语义。
//! - [`hutil`]：`src/OpenMir2/HUtil32.cs` 中数值公式用到的等价物。

/// `due_ms_rel` 的 **TickCount 容差（毫秒）——唯一来源**：比较器（`mir2-parity-tests`）与
/// 驱动（`game-svr` 的 `fireball_case_assert`）共用，禁止两处各写 `32`。
///
/// `due_ms_rel = DeliveryTime − t0` 由 oracle 的 `Environment.TickCount`（15.6ms 粒度）派生
/// ⇒ 同一次生成会抖动（实测同一 stem 的 `640` 在 `1000/1016` 间变）；**精确相等不是该量的
/// 正确判据**。行为面只判批次类（施法批 0 vs 交付批 >0），毫秒按 `|Δ| ≤ 32`（2×粒度）。
pub const DUE_MS_TOLERANCE_MS: i64 = 32;

pub mod hutil;
/// `src/OpenMir2/HUtil32.cs` 的 1:1 逐函数移植（含原实现缺陷，见各函数文档）。
pub mod hutil32;
pub mod replay;
pub mod rng;
/// **M3②③ 决策片（owner 2026-10-12 18:00）**：opt-in 的逐次 RNG trace（与 C# `rng-hook` 同形）。
pub mod rng_trace;
