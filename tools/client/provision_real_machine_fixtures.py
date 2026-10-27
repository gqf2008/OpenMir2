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

    # 5) 动作前的前置门（A6）：备齐=PASS；缺项=UNDECIDABLE(kind=missing_prereq) 且点名缺哪项
    python -I -B tools/client/provision_real_machine_fixtures.py --gate         --db mir2_test_db_r --char aaa --map 3 --profile <profile.json>

    # 6) 自检（离线：假 mysql + 合成 frames + 真 repo 取证）
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


# ---------------------------------------------------------------- 动作前的前置门（A6）


RC_UNDECIDABLE = 6


def load_profile(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def profile_monsters(profile, map_name):
    """profile `fixtures.monsters` 里该图的靶子（schema 见 card_window `_validate_monsters`）。"""
    fixtures = (profile or {}).get("fixtures") or {}
    mons = fixtures.get("monsters") or []
    return [m for m in mons if str(m.get("map")) == str(map_name)]


def prereq_gate(args, mysql):
    """A6：动作前的显式门。**能不能测 = UNDECIDABLE（kind=missing_prereq，点名缺项）；
    全备齐 = PASS。** 不得把「缺夹具」记成 RED（那是「该发生却没发生」的档）。

    检查项（`--need`，缺省全查）：
      gold          金币 >= --min-gold（读库）
      sellable_item 背包行数 >= 1（读库）
      npc           同图有「脚本确有 [@main] 说词（⇒ 643）」的商人（读 Envir）
      floor_item    **结构性不可达**（见 `--a2-evidence`）⇒ 永远报缺，供 pickup 阶段判 UNDECIDABLE
      monsters      profile `fixtures.monsters` 在该图有靶子（读 profile）
    """
    need = args.need or ["gold", "sellable_item", "npc", "floor_item", "monsters"]
    missing, reasons, details = [], {}, {}

    char_row = None
    if "gold" in need or "sellable_item" in need:
        char_row = select_character(mysql, args.db, args.char)
        details["char"] = char_row

    if "gold" in need:
        gold = int(char_row["Gold"])
        details["gold"] = gold
        if gold < int(args.min_gold):
            missing.append("gold")
            reasons["gold"] = "characters.Gold=%d < 要求的 %d（买卖无钱）" % (gold, int(args.min_gold))

    if "sellable_item" in need:
        # 背包 46 行是**槽位**，空槽 `StdIndex=0` 不算物品 ⇒ 只看 `StdIndex<>0` 的行
        rows = mysql.query(
            "SELECT COUNT(*) FROM %s.`characters_bagitem` WHERE `PlayerId`=%s"
            % (ident(args.db), sql_int(char_row["Id"])))
        items = mysql.query(
            "SELECT COUNT(*) FROM %s.`characters_bagitem` WHERE `PlayerId`=%s AND `StdIndex`<>0"
            % (ident(args.db), sql_int(char_row["Id"])))
        details["bag_rows"] = int(rows[0][0])
        details["bag_items"] = int(items[0][0])
        if int(items[0][0]) < 1:
            missing.append("sellable_item")
            reasons["sellable_item"] = ("characters_bagitem 有 %d 个槽位但 `StdIndex<>0` 的 0 行"
                                        "（没有可卖物品）" % int(rows[0][0]))

    if "npc" in need:
        try:
            npc = pick_npc(args.envir_root, args.map, args.npc_script)
            details["npc"] = {k: npc[k] for k in ("script", "map", "x", "y", "chr_name",
                                                  "script_file", "main_say_count", "is_shop")}
        except Prereq as e:
            missing.append("npc")
            reasons["npc"] = str(e)

    if "floor_item" in need:
        ev = a2_evidence(args.repo_root)
        details["floor_item"] = ev["verdict"]
        missing.append("floor_item")
        reasons["floor_item"] = ("地面物结构性不可达（%s）⇒ pickup 阶段这一轮不能测"
                                 % ev["conclusion"])

    if "monsters" in need:
        if not args.profile:
            missing.append("monsters")
            reasons["monsters"] = "未给 --profile（无法证明该图有靶子夹具）"
        else:
            try:
                mons = profile_monsters(load_profile(args.profile), args.map)
            except (OSError, ValueError) as e:
                missing.append("monsters")
                reasons["monsters"] = "profile 读不了：%s" % e
            else:
                details["monsters_for_map"] = len(mons)
                if not mons:
                    missing.append("monsters")
                    reasons["monsters"] = "profile fixtures.monsters 在 map=%s 没有靶子" % args.map

    out = {
        "mode": "gate",
        "kind": "missing_prereq" if missing else "prereq_ok",
        "verdict": "UNDECIDABLE" if missing else "PASS",
        "need": list(need),
        "missing": missing,
        "reasons": reasons,
        "details": details,
    }
    return out, (RC_UNDECIDABLE if missing else RC_OK)


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
    print(("PASS " if ok else "FAIL ") + name
          + (("  (" + str(detail) + ")") if detail else ""))
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
        # ---- A4/F2 守卫三颗牙：经 main() 真走一遍——被拒时 build_mysql **不得被调用**、零 SQL。
        # 牙：把 `guard_db(args.db)` 挪到 `build_mysql(args)` **之后** ⇒ 下面三条必红
        # （calls 变 1 且 FakeMysql 被查询）。旧写法只调 guard_db、fake 从不参与 ⇒ 空断言。
        import contextlib
        import io

        def guard_probe(db_name):
            """跑一次 --gate（guard 在 build 之前），返回 (rc, build 调用次数, SQL 条数, 输出)。

            输出里带 `REFUSED: <理由>`——三条牙要钉**判词分支**，不只钉「被拒」。
            （reviewer-21：把 guard_db 换成 `raise DbRefused("MUT-stub")` ⇒ 旧牙全绿；
              reviewer-20：删掉 PROD_DBS 分支 ⇒ 被白名单兜住 ⇒ 旧牙仍全绿。）
            """
            fake = FakeMysql([])
            calls = {"n": 0}
            real = globals()["build_mysql"]

            def fake_build(_args):
                calls["n"] += 1
                return fake

            globals()["build_mysql"] = fake_build
            try:
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
                    rc = main(["--gate", "--db", db_name, "--char", "aaa", "--map", "9"])
            finally:
                globals()["build_mysql"] = real
            return rc, calls["n"], len(fake.executed), buf.getvalue()

        # 三条牙各钉一条**判词分支**：生产库 / 不在白名单 / 非法标识符。
        # 牙 = ① 删掉 PROD_DBS 分支 ⇒ 第一条红（落到白名单，理由是另一句）；
        #       ② `guard_db(args.db)` 换成 `raise DbRefused("MUT-stub")` ⇒ 三条全红（理由对不上）。
        for db_name, tag, need in (("mir2_db", "生产库", "是生产库"),
                                   ("mir2_test_db", "C# 测试库", "不在测试库白名单"),
                                   ("mir2_test_db_r; DROP", "注入", "不是合法标识符")):
            rc_g, calls_g, sql_g, out_g = guard_probe(db_name)
            ok_all &= _chk(rc_g == RC_REFUSED and calls_g == 0 and sql_g == 0 and need in out_g,
                           "A4/%s 硬拒（理由含「%s」）+ 未连库 + 零 SQL" % (tag, need),
                           "rc=%s build=%s sql=%s reason=%s"
                           % (rc_g, calls_g, sql_g, out_g.strip().replace("\n", " ")[:70]))
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
        # F3：UPDATE 的 WHERE 必须带**四个**谓词（Id/ChrName/Gold/MapName）——退回 Id+ChrName ⇒ 必红
        upd = [q for q in rep["statements"] if q.startswith("UPDATE")][0]
        cond = upd.split(" WHERE ", 1)[1]
        ok_all &= _chk(all(t in cond for t in ("`Id`=7", "`ChrName`='aaa'", "`Gold`=0", "`MapName`='0'")),
                       "F3 乐观锁四谓词（Id+ChrName+Gold+MapName）", cond)

        # ---- F1 dry-run 两颗牙：变异 `if dry_run:` → `if False:` ⇒ 对应那条必红
        fake_dry = FakeMysql([("`characters` WHERE `ChrName`", [[before_row]])])
        rep_d, rc_d = apply_plan(fake_dry, plan, dry_run=True)
        ok_all &= _chk(rc_d == RC_OK and rep_d["dry_run"] is True and fake_dry.batches == []
                       and not any("UPDATE" in q for q in fake_dry.executed),
                       "F1 apply dry-run 零 exec_batch（不写库）",
                       "batches=%d" % len(fake_dry.batches))
        # 给 fake 备好应答：变异（`if False:`）后 rollback 会真的 SELECT+exec ⇒ 断言以 FAIL 报出，
        # 而不是在 FakeMysql 里抛 PREREQ 把整个自检崩掉（红要红在断言上）。
        fake_rbd = FakeMysql([("`characters` WHERE `ChrName`", [[before_row], [before_row]])])
        out_rbd, rc_rbd = rollback_plan(fake_rbd, rep_d, dry_run=True)
        ok_all &= _chk(rc_rbd == RC_OK and fake_rbd.executed == [] and fake_rbd.batches == [],
                       "F1 rollback dry-run 零 SQL（连 SELECT 都不发）",
                       "executed=%d" % len(fake_rbd.executed))

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

        # ---- A6 前置门：缺项 ⇒ UNDECIDABLE(kind=missing_prereq) 并点名；备齐 ⇒ PASS
        poor = ["7", "aaa", "0", "289", "618", "0", "0", "1"]
        rich = ["7", "aaa", "3", "360", "334", "5000", "10", "1"]
        prof = os.path.join(tmp, "profile.json")
        with open(prof, "w", encoding="utf-8") as f:
            json.dump({"fixtures": {"monsters": [{"map": "9", "x": 5, "y": 5, "name": "鸡", "count": 1}]}}, f)
        ga = argparse.Namespace(db="mir2_test_db_r", char="aaa", envir_root=envir, map="9",
                                npc_script=None, profile=prof, repo_root=args.repo_root,
                                min_gold=500, need=["gold", "sellable_item", "monsters"])
        fake_gate = FakeMysql([("`characters` WHERE `ChrName`", [[poor]]),
                               ("`StdIndex`<>0", [[["0"]]]),
                               ("COUNT(*) FROM", [[["46"]]])])
        out, rc = prereq_gate(ga, fake_gate)
        ok_all &= _chk(rc == RC_UNDECIDABLE and out["kind"] == "missing_prereq"
                       and set(out["missing"]) == {"gold", "sellable_item"},
                       "A6 缺金币/可卖物 ⇒ UNDECIDABLE 且点名（不是 RED）", str(out["missing"]))
        fake_ok = FakeMysql([("`characters` WHERE `ChrName`", [[rich]]),
                             ("`StdIndex`<>0", [[["3"]]]),
                             ("COUNT(*) FROM", [[["46"]]])])
        out, rc = prereq_gate(ga, fake_ok)
        ok_all &= _chk(rc == RC_OK and out["verdict"] == "PASS" and out["missing"] == [],
                       "A6 备齐 ⇒ PASS（missing 为空）")
        ga_bag = argparse.Namespace(**dict(vars(ga), need=["sellable_item"]))
        empty_bag = FakeMysql([("`characters` WHERE `ChrName`", [[rich]]),
                               ("`StdIndex`<>0", [[["0"]]]),
                               ("COUNT(*) FROM", [[["46"]]])])
        out, rc = prereq_gate(ga_bag, empty_bag)
        ok_all &= _chk(rc == RC_UNDECIDABLE and out["missing"] == ["sellable_item"]
                       and out["details"]["bag_rows"] == 46 and out["details"]["bag_items"] == 0,
                       "A6 46 个空槽不算可卖物品（StdIndex<>0 才算）",
                       "rows=%s items=%s" % (out["details"]["bag_rows"], out["details"]["bag_items"]))

        ga_all = argparse.Namespace(**dict(vars(ga), need=None, map="9"))
        g = FakeMysql([("`characters` WHERE `ChrName`", [[rich]]),
                       ("`StdIndex`<>0", [[["3"]]]), ("COUNT(*) FROM", [[["46"]]])])
        out, rc = prereq_gate(ga_all, g)
        ok_all &= _chk("floor_item" in out["missing"] and "结构性不可达" in out["reasons"].get("floor_item", ""),
                       "A6 地板物永远报缺且给结构性原因", str(out["missing"]))
        ga_mon = argparse.Namespace(**dict(vars(ga), need=["monsters"], map="8"))
        out, rc = prereq_gate(ga_mon, FakeMysql([]))
        ok_all &= _chk(rc == RC_UNDECIDABLE and out["missing"] == ["monsters"],
                       "A6 profile 该图无靶子 ⇒ 报缺 monsters")

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

    # =====================================================================
    # 真机流程三缺口的牙（卡 `openmir2-mirflow-realmachine-flow-runnable` A4）
    # 每颗牙独立：拆掉对应实现 ⇒ 只有它红。
    # =====================================================================
    import shutil
    import tempfile
    t2 = tempfile.mkdtemp(prefix="prov-gaps-")
    try:
        env = os.path.join(t2, "Envir")
        os.makedirs(env)
        mongen = os.path.join(env, "MonGen.txt")
        original = "3\t320\t328\t鸡\t0\t1\t10\n".encode("gbk")
        with open(mongen, "wb") as fh:
            fh.write(original)
        with open(os.path.join(env, "MapInfo.txt"), "wb") as fh:
            fh.write("[3 盟重省] FIGHT\n[0103 铁器店] FIGHT\n".encode("gbk"))

        # ---- P1 牙①：前插落在**头部**、留下 backup、before/after sha 都记
        spec = [{"map": "3", "x": 331, "y": 331, "name": "鸡", "count": 1}]
        dry = prepend_mongen(env, spec, dry_run=True)
        ok_dry = (dry["action"] == "dry-run" and dry["lines"]
                  and _sha256_file(mongen) == dry["before_sha256"])
        ok_all &= _chk(ok_dry, "P1 牙① dry-run 不写文件、但给出将要前插的行",
                       dry["lines"])
        res = prepend_mongen(env, spec, dry_run=False)
        head = open(mongen, "rb").read().split(b"\n")[0].decode("gbk")
        ok_all &= _chk(res["action"] == "prepended" and head.startswith("3\t331\t331\t鸡")
                       and res["backup"] and os.path.isfile(res["backup"])
                       and res["after_sha256"] != res["before_sha256"],
                       "P1 牙② 前插到 MonGen 头部 + backup + 前后 sha 可区分",
                       head)
        # ---- P1 牙③：还原回写前字节（sha 相等）并删掉 backup
        err = restore_mongen(res)
        ok_all &= _chk(err is None and _sha256_file(mongen) == res["before_sha256"]
                       and not os.path.exists(res["backup"]),
                       "P1 牙③ 还原后 sha256 == 写前、backup 已清", err)

        # ---- P1 牙④：生产根 fail-closed —— 用**假靶**（把 PROD_ENVIR_ROOTS 指到临时根）
        saved_roots = globals()["PROD_ENVIR_ROOTS"]
        globals()["PROD_ENVIR_ROOTS"] = (env,)
        try:
            try:
                prepend_mongen(env, spec, dry_run=False)
                refused = False
            except EnvirRefused:
                refused = True
        finally:
            globals()["PROD_ENVIR_ROOTS"] = saved_roots
        ok_all &= _chk(refused and _sha256_file(mongen) == res["before_sha256"],
                       "P1 牙④ 「生产根」fail-closed 且一个字节都没写", refused)

        # ---- P1 牙⑤：spawned / spawn_failed / undecidable 三态可区分
        frames = os.path.join(t2, "frames.ndjson")
        with open(frames, "w", encoding="utf-8") as fh:
            fh.write(json.dumps({"seq": 9, "t_ms": 1, "ident": 41,
                                 "param": 331, "tag": 331,
                                 "body_hex": "鸡".encode("gbk").hex()}) + "\n")
        sc = spawn_check(frames, "鸡", 331, 331)
        sc_bad = spawn_check(frames, "鸡", 340, 340)
        sc_none = spawn_check(os.path.join(t2, "nope.ndjson"), "鸡", 331, 331)
        ok_all &= _chk(sc["verdict"] == "spawned" and sc_bad["verdict"] == "spawn_failed"
                       and sc_none["verdict"] == "undecidable",
                       "P1 牙⑤ 刷出证明三态可区分（spawned/spawn_failed/undecidable）",
                       (sc["verdict"], sc_bad["verdict"], sc_none["verdict"]))

        # ---- P2 牙⑥：合法剖面 pass；监听落在默认段 / 客户端连错口 ⇒ fail（fail-closed 不放宽）
        good = {"proxy": {"listen": 18600, "upstream_host": "127.0.0.1",
                          "upstream_port": 7200},
                "client": {"gate_port": 18600}}
        bad_port = {"proxy": dict(good["proxy"], listen=5000),
                    "client": {"gate_port": 5000}}
        bad_gate = {"proxy": good["proxy"], "client": {"gate_port": 7200}}
        r_good = check_proxy_profile(good)
        r_port = check_proxy_profile(bad_port)
        r_gate = check_proxy_profile(bad_gate)
        ok_all &= _chk(r_good["verdict"] == "pass" and r_port["verdict"] == "fail"
                       and r_gate["verdict"] == "fail"
                       and any("5000–7200" in p for p in r_port["problems"]),
                       "P2 牙⑥ proxy 监听默认段 5000–7200 ⇒ fail（不放开 fail-closed）",
                       r_port["problems"])

        # ---- P2 牙⑥b（复审 F1）：listen == upstream_port ⇒ fail（自环上游）。
        #      删掉那条判定（改成 `if False:`）⇒ 只有本牙红。
        bad_self = {"proxy": {"listen": 18600, "upstream_host": "127.0.0.1",
                              "upstream_port": 18600},
                    "client": {"gate_port": 18600}}
        r_self = check_proxy_profile(bad_self)
        ok_all &= _chk(r_self["verdict"] == "fail"
                       and any("相同" in p for p in r_self["problems"]),
                       "P2 牙⑥b listen == upstream_port ⇒ fail（自环上游）",
                       r_self["problems"])

        # ---- P1 牙②b（复审 F2）：换行**跟随原文件**——CRLF 源前插 CRLF、LF 源前插 LF，
        #      且原文件字节原样留在尾部。固定 LF 的写法这条必红。
        def _mk_env(tag, eol):
            d = os.path.join(t2, tag)
            os.makedirs(d)
            with open(os.path.join(d, "MonGen.txt"), "wb") as fh:
                fh.write(("3\t320\t328\t"
                          "\u9e21").encode("gbk") + eol.encode())
            with open(os.path.join(d, "MapInfo.txt"), "wb") as fh:
                fh.write("[3 盟重省] FIGHT\n".encode("gbk"))
            return d

        env_lf = _mk_env("EnvirLF", "\n")
        env_crlf = _mk_env("EnvirCRLF", "\r\n")
        orig_crlf = open(os.path.join(env_crlf, "MonGen.txt"), "rb").read()
        res_lf = prepend_mongen(env_lf, spec, dry_run=False)
        res_crlf = prepend_mongen(env_crlf, spec, dry_run=False)
        raw_lf = open(os.path.join(env_lf, "MonGen.txt"), "rb").read()
        raw_crlf = open(os.path.join(env_crlf, "MonGen.txt"), "rb").read()
        ok_eol = (res_lf.get("eol") == "LF" and b"\r\n" not in raw_lf
                  and res_crlf.get("eol") == "CRLF"
                  and raw_crlf.count(b"\n") == raw_crlf.count(b"\r\n")
                  and raw_crlf.endswith(orig_crlf)
                  and raw_lf.endswith(open(os.path.join(env_lf, "MonGen.txt"
                                                        ".pre-fixture.bak"), "rb").read()))
        ok_all &= _chk(ok_eol, "P1 牙②b 换行跟随原文件（CRLF↔CRLF、LF↔LF，原文件字节留在尾部）",
                       (res_lf.get("eol"), res_crlf.get("eol")))

        # ---- P1 牙②c（卡 `openmir2-mirflow-newline-backup-tooth`）：**无尾换行**的夹具。
        #      前插后原文尾部字节必须保留**并补一个 nl**；且与 card_window 的布局机判交叉一致。
        def _mk_env_raw(tag, raw):
            d = os.path.join(t2, tag)
            os.makedirs(d)
            with open(os.path.join(d, "MonGen.txt"), "wb") as fh:
                fh.write(raw)
            with open(os.path.join(d, "MapInfo.txt"), "wb") as fh:
                fh.write("[3 盟重省] FIGHT\n".encode("gbk"))
            return d

        # 交叉判用 card_window 的 `_mongen_fixture_layout`（同口径第二意见）。
        import sys as _sys
        _cw_dir = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                               "conformance")
        if _cw_dir not in _sys.path:
            _sys.path.insert(0, _cw_dir)
        import card_window as _cw

        line1 = "3\t320\t328\t" + "\u9e21" + "\t0\t1\t10"
        line2 = "4\t0\t0\t" + "\u9e7f" + "\t0\t1\t10"
        # 两形态矩阵（复审 N1）：**末行不带换行**的 LF 源 与 **CRLF 源**。
        noeol_cases = [
            ("EnvirNoEOL", line1.encode("gbk"), b"\n", "LF"),
            ("EnvirCRLFNoEOL",
             (line1 + "\r\n" + line2).encode("gbk"), b"\r\n", "CRLF"),
        ]
        ok_matrix = True
        detail_matrix = []
        for tag, raw_before, want_nl, want_eol in noeol_cases:
            d = _mk_env_raw(tag, raw_before)
            res_c = prepend_mongen(d, spec, dry_run=False)
            with open(os.path.join(d, "MonGen.txt"), "rb") as fh:
                raw_after = fh.read()
            tail_ok = (raw_before and not raw_before.endswith(b"\n")
                       and raw_after.endswith(raw_before + want_nl)
                       and res_c.get("eol") == want_eol
                       # 复审 N1：按形态分叉——CRLF 源判「无混排」(`\n` 数 == `\r\n` 数)，
                       # LF 源判「不出现 CRLF」（旧写法 `count(\n)==count(want_nl)` 在
                       # `want_nl == b"\n"` 时是同义反复，永远为真）。
                       and ((raw_after.count(b"\n") == raw_after.count(b"\r\n"))
                            if want_nl == b"\r\n" else (b"\r\n" not in raw_after)))
            lay_ok, why = _cw._mongen_fixture_layout(raw_before, raw_after, res_c["lines"])
            ok_matrix = ok_matrix and tail_ok and lay_ok
            detail_matrix.append((tag, tail_ok, lay_ok, why))
        ok_all &= _chk(ok_matrix,
                       "P1 牙②c 无尾换行两形态（LF / CRLF）：尾部字节保留 + 补的换行跟随原文件"
                       " + 无混排 + card_window 布局机判一致",
                       detail_matrix)

        # ---- P1 牙②e（复审 C1）：**备份缺失** ⇒ restore 干净报错、目标不动、不抛异常。
        #      删掉那条守卫会掉进 `_sha256_file(backup)` 的 `FileNotFoundError`（整档崩）。
        miss_env = _mk_env("EnvirMissBackup", "\n")
        res_m = prepend_mongen(miss_env, spec, dry_run=False)
        before_m = _sha256_file(res_m["path"])
        os.unlink(res_m["backup"])
        try:
            err_m = restore_mongen(res_m)
            raised_m = None
        except Exception as ex:      # noqa: BLE001 — 崩溃即判红，不许整档挂掉
            err_m = None
            raised_m = repr(ex)
        after_m = _sha256_file(res_m["path"])
        # 复审 N2：守护是 `if not backup or not os.path.isfile(backup):`，上面只走
        # `not os.path.isfile(...)` 那半；这里再构造 `backup: None` ⇒ 必须同样干净报错
        # （删掉 `not backup or` 会掉进 `os.path.isfile(None)` 的 TypeError）。
        synth = {"action": "prepended", "path": res_m["path"], "backup": None,
                 "before_sha256": res_m["before_sha256"]}
        try:
            err_n = restore_mongen(synth)
            raised_n = None
        except Exception as ex:      # noqa: BLE001
            err_n = None
            raised_n = repr(ex)
        after_n = _sha256_file(synth["path"])
        ok_all &= _chk(raised_m is None and err_m is not None and "备份缺失" in err_m
                       and after_m == before_m
                       and raised_n is None and err_n is not None and "备份缺失" in err_n
                       and after_n == before_m,
                       "P1 牙②e 备份缺失（unlink / backup=None 两种入参）⇒ 返回「备份缺失」"
                       "错误、不抛异常、目标文件零字节变化",
                       (raised_m, err_m, after_m == before_m,
                        raised_n, err_n, after_n == before_m))

        # ---- P1 牙②d：`backup_sha256` 必须被**执行**——篡改备份内容 ⇒ restore 拒绝且目标不动
        tamper_env = _mk_env("EnvirTamper", "\n")
        res_t = prepend_mongen(tamper_env, spec, dry_run=False)
        before_tamper = _sha256_file(res_t["path"])
        with open(res_t["backup"], "ab") as fh:
            fh.write(b"\n#tampered\n")
        err_t = restore_mongen(res_t)
        after_tamper = _sha256_file(res_t["path"])
        ok_all &= _chk(err_t is not None and "备份" in err_t
                       and after_tamper == before_tamper,
                       "P1 牙②d 备份被篡改 ⇒ restore 拒绝（报错）且目标文件一个字节没动",
                       (err_t, after_tamper == before_tamper))

        # ---- P4 牙⑦：两侧快照 ⇒ 金币差/同一 StdIndex/槽消失 三事实 pass
        def _mk_snap(dirp, gold, rows):
            os.makedirs(dirp, exist_ok=True)
            ct = os.path.join(dirp, "mir2_test_db.characters.tsv")
            bt = os.path.join(dirp, "mir2_test_db.characters_bagitem.tsv")
            with open(ct, "w", encoding="utf-8", newline="\n") as fh:
                fh.write("7\taaa\t%d\n" % gold)
            with open(bt, "w", encoding="utf-8", newline="\n") as fh:
                for r in rows:
                    fh.write("7\t%d\t%d\t%d\n" % r)
            man = {"tool": "dbsnap", "tables": [
                {"table": "mir2_test_db.characters", "rows": 1, "sha256": "aa",
                 "file": "mir2_test_db.characters.tsv",
                 "columns": ["Id", "ChrName", "Gold"]},
                {"table": "mir2_test_db.characters_bagitem", "rows": len(rows),
                 "sha256": "bb", "file": "mir2_test_db.characters_bagitem.tsv",
                 "columns": ["PlayerId", "Position", "StdIndex", "MakeIndex"]}]}
            with open(os.path.join(dirp, "manifest.json"), "w", encoding="utf-8",
                      newline="\n") as fh:
                json.dump(man, fh, ensure_ascii=False)
            return dirp

        bag_before = [(40, 207, 900001), (41, 3, 900002)]
        bag_after = [(41, 3, 900002)]
        cb = _mk_snap(os.path.join(t2, "cb"), 5000, bag_before)
        ca = _mk_snap(os.path.join(t2, "ca"), 4500, bag_after)
        rb = _mk_snap(os.path.join(t2, "rb"), 5000, bag_before)
        ra = _mk_snap(os.path.join(t2, "ra"), 4500, bag_after)
        ev = db_evidence(cb, ca, rb, ra, 7, expect_slot=40, expect_gold_delta=-500)
        ok_all &= _chk(ev["verdict"] == "pass" and ev["rc"] == RC_OK
                       and all(f["verdict"] == "pass" for f in ev["facts"].values())
                       and all(v["manifest_sha256"] for v in ev["snapshots"].values()),
                       "P4 牙⑦ 三事实可机判 + 四份快照都带 sha256", ev["facts"])

        # ---- P4 牙⑧：两侧金币差不同 ⇒ gold_delta fail（rc=4，不绿）
        ra_bad = _mk_snap(os.path.join(t2, "ra2"), 4900, bag_after)
        ev2 = db_evidence(cb, ca, rb, ra_bad, 7, expect_slot=40)
        ok_all &= _chk(ev2["facts"]["gold_delta"]["verdict"] == "fail"
                       and ev2["rc"] == RC_MISMATCH,
                       "P4 牙⑧ 两侧金币差不同 ⇒ gold_delta fail", ev2["facts"]["gold_delta"])

        # ---- P4 牙⑨：缺一侧快照 ⇒ 三事实全部 undecidable（不是绿）
        try:      # 异常安全：缺快照分支被删时应该是**命名 FAIL**，不是整档崩
            ev3 = db_evidence(cb, ca, rb, None, 7, expect_slot=40)
        except Exception:  # noqa: BLE001
            ev3 = None
        ok_all &= _chk(bool(ev3) and ev3["verdict"] == "undecidable"
                       and ev3["rc"] == RC_UNDECIDABLE
                       and all(f["verdict"] == "undecidable" for f in ev3["facts"].values()),
                       "P4 牙⑨ 缺快照 ⇒ 三事实判不了（不给绿）",
                       ev3["verdict"] if ev3 else None)

        # ---- P4 牙⑨b（复审 F3）：bagitem 快照缺 `StdIndex` 列 ⇒ 两条事实 undecidable
        #      且点名缺列（旧实现抛 ValueError ⇒ 整档崩）。
        bad_dir = os.path.join(t2, "bad_cols")
        os.makedirs(bad_dir)
        with open(os.path.join(bad_dir, "mir2_test_db.characters.tsv"), "w",
                  encoding="utf-8", newline="\n") as fh:
            fh.write("7\taaa\t4500\n")
        with open(os.path.join(bad_dir, "mir2_test_db.characters_bagitem.tsv"), "w",
                  encoding="utf-8", newline="\n") as fh:
            fh.write("7\t41\t900002\n")
        with open(os.path.join(bad_dir, "manifest.json"), "w", encoding="utf-8",
                  newline="\n") as fh:
            json.dump({"tool": "dbsnap", "tables": [
                {"table": "mir2_test_db.characters", "rows": 1, "sha256": "aa",
                 "file": "mir2_test_db.characters.tsv",
                 "columns": ["Id", "ChrName", "Gold"]},
                {"table": "mir2_test_db.characters_bagitem", "rows": 1, "sha256": "bb",
                 "file": "mir2_test_db.characters_bagitem.tsv",
                 "columns": ["PlayerId", "Position", "MakeIndex"]}]}, fh,
                ensure_ascii=False)
        try:
            ev4 = db_evidence(cb, ca, rb, bad_dir, 7, expect_slot=40)
        except Exception as ex:   # noqa: BLE001 — 缺列必须判 undecidable，不许崩
            ev4 = None
            ok_all &= _chk(False, "P4 牙⑨b 缺列不得抛异常", repr(ex))
        if ev4 is not None:
            ok_all &= _chk(ev4["facts"]["same_std_index"]["verdict"] == "undecidable"
                           and "StdIndex" in ev4["facts"]["same_std_index"]["detail"]
                           and ev4["rc"] == RC_UNDECIDABLE,
                           "P4 牙⑨b bagitem 缺 StdIndex 列 ⇒ undecidable 且点名缺列",
                           ev4["facts"]["same_std_index"])

        # ---- 漂移牙⑩：本文件的 MonGen 布局常量必须与 card_window 一致
        import re as _re
        cw = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                          "conformance", "card_window.py")
        m = _re.search(r"^FIXTURE_MONGEN_ZEN_TIME = (\d+)$", open(cw, encoding="utf-8").read(),
                       _re.M)
        ok_all &= _chk(m and int(m.group(1)) == FIXTURE_MONGEN_ZEN_TIME,
                       "漂移牙⑩ FIXTURE_MONGEN_ZEN_TIME 与 card_window 一致",
                       m.group(1) if m else "未找到")
    finally:
        shutil.rmtree(t2, ignore_errors=True)

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


