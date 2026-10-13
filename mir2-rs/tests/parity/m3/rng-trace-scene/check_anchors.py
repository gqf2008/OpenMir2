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
它**不写任何文件**（`selftest` 只用内存文本 + 临时目录）。

## 判据（任一条不成立 ⇒ RED，逐条点名）

1. 文档里 `` `<literal>:<line>` `` **缺失 / 重复 / 行号非数字** ⇒ `ANCHOR_MISSING` / `ANCHOR_AMBIGUOUS`；
2. 解析出的源文件不存在 ⇒ `ANCHOR_FILE_MISSING`；
3. 期望符号在文件里**不存在** ⇒ `ANCHOR_SYMBOL_MISSING`；
4. 期望符号的**首个**匹配行 ≠ 文档写的行号 ⇒ `ANCHOR_DRIFT`（点名实际行，即要改成的值）。

## 用法

    python -I check_anchors.py check       # 检查本目录 DRIVER-CONTRACT.md（默认）
    python -I check_anchors.py check --doc <path>   # 检查任意副本（用于「改前/改后」对照）
    python -I check_anchors.py selftest    # check + 改坏必红（把文档行号改回旧值 ⇒ 必红）

退出码：0 = GREEN，1 = RED。
"""
import os
import re
import sys

DOC_NAME = "DRIVER-CONTRACT.md"

# 本文件：<repo>/mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py
HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", "..", ".."))

# ---------------------------------------------------------------------------
# 锚点表（唯一真源在这里；文档必须与它同口径）
#
#   literal  = 文档里出现的 `path` 字面（用来从文档里取出 `<line>`；带负向环视以免
#              `monai.rs` 误命中 `crates/world/src/monai.rs` 的尾部）
#   path     = 仓库根相对路径（用于打开源码）
#   rx       = 该行必须匹配的**符号正则**；要求「首个匹配行 == 文档行号」
#   why      = 这个锚点为什么承重（给 RED 输出用）
# ---------------------------------------------------------------------------
ANCHORS = (
    dict(literal="mir2-rs/bins/game-svr/src/main.rs",
         path="mir2-rs/bins/game-svr/src/main.rs",
         rx=r"enable_production_monster_ai\(&mut world\)",
         why="宿主无条件开怪 AI 调用点（本卡要 AI 开）"),
    dict(literal="mongen.rs",
         path="mir2-rs/bins/game-svr/src/mongen.rs",
         rx=r"pub fn enable_production_monster_ai\(",
         why="宿主 AI 开关接缝"),
    dict(literal="crates/world/src/monai.rs",
         path="mir2-rs/crates/world/src/monai.rs",
         rx=r"pub fn monster_ai_tick\(",
         why="World::tick 的怪 AI 槽位驱动"),
    dict(literal="monai.rs",
         path="mir2-rs/crates/world/src/monai.rs",
         rx=r"pub fn wondering\(",
         why="Wondering 的 Random(20) 抽数入口"),
    dict(literal="src/M2Server/Actor/AnimalObject.cs",
         path="src/M2Server/Actor/AnimalObject.cs",
         rx=r"protected virtual void Wondering",
         why="oracle Wondering 锚（冻结面，仅防误抄）"),
)


def _anchor_re(literal):
    """`` `<literal>:<line>` ``（负向环视防止短 basename 命中长路径的尾部）。"""
    return re.compile(r"(?<![A-Za-z0-9_./])" + re.escape(literal) + r":(\d+)")


def _read_lines(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read().splitlines()


def check_doc_text(doc_text, root=REPO_ROOT, anchors=ANCHORS):
    """返回问题清单（空 = 全部锚点保鲜）。纯函数，便于 selftest 注入改坏文本。"""
    problems = []
    for a in anchors:
        rx = _anchor_re(a["literal"])
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
    print("%s: check_anchors %d/%d 锚点保鲜（文档行号 == 当前源码首个匹配行）"
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

    # ② 改坏必红①：把宿主 AI 行号改回历史漂移值 `:5229` ⇒ 必红且点名实际行。
    #    （2026-10-13 实测：main.rs 合并后该符号在 :5440，文档曾写 :5229。）
    drifted = text
    for a in ANCHORS:
        if a["literal"].endswith("main.rs"):
            drifted = _anchor_re(a["literal"]).sub(
                lambda m: "%s:5229" % a["literal"], drifted, count=1)
            break
    bad = check_doc_text(drifted)
    check_case("mutation-line-drift-red",
               any("ANCHOR_DRIFT" in p and "5229" in p for p in bad),
               "; ".join(bad))

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

    total = len(checks)
    badn = [(n, d) for n, ok, d in checks if not ok]
    for n, ok, d in checks:
        print("  %s %s%s" % ("ok " if ok else "RED", n, ("（%s）" % d) if d else ""))
    if badn:
        print("SELFTEST RED: check_anchors selftest %d/%d" % (total - len(badn), total))
        return 1
    print("SELFTEST GREEN: check_anchors selftest %d/%d（含 3 条改坏必红）" % (total, total))
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
