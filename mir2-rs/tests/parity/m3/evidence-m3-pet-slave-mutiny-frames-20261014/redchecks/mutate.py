"""改动必红（RC1..RC5）——rework: reviewer-6 F-1（同优先级出队序）。

用法（cwd=mir2-rs，隔离 CARGO_TARGET_DIR）：
    python <本文件> rc1 && cargo test -p game-svr --lib sm_changenamecolor_header_only
    python <本文件> rc2 && cargo test -p game-svr --bin game-svr slave_mutiny_downlink_follows_msgqueue_heap_order
    python <本文件> rc3 && cargo test -p game-svr --bin game-svr slave_mutiny
    python <本文件> rc4 && cargo test -p game-svr --lib msgqueue
    python <本文件> rc5 && cargo test -p game-svr --bin game-svr slave_mutiny
每次跑完用 `git checkout -- <文件>` 还原。
"""
import io
import sys

MAIN = "bins/game-svr/src/main.rs"
SMF = "bins/game-svr/src/smframe.rs"
MQ = "bins/game-svr/src/msgqueue.rs"


def patch(path, old, new):
    s = io.open(path, encoding="utf-8").read()
    assert s.count(old) == 1, (path, s.count(old))
    io.open(path, "w", encoding="utf-8", newline="").write(s.replace(old, new))


# 当前（正确）drain 里的队列块。
QUEUE_BLOCK = """        let mut queue: game_svr::msgqueue::NetPriorityQueue<MutinyFrame> =
            game_svr::msgqueue::NetPriorityQueue::new();
        for h in hits.iter().filter(|h| h.0 == observer) {
            queue.enqueue(
                MutinyFrame::NameColor {
                    sid: h.1,
                    color: h.2,
                },
                (mir2_protocol::messages::RM_CHANGENAMECOLOR & 0xFF) as u8,
            );
            queue.enqueue(
                MutinyFrame::UserName {
                    sid: h.1,
                    color: h.2,
                    show_name: h.3.clone(),
                },
                (mir2_protocol::messages::RM_USERNAME & 0xFF) as u8,
            );
        }
        while let Some(frame) = queue.dequeue() {
            let body = match frame {
                MutinyFrame::NameColor { sid, color } => {
                    game_svr::smframe::sm_changenamecolor(sid, color)
                }
                MutinyFrame::UserName {
                    sid,
                    color,
                    show_name,
                } => game_svr::smframe::sm_username(sid, color, &show_name),
            };
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body,
            });
            sent += 1;
        }
"""

# 旧写法（F-1 前）：所有色 → 所有名（同优先级按入队序）⇒ 名帧序与 oracle 相反。
STABLE_BLOCK = """        for h in hits.iter().filter(|h| h.0 == observer) {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: game_svr::smframe::sm_changenamecolor(h.1, h.2),
            });
            sent += 1;
        }
        for h in hits.iter().filter(|h| h.0 == observer) {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: game_svr::smframe::sm_username(h.1, h.2, &h.3),
            });
            sent += 1;
        }
"""

mode = sys.argv[1]
if mode == "rc1":
    # `SM_CHANGENAMECOLOR` 的 `Param = GetChrColor` 被清零。
    patch(
        SMF,
        "messages::SM_CHANGENAMECOLOR,\n        u16::from(name_color),",
        "messages::SM_CHANGENAMECOLOR,\n        0,",
    )
elif mode == "rc2":
    # 退回 F-1 的稳定两循环（同优先级 = 入队序）⇒ 名帧序反。
    patch(MAIN, QUEUE_BLOCK, STABLE_BLOCK)
elif mode == "rc3":
    # 丢掉名帧（只入队色帧）。
    patch(
        MAIN,
        """            queue.enqueue(
                MutinyFrame::UserName {
                    sid: h.1,
                    color: h.2,
                    show_name: h.3.clone(),
                },
                (mir2_protocol::messages::RM_USERNAME & 0xFF) as u8,
            );
""",
        "",
    )
elif mode == "rc4":
    # .NET 是 4 叉堆（Arity=4/Log2Arity=2）；改成二叉 ⇒ 同优先级出队序错。
    patch(MQ, "const LOG2_ARITY: u32 = 2;", "const LOG2_ARITY: u32 = 1;")
elif mode == "rc5":
    # 色帧优先级抬到 255（> 名 165）⇒ 出队序按优先级分组反过来。
    patch(
        MAIN,
        "(mir2_protocol::messages::RM_CHANGENAMECOLOR & 0xFF) as u8,",
        "255_u8,",
    )
else:
    raise SystemExit("bad mode")
print("mutated", mode)
