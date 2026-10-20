#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gate_timer_gate.py — 网关 timer gate：从抓包时间戳判 60s 两阶段超时 / 10s 重连（C10#4）。

输入 = `gate_shadow_proxy.py`（或 `mir2_proxy.py`）的 capture 目录
（`chunks.ndjson` + `conn-*.bin` + 可选 `session.json`）。**不读日志、不起服务端**；
时间全部来自 `chunks.ndjson` 的 `t_wall_ms`，帧时间 = 完成该帧的 TCP 读块时间。

判据（与 oracle 源码逐行对齐，见各 mode 注释）：

  login-idle    LoginGate 60s 空闲：计时从**会话构造**起，收到任何客户端数据都**不刷新**
                （`ClientSession.cs` 的刷新点全被注释）；扫描 >1000ms；严格 `>`；
                第一阶段发 `SM_OUTOFCONNECTION`（16B 截断缺陷帧）后，`TimedService`
                在**同一次扫描**立即关会话（第二阶段线上不可达）。
  sel-idle      SelGate 60s 空闲：计时从会话构造起，**只**被 CM_QUERYCHR/NEWCHR/DELCHR/
                SELCHR（100..103）刷新；扫描 >20s；严格 `>`；超时发 18B 缺陷帧后
                **socket 保持打开**（SessionManager 只摘槽、不关 socket；第二阶段不可达）。
  sel-reconnect SelGate 下行 10s 重连：断链后每个 10s tick 才重连一次
                （`CheckServerTimeOutTime=10s`）⇒ 相邻 open 事件的间隔应 ≈10s。
  sel-keepalive SelGate 就绪态每 10s 发一条 `ServerDataMessage{Type=KeepAlive}`
                ⇒ 下行 capture 里 KeepAlive 包的间隔应 ≈10s。

**无对应帧 ⇒ UNCLOSED（rc=3），绝不因「没采到」判 GREEN。**
退出码：0 GREEN / 1 RED / 2 PREREQ_MISSING / 3 UNCLOSED。
`--selftest` 用合成 capture 逐条证明「会绿 / 会红 / 没观测物会 UNCLOSED」，不依赖外部文件。
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

LOGIN_TIMEOUT_MS = 60_000
LOGIN_SCAN_MS = 1_000
SEL_TIMEOUT_MS = 60_000
SEL_SCAN_MS = 20_000
RECONNECT_TICK_MS = 10_000
SEL_REFRESH_IDS = (100, 101, 102, 103)
SM_OUTOFCONNECTION = 528

STATUS_RANK = {"GREEN": 0, "UNCLOSED": 1, "RED": 2}


