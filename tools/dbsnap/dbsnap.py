#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dbsnap.py — MySQL 快照与字段级比对（C 线工具 ④）。

快照：对每张表做确定性导出（ORDER BY 全列），一表一个 .tsv + manifest.json
（表名/行数/sha256）。比对：两份快照逐表逐行 diff，字段级不一致即红。

不依赖任何 Python MySQL 驱动——直接调 mysql.exe --batch（MariaDB 客户端）。

============================================================================
已登记的口径限制（务必先读；这是设计取舍，不是待修的 bug）
============================================================================
本项目 3 库（mir2_db / mir2_account / mir2_data）的字段值**不应**含 TAB(0x09)/LF(0x0A)；
本工具在该前提下工作，并把下列限制**显式登记**在工具头，绝不静默吞掉：

     snap 用 `mysql --batch --raw` 原样落盘（不转义），因此：
       · 值内含 TAB(0x09) ⇒ 该行被多切出一列（行字段数 **>** 列数）——**前提是该值不含 LF**；
         TAB 与 LF 组合时碎片行字段数可能**恰好=列数**（见下 S1/S2 的分工，勿当 S1 必中）；
       · 值内含 LF(0x0A)  ⇒ 一行被拆成多行（碎片行字段数可能偏少/偏多，**也可能恰好=列数**）。
     ⇒ 一旦命中，快照就是**坏快照**。restore 在「生成 SQL 之前」按**两个**签名检测并拒绝
        （rc≠0，点名表；dry-run 亦拒绝；既不产出坏 SQL、也不静默丢弃或写坏数据）：
          S1 行字段数 ≠ 列数          —— 值内**含 TAB 且不含 LF** ⇒ 必命中（该行 TAB 数 = 列数-1+k > 列数-1）；
                                         TAB+LF 组合值 ⇒ 碎片行字段数**可能恰=列数** ⇒ S1 不命中，由 S2 抓；
                                         纯 LF 值造成的碎片行常也命中（碎片字段数≠列数），但不保证。
          S2 数据行数 ≠ manifest.rows  —— 值内 LF 必命中（一行被拆成 1+#LF 行 ⇒ 行数 > rows）。
        manifest.rows = 快照时刻 `SELECT COUNT(*)` 的**真实逻辑行数**（不是换行字节数）。
        覆盖度（按签名分工，勿简化为「TAB 必被 S1 抓」）：
          · 值内含 LF（含 TAB+LF 组合）⇒ S2 必命中（行数 > rows）；
          · 值内含 TAB 且不含 LF     ⇒ S1 必命中（行字段数 > 列数）；
          ⇒ 两者合起来对「值内 TAB/LF」**完备**：无同源漏检（S1 单独会被「碎片字段数恰=列数」的
            组合绕过 —— 该组合必含 LF，必落在 S2 网内）。
          自检背书：S1 由 PASS 4/11（TAB 无 LF）背书；S2 由 PASS 6/11（TAB+LF 组合值 a
b	c）
            与 PASS 7/11（1 列表纯 LF 值）背书。
        口径：manifest 表项**缺 `rows`**（旧快照）按 fail-closed 处理 —— 拒绝（rc≠0）并说明，不静默放行；
              旧快照的 `rows` 若为旧「换行字节数」口径，则对含 LF 的坏快照可能漏检，应重新 snap。
        仍不在网内（如实登记，勿当已覆盖）：
          (a) 只认 TAB/LF；值内 CR(0x0D) 等其他控制字节不在签名内（CR 可被 sql_escape 转义、可无损往返）。
          (b) L2 的「字面 NULL ↔ SQL NULL」碰撞是独立限制，不属本检测。
          (c) S2 的 rows 由**另一次** `SELECT COUNT(*)` 取值，与 `SELECT *` 之间存在并发写窗口 ⇒
              快照应针对**静默**库；本检测对该窗口取保守（可能 rc≠0 误拒，绝不误放）。
          (d) restore **不**核 tsv 与 manifest 的 sha256（内容篡改检测走 diff 路径）——S1/S2 是**结构**签名。

