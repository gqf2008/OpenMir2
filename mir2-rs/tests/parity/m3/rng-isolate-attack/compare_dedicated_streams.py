#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M3②③ 活栈隔离口径 —— 两侧「攻击路径专用流」逐次比对器（owner 2026-10-12 22:15）。

## 本工具回答什么

owner 2026-10-12 22:15（brief 顶部）第 1 条：**先证「隔离本身是否真的把两侧放在同一流上」**。
判据不是「两侧结果相同」（那是被测行为），而是两步：

1. **每条流水是否真是 `Random(attack_seed)` 的忠实记录**：把该侧自己的 `(kind, arg)` 请求序列
   喂给本文件内**逐行复刻的 .NET legacy `System.Random`**（`DotNetRandom`，与
   `crates/shared/src/rng.rs` 同算法），逐次比对**结果值**。0 mismatch ⇒ 该侧的
   「按调用点分流到专用种子流」是真的（种子/公式/分流都对齐了）。
2. **两侧的 `(kind, arg)` 请求序列是否相同**：这是被测行为（游戏逻辑）——不同 ⇒ 行为差，
   不是隔离差。工具给出**第一处分歧的序号**与两侧各自的 unit 结构。

同时给出**攻方属性判别**所需的中间量：
- `hit_gate` 的 `arg`（= `RandomByte(target.SpeedPoint)` 的 SpeedPoint）；
- 每击「过门/未过门」（unit 里有没有 `hit_struck_damage`）；
- 由此反解命中门 `if (HitPoint < RandomByte(SpeedPoint)) nPower = 0;` 的
  **`HitPoint` 一致性区间**（`T >= max(命中 draw)` 且 `T < min(未命中 draw)`）。

oracle：`src/M2Server/Actor/BaseObject.Attack.cs:37-44`（`_Attack` 命中门）。

## 用法

    python -I mir2-rs/tests/parity/m3/rng-isolate-attack/compare_dedicated_streams.py analyze \
        --csharp <csharp-attack-stream.tsv> --rust <rust-rng-stream.tsv> [--seed 40545] [--json out.json]
    python -I .../compare_dedicated_streams.py selftest

`--seed` 缺省 40545（= 夹具 `MIR2_RNG_SEED=42` 的 `42 + 0x9E37`）。

## 输入形态（两侧与攻击站点表 `attack_sites.tsv` 同源）

- C# 影子 hook 的**专用流**：`kind \t arg \t result \t site`（site = `Type.Method@file:line`）。
- Rust 的 `MIR2_RNG_LOG` 全局 trace：`seq \t kind \t arg \t result \t site`；本工具**只取**
  site 命中「攻击路径站点」的行（`crates/world/src/combat.rs:*`）。

行号/站点口径见同目录 `attack_sites.tsv`（唯一事实源）。
"""

from __future__ import annotations

import argparse
import json
import sys
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
SITES_TSV = HERE / "attack_sites.tsv"

# ---------------------------------------------------------------------------
# .NET legacy System.Random（`DotNetRandom`）逐行复刻 —— 与 crates/shared/src/rng.rs 同算法。
# ---------------------------------------------------------------------------
MBIG = 2147483647
MSEED = 161803398


class DotNetRandom:
    def __init__(self, seed: int) -> None:
        subtraction = MBIG if seed == -2147483648 else abs(seed)
        mj = MSEED - subtraction
        a = [0] * 56
        a[55] = mj
        mk = 1
        for i in range(1, 55):
            ii = (21 * i) % 55
            a[ii] = mk
            mk = mj - mk
            if mk < 0:
                mk += MBIG
            mj = a[ii]
        for _ in range(4):
            for i in range(1, 56):
                a[i] -= a[1 + (i + 30) % 55]
                if a[i] < 0:
                    a[i] += MBIG
        self.a = a
        self.ix = 0
        self.ixp = 21

    def _sample_int(self) -> int:
        li = self.ix + 1
        lp = self.ixp + 1
        if li >= 56:
            li = 1
        if lp >= 56:
            lp = 1
        r = self.a[li] - self.a[lp]
        if r == MBIG:
            r -= 1
        if r < 0:
            r += MBIG
        self.a[li] = r
        self.ix = li
        self.ixp = lp
        return r

    def _sample(self) -> float:
        return self._sample_int() * (1.0 / MBIG)

    def next_value(self) -> int:
        return self._sample_int()

    def next_max(self, max_value: int) -> int:
        return int(self._sample() * max_value)

    def next_range(self, min_value: int, max_value: int) -> int:
        return int(self._sample() * (max_value - min_value)) + min_value


def _arg_min_max(arg: int) -> tuple[int, int]:
    """C# `RecordingRandom` 的 `Next(min,max)` 参数编码：`((long)min << 32) | (uint)max`。"""
    lo = arg & 0xFFFFFFFF
    hi = arg >> 32
    if hi >= 0x80000000:
        hi -= 0x100000000
    return hi, lo


