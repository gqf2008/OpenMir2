#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""frame_drift_report.py — 窗口批次里「邻怪单侧多帧 ⇒ 帧序错位」的离线取证（卡 `openmir2-neighbor-actor-frame-drift`）。

对批次目录下的每个场景，读 `<scenario>/{csharp,rust}/frames.ndjson`（两侧各自方向），给出：

  * 每侧 ident 计数、**单侧多出的 ident 与条数**（`extra(oracle)` / `extra(rust)`）；
  * **首个顺序错位下标**与错位处两侧的 ident 序列（对齐口径与 `compare_runs.py` 相同：按文件顺序）；
  * 分类提示：`extra_tail`（多出帧全在尾部）/ `extra_mid`（多出帧在中间 ⇒ 后续整段错位）/
    `order_only`（两侧计数相同但顺序不同）/ `clean`（无差异）；
  * `extra_side` ∈ `oracle` / `rust` / **`both`**（两侧都多帧时必须报 `both`，否则 JSON 消费方
    会以为另一侧没多帧）。

**只读证据、只做分类**：不判「flaky vs 稳定差异」——那需要同一场景重复跑 ≥3 次，机器由 tester 排期；
本脚本的输出正是那 3 次重复要填的表。控制帧（ident 为 null 且 `frame_class=server_ack_time`）与
`compare_runs.py` 同口径先剔除。

用法（离线，不连库/不起栈/不占端口）：

    python -I -B tools/window/frame_drift_report.py <batch-dir>
    python -I -B tools/window/frame_drift_report.py <batch-dir> --json out.json
    python -I -B tools/window/frame_drift_report.py --selftest
