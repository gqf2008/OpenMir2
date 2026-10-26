#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""wallclock_scan — 清点「断言某耗时/延迟 < 阈值」的自测断言（卡 `openmir2-wallclock-upperbound-scan`）。

**这是一张清点工具，不是判据**：它对仓库**只读**，不改任何阈值、不判红、不进工具门
的 REGISTRY（本文件刻意不提供自测入口；工具门只清点带自测入口的文件）。

为什么需要：**墙钟上界**（`elapsed < T`）在负载下一旦被调度停顿顶穿，绿会变 NEED
（工具门 rc=2 ⇒ 整门 rc=3 ⇒ `goal_gate` 条件④ 报「需人工」）——开窗当天值守会去查一个不存在的缺陷。
`mir2_probe` 的 dwell 反例已经踩过一次（卡号见清单文档）。

分类（只分三类，不判红）：
  * **上界** `x < T` / `x <= T` / `-lt` / `-le` / `.lt(` —— 负载下**更容易失败** ⇒ 建议换可观测同步点；
  * **下界** `x > T` / `x >= T` / `-gt` / `-ge` / `.gt(` —— 负载下**更容易通过**，安全；
  * **等超时** —— 断言里出现 `timed out` / `Timeout` / `TIMEOUT` 等标记（本来就要等满）。

用法：
  python -I tools/hygiene/wallclock_scan.py [--repo .] [--json out.json] [--context N]
