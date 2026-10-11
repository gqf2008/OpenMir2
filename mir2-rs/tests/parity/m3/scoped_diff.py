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
from collections import Counter

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
    # ---- 角色族表（PlayerId 外键，行里没有名字 ⇒ 归属判定见 FAMILY_TABLES）----
    # 列序 = `SELECT *`（与 dbsnap 的 ORDER BY 全列序一致）。新增列会在这里 fail-closed。
    "mir2_db.characters_ablity": [
        "PlayerId", "Level", "Ac", "Mac", "Dc", "Mc", "Sc", "Hp", "Mp", "MaxHP",
        "MAxMP", "Exp", "MaxExp", "Weight", "MaxWeight", "WearWeight", "MaxWearWeight",
        "HandWeight", "MaxHandWeight", "ModifyTime",
    ],
    "mir2_db.characters_item": [
        "Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax",
    ],
    "mir2_db.characters_bagitem": [
        "Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax",
    ],
    "mir2_db.characters_storageitem": [
        "Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax",
    ],
    "mir2_db.characters_bonusability": [
        "PLAYERID", "AC", "MAC", "DC", "MC", "SC", "HP", "MP", "HIT", "SPEED", "RESERVED",
    ],
    "mir2_db.characters_status": [
        "PlayerId", "Status0", "Status1", "Status2", "Status3", "Status4", "Status5",
        "Status6", "Status7", "Status8", "Status9", "Status10", "Status11", "Status12",
        "Status13", "Status14", "Status15",
    ],
}

# 角色族表：以 `PlayerId` 外键归属本次运行的角色。行里**没有名字**，所以 `extract` 不能靠
# "行内出现 prefix" 命中（旧实现在这里就是静默缺口：物品行/Abil 行从来没进过作用域）。
# 归属判定 = `PlayerId ∈ {本次运行建出的 characters.Id}`（由 characters 表的 prefix 匹配先解出）。
FAMILY_TABLES = {
    "mir2_db.characters_ablity": "PlayerId",
    "mir2_db.characters_item": "PlayerId",
    "mir2_db.characters_bagitem": "PlayerId",
    "mir2_db.characters_storageitem": "PlayerId",
    "mir2_db.characters_bonusability": "PLAYERID",
    "mir2_db.characters_status": "PlayerId",
}
CHARACTER_TABLE = "mir2_db.characters"

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
    # ---- 角色族表（PlayerId 外键）----
    # 只把「自增 PK / 外键」与「墙钟」登记为易变；**行为列（Position/MakeIndex/StdIndex/
    # Dura/DuraMax/Abil 各字段/Status*）一律逐字节比**，不拿白名单凑绿。
    # 注意：`MakeIndex` 是物品实例号，**故意不登记为易变**——它是不是跨侧确定，属被比对行为，
    # 由实现决定（若将来实测证明确属不可复现的熵源，须按 owner 裁决单独登记并写明理由）。
    "mir2_db.characters_ablity": {
        "PlayerId": "FK = characters.Id（自增，每次运行必变）",
        "ModifyTime": "墙钟 datetime（每次保存刷新）",
    },
    "mir2_db.characters_item": {
        "Id": "自增 PK（每次运行必变）",
        "PlayerId": "FK = characters.Id（自增，每次运行必变）",
    },
    "mir2_db.characters_bagitem": {
        "Id": "自增 PK（每次运行必变）",
        "PlayerId": "FK = characters.Id（自增，每次运行必变）",
    },
    "mir2_db.characters_storageitem": {
        "Id": "自增 PK（每次运行必变）",
        "PlayerId": "FK = characters.Id（自增，每次运行必变）",
    },
    "mir2_db.characters_bonusability": {
        "PLAYERID": "FK = characters.Id（自增，每次运行必变）",
    },
    "mir2_db.characters_status": {
        "PlayerId": "FK = characters.Id（自增，每次运行必变）",
    },
}

