#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dbsnap.py — MySQL 快照与字段级比对（C 线工具 ④）。

快照：对每张表做确定性导出（ORDER BY 全列），一表一个 .tsv + manifest.json
（表名/行数/sha256）。比对：两份快照逐表逐行 diff，字段级不一致即红。

不依赖任何 Python MySQL 驱动——直接调 mysql.exe --batch（MariaDB 客户端）。

用法：
  python dbsnap.py snap --out DIR [--table mir2_db.characters ...] [--mysql PATH] [--host ..] [--user ..] [--password ..]
  python dbsnap.py diff A_DIR B_DIR [--max-diffs 10]
  python dbsnap.py selftest            # 改坏必红自检（快照→篡改→diff 必须红）

默认表集：mir2_account.account / account_protection + mir2_db.characters* 全族
（对应设计文档 §5.1 的角色数据边界）。
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys

DEFAULT_TABLES = [
    "mir2_account.account",
    "mir2_account.account_protection",
    "mir2_db.characters",
    "mir2_db.characters_ablity",
    "mir2_db.characters_bonusability",
    "mir2_db.characters_item",
    "mir2_db.characters_item_attr",
    "mir2_db.characters_magic",
    "mir2_db.characters_bagitem",
    "mir2_db.characters_storageitem",
    "mir2_db.characters_status",
    "mir2_db.characters_quest",
    "mir2_db.characters_indexes",
]

DEFAULT_MYSQL = r"D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe"


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def run_mysql(args, sql: str, capture=True):
    cmd = [args.mysql, "-h", args.host, "-P", str(args.port),
           "-u", args.user, f"--password={args.password}",
           "--batch", "--raw", "--skip-column-names", "-e", sql]
    r = subprocess.run(cmd, capture_output=capture, timeout=120)
    if r.returncode != 0:
        print(f"RED: mysql 执行失败: {r.stderr.decode('utf-8', 'replace')[:300]}", file=sys.stderr)
        sys.exit(2)
    return r.stdout


def column_count(args, db: str, table: str) -> int:
    out = run_mysql(args, "SELECT COUNT(*) FROM information_schema.COLUMNS "
                          f"WHERE TABLE_SCHEMA='{db}' AND TABLE_NAME='{table}'")
    return int(out.decode("utf-8", "replace").strip().splitlines()[0])


