#!/usr/bin/env python
# -*- coding: utf-8 -*-
"""collect.py — M3 tiered-acceptance 卡（openmir2-tiered-acceptance-policy）仓库侧落地的自证采集器。

只读 `refs/collab/*`（git cat-file --batch），不联网、不起栈、不占端口、不写 worktree 之外。
产出本目录下 01/02/04 三份原始件（00/03/05 为手工粘贴的命令与引文，见 00-commands.txt）。

用法（仓库根）：
    python -I mir2-rs/tests/parity/m3/evidence-m3-tiered-acceptance-20261013/collect.py
"""
import io
import json
import subprocess
import sys
import os
import datetime

HERE = os.path.dirname(os.path.abspath(__file__))


def load_entries():
    out = subprocess.run(
        ["git", "for-each-ref", "refs/collab/inbox/", "--format=%(objectname)"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    p = subprocess.run(["git", "cat-file", "--batch"],
                       input="\n".join(out).encode(), capture_output=True, check=True)
    buf = io.BytesIO(p.stdout)
    entries = []
    while True:
        h = buf.readline()
        if not h:
            break
        parts = h.split()
        if len(parts) < 3:
            continue
        size = int(parts[2])
        data = buf.read(size)
        buf.read(1)
        try:
            j = json.loads(data.decode("utf-8", "replace"))
        except Exception:
            continue
        e = j.get("entry", j)
        entries.append(e)
    return entries


def ts(e):
    return e.get("ts", 0)


def when(e):
    return datetime.datetime.utcfromtimestamp(ts(e)).strftime("%Y-%m-%d %H:%M:%S")


def body(e):
    return e.get("body", {}) or {}


def w(path, text):
    with open(os.path.join(HERE, path), "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def jsonl(e):
    return json.dumps({"oid": e.get("_oid"), "actor": e.get("actor"), "kind": e.get("kind"),
                       "card": e.get("id"), "ts": ts(e), "utc": when(e), "body": body(e)},
                      ensure_ascii=False)


def main():
    entries = load_entries()
    for e in entries:
        e["_oid"] = e.get("id")

    # ---- 01: runtime tier-field inventory (every entry carrying tier/tier_why) ----
    rows = []
    for e in sorted(entries, key=ts):
        b = body(e)
        s = json.dumps(b, ensure_ascii=False)
        if '"tier"' not in s and '"tier_why"' not in s and '"card_tier"' not in s:
            continue
        rows.append("\t".join([
            when(e), e.get("actor", ""), e.get("kind", ""), e.get("id", ""),
            str(b.get("tier", "")), str(b.get("card_tier", "")),
            str(b.get("decision", "")), str(b.get("window_required", "")),
            json.dumps(b.get("tier_why", ""), ensure_ascii=False),
        ]))
    w("01-runtime-tier-fields.txt",
      "ts_utc\tactor\tkind\tcard\ttier\tcard_tier\tdecision\twindow_required\ttier_why\n"
      + "\n".join(rows) + "\n")

    # ---- 02: R-tier cards merged but status != closed (real-machine/window DoD) ----
    lines = []
    for card in ("openmir2-m3-castle-gold-tax", "openmir2-m3-rng-trace-scene-groups"):
        card_e = [e for e in entries if e.get("id") == card]
        mr = [e for e in card_e if e.get("kind") == "merge_result"]
        st = [e for e in card_e if e.get("kind") == "status"]
        rv = [e for e in card_e if e.get("kind") == "review"
              and (body(e).get("tier") == "R" or body(e).get("card_tier") == "R")]
        lines.append("===== CARD %s =====" % card)
        if rv:
            lines.append("[R-tier review] " + jsonl(sorted(rv, key=ts)[-1]))
        if mr:
            lines.append("[merge_result] " + jsonl(sorted(mr, key=ts)[-1]))
        if st:
            latest = sorted(st, key=ts)[-1]
            b = body(latest)
            tiers = [(ts(e), body(e).get("card_tier") or (body(e).get("tier") if e.get("kind") in ("review", "merge_result") else None))
                     for e in card_e]
            tiers = [t for t in tiers if t[1]]
            card_tier = sorted(tiers)[-1][1] if tiers else None
            lines.append("[latest status] status=%s tier=%s card_tier(card-level)=%s" % (
                b.get("status"), b.get("tier"), card_tier))
            lines.append("[latest status raw] " + jsonl(latest))
            ok = (card_tier == "R") and (b.get("status") != "closed")
            lines.append("[SUMMARY acceptance②] card_tier=%s, latest_status=%s (closed=%s) => %s" % (
                card_tier, b.get("status"), b.get("status") == "closed",
                "HOLDS: R-tier card merged and NOT closed" if ok
                else "CHECK: see raw above"))
    w("02-r-tier-merge-not-closed.txt", "\n".join(lines) + "\n")

    # ---- 04: L-tier cards with reviewer's equivalence method + >=1 smoke reading ----
    want = [
        "openmir2-m3-guild-castle-fixture-botsrv",
        "openmir2-m3-pet-slave-summon",
        "openmir2-m3-selloff-consignment-world",
    ]
    lines = []
    for card in want:
        card_e = [e for e in entries if e.get("id") == card]
        rv = [e for e in card_e if e.get("kind") == "review"
              and body(e).get("decision") == "approve" and body(e).get("tier") == "L"]
        mr = [e for e in card_e if e.get("kind") == "merge_result"]
        lines.append("===== L-tier card %s =====" % card)
        if rv:
            lines.append("[L approve raw] " + jsonl(sorted(rv, key=ts)[-1]))
        else:
            lines.append("[L approve] NONE FOUND")
        if mr:
            lines.append("[merge_result raw] " + jsonl(sorted(mr, key=ts)[-1]))
    w("04-l-tier-smoke-sample.txt", "\n".join(lines) + "\n")

    print("entries=%d; wrote 01/02/04 to %s" % (len(entries), HERE))


if __name__ == "__main__":
    sys.exit(main())
