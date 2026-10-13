#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""window_handoff.py —— 卡 `openmir2-client-e2e-enter-gaps` ③ 收口的
**窗口就绪聚合门 + 一键两侧 capture 计划**（只读、零起栈、零端口）。

背景（root 2026-10-14 00:4x ③；本片 = 片⑰）：
  ③「同屏没怪」的离线判决已经成立（`70-aoi-visibility-verdict.txt`：`(291,666)` 在 map 0
  所有 MonGen 生成框之外 ⇒ `vis=0` 正确），但**唯一未闭环的 R 级判据 = 两侧同框真机对照**，
  只能由 root 在独占窗口里跑。窗口是一次性资源，开窗前把「工具/构建源/落点/命令」四件事
  做成**一条机判的 GO/NO-GO**，避免再出现「开了窗口才发现构建源过旧、判据 PREREQ_RED」的烧窗。

本工具**不重新实现任何判据**，只做四件事：
  G1 **聚合 Rust 侧窗口前置门**：调片⑮ `check_rust_window_preflight.py`（R1..R5），解析 `RESULT:`。
     其中 R1/R2 直接点住「窗口脚本 `client-e2e-window.sh` 的 `repo=` 指向的检出是否含片⑪」——
     这是 2026-10-14 实测到的真缺陷（共享主检出停在 `ad0592b19`）。
  G2 **聚合判据工具自检**：证据目录 6 件判据脚本的 `--selftest` 必须全 `SELFTEST OK`（缺一件 ⇒ PREREQ）。
  G3（可选）**聚合 C# 侧抓包前置门**：调片⑭ `check_csharp_capture_preflight.py --shadow-config ...`。
  J1（`--judge`）**聚合 ③b/③a 两趟窗口产物的五条 AOI 判据**（只消费既有 capture/log，不起栈）：
     ③a 第二趟必须用**独立目录**（默认 `<work-dir>-a`）——`start_rust_stack.ps1` 每次起栈都会删掉
     旧 `game-svr.out.log`（`Start-One` 的 `Remove-Item $out, $err`），共用一个目录会把 ③b 读数覆盖掉。
  另加一条**只读信息行**：`check_mongen_dense_points.py --rank` 的窗口内期望出生怪数排行（选点用）。

`--plan` 把 ③a/③b 两侧对照的**全部命令**（Rust 起栈/代理/客户端/解码 + C# 侧 + 三条判据 + 收工）
按 runbook `82-two-side-window-runbook.md` 的口径生成成可粘贴文本；当 G1 判 RED 时，额外打印
「改用专用 worktree 从 origin/master 构建」的修复块（不动共享主检出）。

用法：
  # 就绪门（开窗前跑；rc 0=GO / 1=NO-GO / 2=PREREQ）
  python -I -B window_handoff.py --repo <构建源检出> [--ref HEAD] [--mirserver E:/MirServer]
  python -I -B window_handoff.py --repo <检出> --shadow-config <影子 RunGate/config.conf> --with-csharp
  # 计划（可单独跑；--plan-only 不跑判据）
  python -I -B window_handoff.py --plan --site 287,620 --site-none 291,666
  python -I -B window_handoff.py --plan --best-site        # ③b 由 MonGen 排行自动选最密点
  python -I -B window_handoff.py --plan-only --json
  # 判据聚合（窗口跑完后；消费两趟产物，不起栈）
  python -I -B window_handoff.py --judge --work-dir E:/tmp/window/cliente2e
  python -I -B window_handoff.py --judge --work-dir <b> --pass-a-dir <a>
  python -I -B window_handoff.py --selftest

退出码：`0` = GREEN/GO、`1` = RED/NO-GO（判据或工具不符）、`2` = PREREQ_RED（缺输入/解析不到）。
`--selftest` 不依赖外部件：合成 `RESULT:`/`SELFTEST` 文本 + 临时目录里的假判据脚本，端到端验证
聚合（全绿 / 构建源 RED / 某工具自检 FAILED / 缺工具 PREREQ 四种）与计划文本的必含/必不含项。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))

RUST_PREFLIGHT = "check_rust_window_preflight.py"
CSHARP_PREFLIGHT = "check_csharp_capture_preflight.py"
TOOL_SELFTESTS = (
    "check_mongen_dense_points.py",
    "check_aoi_visibility_diag.py",
    "aoi_visibility_pair.py",
    "check_client_s2c_frames.py",
    RUST_PREFLIGHT,
    CSHARP_PREFLIGHT,
)

# `--judge` 消费的三件判据脚本（③b/③a 两趟各跑服务端 / 客户端 / 两侧）。
AOI_DIAG = "check_aoi_visibility_diag.py"
CLIENT_FRAMES = "check_client_s2c_frames.py"
PAIR_TOOL = "aoi_visibility_pair.py"

_RESULT_RE = re.compile(r"^RESULT:\s*(GREEN|RED|PREREQ_RED)\b", re.MULTILINE)
_SELFTEST_RE = re.compile(r"^SELFTEST\s+(OK|GREEN|FAILED|RED)\b", re.MULTILINE)
# `BEST (605,578) E=12.681 view_range=12 box=(605,578) range=30 count=10 name=半兽人`
_BEST_RE = re.compile(
    r"^BEST\s+\((?P<x>-?\d+),(?P<y>-?\d+)\)\s+E=(?P<e>[0-9]+(?:\.[0-9]+)?)\b", re.MULTILINE
)
_PASS_STATES = ("OK", "GREEN")
_FAIL_STATES = ("FAILED", "RED")


