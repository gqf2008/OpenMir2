#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""stream_equiv.py — 两侧「攻击路径专用流」**逐次取数语义等价**判据（m3cs，可复用件）。

为什么单独做这个（口径）：
    `per_strike_requests.py analyze` 的 first_divergence 按 `(kind, arg)` 比（`Entry.req()`），
    而 Rust 侧 `random_range(0, n)` 记录的 kind 是 `Next(min,max)`（packed arg = min<<32 | max，
    min==0），C# 侧对应 `Random(n)` 记录的是 `Next(max)`。两者**语义相同**（都是 [0,n) 的均匀取数），
    实测 651/651 次 `(canon, arg, result)` 全等，但 `(kind,arg)` 在 index=3 起被判「分歧」——
    这是**仪器口径假阳性**（不是实现差异）。本脚本按语义等价的 kind 归一后再比，把
    「流是否逐次等价」做成一条**独立、可复用、fail-closed** 的机判。

口径（唯一真源见 `../rng-isolate-attack/attack_sites.tsv` 与 `README.md`）：
    * 行选取：C# 专用流 = `kind \\t arg \\t result \\t site`（每行都是分流出来的攻击站点）；
      Rust 全局 trace = `seq \\t kind \\t arg \\t result \\t site`，只保留站点表命中的行。
    * 归一：`Next(min,max)` 的无符号 packed arg 若高 32 位（min）== 0 ⇒ 视为 `Next(max)`，arg=max；
      否则保持 `Next(min,max)`（真 min>0 ⇒ 与 `Next(max)` 不是同一形态，继续判差）。
    * 判据：两侧 `(canon, 归一 kind, arg, result)` 逐 index 全等 ⇒ GREEN（rc=0）。
    * fail-closed（rc=2，绝不静默出绿）：
        1. 任一输入缺失 / 空 / 解析后 0 抽；
        2. C# 专用流里出现**站点表无法归一**的行（站点表陈旧 ⇒ 点名）；
        3. 两侧 trace 行数不一致（一侧根本没跑或没落盘）。
      注意：**两侧抽数不相等**（例如 651 vs 650）是判据 **RED（rc=1）**，不是 PREREQ。

用法：
    python -I stream_equiv.py gate --csharp <csharp-attack-stream.tsv> --rust <rust-rng-stream.tsv> \\
        [--json out.json] [--min-draws N]
    python -I stream_equiv.py selftest

退出码：0 = GREEN / SELFTEST GREEN；1 = 判据 RED；2 = PREREQ（缺件/空件/站点表不命中）。
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True  # 不许在证据目录里落 __pycache__（会被 --check-extra 判多件）

HERE = Path(__file__).resolve().parent
# 本工具的可复用位置是 `attack-request-sequence/`（与 per_strike_requests.py 同目录）；
# 兼容曾放在证据包里的历史副本（同级目录的 per_strike_requests.py）。
_PSR_CANDIDATES = [HERE / "per_strike_requests.py", HERE.parent / "attack-request-sequence" / "per_strike_requests.py"]


class PrereqError(Exception):
    """fail-closed：缺件 / 空件 / 站点表不命中 —— 绝不降级为 GREEN。"""


def _load_psr():
    for cand in _PSR_CANDIDATES:
        if cand.exists():
            spec = importlib.util.spec_from_file_location("per_strike_requests", str(cand))
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            return mod
    raise PrereqError("找不到 per_strike_requests.py（尝试：%s）" % ", ".join(str(p) for p in _PSR_CANDIDATES))


def norm_kind(kind, arg):
    """`Next(min,max)` 且 packed 高 32 位（min）==0 ⇒ 归一为 `Next(max)`（语义等价）。"""
    if kind == "Next(min,max)":
        unsigned = arg & 0xFFFFFFFFFFFFFFFF
        lo = unsigned & 0xFFFFFFFF
        hi = (unsigned >> 32) & 0xFFFFFFFF
        if hi == 0:
            return ("Next(max)", lo)
    return (kind, arg)


