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
  * **门自己必须先能超时退出**：单项超时按**进程树**终止（Windows `taskkill /T /F`、POSIX `killpg`），
    再以 10s 兜底 `communicate()`；另有**总墙钟**（`--total-timeout`，默认 420s < CI 480s）——
    超时/兜底 ⇒ rc=2 并**点名滞留项**，绝不允许「挂住」冒充「跑完」。牙 TSG7/8/9/10 钉住。

**收哪些、不收哪些**（`--inventory` 可看全量清点）：**只要自带 selftest 且能离线判红，就收**
——不按「文件在哪个目录 / 是不是别域 / 是不是一次性脚本」筛（Leader 2026-10-25 裁定：
「一次性 / 人工使用 / 别域」都不是合法出域理由，除非工具**真的需要交互式输入、外部产物或真实环境**）。
门内条数 = `REGISTRY` 的长度（**不在这里写死**；`--inventory` 会打印实时条数与出域理由）。
出域项逐条列在 `OUT_OF_SCOPE`（外部截图产物 / 需要真库 / 需要构建产物 / 部署配置 / 无独立入口的模块）。
新增工具按 `REGISTRY` 同一格式登记即可；**非 Python 的自测**（如 `-Selftest` 的 .ps1）同样登记，
由 `build_cmd()` 按扩展名拼出 PowerShell 命令行。

用法：
  python -I -B tools/hygiene/tool_selftest_gate.py [--repo .] [--only a,b]
         [--skip a,b] [--timeout 300] [--json out.json] [--inventory] [--selftest]
退出码：**0 全绿**（含被 --skip 的项已打印 SKIP）；**3 有「需人工」（NEED）但无红**；**2 有红/用法错**。

