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


def scan(top, rel_path, allowlist, max_gap, stamp_check=True, files=None):
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
    for rel in files:
        try:
            with open(os.path.join(top, rel), "rb") as fh:
                text = fh.read().decode("utf-8")
        except (OSError, UnicodeDecodeError):
            raise GateError(
                "dynamic-count gate: cannot read %s (fail-closed)" % rel, rc=2)
        for number, line in enumerate(text.splitlines(), 1):
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
            findings.append({
                "path": rel, "line": number, "line_sha": digest,
                "snippet": line.strip()[:160],
                "kinds": sorted({h[2] for h in missing}),
            })
    # Shared rot scope: only entries inside the scanned subtree can rot.
    rotten = [e for e in skeleton.allowlist_in_scope(allowlist, rel_path)
              if not e["used"]]
    return {"findings": findings, "rotten": rotten, "unchecked": unchecked,
            "files": len(files)}


def emit(result, rel_path, max_gap, allowlist_path, err=sys.stderr, out=sys.stdout):
    if result["unchecked"]:
        print("stamp-check: 已跳过（--no-stamp-check）——%d 行计数未检查" %
              len(result["unchecked"]), file=out)
        return 0
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
    print("dynamic-count gate: OK（0 缺戳；扫描 %d 个 markdown，path=%s，max-gap=%d）"
          % (result["files"], rel_path, max_gap), file=out)
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
    result = scan(top, rel_path, allowlist, args.max_gap, stamp_check=True)
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