"""
import io
import json
import os
import sys
import collections

SIDES = ("csharp", "rust")


def load_frames(path):
    """读一侧 frames.ndjson：剔除控制帧（ident 为 null 且 frame_class=server_ack_time）。"""
    out = []
    for line in io.open(path, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        rec = json.loads(line)
        if rec.get("ident") is None and rec.get("frame_class") == "server_ack_time":
            continue
        out.append(rec)
    return out


def side_counts(frames):
    return collections.Counter(fr["ident"] for fr in frames)


def first_mismatch(o, r):
    """按文件顺序对齐：首个 ident 不一致的下标；前 n=min(len) 全同则 None。"""
    n = min(len(o), len(r))
    for i in range(n):
        if o[i]["ident"] != r[i]["ident"]:
            return i
    return None


def drift_row(name, o, r):
    """一个场景的取证结果（纯函数，供 selftest 直接喂合成帧）。

    分类口径：多出帧**全在同一侧尾部** ⇔ 按序对齐时前 min(len) 帧完全一致（`first_mismatch=None`）；
    多出帧在中间 ⇒ 对齐必然在某个下标分叉（`extra_mid`，后续整段受影响）；
    两侧计数相同但顺序不同 ⇒ `order_only`（**不是**多帧，别按多帧处理）。
    """
    co, cr = side_counts(o), side_counts(r)
    extra_o = {i: co[i] - cr.get(i, 0) for i in sorted(co) if co[i] - cr.get(i, 0) > 0}
    extra_r = {i: cr[i] - co.get(i, 0) for i in sorted(cr) if cr[i] - co.get(i, 0) > 0}
    at = first_mismatch(o, r)
    if extra_o or extra_r:
        # F3（review）：两侧都多帧时必须报 `both`——只报 oracle 会让 JSON 消费方
        # 以为 rust 没多帧（move_walk 就是两侧各多一条不同来源的帧）。
        if extra_o and extra_r:
            side = "both"
        elif extra_o:
            side = "oracle"
        else:
            side = "rust"
        kind = "extra_tail" if at is None else "extra_mid"
    elif at is not None:
        side, kind = None, "order_only"
    else:
        side, kind = None, "clean"
    head = at if at is not None else min(len(o), len(r))
    return {
        "scenario": name,
        "oracle_frames": len(o),
        "rust_frames": len(r),
        "extra_oracle": extra_o,
        "extra_rust": extra_r,
        "first_mismatch": at,
        "kind": kind,
        "extra_side": side,
        "oracle_head": [fr["ident"] for fr in o[head:head + 8]],
        "rust_head": [fr["ident"] for fr in r[head:head + 8]],
    }


def walk_batch(base):
    rows = []
    for name in sorted(os.listdir(base)):
        d = os.path.join(base, name)
        if not os.path.isdir(d):
            continue
        o_path = os.path.join(d, "csharp", "frames.ndjson")
        r_path = os.path.join(d, "rust", "frames.ndjson")
        if not (os.path.exists(o_path) and os.path.exists(r_path)):
            rows.append({"scenario": name, "kind": "missing_frames",
                         "note": "缺 csharp/ 或 rust/ 的 frames.ndjson（未跑到 probe / 侧未完成）"})
            continue
        rows.append(drift_row(name, load_frames(o_path), load_frames(r_path)))
    return rows


def render(rows):
    lines = ["| 场景 | csharp | rust | 多出(csharp) | 多出(rust) | 首个错位 | 分类 |",
             "|---|---:|---:|---|---|---:|---|"]
    for r in rows:
        if r["kind"] == "missing_frames":
            lines.append("| %s | — | — | — | — | — | 缺产物 |" % r["scenario"])
            continue
        lines.append("| %s | %d | %d | %s | %s | %s | %s |"
                     % (r["scenario"], r["oracle_frames"], r["rust_frames"],
                        json.dumps(r["extra_oracle"], sort_keys=True),
                        json.dumps(r["extra_rust"], sort_keys=True),
                        "—" if r["first_mismatch"] is None else r["first_mismatch"],
                        r["kind"]))
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# selftest：六颗牙（尾多 / 尾多(另一侧) / 两侧都多 / 仅顺序不同 / 完全一致 / 中间多）
# ---------------------------------------------------------------------------
def _f(ident):
    return {"ident": ident}


def _selftest():
    checks = []

    def chk(cond, msg):
        checks.append((bool(cond), msg))

    f = _f
    base = [f(10), f(41), f(643), f(100), f(100)]

    # 牙1：oracle 尾部多 10/41 一对（W1 里 login_only/gm_batch 的形态）
    o, r = base + [f(10), f(41)], base
    row = drift_row("t1", o, r)
    chk(row["kind"] == "extra_tail" and row["extra_side"] == "oracle"
        and row["extra_oracle"] == {10: 1, 41: 1} and row["first_mismatch"] is None,
        "牙1 oracle 尾部多 10/41 => extra_tail / extra(oracle) / 尾部对齐不分叉")

    # 牙2：rust 尾部多 10/41（gm_batch 的形态）
    row = drift_row("t2", base, base + [f(10), f(41)])
    chk(row["kind"] == "extra_tail" and row["extra_side"] == "rust"
        and row["extra_rust"] == {10: 1, 41: 1},
        "牙2 rust 尾部多 10/41 => extra_tail / extra(rust)")

    # 牙6（F3）：两侧各多一条**不同 ident** 的帧 => extra_side 必须是 both
    o = base + [f(10), f(41)]
    r = base + [f(14)]
    row = drift_row("t6", o, r)
    chk(row["extra_side"] == "both" and row["extra_oracle"] == {10: 1, 41: 1}
        and row["extra_rust"] == {14: 1},
        "牙6 两侧都多帧（oracle +10/41、rust +14）=> extra_side=both（不误报成 oracle）")

    # 牙3：两侧计数相同、顺序不同（npc_click 的形态：643 提前）
    o = [f(10), f(41), f(643), f(654)]
    r = [f(643), f(10), f(41), f(654)]
    row = drift_row("t3", o, r)
    chk(row["kind"] == "order_only" and row["extra_oracle"] == {} and row["extra_rust"] == {}
        and row["first_mismatch"] == 0,
        "牙3 计数相同但顺序不同 => order_only（不是多帧）")

    # 牙4：完全一致 => clean；且中间多帧 => extra_mid（不是尾部）
    row = drift_row("t4", base, list(base))
    chk(row["kind"] == "clean" and row["first_mismatch"] is None, "牙4 完全一致 => clean")
    o = [f(10), f(41), f(10), f(41), f(643), f(100)]
    r = [f(10), f(41), f(643), f(100)]
    row = drift_row("t5", o, r)
    chk(row["kind"] == "extra_mid" and row["first_mismatch"] == 2,
        "牙5 中间多 10/41 一对 => extra_mid（后续整段错位）")
    return checks


def main(argv):
    if "--selftest" in argv:
        bad = 0
        for ok, msg in _selftest():
            print(("PASS " if ok else "FAIL ") + msg)
            bad += 0 if ok else 1
        total = len(_selftest())
        print(("GREEN" if bad == 0 else "RED") + ": frame_drift_report selftest %d/%d"
              % (total - bad, total))
        return 0 if bad == 0 else 1
    if not argv or argv[0].startswith("-"):
        print(__doc__)
        return 2
    base = argv[0]
    rows = walk_batch(base)
    print(render(rows))
    if "--json" in argv:
        out = argv[argv.index("--json") + 1]
        io.open(out, "w", encoding="utf-8", newline="\n").write(
            json.dumps({"batch": base, "rows": rows}, ensure_ascii=False, indent=2) + "\n")
        print("json ->", out)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
