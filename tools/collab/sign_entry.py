# -*- coding: utf-8 -*-
"""collab 条目签名脚本（卡 openmir2-signing-script-head-placeholder / openmir2-collab-sign-entry-hardening）。

写侧 fail-closed，三层：
  1. head/base/parent 一律实取并 **assert 全 40 位小写 hex**；非 hex（含 %P/散文/缩写）拒签；
  2. `--parent` **缺席或为空** ⇒ 拒签（除非显式 `--root`）；`--root` 只能用于
     「该 thread 尚无任何条目」（建线用的 `kind=issue` 也不例外，同样要探测历史），
     有历史条目 ⇒ 拒签；
  3. `--body-file` 的 mtime 超过 `--max-body-age`（默认 1800s）⇒ 拒签，
     错误里打印 body 的 `sha256` 与 `mtime`；确认不是旧卡残留后加 `--allow-stale-body` 放行。
     签名成功后打印 `body_sha256=<hex>`，供签后回读比对。

用法：
  python sign_entry.py --repo <worktree> --kind patch --id <thread> \\
      --parent <40hex> --body-file body.json \\
      [--head HEAD] [--base <rev>] [--refs-head @rev] [--refs-base @rev] [--push]
  python sign_entry.py --repo <worktree> --kind issue --id <thread> \\
      --root --body-file body.json          # 建线：显式声明根条目

`--precheck`（sign_entry.sh 调用，也供测试）：只做 parent/root 与 body 新鲜度校验，
stdout 打印 `body_sha256=<hex>`，不签名。

body.json 里若含 "head"/"base" 键：
  * 值为 "@HEAD"/"@<rev>" 形态 ⇒ 由本脚本 rev-parse 展开为全 40 位小写 hex；
  * 值不是 hex 且不是 @rev 形态 ⇒ 拒签。

--selftest：离线夹具（临时 git 仓 + 假 probe），覆盖 head 占位符、parent 缺席/根条目、
`--kind issue` 的线程历史探测（F1）、陈旧 body、body_sha256；跑完断言真实仓库 refs 未被改动。
"""
import argparse
import contextlib
import datetime
import hashlib
import io
import json
import os
import re
import subprocess
import sys
import tempfile
import time

HEX40 = re.compile(r"^[0-9a-f]{40}$")
DEFAULT_MAX_BODY_AGE = 1800  # 30 分钟
KINDS = ("issue", "status", "patch", "review", "merge_result",
         "comment", "review_request")


def _collab_env():
    """`walgit collab` 子进程环境：2026-11-02 起必须显式请求本地命令接口。

    所有调用点都走这个函数，保证「带开关」是可 grep 的单一事实源
    （卡 `openmir2-walgit-local-commands-env`）。
    """
    env = dict(os.environ)
    env["WALGIT_LOCAL_COMMANDS"] = "1"
    return env


def die(msg):
    print("sign_entry: REFUSED: %s" % msg, file=sys.stderr)
    sys.exit(2)


def refuse(msg):
    """写侧校验失败（与 sign_entry.sh 的 rc 契约一致：1 = 拒签）。"""
    print("sign_entry: REFUSED: %s" % msg, file=sys.stderr)
    sys.exit(1)


def rev_parse(repo, rev):
    """rev-parse 实取；失败/非 40 位小写 hex ⇒ None。"""
    try:
        out = subprocess.run(["git", "-C", repo, "rev-parse", rev],
                             capture_output=True, text=True, timeout=30)
    except Exception:
        return None
    v = (out.stdout or "").strip()
    return v if (out.returncode == 0 and HEX40.match(v)) else None


def expand_rev(repo, spec, name):
    """@rev 或 40-hex 字面量 ⇒ 40 位小写 hex；否则拒签。"""
    if spec.startswith("@"):
        v = rev_parse(repo, spec[1:])
        if v is None:
            die("%s 的 @%s 无法 rev-parse 成 40 位 hex" % (name, spec[1:]))
        return v
    return check_hex(name, spec)


