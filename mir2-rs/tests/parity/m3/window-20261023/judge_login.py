#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""judge_login.py — G1/G2a 登录判据 J0..J5 的可执行单一入口（tier=L 夹具，判据本身 tier=R）。

它把 `docs/window-round2-runbook-20261023.md` §3 的 5 条判据固化成一条命令：
  J1 15 帧入图序 + 52/752 存在 + 52 布局钉死
      —— 直接复用 `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/
         check_client_s2c_frames.py` 的 `check()`（**不重造**；同一份 40B/58B 布局表）。
  J2 654 恰一条且在 46 之后
  J3 42 恰两条（第二 42 = RefShowName），657 恰两条且都在 654 与第二 42 之间
  J4 第二 42 之后恰 `100,100,100,100`
  J5 同序列字段级 diff（**复用** `tools/m2-acceptance/s2c_diff.py --out`，读它的 report JSON）
      - `pass=true` => GREEN（同起点重跑后 50/51 param 差异消失 = 输入漂移）
      - 恰好是已登记的 4 条 50/51 差异 + 3 条 52/D-9 白名单 => `KNOWN_PENDING`
        （**不是 GREEN**，runbook §3.3「不得写成 GREEN」）
      - 任何其它差异 => RED

退出码：0=GREEN、1=RED（含 KNOWN_PENDING）、2=PREREQ_RED；末行 `RESULT: GREEN|RED|PREREQ_RED`。
每条判据独立打印 `J<n> <STATUS>`，JSON 证据写 `--out`。

用法：
  python -I -B judge_login.py --scenario G1 --side rust \
      --capture E:/tmp/window/20261023/rust/s2c/capture.jsonl \
      --golden  E:/tmp/window/20261023/csharp/s2c/capture.jsonl \
      --out E:/tmp/window/20261023/judge/g1-rust.json
  python -I -B judge_login.py --scenario G2A-R3 --side csharp --capture ... --no-j5
  python -I -B judge_login.py --selftest

注意：J2..J4 只对 G1（受控登录尾）成立；G2a 的 210/212/621/211 会改变尾序，
所以 G2a 只跑 J1 + J5。
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
# <repo>/mir2-rs/tests/parity/m3/window-20261023
REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", "..", ".."))
CHECKER_REL = os.path.join("mir2-rs", "tests", "parity", "world",
                           "client-e2e-enter-gaps-20261013", "check_client_s2c_frames.py")
S2C_DIFF_REL = os.path.join("tools", "m2-acceptance", "s2c_diff.py")
SCENARIOS_REL = os.path.join("mir2-rs", "tests", "parity", "m3", "window-20261023",
                             "scenarios.json")
WHITELIST_NAME = "s2c-whitelist-d9.jsonl"

# 入图 15 帧（与 check_client_s2c_frames.ENTER_SEQ / runbook §3.1 同口径）
ENTER_SEQ = [658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46]
# §3.3 已登记的 4 条「待查」差异（50/51 的 param 与 raw_hex）
KNOWN_PENDING_DIFFS = {(50, "param"), (50, "raw_hex"),
                       (51, "param"), (51, "raw_hex")}
D9_WHITELIST_KEYS = {(52, "body_len"), (52, "body_sha256"), (52, "raw_hex")}


# ---------------------------------------------------------------------------
# 复用上游模块（单一来源）
# ---------------------------------------------------------------------------
def load_module(path, name):
    if not os.path.isfile(path):
        raise FileNotFoundError(path)
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def load_checker(root):
    return load_module(os.path.join(root, CHECKER_REL), "check_client_s2c_frames")


def load_scenarios(root):
    path = os.path.join(root, SCENARIOS_REL)
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# 判据
# ---------------------------------------------------------------------------
class Check:
    def __init__(self, jid):
        self.jid = jid
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


