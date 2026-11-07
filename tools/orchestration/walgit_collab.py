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
3. 把 CLI 的「接口已切换」哨兵**行**识别为独立失败（rc=3）：即使将来 CLI 改成 rc=0、或把提示打到
   stdout，也不会被当成空看板。**边界**：只对**行首锚定的已知哨兵行** fail-closed；其它
   「Error 文本 + rc=0」仍按子进程 rc 原样透传（真实 CLI 以 rc=1 拒绝，rc 形态的报错一律非零）；
4. 其它情形**原样返回子进程 rc**（0/1/2/…），绝不把非零折叠成 0。

用法::

    python -I -B tools/orchestration/walgit_collab.py board --format json
    out=$(python -I -B tools/orchestration/walgit_collab.py board) || exit 1

退出码：子进程 rc；**3** = CLI 报「不是本地协作接口」（fail-closed）；**2** = 本封装用法错；
**127** = 找不到 walgit；**124** = `WALGIT_COLLAB_TIMEOUT` 秒内没跑完（仅在该变量存在时启用）。

自测（`--selftest`）用**替身 walgit** 覆盖：正常成功/rc 透传/哨兵 rc=0/哨兵在 stdout/找不到可执行/
空参数/下游提前关管道（`| head` 不许崩）。卫生牙覆盖**三个扫描域**，且每域都有合成负例：
`tools/orchestration/*.sh`（引用感知剥注释）、`tools/**/*.py`（AST：裸调/变量形态红，env 或已证
helper 绿，helper 被掏空后重新变红）、`tools/orchestration/briefs/**/*.md`（每个命令必须紧跟
`WALGIT_LOCAL_COMMANDS=1`）。替身通过 `WALGIT_COLLAB_BIN` 注入（测试缝，生产不设）。
"""
import ast
import errno
import glob
import os
import re
import subprocess
import sys
import tempfile

#: CLI「本地命令接口未启用」的哨兵（上游 2026-11-02 原文，仅作文档/提示用）。
SENTINEL = "is not the collaboration interface"
#: 真正的判定：**行首锚定**的整行错误。
#: 不能用子串——`thread`/`board` 的输出是数据，条目正文里**合法地**会引用这句话
#: （本卡自己的线程正文就引用了），子串匹配会把正常数据误判成「接口被拒」。
SENTINEL_LINE_RE = re.compile(
    r"^Error: .*is not the collaboration interface any more: it is MCP at /mcp",
    re.MULTILINE,
)
#: 2026-11-07（卡 `openmir2-collab-interface-wg-migration`）：walgit 0.14.0 起
#: `collab` 子命令被移除，clap 的实况报错是 `error: unrecognized subcommand 'collab'`。
GONE_RE = re.compile(
    r"^(?:error: unrecognized subcommand 'collab'|wg: unknown (?:command|subcommand) 'collab')",
    re.MULTILINE,
)
#: `wg` 的聚合失败。实测本仓库 14,529 条 inbox ref 会触发 `git cat-file --batch`
#: 管道互锁：wg 超时后打这一行，**且可能以 rc=0 结束**（最坏形态：假绿）。行首锚定。
GITFAILED_RE = re.compile(r"^wg: collab .*: GitFailed$", re.MULTILINE)
#: 哨兵命中时的专用退出码（区别于子进程 rc，便于机判与排障）。
RC_SENTINEL = 3
RC_USAGE = 2
RC_NOT_FOUND = 127
RC_TIMEOUT = 124

USAGE = "用法：python -I -B tools/orchestration/walgit_collab.py <collab 子命令> [参数...]"


def _bin_prefix():
    """可执行前缀。默认 `wg`（walgit 0.14.0 起协作接口在独立二进制 wg）；
    `WALGIT_COLLAB_BIN` 仅供自测注入替身。

    以 `.py` 结尾时用当前解释器执行——Windows 的 CreateProcess 不能直接执行 `.bat/.cmd`，
    用 Python 替身才跨平台。
    """
    target = os.environ.get("WALGIT_COLLAB_BIN") or "wg"
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
        msg = "walgit_collab: 找不到可执行文件 %r（PATH 里没有 wg？）\n" % cmd[0]
        return RC_NOT_FOUND, b"", msg.encode("utf-8")
    except subprocess.TimeoutExpired:
        msg = "walgit_collab: `walgit collab` 超过 %ss 未返回，fail-closed\n" % timeout
        return RC_TIMEOUT, b"", msg.encode("utf-8")
    return proc.returncode, proc.stdout or b"", proc.stderr or b""


def sentinel_reason(out, err):
    """stdout/stderr 任一处出现**行首锚定**的错误行 ⇒ 返回原因串，否则 None。

    只看行首：正常数据里引用这些字样（JSON 字符串、条目正文）不算失败。
    三种形态都判「协作接口不可用」：旧版拒绝本地命令、子命令已被移除、聚合 GitFailed。
    """
    sep = bytes([10])
    blob = ((out or b"") + sep + (err or b"")).decode("utf-8", "replace")
    # Windows 下子进程 stderr 是 CRLF；`$` 前残留的 CR 会让行首锚定匹配不到（自测实测）。
    blob = blob.replace(chr(13), "")
    if SENTINEL_LINE_RE.search(blob):
        return "is not the collaboration interface"
    if GONE_RE.search(blob):
        return "unrecognized subcommand 'collab'"
    if GITFAILED_RE.search(blob):
        return "collab aggregation GitFailed"
    return None


def has_sentinel(out, err):
    """兼容旧调用点：等价于 `sentinel_reason(...) is not None`。"""
    return sentinel_reason(out, err) is not None


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
    reason = sentinel_reason(out, err)
    if reason:
        # 原始两路都放出来：排障需要原文；但**绝不**把它当成正常 stdout 交回调用方。
        _emit(sys.stdout, out)
        _emit(sys.stderr, err)
        print(
            ("walgit_collab: FAIL-CLOSED：协作接口不可用（%s）。"
             "CLI 已换代（walgit 0.14.0 起协作命令是 `wg collab`），或聚合失败。"
             "不要重试，更不要把空输出当成「看板为空 / 没有待审」。") % reason,
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
#: Markdown 简报里命令的等价形态：`WALGIT_LOCAL_COMMANDS=1 walgit collab …`。
MD_ENV_RE = re.compile(r"WALGIT_LOCAL_COMMANDS=1\s+walgit\s+collab\b")


def _strip_shell_comment(line):
    """**引用感知**地剥行尾注释：只在引号外的 ` #` 处截断。

    朴素正则 ``\\s+#.*$`` 会把 ``echo " # x"; walgit collab board`` 的
    真实调用一起剥掉（review F4 实测漏报）。这里用最小状态机：
    单/双引号内不认注释；只有「前一个字符是空白」的 ``#`` 才开始注释。
    """
    quote = None
    i = 0
    while i < len(line):
        ch = line[i]
        if quote:
            if ch == "\\" and quote == '"':
                i += 2
                continue
            if ch == quote:
                quote = None
        else:
            if ch in ("'", '"'):
                quote = ch
            elif ch == "#" and (i == 0 or line[i - 1].isspace()):
                return line[:i]
        i += 1
    return line


def bare_collab_hits(text):
    """返回 ``[(lineno, line)]``：去注释后仍裸调 `walgit collab` 的行。

    规则（守卫扫代码不扫注释）：
      * 整行注释（首个非空白字符是 ``#``）⇒ 跳过；
      * 行尾注释（**引号外**的 `` #`` 之后）⇒ 先剥掉再判；
      * 行内显式 ``WALGIT_LOCAL_COMMANDS=1 walgit collab`` ⇒ 放行。
    """
    hits = []
    for lineno, line in enumerate((text or "").splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        code = _strip_shell_comment(stripped).strip()
        if not BARE_COLLAB_RE.search(code):
            continue
        if EXPLICIT_ENV_RE.search(code):
            continue
        hits.append((lineno, code))
    return hits


def _is_collab_argv(node):
    """``["walgit", "collab", …]`` 形态的列表/元组字面量。"""
    if not isinstance(node, (ast.List, ast.Tuple)):
        return False
    values = [e.value for e in node.elts
              if isinstance(e, ast.Constant) and isinstance(e.value, str)]
    return "walgit" in values and "collab" in values


def _defines_collab_env(fn):
    """函数体里出现 ``WALGIT_LOCAL_COMMANDS`` 字面量 ⇒ 可作为 env 提供者。"""
    for node in ast.walk(fn):
        if isinstance(node, ast.Constant) and node.value == "WALGIT_LOCAL_COMMANDS":
            return True
    return False


def _call_sets_collab_env(node, collab_env_funcs):
    """调用的 ``env=`` 是否确实带开关：内联 dict 含字面量，或调用已证的 helper。"""
    for kw in node.keywords:
        if kw.arg != "env":
            continue
        value = kw.value
        if (isinstance(value, ast.Call) and isinstance(value.func, ast.Name)
                and value.func.id in collab_env_funcs):
            return True
        try:
            return "WALGIT_LOCAL_COMMANDS" in ast.unparse(value)
        except Exception:  # pragma: no cover - 保守 fail-closed
            return False
    return False


#: 真正执行子进程的调用名（`subprocess.run` / 本仓的 `_run` / `run_cmd` …）。
_RUNNER_NAMES = {
    "run", "call", "check_call", "check_output", "Popen",
    "run_cmd", "run_cli", "_run", "_run_cli", "spawn", "_spawn",
}


def _callee_name(call):
    func = call.func
    if isinstance(func, ast.Name):
        return func.id
    if isinstance(func, ast.Attribute):
        return func.attr
    return ""


def _scope_nodes(scope):
    """该作用域内的节点，**不下钻**嵌套函数/类（同名变量不跨函数串味）。"""
    stack = list(ast.iter_child_nodes(scope))
    while stack:
        node = stack.pop()
        yield node
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda,
                             ast.ClassDef)):
            continue
        stack.extend(ast.iter_child_nodes(node))


def py_collab_hits(source, filename="<source>"):
    """扫 Python 源码：``walgit collab`` 的 argv 调用必须带 ``env=``。

    判据是 **AST** 而不是文本（注释/字符串里的字样不误报；换行/别名也不漏报）：
      * 直接内联的 ``["walgit", "collab", …]``；
      * 先赋值给名字（如 ``cmd = [...]`` / ``argv += [...]``）再交给**进程执行函数**
        （``run`` / ``_run`` / ``run_cmd`` …；按**作用域**解析，避免同名变量跨函数误报）。
    ``env=`` 必须内联含 ``WALGIT_LOCAL_COMMANDS``，或调用本文件里已证
    （函数体含该字面量）的 helper —— 删掉 helper 里的赋值会让调用点重新变红。
    """
    try:
        tree = ast.parse(source, filename=filename)
    except SyntaxError as exc:
        return [(getattr(exc, "lineno", 0) or 0, "SYNTAX ERROR: %s" % exc)]

    collab_env_funcs = {
        node.name for node in ast.walk(tree)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        and _defines_collab_env(node)
    }
    scopes = [tree] + [
        node for node in ast.walk(tree)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
    ]

    hits = []
    for scope in scopes:
        nodes = list(_scope_nodes(scope))
        collab_names = set()
        for node in nodes:
            if isinstance(node, ast.Assign) and _is_collab_argv(node.value):
                for target in node.targets:
                    if isinstance(target, ast.Name):
                        collab_names.add(target.id)
            elif isinstance(node, ast.AugAssign) and _is_collab_argv(node.value):
                if isinstance(node.target, ast.Name):
                    collab_names.add(node.target.id)
        for node in nodes:
            if not isinstance(node, ast.Call):
                continue
            if _callee_name(node) not in _RUNNER_NAMES:
                continue
            carries_argv = any(
                _is_collab_argv(arg)
                or (isinstance(arg, ast.Name) and arg.id in collab_names)
                for arg in node.args
            )
            if not carries_argv or _call_sets_collab_env(node, collab_env_funcs):
                continue
            snippet = " ".join((ast.get_source_segment(source, node) or "").split())
            hits.append((node.lineno, snippet[:140]))
    hits.sort()
    return hits


def md_collab_hits(text):
    """扫 Markdown：每个 ``walgit collab`` 都必须紧跟 ``WALGIT_LOCAL_COMMANDS=1``。

    先删掉合规形态再找残留 ⇒ 同一行有多条命令时，漏带开关的那条仍会被抓。
    """
    hits = []
    for lineno, line in enumerate((text or "").splitlines(), 1):
        residual = MD_ENV_RE.sub("", line)
        if BARE_COLLAB_RE.search(residual):
            hits.append((lineno, line.strip()[:140]))
    return hits


def _read(path):
    with open(path, encoding="utf-8", errors="replace") as fh:
        return fh.read()


def scan_shell_scripts(root):
    """扫 ``tools/orchestration/*.sh``；返回 ``(files_scanned, hits)``。"""
    files = sorted(glob.glob(os.path.join(root, "tools", "orchestration", "*.sh")))
    hits = []
    for path in files:
        try:
            text = _read(path)
        except OSError as exc:
            hits.append((path, 0, "读不了：%s" % exc))
            continue
        for lineno, code in bare_collab_hits(text):
            hits.append((path, lineno, code))
    return files, hits


def scan_python_tools(root):
    """扫 ``tools/**/*.py``（AST）；返回 ``(files_scanned, hits)``。

    **显式排除** ``mir2-rs/tests/parity/coverage/recon_gates.py``（不在 tools/ 下，
    且由卡 ``openmir2-recon-gates-false-green`` 覆盖）——本函数的域就是 tools/。
    """
    base = os.path.join(root, "tools")
    files = []
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = [d for d in dirnames if d != "__pycache__"]
        for name in filenames:
            if name.endswith(".py"):
                files.append(os.path.join(dirpath, name))
    files.sort()
    hits = []
    for path in files:
        try:
            text = _read(path)
        except OSError as exc:
            hits.append((path, 0, "读不了：%s" % exc))
            continue
        for lineno, code in py_collab_hits(text, path):
            hits.append((path, lineno, code))
    return files, hits


def scan_briefs(root):
    """扫 ``tools/orchestration/briefs/**/*.md``；返回 ``(files_scanned, hits)``。"""
    base = os.path.join(root, "tools", "orchestration", "briefs")
    files = []
    for dirpath, dirnames, filenames in os.walk(base):
        for name in filenames:
            if name.endswith(".md"):
                files.append(os.path.join(dirpath, name))
    files.sort()
    hits = []
    for path in files:
        try:
            text = _read(path)
        except OSError as exc:
            hits.append((path, 0, "读不了：%s" % exc))
            continue
        for lineno, code in md_collab_hits(text):
            hits.append((path, lineno, code))
    return files, hits
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
if mode == "phrase_in_data":
    print('{"note":"Error: `walgit collab` ' + SENTINEL + ' any more: it is MCP at /mcp"}')
    sys.exit(0)
if mode == "no_collab":
    sys.stderr.write("error: unrecognized subcommand 'collab'" + chr(10))
    sys.exit(2)
if mode == "gitfailed":
    sys.stderr.write("wg: collab board: GitFailed" + chr(10))
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

        # --- 3b) 数据里**合法引用**这句话（条目正文/JSON 字符串）⇒ 不许误判成失败 ---
        rc, out, err = _run_wrapper(script, fake, calls, "phrase_in_data", ["thread", "x"])
        add("phrase-in-data:rc0", rc == 0, "rc=%r stderr=%r" % (rc, err[:160]))
        add("phrase-in-data:passthrough", SENTINEL in out, "stdout=%r" % out[:160])

        # --- 3c) 上游移除 `collab` 子命令（walgit 0.14.0 实况）⇒ 必须判 rc=3 ---
        rc, out, err = _run_wrapper(script, fake, calls, "no_collab", ["board"])
        add("no-collab-subcommand:rc3", rc == RC_SENTINEL, "rc=%r stderr=%r" % (rc, err[:160]))
        add("no-collab-subcommand:reason", "接口不可用" in err and "unrecognized subcommand" in err,
            "stderr=%r" % err[:200])

        # --- 3d) wg 聚合失败但 rc=0（假绿最坏形态）⇒ 同样 rc=3 ---
        rc, out, err = _run_wrapper(script, fake, calls, "gitfailed", ["board"])
        add("wg-gitfailed:rc3", rc == RC_SENTINEL, "rc=%r stderr=%r" % (rc, err[:160]))
        add("wg-gitfailed:reason", "GitFailed" in err, "stderr=%r" % err[:200])

        # --- 3e) 负例：同一句话出现在数据行中间（不是行首）不许误判 ---
        add("guard:gitfailed-not-in-data",
            sentinel_reason(b'{"x":"wg: collab board: GitFailed"}', b"") is None)
        add("guard:other-subcommand-not-gone",
            sentinel_reason(b"", b"error: unrecognized subcommand 'board'") is None)

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
    # F4：引号内的「空格+#」不是注释，后面的真实调用必须仍被抓到。
    add("guard:quote-hash-not-comment",
        len(bare_collab_hits('echo " # x"; walgit collab board')) == 1,
        "hits=%r" % bare_collab_hits('echo " # x"; walgit collab board'))

    # --- .py 扫描域（AST）：裸调/变量形态必须红，env 或已证 helper 必须绿 ---
    py_bare = 'import subprocess\nsubprocess.run(["walgit", "collab", "board"])\n'
    py_var = ('import subprocess\ncmd = ["walgit", "collab", "board"]\n'
              'subprocess.run(cmd, capture_output=True)\n')
    py_inline_ok = ('import os, subprocess\n'
                    'subprocess.run(["walgit", "collab", "board"],'
                    ' env=dict(os.environ, WALGIT_LOCAL_COMMANDS="1"))\n')
    py_helper_ok = ('import os, subprocess\n'
                    'def _collab_env():\n'
                    '    e = dict(os.environ)\n'
                    '    e["WALGIT_LOCAL_COMMANDS"] = "1"\n'
                    '    return e\n'
                    'subprocess.run(["walgit", "collab", "board"], env=_collab_env())\n')
    py_helper_broken = ('import os, subprocess\n'
                        'def _collab_env():\n'
                        '    return dict(os.environ)\n'
                        'subprocess.run(["walgit", "collab", "board"], env=_collab_env())\n')
    add("guard-py:bare-flagged", len(py_collab_hits(py_bare)) == 1)
    add("guard-py:variable-flagged", len(py_collab_hits(py_var)) == 1)
    add("guard-py:inline-env-allowed", py_collab_hits(py_inline_ok) == [])
    add("guard-py:helper-env-allowed", py_collab_hits(py_helper_ok) == [])
    # 删掉 helper 里的赋值 ⇒ 调用点重新变红（helper 不能成为免检通道）。
    add("guard-py:broken-helper-flagged", len(py_collab_hits(py_helper_broken)) == 1,
        "hits=%r" % py_collab_hits(py_helper_broken))
    add("guard-py:comment-only", py_collab_hits("# walgit collab board\n") == [])
    add("guard-py:empty-input", py_collab_hits("") == [])

    # --- .md（briefs）扫描域：命令必须紧跟前缀；同行混排时漏带的那条仍要红 ---
    add("guard-md:bare-flagged", len(md_collab_hits("run `walgit collab board`")) == 1)
    add("guard-md:prefixed-allowed",
        md_collab_hits("run `WALGIT_LOCAL_COMMANDS=1 walgit collab board`") == [])
    add("guard-md:mixed-line-flagged",
        len(md_collab_hits(
            "`WALGIT_LOCAL_COMMANDS=1 walgit collab board`; `walgit collab thread x`")) == 1)
    add("guard-md:empty-input", md_collab_hits("") == [])

    sh_files, sh_hits = scan_shell_scripts(root)
    py_files, py_hits = scan_python_tools(root)
    md_files, md_hits = scan_briefs(root)
    add("guard:scanned-at-least-3-shell", len(sh_files) >= 3, "files=%r" % sh_files)
    add("guard:shell-clean", sh_hits == [], "hits=%r" % sh_hits)
    add("guard:scanned-at-least-20-python", len(py_files) >= 20,
        "count=%d" % len(py_files))
    add("guard:python-clean", py_hits == [], "hits=%r" % py_hits)
    add("guard:scanned-at-least-5-briefs", len(md_files) >= 5, "files=%r" % md_files)
    add("guard:briefs-clean", md_hits == [], "hits=%r" % md_hits)

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
