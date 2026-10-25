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
  * 任意 shell 管道 / 重定向（除 `diff_empty` 的 `name_regex` 形态）；多命令串联。
  以上在登记表 `[unsupported_claims]` 里逐条写明，且**允许为空只是暂时的**——新增 claim 若不属于
  两种受支持形态，必须落进该表（本门会打印，不判红）。

**登记表 rot 检测**（防止白名单变成垃圾桶）
  * `[hex_allow]` 里的 oid 必须**仍被引用**且**仍是非 commit**；否则 rc=2；
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

HEX_RE = re.compile(r"`([0-9a-f]{7,40})`")
HEX_HAS_LETTER_RE = re.compile(r"[a-f]")
PATH_RE = re.compile(r"(?<![\w/])((?:docs|tools|mir2-rs|src)/[0-9A-Za-z_.\-/*]*)")
INLINE_CODE_RE = re.compile(r"`[^`\n]*`")
PATH_TRAIL = ".,;:!?)]}>'\"`，。；：！？）】》、"


class GateError(Exception):
    """用法错 / 登记表问题（rc=2）。"""


# ---------------------------------------------------------------------------
# 外部命令解析（Windows 上裸 `bash` 是 System32 的 WSL stub ⇒ rc=1，见 card_window 同口径）
# ---------------------------------------------------------------------------
_BASH_CANDIDATES = (
    r"C:\Program Files\Git\usr\bin\bash.exe",
    r"C:\Program Files (x86)\Git\usr\bin\bash.exe",
    os.path.expandvars(r"%LocalAppData%\Programs\Git\usr\bin\bash.exe"),
)


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
    return subprocess.run(["git"] + list(args), cwd=repo, capture_output=True,
                          text=True, encoding="utf-8", errors="replace")


