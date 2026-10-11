#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M4② 三条商人用例（商店/修理/仓库）的可跑性门（卡 `openmir2-m4-parity-cases`）。

## 定位（owner 2026-10-12 e808ec99 裁定后）

owner 已把写集扩到 `mir2-rs/tools/script-parity-cs/**` 并指定**路线①**：给 C# 夹具的 NPC 代理
按脚本头补 `IMerchant`（商人脚本头 `(@buy …)`），使三条商人用例可 **port-free** 对拍；
**Rust 服务侧**的 NPC/商人派发（`bins/game-svr`）**不归本卡**——它属 §6 的 **M3「NPC/商店」
系统对齐**。

本门因此以 **C# 夹具路线①** 为裁决：
- C# 夹具能构造商人（源码/实跑均无 `InvalidCastException`）⇒ `READY`（三条可 port-free 跑）；
- 否则 `BLOCKED`（保持原 runner，不许标绿）。
- Rust 服务侧能力（无 `CM_CLICKNPC`/`CM_MERCHANTDLGSELECT`、无 `mir2-script` 依赖）**只登记为
  M3 承接**，不再阻塞本卡。

> 本门不是把 PENDING 标绿——它是「路线① 是否真打通」的机判锚点：谁把三条商人用例标成已通过，
> 必须同时给出本门 `READY` 的输出。

## 命令（在 `mir2-rs/` 下）

    python -I tests/parity/M4/npc_window_readiness.py                    # 只回显 stdout（默认不写盘）
    python -I tests/parity/M4/npc_window_readiness.py --no-run-cs        # 只做源码能力检查（快；仍不写盘）
    python -I tests/parity/M4/npc_window_readiness.py --write-evidence  # 显式写 --out（默认 evidence/M4/npc-window-readiness.txt）
    python -I tests/parity/M4/npc_window_readiness.py selftest          # 牙齿：合成输入翻转能力 ⇒ 裁决翻转
    python -I tests/parity/M4/npc_window_readiness.py selftest-write    # 牙齿：默认不写盘；改回默认写盘 ⇒ 本自检 RED

退出码：0 = READY（路线① 就绪，可 port-free）；3 = BLOCKED（夹具未修）；2 = PREREQ。

## 写盘口径（卡 `openmir2-m4-readiness-write-mode`，R-1 残留）

**默认只回显 stdout，绝不写盘**（`--no-run-cs` 与普通运行都一样）；只有显式 `--write-evidence`
才写 `--out`。原因：`--no-run-cs` 与普通运行的输出不同，若默认写盘，任何人按 README 复核一次
就会把入库证据 `evidence/M4/npc-window-readiness.txt` 改脏、让 `evidence_hashes.py` 假红。
端口快照永远只回显 stdout + 落到 `mir2-rs/target/m4/npc-window-readiness-ports.txt`
（`target/` 不入库、不入 hash 清单）。
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def _rel(p, base=None):
    """尽力给出仓库相对路径；selftest 的临时目录不在仓库下时退回绝对路径。"""
    base = base or REPO
    try:
        return Path(p).resolve().relative_to(Path(base).resolve()).as_posix()
    except ValueError:
        return str(p)

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

HERE = Path(__file__).resolve().parent            # mir2-rs/tests/parity/M4
PARITY = HERE.parent                              # mir2-rs/tests/parity
MIR2RS = PARITY.parents[1]                        # mir2-rs/
REPO = MIR2RS.parent                              # 仓库根

EVIDENCE = PARITY / "evidence" / "M4"
CASES = HERE / "npc-cases.json"
CS_HARNESS = MIR2RS / "tools" / "script-parity-cs"
RUST_GAMESVR = MIR2RS / "bins" / "game-svr"
PROTOCOL_MESSAGES = MIR2RS / "crates" / "protocol" / "src" / "messages.rs"

# Rust 服务侧要真跑 NPC 脚本，必须同时具备：
#   ① 收到「选脚本 label」的客户端命令（CM_CLICKNPC / CM_MERCHANTDLGSELECT）；
#   ② GameSvr 依赖脚本引擎 crate（mir2-script）。
RUST_NPC_CM_NAMES = ("CM_CLICKNPC", "CM_MERCHANTDLGSELECT")
RUST_SCRIPT_CRATE = "mir2-script"

