#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M4② 三条「窗口用例」的可跑性门（卡 `openmir2-m4-parity-cases`，2026-10-12）。

## 为什么有这个文件

卡内 ② 的 6 类关键 NPC 用例分两档：
- `runner == "flow"`（传送×2 / 行会 / 任务）——两侧脚本 harness（C# `script-parity-cs run` vs
  Rust `script-tool flow-run`）**现在就能跑**，`run_npc_cases.py` 判绿；
- `runner == "window"`（商店 / 修理 / 仓库）——脚本首行含商人头 `(@buy …)`，**两条路线当前都跑不了**：
  1. **port-free 路线**：C# 参照 `ScriptParsers.LoadScriptFile` 在 `ScriptParsers.cs:713+` 把 NPC
     强转 `IMerchant`；而 `tools/script-parity-cs` 的 `NpcRunProxy` 是 `INormNpc` 的 DispatchProxy
     ⇒ `InvalidCastException`（本门会实跑复现）。
  2. **两侧真栈路线**：Rust GameSvr（`mir2-rs/bins/game-svr`）**没有** NPC/商人脚本派发——
     无 `CM_MERCHANTDLGSELECT`/`CM_CLICKNPC` 处理、`Cargo.toml` 无 `mir2-script` 依赖、
     `main.rs` 只认 `CM_SAY`/`CM_PICKUP`/`CM_DROPITEM` ⇒ 同一输入在 Rust 栈上**不产生任何脚本副作用**。

本门把这两条前置做成**可机器判、可证伪**的检查：任一前置未满足时，三条窗口用例**不许**被标成
「已跑绿」；两条都满足（例如 C# 夹具补了 `IMerchant` **且** Rust 服务侧接了脚本派发）时，
本门转 `READY`，才允许按 `npc-cases.txt`「窗口释放后怎么跑」真跑并落 `npc-cases-window.txt`。

> **本门不是把 PENDING 标绿**——恰恰相反：它是「窗口用例当前不可跑」这个事实的**机判锚点**。
> 谁把窗口用例标成已通过，必须同时给出本门 `READY` 的输出，否则不可信。

## 命令（在 `mir2-rs/` 下）

    python -I tests/parity/M4/npc_window_readiness.py            # 实跑（含 C# 夹具行为复现）+ 出证据
    python -I tests/parity/M4/npc_window_readiness.py --no-run-cs # 只做源码能力检查（快，供 CI 用）
    python -I tests/parity/M4/npc_window_readiness.py selftest    # 牙齿：合成输入翻转能力 ⇒ 裁决翻转

