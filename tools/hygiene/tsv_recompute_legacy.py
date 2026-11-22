#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""tsv_recompute_legacy — 按旧 `write_tick` 的 **vals 位置**回算 perf TSV 受影响列的真值。

## 为什么需要它

2026-11-23 发现 `game-svr` 的 `perf_counters::write_tick` 的 **vals 顺序与表头列名顺序不一致**
（不是列数问题）：
- ⑤ 的 outcome/诊断列在表头 `cross_recipient_missing` 之后，vals 却排在 handoff/zone-k 之后；
- 更早的 own-segment 两列（`hd[11..12]`）在 vals 里排在 zone-k 三列之前，表头却排在之后。

⇒ `perf_tsv_summary.py` **按列名**读旧产物时，错位段之后的列读到的是**别的计数器**的值。
修 writer（本仓已修）**不能**让旧产物变对：旧产物只能按**当时的 vals 位置**读才是真值。
本工具就是那张「表头索引 ↔ 旧 vals 位置」映射的可执行版（只读；不修改任何产物）。

## 版本与映射（1-based 列号，行内含 tick）

- **④a**（无 `cross_recipient_delivered`、无 `mirror_own_segment_ids`）：全部**对齐**。
- **④b**（有 `mirror_own_segment_ids`）：handoff 11 对齐；`zone_cmd/handoff_move/move_map`
  表头 42/43/44 实际是 `hd[11]/hd[12]/ZONE_CMD`，真值 **44/45/46**；
  `own_segment/fresh` 表头 45/46 实际是 `HANDOFF_MOVE/MOVE_MAP`，真值 **42/43**。
- **⑤-6**（有 `cross_recipient_delivered`、无 `cross_enqueue_us`）：⑤ 6 列表头 31–36 实际是
  `hd[0..5]`，真值 **47–52**；handoff 37–47 真值 **31–41**；zone 48–50 真值 **44–46**；
  own 51–52 真值 **42–43**。
- **⑤-8**（有 `cross_enqueue_us`）：⑤ 8 列表头 31–38 真值 **47–54**；其余同上。

**不受影响**（错位段之前）：`tick`、2–23（actor/chr/inbound/barrier 等）、六面、`cross_recipient_missing`、
`refmsg_calls/targets`。⇒ 历史 10.004% 基线（六面/`missing`）可直接用。

## 用法

  python -I -B tools/hygiene/tsv_recompute_legacy.py --tsv <perf_counters.tsv>
  python -I -B tools/hygiene/tsv_recompute_legacy.py --selftest