(L2) 字面字符串 "NULL" 与 SQL NULL **碰撞**。
     `--raw` 下 SQL NULL 落成字面 4 字节 `NULL`，与**字符串值** "NULL" 逐字节相同
     ⇒ 二者在快照里不可区分。restore 把字段 `NULL` 一律当 SQL NULL 灌回，
     故字符串 "NULL" 往返后会**丢失（变 SQL NULL）**。该限制在 restore-selftest
     有构造证明（构造列值恰为字符串 "NULL" ⇒ 该限制如实可见）。

(L3) restore 写库为**单事务**（全表一套）。
     `SET autocommit=0; …(所有表 DELETE/INSERT)…; COMMIT;`
     任一步失败（mysql 非零退出，客户端默认遇错即停）⇒ 未到 COMMIT，会话结束即**整体回滚**，
     目标表**不留部分/截断数据**；失败路径另发一条 `ROLLBACK;` 兜底并 rc≠0 退出。
     仅对 InnoDB 生效（本项目 3 库的表全为 InnoDB）；MyISAM 无事务，不在保证范围内。

用法：
  python dbsnap.py snap --out DIR [--table mir2_db.characters ...] [--mysql PATH] [--host ..] [--user ..] [--password ..]
  python dbsnap.py diff A_DIR B_DIR [--max-diffs 10]
  python dbsnap.py restore --dir DIR [--yes-apply] [--allow-db DB]   # 从快照字段级灌回（默认 dry-run；白名单库才可写）
  python dbsnap.py selftest            # 改坏必红自检（快照→篡改→diff 必须红）
  python dbsnap.py restore-selftest    # restore 自检（临时库往返；三库只读）

