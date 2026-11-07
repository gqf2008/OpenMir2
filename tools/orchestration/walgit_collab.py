#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""walgit_collab — 统一封装协作 CLI（默认 `wg collab`）：接口不可用时 fail-closed。

为什么存在（卡 `openmir2-walgit-local-commands-env`）
----------------------------------------------------
2026-11-02 12:52 起，`walgit collab` 在**没有** `WALGIT_LOCAL_COMMANDS=1` 时直接拒绝：

    Error: `walgit collab` is not the collaboration interface any more: ...
    ... set WALGIT_LOCAL_COMMANDS=1.

实测：该错误走 **stderr**、**rc=1**（见 `--selftest` 的哨兵用例）。危险不在 rc 本身，而在调用方写法——
旧脚本是 `walgit collab board 2>/dev/null | grep …` / `… 2>&1 | tail`，管道取的是 `grep`/`tail` 的 rc，
于是「接口被拒」被吞成「看板是空的」「没有待审」，即**假绿**。

本封装是编排脚本里**协作调用的统一入口**（读路径除外——读走 `collab_read.py`），做四件事：

1. 对子进程设置 `WALGIT_LOCAL_COMMANDS=1`（**旧版 CLI 的兼容开关**；walgit 0.14.0 起无作用，
   对 `wg` 也不影响，保留只为兼容旧主机）；
2. stdout 原样透出、stderr 原样透出，**不吞**、不折叠；
3. 把 CLI 的「接口已切换」哨兵**行**识别为独立失败（rc=3）：即使将来 CLI 改成 rc=0、或把提示打到
   stdout，也不会被当成空看板。**边界**：只对**行首锚定的已知哨兵行** fail-closed；其它
   「Error 文本 + rc=0」仍按子进程 rc 原样透传（真实 CLI 以 rc=1 拒绝，rc 形态的报错一律非零）；
4. 其它情形**原样返回子进程 rc**（0/1/2/…），绝不把非零折叠成 0。

用法::

    python -I -B tools/orchestration/walgit_collab.py board --format json
    out=$(python -I -B tools/orchestration/walgit_collab.py board) || exit 1

退出码：子进程 rc；**3** = 协作接口不可用（子命令已移除 / 聚合 GitFailed / 旧版拒绝本地命令，fail-closed）；
**2** = 本封装用法错；**127** = 找不到可执行文件；**124** = `WALGIT_COLLAB_TIMEOUT` 秒内没跑完（仅在该变量存在时启用）。

自测（`--selftest`）用**替身协作 CLI** 覆盖：正常成功/rc 透传/三种接口不可用哨兵（rc=0 与 stdout 两形态）/
找不到可执行/空参数/下游提前关管道（`| head` 不许崩）。卫生牙覆盖**三个扫描域**，且每域都有合成负例：
`tools/orchestration/*.sh`（引用感知剥注释）、`tools/**/*.py`（AST：裸调/变量形态红，env 或已证
helper 绿，helper 被掏空后重新变红）、`tools/orchestration/briefs/**/*.md`（读写分路规则）。替身通过 `WALGIT_COLLAB_BIN` 注入（测试缝，生产不设）。
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

    子进程环境显式设 ``WALGIT_LOCAL_COMMANDS=1``：旧版 CLI 需要的开关，对 `wg` 无作用，
    保留只为兼容旧主机；调用方不需要自己再 export。
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

#: 2026-11-07（卡 `openmir2-collab-interface-wg-migration`）：walgit 0.14.0 起
#: `collab` 子命令迁到独立二进制 `wg`；读路径还要再走 `collab_read.py`
#: （`wg` 在本仓库读不动：14,529 条 inbox ref 会让 `git cat-file --batch` 与管道互锁）。
#: 守卫规则从「必须带 WALGIT_LOCAL_COMMANDS=1」改为按**读写分路**判：
#:   ① 裸调 `walgit collab`（该二进制已无此子命令）⇒ 红；
#:   ② `wg collab <读子命令>` ⇒ 红，读必须走 collab_read；
#:   ③ `wg collab <写子命令>` ⇒ 放行。
READ_SUBCOMMANDS = frozenset(("board", "thread", "thread-heads", "report", "ls", "pr"))
#: 一行里可能有多条命令；按 shell 分隔符切段后再逐段分词（不用正则，避免转义歧义）。
def _segments(code):
    out, cur, i = [], [], 0
    while i < len(code):
        if code[i:i + 2] in ("&&", "||"):
            out.append("".join(cur)); cur = []; i += 2; continue
        ch = code[i]
        if ch in (";", "|"):
            out.append("".join(cur)); cur = []; i += 1; continue
        cur.append(ch); i += 1
    out.append("".join(cur))
    return out