# 固定建角场景（卡 openmir2-m3-botsrv-fixed-newchr）下，`characters` 的建角输入列
# Hair/Sex/Job 不再是"夹具自身随机"——BotSrv 用配置固定了它们（RobotOptions.FixedNewChr
# + NewChrJob/Hair/Sex），两侧输入同序列 ⇒ 这三列必须逐字节比（该场景判据要求
# "Job 列不置占位、真的比到值"）。其余易变列（自增 PK / 墙钟 datetime）仍按白名单置占位。
FIXED_NEWCHR_NOT_VOLATILE = {
    "mir2_db.characters": {"Hair", "Sex", "Job"},
}


def canonicalize_line(table, line, fixed_newchr=False):
    """把该表白名单列的列值替换为占位符，返回规范化后的整行（其余列逐字节不动）。

    fixed_newchr=True 时，固定建角场景的建角输入列（Hair/Sex/Job）从占位白名单里移除，
    改为逐字节比较——这是本场景"真的比到值"的实现（见 scoped_diff `--fixed-newchr`）。
    """
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
    volatile = dict(VOLATILE_COLUMNS.get(table, {}))
    if fixed_newchr:
        for name in FIXED_NEWCHR_NOT_VOLATILE.get(table, set()):
            volatile.pop(name, None)
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


def _read_fields(path, ncols, table):
    """逐行读 TSV，返回 (raw_line_bytes, fields_str_list)。列数不符即 fail-closed。"""
    with open(path, "rb") as f:
        for raw in f:
            if not raw.strip():
                continue
            line = raw.rstrip(b"\r\n")
            fields = line.decode("utf-8", "replace").split("\t")
            if len(fields) != ncols:
                print(f"RED: {table} 列数不符（快照 {len(fields)} vs 登记 {ncols}）"
                      f"——表结构已变或登记过期", file=sys.stderr)
                sys.exit(4)
            yield raw, line, fields


def player_ids_for_prefix(snapdir, prefix):
    """第一遍：解出本次运行建出的角色 `Id` 集合（角色族表用 PlayerId 外键归属）。

    角色族的行里没有名字，靠 `prefix in row` 命中不了（旧实现里 物品/Abil/Status 行
    从来没进过作用域——这是"悄悄不比"的静默缺口）。这里先把 characters 表里
    命中 prefix 的行收出来，再以 `Id` 归属族表。
    """
    cols = TABLE_COLUMNS[CHARACTER_TABLE]
    id_idx = cols.index("Id")
    ids = set()
    path = None
    for table, p in iter_tables(snapdir):
        if table == CHARACTER_TABLE:
            path = p
            break
    if path is None:
        print(f"RED: 快照缺 {CHARACTER_TABLE}——无法归属 PlayerId 族表（物品/Abil 会成静默缺口）",
              file=sys.stderr)
        sys.exit(3)
    for _raw, _line, fields in _read_fields(path, len(cols), CHARACTER_TABLE):
        if prefix not in "\t".join(fields):
            continue
        ids.add(fields[id_idx])
    return ids


def cmd_extract(args):
    prefix = args.prefix.encode("utf-8", "replace")
    run_player_ids = player_ids_for_prefix(args.snapdir, args.prefix)
    out_rows = 0
    family_rows = 0
    with open(args.out, "w", encoding="utf-8", newline="\n") as fo:
        for table, path in iter_tables(args.snapdir):
            cols = TABLE_COLUMNS.get(table)
            if cols is None:
                print(f"RED: 快照里有未登记表 {table}——先在 scoped_diff.py 的 TABLE_COLUMNS/"
                      f"VOLATILE_COLUMNS 里登记（含每列理由），不许悄悄不比", file=sys.stderr)
                sys.exit(4)
            player_key = FAMILY_TABLES.get(table)
            key_idx = cols.index(player_key) if player_key is not None else None
            for raw, line, fields in _read_fields(path, len(cols), table):
                if key_idx is not None:
                    if fields[key_idx] not in run_player_ids:
                        continue
                    family_rows += 1
                elif prefix not in raw:
                    continue
                text = line.decode("utf-8", "replace")
                if args.normalize:
                    text = text.replace(args.prefix, DEFAULT_PREFIX_MASK)
                digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
                fo.write(json.dumps({"table": table, "row_sha256": digest, "line": text},
                                    ensure_ascii=False) + "\n")
                out_rows += 1
    print(f"EXTRACT prefix={args.prefix} rows={out_rows} (family_by_playerid={family_rows} "
          f"player_ids={len(run_player_ids)}) -> {args.out}")
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


