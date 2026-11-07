//! `PlanesServer` 的纯状态机（G11 / A3）。
//!
//! 传输层（tokio 监听/收发）不在这里：本模块只复刻 oracle 的**槽位、缓冲、解码时机**
//! 三件事，方便无 socket 地测时序与边界。
//!
//! # 槽位语义（C# 探针实测，`E:/tmp/planes-id-probe`）
//!
//! `TouchSocket` 的 `SocketClient.Id` 是**进程内全局递增的十进制字符串**（`1`,`2`,`3`…），
//! 连接断开后**不复用**。oracle 的三条事件各用各的索引：
//!
//! - `Connecting`：填**第一个空槽**（`srvArray` 长 10）；
//! - `Received`：`srvArray[int.Parse(Id) - 1]`——**不核对槽里是谁**；
//! - `Disconnected`：按 `SocketId` 找到槽并置 `null`。
//!
//! ⇒ **首次断线之后**，新连接的 `Id-1` 多半指向空槽（或越界），oracle 在 `Received` 里
//! 抛 `NullReferenceException` / `IndexOutOfRangeException`；`TouchSocket` **静默吞掉**它
//! （探针实测：进程不崩、该连接的数据再也不进任何缓冲）。极端情况下 `Id-1` 指向的槽
//! 属于另一个连接时，数据会被**追加到别人的缓冲**并被当作那个连接的数据解码。
//!
//! 本模块**照抄**这个映射（[`Server::received`]），不"顺手修好"：行为等价优先。
//! 该缺陷已登记在 `docs/g11-planes-design-20261108.md`，是否修属 Owner 决策。

use crate::{message, Decoder, Flavor, Frame};

/// oracle `srvArray` 的固定长度。
pub const MAX_SLOTS: usize = 10;

#[derive(Debug)]
struct Slot {
    /// `TouchSocket` 的 `SocketClient.Id`（十进制字符串）。
    socket_id: String,
    decoder: Decoder,
}

/// `tick` 的产出，按发生顺序排列；跨服中继对每个帧**先于**本地处理。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// 转发给**除 `from` 外**所有已连接槽（C# `DecodeSocStr_SendOtherServer`）。
    /// `inner` 是括号之间的原始字节，发送侧要补回一对括号。
    Relay {
        /// 来源槽下标。
        from: usize,
        /// 原始内层字节。
        inner: Vec<u8>,
    },
    /// 本地处理成功的一帧（oracle `ProcessData` 无副作用，这里只是把帧交出去）。
    Frame {
        /// 来源槽下标。
        slot: usize,
        /// 解出的帧。
        frame: Frame,
    },
    /// oracle 在 `ProcessData` 里 `throw`（211）⇒ `DecodeSocStr` 的 `catch` 丢弃
    /// **本批剩余帧 + 缓冲余量**。此事件之后该槽的缓冲已清空。
    Aborted {
        /// 来源槽下标。
        slot: usize,
    },
}

/// 主服（`ServerIndex == 0`）的跨服状态机。
#[derive(Debug)]
pub struct Server {
    slots: [Option<Slot>; MAX_SLOTS],
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// 新建（10 个空槽）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }

