#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""真机夹具 provisioning（卡 `openmir2-mirflow-real-machine-prereqs`）。

目标：为「真机四类玩家动作」准备可执行的前置夹具——
A1 金币（+ 可选可卖物品）、A3 能发 643（`SM_MERCHANTSAY`）的 NPC、
以及落地位置；产出**一键脚本 + 校验命令 + 回滚**。

**红线（A4）**：只允许写 `mir2_test_db_r` / `mir2_test_account_r` / `mir2_test_data_r`。
库名不在白名单 ⇒ `guard_db` 直接抛 `DbRefused`（rc=3），**一条 SQL 都不会发出**；
生产库名（`mir2_db` / `mir2_account` / `mir2_data`）在报错里点名。

**A2（地板物品）**：两侧都**造不出**地面物 —— oracle `MapItem` 构造器里
`this.ItemId = M2Share.ActorMgr.GetNextIdentity();` 被注释（`src/Modules/SystemModule/Data/MapItem.cs:41`），
而 `Envirnoment.AddItemToMap` 第一行 `if (mapItem.ItemId == 0) return false;`
（`src/M2Server/Maps/Envirnoment.cs:88-92`）；Rust 1:1 复刻同一门（`mir2-rs/bins/game-svr/src/ground.rs:276`）。
本工具因此**拒绝伪造地面物**，`--a2-evidence` 把这组事实做成可机判的断言（任何一条变了即 rc=2，逼人重新评估）。

用法（窗口当天）::

    # 1) 看计划（只读，不写库）
    python -I -B tools/client/provision_real_machine_fixtures.py --check --char aaa

    # 2) 真写测试库（默认 dry-run；确认后加 --yes-apply）
    python -I -B tools/client/provision_real_machine_fixtures.py --apply \
        --db mir2_test_db_r --char aaa --gold 5000 --game-point 10 \
        --map 3 --char-x 326 --char-y 326 --yes-apply --report E:/tmp/fixtures-apply.json

    # 3) 登录后核对（帧里的金币必须等于刚写进 DB 的值）
    python -I -B tools/client/provision_real_machine_fixtures.py --verify \
        --frames <flow-out>/proxy/frames.ndjson --expect-gold 5000

    # 4) 回滚
    python -I -B tools/client/provision_real_machine_fixtures.py --rollback E:/tmp/fixtures-apply.json --yes

    # 5) 自检（离线：假 mysql + 合成 frames + 真 repo 取证）
    python -I -B tools/client/provision_real_machine_fixtures.py --selftest
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys

ALLOWED_DBS = ("mir2_test_db_r", "mir2_test_account_r", "mir2_test_data_r")
PROD_DBS = ("mir2_db", "mir2_account", "mir2_data")
DEFAULT_MYSQL = r"D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe"
DEFAULT_ENVIR_ROOT = r"E:/tmp/smoke/rust-root/M2GameSvr/Envir"
DEFAULT_MAP_ROOT = r"E:/tmp/smoke/rust-root/M2GameSvr/Map"
MAP_HEADER_BYTES = 52          # 口径同 tools/conformance/card_window.py:242
MAP_CELL_BYTES = 12
IDENT_RE = re.compile(r"^[A-Za-z0-9_]+$")

# A1/A3 默认落位：盟重省土城传送员（map 3 / 327,327），角色站 (326,326)（两格均可走，
# 口径 tools/conformance/card_window.py:read_map_cell）。
DEFAULT_NPC_MAP = "3"
DEFAULT_GOLD = 5000
DEFAULT_GAME_POINT = 10

RC_OK = 0
RC_PREREQ = 2
RC_REFUSED = 3
RC_MISMATCH = 4
RC_ROLLED_BACK = 5


class DbRefused(Exception):
    """库名不在测试库白名单（A4 红线）。"""


class Prereq(Exception):
    """前置不满足（配置/数据/夹具缺失）。"""


def guard_db(name):
    """A4：库名不在白名单 ⇒ 硬拒（生产库名点名）。"""
    if not isinstance(name, str) or not IDENT_RE.match(name):
        raise DbRefused("拒绝：库名 %r 不是合法标识符" % (name,))
    if name in PROD_DBS:
        raise DbRefused("拒绝：%r 是生产库（红线：绝不写生产库）" % name)
    if name not in ALLOWED_DBS:
        raise DbRefused("拒绝：库名 %r 不在测试库白名单 %s" % (name, list(ALLOWED_DBS)))
    return name


def ident(name):
    if not isinstance(name, str) or not IDENT_RE.match(name):
        raise Prereq("非法标识符：%r" % (name,))
    return "`" + name + "`"


def sql_str(s):
    return "'" + str(s).replace("\\", "\\\\").replace("'", "''") + "'"