def cmd_snap(args):
    os.makedirs(args.out, exist_ok=True)
    manifest = {"tool": "dbsnap", "tables": []}
    for fq in args.table:
        db, _, table = fq.partition(".")
        if not db or not table:
            print(f"RED: 表名须为 db.table: {fq}", file=sys.stderr)
            sys.exit(2)
        ncols = column_count(args, db, table)
        order = ",".join(str(i + 1) for i in range(ncols))
        out_path = os.path.join(args.out, f"{db}.{table}.tsv")
        data = run_mysql(args, f"SELECT * FROM `{db}`.`{table}` ORDER BY {order}")
        with open(out_path, "wb") as f:
            f.write(data)
        rows = 0 if not data.strip() else data.count(b"\n")
        digest = sha256_file(out_path)
        manifest["tables"].append({"table": fq, "rows": rows, "sha256": digest,
                                   "file": os.path.basename(out_path)})
        print(f"SNAP {fq}: rows={rows} sha256={digest[:16]}…")
    with open(os.path.join(args.out, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
    print(f"SNAP_OK {len(manifest['tables'])} 表 -> {args.out}")


def load_manifest(d: str):
    with open(os.path.join(d, "manifest.json"), "r", encoding="utf-8") as f:
        return json.load(f)


def cmd_diff(args):
    ma = load_manifest(args.a)
    mb = load_manifest(args.b)
    ta = {t["table"]: t for t in ma["tables"]}
    tb = {t["table"]: t for t in mb["tables"]}
    problems = 0
    for name in sorted(set(ta) | set(tb)):
        if name not in ta:
            print(f"DIFF {name}: 仅 B 有"); problems += 1; continue
        if name not in tb:
            print(f"DIFF {name}: 仅 A 有"); problems += 1; continue
        a, b = ta[name], tb[name]
        # 以文件实况为准（不信 manifest 里的旧 hash——快照文件可能被事后改过）
        pa = os.path.join(args.a, a["file"])
        pb = os.path.join(args.b, b["file"])
        if not os.path.exists(pa) or not os.path.exists(pb):
            print(f"DIFF {name}: 缺 tsv 文件"); problems += 1; continue
        ha, hb = sha256_file(pa), sha256_file(pb)
        tampered = []
        if ha != a["sha256"]:
            tampered.append("A 侧文件与自身 manifest 不符（快照被事后改动）")
        if hb != b["sha256"]:
            tampered.append("B 侧文件与自身 manifest 不符（快照被事后改动）")
        if ha == hb and not tampered:
            continue
        problems += 1
        for t in tampered:
            print(f"TAMPER {name}: {t}")
        if ha != hb:
            print(f"DIFF {name}: 内容不一致（rows A={a['rows']} B={b['rows']}）")
        if problems <= args.max_diffs:
            # 逐行对：找第一个不一致行
            with open(pa, "rb") as f:
                la = f.read().splitlines()
            with open(pb, "rb") as f:
                lb = f.read().splitlines()
            for i in range(max(len(la), len(lb))):
                ra = la[i] if i < len(la) else b"<missing>"
                rb = lb[i] if i < len(lb) else b"<missing>"
                if ra != rb:
                    print(f"  首个差异行 #{i+1}:")
                    print(f"    A: {ra[:200]}")
                    print(f"    B: {rb[:200]}")
                    break
    if problems:
        print(f"RED: {problems} 张表不一致")
        sys.exit(1)
    print(f"GREEN: {len(ta)} 张表字段级一致")
    sys.exit(0)


def cmd_selftest(args):
    """改坏必红：连拍两次必须绿；篡改快照一个字节 → diff 必须红。"""
    import tempfile
    tmp = tempfile.mkdtemp(prefix="dbsnap-selftest-")
    a = os.path.join(tmp, "a")
    b = os.path.join(tmp, "b")
    ns = argparse.Namespace(**vars(args))
    ns.out = a
    cmd_snap(ns)
    ns.out = b
    cmd_snap(ns)
    # 1. 一致 → 绿
    ns2 = argparse.Namespace(a=a, b=b, max_diffs=5)
    try:
        cmd_diff(ns2)
        ok = True
    except SystemExit as e:
        ok = (e.code == 0)
    if not ok:
        print("RED: 连拍两次不一致（快照本身不确定！）")
        sys.exit(1)
    print("PASS 1/2 连拍一致判绿")
    # 2. 篡改 b 的一张表 → 必须红
    victim = os.path.join(b, "mir2_account.account.tsv")
    with open(victim, "ab") as f:
        f.write(b"\ttampered\n")
    try:
        cmd_diff(ns2)
        print("RED: 篡改快照后仍判绿——红检失效")
        sys.exit(1)
    except SystemExit as e:
        if e.code != 1:
            print("RED: 篡改后退出码异常:", e.code)
            sys.exit(1)
    print("PASS 2/2 篡改快照后判红（改坏必红成立）")
    print("GREEN: dbsnap selftest 2/2")
    sys.exit(0)


def main():
    ap = argparse.ArgumentParser(description="MySQL 快照与比对")
    ap.add_argument("--mysql", default=DEFAULT_MYSQL, help="mysql.exe 路径")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=3306)
    ap.add_argument("--user", default="root")
    ap.add_argument("--password", default="")
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("snap")
    s.add_argument("--out", required=True)
    s.add_argument("--table", action="append", help="db.table，可多次；默认 mir2 账号/角色全族")
    d = sub.add_parser("diff")
    d.add_argument("a")
    d.add_argument("b")
    d.add_argument("--max-diffs", type=int, default=10)
    sub.add_parser("selftest")
    args = ap.parse_args()
    if not os.path.exists(args.mysql):
        print(f"RED: 找不到 mysql 客户端: {args.mysql}", file=sys.stderr)
        sys.exit(2)
    if getattr(args, "cmd") in ("snap", "selftest"):
        if not getattr(args, "table", None):
            args.table = DEFAULT_TABLES
    {"snap": cmd_snap, "diff": cmd_diff, "selftest": cmd_selftest}[args.cmd](args)


if __name__ == "__main__":
    main()
