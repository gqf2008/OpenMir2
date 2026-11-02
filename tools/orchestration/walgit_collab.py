#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""walgit_collab — 统一封装 `walgit collab`：显式请求本地命令接口 + fail-closed。

为什么存在（卡 `openmir2-walgit-local-commands-env`）
----------------------------------------------------
2026-11-02 12:52 起，`walgit collab` 在**没有** `WALGIT_LOCAL_COMMANDS=1` 时直接拒绝：

    Error: `walgit collab` is not the collaboration interface any more: ...
    ... set WALGIT_LOCAL_COMMANDS=1.

实测：该错误走 **stderr**、**rc=1**（见 `--selftest` 的哨兵用例）。危险不在 rc 本身，而在调用方写法——
旧脚本是 `walgit collab board 2>/dev/null | grep …` / `… 2>&1 | tail`，管道取的是 `grep`/`tail` 的 rc，
于是「接口被拒」被吞成「看板是空的」「没有待审」，即**假绿**。

本封装是编排脚本的**唯一调用点**，做四件事：

1. 对子进程**显式**设置 `WALGIT_LOCAL_COMMANDS=1`（不依赖调用方 shell 碰巧 export 过）；
2. stdout 原样透出、stderr 原样透出，**不吞**、不折叠；
3. 把 CLI 的「接口已切换」哨兵识别为**独立失败**（rc=3）：即使将来 CLI 改成 rc=0、或把提示打到
   stdout，也不会被当成空看板；
4. 其它情形**原样返回子进程 rc**（0/1/2/…），绝不把非零折叠成 0。

用法::

    python -I -B tools/orchestration/walgit_collab.py board --format json
    out=$(python -I -B tools/orchestration/walgit_collab.py board) || exit 1

退出码：子进程 rc；**3** = CLI 报「不是本地协作接口」（fail-closed）；**2** = 本封装用法错；
**127** = 找不到 walgit；**124** = `WALGIT_COLLAB_TIMEOUT` 秒内没跑完（仅在该变量存在时启用）。

