#!/usr/bin/env python3
"""board_coverage_gate — every ledger **card thread** must be visible on the board.

Card: openmir2-board-merged-column (tier=L).

``walgit collab board`` is a projection: a thread is shown only when it matches
a column predicate, and ``.walgit/board.toml`` is "first match wins; a thread
that matches no column is **not shown**".  A projection status without a
column (today: ``merged`` — the newest entry is a ``merge_result``) therefore
silently drops cards off the board, and a dropped card can never be claimed,
reviewed or closed again.

**Card threads, not every ledger thread.**  The CI runner also writes
pseudo-threads into ``refs/collab/inbox/*`` (kind ``ci_claim`` / ``ci_result``,
id ``ci-<hex>``, empty ``parent``).  Those are run records, never cards, and
the board never shows them.  They are excluded through the **closed set**
``NON_CARD_KINDS`` below.  A thread is non-card only when *all* of its entries
carry a kind from that set; **any unknown kind counts as a card**, so a new
thread type can never vanish silently.  Add a kind to ``NON_CARD_KINDS`` only
together with a selftest fixture and a review.

This gate compares the full **card** thread id set in ``refs/collab/inbox/*``
with the ids the board actually shows, and prints every missing thread together
with its newest entry kind and timestamp.

Usage (run from anywhere inside the repo):

  python -I tools/hygiene/board_coverage_gate.py            # rc=0 covered / 1 missing
  python -I tools/hygiene/board_coverage_gate.py --board .walgit/board.toml   # preview an uncommitted column
  python -I tools/hygiene/board_coverage_gate.py --json     # machine-readable
  python -I tools/hygiene/board_coverage_gate.py --selftest # synthetic self-check
  python -I tools/hygiene/board_coverage_gate.py --help

``--board`` may be relative; it is resolved against the repository root, so the
gate works from any subdirectory (``--repo`` is resolved to its git toplevel).

Exit codes: 0 every card thread is on the board; 1 one or more card threads are
missing (listed with kind/ts); 2 ledger or board unreadable (fail-closed —
never green, including any unparsable inbox blob).

Cost: one ``git rev-parse --show-toplevel`` + one ``git for-each-ref`` + **one**
``git cat-file --batch`` over every inbox blob + one ``walgit collab board``
call.  No per-thread subprocesses.
"""

import contextlib
import io
import json
import os
import subprocess
import sys
import time

RUN_TIMEOUT = 120.0

# Closed set of known non-card (CI runner) entry kinds.  Anything outside this
# set is treated as a card thread — unknown kinds must fail closed, never be
# skipped.  Extending this set requires a selftest fixture + review.
NON_CARD_KINDS = frozenset(("ci_claim", "ci_result"))


def _run(argv, timeout=RUN_TIMEOUT):
    """Never raises: returns (rc, stdout, stderr); rc 124/127 on timeout/OSError."""
    try:
        done = subprocess.run(argv, capture_output=True, text=True,
                              timeout=timeout, check=False)
        return done.returncode, done.stdout, done.stderr
    except subprocess.TimeoutExpired:
        return 124, "", "timeout after %ss: %s" % (timeout, argv[0])
    except OSError as exc:
        return 127, "", "cannot execute %r: %s" % (argv[0], exc)


def repo_root(repo):
    """Return (toplevel, error): resolve a (possibly sub-directory) repo path."""
    rc, out, err = _run(["git", "-C", repo, "rev-parse", "--show-toplevel"])
    if rc != 0 or not out.strip():
        return None, "cannot resolve repo root for %r (rc=%d): %s" % (
            repo, rc, err.strip()[:200])
    return out.strip(), None


def inbox_shas(repo):
    """Return the unique object names under refs/collab/inbox/*."""
    rc, out, err = _run(["git", "-C", repo, "for-each-ref",
                         "refs/collab/inbox", "--format=%(objectname)"])
    if rc != 0:
        return None, "cannot list refs/collab/inbox (rc=%d): %s" % (rc, err.strip()[:200])
    return list(dict.fromkeys(out.split())), None


