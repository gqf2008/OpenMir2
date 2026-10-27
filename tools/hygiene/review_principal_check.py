#!/usr/bin/env python3
"""review_principal_check — 派复审/派第二个实例前的 10 秒机械核对。

Card: openmir2-review-principal-check (tier=L).

Answers three questions from the collaboration ledger and the local worktree:

  1. Who are the **candidate authors** of a card?  Every verified signer of a
     ``patch`` entry or of an ``in-progress`` ``status`` entry.  The newest one
     is the *current owner*; the earlier ones are *historical authors* (a
     takeover stays visible instead of silently replacing the real author).
  2. Is the candidate ``--reviewer`` **any** of those candidates?  => SELF_REVIEW
     and rc=1.  This is deliberately fail-closed: a bookkeeping
     ``status{in-progress}`` (R-card keep-alive) must never make the tool tell
     the real patch author "OK, you may review your own card".
  3. Is the target ``--worktree`` busy?  (a python/cargo/rustc process whose
     command line mentions the path or the worktree leaf name, or a non-empty
     ``git status --porcelain``) => BUSY and rc=1.  When the process table
     cannot be read the tool prints ``busy_processes=unavailable`` and returns
     rc=2 — unknown is never reported as "idle".

Only entries with ``verified is True`` are trusted; a missing ``verified`` key
counts as unverified (project rule "不采信未验证条目").

Usage (run from anywhere inside the repo):

  python -I tools/hygiene/review_principal_check.py --card <id> --reviewer <principal>
  python -I tools/hygiene/review_principal_check.py --card <id> --reviewer <p> --worktree <path>
  python -I tools/hygiene/review_principal_check.py --card <id>            # authors only
  python -I tools/hygiene/review_principal_check.py --selftest
  python -I tools/hygiene/review_principal_check.py --help

Exit codes (the three states are always distinguishable by rc alone):
  0  ok / UNCHECKED (authors printed, no --reviewer given)
  1  SELF_REVIEW (reviewer is one of the candidate authors) or BUSY
  2  ledger / worktree / process table unreadable (fail-closed), bad arguments,
     undecodable child output, or any unexpected failure
"""

import collections
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

# 卡 `openmir2-entry-shape-consumer-followups`（N1）：读 collab 条目的形状判定只留一处
# （`tools/collab/entryrefs.py`），本工具不再自己写 `item.get("entry") or {}`。
sys.path.insert(0, os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "collab"))
from entryrefs import EntryRefError, entry_of  # noqa: E402

PROCESS_TIMEOUT = 20.0
BUSY_PROCESS_PREFIXES = ("python", "cargo", "rustc")


_RunResult = collections.namedtuple("_RunResult", "returncode stdout stderr")


def _run(argv, cwd=None, timeout=PROCESS_TIMEOUT):
    """Run ``argv``; **never raises** — every failure becomes a nonzero rc.

    Output is returned as **bytes** (``text=False``).  Decoding is deliberately
    left to the caller so authoritative input is never silently repaired:

      * :func:`ledger_thread` decodes **strict UTF-8**.  ``errors="replace"``
        would turn an undecodable byte into U+FFFD, which is perfectly legal
        inside a JSON string: ``json.loads`` would succeed with a poisoned
        author list and answer "OK" to the real author (reviewer I-2).
      * process-table / worktree readers use :func:`_text`
        (``errors="replace"``).  Those values are only compared and printed,
        and a garbled table already degrades to "unavailable" (rc=2) — never
        to "idle" — so replacement cannot flip a conclusion.

    Internal rcs (all nonzero => callers report rc=2):
      124 timeout, 125 unexpected exception, 126 child error,
      127 cannot execute the program.
    """
    try:
        done = subprocess.run(argv, cwd=cwd, capture_output=True,
                              timeout=timeout, check=False)
        stdout = done.stdout if isinstance(done.stdout, bytes) else b""
        stderr = done.stderr if isinstance(done.stderr, bytes) else b""
        return _RunResult(done.returncode, stdout, stderr)
    except subprocess.TimeoutExpired:
        return _RunResult(124, b"", "timeout after %ss: %s" % (timeout, argv[0]))
    except OSError as exc:
        return _RunResult(127, b"", "cannot execute %r: %s" % (argv[0], exc))
    except (subprocess.SubprocessError, ValueError) as exc:
        return _RunResult(126, b"", "child error from %r: %s" % (argv[0], exc))
    except Exception as exc:  # noqa: BLE001 — fail-closed, never traceback
        return _RunResult(125, b"", "unexpected failure running %r: %s: %s"
                          % (argv[0], type(exc).__name__, exc))


def _text(data, errors="replace"):
    """Decode child output for display/comparison; **never raises**.

    ``errors="replace"`` is allowed here only because these consumers cannot
    turn a replaced character into a verdict: the ledger (the authoritative
    input) uses strict decoding in :func:`ledger_thread`.
    """
    if isinstance(data, bytes):
        return data.decode("utf-8", errors)
    return data if isinstance(data, str) else ""


class LedgerShapeError(Exception):
    """线程视图里有**形状不合法**的条目（既不是 `{entry: …}` 也不是 entry 本身）。

    卡 `openmir2-entry-shape-consumer-followups`（N1）：旧实现 `item.get("entry") or {}`
    会把这种条目当成「空条目」静默跳过——作者列表因此可能少人，且看不出来。现在一律
    fail-loud：`evaluate()` 把它变成 rc=2 并点名位置/principal。
    """


