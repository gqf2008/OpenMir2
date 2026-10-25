#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""tool_selftest_gate — 工具自测纳入合并门禁（卡 `openmir2-tool-selftest-gate`）。

背景（2026-10-25 实证）：`card_window --selftest` 与 `card_evidence --selftest` 不在
`goal_gate` 的 7 项门禁里 ⇒ `db-makeindex-mask` 合入后 master 带着
`card_window --selftest 412/435` 的工具回归，而合并门禁全绿。本门禁把**工具自测**
（含各 hygiene 门自带的 selftest）变成可机判的一道门。

**判定域（必须写清）**：
  * 扫什么：下表中的**固定注册项**（`REGISTRY`），每项 = 一个可复跑的自测命令；
  * 在哪跑：仓库根（`--repo`，默认本脚本上溯两级），`python -I -B`，每项独立子进程；
  * rc 从哪取：子进程退出码（不经管道/`tail`，避免取到 tail 的 rc）；
  * 判定：任一注册项 rc≠0 ⇒ 本门 RED（rc=2）；全 0 ⇒ GREEN（rc=0）；用法错误 ⇒ rc=2。
  * `--skip <name,...>` 必须**打印 SKIP**（不许静默）；`--only` 用于变异自证/快跑。

**收哪些、不收哪些**（`--inventory` 可看全量清点）：**只要自带 selftest 且能离线判红，就收**
——不按「文件在哪个目录 / 是不是别域 / 是不是一次性脚本」筛（Leader 2026-10-25 裁定：
「一次性 / 人工使用 / 别域」都不是合法出域理由，除非工具**真的需要交互式输入、外部产物或真实环境**）。
当前 43 项在门内；出域只有 6 项，逐项理由见 `OUT_OF_SCOPE`（外部截图产物 / 需要真库 / 需要构建产物/
部署配置 / 无独立入口的模块）。新增工具按 `REGISTRY` 同一格式登记即可。

用法：
  python -I -B tools/hygiene/tool_selftest_gate.py [--repo .] [--only a,b]
         [--skip a,b] [--timeout 300] [--json out.json] [--inventory] [--selftest]
退出码：0 全绿（含被 --skip 的项已打印 SKIP）；2 有红/用法错。
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time

