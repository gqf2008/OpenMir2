#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""newchr_delchr_selchr_probe.py — 建角/删角/选角「库后态」采集与离线比对骨架。

定位
====
LB-10（审计 `audit-dbsrv-persistence-20261020.md`）要求 `CM_NEWCHR` / `CM_DELCHR` /
`CM_SELCHR` 的**库后态**有跨实现证据：`characters` 行、`characters_indexes.SelectID` /
`IsDeleted`。本脚本是这条证据的**驱动骨架 + 离线断言器**：

  plan     打印规范步骤序列（含 1000ms 限频延迟）与期望后态，不连库
  run      按序列调用**外部驱动命令**，前后各拍一次 dbsnap 快照（本脚本不起服务端/客户端）
  assert   只读快照 + run.json，机判 S1 后态（离线，不连库）
  compare  两份 run 目录的 after 快照交给 dbsnap_pair.py 做逐字段 diff（离线）
  selftest 全离线自检：合成 run 目录 + 逐条变异必红

安全边界（硬性）
================
- **本脚本自身不写数据库**；`run` 只调用调用方给的 `--driver-cmd`，并调用 dbsnap 做
  **只读**快照。`snap` / `run` 的库名必须匹配 `mir2_test_*`（`test` 是 `mir2_test_db`
  的别名），否则拒绝；显式 `--allow-db <库名>` 才放行非测试库（会在 stderr 醒目告警）。
- `assert` / `compare` / `plan` / `selftest` 完全离线，不连 MySQL。
- 本车道（lb10tool）不执行 `run`；真机驱动属 LB-10-E1 的 R 窗口（见 design 文档 §6.5）。

规范序列（oracle 依据）
=======================
`src/DBSrv/Services/Impl/UserService.cs`：
- `CM_QUERYCHR` 200ms 限频；`CM_NEWCHR` / `CM_DELCHR` 1000ms 限频；`CM_SELCHR` 需
  `boChrQueryed=true`（同 socket 先 QUERYCHR 成功）。
- `CM_NEWCHR` 成功：`characters_indexes` INSERT（`SelectID=0`、`IsDeleted=0`）+ 事务建
  `characters`/`characters_ablity`/`characters_status`/`characters_item`/
  `characters_bagitem`/`characters_storageitem`。
- `CM_DELCHR` 成功（等级 < DeleteMinLevel，默认 30）：**只**把 `characters_indexes.IsDeleted`
  置 1，`characters` 行**仍在**（软删除只在索引表）。
- `CM_SELCHR` 成功：选中行 `SelectID=1`，同账号其他行 `SelectID=0`，再回 `SM_STARTPLAY`。

用法（离线）
============
  python newchr_delchr_selchr_probe.py plan --account acct1 --name Hero
  python newchr_delchr_selchr_probe.py assert --run-dir /e/tmp/run-1
  python newchr_delchr_selchr_probe.py compare --a /e/tmp/run-oracle --b /e/tmp/run-rust \\
      --summary-out /e/tmp/cmp.json --jsonl /e/tmp/cmp.jsonl
  python newchr_delchr_selchr_probe.py selftest

用法（真机，R 窗口；本车道不执行）
=================================
  python newchr_delchr_selchr_probe.py run --db mir2_test_db --sequence create \\
      --account acct1 --name Hero --out /e/tmp/run-1 \\
      --driver-cmd "python driver.py --step {step} --account {account} --name {name}"

