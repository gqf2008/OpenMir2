#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""analyze_rng_trace.py — M3②③ 决策片（owner 2026-10-12 18:00）的抽数流**交错分析**。

问题：把 C# `rng-hook` 的 `rng-stream.tsv` 与 Rust `rng_trace`（`crates/shared/src/rng_trace.rs`）
按「每次攻击恰一次的站点（marker）」分段后，噪声取数到底落在哪里？

- 若攻击路径的取数在流里是**连续块**（两次攻击取数之间 0 条噪声）
  ⇒ 「攻击窗口前两侧同步重播种」原则上能救；
- 若连续两次攻击取数之间**夹着噪声**（尤其同一击内部）
  ⇒ 全局随机源被并发消费者（怪 AI / NPC / 刷怪 / 线程）插入 ⇒ 重播种不够。

子命令（离线；不连网/不碰 DB/不起栈）：
  summary   <trace.tsv> --attack-re RE [--top N]
      总数 + 攻击路径 vs 噪声占比 + Top 噪声站点。
  interleave <trace.tsv> --marker SITE [--attack-re RE] [--max-intra-gap N]
      按 marker 逐击分段，给出**攻击取数之间夹噪声**的分布（决策片的直接输出）。
  selftest
      合成件：连续块 ⇒ intra_noise_attacks=0；夹一条噪声 ⇒ 必红（intra_noise>0 且定位到段）。

