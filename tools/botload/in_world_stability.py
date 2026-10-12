#!/usr/bin/env python3
"""M3 总验收 DoD② 判据 (b)：`in_world` 曲线在稳态段无跌落（机判）。

背景（`docs/Rust重写设计文档.md` §6 M3 总验收 + 卡 `openmir2-m3-final-acceptance`）：
300 假人 24h 长跑的判据 (b) 是「假人 `in_world` 数与连接数的曲线在稳态段无跌落」。
连接数一侧由 `load_report.py` 的 `conn_lost_steady == 0` 判；**本工具判 `in_world` 一侧**：

    稳态段 = [首行 t_ms + ramp_ms, 末行 t_ms - tail_ms]（默认 ramp=1h、tail=5min）
    按 bucket_ms（默认 5min）分桶，取每桶 in_world 均值；
    判据   = min(桶均值) >= min_ratio × max(桶均值)   （默认 0.95）

口径纪律（§6.0）：
  * 缺 `t_ms` / 缺 `in_world` / 非法 JSON ⇒ **PREREQ（rc=2，fail-closed）**，绝不静默当绿；
  * 稳态段为空（窗口太短 / ramp 太大）⇒ PREREQ；
  * 桶均值必然受「收工瞬间假人逐个退出」影响 ⇒ 用 `tail_ms` 排除末尾，不用最后一行当稳态值；
  * `--selftest` 给出改坏必红牙齿：丢一桶 ⇒ RED、缺字段/坏 JSON ⇒ PREREQ、无稳态段 ⇒ PREREQ。

退出码：0=GREEN，1=RED，2=PREREQ。

用法：
    python -I tools/botload/in_world_stability.py <load_stats.ndjson> \
        [--ramp-ms 3600000] [--bucket-ms 300000] [--tail-ms 300000] [--min-ratio 0.95] [--json out.json]
    python -I tools/botload/in_world_stability.py selftest
"""
from __future__ import annotations

import argparse
import json
import statistics
import sys
import tempfile
from pathlib import Path

RC_GREEN = 0
RC_RED = 1
RC_PREREQ = 2


def load_rows(path: Path) -> tuple[list[dict], list[str]]:
    """读 ndjson；返回 (rows, prereq_reasons)。缺字段一律进 prereq_reasons（fail-closed）。"""
    rows: list[dict] = []
    reasons: list[str] = []
    text = path.read_text(encoding="utf-8", errors="replace")
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
        if "in_world" not in obj:
            reasons.append(f"line {lineno}: 缺 in_world（本工具判的就是它，缺即 PREREQ）")
            continue
        rows.append(obj)
    if not rows:
        reasons.append("没有任何含 (t_ms, in_world) 的行")
    return rows, reasons


def evaluate(rows: list[dict], ramp_ms: int, bucket_ms: int, tail_ms: int, min_ratio: float) -> dict:
    rows = sorted(rows, key=lambda r: int(r["t_ms"]))
    t0 = int(rows[0]["t_ms"])
    t1 = int(rows[-1]["t_ms"])
    ramp_end = t0 + ramp_ms
    steady_end = t1 - tail_ms
    steady = [r for r in rows if ramp_end <= int(r["t_ms"]) <= steady_end]
    out: dict = {
        "rows": len(rows),
        "t0_ms": t0,
        "t1_ms": t1,
        "ramp_ms": ramp_ms,
        "bucket_ms": bucket_ms,
        "tail_ms": tail_ms,
        "min_ratio": min_ratio,
        "steady_rows": len(steady),
        "steady_start_ms": ramp_end,
        "steady_end_ms": steady_end,
    }
    if not steady:
        out["verdict"] = "PREREQ"
        out["prereq"] = "稳态段为空（ramp_ms 太大或窗口太短）"
        return out
    buckets: dict[int, list[int]] = {}
    for r in steady:
        b = (int(r["t_ms"]) - ramp_end) // bucket_ms
        buckets.setdefault(b, []).append(int(r["in_world"]))
    means = {b: statistics.fmean(v) for b, v in sorted(buckets.items())}
    out["buckets"] = len(means)
    out["bucket_means"] = {str(b): round(m, 3) for b, m in means.items()}
    out["min_bucket_mean"] = round(min(means.values()), 3)
    out["max_bucket_mean"] = round(max(means.values()), 3)
    out["in_world_drop"] = round(max(means.values()) - min(means.values()), 3)
    ok = min(means.values()) >= min_ratio * max(means.values())
    out["verdict"] = "GREEN" if ok else "RED"
    return out


def _stats(t_start: int, n: int, step: int, fn) -> list[dict]:
    return [{"t_ms": t_start + i * step, "in_world": fn(i)} for i in range(n)]