def multiset_delta(ra, rb):
    """按 **(表内行 key) 的出现次数** 做多重集差，返回 (仅A多出的计数, 仅B多出的计数)。

    为什么不是 set 差集：`row_sha256` 相同即同一行，**重复行数本身是行为的一部分**。
    用 set 差集时 `A=[x,x]` vs `B=[x]` 的 `ha != hb` 成立但 `only_a`/`only_b` 都为空
    ⇒ problems 累加 0、exit 0（**假绿**）。计数式比对把「多出一行」计成 1，判据恢复仲裁力。
    返回 Counter（键=row_sha256，值=该侧多出的次数），调用方用 sum(values()) 得行数。
    """
    ca = Counter(r["row_sha256"] for r in ra)
    cb = Counter(r["row_sha256"] for r in rb)
    return ca - cb, cb - ca


def cmd_diff(args):
    a = bucket(load_rows(args.a))
    b = bucket(load_rows(args.b))
    if getattr(args, "canonical", False):
        fixed_newchr = getattr(args, "fixed_newchr", False)
        for rows in (a, b):
            for table, rs in rows.items():
                for r in rs:
                    r["line"] = canonicalize_line(table, r["line"], fixed_newchr)
                    r["row_sha256"] = hashlib.sha256(r["line"].encode("utf-8")).hexdigest()
    problems = 0
    summary = {}
    for table in sorted(set(a) | set(b)):
        ra, rb = a.get(table, []), b.get(table, [])
        only_a, only_b = multiset_delta(ra, rb)
        n_only_a, n_only_b = sum(only_a.values()), sum(only_b.values())
        summary[table] = {"rows_a": len(ra), "rows_b": len(rb),
                          "only_a": n_only_a, "only_b": n_only_b}
        if n_only_a or n_only_b:
            problems += n_only_a + n_only_b
            print(f"DIFF {table}: rows A={len(ra)} B={len(rb)} 仅A={n_only_a} 仅B={n_only_b}")
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
        removed_c, added_c = multiset_delta(a.get(table, []), b.get(table, []))
        added, removed = sum(added_c.values()), sum(removed_c.values())
        if added or removed:
            delta[table] = {"added": added, "removed": removed}
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


