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
  * `validate()` 是离线可跑的**表自检**（未知组/side、缺 pattern、CORE/AUX 组空缺等）。

判据口径（reviewer-5 O-2）：**组内 draw 计数本身不是判据**——只有「组集合」与「共享组的
相对次序」是。多抽一次只有在它改变了共享组序列长度/相对位置时才会被 `SHARED_REL_ORDER_DIFF`
抓到；不要把它当计数门。

行级覆盖（reviewer-5 O-1）：`iter_sites(path, stats=...)` 会把**解析不了的行**（列<4 或首列非整数）
计入 `stats["rows_dropped"]`。`GROUP_COVERAGE closed` 只证明「已解析行 100% 归类」；只有
`rows_dropped == 0` 才证明「文件里没有被静默吞掉的行」。调用方（`groups --require-closed` /
`groupdiff --strict`）据此 fail-closed。
"""
import os
import re
from collections import Counter

TABLE_NAME = "consumer_groups.tsv"

# 本文件：<repo>/mir2-rs/tests/parity/m3/rng-trace-probe/rng_consumer_groups.py
# 上溯 5 级 = 仓库根（用于把 `oracle_ref` 解析成真实源码路径）。
_HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(_HERE, "..", "..", "..", "..", ".."))

# owner 点名的四组：怪物 AI / 掉落 / 商人 / 刷怪（全服非攻击消费者的绝对主体）。
CORE_GROUPS = ("monster_ai", "drop", "merchant", "spawn")
# 同族构造期/宿主组（C# 侧有独立站点的）。
AUX_GROUPS = ("construct", "npc_ai", "revive", "homemap", "startup")
KNOWN_GROUPS = CORE_GROUPS + AUX_GROUPS
KNOWN_SIDES = ("csharp", "rust")

ATTACK_GROUP = "attack"
OTHER_GROUP = "other"

# ---------------------------------------------------------------------------
# R-6 folded 映射（owner 2026-10-13 裁决）
#
# C# oracle 在**实体构造期**（`BaseObject..ctor` / `AnimalObject..ctor` / `MonsterObject..ctor` /
# `Monsters.*..ctor` / `PlayObject..ctor`）独立抽数 ⇒ 独立 `construct` 组；而 Rust 把构造期抽数
# **折叠进刷怪路径**（`crates/world/src/mongen.rs` 的 `AnimalState::from_spawn_draws`）⇒ Rust 侧
# 暂无独立 `construct` 站点。owner 裁决：**接受这个折叠为「登记建模差异」**，但要求**机判等价**——
# 「C# `construct` 折叠到 Rust `spawn` 后，**折叠计数与相对次序必须相等**；**只写白名单不校验不算达成**」。
#
# 因此：`construct` 归一到 `spawn`（两侧同一条规则），`groupdiff --fold` 在**折叠后的序列**上
# 重新算「组集合 + 组计数 + 共享组相对次序」，并且把「计数不相等」显式判成 `FOLDED_COUNT_DIFF`。
# 折叠不是豁免：计数/次序不等 ⇒ 照样 RED。
FOLD_RULES = {"construct": "spawn"}


def fold_group(group):
    """把一个组名归一到**折叠 vocabulary**（无规则则原样返回）。"""
    return FOLD_RULES.get(group, group)


def folded(seq):
    """组序列 → 折叠组序列（逐元素 `fold_group`）。"""
    return [fold_group(g) for g in seq]


def folded_counts(counts):
    """`Counter(group -> n)` → 折叠后的 `Counter(folded_group -> n)`（计数相加）。"""
    out = Counter()
    for g, n in (counts or {}).items():
        out[fold_group(g)] += n
    return out


def validate_folds():
    """折叠规则的**表自检**（空 = 自洽）。离线可跑。

    只允许「已知组 → 已知组、且源≠目标」；`construct → spawn` 是 owner 点名的 R-6 裁决。
    """
    problems = []
    for src, dst in sorted(FOLD_RULES.items()):
        tag = "fold %s->%s" % (src, dst)
        if src not in KNOWN_GROUPS:
            problems.append("%s: 源组 %r 不是已知组" % (tag, src))
        if dst not in KNOWN_GROUPS:
            problems.append("%s: 目标组 %r 不是已知组" % (tag, dst))
        if src == dst:
            problems.append("%s: 源与目标相同（无意义折叠）" % tag)
    return problems


# ---------------------------------------------------------------------------
# oracle_ref 可解析性自检（2026-10-13 窗口前置审计 #3；reviewer-6 `37c322a9` M-1 编号订正）
#
# `oracle_ref` 是站点归因的**唯一溯源**（`pattern` 判组，`oracle_ref` 供人复查）。但该列此前
# 只校验「非空」⇒ 一条指向不存在源码路径的 ref 能静默通过（真身缺陷：`ServerConf.LoadConfig`
# 的 ref 曾写 `src/SystemModule/Conf/ServerConf.cs`，实际在 `src/Modules/SystemModule/Conf/ServerConf.cs`），
# 承接卡/评审按它去找会找空。这里把它钉成 fail-closed：**能解析成源码文件的 ref，文件必须存在且
# 行号在范围内**；不可解析的 ref（glob、目录、Rust 模块入口无行号）按口径跳过——不是缺陷。
#
# 路径口径（与仓库布局一致，避免把相对 `mir2-rs/` 的 Rust 路径误报成缺件）：
#   * `src/...`          → 相对**仓库根**（C# oracle）
#   * `crates/...`/`bins/...` → 相对 **`mir2-rs/`**（Rust 站点）
#   * `mir2-rs/...`      → 相对**仓库根**
# ---------------------------------------------------------------------------

_SOURCE_EXTS = (".cs", ".rs")


def resolve_oracle_ref_path(ref, repo_root=REPO_ROOT):
    """`oracle_ref` → 绝对源码路径；不可解析（glob / 目录 / 非源码 / 未知前缀）⇒ None。"""
    base = (ref or "").split(":")[0].strip()
    if not base or base.endswith("/") or "*" in base or not base.endswith(_SOURCE_EXTS):
        return None
    if base.startswith("mir2-rs/"):
        rel = base
    elif base.startswith(("crates/", "bins/")):
        rel = "mir2-rs/" + base
    elif base.startswith("src/"):
        rel = base
    else:
        return None
    return os.path.join(repo_root, rel)


def oracle_ref_line(ref):
    """`path:line` → line；无行号 ⇒ None。"""
    m = re.search(r":(\d+)\s*$", (ref or "").strip())
    return int(m.group(1)) if m else None


def validate_oracle_refs(rows, repo_root=REPO_ROOT):
    """返回问题清单（空 = 所有**可解析**的 oracle_ref 都指向存在的文件且行号在范围内）。"""
    problems = []
    for i, (group, side, _rx, ref, _note) in enumerate(rows, 1):
        path = resolve_oracle_ref_path(ref, repo_root)
        if path is None:
            continue
        if not os.path.isfile(path):
            problems.append("行%d(%s/%s): oracle_ref 指向不存在的源码文件 %s（ref=%s）"
                            % (i, group, side, os.path.relpath(path, repo_root), ref))
            continue
        line = oracle_ref_line(ref)
        if line is None:
            continue
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            n = len(fh.read().splitlines())
        if line < 1 or line > n:
            problems.append("行%d(%s/%s): oracle_ref 行 %d 越界（%s 共 %d 行）"
                            % (i, group, side, line, os.path.relpath(path, repo_root), n))
    return problems


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
            if side != "csharp":
                # rust 侧 merchant/construct/npc_ai/revive 等暂时**真实缺位** ⇒ 不作表级强制；
                # 由真实 trace 的 `groups --require-closed` / `groupdiff` 如实报 GROUP_SET/DRIFT。
                continue
            for group in CORE_GROUPS:
                if group not in self.groups_for_side(side):
                    problems.append("CORE 组 %s 在 side=%s 无 pattern（oracle 侧必须齐备）" % (group, side))
            # reviewer-5 O-3：AUX 组**整组被删**也要能被 validate 咬住（不只 CORE）。
            for group in AUX_GROUPS:
                if group not in self.groups_for_side(side):
                    problems.append("AUX 组 %s 在 side=%s 无 pattern（oracle 同族组必须齐备；"
                                    "整组被删要能被 validate 咬住）" % (group, side))
        # R-6 folded 映射也纳入同一处表自检（源/目标都必须是已知组）。
        problems.extend(validate_folds())
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

def iter_sites(path, stats=None):
    """逐行 yield 站点字符串；跳过空行 / `INJECT_OK` / `SEED_PROBE`。

    与 `rng_ledger_diff.parse_trace`、`attack_rng_ledger.parse_trace` 同取舍，但**流式**，
    不在内存里留全量 row（C# 活栈 trace 可达 5M 行 / 数百 MB）。

    **行级覆盖（reviewer-5 O-1）**：`stats`（dict，可选）就地累加——
      * `rows_kept`    = 成功解析并 yield 的行数；
      * `rows_dropped` = 非空、非已知 marker、但**解析不了**的行数（列<4 或首列非整数）。
    `GROUP_COVERAGE closed` 只证明「已解析行 100% 归类」；只有 `rows_dropped == 0` 才证明
    「文件里没有被吞掉的行」。
    """
    if stats is not None:
        stats.setdefault("rows_kept", 0)
        stats.setdefault("rows_dropped", 0)
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\r\n")
            if not line or line.startswith("SEED_PROBE") or "INJECT_OK" in line:
                continue
            parts = line.split("\t")
            if len(parts) < 4:
                if stats is not None:
                    stats["rows_dropped"] += 1
                continue
            try:
                int(parts[0])
            except ValueError:
                if stats is not None:
                    stats["rows_dropped"] += 1
                continue
            if stats is not None:
                stats["rows_kept"] += 1
            yield parts[4] if len(parts) >= 5 else ""


def classify(site, side, table, is_attack):
    """`attack`（is_attack 命中）→ 表内组 → 否则 `other`。"""
    if is_attack(site, side):
        return ATTACK_GROUP
    return table.group_of(site, side) or OTHER_GROUP


def sequence(path, side, table, is_attack, include_attack=True, stats=None):
    """按 trace 顺序返回组标签列表；`include_attack=False` 时剔掉攻击组。

    `stats`（dict，可选）透传给 `iter_sites` 以累加行级覆盖计数（`rows_kept`/`rows_dropped`）。
    """
    out = []
    for site in iter_sites(path, stats):
        g = classify(site, side, table, is_attack)
        if not include_attack and g == ATTACK_GROUP:
            continue
        out.append(g)
    return out


def summarize(path, side, table, is_attack, require_closed=False, include_attack=True, stats=None):
    """返回 `(counts, unclassified, total)`；`counts` 是 `Counter(group -> n)`。

    `stats`（dict，可选）透传给 `iter_sites` 以累加行级覆盖计数（`rows_kept`/`rows_dropped`）。
    """
    counts = Counter()
    unclassified = Counter()
    total = 0
    for site in iter_sites(path, stats):
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


# ---------------------------------------------------------------------------
# 离线自检（`python -I rng_consumer_groups.py [selftest]`）
#
# 2026-10-13 窗口前置审计（卡 openmir2-m3-rng-trace-scene-groups）：宿主侧
# `bins/game-svr/src/mongen.rs`（`create_monster_oracle` / `draw_monster_construct`）也抽构造期
# 随机数，而表里只有 `crates/world/src/mongen.rs` ⇒ 固定场景（图 0137 的 MonGen 0 行）里它是
# **唯一** spawn 来源，漏归类会被算成 `other`（`groupdiff --fold --strict` 的 `GROUP_SET_DIFF`
# + DoD② 的 `MISSING_EXPECTED_GROUPS(spawn)`）⇒ **会在共用窗口里空转一轮**。
# 本自检把「该行存在且生效」钉成机判，并自带「删行 ⇒ 必红」样例（不再只靠散文）。
# ---------------------------------------------------------------------------

_SELFTEST_CASES = (
    # (side, site, is_attack, expected_group)
    ("rust", "bins/game-svr/src/mongen.rs:289", False, "spawn"),            # 宿主构造抽数（本片新增行）
    ("rust", "bins\\game-svr\\src\\mongen.rs:434", False, "spawn"),        # 盘上是 `\` ⇒ 归一后才命中
    ("rust", "crates/world/src/mongen.rs:321", False, "spawn"),
    ("rust", "crates/world/src/monai.rs:171", False, "monster_ai"),
    ("rust", "crates/formula/src/drop.rs:100", False, "drop"),
    ("rust", "bins/game-svr/src/main.rs:2884", False, "startup"),
    ("rust", "crates/world/src/combat.rs:758", True, "attack"),
    ("rust", "bins/game-svr/src/not_in_table.rs:1", False, OTHER_GROUP),   # 未登记 ⇒ other（漂移信号）
)

_HOST_ROW_PATTERN = "bins/game-svr/src/mongen\\.rs:"


def selftest():
    """返回 rc（0 = GREEN，1 = RED）。纯离线，不写任何文件。"""
    table = load()
    checks = []

    def check(name, ok, detail=""):
        checks.append((name, bool(ok), detail))

    problems = table.validate()
    check("table-validate", not problems, "; ".join(problems))

    for side, site, is_attack, want in _SELFTEST_CASES:
        got = classify(site, side, table, lambda s, sd, _a=is_attack: bool(_a))
        check("classify %s -> %s" % (site, want), got == want, "got=%s" % got)

    # 改坏必红①：删掉宿主 mongen 行 ⇒ 同一站点必须落到 `other`（证明该行承重）。
    rows = [r for r in table.rows if r[2].pattern != _HOST_ROW_PATTERN]
    check("mutation-host-row-removed", len(rows) == len(table.rows) - 1,
          "删掉 %d 行" % (len(table.rows) - len(rows)))
    mutated = GroupsTable(rows, table.path + "#mut-host")
    got = classify("bins/game-svr/src/mongen.rs:289", "rust", mutated, lambda s, sd: False)
    check("mutation-host-site-becomes-other", got == OTHER_GROUP, "got=%s" % got)

    # 改坏必红②：删掉 CORE 组（merchant/csharp）⇒ validate 必须点名。
    rows2 = [r for r in table.rows if not (r[0] == "merchant" and r[1] == "csharp")]
    check("mutation-core-group-validate-red",
          any("merchant" in p for p in GroupsTable(rows2, table.path + "#mut-core").validate()),
          "期望 validate 点名 merchant")

    # oracle_ref 可解析性（本片真身缺陷：`startup/csharp` 的 ref 缺了 `Modules/` 段）。
    refs = validate_oracle_refs(table.rows)
    check("oracle-refs-fresh", not refs, "; ".join(refs))

    def _swap_startup_ref(new_ref):
        return [(g, s, rx, (new_ref if (g == "startup" and s == "csharp") else ref), note)
                for (g, s, rx, ref, note) in table.rows]

    bad_path = validate_oracle_refs(_swap_startup_ref("src/NoSuchModule/Conf/ServerConf.cs:114"))
    check("mutation-oracle-ref-path-red",
          any("不存在" in p for p in bad_path), "; ".join(bad_path))

    bad_line = validate_oracle_refs(_swap_startup_ref("src/Modules/SystemModule/Conf/ServerConf.cs:999999"))
    check("mutation-oracle-ref-line-red",
          any("越界" in p for p in bad_line), "; ".join(bad_line))

    total = len(checks)
    bad = [(n, d) for n, ok, d in checks if not ok]
    for n, ok, d in checks:
        print("  %s %s%s" % ("ok " if ok else "RED", n, ("（%s）" % d) if d else ""))
    if bad:
        print("SELFTEST RED: rng_consumer_groups selftest %d/%d" % (total - len(bad), total))
        return 1
    print("SELFTEST GREEN: rng_consumer_groups selftest %d/%d" % (total, total))
    return 0


if __name__ == "__main__":
    import sys

    _mode = sys.argv[1] if len(sys.argv) > 1 else "selftest"
    if _mode != "selftest":
        print("usage: rng_consumer_groups.py [selftest]", file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(selftest())
