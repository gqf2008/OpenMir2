#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M3⑨ 宝宝/下属（slave）登记行存在性 + 锚点门（`m3_pet_slave_registry_gate.py`）——只读、零端口。

## 为什么有它

`mir2-rs/tests/parity/whitelist.md` 是行为差异 / 复刻缺陷的**唯一真源**。⑨-3（卡
`openmir2-m3-pet-slave-lifecycle`）的 reviewer-5 **M-1** 指出：4 条「复刻的 oracle 缺陷」
（`DC` 高字节截断 / `MasterDieMutinySpeed` 除零 / `GainSlaveExp` 死代码 / 色表 latent 越界）
当时只在**代码模块头 + 证据 README** 登记，whitelist 里没有行 ⇒「登记在案」不可机判。

本件把「⑨-2/⑨-3 的登记行**存在**、编号**唯一**、且各自带 oracle 锚」做成一条命令 + 可复跑读数。
它不是通用的 whitelist 审计（那是 `m3_whitelist_registry_audit.py`，审 T-* 唯一性 / 夹具 token 解析）：
本门只对**本片声明的登记合同** fail-closed —— 缺行 / 撞号 / 锚点不符 ⇒ 点名 + `rc=1`。

## 命令

    python -I m3_pet_slave_registry_gate.py check [--whitelist PATH]
    python -I m3_pet_slave_registry_gate.py selftest

口径：**只读**（只读 whitelist.md）、**零端口**、不写仓库。`selftest` 用合成文本 + 改坏必红。

## 边界（如实登记，不假绿）