def _iter_side(side, path, mod, smap):
    """产出 (entries, data_lines, unmapped_sites)。

    data_lines = 该侧**看起来像数据行**的行数（用于 PREREQ「空件」判定）；
    unmapped_sites = 数据行里站点表命中失败的名字（仅 C# 专用流要求为空 ⇒ fail-closed）。
    """
    entries = []
    data_lines = 0
    unmapped = []
    text = path.read_text(encoding="utf-8-sig")
    for raw in text.splitlines():
        p = raw.split("\t")
        if side == "csharp":
            # C# 专用流：kind arg result site（无 seq；site 不以数字开头）
            if len(p) < 4 or not p[3] or p[3][0].isdigit():
                continue
            data_lines += 1
            canon = mod.canon_of("csharp", p[3], smap)
            if canon is None:
                unmapped.append(p[3])
                continue
            entries.append(mod.Entry(p[0], p[1], p[2], canon))
        else:
            # Rust 全局 trace：seq kind arg result site；只保留站点表命中的行（其余是噪声）
            if len(p) < 5 or not p[0].isdigit() or not p[1].startswith("Next"):
                continue
            data_lines += 1
            canon = mod.canon_of("rust", p[4], smap)
            if canon is None:
                continue
            entries.append(mod.Entry(p[1], p[2], p[3], canon))
    return entries, data_lines, unmapped


def _parse(side, path, mod, smap):
    if not path.exists():
        raise PrereqError("PREREQ_MISSING：%s 侧的输入不存在：%s" % (side, path))
    if path.stat().st_size == 0:
        raise PrereqError("PREREQ_EMPTY：%s 侧的输入为空文件：%s" % (side, path))
    entries, data_lines, unmapped = _iter_side(side, path, mod, smap)
    if data_lines == 0:
        raise PrereqError("PREREQ_EMPTY：%s 侧无任何数据行（空/spawn 未落盘）：%s" % (side, path))
    if len(entries) == 0:
        raise PrereqError("PREREQ_UNMAPPED：%s 侧 %d 个数据行无一命中站点表（站点表陈旧？）：%s"
                          % (side, data_lines, path))
    if side == "csharp" and unmapped:
        raise PrereqError("PREREQ_UNMAPPED：C# 专用流有 %d 行站点表无法归一，例如 %r（站点表陈旧 ⇒ 拒绝出绿）"
                          % (len(unmapped), unmapped[0]))
    return entries, data_lines, unmapped


def compare(csharp_path, rust_path, min_draws=1):
    mod = _load_psr()
    smap = mod.load_site_map()
    cs, cs_lines, cs_unmapped = _parse("csharp", Path(csharp_path), mod, smap)
    rt, rt_lines, rt_unmapped = _parse("rust", Path(rust_path), mod, smap)
    if len(cs) < min_draws or len(rt) < min_draws:
        raise PrereqError("PREREQ_TOO_SHORT：抽数 csharp=%d rust=%d < --min-draws=%d"
                          % (len(cs), len(rt), min_draws))

    a = [(e.canon,) + norm_kind(e.kind, e.arg) + (e.result,) for e in cs]
    b = [(e.canon,) + norm_kind(e.kind, e.arg) + (e.result,) for e in rt]
    eq = a == b

    first = None
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            first = {"index_0based": i, "csharp": a[i], "rust": b[i]}
            break
    if first is None and len(a) != len(b):
        first = {"index_0based": min(len(a), len(b)),
                 "csharp": (a[len(b)] if len(a) > len(b) else None),
                 "rust": (b[len(a)] if len(b) > len(a) else None)}

    # 仪器假阳性的证据：`(kind,arg)` 口径下的第一处分歧 + 两侧 kind 标签集合
    raw_first = None
    for i in range(min(len(cs), len(rt))):
        if cs[i].req() != rt[i].req():
            raw_first = {"index_0based": i, "csharp_req": list(cs[i].req()),
                         "rust_req": list(rt[i].req()), "csharp": repr(cs[i]), "rust": repr(rt[i])}
            break

    return {
        "csharp_draws": len(cs), "rust_draws": len(rt),
        "csharp_data_lines": cs_lines, "rust_data_lines": rt_lines,
        "csharp_unmapped": cs_unmapped,
        "equivalent": eq,
        "first_semantic_diff": first,
        "raw_kind_arg_first_diff": raw_first,
        "csharp_kinds": sorted({e.kind for e in cs}),
        "rust_kinds": sorted({e.kind for e in rt}),
    }