def _entry_or_shape_error(item, idx):
    """`entry_of(item)`；形状不合法 ⇒ 抛 `LedgerShapeError`（带 entries[i] 与 principal）。"""
    try:
        return entry_of(item)
    except EntryRefError as ex:
        who = []
        if isinstance(item, dict):
            who.append("principal=%r" % (item.get("principal"),))
            inner = item.get("entry")
            if isinstance(inner, dict):
                who.append("entry.id=%r" % (inner.get("id"),))
        raise LedgerShapeError(
            "entries[%d] %s：%s" % (idx, " ".join(who) or "（非 dict）", ex)) from ex


def author_from_entries(entries):
    """Return (authors, unverified_skipped, malformed_skipped) from entries.

    ``authors`` lists every **distinct** principal that signed a ``patch`` or an
    ``in-progress`` ``status`` entry, in thread order (first appearance wins, so
    a principal that keeps writing does not flood the list).  The last element
    is the current owner; **all** of them are forbidden reviewers.

    A ``review`` / ``needs-review`` / ``closed`` entry never contributes an
    author.  Entries whose ``verified`` key is not exactly ``True`` (including a
    missing key) are skipped and counted as ``unverified_skipped``.

    A verified patch/claim whose ``actor`` is not a non-blank string (``123``,
    ``null``, a list) is corrupt data: it is **never** accepted as an author and
    is counted as ``malformed_skipped``.  If every candidate is malformed the
    author list is empty and :func:`report` answers ``UNKNOWN`` / rc=2, so a
    corrupt actor can never turn a self-review into "OK".
    """
    authors = []
    seen = set()
    skipped = 0
    malformed = 0
    for idx, item in enumerate(entries):
        if item.get("verified") is not True:
            skipped += 1
            continue
        # N1：形状不合法 ⇒ `LedgerShapeError`（不再 `or {}` 静默丢弃整条）
        entry = _entry_or_shape_error(item, idx)
        body = entry.get("body") or {}
        kind = entry.get("kind")
        relevant = kind == "patch" or (
            kind == "status" and body.get("status") == "in-progress")
        if not relevant:
            continue
        actor = entry.get("actor")
        if not isinstance(actor, str) or not actor.strip():
            malformed += 1
            continue
        actor = actor.strip()
        if actor not in seen:
            seen.add(actor)
            authors.append(actor)
    return authors, skipped, malformed


_BRANCH_RE = re.compile(r"^[A-Za-z0-9._/-]+$")


def clean_branch(value):
    """Return a plausible branch name from a ledger ``branch`` field, or None.

    Real ledgers contain free text in ``branch`` (e.g. ``card/x（**force-with-lease
    推送**…）`` or ``card/y（已推 walgit）``).  Taking such a value verbatim makes
    the "possibly used by another card" hint fire on the card's own branch, so
    we keep only the first token before whitespace / full-width or ASCII
    brackets and require it to look like a ref name.  Unusable values are
    skipped (callers keep looking for an earlier clean one).
    """
    if not isinstance(value, str):
        return None
    token = re.split(r"[\s（(，,]", value.strip(), 1)[0].strip()
    if not token or token.startswith(("-", "/", ".")) or ".." in token:
        return None
    if token.endswith("/") or not _BRANCH_RE.match(token):
        return None
    return token


def declared_branch_from_entries(entries):
    """Return the branch this card declared in the ledger, or ``None``.

    The newest verified ``patch`` entry wins: that is the lane that actually
    delivered the work.  Only when the card has no patch at all do we look at
    the newest ``status{in-progress}`` claim.  Post-merge bookkeeping statuses
    (R-card keep-alive) routinely declare ``master`` and must not be mistaken
    for the card branch.

    A card that never declared one yields ``None`` — callers then refrain from
    drawing a conclusion instead of inventing ``card/<id>``.
    """
    patch_branch = None
    status_branch = None
    for idx, item in enumerate(entries):
        if item.get("verified") is not True:
            continue
        entry = _entry_or_shape_error(item, idx)
        body = entry.get("body") or {}
        kind = entry.get("kind")
        value = clean_branch(body.get("branch"))
        if value is None:
            continue
        if kind == "patch":
            patch_branch = value
        elif kind == "status" and body.get("status") == "in-progress":
            status_branch = value
    return patch_branch or status_branch


def decode_ledger_stdout(raw):
    """Strictly decode ledger bytes; return ``(text, error)``.

    The ledger is the authoritative input, so undecodable bytes are an
    operational error (the caller turns it into rc=2) — never U+FFFD.
    """
    if isinstance(raw, bytes):
        try:
            return raw.decode("utf-8"), None
        except UnicodeDecodeError as exc:
            return None, "ledger output is not valid UTF-8: %s" % exc
    if isinstance(raw, str):
        return raw, None
    return None, "ledger output has unexpected type %s" % type(raw).__name__


