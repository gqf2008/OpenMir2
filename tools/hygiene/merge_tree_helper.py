#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""merge_tree_helper.py — 由 git 机判 patch body 里的 merge_tree 值（不手算）。

背景：复核连续发现 4 张卡的 patch body 把 `git merge-tree --write-tree` 的 tree oid
记错（frame-queue×2 / 税 / inbound）。本工具只调用 git、不改变 git 行为，输出两个参照位点：

  * `vs_base`   = `git merge-tree --write-tree <base> <head>`（patch 的 base 对 head）
  * `vs_master` = `git merge-tree --write-tree <master> <head>`（当前主源对 head）

两者都可能因冲突返回 rc=1；tree oid 仍会输出（冲突 tree）。**patch body 必须附本脚本输出**，
评审按同一命令复算即得同一值。

用法：
  python -I -B tools/hygiene/merge_tree_helper.py --base <rev> --head <rev>
  python -I -B tools/hygiene/merge_tree_helper.py --base <rev> --head <rev> --json
  python -I -B tools/hygiene/merge_tree_helper.py --base <rev> --head <rev> --strict
  python -I -B tools/hygiene/merge_tree_helper.py --selftest

  选项：
    --repo PATH      仓库根（默认当前目录）
    --master REV     主源参照（默认 refs/remotes/walgit/master → master）
    --strict         有冲突（任一 rc=1）时本进程 rc=1；默认 rc=0（只负责算出值）
    --json           输出 JSON（stdout 只有 JSON）
    --selftest       离线自测（临时仓库：两 clean + 一 conflict）

输出（文本模式，逐行可 grep）：
  MERGE_TREE base=<40hex> head=<40hex> master=<40hex>
  MERGE_TREE vs_base tree=<40hex> rc=<0|1>
  MERGE_TREE vs_master tree=<40hex> rc=<0|1>
  MERGE_TREE same=<yes|no>

退出码：0 = 已算出两个值；1 = `--strict` 且有冲突；2 = 用法/git 错误（fail-closed）。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

TREE_RE = re.compile(r"^[0-9a-f]{40}$")


class HelperError(Exception):
    pass


def _git(repo, *args, check=True):
    proc = subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise HelperError("git %s rc=%d: %s" % (
            " ".join(args), proc.returncode,
            (proc.stderr or proc.stdout).strip()[:300]))
    return proc


def resolve_commit(repo, rev):
    proc = _git(repo, "rev-parse", "--verify", "%s^{commit}" % rev)
    return proc.stdout.strip()


def _is_ancestor(repo, a, b):
    """a 是 b 的祖先（或相等）⇒ True。"""
    return _git(repo, "merge-base", "--is-ancestor", a, b, check=False).returncode == 0


def resolve_master(repo, explicit=None):
    """返回 {sha, source, candidates, warnings}。

    故障期（walgit 不可用）`refs/remotes/walgit/master` 可能是**陈旧祖先**。未显式给
    `--master` 时：取候选里「是其余所有候选后代」的那一个（R-4）；若候选分叉则回落本地
    `master` 并 WARN。显式 `--master` 永远优先、不做自动纠正。
    """
    if explicit:
        return {"sha": resolve_commit(repo, explicit), "source": "explicit",
                "candidates": [], "warnings": []}
    by_sha = {}
    for ref in ("refs/remotes/walgit/master", "refs/heads/master", "master"):
        proc = _git(repo, "rev-parse", "--verify", ref, check=False)
        sha = proc.stdout.strip() if proc.returncode == 0 else ""
        if sha:
            by_sha.setdefault(sha, ref)
    if not by_sha:
        raise HelperError("找不到主源参照（试过 refs/remotes/walgit/master、refs/heads/master、"
                          "master；用 --master 指定）")
    candidates = [{"ref": ref, "sha": sha} for sha, ref in by_sha.items()]
    warnings = []
    if len(candidates) == 1:
        chosen = candidates[0]
    else:
        descendants = [
            c for c in candidates
            if all(d["sha"] == c["sha"] or _is_ancestor(repo, d["sha"], c["sha"])
                   for d in candidates)
        ]
        local = next((c for c in candidates if c["ref"] in ("refs/heads/master", "master")),
                     candidates[-1])
        if descendants:
            chosen = descendants[0]
            others = [c for c in candidates if c["sha"] != chosen["sha"]]
            warnings.append(
                "主源候选不一致：取后代 %s=%s（忽略祖先/陈旧：%s）；故障期建议显式 --master master"
                % (chosen["ref"], chosen["sha"][:12],
                   ", ".join("%s=%s" % (c["ref"], c["sha"][:12]) for c in others)))
        else:
            chosen = local
            warnings.append(
                "主源候选分叉：回落本地 %s=%s；请显式 --master 指定（候选：%s）"
                % (local["ref"], local["sha"][:12],
                   ", ".join("%s=%s" % (c["ref"], c["sha"][:12]) for c in candidates)))
    return {"sha": chosen["sha"], "source": chosen["ref"],
            "candidates": candidates, "warnings": warnings}


