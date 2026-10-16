#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""whitelist_anchor_gate.py — `mir2-rs/tests/parity/whitelist.md` 行号锚保鲜门（fail-closed）。

卡：`openmir2-m3-whitelist-anchor-drift`（多车道共享增长文件上的行号锚注定腐烂）。

## 为什么有这道门

`whitelist.md` 是「行为差异登记」的唯一真源，里面用 `` `path:line` `` 形式引用源码锚点。
其中一批登记（`T-15`/`T-26`/`T-27`/`T-31`/`T-33`/`T-56`/`T-65` 等「已实现 / 退役」条）
的 file:line 是**对着 master 某一历史 tip（`b08e065fc`）**抄的；master 一直在动
（尤其 `bins/game-svr/src/main.rs`、`crates/world/src/{world,map,combat}.rs` 这些
**多车道共享的增长文件**，10 天内被改 150/39/6/13 次）⇒ 行号**静默失准**：审查者按行号
找代码会找错位置、甚至误判假绿。此前**没有任何机判门**能发现这类漂移（`check_anchors.py`
只守 `DRIVER-CONTRACT.md`，不守 `whitelist.md`；`m3_whitelist_registry_audit.py` 只审
`T-*` 编号唯一性，不看锚）。

本门把「`whitelist.md` 里写的行号 = 该锚点声明的符号在**当前源码**里的行」钉成 fail-closed。

## 判据（任一不成立 ⇒ rc=1；`PREREQ` ⇒ rc=2）

1. **文件存在**：`` `path:line` `` 里 `path` 是仓库内源码（`.rs`/`.cs`）却解析不到文件 ⇒
   `ANCHOR_FILE_MISSING`（RED）。解析不到的**非源码**引用（客户端 `.pas` / 运行期
   `Server.conf` / `mir2_data.sql` 等仓外件）登记为 `EXTERNAL`（info，不判红）。
2. **行号在范围内**：`line`（或区间端点）必须落在 `[1, 文件行数]`，否则 ⇒ `ANCHOR_OUT_OF_RANGE`（RED）。
3. **±3 行内命中声明的符号**：从锚点上下文（同一物理行、锚点附近窗口）的反引号里抽
   「代码标识符」，其中任一出现在锚点行 ±3 行内 ⇒ OK；抽不出符号 ⇒ 记为 `WEAK`（info）；
   抽出符号但 ±3 行内一个都不命中 ⇒ `ANCHOR_DRIFT`（RED，点名「期望 vs 实际」）。
4. **高 churn 文件禁行号锚**（`symbol_unique` 约定，见
   `rng-trace-scene/check_anchors.py:88/95/102`）：对 `HIGH_CHURN` 清单里的文件，
   `whitelist.md` **不得**再写 `path:<line>`，一律改用**符号锚**（只写文件路径 +
   `` `符号` `` 上下文）；写了 `:line` ⇒ `ANCHOR_LINE_NOT_ALLOWED`（RED）。
   收益：别的车道往这些文件插代码不再制造「合并即假 RED」的行号漂移；
   符号被删 → 仍由 `ANCHOR_SYMBOL_MISSING` fail-closed 咬住（判据不降级）。
5. **fail-closed（rc=2）**：`whitelist.md` 缺失，或 `HIGH_CHURN` 任一监控文件缺失
   ⇒ `PREREQ_RED`，**绝不静默通过**。

输出：GREEN ⇒ `RESULT: ALL PASS`；否则 `RESULT: FAILED` + 逐条违例（条目号 + 锚 + 期望 vs 实际）。

## 用法

    python -I whitelist_anchor_gate.py check [--whitelist PATH] [--json OUT]
    python -I whitelist_anchor_gate.py selftest
    python -I whitelist_anchor_gate.py audit            # 覆盖面清点（全部锚 + 分类）

退出码：0 = GREEN；1 = RED；2 = PREREQ_RED。
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
# `mir2-rs/tests/parity/m3` 上溯四级 = 仓库根。
REPO_ROOT = os.path.normpath(os.path.join(HERE, "..", "..", "..", ".."))
DEFAULT_WHITELIST = os.path.normpath(
    os.path.join(REPO_ROOT, "mir2-rs", "tests", "parity", "whitelist.md"))