`--driver-cmd` 每个步骤调用一次；同时注入环境变量 `PROBE_STEP` / `PROBE_ACCOUNT` /
`PROBE_NAME` / `PROBE_DB`，驱动可只用 env 而不解析参数。
"""
import argparse
import importlib.util
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
DEFAULT_DBSNAP = os.path.join(REPO_ROOT, "tools", "dbsnap", "dbsnap.py")
TEST_DB_RE = re.compile(r"^mir2_test_[A-Za-z0-9_]+$")

sys.path.insert(0, HERE)
import dbsnap_pair as dp  # noqa: E402

LB10_TABLES = dp.LB10_TABLES

# 每步：delay_before_ms 是 oracle 限频要求（QUERYCHR 200ms / NEWCHR/DELCHR 1000ms）。
SEQUENCES = {
    "create": [
        {"step": "QUERYCHR_EMPTY", "delay_before_ms": 0,
         "expect": "SM_QUERYCHR 列表 0 个"},
        {"step": "NEWCHR", "delay_before_ms": 1100,
         "expect": "SM_NEWCHR_SUCCESS；indexes(SelectID=0,IsDeleted=0)+建 6 表行"},
        {"step": "QUERYCHR_ONE", "delay_before_ms": 1100,
         "expect": "SM_QUERYCHR 列表 1 个（Hero）"},
        {"step": "SELCHR", "delay_before_ms": 0,
         "expect": "SM_STARTPLAY；indexes.SelectID=1"},
    ],
    "delete": [
        {"step": "QUERYCHR_ONE", "delay_before_ms": 0,
         "expect": "SM_QUERYCHR 列表 1 个（Hero）"},
        {"step": "DELCHR", "delay_before_ms": 1100,
         "expect": "SM_DELCHR_SUCCESS；indexes.IsDeleted=1，characters 行仍在"},
    ],
}

EXPECTED_CHECK_IDS = {
    "create": [
        "steps_all_ok", "characters.row_count", "indexes.row_count",
        "indexes.IsDeleted", "indexes.SelectID", "indexes.others.SelectID",
        "ablity.exists", "status.exists", "item.nonempty", "bagitem.nonempty",
        "storageitem.nonempty",
    ],
    "delete": [
        "steps_all_ok", "indexes.row_count", "indexes.IsDeleted",
        "characters.still_present",
    ],
}


def resolve_db(db):
    return "mir2_test_db" if db == "test" else db


def validate_db_name(db, allow_dbs=()):
    """返回 None=放行，否则返回拒绝原因。只允许 mir2_test_* 或显式 --allow-db。"""
    if not db:
        return "库名为空"
    if db in set(allow_dbs or ()):
        return None
    if TEST_DB_RE.match(db):
        return None
    return ("拒绝库名 %r：只允许 mir2_test_*（或用 --allow-db %s 显式放行非测试库）"
            % (db, db))


def build_steps(sequence):
    return [dict(s, index=i + 1) for i, s in enumerate(SEQUENCES[sequence])]


def expected_after(sequence, name):
    if sequence == "create":
        return {
            "characters": {"where": {"ChrName": name}, "count": 1},
            "characters_indexes": {"where": {"ChrName": name}, "count": 1,
                                   "IsDeleted": "0", "SelectID_after_selchr": "1",
                                   "SelectID_before_selchr": "0"},
            "characters_ablity": {"exists_for_player": True},
            "characters_status": {"exists_for_player": True},
            "characters_item": {"nonempty_for_player": True},
            "characters_bagitem": {"nonempty_for_player": True},
            "characters_storageitem": {"nonempty_for_player": True},
            "not_created_by_newchr": ["characters_bonusability", "characters_magic",
                                      "characters_item_attr"],
        }
    return {
        "characters_indexes": {"where": {"ChrName": name}, "count": 1,
                               "IsDeleted": "1"},
        "characters": {"where": {"ChrName": name}, "count": 1,
                       "note": "软删除只在索引表；characters 行必须仍在"},
    }


def cmd_plan(args):
    db = resolve_db(args.db)
    plan = {
        "tool": "newchr_delchr_selchr_probe",
        "mode": "plan",
        "db": db,
        "account": args.account,
        "chr_name": args.name,
        "sequences": {seq: build_steps(seq) for seq in SEQUENCES},
        "expected_after": {seq: expected_after(seq, args.name) for seq in SEQUENCES},
        "notes": [
            "CM_QUERYCHR 200ms 限频；CM_NEWCHR/CM_DELCHR 1000ms 限频；",
            "CM_SELCHR 需同 socket 先 QUERYCHR 成功（boChrQueryed=true）；",
            "DELCHR 要求角色等级 < DeleteMinLevel（默认 30）；",
            "驱动只由调用方提供；本脚本不启服务端/客户端、不写库。",
        ],
    }
    text = json.dumps(plan, ensure_ascii=False, indent=2)
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            f.write(text + "\n")
        print("PLAN -> %s" % args.out)
    else:
        print(text)
    return 0


def retarget_tables(tables, db):
    """把登记表名的库前缀改成本次 --db，防止 --db 是测试克隆却读到 mir2_db 生产表。"""
    out = []
    for fq in tables:
        _, _, table = fq.partition(".")
        if not table:
            raise ValueError("表名须为 db.table: %r" % fq)
        out.append("%s.%s" % (db, table))
    return out


def _dbsnap_cmd(args, db, out_dir, tables):
    cmd = [sys.executable, args.dbsnap,
           "--mysql", args.mysql, "--host", args.host, "--port", str(args.port),
           "--user", args.user, "--password", args.password,
           "snap", "--out", out_dir, "--allow-db", db]
    for t in retarget_tables(tables, db):
        cmd += ["--table", t]
    return cmd


def take_snapshot(args, db, out_dir, tables):
    os.makedirs(os.path.dirname(out_dir) or ".", exist_ok=True)
    cmd = _dbsnap_cmd(args, db, out_dir, tables)
    r = subprocess.run(cmd, capture_output=True, text=True)
    return {"cmd": cmd, "rc": r.returncode,
            "stdout": (r.stdout or "")[-2000:], "stderr": (r.stderr or "")[-2000:]}


def cmd_snap(args):
    db = resolve_db(args.db)
    err = validate_db_name(db, args.allow_db)
    if err:
        print("RED: %s" % err, file=sys.stderr)
        return 2
    tables = [t.strip() for t in args.tables.split(",") if t.strip()]
    res = take_snapshot(args, db, args.out, tables)
    print(json.dumps(res, ensure_ascii=False, indent=2))
    return 0 if res["rc"] == 0 else res["rc"]


def _driver_argv(template, step, account, name, db):
    fmt = {"step": step, "account": account, "name": name, "db": db}
    parts = shlex.split(template, posix=True)
    return [p.format(**fmt) for p in parts]


def cmd_run(args):
    db = resolve_db(args.db)
    err = validate_db_name(db, args.allow_db)
    if err:
        print("RED: %s" % err, file=sys.stderr)
        return 2
    if not args.driver_cmd:
        print("RED: run 需要 --driver-cmd（外部驱动；本脚本不起服务端/客户端）", file=sys.stderr)
        return 2
    os.makedirs(args.out, exist_ok=True)
    before_dir = os.path.join(args.out, "before")
    after_dir = os.path.join(args.out, "after")
    tables = [t.strip() for t in args.tables.split(",") if t.strip()]
    run = {
        "tool": "newchr_delchr_selchr_probe", "mode": "run",
        "db": db, "account": args.account, "chr_name": args.name,
        "sequence": args.sequence, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                                                time.gmtime()),
        "steps": [], "before_snapshot": "before", "after_snapshot": "after",
    }
    run["before"] = take_snapshot(args, db, before_dir, tables)
    aborted = False
    for st in build_steps(args.sequence):
        if aborted:
            run["steps"].append({**st, "rc": None, "skipped": True})
            continue
        if st["delay_before_ms"]:
            time.sleep(st["delay_before_ms"] / 1000.0)
        env = dict(os.environ, PROBE_STEP=st["step"], PROBE_ACCOUNT=args.account,
                   PROBE_NAME=args.name, PROBE_DB=db)
        body = ""
        try:
            argv = _driver_argv(args.driver_cmd, st["step"], args.account, args.name, db)
            r = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=args.timeout)
            rc = r.returncode
            body = (r.stdout or "") + (r.stderr or "")
        except subprocess.TimeoutExpired:
            rc = 124
            body = "TIMEOUT after %ss" % args.timeout
        except OSError as e:
            rc = 127
            body = "spawn failed: %s" % e
        run["steps"].append({**st, "rc": rc, "output_tail": body[-2000:]})
        if rc != 0 and args.stop_on_fail:
            aborted = True
    run["after"] = take_snapshot(args, db, after_dir, tables)
    run["finished_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    with open(os.path.join(args.out, "run.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(run, f, ensure_ascii=False, indent=2)
    print("RUN -> %s（steps=%d, aborted=%s）" % (args.out, len(run["steps"]), aborted))
    return 0


class AssertError(Exception):
    pass


def load_rows(snapdir, table):
    """返回 (columns, [dict...])；缺表返回 (None, None)。"""
    man = dp.manifest_tables(snapdir)
    if table not in man:
        return None, None
    entry = man[table]
    rows, errs = dp.read_rows(entry, entry["columns"])
    if errs:
        raise AssertError("快照表 %s 结构错误: %s" % (table, errs[:1]))
    return entry["columns"], [dict(zip(entry["columns"], r)) for r in rows]


def _find_rows(rows, where):
    return [r for r in rows if all(r.get(k) == v for k, v in where.items())]


def check_run(run_dir):
    """离线机判。返回 (rc, checks, run)。checks=[{id, ok, detail}]。"""
    run_path = os.path.join(run_dir, "run.json")
    if not os.path.exists(run_path):
        raise AssertError("缺 run.json: %s" % run_path)
    with open(run_path, encoding="utf-8") as f:
        run = json.load(f)
    snap = os.path.join(run_dir, run.get("after_snapshot", "after"))
    if not os.path.isdir(snap):
        raise AssertError("缺 after 快照目录: %s" % snap)
    seq = run.get("sequence")
    name = run.get("chr_name")
    account = run.get("account")
    checks = []

    def add(cid, ok, detail=""):
        checks.append({"id": cid, "ok": bool(ok), "detail": detail})

    steps = run.get("steps", [])
    add("steps_all_ok", all(s.get("rc") == 0 for s in steps),
        "rc=%s" % [s.get("rc") for s in steps])
    selchr_ok = any(s.get("step") == "SELCHR" and s.get("rc") == 0 for s in steps)

    if seq == "create":
        cols, chars = load_rows(snap, "mir2_db.characters")
        if chars is None:
            add("characters.row_count", False, "快照缺 characters 表")
            return 1, checks, run
        mine = _find_rows(chars, {"ChrName": name})
        add("characters.row_count", len(mine) == 1, "count=%d" % len(mine))
        player_id = mine[0]["Id"] if mine else None

        _, idx = load_rows(snap, "mir2_db.characters_indexes")
        if idx is None:
            add("indexes.row_count", False, "快照缺 characters_indexes 表")
            return 1, checks, run
        idx_mine = _find_rows(idx, {"ChrName": name})
        add("indexes.row_count", len(idx_mine) == 1, "count=%d" % len(idx_mine))
        if idx_mine:
            add("indexes.IsDeleted", idx_mine[0].get("IsDeleted") == "0",
                "IsDeleted=%r（期望 0）" % idx_mine[0].get("IsDeleted"))
            want = "1" if selchr_ok else "0"
            add("indexes.SelectID", idx_mine[0].get("SelectID") == want,
                "SelectID=%r（期望 %s，selchr_ok=%s）" %
                (idx_mine[0].get("SelectID"), want, selchr_ok))
        others = [r for r in idx
                  if r.get("Account") == account and r.get("ChrName") != name]
        add("indexes.others.SelectID",
            all(r.get("SelectID") == "0" for r in others),
            "others=%d" % len(others))

        for tid, table in (("ablity", "mir2_db.characters_ablity"),
                           ("status", "mir2_db.characters_status")):
            _, rows = load_rows(snap, table)
            ok = rows is not None and player_id is not None and any(
                r.get("PlayerId") == player_id for r in rows)
            add("%s.exists" % tid, ok, "player_id=%r rows=%s" %
                (player_id, None if rows is None else len(rows)))
        for tid, table in (("item", "mir2_db.characters_item"),
                           ("bagitem", "mir2_db.characters_bagitem"),
                           ("storageitem", "mir2_db.characters_storageitem")):
            _, rows = load_rows(snap, table)
            ok = rows is not None and player_id is not None and any(
                r.get("PlayerId") == player_id for r in rows)
            add("%s.nonempty" % tid, ok, "player_id=%r rows=%s" %
                (player_id, None if rows is None else len(rows)))

    elif seq == "delete":
        _, idx = load_rows(snap, "mir2_db.characters_indexes")
        if idx is None:
            add("indexes.row_count", False, "快照缺 characters_indexes 表")
            return 1, checks, run
        idx_mine = _find_rows(idx, {"ChrName": name})
        add("indexes.row_count", len(idx_mine) == 1, "count=%d" % len(idx_mine))
        add("indexes.IsDeleted",
            bool(idx_mine) and idx_mine[0].get("IsDeleted") == "1",
            "IsDeleted=%r（期望 1）" % (idx_mine[0].get("IsDeleted") if idx_mine else None))
        _, chars = load_rows(snap, "mir2_db.characters")
        n = 0 if chars is None else len(_find_rows(chars, {"ChrName": name}))
        add("characters.still_present", n == 1,
            "count=%d（软删除只写索引表，characters 行必须仍在）" % n)
    else:
        raise AssertError("未知 sequence: %r" % seq)

    rc = 0 if all(c["ok"] for c in checks) else 1
    return rc, checks, run


def cmd_assert(args):
    try:
        rc, checks, run = check_run(args.run_dir)
    except AssertError as e:
        print("RED: %s" % e, file=sys.stderr)
        return 2
    for c in checks:
        print("%-4s %-32s %s" % ("OK" if c["ok"] else "RED", c["id"], c["detail"]))
    if rc == 0:
        print("GREEN: S1 后态断言全部通过（%d 项）" % len(checks))
    else:
        print("RED: %d/%d 项失败" % (sum(1 for c in checks if not c["ok"]), len(checks)))
    if args.json_out:
        with open(args.json_out, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"run": run, "checks": checks, "red": rc != 0}, f,
                      ensure_ascii=False, indent=2)
    return rc


def cmd_compare(args):
    a_snap = os.path.join(args.a, "after") if os.path.isdir(os.path.join(args.a, "after")) else args.a
    b_snap = os.path.join(args.b, "after") if os.path.isdir(os.path.join(args.b, "after")) else args.b
    tables = [t.strip() for t in args.tables.split(",") if t.strip()] or list(LB10_TABLES)
    ignore = [c.strip() for c in (args.ignore_columns or "").split(",") if c.strip()]
    try:
        rc, records, summary = dp.run_pair(
            a_snap, b_snap, tables=tables, require_tables=tables,
            key_mode=args.key_mode, time_mode=args.time_mode,
            ignore_columns=ignore, quiet=args.quiet)
    except dp.InputError as e:
        print("RED: %s" % e, file=sys.stderr)
        return 2
    dp.write_jsonl(args.jsonl, records)
    if args.summary_out:
        with open(args.summary_out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(summary, f, ensure_ascii=False, indent=2, sort_keys=True)
    return rc


# ---------------------------------------------------------------------------
# selftest：全离线
# ---------------------------------------------------------------------------
def _fixture_rows(name="Hero", account="acct1", is_deleted="0", select_id="0",
                  other_select_id="0", item_position="0"):
    r = {
        "mir2_db.characters": [
            dp._synth_row("mir2_db.characters", {"Id": "1", "ChrName": name,
                                                 "LoginID": account, "Level": "1"}),
        ],
        "mir2_db.characters_indexes": [
            dp._synth_row("mir2_db.characters_indexes",
                          {"Id": "1", "Account": account, "ChrName": name,
                           "SelectID": select_id, "IsDeleted": is_deleted}),
            dp._synth_row("mir2_db.characters_indexes",
                          {"Id": "2", "Account": account, "ChrName": name + "2",
                           "SelectID": other_select_id, "IsDeleted": "0"}),
        ],
        "mir2_db.characters_ablity": [
            dp._synth_row("mir2_db.characters_ablity", {"PlayerId": "1", "Level": "1"}),
        ],
        "mir2_db.characters_status": [
            dp._synth_row("mir2_db.characters_status", {"PlayerId": "1"}),
        ],
        "mir2_db.characters_item": [
            dp._synth_row("mir2_db.characters_item",
                          {"Id": "1", "PlayerId": "1", "Position": item_position}),
        ],
        "mir2_db.characters_bagitem": [
            dp._synth_row("mir2_db.characters_bagitem", {"Id": "1", "PlayerId": "1"}),
        ],
        "mir2_db.characters_storageitem": [
            dp._synth_row("mir2_db.characters_storageitem", {"Id": "1", "PlayerId": "1"}),
        ],
        # NewChr 不建这三张表；保留空表，使 compare 的 require-tables（LB-10 十表）成立。
        "mir2_db.characters_bonusability": [],
        "mir2_db.characters_magic": [],
        "mir2_db.characters_item_attr": [],
    }
    return r


def write_fake_run(d, sequence, rows, step_rcs=None, name="Hero", account="acct1"):
    os.makedirs(d, exist_ok=True)
    dp.write_snapshot(os.path.join(d, "after"), rows)
    steps = []
    for st in build_steps(sequence):
        steps.append({**st, "rc": (step_rcs or {}).get(st["step"], 0)})
    run = {"tool": "newchr_delchr_selchr_probe", "mode": "run", "db": "mir2_test_db",
           "account": account, "chr_name": name, "sequence": sequence, "steps": steps,
           "before_snapshot": "before", "after_snapshot": "after"}
    with open(os.path.join(d, "run.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(run, f, ensure_ascii=False, indent=2)


def _assert_rc(d):
    try:
        rc, checks, _ = check_run(d)
        return rc, checks
    except AssertError as e:
        return 2, [{"id": "input_error", "ok": False, "detail": str(e)}]


def cmd_selftest(args):
    tmp = tempfile.mkdtemp(prefix="chr-probe-selftest-")
    passed = failed = 0

    def report(name, ok, detail=""):
        nonlocal passed, failed
        if ok:
            passed += 1
            print("PASS %-46s %s" % (name, detail))
        else:
            failed += 1
            print("FAIL %-46s %s" % (name, detail))

    try:
        def run_case(name, seq, rows, want_rc, want_ids=None, step_rcs=None):
            d = os.path.join(tmp, "case-" + re.sub(r"\W+", "_", name))
            if os.path.exists(d):
                shutil.rmtree(d)
            write_fake_run(d, seq, rows, step_rcs=step_rcs)
            rc, checks = _assert_rc(d)
            failed_ids = {c["id"] for c in checks if not c["ok"]}
            ok = rc == want_rc and (want_ids is None or failed_ids == set(want_ids))
            report(name, ok, "rc=%d failed=%s" % (rc, sorted(failed_ids)))

        # 1) 规范后态 -> 绿
        run_case("create_green", "create", _fixture_rows(select_id="1"), 0)
        run_case("delete_green", "delete", _fixture_rows(is_deleted="1"), 0)

        # 2) 变异必红
        run_case("create_index_deleted_red", "create",
                 _fixture_rows(select_id="1", is_deleted="1"), 1, {"indexes.IsDeleted"})
        run_case("create_selectid_premature_red", "create",
                 _fixture_rows(select_id="1"), 1,
                 {"indexes.SelectID", "steps_all_ok"}, step_rcs={"SELCHR": 1})
        run_case("create_selectid_not_set_red", "create",
                 _fixture_rows(select_id="0"), 1, {"indexes.SelectID"})
        run_case("create_others_selected_red", "create",
                 _fixture_rows(select_id="1", other_select_id="1"), 1,
                 {"indexes.others.SelectID"})
        run_case("delete_isdeleted_stays_0_red", "delete", _fixture_rows(), 1,
                 {"indexes.IsDeleted"})
        rows_no_chr = _fixture_rows(is_deleted="1")
        rows_no_chr["mir2_db.characters"] = []
        run_case("delete_characters_row_gone_red", "delete", rows_no_chr, 1,
                 {"characters.still_present"})
        run_case("driver_step_failed_red", "create",
                 _fixture_rows(select_id="1"), 1, {"steps_all_ok"},
                 step_rcs={"NEWCHR": 1})

        # 3) 离线 compare：一致绿；改坏 Position 必红（M1 类比）
        a = os.path.join(tmp, "cmp-a")
        b = os.path.join(tmp, "cmp-b")
        write_fake_run(a, "create", _fixture_rows(select_id="1"))
        write_fake_run(b, "create", _fixture_rows(select_id="1"))
        rc, recs, _ = dp.run_pair(os.path.join(a, "after"), os.path.join(b, "after"),
                                  tables=list(LB10_TABLES), require_tables=list(LB10_TABLES),
                                  quiet=True)
        report("compare_identical_green", rc == 0, "rc=%d" % rc)
        write_fake_run(b, "create", _fixture_rows(select_id="1", item_position="1"))
        rc, recs, _ = dp.run_pair(os.path.join(a, "after"), os.path.join(b, "after"),
                                  tables=list(LB10_TABLES), require_tables=list(LB10_TABLES),
                                  quiet=True)
        has_pos = any(r.get("column") == "Position" for r in recs)
        report("compare_position_mutation_red", rc == 1 and has_pos, "rc=%d" % rc)

        # 4) 库名守卫
        report("db_guard_test_ok", validate_db_name("mir2_test_db") is None)
        report("db_guard_mir2_db_refused", validate_db_name("mir2_db") is not None)
        report("db_guard_prod_refused", validate_db_name("mir2_prod") is not None)
        report("db_guard_allow_db_ok",
               validate_db_name("mir2_db", ["mir2_db"]) is None)

        # 5) 快照表名必须重定到目标库（--db 是测试克隆时绝不能读 mir2_db）
        rt = retarget_tables(["mir2_db.characters", "mir2_db.characters_item"],
                             "mir2_test_db")
        report("snapshot_tables_retargeted",
               rt == ["mir2_test_db.characters", "mir2_test_db.characters_item"], str(rt))

        # 6) plan 的限频延迟必须是 oracle 口径（NEWCHR/DELCHR > 1000ms）
        plan_ok = True
        for seq in ("create", "delete"):
            for st in build_steps(seq):
                if st["step"] in ("NEWCHR", "DELCHR") and st["delay_before_ms"] <= 1000:
                    plan_ok = False
        report("plan_rate_limit_delays", plan_ok)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    total = passed + failed
    if failed:
        print("RED: newchr_delchr_selchr_probe selftest %d/%d" % (passed, total))
        return 1
    print("GREEN: newchr_delchr_selchr_probe selftest %d/%d" % (passed, total))
    return 0


def _add_common_db_args(p):
    p.add_argument("--db", default="mir2_test_db", help="库名；test=mir2_test_db")
    p.add_argument("--allow-db", action="append", default=[],
                   help="显式放行非 mir2_test_* 库（会告警）")
    p.add_argument("--tables", default=",".join(LB10_TABLES),
                   help="快照表清单（逗号分隔，默认 LB-10 十表）")
    p.add_argument("--dbsnap", default=DEFAULT_DBSNAP)
    p.add_argument("--mysql", default=r"D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe")
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=3306)
    p.add_argument("--user", default="root")
    p.add_argument("--password", default="")


def build_parser():
    p = argparse.ArgumentParser(description="建角/删角/选角库后态采集与离线比对骨架")
    sub = p.add_subparsers(dest="cmd", required=True)

    q = sub.add_parser("plan", help="打印规范步骤序列与期望后态（离线）")
    q.add_argument("--account", default="acct1")
    q.add_argument("--name", default="Hero")
    q.add_argument("--db", default="mir2_test_db")
    q.add_argument("--out")

    s = sub.add_parser("snap", help="只读快照（调用 dbsnap；库名受限）")
    _add_common_db_args(s)
    s.add_argument("--out", required=True)

    r = sub.add_parser("run", help="按序列调用外部驱动并前后快照（R 窗口用）")
    _add_common_db_args(r)
    r.add_argument("--sequence", choices=sorted(SEQUENCES), required=True)
    r.add_argument("--account", default="acct1")
    r.add_argument("--name", default="Hero")
    r.add_argument("--out", required=True)
    r.add_argument("--driver-cmd", help="命令模板，支持 {step}/{account}/{name}/{db}")
    r.add_argument("--timeout", type=int, default=30)
    r.add_argument("--stop-on-fail", action="store_true", default=True)
    r.add_argument("--continue-on-fail", dest="stop_on_fail", action="store_false")

    a = sub.add_parser("assert", help="离线机判 S1 后态")
    a.add_argument("--run-dir", required=True)
    a.add_argument("--json-out")

    c = sub.add_parser("compare", help="两份 run 目录逐字段 diff（调用 dbsnap_pair）")
    c.add_argument("--a", required=True)
    c.add_argument("--b", required=True)
    c.add_argument("--tables", default=",".join(LB10_TABLES))
    c.add_argument("--key-mode", choices=["pk", "multiset"], default="pk")
    c.add_argument("--time-mode", choices=["exact", "presence", "normalized"],
                   default="exact")
    c.add_argument("--ignore-columns", default="")
    c.add_argument("--jsonl", default="", help="差异 JSONL（- = stdout；默认不输出）")
    c.add_argument("--summary-out")
    c.add_argument("--quiet", action="store_true")

    sub.add_parser("selftest", help="全离线自检")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.cmd == "selftest":
        return cmd_selftest(args)
    if args.cmd == "plan":
        return cmd_plan(args)
    if args.cmd == "snap":
        return cmd_snap(args)
    if args.cmd == "run":
        return cmd_run(args)
    if args.cmd == "assert":
        return cmd_assert(args)
    if args.cmd == "compare":
        return cmd_compare(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
