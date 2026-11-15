#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""counters_series — 把 `dotnet-counters collect` 的 CSV 复算成 A8-trend 报告段。

卡：`openmir2-botsrv-loh-growth`（第一交付物：先用已归档的两轮 CSV 做时间序列分析）。
判据定义见 `docs/capacity-model-20261114.md`「内存判据两档」与
`docs/memory-report-template-20261115.md`。本工具只做**可复算的投影**，不做结论：

  * LOH 端点变化、稳态单点值、窗口 OLS 斜率；
  * 全部 samples 数、跨度、Gen2 时刻表（rate 计数 > 0 的样本）；
  * Allocation Rate 的 mean / median / p95 / max 与峰值时刻；
  * LOH 的最大单步跳变（启动批量分配的可观测形状）；
  * LOH 增长步长的主间隔（默认 5s = LoadMetrics 的 flush 周期，供归因交叉核对）。

用法::

    python -I -B tools/botload/counters_series.py <counters.csv> [more.csv ...] [--ramp-end 60] [--json]

退出码：0 成功；2 用法错；3 输入不可判（缺计数器 / 无样本 / 时间戳坏）。
`--selftest` 跑合成夹具，不读真实归档。
"""
import argparse
import csv
import datetime
import json
import os
import statistics
import sys
import tempfile

LOH = "LOH Size (B)"
ALLOC = "Allocation Rate (B / 1 sec)"
GEN2 = "Gen 2 GC Count (Count / 1 sec)"
GEN0 = "Gen 0 GC Count (Count / 1 sec)"
GEN1 = "Gen 1 GC Count (Count / 1 sec)"
REQUIRED = (LOH, ALLOC, GEN2)
TS_FMT = "%m/%d/%Y %H:%M:%S"


class InputError(Exception):
    pass


def _parse_ts(s):
    try:
        return datetime.datetime.strptime(s, TS_FMT)
    except ValueError:
        raise InputError("时间戳不符合 %r：%r" % (TS_FMT, s))


def load_series(path):
    """返回 {counter: [(datetime, value)]}；同秒多样本保留首条（counters 每个刷新周期一条）。"""
    if not os.path.exists(path):
        raise InputError("文件不存在：%s" % path)
    series = {}
    with open(path, "r", encoding="utf-8-sig", newline="") as fh:
        reader = csv.DictReader(fh)
        if not reader.fieldnames or "Timestamp" not in reader.fieldnames:
            raise InputError("CSV 缺表头/缺 Timestamp 列：%s" % path)
        for row in reader:
            if (row.get("Provider") or "") != "System.Runtime":
                continue
            name = row.get("Counter Name") or ""
            raw = row.get("Mean/Increment")
            try:
                val = float(raw)
            except (TypeError, ValueError):
                continue
            ts = _parse_ts(row["Timestamp"])
            bucket = series.setdefault(name, [])
            if bucket and bucket[-1][0] == ts:
                continue
            bucket.append((ts, val))
    if not series:
        raise InputError("没有 System.Runtime 样本：%s" % path)
    for name in REQUIRED:
        if name not in series or not series[name]:
            raise InputError("缺计数器 %r：%s" % (name, path))
    return series


def _ols(points):
    """points: [(x_seconds, y)] -> slope_per_s；样本 <2 或 x 无跨度 ⇒ None。"""
    n = len(points)
    if n < 2:
        return None
    mx = sum(x for x, _ in points) / n
    my = sum(y for _, y in points) / n
    den = sum((x - mx) ** 2 for x, _ in points)
    if den == 0:
        return None
    return sum((x - mx) * (y - my) for x, y in points) / den


def _pct(sorted_vals, q):
    if not sorted_vals:
        return None
    idx = int(len(sorted_vals) * q)
    if idx >= len(sorted_vals):
        idx = len(sorted_vals) - 1
    return sorted_vals[idx]


def analyze(path, ramp_end=60.0):
    series = load_series(path)
    loh_raw = series[LOH]
    t0 = loh_raw[0][0]
    loh = [((ts - t0).total_seconds(), val / 1e6) for ts, val in loh_raw]
    span = loh[-1][0]

    endpoints = {"first_mb": round(loh[0][1], 3), "last_mb": round(loh[-1][1], 3),
                 "delta_mb": round(loh[-1][1] - loh[0][1], 3)}

    def at(t):
        return min(loh, key=lambda p: abs(p[0] - t))

    steady_point = at(ramp_end)

    def slope(lo, hi):
        pts = [(x, y) for x, y in loh if lo <= x <= hi]
        s = _ols(pts)
        return {"n": len(pts), "lo_s": lo, "hi_s": hi,
                "first_mb": round(pts[0][1], 3) if pts else None,
                "last_mb": round(pts[-1][1], 3) if pts else None,
                "ols_mb_per_s": round(s, 5) if s is not None else None}

    windows = {
        "post_ramp_to_end": slope(ramp_end, span),
        "steady_first_240s": slope(ramp_end, min(ramp_end + 240.0, span)),
        "late_tail": slope(max(span - 60.0, ramp_end), span),
    }

    deltas = [(loh[i][0], loh[i][1] - loh[i - 1][1]) for i in range(1, len(loh))]
    max_step = max(deltas, key=lambda p: p[1]) if deltas else None
    steady_hi = min(ramp_end + 240.0, span)
    rise_times = [t for t, d in deltas if d > 0 and ramp_end <= t <= steady_hi]
    gaps = {}
    for i in range(1, len(rise_times)):
        g = int(round(rise_times[i] - rise_times[i - 1]))
        gaps[g] = gaps.get(g, 0) + 1
    gap_mode = sorted(gaps.items(), key=lambda kv: (-kv[1], kv[0]))[:3]

    gen2 = [(round((ts - t0).total_seconds(), 1), val)
            for ts, val in series[GEN2] if val > 0]
    gen2_drop = []
    for t, _cnt in gen2:
        before = at(t - 1.0)
        after = at(t)
        gen2_drop.append({"t_s": t, "loh_delta_mb": round(after[1] - before[1], 3)})

    alloc = [val / 1e6 for _, val in series[ALLOC] if val > 0]
    peak_ts, peak_val = max(series[ALLOC], key=lambda p: p[1])
    steady_alloc = [v / 1e6 for ts, v in series[ALLOC]
                    if v > 0 and (ts - t0).total_seconds() >= ramp_end]

    def alloc_stats(vals):
        if not vals:
            return None
        s = sorted(vals)
        return {"n": len(vals), "mean": round(statistics.mean(vals), 3),
                "median": round(statistics.median(vals), 3),
                "p95": round(_pct(s, 0.95), 3), "max": round(s[-1], 3)}

    return {
        "file": os.path.abspath(path),
        "timebase": {"samples": len(loh), "span_s": round(span, 1),
                     "t0": t0.strftime("%Y-%m-%dT%H:%M:%S"),
                     "ramp_end_s": ramp_end},
        "loh": {"endpoints": endpoints,
                "steady_point_60s_mb": round(steady_point[1], 3),
                "post_ramp_delta_mb": round(loh[-1][1] - steady_point[1], 3),
                "post_ramp_rate_mb_s": round((loh[-1][1] - steady_point[1]) / (span - ramp_end), 5)
                                       if span > ramp_end else None,
                "windows": windows,
                "max_step": {"t_s": round(max_step[0], 1), "delta_mb": round(max_step[1], 6)}
                            if max_step else None,
                "rise_gap_mode_s": [{"gap_s": g, "count": c} for g, c in gap_mode]},
        "gen2": {"count": len(gen2), "times_s": [t for t, _ in gen2],
                 "loh_drop_at_event_mb": gen2_drop},
        "alloc_rate_mb_s": {"samples": len(alloc),
                            "stats": alloc_stats(alloc),
                            "peak": {"t_s": round((peak_ts - t0).total_seconds(), 1),
                                     "mb_s": round(peak_val / 1e6, 3)},
                            "steady_stats": alloc_stats(steady_alloc)},
        "gen0_events": len([v for _, v in series.get(GEN0, []) if v > 0]),
        "gen1_events": len([v for _, v in series.get(GEN1, []) if v > 0]),
        "note": "A8-trend：趋势，不作结论。LOH 结论级判定需 A8-conclusion（爬坡后 >=2 个 Gen2 after-GC 点）。",
    }


def render_md(r):
    tb, loh, g2, al = r["timebase"], r["loh"], r["gen2"], r["alloc_rate_mb_s"]
    w = loh["windows"]
    lines = []
    lines.append("### A8-trend（`%s`）" % os.path.basename(r["file"]))
    lines.append("")
    lines.append("- 数据：`%s`；样本 **n=%d**；跨度 **%.1fs**；t0=%s"
                 % (r["file"], tb["samples"], tb["span_s"], tb["t0"]))
    ep = loh["endpoints"]
    lines.append("- LOH Size（B，MB=10^6）：首 **%.1f** → 末 **%.1f**（端点 **%+.1fMB**）；"
                 "爬坡末端(%.0fs) 单点 **%.1fMB**"
                 % (ep["first_mb"], ep["last_mb"], ep["delta_mb"], tb["ramp_end_s"],
                    loh["steady_point_60s_mb"]))
    if loh["max_step"]:
        lines.append("- **爬坡后端点变化**：%.1fs **%.1fMB** → 末 **%.1fMB** = **%+.1fMB**（%+.4f MB/s，端到端）"
                 % (tb["ramp_end_s"], loh["steady_point_60s_mb"], ep["last_mb"],
                    loh["post_ramp_delta_mb"], loh["post_ramp_rate_mb_s"] or 0.0))
    lines.append("- 最大单步：**%+.1fMB @ %.1fs**（启动批量分配的观测形状）"
                     % (loh["max_step"]["delta_mb"], loh["max_step"]["t_s"]))
    lines.append("- 斜率（OLS，MB/s）：后段(%.0f–%.0fs) **%+.4f**（n=%d）；"
                 "稳态前 240s(%.0f–%.0fs) **%+.4f**（n=%d）；尾 60s **%+.4f**"
                 % (w["post_ramp_to_end"]["lo_s"], w["post_ramp_to_end"]["hi_s"],
                    w["post_ramp_to_end"]["ols_mb_per_s"], w["post_ramp_to_end"]["n"],
                    w["steady_first_240s"]["lo_s"], w["steady_first_240s"]["hi_s"],
                    w["steady_first_240s"]["ols_mb_per_s"], w["steady_first_240s"]["n"],
                    w["late_tail"]["ols_mb_per_s"]))
    if loh["rise_gap_mode_s"]:
        lines.append("- LOH 增长步长主间隔：%s（供 flush/分配周期交叉核对）"
                     % ", ".join("%ds×%d" % (g["gap_s"], g["count"]) for g in loh["rise_gap_mode_s"]))
    lines.append("- Gen 2 GC 时刻（rate>0，共 **%d** 次）：%s"
                 % (g2["count"], ", ".join("%.1fs" % t for t in g2["times_s"]) or "无"))
    if g2["loh_drop_at_event_mb"]:
        lines.append("- 每次 Gen2 的 LOH 变化：%s"
                     % ", ".join("%+.2fMB@%.0fs" % (e["loh_delta_mb"], e["t_s"])
                                 for e in g2["loh_drop_at_event_mb"]))
    st = al["stats"]
    if st:
        lines.append("- Allocation Rate（MB/s）：n=%d；mean **%.1f** / median **%.1f** / "
                     "p95 **%.1f** / max **%.1f** @ **%.1fs**"
                     % (st["n"], st["mean"], st["median"], st["p95"], st["max"], al["peak"]["t_s"]))
    ss = al["steady_stats"]
    if ss:
        lines.append("- Allocation Rate 稳态(≥%.0fs)：mean %.1f / median %.1f / p95 %.1f / max %.1f"
                     % (tb["ramp_end_s"], ss["mean"], ss["median"], ss["p95"], ss["max"]))
    lines.append("- **判读：趋势，不作结论**（`A8-trend`）。")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# 自测：合成 CSV，覆盖「正常 / 缺计数器 / 空样本 / 坏时间戳」四种形态。
# ---------------------------------------------------------------------------
def _write_fake(path, rows):
    with open(path, "w", encoding="utf-8", newline="") as fh:
        w = csv.writer(fh)
        w.writerow(["Timestamp", "Provider", "Counter Name", "Counter Type", "Mean/Increment"])
        for r in rows:
            w.writerow(r)


def _fake_rows(base="11/15/2026 10:00:00", n=30, with_gen2=True, with_alloc=True):
    t0 = datetime.datetime.strptime(base, TS_FMT)
    rows = []
    for i in range(n):
        ts = (t0 + datetime.timedelta(seconds=i)).strftime(TS_FMT)
        loh = 216000000 + (i // 5) * 394432
        rows.append([ts, "System.Runtime", LOH, "Metric", loh])
        if with_alloc:
            rows.append([ts, "System.Runtime", ALLOC, "Rate", 400000000 + i * 1000000])
        rows.append([ts, "System.Runtime", GEN2, "Rate", 1 if (with_gen2 and i == 3) else 0])
    return rows


def selftest():
    checks = []

    def ok(name, cond, detail=""):
        checks.append((name, bool(cond), detail))

    with tempfile.TemporaryDirectory(prefix="counters-series-") as tmp:
        good = os.path.join(tmp, "good.csv")
        _write_fake(good, _fake_rows(n=30))
        r = analyze(good, ramp_end=10.0)
        ok("样本数", r["timebase"]["samples"] == 30, r["timebase"]["samples"])
        ok("跨度", abs(r["timebase"]["span_s"] - 29.0) < 1e-9)
        ok("Gen2 时刻", r["gen2"]["times_s"] == [3.0], r["gen2"]["times_s"])
        ok("步长主间隔=5s", r["loh"]["rise_gap_mode_s"][0]["gap_s"] == 5,
           r["loh"]["rise_gap_mode_s"])
        ok("最大单步=394432B", abs(r["loh"]["max_step"]["delta_mb"] - 0.394432) < 1e-5,
           r["loh"]["max_step"])
        ok("端点正确", r["loh"]["endpoints"]["last_mb"] > r["loh"]["endpoints"]["first_mb"])
        ok("alloc 统计 n>0", r["alloc_rate_mb_s"]["stats"]["n"] == 30)
        md = render_md(r)
        ok("md 含 A8-trend", "A8-trend" in md)
        ok("md 含 Gen2 时刻", "3.0s" in md)

        miss = os.path.join(tmp, "miss.csv")
        _write_fake(miss, [row for row in _fake_rows() if row[2] != GEN2])
        try:
            analyze(miss)
            ok("缺 Gen2 必须失败", False)
        except InputError as e:
            ok("缺 Gen2 必须失败", "缺计数器" in str(e), str(e))

        empty = os.path.join(tmp, "empty.csv")
        _write_fake(empty, [])
        try:
            analyze(empty)
            ok("空样本必须失败", False)
        except InputError:
            ok("空样本必须失败", True)

        bad = os.path.join(tmp, "bad.csv")
        rows = _fake_rows(n=5)
        rows[0][0] = "not-a-timestamp"
        _write_fake(bad, rows)
        try:
            analyze(bad)
            ok("坏时间戳必须失败", False)
        except InputError:
            ok("坏时间戳必须失败", True)

        zero = os.path.join(tmp, "zero.csv")
        open(zero, "w", encoding="utf-8").close()
        try:
            analyze(zero)
            ok("空文件必须失败", False)
        except InputError:
            ok("空文件必须失败", True)

    bad = [c for c in checks if not c[1]]
    for name, passed, detail in checks:
        print("%s %s%s" % ("PASS" if passed else "FAIL", name,
                           "" if passed else " :: %r" % (detail,)))
    print("selftest: %d checks, %d failed" % (len(checks), len(bad)))
    return 1 if bad else 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="dotnet-counters CSV -> A8-trend 报告段")
    ap.add_argument("files", nargs="*")
    ap.add_argument("--ramp-end", type=float, default=60.0)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.files:
        ap.print_usage(sys.stderr)
        print("counters_series: 至少给一个 CSV（或 --selftest）", file=sys.stderr)
        return 2
    out = []
    try:
        for path in args.files:
            out.append(analyze(path, args.ramp_end))
    except InputError as exc:
        print("counters_series: %s" % exc, file=sys.stderr)
        return 3
    if args.json:
        print(json.dumps(out, ensure_ascii=False, indent=1))
    else:
        print("\n\n".join(render_md(r) for r in out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
