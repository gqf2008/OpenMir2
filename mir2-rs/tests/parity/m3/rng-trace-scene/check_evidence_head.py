#!/usr/bin/env python3
"""证据包「声明 head」一致性机判（reviewer-6 O-1 收口，卡 openmir2-m3-rng-trace-scene-groups）。

背景（reviewer-6 在 approve head `2591a7b31` 时记的 O-1，observation / 非阻塞）：
`evidence-m3-rng-scene-anchor-teeth-20261014/README.md §5` 与 `11-boundary.txt`
把本包的「head」写成 **merge commit `2fcebc6cd`**，而**被交付/被审的 head 是 `2591a7b31`**
（其上再一提交「证据 05–11 在合并树重生成」）⇒ 消费方按证据里写的 oid 定位，会落到一个
**非交付 head** 上（实质影响为零——两者对 `origin/master..HEAD` 的路径集合同为 19，
判据/冻结面/零 `.rs` 结论不变；但书证与事实不一致）。

本工具把「**证据包声明的 head 必须是可解析的 commit，且从该 head 到当前 HEAD 之间
只能有『证据/夹具侧』变更**」变成**机判**（fail-closed）：

- 声明的 head ⇒ `git rev-parse --verify <head>^{commit}` 必须成功（否则 `HEAD_UNRESOLVABLE`）；
- `<head>` 必须是当前 `HEAD` 的**祖先**（否则 `HEAD_NOT_ANCESTOR`）；
- `git diff --name-only <head>..HEAD` 的每一条路径，必须落在**本证据目录**下，
  或命中任一 `--allow-prefix`（缺省 `mir2-rs/tests/parity/m3/`）——
  即「自声明 head 之后，交付只动了 m3 证据/夹具侧，没有夹带生产代码」；
  任何越界路径 ⇒ 逐条点名 RED。

这样证据包只需诚实写「本包读数在 head `<oid>` 的树上取得，其后至交付 head 仅证据侧变更」，
就能自洽且可被机判（避免「声明 head 永远落后交付 head 一格」的自指死结）。

**局限性（如实登记）**：本门咬住的是「oid 写错/不可解析、非 HEAD 祖先、声明后夹带生产代码」三类；
它**咬不住** O-1 原本那一类——「声明的是**真实祖先**、但不是读数/交付 head」（`2fcebc6cd` 就是这种）。
那一类属「书证命名」，只能靠人写对 + 评审；本门把其余可机判的相邻失败模式钉死，与文本订正**互补**。

用法（cwd = `mir2-rs/tests/parity/m3`）：
    python -I rng-trace-scene/check_evidence_head.py check \
        --dir    evidence-m3-rng-scene-anchor-teeth-20261014 \
        --head   2591a7b31
    python -I rng-trace-scene/check_evidence_head.py selftest

退出码：0 = GREEN；1 = RED（点名）；2 = PREREQ（缺件 / 坏输入 / 不是 git 工作树）。
"""

import argparse
import contextlib
import io
import os
import subprocess
import sys
import tempfile

DEFAULT_ALLOW_PREFIXES = ("mir2-rs/tests/parity/m3/",)