def enter_seq_spec(value):
    """把 `scenarios.json` 的 `expect_enter_seq` 规范成上游 `parse_enter_seq` 能吃的字符串。

    上游 `check_client_s2c_frames.check()` 只接受字符串（预设名 / 逗号列表，元素可带 `?`/`[]`），
    直接塞 JSON 数组会在 `spec.strip()` 处 AttributeError（曾导致真实 G1/G2a 判据 rc=1 且不写 --out）。
    """
    if value is None:
        return None
    if isinstance(value, str):
        return value
    return ",".join(str(x) for x in value)


def s2c_idents(rows, port=7200):
    """只取 port=7200 dir=s2c 且非 `frame_form=act` 的带 ident 行（runbook §3.2 的筛选口径）。"""
    out = []
    for r in rows:
        if r.get("dir") != "s2c":
            continue
        if r.get("port", port) != port:
            continue
        if r.get("frame_form") == "act":
            continue
        if "ident" in r:
            out.append(r)
    return out


def login_tail(rows):
    """J2..J4。返回 (fails, facts)。全绿时 facts 形如 §3.2 的 LOGIN_TAIL_OK 行。"""
    fails = []
    facts = {}
    seq = [r["ident"] for r in s2c_idents(rows)]
    facts["idents"] = seq
    if 46 not in seq:
        fails.append("46 SM_DAYCHANGING 缺失（入图未完成/导出不全）")
        return fails, facts
    i46 = seq.index(46)
    facts["i46"] = i46
    pre = seq[max(0, i46 - 14):i46 + 1]
    if pre != ENTER_SEQ:
        fails.append("46 之前 15 帧不是基线序列：got=%s" % pre)
    i654 = [i for i, x in enumerate(seq) if x == 654]
    i657 = [i for i, x in enumerate(seq) if x == 657]
    i42 = [i for i, x in enumerate(seq) if x == 42]
    facts.update({"i654s": i654, "i657s": i657, "i42s": i42})

    # J2：654 恰一条且在 46 之后
    if len(i654) != 1:
        fails.append("J2: 654 恰好 1 条，实际 %d 条 %s" % (len(i654), i654))
    elif i654[0] <= i46:
        fails.append("J2: 654 在 46 之前（i654=%d i46=%d）" % (i654[0], i46))

    # J3：42 恰两条 + 657 恰两条且位置
    if len(i42) != 2:
        fails.append("J3: 42 恰好 2 条，实际 %d 条 %s" % (len(i42), i42))
    if len(i657) != 2:
        fails.append("J3: 657 恰好 2 条，实际 %d 条 %s" % (len(i657), i657))
    if len(i654) == 1 and len(i657) == 2 and len(i42) == 2:
        if not (i46 < i654[0] < i657[0] < i657[1] < i42[1]):
            fails.append("J2/J3 次序不符：需 i46 < i654 < 657[0] < 657[1] < i42[1]，实际 "
                         "i46=%d i654=%d i657=%s i42=%s" %
                         (i46, i654[0], i657, i42))
    # J4：第二 42 之后恰 100×4
    if len(i42) == 2:
        tail = seq[i42[1] + 1:i42[1] + 5]
        facts["after_second_42"] = seq[i42[1] + 1:i42[1] + 9]
        if tail != [100, 100, 100, 100]:
            fails.append("J4: 第二 42 之后不是 100×4，实际 %s" % tail)
    return fails, facts


