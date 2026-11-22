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

/// **L2-5 ③ 段 2**：镜像**原地提升为真实体**的次数（handoff 提交侧）。
static HANDOFF_MIRROR_PROMOTED: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 2（目标侧）**：handoff 收下时目标区**没有该 id 实体**（邻区快照还没送到 ⇒ 无镜像）
/// ⇒ **直接落真身** 的次数。与 `handoff_mirror_promoted` 合计 = 跨界次数
/// （「目标区已有镜像」是被证伪的假设，见计划页 §10）。
static HANDOFF_INSERTED_FRESH: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 2（总监 2026-11-22 裁定 ①）**：`handoff_accept` 命中 `AlreadyHeld`（源区拿到
/// ack 前的**每 tick 重发**、目的地已是同 id 真实体 ⇒ 幂等收下）的次数。
///
/// **不为 0 是正常的**（重发天然会发生）；它是「幂等支被删」护栏的**直接读数**：若该支被删，
/// 重发会落到 `RefusedOccupied` ⇒ 源区回滚 ⇒ `handoff_refused > 0`（硬红线），本计数同时掉回 0。
/// **异常口径**：每次跨界收下对应的 `AlreadyHeld` 结构性上界是 3（第一次 offer 被收下；pending 寿命
/// ≤ `HANDOFF_TIMEOUT_TICKS+1 = 4` tick ⇒ 至多还有 3 次重发）——远大于 3 的突变要查。
static HANDOFF_ALREADY_HELD: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 2（总监 2026-11-22 裁定）**：目标区按 offer 身份去重时**丢弃的迟到重发**次数。
///
/// 迟到重发 = 同一次 pending epoch 的重发在目标区已提交（且可能已移除实体）之后才被排到。
/// **丢弃是对的**（它再收下就会把实体「传送」一次）；本计数是这条去重支的读数与护栏：
/// 为 0 正常（重发从未迟到），>0 说明 ack 有延迟但去重生效。
static HANDOFF_OFFER_DUP_DROPPED: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 4（总监 2026-11-22 裁决）**：镜像**写失败**（`upsert_mirror` 落格失败 =
/// `RefusedCell`）被**丢弃**的次数。**不再 abort 整机**；缺口不许静默 ⇒ 进 perf TSV +
/// 限流 WRN（id/map/坐标/原因），跨界验收必须展示它。
static MIRROR_WRITE_REFUSED_CELL: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 4**：镜像写失败的另一原因——该 id 在本区已是**真实体**（`RefusedOccupied`，
/// 处理方式 = 跳过 + `mirror_shadowed_by_local`）。按原因分列便于 ④/⑤ 拆分
/// 「handoff 窗口内」与「静态实体撞名」；本计数与 `mirror_shadowed_by_local` **同时自增**。
static MIRROR_WRITE_REFUSED_OCCUPIED: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 4（容量冒烟读数）**：handoff **offer 发射**次数（源区每次向目标区邮箱 push 一条，
/// 同一 pending epoch 的每 tick 重发各计一次）。与 `handoff_refused` 一起给 ④/⑤ 步定缺口基线。
static HANDOFF_OFFERED: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 4**：handoff **回滚/拒绝**次数（ack reject + 超时 + 落点不属任何区）。
static HANDOFF_REFUSED: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ③ 段 2（兜底，防御不是牙）**：`apply_mirrors` 收到「该 id 已是本区**真实**实体」
/// 而跳过的次数。时序正确时**恒 0**；>0 说明 handoff 时序被破坏（不要用人工变异把它当牙宣传）。
static MIRROR_SHADOWED_BY_LOCAL: AtomicU64 = AtomicU64::new(0);

/// **L2-5 ④b 诊断**：遮蔽按本地真实体类别拆两列（静态 / 玩家）。总数仍是
/// `MIRROR_SHADOWED_BY_LOCAL`；**报告与判据一律看两列分量**（避免「总数归零」掩盖单列非零）。
static MIRROR_SHADOWED_STATIC: AtomicU64 = AtomicU64::new(0);
static MIRROR_SHADOWED_HANDOFF: AtomicU64 = AtomicU64::new(0);

/// **L2-5 own-segment 修正（2026-11-22 总管裁决）**：**本区段** id 的镜像插入次数。
///
/// id 段是「分配来源」不是「当前归属」：玩家跨到区 0 后，区 1 从快照收到它的镜像时 id 仍在
/// 区 1 段 ⇒ 合法插入。**信息性计数**：非 0 正常，突发变化才查（**不是** fail-closed 判据）。
static MIRROR_OWN_SEGMENT_IDS: AtomicU64 = AtomicU64::new(0);

