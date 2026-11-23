#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""L2-5 ③ 段 4：容量切片摘要（perf TSV **按列名**读；fail-closed）。

卡：`openmir2-l25-s3-cross-recipient-count` 的段 4（总监 2026-11-22 三条硬要求）。

职责（只做可复算的投影，不下结论）：
  * **首屏**打印 `cross_recipient_missing` 合计 + 六面分解（合计必须等于六面之和，逐行校验），
    **并带本轮窗口（ticks/时长）与 `refmsg_targets` 分母 + 占比**（L2-5 ④a 的 A4 口径）；
  * 有 ④a 新列（`zone_cmd_unsupported` / `handoff_move_after_transfer_dropped` /
    `move_map_change_unsupported`）就逐列打印（旧二进制产物缺列 ⇒ 标注 absent，不判错）；
  * 读 perf TSV 的 `region_barrier_us`（max / mean）与 `region_barrier_total_us`（mean）；
  * 从 `load_gate` 的 report JSON 取 tick p50/p99（缺 ⇒ PREREQ，不静默）；
  * 从 RSS 采样 NDJSON 取 `game-svr` 的 WS/私有字节 max/mean；
  * 有效 tick 率 = tick 行数 ÷ run.json 的 `[start_wall_ms, end_wall_ms]`；
  * `--compare`：`RSS(2区) − RSS(1区)` 与每区粗估。

**重要口径（2026-11-22 总管踩坑）**：perf TSV 的每一行是**该 tick 的增量**（多数列写行即清零），
**不是累计值**——汇总必须**按列求和**，绝不能把最后一行当全轮读数（末行 `handoff_offered=0`
不代表全轮 0）。本脚本所有汇总都按列求和。

用法::

  python -I -B tools/botload/perf_tsv_summary.py --perf <perf.tsv> --run-json <run.json> \
      --report <tier report.json> [--rss <rss.ndjson>] [--json <out.json>] [--label 2zone]
  python -I -B tools/botload/perf_tsv_summary.py --compare <two.json> <one.json> --labels 2zone 1zone
  python -I -B tools/botload/perf_tsv_summary.py --selftest

退出码：0 = 摘要完整；2 = 输入不可判（缺列/无行/宽不一/JSON 坏/缺 report），**fail-closed**。