三态口径（卡 `openmir2-docs-gate-rc-semantics`）：工具自己用 **rc=2 表示「判不了」**（环境/仓库问题，不是证伪）⇒ 本门记 **NEED**、单独计数、**不并进「红」**；其余非零（含超时 124）才是 **FAIL**。「判不了 ≠ 证伪」——把两者折叠会教人「红了重跑一下」，那会毁掉门禁的可信度。
"""
import argparse
import json
import os
import shutil
import signal
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
    ("selftest_bclass", ["tools/probe/selftest_bclass.py"], False),
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
    # 卡 openmir2-docs-gate-rc-semantics（Leader 2026-10-27 批准扩写集）：窗口期两个 PowerShell
    # 自测此前**不在任何门禁里**。二者都离线、确定性、自带 `-Selftest`（stop-mysql 用注入 hook +
    # 临时 mysqladmin.cmd；invoke-bounded 用临时子进程），且实测**不触碰真实 mysqld**（跑完 pid/StartTime 不变）。
    ("stop_mysql_graceful", ["tools/window/stop-mysql-graceful.ps1", "-Selftest"], False),
    ("invoke_bounded", ["tools/window/invoke-bounded.ps1", "-Selftest"], False),
    ("selftest_mutations", ["tools/conformance/selftest_mutations.py", "--selfcheck"], False),
    ("startup_param", ["tools/window/startup_param.py", "selftest"], False),
    # reviewer-21 实跑：`in_world_stability.py selftest` ⇒ rc=0、7/7（纯合成数据，0.1s）。
    ("in_world_stability", ["tools/botload/in_world_stability.py", "selftest"], False),
    # M3 落地时由「未分类>0 ⇒ rc≠0」当场抓到的第一例（master 新工具，2026-10-25 合入）。
    ("gm_gate_check", ["tools/window/gm_gate_check.py", "selftest"], False),
    # 2026-10-25 mirflow-teeth 合入时由 --inventory 抓到（未分类 1）；离线假 mysql + 合成 frames，0.0s。
    ("provision_real_machine_fixtures",
     ["tools/client/provision_real_machine_fixtures.py", "--selftest"], False),
    ("tool_selftest_gate", ["tools/hygiene/tool_selftest_gate.py", "--selftest"], False),
    # 卡 `openmir2-docs-delivery-gate`：交付文档的结构性门（hex / 表格 / 路径 / 命令期望值）。
    # 自测 38 项、离线、临时 git 仓库；<2s。
    ("docs_delivery_gate", ["tools/hygiene/docs_delivery_gate.py", "--selftest"], False),
]

#: 清点用：仓库内其余带 selftest 的工具 → 为何**不**纳入本门禁（人工复核过的分类）。
OUT_OF_SCOPE = {
    # 2026-10-25 Leader 裁定后重审：只保留**真的无法离线自动判红**的项（当前 6 项；以 `--inventory` 实测为准），理由必须是
    # 「需要交互/外部产物/真实环境」，不能是「一次性/人工使用/别域」（那等于垃圾桶）。
    "tools/client/dlg_links.py":
        "selftest 需外部截图 E:/tmp/window/r7/shot-dice5.png；缺失时打印 SKIP 且 rc=0 ⇒ 接入会永远假绿（要接须先有人给该产物或改工具）",
    "tools/window/alt_port_profile.py":
        "`selftest` 需要**主检出构建产物/部署配置** `src/DBSrv/bin/Release/dbsvr.conf`（实测 `RED: db-srv: 找不到源配置，候选 …` ⇒ rc=1）；"
        "干净 CI 上没有该产物 ⇒ 红属环境、不能当工具回归（不是「只打印」）",
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
    # 非 `.py` 的入口（如 `-Selftest` 的 .ps1）**只收 REGISTRY 显式登记的**：
    # `tools/**/*.ps1` 里有 20+ 个带 selftest 字样的部署/验收脚本，全局扫描会把它们
    # 全变成「未分类」⇒ 判红。清点的语义是「谁在门内/谁被显式出域」，不是「目录爆破」。
    for _n, argv, _slow in REGISTRY:
        if argv and str(argv[0]).lower().endswith(".ps1"):
            rel = str(argv[0]).replace(os.sep, "/")
            if os.path.isfile(os.path.join(root, rel)) and rel not in out:
                out.append(rel)
    return sorted(out)


#: 自测用「会挂住的项」：自己睡 600s，并起一个同样睡 600s 的**孙进程**（继承 stdout）。
#: 专门复现「只杀直接子进程 ⇒ 第二次 communicate() 等管道 EOF ⇒ 门挂住」。
_HANG_CODE = ("import os, subprocess, sys, time\n"
              "p = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(30)'])\n"
              "_pf = os.environ.get('TSG_HANG_PIDFILE')\n"
              "if _pf:\n"
              "    open(_pf, 'w').write(str(p.pid))\n"
              "time.sleep(30)\n")


#: 仅供自测：关掉进程树终止，模拟「只 kill 直接子进程」⇒ 验证 10s 兜底本身能返回。
_TREE_KILL_DISABLED = False


def _apply_test_flag(args, err=None):
    """隐藏自测开关的生效规则（防「牙自己变成漏洞」）。

    * 只有和 `--selftest-hang-probe` 同用才真正关掉进程树终止；
    * `--selftest` 下**不生效**并 WARN（自测本身需要真树终止）；
    * 其它路径一律**忽略并 WARN**，绝不许一次真实门跑被静默关掉防护。
    返回是否真的开启了「关掉树终止」。
    """
    global _TREE_KILL_DISABLED
    if err is None:
        err = sys.stderr          # 不能在 def 默认值绑定：重定向 stderr 时默认值仍是旧对象
    if not getattr(args, "selftest_no_tree_kill", False):
        return False
    if getattr(args, "selftest_hang_probe", False):
        _TREE_KILL_DISABLED = True
        return True
    if getattr(args, "selftest", False):
        print("WARN: --selftest-no-tree-kill 不作用于 --selftest（自测需要真树终止），已忽略",
              file=err)
    else:
        print("WARN: --selftest-no-tree-kill 只在 --selftest-hang-probe 下生效，本次已忽略",
              file=err)
    return False


def _kill_tree(proc):
    """终止 proc **及其所有后代**。只杀直接子进程会留下孙进程占着 stdout 管道 ⇒ 门挂住。"""
    if _TREE_KILL_DISABLED:
        proc.kill()   # 自测路径：故意只杀直接子进程
        return
    if os.name == "nt":
        try:
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(proc.pid)],
                           capture_output=True, text=True, timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()  # taskkill 自己挂住也不许拖住门
            return
    else:
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            proc.kill()


#: 工具自测用 rc=2 表示「判不了」（环境/仓库问题）；与 rc=1（证伪）必须分开。
NEED_RC = 2
PASS, NEED, FAIL = "PASS", "NEED", "FAIL"


def classify_rc(rc):
    """工具自测的 rc → 三态：0 绿 / 2 需人工（判不了） / 其它 红（含 124 超时）。"""
    if rc == 0:
        return PASS
    if rc == NEED_RC:
        return NEED
    return FAIL


def build_cmd(argv):
    """注册项 → 真正执行的命令行。

    `.py` ⇒ `python -I -B <script> ...`；`.ps1` ⇒ `pwsh|powershell -NoProfile -ExecutionPolicy Bypass -File <script> ...`。
    其它（`-c` 片段等自测夹具）一律按 Python 处理。非 `.py` 的入口只允许来自 REGISTRY（不扫目录）。
    """
    argv = [str(a) for a in argv]
    if argv and argv[0].lower().endswith(".ps1"):
        shell = shutil.which("pwsh") or shutil.which("powershell")
        if not shell:
            raise RuntimeError("找不到 PowerShell（pwsh/powershell 都不在 PATH）：%s" % argv[0])
        return [shell, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"] + argv
    return [sys.executable, "-I", "-B"] + argv


def run_one(repo, name, argv, timeout):
    cmd = build_cmd(argv)
    t0 = time.time()
    kw = {"start_new_session": True} if os.name != "nt" else {}
    proc = subprocess.Popen(cmd, cwd=repo, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, encoding="utf-8", errors="replace", **kw)
    timed_out = False
    try:
        out, err = proc.communicate(timeout=timeout)
        rc = proc.returncode
    except subprocess.TimeoutExpired:
        # subprocess.run(timeout=…) 只 kill 直接子进程：孙进程继承 stdout 后，超时路径的
        # 第二次 communicate() 一直等 EOF ⇒ **门可无限挂住、300s 上限形同虚设**（本卡实证）。
        timed_out = True
        _kill_tree(proc)
        rc = 124
        try:
            out, err = proc.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            out, err = "", ""   # 树仍没收干净：照样判红，绝不许挂住
    if timed_out:
        how = ("树终止已被自测关闭，走兜底" if _TREE_KILL_DISABLED
               else "已按进程树终止")
        text = ["TIMEOUT %ss（%s）" % (timeout, how)]
    else:
        text = ((out or "") + (err or "")).strip().splitlines()[-1:] or [""]
    return {"name": name, "argv": list(argv), "rc": rc,
            "elapsed_s": round(time.time() - t0, 1), "tail": text[0][:160],
            "timed_out": timed_out}


def run(repo, only=None, skip=None, timeout=300, total_timeout=420,
        check_inventory=True):
    only = set(only or [])
    skip = set(skip or [])
    # 未知条目的 rc=2 由 main() 前置校验（F4：原先这里还有一条不可达的 raise）
    results = []
    deadline = (time.time() + total_timeout) if total_timeout and total_timeout > 0 else None
    for name, argv, _slow in REGISTRY:
        if only and name not in only:
            continue
        if name in skip:
            print("SKIP %-24s（--skip 显式跳过）" % name)
            results.append({"name": name, "skipped": True})
            continue
        eff = timeout
        if deadline is not None:
            remaining = deadline - time.time()
            if remaining <= 0:
                # 总墙钟兜底：没跑到的项也判红并点名——「跑没跑完」必须可区分，
                # 不许静默退出、更不许挂住（卡 openmir2-tool-selftest-gate 的 F5）。
                r = {"name": name, "rc": 124, "elapsed_s": 0.0, "timed_out": True,
                     "tail": "NOT-RUN：总墙钟 %ss 已尽（滞留项）" % total_timeout}
                results.append(r)
                print("FAIL %-24s rc=124        %s" % (name, r["tail"]))
                continue
            eff = min(timeout, max(1, int(remaining)))
        r = run_one(repo, name, argv, eff)
        r["state"] = classify_rc(r["rc"])
        results.append(r)
        print("%-4s %-24s rc=%-3s %5.1fs  %s"
              % (r["state"], name, r["rc"], r["elapsed_s"], r["tail"]))
    bad = [r for r in results if not r.get("skipped") and r.get("state") == FAIL]
    need = [r for r in results if not r.get("skipped") and r.get("state") == NEED]
    # A3：全跑末尾**显式**检查 inventory（不看牙的副作用）；`--only` 等调试跑同样适用。
    inv_rc, unclassified = inventory_gate_rc() if check_inventory else (0, [])
    print("TOTAL %d 项：绿 %d / 红 %d / 需人工 %d / 跳过 %d%s"
          % (len(results),
             sum(1 for r in results if not r.get("skipped") and r.get("state") == PASS),
             len(bad), len(need), sum(1 for r in results if r.get("skipped")),
             ("；**未分类 %d**" % len(unclassified)) if unclassified else ""))
    if need:
        print("NEED %d 项（工具自述判不了 rc=2，**不是**红；需人工确认环境/仓库）：%s"
              % (len(need), ", ".join(r["name"] for r in need)))
    # 红/未分类 ⇒ 2；只有「判不了」⇒ 3（既不是绿，也不是红）；否则 0。
    if bad or inv_rc == 2:
        return results, 2
    if need:
        return results, 3
    return results, 0


def inventory_rows():
    """分类全部带 selftest 的工具（不打印）：IN_GATE / OUT_OF_SCOPE / UNCLASSIFIED。"""
    in_gate = {a[0] for _n, a, _s in REGISTRY}
    rows = []
    for rel in discover():
        if rel in in_gate:
            status = "IN_GATE"
        elif rel in OUT_OF_SCOPE:
            status = "OUT_OF_SCOPE: " + OUT_OF_SCOPE[rel]
        else:
            status = "UNCLASSIFIED"
        rows.append({"tool": rel, "status": status})
    return rows


def unclassified_tools():
    return [r["tool"] for r in inventory_rows() if r["status"] == "UNCLASSIFIED"]


def inventory_gate_rc():
    """A3：全跑末尾**显式**检查 inventory。未分类>0 ⇒ 2 并点名。

    不再依赖 selftest 里的「仓库无未分类」断言（那会把仓库变化伪装成门坏了）。
    """
    un = unclassified_tools()
    for t in un:
        print("UNCLASSIFIED %s（未登记：加进 REGISTRY 或 OUT_OF_SCOPE）" % t)
    if un:
        print("未分类 %d 项 ⇒ 全跑判红（A3：inventory 是判据，不靠牙的副作用）" % len(un))
    return (2 if un else 0), un


def inventory(repo):
    rows = inventory_rows()
    for r in rows:
        print("%-12s %s" % (r["status"].split(":")[0], r["tool"])
              + (("  （%s）" % r["status"].split(": ", 1)[1]) if ": " in r["status"] else ""))
    n_unclassified = sum(1 for r in rows if r["status"] == "UNCLASSIFIED")
    print("清点：%d 个带 selftest 的工具；门禁内 %d；已分类出域 %d；未分类 %d"
          % (len(rows), sum(1 for r in rows if r["status"] == "IN_GATE"),
             sum(1 for r in rows if r["status"].startswith("OUT_OF_SCOPE")),
             n_unclassified))
    # M3（2026-10-25 裁定）：未分类 > 0 ⇒ rc≠0。清点是判据，不是打印。
    return rows, (2 if n_unclassified else 0)


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
            _res, rc = run(tmp, check_inventory=False)
        chk(rc == 2, "TSG 牙3 一绿一红 ⇒ 门 rc=2")
        buf = io.StringIO()
        with redirect_stdout(buf):
            _res, rc = run(tmp, skip=["bad"], check_inventory=False)
        out = buf.getvalue()
        chk(rc == 0 and "SKIP bad" in out,
            "TSG 牙4 --skip 打印 SKIP 且不放水（rc=0 仅因显式跳过）")
        buf = io.StringIO()
        with redirect_stdout(buf):
            _res, rc = run(tmp, only=["ok"], check_inventory=False)
        chk(rc == 0 and "ok" in buf.getvalue() and "bad" not in buf.getvalue(),
            "TSG 牙5 --only 只跑指定项")
        # 牙6：未知条目 ⇒ rc=2（用法错不许被当成「有红」，也不许假绿）
        import contextlib
        with contextlib.redirect_stderr(io.StringIO()):
            r6 = main(["--repo", tmp, "--only", "no-such-entry"])
            r6b = main(["--repo", tmp, "--skip", "no-such-entry"])
        chk(r6 == 2, "TSG 牙6 未知条目 ⇒ rc=2（用法错）")
        chk(r6b == 2, "TSG 牙6b --skip 未知条目 ⇒ rc=2")

        # ---- PowerShell 入口（卡 openmir2-docs-gate-rc-semantics 扩写集）----
        have_ps = bool(shutil.which("pwsh") or shutil.which("powershell"))
        chk(have_ps, "TSG 牙14 前置：本机有 PowerShell（pwsh 或 powershell）")
        if have_ps:
            ok_ps = os.path.join(tmp, "ok.ps1")
            need_ps = os.path.join(tmp, "need.ps1")
            with open(ok_ps, "w", encoding="utf-8") as fh:
                fh.write("Write-Output 'PS GREEN'\nexit 0\n")
            with open(need_ps, "w", encoding="utf-8") as fh:
                fh.write("Write-Output 'PS NEED 判不了'\nexit 2\n")
            chk(build_cmd([ok_ps])[0].lower().endswith(("pwsh", "pwsh.exe", "powershell",
                                                        "powershell.exe")),
                "TSG 牙14a .ps1 ⇒ build_cmd 走 PowerShell（不是 python）")
            r_ps_ok = run_one(tmp, "ps_ok", [ok_ps], 60)
            chk(r_ps_ok["rc"] == 0 and classify_rc(r_ps_ok["rc"]) == PASS,
                "TSG 牙14b 真跑临时 .ps1（exit 0）⇒ PASS（rc=0）")
            r_ps_need = run_one(tmp, "ps_need", [need_ps], 60)
            chk(r_ps_need["rc"] == 2 and classify_rc(r_ps_need["rc"]) == NEED,
                "TSG 牙14c 真跑临时 .ps1（exit 2）⇒ NEED（判不了，不是红）")

        # ---- 三态（卡 openmir2-docs-gate-rc-semantics）：判不了 ≠ 证伪 ----
        needf = os.path.join(tmp, "need.py")
        with open(needf, "w", encoding="utf-8") as fh:
            fh.write("print('NEED fake: git 不可用 ⇒ 判不了（rc=2）')\nraise SystemExit(2)\n")
        r_need = run_one(tmp, "need", [needf], 30)
        chk(r_need["rc"] == 2 and classify_rc(r_need["rc"]) == NEED,
            "TSG 牙13a 假·判不了（rc=2）⇒ classify=NEED（不是 FAIL）")
        reg13 = [("ok", [okf], False), ("need", [needf], False)]
        saved13 = REGISTRY
        REGISTRY = reg13
        try:
            buf = io.StringIO()
            with redirect_stdout(buf):
                _res13, rc13 = run(tmp, check_inventory=False)
            out13 = buf.getvalue()
        finally:
            REGISTRY = saved13
        chk(rc13 == 3 and "NEED" in out13 and "需人工 1" in out13 and "红 0" in out13,
            "TSG 牙13b 绿+判不了 ⇒ 门 rc=3、NEED 单列、红仍 0（%r）" % rc13)
        REGISTRY = [("ok", [okf], False), ("need", [needf], False), ("bad", [badf], False)]
        try:
            buf13c_ = io.StringIO()
            with redirect_stdout(buf13c_):
                _res13c, rc13c = run(tmp, check_inventory=False)
            out13c = buf13c_.getvalue()
        finally:
            REGISTRY = saved13
        chk(rc13c == 2 and "需人工 1" in out13c and "红 1" in out13c,
            "TSG 牙13c 绿+判不了+红 ⇒ 门 rc=2（红压过需人工）且两者各自计数")
        # 牙7/牙8：**门自己必须先能超时退出**。只杀直接子进程时，孙进程占着 stdout ⇒
        # 第二次 communicate() 等 EOF ⇒ 门无限挂住（本卡实证 pid 1772）/ 300s 上限形同虚设。

        def _bounded(cmd, secs, env=None):
            """跑子进程并按**进程树**终止；返回 (rc|None, out, elapsed)。"""
            t0 = time.time()
            pr = subprocess.Popen(cmd, stdin=subprocess.DEVNULL, env=env,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  text=True, encoding="utf-8", errors="replace")
            try:
                o, e = pr.communicate(timeout=secs)
                return pr.returncode, (o or "") + (e or ""), time.time() - t0
            except subprocess.TimeoutExpired:
                _kill_tree(pr)
                try:
                    o, e = pr.communicate(timeout=10)
                except subprocess.TimeoutExpired:
                    o, e = "", ""
                return None, (o or "") + (e or ""), time.time() - t0

        gate_py = os.path.join(root, "tools", "hygiene", "tool_selftest_gate.py")
        rc7, out7, el7 = _bounded([sys.executable, "-I", "-B", gate_py, "--repo", tmp,
                                   "--selftest-hang-probe", "--timeout", "2"], 25)
        # 墙钟必须≈单项超时（2s）；若进程树没杀掉，会退到 10s 兜底 → ~12s ⇒ 牙红。
        chk(rc7 == 2 and "hang" in out7 and "TIMEOUT" in out7 and el7 < 8,
            "TSG 牙7 挂住的项 ⇒ 进程树终止 + rc=2 + 点名（%.1fs < 8s）" % el7)
        rc8, out8, el8 = _bounded([sys.executable, "-I", "-B", gate_py, "--repo", tmp,
                                   "--selftest-hang-probe", "--timeout", "300",
                                   "--total-timeout", "3"], 25)
        # 未跑项必须**可 grep**：点名（after）+ 标记 NOT-RUN + 原因「总墙钟」，不能只有 rc=124。
        chk(rc8 == 2 and "总墙钟" in out8 and "NOT-RUN" in out8 and "after" in out8
            and el8 < 20,
            "TSG 牙8 总墙钟 3s ⇒ rc=2，滞留项点名可 grep（NOT-RUN + after，%.1fs）" % el8)
        # 牙9：**关掉进程树终止**（只 kill 直接子进程）⇒ 孙进程仍持 stdout，门必须靠
        # 10s 兜底返回 rc=2，**不许挂住**。这条把「兜底」本身钉住，与牙7（树终止生效）可区分。
        pidfile = os.path.join(tmp, "hang-grand.pid")
        env9 = dict(os.environ)
        env9["TSG_HANG_PIDFILE"] = pidfile
        grand = ""
        try:
            rc9, out9, el9 = _bounded([sys.executable, "-I", "-B", gate_py, "--repo", tmp,
                                       "--selftest-hang-probe", "--timeout", "2",
                                       "--selftest-no-tree-kill"], 35, env=env9)
        finally:
            # 失败路径也必须收尾：断言先红时不许把孙进程留在机器上
            try:
                with open(pidfile, encoding="utf-8") as fh:
                    grand = fh.read().strip()
            except OSError:
                grand = ""
            if grand.isdigit():
                if os.name == "nt":
                    _bounded(["taskkill", "/F", "/PID", grand], 10)
                else:
                    try:
                        os.kill(int(grand), signal.SIGKILL)
                    except (ProcessLookupError, PermissionError):
                        pass
        # 收尾后再断言：pidfile 里的 PID 必须已经不存在
        alive9 = False
        if grand.isdigit():
            if os.name == "nt":
                tl = _bounded(["tasklist", "/FI", "PID eq %s" % grand], 10)[1]
                alive9 = grand in (tl or "")
            else:
                try:
                    os.kill(int(grand), 0)
                    alive9 = True
                except (ProcessLookupError, PermissionError):
                    alive9 = False
        chk(rc9 == 2 and "TIMEOUT" in out9 and 10 <= el9 < 30 and grand.isdigit()
            and not alive9,
            "TSG 牙9 关掉树终止 ⇒ 10s 兜底生效：rc=2、%.1fs 返回、孙进程已清理（%s）"
            % (el9, grand or "无孙PID"))
        # 牙10：隐藏开关**不许**在真实门跑/`--selftest` 里生效（防「牙自己变成漏洞」）
        import argparse as _ap
        import contextlib
        import io as _io
        saved_flag = _TREE_KILL_DISABLED

        def _flag_case(**kw):
            a = _ap.Namespace(selftest=False, selftest_hang_probe=False,
                              selftest_no_tree_kill=True)
            for k, v in kw.items():
                setattr(a, k, v)
            buf = _io.StringIO()
            with contextlib.redirect_stderr(buf):
                got = _apply_test_flag(a, err=buf)
            globals()["_TREE_KILL_DISABLED"] = False   # 每个分支后复位
            return got, buf.getvalue()

        got_probe, _ = _flag_case(selftest_hang_probe=True)
        got_self, warn_self = _flag_case(selftest=True)
        got_none, warn_none = _flag_case()
        globals()["_TREE_KILL_DISABLED"] = saved_flag
        chk(got_probe is True and got_self is False and got_none is False
            and "不作用于 --selftest" in warn_self
            and "只在 --selftest-hang-probe" in warn_none,
            "TSG 牙10 隐藏开关：仅挂住夹具生效；--selftest/真实路径 ⇒ 忽略 + WARN")
        buf10 = _io.StringIO()
        with contextlib.redirect_stderr(buf10):
            rc10 = main(["--repo", tmp, "--selftest-no-tree-kill",
                         "--only", "no-such-entry"])
        chk(rc10 == 2 and "WARN" in buf10.getvalue()
            and _TREE_KILL_DISABLED == saved_flag,
            "TSG 牙10b 真实路径 --only 未知项：开关被忽略 + WARN，rc=2")
        # 牙11（M3）：--inventory 出现 UNCLASSIFIED ⇒ rc=2（不再恒 0），并点名
        _orig_discover = discover

        def _fake_discover():
            return sorted(set(_orig_discover()) | {"tools/__fake_unclassified__.py"})

        globals()["discover"] = _fake_discover
        _reg_before11 = REGISTRY
        globals()["REGISTRY"] = saved      # 上面为牙1-6 换成了假注册表，这里必须换回真表
        try:
            buf11 = _io.StringIO()
            with contextlib.redirect_stdout(buf11):
                _rows11, rc11 = inventory(root)
        finally:
            globals()["discover"] = _orig_discover
        chk(rc11 == 2 and "UNCLASSIFIED" in buf11.getvalue(),
            "TSG 牙11 --inventory 有 UNCLASSIFIED ⇒ rc=2 并点名（M3）")
        # 牙11b（A2）：**合成 discover**——只返回 REGISTRY 里的文件，不依赖真实仓库状态。
        _gate_files = sorted({a[0] for _n, a, _s in saved})

        def _fake_clean_discover():
            return list(_gate_files)

        globals()["discover"] = _fake_clean_discover
        try:
            buf11b = _io.StringIO()
            with contextlib.redirect_stdout(buf11b):
                _rows11b, rc11b = inventory(root)
        finally:
            globals()["discover"] = _orig_discover
            globals()["REGISTRY"] = _reg_before11   # 交回牙1-6 的假注册表，保持外层 finally 语义
        chk(rc11b == 0 and "UNCLASSIFIED" not in buf11b.getvalue(),
            "TSG 牙11b（合成 discover）无未分类 ⇒ rc=0（不看真实仓库）")

        # 牙12（A3/F1）：全跑末尾的显式 inventory 检查——**纯合成 discover**（不并真实仓库），
        # 否则「仓库多出一个未分类工具」会把本牙弄红，重演 A2 要消灭的失效模式。
        def _fake_unc_discover():
            return list(_gate_files) + ["tools/__fake_unclassified__.py"]

        globals()["REGISTRY"] = saved
        globals()["discover"] = _fake_unc_discover
        try:
            buf12 = _io.StringIO()
            with contextlib.redirect_stdout(buf12):
                rc12, un12 = inventory_gate_rc()
        finally:
            globals()["discover"] = _orig_discover
            globals()["REGISTRY"] = _reg_before11
        chk(rc12 == 2 and un12 == ["tools/__fake_unclassified__.py"]
            and "UNCLASSIFIED tools/__fake_unclassified__.py" in buf12.getvalue(),
            "TSG 牙12 全跑 inventory 检查：未分类>0 ⇒ rc=2 并单独点名（A3）")
        # 牙12b（F2 接线）：假 discover + **真 run()**；把 run() 末尾的 inventory 接线删掉 ⇒ 本牙红。
        _reg_before12b = REGISTRY

        def _fake_unc_discover2():
            return [okf, "tools/__fake_unclassified__.py"]

        globals()["REGISTRY"] = [("ok", [okf], False)]
        globals()["discover"] = _fake_unc_discover2
        try:
            buf12b = _io.StringIO()
            with contextlib.redirect_stdout(buf12b):
                _res12b, rc12b = run(tmp, timeout=30)
        finally:
            globals()["discover"] = _orig_discover
            globals()["REGISTRY"] = _reg_before12b
        chk(rc12b == 2
            and "UNCLASSIFIED tools/__fake_unclassified__.py" in buf12b.getvalue(),
            "TSG 牙12b（F2）真 run() + 假未分类 ⇒ rc=2 且打印 UNCLASSIFIED（接线牙）")
    finally:
        REGISTRY = saved
    print("%s: tool_selftest_gate selftest %d/%d"
          % ("GREEN" if not fail else "RED", ok, ok + fail))
    return 0 if not fail else 1


def main(argv=None):
    global REGISTRY
    ap = argparse.ArgumentParser(description="工具自测门禁（跑注册表里的 selftest）")
    ap.add_argument("--repo", default=None)
    ap.add_argument("--only", default="")
    ap.add_argument("--skip", default="")
    ap.add_argument("--timeout", type=int, default=300,
                    help="单项墙钟（秒），超时按进程树终止并判红，默认 300")
    ap.add_argument("--total-timeout", type=int, default=420,
                    help="总墙钟（秒），超时后未跑的项判红并点名，默认 420（< CI 任务 480s）；0=不限")
    ap.add_argument("--json", default=None)
    ap.add_argument("--inventory", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--selftest-hang-probe", action="store_true", help=argparse.SUPPRESS)
    ap.add_argument("--selftest-no-tree-kill", action="store_true", help=argparse.SUPPRESS)
    args = ap.parse_args(argv)
    # 守卫必须在 `--selftest` 分支之前：否则 `--selftest --selftest-no-tree-kill` 会被静默忽略。
    _apply_test_flag(args)
    if args.selftest:
        return selftest()
    if args.selftest_hang_probe:
        # 仅供自测：把注册表换成「一个会挂住的项 + 一个正常项」，复现挂住与总墙钟两条路径。
        print("WARN: --selftest-hang-probe 是自测夹具，不是一次真实的门跑", file=sys.stderr)
        REGISTRY = [("hang", ["-c", _HANG_CODE], False),
                    ("after", ["-c", "print('ok')"], False)]
    repo = repo_root(args.repo)
    if args.inventory:
        _rows, inv_rc = inventory(repo)
        return inv_rc
    only = [x for x in args.only.split(",") if x]
    skip = [x for x in args.skip.split(",") if x]
    known = {n for n, _a, _s in REGISTRY}
    unknown = sorted((set(only) | set(skip)) - known)
    if unknown:
        # 用法错 ⇒ rc=2（F3：此前 raise SystemExit(str) 会得到 rc=1，与文档不一致）
        print("tool_selftest_gate: 未知条目 %s（可用：--inventory 或 REGISTRY）"
              % ", ".join(unknown), file=sys.stderr)
        return 2
    results, rc = run(repo, only=only, skip=skip, timeout=args.timeout,
                      total_timeout=args.total_timeout)
    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump({"format": "tool-selftest-gate/1", "repo": repo,
                       "results": results, "rc": rc}, fh, ensure_ascii=False, indent=2)
    return rc


if __name__ == "__main__":
    sys.exit(main())
