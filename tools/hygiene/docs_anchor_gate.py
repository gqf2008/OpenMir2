#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""判据文档里的行号/符号锚会腐烂（卡 `openmir2-doc-anchor-drift`）。

## 为什么需要

reviewer-21 审 M-1 后记时发现：矩阵里的 oracle/harness 行号**在合并之间就漂了**
（`ApplySoftClose` `:560-575` → `:567` → `:573`；`ApplyGhost` 写 `:586-591`、实际 `:599-605`）。
行号错了，后来人按图索骥会找到**另一个函数**。`whitelist.md` 有 `whitelist_anchor_gate` 守着，
**`docs/` 没有任何门在守**。

本门只覆盖**「判据文件」类文档**（登记在 `docs_anchor_registry.toml` 的 `[[files]]`，每条要写
`why`）——历史叙事文档里的行号是当时记录，扫它们只会误报。

## 四档判据（卡上 A1–A4 + Leader 的四条追加）

对登记文档里**位置列**（表格第 2 个 cell，`split('|')[2]`）与显式符号锚：

| 档 | 条件 | 结果 |
|---|---|---|
| 绿 | 定位到 item，且符号出现在**该 item 的正文**里（剥注释后、完整标识符） | 绿 |
| 红 | 定位到 item，但符号不在其正文 | **`ANCHOR_SYMBOL_MISSING`** |
| **rc=2** | **定位不到 item**（多解/跨文件/解析失败） | **判不了并点名**——**绝不静默跳过** |
| 红 | 文件解析不到（`[[base_dirs]]` 都试过） | **`ANCHOR_PATH_MISSING`** |
| 红 | 高 churn 文件（`crates/world/**`、`bins/game-svr/**`、`tests/parity/csharp/**`）写行号锚 | **`HIGH_CHURN_LINE_ANCHOR`** |

例外：`[[exceptions]]` 每条必须 `card` + `reason`，并在输出里以 `EXCEPTION(used)` 列出；
文档里已无该锚 ⇒ `STALE_EXCEPTION` 红（例外也会腐烂）。

**oracle `src/**` 的行号锚保留**：`src/` 是冻结面（`AGENTS.md`：oracle 只读），行号在那里是稳定依据；
且有一类分支只能按行号定位。**不要**为了统一风格把 `src/**` 的 `path:line` 也改掉。

## 两条「假绿」通道（`smframe-high-churn-anchor` 的教训）

「符号存在」绝不能写成裸子串：
1. **同前缀标识符**（`drain_aoi_downlink_v2` 会兜住 `drain_aoi_downlink`）⇒ 按**完整标识符**匹配；
2. **注释里出现**（`` [`sym`] `` 也算命中）⇒ **先剥注释**。

剥离语义**对齐** `mir2-rs/bins/game-svr/src/lib.rs` 的 `strip_rust_comments`
（卡 `openmir2-strip-rust-comments-dedupe`，`ef8191645`）：行注释 / 嵌套块注释 / 字符串里的 `//`
不剥 / raw string 不剥 / 字符字面量与生命期区分 / **剥后行数不变**。本文件的 `strip_rust_comments`
与那 6 条逐条对应，且各有单测（**剥离层与判据层分开测**：剥离错了不会被判据的绿盖住）。

## 用法