def replay(kind: str, arg: int, rng: DotNetRandom) -> int:
    if kind == "Next()":
        return rng.next_value()
    if kind == "Next(max)":
        return rng.next_max(arg)
    if kind == "Next(min,max)":
        lo, hi = _arg_min_max(arg)
        return rng.next_range(lo, hi)
    raise ValueError("未知 kind: %r" % kind)


# ---------------------------------------------------------------------------
# 站点 → canon（唯一事实源 = attack_sites.tsv）
# ---------------------------------------------------------------------------
def load_site_map(tsv: Path = SITES_TSV) -> dict[str, list[tuple[re.Pattern, str]]]:
    out: dict[str, list[tuple[re.Pattern, str]]] = {"csharp": [], "rust": []}
    for line in tsv.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        cols = line.split("\t")
        if len(cols) < 3 or cols[0] == "canon":
            continue
        canon, side, pattern = cols[0], cols[1], cols[2]
        if side not in out:
            continue
        out[side].append((re.compile(pattern), canon))
    return out


def canon_of(side: str, site: str, smap) -> str | None:
    norm = site.replace("\\", "/")
    for pat, canon in smap[side]:
        if pat.search(norm):
            return canon
    return None


class Entry:
    __slots__ = ("kind", "arg", "result", "canon", "site")

    def __init__(self, kind, arg, result, canon, site):
        self.kind = kind
        self.arg = int(arg)
        self.result = int(result)
        self.canon = canon
        self.site = site

    def __repr__(self):
        return "Entry(%s,%d->%d,%s)" % (self.kind, self.arg, self.result, self.canon)


def parse_csharp(path: Path, smap) -> list[Entry]:
    """C# 专用流：`kind \\t arg \\t result \\t site`（无 seq、无头）。"""
    out = []
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        if not raw or "\t" not in raw:
            continue
        p = raw.split("\t")
        if len(p) < 4:
            continue
        if not p[3] or p[3][0].isdigit():
            continue  # 头行/说明行
        canon = canon_of("csharp", p[3], smap)
        if canon is None:
            continue
        out.append(Entry(p[0], p[1], p[2], canon, p[3]))
    return out


def parse_rust(path: Path, smap) -> list[Entry]:
    """Rust 全局 trace：`seq \\t kind \\t arg \\t result \\t site`；只保留攻击站点行。"""
    out = []
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        p = raw.split("\t")
        if len(p) < 5:
            continue
        if not p[0].isdigit() or not p[1].startswith("Next"):
            continue
        canon = canon_of("rust", p[4], smap)
        if canon is None:
            continue
        out.append(Entry(p[1], p[2], p[3], canon, p[4]))
    return out


# ---------------------------------------------------------------------------
# 分析
# ---------------------------------------------------------------------------
ATTACK_STARTS = {"atk_power_a", "atk_power_luck_a", "atk_power_base"}


def segment_units(seq: list[Entry]) -> list[list[Entry]]:
    units: list[list[Entry]] = []
    cur: list[Entry] | None = None
    for e in seq:
        if e.canon in ATTACK_STARTS:
            if cur:
                units.append(cur)
            cur = []
        if cur is None:
            cur = []
        cur.append(e)
    if cur:
        units.append(cur)
    return units


def counts(seq: list[Entry]) -> dict[str, int]:
    out: dict[str, int] = {}
    for e in seq:
        out[e.canon] = out.get(e.canon, 0) + 1
    return out


