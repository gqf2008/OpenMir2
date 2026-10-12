#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""rng_ledger_diff.py — M3②③ 真机**两侧 RNG 账本**的站点 + 顺序 diff（owner 2026-10-12 18:25）。

背景：决策片（`evidence-rng-trace-20261012`）只跑了 **Rust 离线单线程** 的 trace，owner 指出
「Rust 生产活栈的账本仍缺失」⇒ 本工具把 **C# 活栈**（`rng-hook` 的 `rng-stream.tsv`）与
**Rust 活栈**（`crates/shared/src/rng_trace.rs` 的 `MIR2_RNG_LOG`）按 **site + 顺序** 对齐：

  summary <trace.tsv> [--side csharp|rust] [--json out]
      总数 + 攻击路径/噪声占比 + 全站点直方图（口径与 `attack_rng_ledger.py` 同表）。
  order  <trace.tsv> [--side csharp|rust] [--attack-only] [--out order.txt]
      把 trace 的 site 序列**规范化**成 canonical 名（见 `CANON_RULES`）后逐行输出；
      `--attack-only` 只留攻击路径（两侧可映射到同一 vocabulary）。
  diff   <a.tsv> <b.tsv> [--attack-only] [--json out]
      顺序敏感：按 canonical 序列**逐位**比，报**第一处分歧**（第几位、两侧各是什么 site；
      `--attack-only` 时两侧 vocabulary 同表，可直接判「缺消费者 / 次序不同」）；
      另报两侧 per-canonical 计数、只在 A / 只在 B 的 canonical（= 缺消费者的点名清单）。
  selftest
      合成件：同序 ⇒ 绿；插入一次取数（次序漂移）⇒ 必红且定位到第几位；缺一个站点 ⇒ 必红且点名。

退出码：0 = GREEN（分析完成 / 自检过）；1 = RED（自检失败）；2 = PREREQ（文件缺失/解析空）。