# ---------------------------------------------------------------- 解析（纯函数，selftest 直接打）

def parse_result(text: str) -> str | None:
    """从工具 stdout 抽最后一条 `RESULT: GREEN|RED|PREREQ_RED`。"""
    m = None
    for m in _RESULT_RE.finditer(text or ""):
        pass
    return m.group(1) if m else None


def parse_selftest(text: str) -> str | None:
    """从工具 stdout 抽 `SELFTEST OK|FAILED`。"""
    m = _SELFTEST_RE.search(text or "")
    return m.group(1) if m else None


def parse_best_point(text: str) -> dict | None:
    """从 `check_mongen_dense_points.py --best-point` 的 stdout 抽 `BEST (x,y) E=<e>`。

    返回 `{"x": int, "y": int, "e": str}`；无该行 ⇒ `None`（调用方 fail-soft）。
    """
    m = _BEST_RE.search(text or "")
    if not m:
        return None
    return {"x": int(m.group("x")), "y": int(m.group("y")), "e": m.group("e")}


def resolve_best_site(
    check_dir: str, mirserver: str, map_name: str, view_range: int
) -> tuple[str | None, str]:
    """用片⑯ MonGen 排行给 ③b 选「窗口内期望出生怪数最大」的点（root ③①「换怪密点」）。

    只读聚合：调 `check_mongen_dense_points.py --best-point`，解析 `BEST (x,y) E=`。
    缺工具/缺 MonGen/解析不到 ⇒ `(None, 说明)`（fail-soft：计划回落到 `--site` 缺省并打印说明，
    绝不静默假装选过点）。
    """
    tool = os.path.join(check_dir, "check_mongen_dense_points.py")
    mongen = os.path.join(mirserver, "M2GameSvr", "Envir", "MonGen.txt")
    if not os.path.isfile(tool):
        return None, f"选点跳过：缺判据脚本 {tool}"
    if not os.path.isfile(mongen):
        return None, f"选点跳过：缺 {mongen}"
    rc, out = run_tool(tool, ["--mongen", mongen, "--map", map_name,
                              "--view-range", str(view_range), "--best-point"])
    best = parse_best_point(out)
    if rc != 0 or not best:
        return None, f"选点跳过：check_mongen_dense_points.py --best-point rc={rc}（解析不到 BEST）"
    site = f"{best['x']},{best['y']}"
    return site, (f"③b 最密点自动选取 BEST={site} E={best['e']} "
                  f"（check_mongen_dense_points.py --best-point，root ③①「换怪密点」）")


def run_tool(path: str, args: list[str], timeout: int = 180) -> tuple[int, str]:
    """`python -I -B <path> <args...>`（隔离模式，与仓内既有调用口径一致）。"""
    cmd = [sys.executable, "-I", "-B", path, *args]
    try:
        proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
    except (OSError, subprocess.SubprocessError) as exc:  # noqa: BLE001
        return 127, f"{type(exc).__name__}: {exc}"
    return proc.returncode, decode(proc.stdout)


def decode(raw: bytes | str) -> str:
    if isinstance(raw, str):
        return raw
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode("utf-8", "replace")


class Report:
    def __init__(self) -> None:
        self.problems: list[str] = []
        self.prereq: list[str] = []
        self.info: list[str] = []
        self.build_source_stale = False
        self.rust_ok = False

    def rc(self) -> int:
        if self.problems:
            return 1
        if self.prereq:
            return 2
        return 0


# ---------------------------------------------------------------- G1/G2/G3 聚合

