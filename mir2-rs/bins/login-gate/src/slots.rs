//! C# LoginGate 的会话槽位表（`ClientThread.SessionArray`）与会话上限（审计 B-3）。
//!
//! oracle 事实（`src/LoginGate`，逐行核对）：
//! - `GateShare.MaxSession = 10000`（`GateShare.cs:10`），`ClientThread` 构造时
//!   `SessionArray = new TSessionInfo[MaxSession]`（`ClientThread.cs:40`）。
//! - 接入分配：`ClientManager.GetClientThread():130-149` 先 `SessionIsFull()` 判满；
//!   满 ⇒ 返回 null ⇒ `ServerService.ServerSocketClientConnect:65-70` 只打
//!   `获取登陆服务失败。` 并**直接 return**（不关客户端 socket、不建会话、不发 `Enter`）；
//!   未满 ⇒ `ServerService.cs:82-94` 从下标 0 线性找第一个 null 槽，`:91` 写入新会话。
//! - 回收**只有两处**：
//!   ① 下游断开 `ClientThread.ClientSocketDisconnect:116-135` ⇒ 所有槽置 null；
//!   ② 空闲超时 `TimedService.ProcessDelayMsg:79-80` ⇒ 只把命中的那个槽置 null。
//!   客户端正常断开（`ServerService.ServerSocketClientDisconnect:113-127`）**不清槽**
//!   ⇒ 每个正常断开都泄漏一个槽，1e4 次后整网关拒绝新会话。这是 oracle 的既定缺陷，
//!   本模块逐行照抄（AGENTS.md「行为等价优先于修 bug」，卡 B-3 选项 A）。
//!
//! 上限**没有配置键**：oracle 只有 `GateShare.MaxSession` 常量，`config.conf` 不读它。

/// C# `GateShare.cs:10` `MaxSession = 10000`。
pub const MAX_SESSION: usize = 10_000;

/// `ClientThread.SessionArray` 的等价物：定长槽位数组，值为该槽位占用的
/// `ConnectionId`（Rust 侧即多路复用 id）。
#[derive(Debug, Clone, Default)]
pub struct SlotTable {
    slots: Vec<Option<u32>>,
}

