#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""check_anchors.py — 场景两相 trace「承接卡」规格文档的**源码锚点保鲜**机判。

卡：`openmir2-m3-rng-trace-scene-groups`（2026-10-13 窗口前置审计）。

## 为什么需要它

`DRIVER-CONTRACT.md` 是承接卡 `openmir2-m3-rng-scene-driver` 的**规格**，里面用
`` `path:line` `` 形式引用源码锚点（宿主 AI 调用点 / 世界层 tick / oracle Wondering）。
master 一直在动 ⇒ 行号会**静默失准**：实测 `bins/game-svr/src/main.rs` 因 ④-D 与
client-e2e 两次合并而行号增长，`enable_production_monster_ai` 调用点从 `:5229` 漂到 `:5440`，
而文档仍写 `:5229`（承接卡按旧行号找代码会找错，且没人拦）。

本工具把「文档里写的行号 = 该符号在**当前源码**里的**首个**匹配行」钉成 fail-closed 机判；
它**不写任何文件**（`selftest` 只用内存文本 + 临时锚点表）。

## 判据（任一条不成立 ⇒ RED，逐条点名）

1. 文档里 `` `<literal>:<line>` ``（或该锚点的**仓库根相对路径**形式）**缺失 / 重复 /
   行号非数字** ⇒ `ANCHOR_MISSING` / `ANCHOR_AMBIGUOUS`；
2. 解析出的源文件不存在 ⇒ `ANCHOR_FILE_MISSING`；
3. 期望符号在文件里**不存在** ⇒ `ANCHOR_SYMBOL_MISSING`；
4. 期望符号的**首个**匹配行 ≠ 文档写的行号 ⇒ `ANCHOR_DRIFT`（点名实际行，即要改成的值）；
5. **闭世界**：文档里出现的**每个** `<path>.<ext>:<line>` 引用都必须有对应锚点
   （`ANCHORS` 登记），否则 ⇒ `UNGUARDED_ANCHOR`；扩展名不在白名单（`KNOWN_EXTS`）时
   ⇒ `UNKNOWN_REF_EXT`（旧口径只认 6 个扩展名，别的**静默跳过** ⇒ 可静默漂移）。
6. **同名 basename 不得串台**：带目录的引用（如 `foo/monai.rs:5`）必须与某锚点的
   `literal` / `path` **全等**；只有显式 `basename_ok=True` 的锚点才接受裸 basename
   （`monai.rs` / `mongen.rs`）。旧口径用 `token.endswith("/" + literal)` ⇒
   **别的目录下的同名文件**会被误判 COVERED，既非 `UNGUARDED_ANCHOR` 也不 `ANCHOR_DRIFT`（静默）。
7. **同行裸续引用** `:NN`（例：`…consumer_groups.tsv:38`（折叠理由）、`:57`（spawn 行））
   归到**同一物理行里前一个**完整引用，并按锚点 `also` 逐条校验：
   未登记 ⇒ `BARE_REF_UNDECLARED`；越界 ⇒ `BARE_REF_OUT_OF_RANGE`；
   行内容不匹配 ⇒ `BARE_REF_DRIFT`；归属的文件不存在 ⇒ `BARE_REF_FILE_MISSING`。
   （旧口径完全无视裸 `:NN` ⇒ 它指向的行可静默漂移。）
8. **符号唯一性锚**（`symbol_unique=True`，2026-10-14 **根因**收口）：对**多车道共享的增长文件**
   （现为 `bins/game-svr/src/lib.rs`——2026-10-21 单二进制搬迁后宿主代码在 lib.rs，`main.rs` 只剩
   三行薄包装；搬迁前该行号锚实测已漂 6 次 `5229→5440→5488→5598→5665→5795`，每次都是别的车道
   往宿主文件插代码把文档钉的行号推平）**不再做行号锚**：文档只引用该文件路径、
   **不得**再写 `path:<line>`（写了 ⇒ `ANCHOR_LINE_NOT_ALLOWED`），机判改为「期望符号在该文件里
   **恰好出现一次**」——**按匹配总次数计数**（不是「匹配行数」：同一物理行写两遍符号也算两次），
   0 次 ⇒ `ANCHOR_SYMBOL_MISSING`、>1 次 ⇒ `ANCHOR_AMBIGUOUS`。
   收益：① 别的车道往宿主文件插代码不再制造「合并即假 RED」的行号漂移；
   ② 宿主 AI 调用点被删 / 被复制仍 fail-closed 必红（判据不降级，只是把不稳定的行号换成稳定的唯一性）。