def ledger_thread(repo, card):
    """Return (entries, error); ``error`` is a message when unreadable."""
    probe = _run(
        ["walgit", "collab", "thread", "--repo", repo, card],
        cwd=repo if repo else None,
    )
    if probe.returncode != 0:
        return None, "cannot read ledger thread %s (rc=%d): %s" % (
            card,
            probe.returncode,
            _text(probe.stderr or probe.stdout).strip()[:200],
        )
    text, decode_error = decode_ledger_stdout(probe.stdout)
    if decode_error:
        return None, "cannot read ledger thread %s: %s" % (card, decode_error)
    try:
        data = json.loads(text)
    except ValueError as exc:
        return None, "ledger thread %s is not JSON: %s" % (card, exc)
    if not isinstance(data, list):
        return None, "ledger thread %s has unexpected shape (%s)" % (
            card,
            type(data).__name__,
        )
    return data, None


def _norm(path):
    return os.path.realpath(path).replace("\\", "/").rstrip("/").lower()


def _cmd_matches(want, leaf, cmd):
    """True when ``cmd`` names the worktree, by full path or by leaf name.

    Full path covers the usual ``python <script> <worktree>`` shape.  The leaf
    name ("openmir2-wt-xxx") covers commands launched from outside with the
    path spelled differently (short path, mapped drive, trailing separator).
    A command that only carries the path in its *cwd* is still invisible — see
    the "已知限制" section of the doc.
    """
    norm = cmd.replace("\\", "/").lower()
    if want and want in norm:
        return True
    if len(leaf) >= 4:
        pattern = r"(^|[^a-z0-9._-])" + re.escape(leaf) + r"([^a-z0-9._-]|$)"
        if re.search(pattern, norm):
            return True
    return False


def _process_table():
    """Return (rows, status); status is "ok" or "unavailable".

    ``rows`` is [(pid, ppid, name, cmdline)].  Empty rows with status "ok"
    means the table was read fine (nothing matched the query); "unavailable"
    means we could not read it at all and callers must not treat that as
    "nothing is running".
    """
    powershell = shutil.which("powershell") or shutil.which("pwsh")
    if powershell:
        script = (
            "Get-CimInstance Win32_Process | "
            "Select-Object ProcessId,ParentProcessId,Name,CommandLine | "
            "ConvertTo-Json -Compress"
        )
        proc = _run(
            [powershell, "-NoProfile", "-NonInteractive", "-Command", script],
            timeout=40,
        )
        if proc.returncode == 0:
            text = _text(proc.stdout).strip()
            if text:
                try:
                    data = json.loads(text)
                except ValueError:
                    data = None
                if isinstance(data, dict):
                    data = [data]
                if isinstance(data, list):
                    rows = []
                    for item in data:
                        if not isinstance(item, dict):
                            continue
                        try:
                            rows.append((
                                int(item.get("ProcessId")),
                                int(item.get("ParentProcessId") or 0),
                                str(item.get("Name") or ""),
                                str(item.get("CommandLine") or ""),
                            ))
                        except (TypeError, ValueError):
                            continue
                    return rows, "ok"
    ps = shutil.which("ps")
    if ps:
        proc = _run([ps, "-eo", "pid=,ppid=,comm=,args="])
        if proc.returncode == 0:
            rows = []
            for line in _text(proc.stdout).splitlines():
                parts = line.split(None, 3)
                if len(parts) < 3:
                    continue
                try:
                    rows.append((int(parts[0]), int(parts[1]), parts[2],
                                 parts[3] if len(parts) > 3 else ""))
                except ValueError:
                    continue
            return rows, "ok"
    return [], "unavailable"


def _self_and_ancestors(rows):
    """PIDs of this process and its ancestors (their cmdlines mention the path too)."""
    by_pid = {pid: ppid for pid, ppid, _name, _cmd in rows}
    seen = set()
    pid = os.getpid()
    while pid and pid not in seen:
        seen.add(pid)
        pid = by_pid.get(pid, 0)
    return seen


def busy_processes(worktree, table=None):
    """Return (hits, status) for python/cargo/rustc processes in ``worktree``.

    ``table`` is a test seam: pass ``(rows, status)`` to avoid reading the live
    process table.  This process plus its ancestors are always skipped — their
    own command lines mention the path (the checker is invoked *with* it).
    """
    if table is None:
        rows, status = _process_table()
    else:
        rows, status = table
    if status != "ok":
        return [], status
    want = _norm(worktree)
    leaf = want.rsplit("/", 1)[-1]
    skip = _self_and_ancestors(rows)
    hits = []
    for pid, _ppid, name, cmd in rows:
        if pid in skip:
            continue
        if not name.lower().startswith(BUSY_PROCESS_PREFIXES):
            continue
        if _cmd_matches(want, leaf, cmd):
            hits.append((str(pid), name, cmd.strip()[:160]))
    return hits, "ok"


def worktree_state(worktree):
    """Return dict(branch, last_commit, dirty, error) for ``worktree``.

    ``branch`` is ``None`` for a detached HEAD (``git rev-parse
    --abbrev-ref HEAD`` returns the literal ``HEAD`` there, which is not a
    branch name).
    """
    state = {"branch": None, "last_commit": None, "dirty": None, "error": None}
    if not os.path.isdir(worktree):
        state["error"] = "not a directory"
        return state
    inside = _run(["git", "-C", worktree, "rev-parse", "--is-inside-work-tree"])
    if inside.returncode != 0 or _text(inside.stdout).strip() != "true":
        state["error"] = "not a git worktree"
        return state
    branch = _run(["git", "-C", worktree, "rev-parse", "--abbrev-ref", "HEAD"])
    name = _text(branch.stdout).strip()
    state["branch"] = None if name in ("", "HEAD") else name
    stamp = _run(["git", "-C", worktree, "log", "-1", "--format=%h %cI"])
    state["last_commit"] = _text(stamp.stdout).strip() or None
    status = _run(["git", "-C", worktree, "status", "--porcelain"])
    if status.returncode != 0:
        state["error"] = "git status failed"
        return state
    state["dirty"] = bool(_text(status.stdout).strip())
    return state