def check_hex(name, value):
    """签名前断言：必须是全 40 位小写 hex（非 hex 拒签）。"""
    if not isinstance(value, str) or not HEX40.match(value):
        die("%s=%r 不是 40 位小写 hex（占位符/散文/缩写都拒签）" % (name, value))
    return value


def body_fingerprint(path):
    """返回 (sha256 小写 hex, mtime epoch)。读不到 ⇒ 拒签。"""
    try:
        st = os.stat(path)
    except OSError as ex:
        refuse("body 文件不可读：%s（%s）" % (path, ex))
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest(), st.st_mtime


def guard_body_freshness(path, max_age, allow_stale, now=None):
    """body mtime 超过 max_age ⇒ 拒签（除非 allow_stale）；返回 body sha256。"""
    sha, mtime = body_fingerprint(path)
    age = (time.time() if now is None else now) - mtime
    if age > max_age and not allow_stale:
        when = datetime.datetime.fromtimestamp(mtime).astimezone().isoformat()
        refuse(
            "body 文件陈旧：%s mtime=%s（epoch=%.0f，已 %.0fs > 阈值 %ds）"
            " body_sha256=%s；确认不是上一张卡的残留后显式加 --allow-stale-body"
            "（或用 --max-body-age <秒> 调阈值）"
            % (path, when, mtime, age, max_age, sha))
    return sha


def thread_entry_count(repo, thread_id, run=subprocess.run):
    """`walgit collab thread <id>` 判空。返回 (count|None, detail)；None = 判不了。"""
    try:
        pr = run(["walgit", "collab", "thread", thread_id, "--repo", repo],
                 capture_output=True, text=True, timeout=60, env=_collab_env())
    except Exception as ex:
        return None, "调用 walgit collab thread 失败：%s" % ex
    if pr.returncode != 0:
        err = (pr.stderr or "").strip()
        if "no entries for thread" in err:
            return 0, "thread 尚无条目（walgit: %s）" % err
        return None, "walgit collab thread rc=%d stderr=%r" % (pr.returncode, err)
    try:
        data = json.loads(pr.stdout or "")
    except Exception:
        return None, "walgit collab thread 输出不是 JSON：%r" % (pr.stdout or "")[:200]
    if not isinstance(data, list):
        return None, "walgit collab thread 输出不是数组（%s）" % type(data).__name__
    return len(data), "thread 已有 %d 条条目" % len(data)