# ---------------------------------------------------------------------------
# 高 churn 文件清单（判据 4）。判定依据：`git log --since="10 days ago" --name-only
# -- '*.rs'` 的**改次计数**（可复跑）。阈值 = **≥5 次 / 10 天**（多车道共享的增长文件），
# 外加任务显式点名的 `crates/world/src/npc.rs`（当天被多个车道改过；其改次 2，
# 但属同一 contention 区，保守纳入）。Rust 源 + 工具侧 `src/BotSrv/**`（T-1 登记的
# 工具例外，同样被多张夹具卡反复扩展，10 天 20 次）。
# ---------------------------------------------------------------------------
HIGH_CHURN = (
    # (仓库根相对路径, 10 天改次, 说明)
    ("mir2-rs/bins/game-svr/src/main.rs", 150, "game-svr 宿主 main（多车道增长，行号锚实测漂 ≥6 次）"),
    ("mir2-rs/crates/world/src/world.rs", 39, "世界模拟主模块"),
    ("mir2-rs/crates/world/src/entity.rs", 21, "实体层"),
    ("mir2-rs/crates/world/src/combat.rs", 13, "战斗数值"),
    ("mir2-rs/crates/world/src/monai.rs", 10, "怪 AI tick"),
    ("mir2-rs/bins/game-svr/src/useitem.rs", 8, "CM_EAT 面"),
    ("mir2-rs/crates/world/src/map.rs", 6, "地图格/门/地面物"),
    ("mir2-rs/bins/game-svr/src/mongen.rs", 5, "MonGen 刷怪 sink"),
    ("mir2-rs/bins/game-svr/src/dbclient.rs", 5, "存档 DB 客户端"),
    ("mir2-rs/crates/world/src/npc.rs", 2, "固定 NPC tick（任务点名，contention 区）"),
    ("src/BotSrv/AppService.cs", 20, "BotSrv 装配（工具例外，多夹具卡扩展）"),
    ("src/BotSrv/MShare.cs", 20, "BotSrv 静态位（工具例外，多夹具卡扩展）"),
)

# 源码扩展名：解析不到文件 ⇒ RED（判据 1）。
SRC_EXTS = frozenset({"rs", "cs"})
# 其它可解析扩展名：解析不到时降级为 EXTERNAL（运行期/客户端/外部件）。
OTHER_EXTS = frozenset({"pas", "conf", "sql", "txt", "json", "md", "py", "ps1",
                        "toml", "cfg", "ini", "yml", "yaml", "xml", "sh", "bat", "csv", "tsv"})

# 完整锚点 `<path>.<ext>:<line>[-<line2>][/<line3>]`（无 `"`/`@` 前缀干扰）。
ANCHOR_RE = re.compile(
    r"(?<![A-Za-z0-9_./\\@-])"
    r"([A-Za-z0-9_][A-Za-z0-9_./\\-]*?\.([A-Za-z][A-Za-z0-9]{0,11}))"
    r":(\d+)(?:-(\d+))?(?:/(\d+))?"
)
# 同行「裸续引用」`:NNN`（`:` 前不是路径字符 ⇒ 不命中完整锚点自身的 `:NNN`）。
BARE_RE = re.compile(r"(?<![A-Za-z0-9_./\\@:-]):(\d+)(?:-(\d+))?(?:/(\d+))?")

# 上下文里的代码标识符（判据 3 的符号抽取）。
IDENT_RE = re.compile(r"`([^`\n]+)`")
# 「平凡标识符」形（用于判据 3）：允许 `::`/`.`/`_`，不含空格/CJK/路径。
PLAIN_IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_:.]*$")
# 锚点与相邻反引号 token 之间允许的**连接标点**（判据 3「紧贴」口径）。
JOIN_CHARS = set(" \t（）()【】：:、，,")
# 只取紧贴锚点的 token（中间只隔 JOIN_CHARS，且长度 ≤ JOIN_MAX）。
JOIN_MAX = 6
# 裸续引用 `:NNN` 只在紧贴文件引用（中间只隔连接标点、长度 ≤ BARE_GAP）时归属它，否则不猜。
BARE_GAP = 50
BARE_JOIN = JOIN_CHARS | {"`"}