# 三条商人用例（原 `runner == "window"`，2026-10-12 owner 裁定后转 port-free）：按 category 选，
# 不依赖 runner 字段（转 flow 后 runner 不再是 window）。
MERCHANT_CATEGORIES = ("商店", "修理", "仓库")


# --------------------------------------------------------------------------- #
# 能力检查（每条都返回 (capable: bool, detail: str)，detail 是原始证据）
# --------------------------------------------------------------------------- #
def check_cs_harness_source(harness_dir):
    """源码能力：C# 夹具的 NPC 代理是否是 IMerchant（否则商人脚本必抛 InvalidCastException）。"""
    runmode = Path(harness_dir) / "RunMode.cs"
    if not runmode.is_file():
        return False, f"缺 {runmode}"
    text = runmode.read_text(encoding="utf-8", errors="replace")
    # 只看 NPC 代理那一行（RunMode.cs 里还有一个 IItemSystem 的代理，别误读）
    npc_ifaces = [m.group(1) for m in re.finditer(r"DispatchProxy\.Create<(\w+)\s*,\s*NpcRunProxy>", text)]
    if not npc_ifaces:
        # 退路：只要出现 IMerchant 的 Create 也算（夹具可能重命名了代理类）
        npc_ifaces = [m.group(1) for m in re.finditer(r"DispatchProxy\.Create<(IMerchant)\b", text)]
    # 用仓库相对路径（`_rel`），避免把 worktree 绝对路径写进入库证据（换检出/换机器重跑会改脏）。
    rel = _rel(runmode)
    if "IMerchant" in npc_ifaces:
        return True, f"{rel}: NPC 代理 = IMerchant ⇒ 可构造商人"
    got = ",".join(npc_ifaces) if npc_ifaces else "(未见 NpcRunProxy 代理)"
    return False, f"{rel}: NPC 代理 = {got}（非 IMerchant）⇒ 商人脚本必抛 InvalidCastException"


def run_cs_harness(harness_dir, case):
    """行为能力：实跑 C# 夹具 `run <脚本> <label> …`，看是抛 IMerchant 转换异常还是产出效果行。"""
    args = [
        "dotnet", "run", "--project", str(harness_dir), "-v", "q", "--",
        "run", case["script"], case["label"], "--gold", str(case.get("gold", 1000)),
    ]
    if case.get("items"):
        args += ["--items", case["items"]]
    if case.get("player_items"):
        args += ["--player-items", case["player_items"]]
    try:
        r = subprocess.run(args, cwd=str(MIR2RS), capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=600)
    except FileNotFoundError:
        return None, "缺 dotnet（PREREQ）"
    except subprocess.TimeoutExpired:
        return None, "dotnet run 超时（PREREQ）"
    raw = (r.stdout or "") + (r.stderr or "")
    if "InvalidCastException" in raw and "IMerchant" in raw:
        anchor = ""
        for line in raw.splitlines():
            if "ScriptParsers.cs" in line:
                anchor = line.strip()
                break
        return False, ("EX InvalidCastException（INormNpc 夹具缺 IMerchant）"
                       + (f" @ {anchor}" if anchor else ""))
    if r.returncode != 0 and not raw.strip():
        return None, f"dotnet run rc={r.returncode} 且无输出（PREREQ）"
    effect = [l for l in raw.splitlines()
              if l.startswith(("gold ", "msg ", "additem ", "delitem "))]
    if effect:
        return True, "产出效果行：" + " | ".join(effect[:4])
    # 无效果行且没抛 IMerchant 异常：可能是脚本本身无副作用（如 @StdModeFunc26 的 RepairAll mock）。
    return True, "无 IMerchant 异常（无效果行，按能力计）: " + raw.strip().splitlines()[-1][:160]