def self_replay(seq: list[Entry], seed: int) -> list[tuple[int, Entry, int]]:
    """返回 mismatch 列表：(1-based index, entry, replayed_value)。"""
    rng = DotNetRandom(seed)
    bad = []
    for i, e in enumerate(seq, 1):
        v = replay(e.kind, e.arg, rng)
        if v != e.result:
            bad.append((i, e, v))
    return bad


def first_request_divergence(a: list[Entry], b: list[Entry]):
    n = min(len(a), len(b))
    for i in range(n):
        if (a[i].kind, a[i].arg) != (b[i].kind, b[i].arg):
            return i + 1, a[i], b[i]
    if len(a) != len(b):
        longer = "csharp" if len(a) > len(b) else "rust"
        return n + 1, (a[n] if longer == "csharp" else None), (b[n] if longer == "rust" else None)
    return None, None, None


def hitpoint_interval(units: list[list[Entry]]) -> dict:
    """从 player unit 的命中门 draw + 是否过门反解 `HitPoint` 一致区间。

    oracle `BaseObject.Attack.cs:41`：`if (HitPoint < RandomByte(SpeedPoint)) nPower = 0;`
    ⇒ 命中 ⇔ `draw <= HitPoint`；未命中 ⇔ `draw > HitPoint`。
    因此 `HitPoint in [max(hit draws), min(miss draws))`。
    """
    hits, misses, det = [], [], []
    for u in units:
        if u[0].canon not in ("atk_power_a", "atk_power_luck_a"):
            continue
        gates = [e for e in u if e.canon == "hit_gate"]
        if not gates:
            continue
        g = gates[0]
        hit = any(e.canon == "hit_struck_damage" for e in u)
        det.append({"speed_point": g.arg, "draw": g.result, "hit": hit})
        (hits if hit else misses).append(g.result)
    lo = max(hits) if hits else None
    hi = min(misses) if misses else None
    consistent = lo is not None and hi is not None and lo < hi
    return {
        "gates": len(det),
        "hits": len(hits),
        "misses": len(misses),
        "hit_draws": sorted(hits),
        "miss_draws": sorted(misses),
        "speed_points": sorted({d["speed_point"] for d in det}),
        "hitpoint_lo_inclusive": lo,
        "hitpoint_hi_exclusive": hi,
        "consistent": consistent,
        "detail": det,
    }


def unit_structure(units: list[list[Entry]]) -> list[dict]:
    out = []
    for i, u in enumerate(units, 1):
        out.append(
            {
                "unit": i,
                "start": u[0].canon,
                "kind": "base" if u[0].canon == "atk_power_base" else "player",
                "ops": ["%s(%d)->%d" % (e.canon, e.arg, e.result) for e in u],
            }
        )
    return out


def analyze(csharp: Path, rust: Path, seed: int) -> dict:
    smap = load_site_map()
    cs = parse_csharp(csharp, smap)
    rt = parse_rust(rust, smap)
    cs_units = segment_units(cs)
    rt_units = segment_units(rt)
    idx, ae, be = first_request_divergence(cs, rt)
    return {
        "seed": seed,
        "csharp": {
            "path": str(csharp),
            "n": len(cs),
            "site_counts": counts(cs),
            "self_replay_mismatches": len(self_replay(cs, seed)),
            "units": len(cs_units),
            "base_units": sum(1 for u in cs_units if u[0].canon == "atk_power_base"),
            "player_units": sum(1 for u in cs_units if u[0].canon != "atk_power_base"),
            "hitpoint": hitpoint_interval(cs_units),
            "structure": unit_structure(cs_units),
        },
        "rust": {
            "path": str(rust),
            "n": len(rt),
            "site_counts": counts(rt),
            "self_replay_mismatches": len(self_replay(rt, seed)),
            "units": len(rt_units),
            "base_units": sum(1 for u in rt_units if u[0].canon == "atk_power_base"),
            "player_units": sum(1 for u in rt_units if u[0].canon != "atk_power_base"),
            "hitpoint": hitpoint_interval(rt_units),
            "structure": unit_structure(rt_units),
        },
        "first_request_divergence_index": idx,
        "first_request_divergence_csharp": None if ae is None else ae.__repr__(),
        "first_request_divergence_rust": None if be is None else be.__repr__(),
    }