# ===========================================================================
# 真机流程三缺口（卡 `openmir2-mirflow-realmachine-flow-runnable`）
#   P1 夹具前插 MonGen（刷出证明与失败可区分）
#   P2 proxy 端口剖面（不得占默认 5000–7200；-ProxyDir 仍是唯一 fail-closed 证据源）
#   P4 两侧 dbsnap 证据（金币差 / 同一 StdIndex / 槽消失，可机判）
# 离线实现 + 单测；真机验证另列 window_required。
# ===========================================================================

# 与 `card_window.py` 的 `FIXTURE_MONGEN_ZEN_TIME` **同值**（自检里做漂移牙：两边不一致 ⇒ FAIL）。
FIXTURE_MONGEN_ZEN_TIME = 360
PROD_ENVIR_ROOTS = (r"E:\MirServer", "E:/MirServer")
# 默认部署占用的端口段（proxy **不得监听**在这里；作为客户端连出去不受限）
DEFAULT_PORT_RANGE = (5000, 7200)


class EnvirRefused(Exception):
    """拒绝写生产 Envir 根（内容冻结红线）。"""


def _is_prod_envir(root):
    real = os.path.normcase(os.path.abspath(root))
    return any(real.startswith(os.path.normcase(os.path.abspath(p)))
               for p in PROD_ENVIR_ROOTS)


