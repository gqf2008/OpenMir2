#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M3 夹具链族「fmt 证据自描述」门（`m3_fixture_fmt_evidence_gate.py`）——只读、零端口。

## 为什么有它

卡 `openmir2-m3-fixture-chain-gate` 的 reviewer-6（`090d0cc4 @f114a5e90`）**O-1**（非阻塞）：
`evidence-m3-anchor-prefix-o2-20261014/40-fmt.txt` 是 **0 字节空文件**，且未按同族证据
`$ <命令>` / `rc=<n>` 的体例记录命令头与退出码 ⇒ 该件**本身不能证明 fmt 绿**（reviewer 只能
自己复跑）。同族的 `evidence-m3-chain-gate-o2-20261014/20-gate-fmt.txt`、
`evidence-m3-fixture-chain-hero-anchor-20261014/05-gate-fmt.txt`、
`evidence-m3-chain-gate-residuals-20261014/10-gate-fmt.txt`（只有 `rc=0` 无命令头）、
`evidence-m3-fixture-family-minors-20261014/08-gate-fmt.txt` 同病。

这类「空件 / 无命令头」证据是**审计噪声**：文件在、hash 在，却无法自证跑过什么、退出码多少。
本件把「夹具链族各证据包的 fmt 证据**非空 + 有命令头 + 有 rc**」做成一条命令 + 可复跑读数，
并随本片把 5 个欠件补齐；约定沿用同族既有的 `$ cargo fmt --all -- --check` + `rc=0` 两行体例。

## 命令

    python -I m3_fixture_fmt_evidence_gate.py check
    python -I m3_fixture_fmt_evidence_gate.py selftest
    python -I m3_fixture_fmt_evidence_gate.py mutation

口径：**只读**（只读文件，不写仓库）、**零端口**、不依赖 cargo/dotnet。`selftest` 用内存替身；
`mutation` 在临时目录做「抹空 / 删命令头 / 删 rc」三类外部改坏并断言 RED（改坏必红）。

## 接入（卡 `openmir2-m3-fixture-chain-gate`）

与同族登记门同形：无调用方 ⇒ 「在场却坏无声」。故随本片接入 `fixture_chain_gate.py` 的
`SHARED_CHECKS`（`fixture_fmt_evidence_selftest` + `fixture_fmt_evidence_check`），
不看矩阵状态、独立常跑；`fixture_chain_gate.selftest` 内含**防回退牙**（本门被反注册 ⇒ 链门 selftest RED）。

## 边界（如实登记，不假绿）