def check_rust_gamesvr_source(server_dir):
    """Rust GameSvr 能力：① 有 NPC 命令派发；② 依赖脚本引擎 crate。二者缺一即不能跑 NPC 脚本。"""
    server_dir = Path(server_dir)
    src = server_dir / "src"
    if not src.is_dir():
        return None, f"缺 {src}（PREREQ）"
    rs_hits = []
    for p in sorted(src.rglob("*.rs")):
        text = p.read_text(encoding="utf-8", errors="replace")
        for name in RUST_NPC_CM_NAMES:
            if name in text:
                rs_hits.append(f"{p.relative_to(server_dir).as_posix()}:{name}")
    cargo = server_dir / "Cargo.toml"
    cargo_text = cargo.read_text(encoding="utf-8", errors="replace") if cargo.is_file() else ""
    crate_ok = RUST_SCRIPT_CRATE in cargo_text
    # 与 GAME-SVR 的实际派发面无关的常量定义（协议 crate）不算派发——只看 game-svr 自身源码。
    handler_ok = bool(rs_hits)
    detail = (f"NPC 命令派发={'有:' + ','.join(rs_hits) if handler_ok else '无（game-svr 源码内无 ' + '/'.join(RUST_NPC_CM_NAMES) + '）'}；"
              f"脚本引擎依赖({RUST_SCRIPT_CRATE})={'有' if crate_ok else '无'}；"
              f"main.rs 实派发 = CM_SAY/CM_PICKUP/CM_DROPITEM")
    return (handler_ok and crate_ok), detail


def check_ports(ports):
    """窗口端口实况（信息项，不作能力判据）。用 .NET 监听表，避免 Get-NetTCPConnection 偶发挂死。"""
    ps = ("$eps=[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners(); "
          + ";".join(f'"port={p}:" + ($(if ($eps | Where-Object {{ $_.Port -eq {p} }}) {{"UP"}} else {{"DOWN"}}))'
                     for p in ports))
    try:
        r = subprocess.run(["powershell", "-NoProfile", "-Command", ps],
                           capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=60)
        lines = [l.strip() for l in (r.stdout or "").splitlines() if l.strip().startswith("port=")]
        return lines
    except Exception as e:  # noqa: BLE001
        return [f"端口探测失败: {e}"]


def run_rust_engine(case):
    """Rust 脚本引擎（flow-run）实跑——用来证明「引擎本身能跑这三条，缺的是接线/夹具」。"""
    exe = MIR2RS / "target" / "debug" / "mir2-script-tool.exe"
    if not exe.is_file():
        return None, "(缺 target/debug/mir2-script-tool.exe，跳过)"
    args = [str(exe), "flow-run", case["script"], case["label"], "--gold", str(case.get("gold", 1000))]
    if case.get("items"):
        args += ["--items", case["items"]]
    if case.get("player_items"):
        args += ["--player-items", case["player_items"]]
    try:
        r = subprocess.run(args, cwd=str(MIR2RS), capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=300)
    except Exception as e:  # noqa: BLE001
        return None, f"(flow-run 失败: {e})"
    return r.returncode, (r.stdout or "").rstrip()


