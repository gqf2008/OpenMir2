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
CANON_PLACEHOLDER = "<VOLATILE>"

# ---------------------------------------------------------------------------
# 规范化（canonicalize）——M3 正式判据的组成部分（设计文档 §6 / owner 2026-10-11 裁决 A）
#
# 为什么需要：`row_sha256` 覆盖 SELECT * 的全列，而其中一部分列**每次运行必变**
# （自增 PK/FK、墙钟 datetime/unix 秒），另有一列是**夹具自身随机**（建角 Job）。
# 把这些列算进比对 ⇒ 两侧各自都 GREEN 也恒判 RED，判据无仲裁力。
#
# 口径：内容冻结（设计文档 §1 硬约束②）保证表结构不变 ⇒ 列序可静态登记；
# `diff --canonical` 只把**白名单列的列值**替换为占位符，其余列**逐字节比**。
# 白名单按「表 + 列名 + 为什么是易变列」逐列登记，**不做整表 ignore**；
# 遇到未登记的表直接判红（fail-closed），避免出现"悄悄不比"的静默缺口。
# ---------------------------------------------------------------------------
TABLE_COLUMNS = {
    "mir2_account.account": [
        "Id", "Account", "PassWord", "PassFailCount", "PassFailTime", "ValidFrom",
        "ValidUntil", "PayMode", "Seconds", "StopUntil", "State", "CreateTime",
        "ModifyTime", "LastLoginTime",
    ],
    "mir2_account.account_protection": [
        "AccountId", "UserName", "IdCard", "Birthday", "Phone", "MobilePhone",
        "ADDRESS1", "ADDRESS2", "EMail", "Quiz1", "Answer1", "Quiz2", "Answer2",
    ],
    "mir2_db.characters": [
        "Id", "ServerIndex", "LoginID", "ChrName", "MapName", "CX", "CY", "Level",
        "Dir", "Hair", "Sex", "Job", "Gold", "GamePoint", "HomeMap", "HomeX", "HomeY",
        "PkPoint", "ReLevel", "AttatckMode", "FightZoneDieCount", "BodyLuck", "IncHealth",
        "IncSpell", "IncHealing", "CreditPoint", "BonusPoint", "HungerStatus",
        "PayMentPoint", "LockLogon", "MarryCount", "AllowGroup", "AllowGroupReCall",
        "GroupRcallTime", "AllowGuildReCall", "IsMaster", "MasterName", "DearName",
        "StoragePwd", "Deleted", "CREATEDATE", "LASTUPDATE",
    ],
    "mir2_db.characters_indexes": [
        "Id", "Account", "ChrName", "SelectID", "IsDeleted", "CreateDate", "ModifyDate",
    ],
}

# 表 -> {列名: 为什么是易变列}。新增易变列必须同时给出理由（owner 裁决 A 的条件①）。
VOLATILE_COLUMNS = {
    "mir2_account.account": {
        "Id": "自增 PK（每次运行必变）",
        "ValidFrom": "墙钟 datetime（建号时刻）",
        "ValidUntil": "墙钟 datetime（建号时刻）",
        "StopUntil": "墙钟 datetime（建号时刻）",
        "CreateTime": "unix 秒（墙钟）",
        "ModifyTime": "unix 秒（墙钟）",
        "LastLoginTime": "unix 秒（墙钟）",
    },
    "mir2_account.account_protection": {
        "AccountId": "FK = account.Id（自增，每次运行必变）",
    },
    "mir2_db.characters": {
        "Id": "自增 PK（每次运行必变）",
        # 夹具建角输入随机：src/BotSrv/Scenes/Scene/SelectChrScene.cs:154-176
        #   sHair = Random(1)（恒 0 ⇒ 2）、sSex = Random(1)（恒 0）、sJob = Random(2)（0/1 随机）
        #   RandomNumber 无种子（src/OpenMir2/RandomNumber.cs:20 new Random()）
        # ⇒ 两侧输入本就不同，不能拿行值当行为判据。覆盖迁至固定参数场景：
        #   卡 openmir2-m3-botsrv-fixed-newchr（固定 Job/Hair/Sex，判据：该场景不置占位且 diff=0）
        "Hair": "夹具建角输入（BotSrv Random(1)，当前恒 2）——覆盖迁至 openmir2-m3-botsrv-fixed-newchr",
        "Sex": "夹具建角输入（BotSrv Random(1)，当前恒 0）——覆盖迁至 openmir2-m3-botsrv-fixed-newchr",
        "Job": "夹具建角输入（BotSrv Random(2)，0/1 随机）——覆盖迁至 openmir2-m3-botsrv-fixed-newchr",
        "CREATEDATE": "墙钟 datetime（建角时刻）",
        "LASTUPDATE": "墙钟 datetime（建角时刻）",
    },
    "mir2_db.characters_indexes": {
        "Id": "自增 PK（每次运行必变）",
        "CreateDate": "墙钟 datetime（建角时刻）",
        "ModifyDate": "墙钟 datetime（建角时刻）",
    },
}