退出码：0 = 扫描完成（**无论有没有命中**；本工具不判红）；2 = 用法/环境错误。
"""
import argparse
import io
import json
import os
import re
import sys

#: 「像耗时」的名字（赋给变量后再比，也要能认出来）
DUR_NAME_RE = re.compile(
    r"(elapsed|duration|delta|took|delay|actual_ms|waited|first_t|_ms$|^ms_|_dt$|"
    r"lap|span|cost)", re.I)

#: 把「耗时表达式」赋给变量的行：`x = <expr>`，其中 expr 含时间函数差或名字像耗时
ASSIGN_RE = re.compile(r"^\s*(?:let\s+|const\s+|var\s+|\$)?([A-Za-z_][\w.]*)\s*[:=]\s*(.+)$")

#: 时间函数（Python / Rust / PowerShell 常见写法）
TIME_FN = ("time.time()", "time.monotonic()", "time.perf_counter()", "now_ms()",
           "Instant::now()", "System.Diagnostics.Stopwatch", "Get-Date",
           ".elapsed", "elapsed_ms", "now()")

CMP_RE = re.compile(
    r"(?P<lhs>[^<>!=]{1,120}?)\s*(?P<op><=|>=|==|!=|<|>)\s*(?P<rhs>[^\s,;)]{1,40})")
PS_CMP_RE = re.compile(r"(?P<lhs>[^;]{1,120}?)\s+-(?P<op>lt|le|gt|ge|eq|ne)\s+(?P<rhs>[^\s;)]{1,40})",
                       re.I)
TO_TIMEOUT_RE = re.compile(r"(timed? ?out|timeout|TIMEOUT|超时)", re.I)

SKIP_DIRS = {"__pycache__", ".git", "node_modules", "target", "bin", "obj"}
SUFFIXES = (".py", ".ps1", ".rs")


def iter_files(root):
    for base, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for fn in files:
            if fn.endswith(SUFFIXES):
                yield os.path.join(base, fn)


def time_names(lines):
    """本文件里「时间点或耗时」的变量名。

    - **时间点**：赋值表达式含时间函数（`time.time()`、`Get-Date`、`Instant::now()` …），
      或名字本身就像耗时（`elapsed` / `*_ms` / `waited` / `first_t` / `actual_ms` …）；
    - **耗时**：赋值是「两个时间点相减」。

    只把**时间点相减**当耗时：`$end - $start` 里只要有一侧不是时间点，就不是墙钟比较
    （`mir_flow.ps1:493` 的 `($end - $start) -lt 16` 是**字节下标**，曾误报）。
    """
    ts = set()
    for raw in lines:
        m = ASSIGN_RE.match(raw)
        if not m:
            continue
        name, expr = m.group(1).split(".")[-1], m.group(2)
        if any(fn in expr for fn in TIME_FN) or DUR_NAME_RE.search(name):
            ts.add(name)
    return ts


def _tokens(text):
    return [t.split(".")[-1] for t in re.findall(r"[A-Za-z_][\w.]*", text)]


def looks_duration(text, ts):
    if any(fn in text for fn in TIME_FN):
        return True
    # 有减法时**只看减法两侧**：两侧都是时间点才算耗时。
    # （否则 `if (($end - $start) -lt 16)` 里 `$end` 是时间点、`$start` 是字节下标，会被误报。）
    m = re.search(r"([$A-Za-z_][\w.$]*)\s*-\s*([$A-Za-z_][\w.$]*)", text)
    if m:
        a = m.group(1).split(".")[-1].lstrip("$")
        b = m.group(2).split(".")[-1].lstrip("$")
        return a in ts and b in ts
    # 兜底：任何 token 只要「名字像耗时」就算（`res5["elapsed_ms"] < 1200` 这类下标形态，
    # 它的名字不是赋值出来的，靠 ts 认不出来）。
    return any(tok.lstrip("$") in ts or DUR_NAME_RE.search(tok) for tok in _tokens(text))


def classify(op, line):
    if TO_TIMEOUT_RE.search(line):
        return "等超时"
    op = op.lower()
    if op in ("<", "<=", "lt", "le"):
        return "上界"
    if op in (">", ">=", "gt", "ge"):
        return "下界"
    return None


def scan(repo, context=0, extra_roots=()):
    hits = []
    roots = [os.path.join(repo, r) for r in ("tools", "mir2-rs/tests")] + list(extra_roots)
    for root in roots:
        if not os.path.isdir(root):
            continue
        for path in iter_files(root):
            try:
                text = io.open(path, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            lines = text.splitlines()
            ts = time_names(lines)
            for i, raw in enumerate(lines, 1):
                s = raw.strip()
                if not s or s.startswith(("#", "//", "/*", "*")):
                    continue
                # 下标里的**字符串键**先还原成裸 token（`res5["elapsed_ms"]` ⇒ `res5[elapsed_ms]`），
                # 否则下一步剥字符串时会把「像耗时的键名」一起剥掉。
                code = re.sub(r'\[\s*"([^"]+)"\s*\]', r"[\1]", s)
                # 其余字符串字面量剥掉：`"elapsed=%dms，上界 %dms"` 这类**消息**不是断言。
                code = re.sub(r'"[^"]*"', '""', code)
                code = re.sub(r"'[^']*'", "''", code)
                # 行尾注释不是断言（`w.clock_ms = …; // 60201: > 60000` 曾误报）
                code = re.split(r"\s//\s|\s#\s", code, maxsplit=1)[0]
                m = CMP_RE.search(code) or PS_CMP_RE.search(code)
                if not m:
                    continue
                lhs, op, rhs = m.group("lhs"), m.group("op"), m.group("rhs")
                if op in ("==", "!=", "eq", "ne"):
                    continue
                if not looks_duration(lhs.lstrip("$ "), set(t.lstrip("$") for t in ts)):
                    continue
                rhs = rhs.strip().rstrip(":")     # Python 的 `if x < 10:` 会把冒号带进来
                if not (re.match(r"^[\d.]+\s*[a-zA-Z_]*$", rhs)
                        or re.match(r"^Duration::from_(millis|micros|secs)\(\d+\)$", rhs)):
                    continue        # 阈值必须是数字/时长常量
                if re.match(r"^0*(\.0+)?\s*[a-zA-Z_]*$", rhs):
                    continue        # `> 0` / `>= 0` 是存在性检查，不是耗时判据
                kind = classify(op, s)
                if kind is None:
                    continue
                hits.append({
                    "file": os.path.relpath(path, repo).replace(os.sep, "/"),
                    "line": i, "op": op, "threshold": rhs.strip(), "kind": kind,
                    "text": s[:200],
                    "context": [l.rstrip() for l in lines[max(0, i - 1 - context): i + context]],
                })
    hits.sort(key=lambda h: (h["kind"], h["file"], h["line"]))
    return hits


def main(argv=None):
    ap = argparse.ArgumentParser(description="清点「耗时 < 阈值」型自测断言（只读，不判红）")
    ap.add_argument("--repo", default=".")
    ap.add_argument("--json")
    ap.add_argument("--context", type=int, default=0, help="每条附带前后 N 行")
    ap.add_argument("--extra-root", action="append", default=[],
                    help="额外扫描根（相对 --repo）；默认 tools/ 与 mir2-rs/tests/")
    args = ap.parse_args(argv)
    repo = os.path.abspath(args.repo)
    if not os.path.isdir(repo):
        print("wallclock_scan: --repo 不是目录：%s" % repo, file=sys.stderr)
        return 2
    hits = scan(repo, context=max(0, args.context),
                extra_roots=[r for r in args.extra_root])
    order = ("上界", "下界", "等超时")
    counts = {k: sum(1 for h in hits if h["kind"] == k) for k in order}
    for kind in order:
        group = [h for h in hits if h["kind"] == kind]
        print("\n== %s（%d 条）==" % (kind, len(group)))
        for h in group:
            print("%-58s:%-5d %-3s %-6s %s"
                  % (h["file"], h["line"], h["op"], h["threshold"], h["text"]))
            for c in h["context"]:
                print("      | " + c)
    print("\nTOTAL %d 条：上界 %d / 下界 %d / 等超时 %d（**只清点，不判红**）"
          % (len(hits), counts["上界"], counts["下界"], counts["等超时"]))
    if args.json:
        with io.open(args.json, "w", encoding="utf-8", newline="") as fh:
            fh.write(json.dumps({
                "repo": repo,
                "counts": counts,
                "total": len(hits),
                "hits": hits,
            }, ensure_ascii=False, indent=1) + chr(10))
        print("JSON: %s" % args.json)
    return 0


if __name__ == "__main__":
    sys.exit(main())
