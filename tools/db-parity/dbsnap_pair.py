#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dbsnap_pair.py — LB-10 字段级证据：两份 dbsnap 快照的逐表/逐行/逐列 diff。

定位
====
`tools/dbsnap/dbsnap.py` 负责「拍快照 + 整文件 hash 比对」；本工具负责**下一层**：
把两份快照按**主键对齐**后逐列比较，产出机判 JSONL + 摘要，供 LB-10（9 表 +
`characters_indexes`）的跨实现证据使用。快照格式**完全复用 dbsnap**，不另造格式：

    <snapdir>/manifest.json
    <snapdir>/<db>.<table>.tsv        # mysql --batch --raw 输出，无表头，TAB 分隔

manifest 的每个表项至少要有 `table` / `file` / `sha256` / `rows` / `columns`
（`columns` 是 `[{"name":...,"type":...}]`，序 = `SELECT *` 列序）。缺 `columns`
时回退到本工具登记列；**列名/列序与登记不符 ⇒ RED（fail-closed）**，不静默跳过。

口径（显式定义，评审请按此复核）
================================
1) 主键与排序
   - 每张表的主键登记在 `TABLE_SPECS[...]["pk"]`（与 `sql/` 的 CREATE TABLE 一致）。
   - 默认 `--key-mode pk`：按主键元组对齐；行序无关（不是靠 tsv 行号比）。
     同一侧出现**重复主键** ⇒ 立即 RED（`duplicate_key`），因为对齐本身不唯一。
   - `--key-mode multiset`：把「比较列」拼成行签名做多重集比较；用于两侧自增 Id
     不同、又没有可信业务键的场景。此模式下 `Id` 等应通过 `--ignore-columns` 排除，
     并在报告里如实登记「身份列未比较」。
   - 排序：输出按 (表名, 主键/签名) 排序，保证同一输入产出字节稳定。

2) NULL 口径
   - dbsnap 用 `mysql --batch --raw`，SQL NULL 落成 4 字节字面量 `NULL`。
     本工具默认把字段值 **恰好等于 `NULL`** 判为 SQL NULL（`--null-token` 可改）。
   - 已登记限制（同 dbsnap L2）：**字符串值 "NULL" 与 SQL NULL 不可区分**。
     这不是本工具引入的，快照格式本身如此；需要区分时请让 dbsnap 换转义口径。
   - NULL vs 非 NULL 在任何 `--time-mode` 下都判差异（presence 模式只比这一位）。

3) 时间列口径
   - 每张表的时间列登记在 `TABLE_SPECS[...]["time_columns"]`。
   - `--time-mode exact`（默认）：逐字节比（两侧同一基线/同一时钟源时用）。
   - `--time-mode normalized`：DATETIME 统一为 `YYYY-MM-DD HH:MM:SS.ffffff`
     （小数位右补 0 到 6 位；`T` 分隔符归一为空格）；纯整数时间列（unix 秒）
     按整数比较；无法识别的格式按 exact 比，并在摘要 `unrecognized_time_values`
     里计数（不静默）。
   - `--time-mode presence`：只比 NULL/非 NULL，不比时刻值。
   - 时间列**不自动忽略**：要忽略必须显式 `--ignore-columns`（或探针的
     `--ignore-time-columns`），且仅当该列已登记为易变列或时间列时允许；否则工具
     拒绝（rc=2）并说明。

4) C3「changed 标志」模式
   - `--baseline-a BEFORE_A --baseline-b BEFORE_B` 与两份 after 一起给时，额外计算
     每侧 `changed = (after_row != before_row)`（按比较列 + time-mode），并机判：
       · oracle changed=false 的行，Rust 必须也 false；
       · oracle changed=true  的行，Rust 必须也 true；
       · changed 的时间列 after 必须**严格晚于** before（相等或回退都报
         `time_not_later`）；DATETIME 与 unix 秒均支持；
       · before 与 after 走同一套结构校验（hash/columns/rows/重复主键）。
     这直接对应 design-dbsrv-connection-20261021.md §6.4 C3。

5) 表集合
   - 默认比较「两份 manifest 里出现的所有表」；未登记表 ⇒ RED（`unknown_table`），
     除非 `--allow-unknown-table`（届时只能 multiset 比，且摘要显式标注）。
   - `--require-tables` 点名必须在两侧都出现的表，且**进入比较集**（收窄
     `--tables` 也不会被跳过）；缺失 ⇒ RED（`missing_table`）。
   - 表名按 `<任意库前缀>.<表名>` 归一到登记名后对齐：E1 克隆
     `mir2_test_db_oracle` / `mir2_test_db_rust` 仅前缀不同也能比。
   - `--scope lb10` = LB-10 的 9 表 + `characters_indexes`（quest 不在其内）。

退出码：0=GREEN，1=RED（有差异/结构问题），2=用法或输入错误（fail-closed）。