impl SlotTable {
    /// 生产口径：`new TSessionInfo[MaxSession]`。
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(MAX_SESSION)
    }

    /// 容量可注入（仅构造级测试用；生产固定 [`MAX_SESSION`]）。
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            slots: vec![None; cap],
        }
    }

    /// 槽位总数（`SessionArray.Length`）。
    #[cfg(test)]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// 当前非空槽数（oracle `GetSessionCount()` 的记账口径）。
    #[cfg(test)]
    #[must_use]
    pub fn occupied(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// C# `ClientThread.SessionIsFull():84-96`：全部槽位非 null 才算满。
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.slots.iter().all(Option::is_some)
    }

    /// C# `ServerService.ServerSocketClientConnect:82-94` 的线性分配（找第一个 null 槽）。
    ///
    /// 调用前提 `!is_full()`（oracle 由 `ClientManager.GetClientThread` 先判满）。
    /// 返回槽位下标；数组已满时返回 `None`（该分支在 oracle 里被 `GetClientThread`
    /// 的判满抢先，正常时序不可达）。
    pub fn alloc(&mut self, connection_id: u32) -> Option<usize> {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(connection_id);
                return Some(index);
            }
        }
        None
    }

    /// C# `ClientThread.ClientSocketDisconnect:118-130`：遍历所有槽并置 null。
    pub fn reset(&mut self) {
        self.slots.fill(None);
    }

    /// C# `TimedService.ProcessDelayMsg:80`：`SessionArray[j] = null`（只清命中的槽）。
    pub fn clear(&mut self, index: usize) {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = None;
        }
    }

    /// 该槽位当前占用的 `ConnectionId`（构造级测试与对读用）。
    #[cfg(test)]
    #[must_use]
    pub fn get(&self, index: usize) -> Option<u32> {
        self.slots.get(index).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::{SlotTable, MAX_SESSION};

    /// 判据：上限是 oracle 常量 10000；oracle 没有配置键，故这里也不读配置。
    #[test]
    fn max_session_is_oracle_constant() {
        assert_eq!(MAX_SESSION, 10_000, "GateShare.cs:10 MaxSession = 10000");
        assert_eq!(SlotTable::new().capacity(), MAX_SESSION);
        assert_eq!(SlotTable::new().occupied(), 0);
        assert!(!SlotTable::new().is_full());
    }

    /// 判据（达上限后）：cap=4 时第 5 个连接分配不到槽 ⇒ 上层走 oracle
    /// `GetClientThread() == null` 分支（`获取登陆服务失败。`，不建会话、不关 socket）。
    /// 牙：把 `is_full` 改成 `any`、或 `alloc` 允许覆盖非空槽 ⇒ 本用例红。
    #[test]
    fn full_table_rejects_fifth_allocation() {
        let mut table = SlotTable::with_capacity(4);
        for sid in 1..=4 {
            assert_eq!(table.alloc(sid), Some(usize::try_from(sid).unwrap() - 1));
        }
        assert_eq!(table.occupied(), 4);
        assert!(table.is_full());
        assert_eq!(table.alloc(5), None, "满时不得分配（oracle 拒接入）");
        assert_eq!(table.occupied(), 4, "被拒的连接不得占用槽位");
    }

    /// 判据（断开后的槽位状态）：客户端正常断开**不回收**槽位（oracle 缺陷照抄）。
    /// 牙：`SlotTable` 若新增“断开即清槽”的调用或 `alloc` 自动回收 ⇒ 本用例红。
    #[test]
    fn normal_disconnect_leaks_slot() {
        let mut table = SlotTable::with_capacity(2);
        assert_eq!(table.alloc(7), Some(0));
        assert_eq!(table.alloc(8), Some(1));
        // C# `ServerService.ServerSocketClientDisconnect` 只 CloseSession，
        // 不触碰 `SessionArray` ⇒ 这里刻意不做任何调用。
        assert!(table.is_full(), "正常断开后槽位仍应被占用（泄漏照抄）");
        assert_eq!(table.alloc(9), None, "泄漏的槽位不得被复用");
        assert_eq!(table.get(0), Some(7), "陈旧槽保留旧 ConnectionId");
        assert_eq!(table.get(1), Some(8));
    }

    /// 判据（回收点）：超时只清命中槽；下游断开清全部。
    /// 牙：把 `reset` 写成只清一个、或 `clear` 写成清全部 ⇒ 本用例红。
    #[test]
    fn timeout_clears_one_slot_and_downstream_reset_clears_all() {
        let mut table = SlotTable::with_capacity(4);
        for sid in [10, 11, 12, 13] {
            assert!(table.alloc(sid).is_some());
        }
        table.clear(1); // TimedService.ProcessDelayMsg: SessionArray[1] = null
        assert_eq!(table.occupied(), 3);
        assert_eq!(table.get(1), None);
        // 下一个连接按下标顺序拿到第一个空槽（oracle 线性扫描）。
        assert_eq!(table.alloc(14), Some(1));
        assert_eq!(table.occupied(), 4);
        table.reset(); // ClientThread.ClientSocketDisconnect: 全置 null
        assert_eq!(table.occupied(), 0);
        assert!(!table.is_full());
        assert_eq!(table.alloc(15), Some(0));
    }

    /// 判据（陈旧槽构造）：满表被下游断开后，旧 ConnectionId 全部消失；
    /// 而正常断开留下的陈旧槽在满表时持续挡住新连接。两态必须可区分。
    #[test]
    fn stale_slots_block_until_downstream_disconnect() {
        let mut table = SlotTable::with_capacity(3);
        for sid in [100, 101, 102] {
            assert!(table.alloc(sid).is_some());
        }
        // 三个客户端都“正常断开”：槽位保持（123..=125 被拒）。
        assert_eq!(table.alloc(103), None);
        assert_eq!(table.occupied(), 3);
        table.reset();
        // 下游重连（ClientSocketConnect:106 RestSessionArray）前槽位已由
        // ClientSocketDisconnect 清空；此后新连接从头分配。
        assert_eq!(table.alloc(104), Some(0));
    }
}