def canonicalize_line(table, line):
    """把该表白名单列的列值替换为占位符，返回规范化后的整行（其余列逐字节不动）。"""
    cols = TABLE_COLUMNS.get(table)
    if cols is None:
        print(f"RED: --canonical 遇到未登记表 {table}——先在 scoped_diff.py 的 TABLE_COLUMNS/"
              f"VOLATILE_COLUMNS 里登记（含每列理由），不许悄悄不比", file=sys.stderr)
        sys.exit(4)
    fields = line.split("\t")
    if len(fields) != len(cols):
        print(f"RED: {table} 列数不符（快照 {len(fields)} vs 登记 {len(cols)}）——表结构已变或登记过期",
              file=sys.stderr)
        sys.exit(4)
    volatile = VOLATILE_COLUMNS.get(table, {})
    for i, name in enumerate(cols):
        if name in volatile:
            fields[i] = CANON_PLACEHOLDER
    return "\t".join(fields)


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
    if getattr(args, "canonical", False):
        for rows in (a, b):
            for table, rs in rows.items():
                for r in rs:
                    r["line"] = canonicalize_line(table, r["line"])
                    r["row_sha256"] = hashlib.sha256(r["line"].encode("utf-8")).hexdigest()
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


def _write_jsonl(path, rows):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        for line in rows:
            f.write(json.dumps({"table": "mir2_db.characters", "row_sha256": "x", "line": line},
                               ensure_ascii=False) + "\n")


def _chr_line(idv, job, hp):
    # 只填白名单关心的列，其余 39 列用占位常量；列序与 TABLE_COLUMNS 一致。
    cols = TABLE_COLUMNS["mir2_db.characters"]
    vals = {c: "0" for c in cols}
    vals.update({"Id": str(idv), "LoginID": "m3bot0", "ChrName": "m3bot0",
                 "MapName": "0", "Hair": "2", "Sex": "0", "Job": str(job),
                 "Gold": str(hp), "CREATEDATE": "2026-10-11 00:00:00",
                 "LASTUPDATE": "2026-10-11 00:00:00"})
    return "\t".join(vals[c] for c in cols)


def cmd_selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="scoped-diff-selftest-")
    a = os.path.join(tmp, "a.jsonl")
    b = os.path.join(tmp, "b.jsonl")

    def expect(name, ns, code):
        try:
            cmd_diff(ns)
            got = 0
        except SystemExit as e:
            got = e.code
        if got != code:
            print(f"RED: {name} 期望 exit {code} 实得 {got}", file=sys.stderr)
            sys.exit(1)
        print(f"PASS {name}（exit {code}）")

    base = ["m3bot1\t1", "m3bot2\t2"]
    # —— 用例 1/4：非规范化路径，原行为不变（相同→绿、多一行→红）
    with open(a, "w", encoding="utf-8", newline="\n") as f:
        for line in base:
            f.write(json.dumps({"table": "t", "row_sha256": hashlib.sha256(line.encode()).hexdigest(),
                                "line": line}, ensure_ascii=False) + "\n")
    with open(b, "w", encoding="utf-8", newline="\n") as f:
        for line in base:
            f.write(json.dumps({"table": "t", "row_sha256": hashlib.sha256(line.encode()).hexdigest(),
                                "line": line}, ensure_ascii=False) + "\n")
    ns = argparse.Namespace(a=a, b=b, max_diffs=5, summary_out=None, canonical=False)
    expect("1/4 未规范化：相同作用域判绿", ns, 0)
    with open(b, "a", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps({"table": "t", "row_sha256": "cc", "line": "m3bot3\t9"},
                           ensure_ascii=False) + "\n")
    expect("2/4 未规范化：多一行即判红", ns, 1)

    # —— 用例 3/4：--canonical 下，仅易变列（Id/Job/时间戳）不同 ⇒ 必须判绿
    _write_jsonl(a, [_chr_line(12234, 1, 0)])
    _write_jsonl(b, [_chr_line(13239, 0, 0)])
    ns_c = argparse.Namespace(a=a, b=b, max_diffs=5, summary_out=None, canonical=True)
    expect("3/4 规范化后：仅易变列不同判绿", ns_c, 0)

    # —— 用例 4/4（改坏必红）：--canonical 下，改动**被比对列**（Gold）⇒ 必须判红
    _write_jsonl(b, [_chr_line(13239, 0, 999)])
    expect("4/4 规范化后：被比对列被改坏即判红", ns_c, 1)

    # —— 用例 5/5：未登记表 fail-closed
    with open(b, "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps({"table": "mir2_db.unknown", "row_sha256": "x", "line": "1\t2"},
                           ensure_ascii=False) + "\n")
    expect("5/5 未登记表 fail-closed", ns_c, 4)

    print("GREEN: scoped_diff selftest 5/5（含规范化前后 + 改坏必红 + fail-closed）")
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
    f.add_argument("--canonical", action="store_true",
                   help="按易变列白名单规范化后再比（M3 正式门禁用；见设计文档 §6）")

    sub.add_parser("selftest")
    args = ap.parse_args()
    {"extract": cmd_extract, "delta": cmd_delta, "diff": cmd_diff, "selftest": cmd_selftest}[args.cmd](args)


if __name__ == "__main__":
    main()
