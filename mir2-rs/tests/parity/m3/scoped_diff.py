#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""scoped_diff.py — M3 对拍：把 DB 差异限定到"本次运行碰过的行"，并做字段级比对。

为什么不用 dbsnap.py diff 直接比：`mir2_db` 是活库，整表 diff 会被无关行淹没，
也分不清"夹具没生效"和"实现有差异"。M3 的做法是**每次运行用全新前缀**
（账号/角色名 = <prefix><序号>），于是：

  extract  <snapdir> <prefix> <out.jsonl>   抽取该次运行命名空间内的行
  delta    <before.jsonl> <after.jsonl>     同一次运行的"改动清单"（基线产物）
  diff     <a.jsonl> <b.jsonl>              两次运行之间的字段级比对（M3 判据；有差异即红）
  selftest                                  改坏必红自检

抽取规则：行内出现 `<prefix>` 字节即归属本次运行（账号/角色名、以及以角色名为键的表）。
归一化：`--normalize` 打开时，把 <prefix> 统一替换为 `<RUN>`，并把角色名里的序号保留
（序号是确定性命名的一部分，必须比）。

输出 JSONL，每行：{"table": "...", "row_sha256": "...", "line": "..."}
"""
import argparse
import hashlib
import json
import os
import sys

DEFAULT_PREFIX_MASK = "<RUN>"


def iter_tables(snapdir):
    man_path = os.path.join(snapdir, "manifest.json")
    if not os.path.exists(man_path):
        print(f"RED: 快照目录缺 manifest.json: {snapdir}", file=sys.stderr)
        sys.exit(2)
    with open(man_path, "r", encoding="utf-8") as f:
        manifest = json.load(f)
    for t in manifest["tables"]:
        yield t["table"], os.path.join(snapdir, t["file"])


def cmd_extract(args):
    prefix = args.prefix.encode("utf-8", "replace")
    out_rows = 0
    with open(args.out, "w", encoding="utf-8", newline="\n") as fo:
        for table, path in iter_tables(args.snapdir):
            with open(path, "rb") as f:
                for raw in f:
                    if prefix not in raw:
                        continue
                    line = raw.rstrip(b"\r\n")
                    text = line.decode("utf-8", "replace")
                    if args.normalize:
                        text = text.replace(args.prefix, DEFAULT_PREFIX_MASK)
                    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
                    fo.write(json.dumps({"table": table, "row_sha256": digest, "line": text},
                                        ensure_ascii=False) + "\n")
                    out_rows += 1
    print(f"EXTRACT prefix={args.prefix} rows={out_rows} -> {args.out}")
    if out_rows == 0 and not args.allow_empty:
        print("RED: 该前缀在快照里 0 行——夹具没生效（或快照取错时机）", file=sys.stderr)
        sys.exit(3)


def load_rows(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            rows.append(json.loads(line))
    return rows


def bucket(rows):
    by_table = {}
    for r in rows:
        by_table.setdefault(r["table"], []).append(r)
    return by_table


def cmd_diff(args):
    a = bucket(load_rows(args.a))
    b = bucket(load_rows(args.b))
    problems = 0
    summary = {}
    for table in sorted(set(a) | set(b)):
        ra, rb = a.get(table, []), b.get(table, [])
        ha = sorted(r["row_sha256"] for r in ra)
        hb = sorted(r["row_sha256"] for r in rb)
        only_a = [h for h in ha if h not in set(hb)]
        only_b = [h for h in hb if h not in set(ha)]
        summary[table] = {"rows_a": len(ra), "rows_b": len(rb),
                          "only_a": len(only_a), "only_b": len(only_b)}
        if ha != hb:
            problems += len(only_a) + len(only_b)
            print(f"DIFF {table}: rows A={len(ra)} B={len(rb)} 仅A={len(only_a)} 仅B={len(only_b)}")
            if problems <= args.max_diffs:
                # 给一行具体差异（便于定位）
                la = next((r["line"] for r in ra if r["row_sha256"] in only_a), "<none>")
                lb = next((r["line"] for r in rb if r["row_sha256"] in only_b), "<none>")
                print(f"    A: {la[:220]}")
                print(f"    B: {lb[:220]}")
    if args.summary_out:
        with open(args.summary_out, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"summary": summary, "mismatch_hashes": problems}, f,
                      ensure_ascii=False, indent=2)
    if problems:
        print(f"RED: 作用域内 {problems} 行不一致")
        sys.exit(1)
    print(f"GREEN: 作用域内 {len(a)} 张表逐行一致")
    sys.exit(0)


def cmd_delta(args):
    """同一次运行的 before/after：列出新增/消失的行（基线产物，不做判红）。"""
    a = bucket(load_rows(args.before))
    b = bucket(load_rows(args.after))
    delta = {}
    for table in sorted(set(a) | set(b)):
        ha = sorted(r["row_sha256"] for r in a.get(table, []))
        hb = sorted(r["row_sha256"] for r in b.get(table, []))
        added = [h for h in hb if h not in set(ha)]
        removed = [h for h in ha if h not in set(hb)]
        if added or removed:
            delta[table] = {"added": len(added), "removed": len(removed)}
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        json.dump(delta, f, ensure_ascii=False, indent=2, sort_keys=True)
    total = sum(v["added"] + v["removed"] for v in delta.values())
    print(f"DELTA 改动行合计={total} 表数={len(delta)} -> {args.out}")
    for t, v in sorted(delta.items()):
        print(f"  {t}: +{v['added']} -{v['removed']}")
    sys.exit(0)


def cmd_selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="scoped-diff-selftest-")
    a = os.path.join(tmp, "a.jsonl")
    b = os.path.join(tmp, "b.jsonl")
    with open(a, "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps({"table": "t", "row_sha256": "aa", "line": "m3bot1\t1"}, ensure_ascii=False) + "\n")
        f.write(json.dumps({"table": "t", "row_sha256": "bb", "line": "m3bot2\t2"}, ensure_ascii=False) + "\n")
    with open(b, "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps({"table": "t", "row_sha256": "aa", "line": "m3bot1\t1"}, ensure_ascii=False) + "\n")
        f.write(json.dumps({"table": "t", "row_sha256": "bb", "line": "m3bot2\t2"}, ensure_ascii=False) + "\n")
    ns = argparse.Namespace(a=a, b=b, max_diffs=5, summary_out=None)
    try:
        cmd_diff(ns)
        print("PASS 1/2 相同作用域判绿")
    except SystemExit as e:
        if e.code != 0:
            print("RED: 相同内容却判红", file=sys.stderr)
            sys.exit(1)
    with open(b, "a", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps({"table": "t", "row_sha256": "cc", "line": "m3bot3\t9"}, ensure_ascii=False) + "\n")
    try:
        cmd_diff(ns)
        print("RED: 多出一行仍判绿——红检失效", file=sys.stderr)
        sys.exit(1)
    except SystemExit as e:
        if e.code != 1:
            print("RED: 差异退出码异常", file=sys.stderr)
            sys.exit(1)
    print("PASS 2/2 多一行即判红（改坏必红成立）")
    print("GREEN: scoped_diff selftest 2/2")
    sys.exit(0)


def main():
    ap = argparse.ArgumentParser(description="M3 作用域内 DB 差异比对")
    sub = ap.add_subparsers(dest="cmd", required=True)

    e = sub.add_parser("extract")
    e.add_argument("snapdir")
    e.add_argument("prefix")
    e.add_argument("out")
    e.add_argument("--normalize", action="store_true", help="把 prefix 归一化为 <RUN>")
    e.add_argument("--allow-empty", action="store_true",
                   help="允许 0 行（运行**前**的快照按定义就是 0 行，别当失败）")

    d = sub.add_parser("delta")
    d.add_argument("before")
    d.add_argument("after")
    d.add_argument("--out", required=True)

    f = sub.add_parser("diff")
    f.add_argument("a")
    f.add_argument("b")
    f.add_argument("--max-diffs", type=int, default=5)
    f.add_argument("--summary-out")

    sub.add_parser("selftest")
    args = ap.parse_args()
    {"extract": cmd_extract, "delta": cmd_delta, "diff": cmd_diff, "selftest": cmd_selftest}[args.cmd](args)


if __name__ == "__main__":
    main()
