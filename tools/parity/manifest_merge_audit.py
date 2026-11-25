import argparse, json, os, subprocess, sys, tempfile

def load(source):
    if source.startswith("@file:"):
        with open(source[len("@file:"):], "rb") as f:
            doc = json.load(f)
    else:
        t = subprocess.run(["git", "show", source + ":docs/issues/parity-gap-magic.json"],
                           capture_output=True, text=True, check=True).stdout
        doc = json.loads(t)
    return doc

def fields(e):
    ev = e.get("evidence") or {}
    return {"scope_len": len(e.get("scope") or ""),
            "units": len(ev.get("unit_tests") or []),
            "cases": len(ev.get("cases") or []),
            "test": ev.get("test") or ""}

def audit(before_doc, after_doc):
    before, after = {e["id"]: e for e in before_doc["entries"]}, {e["id"]: e for e in after_doc["entries"]}
    bad = 0
    for k, eb in before.items():
        ea = after.get(k)
        if ea is None:
            print("MISSING_ENTRY", k); bad += 1; continue
        fb, fa = fields(eb), fields(ea)
        lost = [n for n in ("scope_len", "units", "cases") if fa[n] < fb[n]]
        if fb["test"] and not fa["test"]:
            lost.append("test")
        if lost:
            print("LOST", k, {n: (fb[n], fa[n]) for n in lost}); bad += 1
    print("summary: entries_before=%d entries_after=%d suspicious=%d" % (len(before), len(after), bad))
    return bad

def selftest():
    def entry(eid, scope, units, cases, test):
        return {"id": eid, "scope": scope, "evidence": {"test": test, "cases": cases, "unit_tests": units}}
    base = {"entries": [entry("A", "x" * 10, ["u1"], ["c1"], "t1"), entry("B", "y" * 4, [], [], "")]}
    shrunk = {"entries": [entry("A", "x" * 3, [], ["c1"], "t1"), entry("B", "y" * 4, [], [], "")]}
    with tempfile.TemporaryDirectory() as td:
        pb, ps = os.path.join(td, "before.json"), os.path.join(td, "after.json")
        json.dump(base, open(pb, "w", encoding="utf-8"))
        json.dump(shrunk, open(ps, "w", encoding="utf-8"))
        print("== selftest RED case (expect LOST A)")
        red = audit(json.load(open(pb, encoding="utf-8")), json.load(open(ps, encoding="utf-8")))
        print("== selftest GREEN case (same doc)")
        green = audit(json.load(open(pb, encoding="utf-8")), json.load(open(pb, encoding="utf-8")))
    ok = red == 1 and green == 0
    print("manifest_merge_audit selftest:", "OK" if ok else "FAIL", "(red=%d green=%d)" % (red, green))
    return 0 if ok else 2

ap = argparse.ArgumentParser(description="只读：比对两棵树里 parity-gap-magic 条目是否单向丢失")
ap.add_argument("--before"); ap.add_argument("--after"); ap.add_argument("--selftest", action="store_true")
a = ap.parse_args()
if a.selftest:
    sys.exit(selftest())
if not (a.before and a.after):
    ap.error("需要 --before/--after（或 --selftest）")
sys.exit(1 if audit(load(a.before), load(a.after)) else 0)
