#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""window_handoff.py —— 卡 `openmir2-client-e2e-enter-gaps` **①②③ 收口**的
**窗口就绪聚合门 + 一键两侧 capture 计划**（只读、零起栈、零端口）。

背景（root 2026-10-14 00:4x ③；本片 = 片⑰）：
  ③「同屏没怪」的离线判决已经成立（`70-aoi-visibility-verdict.txt`：`(291,666)` 在 map 0
  所有 MonGen 生成框之外 ⇒ `vis=0` 正确），但**唯一未闭环的 R 级判据 = 两侧同框真机对照**，
  只能由 root 在独占窗口里跑。窗口是一次性资源，开窗前把「工具/构建源/落点/命令」四件事
  做成**一条机判的 GO/NO-GO**，避免再出现「开了窗口才发现构建源过旧、判据 PREREQ_RED」的烧窗。

本工具**不重新实现任何判据**，只做四件事：
  G1 **聚合 Rust 侧窗口前置门**：调片⑮ `check_rust_window_preflight.py`（R1..R7），解析 `RESULT:`。
     片㊲ 起 R6 = 「构建源必须是当前基线（默认 `origin/master`）」；R1 只管「含两片修复」
     （单向 `--is-ancestor`），一份含两片但停在旧 master 的源 R1 仍 GREEN，必须靠 R6 才能判「测旧件」。
     片㊻ 起 R7 = 「预构建 exe ↔ 构建源出处」（`--target-dir` + `--provenance` 传出）：R6 只比
     **生产路径的 git 差异**，比不出「源刷了但没重编译/ 暖 target 里是别处拷来的旧 exe」（2026-10-14
     实测：专用源只 `checkout` 不 `cargo build` 时 `game-svr.exe` 仍是上一版）⇒ 窗口会从旧 exe 起栈。
     R7 用出处清单（`source_head` + 六件 sha256）做确定性对账；清单缺失 ⇒ PREREQ（先 `--record-provenance`）。
     其中 R1/R2 直接点住「构建源是否含 ③ 判据要的两片」——片⑪（`vr/new/af` 读数）与
     片㉜（受击帧 `SM_STRUCK` 广播 ⇒ DoD③「能攻击造成伤害」）——这是 2026-10-14 实测到的真缺陷
     （共享主检出停在 `ad0592b19`；专用源 `m2svr-window-origin` 停在 `6dba8719d`、缺片㉜）。
     门一旦 RED，`--plan` 的修复块点明「换/刷新构建源」而不会让你白烧窗口。
  G2 **聚合判据工具自检**：证据目录 11 件判据脚本的 `--selftest` 必须全 `SELFTEST OK`（缺一件 ⇒ PREREQ）。
  G3（可选）**聚合 C# 侧抓包前置门**：调片⑭ `check_csharp_capture_preflight.py --shadow-config ...`。
  G4（`--plan`/`--plan-only`）**计划 ↔ 工具面机判**（片㉕）：计划里每个被调脚本必须存在、只用它声明
     的参数，且必须含「代理(7200)/影子 gate(7600) 清端口 + 7200/7600 回落断言 + C# 清栈保留 3306」的
     teardown 段 —— 手工起的影子 gate 与代理**不在** `start_rust_stack.ps1` 的 pid 文件里 ⇒ `-Stop`
     不会收它们，留着会让下一趟起栈预检 RED / C# 影子绑不上 7600，白烧一次性窗口。
  G5（默认开）**root 一键窗口脚本口径 guard**（片㉝，fail-closed）：解析 root 实际会跑的
     `client-e2e-window.sh`（`--window-script`，默认 `E:/tmp/openmir2-watch/client-e2e-window.sh`）里的
     `repo=` / `tgt=`，必须与本工具的 `--repo` / `--target-dir` 一致，且 `repo=` 不得指向共享主检出。
     为什么必须与 G1 分开：G1 校验的是**你传进来的** `--repo`，而 root 开窗时跑的是**脚本里那两行** ——
     两者可以不一致（2026-10-14 实测：工具的 `--repo` 已指专用源，root 的脚本仍指共享主检出），
     于是 `WINDOW_READY=YES` 而窗口照样构建另一棵树 / 从旧 exe 起栈。脚本文件不存在（干净检出）
     ⇒ 该门跳过（INFO），不当 RED。
  J1（`--judge`）**聚合 ③b/③a 两趟窗口产物的八条判据**（只消费既有 capture/log，不起栈）：
     **①②（root 2026-10-14 15:0x 头号项）** ③b Rust 侧 `52` 客户端 58B 布局 + `CM_RUN>=1`（`check_client_run_frames.py`）
     与 `53` 触发前置/已发/尾长 12B（`check_healthspellchanged_trigger.py`）；③b 服务端 `vis>=1` /
     ③b 客户端 `pairs>=1` / ③b 两侧 `visible` / **③b 两侧攻击/伤害（DoD③ 第三半「能攻击造成伤害」）** /
     ③a 服务端 `vis==0` / ③a 两侧 `none`。
     ①② 只在 ③b 趟判（③a 反向点 `(291,666)` 是城墙走道，`CanRun` 恒假 ⇒ `CM_RUN` 必 0，见 `61-*` §B）。
     ③a 第二趟必须用**独立目录**（默认 `<work-dir>-a`）——`start_rust_stack.ps1` 每次起栈都会删掉
     旧 `game-svr.out.log`（`Start-One` 的 `Remove-Item $out, $err`），共用一个目录会把 ③b 读数覆盖掉。
  另加一条**只读信息行**：`check_mongen_dense_points.py --rank` 的窗口内期望出生怪数排行（选点用）。

`--plan` 把 ①②③ 两侧对照的**全部命令**（Rust 起栈/代理/客户端/解码 + C# 侧 + 八条判据 + 收工）
按 runbook `82-two-side-window-runbook.md` 的口径生成成可粘贴文本；①②（root 2026-10-14 15:0x 头号项）
额外带 §1d 压血（53 触发前置）+ `mir_flow.ps1 -RunHoldMs 500`（跑键探针）；当 G1 判 RED 时，额外打印
「改用专用 worktree 从 origin/master 构建」的修复块（不动共享主检出）；当 G5 判 RED 时，额外打印
「把 root 的一键脚本换成入库的逐字替身 `client-e2e-window.fixed.sh.txt`（仅 2 行 diff）」的修复块。

  # 门（可选显式指定 root 实际会跑的一键脚本；默认 = E:/tmp/openmir2-watch/client-e2e-window.sh）
  python -I -B window_handoff.py --repo <构建源> --window-script <脚本路径>
  python -I -B window_handoff.py --repo <构建源> --window-script ''   # 干净检出上显式关掉 G5

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
# 仓根（含 `tools/` 与 `mir2-rs/`）：HERE = <repo>/mir2-rs/tests/parity/world/<this-dir> ⇒ 上溯 5 级。
DEFAULT_REPO_ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", "..", ".."))
# 本机 MySQL CLI（与 `mir2-rs/tests/parity/m3/run_pair.ps1:127` 同源；`--password=` 即空口令）。
# 片㉗：站位计划必须打印**可原样执行**的 SQL，且列名必须是真实列（`MapName/CX/CY`）。
MYSQL_EXE = os.environ.get("MIR2_MYSQL_EXE", "D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe")

RUST_PREFLIGHT = "check_rust_window_preflight.py"
CSHARP_PREFLIGHT = "check_csharp_capture_preflight.py"
# 片㊻：R7 出处清单名（与 `check_rust_window_preflight.py::PROVENANCE_NAME` 同源）；
# G1 把它传给 Rust 前置门的 `--provenance`（<target-dir>/window-provenance.json）。
PROVENANCE_NAME = "window-provenance.json"
# 片㉝：root 实际会跑的一键窗口脚本（默认；文件不存在 ⇒ 干净检出上跳过 G5，不当 RED）。
DEFAULT_WINDOW_SCRIPT = os.environ.get(
    "MIR2_WINDOW_SCRIPT", "E:/tmp/openmir2-watch/client-e2e-window.sh")
# 片㊱：入库的逐字替身（㉘，仅 `repo=`/`tgt=` 两行 diff）。G5 RED 修复块的「换成替身」一路
# 必须用**替身自己的** repo=/tgt= 复跑，否则 `cp` 换完后门仍 RED（见证据 171/172）。
STANDIN_SCRIPT_NAME = "client-e2e-window.fixed.sh.txt"
STANDIN_SCRIPT = os.environ.get(
    "MIR2_WINDOW_STANDIN", os.path.join(HERE, STANDIN_SCRIPT_NAME))
# 共享主检出（rev 车道的 worktree）；窗口构建源**不得**是它。
SHARED_MAIN_CHECKOUT_DEFAULT = os.environ.get(
    "MIR2_SHARED_CHECKOUT", "E:/Users/gxh/Documents/GitHub/OpenMir2")
TOOL_SELFTESTS = (
    "check_mongen_dense_points.py",
    "check_aoi_visibility_diag.py",
    "aoi_visibility_pair.py",
    "check_client_s2c_frames.py",
    "check_appearance_pairs.py",
    "check_attack_observability.py",
    "check_client_run_frames.py",
    "check_healthspellchanged_trigger.py",
    "verify_client_highhp_family.py",
    RUST_PREFLIGHT,
    CSHARP_PREFLIGHT,
)

# `--judge` 消费的判据脚本（③b/③a 两趟各跑服务端 / 客户端 / 两侧 / 攻击；①② 能力布局/跑/53）。
AOI_DIAG = "check_aoi_visibility_diag.py"
CLIENT_FRAMES = "check_client_s2c_frames.py"
PAIR_TOOL = "aoi_visibility_pair.py"
ATTACK_TOOL = "check_attack_observability.py"
# ①②（root 2026-10-14 15:0x 头号项）：52 客户端 58B 布局 + `CM_RUN`；53 触发前置 + `TCharDesc` 尾。
RUN_FRAMES = "check_client_run_frames.py"
HSC_TRIGGER = "check_healthspellchanged_trigger.py"

_RESULT_RE = re.compile(r"^RESULT:\s*(GREEN|RED|PREREQ_RED)\b", re.MULTILINE)
_SELFTEST_RE = re.compile(r"^SELFTEST\s+(OK|GREEN|FAILED|RED)\b", re.MULTILINE)
# 单图（`--map`）：`BEST (605,578) E=12.681 view_range=12 box=(605,578) range=30 count=10 name=半兽人`
# 跨图（`--all-maps`）：`BEST (D10071) (17,21) E=59.533 … tile=Walk clientmap=yes`（图名在坐标前的括号里）
_BEST_RE = re.compile(
    r"^BEST\s+(?:\((?P<map>[^()\s]+)\)\s+)?\((?P<x>-?\d+),(?P<y>-?\d+)\)"
    r"\s+E=(?P<e>[0-9]+(?:\.[0-9]+)?)\b",
    re.MULTILINE,
)
_PASS_STATES = ("OK", "GREEN")
_FAIL_STATES = ("FAILED", "RED")
# 窗口脚本里的赋值：`repo=/e/tmp/x` / `repo='…'` / `repo="…"`（片㉝）。
_WS_VALUE_RE = re.compile(r"(?<![\w-])(?P<key>repo|tgt)=('(?P<sq>[^']*)'|\"(?P<dq>[^\"]*)\"|(?P<bare>[^\s;]+))")
_MSYS_DRIVE_RE = re.compile(r"^/(?P<drive>[A-Za-z])/(?P<rest>.*)$")


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
    """从 `check_mongen_dense_points.py --best-point` 的 stdout 抽 `BEST …`。

    返回 `{"x": int, "y": int, "e": str, "map": str | None}`；`map` 只在 `--all-maps` 形态
    `BEST (图名) (x,y) E=…` 里出现（单图形态无图名 ⇒ `None`）。无该行 ⇒ `None`（调用方 fail-soft）。
    """
    m = _BEST_RE.search(text or "")
    if not m:
        return None
    return {"x": int(m.group("x")), "y": int(m.group("y")), "e": m.group("e"),
            "map": m.group("map")}