def sql_int(v):
    return str(int(v))


# ---------------------------------------------------------------- mysql runner


class Mysql:
    """`mysql.exe --batch` 直连（与 tools/dbsnap 同口径，不依赖 Python 驱动）。"""

    def __init__(self, path, host, port, user, password, timeout=120):
        self.path, self.host, self.port = path, host, port
        self.user, self.password, self.timeout = user, password, timeout

    def _base(self):
        return [self.path, "-h", self.host, "-P", str(self.port),
                "-u", self.user, "--password=%s" % self.password, "--batch", "--raw"]

    def query(self, sql):
        r = subprocess.run(self._base() + ["--skip-column-names", "-e", sql],
                           capture_output=True, timeout=self.timeout)
        if r.returncode != 0:
            raise Prereq("mysql 读失败：%s" % r.stderr.decode("utf-8", "replace")[:300])
        out = r.stdout.decode("utf-8", "replace")
        rows = []
        for line in out.splitlines():
            if line.strip() == "":
                continue
            rows.append([c for c in line.split("\t")])
        return rows

    def exec_batch(self, text):
        r = subprocess.run(self._base() + ["--skip-column-names"],
                           input=text.encode("utf-8", "surrogateescape"),
                           capture_output=True, timeout=self.timeout)
        return r.returncode, r.stderr.decode("utf-8", "replace")[:400]


class FakeMysql:
    """自检用：canned 查询 + 记录每条语句；含 FAILME 的批次判失败。"""

    def __init__(self, rows_by_marker):
        # marker -> [应答1, 应答2, …]（按调用次序取，取尽后重复最后一条）
        self.rows_by_marker = [(m, list(seq)) for m, seq in rows_by_marker]
        self.calls = {}
        self.executed = []
        self.batches = []

    def query(self, sql):
        self.executed.append(sql)
        for marker, seq in self.rows_by_marker:
            if marker in sql:
                i = self.calls.get(marker, 0)
                self.calls[marker] = i + 1
                rows = seq[min(i, len(seq) - 1)]
                return [list(r) for r in rows]
        raise Prereq("FakeMysql 没有匹配 %r 的应答" % sql[:80])

    def exec_batch(self, text):
        self.executed.append(text)
        self.batches.append(text)
        if "FAILME" in text:
            return 1, "ERROR 1064 (42000): fake failure"
        return 0, ""


# ---------------------------------------------------------------- Envir 解析


def read_gbk(path):
    with open(path, "rb") as f:
        return f.read().decode("gbk", "replace")


def parse_merchants(path):
    """`Envir/MerChant.txt`（oracle 叫 Merchant.txt）：script map x y chr flag appr …。

    列序见 `mir2-rs/crates/envir/src/merchant.rs:135` `parse_merchant_line`。
    """
    rows = []
    for raw in read_gbk(path).splitlines():
        line = raw.strip()
        if not line or line.startswith(";"):
            continue
        cols = line.split()
        if len(cols) < 7:
            continue
        try:
            rows.append({
                "script": cols[0], "map": cols[1],
                "x": int(cols[2]), "y": int(cols[3]),
                "chr_name": cols[4], "flag": int(cols[5]), "appr": int(cols[6]),
            })
        except ValueError:
            continue
    return rows


def script_path_for(envir_root, script, map_name):
    """`Market_Def/<script>-<map>.txt`（script 可含 `/` 子目录）。"""
    return os.path.join(envir_root, "Market_Def", script + "-" + map_name + ".txt")


def parse_script_labels(text):
    """把脚本按 `[@label]` 分段：{label: [(lineno, text)]}（行号 1 起）。"""
    labels, cur = {}, None
    for i, raw in enumerate(text.splitlines(), 1):
        s = raw.strip()
        m = re.match(r"^\[@(.+?)\]$", s)
        if m:
            cur = m.group(1)
            labels.setdefault(cur, [])
            continue
        if cur is not None:
            labels[cur].append((i, s))
    return labels


def say_lines(text):
    """`[@main]` 下真正会说出口的行（`#`/`;`/空行/`[@...]` 不算）。"""
    labels = parse_script_labels(text)
    out = []
    for lineno, s in labels.get("main", []):
        if s.startswith("#"):
            break                      # `#IF/#ACT` 之后是命令体，不是说词
        if not s or s.startswith(";"):
            continue
        out.append({"line": lineno, "text": s.rstrip("\\").strip()})
    return out


def is_shop_script(text):
    """商店脚本判据：脚本头 `(@buy @sell)` 或存在 `[goods]` 段（trade 阶段要用）。"""
    return "(@buy" in text or "[goods]" in text