自测（`--selftest`）用**替身 walgit** 覆盖：正常成功/rc 透传/哨兵 rc=0/哨兵在 stdout/找不到可执行/
空参数/下游提前关管道（`| head` 不许崩），以及 `tools/orchestration/*.sh` 里不再出现
「裸调 `walgit collab`」（注释行不误报）。替身通过 `WALGIT_COLLAB_BIN` 注入（测试缝，生产不设）。
"""
import errno
import os
import re
import subprocess
import sys
import tempfile

#: CLI「本地命令接口未启用」的哨兵短语（上游 2026-11-02 原文）。
SENTINEL = "is not the collaboration interface"
#: 哨兵命中时的专用退出码（区别于子进程 rc，便于机判与排障）。
RC_SENTINEL = 3
RC_USAGE = 2
RC_NOT_FOUND = 127
RC_TIMEOUT = 124

USAGE = "用法：python -I -B tools/orchestration/walgit_collab.py <collab 子命令> [参数...]"


def _bin_prefix():
    """可执行前缀。默认 `walgit`；`WALGIT_COLLAB_BIN` 仅供自测注入替身。

    以 `.py` 结尾时用当前解释器执行——Windows 的 CreateProcess 不能直接执行 `.bat/.cmd`，
    用 Python 替身才跨平台。
    """
    target = os.environ.get("WALGIT_COLLAB_BIN") or "walgit"
    if target.lower().endswith(".py"):
        return [sys.executable, target]
    return [target]


def run_collab(args):
    """跑一次 `walgit collab <args>`；返回 ``(rc, stdout_bytes, stderr_bytes)``。

    子进程环境显式设 ``WALGIT_LOCAL_COMMANDS=1``：这是本封装的**唯一**职责边界，
    调用方不需要也不应该自己再 export。
    """
    cmd = _bin_prefix() + ["collab"] + [str(a) for a in args]
    env = dict(os.environ)
    env["WALGIT_LOCAL_COMMANDS"] = "1"
    kwargs = {}
    timeout = os.environ.get("WALGIT_COLLAB_TIMEOUT")
    if timeout:
        kwargs["timeout"] = float(timeout)
    try:
        proc = subprocess.run(
            cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs
        )
    except FileNotFoundError:
        msg = "walgit_collab: 找不到可执行文件 %r（PATH 里没有 walgit？）\n" % cmd[0]
        return RC_NOT_FOUND, b"", msg.encode("utf-8")
    except subprocess.TimeoutExpired:
        msg = "walgit_collab: `walgit collab` 超过 %ss 未返回，fail-closed\n" % timeout
        return RC_TIMEOUT, b"", msg.encode("utf-8")
    return proc.returncode, proc.stdout or b"", proc.stderr or b""


def has_sentinel(out, err):
    """stdout/stderr 任一处出现哨兵即视为「本地接口未生效」。"""
    blob = (out or b"") + b"\n" + (err or b"")
    return SENTINEL in blob.decode("utf-8", "replace")


def _silence(stream):
    """下游关了管道 ⇒ 把该流重定向到 devnull，避免解释器退出时二次 flush 再抛。"""
    try:
        devnull = os.open(os.devnull, os.O_WRONLY)
        try:
            os.dup2(devnull, stream.fileno())
        finally:
            os.close(devnull)
    except (OSError, ValueError, AttributeError):
        pass


def _emit(stream, data):
    if not data:
        return
    buf = getattr(stream, "buffer", None)
    try:
        if buf is not None:
            buf.write(data)
            buf.flush()
        else:  # 被重定向成 StringIO 的测试/嵌入场景
            stream.write(data.decode("utf-8", "replace"))
            stream.flush()
    except BrokenPipeError:
        # `… | head` / `… | grep -m1` 提前关闭管道：这不是失败（rc 仍按子进程给），
        # 但**必须**吞掉，否则封装自己会带着 traceback 退出、把正常命令变成 rc=1。
        _silence(stream)
    except OSError as exc:
        if getattr(exc, "errno", None) != errno.EPIPE:
            raise
        _silence(stream)


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv)
    if not args:
        print(USAGE, file=sys.stderr)
        return RC_USAGE

    rc, out, err = run_collab(args)
    if has_sentinel(out, err):
        # 原始两路都放出来：排障需要原文；但**绝不**把它当成正常 stdout 交回调用方。
        _emit(sys.stdout, out)
        _emit(sys.stderr, err)
        print(
            "walgit_collab: FAIL-CLOSED：walgit CLI 拒绝本地协作接口"
            "（`is not the collaboration interface`）。"
            "WALGIT_LOCAL_COMMANDS=1 未生效，或 CLI 行为再次变更。"
            "不要重试，更不要把空输出当成「看板为空 / 没有待审」。",
            file=sys.stderr,
        )
        return RC_SENTINEL

    _emit(sys.stdout, out)
    _emit(sys.stderr, err)
    return rc


# ---------------------------------------------------------------------------
# 卫生牙：编排脚本里不许再出现「裸调 walgit collab」
# ---------------------------------------------------------------------------

#: `walgit` 与 `collab` 之间有空白；`walgit_collab.py` / `walgit-collab` 不匹配。
BARE_COLLAB_RE = re.compile(r"(?<![\w.-])walgit\s+collab\b")
#: 同一逻辑行显式带开关的写法是允许的（卡面接受「显式带 env」这一等价形态）。
EXPLICIT_ENV_RE = re.compile(r"WALGIT_LOCAL_COMMANDS\s*=\s*1\s+walgit\s+collab\b")


def bare_collab_hits(text):
    """返回 ``[(lineno, line)]``：去注释后仍裸调 `walgit collab` 的行。

    规则（守卫扫代码不扫注释）：
      * 整行注释（首个非空白字符是 ``#``）⇒ 跳过；
      * 行尾注释（`` #`` 之后）⇒ 先剥掉再判；
      * 行内显式 ``WALGIT_LOCAL_COMMANDS=1 walgit collab`` ⇒ 放行。
    """
    hits = []
    for lineno, line in enumerate((text or "").splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        code = re.sub(r"\s+#.*$", "", stripped)
        if not BARE_COLLAB_RE.search(code):
            continue
        if EXPLICIT_ENV_RE.search(code):
            continue
        hits.append((lineno, code))
    return hits


def scan_orchestration_scripts(root):
    """扫 ``tools/orchestration/*.sh``；返回 ``(files_scanned, hits)``。"""
    import glob

    files = sorted(glob.glob(os.path.join(root, "tools", "orchestration", "*.sh")))
    hits = []
    for path in files:
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except OSError as exc:
            hits.append((path, 0, "读不了：%s" % exc))
            continue
        for lineno, code in bare_collab_hits(text):
            hits.append((path, lineno, code))
    return files, hits


# ---------------------------------------------------------------------------
# --selftest
# ---------------------------------------------------------------------------

_FAKE_WALGIT = r'''#!/usr/bin/env python3
import os, sys
mode = os.environ.get("FAKE_WALGIT_MODE", "ok")
calls = os.environ.get("FAKE_WALGIT_CALLS")
if calls:
    with open(calls, "a", encoding="utf-8") as fh:
        fh.write(" ".join(sys.argv[1:]) + "\n")
SENTINEL = "is not the collaboration interface any more: it is MCP at /mcp"
if mode == "ok":
    if os.environ.get("WALGIT_LOCAL_COMMANDS") != "1":
        sys.stderr.write("Error: `walgit collab` " + SENTINEL + "\n")
        sys.exit(0)
    print("OK:" + ":".join(sys.argv[1:]) + ":env=" + os.environ.get("WALGIT_LOCAL_COMMANDS", ""))
    sys.exit(0)
if mode == "sentinel_rc0":
    sys.stderr.write("Error: `walgit collab` " + SENTINEL + "\n")
    sys.exit(0)
if mode == "sentinel_stdout":
    print("Error: `walgit collab` " + SENTINEL)
    sys.exit(0)
if mode == "fail7":
    sys.stderr.write("boom\n")
    sys.exit(7)
sys.exit(99)
'''


def _run_wrapper(script, fake, calls, mode, args):
    """以**清空过 WALGIT_LOCAL_COMMANDS 的父环境**跑封装——证明确实是封装在设，不是继承。"""
    env = dict(os.environ)
    env.pop("WALGIT_LOCAL_COMMANDS", None)
    env.pop("WALGIT_COLLAB_TIMEOUT", None)
    env["WALGIT_COLLAB_BIN"] = fake
    env["FAKE_WALGIT_MODE"] = mode
    env["FAKE_WALGIT_CALLS"] = calls
    proc = subprocess.run(
        [sys.executable, "-I", "-B", script] + list(args),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return proc.returncode, proc.stdout.decode("utf-8", "replace"), proc.stderr.decode("utf-8", "replace")


def selftest():
    script = os.path.abspath(__file__)
    root = os.path.dirname(os.path.dirname(os.path.dirname(script)))
    cases = []

    def add(name, ok, detail=""):
        cases.append((name, bool(ok), detail))

    with tempfile.TemporaryDirectory(prefix="walgit-collab-selftest-") as tmp:
        fake = os.path.join(tmp, "fake_walgit.py")
        with open(fake, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(_FAKE_WALGIT)
        calls = os.path.join(tmp, "calls.log")

        # --- 0) 负对照：替身自己在没有 env 时**必须**报哨兵（证明 env 牙不是空转）---
        env0 = dict(os.environ)
        env0.pop("WALGIT_LOCAL_COMMANDS", None)
        env0["WALGIT_COLLAB_BIN"] = fake
        env0["FAKE_WALGIT_MODE"] = "ok"
        env0["FAKE_WALGIT_CALLS"] = calls
        direct = subprocess.run(
            [sys.executable, "-I", "-B", fake, "collab", "board"],
            env=env0, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        add(
            "neg-control:fake-without-env-prints-sentinel",
            direct.returncode == 0 and SENTINEL in direct.stderr.decode("utf-8", "replace"),
            "rc=%r stderr=%r" % (direct.returncode, direct.stderr[:120]),
        )

        # --- 1) 成功：env 被显式设成 1，stdout 原样透出，rc=0 ---
        open(calls, "w").close()
        rc, out, err = _run_wrapper(script, fake, calls, "ok", ["board", "--format", "json"])
        add("ok:rc0", rc == 0, "rc=%r" % rc)
        add("ok:env-is-1", "env=1" in out, "stdout=%r" % out[:160])
        add("ok:stdout-passthrough", "OK:collab:board:--format:json:env=1" in out, "stdout=%r" % out[:160])
        add("ok:stderr-empty", err.strip() == "", "stderr=%r" % err[:160])
        with open(calls, encoding="utf-8") as fh:
            invoked = [l for l in fh.read().splitlines() if l.strip()]
        add("ok:child-invoked-once", len(invoked) == 1, "calls=%r" % invoked)

        # --- 2) 哨兵 + rc=0（最坏形态：管道假绿）⇒ 封装必须 rc=3 且不吃掉原文 ---
        rc, out, err = _run_wrapper(script, fake, calls, "sentinel_rc0", ["board"])
        add("sentinel-rc0:rc3", rc == RC_SENTINEL, "rc=%r" % rc)
        add("sentinel-rc0:reason", "FAIL-CLOSED" in err, "stderr=%r" % err[:200])
        add("sentinel-rc0:no-stdout-leak", out.strip() == "", "stdout=%r" % out[:160])

        # --- 3) 哨兵打到 stdout ⇒ 同样 rc=3（不依赖错误流位置）---
        rc, out, err = _run_wrapper(script, fake, calls, "sentinel_stdout", ["board"])
        add("sentinel-stdout:rc3", rc == RC_SENTINEL, "rc=%r" % rc)

        # --- 4) 普通失败 rc=7 ⇒ 原样透传，绝不折叠成 0 ---
        rc, out, err = _run_wrapper(script, fake, calls, "fail7", ["board"])
        add("fail7:rc-passthrough", rc == 7, "rc=%r" % rc)
        add("fail7:stderr-passthrough", "boom" in err, "stderr=%r" % err[:160])

        # --- 5) 空参数 ⇒ 用法错 rc=2，且不调用子进程 ---
        open(calls, "w").close()
        rc, out, err = _run_wrapper(script, fake, calls, "ok", [])
        add("usage:rc2", rc == RC_USAGE, "rc=%r" % rc)
        add("usage:message", "用法" in err, "stderr=%r" % err[:160])
        with open(calls, encoding="utf-8") as fh:
            add("usage:no-child-call", fh.read().strip() == "", "calls-file 非空")

        # --- 6) 找不到可执行 ⇒ rc=127 + 明示原因 ---
        env = dict(os.environ)
        env.pop("WALGIT_LOCAL_COMMANDS", None)
        env["WALGIT_COLLAB_BIN"] = os.path.join(tmp, "does-not-exist")
        proc = subprocess.run(
            [sys.executable, "-I", "-B", script, "board"],
            env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        add("missing-bin:rc127", proc.returncode == RC_NOT_FOUND, "rc=%r" % proc.returncode)
        add("missing-bin:reason", "找不到可执行文件" in proc.stderr.decode("utf-8", "replace"),
            "stderr=%r" % proc.stderr[:160])

    # --- 7) 下游提前关管道（`… | head`）⇒ 不许把正常命令崩成 rc=1 ---
    class _BrokenStream:
        """最小替身：write 即 EPIPE，且没有可用的 fileno。"""

        def __init__(self):
            self.buffer = self

        def write(self, _data):
            raise BrokenPipeError(errno.EPIPE, "Broken pipe")

        def flush(self):
            pass

        def fileno(self):
            raise OSError("no fileno")

    try:
        _emit(_BrokenStream(), b"anything")
        add("broken-pipe:no-crash", True)
    except BaseException as exc:  # noqa: BLE001 - 自测就是要抓住任何逃逸
        add("broken-pipe:no-crash", False, "raised %r" % (exc,))

    # --- 8) 卫生牙的**负例/退化形态**（先自证判据不恒真、空输入不空过）---
    add("guard:bare-flagged", len(bare_collab_hits("walgit collab board")) == 1)
    add("guard:comment-ignored", bare_collab_hits("# walgit collab board") == [])
    add("guard:inline-comment-ignored", bare_collab_hits("echo x  # walgit collab board") == [])
    add("guard:explicit-env-allowed",
        bare_collab_hits("WALGIT_LOCAL_COMMANDS=1 walgit collab board") == [])
    add("guard:wrapper-name-not-flagged",
        bare_collab_hits("python walgit_collab.py board") == [])
    add("guard:empty-input", bare_collab_hits("") == [])

    files, hits = scan_orchestration_scripts(root)
    add("guard:scanned-at-least-3-scripts", len(files) >= 3, "files=%r" % files)
    add("guard:no-bare-collab-in-orchestration", hits == [], "hits=%r" % hits)

    failed = [c for c in cases if not c[1]]
    for name, ok, detail in cases:
        if ok:
            print("PASS %s" % name)
        else:
            print("FAIL %s: %s" % (name, detail))
    print("walgit_collab selftest: %d/%d" % (len(cases) - len(failed), len(cases)))
    return 0 if not failed else 1


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--selftest":
        sys.exit(selftest())
    sys.exit(main())