#: (name, argv 相对仓库根, slow?) —— 顺序即执行顺序；slow 项可被 `--skip` 显式跳过。
REGISTRY = [
    ("card_window", ["tools/conformance/card_window.py", "--selftest"], True),
    ("card_evidence", ["tools/window/card_evidence.py", "--selftest"], False),
    ("compare_runs", ["tools/conformance/compare_runs.py", "selftest"], False),
    ("dbsnap_pair", ["tools/db-parity/dbsnap_pair.py", "selftest"], False),
    ("mir2_probe", ["tools/probe/mir2_probe.py", "--selftest"], True),
    ("butch_ledger", ["tools/window/butch_ledger.py", "--selftest"], False),
    ("rc_attribution", ["tools/window/rc_attribution.py", "--selftest"], False),
    ("window_criteria_lint", ["tools/goal/window_criteria_lint.py", "--selftest"], False),
    ("blocked_sources_check", ["tools/window/blocked_sources_check.py", "selftest"], False),
    ("goal_gate", ["tools/goal/goal_gate.py", "selftest"], False),
    ("conflict_marker_gate", ["tools/hygiene/conflict_marker_gate.py", "--selftest"], False),
    ("dynamic_count_stamp_gate", ["tools/hygiene/dynamic_count_stamp_gate.py", "--selftest"], False),
    ("merge_tree_helper", ["tools/hygiene/merge_tree_helper.py", "--selftest"], False),
    ("stale_pointer_gate", ["tools/hygiene/stale_pointer_gate.py", "--selftest"], False),
    ("board_coverage_gate", ["tools/hygiene/board_coverage_gate.py", "--selftest"], False),
    ("t_reservation_gate", ["tools/whitelist/t_reservation_gate.py", "selftest"], False),
    ("next_t", ["tools/whitelist/next_t.py", "selftest"], False),
    # dbsnap 的自测会连**本机 MySQL** 读 13 张库表（mir2_account.* / mir2_db.*，mysql.exe 路径硬编码）
    # ⇒ 无库环境下的红属**环境**而非工具回归（runbook §7「判定域」F2）。
    ("dbsnap", ["tools/dbsnap/dbsnap.py", "selftest"], False),
    ("param_discovery", ["tools/probe/param_discovery.py", "selftest"], False),
    ("mir2frame", ["tools/probe/mir2frame.py", "selftest"], False),
    ("frame_drift_report", ["tools/window/frame_drift_report.py", "--selftest"], False),
    ("check_client_ability_layout_doc_counts",
     ["tools/m2-acceptance/check_client_ability_layout_doc_counts.py", "--selftest"], False),
    ("verify_sha256sums", ["tools/evidence-hash/verify_sha256sums.py", "selftest"], False),
    # --- 2026-10-25 扩容（Leader 裁定：「一次性/人工使用」不是出域理由；离线可判红的一律接入）---
    # 每一项都在本机实测：离线、确定性、rc 为 pass/fail（见 runbook §7 的表）。
    ("review_principal_check", ["tools/hygiene/review_principal_check.py", "--selftest"], False),
    ("scenario_undecidable_audit", ["tools/hygiene/scenario_undecidable_audit.py", "selftest"], False),
    ("rules_approve", ["tools/conformance/rules_approve.py", "--selftest"], False),
    ("capture_login_compare", ["tools/loginfixture/capture_login_compare.py", "--selftest"], False),
    ("s2c_diff", ["tools/m2-acceptance/s2c_diff.py", "--selftest"], False),
    ("error_curve", ["tools/logdiff/error_curve.py", "selftest"], False),
    ("timeline", ["tools/logdiff/timeline.py", "selftest"], False),
    ("world_sampler", ["tools/worldsample/world_sampler.py", "selftest"], False),
    ("preflight", ["tools/conformance/preflight.py", "--selftest"], False),
    ("gate_shadow_proxy", ["tools/gw-capture/gate_shadow_proxy.py", "--selftest"], False),
    ("gate_timer_gate", ["tools/gw-capture/gate_timer_gate.py", "--selftest"], False),
    ("count_m1_logs", ["tools/m1-acceptance/count_m1_logs.py", "--selftest"], False),
    ("selftest_proxy", ["tools/capture/selftest_proxy.py", "--selftest"], False),
    ("selftest_gate_shadow", ["tools/gw-capture/selftest_gate_shadow.py", "--selftest"], False),
    ("load_report", ["tools/botload/load_report.py", "selftest"], False),
    ("prepare_safe_login", ["tools/loginfixture/prepare_safe_login.py", "--selftest"], False),
    ("preflight_private_stack", ["tools/window/preflight_private_stack.py", "--selftest"], False),
    ("selftest_mutations", ["tools/conformance/selftest_mutations.py", "--selfcheck"], False),
    ("startup_param", ["tools/window/startup_param.py", "selftest"], False),
    ("tool_selftest_gate", ["tools/hygiene/tool_selftest_gate.py", "--selftest"], False),
]

#: 清点用：仓库内其余带 selftest 的工具 → 为何**不**纳入本门禁（人工复核过的分类）。
OUT_OF_SCOPE = {
    # 2026-10-25 Leader 裁定后重审：只保留**真的无法离线自动判红**的 7 项，理由必须是
    # 「需要交互/外部产物/真实环境」，不能是「一次性/人工使用/别域」（那等于垃圾桶）。
    "tools/client/dlg_links.py":
        "selftest 需外部截图 E:/tmp/window/r7/shot-dice5.png；缺失时打印 SKIP 且 rc=0 ⇒ 接入会永远假绿（要接须先有人给该产物或改工具）",
    "tools/window/alt_port_profile.py":
        "`selftest` 需要**主检出构建产物/部署配置** `src/DBSrv/bin/Release/dbsvr.conf`（实测 `RED: db-srv: 找不到源配置，候选 …` ⇒ rc=1）；"
        "干净 CI 上没有该产物 ⇒ 红属环境、不能当工具回归（不是「只打印」）",
    "tools/botload/in_world_stability.py":
        "无 selftest 入口：`stats <file>` 需要真实 load_stats.ndjson 产物 ⇒ 无法离线自动判红",
    "tools/capture/golden_fresh_check.py":
        "需要先 `dotnet build` 出 GoldenExport.exe（实测 rc=2「前置不满足」）⇒ 依赖外部构建产物",
    "tools/conformance/selftest_impl.py":
        "无独立 CLI 入口（`--selftest` 空转 rc=0、无输出）：它是 `compare_runs selftest` 的实现模块，已被 compare_runs 覆盖 ⇒ 单列会假绿",
    "tools/db-parity/newchr_delchr_selchr_probe.py":
        "需要真实 MySQL 与建/删/选角色状态（真机），不能离线判红",
}


