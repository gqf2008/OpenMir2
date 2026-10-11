#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""probe_m1_chain.py — M1⑤②③ 的**协议级**假人：N 个并发连接走完整「登录 → 选服 → 选角」链。

为什么需要它（与 BotSrv 的分工）：
  BotSrv 的 `LoadMetrics.login_ok` 是本工程已知会**漏计数**的探针（C6 卡
  `openmir2-botsrv-world-evidence` / T-8/T-9：登录响应会被假人侧的跨假人共享态静默丢弃，
  实测 200 档稳定只到 170）。所以「链本身能不能吃住 200 并发」必须有一个**不经过 BotSrv** 的
  独立判据。本脚本直接用 C# oracle 的 EDCode 帧口径打真实边界链：

    LoginGate(7000): CM_IDPASSWORD   → 期望 529/531（登录成功）
    LoginGate(7000): CM_SELECTSERVER → 期望 530，体 `ip/port/<session>`（SelGate 地址）
    SelGate (port):  CM_QUERYCHR     → 期望 520（角色列表；体 `*名/职业/发型/等级/性别/…`）
    SelGate (port):  CM_SELCHR       → 期望 525（SM_STARTPLAY = 选角成功）

  每个连接一条独立 TCP，错峰 `stagger_ms`（压测口径的一部分，报告里要记）。

用法：
  python tools/m1-acceptance/probe_m1_chain.py --start 0 --count 200 --stagger-ms 20 --port 7000 \
      --out E:\tmp\openmir2-m1load\evidence\probe-rust.json