站点分隔符归一：Windows 落盘是 `crates\\world\\src\\combat.rs:109`，统一成 `/` 再比。
退出码：0 = GREEN（分析完成）；1 = RED（自检失败）；2 = PREREQ（文件缺失/无 marker/解析空）。
"""
import argparse
import collections
import json
import os
import re
import sys

# C# 攻击路径站点（与 m3bag attack_rng_ledger.py 同表 + `GetAttackPower` override 两条线）。
CSHARP_ATTACK_RE = (
    r"(PlayObject\.GetAttackPower@|BaseObject\._Attack@|PlayObject\.StruckDamage@|"
    r"BaseObject\.GetHitStruckDamage@|PlayObject\.AttackSuccess@|PlayObject\.AttackDir@)"
)


def norm(site):
    return (site or "").replace("\\", "/")


def parse_trace(path):
    rows = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\r\n")
            if not line or line.startswith("SEED_PROBE") or "INJECT_OK" in line:
                continue
            parts = line.split("\t")
            if len(parts) < 4:
                continue
            try:
                seq = int(parts[0])
            except ValueError:
                continue
            site = norm(parts[4] if len(parts) >= 5 else "")
            rows.append((seq, parts[1], parts[2], parts[3], site))
    return rows


def cmd_summary(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    rx = re.compile(args.attack_re)
    attack = sum(1 for r in rows if rx.search(r[4]))
    noise = len(rows) - attack
    print("RNG_TRACE_SUMMARY %s total=%d attack_path=%d noise=%d noise_ratio=%.4f"
          % (args.trace, len(rows), attack, noise, (noise / len(rows)) if rows else 0.0))
    hist = collections.Counter(r[4] or "(no-site)" for r in rows if not rx.search(r[4]))
    for site, n in hist.most_common(args.top):
        print("  NOISE %8d  %s" % (n, site))
    return 0


def cmd_interleave(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    marker = norm(args.marker)
    rx = re.compile(args.attack_re)
    groups = []  # [{"marker":bool,"draws":[(seq,site),...]}]
    cur = None

    def is_attack(site):
        return site == marker or bool(rx.search(site))

    for seq, _kind, _arg, _res, site in rows:
        if site == marker:
            cur = {"marker": True, "draws": [(seq, site)]}
            groups.append(cur)
            continue
        if is_attack(site):
            if cur is None or (seq - cur["draws"][-1][0]) > args.max_intra_gap:
                cur = {"marker": False, "draws": [(seq, site)]}
                groups.append(cur)
            else:
                cur["draws"].append((seq, site))

    marked = [g for g in groups if g["marker"]]
    if not marked:
        return _die("PREREQ_MISSING: %s 里没有 marker `%s`（无攻击段）" % (args.trace, args.marker))

    # 只统计「完整」攻击段：段内攻击取数 >= 2（至少 marker + 1 命中路径取数）。
    intra_noise_attacks = 0
    total_internal_noise = 0
    max_internal_noise = 0
    gap_hist = collections.Counter()
    examples = []
    for k, g in enumerate(marked, 1):
        seqs = [d[0] for d in g["draws"]]
        gaps = [b - a - 1 for a, b in zip(seqs, seqs[1:])]
        internal = sum(x for x in gaps if x > 0)
        if internal > 0:
            intra_noise_attacks += 1
            max_internal_noise = max(max_internal_noise, internal)
            if len(examples) < 3:
                examples.append((k, seqs, gaps))
        total_internal_noise += internal
        for x in gaps:
            gap_hist["0" if x == 0 else ("1" if x == 1 else ("2-9" if x <= 9 else "10-99" if x <= 99 else ">=100"))] += 1

    print("RNG_TRACE_INTERLEAVE %s marker=%s attacks=%d" % (args.trace, args.marker, len(marked)))
    print("  intra_noise_attacks=%d/%d（同一击内部两次取数之间夹噪声的攻击数）" %
          (intra_noise_attacks, len(marked)))
    print("  internal_noise_total=%d internal_noise_max=%d" % (total_internal_noise, max_internal_noise))
    print("  attack_draw_gap_histogram=%s" % dict(gap_hist))

    # 击与击之间（上一击最后一次攻击取数 → 下一击 marker）的噪声条数——「窗口之间的噪声」量级。
    inter_gaps = []
    for prev, nxt in zip(marked, marked[1:]):
        inter_gaps.append(nxt["draws"][0][0] - prev["draws"][-1][0] - 1)
    if inter_gaps:
        srt = sorted(inter_gaps)
        print("  inter_attack_noise min=%d median=%d max=%d" %
              (srt[0], srt[len(srt) // 2], srt[-1]))
        seg = collections.Counter()
        for g in inter_gaps:
            seg["<=10" if g <= 10 else ("11-100" if g <= 100 else ("101-1000" if g <= 1000 else ">1000"))] += 1
        print("  inter_attack_noise_histogram=%s" % dict(seg))
    for k, seqs, gaps in examples:
        print("  EXAMPLE attack#%d seqs=%s gaps=%s" % (k, seqs, gaps))
    # 决策：噪声是否夹在攻击路径两次取数之间。
    verdict = "INTERLEAVED（噪声夹在攻击取数之间 ⇒ 重播种不足）" if intra_noise_attacks else \
        "CONTIGUOUS（攻击取数为连续块 ⇒ 窗口前重播种原则上足够）"
    print("  VERDICT: %s" % verdict)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({
                "trace": args.trace, "marker": args.marker, "attacks": len(marked),
                "intra_noise_attacks": intra_noise_attacks,
                "internal_noise_total": total_internal_noise,
                "internal_noise_max": max_internal_noise,
                "attack_draw_gap_histogram": dict(gap_hist),
                "verdict": verdict,
            }, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def _write(path, sites):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        for i, site in enumerate(sites, 1):
            fh.write("%d\tNext(max)\t10\t%d\t%s\n" % (i, i % 10, site))


def cmd_selftest(args):
    import io
    import tempfile
    import contextlib

    tmp = tempfile.mkdtemp(prefix="rng-trace-probe-selftest-")
    M = "crates/world/src/combat.rs:109"
    A = "crates/world/src/combat.rs:606"
    A2 = "crates/world/src/combat.rs:579"
    N = "crates/world/src/animal.rs:427"
    RE = r"combat\.rs:(109|579|606)"
    checks, failures = [], []

    def check(name, cond, detail=""):
        checks.append(name)
        if not cond:
            failures.append(name + (" (%s)" % detail if detail else ""))

    def cap(fn):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
            rc = fn()
        return rc, buf.getvalue()

    def analyze(sites):
        p = os.path.join(tmp, "t.tsv")
        _write(p, sites)
        return cap(lambda: cmd_interleave(argparse.Namespace(
            trace=p, marker=M, attack_re=RE, max_intra_gap=64, json="")))

    # 连续块（每次攻击 marker + 2 取数紧邻）⇒ intra_noise_attacks=0
    rc, out = analyze([M, A, A2, M, A, A2, M, A, A2])
    check("contiguous-green", rc == 0 and "intra_noise_attacks=0/3" in out, out[:200])
    check("contiguous-verdict", "CONTIGUOUS" in out, out[:200])

    # 攻击取数之间夹 1 条噪声（marker, A, N, A2）⇒ intra_noise>0
    rc, out = analyze([M, A, N, A2, M, A, A2])
    check("interleaved-detected", rc == 0 and "intra_noise_attacks=1/2" in out, out[:300])
    check("interleaved-verdict", "INTERLEAVED" in out, out[:200])

    # 无 marker ⇒ PREREQ
    rc, out = analyze([A, A2, A, A2])
    check("no-marker-prereq", rc == 2, "rc=%d" % rc)

    for name in os.listdir(tmp):
        try:
            os.remove(os.path.join(tmp, name))
        except OSError:
            pass
    os.rmdir(tmp)
    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: analyze_rng_trace selftest %d/%d（连续块绿 + 夹噪声必红/定位 + 无 marker PREREQ）"
          % (len(checks), len(checks)))
    return 0


def _die(msg, rc=2):
    print(msg, file=sys.stderr)
    return rc


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 RNG trace interleave analyzer")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("summary")
    p.add_argument("trace")
    p.add_argument("--attack-re", default=CSHARP_ATTACK_RE)
    p.add_argument("--top", type=int, default=8)
    p.set_defaults(func=cmd_summary)

    p = sub.add_parser("interleave")
    p.add_argument("trace")
    p.add_argument("--marker", required=True)
    p.add_argument("--attack-re", default=CSHARP_ATTACK_RE)
    p.add_argument("--max-intra-gap", type=int, default=64)
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_interleave)

    p = sub.add_parser("selftest")
    p.set_defaults(func=cmd_selftest)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