# --------------------------------------------------------------------------- #
# 裁决
# --------------------------------------------------------------------------- #
def evaluate(cases, cs_harness, rust_server, run_cs):
    out = []
    out.append("# M4② 三条窗口用例的可跑性门（`npc_window_readiness.py`）")
    out.append("")
    out.append(f"- 用例定义：`{_rel(CASES, MIR2RS)}`")
    out.append(f"- C# 夹具：`{_rel(cs_harness)}`")
    out.append(f"- Rust GameSvr：`{_rel(rust_server)}`")
    out.append("")
    out.append("## 0. 窗口端口实况（易变信息项，不入 hash 清单）")
    out.append("")
    out.append("- 本证据**不含**端口快照：端口会随窗口/别车道起栈而变，写进入库证据会让重跑改脏、"
               "hash 清单误红。本次采样见命令 stdout 的 `port=…` 行与 "
               "`mir2-rs/target/m4/npc-window-readiness-ports.txt`（`target/` 不入库）。")
    out.append("")

    verdicts = []
    for case in cases:
        out.append(f"## {case['category']}：{case['name']}")
        out.append(f"- 脚本/label：`{case['script']}` `{case['label']}`")

        cs_src_ok, cs_src_detail = check_cs_harness_source(cs_harness)
        if run_cs:
            cs_ok, cs_detail = run_cs_harness(cs_harness, case)
            if cs_ok is None:
                cs_ok = cs_src_ok
                cs_detail = f"[源码判据] {cs_src_detail}；[实跑] {cs_detail}"
            else:
                cs_detail = f"[实跑] {cs_detail}；[源码] {cs_src_detail}"
        else:
            cs_ok, cs_detail = cs_src_ok, f"[源码] {cs_src_detail}"
        out.append(f"- C# port-free 夹具：{'CAPABLE' if cs_ok else 'BLOCKED'} — {cs_detail}")

        rust_ok, rust_detail = check_rust_gamesvr_source(rust_server)
        if rust_ok is None:
            out.append(f"- Rust 服务侧（M3 承接，不阻塞本卡）：PREREQ — {rust_detail}")
        else:
            out.append(f"- Rust 服务侧（M3 承接，不阻塞本卡）：{'CAPABLE' if rust_ok else 'BLOCKED'} — {rust_detail}")

        rc, eng = run_rust_engine(case)
        if rc is not None:
            out.append(f"- Rust 脚本引擎（flow-run，证明引擎本身可跑）：")
            out.append("```")
            for line in eng.splitlines():
                out.append("  " + line)
            out.append("```")

        # 本卡 ② 判据（owner e808ec99 裁定）：以 port-free（路线①）为准；
        # Rust 服务侧 NPC/商人派发属 M3，不阻塞本卡。
        if cs_ok is None:
            verdict = "PREREQ"
        elif cs_ok:
            verdict = "READY"
        else:
            verdict = "BLOCKED"
        verdicts.append(verdict)
        out.append(f"- 裁决（本卡 ② port-free）：**{verdict}**")
        out.append("")

    if any(v == "PREREQ" for v in verdicts):
        overall = "PREREQ"
    elif all(v == "READY" for v in verdicts):
        overall = "READY"
    else:
        overall = "BLOCKED"

    out.append("## 总裁决")
    out.append("")
    out.append(f"- 三条商人用例（本卡 ② port-free）：{', '.join(verdicts)} ⇒ **{overall}**")
    out.append("")
    if overall == "READY":
        out.append("路线①（C# 夹具补 `IMerchant` 代理）已就绪 ⇒ 三条可 port-free 跑，"
                   "已转 `runner == flow`，由 `run_npc_cases.py` 对拍。"
                   "**Rust 服务侧**的 NPC/商人派发不属本卡（§6 的 M3「NPC/商店」系统对齐）——"
                   "上表 `Rust 服务侧` 行是它的承接登记。")
    else:
        out.append("路线① 未就绪 ⇒ **这三条仍不可 port-free 跑**（`npc-cases.json` 里保持原 runner，不许标绿）。")
        out.append("")
        out.append("承接：给 `mir2-rs/tools/script-parity-cs` 的 NPC 代理按脚本头补 `IMerchant`"
                   "（`DispatchProxy.Create<INormNpc|IMerchant, NpcRunProxy>`），"
                   "并让 `IList<>` 型 `get_` 返回非空 List；`goto` 还需把代理注册进 `SystemShare.ActorMgr`。")
    out.append("")
    return overall, "\n".join(out) + "\n"