def parse_window_script(text: str) -> dict:
    """从 `client-e2e-window.sh` 里抽 `repo=` / `tgt=`（片㉝）。

    脚本第 6/7 行形如 `w=…; repo=/e/tmp/m2svr-window-origin`、`tgt='E:\\tmp\\m2svr-window-target'…`。
    同键出现多次**取最后一条**（对齐 bash 顺序脚本的 last-wins；reviewer-5 ㉝ O-1：旧实现取第一条，
    在活脚本出现重复键时会与 bash 实跑不一致 ⇒ 朝假绿判）。解析不到 ⇒ 该键 `None`。
    """
    out: dict = {"repo": None, "tgt": None}
    for m in _WS_VALUE_RE.finditer(text or ""):
        out[m.group("key")] = m.group("sq") or m.group("dq") or m.group("bare") or ""
    return out


def _norm_path(path: str) -> str:
    """路径归一（片㉝）：msys `/e/tmp` → `e:/tmp`、反斜杠 → 正斜杠、去尾斜杠、小写（Win 不区分大小写）。"""
    p = (path or "").strip().replace("\\", "/")
    m = _MSYS_DRIVE_RE.match(p)
    if m:
        p = f"{m.group('drive')}:/{m.group('rest')}"
    return p.rstrip("/").lower()


def standin_script_info(path: str = "") -> dict:
    """入库替身 `client-e2e-window.fixed.sh.txt` 的构建源（片㊱）。

    返回 `{"path", "repo", "tgt"}`；替身文件不存在 / 解析不到 ⇒ `repo`/`tgt` 为 `None`。
    用途：G5 RED 修复块里「换成替身」这条路的**配套复跑命令**必须用**替身的** `repo=/tgt=`，
    不能拿本计划的 `--repo` 去复跑 —— 替身的 `repo=/tgt=` 与计划可以不同（片㊱ 之前就踩到：
    `cp` 换完替身后按修复块的 `--repo <本计划>` 复跑，G5 仍 RED ⇒ 修复块自相矛盾/死循环，
    见证据 `171-g5-remediation-recheck.txt` / `172-redcheck-g5-remediation-recheck.txt`）。
    """
    p = path or STANDIN_SCRIPT
    out: dict = {"path": p, "repo": None, "tgt": None}
    try:
        if not os.path.isfile(p):
            return out
        with open(p, encoding="utf-8", errors="replace") as fh:
            parsed = parse_window_script(fh.read())
    except OSError:
        return out
    out["repo"] = parsed["repo"]
    out["tgt"] = parsed["tgt"]
    return out


def window_script_findings(
    path: str,
    repo: str,
    target_dir: str,
    shared_checkout: str = SHARED_MAIN_CHECKOUT_DEFAULT,
) -> tuple[list[str], list[str], list[str], bool]:
    """G5：root 实际会跑的一键脚本的 `repo=`/`tgt=` 必须与本工具的构建源一致（片㉝，fail-closed）。

    返回 `(problems, prereq, info, stale)`。文件不存在 ⇒ 只记 INFO 跳过（干净检出/评审环境）；
    解析不到 `repo=`、指向共享主检出、或与 `--repo`/`--target-dir` 不一致 ⇒ RED（`stale=True`）。
    """
    problems: list[str] = []
    prereq: list[str] = []
    info: list[str] = []
    stale = False
    if not path:
        return problems, prereq, info, stale
    if not os.path.isfile(path):
        info.append(f"INFO G5 跳过：root 一键脚本不存在 {path}（非本机/干净检出）")
        return problems, prereq, info, stale
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        prereq.append(f"G5 窗口脚本不可读：{exc}")
        return problems, prereq, info, stale
    parsed = parse_window_script(text)
    repo_in = parsed["repo"]
    tgt_in = parsed["tgt"]
    if repo_in is None:
        stale = True
        problems.append(f"G5 窗口脚本解析不到 repo=（{path}）—— 脚本形态变了？失败关闭")
    elif _norm_path(repo_in) == _norm_path(shared_checkout):
        stale = True
        problems.append(
            f"G5 窗口脚本 repo={repo_in} 指向**共享主检出**（{shared_checkout}，rev 车道工作区）"
            f" ⇒ 直接跑它会构建另一棵树（白烧窗口）")
    elif not repo:
        prereq.append("G5 未给 --repo，无法比对 root 窗口脚本的 repo=")
    elif _norm_path(repo_in) != _norm_path(repo):
        stale = True
        problems.append(
            f"G5 root 会跑的窗口脚本 repo={repo_in} != 本计划构建源 --repo={repo}"
            f" ⇒ 直接跑它会构建另一棵树（窗口 PREREQ_RED / 白烧）")
    else:
        info.append(f"OK   G5 root 窗口脚本 repo= 与本计划构建源一致（{repo_in}）")
    if tgt_in is not None and target_dir:
        if _norm_path(tgt_in) != _norm_path(target_dir):
            stale = True
            problems.append(
                f"G5 窗口脚本 tgt={tgt_in} != 本计划 --target-dir={target_dir}"
                f" ⇒ 会从另一份（可能过期的）exe 起栈，判据读数对不上构建源")
        else:
            info.append(f"OK   G5 root 窗口脚本 tgt= 与本计划 target-dir 一致（{tgt_in}）")
    return problems, prereq, info, stale


