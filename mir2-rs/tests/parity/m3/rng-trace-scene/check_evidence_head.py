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

**局限性（如实登记）**：`check` 子命令咬住的是「oid 写错/不可解析、非 HEAD 祖先、声明后夹带生产代码」三类；
它**咬不住** O-1 原本那一类——「声明的是**真实祖先**、但不是读数/交付 head」（`2fcebc6cd` 就是这种）。

**`check-pack` 子命令（reviewer-5 对 head `9740b7d0c` 的 observation O-1 收口）**：把「书证 head 与
机判对象一致」补上——**机判对象 = 包内声明的 head**（从包内 `--decl-file`/缺省探测读 `head = <oid>`，
不再由 CLI 另传一个值冒充声明 head），并用**显式 `--since` 锚**界定差量 `since..声明 head`
（只允许证据/夹具侧）。收益：① 校验的正是书证里写的那个 oid；② **免疫「master 前进后的无关合并」
造成的假红**（`check` 的 `head..HEAD` 口径在 master 前进后会把别人的合并算进来；
`since..声明 head` 只看本片自己的差量）；③ 声明 head 写成**不可解析 / 非 HEAD 祖先 / 自 since 起
夹带生产代码** 的三类仍各自点名。**残留局限（如实登记）**：把**证据侧的其它祖先**（如中间 merge
commit `2fcebc6cd`）当 head 写——若它对同一 `since` 的差量恰好也全在证据侧——本门仍**咬不住**；
那一类属「书证命名」，只能靠人写对 + 评审（与 `check` 的既有局限同源，只是对象换成了书证 head）。

**reviewer-6 O-1 收口（2026-10-14）**：旧口径在缺省探测里**只取首个 `head = <oid>`**——若同一包
同时含「读数 head」与「交付 head」两处声明，会**静默**采信先出现者（缺省顺序
`11-boundary.txt → 10-boundary.txt → README.md`）。现改为**收集全部声明**，先 `rev-parse` **归一到完整 oid**
再判：多处声明指向**同一 commit**（哪怕前缀长短不同，真件 `teeth` 包即 `2591a7b31` vs `2591a7b316ae`）
⇒ 采信首个并继续；出现多处**不同 commit** ⇒ `PREREQ_AMBIGUOUS_HEAD_DECL`（rc=2）逐处点名，
要求用 `--decl-file` 钉住应采信的那一处。其余四类判据（可解析 / since 祖先 / HEAD 祖先 / 仅证据侧）不变。

用法（cwd = `mir2-rs/tests/parity/m3`）：
    python -I rng-trace-scene/check_evidence_head.py check \
        --dir    evidence-m3-rng-scene-anchor-teeth-20261014 \
        --head   2591a7b31
    python -I rng-trace-scene/check_evidence_head.py check-pack \
        --dir    evidence-m3-rng-scene-anchor-teeth-20261014 \
        --since  10f0daa93
    python -I rng-trace-scene/check_evidence_head.py selftest

