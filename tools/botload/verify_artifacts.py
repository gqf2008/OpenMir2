#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""复核 `tests/loadgate/**` 的 `artifacts.sha256.json` 产物清单（C6 F9 / F9-1）。

哈希口径（与 `tests/loadgate/.gitattributes` 的 `-text` 一致，必须成对读）
--------------------------------------------------------------------
    sha256 = SHA256(文件的**原始字节**)，**不做**任何 EOL / 编码归一化。

为什么这样就够了：`tests/loadgate/**` 被 `.gitattributes` 的 `* -text` **冻结字节**，
git 原样存、原样检出（与 `core.autocrlf` 无关）⇒ 清单生成当时哈希的字节，
就是提交的字节，也是任何全新检出得到的字节。所以「在全新检出上重算清单」必须
**0 mismatch**；一旦 mismatch = 某个被引用的证据件真的被覆写了（或清单坏了）。

用法
----
    python tools/botload/verify_artifacts.py <artifacts.sha256.json> [<...> ...]
        [--root DIR]   # 省略时，每条清单以「清单文件所在目录」为根解析 file 字段
    python tools/botload/verify_artifacts.py tests/loadgate/load-*/artifacts.sha256.json

退出码：0 = 全部清单 0 mismatch；1 = 有 mismatch / 缺件 / 读不了的清单。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fp:
        for chunk in iter(lambda: fp.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def verify_one(manifest_path: str, root: str | None) -> tuple[int, int]:
    """返回 (文件总数, mismatch 数)。逐条打印 mismatch/缺失。"""
    # 清单本身可能是 PowerShell `Set-Content -Encoding utf8` 写的（带 UTF-8 BOM）⇒ utf-8-sig。
    with open(manifest_path, "rb") as fp:
        doc = json.loads(fp.read().decode("utf-8-sig"))
    files = doc.get("files")
    if not isinstance(files, list):
        print(f"[BAD] {manifest_path}: 缺 files 数组")
        return 0, 1

    base = root if root is not None else os.path.dirname(os.path.abspath(manifest_path))
    total = 0
    bad = 0
    for entry in files:
        total += 1
        rel = entry.get("file", "")
        want = str(entry.get("sha256", "")).lower()
        path = os.path.join(base, rel.replace("/", os.sep))
        if not os.path.isfile(path):
            print(f"[MISSING]  {rel}  (期望 {want})")
            bad += 1
            continue
        got = sha256_file(path)
        if got != want:
            print(f"[MISMATCH] {rel}\n           期望 {want}\n           实际 {got}")
            bad += 1
    print(f"[{'OK' if bad == 0 else 'FAIL'}] {manifest_path}  {total - bad}/{total} 匹配")
    return total, bad


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="复核 artifacts.sha256.json（字节口径，见 tests/loadgate/.gitattributes）")
    ap.add_argument("manifests", nargs="+", help="artifacts.sha256.json 路径（可多个）")
    ap.add_argument("--root", default=None, help="可选：file 字段的根目录（默认=清单所在目录）")
    args = ap.parse_args(argv)

    grand_total = 0
    grand_bad = 0
    bad_manifests = 0
    for m in args.manifests:
        total, bad = verify_one(m, args.root)
        grand_total += total
        grand_bad += bad
        if bad:
            bad_manifests += 1

    print(
        f"verify_artifacts: {'OK' if grand_bad == 0 else 'FAIL'} "
        f"{len(args.manifests) - bad_manifests}/{len(args.manifests)} 清单, "
        f"{grand_total - grand_bad}/{grand_total} 文件 0 mismatch 期望"
    )
    return 0 if grand_bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