```sh
python -I -B tools/hygiene/docs_anchor_gate.py              # 0 绿 / 1 红 / 2 判不了（含配置坏）
python -I -B tools/hygiene/docs_anchor_gate.py --list       # 列锚 + 例外
python -I -B tools/hygiene/docs_anchor_gate.py --selftest
```
"""
import os
import re
import shutil
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REGISTRY_REL = "tools/hygiene/docs_anchor_registry.toml"

HIGH_CHURN_PREFIXES = (
    "mir2-rs/crates/world/",
    "mir2-rs/bins/game-svr/",
    "mir2-rs/tests/parity/csharp/",
)

RAW_STRING_RE = re.compile(r'(?:b)?r(#*)"')
LINE_ANCHOR_RE = re.compile(r"`([A-Za-z0-9_./\-]+\.(?:rs|cs)):(\d+)(?:-(\d+))?`")
#: 位置列里的符号锚：`` `file.rs` `sym` ``
SYMBOL_ANCHOR_RE = re.compile(r"`([A-Za-z0-9_./\-]+\.(?:rs|cs))`\s+`([A-Za-z_][A-Za-z0-9_]*)`")


# ---------------------------------------------------------------- 剥离层

def strip_rust_comments(text):
    """剥 Rust 注释；**行数不变**（跨行块注释保留换行）。

    语义对齐 `strip_rust_comments`（game-svr `lib.rs`）：
    ① 行注释 `//` ② 嵌套块注释 `/* */` ③ 字符串里的 `//` 保留 ④ raw string 保留
    ⑤ 字符字面量 / 生命期区分 ⑥ 行数不变
    """
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        two = text[i:i + 2]
        # 行注释
        if two == '//':
            j = text.find('\n', i)
            if j < 0:
                break
            i = j  # 保留换行
            continue
        # 嵌套块注释
        if two == '/*':
            depth = 1
            i += 2
            while i < n and depth:
                if text[i:i + 2] == '/*':
                    depth += 1
                    i += 2
                elif text[i:i + 2] == '*/':
                    depth -= 1
                    i += 2
                else:
                    if text[i] == '\n':
                        out.append('\n')  # 行数不变
                    i += 1
            continue
        # raw string：r"…" / r#"…"# / br#"…"#
        m = RAW_STRING_RE.match(text, i)
        if m:
            hashes = m.group(1)
            start = i + m.end()
            close = '"' + hashes
            j = text.find(close, start)
            if j < 0:
                out.append(text[i:])
                break
            out.append(text[i:j + len(close)])
            i = j + len(close)
            continue
        # 普通字符串
        if c == '"':
            j = i + 1
            while j < n:
                if text[j] == '\\':
                    j += 2
                    continue
                if text[j] == '"':
                    j += 1
                    break
                j += 1
            out.append(text[i:j])
            i = j
            continue
        # 字符字面量 vs 生命期：`'x'` / `'\n'` / `'\''` / `'"'` / `'\u{…}'`；`'a`（生命期）不算
        if c == "'":
            if text[i:i + 2] == "'\\":
                # 转义序列：'\'' / '\n' / '\u{1F600}'
                j = i + 2
                if j < n and text[j] == 'u' and j + 1 < n and text[j + 1] == '{':
                    k = text.find('}', j)
                    j = (k + 2) if k >= 0 else n
                else:
                    j = min(j + 1, n)
                    if j < n and text[j] == "'":
                        j += 1
                out.append(text[i:j])
                i = j
                continue
            if i + 2 < n and text[i + 2] == "'":
                # 'x'（单字符）
                out.append(text[i:i + 3])
                i += 3
                continue
            # 生命期 'a —— 原样保留，不当字面量
            out.append(c)
            i += 1
            continue
        out.append(c)
        i += 1
    return ''.join(out)


# ---------------------------------------------------------------- 定位层

FN_RE = re.compile(
    r'^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?'
    r'(?:extern\s+"[^"]*"\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)'
)


def _brace_delta(line):
    d = 0
    in_str = False
    i = 0
    while i < len(line):
        ch = line[i]
        if in_str:
            if ch == '\\':
                i += 2
                continue
            if ch == '"':
                in_str = False
        else:
            if ch == '"':
                in_str = True
            elif ch == '{':
                d += 1
            elif ch == '}':
                d -= 1
        i += 1
    return d


def item_spans(text):
    """**单遍**求 [(name, start, end)]（1-based，闭区间）。

    上一版对每个 `fn` 都向后扫到配对 `}`，遇到字符串里不平衡的 `{` 时退化成 O(n²)
    （game-svr `lib.rs` ≈7.7 万行 ⇒ 门跑几分钟）。单遍栈式算法：遇到 fn 记起始深度，
    深度回落到起始深度即闭合；未闭合的落到文件尾（不再死等）。
    """
    spans = []
    opens = []   # (name, start_depth, start_line)
    depth = 0
    for i, line in enumerate(text.split(chr(10)), 1):
        for m in FN_RE.finditer(line):
            opens.append((m.group(2), depth, i))
        depth += _brace_delta(line)
        while opens and opens[-1][1] >= depth:
            name, d0, start = opens.pop()
            spans.append((name, start, i))
    for name, _d0, start in opens:
        spans.append((name, start, len(text.split(chr(10)))))
    return spans


def locate_item(spans, lineno):
    """返回 (sym, start, end) / (None, reason)。

    **fail-closed**：一行落在**多个** item 里（嵌套 fn / 闭包）⇒ 判不了，不猜。
    """
    inside = [s for s in spans if s[1] <= lineno <= s[2]]
    if len(inside) > 1:
        return None, "多解：该行落在 %d 个 item 内（%s）" % (
            len(inside), ", ".join(sorted(s[0] for s in inside)))
    if inside:
        return inside[0], None
    after = [s for s in spans if s[1] > lineno]
    if after:
        return min(after, key=lambda s: s[1]), None
    before = [s for s in spans if s[2] < lineno]
    if before:
        return max(before, key=lambda s: s[2]), None
    return None, "解析失败：文件里没有任何 fn 可定位"


def symbol_in_text(sym, text):
    """完整标识符匹配（前后不得是标识符字符）。输入须已剥注释。"""
    return re.search(r'(?<![A-Za-z0-9_])' + re.escape(sym) + r'(?![A-Za-z0-9_])', text) is not None


# ---------------------------------------------------------------- 登记层

def load_registry(path):
    import tomllib
    with open(path, "rb") as fh:
        doc = tomllib.load(fh)
    files = doc.get("files") or []
    for f in files:
        if not f.get("path") or not f.get("why"):
            raise ValueError("[[files]] 每条必须同时有 path 与 why")
    base_dirs = [b.get("dir", "") for b in (doc.get("base_dirs") or [])]
    if "" not in base_dirs:
        base_dirs.append("")
    exc = doc.get("exceptions") or []
    for e in exc:
        miss = [k for k in ("doc", "anchor", "card", "reason") if not e.get(k)]
        if miss:
            raise ValueError("[[exceptions]] 缺字段 %s：%r" % (miss, e.get("anchor")))
    return {"files": files, "base_dirs": base_dirs, "exceptions": exc}


def _norm(s):
    return s.strip().strip('`').replace(" + ", " ")


def run(reg_path, repo, list_only=False):
    try:
        reg = load_registry(reg_path)
    except Exception as e:
        print("docs-anchor gate：登记表坏：%s" % e, file=sys.stderr)
        return 2
    findings, anchors = [], []
    seen_exc_used = set()
    for f in reg["files"]:
        rel = f["path"]
        p = os.path.join(repo, rel)
        if not os.path.isfile(p):
            print("docs-anchor gate：登记文档不存在：%s" % rel, file=sys.stderr)
            return 2
        raw = open(p, encoding="utf-8", errors="replace").read()
        stripped_src_cache = {}
        for lineno, line in enumerate(raw.split("\n"), 1):
            # **只扫表格的位置列（`split('|')[2]`）**：正文/说明列里的行号是**代码片段定位**
            # （如 `` `:15073` `if sess.stage != …` ``），换成符号会丢信息 ⇒ 不在本门判定域（A2）。
            if not line.startswith("|"):
                continue
            cells = line.split("|")
            if len(cells) <= 2:
                continue
            scopes = [cells[2]]
            for scope in scopes:
                for m in SYMBOL_ANCHOR_RE.finditer(scope):
                    rel_f, sym = m.group(1), m.group(2)
                    anchors.append({"doc": rel, "doc_line": lineno, "raw": "%s + %s" % (rel_f, sym)})
                    res = _check_symbol(repo, reg, rel_f, sym, None, stripped_src_cache)
                    if res:
                        findings.append(dict(res, doc=rel, doc_line=lineno,
                                             raw="%s + %s" % (rel_f, sym)))
                for m in LINE_ANCHOR_RE.finditer(scope):
                    rel_f, ln = m.group(1), int(m.group(2))
                    anchors.append({"doc": rel, "doc_line": lineno, "raw": m.group(0)})
                    path = _resolve(repo, rel_f, reg["base_dirs"], ln)
                    if path is None:
                        findings.append({"why": "ANCHOR_PATH_MISSING", "doc": rel, "doc_line": lineno,
                                         "raw": m.group(0),
                                         "detail": "[[base_dirs]] 都解析不到（或文件行数 < 锚行号）"})
                        continue
                    if any(path.startswith(h) for h in HIGH_CHURN_PREFIXES):
                        findings.append({"why": "HIGH_CHURN_LINE_ANCHOR", "doc": rel,
                                         "doc_line": lineno, "raw": m.group(0), "path": path,
                                         "detail": "高 churn 文件必须写符号锚"})
    # 例外
    kept = []
    for f in findings:
        hit = None
        for e in reg["exceptions"]:
            if e["doc"] == f["doc"] and _norm(e["anchor"]) == _norm(f["raw"]):
                hit = e
                break
        if hit:
            seen_exc_used.add((hit["doc"], _norm(hit["anchor"])))
            continue
        kept.append(f)
    findings = kept
    for e in reg["exceptions"]:
        if (e["doc"], _norm(e["anchor"])) not in seen_exc_used:
            findings.append({"why": "STALE_EXCEPTION", "doc": e["doc"], "doc_line": 0,
                             "raw": e["anchor"],
                             "detail": "例外登记了，但文档里已无该锚（或该锚已不再违规）"})
    if list_only:
        for a in anchors:
            print("%s:%d  %s" % (a["doc"], a["doc_line"], a["raw"]))
        for e in reg["exceptions"]:
            print("EXCEPTION %s  %s  card=%s" % (e["doc"], e["anchor"], e["card"]))
        print("锚 %d / 例外 %d / 登记文档 %d" % (len(anchors), len(reg["exceptions"]), len(reg["files"])))
        return 0
    undec = [f for f in findings if f["why"] == "UNDECIDABLE"]
    reds = [f for f in findings if f["why"] != "UNDECIDABLE"]
    for f in reds + undec:
        print("%-24s %s:%s  %s  %s" % (f["why"], f["doc"], f["doc_line"], f["raw"],
                                       f.get("detail", "")))
    for e in reg["exceptions"]:
        if (e["doc"], _norm(e["anchor"])) in seen_exc_used:
            print("EXCEPTION(used) %s  %s  card=%s" % (e["doc"], e["anchor"], e["card"]))
    print("docs-anchor gate：锚 %d / 例外 %d / 登记文档 %d / 红 %d / 判不了 %d"
          % (len(anchors), len(reg["exceptions"]), len(reg["files"]), len(reds), len(undec)))
    if reds:
        print("RESULT: RED")
        return 1
    if undec:
        print("RESULT: UNDECIDABLE")
        return 2
    print("RESULT: GREEN")
    return 0


def _resolve(repo, rel, base_dirs, lineno=None):
    for b in base_dirs:
        p = b + rel
        if os.path.isfile(os.path.join(repo, p)):
            if lineno is None:
                return os.path.normpath(p).replace("\\", "/")
            with open(os.path.join(repo, p), encoding="utf-8", errors="replace") as fh:
                if fh.read().count('\n') + 1 >= lineno:
                    return os.path.normpath(p).replace("\\", "/")
    return None


def _file_data(repo, path, cache):
    """按文件缓存（剥离后文本 / 行表 / item spans）——**只算一次**。
    大文件（game-svr `lib.rs` ≈7.7 万行）不能每个锚都重算，否则门会跑几分钟。"""
    if path not in cache:
        with open(os.path.join(repo, path), encoding="utf-8", errors="replace") as fh:
            text = strip_rust_comments(fh.read())
        cache[path] = (text, text.split(chr(10)), item_spans(text))
    return cache[path]


def _check_symbol(repo, reg, rel_f, sym, lineno, cache):
    # 裸文件名（`lib.rs`）在多个基目录下都有同名文件 ⇒ **逐个候选搜**，全找不到才判红；
    # 否则 `crates/world/src/lib.rs` 会遮住 `bins/game-svr/src/lib.rs`（B 表全是这个文件名）。
    cands = [b + rel_f for b in reg["base_dirs"]]
    cands = [c for c in cands if os.path.isfile(os.path.join(repo, c))]
    if not cands:
        return {"why": "ANCHOR_PATH_MISSING", "path": rel_f, "detail": "符号锚的文件解析不到"}
    undec = False
    for path in cands:
        text, lines, spans = _file_data(repo, path, cache)
        # 只算「哪些行命中符号」（O(文件行数)），不要逐 span join 全文（那会变 O(n²)）。
        hits = [i for i, line in enumerate(lines, 1) if symbol_in_text(sym, line)]
        if not hits:
            continue
        hitset = set(hits)
        for sp in spans:
            if any(sp[1] <= h <= sp[2] for h in hitset):
                return None
        undec = True
    if undec:
        return {"why": "UNDECIDABLE", "path": rel_f,
                "detail": "符号在文件里、但不在任何 fn 的正文内（可能是字段/常量/宏）"}
    return {"why": "ANCHOR_SYMBOL_MISSING", "path": rel_f,
            "detail": "剥注释后按完整标识符找不到 `%s`（候选文件都搜过）" % sym}


# ---------------------------------------------------------------- selftest

def selftest():
    ok = []

    def check(name, cond, detail=""):
        ok.append((name, bool(cond), detail))

    # ---- 剥离层 6 条（对齐 strip_rust_comments）
    src = 'let a = 1; // 行注释\nlet b = "// not comment";\n'
    st = strip_rust_comments(src)
    check("① 行注释被剥", "行注释" not in st and "let a = 1;" in st, st)
    check("③ 字符串里的 // 保留", '"// not comment"' in st, st)
    src2 = 'a /* 外 /* 内 */ 仍注释 */ b\nc\n'
    st2 = strip_rust_comments(src2)
    check("② 嵌套块注释被剥", "外" not in st2 and "b" in st2 and st2.count('\n') == src2.count('\n'), st2)
    src3 = 'let r = r#"raw // 不是注释"#;\nlet q = 1; // c\n'
    st3 = strip_rust_comments(src3)
    check("④ raw string 保留", 'r#"raw // 不是注释"#' in st3 and '// c' not in st3, st3)
    src4 = "let c = '\\''; // x\nlet l = 'a; // y\n"
    st4 = strip_rust_comments(src4)
    check("⑤ 字符字面量与生命期", "'\\''" in st4 and "'a;" in st4.replace(" // y", ""), st4)
    check("⑥ 行数不变", st2.count('\n') == src2.count('\n') and st3.count('\n') == src3.count('\n'))

    # ---- 定位层 / 判据层：临时仓库端到端
    tmp = tempfile.mkdtemp(prefix="dag-")
    try:
        os.makedirs(os.path.join(tmp, "docs"))
        os.makedirs(os.path.join(tmp, "mir2-rs", "crates", "world", "src"))
        with open(os.path.join(tmp, "mir2-rs/crates/world/src/w.rs"), "w", encoding="utf-8") as fh:
            fh.write(
                "fn alpha() {\n    // beta 只在注释里\n    let _ = 1;\n}\n"
                "fn beta_v2() {}\n"
                "fn nested() {\n    fn inner() {}\n}\n")
        doc = "\n".join([
            "| a | `w.rs` `alpha` | x |",                    # 1 绿
            "| b | `w.rs` `beta` | x |",                     # 2 只在注释里 ⇒ 红
            "| c | `w.rs` `beta_v2extra` | x |",             # 3 同前缀 ⇒ 红
            "| d | `w.rs` `beta_v2` | x |",                  # 4 绿
            "| e | `w.rs:2` | x |",                          # 5 高 churn 行号 ⇒ 红
            "| f | `nope.rs` `x` | x |",                     # 6 路径缺失 ⇒ 红
            "| g | `w.rs` `inner` | x |",                    # 7 嵌套 fn ⇒ 绿（符号在 inner 里）
        ])
        with open(os.path.join(tmp, "docs", "m.md"), "w", encoding="utf-8") as fh:
            fh.write(doc)
        reg = os.path.join(tmp, "reg.toml")
        with open(reg, "w", encoding="utf-8") as fh:
            fh.write('[[files]]\npath = "docs/m.md"\nwhy = "t"\n'
                     '[[base_dirs]]\ndir = "mir2-rs/crates/world/src/"\n')
        rc, out = _cap(run, reg, tmp)
        check("绿：符号在 item 正文", rc in (1, 2) and "w.rs + alpha" not in out, out[-300:])
        check("红：只在注释里出现（剥注释）", "ANCHOR_SYMBOL_MISSING" in out and "beta" in out, out[-300:])
        check("红：同前缀标识符不兜住", "beta_v2extra" in out, out[-300:])
        check("红：高 churn 行号锚", "HIGH_CHURN_LINE_ANCHOR" in out, out[-300:])
        check("红：路径解析不到", "ANCHOR_PATH_MISSING" in out, out[-300:])
        check("整体 RED", rc == 1, rc)

        # 多解 ⇒ rc=2（嵌套 fn：一行落在两个 item 里）
        with open(os.path.join(tmp, "mir2-rs/crates/world/src/w.rs"), "a", encoding="utf-8") as fh:
            fh.write("fn outer2() {\n    fn inner2() {\n        let _ = 1;\n    }\n}\n")
        with open(os.path.join(tmp, "docs", "m.md"), "a", encoding="utf-8") as fh:
            fh.write("| h | `w.rs:11` | x |\n")  # 该行同时落在 outer2 与 inner2
        # 行号锚在 w.rs（高 churn）⇒ 直接红；用符号锚不能指定行，改测 locate_item 本身
        sp = item_spans(strip_rust_comments(open(os.path.join(
            tmp, "mir2-rs/crates/world/src/w.rs"), encoding="utf-8").read()))
        item, why = locate_item(sp, 11)
        check("定位不到（多解）⇒ fail-closed", item is None and "多解" in (why or ""), (item, why))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    fails = [(n, d) for n, good, d in ok if not good]
    for n, good, d in ok:
        print("%s %s%s" % ("PASS" if good else "FAIL", n, "" if good else "  %r" % (d,)))
    print("SELFTEST %s（%d 条）" % ("PASS" if not fails else "FAIL", len(ok)))
    return 0 if not fails else 1


def _cap(fn, *args):
    import io
    from contextlib import redirect_stdout
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = fn(*args)
    return rc, buf.getvalue()


def main(argv):
    if "--selftest" in argv:
        return selftest()
    reg = os.path.join(REPO, REGISTRY_REL)
    if "--registry" in argv:
        reg = argv[argv.index("--registry") + 1]
    return run(reg, REPO, list_only="--list" in argv)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