def _git(repo, *args):
    """在 `repo` 里跑只读 git 命令，返回 (rc, stdout)。"""
    try:
        p = subprocess.run(["git", "-C", repo] + list(args),
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    except OSError as exc:  # git 不在 PATH
        return 127, "git 不可执行：%s" % exc
    out = p.stdout.decode("utf-8", "replace")
    if p.returncode != 0:
        out = out + p.stderr.decode("utf-8", "replace")
    return p.returncode, out


def _repo_root(repo):
    rc, out = _git(repo, "rev-parse", "--show-toplevel")
    if rc != 0:
        return None
    return out.strip().replace("\\", "/")


def _norm(path):
    return os.path.normpath(path).replace("\\", "/").lstrip("./")


def check(repo, ev_dir, head, allow_prefixes):
    """返回 (rc, problems)。"""
    problems = []
    root = _repo_root(repo)
    if root is None:
        return 2, ["PREREQ_MISSING: %s 不是 git 工作树 / git 不可用" % repo]
    if not ev_dir or not os.path.isdir(ev_dir):
        return 2, ["PREREQ_MISSING: 找不到证据目录 %s" % (ev_dir or "(空)",)]
    if not head:
        return 2, ["PREREQ_MISSING: 缺 --head"]

    # 证据目录 → 仓库根相对路径（POSIX 口径）
    ev_abs = os.path.abspath(ev_dir)
    ev_rel = _norm(os.path.relpath(ev_abs, root))

    # ① 声明的 head 必须解析成 commit
    rc, out = _git(root, "rev-parse", "--verify", "--quiet", "%s^{commit}" % head)
    if rc != 0 or not out.strip():
        return 1, ["HEAD_UNRESOLVABLE: 声明 head %r 不是可解析的 commit" % head]
    oid = out.strip().splitlines()[0]

    # ② 必须是当前 HEAD 的祖先
    rc, out = _git(root, "merge-base", "--is-ancestor", oid, "HEAD")
    if rc != 0:
        return 1, ["HEAD_NOT_ANCESTOR: 声明 head %s 不是当前 HEAD 的祖先" % oid]

    # ③ head..HEAD 只允许证据/夹具侧变更
    rc, out = _git(root, "diff", "--name-only", "%s..HEAD" % oid)
    if rc != 0:
        return 2, ["PREREQ_MISSING: git diff %s..HEAD 失败：%s" % (oid, out.strip())]
    changed = [ln.strip() for ln in out.splitlines() if ln.strip()]

    prefixes = tuple(_norm(p) for p in (allow_prefixes or DEFAULT_ALLOW_PREFIXES))
    ev_prefix = ev_rel + "/"
    outside = []
    for path in changed:
        p = _norm(path)
        if p == ev_rel or p.startswith(ev_prefix):
            continue
        if any(p.startswith(pref) for pref in prefixes):
            continue
        outside.append(p)

    print("EVIDENCE_HEAD dir=%s head=%s oid=%s changed_since=%d allow_prefixes=%s"
          % (ev_rel, head, oid, len(changed), list(prefixes)))
    if outside:
        for p in outside:
            print("  RED HEAD_SCOPE_DRIFT: 声明 head 之后改了非证据侧路径 %s" % p)
        problems.extend("HEAD_SCOPE_DRIFT: %s" % p for p in outside)
        return 1, problems
    print("  OK 声明 head 可解析、是 HEAD 祖先，且其后变更全在证据侧（零生产代码）")
    return 0, []


# ----------------------------- selftest -----------------------------

def _write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)


def _commit(repo, msg):
    _git(repo, "add", "-A")
    rc, out = _git(repo, "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", msg)
    assert rc == 0, "git commit 失败：%s" % out
    rc, out = _git(repo, "rev-parse", "HEAD")
    assert rc == 0, out
    return out.strip()


def _init_repo(tmp):
    repo = os.path.join(tmp, "repo")
    os.makedirs(repo)
    rc, out = _git(repo, "init", "-q")
    assert rc == 0, out
    return repo


def _quiet(fn):
    """吞掉 check() 的正常打印，只取返回值。"""
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        return fn()


