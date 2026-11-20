#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""按 **git blob** 复算夹具声明的 `oracle_source_sha256`（C1 行尾归一化，2026-11-20）。

背景：`eat-probe` / `oil-probe` 里的 `SourceAnchorSha256` 用**工作树文件字节**算锚点；
Windows 上 `core.autocrlf=true` 时工作树是 CRLF、git blob 是 LF ⇒ 同一 commit 会算出两个值
（工作树 `c8a18e59…` vs blob `51a289e2…`）。夹具里声明的值是**按 blob（LF）**算的规则，
本脚本让外部读者不必依赖平台也能自证夹具没陈旧。

算法（与 C# `SourceAnchorSha256` 逐字节一致）：

    h = sha256()
    for rel in files:            # 顺序 = 夹具里的 oracle_source_sha256_files
        h.update(utf8(rel + "\\n"))
        h.update(git_blob_bytes(rel))
        h.update(b"\\n")

用法（仓库根）：

    python -I -B tools/parity/oracle_source_sha.py \
        --fixture mir2-rs/tests/parity/m3/fixtures/eat_vectors.json
    python -I -B tools/parity/oracle_source_sha.py --selftest

退出码：0 = 与声明一致；1 = 不一致（打印两边）；2 = 用法/读文件/git 错误。
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile


def blob_bytes(repo, rel, rev="HEAD"):
    r = subprocess.run(
        ["git", "-C", repo, "show", "%s:%s" % (rev, rel)],
        capture_output=True,
    )
    if r.returncode != 0:
        raise RuntimeError("git show %s:%s 失败：%s" % (rev, rel, r.stderr.decode("utf-8", "replace").strip()))
    return r.stdout


def compute(repo, files, rev="HEAD"):
    h = hashlib.sha256()
    for rel in files:
        h.update(rel.encode("utf-8") + b"\n")
        h.update(blob_bytes(repo, rel, rev))
        h.update(b"\n")
    return h.hexdigest()


def load_fixture(path):
    with open(path, encoding="utf-8") as f:
        doc = json.load(f)
    files = doc.get("oracle_source_sha256_files")
    want = doc.get("oracle_source_sha256")
    if not isinstance(files, list) or not files or not isinstance(want, str):
        raise ValueError("夹具缺 oracle_source_sha256 / oracle_source_sha256_files")
    return files, want


def selftest():
    root = tempfile.mkdtemp(prefix="oracle-src-sha-")
    try:
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "-C", root, "config", "user.email", "t@t"], check=True)
        subprocess.run(["git", "-C", root, "config", "user.name", "t"], check=True)
        files = ["src/a.cs", "src/b.cs"]
        os.makedirs(os.path.join(root, "src"))
        for rel, text in zip(files, ["alpha\n", "beta\n"]):
            with open(os.path.join(root, rel), "w", encoding="utf-8", newline="\n") as f:
                f.write(text)
        subprocess.run(["git", "-C", root, "add", "-A"], check=True)
        subprocess.run(["git", "-C", root, "commit", "-qm", "x"], check=True)

        want = compute(root, files)
        # 负例 1：改声明值 ⇒ 必须不一致
        bad = compute(root, list(reversed(files)))
        if want == bad:
            print("SELFTEST fail: 顺序不敏感（算法与 C# 不一致）")
            return 1
        # 负例 2：把工作树改成 CRLF，blob 算法必须**不受影响**
        with open(os.path.join(root, files[0]), "w", encoding="utf-8", newline="\r\n") as f:
            f.write("alpha\r\n")
        if compute(root, files) != want:
            print("SELFTEST fail: 工作树 CRLF 影响了 blob 计算")
            return 1
        # 正例：夹具 round-trip
        fx = os.path.join(root, "fx.json")
        with open(fx, "w", encoding="utf-8") as f:
            json.dump({"oracle_source_sha256": want, "oracle_source_sha256_files": files}, f)
        got_files, got_want = load_fixture(fx)
        if got_want != want or compute(root, got_files) != want:
            print("SELFTEST fail: 夹具 round-trip 不一致")
            return 1
        print("SELFTEST ok: blob 计算、顺序敏感、CRLF 不敏感、夹具 round-trip")
        return 0
    finally:
        shutil.rmtree(root, ignore_errors=True)


def main(argv=None):
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--fixture")
    ap.add_argument("--repo", default=".")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.fixture:
        print(__doc__)
        return 2
    try:
        files, want = load_fixture(args.fixture)
        got = compute(os.path.abspath(args.repo), files)
    except (OSError, ValueError, RuntimeError) as e:
        print("错误：%s" % e, file=sys.stderr)
        return 2
    print("declared %s" % want)
    print("blob     %s" % got)
    if got == want:
        print("MATCH")
        return 0
    print("MISMATCH（夹具陈旧或源文件已变）")
    return 1


if __name__ == "__main__":
    sys.exit(main())
