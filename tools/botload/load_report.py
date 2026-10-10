#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""load_report.py — 假人压测的指标归算（C6 载荷门禁）。

**口径（与 M2 的 p99-baseline 对齐的就是这一份，两侧共用）**

tick 服务时延（`tick_ms_*`）
  定义：客户端收到明文动作帧 `#+GD/<rtime>!` 的时刻 − 帧里带的 `rtime`，
  两者都在 `Environment.TickCount`（**开机毫秒、全机同域**）上取值。
  语义 =「服务端取时间戳 → 客户端收完这一帧」的耗时（本机环回，无网络时延），
  直接反映世界线程/发送队列被拖住的程度。
  样本来源：假人进程内的直方图（`load_stats.ndjson`，1ms 桶，0..200ms + 溢出桶，累计值）。
  **稳态窗** = 从「登录数首次达到目标」那一行到最后一行的差分（上线爬坡期单列，不混进 P99）。
  P50/P90/P99 = 直方图上"累计占比首次 ≥ q"的那个桶（1ms 分辨率）。
  量化下限：`Environment.TickCount` 在 Windows 上粒度约 15.6ms ⇒ 单样本 ±16ms，报告里显式记 `clock_granularity_note`。

登录成功率（`login_success_pct`）
  = 稳态窗内 `login_ok` 峰值 ÷ 目标假人数（来源：BotSrv 内部计数 `LoadMetrics.LoginOk()`，
  不是抓日志行——日志行只在"无期限提示"分支打印，会漏）。

掉线（`conn_lost`）
  = 登录之后 socket 被关闭/超时（`ConnectionReset`/`TimedOut`/其它）的累计次数；
  与"连不上"（`conn_refused`）分开计。爬坡期的掉线单列（`conn_lost_ramp`），稳态期的算 `conn_lost_steady`。

内存曲线（`mem`）
  每 `--mem-sample-sec` 采样一次各进程的 WS/私有字节/CPU；给峰值、均值、以及**最后 60s 的斜率**（MB/s），
  斜率用来判 M5 的"最后一段斜率 ≈ 0"（真泄漏 vs 分配器高水位）。

用法：
  python tools/botload/load_report.py summarize --stats <load_stats.ndjson> --mem <mem.ndjson> \
      --count 200 --stagger-ms 20 --hold-sec 60 --out <report.json>
  python tools/botload/load_report.py selftest