def pick_npc(envir_root, map_name, script=None):
    """挑一个脚本**确实会发 643** 的商人（`[@main]` 下有说词行）。

    优先级：**商店脚本**（`(@buy @sell)`/`[goods]`）> 其它有说词的商人；
    同档内按 `MerChant.txt` 文件序（确定性）。找不到 ⇒ PREREQ。
    """
    merchants = parse_merchants(os.path.join(envir_root, "MerChant.txt"))
    candidates = []
    for rec in merchants:
        if rec["map"] != map_name:
            continue
        if script and rec["script"] != script:
            continue
        path = script_path_for(envir_root, rec["script"], rec["map"])
        if not os.path.isfile(path):
            continue
        text = read_gbk(path)
        lines = say_lines(text)
        if not lines:
            continue
        rec = dict(rec)
        rec["script_file"] = path
        rec["main_say_lines"] = lines[:3]
        rec["main_say_count"] = len(lines)
        rec["is_shop"] = is_shop_script(text)
        candidates.append(rec)
    for rec in candidates:
        if rec["is_shop"]:
            return rec
    if candidates:
        return candidates[0]
    raise Prereq("map=%s 上找不到「脚本确有 @main 说词（⇒ 643）」的商人%s"
                 % (map_name, ("（限定 script=%s）" % script) if script else ""))


def is_walkable(map_path, x, y):
    """`.map` 格可走判定（口径同 `card_window.read_map_cell`：越界或 bk/fr bit15 = 不可走）。"""
    import struct
    try:
        with open(map_path, "rb") as f:
            header = f.read(MAP_HEADER_BYTES)
            if len(header) < MAP_HEADER_BYTES:
                raise Prereq("地图文件太短：%s" % map_path)
            width, height = struct.unpack_from("<hh", header, 0)
            if x < 0 or y < 0 or x >= width or y >= height:
                return False
            f.seek(MAP_HEADER_BYTES + (x * height + y) * MAP_CELL_BYTES)
            cell = f.read(MAP_CELL_BYTES)
    except OSError as e:
        raise Prereq("读不了地图文件 %s：%s" % (map_path, e))
    if len(cell) < MAP_CELL_BYTES:
        raise Prereq("地图文件被截断：%s" % map_path)
    bk, _mid, fr = struct.unpack_from("<HHH", cell, 0)
    return not (bk & 0x8000) and not (fr & 0x8000)


def derive_char_pos(map_root, map_name, npc):
    """站在 NPC 旁边：从 8 邻格起逐圈找**可走**格（找不到 ⇒ PREREQ，绝不猜）。"""
    map_path = os.path.join(map_root, map_name + ".map")
    if not os.path.isfile(map_path):
        raise Prereq("找不到地图文件 %s（无法验证落点可走）" % map_path)
    for r in (1, 2, 3):
        for dx in range(-r, r + 1):
            for dy in range(-r, r + 1):
                if max(abs(dx), abs(dy)) != r:
                    continue
                x, y = npc["x"] + dx, npc["y"] + dy
                if is_walkable(map_path, x, y):
                    return x, y
    raise Prereq("NPC (%s,%s) 周围 3 格内没有可走格" % (npc["x"], npc["y"]))


# ---------------------------------------------------------------- A2 取证


A2_FACTS = (
    ("oracle MapItem 构造器 ItemId 赋值被注释",
     "src/Modules/SystemModule/Data/MapItem.cs",
     "// this.ItemId = M2Share.ActorMgr.GetNextIdentity();"),
    ("oracle AddItemToMap 首行 ItemId==0 门",
     "src/M2Server/Maps/Envirnoment.cs",
     "if (mapItem.ItemId == 0)"),
    ("Rust put_item 复刻同一门",
     "mir2-rs/bins/game-svr/src/ground.rs",
     "if item.item_id == 0"),
)


def a2_evidence(repo_root):
    """A2：地面物**结构性不可达**的机判取证（任一事实变了 ⇒ 重新评估）。"""
    facts, missing = [], []
    for desc, rel, needle in A2_FACTS:
        path = os.path.join(repo_root, rel)
        try:
            with open(path, encoding="utf-8", errors="replace") as f:
                ok = needle in f.read()
        except OSError:
            ok = False
        facts.append({"fact": desc, "file": rel, "needle": needle, "holds": ok})
        if not ok:
            missing.append(rel)
    return {
        "verdict": "BLOCKED" if not missing else "REEVALUATE",
        "facts": facts,
        "missing": missing,
        "conclusion": ("两侧都造不出地面物 ⇒ pickup 阶段在真机上无物可捡；"
                       "本工具不伪造地面物（伪造即偏离 oracle）"),
    }


# ---------------------------------------------------------------- 计划/写入


