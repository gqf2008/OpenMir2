#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""attack_rng_ledger.py — M3②③ 攻击对拍的**同种子抽数账本**机判（owner 2026-10-12 17:35，残差①）。

背景：`attack_diff.py diff` 在 k=2 会话上仍差 80+ 项，owner 已裁决「RNG 流位置未对齐」是结论、
不是定位。本脚本把两侧同种子的 `Random` 消费序列按**第 k 次攻击**分段逐项对齐，指出
**具体在第几步、多抽还是少抽**（D2 口径：`RNG同种子与记录回放方案.md`）。

数据源（两侧同形）：
  * 抽数流水 = C# rng-hook（`MIR2_RNG_SEED`/`MIR2_RNG_LOG`/`MIR2_RNG_SITE=1`）写出的 TSV：
        seq \t kind \t max \t result \t site
    形如 `223799\tNext(max)\t1\t0\tM2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929`。
    首两行是 `1\tINJECT_OK\t42\t0` 与 `SEED_PROBE ...`（跳过）。
  * 分段锚 = 攻击路径里**每次攻击恰调用一次**的站点（C# 侧 = `PlayObject.GetAttackPower@PlayObject.cs:2929`，
    真机 120 击实测恰 120 次）。每个锚开启一个 segment = 第 k 次攻击的取数窗口。

子命令（都不连网/不碰 DB/不起栈）：
  summary <trace.tsv> [--top N]
      总数 + 攻击路径取数 vs **噪声取数**（怪 AI `AnimalObject.Wondering`、`Merchant.Run`、
      `MonGetRandomItems` …）的占比与 Top 站点。噪声占比高 ⇒ 「同种子 ⇒ 逐 k 相同」的前提不成立。
  segment <trace.tsv> --marker <site> [--json out.jsonl]
      按 marker 分段，逐段输出 `{"seg":k,"draws_total":N,"draws_attack":A,"draws_noise":M,"first_seq":…,"last_seq":…}`。
  diff <a.segments.jsonl> <b.segments.jsonl> [--max-diffs N]
      跨侧逐 k 对齐：`draws_total/attack/noise` 任一不等即 RED，打印「a 多抽/少抽 X」。长度不等即 RED。
  selftest
      合成件：同账本 ⇒ 绿；某段多一次/少一次 ⇒ 必红且报多抽/少抽；段数不等 ⇒ 必红。

