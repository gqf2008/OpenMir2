#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""whitelist_anchor_gate.py — `mir2-rs/tests/parity/whitelist.md` 行号锚保鲜门（fail-closed）。

卡：`openmir2-m3-whitelist-anchor-drift`（多车道共享增长文件上的行号锚注定腐烂）。

## 为什么有这道门

`whitelist.md` 是「行为差异登记」的唯一真源，里面用 `` `path:line` `` 形式引用源码锚点。
其中一批登记（`T-15`/`T-26`/`T-27`/`T-31`/`T-33`/`T-56`/`T-65` 等「已实现 / 退役」条）
的 file:line 是**对着 master 某一历史 tip（`b08e065fc`）**抄的；master 一直在动
（尤其 `bins/game-svr/src/main.rs`、`crates/world-bevy/src/{world,map,combat}.rs` 这些
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
# 外加任务显式点名的 `crates/world-bevy/src/npc.rs`（当天被多个车道改过；其改次 2，
# 但属同一 contention 区，保守纳入）。Rust 源 + 工具侧 `src/BotSrv/**`（T-1 登记的
# 工具例外，同样被多张夹具卡反复扩展，10 天 20 次）。
# ---------------------------------------------------------------------------
HIGH_CHURN = (
    # (仓库根相对路径, 10 天改次, 说明)
    # 2026-10-21 单二进制搬迁：宿主代码整体 `git mv` 到 lib.rs（main.rs 只剩三行薄包装），
    # 高 churn 监控与符号锚一律跟到 lib.rs。
    ("mir2-rs/bins/game-svr/src/lib.rs", 150, "game-svr 宿主（多车道增长，搬迁前为 main.rs；行号锚实测漂 ≥6 次）"),
    ("mir2-rs/crates/world-bevy/src/world.rs", 39, "世界模拟主模块"),
    ("mir2-rs/crates/world-bevy/src/entity.rs", 21, "实体层"),
    ("mir2-rs/crates/world-bevy/src/combat.rs", 13, "战斗数值"),
    ("mir2-rs/crates/world-bevy/src/monai.rs", 10, "怪 AI tick"),
    ("mir2-rs/bins/game-svr/src/useitem.rs", 8, "CM_EAT 面"),
    ("mir2-rs/crates/world-bevy/src/map.rs", 6, "地图格/门/地面物"),
    ("mir2-rs/bins/game-svr/src/mongen.rs", 5, "MonGen 刷怪 sink"),
    ("mir2-rs/bins/game-svr/src/npc.rs", 5, "NPC 宿主角（实测 10 天 5 次 / 30 天 45 次；A-14/T-207 锚两次被顶走）"),
    ("mir2-rs/bins/game-svr/src/dbclient.rs", 5, "存档 DB 客户端"),
    ("mir2-rs/bins/game-svr/src/smframe.rs", 13, "出站编码器面（本文件 13 次/10 天；`smframe*.rs` 合计 22。2026-10-27 turn-order 的 +150/−46 把 T-206 锚顶走 69 行）"),
    ("mir2-rs/crates/world-bevy/src/npc.rs", 2, "固定 NPC tick（任务点名，contention 区）"),
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
        # 2026-10-21 修：原来按**绝对** dirpath 判跳过，root 自身在 `.worktrees/` 下时会把
        # 整棵树都跳过（索引为空 ⇒ 裸 basename 全判 ANCHOR_FILE_MISSING）。按**相对**路径判，
        # 才既能跳过嵌套的 .git/.worktrees，又能在 worktree 里正常工作。
        parts = _norm(os.path.relpath(dirpath, root)).split("/")
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



_DECL_KEYWORDS = r"fn|struct|enum|trait|mod|const|static|type|union|macro_rules"


def _strip_line_comments(filetxt):
    """去掉 `//` 之后的注释（行注释整段），保留换行。

    review `4c74851f` 的第二条 fail-open 通道：注释里的 [`sm_skeleton`] 让符号锚假装还在。
    """
    out = []
    for ln in filetxt.split(chr(10)):
        i = ln.find("//")
        out.append(ln if i < 0 else ln[:i])
    return chr(10).join(out)


def _declared_in(filetxt, name):
    """符号是否以**完整标识符**出现在（去掉注释后的）文件里（判据 4b）。

    review `4c74851f`：原实现是**原始子串**匹配 ⇒ 两条 fail-open 通道：
    ① 同前缀标识符（`fn sm_skeleton_frame()` 兜住 `sm_skeleton`）；
    ② 注释（`[`sm_skeleton`]`）。⇒ 先去注释，再按「完整标识符」匹配（前后不得是
    标识符字符）。这样既堵住两条通道，又不误伤对**字段/表达式/变体**的合法引用。
    """
    edge = "(?:^|[^A-Za-z0-9_])"
    tail = "(?:[^A-Za-z0-9_]|$)"
    pat = edge + re.escape(name) + tail
    return re.search(pat, _strip_line_comments(filetxt)) is not None


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
        # 2026-11-21（QA）：白名单里的路径常写作 `bins/game-svr/...`（相对 `mir2-rs/`），
        # 此前只匹配仓库根相对/裸文件名 ⇒ 本规则对多数实际锚**从未生效**
        # （负例：把 `merchant_say_gbk` 改成 `merchant_say_gbkXX` 仍 ALL PASS）。
        _forms = [p, os.path.basename(p)]
        if p.startswith("mir2-rs/"):
            _forms.append(p[len("mir2-rs/"):])
        for form in _forms:
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
            _ftxt = chr(10).join(_read_lines(src))
            if not any(_declared_in(_ftxt, c) for c in _candidates_filewide(sym)):
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
    #   （`mir2-rs/bins/game-svr/src/lib.rs`；单二进制搬迁后宿主代码在此，main.rs 只剩薄包装），
    #   以便分别验证「行号锚 ±3」与「高 churn 禁行号」。
    with tempfile.TemporaryDirectory() as td:
        _write(td, "mir2-rs/crates/data/src/lists.rs",
               "pub fn parse_game_log_item_name_list() {}\n"
               "pub fn game_logs_item() {}\n"
               "pub fn load_game_log_item_name_list_file() {}\n"
               "pub fn floor_items_view_sorted() {}\n" + "// filler\n" * 20)
        _write(td, "mir2-rs/bins/game-svr/src/lib.rs",
               "mod homemap;\nfn step_mon_gen() {}\nfn save_human_rcd() {}\n")
        # 合成 whitelist：一条正常行号锚（非高 churn + 紧贴符号）+ 一条高 churn 符号锚。
        good = (
            "| T-90 | 见 `floor_items_view_sorted`（`crates/data/src/lists.rs:4`） |\n"
            "| T-91 | 宿主 `step_mon_gen`（`mir2-rs/bins/game-svr/src/lib.rs`） |\n"
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
        bad_hc = good + "| T-92 | 见 `step_mon_gen`（`mir2-rs/bins/game-svr/src/lib.rs:2`） |\n"
        reds, _i, _p = check_doc_text(bad_hc, root=td)
        expect("mutation-highchurn-line-red",
               any(c == "ANCHOR_LINE_NOT_ALLOWED" and "T-92" in m for c, m in reds),
               "; ".join("%s %s" % (c, m) for c, m in reds))

        # ④b 改坏必红（判据 4b，review `4c74851f` 的两颗牙）：高 churn 符号锚的
        # 符号整体改名 ⇒ 必须红（原实现是子串匹配，被 `fn sm_skeleton_frame()` 兜住）。
        # 用独立合成根：lib.rs 里**只剩**同前缀的 `fn step_mon_gen_frame()`
        # （真实事故形态：`sm_skeleton` 被改名后，`fn sm_skeleton_frame()` 兜住了它）——
        # 子串匹配会放行，完整标识符匹配必须红。
        with tempfile.TemporaryDirectory() as td_b:
            _write(td_b, "mir2-rs/bins/game-svr/src/lib.rs",
                   "fn step_mon_gen_frame() {}" + chr(10))
            bad_renamed = ("| T-94 | 宿主 `step_mon_gen`（`mir2-rs/bins/game-svr/src/lib.rs`） |" + chr(10))
            reds, _i, _p = check_doc_text(bad_renamed, root=td_b)
            expect("mutation-highchurn-symbol-renamed-red",
                   any(c == "ANCHOR_SYMBOL_MISSING" and "step_mon_gen" in m for c, m in reds),
                   "; ".join("%s %s" % (c, m) for c, m in reds))

        # ④c 改坏必红（判据 4b 第二颗）：符号是另一个标识符的**前缀**（`step_mon` 在
        # `step_mon_gen` 里面）—— 子串匹配会放行，完整标识符匹配必须红。
        bad_prefix = good.replace("宿主 `step_mon_gen`", "宿主 `step_mon`")
        reds, _i, _p = check_doc_text(bad_prefix, root=td)
        expect("mutation-highchurn-symbol-prefix-red",
               any(c == "ANCHOR_SYMBOL_MISSING" and "step_mon" in m for c, m in reds),
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
        os.remove(os.path.join(td, "mir2-rs/bins/game-svr/src/lib.rs"))
        reds, _i, prereq = check_doc_text(good, root=td)
        expect("mutation-monitored-file-missing-prereq",
               any("lib.rs" in p for p in prereq),
               "; ".join(prereq))

    # ⑧ 改坏必红⑤（判据 5）：whitelist 缺失 ⇒ rc=2（不是静默通过）。
    with tempfile.TemporaryDirectory() as td:
        rc = run_check(os.path.join(td, "no_such_whitelist.md"))
        expect("mutation-whitelist-missing-rc2", rc == 2, "rc=%d" % rc)

    # ⑨ 真身边界：把真 whitelist 的一份高 churn 符号锚改回行号形式 ⇒ 必红（现场牙）。
    #    取真身文本里第一条 `bins/game-svr/src/lib.rs` 的**符号锚**（无 :line）注入一个行号版本。
    mutated = text + "\n（改坏：`mir2-rs/bins/game-svr/src/lib.rs:1`）\n"
    reds, _i, _p = check_doc_text(mutated)
    expect("real-mutation-mainrs-line-red",
           any(c == "ANCHOR_LINE_NOT_ALLOWED" and "lib.rs" in m for c, m in reds),
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



# ---------------------------------------------------------------------------
# Phase 0（2026-11-21，QA）：冻结面 `src/**` 锚的**声明级**漂移报告（只报告，不判红）。
# 方案 A（声明级窗口匹配）+ B（资格过滤）；默认 `check` 行为不变。
# ---------------------------------------------------------------------------
def _decl_hit(lines, lo, hi, sym):
    """±3 行窗口内是否存在**声明形态**的 `sym`（C# 方法/属性/字段/枚举成员；Rust fn/struct/…）。"""
    cands = _candidates(sym)
    win = lines[max(0, lo - 4):min(len(lines), hi + 3)]
    cs_mods = re.compile(r"\b(?:public|private|protected|internal|static|override|virtual|async|sealed|partial|void|bool|int|uint|ushort|short|byte|string|object|Task|List)\b")
    for line in win:
        for c in cands:
            e = re.escape(c)
            if re.search(r"\b(?:class|struct|enum|interface)\s+%s\b" % e, line): return True
            if re.search(r"\b(?:fn|struct|enum|const|static|type|trait|mod)\b[^\n]*\b%s\b" % e, line): return True
            if re.search(r"\b%s\s*\(" % e, line) and cs_mods.search(line): return True
            if re.search(r"\b%s\s*[{=;]" % e, line): return True
            if re.search(r"\b%s\s*,\s*$" % e, line): return True
            if re.search(r"\b%s\s*[:(]" % e, line) and ("fn " in line or "let " in line): return True
    return False

def _loose_dist(lines, lo, hi, sym):
    """松口径：任意出现（子串）到锚区间的最近行距；文件内不存在 ⇒ None。"""
    cands = _candidates(sym)
    best = None
    for i, line in enumerate(lines, 1):
        if any(c in line for c in cands):
            d = min(abs(i - x) for x in range(lo, hi + 1))
            if best is None or d < best: best = d
    return best

def _qualified(sym, lines):
    """B：带限定前缀 / 文件内唯一 / CamelCase≥2 词或长度≥6 / 全大写常量 ⇒ 合格。"""
    if "::" in sym or "." in sym: return True
    n = sum(1 for l in lines if re.search(r"\b%s\b" % re.escape(sym), l))
    if n <= 1: return True
    if sym.isupper() and len(sym) >= 4: return True
    humps = sum(1 for ch in sym[1:] if ch.isupper())
    return humps >= 1 and len(sym) >= 6



def _strip_code_noise(line):
    """去掉行内字符串字面量与 `//` 注释（用于括号计数与"字符串内标识符"判定）。"""
    out = []
    i = 0
    n = len(line)
    in_str = None
    while i < n:
        ch = line[i]
        if in_str:
            if ch == "\\" and i + 1 < n:
                i += 2
                continue
            if ch == in_str:
                in_str = None
            i += 1
            continue
        if ch in ("\"", "'"):
            in_str = ch
            i += 1
            continue
        if ch == "/" and i + 1 < n and line[i + 1] == "/":
            break
        out.append(ch)
        i += 1
    return "".join(out)


def _decl_spans(lines, cands):
    """返回 `[(decl_line, span_end_line)]`：声明行 + 由**配对花括号**得到的体结束行。

    2026-11-21（`10` 抽样后）：span 用花括号配平（不看缩进）；覆盖嵌套块 / 本地函数 /
    属性 getter（`Sym {` 或 `Sym => …` 前的块）。只认声明形态，不认调用行。
    """
    spans = []
    for idx, raw in enumerate(lines, 1):
        code = _strip_code_noise(raw)
        matched = False
        for c in cands:
            e = re.escape(c)
            if re.search(r"\b(?:public|private|protected|internal|static|override|virtual|async|sealed|partial|extern|new|unsafe)\b[^\n]*\b%s\s*\(" % e, code) \
               or re.search(r"\b(?:class|struct|enum|interface|fn|trait|mod)\s+%s\b" % e, code) \
               or re.search(r"\b%s\s*\{" % e, code) \
               or re.search(r"^\s*(?:[A-Za-z_][\w<>\[\],\.]*\s+)+%s\s*\(" % e, code):
                matched = True
                break
        if not matched:
            continue
        depth = 0
        started = False
        end_line = idx
        for j in range(idx - 1, len(lines)):
            cj = _strip_code_noise(lines[j])
            for ch in cj:
                if ch == "{":
                    depth += 1
                    started = True
                elif ch == "}" and started:
                    depth -= 1
                    if depth == 0:
                        end_line = j + 1
                        break
            if started and depth == 0:
                break
            if not started and j > idx + 2:   # 声明后 3 行内仍无 `{` ⇒ 不是带体的声明（如接口/调用）
                break
        if started and depth == 0:
            spans.append((idx, end_line))
    return spans

def _in_decl_span(spans, lo, hi):
    for d, end in spans:
        if d <= lo and hi <= end:
            return True
    return False


def _only_in_strings(lines, tok):
    """`tok` 是否只出现在字符串字面量里（DB/SQL 表名列名等）。"""
    outside = any(tok in _strip_code_noise(l) for l in lines)
    inside = any(tok in l for l in lines)
    return inside and not outside

def _pair_symbol_v2(row, start, end, anchor_text, target_text, target_lines=None):
    """Phase 0 修订（2026-11-21）：按 `10` 的四条规则配对锚的声明符号。

    ① 排除泛名：`*Exception` 不作符号；`SM_*/RM_*/CM_*` 视为帧/消息，不参与符号匹配。
    ② 否定/对照引用：符号前 15 字内出现「无/缺/不进/未/not」⇒ 判否定引用，不配符号。
    ③ 语言一致：候选符号必须**出现在锚的目标文件里**（同语言）；Rust 名配 .cs 锚等跨语言句
       自然落空 ⇒ 返回 None（WARN，不 RED）。
    ④ 同句多锚：在"距离 + 语言"双条件下取最近合格候选。
    """
    best = None
    for ts, te, val in _tokens_in(row):
        if val == anchor_text or re.search(r":\d", val): continue
        if re.search(r"\.(cs|rs|py|pas|sql|ps1|md|json|conf|txt|toml|cfg|ini)$", val, re.I): continue
        if te <= start: gap = row[te:start]
        elif ts >= end: gap = row[end:ts]
        else: continue
        if len(gap) > JOIN_MAX: continue
        nonbt = [ch for ch in gap if ch != "`"]
        if len(nonbt) > 1 or (nonbt and nonbt[0] not in JOIN_CHARS): continue
        sym = re.sub(r"\([^)]*\)\s*$", "", val).strip()
        if not PLAIN_IDENT.match(sym) or len(sym) < 3: continue
        if re.search(r"Exception$", sym): continue
        if re.match(r"^(SM|RM|CM)_", sym): continue
        # 规则 2 精化（2026-11-21）：否定词须**紧邻符号**——否定词之后到符号之间
        # 不得再出现别的字母/数字（否则如"缺 657**：oracle `Sym`"会被误判成否定）。
        pre = row[max(0, ts - 15):ts]
        neg = None
        for w in ("不进", "无", "缺", "未", "not"):
            k = pre.rfind(w)
            if k >= 0 and (neg is None or k > neg[0]):
                neg = (k, w)
        if neg is not None and not re.search(r"[A-Za-z0-9]", pre[neg[0] + len(neg[1]):]):
            continue
        if not any(c in target_text for c in _candidates_filewide(sym)): continue
        # 规则 6（2026-11-21，10 抽样）：DB/SQL 标识符不参与代码符号匹配——
        # 点号全名且只出现在字符串字面量里（表名/列名/GetByte 参数）⇒ 跳过。
        if "." in sym and target_lines is not None:
            # 规则 6 扩展（10 抽样）：`表.列` 形态——左半段（表名）只在字符串里出现 ⇒ SQL 标识符，跳过。
            left = sym.split(".", 1)[0]
            if _only_in_strings(target_lines, sym) or _only_in_strings(target_lines, left):
                continue
        dist = len(gap)
        if best is None or dist < best[0]:
            best = (dist, sym)
    return best[1] if best else None

def cmd_report_src_drift(args):
    root = REPO_ROOT
    text = open(args.whitelist, encoding="utf-8").read()
    idx = _build_basename_index(root)
    rows = []
    for a in parse_anchors(text):
        rel, _reason = _resolve(a["path"], root, idx)
        if not rel or not rel.startswith("src/"): continue
        lines = _read_lines(os.path.join(root, rel))
        sym = _pair_symbol_v2(a["row"], a["start"] - a["row_start"], a["end"] - a["row_start"], a["path"], chr(10).join(lines), lines)
        lo, hi = min(a["lines"]), max(a["lines"])
        if not sym:
            bucket, dist, strict = "无相邻符号", None, False
        elif not _qualified(sym, lines):
            bucket, dist, strict = "符号不合格(通用名)", None, False
        elif _in_decl_span(_decl_spans(lines, _candidates(sym)), lo, hi):
            # 规则 5（2026-11-21，10 抽样）：锚落在该声明的方法体内 ⇒ 命中（dist=0）；
            # 否则才量距离——markdown 引的多是"某方法里的这一句"，按到声明行距离判漂会把常态判成异常。
            strict = True
            dist = 0
            bucket = "严格命中"
        else:
            strict = False
            dist = _loose_dist(lines, lo, hi, sym)
            if dist is None: bucket = "已漂出"
            elif dist == 0: bucket = "松命中·非声明"
            elif dist <= 3: bucket = "±3 边缘"
            else: bucket = "已漂出"
        rows.append(dict(entry=a["entry"], line=a["line"], path=rel, lines=a["lines"], sym=sym,
                         bucket=bucket, dist=dist, strict=strict))
    from collections import Counter
    cnt = Counter(r["bucket"] for r in rows)
    print("SRC_ANCHOR_REPORT total=%d %s" % (len(rows), dict(cnt)))
    if args.json:
        json.dump(rows, open(args.json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    if args.tsv:
        with open(args.tsv, "w", encoding="utf-8", newline="\n") as f:
            f.write("bucket\tentry\tpath\tlines\tsym\tdist\n")
            for r in rows:
                f.write("{}\t{}\t{}\t{}\t{}\t{}\n".format(
                    r["bucket"], r["entry"], r["path"],
                    "/".join(map(str, r["lines"])), r["sym"] or "",
                    "" if r["dist"] is None else r["dist"]))
    return 0



def cmd_report_selftest(argv_args=None):
    """Phase 0 检测器的**合成夹具**回归（不依赖真实 src/，文件会漂、夹具不会）。

    正例：锚在方法体内 ⇒ span 命中（dist=0）；反向牙：锚在方法外 5 行 ⇒ dist>0（真漂）。
    另含：属性 getter / 本地函数 / SQL 字符串 / 否定引用 / `*Exception`/`SM_*` 排除。
    """
    ok = True
    def check(name, cond, detail=""):
        nonlocal ok
        print(("ok  " if cond else "FAIL") + " " + name + (" | " + detail if detail and not cond else ""))
        ok = ok and cond

    body = [
        "class Demo {",                         # 1
        "    public void Body() {",              # 2
        "        int a = 1;",                    # 3
        "        int b = 2;",                    # 4
        "        void Local() {",                # 5
        "            int c = 3;",                # 6
        "        }",                             # 7
        "    }",                                 # 8
        "    public int P { get { return 1; } }",# 9
        "    public void Other() {",             # 10
        "        int d = 4;",                    # 11
        "    }",                                 # 12
        "}",                                     # 13
    ]
    spans = _decl_spans(body, _candidates("Body"))
    check("span: Body 体 = 2..8", spans and spans[0] == (2, 8), str(spans))
    check("span 正例: 锚在体内(4) ⇒ 命中", _in_decl_span(spans, 4, 4))
    check("**反向牙**: 锚在方法外 5 行(11 in Other) ⇒ 不命中", not _in_decl_span(spans, 11, 11))
    d = _loose_dist(body, 11, 11, "Body")
    check("**反向牙**: 方法外 dist > 3（真漂，不是 0）", d is not None and d > 3, str(d))
    spans_l = _decl_spans(body, _candidates("Local"))
    check("本地函数也算 span（5..7 命中）", _in_decl_span(spans_l, 6, 6))
    body2 = body[:8] + ["    public int Prop { get { return 1; } }"] + body[9:]
    spans_p = _decl_spans(body2, _candidates("Prop"))
    check("属性 getter 也算 span（9 命中）", _in_decl_span(spans_p, 9, 9))

    sql_line = '            var x = Db.GetByte("SELECT Level FROM characters_ablity WHERE ID=@ID");'
    check("SQL: 表名只在字符串里 ⇒ 跳过", _only_in_strings([sql_line], "characters_ablity"))
    check("SQL: `表.列` 左半段在字符串里 ⇒ 跳过", _only_in_strings(['x = Db.GetByte("SELECT characters_ablity.Level FROM characters_ablity");'], "characters_ablity"))
    neg_row = "| T-1 | 本宿主**无** `WeightChanged`（`src/X.cs:1`） |"
    sym = _pair_symbol_v2(neg_row, neg_row.find("`src"), neg_row.find("`）") + 1, "src/X.cs", "class X{}", ["class X{}"])
    check("否定引用 ⇒ 不配符号", sym is None, str(sym))
    exc_row = "| T-2 | 抛 `IndexOutOfRangeException`（`src/X.cs:1`） |"
    sym2 = _pair_symbol_v2(exc_row, exc_row.find("`src"), exc_row.find("`）") + 1, "src/X.cs", "class X{}", ["class X{}"])
    check("`*Exception` 不作符号", sym2 is None, str(sym2))
    print("REPORT_SELFTEST " + ("GREEN" if ok else "RED"))
    return 0 if ok else 1

def main(argv=None):
    p = argparse.ArgumentParser(description="whitelist.md 行号锚保鲜门（只读、零端口）")
    sub = p.add_subparsers(dest="cmd", required=True)
    pc = sub.add_parser("check", help="核 whitelist.md 的全部 path:line 锚")
    pc.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pc.add_argument("--json", default=None)
    pr = sub.add_parser("report-src-drift", help="Phase0：冻结面 src 锚的声明级漂移报告（只报告不判红）")
    pr.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pr.add_argument("--json", default=None)
    pr.add_argument("--tsv", default=None)
    pa = sub.add_parser("audit", help="清点全部锚与分类（覆盖面）")
    pa.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    prs = sub.add_parser("report-selftest", help="Phase0 检测器合成夹具回归（含反向牙，不依赖真实 src/）")
    ps = sub.add_parser("selftest", help="合成文本 + 改坏必红的 fail-closed 自检")
    args = p.parse_args(argv)
    if args.cmd == "check":
        return run_check(args.whitelist, json_out=args.json)
    if args.cmd == "audit":
        return cmd_audit(args)
    if args.cmd == "report-src-drift":
        return cmd_report_src_drift(args)
    if args.cmd == "report-selftest":
        return cmd_report_selftest()
    return selftest()


if __name__ == "__main__":
    sys.exit(main())