def resolve_best_site(
    check_dir: str,
    mirserver: str,
    map_name: str,
    view_range: int,
    all_maps: bool = False,
    map_dir: str = "",
    client_map_dir: str = "",
) -> tuple[str | None, str | None, str]:
    """用片⑯ MonGen 排行给 ③b 选「窗口内期望出生怪数最大」的点（root ③①「换怪密点」）。

    只读聚合：调 `check_mongen_dense_points.py --best-point`，解析 `BEST [(图名)] (x,y) E=`。
    返回 `(site, site_map, 说明)`；`all_maps=True` 时用 `--all-maps`（跨图排行）并回传**选点所在图名**。

    片㉛：把片㉚ 的「服务端可站立（tile=Walk）+ 冻结客户端带图（clientmap=yes）」过滤接进来 ——
    有目录就传 `--map-dir/--client-map-dir` + `--require-walkable/--require-client-map`，
    缺目录**不静默**：在说明里登记「未做该层过滤」。
    缺工具/缺 MonGen/解析不到 ⇒ `(None, None, 说明)`（fail-soft：计划回落到 `--site` 缺省，
    绝不静默假装选过点）。
    """
    tool = os.path.join(check_dir, "check_mongen_dense_points.py")
    mongen = os.path.join(mirserver, "M2GameSvr", "Envir", "MonGen.txt")
    mapinfo = os.path.join(mirserver, "M2GameSvr", "Envir", "MapInfo.txt")
    if not os.path.isfile(tool):
        return None, None, f"选点跳过：缺判据脚本 {tool}"
    if not os.path.isfile(mongen):
        return None, None, f"选点跳过：缺 {mongen}"
    args = ["--mongen", mongen]
    notes: list[str] = []
    if os.path.isfile(mapinfo):
        args += ["--mapinfo", mapinfo, "--require-enterable"]
    else:
        notes.append("缺 MapInfo.txt ⇒ 未做 enterable 过滤")
    if all_maps:
        args += ["--all-maps"]
    else:
        args += ["--map", map_name]
    args += ["--view-range", str(view_range)]
    if map_dir and os.path.isdir(map_dir):
        args += ["--map-dir", map_dir, "--require-walkable"]
    else:
        notes.append("服务端 Map 目录不存在 ⇒ 未做可站立过滤")
    if client_map_dir and os.path.isdir(client_map_dir):
        args += ["--client-map-dir", client_map_dir, "--require-client-map"]
    else:
        notes.append("客户端 Map 目录不存在 ⇒ 未做 clientmap 过滤")
    args += ["--best-point"]
    rc, out = run_tool(tool, args)
    best = parse_best_point(out)
    if rc != 0 or not best:
        detail = (f"选点跳过：check_mongen_dense_points.py --best-point rc={rc}（解析不到 BEST）")
        if notes:
            detail += "；" + "；".join(notes)
        return None, None, detail
    site = f"{best['x']},{best['y']}"
    picked_map = best.get("map") or map_name
    scope = "全图最密点" if all_maps else "本图最密点"
    note = (f"③b {scope}自动选取 BEST={site}"
            + (f" map={picked_map}" if all_maps else "")
            + f" E={best['e']}（check_mongen_dense_points.py --best-point，root ③①「换怪密点」）")
    if notes:
        note += "；" + "；".join(notes)
    return site, picked_map, note


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
        # 片㉝：root 实际会跑的一键脚本 ≠ 本工具构建源（WINDOW_READY 必须 NO）。
        self.live_script_stale = False

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
    target_dir: str = "",
    window_script: str = "",
    shared_checkout: str = SHARED_MAIN_CHECKOUT_DEFAULT,
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
        rc, out = run_tool(rp, [
            "--repo", repo, "--ref", ref, "--mirserver", mirserver,
            "--target-dir", target_dir,
            "--provenance", os.path.join(target_dir, PROVENANCE_NAME),
        ], timeout=180)
        result = parse_result(out)
        if rc == 0 and result == "GREEN":
            rep.rust_ok = True
            rep.info.append("OK   G1 Rust 窗口前置门 GREEN（R1..R7）")
        elif result == "RED" or rc == 1:
            rep.build_source_stale = True
            rep.problems.append(
                f"G1 Rust 窗口前置门 RED（构建源/读数标记不符；窗口会 PREREQ_RED）"
                f"—— 见下方修复块"
            )
        else:
            rep.prereq.append(f"G1 Rust 窗口前置门 PREREQ_RED（rc={rc}，末行={last_line(out)}）")

    # ---- G5 root 一键脚本口径（片㉝）：默认关（`--window-script` 为空 ⇒ 跳过），
    #      main() 默认传 DEFAULT_WINDOW_SCRIPT；selftest 直接调本函数时保持不启用。
    if window_script:
        _p, _q, _i, _stale = window_script_findings(window_script, repo, target_dir, shared_checkout)
        rep.problems.extend(_p)
        rep.prereq.extend(_q)
        rep.info.extend(_i)
        rep.live_script_stale = rep.live_script_stale or _stale

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
    """八条窗口判据（只聚合既有脚本，不复制判据逻辑）：

      ①②（仅 ③b 趟，Rust 侧）：`52` 客户端 58B 布局 + `CM_RUN>=1`；`53` 触发前置/已发/尾长 12B；
      ③b：服务端 `vis>=1` + 客户端 `pairs>=1` + 两侧 `visible` + 两侧攻击/伤害（DoD③ 第三半）；
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
        ("③b 两侧攻击/伤害", ATTACK_TOOL,
         ["--left", f"{b_rust}/s2c/capture.jsonl", "--right", f"{b_csh}/s2c/capture.jsonl",
          "--expect", "any", "--min-hits", "1"],
         [f"{b_rust}/s2c/capture.jsonl", f"{b_csh}/s2c/capture.jsonl"]),
        # ①② root 2026-10-14 15:0x 头号项（52 客户端布局 + 能跑 + 53 尾）。只在 ③b 趟判：
        # ③a 反向点 (291,666) 是城墙走道（`61-*` §B 实测 `CM_RUN=0`）⇒ 放到 ③a 会假 RED。
        ("①② Rust 52=58 + CM_RUN>=1", RUN_FRAMES,
         ["--capture", f"{b_rust}/s2c/capture.jsonl",
          "--expect-52-body-len", "58", "--min-run-count", "1"],
         [f"{b_rust}/s2c/capture.jsonl"]),
        ("① Rust 53 触发+尾长", HSC_TRIGGER,
         ["--log", f"{b_rust}/stack/game-svr.out.log",
          "--capture", f"{b_rust}/s2c/capture.jsonl"],
         [f"{b_rust}/stack/game-svr.out.log", f"{b_rust}/s2c/capture.jsonl"]),
        ("③a 服务端 vis==0", AOI_DIAG,
         ["--log", f"{a_rust}/stack/game-svr.out.log", "--expect", "none"],
         [f"{a_rust}/stack/game-svr.out.log"]),
        ("③a 两侧 none", PAIR_TOOL,
         ["--left", f"{a_rust}/s2c/capture.jsonl", "--right", f"{a_csh}/s2c/capture.jsonl",
          "--expect", "none"],
         [f"{a_rust}/s2c/capture.jsonl", f"{a_csh}/s2c/capture.jsonl"]),
    ]


def run_window_judges(work_dir: str, pass_a_dir: str = "", check_dir: str = HERE) -> Report:
    """跑八条窗口判据并聚合成一个 rc：0 全绿 / 1 有 RED / 2 有 PREREQ（缺证据/判不了）。

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
    site_map: str = "0",
    site_none_map: str = "0",
    live_script_stale: bool = False,
    window_script: str = "",
) -> str:
    w, rust, csh, w_a = pass_roots(work_dir, pass_a_dir)
    rust_a = f"{w_a}/rust"
    csh_a = f"{w_a}/csharp"
    bin_dir = f"{target_dir.rstrip('/\\')}/debug"
    bin_dir_w = bin_dir.replace("/", "\\")
    cs_gate_w = (mirserver.rstrip("/\\") + "/RunGate/GameGate.exe").replace("/", "\\")
    tgt_win = target_dir.rstrip("/\\").replace("/", chr(92))
    # 影子 gate 的 conf / 目录（片㉖）：路径必须**具体**，不能让 root 现场猜（<...> 占位 = 烧窗口）。
    shadow_dir = f"{w}/shadow"
    shadow_dir_w = shadow_dir.replace("/", "\\")
    rs_gate_conf = f"{shadow_dir}/game-gate.conf"
    cs_shadow_dir = f"{shadow_dir}/run-gate"
    cs_shadow_dir_w = cs_shadow_dir.replace("/", "\\")
    cs_shadow_conf = f"{cs_shadow_dir}/config.conf"
    cs_run_gate_w = (mirserver.rstrip("/\\") + "/RunGate").replace("/", "\\")
    # 本计划出处（判据脚本所在的检出）：`--judge` 必须跑**这份** `window_handoff.py`/`--check-dir`，
    # 不能跑构建源 `repo=` 里的旧副本（可能不含本片新增的 ①② 判据）。
    here_fwd = HERE.replace("\\", "/")
    wh_fwd = os.path.join(HERE, "window_handoff.py").replace("\\", "/")
    lines: list[str] = []
    A = lines.append

    A(f"# ①②③ 两侧对照窗口计划（卡 openmir2-client-e2e-enter-gaps）")
    A(f"# 构建源 repo={repo or '(未给 --repo)'} ref={ref}")
    A(f"# ③b 有怪点 site={site} (MapName={site_map}) / ③a 反向点 site_none={site_none} "
      f"(MapName={site_none_map}) / view_range={view_range}")
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
    if live_script_stale:
        _here_fwd = here_fwd
        _wh_fwd = wh_fwd
        _si = standin_script_info()
        A("# ==== 修复块：G5 RED（root 实际会跑的一键脚本 ≠ 本计划的构建源） ====")
        A(f"# root 现跑的脚本：{window_script or DEFAULT_WINDOW_SCRIPT}")
        A("# 它的 repo=/tgt= 与本计划 -repo/--target-dir 不一致（或指共享主检出）⇒ 直接跑它会构建另一棵树")
        A("# / 从旧 exe 起栈，白烧一次性窗口。**两条修法任选一条、别混着用**（混用会让本门一直 RED）；")
        A("# 每条后面各带**与它配套**的复跑命令 —— ① 用替身自己的构建源，② 才用本计划的构建源：")
        A("#   ① 换成入库替身（推荐；替身与 root 现脚本只差 repo=/tgt= 两行，bash -n OK）：")
        A(f"#        cp {_here_fwd}/{STANDIN_SCRIPT_NAME} /e/tmp/openmir2-watch/client-e2e-window.sh")
        if _si["repo"] and _si["tgt"]:
            A(f"#      替身自带构建源 repo={_si['repo']} tgt={_si['tgt']}；换完**用替身的构建源**复跑本门至 GREEN：")
            A(f"#        python -I -B {_wh_fwd} --repo {_si['repo']} --target-dir {_si['tgt']} --ref {ref}")
            A("#      （⚠ 别拿 ② 的 --repo 去复跑 ①：替身的 repo= 是它自己那份，用别的 --repo 会一直 RED）")
        else:
            A("#      ⚠ 本机取不到替身的 repo=/tgt=（文件缺失/形态变了）⇒ 先按 ㉘ 生成替身，或直接走 ② 手改。")
        A("#   ② 手改活脚本的 repo= / tgt= 两行（改成上面首行打印的构建源）；改完用**本计划构建源**复跑至 GREEN：")
        A(f"#        repo={repo or '(未给 --repo)'}   tgt='{tgt_win}'")
        A(f"#        python -I -B {_wh_fwd} --repo {repo or '(未给 --repo)'} --ref {ref}")
        A("")
    A("# 0) 前置：端口/进程（5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN；仅共享 3306 可 UP）")
    A("#    起栈禁接管道（禁 | tee/tail/head/grep，会把调用挂死）；一律 > <log> 2>&1")
    A("")
    A(f"# 0a) 构建 Rust 侧六件到 {target_dir}（必须先做：start_rust_stack.ps1 在 {target_dir}/debug 缺 exe/config 时会")
    A("#     PREREQ_MISSING exit 2 / 或在窗口里空转 —— §2 的 -RustBinDir 指向这里）")
    _build_repo = repo or "(未给 --repo)"
    A(f"#   git -C {_build_repo} rev-parse HEAD   # 记进证据（构建源 oid）")
    A(f"#   cd {_build_repo}/mir2-rs && CARGO_TARGET_DIR={target_dir} cargo build --offline \\")
    A("#     -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv")
    A("#   0a-2) 录 R7 出处清单（每次构建后必做，否则 R7 PREREQ）：")
    _rp_fwd = os.path.join(HERE, RUST_PREFLIGHT).replace("\\", "/")
    A(
        f"#   python -I -B {_rp_fwd} --repo {_build_repo} --ref HEAD "
        f"--target-dir {target_dir} --record-provenance {target_dir}/{PROVENANCE_NAME}"
    )
    if bin_missing:
        A(f"#   ⚠ 构建产物不存在：{bin_dir}/game-svr.exe（本次 --plan 时实测）⇒ 不先跑 §0a 就地起栈必 PREREQ_MISSING")
    A("")
    _bx, _by = site.split(",")[0].strip(), site.split(",")[1].strip()
    _nx, _ny = site_none.split(",")[0].strip(), site_none.split(",")[1].strip()
    _my = MYSQL_EXE
    _myargs = "-h 127.0.0.1 -P 3306 -u root --password= --batch --skip-column-names"
    A("# 1) 站位：两侧必须逐字节同坐标。real 列 = `mir2_db.characters.MapName/CX/CY`；")
    A("#    本表**没有** x/y 列（实测 2026-10-14：SHOW COLUMNS 只有 MapName/CX/CY）⇒ 旧计划的")
    A("#    `SET x=…, y=…` 必 MySQL ERROR 1054（Unknown column）且角色不动，两趟都停在旧点位 = 白烧窗口")
    A("#    （证据 135-window-plan-position-columns.txt）；G4 已加机判牙钉死。")
    A("#    ⚠ 每一趟**起栈前**都要重跑本节：上一趟客户端走动/断线存档会改写 aaa 的坐标。")
    A("#   1a. 记 before（还原用）：")
    A(f"#   {_my} {_myargs} -e \"SELECT MapName,CX,CY FROM mir2_db.characters WHERE ChrName='aaa';\"")
    A("#   1b. ③b 有怪点：")
    A(f"#   {_my} {_myargs} -e \"UPDATE mir2_db.characters SET MapName='{site_map}', CX={_bx}, CY={_by} WHERE ChrName='aaa';\"")
    A("#   1c. ③a 反向点（第二趟起栈前改）：")
    A(f"#   {_my} {_myargs} -e \"UPDATE mir2_db.characters SET MapName='{site_none_map}', CX={_nx}, CY={_ny} WHERE ChrName='aaa';\"")
    A("#   1d. 53 触发前置（片㊴/㊵ 实测：满血进图**永不发** 53 ⇒ ①② 覆盖不到；**每趟起栈前重跑**）：")
    A("#       主键/列名实测：characters_ablity.PlayerId = characters.Id；HP 在 characters_ablity.Hp / MaxHP")
    A("#       （不在 characters；见 sql/mir2_db.sql 与 82-two-side-window-runbook.md §2b）")
    A(f"#   {_my} {_myargs} -e \"SELECT c.ChrName,a.Hp,a.MaxHP FROM mir2_db.characters c JOIN mir2_db.characters_ablity a ON a.PlayerId=c.Id WHERE c.ChrName='aaa';\"")
    A(f"#   {_my} {_myargs} -e \"UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id SET a.Hp = a.MaxHP - 2 WHERE c.ChrName='aaa' AND a.MaxHP >= 3;\"")
    if site_map != "0":
        A("#   ⚠ ③b 是**跨图**点（root ①「MonGen 里密度高的图」）：选点已过 tile=Walk + clientmap=yes")
        A("#     过滤；起栈前仍须实拍客户端能进该图（MapInfo 属性 NORECONNECT/DAY 与客户端 .map 一致）。")
    A("")
    A("# 2) ③b 两侧跑（有怪点；两侧不同时起 —— 端口互斥）")
    A("#   [③b] Rust：")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 ^")
    A(f"#       -RustBinDir {bin_dir} -Seed {seed} -LogDir {rust}/stack")
    A("#     # 2a) 【起代理前必做】抓包拓扑：代理占官方 7200 转发到**影子** game-gate(7600)；")
    A("#     #     不先做 ⇒ mir2_proxy.py 绑不上 7200 ⇒ 无 capture ⇒ ③b 客户端 pairs=0/PREREQ，白烧窗口。")
    A("#     #     已证步骤：tools/m2-acceptance/evidence/m2svr-whitelist-rerun-20261012/README.md §4。")
    A(f"#     #   (本计划 bin_dir = {bin_dir})")
    A("#     #   a. 停本栈标准 game-gate 让出 7200/7201/7202（按绝对路径精确停，勿误伤同名进程）：")
    A(f"#     #        Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{bin_dir_w}\\game-gate.exe' }} |")
    A("#     #          ForEach-Object { Stop-Process -Id $_.ProcessId -Force }")
    A("#     #   b0. 造影子 gate 的 conf（从部署件拷、**GBK 原样**改 Count=1 / GatePort1=7600；两趟共用）：")
    A("#     #        ⚠ 直接照抄 `RunGate/config.conf`（Count=3）会再绑 7201/7202 ⇒ 代理抢不到 7200（白烧窗口）；")
    A("#     #        ⚠ 用 UTF-8 读写会把 `[MagicInterval]`/`[Strings]` 的中文 GBK 字节改坏 ⇒ 必须用 cp936 读写。")
    A(f"#     #        New-Item -ItemType Directory -Force '{shadow_dir}' | Out-Null")
    A(f"#     #        Copy-Item -LiteralPath '{cs_run_gate_w}\\config.conf' -Destination '{rs_gate_conf}' -Force")
    A(f"#     #        $enc = [Text.Encoding]::GetEncoding(936); $t = [IO.File]::ReadAllText('{rs_gate_conf}', $enc); $t = $t -replace '(?m)(?<=^Count=)\\d+(?=\\r?$)','1' -replace '(?m)(?<=^GatePort1=)\\d+(?=\\r?$)','7600'; [IO.File]::WriteAllText('{rs_gate_conf}', $t, $enc)")
    A("#     #        # 复核两键（应打印 Count=1 / GatePort1=7600）：")
    A(f"#     #        Select-String -Path '{rs_gate_conf}' -Pattern '^Count=|^GatePort1='")
    A("#     #   b. 起影子 game-gate 于 7600（Rust 读法 = 第 1 个位置参数是 conf 路径，`bins/game-gate/src/main.rs:81-84`）：")
    A(f"#     #        Start-Process -FilePath '{bin_dir_w}\\game-gate.exe' -ArgumentList '{rs_gate_conf}' -WorkingDirectory '{shadow_dir}'")
    A(f"#     #   c. 确认 7200 DOWN、7600 UP（tools/m2-acceptance/start_rust_stack.ps1 -Status / {mirserver}/port-status.ps1）后才起代理：")
    A(f"#     python tools/capture/mir2_proxy.py --out {rust}/proxy --map 7200=127.0.0.1:7600")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 ^")
    A(f"#       -Account mir2test -Password mir2pass -OutDir {rust}/shots -WalkClicks 0 -AutoPlaySec 12 ^")
    A(f"#       -AttackClicks 12 -RunHoldMs 500 -SkipReenter -SkipLogout -SkipTalk")
    A("#     #  -RunHoldMs 500 = 跑键探针（8 方向在**目标点**按住左键；① ② 的 `CM_RUN` 靠它；")
    A("#     #  ③b 是开阔点才跑得动，③a 城墙走道 `CM_RUN` 恒 0 ⇒ ①② 只在 ③b 趟判）")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 ^")
    A(f"#       -ProxyDir {rust}/proxy -OutDir {rust}/s2c")
    A("#     # 2b) 【本趟跑完必做 · 进 C# 趟/③a 前必做】清端口：")
    A("#     #     代理(7200) 与**手工起的**影子 game-gate(7600) **都不在** start_rust_stack 的 pid 文件里")
    A("#     #     ⇒ 它的 -Stop 不会收它们；留着 ⇒ 下一趟起栈预检报『目标端口已被监听 7200』rc=1、")
    A("#     #     C# 影子也绑不上 7600 ⇒ 再次白烧窗口。")
    A("#     #   a. 停代理（ready.flag 里就是它的 pid）：")
    A(f"#     #        $pp = Get-Content '{rust}/proxy/ready.flag' -EA SilentlyContinue; if ($pp) {{ Stop-Process -Id ([int]$pp) -Force -EA SilentlyContinue }}")
    A("#     #   b. 停影子 game-gate（按绝对路径精确停；标准 gate 已由 §2a.a 让出）：")
    A(f"#     #        Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{bin_dir_w}\\game-gate.exe' }} |")
    A("#     #          ForEach-Object { Stop-Process -Id $_.ProcessId -Force }")
    A("#     #   c. 停本栈其余（只停 pid 文件记录的）：")
    A(f"#     #        powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Stop")
    A(f"#     #   d. 断言 7200/7600 都 DOWN 再往下：{mirserver}/port-status.ps1")
    A("#   [③b] C#（先读 85-csharp-capture-topology-rootcause.md；同一条 capture 管线）：")
    A("#     # 2a.0) 造影子 RunGate = **整目录拷贝**（不是只拷 config.conf；配方见 82-two-side-window-runbook.md §4.1）：")
    A("#     #     拷贝即保留 ServerWorkThread=1（S1 §2.1 多网关错配边界）；oracle 的 conf 路径 =")
    A("#     #     `Path.Combine(AppContext.BaseDirectory, 'config.conf')`（src/GameGate/Conf/ConfigManager.cs:5；")
    A("#     #     appsettings.json 同法，Program.cs:31），且 Main 不吃 conf 参数 ⇒ 直接起")
    A(f"#     #     {cs_gate_w} 只会读回原 config（仍绑 7200）⇒ 代理绑不上，且前置门会拿着 7600 副本")
    A("#     #     误判 GREEN（false-green）。影子必须是**自己的目录 + 自己目录下的 config.conf**。")
    A(f"#     #        Remove-Item -Recurse -Force '{cs_shadow_dir}' -EA SilentlyContinue   # 幂等：③a 第二趟重跑不嵌套")
    A(f"#     #        Copy-Item -Recurse -Force '{cs_run_gate_w}' '{cs_shadow_dir}'")
    A(f"#     #        Copy-Item -Force '{rs_gate_conf}' '{cs_shadow_conf}'   # 复用 §2a.Rust b0 那份（Count=1/GatePort1=7600，GBK 原样）")
    A("#     #        # 复核三键（应打印 Count=1 / GatePort1=7600 / ServerWorkThread=1）：")
    A(f"#     #        Select-String -Path '{cs_shadow_conf}' -Pattern '^Count=|^GatePort1=|^ServerWorkThread='")
    A(f"#     python -I -B {CSHARP_PREFLIGHT} --repo {repo or '(未给 --repo)'} --mirserver {mirserver} ^")
    A(f"#       --shadow-config {cs_shadow_conf}        # 必须 GREEN 再起栈")
    A(f"#     powershell -ExecutionPolicy Bypass -File {mirserver}/start-all.ps1")
    A("#     # 2a) 【起代理前必做】抓包拓扑（同 Rust）：停标准 RunGate(7200) → 起影子 RunGate(7600)")
    A("#     #   a. 停标准 RunGate 让出 7200（按绝对路径精确停；只停真身，勿误伤影子目录里的同名进程）：")
    A(f"#     #        Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{cs_gate_w}' }} |")
    A("#     #          ForEach-Object { Stop-Process -Id $_.ProcessId -Force }")
    A("#     #   b. 起影子 RunGate 于 7600（读自己目录下的 config.conf，即上面那份）：")
    A(f"#     #        Start-Process -FilePath '{cs_shadow_dir_w}\\GameGate.exe' -WorkingDirectory '{cs_shadow_dir}'")
    A(f"#     #   c. 确认 7200 DOWN、7600 UP（{mirserver}/port-status.ps1）后才起代理占 7200")
    A(f"#     python tools/capture/mir2_proxy.py --out {csh}/proxy --map 7200=127.0.0.1:7600")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 -Account mir2test ^")
    A(f"#       -Password mir2pass -OutDir {csh}/shots -WalkClicks 0 -AutoPlaySec 12 -AttackClicks 12 -SkipReenter -SkipLogout -SkipTalk")
    A(f"#     powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 ^")
    A(f"#       -ProxyDir {csh}/proxy -OutDir {csh}/s2c")
    A("#     # 2b) 【C# 趟跑完必做】清端口并收栈：")
    A(f"#     #     ⚠ **不要**用 `{mirserver}/start-all.ps1 -Stop` 收 C# 栈 —— 它连**共享 MySQL 3306** 一起停")
    A("#     #       （Start-Stack 的 targets 含 mysqld）⇒ 与开工前『仅 3306 UP』不一致。")
    A("#     #   a. 停代理（同 §2b.a）：")
    A(f"#     #        $pp = Get-Content '{csh}/proxy/ready.flag' -EA SilentlyContinue; if ($pp) {{ Stop-Process -Id ([int]$pp) -Force -EA SilentlyContinue }}")
    A("#     #   b. 停影子 gate（C# 侧 exe = 影子目录下 GameGate.exe，按绝对路径精确停）：")
    A(f"#     #        Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{cs_shadow_dir_w}\\GameGate.exe' }} |")
    A("#     #          ForEach-Object { Stop-Process -Id $_.ProcessId -Force }")
    A("#     #   c. 停 6 个 C# 组件、**保留 MySQL 3306**（同 tools/m2-acceptance/evidence/session-lifecycle-rerun-20261012）：")
    A("#     #        Get-Process GameSvr,DBSrv,LoginSrv,LoginGate,SelGate,GameGate -EA SilentlyContinue | Stop-Process -Force")
    A(f"#     #   d. 断言 7200/7600 都 DOWN 再往下：{mirserver}/port-status.ps1")
    A("")
    A(f"# 3) ①②③b 判据（一键，用本计划出处的判据：--check-dir {here_fwd}）：")
    A(f"#   python -I -B {wh_fwd} --judge --work-dir {w} --check-dir {here_fwd}")
    A(f"#   服务端（Rust，新读数 vr/new/af）：python -I -B check_aoi_visibility_diag.py \\")
    A(f"#     --log {rust}/stack/game-svr.out.log --expect visible --min-vis 1")
    A(f"#   客户端（Rust）：python -I -B check_client_s2c_frames.py \\")
    A(f"#     --capture {rust}/s2c/capture.jsonl --min-appearance-pairs 1")
    A(f"#   ③b 两侧：python -I -B aoi_visibility_pair.py --left {rust}/s2c/capture.jsonl \\")
    A(f"#     --right {csh}/s2c/capture.jsonl --expect visible --min-pairs 1")
    A(f"#   ③b 攻击/伤害（DoD③ 第三半「能攻击造成伤害」）：python -I -B check_attack_observability.py \\")
    A(f"#     --left {rust}/s2c/capture.jsonl --right {csh}/s2c/capture.jsonl --expect any --min-hits 1")
    A("#   ①②（root 2026-10-14 15:0x 头号项）52 客户端 58B 布局 + 跑位（`CM_RUN` 需 HP>=10）：")
    A(f"#   python -I -B check_client_run_frames.py --capture {rust}/s2c/capture.jsonl \\")
    A("#     --expect-52-body-len 58 --min-run-count 1")
    A("#   ① 53 触发前置+已发+尾长（12B `TCharDesc`；前置由 §1d 压血保证）：")
    A(f"#   python -I -B check_healthspellchanged_trigger.py \\")
    A(f"#     --log {rust}/stack/game-svr.out.log --capture {rust}/s2c/capture.jsonl")
    A("")
    A(f"# 4) ③a 反向第二趟：把 §1 的 aaa 改到 site_none={site_none} 后重跑 §2，**独立目录**")
    A(f"#    （③a 根 = {w_a}；start_rust_stack.ps1 每次起栈删旧 game-svr.out.log ⇒ 不独立就覆盖 ③b 读数）")
    A("#    ⚠ 重跑前先按 §2b/§2b(C#) 清 7200/7600：代理与影子 gate 不在 start_rust_stack 的 pid 文件里，")
    A("#      `-Stop` 收不走它们（片㉕ 源码实测，见 127-window-plan-teardown.txt）")
    A(f"#    ③a Rust：§2 的 [③b] Rust 命令，把 -LogDir/--out/-OutDir 的 {rust} 换成 {rust_a}")
    A(f"#    ③a C#  ：§2 的 [③b] C# 命令，把 --out/-OutDir 的 {csh} 换成 {csh_a}")
    A(f"#   判据（一键，同 §3 的出处：--check-dir {here_fwd}）：")
    A(f"#   python -I -B {wh_fwd} --judge --work-dir {w} --pass-a-dir {w_a} --check-dir {here_fwd}")
    A(f"#   服务端：python -I -B check_aoi_visibility_diag.py \\")
    A(f"#     --log {rust_a}/stack/game-svr.out.log --expect none")
    A(f"#   ③a 两侧：python -I -B aoi_visibility_pair.py --left {rust_a}/s2c/capture.jsonl \\")
    A(f"#     --right {csh_a}/s2c/capture.jsonl --expect none")
    A(f"#   （③a 两侧都必须 0 对出现帧 + 服务端 vis==0 且 af==0）")
    A("")
    A("# 5) 收工：§2b / §2b(C#) 清代理+影子 gate；start_rust_stack 的 -Stop；6 个 C# 组件 Stop-Process（保留 3306）；")
    A("#     还原 aaa：把 §1a 打印的 before 三值填回 ⇒ UPDATE mir2_db.characters SET MapName=BEFORE_MAP, CX=BEFORE_X, CY=BEFORE_Y WHERE ChrName='aaa';")
    A("#     还原压血：把 §1d 打印的 before Hp 写回 ⇒ UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id SET a.Hp=BEFORE_HP WHERE c.ChrName='aaa';")
    A("#     实拍十端口；SHA256SUMS 按 LF/git-blob 口径")
    A("")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------- G4：计划 ↔ 工具面 机判

