#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""docs_delivery_gate — 交付文档的**结构性**机判门（卡 `openmir2-docs-delivery-gate`，tier=L）。

背景：今天三类「文档假绿」全部真实发生、且**没有任何门报警**：

  ① 过时的「已验证」断言——例：`docs/delivery-report-*.md` 曾写
     `git diff --name-only 52be82700 HEAD | grep .rs` **为空**；后续合入 Rust 改动后变假，六道卫生门仍全绿。
  ② 表格结构错位——把换行塞进表行、或多一个 `|`（渲染多一列），靠人眼才发现（两次）。
  ③ hex 引用悬空——报告里几十处 commit / 条目引用，只用一次性脚本核过，没有常驻门。
  ④（c54c4875 补）**期望值过时**——owner 开窗前逐项打勾的清单里 `run_batch --list ⇒ 24（blocked 3 ⇒ 21）`
     实测已是 `24（blocked 2 ⇒ 22）`；勾到不符会让人停手排查一个不存在的问题。

**判定域（写清）**
  * 扫什么：`docs/delivery-*.md`、`docs/needs-human-*.md`、`docs/three-day-report-*.md`、
    `docs/rcard-*.md`——这些是 **owner 开窗前会照做的收口/清单文档**，其中的现在时断言会被**执行**。
    历史叙事文档**不在域内**：它们引用的是「当时的记录」，不是「现在的状态」（`--path` 可覆盖）。
  * **rc 语义**：`1` = 证伪（文档确实错了）；`2` = **判不了**（git 失败 / 命令跑不起来 / 登记表 rot）。
    git 自己失败（`cat-file` 的非「对象不存在」错误、`--is-ancestor` 的 rc>1、`ls-files` 失败）
    一律走 `GitUnavailable` ⇒ rc=2，**绝不**降级成 findings——那会把「我没法判断」说成「你错了」。
  * `command_output` 的 `argv[0]` 有白名单（bash/git/python/python3），且**禁 `bash -c`**。
  * 在哪跑：仓库根，只用只读 git 命令（`cat-file` / `merge-base --is-ancestor` / `ls-files` / `diff`）；
    `command_output` 里若 argv[0] 是裸 `bash`，本门会解析成 **Git bash 绝对路径**
    （Windows 上 System32 的 `bash.exe` 是 WSL stub，跑脚本 rc=1；找不到则 fail-closed）。
  * rc：`0` 全绿；`1` 有 findings；`2` 用法错 / **登记表自身过期（rot）** / claim 跑不起来。