_QUOTES = "'" + chr(34) + "`"


def _tokens(segment):
    return [t.strip(_QUOTES) for t in segment.split() if t.strip(_QUOTES)]


_SHELL_WRAPPERS = frozenset(("sudo", "env", "time", "command", "exec", "nohup",
                                "xargs", "nice", "stdbuf"))
#: 各执行器「吃一个值」的**短选项名**（`sudo -u root`、`exec -a x`、`xargs -I {}` …）
_VALUE_FLAGS = {
    "sudo": frozenset(("u", "g", "p", "C", "r", "t", "U", "h", "D", "R", "T", "a", "c")),
    "exec": frozenset(("a",)),
    "env": frozenset(("u", "C", "S")),
    "xargs": frozenset(("I", "n", "P", "d", "L", "s", "E", "a")),
    "nice": frozenset(("n",)),
    "stdbuf": frozenset(("i", "o", "e")),
    "time": frozenset(("f", "o")),
}
#: 长选项 → 等价的短名（只要长名在表里，就按「吃一个值」处理）
_LONG_TO_SHORT = {
    "user": "u", "group": "g", "prompt": "p", "unset": "u", "chdir": "C",
    "max-args": "n", "adjustment": "n", "output": "o", "format": "f",
    "delimiter": "d", "max-lines": "L", "max-chars": "s", "arg-file": "a",
    "eof": "E", "replace": "I", "max-procs": "P", "input": "i",
}
_ASSIGN_PREFIX_RE = re.compile("^[A-Za-z_][A-Za-z0-9_]*=")
#: 行首重定向（`2>&1`、`>out`、`&>`、`<<EOF`）与 shell/CMD 提示符
_REDIRECT_RE = re.compile("^[0-9]*(<<?|>>?|&>)")
_PROMPT_RE = re.compile("^[A-Za-z]:[" + chr(92) + chr(92) + "/].*>$")


def _is_noise(tok):
    return bool(_REDIRECT_RE.match(tok) or _PROMPT_RE.match(tok)) or tok in ("$", "PS>", "%")


def _takes_value(flag, value_flags):
    """`-u` / `--user` / `--user=x` / 组合短选项 `-iu` 是否吃掉下一个 token。"""
    if "=" in flag:
        return False
    name = flag.lstrip("-")
    if flag.startswith("--"):
        return _LONG_TO_SHORT.get(name) in value_flags
    return any(ch in value_flags for ch in name)


def _command_token(toks):
    """段首的**命令 token** 及其下标。

    跳过：`VAR=…` 赋值、行首重定向与提示符、以及执行器（sudo/env/time/command/exec/
    nohup/xargs/nice/stdbuf）**连同它自己的选项与取值**（`sudo -u root`、`exec -a x`、
    `time -f %E`、`xargs -I {}`）。
    """
    i = 0
    while i < len(toks):
        t = toks[i]
        low = t.lower()
        if _ASSIGN_PREFIX_RE.match(t) or _is_noise(t):
            i += 1
            continue
        if low in _SHELL_WRAPPERS:
            i += 1
            value_flags = _VALUE_FLAGS.get(low, frozenset())
            while i < len(toks):
                nxt = toks[i]
                if nxt == "--":
                    i += 1
                    continue
                if nxt.startswith("-"):
                    i += 2 if _takes_value(nxt, value_flags) else 1
                    continue
                if _ASSIGN_PREFIX_RE.match(nxt) or _is_noise(nxt):
                    i += 1
                    continue
                break
            continue
        return t, i
    return None, i


def _any_position_hit(toks):
    """**任意位置**出现 `walgit`/`wg` 且其后有 `collab` 即命中。

    只用于 markdown：散文里命令不一定在行首（「先 … ，再 walgit collab board」），
    双反引号套单反引号也靠整行兜住。
    """
    for i, t in enumerate(toks):
        if _bin_name(t) not in ("walgit", "wg"):
            continue
        rest = toks[i + 1:]
        if "collab" not in rest:
            continue
        j = rest.index("collab")
        sub = rest[j + 1] if j + 1 < len(rest) else ""
        if _bin_name(t) == "walgit" or sub in READ_SUBCOMMANDS:
            return True
    return False