_SCRIPT_TOKEN_RE = re.compile(r"(?:[A-Za-z]:)?[A-Za-z0-9_./\\:-]*\.(?:ps1|py)\b")
_PS_FLAG_RE = re.compile(r"(?<![\w-])-([A-Za-z][A-Za-z0-9_]*)")
_PY_FLAG_RE = re.compile(r"(?<![\w-])--([a-z][a-z0-9-]*)")
_PS_DECL_RE = re.compile(r"\$(\w+)")
_PY_DECL_RE = re.compile(r"add_argument\(\s*['\"](--[A-Za-z0-9-]+)")
# 调用宿主 powershell 自身的开关，不属于被调脚本
_PS_HOST_FLAGS = {
    "-ExecutionPolicy", "-File", "-NoProfile", "-Command", "-EA", "-ErrorAction",
}
# 计划里必须出现的 teardown / 清端口标记（缺一 ⇒ RED，片㉕）
TEARDOWN_MARKERS = (
    "2b) 【本趟跑完必做",                                  # Rust 趟清端口段存在
    "proxy/ready.flag",                                   # 代理 pid 回收
    "停影子 game-gate（按绝对路径精确停",                 # Rust 影子 gate 回收
    "停影子 gate（C# 侧 exe",                             # C# 影子 gate 回收
    "断言 7200/7600 都 DOWN",                              # 端口回落断言
    "保留 MySQL 3306",                                    # C# 清栈不动共享 DB
)
_TOOL_DIRS = ("tools/m2-acceptance", "tools/client", "tools/capture", "tools/oracle")
# 片㉖：命令面残留占位。凡「调脚本 / 起进程 / 写文件」的行不得留 `<...>`
# （root 现场猜路径 = 烧一次性窗口；片㉕ 只禁了 `<bin_dir>` 一个，太窄）。
_CMDLINE_FACE_RE = re.compile(
    r"\.(?:ps1|py|sh)\b|Start-Process|Stop-Process|Get-Process|Get-CimInstance|"
    r"Copy-Item|Set-Content|New-Item|Select-String|\bcargo\b|mysql>"
)
_PLACEHOLDER_RE = re.compile(r"<[^<>\n]{1,80}>")