def build_plan(args, npc, char_x, char_y):
    return {
        "db": args.db,
        "char": args.char,
        "gold": args.gold,
        "game_point": args.game_point,
        "map": args.map,
        "char_x": char_x,
        "char_y": char_y,
        "npc": {k: npc[k] for k in ("script", "map", "x", "y", "chr_name", "appr",
                                    "script_file", "main_say_count", "main_say_lines",
                                    "is_shop")},
        "bag_item": ({"std_index": args.bag_std_index, "dura": args.bag_dura,
                      "dura_max": args.bag_dura_max, "make_index": args.bag_make_index,
                      "position": args.bag_position}
                     if args.bag_std_index is not None else None),
    }


def select_character(mysql, db, char):
    rows = mysql.query(
        "SELECT `Id`,`ChrName`,`MapName`,`CX`,`CY`,`Gold`,`GamePoint`,`Level` "
        "FROM %s.`characters` WHERE `ChrName`=%s" % (ident(db), sql_str(char)))
    if len(rows) != 1:
        raise Prereq("角色 %r 在 %s.characters 里命中 %d 行（必须恰好 1 行）"
                     % (char, db, len(rows)))
    r = rows[0]
    return {"Id": r[0], "ChrName": r[1], "MapName": r[2], "CX": r[3], "CY": r[4],
            "Gold": r[5], "GamePoint": r[6], "Level": r[7]}


def apply_plan(mysql, plan, dry_run=False):
    """先 SELECT 证现状 → 写 → 再 SELECT 证结果；失败 ROLLBACK。"""
    db = plan["db"]
    before = select_character(mysql, db, plan["char"])
    row_id = int(before["Id"])

    stmts = ["START TRANSACTION;"]
    stmts.append(
        "UPDATE %s.`characters` SET `Gold`=%s, `GamePoint`=%s, `MapName`=%s, `CX`=%s, `CY`=%s "
        "WHERE `Id`=%s AND `ChrName`=%s AND `Gold`=%s AND `MapName`=%s;"
        % (ident(db), sql_int(plan["gold"]), sql_int(plan["game_point"]),
           sql_str(plan["map"]), sql_int(plan["char_x"]), sql_int(plan["char_y"]),
           sql_int(row_id), sql_str(plan["char"]),
           sql_int(before["Gold"]), sql_str(before["MapName"])))
    rollback_stmts = [
        "START TRANSACTION;",
        "UPDATE %s.`characters` SET `Gold`=%s, `GamePoint`=%s, `MapName`=%s, `CX`=%s, `CY`=%s "
        "WHERE `Id`=%s AND `ChrName`=%s;"
        % (ident(db), sql_int(before["Gold"]), sql_int(before["GamePoint"]),
           sql_str(before["MapName"]), sql_int(before["CX"]), sql_int(before["CY"]),
           sql_int(row_id), sql_str(before["ChrName"])),
    ]
    bag = plan.get("bag_item")
    if bag:
        used = mysql.query("SELECT COUNT(*) FROM %s.`characters_bagitem` WHERE `MakeIndex`=%s"
                           % (ident(db), sql_int(bag["make_index"])))
        if int(used[0][0]) != 0:
            raise Prereq("MakeIndex %s 已被占用（必须先 SELECT 证空位）" % bag["make_index"])
        stmts.append(
            "INSERT INTO %s.`characters_bagitem` (`PlayerId`,`Position`,`MakeIndex`,`StdIndex`,`Dura`,`DuraMax`) "
            "VALUES (%s,%s,%s,%s,%s,%s);"
            % (ident(db), sql_int(row_id), sql_int(bag["position"]), sql_int(bag["make_index"]),
               sql_int(bag["std_index"]), sql_int(bag["dura"]), sql_int(bag["dura_max"])))
        rollback_stmts.append(
            "DELETE FROM %s.`characters_bagitem` WHERE `PlayerId`=%s AND `MakeIndex`=%s;"
            % (ident(db), sql_int(row_id), sql_int(bag["make_index"])))
    stmts.append("COMMIT;")
    rollback_stmts.append("COMMIT;")

    report = {"mode": "apply", "dry_run": bool(dry_run), "db": db, "char": plan["char"],
              "id": row_id, "before": before, "statements": stmts,
              "rollback": rollback_stmts, "state": "planned"}

    if dry_run:
        report["note"] = "dry-run：未写库；确认后加 --yes-apply"
        return report, RC_OK

    rc, err = mysql.exec_batch("\n".join(stmts))
    if rc != 0:
        mysql.exec_batch("ROLLBACK;")             # 兜底（客户端遇错即停，会话本会整体回滚）
        report.update({"state": "rolled_back", "error": err})
        return report, RC_ROLLED_BACK

    after = select_character(mysql, db, plan["char"])
    report["after"] = after
    ok = (int(after["Gold"]) == int(plan["gold"])
          and int(after["GamePoint"]) == int(plan["game_point"])
          and after["MapName"] == plan["map"]
          and int(after["CX"]) == int(plan["char_x"])
          and int(after["CY"]) == int(plan["char_y"]))
    report["state"] = "applied" if ok else "MISMATCH"
    return report, (RC_OK if ok else RC_MISMATCH)


