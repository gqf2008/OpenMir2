#!/usr/bin/env python
# 02 的三条分支独立构造（不经 selftest，直接喂 check_doc_text）—— 每条都真会红并点名。
# 复跑（自包含；cwd = mir2-rs/tests/parity/m3）：
#   python -I rng-trace-scene/../evidence-m3-rng-scene-anchor-teeth-20261014/02-branch-firings.py
# 或：cd mir2-rs/tests/parity/m3 && python -I evidence-m3-rng-scene-anchor-teeth-20261014/02-branch-firings.py
import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "rng-trace-scene")))
import check_anchors as ca  # noqa: E402

DOC = os.path.join(os.path.dirname(__file__), "..", "rng-trace-scene", "DRIVER-CONTRACT.md")
text = open(DOC, "r", encoding="utf-8-sig").read()

# ① 裸续引用归属的完整引用本身未登记锚点（merchant.rs 不在 ANCHORS 里）
p = ca.check_doc_text(text + "\n（另见 `crates/world/src/merchant.rs:999`、`:12`）\n")
print("① BARE_REF_UNGUARDED ->", [x for x in p if x.startswith("BARE_REF_UNGUARDED")])

# ② 归属文件不存在（注入一条 path 不存在的锚点 + 其 also 登记行）
fake = dict(literal="crates/world/src/no_such_module.rs",
            path="mir2-rs/crates/world/src/no_such_module.rs",
            rx=r"pub fn nothing", why="改坏：归属文件缺失",
            also=(dict(line=7, rx=r".", why="改坏：归属文件缺失"),))
p = ca.check_doc_text(text + "\n（另见 `crates/world/src/no_such_module.rs:3`、`:7`）\n",
                      anchors=tuple(list(ca.ANCHORS) + [fake]))
print("② BARE_REF_FILE_MISSING ->", [x for x in p if x.startswith("BARE_REF_FILE_MISSING")])

# ③ 已登记的 also 行号越界（超出文件行数）
oor = []
for a in ca.ANCHORS:
    if a["literal"].endswith("consumer_groups.tsv"):
        a = dict(a, also=(dict(line=99999, rx=r".", why="改坏：登记行号越界"),))
    oor.append(a)
p = ca.check_doc_text(text.replace("`:53`", "`:99999`"), anchors=tuple(oor))
print("③ BARE_REF_OUT_OF_RANGE ->", [x for x in p if x.startswith("BARE_REF_OUT_OF_RANGE")])