def classify_j5(report):
    """§3.3 的判别口径。返回 (status, detail)。status ∈ GREEN|KNOWN_PENDING|RED。"""
    if report.get("pass") and report.get("diff_count") == 0:
        return "GREEN", ["同序列逐字节 diff=0（50/51 param 差异消失 => 上一轮是输入漂移）"]
    diffs = report.get("diffs") or []
    wl = report.get("whitelisted") or []
    keys = {(d.get("ident"), d.get("field")) for d in diffs}
    wlkeys = {(w.get("ident"), w.get("field")) for w in wl}
    if (keys == KNOWN_PENDING_DIFFS and
            wlkeys == D9_WHITELIST_KEYS and
            report.get("diff_count") == 4 and report.get("whitelisted_count") == 3):
        return "KNOWN_PENDING", [
            "恰为已登记 4 条 50/51 差异 + 3 条 52/D-9 白名单 => **RED（待查）**，"
            "不得写成 GREEN（runbook §3.3）"]
    extra = sorted(keys - KNOWN_PENDING_DIFFS)
    missing = sorted(KNOWN_PENDING_DIFFS - keys)
    detail = []
    if extra:
        detail.append("未登记/未知差异：%s" % extra)
    if missing:
        detail.append("已知 4 条差异不齐（少 %s）；若两侧起点不同请重跑" % missing)
    if wlkeys != D9_WHITELIST_KEYS:
        detail.append("52/D-9 白名单未命中全部 3 条：实际 %s" % sorted(wlkeys))
    if not detail:
        detail.append("diff 集合不在已裁决口径内")
    return "RED", detail


