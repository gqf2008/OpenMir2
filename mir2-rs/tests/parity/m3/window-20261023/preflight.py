#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""preflight.py — 2026-10-23 真机窗口「开窗前一条命令」前置门（只读探针 + 离线逻辑）。

来源（逐条可追）：
  * 九游戏端口 DOWN / 7600 DOWN / 3306 UP
      `docs/window-round2-runbook-20261023.md` §1.1、§D.0（run sheet v3）
  * 镜像 provenance（构建源 == 出处清单，六件 exe 逐字节对账）
      同 runbook §1.1 step 4、§0.4；语义同 `check_rust_window_preflight.py` R6/R7，
      本工具只做**开窗前的一眼**，完整 R1..R8 仍由 `client-e2e-window.fixed.sh.txt guard` 判。
  * 测试库存在且已从生产克隆（三库表集合相同 + 关键表行数相同）
      runbook §4.1.1（克隆）/ §4.1.2（断言：11/13/7 与 `1 1 1`）

设计口径：
  * 只读：不 bind 端口（DOWN 判定用「能否 bind」反证），不写库（只跑 SELECT），不起任何进程。
  * fail-closed：
      - 缺外部件（git / mysql.exe / provenance 文件）          => PREREQ_RED (rc=2)
      - 状态不符（端口在听 / 清单与源不符 / 测试库与生产不符） => RED       (rc=1)
      - 全绿                                                   => GREEN     (rc=0)
  * `--selftest` 完全离线（合成探针 + 合成 mysql 应答），不需要 MySQL / git / 端口。

用法：
  python -I -B preflight.py --win E:/tmp/window/20261023
  python -I -B preflight.py --win E:/tmp/window/20261023 --out E:/tmp/window/20261023/preflight/preflight.json
  python -I -B preflight.py --selftest
  # 窗口当天重新解析基线后把它钉死（可选；不钉只打印 INFO）：
  python -I -B preflight.py --win ... --expect-source-head 23748419d

环境依赖（窗口机）：Windows PowerShell 可用（端口探针用 .NET 监听表更准）；无 PowerShell 时
退回 socket bind 反证并打 INFO。git / mysql 走 PATH 或显式参数。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import socket
import subprocess
import sys
from datetime import datetime

# ---------------------------------------------------------------------------
# 常量（与 runbook / check_rust_window_preflight.py 同口径）
# ---------------------------------------------------------------------------
GAME_PORTS = (5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200)
SHADOW_PORT = 7600
MYSQL_PORT = 3306
WINDOW_BINS = ("game-svr.exe", "game-gate.exe", "login-srv.exe",
               "login-gate.exe", "sel-gate.exe", "db-srv.exe")
PROVENANCE_VERSION = 1

TEST_DBS = ("mir2_test_db", "mir2_test_account", "mir2_test_data")
PROD_DBS = ("mir2_db", "mir2_account", "mir2_data")
PAIRS = tuple(zip(PROD_DBS, TEST_DBS))
EXPECT_TABLES = {"mir2_test_db": 11, "mir2_test_account": 13, "mir2_test_data": 7}
# §4.1.2 的关键表行数口径（生产 == 测试，且 > 0）
KEY_TABLES = ("mir2_db.characters", "mir2_account.account", "mir2_data.stditems")

DEFAULT_MYSQL = r"D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe"
DEFAULT_MIRROR = "E:/tmp/m2svr-window-origin"
DEFAULT_TARGET = "E:/tmp/m2svr-window-target"
DEFAULT_WIN = "E:/tmp/window/20261023"


# ---------------------------------------------------------------------------
# 探针层（可用 --probe-json 完全替换，供离线自检）
# ---------------------------------------------------------------------------
def listening_ports_dotnet():
    """用 PowerShell/.NET 取 LISTEN 端口集合；失败返回 None（调用方退回 bind 反证）。"""
    ps = (
        "$ErrorActionPreference='Stop';"
        "[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties()"
        ".GetActiveTcpListeners() | ForEach-Object { $_.Port } | Sort-Object -Unique"
    )
    try:
        out = subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-Command", ps],
            capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if out.returncode != 0:
        return None
    ports = set()
    for line in out.stdout.splitlines():
        line = line.strip()
        if line.isdigit():
            ports.add(int(line))
    return ports