def run_checks(
    check_dir: str,
    repo: str,
    ref: str,
    mirserver: str,
    shadow_config: str,
    with_csharp: bool,
    rank: int,
    view_range: int,
    map_name: str,
) -> Report:
    rep = Report()

    # ---- G2 判据工具自检（先跑：工具坏了就别信后面的读数） ----
    missing = [t for t in TOOL_SELFTESTS if not os.path.isfile(os.path.join(check_dir, t))]
    if missing:
        rep.prereq.append(f"G2 缺判据工具 {missing}（--check-dir={check_dir}）")
    for tool in TOOL_SELFTESTS:
        path = os.path.join(check_dir, tool)
        if not os.path.isfile(path):
            continue
        rc, out = run_tool(path, ["--selftest"], timeout=120)
        state = parse_selftest(out)
        if rc == 0 and state in _PASS_STATES:
            rep.info.append(f"OK   G2 {tool} --selftest")
        elif state in _FAIL_STATES or rc == 1:
            rep.problems.append(f"G2 {tool} --selftest FAILED（rc={rc}）")
        else:
            rep.prereq.append(
                f"G2 {tool} --selftest 判不了（rc={rc}，state={state}，末行={last_line(out)}）"
            )

    # ---- G1 Rust 侧窗口前置门 ----
    rp = os.path.join(check_dir, RUST_PREFLIGHT)
    if not repo:
        rep.prereq.append("G1 缺 --repo（Rust 构建源检出）")
    elif not os.path.isfile(rp):
        rep.prereq.append(f"G1 缺 {rp}")
    else:
        rc, out = run_tool(rp, ["--repo", repo, "--ref", ref, "--mirserver", mirserver], timeout=180)
        result = parse_result(out)
        if rc == 0 and result == "GREEN":
            rep.rust_ok = True
            rep.info.append("OK   G1 Rust 窗口前置门 GREEN（R1..R5）")
        elif result == "RED" or rc == 1:
            rep.build_source_stale = True
            rep.problems.append(
                f"G1 Rust 窗口前置门 RED（构建源/读数标记不符；窗口会 PREREQ_RED）"
                f"—— 见下方修复块"
            )
        else:
            rep.prereq.append(f"G1 Rust 窗口前置门 PREREQ_RED（rc={rc}，末行={last_line(out)}）")

    # ---- G3 C# 侧抓包前置门（可选） ----
    if with_csharp:
        cp = os.path.join(check_dir, CSHARP_PREFLIGHT)
        if not os.path.isfile(cp):
            rep.prereq.append(f"G3 缺 {cp}")
        else:
            args = ["--repo", repo or check_dir, "--mirserver", mirserver]
            if shadow_config:
                args += ["--shadow-config", shadow_config]
            rc, out = run_tool(cp, args, timeout=180)
            result = parse_result(out)
            if rc == 0 and result == "GREEN":
                rep.info.append("OK   G3 C# 抓包拓扑前置门 GREEN")
            elif result == "RED" or rc == 1:
                rep.problems.append("G3 C# 抓包拓扑前置门 RED（红灯先修，别带红灯起栈）")
            else:
                rep.prereq.append(f"G3 C# 前置门 PREREQ_RED（rc={rc}，末行={last_line(out)}）")

    # ---- 只读信息：MonGen 窗口内期望出生怪数排行（选点用） ----
    if rank > 0:
        dp = os.path.join(check_dir, "check_mongen_dense_points.py")
        mongen = os.path.join(mirserver, "M2GameSvr", "Envir", "MonGen.txt")
        if os.path.isfile(dp) and os.path.isfile(mongen):
            rc, out = run_tool(
                dp,
                ["--mongen", mongen, "--map", map_name, "--view-range", str(view_range), "--rank", str(rank)],
                timeout=120,
            )
            for line in out.splitlines():
                if line.startswith("RANK "):
                    rep.info.append("INFO " + line)
        else:
            rep.info.append("INFO 跳过 MonGen 排行（缺判据脚本或 MonGen.txt）")

    return rep


def last_line(text: str) -> str:
    lines = [ln for ln in (text or "").splitlines() if ln.strip()]
    return lines[-1] if lines else "<无输出>"


# ---------------------------------------------------------------- J1 窗口判据聚合

def pass_roots(work_dir: str, pass_a_dir: str = "") -> tuple[str, str, str, str]:
    """返回 (③b 根, ③b/rust, ③b/csharp, ③a 根)；③a 根缺省 = ③b 根 + `-a`。

    ③b 路径保持片⑰ 的 `<work-dir>/{rust,csharp}` 逐字节不变；③a 单独一棵，
    免得 `start_rust_stack.ps1` 起栈时 `Remove-Item` 把 ③b 的 `game-svr.out.log` 覆盖掉。
    """
    w = work_dir.rstrip("/\\")
    w_a = pass_a_dir.rstrip("/\\") if pass_a_dir else (w + "-a")
    return w, f"{w}/rust", f"{w}/csharp", w_a


def window_judge_items(work_dir: str, pass_a_dir: str = "", check_dir: str = HERE):
    """五条窗口判据（只聚合既有脚本，不复制判据逻辑）：

      ③b：服务端 `vis>=1` + 客户端 `pairs>=1` + 两侧 `visible`；
      ③a：服务端 `vis==0 && af==0` + 两侧 `none`。
    每项 = (名称, 判据脚本, argv, 必需证据件)。
    """
    _w, b_rust, b_csh, w_a = pass_roots(work_dir, pass_a_dir)
    a_rust, a_csh = f"{w_a}/rust", f"{w_a}/csharp"
    return [
        ("③b 服务端 vis>=1", AOI_DIAG,
         ["--log", f"{b_rust}/stack/game-svr.out.log", "--expect", "visible", "--min-vis", "1"],
         [f"{b_rust}/stack/game-svr.out.log"]),
        ("③b 客户端 pairs>=1", CLIENT_FRAMES,
         ["--capture", f"{b_rust}/s2c/capture.jsonl", "--min-appearance-pairs", "1"],
         [f"{b_rust}/s2c/capture.jsonl"]),
        ("③b 两侧 visible", PAIR_TOOL,
         ["--left", f"{b_rust}/s2c/capture.jsonl", "--right", f"{b_csh}/s2c/capture.jsonl",
          "--expect", "visible", "--min-pairs", "1"],
         [f"{b_rust}/s2c/capture.jsonl", f"{b_csh}/s2c/capture.jsonl"]),
        ("③a 服务端 vis==0", AOI_DIAG,
         ["--log", f"{a_rust}/stack/game-svr.out.log", "--expect", "none"],
         [f"{a_rust}/stack/game-svr.out.log"]),
        ("③a 两侧 none", PAIR_TOOL,
         ["--left", f"{a_rust}/s2c/capture.jsonl", "--right", f"{a_csh}/s2c/capture.jsonl",
          "--expect", "none"],
         [f"{a_rust}/s2c/capture.jsonl", f"{a_csh}/s2c/capture.jsonl"]),
    ]


