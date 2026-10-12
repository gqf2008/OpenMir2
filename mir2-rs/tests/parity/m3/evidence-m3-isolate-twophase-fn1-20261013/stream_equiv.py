#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""stream_equiv.py — 两侧「攻击路径专用流」的**逐次取数等价**判据（m3cs 2026-10-13）。

为什么单独做这个：
    `per_strike_requests.py analyze` 的 `first_divergence` 按 `(kind, arg)` 比（`Entry.req()`），
    而 Rust 侧 `random_range(0, n)` 记录的 kind 是 `Next(min,max)`（packed arg = 0<<32|n），
    C# 侧对应 `Random(n)` 记录的是 `Next(max)`。两者**语义相同**（都是 [0,n) 的均匀取数），
    实测 651/651 次 (canon, arg, result) 全等，但 `(kind,arg)` 在 index=3 起被判「分歧」——
    这是**仪器假阳性**（不是实现差异）。本脚本按「语义等价」的 kind 归一后再比，
    把「流是否逐次等价」做成一条机判。

口径：
    * 归一：`Next(min,max)` 的 packed arg 若无符号高 32 位 == 0 ⇒ 视为 `Next(max)`，arg=max；
      否则保持 `Next(min,max)`（真的 min>0 ⇒ 与 Next(max) 不是同一形态，继续判差）。
    * 判据：两侧 `(canon, normalized_kind, arg, result)` 逐 index 全等 ⇒ EQUIVALENT（rc=0）。
    * 红检：把任一侧任一 result 改一个数 ⇒ DIFFERENT（rc=1）。

用法：
    python -I stream_equiv.py --csharp <csharp-attack-stream.tsv> --rust <rust-rng-stream.tsv> [--json out.json]
    python -I stream_equiv.py selftest
"""
import argparse
import importlib.util
import json
import os
import sys
from pathlib import Path

sys.dont_write_bytecode = True  # 不许在证据目录里落 __pycache__（会被 --check-extra 判多件）

HERE = Path(__file__).resolve().parent
PSR = HERE.parent / "attack-request-sequence" / "per_strike_requests.py"


def _load_psr():
    spec = importlib.util.spec_from_file_location("per_strike_requests", str(PSR))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def norm_kind(kind, arg):
    """`Next(min,max)` 且 packed 高 32 位（min）==0 ⇒ 归一为 `Next(max)`（语义等价）。"""
    if kind == "Next(min,max)":
        unsigned = arg & 0xFFFFFFFFFFFFFFFF
        lo = unsigned & 0xFFFFFFFF
        hi = (unsigned >> 32) & 0xFFFFFFFF
        if hi == 0:
            return ("Next(max)", lo)
    return (kind, arg)


def equiv_seq(mod, csharp_path, rust_path):
    smap = mod.load_site_map()
    cs = mod.parse_csharp(Path(csharp_path), smap)
    rt = mod.parse_rust(Path(rust_path), smap)
    a = [(e.canon,) + norm_kind(e.kind, e.arg) + (e.result,) for e in cs]
    b = [(e.canon,) + norm_kind(e.kind, e.arg) + (e.result,) for e in rt]
    return cs, rt, a, b


def compare(csharp_path, rust_path):
    mod = _load_psr()
    cs, rt, a, b = equiv_seq(mod, csharp_path, rust_path)
    eq = a == b
    first = None
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            first = {"index_0based": i, "csharp": a[i], "rust": b[i]}
            break
    if first is None and len(a) != len(b):
        first = {"index_0based": min(len(a), len(b)), "csharp": (a[len(b)] if len(a) > len(b) else None),
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
        "equivalent": eq,
        "first_semantic_diff": first,
        "raw_kind_arg_first_diff": raw_first,
        "csharp_kinds": sorted({e.kind for e in cs}),
        "rust_kinds": sorted({e.kind for e in rt}),
    }


def cmd_gate(args):
    res = compare(args.csharp, args.rust)
    print("=== 两侧专用流逐次取数（语义等价口径）===")
    print("  csharp draws=%d rust draws=%d" % (res["csharp_draws"], res["rust_draws"]))
    print("  csharp kinds=%s" % (",".join(res["csharp_kinds"])))
    print("  rust   kinds=%s" % (",".join(res["rust_kinds"])))
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
    # 合成：一条 player unit，C# 用 Next(max)，Rust 用 Next(min,max) packed(min=0)
    cs_lines = ["Next(max)\t37\t22\tM2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:792",
                "Next(max)\t22\t15\tM2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:793",
                "Next(max)\t10\t7\tM2Server.Player.PlayObject.StruckDamage@PlayObject.cs:2957"]
    rt_lines = ["1\tNext(min,max)\t37\t22\tcrates\\world\\src\\combat.rs:628",
                "2\tNext(min,max)\t22\t15\tcrates\\world\\src\\combat.rs:630",
                "3\tNext(max)\t10\t7\tcrates\\world\\src\\combat.rs:601"]
    rc_fail = 0
    with tempfile.TemporaryDirectory() as td:
        csp = Path(td) / "cs.tsv"; rtp = Path(td) / "rt.tsv"
        csp.write_text("\n".join(cs_lines) + "\n", encoding="utf-8")
        rtp.write_text("\n".join(rt_lines) + "\n", encoding="utf-8")
        ok = compare(str(csp), str(rtp))
        print("[selftest] 等价（Next(min,max) min=0 归一后相等）：", ok["equivalent"], "raw_first=", bool(ok["raw_kind_arg_first_diff"]))
        rc_fail |= 0 if ok["equivalent"] and ok["raw_kind_arg_first_diff"] else 1
        # 改坏：Rust 第 2 抽结果改一个数 ⇒ 必红
        bad = list(rt_lines); bad[1] = bad[1].replace("\t15\t", "\t16\t", 1)
        rtp.write_text("\n".join(bad) + "\n", encoding="utf-8")
        nok = compare(str(csp), str(rtp))
        print("[selftest] 改坏一抽结果 ⇒ equivalent=", nok["equivalent"], "（期望 False）")
        rc_fail |= 0 if not nok["equivalent"] else 1
        # 真 min>0：Rust 记 Next(min,max) packed(min=5,max=37) ⇒ 不归一 ⇒ 必红
        packed = (5 << 32) | 37
        rtp.write_text("1\tNext(min,max)\t%d\t22\tcrates\\world\\src\\combat.rs:628\n" % packed, encoding="utf-8")
        csp.write_text("Next(max)\t37\t22\tM2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:792\n", encoding="utf-8")
        gok = compare(str(csp), str(rtp))
        print("[selftest] min=5 真分歧不归一抹平 ⇒ equivalent=", gok["equivalent"], "（期望 False）")
        rc_fail |= 0 if not gok["equivalent"] else 1
    print("SELFTEST", "GREEN" if rc_fail == 0 else "RED")
    return rc_fail


def main(argv=None):
    ap = argparse.ArgumentParser(description="两侧攻击路径专用流逐次取数等价判据（m3cs）")
    sub = ap.add_subparsers(dest="cmd")
    g = sub.add_parser("gate")
    g.add_argument("--csharp", required=True)
    g.add_argument("--rust", required=True)
    g.add_argument("--json", default="")
    s = sub.add_parser("selftest")
    args = ap.parse_args(argv)
    if args.cmd == "gate":
        return cmd_gate(args)
    if args.cmd == "selftest":
        return cmd_selftest(args)
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