/// **L2-5 own-segment 修正**：handoff **回原生区**时以本区段 id 直接落真身的次数。
/// 同样**信息性**（回迁天然发生；不为 0 正常）。
static HANDOFF_FRESH_OWN_SEGMENT: AtomicU64 = AtomicU64::new(0);

/// 记一次「本区段 id 的镜像插入」（信息性）。
pub fn note_mirror_own_segment_id() {
    MIRROR_OWN_SEGMENT_IDS.fetch_add(1, Ordering::Relaxed);
}

/// 本区段 id 镜像插入次数（测试/诊断）。
#[must_use]
pub fn mirror_own_segment_ids() -> u64 {
    MIRROR_OWN_SEGMENT_IDS.load(Ordering::Relaxed)
}

/// 复位（**只给测试**）。
pub fn reset_mirror_own_segment_ids_for_tests() {
    MIRROR_OWN_SEGMENT_IDS.store(0, Ordering::Relaxed);
}

/// 记一次「handoff 回原生区 fresh 插入」（信息性）。
pub fn note_handoff_fresh_own_segment() {
    HANDOFF_FRESH_OWN_SEGMENT.fetch_add(1, Ordering::Relaxed);
}

/// handoff 回原生区 fresh 插入次数（测试/诊断）。
#[must_use]
pub fn handoff_fresh_own_segment() -> u64 {
    HANDOFF_FRESH_OWN_SEGMENT.load(Ordering::Relaxed)
}

/// 复位（**只给测试**）。
pub fn reset_handoff_fresh_own_segment_for_tests() {
    HANDOFF_FRESH_OWN_SEGMENT.store(0, Ordering::Relaxed);
}

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

/// 记一次 handoff 「直接落真身」（目标区无镜像）。
pub fn note_handoff_inserted_fresh() {
    HANDOFF_INSERTED_FRESH.fetch_add(1, Ordering::Relaxed);
}

/// 「直接落真身」计数（测试/诊断）。
#[must_use]
pub fn handoff_inserted_fresh() -> u64 {
    HANDOFF_INSERTED_FRESH.load(Ordering::Relaxed)
}

/// 复位「直接落真身」计数（**只给测试**）。
pub fn reset_handoff_inserted_fresh_for_tests() {
    HANDOFF_INSERTED_FRESH.store(0, Ordering::Relaxed);
}

/// 记一次镜像原地提升。
pub fn note_handoff_mirror_promoted() {
    HANDOFF_MIRROR_PROMOTED.fetch_add(1, Ordering::Relaxed);
}

/// 提升计数（测试/诊断）。
#[must_use]
pub fn handoff_mirror_promoted() -> u64 {
    HANDOFF_MIRROR_PROMOTED.load(Ordering::Relaxed)
}

/// 记一次镜像落格失败（RefusedCell；已丢弃，不 abort）。
pub fn note_mirror_write_refused_cell() {
    MIRROR_WRITE_REFUSED_CELL.fetch_add(1, Ordering::Relaxed);
}

/// 镜像落格失败次数（测试/诊断/容量读数）。
#[must_use]
pub fn mirror_write_refused_cell() -> u64 {
    MIRROR_WRITE_REFUSED_CELL.load(Ordering::Relaxed)
}

/// 复位镜像落格失败计数（**只给测试**）。
pub fn reset_mirror_write_refused_cell_for_tests() {
    MIRROR_WRITE_REFUSED_CELL.store(0, Ordering::Relaxed);
}

/// 记一次镜像被本地真实体占位（与 `mirror_shadowed_by_local` 同事件）。
pub fn note_mirror_write_refused_occupied() {
    MIRROR_WRITE_REFUSED_OCCUPIED.fetch_add(1, Ordering::Relaxed);
}

/// 镜像被本地真实体占位的次数（与 `mirror_shadowed_by_local` 相同口径）。
#[must_use]
pub fn mirror_write_refused_occupied() -> u64 {
    MIRROR_WRITE_REFUSED_OCCUPIED.load(Ordering::Relaxed)
}

/// 复位该计数（**只给测试**）。
pub fn reset_mirror_write_refused_occupied_for_tests() {
    MIRROR_WRITE_REFUSED_OCCUPIED.store(0, Ordering::Relaxed);
}

/// 记一次 handoff offer 发射。
pub fn note_handoff_offered() {
    HANDOFF_OFFERED.fetch_add(1, Ordering::Relaxed);
}

/// offer 发射次数（测试/诊断）。
#[must_use]
pub fn handoff_offered() -> u64 {
    HANDOFF_OFFERED.load(Ordering::Relaxed)
}