自检
====
`python dbsnap_pair.py selftest` 全离线：在临时目录里合成 dbsnap 格式快照，
逐项验证「一致判绿 / 改坏必红」，不连 MySQL、不写数据库。每次变异都要求
指定断言必须红（见末尾 PASS 列表）。
"""
import argparse
import hashlib
import json
import os
import re
import sys

# ---------------------------------------------------------------------------
# 表登记（列序 = SELECT *；与 mir2-rs/tests/parity/m3/scoped_diff.py 的
# TABLE_COLUMNS / VOLATILE_COLUMNS 同源。selftest 会做漂移对账。）
# ---------------------------------------------------------------------------
TABLE_SPECS = {
    "mir2_account.account": {
        "columns": ["Id", "Account", "PassWord", "PassFailCount", "PassFailTime",
                    "ValidFrom", "ValidUntil", "PayMode", "Seconds", "StopUntil",
                    "State", "CreateTime", "ModifyTime", "LastLoginTime"],
        "pk": ["Id"],
        "time_columns": ["ValidFrom", "ValidUntil", "StopUntil", "CreateTime",
                         "ModifyTime", "LastLoginTime"],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "ValidFrom": "墙钟 datetime", "ValidUntil": "墙钟 datetime",
                     "StopUntil": "墙钟 datetime", "CreateTime": "unix 秒（墙钟）",
                     "ModifyTime": "unix 秒（墙钟）", "LastLoginTime": "unix 秒（墙钟）"},
    },
    "mir2_account.account_protection": {
        "columns": ["AccountId", "UserName", "IdCard", "Birthday", "Phone",
                    "MobilePhone", "ADDRESS1", "ADDRESS2", "EMail", "Quiz1",
                    "Answer1", "Quiz2", "Answer2"],
        "pk": ["AccountId"],
        "time_columns": ["Birthday"],
        "volatile": {"AccountId": "FK = account.Id（自增）"},
    },
    "mir2_db.characters": {
        "columns": ["Id", "ServerIndex", "LoginID", "ChrName", "MapName", "CX", "CY",
                    "Level", "Dir", "Hair", "Sex", "Job", "Gold", "GamePoint", "HomeMap",
                    "HomeX", "HomeY", "PkPoint", "ReLevel", "AttatckMode",
                    "FightZoneDieCount", "BodyLuck", "IncHealth", "IncSpell",
                    "IncHealing", "CreditPoint", "BonusPoint", "HungerStatus",
                    "PayMentPoint", "LockLogon", "MarryCount", "AllowGroup",
                    "AllowGroupReCall", "GroupRcallTime", "AllowGuildReCall", "IsMaster",
                    "MasterName", "DearName", "StoragePwd", "Deleted", "CREATEDATE",
                    "LASTUPDATE"],
        "pk": ["Id"],
        "time_columns": ["CREATEDATE", "LASTUPDATE"],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "Hair": "夹具建角输入（BotSrv Random(1)）",
                     "Sex": "夹具建角输入（BotSrv Random(1)）",
                     "Job": "夹具建角输入（BotSrv Random(2)）",
                     "CREATEDATE": "墙钟 datetime", "LASTUPDATE": "墙钟 datetime"},
    },
    "mir2_db.characters_indexes": {
        "columns": ["Id", "Account", "ChrName", "SelectID", "IsDeleted",
                    "CreateDate", "ModifyDate"],
        "pk": ["Id"],
        "time_columns": ["CreateDate", "ModifyDate"],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "CreateDate": "墙钟 datetime", "ModifyDate": "墙钟 datetime"},
    },
    "mir2_db.characters_ablity": {
        "columns": ["PlayerId", "Level", "Ac", "Mac", "Dc", "Mc", "Sc", "Hp", "Mp",
                    "MaxHP", "MAxMP", "Exp", "MaxExp", "Weight", "MaxWeight",
                    "WearWeight", "MaxWearWeight", "HandWeight", "MaxHandWeight",
                    "ModifyTime"],
        "pk": ["PlayerId"],
        "time_columns": ["ModifyTime"],
        "volatile": {"PlayerId": "FK = characters.Id（自增）",
                     "ModifyTime": "墙钟 datetime"},
    },
    "mir2_db.characters_item": {
        "columns": ["Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura",
                    "DuraMax"],
        "pk": ["Id"],
        "time_columns": [],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "PlayerId": "FK = characters.Id（自增）"},
    },
    "mir2_db.characters_bagitem": {
        "columns": ["Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura",
                    "DuraMax"],
        "pk": ["Id"],
        "time_columns": [],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "PlayerId": "FK = characters.Id（自增）"},
    },
    "mir2_db.characters_storageitem": {
        "columns": ["Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura",
                    "DuraMax"],
        "pk": ["Id"],
        "time_columns": [],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "PlayerId": "FK = characters.Id（自增）"},
    },
    "mir2_db.characters_bonusability": {
        "columns": ["PLAYERID", "AC", "MAC", "DC", "MC", "SC", "HP", "MP", "HIT",
                    "SPEED", "RESERVED"],
        "pk": ["PLAYERID"],
        "time_columns": [],
        "volatile": {"PLAYERID": "FK = characters.Id（自增）"},
    },
    "mir2_db.characters_status": {
        "columns": ["PlayerId", "Status0", "Status1", "Status2", "Status3", "Status4",
                    "Status5", "Status6", "Status7", "Status8", "Status9", "Status10",
                    "Status11", "Status12", "Status13", "Status14", "Status15"],
        "pk": ["PlayerId"],
        "time_columns": [],
        "volatile": {"PlayerId": "FK = characters.Id（自增）"},
    },
    "mir2_db.characters_item_attr": {
        "columns": ["Id", "PlayerId", "MakeIndex", "Value0", "Value1", "Value2",
                    "Value3", "Value4", "Value5", "Value6", "Value7", "Value8",
                    "Value9", "Value10", "Value11", "Value12", "Value13"],
        "pk": ["Id"],
        "time_columns": [],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "PlayerId": "FK = characters.Id（自增）"},
    },
    "mir2_db.characters_magic": {
        "columns": ["Id", "PlayerId", "MagicId", "Level", "UseKey", "CurrTrain"],
        "pk": ["Id"],
        "time_columns": [],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "PlayerId": "FK = characters.Id（自增）"},
    },
    "mir2_db.characters_quest": {
        "columns": ["Id", "PLAYERID", "QUESTOPENINDEX", "QUESTFININDEX", "QUEST"],
        "pk": ["Id"],
        "time_columns": [],
        "volatile": {"Id": "自增 PK（每次运行必变）",
                     "PLAYERID": "FK = characters.Id（自增；列名全大写、可空）"},
    },
}

# LB-10 的取证范围：9 表（characters + 8 张附属表，quest 不在内）+ characters_indexes。
LB10_TABLES = [
    "mir2_db.characters",
    "mir2_db.characters_indexes",
    "mir2_db.characters_ablity",
    "mir2_db.characters_item",
    "mir2_db.characters_bagitem",
    "mir2_db.characters_storageitem",
    "mir2_db.characters_bonusability",
    "mir2_db.characters_status",
    "mir2_db.characters_item_attr",
    "mir2_db.characters_magic",
]

NULL_TOKEN_DEFAULT = "NULL"
_DT_RE = re.compile(r"^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})(?:\.(\d{1,6}))?$")
_UNIX_RE = re.compile(r"^\d+$")


def canonical_table_name(fq):
    """把 `<任意库前缀>.<表名>` 归一到 TABLE_SPECS 的登记名。

    E1 裁决用测试库克隆（`mir2_test_db_oracle`/`mir2_test_db_rust`），dbsnap 快照里
    的表名带各自库前缀。两侧只差前缀时也必须能按同一张表对齐比较，否则 compare 会
    报 `missing_table`/`unknown_table`（LB-10 实测缺陷）。未登记的表原样返回。
    """
    if fq in TABLE_SPECS:
        return fq
    bare = fq.rpartition(".")[2]
    matches = [k for k in TABLE_SPECS if k.rpartition(".")[2] == bare]
    return matches[0] if len(matches) == 1 else fq


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def is_null(value, null_token):
    return value == null_token


def norm_time(value):
    """DATETIME -> YYYY-MM-DD HH:MM:SS.ffffff；整数/未知格式原样返回。"""
    m = _DT_RE.match(value)
    if m:
        frac = (m.group(3) or "").ljust(6, "0")
        return "%s %s.%s" % (m.group(1), m.group(2), frac)
    return value


def time_value_kind(value):
    """时间列值分类：'dt'=DATETIME 文本、'unix'=纯整数秒、None=无法识别。"""
    if _DT_RE.match(value):
        return "dt"
    if _UNIX_RE.match(value):
        return "unix"
    return None


def parse_time_value(value):
    """把时间列值变成可比较键：('dt', 归一文本) / ('unix', int) / None。"""
    kind = time_value_kind(value)
    if kind == "dt":
        return ("dt", norm_time(value))
    if kind == "unix":
        return ("unix", int(value))
    return None


def values_equal(a, b, *, is_time, time_mode, null_token, stats=None):
    """单列比较。非时间列永远逐字节；时间列按 time_mode。"""
    a_null = is_null(a, null_token)
    b_null = is_null(b, null_token)
    if a_null or b_null:
        return a_null and b_null
    if not is_time or time_mode == "exact":
        return a == b
    if time_mode == "presence":
        return True
    if time_mode == "normalized":
        # 无法识别的格式按 exact 比，并计数（摘要里显式标注，不静默）。
        if stats is not None and (time_value_kind(a) is None
                                  or time_value_kind(b) is None):
            stats["unrecognized_time_values"] = (
                stats.get("unrecognized_time_values", 0) + 1)
        return norm_time(a) == norm_time(b)
    raise ValueError("未知 time_mode: %s" % time_mode)


class InputError(Exception):
    pass


def load_manifest(snapdir):
    path = os.path.join(snapdir, "manifest.json")
    if not os.path.exists(path):
        raise InputError("缺 manifest.json: %s" % path)
    try:
        with open(path, "r", encoding="utf-8") as f:
            man = json.load(f)
    except (OSError, ValueError) as e:
        raise InputError("manifest.json 不可读: %s (%s)" % (path, e))
    if not isinstance(man.get("tables"), list):
        raise InputError("manifest 缺 tables 数组: %s" % path)
    return man


def manifest_tables(snapdir):
    """返回 {fq: {columns:[name...], file, sha256, rows, raw_columns}}。"""
    man = load_manifest(snapdir)
    out = {}
    for entry in man["tables"]:
        raw_fq = entry.get("table")
        if not raw_fq or "." not in raw_fq:
            raise InputError("manifest 表项缺 table 或格式非法: %r" % (entry,))
        # 两侧克隆库前缀可能不同（mir2_test_db_oracle.* / mir2_test_db_rust.*），
        # 归一后按同一张登记表对齐。
        fq = canonical_table_name(raw_fq)
        if fq in out:
            raise InputError("快照里多张表归一到同一登记名 %s: %r 与 %r" %
                             (fq, out[fq]["raw_table"], raw_fq))
        if "columns" in entry:
            cols = [c.get("name") if isinstance(c, dict) else c
                    for c in entry["columns"]]
        elif fq in TABLE_SPECS:
            cols = list(TABLE_SPECS[fq]["columns"])
        else:
            cols = []
        file_name = entry.get("file") or (raw_fq + ".tsv")
        path = os.path.join(snapdir, file_name)
        if not os.path.exists(path):
            raise InputError("快照缺 tsv: %s" % path)
        out[fq] = {"columns": cols, "file": path, "sha256": entry.get("sha256"),
                   "rows": entry.get("rows"), "raw_columns": entry.get("columns"),
                   "raw_table": raw_fq}
    return out


def read_rows(entry, expected_cols):
    """读 tsv。返回 (rows, structural_errors)。每行按 TAB 切列。"""
    rows = []
    errors = []
    with open(entry["file"], "rb") as f:
        raw = f.read()
    # 与 dbsnap 一致：一条逻辑行一个 LF；不处理值内 LF（快照结构签名由 dbsnap 保证）。
    lines = raw.split(b"\n")
    if lines and lines[-1] == b"":
        lines.pop()
    ncols = len(expected_cols)
    for i, line in enumerate(lines):
        fields = line.split(b"\t")
        if len(fields) != ncols:
            errors.append({"kind": "column_count", "table": None, "row": i + 1,
                           "expected": ncols, "actual": len(fields)})
            continue
        rows.append([x.decode("latin-1") for x in fields])
    return rows, errors


def row_key(row, col_index, pk):
    return tuple(row[col_index[c]] for c in pk)


def key_json(key):
    return list(key)


def normalized_row(row, col_index, compare_cols, time_cols, time_mode, null_token):
    vals = []
    for c in compare_cols:
        v = row[col_index[c]]
        if c in time_cols:
            if is_null(v, null_token):
                vals.append(("<NULL>",))
            elif time_mode == "normalized":
                vals.append(("t", norm_time(v)))
            elif time_mode == "presence":
                vals.append(("present",))
            else:
                vals.append(("t", v))
        else:
            vals.append(("<NULL>",) if is_null(v, null_token) else ("v", v))
    return tuple(vals)


def compare_tables(fq, a_entry, b_entry, opts, records):
    """比较一张表；把差异 append 到 records，返回该表差异数。"""
    spec = TABLE_SPECS.get(fq)
    if spec is None and not opts["allow_unknown_table"]:
        records.append({"table": fq, "kind": "unknown_table",
                        "detail": "未登记表；先登记主键/时间列，或用 --allow-unknown-table + --key-mode multiset"})
        return 1
    n_before = len(records)

    # tamper 检测：以文件实况为准，不信 manifest 里的旧 hash。
    for side, entry in (("a", a_entry), ("b", b_entry)):
        actual = sha256_file(entry["file"])
        if entry["sha256"] and actual != entry["sha256"]:
            records.append({"table": fq, "kind": "tamper", "side": side,
                            "expected_sha256": entry["sha256"],
                            "actual_sha256": actual,
                            "detail": "快照被事后改动（manifest 与 tsv 不符）"})
    if len(records) > n_before:
        return len(records) - n_before

    a_cols, b_cols = a_entry["columns"], b_entry["columns"]
    if not a_cols or not b_cols:
        records.append({"table": fq, "kind": "schema", "detail": "manifest 缺 columns"})
        return 1
    if a_cols != b_cols:
        records.append({"table": fq, "kind": "schema", "detail": "两侧列名/列序不同",
                        "a_columns": a_cols, "b_columns": b_cols})
        return 1
    if spec is not None and a_cols != spec["columns"]:
        records.append({"table": fq, "kind": "schema",
                        "detail": "快照列与登记表结构不符（表结构漂移或登记过期）",
                        "manifest_columns": a_cols, "registered_columns": spec["columns"]})
        return 1

    cols = a_cols
    idx = {c: i for i, c in enumerate(cols)}
    pk = (spec or {}).get("pk", [])
    time_cols = set((spec or {}).get("time_columns", []))
    # ignore 是全局参数；只对本表实际存在的列生效（不同表列集不同）。
    ignore = set(opts["ignore_columns"]) & set(cols)

    # ignore 白名单校验：只允许已登记易变列；否则拒绝（不悄悄不比）。
    if spec is not None:
        # 已登记时间列允许显式忽略（--ignore-time-columns 会展开到这些列）。
        allowed_ignore = set(spec["volatile"]) | set(spec["time_columns"]) | set(pk)
        bad = sorted(ignore - allowed_ignore)
        if bad:
            raise InputError("表 %s 的 --ignore-columns %s 不是已登记易变列/时间列/主键；"
                             "行为列不许静默忽略（先登记理由）" % (fq, bad))
    compare_cols = [c for c in cols if c not in ignore]

    a_rows, a_err = read_rows(a_entry, cols)
    b_rows, b_err = read_rows(b_entry, cols)
    for e in a_err + b_err:
        e["table"] = fq
        records.append(e)
    if a_err or b_err:
        return len(records) - n_before
    # 与 dbsnap 的 S2 同口径：manifest.rows 是逻辑行数；tsv 行数不符 ⇒ 坏快照。
    for side, entry, n_lines in (("a", a_entry, len(a_rows)),
                                 ("b", b_entry, len(b_rows))):
        if entry.get("rows") is not None and int(entry["rows"]) != n_lines:
            records.append({"table": fq, "kind": "row_count", "side": side,
                            "manifest_rows": entry["rows"], "tsv_rows": n_lines,
                            "detail": "manifest.rows 与 tsv 逻辑行数不符（坏快照/值内换行）"})
    if len(records) > n_before:
        return len(records) - n_before

    if opts["key_mode"] == "pk":
        if not pk:
            records.append({"table": fq, "kind": "schema",
                            "detail": "未登记主键，不能用 --key-mode pk"})
            return 1
        ma, dup_a = index_by_key(a_rows, idx, pk)
        mb, dup_b = index_by_key(b_rows, idx, pk)
        for dup, side in ((dup_a, "a"), (dup_b, "b")):
            for key, count in dup:
                records.append({"table": fq, "kind": "duplicate_key", "side": side,
                                "key": key_json(key), "count": count,
                                "detail": "同侧重复主键，无法唯一对齐"})
        if dup_a or dup_b:
            return len(records) - n_before
        for key in sorted(set(ma) | set(mb)):
            ra, rb = ma.get(key), mb.get(key)
            if rb is None:
                records.append({"table": fq, "kind": "row_only_a", "key": key_json(key),
                                "a": dict(zip(compare_cols, [ra[idx[c]] for c in compare_cols]))})
                continue
            if ra is None:
                records.append({"table": fq, "kind": "row_only_b", "key": key_json(key),
                                "b": dict(zip(compare_cols, [rb[idx[c]] for c in compare_cols]))})
                continue
            for c in compare_cols:
                va, vb = ra[idx[c]], rb[idx[c]]
                if not values_equal(va, vb, is_time=(c in time_cols),
                                    time_mode=opts["time_mode"],
                                    null_token=opts["null_token"],
                                    stats=opts.get("stats")):
                    records.append({"table": fq, "kind": "value", "key": key_json(key),
                                    "column": c, "a": va, "b": vb})
    else:  # multiset
        from collections import Counter
        sig_a = Counter(normalized_row(r, idx, compare_cols, time_cols,
                                       opts["time_mode"], opts["null_token"]) for r in a_rows)
        sig_b = Counter(normalized_row(r, idx, compare_cols, time_cols,
                                       opts["time_mode"], opts["null_token"]) for r in b_rows)
        for sig in sorted(set(sig_a) | set(sig_b), key=repr):
            ca, cb = sig_a.get(sig, 0), sig_b.get(sig, 0)
            if ca == cb:
                continue
            kind = "row_only_a" if cb == 0 else ("row_only_b" if ca == 0 else "row_count")
            records.append({"table": fq, "kind": kind, "signature": repr(sig),
                            "a_count": ca, "b_count": cb})
    return len(records) - n_before


def index_by_key(rows, idx, pk):
    out, dup = {}, []
    counts = {}
    for r in rows:
        k = row_key(r, idx, pk)
        counts[k] = counts.get(k, 0) + 1
        out.setdefault(k, r)
    for k, n in counts.items():
        if n > 1:
            dup.append((k, n))
    return out, dup


def _entry_structure_errors(fq, label, entry, spec, records):
    """before/after 共用的一侧结构校验：申报 hash、columns 名单/列序、登记结构。

    返回 True=通过；任一项不符都追加记录并返回 False（fail-closed）。
    """
    ok = True
    if entry.get("sha256"):
        actual = sha256_file(entry["file"])
        if actual != entry["sha256"]:
            records.append({"table": fq, "kind": "tamper", "side": label,
                            "expected_sha256": entry["sha256"],
                            "actual_sha256": actual,
                            "detail": "快照被事后改动（manifest 与 tsv 不符）"})
            ok = False
    cols = entry.get("columns") or []
    if not cols:
        records.append({"table": fq, "kind": "schema", "side": label,
                        "detail": "manifest 缺 columns"})
        return False
    if spec is not None and cols != spec["columns"]:
        records.append({"table": fq, "kind": "schema", "side": label,
                        "detail": "快照列与登记表结构不符（表结构漂移或登记过期）",
                        "manifest_columns": cols,
                        "registered_columns": spec["columns"]})
        ok = False
    return ok


def compare_changed(fq, base_a, after_a, base_b, after_b, opts, records):
    """C3 changed 标志：按主键对齐每侧的 before/after。

    before 与 after 用**同一套**结构校验（hash / columns 名单与列序 / rows /
    重复主键），不只信 after；before 被事后篡改或列序漂移必须红。
    """
    spec = TABLE_SPECS.get(fq)
    if spec is None:
        return 0
    n_before = len(records)
    sides = (("a_before", base_a), ("a_after", after_a),
             ("b_before", base_b), ("b_after", after_b))
    ok = True
    for label, entry in sides:
        if not _entry_structure_errors(fq, label, entry, spec, records):
            ok = False
    if not ok:
        return len(records) - n_before

    cols = after_a["columns"]
    for label, entry in sides:
        if entry["columns"] != cols:
            records.append({"table": fq, "kind": "schema", "side": label,
                            "detail": "before/after 列名/列序不同",
                            "columns": entry["columns"], "expected": cols})
    if len(records) > n_before:
        return len(records) - n_before

    idx = {c: i for i, c in enumerate(cols)}
    pk = spec["pk"]
    time_cols = set(spec["time_columns"])
    ignore = set(opts["ignore_columns"]) & set(cols)
    compare_cols = [c for c in cols if c not in ignore]

    def load(label, entry):
        rows, errs = read_rows(entry, cols)
        for e in errs:
            e["table"] = fq
            e["side"] = label
            records.append(e)
        if errs:
            return None
        if entry.get("rows") is not None and int(entry["rows"]) != len(rows):
            records.append({"table": fq, "kind": "row_count", "side": label,
                            "manifest_rows": entry["rows"], "tsv_rows": len(rows),
                            "detail": "manifest.rows 与 tsv 逻辑行数不符（坏快照）"})
            return None
        mapping, dup = index_by_key(rows, idx, pk)
        for key, count in dup:
            records.append({"table": fq, "kind": "duplicate_key", "side": label,
                            "key": key_json(key), "count": count,
                            "detail": "同侧重复主键，无法唯一对齐"})
        return None if dup else mapping

    loaded = [(label, load(label, entry)) for label, entry in sides]
    if any(m is None for _, m in loaded):
        return len(records) - n_before
    ba, aa, bb, ab = (m for _, m in loaded)

    def side_changed(rows_before, rows_after):
        out = {}
        for k in set(rows_before) | set(rows_after):
            if k not in rows_before or k not in rows_after:
                out[k] = True
                continue
            ra, rb = rows_before[k], rows_after[k]
            changed = any(not values_equal(ra[idx[c]], rb[idx[c]],
                                           is_time=(c in time_cols),
                                           time_mode=opts["time_mode"],
                                           null_token=opts["null_token"],
                                           stats=opts.get("stats"))
                          for c in compare_cols)
            out[k] = changed
            if not changed:
                continue
            for c in sorted(time_cols):
                if c in ignore:
                    continue
                va, vb = ra[idx[c]], rb[idx[c]]
                if is_null(va, opts["null_token"]) or is_null(vb, opts["null_token"]):
                    continue
                ta, tb = parse_time_value(va), parse_time_value(vb)
                # 设计 C3：changed 行的 after 必须**严格晚于** before
                # （DATETIME 与 unix 秒各自按同一量纲比；格式不明则跳过）。
                if ta is None or tb is None or ta[0] != tb[0]:
                    continue
                # 违反「严格晚于」的条件是 after <= before（相等也不行）。
                if tb[1] <= ta[1]:
                    records.append({"table": fq, "kind": "time_not_later",
                                    "key": key_json(k), "column": c,
                                    "before": va, "after": vb,
                                    "detail": "changed 行的 after 未严格晚于 before，"
                                              "不在因果窗内"})
        return out

    ca = side_changed(ba, aa)
    cb = side_changed(bb, ab)
    for k in sorted(set(ca) | set(cb)):
        xa, xb = ca.get(k), cb.get(k)
        if xa != xb:
            records.append({"table": fq, "kind": "changed_mismatch",
                            "key": key_json(k), "a_changed": xa, "b_changed": xb})
    return len(records) - n_before


def run_pair(a_dir, b_dir, *, tables=None, require_tables=None, key_mode="pk",
             time_mode="exact", null_token=NULL_TOKEN_DEFAULT, ignore_columns=(),
             allow_unknown_table=False, baseline_a=None, baseline_b=None,
             quiet=False):
    """核心入口（不 sys.exit）。返回 (rc, records, summary)。"""
    opts = {"key_mode": key_mode, "time_mode": time_mode,
            "null_token": null_token, "ignore_columns": list(ignore_columns),
            "allow_unknown_table": allow_unknown_table, "stats": {}}
    if time_mode not in ("exact", "presence", "normalized"):
        raise InputError("未知 --time-mode: %s" % time_mode)
    if key_mode not in ("pk", "multiset"):
        raise InputError("未知 --key-mode: %s" % key_mode)
    if bool(baseline_a) != bool(baseline_b):
        raise InputError("--baseline-a 与 --baseline-b 必须同时给")

    A = manifest_tables(a_dir)
    B = manifest_tables(b_dir)

    # 调用方可以给登记名或带实际库前缀的表名（E1 克隆）；统一归一到登记名。
    tables = [canonical_table_name(t) for t in tables] if tables else None
    require_tables = [canonical_table_name(t) for t in (require_tables or [])]
    require_set = set(require_tables)

    if tables:
        names = list(tables)
    else:
        names = [t for t in TABLE_SPECS if t in A or t in B]
        names += sorted((set(A) | set(B)) - set(names))
    # --require-tables 不只是「存在性检查」：它必须进入比较集，
    # 否则收窄 --tables 时被点名要求的表会被静默跳过。
    seen = set(names)
    for t in require_tables:
        if t not in seen:
            names.append(t)
            seen.add(t)

    records = []
    BA = manifest_tables(baseline_a) if baseline_a else None
    BB = manifest_tables(baseline_b) if baseline_b else None
    per_table = {}
    for fq in names:
        if fq not in A or fq not in B:
            both_missing = fq not in A and fq not in B
            rec = {"table": fq, "kind": "missing_table",
                   "detail": ("两侧 after 快照都没有该表" if both_missing
                              else "仅一侧 after 快照有该表")}
            if fq in require_set:
                rec["required"] = True
            records.append(rec)
            per_table[fq] = 1
            continue
        n = compare_tables(fq, A[fq], B[fq], opts, records)
        per_table[fq] = n
        if BA is not None and BB is not None:
            if fq not in BA or fq not in BB:
                records.append({"table": fq, "kind": "missing_table",
                                "detail": "changed 模式要求两侧都有 before 基线快照"})
                per_table[fq] = n + 1
                continue
            n += compare_changed(fq, BA[fq], A[fq], BB[fq], B[fq], opts, records)
            per_table[fq] = n

    counts = {}
    for r in records:
        counts[r["kind"]] = counts.get(r["kind"], 0) + 1
    red = len(records) > 0
    unknown_tables = sorted(t for t in names if t not in TABLE_SPECS)
    unrecognized = opts["stats"].get("unrecognized_time_values", 0)
    summary = {
        "tool": "dbsnap_pair",
        "a": os.path.abspath(a_dir), "b": os.path.abspath(b_dir),
        "options": {"key_mode": key_mode, "time_mode": time_mode,
                    "null_token": null_token,
                    "ignore_columns": sorted(ignore_columns),
                    "allow_unknown_table": bool(allow_unknown_table),
                    "baseline_a": os.path.abspath(baseline_a) if baseline_a else None,
                    "baseline_b": os.path.abspath(baseline_b) if baseline_b else None},
        "tables": per_table,
        "diff_counts": counts,
        "diff_records": len(records),
        "red": red,
        # 低层行为显式登记：未登记表（仅在 --allow-unknown-table 下放行）
        # 与 normalized 下无法识别、按 exact 比较的时间值数量。
        "unknown_tables": unknown_tables,
        "unknown_table_policy": ("allowed（仅 multiset 可比的表见 diff）"
                                 if allow_unknown_table and unknown_tables else ""),
        "unrecognized_time_values": unrecognized,
    }
    if not quiet:
        for fq in names:
            n = per_table.get(fq, 0)
            print("%-8s %s (%d)" % ("DIFF" if n else "OK", fq, n))
        if unrecognized:
            print("NOTE: normalized 下有 %d 个时间值无法识别格式，已按 exact 比较"
                  % unrecognized)
        if unknown_tables and allow_unknown_table:
            print("NOTE: --allow-unknown-table 放行未登记表 %s（摘要已标注）"
                  % ", ".join(unknown_tables))
        if red:
            print("RED: %d 差异记录（%s）" % (len(records), counts))
        else:
            print("GREEN: %d 张表逐列一致" % len(names))
    return (1 if red else 0), records, summary


def write_jsonl(path, records):
    """path: None/""=不输出；"-"=stdout；否则写文件。JSON 一律 ASCII 转义（字节安全）。"""
    if path in (None, ""):
        return
    if path == "-":
        for r in records:
            print(json.dumps(r, ensure_ascii=True, sort_keys=True))
        return
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        for r in records:
            f.write(json.dumps(r, ensure_ascii=True, sort_keys=True) + "\n")

# ---------------------------------------------------------------------------
# selftest：全离线合成快照
# ---------------------------------------------------------------------------
def _prefixed(rows, prefix):
    """把 {mir2_db.characters: rows} 改写成 {<prefix>.characters: rows}（模拟克隆库）。"""
    out = {}
    for fq, vals in rows.items():
        out["%s.%s" % (prefix, fq.rpartition(".")[2])] = vals
    return out


def _synth_row(fq, overrides=None):
    spec = TABLE_SPECS[canonical_table_name(fq)]
    row = []
    for c in spec["columns"]:
        if c == "ChrName":
            row.append("Hero")
        elif c in ("Account", "LoginID"):
            row.append("acct1")
        elif c in spec["time_columns"]:
            row.append("2026-10-20 12:00:00.000000")
        else:
            row.append("0")
    idx = {c: i for i, c in enumerate(spec["columns"])}
    for c, v in (overrides or {}).items():
        row[idx[c]] = v
    return row


def write_snapshot(d, tables_rows):
    """tables_rows: {fq: [[str,...], ...]}，列按 TABLE_SPECS 登记序。

    fq 可带任意库前缀（模拟 E1 的 mir2_test_db_oracle/rust 克隆）；列结构按归一后
    的登记表取。
    """
    os.makedirs(d, exist_ok=True)
    man = {"tool": "dbsnap", "tables": []}
    for fq, rows in tables_rows.items():
        spec = TABLE_SPECS[canonical_table_name(fq)]
        lines = [("\t".join(r) + "\n").encode("latin-1") for r in rows]
        name = fq + ".tsv"
        path = os.path.join(d, name)
        with open(path, "wb") as f:
            f.write(b"".join(lines))
        man["tables"].append({
            "table": fq, "rows": len(rows), "sha256": sha256_file(path), "file": name,
            "columns": [{"name": c, "type": "text"} for c in spec["columns"]],
        })
    with open(os.path.join(d, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(man, f, ensure_ascii=False, indent=2)


def _find_scoped_diff():
    here = os.path.dirname(os.path.abspath(__file__))
    cand = os.path.normpath(os.path.join(
        here, "..", "..", "mir2-rs", "tests", "parity", "m3", "scoped_diff.py"))
    if os.path.exists(cand):
        return cand
    print("NOTE: check-spec 未找到 %s；表登记漂移对账被跳过（本工具仍可独立运行）。"
          "若该文件被移动，请同步更新 dbsnap_pair._find_scoped_diff 的路径。" % cand,
          file=sys.stderr)
    return None


def check_spec_drift():
    """与 scoped_diff.py 的 TABLE_COLUMNS 对账；返回问题列表（空=一致）。"""
    path = _find_scoped_diff()
    if not path:
        return ["SKIP: 未找到 scoped_diff.py（本工具可独立运行）"]
    import importlib.util
    spec = importlib.util.spec_from_file_location("_scoped_diff_for_drift", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    problems = []
    theirs = set(mod.TABLE_COLUMNS)
    ours = set(TABLE_SPECS)
    for t in sorted(theirs - ours):
        problems.append("scoped_diff 有而本工具未登记: %s" % t)
    for t in sorted(ours - theirs):
        problems.append("本工具有而 scoped_diff 未登记: %s" % t)
    for t in sorted(theirs & ours):
        if list(mod.TABLE_COLUMNS[t]) != TABLE_SPECS[t]["columns"]:
            problems.append("列表不一致: %s" % t)
    return problems


def _run_rc(a, b, **kw):
    try:
        rc, recs, _ = run_pair(a, b, quiet=True, **kw)
        return rc, recs
    except InputError as e:
        return 2, [{"kind": "input_error", "detail": str(e)}]


def cmd_selftest(args):
    import shutil
    import tempfile
    tmp = tempfile.mkdtemp(prefix="dbsnap-pair-selftest-")
    passed = failed = 0
    try:
        base = os.path.join(tmp, "base")
        rows = {
            "mir2_db.characters": [_synth_row("mir2_db.characters", {"Id": "1"})],
            "mir2_db.characters_indexes": [_synth_row("mir2_db.characters_indexes",
                                                      {"Id": "1", "SelectID": "0",
                                                       "IsDeleted": "0"})],
            "mir2_db.characters_item": [
                _synth_row("mir2_db.characters_item", {"Id": "1", "PlayerId": "1",
                                                       "Position": "0"}),
                _synth_row("mir2_db.characters_item", {"Id": "2", "PlayerId": "1",
                                                       "Position": "1"}),
            ],
            "mir2_db.characters_ablity": [_synth_row("mir2_db.characters_ablity",
                                                     {"PlayerId": "1"})],
        }
        write_snapshot(base, rows)

        def case(name, mutate, want_rc, want_kind=None, **kw):
            nonlocal passed, failed
            other = os.path.join(tmp, "case-" + re.sub(r"\W+", "_", name))
            if os.path.exists(other):
                shutil.rmtree(other)
            write_snapshot(other, rows)
            if mutate:
                mutate(other)
            rc, recs = _run_rc(base, other, **kw)
            kinds = {r.get("kind") for r in recs}
            ok = rc == want_rc and (want_kind is None or want_kind in kinds)
            if ok:
                passed += 1
                print("PASS %-42s rc=%d kinds=%s" % (name, rc, sorted(kinds)))
            else:
                failed += 1
                print("FAIL %-42s rc=%d (want %d) kinds=%s recs=%s" %
                      (name, rc, want_rc, sorted(kinds), recs[:3]))

        def change_cell(table, col, value):
            def f(d):
                p = os.path.join(d, table + ".tsv")
                spec = TABLE_SPECS[table]
                i = spec["columns"].index(col)
                lines = open(p, encoding="utf-8").read().splitlines()
                fields = lines[0].split("\t")
                fields[i] = value
                lines[0] = "\t".join(fields)
                open(p, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
                _resign_dir(d)
            return f

        def _resign(d):
            _resign_dir(d)

        case("identical", None, 0)
        case("value_changed", change_cell("mir2_db.characters", "Level", "9"), 1, "value")
        case("row_added", lambda d: _add_row(d, "mir2_db.characters_item",
                                             _synth_row("mir2_db.characters_item",
                                                        {"Id": "3", "PlayerId": "1",
                                                         "Position": "2"})), 1, "row_only_b")
        case("row_removed", lambda d: _drop_row(d, "mir2_db.characters_item", 1),
             1, "row_only_a")
        case("null_vs_zero", change_cell("mir2_db.characters", "GamePoint", "NULL"), 1, "value")

        def time_reformat(d):
            # 同一时刻的不同书写：T 分隔、无小数位；normalized 必须判绿，exact 必须红。
            change_cell("mir2_db.characters", "LASTUPDATE",
                        "2026-10-20T12:00:00")(d)
        case("time_exact_red", time_reformat, 1, "value")
        case("time_presence_green", time_reformat, 0, time_mode="presence")
        case("time_normalized_green", time_reformat, 0, time_mode="normalized")

        def schema_drift(d):
            man_path = os.path.join(d, "manifest.json")
            man = json.load(open(man_path, encoding="utf-8"))
            for t in man["tables"]:
                if t["table"] == "mir2_db.characters":
                    t["columns"] = list(reversed(t["columns"]))
            json.dump(man, open(man_path, "w", encoding="utf-8", newline="\n"),
                      ensure_ascii=False, indent=2)
        case("schema_drift_red", schema_drift, 1, "schema")

        def tamper(d):
            p = os.path.join(d, "mir2_db.characters.tsv")
            with open(p, "ab") as f:
                f.write(b"\t9\n")
        case("tamper_red", tamper, 1, "tamper")

        def bad_rows(d):
            man_path = os.path.join(d, "manifest.json")
            man = json.load(open(man_path, encoding="utf-8"))
            for t in man["tables"]:
                if t["table"] == "mir2_db.characters_item":
                    t["rows"] = int(t["rows"]) + 1
            json.dump(man, open(man_path, "w", encoding="utf-8", newline="\n"),
                      ensure_ascii=False, indent=2)
        case("row_count_mismatch_red", bad_rows, 1, "row_count")

        def dup_key(d):
            _add_row(d, "mir2_db.characters_item",
                     _synth_row("mir2_db.characters_item",
                                {"Id": "1", "PlayerId": "1", "Position": "7"}))
        case("duplicate_key_red", dup_key, 1, "duplicate_key")
        case("ignore_behavior_column_refused",
             change_cell("mir2_db.characters", "Level", "9"), 2, "input_error",
             ignore_columns=["Level"])
        case("ignore_id_multiset_green",
             _set_id("mir2_db.characters", "99"), 0,
             key_mode="multiset", ignore_columns=["Id"])

        # changed 标志：A 侧改了 Level，B 侧没改 -> 必须红；两侧同改 -> 绿。
        after_a = os.path.join(tmp, "after-a")
        after_b = os.path.join(tmp, "after-b")
        write_snapshot(after_a, rows)
        write_snapshot(after_b, rows)
        change_cell("mir2_db.characters", "Level", "9")(after_a)
        rc, recs = _run_rc(base, base, baseline_a=base, baseline_b=base)
        case_ok = (rc == 0)
        passed += 1 if case_ok else 0
        failed += 0 if case_ok else 1
        print("%s %-42s rc=%d" % ("PASS" if case_ok else "FAIL",
                                  "changed_all_false_green", rc))
        rc, recs = _run_rc(after_a, after_b, baseline_a=base, baseline_b=base)
        kinds = {r.get("kind") for r in recs}
        case_ok = (rc == 1 and "changed_mismatch" in kinds)
        passed += 1 if case_ok else 0
        failed += 0 if case_ok else 1
        print("%s %-42s rc=%d kinds=%s" % ("PASS" if case_ok else "FAIL",
                                           "changed_mismatch_red", rc, sorted(kinds)))

        # 两侧仅库前缀不同（E1 克隆 mir2_test_db_oracle/rust）也必须能对齐比较。
        prefix_a = os.path.join(tmp, "prefix-a")
        prefix_b = os.path.join(tmp, "prefix-b")
        write_snapshot(prefix_a, _prefixed(rows, "mir2_test_db_oracle"))
        write_snapshot(prefix_b, _prefixed(rows, "mir2_test_db_rust"))
        rc, recs = _run_rc(prefix_a, prefix_b)
        case_ok = (rc == 0 and not recs)
        passed += 1 if case_ok else 0
        failed += 0 if case_ok else 1
        print("%s %-42s rc=%d recs=%s" % ("PASS" if case_ok else "FAIL",
                                          "prefix_only_diff_green", rc, recs[:2]))

        # require-tables 必须进入比较集（收窄 --tables 时也不能被跳过）。
        req_changed = os.path.join(tmp, "require-items-changed")
        write_snapshot(req_changed, rows)
        change_cell("mir2_db.characters_item", "Position", "9")(req_changed)
        rc, recs = _run_rc(base, req_changed, tables=["mir2_db.characters"],
                           require_tables=["mir2_db.characters_item"])
        hit = any(r.get("table") == "mir2_db.characters_item" and r.get("column") == "Position"
                  for r in recs)
        case_ok = (rc == 1 and hit)
        passed += 1 if case_ok else 0
        failed += 0 if case_ok else 1
        print("%s %-42s rc=%d kinds=%s" % ("PASS" if case_ok else "FAIL",
                                           "require_table_compared_red", rc,
                                           sorted({r.get("kind") for r in recs})))

        # C3 时间列：changed 行的 after 必须**严格晚于** before（相等也红）。
        def changed_time_case(label, after_overrides, want_rc):
            nonlocal passed, failed
            d = os.path.join(tmp, label)
            if os.path.exists(d):
                shutil.rmtree(d)
            write_snapshot(d, rows)
            change_cell("mir2_db.characters", "Level", "9")(d)
            for col, value in after_overrides.items():
                change_cell("mir2_db.characters", col, value)(d)
            rc, recs = _run_rc(d, d, baseline_a=base, baseline_b=base)
            kinds = {r.get("kind") for r in recs}
            ok = rc == want_rc and (want_rc == 0 or "time_not_later" in kinds)
            passed += 1 if ok else 0
            failed += 0 if ok else 1
            print("%s %-42s rc=%d kinds=%s" % ("PASS" if ok else "FAIL",
                                               label, rc, sorted(kinds)))

        # CREATEDATE/LASTUPDATE 都推进 1 秒 => 绿；相等/回退 => time_not_later 红。
        changed_time_case("changed_time_later_green",
                          {"CREATEDATE": "2026-10-20 12:00:01.000000",
                           "LASTUPDATE": "2026-10-20 12:00:01.000000"}, 0)
        changed_time_case("changed_time_equal_red", {}, 1)
        changed_time_case("changed_time_earlier_red",
                          {"CREATEDATE": "2026-10-20 11:59:59.000000",
                           "LASTUPDATE": "2026-10-20 11:59:59.000000"}, 1)

        # unix 秒时间列（account.CreateTime/LastLoginTime）按整数比较。
        unix_before = {"mir2_account.account": [_synth_row(
            "mir2_account.account",
            {"Id": "1", "CreateTime": "1700000000", "LastLoginTime": "1700000000"})]}
        unix_base = os.path.join(tmp, "unix-base")
        write_snapshot(unix_base, unix_before)

        def unix_case(label, after_ts, want_rc, extra=None):
            nonlocal passed, failed
            d = os.path.join(tmp, label)
            after = {"mir2_account.account": [_synth_row(
                "mir2_account.account",
                {"Id": "1", "CreateTime": after_ts, "LastLoginTime": after_ts})]}
            if extra:
                after["mir2_account.account"][0] = _synth_row(
                    "mir2_account.account",
                    dict({"Id": "1", "CreateTime": after_ts,
                          "LastLoginTime": after_ts}, **extra))
            write_snapshot(d, after)
            rc, recs = _run_rc(d, d, baseline_a=unix_base, baseline_b=unix_base,
                               tables=["mir2_account.account"],
                               ignore_columns=["ValidFrom", "ValidUntil",
                                               "StopUntil", "ModifyTime"])
            kinds = {r.get("kind") for r in recs}
            ok = rc == want_rc and (want_rc == 0 or "time_not_later" in kinds)
            passed += 1 if ok else 0
            failed += 0 if ok else 1
            print("%s %-42s rc=%d kinds=%s" % ("PASS" if ok else "FAIL",
                                               label, rc, sorted(kinds)))

        unix_case("changed_unix_time_later_green", "1700000001", 0)
        unix_case("changed_unix_time_equal_red", "1700000000", 1,
                  extra={"PassWord": "x"})
        unix_case("changed_unix_time_earlier_red", "1699999999", 1)

        # changed 模式的 before 也必须做同一套校验：篡改/列序反序都必红。
        before_tamper = os.path.join(tmp, "before-tamper")
        shutil.copytree(base, before_tamper)
        with open(os.path.join(before_tamper, "mir2_db.characters.tsv"), "ab") as f:
            f.write(b"\t9\n")
        rc, recs = _run_rc(base, base, baseline_a=before_tamper, baseline_b=base)
        hit = any(r.get("kind") == "tamper" and r.get("side") == "a_before"
                  for r in recs)
        case_ok = (rc == 1 and hit)
        passed += 1 if case_ok else 0
        failed += 0 if case_ok else 1
        print("%s %-42s rc=%d kinds=%s" % ("PASS" if case_ok else "FAIL",
                                           "before_tamper_red", rc,
                                           sorted({r.get("kind") for r in recs})))

        before_rev = os.path.join(tmp, "before-columns-reversed")
        shutil.copytree(base, before_rev)
        man_path = os.path.join(before_rev, "manifest.json")
        man = json.load(open(man_path, encoding="utf-8"))
        for t in man["tables"]:
            if t["table"] == "mir2_db.characters":
                t["columns"] = list(reversed(t["columns"]))
        json.dump(man, open(man_path, "w", encoding="utf-8", newline="\n"),
                  ensure_ascii=False, indent=2)
        rc, recs = _run_rc(base, base, baseline_a=before_rev, baseline_b=base)
        hit = any(r.get("kind") == "schema" and r.get("side") == "a_before"
                  for r in recs)
        case_ok = (rc == 1 and hit)
        passed += 1 if case_ok else 0
        failed += 0 if case_ok else 1
        print("%s %-42s rc=%d kinds=%s" % ("PASS" if case_ok else "FAIL",
                                           "before_columns_reversed_red", rc,
                                           sorted({r.get("kind") for r in recs})))

        # require-tables 缺失必须红。
        rc, recs = _run_rc(base, base, require_tables=["mir2_db.characters_magic"])
        kinds = {r.get("kind") for r in recs}
        case_ok = (rc == 1 and "missing_table" in kinds)
        passed += 1 if case_ok else 0
        failed += 0 if case_ok else 1
        print("%s %-42s rc=%d kinds=%s" % ("PASS" if case_ok else "FAIL",
                                           "missing_required_table_red", rc, sorted(kinds)))

        # 低层行为：normalized 无法识别的时间值要计数；--allow-unknown-table
        # 必须在摘要里显式标注。
        weird = os.path.join(tmp, "unrecognized-time")
        write_snapshot(weird, rows)
        change_cell("mir2_db.characters", "LASTUPDATE", "not-a-time")(weird)
        rc, recs, summary = run_pair(base, weird, quiet=True, time_mode="normalized")
        ok = rc == 1 and summary.get("unrecognized_time_values", 0) >= 1
        passed += 1 if ok else 0
        failed += 0 if ok else 1
        print("%s %-42s count=%s" % ("PASS" if ok else "FAIL",
                                     "normalized_unrecognized_counted",
                                     summary.get("unrecognized_time_values")))

        unknown_dir = os.path.join(tmp, "unknown-table")
        write_snapshot(unknown_dir, rows)
        extra_name = "custom.extra.tsv"
        extra_path = os.path.join(unknown_dir, extra_name)
        with open(extra_path, "wb") as f:
            f.write(b"1" + bytes([10]))
        man_path2 = os.path.join(unknown_dir, "manifest.json")
        man2 = json.load(open(man_path2, encoding="utf-8"))
        man2["tables"].append({"table": "custom.extra", "rows": 1,
                               "sha256": sha256_file(extra_path), "file": extra_name,
                               "columns": [{"name": "Id", "type": "text"}]})
        json.dump(man2, open(man_path2, "w", encoding="utf-8", newline="\n"),
                  ensure_ascii=False, indent=2)
        rc, recs, summary = run_pair(unknown_dir, unknown_dir, quiet=True,
                                     allow_unknown_table=True, key_mode="multiset")
        ok = (rc == 0 and summary.get("unknown_tables") == ["custom.extra"]
              and "allowed" in summary.get("unknown_table_policy", ""))
        passed += 1 if ok else 0
        failed += 0 if ok else 1
        print("%s %-42s rc=%d unknown=%s" % ("PASS" if ok else "FAIL",
                                             "allow_unknown_table_annotated", rc,
                                             summary.get("unknown_tables")))

        drift = check_spec_drift()
        if drift and not drift[0].startswith("SKIP"):
            failed += 1
            print("FAIL spec_drift %s" % drift)
        else:
            passed += 1
            print("PASS %-42s %s" % ("spec_drift", drift[0] if drift else "ok"))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    total = passed + failed
    if failed:
        print("RED: dbsnap_pair selftest %d/%d" % (passed, total))
        return 1
    print("GREEN: dbsnap_pair selftest %d/%d" % (passed, total))
    return 0


def _add_row(d, table, row):
    p = os.path.join(d, table + ".tsv")
    with open(p, "a", encoding="utf-8", newline="\n") as f:
        f.write("\t".join(row) + "\n")
    _resign_dir(d)


def _drop_row(d, table, index):
    p = os.path.join(d, table + ".tsv")
    lines = open(p, encoding="utf-8").read().splitlines()
    del lines[index]
    open(p, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    _resign_dir(d)


def _set_id(table, value):
    def f(d):
        i = TABLE_SPECS[table]["columns"].index("Id")
        p = os.path.join(d, table + ".tsv")
        lines = open(p, encoding="utf-8").read().splitlines()
        fields = lines[0].split("\t")
        fields[i] = value
        lines[0] = "\t".join(fields)
        open(p, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
        _resign_dir(d)
    return f


def _resign_dir(d):
    man_path = os.path.join(d, "manifest.json")
    man = json.load(open(man_path, encoding="utf-8"))
    for t in man["tables"]:
        path = os.path.join(d, t["file"])
        t["sha256"] = sha256_file(path)
        t["rows"] = len(open(path, encoding="utf-8").read().splitlines())
    json.dump(man, open(man_path, "w", encoding="utf-8", newline="\n"),
              ensure_ascii=False, indent=2)


def build_parser():
    p = argparse.ArgumentParser(description="dbsnap 快照逐表逐行逐列 diff（LB-10 证据工具）")
    sub = p.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("diff", help="比较两份快照目录")
    d.add_argument("a")
    d.add_argument("b")
    d.add_argument("--tables", help="逗号分隔的 db.table 白名单（默认：两侧出现的全部）")
    d.add_argument("--scope", choices=["all", "lb10"], default="all")
    d.add_argument("--require-tables", help="点名必须在两侧都出现的表（逗号分隔）")
    d.add_argument("--key-mode", choices=["pk", "multiset"], default="pk")
    d.add_argument("--time-mode", choices=["exact", "presence", "normalized"],
                   default="exact")
    d.add_argument("--null-token", default=NULL_TOKEN_DEFAULT)
    d.add_argument("--ignore-columns", default="",
                   help="逗号分隔；只允许已登记易变列/主键")
    d.add_argument("--allow-unknown-table", action="store_true")
    d.add_argument("--baseline-a")
    d.add_argument("--baseline-b")
    d.add_argument("--jsonl", default="", help="差异 JSONL 输出路径（- = stdout；默认不输出）")
    d.add_argument("--summary-out", help="摘要 JSON 输出路径")
    d.add_argument("--quiet", action="store_true")
    sub.add_parser("check-spec", help="与 scoped_diff.py 的表登记对账")
    sub.add_parser("selftest", help="离线改坏必红自检")
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.cmd == "selftest":
        return cmd_selftest(args)
    if args.cmd == "check-spec":
        drift = check_spec_drift()
        for x in drift:
            print(x)
        if drift and not drift[0].startswith("SKIP"):
            print("RED: 表登记与 scoped_diff.py 漂移")
            return 1
        print("GREEN: 表登记一致")
        return 0
    tables = None
    if args.scope == "lb10":
        tables = list(LB10_TABLES)
    if args.tables:
        tables = [t.strip() for t in args.tables.split(",") if t.strip()]
    require = [t.strip() for t in (args.require_tables or "").split(",") if t.strip()]
    ignore = [c.strip() for c in (args.ignore_columns or "").split(",") if c.strip()]
    try:
        rc, records, summary = run_pair(
            args.a, args.b, tables=tables, require_tables=require,
            key_mode=args.key_mode, time_mode=args.time_mode,
            null_token=args.null_token, ignore_columns=ignore,
            allow_unknown_table=args.allow_unknown_table,
            baseline_a=args.baseline_a, baseline_b=args.baseline_b,
            quiet=args.quiet)
    except InputError as e:
        print("RED: %s" % e, file=sys.stderr)
        return 2
    write_jsonl(args.jsonl, records)
    if args.summary_out:
        with open(args.summary_out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(summary, f, ensure_ascii=False, indent=2, sort_keys=True)
    return rc


if __name__ == "__main__":
    sys.exit(main())
