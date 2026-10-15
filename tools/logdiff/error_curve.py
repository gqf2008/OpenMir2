#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""error_curve.py — ERROR 分桶曲线归算器（M5①「ERROR 不上升曲线」机判器）。

把多进程 Serilog 控制台日志（*.out.log，`HH:mm:ss.fff [LVL] msg` 行首）里的
ERROR/FTL 行按时间桶（默认 1h，可 6h）分桶，产出「按桶 × 进程」的错误曲线；
可并入 BotSrv 落的 `load_stats.ndjson` 的 `internal_errors`（累计→每桶增量）。

判据（默认**只报告不判红**，加 `--gate` 才用退出码判红）：
  「不上升」= 尾窗均值 <= rise_tol_ratio × 首窗均值（首/尾窗 = 桶序三等分各取 k 桶）。
  * rise_tol_ratio=1.0（默认）⇒ 严格不上升（尾窗均值 <= 首窗均值）；
  * --max-slope 给定且桶间 OLS 斜率 > max_slope ⇒ 追加 RED（绝对斜率牙）。
  * verdict ∈ {GREEN 不上升, RED 上升, PREREQ 缺输入/桶数不足}；rc 见下。

跨午夜：日志无日期（HH:mm:ss.fff）⇒ 用 --anchor 补日；同文件内时间戳回退即
+86400s（72h 长跑必须），保证派生 t_ms 单调、桶号非负。

退出码：默认 rc=0（纯报告，不判红）；`--gate` 时 0=GREEN / 1=RED / 2=PREREQ。
`selftest`：改坏必红自检。

用法：
  python -I tools/logdiff/error_curve.py curve <LogDir|*.out.log> [...] \
      [--logdir DIR ...] [--glob "*.out.log"] [--stats load_stats.ndjson] \
      [--anchor 2026-10-15] [--bucket-sec 3600] [--rise-tol-ratio 1.0] \
      [--min-buckets 3] [--max-slope FLOAT] [--gate] [--json out.json]
  python -I tools/logdiff/error_curve.py selftest