_REVIEWER_SEPARATORS = re.compile(r"^[\s,;/|、，；]+$")
_REVIEWER_PLACEHOLDERS = frozenset(("none", "null", "n/a", "na", "nil", "tbd"))  # carried F2: placeholder literals are not a reviewer
_PRINCIPAL_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9-]*")  # a principal is letters/digits/dash; anything else is an annotation bug


def normalize_reviewer(value):
    """Canonical reviewer principal, or ``None`` when the value is unusable.

    ``--reviewer "$R"`` with an unset ``R`` passes ``""``; whitespace-only and
    separator-only values (``", "``, ``" ; "``) are the same caller bug.  If
    they were treated as "a reviewer was given", ``reviewer in authors`` would
    be false forever and the tool would tell the **real author** "OK" — the
    exact self-review fail-open this tool exists to prevent (reviewer I-1).
    """
    if not isinstance(value, str):
        return None
    token = value.strip()
    if not token or _REVIEWER_SEPARATORS.match(token):
        return None
    if token.lower() in _REVIEWER_PLACEHOLDERS:
        return None
    if not _PRINCIPAL_RE.fullmatch(token):
        # "a,b" / "a/b" / "openmir2-worker-a（作者）" — annotated or multi-value
        # caller bugs.  Treating them as "a reviewer was given" would answer OK
        # to the real author, so they are unusable ⇒ INVALID_REVIEWER (F2).
        return None
    return token


def report(card, authors, skipped, reviewer, worktree,
           declared_branch=None, out=sys.stdout, table=None, malformed=0):
    """Print the findings and return the exit code (never touches the ledger).

    ``authors`` is the distinct candidate-author list in thread order; the last
    element is the current owner.  ``SELF_REVIEW`` is a membership test against
    the whole list — never a single-name comparison (fail-closed).
    """
    print("card=%s" % card, file=out)
    if skipped:
        print("unverified_entries_skipped=%d" % skipped, file=out)
    if malformed:
        print("malformed_entries_skipped=%d (verified patch/in-progress entry "
              "with a non-string or blank actor)" % malformed, file=out)
    if not authors:
        print("author=<none>  (no verified patch/in-progress status entry)", file=out)
        print("verdict=UNKNOWN", file=out)
        return 2

    owner = authors[-1]
    print("author=%s" % owner, file=out)
    if len(authors) > 1:
        print("history_authors=%s" % ",".join(authors[:-1]), file=out)

    rc = 0
    review_verdict = "UNCHECKED"
    if reviewer is None:
        print("reviewer=<not given>", file=out)
        print("review_verdict=UNCHECKED (pass --reviewer to check SELF_REVIEW)",
              file=out)
    else:
        reviewer = normalize_reviewer(reviewer)
        if reviewer is None:
            print("reviewer=<invalid>", file=out)
            print("review_verdict=INVALID_REVIEWER", file=out)
            print("verdict=INVALID_REVIEWER", file=out)
            return 2
        print("reviewer=%s" % reviewer, file=out)
        if reviewer in authors:
            review_verdict = "SELF_REVIEW"
            where = (
                "current owner (newest verified patch/in-progress signer)"
                if reviewer == owner
                else "historical author (earlier verified signer in this thread)"
            )
            print("self_review_match=%s (%s)" % (reviewer, where), file=out)
            rc = 1
        else:
            review_verdict = "OK"
            if malformed:
                # carried F2 invariant: a corrupt (skipped) entry must never
                # yield "OK" — the dropped claim could be the reviewer's own.
                review_verdict = "MALFORMED_SKIPPED"
                rc = 2
                print("warn: malformed_entries_skipped=%d => 不能给 OK"
                      "（fail-closed，rc=2）" % malformed, file=out)
        print("review_verdict=%s" % review_verdict, file=out)

    if worktree is None:
        print("verdict=%s" % review_verdict, file=out)
        return rc

    print("worktree=%s" % worktree, file=out)
    if len(_norm(worktree)) < 4:
        print("worktree_error=path too short to compare meaningfully", file=out)
        print("verdict=WORKTREE_UNREADABLE", file=out)
        return 2
    state = worktree_state(worktree)
    if state["error"]:
        print("worktree_error=%s" % state["error"], file=out)
        print("verdict=WORKTREE_UNREADABLE", file=out)
        return 2
    print("branch=%s" % (state["branch"] or "<detached>"), file=out)
    print("last_commit=%s" % (state["last_commit"] or "<unknown>"), file=out)
    procs, status = busy_processes(worktree, table=table)
    if status == "unavailable":
        print("busy_processes=unavailable", file=out)
        print("warn: 进程表读不到 => 不能声称空闲（fail-closed，rc=2）", file=out)
    else:
        print("busy_processes=%d" % len(procs), file=out)
        for pid, name, cmd in procs:
            print("  pid=%s name=%s cmd=%s" % (pid, name, cmd), file=out)
    print("worktree_dirty=%s" % ("yes" if state["dirty"] else "no"), file=out)

    if state["branch"] is None:
        print("branch_hint=skip (detached HEAD)", file=out)
    elif declared_branch:
        if state["branch"] != declared_branch:
            print("declared_branch=%s" % declared_branch, file=out)
            print("hint: current branch %s != 账本声明分支 %s => 可能被其它卡占用"
                  % (state["branch"], declared_branch), file=out)
    else:
        print("branch_hint=skip (账本未声明分支，不下占用结论)", file=out)

    if procs or state["dirty"]:
        print("worktree_verdict=BUSY (进程/脏工作树)", file=out)
        print("verdict=BUSY", file=out)
        return 1
    if status == "unavailable":
        print("verdict=PROCESS_TABLE_UNAVAILABLE", file=out)
        return 2
    print("verdict=%s" % review_verdict, file=out)
    return rc