def guard_parent_mode(repo, kind, thread_id, parent, want_root, probe=None):
    """--parent 缺席即拒签；--root 只能用于线程首条目（kind=issue 建线亦然）。"""
    has_parent = isinstance(parent, str) and parent.strip() != ""
    if want_root and has_parent:
        refuse("--root 与 --parent 互斥：已给 --parent=%s 就不能声明根条目" % parent)
    if has_parent:
        return
    if not want_root:
        refuse("--parent 缺席或为空：拒绝静默签「根条目」（常见于取值管道空转）；"
               "建线/首条目请显式加 --root")
    # F1（卡 openmir2-collab-tool-tails）：`--kind issue` 不再早退——它同样要探测线程历史，
    # 否则把 kind 错写成 issue 就能在有历史的线程上再落一个根条目（oid_gate 对空 parent 跳过）。
    if probe is None:
        probe = lambda: thread_entry_count(repo, thread_id)  # noqa: E731
    count, detail = probe()
    if count is None:
        refuse("--root 无法判定该 thread 是否已有历史：%s（按 fail-closed 拒绝）" % detail)
    if count > 0:
        refuse("--root 只允许用于线程首条目：%s；有历史就必须给 --parent=<线程尾 entry oid>"
               % detail)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--kind", default=None)
    ap.add_argument("--id", default=None)
    ap.add_argument("--parent", default=None, help="线程尾 entry oid（40 hex）")
    ap.add_argument("--root", action="store_true",
                    help="显式声明根条目（仅 thread 首条目，kind=issue 建线亦然）")
    ap.add_argument("--body-file", default=None)
    ap.add_argument("--allow-stale-body", action="store_true",
                    help="显式放行陈旧 body（默认 mtime > 1800s 拒签）")
    ap.add_argument("--max-body-age", type=int, default=DEFAULT_MAX_BODY_AGE,
                    help="body mtime 容忍秒数（默认 %d）" % DEFAULT_MAX_BODY_AGE)
    ap.add_argument("--head", default=None, help="body.head 的取值来源 rev（默认 HEAD）")
    ap.add_argument("--base", default=None, help="body.base 的取值来源 rev")
    ap.add_argument("--refs-head", default=None,
                    help="walgit --head（进 entry.refs，机器读者用）；@rev 或 40 hex")
    ap.add_argument("--refs-base", default=None,
                    help="walgit --base（进 entry.refs）；@rev 或 40 hex")
    ap.add_argument("--push", action="store_true")
    ap.add_argument("--dry-run", action="store_true",
                    help="只校验并打印将签的 body，不调 walgit")
    ap.add_argument("--precheck", action="store_true",
                    help="只做 parent/root 与 body 新鲜度校验，打印 body_sha256")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(args.repo)

    if not args.body_file:
        die("--body-file 必填")
    if not os.path.isfile(args.body_file):
        die("body 文件不存在：%s" % args.body_file)
    if args.kind not in KINDS:
        die("kind=%r 不在允许集合" % args.kind)
    if not args.id or not re.match(r"^[\w\-]{4,80}$", args.id):
        die("id=%r 形状不像线程 id" % args.id)
    if args.max_body_age < 0:
        die("--max-body-age 不能为负（%d）" % args.max_body_age)

    # ---- 写侧 fail-closed：先于 JSON 解析与任何签名动作 ----
    guard_parent_mode(args.repo, args.kind, args.id, args.parent, args.root)
    body_sha = guard_body_freshness(args.body_file, args.max_body_age,
                                    args.allow_stale_body)
    if args.precheck:
        print("body_sha256=%s" % body_sha)
        return 0

    body = json.load(open(args.body_file, encoding="utf-8"))

    # ---- A1 核心：head/base 展开与断言（先于任何签名动作）----
    if "head" in body:
        v = body["head"]
        if isinstance(v, str) and v.startswith("@"):
            real = rev_parse(args.repo, v[1:])
            if real is None:
                die("body.head 的 @%s 无法 rev-parse 成 40 位 hex" % v[1:])
            body["head"] = real
        check_hex("body.head", body["head"])
    if "base" in body:
        v = body["base"]
        if isinstance(v, str) and v.startswith("@"):
            real = rev_parse(args.repo, v[1:])
            if real is None:
                die("body.base 的 @%s 无法 rev-parse 成 40 位 hex" % v[1:])
            body["base"] = real
        check_hex("body.base", body["base"])
    # refs.head / refs.base（entry.refs —— 机器读者按这里取）：@rev 展开成 40 hex。
    refs_head = refs_base = None
    if args.refs_head is not None:
        refs_head = expand_rev(args.repo, args.refs_head, "refs_head")
    if args.refs_base is not None:
        refs_base = expand_rev(args.repo, args.refs_base, "refs_base")
    # parent（线程尾）也必须是 hex：防「从散文里截一段」当 parent。
    if args.parent:
        check_hex("parent", args.parent)

    payload = json.dumps(body, ensure_ascii=False)
    print("sign_entry: OK head=%s base=%s parent=%s"
          % (body.get("head", "-"), body.get("base", "-"),
             args.parent[:12] if args.parent else "<root>"))
    if args.dry_run:
        print("body_sha256=%s" % body_sha)
        print(payload)
        return 0

    cmd = ["walgit", "collab", "entry", "--repo", args.repo, "--kind", args.kind,
           "--id", args.id]
    if args.parent:
        cmd += ["--parent", args.parent]
    cmd += ["--body", payload]
    if refs_head is not None:
        cmd += ["--head", refs_head]
    if refs_base is not None:
        cmd += ["--base", refs_base]
    if args.push:
        cmd += ["--push", "walgit"]
    pr = subprocess.run(cmd, capture_output=True, text=True, env=_collab_env())
    sys.stdout.write(pr.stdout or "")
    sys.stderr.write(pr.stderr or "")
    if pr.returncode != 0:
        die("walgit collab entry 失败 rc=%d" % pr.returncode)
    print("sign_entry: body_sha256=%s" % body_sha)
    return pr.returncode