_POSITION_UPDATE_RE = re.compile(r"UPDATE\s+(\S*characters)\s+SET\s+([^;]*?)\s+WHERE", re.IGNORECASE)


def position_sql_findings(plan: str, site: str = "", site_map: str = "",
                          site_none: str = "", site_none_map: str = "") -> list[str]:
    """站位 SQL 必须写**真实列**（`MapName/CX/CY`），钉同一张图同一坐标。

    本表没有 `x`/`y` 列（实测 `SHOW COLUMNS FROM mir2_db.characters`）⇒ `SET x=…, y=…`
    会 MySQL ERROR 1054，角色不移动、两趟都停在旧点位 ⇒ 一次性窗口白烧。
    纯函数、不连库；只做「计划文本 → 列名」的静态对账（片㉗）。

    片㉛：给定选点（`site`/`site_map`、`site_none`/`site_none_map`）时，还要求命中该坐标的站位
    UPDATE 的 `MapName` **与选点同图** —— 否则 `--map-name`/`--best-site-all-maps` 选出的跨图点
    会被写成 `MapName='0'`（角色落在 0 号图 (17,21) 而不是 `D10071 (17,21)`）= 假绿 + 白烧窗口。
    """
    problems: list[str] = []
    seen = 0
    for m in _POSITION_UPDATE_RE.finditer(plan):
        seen += 1
        table = m.group(1)
        assigns = m.group(2)
        pairs: dict[str, str] = {}
        for c in assigns.split(","):
            if "=" not in c:
                continue
            k, v = c.split("=", 1)
            pairs[k.strip().strip("`").lower()] = v.strip().strip("'\"")
        lowered = set(pairs)
        if "cx" not in lowered or "cy" not in lowered:
            problems.append(
                f"站位 SQL 未用真实列 CX/CY（本表无 x/y）：{table} SET {assigns.strip()}")
        elif "x" in lowered or "y" in lowered:
            problems.append(
                f"站位 SQL 引用了不存在的列（MySQL ERROR 1054）：{table} SET {assigns.strip()}")
        if "mapname" not in lowered:
            problems.append(
                f"站位 SQL 未钉 MapName（两侧可能不同图）：{table} SET {assigns.strip()}")
            continue
        for label, coords, want in (("③b", site, site_map), ("③a", site_none, site_none_map)):
            if not coords or not want or "," not in coords:
                continue
            wx, wy = (p.strip() for p in coords.split(",", 1))
            if pairs.get("cx") == wx and pairs.get("cy") == wy and pairs["mapname"] != want:
                problems.append(
                    f"站位 SQL 的 {label} 图名与选点不一致：MapName='{pairs['mapname']}'"
                    f" != 选点图 '{want}'（把角色放到别的图 = 白烧窗口）")
    if seen == 0:
        problems.append("计划缺站位 UPDATE mir2_db.characters 段（无法把两侧钉到同一坐标）")
    return problems


def _plan_command_lines(plan: str) -> list[str]:
    """把计划文本（全注释）拆成「去 # + 拼接续行（^ / \\）」后的命令串。"""
    out: list[str] = []
    buf = ""
    for raw in plan.splitlines():
        line = raw.strip()
        while line.startswith("#"):
            line = line[1:].strip()
        if buf:
            line = (buf + " " + line).strip()
            buf = ""
        if line.endswith("^") or line.endswith("\\"):
            buf = line[:-1].rstrip()
            continue
        if line:
            out.append(line)
    if buf:
        out.append(buf)
    return out


def _resolve_tool(token: str, repo_root: str, check_dir: str, mirserver: str) -> str | None:
    cands: list[str] = []
    if os.path.isabs(token):
        cands.append(token)
    else:
        for base in (repo_root, check_dir, mirserver):
            if base:
                cands.append(os.path.join(base, token.replace("\\", "/")))
    base_name = os.path.basename(token.replace("\\", "/"))
    cands.append(os.path.join(check_dir, base_name))
    if mirserver:
        cands.append(os.path.join(mirserver, base_name))
    for d in _TOOL_DIRS:
        if repo_root:
            cands.append(os.path.join(repo_root, d, base_name))
    for c in cands:
        if c and os.path.isfile(c):
            return c
    return None


def _declared_flags(path: str) -> set[str]:
    """读出脚本声明的参数：.ps1 取 param(...) 里的 `$name`；.py 取 add_argument 的长选项。"""
    with open(path, encoding="utf-8", errors="replace") as fh:
        src = fh.read()
    if path.endswith(".py"):
        return set(_PY_DECL_RE.findall(src))
    m = re.search(r"param\s*\(", src, re.IGNORECASE)
    if not m:
        return set()
    depth = 0
    i = m.end() - 1
    while i < len(src):
        if src[i] == "(":
            depth += 1
        elif src[i] == ")":
            depth -= 1
            if depth == 0:
                break
        i += 1
    return {"-" + n for n in _PS_DECL_RE.findall(src[m.end():i])}


