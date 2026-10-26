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
import re
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
    # 卡 openmir2-refs-head-guard（Lead 裁决「只加这一行」）：collab 条目 refs.head/base 的
    # fail-closed 取值 + 反模式 lint；不登记就是 UNCLASSIFIED rc=2（既有团队规矩：新工具必须进工具门）。
    ("entryrefs", ["tools/collab/entryrefs.py", "--selftest"], False),
    ("run_batch", ["tools/window/run_batch_selftest.py"], False),
    ("goal_gate", ["tools/goal/goal_gate.py", "selftest"], False),
    ("conflict_marker_gate", ["tools/hygiene/conflict_marker_gate.py", "--selftest"], False),
    ("dynamic_count_stamp_gate", ["tools/hygiene/dynamic_count_stamp_gate.py", "--selftest"], False),
    ("merge_tree_helper", ["tools/hygiene/merge_tree_helper.py", "--selftest"], False),
    ("stale_pointer_gate", ["tools/hygiene/stale_pointer_gate.py", "--selftest"], False),
    ("board_coverage_gate", ["tools/hygiene/board_coverage_gate.py", "--selftest"], False),
    # 卡 `openmir2-scenario-manifest-gate`：卡面/文档引用的 probe 场景必须真的在 run3 清单里。
    ("scenario_manifest_gate", ["tools/hygiene/scenario_manifest_gate.py", "--selftest"], False),
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
    # C7（reviewer-21）：`tools/client/mir_flow.selftest.ps1` 有 **153 条断言**却既不在 REGISTRY
    # 也不在 OUT_OF_SCOPE —— 此前只被手工跑过（Leader 2026-10-27 合并树上手工 `SELFTEST_PASS=153`）。
    # 依赖 C3：无 PowerShell 的机器会落 NEED（rc=3）而不是把门崩成红。
    ("mir_flow_selftest", ["tools/client/mir_flow.selftest.ps1"], False),
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


_ORIG_DISCOVER = discover


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
#: 自测里的「探针」重入标记（F1rev：整跑一次 selftest 验 SKIP 不吞 FAIL）。
_PROBE = {"active": False}
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


#: 失败诊断行（卡 `openmir2-mir2-probe-selftest-need-flake` A0）——工具打印完失败清单后，
#: 往往还会再往 stderr 打一句 `note: …`；只留最后一行就把**原因**顶掉了（mir2_probe 实测）。
FAIL_LINE_RE = re.compile(
    r"^(FAIL\b|FAILED\b|ERROR\b|Traceback\b|panic(ked)? at |AssertionError\b|- \S)|.*\bFAIL(ED)?\b")

#: 失败时最多保留多少行诊断（够看到失败清单，又不把整屏塞进结果）。
FAIL_LINES_MAX = 12


def _failure_lines(lines, limit=FAIL_LINES_MAX):
    """从工具输出里**挑出失败诊断行**（而不是只留最后一行）。

    退化输入（规则 19）：没有任何匹配行 ⇒ **不返回空**，退回最后 5 行（宁可粗糙也不能空）；
    空输入 ⇒ `[]`。去重保序，单行截到 200 字符，最多 `limit` 行。
    """
    if not lines:
        return []
    picked = [l for l in lines if FAIL_LINE_RE.match(l.strip())] or lines[-5:]
    seen, out = set(), []
    for line in picked:
        if line in seen:
            continue
        seen.add(line)
        out.append(line[:200])
    return out[-limit:]


def failure_report_lines(state, fail_lines):
    """只有 FAIL / NEED 才回吐诊断行（绿色不许产生噪声）。"""
    if state not in (FAIL, NEED) or not fail_lines:
        return []
    return list(fail_lines[-6:])


def run_one(repo, name, argv, timeout):
    try:
        cmd = build_cmd(argv)
    except RuntimeError as ex:
        # C3（reviewer-21 7060d720）：拿不到 PowerShell 是**判不了**，不是「工具坏了」。
        # 记 NEED（rc=2）而不是抛出去把整个门崩成 rc=1。
        return {"name": name, "argv": [str(a) for a in argv], "rc": NEED_RC,
                "elapsed_s": 0.0, "tail": "判不了：%s" % ex, "timed_out": False}
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
        fail_lines = list(text)
    else:
        all_lines = ((out or "") + (err or "")).strip().splitlines()
        text = all_lines[-1:] or [""]
        # A0：**多条**诊断行单独存（`tail` 保持单行，打印布局不变）
        fail_lines = _failure_lines(all_lines)
    return {"name": name, "argv": list(argv), "rc": rc,
            "elapsed_s": round(time.time() - t0, 1), "tail": text[0][:160],
            "fail_lines": fail_lines, "timed_out": timed_out}


DOC_ANCHOR_REL = "docs/real-machine-window-plan-20261025.md"

# 卡 `openmir2-goal-gate-count-anchor`：同一块文档里的另外两行也要机判——
#   ① `**N 条断言 N 通过**`（goal_gate selftest 的实跑条数）；
#   ② `**N suites，running R = passed P + ignored I**（`<tip>` 实测）`（cargo 三元组）。
# ② 的数值来自**测试车道的实测证据文件**（不在这里跑 workspace：那是几分钟量级）。
# 证据缺失 / 格式不认识 / 该 tip 没有条目 / 该条目 rc≠0 ⇒ **rc=2 判不了**（不是绿，也不是红）。
DOC_EVIDENCE_REL = ("tester", "last-workspace-test.json")
DOC_EVIDENCE_FORMAT = "mir2-tester-workspace-evidence/1"


def _clean_git_env():
    """去掉会让 `git -C <repo>` 指错仓库的变量（与 docs_delivery_gate 同口径）。"""
    drop = {"GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"}
    return {k: v for k, v in os.environ.items() if k not in drop}


def _git_common_dir(repo):
    """返回 `<repo>` 的 git 公共目录（跨 worktree 共享）；拿不到 ⇒ None。"""
    try:
        p = subprocess.run(["git", "-C", str(repo), "rev-parse",
                            "--path-format=absolute", "--git-common-dir"],
                           capture_output=True, text=True, timeout=30,
                           env=_clean_git_env())
    except (OSError, subprocess.SubprocessError):
        return None
    out = (p.stdout or "").strip()
    return out if p.returncode == 0 and out else None


def _default_evidence_path(repo):
    gcd = _git_common_dir(repo)
    return os.path.join(gcd, *DOC_EVIDENCE_REL) if gcd else None