def _capture_refusal(fn):
    """跑一个应当 refuse 的函数，返回 (SystemExit.code|None, stderr 文本)。"""
    buf = io.StringIO()
    try:
        with contextlib.redirect_stderr(buf):
            fn()
    except SystemExit as ex:
        return ex.code, buf.getvalue()
    return None, buf.getvalue()


def _capture_main(argv):
    """在**当前进程**里跑一次 main()（便于注入探针），返回 (SystemExit.code|返回值, stderr)。"""
    buf = io.StringIO()
    out = io.StringIO()
    orig_argv = sys.argv
    try:
        sys.argv = [__file__] + list(argv)
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(buf):
            rc = main()
    except SystemExit as ex:
        return ex.code, buf.getvalue()
    finally:
        sys.argv = orig_argv
    return rc, buf.getvalue()


def _run_cli(extra, env=None):
    return subprocess.run([sys.executable, "-B", __file__] + list(extra),
                          capture_output=True, text=True, env=env)


def _selftest_env():
    """剥离会污染夹具的 git 环境变量（绝不把 GIT_DIR 指到真实仓库）。"""
    env = dict(os.environ)
    for k in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
              "GIT_ALTERNATE_OBJECT_DIRECTORIES", "GIT_COMMON_DIR"):
        env.pop(k, None)
    return env


def _refs_snapshot(repo):
    try:
        pr = subprocess.run(["git", "-C", repo, "for-each-ref", "refs/heads",
                             "--format=%(refname) %(objectname)"],
                            capture_output=True, text=True, timeout=30)
    except Exception:
        return None
    return pr.stdout if pr.returncode == 0 else None


def _rmtree_force(func, path, _exc_info):
    """T1（卡 openmir2-sign-entry-selftest-tmp-leak）：Windows 上 git 的 `.git`/loose object
    是只读的，直接 rmtree 会 PermissionError；先去只读位再删（只对**本进程 mkdtemp 自建**的树调用）。"""
    import stat
    try:
        os.chmod(path, stat.S_IWRITE)
    except OSError:
        pass
    func(path)


