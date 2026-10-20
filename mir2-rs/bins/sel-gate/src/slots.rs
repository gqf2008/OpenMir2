//! C# SelGate 的会话槽位表（`ClientThread.SessionArray`）与会话上限（审计 B-3）。
//!
//! oracle 事实（`src/SelGate`，逐行核对）：
//! - `ClientThread.cs:28` `MaxSession = 2000`；构造时 `SessionArray = new SessionInfo[MaxSession]`
//!   （`ClientThread.cs:65`）。
//! - `ServerService.ServerSocketClientConnect:99-110` 的分配循环：
//!   ```text
//!   for (nIdx = 0; nIdx < MaxSession; nIdx++) {
//!       sessionInfo = clientThread.SessionArray[nIdx];
//!       if (sessionInfo == null) {
//!           sessionInfo = new SessionInfo();
//!           sessionInfo.SocketId = client.Id; ...
//!           break;
//!       }
//!   }
//!   ```
//!   **注意：没有 `SessionArray[nIdx] = sessionInfo;`**——与 LoginGate 的同段代码
//!   （`ServerService.cs:91` 有回写）不同。运行期数组因此恒为全 null，`select()` 永远
//!   返回 `Fresh`，`MaxSession` 上限与下面的“全满 ⇒ 保留最后一个非空槽”分支
//!   **在 oracle 里不可达**（评审员更正只指出“满时返回最后一个非空槽”，未发现漏写回）。
//!   本模块仍把该循环逐行实现（含陈旧槽语义），并用构造级用例（人工填充数组）钉住；
//!   运行时按 oracle 事实**从不写回**（`select` 取 `&self`，类型上禁止）。
//! - 回收：`ServerService.ServerSocketClientDisconnect:126-150` 只 `CloseSession`（摘会话），
//!   不碰数组；`RestSessionArray():231-242` 只把非空槽的 `SocketId`/`ClientIP` 置空，
//!   **不置 null**；`TimedService.CleanOutSession:90-100` 只把**局部变量**置 null（同样漏写回）。
//!   即：**没有任何代码会把槽位写回 null**——数组为空只是因为它从未被填充。
//! - 上限**没有配置键**：oracle 只有 `ClientThread.MaxSession` 常量。

/// C# `SelGate/Services/ClientThread.cs:28` `MaxSession = 2000`。
pub const MAX_SESSION: usize = 2_000;

/// C# `SelGate/Datas/SessionInfo.cs` 的两个可观测字段（`SocketId` / `ClientIP`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotInfo {
    /// C# `SessionInfo.SocketId`：客户端 socket 的 TouchSocket id（Rust 侧为多路复用 id 字符串）。
    pub socket_id: String,
    /// C# `SessionInfo.ClientIP`。
    pub client_ip: String,
}

/// `ServerService.ServerSocketClientConnect:99-118` 的选择结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotChoice {
    /// 找到 null 槽：`new SessionInfo()`（新连接的 id/ip）。
    Fresh(SlotInfo),
    /// 全满：循环结束后 `sessionInfo` 保留**最后一个非空槽**（陈旧会话）。
    Stale(SlotInfo),
}

/// `ClientThread.SessionArray` 的等价物。
#[derive(Debug, Clone, Default)]
pub struct SlotTable {
    slots: Vec<Option<SlotInfo>>,
}

impl SlotTable {
    /// 生产口径：`new SessionInfo[MaxSession]`。
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

