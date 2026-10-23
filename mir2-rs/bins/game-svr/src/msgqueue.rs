//! `game_svr::msgqueue` —— [`mir2_world::msgqueue`] 的**再导出**。
//!
//! 实现（.NET 8 `PriorityQueue` 的四叉最小堆逐操作复刻 + `SendUpdateMsg` 队首-only 去重模型）
//! 已下沉到 `crates/world/src/msgqueue.rs`（卡 `openmir2-frame-drain-order-audit`）：宿主的排水点
//! 与 `crates/world` 的排水点（`party.rs`）必须共用**同一份**堆实现，不允许各处手写。
//!
//! 本模块只为保留既有引用路径（`game_svr::msgqueue::NetPriorityQueue` /
//! `game_svr::msgqueue::send_update_msg`）而存在。

pub use mir2_world::msgqueue::*;

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_protocol::messages;

    /// 协议常量 ↔ `crates/world/src/msgqueue.rs` 用例里写死的字面量必须一致
    /// （那边不依赖 `mir2-protocol`，用字面量表达 `(byte)ident`）。
    #[test]
    fn protocol_constants_match_world_literals() {
        assert_eq!((messages::RM_HIT & 0xFF) as u8, 20);
        assert_eq!((messages::RM_HEALTHSPELLCHANGED & 0xFF) as u8, 68);
        assert_eq!((messages::RM_WINEXP & 0xFF) as u8, 139);
        assert_eq!((messages::RM_LEVELUP & 0xFF) as u8, 150);
        assert_eq!((messages::RM_ABILITY & 0xFF) as u8, 67);
        assert_eq!((messages::RM_SUBABILITY & 0xFF) as u8, 62);
    }

    /// **同优先级 3 条出队 `[0,2,1]`、4 条 `[0,3,2,1]`**（.NET 4 叉堆的可见后果，dotnet 实跑）。
    /// 牙（改坏必红）：把 [`NetPriorityQueue::dequeue`] 换成「按入队序出队」⇒ 断言红。
    #[test]
    fn ties_dequeue_in_four_ary_heap_order() {
        for (n, want) in [(3usize, vec![0usize, 2, 1]), (4, vec![0, 3, 2, 1])] {
            let mut q: NetPriorityQueue<usize> = NetPriorityQueue::new();
            for i in 0..n {
                q.enqueue(i, 7);
            }
            let mut got = Vec::new();
            while let Some(e) = q.dequeue() {
                got.push(e);
            }
            assert_eq!(got, want, "n={n}：同优先级出队序必须是四叉堆序");
        }
    }
}