"""
from __future__ import annotations

import argparse
import datetime
import glob as _glob
import json
import os
import re
import sys
from pathlib import Path

RC_GREEN, RC_RED, RC_PREREQ = 0, 1, 2
DAY_MS = 86_400_000

# 行首 `HH:mm:ss.fff [LVL] msg`：仅 ERR/FTL 计入（Serilog {Level:u3}：ERROR=[ERR]、Fatal=[FTL]）。
# 非时间戳横幅行（如 LoginSrv ASCII banner）不匹配 ⇒ 跳过。
RE_ERR = re.compile(r"^(\d{2}):(\d{2}):(\d{2})\.(\d{3}) \[(ERR|FTL)\] (.*)$")


def day_ms_from_anchor(anchor: str) -> int:
    """与 timeline.py 同口径（naive 日期按本机本地时区解释）。"""
    dt = datetime.datetime.strptime(anchor, "%Y-%m-%d")
    return int(dt.timestamp() * 1000)


def proc_name(path: str) -> str:
    base = os.path.basename(path)
    low = base.lower()
    for suf in (".out.log", ".err.log", ".log"):
        if low.endswith(suf):
            return base[: -len(suf)]
    return base


def parse_error_lines(path: str, proc: str, day_ms: int) -> list[int]:
    """返回该文件中 ERR/FTL 行的派生 t_ms 列表（单调不减）。

    proc 仅用于错误定位/标注（保持规格签名 `parse_error_lines(path, proc, day_ms)`）。
    跨午夜按「时间戳回退即 +86400s」累加，非时间戳行跳过。
    """
    events: list[int] = []
    day_offset = 0
    prev: int | None = None
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = RE_ERR.match(line.rstrip("\n"))
            if not m:
                continue
            h, mi, s, ms = int(m.group(1)), int(m.group(2)), int(m.group(3)), int(m.group(4))
            hms = ((h * 60 + mi) * 60 + s) * 1000 + ms
            t = day_ms + day_offset + hms
            if prev is not None and t < prev:
                day_offset += DAY_MS
                t += DAY_MS
            events.append(t)
            prev = t
    if not events:
        _ = proc  # 保留参数（无事件时无副作用）
    return events


def bucket(events: list[int], bucket_ms: int) -> dict[int, int]:
    """按 floor(t/bucket_ms) 分桶；桶号按 epoch 整点对齐（跨进程一致）。"""
    counts: dict[int, int] = {}
    for t in events:
        b = t // bucket_ms
        counts[b] = counts.get(b, 0) + 1
    return counts


def _fill(series: dict[int, int], lo: int, hi: int) -> dict[int, int]:
    """缺桶补零：把 [lo,hi] 内缺席的桶补 0。"""
    return {b: series.get(b, 0) for b in range(lo, hi + 1)}


def load_stats(path: str) -> tuple[list[dict], list[str]]:
    """读 load_stats.ndjson；缺 t_ms / internal_errors / 非法 JSON ⇒ reasons（fail-closed）。"""
    rows: list[dict] = []
    reasons: list[str] = []
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    for lineno, line in enumerate(text.splitlines(), 1):
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError:
            reasons.append(f"line {lineno}: 非法 JSON")
            continue
        if not isinstance(obj, dict):
            reasons.append(f"line {lineno}: 不是 JSON 对象")
            continue
        if "t_ms" not in obj:
            reasons.append(f"line {lineno}: 缺 t_ms")
            continue
        if "internal_errors" not in obj:
            reasons.append(f"line {lineno}: 缺 internal_errors")
            continue
        rows.append(obj)
    if not rows:
        reasons.append("没有任何含 (t_ms, internal_errors) 的行")
    return rows, reasons


def delta_series(rows: list[dict], bucket_ms: int) -> dict[int, int]:
    """累计 internal_errors → 每桶增量。

    口径：桶 b 的增量 = (b 内最后一个样本的累计值) − (b 之前最后一个样本的累计值)。
    稀疏桶把期间累计增长并入下一个有样本的桶（真实 ndjson 为 5s 连续采样，每桶都有样本）。
    """
    pts = sorted((int(r["t_ms"]), int(r["internal_errors"])) for r in rows)
    last_in_bucket: dict[int, int] = {}
    for t, v in pts:
        last_in_bucket[t // bucket_ms] = v
    deltas: dict[int, int] = {}
    prev_val = 0
    for b in sorted(last_in_bucket):
        deltas[b] = last_in_bucket[b] - prev_val
        prev_val = last_in_bucket[b]
    return deltas


def build_curves(per_proc_events: dict[str, list[int]],
                 ie_rows: list[dict], bucket_ms: int) -> dict:
    """产出 {procs, total, ie, lo, hi, empty}（各曲线按同一 [lo,hi] 补零）。"""
    proc_buckets = {p: bucket(ev, bucket_ms) for p, ev in per_proc_events.items()}
    ie_buckets = delta_series(ie_rows, bucket_ms) if ie_rows else {}
    idxs: list[int] = []
    for c in proc_buckets.values():
        idxs += list(c)
    idxs += list(ie_buckets)
    if not idxs:
        return {"lo": None, "hi": None, "procs": {}, "total": {}, "ie": {}, "empty": True}
    lo, hi = min(idxs), max(idxs)
    procs = {p: _fill(c, lo, hi) for p, c in proc_buckets.items()}
    total: dict[int, int] = {b: 0 for b in range(lo, hi + 1)}
    for c in procs.values():
        for b, v in c.items():
            total[b] += v
    ie = _fill(ie_buckets, lo, hi)
    return {"lo": lo, "hi": hi, "procs": procs, "total": total, "ie": ie, "empty": False}


def _ols_slope(ys: list[float]) -> float:
    n = len(ys)
    if n < 2:
        return 0.0
    mx = (n - 1) / 2.0
    my = sum(ys) / n
    num = sum((i - mx) * (y - my) for i, y in enumerate(ys))
    den = sum((i - mx) ** 2 for i in range(n))
    return num / den if den else 0.0


def trend_verdict(total_counts: list[int], rise_tol_ratio: float,
                  min_buckets: int, max_slope: float | None = None) -> dict:
    """曲线趋势判定（首/尾窗三等分比值 + 可选绝对斜率）。"""
    n = len(total_counts)
    out: dict = {
        "buckets": n,
        "rise_tol_ratio": rise_tol_ratio,
        "min_buckets": min_buckets,
        "max_slope": max_slope,
    }
    if n == 0:
        out.update(verdict="GREEN", why="空曲线（无 ERROR 事件）",
                   first_mean=0.0, last_mean=0.0, slope=0.0,
                   rising_ratio=False, rising_slope=False)
        return out
    if n < min_buckets:
        out.update(verdict="PREREQ", why=f"桶数 {n} < min_buckets {min_buckets}")
        return out
    k = n // 3
    first = total_counts[:k]
    last = total_counts[-k:]
    fm = sum(first) / k
    lm = sum(last) / k
    slope = _ols_slope([float(x) for x in total_counts])
    rising_ratio = lm > rise_tol_ratio * fm
    rising_slope = (max_slope is not None) and (slope > max_slope)
    out.update(first_mean=round(fm, 4), last_mean=round(lm, 4),
               slope=round(slope, 6),
               rising_ratio=rising_ratio, rising_slope=rising_slope)
    if rising_ratio or rising_slope:
        why = []
        if rising_ratio:
            why.append(f"尾窗均值 {lm:.3f} > {rise_tol_ratio} × 首窗均值 {fm:.3f}")
        if rising_slope:
            why.append(f"斜率 {slope:.6f} > max_slope {max_slope}")
        out.update(verdict="RED", why="；".join(why))
    else:
        out.update(verdict="GREEN",
                   why=f"尾窗均值 {lm:.3f} <= {rise_tol_ratio} × 首窗均值 {fm:.3f}")
    return out


def _fmt_ts(ms: int) -> str:
    return datetime.datetime.fromtimestamp(ms / 1000.0).strftime("%Y-%m-%dT%H:%M:%S")


def render_table(curves: dict, trend: dict, bucket_ms: int, anchor: str) -> str:
    """文本表：idx / 桶起点 / 每进程计数 / 合计 / internal_errors 增量，末行 VERDICT。"""
    lines: list[str] = []
    if curves["empty"]:
        lines.append(f"ERROR 曲线：bucket={bucket_ms // 1000}s anchor={anchor} buckets=0"
                     f"（无 ERROR/internal_errors 数据）")
    else:
        lo, hi = curves["lo"], curves["hi"]
        procs = sorted(curves["procs"].keys())
        idxs = list(range(lo, hi + 1))
        lines.append(f"ERROR 曲线：bucket={bucket_ms // 1000}s anchor={anchor} "
                     f"buckets={len(idxs)} span={_fmt_ts(lo * bucket_ms)}..{_fmt_ts((hi + 1) * bucket_ms)}")
        lines.append(f"{'idx':>3}  {'start':<19}  " +
                     "  ".join(f"{p:>10}" for p in procs) +
                     f"  {'total':>7}  {'ie':>7}")
        for b in idxs:
            lines.append(f"{b:>3}  {_fmt_ts(b * bucket_ms):<19}  " +
                         "  ".join(f"{curves['procs'][p][b]:>10}" for p in procs) +
                         f"  {curves['total'][b]:>7}  {curves['ie'][b]:>7}")
    lines.append(f"VERDICT {trend['verdict']} (buckets={trend['buckets']} "
                 f"first_mean={trend.get('first_mean')} last_mean={trend.get('last_mean')} "
                 f"slope={trend.get('slope')})")
    lines.append(f"  why: {trend['why']}")
    return "\n".join(lines)


def resolve_inputs(raw: list[str], glob_pat: str) -> tuple[list[str], list[str], int]:
    """展开路径；返回 (files, prereq_reasons, dirs_seen)。目录内按 glob_pat 取。"""
    files: list[str] = []
    reasons: list[str] = []
    dirs_seen = 0
    for p in raw:
        pp = Path(p)
        if pp.is_dir():
            dirs_seen += 1
            files += sorted(_glob.glob(os.path.join(str(pp), glob_pat)))
        elif pp.is_file():
            files.append(str(pp))
        else:
            reasons.append(f"路径不存在: {p}")
    return files, reasons, dirs_seen


def cmd_curve(args) -> int:
    raw = list(args.paths) + list(args.logdir)
    bucket_ms = args.bucket_sec * 1000
    if not raw and not args.stats:
        print("PREREQ: 未提供任何 logdir/日志文件（--logdir 或位置参数）", file=sys.stderr)
        return RC_PREREQ if args.gate else RC_GREEN

    files, reasons, dirs_seen = resolve_inputs(raw, args.glob)
    if not files and not dirs_seen and reasons:
        # 全部路径不存在 ⇒ PREREQ（缺 logdir）；
        # 若给的是「存在但为空」的目录 ⇒ files=[] & dirs_seen>0 ⇒ 落到空曲线 GREEN。
        for r in reasons:
            print(f"PREREQ: {r}", file=sys.stderr)
        return RC_PREREQ if args.gate else RC_GREEN

    # anchor：显式 --anchor 优先，否则取最早输入文件 mtime 的日期
    if args.anchor:
        day_ms = day_ms_from_anchor(args.anchor)
        anchor = args.anchor
    elif files:
        mt = min(os.path.getmtime(f) for f in files)
        anchor = datetime.datetime.fromtimestamp(mt).strftime("%Y-%m-%d")
        day_ms = day_ms_from_anchor(anchor)
    else:
        anchor = datetime.datetime.now().strftime("%Y-%m-%d")
        day_ms = day_ms_from_anchor(anchor)

    per_proc: dict[str, list[int]] = {}
    for p in files:
        ev = parse_error_lines(p, proc_name(p), day_ms)
        per_proc.setdefault(proc_name(p), []).extend(ev)

    ie_rows: list[dict] = []
    if args.stats:
        sp = Path(args.stats)
        if not sp.is_file():
            print(f"PREREQ: 缺 --stats {sp}", file=sys.stderr)
            return RC_PREREQ if args.gate else RC_GREEN
        ie_rows, stat_reasons = load_stats(str(sp))
        if stat_reasons:
            print(f"PREREQ: {sp}", file=sys.stderr)
            for r in stat_reasons[:20]:
                print(f"  - {r}", file=sys.stderr)
            return RC_PREREQ if args.gate else RC_GREEN

    curves = build_curves(per_proc, ie_rows, bucket_ms)
    total_counts = ([curves["total"][b] for b in range(curves["lo"], curves["hi"] + 1)]
                    if not curves["empty"] else [])
    trend = trend_verdict(total_counts, args.rise_tol_ratio, args.min_buckets, args.max_slope)

    print(render_table(curves, trend, bucket_ms, anchor))

    if args.json:
        js = {
            "anchor": anchor,
            "bucket_sec": args.bucket_sec,
            "files": files,
            "curves": {p: {str(b): v for b, v in sorted(c.items())}
                       for p, c in curves["procs"].items()},
            "total_curve": {str(b): v for b, v in sorted(curves["total"].items())},
            "internal_errors_curve": {str(b): v for b, v in sorted(curves["ie"].items())},
            "empty": curves["empty"],
            "trend": trend,
        }
        Path(args.json).write_text(json.dumps(js, ensure_ascii=False, indent=2) + "\n",
                                   encoding="utf-8", newline="\n")

    v = trend["verdict"]
    if v == "GREEN":
        print(f"GREEN: ERROR 曲线不上升（{trend['why']}）")
    elif v == "RED":
        print(f"RED: ERROR 曲线上升（{trend['why']}）", file=sys.stderr)
    else:
        print(f"PREREQ: {trend['why']}", file=sys.stderr)

    if not args.gate:
        return RC_GREEN  # 默认只报告不判红
    return {"GREEN": RC_GREEN, "RED": RC_RED, "PREREQ": RC_PREREQ}[v]


# ---------------- selftest（改坏必红） ----------------

def _write(dir: Path, name: str, lines: list[str]) -> str:
    p = dir / name
    p.write_text("".join(x + "\n" for x in lines), encoding="utf-8", newline="\n")
    return str(p)


def _errs(hour: int, count: int, minute: int = 0) -> list[str]:
    return [f"{hour:02d}:{minute:02d}:00.000 [ERR] boom {i}" for i in range(count)]


def selftest() -> int:
    import tempfile
    anchor = "2026-10-15"
    day_ms = day_ms_from_anchor(anchor)
    bms = 3_600_000
    cases = 0

    def tooth(name: str, cond: bool) -> None:
        nonlocal cases
        cases += 1
        assert cond, f"TOOTH FAIL: {name}"

    def totals_of(path: str, proc: str) -> list[int]:
        ev = parse_error_lines(path, proc, day_ms)
        c = build_curves({proc: ev}, [], bms)
        return [c["total"][b] for b in range(c["lo"], c["hi"] + 1)]

    with tempfile.TemporaryDirectory(prefix="ec-selftest-") as d:
        dd = Path(d)

        # 1) 平坦（每桶等值 1）⇒ GREEN
        f_flat = _write(dd, "flat.out.log",
                        _errs(0, 1) + _errs(1, 1) + _errs(2, 1) + _errs(3, 1) + _errs(4, 1) + _errs(5, 1))
        tooth("平坦⇒GREEN", trend_verdict(totals_of(f_flat, "flat"), 1.0, 3)["verdict"] == "GREEN")

        # 2) 递增（每桶 +1）⇒ RED（改坏必红：漏判上升即失败）
        f_inc = _write(dd, "inc.out.log",
                       _errs(0, 1) + _errs(1, 2) + _errs(2, 3) + _errs(3, 4) + _errs(4, 5) + _errs(5, 6))
        tooth("递增⇒RED", trend_verdict(totals_of(f_inc, "inc"), 1.0, 3)["verdict"] == "RED")

        # 3) 缓升但在 tol 内 ⇒ GREEN（阈值有牙齿：1↦2，ratio=2.5 内）
        f_mild = _write(dd, "mild.out.log",
                        _errs(0, 1) + _errs(1, 1) + _errs(2, 2) + _errs(3, 2) + _errs(4, 2) + _errs(5, 2))
        tc = totals_of(f_mild, "mild")
        tooth("缓升 ratio=1.0 ⇒ RED", trend_verdict(tc, 1.0, 3)["verdict"] == "RED")
        tooth("缓升 ratio=2.5 ⇒ GREEN", trend_verdict(tc, 2.5, 3)["verdict"] == "GREEN")

        # 4) 分类牙：`[INF]`/`[DBG]` 行不得计入
        f_mix = _write(dd, "mix.out.log",
                       ["00:00:00.000 [ERR] a", "00:00:01.000 [INF] b",
                        "00:00:02.000 [DBG] c", "00:00:03.000 [ERR] d"])
        tooth("只计 ERR/FTL（INF/DBG 不计）",
              len(parse_error_lines(f_mix, "mix", day_ms)) == 2)

        # 5) 漏计牙：去掉一行 [ERR] ⇒ 计数必变
        f2 = _write(dd, "miss2.out.log", ["00:00:00.000 [ERR] a", "00:00:01.000 [ERR] b"])
        f1 = _write(dd, "miss1.out.log", ["00:00:00.000 [ERR] a"])
        tooth("漏计一行 [ERR] 计数必变",
              len(parse_error_lines(f2, "m2", day_ms)) == 2 and
              len(parse_error_lines(f1, "m1", day_ms)) == 1)

        # 6) 跨午夜牙：23:59:59.000 → 00:00:01.000 ⇒ t 不倒退、差 2000ms、无负桶
        f_mid = _write(dd, "mid.out.log", ["23:59:59.000 [ERR] a", "00:00:01.000 [ERR] b"])
        ev = parse_error_lines(f_mid, "mid", day_ms)
        tooth("跨午夜 t 不倒退", ev[1] > ev[0])
        tooth("跨午夜 相邻差=2000ms", ev[1] - ev[0] == 2000)
        tooth("跨午夜 桶号非负", all(b >= 0 for b in bucket(ev, bms)))

        # 7) 横幅行跳过不崩：非时间戳 ASCII + 一行 ERR
        f_ban = _write(dd, "ban.out.log",
                       ["===========================", "  Login Server  v1.0  ",
                        "===========================", "00:00:00.000 [ERR] real"])
        tooth("横幅行跳过（只取 1 行）", len(parse_error_lines(f_ban, "ban", day_ms)) == 1)

        # 8) PREREQ：ndjson 缺 t_ms
        stats_bad = dd / "bad.ndjson"
        stats_bad.write_text('{"internal_errors":0}\n{"t_ms":0,"internal_errors":1}\n',
                             encoding="utf-8", newline="\n")
        _, reasons = load_stats(str(stats_bad))
        tooth("ndjson 缺 t_ms ⇒ PREREQ", bool(reasons))

        # 9) PREREQ：桶数 < min_buckets
        tooth("桶数<min_buckets ⇒ PREREQ", trend_verdict([1, 2], 1.0, 3)["verdict"] == "PREREQ")

        # 10) 空曲线 ⇒ GREEN 不崩
        c = build_curves({}, [], bms)
        tooth("空曲线⇒empty", c["empty"] is True)
        tooth("空曲线⇒GREEN", trend_verdict([], 1.0, 3)["verdict"] == "GREEN")

        # 11) internal_errors 累计→增量：0,0,3,3 ⇒ 桶增 {0:0, 1:3}
        ie_rows = [{"t_ms": 0, "internal_errors": 0}, {"t_ms": 3_500_000, "internal_errors": 0},
                   {"t_ms": 4_000_000, "internal_errors": 3}, {"t_ms": 7_000_000, "internal_errors": 3}]
        tooth("累计→增量 (0,3)", delta_series(ie_rows, bms) == {0: 0, 1: 3})

        # 12) max_slope 牙：平坦曲线给紧斜率上限应判 RED（阈值真接线）
        flat = [0, 0, 0, 0, 0, 0]
        tooth("max_slope 未设 ⇒ GREEN", trend_verdict(flat, 1.0, 3)["verdict"] == "GREEN")
        tooth("max_slope=-1 ⇒ RED", trend_verdict(flat, 1.0, 3, -1.0)["verdict"] == "RED")

        # 13) 桶界索引牙（2026-10-15 验证者 F 补）：桶界整体平移 1 秒不改变形状/verdict，
        #     所以必须显式钉「某时刻落哪个桶号」——否则分桶错位永远测不出来。
        _d2 = day_ms_from_anchor("2026-10-15")
        _ev = [_d2 + 3_600_000 - 500, _d2 + 3_600_000]   # 00:59:59.500 与 01:00:00.000
        _bk = sorted(bucket(_ev, 3_600_000))
        tooth("桶界索引：59:59.500 与 01:00:00.000 必须落相邻两桶（桶界平移必红）",
              _bk == [_d2 // 3_600_000, _d2 // 3_600_000 + 1])

        # 14) 窗口大小牙：k = n//3 才使首/尾窗互不污染；向量中段埋 9 使 k=n//2 时均值立刻不同。
        _w = [1, 1, 1, 1, 9, 9, 9, 9, 5, 5, 5, 5]
        _tv = trend_verdict(_w, 1.0, 3)
        tooth("窗口大小牙：k=n//3（n=12 ⇒ 首窗=[1,1,1,1] 均值 1.0、尾窗=[5,5,5,5] 均值 5.0）",
              _tv["first_mean"] == 1.0 and _tv["last_mean"] == 5.0 and _tv["verdict"] == "RED")
    print(f"SELFTEST GREEN: error_curve {cases}/{cases}")
    return RC_GREEN


def main() -> int:
    ap = argparse.ArgumentParser(description="ERROR 分桶曲线归算器（M5①「ERROR 不上升」机判）")
    sub = ap.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("curve", help="按桶归算 ERROR 曲线")
    c.add_argument("paths", nargs="*", help="LogDir 或 *.out.log（可多个）")
    c.add_argument("--logdir", action="append", default=[], help="日志目录（可多次）")
    c.add_argument("--glob", default="*.out.log", help="目录内匹配（默认 *.out.log）")
    c.add_argument("--stats", default="", help="load_stats.ndjson（internal_errors 源）")
    c.add_argument("--anchor", default="",
                   help="HH:mm:ss 日志的日期锚 YYYY-MM-DD（默认取最早输入 mtime）")
    c.add_argument("--bucket-sec", type=int, default=3600, help="桶宽秒（默认 3600=1h，可 21600=6h）")
    c.add_argument("--rise-tol-ratio", type=float, default=1.0,
                   help="「不上升」阈值（尾窗均值 <= ratio × 首窗均值；默认 1.0=严格不上升）")
    c.add_argument("--min-buckets", type=int, default=3, help="判趋势所需最少桶数（默认 3，否则 PREREQ）")
    c.add_argument("--max-slope", type=float, default=None,
                   help="可选绝对斜率上限（桶间 OLS 斜率 > 该值 ⇒ RED）")
    c.add_argument("--gate", action="store_true",
                   help="用退出码判红（默认只报告不判红，rc 恒 0）")
    c.add_argument("--json", default="", help="把结果 JSON 写到该路径")

    sub.add_parser("selftest")
    args = ap.parse_args()

    if args.cmd == "selftest":
        return selftest()
    return cmd_curve(args)


if __name__ == "__main__":
    raise SystemExit(main())
