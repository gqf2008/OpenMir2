#!/usr/bin/env python3
"""conflict_marker_gate — fail if a tracked file contains a merge-conflict marker.

Card: openmir2-idea-workspace-marker-cleanup (tier=L).

Scans the files git knows about (``git ls-files -z`` — the index, so an
``--intent-to-add`` copy is covered too) and reports every line that begins a
conflict block:

  * seven ``<`` followed by a space or end of line   (ours)
  * seven ``>`` followed by a space or end of line   (theirs)
  * exactly seven ``=`` on its own line              (base separator)

The ``=`` separator is only reported when the same file also carries an
ours/theirs marker: a bare run of seven ``=`` is legal Markdown (an H1
underline), so requiring the conflict context keeps the gate honest on prose.

Marker patterns are assembled at runtime (``b"<" * 7`` and friends) so this
file never matches itself and needs no allow-list entry.

Usage (run from anywhere inside the repo):

  python -I tools/hygiene/conflict_marker_gate.py             # rc=0 clean / 1 dirty
  python -I tools/hygiene/conflict_marker_gate.py --selftest  # red/green self-check
  python -I tools/hygiene/conflict_marker_gate.py --help

Exit codes: 0 clean (or selftest passed); 1 markers found, a tracked file was
unreadable (fail-closed), or selftest failed; 2 not inside a git repository.
"""

import os
import re
import subprocess
import sys
import tempfile

# Built dynamically: the literals never appear in this source, so the gate
# scans its own file clean without an exception list.
_OURS = re.compile(rb"^<{7}(?: |$)")
_THEIRS = re.compile(rb"^>{7}(?: |$)")
_BASE = re.compile(rb"^={7}$")

_BINARY_PROBE = 8192


def scan_text(data):
    """Return sorted [(lineno, line_bytes)] of conflict-marker lines in ``data``."""
    hard = []
    base = []
    for lineno, raw in enumerate(data.split(b"\n"), 1):
        line = raw.rstrip(b"\r")
        if _OURS.match(line) or _THEIRS.match(line):
            hard.append((lineno, line))
        elif _BASE.match(line):
            base.append((lineno, line))
    return sorted(hard + (base if hard else []))


def scan_file(path):
    """Return conflict-marker hits in one file; binary bodies are skipped."""
    with open(path, "rb") as fh:
        head = fh.read(_BINARY_PROBE)
        if b"\0" in head:
            return []
        data = head + fh.read()
    return scan_text(data)


def repo_root_and_files():
    probe = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, check=False
    )
    if probe.returncode != 0:
        raise SystemExit("conflict-marker gate: not inside a git repository")
    top = os.fsdecode(probe.stdout).strip()
    listing = subprocess.run(
        ["git", "-C", top, "ls-files", "-z"], capture_output=True, check=True
    )
    paths = [os.fsdecode(p) for p in listing.stdout.split(b"\0") if p]
    return top, paths


def selftest():
    with tempfile.TemporaryDirectory(prefix="conflict-marker-gate-") as tmp:

        def write(name, payload):
            path = os.path.join(tmp, name)
            with open(path, "wb") as fh:
                fh.write(payload)
            return path

        clean = write("clean.txt", b"hello\nworld\n")
        ours = write("ours.txt", b"a\n" + b"<" * 7 + b" HEAD\n")
        theirs = write("theirs.txt", b"a\n" + b">" * 7 + b" topic\n")
        block = write(
            "block.xml",
            b"x\n"
            + b"<" * 7 + b" HEAD\n"
            + b"=" * 7 + b"\n"
            + b"n\n"
            + b">" * 7 + b" other\n",
        )
        markdown = write("doc.md", b"Title\n" + b"=" * 7 + b"\n")
        binary = write("blob.bin", b"\x00\x01\n" + b"<" * 7 + b" HEAD\n")

        checks = [
            ("clean text is green", scan_file(clean) == []),
            ("ours marker is red", len(scan_file(ours)) == 1),
            ("theirs marker is red", len(scan_file(theirs)) == 1),
            ("full block is red and names 3 lines",
             [n for n, _ in scan_file(block)] == [2, 3, 5]),
            ("bare '=' (markdown H1 underline) stays green", scan_file(markdown) == []),
            ("binary body is skipped", scan_file(binary) == []),
        ]

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

    top, paths = repo_root_and_files()
    total = 0
    unreadable = 0
    for rel in paths:
        try:
            hits = scan_file(os.path.join(top, rel))
        except OSError as exc:
            print("ERROR cannot read %s: %s" % (rel, exc), file=sys.stderr)
            unreadable += 1
            continue
        for lineno, _line in hits:
            print("%s:%d" % (rel, lineno))
            total += 1

    if total or unreadable:
        print(
            "conflict-marker gate: FAIL (%d marker line(s) across %d tracked files, "
            "%d unreadable)" % (total, len(paths), unreadable),
            file=sys.stderr,
        )
        return 1
    print("conflict-marker gate: OK (0 markers across %d tracked files)" % len(paths))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