def _run_goal_assertion_count(repo, goal_cmd=None):
    """跑 `goal_gate selftest` 取**实跑**断言数；任何失败 ⇒ `(None, 原因)`，由调用方判 rc=2。"""
    if goal_cmd is None:
        tool = os.path.normpath(os.path.join(
            os.path.dirname(os.path.abspath(__file__)), "..", "goal", "goal_gate.py"))
        goal_cmd = [sys.executable, "-I", "-B", tool, "selftest"]
    try:
        p = subprocess.run(list(goal_cmd), cwd=str(repo), capture_output=True,
                           text=True, timeout=120)
    except (OSError, subprocess.SubprocessError) as ex:
        return None, "`goal_gate selftest` 跑不起来（%s）" % ex
    if p.returncode != 0:
        return None, "`goal_gate selftest` rc=%d（判不了，不是红）" % p.returncode
    m = re.search(r"(\d+)\s*条断言[，,]\s*(\d+)\s*通过", p.stdout or "")
    if not m:
        return None, "解析不到 `N 条断言 N 通过`（判不了）"
    total, passed = int(m.group(1)), int(m.group(2))
    if total != passed:
        return None, "`goal_gate selftest` 自报 %d 条断言但只过 %d 条（判不了）" % (total, passed)
    return total, ""


def _tip_distance(repo, tip):
    """info 用：证据 tip 到 HEAD 之间有多少个提交；查不到 ⇒ None。

    **只作显示、绝不判红**（review/Leader：窗口值守要看得见「这是几个提交前的数」，
    但每次合并都会拉开距离，若据此判红就会逼人放松判据）。
    """
    try:
        p = subprocess.run(["git", "-C", str(repo), "rev-list", "--count", tip + "..HEAD"],
                           capture_output=True, text=True, timeout=30, env=_clean_git_env())
    except (OSError, subprocess.SubprocessError):
        return None
    try:
        return int((p.stdout or "").strip())
    except ValueError:
        return None


def _load_evidence(path):
    """读测试车道的 workspace 证据文件；`(entries, 原因)`，第二个非空即判不了。"""
    if not path:
        return None, "定位不到证据文件（拿不到 git 公共目录？）"
    if not os.path.isfile(path):
        return None, "证据文件缺失：%s" % path
    try:
        with open(path, encoding="utf-8") as fh:
            d = json.load(fh)
    except (OSError, ValueError) as ex:
        return None, "证据文件读不了（%s）：%s" % (path, ex)
    if not isinstance(d, dict) or d.get("format") != DOC_EVIDENCE_FORMAT:
        return None, "证据文件格式不认识：%r（要 %s）" % (
            d.get("format") if isinstance(d, dict) else d, DOC_EVIDENCE_FORMAT)
    entries = d.get("entries")
    if not isinstance(entries, dict):
        return None, "证据文件缺 entries（%s）" % path
    return entries, ""