    /// 对应 oracle `Connecting`：取第一个空槽；槽满返回 `None`（oracle 也静默不登记）。
    pub fn connected(&mut self, socket_id: &str) -> Option<usize> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(Slot {
                    socket_id: socket_id.to_owned(),
                    decoder: Decoder::new(Flavor::Server),
                });
                return Some(i);
            }
        }
        None
    }

    /// 对应 oracle `Disconnected`：按 `SocketId` 找槽并置空，返回被清空的槽。
    pub fn disconnected(&mut self, socket_id: &str) -> Option<usize> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.as_ref().is_some_and(|s| s.socket_id == socket_id) {
                *slot = None;
                return Some(i);
            }
        }
        None
    }

    /// 对应 oracle `Received`：**按 `socket_id` 的数字取 `slots[id - 1]`**。
    ///
    /// 槽空 / 下标越界 / `socket_id` 不是正整数 ⇒ 数据丢弃并返回 `false`
    /// （oracle 抛异常后被 `TouchSocket` 吞掉，效果相同）。**不核对归属**，与 oracle 一致。
    pub fn received(&mut self, socket_id: &str, chunk: &[u8]) -> bool {
        let Ok(id) = socket_id.parse::<usize>() else {
            return false;
        };
        let Some(idx) = id.checked_sub(1) else {
            return false;
        };
        let Some(slot) = self.slots.get_mut(idx).and_then(Option::as_mut) else {
            return false;
        };
        slot.decoder.push(chunk);
        true
    }

    /// 对应 oracle `Run()`：按槽序逐槽解码，每帧先中继再本地处理。
    ///
    /// 单槽遇到 [`message::ProcessError`] 时：先给出该帧的 [`Event::Relay`]，
    /// 清空缓冲余量，给出 [`Event::Aborted`]，然后继续下一个槽（oracle 的 catch 在
    /// 单槽 `DecodeSocStr` 内部，不影响其他槽）。
    pub fn tick(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let Some(slot) = slot.as_mut() else {
                continue;
            };
            for (inner, frame) in slot.decoder.decode_raw() {
                out.push(Event::Relay { from: i, inner });
                if message::process(frame.ident).is_err() {
                    slot.decoder.clear();
                    out.push(Event::Aborted { slot: i });
                    break;
                }
                out.push(Event::Frame { slot: i, frame });
            }
        }
        out
    }

    /// 某槽当前的 `SocketId`（未占用为 `None`）。
    #[must_use]
    pub fn socket_id(&self, slot: usize) -> Option<&str> {
        self.slots
            .get(slot)
            .and_then(Option::as_ref)
            .map(|s| s.socket_id.as_str())
    }

    /// 已占用槽数。
    #[must_use]
    pub fn connected_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr(ident: i32, server_num: i32, body: &[u8]) -> Frame {
        Frame {
            ident,
            server_num,
            body: body.to_vec(),
        }
    }

    #[test]
    fn connected_fills_first_free_slot_and_caps_at_ten() {
        let mut s = Server::new();
        for i in 1..=MAX_SLOTS {
            assert_eq!(s.connected(&i.to_string()), Some(i - 1));
        }
        assert_eq!(s.connected("11"), None, "第 11 个连接 oracle 不登记");
        assert_eq!(s.connected_count(), MAX_SLOTS);
        // 空出中间槽后，新连接填进去（与 oracle `Connecting` 一致）
        assert_eq!(s.disconnected("3"), Some(2));
        assert_eq!(s.connected("12"), Some(2));
        assert_eq!(s.socket_id(2), Some("12"));
    }

    #[test]
    fn received_maps_by_numeric_id_not_by_slot_order() {
        let mut s = Server::new();
        s.connected("1");
        s.connected("2");
        // 两个连接都在，按 id 各归各槽
        assert!(s.received("2", b"(201/9/b)"));
        assert!(s.received("1", b"(202/8/a)"));
        let ev = s.tick();
        assert_eq!(
            ev,
            vec![
                Event::Relay {
                    from: 0,
                    inner: b"202/8/a".to_vec()
                },
                Event::Frame {
                    slot: 0,
                    frame: fr(202, 8, b"a")
                },
                Event::Relay {
                    from: 1,
                    inner: b"201/9/b".to_vec()
                },
                Event::Frame {
                    slot: 1,
                    frame: fr(201, 9, b"b")
                },
            ],
            "槽序 0→1，每帧先 Relay 后 Frame"
        );
    }

    #[test]
    fn reconnect_quirk_data_is_dropped_after_first_disconnect() {
        // 探针实测的 oracle 行为：id 全局递增不复用，断开后新连接的 id-1 指向空槽 ⇒ 数据丢弃。
        let mut s = Server::new();
        assert_eq!(s.connected("1"), Some(0));
        assert!(s.received("1", b"(201/9/a)"));
        assert_eq!(s.disconnected("1"), Some(0));
        // 新连接拿到空槽 0，但它的 id=2 ⇒ Received 走 slots[1]，是空的
        assert_eq!(s.connected("2"), Some(0));
        assert!(!s.received("2", b"(201/9/b)"), "oracle 在这里抛 NRE 并被吞");
        assert_eq!(s.tick(), Vec::new(), "数据从未进缓冲");
    }

    #[test]
    fn received_rejects_bad_ids() {
        let mut s = Server::new();
        s.connected("1");
        assert!(!s.received("abc", b"x"), "非数字 id");
        assert!(!s.received("", b"x"), "空 id");
        assert!(!s.received("0", b"x"), "id=0 会让下标 -1");
        assert!(!s.received("11", b"x"), "越界（oracle IndexOutOfRange）");
        assert!(!s.received("999999999999999999999999", b"x"), "溢出");
        assert!(s.received("1", b"x"));
    }

    #[test]
    fn cross_slot_append_is_reproduced() {
        // 探针推出的极端形态：slots[id-1] 属于另一个连接时，数据被追加到别人的缓冲。
        let mut s = Server::new();
        s.connected("1"); // slot0 = id1
        s.connected("2"); // slot1 = id2
        s.disconnected("1"); // slot0 空
        s.connected("3"); // slot0 = id3
        s.connected("4"); // slot1 已空? 不：slot1 仍被 id2 占着 ⇒ 新连接进 slot2
        assert_eq!(s.socket_id(1), Some("2"));
        assert_eq!(s.socket_id(2), Some("4"));
        // id3 的数据按 id-1 落到 slot2（属于 id4）
        assert!(s.received("3", b"(201/9/from3)"));
        let ev = s.tick();
        assert_eq!(
            ev,
            vec![
                Event::Relay {
                    from: 2,
                    inner: b"201/9/from3".to_vec()
                },
                Event::Frame {
                    slot: 2,
                    frame: fr(201, 9, b"from3")
                },
            ],
            "oracle 会把它当作 slot2（id4）的数据"
        );
        assert_eq!(s.socket_id(2), Some("4"));
    }

    #[test]
    fn throw_aborts_batch_and_clears_remainder() {
        let mut s = Server::new();
        s.connected("1");
        // 一批里 211 在前、201 在后，且尾部还有半帧
        s.received("1", b"(211/0/x)(201/1/y)(202/2/z");
        let ev = s.tick();
        assert_eq!(
            ev,
            vec![
                Event::Relay {
                    from: 0,
                    inner: b"211/0/x".to_vec()
                },
                Event::Aborted { slot: 0 },
            ],
            "211 中继先发出，然后整批剩余（201）与半帧（202）一起丢弃"
        );
        // 缓冲已被清空：后续补齐的字节不会复活那半帧
        s.received("1", b")");
        assert_eq!(s.tick(), Vec::new());

        // 对照组：没有 211 时同形态输入逐帧都处理
        let mut s2 = Server::new();
        s2.connected("1");
        s2.received("1", b"(201/1/y)(202/2/z)(203/3/w)");
        let ev2 = s2.tick();
        assert_eq!(ev2.len(), 6, "3 帧 × (Relay + Frame)");
    }

    #[test]
    fn partial_frame_waits_for_next_tick() {
        let mut s = Server::new();
        s.connected("1");
        assert!(s.received("1", b"(201/9/al"));
        assert_eq!(s.tick(), Vec::new(), "未闭合不下发");
        assert!(s.received("1", b"ice)"));
        assert_eq!(
            s.tick(),
            vec![
                Event::Relay {
                    from: 0,
                    inner: b"201/9/alice".to_vec()
                },
                Event::Frame {
                    slot: 0,
                    frame: fr(201, 9, b"alice")
                },
            ]
        );
    }
}
