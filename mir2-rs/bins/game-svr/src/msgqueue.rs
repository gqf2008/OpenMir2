//! `.NET PriorityQueue<TElement, TPriority>` 的**出队序**等价物（优先级 = `u8`）。
//!
//! # 为什么需要它
//!
//! oracle 的观察者消息队列是 `PriorityQueue<SendMessage, byte>`
//! （`src/M2Server/Actor/ActorEntity.cs:16`），入队 `MsgQueue.Enqueue(msg, (byte)wIdent)`
//! （`BaseObject.Message.cs:181`），出队 `TryDequeue`
//! （`BaseObject.Message.cs:338/361`）。**同优先级元素的出队序不是入队序** —— 它是 d 叉最小堆
//! 的 sift 结果（.NET 文档明示同优先级「not guaranteed to be stable」）。若按「优先级稳定排序」
//! 复刻，同优先级多条消息的**相对次序会与 oracle 相反**（`hero_pet_diff.py` 逐项比帧种类序 ⇒ 真机必红）。
//!
//! # 算法（.NET 8 `System.Collections.Generic.PriorityQueue`）
//!
//! .NET 8 用的是 **4 叉最小堆**（`Arity = 4`、`Log2Arity = 2`）：
//! `GetParentIndex(i) = (i-1) >> 2`、`GetFirstChildIndex(i) = (i << 2) + 1`。
//! 入队 `MoveUpDefaultComparer`（严格小于父才上浮）；出队 `RemoveRootNode`
//! （末元素回填根 + `MoveDownDefaultComparer`：在 4 个孩子里取**首个**最小者，末元素 `<=` 它时停）。
//! 本模块逐操作复刻之，使同一串 `(element, priority)` 入队得到与 oracle **逐位相同**的出队序。
//!
//! 证据（net8.0.26 实跑 `dotnet`，见
//! `mir2-rs/tests/parity/m3/evidence-m3-pet-slave-mutiny-frames-20261014/msgqueue-dotnet-ref.txt`）：
//! `c` = 优先级 154（`(byte)RM_CHANGENAMECOLOR` = `8090 & 0xFF`）、
//! `n` = 优先级 165（`(byte)RM_USERNAME` = `8101 & 0xFF`），入队序 `c_n,n_n,…,c_1,n_1`：
//!
//! ```text
//! n=1: c1,n1
//! n=2: c2,c1,n1,n2
//! n=3: c3,c2,c1,n2,n1,n3
//! n=4: c4,c1,c3,c2,n2,n3,n4,n1
//! n=5: c5,c2,c1,c4,c3,n3,n5,n4,n2,n1
//! n=6: c6,c3,c1,c2,c5,c4,n6,n4,n3,n5,n1,n2
//! n=7: c7,c4,c1,c3,c2,c6,c5,n4,n7,n5,n1,n6,n3,n2
//! n=8: c8,c5,c4,c7,c2,c3,c1,c6,n1,n5,n8,n6,n4,n2,n3,n7
//! ```
//!
//! # 边界（未 1:1，登记见卡 `openmir2-m3-pet-slave-masterdie-wire`）
//!
//! 本模块只复刻**堆算法本身**（含 .NET 8 的 4 叉布局）。oracle 的真实出队序还取决于
//! **入队前队列里已有的其它消息**（跨 tick、其它系统的下行）；本项目的下行族按「每 tick 直发」
//! 产出、不建模整条观察者队列，故本模块用于**同一批同源入队**（如叛变帧的 `色/名` 序列）内部的
//! 出队序；混合队列场景登记为 `not_verified`（R 级，真机窗口）。堆布局随 .NET 运行时版本可能变，
//! 本模块对齐**现状 oracle 运行版本 net8.0.26**。

/// `.NET` `PriorityQueue` 的 d 叉数（`Arity = 4`，`Log2Arity = 2`）。
const LOG2_ARITY: u32 = 2;
/// `Arity = 1 << LOG2_ARITY`。
const ARITY: usize = 1 << LOG2_ARITY;

/// `.NET` `GetParentIndex`：`(i - 1) >> Log2Arity`。
#[inline]
fn parent_index(index: usize) -> usize {
    (index - 1) >> LOG2_ARITY
}

/// `.NET` `GetFirstChildIndex`：`(i << Log2Arity) + 1`。
#[inline]
fn first_child_index(index: usize) -> usize {
    (index << LOG2_ARITY) + 1
}

/// `.NET PriorityQueue<TElement, TPriority>`（`TPriority = byte`）的堆序等价物。
#[derive(Debug, Clone)]
pub struct NetPriorityQueue<T> {
    nodes: Vec<(T, u8)>,
}

impl<T> Default for NetPriorityQueue<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