退出码：全体四步都成功 ⇒ 0；否则 1（并打印失败清单）。
"""
import argparse
import json
import socket
import struct
import sys
import threading
import time
from collections import Counter

# ==== EDCode（与 C# oracle / BotSrv 同一口径；从 mixed_probe.py 收敛进本脚本，自包含）====
SEED = 0xAC
BASE = 0x3C


def enc(src: bytes) -> bytes:
    out = bytearray()
    no = 2
    rem = 0
    for b in src:
        c = b ^ SEED
        if no == 6:
            out.append(((c & 0x3F) + BASE) & 0xFF)
            rem |= (c >> 2) & 0x30
            out.append((rem + BASE) & 0xFF)
            rem = 0
        else:
            t = c >> 2
            out.append((((t & 0x3C) | (c & 3)) + BASE) & 0xFF)
            rem = ((rem << 2) | (t & 3)) & 0xFF
        no = no % 6 + 2
    if no != 2:
        out.append((rem + BASE) & 0xFF)
    return bytes(out)


def dec(src: bytes) -> bytes:
    out = bytearray()
    n = len(src)
    cyc = n // 4
    left = n % 4
    m = lambda x: (x - BASE) & 0xFF  # noqa: E731
    for i in range(cyc):
        b = i * 4
        r = m(src[b + 3])
        t = m(src[b])
        out.append(((((t << 2) & 0xF0) | (r & 0x0C) | (t & 3)) ^ SEED))
        t = m(src[b + 1])
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED))
        t = m(src[b + 2])
        out.append(((t | ((r << 2) & 0xC0)) ^ SEED))
    if left == 2:
        r = m(src[n - 1])
        t = m(src[n - 2])
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED))
    elif left == 3:
        r = m(src[n - 1])
        t = m(src[n - 3])
        out.append(((((t << 2) & 0xF0) | (r & 0x0C) | (t & 3)) ^ SEED))
        t = m(src[n - 2])
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED))
    return bytes(out)


def frame(ident: int, body: bytes = b"", recog: int = 0) -> bytes:
    head = struct.pack("<iHHHH", recog, ident, 0, 0, 0)
    out = b"#1" + enc(head)
    if body:
        out += enc(body)
    return out + b"!"


def read_frames(sock, timeout=8.0):
    """读 s2c 帧（`#…!` / `#…!$`），返回 [(tail, ident, recog, series, body_plain)]。"""
    sock.settimeout(timeout)
    buf = b""
    out = []
    end = time.time() + timeout
    while time.time() < end:
        try:
            c = sock.recv(65536)
        except socket.timeout:
            break
        except OSError:
            break
        if not c:
            break
        buf += c
        while True:
            if not buf.startswith(b"#"):
                i = buf.find(b"#")
                if i < 0:
                    buf = b""
                    break
                buf = buf[i:]
            j = buf.find(b"!")
            if j < 0:
                break
            tail = b"!$" if buf[j:j + 2] == b"!$" else b"!"
            raw = buf[:j + len(tail)]
            buf = buf[j + len(tail):]
            inner = raw[2:len(raw) - len(tail)] if raw[1:2] == b"1" else raw[1:len(raw) - len(tail)]
            head = dec(inner[:16])
            ident = struct.unpack_from("<H", head, 4)[0]
            recog = struct.unpack_from("<i", head, 0)[0]
            series = struct.unpack_from("<H", head, 10)[0]
            try:
                body = dec(inner[16:]).decode("gbk", "replace")
            except Exception:
                body = None
            out.append((tail.decode(), ident, recog, series, body))
    return out


def first_char_name(body: str):
    """角色列表体：`*名/职业/发型/等级/性别/…`（可多条）。取第一条的名字（去 `*`）。"""
    if not body:
        return None
    fields = body.split("/")
    for k in range(0, len(fields) - 4, 5):
        name = fields[k]
        level = fields[k + 3]
        if name and level:
            return name[1:] if name.startswith("*") else name
    return None


def one(i: int, args, results: dict, lock: threading.Lock):
    acct = f"{args.prefix}{i}"
    time.sleep(i * args.stagger_ms / 1000.0)
    r = {"acct": acct, "login": False, "select_server": False, "query_chr": False,
         "select_chr": False, "session": None, "chr": None, "idents": [], "err": None}
    try:
        s = socket.create_connection(("127.0.0.1", args.port), 5)
        s.sendall(frame(2001, f"{acct}/{acct}".encode()))
        f1 = read_frames(s)
        r["idents"] += [x[1] for x in f1]
        r["login"] = any(x[1] in (529, 531) for x in f1)
        if not r["login"]:
            s.close()
            with lock:
                results[i] = r
            return
        s.sendall(frame(104, args.server.encode("gbk")))
        f2 = read_frames(s)
        r["idents"] += [x[1] for x in f2]
        sel = next((x for x in f2 if x[1] == 530), None)
        r["select_server"] = sel is not None
        s.close()
        if not sel or not sel[4]:
            with lock:
                results[i] = r
            return
        parts = sel[4].split("/")
        ip, sport = parts[0], int(parts[1])
        r["session"] = parts[2] if len(parts) > 2 else None
        sid = socket.create_connection((ip, sport), 5)
        sid.sendall(frame(100, f"{acct}/{r['session']}".encode()))
        f3 = read_frames(sid)
        r["idents"] += [x[1] for x in f3]
        q = next((x for x in f3 if x[1] == 520), None)
        r["query_chr"] = q is not None
        if not q:
            sid.close()
            with lock:
                results[i] = r
            return
        chrname = first_char_name(q[4])
        r["chr"] = chrname
        if not chrname:
            sid.close()
            with lock:
                results[i] = r
            return
        sid.sendall(frame(103, f"{acct}/{chrname}".encode()))
        f4 = read_frames(sid)
        r["idents"] += [x[1] for x in f4]
        r["select_chr"] = any(x[1] == 525 for x in f4)
        sid.close()
    except Exception as e:  # noqa: BLE001
        r["err"] = repr(e)
    with lock:
        results[i] = r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--start", type=int, default=0)
    ap.add_argument("--count", type=int, default=200)
    ap.add_argument("--stagger-ms", type=int, default=20)
    ap.add_argument("--port", type=int, default=7000)
    ap.add_argument("--server", default="热血传奇")
    ap.add_argument("--prefix", default="loadbot")
    ap.add_argument("--out", default="")
    args = ap.parse_args()

    results = {}
    lock = threading.Lock()
    ths = [threading.Thread(target=one, args=(args.start + k, args, results, lock)) for k in range(args.count)]
    t0 = time.time()
    for t in ths:
        t.start()
    for t in ths:
        t.join()
    dt = time.time() - t0

    steps = ["login", "select_server", "query_chr", "select_chr"]
    summary = {k: sum(1 for r in results.values() if r[k]) for k in steps}
    fails = [r["acct"] for r in results.values() if not r["select_chr"]]
    errs = [{"acct": r["acct"], "err": r["err"], "idents": r["idents"]} for r in results.values() if r["err"]]
    print(f"probe_m1_chain: start={args.prefix}{args.start} count={args.count} stagger={args.stagger_ms}ms "
          f"port={args.port} elapsed={dt:.1f}s")
    for k in steps:
        print(f"  {k:14s} = {summary[k]}/{args.count} ({100.0 * summary[k] / args.count:.1f}%)")
    ident_hist = Counter(x for r in results.values() for x in r["idents"])
    print("  ident_hist:", dict(sorted(ident_hist.items())))
    if fails:
        print(f"  FAIL({len(fails)}):", fails[:30])
    for e in errs[:10]:
        print("  ERR:", e)

    report = {
        "tool": "m1-acceptance/probe_m1_chain.py",
        "count": args.count, "stagger_ms": args.stagger_ms, "port": args.port, "elapsed_s": round(dt, 1),
        "steps": summary, "ident_hist": {str(k): v for k, v in ident_hist.items()},
        "fails": fails, "errors": errs, "pass": len(fails) == 0,
    }
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            json.dump(report, f, ensure_ascii=False, indent=1)
        print("  EVIDENCE", args.out)
    sys.exit(0 if not fails else 1)


if __name__ == "__main__":
    main()