退出码：0 = GREEN；1 = RED（点名）；2 = PREREQ（缺件 / 坏输入 / 不是 git 工作树）。
"""

import argparse
import contextlib
import io
import os
import re
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


# ------------------- check-pack：从包内读声明 head + 显式 since 锚 -------------------

DEFAULT_DECL_FILES = ("11-boundary.txt", "10-boundary.txt", "README.md")
# 只认「head = <oid>」这种带等号的写法（散文里的 `approve head `oid`` 不算声明）。
DEFAULT_DECL_RE = r"head\s*=\s*[`\"'<]?([0-9a-fA-F]{7,40})"


def _read_declared_head(ev_dir, decl_file, decl_regex):
    """从包内读「声明 head」。

    ⇒ `(首个声明原文, 来源, 说明, 全部声明)`：
      - 找到声明：`(raw, src, detail, [(raw, file, line)…])`；
      - 找不到：`(None, tried, None, [])`。
    **歧义判定不在本函数做**：同一 commit 可能被写成不同长度的前缀（reviewer-6 O-1
    实测 `teeth` 包就是 `2591a7b31` vs `2591a7b316ae`），必须由 `check_pack` 先 `rev-parse`
    归一成完整 oid 再判「多处**不同 commit**」，不得按字符串比。
    """
    try:
        pat = re.compile(decl_regex)
    except re.error as exc:
        return None, "regex-error:%s" % exc, None, []
    candidates = [decl_file] if decl_file else list(DEFAULT_DECL_FILES)
    tried = []
    matches = []  # [(raw, name, lineno)]，按探测顺序
    for name in candidates:
        path = os.path.join(ev_dir, name)
        if not os.path.isfile(path):
            tried.append(name + "(缺)")
            continue
        tried.append(name)
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            for lineno, line in enumerate(fh, 1):
                m = pat.search(line)
                if m:
                    matches.append((m.group(1), name, lineno))
    if not matches:
        return None, ",".join(tried), None, []
    raw, name, lineno = matches[0]
    detail = "line %d" % lineno
    if len(matches) > 1:
        detail += "（共 %d 处声明）" % len(matches)
    return raw, name, detail, matches


def check_pack(repo, ev_dir, since, decl_file, decl_regex, allow_prefixes):
    """check-pack：机判对象 = **包内声明的 head**（不是当前 HEAD），用显式 `since` 锚界定差量。

    判据：① 包内能读到 `head = <oid>` 声明；② 该 oid 可解析；③ `--since` 可解析且是声明 head
    的祖先；④ 声明 head 是当前 HEAD 的祖先；⑤ `diff --name-only since..声明 head` 只含证据/夹具侧路径。
    """
    problems = []
    root = _repo_root(repo)
    if root is None:
        return 2, ["PREREQ_MISSING: %s 不是 git 工作树 / git 不可用" % repo]
    if not ev_dir or not os.path.isdir(ev_dir):
        return 2, ["PREREQ_MISSING: 找不到证据目录 %s" % (ev_dir or "(空)",)]
    if not since:
        return 2, ["PREREQ_MISSING: 缺 --since（显式锚）"]

    declared, src, detail, matches = _read_declared_head(ev_dir, decl_file, decl_regex)
    if declared is None:
        return 2, ["PREREQ_MISSING: 包内未找到 `head = <oid>` 声明（试过 %s）" % src]

    # O-1 收口：收集全部声明，按 **resolved 完整 oid** 归组；出现多处**不同 commit** ⇒ fail-closed。
    # （不得按字符串比：同一 commit 可写成不同长度前缀，真件 `teeth` 包即此。）
    groups = {}
    for raw, name, lineno in matches:
        rcx, outx = _git(root, "rev-parse", "--verify", "--quiet", "%s^{commit}" % raw)
        key = outx.strip().splitlines()[0] if (rcx == 0 and outx.strip()) else "UNRESOLVED:" + raw.lower()
        groups.setdefault(key, []).append((raw, name, lineno))
    if len(groups) > 1:
        parts = []
        for key in sorted(groups):
            locs = "; ".join("%s@%s:%d" % (raw, name, lineno) for (raw, name, lineno) in groups[key])
            parts.append("[%s] %s" % (key[:12], locs))
        return 2, ["PREREQ_AMBIGUOUS_HEAD_DECL: 包内有多处**不一致**的 `head = <oid>` 声明"
                   "（旧口径静默取首个 = reviewer-6 O-1 漏洞）：%s；请用 --decl-file 钉住应采信的那一处"
                   % " | ".join(parts)]

    ev_abs = os.path.abspath(ev_dir)
    ev_rel = _norm(os.path.relpath(ev_abs, root))

    rc, out = _git(root, "rev-parse", "--verify", "--quiet", "%s^{commit}" % declared)
    if rc != 0 or not out.strip():
        return 1, ["HEAD_UNRESOLVABLE: 包内声明 head %r 不是可解析的 commit（来源 %s %s）"
                   % (declared, src, detail)]
    decl_oid = out.strip().splitlines()[0]

    rc, out = _git(root, "rev-parse", "--verify", "--quiet", "%s^{commit}" % since)
    if rc != 0 or not out.strip():
        return 2, ["PREREQ_MISSING: --since %r 不是可解析的 commit" % since]
    since_oid = out.strip().splitlines()[0]

    rc, _ = _git(root, "merge-base", "--is-ancestor", since_oid, decl_oid)
    if rc != 0:
        return 1, ["SINCE_NOT_ANCESTOR: --since %s 不是包内声明 head %s 的祖先"
                   % (since_oid, decl_oid)]

    rc, _ = _git(root, "merge-base", "--is-ancestor", decl_oid, "HEAD")
    if rc != 0:
        return 1, ["HEAD_NOT_ANCESTOR: 包内声明 head %s 不是当前 HEAD 的祖先" % decl_oid]

    rc, out = _git(root, "diff", "--name-only", "%s..%s" % (since_oid, decl_oid))
    if rc != 0:
        return 2, ["PREREQ_MISSING: git diff %s..%s 失败：%s" % (since_oid, decl_oid, out.strip())]
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

    print("EVIDENCE_HEAD_PACK dir=%s decl=%s(%s) declared=%s since=%s changed=%d allow_prefixes=%s"
          % (ev_rel, src, detail, decl_oid, since_oid, len(changed), list(prefixes)))
    if outside:
        for p in outside:
            print("  RED HEAD_SCOPE_DRIFT: 声明 head 相对 since 锚改了非证据侧路径 %s" % p)
        problems.extend("HEAD_SCOPE_DRIFT: %s" % p for p in outside)
        return 1, problems
    print("  OK 包内声明 head 可解析、since 锚 ≤ 声明 head ≤ HEAD，且 since..声明 head 全在证据侧")
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
        # README 不写 `head = <oid>`（否则缺省探测会与 11-boundary.txt 撞成「多处声明」；
        # 见下方 pack-ambiguous-decl-prereq 的正向用例）。
        _write(os.path.join(ev, "README.md"), "# 合成证据包（声明见 11-boundary.txt）\n")
        _write(os.path.join(repo, prod, "code.rs"), "fn a() {}\n")
        head_a = _commit(repo, "A")

        # B：只动证据/夹具侧（ev 目录 + m3 下一个工具文件）⇒ 声明的 A 合法
        _write(os.path.join(ev, "README.md"), "# 合成证据包（B 仅证据侧；声明见 11-boundary.txt）\n")
        _write(os.path.join(repo, base, "tool.py"), "# 夹具侧工具\n")
        _commit(repo, "B")

        def _green():
            rc, probs = _quiet(lambda: check(repo, ev, head_a, None))
            assert rc == 0, "expected GREEN rc=0 got %d %s" % (rc, probs)
        case("evidence-only-since-head-green", _green)

        # 改坏①：B 之后又动了生产代码 ⇒ 声明 A 必红并点名
        _write(os.path.join(repo, prod, "code.rs"), "fn a() { /* x */ }\n")
        head_c = _commit(repo, "C touch code")

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

        # ---------------- check-pack：从包内读 declared head + 显式 since 锚 ----------------
        decl = os.path.join(ev, "11-boundary.txt")

        # E 只动证据侧（写声明文件本身）⇒ since=C（含生产改动）到 declared=E 的差量仍是证据侧。
        _write(decl, "### head = PLACEHOLDER\n")
        e_oid = _commit(repo, "E decl placeholder")
        _write(decl, "### 边界\n### head = %s（交付 head）\n" % e_oid)
        _commit(repo, "F decl to E")

        def _pack_green():
            rc, probs = _quiet(lambda: check_pack(repo, ev, head_c, None, DEFAULT_DECL_RE, None))
            assert rc == 0, "expected GREEN rc=0 got %d %s" % (rc, probs)
        case("pack-declared-head-green", _pack_green)

        def _pack_decl_eq_since_green():
            rc, probs = _quiet(lambda: check_pack(repo, ev, e_oid, None, DEFAULT_DECL_RE, None))
            assert rc == 0, "expected GREEN rc=0 got %d %s" % (rc, probs)
        case("pack-declared-eq-since-green", _pack_decl_eq_since_green)

        # 改坏：声明写成 head_c（自 since=A 起含生产改动）⇒ 越界点名
        def _pack_scope_drift_red():
            _write(decl, "### head = %s\n" % head_c)
            rc, probs = _quiet(lambda: check_pack(repo, ev, head_a, None, DEFAULT_DECL_RE, None))
            assert rc == 1 and any("code.rs" in p for p in probs), (rc, probs)
        case("pack-scope-drift-red", _pack_scope_drift_red)

        # 改坏：声明一个解析不出来的 oid ⇒ HEAD_UNRESOLVABLE
        def _pack_unresolvable_red():
            _write(decl, "### head = deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n")
            rc, probs = _quiet(lambda: check_pack(repo, ev, head_a, None, DEFAULT_DECL_RE, None))
            assert rc == 1 and any("HEAD_UNRESOLVABLE" in p for p in probs), (rc, probs)
        case("pack-unresolvable-head-red", _pack_unresolvable_red)

        # 改坏：声明一个非 HEAD 祖先 ⇒ HEAD_NOT_ANCESTOR
        def _pack_non_ancestor_red():
            _write(decl, "### head = %s\n" % side)
            rc, probs = _quiet(lambda: check_pack(repo, ev, head_a, None, DEFAULT_DECL_RE, None))
            assert rc == 1 and any("HEAD_NOT_ANCESTOR" in p for p in probs), (rc, probs)
        case("pack-non-ancestor-head-red", _pack_non_ancestor_red)

        # 改坏：--since 不是声明 head 的祖先 ⇒ SINCE_NOT_ANCESTOR
        def _pack_since_not_ancestor_red():
            _write(decl, "### head = %s\n" % e_oid)
            rc, probs = _quiet(lambda: check_pack(repo, ev, side, None, DEFAULT_DECL_RE, None))
            assert rc == 1 and any("SINCE_NOT_ANCESTOR" in p for p in probs), (rc, probs)
        case("pack-since-not-ancestor-red", _pack_since_not_ancestor_red)

        # PREREQ：包内没有可读的声明文件
        def _pack_missing_decl_prereq():
            ev2 = os.path.join(repo, base, "ev-no-decl")
            _write(os.path.join(ev2, "README.md"), "no declaration here\n")
            rc, probs = _quiet(lambda: check_pack(repo, ev2, head_a, None, DEFAULT_DECL_RE, None))
            assert rc == 2, "expected PREREQ rc=2 got %d %s" % (rc, probs)
        case("pack-missing-decl-prereq", _pack_missing_decl_prereq)

        # PREREQ：声明文件在但没有 `head =` 行
        def _pack_no_head_line_prereq():
            _write(decl, "### 边界\n### 只有 base，没有 head 声明\n")
            rc, probs = _quiet(lambda: check_pack(repo, ev, head_a, None, DEFAULT_DECL_RE, None))
            assert rc == 2, "expected PREREQ rc=2 got %d %s" % (rc, probs)
        case("pack-no-head-line-prereq", _pack_no_head_line_prereq)

        # PREREQ：--since 不可解析
        def _pack_since_unresolvable_prereq():
            _write(decl, "### head = %s\n" % e_oid)
            rc, probs = _quiet(lambda: check_pack(
                repo, ev, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", None, DEFAULT_DECL_RE, None))
            assert rc == 2, "expected PREREQ rc=2 got %d %s" % (rc, probs)
        case("pack-since-unresolvable-prereq", _pack_since_unresolvable_prereq)

        # ---------------- reviewer-6 O-1：多处声明不得静默取首个 ----------------
        readme = os.path.join(ev, "README.md")

        # 改坏：11-boundary.txt 声明 e_oid、README.md 声明 head_c（**不一致**）⇒ fail-closed 点名两处
        def _pack_ambiguous_decl_prereq():
            _write(decl, "### head = %s（交付 head）\n" % e_oid)
            _write(readme, "旧读数 head = %s\n" % head_c)
            rc, probs = _quiet(lambda: check_pack(repo, ev, head_a, None, DEFAULT_DECL_RE, None))
            assert rc == 2, "expected PREREQ rc=2 got %d %s" % (rc, probs)
            assert any("PREREQ_AMBIGUOUS_HEAD_DECL" in p for p in probs), probs
            assert any(e_oid[:12] in p and head_c[:12] in p for p in probs), probs
        case("pack-ambiguous-decl-prereq", _pack_ambiguous_decl_prereq)

        # 绿向：两处声明指向**同一 commit**但写成不同长度前缀（真件 teeth 包的形状）⇒ 归一后不歧义
        def _pack_same_commit_prefix_multi_decl_green():
            _write(decl, "### head = %s\n" % e_oid)
            _write(readme, "交付 head = %s（同 commit 短前缀）\n" % e_oid[:8])
            rc, probs = _quiet(lambda: check_pack(repo, ev, head_c, None, DEFAULT_DECL_RE, None))
            assert rc == 0, "expected GREEN rc=0 got %d %s" % (rc, probs)
        case("pack-same-commit-prefix-multi-decl-green", _pack_same_commit_prefix_multi_decl_green)

        # 绿向：--decl-file 钉住边界件 ⇒ 即使 README 另有不一致声明也只看被钉件
        def _pack_decl_file_pins_green():
            _write(decl, "### head = %s\n" % e_oid)
            _write(readme, "旧读数 head = %s\n" % head_c)
            rc, probs = _quiet(lambda: check_pack(
                repo, ev, head_c, "11-boundary.txt", DEFAULT_DECL_RE, None))
            assert rc == 0, "expected GREEN rc=0 got %d %s" % (rc, probs)
        case("pack-decl-file-pins-green", _pack_decl_file_pins_green)
    finally:
        import shutil
        shutil.rmtree(tmp, ignore_errors=True)

    total = ok + len(failed)
    for name, msg in failed:
        print("  FAIL %s: %s" % (name, msg))
    if failed:
        print("SELFTEST_RED: check_evidence_head %d/%d（失败 %d）" % (ok, total, len(failed)))
        return 1
    print("SELFTEST GREEN: check_evidence_head %d/%d（check 7 + check-pack 12：含改坏必红 + PREREQ"
          " + O-1 多处声明 fail-closed）"
          % (ok, total))
    return 0


def _print_problems(probs, skip_prefixes=()):
    """把 check/check-pack 返回的问题逐条打出（避免与已打印的 `RED ...` 行重复）。"""
    for p in probs or []:
        if any(p.startswith(pre) for pre in skip_prefixes):
            continue
        print(p)


def main(argv):
    ap = argparse.ArgumentParser(prog="check_evidence_head.py")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_check = sub.add_parser("check", help="核对证据包声明的 head（对当前 HEAD）")
    p_check.add_argument("--dir", required=True, help="证据目录")
    p_check.add_argument("--head", required=True, help="证据包声明的 head（commit oid/ref）")
    p_check.add_argument("--repo", default=".", help="git 工作树（缺省 .）")
    p_check.add_argument("--allow-prefix", action="append", dest="allow_prefixes",
                         help="允许在声明 head 之后变更的仓库根相对路径前缀（可重复；缺省 %s）"
                              % list(DEFAULT_ALLOW_PREFIXES))

    p_pack = sub.add_parser(
        "check-pack",
        help="从包内读声明 head + 显式 since 锚核对（机判对象 = 书证 head）")
    p_pack.add_argument("--dir", required=True, help="证据目录")
    p_pack.add_argument("--since", required=True, help="显式锚（本片 patch base 的 commit oid/ref）")
    p_pack.add_argument("--decl-file", default=None,
                        help="包内声明 head 的文件（缺省按 %s 探测）" % list(DEFAULT_DECL_FILES))
    p_pack.add_argument("--decl-regex", default=DEFAULT_DECL_RE,
                        help="从声明文件里抓 head oid 的正则（缺省 %r）" % DEFAULT_DECL_RE)
    p_pack.add_argument("--repo", default=".", help="git 工作树（缺省 .）")
    p_pack.add_argument("--allow-prefix", action="append", dest="allow_prefixes",
                        help="允许在 since..声明 head 之间变更的路径前缀（可重复；缺省 %s）"
                             % list(DEFAULT_ALLOW_PREFIXES))

    sub.add_parser("selftest", help="机判自检（含改坏必红）")

    args = ap.parse_args(argv)
    if args.cmd == "check":
        rc, probs = check(args.repo, args.dir, args.head, args.allow_prefixes)
        _print_problems(probs, skip_prefixes=("HEAD_SCOPE_DRIFT:",))
        return rc
    if args.cmd == "check-pack":
        rc, probs = check_pack(args.repo, args.dir, args.since, args.decl_file,
                               args.decl_regex, args.allow_prefixes)
        _print_problems(probs, skip_prefixes=("HEAD_SCOPE_DRIFT:",))
        return rc
    return cmd_selftest(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