    /// 当前非空槽数。
    #[cfg(test)]
    #[must_use]
    pub fn occupied(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// `ServerService.ServerSocketClientConnect:99-110` 的选择循环。
    ///
    /// **oracle 在该循环里从不写回 `SessionArray`**（见模块注释）⇒ 本方法取 `&self`：
    /// 生产路径每次都返回 `Fresh`；`Stale` 只在人工预填充的数组上可达（构造级用例）。
    #[must_use]
    pub fn select(&self, socket_id: &str, client_ip: &str) -> SlotChoice {
        for slot in &self.slots {
            if slot.is_none() {
                return SlotChoice::Fresh(SlotInfo {
                    socket_id: socket_id.to_string(),
                    client_ip: client_ip.to_string(),
                });
            }
        }
        // 全满：循环结束时 `sessionInfo` 是最后一个非空槽（不是 null）⇒ 上层走
        // `AddClientThread(staleId)` / `UserEnter()` / `AddSession(staleId)`。
        match self.slots.last().and_then(Clone::clone) {
            Some(stale) => SlotChoice::Stale(stale),
            // cap == 0 时 `sessionInfo` 仍是初值 null ⇒ 走 `禁止连接`（同样不可达）。
            None => SlotChoice::Fresh(SlotInfo {
                socket_id: socket_id.to_string(),
                client_ip: client_ip.to_string(),
            }),
        }
    }

    /// C# `ClientThread.RestSessionArray():231-242`：非空槽只清 `SocketId`/`ClientIP`，
    /// **不置 null**（槽位/占用状态保留）。
    pub fn rest(&mut self) {
        for info in self.slots.iter_mut().flatten() {
            info.socket_id.clear();
            info.client_ip.clear();
        }
    }

    /// 构造级用例：把槽位直接填成“陈旧会话”（oracle 运行期不会发生）。
    #[cfg(test)]
    fn set(&mut self, index: usize, info: SlotInfo) {
        self.slots[index] = Some(info);
    }
}

#[cfg(test)]
mod tests {
    use super::{SlotChoice, SlotInfo, SlotTable, MAX_SESSION};

    fn info(id: &str, ip: &str) -> SlotInfo {
        SlotInfo {
            socket_id: id.to_string(),
            client_ip: ip.to_string(),
        }
    }

    /// 判据：上限是 oracle 常量 2000；无配置键。
    #[test]
    fn max_session_is_oracle_constant() {
        assert_eq!(MAX_SESSION, 2_000, "SelGate ClientThread.cs:28");
        assert_eq!(SlotTable::new().capacity(), MAX_SESSION);
        assert_eq!(SlotTable::new().occupied(), 0);
    }

    /// 判据（oracle 事实）：分配循环**不写回数组** ⇒ 连续接入永远 `Fresh`，
    /// 跑满 3 倍 MaxSession 也到不了“满”，陈旧槽分支在运行期不可达。
    /// 牙：给 `select` 加上写回（`&mut self` + `*slot = Some(...)`）⇒ 第 cap+1 次
    /// 会变成 `Stale` ⇒ 本用例红。
    #[test]
    fn selection_never_fills_slots_because_oracle_does_not_write_back() {
        let mut table = SlotTable::with_capacity(4);
        for n in 0..12 {
            let choice = table.select(&n.to_string(), "10.0.0.1");
            assert!(
                matches!(choice, SlotChoice::Fresh(_)),
                "第 {n} 次接入应总是 Fresh（oracle 不写回数组），实得 {choice:?}"
            );
            table.rest(); // 下游每次连接/断开都会 RestSessionArray
        }
        assert_eq!(table.occupied(), 0, "oracle 运行期数组恒为空");
    }

    /// 判据（陈旧槽构造）：人工把 cap=4 的数组填满后，
    /// 第 5 个连接返回**最后一个非空槽**（下标 3）的 id/ip，而不是自己的 id/ip。
    /// 牙：把 `last()` 改成 `first()`/`find_map` ⇒ 本用例红。
    #[test]
    fn full_table_reuses_last_non_null_slot() {
        let mut table = SlotTable::with_capacity(4);
        for i in 0..4 {
            table.set(i, info(&format!("stale-{i}"), "192.0.2.9"));
        }
        assert_eq!(table.occupied(), 4);
        assert_eq!(
            table.select("new-conn", "198.51.100.7"),
            SlotChoice::Stale(info("stale-3", "192.0.2.9")),
            "满时 oracle 保留 loop 的最后一次赋值（下标 3 的陈旧槽）"
        );
    }

    /// 判据（RestSessionArray）：只清 SocketId/ClientIP，**不置 null**；
    /// 之后满表选择得到的是“空 id/空 ip 的陈旧槽”（`%K//$` 的来源）。
    /// 牙：把 `rest` 改成置 null ⇒ 首次 `occupied()` 变 0、选择变 Fresh ⇒ 本用例红。
    #[test]
    fn rest_clears_ids_but_keeps_slots() {
        let mut table = SlotTable::with_capacity(2);
        table.set(0, info("stale-0", "192.0.2.9"));
        table.set(1, info("stale-1", "192.0.2.10"));
        table.rest();
        assert_eq!(table.occupied(), 2, "RestSessionArray 不清槽");
        assert_eq!(
            table.select("new-conn", "198.51.100.7"),
            SlotChoice::Stale(info("", "")),
            "满表 + 已 Rest ⇒ 最后一个陈旧槽的 id/ip 均为空"
        );
    }
}