impl<T> NetPriorityQueue<T> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `.NET PriorityQueue.Enqueue` → `MoveUpDefaultComparer`：尾部追加 + **上浮**
    /// （严格小于父才上浮，hole 法与交换法末态相同）。
    pub fn enqueue(&mut self, element: T, priority: u8) {
        let mut node_index = self.nodes.len();
        self.nodes.push((element, priority));
        while node_index > 0 {
            let parent = parent_index(node_index);
            if self.nodes[parent].1 <= self.nodes[node_index].1 {
                break;
            }
            self.nodes.swap(parent, node_index);
            node_index = parent;
        }
    }

    /// `.NET Dequeue` → `RemoveRootNode`：根取出，**末元素**回填根后 `MoveDownDefaultComparer`
    /// 下沉（4 个孩子里取**首个**最小者；末元素 `<=` 选中子时停）。
    pub fn dequeue(&mut self) -> Option<T> {
        if self.nodes.is_empty() {
            return None;
        }
        // `Vec::swap_remove(0)` == oracle `RemoveRootNode` 的「末元素落根 + size--」。
        let (root, _) = self.nodes.swap_remove(0);
        let size = self.nodes.len();
        let mut node_index = 0_usize;
        loop {
            let first_child = first_child_index(node_index);
            if first_child >= size {
                break;
            }
            let mut min_child = first_child;
            let mut min_priority = self.nodes[first_child].1;
            let mut i = first_child + 1;
            let upper = core::cmp::min(first_child + ARITY, size);
            while i < upper {
                if self.nodes[i].1 < min_priority {
                    min_priority = self.nodes[i].1;
                    min_child = i;
                }
                i += 1;
            }
            if self.nodes[node_index].1 <= min_priority {
                break;
            }
            self.nodes.swap(node_index, min_child);
            node_index = min_child;
        }
        Some(root)
    }
}

#[cfg(test)]
mod tests {
    use super::NetPriorityQueue;

    /// 与 `dotnet`（net8.0.26）实跑逐位对齐（入队序 `c_n,n_n,…,c_1,n_1`；`c`=154、`n`=165）。
    #[test]
    fn matches_dotnet_priority_queue_dequeue_order() {
        // (宝宝数, oracle 实出队序标签)
        let cases: &[(usize, &[&str])] = &[
            (1, &["c1", "n1"]),
            (2, &["c2", "c1", "n1", "n2"]),
            (3, &["c3", "c2", "c1", "n2", "n1", "n3"]),
            (4, &["c4", "c1", "c3", "c2", "n2", "n3", "n4", "n1"]),
            (
                5,
                &["c5", "c2", "c1", "c4", "c3", "n3", "n5", "n4", "n2", "n1"],
            ),
            (
                6,
                &[
                    "c6", "c3", "c1", "c2", "c5", "c4", "n6", "n4", "n3", "n5", "n1", "n2",
                ],
            ),
            (
                7,
                &[
                    "c7", "c4", "c1", "c3", "c2", "c6", "c5", "n4", "n7", "n5", "n1", "n6", "n3",
                    "n2",
                ],
            ),
            (
                8,
                &[
                    "c8", "c5", "c4", "c7", "c2", "c3", "c1", "c6", "n1", "n5", "n8", "n6", "n4",
                    "n2", "n3", "n7",
                ],
            ),
            (
                9,
                &[
                    "c9", "c6", "c1", "c5", "c8", "c4", "c3", "c2", "c7", "n5", "n1", "n6", "n9",
                    "n7", "n8", "n4", "n3", "n2",
                ],
            ),
            (
                10,
                &[
                    "c10", "c7", "c1", "c6", "c2", "c9", "c5", "c4", "c3", "c8", "n3", "n6", "n1",
                    "n7", "n10", "n8", "n5", "n9", "n4", "n2",
                ],
            ),
            (
                11,
                &[
                    "c11", "c8", "c1", "c7", "c2", "c10", "c3", "c6", "c5", "c4", "c9", "n6", "n3",
                    "n7", "n1", "n8", "n11", "n9", "n4", "n5", "n10", "n2",
                ],
            ),
            (
                12,
                &[
                    "c12", "c9", "c1", "c8", "c2", "c11", "c3", "c7", "c4", "c6", "c5", "c10",
                    "n4", "n7", "n3", "n8", "n10", "n9", "n12", "n1", "n6", "n11", "n5", "n2",
                ],
            ),
        ];
        for (n, expected) in cases {
            let mut q: NetPriorityQueue<String> = NetPriorityQueue::new();
            for i in (1..=*n).rev() {
                q.enqueue(format!("c{i}"), 154);
                q.enqueue(format!("n{i}"), 165);
            }
            let mut got = Vec::new();
            while let Some(e) = q.dequeue() {
                got.push(e);
            }
            let expected: Vec<String> = expected.iter().map(|s| (*s).to_string()).collect();
            assert_eq!(&got, &expected, "n={n}：与 dotnet 实出队序不符");
        }
    }

    /// 同优先级**不是**入队序（.NET 4 叉堆的可见后果）—— 这正是本卡 reviewer-6 F-1 的根因：
    /// 入队 `c1,n1,c2,n2`（`c`=154、`n`=165）⇒ 名组出队序与入队序**相反**。
    #[test]
    fn ties_are_not_insertion_order() {
        let mut q: NetPriorityQueue<&str> = NetPriorityQueue::new();
        for e in [("c1", 154_u8), ("n1", 165), ("c2", 154), ("n2", 165)] {
            q.enqueue(e.0, e.1);
        }
        let mut got = Vec::new();
        while let Some(e) = q.dequeue() {
            got.push(e);
        }
        assert_eq!(got, vec!["c1", "c2", "n2", "n1"], "名组出队序与入队序相反");
    }
}