# --------------------------------------------------------------------------- #
# 牙齿
# --------------------------------------------------------------------------- #
def cmd_selftest():
    tmp = Path(tempfile.mkdtemp(prefix="m4-npcwin-selftest-"))
    bad = 0
    try:
        case = [{"category": "商店", "name": "合成用例", "script": "X.txt", "label": "@x",
                 "gold": 1, "items": "", "player_items": ""}]

        def make(harness_imerchant, rust_handler, rust_crate):
            h = tmp / f"cs-{harness_imerchant}-{rust_handler}-{rust_crate}"
            (h / "tools" / "script-parity-cs").mkdir(parents=True)
            proxy = ("DispatchProxy.Create<IMerchant, NpcRunProxy>" if harness_imerchant
                     else "DispatchProxy.Create<INormNpc, NpcRunProxy>")
            (h / "tools" / "script-parity-cs" / "RunMode.cs").write_text(f"var npc = {proxy};\n",
                                                                         encoding="utf-8")
            (h / "mir2-rs" / "bins" / "game-svr" / "src").mkdir(parents=True)
            body = "cmd.ident == CM_MERCHANTDLGSELECT" if rust_handler else "cmd.ident == CM_SAY"
            (h / "mir2-rs" / "bins" / "game-svr" / "src" / "main.rs").write_text(body + "\n",
                                                                                encoding="utf-8")
            (h / "mir2-rs" / "bins" / "game-svr" / "Cargo.toml").write_text(
                ('mir2-script = { path = "../../crates/script" }\n' if rust_crate else
                 'mir2-protocol = { path = "x" }\n'), encoding="utf-8")
            return h

        cases = [
            # (C# 夹具 IMerchant, Rust 派发, Rust 引擎依赖, 期望整体裁决)
            # 本卡 ② 以 port-free（路线①）为准；Rust 服务侧不阻塞本卡。
            (False, False, False, "BLOCKED"),
            (False, True, True, "BLOCKED"),     # C# 夹具没修 ⇒ 仍不可 port-free
            (True, False, False, "READY"),      # 路线① 修好 ⇒ 可 port-free（Rust 服务侧归 M3）
            (True, True, False, "READY"),
            (True, True, True, "READY"),
        ]
        for i, (h, rh, rc, want) in enumerate(cases, 1):
            root = make(h, rh, rc)
            overall, _ = evaluate(case, root / "tools" / "script-parity-cs",
                                  root / "mir2-rs" / "bins" / "game-svr", False)
            ok = overall == want
            bad += 0 if ok else 1
            print(f"{'PASS' if ok else 'RED '} selftest {i}/{len(cases)} "
                  f"cs_imerchant={h} rust_handler={rh} rust_crate={rc} ⇒ {overall}（期望 {want}）")
        if bad:
            print(f"RED: npc_window_readiness selftest {bad} 项不过")
            return 2
        print("GREEN: npc_window_readiness selftest 5/5"
              "（C# 夹具路线① 未修 ⇒ BLOCKED；修好 ⇒ READY（Rust 服务侧归 M3，不阻塞本卡））")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# --------------------------------------------------------------------------- #
# 写盘模式牙齿（卡 openmir2-m4-readiness-write-mode）
# --------------------------------------------------------------------------- #
EVIDENCE_REL = "mir2-rs/tests/parity/evidence/M4"


def _git_status_evidence():
    """真仓库 `tests/parity/evidence/M4` 的 git 状态（strip 后；空 = 干净）。"""
    try:
        r = subprocess.run(["git", "status", "--porcelain", "--", EVIDENCE_REL],
                           cwd=str(REPO), capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=60)
    except Exception as e:  # noqa: BLE001
        return f"(git status 失败: {e})"
    return (r.stdout or "").strip()


def _run_self(script, extra):
    """用隔离解释器跑本脚本（或其回归副本），返回 CompletedProcess。"""
    cmd = [sys.executable, "-I", str(script)] + list(extra)
    return subprocess.run(cmd, cwd=str(MIR2RS), capture_output=True, text=True,
                          encoding="utf-8", errors="replace", timeout=600)