def _sha256_file(path):
    import hashlib
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def prepend_mongen(envir_root, specs, dry_run=True):
    """把怪物行**前插**到 `<envir_root>/MonGen.txt` 头部（与 card_window 同布局）。

    ``specs`` = [{"map","x","y","name","count"}]。返回证据 dict：
      ``action`` ∈ {``dry-run``, ``prepended``}；``before_sha256`` / ``after_sha256``；
      ``backup``（原文件副本，还原用）；``lines``（实际前插的行）。
    失败 fail-closed：生产根 ⇒ ``EnvirRefused``；地图不在 MapInfo.txt ⇒ ``Prereq``；
    MonGen.txt 缺失 ⇒ ``Prereq``。
    """
    if _is_prod_envir(envir_root):
        raise EnvirRefused("拒绝写生产 Envir 根（内容冻结）：%s" % envir_root)
    mongen = os.path.join(envir_root, "MonGen.txt")
    mapinfo = os.path.join(envir_root, "MapInfo.txt")
    if not os.path.isfile(mongen):
        raise Prereq("MonGen.txt 不存在：%s" % mongen)
    res = {"envir_root": envir_root, "path": mongen, "action": "dry-run",
           "before_sha256": None, "after_sha256": None, "backup": None,
           "lines": [], "rc": RC_OK, "error": None}
    res["before_sha256"] = _sha256_file(mongen)
    maps = set()
    if os.path.isfile(mapinfo):
        with open(mapinfo, "rb") as fh:
            for raw in fh:
                line = raw.decode("gbk", "replace").strip()
                if not line or line.startswith(";") or not line.startswith("["):
                    continue
                maps.add(line[1:].split()[0].strip())
    lines = []
    for spec in specs:
        if spec["map"] not in maps:
            raise Prereq("fixtures 指定的地图 %r 不在 MapInfo.txt（fail-closed）" % spec["map"])
        lines.append("\t".join([spec["map"], str(spec["x"]), str(spec["y"]),
                                spec["name"], "0", str(spec.get("count", 1)),
                                str(FIXTURE_MONGEN_ZEN_TIME)]))
    res["lines"] = lines
    if dry_run:
        return res
    with open(mongen, "rb") as fh:
        original = fh.read()
    # 复审 F2：换行**跟随原文件**（与 `card_window.py` 同口径：含 CRLF ⇒ 用 CRLF）。
    # 固定 LF 会让 CRLF 部署件出现混排，也与 card_window 的布局机判不等价。
    nl = b"\r\n" if b"\r\n" in original else b"\n"
    body = original if (not original or original.endswith(b"\n")) else original + nl
    block = b"".join(ln.encode("gbk") + nl for ln in lines)
    backup = mongen + ".pre-fixture.bak"
    with open(backup, "wb") as fh:
        fh.write(original)
    with open(mongen, "wb") as fh:
        fh.write(block + body)
    res["eol"] = "CRLF" if nl == b"\r\n" else "LF"
    res["backup_sha256"] = res["before_sha256"]
    res["backup"] = backup
    res["after_sha256"] = _sha256_file(mongen)
    res["action"] = "prepended"
    return res


