#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""stale_pointer_gate — fail when a doc calls an already-merged commit "not merged".

Card: openmir2-docs-stale-pointer-gate (tier=L).  See also
``docs/stale-pointer-gate-20261024.md`` (what this gate does *not* catch).

The gate scans Markdown under ``docs/`` (``--path`` to narrow it) for a
"pointer + unfinished assertion" pair on the **same line and close together**:

  * a commit pointer: 7–40 hex characters (at least one ``a``–``f`` letter, so
    plain numbers such as a date ``20261023`` are not mistaken for a hash);
  * an unfinished word within ``--max-gap`` characters (default 24):
    尚未合入 / 尚未合并 / 未合并 / 未合入 / 未上master / 不在master /
    未进master / 未 merge / 未merge / 未落地 / 待合 / 在办 / 未合 / 未接.

For every pointer it runs ``git merge-base --is-ancestor <hash> <ref>``
(default ``refs/remotes/walgit/master``).  Being an ancestor means the commit is
already in the target branch, so the line claiming otherwise is stale ⇒ red.

Three exemption mechanisms (all mandatory to stay narrow):

  * **quotation** — the unfinished word sits inside 「…」/『…』, i.e. the line is
    quoting someone else's stale wording (the drift-sweep reports tabulate
    "原说法 → 订正").  Quoting is not asserting.
  * **correction marker** — the line itself carries ``〔…订正…〕`` or a dated
    ``YYYY-MM-DD 订正`` marker (the repo's own convention for "this paragraph
    is a historical note; see the correction").  Such a line is opt-in
    self-documented and is not reported.
  * **allow-list** — ``tools/hygiene/stale_pointer_allowlist.toml`` entries keyed
    by ``(path, line_sha256)`` plus a mandatory ``reason``.  A fingerprint is
    ``sha256`` of the line with trailing whitespace stripped.  An entry that
    does not suppress a real hit (i.e. no line matches it, or the line no
    longer produces a hit) is reported as **rot** and fails the gate itself.

Pointers that do not resolve to a commit object in this repository (e.g. a
thread-entry oid, or a hash from another checkout) are reported as
``unresolved`` and are **not** red: the gate cannot judge them.  A short hash
that merely looks like a hash but does not exist is therefore green — one of
the documented blind spots.

Second class — **card status** (card ``openmir2-stale-pointer-card-status-check``).
The hash check above is blind to rebased / force-pushed branches: an old head
is *not* an ancestor even though its content was merged.  This class therefore
judges by *card identity* instead of commit identity: a line that pairs an
``openmir2-*`` card id with a status-style unfinished word (未合入 / 未合 /
在办 / 待开工 / 未开 / …) is looked up in the collaboration ledger.  Red when
the ledger says the card is ``closed`` or has **any** ``merge_result`` with
``merged: true``.

Locality is the card's own criterion: the card id and the word must sit on the
**same line** and be close (``--max-gap``).  A Markdown table row is one line,
so a card id in one cell and the unfinished word in another cell *do* pair.
(The first version required both in the same cell; reviewer-3 showed that this
dropped 4 hits — **2 real + 2 false positives**, not "4 false hits".  The 2 real
ones were ``leader-quota-block-20261023.md:22`` and
``window-round3-runbook-20261021.md:30`` (both written 16 / 40 minutes before
their card merged).  The 2 false positives were the table header 「未合并」
(``goal-gate-full-20261024.md:47``, a statistic-row label) and the direction
word 「未接线」 (``window-criteria-backfill-20261024.md:78``).  So the same-cell
rule was a net loss: it hid 2 real drifts to suppress 2 false ones.)

Two literals that merely *contain* a pending word are not status assertions and
are skipped: 「未接线」/「未接入」 (a subsystem is not wired — a direction word,
not a card lifecycle) and 「含未合并」/「其中未合并」 (a statistic-row category
label).  This is a literal exclusion list, not a narrowing of the word tables.

Third class — **snapshot declarations** (card
``openmir2-stale-pointer-snapshot-exemption``).  A document that declares, in
its *header*, that it was frozen at a commit (``CUTOFF=<epoch>`` /
``截至 <sha> @ <YYYY-MM-DD HH:MM>`` / ``快照 @ <sha>`` / ``取数时点 …``) is
judged under **snapshot semantics**: hits become ``INFO`` lines that print the
declaration and why it was accepted — they are never silently dropped, and a
*rejected* declaration is printed as a ``NOTE`` with the candidate sha and the
failed check.  The declaration is **checked**, not trusted: (a) the declared
sha must exist and be an ancestor of ``--ref``; (b) the declared time (UTC+8
unless an explicit offset / ``Z`` is written) must be within
``--snapshot-tol`` seconds (default 900) of that commit's author or committer
time; (c) the whole declaration must sit *before* the hit; (d) the declaration
lines must themselves have been **committed** within ``--decl-tol`` seconds
(default 86400 = 1 day) of the claimed time, judged by ``git blame
--porcelain``.  (d) is what stops a declaration written months later from
back-dating an arbitrarily old sha.  What (d) cannot catch is a whole file
written and committed in one go (declaration and forgery share one blame
commit); only a human can check that.  Declarations are only recognised in the
document header (before the first ``##`` heading and within the first 60
lines).  Several sha candidates inside that small header window are tried
nearest-first, so an unrelated ``基线 <sha>`` neighbour no longer silently
swallows the real one; every rejected candidate is printed.  Any failed check
leaves the hit red.

A card-shaped id that is **not** in the ledger thread set is a feature name /
label, not a card: it is printed as ``INFO 非账本卡号`` and is not red.  An
unreadable ledger stays fail-closed rc=2 — "cannot look it up" is never "not a
card".

The ledger is read in **one aggregate pass**: ``walgit collab board --format
json`` once (the projection gives every card's status) plus one bulk
``git cat-file --batch`` over ``refs/collab/inbox/*`` (entry bodies carry the
``merge_result`` flag that the board projection does not expose).  No
per-card process.  The card check is skipped only when asked:
``--no-ledger`` prints ``已跳过`` and judges nothing; an unreadable ledger is
**fail-closed rc=2** (distinct from a stale-file rc=1).  A ledger that reads as
**zero threads** is also rc=2 ("cannot see" is not "no problem": a fresh
checkout without ``git fetch … refs/collab/*`` looks exactly like that);
``--allow-empty-ledger`` downgrades it to a loud NOTE.  ``--ledger-json`` reads
a board-shaped snapshot from a file instead of running ``walgit`` (offline
replay / selftest), still merging local snapshot/inbox merge flags.

Usage (run from anywhere inside the repo):

  python -I tools/hygiene/stale_pointer_gate.py                  # scan docs/
  python -I tools/hygiene/stale_pointer_gate.py --path docs/m2   # narrow
  python -I tools/hygiene/stale_pointer_gate.py --ref refs/remotes/walgit/master
  python -I tools/hygiene/stale_pointer_gate.py --no-ledger      # hash check only
  python -I tools/hygiene/stale_pointer_gate.py --allow-empty-ledger  # 0-thread ledger: NOTE
  python -I tools/hygiene/stale_pointer_gate.py --selftest       # synthetic repo
  python -I tools/hygiene/stale_pointer_gate.py --fingerprint docs/foo.md:123

Exit codes: 0 clean (or selftest passed); 1 stale pointer(s) / stale card
status found, allow-list rot, or an unreadable scan target (fail-closed);
2 usage error / not inside a git repository / target ref, allow-list or
ledger unavailable (fail-closed).
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import tomllib

DEFAULT_PATH = "docs"
DEFAULT_REF = "refs/remotes/walgit/master"
DEFAULT_ALLOWLIST = os.path.join("tools", "hygiene", "stale_pointer_allowlist.toml")
DEFAULT_MAX_GAP = 24

# A commit pointer.  Requiring at least one a-f letter keeps dates and decimal
# identifiers out; an all-digit abbreviated hash is a documented blind spot.
HASH_RE = re.compile(r"(?<![0-9a-fA-F])([0-9a-fA-F]{7,40})(?![0-9a-fA-F])")
HAS_HEX_LETTER = re.compile(r"[a-fA-F]")

# Longest alternatives first so "未合并" is not split into "未合" + garbage.
PENDING_WORDS = (
    "尚未合入",
    "尚未合并",
    "未上master",
    "不在master",
    "未进master",
    "未 merge",
    "未merge",
    "未合并",
    "未合入",
    "未落地",
    "待合",
    "在办",
    "未合",
    "未接",
)
PENDING_RE = re.compile("|".join(re.escape(w) for w in PENDING_WORDS))

# ---- class 2: card status -------------------------------------------------
# Card ids.  The lookarounds keep ``openmir2-worker-67``-style principals out
# only when they are glued to other identifier characters; principal-looking
# ids simply miss in the ledger and stay green.  A trailing hyphen is not part
# of an id.
CARD_ID_RE = re.compile(
    r"(?<![0-9A-Za-z_-])(openmir2-[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)(?![0-9A-Za-z_-])"
)

# Status-oriented unfinished words for the *card* check.  This is a separate
# predicate from PENDING_WORDS above ("is this commit wired?" vs "does the doc
# say this card has not started / not merged?").  It is a strict superset of
# the card's own vocabulary (未合入 / 未合 / 在办 / 待开工 / 未开), and it keeps
# 未接 so the card check never *drops* a word the hash check already honours.
CARD_PENDING_WORDS = (
    "尚未合入",
    "尚未合并",
    "未上master",
    "不在master",
    "未进master",
    "未 merge",
    "未merge",
    "未合并",
    "未合入",
    "未落地",
    "待合并",
    "待合",
    "在办",
    "未合",
    "未接",
    "未开始",
    "未开工",
    "待开工",
    "未启动",
    "未开",
    "未认领",
    "待认领",
    "未完成",
    "未关闭",
)
CARD_PENDING_RE = re.compile("|".join(re.escape(w) for w in CARD_PENDING_WORDS))

# Longer literals that *contain* a card-pending word but are not a card-lifecycle
# assertion.  「未接线」= a subsystem is not wired (direction, not a card state);
# 「含未合并」/「其中未合并」/「另未合并」= a statistic-row category label.  The
# third spelling is goal_gate's wording from 2026-10-24 on (detail line
# "window_required 131（另未合并 2）"); it is added here so a verbatim quote of
# that output is not read as a live "the card is still unmerged" claim.  A
# pending-word match lying inside one of these spans is skipped (reviewer-3
# finding I-1 on e5c9f169c: these were the two false reds that the same-cell
# rule used to hide).  Both candidate classes consume the table: the card class
# in ``candidate_cards`` and, since the hash class gained the same filter, the
# pointer class in ``candidate_pointers`` (a statistic label next to a commit
# hash is equally not a live "still unmerged" claim).  A *real* pending word on
# the same line still pairs in both classes (selftest ⑰c / ⑰f pin that).
NOT_ASSERTIVE = ("含未合并", "其中未合并", "另未合并", "未接线", "未接入")

# The repo's two correction-marker spellings: 〔…订正…〕 and "YYYY-MM-DD 订正",
# the latter optionally wrapped in bold.  Both mean "this line is a historical
# note carrying its own correction".
CORRECTION_RE = re.compile(r"〔[^〕]*订正[^〕]*〕|\d{4}-\d{2}-\d{2}\s*\*{0,2}\s*订正")

# Chinese quotation marks.  A pending word inside 「…」/『…』 is someone else's
# *wording being quoted* (the drift-sweep reports literally tabulate
# "原说法 → 订正"), not this line's own assertion, so it is not a hit.
QUOTE_RE = re.compile(r"「[^」]*」|『[^』]*』")

# ---- snapshot declarations -------------------------------------------------
# A document-level freeze marker.  The *anchor* keyword makes a header line a
# candidate; the sha and the time may sit on the same line or within
# SNAPSHOT_NEAR lines of it (``截至 <sha>`` on the title and the timestamp on
# the next metadata line is the common shape).
SNAPSHOT_ANCHOR_RE = re.compile(r"CUTOFF|截至|快照|取数时点|SNAPSHOT", re.IGNORECASE)
SNAPSHOT_EPOCH_RE = re.compile(
    r"(?:CUTOFF|EPOCH)\s*[=:：]?\s*`?(\d{9,11})`?", re.IGNORECASE
)
SNAPSHOT_DATETIME_RE = re.compile(
    r"(?<![0-9])([0-9]{4})-([0-9]{2})-([0-9]{2})[^0-9]{0,6}?([0-9]{2}):([0-9]{2})(?::([0-9]{2}))?"
    r"(?:\s*([+-])([0-9]{2})(?::?([0-9]{2}))?|\s*(?i:(Z|UTC)))?"
)
DEFAULT_SNAPSHOT_TOL = 900  # seconds (--snapshot-tol)
DEFAULT_DECL_TOL = 86400  # seconds (--decl-tol): declaration-line blame gap
SNAPSHOT_NEAR = 2  # lines: anchor <-> sha/time
SNAPSHOT_PREAMBLE_MAX = 60  # declarations must sit in the document header

SNIPPET_LIMIT = 200


class GateError(Exception):
    """Fatal condition: rc=2 (usage/environment), not a stale pointer."""

    def __init__(self, message: str, rc: int = 2):
        super().__init__(message)
        self.rc = rc


# --------------------------------------------------------------------------- git
def _git_text(top, *args):
    proc = subprocess.run(
        ["git", "-C", top, *args], capture_output=True, check=False
    )
    return (
        proc.returncode,
        proc.stdout.decode("utf-8", "replace").strip(),
        proc.stderr.decode("utf-8", "replace").strip(),
    )


def repo_root():
    rc, out, _err = _git_text(".", "rev-parse", "--show-toplevel")
    if rc != 0 or not out:
        raise GateError("stale-pointer gate: not inside a git repository", rc=2)
    return out


def resolve_ref(top, ref):
    rc, out, err = _git_text(top, "rev-parse", "--verify", ref + "^{commit}")
    if rc != 0 or not out:
        raise GateError(
            "stale-pointer gate: cannot resolve --ref %r (%s); fetch the remote "
            "first" % (ref, err or "unknown ref"),
            rc=2,
        )
    return out


def is_commit(top, sha):
    proc = subprocess.run(
        ["git", "-C", top, "cat-file", "-e", sha + "^{commit}"],
        capture_output=True,
        check=False,
    )
    return proc.returncode == 0


def is_ancestor(top, sha, ref):
    """Return True/False; a hard git failure is fail-closed (GateError)."""
    proc = subprocess.run(
        ["git", "-C", top, "merge-base", "--is-ancestor", sha, ref],
        capture_output=True,
        check=False,
    )
    if proc.returncode == 0:
        return True
    if proc.returncode == 1:
        return False
    raise GateError(
        "stale-pointer gate: git merge-base --is-ancestor %s %s failed: %s"
        % (sha, ref, proc.stderr.decode("utf-8", "replace").strip()),
        rc=2,
    )


def repo_relative(top, path):
    absolute = os.path.abspath(path)
    rel = os.path.relpath(absolute, top)
    if rel.startswith(".." + os.sep) or rel == "..":
        raise GateError(
            "stale-pointer gate: --path %r is outside the repository %r"
            % (path, top),
            rc=2,
        )
    return rel.replace(os.sep, "/")


def markdown_files(top, rel_path):
    """Tracked Markdown files under ``rel_path`` (repo-relative)."""
    proc = subprocess.run(
        ["git", "-C", top, "ls-files", "-z", "--", rel_path],
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        raise GateError(
            "stale-pointer gate: git ls-files failed under %r: %s"
            % (rel_path, proc.stderr.decode("utf-8", "replace").strip()),
            rc=2,
        )
    names = [os.fsdecode(p) for p in proc.stdout.split(b"\0") if p]
    return [n for n in names if n.lower().endswith(".md")]


# ----------------------------------------------------------------------- scanning
def line_fingerprint(line):
    return hashlib.sha256(line.rstrip().encode("utf-8")).hexdigest()


def nearest_pending(line, start, end, quote_spans):
    """Return (word, gap) for the nearest *unquoted* unfinished word, or None."""
    best = None
    for match in PENDING_RE.finditer(line):
        if any(match.start() >= a and match.end() <= b for a, b in quote_spans):
            continue
        if match.end() <= start:
            gap = start - match.end()
        elif match.start() >= end:
            gap = match.start() - end
        else:  # overlapping: the word sits inside the pointer token
            gap = 0
        if best is None or gap < best[1]:
            best = (match.group(0), gap)
    return best


def candidate_pointers(line, max_gap):
    """Return [(hash, word, gap)] for pointers close to an unfinished word.

    The hash class consumes the same NOT_ASSERTIVE literal table as the card
    class: a statistic-row label (含未合并 / 其中未合并 / 另未合并 / …) is a
    category, not this line's own "still unmerged" claim, so a pending word
    inside one of those spans is skipped exactly like a quoted word
    (goal_gate's 2026-10-24 comparison table puts a hash and
    "（另未合并 2）" on one line — without this filter that verbatim row is a
    false red).  A *real* pending word (未合入 / 未合并 bare of a label) on the
    same line still pairs and stays red.
    """
    quote_spans = [(m.start(), m.end()) for m in QUOTE_RE.finditer(line)]
    skip_spans = quote_spans + _literal_spans(line, NOT_ASSERTIVE)
    out = []
    for match in HASH_RE.finditer(line):
        token = match.group(1)
        if not HAS_HEX_LETTER.search(token):
            continue
        near = nearest_pending(line, match.start(), match.end(), skip_spans)
        if near is not None and near[1] <= max_gap:
            out.append((token, near[0], near[1]))
    return out


def _literal_spans(line, needles):
    """Every (start, end) span where one of ``needles`` occurs in ``line``."""
    spans = []
    for needle in needles:
        start = line.find(needle)
        while start != -1:
            spans.append((start, start + len(needle)))
            start = line.find(needle, start + 1)
    return spans


def candidate_cards(line, max_gap):
    """Return [(card_id, word, gap)] for card ids close to a status word.

    The card id and the word must be on the same **line** — the card's own
    acceptance criterion.  Table cells are *not* separate records here: a row
    ``| `openmir2-x` | 未合入 |`` is one statement about x.
    """
    quote_spans = [(m.start(), m.end()) for m in QUOTE_RE.finditer(line)]
    noise_spans = _literal_spans(line, NOT_ASSERTIVE)
    words = [
        m
        for m in CARD_PENDING_RE.finditer(line)
        if not any(m.start() >= a and m.end() <= b for a, b in quote_spans)
        and not any(m.start() >= a and m.end() <= b for a, b in noise_spans)
    ]
    if not words:
        return []
    out = []
    for match in CARD_ID_RE.finditer(line):
        best = None
        for word in words:
            if word.end() <= match.start():
                gap = match.start() - word.end()
            elif word.start() >= match.end():
                gap = word.start() - match.end()
            else:  # overlapping: the word sits inside the id token
                gap = 0
            if best is None or gap < best[1]:
                best = (word.group(0), gap)
        if best is not None and best[1] <= max_gap:
            out.append((match.group(1), best[0], best[1]))
    return out


# ------------------------------------------------------- snapshot declarations
def _line_shas(line):
    return [
        match.group(1)
        for match in HASH_RE.finditer(line)
        if HAS_HEX_LETTER.search(match.group(1))
    ]


def _line_times(line):
    """Return [(epoch, text)] for every timestamp on one line."""
    out = []
    for match in SNAPSHOT_DATETIME_RE.finditer(line):
        year, month, day = (int(match.group(index)) for index in (1, 2, 3))
        hour, minute = int(match.group(4)), int(match.group(5))
        second = int(match.group(6) or 0)
        sign, off_h, off_m = match.group(7), match.group(8), match.group(9)
        if sign:
            offset = int(off_h) * 60 + int(off_m or 0)
            if sign == "-":
                offset = -offset
        elif match.group(10):
            offset = 0
        else:
            # Project convention: doc timestamps are local (+08:00) unless an
            # explicit offset (or Z/UTC) is written.
            offset = 480
        try:
            stamp = datetime.datetime(
                year,
                month,
                day,
                hour,
                minute,
                second,
                tzinfo=datetime.timezone(datetime.timedelta(minutes=offset)),
            )
        except ValueError:
            continue
        out.append((int(stamp.timestamp()), match.group(0)))
    for match in SNAPSHOT_EPOCH_RE.finditer(line):
        out.append((int(match.group(1)), match.group(0)))
    return out


def _sha_candidates(lines, low, high, anchor):
    """Candidate ``(line, sha)`` pairs in the header window, nearest first.

    The first version took *one* nearest sha and gave up as soon as it failed
    check (a)/(b).  A neighbouring line above the anchor (``基线 <sha>``) could
    then swallow the real declaration and turn a legitimate snapshot into a
    false red, silently (reviewer-3 M-1).  Now every single-valued line is a
    candidate; the caller tries them nearest-first, the first one passing
    *all* checks wins, and rejected candidates are printed as NOTEs.

    A line carrying two different hashes is ambiguous: if it is the nearest
    sha-bearing line the whole declaration is refused ("歧义即放弃"); farther
    ambiguous lines are merely skipped.
    """
    out = []
    for distance in range(0, high - low + 1):
        for index in (anchor - distance, anchor + distance):
            if not low <= index <= high:
                continue
            values = _line_shas(lines[index - 1])
            if not values:
                continue
            distinct = set(values)
            if len(distinct) != 1:
                if not out:
                    return None  # ambiguous nearest header: refuse the exemption
                continue
            sha = distinct.pop()
            if all(sha != seen for _line, seen in out):
                out.append((index, sha))
    return out


def _nearest_time(lines, low, high, anchor):
    for distance in range(0, high - low + 1):
        for index in (anchor - distance, anchor + distance):
            if not low <= index <= high:
                continue
            values = _line_times(lines[index - 1])
            if not values:
                continue
            distinct = {value[0]: value[1] for value in values}
            if len(distinct) != 1:
                return None
            epoch, text = next(iter(distinct.items()))
            return index, epoch, text
    return None


def snapshot_declarations(text):
    """Document-level snapshot declarations, per the module docstring.

    Only the header is eligible: before the first ``##`` heading **and**
    within ``SNAPSHOT_PREAMBLE_MAX`` lines.  Both bounds are load-bearing: a
    mid-body ``board 快照 <sha> @ <ts>`` is a data point, not a "frozen here"
    declaration, and letting it exempt the whole file would launder a real
    drift (reviewer-3 I-1; selftest ㉗ pins both bounds).  A declaration's
    ``position`` is the last line it occupies, so ``position < hit_line``
    means "the whole declaration was written before the assertion".
    """
    lines = text.splitlines()
    preamble = len(lines) + 1
    for index, line in enumerate(lines, 1):
        if line.startswith("## "):
            preamble = index
            break
    high = min(preamble - 1, SNAPSHOT_PREAMBLE_MAX, len(lines))
    out = []
    seen = set()
    for anchor in range(1, high + 1):
        if not SNAPSHOT_ANCHOR_RE.search(lines[anchor - 1]):
            continue
        low = max(1, anchor - SNAPSHOT_NEAR)
        top = min(high, anchor + SNAPSHOT_NEAR)
        sha_candidates = _sha_candidates(lines, low, top, anchor)
        when = _nearest_time(lines, low, top, anchor)
        if not sha_candidates or when is None:
            continue
        time_line, epoch, time_text = when
        key = (tuple(sha_candidates), epoch)
        if key in seen:
            continue
        seen.add(key)
        snippet_of = {}
        for index in sorted(
            {anchor, time_line} | {line for line, _sha in sha_candidates}
        ):
            snippet = lines[index - 1].strip()
            if len(snippet) > SNIPPET_LIMIT:
                snippet = snippet[: SNIPPET_LIMIT - 1] + "…"
            snippet_of[index] = snippet
        out.append(
            {
                "anchor": anchor,
                "time_line": time_line,
                "time_text": time_text,
                "epoch": epoch,
                "sha_candidates": sha_candidates,
                "sha": sha_candidates[0][1],
                "position": max(anchor, time_line, sha_candidates[0][0]),
                "snippet_of": snippet_of,
            }
        )
    return out


def _commit_time(top, sha, cache):
    if sha in cache:
        return cache[sha]
    rc, out, _err = _git_text(top, "show", "-s", "--format=%at %ct", sha)
    parts = out.split()
    if rc != 0 or len(parts) != 2 or not all(part.isdigit() for part in parts):
        cache[sha] = None
        return None
    cache[sha] = (int(parts[0]), int(parts[1]))
    return cache[sha]


def _blame_delta(top, rel, lines, epoch, cache, reasons=None):
    """``|commit time - declared time|`` for the *newest* declaration line.

    ``git blame --porcelain`` names the commit that last touched a line.  A
    snapshot declaration is only believable when that commit was made near the
    instant the declaration claims to describe: a header added months after
    the fact (or an arbitrarily old sha back-dated with its own real
    timestamp) is exactly the forgery reviewer-3 I-2 demonstrated.  Returns
    ``None`` when a line cannot be blamed (uncommitted working-tree edit,
    unknown file) -- "cannot verify" must never exempt.

    Per line both author and committer time are accepted (a rebase rewrites
    only the latter); the declaration as a whole is only as old as its newest
    line, hence the ``max``.
    """
    worst = 0
    for line in lines:
        key = (rel, line)
        if key not in cache:
            rc, out, _err = _git_text(
                top, "blame", "-L", "%d,%d" % (line, line), "--porcelain", "--", rel
            )
            times = []
            reason = None
            if rc != 0:
                reason = "git blame rc=%d" % rc
            else:
                rows = out.splitlines()
                if rows:
                    sha = rows[0].split()[0] if rows[0].split() else ""
                    if sha == "0" * 40 or not sha:
                        reason = ("blame commit 全 0（该行未提交：Not Committed Yet）")
                if any(row.strip() == "author Not Committed Yet" for row in rows):
                    reason = "blame 输出含 Not Committed Yet（声明行未提交）"
                for raw in rows:
                    if raw.startswith("author-time "):
                        times.append(int(raw.split()[1]))
                    elif raw.startswith("committer-time "):
                        times.append(int(raw.split()[1]))
                if reason is None and not times:
                    reason = "解析不出 author/committer 时间"
            # Card openmir2-stale-pointer-blame-failclosed: uncommitted / all-zero
            # blame must be **unusable** (never exempt), and the reason is kept for
            # the caller's NOTE instead of a bare "unavailable".
            cache[key] = (tuple(times), reason) if not reason else (None, reason)
        times, reason = cache[key]
        if not times:
            if reasons is not None and reason:
                reasons.append(reason)
            return None
        worst = max(worst, min(abs(epoch - stamp) for stamp in times))
    return worst


def checked_declarations(
    top,
    rel,
    text,
    tolerance,
    decl_tolerance,
    ancestor,
    commit_cache,
    blame_cache,
    reject_sink=None,
):
    """Keep only declarations that pass (a), (b) and (d) of the docstring.

    (c) "declaration before the hit" is enforced per hit line by the caller.
    Every rejected declaration is appended to ``reject_sink`` (printed as a
    NOTE) instead of being dropped silently.
    """
    out = []
    for decl in snapshot_declarations(text):
        rejected = []
        accepted = None
        for sha_line, sha_text in decl["sha_candidates"]:
            state = ancestor(sha_text)
            if state is None:
                rejected.append(
                    "L%d %s: not a commit in this repository" % (sha_line, sha_text)
                )
                continue
            if not state:
                rejected.append(
                    "L%d %s: not an ancestor of --ref" % (sha_line, sha_text)
                )
                continue
            times = _commit_time(top, sha_text, commit_cache)
            if times is None:
                rejected.append(
                    "L%d %s: cannot read its commit time" % (sha_line, sha_text)
                )
                continue
            delta = min(abs(decl["epoch"] - times[0]), abs(decl["epoch"] - times[1]))
            if delta > tolerance:
                rejected.append(
                    "L%d %s: Δ%ds > --snapshot-tol %ds"
                    % (sha_line, sha_text, delta, tolerance)
                )
                continue
            accepted = (sha_line, sha_text, times, delta)
            break
        if accepted is None:
            if reject_sink is not None:
                reject_sink.append(
                    {
                        "path": rel,
                        "line": decl["anchor"],
                        "sha": decl["sha"],
                        "time": decl["time_text"],
                        "reasons": rejected or ["no sha in the header window"],
                    }
                )
            continue
        sha_line, sha_text, times, delta = accepted
        lines = sorted({decl["anchor"], decl["time_line"], sha_line})
        blame_reasons = []
        blamed = _blame_delta(top, rel, lines, decl["epoch"], blame_cache,
                              reasons=blame_reasons)
        if blamed is None:
            if reject_sink is not None:
                reject_sink.append(
                    {
                        "path": rel,
                        "line": decl["anchor"],
                        "sha": sha_text,
                        "time": decl["time_text"],
                        "reasons": blame_reasons or [
                            "declaration line is not committed (blame unavailable)"
                        ],
                    }
                )
            continue
        if blamed > decl_tolerance:
            if reject_sink is not None:
                reject_sink.append(
                    {
                        "path": rel,
                        "line": decl["anchor"],
                        "sha": sha_text,
                        "time": decl["time_text"],
                        "reasons": [
                            "declaration written Δ%ds from the claimed time > "
                            "--decl-tol %ds (added after the fact?)"
                            % (blamed, decl_tolerance)
                        ],
                    }
                )
            continue
        checked = dict(decl)
        checked.update(
            {
                "sha": sha_text,
                "sha_line": sha_line,
                "position": max(decl["anchor"], decl["time_line"], sha_line),
                "delta": delta,
                "commit_times": times,
                "blame_delta": blamed,
                "lines": lines,
                "rejected_candidates": rejected,
                "text": " / ".join(
                    "L%d: %s" % (index, decl["snippet_of"][index]) for index in lines
                ),
            }
        )
        out.append(checked)
    return out


# ------------------------------------------------------------------- card ledger
class Ledger:
    """Read-only snapshot of the collaboration ledger.

    ``cards`` maps a thread id to ``{"status", "merged", "on_board",
    "entries"}``.  ``merged`` is True when *any* ``merge_result`` entry of the
    thread says ``merged: true`` — the acceptance criterion is existential, so
    a later ``merged: false`` does not erase an earlier merge.
    """

    def __init__(self, cards, source="ledger"):
        self.cards = cards
        self.source = source

    def state(self, card_id):
        return self.cards.get(card_id)

    def __len__(self):
        return len(self.cards)


def _merged_flag(value):
    """True only for the boolean ``true`` of ``merge_result.body.merged``.

    Every observable value is the boolean ``true`` (800 entries) or ``null``
    (1 entry); no entry stores a string.  The old "a string oid means merged"
    branch was dead code — reviewer-3 on e5c9f169c — so the contract is now the
    single testable rule below (selftest ⑲).
    """
    return value is True


def _cards_from_raw(raw):
    """Flatten a board-shaped JSON document into a list of card dicts."""
    if isinstance(raw, dict):
        if isinstance(raw.get("columns"), list):
            cards = []
            for column in raw["columns"]:
                if isinstance(column, dict) and isinstance(column.get("cards"), list):
                    cards.extend(c for c in column["cards"] if isinstance(c, dict))
            return cards
        if isinstance(raw.get("cards"), list):
            return [c for c in raw["cards"] if isinstance(c, dict)]
        if isinstance(raw.get("threads"), dict):
            out = []
            for thread_id, info in raw["threads"].items():
                if isinstance(info, dict):
                    card = dict(info)
                    card.setdefault("id", thread_id)
                else:
                    card = {"id": thread_id, "status": info}
                out.append(card)
            return out
    raise GateError(
        "stale-pointer gate: ledger JSON has no `columns`, `cards` or `threads` "
        "(fail-closed)",
        rc=2,
    )


def _card_record(card):
    merged = card.get("merge_result") if "merge_result" in card else card.get("merged")
    return {
        "status": card.get("status"),
        "merged": _merged_flag(merged),
        "on_board": bool(card.get("on_board", True)),
        "entries": card.get("entries"),
    }


def load_board_file(path):
    """Read a board-shaped ledger snapshot for ``--ledger-json``."""
    try:
        with open(path, "rb") as fh:
            return json.load(fh)
    except FileNotFoundError:
        raise GateError("stale-pointer gate: --ledger-json not found: %s" % path, rc=2)
    except (OSError, ValueError) as exc:
        raise GateError(
            "stale-pointer gate: cannot read --ledger-json %s: %s (fail-closed)"
            % (path, exc),
            rc=2,
        )


def _run_board(top):
    """One ``walgit collab board --format json`` call (never per card)."""
    try:
        proc = subprocess.run(
            ["walgit", "collab", "board", "--format", "json", "--repo", top],
            capture_output=True,
            check=False,
        )
    except FileNotFoundError:
        raise GateError(
            "stale-pointer gate: `walgit` not found on PATH; the card-status check "
            "needs the collaboration ledger (fail-closed). Use --no-ledger to skip it.",
            rc=2,
        )
    if proc.returncode != 0:
        raise GateError(
            "stale-pointer gate: `walgit collab board --format json` failed (rc=%d): %s "
            "(fail-closed; use --no-ledger to skip the card-status check)"
            % (
                proc.returncode,
                proc.stderr.decode("utf-8", "replace").strip()[:400] or "no stderr",
            ),
            rc=2,
        )
    try:
        return json.loads(proc.stdout.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        raise GateError(
            "stale-pointer gate: board JSON is not valid JSON: %s (fail-closed)" % exc,
            rc=2,
        )


def _fold_entries(entries):
    """Fold entry dicts into ``{thread_id: {status, merged, entries}}``."""
    status_at = {}
    merged_at = {}
    counts = {}
    for entry in entries:
        if not isinstance(entry, dict):
            continue
        thread_id = entry.get("id")
        if not isinstance(thread_id, str) or not thread_id:
            continue
        counts[thread_id] = counts.get(thread_id, 0) + 1
        ts = entry.get("ts") if isinstance(entry.get("ts"), int) else 0
        body = entry.get("body") if isinstance(entry.get("body"), dict) else {}
        if entry.get("kind") == "status":
            value = body.get("status")
            if isinstance(value, str) and (
                thread_id not in status_at or ts >= status_at[thread_id][0]
            ):
                status_at[thread_id] = (ts, value)
        elif entry.get("kind") == "merge_result" and _merged_flag(body.get("merged")):
            if thread_id not in merged_at or ts >= merged_at[thread_id][0]:
                merged_at[thread_id] = (ts, True)
    return {
        thread_id: {
            "status": status_at.get(thread_id, (0, None))[1],
            "merged": thread_id in merged_at,
            "entries": count,
        }
        for thread_id, count in counts.items()
    }


def _snapshot_entries(top):
    """Read the folded ``refs/collab/meta/snapshot`` blob, if any.

    Format (``walgit collab gc``): ``{"version":1,"kind":"collab_snapshot",
    "ts":…,"entries":[{"oid","principal","json":"<entry JSON string>"}]}``.
    """
    rc, out, _err = _git_text(top, "rev-parse", "--verify", "refs/collab/meta/snapshot")
    if rc != 0 or not out:
        return [], set()
    proc = subprocess.run(
        ["git", "-C", top, "cat-file", "blob", out],
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        raise GateError(
            "stale-pointer gate: cannot read refs/collab/meta/snapshot: %s "
            "(fail-closed)"
            % (proc.stderr.decode("utf-8", "replace").strip() or "git failed"),
            rc=2,
        )
    try:
        raw = json.loads(proc.stdout.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        raise GateError(
            "stale-pointer gate: refs/collab/meta/snapshot is not valid JSON: %s "
            "(fail-closed)" % exc,
            rc=2,
        )
    entries = []
    seen = set()
    for item in raw.get("entries", []) if isinstance(raw, dict) else []:
        if not isinstance(item, dict):
            continue
        oid = item.get("oid")
        if isinstance(oid, str) and oid:
            if oid in seen:
                continue
            seen.add(oid)
        payload = item.get("json")
        if isinstance(payload, str):
            try:
                entry = json.loads(payload)
            except ValueError:
                continue
        elif isinstance(payload, dict):
            entry = payload
        else:
            continue
        entries.append(entry)
    return entries, seen


def _inbox_entries(top, skip_oids):
    """One aggregate ``git cat-file --batch`` read of the inbox tail."""
    proc = subprocess.run(
        ["git", "-C", top, "for-each-ref", "--format=%(objectname)", "refs/collab/inbox"],
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        raise GateError(
            "stale-pointer gate: cannot list refs/collab/inbox: %s (fail-closed)"
            % (proc.stderr.decode("utf-8", "replace").strip() or "git failed"),
            rc=2,
        )
    oids = [oid for oid in proc.stdout.split() if oid and oid.decode() not in skip_oids]
    if not oids:
        return [], 0
    proc = subprocess.run(
        ["git", "-C", top, "cat-file", "--batch"],
        input=b"\n".join(oids) + b"\n",
        capture_output=True,
        check=False,
    )
    if proc.returncode != 0:
        raise GateError(
            "stale-pointer gate: cannot read collab entries: %s (fail-closed)"
            % (proc.stderr.decode("utf-8", "replace").strip() or "git failed"),
            rc=2,
        )
    data = proc.stdout
    pos = 0
    entries = []
    while pos < len(data):
        newline = data.find(b"\n", pos)
        if newline < 0:
            break
        header = data[pos:newline].decode("utf-8", "replace").split()
        pos = newline + 1
        if len(header) != 3 or not header[2].isdigit():
            continue
        size = int(header[2])
        blob = data[pos : pos + size]
        pos += size + 1
        try:
            entry = json.loads(blob.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            continue
        if isinstance(entry, dict):
            entries.append(entry)
    return entries, len(oids)


def read_ledger(top, board_raw=None):
    """One board read + one snapshot read + one inbox tail read (never per card).

    ``board_raw`` overrides the ``walgit collab board`` projection
    (``--ledger-json``); the git-side snapshot/inbox read still runs so merge
    flags are available whenever the refs are present.
    """
    snapshot, seen = _snapshot_entries(top)
    tail_entries, ref_count = _inbox_entries(top, seen)
    entries = snapshot + tail_entries
    live = board_raw is None
    if live:
        board_raw = _run_board(top)

    cards = {}
    for card in _cards_from_raw(board_raw):
        card_id = card.get("id")
        if isinstance(card_id, str) and card_id:
            cards[card_id] = _card_record(card)

    fold = _fold_entries(entries)
    if not fold and cards and live:
        raise GateError(
            "stale-pointer gate: no collaboration entries are readable while the "
            "board lists cards; fetch refs/collab/* first (fail-closed)",
            rc=2,
        )
    for card_id, record in fold.items():
        if card_id in cards:
            cards[card_id]["merged"] = cards[card_id]["merged"] or record["merged"]
            if cards[card_id]["entries"] is None:
                cards[card_id]["entries"] = record["entries"]
        else:
            # The board projection omits some threads (e.g. status-less or
            # view-stale ones); the raw ledger still knows them.
            cards[card_id] = {
                "status": record["status"],
                "merged": record["merged"],
                "on_board": False,
                "entries": record["entries"],
            }
    return Ledger(cards, source="board+snapshot+refs" if live else "file+snapshot+refs")


def card_state(ledger, card_id):
    """Return ``(stale, label)`` for a card, or None when it cannot be judged."""
    if ledger is None:
        return None
    record = ledger.state(card_id)
    if record is None:
        return None  # not a thread id (e.g. a principal name) — nothing to judge
    status = record.get("status")
    if status == "closed":
        return True, "closed"
    if record.get("merged") is True:
        return True, "merged (status=%s)" % (status or "unknown")
    return False, status or "open"


def load_allowlist(path, require_added=False, prog="stale-pointer gate"):
    """Load and validate the allow-list.

    ``require_added`` additionally demands an ``added`` date (``YYYY-MM-DD``) on
    every entry; the dynamic-count gate needs dated exemptions, while the
    stale-pointer allow-list predates that rule.  ``prog`` only prefixes the
    error text.  Both default to the historical behaviour, so existing callers
    (and this gate's own messages) are unchanged.
    """

    def fail(message):
        raise GateError("%s: %s" % (prog, message), rc=2)

    try:
        with open(path, "rb") as fh:
            data = tomllib.load(fh)
    except FileNotFoundError:
        fail("allow-list not found: %s (fail-closed)" % path)
    except (OSError, tomllib.TOMLDecodeError) as exc:
        fail("cannot read allow-list %s: %s" % (path, exc))
    entries = data.get("allow", [])
    if not isinstance(entries, list):
        fail("allow-list %s: `allow` must be an array of tables" % path)
    seen = {}
    checked = []
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            fail("allow-list %s: entry #%d is not a table" % (path, index + 1))
        rel = entry.get("path")
        fingerprint = entry.get("line_sha256")
        reason = entry.get("reason")
        added = entry.get("added")
        if not isinstance(rel, str) or not rel:
            fail(
                "allow-list %s: entry #%d lacks a non-empty `path`"
                % (path, index + 1)
            )
        if (
            not isinstance(fingerprint, str)
            or len(fingerprint) != 64
            or any(c not in "0123456789abcdef" for c in fingerprint)
        ):
            fail(
                "allow-list %s: entry #%d (%s) has no valid 64-hex `line_sha256`"
                % (path, index + 1, rel)
            )
        if not isinstance(reason, str) or not reason.strip():
            fail(
                "allow-list %s: entry #%d (%s) lacks a `reason` — every exemption "
                "must say why" % (path, index + 1, rel)
            )
        if require_added and (
            not isinstance(added, str)
            or not re.match(r"^20\d{2}-\d{2}-\d{2}$", added)
        ):
            fail(
                "allow-list %s: entry #%d (%s) lacks `added` (YYYY-MM-DD) — "
                "exemptions must be dated" % (path, index + 1, rel)
            )
        key = (rel.replace("\\", "/"), fingerprint)
        if key in seen:
            fail("allow-list %s: duplicate entry for %s" % (path, rel))
        seen[key] = index
        record = {
            "path": key[0],
            "line_sha256": fingerprint,
            "reason": reason.strip(),
            "index": index + 1,
            "used": False,
        }
        if require_added:
            record["added"] = added
        checked.append(record)
    return checked


def allowlist_in_scope(allowlist, rel_path):
    """Entries a scan of ``rel_path`` is responsible for keeping alive.

    Rot detection only applies to entries inside the scanned subtree — a
    narrowed ``--path`` must not flag allow-list entries that were simply not
    looked at.  Scanning the repo root yields ``rel_path`` "." (or ""), which
    matches no entry prefix; normalize it so root scans keep checking every
    entry (reviewer-3 finding on eeb8d064f: rot was silently off for ``--path .``).
    """
    prefix = rel_path.rstrip("/")
    if prefix in ("", "."):
        return list(allowlist)
    return [
        e
        for e in allowlist
        if e["path"] == prefix or e["path"].startswith(prefix + "/")
    ]


def scan(
    top,
    rel_path,
    ref,
    allowlist,
    max_gap,
    ledger=None,
    snapshot_tol=DEFAULT_SNAPSHOT_TOL,
    decl_tol=DEFAULT_DECL_TOL,
):
    """Scan ``rel_path``; return a dict with hits/exemptions/errors.

    ``ledger`` is a :class:`Ledger` (card-status check enabled) or ``None``
    (``--no-ledger``: the card-status class is skipped and said so).
    """
    files = markdown_files(top, rel_path)
    allow_index = {(e["path"], e["line_sha256"]): e for e in allowlist}
    result = {
        "files": len(files),
        "lines": 0,
        "hits": [],
        "card_hits": [],
        "unresolved": [],
        "marker_exempt": 0,
        "allow_exempt": 0,
        "green_pointers": 0,
        "card_green": 0,
        "card_lines": 0,
        "ledger_skipped": ledger is None,
        "errors": [],
        "allowlist": allowlist,
        "snapshot_info": [],
        "snapshot_exempt": 0,
        "snapshot_tol": snapshot_tol,
        "snapshot_decl_tol": decl_tol,
        "decl_notes": [],
        "card_nonid": [],
    }
    ancestry = {}
    commit_times = {}
    blame_times = {}

    def ancestor(sha):
        if sha not in ancestry:
            if not is_commit(top, sha):
                ancestry[sha] = None
            else:
                ancestry[sha] = is_ancestor(top, sha, ref)
        return ancestry[sha]

    for rel in files:
        path = os.path.join(top, rel)
        try:
            with open(path, "rb") as fh:
                text = fh.read().decode("utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            result["errors"].append("cannot read %s: %s" % (rel, exc))
            continue
        declarations = checked_declarations(
            top,
            rel,
            text,
            snapshot_tol,
            decl_tol,
            ancestor,
            commit_times,
            blame_times,
            reject_sink=result["decl_notes"],
        )
        for lineno, line in enumerate(text.splitlines(), 1):
            result["lines"] += 1
            candidates = candidate_pointers(line, max_gap)
            cards = candidate_cards(line, max_gap)
            if not candidates and not cards:
                continue
            if CORRECTION_RE.search(line):
                result["marker_exempt"] += 1
                continue
            fingerprint = line_fingerprint(line)
            snippet = line.strip()
            if len(snippet) > SNIPPET_LIMIT:
                snippet = snippet[: SNIPPET_LIMIT - 1] + "…"
            resolved = [(token, word, ancestor(token)) for token, word, _gap in candidates]
            stale_hash = any(state for _t, _w, state in resolved)
            card_red = []
            if cards:
                result["card_lines"] += 1
                seen = set()
                for card_id, _word, _gap in cards:
                    if card_id in seen:
                        continue
                    seen.add(card_id)
                    if ledger is None:
                        continue
                    verdict = card_state(ledger, card_id)
                    if verdict is None:
                        # Not a thread id in the ledger (a feature name, label or
                        # principal): nothing to judge, which is green — but say
                        # so, so "not a card" is visible rather than silent.
                        result["card_nonid"].append(
                            {"path": rel, "line": lineno, "card": card_id}
                        )
                        result["card_green"] += 1
                    elif verdict[0]:
                        card_red.append((card_id, verdict[1]))
                    else:
                        result["card_green"] += 1
            # An allow-list entry may only suppress a line that would actually be
            # reported.  With --no-ledger a card line cannot be judged, but it
            # still has to keep its entry alive: otherwise a legitimate historical
            # exemption would be reported as rot merely because the ledger was off.
            reportable = stale_hash or bool(card_red) or (ledger is None and bool(cards))
            if not reportable:
                for token, _word, state in resolved:
                    if state is None:
                        result["unresolved"].append(
                            {"path": rel, "line": lineno, "hash": token, "snippet": snippet}
                        )
                    else:
                        result["green_pointers"] += 1
                continue
            entry = allow_index.get((rel, fingerprint))
            if entry is not None:
                entry["used"] = True
                result["allow_exempt"] += 1
                continue
            decl = None
            for candidate_decl in reversed(declarations):
                if candidate_decl["position"] < lineno:
                    decl = candidate_decl
                    break
            if decl is not None:
                # A checked snapshot declaration only *downgrades* hits to INFO;
                # it never hides them.  (c) is the ``position < lineno`` test.
                for token, word, state in resolved:
                    if state is None:
                        result["unresolved"].append(
                            {"path": rel, "line": lineno, "hash": token, "snippet": snippet}
                        )
                    elif state:
                        result["snapshot_info"].append(
                            {
                                "path": rel,
                                "line": lineno,
                                "kind": "hash",
                                "name": token,
                                "word": word,
                                "decl": decl,
                            }
                        )
                for card_id, label in card_red:
                    result["snapshot_info"].append(
                        {
                            "path": rel,
                            "line": lineno,
                            "kind": "card",
                            "name": card_id,
                            "word": label,
                            "decl": decl,
                        }
                    )
                result["snapshot_exempt"] += 1
                continue
            for token, word, state in resolved:
                if state is None:
                    result["unresolved"].append(
                        {"path": rel, "line": lineno, "hash": token, "snippet": snippet}
                    )
                elif state:
                    result["hits"].append(
                        {
                            "path": rel,
                            "line": lineno,
                            "hash": token,
                            "word": word,
                            "snippet": snippet,
                        }
                    )
                else:
                    result["green_pointers"] += 1
            for card_id, label in card_red:
                result["card_hits"].append(
                    {
                        "path": rel,
                        "line": lineno,
                        "card": card_id,
                        "state": label,
                        "snippet": snippet,
                    }
                )

    # Rot detection only applies to entries inside the scanned subtree; the
    # scoping rule lives in ``allowlist_in_scope`` so the dynamic-count gate
    # reuses it instead of re-deriving it (and getting it wrong).
    result["rot"] = [
        e for e in allowlist_in_scope(allowlist, rel_path) if not e["used"]
    ]
    result["rc"] = 1 if (result["hits"] or result["card_hits"] or result["errors"] or result["rot"]) else 0
    return result


# -------------------------------------------------------------------------- output
def emit(result, ref, allowlist_path, out=sys.stdout, err=sys.stderr):
    if result["ledger_skipped"]:
        print(
            "NOTE card-status check SKIPPED 已跳过（--no-ledger）："
            "卡号 + 未完成词 → 账本状态 未判定",
            file=err,
        )
    for hit in result["hits"]:
        print(
            "%s:%d  %s  %s" % (hit["path"], hit["line"], hit["hash"], hit["snippet"]),
            file=out,
        )
    for hit in result["card_hits"]:
        # Contract output: file:line  卡号  账本状态
        print("%s:%d  %s  %s" % (hit["path"], hit["line"], hit["card"], hit["state"]), file=out)
    for item in result["card_nonid"]:
        print(
            "INFO 非账本卡号（按特性名/标签放行） %s:%d  %s"
            % (item["path"], item["line"], item["card"]),
            file=out,
        )
    for item in result["snapshot_info"]:
        decl = item["decl"]
        override = (
            "; nearer candidate(s) rejected: " + " | ".join(decl["rejected_candidates"])
            if decl["rejected_candidates"]
            else ""
        )
        print(
            "INFO snapshot-exempt %s:%d  %s %s  (declaration %s; sha %s @ %s "
            "(epoch %d), commit %s, Δ%ds ≤ %ds, ancestor of ref; declaration "
            "committed Δ%ds ≤ %ds%s)"
            % (
                item["path"],
                item["line"],
                item["kind"],
                item["name"],
                decl["text"],
                decl["sha"],
                decl["time_text"],
                decl["epoch"],
                "%d/%d" % decl["commit_times"],
                decl["delta"],
                result["snapshot_tol"],
                decl["blame_delta"],
                result["snapshot_decl_tol"],
                override,
            ),
            file=out,
        )
    for note in result["decl_notes"]:
        print(
            "NOTE snapshot-declaration rejected %s:L%d  %s @ %s  (%s)"
            % (
                note["path"],
                note["line"],
                note["sha"],
                note["time"],
                "; ".join(note["reasons"]),
            ),
            file=err,
        )
    for item in result["unresolved"]:
        print(
            "NOTE unresolved %s:%d  %s (not a commit in this repository; not judged)"
            % (item["path"], item["line"], item["hash"]),
            file=err,
        )
    for message in result["errors"]:
        print("ERROR " + message, file=err)
    for entry in result["rot"]:
        print(
            "ERROR allow-list rot: %s (entry #%d) matches no line that this gate "
            "would otherwise report — drop it or re-fingerprint: %s"
            % (entry["path"], entry["index"], entry["reason"]),
            file=err,
        )
    if result["rc"] == 0:
        print(
            "stale-pointer gate: OK (%d files / %d lines scanned under docs; "
            "%d green pointer(s), %d correction-marker line(s), %d allow-listed "
            "line(s), %d snapshot-exempt line(s), %d unresolved; %d card-status "
            "hit(s), %d card id(s) judged, %d card-bearing line(s)%s; ref=%s, "
            "allow-list=%s)"
            % (
                result["files"],
                result["lines"],
                result["green_pointers"],
                result["marker_exempt"],
                result["allow_exempt"],
                result["snapshot_exempt"],
                len(result["unresolved"]),
                len(result["card_hits"]),
                result["card_green"],
                result["card_lines"],
                "" if not result["ledger_skipped"] else ", card-status check SKIPPED (--no-ledger)",
                ref,
                allowlist_path,
            ),
            file=out,
        )
    else:
        print(
            "stale-pointer gate: FAIL (%d stale pointer(s), %d card-status hit(s), "
            "%d allow-list rot, %d error(s); ref=%s)"
            % (
                len(result["hits"]),
                len(result["card_hits"]),
                len(result["rot"]),
                len(result["errors"]),
                ref,
            ),
            file=err,
        )


# ------------------------------------------------------------------------- selftest
def _run_git(cwd, *args):
    proc = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, check=False, text=True
    )
    if proc.returncode != 0:
        raise RuntimeError(
            "selftest git %s failed: %s" % (" ".join(args), proc.stderr.strip())
        )
    return proc.stdout.strip()


def _run_git_env(cwd, env, *args):
    full = dict(os.environ)
    full.update(env)
    proc = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, check=False, text=True, env=full
    )
    if proc.returncode != 0:
        raise RuntimeError(
            "selftest git %s failed: %s" % (" ".join(args), proc.stderr.strip())
        )
    return proc.stdout.strip()


def _write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)


def selftest():
    checks = []

    def check(name, ok):
        checks.append((name, bool(ok)))

    with tempfile.TemporaryDirectory(prefix="stale-pointer-gate-") as tmp:
        repo = os.path.join(tmp, "repo")
        os.makedirs(repo)
        _run_git(repo, "init", "-q", "-b", "main")
        _run_git(repo, "config", "user.email", "selftest@example.invalid")
        _run_git(repo, "config", "user.name", "selftest")

        # Base commit A is the "already merged" pointer.
        _write(os.path.join(repo, "docs", "plain", "seed.md"), "# seed\n")
        _run_git(repo, "add", "-A")
        _run_git(repo, "commit", "-qm", "base")
        merged = _run_git(repo, "rev-parse", "HEAD")

        # An old ancestor dated 2026-01-01: quoting it with its *own real*
        # timestamp passes checks (a)/(b)/(c) -- only the declaration-line
        # blame check (d) can tell that the declaration was written today.
        _write(os.path.join(repo, "docs", "plain", "old.md"), "old\n")
        _run_git(repo, "add", "-A")
        _run_git_env(
            repo,
            {
                "GIT_AUTHOR_DATE": "2026-01-01T12:00:00+0800",
                "GIT_COMMITTER_DATE": "2026-01-01T12:00:00+0800",
            },
            "commit",
            "-qm",
            "old",
        )
        old = _run_git(repo, "rev-parse", "HEAD")

        # Side-branch commit B is reachable in the object DB but not an ancestor.
        _run_git(repo, "checkout", "-qb", "side")
        _write(os.path.join(repo, "docs", "plain", "side.md"), "side\n")
        _run_git(repo, "add", "-A")
        _run_git(repo, "commit", "-qm", "side")
        pending = _run_git(repo, "rev-parse", "HEAD")
        _run_git(repo, "checkout", "-q", "main")

        _write(
            os.path.join(repo, "docs", "red", "stale.md"),
            "分支 head `%s`，**未合入**\n" % merged,
        )
        _write(
            os.path.join(repo, "docs", "green", "pending.md"),
            "分支 head `%s`，**未合入**\n" % pending,
        )
        _write(
            os.path.join(repo, "docs", "marker", "marked.md"),
            "分支 head `%s`，**未合入** 〔2026-10-24 订正：已合入 %s〕\n"
            % (merged, merged),
        )
        allowed_path = os.path.join(repo, "docs", "allow", "allowed.md")
        allowed_line = "分支 head `%s`，**未合入**\n" % merged
        _write(allowed_path, allowed_line)
        _write(
            os.path.join(repo, "docs", "unknown", "unknown.md"),
            "分支 head `deadbeef`，**未合入**\n",
        )
        _write(
            os.path.join(repo, "docs", "quote", "quoted.md"),
            "原说法：「head `%s`，**未合入**」\n" % merged,
        )
        # Class-2 fixtures: an openmir2 card id next to a status word.
        _write(
            os.path.join(repo, "docs", "cardclosed", "closed.md"),
            "卡 `openmir2-synthetic-closed` 未合入\n",
        )
        _write(
            os.path.join(repo, "docs", "cardopen", "open.md"),
            "卡 `openmir2-synthetic-open` 未开始\n",
        )
        _write(
            os.path.join(repo, "docs", "cardmerged", "merged.md"),
            "卡 `openmir2-synthetic-merged` 待开工\n",
        )
        _write(
            os.path.join(repo, "docs", "cardquoted", "quoted.md"),
            "原说法：「卡 `openmir2-synthetic-closed` 未合入」\n",
        )
        _write(
            os.path.join(repo, "docs", "cardtable", "row.md"),
            "| `openmir2-synthetic-closed` | 未合入 |\n",
        )
        # A compound that merely *contains* a pending word: a direction word
        # inside a table cell (未接线) and a statistic-row category label
        # (含未合并).  Neither is an assertion about the card on the line.
        _write(
            os.path.join(repo, "docs", "cardnoise", "noise.md"),
            "| `openmir2-synthetic-closed` | 前置被阻塞：⑧ 攻城层未接线 |\n"
            "| window_required（含未合并） | 2 | `openmir2-synthetic-closed` |\n",
        )
        # goal_gate's 2026-10-24 wording 「另未合并」: a verbatim statistic line
        # must not pair as a card assertion; the same line with a real pending
        # word must still be red; and the hash class applies the same filter
        # (selftest ⑰b-⑰f).
        _write(
            os.path.join(repo, "docs", "cardspan", "span.md"),
            "| `openmir2-synthetic-closed` | 未接线，且未合并 |\n",
        )
        _write(
            os.path.join(repo, "docs", "newword", "new.md"),
            "| `openmir2-synthetic-closed`（另未合并 2） |\n",
        )
        _write(
            os.path.join(repo, "docs", "newwordred", "red.md"),
            "| `openmir2-synthetic-closed`（未合入） |\n",
        )
        _write(
            os.path.join(repo, "docs", "newwordhash", "hash.md"),
            "| window_required 131（另未合并 2） | `%s` |\n" % merged,
        )
        # Same shape with the older label spelling: hash + （含未合并 2） is a
        # statistic row, not a stale-pointer assertion (reviewer finding: the
        # hash class must consume NOT_ASSERTIVE like the card class).
        _write(
            os.path.join(repo, "docs", "newwordhashstat", "hash.md"),
            "| window_required 131（含未合并 2） | `%s` |\n" % merged,
        )
        # The boundary that keeps the filter honest: a hash next to a *real*
        # pending word on the same line is still a stale pointer.
        _write(
            os.path.join(repo, "docs", "newwordhashred", "hash.md"),
            "| 窗口未合入 2 | `%s` |\n" % merged,
        )
        # Span-level, never whole-line (hash-class twin of ㉜): a label on the
        # line does not veto a *real* pending word further out on the same line.
        _write(
            os.path.join(repo, "docs", "newwordhashspan", "hash.md"),
            "| 窗口未合并 2（其中未合并 1） | `%s` |\n" % merged,
        )
        # A card the board projection does not list: only snapshot/inbox know it.
        _write(
            os.path.join(repo, "docs", "cardoffboard", "offboard.md"),
            "卡 `openmir2-synthetic-offboard` 未合入\n",
        )
        # Snapshot-declaration fixtures.  ``merged`` is an ancestor of the ref,
        # so a declaration naming it with its own timestamp must downgrade hits
        # to INFO; the three broken variants must stay red.
        def _stamp(rev, delta=0):
            epoch = int(_run_git(repo, "show", "-s", "--format=%ct", rev)) + delta
            return datetime.datetime.fromtimestamp(
                epoch, datetime.timezone(datetime.timedelta(hours=8))
            ).strftime("%Y-%m-%d %H:%M:%S +08")

        _write(
            os.path.join(repo, "docs", "snapshot-ok", "ok.md"),
            "# 快照 @ `%s` @ %s\n\n分支 head `%s`，**未合入**\n"
            "卡 `openmir2-synthetic-closed` 未合入\n" % (merged, _stamp(merged), merged),
        )
        _write(
            os.path.join(repo, "docs", "snapshot-sha", "bad.md"),
            "# 快照 @ `%s` @ %s\n\n分支 head `%s`，**未合入**\n"
            % (pending, _stamp(pending), merged),
        )
        _write(
            os.path.join(repo, "docs", "snapshot-time", "bad.md"),
            "# 快照 @ `%s` @ %s\n\n分支 head `%s`，**未合入**\n"
            % (merged, _stamp(merged, 3600), merged),
        )
        _write(
            os.path.join(repo, "docs", "snapshot-order", "bad.md"),
            "分支 head `%s`，**未合入**\n\n# 快照 @ `%s` @ %s\n"
            % (merged, merged, _stamp(merged)),
        )
        # (I-1) The header boundary.  A snapshot declaration placed *after*
        # the first "##" heading, or *after* line 60, is a mid-body data
        # point, not a document freeze -- it must NOT downgrade the hit.
        # Mutation high=len(lines) makes both INFO and must fail check ㉗.
        _write(
            os.path.join(repo, "docs", "snapshot-header", "body.md"),
            "# 头部标题\n\n## 1. 正文\n\n"
            "board 快照 `%s` @ %s\n\n"
            "分支 head `%s`，**未合入**\n"
            % (merged, _stamp(merged), merged),
        )
        deep_lines = ["# 深度边界夹具"]
        deep_lines.extend("填充行 %d。" % i for i in range(2, 62))
        deep_lines.append("board 快照 `%s` @ %s" % (merged, _stamp(merged)))
        deep_lines.append("")
        deep_lines.append("分支 head `%s`，**未合入**" % merged)
        _write(
            os.path.join(repo, "docs", "snapshot-header", "deep.md"),
            "\n".join(deep_lines) + "\n",
        )
        # (I-2) A declaration back-dating the 2026-01-01 ancestor while the
        # line asserting today's drift is current: (a)/(b)/(c) all pass, only
        # the blame check (d) rejects it.
        _write(
            os.path.join(repo, "docs", "snapshot-old", "old.md"),
            "# 旧 sha 夹具\n\n快照 @ `%s` @ %s\n\n分支 head `%s`，**未合入**\n"
            % (old, _stamp(old), merged),
        )
        # (M-1) An unrelated 基线 sha above the anchor must not swallow the
        # real declaration: candidates are tried nearest-first, the real sha
        # (line 4) passes (a)/(b) while the baseline (line 2) does not.
        _write(
            os.path.join(repo, "docs", "snapshot-neighbor", "neighbor.md"),
            "# 邻居 sha 夹具\n"
            "基线 `%s`\n"
            "取数时点 %s\n"
            "head `%s`\n\n"
            "分支 head `%s`，**未合入**\n"
            % (old, _stamp(merged), merged, merged),
        )
        # A feature name that merely *looks* like a card id: absent from the
        # ledger, so it must be INFO (非账本卡号) and not red.
        _write(
            os.path.join(repo, "docs", "cardnonid", "feature.md"),
            "特性名 `openmir2-feature-flag-name` 未开始\n",
        )
        # A card-bearing line whose allow-list entry must survive --no-ledger.
        allowed_card_line = "卡 `openmir2-synthetic-closed` 未合入\n"
        _write(os.path.join(repo, "docs", "allowcard", "allowed.md"), allowed_card_line)
        _run_git(repo, "add", "-A")
        _run_git(repo, "commit", "-qm", "fixtures")

        # (I-2) A declaration added *after the fact*: the doc claims the
        # snapshot instant of `merged`, but the commit carrying the header is
        # dated 30 days later, so blame exposes the back-dating.
        _write(
            os.path.join(repo, "docs", "snapshot-late", "late.md"),
            "# 事后声明的夹具\n\n快照 @ `%s` @ %s\n\n分支 head `%s`，**未合入**\n"
            % (merged, _stamp(merged), merged),
        )
        _run_git(repo, "add", "-A")
        late_epoch = int(_run_git(repo, "show", "-s", "--format=%ct", merged)) + 30 * 86400
        late_stamp = datetime.datetime.fromtimestamp(
            late_epoch, datetime.timezone(datetime.timedelta(hours=8))
        ).strftime("%Y-%m-%dT%H:%M:%S+08:00")
        _run_git_env(
            repo,
            {"GIT_AUTHOR_DATE": late_stamp, "GIT_COMMITTER_DATE": late_stamp},
            "commit",
            "-qm",
            "late declaration",
        )

        # Collaboration refs the board projection cannot see: a thread absent
        # from the board JSON but present in the inbox tail (status +
        # merge_result), plus the closed fixture the board does list.
        def _blob(text):
            proc = subprocess.run(
                ["git", "hash-object", "-w", "--stdin"],
                cwd=repo,
                input=text,
                capture_output=True,
                check=True,
                text=True,
                encoding="utf-8",
            )
            return proc.stdout.strip()

        def _entry(kind, card_id, ts, body):
            return json.dumps(
                {
                    "version": 1,
                    "kind": kind,
                    "id": card_id,
                    "actor": "selftest",
                    "ts": ts,
                    "parent": "",
                    "body": body,
                },
                ensure_ascii=False,
            )

        _run_git(
            repo,
            "update-ref",
            "refs/collab/inbox/selftest/status",
            _blob(_entry("status", "openmir2-synthetic-offboard", 10, {"status": "in-progress"})),
        )
        _run_git(
            repo,
            "update-ref",
            "refs/collab/inbox/selftest/merge",
            _blob(_entry("merge_result", "openmir2-synthetic-offboard", 11, {"merged": True})),
        )
        _run_git(
            repo,
            "update-ref",
            "refs/collab/inbox/selftest/closed",
            _blob(_entry("status", "openmir2-synthetic-closed", 12, {"status": "closed"})),
        )
        board_path = os.path.join(tmp, "board.json")
        _write(
            board_path,
            json.dumps(
                {
                    "columns": [
                        {
                            "name": "x",
                            "cards": [
                                {"id": "openmir2-synthetic-closed", "status": "closed"},
                            ],
                        }
                    ]
                }
            ),
        )
        empty_allowlist = os.path.join(tmp, "empty.toml")
        _write(empty_allowlist, "# no exemptions\n")

        allowlist_path = os.path.join(tmp, "allow.toml")
        _write(
            allowlist_path,
            '[[allow]]\npath = "docs/allow/allowed.md"\n'
            'line_sha256 = "%s"\nreason = "selftest fixture"\n'
            % line_fingerprint(allowed_line),
        )

        allowcard_path = os.path.join(tmp, "allowcard.toml")
        _write(
            allowcard_path,
            '[[allow]]\npath = "docs/allowcard/allowed.md"\n'
            'line_sha256 = "%s"\nreason = "selftest fixture (card-only line)"\n'
            % line_fingerprint(allowed_card_line),
        )
        rotten_path = os.path.join(tmp, "rot.toml")
        seed_line = _read_seed_line(repo)
        _write(
            rotten_path,
            '[[allow]]\npath = "docs/plain/seed.md"\n'
            'line_sha256 = "%s"\nreason = "entry that suppresses nothing"\n'
            % line_fingerprint(seed_line),
        )

        ref = "refs/heads/main"
        red = scan(
            repo, "docs/red", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP
        )
        check(
            "① merged pointer + 未合入 ⇒ red",
            red["rc"] == 1
            and len(red["hits"]) == 1
            and red["hits"][0]["hash"] == merged
            and red["hits"][0]["line"] == 1,
        )

        green = scan(
            repo, "docs/green", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP
        )
        check(
            "② pointer not merged ⇒ green",
            green["rc"] == 0 and not green["hits"] and green["green_pointers"] == 1,
        )

        marker = scan(
            repo, "docs/marker", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP
        )
        check(
            "③ merged pointer with correction marker ⇒ green",
            marker["rc"] == 0 and not marker["hits"] and marker["marker_exempt"] == 1,
        )

        allowed = scan(repo, "docs/allow", ref, load_allowlist(allowlist_path), DEFAULT_MAX_GAP)
        check(
            "④ merged pointer allow-listed ⇒ green",
            allowed["rc"] == 0 and not allowed["hits"] and allowed["allow_exempt"] == 1,
        )

        unknown = scan(
            repo, "docs/unknown", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP
        )
        check(
            "⑤ unresolvable pointer is noted, not red",
            unknown["rc"] == 0 and len(unknown["unresolved"]) == 1,
        )

        quoted = scan(
            repo, "docs/quote", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP
        )
        check(
            "⑥ merged pointer inside 「…」 quotation ⇒ green",
            quoted["rc"] == 0 and not quoted["hits"],
        )

        rotten = scan(
            repo, "docs/plain", ref, load_allowlist(rotten_path), DEFAULT_MAX_GAP
        )
        check(
            "⑦ allow-list entry that suppresses nothing ⇒ red (rot)",
            rotten["rc"] == 1
            and len(rotten["rot"]) == 1
            and rotten["rot"][0]["path"] == "docs/plain/seed.md",
        )

        # ---- class 2: card id + status word judged against a synthetic ledger
        ledger = Ledger(
            {
                "openmir2-synthetic-closed": {
                    "status": "closed",
                    "merged": True,
                    "on_board": True,
                    "entries": 3,
                },
                "openmir2-synthetic-merged": {
                    "status": "in-progress",
                    "merged": True,
                    "on_board": True,
                    "entries": 5,
                },
            }
        )

        card_closed = scan(
            repo, "docs/cardclosed", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "⑧ ledger says card closed + 未合入 ⇒ red",
            card_closed["rc"] == 1
            and len(card_closed["card_hits"]) == 1
            and card_closed["card_hits"][0]["card"] == "openmir2-synthetic-closed"
            and card_closed["card_hits"][0]["state"] == "closed",
        )

        card_open = scan(
            repo, "docs/cardopen", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "⑨ card absent from the ledger + 未开始 ⇒ green",
            card_open["rc"] == 0
            and not card_open["card_hits"]
            and card_open["card_green"] == 1,
        )

        card_merged = scan(
            repo, "docs/cardmerged", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "⑩ merge_result merged:true + status in-progress ⇒ red",
            card_merged["rc"] == 1
            and len(card_merged["card_hits"]) == 1
            and card_merged["card_hits"][0]["state"].startswith("merged"),
        )

        card_table = scan(
            repo, "docs/cardtable", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "⑪ table row: id and word in different cells ARE a pair (same line) ⇒ red",
            card_table["rc"] == 1
            and len(card_table["card_hits"]) == 1
            and card_table["card_hits"][0]["card"] == "openmir2-synthetic-closed",
        )

        card_quoted = scan(
            repo, "docs/cardquoted", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "⑫ card hit inside 「…」 quotation ⇒ green",
            card_quoted["rc"] == 0 and not card_quoted["card_hits"],
        )

        no_ledger = scan(
            repo, "docs/cardclosed", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, None
        )
        cli = subprocess.run(
            [
                sys.executable,
                "-I",
                os.path.abspath(__file__),
                "--path",
                "docs/cardclosed",
                "--ref",
                ref,
                "--allowlist",
                empty_allowlist,
                "--no-ledger",
            ],
            cwd=repo,
            capture_output=True,
            check=False,
        )
        cli_text = (cli.stdout + cli.stderr).decode("utf-8", "replace")
        check(
            "⑬ --no-ledger: class skipped, CLI prints 已跳过, rc=0",
            no_ledger["rc"] == 0
            and not no_ledger["card_hits"]
            and no_ledger["ledger_skipped"]
            and cli.returncode == 0
            and "已跳过" in cli_text,
        )

        root_rot = scan(repo, ".", ref, load_allowlist(rotten_path), DEFAULT_MAX_GAP, None)
        check(
            "⑭ --path . (repo root) still reports allow-list rot",
            root_rot["rc"] == 1 and len(root_rot["rot"]) == 1,
        )

        # ---- board union: the projection omits threads the raw ledger knows
        with open(board_path, encoding="utf-8") as fh:
            board_raw = json.load(fh)
        board_ledger = read_ledger(repo, board_raw=board_raw)
        offboard = board_ledger.state("openmir2-synthetic-offboard")
        card_offboard = scan(
            repo, "docs/cardoffboard", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, board_ledger
        )
        check(
            "⑮ board 漏卡 + snapshot/inbox 有 ⇒ 并集补回（on_board=False）并判红",
            offboard is not None
            and offboard["on_board"] is False
            and offboard["merged"] is True
            and card_offboard["rc"] == 1
            and len(card_offboard["card_hits"]) == 1
            and card_offboard["card_hits"][0]["card"] == "openmir2-synthetic-offboard",
        )

        # ---- fold / flatten helpers directly (no scan, no subprocess)
        folded = _fold_entries(
            [
                {"id": "openmir2-synthetic-offboard", "kind": "status", "ts": 5, "body": {"status": "open"}},
                {"id": "openmir2-synthetic-offboard", "kind": "status", "ts": 7, "body": {"status": "in-progress"}},
                {"id": "openmir2-synthetic-offboard", "kind": "merge_result", "ts": 6, "body": {"merged": True}},
                {"id": "", "kind": "status", "ts": 1, "body": {"status": "closed"}},
            ]
        )
        columns_cards = _cards_from_raw({"columns": [{"cards": [{"id": "a"}]}]})
        thread_cards = _cards_from_raw({"threads": {"b": {"status": "closed"}}})
        plain_cards = _cards_from_raw({"cards": [{"id": "c"}]})
        check(
            "⑯ _fold_entries 取最新 status + 存在 merge；_cards_from_raw 三种形状",
            folded["openmir2-synthetic-offboard"]["status"] == "in-progress"
            and folded["openmir2-synthetic-offboard"]["merged"] is True
            and len(folded) == 1
            and [c["id"] for c in columns_cards] == ["a"]
            and [c["id"] for c in thread_cards] == ["b"]
            and [c["id"] for c in plain_cards] == ["c"],
        )

        # ---- compound words are not card assertions
        card_noise = scan(
            repo, "docs/cardnoise", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "⑰ 未接线 / 含未合并 是方向词与分类标签 ⇒ 不配对、不判红",
            card_noise["rc"] == 0
            and not card_noise["card_hits"]
            and card_noise["card_lines"] == 0,
        )

        # ---- goal_gate's new wording 「另未合并」 (2026-10-24)
        newword = scan(
            repo, "docs/newword", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        newword_red = scan(
            repo, "docs/newwordred", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        newword_hash = scan(
            repo, "docs/newwordhash", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "⑰b 「另未合并」是统计行标签 ⇒ 与卡号同行不配对、不判红",
            newword["rc"] == 0 and not newword["card_hits"],
        )
        check(
            "⑰c 同一位置换成真未完成词（未合入）⇒ 仍红（防放宽成万能豁免）",
            newword_red["rc"] == 1
            and len(newword_red["card_hits"]) == 1
            and newword_red["card_hits"][0]["card"] == "openmir2-synthetic-closed"
            and newword_red["card_hits"][0]["state"] == "closed",
        )
        check(
            "⑰d 反转：hash 类同样消费字面表 ⇒ hash + 另未合并 不判红（旧行为是假红，本卡修掉）",
            newword_hash["rc"] == 0
            and not newword_hash["hits"]
            and not newword_hash["card_hits"],
        )
        # Same filter, older label spelling: the hash sits next to a
        # statistic-row category, not next to this line's own claim.
        newword_hashstat = scan(
            repo,
            "docs/newwordhashstat",
            ref,
            load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP,
            ledger,
        )
        check(
            "⑰e hash + （含未合并 2）同行是统计行 ⇒ 不判红（hash 类与卡类同权豁免）",
            newword_hashstat["rc"] == 0
            and not newword_hashstat["hits"]
            and not newword_hashstat["card_hits"],
        )
        # The boundary that keeps the table from becoming a blanket exemption:
        # a hash next to a *real* pending word on the same line stays red.
        newword_hashred = scan(
            repo,
            "docs/newwordhashred",
            ref,
            load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP,
            ledger,
        )
        check(
            "⑰f 边界：hash + 真断言（未合入）同行 ⇒ 仍红（字面表不是万能豁免）",
            newword_hashred["rc"] == 1
            and len(newword_hashred["hits"]) == 1
            and newword_hashred["hits"][0]["hash"] == merged
            and newword_hashred["hits"][0]["word"] == "未合入",
        )
        # Span-level, never whole-line, hash class too: the（其中未合并 1）label
        # must not veto the real 未合并 further out on the same line.
        newword_hashspan = scan(
            repo,
            "docs/newwordhashspan",
            ref,
            load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP,
            ledger,
        )
        check(
            "⑰g span 级语义（hash 类）：（其中未合并 1）不否决同行真断言 未合并 ⇒ 仍红（整行否决变异必红）",
            newword_hashspan["rc"] == 1
            and len(newword_hashspan["hits"]) == 1
            and newword_hashspan["hits"][0]["word"] == "未合并",
        )

        # ---- carried #38: the exclusion is span-level, never whole-line
        card_span = scan(
            repo, "docs/cardspan", ref, load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger
        )
        check(
            "㉜ span 级语义：「未接线」同行还有真断言「未合并」⇒ 仍红（整行否决变异必红）",
            card_span["rc"] == 1
            and len(card_span["card_hits"]) == 1
            and card_span["card_hits"][0]["card"] == "openmir2-synthetic-closed",
        )

        # ---- carried #101: a failing git blame must be unusable, never exempt
        blame_epoch = int(_run_git(repo, "show", "-s", "--format=%ct", merged))
        blame_reasons = []
        blame_result = _blame_delta(
            repo, "docs/does-not-exist.md", [1], blame_epoch, {}, reasons=blame_reasons
        )
        check(
            "㉝ blame 执行失败（rc≠0）⇒ 不可用（返回 None + 点名 rc），绝不豁免",
            blame_result is None
            and len(blame_reasons) == 1
            and blame_reasons[0].startswith("git blame rc="),
        )

        # ---- --no-ledger must still keep a card-only entry alive (no false rot)
        allowcard_noledger = scan(
            repo, "docs/allowcard", ref, load_allowlist(allowcard_path), DEFAULT_MAX_GAP, None
        )
        check(
            "⑱ --no-ledger 下只压卡号命中的白名单条目仍算 used（不误报腐烂）",
            allowcard_noledger["rc"] == 0
            and allowcard_noledger["allow_exempt"] == 1
            and not allowcard_noledger["rot"],
        )

        # ---- merge flag contract is the boolean true, nothing else
        check(
            "⑲ _merged_flag 只认布尔 true（字符串 oid / null / false 都不算）",
            _merged_flag(True) is True
            and _merged_flag(False) is False
            and _merged_flag(None) is False
            and _merged_flag("9ff7ff064") is False,
        )

        # ---- an empty ledger is fail-closed: 查不到 ≠ 无问题
        empty_repo = os.path.join(tmp, "empty-repo")
        os.makedirs(empty_repo)
        _run_git(empty_repo, "init", "-q", "-b", "main")
        _run_git(empty_repo, "config", "user.email", "selftest@example.invalid")
        _run_git(empty_repo, "config", "user.name", "selftest")
        _write(
            os.path.join(empty_repo, "docs", "card.md"),
            "卡 `openmir2-synthetic-closed` 未合入\n",
        )
        _run_git(empty_repo, "add", "-A")
        _run_git(empty_repo, "commit", "-qm", "card")
        empty_board = os.path.join(tmp, "empty-board.json")
        _write(empty_board, json.dumps({"columns": []}))
        empty_args = [
            sys.executable,
            "-I",
            os.path.abspath(__file__),
            "--ref",
            "refs/heads/main",
            "--allowlist",
            empty_allowlist,
            "--ledger-json",
            empty_board,
        ]
        empty_run = subprocess.run(empty_args, cwd=empty_repo, capture_output=True, check=False)
        empty_run_allowed = subprocess.run(
            empty_args + ["--allow-empty-ledger"], cwd=empty_repo, capture_output=True, check=False
        )
        empty_text = (empty_run.stdout + empty_run.stderr).decode("utf-8", "replace")
        allowed_text = (empty_run_allowed.stdout + empty_run_allowed.stderr).decode(
            "utf-8", "replace"
        )
        check(
            "⑳ 账本 0 线程 ⇒ rc=2；--allow-empty-ledger ⇒ rc=0 + NOTE",
            empty_run.returncode == 2
            and "0 threads" in empty_text
            and empty_run_allowed.returncode == 0
            and "allow-empty-ledger" in allowed_text,
        )

        # ---- snapshot declarations: checked, never unconditional
        snap_ok = scan(
            repo, "docs/snapshot-ok", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉑ 受检快照声明（sha 祖先 + 时间吻合 + 声明在前）⇒ 命中降级 INFO，不红",
            snap_ok["rc"] == 0
            and not snap_ok["hits"]
            and not snap_ok["card_hits"]
            and snap_ok["snapshot_exempt"] == 2
            and {item["kind"] for item in snap_ok["snapshot_info"]} == {"hash", "card"}
            and all(item["decl"]["sha"] == merged for item in snap_ok["snapshot_info"]),
        )

        snap_sha = scan(
            repo, "docs/snapshot-sha", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉒ 声明 sha 存在但不是 --ref 祖先 ⇒ 不豁免，仍红",
            snap_sha["rc"] == 1
            and len(snap_sha["hits"]) == 1
            and snap_sha["hits"][0]["hash"] == merged
            and not snap_sha["snapshot_info"],
        )

        snap_time = scan(
            repo, "docs/snapshot-time", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉓ 声明时间与 commit 差 > --snapshot-tol ⇒ 不豁免，仍红",
            snap_time["rc"] == 1
            and len(snap_time["hits"]) == 1
            and snap_time["hits"][0]["hash"] == merged
            and not snap_time["snapshot_info"],
        )

        snap_order = scan(
            repo, "docs/snapshot-order", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉔ 声明写在命中之后 ⇒ 不豁免，仍红",
            snap_order["rc"] == 1
            and len(snap_order["hits"]) == 1
            and snap_order["hits"][0]["hash"] == merged
            and not snap_order["snapshot_info"],
        )

        card_nonid = scan(
            repo, "docs/cardnonid", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉕ 不在账本的卡号形状 ⇒ 非卡号（特性名），INFO 且不红",
            card_nonid["rc"] == 0
            and not card_nonid["card_hits"]
            and len(card_nonid["card_nonid"]) == 1
            and card_nonid["card_nonid"][0]["card"] == "openmir2-feature-flag-name",
        )

        missing_ledger = os.path.join(tmp, "does-not-exist.json")
        unreadable = subprocess.run(
            [
                sys.executable,
                "-I",
                os.path.abspath(__file__),
                "--path",
                "docs/snapshot-ok",
                "--ref",
                ref,
                "--allowlist",
                empty_allowlist,
                "--ledger-json",
                missing_ledger,
            ],
            cwd=repo,
            capture_output=True,
            check=False,
        )
        unreadable_text = (unreadable.stdout + unreadable.stderr).decode(
            "utf-8", "replace"
        )
        check(
            "㉖ 账本读不到 ⇒ rc=2（查不到 ≠ 不是卡号）",
            unreadable.returncode == 2 and "not found" in unreadable_text,
        )

        snap_header = scan(
            repo, "docs/snapshot-header", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉗ 文档头边界外的快照声明（首个 ## 之后 / 第 60 行之后）⇒ 不降级，仍红",
            snap_header["rc"] == 1
            and len(snap_header["hits"]) == 2
            and snap_header["snapshot_exempt"] == 0
            and {hit["path"] for hit in snap_header["hits"]}
            == {"docs/snapshot-header/body.md", "docs/snapshot-header/deep.md"},
        )

        snap_late = scan(
            repo, "docs/snapshot-late", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉘ 声明行事后补（blame 时间与声称时间差 > --decl-tol）⇒ 不降级，仍红",
            snap_late["rc"] == 1
            and len(snap_late["hits"]) == 1
            and not snap_late["snapshot_info"]
            and len(snap_late["decl_notes"]) == 1
            and "--decl-tol" in " ".join(snap_late["decl_notes"][0]["reasons"]),
        )

        snap_old = scan(
            repo, "docs/snapshot-old", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉙ 2026-01-01 的祖先用其真实时间声明、今天才写入 ⇒ 仍红（blame 新近性）",
            snap_old["rc"] == 1
            and len(snap_old["hits"]) == 1
            and not snap_old["snapshot_info"]
            and len(snap_old["decl_notes"]) == 1
            and "--decl-tol" in " ".join(snap_old["decl_notes"][0]["reasons"]),
        )

        snap_neighbor = scan(
            repo, "docs/snapshot-neighbor", ref, load_allowlist(empty_allowlist),
            DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉚ 相邻 `基线 <sha>` 不再吞掉真声明：候选近到远试，真 sha 胜出 ⇒ INFO",
            snap_neighbor["rc"] == 0
            and not snap_neighbor["hits"]
            and snap_neighbor["snapshot_exempt"] == 1
            and len(snap_neighbor["snapshot_info"]) == 1
            and snap_neighbor["snapshot_info"][0]["decl"]["sha"] == merged
            and snap_neighbor["snapshot_info"][0]["decl"]["sha_line"] == 4
            and snap_neighbor["snapshot_info"][0]["decl"]["rejected_candidates"],
        )

        # ---- ㉛ (card openmir2-stale-pointer-blame-failclosed): staged but NOT committed
        staged_repo = os.path.join(tmp, "staged-repo")
        os.makedirs(staged_repo)
        _run_git(staged_repo, "init", "-q", "-b", "main")
        _run_git(staged_repo, "config", "user.email", "selftest@example.invalid")
        _run_git(staged_repo, "config", "user.name", "selftest")
        _write(os.path.join(staged_repo, "seed.md"), "seed" + chr(10))
        _run_git(staged_repo, "add", "-A")
        _run_git(staged_repo, "commit", "-qm", "base")
        staged_base = _run_git(staged_repo, "rev-parse", "HEAD")
        staged_epoch = int(
            _run_git(staged_repo, "show", "-s", "--format=%ct", staged_base)
        )
        staged_stamp = datetime.datetime.fromtimestamp(
            staged_epoch, datetime.timezone(datetime.timedelta(hours=8))
        ).strftime("%Y-%m-%d %H:%M:%S +08")
        _write(
            os.path.join(staged_repo, "docs", "staged", "staged.md"),
            ("# 快照 @ `%s` @ %s" + chr(10) * 2
             + "分支 head `%s`，**未合入**" + chr(10))
            % (staged_base, staged_stamp, staged_base),
        )
        _run_git(staged_repo, "add", "-A")  # staged only: blame = 全 0 / Not Committed Yet
        staged = scan(
            staged_repo, "docs", "refs/heads/main",
            load_allowlist(empty_allowlist), DEFAULT_MAX_GAP, ledger,
        )
        check(
            "㉛ 声明行已 stage 未 commit ⇒ blame 全 0（Not Committed Yet）⇒ 不豁免，仍红",
            staged["rc"] == 1
            and len(staged["hits"]) == 1
            and staged["snapshot_exempt"] == 0
            and len(staged["decl_notes"]) == 1
            and "Not Committed Yet" in " ".join(staged["decl_notes"][0]["reasons"]),
        )

    failed = 0
    for name, ok in checks:
        print(("PASS " if ok else "FAIL ") + name)
        failed += 0 if ok else 1
    print("selftest: %d/%d passed" % (len(checks) - failed, len(checks)))
    return 0 if failed == 0 else 1


def _read_seed_line(repo):
    with open(os.path.join(repo, "docs", "plain", "seed.md"), encoding="utf-8") as fh:
        return fh.readline()


# ----------------------------------------------------------------------------- cli
def display_path(top, path):
    """Repo-relative when inside the repo, absolute otherwise (cross-drive)."""
    try:
        rel = os.path.relpath(path, top)
    except ValueError:  # e.g. --allowlist on another Windows drive
        return os.path.abspath(path)
    return rel.replace(os.sep, "/")


def fingerprint_command(top, spec):
    if ":" not in spec:
        raise GateError("--fingerprint expects PATH:LINE, got %r" % spec)
    path_part, _, line_part = spec.rpartition(":")
    try:
        lineno = int(line_part)
    except ValueError:
        raise GateError("--fingerprint expects PATH:LINE, got %r" % spec)
    rel = repo_relative(top, path_part)
    with open(os.path.join(top, rel), "rb") as fh:
        lines = fh.read().decode("utf-8").splitlines()
    if not 1 <= lineno <= len(lines):
        raise GateError("%s has no line %d" % (rel, lineno))
    line = lines[lineno - 1]
    print('[[allow]]\npath = "%s"\nline_sha256 = "%s"' % (rel, line_fingerprint(line)))
    print("reason = \"\"  # REQUIRED: say why this historical line must stay")
    print("# line %d: %s" % (lineno, line.strip()), file=sys.stderr)
    return 0


def main(argv):
    parser = argparse.ArgumentParser(
        prog="stale_pointer_gate.py",
        description="Fail when docs call an already-merged commit 'not merged'.",
    )
    parser.add_argument(
        "--path",
        default=DEFAULT_PATH,
        help="directory to scan, relative to the repo root (default: docs)",
    )
    parser.add_argument(
        "--ref",
        default=DEFAULT_REF,
        help="target ref for ancestry (default: refs/remotes/walgit/master)",
    )
    parser.add_argument(
        "--allowlist",
        default=None,
        help="allow-list TOML (default: <repo>/tools/hygiene/"
        "stale_pointer_allowlist.toml)",
    )
    parser.add_argument(
        "--max-gap",
        type=int,
        default=DEFAULT_MAX_GAP,
        help="max characters between a pointer/card id and an unfinished word "
        "(default: 24)",
    )
    parser.add_argument(
        "--snapshot-tol",
        type=int,
        default=DEFAULT_SNAPSHOT_TOL,
        help="max seconds between a doc's snapshot declaration and the declared "
        "commit's author/committer time (default %d)" % DEFAULT_SNAPSHOT_TOL,
    )
    parser.add_argument(
        "--decl-tol",
        type=int,
        default=DEFAULT_DECL_TOL,
        help="max seconds between a snapshot declaration's claimed time and the "
        "commit that wrote the declaration line (git blame); a larger gap means "
        "the declaration was added after the fact (default %d = 1 day)"
        % DEFAULT_DECL_TOL,
    )
    parser.add_argument(
        "--no-ledger",
        action="store_true",
        help="skip the card-status (ledger) check; prints 已跳过 and judges nothing",
    )
    parser.add_argument(
        "--allow-empty-ledger",
        action="store_true",
        help="a ledger that reads as 0 threads is rc=2 (查不到 ≠ 无问题); this flag "
        "downgrades it to a loud NOTE",
    )
    parser.add_argument(
        "--ledger-json",
        metavar="PATH",
        default=None,
        help="read a board-shaped ledger snapshot from PATH instead of running "
        "`walgit collab board` (offline replay / selftest)",
    )
    parser.add_argument("--selftest", action="store_true", help="run the synthetic self-check")
    parser.add_argument(
        "--fingerprint",
        metavar="PATH:LINE",
        help="print the allow-list stanza for one line and exit",
    )
    args = parser.parse_args(argv)

    if args.selftest:
        return selftest()

    top = repo_root()
    if args.fingerprint:
        return fingerprint_command(top, args.fingerprint)

    if args.no_ledger and args.ledger_json:
        raise GateError(
            "stale-pointer gate: --no-ledger and --ledger-json are mutually exclusive",
            rc=2,
        )

    ref = resolve_ref(top, args.ref)
    rel_path = repo_relative(top, os.path.join(top, args.path))
    if not os.path.exists(os.path.join(top, rel_path)):
        raise GateError("stale-pointer gate: --path %r does not exist" % args.path, rc=2)

    allowlist_path = args.allowlist or os.path.join(top, DEFAULT_ALLOWLIST)
    allowlist = load_allowlist(allowlist_path)

    ledger = None
    if not args.no_ledger:
        if args.ledger_json:
            ledger = read_ledger(top, board_raw=load_board_file(args.ledger_json))
        else:
            ledger = read_ledger(top)
        if len(ledger) == 0:
            # A checkout that never fetched refs/collab/* looks exactly like a
            # project with no cards.  "Cannot see" is not "no problem" — this
            # repo has burned three reworks on that confusion — so an empty
            # ledger is fail-closed unless the caller says otherwise.
            message = (
                "stale-pointer gate: the ledger lists 0 threads — 查不到 ≠ 无问题. "
                "A fresh checkout without `git fetch <remote> '+refs/collab/*:refs/collab/*'` "
                "looks exactly like this, and the card-status check would silently pass. "
                "Fetch refs/collab/* first, or pass --no-ledger (skips the class) / "
                "--allow-empty-ledger (downgrades this to a NOTE)."
            )
            if args.allow_empty_ledger:
                print("NOTE " + message + " (--allow-empty-ledger: continuing)", file=sys.stderr)
            else:
                raise GateError(message, rc=2)

    try:
        result = scan(
            top,
            rel_path,
            ref,
            allowlist,
            args.max_gap,
            ledger,
            snapshot_tol=args.snapshot_tol,
            decl_tol=args.decl_tol,
        )
    except GateError as exc:
        print(str(exc), file=sys.stderr)
        return exc.rc
    emit(result, ref, display_path(top, allowlist_path))
    return result["rc"]


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