**只覆盖 `FAMILY_DIRS` 登记的 9 个夹具链族证据包**，不对全仓 ≈40 个历史 `*fmt*` 欠件判红
（那是全仓证据卫生清理，需单开卡 + 逐包重记 SHA256SUMS，不在本片写集）。本门也不检查证据内容
是否与真实 `cargo fmt` 结果一致（那由四点四件套的「命令 + 原始件 + hash」+ 四门禁复核承担）。
"""
from __future__ import annotations

import argparse
import os
import re
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))

# 夹具链族（fixture_chain_gate）历次片的证据包；相对 REPO_ROOT 的 posix 相对路径。
FAMILY_DIRS = [
    "mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-gate-20261013",
    "mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-guild-anchor-20261014",
    "mir2-rs/tests/parity/m3/evidence-m3-fixture-chain-hero-anchor-20261014",
    "mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-gate-20261014",
    "mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o1-20261014",
    "mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014",
    "mir2-rs/tests/parity/m3/evidence-m3-chain-gate-o2-20261014",
    "mir2-rs/tests/parity/m3/evidence-m3-chain-gate-residuals-20261014",
    "mir2-rs/tests/parity/m3/evidence-m3-fixture-family-minors-20261014",
]

# fmt 证据件：文件名含 `fmt` 的 `.txt`。
def is_fmt_evidence(name):
    base = os.path.basename(name)
    return base.lower().endswith(".txt") and "fmt" in base.lower()


# 命令头：行首（允许缩进）`$ ...`；rc：整行 `rc=<数字>`。
CMD_RE = re.compile(r"^\s*\$\s*\S", re.MULTILINE)
RC_RE = re.compile(r"^\s*rc=\d+\s*$", re.MULTILINE)


def evaluate(dirs, lister, reader):
    """纯函数（便于 selftest 注入）。

    dirs   = [相对路径...]
    lister = fn(dir) -> None（目录不存在）或 [文件名...]
    reader = fn(dir, name) -> str
    ⇒ (findings, reds)
    """
    findings, reds = [], []
    for d in dirs:
        names = lister(d)
        if names is None:
            reds.append("MISSING_DIR {0}".format(d))
            continue
        fmts = sorted(n for n in names if is_fmt_evidence(n))
        if not fmts:
            reds.append("NO_FMT_EVIDENCE {0}".format(d))
            continue
        for n in fmts:
            rel = "{0}/{1}".format(d, n)
            text = reader(d, n)
            if not text.strip():
                reds.append("EMPTY {0}".format(rel))
                continue
            if not CMD_RE.search(text):
                reds.append("NO_COMMAND {0}".format(rel))
            if not RC_RE.search(text):
                reds.append("NO_RC {0}".format(rel))
            if not CMD_RE.search(text) or not RC_RE.search(text):
                findings.append("{0}: 期望体例 `$ <命令>` + `rc=<n>`".format(rel))
    return findings, reds


# --------------------------------------------------------------------------- #
# 真身扫描
def _real_lister(d):
    p = os.path.join(REPO_ROOT, d)
    if not os.path.isdir(p):
        return None
    out = []
    for root, _dirs, files in os.walk(p):
        for f in files:
            rel = os.path.relpath(os.path.join(root, f), p).replace(os.sep, "/")
            out.append(rel)
    return sorted(out)


def _real_reader(d, name):
    with open(os.path.join(REPO_ROOT, d, name), "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def cmd_check(_args):
    print("FIXTURE_FMT_EVIDENCE dirs={0}".format(len(FAMILY_DIRS)))
    findings, reds = evaluate(FAMILY_DIRS, _real_lister, _real_reader)
    for f in findings:
        print("  FINDING {0}".format(f))
    if reds:
        print("FIXTURE_FMT_EVIDENCE_RED {0}：{1}".format(len(reds), "; ".join(reds)))
        return 1
    total = 0
    for d in FAMILY_DIRS:
        total += len([n for n in _real_lister(d) if is_fmt_evidence(n)])
    print("FIXTURE_FMT_EVIDENCE GREEN（{0} 个证据包 / {1} 件 fmt 证据非空 + 命令头 + rc）".format(
        len(FAMILY_DIRS), total))
    return 0


# --------------------------------------------------------------------------- #
# 自检：注入式内存替身
def cmd_selftest(_args):
    ok, total = 0, 0

    def expect(name, cond):
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
            print("  SELFTEST ok  {0}".format(name))
        else:
            print("  SELFTEST FAIL {0}".format(name))

    GOOD = "$ cargo fmt --all -- --check\nrc=0\n"

    def mk(files, missing=()):
        def lister(d):
            if d in missing:
                return None
            return sorted(n for (dd, n) in files if dd == d)
        def reader(d, n):
            return files[(d, n)]
        return lister, reader

    D = "pkgA"
    files = {
        (D, "40-fmt.txt"): GOOD,
        (D, "01-boundary.txt"): "not examined\n",
    }
    l, r = mk(files, missing={"pkgB"})
    _f, reds = evaluate([D], l, r)
    expect("合规 fmt 证据 -> GREEN", reds == [])

    files_empty = dict(files)
    files_empty[(D, "40-fmt.txt")] = ""
    _f, reds = evaluate([D], *mk(files_empty))
    expect("空件 -> EMPTY RED", any(x.startswith("EMPTY ") for x in reds))

    files_ws = dict(files)
    files_ws[(D, "40-fmt.txt")] = "   \n\n"
    _f, reds = evaluate([D], *mk(files_ws))
    expect("纯空白 -> EMPTY RED", any(x.startswith("EMPTY ") for x in reds))

    files_nocmd = dict(files)
    files_nocmd[(D, "40-fmt.txt")] = "rc=0\n"
    _f, reds = evaluate([D], *mk(files_nocmd))
    expect("只有 rc 无命令头 -> NO_COMMAND RED", any(x.startswith("NO_COMMAND ") for x in reds))

    files_norc = dict(files)
    files_norc[(D, "40-fmt.txt")] = "$ cargo fmt --all -- --check\n"
    _f, reds = evaluate([D], *mk(files_norc))
    expect("只有命令头无 rc -> NO_RC RED", any(x.startswith("NO_RC ") for x in reds))

    files_nofmt = dict(files)
    del files_nofmt[(D, "40-fmt.txt")]
    files_nofmt[(D, "01-boundary.txt")] = "x\n"
    _f, reds = evaluate([D], *mk(files_nofmt))
    expect("包内无 fmt 证据 -> NO_FMT_EVIDENCE RED", any(x.startswith("NO_FMT_EVIDENCE ") for x in reds))

    _f, reds = evaluate(["pkgB"], *mk({}, missing={"pkgB"}))
    expect("登记包缺失 -> MISSING_DIR RED", any(x.startswith("MISSING_DIR ") for x in reds))

    # 非 .txt 或名不含 fmt 的文件不参与（避免误伤其它证据体例）
    files_other = {(D, "41-clippy.txt"): "raw cargo output\n", (D, "40-fmt.txt"): GOOD}
    _f, reds = evaluate([D], *mk(files_other))
    expect("只审 *fmt*.txt（raw 证据不判红）", reds == [])

    print("SELFTEST {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


# --------------------------------------------------------------------------- #
# 改坏必红：临时目录造包 -> 抹空 / 删命令头 / 删 rc
def cmd_mutation(_args):
    ok, total = 0, 0

    def expect(name, cond):
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
            print("  MUTATION ok  {0}".format(name))
        else:
            print("  MUTATION FAIL {0}".format(name))

    good = "$ cargo fmt --all -- --check\nrc=0\n"
    tmp = tempfile.mkdtemp(prefix="fixture-fmt-evidence-mut-")
    try:
        # 真身树复制两处欠件目录到临时 root，再按外部改坏复现。
        rel = "mir2-rs/tests/parity/m3/evidence-m3-anchor-prefix-o2-20261014"
        os.makedirs(os.path.join(tmp, rel), exist_ok=True)
        target = os.path.join(tmp, rel, "40-fmt.txt")

        def run():
            def lister(d):
                p = os.path.join(tmp, d)
                if not os.path.isdir(p):
                    return None
                out = []
                for root, _dirs, files in os.walk(p):
                    for f in files:
                        out.append(os.path.relpath(os.path.join(root, f), p).replace(os.sep, "/"))
                return sorted(out)

            def reader(d, n):
                with open(os.path.join(tmp, d, n), "r", encoding="utf-8", errors="replace") as fh:
                    return fh.read()
            return evaluate([rel], lister, reader)

        with open(target, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(good)
        _f, reds = run()
        expect("真身体例（临时副本）-> GREEN", reds == [])

        with open(target, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("")
        _f, reds = run()
        expect("改坏 MUT1：抹空 -> EMPTY RED", any(x.startswith("EMPTY ") for x in reds))

        with open(target, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("rc=0\n")
        _f, reds = run()
        expect("改坏 MUT2：删命令头 -> NO_COMMAND RED", any(x.startswith("NO_COMMAND ") for x in reds))

        with open(target, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("$ cargo fmt --all -- --check\n")
        _f, reds = run()
        expect("改坏 MUT3：删 rc -> NO_RC RED", any(x.startswith("NO_RC ") for x in reds))

        os.remove(target)
        _f, reds = run()
        expect("改坏 MUT4：删 fmt 证据件 -> NO_FMT_EVIDENCE RED",
               any(x.startswith("NO_FMT_EVIDENCE ") for x in reds))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    print("MUTATION {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


def main(argv=None):
    p = argparse.ArgumentParser(description="M3 夹具链族 fmt 证据自描述门（只读。零端口）")
    sub = p.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check").set_defaults(fn=cmd_check)
    sub.add_parser("selftest").set_defaults(fn=cmd_selftest)
    sub.add_parser("mutation").set_defaults(fn=cmd_mutation)
    args = p.parse_args(argv)
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
