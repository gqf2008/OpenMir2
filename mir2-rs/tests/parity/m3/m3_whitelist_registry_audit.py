#!/usr/bin/env python3
"""M3 白名单 T-* 登记号审计（`m3_whitelist_registry_audit.py`）——只读、零端口。

背景：`mir2-rs/tests/parity/whitelist.md` 是「行为差异登记」的**唯一真源**，M3 的每副夹具
（`fixture_matrix.MATRIX` 的 `t` 列）与 M3 总验收报告都用 `T-<n>` 号互相引用。该表的约定是
**全局唯一编号**（表内多处 `> 编号连号：T-41 → T-42 → …` 明确写了连号）。但历史上出现过
两处 `T-42` 同时存在（M3⑤ 商人 tick 墙钟预算 vs M3⑥ 任务夹具）⇒ 引用方无法判断
`T-42` 指向哪一行，`fixture_chain_gate.py --require-all` 的 `"| T-42 |"` 子串匹配也会同时命中两行。

本件把「登记号是否唯一、夹具 token 是否解析到恰一行」做成**一条命令 + 可复跑读数**，
避免继续靠肉眼。**它是审计（audit），不是 fail-closed 门**：默认只报告、`rc=0`；
`--strict` 才把 `DUPLICATE` / `TOKEN_MULTI` / （当前树 READY 的）`TOKEN_MISSING` 判 `rc=1`。

命令：
  python -I m3_whitelist_registry_audit.py audit [--strict] [--json OUT]
  python -I m3_whitelist_registry_audit.py selftest

口径：**只读**（只读 `whitelist.md` + 对 `fixture_matrix.MATRIX` 的 token 做解析核对，
不写仓库文件、不起栈、不占端口）。`selftest` 用合成文本 + 注入式 classify，含「改坏必红」。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_WHITELIST = os.path.normpath(os.path.join(HERE, "..", "whitelist.md"))

ROW_RE = re.compile(r"^\|\s*(T-\d+)\s*\|", re.MULTILINE)
NUM_RE = re.compile(r"^T-(\d+)$")


def parse_entries(text):
    """⇒ [{num:int, token:str, line:int, label:str}]（按出现顺序）。"""
    out = []
    for m in ROW_RE.finditer(text):
        token = m.group(1)
        num = int(NUM_RE.match(token).group(1))
        line = text.count("\n", 0, m.start()) + 1
        # label = 该行 | 后的第一段，截断，便于人读
        seg = text[m.end():text.find("\n", m.end()) if "\n" in text[m.end():] else len(text)]
        label = re.sub(r"\s+", " ", seg).strip(" |")[:72]
        out.append({"num": num, "token": token, "line": line, "label": label})
    return out


def audit_text(text, tokens, classify=None, strict=False, min_num=36):
    """纯函数（便于 selftest 注入）⇒ (entries, report, reds)。

    tokens   = [{"key","system","t"}]（夹具 token 表；`t` 可为 None）
    classify = 可选 k->status（"READY"/"PENDING"/...），用于 TOKEN_MISSING 是否判红
    min_num  = 只审 `T-<min_num>` 及以后的登记（默认 36 = M3 登记块起点；更早的 M1/M2/C6 历史段
               不属 M3 登记面，其历史重号不产生噪声）
    """
    entries = [e for e in parse_entries(text) if e["num"] >= min_num]
    by_num = {}
    for e in entries:
        by_num.setdefault(e["num"], []).append(e)

    findings = []
    reds = []
    for num, rows in sorted(by_num.items()):
        if len(rows) > 1:
            where = ",".join("L{0}".format(r["line"]) for r in rows)
            findings.append("DUPLICATE T-{0} × {1} @ {2}（全局唯一编号约定被破坏）".format(
                num, len(rows), where))
            if strict:
                reds.append("DUPLICATE T-{0} @ {1}".format(num, where))

    resolved = []
    for tk in tokens:
        t = tk.get("t")
        if not t:
            continue
        num = int(NUM_RE.match(t).group(1))
        rows = by_num.get(num, [])
        status = classify(tk["key"]) if classify else None
        resolved.append({
            "key": tk["key"], "system": tk["system"], "t": t,
            "rows": [r["line"] for r in rows], "fixture_status": status,
        })
        if len(rows) > 1:
            findings.append("TOKEN_MULTI {0} {1}：token {2} 解析到 {3} 行（L{4}）".format(
                tk["key"], tk["system"], t, len(rows),
                ",".join(str(r["line"]) for r in rows)))
            if strict:
                reds.append("TOKEN_MULTI {0}={1}".format(t, len(rows)))
        elif len(rows) == 0:
            hard = strict and status == "READY"
            findings.append("{0} {1} {2}：token {3} 在 whitelist 里不存在（{4}）".format(
                "MISSING" if hard else "MISSING(info)", tk["key"], tk["system"], t,
                "本树 READY ⇒ --strict 判红" if hard else "夹具未合并/未登记，登记为 info"))
            if hard:
                reds.append("TOKEN_MISSING {0} {1}（READY）".format(tk["key"], t))
    return entries, {"duplicates": sorted(n for n, r in by_num.items() if len(r) > 1),
                     "resolved": resolved, "findings": findings, "min_num": min_num}, reds


def _load_tokens():
    """夹具 token 表（单一真源 `fixture_matrix.MATRIX`）。

    `fixture_matrix.py` 若在本树不存在（其卡未合入）⇒ 返回 `(None, None)`，审计退化为
    「只查全局编号唯一性」，并在报告里登记 `tokens=unavailable`（不静默、不伪造）。
    """
    sys.path.insert(0, HERE)
    try:
        import fixture_matrix as fm  # 单一真源：夹具 token 表
    except ImportError:
        return None, None
    tokens = [{"key": e["key"], "system": e["system"], "t": e.get("t")} for e in fm.MATRIX]

    def classify(key):
        entry = next(e for e in fm.MATRIX if e["key"] == key)
        return fm.classify(HERE, entry)[0]

    return tokens, classify


def cmd_audit(args):
    with open(args.whitelist, "r", encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    tokens, classify = _load_tokens()
    entries, report, reds = audit_text(text, tokens or [], classify=classify, strict=args.strict,
                                       min_num=args.min_num)
    report["whitelist"] = args.whitelist
    report["entries"] = len(entries)
    report["strict"] = bool(args.strict)
    report["tokens"] = "unavailable" if tokens is None else len(tokens)

    print("WHITELIST_REGISTRY scope=T-{0}+ entries={1} duplicates={2} strict={3} tokens={4}".format(
        args.min_num, len(entries), report["duplicates"] or "none", args.strict, report["tokens"]))
    if tokens is None:
        print("  NOTE fixture_matrix.py 未在本树（其卡未合入）⇒ 只查全局编号唯一性")
    for tk in report["resolved"]:
        print("  token {0:6} <- {1} {2}  rows={3} fixture={4}".format(
            tk["t"], tk["key"], tk["system"], tk["rows"] or "[]", tk["fixture_status"]))
    for f in report["findings"]:
        print("  FINDING {0}".format(f))
    if reds:
        print("WHITELIST_REGISTRY_RED {0}：{1}".format(len(reds), "; ".join(reds)))
        rc = 1
    else:
        print("WHITELIST_REGISTRY GREEN（无重复编号；夹具 token 解析唯一或不可用）")
        rc = 0
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(report, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return rc


def cmd_selftest(_args):
    ok = 0
    total = 0

    def expect(name, cond):
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
            print("  SELFTEST ok  {0}".format(name))
        else:
            print("  SELFTEST FAIL {0}".format(name))

    good = (
        "| T-41 | a |\n| T-42 | b |\n| T-43 | c |\n"
    )
    dup = (
        "| T-41 | a |\n| T-42 | merchant |\n| T-42 | quest |\n| T-43 | c |\n"
    )
    tokens = [
        {"key": "5", "system": "shop", "t": "T-41"},
        {"key": "6", "system": "quest", "t": "T-42"},
        {"key": "9", "system": "hero", "t": "T-45"},
    ]

    e, r, reds = audit_text(good, tokens, classify=lambda k: "READY", strict=False)
    expect("唯一编号 -> 无 DUPLICATE", r["duplicates"] == [] and not reds)
    expect("token T-41 解析 1 行", next(x for x in r["resolved"] if x["t"] == "T-41")["rows"] == [1])
    expect("缺 token T-45 默认 info、不判红",
           any("MISSING(info)" in f or "T-45" in f for f in r["findings"]) and not reds)
    expect("行号解析（T-42 -> L2）",
           next(x for x in r["resolved"] if x["t"] == "T-42")["rows"] == [2])

    e2, r2, reds2 = audit_text(dup, tokens, classify=lambda k: "READY", strict=True)
    expect("重复 T-42 被点名", r2["duplicates"] == [42])
    expect("重复 token -> TOKEN_MULTI", any("TOKEN_MULTI 6" in f for f in r2["findings"]))
    expect("--strict 下重复必红（改坏必红）", any("DUPLICATE T-42" in x for x in reds2) and reds2)
    expect("--strict 下 READY 缺 token 判红", any("TOKEN_MISSING" in x for x in reds2))

    # 非 strict：重复只报不红（审计语义）
    _e3, r3, reds3 = audit_text(dup, tokens, classify=lambda k: "READY", strict=False)
    expect("非 strict 只报告不判红", r3["duplicates"] == [42] and reds3 == [])

    print("SELFTEST {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


def main(argv=None):
    p = argparse.ArgumentParser(description="M3 whitelist T-* 登记号审计（只读。零端口）")
    sub = p.add_subparsers(dest="cmd", required=True)
    pa = sub.add_parser("audit", help="审计 whitelist.md 的 T-* 唯一性与夹具 token 解析")
    pa.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pa.add_argument("--strict", action="store_true",
                    help="把 DUPLICATE / TOKEN_MULTI /（READY 的）TOKEN_MISSING 判为 rc=1")
    pa.add_argument("--min-num", type=int, default=36,
                    help="只审 T-<n> 及以后（默认 36 = M3 登记块起点）")
    pa.add_argument("--json", default=None, help="把审计报告写成 JSON")
    pa.set_defaults(func=cmd_audit)
    ps = sub.add_parser("selftest", help="合成文本 + 注入式 classify 的 fail-closed 自检")
    ps.set_defaults(func=cmd_selftest)
    args = p.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