def batch_entries(repo, shas):
    """Return (entries, error): every inbox blob parsed, in one cat-file --batch.

    Fail-closed: a malformed cat-file header/body, an unparsable blob or an
    entry without an id is an error (rc=2 upstream), never a silent skip.
    """
    if not shas:
        return [], None
    try:
        proc = subprocess.Popen(["git", "-C", repo, "cat-file", "--batch"],
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE)
    except OSError as exc:
        return None, "cannot run git cat-file --batch: %s" % exc
    try:
        out, err = proc.communicate(("\n".join(shas) + "\n").encode(), timeout=RUN_TIMEOUT)
    except subprocess.TimeoutExpired:
        proc.kill()
        return None, "git cat-file --batch timed out after %ss" % RUN_TIMEOUT
    if proc.returncode != 0:
        return None, "git cat-file --batch rc=%d: %s" % (
            proc.returncode, err.decode("utf-8", "replace").strip()[:200])
    entries = []
    pos = 0
    while pos < len(out):
        newline = out.find(b"\n", pos)
        if newline < 0:
            return None, "cat-file --batch: truncated header near byte %d" % pos
        header = out[pos:newline].decode("utf-8", "replace")
        pos = newline + 1
        parts = header.split()
        if len(parts) != 3:
            return None, "cat-file --batch: unexpected header %r" % header[:120]
        try:
            size = int(parts[2])
        except ValueError:
            return None, "cat-file --batch: bad size in header %r" % header[:120]
        if size < 0 or pos + size > len(out):
            return None, "cat-file --batch: truncated body for %s" % parts[0][:12]
        body = out[pos:pos + size].decode("utf-8", "replace")
        pos += size + 1  # skip the LF that terminates the body
        try:
            item = json.loads(body)
        except ValueError as exc:
            return None, "inbox blob %s is not JSON: %s" % (parts[0][:12], exc)
        entry = _entry_of(item)
        if entry is None or not entry.get("id"):
            return None, "inbox blob %s has no entry id" % parts[0][:12]
        entries.append(item)
    return entries, None


def _entry_of(item):
    """Entry dict from an inbox blob.

    Inbox blobs are the signed entry itself; `walgit collab thread` wraps them
    as ``{"entry": ..., "oid": ..., "verified": ...}``.  Accept both shapes.
    """
    if not isinstance(item, dict):
        return None
    inner = item.get("entry")
    if isinstance(inner, dict):
        return inner
    if item.get("id") and item.get("kind"):
        return item
    return None


def newest_by_thread(entries):
    """Return {thread_id: (ts, kind, actor)} keeping the newest entry per thread."""
    newest = {}
    for item in entries:
        entry = _entry_of(item) or {}
        thread = entry.get("id")
        if not thread:
            continue
        try:
            ts = int(entry.get("ts") or 0)
        except (TypeError, ValueError):
            ts = 0
        candidate = (ts, str(entry.get("kind") or ""), str(entry.get("actor") or ""))
        if thread not in newest or candidate[0] > newest[thread][0]:
            newest[thread] = candidate
    return newest


def thread_kinds(entries):
    """Return {thread_id: {kind, ...}} over every entry of every thread."""
    kinds = {}
    for item in entries:
        entry = _entry_of(item) or {}
        thread = entry.get("id")
        if not thread:
            continue
        kinds.setdefault(thread, set()).add(str(entry.get("kind") or ""))
    return kinds


def card_threads(entries):
    """Return {thread_id: (ts, kind, actor)} for card threads only.

    A thread is non-card only when **every** one of its entries has a kind in
    the closed set ``NON_CARD_KINDS``.  Unknown kinds are cards by default, so
    a novel thread type is reported (red) instead of silently disappearing.
    """
    newest = newest_by_thread(entries)
    kinds = thread_kinds(entries)
    return {thread: value for thread, value in newest.items()
            if not kinds.get(thread, set()) <= NON_CARD_KINDS}


def hidden_threads(newest, shown):
    """Threads present in the ledger but absent from the board (sorted)."""
    return sorted(t for t in newest if t not in shown)


def board_ids(repo, board=None):
    """Return (ids, error) from one `walgit collab board --format json` call.

    ``board`` mirrors ``walgit collab board --board <path>``: the board reads
    `.walgit/board.toml` **at HEAD**, so an uncommitted column change is only
    visible through this override.
    """
    argv = ["walgit", "collab", "board", "--repo", repo, "--format", "json"]
    if board:
        argv += ["--board", board]
    rc, out, err = _run(argv)
    if rc != 0:
        return None, "cannot read board (rc=%d): %s" % (rc, (err or out).strip()[:200])
    try:
        data = json.loads(out)
    except ValueError as exc:
        return None, "board output is not JSON: %s" % exc
    ids = set()
    try:
        for column in data.get("columns") or []:
            for card in column.get("cards") or []:
                if card.get("id"):
                    ids.add(card["id"])
    except AttributeError:
        return None, "board output has an unexpected shape"
    return ids, None


def check(repo, board=None):
    """Return (hidden, newest, counts, error)."""
    root, err = repo_root(repo)
    if err:
        return None, None, None, err
    if board and not os.path.isabs(board):
        board = os.path.join(root, board)
    shas, err = inbox_shas(root)
    if err:
        return None, None, None, err
    entries, err = batch_entries(root, shas)
    if err:
        return None, None, None, err
    newest_all = newest_by_thread(entries)
    newest = card_threads(entries)
    shown, err = board_ids(root, board=board)
    if err:
        return None, None, None, err
    hidden = hidden_threads(newest, shown)
    return hidden, newest, {"blobs": len(shas), "entries": len(entries),
                            "threads": len(newest),
                            "non_card_threads": len(newest_all) - len(newest),
                            "board_ids": len(shown)}, None