def run_window_judges(work_dir: str, pass_a_dir: str = "", check_dir: str = HERE) -> Report:
    """跑五条窗口判据并聚合成一个 rc：0 全绿 / 1 有 RED / 2 有 PREREQ（缺证据/判不了）。

    fail-closed：任何必需证据件缺失 ⇒ PREREQ（**绝不静默当绿**）；缺判据脚本同理。
    """
    rep = Report()
    for name, tool, args, arts in window_judge_items(work_dir, pass_a_dir, check_dir):
        missing = [p for p in arts if not os.path.isfile(p)]
        if missing:
            rep.prereq.append(f"{name}：缺窗口证据 {missing}")
            continue
        path = os.path.join(check_dir, tool)
        if not os.path.isfile(path):
            rep.prereq.append(f"{name}：缺判据脚本 {path}")
            continue
        rc, out = run_tool(path, args, timeout=180)
        if rc == 0:
            rep.info.append(f"OK   {name}（{last_line(out)}）")
        elif rc == 2:
            rep.prereq.append(f"{name} PREREQ：判不了（末行={last_line(out)}）")
        else:
            rep.problems.append(f"{name} RED（末行={last_line(out)}）")
    return rep


# ---------------------------------------------------------------- 计划文本

def plan_text(
    repo: str,
    ref: str,
    mirserver: str,
    site: str,
    site_none: str,
    work_dir: str,
    target_dir: str,
    seed: str,
    view_range: int,
    stale: bool,
    site_note: str = "",
    pass_a_dir: str = "",
    bin_missing: bool = False,
) -> str:
    w, rust, csh, w_a = pass_roots(work_dir, pass_a_dir)
    rust_a = f"{w_a}/rust"
    csh_a = f"{w_a}/csharp"
    bin_dir = f"{target_dir.rstrip('/\\')}/debug"
    tgt_win = target_dir.rstrip("/\\").replace("/", chr(92))
    lines: list[str] = []
    A = lines.append

    A(f"# ③ 两侧同框对照窗口计划（卡 openmir2-client-e2e-enter-gaps）")
    A(f"# 构建源 repo={repo or '<未给>'} ref={ref}")
    A(f"# ③b 有怪点 site={site} / ③a 反向点 site_none={site_none} / view_range={view_range}")
    if site_note:
        A(f"# {site_note}")
    A("")
    if stale:
        A("# ==== 修复块：G1 RED（构建源过旧）⇒ 窗口脚本的 repo= 必须换树（勿动共享主检出） ====")
        A("# root 的 client-e2e-window.sh 默认 repo=/e/Users/gxh/Documents/GitHub/OpenMir2（共享主检出，")
        A("# rev 车道工作区，实测停在 ad0592b19 ⇒ 无 vr/new/af 读数）。用专用 worktree 从 origin/master 构建：")
        A("#   git -C <任一检出> worktree add --detach E:/tmp/m2svr-window-origin origin/master")
        A("#   git -C E:/tmp/m2svr-window-origin rev-parse HEAD   # 记进证据")
        A("# 再把窗口脚本 2 行指向它：")
        A("#   repo=/e/tmp/m2svr-window-origin")
        A(f"#   tgt='{tgt_win}'")
        A("# 开窗前复跑本门至 GREEN：")
        A(f"#   python -I -B window_handoff.py --repo E:/tmp/m2svr-window-origin --ref HEAD")
        A("")
    A("# 0) 前置：端口/进程（5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN；仅共享 3306 可 UP）")
    A("#    起栈禁接管道（禁 | tee/tail/head/grep，会把调用挂死）；一律 > <log> 2>&1")
    A("")
    A("# 0a) 构建 Rust 侧六件到 <target-dir>（必须先做：start_rust_stack.ps1 在 <target-dir>/debug 缺 exe/config 时会")
    A("#     PREREQ_MISSING exit 2 / 或在窗口里空转 —— §2 的 -RustBinDir 指向这里）")
    _build_repo = repo or "<检出>"
    A(f"#   git -C {_build_repo} rev-parse HEAD   # 记进证据（构建源 oid）")
    A(f"#   cd {_build_repo}/mir2-rs && CARGO_TARGET_DIR={target_dir} cargo build --offline \\")
    A("#     -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv")
    if bin_missing:
        A(f"#   ⚠ 构建产物不存在：{bin_dir}/game-svr.exe（本次 --plan 时实测）⇒ 不先跑 §0a 就地起栈必 PREREQ_MISSING")
    A("")
    A("# 1) 站位：两侧必须逐字节同坐标（跑完还原并记录 before/after）")
    A(f"#   mysql> UPDATE characters SET x={site.split(',')[0]}, y={site.split(',')[1]} WHERE chrname='aaa';  -- ③b 有怪")
    A(f"#   mysql> UPDATE characters SET x={site_none.split(',')[0]}, y={site_none.split(',')[1]} WHERE chrname='aaa';  -- ③a 反向")
    A("")
    A("# 2) ③b 两侧跑（有怪点；两侧不同时起 —— 端口互斥）")
    A("#   [③b] Rust：")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 ^")
    A(f"#       -RustBinDir {bin_dir} -Seed {seed} -LogDir {rust}/stack")
    A(f"#     python tools/capture/mir2_proxy.py --out {rust}/proxy --map 7200=127.0.0.1:7600")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 ^")
    A(f"#       -Account mir2test -Password mir2pass -OutDir {rust}/shots -WalkClicks 0 -AutoPlaySec 12 ^")
    A(f"#       -SkipReenter -SkipLogout -SkipTalk")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 ^")
    A(f"#       -ProxyDir {rust}/proxy -OutDir {rust}/s2c")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Stop")
    A("#   [③b] C#（先读 85-csharp-capture-topology-rootcause.md；同一条 capture 管线）：")
    A(f"#     python -I -B {CSHARP_PREFLIGHT} --repo {repo or '<检出>'} --mirserver {mirserver} ^")
    A(f"#       --shadow-config <影子 RunGate/config.conf>        # 必须 GREEN 再起栈")
    A(f"#     powershell -ExecutionPolicy Bypass -File {mirserver}/start-all.ps1")
    A(f"#     # 影子 game-gate(7600) + 代理占 7200；再到 C# 侧客户端与解码：")
    A(f"#     python tools/capture/mir2_proxy.py --out {csh}/proxy --map 7200=127.0.0.1:7600")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 -Account mir2test ^")
    A(f"#       -Password mir2pass -OutDir {csh}/shots -WalkClicks 0 -AutoPlaySec 12 -SkipReenter -SkipLogout -SkipTalk")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 ^")
    A(f"#       -ProxyDir {csh}/proxy -OutDir {csh}/s2c")
    A(f"#     powershell -ExecutionPolicy Bypass -File {mirserver}/stop-all.ps1   # 收 C# 栈（或等价 -Stop）")
    A("")
    A(f"# 3) ③b 判据（一键：python -I -B window_handoff.py --judge --work-dir {w}）")
    A(f"#   服务端（Rust，新读数 vr/new/af）：python -I -B check_aoi_visibility_diag.py \\")
    A(f"#     --log {rust}/stack/game-svr.out.log --expect visible --min-vis 1")
    A(f"#   客户端（Rust）：python -I -B check_client_s2c_frames.py \\")
    A(f"#     --capture {rust}/s2c/capture.jsonl --min-appearance-pairs 1")
    A(f"#   ③b 两侧：python -I -B aoi_visibility_pair.py --left {rust}/s2c/capture.jsonl \\")
    A(f"#     --right {csh}/s2c/capture.jsonl --expect visible --min-pairs 1")
    A("")
    A(f"# 4) ③a 反向第二趟：把 §1 的 aaa 改到 site_none={site_none} 后重跑 §2，**独立目录**")
    A(f"#    （③a 根 = {w_a}；start_rust_stack.ps1 每次起栈删旧 game-svr.out.log ⇒ 不独立就覆盖 ③b 读数）")
    A(f"#    ③a Rust：§2 的 [③b] Rust 命令，把 -LogDir/--out/-OutDir 的 {rust} 换成 {rust_a}")
    A(f"#    ③a C#  ：§2 的 [③b] C# 命令，把 --out/-OutDir 的 {csh} 换成 {csh_a}")
    A(f"#   判据（一键：python -I -B window_handoff.py --judge --work-dir {w} --pass-a-dir {w_a}）")
    A(f"#   服务端：python -I -B check_aoi_visibility_diag.py \\")
    A(f"#     --log {rust_a}/stack/game-svr.out.log --expect none")
    A(f"#   ③a 两侧：python -I -B aoi_visibility_pair.py --left {rust_a}/s2c/capture.jsonl \\")
    A(f"#     --right {csh_a}/s2c/capture.jsonl --expect none")
    A(f"#   （③a 两侧都必须 0 对出现帧 + 服务端 vis==0 且 af==0）")
    A("")
    A("# 5) 收工：start_rust_stack.ps1 -Stop / C# -Stop；还原 aaa；实拍十端口；SHA256SUMS 按 LF/git-blob 口径")
    A("")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------- selftest