def restore_mongen(res):
    """按证据把 MonGen.txt 还原成写前字节，并核对 sha256。

    卡 `openmir2-mirflow-newline-backup-tooth`：还原**之前**先核备份本身的 sha256
    （`backup_sha256`，缺省回落到 `before_sha256`）——备份被改动时**拒绝还原**并报错，
    绝不把坏字节写回目标文件。
    """
    if not res or res.get("action") != "prepended":
        return None
    backup = res.get("backup")
    if not backup or not os.path.isfile(backup):
        return "备份缺失：%r（拒绝还原）" % (backup,)
    want = res.get("backup_sha256") or res.get("before_sha256")
    got_backup = _sha256_file(backup)
    if got_backup != want:
        return ("备份 sha256=%s != 期望 %s（备份已被改动，拒绝还原；目标文件未动）"
                % (got_backup, want))
    with open(res["backup"], "rb") as fh:
        original = fh.read()
    with open(res["path"], "wb") as fh:
        fh.write(original)
    got = _sha256_file(res["path"])
    if got != res["before_sha256"]:
        return "还原后 sha256=%s != 写前 %s" % (got, res["before_sha256"])
    os.unlink(res["backup"])
    return None


def spawn_check(frames_path, name, x, y):
    """从 `frames.ndjson` 判「靶子刷出」——三种结论**可区分**（A4）。

    * 无 frames / 文件不存在 ⇒ ``undecidable``（**不是**绿）；
    * 有 frames 但(`x`,`y`)格从未出现该名字的 actor ⇒ ``spawn_failed``；
    * 出现 ⇒ ``spawned``，并记录首见 seq/t_ms。
    名字来自 oracle 帧体的 GBK 文本；本检查只按 `body_text_contains` 的等价物做**宽松**匹配：
    帧里 `body_hex` 解出的 GBK 文本含名字即算出现。
    """
    out = {"frames": frames_path, "name": name, "x": int(x), "y": int(y),
           "verdict": "undecidable", "detail": "", "first_seq": None, "first_t_ms": None}
    if not frames_path or not os.path.isfile(frames_path):
        out["detail"] = "没有 frames.ndjson（证据缺失 ⇒ 判不了）"
        return out
    want = name.encode("gbk", "replace")
    seen_any = False
    with open(frames_path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            seen_any = True
            try:
                fr = json.loads(line)
            except ValueError:
                out["detail"] = "frames.ndjson 有坏行（判不了）"
                out["verdict"] = "undecidable"
                return out
            body = fr.get("body_hex") or ""
            try:
                raw = bytes.fromhex(body) if body else b""
            except ValueError:
                raw = b""
            named = want in raw
            pos_ok = (fr.get("param") == int(x) and fr.get("tag") == int(y))
            if named and pos_ok:
                out.update({"verdict": "spawned", "first_seq": fr.get("seq"),
                            "first_t_ms": fr.get("t_ms"), "detail": "靶子出现在约定格"})
                return out
    out["verdict"] = "spawn_failed"
    out["detail"] = ("有 %d 帧，但 (%d,%d) 上没有名为 %r 的 actor"
                     % (1 if seen_any else 0, int(x), int(y), name)) if seen_any else "frames 为空（判不了）"
    if not seen_any:
        out["verdict"] = "undecidable"
    return out


def _load_snap(dirpath):
    """读 dbsnap 目录：返回 {"manifest":…, "sha256":…, "tables": {fq: (columns, rows)}}。"""
    man_path = os.path.join(dirpath, "manifest.json")
    if not os.path.isfile(man_path):
        return None
    with open(man_path, encoding="utf-8") as fh:
        man = json.load(fh)
    tables = {}
    for t in man.get("tables") or []:
        fq = t.get("table")
        fpath = os.path.join(dirpath, t.get("file") or "")
        rows = []
        if os.path.isfile(fpath):
            with open(fpath, encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    line = line.rstrip("\n")
                    if line == "":
                        continue
                    rows.append(line.split("\t"))
        tables[fq] = (t.get("columns") or [], rows)
    return {"manifest": man, "sha256": _sha256_file(man_path),
            "path": os.path.abspath(dirpath), "tables": tables}


def _cell(tables, fq, row, column):
    cols, rows = tables[fq]
    if column not in cols:
        raise Prereq("快照表 %s 缺列 %r" % (fq, column))
    return rows[row][cols.index(column)]


def _char_row(tables, fq, player_id):
    cols, rows = tables[fq]
    if "Id" not in cols:
        raise Prereq("快照表 %s 缺列 Id" % fq)
    idx = cols.index("Id")
    for i, r in enumerate(rows):
        if r[idx] == str(player_id):
            return i
    return None


def db_evidence(csharp_before, csharp_after, rust_before, rust_after,
                player_id, expect_slot=None, expect_gold_delta=None):
    """两侧 dbsnap → 三条**可机判**事实（A3）：金币差 / 同一 StdIndex / 槽消失。

    任一侧快照缺失或读不了 ⇒ 三条都判 ``undecidable``（**不是**绿）。
    """
    out = {"verdict": "undecidable", "facts": {}, "snapshots": {}, "rc": RC_UNDECIDABLE}
    snaps = {}
    for key, d in (("csharp_before", csharp_before), ("csharp_after", csharp_after),
                   ("rust_before", rust_before), ("rust_after", rust_after)):
        loaded = _load_snap(d) if d else None
        snaps[key] = loaded
        out["snapshots"][key] = ({"path": loaded["path"], "manifest_sha256": loaded["sha256"]}
                                 if loaded else {"path": d, "manifest_sha256": None})
    missing = [k for k, v in snaps.items() if v is None]
    if missing:
        for f in ("gold_delta", "same_std_index", "slot_disappeared"):
            out["facts"][f] = {"verdict": "undecidable",
                               "detail": "缺快照 %s（判不了）" % ",".join(missing)}
        return out

    ct = "mir2_test_db.characters"

    def gold(snap):
        row = _char_row(snap["tables"], ct, player_id)
        if row is None:
            raise Prereq("快照里找不到 Id=%s 的角色行" % player_id)
        return int(_cell(snap["tables"], ct, row, "Gold"))

    try:
        deltas = {"csharp": gold(snaps["csharp_after"]) - gold(snaps["csharp_before"]),
                  "rust": gold(snaps["rust_after"]) - gold(snaps["rust_before"])}
    except (Prereq, ValueError, IndexError) as ex:
        out["facts"]["gold_delta"] = {"verdict": "undecidable", "detail": str(ex)}
        deltas = None
    if deltas is not None:
        ok = deltas["csharp"] == deltas["rust"]
        if expect_gold_delta is not None:
            ok = ok and deltas["csharp"] == int(expect_gold_delta)
        out["facts"]["gold_delta"] = {
            "verdict": "pass" if ok else "fail",
            "detail": "两侧金币差 csharp=%d rust=%d%s"
                      % (deltas["csharp"], deltas["rust"],
                         "" if expect_gold_delta is None else " expect=%d" % expect_gold_delta)}

    bt = "mir2_test_db.characters_bagitem"

    def slots(snap):
        if bt not in snap["tables"]:
            raise Prereq("快照里没有表 %s（判不了）" % bt)
        cols, rows = snap["tables"][bt]
        for need in ("PlayerId", "Position", "StdIndex"):
            if need not in cols:
                raise Prereq("bagitem 快照缺列 %r（判不了）" % need)
        pid = cols.index("PlayerId")
        pos = cols.index("Position")
        std = cols.index("StdIndex")
        return {(r[pos], r[std]) for r in rows if r[pid] == str(player_id)}

    try:
        gone = {"csharp": slots(snaps["csharp_before"]) - slots(snaps["csharp_after"]),
                "rust": slots(snaps["rust_before"]) - slots(snaps["rust_after"])}
    except Prereq as ex:
        # 复审 F3：缺列/缺表是**判不了**（undecidable），不是崩；点名缺哪一列。
        for f in ("same_std_index", "slot_disappeared"):
            out["facts"][f] = {"verdict": "undecidable", "detail": str(ex)}
        gone = None
    if gone is None:
        gold_fact = out["facts"].get("gold_delta") or {"verdict": "undecidable",
                                                       "detail": "其它事实判不了"}
        out["verdict"] = "fail" if gold_fact["verdict"] == "fail" else "undecidable"
        out["rc"] = RC_MISMATCH if gold_fact["verdict"] == "fail" else RC_UNDECIDABLE
        return out
    stds = {"csharp": sorted({s for _p, s in gone["csharp"] if s != "0"}),
            "rust": sorted({s for _p, s in gone["rust"] if s != "0"})}
    ok_std = bool(stds["csharp"]) and stds["csharp"] == stds["rust"]
    out["facts"]["same_std_index"] = {
        "verdict": "pass" if ok_std else "fail",
        "detail": "消失行的 StdIndex csharp=%s rust=%s" % (stds["csharp"], stds["rust"])}
    if expect_slot is not None:
        slot = str(expect_slot)
        gone_pos = {"csharp": {p for p, _s in gone["csharp"]},
                    "rust": {p for p, _s in gone["rust"]}}
        ok_slot = slot in gone_pos["csharp"] and slot in gone_pos["rust"]
        out["facts"]["slot_disappeared"] = {
            "verdict": "pass" if ok_slot else "fail",
            "detail": "槽 %s 消失：csharp=%s rust=%s"
                      % (slot, slot in gone_pos["csharp"], slot in gone_pos["rust"])}
    else:
        out["facts"]["slot_disappeared"] = {"verdict": "undecidable",
                                            "detail": "未给 --expect-slot（判不了）"}

    verdicts = {f["verdict"] for f in out["facts"].values()}
    if verdicts == {"pass"}:
        out.update({"verdict": "pass", "rc": RC_OK})
    elif "fail" in verdicts:
        out.update({"verdict": "fail", "rc": RC_MISMATCH})
    return out


def check_proxy_profile(data):
    """proxy 端口剖面校验（P2）：**proxy 不得监听默认 5000–7200**，fail-closed。"""
    problems = []
    if not isinstance(data, dict):
        return {"verdict": "fail", "problems": ["profile 不是对象"], "rc": RC_PREREQ}
    proxy = data.get("proxy") or {}
    client = data.get("client") or {}

    def port(v, label):
        if not isinstance(v, int) or isinstance(v, bool) or not (1 <= v <= 65535):
            problems.append("%s 不是合法端口：%r" % (label, v))
            return None
        return v

    listen = port(proxy.get("listen"), "proxy.listen")
    upstream = port(proxy.get("upstream_port"), "proxy.upstream_port")
    gate_port = port(client.get("gate_port"), "client.gate_port")
    host = proxy.get("upstream_host")
    if not isinstance(host, str) or not host.strip():
        problems.append("proxy.upstream_host 必须是非空字符串")
    if listen is not None and DEFAULT_PORT_RANGE[0] <= listen <= DEFAULT_PORT_RANGE[1]:
        problems.append("proxy.listen=%d 落在默认端口段 %d–%d（proxy 不得占用；"
                        "默认段留给 oracle/rust 自身监听）"
                        % (listen, DEFAULT_PORT_RANGE[0], DEFAULT_PORT_RANGE[1]))
    if listen is not None and upstream == listen:
        problems.append("proxy.listen 与 upstream_port 相同（%d）：会把上游打回自己" % listen)
    if listen is not None and gate_port is not None and gate_port != listen:
        problems.append("client.gate_port=%d ≠ proxy.listen=%d（客户端要连 proxy）"
                        % (gate_port, listen))
    return {"verdict": "pass" if not problems else "fail", "problems": problems,
            "rc": RC_OK if not problems else RC_PREREQ,
            "ports": {"listen": listen, "upstream": upstream, "gate_port": gate_port}}


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
    mode.add_argument("--gate", action="store_true",
                      help="A6 动作前的前置门：备齐=PASS；缺项=UNDECIDABLE(kind=missing_prereq) rc=6")
    mode.add_argument("--prepend-mongen", action="store_true", help="P1: prepend monsters into MonGen.txt")
    mode.add_argument("--spawn-check", action="store_true", help="P1: spawn proof from frames")
    mode.add_argument("--db-evidence", action="store_true", help="P4: two-side dbsnap facts")
    mode.add_argument("--proxy-profile-check", action="store_true", help="P2: proxy port profile")
    mode.add_argument("--selftest", action="store_true")

    ap.add_argument("--db", default="mir2_test_db_r")
    ap.add_argument("--frames", default=None)
    ap.add_argument("--expect-gold", type=int, default=None)
    ap.add_argument("--expect-rows", type=int, default=None)
    ap.add_argument("--need", action="append", default=[],
                    choices=["gold", "sellable_item", "npc", "floor_item", "monsters"],
                    help="--gate 的检查项，可多次；缺省全查")
    ap.add_argument("--min-gold", type=int, default=500, help="--gate 的金币下限（默认 500）")
    ap.add_argument("--profile", default=None, help="--gate 查 monsters 用的 card-window profile")
    ap.add_argument("--report", default=None, help="apply 报告输出路径")
    ap.add_argument("--mon-map", default=None, help="--prepend-mongen 的地图号")
    ap.add_argument("--mon-x", type=int, default=None)
    ap.add_argument("--mon-y", type=int, default=None)
    ap.add_argument("--mon-name", default=None, help="--prepend-mongen 的怪物名（GBK 落地）")
    ap.add_argument("--mon-count", type=int, default=1)
    ap.add_argument("--player-id", type=int, default=None, help="--db-evidence 的角色 Id")
    ap.add_argument("--expect-slot", type=int, default=None)
    ap.add_argument("--expect-gold-delta", type=int, default=None)
    ap.add_argument("--csharp-before", default=None)
    ap.add_argument("--csharp-after", default=None)
    ap.add_argument("--rust-before", default=None)
    ap.add_argument("--rust-after", default=None)
    ap.add_argument("--proxy-profile", default=None, help="--proxy-profile-check 的 JSON 路径")
    ap.add_argument("--yes-apply", action="store_true")
    ap.add_argument("--yes", action="store_true", help="rollback 真写")
    add_db_args(ap)
    args = ap.parse_args(argv)

    try:
        if args.selftest:
            return selftest(args)

        if args.prepend_mongen:
            for need in ("mon_map", "mon_x", "mon_y", "mon_name"):
                if getattr(args, need) in (None, ""):
                    raise Prereq("--prepend-mongen 需要 --mon-map/--mon-x/--mon-y/--mon-name")
            res = prepend_mongen(args.envir_root,
                                 [{"map": args.mon_map, "x": args.mon_x, "y": args.mon_y,
                                   "name": args.mon_name, "count": args.mon_count}],
                                 dry_run=not args.yes_apply)
            if args.frames:
                res["spawn_check"] = spawn_check(args.frames, args.mon_name,
                                                 args.mon_x, args.mon_y)
            print(json.dumps(res, ensure_ascii=False, indent=1))
            return res["rc"]

        if args.spawn_check:
            if not args.frames or args.mon_name is None:
                raise Prereq("--spawn-check 需要 --frames 与 --mon-name（以及 --mon-x/--mon-y）")
            out = spawn_check(args.frames, args.mon_name,
                              args.mon_x if args.mon_x is not None else 0,
                              args.mon_y if args.mon_y is not None else 0)
            print(json.dumps(out, ensure_ascii=False, indent=1))
            return {"spawned": RC_OK, "spawn_failed": RC_PREREQ}.get(out["verdict"],
                                                                     RC_UNDECIDABLE)

        if args.db_evidence:
            if args.player_id is None:
                raise Prereq("--db-evidence 需要 --player-id")
            out = db_evidence(args.csharp_before, args.csharp_after,
                              args.rust_before, args.rust_after, args.player_id,
                              expect_slot=args.expect_slot,
                              expect_gold_delta=args.expect_gold_delta)
            print(json.dumps(out, ensure_ascii=False, indent=1))
            return out["rc"]

        if args.proxy_profile_check:
            if not args.proxy_profile:
                raise Prereq("--proxy-profile-check 需要 --proxy-profile <json>")
            with open(args.proxy_profile, encoding="utf-8") as fh:
                data = json.load(fh)
            out = check_proxy_profile(data)
            print(json.dumps(out, ensure_ascii=False, indent=1))
            return out["rc"]

        if args.a2_evidence:
            ev = a2_evidence(args.repo_root)
            print(json.dumps(ev, ensure_ascii=False, indent=1))
            return RC_OK if ev["verdict"] == "BLOCKED" else RC_PREREQ

        if args.gate:
            guard_db(args.db)
            out, rc = prereq_gate(args, build_mysql(args))
            print(json.dumps(out, ensure_ascii=False, indent=1))
            return rc

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