def selftest(repo):
    """离线夹具：临时 git 仓 + 假 probe，覆盖既有 head 占位符与新增三件事。"""
    cases = []
    good = "0123456789abcdef0123456789abcdef01234567"

    def run_case(body, parent, want_refuse):
        f = tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False, encoding="utf-8")
        json.dump(body, f, ensure_ascii=False)
        f.close()
        try:
            pr = subprocess.run(
                [sys.executable, "-B", __file__, "--repo", repo, "--kind", "patch",
                 "--id", "selftest-signing-case", "--parent", parent,
                 "--body-file", f.name, "--dry-run"],
                capture_output=True, text=True, env=_selftest_env())
        finally:
            os.unlink(f.name)
        refused = (pr.returncode == 2)
        assert refused == want_refuse, (
            "case head=%r parent=%r: refused=%r want=%r stderr=%s"
            % (body.get("head"), parent[:12], refused, want_refuse, pr.stderr))
        cases.append(("legacy:%r" % body.get("head"), refused))

    # ① 事故原形态：字面 %P 散文 ⇒ 必须拒签（旧流程会照签产出 refs.head 缺失条目）。
    run_case({"head": "docs 片单提交（parent=b5442dbd4，%P 实取）", "base": "b5442dbd4"},
             good, True)
    # ② 缩写也不收：12 位短哈希 ⇒ 拒签。
    run_case({"head": "b5442dbd440d", "base": "b5442dbd4"}, good, True)
    # ③ @HEAD ⇒ 展开（dry-run 不签，只校验）。
    run_case({"head": "@HEAD", "base": "@HEAD"}, good, False)
    # ④ parent 非 hex ⇒ 拒签。
    run_case({"head": "@HEAD", "base": "@HEAD"}, "%P 实取", True)
    # ⑤ 全 hex 直填 ⇒ 放行。
    run_case({"head": good, "base": good}, good, False)

    # ---- 新增：parent/root 与 body 新鲜度（真实临时 git 仓，不碰本仓 refs）----
    before_refs = _refs_snapshot(repo)
    tmp = tempfile.mkdtemp(prefix="sign-entry-selftest-")
    try:
        env = _selftest_env()
        frepo = os.path.join(tmp, "repo")
        os.makedirs(frepo)
        for cmd in (["git", "init", "-q", frepo],
                    ["git", "-C", frepo, "config", "user.email", "selftest@example.invalid"],
                    ["git", "-C", frepo, "config", "user.name", "selftest"]):
            pr = subprocess.run(cmd, capture_output=True, text=True, env=env)
            assert pr.returncode == 0, ("夹具 git 失败：%r %s" % (cmd, pr.stderr))
        seed = os.path.join(frepo, "seed.txt")
        open(seed, "w", encoding="utf-8").write("selftest\n")
        for cmd in (["git", "-C", frepo, "add", "seed.txt"],
                    ["git", "-C", frepo, "commit", "-q", "-m", "seed"]):
            pr = subprocess.run(cmd, capture_output=True, text=True, env=env)
            assert pr.returncode == 0, ("夹具 git 失败：%r %s" % (cmd, pr.stderr))

        body = os.path.join(tmp, "body.json")
        open(body, "w", encoding="utf-8").write('{"status": "selftest"}\n')
        body_sha = hashlib.sha256(open(body, "rb").read()).hexdigest()

        def cli(*extra):
            return _run_cli(["--precheck", "--repo", frepo, "--kind", "patch",
                             "--id", "selftest-signing-case",
                             "--body-file", body] + list(extra), env=env)

        # N1：缺 --parent ⇒ 拒签（rc=1，不静默签根条目）。
        pr = cli()
        assert pr.returncode == 1, ("N1 缺 --parent 未拒签 rc=%r stderr=%s"
                                    % (pr.returncode, pr.stderr))
        assert "REFUSED" in pr.stderr and "--parent" in pr.stderr, pr.stderr
        cases.append(("N1:missing-parent", True))

        # N1b：--parent 传空串（事故①形态）同样拒签。
        pr = cli("--parent", "")
        assert pr.returncode == 1, ("N1b 空 parent 未拒签 rc=%r" % pr.returncode)
        cases.append(("N1b:empty-parent", True))

        # N2：有历史线程上用 --root ⇒ 拒签（假 probe：1 条历史）。
        code, out = _capture_refusal(lambda: guard_parent_mode(
            frepo, "patch", "selftest-signing-case", None, True,
            probe=lambda: (1, "thread 已有 1 条条目")))
        assert code == 1 and "--root" in out, ("N2 rc=%r out=%s" % (code, out))
        cases.append(("N2:root-with-history", True))

        # N2b：空线程 ⇒ --root 放行（负例对照；patch 与 issue 的首条目都允许）。
        code, out = _capture_refusal(lambda: guard_parent_mode(
            frepo, "patch", "selftest-signing-case", None, True,
            probe=lambda: (0, "thread 尚无条目")))
        assert code is None, ("N2b 空线程 --root 被误拒：%r %s" % (code, out))
        code, out = _capture_refusal(lambda: guard_parent_mode(
            frepo, "issue", "selftest-signing-case", None, True,
            probe=lambda: (0, "thread 尚无条目")))
        assert code is None, ("N2b 空线程 kind=issue --root 被误拒：%r %s"
                              % (code, out))
        cases.append(("N2b:root-allowed", False))

        # ---- F1（卡 openmir2-collab-tool-tails）：`--kind issue` 不得绕过历史探测 ----
        # 旧实现 `if kind == "issue": return` 早退 ⇒ 把 kind 写成 issue 就能在有历史的
        # 线程上再落一个根条目（oid_gate 对空 parent 跳过，抓不到）。
        # 牙②：有历史 + issue + --root ⇒ 必须拒签（单元层）。
        code, out = _capture_refusal(lambda: guard_parent_mode(
            frepo, "issue", "selftest-signing-case", None, True,
            probe=lambda: (3, "thread 已有 3 条条目")))
        assert code == 1 and "--root" in out, (
            "F1 牙② kind=issue 有历史 --root 未拒签：%r %s" % (code, out))
        cases.append(("F1:issue-root-with-history", True))

        # 牙③：CLI 层（跑真 main()）——有历史 + issue + --root ⇒ rc=1，且 **walgit
        # 调用 0 次**。探针注入：thread 探测顶替成「有 1 条历史」，subprocess.run 顶替成
        # 记录器；记录器一旦看到 walgit 就说明拒绝路径仍在动 walgit（拒绝必须早于它）。
        orig_probe = globals()["thread_entry_count"]
        orig_run = subprocess.run
        walgit_calls = []

        def _spy_run(cmd, *a, **kw):
            if isinstance(cmd, (list, tuple)) and cmd and cmd[0] == "walgit":
                walgit_calls.append(list(cmd))
                return subprocess.CompletedProcess(cmd, 1, "", "spy: no walgit")
            return orig_run(cmd, *a, **kw)

        try:
            globals()["thread_entry_count"] = (
                lambda repo, thread_id, run=None: (1, "thread 已有 1 条条目"))
            subprocess.run = _spy_run
            code, out = _capture_main([
                "--repo", frepo, "--kind", "issue", "--id",
                "selftest-signing-case", "--root", "--body-file", body])
        finally:
            subprocess.run = orig_run
            globals()["thread_entry_count"] = orig_probe
        assert code == 1 and "--root" in out, (
            "F1 牙③ CLI 有历史 issue --root 未拒签：%r %s" % (code, out))
        assert walgit_calls == [], (
            "F1 牙③ 拒绝路径仍调用了 walgit %d 次：%r"
            % (len(walgit_calls), walgit_calls))
        cases.append(("F1:cli-refuse-zero-walgit", True))

        # 牙①：CLI 层空线程 + issue + --root ⇒ 放行（main 返回 0，同样零 walgit）。
        try:
            globals()["thread_entry_count"] = (
                lambda repo, thread_id, run=None: (0, "thread 尚无条目"))
            subprocess.run = _spy_run
            code, out = _capture_main([
                "--repo", frepo, "--kind", "issue", "--id",
                "selftest-signing-case", "--root", "--body-file", body,
                "--precheck"])
        finally:
            subprocess.run = orig_run
            globals()["thread_entry_count"] = orig_probe
        assert code == 0, (
            "F1 牙① CLI 空线程 issue --root 被误拒：%r %s" % (code, out))
        assert walgit_calls == [], (
            "F1 牙① 放行路径不应调用 walgit：%r" % (walgit_calls,))
        cases.append(("F1:cli-empty-allowed", False))

        # N2c：--root 与 --parent 互斥 ⇒ 拒签（CLI 层，不走 walgit）。
        pr = cli("--root", "--parent", good)
        assert pr.returncode == 1 and "互斥" in pr.stderr, (
            "N2c rc=%r stderr=%s" % (pr.returncode, pr.stderr))
        cases.append(("N2c:root+parent", True))

        # 陈旧 body 夹具（mtime 1 小时前，内容与新鲜 body 不同）。
        stale = os.path.join(tmp, "stale.json")
        open(stale, "w", encoding="utf-8").write('{"status": "selftest-stale"}\n')
        old_t = time.time() - 3600
        os.utime(stale, (old_t, old_t))
        stale_sha = hashlib.sha256(open(stale, "rb").read()).hexdigest()

        # N4：新鲜 body ⇒ 通过，并打印 body_sha256（供签后回读）。
        pr = cli("--parent", good)
        assert pr.returncode == 0, ("N4 新鲜 body 被误拒 rc=%r stderr=%s"
                                    % (pr.returncode, pr.stderr))
        assert ("body_sha256=%s" % body_sha) in pr.stdout, (
            "N4 body_sha256 不匹配：%r" % pr.stdout)
        cases.append(("N4:fresh-body", False))

        # N5：--allow-stale-body ⇒ 陈旧 body 显式放行。
        pr = _run_cli(["--precheck", "--repo", frepo, "--kind", "patch",
                       "--id", "selftest-signing-case", "--parent", good,
                       "--body-file", stale, "--allow-stale-body"], env=env)
        assert pr.returncode == 0, ("N5 --allow-stale-body 仍被拒 rc=%r stderr=%s"
                                    % (pr.returncode, pr.stderr))
        assert ("body_sha256=%s" % stale_sha) in pr.stdout, (
            "N5 body_sha256 不匹配：%r" % pr.stdout)
        cases.append(("N5:allow-stale", False))

        # N3：陈旧 body ⇒ 拒签，错误含 body_sha256 与 mtime。
        pr = _run_cli(["--precheck", "--repo", frepo, "--kind", "patch",
                       "--id", "selftest-signing-case", "--parent", good,
                       "--body-file", stale], env=env)
        assert pr.returncode == 1, ("N3 陈旧 body 未拒签 rc=%r" % pr.returncode)
        assert ("body_sha256=%s" % stale_sha) in pr.stderr, (
            "N3 拒绝信息缺 body_sha256：%r" % pr.stderr)
        assert "mtime=" in pr.stderr, ("N3 拒绝信息缺 mtime：%r" % pr.stderr)
        cases.append(("N3:stale-body", True))
    finally:
        import shutil
        # T1（卡 openmir2-sign-entry-selftest-tmp-leak）：只清理**本进程用 mkdtemp 建的这一棵**
        # （路径已知）；绝不按通配扫临时目录——非本进程创建的 `sign-entry-selftest-*` 一律不碰。
        # 旧实现 `rmtree(..., ignore_errors=True)` 把只读 loose object 的 PermissionError 吞掉，
        # 每跑一次 `--selftest` 静默泄漏一个目录；改为先去只读位再删（_rmtree_force），
        # 残余失败照常抛出（与 oid_gate.py 的已合入修法一致）。
        shutil.rmtree(tmp, onerror=_rmtree_force)

    after_refs = _refs_snapshot(repo)
    assert before_refs == after_refs, "selftest 改动/新增了真实仓库的 refs（夹具泄漏）"

    print("sign_entry selftest: OK (%d cases：legacy head 占位符 5 + "
          "N1 缺/空 parent 拒签 / N2 有历史 --root 拒签 / N2b 空线程 --root 放行 / "
          "N2c --root+--parent 拒签 / N3 陈旧 body 拒签 / N4 新鲜 body 通过 / "
          "N5 --allow-stale-body 放行 / F1 issue 也探测历史：空线程放行、"
          "有历史拒签（拒绝路径 walgit 调用 0 次）)" % len(cases))
    return 0


if __name__ == "__main__":
    sys.exit(main())