def selftest() -> int:
    ramp, bucket, tail = 60_000, 30_000, 30_000
    ratio = 0.95
    cases = 0

    def expect(name: str, rows: list[dict], want: str) -> None:
        nonlocal cases
        cases += 1
        got = evaluate(rows, ramp, bucket, tail, ratio)["verdict"]
        assert got == want, f"{name}: want {want}, got {got}"

    def expect_prereq(name: str, text: str) -> None:
        nonlocal cases
        cases += 1
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "stats.ndjson"
            p.write_text(text, encoding="utf-8", newline="\n")
            _, reasons = load_rows(p)
            assert reasons, f"{name}: 必须 PREREQ（reasons 非空），实测空"

    # 1) 爬坡后稳态平（30 点 × 10s = 300s）：GREEN
    expect("爬坡后平", _stats(0, 30, 10_000, lambda i: 0 if i < 3 else 100), "GREEN")
    # 2) 稳态中一个桶掉到 50%：RED（改坏必红：丢了桶仍须判红）
    expect("稳态掉一桶", _stats(0, 30, 10_000, lambda i: (50 if 10 <= i < 13 else (0 if i < 3 else 100))), "RED")
    # 3) 轻微抖动 96%/100%（ratio 0.95）：GREEN
    expect("抖动 96%", _stats(0, 30, 10_000, lambda i: (96 if 10 <= i < 13 else (0 if i < 3 else 100))), "GREEN")
    # 4) 缺 in_world：PREREQ（fail-closed，不许静默当绿）
    expect_prereq("缺 in_world", '{"t_ms":0,"login_ok":1}\n{"t_ms":10000}\n')
    # 5) 非法 JSON：PREREQ
    expect_prereq("非法 JSON", '{"t_ms":0,"in_world":1}\nnot-json\n')
    # 6) 全在 ramp 内（窗口太短）：PREREQ
    expect("无稳态段", _stats(0, 4, 10_000, lambda i: 100), "PREREQ")  # t1=30000 < ramp=60000
    # 7) 稳态缓升 100→102（无跌落；min/max=0.980 ≥ 0.95）：GREEN
    expect("稳态缓升", _stats(0, 30, 10_000, lambda i: 100 + i // 10), "GREEN")

    print(f"SELFTEST GREEN: in_world_stability {cases}/{cases}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="M3 DoD② (b)：in_world 稳态段无跌落机判")
    ap.add_argument("stats", help="load_stats.ndjson 路径，或 'selftest'")
    ap.add_argument("--ramp-ms", type=int, default=3_600_000, help="爬坡期（默认 1h）")
    ap.add_argument("--bucket-ms", type=int, default=300_000, help="分桶宽度（默认 5min）")
    ap.add_argument("--tail-ms", type=int, default=300_000, help="稳态段末尾排除（默认 5min）")
    ap.add_argument("--min-ratio", type=float, default=0.95, help="桶均值下限比（默认 0.95）")
    ap.add_argument("--json", default="", help="把结果 JSON 写到该路径")
    args = ap.parse_args()

    if args.stats == "selftest":
        return selftest()

    path = Path(args.stats)
    if not path.is_file():
        print(f"PREREQ: 缺目标 {path}", file=sys.stderr)
        return RC_PREREQ
    rows, reasons = load_rows(path)
    if reasons:
        print(f"PREREQ: {path}", file=sys.stderr)
        for r in reasons[:20]:
            print(f"  - {r}", file=sys.stderr)
        return RC_PREREQ
    out = evaluate(rows, args.ramp_ms, args.bucket_ms, args.tail_ms, args.min_ratio)
    if args.json:
        Path(args.json).write_text(
            json.dumps(out, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n"
        )
    print(json.dumps(out, ensure_ascii=False, indent=2))
    if out["verdict"] == "PREREQ":
        print(f"PREREQ: {out['prereq']}", file=sys.stderr)
        return RC_PREREQ
    if out["verdict"] == "GREEN":
        print(
            f"GREEN: in_world 稳态段无跌落（桶均值 min={out['min_bucket_mean']} "
            f">= {args.min_ratio} × max={out['max_bucket_mean']}；drop={out['in_world_drop']}）"
        )
        return RC_GREEN
    print(
        f"RED: in_world 稳态段有跌落（桶均值 min={out['min_bucket_mean']} "
        f"< {args.min_ratio} × max={out['max_bucket_mean']}；drop={out['in_world_drop']}）",
        file=sys.stderr,
    )
    return RC_RED


if __name__ == "__main__":
    raise SystemExit(main())