站点表与 m3cs 决策片 `analyze_rng_trace.py` / m3bag `attack_rng_ledger.py` **同口径**
（C# = `PlayObject.GetAttackPower@PlayObject.cs:2929` 为每次攻击恰一次的 marker；
Rust = `crates\world\src\combat.rs:109`）。
"""
import argparse
import collections
import json
import os
import re
import sys

# C# 站点（含 `@类名.cs:行号` 全名）。
CSHARP_ATTACK_RE = re.compile(
    r"(PlayObject\.GetAttackPower@|BaseObject\._Attack@|PlayObject\.StruckDamage@|"
    r"BaseObject\.GetHitStruckDamage@|PlayObject\.AttackSuccess@|PlayObject\.AttackDir@|"
    r"BaseObject\.GetAttackPower@)")
# Rust 站点（世界层调用点）。
RUST_ATTACK_RE = re.compile(r"combat\.rs:(67|109|111|250|579|587|606|608|736)|magic\.rs:")

CSHARP_MARKER = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"
RUST_MARKER = r"crates\world\src\combat.rs:109"

# canonical 映射：两侧语义一一对应的站点 → 同一名（顺序敏感比对才有仲裁力）。
CANON_RULES = [
    # C#（oracle）
    (re.compile(r"M2Server\.Player\.PlayObject\.GetAttackPower@PlayObject\.cs:2929$"), "atk_power_a"),
    (re.compile(r"M2Server\.Player\.PlayObject\.GetAttackPower@PlayObject\.cs:2934$"), "atk_power_b"),
    (re.compile(r"M2Server\.Actor\.BaseObject\.GetAttackPower@BaseObject\.Base\.cs:892$"), "atk_power_base"),
    (re.compile(r"M2Server\.Actor\.BaseObject\._Attack@BaseObject\.Attack\.cs:41$"), "hit_gate"),
    (re.compile(r"M2Server\.Player\.PlayObject\.AttackSuccess@PlayObject\.Attack\.cs:223$"), "attack_success"),
    (re.compile(r"M2Server\.Player\.PlayObject\.StruckDamage@PlayObject\.cs:2957$"), "struck_durability"),
    (re.compile(r"M2Server\.Actor\.BaseObject\.GetHitStruckDamage@BaseObject\.Base\.cs:792$"), "hit_struck_damage"),
    # Rust（世界层）
    (re.compile(r"crates/world/src/combat\.rs:109$"), "atk_power_a"),
    (re.compile(r"crates/world/src/combat\.rs:111$"), "atk_power_b"),
    (re.compile(r"crates/world/src/combat\.rs:67$"), "atk_power_base"),
    (re.compile(r"crates/world/src/combat\.rs:736$"), "hit_gate"),
    (re.compile(r"crates/world/src/combat\.rs:606$"), "hit_struck_damage"),
    (re.compile(r"crates/world/src/combat\.rs:608$"), "hit_struck_damage2"),
    (re.compile(r"crates/world/src/combat\.rs:579$"), "struck_durability"),
    (re.compile(r"crates/world/src/combat\.rs:587$"), "struck_durability_slot"),
    (re.compile(r"crates/world/src/combat\.rs:250$"), "attack_skill_point_count"),
]

ATTACK_CANON = {
    "atk_power_a", "atk_power_b", "atk_power_base", "hit_gate", "attack_success",
    "hit_struck_damage", "hit_struck_damage2", "struck_durability", "struck_durability_slot",
    "attack_skill_point_count",
}

SIDES = {"csharp": CSHARP_ATTACK_RE, "rust": RUST_ATTACK_RE}


def norm(site):
    return (site or "").replace("\\", "/")


def canonical(site):
    s = norm(site)
    for rx, name in CANON_RULES:
        if rx.search(s):
            return name
    return s


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
            site = parts[4] if len(parts) >= 5 else ""
            rows.append((seq, parts[1], parts[2], parts[3], site))
    return rows


def is_attack(site, side):
    rx = SIDES[side]
    return bool(rx.search(norm(site)))


def cmd_summary(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    side = args.side
    attack = sum(1 for r in rows if is_attack(r[4], side))
    hist = collections.Counter(norm(r[4]) or "(no-site)" for r in rows)
    canon = collections.Counter(canonical(r[4]) for r in rows)
    print("RNG_LEDGER %s side=%s total=%d attack_path=%d noise=%d noise_ratio=%.4f distinct_sites=%d"
          % (args.trace, side, len(rows), attack, len(rows) - attack,
             (len(rows) - attack) / len(rows), len(hist)))
    print("  CANONICAL %s" % dict(sorted(canon.items(), key=lambda kv: -kv[1])))
    for site, n in hist.most_common(args.top):
        print("  SITE %8d  %s" % (n, site))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"trace": args.trace, "side": side, "total": len(rows), "attack_path": attack,
                       "noise": len(rows) - attack, "sites": dict(hist), "canonical": dict(canon)},
                      fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def cmd_order(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    seq = []
    for _seq, _kind, _arg, _res, site in rows:
        if args.attack_only and not is_attack(site, args.side):
            continue
        seq.append(canonical(site))
    out = args.out or (os.path.splitext(args.trace)[0] + (".attack.order.txt" if args.attack_only else ".order.txt"))
    with open(out, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(seq) + ("\n" if seq else ""))
    print("RNG_ORDER %s side=%s attack_only=%s draws=%d -> %s" % (args.trace, args.side, args.attack_only, len(seq), out))
    return 0


def _segments(path, side):
    """把攻击路径按 marker（`atk_power_a`，每次攻击恰一次）逐击分段。

    返回 `[(seg_idx, (canonical, ...)), ...]`；marker 之前的预热/怪物初始化取数不计入任何段。
    """
    rows = parse_trace(path)
    segs = []
    cur = None
    for _seq, _kind, _arg, _res, site in rows:
        if not is_attack(site, side):
            continue
        c = canonical(site)
        if c == "atk_power_a":
            if cur is not None:
                segs.append((len(segs) + 1, tuple(cur)))
            cur = [c]
        elif cur is not None:
            cur.append(c)
    if cur is not None:
        segs.append((len(segs) + 1, tuple(cur)))
    return segs


def cmd_segments(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    segs = _segments(args.trace, args.side)
    print("RNG_SEGMENTS %s side=%s segments=%d" % (args.trace, args.side, len(segs)))
    shapes = collections.Counter(s for _k, s in segs)
    for shape, n in shapes.most_common():
        print("  SHAPE x%-4d %s" % (n, list(shape)))
    for k, s in segs[: args.head]:
        print("  SEG k=%d %s" % (k, list(s)))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"trace": args.trace, "side": args.side, "segments": len(segs),
                       "shapes": {"|".join(k): v for k, v in shapes.items()},
                       "per_seg": [{"k": k, "shape": list(s)} for k, s in segs]},
                      fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def cmd_segdiff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到 %s" % p)
    sa = _segments(args.a, args.a_side)
    sb = _segments(args.b, args.b_side)
    if not sa or not sb:
        print("RED: 空段账本（a=%d b=%d）" % (len(sa), len(sb)))
        return 1
    shapes_a = collections.Counter(s for _k, s in sa)
    shapes_b = collections.Counter(s for _k, s in sb)
    only_a = {k: v for k, v in shapes_a.items() if k not in shapes_b}
    only_b = {k: v for k, v in shapes_b.items() if k not in shapes_a}
    first = None
    for i in range(min(len(sa), len(sb))):
        if sa[i][1] != sb[i][1]:
            first = i
            break
    print("RNG_SEGDIFF a=%s(%d segments) b=%s(%d segments)" % (args.a, len(sa), args.b, len(sb)))
    print("  A_SHAPES %s" % {_shape_name(k): v for k, v in shapes_a.most_common()})
    print("  B_SHAPES %s" % {_shape_name(k): v for k, v in shapes_b.most_common()})
    print("  SHAPES_ONLY_IN_A %s" % {_shape_name(k): v for k, v in only_a.items()})
    print("  SHAPES_ONLY_IN_B %s" % {_shape_name(k): v for k, v in only_b.items()})
    if first is None:
        print("  FIRST_SEG_DIVERGENCE none（段形状逐位全等至 min=%d）" % min(len(sa), len(sb)))
        verdict = "SEG_SHAPES_IDENTICAL_PREFIX" if len(sa) != len(sb) else "SEG_IDENTICAL"
    else:
        print("  FIRST_SEG_DIVERGENCE seg=%d  a=%s  b=%s"
              % (first + 1, list(sa[first][1]), list(sb[first][1])))
        verdict = "SEG_SHAPE_DIVERGENCE"
    print("  SEG_COUNT a=%d b=%d verdict=%s" % (len(sa), len(sb), verdict))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({
                "a": args.a, "b": args.b, "a_segments": len(sa), "b_segments": len(sb),
                "first_seg_divergence": first,
                "a_shapes": {"|".join(k): v for k, v in shapes_a.items()},
                "b_shapes": {"|".join(k): v for k, v in shapes_b.items()},
                "shapes_only_in_a": {"|".join(k): v for k, v in only_a.items()},
                "shapes_only_in_b": {"|".join(k): v for k, v in only_b.items()},
                "verdict": verdict,
            }, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def _shape_name(shape):
    return ">".join(shape)


def _load_seq(path, side, attack_only):
    rows = parse_trace(path)
    seq = []
    for _seq, _kind, _arg, _res, site in rows:
        if attack_only and not is_attack(site, side):
            continue
        seq.append((canonical(site), norm(site)))
    return seq


def cmd_diff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到 %s" % p)
    a = _load_seq(args.a, args.a_side, args.attack_only)
    b = _load_seq(args.b, args.b_side, args.attack_only)
    if not a or not b:
        print("RED: 空账本（a=%d b=%d）" % (len(a), len(b)))
        return 1
    ca = collections.Counter(x[0] for x in a)
    cb = collections.Counter(x[0] for x in b)
    only_a = {k: v for k, v in ca.items() if k not in cb}
    only_b = {k: v for k, v in cb.items() if k not in ca}
    # 顺序敏感：逐位比，找第一处分歧。
    first = None
    for i in range(min(len(a), len(b))):
        if a[i][0] != b[i][0]:
            first = i
            break
    print("RNG_LEDGER_DIFF attack_only=%s a=%s(%d) b=%s(%d)"
          % (args.attack_only, args.a, len(a), args.b, len(b)))
    print("  A_CANONICAL %s" % dict(sorted(ca.items(), key=lambda kv: -kv[1])))
    print("  B_CANONICAL %s" % dict(sorted(cb.items(), key=lambda kv: -kv[1])))
    print("  ONLY_IN_A(missing_in_B) %s" % dict(sorted(only_a.items(), key=lambda kv: -kv[1])))
    print("  ONLY_IN_B(extra_in_B)   %s" % dict(sorted(only_b.items(), key=lambda kv: -kv[1])))
    if first is None:
        if len(a) == len(b):
            print("  FIRST_DIVERGENCE none（逐位全等，len=%d）" % len(a))
            verdict = "IDENTICAL"
        else:
            print("  FIRST_DIVERGENCE 前缀全等；a 长 %d、b 长 %d（b 在 index %d 之后缺/多）"
                  % (len(a), len(b), min(len(a), len(b))))
            verdict = "PREFIX_EQUAL_LENGTH_DIFF"
    else:
        print("  FIRST_DIVERGENCE index=%d  a=%s  b=%s" % (first, a[first][1], b[first][1]))
        print("    context a[%d:%d]=%s" % (max(0, first - 2), first + 2,
                                          [x[0] for x in a[max(0, first - 2):first + 2]]))
        print("    context b[%d:%d]=%s" % (max(0, first - 2), first + 2,
                                          [x[0] for x in b[max(0, first - 2):first + 2]]))
        verdict = "ORDER_DIVERGENCE"
    print("  VERDICT %s" % verdict)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({
                "attack_only": bool(args.attack_only), "a": args.a, "b": args.b,
                "a_len": len(a), "b_len": len(b), "first_divergence": first,
                "a_canonical": dict(ca), "b_canonical": dict(cb),
                "only_in_a": only_a, "only_in_b": only_b, "verdict": verdict,
            }, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def _write_trace(path, sites):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        for i, site in enumerate(sites, 1):
            fh.write("%d\tNext(max)\t10\t%d\t%s\n" % (i, i % 10, site))


def cmd_selftest(args):
    import contextlib
    import io
    import tempfile
    tmp = tempfile.mkdtemp(prefix="rng-ledger-diff-selftest-")
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

    # C# 侧 3 次攻击，每次 [a,b,hit_gate]；Rust 侧同序。
    C = ["M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929",
         "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934",
         "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"]
    R = [r"crates\world\src\combat.rs:109", r"crates\world\src\combat.rs:111",
         r"crates\world\src\combat.rs:736"]
    sites_a = C * 3
    sites_b = R * 3
    ta = os.path.join(tmp, "a.tsv")
    tb = os.path.join(tmp, "b.tsv")
    _write_trace(ta, sites_a)
    _write_trace(tb, sites_b)
    rc, out = cap(lambda: cmd_diff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", attack_only=True, json="")))
    check("identical-green", rc == 0 and "IDENTICAL" in out, out[:300])

    # 次序漂移：b 在 index 1 处插入一条 hit_gate（取数次序被推移）⇒ 第一处分歧 index=1。
    _write_trace(tb, R[:1] + [R[2]] + R[1:] + R * 2)
    rc, out = cap(lambda: cmd_diff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", attack_only=True, json="")))
    check("order-shift-red", rc == 0 and "FIRST_DIVERGENCE index=1" in out, out[:400])

    # 缺消费者：b 少一个 struck_durability ⇒ only_in_a 点名 + 第一处分歧。
    _write_trace(tb, R * 3)
    _write_trace(ta, C[:2] + ["M2Server.Player.PlayObject.StruckDamage@PlayObject.cs:2957"] + C[2:] * 3)
    rc, out = cap(lambda: cmd_diff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", attack_only=True, json="")))
    check("missing-consumer-red", rc == 0 and "struck_durability" in out and "FIRST_DIVERGENCE" in out, out[:400])

    # 逐段：a 段形状 [a,b,hit_gate]，b 段形状 [a,b,hit_gate,hit_struck_damage] ⇒ 第一段即分歧。
    _write_trace(ta, C * 2)
    _write_trace(tb, (R + [r"crates\world\src\combat.rs:606"]) * 2)
    rc, out = cap(lambda: cmd_segdiff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", json="")))
    check("seg-shape-red", rc == 0 and "FIRST_SEG_DIVERGENCE seg=1" in out and "SEG_SHAPE_DIVERGENCE" in out, out[:500])
    check("seg-count", "SEG_COUNT a=2 b=2" in out, out[:500])

    for name in os.listdir(tmp):
        try:
            os.remove(os.path.join(tmp, name))
        except OSError:
            pass
    os.rmdir(tmp)
    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: rng_ledger_diff selftest %d/%d（同序绿 + 次序漂移必红且定位 + 缺消费者必红且点名）"
          % (len(checks), len(checks)))
    return 0


def _die(msg, rc=2):
    print(msg, file=sys.stderr)
    return rc


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 two-sided RNG ledger site+order diff")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("summary")
    p.add_argument("trace")
    p.add_argument("--side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--top", type=int, default=15)
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_summary)

    p = sub.add_parser("order")
    p.add_argument("trace")
    p.add_argument("--side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--attack-only", action="store_true")
    p.add_argument("--out", default="")
    p.set_defaults(func=cmd_order)

    p = sub.add_parser("diff")
    p.add_argument("a")
    p.add_argument("b")
    p.add_argument("--a-side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--b-side", choices=["csharp", "rust"], default="rust")
    p.add_argument("--attack-only", action="store_true")
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_diff)

    p = sub.add_parser("segments")
    p.add_argument("trace")
    p.add_argument("--side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--head", type=int, default=3)
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_segments)

    p = sub.add_parser("segdiff")
    p.add_argument("a")
    p.add_argument("b")
    p.add_argument("--a-side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--b-side", choices=["csharp", "rust"], default="rust")
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_segdiff)

    p = sub.add_parser("selftest")
    p.set_defaults(func=cmd_selftest)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