退出码：0 = GREEN；1 = RED（有差异）；2 = PREREQ（文件缺失/无 marker/解析空）。
"""
import argparse
import collections
import json
import os
import re
import sys

# 攻击路径站点（分类用；命中即「有仲裁力的取数」）。与 oracle 攻击链一一对应。
ATTACK_PATH_RE = re.compile(
    r"(PlayObject\.GetAttackPower@|BaseObject\._Attack@|PlayObject\.StruckDamage@|"
    r"BaseObject\.GetHitStruckDamage@|PlayObject\.AttackSuccess@|PlayObject\.AttackDir@)"
)


def parse_trace(path):
    """读 rng-hook TSV ⇒ [(seq, kind, max, result, site)]；跳过 INJECT_OK/SEED_PROBE/空行。"""
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
            site = parts[4] if len(parts) >= 5 else ""
            rows.append((seq, parts[1], parts[2], parts[3], site))
    return rows


def is_attack(site):
    return bool(ATTACK_PATH_RE.search(site or ""))


def cmd_summary(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到抽数流水 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空（钩子没开/格式不符）" % args.trace)
    attack = sum(1 for r in rows if is_attack(r[4]))
    noise = len(rows) - attack
    hist = collections.Counter(r[4] or "(no-site)" for r in rows if not is_attack(r[4]))
    print("RNG_LEDGER_SUMMARY %s total=%d attack_path=%d noise=%d noise_ratio=%.4f" %
          (args.trace, len(rows), attack, noise, (noise / len(rows)) if rows else 0.0))
    for site, n in hist.most_common(args.top):
        print("  NOISE %7d  %s" % (n, site))
    return 0


def cmd_segment(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到抽数流水 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    marker = args.marker
    segments = []
    cur = None
    for seq, kind, mx, res, site in rows:
        if site == marker:
            # marker 自身算作新段的第 1 次取数（每次攻击恰一次 ⇒ 段号 = 第 k 次攻击）。
            cur = {"seg": len(segments) + 1, "draws_total": 0, "draws_attack": 0,
                   "draws_noise": 0, "first_seq": seq, "last_seq": seq,
                   "noise_sites": collections.Counter()}
            segments.append(cur)
        if cur is None:
            continue  # marker 之前的预热/登录取数不计入任何攻击段
        cur["draws_total"] += 1
        cur["last_seq"] = seq
        if is_attack(site):
            cur["draws_attack"] += 1
        else:
            cur["draws_noise"] += 1
            cur["noise_sites"][site or "(no-site)"] += 1
    if not segments:
        return _die("PREREQ_MISSING: %s 里没有 marker `%s`（无攻击段）" % (args.trace, marker))
    out = args.json or (os.path.splitext(args.trace)[0] + ".segments.jsonl")
    with open(out, "w", encoding="utf-8", newline="\n") as fh:
        for s in segments:
            top = s["noise_sites"].most_common(1)
            fh.write(json.dumps({
                "seg": s["seg"], "draws_total": s["draws_total"],
                "draws_attack": s["draws_attack"], "draws_noise": s["draws_noise"],
                "first_seq": s["first_seq"], "last_seq": s["last_seq"],
                "top_noise": (top[0][0] if top else ""),
            }, ensure_ascii=False) + "\n")
    print("RNG_LEDGER_SEGMENTS %s marker=%s segments=%d" % (out, marker, len(segments)))
    for s in segments[:3]:
        print("  SEG k=%d total=%d attack=%d noise=%d first_seq=%d" %
              (s["seg"], s["draws_total"], s["draws_attack"], s["draws_noise"], s["first_seq"]))
    return 0


def load_segments(path):
    segs = []
    with open(path, "r", encoding="utf-8-sig") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            segs.append(json.loads(line))
    return segs


def cmd_diff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到分段账本 %s" % p)
    a = load_segments(args.a)
    b = load_segments(args.b)
    if not a or not b:
        print("RED: 空账本（a=%d b=%d）" % (len(a), len(b)))
        return 1
    problems = []
    if len(a) != len(b):
        problems.append("段数不等: a=%d b=%d" % (len(a), len(b)))
    for i in range(min(len(a), len(b))):
        ra, rb = a[i], b[i]
        for key in ("draws_total", "draws_attack", "draws_noise"):
            va, vb = int(ra.get(key, 0)), int(rb.get(key, 0))
            if va != vb:
                d = vb - va
                problems.append("k=%s %s 不等: a=%d b=%d（b %s %d）" %
                                (ra.get("seg"), key, va, vb, "多抽" if d > 0 else "少抽", abs(d)))
    if not problems:
        print("GREEN: 跨侧抽数账本逐 k 一致（segments=%d；attack=%d noise=%d）" %
              (len(a), a[0].get("draws_attack", 0), a[0].get("draws_noise", 0)))
        return 0
    print("RED: 跨侧抽数账本差异 %d 项（定位到「第几步 / 多抽还是少抽」）" % len(problems))
    for p in problems[:args.max_diffs]:
        print("  - " + p)
    if len(problems) > args.max_diffs:
        print("  ... 另有 %d 项（--max-diffs 截断）" % (len(problems) - args.max_diffs))
    return 1


def _write_trace(path, sites):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        for i, site in enumerate(sites, 1):
            fh.write("%d\tNext(max)\t10\t%d\t%s\n" % (i, i % 10, site))


def cmd_selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="rng-ledger-selftest-")
    checks, failures = [], []

    def check(name, cond, detail=""):
        checks.append(name)
        if not cond:
            failures.append(name + (" (%s)" % detail if detail else ""))

    M = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"
    N = "M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427"
    A = "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"
    # 3 次攻击：每次 marker + 2 攻击路径 + 3 噪声（噪声数故意逐次不同）
    sites_a = [M, A, M, N, N, N, M, A, M, N, N, M, A, M, N, N, N, N]
    # b 与 a 完全同形 ⇒ 绿
    sites_b = list(sites_a)
    ta = os.path.join(tmp, "a.tsv")
    tb = os.path.join(tmp, "b.tsv")
    _write_trace(ta, sites_a)
    _write_trace(tb, sites_b)
    sa = os.path.join(tmp, "a.seg.jsonl")
    sb = os.path.join(tmp, "b.seg.jsonl")
    check("segment-a-rc", cmd_segment(argparse.Namespace(trace=ta, marker=M, json=sa)) == 0)
    check("segment-b-rc", cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb)) == 0)
    rc = cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20))
    check("identical-ledger-green", rc == 0, "rc=%d" % rc)

    # 改坏：给 b 的 k=2 段多插一次噪声 ⇒ 必红且报「b 多抽 1」
    _write_trace(tb, sites_b[:6] + [N] + sites_b[6:])
    cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb))
    out = _capture(lambda: cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20)))
    check("extra-draw-red", out[0] == 1, "rc=%d" % out[0])
    check("extra-draw-shaped", "多抽 1" in out[1], out[1][:200])

    # 改坏：给 b 的 k=1 段少一次攻击路径取数 ⇒ 必红且报「b 少抽」
    _write_trace(tb, [sites_b[0], sites_b[2]] + sites_b[3:])  # 去掉 sites_b[1]（seg1 的攻击路径取数）
    cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb))
    out = _capture(lambda: cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20)))
    check("missing-draw-red", out[0] == 1, "rc=%d" % out[0])
    check("missing-draw-shaped", "少抽" in out[1], out[1][:200])

    # 段数不等 ⇒ 必红
    _write_trace(tb, sites_b[:-1])
    cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb))
    out = _capture(lambda: cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20)))
    check("segcount-mismatch-red", out[0] == 1, "rc=%d" % out[0])

    for name in os.listdir(tmp):
        try:
            os.remove(os.path.join(tmp, name))
        except OSError:
            pass
    os.rmdir(tmp)
    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: attack_rng_ledger selftest %d/%d（同账本绿 + 多抽/少抽必红且定位 + 段数不等必红）"
          % (len(checks), len(checks)))
    return 0


def _capture(fn):
    import io
    import contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = fn()
    return rc, buf.getvalue()


def _die(msg, rc=2):
    print(msg, file=sys.stderr)
    return rc


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 attack RNG draw ledger")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_sum = sub.add_parser("summary")
    p_sum.add_argument("trace")
    p_sum.add_argument("--top", type=int, default=8)
    p_sum.set_defaults(func=cmd_summary)

    p_seg = sub.add_parser("segment")
    p_seg.add_argument("trace")
    p_seg.add_argument("--marker", required=True)
    p_seg.add_argument("--json", default="")
    p_seg.set_defaults(func=cmd_segment)

    p_diff = sub.add_parser("diff")
    p_diff.add_argument("a")
    p_diff.add_argument("b")
    p_diff.add_argument("--max-diffs", type=int, default=20)
    p_diff.set_defaults(func=cmd_diff)

    p_st = sub.add_parser("selftest")
    p_st.set_defaults(func=cmd_selftest)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
