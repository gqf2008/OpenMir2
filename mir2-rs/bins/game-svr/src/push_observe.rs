//! **只记录**：推挤帧的收件人（`SendRefMsg` 的 500ms `VisibleHumanList` 缓存过滤后的列表）。
//!
//! 卡 `DEV-MG57-RM-PUSH-CACHE`（`docs/issues/golden-harness-blind-spots.md` B30）：parity 台账的
//! `RM_PUSH(8092)` 过去由驱动用无缓存的 `chat::hear_targets_at` 重建，与生产帧
//! `SM_BACKSTEP(9)`（走 `ref_msg_cache`：首扫 `|Δ|≤range`、同窗口复用 `|Δ|<11`）不同源。
//! 本模块在**推挤帧发射点**把该次 `ref_msg_cache` 的收件人原样抄下来，供驱动在 P2 用它重建 `Rm`。
//!
//! **生产零影响**：缺省未武装 ⇒ 每个推挤帧只多一次 `thread_local` 读，不分配、不加锁、
//! 不改任何帧/状态；只有 parity 驱动显式 [`arm`] 后才开始记录，驱动取用后清空（[`take`]）。
//!
//! **线程局部**：日志与武装标志都是 `thread_local`——世界 tick 必须同线程串行（见 `World::tick`
//! 的断言），而默认门用例在同一个测试二进制里**并行**跑多个 case；进程级静态会让并行用例互相
//! 污染（2026-11-26 实测：`记录条数 4 != 推挤格数 1`）。

use std::cell::{Cell, RefCell};

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static LOG: RefCell<Vec<PushTargets>> = const { RefCell::new(Vec::new()) };
}

/// 一条推挤帧的收件人记录（逐格一条，按发射序）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushTargets {
    /// 被推者（`SendRefMsg(RM_PUSH)` 的发送者）。
    pub actor_id: i32,
    /// 该格坐标（收件人按此格算，见 `ref_msg_cache::targets_at`）。
    pub x: i32,
    pub y: i32,
    /// 缓存过滤后的收件人（与帧收件人**同源**）。
    pub targets: Vec<i32>,
}

/// 是否已武装（调用方据此**只在武装时**克隆收件人列表，保证生产路径零分配）。
pub(crate) fn armed() -> bool {
    ARMED.with(Cell::get)
}

/// 武装记录（parity 驱动在每个 case 开始前调用）。
///
/// **P1 只落能力**：目前只有本模块的单测调用；P2 的 `push_assert` 会用它（`allow(dead_code)`
/// 标明"生产不调用、测试驱动专用"，不是遗漏接线）。
#[allow(dead_code)]
pub fn arm() {
    ARMED.with(|a| a.set(true));
}

/// 取走本 case 的全部记录并解除武装（驱动在 case 结束后调用）。
#[must_use]
#[allow(dead_code)] // 同上：P2 的驱动才会调用。
pub fn take() -> Vec<PushTargets> {
    ARMED.with(|a| a.set(false));
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

/// 推挤帧发射点调用：未武装 ⇒ 立即返回（零分配）。
pub(crate) fn note(actor_id: i32, x: i32, y: i32, targets: &[i32]) {
    if !armed() {
        return;
    }
    LOG.with(|l| {
        l.borrow_mut().push(PushTargets {
            actor_id,
            x,
            y,
            targets: targets.to_vec(),
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_is_noop_when_disarmed_and_drains_when_armed() {
        // 未武装：note 不产生记录（"零行为"面的负例）。
        let _ = take();
        note(1, 10, 10, &[2, 3]);
        assert!(take().is_empty(), "未武装时不得记录");

        // 武装后逐条记录、按发射序；take 取走并解除武装。
        arm();
        note(1, 10, 10, &[2, 3]);
        note(1, 11, 10, &[2]);
        let got = take();
        assert_eq!(
            got,
            vec![
                PushTargets {
                    actor_id: 1,
                    x: 10,
                    y: 10,
                    targets: vec![2, 3]
                },
                PushTargets {
                    actor_id: 1,
                    x: 11,
                    y: 10,
                    targets: vec![2]
                },
            ]
        );
        note(1, 12, 10, &[2]);
        assert!(take().is_empty(), "take 之后必须自动解除武装");
    }
}
