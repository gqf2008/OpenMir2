#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""EOL 陷阱夹具（`openmir2-m3-attack-pair-fixture`，owner 2026-10-12 23:30）。

在临时 git 仓库里构造两种「盘上字节 / git blob 字节 与仓库口径（LF 归一）在 EOL 上不一致」
的样例目录，跑 `tools/evidence-hash/verify_sha256sums.py` 验证**必红且点名**；再把清单改写成
仓库口径验证**必绿**。原始件（本脚本输出的 stdout）入库到
`tools/evidence-hash/evidence/2026-10-12-eol-tooth/`。

用法（仓库根）：

    python -I tools/evidence-hash/fixtures/eol-trap/demo.py

退出码：`0` = 两个样例都按预期红→绿；`2` = 有样例不符预期。
"""

from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve()
REPO = HERE.parents[4]                       # tools/evidence-hash/fixtures/eol-trap/demo.py → 仓库根
VERIFIER = REPO / "tools" / "evidence-hash" / "verify_sha256sums.py"

LINE_LF = b"line1\nline2\n"
LINE_CRLF = b"line1\r\nline2\r\n"


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def run_git(repo: Path, *args: str, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args], check=True, **kw)


def verify(base: Path, repo: Path, *extra: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, "-I", str(VERIFIER), "--repo", str(repo), *extra, str(base)],
        capture_output=True, text=True)


def write_manifest(base: Path, items: list[tuple[str, str]]) -> None:
    (base / "SHA256SUMS.txt").write_text(
        "".join(f"{h} {name}\n" for h, name in items), encoding="utf-8", newline="\n")


def show(title: str, cp: subprocess.CompletedProcess) -> None:
    print(f"$ {title}")
    print(cp.stdout.rstrip())
    if cp.stderr.strip():
        print(cp.stderr.rstrip())
    print(f"[exit={cp.returncode}]")
    print()


def check(cond: bool, name: str) -> bool:
    print(f"{'PASS' if cond else 'RED '} {name}")
    return cond


def main() -> int:
    tmp = Path(tempfile.mkdtemp(prefix="eol-trap-demo-"))
    ok = True
    try:
        repo = tmp / "trap-repo"
        ev = repo / "evidence"
        ev.mkdir(parents=True)
        run_git(repo, "init", "-q")
        run_git(repo, "config", "core.autocrlf", "true")
        run_git(repo, "config", "user.email", "demo@example.invalid")
        run_git(repo, "config", "user.name", "demo")
        (repo / ".gitattributes").write_text("*.txt text eol=lf\n", encoding="utf-8", newline="\n")

        # ---- 样例 1：盘上 CRLF / blob LF（清单按盘上字节记 = 陷阱） ----
        (ev / "a.txt").write_bytes(LINE_LF)
        run_git(repo, "add", "-A")
        run_git(repo, "commit", "-q", "-m", "blob a = LF")
        (ev / "a.txt").write_bytes(LINE_CRLF)          # 盘上 CRLF，blob 仍是 LF
        h_a_lf, h_a_crlf = sha(LINE_LF), sha(LINE_CRLF)

        print("== 样例 1：盘上 CRLF / blob LF；清单记盘上原始字节(CRLF) ==")
        write_manifest(ev, [(h_a_crlf, "a.txt")])
        cp = verify(ev, repo)
        show("verify（仓库口径，陷阱清单）", cp)
        ok &= check(cp.returncode == 1 and "EOL 陷阱" in cp.stdout and "a.txt" in cp.stdout,
                    "样例1 仓库口径：必红且点名 a.txt")
        cp_disk = verify(ev, repo, "--disk-bytes")
        show("verify --disk-bytes（同陷阱清单）", cp_disk)
        ok &= check(cp_disk.returncode == 1,
                    "样例1 --disk-bytes：也必红（不许靠清单自洽拿假绿）")

        write_manifest(ev, [(h_a_lf, "a.txt")])
        cp_fix = verify(ev, repo)
        show("verify（清单改写为仓库口径 LF）", cp_fix)
        ok &= check(cp_fix.returncode == 0 and "GREEN" in cp_fix.stdout,
                    "样例1 修法：清单按仓库口径(LF)记 ⇒ 绿")

        # ---- 样例 2：blob 自己带 CRLF / 盘上 LF（m3cs 真实先例） ----
        oid = subprocess.run(
            ["git", "-C", str(repo), "hash-object", "-w", "--stdin", "--no-filters"],
            input=LINE_CRLF, capture_output=True, check=True).stdout.strip().decode()
        run_git(repo, "update-index", "--add", "--cacheinfo", f"100644,{oid},evidence/b.txt")
        (ev / "b.txt").write_bytes(LINE_LF)            # 盘上 LF，但 blob 含 CRLF
        run_git(repo, "commit", "-q", "-m", "blob b = CRLF")

        print("== 样例 2：blob 含 CRLF / 盘上 LF；清单记 blob 原始字节（m3cs 真实先例） ==")
        write_manifest(ev, [(h_a_lf, "a.txt"), (h_a_crlf, "b.txt")])
        cp2 = verify(ev, repo)
        show("verify（仓库口径，陷阱清单）", cp2)
        ok &= check(cp2.returncode == 1 and "EOL 陷阱" in cp2.stdout and "b.txt" in cp2.stdout
                    and "blob" in cp2.stdout,
                    "样例2 仓库口径：必红且点名 b.txt（source=git blob 原始字节）")

        write_manifest(ev, [(h_a_lf, "a.txt"), (h_a_lf, "b.txt")])
        cp2_fix = verify(ev, repo)
        show("verify（清单改写为仓库口径 LF）", cp2_fix)
        ok &= check(cp2_fix.returncode == 0 and "GREEN" in cp2_fix.stdout,
                    "样例2 修法：清单按仓库口径(LF)记 ⇒ 绿")
        return 0 if ok else 2
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