def repo_root(explicit=None):
    if explicit:
        return os.path.abspath(explicit)
    return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def discover():
    """扫 tools/**/*.py 里带 selftest 的文件（清点用；不执行）。"""
    root = repo_root()
    out = []
    for base, _dirs, files in os.walk(os.path.join(root, "tools")):
        if "__pycache__" in base:
            continue
        for fn in files:
            if not fn.endswith(".py"):
                continue
            path = os.path.join(base, fn)
            try:
                text = open(path, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            if "--selftest" in text or "selftest" in text:
                out.append(os.path.relpath(path, root).replace(os.sep, "/"))
    return sorted(out)


def run_one(repo, name, argv, timeout):
    cmd = [sys.executable, "-I", "-B"] + list(argv)
    t0 = time.time()
    try:
        p = subprocess.run(cmd, cwd=repo, capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=timeout)
        rc = p.returncode
        tail = ((p.stdout or "") + (p.stderr or "")).strip().splitlines()[-1:] or [""]
    except subprocess.TimeoutExpired:
        rc = 124
        tail = ["TIMEOUT %ss" % timeout]
    elapsed = time.time() - t0
    return {"name": name, "argv": list(argv), "rc": rc, "elapsed_s": round(elapsed, 1),
            "tail": tail[0][:160]}


def run(repo, only=None, skip=None, timeout=300):
    names = {n for n, _a, _s in REGISTRY}
    only = set(only or [])
    skip = set(skip or [])
    unknown = (only | skip) - names
    if unknown:
        raise SystemExit("tool_selftest_gate: 未知条目 %s（可用：%s）"
                         % (sorted(unknown), sorted(names)))
    results = []
    for name, argv, _slow in REGISTRY:
        if only and name not in only:
            continue
        if name in skip:
            print("SKIP %-24s（--skip 显式跳过）" % name)
            results.append({"name": name, "skipped": True})
            continue
        r = run_one(repo, name, argv, timeout)
        results.append(r)
        print("%-4s %-24s rc=%-3s %5.1fs  %s"
              % ("PASS" if r["rc"] == 0 else "FAIL", name, r["rc"],
                 r["elapsed_s"], r["tail"]))
    bad = [r for r in results if not r.get("skipped") and r["rc"] != 0]
    print("TOTAL %d 项：绿 %d / 红 %d / 跳过 %d"
          % (len(results), sum(1 for r in results if not r.get("skipped") and r["rc"] == 0),
             len(bad), sum(1 for r in results if r.get("skipped"))))
    return results, (2 if bad else 0)


def inventory(repo):
    disc = discover()
    in_gate = {a[0] for _n, a, _s in REGISTRY}
    rows = []
    for rel in disc:
        if rel in in_gate:
            status = "IN_GATE"
        elif rel in OUT_OF_SCOPE:
            status = "OUT_OF_SCOPE: " + OUT_OF_SCOPE[rel]
        else:
            status = "UNCLASSIFIED"
        rows.append({"tool": rel, "status": status})
    for r in rows:
        print("%-12s %s" % (r["status"].split(":")[0], r["tool"])
              + (("  （%s）" % r["status"].split(": ", 1)[1]) if ": " in r["status"] else ""))
    print("清点：%d 个带 selftest 的工具；门禁内 %d；已分类出域 %d；未分类 %d"
          % (len(rows), sum(1 for r in rows if r["status"] == "IN_GATE"),
             sum(1 for r in rows if r["status"].startswith("OUT_OF_SCOPE")),
             sum(1 for r in rows if r["status"] == "UNCLASSIFIED")))
    return rows


def selftest():
    global REGISTRY
    ok = fail = 0

    def chk(cond, name):
        nonlocal ok, fail
        if cond:
            ok += 1
            print("PASS " + name)
        else:
            fail += 1
            print("FAIL " + name)

    root = repo_root()
    names = [n for n, _a, _s in REGISTRY]
    chk(len(names) == len(set(names)), "TSG1 注册项名字唯一")
    missing = [a[0] for _n, a, _s in REGISTRY if not os.path.exists(os.path.join(root, a[0]))]
    chk(not missing, "TSG2 注册项文件都存在（缺 %r）" % missing)
    tmp = tempfile.mkdtemp(prefix="tsg-selftest-")
    okf = os.path.join(tmp, "ok.py")
    badf = os.path.join(tmp, "bad.py")
    with open(okf, "w", encoding="utf-8") as fh:
        fh.write("print('GREEN fake 1/1')\n")
    with open(badf, "w", encoding="utf-8") as fh:
        fh.write("print('RED fake 0/1')\nraise SystemExit(1)\n")
    r_ok = run_one(tmp, "ok", [okf], 30)
    chk(r_ok["rc"] == 0, "TSG 牙1 假·绿 ⇒ rc=0")
    r_bad = run_one(tmp, "bad", [badf], 30)
    chk(r_bad["rc"] == 1 and "RED" in r_bad["tail"], "TSG 牙2 假·红 ⇒ rc=1（门会判红）")
    saved = REGISTRY
    REGISTRY = [("ok", [okf], False), ("bad", [badf], False)]
    try:
        import io
        from contextlib import redirect_stdout
        buf = io.StringIO()
        with redirect_stdout(buf):
            _res, rc = run(tmp)
        chk(rc == 2, "TSG 牙3 一绿一红 ⇒ 门 rc=2")
        buf = io.StringIO()
        with redirect_stdout(buf):
            _res, rc = run(tmp, skip=["bad"])
        out = buf.getvalue()
        chk(rc == 0 and "SKIP bad" in out,
            "TSG 牙4 --skip 打印 SKIP 且不放水（rc=0 仅因显式跳过）")
        buf = io.StringIO()
        with redirect_stdout(buf):
            _res, rc = run(tmp, only=["ok"])
        chk(rc == 0 and "ok" in buf.getvalue() and "bad" not in buf.getvalue(),
            "TSG 牙5 --only 只跑指定项")
        # 牙6：未知条目 ⇒ rc=2（用法错不许被当成「有红」，也不许假绿）
        import contextlib
        with contextlib.redirect_stderr(io.StringIO()):
            r6 = main(["--repo", tmp, "--only", "no-such-entry"])
            r6b = main(["--repo", tmp, "--skip", "no-such-entry"])
        chk(r6 == 2, "TSG 牙6 未知条目 ⇒ rc=2（用法错）")
        chk(r6b == 2, "TSG 牙6b --skip 未知条目 ⇒ rc=2")
    finally:
        REGISTRY = saved
    print("%s: tool_selftest_gate selftest %d/%d"
          % ("GREEN" if not fail else "RED", ok, ok + fail))
    return 0 if not fail else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description="工具自测门禁（跑注册表里的 selftest）")
    ap.add_argument("--repo", default=None)
    ap.add_argument("--only", default="")
    ap.add_argument("--skip", default="")
    ap.add_argument("--timeout", type=int, default=300)
    ap.add_argument("--json", default=None)
    ap.add_argument("--inventory", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    repo = repo_root(args.repo)
    if args.inventory:
        inventory(repo)
        return 0
    only = [x for x in args.only.split(",") if x]
    skip = [x for x in args.skip.split(",") if x]
    known = {n for n, _a, _s in REGISTRY}
    unknown = sorted((set(only) | set(skip)) - known)
    if unknown:
        # 用法错 ⇒ rc=2（F3：此前 raise SystemExit(str) 会得到 rc=1，与文档不一致）
        print("tool_selftest_gate: 未知条目 %s（可用：--inventory 或 REGISTRY）"
              % ", ".join(unknown), file=sys.stderr)
        return 2
    results, rc = run(repo, only=only, skip=skip, timeout=args.timeout)
    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump({"format": "tool-selftest-gate/1", "repo": repo,
                       "results": results, "rc": rc}, fh, ensure_ascii=False, indent=2)
    return rc


if __name__ == "__main__":
    sys.exit(main())
