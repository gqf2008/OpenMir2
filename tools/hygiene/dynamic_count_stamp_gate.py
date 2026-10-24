#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dynamic_count_stamp_gate — a drifting count must carry a snapshot stamp.

Card: openmir2-dynamic-count-stamp-gate (tier=L).  See also
``docs/dynamic-count-stamp-gate-20261024.md`` (what this gate does *not* catch).

``docs/`` is full of numbers that rot: board column counts, R-card totals,
``FAIL`` counts, runnable-scenario counts, recon ``tracked`` counts.  Three
incidents in one day came from a reader (or a dispatch decision) taking a stale
number as current.  The rule this gate enforces is narrow:

  A **drift-prone count** must carry a **snapshot stamp** within ``--max-gap``
  characters on the **same line**: ``@ <sha>``, ``截至 <sha>``, ``@ YYYY-MM-DD``
  or ``快照 … <sha>`` (see ``STAMP_RE`` below — the exact regexes are part of
  the contract and are exercised by ``--selftest``).

This module deliberately **reuses the stale-pointer gate's plumbing** rather
than forking a second scanner (reviewer-3 finding F2 on 1679b4fa9: the copy had
already drifted — unscoped rot, no cross-drive guard, no quotation exemption,
``os.walk`` instead of the tracked file set).  Shared, via
``tools/hygiene/stale_pointer_gate.py``:

  * ``GateError`` (identical rc semantics), ``repo_root``, ``repo_relative``
  * ``markdown_files`` — **tracked** Markdown only (``git ls-files``); an
    untracked scratch ``.md`` is not part of the tree the gate judges
  * ``line_fingerprint`` — the allow-list key
  * ``load_allowlist`` (``require_added=True``) and ``allowlist_in_scope`` —
    allow-list validation and the rot scope (a narrowed ``--path`` must not
    flag entries that were simply not scanned)
  * ``QUOTE_RE`` — the quotation exemption: a count inside 「…」/『…』 is
    being quoted, not asserted, so it is not a claim about the current state

What is *only* here: ``STAMP_RE``, ``COUNT_PATTERNS`` and this gate's selftest.

Patterns recognised as drift-prone (Chinese matched with Python's unicode
``re`` — never byte-mode ``grep -P``):

  * board columns:   ``待认领 / 进行中 / 待审 / 已完成`` + number (also ``'进行中': 236``)
  * R cards:         ``R 卡 198 张`` / ``R 卡 198``
  * FAIL:            ``FAIL 0`` / ``FAIL 卡 3``
  * runnable:        ``可跑场景 18``
  * recon tracked:   ``tracked 0``

Exemptions (all mandatory to stay narrow):

  * **allow-list** — ``tools/hygiene/dynamic_count_allowlist.toml`` keyed by
    ``(path, line_sha256)`` with a mandatory ``reason`` **and** ``added``
    timestamp.  An entry that suppresses no current hit inside the scanned
    subtree is **rot** and fails the gate itself (rc=1).
  * **quotation** — a hit wholly inside 「…」/『…』 is a citation, not an
    assertion (shared ``QUOTE_RE`` with the skeleton).  A line that also has an
    unquoted count is still red.
  * ``--no-stamp-check`` — turns the check off **loudly** (prints
    ``stamp-check: 已跳过``); it is never a silent downgrade.

Usage (run from anywhere inside the repo):

  python -I tools/hygiene/dynamic_count_stamp_gate.py              # scan docs/
  python -I tools/hygiene/dynamic_count_stamp_gate.py --path docs/rules
  python -I tools/hygiene/dynamic_count_stamp_gate.py --max-gap 40
  python -I tools/hygiene/dynamic_count_stamp_gate.py --no-stamp-check
  python -I tools/hygiene/dynamic_count_stamp_gate.py --fingerprint docs/foo.md:123
  python -I tools/hygiene/dynamic_count_stamp_gate.py --selftest