def _write_fake_tools(tmp: str, rust_result: str, bad_selftest: str = "") -> None:
    """在 tmp 里造一套假判据脚本：rust preflight 打 `RESULT:`，其余打 `SELFTEST OK/FAILED`。"""
    rust_rc = {"GREEN": 0, "RED": 1, "PREREQ_RED": 2}[rust_result]
    rust_state = "FAILED" if bad_selftest == RUST_PREFLIGHT else "OK"
    rust_selftest_rc = 1 if rust_state == "FAILED" else 0
    with open(os.path.join(tmp, RUST_PREFLIGHT), "w", encoding="utf-8") as fh:
        fh.write(
            "import sys\n"
            "if '--selftest' in sys.argv:\n"
            f"    print('SELFTEST {rust_state} (1/1)')\n"
            f"    sys.exit({rust_selftest_rc})\n"
            f"print('RESULT: {rust_result}')\n"
            f"sys.exit({rust_rc})\n"
        )
    for tool in TOOL_SELFTESTS:
        if tool == RUST_PREFLIGHT:
            continue
        state = "FAILED" if tool == bad_selftest else "OK"
        rc = 1 if state == "FAILED" else 0
        with open(os.path.join(tmp, tool), "w", encoding="utf-8") as fh:
            fh.write(f"import sys\nprint('SELFTEST {state} (1/1)')\nsys.exit({rc})\n")


def _write_fake_judges(check_dir: str, rc_map: dict | None = None) -> None:
    """造三件假判据脚本：按 `_rc/<tool>.rc` 里的 rc 退出（缺文件 ⇒ 0）。"""
    os.makedirs(check_dir, exist_ok=True)
    rc_dir = os.path.join(check_dir, "_rc")
    os.makedirs(rc_dir, exist_ok=True)
    for tool in (AOI_DIAG, CLIENT_FRAMES, PAIR_TOOL):
        with open(os.path.join(check_dir, tool), "w", encoding="utf-8") as fh:
            fh.write(
                "import os, sys\n"
                "d = os.path.join(os.path.dirname(os.path.abspath(__file__)), '_rc')\n"
                f"f = os.path.join(d, {tool!r} + '.rc')\n"
                "sys.exit(int(open(f).read().strip()) if os.path.isfile(f) else 0)\n"
            )
    for tool, rc in (rc_map or {}).items():
        with open(os.path.join(rc_dir, tool + ".rc"), "w", encoding="utf-8") as fh:
            fh.write(f"{rc}\n")