**旧产物警告（2026-11-23）**：writer 修复（vals 与表头列名对齐）**之前**的 perf TSV，按列名读在错位段
之后是错的；受影响列（⑤ outcome、handoff/zone/own-segment，以及 legacy 七面）必须先用
``tools/hygiene/tsv_recompute_legacy.py --tsv <perf_counters.tsv>`` 按**旧 vals 位置**回算。
本脚本的 legacy 分支只保证六面 / ``cross_recipient_missing``（错位段之前）与「seven = 六面」。
"""
import argparse
import json
import os
import sys
import tempfile

CROSS_COLS = [
    "cross_recipient_refmsg",
    "cross_recipient_run_frames",
    "cross_recipient_npc_merchant",
    "cross_recipient_feature_changed",
    "cross_recipient_mountride",
    "cross_recipient_merchant_disappear",
]
TOTAL_COL = "cross_recipient_missing"
# **L2-5 ⑦**：第七面（离世帧）与七面合计；`missing` 仍是六面口径（10.004% 只与它比）。
DEPART_COL = "cross_recipient_depart_frames"
SEVEN_COL = "cross_recipient_seven_total"
WRN_SUPP_COL = "depart_wrn_suppressed"
# **L2-5 follow-up**：镜像观察者被条件抑制的帧数（被抑制的帧不进 ⑤ 账）。
MIRROR_SUPP_COL = "depart_frames_mirror_suppressed"
# **L2-5 ⑤**：六面未命中的投递结果三桶（构造保证 delivered+unroutable+overflow == 六面合计）。
OUTCOME_COLS = [
    "cross_recipient_delivered",
    "cross_recipient_unroutable",
    "cross_frame_queue_overflow",
]
# ⑤ 的 canary/诊断列（有就打印；旧产物 absent）。
DIAG_COLS = [
    "cross_frame_dup",
    "cross_frame_drop_closed",
    "cross_recipient_zone_unknown",
]
# **L2-5 ⑤ 成本探针**：本 tick 跨区入队总耗时（µs）与次数；窗口打印每事件 us。
COST_COLS = [
    "cross_enqueue_us",
    "cross_enqueue_events",
]
# L2-5 ③ 段 4 追加的 handoff 列：有就打印（新二进制），没有就标注 absent（旧产物仍可读）。
HANDFOFF_COLS = [
    "handoff_offered",
    "handoff_accepted",
    "handoff_refused",
    "handoff_promoted",
    "handoff_inserted_fresh",
    # 段 4 裁决：该列**不是** handoff 专属指标——「镜像 id 撞本区真实体」，包含静态图实体撞名。
    "mirror_shadowed_by_local",
    "handoff_dup_dropped",
    "mirror_write_refused_cell",
    "mirror_write_refused_occupied",
    # **own-segment 修正**：信息性列（本段 id 的镜像/回迁插入；非 0 正常，突发变化才查）。
    "mirror_own_segment_ids",
    "handoff_fresh_own_segment",
]
# L2-5 ④a 追加的区 k 缺口列：有就打印（新二进制），没有就标注 absent（旧产物仍可读）。
ZONE_K_COLS = [
    "zone_cmd_unsupported",
    "handoff_move_after_transfer_dropped",
    "move_map_change_unsupported",
]
REQUIRED = CROSS_COLS + [
    TOTAL_COL,
    "region_barrier_us",
    "region_barrier_total_us",
    "inbound_enqueued",
    "inbound_drained",
    "inbound_carry",
    "inbound_budget_hit",
    "refmsg_calls",
    "refmsg_targets",
]


class InputError(Exception):
    pass


def _read_tsv(path):
    """按**表头名**读列（不按位置）；缺列/无行/宽不一 ⇒ InputError（rc=2）。"""
    if not os.path.exists(path):
        raise InputError("perf TSV 不存在：%s" % path)
    header = None
    rows = []
    with open(path, "r", encoding="utf-8", newline="") as fh:
        for lineno, line in enumerate(fh, 1):
            line = line.rstrip("\r\n")
            if not line:
                continue
            cells = line.split("\t")
            if header is None:
                header = cells
                missing = [c for c in REQUIRED if c not in header]
                if missing:
                    raise InputError("perf TSV 缺列（按名读，不按位置）：%s" % ",".join(missing))
                continue
            if len(cells) != len(header):
                raise InputError("第 %d 行列数 %d != 表头 %d（宽不一）" % (lineno, len(cells), len(header)))
            rows.append(cells)
    if header is None:
        raise InputError("perf TSV 为空（无表头）：%s" % path)
    if not rows:
        raise InputError("perf TSV 只有表头、无 tick 行（探针 fail-closed）：%s" % path)
    optional = [c for c in HANDFOFF_COLS if c in header]
    optional += [c for c in ZONE_K_COLS if c in header]
    optional += [c for c in OUTCOME_COLS if c in header]
    optional += [c for c in DIAG_COLS if c in header]
    optional += [c for c in COST_COLS if c in header]
    optional += [c for c in (DEPART_COL, SEVEN_COL, WRN_SUPP_COL) if c in header]
    want = REQUIRED + optional
    idx = {name: header.index(name) for name in want}
    out = {name: [] for name in want}
    for cells in rows:
        for name in want:
            raw = cells[idx[name]].strip()
            try:
                out[name].append(int(raw))
            except ValueError:
                raise InputError("列 %r 出现非整数：%r" % (name, raw))
    return out


def _load_json(path, what):
    try:
        with open(path, "r", encoding="utf-8-sig") as fh:
            return json.load(fh)
    except (OSError, ValueError) as exc:
        raise InputError("%s 读不了：%s（%s）" % (what, path, exc))


def _tick_percentiles(report, bots):
    """从 load report 里取 tick 分位。兼容 `{tiers:[...]}` 与单 tier；按 `count` 选档。"""
    tiers = report.get("tiers") if isinstance(report, dict) else None
    if not tiers:
        tiers = [report] if isinstance(report, dict) else []
    if not tiers:
        raise InputError("report 里没有 tiers")
    chosen = None
    for t in tiers:
        if bots is None or t.get("count") == bots:
            chosen = t
            break
    if chosen is None:
        chosen = tiers[0]
    if chosen.get("tick_p50_ms") is None:
        raise InputError("report 该档缺 tick_p50_ms（缺样本 ⇒ 不可判）")
    return {
        "count": chosen.get("count"),
        "tick_p50_ms": chosen.get("tick_p50_ms"),
        "tick_p90_ms": chosen.get("tick_p90_ms"),
        "tick_p99_ms": chosen.get("tick_p99_ms"),
        "max_ms": chosen.get("max_ms"),
        "login_pct": chosen.get("login_pct"),
        "world_ok": chosen.get("world_ok"),
        "verdict": chosen.get("verdict"),
    }


def _rss(game_role, path):
    """RSS 采样 NDJSON → game-svr 的 ws/priv max/mean（MB）。缺 game-svr 样本 ⇒ InputError。"""
    if not path:
        return None
    rows = []
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except ValueError:
                raise InputError("rss NDJSON 坏行：%r" % line[:80])
    mine = [r for r in rows if str(r.get("role", "")) == game_role]
    if not mine:
        roles = sorted({str(r.get("role", "")) for r in rows})
        raise InputError("rss 采样里没有 role=%r（实有 %s）" % (game_role, roles))

    def stat(key):
        vals = [int(r[key]) for r in mine if r.get(key) is not None]
        if not vals:
            raise InputError("rss 采样缺 %r" % key)
        return {"max_mb": round(max(vals) / 1048576.0, 2), "mean_mb": round(sum(vals) / len(vals) / 1048576.0, 2), "samples": len(vals)}

    return {"ws": stat("ws_bytes"), "priv": stat("priv_bytes")}


def summarize(perf_path, run_json=None, report=None, rss_path=None, bots=None, label=None, game_role="game-svr"):
    perf = _read_tsv(perf_path)
    rows = len(perf[CROSS_COLS[0]])
    # **六面是权威口径**：合计 = 六面之和（合计列只是冗余展示）。列值不一致 ⇒ 记 info
    # （`total_column_mismatch`），不判 PREREQ——2026-11-22 实测记到过 writer 把合计恒写 0
    # 的 bug；读数不能被冗余列绑架，但差异必须显式暴露。
    cross = {c: sum(perf[c]) for c in CROSS_COLS}
    cross["total"] = sum(cross[c] for c in CROSS_COLS)
    total_col = sum(perf[TOTAL_COL])
    total_mismatch = None
    if total_col != cross["total"]:
        first = next(i for i in range(rows)
                     if perf[TOTAL_COL][i] != sum(perf[c][i] for c in CROSS_COLS))
        total_mismatch = {
            "column_sum": total_col,
            "six_sum": cross["total"],
            "first_bad_row": first + 1,
        }
    # **L2-5 ⑦**：七面口径。`missing` 仍是六面（历史 10.004% 只与它比）；第七面与七面合计单列。
    depart = sum(perf[DEPART_COL]) if DEPART_COL in perf else None
    seven = (
        sum(perf[SEVEN_COL])
        if SEVEN_COL in perf
        else (cross["total"] + (depart or 0))
    )
    # **L2-5 ⑤/⑦**：三桶 + 恒等式（只在三列齐全时校验；旧产物缺列 ⇒ absent，不判错）。
    outcome = None
    if all(c in perf for c in OUTCOME_COLS):
        outcome = {c: sum(perf[c]) for c in OUTCOME_COLS}
        outcome["total"] = sum(outcome[c] for c in OUTCOME_COLS)
        # **窗口合计**校验：跨线程 increment（区 k）与 `write_tick` 的逐行 swap 可能错一拍，
        # 但每个 incidence 同时给「一个面」与「一个结果桶」各 +1 ⇒ **窗口合计严格相等**。
        if SEVEN_COL in perf:
            # 七面产物：`seven_total == 三桶`；且 `seven_total == 六面小计 + 第七面`。
            for i in range(rows):
                sv = perf[SEVEN_COL][i]
                six = sum(perf[c][i] for c in CROSS_COLS)
                buckets = sum(perf[c][i] for c in OUTCOME_COLS)
                if sv != 0 and six != 0 and sv != buckets:
                    raise InputError(
                        "七面恒等式破坏（第 %d 行）：seven_total=%d != delivered+unroutable+overflow=%d"
                        % (i + 1, sv, buckets)
                    )
                if DEPART_COL in perf and sv != six + perf[DEPART_COL][i]:
                    raise InputError(
                        "七面合计 != 六面小计 + 第七面（第 %d 行）：seven=%d six=%d depart=%d"
                        % (i + 1, sv, six, perf[DEPART_COL][i])
                    )
        else:
            # ⑤-era 产物（有三桶、无第七面）：按六面口径校验；注意 ⑤ 加列时的历史
            # writer 有列错位，这类产物本身不可信，恒等式破坏即 fail-closed。
            if outcome["total"] != cross["total"]:
                raise InputError(
                    "三桶恒等式破坏（窗口合计）：delivered+unroutable+overflow=%d != 六面=%d"
                    % (outcome["total"], cross["total"])
                )
    present_diag = [c for c in DIAG_COLS if c in perf]
    cost = None
    if all(c in perf for c in COST_COLS):
        us = sum(perf["cross_enqueue_us"])
        events = sum(perf["cross_enqueue_events"])
        cost = {
            "us_total": us,
            "events_total": events,
            "us_per_event": round(us / events, 3) if events else None,
        }
    summary = {
        "label": label,
        "perf_tsv": os.path.abspath(perf_path),
        "tick_rows": rows,
        "cross_recipient_missing": cross,
        "cross_recipient_depart_frames": depart,
        "cross_recipient_seven_total": seven,
        "cross_recipient_outcome": outcome,
        "cross_frame_diag": ({c: sum(perf[c]) for c in present_diag} if present_diag else None),
        "depart_wrn_suppressed": (sum(perf[WRN_SUPP_COL]) if WRN_SUPP_COL in perf else None),
        "depart_frames_mirror_suppressed": (
            sum(perf[MIRROR_SUPP_COL]) if MIRROR_SUPP_COL in perf else None),
        "cross_enqueue": cost,
        "total_column_mismatch": total_mismatch,
        "region_barrier_us": {
            "max": max(perf["region_barrier_us"]),
            "mean": round(sum(perf["region_barrier_us"]) / rows, 1),
        },
        "region_barrier_total_us": {
            "mean_per_tick": round(sum(perf["region_barrier_total_us"]) / rows, 1),
        },
        "inbound": {
            "enqueued_mean": round(sum(perf["inbound_enqueued"]) / rows, 2),
            "drained_mean": round(sum(perf["inbound_drained"]) / rows, 2),
            "carry_ticks": sum(1 for v in perf["inbound_carry"] if v > 0),
            "budget_hit_ticks": sum(1 for v in perf["inbound_budget_hit"] if v > 0),
        },
        "refmsg": {
            "calls_total": sum(perf["refmsg_calls"]),
            "targets_total": sum(perf["refmsg_targets"]),
        },
    }
    present_z = [c for c in ZONE_K_COLS if c in perf]
    summary["zone_k"] = {c: sum(perf[c]) for c in present_z} if present_z else None
    present = [c for c in HANDFOFF_COLS if c in perf]
    if len(present) == len(HANDFOFF_COLS):
        summary["handoff"] = {c: sum(perf[c]) for c in HANDFOFF_COLS}
    else:
        summary["handoff"] = None
    if run_json:
        run = _load_json(run_json, "run.json")
        try:
            start = int(run["start_wall_ms"])
            end = int(run["end_wall_ms"])
        except (KeyError, TypeError, ValueError):
            raise InputError("run.json 缺 start_wall_ms/end_wall_ms（整数）")
        dur = (end - start) / 1000.0
        if dur <= 0:
            raise InputError("run.json 时长非正：start=%d end=%d" % (start, end))
        summary["run"] = {"start_wall_ms": start, "end_wall_ms": end, "duration_sec": round(dur, 3)}
        summary["effective_tick_hz"] = round(rows / dur, 3)
    if report:
        rep = _load_json(report, "load report")
        summary["tick"] = _tick_percentiles(rep, bots)
    if rss_path:
        summary["rss"] = {game_role: _rss(game_role, rss_path)}
    return summary


def _window_text(s):
    """本轮窗口：ticks 恒有；时长只在给了 run.json 时才有（缺 ⇒ 明写 unknown，不静默）。"""
    if "run" in s:
        return "ticks=%d 时长=%.1fs" % (s["tick_rows"], s["run"]["duration_sec"])
    return "ticks=%d 时长=unknown（未给 run.json）" % s["tick_rows"]


def print_summary(s):
    cross = s["cross_recipient_missing"]
    print("=== L2-5 ③ 容量切片摘要 (%s) ===" % (s.get("label") or "unlabeled"))
    # 首屏第一行：合计 + 六面分解（总监硬要求 1）+ ④a 的 A4 分母口径：
    # 分母 = 同一投递段的候选收件人次 `refmsg_targets`，占比 = 合计/分母；窗口 = ticks/时长。
    denom = s["refmsg"]["targets_total"]
    pct = "%.3f%%" % (100.0 * cross["total"] / denom) if denom > 0 else "n/a"
    print(
        "cross_recipient_missing（合计）= %d / 分母 refmsg_targets=%d = %s（窗口 %s）  [%s]"
        % (
            cross["total"],
            denom,
            pct,
            _window_text(s),
            " ".join("%s=%d" % (c.replace("cross_recipient_", ""), cross[c]) for c in CROSS_COLS),
        )
    )
    six = cross["total"]
    depart = s.get("cross_recipient_depart_frames")
    seven = s.get("cross_recipient_seven_total")
    if depart is None and seven is None:
        print("cross windows: missing(六面)=%d | 第七面 absent（旧产物；七面合计=六面）" % six)
    else:
        print(
            "cross windows: missing(六面)=%d | depart_frames=%s | seven_total=%s"
            "（10.004%% 只对六面小计）"
            % (six, "absent" if depart is None else depart,
               "absent" if seven is None else seven))
    oc = s.get("cross_recipient_outcome")
    if oc:
        print(
            "cross outcome: delivered=%d unroutable=%d overflow=%d | seven_total=%d"
            "（= 六面 %d + 第七面 %s；恒等式 OK）"
            % (oc["cross_recipient_delivered"], oc["cross_recipient_unroutable"],
               oc["cross_frame_queue_overflow"], seven,
               six, "absent" if depart is None else depart))
    else:
        print("cross outcome: absent（旧二进制产物，无三桶列）")
    dg = s.get("cross_frame_diag")
    if dg:
        supp = s.get("depart_wrn_suppressed") or 0
        msupp = s.get("depart_frames_mirror_suppressed")
        print("cross diag: dup=%d drop_closed=%d zone_unknown=%d | depart_wrn_suppressed=%d"
              " | depart_frames_mirror_suppressed=%s" % (
                  dg["cross_frame_dup"], dg["cross_frame_drop_closed"],
                  dg["cross_recipient_zone_unknown"], supp,
                  "absent" if msupp is None else msupp))
    ce = s.get("cross_enqueue")
    if ce:
        per = "%.3f" % ce["us_per_event"] if ce["us_per_event"] is not None else "n/a"
        print("cross enqueue: events=%d total_us=%d per_event_us=%s" % (
            ce["events_total"], ce["us_total"], per))
    if s.get("total_column_mismatch"):
        m = s["total_column_mismatch"]
        print("NOTE total 列与六面之和不一致（合计列冗余，按六面计）：列=%d 六面=%d 首个坏行=%d"
              % (m["column_sum"], m["six_sum"], m["first_bad_row"]))
    tail = []
    if "effective_tick_hz" in s:
        tail.append("有效 tick 率=%.2f/s (rows=%d, %.1fs)" % (s["effective_tick_hz"], s["tick_rows"], s["run"]["duration_sec"]))
    if "tick" in s:
        t = s["tick"]
        tail.append("tick p50=%sms p99=%sms max=%sms" % (t["tick_p50_ms"], t["tick_p99_ms"], t["max_ms"]))
    tail.append("region_barrier_us max=%dµs mean=%.1fµs" % (s["region_barrier_us"]["max"], s["region_barrier_us"]["mean"]))
    tail.append("region_barrier_total_us mean=%.1fµs/tick" % s["region_barrier_total_us"]["mean_per_tick"])
    print(" | ".join(tail))
    if "rss" in s:
        for role, r in s["rss"].items():
            print("RSS(%s): ws max=%.2fMB mean=%.2fMB | priv max=%.2fMB mean=%.2fMB" % (
                role, r["ws"]["max_mb"], r["ws"]["mean_mb"], r["priv"]["max_mb"], r["priv"]["mean_mb"]))
    h = s.get("handoff")
    if h:
        print("handoff: offered=%d accepted=%d refused=%d | promoted=%d inserted_fresh=%d dup_dropped=%d" % (
            h["handoff_offered"], h["handoff_accepted"], h["handoff_refused"],
            h["handoff_promoted"], h["handoff_inserted_fresh"], h["handoff_dup_dropped"]))
        print("mirror: shadowed_by_local=%d（含静态图实体撞名，非 handoff 专属、不是红线） | write_refused cell=%d occupied=%d" % (
            h["mirror_shadowed_by_local"], h["mirror_write_refused_cell"],
            h["mirror_write_refused_occupied"]))
    else:
        print("handoff: absent（本 TSV 无 handoff 列：旧二进制产物）")
    zk = s.get("zone_k")
    if zk:
        print("zone-k 缺口: cmd_unsupported=%d post_transfer_move_dropped=%d map_change_unsupported=%d" % (
            zk.get("zone_cmd_unsupported", 0),
            zk.get("handoff_move_after_transfer_dropped", 0),
            zk.get("move_map_change_unsupported", 0),
        ))
    else:
        print("zone-k 缺口: absent（本 TSV 无 ④a 列：旧二进制产物）")
    ib = s["inbound"]
    print("inbound: enq=%.2f/tick drained=%.2f/tick carry_ticks=%d budget_hit_ticks=%d" % (
        ib["enqueued_mean"], ib["drained_mean"], ib["carry_ticks"], ib["budget_hit_ticks"]))
    print("refmsg: calls=%d targets=%d" % (s["refmsg"]["calls_total"], s["refmsg"]["targets_total"]))


def compare(a, b, labels):
    ra, rb = a.get("rss"), b.get("rss")
    if not ra or not rb:
        raise InputError("compare 需要两份摘要都带 rss")
    role = next(iter(ra))
    if role not in rb:
        raise InputError("两份摘要的 RSS 角色不一致（%s vs %s）" % (role, sorted(rb)))
    za, zb = a.get("zones"), b.get("zones")
    if za is None or zb is None or za <= zb:
        raise InputError("compare 需要 zones 字段且 A 区数 > B 区数（实得 %r / %r）" % (za, zb))
    d_ws = ra[role]["ws"]["max_mb"] - rb[role]["ws"]["max_mb"]
    d_priv = ra[role]["priv"]["max_mb"] - rb[role]["priv"]["max_mb"]
    per_zone = d_ws / (za - zb)
    print("=== 内存伸缩（%s %d 区 − %s %d 区，同机同时长）===" % (labels[0], za, labels[1], zb))
    print("RSS(%s) ws: %.2fMB − %.2fMB = Δ%.2fMB；priv: Δ%.2fMB" % (role, ra[role]["ws"]["max_mb"], rb[role]["ws"]["max_mb"], d_ws, d_priv))
    print("每区粗估（ws）：约 %.2fMB/区（Δ÷(%d−%d)）" % (per_zone, za, zb))
    return {"role": role, "delta_ws_mb": round(d_ws, 2), "delta_priv_mb": round(d_priv, 2), "per_zone_ws_mb": round(per_zone, 2)}


# --------------------------- selftest ---------------------------
def _write(path, text):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)


def _tsv(header, rows):
    out = ["\t".join(header)]
    for r in rows:
        out.append("\t".join(str(v) for v in r))
    return "\n".join(out) + "\n"


def selftest():
    ok = True

    def expect(cond, msg):
        nonlocal ok
        if not cond:
            ok = False
            print("SELFTEST_FAIL: " + msg)

    with tempfile.TemporaryDirectory() as td:
        # 故意把列顺序打散：证明按名读、不按位置
        header = ["region_barrier_us"] + list(reversed(CROSS_COLS)) + [
            "region_barrier_total_us", "inbound_enqueued", "inbound_drained", "inbound_carry",
            "inbound_budget_hit", "refmsg_calls", "refmsg_targets", TOTAL_COL,
        ] + list(OUTCOME_COLS) + list(DIAG_COLS) + list(COST_COLS) + [
            DEPART_COL, SEVEN_COL, WRN_SUPP_COL,
        ] + list(reversed(ZONE_K_COLS))
        row = {c: 0 for c in header}
        row["region_barrier_us"] = 120
        row["region_barrier_total_us"] = 240
        row["inbound_enqueued"] = 10
        row["inbound_drained"] = 9
        row["refmsg_calls"] = 2
        row["refmsg_targets"] = 5
        row["cross_recipient_refmsg"] = 1
        row["cross_recipient_mountride"] = 2
        row[TOTAL_COL] = 3  # = 1 + 2
        # ⑤ 三桶：每行 delivered(2)+unroutable(1)+overflow(0) == 六面(3)
        # ⑦：六面 3 + 第七面 1 = 七面 4；三桶 3+1+0 = 4。
        row["cross_recipient_delivered"] = 3
        row["cross_recipient_unroutable"] = 1
        row["cross_frame_queue_overflow"] = 0
        row[DEPART_COL] = 1
        row[SEVEN_COL] = 4
        row["cross_frame_dup"] = 0
        row["cross_frame_drop_closed"] = 0
        row["cross_recipient_zone_unknown"] = 0
        row[WRN_SUPP_COL] = 0
        row["cross_enqueue_us"] = 100
        row["cross_enqueue_events"] = 4
        row["zone_cmd_unsupported"] = 3
        row["handoff_move_after_transfer_dropped"] = 1
        row["move_map_change_unsupported"] = 0
        rows = [dict(row), dict(row)]
        perf = os.path.join(td, "perf.tsv")
        _write(perf, _tsv(header, [[r[h] for h in header] for r in rows]))
        run = os.path.join(td, "run.json")
        _write(run, json.dumps({"start_wall_ms": 0, "end_wall_ms": 400}))
        rep = os.path.join(td, "report.json")
        _write(rep, json.dumps({"tiers": [{"count": 1000, "tick_p50_ms": 41, "tick_p90_ms": 80, "tick_p99_ms": 96, "max_ms": 120}]}))
        rss = os.path.join(td, "rss.ndjson")
        _write(rss, "\n".join([
            json.dumps({"role": "game-svr", "ws_bytes": 100 * 1048576, "priv_bytes": 80 * 1048576}),
            json.dumps({"role": "game-svr", "ws_bytes": 200 * 1048576, "priv_bytes": 90 * 1048576}),
            json.dumps({"role": "login-srv", "ws_bytes": 10 * 1048576, "priv_bytes": 5 * 1048576}),
        ]) + "\n")
        s = summarize(perf, run, rep, rss, bots=1000, label="2zone")
        expect(s["cross_recipient_missing"]["total"] == 6, "合计应为 6")
        expect(s["cross_recipient_missing"]["cross_recipient_mountride"] == 4, "mountride 应为 4")
        expect(s["effective_tick_hz"] == 5.0, "2 行 / 0.4s 应为 5.0/s")
        expect(s["tick"]["tick_p50_ms"] == 41, "p50 应取 count=1000 档")
        expect(s["rss"]["game-svr"]["ws"]["max_mb"] == 200.0, "RSS ws max 应为 200MB")
        expect(s["region_barrier_us"]["max"] == 120, "barrier max 应为 120")
        expect(s["zone_k"]["zone_cmd_unsupported"] == 6, "④a 列按名求和（3×2 行）")

        # A4 口径：首屏第一行必带窗口（ticks/时长）+ refmsg_targets 分母 + 占比。
        import contextlib
        import io as _io
        buf = _io.StringIO()
        with contextlib.redirect_stdout(buf):
            print_summary(s)
        first = buf.getvalue().splitlines()[1]  # 第 1 行是标题，A4 口径在首屏第二行
        expect("分母 refmsg_targets=10" in first, "首屏必须带 refmsg_targets 分母：%s" % first)
        expect("窗口 ticks=2 时长=0.4s" in first, "首屏必须带窗口（ticks/时长）：%s" % first)
        expect("= 60.000%" in first, "占比 = 6/10：%s" % first)
        expect("zone-k 缺口" in buf.getvalue(), "④a 列必须打印")
        expect("delivered=6 unroutable=2 overflow=0" in buf.getvalue(),
               "⑤ 三桶必须同屏打印：%s" % buf.getvalue())
        expect("missing(六面)=6 | depart_frames=2 | seven_total=8" in buf.getvalue(),
               "⑦ 六面/第七面/七面合计必须同屏：%s" % buf.getvalue())
        expect("恒等式 OK" in buf.getvalue(), "⑤ 恒等式必须显式展示")
        expect("cross enqueue: events=8 total_us=200 per_event_us=25.000" in buf.getvalue(),
               "⑤ 每事件成本必须打印：%s" % buf.getvalue())

        # 无 run.json ⇒ 时长明写 unknown（不静默丢窗口）。
        s_norun = summarize(perf, None, rep, None, bots=1000, label="2zone")
        buf2 = _io.StringIO()
        with contextlib.redirect_stdout(buf2):
            print_summary(s_norun)
        first2 = buf2.getvalue().splitlines()[1]
        expect("窗口 ticks=2 时长=unknown" in first2, "缺 run.json 时长必须明写 unknown：%s" % first2)

        # 负例 1：缺列（把 total 列改名）⇒ rc=2
        bad = perf + ".bad.tsv"
        _write(bad, _tsv([h for h in header if h != TOTAL_COL], [[r[h] for h in header if h != TOTAL_COL] for r in rows]))
        try:
            summarize(bad)
            expect(False, "缺列必须 InputError")
        except InputError as e:
            expect(TOTAL_COL in str(e), "缺列报错必须点名列：%s" % e)

        # 负例 2：合计列与六面不等 ⇒ 六面权威、差异显式暴露（不再判 PREREQ）
        drift = os.path.join(td, "drift.tsv")
        r2 = [dict(row), dict(row)]
        r2[0][TOTAL_COL] = 99
        _write(drift, _tsv(header, [[r[h] for h in header] for r in r2]))
        sd = summarize(drift)
        expect(sd["total_column_mismatch"] is not None, "合计列漂移必须被显式记录")
        expect(sd["total_column_mismatch"]["six_sum"] == 6, "六面之和应为 6（权威口径）")
        expect(sd["cross_recipient_missing"]["total"] == 6, "合计应按六面之和计")

        # 负例 2b（⑤）：三桶与六面不等 ⇒ rc=2（构造保证，破坏即缺陷）
        broke = os.path.join(td, "identity_broke.tsv")
        rb = [dict(row), dict(row)]
        rb[0]["cross_recipient_delivered"] = 1  # 1+1+0=2 != 七面 4
        _write(broke, _tsv(header, [[r[h] for h in header] for r in rb]))
        try:
            summarize(broke)
            expect(False, "三桶恒等式破坏必须 InputError")
        except InputError as e:
            expect("七面恒等式破坏" in str(e), "必须点名恒等式：%s" % e)

        # 负例 2c（⑤）：旧产物无三桶列 ⇒ absent（不判错）
        legacy = os.path.join(td, "legacy_no_outcome.tsv")
        lh = [h for h in header
              if h not in OUTCOME_COLS and h not in DIAG_COLS and h not in COST_COLS
              and h not in (DEPART_COL, SEVEN_COL, WRN_SUPP_COL)]
        _write(legacy, _tsv(lh, [[r[h] for h in lh] for r in [dict(row), dict(row)]]))
        sl = summarize(legacy)
        expect(sl["cross_recipient_outcome"] is None, "旧产物应 absent")
        buf3 = _io.StringIO()
        with contextlib.redirect_stdout(buf3):
            print_summary(sl)
        expect("cross outcome: absent" in buf3.getvalue(), "旧产物必须标 absent")

        # 负例 3：只有表头 ⇒ rc=2
        empty = os.path.join(td, "empty.tsv")
        _write(empty, _tsv(header, []))
        try:
            summarize(empty)
            expect(False, "无 tick 行必须 InputError")
        except InputError:
            pass

        # 负例 4：report 缺 p50 ⇒ rc=2（缺样本不可判）
        badrep = os.path.join(td, "badrep.json")
        _write(badrep, json.dumps({"tiers": [{"count": 1000, "tick_p50_ms": None}]}))
        try:
            summarize(perf, run, badrep)
            expect(False, "缺 p50 必须 InputError")
        except InputError:
            pass

        # compare：2 区 200MB vs 1 区 150MB ⇒ Δ50MB、每区 50MB
        s1 = json.loads(json.dumps(s))
        s1["zones"] = 2
        s2 = json.loads(json.dumps(s))
        s2["zones"] = 1
        s2["rss"]["game-svr"]["ws"]["max_mb"] = 150.0
        d = compare(s1, s2, ("2zone", "1zone"))
        expect(d["delta_ws_mb"] == 50.0 and d["per_zone_ws_mb"] == 50.0, "RSS 差与每区粗估")
        s2["zones"] = 2
        try:
            compare(s1, s2, ("a", "b"))
            expect(False, "区数不递减必须 InputError")
        except InputError:
            pass

    print("SELFTEST " + ("OK" if ok else "FAIL"))
    return 0 if ok else 2


def main(argv):
    ap = argparse.ArgumentParser(description="L2-5 ③ 段 4 容量切片摘要（按列名读；fail-closed）")
    ap.add_argument("--perf")
    ap.add_argument("--run-json")
    ap.add_argument("--report")
    ap.add_argument("--rss")
    ap.add_argument("--bots", type=int)
    ap.add_argument("--label")
    ap.add_argument("--zones", type=int)
    ap.add_argument("--json", dest="out_json")
    ap.add_argument("--compare", nargs=2, metavar=("A", "B"))
    ap.add_argument("--labels", nargs=2)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    try:
        if args.selftest:
            return selftest()
        if args.compare:
            a = _load_json(args.compare[0], "摘要 A")
            b = _load_json(args.compare[1], "摘要 B")
            labels = args.labels or (a.get("label") or "A", b.get("label") or "B")
            if args.out_json:
                d = compare(a, b, labels)
                d["labels"] = labels
                _write_json(args.out_json, d)
            else:
                compare(a, b, labels)
            return 0
        if not args.perf:
            raise InputError("缺 --perf（或 --selftest / --compare）")
        if not args.report:
            raise InputError("缺 --report（容量摘要必须带 tick p50/p99，缺样本不可判）")
        s = summarize(args.perf, args.run_json, args.report, args.rss, args.bots, args.label)
        s["zones"] = args.zones
        print_summary(s)
        if args.out_json:
            _write_json(args.out_json, s)
        return 0
    except InputError as e:
        print("PREREQ: " + str(e), file=sys.stderr)
        return 2


def _write_json(path, obj):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(obj, fh, ensure_ascii=False, indent=2)
        fh.write("\n")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