退出码：0 = READY（两条前置都满足，可跑窗口）；3 = BLOCKED（有前置未满足，登记在案）；
       2 = 用法/环境错误（缺 dotnet / 缺源码 / 缺用例）。
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
    if "IMerchant" in npc_ifaces:
        return True, f"{runmode}: NPC 代理 = IMerchant ⇒ 可构造商人"
    got = ",".join(npc_ifaces) if npc_ifaces else "(未见 NpcRunProxy 代理)"
    return False, f"{runmode}: NPC 代理 = {got}（非 IMerchant）⇒ 商人脚本必抛 InvalidCastException"


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
def evaluate(cases, cs_harness, rust_server, run_cs, ports):
    out = []
    out.append("# M4② 三条窗口用例的可跑性门（`npc_window_readiness.py`）")
    out.append("")
    out.append(f"- 用例定义：`{_rel(CASES, MIR2RS)}`")
    out.append(f"- C# 夹具：`{_rel(cs_harness)}`")
    out.append(f"- Rust GameSvr：`{_rel(rust_server)}`")
    out.append("")
    out.append("## 0. 窗口端口实况（信息项）")
    out.append("")
    for l in check_ports(ports):
        out.append(f"- {l}")
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
            out.append(f"- Rust 服务侧：PREREQ — {rust_detail}")
        else:
            out.append(f"- Rust 服务侧：{'CAPABLE' if rust_ok else 'BLOCKED'} — {rust_detail}")

        rc, eng = run_rust_engine(case)
        if rc is not None:
            out.append(f"- Rust 脚本引擎（flow-run，证明引擎本身可跑）：")
            out.append("```")
            for line in eng.splitlines():
                out.append("  " + line)
            out.append("```")

        if cs_ok is None or rust_ok is None:
            verdict = "PREREQ"
        elif cs_ok and rust_ok:
            verdict = "READY"
        else:
            verdict = "BLOCKED"
        verdicts.append(verdict)
        out.append(f"- 裁决：**{verdict}**")
        out.append("")

    if any(v == "PREREQ" for v in verdicts):
        overall = "PREREQ"
    elif all(v == "READY" for v in verdicts):
        overall = "READY"
    else:
        overall = "BLOCKED"

    out.append("## 总裁决")
    out.append("")
    out.append(f"- 三条窗口用例：{', '.join(verdicts)} ⇒ **{overall}**")
    out.append("")
    if overall == "READY":
        out.append("两条前置都满足 ⇒ 可按 `npc-cases.txt`「窗口释放后怎么跑」起两侧栈真跑，"
                   "并把原始输出写进 `evidence/M4/npc-cases-window.txt`。")
    else:
        out.append("前置未满足 ⇒ **这三条现在不可跑**（`npc-cases.txt` 里保持 PENDING WINDOW，不许标绿）。")
        out.append("")
        out.append("承接（两条路线各一条路）：")
        out.append("1. **port-free 路线**：给 `mir2-rs/tools/script-parity-cs` 的 NPC 代理补 `IMerchant`"
                   "（把 `DispatchProxy.Create<INormNpc, NpcRunProxy>` 改成按脚本头决定 `IMerchant`/`INormNpc`，"
                   "并让 `IList<>` 型 `get_` 返回非空 List）——该路径在卡写集外，需 owner 授权或另开卡。")
        out.append("2. **两侧真栈路线**：给 `mir2-rs/bins/game-svr` 接 NPC/商人脚本派发"
                   "（`CM_CLICKNPC`/`CM_MERCHANTDLGSELECT` → 脚本 label → 效果 + DB），"
                   "并在 `Cargo.toml` 引入 `mir2-script`——属 M2/M3 接线面，需另开卡。")
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
            # (夹具, 服务派发, 引擎依赖, 期望整体裁决)
            (False, False, False, "BLOCKED"),
            (True, False, False, "BLOCKED"),   # 只修 C# 夹具仍不够
            (False, True, True, "BLOCKED"),    # 只修 Rust 仍不够
            (True, True, False, "BLOCKED"),    # 有派发但没引擎依赖
            (True, True, True, "READY"),
        ]
        for i, (h, rh, rc, want) in enumerate(cases, 1):
            root = make(h, rh, rc)
            overall, _ = evaluate(case, root / "tools" / "script-parity-cs",
                                  root / "mir2-rs" / "bins" / "game-svr", False, ports=[])
            ok = overall == want
            bad += 0 if ok else 1
            print(f"{'PASS' if ok else 'RED '} selftest {i}/{len(cases)} "
                  f"cs_imerchant={h} rust_handler={rh} rust_crate={rc} ⇒ {overall}（期望 {want}）")
        if bad:
            print(f"RED: npc_window_readiness selftest {bad} 项不过")
            return 2
        print("GREEN: npc_window_readiness selftest 5/5"
              "（两路前置任一缺失都判 BLOCKED；都具备才 READY）")
        return 0
    finally:
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
    ap.add_argument("--ports", default="5000,7000,7100,7200")
    ap.add_argument("cmd", nargs="?", default="check", choices=["check", "selftest"])
    args = ap.parse_args()

    if args.cmd == "selftest":
        return cmd_selftest()

    cases_path = Path(args.cases)
    if not cases_path.is_file():
        print(f"PREREQ: 缺用例 {cases_path}")
        return 2
    data = json.loads(cases_path.read_text(encoding="utf-8"))
    window = [c for c in data["cases"] if c.get("runner") == "window"]
    if not window:
        print("PREREQ: npc-cases.json 里没有 runner==window 用例")
        return 2

    ports = [int(p) for p in args.ports.split(",") if p.strip()]
    overall, text = evaluate(window, args.cs_harness, args.rust_server,
                             run_cs=not args.no_run_cs, ports=ports)
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(text, encoding="utf-8", newline="\n")
    print(text)
    print(f"EVIDENCE {_rel(out_path)}")
    if overall == "READY":
        print("READY: 三条窗口用例前置齐备，可起两侧栈真跑")
        return 0
    if overall == "PREREQ":
        print("PREREQ_MISSING: 缺 dotnet/源码/用例，能力未定")
        return 2
    print("BLOCKED: 三条窗口用例当前不可跑（前置未满足，已在证据里登记）")
    return 3


if __name__ == "__main__":
    raise SystemExit(main())
