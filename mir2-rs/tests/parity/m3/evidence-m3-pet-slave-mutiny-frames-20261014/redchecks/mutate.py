import sys, io
mode = sys.argv[1]
main = "bins/game-svr/src/main.rs"
smf  = "bins/game-svr/src/smframe.rs"
def patch(path, old, new):
    s = io.open(path, encoding="utf-8").read()
    assert s.count(old) == 1, (path, s.count(old))
    io.open(path, "w", encoding="utf-8", newline="").write(s.replace(old, new))
if mode == "m1":
    patch(smf,
      "messages::SM_CHANGENAMECOLOR,\n        u16::from(name_color),",
      "messages::SM_CHANGENAMECOLOR,\n        0,")
elif mode == "m2":
    old = """        // ① 色帧（优先级字节 154，`mutinied` 序）。
        for h in hits.iter().filter(|h| h.0 == observer) {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: game_svr::smframe::sm_changenamecolor(h.1, h.2),
            });
            sent += 1;
        }
        // ② 名帧（优先级字节 165，同序）。
        for h in hits.iter().filter(|h| h.0 == observer) {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: game_svr::smframe::sm_username(h.1, h.2, &h.3),
            });
            sent += 1;
        }"""
    new = """        for h in hits.iter().filter(|h| h.0 == observer) {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: game_svr::smframe::sm_changenamecolor(h.1, h.2),
            });
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: game_svr::smframe::sm_username(h.1, h.2, &h.3),
            });
            sent += 2;
        }"""
    patch(main, old, new)
elif mode == "m3":
    old = """        // ② 名帧（优先级字节 165，同序）。
        for h in hits.iter().filter(|h| h.0 == observer) {
            let _ = tx.send(SimToGate::SendFrame {
                socket: entry.socket,
                body: game_svr::smframe::sm_username(h.1, h.2, &h.3),
            });
            sent += 1;
        }
"""
    patch(main, old, "")
else:
    raise SystemExit("bad mode")
print("mutated", mode)