def load_common():
    path = os.path.join(HERE, "gwcap_common.py")
    spec = importlib.util.spec_from_file_location("_gwcap_timer_common", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _result(mode, checks, unclosed_reason=None, facts=None):
    if any(not c["ok"] for c in checks):
        status = "RED"
    elif unclosed_reason:
        status = "UNCLOSED"
    else:
        status = "GREEN"
    return {
        "mode": mode,
        "status": status,
        "checks": checks,
        "unclosed_reason": unclosed_reason,
        "facts": facts or {},
    }


def _mkcheck(name, ok, detail):
    return {"name": name, "ok": bool(ok), "detail": detail}


def _select_conns(cap, want_port, explicit_port=None):
    """按 session.json 映射的上游端口（7000/7100/5100）或显式监听端口选连接。"""
    target = explicit_port if explicit_port is not None else want_port
    out = []
    for conn in cap.conns():
        if cap.gate_port(conn) == target or cap.listen_port(conn) == target:
            out.append(conn)
    return out


def eval_login_idle(cap, args):
    conns = _select_conns(cap, 7000, args.port)
    if not conns:
        return _result("login-idle", [], "capture 里没有映射到 LoginGate 7000 的连接")
    checks = []
    facts = {"conns": [], "phase1": "16B = '#' + 15 EDCode chars (truncated defect)",
             "phase2_online": False,
             "oracle": "TimedService closes the session in the same >1000ms scan"}
    found = False
    missing_close = []
    for conn in conns:
        t_open = cap.open_time(conn)
        frames = [
            f for f in cap.frames(conn, "s2c")
            if f.get("wire") == "login-timeout-16"
        ]
        if not frames:
            facts["conns"].append({"conn": conn, "open_ms": t_open, "timeout_frame": None})
            continue
        found = True
        frame = frames[0]
        elapsed = frame["t_wall_ms"] - t_open
        lo = elapsed > args.timeout_ms
        hi = elapsed <= args.timeout_ms + args.login_scan_ms + args.tolerance_ms
        checks.append(_mkcheck(
            "conn%d.strictly-after-60s" % conn, lo,
            "elapsed=%dms，判据 elapsed > %dms（严格）" % (elapsed, args.timeout_ms),
        ))
        checks.append(_mkcheck(
            "conn%d.within-one-scan" % conn, hi,
            "elapsed=%dms，上界 %dms（60s + >1000ms 扫描 + %dms 容差）"
            % (elapsed, args.timeout_ms + args.login_scan_ms + args.tolerance_ms,
               args.tolerance_ms),
        ))
        close_t = cap.close_time(conn)
        if close_t is None:
            missing_close.append(conn)
            checks.append(_mkcheck(
                "conn%d.closed-same-scan" % conn, True,
                "未采到 close 事件（抓包可能已结束）——本项不判绿，整体记 UNCLOSED",
            ))
        else:
            gap = close_t - frame["t_wall_ms"]
            checks.append(_mkcheck(
                "conn%d.closed-same-scan" % conn,
                0 <= gap <= args.login_scan_ms + args.tolerance_ms,
                "frame→close=%dms，上界 %dms（同一次扫描关会话）"
                % (gap, args.login_scan_ms + args.tolerance_ms),
            ))
        facts["conns"].append({
            "conn": conn, "open_ms": t_open,
            "timeout_frame_ms": frame["t_wall_ms"],
            "elapsed_ms": elapsed, "close_ms": close_t,
            "frame_len": frame["len"], "frame_ident": frame.get("ident"),
        })
    if not found:
        return _result("login-idle", [], "没有 16B LoginGate 超时帧（'#' + 15 字符）——无观测物", facts)
    reason = None
    if missing_close:
        reason = "连接 %s 缺 close 事件：超时帧已见，但无法证明同一次扫描关会话" % missing_close
    return _result("login-idle", checks, reason, facts)


def eval_sel_idle(cap, args):
    conns = _select_conns(cap, 7100, args.port)
    if not conns:
        return _result("sel-idle", [], "capture 里没有映射到 SelGate 7100 的连接")
    checks = []
    facts = {"conns": [], "refresh_idents": list(SEL_REFRESH_IDS),
             "phase1": "18B = EDCode(12B)+0x00+'!' (no '#' lead)",
             "phase2_online": False,
             "oracle": "SessionManager only TryRemove; socket stays open"}
    found = False
    for conn in conns:
        t_open = cap.open_time(conn)
        candidates = [
            f for f in cap.frames(conn, "s2c")
            if f.get("wire") == "sel-timeout-18"
            and (f.get("ident") in (None, SM_OUTOFCONNECTION))
        ]
        if not candidates:
            facts["conns"].append({"conn": conn, "open_ms": t_open, "timeout_frame": None})
            continue
        found = True
        frame = candidates[0]
        refresh = [
            f for f in cap.frames(conn, "c2s")
            if f.get("ident") in SEL_REFRESH_IDS and f["t_wall_ms"] <= frame["t_wall_ms"]
        ]
        anchor = t_open
        anchor_src = "open"
        if refresh:
            last = max(refresh, key=lambda f: f["t_wall_ms"])
            if last["t_wall_ms"] > anchor:
                anchor = last["t_wall_ms"]
                anchor_src = "refresh-ident-%d" % last["ident"]
        elapsed = frame["t_wall_ms"] - anchor
        lo = elapsed > args.timeout_ms
        hi = elapsed <= args.timeout_ms + args.sel_scan_ms + args.tolerance_ms
        checks.append(_mkcheck(
            "conn%d.strictly-after-60s" % conn, lo,
            "elapsed=%dms（锚点=%s），判据 > %dms（严格）"
            % (elapsed, anchor_src, args.timeout_ms),
        ))
        checks.append(_mkcheck(
            "conn%d.within-one-scan" % conn, hi,
            "elapsed=%dms，上界 %dms（60s + >20s 扫描 + %dms 容差）"
            % (elapsed, args.timeout_ms + args.sel_scan_ms + args.tolerance_ms,
               args.tolerance_ms),
        ))
        close_t = cap.close_time(conn)
        if close_t is not None and close_t - frame["t_wall_ms"] <= args.keep_open_ms:
            checks.append(_mkcheck(
                "conn%d.socket-stays-open" % conn, False,
                "frame 后 %dms 就 close —— 违反 SelGate 只摘槽不关 socket"
                % (close_t - frame["t_wall_ms"]),
            ))
        else:
            checks.append(_mkcheck(
                "conn%d.socket-stays-open" % conn, True,
                "frame 后未见近距 close（%s）"
                % ("close at +%dms" % (close_t - frame["t_wall_ms"]) if close_t else "无 close 事件"),
            ))
        facts["conns"].append({
            "conn": conn, "open_ms": t_open, "anchor_ms": anchor,
            "anchor_source": anchor_src,
            "refresh_count": len(refresh),
            "timeout_frame_ms": frame["t_wall_ms"],
            "elapsed_ms": elapsed, "close_ms": close_t,
            "frame_len": frame["len"], "frame_ident": frame.get("ident"),
        })
    if not found:
        return _result("sel-idle", [], "没有 18B SelGate 超时帧（ident 528 / 0x00+'!'）——无观测物", facts)
    return _result("sel-idle", checks, None, facts)


def eval_sel_reconnect(cap, args):
    conns = _select_conns(cap, 5100, args.port)
    if not conns:
        return _result("sel-reconnect", [],
                       "capture 里没有映射到 DBSrv 5100 的下行连接")
    opens = sorted(
        [(cap.open_time(c), c) for c in conns if cap.open_time(c) is not None]
    )
    facts = {"opens": [{"conn": c, "ms": t} for t, c in opens],
             "pairs": [], "dir_note": "proxy 位于 SelGate→DBSrv 路径时，open=每次重连尝试"}
    if len(opens) < 2:
        return _result("sel-reconnect", [],
                       "只有 %d 次 open；至少 2 次（一次断链 + 一次重连）才能判 10s 节拍"
                       % len(opens), facts)
    pairs = []
    for (t1, c1), (t2, c2) in zip(opens, opens[1:]):
        end = cap.end_time(c1)
        if end is None or not (t1 <= end <= t2):
            continue  # 两条并存连接，不是重连
        pairs.append((t2 - t1, c1, c2))
        facts["pairs"].append({"from_conn": c1, "to_conn": c2,
                               "gap_ms": t2 - t1, "prev_end_ms": end})
    if not pairs:
        return _result("sel-reconnect", [],
                       "没有「前一条连接已结束 → 下一次 open」的配对——无重连观测物", facts)
    checks = []
    for gap, c1, c2 in pairs:
        ok = args.min_gap_ms < gap <= args.max_gap_ms
        checks.append(_mkcheck(
            "conn%d->conn%d.reconnect-gap" % (c1, c2), ok,
            "gap=%dms，判据 %dms < gap <= %dms（10s tick + 容差）"
            % (gap, args.min_gap_ms, args.max_gap_ms),
        ))
    return _result("sel-reconnect", checks, None, facts)


def eval_sel_keepalive(cap, args):
    conns = _select_conns(cap, 5100, args.port)
    if not conns:
        return _result("sel-keepalive", [],
                       "capture 里没有映射到 DBSrv 5100 的下行连接")
    checks = []
    facts = {"conns": [], "direction": args.dir,
             "keepalive_rule": "ServerDataMessage{Type=KeepAlive(3)} 每 10s tick"}
    found = False
    for conn in conns:
        packets = [p for p in cap.packets(conn, args.dir) if p.get("kind") == 3]
        times = [p["t_wall_ms"] for p in packets]
        facts["conns"].append({"conn": conn, "keepalive_count": len(times),
                               "times_ms": times[:20]})
        if len(times) < 2:
            continue
        found = True
        for i in range(1, len(times)):
            gap = times[i] - times[i - 1]
            ok = args.min_gap_ms < gap <= args.max_gap_ms
            checks.append(_mkcheck(
                "conn%d.keepalive-%d" % (conn, i), ok,
                "gap=%dms，判据 %dms < gap <= %dms"
                % (gap, args.min_gap_ms, args.max_gap_ms),
            ))
    if not found:
        return _result("sel-keepalive", [],
                       "没有 ≥2 条 KeepAlive 帧——无 10s 心跳观测物", facts)
    return _result("sel-keepalive", checks, None, facts)


def _has_keepalive(cap, args):
    for conn in _select_conns(cap, 5100, args.port):
        if any(p.get("kind") == 3 for p in cap.packets(conn, args.dir)):
            return True
    return False


def _reconnect_pairs(cap, args):
    conns = _select_conns(cap, 5100, args.port)
    opens = sorted(
        [(cap.open_time(c), c) for c in conns if cap.open_time(c) is not None]
    )
    pairs = 0
    for (t1, c1), (t2, _c2) in zip(opens, opens[1:]):
        end = cap.end_time(c1)
        if end is not None and t1 <= end <= t2:
            pairs += 1
    return pairs


def run_modes(cap, args):
    funcs = {
        "login-idle": eval_login_idle,
        "sel-idle": eval_sel_idle,
        "sel-reconnect": eval_sel_reconnect,
        "sel-keepalive": eval_sel_keepalive,
    }
    if args.mode == "all":
        names = list(funcs)
    elif args.mode == "auto":
        names = []
        if _select_conns(cap, 7000, args.port):
            names.append("login-idle")
        if _select_conns(cap, 7100, args.port):
            names.append("sel-idle")
        if _select_conns(cap, 5100, args.port):
            if _reconnect_pairs(cap, args) > 0:
                names.append("sel-reconnect")
            elif _has_keepalive(cap, args):
                names.append("sel-keepalive")
            else:
                names.extend(["sel-reconnect", "sel-keepalive"])
        if not names:
            return [_result("auto", [],
                            "capture 里没有 7000/7100/5100 映射，auto 无法选判据")]
    else:
        names = [args.mode]
    return [funcs[name](cap, args) for name in names]


def _overall(results):
    if not results:
        return "UNCLOSED"
    worst = max(results, key=lambda r: STATUS_RANK[r["status"]])
    return worst["status"]


def run_report(capture_dir, args, json_out=None):
    common = load_common()
    try:
        cap = common.Capture(capture_dir)
    except FileNotFoundError as exc:
        print("PREREQ_MISSING: %s" % exc, file=sys.stderr)
        return 2
    try:
        results = run_modes(cap, args)
    except ValueError as exc:
        print("PREREQ_MISSING: %s" % exc, file=sys.stderr)
        return 2
    overall = _overall(results)
    rc = {"GREEN": 0, "RED": 1, "UNCLOSED": 3}[overall]
    for res in results:
        print("[%s] %s" % (res["status"], res["mode"]))
        for chk in res["checks"]:
            print("  %s %s: %s" % ("OK" if chk["ok"] else "FAIL", chk["name"], chk["detail"]))
        if res["unclosed_reason"]:
            print("  UNCLOSED: %s" % res["unclosed_reason"])
    print("OVERALL %s (rc=%d)" % (overall, rc))
    report = {
        "tool": "gate_timer_gate",
        "capture": os.path.abspath(capture_dir),
        "mode": args.mode,
        "thresholds": {
            "timeout_ms": args.timeout_ms,
            "login_scan_ms": args.login_scan_ms,
            "sel_scan_ms": args.sel_scan_ms,
            "tolerance_ms": args.tolerance_ms,
            "keep_open_ms": args.keep_open_ms,
            "min_gap_ms": args.min_gap_ms,
            "max_gap_ms": args.max_gap_ms,
        },
        "overall": overall,
        "rc": rc,
        "results": results,
    }
    if json_out:
        with open(json_out, "w", encoding="utf-8") as fh:
            json.dump(report, fh, ensure_ascii=False, indent=2)
    return rc


def _parser():
    ap = argparse.ArgumentParser(
        description="网关 timer gate：60s 两阶段超时 / 10s 重连（读 capture 时间戳）"
    )
    ap.add_argument("--capture", help="gate_shadow_proxy / mir2_proxy 输出目录")
    ap.add_argument("--mode", default="auto",
                    choices=("auto", "login-idle", "sel-idle", "sel-reconnect",
                             "sel-keepalive", "all"))
    ap.add_argument("--port", type=int, default=None,
                    help="显式端口（监听或上游）；auto 时默认 7000/7100/5100")
    ap.add_argument("--dir", default="c2s", choices=("c2s", "s2c"),
                    help="sel-keepalive 的 KeepAlive 方向（下行代理路径通常 c2s）")
    ap.add_argument("--timeout-ms", type=int, default=60_000)
    ap.add_argument("--login-scan-ms", type=int, default=LOGIN_SCAN_MS)
    ap.add_argument("--sel-scan-ms", type=int, default=SEL_SCAN_MS)
    ap.add_argument("--tolerance-ms", type=int, default=2_000)
    ap.add_argument("--keep-open-ms", type=int, default=1_000,
                    help="SelGate 超时帧后此窗口内 close 即判 RED")
    ap.add_argument("--min-gap-ms", type=int, default=9_500)
    ap.add_argument("--max-gap-ms", type=int, default=11_000)
    ap.add_argument("--json-out")
    ap.add_argument("--selftest", action="store_true")
    return ap


def main(argv=None):
    args = _parser().parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.capture:
        print("PREREQ_MISSING: 需要 --capture DIR（或 --selftest）", file=sys.stderr)
        return 2
    return run_report(args.capture, args, args.json_out)


# ------------------------------------------------------------------ selftest

class _Events:
    """合成 capture：写 chunks.ndjson + conn-*.bin + session.json（无 socket）。"""

    def __init__(self, out_dir, mapping):
        self.dir = out_dir
        self.base = 1_700_000_000_000
        self.mapping = mapping
        self.rows = []
        self.offs = {}
        self.bins = {}
        os.makedirs(out_dir, exist_ok=True)

    def _t(self, t):
        return {"t_wall_ms": self.base + t, "t_rel_us": t * 1000}

    def open(self, conn, port, t):
        self.rows.append({"event": "open", "conn": conn, "port": port,
                          "peer": "127.0.0.1:1", "upstream": "127.0.0.1:0",
                          **self._t(t)})

    def data(self, conn, port, direction, blob, t):
        key = (conn, direction)
        off = self.offs.get(key, 0)
        self.rows.append({"event": "data", "conn": conn, "port": port,
                          "dir": direction, "off": off, "n": len(blob),
                          "sha256": __import__("hashlib").sha256(blob).hexdigest(),
                          **self._t(t)})
        self.offs[key] = off + len(blob)
        self.bins[key] = self.bins.get(key, b"") + blob

    def close(self, conn, port, t):
        self.rows.append({"event": "close", "conn": conn, "port": port,
                          **self._t(t)})

    def upstream_fail(self, conn, port, t):
        self.rows.append({"event": "upstream_fail", "conn": conn, "port": port,
                          "error": "synthetic", **self._t(t)})

    def write(self):
        with open(os.path.join(self.dir, "chunks.ndjson"), "w", encoding="utf-8") as fh:
            for row in sorted(self.rows, key=lambda r: r["t_wall_ms"]):
                fh.write(json.dumps(row, ensure_ascii=False) + "\n")
        for (conn, direction), blob in self.bins.items():
            path = os.path.join(self.dir, "conn-%04d.%s.bin" % (conn, direction))
            with open(path, "wb") as fh:
                fh.write(blob)
        with open(os.path.join(self.dir, "session.json"), "w", encoding="utf-8") as fh:
            json.dump({"tool": "gate_shadow_proxy", "map": self.mapping}, fh)
        return self.dir


def _ed():
    path = os.path.join(
        HERE, "..", "..", "mir2-rs", "tests", "parity", "loginsrv", "oracle_capture.py"
    )
    spec = importlib.util.spec_from_file_location("_gwcap_timer_ed", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _header(ident):
    return (0).to_bytes(4, "little") + ident.to_bytes(2, "little") + b"\x00" * 6


def _login_frame(ed):
    return b"#" + ed.edcode_encode(_header(SM_OUTOFCONNECTION))[:-1]


def _sel_frame(ed):
    return ed.edcode_encode(_header(SM_OUTOFCONNECTION)) + b"\x00!"


def _c2s_frame(ed, ident):
    return b"#1" + ed.edcode_encode(_header(ident)) + b"!"


def _keepalive_packet():
    body = b"\x04\x03" + b"\x00" * 10  # OBJECT_HEADER + KeepAlive + null SocketId/Data
    return (0xAA9AAA9A).to_bytes(4, "little") + len(body).to_bytes(2, "little") + body


def _mk_login(path, t_frame, with_close=True, frame=None, client_at=None,
              with_frame=True):
    ed = _ed()
    ev = _Events(path, {"7001": "127.0.0.1:7000"})
    ev.open(1, 7001, 0)
    if client_at is not None:
        ev.data(1, 7001, "c2s", _c2s_frame(ed, 101), client_at)
    if with_frame:
        ev.data(1, 7001, "s2c", frame if frame is not None else _login_frame(ed), t_frame)
    if with_close:
        ev.close(1, 7001, t_frame + 100)
    return ev.write()


def _mk_sel(path, t_frame, refresh_at=None, close_after=None, with_frame=True,
            normal_before=False):
    ed = _ed()
    ev = _Events(path, {"7101": "127.0.0.1:7100"})
    ev.open(1, 7101, 0)
    if refresh_at is not None:
        ev.data(1, 7101, "c2s", _c2s_frame(ed, 101), refresh_at)
    if normal_before:
        normal = b"#" + ed.edcode_encode(_header(520)) + b"!"
        ev.data(1, 7101, "s2c", normal, 5_000)
    if with_frame:
        ev.data(1, 7101, "s2c", _sel_frame(ed), t_frame)
    if close_after is not None:
        ev.close(1, 7101, t_frame + close_after)
    return ev.write()


def _mk_recon(path, opens, ends):
    ev = _Events(path, {"5510": "127.0.0.1:5100"})
    for conn, t in opens:
        ev.open(conn, 5510, t)
    for conn, t, kind in ends:
        if kind == "fail":
            ev.upstream_fail(conn, 5510, t)
        else:
            ev.close(conn, 5510, t)
    return ev.write()


def _mk_keepalive(path, times):
    ev = _Events(path, {"5510": "127.0.0.1:5100"})
    ev.open(1, 5510, times[0] if times else 0)
    for t in times:
        ev.data(1, 5510, "c2s", _keepalive_packet(), t)
    return ev.write()


def selftest():
    import contextlib
    import io
    import shutil
    import tempfile

    tmp = tempfile.mkdtemp(prefix="gwcap-timer-selftest-")
    failures = []
    count = [0]

    def status(capture, mode, tag):
        path = os.path.join(tmp, "report-%s.json" % tag)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = main(["--capture", capture, "--mode", mode, "--json-out", path])
        with open(path, "r", encoding="utf-8") as fh:
            report = json.load(fh)
        return rc, report["overall"]

    def expect(name, got, want_status, want_rc):
        count[0] += 1
        if got != (want_rc, want_status):
            failures.append("%s: 期望 rc=%d/%s，实得 rc=%d/%s"
                            % (name, want_rc, want_status, got[0], got[1]))
            print("FAIL %s: got=%r" % (name, got))
        else:
            print("PASS %d %s" % (count[0], name))

    def sub(name):
        path = os.path.join(tmp, name)
        os.makedirs(path, exist_ok=True)
        return path

    try:
        # ---- login-idle ----
        d = _mk_login(sub("login-green"), 60_500, client_at=50_000)
        expect("login-idle 60.5s（中途客户端数据不刷新）⇒ GREEN",
               status(d, "login-idle", "login-green"), "GREEN", 0)
        d = _mk_login(sub("login-early"), 59_500)
        expect("login-idle 59.5s（严格 >60s）⇒ RED",
               status(d, "login-idle", "login-early"), "RED", 1)
        d = _mk_login(sub("login-late"), 63_500)
        expect("login-idle 63.5s（超过 60s+1s 扫描+容差）⇒ RED",
               status(d, "login-idle", "login-late"), "RED", 1)
        d = _mk_login(sub("login-noframe"), 60_500, with_frame=False)
        expect("login-idle 无 16B 超时帧 ⇒ UNCLOSED",
               status(d, "login-idle", "login-noframe"), "UNCLOSED", 3)
        d = _mk_login(sub("login-noclose"), 60_500, with_close=False)
        expect("login-idle 有帧但无 close（观测不全）⇒ UNCLOSED",
               status(d, "login-idle", "login-noclose"), "UNCLOSED", 3)
        ed = _ed()
        normal18 = b"#" + ed.edcode_encode(_header(SM_OUTOFCONNECTION)) + b"!"
        d = _mk_login(sub("login-wrongshape"), 60_500, frame=normal18)
        expect("login-idle 18B 正常帧冒充（不是截断缺陷形状）⇒ UNCLOSED",
               status(d, "login-idle", "login-wrongshape"), "UNCLOSED", 3)
        d = _mk_login(sub("login-auto"), 60_500)
        expect("auto 只跑 7000 上的判据 ⇒ GREEN",
               status(d, "auto", "login-auto"), "GREEN", 0)

        # ---- sel-idle ----
        d = _mk_sel(sub("sel-green"), 85_000, refresh_at=10_000, normal_before=True)
        expect("sel-idle 正常 18B 帧在前 + 刷新后 75s ⇒ GREEN（不误认正常帧）",
               status(d, "sel-idle", "sel-green"), "GREEN", 0)
        d = _mk_sel(sub("sel-norefresh"), 85_000, refresh_at=None)
        expect("sel-idle 无 100..103 刷新 ⇒ 85s 超上界 ⇒ RED（刷新语义有牙）",
               status(d, "sel-idle", "sel-norefresh"), "RED", 1)
        d = _mk_sel(sub("sel-late-refresh"), 85_000, refresh_at=86_000)
        expect("sel-idle 刷新在超时帧之后不得回填 ⇒ RED",
               status(d, "sel-idle", "sel-late-refresh"), "RED", 1)
        d = _mk_sel(sub("sel-close"), 85_000, refresh_at=10_000, close_after=100)
        expect("sel-idle 帧后 100ms 关 socket ⇒ RED（SelGate 只摘槽）",
               status(d, "sel-idle", "sel-close"), "RED", 1)
        d = _mk_sel(sub("sel-noframe"), 85_000, refresh_at=10_000, with_frame=False)
        expect("sel-idle 无 528/18B 缺陷帧 ⇒ UNCLOSED",
               status(d, "sel-idle", "sel-noframe"), "UNCLOSED", 3)
        d = _mk_sel(sub("sel-normal18"), 85_000, refresh_at=10_000,
                    with_frame=False, normal_before=True)
        expect("sel-idle 只有正常 18B 帧（'#' 前导）⇒ UNCLOSED（形状判据有牙）",
               status(d, "sel-idle", "sel-normal18"), "UNCLOSED", 3)
        d = _mk_sel(sub("sel-auto"), 85_000, refresh_at=10_000)
        expect("auto 只跑 7100 上的判据 ⇒ GREEN",
               status(d, "auto", "sel-auto"), "GREEN", 0)

        # ---- sel-reconnect ----
        d = _mk_recon(
            sub("recon-green"),
            [(1, 0), (2, 10_100), (3, 20_200)],
            [(1, 5_000, "close"), (2, 15_000, "close")],
        )
        expect("sel-reconnect 相邻 open 间隔 10.1s ⇒ GREEN",
               status(d, "sel-reconnect", "recon-green"), "GREEN", 0)
        d = _mk_recon(
            sub("recon-red"), [(1, 0), (2, 5_000)], [(1, 1_000, "close")]
        )
        expect("sel-reconnect 5s 重连 ⇒ RED",
               status(d, "sel-reconnect", "recon-red"), "RED", 1)
        d = _mk_recon(sub("recon-unclosed"), [(1, 0)], [])
        expect("sel-reconnect 只有一次 open ⇒ UNCLOSED",
               status(d, "sel-reconnect", "recon-unclosed"), "UNCLOSED", 3)
        d = _mk_recon(
            sub("recon-fail-green"),
            [(1, 0), (2, 10_100), (3, 20_200)],
            [(1, 500, "fail"), (2, 15_000, "fail")],
        )
        expect("sel-reconnect 连接失败（upstream_fail）也计一次尝试 ⇒ GREEN",
               status(d, "sel-reconnect", "recon-fail-green"), "GREEN", 0)

        # ---- sel-keepalive ----
        d = _mk_keepalive(sub("ka-green"), [0, 10_000, 20_010])
        expect("sel-keepalive 相邻 KeepAlive 10.0/10.01s ⇒ GREEN",
               status(d, "sel-keepalive", "ka-green"), "GREEN", 0)
        d = _mk_keepalive(sub("ka-red"), [0, 5_000])
        expect("sel-keepalive 5s 心跳 ⇒ RED",
               status(d, "sel-keepalive", "ka-red"), "RED", 1)
        d = _mk_keepalive(sub("ka-unclosed"), [0])
        expect("sel-keepalive 只有 1 条 KeepAlive ⇒ UNCLOSED",
               status(d, "sel-keepalive", "ka-unclosed"), "UNCLOSED", 3)
        d = _mk_keepalive(sub("ka-auto"), [0, 10_000, 20_010])
        expect("auto 在 5100 上选可观测的 keepalive ⇒ GREEN",
               status(d, "auto", "ka-auto"), "GREEN", 0)

        # ---- strictness ----
        d = _mk_login(sub("all-login"), 60_500)
        expect("--mode all 在只有 7000 的 capture 上：其余判据 UNCLOSED ⇒ 整体 UNCLOSED",
               status(d, "all", "all-login"), "UNCLOSED", 3)
        count[0] += 1
        errbuf = io.StringIO()
        with contextlib.redirect_stderr(errbuf):
            rc_missing = main(["--capture", os.path.join(tmp, "does-not-exist"),
                               "--mode", "login-idle"])
        if rc_missing != 2:
            failures.append("缺 capture 目录: 期望 rc=2，实得 %d" % rc_missing)
            print("FAIL missing capture rc=%d" % rc_missing)
        else:
            print("PASS %d 缺 capture 目录 ⇒ PREREQ_MISSING rc=2" % count[0])

        if failures:
            print("RED: gate timer selftest %d/%d 失败" % (len(failures), count[0]))
            for f in failures:
                print("  - " + f)
            return 1
        print("GREEN: gate timer selftest %d/%d" % (count[0], count[0]))
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