def plan_conformance(plan: str, repo_root: str = "", check_dir: str = HERE,
                     mirserver: str = "", site: str = "", site_map: str = "",
                     site_none: str = "", site_none_map: str = "") -> list[str]:
    """机判计划与工具面一致：脚本必须存在、参数必须被声明、teardown/清端口段必须齐。

    不复制任何其它门的逻辑；只做「字符串 → 脚本 → 声明集合」的静态对账（零起栈/零端口）。
    """
    problems: list[str] = []
    repo_root = repo_root or DEFAULT_REPO_ROOT
    declared_cache: dict[str, set[str]] = {}
    for line in _plan_command_lines(plan):
        if _CMDLINE_FACE_RE.search(line):
            for ph in _PLACEHOLDER_RE.findall(line):
                problems.append(f"计划命令面残留未替换占位 {ph}（应代入实际路径/值；勿让 root 现场猜）")
        for m in _SCRIPT_TOKEN_RE.finditer(line):
            token = m.group(0)
            base = os.path.basename(token)
            resolved = _resolve_tool(token, repo_root, check_dir, mirserver)
            if resolved is None:
                problems.append(f"计划引用不存在的脚本：{token}")
                continue
            if resolved not in declared_cache:
                declared_cache[resolved] = _declared_flags(resolved)
            declared = declared_cache[resolved]
            rest = line[m.end():]
            if base.endswith(".py"):
                used = {"--" + f for f in _PY_FLAG_RE.findall(rest)}
            else:
                used = {"-" + f for f in _PS_FLAG_RE.findall(rest)} - _PS_HOST_FLAGS
            for f in sorted(used):
                if f not in declared:
                    problems.append(f"{base} 未声明的参数：{f}")
    for marker in TEARDOWN_MARKERS:
        if marker not in plan:
            problems.append(f"计划缺 teardown/清端口段：{marker!r}")
    if "stop-all.ps1" in plan:
        problems.append("计划引用不存在的 {mirserver}/stop-all.ps1（应改为确定性清栈：6 组件 Stop-Process + 保留 3306）")
    if "<bin_dir>" in plan:
        problems.append("计划残留未替换占位 <bin_dir>（应代入 -RustBinDir 的实际目录）")
    problems.extend(position_sql_findings(plan, site, site_map, site_none, site_none_map))
    seen: set[str] = set()
    uniq: list[str] = []
    for p in problems:
        if p not in seen:
            seen.add(p)
            uniq.append(p)
    return uniq


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
    for tool in (AOI_DIAG, CLIENT_FRAMES, PAIR_TOOL, ATTACK_TOOL, RUN_FRAMES, HSC_TRIGGER):
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