def run_j5(capture, golden, whitelist, out_path, python_exe=None, report_override=None):
    """返回 (report, err)。report_override 供离线自检。"""
    if report_override is not None:
        return report_override, None
    tool = os.path.join(REPO_ROOT, S2C_DIFF_REL)
    if not os.path.isfile(tool):
        return None, "缺 %s" % tool
    cmd = [python_exe or sys.executable, "-I", "-B", tool,
           "--rust", capture, "--golden", golden,
           "--start-ident", "658", "--upto-ident", "46",
           "--whitelist", whitelist, "--out", out_path]
    try:
        p = subprocess.run(cmd, capture_output=True, text=True, timeout=180,
                           encoding="utf-8", errors="replace")
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, "s2c_diff 调用失败：%s" % e
    if p.returncode == 2:
        return None, "s2c_diff PREREQ：%s" % (p.stderr or p.stdout).strip()[:500]
    if not os.path.isfile(out_path):
        return None, "s2c_diff 未产出 report（rc=%s）：%s" % (p.returncode,
                                                             (p.stdout or "").strip()[-500:])
    with open(out_path, "r", encoding="utf-8") as f:
        return json.load(f), None


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def judge(args):
    checks = []
    scenarios = load_scenarios(REPO_ROOT)
    sc = scenarios["scenarios"].get(args.scenario)
    if sc is None:
        print("PREREQ_RED: 未知 scenario=%s（见 scenarios.json）" % args.scenario)
        return 2
    kind = sc.get("judge", {}).get("kind")
    if kind != "login":
        print("PREREQ_RED: scenario=%s 的 judge.kind=%r 不是 login；用 run_scenario.ps1 -Phase judge"
              % (args.scenario, kind))
        return 2

    j = sc["judge"]
    # scenarios.json 里是 JSON 数组；上游只吃字符串 —— 在这里统一规范化（见 enter_seq_spec）。
    expect_seq = enter_seq_spec(args.expect_enter_seq or j.get("expect_enter_seq"))
    layout_map = j.get("expect_52_layout", {})
    layout = args.expect_52_layout or layout_map.get(args.side)

    # ---- J0：输入 ----
    c0 = Check("J0")
    rows = []
    if not os.path.isfile(args.capture):
        c0.fail("capture 不存在：%s" % args.capture, prereq=True)
    else:
        try:
            with open(args.capture, "r", encoding="utf-8") as f:
                rows = [json.loads(line) for line in f if line.strip()]
        except (OSError, ValueError) as e:
            c0.fail("capture 解析失败：%s" % e, prereq=True)
    if rows and not [r for r in rows if r.get("dir") == "s2c"
                    and r.get("port", 7200) == 7200]:
        c0.fail("capture 里没有 port=7200 dir=s2c 的帧（先跑 export_s2c.ps1，"
                "不要喂 proxy 临时 dump）", prereq=True)
    if rows:
        c0.info("rows=%d s2c7200=%d" % (len(rows), len(s2c_idents(rows))))
    checks.append(c0)
    if c0.status != "GREEN":
        return finish(args, checks, scenarios, sc, None)

    # ---- J1：入图序 / 52 / 752 ----
    c1 = Check("J1")
    try:
        chk = load_checker(REPO_ROOT)
        fails, facts = chk.check(rows, expect_enter_seq=expect_seq,
                                 expect_52_layout=layout)
    except (OSError, ValueError) as e:
        c1.fail("J1 前置/参数错误：%s" % e, prereq=True)
        checks.append(c1)
        return finish(args, checks, scenarios, sc, None)
    if fails:
        for f in fails:
            c1.fail(f)
    else:
        c1.info("enter_seq=%s" % ",".join(str(x) for x in facts["enter_seq"]))
        c1.info("52_present=%s body_len=%s 752_present=%s" %
                (facts["52_present"], facts["52_body_len"], facts["752_present"]))
    checks.append(c1)
    j1_facts = facts

    # ---- J2..J4：仅 G1 ----
    tail_facts = None
    if j.get("login_tail"):
        fails, tail_facts = login_tail(rows)
        c2 = Check("J2")
        c3 = Check("J3")
        c4 = Check("J4")
        # 把 login_tail 的失败按判据归位
        for f in fails:
            if f.startswith("J2"):
                c2.fail(f)
            elif f.startswith("J3"):
                c3.fail(f)
            elif f.startswith("J4"):
                c4.fail(f)
            else:
                c2.fail(f)
                c3.fail(f)
        if not fails:
            c2.info("i46=%d i654=%s" % (tail_facts["i46"], tail_facts["i654s"]))
            c3.info("i657=%s i42=%s" % (tail_facts["i657s"], tail_facts["i42s"]))
            c4.info("after_second_42=%s" % tail_facts.get("after_second_42"))
        checks.extend([c2, c3, c4])

    # ---- J5：同序列 diff ----
    c5 = Check("J5")
    report = None
    if args.no_j5:
        c5.info("--no-j5：跳过")
    elif not args.golden:
        c5.fail("J5 需要 --golden <本轮 C# capture>（不得用静态金标准）", prereq=True)
    else:
        if not os.path.isfile(args.whitelist):
            c5.fail("白名单不存在：%s" % args.whitelist, prereq=True)
        else:
            out = args.j5_out or os.path.join(os.path.dirname(os.path.abspath(args.out or ".")),
                                              "j5-%s-%s.json" % (args.scenario, args.side))
            os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
            report, err = run_j5(args.capture, args.golden, args.whitelist, out,
                                 report_override=(args.j5_report or None))
            if err:
                c5.fail(err, prereq=True)
            else:
                st, detail = classify_j5(report)
                for d in detail:
                    (c5.info if st == "GREEN" else c5.fail)(d)
                c5.info("J5_STATUS=%s" % st)
                c5.j5_status = st
    checks.append(c5)
    return finish(args, checks, scenarios, sc, report)


def finish(args, checks, scenarios, sc, j5_report):
    any_red = any(c.status == "RED" for c in checks)
    any_prereq = any(c.status == "PREREQ_RED" for c in checks)
    verdict = "RED" if any_red else ("PREREQ_RED" if any_prereq else "GREEN")
    for c in checks:
        print("%s %s" % (c.jid, c.status))
        for d in c.detail:
            print("  " + d)
    print("RESULT: %s" % verdict)
    if args.out:
        os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
        payload = {
            "tool": "window-20261023/judge_login.py",
            "scenario": args.scenario,
            "side": args.side,
            "capture": os.path.abspath(args.capture) if args.capture else None,
            "golden": os.path.abspath(args.golden) if args.golden else None,
            "whitelist": os.path.abspath(args.whitelist) if args.whitelist else None,
            "checks": [{"id": c.jid, "status": c.status, "detail": c.detail}
                       for c in checks],
            "j5_report": j5_report,
            "result": verdict,
        }
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(payload, f, ensure_ascii=False, indent=1)
        print("REPORT %s" % args.out)
    return 0 if verdict == "GREEN" else (1 if verdict == "RED" else 2)


