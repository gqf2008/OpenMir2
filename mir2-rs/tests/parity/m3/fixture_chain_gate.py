#!/usr/bin/env python3
"""M3 夹具链深检门（fixture_chain_gate.py）——`fixture_matrix.py` 的伴生「runner 级」回归门。

背景：`fixture_matrix.py`（M3 夹具矩阵与就绪度门）刻意只做**轻量**核对（三件套在场 +
比较器 `selftest`），以便高频跑、不依赖 dotnet。它**不跑各夹具驱动自己的
`-SelfCheckNoStack`**——而那正是「共享管线（`src/BotSrv/**` + `run_pair.ps1` /
`gate_guards.ps1` / `run_bots.ps1`）被多张夹具卡反复扩展后有没有互相踩坏」的**唯一可判决面**
（owner 本日已两次手工解 9 处共享文件冲突）。

本文件就是那个**深检伴生门**：登记表仍单一取自 `fixture_matrix.MATRIX`（不复制第二真源），
对每个 current-tree 状态下 READY 的系统**额外**跑：

  * 该系统的 `runner -SelfCheckNoStack`（离线、零端口）；rc=0 ⇒ `RUN-OK`，
    rc=2 ⇒ `PREREQ`（该驱动自己声明的「前置未到」fail-closed），其它 rc ⇒ `RED`；
  * 共享管线自检：`tools/botload/run_bots.ps1 -SelfCheckNoStack` 与
    `mir2-rs/tests/parity/m3/selftest_gate.ps1`。

fail-closed 口径：
  * 三件套缺件（`PENDING`/`INCONSISTENT`）与 runner 的 `PREREQ` **默认只登记**，
    `--require-all` 时判 `RED`（避免未合卡的正常 PENDING 噪声）；
  * **在场却坏**（比较器 selftest rc≠0 / runner rc∉{0,2} / 共享管线 rc≠0）⇒ **恒 RED**；
  * `INCONSISTENT`（半拉子合并）⇒ **恒 RED**；
  * 绝不对「没跑」判绿。

命令：
  python -I fixture_chain_gate.py check [--root HERE] [--require-all] [--skip-shared]
      [--json OUT] [--evidence-dir DIR] [--timeout SEC]
  python -I fixture_chain_gate.py selftest

口径：**只读**（只读文件 + 跑各件自带的无栈自检，不写任何仓库文件、不起栈、不占端口）；
`selftest` 在内存/临时目录里用注入式 stub 造合成件，含 fail-closed 与改坏必红。
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
# `python -I` 会连 `-P` 一起开（不把脚本目录放进 sys.path）⇒ 显式补回，才能 import 单一真源。
if HERE not in sys.path:
    sys.path.insert(0, HERE)

# 仓库根：`mir2-rs/tests/parity/m3` 上溯四级。共享管线（tools/**）的路径相对它解析，
# 各夹具三件套（runner/comparator/scenario）的路径相对 HERE（= 矩阵 --root）解析。
REPO_ROOT = os.path.abspath(os.path.join(HERE, os.pardir, os.pardir, os.pardir, os.pardir))

import fixture_matrix as fm  # noqa: E402  （单一真源：MATRIX / evaluate / _default_run_comparator）

# 共享管线 + 主机门自检（跨夹具卡反复扩展的接缝；任一红即「链被踩坏」）。
# `.py` 项按扩展名走 `python -I`（见 `_tool_argv`）。
SHARED_CHECKS = [
    ("run_bots", "tools/botload/run_bots.ps1", ["-SelfCheckNoStack"]),
    ("selftest_gate", "mir2-rs/tests/parity/m3/selftest_gate.ps1", []),
    # 主机门（reviewer-5 ⑧-6 O-1）：castle 锚门不再只是「手工读数」，接入 runner 常跑。
    #   selftest = 门自身表结构/牙齿；check = 真身锚 + 覆盖面（未登记/裸锚）。
    ("castle_anchor_selftest", "mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py", ["selftest"]),
    ("castle_anchor_check", "mir2-rs/tests/parity/m3/castle_oracle_anchor_gate.py", ["check"]),
    # 主机门（⑧-10 残余 Minor：「锚门未注册进 fixture_chain 链深检门」）：行会/攻城夹具的
    #   场景锚门同口径接入常跑，与上面 castle 锚门同形。此前它只挂在
    #   `run_guild_castle_pair.ps1 -SelfCheckNoStack` 里 ⇒ 系统 8 一旦不在 READY
    #   （三件套缺件 PENDING/INCONSISTENT），该锚门就只剩手工读数；登记进共享管线后
    #   不看矩阵状态、独立常跑（在场却坏 ⇒ 恒 RED）。
    ("guild_castle_anchor_selftest", "mir2-rs/tests/parity/m3/guild_castle_anchor_gate.py", ["selftest"]),
    ("guild_castle_anchor_check", "mir2-rs/tests/parity/m3/guild_castle_anchor_gate.py", ["check"]),
    # 主机门（⑨-5 残余 Minor，与上面两族同形 follow-up）：英雄/宠物夹具的场景锚门
    #   （`hero_pet_anchor_gate.py`，由 ⑨-5 收口片新增、随 master f0b333c96 合入）
    #   同样只挂在 `run_hero_pet_pair.ps1 -SelfCheckNoStack` 里 ⇒ 系统 9 一旦不在 READY
    #   （三件套缺件 PENDING/INCONSISTENT），该锚门就只剩手工读数。登记进共享管线后
    #   不看矩阵状态、独立常跑（在场却坏 ⇒ 恒 RED）。
    ("hero_pet_anchor_selftest", "mir2-rs/tests/parity/m3/hero_pet_anchor_gate.py", ["selftest"]),
    ("hero_pet_anchor_check", "mir2-rs/tests/parity/m3/hero_pet_anchor_gate.py", ["check"]),
    # 主机门（⑩-3 残余 Minor：「场景 §B 的 oracle 锚点本轮未行级核对」，与上面两族同形 follow-up）：
    #   元宝寄售夹具的场景锚门（`selloff_anchor_gate.py`，由本片新增）此前完全没有，
    #   ⑩ 场景锚可以静默漂移 / 复制错行号。登记进共享管线后不看矩阵状态、独立常跑
    #   （在场却坏 ⇒ 恒 RED）。
    ("selloff_anchor_selftest", "mir2-rs/tests/parity/m3/selloff_anchor_gate.py", ["selftest"]),
    ("selloff_anchor_check", "mir2-rs/tests/parity/m3/selloff_anchor_gate.py", ["check"]),
    # 主机门（⑥-5 / ⑦-5 锚门族覆盖面收口片，与上面四族同形）：任务（⑥-5）与组队/交易（⑦-5）
    #   两个场景件同样带 `oracle_anchors`，但此前从未有行级门 —— 锚可静默漂移 / 写错文件基名
    #   （`Query.cs`/`Save.cs`/`Base.cs` 都不在树里）而不被发现。登记进共享管线后不看矩阵状态、
    #   独立常跑（在场却坏 ⇒ 恒 RED），把“谁被 reviewer 点名谁补”的锚门族补成 ⑥⑦⑧⑨⑩ 全覆盖。
    ("quest_anchor_selftest", "mir2-rs/tests/parity/m3/quest_anchor_gate.py", ["selftest"]),
    ("quest_anchor_check", "mir2-rs/tests/parity/m3/quest_anchor_gate.py", ["check"]),
    ("party_trade_anchor_selftest", "mir2-rs/tests/parity/m3/party_trade_anchor_gate.py", ["selftest"]),
    ("party_trade_anchor_check", "mir2-rs/tests/parity/m3/party_trade_anchor_gate.py", ["check"]),
    # 共享管线门（本片新增，与上面三组主机门同形）：markers 前缀口径门。它判的正是
    #   「多张夹具卡反复扩展 `run_pair.ps1` / BotSrv 后有没有互相踩坏」：run_pair 去前缀截断 +
    #   每个新夹具族的 `Assert-M3MarkerPrefix` 牙必须登记（COMPARATORS / GUARD_ONLY_MARKERS）+
    #   比较器按前缀扫描 + BotSrv 无裸 `skip=` 诊断。
    #   背景：④ 夹具（`M3DROPEXPSEQ` 牙 + `drop-exp/drop_exp_diff.py`）合入后漏登记，
    #   门只靠手工 `selftest` 才发现 —— 接入共享管线后不看矩阵状态、独立常跑
    #   （在场却坏 ⇒ 恒 RED，不再静默）。
    ("marker_prefix_selftest", "mir2-rs/tests/parity/m3/marker_prefix_gate.py", ["selftest"]),
    ("marker_prefix_check", "mir2-rs/tests/parity/m3/marker_prefix_gate.py", ["check"]),
    # 宝宝/下属登记门（`m3_pet_slave_registry_gate.py`，⑨-2/⑨-3 reviewer-5 M-1 收口片）此前
    #   **无任何调用方** —— 只在它自己的证据 README 里手工跑。⇒ 与 markers 前缀门同类的
    #   「在场却坏无声」隐患：缺登记行 / 撞号 / 锚点不符会静默通过整条链。接入共享管线后
    #   不看矩阵状态、独立常跑（在场却坏 ⇒ 恒 RED，绝不判绿）。
    ("pet_slave_registry_selftest", "mir2-rs/tests/parity/m3/m3_pet_slave_registry_gate.py", ["selftest"]),
    ("pet_slave_registry_check", "mir2-rs/tests/parity/m3/m3_pet_slave_registry_gate.py", ["check"]),
    # 白名单登记号审计门（`m3_whitelist_registry_audit.py`，卡 `openmir2-m3-whitelist-registry-integrity`）
    #   此前**无任何调用方**（reviewer-6 `4f5d3eeb1` O-1：纯审计、非 fail-closed 门仍零调用方）——
    #   与宝宝登记门同类的「在场却坏无声」隐患。本片接入其 **selftest**（工具自身逻辑 + 改坏必红）。
    #   ⚠ 未接 `audit --strict`：`whitelist.md` 现存**两行 `| T-42 |`**（⑤-2 商人 tick vs ⑥-5 任务夹具），
    #   该撞号是 owner 已路由的跨车道登记号重编（⑤-2 证据 README 在 `tests/parity/world/**`，
    #   非本车道写集）⇒ `--strict` 现为 RED。等 owner 裁决重编后，把 `audit --strict` 补登记为 `check`
    #   （届时本段注释一并更新）。
    ("whitelist_registry_selftest", "mir2-rs/tests/parity/m3/m3_whitelist_registry_audit.py", ["selftest"]),
    # 主机门（本片新增，锚门族最后一处「在场却坏无声」）：本卡场景两相 trace 的承接卡规格锚门
    #   `rng-trace-scene/check_anchors.py`——它把 `DRIVER-CONTRACT.md` 里每个 `<path>:<line>`
    #   引用与**当前源码**的首个匹配行钉成 fail-closed。它此前**零调用方**（只靠手工跑证据
    #   README）⇒ 实测已静默漂移：`main.rs:5598` 因合并增长到 `:5665` 而 gate RED、无人发现。
    #   接入共享管线后不看矩阵状态、独立常跑（在场却坏 ⇒ 恒 RED）。
    ("rng_scene_anchor_selftest", "mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py", ["selftest"]),
    ("rng_scene_anchor_check", "mir2-rs/tests/parity/m3/rng-trace-scene/check_anchors.py", ["check"]),
]

RUNNER_PREREQ_RC = 2  # 各驱动约定的「前置未到，fail-closed」退出码


def _tail(text, limit=200):
    for line in (text or "").splitlines()[::-1]:
        if line.strip():
            return line.strip()[:limit]
    return ""


def _powershell():
    return shutil.which("powershell") or shutil.which("pwsh") or "powershell"


def _ps_argv(script, extra):
    return [_powershell(), "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script] + list(extra)


def _tool_argv(script, extra):
    """按扩展名分派：`.py` ⇒ `python -I`；其它 ⇒ powershell `-File`。"""
    if script.lower().endswith(".py"):
        return [sys.executable, "-I", script] + list(extra)
    return _ps_argv(script, extra)


def _run_tool(script, extra, cwd, timeout):
    """跑一个主机工具（`.py` / `.ps1`）⇒ (rc, tail, stdout+stderr)。缺文件 ⇒ (127, ...)。"""
    if not os.path.isfile(script):
        return 127, "missing: " + script, ""
    try:
        p = subprocess.run(_tool_argv(script, extra), cwd=cwd, capture_output=True,
                           text=True, encoding="utf-8", errors="replace", timeout=timeout)
    except (OSError, subprocess.SubprocessError) as exc:  # pragma: no cover - 环境异常
        return 127, "无法运行 {0}: {1}".format(script, exc), ""
    blob = (p.stdout or "") + (p.stderr or "")
    return p.returncode, _tail(blob), blob


def make_run_runner(timeout):
    """默认 runner 执行器（可注入：selftest 用 stub 替换）。⇒ (rc, tail, blob)。"""
    def _run(root, entry):
        return _run_tool(os.path.join(root, entry["runner"]), ["-SelfCheckNoStack"], root, timeout)
    return _run


def make_run_shared(timeout):
    """默认共享管线执行器（路径相对 REPO_ROOT，cwd=REPO_ROOT）。⇒ [{name,file,present,rc,tail,blob}]。"""
    def _run(root):
        out = []
        for name, rel, extra in SHARED_CHECKS:
            path = os.path.join(REPO_ROOT, rel)
            present = os.path.isfile(path)
            if present:
                rc, tail, blob = _run_tool(path, extra, REPO_ROOT, timeout)
            else:
                rc, tail, blob = None, "", ""
            out.append({"name": name, "file": rel, "present": present,
                        "rc": rc, "tail": tail, "blob": blob})
        return out
    return _run


# ---------------------------------------------------------------------------
# 纯分类 + 汇总（可注入，供 selftest）
# ---------------------------------------------------------------------------
def classify_runner(rc):
    """runner rc ⇒ 状态：0 RUN-OK / 2 PREREQ / 其它 RED。纯函数。"""
    if rc == 0:
        return "RUN-OK"
    if rc == RUNNER_PREREQ_RC:
        return "PREREQ"
    return "RED"


def collect(root, matrix, run_comparator, run_runner, run_shared,
            require_all=False, check_whitelist_path=None, skip_shared=False):
    """⇒ (receipts, shared, reds)。比较器面复用 fixture_matrix.evaluate（同一口径）。"""
    base_receipts, reds = fm.evaluate(
        root, matrix, run_comparator,
        require_all=require_all, check_whitelist_path=check_whitelist_path)
    by_key = {e["key"]: e for e in matrix}
    receipts = []
    for base in base_receipts:
        rec = dict(base)
        entry = by_key[rec["key"]]
        rec["runner_rc"] = None
        rec["runner_tail"] = ""
        rec["runner_status"] = "SKIP"
        if rec["status"] == "READY":
            rc, tail, _blob = run_runner(root, entry)
            rec["runner_rc"] = rc
            rec["runner_tail"] = tail
            status = classify_runner(rc)
            rec["runner_status"] = status
            if status == "RED":
                rec["status"] = "RED"
                reds.append("{0} {1}：runner -SelfCheckNoStack rc={2}（{3}）".format(
                    entry["key"], entry["system"], rc, tail))
            elif status == "PREREQ" and require_all:
                reds.append("{0} {1}：runner 前置未到 rc=2（{2}）⇒ --require-all 判 RED".format(
                    entry["key"], entry["system"], tail))
        receipts.append(rec)

    shared = [] if skip_shared else run_shared(root)
    for s in shared:
        if not s["present"]:
            if require_all:
                reds.append("共享管线 {0}：缺件 {1} ⇒ --require-all 判 RED".format(s["name"], s["file"]))
        elif s["rc"] != 0:
            reds.append("共享管线 {0}：{1} rc={2}（{3}）".format(s["name"], s["file"], s["rc"], s["tail"]))
    return receipts, shared, reds


# ---------------------------------------------------------------------------
# selftest：注入式 stub + fail-closed + 改坏必红
# ---------------------------------------------------------------------------
def cmd_selftest(_args):
    ok = 0
    total = 0

    def expect(name, cond):
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
            print("  SELFTEST ok  {0}".format(name))
        else:
            print("  SELFTEST FAIL {0}".format(name))

    # 1) 纯分类
    expect("classify_runner(0) ⇒ RUN-OK", classify_runner(0) == "RUN-OK")
    expect("classify_runner(2) ⇒ PREREQ", classify_runner(2) == "PREREQ")
    expect("classify_runner(1) ⇒ RED", classify_runner(1) == "RED")
    expect("classify_runner(127) ⇒ RED", classify_runner(127) == "RED")

    entries = [
        {"key": "A", "system": "ok", "card": "card-a", "t": None,
         "runner": "run_a.ps1", "comparator": "a_diff.py", "scenario": "a.json"},
        {"key": "B", "system": "pending", "card": "card-b", "t": None,
         "runner": "run_b.ps1", "comparator": "b_diff.py", "scenario": "b.json"},
        {"key": "C", "system": "runner-prereq", "card": "card-c", "t": None,
         "runner": "run_c.ps1", "comparator": "c_diff.py", "scenario": "c.json"},
        {"key": "D", "system": "runner-red", "card": "card-d", "t": None,
         "runner": "run_d.ps1", "comparator": "d_diff.py", "scenario": "d.json"},
        {"key": "E", "system": "comparator-red", "card": "card-e", "t": None,
         "runner": "run_e.ps1", "comparator": "e_diff.py", "scenario": "e.json"},
    ]
    tmp = tempfile.mkdtemp(prefix="fixture-chain-selftest-")
    for name in ("a", "c", "d", "e"):
        for rel, body in (("run_{0}.ps1".format(name), name),
                          ("{0}_diff.py".format(name), name),
                          ("{0}.json".format(name), "{}")):
            with open(os.path.join(tmp, rel), "w", encoding="utf-8", newline="\n") as fh:
                fh.write(body)

    def fake_comparator(root, comparator):
        return (1, "SYNTHETIC COMPARATOR RED") if comparator == "e_diff.py" else (0, "SYNTHETIC GREEN")

    def fake_runner(root, entry):
        key = entry["key"]
        if key == "C":
            return 2, "SYNTHETIC PREREQ", ""
        if key == "D":
            return 1, "SYNTHETIC RUNNER RED", ""
        return 0, "SYNTHETIC RUNNER GREEN", ""

    def fake_shared(root):
        return [{"name": "run_bots", "file": "run_bots.ps1", "present": True,
                 "rc": 0, "tail": "GREEN", "blob": ""},
                {"name": "selftest_gate", "file": "selftest_gate.ps1", "present": True,
                 "rc": 0, "tail": "GREEN", "blob": ""}]

    # 2) 默认：A RUN-OK；B PENDING 不红；C PREREQ 不红；D runner 红；E comparator 红
    receipts, shared, reds = collect(tmp, entries, fake_comparator, fake_runner, fake_shared,
                                     require_all=False)
    by = {r["key"]: r for r in receipts}
    expect("A READY + RUN-OK", by["A"]["status"] == "READY" and by["A"]["runner_status"] == "RUN-OK")
    expect("B PENDING（runner SKIP）", by["B"]["status"] == "PENDING" and by["B"]["runner_status"] == "SKIP")
    expect("C READY + PREREQ（默认不红）", by["C"]["status"] == "READY" and by["C"]["runner_status"] == "PREREQ")
    expect("D runner rc=1 ⇒ RED", by["D"]["status"] == "RED" and by["D"]["runner_status"] == "RED")
    expect("E comparator rc=1 ⇒ RED", by["E"]["status"] == "RED")
    expect("默认 RED 仅 2 条（D runner + E comparator）", len(reds) == 2)
    expect("默认 PENDING / PREREQ 不判红", not any("PENDING" in r or "前置未到" in r for r in reds))

    # 3) --require-all：B（PENDING）+ C（PREREQ）也判红
    _, _, reds_all = collect(tmp, entries, fake_comparator, fake_runner, fake_shared,
                             require_all=True)
    expect("require-all 把 PENDING 判红", any("尚未合并" in r for r in reds_all))
    expect("require-all 把 runner PREREQ 判红", any("前置未到 rc=2" in r for r in reds_all))

    # 4) 共享管线红 ⇒ 判红
    def fake_shared_red(root):
        return [{"name": "selftest_gate", "file": "selftest_gate.ps1", "present": True,
                 "rc": 3, "tail": "SYNTHETIC SHARED RED", "blob": ""}]
    _, _, reds_sh = collect(tmp, entries, fake_comparator, fake_runner, fake_shared_red)
    expect("共享管线 rc=3 ⇒ 判红", any("共享管线 selftest_gate" in r for r in reds_sh))

    # 5) 共享管线缺件：默认只登记，require-all 判红
    def fake_shared_missing(root):
        return [{"name": "run_bots", "file": "run_bots.ps1", "present": False,
                 "rc": None, "tail": "", "blob": ""}]
    _, _, reds_miss = collect(tmp, entries, fake_comparator, fake_runner, fake_shared_missing)
    expect("共享管线缺件默认不红", not reds_miss or all("共享管线" not in r for r in reds_miss))
    _, _, reds_miss_all = collect(tmp, entries, fake_comparator, fake_runner, fake_shared_missing,
                                  require_all=True)
    expect("共享管线缺件 require-all 判红", any("共享管线 run_bots" in r for r in reds_miss_all))

    # 6) 改坏必红：删 A 的 runner ⇒ A 由 READY 变 INCONSISTENT（恒红）
    os.remove(os.path.join(tmp, "run_a.ps1"))
    receipts_mut, _, reds_mut = collect(tmp, entries, fake_comparator, fake_runner, fake_shared)
    by_mut = {r["key"]: r for r in receipts_mut}
    expect("删 runner ⇒ INCONSISTENT（改坏必红）",
           by_mut["A"]["status"] == "INCONSISTENT" and any("A" == r.split(" ")[0] for r in reds_mut))

    # 7) 真源一致性：MATRIX 的 runner 命名与比较器扩展名（与 fixture_matrix 同口径）
    expect("MATRIX runner 命名合规", all(
        e["runner"].startswith("run_") or "/run_" in e["runner"] for e in fm.MATRIX))
    expect("MATRIX comparator 命名合规", all(e["comparator"].endswith(".py") for e in fm.MATRIX))
    expect("共享管线登记项 ≥2", len(SHARED_CHECKS) >= 2)
    expect("共享管线件在本仓库树内可解析（REPO_ROOT 口径）",
           all(os.path.isfile(os.path.join(REPO_ROOT, rel)) for _n, rel, _e in SHARED_CHECKS))
    expect("主机门（.py）登记项 ≥1",
           any(rel.endswith(".py") for _n, rel, _e in SHARED_CHECKS))
    # ⑧-10 残余 Minor 的防回退牙：行会/攻城锚门必须常驻共享管线（selftest + check 两向），
    #   不能只作为夹具 runner 内部的一步（那样系统 8 不 READY 时锚门就静默不跑）。
    _gc_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("guild_castle_anchor_gate.py")]
    expect("行会/攻城锚门已接入共享管线（⑧-10 残余 Minor，防回退）",
           ["selftest"] in _gc_gate and ["check"] in _gc_gate)
    # ⑨-5 残余 Minor 的防回退牙：英雄/宠物锚门必须常驻共享管线（selftest + check 两向）。
    _hp_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("hero_pet_anchor_gate.py")]
    expect("英雄/宠物锚门已接入共享管线（⑨-5 残余 Minor，防回退）",
           ["selftest"] in _hp_gate and ["check"] in _hp_gate)
    # ⑩-3 残余 Minor 的防回退牙：元宝寄售锚门必须常驻共享管线（selftest + check 两向）。
    #   它此前完全不存在（⑩ 场景锚无人行级复核）⇒ 缺行/漂移/缺陷被“修好”都会静默。
    _so_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("selloff_anchor_gate.py")]
    expect("元宝寄售锚门已接入共享管线（⑩-3 残余 Minor，防回退）",
           ["selftest"] in _so_gate and ["check"] in _so_gate)
    # ⑥-5 / ⑦-5 锚门族覆盖面收口片的防回退牙：任务与组队/交易锚门必须常驻共享管线
    #   （selftest + check 两向）。它们此前完全不存在（⑥/⑦ 场景锚无人行级复核）⇒
    #   缺行/漂移/写错文件基名都会静默。
    _q_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("quest_anchor_gate.py")]
    expect("任务锚门已接入共享管线（⑥ 锚门族覆盖面收口，防回退）",
           ["selftest"] in _q_gate and ["check"] in _q_gate)
    _pt_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("party_trade_anchor_gate.py")]
    expect("组队/交易锚门已接入共享管线（⑦ 锚门族覆盖面收口，防回退）",
           ["selftest"] in _pt_gate and ["check"] in _pt_gate)
    # 本片防回退牙：markers 前缀门必须常驻共享管线（selftest + check 两向）。它当前是
    #   ④/⑤/⑥/⑦/⑧/⑨/⑩ 共享 `run_pair.ps1` 扩展面的唯一机判门，不能只靠手工跑。
    _mp_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("marker_prefix_gate.py")]
    expect("markers 前缀门已接入共享管线（防回退）",
           ["selftest"] in _mp_gate and ["check"] in _mp_gate)
    # 本片防回退牙：⑨-2/⑨-3 的宝宝登记门（`m3_pet_slave_registry_gate.py`）必须常驻共享管线
    #   （selftest + check 两向）。它此前**零调用方**（只靠手工跑证据 README 的读数）⇒
    #   缺行/撞号/锚点不符会静默通过整条链，与本卡修掉的 markers 前缀门同根因。
    _pet_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("m3_pet_slave_registry_gate.py")]
    expect("宝宝登记门已接入共享管线（防回退）",
           ["selftest"] in _pet_gate and ["check"] in _pet_gate)
    # 本片防回退牙：白名单登记号审计门（`m3_whitelist_registry_audit.py`）必须常驻共享管线
    #   （selftest 向）。它此前零调用方（reviewer-6 `4f5d3eeb1` O-1）⇒ 工具自身逻辑回归会静默。
    _wl_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("m3_whitelist_registry_audit.py")]
    expect("白名单登记号审计门 selftest 已接入共享管线（防回退）", ["selftest"] in _wl_gate)
    # 本片防回退牙：本卡场景两相 trace 的承接卡规格锚门（`rng-trace-scene/check_anchors.py`）
    #   必须常驻共享管线（selftest + check 两向）。它此前零调用方（只靠手工跑），实测已静默
    #   漂移（`main.rs:5598`→`:5665`）⇒ 缺行/漂移会静默通过整条链。
    _sa_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("rng-trace-scene/check_anchors.py")]
    expect("RNG 场景锚门已接入共享管线（防回退）",
           ["selftest"] in _sa_gate and ["check"] in _sa_gate)
    expect("主机门（.py）登记项 ≥19（castle + 行会/攻城 + 英雄/宠物 + 元宝寄售 + 任务 + 组队/交易 + markers 前缀 + 宝宝登记 + 白名单审计 + RNG 场景锚 各 selftest[/check]）",
           sum(1 for _n, rel, _e in SHARED_CHECKS if rel.endswith(".py")) >= 19)
    expect("共享管线登记名唯一（防重复注册偷懒）",
           len({n for n, _r, _e in SHARED_CHECKS}) == len(SHARED_CHECKS))
    expect("共享管线 (file,args) 登记唯一",
           len({(r, tuple(e)) for _n, r, e in SHARED_CHECKS}) == len(SHARED_CHECKS))
    expect("_tool_argv(.py) ⇒ python -I",
           _tool_argv("x.py", ["selftest"])[:3] == [sys.executable, "-I", "x.py"])
    expect("_tool_argv(.ps1) ⇒ powershell -File",
           _tool_argv("x.ps1", [])[0] != sys.executable and "-File" in _tool_argv("x.ps1", []))
    pyfile = os.path.join(tmp, "probe.py")
    with open(pyfile, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("import sys\nsys.exit(0)\n")
    rc_probe, _t, _b = _run_tool(pyfile, [], tmp, 60)
    expect("_run_tool(.py) 真跑 rc=0", rc_probe == 0)
    rc_miss, _t2, _b2 = _run_tool(os.path.join(tmp, "nope.py"), [], tmp, 60)
    expect("_run_tool 缺件 ⇒ 127", rc_miss == 127)

    print("SELFTEST {0}: fixture_chain_gate selftest {1}/{2}".format(
        "GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


# ---------------------------------------------------------------------------
# check
# ---------------------------------------------------------------------------
def _print_table(receipts, shared):
    print("== M3 夹具链深检（runner 级）==")
    print("{0:<5} {1:<22} {2:<12} {3:<10} {4}".format("#", "系统", "矩阵状态", "runner", "卡"))
    for r in receipts:
        print("{0:<5} {1:<22} {2:<12} {3:<10} {4}".format(
            r["key"], r["system"], r["status"], r["runner_status"], r["card"]))
        if r["runner_status"] in ("RUN-OK", "PREREQ", "RED"):
            print("       runner -SelfCheckNoStack rc={0} :: {1}".format(
                r["runner_rc"], (r["runner_tail"] or "")[:96]))
    print("-- 共享管线 / 主机门 --")
    for s in shared:
        print("{0:<16} {1:<10} rc={2} :: {3}".format(
            s["name"], "present" if s["present"] else "MISSING",
            s["rc"], (s["tail"] or "")[:80]))


def cmd_check(args):
    root = os.path.abspath(args.root)
    wl = os.path.abspath(os.path.join(root, os.pardir, "whitelist.md"))
    wl = wl if os.path.isfile(wl) else None
    run_runner = make_run_runner(args.timeout)
    run_shared = make_run_shared(args.timeout)
    receipts, shared, reds = collect(
        root, fm.MATRIX, fm._default_run_comparator, run_runner, run_shared,
        require_all=args.require_all, check_whitelist_path=wl, skip_shared=args.skip_shared)

    if args.evidence_dir:
        os.makedirs(args.evidence_dir, exist_ok=True)
        for rec in receipts:
            if rec["runner_status"] in ("RUN-OK", "PREREQ", "RED"):
                name = "runner-{0}-{1}.txt".format(rec["key"].replace("/", "_"), rec["runner_status"])
                with open(os.path.join(args.evidence_dir, name), "w",
                          encoding="utf-8", newline="\n") as fh:
                    fh.write("key={0} system={1} runner_status={2} rc={3}\n{4}\n".format(
                        rec["key"], rec["system"], rec["runner_status"],
                        rec["runner_rc"], rec["runner_tail"]))
        for s in shared:
            name = "shared-{0}.txt".format(s["name"])
            with open(os.path.join(args.evidence_dir, name), "w",
                      encoding="utf-8", newline="\n") as fh:
                fh.write("name={0} file={1} present={2} rc={3}\n{4}\n".format(
                    s["name"], s["file"], s["present"], s["rc"], s["tail"]))

    payload = {
        "require_all": bool(args.require_all),
        "skip_shared": bool(args.skip_shared),
        "receipts": receipts,
        "shared": [{"name": s["name"], "file": s["file"], "present": s["present"],
                    "rc": s["rc"], "tail": s["tail"]} for s in shared],
        "reds": reds,
    }
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(payload, fh, ensure_ascii=False, indent=2, sort_keys=True)
            fh.write("\n")

    _print_table(receipts, shared)
    run_ok = sum(1 for r in receipts if r["runner_status"] == "RUN-OK")
    prereq = sum(1 for r in receipts if r["runner_status"] in ("PREREQ", "SKIP"))
    if reds:
        print("FIXTURE_CHAIN RED: runner_ok={0} prereq_or_skip={1} red={2}".format(
            run_ok, prereq, len(reds)))
        for r in reds:
            print("  RED " + r)
        return 1
    print("FIXTURE_CHAIN GREEN: runner_ok={0} prereq_or_skip={1} red=0{2}".format(
        run_ok, prereq, "（--require-all）" if args.require_all else ""))
    return 0


def main():
    ap = argparse.ArgumentParser(description="M3 夹具链深检门（runner 级，只读、零端口）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p_check = sub.add_parser("check", help="矩阵门 + 各 READY runner -SelfCheckNoStack + 共享管线自检")
    p_check.add_argument("--root", default=HERE)
    p_check.add_argument("--require-all", action="store_true",
                         help="PENDING / runner PREREQ / 共享管线缺件 也判 RED")
    p_check.add_argument("--skip-shared", action="store_true", help="不跑共享管线自检")
    p_check.add_argument("--json", default=None)
    p_check.add_argument("--evidence-dir", default=None)
    p_check.add_argument("--timeout", type=int, default=900, help="单个 runner 的超时秒数")
    p_check.set_defaults(func=cmd_check)
    p_self = sub.add_parser("selftest", help="注入式 stub fail-closed + 改坏必红自检")
    p_self.set_defaults(func=cmd_selftest)
    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