def _selftest_exit_wiring():
    """Exercise the real main() with an injected check().

    Pins "printed FAIL ⇒ non-zero exit" and "OK ⇒ 0": mutating the final
    ``return 1 if hidden else 0`` to ``return 0`` must turn this red.
    """
    global check
    original = check

    def fake_with_hidden(repo, board=None):
        return (["t-hidden"], {"t-hidden": (1, "comment", "a")},
                {"blobs": 1, "entries": 2, "threads": 2, "non_card_threads": 0,
                 "board_ids": 1}, None)

    def fake_clean(repo, board=None):
        return ([], {"t-shown": (1, "status", "a")},
                {"blobs": 1, "entries": 1, "threads": 1, "non_card_threads": 0,
                 "board_ids": 1}, None)

    rc_fail = rc_ok = None
    output = ""
    try:
        check = fake_with_hidden
        buf_out, buf_err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(buf_out), contextlib.redirect_stderr(buf_err):
            rc_fail = main([])
        output = buf_out.getvalue() + buf_err.getvalue()
        check = fake_clean
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            rc_ok = main([])
    finally:
        check = original
    return [
        ("⑦ printed FAIL wires to exit code 1", "FAIL" in output and rc_fail == 1),
        ("⑧ clean run wires to exit code 0", rc_ok == 0),
    ]


def selftest():
    """Synthetic fixtures only — no repo, ledger or board required."""
    def item(thread, kind, ts, actor="a"):
        return {"entry": {"id": thread, "kind": kind, "ts": ts, "actor": actor}}

    entries = [
        item("t-shown", "status", 10),
        item("t-hidden", "merge_result", 20, "b"),
        item("t-hidden", "patch", 15, "b"),
    ]
    ci_entries = [item("ci-deadbeef", "ci_claim", 30),
                  item("ci-deadbeef", "ci_result", 31, "runner")]
    unknown_entries = [item("t-unknown", "foo_bar", 40)]

    newest = newest_by_thread(entries)
    cards_with_ci = card_threads(entries + ci_entries)
    unknown = card_threads(unknown_entries)
    checks = [
        ("① newest entry per thread wins (kind=merge_result, ts=20)",
         newest.get("t-hidden") == (20, "merge_result", "b")),
        ("② thread absent from board ⇒ hidden",
         hidden_threads(newest, {"t-shown"}) == ["t-hidden"]),
        ("③ thread present on board ⇒ covered",
         hidden_threads(newest, {"t-shown", "t-hidden"}) == []),
        ("④ ci_claim/ci_result are not card threads",
         "ci-deadbeef" not in cards_with_ci
         and "ci-deadbeef" in newest_by_thread(entries + ci_entries)),
        ("⑤ an unknown kind (foo_bar) counts as a card thread",
         "t-unknown" in unknown),
        ("⑥ unknown-kind card missing from board ⇒ reported",
         hidden_threads(unknown, set()) == ["t-unknown"]),
    ]
    checks += _selftest_exit_wiring()
    failed = 0
    for name, ok in checks:
        print(("PASS " if ok else "FAIL ") + name)
        failed += 0 if ok else 1
    print("selftest: %d/%d passed" % (len(checks) - failed, len(checks)))
    return 0 if failed == 0 else 1


def main(argv):
    if "-h" in argv or "--help" in argv:
        print(__doc__.strip())
        return 0
    if "--selftest" in argv:
        return selftest()

    repo = "."
    board = None
    as_json = "--json" in argv
    rest = [a for a in argv if a not in ("--json",)]
    while rest:
        arg = rest.pop(0)
        if arg == "--repo":
            if not rest:
                print("board-coverage gate: --repo needs a value", file=sys.stderr)
                return 2
            repo = rest.pop(0)
        elif arg == "--board":
            if not rest:
                print("board-coverage gate: --board needs a value", file=sys.stderr)
                return 2
            board = rest.pop(0)
        else:
            print("board-coverage gate: unknown argument %r" % arg, file=sys.stderr)
            return 2
    repo = os.path.abspath(repo)

    started = time.time()
    hidden, newest, counts, err = check(repo, board=board)
    elapsed = time.time() - started
    if err:
        print("board-coverage gate: %s" % err, file=sys.stderr)
        return 2

    if as_json:
        print(json.dumps({
            "hidden": [{"id": t, "kind": newest[t][1], "ts": newest[t][0],
                        "actor": newest[t][2]} for t in hidden],
            "counts": counts, "elapsed_s": round(elapsed, 2),
        }, ensure_ascii=False, indent=1))
    else:
        for thread in hidden:
            ts, kind, actor = newest[thread]
            print("%s\t%s\t%d\t%s" % (thread, kind, ts, actor))
        print("board-coverage gate: %s (%d hidden of %d card threads; board shows %d ids; "
              "%d non-card threads excluded; %.2fs)"
              % ("FAIL" if hidden else "OK", len(hidden), counts["threads"],
                 counts["board_ids"], counts["non_card_threads"], elapsed),
              file=sys.stderr if hidden else sys.stdout)
    return 1 if hidden else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
