#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""timeline.py — 日志时间线对齐与对拍（C 线工具 ③）。

把多源日志归一成带毫秒时间戳的事件流，按时间合并成一条时间线；
或把两条时间线做序列对拍（基线 vs 新跑），第一个分叉点即红。

支持的源（按内容自动识别）：
  chunks.ndjson   mir2_proxy 抓包事件（t_wall_ms 绝对时间）
  stages.ndjson   mir_flow 阶段标记（t_wall_ms 绝对时间）
  cl_trace.txt    客户端 hook 日志（HH:mm:ss.fff 行首，日期用 --anchor 或文件 mtime）
  *.out.log       服务端 Serilog 控制台日志（HH:mm:ss.fff [LVL] 行首，同上）

用法：
  python timeline.py merge --out timeline.ndjson --src name=path [--src ...] [--anchor 2026-10-10]
  python timeline.py diff A.ndjson B.ndjson [--max-diffs 5]
  python timeline.py selftest          # 改坏必红自检
"""
import argparse
import hashlib
import json
import os
import re
import sys

RE_HMS = re.compile(r"^(\d{2}):(\d{2}):(\d{2})\.(\d{3})\s+(.*)$")


def day_ms_from_anchor(anchor: str) -> int:
    import datetime
    dt = datetime.datetime.strptime(anchor, "%Y-%m-%d")
    return int(dt.timestamp() * 1000)


def parse_ndjson(path: str, source: str):
    rows = []
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            if "t_wall_ms" not in r:
                continue
            if "stage" in r:
                rows.append({"t_ms": r["t_wall_ms"], "src": source, "kind": "stage", "text": r["stage"]})
            elif r.get("event") == "open":
                rows.append({"t_ms": r["t_wall_ms"], "src": source, "kind": "conn_open",
                             "text": f"conn={r['conn']} port={r['port']}"})
            elif r.get("event") == "close":
                rows.append({"t_ms": r["t_wall_ms"], "src": source, "kind": "conn_close",
                             "text": f"conn={r['conn']} port={r['port']}"})
            elif r.get("event") == "data":
                rows.append({"t_ms": r["t_wall_ms"], "src": source, "kind": "pkt",
                             "text": f"conn={r['conn']} {r['dir']} n={r['n']}"})
    return rows


def parse_hms_log(path: str, source: str, day_ms: int):
    rows = []
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            m = RE_HMS.match(line.rstrip("\n"))
            if not m:
                continue
            h, mi, s, ms, rest = int(m.group(1)), int(m.group(2)), int(m.group(3)), int(m.group(4)), m.group(5)
            t = day_ms + ((h * 60 + mi) * 60 + s) * 1000 + ms
            kind = "pkt" if rest.startswith(("PKT", "RECV", "SEND")) else ("state" if rest.startswith("STATE") else "log")
            rows.append({"t_ms": t, "src": source, "kind": kind, "text": rest})
    return rows


def detect_and_parse(name: str, path: str, day_ms: int):
    base = os.path.basename(path).lower()
    if base.endswith(".ndjson"):
        return parse_ndjson(path, name)
    return parse_hms_log(path, name, day_ms)


def normalize(text: str) -> str:
    """对拍键：抹掉易变字段（数字串/IP/端口/hash），保留事件骨架。"""
    t = re.sub(r"\b\d{1,3}(?:\.\d{1,3}){3}\b", "<ip>", text)
    t = re.sub(r"[0-9a-f]{16,}", "<hex>", t)
    t = re.sub(r"\d+", "<n>", t)
    return t


def cmd_merge(args):
    day_ms = 0
    if args.anchor:
        day_ms = day_ms_from_anchor(args.anchor)
    else:
        # 用第一个源文件的 mtime 日期作锚
        first = args.src[0].split("=", 1)[1]
        import datetime
        day_ms = day_ms_from_anchor(datetime.datetime.fromtimestamp(os.path.getmtime(first)).strftime("%Y-%m-%d"))
    rows = []
    for spec in args.src:
        name, _, path = spec.partition("=")
        if not name or not os.path.exists(path):
            print(f"RED: 源不存在或格式错: {spec}", file=sys.stderr)
            sys.exit(2)
        rows.extend(detect_and_parse(name, path, day_ms))
    rows.sort(key=lambda r: (r["t_ms"], r["src"]))
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    kinds = {}
    for r in rows:
        kinds[r["kind"]] = kinds.get(r["kind"], 0) + 1
    print(f"MERGED {len(rows)} events kinds={kinds} -> {args.out}")


def cmd_diff(args):
    def load(p):
        rows = []
        with open(p, "r", encoding="utf-8") as f:
            for line in f:
                r = json.loads(line)
                if r["kind"] in ("pkt", "stage", "conn_open", "conn_close"):
                    rows.append(r)
        return rows

    a = load(args.a)
    b = load(args.b)
    ka = [f"{r['kind']}|{normalize(r['text'])}" for r in a]
    kb = [f"{r['kind']}|{normalize(r['text'])}" for r in b]
    diffs = 0
    import difflib
    sm = difflib.SequenceMatcher(a=ka, b=kb, autojunk=False)
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag != "equal":
            diffs += 1
            if diffs <= args.max_diffs:
                print(f"DIFF @{i1}/{j1} {tag}:")
                for r in a[i1:i2][:3]:
                    print(f"  A[{i1}] {r['kind']} {r['text'][:120]}")
                for r in b[j1:j2][:3]:
                    print(f"  B[{j1}] {r['kind']} {r['text'][:120]}")
    if diffs:
        print(f"RED: 时间线序列存在 {diffs} 处分叉（A={len(a)} 事件, B={len(b)} 事件）")
        sys.exit(1)
    print(f"GREEN: 两条时间线事件序列一致（{len(a)} 事件）")
    sys.exit(0)


def cmd_selftest():
    """改坏必红：构造两条一致时间线 → diff 必须绿；篡改一个事件 → 必须红。"""
    import tempfile
    tmp = tempfile.mkdtemp(prefix="timeline-selftest-")
    base = [
        {"t_ms": 1, "src": "s", "kind": "conn_open", "text": "conn=1 port=7000"},
        {"t_ms": 2, "src": "s", "kind": "pkt", "text": "conn=1 c2s n=30"},
        {"t_ms": 3, "src": "s", "kind": "conn_close", "text": "conn=1 port=7000"},
    ]
    pa = os.path.join(tmp, "a.ndjson")
    pb = os.path.join(tmp, "b.ndjson")
    with open(pa, "w", encoding="utf-8") as f:
        for r in base:
            f.write(json.dumps(r) + "\n")
    with open(pb, "w", encoding="utf-8") as f:
        for r in base:
            f.write(json.dumps(r) + "\n")
    ns = argparse.Namespace(a=pa, b=pb, max_diffs=5)
    try:
        cmd_diff(ns)
        green_ok = True
    except SystemExit as e:
        green_ok = (e.code == 0)
    if not green_ok:
        print("RED: 一致时间线被误判为分叉")
        sys.exit(1)
    print("PASS 1/2 一致时间线判绿")
    # 篡改：抽掉中间事件
    with open(pb, "w", encoding="utf-8") as f:
        for r in [base[0], base[2]]:
            f.write(json.dumps(r) + "\n")
    try:
        cmd_diff(ns)
        print("RED: 抽掉一个事件后仍判绿——红检失效")
        sys.exit(1)
    except SystemExit as e:
        if e.code != 1:
            print("RED: 篡改后退出码异常:", e.code)
            sys.exit(1)
    print("PASS 2/2 抽掉事件后判红（改坏必红成立）")
    print("GREEN: timeline selftest 2/2")
    sys.exit(0)


def main():
    ap = argparse.ArgumentParser(description="日志时间线对齐与对拍")
    sub = ap.add_subparsers(dest="cmd", required=True)
    m = sub.add_parser("merge")
    m.add_argument("--out", required=True)
    m.add_argument("--src", action="append", required=True, help="name=path，可多次")
    m.add_argument("--anchor", default="", help="HH:mm:ss 日志的日期锚 YYYY-MM-DD（默认取首个源文件 mtime）")
    d = sub.add_parser("diff")
    d.add_argument("a")
    d.add_argument("b")
    d.add_argument("--max-diffs", type=int, default=5)
    sub.add_parser("selftest")
    args = ap.parse_args()
    {"merge": cmd_merge, "diff": cmd_diff, "selftest": lambda a: cmd_selftest()}[args.cmd](args)


if __name__ == "__main__":
    main()
