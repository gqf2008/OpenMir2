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
AUX_GROUPS = ("construct", "npc_ai", "revive", "homemap", "startup", "space_move", "slave_mutiny")
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
# construct 组**已启用**（片114：固定 NPC/商人列表构造链的唯一抽数点 = `crates/world/src/entity.rs` 的 `BaseAnimalCtorDraws::draw`；怪物构造仍折在 mongen 宿主）。owner 裁决：**接受这个折叠为「登记建模差异」**，但要求**机判等价**——
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


# ---- C# 方法体归属语义牙（2026-10-14 收口；与 `drop-exp/drop_exp_purity_gate.py` 同口径）----
#
# 存在性/范围牙只能抓「文件不存在 / 行号越界」；真身缺陷 `RegenMonsters` 的 ref 曾写
# `WorldServer.MonGen.cs:238`（落在 `ProcessMonsters` 里，行号在范围内）⇒ 人复核会找空。
# 这里再加一道**语义**牙：C# 行的 ref 必须落在其 `pattern` 点名的**方法体**内。
# 同片还收 reviewer-5 `f7318e59` 的 O-1：旧口径把任何 `name(`（含 `if (Think()) { … }`
# 的调用点）都当声明 ⇒ 产出**伪 range**（潜在假负通道）；现要求「名字前是类型/修饰符」的真声明。
# 正则族名（`GameItemSystem.UpgradeRandom\w+@...`）按**字面前缀**匹配（`UpgradeRandom\w*(`）。
#
# 本段与 `drop-exp/drop_exp_purity_gate.py` 的同名实现**逐行同口径**（两处各带改坏必红：
# 伪 range / 调用点 / 正则族错行），单侧改动会被对面的 selftest 咬住。

# 控制/表达式关键字：若紧邻被点名符号的**前一个词**是它们，则匹配处是调用/表达式而非声明。
_CTRL_BLOCKERS = frozenset((
    "if", "while", "for", "foreach", "switch", "catch", "lock", "using",
    "return", "new", "case", "else", "do", "throw", "yield", "when", "in", "is", "as",
))


def _pattern_symbol(pattern):
    """从站点 `pattern` 抽出被点名的方法/构造名 + 是否**正则族**。

    `...\\Class.Method@File.cs:NN` ⇒ `("Method", False)`；`\\.ctor` ⇒ 该类名（构造函数）；
    含正则元字符的族名（`GameItemSystem.UpgradeRandom\\w+@...`）⇒ 取**字面前缀**
    （`("UpgradeRandom", True)`，按前缀覆盖 `UpgradeRandomWeapon/Dress/...`）。抽不出
    （类名本身是正则，如 `Monsters.\\w+..ctor@`）⇒ `(None, False)`（跳过语义牙，不误伤）。
    """
    head = (pattern or "").split("@")[0].replace("\\.", ".")
    segs = [s for s in head.split(".") if s]
    if not segs:
        return None, False
    if segs[-1] == "ctor":
        if len(segs) < 2:
            return None, False
        segs = segs[:-1]
    seg = segs[-1]
    m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", seg)
    if not m:
        return None, False
    literal = m.group(0)
    return literal, literal != seg


def _read_lines(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read().splitlines()


def _code_only(lines):
    """逐行剥掉字符串/字符/注释（花括号计数前用，避免 `'{'` 之类误计）。"""
    out = []
    in_block = False
    for s in lines:
        i = 0
        n = len(s)
        res = []
        while i < n:
            if in_block:
                j = s.find("*/", i)
                if j < 0:
                    break
                in_block = False
                i = j + 2
                continue
            c = s[i]
            if c == '"':
                if i > 0 and s[i - 1] == "@":  # C# 逐字字符串（内部 `""` 转义）
                    j = i + 1
                    while j < n:
                        if s[j] == '"':
                            if j + 1 < n and s[j + 1] == '"':
                                j += 2
                                continue
                            break
                        j += 1
                    i = j + 1
                    continue
                j = i + 1
                while j < n:
                    if s[j] == "\\":
                        j += 2
                        continue
                    if s[j] == '"':
                        break
                    j += 1
                i = j + 1
                continue
            if c == "'":
                j = i + 1
                while j < n:
                    if s[j] == "\\":
                        j += 2
                        continue
                    if s[j] == "'":
                        break
                    j += 1
                i = j + 1
                continue
            if c == "/" and i + 1 < n and s[i + 1] == "/":
                break
            if c == "/" and i + 1 < n and s[i + 1] == "*":
                in_block = True
                i += 2
                continue
            res.append(c)
            i += 1
        out.append("".join(res))
    return out


def _looks_like_declaration(masked_line, match):
    """匹配处是否像**方法/构造声明**（而非调用点后跟花括号块）。

    O-1（reviewer-5 `f7318e59`，2026-10-14 收口）：旧口径把任何 `name(` 都当声明，于是
    `if (Think()) { … }` 的**块**被误当成 `Think` 的方法体（伪 range）⇒ 假负通道。
    判据 = 名字前（同行）最后一个非空字符必须是标识符尾 / 泛型 `>` / 数组 `]`
    （即前面是类型或修饰符），且前一个词不是控制/表达式关键字。多行签名保守跳过。
    """
    before = masked_line[:match.start()].rstrip()
    if not before:
        return False
    if before[-1] not in (">", "]") and not re.match(r"[A-Za-z0-9_]", before[-1]):
        return False
    last = re.search(r"([A-Za-z_][A-Za-z0-9_]*)\s*$", before)
    if last and last.group(1) in _CTRL_BLOCKERS:
        return False
    return True


def _method_body_ranges(lines, name, prefix=False):
    """找 `name` 每个**声明**的方法体 `(decl_line, close_line)`（1-based，含声明行）。

    `prefix=True`（正则族名）⇒ 按 `<name>\\w*(` 前缀匹配。调用点（`_looks_like_declaration`）
    不再产生伪 range；`;` 先于 `{` 出现也算「非声明体」（原口径）。
    """
    masked = _code_only(lines)
    pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name)
                     + (r"\w*" if prefix else "") + r"\s*\(")
    ranges = []
    for idx, ml in enumerate(masked):
        match = pat.search(ml)
        if not match or not _looks_like_declaration(ml, match):
            continue
        opened = False
        depth = 0
        j = idx
        while j < len(masked):
            stop = False
            for ch in masked[j]:
                if ch == "{":
                    opened = True
                    depth += 1
                elif ch == "}":
                    depth -= 1
                    if opened and depth == 0:
                        ranges.append((idx + 1, j + 1))
                        stop = True
                        break
                elif ch == ";" and not opened:
                    stop = True
                    break
            if stop:
                break
            j += 1
    return ranges