Exit codes: 0 clean (or selftest passed); 1 missing stamps or allow-list rot;
2 usage error / scan target outside the repository, target not found,
unreadable Markdown or allow-list (fail-closed).
"""

from __future__ import annotations

import argparse
import io
import os
import re
import sys
import subprocess
import tempfile

# ``python -I`` does not put the script's directory on sys.path (it implies
# -P), so the sibling module has to be made importable explicitly.
_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:
    sys.path.insert(0, _HERE)

import stale_pointer_gate as skeleton  # noqa: E402  (path set up above)

GateError = skeleton.GateError

DEFAULT_PATH = "docs"
DEFAULT_ALLOWLIST = os.path.join(
    "tools", "hygiene", "dynamic_count_allowlist.toml")
DEFAULT_MAX_GAP = 24

# Snapshot stamps.  Same line only (cross-line stamps are a documented blind
# spot: the window never crosses ``\n``).
STAMP_RE = re.compile(
    r"(?:@|截至|as\s+of)\s*[`'\"（(]?\s*[0-9a-f]{7,40}(?![0-9a-fA-F])"  # @ <sha>
    r"|(?:@|截至|as\s+of)\s*[`'\"（(]?\s*20\d{2}-\d{2}-\d{2}"             # @ 2026-10-24
    r"|快照[^\n]{0,12}[`'\"（(]?\s*[0-9a-f]{7,40}(?![0-9a-fA-F])"          # 快照 <sha>
    r"|快照[^\n]{0,12}[`'\"（(]?\s*20\d{2}-\d{2}-\d{2}"                    # 快照 2026-10-24
)

# Drift-prone counts.  ``[^\n\d]{0,6}`` skips the separators people write
# (``：``, `` `` , ``'` ``, ``=``) without ever swallowing another number.
COUNT_PATTERNS = (
    # board column counts: 「进行中 236」「'待认领': 2」「已完成 269」——the number
    # must follow the column word directly (separators allowed, no other digits).
    ("board", re.compile(r"(?:待认领|进行中|待审|已完成)[^\n\d{}]{0,6}(\d+)(?![\d)])")),
    # R-card totals are written「R 卡 198 张」——require the unit 张 so prose like
    # 「R 卡证据」never matches.
    ("r-cards", re.compile(r"R\s*卡\s*[:：]?\s*(\d+)\s*张")),
    # FAIL counts:「FAIL 卡 0」「FAIL 3」——not `[FAIL]`, not `FAILED`, not `FAIL (rc…)`.
    ("fail", re.compile(r"(?<![A-Za-z\[])FAIL\s*卡?\s*[:：]?\s*(\d+)(?![\d)])")),
    ("runnable", re.compile(r"可跑场景\s*[:：]?\s*(\d+)")),
    ("tracked", re.compile(r"tracked\s*(?:=|:|：)?\s*(\d+)(?![0-9a-fA-F])")),
)


def repo_root():
    try:
        return skeleton.repo_root()
    except GateError:
        raise GateError("dynamic-count gate: not inside a git repository", rc=2)


def resolve_scan_root(top, path):
    """Repo-relative scan root; anything outside the repo is fail-closed rc=2.

    ``os.path.relpath`` raises ``ValueError`` (not ``GateError``) for a path on
    another Windows drive, so a bare call would print a traceback and exit 1 —
    exactly the fail-open shape this project keeps paying for.
    """
    try:
        return skeleton.repo_relative(top, os.path.join(top, path))
    except ValueError:
        raise GateError(
            "dynamic-count gate: --path %r is on another drive than the "
            "repository %r (fail-closed)" % (path, top), rc=2)
    except GateError:
        raise GateError(
            "dynamic-count gate: --path %r is outside the repository %r "
            "(fail-closed)" % (path, top), rc=2)


def markdown_files(top, rel_path):
    return skeleton.markdown_files(top, rel_path)


def line_fingerprint(line):
    return skeleton.line_fingerprint(line)


def load_allowlist(path):
    """Shared loader; this gate additionally requires a dated ``added``."""
    return skeleton.load_allowlist(path, require_added=True,
                                   prog="dynamic-count gate")


def quoted_spans(line):
    """Spans of 「…」 / 『…』 — shared with the stale-pointer gate."""
    return [(m.start(), m.end()) for m in skeleton.QUOTE_RE.finditer(line)]


def stamp_near(line, start, end, max_gap):
    """True when a snapshot stamp sits within ``max_gap`` chars of the match."""
    left = max(0, start - max_gap)
    right = min(len(line), end + max_gap)
    return bool(STAMP_RE.search(line[left:right]))


def count_hits(line):
    """Return [(start, end, kind, number)] for drift-prone counts on ``line``.

    A match whose **leading word** sits inside a quotation is skipped: quoted
    text cites someone else's number, it is not this document's claim about the
    present.  (Same rule as the skeleton's ``candidate_cards``: it tests the
    status word, not the whole match, so ``「未开始/待认领」指 §17.1`` never
    measures the number that happens to follow the closing bracket.)
    """
    quotes = quoted_spans(line)
    hits = []
    for kind, pattern in COUNT_PATTERNS:
        for match in pattern.finditer(line):
            if any(a <= match.start() < b for a, b in quotes):
                continue
            hits.append((match.start(), match.end(), kind, match.group(1)))
    hits.sort()
    return hits


# ---- 段落级快照声明（卡 openmir2-dynamic-count-paragraph-snapshot-exemption）----
#
# 背景：allow-list 里 92 条中有 23 条属「整段已声明快照、段内计数随之冻结」的文档；
# 每次文档作者改一行，rot 就把别人写集的改动变成对本门的阻塞。这里加一个**受检**的
# 段落级豁免：只有同一段里、命中之前、且四道检查（祖先 / 时间容差 / 先后 / blame 新近性）
# 全过的声明，才把命中降级为 INFO。
#
# 「段」的定义（可机械判定，不用「同行 ± N 字符」这种会随排版漂移的口径）：
#   命中行向上找**最近的标题行**（`#`..`######`）作为段首；段尾 = 其后第一个
#   **同级或更高级**标题的前一行（没有上级标题 ⇒ 到文件末尾；全文无标题 ⇒ 整篇为一段）。
#
# 标题识别按 CommonMark 收窄了两个会改变豁免宽度的边界（reviewer-6 M-5）：
#   * 代码围栏（``` / ~~~）内的 `# 注释` **不是**标题——否则段被切小，段内合法声明
#     覆盖不到命中（假红）；
#   * ATX 标题允许 **0–3 个前导空格**，且识别 setext 标题（`标题` + `===`/`---`）——
#     否则这类标题不被识别，下一节的裸计数会被**上一节**的声明豁免（假绿）。
PARAGRAPH_HEADING_RE = re.compile(r"^ {0,3}(#{1,6})(?:\s+|$)")
FENCE_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
SETEXT_UNDERLINE_RE = re.compile(r"^ {0,3}(=+|-{2,})\s*$")
_SETEXT_TITLE_BLOCKERS_RE = re.compile(r"^ {0,3}(?:[-*+]\s|\d+[.)]\s|>|\|)")
DEFAULT_PARAGRAPH_TOL = 3600   # 声明时间与 commit 时间的容差（秒），理由见文档 §2
DEFAULT_DECL_TOL = 86400       # blame 新近性容差（秒，与 skeleton --decl-tol 同值）


def heading_levels(lines):
    """Return ``{line_no: level}`` for headings outside fenced code blocks.

    ATX (0–3 leading spaces) and setext (``===`` ⇒ 1, ``---`` ⇒ 2) are both
    recognised; a setext heading is attributed to its **text** line.  A fenced
    block never yields headings (its ``# comment`` lines used to split the
    section and cause false reds).  Fences track both character and length.
    """
    levels = {}
    fence = None  # (char, length)
    prev_is_title_text = False
    for index, line in enumerate(lines, 1):
        if fence is not None:
            marker = FENCE_RE.match(line)
            if (marker and marker.group(1)[0] == fence[0]
                    and len(marker.group(1)) >= fence[1]):
                fence = None
            prev_is_title_text = False
            continue
        marker = FENCE_RE.match(line)
        if marker:
            fence = (marker.group(1)[0], len(marker.group(1)))
            prev_is_title_text = False
            continue
        atx = PARAGRAPH_HEADING_RE.match(line)
        if atx:
            levels[index] = len(atx.group(1))
            prev_is_title_text = False
            continue
        underline = SETEXT_UNDERLINE_RE.match(line)
        if underline and prev_is_title_text:
            levels[index - 1] = 1 if underline.group(1)[0] == "=" else 2
            prev_is_title_text = False
            continue
        prev_is_title_text = bool(
            line.strip()
            and not line.startswith("    ")
            and not _SETEXT_TITLE_BLOCKERS_RE.match(line)
        )
    return levels


def paragraph_bounds(lines, hit_line, levels=None):
    """Return (start, end) 1-based inclusive for the section containing ``hit_line``."""
    if levels is None:
        levels = heading_levels(lines)
    level = 0
    start = 1
    for i in range(hit_line, 0, -1):
        if i in levels:
            level = levels[i]
            start = i
            break
    if level == 0:
        return 1, len(lines)
    end = len(lines)
    for j in range(hit_line + 1, len(lines) + 1):
        if j in levels and levels[j] <= level:
            end = j - 1
            break
    return start, end


def paragraph_declarations(lines, start, end):
    """Candidate declarations *inside* the section ``[start, end]`` (nearest-first).

    Every distinct sha in the ±``SNAPSHOT_NEAR`` window is its own candidate,
    paired with the nearest timestamp (reviewer-6 M-1, the same shape as
    skeleton reviewer-3 M-1): a line like ``快照 截至 <B 旧> 与 <A 新> @ T``
    must not lose ``A`` because ``B`` is written first, and a neighbouring
    line's sha must not swallow the real declaration.  The caller tries them
    nearest-first; the first passing **all** checks wins, rejected candidates
    become NOTEs.
    """
    out = []
    for anchor in range(start, end + 1):
        if not skeleton.SNAPSHOT_ANCHOR_RE.search(lines[anchor - 1]):
            continue
        low = max(start, anchor - skeleton.SNAPSHOT_NEAR)
        high = min(end, anchor + skeleton.SNAPSHOT_NEAR)
        times = []
        for index in range(low, high + 1):
            for value in skeleton._line_times(lines[index - 1]):
                times.append((index, value[0], value[1]))
        if not times:
            continue
        times.sort(key=lambda item: (abs(item[0] - anchor), item[0]))
        time_line, epoch, time_text = times[0]
        seen = set()
        for distance in range(0, high - low + 1):
            for index in (anchor - distance, anchor + distance):
                if not low <= index <= high:
                    continue
                for sha in skeleton._line_shas(lines[index - 1]):
                    if sha in seen:
                        continue
                    seen.add(sha)
                    out.append({
                        "anchor": anchor,
                        "sha_line": index,
                        "sha": sha,
                        "time_line": time_line,
                        "epoch": epoch,
                        "time_text": time_text,
                        "position": max(anchor, index, time_line),
                    })
    return out


def checked_paragraph_declarations(top, rel, lines, start, end, hit_line, ref,
                                   paragraph_tol, decl_tol, commit_cache,
                                   blame_cache):
    """Return (infos, notes): declarations passing (a)(b)(c)(d)."""
    infos = []
    notes = []
    for decl in paragraph_declarations(lines, start, end):
        if decl["position"] >= hit_line:
            notes.append("L%d: 声明在命中行之后（%s）" % (decl["anchor"], decl["sha"]))
            continue
        try:
            ancestor = skeleton.is_ancestor(top, decl["sha"], ref)
        except GateError as exc:
            notes.append("L%d: 祖先判定失败：%s" % (decl["anchor"], exc))
            continue
        if not ancestor:
            notes.append("L%d %s: 不是 --ref %s 的祖先"
                         % (decl["anchor"], decl["sha"], ref))
            continue
        times = skeleton._commit_time(top, decl["sha"], commit_cache)
        if times is None:
            notes.append("L%d %s: 读不到 commit 时间" % (decl["anchor"], decl["sha"]))
            continue
        delta = min(abs(decl["epoch"] - times[0]), abs(decl["epoch"] - times[1]))
        if delta > paragraph_tol:
            notes.append("L%d %s: 声明时间与 commit 时间差 %ds > --paragraph-tol %ds"
                         % (decl["anchor"], decl["sha"], delta, paragraph_tol))
            continue
        blamed_lines = sorted({decl["anchor"], decl["time_line"], decl["sha_line"]})
        blamed = skeleton._blame_delta(top, rel, blamed_lines, decl["epoch"],
                                       blame_cache)
        if blamed is None:
            notes.append("L%d: 声明行未提交，blame 不可用" % decl["anchor"])
            continue
        if blamed > decl_tol:
            notes.append("L%d: 声明行写入时间距声称时间 %ds > --decl-tol %ds（事后补的？）"
                         % (decl["anchor"], blamed, decl_tol))
            continue
        infos.append(dict(decl, delta=delta, blame_delta=blamed))
    return infos, notes


def scan(top, rel_path, allowlist, max_gap, stamp_check=True, files=None,
         ref=None, paragraph_tol=DEFAULT_PARAGRAPH_TOL, decl_tol=DEFAULT_DECL_TOL):

    """Scan ``rel_path``; return findings / rot / unchecked rows.

    ``files`` (repo-relative) lets the selftest run against a synthetic tree
    without a git repository; ``None`` means "the tracked Markdown under
    ``rel_path``", enumerated by the shared skeleton helper.
    """
    if files is None:
        if not os.path.exists(os.path.join(top, rel_path)):
            raise GateError(
                "dynamic-count gate: scan target not found: %s (fail-closed)"
                % rel_path, rc=2)
        files = markdown_files(top, rel_path)
    findings = []
    unchecked = []
    infos = []
    notes = []
    decl_cache = {}
    commit_cache = {}
    blame_cache = {}
    for rel in files:
        try:
            with open(os.path.join(top, rel), "rb") as fh:
                text = fh.read().decode("utf-8")
        except (OSError, UnicodeDecodeError):
            raise GateError(
                "dynamic-count gate: cannot read %s (fail-closed)" % rel, rc=2)
        all_lines = text.splitlines()
        levels = heading_levels(all_lines)
        for number, line in enumerate(all_lines, 1):
            hits = count_hits(line)
            if not hits:
                continue
            digest = line_fingerprint(line)
            if not stamp_check:
                unchecked.append((rel, number, line.strip()[:120]))
                continue
            missing = [h for h in hits
                       if not stamp_near(line, h[0], h[1], max_gap)]
            if not missing:
                continue
            entry = next((e for e in allowlist
                          if e["path"] == rel and e["line_sha256"] == digest), None)
            if entry is not None:
                entry["used"] = True
                continue
            if ref is not None:
                key = (rel, number)
                if key not in decl_cache:
                    start, end = paragraph_bounds(all_lines, number, levels)
                    decl_cache[key] = checked_paragraph_declarations(
                        top, rel, all_lines, start, end, number, ref,
                        paragraph_tol, decl_tol, commit_cache, blame_cache)
                infos_here, notes_here = decl_cache[key]
                # Rejected candidates are always reported (reviewer-6 M-1):
                # keeping the NOTE only when *every* candidate failed hid why
                # a neighbouring sha was refused.
                notes.extend("%s:%d %s" % (rel, number, n) for n in notes_here)
                if infos_here:
                    decl = infos_here[0]
                    infos.append({
                        "path": rel, "line": number,
                        "snippet": line.strip()[:160],
                        "decl": "L%d %s @ %s" % (decl["anchor"], decl["sha"],
                                                 decl["time_text"]),
                        "kinds": sorted({h[2] for h in missing}),
                    })
                    continue
            findings.append({
                "path": rel, "line": number, "line_sha": digest,
                "snippet": line.strip()[:160],
                "kinds": sorted({h[2] for h in missing}),
            })
    # Shared rot scope: only entries inside the scanned subtree can rot.
    rotten = [e for e in skeleton.allowlist_in_scope(allowlist, rel_path)
              if not e["used"]]
    return {"findings": findings, "rotten": rotten, "unchecked": unchecked,
            "infos": infos, "notes": notes, "files": len(files)}


def emit(result, rel_path, max_gap, allowlist_path, err=sys.stderr, out=sys.stdout):
    if result["unchecked"]:
        print("stamp-check: 已跳过（--no-stamp-check）——%d 行计数未检查" %
              len(result["unchecked"]), file=out)
        return 0
    for item in result.get("infos") or []:
        print("INFO %s:%d\t%s\t段落声明 %s（降级，不计缺戳）"
              % (item["path"], item["line"], item["snippet"], item["decl"]), file=out)
    for note in result.get("notes") or []:
        print("NOTE %s（段落声明未通过检查）" % note, file=err)
    for item in result["findings"]:
        print("%s:%d\t%s\t缺快照戳（%s）"
              % (item["path"], item["line"], item["snippet"],
                 ",".join(item["kinds"])), file=out)
    for entry in result["rotten"]:
        print("ROT %s: line_sha256=%s 未抑制任何命中（allow-list 腐烂）"
              % (entry["path"], entry["line_sha256"][:16]), file=err)
    if result["findings"] or result["rotten"]:
        print("dynamic-count gate: FAIL (%d 缺戳 / %d allow-list 腐烂；扫描 %d 个 "
              "markdown，path=%s，max-gap=%d，allow-list=%s)"
              % (len(result["findings"]), len(result["rotten"]),
                 result["files"], rel_path, max_gap, allowlist_path), file=err)
        return 1
    print("dynamic-count gate: OK（0 缺戳；%d 条段落声明降级 INFO；扫描 %d 个 "
          "markdown，path=%s，max-gap=%d）"
          % (len(result.get("infos") or []), result["files"], rel_path, max_gap),
          file=out)
    return 0


def selftest():
    """Synthetic fixtures only — no repo, docs tree or real allow-list needed."""
    checks = []

    def check(name, ok):
        checks.append((name, bool(ok)))

    def run_tree(files, allow=(), stamp_check=True, max_gap=DEFAULT_MAX_GAP,
                 scan_root="docs", scan_files=None):
        with tempfile.TemporaryDirectory(prefix="dynamic-count-") as top:
            for rel, body in files.items():
                path = os.path.join(top, rel)
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8", newline="\n") as fh:
                    fh.write(body)
            allow_entries = [dict(e, used=False) for e in allow]
            swept = scan(top, scan_root, allow_entries, max_gap, stamp_check,
                         files=sorted(files) if scan_files is None else scan_files)
        return swept, allow_entries

    result, _ = run_tree({"docs/a.md": "# x\n看板 进行中 236（截至 `3738ad234`）\n"})
    check("① 带戳的计数 => 绿", result["findings"] == [])

    result, _ = run_tree({"docs/a.md": "看板 进行中 236\n"})
    check("② 裸计数 => 红", len(result["findings"]) == 1)

    result, _ = run_tree(
        {"docs/a.md": "看板 进行中 236" + " " * 40 + "@ 3738ad234\n"})
    check("③ 戳在 --max-gap 之外 => 红", len(result["findings"]) == 1)
    result2, _ = run_tree(
        {"docs/a.md": "看板 进行中 236" + " " * 40 + "@ 3738ad234\n"},
        max_gap=80)
    check("③b 同一条把 --max-gap 放大 => 绿", result2["findings"] == [])

    tree = {
        "docs/keep.md": "R 卡 198 张\n",
        "docs/sub/skip.md": "FAIL 3\n",
    }
    root, _ = run_tree(tree)
    narrowed, _ = run_tree(tree, scan_root="docs/sub",
                           scan_files=["docs/sub/skip.md"])
    check("④ --path 缩窄生效（全树 2 红 vs 子目录 1 红）",
          len(root["findings"]) == 2 and len(narrowed["findings"]) == 1
          and narrowed["findings"][0]["path"].endswith("sub/skip.md"))

    with tempfile.TemporaryDirectory(prefix="dynamic-count-") as top:
        try:
            scan(top, "missing-dir", [], DEFAULT_MAX_GAP)
            unavailable = False
        except GateError as exc:
            unavailable = exc.rc == 2
    check("⑤ 扫描目标不存在 => rc=2（fail-closed）", unavailable)

    result, _ = run_tree({"docs/a.md": "看板 进行中 236\n"}, stamp_check=False)
    buf = io.StringIO()
    rc_skip = emit(result, "docs", DEFAULT_MAX_GAP, "synthetic", out=buf,
                   err=io.StringIO())
    check("⑥ --no-stamp-check => 打印「已跳过」且 rc=0",
          "已跳过" in buf.getvalue() and rc_skip == 0)

    line = "R 卡 198 张\n"
    digest = line_fingerprint(line.rstrip("\n"))
    allow = [{"path": "docs/a.md", "line_sha256": digest,
              "reason": "历史快照", "added": "2026-10-24"}]
    result, _ = run_tree({"docs/a.md": line}, allow=allow)
    check("⑦ allow-list 带理由+日期 => 抑制该命中",
          result["findings"] == [] and result["rotten"] == [])

    stale_allow = [{"path": "docs/a.md", "line_sha256": "0" * 64,
                    "reason": "不存在的行", "added": "2026-10-24"}]
    result, _ = run_tree({"docs/a.md": "看板 进行中 236（@ 3738ad234）\n"},
                         allow=stale_allow)
    check("⑧ allow-list 条目压不到命中 => rot（rc=1 路径）",
          len(result["rotten"]) == 1)

    # ⑨ rot must be scoped to the scanned subtree (reviewer-3 F1): a non-empty
    # allow-list plus a narrowed --path that covers no entry must stay green.
    tree = {"docs/a.md": line, "docs/sub/clean.md": "# 干净\n"}
    full, _ = run_tree(tree, allow=allow)
    narrowed, _ = run_tree(tree, allow=allow, scan_root="docs/sub",
                           scan_files=["docs/sub/clean.md"])
    buf = io.StringIO()
    rc_narrow = emit(narrowed, "docs/sub", DEFAULT_MAX_GAP, "synthetic", out=buf,
                     err=io.StringIO())
    check("⑨ 非空 allow-list + 收窄 --path（不覆盖任何条目）=> 不报 rot、rc=0",
          full["findings"] == [] and full["rotten"] == []
          and narrowed["findings"] == [] and narrowed["rotten"] == []
          and rc_narrow == 0)

    # ⑩ a narrowed scan that does cover one entry: that entry must be marked
    # used, the covered-but-outside entry must not be called rot.
    tree = {"docs/a.md": line, "docs/sub/b.md": "FAIL 3\n"}
    sub_line = "FAIL 3\n"
    allow_two = [
        {"path": "docs/a.md", "line_sha256": digest,
         "reason": "子树外", "added": "2026-10-24"},
        {"path": "docs/sub/b.md", "line_sha256": line_fingerprint(sub_line),
         "reason": "子树内", "added": "2026-10-24"},
    ]
    narrowed, entries = run_tree(tree, allow=allow_two, scan_root="docs/sub",
                                 scan_files=["docs/sub/b.md"])
    check("⑩ 收窄覆盖某条目 => 该条目已用、子树外条目不判 rot",
          narrowed["findings"] == [] and narrowed["rotten"] == []
          and entries[0]["used"] is False and entries[1]["used"] is True)

    # ⑪ quotation exemption (shared QUOTE_RE): quoted counts are citations.
    result, _ = run_tree({"docs/a.md": "旧文写「可跑场景 16」，已作废\n"})
    check("⑪ 引述「可跑场景 16」=> 绿（引述豁免）", result["findings"] == [])
    result, _ = run_tree({"docs/a.md": "旧文写「可跑场景 16」，现为 可跑场景 18\n"})
    check("⑪b 引述外仍有裸计数 => 红", len(result["findings"]) == 1)
    result, _ = run_tree({"docs/a.md": "行内「未开始/待认领」指 §17.1 其余四项\n"})
    check("⑪c 引述内的词不吞引述外数字（「待认领」指 §17.1）=> 绿",
          result["findings"] == [])

    # ⑫ a scan root outside the repo is rc=2, never a traceback (reviewer-3 F2).
    with tempfile.TemporaryDirectory(prefix="dynamic-count-") as top:
        try:
            resolve_scan_root(top, os.path.join("..", "outside"))
            outside_rc = None
        except GateError as exc:
            outside_rc = exc.rc
    check("⑫ 扫描根在仓库外 => rc=2（不是 traceback）", outside_rc == 2)

    # ⑬-⑰ allow-list validation is pinned by selftest, not just at runtime:
    # a valid entry loads; missing reason / missing added / bad sha / duplicate
    # entry are all rc=2 (reviewer-3 F4).
    with tempfile.TemporaryDirectory(prefix="dynamic-count-") as td:
        good_entry = ('[[allow]]\npath = "docs/a.md"\nline_sha256 = "%s"\n'
                      'reason = "why"\nadded = "2026-10-24"\n' % ("a" * 64))

        def load_probe(name, body):
            path = os.path.join(td, name)
            with open(path, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(body)
            try:
                return load_allowlist(path)
            except GateError as exc:
                return exc.rc

        loaded = load_probe("ok.toml", good_entry)
        check("⑬ 合法条目 => 载入成功（含 added）",
              isinstance(loaded, list) and loaded[0].get("added") == "2026-10-24")
        check("⑭ 缺 reason => rc=2",
              load_probe("no-reason.toml",
                         '[[allow]]\npath = "docs/a.md"\nline_sha256 = "%s"\n'
                         'added = "2026-10-24"\n' % ("a" * 64)) == 2)
        check("⑮ 缺 added => rc=2",
              load_probe("no-added.toml",
                         '[[allow]]\npath = "docs/a.md"\nline_sha256 = "%s"\n'
                         'reason = "why"\n' % ("a" * 64)) == 2)
        check("⑯ 坏 sha => rc=2",
              load_probe("bad-sha.toml",
                         '[[allow]]\npath = "docs/a.md"\nline_sha256 = "abc"\n'
                         'reason = "why"\nadded = "2026-10-24"\n') == 2)
        check("⑰ 重复条目 => rc=2",
              load_probe("dup.toml", good_entry + "\n" + good_entry) == 2)

    # ⑱ the enumeration surface is pinned, not just documented (reviewer-3
    # M1): only **git-tracked** Markdown is scanned, via the shared skeleton
    # ``markdown_files`` (``git ls-files``).  Swapping that helper for
    # ``os.walk`` used to keep the whole selftest green while silently scanning
    # untracked files (and losing the fail-closed unreadable-file path); this
    # fixture makes such a switch red: the untracked bare count would be found.
    with tempfile.TemporaryDirectory(prefix="dynamic-count-") as top:
        subprocess.run(["git", "-C", top, "init", "-q"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       check=True)
        os.makedirs(os.path.join(top, "docs"), exist_ok=True)
        for name, body in (("tracked.md", "# 干净\n"),
                           ("untracked.md", "FAIL 3\n")):
            with open(os.path.join(top, "docs", name), "w",
                      encoding="utf-8", newline="\n") as fh:
                fh.write(body)
        subprocess.run(["git", "-C", top, "add", "docs/tracked.md"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       check=True)
        tracked_scan = scan(top, "docs", [], DEFAULT_MAX_GAP)  # files=None
    check("⑱ 枚举只用 git 跟踪集（未跟踪的裸计数不扫；改 os.walk 即红）",
          tracked_scan["files"] == 1 and tracked_scan["findings"] == [])

    # ---- ⑲–㉔ 段落级快照声明（卡 openmir2-dynamic-count-paragraph-snapshot-exemption）----
    T0 = "2026-10-24T10:00:00+08:00"
    T1 = "2026-10-24T10:05:00+08:00"
    T_LATE = "2026-10-24T15:00:00+08:00"      # 与 T0 差 5h > paragraph-tol 3600
    T_FORGED = "2026-10-27T10:00:00+08:00"    # 声明行 3 天后才提交 > decl-tol 86400

    def _git(tmp, *args, env=None):
        return subprocess.run(["git", "-C", tmp] + list(args), capture_output=True,
                              text=True, env=env)

    def _commit(tmp, when, files):
        for rel, body in files.items():
            path = os.path.join(tmp, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(body)
        env = dict(os.environ, GIT_AUTHOR_DATE=when, GIT_COMMITTER_DATE=when)
        _git(tmp, "add", "-A", env=env)
        _git(tmp, "commit", "-q", "-m", "x", env=env)
        return _git(tmp, "rev-parse", "HEAD").stdout.strip()

    def _fixture(build):
        with tempfile.TemporaryDirectory(prefix="dc-para-") as tmp:
            _git(tmp, "init", "-q")
            _git(tmp, "config", "user.email", "t@example.invalid")
            _git(tmp, "config", "user.name", "t")
            build(tmp)
            return scan(tmp, "docs", [], DEFAULT_MAX_GAP,
                        files=["docs/a.md"], ref="HEAD")

    def _case_a(tmp):
        sha1 = _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("# t\n\n## 段\n\n快照 截至 %s @ %s\n\n进行中 236\n"
               % (sha1, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_a = _fixture(_case_a)
    check("⑲ 段内合法声明（祖先 + 时间吻合 + 声明在前 + blame 新近）=> INFO",
          len(res_a["infos"]) == 1 and res_a["findings"] == [])

    def _case_b(tmp):
        # A **real, valid** commit that is NOT an ancestor of --ref (HEAD): a
        # sibling branch.  ``"d" * 40`` cannot serve as this tooth -- it is a
        # nonexistent object name, so it goes red on the fail-closed
        # "ancestor check failed" GitError path and the selftest stayed 27/27
        # even when ``is_ancestor`` always returned True (reviewer-6 I-1).
        # The side commit's real time equals the declared time, so (b) and (d)
        # pass and only (a)=False can reject it.
        _commit(tmp, T0, {"base.txt": "x\n"})
        branch = _git(tmp, "rev-parse", "--abbrev-ref", "HEAD").stdout.strip()
        _git(tmp, "checkout", "-q", "-b", "side")
        side_sha = _commit(tmp, T1, {"side.txt": "y\n"})
        _git(tmp, "checkout", "-q", branch)
        doc = ("## 段\n\n快照 截至 %s @ %s\n\n进行中 236\n"
               % (side_sha, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_b = _fixture(_case_b)
    check("⑳ 合法但非祖先的 commit => 仍红（只能由 (a) 拒）",
          res_b["infos"] == [] and len(res_b["findings"]) == 1
          and any("不是 --ref" in note for note in res_b["notes"]))

    def _case_b2(tmp):
        # Kept separately as a fail-closed tooth: a nonexistent object name
        # makes ``is_ancestor`` raise GateError, which must NOT exempt.  It
        # never exercises (a)=False, which is exactly why the two cases split.
        _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("## 段\n\n快照 截至 %s @ %s\n\n进行中 236\n"
               % ("d" * 40, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_b2 = _fixture(_case_b2)
    check("⑳b 不存在的对象名 => 仍红（is_ancestor GateError，fail-closed）",
          res_b2["infos"] == [] and len(res_b2["findings"]) == 1
          and any("祖先判定失败" in note for note in res_b2["notes"]))

    def _case_c(tmp):
        sha1 = _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("## 段\n\n快照 截至 %s @ %s\n\n进行中 236\n"
               % (sha1, T_LATE))
        _commit(tmp, T_LATE, {"docs/a.md": doc})

    res_c = _fixture(_case_c)
    check("㉑ 声明时间与 commit 时间差 > --paragraph-tol => 仍红",
          res_c["infos"] == [] and len(res_c["findings"]) == 1)

    def _case_d(tmp):
        sha1 = _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("## 段\n\n进行中 236\n\n快照 截至 %s @ %s\n"
               % (sha1, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_d = _fixture(_case_d)
    check("㉒ 声明出现在命中之后 => 仍红",
          res_d["infos"] == [] and len(res_d["findings"]) == 1)

    def _case_e(tmp):
        sha1 = _commit(tmp, "2026-10-24T09:00:00+08:00", {"base.txt": "x\n"})
        doc = ("## 段\n\n快照 截至 %s @ %s\n\n进行中 236\n"
               % (sha1, "2026-10-24T09:00:00+08:00"))
        _commit(tmp, T_FORGED, {"docs/a.md": doc})

    res_e = _fixture(_case_e)
    check("㉓ 声明行 3 天后才提交（blame 新近性不符）=> 仍红",
          res_e["infos"] == [] and len(res_e["findings"]) == 1)

    def _case_f(tmp):
        sha1 = _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("## A\n\n快照 截至 %s @ %s\n\n进行中 236\n\n"
               "## B\n\n待审 5\n" % (sha1, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_f = _fixture(_case_f)
    check("㉔ 段外裸计数仍红（声明不赦免全文）",
          len(res_f["infos"]) == 1 and len(res_f["findings"]) == 1
          and res_f["findings"][0]["line"] > 5)

    def _case_g(tmp):
        # 3-space indented ATX heading is a legal CommonMark heading
        # (reviewer-6 M-5 1): missing it lets section A's declaration exempt
        # a bare count in section B (false green).
        sha1 = _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("## A\n\n快照 截至 %s @ %s\n\n进行中 236\n\n"
               "   ## B\n\n待审 5\n" % (sha1, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_g = _fixture(_case_g)
    check("㉕ 3 空格缩进的 ATX 标题也切段（上一节声明不豁免下一节）",
          len(res_g["infos"]) == 1 and len(res_g["findings"]) == 1
          and res_g["findings"][0]["line"] == 9)

    def _case_h(tmp):
        # setext headings (=== / ---) split sections too.
        sha1 = _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("A\n===\n\n快照 截至 %s @ %s\n\n进行中 236\n\n"
               "B\n---\n\n待审 5\n" % (sha1, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_h = _fixture(_case_h)
    check("㉖ setext 标题（===/---）也切段（上一节声明不豁免下一节）",
          len(res_h["infos"]) == 1 and len(res_h["findings"]) == 1
          and res_h["findings"][0]["line"] == 11)

    def _case_i(tmp):
        # A "# comment" inside a fenced block is not a heading; treating it as
        # one used to split the section and turn a legitimate declaration into
        # a false red (reviewer-6 M-5 2).
        sha1 = _commit(tmp, T0, {"base.txt": "x\n"})
        doc = ("## A\n\n快照 截至 %s @ %s\n\n```text\n# 注释\n```\n\n"
               "进行中 236\n" % (sha1, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_i = _fixture(_case_i)
    check("㉗ 围栏内 `# 注释` 不切段（段内声明仍降级，无假红）",
          len(res_i["infos"]) == 1 and res_i["findings"] == [])

    def _case_j(tmp):
        # Two shas on the declaration line, the non-ancestor written first:
        # the old shas[0] code lost the legitimate second sha (reviewer-6 M-1,
        # the skeleton reviewer-3 M-1 shape).
        sha0 = _commit(tmp, T0, {"base.txt": "x\n"})
        branch = _git(tmp, "rev-parse", "--abbrev-ref", "HEAD").stdout.strip()
        _git(tmp, "checkout", "-q", "-b", "side")
        side_sha = _commit(tmp, T1, {"side.txt": "y\n"})
        _git(tmp, "checkout", "-q", branch)
        doc = ("## 段\n\n快照 截至 %s（旧）与 %s（新） @ %s\n\n进行中 236\n"
               % (side_sha, sha0, T1))
        _commit(tmp, T1, {"docs/a.md": doc})

    res_j = _fixture(_case_j)
    check("㉘ 同一行两个 sha：先出现的非祖先不吞掉后面合法的祖先声明 => INFO",
          len(res_j["infos"]) == 1 and res_j["findings"] == []
          and any("不是 --ref" in note for note in res_j["notes"]))

    failed = 0
    for name, ok in checks:
        print(("PASS " if ok else "FAIL ") + name)
        failed += 0 if ok else 1
    print("selftest: %d/%d passed" % (len(checks) - failed, len(checks)))
    return 0 if failed == 0 else 1


def fingerprint_command(top, spec):
    if ":" not in spec:
        raise GateError("dynamic-count gate: --fingerprint needs PATH:LINE", rc=2)
    rel, number = spec.rsplit(":", 1)
    try:
        number = int(number)
    except ValueError:
        raise GateError("dynamic-count gate: --fingerprint line must be a number",
                        rc=2)
    rel = resolve_scan_root(top, rel)
    try:
        with open(os.path.join(top, rel), "rb") as fh:
            lines = fh.read().decode("utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as exc:
        raise GateError("dynamic-count gate: cannot read %s: %s" % (rel, exc), rc=2)
    if number < 1 or number > len(lines):
        raise GateError("dynamic-count gate: %s:%d is out of range"
                        % (rel, number), rc=2)
    print(line_fingerprint(lines[number - 1]))
    return 0


def main(argv):
    parser = argparse.ArgumentParser(
        prog="dynamic_count_stamp_gate.py",
        description="Fail when a drift-prone count in docs/ has no snapshot stamp.")
    parser.add_argument("--path", default=DEFAULT_PATH,
                        help="scan root, repo-relative (default: docs)")
    parser.add_argument("--max-gap", type=int, default=DEFAULT_MAX_GAP,
                        help="max chars between a count and its stamp (default: 24)")
    parser.add_argument("--allowlist", default=DEFAULT_ALLOWLIST,
                        help="allow-list TOML (default: tools/hygiene/"
                             "dynamic_count_allowlist.toml)")
    parser.add_argument("--no-stamp-check", action="store_true",
                        help="skip the stamp check and print 已跳过 (never silent)")
    parser.add_argument("--fingerprint", metavar="PATH:LINE",
                        help="print the line_sha256 for an allow-list entry")
    parser.add_argument("--ref", default="refs/remotes/walgit/master",
                        help="branch/ref the paragraph snapshot declaration must be an "
                             "ancestor of (default: refs/remotes/walgit/master)")
    parser.add_argument("--paragraph-tol", type=int, default=DEFAULT_PARAGRAPH_TOL,
                        help="max |declared time - commit time| in seconds "
                             "(default: %d = 1h)" % DEFAULT_PARAGRAPH_TOL)
    parser.add_argument("--decl-tol", type=int, default=DEFAULT_DECL_TOL,
                        help="max |declaration line blame time - declared time| in "
                             "seconds (default: %d = 1 day)" % DEFAULT_DECL_TOL)
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args(argv)

    if args.selftest:
        return selftest()

    top = repo_root()
    if args.fingerprint:
        return fingerprint_command(top, args.fingerprint)

    rel_path = resolve_scan_root(top, args.path)
    allowlist = load_allowlist(
        args.allowlist if os.path.isabs(args.allowlist)
        else os.path.join(top, args.allowlist))
    if args.no_stamp_check:
        print("stamp-check: 已跳过（--no-stamp-check）——只统计计数，不做戳判定",
              file=sys.stdout)
        return 0
    # An unresolvable ``--ref`` is an environment/config error, not a missing
    # stamp: fail rc=2 before scanning (reviewer-6 M-6; same contract as the
    # skeleton's ``resolve_ref``).  Otherwise every declaration would NOTE
    # 「祖先判定失败」 and the gate would exit 1 as if the docs were red — the
    # default ``refs/remotes/walgit/master`` is absent in a clone that never
    # fetched ``walgit``.
    try:
        ref = skeleton.resolve_ref(top, args.ref)
    except GateError as exc:
        raise GateError("dynamic-count gate: cannot resolve --ref %r (%s)"
                        % (args.ref, exc), rc=2)
    result = scan(top, rel_path, allowlist, args.max_gap, stamp_check=True,
                  ref=ref, paragraph_tol=args.paragraph_tol,
                  decl_tol=args.decl_tol)
    return emit(result, args.path, args.max_gap, args.allowlist)


if __name__ == "__main__":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    except Exception:  # pragma: no cover - very old/odd stdio
        pass
    try:
        sys.exit(main(sys.argv[1:]))
    except GateError as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(exc.rc)