def _write_fake_plan_tree(tmp: str) -> tuple[str, str, str]:
    """造一棵位置无关的「仓 + 判据目录 + 部署根」假树，供 plan_conformance 自证绿/红。

    这样 G4 的自检不依赖脚本自身所在目录（在 /tmp 复制件上跑变异时仍能得到真红/真绿）。
    """
    repo = os.path.join(tmp, "repo")
    check = os.path.join(tmp, "check")
    ms = os.path.join(tmp, "MirServer")

    def write(path: str, text: str) -> None:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)

    write(os.path.join(repo, "tools/m2-acceptance/start_rust_stack.ps1"),
          "param([string]$RustBinDir,[string]$Seed,[string]$LogDir,[switch]$Stop,[switch]$Status)\n")
    write(os.path.join(repo, "tools/m2-acceptance/export_s2c.ps1"),
          "param([string]$ProxyDir,[string]$OutDir)\n")
    write(os.path.join(repo, "tools/client/mir_flow.ps1"),
          "param([string]$Account,[string]$Password,[string]$OutDir,[int]$WalkClicks,"
          "[int]$AutoPlaySec,[int]$AttackClicks,[int]$RunHoldMs,[switch]$SkipReenter,[switch]$SkipLogout,[switch]$SkipTalk)\n")
    write(os.path.join(repo, "tools/capture/mir2_proxy.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--out')\nap.add_argument('--map')\n")
    write(os.path.join(check, "window_handoff.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--repo')\nap.add_argument('--ref')\n"
          "ap.add_argument('--judge', action='store_true')\n"
          "ap.add_argument('--work-dir')\nap.add_argument('--pass-a-dir')\n")
    write(os.path.join(check, "check_csharp_capture_preflight.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--repo')\nap.add_argument('--mirserver')\nap.add_argument('--shadow-config')\n")
    write(os.path.join(check, "check_aoi_visibility_diag.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--log')\nap.add_argument('--expect')\nap.add_argument('--min-vis', type=int)\n")
    write(os.path.join(check, "check_client_s2c_frames.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--capture')\nap.add_argument('--min-appearance-pairs', type=int)\n")
    write(os.path.join(check, "aoi_visibility_pair.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--left')\nap.add_argument('--right')\nap.add_argument('--expect')\n"
          "ap.add_argument('--min-pairs', type=int)\n")
    write(os.path.join(check, "check_attack_observability.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--left')\nap.add_argument('--right')\nap.add_argument('--capture')\n"
          "ap.add_argument('--expect')\nap.add_argument('--min-hits', type=int)\n"
          "ap.add_argument('--min-struck', type=int)\nap.add_argument('--min-kills', type=int)\n")
    write(os.path.join(check, "check_client_run_frames.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--capture')\nap.add_argument('--expect-52-body-len', type=int)\n"
          "ap.add_argument('--min-run-count', type=int)\n")
    write(os.path.join(check, "check_healthspellchanged_trigger.py"),
          "import argparse\nap=argparse.ArgumentParser()\n"
          "ap.add_argument('--log')\nap.add_argument('--capture')\n"
          "ap.add_argument('--expect-body-len', type=int)\nap.add_argument('--min-53', type=int)\n")
    for stub in ("start-all.ps1", "port-status.ps1"):
        write(os.path.join(ms, stub), "param([switch]$Stop)\n")
    return repo, check, ms


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
        == {"x": 605, "y": 578, "e": "12.681", "map": None})
    chk("parse-best-point-missing", parse_best_point("RANK (605,578) E=12.681") is None)
    chk("parse-best-point-int-e",
        parse_best_point("BEST (1,2) E=3") == {"x": 1, "y": 2, "e": "3", "map": None})
    # 片㉛：`--all-maps` 形态带图名（窗口计划 `--best-site-all-maps` 靠它回传选点图）
    chk("parse-best-point-allmaps",
        parse_best_point("BEST (D10071) (17,21) E=59.533 view_range=12 box=(17,21) range=0 "
                         "count=1 name=赤月恶魔 enterable=yes tile=Walk clientmap=yes")
        == {"x": 17, "y": 21, "e": "59.533", "map": "D10071"})
    # 选点 fail-soft：缺部署根 ⇒ 不得抛异常，返回 (None, None, 说明)
    _none_site, _none_map, _none_note = resolve_best_site(HERE, "E:/no-such-mirserver-xyz", "0", 12)
    chk("resolve-best-site-failsoft",
        _none_site is None and _none_map is None and "选点跳过" in _none_note, _none_note)

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
        # 片㉔：计划必须自带 §2a 抓包拓扑（代理占官方 7200 → 影子 game-gate 7600）；不先做 ⇒ 代理绑不上 7200
        # ⇒ 无 capture ⇒ ③b 客户端 pairs=0/PREREQ，白烧窗口。
        chk("plan-has-capture-topology",
            "起影子 game-gate 于 7600" in _build_plan and "让出 7200" in _build_plan
            and "白烧窗口" in _build_plan)
        _topo_i = _build_plan.find("起影子 game-gate 于 7600")
        _proxy_i = _build_plan.find("mir2_proxy.py --out")
        chk("plan-topology-before-proxy", 0 <= _topo_i < _proxy_i, f"topo={_topo_i} proxy={_proxy_i}")
        chk("plan-topology-shadow-conf",
            "GatePort1=7600" in _build_plan and "Count=1" in _build_plan
            and "GetEncoding(936)" in _build_plan)
        chk("plan-topology-csharp-pointer",
            "82-two-side-window-runbook.md" in _build_plan and "ServerWorkThread=1" in _build_plan)
        # 片㉖：影子 gate 的 conf 必须「造得出、路径具体、C# 走整目录拷贝」（否则 root 现场猜 = 烧窗口）
        chk("plan-shadow-conf-concrete",
            "<影子" not in _build_plan and "<conf 目录>" not in _build_plan
            and "--shadow-config E:/tmp/w/shadow/run-gate/config.conf" in _build_plan
            and "-ArgumentList 'E:/tmp/w/shadow/game-gate.conf'" in _build_plan)
        chk("plan-shadow-conf-created",
            "Copy-Item -LiteralPath 'M\\RunGate\\config.conf'" in _build_plan
            and "(?<=^Count=)\\d+(?=\\r?$)" in _build_plan
            and "[Text.Encoding]::GetEncoding(936)" in _build_plan)
        chk("plan-shadow-conf-before-start",
            0 <= _build_plan.find("Copy-Item -LiteralPath 'M\\RunGate\\config.conf'")
            < _build_plan.find("-ArgumentList 'E:/tmp/w/shadow/game-gate.conf'"))
        chk("plan-csharp-shadow-copy-dir",
            "Remove-Item -Recurse -Force 'E:/tmp/w/shadow/run-gate'" in _build_plan
            and "Copy-Item -Recurse -Force 'M\\RunGate' 'E:/tmp/w/shadow/run-gate'" in _build_plan
            and "Start-Process -FilePath 'E:\\tmp\\w\\shadow\\run-gate\\GameGate.exe'" in _build_plan
            and "AppContext.BaseDirectory" in _build_plan)
        chk("plan-csharp-shadow-not-real-exe",
            "Start-Process -FilePath 'M\\RunGate\\GameGate.exe'" not in _build_plan)

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

    # --- J1 窗口判据聚合（--judge）：八条判据消费 ③b/③a 两趟产物（含 ①②） ---
    with tempfile.TemporaryDirectory(prefix="wh-judge-") as tmp:
        cdir = os.path.join(tmp, "check")
        work = os.path.join(tmp, "work")
        _write_fake_judges(cdir)
        _write_fake_artifacts(work)
        _write_fake_artifacts(work + "-a")  # ③a 缺省根 = <work-dir>-a
        rep = run_window_judges(work, "", cdir)
        chk("judge-allgreen-rc0", rep.rc() == 0, str(rep.rc()))
        chk("judge-eight-items", len(rep.info) == 8, str(len(rep.info)))
        # ①②（root 15:0x 头号项）必须在聚合面里：drop 任一条 ⇒ 本条必红。
        _items = [name for name, *_ in window_judge_items(work, "", cdir)]
        chk("judge-ability-run-item-present",
            any("①②" in n for n in _items), " | ".join(_items))
        chk("judge-53-trigger-item-present",
            any("53 触发" in n for n in _items), " | ".join(_items))

    with tempfile.TemporaryDirectory(prefix="wh-judge-red-") as tmp:
        cdir = os.path.join(tmp, "check")
        work = os.path.join(tmp, "work")
        _write_fake_judges(cdir, {AOI_DIAG: 1})
        _write_fake_artifacts(work)
        _write_fake_artifacts(work + "-a")
        rep = run_window_judges(work, "", cdir)
        chk("judge-red-rc1", rep.rc() == 1, str(rep.rc()))
        chk("judge-red-two", len(rep.problems) == 2 and len(rep.info) == 6,
            f"{len(rep.problems)}/{len(rep.info)}")

    # ①② 各自 RED 必须传播（不能只加条目不接线：把 rc 吞掉 ⇒ 本条必红）。
    with tempfile.TemporaryDirectory(prefix="wh-judge-run-") as tmp:
        cdir = os.path.join(tmp, "check")
        work = os.path.join(tmp, "work")
        _write_fake_judges(cdir, {RUN_FRAMES: 1})
        _write_fake_artifacts(work)
        _write_fake_artifacts(work + "-a")
        rep = run_window_judges(work, "", cdir)
        chk("judge-run-red",
            rep.rc() == 1 and any("①②" in p for p in rep.problems), "; ".join(rep.problems))

    with tempfile.TemporaryDirectory(prefix="wh-judge-53-") as tmp:
        cdir = os.path.join(tmp, "check")
        work = os.path.join(tmp, "work")
        _write_fake_judges(cdir, {HSC_TRIGGER: 1})
        _write_fake_artifacts(work)
        _write_fake_artifacts(work + "-a")
        rep = run_window_judges(work, "", cdir)
        chk("judge-53-red",
            rep.rc() == 1 and any("53 触发" in p for p in rep.problems), "; ".join(rep.problems))

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

    # --- G4：计划 ↔ 工具面 机判 + teardown 清端口段（片㉕） ---
    _fresh = plan_text("R", "HEAD", "M", "605,578", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                       "42", 12, stale=False)
    chk("plan-has-teardown-proxy",
        "proxy/ready.flag" in _fresh and "Stop-Process -Id ([int]$pp)" in _fresh)
    chk("plan-has-teardown-shadow-gate",
        "停影子 game-gate（按绝对路径精确停" in _fresh and "停影子 gate（C# 侧 exe" in _fresh)
    chk("plan-has-port-assert", "断言 7200/7600 都 DOWN" in _fresh)
    chk("plan-csharp-keep-mysql",
        "保留 MySQL 3306" in _fresh and "stop-all.ps1" not in _fresh
        and "Get-Process GameSvr,DBSrv,LoginSrv,LoginGate,SelGate,GameGate" in _fresh)
    chk("plan-no-bin-dir-placeholder", "<bin_dir>" not in _fresh)
    # 片㉜：DoD③ 第三半「能攻击造成伤害」——真机跑必须带攻击步 + 攻击/伤害判据
    chk("plan-has-attack-step", "-AttackClicks 12" in _fresh)
    chk("plan-has-attack-judge",
        "check_attack_observability.py" in _fresh and "--expect any --min-hits 1" in _fresh)
    # ①②（root 2026-10-14 15:0x 头号项）：真机跑必须带跑键探针 + 压血前置 + 52/53 判据
    chk("plan-has-run-step", "-AttackClicks 12 -RunHoldMs 500" in _fresh)
    chk("plan-has-ability-run-judge",
        "check_client_run_frames.py" in _fresh and "--expect-52-body-len 58" in _fresh
        and "--min-run-count 1" in _fresh)
    chk("plan-has-53-trigger-judge",
        "check_healthspellchanged_trigger.py" in _fresh
        and "--log E:/tmp/w/rust/stack/game-svr.out.log" in _fresh)
    chk("plan-has-hp-precondition",
        "characters_ablity" in _fresh and "a.Hp = a.MaxHP - 2" in _fresh
        and "SELECT c.ChrName,a.Hp,a.MaxHP FROM mir2_db.characters c JOIN" in _fresh)
    # 片㉗：站位 SQL 必须真实列（本表无 x/y），且给出 before 探针
    chk("plan-position-sql-real-columns",
        position_sql_findings(_fresh) == []
        and "UPDATE mir2_db.characters SET MapName='0', CX=605, CY=578 WHERE" in _fresh)
    chk("plan-position-sql-before-probe",
        "SELECT MapName,CX,CY FROM mir2_db.characters WHERE ChrName='aaa'" in _fresh)
    chk("position-sql-x-y-red",
        len(position_sql_findings("UPDATE mir2_db.characters SET x=1, y=2 WHERE ChrName='aaa';")) >= 1)
    chk("position-sql-missing-mapname-red",
        len(position_sql_findings("UPDATE mir2_db.characters SET CX=1, CY=2 WHERE ChrName='aaa';")) >= 1)
    # 片㉛：给定选点时，站位 SQL 的 MapName 必须与选点同图（跨图选点写回 '0' = 假绿 + 白烧窗口）
    chk("position-sql-site-map-green",
        position_sql_findings(
            "UPDATE mir2_db.characters SET MapName='D10071', CX=17, CY=21 WHERE ChrName='aaa';",
            site="17,21", site_map="D10071") == [])
    chk("position-sql-site-map-red",
        len(position_sql_findings(
            "UPDATE mir2_db.characters SET MapName='0', CX=17, CY=21 WHERE ChrName='aaa';",
            site="17,21", site_map="D10071")) >= 1)
    chk("plan-teardown-before-csharp",
        0 <= _fresh.find("2b) 【本趟跑完必做") < _fresh.find("[③b] C#"))

    with tempfile.TemporaryDirectory(prefix="wh-conf-") as tmp:
        _repo, _chk, _ms = _write_fake_plan_tree(tmp)
        _conf_plan = plan_text(_repo, "HEAD", _ms, "605,578", "291,666",
                               "E:/tmp/w", "E:/tmp/tgt", "42", 12, stale=False)
        _probs = plan_conformance(_conf_plan, _repo, _chk, _ms)
        chk("plan-conf-green", _probs == [], "; ".join(_probs))
        _bad_no_teardown = _conf_plan.replace("proxy/ready.flag", "PROXY_READY").replace(
            "断言 7200/7600 都 DOWN", "略")
        chk("plan-conf-red-no-teardown",
            any("teardown" in p for p in plan_conformance(_bad_no_teardown, _repo, _chk, _ms)))
        _bad_stop_all = _conf_plan.replace("/start-all.ps1", "/stop-all.ps1")
        chk("plan-conf-red-missing-script",
            any("stop-all.ps1" in p for p in plan_conformance(_bad_stop_all, _repo, _chk, _ms)))
        _bad_flag = _conf_plan.replace("--min-vis 1", "--min-visibility 1")
        chk("plan-conf-red-unknown-flag",
            any("min-visibility" in p for p in plan_conformance(_bad_flag, _repo, _chk, _ms)))
        _bad_runflag = _conf_plan.replace("--min-run-count 1", "--min-runs 1")
        chk("plan-conf-red-run-flag",
            any("min-runs" in p for p in plan_conformance(_bad_runflag, _repo, _chk, _ms)),
            "; ".join(plan_conformance(_bad_runflag, _repo, _chk, _ms)))
        # 片㉖：命令面不得留 <...> 占位（root 现场猜路径 = 烧窗口）
        _bad_ph = _conf_plan.replace("--shadow-config E:/tmp/w/shadow/run-gate/config.conf",
                                     "--shadow-config <影子 conf>")
        chk("plan-conf-red-cmdline-placeholder",
            any("占位" in p for p in plan_conformance(_bad_ph, _repo, _chk, _ms)))
        # 片㉗：把真实列改回 x/y ⇒ 站位牙必红
        _bad_pos = _conf_plan.replace("SET MapName='0', CX=605, CY=578", "SET x=605, y=578")
        chk("plan-conf-red-position-columns",
            any("站位 SQL" in p for p in plan_conformance(_bad_pos, _repo, _chk, _ms)),
            "; ".join(plan_conformance(_bad_pos, _repo, _chk, _ms)))
        # 片㉛：跨图选点（--best-site-all-maps / --map-name）⇒ §1b 站位 SQL 写选点图名，G4 钉死
        _xmap_kw = dict(site="17,21", site_map="D10071", site_none="291,666", site_none_map="0")
        _xplan = plan_text(_repo, "HEAD", _ms, "17,21", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                           "42", 12, stale=False, site_map="D10071")
        chk("plan-cross-map-sql",
            "UPDATE mir2_db.characters SET MapName='D10071', CX=17, CY=21 WHERE" in _xplan
            and "跨图" in _xplan)
        _xprobs = plan_conformance(_xplan, _repo, _chk, _ms, **_xmap_kw)
        chk("plan-conf-cross-map-green", _xprobs == [], "; ".join(_xprobs))
        _bad_xmap = _xplan.replace("SET MapName='D10071', CX=17, CY=21",
                                   "SET MapName='0', CX=17, CY=21")
        chk("plan-conf-red-site-map-mismatch",
            any("图名与选点不一致" in p for p in plan_conformance(_bad_xmap, _repo, _chk, _ms,
                                                                **_xmap_kw)),
            "; ".join(plan_conformance(_bad_xmap, _repo, _chk, _ms, **_xmap_kw)))

    # --- G5：root 一键窗口脚本口径 guard（片㉝） ---
    # 解析/归一（纯函数）
    _fixed = os.path.join(HERE, "client-e2e-window.fixed.sh.txt")
    if os.path.isfile(_fixed):
        with open(_fixed, encoding="utf-8", errors="replace") as _fh:
            _fixed_text = _fh.read()
        _fixed_parsed = parse_window_script(_fixed_text)
        chk("ws-parse-fixed-repo", _fixed_parsed["repo"] == "/e/tmp/m2svr-window-origin",
            str(_fixed_parsed))
        chk("ws-parse-fixed-tgt", _fixed_parsed["tgt"] == "E:\\tmp\\m2svr-window-target",
            str(_fixed_parsed))
        chk("ws-norm-msys", _norm_path("/e/tmp/x") == _norm_path("E:\\tmp\\x") == "e:/tmp/x")
        chk("ws-norm-quoted", _norm_path("'E:\\tmp\\x'".strip("'")) == "e:/tmp/x")
    chk("ws-parse-missing", parse_window_script("no assignments here") == {"repo": None, "tgt": None})
    # 片㉞（reviewer-5 ㉝ O-1）：重复键取**最后一条**（对齐 bash last-wins），不是第一条。
    # 旧实现取 /good，而 bash 会跑 /bad ⇒ G5 会朝假绿判（fail-open）。
    _dup = parse_window_script("repo=/good\nrepo=/bad\ntgt=/t1; tgt=/t2\n")
    chk("ws-parse-dup-last-wins", _dup["repo"] == "/bad" and _dup["tgt"] == "/t2", str(_dup))
    # G5 四种：一致 GREEN / repo 不一致 RED / 指共享主检出 RED / 文件缺失跳过
    with tempfile.TemporaryDirectory(prefix="wh-g5-") as tmp:
        _write_fake_tools(tmp, "GREEN")
        ws_ok = os.path.join(tmp, "win-ok.sh")
        with open(ws_ok, "w", encoding="utf-8") as fh:
            fh.write("w=/e/tmp/w; repo=R\ntgt='E:\\tmp\\tgt'; logdir='E:\\tmp\\l'\n")
        rep_ok = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                            with_csharp=False, rank=0, view_range=12, map_name="0",
                            target_dir="E:/tmp/tgt", window_script=ws_ok)
        chk("g5-match-rc0", rep_ok.rc() == 0 and not rep_ok.live_script_stale, str(rep_ok.problems))
        ws_other = os.path.join(tmp, "win-other.sh")
        with open(ws_other, "w", encoding="utf-8") as fh:
            fh.write("w=/e/tmp/w; repo=/e/other\ntgt='E:\\tmp\\tgt'\n")
        rep_other = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                               with_csharp=False, rank=0, view_range=12, map_name="0",
                               target_dir="E:/tmp/tgt", window_script=ws_other)
        chk("g5-repo-mismatch-red",
            rep_other.rc() == 1 and rep_other.live_script_stale
            and any("G5" in p for p in rep_other.problems), "; ".join(rep_other.problems))
        ws_shared = os.path.join(tmp, "win-shared.sh")
        with open(ws_shared, "w", encoding="utf-8") as fh:
            fh.write("w=/e/tmp/w; repo=/e/Users/gxh/Documents/GitHub/OpenMir2\n"
                     "tgt='E:\\tmp\\tgt'\n")
        rep_shared = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                                with_csharp=False, rank=0, view_range=12, map_name="0",
                                target_dir="E:/tmp/tgt", window_script=ws_shared)
        chk("g5-shared-checkout-red",
            rep_shared.rc() == 1 and any("共享主检出" in p for p in rep_shared.problems),
            "; ".join(rep_shared.problems))
        ws_tgt = os.path.join(tmp, "win-tgt.sh")
        with open(ws_tgt, "w", encoding="utf-8") as fh:
            fh.write("w=/e/tmp/w; repo=R\ntgt='E:\\tmp\\merge-target'\n")
        rep_tgt = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                             with_csharp=False, rank=0, view_range=12, map_name="0",
                             target_dir="E:/tmp/tgt", window_script=ws_tgt)
        chk("g5-tgt-mismatch-red",
            rep_tgt.rc() == 1 and any("target-dir" in p for p in rep_tgt.problems),
            "; ".join(rep_tgt.problems))
        rep_nofile = run_checks(tmp, repo="R", ref="HEAD", mirserver="M", shadow_config="",
                                with_csharp=False, rank=0, view_range=12, map_name="0",
                                target_dir="E:/tmp/tgt",
                                window_script=os.path.join(tmp, "nope.sh"))
        chk("g5-missing-skip",
            rep_nofile.rc() == 0 and not rep_nofile.live_script_stale
            and any("G5 跳过" in i for i in rep_nofile.info), "; ".join(rep_nofile.info))
        # 计划：G5 RED 时必须有替身修复块；G5 绿时不得出现。
        _g5_plan = plan_text("R", "HEAD", "M", "287,620", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                             "42", 12, stale=False, live_script_stale=True, window_script="/e/tmp/x.sh")
        _g5_plan_ok = plan_text("R", "HEAD", "M", "287,620", "291,666", "E:/tmp/w", "E:/tmp/tgt",
                                "42", 12, stale=False, live_script_stale=False)
        chk("plan-g5-remediation",
            "G5 RED" in _g5_plan and "client-e2e-window.fixed.sh.txt" in _g5_plan)
        chk("plan-g5-no-remediation", "G5 RED" not in _g5_plan_ok)
        # 片㊱：修复块两条路各配各的复跑命令 —— ①「换替身」必须用**替身自己的** repo/tgt
        # （拿本计划的 --repo 去复跑 ① ⇒ `cp` 换完替身后 G5 仍 RED，修复块自相矛盾/死循环）；
        # ②「手改活脚本」才用本计划的 --repo。两行都在、且不得互相串。
        _si = standin_script_info()
        chk("plan-g5-standin-recheck",
            bool(_si["repo"] and _si["tgt"])
            and f"--repo {_si['repo']} --target-dir {_si['tgt']} --ref HEAD" in _g5_plan,
            f"standin={_si}")
        chk("plan-g5-edit-recheck", "--repo R --ref HEAD" in _g5_plan)
        # 死循环守卫：① 的复跑行（= 含替身 repo 且是 `--repo` 复跑命令的那行）必须带 --target-dir。
        _standin_lines = [l for l in _g5_plan.splitlines()
                          if _si["repo"] and _si["repo"] in l and "--repo" in l]
        chk("plan-g5-standin-recheck-distinct",
            bool(_standin_lines) and all("--target-dir" in l for l in _standin_lines),
            " | ".join(_standin_lines))

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
    ap.add_argument("--plan", action="store_true", help="打印 ①②③ 两侧对照窗口计划")
    ap.add_argument("--plan-only", action="store_true", help="只打印计划，不跑判据")
    ap.add_argument("--site", default="287,620", help="③b 有怪点 x,y（默认 287,620，片⑦已证 13 对）")
    ap.add_argument("--best-site", action="store_true",
                    help="③b site 由 MonGen 排行自动选取（--best-point；root ③①「换怪密点」）")
    ap.add_argument("--best-site-all-maps", action="store_true",
                    help="③b site 跨全部地图自动选取（--all-maps --best-point；root ①「密度高的图」）")
    ap.add_argument("--client-map-dir", default="D:/MirClient-run/Map",
                    help="冻结客户端 Map 目录（选点 clientmap 过滤；缺目录 ⇒ 如实登记不静默）")
    ap.add_argument("--site-none", default="291,666", help="③a 反向点 x,y（默认 291,666，E=0.000）")
    ap.add_argument("--site-none-map", default="0", help="③a 反向点逻辑图名（默认 0）")
    ap.add_argument("--work-dir", default="E:/tmp/window/cliente2e", help="窗口工作目录")
    ap.add_argument("--pass-a-dir", default="", help="③a 第二趟证据根（默认 = <work-dir>-a）")
    ap.add_argument("--judge", action="store_true",
                    help="聚合 ③b/③a 两趟窗口产物的八条判据（①② 能力布局/跑/53 + ③ AOI；消费既有 capture/log，不起栈）")
    ap.add_argument("--target-dir", default="E:/tmp/m2svr-window-target",
                    help="CARGO_TARGET_DIR（RustBinDir = <target-dir>/debug）")
    ap.add_argument("--window-script", default=DEFAULT_WINDOW_SCRIPT,
                    help="root 实际会跑的一键窗口脚本（G5；默认 E:/tmp/openmir2-watch/client-e2e-window.sh；"
                         "传空串 '' 显式关掉 G5）")
    ap.add_argument("--shared-checkout", default=SHARED_MAIN_CHECKOUT_DEFAULT,
                    help="共享主检出路径（G5：窗口脚本 repo= 不得指它）")
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
                "window_judge": label,
                "aoi_gap3": label,
                "rc": rc,
                "work_dir": args.work_dir,
                "pass_a_dir": args.pass_a_dir or (args.work_dir.rstrip("/\\") + "-a"),
                "problems": rep.problems,
                "prereq": rep.prereq,
                "info": rep.info,
            }, ensure_ascii=False))
        else:
            print(f"WINDOW_JUDGE={label}")
            print(f"AOI_GAP3={label}")
        return rc

    site = args.site
    site_map = args.map_name
    site_none_map = args.site_none_map
    site_note = ""
    if args.best_site or args.best_site_all_maps:
        auto_site, auto_map, site_note = resolve_best_site(
            args.check_dir, args.mirserver, args.map_name, args.view_range,
            all_maps=args.best_site_all_maps,
            map_dir=os.path.join(args.mirserver, "M2GameSvr", "Map"),
            client_map_dir=args.client_map_dir)
        if auto_site:
            site = auto_site
            site_map = auto_map or args.map_name

    if args.plan_only:
        bin_missing = not os.path.isfile(os.path.join(args.target_dir, "debug", "game-svr.exe"))
        _wp, _wq, _wi, _wstale = window_script_findings(
            args.window_script, args.repo, args.target_dir, args.shared_checkout)
        plan = plan_text(args.repo, args.ref, args.mirserver, site, args.site_none,
                         args.work_dir, args.target_dir, args.seed, args.view_range,
                         stale=False, site_note=site_note, pass_a_dir=args.pass_a_dir,
                         bin_missing=bin_missing, site_map=site_map,
                         site_none_map=site_none_map, live_script_stale=_wstale,
                         window_script=args.window_script)
        print(plan)
        for line in _wi:
            print(line)
        for line in _wp:
            print("RED " + line)
        for line in _wq:
            print("PREREQ " + line)
        _conf = plan_conformance(plan, args.repo, args.check_dir, args.mirserver,
                                 site=site, site_map=site_map,
                                 site_none=args.site_none, site_none_map=site_none_map)
        for p in _conf:
            print("RED " + p)
        _label = "RED" if _conf else "GREEN"
        if args.json:
            print(json.dumps({"plan_conformance": _label, "problems": _conf,
                              "rc": 1 if _conf else 0}, ensure_ascii=False))
        else:
            print(f"PLAN_CONFORMANCE={_label}")
        return 1 if _conf else 0

    rep = run_checks(args.check_dir, args.repo, args.ref, args.mirserver,
                     args.shadow_config, args.with_csharp, args.rank, args.view_range, args.map_name,
                     target_dir=args.target_dir, window_script=args.window_script,
                     shared_checkout=args.shared_checkout)

    for line in rep.info:
        print(line)
    for line in rep.problems:
        print("RED " + line)
    for line in rep.prereq:
        print("PREREQ " + line)

    conf_label = ""
    if args.plan:
        print("")
        bin_missing = not os.path.isfile(os.path.join(args.target_dir, "debug", "game-svr.exe"))
        plan = plan_text(args.repo, args.ref, args.mirserver, site, args.site_none,
                         args.work_dir, args.target_dir, args.seed, args.view_range,
                         stale=rep.build_source_stale, site_note=site_note, pass_a_dir=args.pass_a_dir,
                         bin_missing=bin_missing, site_map=site_map,
                         site_none_map=site_none_map, live_script_stale=rep.live_script_stale,
                         window_script=args.window_script)
        print(plan)
        conf_probs = plan_conformance(plan, args.repo, args.check_dir, args.mirserver,
                                      site=site, site_map=site_map,
                                      site_none=args.site_none, site_none_map=site_none_map)
        for p in conf_probs:
            print("RED " + p)
        conf_label = "RED" if conf_probs else "GREEN"
        print(f"PLAN_CONFORMANCE={conf_label}")
        rep.problems.extend(conf_probs)

    rc = rep.rc()
    ready = "YES" if rc == 0 else "NO"
    if args.json:
        print(json.dumps({
            "window_ready": ready,
            "rc": rc,
            "result": "GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED"),
            "build_source_stale": rep.build_source_stale,
            "live_script_stale": rep.live_script_stale,
            "window_script": args.window_script,
            "problems": rep.problems,
            "prereq": rep.prereq,
            "info": rep.info,
            "plan_conformance": conf_label or None,
        }, ensure_ascii=False))
    else:
        print(f"WINDOW_READY={ready}")
        print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