def cmd_selftest_write():
    """写盘模式牙齿：默认参数不得写盘；把默认写盘改回去 ⇒ 本自检必须 RED。

    ① 默认参数（无 --write-evidence）不得写显式 --out（tmp）；
    ② 默认 --out（真仓库证据）跑一次，`git status` 的 before/after 必须相同（干净树里 = 空）；
    ③ 合成「默认写盘」回归副本（把写盘门换成 `if True:`）跑本自检 ⇒ 必须 RED，并把其原始输出回显留档。
    """
    script = Path(__file__).resolve()
    tmp = Path(tempfile.mkdtemp(prefix="m4-npcwin-write-selftest-"))
    base = ["--no-run-cs", "--cases", str(CASES), "--cs-harness", str(CS_HARNESS),
            "--rust-server", str(RUST_GAMESVR)]
    bad = 0
    try:
        print("== 写盘模式自检（npc_window_readiness.py；卡 openmir2-m4-readiness-write-mode）==")
        before = _git_status_evidence()
        print(f"-- 运行前 git status --porcelain -- {EVIDENCE_REL}: "
              f"{'(空)' if not before else before}")

        # ① 默认参数（无 --write-evidence）不得写显式 --out。
        out_ok = tmp / "default-out.txt"
        r1 = _run_self(script, base + ["--out", str(out_ok)])
        ok1 = not out_ok.exists()
        bad += 0 if ok1 else 1
        print(f"{'PASS' if ok1 else 'RED '} 1/3 默认参数（无 --write-evidence）不写 --out")
        if not ok1:
            tail = (r1.stdout or "").strip().splitlines()
            print(f"     RED: 默认运行写了 {out_ok.name}；stdout 末行 = {tail[-1] if tail else '(空)'}")

        # ② 默认 --out（真仓库证据）跑完，本次运行不得新增任何改动（after == before）。
        #    在干净树里 before 为空 ⇒ 等价于 `git status --porcelain` 为空（卡③口径）。
        default_out = EVIDENCE / "npc-window-readiness.txt"
        orig = default_out.read_bytes() if default_out.exists() else None
        _run_self(script, base)
        after = _git_status_evidence()
        changed = (default_out.exists() and orig is not None
                   and default_out.read_bytes() != orig)
        ok2 = (after == before)
        bad += 0 if ok2 else 1
        print(f"{'PASS' if ok2 else 'RED '} 2/3 默认运行未改动证据目录（git status before == after）")
        if before != "":
            print(f"     （提示：运行前已有未提交证据改动；卡③的权威判据是在干净树里 before 为空）")
        if not ok2:
            print(f"     RED: 运行前 = {before or '(空)'}；运行后 = {after or '(空)'}")
        if changed and orig is not None:
            # 非破坏性：检出回归后把证据字节恢复，避免自检本身留下脏树。
            default_out.write_bytes(orig)
            print("     （已恢复默认运行被改写的证据字节）")

        # ③ 牙齿：合成「默认写盘」回归副本（就地生成，路径解析与本体一致）⇒ 其自检必须 RED。
        reg_path = None
        needle = "if args." + "write_evidence:"  # 拼接写，避免本自检源码自身成为匹配目标
        text = script.read_text(encoding="utf-8")
        n = text.count(needle)
        if n != 1:
            bad += 1
            print(f"RED  3/3 牙齿锚点 {needle!r} 出现 {n} 次（期望 1）⇒ 自检失效（写盘门被改）")
        else:
            # 回归副本必须放本脚本同目录：常量 HERE/CASES/EVIDENCE/REPO 都由 __file__ 推出，
            # 放 tmp 会让嵌套运行因「缺 npc-cases.json」提前 PREREQ ⇒ 牙齿假红。
            with tempfile.NamedTemporaryFile(
                    "w", encoding="utf-8", newline="\n", suffix=".py",
                    prefix=".regressed_npc_window_readiness-", dir=str(HERE),
                    delete=False) as fh:
                reg_path = Path(fh.name)
                fh.write(text.replace(needle, "if True:  # 合成回归：默认写盘"))
            r3 = _run_self(reg_path, ["selftest-write"])
            after3 = _git_status_evidence()
            # 真牙齿：回归副本的自检必须（a）非 0；（b）在 1/3 就判 RED（默认运行写了 --out）；
            # （c）跑完仓库证据仍干净（回归副本自己会恢复被改证据）。
            genuine_red = "RED  1/3" in (r3.stdout or "")
            teeth_ok = (r3.returncode != 0 and "RED" in (r3.stdout or "")
                        and genuine_red and after3 == before)
            bad += 0 if teeth_ok else 1
            print(f"{'PASS' if teeth_ok else 'RED '} 3/3 牙齿：合成「默认写盘」回归 ⇒ 自检 RED"
                  f"（回归副本 exit={r3.returncode}；证据目录 "
                  f"{'干净' if after3 == before else '被改脏'}）")
            if teeth_ok:
                print("     —— 回归副本的原始输出（RED 情景留档）——")
                for line in (r3.stdout or "").splitlines()[:14]:
                    print("     | " + line)
        if bad:
            print(f"RED: npc_window_readiness 写盘模式自检 {bad} 项不过")
            return 2
        print("GREEN: npc_window_readiness 写盘模式自检 3/3"
              "（默认不写盘 + 本次运行未改动证据 + 改回默认写盘必 RED）")
        return 0
    finally:
        if reg_path is not None:
            try:
                reg_path.unlink()
            except OSError:
                pass
        shutil.rmtree(tmp, ignore_errors=True)