def cmd_selftest(_args):
    ok = 0
    failed = []

    def case(name, fn):
        nonlocal ok
        try:
            fn()
            ok += 1
        except AssertionError as exc:
            failed.append((name, str(exc)))

    tmp = tempfile.mkdtemp(prefix="evhead-")
    repo = None
    try:
        repo = _init_repo(tmp)
        base = "mir2-rs/tests/parity/m3"
        prod = "mir2-rs/bins/game-svr/src"          # 生产代码侧（在缺省 allow-prefix 之外）
        ev = os.path.join(repo, base, "ev-synthetic")
        _write(os.path.join(ev, "README.md"), "head = A\n")
        _write(os.path.join(repo, prod, "code.rs"), "fn a() {}\n")
        head_a = _commit(repo, "A")

        # B：只动证据/夹具侧（ev 目录 + m3 下一个工具文件）⇒ 声明的 A 合法
        _write(os.path.join(ev, "README.md"), "head = A\n（B 仅证据侧）\n")
        _write(os.path.join(repo, base, "tool.py"), "# 夹具侧工具\n")
        _commit(repo, "B")

        def _green():
            rc, probs = _quiet(lambda: check(repo, ev, head_a, None))
            assert rc == 0, "expected GREEN rc=0 got %d %s" % (rc, probs)
        case("evidence-only-since-head-green", _green)

        # 改坏①：B 之后又动了生产代码 ⇒ 声明 A 必红并点名
        _write(os.path.join(repo, prod, "code.rs"), "fn a() { /* x */ }\n")
        _commit(repo, "C touch code")

        def _scope_red():
            rc, probs = _quiet(lambda: check(repo, ev, head_a, None))
            assert rc == 1 and any("code.rs" in p for p in probs), (rc, probs)
        case("code-drift-since-head-red", _scope_red)

        # 改坏②：声明一个解析不出来的 oid ⇒ RED，点名 HEAD_UNRESOLVABLE
        def _bogus_red():
            rc, probs = _quiet(
                lambda: check(repo, ev, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", None))
            assert rc == 1 and any("HEAD_UNRESOLVABLE" in p for p in probs), (rc, probs)
        case("unresolvable-head-red", _bogus_red)

        # 改坏③：声明一个非祖先（另一条分歧分支上的提交）⇒ RED，点名 HEAD_NOT_ANCESTOR
        cur_branch = _git(repo, "symbolic-ref", "--short", "HEAD")[1].strip()
        _git(repo, "checkout", "-q", "-b", "side", head_a)
        _write(os.path.join(repo, base, "side.txt"), "side\n")
        side = _commit(repo, "side")
        _git(repo, "checkout", "-q", cur_branch)

        def _not_ancestor_red():
            rc, probs = _quiet(lambda: check(repo, ev, side, None))
            assert rc == 1 and any("HEAD_NOT_ANCESTOR" in p for p in probs), (rc, probs)
        case("non-ancestor-head-red", _not_ancestor_red)

        # 绿向：显式 --allow-prefix 放行生产侧目录前缀 ⇒ 合法（口径可配）
        def _allow_prefix_green():
            rc, probs = _quiet(lambda: check(repo, ev, head_a,
                                             ["mir2-rs/tests/parity/m3/", "mir2-rs/bins/"]))
            assert rc == 0, "expected GREEN rc=0 got %d %s" % (rc, probs)
        case("explicit-allow-prefix-green", _allow_prefix_green)

        # PREREQ：目录不存在
        def _missing_dir_prereq():
            rc, _ = _quiet(lambda: check(repo, os.path.join(repo, base, "nope"), head_a, None))
            assert rc == 2, "expected PREREQ rc=2 got %d" % rc
        case("missing-dir-prereq", _missing_dir_prereq)

        # PREREQ：不是 git 工作树
        def _not_repo_prereq():
            plain = os.path.join(tmp, "plain")
            os.makedirs(plain, exist_ok=True)
            rc, _ = _quiet(lambda: check(plain, plain, head_a, None))
            assert rc == 2, "expected PREREQ rc=2 got %d" % rc
        case("not-a-repo-prereq", _not_repo_prereq)
    finally:
        import shutil
        shutil.rmtree(tmp, ignore_errors=True)

    total = ok + len(failed)
    for name, msg in failed:
        print("  FAIL %s: %s" % (name, msg))
    if failed:
        print("SELFTEST_RED: check_evidence_head %d/%d（失败 %d）" % (ok, total, len(failed)))
        return 1
    print("SELFTEST GREEN: check_evidence_head %d/%d（含 4 条改坏必红 + 2 条 PREREQ）"
          % (ok, total))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(prog="check_evidence_head.py")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_check = sub.add_parser("check", help="核对证据包声明的 head")
    p_check.add_argument("--dir", required=True, help="证据目录")
    p_check.add_argument("--head", required=True, help="证据包声明的 head（commit oid/ref）")
    p_check.add_argument("--repo", default=".", help="git 工作树（缺省 .）")
    p_check.add_argument("--allow-prefix", action="append", dest="allow_prefixes",
                         help="允许在声明 head 之后变更的仓库根相对路径前缀（可重复；缺省 %s）"
                              % list(DEFAULT_ALLOW_PREFIXES))

    sub.add_parser("selftest", help="机判自检（含改坏必红）")

    args = ap.parse_args(argv)
    if args.cmd == "check":
        rc, _ = check(args.repo, args.dir, args.head, args.allow_prefixes)
        return rc
    return cmd_selftest(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