def cmd_gate(args):
    try:
        res = compare(args.csharp, args.rust, min_draws=args.min_draws)
    except PrereqError as exc:
        print("PREREQ %s" % exc)
        return 2
    print("=== 两侧专用流逐次取数（语义等价口径）===")
    print("  csharp draws=%d rust draws=%d" % (res["csharp_draws"], res["rust_draws"]))
    print("  csharp kinds=%s" % ",".join(res["csharp_kinds"]))
    print("  rust   kinds=%s" % ",".join(res["rust_kinds"]))
    if res["raw_kind_arg_first_diff"]:
        rf = res["raw_kind_arg_first_diff"]
        print("  [仪器] `(kind,arg)` 口径第一处分歧 index=%d：csharp req=%s / rust req=%s —— 两侧 repr 均为 %s"
              % (rf["index_0based"], rf["csharp_req"], rf["rust_req"], rf["csharp"]))
    if res["equivalent"]:
        print("  GREEN: 两侧 (canon,归一 kind,arg,result) 逐次全等（%d/%d）——攻击路径取数等价"
              % (res["csharp_draws"], res["csharp_draws"]))
        rc = 0
    else:
        print("  RED: 两侧逐次取数不等；第一处 index=%s %s"
              % (res["first_semantic_diff"]["index_0based"] if res["first_semantic_diff"] else "?",
                 json.dumps(res["first_semantic_diff"], ensure_ascii=False)))
        rc = 1
    if args.json:
        Path(args.json).write_text(json.dumps(res, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        print("  [json] %s" % args.json)
    return rc


def cmd_selftest(_args):
    import tempfile
    mod = _load_psr()
    smap = mod.load_site_map()
    rc = 0
    n_ok = 0
    n_tot = 0

    def check(name, cond, detail=""):
        nonlocal rc, n_ok, n_tot
        n_tot += 1
        if cond:
            n_ok += 1
        else:
            rc = 1
        print("[selftest] %s: %s%s" % (name, "ok" if cond else "FAIL", (" — " + detail) if detail else ""))

    # 合成一段真实形状的 C# 专用流 / Rust 全局 trace（player 单元模板 + 命中）
    cs_lines = [
        "Next(max)\t2\t0\tM2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929",
        "Next(max)\t10\t4\tM2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934",
        "Next(max)\t15\t3\tM2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41",
        "Next(max)\t37\t22\tM2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:792",
        "Next(max)\t22\t15\tM2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:793",
        "Next(max)\t10\t7\tM2Server.Player.PlayObject.StruckDamage@PlayObject.cs:2957",
        "Next(max)\t5\t2\tM2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223",
    ]
    rt_lines = [
        "1\tNext(min,max)\t2\t0\tcrates\\world\\src\\combat.rs:109",
        "2\tNext(min,max)\t10\t4\tcrates\\world\\src\\combat.rs:111",
        "3\tNext(min,max)\t15\t3\tcrates\\world\\src\\combat.rs:758",
        "4\tNext(min,max)\t37\t22\tcrates\\world\\src\\combat.rs:628",
        "5\tNext(min,max)\t22\t15\tcrates\\world\\src\\combat.rs:630",
        "6\tNext(max)\t10\t7\tcrates\\world\\src\\combat.rs:601",
        "7\tNext(min,max)\t5\t2\tcrates\\world\\src\\combat.rs:831",
    ]

    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        csp = tdp / "cs.tsv"
        rtp = tdp / "rt.tsv"

        def write_cs(lines):
            csp.write_text("\n".join(lines) + "\n", encoding="utf-8")

        def write_rt(lines):
            rtp.write_text("\n".join(lines) + "\n", encoding="utf-8")

        # 1) 语义等价：min==0 的 Next(min,max) 归一后全等；且 (kind,arg) 口径确有一处假分歧
        write_cs(cs_lines); write_rt(rt_lines)
        ok = compare(str(csp), str(rtp))
        check("min=0 归一后逐次全等（7/7）", ok["equivalent"] is True)
        check("(kind,arg) 口径确有假分歧（仪器口径的证据）", bool(ok["raw_kind_arg_first_diff"]))
        check("首处分歧是 hit_struck_damage 的 Next(max) vs Next(min,max)",
              ok["raw_kind_arg_first_diff"] and ok["raw_kind_arg_first_diff"]["csharp_req"][0] == "Next(max)"
              and ok["raw_kind_arg_first_diff"]["rust_req"][0] == "Next(min,max)")

        # 2) 改坏一抽结果 ⇒ 必红
        bad = list(rt_lines); bad[3] = bad[3].replace("\t22\t", "\t23\t", 1)
        write_rt(bad)
        nok = compare(str(csp), str(rtp))
        check("改坏一抽结果 ⇒ RED", nok["equivalent"] is False)

        # 3) 真 min>0：不归一、不抹平 ⇒ 必红
        write_rt(["1\tNext(min,max)\t%d\t22\tcrates\\world\\src\\combat.rs:628" % ((5 << 32) | 37)])
        write_cs(["Next(max)\t37\t22\tM2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:792"])
        gok = compare(str(csp), str(rtp))
        check("真 min=5 不抹平 ⇒ RED", gok["equivalent"] is False)

        # 4) 抽数不等 ⇒ RED（不是 PREREQ）
        write_cs(cs_lines)
        write_rt(rt_lines + ["8\tNext(min,max)\t5\t2\tcrates\\world\\src\\combat.rs:831"])
        cnt = compare(str(csp), str(rtp))
        check("抽数不等 ⇒ RED（rc=1 语义）", cnt["equivalent"] is False and cnt["first_semantic_diff"]["index_0based"] == 7)

        # 5) fail-closed：缺件
        try:
            compare(str(tdp / "nope.tsv"), str(rtp))
            check("缺 C# 输入 ⇒ PREREQ", False)
        except PrereqError:
            check("缺 C# 输入 ⇒ PREREQ", True)

        # 6) fail-closed：空件
        (tdp / "empty.tsv").write_text("", encoding="utf-8")
        try:
            compare(str(tdp / "empty.tsv"), str(rtp))
            check("空输入 ⇒ PREREQ", False)
        except PrereqError:
            check("空输入 ⇒ PREREQ", True)

        # 7) fail-closed：C# 专用流站点表不命中（陈旧站点表）
        write_cs(["Next(max)\t37\t22\tM2Server.Player.PlayObject.NotARealSite@PlayObject.cs:9999"])
        try:
            compare(str(csp), str(rtp))
            check("C# 站点表不命中 ⇒ PREREQ（不静默出绿）", False)
        except PrereqError:
            check("C# 站点表不命中 ⇒ PREREQ（不静默出绿）", True)

        # 8) fail-closed：--min-draws 不满足
        write_cs(cs_lines); write_rt(rt_lines)
        try:
            compare(str(csp), str(rtp), min_draws=999)
            check("--min-draws 不满足 ⇒ PREREQ", False)
        except PrereqError:
            check("--min-draws 不满足 ⇒ PREREQ", True)

    print("SELFTEST %s: stream_equiv %d/%d" % ("GREEN" if rc == 0 else "RED", n_ok, n_tot))
    return rc


def main(argv=None):
    ap = argparse.ArgumentParser(description="两侧攻击路径专用流逐次取数语义等价判据（m3cs，可复用件）")
    sub = ap.add_subparsers(dest="cmd")
    g = sub.add_parser("gate")
    g.add_argument("--csharp", required=True)
    g.add_argument("--rust", required=True)
    g.add_argument("--json", default="")
    g.add_argument("--min-draws", type=int, default=1, dest="min_draws")
    sub.add_parser("selftest")
    args = ap.parse_args(argv)
    if args.cmd == "gate":
        return cmd_gate(args)
    if args.cmd == "selftest":
        return cmd_selftest(args)
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