退出码：0 = 读到并回算；2 = 输入不可判/自测失败（fail-closed）。
"""
import argparse
import os
import sys
import tempfile

LEADING = [
    "actor_indexed", "chr_indexed", "actor_linear", "chr_linear", "refmsg_calls",
    "refmsg_targets", "say_calls", "say_recipients", "kick_scans", "inbound_enqueued",
    "inbound_drained", "inbound_carry", "inbound_budget_hit", "inbound_wall_us",
    "drained_command", "drained_close", "drained_control", "max_single_event_us",
    "zone_router_overwrites", "zone_router_rejected_socket", "region_barrier_us",
    "region_barrier_total_us",
]
SIX = [
    "cross_recipient_refmsg", "cross_recipient_run_frames", "cross_recipient_npc_merchant",
    "cross_recipient_feature_changed", "cross_recipient_mountride",
    "cross_recipient_merchant_disappear",
]
HANDOFF = [
    "handoff_offered", "handoff_accepted", "handoff_refused", "handoff_promoted",
    "handoff_inserted_fresh", "mirror_shadowed_by_local", "mirror_shadowed_static",
    "mirror_shadowed_handoff", "handoff_dup_dropped", "mirror_write_refused_cell",
    "mirror_write_refused_occupied",
]
ZONE = ["zone_cmd_unsupported", "handoff_move_after_transfer_dropped", "move_map_change_unsupported"]
OWN = ["mirror_own_segment_ids", "handoff_fresh_own_segment"]
FIVE = [
    "cross_recipient_delivered", "cross_recipient_unroutable",
    "cross_frame_queue_overflow", "cross_frame_dup", "cross_frame_drop_closed",
    "cross_recipient_zone_unknown", "cross_enqueue_us", "cross_enqueue_events",
]
ALIGNED = LEADING + SIX + ["cross_recipient_missing"]


class InputError(Exception):
    pass


def detect(header):
    """返回 (version, n5)：version ∈ {d4a, d4b, d5}；n5 = ⑤ 列数（0/6/8）。"""
    if "cross_recipient_delivered" in header:
        return "d5", (8 if "cross_enqueue_us" in header else 6)
    if "mirror_own_segment_ids" in header:
        return "d4b", 0
    return "d4a", 0


def true_row0(name, ver, n5, header):
    """真值的 0-based 行内列号（row 含 tick 在 0）；None = 旧产物没有该列。"""
    if ver == "d4a" or name in ALIGNED:
        return header.index(name)
    if name in HANDOFF:
        return 30 + HANDOFF.index(name)
    if name in OWN:
        return 41 + OWN.index(name)
    if name in ZONE:
        return 43 + ZONE.index(name)
    if name in FIVE:
        i = FIVE.index(name)
        if i >= n5:
            return None
        return 46 + i
    return header.index(name)


def load(path):
    header = None
    rows = []
    with open(path, "r", encoding="utf-8", newline="") as f:
        for lineno, line in enumerate(f, 1):
            line = line.rstrip("\r\n")
            if not line:
                continue
            cells = line.split("\t")
            if header is None:
                header = cells
            else:
                rows.append([int(c) for c in cells])
    if header is None:
        raise InputError("空 TSV：%s" % path)
    if not rows:
        raise InputError("只有表头、无 tick 行：%s" % path)
    return header, rows


def recompute(path):
    header, rows = load(path)
    ver, n5 = detect(header)
    cols = ALIGNED + HANDOFF + ZONE + OWN
    if ver == "d5":
        cols += FIVE
    out = []
    for name in cols:
        if name not in header:
            continue
        tc = true_row0(name, ver, n5, header)
        ai = header.index(name)
        as_read = sum(r[ai] for r in rows)
        true = None if tc is None else sum(r[tc] for r in rows)
        out.append((name, as_read, true))
    return ver, len(rows), out


def print_table(path):
    ver, n, table = recompute(path)
    print("=== %s === ver=%s rows=%d" % (path, ver, n))
    print("%-40s %14s %14s %s" % ("column", "as_read", "true", "FLAG"))
    for name, a, t in table:
        flag = "" if (t is None or a == t) else "MISMATCH"
        print("%-40s %14d %14s %s" % (name, a, "n/a" if t is None else t, flag))


def _synthetic_header(ver, n5):
    h = ["tick"] + LEADING + SIX + ["cross_recipient_missing"]
    if ver == "d5":
        h += FIVE[:n5]
    h += HANDOFF + ZONE + OWN
    return h


def _synthetic_row(ver, n5):
    # vals 旧顺序：leading 22, six 6, missing, hd 13, ZONE_CMD, HANDOFF_MOVE, MOVE_MAP, [⑤ n5]
    hd = [100 + i for i in range(13)]
    vals = [0] * 22 + [1, 2, 3, 4, 5, 6, 21] + hd + [200, 201, 202]
    if ver == "d5":
        vals += [300 + i for i in range(n5)]
    return [0] + vals  # tick=0


def selftest():
    ok = True

    def expect(cond, msg):
        nonlocal ok
        if not cond:
            ok = False
            print("SELFTEST_FAIL: " + msg)

    with tempfile.TemporaryDirectory() as td:
        for ver, n5 in (("d4b", 0), ("d5", 6), ("d5", 8)):
            h = _synthetic_header(ver, n5)
            r = _synthetic_row(ver, n5)
            path = os.path.join(td, "%s_%d.tsv" % (ver, n5))
            with open(path, "w", encoding="utf-8", newline="\n") as f:
                f.write("\t".join(h) + "\n")
                f.write("\t".join(str(v) for v in r) + "\n")
            gotver, _, table = recompute(path)
            d = {name: (a, t) for name, a, t in table}
            expect(gotver == ver, "%s 版本识别错：%s" % (path, gotver))
            # 对齐列：as_read == true
            for name in ("cross_recipient_refmsg", "cross_recipient_missing", "refmsg_targets"):
                expect(d[name][0] == d[name][1], "%s 对齐列 %s 不应 MISMATCH" % (ver, name))
            # ④b/⑤：handoff 11 的真值在 30..40；zone/own 错位
            expect(d["handoff_offered"][1] == 100, "%s handoff_offered true 应为 hd[0]=100" % ver)
            expect(d["zone_cmd_unsupported"][1] == 200, "%s zone_cmd true 应为 ZONE_CMD=200" % ver)
            expect(d["handoff_move_after_transfer_dropped"][1] == 201,
                   "%s handoff_move true 应为 201" % ver)
            expect(d["move_map_change_unsupported"][1] == 202, "%s move_map true 应为 202" % ver)
            expect(d["mirror_own_segment_ids"][1] == 111, "%s own_segment true 应为 hd[11]=111" % ver)
            expect(d["handoff_fresh_own_segment"][1] == 112, "%s fresh true 应为 hd[12]=112" % ver)
            if ver == "d4b":
                # ④b: zone_cmd as_read = hd[11] (111)，own as_read = HANDOFF_MOVE (201)
                expect(d["zone_cmd_unsupported"][0] == 111, "④b zone_cmd as_read 应为 111")
                expect(d["mirror_own_segment_ids"][0] == 201, "④b own_segment as_read 应为 201")
            else:
                # ⑤: delivered true = 300；handoff_offered as_read = hd[6]（100+6=106）
                expect(d["cross_recipient_delivered"][1] == 300, "⑤ delivered true 应为 300")
                # ⑤-6 的 as-read 落在 hd[6]（106）；⑤-8 落在 hd[8]（108）。
                expect(d["handoff_offered"][0] == (106 if n5 == 6 else 108),
                       "⑤ handoff_offered as_read 应为 hd[%d]=%d" % (n5, 106 if n5 == 6 else 108))
                if n5 == 8:
                    expect(d["cross_enqueue_events"][1] == 307, "⑤-8 enqueue_events true 应为 307")
    print("SELFTEST " + ("OK" if ok else "FAIL"))
    return 0 if ok else 2


def main(argv):
    ap = argparse.ArgumentParser(description="按旧 vals 位置回算 perf TSV 受影响列（只读）")
    ap.add_argument("--tsv", help="perf_counters.tsv 路径")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.tsv:
        print("用法：--tsv <perf_counters.tsv> 或 --selftest", file=sys.stderr)
        return 2
    try:
        print_table(args.tsv)
    except (InputError, OSError, ValueError) as exc:
        print("INPUT_ERROR: %s" % exc, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