**不检查** `T-42` 撞号（whitelist 现存两行 `| T-42 |`：⑤-2 商人 tick vs ⑥-5 任务夹具）。
那是跨车道历史问题，已由闭合卡 `openmir2-m3-whitelist-registry-integrity` 路由 owner、
建议重编到 T-48；本门不单方面重编号、也不拿它判本片的红。
"""
from __future__ import annotations

import argparse
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_WHITELIST = os.path.normpath(os.path.join(HERE, "..", "whitelist.md"))

# 登记行 = `| <前缀>-<数字> | ...`；本门只认 B-* 与 T-*。
ROW_RE = re.compile(r"^\|\s*([A-Z]{1,3})-(\d+)\s*\|(.*)$", re.MULTILINE)

# 本片声明的登记合同：token -> 该行必须出现的 oracle 锚 / 语义关键词。
# 锚点取自冻结 oracle（`src/**`，只读；本门不改它）。
REQUIRED = [
    ("B-112", ["PlayObject.Base.cs:349", "process_slave_mutiny", "截"]),
    ("B-113", ["PlayObject.Base.cs:350", "ServerConf.cs:521", "MasterDieMutinySpeed"]),
    ("B-114", ["MonsterObject.cs:71-84", "GainSlaveExp"]),
    ("B-115", ["BaseObject.Base.cs:877-881", "Grobal2.cs:23", "SlaveColor"]),
    ("T-49", ["BaseObject.Base.cs:208-217", "slave_list_cleanup_all", "MonsterObject.cs:251-360"]),
]

# 历史编号说明（不判红，只提示）：T-48 被建议留给 T-42 去重；B-112 曾被建议但未落行。
RESERVED_NOTE = {
    "T-48": "已闭合卡 openmir2-m3-whitelist-registry-integrity 建议留给「T-42 撞号去重」（待 owner）",
    "B-112": "早前 reviewer 建议用于 811 Job 面，作者判定 1:1 ⇒ 未落行；本片取用",
}


def parse_rows(text):
    """⇒ [{token, prefix, num, line, body}]（按出现顺序；body = `|` 后到行尾的原文）。"""
    out = []
    for m in ROW_RE.finditer(text):
        prefix, num, body = m.group(1), int(m.group(2)), m.group(3)
        # 行/列标记行（`| --- |`）不会匹配（要求数字），无需特判。
        line = text.count("\n", 0, m.start()) + 1
        out.append({"token": "{0}-{1}".format(prefix, num), "prefix": prefix,
                    "num": num, "line": line, "body": body})
    return out


def check_text(text):
    """纯函数（便于 selftest 注入）⇒ (findings, reds)。"""
    rows = parse_rows(text)
    by_token = {}
    for r in rows:
        by_token.setdefault(r["token"], []).append(r)

    findings = []
    reds = []

    # ① 本片声明的行：存在、唯一、锚点齐。
    for token, anchors in REQUIRED:
        hits = by_token.get(token, [])
        if not hits:
            reds.append("MISSING {0}：whitelist 里没有 `| {0} |` 行".format(token))
            continue
        if len(hits) > 1:
            where = ",".join("L{0}".format(h["line"]) for h in hits)
            reds.append("DUPLICATE {0} x {1} @ {2}".format(token, len(hits), where))
            continue
        body = hits[0]["body"]
        missing = [a for a in anchors if a not in body]
        if missing:
            reds.append("ANCHOR {0} @L{1}：缺锚点 {2}".format(
                token, hits[0]["line"], ",".join(missing)))
        else:
            findings.append("OK {0} @L{1}：锚点 {2}/{2} 齐".format(
                token, hits[0]["line"], len(anchors)))

    # ② B 段全局编号唯一（T 段唯一性归 m3_whitelist_registry_audit.py；T-42 撞号是已路由 owner 的历史问题）。
    bnums = {}
    for r in rows:
        if r["prefix"] == "B":
            bnums.setdefault(r["num"], []).append(r)
    for num, rs in sorted(bnums.items()):
        if len(rs) > 1:
            where = ",".join("L{0}".format(x["line"]) for x in rs)
            reds.append("DUPLICATE B-{0} x {1} @ {2}".format(num, len(rs), where))

    # ③ 提示（不判红）：本片声明的号若被别处占用（编号说明需同步）。
    for token, note in RESERVED_NOTE.items():
        if token in by_token:
            findings.append("NOTE {0} 存在（{1}）".format(token, note))
    return findings, reds


def cmd_check(args):
    if not os.path.isfile(args.whitelist):
        print("WHITELIST_MISSING {0}".format(args.whitelist))
        return 2
    with open(args.whitelist, "r", encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    findings, reds = check_text(text)
    print("PET_SLAVE_REGISTRY_GATE whitelist={0} required={1} B-unique={2}".format(
        args.whitelist, len(REQUIRED), "checked"))
    for f in findings:
        print("  FINDING {0}".format(f))
    if reds:
        print("PET_SLAVE_REGISTRY_RED {0}：{1}".format(len(reds), "; ".join(reds)))
        return 1
    print("PET_SLAVE_REGISTRY GREEN（{0} 行登记齐 + 锚点齐 + B 段编号唯一）".format(len(REQUIRED)))
    return 0


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

    def row(token, body):
        return "| {0} | {1} |".format(token, body)

    base = "\n".join([
        row("B-112", "DC 截断 PlayObject.Base.cs:349 process_slave_mutiny（截）"),
        row("B-113", "除零 PlayObject.Base.cs:350 ServerConf.cs:521 MasterDieMutinySpeed"),
        row("B-114", "死代码 MonsterObject.cs:71-84 GainSlaveExp"),
        row("B-115", "色表 BaseObject.Base.cs:877-881 Grobal2.cs:23 SlaveColor"),
        row("T-49", "调度 BaseObject.Base.cs:208-217 slave_list_cleanup_all MonsterObject.cs:251-360"),
        # 干扰项：T-42 撞号（本门不判红） + 夹具 T-45
        row("T-42", "⑤-2 商人 tick"),
        row("T-42", "⑥-5 任务夹具"),
        row("T-45", "夹具"),
    ])

    _f, reds = check_text(base)
    expect("登记齐 + 锚点齐 -> GREEN（T-42 撞号不判红）", reds == [])

    no_b113 = base.replace(row("B-113", "除零 PlayObject.Base.cs:350 ServerConf.cs:521 MasterDieMutinySpeed") + "\n", "")
    _f, reds = check_text(no_b113)
    expect("删 B-113 行 -> RED", any("MISSING B-113" in r for r in reds))

    wrong_anchor = base.replace("PlayObject.Base.cs:350", "PlayObject.Base.cs:9999")
    _f, reds = check_text(wrong_anchor)
    expect("锚点改错 -> RED", any("ANCHOR B-113" in r for r in reds))

    dup = base + "\n" + row("B-112", "重复行 PlayObject.Base.cs:349 process_slave_mutiny 截")
    _f, reds = check_text(dup)
    expect("B-112 撞号 -> RED", any("DUPLICATE B-112" in r for r in reds))

    empty = "## 无登记\n"
    _f, reds = check_text(empty)
    expect("空表 -> 5 行全 MISSING", len([r for r in reds if r.startswith("MISSING")]) == len(REQUIRED))

    print("SELFTEST {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


def main(argv=None):
    p = argparse.ArgumentParser(description="M3⑨ slave registry gate（只读）")
    sub = p.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check")
    c.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    c.set_defaults(fn=cmd_check)
    s = sub.add_parser("selftest")
    s.set_defaults(fn=cmd_selftest)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
