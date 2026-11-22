//! **L2-5 ② 的镜像诊断计数**（进程级原子；只服务对拍/判据，不参与任何业务分支）。
//!
//! 为什么不在 `bins/game-svr/src/perf_counters`：这两处计数点分别在 `monai`（AI 隔离）与
//! `world::attack_once_impl`（攻击隔离），都在本 crate；`perf_counters` 是宿主 crate 的私有
//! 模块。口径相同（Relaxed 原子、测试可读、复位只给测试）。

use std::sync::atomic::{AtomicU64, Ordering};

/// 镜像被怪物 AI 段驱动的次数——**正常恒 0**（`monster_ai_tick` 的 `ids.retain` 排除镜像）。
static MIRROR_AI_ACTIONS: AtomicU64 = AtomicU64::new(0);

/// 指向镜像的攻击次数（v1 **只计数、不生效**；跨区伤害生效留 intent 步）。
static CROSS_HIT_DELAYED: AtomicU64 = AtomicU64::new(0);

/// 快照里**已落格但查不到格内序**的次数（`cell_pos = usize::MAX`）。
///
/// **合法中间态**：`add_to_mapped` 在生成/登录时就置真，进格要等 `Command::Enter` 被消费；
/// 本 tick 的 AOI 也看不见它。②对拍夹具必须断言本计数为 0（夹具里真取不到格内序 = 牙）。
static MIRROR_CELL_POS_UNKNOWN: AtomicU64 = AtomicU64::new(0);

/// 记一次「镜像进了 AI 段」（隔离被拆的唯一读数）。
pub fn note_mirror_ai_action() {
    MIRROR_AI_ACTIONS.fetch_add(1, Ordering::Relaxed);
}

/// 当前读数（测试/诊断）。
#[must_use]
pub fn mirror_ai_actions() -> u64 {
    MIRROR_AI_ACTIONS.load(Ordering::Relaxed)
}

/// 记一次「攻击指向镜像」（Q2 裁决：只计数、不生效）。
pub fn note_cross_hit_delayed() {
    CROSS_HIT_DELAYED.fetch_add(1, Ordering::Relaxed);
}

/// 当前读数（测试/诊断）。
#[must_use]
pub fn cross_hit_delayed() -> u64 {
    CROSS_HIT_DELAYED.load(Ordering::Relaxed)
}

/// 记一次「快照取不到格内序」（合法中间态；②夹具断言恒 0）。
pub fn note_mirror_cell_pos_unknown() {
    MIRROR_CELL_POS_UNKNOWN.fetch_add(1, Ordering::Relaxed);
}

/// 当前读数（测试/诊断）。
#[must_use]
pub fn mirror_cell_pos_unknown() -> u64 {
    MIRROR_CELL_POS_UNKNOWN.load(Ordering::Relaxed)
}

/// 复位「格内序未知」计数（**只给测试**）。
pub fn reset_mirror_cell_pos_unknown_for_tests() {
    MIRROR_CELL_POS_UNKNOWN.store(0, Ordering::Relaxed);
}

/// 复位 AI 计数（**只给测试**；并行用例只复位自己读的计数，避免互相清零）。
pub fn reset_mirror_ai_actions_for_tests() {
    MIRROR_AI_ACTIONS.store(0, Ordering::Relaxed);
}

/// 复位「指向镜像的攻击」计数（**只给测试**）。
pub fn reset_cross_hit_delayed_for_tests() {
    CROSS_HIT_DELAYED.store(0, Ordering::Relaxed);
}