/// 复位 offer 发射计数（**只给测试**）。
pub fn reset_handoff_offered_for_tests() {
    HANDOFF_OFFERED.store(0, Ordering::Relaxed);
}

/// 记一次 handoff 回滚/拒绝。
pub fn note_handoff_refused() {
    HANDOFF_REFUSED.fetch_add(1, Ordering::Relaxed);
}

/// 回滚/拒绝次数（测试/诊断）。
#[must_use]
pub fn handoff_refused() -> u64 {
    HANDOFF_REFUSED.load(Ordering::Relaxed)
}

/// 复位回滚/拒绝计数（**只给测试**）。
pub fn reset_handoff_refused_for_tests() {
    HANDOFF_REFUSED.store(0, Ordering::Relaxed);
}

/// 记一次 handoff「重发时目的地已是同 id 真实体 ⇒ 幂等收下」。
pub fn note_handoff_already_held() {
    HANDOFF_ALREADY_HELD.fetch_add(1, Ordering::Relaxed);
}

/// 「幂等收下」次数（测试/诊断；不为 0 正常，每次跨界收下的结构性上界是 3）。
#[must_use]
pub fn handoff_already_held() -> u64 {
    HANDOFF_ALREADY_HELD.load(Ordering::Relaxed)
}

/// 复位「幂等收下」计数（**只给测试**）。
pub fn reset_handoff_already_held_for_tests() {
    HANDOFF_ALREADY_HELD.store(0, Ordering::Relaxed);
}

/// 记一次「目标区丢弃迟到重发 offer」。
pub fn note_handoff_offer_dup_dropped() {
    HANDOFF_OFFER_DUP_DROPPED.fetch_add(1, Ordering::Relaxed);
}

/// 「丢弃迟到重发」次数（测试/诊断）。
#[must_use]
pub fn handoff_offer_dup_dropped() -> u64 {
    HANDOFF_OFFER_DUP_DROPPED.load(Ordering::Relaxed)
}

/// 复位「丢弃迟到重发」计数（**只给测试**）。
pub fn reset_handoff_offer_dup_dropped_for_tests() {
    HANDOFF_OFFER_DUP_DROPPED.store(0, Ordering::Relaxed);
}

/// 记一次「镜像被本地真实体遮蔽」（防御计数；时序正确时恒 0）。
pub fn note_mirror_shadowed_by_local() {
    MIRROR_SHADOWED_BY_LOCAL.fetch_add(1, Ordering::Relaxed);
}

/// **L2-5 ④b 诊断**：遮蔽的本地实体是**静态**（非玩家：NPC/怪/守卫/商人）。
/// 语义 = 「同一静态实体在多个区各模拟一份」的直接读数；目标是归零。
pub fn note_mirror_shadowed_static() {
    MIRROR_SHADOWED_STATIC.fetch_add(1, Ordering::Relaxed);
}

/// 遮蔽的本地实体是**玩家**（handoff 收敛窗口，或玩家副本）。
pub fn note_mirror_shadowed_handoff() {
    MIRROR_SHADOWED_HANDOFF.fetch_add(1, Ordering::Relaxed);
}

/// 静态撞名遮蔽计数。
#[must_use]
pub fn mirror_shadowed_static() -> u64 {
    MIRROR_SHADOWED_STATIC.load(Ordering::Relaxed)
}

/// 玩家遮蔽计数。
#[must_use]
pub fn mirror_shadowed_handoff() -> u64 {
    MIRROR_SHADOWED_HANDOFF.load(Ordering::Relaxed)
}

/// 复位两列拆分计数（**只给测试**）。
pub fn reset_mirror_shadowed_split_for_tests() {
    MIRROR_SHADOWED_STATIC.store(0, Ordering::Relaxed);
    MIRROR_SHADOWED_HANDOFF.store(0, Ordering::Relaxed);
}

/// 遮蔽计数（测试/诊断）。
#[must_use]
pub fn mirror_shadowed_by_local() -> u64 {
    MIRROR_SHADOWED_BY_LOCAL.load(Ordering::Relaxed)
}

/// 复位提升计数（**只给测试**）。
pub fn reset_handoff_mirror_promoted_for_tests() {
    HANDOFF_MIRROR_PROMOTED.store(0, Ordering::Relaxed);
}

/// 复位遮蔽计数（**只给测试**）。
pub fn reset_mirror_shadowed_by_local_for_tests() {
    MIRROR_SHADOWED_BY_LOCAL.store(0, Ordering::Relaxed);
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
