#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""check_predicates.py — M3②③ 活栈隔离口径的**分流谓词机判**（owner 2026-10-12 21:15）。

两侧（C# 影子 hook / Rust `crates/world`）把攻击路径取数分流到各自专用种子流。本脚本按
唯一事实源 `attack_sites.tsv` 做机判，**缺一条即 fail-closed**：

  static          校验表内每个 `divert=1` 的 canon 两侧（csharp/rust）都有行。
  table-vs-csharp 校验 C# hook 实测记录的分流 site 列表**覆盖表内每个 canon**（漏一个 ⇒ RED），
                  且实测 site 全部能在表里查到（有未登记 site ⇒ RED）。
  align           两侧实测 site 列表 → canonical 序列，**顺序敏感**逐位比；报第一处分歧。
                  `--set-only` 只比集合（顺序不敏感；用于“站点齐不齐”而非“次序”）。

数据格式：
  * 表：`attack_sites.tsv`（制表符分隔；`#` 起注释；列见文件头）。
  * 实测 site 列表文件：**每行一个 site 字符串**（C# = hook 记录的 `Type.Method@file:line`；
    Rust = `MIR2_RNG_LOG` 里攻击路径行的第 5 列，如 `crates\world\src\combat.rs:109`）。
    空行/`#`/`SEED_PROBE`/`INJECT_OK`/`ISOLATE_ATTACK` 行忽略。

