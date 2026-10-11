#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M4 证据 hash 口径复核器（卡 `openmir2-m4-parity-cases`，评审发现 N-1）。

## 口径（这是本清单唯一权威定义）

本仓库 `core.autocrlf=true`：工作树里是 CRLF，Git blob 里是 LF。证据 hash 一律定义为

    SHA-256( 文件字节，先把 CRLF 归一为 LF )

它**等于** `git show <commit>:<path> | sha256sum`（即 Git blob 内容的 sha256），
但**不等于**对工作树直接跑 `sha256sum` 得到的值（那是 CRLF 值）。
评审 N-1 就是这个口径差：卡/报告里 8 个 hash 是 LF(blob) 值，裸 `sha256sum` 有 2 个对不上。

## 命令（在 `mir2-rs/` 下）

    python -I tests/parity/M4/evidence_hashes.py --write    # 改动证据后显式重生成清单
    python -I tests/parity/M4/evidence_hashes.py            # 复核：逐项比对 + 缺失/多余检查
    python -I tests/parity/M4/evidence_hashes.py selftest   # 牙齿：改 hash / 多文件 / 少文件 ⇒ 必红

`--write` 是**显式**动作，不会在 `m4_parity.py` 里被自动调用：
清单是「被审计证据」的锚点，若每次跑都自动改写，它就退化成恒绿。

