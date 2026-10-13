#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""rng_consumer_groups.py — Claim B「全服 RNG 消费者站点组」的**共享加载/分类/校验**模块。

唯一真源 = 同目录的 `consumer_groups.tsv`（列 `group/side/pattern/oracle_ref/note`）。
`rng_ledger_diff.py` 与 `m3/attack_rng_ledger.py` 都从这里取表/分类，**不许各自手抄**。

设计要点（owner 2026-10-13 09:35 派单）：
  * 每条 trace 站点先试**攻击路径**（由调用方传入该侧的 `is_attack` 回调），命中即 `attack`；
    未命中再按本表 `pattern`（对归一化后的 `\` → `/` site 做 `re.search`）归组；
    仍未命中 ⇒ `other`（**表漂移信号**，`--require-closed` 下判红，拒给假绿）。
  * 组 vocabulary 两侧共用 ⇒「site 集合 + 相对次序 diff」才有仲裁力。
  * `validate()` 是离线可跑的**表自检**（未知组/side、缺 pattern、CORE 组两侧空缺等）。
"""
import os
import re
from collections import Counter

TABLE_NAME = "consumer_groups.tsv"

# owner 点名的四组：怪物 AI / 掉落 / 商人 / 刷怪（全服非攻击消费者的绝对主体）。
CORE_GROUPS = ("monster_ai", "drop", "merchant", "spawn")
# 同族构造期/宿主组（C# 侧有独立站点的）。
AUX_GROUPS = ("construct", "npc_ai", "revive", "homemap", "startup")
KNOWN_GROUPS = CORE_GROUPS + AUX_GROUPS
KNOWN_SIDES = ("csharp", "rust")

ATTACK_GROUP = "attack"
OTHER_GROUP = "other"


def norm(site):
    return (site or "").replace("\\", "/")


def default_table_path():
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), TABLE_NAME)


class GroupsTable(object):
    """`consumer_groups.tsv` 的只读视图；行序 = 文件序（同 side 内先命中先算）。"""

    def __init__(self, rows, path):
        # rows: [(group, side, compiled_rx, oracle_ref, note), ...]
        self.rows = rows
        self.path = path

    def group_of(self, site, side):
        s = norm(site)
        for group, rside, rx, _ref, _note in self.rows:
            if rside == side and rx.search(s):
                return group
        return None

    def groups_for_side(self, side):
        out = []
        for group, rside, _rx, _ref, _note in self.rows:
            if rside == side and group not in out:
                out.append(group)
        return out

    def core_groups_for_side(self, side):
        return [g for g in self.groups_for_side(side) if g in CORE_GROUPS]

    def validate(self):
        """返回问题清单（空 = 表自洽）。离线可跑，不做任何 I/O 之外的事。"""
        problems = []
        seen = set()
        for i, (group, side, rx, ref, note) in enumerate(self.rows, 1):
            tag = "行%d(%s/%s)" % (i, group, side)
            if group not in KNOWN_GROUPS:
                problems.append("%s: 未知 group %r（不是 %s）" % (tag, group, "、".join(KNOWN_GROUPS)))
            if side not in KNOWN_SIDES:
                problems.append("%s: 未知 side %r" % (tag, side))
            if not (rx.pattern or "").strip():
                problems.append("%s: pattern 为空" % tag)
            if not (ref or "").strip():
                problems.append("%s: oracle_ref 为空（每行都要可溯源）" % tag)
            if (group, side, rx.pattern) in seen:
                problems.append("%s: 重复行（同 group/side/pattern）" % tag)
            seen.add((group, side, rx.pattern))
        for side in KNOWN_SIDES:
            for group in CORE_GROUPS:
                if group not in self.groups_for_side(side):
                    # rust 侧 merchant 等暂时真实缺位，本项只对 csharp 强制（oracle 全组齐备）。
                    if side == "csharp":
                        problems.append("CORE 组 %s 在 side=%s 无 pattern（oracle 侧必须齐备）" % (group, side))
        return problems


def parse_rows(text):
    rows = []
    for lineno, line in enumerate(text.splitlines(), 1):
        line = line.rstrip("\r\n")
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 4:
            raise ValueError("%s:%d: 列不足（需要 group/side/pattern/oracle_ref[note]）: %r"
                             % (TABLE_NAME, lineno, line))
        group, side, pattern = parts[0].strip(), parts[1].strip(), parts[2]
        ref = parts[3].strip() if len(parts) > 3 else ""
        note = parts[4] if len(parts) > 4 else ""
        try:
            rx = re.compile(pattern)
        except re.error as exc:
            raise ValueError("%s:%d: 非法正则 %r: %s" % (TABLE_NAME, lineno, pattern, exc))
        rows.append((group, side, rx, ref, note))
    return rows


def load(path=None):
    path = path or default_table_path()
    with open(path, "r", encoding="utf-8-sig") as fh:
        return GroupsTable(parse_rows(fh.read()), path)


# ---------------------------------------------------------------------------
# trace 读取 / 归组（两侧 TSV 同形：`seq \t kind \t max \t result \t site`）
# ---------------------------------------------------------------------------

def iter_sites(path):
    """逐行 yield 站点字符串；跳过空行 / `INJECT_OK` / `SEED_PROBE` / 非法行。

    与 `rng_ledger_diff.parse_trace`、`attack_rng_ledger.parse_trace` 同取舍，但**流式**，
    不在内存里留全量 row（C# 活栈 trace 可达 5M 行 / 数百 MB）。
    """
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\r\n")
            if not line or line.startswith("SEED_PROBE") or "INJECT_OK" in line:
                continue
            parts = line.split("\t")
            if len(parts) < 4:
                continue
            try:
                int(parts[0])
            except ValueError:
                continue
            yield parts[4] if len(parts) >= 5 else ""


def classify(site, side, table, is_attack):
    """`attack`（is_attack 命中）→ 表内组 → 否则 `other`。"""
    if is_attack(site, side):
        return ATTACK_GROUP
    return table.group_of(site, side) or OTHER_GROUP


def sequence(path, side, table, is_attack, include_attack=True):
    """按 trace 顺序返回组标签列表；`include_attack=False` 时剔掉攻击组。"""
    out = []
    for site in iter_sites(path):
        g = classify(site, side, table, is_attack)
        if not include_attack and g == ATTACK_GROUP:
            continue
        out.append(g)
    return out


def summarize(path, side, table, is_attack, require_closed=False, include_attack=True):
    """返回 `(counts, unclassified, total)`；`counts` 是 `Counter(group -> n)`。"""
    counts = Counter()
    unclassified = Counter()
    total = 0
    for site in iter_sites(path):
        total += 1
        g = classify(site, side, table, is_attack)
        if not include_attack and g == ATTACK_GROUP:
            continue
        counts[g] += 1
        if g == OTHER_GROUP:
            unclassified[norm(site)] += 1
    return counts, unclassified, total


def rle(seq):
    """组序列 → `[(group, run_len), ...]`（连续同名压一段）。"""
    out = []
    for g in seq:
        if out and out[-1][0] == g:
            out[-1] = (g, out[-1][1] + 1)
        else:
            out.append((g, 1))
    return out


def first_divergence(a, b):
    """逐位比；返回第一处分歧的 index，全等（前缀）返回 None。"""
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            return i
    return None
