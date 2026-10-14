#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M3⑧-5 行会战「先扣钱后判掌门」登记行存在性 + 锚点门（`m3_guild_war_quirk_registry_gate.py`）——只读、零端口。

## 为什么有它

`mir2-rs/tests/parity/whitelist.md` 是行为差异 / 复刻缺陷的**唯一真源**。⑧-5
（卡 `openmir2-m3-guild-war`）的 reviewer-6 **O-1** 指出：NPC 层
`GuildOfficial.ReQuestGuildWar`（`GuildOfficial.cs:186`）先 `DecGold(GuildWarPrice)`，
`PlayObject.ReQuestGuildWar`（`PlayObject.Base.cs:3351`）才判 `IsGuildMaster()` ⇒
「非掌门 / 非 0 区 / 目标缺失也白扣」这条照搬怪行为当时只在 `crates/world/src/guild.rs`
模块头 + 证据 README 说明，`whitelist.md` 无行 ⇒「登记在案」不可机判、将来被「修好」不会被发现。

本件把「T-54 登记行**存在**、编号**唯一**、且带 oracle 锚 + Rust 锁定用例名」做成一条命令 +
可复跑读数。它不是通用的 whitelist 审计（那是 `m3_whitelist_registry_audit.py`，审 T-* 全局唯一性 /
夹具 token 解析）：本门只对**本片声明的登记合同** fail-closed —— 缺行 / 撞号 / 锚点不符 ⇒ 点名 + `rc=1`。

## 命令

    python -I m3_guild_war_quirk_registry_gate.py check [--whitelist PATH]
    python -I m3_guild_war_quirk_registry_gate.py selftest

口径：**只读**（只读 whitelist.md）、**零端口**、不写仓库。`selftest` 用合成文本 + 改坏必红。

## 接入（2026-10-14，卡 `openmir2-m3-guild-war`）

本门与 `m3_pet_slave_registry_gate.py` 同形：若**无任何调用方**（只在证据 README 里手工跑）
就会成为「在场却坏无声」隐患。故随本片一并接入 `fixture_chain_gate.py` 的 `SHARED_CHECKS`
（`guild_war_quirk_registry_selftest` + `guild_war_quirk_registry_check`），不看矩阵状态、独立常跑；
`fixture_chain_gate.selftest` 内含**防回退牙**（本门被反注册 ⇒ 链门 selftest RED）。

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

# 登记行 = `| <前缀>-<数字> | ...`；本门只认 T-*。
ROW_RE = re.compile(r"^\|\s*([A-Z]{1,3})-(\d+)\s*\|(.*)$", re.MULTILINE)

# 本片声明的登记合同：token -> 该行必须出现的 oracle 锚 / Rust 锁定用例名 / 语义关键词。
# 锚点取自冻结 oracle（`src/**`，只读；本门不改它）与已合入的 Rust 用例名。
REQUIRED = [
    ("T-54", ["GuildOfficial.cs:186", "PlayObject.Base.cs:3351", "PlayObject.cs:4402",
              "GuildWarPrice", "request_guild_war", "non_master_still_pays_gold"]),
]

# 历史编号说明（不判红，只提示）。
RESERVED_NOTE = {
    "T-48": "已闭合卡 openmir2-m3-whitelist-registry-integrity 建议留给「T-42 撞号去重」（待 owner）",
    "T-52": "已由卡 openmir2-m3-party-recall-ext（合入 master 2d2426f5b）占用",
    "T-53": "已由卡 openmir2-m3-party-recall-ext（合入 master 2d2426f5b）占用",
}


def parse_rows(text):
    """⇒ [{token, prefix, num, line, body}]（按出现顺序；body = `|` 后到行尾的原文）。"""
    out = []
    for m in ROW_RE.finditer(text):
        prefix, num, body = m.group(1), int(m.group(2)), m.group(3)
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

    # ② 提示（不判红）：本片声明的保留号若出现在表里（编号说明需同步）。
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
    print("GUILD_WAR_QUIRK_REGISTRY_GATE whitelist={0} required={1}".format(
        args.whitelist, len(REQUIRED)))
    for f in findings:
        print("  FINDING {0}".format(f))
    if reds:
        print("GUILD_WAR_QUIRK_REGISTRY_RED {0}：{1}".format(len(reds), "; ".join(reds)))
        return 1
    print("GUILD_WAR_QUIRK_REGISTRY GREEN（{0} 行登记齐 + 锚点齐 + 编号唯一）".format(len(REQUIRED)))
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

    t54_body = ("N6 先扣钱后判掌门 GuildOfficial.cs:186-187 PlayObject.Base.cs:3351 "
                "PlayObject.cs:4402 GuildWarPrice request_guild_war_non_master_still_pays_gold "
                "T-39 AddWarGuild")
    base = "\n".join([
        row("T-54", t54_body),
        # 干扰项：T-42 撞号（本门不判红） + 邻行 T-39 / T-46
        row("T-39", "N2 AddWarGuild 反逻辑"),
        row("T-42", "⑤-2 商人 tick"),
        row("T-42", "⑥-5 任务夹具"),
        row("T-46", "夹具"),
    ])

    _f, reds = check_text(base)
    expect("T-54 登记齐 + 锚点齐 -> GREEN（T-42 撞号不判红）", reds == [])

    no_t54 = base.replace(row("T-54", t54_body) + "\n", "")
    _f, reds = check_text(no_t54)
    expect("删 T-54 行 -> RED", any("MISSING T-54" in r for r in reds))

    wrong_anchor = base.replace("PlayObject.Base.cs:3351", "PlayObject.Base.cs:9999")
    _f, reds = check_text(wrong_anchor)
    expect("锚点改错 -> RED", any("ANCHOR T-54" in r for r in reds))

    dup = base + "\n" + row("T-54", "重复行 " + t54_body)
    _f, reds = check_text(dup)
    expect("T-54 撞号 -> RED", any("DUPLICATE T-54" in r for r in reds))

    # 「照搬口径被悄悄改写」的一个可机判面：删掉 Rust 锁定用例名 / 号链交叉引用即缺锚点。
    no_lock = base.replace("non_master_still_pays_gold", "some_other_test")
    _f, reds = check_text(no_lock)
    expect("删掉 Rust 锁定用例名 -> RED", any("ANCHOR T-54" in r for r in reds))

    empty = "## 无登记\n"
    _f, reds = check_text(empty)
    expect("空表 -> 全 MISSING", len([r for r in reds if r.startswith("MISSING")]) == len(REQUIRED))

    print("SELFTEST {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


def main(argv=None):
    p = argparse.ArgumentParser(description="M3⑧-5 行会战怪行为登记门（只读。零端口）")
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