退出码：0 = 复核通过；1 = 有差异（红）；2 = 用法/环境错误。
"""

import argparse
import hashlib
import json
import shutil
import sys
import tempfile
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

HERE = Path(__file__).resolve().parent            # mir2-rs/tests/parity/M4
PARITY = HERE.parent                              # mir2-rs/tests/parity
MIR2RS = PARITY.parents[1]                        # mir2-rs/
REPO = MIR2RS.parent                              # 仓库根
EVIDENCE = PARITY / "evidence" / "M4"
MANIFEST_NAME = "evidence-hashes.json"

HASH_SCOPE = (
    "SHA-256(文件字节，先把 CRLF 归一为 LF) == git blob 内容 hash "
    "(`git show <commit>:<path> | sha256sum`)；core.autocrlf=true 下裸 "
    "`sha256sum` 得的是 CRLF 值，不是本口径。"
)


# --------------------------------------------------------------------------- #
# 口径
# --------------------------------------------------------------------------- #
def lf_sha256(path):
    """LF 归一后的 sha256（与 Git blob 内容 hash 同口径）。"""
    data = Path(path).read_bytes().replace(b"\r\n", b"\n")
    return hashlib.sha256(data).hexdigest()


def collect(evidence_dir, repo):
    """扫描证据目录（递归，排除清单自身）→ {仓库相对路径(posix): sha256}。"""
    evidence_dir = Path(evidence_dir)
    out = {}
    if not evidence_dir.is_dir():
        return out
    for p in sorted(evidence_dir.rglob("*")):
        if not p.is_file():
            continue
        if p.name == MANIFEST_NAME:
            continue
        rel = p.relative_to(repo).as_posix()
        out[rel] = lf_sha256(p)
    return out


def verify(manifest, computed):
    """返回问题列表（空 = 绿）。manifest/computed 均为 {path: hash}。"""
    problems = []
    for path, want in sorted(manifest.items()):
        if path not in computed:
            problems.append(f"清单有、证据缺：{path}")
        elif computed[path] != want:
            problems.append(f"hash 不一致：{path}\n    清单 {want}\n    实算 {computed[path]}")
    for path in sorted(computed):
        if path not in manifest:
            problems.append(f"证据有、清单缺：{path}（重生成清单：--write）")
    return problems


def make_manifest(hashes):
    return {
        "_meta": {
            "card": "openmir2-m4-parity-cases",
            "finding": "N-1：证据 hash 口径（LF/blob）与工作树 CRLF 裸 sha256sum 不同",
            "hash_scope": HASH_SCOPE,
            "regenerate": "python -I tests/parity/M4/evidence_hashes.py --write",
            "verify": "python -I tests/parity/M4/evidence_hashes.py",
            "teeth": "python -I tests/parity/M4/evidence_hashes.py selftest",
            "instrument": "tests/parity/M4/evidence_hashes.py",
        },
        "hashes": {k: hashes[k] for k in sorted(hashes)},
    }


# --------------------------------------------------------------------------- #
# 子命令
# --------------------------------------------------------------------------- #
def cmd_write(manifest_path, evidence_dir, repo):
    hashes = collect(evidence_dir, repo)
    if not hashes:
        print(f"证据目录为空/不存在：{evidence_dir}")
        return 2
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(make_manifest(hashes), ensure_ascii=False, indent=2, sort_keys=False) + "\n"
    manifest_path.write_text(text, encoding="utf-8", newline="\n")
    print(f"已写入 {manifest_path}（{len(hashes)} 条，口径 = LF 归一 sha256）")
    return 0


def cmd_verify(manifest_path, evidence_dir, repo):
    if not manifest_path.exists():
        print(f"缺清单：{manifest_path}（先跑 --write）")
        return 2
    manifest = json.loads(manifest_path.read_text(encoding="utf-8")).get("hashes", {})
    computed = collect(evidence_dir, repo)
    problems = verify(manifest, computed)
    for p in problems:
        print("RED " + p)
    if problems:
        print(f"RED: 证据 hash 复核 {len(problems)} 项不过 "
              f"（口径：{HASH_SCOPE}）")
        return 1
    print(f"GREEN: 证据 hash 复核 {len(computed)}/{len(manifest)} 项一致"
          f"（口径 = LF 归一 sha256 == git blob hash）")
    return 0


def cmd_selftest(_args):
    tmp = Path(tempfile.mkdtemp(prefix="m4-hash-selftest-"))
    try:
        ev = tmp / "evidence"
        (ev / "sub").mkdir(parents=True)
        (ev / "a.txt").write_bytes(b"line1\r\nline2\r\n")   # 工作树 CRLF
        (ev / "sub" / "b.txt").write_bytes(b"x\ny\n")
        # 口径自检：LF 归一值 == 把 CRLF 换成 LF 后的 sha256
        want_a = hashlib.sha256(b"line1\nline2\n").hexdigest()
        got_a = lf_sha256(ev / "a.txt")
        ok_a = got_a == want_a
        print(f"{'PASS' if ok_a else 'RED '} 0/4 口径：CRLF 工作树文件的 LF 归一 "
              f"sha256 == git blob 口径")
        if not ok_a:
            return 2

        good = collect(ev, tmp)
        cases = [
            ("1/4 清单与证据一致 → 绿", good, []),
            ("2/4 改 1 位 hash → 必红",
             {**good, "evidence/a.txt": "0" * 64}, ["hash 不一致：evidence/a.txt"]),
            ("3/4 证据多一个文件 → 必红",
             {k: v for k, v in good.items() if k != "evidence/sub/b.txt"},
             ["证据有、清单缺：evidence/sub/b.txt"]),
            ("4/4 清单多一个条目 → 必红",
             {**good, "evidence/ghost.txt": "1" * 64},
             ["清单有、证据缺：evidence/ghost.txt"]),
        ]
        bad = 0
        for name, manifest, want_sub in cases:
            problems = verify(manifest, collect(ev, tmp))
            hit = all(any(w in p for p in problems) for w in want_sub)
            ok = (problems == [] and want_sub == []) or (problems != [] and hit)
            bad += 0 if ok else 1
            print(f"{'PASS' if ok else 'RED '} {name}")
            for p in problems:
                print("     " + p.splitlines()[0])
        if bad:
            print(f"RED: evidence_hashes selftest {bad} 项不过")
            return 2
        print("GREEN: evidence_hashes selftest 4/4（口径 + 改 hash / 多文件 / 少文件必红）")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description="M4 证据 hash 口径复核器（LF 归一 = git blob）")
    ap.add_argument("--manifest", default=str(EVIDENCE / MANIFEST_NAME))
    ap.add_argument("--evidence", default=str(EVIDENCE))
    ap.add_argument("--repo", default=str(REPO))
    ap.add_argument("--write", action="store_true",
                    help="重生成清单（改动证据后显式执行）")
    ap.add_argument("cmd", nargs="?", default="verify",
                    choices=["verify", "write", "selftest"])
    args = ap.parse_args()
    if args.write or args.cmd == "write":
        return cmd_write(Path(args.manifest), Path(args.evidence), Path(args.repo))
    if args.cmd == "selftest":
        return cmd_selftest(args)
    return cmd_verify(Path(args.manifest), Path(args.evidence), Path(args.repo))


if __name__ == "__main__":
    raise SystemExit(main())