def rollback_plan(mysql, report, dry_run=False):
    guard_db(report.get("db"))
    stmts = report.get("rollback") or []
    if not stmts:
        raise Prereq("报告里没有 rollback 语句")
    if dry_run:
        return {"mode": "rollback", "dry_run": True, "statements": stmts}, RC_OK
    before = select_character(mysql, report["db"], report["char"])
    rc, err = mysql.exec_batch("\n".join(stmts))
    if rc != 0:
        return {"mode": "rollback", "state": "failed", "error": err}, RC_ROLLED_BACK
    after = select_character(mysql, report["db"], report["char"])
    want = {k: str(report["before"][k]) for k in ("Gold", "GamePoint", "MapName", "CX", "CY")}
    got = {k: str(after[k]) for k in ("Gold", "GamePoint", "MapName", "CX", "CY")}
    ok = want == got
    return {"mode": "rollback", "state": "restored" if ok else "MISMATCH",
            "before_rollback": before, "after_rollback": after,
            "want": want, "got": got}, (RC_OK if ok else RC_MISMATCH)


# ---------------------------------------------------------------- 帧核对


def verify_frames(path, expect_gold, expect_rows=None):
    """A1：登录后客户端帧里的金币（`SM_ABILITY(52).recog`）必须等于 DB 值。"""
    gold, rows, seen52 = None, None, 0
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                ev = json.loads(line)
            except ValueError:
                continue
            if ev.get("dir") != "s2c":
                continue
            try:
                ident_v = int(ev.get("ident"))
            except (TypeError, ValueError):
                continue
            if ident_v == 52:
                gold, seen52 = int(ev.get("recog") or 0), seen52 + 1
            elif ident_v == 653:
                gold = int(ev.get("recog") or 0)
            elif ident_v == 201:
                rows = int(ev.get("series") or 0)
    if seen52 == 0 and gold is None:
        return {"verdict": "PREREQ", "reason": "帧里没有 s2c SM_ABILITY(52)/SM_GOLDCHANGED(653)"}, RC_PREREQ
    out = {"verdict": "PASS", "gold_in_frames": gold, "expect_gold": int(expect_gold),
           "sm_ability_frames": seen52, "bag_rows": rows}
    if int(gold) != int(expect_gold):
        out.update({"verdict": "MISMATCH",
                    "reason": "帧里金币 %s != 期望 %s（DB 与客户端不一致）" % (gold, expect_gold)})
        return out, RC_MISMATCH
    if expect_rows is not None and rows != int(expect_rows):
        out.update({"verdict": "MISMATCH",
                    "reason": "SM_BAGITEMS(201).series=%s != 期望 %s" % (rows, expect_rows)})
        return out, RC_MISMATCH
    return out, RC_OK


# ---------------------------------------------------------------- 自检


def _chk(ok, name, detail=""):
    print(("PASS " if ok else "FAIL ") + name + (("  (" + detail + ")") if detail else ""))
    return ok


def _mk_envir(tmp):
    """合成最小 Envir：一个会说词的商人 + 一个不说话的商人（负例）。"""
    envir = os.path.join(tmp, "Envir")
    os.makedirs(os.path.join(envir, "Market_Def", "test"), exist_ok=True)
    lines = [
        "test/好玩商人 9 100 100 好玩商人 0 7 0",
        "test/哑巴商人 9 101 101 哑巴商人 0 7 0",
        "test/无脚本商人 9 102 102 无脚本 0 7 0",
        "test/商店 9 103 103 商店老板 0 7 0",
    ]
    with open(os.path.join(envir, "MerChant.txt"), "wb") as f:
        f.write(("\r\n".join(lines) + "\r\n").encode("gbk"))
    with open(os.path.join(envir, "Market_Def", "test", "好玩商人-9.txt"), "wb") as f:
        f.write(b"[@main]\r\nhello there\\\r\nMESSAGEBOX test\r\n[@other]\r\nx\r\n")
    with open(os.path.join(envir, "Market_Def", "test", "哑巴商人-9.txt"), "wb") as f:
        f.write(b"[@main]\r\n#if\r\ncheckgold 1\r\n#act\r\nMESSAGEBOX no say\r\n")
    with open(os.path.join(envir, "Market_Def", "test", "商店-9.txt"), "wb") as f:
        f.write(b"(@buy @sell)\r\n[goods]\r\nitem 100 3\r\n[@main]\r\nbuy or sell\r\n")
    return envir