def evaluate(repo, card, reviewer, worktree, out=sys.stdout, read_thread=None,
             err=sys.stderr):
    """Read the ledger, then run the checks; return the exit code.

    ``read_thread`` is a test seam returning ``(entries, error)``; production
    always uses :func:`ledger_thread`.  Ledger errors are operational errors
    (rc=2), never ``SELF_REVIEW``/``BUSY`` (rc=1).
    """
    read = read_thread or ledger_thread
    try:
        entries, message = read(repo, card)
    except Exception as exc:  # noqa: BLE001 — any reader failure is operational
        entries, message = None, "ledger reader raised %s: %s" % (
            type(exc).__name__, exc)
    if message:
        print("review-principal check: %s" % message, file=err)
        return 2
    try:
        authors, skipped, malformed = author_from_entries(entries)
        declared = declared_branch_from_entries(entries)
    except LedgerShapeError as ex:
        # N1：形状不合法是**判不了**，不是「这条没有作者」——必须 rc=2 且点名，
        # 否则一个坏条目会安静地从作者列表里消失（自审检查因此可能放行）。
        print("review-principal check: 线程视图里有形状不合法的条目：%s" % ex, file=err)
        return 2
    return report(
        card,
        authors,
        skipped,
        reviewer,
        worktree,
        declared_branch=declared,
        out=out,
        malformed=malformed,
    )


def _fixture(kind, actor, body=None, verified=True):
    entry = {"kind": kind, "actor": actor}
    if body is not None:
        entry["body"] = body
    item = {"entry": entry}
    if verified is not None:  # None => omit the key entirely
        item["verified"] = verified
    return item