def _norm(tok):
    return tok.replace("\\", "/")


def _read_lines(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read().splitlines()


def _build_basename_index(root):
    idx = {}
    for dirpath, dirnames, filenames in os.walk(root):
        parts = _norm(dirpath).split("/")
        if ".git" in parts or ".worktrees" in parts:
            continue
        for fn in filenames:
            rel = os.path.relpath(os.path.join(dirpath, fn), root).replace("\\", "/")
            idx.setdefault(fn, []).append(rel)
    return idx


def _resolve(path_tok, root, idx):
    """⇒ (relpath | None, reason)。reason ∈ {"ok","ambiguous","external"}。"""
    tok = _norm(path_tok)
    cands = []
    for cand in (tok, "mir2-rs/" + tok):
        ap = os.path.join(root, cand)
        if os.path.isfile(ap):
            cands.append(cand)
    if cands:
        return cands[0], "ok"
    base = os.path.basename(tok)
    hits = idx.get(base, [])
    if len(hits) == 1:
        return hits[0], "ok"
    if len(hits) > 1:
        # 带目录时按后缀消歧；否则歧义。
        if "/" in tok:
            suf = [h for h in hits if h.endswith("/" + tok) or h.endswith(tok)]
            if len(suf) == 1:
                return suf[0], "ok"
        return None, "ambiguous"
    return None, "external"


def _tokens_in(row_text):
    return [(m.start(), m.end(), m.group(1).strip()) for m in IDENT_RE.finditer(row_text)]


def _adjacent_symbol(row_text, start, end, anchor_text):
    """取**紧贴**锚点（只隔 `JOIN_CHARS`/反引号、非反引号连接标点 ≤1 个）的反引号 token 作声明符号。

    紧贴口径（判据 3）：锚点常与它声明的符号用 `（`/`：`/空白相邻，两种写法都认：
    `` `symbol`（`path:line`） `` 与 `` `path:line`（`symbol`） ``。
    中间夹了散文（CJK 词/标点）或多于一个分隔符（= 前一条锚的符号）就判「抽不到」⇒
    该条记为 WEAK（不判红）。
    """
    toks = _tokens_in(row_text)
    best = None
    for ts, te, val in toks:
        if val == anchor_text or re.search(r":\d", val):
            continue
        if re.search(r"\.(cs|rs|py|pas|sql|ps1|md|json|conf|txt|toml|cfg|ini)$", val, re.I):
            continue
        # 右侧紧贴：锚点 end → token start
        if te <= start:
            gap = row_text[te:start]
            side = "L"
        elif ts >= end:
            gap = row_text[end:ts]
            side = "R"
        else:
            continue
        if len(gap) > JOIN_MAX:
            continue
        # 只允许「反引号 + **至多一个**连接标点」（如 `` `SYM`（`PATH:LINE`） ``），
        # 否则 gap 里会出现两个分隔符（如 `` `SYM_A`）、`PATH:LINE` `` —— 那是**前一条锚**
        # 的符号，不属于本条 ⇒ 判「抽不到」。
        nonbt = [ch for ch in gap if ch != "`"]
        if len(nonbt) > 1 or (nonbt and nonbt[0] not in JOIN_CHARS):
            continue
        # 归一：调用式 `f(x)` / `a::b()` 去掉尾部参数表。
        sym = re.sub(r"\([^)]*\)\s*$", "", val).strip()
        if not PLAIN_IDENT.match(sym) or len(sym) < 3:
            continue
        dist = len(gap)
        if best is None or dist < best[0]:
            best = (dist, sym, side)
    return best[1] if best else None


def _candidates(sym):
    """符号的等价写法（去掉 `::`/`.` 前缀，得到可匹配的裸名）。"""
    parts = {sym}
    if "::" in sym:
        parts.add(sym.split("::")[-1])
    if "." in sym:
        parts.add(sym.split(".")[-1])
    return {p for p in parts if len(p) >= 3}


def _candidates_filewide(sym):
    """判据 3 的「符号在文件里」判定用（与 `_candidates` 同口径，不拆出 Class 前缀段，
    避免 `Merchant`/`World` 这类泛化前缀到处命中）。"""
    return _candidates(sym)


def _entry_id(row_text):
    m = re.match(r"\s*\|\s*(T-\d+|D-\d+|B-\d+|A-\d+|M1-[A-Z0-9-]+|S4B-\d+)\s*\|", row_text)
    if m:
        return m.group(1)
    return "(散文)"


def parse_anchors(text):
    """⇒ [{line, path, ext, lines:[...], start, end, bare}]（按出现顺序）。

    同行裸续引用 `:NNN` 归到同一物理行里**前一个**完整锚点（同 `check_anchors.py` 判据 7 口径）。
    """
    # 物理行边界
    starts = [0]
    for i, ch in enumerate(text):
        if ch == "\n":
            starts.append(i + 1)

    def row_bounds(pos):
        ls = text.rfind("\n", 0, pos) + 1
        le = text.find("\n", pos)
        if le < 0:
            le = len(text)
        return ls, le

    anchors = []
    by_row = {}
    for m in ANCHOR_RE.finditer(text):
        path, ext, a, b, c = m.group(1), m.group(2).lower(), int(m.group(3)), \
            (int(m.group(4)) if m.group(4) else None), (int(m.group(5)) if m.group(5) else None)
        ls, le = row_bounds(m.start())
        rec = dict(line=text.count("\n", 0, m.start()) + 1,
                   path=path, ext=ext, lines=[a] + [x for x in (b, c) if x],
                   start=m.start(), end=m.end(), bare=False,
                   row=text[ls:le], row_start=ls,
                   entry=_entry_id(text[ls:le]))
        anchors.append(rec)
        by_row.setdefault(ls, []).append(rec)

    for m in BARE_RE.finditer(text):
        ls, le = row_bounds(m.start())
        prevs = [r for r in by_row.get(ls, []) if r["end"] <= m.start()]
        if not prevs:
            continue  # 散文里的历史值，不归属任何锚
        ref = max(prevs, key=lambda r: r["end"])
        # 只有**紧贴**在文件引用之后（中间只隔连接标点、长度 ≤ BARE_GAP）的裸续引用才归属它；
        # 否则多半是在用另一种写法（方法名，如 `Envirnoment.AddItemToMap`（`:88-92`））
        # 指另一个文件 ⇒ 不归属（不猜）。
        gap = text[ref["end"]:m.start()]
        if len(gap) > BARE_GAP or any(ch not in BARE_JOIN for ch in gap):
            continue
        nums = [int(m.group(1))] + [int(x) for x in (m.group(2), m.group(3)) if x]
        anchors.append(dict(line=text.count("\n", 0, m.start()) + 1,
                            path=ref["path"], ext=ref["ext"], lines=nums,
                            start=m.start(), end=m.end(), bare=True,
                            row=ref["row"], row_start=ref["row_start"],
                            entry=ref["entry"]))
    anchors.sort(key=lambda r: r["start"])
    return anchors


def check_doc_text(text, root=REPO_ROOT, whitelist_label="<mem>", idx=None,
                   high_churn=HIGH_CHURN):
    """纯函数（便于 selftest 注入改坏文本）⇒ (reds, infos, prereq)。

    reds   = [(code, msg)]
    infos  = [str]（EXTERNAL / WEAK 等不判红项）
    prereq = 非空 ⇒ rc=2（fail-closed）
    """
    reds, infos, prereq = [], [], []
    if idx is None:
        idx = _build_basename_index(root)

    hc_paths = [_norm(p) for p, _n, _w in high_churn]
    hc_basenames = {os.path.basename(p) for p in hc_paths}

    anchors = parse_anchors(text)
    for a in anchors:
        tok = _norm(a["path"])
        base = os.path.basename(tok)
        rel, reason = _resolve(a["path"], root, idx)

        # 判据 4：高 churn 文件禁行号锚（无论能否解析）。
        #   命中口径：① 解析到的仓库相对路径 ∈ HIGH_CHURN；或
        #   ② Rust 源码裸 basename（Rust 的裸名歧义是良性的——如 `main.rs` 只可能是
        #      db-srv(89 行)/game-svr(2 万行) 之一，而本表引用处皆是 game-svr；`mongen.rs`
        #      等同理）。`.cs` 的裸 basename（如 `AppService.cs` 仓内 12 处）**不**按
        #      裸名判（否则会误伤 DBSrv/GameSrv 的冻结文件）⇒ 只有路径解析到 HC 文件才命中。
        is_hc = (rel is not None and rel in hc_paths) or \
                (a["ext"] == "rs" and "/" not in tok and base in hc_basenames)
        if is_hc:
            reds.append(("ANCHOR_LINE_NOT_ALLOWED",
                         "%s：`%s:%s` 落在**高 churn 文件**上（symbol_unique 约定：共享增长文件"
                         "不得写行号锚，改用符号锚；行号必被别的车道推平）"
                         % (a["entry"], a["path"], a["lines"][0])))

        if rel is None:
            if reason == "external":
                if a["ext"] in SRC_EXTS:
                    reds.append(("ANCHOR_FILE_MISSING",
                                 "%s：`%s:%s` 是源码引用（.%s）却解析不到文件"
                                 % (a["entry"], a["path"], a["lines"][0], a["ext"])))
                else:
                    infos.append("%s EXTERNAL `%s:%s`（仓外件，无法机判）"
                                 % (a["entry"], a["path"], a["lines"][0]))
            else:  # ambiguous
                infos.append("%s AMBIGUOUS `%s:%s`（同名文件多处，跳过符号核）"
                             % (a["entry"], a["path"], a["lines"][0]))
            continue

        src = os.path.join(root, rel)
        if not os.path.isfile(src):
            prereq.append("监控/引用文件缺失：%s" % rel)
            continue
        lines = _read_lines(src)
        n = len(lines)
        bad = [x for x in a["lines"] if x < 1 or x > n]
        if bad:
            reds.append(("ANCHOR_OUT_OF_RANGE",
                         "%s：`%s:%s` 越界（%s 共 %d 行）"
                         % (a["entry"], rel, ",".join(str(x) for x in bad), rel, n)))
            continue
        if is_hc and a["lines"]:
            continue  # 已判 LINE_NOT_ALLOWED，不再叠符号核
        # 冻结 oracle 面（`src/**` 除工具侧 `src/BotSrv/**`）：C# oracle 源码不随开发增长
        # （判据仅 6 个 bins + src/BotSrv 是被多车道改的），其行号锚不会「随合并漂移」⇒
        # 只核「文件存在 + 行号在范围内」，符号核降级为 info（避免把「区域锚」——锚点落在
        # 被点名的函数**体内**——误判成漂移）。
        if rel.startswith("src/") and not rel.startswith("src/BotSrv/"):
            infos.append("%s FROZEN-ORACLE `%s:%s`（src/ 冻结面：只核文件 + 行号）"
                         % (a["entry"], rel, ",".join(str(x) for x in a["lines"])))
            continue
        # 判据 3：±3 行内命中上下文声明的符号（窗口覆盖**整个区间**，不只是端点）。
        sym = _adjacent_symbol(a["row"], a["start"] - a["row_start"],
                               a["end"] - a["row_start"], a["path"] + ":" + str(a["lines"][0]))
        if not sym:
            infos.append("%s WEAK `%s:%s`（抽不到紧贴符号，仅核文件存在 + 行号在范围内）"
                         % (a["entry"], rel, ",".join(str(x) for x in a["lines"])))
            continue
        lo, hi = min(a["lines"]), max(a["lines"])
        win = set()
        for i in range(max(1, lo - 3), min(n, hi + 3) + 1):
            win.add(lines[i - 1])
        wintxt = "\n".join(win)
        if any(c in wintxt for c in _candidates(sym)):
            continue  # OK
        filetxt = "\n".join(lines)
        if any(c in filetxt for c in _candidates_filewide(sym)):
            reds.append(("ANCHOR_DRIFT",
                         "%s：`%s:%s` ±3 行内不含声明的符号 `%s`（该符号在文件别处存在 ⇒ 行号漂移）"
                         % (a["entry"], rel, ",".join(str(x) for x in a["lines"]), sym)))
        else:
            # 2026-10-17 修正（复验者 must_fix）：原实现把「声明符号全文件不存在」一律记 WEAK info
            # ⇒ fail-open（把 `` `parse_disable_send_msg_list` `` 改成幽灵符号仍 ALL PASS）。
            # 现按形态分流：**标识符形态**（`Foo` / `Foo::Bar`）不存在 ⇒ fail-closed 判红；
            # 非标识符（中文、`[Setup]`、路径片段等散文性括注）⇒ 仍记 WEAK info。
            if re.match(r"^[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*$", sym):
                reds.append(("ANCHOR_SYMBOL_MISSING",
                             "%s：`%s:%s` 声明的符号 `%s` 在该文件里不存在（标识符形态 ⇒ fail-closed）"
                             % (a["entry"], rel, ",".join(str(x) for x in a["lines"]), sym)))
            else:
                infos.append("%s WEAK `%s:%s`（紧贴符号 `%s` 非标识符形态 ⇒ 视为上下文描述，跳过）"
                             % (a["entry"], rel, ",".join(str(x) for x in a["lines"]), sym))

    # 判据 4b：高 churn 文件的**符号锚**——紧贴的反引号符号必须真存在（防「符号被删」假绿）。
    for p, _n, _w in high_churn:
        for form in (p, os.path.basename(p)):
          for m in re.finditer(r"`%s`" % re.escape(form), text):
            ls, le = text.rfind("\n", 0, m.start()) + 1, text.find("\n", m.start())
            if le < 0:
                le = len(text)
            row = text[ls:le]
            sym = _adjacent_symbol(row, m.start() - ls, m.end() - ls, form)
            if not sym:
                continue
            src = os.path.join(root, p)
            if not os.path.isfile(src):
                continue
            if not any(c in "\n".join(_read_lines(src)) for c in _candidates_filewide(sym)):
                reds.append(("ANCHOR_SYMBOL_MISSING",
                             "高 churn 符号锚 `%s`（`%s`）：声明的符号在文件里不存在（fail-closed）"
                             % (p, sym)))

    # 判据 5：监控文件缺失 ⇒ PREREQ。
    for p, _n, _w in high_churn:
        if not os.path.isfile(os.path.join(root, p)):
            prereq.append("HIGH_CHURN 监控文件缺失：%s" % p)
    return reds, infos, prereq


def run_check(whitelist=DEFAULT_WHITELIST, root=REPO_ROOT, high_churn=HIGH_CHURN,
              json_out=None):
    if not os.path.isfile(whitelist):
        print("RESULT: FAILED")
        print("  PREREQ_RED whitelist.md 缺失：%s（fail-closed，绝不静默通过）" % whitelist)
        return 2
    with open(whitelist, "r", encoding="utf-8-sig") as fh:
        text = fh.read()
    reds, infos, prereq = check_doc_text(text, root=root, high_churn=high_churn)

    if prereq:
        print("RESULT: FAILED")
        for p in dict.fromkeys(prereq):
            print("  PREREQ_RED %s" % p)
        return 2

    show_info = os.environ.get("WHITELIST_ANCHOR_QUIET") != "1"
    if show_info:
        for i in infos:
            print("  info %s" % i)

    if reds:
        print("RESULT: FAILED")
        for code, msg in reds:
            print("  RED %s %s" % (code, msg))
        rc = 1
    else:
        print("RESULT: ALL PASS（%d 条锚：行号锚 ±3 命中声明符号；高 churn 文件无行号锚；"
              "仓外件登记 EXTERNAL）" % len(parse_anchors(text)))
        rc = 0
    if json_out:
        with open(json_out, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"reds": reds, "infos": infos, "prereq": prereq,
                       "anchors": len(parse_anchors(text))}, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return rc


def cmd_audit(args):
    with open(args.whitelist, "r", encoding="utf-8-sig") as fh:
        text = fh.read()
    idx = _build_basename_index(REPO_ROOT)
    anchors = parse_anchors(text)
    hc_paths = {_norm(p) for p, _n, _w in HIGH_CHURN}
    hc_bn = {os.path.basename(p) for p in hc_paths}
    buckets = {"internal": 0, "high_churn": 0, "external": 0, "ambiguous": 0}
    for a in anchors:
        tok = _norm(a["path"])
        rel, reason = _resolve(a["path"], REPO_ROOT, idx)
        if (rel is not None and _norm(rel) in hc_paths) or \
                (a["ext"] == "rs" and "/" not in tok and os.path.basename(tok) in hc_bn):
            buckets["high_churn"] += 1
        elif rel is not None:
            buckets["internal"] += 1
        elif reason == "ambiguous":
            buckets["ambiguous"] += 1
        else:
            buckets["external"] += 1
    print("ANCHOR_AUDIT whitelist=%s total=%d %s" % (
        args.whitelist, len(anchors), " ".join("%s=%d" % kv for kv in buckets.items())))
    for a in anchors:
        rel, reason = _resolve(a["path"], REPO_ROOT, idx)
        print("  L%-4d %-14s %-46s %s" % (
            a["line"], a["entry"], a["path"] + ":" + ",".join(str(x) for x in a["lines"]),
            rel or ("<%s>" % reason)))
    return 0


def _write(root, rel, content):
    p = os.path.join(root, rel)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(content)
    return p


def selftest():
    import shutil
    import tempfile

    checks = []

    def expect(name, cond, detail=""):
        checks.append((name, bool(cond), detail))

    real = DEFAULT_WHITELIST
    with open(real, "r", encoding="utf-8-sig") as fh:
        text = fh.read()

    # ① 真身：当前 whitelist.md + 当前源码必须全绿。
    rc_real = run_check(real)
    expect("real-whitelist-fresh", rc_real == 0, "rc=%d" % rc_real)

    # 用合成根（含少量真源文件）做改坏必红，避免依赖真仓库漂移。
    #   合成文件之一刻意**不在** HIGH_CHURN（`crates/data/src/lists.rs`），另一在
    #   （`mir2-rs/bins/game-svr/src/main.rs`），以便分别验证「行号锚 ±3」与「高 churn 禁行号」。
    with tempfile.TemporaryDirectory() as td:
        _write(td, "mir2-rs/crates/data/src/lists.rs",
               "pub fn parse_game_log_item_name_list() {}\n"
               "pub fn game_logs_item() {}\n"
               "pub fn load_game_log_item_name_list_file() {}\n"
               "pub fn floor_items_view_sorted() {}\n" + "// filler\n" * 20)
        _write(td, "mir2-rs/bins/game-svr/src/main.rs",
               "mod homemap;\nfn step_mon_gen() {}\nfn save_human_rcd() {}\n")
        # 合成 whitelist：一条正常行号锚（非高 churn + 紧贴符号）+ 一条高 churn 符号锚。
        good = (
            "| T-90 | 见 `floor_items_view_sorted`（`crates/data/src/lists.rs:4`） |\n"
            "| T-91 | 宿主 `step_mon_gen`（`mir2-rs/bins/game-svr/src/main.rs`） |\n"
        )

        # ② 真身合成 GREEN。
        reds, _i, _p = check_doc_text(good, root=td)
        expect("synth-good-green", not reds, "; ".join(m for _c, m in reds))

        # ③ 改坏必红①（判据 3）：行号锚偏 10 行 ⇒ ANCHOR_DRIFT，点名条目 T-90。
        bad_drift = good.replace("lists.rs:4", "lists.rs:14")
        reds, _i, _p = check_doc_text(bad_drift, root=td)
        expect("mutation-anchor-drift-red",
               any(c == "ANCHOR_DRIFT" and "T-90" in m for c, m in reds),
               "; ".join("%s %s" % (c, m) for c, m in reds))

        # ④ 改坏必红②（判据 4）：给高 churn 文件塞一个 `:line` 锚 ⇒ ANCHOR_LINE_NOT_ALLOWED。
        bad_hc = good + "| T-92 | 见 `step_mon_gen`（`mir2-rs/bins/game-svr/src/main.rs:2`） |\n"
        reds, _i, _p = check_doc_text(bad_hc, root=td)
        expect("mutation-highchurn-line-red",
               any(c == "ANCHOR_LINE_NOT_ALLOWED" and "T-92" in m for c, m in reds),
               "; ".join("%s %s" % (c, m) for c, m in reds))

        # ⑤ 改坏必红③（判据 1）：源码引用解析不到 ⇒ ANCHOR_FILE_MISSING。
        bad_missing = good + "| T-93 | 见 `crates/data/src/ghost.rs:1` |\n"
        reds, _i, _p = check_doc_text(bad_missing, root=td)
        expect("mutation-missing-file-red",
               any(c == "ANCHOR_FILE_MISSING" and "ghost.rs" in m for c, m in reds),
               "; ".join("%s %s" % (c, m) for c, m in reds))

        # ⑥ 改坏必红④（判据 2）：行号越界 ⇒ ANCHOR_OUT_OF_RANGE。
        bad_oor = good.replace("lists.rs:4", "lists.rs:400")
        reds, _i, _p = check_doc_text(bad_oor, root=td)
        expect("mutation-out-of-range-red",
               any(c == "ANCHOR_OUT_OF_RANGE" for c, _m in reds),
               "; ".join("%s %s" % (c, m) for c, m in reds))

        # ⑦ fail-closed：监控文件缺失 ⇒ PREREQ（rc≠0）。
        os.remove(os.path.join(td, "mir2-rs/bins/game-svr/src/main.rs"))
        reds, _i, prereq = check_doc_text(good, root=td)
        expect("mutation-monitored-file-missing-prereq",
               any("main.rs" in p for p in prereq),
               "; ".join(prereq))

    # ⑧ 改坏必红⑤（判据 5）：whitelist 缺失 ⇒ rc=2（不是静默通过）。
    with tempfile.TemporaryDirectory() as td:
        rc = run_check(os.path.join(td, "no_such_whitelist.md"))
        expect("mutation-whitelist-missing-rc2", rc == 2, "rc=%d" % rc)

    # ⑨ 真身边界：把真 whitelist 的一份高 churn 符号锚改回行号形式 ⇒ 必红（现场牙）。
    #    取真身文本里第一条 `bins/game-svr/src/main.rs` 的**符号锚**（无 :line）注入一个行号版本。
    mutated = text + "\n（改坏：`mir2-rs/bins/game-svr/src/main.rs:1`）\n"
    reds, _i, _p = check_doc_text(mutated)
    expect("real-mutation-mainrs-line-red",
           any(c == "ANCHOR_LINE_NOT_ALLOWED" and "main.rs" in m for c, m in reds),
           "; ".join("%s %s" % (c, m) for c, m in reds))

    ok = sum(1 for _n, c, _d in checks if c)
    total = len(checks)
    for n, c, d in checks:
        print("  %s %s%s" % ("ok " if c else "RED", n, ("（%s）" % d) if d and not c else ""))
    if ok != total:
        print("SELFTEST RED: whitelist_anchor_gate selftest %d/%d" % (ok, total))
        return 1
    print("SELFTEST GREEN: whitelist_anchor_gate selftest %d/%d（含 6 条改坏必红：行号锚偏 10 行 / "
          "高 churn 塞行号锚 / 源码引用文件缺失 / 行号越界 / 监控文件缺失 PREREQ / "
          "whitelist 缺失 rc=2）" % (ok, total))
    return 0


def main(argv=None):
    p = argparse.ArgumentParser(description="whitelist.md 行号锚保鲜门（只读、零端口）")
    sub = p.add_subparsers(dest="cmd", required=True)
    pc = sub.add_parser("check", help="核 whitelist.md 的全部 path:line 锚")
    pc.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pc.add_argument("--json", default=None)
    pa = sub.add_parser("audit", help="清点全部锚与分类（覆盖面）")
    pa.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    ps = sub.add_parser("selftest", help="合成文本 + 改坏必红的 fail-closed 自检")
    args = p.parse_args(argv)
    if args.cmd == "check":
        return run_check(args.whitelist, json_out=args.json)
    if args.cmd == "audit":
        return cmd_audit(args)
    return selftest()


if __name__ == "__main__":
    sys.exit(main())