def selftest(args):
    import tempfile

    tmp = tempfile.mkdtemp(prefix="prov-fixtures-")
    ok_all = True
    try:
        # ---- A4 守卫三颗牙：拒绝且**一条 SQL 都没发出**
        for db, tag in (("mir2_db", "生产库"), ("mir2_test_db", "C# 测试库"), ("mir2_test_db_r; DROP", "注入")):
            fake = FakeMysql([])
            try:
                guard_db(db)
                ok_all &= _chk(False, "A4/%s 未被拒" % tag, db)
            except DbRefused as e:
                ok_all &= _chk(fake.executed == [], "A4/%s 硬拒且零 SQL" % tag, str(e)[:60])
        ok_all &= _chk(guard_db("mir2_test_db_r") == "mir2_test_db_r", "A4/白名单库放行")

        # ---- A1 事务：before 登记 → 写 → after 核对；回滚语句还原 before
        before_row = ["7", "aaa", "0", "289", "618", "0", "0", "1"]
        after_row = ["7", "aaa", "3", "326", "326", "5000", "10", "1"]
        fake = FakeMysql([("`characters` WHERE `ChrName`", [[before_row], [after_row]])])
        plan = {"db": "mir2_test_db_r", "char": "aaa", "gold": 5000, "game_point": 10,
                "map": "3", "char_x": 326, "char_y": 326, "bag_item": None}
        rep, rc = apply_plan(fake, plan)
        ok_all &= _chk(rc == RC_OK and rep["state"] == "applied", "A1 应用到测试库 rc=0", "state=%s" % rep["state"])
        ok_all &= _chk(rep["before"]["Gold"] == "0", "A1 before 已登记", rep["before"]["Gold"])
        ok_all &= _chk("ROLLBACK" not in rep and "COMMIT;" in fake.batches[0], "A1 单事务提交")
        rb = rep["rollback"][1]
        ok_all &= _chk("`Gold`=0" in rb and "`MapName`='0'" in rb, "A1 回滚语句带 before 值")

        # ---- 失败 ⇒ ROLLBACK 且 rc≠0
        fake_fail = FakeMysql([("`characters` WHERE `ChrName`", [[before_row]])])
        plan_fail = dict(plan, gold=5000)
        orig_exec = fake_fail.exec_batch

        def exec_fail(text):
            return orig_exec(text + "\n-- FAILME")

        fake_fail.exec_batch = exec_fail
        rep_f, rc_f = apply_plan(fake_fail, plan_fail)
        ok_all &= _chk(rc_f == RC_ROLLED_BACK and rep_f["state"] == "rolled_back",
                       "A1 写失败 ⇒ rolled_back 且 rc≠0", "rc=%s" % rc_f)
        ok_all &= _chk(any("ROLLBACK;" in b for b in fake_fail.batches), "A1 失败 ⇒ 发出 ROLLBACK")

        # ---- 可卖物品：MakeIndex 被占 ⇒ PREREQ（不写）
        fake_bag = FakeMysql([("`characters` WHERE `ChrName`", [[before_row]]),
                              ("COUNT(*) FROM", [[["1"]]])])
        plan_bag = dict(plan, bag_item={"std_index": 207, "dura": 4000, "dura_max": 4000,
                                        "make_index": 5001, "position": 40})
        try:
            apply_plan(fake_bag, plan_bag)
            ok_all &= _chk(False, "A1 占用 MakeIndex 未拦")
        except Prereq:
            ok_all &= _chk(not any("INSERT" in b for b in fake_bag.batches), "A1 MakeIndex 被占 ⇒ 不写")

        # ---- A1 帧核对：一致 ⇒ PASS；不一致 ⇒ MISMATCH
        frames = os.path.join(tmp, "frames.ndjson")
        with open(frames, "w", encoding="utf-8") as f:
            f.write(json.dumps({"dir": "s2c", "ident": 52, "recog": 5000}) + "\n")
            f.write(json.dumps({"dir": "s2c", "ident": 201, "series": 3}) + "\n")
        out, rc = verify_frames(frames, 5000, 3)
        ok_all &= _chk(rc == RC_OK and out["verdict"] == "PASS", "A1 帧金币与 DB 一致 ⇒ PASS")
        out, rc = verify_frames(frames, 0, 3)
        ok_all &= _chk(rc == RC_MISMATCH and "5000" in out["reason"], "A1 帧金币不一致 ⇒ MISMATCH", out.get("reason", ""))
        with open(os.path.join(tmp, "empty.ndjson"), "w", encoding="utf-8") as f:
            f.write("{}\n")
        out, rc = verify_frames(os.path.join(tmp, "empty.ndjson"), 5000)
        ok_all &= _chk(rc == RC_PREREQ, "A1 无 52/653 ⇒ PREREQ（不假绿）")

        # ---- A3：合成 Envir 选商人（会说词）+ 负例（只有 #act MESSAGEBOX 不算 643）
        envir = _mk_envir(tmp)
        npc = pick_npc(envir, "9")
        ok_all &= _chk(npc["script"] == "test/商店" and npc["is_shop"],
                       "A3 商店脚本（(@buy @sell)/[goods]）优先", "%s shop=%s" % (npc["script"], npc["is_shop"]))
        npc2 = pick_npc(envir, "9", script="test/好玩商人")
        ok_all &= _chk(npc2["script"] == "test/好玩商人" and not npc2["is_shop"],
                       "A3 指定非商店脚本时照样可选（643 说词成立）")
        npc = npc2
        ok_all &= _chk(npc["main_say_lines"][0]["line"] == 2, "A3 给出脚本 文件:行号", str(npc["main_say_lines"][0]))
        try:
            pick_npc(envir, "9", script="test/哑巴商人")
            ok_all &= _chk(False, "A3 无说词脚本未拦")
        except Prereq:
            ok_all &= _chk(True, "A3 只有 #act MESSAGEBOX 的脚本不算 643 ⇒ 拒绝")
        ok_all &= _chk(not os.path.exists(script_path_for(envir, "test/无脚本商人", "9")),
                       "A3 缺脚本文件的商人不被选中")

        # ---- A3 落点：从 NPC 旁挑可走格；墙格跳过；缺地图 ⇒ PREREQ（不猜）
        import struct
        map_root = os.path.join(tmp, "Map")
        os.makedirs(map_root, exist_ok=True)
        w = h = 8
        cells = []
        for x in range(w):
            for y in range(h):
                bk = 0x8000 if (x, y) == (4, 4) else 0     # (4,4) 是墙；NPC 就站那
                cells.append(struct.pack("<HHH", bk, 0, 0) + bytes(MAP_CELL_BYTES - 6))
        with open(os.path.join(map_root, "9.map"), "wb") as f:
            f.write(struct.pack("<hh", w, h) + bytes(MAP_HEADER_BYTES - 4) + b"".join(cells))
        cx, cy = derive_char_pos(map_root, "9", {"x": 4, "y": 4})
        ok_all &= _chk((cx, cy) != (4, 4) and is_walkable(os.path.join(map_root, "9.map"), cx, cy),
                       "A3 落点取 NPC 旁可走格（跳过墙格）", "(%d,%d)" % (cx, cy))
        try:
            derive_char_pos(map_root, "nope", {"x": 1, "y": 1})
            ok_all &= _chk(False, "A3 缺地图文件未拦")
        except Prereq:
            ok_all &= _chk(True, "A3 缺地图 ⇒ PREREQ（不猜落点）")

        # ---- A2：真 repo 三事实成立 ⇒ BLOCKED；把之一改坏 ⇒ REEVALUATE
        repo = args.repo_root
        ev = a2_evidence(repo)
        ok_all &= _chk(ev["verdict"] == "BLOCKED", "A2 地面物结构性不可达（三事实成立）")
        bad_repo = os.path.join(tmp, "repo")
        for desc, rel, needle in A2_FACTS:
            p = os.path.join(bad_repo, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w", encoding="utf-8") as f:
                f.write(needle + "\n")
        with open(os.path.join(bad_repo, A2_FACTS[2][1]), "w", encoding="utf-8") as f:
            f.write("// gate removed\n")
        ev2 = a2_evidence(bad_repo)
        ok_all &= _chk(ev2["verdict"] == "REEVALUATE" and bool(ev2["missing"]),
                       "A2 事实被改动 ⇒ REEVALUATE（不静默绿）", str(ev2["missing"]))

        # ---- 生成的 SQL 不得出现任何伪造地面物
        joined = " ".join(rep["statements"] + rep["rollback"]).lower()
        ok_all &= _chk("mapitem" not in joined and "ground" not in joined,
                       "A2 provisioning 不伪造地面物（SQL 无 mapitem/ground）")
    finally:
        import shutil
        shutil.rmtree(tmp, ignore_errors=True)

    print("SELFTEST " + ("PASS" if ok_all else "FAIL"))
    return RC_OK if ok_all else RC_PREREQ


# ---------------------------------------------------------------- CLI


def build_mysql(args):
    return Mysql(args.mysql, args.host, args.port, args.user, args.password)


def add_db_args(ap):
    ap.add_argument("--mysql", default=DEFAULT_MYSQL, help="mysql.exe 路径")
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=3306)
    ap.add_argument("--user", default="root")
    ap.add_argument("--password", default="")


