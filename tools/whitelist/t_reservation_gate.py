#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`T-*` 编号**预约一致性门禁**（卡 `openmir2-t-number-reservation`，tier=L）。

配合 `next_t.py`（唯一取号入口）与 `mir2-rs/tests/parity/T-RESERVATIONS.md`（预约表）：

    取号   python -I tools/whitelist/next_t.py --card <卡 id>
    校验   python -I tools/whitelist/t_reservation_gate.py check      # 合并前必跑
    自检   python -I tools/whitelist/t_reservation_gate.py selftest
    牙齿   python -I tools/whitelist/t_reservation_gate.py teeth

## 三向校验（任一不满足 ⇒ rc=1 并点名）

- **①** `whitelist.md` 里出现的每个 `T-<n>` 必须在预约表里有对应行（状态「在用」或「已落地」；
  只剩「已释放」行也判红——那说明登记还在但认领被撤销了）。
- **②** 同一个号在预约表里不得有**两个「在用」认领**，也不得出现同号多行（含历史重复）。
- **③** 预约表里状态「已落地」的号必须真的在 `whitelist.md` 里出现（否则是「登记了但没落地」
  的幽灵行，会把号占死）。
- **⓪**（附加、fail-closed）预约表里以 `| T-` 开头但解析不成四列的行 = 坏行，判红并点名行号。
  围栏代码块（```）内的示例行不算登记行。

不可读 / 缺文件 ⇒ rc=2（fail-closed，不静默通过）。

## 牙齿（`teeth`）

在**真实**预约表 + whitelist 的副本上做三种破坏，每次都必须变红且**点名对应规则**；
每次破坏后逐字节还原并核对 sha256（正本全程不落笔，还原校验在副本上做，另核对正本 sha256 不变）：
① 复制一条「在用」认领（②）② 删一条「已落地」行（①）③ 伪造一条 whitelist 没有的「已落地」（③）。
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
DEFAULT_TABLE = os.path.join(ROOT, "mir2-rs", "tests", "parity", "T-RESERVATIONS.md")
DEFAULT_WHITELIST = os.path.join(ROOT, "mir2-rs", "tests", "parity", "whitelist.md")

LIVE, LANDED, RELEASED = "在用", "已落地", "已释放"
STATUSES = (LIVE, LANDED, RELEASED)
RULE_NAMES = {"0": "坏行", "1": "whitelist 有、预约表无", "2": "同号多认领 / 多行",
              "3": "已落地但 whitelist 无"}

ROW_ONE = re.compile(
    r"^\|\s*(T-\d+)\s*\|\s*([^|]*?)\s*\|\s*([^|]*?)\s*\|\s*(在用|已落地|已释放)\s*\|\s*$")
WROW_ONE = re.compile(r"^\|\s*(T-\d+)\s*\|")
NUM_RE = re.compile(r"^T-(\d+)$")


# --------------------------------------------------------------------------- 解析
def _lines(text):
    """逐行产出 (line_text, line_no)，跳过 ``` 围栏代码块（里面的示例行不是登记行）。"""
    in_fence = False
    for idx, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        yield line, idx


def parse_table(text):
    """⇒ (rows, malformed)。rows = [{num,token,card,ts,status,line}]。"""
    rows, bad = [], []
    for line, no in _lines(text):
        m = ROW_ONE.match(line)
        if m:
            rows.append({"num": int(NUM_RE.match(m.group(1)).group(1)), "token": m.group(1),
                         "card": m.group(2).strip(), "ts": m.group(3).strip(),
                         "status": m.group(4), "line": no})
        elif line.lstrip().startswith("| T-") or line.lstrip().startswith("|T-"):
            bad.append((no, line))
    return rows, bad


def parse_whitelist_numbers(text):
    return {int(NUM_RE.match(m.group(1)).group(1))
            for line, _no in _lines(text) for m in [WROW_ONE.match(line)] if m}


# --------------------------------------------------------------------------- 校验
def check_texts(table_text, whitelist_text):
    """纯函数（selftest 注入合成文本）⇒ {violations, counts...}。"""
    rows, malformed = parse_table(table_text)
    wl = parse_whitelist_numbers(whitelist_text)
    v = []

    for no, line in malformed:                                               # ⓪
        v.append("⓪ L{0} 无法解析的预约行（应为 `| T-n | 卡 id | 时间 | 状态 |`）：{1}".format(
            no, line.strip()[:80]))

    by_num = {}
    for r in rows:
        by_num.setdefault(r["num"], []).append(r)

    for n in sorted(wl):                                                     # ①
        rs = by_num.get(n, [])
        if not rs:
            v.append("① T-{0} 在 whitelist.md 有登记，但预约表没有对应的行".format(n))
        elif not any(r["status"] in (LIVE, LANDED) for r in rs):
            v.append("① T-{0} 预约表只有「已释放」行（L{1}），但 whitelist.md 仍登记着".format(
                n, ",".join(str(r["line"]) for r in rs)))

    for n, rs in sorted(by_num.items()):                                     # ②
        live = [r for r in rs if r["status"] == LIVE]
        if len(live) > 1:
            v.append("② T-{0} 有 {1} 个「在用」认领：{2}".format(
                n, len(live),
                "、".join("L{0}({1})".format(r["line"], r["card"]) for r in live)))
        elif len(rs) > 1:
            v.append("② T-{0} 在预约表出现 {1} 行（同号多行/历史重复）：{2}".format(
                n, len(rs), ",".join("L{0}".format(r["line"]) for r in rs)))

    for n in sorted(by_num):                                                 # ③
        ghosts = [r for r in by_num[n] if r["status"] == LANDED and n not in wl]
        if ghosts:
            v.append("③ T-{0} 在预约表是「已落地」（L{1}），但 whitelist.md 没有这个号".format(
                n, ",".join(str(r["line"]) for r in ghosts)))

    counts = {
        "rows": len(rows),
        "whitelist": len(wl),
        "live": sum(1 for r in rows if r["status"] == LIVE),
        "landed": sum(1 for r in rows if r["status"] == LANDED),
        "released": sum(1 for r in rows if r["status"] == RELEASED),
        "malformed": len(malformed),
        "max_num": max(by_num) if by_num else 0,
    }
    return {"violations": v, "counts": counts,
            "by_num": sorted(by_num.keys()), "missing_in_whitelist":
                sorted(n for n in by_num if n not in wl)}


# --------------------------------------------------------------------------- 命令
def read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace", newline="") as fh:
        return fh.read()


def cmd_check(args):
    if not os.path.isfile(args.table):
        print("T_RESERVATION_GATE RED 预约表缺失：{0}".format(args.table))
        return 2
    if not os.path.isfile(args.whitelist):
        print("T_RESERVATION_GATE RED whitelist 缺失：{0}".format(args.whitelist))
        return 2
    try:
        report = check_texts(read_text(args.table), read_text(args.whitelist))
    except OSError as exc:
        print("T_RESERVATION_GATE RED 读取失败：{0}".format(exc))
        return 2
    c = report["counts"]
    print("T_RESERVATION_GATE table={0}".format(args.table))
    print("  rows={rows} whitelist={whitelist} live={live} landed={landed} "
          "released={released} malformed={malformed} max=T-{max_num}".format(**c))
    for item in report["violations"]:
        print("  VIOLATION {0}".format(item))
    if report["violations"]:
        print("T_RESERVATION_GATE RED {0} 条违规（①whitelist→预约表 ②同号多认领 "
              "③已落地→whitelist ⓪坏行）".format(len(report["violations"])))
        rc = 1
    else:
        print("T_RESERVATION_GATE GREEN（三向一致：预约表 {rows} 行 / whitelist {whitelist} 号）"
              .format(**c))
        rc = 0
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(report, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return rc


# --------------------------------------------------------------------------- 自检
def cmd_selftest(_args):
    ok = total = 0

    def expect(name, cond, detail=""):
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
            print("  SELFTEST ok  {0}".format(name))
        else:
            print("  SELFTEST FAIL {0}{1}".format(name, ("  <- " + detail) if detail else ""))

    circled = {"⓪": "0", "①": "1", "②": "2", "③": "3"}

    def rules(rep):
        return {circled.get(x[0], x[0]) for x in rep["violations"]}

    table = ("# 预约表\n\n| 号 | 卡 id | 认领时间 | 状态 |\n| --- | --- | --- | --- |\n"
             "| T-1 | a | 2026-01-01T00:00:00Z | 已落地 |\n"
             "| T-2 | b | 2026-01-01T00:00:00Z | 已落地 |\n"
             "| T-3 | c | 2026-01-01T00:00:00Z | 在用 |\n")
    wl = "| T-1 | x |\n| T-2 | y |\n| T-3 | z |\n"

    rep = check_texts(table, wl)
    expect("干净的三向一致 ⇒ 无违规", rep["violations"] == [], str(rep["violations"]))
    expect("计数正确（3 行 / 3 号 / 1 在用 / 2 已落地）",
           rep["counts"]["rows"] == 3 and rep["counts"]["whitelist"] == 3
           and rep["counts"]["live"] == 1 and rep["counts"]["landed"] == 2)

    rep = check_texts(table, wl + "| T-30 | ghost |\n")
    expect("① whitelist 有、预约表无 ⇒ 点名 T-30",
           "1" in rules(rep) and any("T-30" in x for x in rep["violations"]), str(rep["violations"]))

    rep = check_texts(table + "| T-3 | d | 2026-01-01T00:00:00Z | 在用 |\n", wl)
    expect("② 同一号两个「在用」认领 ⇒ 点名 T-3",
           "2" in rules(rep) and any("T-3" in x for x in rep["violations"]), str(rep["violations"]))

    rep = check_texts(table + "| T-99 | e | 2026-01-01T00:00:00Z | 已落地 |\n", wl)
    expect("③ 已落地但 whitelist 无 ⇒ 点名 T-99",
           "3" in rules(rep) and any("T-99" in x for x in rep["violations"]), str(rep["violations"]))

    rep = check_texts(table.replace("| T-2 | b | 2026-01-01T00:00:00Z | 已落地 |",
                                    "| T-2 | b | 2026-01-01T00:00:00Z | 已释放 |"), wl)
    expect("① 只剩「已释放」行而 whitelist 仍登记 ⇒ 点名 T-2",
           any(x.startswith("① T-2") for x in rep["violations"]), str(rep["violations"]))

    rep = check_texts(table + "| T-4 | 缺状态 | 时间 |\n", wl)
    expect("⓪ 坏行（缺合法状态）⇒ 判红并给行号",
           "0" in rules(rep) and any("L" in x for x in rep["violations"]), str(rep["violations"]))

    rep = check_texts(table + "```\n| T-77 | example | ts | 在用 |\n| T-78 | half\n```\n", wl)
    expect("围栏代码块里的示例行不算登记行", rep["violations"] == [], str(rep["violations"]))

    rep = check_texts(table, wl)
    expect("正样本可重入（无状态残留）", rep["violations"] == [])

    # 写盘路径：缺文件必须 rc=2（fail-closed）
    empty = tempfile.mkdtemp(prefix="tresv-selftest-")
    ns = argparse.Namespace(table=os.path.join(empty, "nope.md"),
                            whitelist=os.path.join(empty, "nope2.md"), json=None)
    expect("缺预约表 ⇒ rc=2（fail-closed，不静默绿）", cmd_check(ns) == 2)

    print("SELFTEST {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


# --------------------------------------------------------------------------- 牙齿
def _sha256_bytes(blob):
    return hashlib.sha256(blob).hexdigest()


def _restore_and_verify(path, frozen, label):
    with open(path, "wb") as fh:
        fh.write(frozen)
        fh.flush()
        os.fsync(fh.fileno())
    with open(path, "rb") as fh:
        back = fh.read()
    ok = _sha256_bytes(back) == _sha256_bytes(frozen) and back == frozen
    print("    RESTORE {0} sha256={1} {2}".format(
        label, _sha256_bytes(back), "OK（逐字节还原）" if ok else "FAIL"))
    return ok


def cmd_teeth(args):
    for p in (args.table, args.whitelist):
        if not os.path.isfile(p):
            print("TEETH RED 输入缺失：{0}".format(p))
            return 2
    with open(args.table, "rb") as fh:
        orig_table = fh.read()
    with open(args.whitelist, "rb") as fh:
        orig_wl = fh.read()
    orig_table_sha, orig_wl_sha = _sha256_bytes(orig_table), _sha256_bytes(orig_wl)
    print("TEETH 正本 table sha256={0}".format(orig_table_sha))
    print("TEETH 正本 whitelist sha256={0}".format(orig_wl_sha))
    print("TEETH 正本大小 table={0}B whitelist={1}B".format(len(orig_table), len(orig_wl)))

    tmp = tempfile.mkdtemp(prefix="tresv-teeth-")
    table = os.path.join(tmp, "T-RESERVATIONS.md")
    wl = os.path.join(tmp, "whitelist.md")
    table_text = orig_table.decode("utf-8", "replace")
    wl_text = orig_wl.decode("utf-8", "replace")

    # 选一条真实「已落地」且 whitelist 里确有的号来删（牙 2）
    rows, _bad = parse_table(table_text)
    wl_nums = parse_whitelist_numbers(wl_text)
    landed = [r for r in rows if r["status"] == LANDED and r["num"] in wl_nums]
    if not landed:
        print("TEETH RED 正本里没有「已落地」行可做删行牙——先跑 next_t.py --bootstrap")
        return 2
    victim = max(landed, key=lambda r: r["num"])
    victim_line = None
    for line, no in _lines(table_text):
        if no == victim["line"]:
            victim_line = line
            break

    def run_case(label, expect_rule, mutate):
        """mutate(table_text, wl_text) -> (t2, w2)；必须变红且点名 expect_rule。"""
        with open(table, "wb") as fh:
            fh.write(orig_table)
        with open(wl, "wb") as fh:
            fh.write(orig_wl)
        t2, w2 = mutate(table_text, wl_text)
        with open(table, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(t2)
        with open(wl, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(w2)
        rep = check_texts(read_text(table), read_text(wl))
        hit = [x for x in rep["violations"] if x.startswith(expect_rule)]
        print("  TEETH {0}：{1} 违规={2} 命中{3}={4}".format(
            "ok " if hit else "FAIL", label, len(rep["violations"]), expect_rule, len(hit)))
        for x in rep["violations"][:4]:
            print("      {0}".format(x))
        restored = _restore_and_verify(table, orig_table, "table")
        restored &= _restore_and_verify(wl, orig_wl, "whitelist")
        return bool(hit) and restored

    stamp = "2026-10-23T00:00:00Z"

    def tooth_dup_live(t, w):
        live = ("| T-901 | teeth/alpha | " + stamp + " | 在用 |\n")
        return t + live + live.replace("teeth/alpha", "teeth/beta"), w

    def tooth_del_landed(t, w):
        kept = [ln for ln, no in _lines(t) if no != victim["line"]]
        return "\n".join(kept) + "\n", w

    def tooth_ghost_landed(t, w):
        return t + "| T-999 | teeth/ghost | " + stamp + " | 已落地 |\n", w

    results = [
        ("牙1 复制一条「在用」认领（同号两认领）", "②", tooth_dup_live),
        ("牙2 删一条「已落地」行（T-{0}）".format(victim["num"]), "①", tooth_del_landed),
        ("牙3 伪造 whitelist 没有的「已落地」T-999", "③", tooth_ghost_landed),
    ]
    ok = 0
    for label, rule, fn in results:
        if run_case(label, rule, fn):
            ok += 1

    with open(args.table, "rb") as fh:
        now_table = fh.read()
    with open(args.whitelist, "rb") as fh:
        now_wl = fh.read()
    intact = (_sha256_bytes(now_table) == orig_table_sha
              and _sha256_bytes(now_wl) == orig_wl_sha)
    print("  TEETH 正本未被修改：table={0} whitelist={1}".format(
        _sha256_bytes(now_table), _sha256_bytes(now_wl)))
    shutil.rmtree(tmp, ignore_errors=True)
    green = ok == len(results) and intact
    print("TEETH {0} {1}/{2}{3}".format("GREEN" if green else "RED", ok, len(results),
                                         "" if intact else "（正本 sha256 变了！）"))
    return 0 if green else 1


def main(argv=None):
    p = argparse.ArgumentParser(description="T-* 编号预约一致性门禁（只读，零端口）")
    sub = p.add_subparsers(dest="cmd", required=True)
    pc = sub.add_parser("check", help="三向校验（合并前必跑）")
    pc.add_argument("--table", default=DEFAULT_TABLE)
    pc.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pc.add_argument("--json", default=None, help="把报告写成 JSON")
    pc.set_defaults(func=cmd_check)
    sub.add_parser("selftest", help="合成文本自检（含改坏必红）").set_defaults(func=cmd_selftest)
    pt = sub.add_parser("teeth", help="真实表的破坏-变红-逐字节还原-核 sha256")
    pt.add_argument("--table", default=DEFAULT_TABLE)
    pt.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pt.set_defaults(func=cmd_teeth)
    args = p.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
