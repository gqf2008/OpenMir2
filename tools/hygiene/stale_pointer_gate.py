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
silently swallowed two real drifts — ``leader-quota-block-20261023.md:22`` and
``window-round3-runbook-20261021.md:30`` — while removing only 4 false hits.)

Two literals that merely *contain* a pending word are not status assertions and
are skipped: 「未接线」/「未接入」 (a subsystem is not wired — a direction word,
not a card lifecycle) and 「含未合并」/「其中未合并」 (a statistic-row category
label).  This is a literal exclusion list, not a narrowing of the word tables.

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
# 「含未合并」/「其中未合并」= a statistic-row category label.  A pending-word
# match lying inside one of these spans is skipped (reviewer-3 finding I-1 on
# e5c9f169c: these were the two false reds that the same-cell rule used to hide).
NOT_ASSERTIVE = ("含未合并", "其中未合并", "未接线", "未接入")

# The repo's two correction-marker spellings: 〔…订正…〕 and "YYYY-MM-DD 订正",
# the latter optionally wrapped in bold.  Both mean "this line is a historical
# note carrying its own correction".
CORRECTION_RE = re.compile(r"〔[^〕]*订正[^〕]*〕|\d{4}-\d{2}-\d{2}\s*\*{0,2}\s*订正")

# Chinese quotation marks.  A pending word inside 「…」/『…』 is someone else's
# *wording being quoted* (the drift-sweep reports literally tabulate
# "原说法 → 订正"), not this line's own assertion, so it is not a hit.
QUOTE_RE = re.compile(r"「[^」]*」|『[^』]*』")

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
    """Return [(hash, word, gap)] for pointers close to an unfinished word."""
    quote_spans = [(m.start(), m.end()) for m in QUOTE_RE.finditer(line)]
    out = []
    for match in HASH_RE.finditer(line):
        token = match.group(1)
        if not HAS_HEX_LETTER.search(token):
            continue
        near = nearest_pending(line, match.start(), match.end(), quote_spans)
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


def load_allowlist(path):
    try:
        with open(path, "rb") as fh:
            data = tomllib.load(fh)
    except FileNotFoundError:
        raise GateError(
            "stale-pointer gate: allow-list not found: %s (fail-closed)" % path,
            rc=2,
        )
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise GateError(
            "stale-pointer gate: cannot read allow-list %s: %s" % (path, exc),
            rc=2,
        )
    entries = data.get("allow", [])
    if not isinstance(entries, list):
        raise GateError(
            "stale-pointer gate: allow-list %s: `allow` must be an array of tables"
            % path
        )
    seen = {}
    checked = []
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            raise GateError(
                "stale-pointer gate: allow-list %s: entry #%d is not a table"
                % (path, index + 1)
            )
        rel = entry.get("path")
        fingerprint = entry.get("line_sha256")
        reason = entry.get("reason")
        if not isinstance(rel, str) or not rel:
            raise GateError(
                "stale-pointer gate: allow-list %s: entry #%d lacks a non-empty "
                "`path`" % (path, index + 1)
            )
        if (
            not isinstance(fingerprint, str)
            or len(fingerprint) != 64
            or any(c not in "0123456789abcdef" for c in fingerprint)
        ):
            raise GateError(
                "stale-pointer gate: allow-list %s: entry #%d (%s) has no valid "
                "64-hex `line_sha256`" % (path, index + 1, rel)
            )
        if not isinstance(reason, str) or not reason.strip():
            raise GateError(
                "stale-pointer gate: allow-list %s: entry #%d (%s) lacks a "
                "`reason` — every exemption must say why" % (path, index + 1, rel)
            )
        key = (rel.replace("\\", "/"), fingerprint)
        if key in seen:
            raise GateError(
                "stale-pointer gate: allow-list %s: duplicate entry for %s"
                % (path, rel)
            )
        seen[key] = index
        checked.append(
            {
                "path": key[0],
                "line_sha256": fingerprint,
                "reason": reason.strip(),
                "index": index + 1,
                "used": False,
            }
        )
    return checked


def scan(top, rel_path, ref, allowlist, max_gap, ledger=None):
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
    }
    ancestry = {}

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
                        # Not a thread id in the ledger (e.g. a principal name):
                        # nothing to judge, which is green.
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

    # Rot detection only applies to entries inside the scanned subtree — a
    # narrowed --path must not flag allow-list entries that were simply not
    # looked at.  Scanning the repo root yields rel_path ".", which matches no
    # entry prefix; normalize it so root scans keep checking every entry
    # (reviewer-3 finding on eeb8d064f: rot was silently off for --path .).
    prefix = rel_path.rstrip("/")
    if prefix in ("", "."):
        in_scope = allowlist
    else:
        in_scope = [
            e for e in allowlist if e["path"] == prefix or e["path"].startswith(prefix + "/")
        ]
    result["rot"] = [e for e in in_scope if not e["used"]]
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
            "line(s), %d unresolved; %d card-status hit(s), %d card id(s) judged, "
            "%d card-bearing line(s)%s; ref=%s, allow-list=%s)"
            % (
                result["files"],
                result["lines"],
                result["green_pointers"],
                result["marker_exempt"],
                result["allow_exempt"],
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
        # A card the board projection does not list: only snapshot/inbox know it.
        _write(
            os.path.join(repo, "docs", "cardoffboard", "offboard.md"),
            "卡 `openmir2-synthetic-offboard` 未合入\n",
        )
        # A card-bearing line whose allow-list entry must survive --no-ledger.
        allowed_card_line = "卡 `openmir2-synthetic-closed` 未合入\n"
        _write(os.path.join(repo, "docs", "allowcard", "allowed.md"), allowed_card_line)
        _run_git(repo, "add", "-A")
        _run_git(repo, "commit", "-qm", "fixtures")

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
        result = scan(top, rel_path, ref, allowlist, args.max_gap, ledger)
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
