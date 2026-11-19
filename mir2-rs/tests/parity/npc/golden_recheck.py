#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""NPC golden 重录复核（卡 card/npc-parity；opc-07 纪律 2026-11-20）。

重录不是免检操作：探针新增元数据字段后重录，必须证明**行为字段一个字节都没变**。

用法：
  python -I tests/parity/npc/golden_recheck.py before.json after.json
  python -I tests/parity/npc/golden_recheck.py selftest

判定：
- 剥离的元数据键（递归）：`script_name`、`script_sha256`、`fixture`；
- 其余整棵树必须逐字节相等 ⇒ rc=0 打印 BEHAVIOR_IDENTICAL；
- 有任何差异 ⇒ rc=1，逐条打印 JSON 路径；
- 文件缺失/解析失败/零 cases ⇒ rc=2（PREREQ，不判绿）。
"""
import json
import sys

METADATA_KEYS = {"script_name", "script_sha256", "fixture"}


def strip_metadata(node):
    if isinstance(node, dict):
        return {k: strip_metadata(v) for k, v in node.items() if k not in METADATA_KEYS}
    if isinstance(node, list):
        return [strip_metadata(v) for v in node]
    return node


def walk(path, a, b, out):
    if type(a) is not type(b):
        out.append((path, a, b))
        return
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a or k not in b:
                out.append((path + "/" + k, a.get(k, "<missing>"), b.get(k, "<missing>")))
            else:
                walk(path + "/" + k, a[k], b[k], out)
        return
    if isinstance(a, list):
        if len(a) != len(b):
            out.append((path + "/len", len(a), len(b)))
        for i in range(min(len(a), len(b))):
            walk("%s/%d" % (path, i), a[i], b[i], out)
        return
    if a != b:
        out.append((path, a, b))


def load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def compare(before, after):
    cases = after.get("cases") if isinstance(after, dict) else None
    if not cases:
        return 2, ["case=[]（零用例不得判绿）"]
    a, b = strip_metadata(before), strip_metadata(after)
    diffs = []
    walk("", a, b, diffs)
    return (0 if not diffs else 1), ["%s: %r != %r" % d for d in diffs]


def selftest():
    base = {"cases": [{"name": "x", "rm": [{"ident": 11009}], "sm": [{"ident": 643}],
                       "state": {"last_npc_is_npc": True}}]}
    meta_only = json.loads(json.dumps(base))
    meta_only["cases"][0]["script_name"] = "老人"
    meta_only["cases"][0]["script_sha256"] = "ab" * 32
    meta_only["fixture"] = {"source_path": "x", "sha256": "cd" * 32}
    bad = json.loads(json.dumps(base))
    bad["cases"][0]["sm"][0]["ident"] = 644
    rc1, _ = compare(base, meta_only)
    rc2, _ = compare(base, bad)
    rc3, _ = compare(base, {"cases": []})
    ok = (rc1 == 0) and (rc2 == 1) and (rc3 == 2)
    print("selftest: metadata-only=%d behavior-change=%d empty=%d => %s" %
          (rc1, rc2, rc3, "OK" if ok else "FAIL"))
    return 0 if ok else 1


def main(argv):
    if len(argv) == 2 and argv[1] == "selftest":
        return selftest()
    if len(argv) != 3:
        sys.stderr.write("usage: golden_recheck.py before.json after.json | selftest\n")
        return 2
    try:
        before, after = load(argv[1]), load(argv[2])
    except (OSError, ValueError) as e:
        sys.stderr.write("PREREQ: %s\n" % e)
        return 2
    rc, lines = compare(before, after)
    if rc == 0:
        print("BEHAVIOR_IDENTICAL（仅元数据新增；行为字段逐字节相等）")
    elif rc == 2:
        print("PREREQ_FAIL: " + lines[0])
    else:
        print("BEHAVIOR_CHANGED（PASS 作废，须按新 golden 重判）：")
        for line in lines:
            print("  " + line)
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv))
