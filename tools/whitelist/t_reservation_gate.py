#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`T-*` 编号**预约一致性门禁**（卡 `openmir2-t-number-reservation`，tier=L）。

配合 `next_t.py`（唯一取号入口）与 `mir2-rs/tests/parity/T-RESERVATIONS.md`（预约表）：

    取号   python -I tools/whitelist/next_t.py --card <卡 id>
    校验   python -I tools/whitelist/t_reservation_gate.py check      # **合并结果树**上必跑
    自检   python -I tools/whitelist/t_reservation_gate.py selftest
    牙齿   python -I tools/whitelist/t_reservation_gate.py teeth

> 在**自己分支的 worktree** 上跑永远是绿的：两条分支各自都只见到自己的号。必须在
> **合并结果树**上跑（`git merge --no-commit` 的结果，或临时 worktree 合完再验）。

## 校验规则（任一不满足 ⇒ rc=1 并点名）

- **①** `whitelist.md` 的每个 `T-<n>`（**只认行首 `| T-n |` 的登记行**，正文引用不查）必须在预约表
  有「在用」或「已落地」行；只剩「已释放」行也判红。
- **②** 同一个号在预约表里不得有**两个「在用」认领**，也不得出现同号多行。
- **③** 预约表「已落地」的号必须真的在 `whitelist.md` 里出现。
- **④** `whitelist.md` 内部同一号出现多行（T-4 那一类撞号）⇒ 红，点名号与行号。
- **⑤** 同一**卡 id** 不得有多个「在用」认领（`--force-new` 应把旧行改「已释放」）。
- **⓪** 坏输入：预约表里以 `| T-` 开头但解析不成四列的行（带行号）；**任一文件 ``` 围栏不成对**
  （会静默跳过围栏后的登记行 ⇒ fail-closed）。

不可读 / 缺文件 / **空表或空 whitelist** ⇒ rc=2（fail-closed，不静默通过）。

## 牙齿（`teeth`）

在**真实**文件的副本上做五种破坏，每次都必须变红且点名对应规则，随后**逐字节还原并核对 sha256**
（正本全程不落笔，结束时再核一次正本 sha256 未变）：
复制「在用」⇒②、删「已落地」⇒①、伪造「已落地」⇒③、复制 whitelist 登记行⇒④、孤立围栏⇒⓪。
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

ROW_ONE = re.compile(
    r"^\|\s*(T-\d+)\s*\|\s*([^|]*?)\s*\|\s*([^|]*?)\s*\|\s*(在用|已落地|已释放)\s*\|\s*$")
WROW_ONE = re.compile(r"^\|\s*(T-\d+)\s*\|")
NUM_RE = re.compile(r"^T-(\d+)$")


# --------------------------------------------------------------------------- 解析
def scan_lines(text):
    """⇒ (lines, fence_ok)。跳过 ``` 围栏内；**围栏不成对 ⇒ fence_ok=False（必须 fail-closed）**。"""
    out, in_fence = [], False
    for idx, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        out.append((line, idx))
    return out, not in_fence


def parse_table(text):
    """⇒ (rows, malformed)。rows=[{num,token,card,ts,status,line}]。"""
    lines, _ok = scan_lines(text)
    rows, bad = [], []
    for line, no in lines:
        m = ROW_ONE.match(line)
        if m:
            rows.append({"num": int(NUM_RE.match(m.group(1)).group(1)), "token": m.group(1),
                         "card": m.group(2).strip(), "ts": m.group(3).strip(),
                         "status": m.group(4), "line": no})
        elif line.lstrip().startswith("| T-"):
            bad.append((no, line))
    return rows, bad


def parse_whitelist(text):
    """⇒ (nums, dups)。dups={num: [line_no,...]}（同一号出现多行）。只认行首 `| T-n |`。"""
    lines, _ok = scan_lines(text)
    seen = {}
    for line, no in lines:
        m = WROW_ONE.match(line)
        if m:
            seen.setdefault(int(NUM_RE.match(m.group(1)).group(1)), []).append(no)
    nums = set(seen)
    dups = {n: ls for n, ls in seen.items() if len(ls) > 1}
    return nums, dups


# --------------------------------------------------------------------------- 校验
def check_texts(table_text, whitelist_text):
    """纯函数（selftest 注入合成文本）⇒ {violations, counts,...}。"""
    rows, malformed = parse_table(table_text)
    wl, wdup = parse_whitelist(whitelist_text)
    _tl, table_fence = scan_lines(table_text)
    _wl, wl_fence = scan_lines(whitelist_text)
    v = []

    if not table_fence:                                                      # ⓪
        v.append("⓪ 预约表 ``` 围栏不成对（围栏后的登记行会被静默跳过）⇒ fail-closed")
    if not wl_fence:
        v.append("⓪ whitelist.md ``` 围栏不成对（围栏后的号会被漏掉）⇒ fail-closed")
    for no, line in malformed:
        v.append("⓪ L{0} 无法解析的预约行（应为 `| T-n | 卡 id | 时间 | 状态 |`）：{1}".format(
            no, line.strip()[:80]))

    for n in sorted(wdup):                                                   # ④
        v.append("④ whitelist.md 内部 T-{0} 重复 {1} 行：L{2}（撞号，引用方无法判定）".format(
            n, len(wdup[n]), ",".join(str(x) for x in wdup[n])))

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

    by_card = {}
    for r in rows:                                                           # ⑤
        if r["status"] == LIVE:
            by_card.setdefault(r["card"], []).append(r)
    for card, rs in sorted(by_card.items()):
        if len(rs) > 1:
            v.append("⑤ 卡 id `{0}` 有 {1} 个「在用」认领：{2}（--force-new 应把旧行改「已释放」）"
                     .format(card, len(rs),
                             "、".join("L{0}({1})".format(r["line"], r["token"]) for r in rs)))

    counts = {
        "rows": len(rows),
        "whitelist": len(wl),
        "live": sum(1 for r in rows if r["status"] == LIVE),
        "landed": sum(1 for r in rows if r["status"] == LANDED),
        "released": sum(1 for r in rows if r["status"] == RELEASED),
        "malformed": len(malformed),
        "whitelist_dups": len(wdup),
        "table_fence_ok": table_fence,
        "whitelist_fence_ok": wl_fence,
        "max_num": max(by_num) if by_num else 0,
    }
    return {"violations": v, "counts": counts, "by_num": sorted(by_num.keys())}


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
          "released={released} malformed={malformed} wl_dups={whitelist_dups} "
          "fence_ok={table_fence_ok}/{whitelist_fence_ok} max=T-{max_num}".format(**c))
    for item in report["violations"]:
        print("  VIOLATION {0}".format(item))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(report, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    if report["violations"]:
        print("T_RESERVATION_GATE RED {0} 条违规（①whitelist→预约表 ②同号多认领 "
              "③已落地→whitelist ④whitelist 同号 ⑤同卡多认领 ⓪坏输入）".format(
                  len(report["violations"])))
        return 1
    if c["rows"] == 0 or c["whitelist"] == 0:
        print("T_RESERVATION_GATE RED 空表 / 空 whitelist（rows={0} whitelist={1}）⇒ fail-closed"
              .format(c["rows"], c["whitelist"]))
        return 2
    print("T_RESERVATION_GATE GREEN（①-⑤ 与 ⓪ 全过：预约表 {rows} 行 / whitelist {whitelist} 号）"
          .format(**c))
    return 0


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

    circled = {"⓪": "0", "①": "1", "②": "2", "③": "3", "④": "4", "⑤": "5"}

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
           "1" in rules(rep) and any("T-30" in x for x in rep["violations"]),
           str(rep["violations"]))

    rep = check_texts(table + "| T-3 | d | 2026-01-01T00:00:00Z | 在用 |\n", wl)
    expect("② 同一号两个「在用」认领 ⇒ 点名 T-3",
           "2" in rules(rep) and any("T-3" in x for x in rep["violations"]),
           str(rep["violations"]))

    rep = check_texts(table + "| T-99 | e | 2026-01-01T00:00:00Z | 已落地 |\n", wl)
    expect("③ 已落地但 whitelist 无 ⇒ 点名 T-99",
           "3" in rules(rep) and any("T-99" in x for x in rep["violations"]),
           str(rep["violations"]))

    rep = check_texts(table.replace("| T-2 | b | 2026-01-01T00:00:00Z | 已落地 |",
                                    "| T-2 | b | 2026-01-01T00:00:00Z | 已释放 |"), wl)
    expect("① 只剩「已释放」行而 whitelist 仍登记 ⇒ 点名 T-2",
           any(x.startswith("① T-2") for x in rep["violations"]), str(rep["violations"]))

    rep = check_texts(table, wl + "| T-1 | duplicate-registration |\n")
    expect("④ whitelist 内部同号（T-4 那一类）⇒ 点名 T-1 与两处行号",
           "4" in rules(rep) and any("T-1" in x and "L1" in x for x in rep["violations"]),
           str(rep["violations"]))

    rep = check_texts(table + "| T-3 | d | 2026-01-01T00:00:00Z | 在用 |\n", wl)
    rep5 = check_texts(table.replace("| T-3 | c | 2026-01-01T00:00:00Z | 在用 |",
                                     "| T-3 | c | 2026-01-01T00:00:00Z | 在用 |\n"
                                     "| T-4 | c | 2026-01-01T00:00:00Z | 在用 |"), wl)
    expect("⑤ 同一卡 id 两个「在用」⇒ 判红",
           "5" in rules(rep5) and any("`c`" in x for x in rep5["violations"]),
           str(rep5["violations"]))
    _ = rep

    rep = check_texts(table + "| T-4 | 缺状态 | 时间 |\n", wl)
    expect("⓪ 坏行（缺合法状态）⇒ 判红并给行号",
           "0" in rules(rep) and any("L" in x for x in rep["violations"]),
           str(rep["violations"]))

    rep = check_texts(table + "```\n", wl)
    expect("⓪ 预约表围栏不成对 ⇒ 判红（fail-closed）",
           "0" in rules(rep) and any("围栏" in x for x in rep["violations"]),
           str(rep["violations"]))
    rep = check_texts(table, wl + "```\n| T-707 | real-registration |\n")
    expect("⓪ whitelist 围栏不成对 ⇒ 判红（围栏后的号不再被静默跳过）",
           "0" in rules(rep), str(rep["violations"]))

    rep = check_texts(table + "```\n| T-77 | example | ts | 在用 |\n```\n", wl)
    expect("成对围栏里的示例行不算登记行", rep["violations"] == [], str(rep["violations"]))

    rep = check_texts(table, wl)
    expect("正样本可重入（无状态残留）", rep["violations"] == [])

    # 写盘路径：缺文件 / 空文件都必须 fail-closed
    empty = tempfile.mkdtemp(prefix="tresv-selftest-")
    try:
        ns = argparse.Namespace(table=os.path.join(empty, "nope.md"),
                                whitelist=os.path.join(empty, "nope2.md"), json=None)
        expect("缺预约表 ⇒ rc=2（fail-closed，不静默绿）", cmd_check(ns) == 2)
        zero_t = os.path.join(empty, "zero.md")
        zero_w = os.path.join(empty, "zero-w.md")
        for p in (zero_t, zero_w):
            with open(p, "w", encoding="utf-8", newline="\n") as fh:
                fh.write("")
        ns = argparse.Namespace(table=zero_t, whitelist=zero_w, json=None)
        expect("0 字节空表 ⇒ rc=2（N-2：空文件不是绿）", cmd_check(ns) == 2)
    finally:
        shutil.rmtree(empty, ignore_errors=True)

    # N-5：两份解析器 cross-check（同一份文本，号集合 / 行号 / 状态必须一致）
    sample = (table + "| T-4 | d | 2026-01-01T00:00:00Z | 已释放 |\n| T-5 | bad |\n"
              "```\n| T-77 | example | ts | 在用 |\n```\n")
    sample_wl = (wl + "| T-9 | z |\n```\n| T-8 | fenced |\n```\n")
    try:
        sys.path.insert(0, HERE)
        import next_t as nt
        nt_rows = {(r["num"], r["line"], r["status"]) for r in nt.parse_table(sample)}
        g_rows, _bad = parse_table(sample)
        g_set = {(r["num"], r["line"], r["status"]) for r in g_rows}
        expect("N-5 cross-check：两份解析器的（号,行号,状态）集合一致",
               nt_rows == g_set, "{0} vs {1}".format(sorted(nt_rows), sorted(g_set)))
        expect("N-5 cross-check：whitelist 号集合一致",
               nt.parse_whitelist_numbers(sample_wl) == parse_whitelist(sample_wl)[0])
        bad_fence = sample + "```\n"
        expect("N-5 cross-check：围栏不成对的判定一致",
               nt.scan_lines(bad_fence)[1] is False and scan_lines(bad_fence)[1] is False)
    except ImportError as exc:                                   # 不静默：缺兄弟脚本即失败
        expect("N-5 cross-check：next_t.py 可被导入", False, str(exc))

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

    tmp = tempfile.mkdtemp(prefix="tresv-teeth-")
    try:
        table = os.path.join(tmp, "T-RESERVATIONS.md")
        wl = os.path.join(tmp, "whitelist.md")
        table_text = orig_table.decode("utf-8", "replace")
        wl_text = orig_wl.decode("utf-8", "replace")

        rows, _bad = parse_table(table_text)
        wl_nums, _dups = parse_whitelist(wl_text)
        landed = [r for r in rows if r["status"] == LANDED and r["num"] in wl_nums]
        if not landed:
            print("TEETH RED 正本里没有「已落地」行可做删行牙——先跑 next_t.py --bootstrap")
            return 2
        victim = max(landed, key=lambda r: r["num"])
        victim_line = next((ln for ln, no in scan_lines(table_text)[0]
                            if no == victim["line"]), None)
        first_wl_line = next((ln for ln, _no in scan_lines(wl_text)[0]
                              if WROW_ONE.match(ln)), None)
        first_wl_num = WROW_ONE.match(first_wl_line).group(1) if first_wl_line else None

        def run_case(label, expect_rule, mutate):
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
            live = "| T-901 | teeth/alpha | " + stamp + " | 在用 |\n"
            return t + live + live.replace("teeth/alpha", "teeth/beta"), w

        def tooth_del_landed(t, w):
            kept = [ln for ln, no in scan_lines(t)[0] if no != victim["line"]]
            return "\n".join(kept) + "\n", w

        def tooth_ghost_landed(t, w):
            return t + "| T-999 | teeth/ghost | " + stamp + " | 已落地 |\n", w

        def tooth_dup_wl(t, w):
            if not first_wl_line:
                return t + "\n", w
            suffix = "" if w.endswith("\n") else "\n"
            return t, w + suffix + first_wl_line + "\n"

        def tooth_fence(t, w):
            return t + "```\n", w

        results = [
            ("牙1 复制一条「在用」认领（同号两认领）", "②", tooth_dup_live),
            ("牙2 删一条「已落地」行（T-{0}）".format(victim["num"]), "①", tooth_del_landed),
            ("牙3 伪造 whitelist 没有的「已落地」T-999", "③", tooth_ghost_landed),
            ("牙4 复制一条 whitelist 登记行（{0}）".format(first_wl_num), "④", tooth_dup_wl),
            ("牙5 追加孤立围栏（围栏不成对）", "⓪", tooth_fence),
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
        green = ok == len(results) and intact
        print("TEETH {0} {1}/{2}{3}".format("GREEN" if green else "RED", ok, len(results),
                                            "" if intact else "（正本 sha256 变了！）"))
        return 0 if green else 1
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main(argv=None):
    p = argparse.ArgumentParser(description="T-* 编号预约一致性门禁（只读，零端口）")
    sub = p.add_subparsers(dest="cmd", required=True)
    pc = sub.add_parser("check", help="①-⑤ + ⓪ 校验（**合并结果树**上必跑）")
    pc.add_argument("--table", default=DEFAULT_TABLE)
    pc.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pc.add_argument("--json", default=None, help="把报告写成 JSON")
    pc.set_defaults(func=cmd_check)
    sub.add_parser("selftest", help="合成文本自检（含改坏必红 / 解析器 cross-check）") \
        .set_defaults(func=cmd_selftest)
    pt = sub.add_parser("teeth", help="真实表的破坏-变红-逐字节还原-核 sha256")
    pt.add_argument("--table", default=DEFAULT_TABLE)
    pt.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    pt.set_defaults(func=cmd_teeth)
    args = p.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
