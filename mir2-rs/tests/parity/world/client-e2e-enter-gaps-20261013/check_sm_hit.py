#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_sm_hit.py — SM_HIT(14) 帧级判据（评审 C1；卡 `openmir2-monster-attack-hit-frame`）。

背景：`check_attack_observability.py` 只数 `CM_HIT/SM_STRUCK/SM_NOWDEATH` 条数，
判不了「攻击动作帧」本身的头字段、收件人和先后。本工具补这三项：

  ① **头字段**：每条 s2c `SM_HIT(14)` 断言
     `Recog = 攻方 ActorId`、`Param = X`、`Tag = Y`、`Series = Dir`
     （oracle：`AnimalObject.Attack` -> `SendAttackMsg(RM_HIT, Dir, CurrX, CurrY)`
      -> `SendRefMsg(RM_HIT, btDir, nX, nY, ...)` -> `PlayObject.Message` RM_HIT 支
      `MakeMessage(SM_HIT, ActorId, nParam1, nParam2, wParam)` =>
      `recog=ActorId, param=CurrX, tag=CurrY, series=Dir`）。
     期望值用 `--expect-hit recog,x,y,dir`（可重复、按出现序）或
     `--expect-recog/--expect-x/--expect-y/--expect-dir`（集合口径）给；
     不给期望时仍做类型/范围校验（Series 0..7，X/Y 0..65535）。

  ② **收件人集合**：按 `(conn, port, hop)` 汇总收到 SM_HIT 的连接，打印；
     `--expect-recipients 1,2`（conn 列表）或 `--expect-recipients count=2` 可机判。
     单客户端窗口通常只有 conn=1。

  ③ **先后（前导）**：对每条 s2c `SM_STRUCK(31)`，在同 conn 上找**它之前**
     （seq 更小）且在 `--lead-ms`（默认 250ms，评审口径）内的最近一条 SM_HIT，
     记为 1 个 pair；`--expect order` 要求 pair 数 >= `--min-order-pairs`（默认 1），
     即「同 tick 的 SM_HIT 在对应 SM_STRUCK 之前」。`--require-each-struck` 可把
     「每条受击都必须有前导命中帧」也设为硬判据（真实窗口里怪物未命中/多点攻击
     常无前导，默认不开）。

`--expect` 接受逗号列表：`count`（条数门）、`order`（前导门），默认两者都开。
  count 用 `--expect-count N`（精确）或 `--min-count N`（默认 1）。

用法（真实窗口）：
  python -I check_sm_hit.py --capture E:/tmp/window/cliente2e/csharp/s2c-w01b/capture.jsonl \
      --expect count,order --expect-count 26 --expect-hit 386648734,605,592,0 \
      --expect-recipients 1 --lead-ms 250