def main(argv=None):
    ap = argparse.ArgumentParser(description="真机夹具 provisioning（只写 mir2_test_*_r）")
    ap.add_argument("--envir-root", default=DEFAULT_ENVIR_ROOT)
    ap.add_argument("--repo-root", default=os.path.dirname(os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))))
    ap.add_argument("--char", default="aaa")
    ap.add_argument("--gold", type=int, default=DEFAULT_GOLD)
    ap.add_argument("--game-point", type=int, default=DEFAULT_GAME_POINT)
    ap.add_argument("--map-root", default=DEFAULT_MAP_ROOT)
    ap.add_argument("--map", default=DEFAULT_NPC_MAP)
    # 缺省不写死落点：按 NPC 位置从 8 邻格逐圈找可走格（找不到 ⇒ PREREQ，不猜）
    ap.add_argument("--char-x", type=int, default=None)
    ap.add_argument("--char-y", type=int, default=None)
    ap.add_argument("--npc-script", default=None)
    ap.add_argument("--bag-std-index", type=int, default=None,
                    help="可选：塞一件可卖物品到 characters_bagitem（缺省不塞）")
    ap.add_argument("--bag-dura", type=int, default=4000)
    ap.add_argument("--bag-dura-max", type=int, default=4000)
    ap.add_argument("--bag-make-index", type=int, default=900001)
    ap.add_argument("--bag-position", type=int, default=40)

    mode = ap.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="只读：打印计划（不连库）")
    mode.add_argument("--apply", action="store_true", help="写测试库（默认 dry-run）")
    mode.add_argument("--rollback", metavar="REPORT", help="按 apply 报告回滚")
    mode.add_argument("--verify", action="store_true", help="核对登录帧里的金币")
    mode.add_argument("--a2-evidence", action="store_true", help="A2 地面物结构性不可达取证")
    mode.add_argument("--selftest", action="store_true")

    ap.add_argument("--db", default="mir2_test_db_r")
    ap.add_argument("--frames", default=None)
    ap.add_argument("--expect-gold", type=int, default=None)
    ap.add_argument("--expect-rows", type=int, default=None)
    ap.add_argument("--report", default=None, help="apply 报告输出路径")
    ap.add_argument("--yes-apply", action="store_true")
    ap.add_argument("--yes", action="store_true", help="rollback 真写")
    add_db_args(ap)
    args = ap.parse_args(argv)

    try:
        if args.selftest:
            return selftest(args)

        if args.a2_evidence:
            ev = a2_evidence(args.repo_root)
            print(json.dumps(ev, ensure_ascii=False, indent=1))
            return RC_OK if ev["verdict"] == "BLOCKED" else RC_PREREQ

        if args.verify:
            if not args.frames or args.expect_gold is None:
                raise Prereq("--verify 需要 --frames 与 --expect-gold")
            out, rc = verify_frames(args.frames, args.expect_gold, args.expect_rows)
            print(json.dumps(out, ensure_ascii=False, indent=1))
            return rc

        if args.rollback:
            guard_db(args.db)
            with open(args.rollback, encoding="utf-8") as f:
                report = json.load(f)
            mysql = build_mysql(args)
            out, rc = rollback_plan(mysql, report, dry_run=not args.yes)
            print(json.dumps(out, ensure_ascii=False, indent=1))
            return rc

        npc = pick_npc(args.envir_root, args.map, args.npc_script)
        if (args.char_x is None) != (args.char_y is None):
            raise Prereq("--char-x / --char-y 必须同时给或同时不给")
        if args.char_x is None:
            char_x, char_y = derive_char_pos(args.map_root, npc["map"], npc)
        else:
            char_x, char_y = args.char_x, args.char_y
            if not is_walkable(os.path.join(args.map_root, npc["map"] + ".map"), char_x, char_y):
                raise Prereq("指定落点 (%d,%d) 在 map=%s 上不可走" % (char_x, char_y, npc["map"]))
        plan = build_plan(args, npc, char_x, char_y)

        if args.check:
            plan["a2"] = a2_evidence(args.repo_root)["verdict"]
            print(json.dumps(plan, ensure_ascii=False, indent=1))
            return RC_OK

        guard_db(args.db)                      # A4：写库前最后一道门（先拒后连）
        mysql = build_mysql(args)
        out, rc = apply_plan(mysql, plan, dry_run=not args.yes_apply)
        text = json.dumps(out, ensure_ascii=False, indent=1)
        print(text)
        if args.report:
            with open(args.report, "w", encoding="utf-8", newline="\n") as f:
                f.write(text + "\n")
        return rc
    except DbRefused as e:
        print("REFUSED: %s" % e, file=sys.stderr)
        return RC_REFUSED
    except Prereq as e:
        print("PREREQ: %s" % e, file=sys.stderr)
        return RC_PREREQ


if __name__ == "__main__":
    sys.exit(main())