def _write_fake_artifacts(root: str) -> None:
    for rel in ("rust/stack/game-svr.out.log", "rust/s2c/capture.jsonl", "csharp/s2c/capture.jsonl"):
        path = os.path.join(root, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write("x\n")


def selftest() -> int:
    ok = 0
    total = 0

    def chk(name: str, cond: bool, detail: str = "") -> None:
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
        print(f"[{'PASS' if cond else 'FAIL'}] selftest {name}{(' ' + detail) if detail else ''}")

    # --- 解析 ---
    chk("parse-result-green", parse_result("x\nRESULT: GREEN\n") == "GREEN")
    chk("parse-result-red", parse_result("RESULT: RED\n") == "RED")
    chk("parse-result-prereq", parse_result("RESULT: PREREQ_RED\n") == "PREREQ_RED")
    chk("parse-result-suffix", parse_result("RESULT: GREEN (Top-5)\n") == "GREEN")
    chk("parse-result-missing", parse_result("no result here") is None)
    chk("parse-result-last-wins", parse_result("RESULT: RED\nRESULT: GREEN\n") == "GREEN")
    chk("parse-selftest-ok", parse_selftest("SELFTEST OK (19/19)") == "OK")
    chk("parse-selftest-green", parse_selftest("SELFTEST GREEN") == "GREEN")
    chk("parse-selftest-failed", parse_selftest("SELFTEST FAILED (18/19)") == "FAILED")
    chk("parse-selftest-red", parse_selftest("SELFTEST RED") == "RED")
    chk("parse-selftest-missing", parse_selftest("nothing") is None)
    chk("parse-best-point",
        parse_best_point("BEST (605,578) E=12.681 view_range=12 box=(605,578) range=30 count=10 name=半兽人")
        == {"x": 605, "y": 578, "e": "12.681"})
    chk("parse-best-point-missing", parse_best_point("RANK (605,578) E=12.681") is None)
    chk("parse-best-point-int-e", parse_best_point("BEST (1,2) E=3") == {"x": 1, "y": 2, "e": "3"})
    # 选点 fail-soft：缺部署根 ⇒ 不得抛异常，返回 (None, 说明)
    _none_site, _none_note = resolve_best_site(HERE, "E:/no-such-mirserver-xyz", "0", 12)
    chk("resolve-best-site-failsoft", _none_site is None and "选点跳过" in _none_note, _none_note)

    # --- 端到端聚合（假判据脚本） ---
    with tempfile.TemporaryDirectory(prefix="wh-allgreen-") as tmp:
        _write_fake_tools(tmp, "GREEN")
        rep = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                         with_csharp=False, rank=0, view_range=12, map_name="0")
        chk("agg-allgreen-rc0", rep.rc() == 0, str(rep.rc()))
        chk("agg-allgreen-rust-ok", rep.rust_ok)

    with tempfile.TemporaryDirectory(prefix="wh-stale-") as tmp:
        _write_fake_tools(tmp, "RED")
        rep = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                         with_csharp=False, rank=0, view_range=12, map_name="0")
        chk("agg-stale-rc1", rep.rc() == 1, str(rep.rc()))
        chk("agg-stale-flag", rep.build_source_stale)
        chk("agg-stale-reason", any("G1" in p for p in rep.problems), "; ".join(rep.problems))
        # 计划里必须出现修复块；构建源新鲜时不得出现。
        stale_plan = plan_text("R", "HEAD", "M", "287,620", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                               "42", 12, stale=True)
        fresh_plan = plan_text("R", "HEAD", "M", "287,620", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                               "42", 12, stale=False)
        chk("plan-stale-remediation", "worktree add --detach" in stale_plan)
        chk("plan-fresh-no-remediation", "worktree add --detach" not in fresh_plan)
        chk("plan-has-comparators",
            all(s in fresh_plan for s in ("check_aoi_visibility_diag.py", "check_client_s2c_frames.py",
                                          "aoi_visibility_pair.py", "--expect none", "--expect visible")))
        chk("plan-has-sites", "287,620" in fresh_plan and "291,666" in fresh_plan)
        # O-2 收口：③a 反向必须是完整可粘贴命令，不能用「同上加 …」这种半句。
        chk("plan-none-full-commands",
            "--expect none" in fresh_plan
            and "check_aoi_visibility_diag.py" in fresh_plan
            and "aoi_visibility_pair.py" in fresh_plan
            and "同上加" not in fresh_plan)
        chk("plan-none-second-pass-note", "第二趟" in fresh_plan and "site_none=291,666" in fresh_plan)
        _note_plan = plan_text("R", "HEAD", "M", "605,578", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                               "42", 12, stale=False,
                               site_note="③b 最密点自动选取 BEST=605,578 E=12.681")
        chk("plan-site-note", "最密点自动选取" in _note_plan and "605,578" in _note_plan)
        chk("plan-no-note-when-empty", "最密点自动选取" not in fresh_plan)
        chk("plan-has-pipeline",
            all(s in fresh_plan for s in ("start_rust_stack.ps1", "mir2_proxy.py", "export_s2c.ps1",
                                          "mir_flow.ps1")))
        # J1：③a 第二趟必须独立目录（否则 `start_rust_stack.ps1` 删旧 log 会覆盖 ③b 读数）。
        chk("plan-pass-a-separate-dirs",
            "E:/tmp/w-a/rust" in fresh_plan and "E:/tmp/w-a/csharp" in fresh_plan
            and "E:/tmp/w/rust" in fresh_plan and "E:/tmp/w/csharp" in fresh_plan)
        chk("plan-pass-a-judge-args",
            "--log E:/tmp/w-a/rust/stack/game-svr.out.log" in fresh_plan
            and "--right E:/tmp/w-a/csharp/s2c/capture.jsonl" in fresh_plan
            and "--left E:/tmp/w/rust/s2c/capture.jsonl" in fresh_plan)
        chk("plan-judge-command",
            "--judge" in fresh_plan and "--pass-a-dir E:/tmp/w-a" in fresh_plan)
        chk("plan-pass-a-not-overwritten-note", "覆盖 ③b 读数" in fresh_plan)
        # 片㉓：计划必须自带 §0a 构建步骤（-RustBinDir 指向 <target-dir>/debug；缺 exe ⇒ PREREQ_MISSING exit 2）
        _build_plan = plan_text("R", "HEAD", "M", "287,620", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                                "42", 12, stale=False)
        chk("plan-has-build-step",
            all(s in _build_plan for s in ("cargo build --offline", "CARGO_TARGET_DIR=E:/tmp/tgt",
                                           "-p game-svr", "-p game-gate", "-p db-srv")))
        chk("plan-build-step-repo", "cd R/mir2-rs" in _build_plan)
        chk("plan-build-before-start",
            ("cargo build --offline" in _build_plan and "# 2) ③b" in _build_plan
             and _build_plan.index("cargo build --offline") < _build_plan.index("# 2) ③b")))
        _missing_plan = plan_text("R", "HEAD", "M", "287,620", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                                  "42", 12, stale=False, bin_missing=True)
        chk("plan-bin-missing-warning",
            "构建产物不存在" in _missing_plan and "PREREQ_MISSING" in _missing_plan
            and "构建产物不存在" not in _build_plan)

    with tempfile.TemporaryDirectory(prefix="wh-badtool-") as tmp:
        _write_fake_tools(tmp, "GREEN", bad_selftest="aoi_visibility_pair.py")
        rep = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                         with_csharp=False, rank=0, view_range=12, map_name="0")
        chk("agg-badtool-rc1", rep.rc() == 1, str(rep.rc()))
        chk("agg-badtool-reason",
            any("aoi_visibility_pair.py" in p and "G2" in p for p in rep.problems),
            "; ".join(rep.problems))

    with tempfile.TemporaryDirectory(prefix="wh-missing-") as tmp:
        _write_fake_tools(tmp, "GREEN")
        os.remove(os.path.join(tmp, "check_client_s2c_frames.py"))
        rep = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                         with_csharp=False, rank=0, view_range=12, map_name="0")
        chk("agg-missing-prereq-rc2", rep.rc() == 2, str(rep.rc()))
        chk("agg-missing-reason",
            any("check_client_s2c_frames.py" in p for p in rep.prereq), "; ".join(rep.prereq))

    with tempfile.TemporaryDirectory(prefix="wh-prereq-") as tmp:
        _write_fake_tools(tmp, "PREREQ_RED")
        rep = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                         with_csharp=False, rank=0, view_range=12, map_name="0")
        chk("agg-rust-prereq-rc2", rep.rc() == 2, str(rep.rc()))

    # --- 缺 repo ⇒ PREREQ（G1 不打绿） ---
    with tempfile.TemporaryDirectory(prefix="wh-norepo-") as tmp:
        _write_fake_tools(tmp, "GREEN")
        rep = run_checks(tmp, repo="", ref="HEAD", mirserver="M", shadow_config="",
                         with_csharp=False, rank=0, view_range=12, map_name="0")
        chk("agg-norepo-prereq", rep.rc() == 2 and any("--repo" in p for p in rep.prereq), str(rep.rc()))

    # --- J1 窗口判据聚合（--judge）：五条判据消费 ③b/③a 两趟产物 ---
    with tempfile.TemporaryDirectory(prefix="wh-judge-") as tmp:
        cdir = os.path.join(tmp, "check")
        work = os.path.join(tmp, "work")
        _write_fake_judges(cdir)
        _write_fake_artifacts(work)
        _write_fake_artifacts(work + "-a")  # ③a 缺省根 = <work-dir>-a
        rep = run_window_judges(work, "", cdir)
        chk("judge-allgreen-rc0", rep.rc() == 0, str(rep.rc()))
        chk("judge-five-items", len(rep.info) == 5, str(len(rep.info)))

    with tempfile.TemporaryDirectory(prefix="wh-judge-red-") as tmp:
        cdir = os.path.join(tmp, "check")
        work = os.path.join(tmp, "work")
        _write_fake_judges(cdir, {AOI_DIAG: 1})
        _write_fake_artifacts(work)
        _write_fake_artifacts(work + "-a")
        rep = run_window_judges(work, "", cdir)
        chk("judge-red-rc1", rep.rc() == 1, str(rep.rc()))
        chk("judge-red-two", len(rep.problems) == 2 and len(rep.info) == 3,
            f"{len(rep.problems)}/{len(rep.info)}")

    with tempfile.TemporaryDirectory(prefix="wh-judge-missing-") as tmp:
        cdir = os.path.join(tmp, "check")
        work = os.path.join(tmp, "work")
        _write_fake_judges(cdir)
        _write_fake_artifacts(work)
        _write_fake_artifacts(work + "-a")
        os.remove(os.path.join(work, "csharp", "s2c", "capture.jsonl"))
        rep = run_window_judges(work, "", cdir)
        chk("judge-missing-prereq-rc2", rep.rc() == 2, str(rep.rc()))
        chk("judge-missing-names-pair",
            any("③b 两侧" in p for p in rep.prereq), "; ".join(rep.prereq))

    print(f"SELFTEST {'OK' if ok == total else 'FAILED'} ({ok}/{total})")
    return 0 if ok == total else 1