默认表集：mir2_account.account / account_protection + mir2_db.characters* 全族
（对应设计文档 §5.1 的角色数据边界）。
"""
import argparse
import contextlib
import hashlib
import io
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

# restore 目标白名单：只允许把快照写回这三个库；其他库名一律拒绝（rc≠0）。
# 自检临时库经显式 --allow-db 放行（仅用于 selftest/受控场景，不进默认路径）。
ALLOWED_DBS = frozenset({"mir2_db", "mir2_account", "mir2_data"})
NUMERIC_TYPES = frozenset({"tinyint", "smallint", "mediumint", "int", "integer", "bigint",
                           "decimal", "numeric", "float", "double", "bit", "year"})


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


def table_columns(args, db: str, table: str):
    # 返回 [{name,type}, ...]，序 = information_schema ORDINAL_POSITION（= SELECT * 的列序）。
    out = run_mysql(args, "SELECT COLUMN_NAME, DATA_TYPE FROM information_schema.COLUMNS "
                          f"WHERE TABLE_SCHEMA='{db}' AND TABLE_NAME='{table}' "
                          "ORDER BY ORDINAL_POSITION")
    cols = []
    for line in out.decode("utf-8", "replace").splitlines():
        if not line.strip():
            continue
        name, _, dtype = line.partition("\t")
        cols.append({"name": name, "type": dtype})
    return cols


def run_mysql_stdin(args, sql_bytes: bytes, timeout=600):
    # 走 stdin 喂 SQL：绕开 Windows 命令行长度上限（大表 INSERT 会超 -e）。
    # **不**在此处 sys.exit：restore 的单事务需要自己看 rc 再决定回滚，故只返回 CompletedProcess。
    cmd = [args.mysql, "-h", args.host, "-P", str(args.port),
           "-u", args.user, f"--password={args.password}", "--batch"]
    return subprocess.run(cmd, input=sql_bytes, capture_output=True, timeout=timeout)


def _run_rollback(args):
    # (L3) 兜底 ROLLBACK：主会话一旦出错，mysql 客户端默认遇错即停并关闭连接，
    # autocommit=0 下未 COMMIT 的事务随连接结束**已整体回滚**；此处再显式发一条
    # 仅作审计可见的兜底（新会话的空事务），失败也不改变结论。
    try:
        run_mysql_stdin(args, b"ROLLBACK;\n", timeout=60)
    except Exception as e:  # noqa: BLE001 — 兜底失败不影响「主事务已回滚」这一结论
        print(f"WARN: 兜底 ROLLBACK 未能执行: {e}", file=sys.stderr)


def detect_delimiter_corruption(fq: str, lines, ncols: int, expected_rows=None):
    # (L1) 检测 --raw 快照里「值内 TAB/LF」造成的坏快照。两个签名，任一命中即拒绝：
    #   S1 行字段数 ≠ 列数：值内含 TAB ⇒ 该行多切出列；值内含 LF ⇒ 碎片行字段数偏少/偏多。
    #   S2 数据行数 ≠ manifest.rows：值内含 LF ⇒ 一行被拆成 1+#LF 行。S1 会被「碎片字段数
    #      恰=列数」的组合绕过（复验者用例），S2 用真实逻辑行数对账补齐。
    #      expected_rows=None 表示调用方未提供，不启用 S2（仅历史/内部路径）。
    # 命中即返回点名错误串（表 + 行号 / 两行数值），否则 None。只读，不写库。
    for i, line in enumerate(lines):
        n = len(line.split(b"\t"))
        if n != ncols:
            return (f"{fq} 第 {i + 1} 行字段数={n} ≠ 列数={ncols}："
                    f"疑似值内含 TAB(0x09)/LF(0x0A)（--raw 口径下不可表示，见工具头 L1-S1）")
    if expected_rows is not None and len(lines) != expected_rows:
        return (f"{fq} 数据行数={len(lines)} ≠ manifest.rows={expected_rows}："
                f"疑似值内含 LF(0x0A)（--raw 口径下一行被拆成多行，不可表示，见工具头 L1-S2）")
    return None


def sql_escape(s: str) -> str:
    return (s.replace("\\", "\\\\").replace("'", "\\'")
             .replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t")
             .replace("\x00", "\\0"))


def sql_literal(field: str, col_type: str) -> str:
    # snap 用 --raw：NULL 落成字面 'NULL'，空串是空串（见文件头 snap 口径）。据此区分。
    if field == "NULL":
        return "NULL"
    if col_type.lower().split("(")[0].strip() in NUMERIC_TYPES:
        return field if field != "" else "NULL"
    return "'" + sql_escape(field) + "'"


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
        # rows = 真实逻辑行数（SELECT COUNT(*)），**不是** data.count(b"\n"):
        # 后者在「值内含 LF」时把 1 行数成多行，会令 restore 的行数对账（L1-S2）失效。
        rows = int(run_mysql(args, f"SELECT COUNT(*) FROM `{db}`.`{table}`")
                   .decode("utf-8", "replace").strip().splitlines()[0])
        digest = sha256_file(out_path)
        manifest["tables"].append({"table": fq, "rows": rows, "sha256": digest,
                                   "file": os.path.basename(out_path),
                                   "columns": table_columns(args, db, table)})
        print(f"SNAP {fq}: rows={rows} sha256={digest[:16]}…")
    with open(os.path.join(args.out, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
    print(f"SNAP_OK {len(manifest['tables'])} 表 -> {args.out}")


def load_manifest(d: str):
    path = os.path.join(d, "manifest.json")
    if not os.path.exists(path):
        print(f"RED: 缺 manifest.json: {path}", file=sys.stderr)
        sys.exit(2)
    with open(path, "r", encoding="utf-8") as f:
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


def cmd_restore(args):
    """从快照目录（manifest.json + tsv）把字段级内容灌回表。

    默认 dry-run：只打印摘要（表/行数/目标库），不写库；显式 --yes-apply 才执行。
    写语义 = 先 DELETE 再 INSERT（替换），**单事务**（见工具头 L3）；**只**对白名单库执行，
    其他库名一律 rc≠0 拒绝。生成 SQL 前先按工具头 L1 拒绝含 TAB/LF 的坏快照（dry-run 亦拒绝）。
    """
    manifest = load_manifest(args.dir)
    allowed = set(ALLOWED_DBS) | set(getattr(args, "allow_db", None) or [])
    plans = []
    for t in manifest.get("tables", []):
        fq = t["table"]
        db, _, table = fq.partition(".")
        if not db or not table:
            print(f"RED: 非法表名（须 db.table）: {fq}", file=sys.stderr)
            sys.exit(2)
        if db not in allowed:
            print(f"RED: 目标库 '{db}' 不在白名单，拒绝写入（允许: {sorted(allowed)}）；"
                  f"如确需（仅自检临时库）显式 --allow-db {db}", file=sys.stderr)
            sys.exit(2)
        tsv = os.path.join(args.dir, t.get("file") or f"{db}.{table}.tsv")
        if not os.path.exists(tsv):
            print(f"RED: 缺 tsv 文件: {tsv}", file=sys.stderr)
            sys.exit(2)
        cols = t.get("columns")
        if not cols:
            cols = table_columns(args, db, table)  # 回退：按目标表现况列序（旧快照无 columns）
        if not cols:
            print(f"RED: 无法确定 {fq} 的列（manifest 无 columns 且目标表不存在）", file=sys.stderr)
            sys.exit(2)
        # (L1 fail-closed) 旧快照缺 rows ⇒ 无法做 S2 行数对账，拒绝而非静默放行。
        if "rows" not in t:
            print(f"RED: 快照表项缺 rows 字段（旧快照），按 fail-closed 拒绝: {fq}"
                  f"（见工具头 L1）", file=sys.stderr)
            sys.exit(2)
        with open(tsv, "rb") as f:
            raw = f.read()
        lines = raw.split(b"\n")
        if lines and lines[-1] == b"":
            lines.pop()  # 末尾换行不算一行
        # (L1) 生成 SQL 前检测坏快照：S1 字段数≠列数 + S2 行数≠manifest.rows。命中即 rc≠0 拒绝。
        bad = detect_delimiter_corruption(fq, lines, len(cols), expected_rows=t["rows"])
        if bad:
            print(f"RED: 坏快照，拒绝生成 SQL（值内含 TAB/LF 不可表示，工具头 L1）: {bad}",
                  file=sys.stderr)
            sys.exit(2)
        plans.append({"db": db, "table": table, "fq": fq, "cols": cols,
                      "lines": lines, "rows": len(lines)})
    total = sum(p["rows"] for p in plans)
    print(f"RESTORE PLAN: {len(plans)} 表 / {total} 行 / 目标库 "
          f"{sorted({p['db'] for p in plans})} <- {args.dir}")
    for p in plans:
        print(f"  {p['fq']}: {p['rows']} 行, 列={[c['name'] for c in p['cols']]}")
    if not getattr(args, "yes_apply", False):
        print("DRY-RUN: 未执行任何写库（加 --yes-apply 才真正灌库）")
        return
    # (L3) 单事务：**所有**表的 DELETE/INSERT 打进同一事务；任一步失败即整体回滚，
    # 目标表不留部分/截断数据（对比旧实现：逐表独立提交，中途失败会留下半成品）。
    stmts = ["SET autocommit=0;"]
    for p in plans:
        collist = ",".join("`" + c["name"] + "`" for c in p["cols"])
        stmts.append(f"DELETE FROM `{p['db']}`.`{p['table']}`;")
        for raw_line in p["lines"]:
            fields = raw_line.decode("utf-8", "surrogateescape").split("\t")
            vals = []
            for i, c in enumerate(p["cols"]):
                field = fields[i] if i < len(fields) else ""
                vals.append(sql_literal(field, c["type"]))
            stmts.append(f"INSERT INTO `{p['db']}`.`{p['table']}` ({collist}) "
                         f"VALUES ({','.join(vals)});")
    stmts.append("COMMIT;")
    r = run_mysql_stdin(args, ("\n".join(stmts) + "\n").encode("utf-8", "surrogateescape"))
    if r.returncode != 0:
        _run_rollback(args)
        print(f"RED: 灌库失败——单事务未提交、已整体回滚，目标表保持原样: "
              f"{r.stderr.decode('utf-8', 'replace')[:500]}", file=sys.stderr)
        sys.exit(2)
    for p in plans:
        print(f"RESTORE_APPLIED {p['fq']}: {p['rows']} 行 -> `{p['db']}`")
    print(f"RESTORE_OK {len(plans)} 表 / {total} 行（单事务已 COMMIT）")


TMP_DB = "dbsnap_selftest_tmp"
GUARDED_DBS = ["mir2_db", "mir2_account", "mir2_data"]


def _show_databases(args):
    out = run_mysql(args, "SHOW DATABASES")
    return set(x for x in out.decode("utf-8", "replace").splitlines() if x.strip())


def _table_status_count(args, db):
    out = run_mysql(args, f"SHOW TABLE STATUS FROM `{db}`")
    return len([x for x in out.decode("utf-8", "replace").splitlines() if x.strip()])


def _write_synth_snapshot(d, tables):
    """写一份合成快照（manifest.json + 每表 tsv），供坏快照/单事务/NULL 牙构造用。

    tables: [{db, table, cols:[{name,type}], tsv: bytes}] —— tsv **原样**写入，
    便于把 TAB/LF 等不可表示字节注入 tsv 以构造坏快照。只读工具，不碰真库。
    """
    os.makedirs(d, exist_ok=True)
    man = {"tool": "dbsnap", "tables": []}
    for t in tables:
        fname = f"{t['db']}.{t['table']}.tsv"
        fpath = os.path.join(d, fname)
        with open(fpath, "wb") as f:
            f.write(t["tsv"])
        man["tables"].append({"table": f"{t['db']}.{t['table']}",
                              # rows 可显式给（=真实逻辑行数，须与 cmd_snap 的 SELECT COUNT(*) 口径一致）；
                              # 不给则回退到换行字节数（仅对「值内无 LF」的合成 tsv 有效）。
                              "rows": t.get("rows", t["tsv"].count(b"\n")),
                              "sha256": sha256_file(fpath),
                              "file": fname, "columns": t["cols"]})
    with open(os.path.join(d, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(man, f, ensure_ascii=False, indent=2)


def cmd_restore_selftest(args):
    """restore 自检：只在临时库做完整往返；三库只读（SHOW DATABASES / SHOW TABLE STATUS）。

    硬断言：自检前后 SHOW DATABASES 一致（临时库不残留）+ 三库 SHOW TABLE STATUS 行数一致。
    牙（改坏必红，编号按本文件当前顺序；两个签名各一条）：
      · 去白名单 ⇒ PASS 3 失败（rc≠0）；
      · 去掉/短路 (L1-S1) 字段数检测 ⇒ PASS 4/5 失败（TAB / 碎片行不再被拒）；
      · 去掉/短路 (L1-S2) 行数对账 ⇒ PASS 6/7 失败（复验者绕过用例不再被拒）；
      · 把单事务改回逐表提交 ⇒ PASS 8 失败（中途失败后目标表未回滚）。
    """
    import tempfile
    before_dbs = _show_databases(args)
    before_counts = {db: _table_status_count(args, db) for db in GUARDED_DBS}
    tmp = tempfile.mkdtemp(prefix="dbsnap-restore-selftest-")
    ref = os.path.join(tmp, "ref")
    try:
        run_mysql(args, f"DROP DATABASE IF EXISTS `{TMP_DB}`")
        run_mysql(args, f"CREATE DATABASE `{TMP_DB}`")
        run_mysql(args, f"CREATE TABLE `{TMP_DB}`.`t_roundtrip` "
                        "(id INT NOT NULL PRIMARY KEY, name VARCHAR(32), score INT)")
        run_mysql(args, f"INSERT INTO `{TMP_DB}`.`t_roundtrip` VALUES "
                        "(1,'alice',10),(2,'bob',20),(3,'carol',30)")
        # 参考快照（期望内容）
        ns = argparse.Namespace(**vars(args))
        ns.out = ref
        ns.table = [f"{TMP_DB}.t_roundtrip"]
        cmd_snap(ns)
        # 弄脏临时表
        run_mysql(args, f"DELETE FROM `{TMP_DB}`.`t_roundtrip`")
        # 灌回（--yes-apply + 显式放行临时库）
        cmd_restore(argparse.Namespace(dir=ref, yes_apply=True, allow_db=[TMP_DB],
                                       mysql=args.mysql, host=args.host, port=args.port,
                                       user=args.user, password=args.password))
        # 往返：再拍一次 → 与参考快照 diff 必须绿
        got = os.path.join(tmp, "got")
        ns2 = argparse.Namespace(**vars(args))
        ns2.out = got
        ns2.table = [f"{TMP_DB}.t_roundtrip"]
        cmd_snap(ns2)
        try:
            cmd_diff(argparse.Namespace(a=ref, b=got, max_diffs=5))
            rt = True
        except SystemExit as e:
            rt = (e.code == 0)
        if not rt:
            print("RED: 灌库往返不一致（restore 未能字段级复原）")
            sys.exit(1)
        print("PASS 1/11 建表→灌快照→往返 diff 判绿")
        # 篡改一行 → 必须红
        run_mysql(args, f"UPDATE `{TMP_DB}`.`t_roundtrip` SET score=999 WHERE id=2")
        got2 = os.path.join(tmp, "got2")
        ns3 = argparse.Namespace(**vars(args))
        ns3.out = got2
        ns3.table = [f"{TMP_DB}.t_roundtrip"]
        cmd_snap(ns3)
        try:
            cmd_diff(argparse.Namespace(a=ref, b=got2, max_diffs=5))
            print("RED: 篡改一行后仍判绿——红检失效")
            sys.exit(1)
        except SystemExit as e:
            if e.code != 1:
                print("RED: 篡改后退出码异常:", e.code)
                sys.exit(1)
        print("PASS 2/11 篡改一行后判红（改坏必红成立）")
        # 白名单牙：不带 --allow-db 往临时库灌 → 必须被拒（rc≠0）
        guard_ok = False
        try:
            cmd_restore(argparse.Namespace(dir=ref, yes_apply=False, allow_db=[],
                                           mysql=args.mysql, host=args.host, port=args.port,
                                           user=args.user, password=args.password))
        except SystemExit as e:
            guard_ok = (e.code != 0)
        if not guard_ok:
            print("RED: 白名单未拦住非白名单库——守卫失效")
            sys.exit(1)
        print("PASS 3/11 非白名单库被拒（白名单守卫成立）")

        def _restore_reject(snapdir, apply):
            """跑一次 restore，捕 stderr，返回 (rc, stderr)。用于坏快照/失败路径的牙。"""
            buf = io.StringIO()
            rc = 0
            try:
                with contextlib.redirect_stderr(buf):
                    cmd_restore(argparse.Namespace(dir=snapdir, yes_apply=apply, allow_db=[TMP_DB],
                                                   mysql=args.mysql, host=args.host, port=args.port,
                                                   user=args.user, password=args.password))
            except SystemExit as e:
                rc = e.code or 0
            return rc, buf.getvalue()

        # (L1) 牙：值内含 TAB ⇒ 坏快照，生成 SQL 前即 rc≠0 且点名表
        snap_tab = os.path.join(tmp, "snap_tab")
        _write_synth_snapshot(snap_tab, [{
            "db": TMP_DB, "table": "t_delim_tab",
            "cols": [{"name": "id", "type": "int"}, {"name": "name", "type": "varchar"}],
            "tsv": b"1\talice\n2\tbo\tb\n"}])  # 第 2 行值 "bo<TAB>b" 注入 TAB（多切一列）
        rc, err = _restore_reject(snap_tab, False)
        if rc == 0 or "t_delim_tab" not in err:
            print(f"RED: 含 TAB 的坏快照未被拒（rc={rc}）：{err[:200]}")
            sys.exit(1)
        print("PASS 4/11 含 TAB 的坏快照被拒（rc≠0 且点名 t_delim_tab）")

        # (L1) 牙：值内含 LF ⇒ 坏快照，生成 SQL 前即 rc≠0 且点名表
        snap_lf = os.path.join(tmp, "snap_lf")
        _write_synth_snapshot(snap_lf, [{
            "db": TMP_DB, "table": "t_delim_lf",
            "cols": [{"name": "id", "type": "int"}, {"name": "name", "type": "varchar"}],
            "tsv": b"1\talice\n2\tbo\nb\n"}])  # 值 "bo<LF>b" 注入 LF（一行被拆两行）
        rc, err = _restore_reject(snap_lf, False)
        if rc == 0 or "t_delim_lf" not in err:
            print(f"RED: 含 LF 的坏快照未被拒（rc={rc}）：{err[:200]}")
            sys.exit(1)
        print("PASS 5/11 含 LF 的坏快照被拒（rc≠0 且点名 t_delim_lf）")

        # (L1-S2) 牙：复验者绕过用例 —— 2 列表，值行 = 1<TAB>(a<LF>b<TAB>c)（值同时含 LF 与 TAB）。
        # --raw 落盘被拆成 "1<TAB>a" <LF> "b<TAB>c"：两片字段数**恰=列数** ⇒ S1 漏检；
        # 由 S2 行数对账抓（该表真实 1 行，manifest.rows=1）。
        snap_byp2 = os.path.join(tmp, "snap_byp2")
        _write_synth_snapshot(snap_byp2, [{
            "db": TMP_DB, "table": "t_byp_2col",
            "cols": [{"name": "id", "type": "int"}, {"name": "name", "type": "varchar"}],
            "tsv": b"1\ta\nb\tc\n", "rows": 1}])  # 1 行，值含 LF+TAB
        rc, err = _restore_reject(snap_byp2, False)
        if rc == 0 or "t_byp_2col" not in err:
            print(f"RED: S1 绕过用例(2列,值 a\nb\tc)未被 S2 拦住（rc={rc}）：{err[:200]}")
            sys.exit(1)
        print("PASS 6/11 复验者绕过用例(2列表+值 a\nb\tc)被 S2 行数对账拦住")

        # (L1-S2) 牙：1 列表 + 纯 LF 值 ⇒ 拆行后每片字段数=1=列数 ⇒ S1 漏检；由 S2 抓。
        snap_byp1 = os.path.join(tmp, "snap_byp1")
        _write_synth_snapshot(snap_byp1, [{
            "db": TMP_DB, "table": "t_byp_1col",
            "cols": [{"name": "v", "type": "varchar"}],
            "tsv": b"a\nb\n", "rows": 1}])  # 1 行，纯 LF 值
        rc, err = _restore_reject(snap_byp1, False)
        if rc == 0 or "t_byp_1col" not in err:
            print(f"RED: S1 绕过用例(1列,纯 LF 值)未被 S2 拦住（rc={rc}）：{err[:200]}")
            sys.exit(1)
        print("PASS 7/11 复验者绕过用例(1列表+纯 LF 值)被 S2 行数对账拦住")

        # (L3) 牙：中途失败（第二张表 SQL 故意错）⇒ 单事务整体回滚，目标表回原样
        run_mysql(args, f"CREATE TABLE `{TMP_DB}`.`t_first` "
                        "(id INT NOT NULL PRIMARY KEY, name VARCHAR(16)) ENGINE=InnoDB")
        run_mysql(args, f"CREATE TABLE `{TMP_DB}`.`t_bad` "
                        "(id INT NOT NULL PRIMARY KEY) ENGINE=InnoDB")
        run_mysql(args, f"INSERT INTO `{TMP_DB}`.`t_first` VALUES (1,'orig1'),(2,'orig2')")
        txn_orig = os.path.join(tmp, "txn_orig")
        nso = argparse.Namespace(**vars(args))
        nso.out = txn_orig
        nso.table = [f"{TMP_DB}.t_first"]
        cmd_snap(nso)
        snap_txn = os.path.join(tmp, "snap_txn")
        _write_synth_snapshot(snap_txn, [
            {"db": TMP_DB, "table": "t_first",  # 与目标表内容**不同**，故「未回滚」必被 diff 抓到
             "cols": [{"name": "id", "type": "int"}, {"name": "name", "type": "varchar"}],
             "tsv": b"1\tnew1\n2\tnew2\n3\tnew3\n"},
            {"db": TMP_DB, "table": "t_bad",    # 列 ghost 目标表没有 ⇒ INSERT 必失败
             "cols": [{"name": "id", "type": "int"}, {"name": "ghost", "type": "varchar"}],
             "tsv": b"1\tx\n"}])
        rc, err = _restore_reject(snap_txn, True)
        if rc == 0:
            print("RED: 中途失败的灌库竟 rc=0（应 rc≠0）")
            sys.exit(1)
        txn_after = os.path.join(tmp, "txn_after")
        nsa = argparse.Namespace(**vars(args))
        nsa.out = txn_after
        nsa.table = [f"{TMP_DB}.t_first"]
        cmd_snap(nsa)
        try:
            cmd_diff(argparse.Namespace(a=txn_orig, b=txn_after, max_diffs=5))
            rolled = True
        except SystemExit as e:
            rolled = (e.code == 0)
        if not rolled:
            print("RED: 中途失败后 t_first 未回滚到原样——单事务失效（目标表被留成半成品）")
            sys.exit(1)
        print("PASS 8/11 中途失败后目标表回滚到原样（单事务成立）")

        # (L2) 登记限制可见：字面字符串 "NULL" 与 SQL NULL 碰撞（往返丢失）
        run_mysql(args, f"CREATE TABLE `{TMP_DB}`.`t_null` "
                        "(id INT NOT NULL PRIMARY KEY, name VARCHAR(16)) ENGINE=InnoDB")
        run_mysql(args, f"INSERT INTO `{TMP_DB}`.`t_null` VALUES (1,'NULL'),(2,NULL)")
        b0 = int(run_mysql(args, f"SELECT COUNT(*) FROM `{TMP_DB}`.`t_null` "
                                 "WHERE id=1 AND name IS NULL").decode().strip())
        null_ref = os.path.join(tmp, "null_ref")
        nsr = argparse.Namespace(**vars(args))
        nsr.out = null_ref
        nsr.table = [f"{TMP_DB}.t_null"]
        cmd_snap(nsr)
        run_mysql(args, f"DELETE FROM `{TMP_DB}`.`t_null`")
        cmd_restore(argparse.Namespace(dir=null_ref, yes_apply=True, allow_db=[TMP_DB],
                                       mysql=args.mysql, host=args.host, port=args.port,
                                       user=args.user, password=args.password))
        a1 = int(run_mysql(args, f"SELECT COUNT(*) FROM `{TMP_DB}`.`t_null` "
                                 "WHERE id=1 AND name IS NULL").decode().strip())
        if not (b0 == 0 and a1 == 1):
            print(f"RED: NULL 碰撞未按登记口径出现（before={b0} after={a1}）"
                  "——若已换「不碰撞表示」请同步更新本构造")
            sys.exit(1)
        print('PASS 9/11 字面字符串 "NULL" 往返丢失变 SQL NULL（L2 限制如实可见）')
    finally:
        run_mysql(args, f"DROP DATABASE IF EXISTS `{TMP_DB}`")
    after_dbs = _show_databases(args)
    after_counts = {db: _table_status_count(args, db) for db in GUARDED_DBS}
    if after_dbs != before_dbs:
        print(f"RED: SHOW DATABASES 前后不一致\n  before={sorted(before_dbs)}\n  after ={sorted(after_dbs)}")
        sys.exit(1)
    print(f"PASS 10/11 SHOW DATABASES 前后一致（{len(after_dbs)} 库，临时库无残留）")
    if after_counts != before_counts:
        print(f"RED: 三库 SHOW TABLE STATUS 行数前后不一致\n  before={before_counts}\n  after ={after_counts}")
        sys.exit(1)
    print(f"PASS 11/11 三库 SHOW TABLE STATUS 行数前后一致 {after_counts}")
    print("GREEN: dbsnap restore-selftest 11/11")
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
    r = sub.add_parser("restore")
    r.add_argument("--dir", required=True, help="快照目录（含 manifest.json + tsv）")
    r.add_argument("--yes-apply", action="store_true", help="真正写库（默认 dry-run，不写）")
    r.add_argument("--allow-db", action="append", default=[],
                   help="扩展白名单库（仅自检/受控场景；默认只允许 mir2_db/mir2_account/mir2_data）")
    sub.add_parser("selftest")
    sub.add_parser("restore-selftest")
    args = ap.parse_args()
    if not os.path.exists(args.mysql):
        print(f"RED: 找不到 mysql 客户端: {args.mysql}", file=sys.stderr)
        sys.exit(2)
    if getattr(args, "cmd") in ("snap", "selftest"):
        if not getattr(args, "table", None):
            args.table = DEFAULT_TABLES
    {"snap": cmd_snap, "diff": cmd_diff, "selftest": cmd_selftest,
     "restore": cmd_restore, "restore-selftest": cmd_restore_selftest}[args.cmd](args)


if __name__ == "__main__":
    main()