def selftest():
    """Synthetic fixtures only; no real card, ledger or worktree is required.

    Every rc assertion goes through report()/evaluate() — the same functions
    the CLI uses — so breaking the production decision changes the test's exit
    code (not just its text).
    """
    same = [
        _fixture("patch", "openmir2-worker-x"),
        _fixture("status", "openmir2-worker-x", {"status": "needs-review"}),
    ]
    diff = [
        _fixture("patch", "openmir2-worker-x"),
        _fixture("status", "openmir2-reviewer-y", {"status": "needs-review"}),
    ]
    takeover = [
        _fixture("patch", "openmir2-worker-a"),
        _fixture("patch", "openmir2-worker-b"),
    ]
    # The M-7 shape: a real patch author followed by an orchestrator keep-alive
    # status{in-progress}.  The owner becomes the orchestrator, but the real
    # patch author must still be refused as a reviewer.
    keepalive = [
        _fixture("patch", "openmir2-worker-63"),
        _fixture("status", "openmir2-orchestrator", {"status": "in-progress"}),
    ]
    unverified_false = [
        _fixture("patch", "openmir2-worker-z", verified=False),
        _fixture("patch", "openmir2-worker-a"),
    ]
    unverified_missing = [
        _fixture("patch", "openmir2-worker-z", verified=None),
        _fixture("patch", "openmir2-worker-a"),
    ]

    checks = []

    authors_same, _, _ = author_from_entries(same)
    buf = io.StringIO()
    rc_same = report("openmir2-synthetic-card", authors_same, 0,
                     "openmir2-worker-x", None, out=buf)
    checks.append((
        "① reviewer == author => SELF_REVIEW (rc=1，经 report())",
        authors_same == ["openmir2-worker-x"] and rc_same == 1
        and "verdict=SELF_REVIEW" in buf.getvalue(),
    ))

    authors_diff, _, _ = author_from_entries(diff)
    buf = io.StringIO()
    rc_diff = report("openmir2-synthetic-card", authors_diff, 0,
                     "openmir2-reviewer-y", None, out=buf)
    checks.append((
        "② reviewer != author => OK (rc=0)；needs-review 的 status 不是作者",
        authors_diff == ["openmir2-worker-x"] and rc_diff == 0
        and "verdict=OK" in buf.getvalue(),
    ))

    authors_take, _, _ = author_from_entries(takeover)
    checks.append((
        "③ 接管：owner=最新签名 patch 作者，历史作者按首次出现去重保留",
        authors_take == ["openmir2-worker-a", "openmir2-worker-b"],
    ))

    clean_table = ([], "ok")
    with tempfile.TemporaryDirectory(prefix="review-principal-check-") as tmp:
        _run(["git", "init", "-q"], cwd=tmp)
        with open(os.path.join(tmp, "dirty.txt"), "w", encoding="utf-8") as fh:
            fh.write("x\n")
        hit_procs, hit_status = busy_processes(tmp, table=clean_table)
        rc_busy = report("openmir2-synthetic-card", ["openmir2-worker-a"], 0,
                         "openmir2-reviewer-b", tmp, out=io.StringIO(),
                         table=clean_table)
        checks.append((
            "④ 脏 worktree => BUSY 且 rc=1（端到端经 report()）",
            hit_procs == [] and hit_status == "ok" and rc_busy == 1,
        ))

    with tempfile.TemporaryDirectory(prefix="review-principal-clean-") as clean:
        _run(["git", "init", "-q"], cwd=clean)
        buf = io.StringIO()
        rc_unavail = report("openmir2-synthetic-card", ["openmir2-worker-a"], 0,
                            "openmir2-reviewer-b", clean, out=buf,
                            table=([], "unavailable"))
        checks.append((
            "⑤ 进程表不可读 => WARN + busy_processes=unavailable + rc=2",
            rc_unavail == 2 and "busy_processes=unavailable" in buf.getvalue()
            and "busy_processes=0" not in buf.getvalue(),
        ))

    authors_false, skipped_false, _ = author_from_entries(unverified_false)
    authors_missing, skipped_missing, _ = author_from_entries(unverified_missing)
    checks.append((
        "⑥ verified=false 的 patch 不采信",
        authors_false == ["openmir2-worker-a"] and skipped_false == 1,
    ))
    checks.append((
        "⑦ verified 缺键 => 默认不采信（fail-closed）",
        authors_missing == ["openmir2-worker-a"] and skipped_missing == 1,
    ))

    def _broken(repo, card):
        return None, "cannot read ledger thread %s (synthetic)" % card

    rc_broken = evaluate(".", "openmir2-synthetic-card", "openmir2-reviewer-b",
                         None, out=io.StringIO(), read_thread=_broken,
                         err=io.StringIO())
    checks.append((
        "⑧ 账本读不到 => rc=2（经 evaluate()，与 0/1 可区分）",
        rc_broken == 2,
    ))

    authors_keep, _, _ = author_from_entries(keepalive)
    buf = io.StringIO()
    rc_keep = report("openmir2-synthetic-card", authors_keep, 0,
                     "openmir2-worker-63", None, out=buf)
    checks.append((
        "⑨ 保活 status 顶掉 owner 时，真实 patch 作者仍判 SELF_REVIEW (rc=1)",
        authors_keep == ["openmir2-worker-63", "openmir2-orchestrator"]
        and rc_keep == 1 and "historical author" in buf.getvalue(),
    ))

    with tempfile.TemporaryDirectory(prefix="review-principal-detached-") as tmp:
        _run(["git", "init", "-q"], cwd=tmp)
        _run(["git", "-c", "user.email=t@example.invalid", "-c", "user.name=t",
              "commit", "-q", "--allow-empty", "-m", "x"], cwd=tmp)
        _run(["git", "checkout", "-q", "--detach"], cwd=tmp)
        detached = worktree_state(tmp)
    checks.append((
        "⑩ detached HEAD => branch=<detached>（不打印 branch=HEAD）",
        detached["branch"] is None and detached["error"] is None,
    ))

    with tempfile.TemporaryDirectory(prefix="review-principal-branch-") as tmp:
        _run(["git", "init", "-q"], cwd=tmp)
        _run(["git", "-c", "user.email=t@example.invalid", "-c", "user.name=t",
              "commit", "-q", "--allow-empty", "-m", "x"], cwd=tmp)
        actual = worktree_state(tmp)["branch"]

        def _hint(declared):
            buf = io.StringIO()
            report("openmir2-synthetic-card", ["openmir2-worker-a"], 0,
                   "openmir2-reviewer-b", tmp, declared_branch=declared,
                   out=buf, table=clean_table)
            return buf.getvalue()

        match_out = _hint(actual)
        other_out = _hint("worktree-other-card")
        none_out = _hint(None)
    checks.append((
        "⑪ 占用提示按账本声明分支比对：相同不提示 / 不同才提示 / 未声明不下结论",
        "hint:" not in match_out
        and "hint:" in other_out
        and "hint:" not in none_out
        and "branch_hint=skip" in none_out,
    ))

    # ⑫ _run never raises; an exception from the ledger reader is rc=2
    timed = _run([sys.executable, "-c", "import time; time.sleep(3)"], timeout=0.3)
    missing = _run(["definitely-not-a-real-binary-openmir2"], timeout=1)

    def _raising(repo, card):
        raise RuntimeError("boom")

    rc_raise = evaluate(".", "openmir2-synthetic-card", "openmir2-reviewer-b",
                        None, out=io.StringIO(), read_thread=_raising,
                        err=io.StringIO())
    checks.append((
        "⑫ _run 超时/缺命令不抛异常（rc=124/127）；reader 抛异常 => evaluate rc=2",
        timed.returncode == 124 and missing.returncode == 127 and rc_raise == 2,
    ))

    # ⑬ M-2 regression: free text in the ledger's branch field must not be
    # mistaken for a branch name (that made the "possibly used" hint fire on
    # the card's own branch).
    annotated = [
        _fixture("patch", "openmir2-worker-a",
                 {"branch": "card/attack-weapon-wear-642（**force-with-lease 推送**…）"}),
        _fixture("patch", "openmir2-worker-b",
                 {"branch": "card/weaponstrong-dura-zero（已推 walgit）"}),
    ]
    checks.append((
        "⑬ branch 带中文注释 => 只取首个 token（不把说明当分支名）",
        declared_branch_from_entries(annotated) == "card/weaponstrong-dura-zero",
    ))

    # ⑭ ``_run`` now hands back **bytes** (no reader-thread text=True decode,
    # so stdout can never become None); only the display helper replaces bytes.
    raw = _run([sys.executable, "-c",
                "import sys;sys.stdout.buffer.write(bytes([255,254,129]))"],
               timeout=10)
    checks.append((
        "⑭ 子进程输出按字节返回；展示侧 _text 才 replace（权威账本侧严格解码）",
        raw.returncode == 0 and isinstance(raw.stdout, bytes)
        and b"\xff\xfe\x81" in raw.stdout
        and isinstance(_text(raw.stdout), str)
        and "\ufffd" in _text(raw.stdout),
    ))

    # ⑮ outermost guard: operational failures (incl. print/encoding errors)
    # are rc=2, never rc=1.
    def _boom():
        raise RuntimeError("synthetic unexpected failure")

    guard_rc = guarded(_boom, err=io.StringIO())
    guard_pass = guarded(lambda: 7)
    checks.append((
        "⑮ guarded()：意外异常 => rc=2，正常返回原样透传",
        guard_rc == 2 and guard_pass == 7,
    ))

    # ⑯ main()'s outermost guarded() wiring.  Patching the module-level
    # ``evaluate`` (looked up at call time) means deleting ``guarded(...)`` from
    # main leaks the exception here: this check goes red instead of surviving.
    real_evaluate = evaluate

    def _explode(*_args, **_kwargs):
        raise RuntimeError("synthetic evaluate failure")

    globals()["evaluate"] = _explode
    rc_main = None
    try:
        try:
            rc_main = main(["--card", "openmir2-synthetic-card"],
                          err=io.StringIO())
        except BaseException:  # noqa: BLE001 — unguarded main leaked it
            rc_main = None
    finally:
        globals()["evaluate"] = real_evaluate
    checks.append((
        "⑯ main() 外层 guarded 真的接在 evaluate 上：抛异常 => rc=2（拆掉 guarded 立即红）",
        rc_main == 2,
    ))

    # ⑰ empty / blank / separator-only reviewer must be rc=2, never OK.
    invalid_rcs = []
    invalid_outs = []
    for bad in ("", "   ", "\t", ",", " ; ", "、"):
        buf = io.StringIO()
        invalid_rcs.append(report("openmir2-synthetic-card",
                                  ["openmir2-worker-a"], 0, bad, None, out=buf))
        invalid_outs.append(buf.getvalue())
    checks.append((
        "⑰ 空串/纯空白/仅分隔符 reviewer => INVALID_REVIEWER rc=2（不是 OK）",
        invalid_rcs == [2] * 6
        and all("verdict=OK" not in text for text in invalid_outs)
        and all("review_verdict=INVALID_REVIEWER" in text for text in invalid_outs),
    ))

    # ⑰b carried F2: placeholder literals, annotated and multi-value reviewers
    # are unusable too (they must not reach verdict=OK).
    invalid2_rcs = []
    invalid2_outs = []
    for bad in ("None", "null", "n/a", "openmir2-worker-a（作者）", "a,b", "a/b"):
        buf = io.StringIO()
        invalid2_rcs.append(report("openmir2-synthetic-card",
                                   ["openmir2-worker-a"], 0, bad, None, out=buf))
        invalid2_outs.append(buf.getvalue())
    checks.append((
        "⑰b 占位字面/带注记/多值 reviewer => INVALID_REVIEWER rc=2（carried F2）",
        invalid2_rcs == [2] * 6
        and all("verdict=OK" not in text for text in invalid2_outs)
        and all("review_verdict=INVALID_REVIEWER" in text for text in invalid2_outs),
    ))

    # ⑰c carried F2 invariant: a corrupt (skipped) claim entry must never
    # produce verdict=OK — the dropped entry could be the reviewer's own.
    corrupt_thread = [
        _fixture("patch", 123),
        _fixture("patch", "openmir2-worker-b"),
    ]

    def _corrupt(repo, card):
        # _fixture already wraps each entry as {"entry": ..., "verified": ...}.
        return corrupt_thread, None

    buf = io.StringIO()
    rc_corrupt = evaluate(".", "openmir2-synthetic-card",
                          "openmir2-reviewer-b", None, out=buf,
                          read_thread=_corrupt, err=io.StringIO())
    checks.append((
        "⑰c 损坏条目（actor=123）被跳过 ⇒ 不得给 OK（rc=2，carried F2 不变量）",
        rc_corrupt == 2
        and "verdict=MALFORMED_SKIPPED" in buf.getvalue()
        and "malformed_entries_skipped=1" in buf.getvalue(),
    ))

    # ⑱ normalization: a padded principal is the same principal.
    checks.append((
        '⑱ --reviewer 首尾空白归一（" p " == "p"），否则真审查者会被判成 OK',
        normalize_reviewer("  openmir2-reviewer-y  ") == "openmir2-reviewer-y"
        and normalize_reviewer("") is None
        and normalize_reviewer(" , ") is None,
    ))

    # ⑲ M-2: a non-string actor is corrupt data, never an author.
    bad_actors = [_fixture("patch", 123), _fixture("patch", None),
                  _fixture("status", "", {"status": "in-progress"})]
    authors_bad, _skipped_bad, malformed_bad = author_from_entries(bad_actors)
    buf = io.StringIO()
    rc_bad = report("openmir2-synthetic-card", authors_bad, _skipped_bad,
                    "openmir2-reviewer-b", None, out=buf,
                    malformed=malformed_bad)
    checks.append((
        "⑲ 非字符串/空白 actor 不采信（计入 malformed；无作者 => UNKNOWN rc=2）",
        authors_bad == [] and malformed_bad == 3 and rc_bad == 2
        and "malformed_entries_skipped=3" in buf.getvalue(),
    ))

    # ⑳ I-2 unit level: the ledger decoder is strict and never returns text
    # for undecodable bytes (the end-to-end rc=2 is exercised with a fake
    # walgit in the card evidence).
    decoded, reason = decode_ledger_stdout(
        b'[{"entry":{"kind":"patch","actor":"openmir2-worker-a\xff"}}]')
    checks.append((
        "⑳ 账本严格 UTF-8：不可解码 => 不返回文本（走 rc=2 通道）",
        decoded is None and bool(reason)
        and decode_ledger_stdout(b"[]") == ("[]", None),
    ))

    # ㉑ N1（卡 `openmir2-entry-shape-consumer-followups` A2）：malformed thread 视图
    #     ⇒ `author_from_entries` **必须抛**（旧实现 `or {}` 会静默跳过它）；
    #     经 `evaluate` 时必须 rc=2 且点名 principal/位置——不是「没有作者」。
    malformed_view = [{"oid": "f" * 40, "principal": "openmir2-worker-q", "verified": True}]
    try:
        author_from_entries(malformed_view)
        raise_shape = False
    except LedgerShapeError as ex:
        raise_shape = "entries[0]" in str(ex) and "openmir2-worker-q" in str(ex)

    def _fake_read(_repo, _card):
        return malformed_view, None

    err_buf = io.StringIO()
    out_buf = io.StringIO()
    rc_shape = evaluate(".", "openmir2-synthetic-card", "openmir2-reviewer-b", None,
                        out=out_buf, read_thread=_fake_read, err=err_buf)
    checks.append((
        "㉑ malformed thread 视图 ⇒ 抛 LedgerShapeError（不静默丢弃）",
        raise_shape,
    ))
    checks.append((
        "㉒ malformed thread 视图 ⇒ evaluate rc=2 且点名 principal（不是 UNKNOWN/跳过）",
        rc_shape == 2 and "openmir2-worker-q" in err_buf.getvalue()
        and "author=" not in out_buf.getvalue(),
    ))

    failed = 0
    for name, ok in checks:
        print(("PASS " if ok else "FAIL ") + name)
        failed += 0 if ok else 1
    print("selftest: %d/%d passed" % (len(checks) - failed, len(checks)))
    return 0 if failed == 0 else 1