**四类检查（全部结构性、可证伪）**
  A1 hex 引用：抽 `` `[0-9a-f]{7,40}` ``（**至少含一个 a–f**，纯数字如日期/时间戳不算）；
     `git cat-file -t` 逐个解析：
       * `commit` ⇒ 必须 `git merge-base --is-ancestor <oid> HEAD`；不是祖先 ⇒ 红
         （未合并分支 head 属正常事实 ⇒ 在登记表 `[[non_ancestor_commits]]` 里逐条说明后放行）；
       * 其它类型（blob/tree/tag）或**解析不到** ⇒ 必须登记在 `[hex_allow]`（逐 oid，带理由）；
       * **不得静默放行**：没登记的 blob / 缺失 oid 一律红。
  A2 表格结构：连续 `|` 行成组，逐行 `|` 数必须等于表头；**跳过 fenced code 块（```/~~~）与
     `/* … */` 区**，**内联代码 `` `…` `` 里的 `|` 不算**、`\\|` 不算。不一致 ⇒ 红并点名行号。
  A3 路径引用：`docs/` `tools/` `mir2-rs/` `src/` 形态的 token：
       * 存在且是文件 ⇒ 必须已跟踪（`git ls-files`）；
       * 目录（或以 `/` 结尾）⇒ 其下至少 1 个已跟踪文件；
       * 含 `*` 的 glob ⇒ 至少命中 1 个已跟踪文件；
       * 不存在 ⇒ 必须登记 `[external_paths]`（外部件/约定名），否则红。
  A4 「命令 ⇒ 期望值」对：登记在 `[[claims]]`，**门内真跑再比**，只支持两种可复算形态：
       * `kind = "diff_empty"`：`git diff --name-only <base> [<head>] [-- paths…]`（可选 `name_regex`
         过滤文件名）必须为空——正是 ① 的形态；
       * `kind = "command_output"`：跑 `<argv>`，rc=0 且 stdout 含 `expect` 子串——正是 ④ 的形态。
     登记的 `anchor` 必须仍出现在文档里（否则是**过期登记** ⇒ rc=2）。

**显式不支持（不假装覆盖，卡 `known_limits`）**
  * 「一段散文是否仍然为真」——**无法通用机判**，本门不做语义判断；
  * 需要网络 / 真窗口 / 外部产物的命令（没有证据就不能判）；
  * 任意 shell 管道 / 重定向（除 `diff_empty` 的 `name_regex` 形态）；多命令串联；
  * **本门 GREEN ≠「全仓没有过时数字」**：域外的历史叙事文档（如仍写着
    `场景数 24（blocked 3 ⇒ 入批 21）` 的 4 份旧报告）**不被检查**——「历史叙事不在判定域」
    是刻意的，但别把本门的 GREEN 当成全仓保证。
  以上在登记表 `[unsupported_claims]` 里逐条写明，且**允许为空只是暂时的**——新增 claim 若不属于
  两种受支持形态，必须落进该表（本门会打印，不判红）。

**登记表 rot 检测**（防止白名单变成垃圾桶）
  * `[hex_allow]` 里的 oid 必须**仍被引用**；否则 rc=2。
    （「仍是非 commit」**不复核**：git 对象不可变；而误登记的 commit 会先走祖先检查、
    永远不会被标记 used ⇒ 同样命中这条 rot，不必另写一条。）
  * `[[non_ancestor_commits]]` 必须仍未合并（若已变成 HEAD 祖先 ⇒ 文档里的「未合并」已过时）⇒ rc=2；
  * `[external_paths]` 登记的路径必须**仍然不存在**（若出现了 ⇒ 该登记过期）⇒ rc=2；
  * `[[claims]]` 的 anchor 必须仍在文档里，且对应命令必须能跑通。

用法：
  python -I -B tools/hygiene/docs_delivery_gate.py [--repo .] [--path F ...]
         [--registry tools/hygiene/docs_delivery_gate.toml] [--json out.json] [--selftest]
退出码：0 绿 / 1 有 findings / 2 用法或登记表问题。
"""
import argparse
import fnmatch
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

try:
    import tomllib
except ImportError:          # Python < 3.11
    tomllib = None

DEFAULT_REGISTRY = "tools/hygiene/docs_delivery_gate.toml"
#: 判定域：owner 开窗前会**照做**的收口/清单文档（历史叙事文档不在域内，见模块头）。
DEFAULT_GLOBS = (
    "docs/delivery-*.md",
    "docs/needs-human-*.md",
    "docs/three-day-report-*.md",
    "docs/rcard-*.md",
)

#: `command_output` 允许的 argv[0]（basename，小写）。白名单优先于「能跑就跑」：
#: 登记表是仓库内文件，但它也可能被一次坏 PR 改掉——不能让门变成任意命令执行入口。
ALLOWED_ARGV0 = frozenset(("bash", "bash.exe", "git", "git.exe", "python", "python.exe",
                           "python3", "python3.exe"))

HEX_RE = re.compile(r"`([0-9a-f]{7,40})`")
HEX_HAS_LETTER_RE = re.compile(r"[a-f]")
PATH_RE = re.compile(r"(?<![\w/])((?:docs|tools|mir2-rs|src)/[0-9A-Za-z_.\-/*]*)")
INLINE_CODE_RE = re.compile(r"`[^`\n]*`")
PATH_TRAIL = ".,;:!?)]}>'\"`，。；：！？）】》、"


class GateError(Exception):
    """用法错 / 登记表问题（rc=2）。"""


class GitUnavailable(GateError):
    """git 自己失败 ⇒ **判不了**（rc=2）。

    review ba964a07 N1：`cat-file` 的 rc≠0 与 `merge-base --is-ancestor` 的 rc>1 不能当成
    「对象不存在 / 不是祖先」——那会把「我没法判断」说成「你文档错了」（findings, rc=1）。
    """


#: 自测里的探针重入标记（R1：整跑一次 selftest 验 hexref 调用点）。
_DG_PROBE = {"r1": False}

#: 自测夹具用的 GIT_* 变量必须**剔除**：合并/门禁 shell 里若导出了
#: `GIT_DIR` / `GIT_WORK_TREE`，`git init` 会直接 fatal（我们实测过
#: `GIT_WORK_TREE` 单独存在 ⇒ `git init` 报 "not allowed without specifying GIT_DIR"），
#: 于是自测以 rc=1 崩掉，而它其实是**判不了**（环境问题）。
#: 卡 `openmir2-docs-gate-rc-semantics`：三态里「判不了」不能伪装成「证伪」。
FIXTURE_ENV_SCRUB = ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE",
                     "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                     "GIT_COMMON_DIR", "GIT_NAMESPACE", "GIT_PREFIX")


def _clean_git_env(base=None):
    """自测夹具的 git 环境：继承 + 固定作者身份 + **剔除 GIT_* 干扰**。"""
    env = dict(os.environ if base is None else base)
    for k in FIXTURE_ENV_SCRUB:
        env.pop(k, None)
    env.update(GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t",
               GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@t")
    return env


def _fixture_git(repo, args, env, git_cmd=("git",)):
    """夹具里的 git 调用：跑不起来 / rc≠0 一律 `GitUnavailable`（判不了，rc=2）。

    绝不用 AssertionError——那会变成未捕获异常（rc=1），把环境问题报成「门坏了」。
    `git_cmd` 只为自测注入（牙 N2b：给一个不存在的可执行 ⇒ 必须 GitUnavailable）。
    """
    try:
        r = subprocess.run(list(git_cmd) + list(args), cwd=repo, capture_output=True, text=True,
                           encoding="utf-8", errors="replace", env=env)
    except OSError as ex:
        raise GitUnavailable("git 跑不起来（%s：%s）⇒ 判不了" % (ex.__class__.__name__, ex))
    if r.returncode != 0:
        raise GitUnavailable("git %s rc=%d：%s ⇒ 判不了"
                             % (" ".join(args), r.returncode, (r.stderr or "").strip()[:200]))
    return r.stdout.strip()


# ---------------------------------------------------------------------------
# 外部命令解析（Windows 上裸 `bash` 是 System32 的 WSL stub ⇒ rc=1，见 card_window 同口径）
# ---------------------------------------------------------------------------
_BASH_CANDIDATES = (
    r"C:\Program Files\Git\usr\bin\bash.exe",
    r"C:\Program Files (x86)\Git\usr\bin\bash.exe",
    os.path.expandvars(r"%LocalAppData%\Programs\Git\usr\bin\bash.exe"),
)


def _git_run(repo, args, env, stdin_text=None):
    """夹具里的 git 调用（返回 CompletedProcess，供需要 rc 的调用方判断）。

    C6（reviewer-21 7060d720）：**rc=2 一律是「判不了」**——并发/慢盘下
    `merge-base --is-ancestor` 会返回 2（不是「不是祖先」）；进程起不来同理。
    调用方若拿到 rc=2 必须转 GitUnavailable，不得当成业务答案。
    """
    try:
        return subprocess.run(["git"] + list(args), cwd=repo, capture_output=True,
                              text=True, encoding="utf-8", errors="replace", env=env,
                              input=stdin_text)
    except OSError as ex:
        raise GitUnavailable("git 跑不起来（%s：%s）⇒ 判不了" % (ex.__class__.__name__, ex))


def _assert_fixture_consistent(tmp, env, side, c1, head):
    """夹具自证不变式：`side` 非 head 祖先（rc=1）、`c1` 是 head 祖先（rc=0）。

    这是 HEX-1/HEX-7/N1a/N1b 的前提。夹具在慢盘/并发下偶尔不成立——那时下游的红是
    **判不了**（rc=2/NEED），不是「门坏了」。守卫本身可被自测直接调用（C1 牙）。
    """
    side_rc = _git_run(tmp, ["merge-base", "--is-ancestor", side, head], env).returncode
    c1_rc = _git_run(tmp, ["merge-base", "--is-ancestor", c1, head], env).returncode
    if (side_rc, c1_rc) == (1, 0):
        return True
    try:
        log_dump = _git_run(tmp, ["log", "--oneline", "--graph", "--all", "--decorate"],
                            env).stdout.strip().replace("\n", " / ")[:300]
    except GitUnavailable:
        log_dump = ""
    raise GitUnavailable(
        "夹具自相矛盾：期望 side 非祖先(rc=1)/c1 祖先(rc=0)，实际 side=%d c1=%d"
        "（c1=%s side=%s head=%s；graph: %s）⇒ 判不了"
        % (side_rc, c1_rc, c1[:8], side[:8], head[:8], log_dump))


def hexref(oid, want=9):
    """取 oid 前缀，**必须含 a–f**（门的 HEX_RE 要求至少一个字母，纯数字会被当日期跳过）。

    规则 16 + review bdd1a28c F2：这是自测夹具的**关键不变式**，必须是模块级可单测函数；
    全数字前缀（约 0.8%/个）会让整组 hex 用例静默不触发——那正是 N1a/N1b 与 HEX-1/HEX-7
    两类假红的共同根因。找不到含字母的前缀（≤ want+6 位）⇒ 判不了（GitUnavailable），
    **不许**静默返回纯数字前缀。
    """
    for n in range(want, min(len(oid), want + 6) + 1):
        if re.search(r"[a-f]", oid[:n]):
            return oid[:n]
    raise GitUnavailable("oid %s 的前 %d 位没有 a–f，构造不出可解析的 hex 锚 ⇒ 判不了"
                         % (oid[:12], want + 6))


def _require_branch(repo, env, want="master"):
    """夹具必须停在 `want` 分支；不是 ⇒ GitUnavailable（判不了），不是裸 assert。"""
    got = _fixture_git(repo, ["rev-parse", "--abbrev-ref", "HEAD"], env)
    if got != want:
        raise GitUnavailable("夹具没切回 %s（实际 %r）⇒ 判不了" % (want, got))
    return True


def _is_wsl_stub(path):
    p = os.path.normcase(str(path).replace("/", os.sep))
    return (os.path.basename(p).startswith("bash")
            and ("system32" in os.path.dirname(p) or "sysnative" in os.path.dirname(p)))


def detect_git_bash():
    """环境变量 `DOCS_DELIVERY_GATE_BASH` / `CARD_WINDOW_GIT_BASH` → 常见安装位 → PATH（跳过 stub）。"""
    for var in ("DOCS_DELIVERY_GATE_BASH", "CARD_WINDOW_GIT_BASH"):
        c = os.environ.get(var, "").strip()
        if c and os.path.isfile(c) and not _is_wsl_stub(c):
            return c.replace(os.sep, "/")
    for c in _BASH_CANDIDATES:
        if c and os.path.isfile(c) and not _is_wsl_stub(c):
            return c.replace(os.sep, "/")
    found = shutil.which("bash")
    if found and os.path.isfile(found) and not _is_wsl_stub(found):
        return found.replace(os.sep, "/")
    return None


# ---------------------------------------------------------------------------
# git plumbing
# ---------------------------------------------------------------------------
def git(repo, *args):
    # 卡的 `--repo` 才是事实源：调用方 shell 里若有 GIT_DIR/GIT_WORK_TREE（合并脚本、
    # 某些门禁包装器会导出），`git -C <repo>` 会被 GIT_DIR 覆盖，读到的就是**别的仓库**
    # ——那会把「判不了/判错仓库」伪装成 findings。故显式剔除 GIT_*（卡 openmir2-docs-gate-rc-semantics）。
    return subprocess.run(["git"] + list(args), cwd=repo, capture_output=True,
                          text=True, encoding="utf-8", errors="replace",
                          env=_clean_git_env())


class GitCache:
    """`cat-file -t` / `merge-base --is-ancestor` 的记忆化（同一 oid 在文档里出现很多次）。

    **rc 语义（review ba964a07 N1）**：`cat-file` 只有在 git 明确说「对象不存在」时才算
    `None`（= 证伪）；其它非零 rc（仓库坏了 / GIT_DIR 指错 / 磁盘问题）一律 `GitUnavailable`
    （= 判不了，rc=2）。`merge-base --is-ancestor` 的 `0=祖先 / 1=不是祖先` 是**答案**，
    `2+` 是**判不了**。绝不能把环境抖动报成「文档引了野 oid」。
    """

    #: git 各版本对「对象确实不存在」的文案（小写匹配）。
    _MISSING_MARKERS = ("not a valid object name", "invalid object name", "bad object")

    def __init__(self, repo):
        self.repo = repo
        self._kind = {}
        self._anc = {}

    def kind(self, oid):
        if oid not in self._kind:
            r = git(self.repo, "cat-file", "-t", oid)
            if r.returncode == 0:
                self._kind[oid] = r.stdout.strip()
            else:
                err = ((r.stderr or "") + (r.stdout or "")).lower()
                if any(m in err for m in self._MISSING_MARKERS):
                    self._kind[oid] = None
                else:
                    raise GitUnavailable(
                        "git cat-file -t %s 失败（rc=%d）：%s —— 这是**判不了**（环境/仓库问题），"
                        "不是「文档引了野 oid」" % (oid, r.returncode, err.strip()[:200]))
        return self._kind[oid]

    def is_ancestor(self, oid, ref="HEAD"):
        key = (oid, ref)
        if key not in self._anc:
            r = git(self.repo, "merge-base", "--is-ancestor", oid, ref)
            if r.returncode not in (0, 1):
                raise GitUnavailable(
                    "git merge-base --is-ancestor %s %s 失败（rc=%d）：%s —— 判不了，"
                    "不是「不是祖先」" % (oid, ref, r.returncode,
                                        (r.stderr or "").strip()[:200]))
            self._anc[key] = r.returncode == 0
        return self._anc[key]


# ---------------------------------------------------------------------------
# 文档行处理
# ---------------------------------------------------------------------------
def mask_inline_code(line):
    """把内联代码 `` `…` `` 换成等长空格——代码里的 `|` / 路径不是 Markdown 结构。"""
    return INLINE_CODE_RE.sub(lambda m: " " * len(m.group(0)), line)


def table_view(lines):
    """返回 [(行号, 已屏蔽内联代码的行)]，**跳过 fenced code 块与 `/* … */` 区**。

    这两条就是今天真实踩过的坑：表格行被包进代码块或（在文档里的）块注释后，
    直接按 `|` 数扫会误判/漏判。
    """
    out = []
    in_fence = False
    fence_mark = None
    in_block = False
    for no, line in enumerate(lines, 1):
        s = line.strip()
        if in_fence:
            if s.startswith(fence_mark):
                in_fence = False
            continue
        if in_block:
            if "*/" in s:
                in_block = False
            continue
        if s.startswith("```") or s.startswith("~~~"):
            in_fence = True
            fence_mark = s[:3]
            continue
        if "/*" in s and "*/" not in s:
            in_block = True
            continue
        out.append((no, mask_inline_code(line)))
    return out


def pipe_count(line):
    """表格分隔符个数：`\\|` 是转义（不算），其余 `|` 都算。"""
    return line.replace("\\|", "").count("|")


# ---------------------------------------------------------------------------
# 四类检查
# ---------------------------------------------------------------------------
def check_hex(repo, docs, registry, cache):
    findings = []
    allow = set(registry.get("hex_allow", {}).get("items", []))
    non_anc = {(e["oid"], e["file"]) for e in registry.get("non_ancestor_commits", [])}
    used_allow = set()
    used_non_anc = set()
    for path, lines in docs:
        for no, line in enumerate(lines, 1):
            for m in HEX_RE.finditer(line):
                oid = m.group(1)
                if not HEX_HAS_LETTER_RE.search(oid):
                    continue          # 纯数字（日期 / 时间戳 / ActorId）不是 hash
                kind = cache.kind(oid)
                if kind is None:
                    if oid in allow:
                        used_allow.add(oid)
                        continue
                    findings.append(dict(
                        check="hex-missing", file=path, line=no, oid=oid,
                        msg="`%s` 解析不到（git cat-file -t 失败）：既不是本仓对象，也未登记 [hex_allow]"
                            % oid))
                    continue
                if kind == "commit":
                    if cache.is_ancestor(oid):
                        continue
                    if (oid, path) in non_anc:
                        used_non_anc.add((oid, path))
                        continue
                    findings.append(dict(
                        check="hex-not-ancestor", file=path, line=no, oid=oid,
                        msg="`%s` 是 commit 但**不是 HEAD 的祖先**（未合并分支 head？"
                            "请在 [[non_ancestor_commits]] 逐条登记理由，或改掉引用）" % oid))
                    continue
                # blob / tree / tag：交付文档里引用协作条目（blob）很常见 ⇒ 必须显式登记
                if oid in allow:
                    used_allow.add(oid)
                    continue
                findings.append(dict(
                    check="hex-non-commit", file=path, line=no, oid=oid,
                    msg="`%s` 是 %s 不是 commit（协作条目 oid？）：必须在 [hex_allow] 登记，"
                        "不得静默放行" % (oid, kind)))
    rot = []
    for oid in sorted(allow - used_allow):
        rot.append("hex_allow 里的 `%s` 未被任何交付文档引用（白名单过期）" % oid)
    # N4：若某个登记已经并入 HEAD，只报「已是祖先」那条（可操作：更新文档 + 删/刷登记）；
    # 同时再报「未被引用」会把人引去删登记、而不是改文档。
    now_anc = {(e["oid"], e["file"]) for e in registry.get("non_ancestor_commits", [])
               if cache.kind(e["oid"]) == "commit" and cache.is_ancestor(e["oid"])}
    for (oid, path) in sorted(non_anc - used_non_anc):
        if (oid, path) in now_anc:
            continue
        rot.append("non_ancestor_commits 里的 `%s`@%s 未被引用（白名单过期）" % (oid, path))
    for entry in registry.get("non_ancestor_commits", []):
        if (entry["oid"], entry["file"]) in now_anc:
            rot.append("non_ancestor_commits 里的 `%s`@%s 已是 HEAD 祖先 ⇒ 文档里的「未合并」过时了"
                       "（更新文档后删除/刷新该登记）" % (entry["oid"], entry["file"]))
    return findings, rot


def check_tables(docs):
    findings = []
    for path, lines in docs:
        rows = [(no, line.strip()) for no, line in table_view(lines)
                if line.strip().startswith("|") and line.count("|") >= 2]
        group = []
        prev = None
        groups = []
        for no, s in rows:
            if prev is not None and no == prev + 1:
                group.append((no, s))
            else:
                if group:
                    groups.append(group)
                group = [(no, s)]
            prev = no
        if group:
            groups.append(group)
        for g in groups:
            if len(g) < 2:
                continue
            n_head = pipe_count(g[0][1])
            for no, s in g[1:]:
                n = pipe_count(s)
                if n != n_head:
                    findings.append(dict(
                        check="table-pipes", file=path, line=no,
                        msg="表格行列数不一致：本行 %d 个 `|`，表头 %d 个（第 %d 行）"
                            % (n, n_head, g[0][0])))
    return findings, []


def check_paths(repo, docs, registry, tracked):
    findings = []
    external = {e["path"]: e for e in registry.get("external_paths", [])}
    used_ext = set()
    for path, lines in docs:
        for no, line in enumerate(lines, 1):
            for m in PATH_RE.finditer(line):
                token = m.group(1).rstrip(PATH_TRAIL)
                if not token or token in ("docs/", "tools/", "mir2-rs/", "src/"):
                    continue
                if "*" in token:
                    if any(fnmatch.fnmatch(p, token) for p in tracked):
                        continue
                elif token.endswith("/") or os.path.isdir(os.path.join(repo, token)):
                    prefix = token.rstrip("/") + "/"
                    if any(p.startswith(prefix) for p in tracked):
                        continue
                elif os.path.exists(os.path.join(repo, token)):
                    if token in tracked:
                        continue
                    findings.append(dict(
                        check="path-untracked", file=path, line=no, token=token,
                        msg="`%s` 存在但**未被 git 跟踪**（必须是已跟踪文件，或删掉该引用）" % token))
                    continue
                # 走到这里：不见了 / 目录下无跟踪文件 / glob 无命中
                if token in external:
                    used_ext.add(token)
                    continue
                findings.append(dict(
                    check="path-missing", file=path, line=no, token=token,
                    msg="`%s` 不存在（也不是已跟踪的文件/目录/glob）：请修正引用，"
                        "或登记到 [external_paths]（外部件/约定名）" % token))
    rot = []
    for token in sorted(set(external) - used_ext):
        rot.append("external_paths 里的 `%s` 未被引用（登记过期）" % token)
    for token in external:
        if os.path.exists(os.path.join(repo, token)):
            rot.append("external_paths 里的 `%s` 现在存在了 ⇒ 该登记过期（应直接引用）" % token)
    return findings, rot


def _doc_text(docs, path):
    for p, lines in docs:
        if p == path:
            return "\n".join(lines)
    return None


def check_claims(repo, docs, registry):
    findings = []
    rot = []
    for claim in registry.get("claims", []):
        cid = claim.get("id") or "<unnamed>"
        path = claim["file"]
        text = _doc_text(docs, path)
        if text is None:
            rot.append("claim %s：文档 `%s` 不在本次扫描域里（--path 覆盖或 globs 不含它）"
                       % (cid, path))
            continue
        if claim["anchor"] not in text:
            rot.append("claim %s：文档里找不到 anchor「%s」（登记过期：断言已被改写/删除）"
                       % (cid, claim["anchor"]))
            continue
        kind = claim.get("kind")
        if kind == "diff_empty":
            args = ["diff", "--name-only", claim["base"], claim.get("head", "HEAD")]
            if claim.get("paths"):
                args += ["--"] + list(claim["paths"])
            r = git(repo, *args)
            if r.returncode != 0:
                rot.append("claim %s：`git %s` 跑失败（rc=%d）：%s"
                           % (cid, " ".join(args), r.returncode, (r.stderr or "").strip()[:200]))
                continue
            files = [ln for ln in r.stdout.splitlines() if ln.strip()]
            if claim.get("name_regex"):
                rx = re.compile(claim["name_regex"])
                files = [f for f in files if rx.search(f)]
            if files:
                findings.append(dict(
                    check="claim-stale", file=path, line=0, claim=cid,
                    msg="「%s」已过时：`git %s` 现在有 %d 个文件（%s）——文档仍写「为空」"
                        % (claim.get("title", cid), " ".join(args), len(files),
                           ", ".join(files[:5]) + (" …" if len(files) > 5 else ""))))
        elif kind == "command_output":
            argv = list(claim["argv"])
            base0 = os.path.basename(argv[0]).lower()
            if base0 not in ALLOWED_ARGV0:
                rot.append("claim %s：argv[0]=%r 不在白名单 %s ⇒ 拒绝执行"
                           "（防登记表变成任意命令入口）"
                           % (cid, argv[0], sorted(ALLOWED_ARGV0)))
                continue
            if base0 in ("bash", "bash.exe") and any(
                    a in ("-c", "--command") for a in argv[1:]):
                rot.append("claim %s：禁 `bash -c`（会绕过 argv 白名单跑任意 shell）" % cid)
                continue
            if base0 in ("bash", "bash.exe") and not os.path.dirname(argv[0]):
                bash = detect_git_bash()
                if not bash:
                    rot.append("claim %s：本机找不到 Git bash（裸 `bash` 是 WSL stub）——"
                               "装 Git for Windows / 设 DOCS_DELIVERY_GATE_BASH" % cid)
                    continue
                argv[0] = bash
            try:
                r = subprocess.run(argv, cwd=repo, capture_output=True, text=True,
                                   encoding="utf-8", errors="replace", timeout=120,
                                   stdin=subprocess.DEVNULL)
            except (OSError, subprocess.TimeoutExpired) as ex:
                rot.append("claim %s：命令跑不起来（%r）——登记成 unsupported 或修环境" % (cid, ex))
                continue
            if r.returncode != 0:
                rot.append("claim %s：命令 rc=%d（%s）"
                           % (cid, r.returncode, (r.stderr or "").strip().splitlines()[-1:]))
                continue
            if claim["expect"] not in (r.stdout or ""):
                findings.append(dict(
                    check="claim-stale", file=path, line=0, claim=cid,
                    msg="「%s」已过时：命令输出里没有「%s」（实况见 stdout；文档需更新或去掉写死的数字）"
                        % (claim.get("title", cid), claim["expect"])))
        else:
            rot.append("claim %s：未知 kind=%r（只支持 diff_empty / command_output；"
                       "做不到的形态登记到 [unsupported_claims]）" % (cid, kind))
    return findings, rot


# ---------------------------------------------------------------------------
# 入口
# ---------------------------------------------------------------------------
def load_registry(path):
    if tomllib is None:
        raise GateError("需要 Python 3.11+（tomllib）；当前 %s" % sys.version.split()[0])
    try:
        with open(path, "rb") as fh:
            return tomllib.load(fh)
    except OSError as ex:
        raise GateError("读不到登记表 %s：%s" % (path, ex))
    except tomllib.TOMLDecodeError as ex:
        raise GateError("登记表 %s 不是合法 TOML：%s" % (path, ex))


def resolve_docs(repo, paths, registry):
    if paths:
        rels = [os.path.relpath(os.path.abspath(os.path.join(repo, p)), repo).replace(os.sep, "/")
                if not os.path.isabs(p) else os.path.relpath(p, repo).replace(os.sep, "/")
                for p in paths]
    else:
        globs = registry.get("domain", {}).get("globs") or list(DEFAULT_GLOBS)
        out = []
        for base, _dirs, files in os.walk(os.path.join(repo, "docs")):
            for fn in files:
                if not fn.endswith(".md"):
                    continue
                rel = os.path.relpath(os.path.join(base, fn), repo).replace(os.sep, "/")
                if any(fnmatch.fnmatch(rel, g) for g in globs):
                    out.append(rel)
        rels = sorted(out)
    docs = []
    for rel in rels:
        full = os.path.join(repo, rel)
        if not os.path.isfile(full):
            raise GateError("--path %s 不存在或不是文件" % rel)
        with open(full, encoding="utf-8", errors="replace") as fh:
            docs.append((rel, fh.read().splitlines()))
    return docs


def check(repo, docs, registry):
    wt = git(repo, "rev-parse", "--is-inside-work-tree")
    if wt.returncode != 0 or wt.stdout.strip() != "true":
        raise GitUnavailable(
            "git 不可用（`rev-parse --is-inside-work-tree` rc=%d：%s）⇒ **判不了**（rc=2）；"
            "本次不产出任何 findings" % (wt.returncode, (wt.stderr or "").strip()[:200]))
    ls = git(repo, "ls-files")
    if ls.returncode != 0:
        raise GitUnavailable("git ls-files 失败（rc=%d）：%s ⇒ 判不了（rc=2）"
                             % (ls.returncode, (ls.stderr or "").strip()[:200]))
    cache = GitCache(repo)
    tracked = set(ls.stdout.splitlines())
    findings, rot = [], []
    for fn in (lambda: check_hex(repo, docs, registry, cache),
               lambda: check_tables(docs),
               lambda: check_paths(repo, docs, registry, tracked),
               lambda: check_claims(repo, docs, registry)):
        f, r = fn()
        findings += f
        rot += r
    return findings, rot


def print_report(findings, rot, docs, registry):
    for f in findings:
        where = "%s:%d" % (f["file"], f["line"]) if f.get("line") else f["file"]
        print("RED   %-17s %s  %s" % (f["check"], where, f["msg"]))
    for r in rot:
        print("ROT   %s" % r)
    unsupported = registry.get("unsupported_claims", [])
    for u in unsupported:
        print("INFO  不支持：%s —— %s" % (u["form"], u["reason"]))
    print("DOCS_DELIVERY_GATE files=%d findings=%d rot=%d"
          % (len(docs), len(findings), len(rot)))
    if findings or rot:
        print("DOCS_DELIVERY_GATE RED（%d findings / %d rot）" % (len(findings), len(rot)))
    else:
        print("DOCS_DELIVERY_GATE GREEN（结构性检查全过；语义真伪不判，见 known_limits）")


def main(argv=None):
    ap = argparse.ArgumentParser(description="交付文档结构性门禁（hex / 表格 / 路径 / 命令期望值）")
    ap.add_argument("--repo", default=None)
    ap.add_argument("--path", action="append", default=[],
                    help="只扫这些文件（可重复）；缺省按登记表 domain.globs")
    ap.add_argument("--registry", default=None)
    ap.add_argument("--json", default=None, help="把 findings 落到 JSON")
    ap.add_argument("--selftest", action="store_true", help="离线自测（临时 git 仓库，不碰本仓）")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    repo = os.path.abspath(args.repo or
                           os.path.dirname(os.path.dirname(os.path.dirname(
                               os.path.abspath(__file__)))))
    reg_path = args.registry or os.path.join(repo, DEFAULT_REGISTRY)
    try:
        registry = load_registry(reg_path)
        docs = resolve_docs(repo, args.path, registry)
        findings, rot = check(repo, docs, registry)
    except GateError as ex:
        print("docs_delivery_gate: %s" % ex, file=sys.stderr)
        return 2
    print_report(findings, rot, docs, registry)
    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump({"findings": findings, "rot": rot}, fh, ensure_ascii=False, indent=1)
    return 2 if rot else (1 if findings else 0)


# ---------------------------------------------------------------------------
# 自测（每类 ≥2 个反例；删掉对应检查分支 ⇒ 这里的断言必红）
# ---------------------------------------------------------------------------
def selftest():
    """自测入口：**环境建不了夹具 ⇒ rc=2（判不了）**，不是 rc=1（证伪）。

    牙 N2c 钉住这条：把 `_selftest_run` 换成「抛 GitUnavailable」⇒ 本函数必须返回 2。
    """
    try:
        return _selftest_run()
    except GitUnavailable as ex:
        print("docs_delivery_gate selftest: 判不了（rc=2）—— 环境建不了 git 夹具：%s" % ex)
        return 2
    except OSError as ex:
        # 连临时目录都建不了（TEMP/TMP 不可写、磁盘满等）同样是「判不了」，不是「门坏了」。
        print("docs_delivery_gate selftest: 判不了（rc=2）—— 环境不能建临时夹具：%s: %s"
              % (ex.__class__.__name__, ex))
        return 2


def _selftest_run():
    fails = []
    checked = [0]

    def chk(cond, name):
        checked[0] += 1
        if cond:
            print("PASS " + name)
        else:
            fails.append(name)
            print("FAIL " + name)

    tmp = tempfile.mkdtemp(prefix="docs-delivery-gate-")
    try:
        env = _clean_git_env()

        def g(*a):
            return _fixture_git(tmp, a, env)

        g("init", "-q")
        g("config", "user.name", "t")
        g("config", "user.email", "t@t")
        os.makedirs(os.path.join(tmp, "docs"), exist_ok=True)
        os.makedirs(os.path.join(tmp, "tools", "x"), exist_ok=True)
        with open(os.path.join(tmp, "docs", "tracked.md"), "w", encoding="utf-8") as fh:
            fh.write("base\n")
        with open(os.path.join(tmp, "tools", "x", "a.py"), "w", encoding="utf-8") as fh:
            fh.write("print(1)\n")
        with open(os.path.join(tmp, "a.rs"), "w", encoding="utf-8") as fh:
            fh.write("fn a() {}\n")
        g("add", "-A")
        g("commit", "-q", "-m", "c1")
        c1 = g("rev-parse", "HEAD")
        # 未合并分支上的 commit
        g("checkout", "-q", "-b", "side")
        with open(os.path.join(tmp, "a.rs"), "w", encoding="utf-8") as fh:
            fh.write("fn a() { /* side */ }\n")
        g("commit", "-q", "-am", "side")
        side = g("rev-parse", "HEAD")
        g("checkout", "-q", "master")
        # fixture 守卫：切回失败会让 `side` 变成 HEAD 祖先，表现为 HEX-1/HEX-7 间歇红
        # （review ba964a07 观察到的「3 次里 1 次」形态）⇒ 这里显式钉住，别让它伪装成检查缺陷。
        _require_branch(tmp, env, "master")   # 裸 assert 会把环境问题崩成 rc=1（reviewer-21 C4）
        # HEAD 上再提交一次（只改 docs/tools ⇒ 便于 diff_empty 用例）
        with open(os.path.join(tmp, "docs", "tracked.md"), "w", encoding="utf-8") as fh:
            fh.write("base2\n")
        g("commit", "-q", "-am", "c3")
        head = g("rev-parse", "HEAD")
        # ---- 夹具自洽守卫（卡 openmir2-ps1-tool-registry）----
        # HEX-1/HEX-7 与 N1a/N1b 都依赖「side 不是 HEAD 的祖先、c1 是 HEAD 的祖先」这一夹具事实；
        # **归因订正（规则 16）**：早期把这里的误红说成「负载/慢盘下的竞态」，实测是**概率性夹具缺陷**——
        # 门的 HEX_RE 要求引用含 a–f，纯数字前缀（约 0.8%/个）会让整组 hex 用例静默不触发，
        # 与并发无关（60 次连跑 0 失败的对照见卡 openmir2-ps1-tool-registry）。
        # 无论哪种原因，夹具一旦没落到这个状态，下游报的都是「看起来像门坏了」的 FAIL。
        # 守卫抽成模块级函数，好让自测**直接单测它**（reviewer-21 C1：只有人读没有机器守 = 假牙）。
        _assert_fixture_consistent(tmp, env, side, c1, head)
        # C6（reviewer-21）：夹具里的 git 调用一律走 _git_run——**rc=2 也是「判不了」**
        # （并发/慢盘下 merge-base/cat-file 会返回 2；那不是「文档错了」，是环境判不了）。
        p = _git_run(tmp, ["hash-object", "-w", "--stdin"], env, stdin_text="x" + chr(10))
        if p.returncode == 2:
            raise GitUnavailable("夹具 git hash-object rc=2 ⇒ 判不了")
        blob = p.stdout.strip()

        # 规则 16：夹具先自证不变式——门的 HEX_RE 要求引用**至少含一个 a–f**（纯数字当日期跳过）。
        # 直接用 `c1_ref`/`side_ref` 会以约 0.8%/个的概率撞上「前缀全是数字」，于是该轮所有
        # hex 用例静默不触发（实测：30 次里 2 次 N1a/N1b 假红、25 次里 3 次 HEX-1/HEX-7 假红）。
        # 这里把前缀延长到出现字母为止；都到 15 位还没有字母 ⇒ 判不了（rc=2），不是红。
        c1_ref = hexref(c1)
        side_ref = hexref(side)
        blob_ref = hexref(blob, 8)
        # 未跟踪文件
        with open(os.path.join(tmp, "docs", "untracked.md"), "w", encoding="utf-8") as fh:
            fh.write("nope\n")

        # ---- N2（卡 openmir2-docs-gate-rc-semantics）：环境问题 ⇒ 判不了（rc=2），不是证伪 ----
        # 牙必须**行为化**：注入污染 env 再看清洗结果（只断言「env 里没有这些键」在清洗表
        # 被清空时会**真空通过**——变异自证抓到的假牙）。
        polluted = dict(os.environ, GIT_DIR="X", GIT_WORK_TREE="Y", GIT_INDEX_FILE="Z")
        cleaned = _clean_git_env(polluted)
        chk(all(k not in cleaned for k in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE"))
            and cleaned.get("GIT_AUTHOR_NAME") == "t",
            "N2a 注入 GIT_DIR/GIT_WORK_TREE/GIT_INDEX_FILE ⇒ 清洗后全剔除、作者身份仍在")
        # 清空 PATH ⇒ `git` 根本起不来：必须是 GitUnavailable（判不了），不是 AssertionError/OSError
        bare = _clean_git_env()
        # 注意：Windows 的 CreateProcess **用父进程的 PATH 找可执行**，改 env["PATH"] 没用
        # ⇒ 直接注入一个不存在的命令名（与「机器上没装 git」同一条失败路径）。
        try:
            _fixture_git(tmp, ["rev-parse", "HEAD"], bare, git_cmd=("no-such-git-binary",))
            n2b = "no-raise"
        except GitUnavailable as ex:
            n2b = "unavailable" if "判不了" in str(ex) else "wrong-text"
        except Exception as ex:  # noqa: BLE001 - 这里就要抓「任何别的异常」
            n2b = "other:%s" % ex.__class__.__name__
        chk(n2b == "unavailable", "N2b git 起不来 ⇒ GitUnavailable（判不了），实际 %r" % n2b)

        # N2b2（review b427aad2 F1）：**git 起来了但 rc≠0** 也必须 GitUnavailable——
        # 否则夹具带着半成品仓继续跑，产出一堆莫名其妙的 FAIL（看起来像门坏了）。
        fail_script = os.path.join(tmp, "fake_git_fail.py")
        with open(fail_script, "w", encoding="utf-8") as fh:
            fh.write("import sys" + chr(10) + "sys.stderr.write('fake git error')"
                     + chr(10) + "sys.exit(3)" + chr(10))
        fake_fail = (sys.executable, fail_script)
        try:
            _fixture_git(tmp, ["init"], env, git_cmd=fake_fail)
            n2b2 = "no-raise"
        except GitUnavailable as ex:
            n2b2 = "unavailable" if "rc=3" in str(ex) else "wrong-text:%s" % str(ex)[:60]
        except Exception as ex:  # noqa: BLE001
            n2b2 = "other:%s" % ex.__class__.__name__
        chk(n2b2 == "unavailable",
            "N2b2（F1）git 起来了但 rc=3 ⇒ GitUnavailable（不吞失败），实际 %r" % n2b2)

        real_run = globals()["_selftest_run"]

        def _boom():
            raise GitUnavailable("fixture boom（牙注入）")

        globals()["_selftest_run"] = _boom
        try:
            import contextlib as _cl
            import io as _io
            _buf = _io.StringIO()
            with _cl.redirect_stdout(_buf):
                n2c = selftest()
            n2c_out = _buf.getvalue()
        finally:
            globals()["_selftest_run"] = real_run
        chk(n2c == 2 and "判不了" in n2c_out and "rc=2" in n2c_out,
            "N2c 夹具建不起来 ⇒ selftest rc=2 且输出「判不了（rc=2）」（不是 rc=1）")

        # N2d（卡 openmir2-ps1-tool-registry）：**夹具自相矛盾**（side 变成 head 的祖先）
        # ⇒ 必须判「判不了」，绝不把下游 HEX/N1 的失败当成「门坏了」。
        _saved_fixture = globals()["_selftest_run"]

        def _bad_fixture():
            raise GitUnavailable("夹具自相矛盾：side 应为非祖先(rc=1)/c1 应为祖先(rc=0)，"
                                 "实际 side=0 c1=0（牙注入）⇒ 判不了")

        globals()["_selftest_run"] = _bad_fixture
        try:
            import contextlib as _cl3
            import io as _io3
            _buf3 = _io3.StringIO()
            with _cl3.redirect_stdout(_buf3):
                n2d = selftest()
            n2d_out = _buf3.getvalue()
        finally:
            globals()["_selftest_run"] = _saved_fixture
        chk(n2d == 2 and "夹具自相矛盾" in n2d_out and "rc=2" in n2d_out,
            "N2d 夹具自相矛盾 ⇒ selftest rc=2（判不了），不是把 HEX/N1 报成红")

        # ---- C1/C4（reviewer-21 7060d720）：守卫与分支检查本身必须有牙 ----
        fx = os.path.join(tmp, "c1fx")
        os.makedirs(fx, exist_ok=True)

        def _fg(*a):
            return _fixture_git(fx, a, env)

        _fg("init", "-q")
        _fg("config", "user.name", "t")
        _fg("config", "user.email", "t@t")
        with open(os.path.join(fx, "a.md"), "w", encoding="utf-8") as fh:
            fh.write("base\n")
        _fg("add", "-A")
        _fg("commit", "-q", "-m", "c1")
        c1_fx = _fg("rev-parse", "HEAD")
        _fg("checkout", "-q", "-b", "side")
        with open(os.path.join(fx, "a.md"), "w", encoding="utf-8") as fh:
            fh.write("side\n")
        _fg("commit", "-q", "-am", "side")
        side_fx = _fg("rev-parse", "HEAD")
        _fg("checkout", "-q", "master")
        # 牙的先决夹具本身先自证：分叉形态必须成立（否则这颗牙是假的）
        if _git_run(fx, ["merge-base", "--is-ancestor", side_fx, c1_fx], env).returncode != 1:
            raise GitUnavailable("C1 牙的夹具没分叉 ⇒ 判不了")
        try:
            c1a = _assert_fixture_consistent(fx, env, side_fx, c1_fx, c1_fx)
        except GitUnavailable as ex:
            c1a = "raised:%s" % str(ex)[:80]
        chk(c1a is True, "C1a 分叉夹具（side 非祖先 / c1 祖先）⇒ 守卫放行")
        try:
            _assert_fixture_consistent(fx, env, c1_fx, c1_fx, side_fx)   # side 变成 side_fx 的祖先
            c1b = "no-raise"
        except GitUnavailable as ex:
            msg = str(ex)
            c1b = "ok" if (c1_fx[:8] in msg and side_fx[:8] in msg) else "missing-oids:%s" % msg[:80]
        chk(c1b == "ok", "C1b 夹具自相矛盾 ⇒ GitUnavailable 且消息含 oid（机器守）")

        # C6（reviewer-21）：假 git **返回 rc=2**（并发/慢盘下 merge-base 的真实形态）
        # ⇒ 必须 GitUnavailable；且 selftest 级要 rc=2 且**没有任何 FAIL 行**（假红已消灭）。
        rc2_git = os.path.join(tmp, "fake_git_rc2.py")
        with open(rc2_git, "w", encoding="utf-8") as fh:
            fh.write("import sys" + chr(10) + "sys.stderr.write('fatal: unable to read tree')"
                     + chr(10) + "sys.exit(2)" + chr(10))
        try:
            _fixture_git(tmp, ["merge-base", "--is-ancestor", c1_fx, side_fx], env,
                         git_cmd=(sys.executable, rc2_git))
            c6a = "no-raise"
        except GitUnavailable as ex:
            c6a = "ok" if ("rc=2" in str(ex) or "判不了" in str(ex)) else "wrong:%s" % str(ex)[:60]
        except Exception as ex:  # noqa: BLE001
            c6a = "other:%s" % ex.__class__.__name__
        chk(c6a == "ok", "C6a 假 git 返回 rc=2 ⇒ GitUnavailable（判不了），实际 %r" % (c6a,))
        # F2（review bdd1a28c）：`hexref` 的**确定性**单测——不能靠「跑 5 次都绿」。
        # ① 正常 oid ⇒ 返回值含 a–f 且长度 ≥ want；② 40 位纯数字 ⇒ 判不了；
        # ③ 短且纯数字（不足 want+6）⇒ 判不了（**不许**静默截断成纯数字前缀）。
        _ok_ref = hexref("0123456789abcdef" * 3)
        chk(len(_ok_ref) >= 9 and re.search(r"[a-f]", _ok_ref),
            "F2a hexref(正常 oid) ⇒ 含 a–f 且 ≥9 位（%r）" % (_ok_ref,))
        for label, bad_oid in (("40 位纯数字", "1" * 40), ("短且纯数字", "7" * 12)):
            try:
                hexref(bad_oid)
                _f2 = "no-raise"
            except GitUnavailable as ex:
                _f2 = "ok" if "判不了" in str(ex) else "no-text"
            except Exception as ex:  # noqa: BLE001
                _f2 = "other:%s" % ex.__class__.__name__
            chk(_f2 == "ok", "F2 %s ⇒ hexref 判不了（GitUnavailable），实际 %r" % (label, _f2))
        chk(all(re.search(r"[a-f]", x) for x in (c1_ref, side_ref, blob_ref))
            and len(c1_ref) >= 9 and len(blob_ref) >= 8,
            "规则16 牙：夹具 hex 锚都含 a–f（%s/%s/%s）——否则门按设计静默跳过"
            % (c1_ref, side_ref, blob_ref))
        _saved_run_c6 = globals()["_selftest_run"]

        def _rc2_fixture():
            raise GitUnavailable("夹具 git rc=2（牙注入）⇒ 判不了")

        globals()["_selftest_run"] = _rc2_fixture
        try:
            import contextlib as _cl6
            import io as _io6
            _buf6 = _io6.StringIO()
            with _cl6.redirect_stdout(_buf6):
                c6b_rc = selftest()
            c6b_out = _buf6.getvalue()
        finally:
            globals()["_selftest_run"] = _saved_run_c6
        chk(c6b_rc == 2 and "判不了" in c6b_out
            and not any(l.startswith("FAIL ") for l in c6b_out.splitlines()),
            "C6b rc=2 的夹具失败 ⇒ selftest rc=2 且**无 FAIL 行**（假红已消灭）")
        # R1（卡 openmir2-tool-gate-teeth-residuals）：`hexref` 的**调用点**也要有牙——
        # 把三处 `hexref(...)` 改回 `c1[:9]` 直切（函数留着不删）时，本牙必须红。
        # 做法：装**探针**记录 hexref 的入参；夹具构建在最前 ⇒ 头三次调用必须是三个 40 位 oid
        # （c1 / side / blob）。若调用点被改成切片，头几次记录就变成 F2 牙里的合成字面量（48/40/12 位）⇒ 红。
        if not _DG_PROBE["r1"]:
            _DG_PROBE["r1"] = True
            _real_hexref = globals()["hexref"]
            _seen_r1 = []

            def _spy_hexref(oid, *a, **k):
                _seen_r1.append(str(oid))
                return _real_hexref(oid, *a, **k)

            globals()["hexref"] = _spy_hexref
            try:
                import contextlib as _cl_r1
                import io as _io_r1
                _buf_r1 = _io_r1.StringIO()
                with _cl_r1.redirect_stdout(_buf_r1):
                    selftest()
            finally:
                globals()["hexref"] = _real_hexref
                _DG_PROBE["r1"] = False
            chk(len(_seen_r1) >= 3 and all(len(x) == 40 for x in _seen_r1[:3]),
                "R1 hexref 调用点在夹具链路上（头三次入参须是 40 位 oid；实际 %r）"
                % (_seen_r1[:3],))
        try:
            c4a = _require_branch(fx, env, "master") is True
        except GitUnavailable:
            c4a = False
        _fg("checkout", "-q", "side")
        try:
            _require_branch(fx, env, "master")
            c4b = "no-raise"
        except GitUnavailable as ex:
            c4b = "ok" if "判不了" in str(ex) else "no-text:%s" % str(ex)[:60]
        except AssertionError:
            c4b = "assertion"
        _fg("checkout", "-q", "master")
        chk(c4a, "C4a 在 master ⇒ _require_branch 放行")
        chk(c4b == "ok", "C4b 不在 master ⇒ GitUnavailable（判不了），不是 AssertionError")

        real_mkdtemp = globals()["tempfile"].mkdtemp

        def _no_tmp(*a, **k):
            raise OSError("TEMP 不可写（牙注入）")

        globals()["tempfile"].mkdtemp = _no_tmp
        try:
            import contextlib as _cl2
            import io as _io2
            _buf2 = _io2.StringIO()
            with _cl2.redirect_stdout(_buf2):
                n2c2 = selftest()
            n2c2_out = _buf2.getvalue()
        finally:
            globals()["tempfile"].mkdtemp = real_mkdtemp
        chk(n2c2 == 2 and "判不了" in n2c2_out,
            "N2c2 临时目录建不了（OSError）⇒ selftest rc=2（判不了，不是 rc=1）")

        def run(parts, registry):
            """parts: [(rel, text)] → (findings, rot)。"""
            docs = []
            for rel, text in parts:
                docs.append((rel, text.splitlines()))
            return check(tmp, docs, registry)

        base_reg = {"hex_allow": {"items": []}, "non_ancestor_commits": [],
                    "external_paths": [], "claims": []}

        # ---- A1 hex：每类 ≥2 反例 ----
        f, r = run([("docs/delivery-x.md", "ref `%s`\n" % side_ref)], base_reg)
        chk(any(x["check"] == "hex-not-ancestor" for x in f),
            "HEX-1 未合并 commit ⇒ hex-not-ancestor")
        f, r = run([("docs/delivery-x.md", "ref `%s`\n" % c1_ref)], base_reg)
        chk(not f and not r, "HEX-2 HEAD 祖先 commit ⇒ 绿")
        f, r = run([("docs/delivery-x.md", "entry `%s`\n" % blob_ref)], base_reg)
        chk(any(x["check"] == "hex-non-commit" for x in f),
            "HEX-3 未登记 blob ⇒ hex-non-commit（不静默放行）")
        reg2 = dict(base_reg, hex_allow={"items": [blob_ref]})
        f, r = run([("docs/delivery-x.md", "entry `%s`\n" % blob_ref)], reg2)
        chk(not f and not r, "HEX-4 已登记 blob ⇒ 绿")
        f, r = run([("docs/delivery-x.md", "no refs\n")], reg2)
        chk(any("hex_allow" in x and "未被" in x for x in r), "HEX-5 白名单 rot ⇒ rc=2")
        f, r = run([("docs/delivery-x.md", "ref `deadbeef`\n")], base_reg)
        chk(any(x["check"] == "hex-missing" for x in f), "HEX-6 解析不到的 oid ⇒ hex-missing")
        # 缺失但已登记（本仓真实形态：fold/GC 后的协作条目 b42f20a8）⇒ 绿，且不能变成 rot。
        # 删掉 missing 分支里的 `used_allow.add(oid)` ⇒ 白名单会被判「未被引用」⇒ 本条红。
        f, r = run([("docs/delivery-x.md", "ref `deadbeef`\n")],
                   dict(base_reg, hex_allow={"items": ["deadbeef"]}))
        chk(not f and not r, "HEX-11 缺失但已登记 ⇒ 绿（不再报 hex-missing、也不算白名单 rot）")
        f, r = run([("docs/delivery-x.md", "ref `%s`\n" % side_ref)],
                   dict(base_reg, non_ancestor_commits=[
                       {"oid": side_ref, "file": "docs/delivery-x.md", "reason": "未合并"}]))
        chk(not f and not r, "HEX-7 已登记的未合并 head ⇒ 绿")
        # K1（review 83645e5f）：`[[non_ancestor_commits]]` 的两条 rot 分支各一颗牙。
        # 无牙时「某 head 合入后文档仍写未合并」会被静默放过——正是本门要防的方向。
        f, r = run([("docs/delivery-x.md", "ref `%s`\n" % c1_ref)],
                   dict(base_reg, non_ancestor_commits=[
                       {"oid": c1_ref, "file": "docs/delivery-x.md",
                        "reason": "fixture：登记的其实是祖先"}]))
        chk(any("已是 HEAD 祖先" in x for x in r),
            "HEX-8 登记的「未合并 head」其实已并入 ⇒ rot（文档「未合并」过时）")
        f, r = run([("docs/delivery-x.md", "no refs here\n")],
                   dict(base_reg, non_ancestor_commits=[
                       {"oid": side_ref, "file": "docs/delivery-x.md",
                        "reason": "fixture：登记了但文档不再引用"}]))
        chk(any("未被引用" in x for x in r),
            "HEX-9 登记的未合并 head 未被任何文档引用 ⇒ rot（白名单过期）")
        # N4（review ba964a07）：已并入 HEAD 的登记若同时「未被引用」，**只报可操作的那条**
        # （「已是祖先 ⇒ 更新文档 + 删/刷登记」），不要再报「未被引用」把人引去删登记。
        f, r = run([("docs/delivery-x.md", "no refs here\n")],
                   dict(base_reg, non_ancestor_commits=[
                       {"oid": c1_ref, "file": "docs/delivery-x.md",
                        "reason": "fixture：已并入且文档不再引用"}]))
        chk(any("已是 HEAD 祖先" in x for x in r) and not any("未被引用" in x for x in r),
            "HEX-10 已并入 + 未被引用 ⇒ 只报「已是祖先」（N4 不误导）")

        # ---- A2 表格：fenced code / /* */ / 内联代码 里的 `|` 不算 ----
        good = "| a | b |\n|---|---|\n| 1 | 2 |\n"
        f, _ = run([("docs/delivery-x.md", good)], base_reg)
        chk(not f, "TBL-1 正常表格 ⇒ 绿")
        f, _ = run([("docs/delivery-x.md", "| a | b |\n|---|---|\n| 1 | 2 | 3 |\n")], base_reg)
        chk(any(x["check"] == "table-pipes" for x in f), "TBL-2 多一列 ⇒ table-pipes")
        f, _ = run([("docs/delivery-x.md",
                     "| a | b |\n|---|---|\n| 1 | 2 |\n```\n| x | y | z |\n```\n")], base_reg)
        chk(not f, "TBL-3 fenced code 里的 `|` 不算")
        # 真正的**多行块**（review 83645e5f K2：旧用例通过的原因不对——那行不以 `|` 开始，
        # 且 `/*` 与 `*/` 同行 ⇒ `in_block` 从未置真）。这里块在**表行末尾**打开，块内两行都以
        # `|` 开头且列数不同 ⇒ 消费分支或 `/*` 检测分支任一失效，两行都会成组并判红。
        f, _ = run([("docs/delivery-x.md",
                     "| a | b | /*\n| 1 | 2 | 3 |\n| 4 | 5 |\n*/\n| 9 | 8 |\n")], base_reg)
        chk(not f, "TBL-4 多行 /* */ 块（块在表行末尾打开、块内两行列数不等）整块跳过")
        f, _ = run([("docs/delivery-x.md",
                     "| a | b |\n|---|---|\n| `x | y` | 2 |\n")], base_reg)
        chk(not f, "TBL-5 内联代码里的 `|` 不算")
        f, _ = run([("docs/delivery-x.md",
                     "| a | b |\n|---|---|\n| x \\| y | 2 |\n")], base_reg)
        chk(not f, "TBL-6 单元格里的 `\\|` 是转义，不算分隔符")

        # ---- A3 路径 ----
        f, _ = run([("docs/delivery-x.md", "见 `docs/tracked.md`\n")], base_reg)
        chk(not f, "PATH-1 已跟踪文件 ⇒ 绿")
        f, _ = run([("docs/delivery-x.md", "见 `docs/untracked.md`\n")], base_reg)
        chk(any(x["check"] == "path-untracked" for x in f), "PATH-2 存在但未跟踪 ⇒ path-untracked")
        f, _ = run([("docs/delivery-x.md", "见 `docs/nope.md`\n")], base_reg)
        chk(any(x["check"] == "path-missing" for x in f), "PATH-3 不存在 ⇒ path-missing")
        f, _ = run([("docs/delivery-x.md", "见 `tools/x/`、`tools/x/*.py`\n")], base_reg)
        chk(not f, "PATH-4 目录与 glob 有已跟踪文件 ⇒ 绿")
        reg3 = dict(base_reg, external_paths=[
            {"path": "docs/nope.md", "reason": "外部件"}])
        f, r = run([("docs/delivery-x.md", "见 `docs/nope.md`\n")], reg3)
        chk(not f and not r, "PATH-5 已登记 EXTERNAL ⇒ 绿")
        f, r = run([("docs/delivery-x.md", "见 `docs/tracked.md`\n")], reg3)
        chk(any("external_paths" in x for x in r), "PATH-6 EXTERNAL 登记过期 ⇒ rot")

        # ---- A4 命令 ⇒ 期望值 ----
        # fixture：c1 之后只有 `docs/tracked.md` 变过 ⇒ `-- tools/` 为空、`-- .` 非空。
        claim_doc = "`git diff --name-only %s HEAD -- tools/` **为空**\n" % c1_ref
        reg4 = dict(base_reg, claims=[{
            "id": "x", "file": "docs/delivery-x.md",
            "anchor": "git diff --name-only %s HEAD -- tools/" % c1_ref,
            "kind": "diff_empty", "base": c1_ref, "head": "HEAD", "paths": ["tools/"]}])
        f, r = run([("docs/delivery-x.md", claim_doc)], reg4)
        chk(not f and not r, "CLAIM-1 diff 为空 ⇒ 绿")
        reg5 = dict(base_reg, claims=[{
            "id": "y", "file": "docs/delivery-x.md",
            "anchor": "git diff --name-only %s HEAD -- tools/" % c1_ref,
            "kind": "diff_empty", "base": c1_ref, "head": "HEAD", "paths": ["."]}])
        f, r = run([("docs/delivery-x.md", claim_doc)], reg5)
        chk(any(x["check"] == "claim-stale" for x in f), "CLAIM-2 diff 非空 ⇒ claim-stale")
        reg6 = dict(base_reg, claims=[{
            "id": "z", "file": "docs/delivery-x.md", "anchor": "不存在的 anchor",
            "kind": "diff_empty", "base": c1_ref}])
        f, r = run([("docs/delivery-x.md", claim_doc)], reg6)
        chk(any("找不到 anchor" in x for x in r), "CLAIM-3 anchor 丢了 ⇒ rot")
        reg7 = dict(base_reg, claims=[{
            "id": "w", "file": "docs/delivery-x.md",
            "anchor": "python -c print", "kind": "command_output",
            "argv": [sys.executable, "-c", "print('场景数 24（blocked 2 ⇒ 入批 22）')"],
            "expect": "场景数 24（blocked 2 ⇒ 入批 22）"}])
        f, r = run([("docs/delivery-x.md", "python -c print\n")], reg7)
        chk(not f and not r, "CLAIM-4 command_output 命中 ⇒ 绿")
        reg8 = dict(base_reg, claims=[{
            "id": "v", "file": "docs/delivery-x.md",
            "anchor": "python -c print", "kind": "command_output",
            "argv": [sys.executable, "-c", "print('场景数 25')"],
            "expect": "场景数 24（blocked 2 ⇒ 入批 22）"}])
        f, r = run([("docs/delivery-x.md", "python -c print\n")], reg8)
        chk(any(x["check"] == "claim-stale" for x in f), "CLAIM-5 command_output 不符 ⇒ claim-stale")
        reg9 = dict(base_reg, claims=[{
            "id": "u", "file": "docs/delivery-x.md", "anchor": "print",
            "kind": "shell_pipe", "base": c1_ref}])
        f, r = run([("docs/delivery-x.md", "print\n")], reg9)
        chk(any("未知 kind" in x for x in r), "CLAIM-6 不支持的形态 ⇒ rot（不假装覆盖）")
        # N2（review ba964a07）：argv 白名单 + 禁 `bash -c`（防登记表变成任意命令入口）。
        reg10 = dict(base_reg, claims=[{
            "id": "evil", "file": "docs/delivery-x.md", "anchor": "print",
            "kind": "command_output", "argv": ["cmd.exe", "/c", "echo 1"], "expect": "1"}])
        f, r = run([("docs/delivery-x.md", "print\n")], reg10)
        chk(any("不在白名单" in x for x in r), "CLAIM-7 argv[0] 不在白名单 ⇒ rot（拒绝执行）")
        reg11 = dict(base_reg, claims=[{
            "id": "shellc", "file": "docs/delivery-x.md", "anchor": "print",
            "kind": "command_output", "argv": ["bash", "-c", "echo 1"], "expect": "1"}])
        f, r = run([("docs/delivery-x.md", "print\n")], reg11)
        chk(any("bash -c" in x for x in r), "CLAIM-8 `bash -c` ⇒ rot（绕过白名单）")
        # N1 同型：claim 的 git 命令自己失败（比如 base 写错）⇒ **判不了**（rot），不是「断言过时」。
        reg12 = dict(base_reg, claims=[{
            "id": "badbase", "file": "docs/delivery-x.md",
            "anchor": "git diff --name-only deadbeef HEAD", "kind": "diff_empty",
            "base": "deadbeef", "head": "HEAD"}])
        f, r = run([("docs/delivery-x.md", "git diff --name-only deadbeef HEAD\n")], reg12)
        chk(any("跑失败" in x for x in r) and not f,
            "CLAIM-9 claim 的 git 跑失败 ⇒ rot（判不了），不产出 findings")
        chk(len(head) == 40, "fixture：HEAD 是完整 oid")

        # ---- N1（review ba964a07）：git 自己失败 ⇒ **判不了**（rc=2），不是 findings（rc=1） ----
        # 三条**互不替代**的牙：cat-file 失败 / merge-base rc=2 / 整体 git 挂。
        import contextlib
        import io
        real_git = globals()["git"]
        doc_ref = [("docs/delivery-main.md", ["ref `%s`" % c1_ref])]

        def run_with(stub):
            globals()["git"] = stub
            try:
                try:
                    check(tmp, doc_ref, base_reg)
                    return None
                except GateError as ex:
                    return str(ex)
            finally:
                globals()["git"] = real_git

        def cat_file_broken(repo, *args):
            if args and args[0] == "cat-file":
                return subprocess.CompletedProcess(list(args), 128, "",
                                                   "fatal: unable to read tree (corrupt)")
            return real_git(repo, *args)

        err = run_with(cat_file_broken)
        chk(err is not None and "判不了" in err,
            "N1a `cat-file` 非「对象不存在」失败 ⇒ 判不了（不降级成野 oid）")

        def merge_base_broken(repo, *args):
            if args and args[0] == "merge-base":
                return subprocess.CompletedProcess(list(args), 2, "", "fatal: bad revision")
            return real_git(repo, *args)

        err = run_with(merge_base_broken)
        chk(err is not None and "判不了" in err,
            "N1b `merge-base --is-ancestor` rc=2 ⇒ 判不了（不是「不是祖先」）")

        def all_broken(repo, *args):
            return subprocess.CompletedProcess(list(args), 128, "",
                                               "fatal: not a git repository (or any parent)")

        regfile = os.path.join(tmp, "reg-main.toml")
        with open(regfile, "w", encoding="utf-8") as fh:
            fh.write('[domain]\nglobs = ["docs/delivery-main.md"]\n'
                     '[hex_allow]\nitems = []\n')
        with open(os.path.join(tmp, "docs", "delivery-main.md"), "w", encoding="utf-8") as fh:
            fh.write("ref `%s`\n" % c1_ref)
        globals()["git"] = all_broken
        try:
            buf = io.StringIO()
            # stderr 也收进 buf：否则这条**故意注入**的「判不了」会变成工具输出的最后一行，
            # 在 `tool_selftest_gate` 里看起来像「这个工具红了」（卡 openmir2-docs-gate-rc-semantics
            # 观察到的误读形态）。
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
                rc_n1 = main(["--repo", tmp, "--registry", regfile])
            chk(rc_n1 == 2 and "RED " not in buf.getvalue(),
                "N1c main()：git 全挂 ⇒ rc=2 且**没有** RED 行（不把判不了说成文档错）")
            chk("判不了" in buf.getvalue(),
                "N1c2 该路径的输出**明确说**「判不了」（便于人/门区分 rc=2 与 rc=1）")
        finally:
            globals()["git"] = real_git

        # ---- rc 语义：1 = 证伪 / 0 = 绿（配合 N1 的 2 = 判不了） ----
        with open(os.path.join(tmp, "docs", "delivery-main.md"), "w", encoding="utf-8") as fh:
            fh.write("| a | b |\n|---|---|\n| 1 | 2 | 3 |\n")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc_bad = main(["--repo", tmp, "--registry", regfile])
        chk(rc_bad == 1 and "RED   table-pipes" in buf.getvalue(),
            "RC-1 有 findings ⇒ rc=1（证伪）")
        with open(os.path.join(tmp, "docs", "delivery-main.md"), "w", encoding="utf-8") as fh:
            fh.write("nothing here\n")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc_ok = main(["--repo", tmp, "--registry", regfile])
        chk(rc_ok == 0 and "GREEN" in buf.getvalue(), "RC-2 干净 ⇒ rc=0（绿）")
    finally:
        import shutil
        shutil.rmtree(tmp, ignore_errors=True)

    if fails:
        print("docs_delivery_gate selftest: FAIL (%d/%d)" % (len(fails), checked[0]))
        for n in fails:
            print("  - " + n)
        return 1
    print("docs_delivery_gate selftest: OK (%d checks)" % checked[0])
    return 0


if __name__ == "__main__":
    sys.exit(main())