def check_doc_anchors(repo, evidence_path=None, goal_cmd=None):
    """R3（卡 openmir2-tool-gate-teeth-residuals）+ F1/F2（review 92f425e9 / Leader 活锚地板）。

    开窗当天会**照抄三处渲染**里的数字确认「开的就是预期那棵树」：
      ① 表行：`⇒ **N 项全绿**；`
      ② 正文：`⇒ **N 项全绿**；… 清点 **M = N + K + 0**`
      ③ 提示行：`（**N 项 / 清点 M = N + K + 0**）`
    只机判其中一种时，另两种各说各话而门仍绿（review F1：把表行改成 99 仍 rc=0）⇒ 现在**三处都收**：
    所有 `N 项(全绿|/)` 命中都必须等于 `len(REGISTRY)`；所有 `清点 M = N + K + Z` 命中都必须等于
    `(total, in_gate, out_scope, 0)`。

    **历史叙事跳过 + 活锚地板**：紧邻前文出现「历史 / 此前 / 原始记录 / 当时 / 曾」的命中是**记录、不是锚**，
    跳过；但**每一类在排除之后都必须仍有 ≥1 处活锚**——某类被排除到空 ⇒ **rc=2 判不了**
    （否则「历史」两个字就成了新的万能挡箭牌，和 `LEGACY_MARK_RE` 同族）。
    被排除的条数与行号**打印在输出里**，让「跳过清单变长」可见。

    **排除项必须严格小于现值**（两类计数只增）：`排除的 N < len(REGISTRY)`、`排除的 M < total`。
    否则「把写错的锚加个『历史』前缀」就能躲进排除集（review 实测：`历史：**53 项全绿**` 曾 rc=0）。
    若某天计数真的缩水，别用「历史」标记写法——把旧值写成不带标记的叙述，或改这份 docstring 的契约。

    **另外两类（卡 openmir2-goal-gate-count-anchor）**，同一块文档、同一套「历史排除 + 活锚地板」口径：
      ④ `**N 条断言 N 通过**` —— 与 `goal_gate selftest` 的**实跑**条数比对；跑不起来 / 解析不到 ⇒ **rc=2**。
      ⑤ `**N suites，running R = passed P + ignored I**（`<tip>` 实测）` —— 先自洽（R = P + I），
         再与测试车道的证据文件（`<git-common-dir>/tester/last-workspace-test.json`，
         format `mir2-tester-workspace-evidence/1`）里**该 tip** 的实测比。
         证据缺失 / 格式不认识 / 该 tip 无条目 / 该条目 `rc≠0` ⇒ **rc=2**（判不了 ≠ 绿）。
         ⑤ 的锚**必须标注测量 tip**，否则无从对账 ⇒ 也判 rc=2。
    """
    path = os.path.join(str(repo), DOC_ANCHOR_REL)
    if not os.path.isfile(path):
        return 2, "认树锚文档缺失：%s" % DOC_ANCHOR_REL
    with open(path, encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    # 早绑定：`_dist_note()` 在「伪造历史」那条提前返回里也会调用，别让 `wrong` 未绑定。
    missing, wrong = [], []
    n_items = len(REGISTRY)
    rows = inventory_rows()
    total = len(rows)
    in_gate = sum(1 for r in rows if r["status"] == "IN_GATE")
    out_scope = sum(1 for r in rows if r["status"].startswith("OUT_OF_SCOPE"))

    _HISTORY_MARKERS = ("历史", "此前", "原始记录", "当时", "曾")

    def _is_history(pos):
        # **行内**判定：标记必须与数字在同一行（早期版本看前 40 字符，会误伤下一行——review 的
        # `历史：**51 项全绿**` 下一行的三元组被连带当成历史）。
        line_start = text.rfind(chr(10), 0, pos) + 1
        return any(w in text[line_start:pos] for w in _HISTORY_MARKERS)

    def _lineno(pos):
        return text.count("\n", 0, pos) + 1

    n_all = [(int(m.group(1)), m.start())
             for m in re.finditer(r"\*\*(\d+)\s*项全绿\*\*", text)]
    n_all += [(int(m.group(1)), m.start())
              for m in re.finditer(r"\*\*(\d+)\s*项\s*/\s*清点", text)]
    tri_all = [(tuple(int(x) for x in m.groups()), m.start()) for m in re.finditer(
        r"清点\s*\*{0,2}\s*(\d+)\s*=\s*(\d+)\s*\+\s*(\d+)\s*\+\s*(\d+)", text)]

    n_live = [(v, p) for v, p in n_all if not _is_history(p)]
    tri_live = [(v, p) for v, p in tri_all if not _is_history(p)]
    n_excl = [p for _v, p in n_all if _is_history(p)]
    tri_excl = [p for _v, p in tri_all if _is_history(p)]

    # **伪造历史**（review：多锚情形下的洞）：把写错的锚前缀加个『历史』就能躲进排除集。
    # 排除项必须**严格小于**现值——本卡两类计数都是只增的，真历史值（47/48、52=47+5+0）都小于现值；
    # 出现「排除项 ≥ 现值」只可能是伪造（或计数真的缩过水，那就别用历史标记写法，见 docstring）。
    forged = []
    for v, p in n_all:
        if _is_history(p) and v >= n_items:
            forged.append("N 类排除项 %d ≥ 现值 %d（行 %d）⇒ 不能当历史" % (v, n_items, _lineno(p)))
    for t, p in tri_all:
        if _is_history(p) and t[0] >= total:
            forged.append("三元组类排除项 M=%d ≥ 现值 %d（行 %d）⇒ 不能当历史"
                          % (t[0], total, _lineno(p)))

    # ---- ④ 断言数类 / ⑤ cargo 三元组类（卡 openmir2-goal-gate-count-anchor）----
    # 这两类与上面两类**同一块文档**：历史排除 + 活锚地板 + 「排除项必须能证成历史」同口径。
    ga_all = [(int(m.group(1)), int(m.group(2)), m.start()) for m in re.finditer(
        r"\*\*(\d+)\s*条断言\s*(\d+)\s*通过\*\*", text)]
    car_all = []
    for m in re.finditer(
            r"(\d+)\s*suites?[，,]\s*running\s+(\d+)\s*=\s*passed\s+(\d+)\s*\+\s*ignored\s+(\d+)",
            text):
        ls = text.rfind(chr(10), 0, m.start()) + 1
        le = text.find(chr(10), m.end())
        le = len(text) if le < 0 else le
        tm = re.search(r"`([0-9a-f]{7,40})`", text[ls:le])
        car_all.append((tuple(int(x) for x in m.groups()), m.start(),
                        tm.group(1) if tm else None))

    ga_live = [x for x in ga_all if not _is_history(x[2])]
    car_live = [x for x in car_all if not _is_history(x[1])]
    ga_excl = [x[2] for x in ga_all if _is_history(x[2])]
    car_excl = [x[1] for x in car_all if _is_history(x[1])]

    # ④ 实跑值：**可失败**地取（A2）——跑不起来 / 解析不到 / 自报通过数不等 ⇒ (None, 原因) ⇒ rc=2。
    # 有锚就取（**即使活锚为空**）：伪造历史（排除值 ≥ 实跑值）要先有实跑值才判得了。
    ga_actual, ga_err = (None, "")
    if ga_all:
        ga_actual, ga_err = _run_goal_assertion_count(repo, goal_cmd)
    # ⑤ 证据：只在有带 tip 的活锚时才读（否则在「活锚为空 / 无 tip」处已判 rc=2）。
    entries, ev_err = (None, "")
    if car_live and any(tip for _t, _p, tip in car_live):
        entries, ev_err = _load_evidence(
            evidence_path if evidence_path is not None else _default_evidence_path(repo))

    # 伪造历史（④）：排除值 ≥ 实跑值 ⇒ 只可能是把写错的锚加个「历史」前缀躲进来。
    if ga_actual is not None:
        for a, _b, p in ga_all:
            if _is_history(p) and a >= ga_actual:
                forged.append("断言数类排除项 %d ≥ 实跑 %d（行 %d）⇒ 不能当历史"
                              % (a, ga_actual, _lineno(p)))
    # 伪造历史（⑤）：排除行与活锚**同一 tip** 但数值不同 ⇒ 自相矛盾，不能当历史。
    _live_by_tip = {}
    for _t, _p, _tip in car_live:
        if _tip:
            _live_by_tip.setdefault(_tip, _t)
    for t, p, tip in car_all:
        if _is_history(p) and tip in _live_by_tip and t != _live_by_tip[tip]:
            forged.append("cargo 类排除项与活锚同一 tip `%s` 但数值不同（行 %d）⇒ 不能当历史"
                          % (tip, _lineno(p)))

    def _dist_note():
        """info：每个活锚的 tip 距 HEAD 多少个提交（查不到就不显示，绝不判红）。"""
        seen, parts = set(), []
        for _t, _p, tip in car_live:
            if not tip or tip in seen:
                continue
            seen.add(tip)
            d = _tip_distance(repo, tip)
            if d is not None:
                parts.append("证据 tip `%s` 距 HEAD %d 个提交（info，不判红）" % (tip, d))
        return ("；" + "；".join(parts)) if parts else ""

    def _excl_note():
        parts = []
        for name, exc in (("N 类", n_excl), ("三元组类", tri_excl),
                          ("断言数类", ga_excl), ("cargo 类", car_excl)):
            if exc:
                parts.append("%s排除历史 %d 处（行 %s）"
                             % (name, len(exc), ",".join(str(_lineno(p)) for p in exc[:8])))
        return ("；" + "；".join(parts)) if parts else ""

    if forged:
        return 1, "；".join(forged) + _excl_note() + _dist_note()

    if not n_live:
        why = ("所有 N 类锚都被历史排除（共 %d 处）⇒ 该类失去判据" % len(n_all)) if n_all \
            else "找不到任何「**N 项全绿**」或「**N 项 / 清点」锚"
        missing.append("N 类活锚为空：" + why)
    if not tri_live:
        why = ("所有三元组锚都被历史排除（共 %d 处）⇒ 该类失去判据" % len(tri_all)) if tri_all \
            else "找不到任何「清点 M = N + K + 0」锚"
        missing.append("三元组类活锚为空：" + why)
    for v in sorted({v for v, _p in n_live}):
        if v != n_items:
            wrong.append("`N 项` 锚写作 %d，实况 len(REGISTRY)=%d" % (v, n_items))
    want = (total, in_gate, out_scope, 0)
    for t in sorted({v for v, _p in tri_live}):
        if t != want:
            wrong.append("清点锚写作 %d = %d + %d + %d，实况 %d = %d + %d + %d" % (t + want))

    # ④ 断言数类：地板 → 实跑可失败 → 文档值必等于实跑值。
    if not ga_live:
        why = ("所有「N 条断言」锚都被历史排除（共 %d 处）⇒ 该类失去判据" % len(ga_all)) if ga_all             else "找不到任何「**N 条断言 N 通过**」锚"
        missing.append("断言数类活锚为空：" + why)
    elif ga_actual is None:
        missing.append("断言数类判不了：" + ga_err)
    else:
        for a, b, p in ga_live:
            if a != b:
                wrong.append("「N 条断言 N 通过」自相矛盾（行 %d）：%d ≠ %d"
                             % (_lineno(p), a, b))
            elif a != ga_actual:
                wrong.append("「N 条断言」锚写作 %d（行 %d），`goal_gate selftest` 实跑 %d"
                             % (a, _lineno(p), ga_actual))

    # ⑤ cargo 三元组类：地板 → 每个活锚必须带 tip → 自洽 → 证据可查且数值一致。
    if not car_live:
        why = ("所有 cargo 三元组锚都被历史排除（共 %d 处）⇒ 该类失去判据" % len(car_all)) if car_all             else "找不到任何「N suites，running R = passed P + ignored I」锚"
        missing.append("cargo 类活锚为空：" + why)
    else:
        for t, p, tip in car_live:
            if t[1] != t[2] + t[3]:
                wrong.append("cargo 三元组不自洽（行 %d）：running %d ≠ passed %d + ignored %d"
                             % (_lineno(p), t[1], t[2], t[3]))
            if not tip:
                missing.append("cargo 锚没有标注测量 tip（行 %d）⇒ 无从与证据对账"
                               % _lineno(p))
        if entries is None:
            missing.append("cargo 类判不了：" + (ev_err or "没有带 tip 的 cargo 锚，无从读证据"))
        else:
            for t, p, tip in car_live:
                if not tip:
                    continue
                keys = [k for k in entries if str(k).startswith(tip)]
                if not keys:
                    missing.append("证据里没有 tip `%s` 的实测（行 %d）⇒ 证据缺失/过期"
                                   % (tip, _lineno(p)))
                    continue
                if len(keys) > 1:
                    missing.append("证据里 tip 前缀 `%s` 命中 %d 个条目（行 %d）⇒ 判不了"
                                   % (tip, len(keys), _lineno(p)))
                    continue
                e = entries[keys[0]]
                if isinstance(e, dict) and all(
                        isinstance(e.get(k), int) for k in ("suites", "running", "passed", "ignored"))                         and e["running"] != e["passed"] + e["ignored"]:
                    # **证据不自洽 = 证据坏了**（不是文档写错）⇒ rc=2 判不了，不许拿它去判别人。
                    missing.append("证据条目（`%s`）不自洽：running %d ≠ passed %d + ignored %d "
                                   "⇒ 坏证据，判不了" % (tip, e["running"], e["passed"], e["ignored"]))
                    continue
                if not isinstance(e, dict) or not all(
                        isinstance(e.get(k), int) for k in ("suites", "running", "passed", "ignored")):
                    missing.append("证据条目（`%s`）缺字段或字段不是整数 ⇒ 坏证据，判不了" % tip)
                    continue
                if e.get("rc") not in (0, "0"):
                    missing.append("tip `%s` 的实测 rc=%r（行 %d）⇒ 失败的实跑不能当证据"
                                   % (tip, (e or {}).get("rc") if isinstance(e, dict) else e,
                                      _lineno(p)))
                    continue
                for idx, name in enumerate(("suites", "running", "passed", "ignored")):
                    if e.get(name) != t[idx]:
                        wrong.append("cargo 锚 %s 写作 %d（行 %d），证据（`%s`）是 %r"
                                     % (name, t[idx], _lineno(p), tip, e.get(name)))
    # 距离 info 在**判定之前**求值：info 通道自己若写错（比如被改成往 wrong 里塞），
    # 必须能在下面的 rc 判定里体现出来，而不是在 return 表达式里算完就丢。
    _dist = _dist_note()
    if missing:
        return 2, "；".join(missing + wrong) + _excl_note() + _dist
    if wrong:
        return 1, "；".join(wrong) + _excl_note() + _dist
    return 0, ("认树锚一致：%d 项 / 清点 %d = %d + %d + 0（N 活 %d/共 %d、三元组活 %d/共 %d、"
               "断言数活 %d/共 %d=实跑 %s、cargo 活 %d/共 %d）%s"
               % (n_items, total, in_gate, out_scope,
                  len(n_live), len(n_all), len(tri_live), len(tri_all),
                  len(ga_live), len(ga_all), ga_actual, len(car_live), len(car_all),
                  _excl_note() + _dist))


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
        # A0：失败/判不了时把**原因行**也打出来（否则只看到最后一句 note）
        for _line in failure_report_lines(r["state"], r.get("fail_lines")):
            print("       | %s" % _line)
    bad = [r for r in results if not r.get("skipped") and r.get("state") == FAIL]
    need = [r for r in results if not r.get("skipped") and r.get("state") == NEED]
    # A3：全跑末尾**显式**检查 inventory（不看牙的副作用）；`--only` 等调试跑同样适用。
    inv_rc, unclassified = inventory_gate_rc() if check_inventory else (0, [])
    doc_rc, doc_detail = check_doc_anchors(repo) if check_inventory else (0, "")
    if check_inventory:
        print("%s DOC_ANCHORS %s" % (
            {0: "PASS", 1: "FAIL", 2: "NEED"}.get(doc_rc, "FAIL"), doc_detail))
    print("TOTAL %d 项：绿 %d / 红 %d / 需人工 %d / 跳过 %d%s"
          % (len(results),
             sum(1 for r in results if not r.get("skipped") and r.get("state") == PASS),
             len(bad), len(need), sum(1 for r in results if r.get("skipped")),
             ("；**未分类 %d**" % len(unclassified)) if unclassified else ""))
    if need:
        print("NEED %d 项（工具自述判不了 rc=2，**不是**红；需人工确认环境/仓库）：%s"
              % (len(need), ", ".join(r["name"] for r in need)))
    # 红/未分类 ⇒ 2；只有「判不了」⇒ 3（既不是绿，也不是红）；否则 0。
    if bad or inv_rc == 2 or doc_rc == 1:
        return results, 2
    if need or doc_rc == 2:
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
    n_in_gate = sum(1 for r in rows if r["status"] == "IN_GATE")
    print("清点：%d 个带 selftest 的工具；门禁内 %d；已分类出域 %d；未分类 %d"
          % (len(rows), n_in_gate,
             sum(1 for r in rows if r["status"].startswith("OUT_OF_SCOPE")),
             n_unclassified))
    # M3（2026-10-25 裁定）：未分类 > 0 ⇒ rc≠0。清点是判据，不是打印。
    if n_unclassified:
        return rows, 2
    # discover() 与 REGISTRY 必须自洽：否则「门内 N」是假的（例如有人给 REGISTRY 加了非 .py
    # 入口却没同步 discover()，门会在跑它、清点却看不见）。只在**真实** discover 下判——
    # 自测会替换 discover 做合成清点（牙11/11b），那不是事实源。
    if globals().get("discover") is _ORIG_DISCOVER and n_in_gate != len(REGISTRY):
        print("INVENTORY_MISMATCH 门禁内 %d ≠ REGISTRY %d（discover() 与 REGISTRY 不自洽）"
              % (n_in_gate, len(REGISTRY)))
        return rows, 2
    return rows, 0


def selftest():
    global REGISTRY
    ok = fail = skip_n = 0

    def chk(cond, name):
        nonlocal ok, fail
        if cond:
            ok += 1
            print("PASS " + name)
        else:
            fail += 1
            print("FAIL " + name)

    def skip(name):
        """环境不具备 ⇒ SKIP（**不是** PASS，也**不是** FAIL）。

        F1（review bdd1a28c）：C3 的判据是「**门仍能判**」而不是「门必须绿」——
        没有 PowerShell 的机器上，`牙14 前置` 不该把门自测判红。
        """
        nonlocal skip_n
        skip_n += 1
        print("SKIP " + name)

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
    # 牙A0（卡 `openmir2-mir2-probe-selftest-need-flake`）：失败时**不能只留最后一行**。
    # 复刻 mir2_probe 的真实形态：stdout 打 FAIL 清单，stderr 再打一句 note（正是它把原因顶掉）。
    probef = os.path.join(tmp, "probe_like.py")
    with open(probef, "w", encoding="utf-8") as fh:
        fh.write("import sys" + chr(10)
                 + "print('mir2_probe selftest: FAIL')" + chr(10)
                 + "print('  - dwell-after-enter: late frames missing')" + chr(10)
                 + "sys.stderr.write('note: scenario.placement ...')" + chr(10)
                 + "raise SystemExit(2)" + chr(10))
    r_a0 = run_one(tmp, "probe_like", [probef], 30)
    chk(r_a0["rc"] == 2 and "note: scenario.placement" in r_a0["tail"],
        "TSG 牙A0a 前置：最后一行确实是 note（原诊断被顶掉的形态；%r）" % (r_a0["tail"],))
    chk(any("FAIL" in l for l in r_a0["fail_lines"])
        and any("dwell-after-enter" in l for l in r_a0["fail_lines"]),
        "TSG 牙A0b 失败清单整段留在 fail_lines（%r）" % (r_a0["fail_lines"],))
    chk(failure_report_lines(PASS, r_a0["fail_lines"]) == [],
        "TSG 牙A0c 绿色不产生噪声（PASS ⇒ 不回吐诊断行）")
    chk(failure_report_lines(FAIL, r_a0["fail_lines"]) != []
        and failure_report_lines(NEED, r_a0["fail_lines"]) != [],
        "TSG 牙A0d 红与判不了都回吐（FAIL/NEED ⇒ 非空）")
    chk(_failure_lines([]) == [], "TSG 牙A0e 空输入 ⇒ []（规则 19 退化输入）")
    chk(_failure_lines(["只有一行普通输出"]) == ["只有一行普通输出"],
        "TSG 牙A0f 没有失败样式的行 ⇒ 退回末尾（不许返回空）")
    # 端到端接线牙：**只存不打印 = 人还是看不到** ⇒ 真 run() 必须把它打到 stdout。
    import io as _io_a0
    from contextlib import redirect_stdout as _rdout_a0
    saved_a0 = REGISTRY
    REGISTRY = [("probe_like", [probef], False)]
    try:
        buf_a0 = _io_a0.StringIO()
        with _rdout_a0(buf_a0):
            _res_a0, rc_a0 = run(tmp, check_inventory=False)
    finally:
        REGISTRY = saved_a0
    out_a0 = buf_a0.getvalue()
    chk(rc_a0 in (2, 3) and "dwell-after-enter" in out_a0,
        "TSG 牙A0g 真 run() 把失败原因打出来（否则只剩最后一句 note；输出尾=%r）"
        % (out_a0.splitlines()[-3:],))
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
        if not have_ps:
            skip("TSG 牙14a/14b/14c（本机无 PowerShell：跳过 PS 分发牙；"
                 "C3 判据是「门仍能判」，不是「门必须绿」）")
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

        # ---- C2/C3（reviewer-21 7060d720）：自洽判据与 PowerShell 缺失都必须有机器守 ----
        # 注意：此刻 REGISTRY 还是牙3 的假表（[ok,bad]）——必须换成**真实**表再追加，
        # 否则 discovered 里的真工具全变 UNCLASSIFIED，判据在 mismatch 之前就先 rc=2 了。
        _real_reg_c2 = list(saved)
        REGISTRY[:] = _real_reg_c2 + [("r21_fake", ["tools/__r21_missing__.py", "--selftest"], False)]
        try:
            _buf_c2 = io.StringIO()
            with redirect_stdout(_buf_c2):
                _rows_c2, rc_c2 = inventory(root)
            out_c2 = _buf_c2.getvalue()
        finally:
            REGISTRY[:] = _real_reg_c2
        chk(rc_c2 == 2 and "INVENTORY_MISMATCH" in out_c2,
            "TSG 牙C2a REGISTRY 多一条但 discover 收不到 ⇒ --inventory rc=2 + INVENTORY_MISMATCH"
            "（rc=%r，输出尾=%r）" % (rc_c2, out_c2.strip().splitlines()[-1:]))
        # 反例：discover 被替换（合成清点）时**不**套这条判据——判据只在真实 discover 下生效
        _orig_disc_c2 = globals()["discover"]
        REGISTRY[:] = _real_reg_c2
        globals()["discover"] = lambda: []
        try:
            _buf_c2b = io.StringIO()
            with redirect_stdout(_buf_c2b):
                _rows_c2b, rc_c2b = inventory(root)
        finally:
            globals()["discover"] = _orig_disc_c2
        chk(rc_c2b == 0,
            "TSG 牙C2b discover 被替换（合成清点）⇒ 不套自洽判据（rc=0，作用域牙）")
        chk(any(str(a[0]).endswith("mir_flow.selftest.ps1") for _n, a, _s in saved),
            "TSG 牙C7 tools/client/mir_flow.selftest.ps1 已登记进 REGISTRY（153 条断言不再无人跑）")
        # C3：拿不到 PowerShell ⇒ run_one 记 NEED（不抛）
        _real_which = shutil.which
        globals()["shutil"].which = lambda name, *a, **k: None
        try:
            r_c3 = run_one(tmp, "ps_noshell", ["tools/window/no-such.ps1"], 5)
        finally:
            globals()["shutil"].which = _real_which
        # R3（卡 openmir2-tool-gate-teeth-residuals）+ F1（review 92f425e9）：
        # 认树锚机判必须覆盖**三种渲染**且彼此一致：表行 `**N 项全绿**`、正文三元组、提示行 `**N 项 / 清点 …**`。
        import tempfile as _tf_r3
        _r3_root = _tf_r3.mkdtemp(prefix="tsg-docanchor-")
        _r3_doc = os.path.join(_r3_root, "docs")
        os.makedirs(_r3_doc, exist_ok=True)
        _r3_path = os.path.join(_r3_doc, "real-machine-window-plan-20261025.md")
        _rows = inventory_rows()
        _m = len(_rows)
        _g = sum(1 for r in _rows if r["status"] == "IN_GATE")
        _o = sum(1 for r in _rows if r["status"].startswith("OUT_OF_SCOPE"))
        _n = len(REGISTRY)

        # ④⑤（卡 openmir2-goal-gate-count-anchor）夹具：合成证据文件 + 真实的 goal_gate 实跑值。
        # 夹具里的 tip 是**合成**的（不是真 commit）——⑤ 只认证据里的条目，不查 git 是否存在该 commit；
        # 「证据缺失/过期 ⇒ rc=2」由 R5c/R5d 钉住。
        _r3_ev = os.path.join(_r3_root, "evidence.json")
        _r3_tip = "abc1234de"
        _r3_car = (141, 3366, 3303, 63)
        _ga_real, _ga_real_err = _run_goal_assertion_count(_r3_root)
        chk(_ga_real is not None and _ga_real > 0,
            "R4pre goal_gate selftest 实跑值可取得（实跑 %r；%s）" % (_ga_real, _ga_real_err))

        def _r3_ev_write(entries=None, drop=False):
            if drop:
                try:
                    os.remove(_r3_ev)
                except OSError:
                    pass
                return
            d = {"format": DOC_EVIDENCE_FORMAT,
                 "entries": {_r3_tip: {"suites": _r3_car[0], "running": _r3_car[1],
                                       "passed": _r3_car[2], "ignored": _r3_car[3], "rc": 0}}
                 if entries is None else entries}
            with open(_r3_ev, "w", encoding="utf-8") as fh:
                fh.write(json.dumps(d))

        _r3_ev_write()

        def _r3_check(**kw):
            kw.setdefault("evidence_path", _r3_ev)
            return check_doc_anchors(_r3_root, **kw)

        def _r3_write(table_n=None, tri=None, note_n=None, drop_tri=False,
                      ga=None, car=None, car_tip=_r3_tip, drop_ga=False, drop_car=False):
            tn = _n if table_n is None else table_n
            tr = (_m, _g, _o) if tri is None else tri
            nn = _n if note_n is None else note_n
            _NL3 = chr(10)
            body = "| 1 | 工具门 | ⇒ **%d 项全绿**；`--inventory` rc=0 |" % tn + _NL3
            if not drop_tri:
                body += ("- ⇒ **%d 项全绿**；`--inventory` ⇒ 清点 **%d = %d + %d + 0**"
                         % (_n, tr[0], tr[1], tr[2])) + _NL3
                body += ("> 提示：**%d 项 / 清点 %d = %d + %d + 0**"
                         % (nn, tr[0], tr[1], tr[2])) + _NL3
            # ④⑤：与生产文档同形的两行（不写死 85 —— 跟着实跑值走，否则下次加牙自己就红了）。
            gv = (_ga_real, _ga_real) if ga is None else ga
            cv = _r3_car if car is None else car
            if not drop_ga:
                body += ("- `goal_gate selftest` ⇒ **%d 条断言 %d 通过**" % gv) + _NL3
            if not drop_car:
                body += (("- `cargo test --workspace` ⇒ **%d suites，running %d = passed %d + "
                          "ignored %d**" % cv)
                         + (("（`%s` 实测）" % car_tip) if car_tip else "") + _NL3)
            with open(_r3_path, "w", encoding="utf-8") as fh:
                fh.write(body)

        _r3_write()
        _r3_rc, _r3_dt = _r3_check()
        chk(_r3_rc == 0 and "一致" in _r3_dt, "R3a 三种渲染都一致 ⇒ rc=0（%r）" % (_r3_dt,))

        _r3_write(table_n=_n + 1)
        _r3b_rc, _r3b_dt = _r3_check()
        chk(_r3b_rc == 1 and "实况 len(REGISTRY)=" in _r3b_dt,
            "R3b 表行写错一项 ⇒ rc=1 并点名（%r）" % (_r3b_dt,))

        _r3_write(note_n=_n + 1)
        _r3e_rc, _r3e_dt = _r3_check()
        chk(_r3e_rc == 1 and "实况 len(REGISTRY)=" in _r3e_dt,
            "R3e 提示行 `**N 项 / 清点 …**` 写错 ⇒ rc=1（%r）" % (_r3e_dt,))

        _r3_write(tri=(_m, _g + 1 if _g < _m else _g, _o))
        _r3d_rc, _r3d_dt = _r3_check()
        chk(_r3d_rc == 1 and "清点锚写作" in _r3d_dt,
            "R3d 紧凑三元组写错 ⇒ rc=1（%r）" % (_r3d_dt,))

        _r3_write(drop_tri=True)
        _r3c_rc, _r3c_dt = _r3_check()
        chk(_r3c_rc == 2 and "找不到任何「清点" in _r3c_dt,
            "R3c 三元组锚被删掉 ⇒ rc=2 判不了（不静默通过；%r）" % (_r3c_dt,))
        # 活锚地板（Leader 2026-10-27）：某一类被「历史」排除到空 ⇒ rc=2 判不了。
        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("> 历史：**%d 项全绿**（旧记录）" % (_n - 1) + chr(10)
                     + "清点 **%d = %d + %d + 0**" % (_m, _g, _o) + chr(10))
        _r3f_rc, _r3f_dt = _r3_check()
        chk(_r3f_rc == 2 and "N 类活锚为空" in _r3f_dt and "历史" in _r3f_dt,
            "R3f N 类的唯一活锚被『历史』排除 ⇒ rc=2 判不了（%r）" % (_r3f_dt,))
        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("**%d 项全绿**" % _n + chr(10)
                     + "> 历史：清点 **%d = %d + %d + 0**（旧记录）" % (_m - 1, _g, _o) + chr(10))
        _r3g_rc, _r3g_dt = _r3_check()
        chk(_r3g_rc == 2 and "三元组类活锚为空" in _r3g_dt,
            "R3g 三元组类的唯一活锚被『历史』排除 ⇒ rc=2 判不了（%r）" % (_r3g_dt,))
        _r3_write()
        with open(_r3_path, "a", encoding="utf-8") as fh:
            fh.write("> 历史：清点 52 = 47 + 5 + 0（旧记录）" + chr(10))
        _r3h_rc, _r3h_dt = _r3_check()
        chk(_r3h_rc == 0 and "排除历史 1 处（行" in _r3h_dt,
            "R3h 被排除的条数与行号出现在输出里（%r）" % (_r3h_dt,))
        # 伪造历史（review 实测的洞）：把写错的锚前缀加「历史」躲进排除集 ⇒ 排除项 ≥ 现值 ⇒ rc=1。
        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("> 历史：**%d 项全绿**（伪造）" % (_n + 1) + chr(10)
                     + "| 1 | 工具门 | ⇒ **%d 项全绿**；`--inventory` rc=0 |" % _n + chr(10)
                     + "清点 **%d = %d + %d + 0**" % (_m, _g, _o) + chr(10))
        _r3i_rc, _r3i_dt = _r3_check()
        chk(_r3i_rc == 1 and "不能当历史" in _r3i_dt,
            "R3i 单锚伪装：`历史：**N+1 项全绿**` ⇒ rc=1（不能当历史；%r）" % (_r3i_dt,))
        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("> 历史：**%d 项全绿**（伪造1）" % (_n + 1) + chr(10)
                     + "- 历史：**%d 项全绿**（伪造2）" % (_n + 2) + chr(10)
                     + "| 1 | 工具门 | ⇒ **%d 项全绿**；`--inventory` rc=0 |" % _n + chr(10)
                     + "清点 **%d = %d + %d + 0**" % (_m, _g, _o) + chr(10))
        _r3j_rc, _r3j_dt = _r3_check()
        chk(_r3j_rc == 1 and "不能当历史" in _r3j_dt,
            "R3j 多锚伪装（N 活 1/共 3）：两处 53/54 带历史前缀 ⇒ rc=1（%r）" % (_r3j_dt,))
        # ④⑤（卡 openmir2-goal-gate-count-anchor）：同一块文档的另两行也必须有牙。
        def _r3_live_lines(ga=None, car=None, tip=_r3_tip):
            _N = chr(10)
            g = (_ga_real, _ga_real) if ga is None else ga
            c = _r3_car if car is None else car
            return ("| 1 | 工具门 | ⇒ **%d 项全绿**；`--inventory` rc=0 |" % _n + _N
                    + "清点 **%d = %d + %d + 0**" % (_m, _g, _o) + _N
                    + "- `goal_gate selftest` ⇒ **%d 条断言 %d 通过**" % g + _N
                    + "- `cargo test --workspace` ⇒ **%d suites，running %d = passed %d + "
                      "ignored %d**%s" % (c + ((("（`%s` 实测）" % tip) if tip else ""),)) + _N)

        _r3_write(ga=(_ga_real + 1, _ga_real + 1))
        _r4a_rc, _r4a_dt = _r3_check()
        chk(_r4a_rc == 1 and ("实跑 %d" % _ga_real) in _r4a_dt,
            "R4a 断言数写成实跑+1 ⇒ rc=1 并点名实跑值（%r）" % (_r4a_dt,))

        _r3_write(ga=(_ga_real, _ga_real - 1))
        _r4f_rc, _r4f_dt = _r3_check()
        chk(_r4f_rc == 1 and "自相矛盾" in _r4f_dt,
            "R4f 「N 条断言 M 通过」两数不等 ⇒ rc=1（自相矛盾；%r）" % (_r4f_dt,))

        _r3_write(drop_ga=True)
        _r4b_rc, _r4b_dt = _r3_check()
        chk(_r4b_rc == 2 and "断言数类活锚为空" in _r4b_dt,
            "R4b 断言数整行删掉 ⇒ rc=2 判不了（退化输入；%r）" % (_r4b_dt,))

        _r3_write()
        _r4c_rc, _r4c_dt = _r3_check(goal_cmd=[
            sys.executable, "-I", "-B", os.path.join(_r3_root, "no-such-goal.py"), "selftest"])
        chk(_r4c_rc == 2 and "断言数类判不了" in _r4c_dt,
            "R4c `goal_gate selftest` 跑不起来 ⇒ rc=2 判不了（不是绿、也不是红；%r）" % (_r4c_dt,))

        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("> 历史：**%d 条断言 %d 通过**（旧记录）" % (_ga_real - 1, _ga_real - 1) + chr(10)
                     + "| 1 | 工具门 | ⇒ **%d 项全绿**；`--inventory` rc=0 |" % _n + chr(10)
                     + "清点 **%d = %d + %d + 0**" % (_m, _g, _o) + chr(10)
                     + "- `cargo test --workspace` ⇒ **%d suites，running %d = passed %d + ignored %d**（`%s` 实测）" % (_r3_car + (_r3_tip,)) + chr(10))
        _r4d_rc, _r4d_dt = _r3_check()
        chk(_r4d_rc == 2 and "断言数类活锚为空" in _r4d_dt,
            "R4d 断言数类的唯一活锚被『历史』排除 ⇒ rc=2（活锚地板；%r）" % (_r4d_dt,))

        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("> 历史：**%d 条断言 %d 通过**（伪造）" % (_ga_real + 1, _ga_real + 1) + chr(10)
                     + "| 1 | 工具门 | ⇒ **%d 项全绿**；`--inventory` rc=0 |" % _n + chr(10)
                     + "清点 **%d = %d + %d + 0**" % (_m, _g, _o) + chr(10)
                     + "- `cargo test --workspace` ⇒ **%d suites，running %d = passed %d + ignored %d**（`%s` 实测）" % (_r3_car + (_r3_tip,)) + chr(10))
        _r4e_rc, _r4e_dt = _r3_check()
        chk(_r4e_rc == 1 and "不能当历史" in _r4e_dt,
            "R4e 伪造历史（断言数排除值 ≥ 实跑）⇒ rc=1（%r）" % (_r4e_dt,))

        _r3_write(car=(141, 3366, 3303, 62))
        _r5a_rc, _r5a_dt = _r3_check()
        chk(_r5a_rc == 1 and "不自洽" in _r5a_dt,
            "R5a cargo 三元组 running ≠ passed+ignored ⇒ rc=1（自洽判据；%r）" % (_r5a_dt,))

        _r3_write(car_tip=None)
        _r5b_rc, _r5b_dt = _r3_check()
        chk(_r5b_rc == 2 and "没有标注测量 tip" in _r5b_dt,
            "R5b cargo 锚不标 tip ⇒ rc=2（无从对账；%r）" % (_r5b_dt,))

        _r3_write()   # 恢复带 tip 的形态：后面的牙只改证据、不改文档
        _r3_ev_write(drop=True)
        _r5c_rc, _r5c_dt = _r3_check()
        chk(_r5c_rc == 2 and "证据文件缺失" in _r5c_dt,
            "R5c 证据文件缺失 ⇒ rc=2 判不了（不是绿；%r）" % (_r5c_dt,))
        _r3_ev_write()

        _r3_ev_write(entries={})
        _r5d_rc, _r5d_dt = _r3_check()
        chk(_r5d_rc == 2 and ("证据里没有 tip `%s`" % _r3_tip) in _r5d_dt,
            "R5d 证据里没有该 tip（缺失/过期）⇒ rc=2（%r）" % (_r5d_dt,))
        _r3_ev_write()

        _r3_ev_write(entries={_r3_tip: {"suites": 141, "running": 3358, "passed": 3295,
                                        "ignored": 63, "rc": 0}})
        _r5e_rc, _r5e_dt = _r3_check()
        chk(_r5e_rc == 1 and "cargo 锚 running 写作 3366" in _r5e_dt,
            "R5e 证据数值与文档不符 ⇒ rc=1 并点名（%r）" % (_r5e_dt,))
        _r3_ev_write()

        _r3_ev_write(entries={_r3_tip: {"suites": 141, "running": 3366, "passed": 3303,
                                        "ignored": 63, "rc": 1}})
        _r5f_rc, _r5f_dt = _r3_check()
        chk(_r5f_rc == 2 and "不能当证据" in _r5f_dt,
            "R5f 该 tip 的实跑 rc≠0 ⇒ rc=2（失败的真跑不能当证据；%r）" % (_r5f_dt,))
        _r3_ev_write()

        _r3_write(drop_car=True)
        _r5g_rc, _r5g_dt = _r3_check()
        chk(_r5g_rc == 2 and "cargo 类活锚为空" in _r5g_dt,
            "R5g cargo 整行删掉 ⇒ rc=2 判不了（%r）" % (_r5g_dt,))

        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("> 历史：`cargo test --workspace` ⇒ **141 suites，running 3358 = passed 3295 + "
                     "ignored 63**（`%s` 实测）" % _r3_tip + chr(10)
                     + "| 1 | 工具门 | ⇒ **%d 项全绿**；`--inventory` rc=0 |" % _n + chr(10)
                     + "清点 **%d = %d + %d + 0**" % (_m, _g, _o) + chr(10)
                     + "- `goal_gate selftest` ⇒ **%d 条断言 %d 通过**" % (_ga_real, _ga_real) + chr(10))
        _r5h_rc, _r5h_dt = _r3_check()
        chk(_r5h_rc == 2 and "cargo 类活锚为空" in _r5h_dt,
            "R5h cargo 类的唯一活锚被『历史』排除 ⇒ rc=2（活锚地板；%r）" % (_r5h_dt,))

        with open(_r3_path, "w", encoding="utf-8") as fh:
            fh.write("> 历史：`cargo test --workspace` ⇒ **141 suites，running 3358 = passed 3295 + "
                     "ignored 63**（`%s` 实测）" % _r3_tip + chr(10)
                     + _r3_live_lines())
        _r5i_rc, _r5i_dt = _r3_check()
        chk(_r5i_rc == 1 and "不能当历史" in _r5i_dt,
            "R5i 排除行与活锚同一 tip 但数值不同 ⇒ rc=1（不能当历史；%r）" % (_r5i_dt,))

        _r3_write()   # R5i 手写过文档：先恢复干净形态，再单独动证据
        _r3_ev_write(entries={_r3_tip: {"suites": 141, "running": 3400, "passed": 3303,
                                        "ignored": 63, "rc": 0}})
        _r5j_rc, _r5j_dt = _r3_check()
        chk(_r5j_rc == 2 and ("不自洽" in _r5j_dt) and ("坏证据" in _r5j_dt),
            "R5j 证据自身 running ≠ passed+ignored ⇒ rc=2（坏证据 = 判不了，不是文档写错）"
            "（%r）" % (_r5j_dt,))
        _r3_ev_write(entries={_r3_tip: {"suites": 141, "running": 3366, "passed": 3303, "rc": 0}})
        _r5k_rc, _r5k_dt = _r3_check()
        chk(_r5k_rc == 2 and "缺字段" in _r5k_dt,
            "R5k 证据条目缺 ignored 字段 ⇒ rc=2（坏证据，判不了；%r）" % (_r5k_dt,))
        _r3_ev_write()

        _r5l_root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        _r5l_head = subprocess.run(["git", "-C", _r5l_root, "rev-parse", "HEAD"],
                                   capture_output=True, text=True).stdout.strip()
        chk(bool(_r5l_head) and _tip_distance(_r5l_root, _r5l_head[:9]) == 0,
            "R5l 距离 info：真实 tip（HEAD 前缀）⇒ 0 个提交（实得 %r）"
            % (_tip_distance(_r5l_root, _r5l_head[:9]),))
        chk(_tip_distance(_r5l_root, "0" * 9) is None,
            "R5m 距离 info：查不到的 tip ⇒ None（**不判红**，只是不显示）")
        _r5y_rc, _r5y_dt = _r3_check()      # 夹具 tip 是合成的 ⇒ 距离显示不出来，但必须仍 rc=0
        chk(_r5y_rc == 0 and "距 HEAD" not in _r5y_dt,
            "R5n 合不成的 tip 不显示距离且**不判红**（info 通道不许变成红；%r）" % (_r5y_dt,))

        _r3_write()
        _r3_ev_write()
        _r3z_rc, _r3z_dt = _r3_check()
        chk(_r3z_rc == 0, "R4z/R5z 复原后仍 rc=0（%r）" % (_r3z_dt,))
        chk(r_c3["rc"] == NEED_RC and classify_rc(r_c3["rc"]) == NEED
            and "PowerShell" in r_c3["tail"],
            "TSG 牙C3 找不到 PowerShell ⇒ run_one rc=2/NEED（判不了，不抛异常）")
        # F3（review bdd1a28c）：`docs_delivery_gate` 的**守卫调用点**也要有牙——
        # 只删调用、函数留着时，那边的自测仍 52/52 绿（下一个人顺手摘掉链路不会红）。
        # 做法：把 _assert_fixture_consistent 换成抛异常的探针，再整跑一次 docs gate selftest；
        # 调用点若被删 ⇒ 探针不触发 ⇒ 那边 rc=0（本牙红）。
        import importlib.util as _ilu
        _dg_path = os.path.join(root, "tools", "hygiene", "docs_delivery_gate.py")
        _spec = _ilu.spec_from_file_location("_dg_probe", _dg_path)
        _dg = _ilu.module_from_spec(_spec)
        _spec.loader.exec_module(_dg)

        def _guard_bomb(*a, **k):
            raise _dg.GitUnavailable("守卫调用点牙（F3）：夹具必须过守卫")

        _real_guard = _dg._assert_fixture_consistent
        _dg._assert_fixture_consistent = _guard_bomb
        try:
            import contextlib as _cl3
            import io as _io3
            _buf3 = _io3.StringIO()
            with _cl3.redirect_stdout(_buf3):
                _f3_rc = _dg.selftest()
            _f3_out = _buf3.getvalue()
        finally:
            _dg._assert_fixture_consistent = _real_guard
        chk(_f3_rc == 2 and "守卫调用点牙" in _f3_out,
            "TSG 牙F3 docs gate 的守卫**调用点**在链路上（删调用 ⇒ 本牙红；rc=%r）" % (_f3_rc,))

        # F1rev（Leader 2026-10-27 反向要求）：`skip()` 是「把红变不红」的通道，必须证明它**不能**
        # 掩盖真正的 FAIL——PS 在场（which 假返回）且 PS 牙真失败 ⇒ 自测必须仍红。
        if not _PROBE["active"]:
            _PROBE["active"] = True
            _real_which_f1 = shutil.which
            _real_run_one_f1 = globals()["run_one"]

            def _fake_which_f1(name, *a, **k):
                return ("C:/fake/pwsh.exe" if name in ("pwsh", "powershell")
                        else _real_which_f1(name, *a, **k))

            def _fake_run_one_f1(repo, name, argv, timeout):
                if name.startswith("ps_"):
                    return {"name": name, "argv": list(argv), "rc": 1, "elapsed_s": 0.0,
                            "tail": "fake PS failure（牙注入）", "timed_out": False}
                return _real_run_one_f1(repo, name, argv, timeout)

            globals()["shutil"].which = _fake_which_f1
            globals()["run_one"] = _fake_run_one_f1
            try:
                _buf_f1 = io.StringIO()
                with redirect_stdout(_buf_f1):
                    _rc_f1 = selftest()
                _out_f1 = _buf_f1.getvalue()
            finally:
                globals()["shutil"].which = _real_which_f1
                globals()["run_one"] = _real_run_one_f1
                _PROBE["active"] = False
            chk(_rc_f1 == 1 and any(l.startswith("FAIL TSG 牙14") for l in _out_f1.splitlines()),
                "TSG 牙F1rev PS 在场且 PS 牙真失败 ⇒ 自测仍红（rc=%r）；SKIP 通道没掩盖 FAIL" % (_rc_f1,))

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
    print("%s: tool_selftest_gate selftest %d/%d%s"
          % ("GREEN" if not fail else "RED", ok, ok + fail,
             ("（SKIP %d）" % skip_n) if skip_n else ""))
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