def merge_tree(repo, base, head):
    """返回 {tree, rc, stdout, stderr}；rc 只允许 0（clean）或 1（conflict）。"""
    proc = _git(repo, "merge-tree", "--write-tree", base, head, check=False)
    if proc.returncode not in (0, 1):
        raise HelperError("git merge-tree rc=%d: %s" % (
            proc.returncode, (proc.stderr or proc.stdout).strip()[:300]))
    lines = [ln.strip() for ln in proc.stdout.splitlines() if ln.strip()]
    if not lines or not TREE_RE.match(lines[0]):
        raise HelperError("git merge-tree 输出首行不是 tree oid：%r" %
                          (proc.stdout[:200],))
    return {"tree": lines[0], "rc": proc.returncode,
            "stdout": proc.stdout, "stderr": proc.stderr}


def compute(repo, base_rev, head_rev, master_rev=None):
    repo = os.path.abspath(repo)
    base = resolve_commit(repo, base_rev)
    head = resolve_commit(repo, head_rev)
    m = resolve_master(repo, master_rev)
    master = m["sha"]
    vs_base = merge_tree(repo, base, head)
    vs_master = merge_tree(repo, master, head)
    return {
        "format": "merge-tree-helper/1",
        "repo": repo,
        "base": base,
        "head": head,
        "master": master,
        "master_source": m["source"],
        "master_candidates": m["candidates"],
        "warnings": m["warnings"],
        "vs_base": {"tree": vs_base["tree"], "rc": vs_base["rc"]},
        "vs_master": {"tree": vs_master["tree"], "rc": vs_master["rc"]},
        "same": vs_base["tree"] == vs_master["tree"]
        and vs_base["rc"] == vs_master["rc"],
    }


def emit(doc, as_json):
    if as_json:
        print(json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True))
        return
    print("MERGE_TREE base=%s head=%s master=%s" %
          (doc["base"], doc["head"], doc["master"]))
    print("MERGE_TREE master_source=%s" % doc["master_source"])
    print("MERGE_TREE master_candidates=%s" %
          ",".join("%s=%s" % (c["ref"], c["sha"]) for c in doc["master_candidates"]))
    for w in doc["warnings"]:
        print("WARN merge_tree_helper: %s" % w, file=sys.stderr)
    print("MERGE_TREE vs_base tree=%s rc=%d" %
          (doc["vs_base"]["tree"], doc["vs_base"]["rc"]))
    print("MERGE_TREE vs_master tree=%s rc=%d" %
          (doc["vs_master"]["tree"], doc["vs_master"]["rc"]))
    print("MERGE_TREE same=%s" % ("yes" if doc["same"] else "no"))


# ---------------------------------------------------------------------------
# --selftest：临时仓库，两 clean + 一 conflict；值与直接 git 输出逐字比对
# ---------------------------------------------------------------------------
def _write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)


def _commit(repo, msg):
    _git(repo, "add", "-A")
    _git(repo, "commit", "-q", "-m", msg)
    return _git(repo, "rev-parse", "HEAD").stdout.strip()