"""
import argparse
import json
import os
import sys

DEFAULT_THRESHOLDS = {
    "login_success_pct": 100.0,     # §6 M5：登录成功率 100%
    "conn_lost_steady": 0,          # §6 M5：无掉线
    "tick_p99_ms": 100.0,           # §6 M5：tick P99 ≤ 100ms
}


def load_ndjson(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def hist_sub(a, b):
    ha, hb = a.get("hist", []), b.get("hist", [])
    n = max(len(ha), len(hb))
    return [(hb[i] if i < len(hb) else 0) - (ha[i] if i < len(ha) else 0) for i in range(n)]


def quantile_from_hist(hist, q, bucket_ms=1):
    """直方图上的分位数：最小的桶 i 使得累计占比 ≥ q（1ms 分辨率）。返回 (ms, 样本数)。"""
    total = sum(hist)
    if total <= 0:
        return None, 0
    need = q * total
    cum = 0
    for i, c in enumerate(hist):
        cum += c
        if cum >= need:
            return i * bucket_ms, total
    return (len(hist) - 1) * bucket_ms, total


def summarize_tick(stats, count):
    """稳态窗（登录数达到目标之后）的 tick 分布；爬坡期单列。"""
    if not stats:
        return {"error": "load_stats 为空"}
    peak_login = max(r.get("login_ok", 0) for r in stats)
    ramp_idx = None
    for i, r in enumerate(stats):
        if r.get("login_ok", 0) >= count:
            ramp_idx = i
            break
    last = stats[-1]
    base = stats[ramp_idx] if ramp_idx is not None else stats[0]
    hist = hist_sub(base, last)
    out = {
        "samples": sum(hist),
        "samples_total": last.get("tick_samples", 0),
        "max_ms": last.get("tick_max_ms"),
        "mean_ms": (round(last.get("tick_sum_ms", 0) / last["tick_samples"], 2)
                    if last.get("tick_samples") else None),
        "ramp_complete": ramp_idx is not None,
        "steady_window_ms": (last.get("t_ms", 0) - base.get("t_ms", 0)),
        "peak_login_ok": peak_login,
        "login_success_pct": round(100.0 * peak_login / count, 2) if count else None,
        "conn_refused": last.get("conn_refused", 0),
        "conn_lost": last.get("conn_lost", 0),
        "conn_lost_ramp": base.get("conn_lost", 0),
        "conn_lost_steady": last.get("conn_lost", 0) - base.get("conn_lost", 0),
        "hist_bucket_ms": last.get("hist_bucket_ms", 1),
    }
    for q, key in ((0.50, "tick_p50_ms"), (0.90, "tick_p90_ms"), (0.99, "tick_p99_ms")):
        v, _ = quantile_from_hist(hist, q, out["hist_bucket_ms"])
        out[key] = v
    # 爬坡时间 = 首次达到目标登录数的那一行的时间戳（相对首行）
    out["ramp_ms"] = (base.get("t_ms", 0) - stats[0].get("t_ms", 0)) if ramp_idx is not None else None
    out["clock_granularity_note"] = ("Environment.TickCount 在 Windows 上粒度约 15.6ms"
                                     "（单样本 ±16ms 量化，是测量下限，不是噪声消除）")
    return out


def summarize_mem(rows, tail_sec=60):
    """按进程归算内存曲线：峰值/均值/最后 tail_sec 的斜率（MB/s）。"""
    by_proc = {}
    for r in rows:
        by_proc.setdefault(r["proc"], []).append(r)
    out = {}
    for proc, rs in sorted(by_proc.items()):
        rs = [r for r in rs if r.get("ws_mb") is not None]
        if not rs:
            continue
        ws = [r["ws_mb"] for r in rs]
        t0 = rs[0].get("t_ms", 0)
        last = rs[-1]
        tail = [r for r in rs if r.get("t_ms", 0) >= last.get("t_ms", 0) - tail_sec * 1000]
        if len(tail) >= 2:
            dt = (tail[-1]["t_ms"] - tail[0]["t_ms"]) / 1000.0
            slope = (tail[-1]["ws_mb"] - tail[0]["ws_mb"]) / dt if dt > 0 else 0.0
        else:
            slope = None
        out[proc] = {
            "samples": len(rs),
            "ws_peak_mb": round(max(ws), 1),
            "ws_mean_mb": round(sum(ws) / len(ws), 1),
            "ws_last_mb": round(ws[-1], 1),
            "priv_peak_mb": round(max((r.get("priv_mb") or 0) for r in rs), 1),
            "slope_tail_mb_per_s": (round(slope, 3) if slope is not None else None),
            "tail_window_s": round((last.get("t_ms", 0) - tail[0].get("t_ms", 0)) / 1000.0, 1) if tail else 0,
            "window_s": round((last.get("t_ms", 0) - t0) / 1000.0, 1),
        }
    return out


def judge(tier, thresholds):
    """按 §6 M5 的三条判据给单档结论（掉线只看稳态窗）。"""
    fails = []
    if tier.get("login_success_pct") is None or tier["login_success_pct"] < thresholds["login_success_pct"]:
        fails.append(f"登录成功率 {tier.get('login_success_pct')}% < {thresholds['login_success_pct']}%")
    if tier.get("conn_lost_steady", 0) > thresholds["conn_lost_steady"]:
        fails.append(f"稳态掉线 {tier['conn_lost_steady']} > {thresholds['conn_lost_steady']}")
    if tier.get("tick_p99_ms") is None or tier["tick_p99_ms"] > thresholds["tick_p99_ms"]:
        fails.append(f"tick P99 {tier.get('tick_p99_ms')}ms > {thresholds['tick_p99_ms']}ms")
    if not tier.get("ramp_complete", False):
        fails.append("登录数未达目标（爬坡未完成）")
    tier["verdict"] = "GREEN" if not fails else "RED"
    tier["fails"] = fails
    return tier


def cmd_summarize(args):
    stats = load_ndjson(args.stats) if args.stats else []
    tier = summarize_tick(stats, args.count)
    if args.mem:
        tier["mem"] = summarize_mem(load_ndjson(args.mem))
    tier["count"] = args.count
    tier["stagger_ms"] = args.stagger_ms
    tier["hold_sec"] = args.hold_sec
    judge(tier, DEFAULT_THRESHOLDS)
    text = json.dumps(tier, ensure_ascii=False, indent=2)
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            f.write(text + "\n")
    print(text)
    return 0 if tier["verdict"] == "GREEN" else 1


def cmd_selftest(_args):
    ok = True

    def expect(cond, msg):
        nonlocal ok
        if not cond:
            ok = False
            print("RED: " + msg)

    # 1) 分位数：980 个 5ms + 20 个 150ms ⇒ P50=5、P99=150；尾部敏感（挪走尾巴 ⇒ P99=5）
    hist = [0] * 201
    hist[5] = 980
    hist[150] = 20
    stats = [
        {"t_ms": 1000, "login_ok": 100, "tick_samples": 0, "tick_sum_ms": 0, "tick_max_ms": 0,
         "conn_lost": 0, "hist": [0] * 201},
        {"t_ms": 2000, "login_ok": 100, "tick_samples": 1000, "tick_sum_ms": 5 * 980 + 150 * 20,
         "tick_max_ms": 150, "conn_lost": 0, "hist": list(hist)},
    ]
    t = summarize_tick(stats, 100)
    expect(t["tick_p50_ms"] == 5, f"P50 应为 5，实得 {t['tick_p50_ms']}")
    expect(t["tick_p99_ms"] == 150, f"P99 应为 150，实得 {t['tick_p99_ms']}")
    expect(t["samples"] == 1000, f"稳态样本数应为 1000，实得 {t['samples']}")
    expect(t["ramp_complete"] is True, "登录达标应判 ramp_complete")
    hist2 = list(hist)
    hist2[150] = 0
    hist2[5] = 1000
    stats2 = [stats[0], dict(stats[1], hist=hist2, tick_max_ms=5)]
    t2 = summarize_tick(stats2, 100)
    expect(t2["tick_p99_ms"] == 5, "挪走尾巴后 P99 应降到 5（尾部敏感）")
    expect(t["tick_p99_ms"] != t2["tick_p99_ms"], "改坏输入必须让 P99 变（不是恒绿）")

    # 2) 爬坡未完成要被判红
    stats3 = [dict(stats[0], login_ok=10), dict(stats[1], login_ok=10)]
    t3 = judge(summarize_tick(stats3, 100), DEFAULT_THRESHOLDS)
    expect(t3["verdict"] == "RED" and any("爬坡未完成" in f for f in t3["fails"]),
           "登录没达目标必须判红")

    # 3) 掉线：稳态掉线判红、爬坡期掉线不算稳态
    stats4 = [dict(stats[0], conn_lost=3), dict(stats[1], conn_lost=5)]
    t4 = judge(summarize_tick(stats4, 100), DEFAULT_THRESHOLDS)
    expect(t4["conn_lost_ramp"] == 3 and t4["conn_lost_steady"] == 2, "爬坡/稳态掉线应分开")
    expect(t4["verdict"] == "RED", "稳态掉线 > 0 必须判红")

    # 4) 内存斜率：线性上升 100s 涨 100MB ⇒ ≈1MB/s；平的 ⇒ ≈0
    mem_up = [{"t_ms": i * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 100 + i, "priv_mb": 120 + i}
              for i in range(101)]
    mem_flat = [{"t_ms": i * 1000, "proc": "GameSvr", "pid": 1, "ws_mb": 100, "priv_mb": 120}
                for i in range(101)]
    s_up = summarize_mem(mem_up)["GameSvr"]
    s_flat = summarize_mem(mem_flat)["GameSvr"]
    expect(abs(s_up["slope_tail_mb_per_s"] - 1.0) < 0.05,
           f"上升曲线斜率应 ≈1MB/s，实得 {s_up['slope_tail_mb_per_s']}")
    expect(abs(s_flat["slope_tail_mb_per_s"]) < 0.01,
           f"平坦曲线斜率应 ≈0，实得 {s_flat['slope_tail_mb_per_s']}")
    expect(s_up["ws_peak_mb"] == 200.0 and s_flat["ws_peak_mb"] == 100.0, "峰值归算错")

    if not ok:
        print("RED: load_report selftest 有未通过项")
        return 1
    print("GREEN: load_report selftest 全过（分位数/尾部敏感/爬坡判红/掉线分窗/内存斜率）")
    return 0


def svg_curve(series, width=560, height=180, title="", y_label="", x_label=""):
    """零依赖 SVG 折线图（三档曲线要能入库，不引 matplotlib）。
    series: [(label, [(x, y), ...]), ...]"""
    if not series:
        return ""
    xs = [p[0] for _, pts in series for p in pts]
    ys = [p[1] for _, pts in series for p in pts]
    xmin, xmax = min(xs), max(xs)
    ymin, ymax = 0.0, max(ys) if max(ys) > 0 else 1.0
    pad_l, pad_r, pad_t, pad_b = 46, 12, 22, 28
    w, h = width - pad_l - pad_r, height - pad_t - pad_b

    def px(x):
        return pad_l + (0 if xmax == xmin else (x - xmin) / (xmax - xmin) * w)

    def py(y):
        return pad_t + h - (y - ymin) / (ymax - ymin) * h

    colors = ["#2563eb", "#dc2626", "#16a34a", "#9333ea", "#ea580c"]
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
           f'viewBox="0 0 {width} {height}" font-family="sans-serif" font-size="11">',
           f'<rect width="{width}" height="{height}" fill="#fff"/>',
           f'<text x="{pad_l}" y="14" font-size="12" font-weight="bold">{title}</text>',
           f'<line x1="{pad_l}" y1="{pad_t + h}" x2="{pad_l + w}" y2="{pad_t + h}" stroke="#94a3b8"/>',
           f'<line x1="{pad_l}" y1="{pad_t}" x2="{pad_l}" y2="{pad_t + h}" stroke="#94a3b8"/>',
           f'<text x="4" y="{pad_t + 8}">{ymax:.0f}</text>',
           f'<text x="4" y="{pad_t + h}">0</text>',
           f'<text x="{pad_l + w - 60}" y="{height - 6}">{x_label}</text>',
           f'<text x="4" y="{pad_t - 8}">{y_label}</text>']
    for i, (label, pts) in enumerate(series):
        c = colors[i % len(colors)]
        d = " ".join(("M" if j == 0 else "L") + f"{px(x):.1f},{py(y):.1f}" for j, (x, y) in enumerate(pts))
        out.append(f'<path d="{d}" fill="none" stroke="{c}" stroke-width="1.8"/>')
        for x, y in pts:
            out.append(f'<circle cx="{px(x):.1f}" cy="{py(y):.1f}" r="2.4" fill="{c}"/>')
        out.append(f'<text x="{pad_l + 6 + i * 92}" y="{pad_t + 12}" fill="{c}">{label}</text>')
    out.append("</svg>")
    return "\n".join(out)


def cmd_aggregate(args):
    """三档汇总 → SUMMARY.md（表 + 曲线）。输入 = load_gate_summary.json。"""
    # PowerShell 5.1 的 Set-Content -Encoding utf8 会带 BOM ⇒ 用 utf-8-sig 读（两种都能吃）
    data = json.load(open(args.summary, encoding="utf-8-sig"))
    reports = data.get("reports", [])
    outdir = args.out_dir or os.path.dirname(os.path.abspath(args.summary))
    lines = ["# 压测报告（C6 载荷门禁）", ""]
    lines.append(f"- 口径：{data.get('caliber', '见 tools/botload/load_report.py')}")
    lines.append(f"- 档位：{data.get('tiers')}；连接错峰 {data.get('stagger_ms')}ms/个；稳态时长 {data.get('hold_sec')}s")
    lines.append(f"- 判据（§6 M5）：登录成功率 100% / 稳态掉线 0 / tick P99 ≤ 100ms")
    lines.append("")
    lines.append("| 档位 | 登录成功率 | 稳态掉线 | 连不上 | tick P50 | P90 | P99 | max | 稳态样本 | 上线耗时 | 结论 |")
    lines.append("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for t in sorted(reports, key=lambda r: r.get("count", 0)):
        lines.append("| {count} | {lsp}% | {cls} | {cr} | {p50} | {p90} | {p99} | {mx} | {n} | {ramp} | {v} |".format(
            count=t.get("count"), lsp=t.get("login_success_pct"), cls=t.get("conn_lost_steady"),
            cr=t.get("conn_refused"), p50=t.get("tick_p50_ms"), p90=t.get("tick_p90_ms"),
            p99=t.get("tick_p99_ms"), mx=t.get("max_ms"), n=t.get("samples"),
            ramp=((str(round(t["ramp_ms"] / 1000.0, 1)) + "s") if t.get("ramp_ms") is not None else "-"),
            v=t.get("verdict")))
    lines.append("")
    # 曲线 1：tick P99 / P50 / max vs 档位
    pts_p99 = [(t["count"], t["tick_p99_ms"] or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    pts_p50 = [(t["count"], t["tick_p50_ms"] or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    pts_mx = [(t["count"], t.get("max_ms") or 0) for t in sorted(reports, key=lambda r: r.get("count", 0))]
    lines.append("## 曲线 1：tick 服务时延 vs 假人数（目标 P99 ≤ 100ms）")
    lines.append("")
    lines.append("```html")
    lines.append(svg_curve([("P99", pts_p99), ("P50", pts_p50), ("max", pts_mx)],
                           title="tick latency vs bots", y_label="ms", x_label="bots"))
    lines.append("```")
    # 曲线 2：GameSvr 内存曲线（各档）
    mem_series = []
    for t in sorted(reports, key=lambda r: r.get("count", 0)):
        g = (t.get("mem") or {}).get("GameSrv")
        if g:
            mem_series.append((f"{t['count']} bots", [(t["count"], g.get("ws_peak_mb", 0)),
                                                      (t["count"], g.get("ws_mean_mb", 0))]))
    lines.append("")
    lines.append("## 曲线 2：GameSvr 内存（峰值/均值 vs 假人数）")
    lines.append("")
    lines.append("```html")
    lines.append(svg_curve([(lbl, pts) for lbl, pts in mem_series],
                           title="GameSvr WS vs bots", y_label="MB", x_label="bots"))
    lines.append("```")
    lines.append("")
    lines.append("## 各进程内存（末档）")
    lines.append("")
    last = sorted(reports, key=lambda r: r.get("count", 0))[-1] if reports else None
    if last and last.get("mem"):
        lines.append("| 进程 | 峰值 WS | 均值 WS | 私有峰值 | 末段斜率 | 窗口 |")
        lines.append("| --- | --- | --- | --- | --- | --- |")
        for proc, m in sorted(last["mem"].items()):
            lines.append(f"| {proc} | {m['ws_peak_mb']}MB | {m['ws_mean_mb']}MB | {m['priv_peak_mb']}MB | "
                         f"{m['slope_tail_mb_per_s']}MB/s | {m['window_s']}s |")
    md_path = os.path.join(outdir, "SUMMARY.md")
    with open(md_path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"AGGREGATE_OUT {md_path}")
    return 0


def main():
    ap = argparse.ArgumentParser(description="假人压测指标归算（C6）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("summarize")
    s.add_argument("--stats", required=True, help="BotSrv 落的 load_stats.ndjson")
    s.add_argument("--mem", default="", help="内存采样 mem.ndjson（可选）")
    s.add_argument("--count", type=int, required=True, help="目标假人数（档位）")
    s.add_argument("--stagger-ms", type=int, default=0)
    s.add_argument("--hold-sec", type=int, default=0)
    s.add_argument("--out", default="")
    a = sub.add_parser("aggregate")
    a.add_argument("--summary", required=True, help="load_gate_summary.json")
    a.add_argument("--out-dir", default="")
    sub.add_parser("selftest")
    args = ap.parse_args()
    if args.cmd == "summarize":
        return cmd_summarize(args)
    if args.cmd == "aggregate":
        return cmd_aggregate(args)
    return cmd_selftest(args)


if __name__ == "__main__":
    sys.exit(main())