def check_oracle_ref_methods(rows, repo_root=REPO_ROOT):
    """C# 行的 ref 必须落在其 `pattern` 点名的方法体内（语义锚；Rust / 不可解析行跳过）。

    抓不到方法体（同名多载皆不含 / 解析失败）⇒ 点名 RED（fail-closed）；`pattern` 抽不出
    方法名 ⇒ 跳过（不可判，保守不误伤）。
    """
    problems = []
    for i, (group, side, rx, ref, _note) in enumerate(rows, 1):
        if side != "csharp":
            continue
        line = oracle_ref_line(ref)
        name, is_regex = _pattern_symbol(rx.pattern)
        if line is None or not name:
            continue
        path = resolve_oracle_ref_path(ref, repo_root)
        if path is None or not os.path.isfile(path):
            continue  # 存在性牙已点名，不重复
        ranges = _method_body_ranges(_read_lines(path), name, prefix=is_regex)
        if not any(a <= line <= b for a, b in ranges):
            problems.append(
                "行%d(%s/%s): oracle_ref 行 %d 不在 `%s` 方法体内（range=%s；ref=%s）"
                % (i, group, side, line, name, ranges, ref))
    return problems


def check_oracle_refs(rows, repo_root=REPO_ROOT):
    """`oracle_ref` 全量保鲜：存在性/范围 + C# 方法体归属。"""
    return validate_oracle_refs(rows, repo_root) + check_oracle_ref_methods(rows, repo_root)


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

    # oracle_ref **方法体归属**语义牙（2026-10-14 收口；与 `drop-exp/drop_exp_purity_gate.py`
    # 同口径）：存在性/范围牙抓不到「行号在范围内但落在**别的**方法里」的真身错锚。
    def _swap_pattern_ref(sub, new_ref):
        return [(g, s, rx, (new_ref if sub in rx.pattern else ref), note)
                for (g, s, rx, ref, note) in table.rows]

    method_refs = check_oracle_ref_methods(table.rows)
    check("oracle-ref-methods-fresh", not method_refs, "; ".join(method_refs))

    # 改坏必红：真身错锚 `RegenMonsters` 的 ref 写 `:238`（落在 `ProcessMonsters` 里）⇒ 必红。
    m_bad = check_oracle_ref_methods(
        _swap_pattern_ref("RegenMonsters", "src/GameSrv/Word/WorldServer.MonGen.cs:238"))
    check("mutation-oracle-ref-method-red",
          any("不在 `RegenMonsters` 方法体内" in p for p in m_bad), "; ".join(m_bad))

    # 改坏必红（O-1）：`MonsterObject.cs:255` 是调用点 `if (Think()) { … }` 的块
    # （旧口径伪 range `(255,259)` 会放行）⇒ 收紧为「真声明」后必点名。
    m_pseudo = check_oracle_ref_methods(
        _swap_pattern_ref("Think", "src/M2Server/Monster/MonsterObject.cs:255"))
    check("mutation-oracle-ref-pseudo-callsite-red",
          any("不在 `Think` 方法体内" in p for p in m_pseudo), "; ".join(m_pseudo))

    # 改坏必红（正则族）：`UpgradeRandom\w+` 的 ref 从 `:182`（声明）错写成 `:142`
    # （`GetUpgrade` 方法体内）⇒ **字面前缀**匹配下必点名。
    m_rx = check_oracle_ref_methods(
        _swap_pattern_ref("UpgradeRandom", "src/M2Server/Items/GameItemSystem.cs:142"))
    check("mutation-oracle-ref-method-regex-family-red",
          any("不在 `UpgradeRandom` 方法体内" in p for p in m_rx), "; ".join(m_rx))

    # 反向：正则族名的**正确** ref `:182`（`UpgradeRandomWeapon` 声明）⇒ 不误伤。
    rx_ok = check_oracle_ref_methods(
        _swap_pattern_ref("UpgradeRandom", "src/M2Server/Items/GameItemSystem.cs:182"))
    check("oracle-ref-method-regex-family-ok", not rx_ok, "; ".join(rx_ok))

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