# ---------------------------------------------------------------------------
# 离线自检
# ---------------------------------------------------------------------------
def selftest():
    ok = True
    n = [0]

    def expect(name, got, want):
        n[0] += 1
        nonlocal ok
        if got != want:
            ok = False
            print("FAIL %s：got=%r want=%r" % (name, got, want))
        else:
            print("ok   %s" % name)

    # 0) 上游判据工具自检（离线、无外部件）——证明复用的 check()/布局表没漂
    chk = load_checker(REPO_ROOT)
    print("-- upstream check_client_s2c_frames.selftest --")
    expect("上游 check_client_s2c_frames --selftest rc=0", chk.selftest(), 0)

    # 1) G1 合成流（40B oracle 布局 + 登录尾）
    base = chk.synthetic_ability(40)
    for r in base:
        r["port"] = 7200
        r["kind"] = "frame"
    tail = [
        chk.frame_row(654, b"\x01\x02\x03\x04", seq=15),
        chk.frame_row(657, b"\x05\x06", seq=16),
        chk.frame_row(657, b"\x07\x08", seq=17),
        chk.frame_row(42, b"second-refshow", seq=18),
        chk.frame_row(100, b"a", seq=19),
        chk.frame_row(100, b"b", seq=20),
        chk.frame_row(100, b"c", seq=21),
        chk.frame_row(100, b"d", seq=22),
    ]
    for r in tail:
        r["port"] = 7200
    rows = base + tail
    fails, facts = login_tail(rows)
    expect("干净登录尾 J2..J4 全绿", fails, [])
    expect("干净登录尾 i46=14", facts["i46"], 14)
    expect("干净登录尾 i654=15", facts["i654s"], [15])
    expect("干净登录尾 i657=[16,17]", facts["i657s"], [16, 17])
    expect("干净登录尾 i42=[6,18]", facts["i42s"], [6, 18])

    # 2) J1 走上游 check()：40B+oracle40 GREEN / 40B+client58 RED / 58B+client58 GREEN
    f, _ = chk.check(rows, expect_enter_seq="default", expect_52_layout="oracle40")
    expect("J1 40B + oracle40 => GREEN", f, [])
    f, _ = chk.check(rows, expect_enter_seq="default", expect_52_layout="client58")
    expect("J1 40B + client58 => RED", bool(f), True)
    rows58 = []
    for r in chk.synthetic_ability(58):
        r["port"] = 7200
        rows58.append(r)
    rows58 += tail
    f, _ = chk.check(rows58, expect_enter_seq="default", expect_52_layout="client58")
    expect("J1 58B + client58 => GREEN", f, [])

    # 2b) scenarios.json 的 JSON 数组必须能直接喂给上游 check()（回归 Critical 1）
    sc_all = load_scenarios(REPO_ROOT)["scenarios"]
    spec_g1 = enter_seq_spec(sc_all["G1"]["judge"]["expect_enter_seq"])
    expect("enter_seq_spec(list) 是逗号字符串", isinstance(spec_g1, str), True)
    expect("enter_seq_spec(list) 与基线 15 帧一致",
           spec_g1, ",".join(str(x) for x in ENTER_SEQ))
    f, _ = chk.check(rows, expect_enter_seq=spec_g1, expect_52_layout="oracle40")
    expect("scenarios G1 数组 => check() 可消费且 GREEN", f, [])
    expect("enter_seq_spec 保留字符串可选标记",
           chk.parse_enter_seq(enter_seq_spec(["658", "51?"])),
           [(658, False), (51, True)])

    # 3) 变异：删 654 ⇒ J2 红
    m = [dict(r) for r in rows if not (r.get("ident") == 654)]
    f, _ = login_tail(m)
    expect("删 654 => J2 红", (bool(f), any("J2" in x for x in f)), (True, True))

    # 4) 变异：删第二 42 ⇒ J3 红
    seen = 0
    m = []
    for r in rows:
        if r.get("ident") == 42:
            seen += 1
            if seen == 2:
                continue
        m.append(r)
    f, _ = login_tail(m)
    expect("删第二 42 => J3 红", (bool(f), any("J3" in x for x in f)), (True, True))

    # 4b) 变异：删一条 657 ⇒ J3 红（657 恰两条门）
    seen = 0
    m = []
    for r in rows:
        if r.get("ident") == 657:
            seen += 1
            if seen == 2:
                continue
        m.append(r)
    f, _ = login_tail(m)
    expect("删一条 657 => J3 红", (bool(f), any("J3" in x for x in f)), (True, True))

    # 5) 变异：100 只有 3 条 ⇒ J4 红
    m = [r for r in rows if not (r.get("ident") == 100 and r.get("seq") == 22)]
    f, _ = login_tail(m)
    expect("100×3 => J4 红", (bool(f), any("J4" in x for x in f)), (True, True))

    # 6) 变异：654 挪到 46 之前 ⇒ J2 红
    m = [dict(r) for r in rows]
    i654 = next(i for i, r in enumerate(m) if r.get("ident") == 654)
    moved = m.pop(i654)
    i46 = next(i for i, r in enumerate(m) if r.get("ident") == 46)
    m.insert(i46, moved)
    f, _ = login_tail(m)
    expect("654 在 46 前 => J2 红", (bool(f), any("J2" in x for x in f)), (True, True))

    # 7) act 行（无 ident）混入不得崩，且不影响判据
    m = []
    for r in rows:
        m.append(r)
        if r.get("ident") == 654:
            m.append({"kind": "frame", "dir": "s2c", "port": 7200,
                      "frame_form": "act", "raw_hex": "232b47442f3121"})
    f, _ = login_tail(m)
    expect("混入 frame_form=act 无 ident 行 => 不崩且全绿", f, [])

    # 8) J5 判别口径
    green = {"pass": True, "diff_count": 0, "whitelisted_count": 3,
             "diffs": [], "whitelisted": []}
    expect("J5 pass => GREEN", classify_j5(green)[0], "GREEN")
    known = {
        "pass": False, "diff_count": 4, "whitelisted_count": 3,
        "diffs": [{"ident": 51, "field": "param"}, {"ident": 51, "field": "raw_hex"},
                  {"ident": 50, "field": "param"}, {"ident": 50, "field": "raw_hex"}],
        "whitelisted": [{"ident": 52, "field": "body_len"},
                        {"ident": 52, "field": "body_sha256"},
                        {"ident": 52, "field": "raw_hex"}],
    }
    expect("J5 恰为已登记 4+3 => KNOWN_PENDING", classify_j5(known)[0], "KNOWN_PENDING")
    extra = json.loads(json.dumps(known))
    extra["diffs"].append({"ident": 52, "field": "body_len"})
    extra["diff_count"] = 5
    expect("J5 多一条差异 => RED", classify_j5(extra)[0], "RED")
    nowl = json.loads(json.dumps(known))
    nowl["whitelisted"] = []
    nowl["whitelisted_count"] = 0
    expect("J5 白名单未命中 => RED", classify_j5(nowl)[0], "RED")
    reorder = json.loads(json.dumps(known))
    reorder["diffs"] = [{"ident": 52, "field": "body_len", "kind": "order"}]
    reorder["diff_count"] = 1
    expect("J5 次序分叉 => RED", classify_j5(reorder)[0], "RED")

    # 9) 场景表：G1/G2a 的期望序列与 runbook 逐字一致
    sc = load_scenarios(REPO_ROOT)["scenarios"]
    expect("G1 期望序列 = 基线 15 帧", sc["G1"]["judge"]["expect_enter_seq"], ENTER_SEQ)
    expect("G2A-R1 期望含 210 且 658 后",
           sc["G2A-R1"]["judge"]["expect_enter_seq"][:2], [658, 210])
    expect("G2A-R2 期望含 212 且 658 后",
           sc["G2A-R2"]["judge"]["expect_enter_seq"][:2], [658, 212])
    expect("G2A-R3 期望 621,211 在 752 后 811 前",
           sc["G2A-R3"]["judge"]["expect_enter_seq"][-4:], [621, 211, 811, 46])
    expect("G2a 布局：C# oracle40 / Rust client58",
           (sc["G2A-R1"]["judge"]["expect_52_layout"]["csharp"],
            sc["G2A-R1"]["judge"]["expect_52_layout"]["rust"]),
           ("oracle40", "client58"))

    # 10) 随包白名单：3 条 52/D-9，逐条带 reason + reviewer
    wl_path = os.path.join(HERE, WHITELIST_NAME)
    expect("随包白名单存在", os.path.isfile(wl_path), True)
    if os.path.isfile(wl_path):
        entries = [json.loads(l) for l in open(wl_path, encoding="utf-8") if l.strip()]
        keys = {(e.get("ident"), e.get("field")) for e in entries}
        expect("白名单键 = 52/body_len+body_sha256+raw_hex", keys, D9_WHITELIST_KEYS)
        expect("白名单每条都有 reason+reviewer",
               all(e.get("reason") and e.get("reviewer") for e in entries), True)

    # 11) 退出码映射（通过 finish 的 verdict 计算）
    class A:
        out = ""
        scenario = "G1"
        side = "rust"
        capture = ""
        golden = ""
        whitelist = ""
    def verdict_of(statuses):
        cs = []
        for i, s in enumerate(statuses):
            c = Check("J%d" % i)
            c.status = s
            cs.append(c)
        import io
        from contextlib import redirect_stdout
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = finish(A(), cs, {}, {}, None)
        return rc
    expect("全绿 rc=0", verdict_of(["GREEN", "GREEN"]), 0)
    expect("RED rc=1", verdict_of(["GREEN", "RED"]), 1)
    expect("PREREQ rc=2", verdict_of(["GREEN", "PREREQ_RED"]), 2)

    print("SELFTEST %s (%d assertions)" % ("GREEN" if ok else "RED", n[0]))
    return 0 if ok else 1