def selftest():
    pass_n = fail_n = 0

    def chk(cond, name):
        nonlocal pass_n, fail_n
        if cond:
            pass_n += 1
            print("PASS %s" % name)
        else:
            fail_n += 1
            print("FAIL %s" % name)

    tmp = tempfile.mkdtemp(prefix="merge-tree-helper-selftest.")
    try:
        repo = os.path.join(tmp, "repo")
        os.makedirs(repo)
        _git(repo, "init", "-q", "-b", "master")
        _git(repo, "config", "user.email", "selftest@example.invalid")
        _git(repo, "config", "user.name", "selftest")
        _write(os.path.join(repo, "a.txt"), "base\n")
        base = _commit(repo, "base")

        # clean 1：分支只新增文件（vs base 为 ff/clean）
        _git(repo, "checkout", "-q", "-b", "clean1", base)
        _write(os.path.join(repo, "f1.txt"), "one\n")
        clean1 = _commit(repo, "clean1")

        # clean 2：另一分支新增不同文件（clean1 vs clean2 无冲突）
        _git(repo, "checkout", "-q", "-b", "clean2", base)
        _write(os.path.join(repo, "f2.txt"), "two\n")
        clean2 = _commit(repo, "clean2")

        # conflict：两分支改同一行
        _git(repo, "checkout", "-q", "-b", "c1", base)
        _write(os.path.join(repo, "a.txt"), "left\n")
        c1 = _commit(repo, "c1")
        _git(repo, "checkout", "-q", "-b", "c2", base)
        _write(os.path.join(repo, "a.txt"), "right\n")
        c2 = _commit(repo, "c2")

        d1 = compute(repo, base, clean1, master_rev=clean1)
        chk(d1["vs_base"]["rc"] == 0 and TREE_RE.match(d1["vs_base"]["tree"]) is not None,
            "clean 1：base→clean1 rc=0 且 tree 为 40hex")
        chk(d1["vs_base"]["tree"] == _git(repo, "rev-parse", clean1 + "^{tree}").stdout.strip(),
            "clean 1：tree 与 git rev-parse 逐字一致")

        d2 = compute(repo, clean1, clean2, master_rev=clean2)
        chk(d2["vs_base"]["rc"] == 0 and d2["vs_master"]["rc"] == 0,
            "clean 2：clean1 vs clean2 两个参照 rc=0")
        direct = _git(repo, "merge-tree", "--write-tree", clean1, clean2)
        chk(d2["vs_base"]["tree"] == direct.stdout.splitlines()[0].strip(),
            "clean 2：tree 与直接 git merge-tree 逐字一致")

        # R-4：陈旧 remote-tracking（祖先）vs 本地 master（后代）⇒ 未显式 --master 时取后代 + WARN
        _git(repo, "update-ref", "refs/remotes/walgit/master", base)
        _git(repo, "update-ref", "refs/heads/master", clean2)
        d_stale = compute(repo, base, clean1)
        chk(d_stale["master"] == clean2,
            "R-4：陈旧 remote-tracking 被后代本地 master 取代")
        chk(len(d_stale["master_candidates"]) >= 2,
            "R-4：输出 master_candidates（两个候选）")
        chk(any("后代" in w for w in d_stale["warnings"]),
            "R-4：WARN 说明取后代/忽略陈旧")
        chk(d_stale["vs_master"]["rc"] == 0
            and TREE_RE.match(d_stale["vs_master"]["tree"]) is not None,
            "R-4：vs_master 用后代本地 master 计算（rc=0、tree 40hex）")
        d_explicit = compute(repo, base, clean1, master_rev=base)
        chk(d_explicit["master"] == base and d_explicit["master_source"] == "explicit",
            "R-4：显式 --master 优先、不自动纠正")

        d3 = compute(repo, c1, c2, master_rev=c1)
        chk(d3["vs_base"]["rc"] == 1 and d3["vs_master"]["rc"] == 1,
            "conflict：c1 vs c2 两个参照 rc=1")
        chk(TREE_RE.match(d3["vs_base"]["tree"]) is not None,
            "conflict：仍输出 tree oid（冲突 tree）")
        chk(d3["same"] is True, "conflict：两个参照 tree/rc 相同 ⇒ same=yes")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if fail_n:
        print("RED: merge_tree_helper selftest %d/%d" % (pass_n, pass_n + fail_n))
        return 1
    print("GREEN: merge_tree_helper selftest %d/%d" % (pass_n, pass_n + fail_n))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="由 git 机判 patch body 的 merge_tree 值（vs_base / vs_master）")
    ap.add_argument("--repo", default=".", help="仓库根（默认当前目录）")
    ap.add_argument("--base", help="patch base（rev）")
    ap.add_argument("--head", default="HEAD", help="交付 head（默认 HEAD）")
    ap.add_argument("--master", default=None,
                    help="主源参照（默认 refs/remotes/walgit/master → master）")
    ap.add_argument("--strict", action="store_true",
                    help="任一参照 rc=1（冲突）时本进程 rc=1")
    ap.add_argument("--json", action="store_true", help="stdout 只输出 JSON")
    ap.add_argument("--selftest", action="store_true", help="离线自测（临时仓库）")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if not args.base:
        ap.error("缺 --base（patch 的 base rev）")
    try:
        doc = compute(args.repo, args.base, args.head, args.master)
    except HelperError as ex:
        print("merge_tree_helper: %s" % ex, file=sys.stderr)
        return 2
    emit(doc, args.json)
    if args.strict and (doc["vs_base"]["rc"] or doc["vs_master"]["rc"]):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