def _bin_name(token):
    """命令 token 归一化成二进制名：去前导 `$(`/`(`/反引号，取路径 basename，去 `.exe`。"""
    t = token.strip(_QUOTES).lstrip("$(").strip("()")
    # 只按「句子级」标点切（不含 `.`，否则会把 `walgit.exe` 切碎）
    t = re.split("[" + "，。、；：！？,;:!?" + "]", t)[-1]
    parts = [x for x in re.split("[" + chr(92) + chr(92) + "/]", t) if x]
    t = parts[-1] if parts else t
    if t.lower().endswith(".exe"):
        t = t[:-4]
    return t.lower()


def collab_hit(code):
    """判定一段代码：命中返回原因串，放行返回 None。

    判定口径：**段首命令 token** 是 `walgit`/`walgit.exe`/`wg` 时才算（允许路径前缀、
    `$(`/`(` 包装、`VAR=…` 与 sudo 之类执行器前缀）；不是拿「行里出现过 walgit」当二进制。
    """
    for seg in _segments(code):
        toks = _tokens(seg)
        cmd, idx = _command_token(toks)
        if not cmd:
            continue
        name = _bin_name(cmd)
        rest = toks[idx + 1:]
        if name == "walgit" and "collab" in rest:
            return "walgit collab（该子命令已随 0.14.0 移除）"
        if name == "wg" and "collab" in rest:
            j = rest.index("collab")
            sub = rest[j + 1] if j + 1 < len(rest) else ""
            if sub in READ_SUBCOMMANDS:
                return "wg collab %s 是读路径，必须走 collab_read.py" % sub
    return None