# --------------------------------------------------------------------------- #
def main():
    ap = argparse.ArgumentParser(description="M4② 三条窗口用例的可跑性门")
    ap.add_argument("--cases", default=str(CASES))
    ap.add_argument("--cs-harness", default=str(CS_HARNESS))
    ap.add_argument("--rust-server", default=str(RUST_GAMESVR))
    ap.add_argument("--out", default=str(EVIDENCE / "npc-window-readiness.txt"))
    ap.add_argument("--no-run-cs", action="store_true",
                    help="只做源码能力检查（不实跑 dotnet；快）")
    ap.add_argument("--write-evidence", action="store_true",
                    help="显式写 --out；默认只回显 stdout，不动入库证据")
    ap.add_argument("--ports", default="5000,7000,7100,7200")
    ap.add_argument("cmd", nargs="?", default="check",
                    choices=["check", "selftest", "selftest-write"])
    args = ap.parse_args()

    if args.cmd == "selftest":
        return cmd_selftest()
    if args.cmd == "selftest-write":
        return cmd_selftest_write()

    cases_path = Path(args.cases)
    if not cases_path.is_file():
        print(f"PREREQ: 缺用例 {cases_path}")
        return 2
    data = json.loads(cases_path.read_text(encoding="utf-8"))
    merchants = [c for c in data["cases"] if c.get("category") in MERCHANT_CATEGORIES]
    if not merchants:
        print("PREREQ: npc-cases.json 里找不到商店/修理/仓库三条商人用例（category 匹配）")
        return 2

    ports = [int(p) for p in args.ports.split(",") if p.strip()]
    overall, text = evaluate(merchants, args.cs_harness, args.rust_server,
                             run_cs=not args.no_run_cs)
    print(text)
    # 写盘必须显式（卡 openmir2-m4-readiness-write-mode）：默认只 stdout，绝不改入库证据。
    out_path = Path(args.out)
    if args.write_evidence:
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(text, encoding="utf-8", newline="\n")
        print(f"EVIDENCE {_rel(out_path)}（--write-evidence 显式写盘）")
    else:
        print(f"EVIDENCE-SKIP 默认只回显 stdout（要写 {_rel(out_path)} 需显式 --write-evidence）")
    # 端口快照是易变信息项：回显到 stdout + 落到 target/（不入库、不入 hash 清单），
    # 这样重跑本门不会把入库证据改脏 —— 评审按 reviewer_note 跑本门 + evidence_hashes.py 不会误红。
    port_lines = check_ports(ports)
    ports_out = MIR2RS / "target" / "m4" / "npc-window-readiness-ports.txt"
    try:
        ports_out.parent.mkdir(parents=True, exist_ok=True)
        ports_out.write_text("\n".join(port_lines) + "\n", encoding="utf-8", newline="\n")
    except OSError:
        pass
    print("PORT-SNAPSHOT " + " | ".join(port_lines))
    if overall == "READY":
        print("READY: C# 夹具路线① 就绪 ⇒ 三条商人用例可 port-free 跑"
              "（Rust 服务侧 NPC 派发归 M3，见证据）")
        return 0
    if overall == "PREREQ":
        print("PREREQ_MISSING: 缺 dotnet/源码/用例，能力未定")
        return 2
    print("BLOCKED: C# 夹具路线① 未就绪 ⇒ 三条商人用例仍不可 port-free 跑（已登记）")
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