退出码：0 GREEN / 1 RED / 2 PREREQ_MISSING（缺 capture/不可解析/参数不当）。
`--selftest` 不依赖外部文件（合成 capture 正反例）。
"""

from __future__ import annotations

import argparse
import json
import os
import sys

SM_HIT = 14
SM_STRUCK = 31

FRAME_FIELDS = ("recog", "param", "tag", "series")


def load_capture(path):
    if not os.path.isfile(path):
        raise FileNotFoundError(path)
    rows = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def _int_list(spec):
    out = []
    for part in spec.replace(";", ",").split(","):
        part = part.strip()
        if part:
            out.append(int(part, 0))
    return out


def parse_hit_spec(spec):
    """`recog,x,y,dir` -> 4 元组（允许 0x 前缀）。"""
    parts = [p.strip() for p in spec.split(",")]
    if len(parts) != 4:
        raise ValueError("--expect-hit 需要 4 段 recog,x,y,dir：%r" % spec)
    return tuple(int(p, 0) for p in parts)


def s2c_hits(rows, conn=None):
    return [r for r in rows
            if r.get("dir") == "s2c" and r.get("ident") == SM_HIT
            and (conn is None or r.get("conn") == conn)]


def s2c_struck(rows, conn=None):
    return [r for r in rows
            if r.get("dir") == "s2c" and r.get("ident") == SM_STRUCK
            and (conn is None or r.get("conn") == conn)]


def recipients(rows):
    return sorted({(r.get("conn"), r.get("port"), r.get("hop")) for r in s2c_hits(rows)},
                  key=lambda t: (str(t[0]), str(t[1]), str(t[2])))


def check_headers(hits, expect_hits, expect_recog, expect_x, expect_y, expect_dir):
    fails = []
    for i, h in enumerate(hits):
        bad = [f for f in FRAME_FIELDS if not isinstance(h.get(f), int)]
        if bad:
            fails.append("SM_HIT[%d] seq=%s 缺/非整数字段 %s" % (i, h.get("seq"), ",".join(bad)))
            continue
        if not (0 <= h["param"] <= 0xFFFF):
            fails.append("SM_HIT[%d] Param(X)=%d 越界（应 0..65535）" % (i, h["param"]))
        if not (0 <= h["tag"] <= 0xFFFF):
            fails.append("SM_HIT[%d] Tag(Y)=%d 越界（应 0..65535）" % (i, h["tag"]))
        if not (0 <= h["series"] <= 7):
            fails.append("SM_HIT[%d] Series(Dir)=%d 越界（应 0..7）" % (i, h["series"]))
        if expect_recog is not None and h["recog"] not in expect_recog:
            fails.append("SM_HIT[%d] Recog=%d 不在期望攻方集合 %s"
                         % (i, h["recog"], expect_recog))
        if expect_x is not None and h["param"] not in expect_x:
            fails.append("SM_HIT[%d] Param(X)=%d 不在期望 %s" % (i, h["param"], expect_x))
        if expect_y is not None and h["tag"] not in expect_y:
            fails.append("SM_HIT[%d] Tag(Y)=%d 不在期望 %s" % (i, h["tag"], expect_y))
        if expect_dir is not None and h["series"] not in expect_dir:
            fails.append("SM_HIT[%d] Series(Dir)=%d 不在期望 %s"
                         % (i, h["series"], expect_dir))
    if expect_hits:
        if len(hits) < len(expect_hits):
            fails.append("SM_HIT 条数 %d < --expect-hit 期望帧数 %d"
                         % (len(hits), len(expect_hits)))
        for i, exp in enumerate(expect_hits):
            if i >= len(hits):
                break
            h = hits[i]
            got = (h.get("recog"), h.get("param"), h.get("tag"), h.get("series"))
            if got != exp:
                fails.append("SM_HIT[%d] (recog,param,tag,series)=%s 期望 %s"
                             % (i, got, exp))
    return fails


def count_fails(hits, expect_count, min_count):
    n = len(hits)
    if expect_count is not None:
        if n != expect_count:
            return ["SM_HIT(%d) 条数 %d != --expect-count %d" % (SM_HIT, n, expect_count)]
        return []
    if n < min_count:
        return ["SM_HIT(%d) 条数 %d < --min-count %d" % (SM_HIT, n, min_count)]
    return []


def order_pairs(rows, lead_ms, conn):
    """对每条 SM_STRUCK 取同 conn、seq 更小、0 <= struck.ts-hit.ts <= lead_ms 的最近 SM_HIT。"""
    hits = s2c_hits(rows, conn)
    struck = s2c_struck(rows, conn)
    pairs = []
    for st in struck:
        st_ts = st.get("ts_ms")
        st_seq = st.get("seq")
        if not isinstance(st_ts, int) or not isinstance(st_seq, int):
            continue
        cand = [h for h in hits
                if h.get("conn") == st.get("conn")
                and isinstance(h.get("ts_ms"), int) and isinstance(h.get("seq"), int)
                and h["seq"] < st_seq and 0 <= st_ts - h["ts_ms"] <= lead_ms]
        if cand:
            h = max(cand, key=lambda x: x["ts_ms"])
            pairs.append({"hit_seq": h["seq"], "struck_seq": st_seq,
                          "delta_ms": st_ts - h["ts_ms"], "conn": st.get("conn"),
                          "hit_recog": h.get("recog"), "struck_recog": st.get("recog")})
    return pairs


def check_order(pairs, struck_count, min_pairs, require_each_struck):
    fails = []
    if len(pairs) < min_pairs:
        fails.append("同 tick 前导 pair 只有 %d < --min-order-pairs %d"
                     "（SM_HIT 未在 SM_STRUCK 之前出现）" % (len(pairs), min_pairs))
    if require_each_struck:
        missing = struck_count - len(pairs)
        if missing > 0:
            fails.append("%d 条 SM_STRUCK 没有前导 SM_HIT（--require-each-struck）" % missing)
    return fails


def check_recipients(recips, spec):
    if not spec:
        return []
    spec = spec.strip()
    if spec.startswith("count="):
        want = int(spec.split("=", 1)[1])
        if len(recips) != want:
            return ["SM_HIT 收件人连接数 %d != count=%d（实际 %s）" % (len(recips), want, recips)]
        return []
    want_conns = _int_list(spec)
    got_conns = sorted({r[0] for r in recips if isinstance(r[0], int)})
    if got_conns != sorted(want_conns):
        return ["SM_HIT 收件人 conn=%s 期望 %s（完整集合 %s）"
                % (got_conns, sorted(want_conns), recips)]
    return []


def check(rows, expect="count,order", expect_count=None, min_count=1, lead_ms=250,
          min_order_pairs=1, require_each_struck=False, expect_hits=None,
          expect_recog=None, expect_x=None, expect_y=None, expect_dir=None,
          expect_recipients="", conn=None):
    modes = {m.strip() for m in expect.replace(";", ",").split(",") if m.strip()}
    unknown = modes - {"count", "order"}
    if unknown:
        raise ValueError("未知 --expect 项：%s（只支持 count/order）" % ",".join(sorted(unknown)))
    fails = []
    hits = s2c_hits(rows, conn)
    struck = s2c_struck(rows, conn)
    recips = recipients(rows)
    pairs = order_pairs(rows, lead_ms, conn)
    fails.extend(check_headers(hits, expect_hits or [], expect_recog, expect_x,
                               expect_y, expect_dir))
    if "count" in modes:
        fails.extend(count_fails(hits, expect_count, min_count))
    if "order" in modes:
        fails.extend(check_order(pairs, len(struck), min_order_pairs, require_each_struck))
    fails.extend(check_recipients(recips, expect_recipients))
    facts = {
        "sm_hit": len(hits),
        "sm_struck": len(struck),
        "recipients": [{"conn": r[0], "port": r[1], "hop": r[2]} for r in recips],
        "order_pairs": len(pairs),
        "pairs": pairs,
        "hits": [{"seq": h.get("seq"), "recog": h.get("recog"), "x": h.get("param"),
                  "y": h.get("tag"), "dir": h.get("series"), "conn": h.get("conn")}
                 for h in hits],
        "expect": sorted(modes),
        "lead_ms": lead_ms,
    }
    return fails, facts


# ---------------------------------------------------------------------------
# selftest：合成 capture，正反例
# ---------------------------------------------------------------------------

ATTACKER = 386648734
VICTIM = 1214446153
OTHER = 999


def synthetic(n_hits=3, hit_before=True, hit_recog=ATTACKER, x=605, y=592, d=0,
              struck_conn=1, hit_conn=1):
    rows = []
    seq = 0

    def add(direction, ident, ts, c, **kw):
        nonlocal seq
        seq += 1
        row = {"kind": "frame", "dir": direction, "seq": seq, "ts_ms": ts,
               "conn": c, "port": 7200, "hop": "game", "ident": ident,
               "recog": 0, "param": 0, "tag": 0, "series": 0, "body_len": 0,
               "raw_hex": "00"}
        row.update(kw)
        rows.append(row)

    for i in range(n_hits):
        hit_ts = 1000 + i * 1000
        if hit_before:
            add("s2c", SM_HIT, hit_ts, hit_conn,
                recog=hit_recog, param=x, tag=y, series=d)
            add("s2c", SM_HIT + 999, hit_ts + 5, hit_conn)  # noise
            add("s2c", SM_STRUCK, hit_ts + 200, struck_conn,
                recog=VICTIM, param=7, tag=32, series=5)
        else:
            add("s2c", SM_STRUCK, hit_ts, struck_conn,
                recog=VICTIM, param=7, tag=32, series=5)
            add("s2c", SM_HIT, hit_ts + 100, hit_conn,
                recog=hit_recog, param=x, tag=y, series=d)
    return rows


def _raises(fn):
    try:
        fn()
    except ValueError:
        return True
    return False


def selftest():
    ok = True

    def expect(name, got, want):
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-64s %s (rc=%d, want %d)" % (name, "OK" if good else "FAIL", got, want))

    def rc(fails):
        return 1 if fails else 0

    rows = synthetic()
    expect("正例：3 条 SM_HIT + 前导 200ms => GREEN",
           rc(check(rows, "count,order", expect_count=3)[0]), 0)
    expect("负例：条数不符（expect-count 4）=> RED",
           rc(check(rows, "count", expect_count=4)[0]), 1)
    expect("负例：空 capture 计数门 => RED",
           rc(check([], "count", min_count=1)[0]), 1)
    expect("负例：SM_STRUCK 在 SM_HIT 前 => order RED",
           rc(check(synthetic(2, hit_before=False), "order")[0]), 1)
    expect("正例：order 单独判（有前导 pair）=> GREEN",
           rc(check(synthetic(2), "order")[0]), 0)
    expect("负例：错误攻方 ActorId => RED",
           rc(check(synthetic(hit_recog=OTHER), "count", expect_recog=[ATTACKER])[0]), 1)
    expect("负例：错误 X（Param）=> RED",
           rc(check(synthetic(x=606), "count", expect_x=[605])[0]), 1)
    expect("负例：错误 Y（Tag）=> RED",
           rc(check(synthetic(y=585), "count", expect_y=[592])[0]), 1)
    expect("负例：错误 Dir（Series）=> RED",
           rc(check(synthetic(d=7), "count", expect_dir=[0])[0]), 1)
    expect("负例：--expect-hit 逐帧不符 => RED",
           rc(check(synthetic(), "count", expect_hits=[(ATTACKER, 605, 592, 7)])[0]), 1)
    expect("正例：--expect-hit 逐帧命中 => GREEN",
           rc(check(synthetic(1), "count", expect_hits=[(ATTACKER, 605, 592, 0)])[0]), 0)
    expect("负例：收件人 conn=1 期望 2 => RED",
           rc(check(synthetic(), "count", expect_recipients="2")[0]), 1)
    expect("正例：收件人 conn=1 期望 1 => GREEN",
           rc(check(synthetic(), "count", expect_recipients="1")[0]), 0)
    orphan = {"kind": "frame", "dir": "s2c", "seq": 9999, "ts_ms": 999999,
              "conn": 1, "port": 7200, "hop": "game", "ident": SM_STRUCK,
              "recog": VICTIM, "param": 1, "tag": 32, "series": 1,
              "body_len": 16, "raw_hex": "00"}
    expect("负例：require-each-struck 有孤立受击 => RED",
           rc(check(synthetic(1) + [orphan], "order", require_each_struck=True)[0]), 1)
    expect("负例：未知 --expect 项 => 抛错",
           1 if _raises(lambda: check(rows, "bogus")) else 0, 1)
    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description="SM_HIT(14) 帧级判据：头字段/收件人/前导顺序")
    ap.add_argument("--capture", default="", help="capture.jsonl")
    ap.add_argument("--expect", default="count,order",
                    help="逗号列表 count,order（默认两者都判）")
    ap.add_argument("--expect-count", type=int, default=None, help="SM_HIT 精确条数")
    ap.add_argument("--min-count", type=int, default=1, help="SM_HIT 最少条数")
    ap.add_argument("--lead-ms", type=int, default=250,
                    help="前导窗口：SM_HIT 在 SM_STRUCK 之前多少 ms 内（默认 250）")
    ap.add_argument("--min-order-pairs", type=int, default=1,
                    help="至少多少个命中帧->受击帧 前导对（默认 1）")
    ap.add_argument("--require-each-struck", action="store_true",
                    help="每条 SM_STRUCK 都要求有前导 SM_HIT（默认关）")
    ap.add_argument("--expect-hit", action="append", default=[],
                    metavar="RECOG,X,Y,DIR",
                    help="按出现序断言 SM_HIT 头字段，可重复")
    ap.add_argument("--expect-recog", default="", help="攻方 ActorId 集合（逗号/0x）")
    ap.add_argument("--expect-x", default="", help="Param(X) 允许集合")
    ap.add_argument("--expect-y", default="", help="Tag(Y) 允许集合")
    ap.add_argument("--expect-dir", default="", help="Series(Dir) 允许集合")
    ap.add_argument("--expect-recipients", default="",
                    help="收件人 conn 列表（如 1,2）或 count=N")
    ap.add_argument("--conn", type=int, default=None, help="只看指定 conn")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if not args.capture:
        print("PREREQ_MISSING: 必须给 --capture <capture.jsonl>（或 --selftest）")
        print("RESULT: PREREQ_RED")
        return 2
    try:
        rows = load_capture(args.capture)
    except FileNotFoundError as exc:
        print("PREREQ_MISSING: 找不到 capture %s" % exc)
        print("RESULT: PREREQ_RED")
        return 2
    except (ValueError, json.JSONDecodeError) as exc:
        print("PREREQ_MISSING: capture 不可解析：%s" % exc)
        print("RESULT: PREREQ_RED")
        return 2
    try:
        expect_hits = [parse_hit_spec(s) for s in args.expect_hit]
        fails, facts = check(
            rows, args.expect, args.expect_count, args.min_count, args.lead_ms,
            args.min_order_pairs, args.require_each_struck, expect_hits,
            _int_list(args.expect_recog) if args.expect_recog else None,
            _int_list(args.expect_x) if args.expect_x else None,
            _int_list(args.expect_y) if args.expect_y else None,
            _int_list(args.expect_dir) if args.expect_dir else None,
            args.expect_recipients, args.conn)
    except ValueError as exc:
        print("PREREQ_MISSING: 参数不当：%s" % exc)
        print("RESULT: PREREQ_RED")
        return 2

    if args.json:
        print(json.dumps({"capture": args.capture, "failures": fails, "facts": facts},
                         ensure_ascii=False))
    else:
        print("capture=%s" % args.capture)
        print("  sm_hit=%d sm_struck=%d order_pairs=%d lead_ms=%d"
              % (facts["sm_hit"], facts["sm_struck"], facts["order_pairs"], facts["lead_ms"]))
        print("  recipients=%s" % facts["recipients"])
        for i, h in enumerate(facts["hits"]):
            print("  HIT[%d] seq=%s conn=%s recog=%s X=%s Y=%s Dir=%s"
                  % (i, h["seq"], h["conn"], h["recog"], h["x"], h["y"], h["dir"]))
        for p in facts["pairs"]:
            print("  PAIR hit_seq=%s -> struck_seq=%s delta=%dms conn=%s"
                  % (p["hit_seq"], p["struck_seq"], p["delta_ms"], p["conn"]))
        for f in fails:
            print("  RED: %s" % f)
    print("RESULT: %s" % ("RED" if fails else "GREEN"))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