def _strip_shell_comment(line):
    """**引用感知**地剥行尾注释：只在引号外的 ` #` 处截断。

    朴素正则 ``\\s+#.*$`` 会把 ``echo " # x"; wg collab board`` 的
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
    """返回 ``[(lineno, line)]``：去注释后仍违规的协作调用行。

    规则见 ``collab_hit``：裸调 `walgit collab` 红；`wg collab <读>` 红；写子命令放行。
    整行注释与引号外的行尾注释都跳过（守卫扫代码不扫注释）。
    """
    hits = []
    for lineno, line in enumerate((text or "").splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        code = _strip_shell_comment(stripped).strip()
        if collab_hit(code):
            hits.append((lineno, code))
    return hits


def _is_collab_argv(node):
    """``["walgit"|"wg", "collab", …]`` 形态的列表/元组字面量 ⇒ (binary, sub)。"""
    if not isinstance(node, (ast.List, ast.Tuple)):
        return None
    vals = [e.value for e in node.elts
            if isinstance(e, ast.Constant) and isinstance(e.value, str)]
    if "collab" not in vals:
        return None
    i = vals.index("collab")
    # 二进制取「collab 之前的段首命令 token」（同样跳过 VAR=… 与 sudo/env 包装）
    cmd, _idx = _command_token(vals[:i])
    binary = _bin_name(cmd) if cmd else None
    sub = vals[i + 1] if i + 1 < len(vals) else None
    return binary, sub


def _collab_violation(node):
    kind = _is_collab_argv(node)
    if kind is None:
        return None
    binary, sub = kind
    if binary == "walgit":
        return "bare walgit collab（该子命令已随 0.14.0 移除）"
    if binary == "wg" and sub in READ_SUBCOMMANDS:
        return "wg collab %s 是读路径，必须走 collab_read.py" % sub
    return None


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
    """扫 Python 源码：协作调用的 argv 按读写分路判（AST，不看注释/字符串）。

      * 直接内联的 ``["walgit"|"wg", "collab", …]``；
      * 先赋值给名字（如 ``cmd = [...]`` / ``argv += [...]``）再交给**进程执行函数**
        （``run`` / ``_run`` / ``run_cmd`` …；按**作用域**解析，避免同名变量跨函数误报）。
    """
    try:
        tree = ast.parse(source, filename=filename)
    except SyntaxError as exc:
        return [(getattr(exc, "lineno", 0) or 0, "SYNTAX ERROR: %s" % exc)]

    scopes = [tree] + [
        node for node in ast.walk(tree)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
    ]

    hits = []
    for scope in scopes:
        nodes = list(_scope_nodes(scope))
        # 名字 → 违规原因（写子命令不记，避免把合法写入误判）
        collab_names = {}
        for node in nodes:
            if isinstance(node, ast.Assign) and _is_collab_argv(node.value):
                reason = _collab_violation(node.value)
                if reason:
                    for target in node.targets:
                        if isinstance(target, ast.Name):
                            collab_names[target.id] = reason
            elif isinstance(node, ast.AugAssign) and _is_collab_argv(node.value):
                reason = _collab_violation(node.value)
                if reason and isinstance(node.target, ast.Name):
                    collab_names[node.target.id] = reason
        for node in nodes:
            if not isinstance(node, ast.Call):
                continue
            if _callee_name(node) not in _RUNNER_NAMES:
                continue
            reason = None
            for arg in node.args:
                if _is_collab_argv(arg):
                    reason = _collab_violation(arg)
                elif isinstance(arg, ast.Name):
                    reason = reason or collab_names.get(arg.id)
            if not reason:
                continue
            snippet = " ".join((ast.get_source_segment(source, node) or "").split())
            hits.append((node.lineno, snippet[:140]))
    hits.sort()
    return hits


def md_collab_hits(text):
    """扫 Markdown：命令写在反引号里，段首通常是散文 ⇒ **逐片段**判定。

    没有反引号的行退回按整行判（例如裸写一条命令）。
    """
    hits = []
    for lineno, line in enumerate((text or "").splitlines(), 1):
        spans = re.findall("`([^`]+)`", line)
        # 片段按命令行判 + 整行按「任意位置」判：散文里命令不一定在行首
        # （「先 … ，再 walgit collab board」），双反引号套单反引号也靠整行兜住
        if collab_hit(line) or any(collab_hit(span) for span in spans)                 or _any_position_hit(_tokens(line)):
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

    # --- 8) 卫生牙：读写分路（先自证判据不恒真、空输入不空过）---
    add("guard:legacy-walgit-flagged", len(bare_collab_hits("walgit collab board")) == 1)
    add("guard:legacy-env-no-longer-exempts",
        len(bare_collab_hits("WALGIT_LOCAL_COMMANDS=1 walgit collab board")) == 1)
    add("guard:comment-ignored", bare_collab_hits("# walgit collab board") == [])
    add("guard:inline-comment-ignored", bare_collab_hits("echo x  # walgit collab board") == [])
    add("guard:wg-read-flagged", len(bare_collab_hits("wg collab board --repo .")) == 1)
    add("guard:wg-thread-flagged", len(bare_collab_hits("wg collab thread x")) == 1)
    add("guard:wg-write-allowed", bare_collab_hits("wg collab entry --kind issue") == [])
    # reviewer 实测的漏检形态：二进制与子命令之间夹选项
    # reviewer 实测的漏检形态：二进制与子命令之间夹选项
    add("guard:walgit-with-flags-flagged",
        len(bare_collab_hits("walgit --config x collab board")) == 1)
    add("guard:walgit-join-with-flags-flagged",
        len(bare_collab_hits("walgit --config x collab join --push walgit")) == 1)
    # 负对照：refs/collab 路径片段不得被当成子命令
    # reviewer round2 的形态表：路径前缀 / 命令替换 / 子 shell / .exe / 误报负例
    add("guard:path-prefixed-flagged",
        len(bare_collab_hits("C:\\bin\\walgit collab board")) == 1)
    add("guard:dot-slash-flagged", len(bare_collab_hits("./walgit collab board")) == 1)
    add("guard:cmd-subst-flagged", len(bare_collab_hits("$(walgit collab board)")) == 1)
    add("guard:subshell-flagged", len(bare_collab_hits("(walgit collab board)")) == 1)
    add("guard:exe-suffix-flagged", len(bare_collab_hits("walgit.exe collab board")) == 1)
    # 执行器选项 / xargs / 重定向 / 提示符（reviewer round3 的边角表）
    add("guard:sudo-u-flagged", len(bare_collab_hits("sudo -u root walgit collab board")) == 1)
    add("guard:env-i-flagged", len(bare_collab_hits("env -i walgit collab board")) == 1)
    add("guard:time-p-flagged", len(bare_collab_hits("time -p walgit collab board")) == 1)
    add("guard:command-dd-flagged", len(bare_collab_hits("command -- walgit collab board")) == 1)
    add("guard:exec-a-flagged", len(bare_collab_hits("exec -a x walgit collab board")) == 1)
    add("guard:xargs-flagged", len(bare_collab_hits("xargs walgit collab board")) == 1)
    add("guard:pipe-xargs-flagged", len(bare_collab_hits("echo board | xargs walgit collab board")) == 1)
    add("guard:redirect-first-flagged", len(bare_collab_hits("2>&1 walgit collab board")) == 1)
    add("guard:prompt-flagged", len(bare_collab_hits("$ walgit collab board")) == 1)
    # round1 的 F1/F2/F3：长选项、组合短选项、time、heredoc、CMD 提示符、标点
    add("guard:time-f-flagged", len(bare_collab_hits("time -f %E walgit collab board")) == 1)
    add("guard:sudo-long-user-flagged", len(bare_collab_hits("sudo --user root walgit collab board")) == 1)
    add("guard:sudo-combined-short-flagged", len(bare_collab_hits("sudo -iu root walgit collab board")) == 1)
    add("guard:nice-n-flagged", len(bare_collab_hits("nice -n 10 walgit collab board")) == 1)
    add("guard:stdbuf-o-flagged", len(bare_collab_hits("stdbuf -o L walgit collab board")) == 1)
    add("guard:xargs-I-flagged", len(bare_collab_hits("xargs -I {} walgit collab board")) == 1)
    add("guard:heredoc-prefix-flagged", len(bare_collab_hits("<<EOF walgit collab board")) == 1)
    add("guard:cmd-prompt-flagged",
        len(bare_collab_hits("C:\\Users\\gxh> walgit collab board")) == 1)
    add("guard-md:cjk-punct-flagged",
        len(md_collab_hits("先读卡，walgit collab board")) == 1)
    add("guard:sudo-apt-not-flagged", bare_collab_hits("sudo apt install wg") == [])
    # markdown：双反引号套单反引号 / 片段外裸命令
    add("guard-md:double-backtick",
        len(md_collab_hits("``walgit `collab` board``")) == 1)
    add("guard-md:bare-outside-span",
        len(md_collab_hits("先 `git fetch`，再 walgit collab board")) == 1)
    add("guard:fetch-walgit-not-flagged",
        bare_collab_hits("git fetch walgit --tags collab") == [])
    add("guard:echo-string-not-flagged",
        bare_collab_hits('echo "walgit collab board"') == [])
    add("guard-py:fetch-walgit-not-flagged",
        py_collab_hits('import subprocess' + chr(10)
                       + 'subprocess.run(["git", "fetch", "walgit", "--tags", "collab"])' + chr(10)) == [])
    add("guard:refspec-not-flagged",
        bare_collab_hits("git fetch walgit '+refs/collab/*:refs/collab/*'") == [])
    add("guard:py-walgit-with-flags-flagged",
        len(py_collab_hits("import subprocess" + chr(10)
                           + 'subprocess.run(["walgit", "--config", "x", "collab", "board"])' + chr(10))) == 1)
    add("guard:read-after-write-same-line",
        len(bare_collab_hits("wg collab entry --kind issue && wg collab board")) == 1)
    add("guard:read-before-write-same-line",
        len(bare_collab_hits("wg collab thread x; wg collab entry --kind issue")) == 1)
    add("guard:wg-join-allowed", bare_collab_hits("wg collab join --principal w1") == [])
    add("guard:wrapper-name-not-flagged",
        bare_collab_hits("python walgit_collab.py board") == [])
    add("guard:empty-input", bare_collab_hits("") == [])
    add("guard:quote-hash-not-comment",
        len(bare_collab_hits('echo " # x"; wg collab board')) == 1,
        "hits=%r" % bare_collab_hits('echo " # x"; wg collab board'))

    # --- .py 扫描域（AST）：读必须走 collab_read；写可走 wg ---
    nl = chr(10)
    py_legacy = "import subprocess" + nl + 'subprocess.run(["walgit", "collab", "board"])' + nl
    py_var = ("import subprocess" + nl + 'cmd = ["wg", "collab", "board"]' + nl
              + "subprocess.run(cmd, capture_output=True)" + nl)
    py_read_ok = ("import subprocess" + nl
                  + 'subprocess.run(["python", "-I", "-B", "collab_read.py", "board", "--json"])' + nl)
    py_write_ok = "import subprocess" + nl + 'subprocess.run(["wg", "collab", "entry", "--kind", "x"])' + nl
    add("guard-py:legacy-flagged", len(py_collab_hits(py_legacy)) == 1)
    add("guard-py:variable-read-flagged", len(py_collab_hits(py_var)) == 1)
    add("guard-py:collab-read-allowed", py_collab_hits(py_read_ok) == [])
    add("guard-py:wg-write-allowed", py_collab_hits(py_write_ok) == [])
    add("guard-py:comment-only", py_collab_hits("# walgit collab board" + nl) == [])
    add("guard-py:empty-input", py_collab_hits("") == [])

    # --- .md（briefs）扫描域：与 shell 同规则 ---
    add("guard-md:read-flagged", len(md_collab_hits("run `wg collab thread x`")) == 1)
    add("guard-md:legacy-flagged", len(md_collab_hits("run `walgit collab board`")) == 1)
    add("guard-md:write-allowed", md_collab_hits("run `wg collab join --principal w1`") == [])
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