def _write_tbl(path, table, lines):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        for line in lines:
            digest = hashlib.sha256(line.encode("utf-8")).hexdigest()
            f.write(json.dumps({"table": table, "row_sha256": digest, "line": line},
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
    # —— 用例 1/9：非规范化路径，原行为不变（相同→绿、多一行→红）
    with open(a, "w", encoding="utf-8", newline="\n") as f:
        for line in base:
            f.write(json.dumps({"table": "t", "row_sha256": hashlib.sha256(line.encode()).hexdigest(),
                                "line": line}, ensure_ascii=False) + "\n")
    with open(b, "w", encoding="utf-8", newline="\n") as f:
        for line in base:
            f.write(json.dumps({"table": "t", "row_sha256": hashlib.sha256(line.encode()).hexdigest(),
                                "line": line}, ensure_ascii=False) + "\n")
    ns = argparse.Namespace(a=a, b=b, max_diffs=5, summary_out=None, canonical=False,
                            fixed_newchr=False)
    expect("1/9 未规范化：相同作用域判绿", ns, 0)
    with open(b, "a", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps({"table": "t", "row_sha256": "cc", "line": "m3bot3\t9"},
                           ensure_ascii=False) + "\n")
    expect("2/9 未规范化：多一行即判红", ns, 1)

    # —— 用例 3/9：--canonical 下，仅易变列（Id/Job/时间戳）不同 ⇒ 必须判绿
    _write_jsonl(a, [_chr_line(12234, 1, 0)])
    _write_jsonl(b, [_chr_line(13239, 0, 0)])
    ns_c = argparse.Namespace(a=a, b=b, max_diffs=5, summary_out=None, canonical=True,
                              fixed_newchr=False)
    expect("3/9 规范化后：仅易变列不同判绿", ns_c, 0)

    # —— 用例 4/9（改坏必红）：--canonical 下，改动**被比对列**（Gold）⇒ 必须判红
    _write_jsonl(b, [_chr_line(13239, 0, 999)])
    expect("4/9 规范化后：被比对列被改坏即判红", ns_c, 1)

    # —— 用例 5/9：未登记表 fail-closed
    with open(b, "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps({"table": "mir2_db.unknown", "row_sha256": "x", "line": "1\t2"},
                           ensure_ascii=False) + "\n")
    expect("5/9 未登记表 fail-closed", ns_c, 4)

    # —— 用例 6/9（固定建角场景）：--fixed-newchr 下 Job/Hair/Sex **不**置占位；
    #    固定了参数 ⇒ Job 相同；仅自增 Id 变 ⇒ 必须判绿。
    ns_fx = argparse.Namespace(a=a, b=b, max_diffs=5, summary_out=None, canonical=True,
                               fixed_newchr=True)
    _write_jsonl(a, [_chr_line(12234, 1, 0)])
    _write_jsonl(b, [_chr_line(13239, 1, 0)])
    expect("6/9 固定建角场景：Job 固定且不置占位，仅 Id 变判绿", ns_fx, 0)

    # —— 用例 7/9（改坏必红）：固定建角场景下 Job 真的被比，改动即红色
    #    （同数据在 --canonical 不带 --fixed-newchr 时会因 Job 占位而误判绿）。
    _write_jsonl(b, [_chr_line(13239, 0, 0)])
    expect("7/9 固定建角场景：Job 被改坏即判红", ns_fx, 1)

    # —— 用例 8/9（多重集/计数式，卡 openmir2-m3-scopeddiff-multiset）：
    #    重复行 A=[x,x] vs B=[x] 必须判红——旧实现用 set 差集，仅A/仅B 都是 0 ⇒ 假绿。
    #    注：这里 write 用默认 hash 路径（不规范化），确保核的是 cmd_diff 的计数语义本身。
    dup = "m3bot1\t1"
    dup_row = json.dumps({"table": "t",
                          "row_sha256": hashlib.sha256(dup.encode()).hexdigest(),
                          "line": dup}, ensure_ascii=False)
    with open(a, "w", encoding="utf-8", newline="\n") as f:
        f.write(dup_row + "\n")
        f.write(dup_row + "\n")
    with open(b, "w", encoding="utf-8", newline="\n") as f:
        f.write(dup_row + "\n")
    expect("8/9 重复行：A=[x,x] vs B=[x] 判红（旧实现假绿）", ns, 1)

    # —— 用例 9/9：重复行两侧相同 ⇒ 必须判绿（避免「见重复即红」的过度收紧）。
    with open(b, "w", encoding="utf-8", newline="\n") as f:
        f.write(dup_row + "\n")
        f.write(dup_row + "\n")
    expect("9/9 重复行：A=[x,x] vs B=[x,x] 判绿", ns, 0)

    # ---- 10/12：族表归属（PlayerId 外键）——extract 必须把物品/Abil 行纳入作用域，
    #      且只纳入本次运行角色的行（别的 PlayerId 不许混入）。
    snap = os.path.join(tmp, "snap")
    os.makedirs(snap, exist_ok=True)
    item_cols = TABLE_COLUMNS["mir2_db.characters_item"]
    abil_cols = TABLE_COLUMNS["mir2_db.characters_ablity"]

    def _row(cols, **vals):
        d = {c: "0" for c in cols}
        d.update(vals)
        return "\t".join(d[c] for c in cols)

    snap_files = {
        "mir2_db.characters": ("mir2_db.characters.tsv", [_chr_line(42, 1, 0)]),
        "mir2_db.characters_item": ("mir2_db.characters_item.tsv", [
            _row(item_cols, Id="1", PlayerId="42", Position="0", MakeIndex="7001",
                 StdIndex="118", Dura="10", DuraMax="10"),
            _row(item_cols, Id="2", PlayerId="99", Position="0", MakeIndex="7002",
                 StdIndex="9", Dura="1", DuraMax="1"),
        ]),
        "mir2_db.characters_ablity": ("mir2_db.characters_ablity.tsv", [
            _row(abil_cols, PlayerId="42", Level="1", Dc="1", MaxHP="15",
                 ModifyTime="2026-10-11 00:00:00"),
        ]),
    }
    manifest = {"tool": "dbsnap", "tables": []}
    for t, (fn, rows) in snap_files.items():
        with open(os.path.join(snap, fn), "w", encoding="utf-8", newline="\n") as f:
            for r in rows:
                f.write(r + "\n")
        manifest["tables"].append({"table": t, "file": fn, "rows": len(rows)})
    with open(os.path.join(snap, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f)
    out = os.path.join(tmp, "extract.jsonl")
    cmd_extract(argparse.Namespace(snapdir=snap, prefix="m3bot", out=out, normalize=False,
                                   allow_empty=False))
    extracted = [json.loads(single) for single in open(out, encoding="utf-8")]
    got_tables = Counter(r["table"] for r in extracted)
    if (got_tables.get("mir2_db.characters_item") != 1
            or got_tables.get("mir2_db.characters_ablity") != 1):
        print(f"RED: 10/12 族表未按 PlayerId 纳入作用域: {dict(got_tables)}", file=sys.stderr)
        sys.exit(1)
    if any("\t99\t" in r["line"] for r in extracted):
        print("RED: 10/12 非本次运行的 PlayerId=99 行混入作用域", file=sys.stderr)
        sys.exit(1)
    print("PASS 10/12 族表（物品/Abil）按 PlayerId 归属纳入作用域，且不混入他人行")

    # ---- 11/12：族表规范化——PlayerId/Id 是易变列（自增 PK/FK），其余列相同 ⇒ 判绿
    _write_tbl(a, "mir2_db.characters_item", [
        _row(item_cols, Id="1", PlayerId="42", Position="0", MakeIndex="7001",
             StdIndex="118", Dura="10", DuraMax="10")])
    _write_tbl(b, "mir2_db.characters_item", [
        _row(item_cols, Id="7", PlayerId="51", Position="0", MakeIndex="7001",
             StdIndex="118", Dura="10", DuraMax="10")])
    expect("11/12 族表：仅 PlayerId/Id 易变 ⇒ 判绿", ns_c, 0)

    # ---- 12/12（改坏必红）：族表被比对列（StdIndex）改坏 ⇒ 判红
    _write_tbl(b, "mir2_db.characters_item", [
        _row(item_cols, Id="7", PlayerId="51", Position="0", MakeIndex="7001",
             StdIndex="119", Dura="10", DuraMax="10")])
    expect("12/12 族表：被比对列（StdIndex）改坏即判红", ns_c, 1)

    print("GREEN: scoped_diff selftest 12/12（含规范化前后 + 固定建角不置占位 + 改坏必红 "
          "+ 多重集重复行 + fail-closed + 族表 PlayerId 归属）")
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
    f.add_argument("--fixed-newchr", action="store_true",
                   help="固定建角场景（卡 openmir2-m3-botsrv-fixed-newchr）：characters 的 "
                        "Hair/Sex/Job 不置占位、逐字节比；须配合 --canonical")

    sub.add_parser("selftest")
    args = ap.parse_args()
    {"extract": cmd_extract, "delta": cmd_delta, "diff": cmd_diff, "selftest": cmd_selftest}[args.cmd](args)


if __name__ == "__main__":
    main()