def build_parser():
    ap = argparse.ArgumentParser(description="G1/G2a 登录判据 J0..J5（tier=L 夹具）")
    ap.add_argument("--scenario", default="G1", help="G1 / G2A-R1 / G2A-R2 / G2A-R3")
    ap.add_argument("--side", default="rust", choices=["csharp", "rust"])
    ap.add_argument("--capture", default="")
    ap.add_argument("--golden", default="", help="本轮 C# capture（J5 用；不得用静态金标准）")
    ap.add_argument("--whitelist", default=os.path.join(HERE, WHITELIST_NAME))
    ap.add_argument("--expect-enter-seq", default="", help="覆盖 scenarios.json 的期望序列")
    ap.add_argument("--expect-52-layout", default="", choices=["", "oracle40", "client58"])
    ap.add_argument("--no-j5", action="store_true")
    ap.add_argument("--j5-out", default="")
    ap.add_argument("--j5-report", default="", help="（离线复算）直接吃 s2c_diff report JSON")
    ap.add_argument("--out", default="")
    ap.add_argument("--selftest", action="store_true")
    return ap


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.selftest:
        return selftest()
    if args.j5_report:
        with open(args.j5_report, "r", encoding="utf-8") as f:
            args.j5_report = json.load(f)
    return judge(args)


if __name__ == "__main__":
    sys.exit(main())