def guarded(fn, err=sys.stderr):
    """Run ``fn``; an unexpected exception becomes rc=2 instead of a traceback.

    Outermost fail-closed boundary for failures the ledger/worktree seams do
    not model (for example a console encoding error while printing the
    Chinese hint).  rc=1 is reserved for ``SELF_REVIEW`` / ``BUSY``, so an
    operational failure must never surface as 1.
    """
    try:
        return fn()
    except Exception as exc:  # noqa: BLE001
        print("review-principal check: unexpected failure: %s: %s"
              % (type(exc).__name__, exc), file=err)
        return 2


def main(argv, err=sys.stderr):
    if "-h" in argv or "--help" in argv:
        print(__doc__.strip())
        return 0
    if "--selftest" in argv:
        return selftest()

    opts = {"--card": None, "--reviewer": None, "--worktree": None, "--repo": "."}
    rest = list(argv)
    while rest:
        arg = rest.pop(0)
        if arg in opts:
            if not rest:
                print("review-principal check: %s needs a value" % arg, file=err)
                return 2
            opts[arg] = rest.pop(0)
        else:
            print("review-principal check: unknown argument %r" % arg, file=err)
            return 2
    card = (opts["--card"] or "").strip()
    if not card:
        print("review-principal check: --card is required (or --selftest)",
              file=err)
        return 2
    reviewer = opts["--reviewer"]
    if reviewer is not None:
        canonical = normalize_reviewer(reviewer)
        if canonical is None:
            print("review-principal check: --reviewer %r is empty/blank/"
                  "separator-only: bad argument (rc=2); an unset shell "
                  "variable must not look like a reviewer" % reviewer,
                  file=err)
            return 2
        reviewer = canonical
    return guarded(
        lambda: evaluate(opts["--repo"], card, reviewer, opts["--worktree"]),
        err=err)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