# ---------------------------------------------------------------- main

def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default="", help="Rust 构建源检出（窗口脚本 repo= 指向的那棵）")
    ap.add_argument("--ref", default="HEAD", help="被检查的 ref/oid（默认 HEAD）")
    ap.add_argument("--mirserver", default="E:/MirServer", help="oracle 部署根（默认 E:/MirServer）")
    ap.add_argument("--check-dir", default=HERE, help="判据脚本所在目录（默认 = 本脚本目录）")
    ap.add_argument("--with-csharp", action="store_true", help="同时跑 C# 侧抓包前置门（G3）")
    ap.add_argument("--shadow-config", default="", help="影子 RunGate/config.conf（G3）")
    ap.add_argument("--rank", type=int, default=0, help=">0 时打印 MonGen 窗口内期望出生怪数排行（选点）")
    ap.add_argument("--view-range", type=int, default=12, help="观察者视野半径（默认 12）")
    ap.add_argument("--map-name", default="0", help="MonGen 逻辑图名（默认 0）")
    ap.add_argument("--plan", action="store_true", help="打印 ③ 两侧对照窗口计划")
    ap.add_argument("--plan-only", action="store_true", help="只打印计划，不跑判据")
    ap.add_argument("--site", default="287,620", help="③b 有怪点 x,y（默认 287,620，片⑦已证 13 对）")
    ap.add_argument("--best-site", action="store_true",
                    help="③b site 由 MonGen 排行自动选取（--best-point；root ③①「换怪密点」）")
    ap.add_argument("--site-none", default="291,666", help="③a 反向点 x,y（默认 291,666，E=0.000）")
    ap.add_argument("--work-dir", default="E:/tmp/window/cliente2e", help="窗口工作目录")
    ap.add_argument("--pass-a-dir", default="", help="③a 第二趟证据根（默认 = <work-dir>-a）")
    ap.add_argument("--judge", action="store_true",
                    help="聚合 ③b/③a 两趟窗口产物的五条 AOI 判据（消费既有 capture/log，不起栈）")
    ap.add_argument("--target-dir", default="E:/tmp/m2svr-window-target",
                    help="CARGO_TARGET_DIR（RustBinDir = <target-dir>/debug）")
    ap.add_argument("--seed", default="42", help="MIR2_RNG_SEED（两侧同种子）")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if args.judge:
        rep = run_window_judges(args.work_dir, args.pass_a_dir, args.check_dir)
        for line in rep.info:
            print(line)
        for line in rep.problems:
            print("RED " + line)
        for line in rep.prereq:
            print("PREREQ " + line)
        rc = rep.rc()
        label = "GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")
        if args.json:
            print(json.dumps({
                "aoi_gap3": label,
                "rc": rc,
                "work_dir": args.work_dir,
                "pass_a_dir": args.pass_a_dir or (args.work_dir.rstrip("/\\") + "-a"),
                "problems": rep.problems,
                "prereq": rep.prereq,
                "info": rep.info,
            }, ensure_ascii=False))
        else:
            print(f"AOI_GAP3={label}")
        return rc

    site = args.site
    site_note = ""
    if args.best_site:
        auto_site, site_note = resolve_best_site(
            args.check_dir, args.mirserver, args.map_name, args.view_range)
        if auto_site:
            site = auto_site

    if args.plan_only:
        bin_missing = not os.path.isfile(os.path.join(args.target_dir, "debug", "game-svr.exe"))
        print(plan_text(args.repo, args.ref, args.mirserver, site, args.site_none,
                        args.work_dir, args.target_dir, args.seed, args.view_range,
                        stale=False, site_note=site_note, pass_a_dir=args.pass_a_dir,
                        bin_missing=bin_missing))
        return 0

    rep = run_checks(args.check_dir, args.repo, args.ref, args.mirserver,
                     args.shadow_config, args.with_csharp, args.rank, args.view_range, args.map_name)

    for line in rep.info:
        print(line)
    for line in rep.problems:
        print("RED " + line)
    for line in rep.prereq:
        print("PREREQ " + line)

    rc = rep.rc()
    ready = "YES" if rc == 0 else "NO"
    if args.plan:
        print("")
        bin_missing = not os.path.isfile(os.path.join(args.target_dir, "debug", "game-svr.exe"))
        print(plan_text(args.repo, args.ref, args.mirserver, site, args.site_none,
                        args.work_dir, args.target_dir, args.seed, args.view_range,
                        stale=rep.build_source_stale, site_note=site_note, pass_a_dir=args.pass_a_dir,
                        bin_missing=bin_missing))
    if args.json:
        print(json.dumps({
            "window_ready": ready,
            "rc": rc,
            "result": "GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED"),
            "build_source_stale": rep.build_source_stale,
            "problems": rep.problems,
            "prereq": rep.prereq,
            "info": rep.info,
        }, ensure_ascii=False))
    else:
        print(f"WINDOW_READY={ready}")
        print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