> 收录自 2026-10-14 reviewer-5 M-1 / reviewer-6 M-1+M-2（均为对
> `a173eba89`「锚点闭世界」片的同族非阻塞 Minor）的收口。

## 用法

    python -I check_anchors.py check       # 检查本目录 DRIVER-CONTRACT.md（默认）
    python -I check_anchors.py check --doc <path>   # 检查任意副本（用于「改前/改后」对照）
    python -I check_anchors.py selftest    # check + 改坏必红（含同名目录串台 / 裸 :NN / 未知扩展名）

退出码：0 = GREEN，1 = RED。
"""
import bisect
import os
import re
import sys
import tempfile

DOC_NAME = "DRIVER-CONTRACT.md"

# 本文件：<repo>/mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py
HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", "..", ".."))

# ---------------------------------------------------------------------------
# 可被保鲜的扩展名白名单。文档里出现 `path.<ext>:<line>` 且 ext 不在白名单 ⇒
# `UNKNOWN_REF_EXT` fail-closed（旧口径 `_DOC_REF_RE` 只认 6 个扩展名，
# 不在其内的引用**既不算覆盖、也不报错** ⇒ 静默漂移的通道）。
# ---------------------------------------------------------------------------
KNOWN_EXTS = frozenset({
    "rs", "cs", "py", "ps1", "psm1", "json", "jsonl", "tsv", "csv",
    "txt", "md", "toml", "yaml", "yml", "ini", "cfg", "xml", "sql",
    "sh", "bat",
})

# ---------------------------------------------------------------------------
# 锚点表（唯一真源在这里；文档必须与它同口径）
#
#   literal      = 文档里出现的 `path` 字面（或该锚点的仓库根相对路径）
#   path         = 仓库根相对路径（用于打开源码）
#   rx           = 该行必须匹配的**符号正则**；要求「首个匹配行 == 文档行号」
#   why          = 这个锚点为什么承重（给 RED 输出用）
#   basename_ok  = 允许文档用**裸 basename**（无目录）引用本锚点
#   symbol_unique= True ⇒ **不做行号锚**（共享增长文件）：文档只写文件路径、不得带 `:line`，
#                  机判 = `rx` 在该文件里**恰好匹配 1 次**（按匹配总次数计，不是行数；
#                  0 ⇒ ANCHOR_SYMBOL_MISSING，>1 ⇒ ANCHOR_AMBIGUOUS）。见判据 8。
#   also         = 同一 literal 的**同行裸续引用** `:NN` 的登记表：
#                  {line, rx, why}；未登记 ⇒ BARE_REF_UNDECLARED
# ---------------------------------------------------------------------------
ANCHORS = (
    # [symbol_unique] 宿主 AI 调用点落在**多车道共享的增长文件**上：行号锚实测已漂 6 次
    # （5229→5440→5488→5598→5665→5795，每次都是别的车道插代码推平）⇒ 改做符号唯一性锚，
    # 从根上不再产生「合并即假 RED」，同时保留「调用点被删 / 被复制 ⇒ 必红」的半导体牙。
    # 2026-10-21 单二进制搬迁：宿主代码在 `lib.rs`（`main.rs` 只剩三行薄包装）⇒ 锚点跟到 lib.rs。
    dict(literal="mir2-rs/bins/game-svr/src/lib.rs",
         path="mir2-rs/bins/game-svr/src/lib.rs",
         rx=r"enable_production_monster_ai\(&mut world\)",
         why="宿主无条件开怪 AI 调用点（本卡要 AI 开）",
         symbol_unique=True),
    dict(literal="mongen.rs",
         path="mir2-rs/bins/game-svr/src/mongen.rs",
         rx=r"pub fn enable_production_monster_ai\(",
         why="宿主 AI 开关接缝",
         basename_ok=True),
    dict(literal="crates/world/src/monai.rs",
         path="mir2-rs/crates/world/src/monai.rs",
         rx=r"pub fn monster_ai_tick\(",
         why="World::tick 的怪 AI 槽位驱动"),
    dict(literal="monai.rs",
         path="mir2-rs/crates/world/src/monai.rs",
         rx=r"pub fn wondering\(",
         why="Wondering 的 Random(20) 抽数入口",
         basename_ok=True),
    dict(literal="src/M2Server/Actor/AnimalObject.cs",
         path="src/M2Server/Actor/AnimalObject.cs",
         rx=r"protected virtual void Wondering",
         why="oracle Wondering 锚（冻结面，仅防误抄）"),
    dict(literal="rng-trace-probe/rng_consumer_groups.py",
         path="mir2-rs/tests/parity/m3/rng-trace-probe/rng_consumer_groups.py",
         rx=r"^FOLD_RULES = ",
         why="R-6 折叠映射定义（文档用 :line 指向 FOLD_RULES；本片真身：同片 +5 行使它从 :52 漂到 :57）"),
    dict(literal="rng-trace-probe/consumer_groups.tsv",
         path="mir2-rs/tests/parity/m3/rng-trace-probe/consumer_groups.tsv",
         rx=r"FOLD_RULES = \{construct: spawn\}",
         why="站点表的 R-6 折叠理由行",
         # 同一 literal 的**同行裸续引用**必须在此登记（旧口径不校验裸 :NN ⇒ 可静默漂移）。
         also=(dict(line=57,
                    rx=r"^spawn\trust\tcrates/world/src/mongen",
                    why="Rust spawn 行（表内唯一 rust spawn 站点；固定场景唯一 spawn 来源）"),)),
)


def _norm(tok):
    """Windows 反斜杠路径归一成 `/`（文档/表里两种写法都可能出现）。"""
    return tok.replace("\\", "/")


def _anchor_re(a):
    """`` `<literal|path>:<line>` ``（负向环视防止短 basename 命中长路径的尾部）。"""
    alts = sorted({_norm(a["literal"]), _norm(a["path"])}, key=len, reverse=True)
    pat = "|".join(re.escape(x) for x in alts)
    return re.compile(r"(?<![A-Za-z0-9_./\\-])(?:" + pat + r"):(\d+)")


def _read_lines(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read().splitlines()


def _resolve_ref(token, anchors):
    """把文档里的引用 token 解析成**精确**锚点；解析不出 ⇒ None。

    带目录的 token 必须与某锚点的 `literal` / `path` **全等**（旧口径用
    `endswith("/" + literal)` ⇒ `foo/monai.rs` 会误命中 `monai.rs` 锚点）。
    裸 basename 只接受显式 `basename_ok=True` 的锚点。
    """
    tok = _norm(token)
    has_dir = "/" in tok
    for a in anchors:
        lit = _norm(a["literal"])
        path = _norm(a["path"])
        if has_dir:
            if tok == lit or tok == path:
                return a
        elif a.get("basename_ok") and (tok == lit or os.path.basename(path) == tok):
            return a
    return None


# 完整引用 `<path>.<ext>:<line>`（ext 以字母开头，避免把 `127.0.0.1:80` / `v1.2:3`
# 之类误判成源码引用）。原先只认 6 个扩展名 ⇒ 其余静默跳过。
_FULL_REF_RE = re.compile(
    r"(?<![A-Za-z0-9_./\\-])"
    r"([A-Za-z0-9_][A-Za-z0-9_./\\-]*?\.([A-Za-z][A-Za-z0-9]{0,11})):(\d+)"
)
# 同行**裸续引用** `:NN`（`:` 前不是路径字符 ⇒ 不会命中完整引用本身的 `:NN`）。
_BARE_REF_RE = re.compile(r"(?<![A-Za-z0-9_./\\-]):(\d+)")


def check_doc_text(doc_text, root=REPO_ROOT, anchors=ANCHORS):
    """返回问题清单（空 = 全部锚点保鲜）。纯函数，便于 selftest 注入改坏文本。"""
    problems = []

    # ---- ① 主锚点：文档写的行号 == 当前源码首个匹配行（行号锚）/ 符号唯一（symbol_unique） ----
    for a in anchors:
        if a.get("symbol_unique"):
            # 符号唯一性锚（判据 8）：共享增长文件不做行号锚 —— 文档须引用该文件，且**不得**带行号；
            # 机判只看期望符号在该文件里「恰好出现一次」（稳定，不再被别的车道插代码推平）。
            if _norm(a["literal"]) not in _norm(doc_text):
                problems.append("ANCHOR_MISSING %s：文档里找不到 `%s`（符号唯一性锚：只认文件路径，"
                                "不写行号）" % (a["why"], a["literal"]))
                continue
            if _anchor_re(a).search(doc_text):
                problems.append("ANCHOR_LINE_NOT_ALLOWED %s：`%s` 是**符号唯一性锚**（共享增长文件不做"
                                "行号锚，行号必漂）⇒ 文档不得写 `%s:<line>`，只写文件路径"
                                % (a["why"], a["literal"], a["literal"]))
            src = os.path.join(root, a["path"])
            if not os.path.isfile(src):
                problems.append("ANCHOR_FILE_MISSING %s：%s 不存在" % (a["why"], a["path"]))
                continue
            lines = _read_lines(src)
            # 按「匹配总次数」计数（reviewer-5 F-1 收口）：旧口径 `sum(1 for line if re.search)`
            # 数的是**匹配行数** ⇒ 同一物理行写两遍符号只计 1 次而放行（现实形状不可达，但门不该有这条缝）。
            n = sum(len(re.findall(a["rx"], line)) for line in lines)
            if n == 0:
                problems.append("ANCHOR_SYMBOL_MISSING %s：%s 里找不到 /%s/"
                                % (a["why"], a["path"], a["rx"]))
            elif n > 1:
                problems.append("ANCHOR_AMBIGUOUS %s：%s 里 /%s/ 出现 %d 次 ⇒ 无法判定"
                                % (a["why"], a["path"], a["rx"], n))
            continue
        rx = _anchor_re(a)
        hits = rx.findall(doc_text)
        if not hits:
            problems.append("ANCHOR_MISSING %s：文档里找不到 `%s:<line>`"
                            % (a["why"], a["literal"]))
            continue
        if len(hits) > 1:
            problems.append("ANCHOR_AMBIGUOUS %s：`%s:<line>` 出现 %d 次（%s）⇒ 无法判定"
                            % (a["why"], a["literal"], len(hits), ",".join(hits)))
            continue
        cited = int(hits[0])
        src = os.path.join(root, a["path"])
        if not os.path.isfile(src):
            problems.append("ANCHOR_FILE_MISSING %s：%s 不存在" % (a["why"], a["path"]))
            continue
        lines = _read_lines(src)
        first = None
        for i, line in enumerate(lines, 1):
            if re.search(a["rx"], line):
                first = i
                break
        if first is None:
            problems.append("ANCHOR_SYMBOL_MISSING %s：%s 里找不到 /%s/"
                            % (a["why"], a["path"], a["rx"]))
        elif first != cited:
            problems.append("ANCHOR_DRIFT %s：文档写 %s:%d，但 /%s/ 的首个匹配在 :%d"
                            % (a["why"], a["path"], cited, a["rx"], first))

    # ---- ② 闭世界：每个完整 `path.<ext>:<line>` 引用都必须被锚点覆盖 ----
    full_matches = list(_FULL_REF_RE.finditer(doc_text))
    for m in full_matches:
        token, ext, line = m.group(1), m.group(2).lower(), m.group(3)
        if ext not in KNOWN_EXTS:
            problems.append("UNKNOWN_REF_EXT 文档引用 `%s:%s` 的扩展名 `.%s` 不在保鲜白名单里 —— "
                            "未识别扩展名必须显式登记（旧口径会静默跳过 ⇒ 可静默漂移）"
                            % (token, line, ext))
            continue
        if _resolve_ref(token, anchors) is None:
            problems.append("UNGUARDED_ANCHOR 文档引用 `%s:%s` 没有保鲜锚点 —— "
                            "新增 `path:line` 必须在 ANCHORS 登记（否则行号会静默漂移）"
                            % (token, line))

    # ---- ③ 同行裸续引用 `:NN`：归到同一物理行里前一个完整引用，按 `also` 校验 ----
    starts = [0]
    for i, ch in enumerate(doc_text):
        if ch == "\n":
            starts.append(i + 1)

    def line_idx(pos):
        return bisect.bisect_right(starts, pos) - 1

    full_by_line = {}
    for m in full_matches:
        full_by_line.setdefault(line_idx(m.start()), []).append(m)

    cache = {}

    def cached_lines(src):
        if src not in cache:
            cache[src] = _read_lines(src)
        return cache[src]

    for m in _BARE_REF_RE.finditer(doc_text):
        li = line_idx(m.start())
        cands = [f for f in full_by_line.get(li, ()) if f.end() <= m.start()]
        if not cands:
            # 同行没有前置完整引用 ⇒ 视为散文（文档里历史值 `:5229`/`:5440` 即此）。
            continue
        ref = max(cands, key=lambda f: f.end())
        token, line = ref.group(1), int(m.group(1))
        a = _resolve_ref(token, anchors)
        if a is None:
            problems.append("BARE_REF_UNGUARDED 同行裸续引用 `:%d` 归属 `%s`（未登记锚点）"
                            % (line, token))
            continue
        decl = None
        for e in a.get("also", ()):
            if e["line"] == line:
                decl = e
                break
        if decl is None:
            problems.append("BARE_REF_UNDECLARED %s：同行裸续引用 `:%d`（归属 `%s`）未在锚点 "
                            "`also` 登记（裸 `:NN` 不登记 ⇒ 行号可静默漂移）"
                            % (a["why"], line, token))
            continue
        src = os.path.join(root, a["path"])
        if not os.path.isfile(src):
            problems.append("BARE_REF_FILE_MISSING %s：%s 不存在" % (a["why"], a["path"]))
            continue
        lines = cached_lines(src)
        if line < 1 or line > len(lines):
            problems.append("BARE_REF_OUT_OF_RANGE %s：`%s:%d` 越界（文件共 %d 行）"
                            % (a["why"], a["path"], line, len(lines)))
            continue
        if not re.search(decl["rx"], lines[line - 1]):
            problems.append("BARE_REF_DRIFT %s：`%s:%d` 不匹配 /%s/（实际 `%s`）"
                            % (decl.get("why", a["why"]), a["path"], line, decl["rx"],
                               lines[line - 1][:80]))
    return problems


def check(doc=None, root=REPO_ROOT):
    doc = doc or os.path.join(HERE, DOC_NAME)
    with open(doc, "r", encoding="utf-8-sig") as fh:
        text = fh.read()
    return check_doc_text(text, root)


def _print(problems, prefix):
    for p in problems:
        print("  RED %s" % p)
    if problems:
        print("%s: check_anchors %d 项 RED" % (prefix, len(problems)))
        return 1
    print("%s: check_anchors %d/%d 锚点保鲜（行号锚：文档行号 == 当前源码首个匹配行；"
          "符号唯一性锚：文档只引用文件、符号恰出现 1 次；闭世界 + 裸续引用）"
          % (prefix, len(ANCHORS), len(ANCHORS)))
    return 0


def selftest():
    """check + 改坏必红。返回 rc（0 = GREEN）。"""
    checks = []

    def check_case(name, ok, detail=""):
        checks.append((name, bool(ok), detail))

    # ① 真身：当前文档 + 当前源码必须全绿。
    real = check()
    check_case("real-doc-fresh", not real, "; ".join(real))

    doc_path = os.path.join(HERE, DOC_NAME)
    with open(doc_path, "r", encoding="utf-8-sig") as fh:
        text = fh.read()

    # ② 改坏必红①（符号唯一性锚，2026-10-14 根因收口）：宿主 AI 调用点在共享增长文件 lib.rs 上
    #    ⇒ 不做行号锚。改坏：文档把它写回带行号的形式 ⇒ `ANCHOR_LINE_NOT_ALLOWED` 必红。
    a_main = next(a for a in ANCHORS if a.get("symbol_unique"))
    line_ref = text + "\n（另见 `%s:5795`）\n" % a_main["literal"]
    bad = check_doc_text(line_ref)
    check_case("mutation-symbol-line-ref-red",
               any("ANCHOR_LINE_NOT_ALLOWED" in p and "lib.rs" in p for p in bad),
               "; ".join(bad))

    # ②b 改坏必红②：把符号唯一性锚的**文件引用**从文档里删掉 ⇒ `ANCHOR_MISSING` 必红。
    dropped_ref = text.replace("`%s`" % a_main["literal"], "`（该文件）`")
    bad1b = check_doc_text(dropped_ref)
    check_case("mutation-symbol-ref-removed-red",
               any("ANCHOR_MISSING" in p and "lib.rs" in p for p in bad1b),
               "; ".join(bad1b))

    # ②c 改坏必红③（唯一性牙）：符号在文件里出现 >1 次 ⇒ `ANCHOR_AMBIGUOUS` 必红
    #     （证明「恰好一次」不是空话：调用点被复制 = 判据必红）。
    dup_symbol = dict(literal="rng-trace-scene/check_anchors.py",
                      path="mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py",
                      rx=r"^def ",
                      why="改坏：符号在文件里出现多次",
                      symbol_unique=True)
    bad1c = check_doc_text(text, anchors=tuple(list(ANCHORS) + [dup_symbol]))
    check_case("mutation-symbol-ambiguous-red",
               any("ANCHOR_AMBIGUOUS" in p and "check_anchors.py" in p for p in bad1c),
               "; ".join(bad1c))

    # ②d 改坏必红④（存在性牙）：符号在文件里 0 次 ⇒ `ANCHOR_SYMBOL_MISSING` 必红
    #     （证明调用点被删仍 fail-closed）。
    missing_symbol = dict(literal="rng-trace-scene/check_anchors.py",
                          path="mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py",
                          rx=r"^def NOPE_THIS_SYMBOL_IS_NOT_HERE\(",
                          why="改坏：符号在文件里不存在",
                          symbol_unique=True)
    bad1d = check_doc_text(text, anchors=tuple(list(ANCHORS) + [missing_symbol]))
    check_case("mutation-symbol-missing-red",
               any("ANCHOR_SYMBOL_MISSING" in p and "check_anchors.py" in p for p in bad1d),
               "; ".join(bad1d))

    # ②e 改坏必红③b（reviewer-5 F-1 收口）：期望符号在**同一物理行**出现两次（如 `spawn(); spawn();`）
    #    也必须判 `ANCHOR_AMBIGUOUS`。旧实现按「匹配**行**数」计数会把同行两遍算 1 次 ⇒ 假绿；
    #    新实现按「匹配总次数」计数 ⇒ 必红。（用临时源文件，跑完即删，不留痕。）
    with tempfile.TemporaryDirectory() as td:
        src_dir = os.path.join(td, "crates", "world", "src")
        os.makedirs(src_dir)
        with open(os.path.join(src_dir, "dup_same_line.rs"), "w", encoding="utf-8") as fh:
            fh.write("spawn(); spawn();\n")
        dup_same_line_anchor = dict(literal="crates/world/src/dup_same_line.rs",
                                    path="crates/world/src/dup_same_line.rs",
                                    rx=r"spawn\(\)",
                                    why="改坏：符号在同一物理行出现两次",
                                    symbol_unique=True)
        bad1e = check_doc_text("（另见 `crates/world/src/dup_same_line.rs`）\n",
                               root=td,
                               anchors=(dup_same_line_anchor,))
    check_case("mutation-symbol-ambiguous-same-line-red",
               any("ANCHOR_AMBIGUOUS" in p and "dup_same_line.rs" in p for p in bad1e),
               "; ".join(bad1e))

    # ③ 改坏必红②：删掉 `mongen.rs:61` 锚点行 ⇒ 必红且点名 ANCHOR_MISSING。
    missing = text.replace("`mongen.rs:61", "`mongen.rs")
    bad2 = check_doc_text(missing)
    check_case("mutation-anchor-removed-red",
               any("ANCHOR_MISSING" in p for p in bad2),
               "; ".join(bad2))

    # ④ 改坏必红③：把同一 literal 写成两处行号 ⇒ 必红（AMBIGUOUS，禁止歧义锚）。
    dup = text + "\n（重复引用 `%s:%d`）\n" % (ANCHORS[3]["literal"], 171)
    bad3 = check_doc_text(dup)
    check_case("mutation-anchor-ambiguous-red",
               any("ANCHOR_AMBIGUOUS" in p for p in bad3),
               "; ".join(bad3))

    # ⑤ 改坏必红④：文档新增一个**未登记**的 `path:line` 引用 ⇒ `UNGUARDED_ANCHOR` 必红。
    unguarded = text + "\n（另见 `crates/world/src/merchant.rs:999`）\n"
    bad4 = check_doc_text(unguarded)
    check_case("mutation-unguarded-anchor-red",
               any("UNGUARDED_ANCHOR" in p and "merchant.rs" in p for p in bad4),
               "; ".join(bad4))

    # ⑥ 改坏必红⑤：把 R-6 折叠映射定义行号改回历史值 `:52`（本片真身漂移）⇒ `ANCHOR_DRIFT` 必红。
    a_fold = next(a for a in ANCHORS if a["literal"].endswith("rng_consumer_groups.py"))
    fold_drifted = _anchor_re(a_fold).sub(
        "rng-trace-probe/rng_consumer_groups.py:52", text, count=1)
    bad5 = check_doc_text(fold_drifted)
    check_case("mutation-fold-def-drift-red",
               any("ANCHOR_DRIFT" in p and "FOLD_RULES" in p for p in bad5),
               "; ".join(bad5))

    # ⑦ 改坏必红⑥（reviewer-5 M-1 / reviewer-6 M-2 收口）：**别的目录**下的同名 basename
    #    引用必须判 `UNGUARDED_ANCHOR`（旧口径 `endswith("/"+literal)` 会误判 COVERED ⇒ 静默）。
    sibling = text + "\n（另见 `foo/monai.rs:999`）\n"
    bad6 = check_doc_text(sibling)
    check_case("mutation-sibling-basename-red",
               any("UNGUARDED_ANCHOR" in p and "foo/monai.rs" in p for p in bad6),
               "; ".join(bad6))

    # ⑧ 改坏必红⑦（reviewer-6 M-1 收口）：同行裸续引用 `:57` 改成未登记的 `:58`
    #    ⇒ `BARE_REF_UNDECLARED` 必红（裸 `:NN` 不登记 = 静默漂移通道）。
    assert text.count("`:57`") == 1, "文档里应恰有一处裸续引用 `:57`（自检前置被破坏）"
    bare_drift = text.replace("`:57`", "`:58`")
    bad7 = check_doc_text(bare_drift)
    check_case("mutation-bare-ref-undeclared-red",
               any("BARE_REF_UNDECLARED" in p and ":58" in p for p in bad7),
               "; ".join(bad7))

    # ⑨ 改坏必红⑧：裸续引用已登记，但**目标行内容**漂移 ⇒ `BARE_REF_DRIFT` 必红
    #    （证明 `also` 的 `rx` 是真牙，不是只查行号在范围内）。
    bad_anchors = []
    for a in ANCHORS:
        if a["literal"].endswith("consumer_groups.tsv"):
            a = dict(a, also=(dict(line=57, rx=r"^NOPE_THIS_ROW_IS_NOT_SPAWN",
                                   why="改坏：内容牙"),))
        bad_anchors.append(a)
    bad8 = check_doc_text(text, anchors=tuple(bad_anchors))
    check_case("mutation-bare-ref-content-drift-red",
               any("BARE_REF_DRIFT" in p for p in bad8),
               "; ".join(bad8))

    # ⑩ 改坏必红⑨：未识别扩展名的 `path:line` 引用必须 fail-closed ⇒ `UNKNOWN_REF_EXT`
    #    （旧口径 `_DOC_REF_RE` 只认 6 个扩展名，其余静默跳过）。
    unknown_ext = text + "\n（另见 `docs/spec.xyz:12`）\n"
    bad9 = check_doc_text(unknown_ext)
    check_case("mutation-unknown-ext-red",
               any("UNKNOWN_REF_EXT" in p and "spec.xyz" in p for p in bad9),
               "; ".join(bad9))

    # ⑪ 改坏必红⑩（reviewer-6 M-1 收口）：同行裸续引用归属的**完整引用本身未登记锚点** ⇒
    #    `BARE_REF_UNGUARDED` 必红（此前该分支无活用例）。
    unguarded_bare = text + "\n（另见 `crates/world/src/merchant.rs:999`、`:12`）\n"
    bad10 = check_doc_text(unguarded_bare)
    check_case("mutation-bare-ref-unguarded-red",
               any("BARE_REF_UNGUARDED" in p for p in bad10),
               "; ".join(bad10))

    # ⑫ 改坏必红⑪（reviewer-6 M-1 收口）：裸续引用已登记、行号在范围内，但归属文件**不存在**
    #    ⇒ `BARE_REF_FILE_MISSING` 必红（此前该分支无活用例）。
    fake_missing = dict(literal="crates/world/src/no_such_module.rs",
                        path="mir2-rs/crates/world/src/no_such_module.rs",
                        rx=r"pub fn nothing",
                        why="改坏：归属文件缺失",
                        also=(dict(line=7, rx=r".", why="改坏：归属文件缺失"),))
    bad11 = check_doc_text(text + "\n（另见 `crates/world/src/no_such_module.rs:3`、`:7`）\n",
                           anchors=tuple(list(ANCHORS) + [fake_missing]))
    check_case("mutation-bare-ref-file-missing-red",
               any("BARE_REF_FILE_MISSING" in p for p in bad11),
               "; ".join(bad11))

    # ⑬ 改坏必红⑫（reviewer-6 M-1 收口）：裸续引用已登记，但登记行号**越界**（超出文件行数）
    #    ⇒ `BARE_REF_OUT_OF_RANGE` 必红（此前该分支无活用例）。
    oor_anchors = []
    for a in ANCHORS:
        if a["literal"].endswith("consumer_groups.tsv"):
            a = dict(a, also=(dict(line=99999, rx=r".", why="改坏：登记行号越界"),))
        oor_anchors.append(a)
    bad12 = check_doc_text(text.replace("`:57`", "`:99999`"), anchors=tuple(oor_anchors))
    check_case("mutation-bare-ref-out-of-range-red",
               any("BARE_REF_OUT_OF_RANGE" in p for p in bad12),
               "; ".join(bad12))

    total = len(checks)
    badn = [(n, d) for n, ok, d in checks if not ok]
    for n, ok, d in checks:
        print("  %s %s%s" % ("ok " if ok else "RED", n, ("（%s）" % d) if d else ""))
    if badn:
        print("SELFTEST RED: check_anchors selftest %d/%d" % (total - len(badn), total))
        return 1
    print("SELFTEST GREEN: check_anchors selftest %d/%d（含 16 条改坏必红：符号唯一性锚写回行号 / "
          "文件引用删除 / 符号重复 / 符号同行重复 / 符号缺失 / 行号锚缺失 / 行号锚歧义 / 未登记引用 / "
          "折叠定义回退 / 同名目录串台 / 裸续引用未登记 / 裸续内容漂移 / 未识别扩展名 / "
          "裸续引用归属未登记 / 裸续引用归属文件缺失 / 裸续引用登记行越界）"
          % (total, total))
    return 0


if __name__ == "__main__":
    argv = sys.argv[1:]
    mode = argv[0] if argv and not argv[0].startswith("--") else "check"
    doc = argv[argv.index("--doc") + 1] if "--doc" in argv else None
    if mode == "check":
        problems = check(doc=doc)
        raise SystemExit(_print(problems, "GREEN" if not problems else "RED"))
    if mode == "selftest":
        raise SystemExit(selftest())
    print("usage: check_anchors.py [check|selftest] [--doc <path>]", file=sys.stderr)
    raise SystemExit(2)