class GitCache:
    """`cat-file -t` / `merge-base --is-ancestor` 的记忆化（同一 oid 在文档里出现很多次）。"""

    def __init__(self, repo):
        self.repo = repo
        self._kind = {}
        self._anc = {}

    def kind(self, oid):
        if oid not in self._kind:
            r = git(self.repo, "cat-file", "-t", oid)
            self._kind[oid] = r.stdout.strip() if r.returncode == 0 else None
        return self._kind[oid]

    def is_ancestor(self, oid, ref="HEAD"):
        key = (oid, ref)
        if key not in self._anc:
            self._anc[key] = git(self.repo,
                                 "merge-base", "--is-ancestor", oid, ref).returncode == 0
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
    for (oid, path) in sorted(non_anc - used_non_anc):
        rot.append("non_ancestor_commits 里的 `%s`@%s 未被引用（白名单过期）" % (oid, path))
    for e in registry.get("non_ancestor_commits", []):
        if cache.kind(e["oid"]) == "commit" and cache.is_ancestor(e["oid"]):
            rot.append("non_ancestor_commits 里的 `%s`@%s 已是 HEAD 祖先 ⇒ 文档里的「未合并」过时了"
                       % (e["oid"], e["file"]))
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
            if os.path.basename(argv[0]).lower() in ("bash", "bash.exe") and not os.path.dirname(argv[0]):
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
    cache = GitCache(repo)
    tracked = set(git(repo, "ls-files").stdout.splitlines())
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
        env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t",
                   GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@t")

        def g(*a):
            r = subprocess.run(["git"] + list(a), cwd=tmp, capture_output=True, text=True,
                               encoding="utf-8", errors="replace", env=env)
            if r.returncode != 0:
                raise AssertionError("git %s 失败: %s" % (" ".join(a), r.stderr))
            return r.stdout.strip()

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
        # HEAD 上再提交一次（只改 docs/tools ⇒ 便于 diff_empty 用例）
        with open(os.path.join(tmp, "docs", "tracked.md"), "w", encoding="utf-8") as fh:
            fh.write("base2\n")
        g("commit", "-q", "-am", "c3")
        head = g("rev-parse", "HEAD")
        p = subprocess.run(["git", "hash-object", "-w", "--stdin"], cwd=tmp, input="x\n",
                           capture_output=True, text=True, encoding="utf-8", env=env)
        blob = p.stdout.strip()
        # 未跟踪文件
        with open(os.path.join(tmp, "docs", "untracked.md"), "w", encoding="utf-8") as fh:
            fh.write("nope\n")

        def run(parts, registry):
            """parts: [(rel, text)] → (findings, rot)。"""
            docs = []
            for rel, text in parts:
                docs.append((rel, text.splitlines()))
            return check(tmp, docs, registry)

        base_reg = {"hex_allow": {"items": []}, "non_ancestor_commits": [],
                    "external_paths": [], "claims": []}

        # ---- A1 hex：每类 ≥2 反例 ----
        f, r = run([("docs/delivery-x.md", "ref `%s`\n" % side[:9])], base_reg)
        chk(any(x["check"] == "hex-not-ancestor" for x in f),
            "HEX-1 未合并 commit ⇒ hex-not-ancestor")
        f, r = run([("docs/delivery-x.md", "ref `%s`\n" % c1[:9])], base_reg)
        chk(not f and not r, "HEX-2 HEAD 祖先 commit ⇒ 绿")
        f, r = run([("docs/delivery-x.md", "entry `%s`\n" % blob[:8])], base_reg)
        chk(any(x["check"] == "hex-non-commit" for x in f),
            "HEX-3 未登记 blob ⇒ hex-non-commit（不静默放行）")
        reg2 = dict(base_reg, hex_allow={"items": [blob[:8]]})
        f, r = run([("docs/delivery-x.md", "entry `%s`\n" % blob[:8])], reg2)
        chk(not f and not r, "HEX-4 已登记 blob ⇒ 绿")
        f, r = run([("docs/delivery-x.md", "no refs\n")], reg2)
        chk(any("hex_allow" in x and "未被" in x for x in r), "HEX-5 白名单 rot ⇒ rc=2")
        f, r = run([("docs/delivery-x.md", "ref `deadbeef`\n")], base_reg)
        chk(any(x["check"] == "hex-missing" for x in f), "HEX-6 解析不到的 oid ⇒ hex-missing")
        f, r = run([("docs/delivery-x.md", "ref `%s`\n" % side[:9])],
                   dict(base_reg, non_ancestor_commits=[
                       {"oid": side[:9], "file": "docs/delivery-x.md", "reason": "未合并"}]))
        chk(not f and not r, "HEX-7 已登记的未合并 head ⇒ 绿")

        # ---- A2 表格：fenced code / /* */ / 内联代码 里的 `|` 不算 ----
        good = "| a | b |\n|---|---|\n| 1 | 2 |\n"
        f, _ = run([("docs/delivery-x.md", good)], base_reg)
        chk(not f, "TBL-1 正常表格 ⇒ 绿")
        f, _ = run([("docs/delivery-x.md", "| a | b |\n|---|---|\n| 1 | 2 | 3 |\n")], base_reg)
        chk(any(x["check"] == "table-pipes" for x in f), "TBL-2 多一列 ⇒ table-pipes")
        f, _ = run([("docs/delivery-x.md",
                     "| a | b |\n|---|---|\n| 1 | 2 |\n```\n| x | y | z |\n```\n")], base_reg)
        chk(not f, "TBL-3 fenced code 里的 `|` 不算")
        f, _ = run([("docs/delivery-x.md",
                     "| a | b |\n|---|---|\n/* | x | y | z | */\n| 1 | 2 |\n")], base_reg)
        chk(not f, "TBL-4 /* */ 包住的表格行跳过")
        f, _ = run([("docs/delivery-x.md",
                     "| a | b |\n|---|---|\n| `x | y` | 2 |\n")], base_reg)
        chk(not f, "TBL-5 内联代码里的 `|` 不算")

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
        claim_doc = "`git diff --name-only %s HEAD -- tools/` **为空**\n" % c1[:9]
        reg4 = dict(base_reg, claims=[{
            "id": "x", "file": "docs/delivery-x.md",
            "anchor": "git diff --name-only %s HEAD -- tools/" % c1[:9],
            "kind": "diff_empty", "base": c1[:9], "head": "HEAD", "paths": ["tools/"]}])
        f, r = run([("docs/delivery-x.md", claim_doc)], reg4)
        chk(not f and not r, "CLAIM-1 diff 为空 ⇒ 绿")
        reg5 = dict(base_reg, claims=[{
            "id": "y", "file": "docs/delivery-x.md",
            "anchor": "git diff --name-only %s HEAD -- tools/" % c1[:9],
            "kind": "diff_empty", "base": c1[:9], "head": "HEAD", "paths": ["."]}])
        f, r = run([("docs/delivery-x.md", claim_doc)], reg5)
        chk(any(x["check"] == "claim-stale" for x in f), "CLAIM-2 diff 非空 ⇒ claim-stale")
        reg6 = dict(base_reg, claims=[{
            "id": "z", "file": "docs/delivery-x.md", "anchor": "不存在的 anchor",
            "kind": "diff_empty", "base": c1[:9]}])
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
            "kind": "shell_pipe", "base": c1[:9]}])
        f, r = run([("docs/delivery-x.md", "print\n")], reg9)
        chk(any("未知 kind" in x for x in r), "CLAIM-6 不支持的形态 ⇒ rot（不假装覆盖）")
        chk(len(head) == 40, "fixture：HEAD 是完整 oid")
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
