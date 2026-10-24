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

Usage (run from anywhere inside the repo):

  python -I tools/hygiene/stale_pointer_gate.py                  # scan docs/
  python -I tools/hygiene/stale_pointer_gate.py --path docs/m2   # narrow
  python -I tools/hygiene/stale_pointer_gate.py --ref refs/remotes/walgit/master
  python -I tools/hygiene/stale_pointer_gate.py --selftest       # synthetic repo
  python -I tools/hygiene/stale_pointer_gate.py --fingerprint docs/foo.md:123

Exit codes: 0 clean (or selftest passed); 1 stale pointer(s) found, allow-list
rot, or an unreadable scan target (fail-closed); 2 usage error / not inside a
git repository / target ref or allow-list missing (fail-closed).
"""

from __future__ import annotations

import argparse
import hashlib
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


def scan(top, rel_path, ref, allowlist, max_gap):
    """Scan ``rel_path``; return a dict with hits/exemptions/errors."""
    files = markdown_files(top, rel_path)
    allow_index = {(e["path"], e["line_sha256"]): e for e in allowlist}
    result = {
        "files": len(files),
        "lines": 0,
        "hits": [],
        "unresolved": [],
        "marker_exempt": 0,
        "allow_exempt": 0,
        "green_pointers": 0,
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
            if not candidates:
                continue
            if CORRECTION_RE.search(line):
                result["marker_exempt"] += 1
                continue
            fingerprint = line_fingerprint(line)
            snippet = line.strip()
            if len(snippet) > SNIPPET_LIMIT:
                snippet = snippet[: SNIPPET_LIMIT - 1] + "…"
            resolved = [(token, word, ancestor(token)) for token, word, _gap in candidates]
            # An allow-list entry may only suppress a line that would actually be
            # reported (i.e. at least one hash is an ancestor); otherwise it has
            # rotted and must be removed.  The same goes for "resolved to nothing
            # stale" lines: no entry is consumed, so rot detection stays honest.
            if not any(state for _t, _w, state in resolved):
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

    # Rot detection only applies to entries inside the scanned subtree — a
    # narrowed --path must not flag allow-list entries that were simply not
    # looked at.
    prefix = rel_path.rstrip("/")
    in_scope = [
        e for e in allowlist if e["path"] == prefix or e["path"].startswith(prefix + "/")
    ]
    result["rot"] = [e for e in in_scope if not e["used"]]
    result["rc"] = 1 if (result["hits"] or result["errors"] or result["rot"]) else 0
    return result


# -------------------------------------------------------------------------- output
def emit(result, ref, allowlist_path, out=sys.stdout, err=sys.stderr):
    for hit in result["hits"]:
        print(
            "%s:%d  %s  %s" % (hit["path"], hit["line"], hit["hash"], hit["snippet"]),
            file=out,
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
            "line(s), %d unresolved; ref=%s, allow-list=%s)"
            % (
                result["files"],
                result["lines"],
                result["green_pointers"],
                result["marker_exempt"],
                result["allow_exempt"],
                len(result["unresolved"]),
                ref,
                allowlist_path,
            ),
            file=out,
        )
    else:
        print(
            "stale-pointer gate: FAIL (%d stale pointer(s), %d allow-list rot, "
            "%d error(s); ref=%s)"
            % (len(result["hits"]), len(result["rot"]), len(result["errors"]), ref),
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
        _run_git(repo, "add", "-A")
        _run_git(repo, "commit", "-qm", "fixtures")

        empty_allowlist = os.path.join(tmp, "empty.toml")
        _write(empty_allowlist, "# no exemptions\n")

        allowlist_path = os.path.join(tmp, "allow.toml")
        _write(
            allowlist_path,
            '[[allow]]\npath = "docs/allow/allowed.md"\n'
            'line_sha256 = "%s"\nreason = "selftest fixture"\n'
            % line_fingerprint(allowed_line),
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
        help="max characters between a pointer and an unfinished word (default: 24)",
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

    ref = resolve_ref(top, args.ref)
    rel_path = repo_relative(top, os.path.join(top, args.path))
    if not os.path.exists(os.path.join(top, rel_path)):
        raise GateError("stale-pointer gate: --path %r does not exist" % args.path, rc=2)

    allowlist_path = args.allowlist or os.path.join(top, DEFAULT_ALLOWLIST)
    allowlist = load_allowlist(allowlist_path)

    try:
        result = scan(top, rel_path, ref, allowlist, args.max_gap)
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