def port_is_listening_bind(port):
    """bind 反证：能独占 bind 0.0.0.0:port => 无监听者。"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(1.0)
    try:
        s.bind(("0.0.0.0", port))
        return False
    except OSError:
        return True
    finally:
        s.close()


def probe_ports(override=None):
    """返回 (listen_set, source)；override = 合成 LISTEN 集合（selftest）。"""
    if override is not None:
        return set(override), "injected"
    live = listening_ports_dotnet()
    if live is not None:
        return live, ".NET GetActiveTcpListeners"
    return ({p for p in (GAME_PORTS + (SHADOW_PORT, MYSQL_PORT)) if port_is_listening_bind(p)},
            "socket bind 反证（无 PowerShell）")


# ---------------------------------------------------------------------------
# MySQL 探针（只读 SELECT；可用 --probe-json 的 mysql 段替换）
# ---------------------------------------------------------------------------
def run_mysql(mysql, host, port, user, password, sql, timeout=60):
    args = [mysql, "-N", "-B", "-h", host, "-P", str(port), "-u", user,
            "--password=%s" % password, "--default-character-set=utf8mb4", "-e", sql]
    try:
        out = subprocess.run(args, capture_output=True, text=True, timeout=timeout,
                             encoding="utf-8", errors="replace")
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, "mysql 调用失败：%s" % e
    if out.returncode != 0:
        return None, "mysql rc=%s：%s" % (out.returncode, (out.stderr or "").strip()[:400])
    return out.stdout, None


def collect_db_state(mysql, host, port, user, password, override=None):
    """返回 dict：{schemas:[], tables:{db:[..]}, counts:{'db.tbl':n}, errors:[]}。

    override（selftest）形如：
      {"schemas": ["mir2_db", ...], "tables": {"mir2_db": ["characters", ...]},
       "counts": {"mir2_db.characters": 7338, ...}, "errors": []}
    """
    if override is not None:
        st = dict(override)
        st.setdefault("errors", [])
        return st
    st = {"schemas": [], "tables": {}, "counts": {}, "errors": []}
    q1 = ("SELECT table_schema, table_name FROM information_schema.tables "
          "WHERE table_schema IN (%s) ORDER BY table_schema, table_name;"
          % ",".join("'%s'" % d for d in (PROD_DBS + TEST_DBS)))
    out, err = run_mysql(mysql, host, port, user, password, q1)
    if err:
        st["errors"].append(err)
        return st
    for line in (out or "").splitlines():
        parts = line.split("\t")
        if len(parts) != 2:
            continue
        db, tbl = parts[0].strip(), parts[1].strip()
        if db not in st["tables"]:
            st["tables"][db] = []
            st["schemas"].append(db)
        st["tables"][db].append(tbl)
    st["tables"] = {k: sorted(v) for k, v in st["tables"].items()}
    st["schemas"] = sorted(st["schemas"])
    for t in KEY_TABLES:
        out, err = run_mysql(mysql, host, port, user, password,
                             "SELECT COUNT(*) FROM %s;" % t)
        if err:
            st["errors"].append(err)
            st["counts"][t] = None
        else:
            try:
                st["counts"][t] = int((out or "0").strip().splitlines()[0])
            except (ValueError, IndexError):
                st["counts"][t] = None
    return st


# ---------------------------------------------------------------------------
# git / provenance
# ---------------------------------------------------------------------------
def git_head(repo):
    try:
        out = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"],
                             capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, "git 调用失败：%s" % e
    if out.returncode != 0:
        return None, "git rc=%s：%s" % (out.returncode, (out.stderr or "").strip()[:300])
    return out.stdout.strip(), None


def sha256_file(path):
    try:
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()
    except OSError:
        return None


def norm(p):
    return os.path.normcase(os.path.abspath(p))


# ---------------------------------------------------------------------------
# 判据
# ---------------------------------------------------------------------------
class Result:
    def __init__(self, name):
        self.name = name
        self.status = "GREEN"
        self.detail = []

    def fail(self, msg, prereq=False):
        if prereq and self.status != "RED":
            self.status = "PREREQ_RED"
        elif not prereq:
            self.status = "RED"
        self.detail.append(msg)

    def info(self, msg):
        self.detail.append("INFO " + msg)


def check_ports(res, listen, source):
    up_game = [p for p in GAME_PORTS if p in listen]
    if up_game:
        res.fail("九游戏端口仍有监听：%s（开窗前必须全 DOWN）" % ",".join(map(str, up_game)))
    else:
        res.info("九游戏端口全 DOWN")
    if SHADOW_PORT in listen:
        res.fail("7600 仍在监听（影子 gate 未收干净；port-status.ps1 不含 7600）")
    else:
        res.info("7600 DOWN")
    if MYSQL_PORT not in listen:
        res.fail("3306 未在监听（共享 MySQL 必须 UP）")
    else:
        res.info("3306 UP")
    res.info("探针=%s" % source)


def check_provenance(res, mirror, target_dir, prov_path, expect_source_head=None):
    head, err = git_head(mirror)
    if err:
        res.fail("镜像不可读（%s）：%s" % (mirror, err), prereq=True)
        return
    res.info("mirror HEAD=%s" % head)
    if expect_source_head:
        want = expect_source_head.strip()
        if not (head == want or head.startswith(want)):
            res.fail("镜像 HEAD=%s 与 --expect-source-head=%s 不符" % (head, want))
        else:
            res.info("镜像 HEAD 与 --expect-source-head 相符")
    if not os.path.isfile(prov_path):
        res.fail("缺出处清单 %s（先 cargo build 六件 + --record-provenance）" % prov_path,
                 prereq=True)
        return
    try:
        with open(prov_path, "r", encoding="utf-8") as f:
            prov = json.load(f)
    except (OSError, ValueError) as e:
        res.fail("出处清单解析失败：%s" % e, prereq=True)
        return
    if prov.get("version") != PROVENANCE_VERSION:
        res.fail("出处清单 version=%r，期望 %d" % (prov.get("version"), PROVENANCE_VERSION))
    src = (prov.get("source_head") or "").strip()
    if not src:
        res.fail("出处清单缺 source_head")
    elif src != head:
        res.fail("出处清单 source_head=%s 与镜像 HEAD=%s 不符（测的是旧 exe）" % (src, head))
    else:
        res.info("provenance source_head == 镜像 HEAD")
    pt = prov.get("target_dir")
    if pt and target_dir and norm(pt) != norm(target_dir):
        res.fail("出处清单 target_dir=%s 与 --target-dir=%s 不符" % (pt, target_dir))
    bins = prov.get("bins") or {}
    for name in WINDOW_BINS:
        ent = bins.get(name)
        if not ent or not ent.get("sha256"):
            res.fail("出处清单缺 %s（六件必须齐全）" % name)
            continue
        path = os.path.join(target_dir, "debug", name)
        got = sha256_file(path)
        if got is None:
            res.fail("%s 不存在：%s" % (name, path), prereq=True)
        elif got != ent["sha256"]:
            res.fail("%s sha256 与清单不符（清单=%s 实际=%s）" %
                     (name, ent["sha256"][:16], got[:16]))
        else:
            res.info("%s sha256 OK" % name)


def check_test_db(res, db):
    errors = db.get("errors") or []
    if errors:
        res.fail("MySQL 读取失败：%s" % errors[0], prereq=True)
        return
    schemas = set(db.get("schemas") or [])
    missing_test = [d for d in TEST_DBS if d not in schemas]
    if missing_test:
        res.fail("缺测试库：%s（先按 runbook §4.1.1 克隆）" % ",".join(missing_test))
        return
    missing_prod = [d for d in PROD_DBS if d not in schemas]
    if missing_prod:
        res.fail("缺生产库：%s => 无法证明测试库是克隆" % ",".join(missing_prod), prereq=True)
        return
    for prod, test in PAIRS:
        pt = db["tables"].get(prod) or []
        tt = db["tables"].get(test) or []
        if len(tt) != EXPECT_TABLES[test]:
            res.fail("%s 表数=%d，期望 %d" % (test, len(tt), EXPECT_TABLES[test]))
        if sorted(pt) != sorted(tt):
            only_p = sorted(set(pt) - set(tt))[:6]
            only_t = sorted(set(tt) - set(pt))[:6]
            res.fail("%s 表集合与 %s 不同（仅生产=%s 仅测试=%s）" %
                     (test, prod, only_p, only_t))
        else:
            res.info("%s 表集合 == %s（%d 张）" % (test, prod, len(tt)))
    for t in KEY_TABLES:
        n = db["counts"].get(t)
        prod_db, tbl = t.split(".", 1)
        test_db = dict(PAIRS)[prod_db]
        nt = db["counts"].get("%s.%s" % (test_db, tbl))
        if n is None or nt is None:
            res.fail("关键表行数读不到：%s / %s.%s" % (t, test_db, tbl), prereq=True)
            continue
        if n <= 0:
            res.fail("生产 %s 行数=0，克隆不可信" % t, prereq=True)
        elif n != nt:
            res.fail("%s 行数=%d 与 %s 行数=%d 不等（不是同一次克隆）" %
                     (test_db + "." + tbl, nt, t, n))
        else:
            res.info("%s == %s (%d 行)" % (test_db + "." + tbl, t, n))


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def evaluate(args, port_override=None, db_override=None):
    results = []
    listen, source = probe_ports(port_override)

    r = Result("P1_PORTS")
    check_ports(r, listen, source)
    results.append(r)

    r = Result("P2_MIRROR_PROVENANCE")
    check_provenance(r, args.mirror, args.target_dir, args.provenance, args.expect_source_head)
    results.append(r)

    r = Result("P3_TEST_DB_CLONE")
    db = collect_db_state(args.mysql, args.db_host, args.db_port, args.db_user,
                          args.db_password, db_override)
    check_test_db(r, db)
    results.append(r)
    return results, {"listen": sorted(listen), "listen_source": source, "db": db}


def report(results):
    for r in results:
        print("%s %s" % (r.name, r.status))
        for d in r.detail:
            print("  " + d)
    statuses = [r.status for r in results]
    if "RED" in statuses:
        verdict = "RED"
    elif "PREREQ_RED" in statuses:
        verdict = "PREREQ_RED"
    else:
        verdict = "GREEN"
    print("RESULT: %s" % verdict)
    return verdict, (0 if verdict == "GREEN" else (1 if verdict == "RED" else 2))


def main(argv=None):
    ap = argparse.ArgumentParser(description="2026-10-23 窗口开窗前一条命令前置门（只读）")
    ap.add_argument("--win", default=DEFAULT_WIN, help="窗口工作目录（仅用于默认输出路径）")
    ap.add_argument("--mirror", default=DEFAULT_MIRROR)
    ap.add_argument("--target-dir", default=DEFAULT_TARGET)
    ap.add_argument("--provenance", default="")
    ap.add_argument("--expect-source-head", default="",
                    help="可选：钉死当天基线（前缀匹配）；不给只打印 INFO")
    ap.add_argument("--mysql", default=DEFAULT_MYSQL)
    ap.add_argument("--db-host", default="127.0.0.1")
    ap.add_argument("--db-port", type=int, default=MYSQL_PORT)
    ap.add_argument("--db-user", default="root")
    ap.add_argument("--db-password", default="")
    ap.add_argument("--out", default="", help="证据 JSON；不给只打印")
    ap.add_argument("--probe-json", default="", help="（自检/离线复算）注入合成探针与 mysql 应答")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.provenance:
        args.provenance = os.path.join(args.target_dir, "window-provenance.json")

    port_override = None
    db_override = None
    if args.probe_json:
        try:
            with open(args.probe_json, "r", encoding="utf-8") as f:
                pj = json.load(f)
        except (OSError, ValueError) as e:
            print("PREREQ_RED: --probe-json 解析失败：%s" % e)
            return 2
        port_override = pj.get("listen_ports")
        db_override = pj.get("db")

    results, extra = evaluate(args, port_override, db_override)
    verdict, rc = report(results)
    if args.out:
        os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
        payload = {
            "tool": "window-20261023/preflight.py",
            "stamp": datetime.now().isoformat(timespec="seconds"),
            "mirror": args.mirror,
            "target_dir": args.target_dir,
            "provenance": args.provenance,
            "expect_source_head": args.expect_source_head or None,
            "checks": [{"name": r.name, "status": r.status, "detail": r.detail}
                       for r in results],
            "probe": {"listen": extra["listen"], "source": extra["listen_source"]},
            "result": verdict,
        }
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(payload, f, ensure_ascii=False, indent=1)
        print("REPORT %s" % args.out)
    return rc


# ---------------------------------------------------------------------------
# 离线自检
# ---------------------------------------------------------------------------
def selftest():
    ok = True
    count = [0]

    def expect(name, got, want):
        count[0] += 1
        nonlocal ok
        if got != want:
            ok = False
            print("FAIL %s：got=%r want=%r" % (name, got, want))
        else:
            print("ok   %s" % name)

    def status_of(results, name):
        for r in results:
            if r.name == name:
                return r.status
        return "MISSING"

    base_db = {
        "schemas": list(PROD_DBS + TEST_DBS),
        "tables": {
            "mir2_db": ["characters", "characters_ablity", "characters_item"] + ["t%d" % i for i in range(8)],
            "mir2_test_db": ["characters", "characters_ablity", "characters_item"] + ["t%d" % i for i in range(8)],
            "mir2_account": ["account"] + ["a%d" % i for i in range(12)],
            "mir2_test_account": ["account"] + ["a%d" % i for i in range(12)],
            "mir2_data": ["stditems"] + ["d%d" % i for i in range(6)],
            "mir2_test_data": ["stditems"] + ["d%d" % i for i in range(6)],
        },
        "counts": {"mir2_db.characters": 7338, "mir2_account.account": 7477,
                   "mir2_data.stditems": 1000, "mir2_test_db.characters": 7338,
                   "mir2_test_account.account": 7477, "mir2_test_data.stditems": 1000},
        "errors": [],
    }

    def mk_args(tmpdir, prov=None):
        return argparse.Namespace(
            win=tmpdir, mirror=tmpdir, target_dir=os.path.join(tmpdir, "target"),
            provenance=prov or os.path.join(tmpdir, "target", "window-provenance.json"),
            expect_source_head="", mysql="mysql", db_host="127.0.0.1",
            db_port=3306, db_user="root", db_password="", out="", probe_json="")

    import tempfile

    with tempfile.TemporaryDirectory() as td:
        dbg = os.path.join(td, "target", "debug")
        os.makedirs(dbg)
        os.makedirs(os.path.join(td, "mirror"))
        subprocess.run(["git", "init", "-q", os.path.join(td, "mirror")], check=False)
        subprocess.run(["git", "-C", os.path.join(td, "mirror"), "config", "user.email", "t@t"], check=False)
        subprocess.run(["git", "-C", os.path.join(td, "mirror"), "config", "user.name", "t"], check=False)
        with open(os.path.join(td, "mirror", "f"), "w") as f:
            f.write("x")
        subprocess.run(["git", "-C", os.path.join(td, "mirror"), "add", "."], check=False)
        subprocess.run(["git", "-C", os.path.join(td, "mirror"), "commit", "-qm", "b"], check=False)
        head = subprocess.run(["git", "-C", os.path.join(td, "mirror"), "rev-parse", "HEAD"],
                              capture_output=True, text=True).stdout.strip()
        bins = {}
        for name in WINDOW_BINS:
            with open(os.path.join(dbg, name), "wb") as f:
                f.write(("body-" + name).encode())
            bins[name] = {"sha256": sha256_file(os.path.join(dbg, name)),
                          "bytes": len("body-" + name)}
        prov = {"version": PROVENANCE_VERSION, "source_head": head,
                "target_dir": os.path.abspath(os.path.join(td, "target")), "bins": bins}
        prov_path = os.path.join(td, "target", "window-provenance.json")
        with open(prov_path, "w") as f:
            json.dump(prov, f)

        args = mk_args(td, prov_path)
        args.mirror = os.path.join(td, "mirror")

        # 1) 干净场景 ⇒ GREEN
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=base_db)
        expect("干净场景整体 GREEN", [status_of(res, n) for n in
                                 ("P1_PORTS", "P2_MIRROR_PROVENANCE", "P3_TEST_DB_CLONE")],
               ["GREEN", "GREEN", "GREEN"])

        # 2) 任一游戏端口在听 ⇒ RED 且点名
        for p in GAME_PORTS:
            res, _ = evaluate(args, port_override={MYSQL_PORT, p}, db_override=base_db)
            st = status_of(res, "P1_PORTS")
            detail = [d for r in res if r.name == "P1_PORTS" for d in r.detail]
            expect("游戏端口 %d 在听 => RED 且点名" % p,
                   (st, any(str(p) in d for d in detail)), ("RED", True))

        # 3) 7600 在听 ⇒ RED
        res, _ = evaluate(args, port_override={MYSQL_PORT, 7600}, db_override=base_db)
        expect("7600 在听 => RED", status_of(res, "P1_PORTS"), "RED")

        # 4) 3306 DOWN ⇒ RED
        res, _ = evaluate(args, port_override=set(), db_override=base_db)
        expect("3306 DOWN => RED", status_of(res, "P1_PORTS"), "RED")

        # 5) provenance：source_head 与镜像不符 ⇒ RED
        bad = dict(prov)
        bad["source_head"] = "0" * 40
        with open(prov_path, "w") as f:
            json.dump(bad, f)
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=base_db)
        expect("source_head 不符 => RED", status_of(res, "P2_MIRROR_PROVENANCE"), "RED")

        # 6) provenance：exe sha256 不符 ⇒ RED
        with open(prov_path, "w") as f:
            json.dump(prov, f)
        with open(os.path.join(dbg, "game-svr.exe"), "wb") as f:
            f.write(b"tampered")
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=base_db)
        expect("exe 被换 => RED", status_of(res, "P2_MIRROR_PROVENANCE"), "RED")
        with open(os.path.join(dbg, "game-svr.exe"), "wb") as f:
            f.write(b"body-game-svr.exe")

        # 7) provenance 缺件 ⇒ PREREQ
        os.remove(os.path.join(dbg, "sel-gate.exe"))
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=base_db)
        expect("缺 sel-gate.exe => PREREQ", status_of(res, "P2_MIRROR_PROVENANCE"), "PREREQ_RED")
        with open(os.path.join(dbg, "sel-gate.exe"), "wb") as f:
            f.write(b"body-sel-gate.exe")

        # 8) 测试库表数不符（11 -> 10）
        db = json.loads(json.dumps(base_db))
        db["tables"]["mir2_test_db"] = db["tables"]["mir2_test_db"][:-1]
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=db)
        expect("测试库表数=10 => RED", status_of(res, "P3_TEST_DB_CLONE"), "RED")

        # 9) 测试库表集合与生产不同 ⇒ RED
        db = json.loads(json.dumps(base_db))
        db["tables"]["mir2_test_db"][0] = "renamed"
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=db)
        expect("测试库表集合漂移 => RED", status_of(res, "P3_TEST_DB_CLONE"), "RED")

        # 10) 克隆漂移：行数不等 ⇒ RED
        db = json.loads(json.dumps(base_db))
        db["counts"]["mir2_test_db.characters"] = 7000
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=db)
        expect("克隆行数漂移 => RED", status_of(res, "P3_TEST_DB_CLONE"), "RED")

        # 11) 缺测试库 ⇒ RED
        db = json.loads(json.dumps(base_db))
        db["schemas"] = [d for d in db["schemas"] if d != "mir2_test_data"]
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=db)
        expect("缺 mir2_test_data => RED", status_of(res, "P3_TEST_DB_CLONE"), "RED")

        # 12) MySQL 读失败 ⇒ PREREQ
        db = {"schemas": [], "tables": {}, "counts": {}, "errors": ["mysql rc=1"]}
        res, _ = evaluate(args, port_override={MYSQL_PORT}, db_override=db)
        expect("MySQL 读失败 => PREREQ", status_of(res, "P3_TEST_DB_CLONE"), "PREREQ_RED")

        # 13) --expect-source-head 前缀匹配
        args2 = mk_args(td, prov_path)
        args2.mirror = os.path.join(td, "mirror")
        args2.expect_source_head = head[:8]
        res, _ = evaluate(args2, port_override={MYSQL_PORT}, db_override=base_db)
        expect("expect-source-head 前缀匹配 => GREEN",
               status_of(res, "P2_MIRROR_PROVENANCE"), "GREEN")
        args2.expect_source_head = "deadbeef"
        res, _ = evaluate(args2, port_override={MYSQL_PORT}, db_override=base_db)
        expect("expect-source-head 不符 => RED",
               status_of(res, "P2_MIRROR_PROVENANCE"), "RED")

        # 14) 退出码映射
        results = [Result("A")]
        expect("全绿 rc=0", report(results)[1], 0)
        results = [Result("A")]
        results[0].fail("x")
        expect("RED rc=1", report(results)[1], 1)
        results = [Result("A")]
        results[0].fail("x", prereq=True)
        expect("PREREQ rc=2", report(results)[1], 2)

    print("SELFTEST %s (%d assertions)" % ("GREEN" if ok else "RED", count[0]))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
