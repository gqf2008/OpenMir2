#!/usr/bin/env python3
"""check_client_ability_layout_doc_counts.py — 文档漂移门（卡 `openmir2-client-e2e-enter-gaps`）

来源（reviewer-5 / reviewer-6 对片㊷ 的同一非阻塞 Minor）：`README.md §0b.4` 曾写
`15/15` + 改坏必红 `②a/③a`，而**可执行判据**的实际读数是 `20/20` +
`②a/②c/③a/⑤b`。文档与判据/证据脱节 ⇒ 下一次窗口照文档判读会误判。

本脚本把这条口径变成**可判红/绿的静态门**（纯文件读取，零进程 / 零端口 / 零起栈）：

  D1 `README` 的 `N/M` 形式自洽（`N == M`）
  D2 `README` 的 `N` == `check_client_ability_layout_plumb.ps1` 的 `Check '` 条数（活口径）
  D3 `README` 的 `N` == 证据 `02-selftest.txt` 的 `SUMMARY total=`
  D4 `README` 的改坏必红 id 集 == 证据 `03-redcheck.txt` 的 `[FAIL]` id 集（顺序无关）

退出码：`0` = GREEN；`1` = FAIL；`2` = PREREQ_MISSING（缺文件，不静默通过）。

改坏必红（本片证据 `06-readme-doc-counts.txt`）：把 `README.md §0b.4` 的 `20/20`
改回 `15/15`（或红检集改回 `②a/③a`）⇒ `D2`/`D4` 必红 `rc=1`；还原后复绿。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent            # tools/m2-acceptance
REPO = HERE.parent.parent                         # repo root
README = HERE / "README.md"
PLUMB = HERE / "check_client_ability_layout_plumb.ps1"
EVID = HERE / "evidence" / "m2svr-client-ability-layout-plumb-20261014"
GREEN_EVID = EVID / "02-selftest.txt"
RED_EVID = EVID / "03-redcheck.txt"

MARK = "CLIENT_ABILITY_LAYOUT_PLUMB GREEN"
GREEN_RE = re.compile(r"(\d+)\s*/\s*(\d+)\s*⇒\s*" + re.escape(MARK))
REDSET_RE = re.compile(r"⇒\s*((?:[①-⑩][a-z]\s*/\s*)*[①-⑩][a-z])\s*必红")
FAIL_RE = re.compile(r"\[FAIL\]\s+(\S+)")
TOTAL_RE = re.compile(r"SUMMARY total=(\d+)")
CHECK_RE = re.compile(r"^\s*Check '")


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def main() -> int:
    missing = [str(p.relative_to(REPO)) for p in (README, PLUMB, GREEN_EVID, RED_EVID) if not p.is_file()]
    if missing:
        print("PREREQ_MISSING: " + ", ".join(missing) + "（本门不静默通过）")
        return 2

    checks: list[tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        checks.append((name, ok, detail))

    readme = read_text(README)
    doc_lines = [ln for ln in readme.splitlines() if MARK in ln]
    doc_line = doc_lines[0] if doc_lines else ""

    m = GREEN_RE.search(doc_line)
    num = int(m.group(1)) if m else None
    den = int(m.group(2)) if m else None
    check(
        "D0 README 含 `<N>/<M> ⇒ CLIENT_ABILITY_LAYOUT_PLUMB GREEN` 行",
        bool(m),
        "doc_line=" + (doc_line.strip() or "<missing>"),
    )
    check(
        "D1 `N/M` 形式自洽（N == M）",
        (num is not None) and (num == den),
        f"N={num} M={den}",
    )

    check_count = sum(1 for ln in read_text(PLUMB).splitlines() if CHECK_RE.search(ln))
    check(
        f"D2 README N == 脚本 Check 条数（活口径 = {check_count}）",
        (num is not None) and (num == check_count),
        f"N={num} script_check_count={check_count}",
    )

    mt = TOTAL_RE.search(read_text(GREEN_EVID))
    ev_total = int(mt.group(1)) if mt else None
    check(
        f"D3 README N == 证据 02 SUMMARY total（= {ev_total}）",
        (num is not None) and (num == ev_total),
        f"N={num} evidence_total={ev_total}",
    )

    doc_ids = sorted({t.strip() for t in REDSET_RE.search(doc_line).group(1).split("/")}) if REDSET_RE.search(doc_line) else []
    ev_ids = sorted({m2.group(1) for m2 in FAIL_RE.finditer(read_text(RED_EVID))})
    check(
        "D4 README 改坏必红 id 集 == 证据 03 [FAIL] 集（顺序无关）",
        doc_ids == ev_ids,
        f"doc={doc_ids} evidence={ev_ids}",
    )

    failed = [c for c in checks if not c[1]]
    for name, ok, detail in checks:
        print(f"  [{'OK  ' if ok else 'FAIL'}] {name}")
        if detail:
            print(f"        {detail}")
    print(f"SUMMARY total={len(checks)} ok={len(checks) - len(failed)} fail={len(failed)}")
    if failed:
        print("CLIENT_ABILITY_LAYOUT_DOC_COUNTS FAIL")
        return 1
    print("CLIENT_ABILITY_LAYOUT_DOC_COUNTS GREEN")
    return 0


if __name__ == "__main__":
    sys.exit(main())