def print_report(r: dict) -> None:
    print("=== M3 隔离口径 · 两侧攻击路径专用流逐次比对 ===")
    print("attack_seed =", r["seed"])
    for side in ("csharp", "rust"):
        s = r[side]
        print("\n--- %s: %s" % (side, s["path"]))
        print("  取数条数 = %d（站点计数：%s）" % (s["n"], s["site_counts"]))
        print("  自复算 mismatch = %d（>0 ⇒ 该侧流水不是 Random(%d) 的忠实记录）" % (s["self_replay_mismatches"], r["seed"]))
        print("  攻击单元 = %d（player=%d, base=%d）" % (s["units"], s["player_units"], s["base_units"]))
        h = s["hitpoint"]
        print(
            "  player 命中门：gates=%d hits=%d misses=%d；SpeedPoint=%s；"
            "HitPoint ∈ [%s, %s)" % (
                h["gates"], h["hits"], h["misses"], h["speed_points"],
                h["hitpoint_lo_inclusive"], h["hitpoint_hi_exclusive"],
            )
        )
    print("\n--- 请求序列第一处分歧")
    print("  index =", r["first_request_divergence_index"])
    print("  csharp:", r["first_request_divergence_csharp"])
    print("  rust  :", r["first_request_divergence_rust"])


def selftest() -> int:
    """牙齿：改坏必红（值、请求序列、阈值区间三向）。"""
    fails: list[str] = []
    # 1) DotNetRandom(42) 参照序列（与真机 SEED_PROBE 同源）
    d = DotNetRandom(42)
    probe = [d.next_value() for _ in range(8)]
    expect = [1434747710, 302596119, 269548474, 1122627734, 361709742, 563913476, 1555655117, 1101493307]
    if probe != expect:
        fails.append("seed42_probe")
    # 2) 自复算的 teeth：把一个结果值 +1 ⇒ 必报 1 mismatch
    seq = [
        Entry("Next(max)", 1, 0, "atk_power_a", "x"),
        Entry("Next(max)", 10, 4, "atk_power_b", "x"),
        Entry("Next(max)", 15, 3, "hit_gate", "x"),
    ]
    if len(self_replay(seq, 40545)) != 0:
        fails.append("clean_replay_must_be_0")
    seq[1].result = 5
    if len(self_replay(seq, 40545)) != 1:
        fails.append("mutated_replay_must_be_1")
    # 3) 请求序列分歧 teeth
    a = [Entry("Next(max)", 1, 0, "atk_power_a", "x")]
    b = [Entry("Next(max)", 10, 0, "atk_power_b", "x")]
    idx, _, _ = first_request_divergence(a, b)
    if idx != 1:
        fails.append("first_divergence_must_be_1")
    # 4) HitPoint 区间 teeth（hit=5 / miss=6 ⇒ [5,6)）
    u = [
        [Entry("Next(max)", 1, 0, "atk_power_a", "x"), Entry("Next(max)", 15, 5, "hit_gate", "x"), Entry("Next(min,max)", 1, 0, "hit_struck_damage", "x")],
        [Entry("Next(max)", 1, 0, "atk_power_a", "x"), Entry("Next(max)", 15, 6, "hit_gate", "x")],
    ]
    h = hitpoint_interval(u)
    if not (h["hitpoint_lo_inclusive"] == 5 and h["hitpoint_hi_exclusive"] == 6 and h["consistent"]):
        fails.append("hitpoint_interval_must_be_5_6")
    if fails:
        print("SELFTEST RED: " + ",".join(fails))
        return 1
    print("SELFTEST GREEN: 9/9")
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="M3 隔离口径两侧专用流逐次比对")
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("analyze")
    a.add_argument("--csharp", required=True)
    a.add_argument("--rust", required=True)
    a.add_argument("--seed", type=int, default=40545)
    a.add_argument("--json")
    sub.add_parser("selftest")
    args = ap.parse_args(argv)

    if args.cmd == "selftest":
        return selftest()
    r = analyze(Path(args.csharp), Path(args.rust), args.seed)
    print_report(r)
    if args.json:
        Path(args.json).write_text(json.dumps(r, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        print("\n[json] " + args.json)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