退出码：0 = GREEN；1 = RED（缺站点/未登记/次序分歧）；2 = PREREQ（表/文件缺失或解析为空）。
"""
import argparse
import os
import re
import sys
import tempfile

DEFAULT_TABLE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "attack_sites.tsv")
SIDES = ("csharp", "rust")


def parse_table(path):
    if not os.path.exists(path):
        raise FileNotFoundError(path)
    rows = []
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.rstrip("\r\n")
            if not line or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 6:
                continue
            canon, side, pattern, draw, oracle_ref, divert = parts[:6]
            note = parts[6] if len(parts) > 6 else ""
            if side not in SIDES:
                continue
            rows.append({
                "canon": canon, "side": side, "pattern": pattern, "draw": draw,
                "oracle_ref": oracle_ref, "divert": divert == "1", "note": note,
            })
    return rows


def norm(site):
    return (site or "").replace("\\", "/")


def read_sites(path):
    if not os.path.exists(path):
        raise FileNotFoundError(path)
    out = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\r\n")
            if not line or line.lstrip().startswith("#"):
                continue
            if line.startswith("SEED_PROBE") or "INJECT_OK" in line or line.startswith("ISOLATE_ATTACK"):
                continue
            # Rust trace 行是 TSV：取第 5 列；C# dump 行是单列。
            site = line.split("\t")[4] if line.count("\t") >= 4 else line
            if site:
                out.append(site.strip())
    return out


def canon_of(site, side, rows):
    s = norm(site)
    for r in rows:
        if r["side"] != side or not r["divert"]:
            continue
        if re.search(r["pattern"], s):
            return r["canon"]
    return None


def map_side(sites, side, rows):
    """返回 (canon_list, unknown_list)。"""
    canons, unknown = [], []
    for s in sites:
        c = canon_of(s, side, rows)
        if c is None:
            unknown.append(s)
        else:
            canons.append(c)
    return canons, unknown


def table_canons(rows, side):
    return {r["canon"] for r in rows if r["divert"] and r["side"] == side}


def cmd_static(args):
    rows = parse_table(args.table)
    if not rows:
        return _die("PREREQ_MISSING: 表解析为空 %s" % args.table)
    cs = table_canons(rows, "csharp")
    rs = table_canons(rows, "rust")
    missing_c = sorted(rs - cs)
    missing_r = sorted(cs - rs)
    print("STATIC table=%s csharp_canons=%d rust_canons=%d" % (args.table, len(cs), len(rs)))
    for c in sorted(cs):
        print("  CANON %s" % c)
    if missing_c or missing_r:
        print("RED: canon 未两侧齐备 只在 rust=%s 只在 csharp=%s" % (missing_c, missing_r))
        return 1
    print("GREEN: 表内每个 divert canon 两侧齐备（csharp/rust）")
    return 0


def cmd_table_vs_csharp(args):
    rows = parse_table(args.table)
    if not rows:
        return _die("PREREQ_MISSING: 表解析为空 %s" % args.table)
    if not args.csharp:
        return _die("PREREQ_MISSING: table-vs-csharp 需要 --csharp <sites.txt>")
    sites = read_sites(args.csharp)
    if not sites:
        return _die("PREREQ_MISSING: %s 解析为空" % args.csharp)
    canons, unknown = map_side(sites, "csharp", rows)
    seen = set(canons)
    expected = table_canons(rows, "csharp")
    missing = sorted(expected - seen)
    rc = 0
    print("TABLE_VS_CSHARP table=%s csharp=%s draws=%d canon=%d" % (args.table, args.csharp, len(sites), len(seen)))
    if unknown:
        print("RED: C# 分流了未登记的 site（表里查不到 ⇒ fail-closed）:")
        for u in sorted(set(unknown)):
            print("  UNKNOWN %s" % u)
        rc = 1
    if missing:
        print("RED: C# 未分流表内站点（缺一条 ⇒ fail-closed）: %s" % missing)
        rc = 1
    if rc == 0:
        print("GREEN: C# 分流站点覆盖表内全部 canon 且无未登记站点")
    return rc


def cmd_align(args):
    rows = parse_table(args.table)
    if not rows:
        return _die("PREREQ_MISSING: 表解析为空 %s" % args.table)
    if not (args.csharp and args.rust):
        return _die("PREREQ_MISSING: align 需要 --csharp 与 --rust")
    cs_sites = read_sites(args.csharp)
    rs_sites = read_sites(args.rust)
    if not cs_sites or not rs_sites:
        return _die("PREREQ_MISSING: 一侧 site 列表为空")
    # align 用于**已隔离**的活栈：只保留表内攻击路径站点（噪声消费者不计入）。
    cs_canons = [c for c in (canon_of(s, "csharp", rows) for s in cs_sites) if c is not None]
    rs_canons = [c for c in (canon_of(s, "rust", rows) for s in rs_sites) if c is not None]
    rc = 0
    if not cs_canons or not rs_canons:
        print("RED: 一侧在表内无任何攻击路径站点（隔离没生效 / 表与 trace 不符）")
        return 1
    if args.set_only:
        diff = sorted(set(cs_canons) ^ set(rs_canons))
        print("ALIGN(set) csharp=%d rust=%d symmetric_diff=%d" % (len(set(cs_canons)), len(set(rs_canons)), len(diff)))
        if diff:
            print("RED: canon 集合不同: %s" % diff)
            rc = 1
    else:
        n = min(len(cs_canons), len(rs_canons))
        first = None
        for i in range(n):
            if cs_canons[i] != rs_canons[i]:
                first = i
                break
        if first is None and len(cs_canons) != len(rs_canons):
            first = n
        print("ALIGN(order) csharp=%d rust=%d first_divergence=%s"
              % (len(cs_canons), len(rs_canons), "none" if first is None else first))
        if first is not None:
            cs_v = cs_canons[first] if first < len(cs_canons) else "(end)"
            rs_v = rs_canons[first] if first < len(rs_canons) else "(end)"
            print("RED: 第一处分歧 @%d csharp=%s rust=%s" % (first + 1, cs_v, rs_v))
            rc = 1
    if rc == 0:
        print("GREEN: 两侧分流站点对齐（%s）" % ("set" if args.set_only else "order"))
        return 0
    return 1


def _write(path, lines):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")


def cmd_selftest(args):
    tmp = tempfile.mkdtemp(prefix="check_predicates_")
    table = os.path.join(tmp, "table.tsv")
    _write(table, [
        "\t".join(["canon", "side", "pattern", "draw", "oracle_ref", "divert"]),
        "\t".join(["a_a", "csharp", r"Foo@Foo\.cs:10$", "Random(5)", "Foo.cs:10", "1"]),
        "\t".join(["a_a", "rust", r"combat\.rs:10$", "random_below(5)", "Foo.cs:10", "1"]),
        "\t".join(["b_b", "csharp", r"Foo@Foo\.cs:20$", "Random(7)", "Foo.cs:20", "1"]),
        "\t".join(["b_b", "rust", r"combat\.rs:20$", "random_below(7)", "Foo.cs:20", "1"]),
    ])
    cs_good = os.path.join(tmp, "cs_good.txt")
    rs_good = os.path.join(tmp, "rs_good.txt")
    cs_bad = os.path.join(tmp, "cs_bad.txt")
    rs_bad = os.path.join(tmp, "rs_bad.txt")
    cs_unknown = os.path.join(tmp, "cs_unknown.txt")
    _write(cs_good, ["Foo@Foo.cs:10", "Foo@Foo.cs:20"])
    _write(rs_good, [r"crates\world\src\combat.rs:10", r"crates\world\src\combat.rs:20"])
    _write(cs_bad, ["Foo@Foo.cs:20"])                     # 缺 a_a
    _write(rs_bad, [r"crates\world\src\combat.rs:20", r"crates\world\src\combat.rs:10"])  # 次序错
    _write(cs_unknown, ["Bar@Bar.cs:99"])                 # 未登记

    checks = [
        ("static", cmd_static, dict(table=table), 0),
        ("align-good", cmd_align, dict(table=table, csharp=cs_good, rust=rs_good, set_only=False), 0),
        ("table-vs-csharp-good", cmd_table_vs_csharp, dict(table=table, csharp=cs_good), 0),
        ("table-vs-csharp-missing", cmd_table_vs_csharp, dict(table=table, csharp=cs_bad), 1),
        ("table-vs-csharp-unknown", cmd_table_vs_csharp, dict(table=table, csharp=cs_unknown), 1),
        ("align-order-mismatch", cmd_align, dict(table=table, csharp=cs_good, rust=rs_bad, set_only=False), 1),
        ("align-set-ok", cmd_align, dict(table=table, csharp=cs_good, rust=rs_bad, set_only=True), 0),
    ]
    failed = 0
    for name, fn, kw, expect in checks:
        print("--- selftest %s (expect rc=%d) ---" % (name, expect))
        rc = fn(argparse.Namespace(**kw))
        if rc != expect:
            print("SELFTEST FAIL %s: rc=%d expect=%d" % (name, rc, expect))
            failed += 1
    print("SELFTEST total=%d failed=%d" % (len(checks), failed))
    return 1 if failed else 0


def _die(msg):
    print(msg)
    return 2


def main(argv=None):
    p = argparse.ArgumentParser(description="M3②③ 攻击路径分流谓词机判")
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("static", "table-vs-csharp", "align", "selftest"):
        sp = sub.add_parser(name)
        sp.add_argument("--table", default=DEFAULT_TABLE)
        sp.add_argument("--csharp")
        sp.add_argument("--rust")
        sp.add_argument("--set-only", action="store_true")
    args = p.parse_args(argv)
    fn = {"static": cmd_static, "table-vs-csharp": cmd_table_vs_csharp,
          "align": cmd_align, "selftest": cmd_selftest}[args.cmd]
    return fn(args)


if __name__ == "__main__":
    sys.exit(main())
