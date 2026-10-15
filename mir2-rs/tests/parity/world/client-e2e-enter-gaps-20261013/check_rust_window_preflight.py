#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_rust_window_preflight.py —— 卡 `openmir2-client-e2e-enter-gaps` DoD③
**Rust 侧窗口前置门**（只读、零端口、零起栈）。是片⑭ `check_csharp_capture_preflight.py`
（C# 侧）的对称件：那份判 C# 部署/网关拓扑，本份判**Rust 侧从哪棵树构建、树里有没有 ③ 判据要的读数**。

为什么需要（2026-10-14 实测到的真缺陷）：
  root 的窗口脚本 `E:/tmp/openmir2-watch/client-e2e-window.sh` 里
      repo=/e/Users/gxh/Documents/GitHub/OpenMir2
      ( cd "$repo/mir2-rs" && CARGO_TARGET_DIR=... cargo build ... )
  **构建源 = 共享主检出**。该检出当时停在 `ad0592b19`（`origin/master` 的祖先、但**不含**片⑪
  `d16b5199d`）⇒ `main.rs` 里 `SessionAoiDiag` 仍是旧版式
      `[{name}({aid}) map={} vis={} same_map={} nearest={}]`
  而 `check_aoi_visibility_diag.py` 的 ① 版式门**要求** `vr=/new=/af=`
  （片⑪ `d16b5199d`：`vis` 语义 = 当前可见集 `Show|Invisible` 不含自身，新增 `vr/new/af`）
  ⇒ 直接跑窗口会得到 `PREREQ_RED`，白烧一个独占窗口。

本工具把窗口前的 Rust 侧静态前置做成机判（fail-closed），四组判据：
  R1 **构建源含全部必需修复提交**：对每个 `<fix-commit>` 跑
     `git -C <repo> merge-base --is-ancestor <fix-commit> <ref>`；默认五个——片⑪ `d16b5199d`
     （推出 ③ 判据读数 `vr/new/af`）、片㉜ `0d0d62a69`（受击帧 `SM_STRUCK` 按 oracle `SendRefMsg`
     广播，DoD③ 第三半「能攻击造成伤害」必需），以及 **HIGHHP 家族三帧的客户端布局修复**：
     片㊴ `c61b9627a`（53 `TCharDesc` 尾）/ 片㊼ `6cadc9fbf`（52 全调用点）/ 片 `fd68e037d`（31）。
     后三片是「HUD `0/0` + `CM_RUN=0`」**真凶的修复**（触发 = 进图 `HP<MaxHP` 时发 53；缺尾 ⇒
     HIGHHP 客户端把 HP 解成 0）；缺任一片都会让窗口测到**修复前镜像** —— root 2026-10-14 15:0x
     的实测失败即此：源 `8e491f81e` 含片⑪/㉜ 但缺 53 尾修，旧的两片集判 R1 **GREEN**。
     `--fix-commit` 可重复覆盖（单值 = 只查那一个，向后兼容旧调用）；`ref` 默认 `HEAD`。
     只含片⑪、缺片㉜ 的专用源（实测 `m2svr-window-origin @6dba8719d`）必须判 RED —— 这正是
     「窗口构建源过期」的 fail-closed 门。
  R2 **源码含 ③ 读数标记**：`<repo>/mir2-rs/bins/game-svr/src/main.rs` 必须同时含
     版式字面量 `"vr={} vis={} new={} af={}"` 与可见集判据 `!= mir2_world::VisibleFlag::Hidden`
     （旧版 `== VisibleFlag::Show` 只数「本轮新增」，是 root 误读的根因，`70-aoi-visibility-verdict.txt` §A）。
  R3 **`map_id` ↔ MapInfo 逻辑名映射**：`Envir/MapInfo.txt` 的 section 顺序就是 `map_id`
     （`crates/envir/src/registry.rs` / oracle `Maps.LoadMapInfo`）。真机周期行打的是 `map=26`，
     而 MonGen 判据按**逻辑名** `0` ⇒ 必须钉住「第 26 个 section 的逻辑名 == 0」，
     否则 `check_mongen_dense_points.py --map 0` 判的可能是另一张图（2026-10-14 复核实测 = 命中）。
  R4 **窗口坐标与 MonGen 生成框一致**：对每个 `x,y:inside|outside` 调用
     `check_mongen_dense_points.py --point x,y --expect ...`，退出码必须为 0。
  R5 **判据工具齐备**：`check_mongen_dense_points.py` / `check_aoi_visibility_diag.py` /
     `aoi_visibility_pair.py` / `check_client_s2c_frames.py` 必须在证据目录里（缺 ⇒ PREREQ_RED）。
  R6 **构建源必须是当前基线**（片㊲，默认基线 `origin/master`）：只含两片修复**不等于**「源是基线」——
     `R1` 是 `--is-ancestor` 单向门，一份停在更早 master 的旧源（例如刷源后 master 又前进了 2 条）
     仍会 GREEN，窗口就会**测旧 exe**。R6 补上另一半：`--ref` 要么 == 基线，要么是基线的后继；
     否则必须 `git diff --name-only <基线> <ref> -- <生产路径>` 为空（非生产面差异可接受）。
     生产路径 = `mir2-rs/bins` / `mir2-rs/crates` / `mir2-rs/Cargo.toml` / `mir2-rs/Cargo.lock` / `src` / `sql`
     （与合并门禁同口径）。基线解析不到 ⇒ PREREQ_RED（fail-closed）。`--baseline-ref ''` 显式跳过
     （只给无 remote 的离线自测用，**不是**窗口口径）。
  R7 **预构建二进制 ↔ 构建源出处**（片㊻，收 `175` §F 已登记的 R6 界限）：R6 比的是**生产路径的
git 差异**，不是「`<target-dir>/debug` 里的 exe 与源一一对应」——源刷了但**没重编译**（或暖 target
里是别处拷来的旧件）时 R6 仍 GREEN，窗口照样从旧 exe 起栈（2026-10-14 实测：专用源刷到 master 后
若只 `checkout` 不 `cargo build`，`game-svr.exe` 仍是上一版）。R7 用**出处清单**（`--record-provenance`
录制的 JSON：`source_head` + 六件 exe 的 sha256/字节）做确定性对账：`source_head` 必须 == 当前 `--ref`，
且每件 exe 的 sha256 必须与清单逐字节相等；缺件/缺清单/清单与源不一致 ⇒ RED/PREREQ（fail-closed）。
  R8 **窗口槽位空闲**（片60，收「`WINDOW_READY=YES` 却仍跑着修复前栈」这个假绿）：R1..R7 只看
     **源/清单/exe 文件**，不看**有没有别的栈正占着游戏端口**。2026-10-14 15:0x 的「回归」复现根因
     即此：泄漏栈 `game-svr.exe`（`E:/tmp/openmir2-merge-target/debug`，修复前 exe）仍在
     LISTENING 5000/5100/5500/5600/5700/6000/7000/7100/7200 ⇒ 窗口起不来或从旧 exe 抓包
     （HUD `0/0` + `CM_RUN=0`）。R8 是 fail-closed：十个游戏端口（3306 共享 MySQL **不在门内**）
     必须全 DOWN；任何在跑的 `game-svr.exe` 必须是本计划 `--target-dir/debug/game-svr.exe`。
     探不动本机（非 Windows / 无 PowerShell）⇒ INFO 跳过（干净检出本来就没有栈）。
     **片63（停栈目录校验）**：`start_rust_stack.ps1 -Stop` 只读自己 `-LogDir` 下的
     `rust-stack.pid.json` ⇒ 目录选错就是静默 no-op（`NOTHING_TO_STOP`）。旧版把
     `E:/tmp/client-e2e/stack2` **写死**，换个窗口目录的泄漏栈时操作者会误以为已停干净、
     随后起窗又撞上旧栈（= 再测修复前镜像）。现在 `--slot-log-dir`（默认仍是 `stack2`，向后兼容）
     下的 pid 文件必须**持有全部占用端口的 pid**，否则另加 RED 且**不打印**任何像「照抄即可」的命令。
     自测/证据复算可用 `--slot-probe-json` 注入合成探针（会打 `WARN`，窗口机上禁用）。
用法：
  python -I -B check_rust_window_preflight.py \
      --repo E:/tmp/m2svr-window-origin [--ref HEAD] \
      [--baseline-ref origin/master] \
      --mirserver E:/MirServer \
      --target-dir E:/tmp/m2svr-window-target \
      [--provenance <target-dir>/window-provenance.json] \
      --points "291,666:outside;291,651:outside;289,618:inside;287,620:inside" \
      [--fix-commit <oid>] [--map-name 0] [--map-id 26]
  # 录出处清单（每次 `cargo build` 之后跑；写进 target-dir）
  python -I -B check_rust_window_preflight.py --repo <源> --ref HEAD \
      --target-dir E:/tmp/m2svr-window-target \
      --record-provenance E:/tmp/m2svr-window-target/window-provenance.json
  python -I -B check_rust_window_preflight.py --selftest

退出码：`0` = GREEN、`1` = RED（判据不符）、`2` = PREREQ_RED（缺输入/解析不到 git）。
`--selftest` 不依赖外部件（临时 git 仓 + 合成 main.rs/MapInfo + 合成 mongen 检查器）。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile

# 片⑪：`fix(game-svr): AOI 诊断 vis 语义（当前可见集，不含自身）+ vr/new/af 读数`
FIX_COMMIT_DIAG = "d16b5199d98029081daccd9252f81b94a9596eaf"
# 片㉜：`fix(game-svr): 受击帧按 oracle SendRefMsg(RM_STRUCK) 广播给视野内全体 Play`
#   —— DoD③ 第三半「屏幕见怪并能攻击造成伤害」的判据（`check_attack_observability.py`）要靠它。
FIX_COMMIT_STRUCK = "0d0d62a695082730dd2398e6d35bf40c8f70480a"
# 片㊴：`fix(game-svr): 片㊴ 53 帧 SM_HEALTHSPELLCHANGED 补冻结客户端 TCharDesc 尾`
#   —— 卡级「HUD 0/0 + 跑=0」的**真凶修复**（触发 = 进图 `HP<MaxHP` 时发 53；缺尾 ⇒ HIGHHP 客户端解成 0）。
FIX_COMMIT_53_TAIL = "c61b9627a1cad79f300b0efab184ec1dd1eec017"
# 片㊼：`fix(game-svr): 片㊼ SM_ABILITY(52) 生产调用点布局全覆盖`（入图/升级/装备卸下全走冻结客户端 58B）。
FIX_COMMIT_52_LAYOUT = "6cadc9fbf9004198a0ce02c2452cb7851e0bc488"
# 片「SM_STRUCK(31) 客户端布局」：HIGHHP 家族第三帧闭环（收 `213-highhp-family-audit.txt` §4 GAP）。
FIX_COMMIT_STRUCK_LAYOUT = "fd68e037db0c68e00774b0c3ab4b1eeda29af051"
# R1 默认要求源含五片（③ 读数 + 受击广播 + HIGHHP 家族三帧）；`--fix-commit` 可重复覆盖。
REQUIRED_FIX_COMMITS: tuple[tuple[str, str], ...] = (
    (FIX_COMMIT_DIAG, "片⑪ AOI 诊断 vr/new/af"),
    (FIX_COMMIT_STRUCK, "片㉜ 受击帧 SM_STRUCK 广播"),
    (FIX_COMMIT_53_TAIL, "片㊴ 53 SM_HEALTHSPELLCHANGED TCharDesc 尾"),
    (FIX_COMMIT_52_LAYOUT, "片㊼ 52 SM_ABILITY 调用点布局全覆盖"),
    (FIX_COMMIT_STRUCK_LAYOUT, "片31 SM_STRUCK 客户端布局（HIGHHP 家族）"),
)
# 向后兼容旧签名（单值默认）。
FIX_COMMIT_DEFAULT = FIX_COMMIT_DIAG
# 新周期行版式（`main.rs::SessionAoiDiag::session_summary`）
DIAG_FORMAT_MARKER = "vr={} vis={} new={} af={}"
# 可见集过滤（`Show|Invisible` == `!= Hidden`）；旧版是 `== VisibleFlag::Show`
VIS_FILTER_MARKER = "!= mir2_world::VisibleFlag::Hidden"
# R5 判据工具（相对证据目录）
REQUIRED_TOOLS = (
    "check_mongen_dense_points.py",
    "check_aoi_visibility_diag.py",
    "aoi_visibility_pair.py",
    "check_client_s2c_frames.py",
)
MAIN_RS_REL = os.path.join("mir2-rs", "bins", "game-svr", "src", "main.rs")
# R6 生产路径（与合并门禁同口径：只有这些路径上的差异才算「构建源并非当前基线」）。
PRODUCTION_PATHS = (
    "mir2-rs/bins",
    "mir2-rs/crates",
    "mir2-rs/Cargo.toml",
    "mir2-rs/Cargo.lock",
    "src",
    "sql",
)
# R6 基线 ref（默认 origin/master；空串 ⇒ 显式跳过，仅用于无 remote 的离线自测）。
BASELINE_REF_DEFAULT = "origin/master"
# R7 只关心「会影响这六个 Rust exe 的源」：清单 `source_head` 与当前 ref 若只差在这些路径之外，
# exe 逐字节不变（cargo 不会重链）⇒ 不能判 RED（否则 master 前进一条非生产提交就假红）。
# 不用 R6 的 `PRODUCTION_PATHS`（它含 `src`/`sql`，包括会变的 `src/BotSrv` —— 那不影响这六个 exe）。
RUST_BUILD_PATHS = (
    "mir2-rs/bins",
    "mir2-rs/crates",
    "mir2-rs/Cargo.toml",
    "mir2-rs/Cargo.lock",
)
# R7 窗口二进制（= `client-e2e-window.sh` 第 2 步 `cargo build` 的 `-p` 列表，逐字同源）。
WINDOW_BINS: tuple[str, ...] = (
    "game-svr.exe",
    "game-gate.exe",
    "login-srv.exe",
    "login-gate.exe",
    "sel-gate.exe",
    "db-srv.exe",
)
# R7 出处清单（provenance manifest）默认落点（放 target-dir 根）。
PROVENANCE_NAME = "window-provenance.json"
PROVENANCE_VERSION = 1
# R8 窗口槽位（片60）：九个 Rust 栈游戏端口；3306 是**共享 MySQL**（必须常开）⇒ 不在门内。
GAME_PORTS: tuple[int, ...] = (5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200)
# R8 停泄漏栈（片63）：`-LogDir` 必须 = 持有占用端口的 `rust-stack.pid.json` 所在目录。
# 旧版把 `E:/tmp/client-e2e/stack2` **写死** —— 换一台/L 另一窗口目录的泄漏栈时，照抄该命令会
# `NOTHING_TO_STOP`（`start_rust_stack.ps1 -Stop` 只读自己 `-LogDir` 下的 pid 文件），操作者会误以为
# 已停干净、随后起窗又撞上旧栈 ⇒ 再次测修复前镜像（正是 2026-10-14 15:0x 假回归的形态）。
SLOT_LOG_DIR_DEFAULT = "E:/tmp/client-e2e/stack2"
SLOT_PID_FILE_NAME = "rust-stack.pid.json"


def slot_stop_command(slot_log_dir: str) -> str:
    """停栈命令（`<源>` = 当前 `--repo`）；`slot_log_dir` 由调用方按**校验后**的目录传入。"""
    return (
        "powershell -NoProfile -ExecutionPolicy Bypass -File "
        "<源>/tools/m2-acceptance/start_rust_stack.ps1 "
        f"-LogDir {slot_log_dir} -Stop"
    )


# R7 恢复命令（片64）：与 R8 的 `PREFLIGHT_STOP_CMD` 对等 —— R7 的 RED/PREREQ 必须自带**可照抄**的
# 恢复动作，否则一次 R7 RED 就把开窗卡死。
PROVENANCE_RECORD_CMD_MARK = "PREFLIGHT_RECORD_CMD:"
CHECKER_SELF_NAME = "check_rust_window_preflight.py"
# R7 恢复命令的 `-p` 列表 = 窗口脚本第 2 步逐字同源（与 `WINDOW_BINS` 一一对应）。
WINDOW_BUILD_PKGS: tuple[str, ...] = (
    "game-svr",
    "game-gate",
    "login-srv",
    "login-gate",
    "sel-gate",
    "db-srv",
)


def provenance_record_tail(repo: str, target_dir: str, provenance_path: str) -> str:
    """R7 恢复命令的 **argv 尾**（`python -I -B <checker>` 之后的参数，已按 shell 词法引号化）。

    **片65（reviewer-5 F-1 返工）**：`--record-provenance` 在 argparse 里是**带值**选项
    （`main()` 拿它当输出路径），而片64 打印的命令只写了 `--record-provenance`（**没给值**）
    ⇒ 操作员逐字节照抄得到 `error: argument --record-provenance: expected one argument`（rc=2），
    「可照抄」的 DoD 不成立。故尾部由本函数**单一来源**产出（与工具 `--help` 的
    `--record-provenance <path>` 逐字一致），`provenance_record_command()` 只加前半段；
    selftest 再把该尾按 shell 词法切开喂回 `build_parser().parse_args(...)` 做**执行性**牙。
    """
    return (
        f'--repo "{repo}" --ref HEAD --target-dir "{target_dir}" '
        f'--record-provenance "{provenance_path}"'
    )


def provenance_record_command(repo: str, target_dir: str, provenance_path: str) -> str:
    """R7 恢复命令（片64；片65 修 reviewer-5 F-1 的不可执行）。

    **先重编六件 exe，编译成功才重录出处清单**。

    为什么需要（2026-10-15 实测到的操作缺口）：R8 的 RED 自带可照抄的停栈命令
    （`PREFLIGHT_STOP_CMD`，片63），而 R7 的 RED 只有一句散文「先重编译 + 重录清单」；
    操作员得回翻证据包才拿到确切命令，而 `client-e2e-window.fixed.sh.txt start` 又在
    `run_guard || exit 1` 处停住（**不会**替操作员编译）⇒ 一次 R7 RED（源前移且 `mir2-rs/**`
    有生产差异、或暖 target 里是别处拷来的旧件）就把开窗卡死。

    **fail-closed 要点**：`record_provenance` 只是把**当前** `--ref` 盖到清单上，它**不校验**
    exe 是否真由该 ref 编出（这是该子命令的信任边界）⇒ 「编译失败也录制」会把旧件洗成新源。
    故命令用 shell 的 `&&` 把录制**门在编译成功之后**（`cargo build` 非零 ⇒ 整条链短路，
    清单保持原样 ⇒ R7 继续 RED，不会假绿）。

    形态 = **bash 单行**（`&&` 链）：窗口脚本与 root 的手册命令都在 MSYS bash 里执行，
    `cd /e/tmp/...` / `cd E:/tmp/...` 两种写法都能用；路径加双引号以容忍空格。
    尾部由 `provenance_record_tail()` 单一来源产出（与工具的 `--help` 用法逐字一致：
    `--record-provenance <path>`，**带值**）。
    """
    checker = os.path.join(os.path.dirname(os.path.abspath(__file__)), CHECKER_SELF_NAME)
    pkgs = " ".join(f"-p {p}" for p in WINDOW_BUILD_PKGS)
    return (
        f'cd "{repo}/mir2-rs" && CARGO_TARGET_DIR="{target_dir}" '
        f"cargo build --offline {pkgs} && "
        f'python -I -B "{checker}" {provenance_record_tail(repo, target_dir, provenance_path)}'
    )


HASH_CHUNK = 1024 * 1024
# MapInfo 逻辑名分隔符（`crates/envir/src/mapinfo.rs:28`：空格/逗号/制表）
_MAPINFO_SEPS = " \t,"
SECTION_RE = re.compile(r"^\s*\[([^\]]*)\]")


def decode_text(raw: bytes) -> str:
    """与 `crates/envir` 的 `decode_text` 同口径（部署件是 GBK；失败不抛）。"""
    for enc in ("gbk", "utf-8"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("gbk", errors="replace")


def find_repo_root(start: str) -> str | None:
    """从 `start` 向上找第一个含 `.git`（目录或 worktree 文件）的目录。"""
    cur = os.path.abspath(start)
    while True:
        if os.path.exists(os.path.join(cur, ".git")):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            return None
        cur = parent


def git(repo: str, *args: str) -> tuple[int, str]:
    """跑只读 git 子命令；返回 `(rc, 合并后的输出)`。"""
    try:
        proc = subprocess.run(
            ["git", "-C", repo, *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=30,
        )
    except (OSError, subprocess.SubprocessError) as exc:  # git 不在 PATH / 超时
        return 127, f"{type(exc).__name__}: {exc}"
    return proc.returncode, decode_text(proc.stdout).strip()


def is_ancestor(repo: str, commit: str, ref: str) -> tuple[bool | None, str]:
    """`git merge-base --is-ancestor`；`None` = 判不了（缺提交/缺 ref/不是仓）。"""
    rc, out = git(repo, "merge-base", "--is-ancestor", commit, ref)
    if rc == 0:
        return True, ""
    if rc == 1:
        return False, ""
    return None, out or f"rc={rc}"


def required_fix_findings(
    repo: str, ref: str, fixes: "list[tuple[str, str]] | tuple[tuple[str, str], ...]"
) -> tuple[list[str], list[str], list[str]]:
    """R1 的纯逻辑：对每个必需修复提交跑 ancestry，分类成 `(ok 行, problems, prereqs)`。

    `False`（提交存在但不是 `ref` 祖先）⇒ problem（缺该片 ⇒ 对应判据 PREREQ/红）；
    `None`（缺提交/缺 ref）⇒ prereq。不打印，便于 selftest 直测与调用方分类。
    """
    oks: list[str] = []
    problems: list[str] = []
    prereqs: list[str] = []
    for commit, label in fixes:
        nice = commit[:10] + (f"（{label}）" if label else "")
        val, err = is_ancestor(repo, commit, ref)
        if val is True:
            oks.append(f"[OK] R1 构建源 {repo} 的 {ref} 含修复提交 {nice}（--is-ancestor=YES）")
        elif val is False:
            rc, head = git(repo, "rev-parse", ref)
            problems.append(
                f"RED R1: {repo} 的 {ref}={head or '?'} 不含 {nice}"
                "（窗口缺该片 ⇒ 对应判据 PREREQ/红，白烧一次性窗口）"
            )
        else:
            prereqs.append(f"PREREQ_RED R1: 判不了 {nice}（{err}）")
    return oks, problems, prereqs


def baseline_freshness_findings(
    repo: str, ref: str, baseline_ref: str
) -> tuple[list[str], list[str], list[str]]:
    """R6 的纯逻辑：构建源 `ref` 必须含基线（默认 `origin/master`）在生产路径上的全部提交。

    返回 `(ok 行, problems, prereqs)`。语义：
      * `baseline_ref` 为空 ⇒ 显式跳过（返回一条 INFO，供无 remote 的离线自测；非窗口口径）。
      * 解析不到基线/被检 ref ⇒ prereq（fail-closed，不静默放行）。
      * `ref` == 基线，或 `ref` 是基线的后继（`--is-ancestor <base> <ref>`）⇒ OK。
      * 否则比 `git diff --name-only <base> <ref> -- <生产路径>`：空 ⇒ OK（非生产面差异可接受）；
        非空 ⇒ RED（构建源落后于基线 ⇒ 窗口会测旧件）。
    不打印，便于 selftest 直测与调用方分类。
    """
    if not baseline_ref:
        return (["INFO R6 跳过（--baseline-ref 为空）"], [], [])
    rc, base_oid = git(repo, "rev-parse", "--verify", f"{baseline_ref}^{{commit}}")
    if rc != 0 or not base_oid:
        return ([], [], [f"PREREQ_RED R6: 解析不到基线 ref {baseline_ref!r}（rc={rc}）"])
    rc, ref_oid = git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}")
    if rc != 0 or not ref_oid:
        return ([], [], [f"PREREQ_RED R6: 解析不到被检 ref {ref!r}（rc={rc}）"])
    short = f"{baseline_ref}@{base_oid[:10]}"
    if ref_oid == base_oid:
        return ([f"[OK] R6 构建源 {ref}={ref_oid[:10]} == 基线 {short}"], [], [])
    val, err = is_ancestor(repo, base_oid, ref)
    if val is True:
        return (
            [f"[OK] R6 构建源 {ref}={ref_oid[:10]} 含基线 {short}（--is-ancestor=YES）"],
            [],
            [],
        )
    rc, out = git(repo, "diff", "--name-only", base_oid, ref_oid, "--", *PRODUCTION_PATHS)
    if rc != 0:
        return ([], [], [f"PREREQ_RED R6: git diff 失败（{out or 'rc=' + str(rc)}）"])
    changed = [ln for ln in out.splitlines() if ln.strip()]
    if not changed:
        return (
            [f"[OK] R6 构建源 {ref}={ref_oid[:10]} 相对基线 {short} 生产路径零差异"],
            [],
            [],
        )
    head = ", ".join(changed[:5]) + (" …" if len(changed) > 5 else "")
    return (
        [],
        [
            f"RED R6: 构建源 {ref}={ref_oid[:10]} 落后于基线 {short} 的生产路径"
            f"（{len(changed)} 件：{head}）⇒ 窗口会测旧件，先 `fetch` + `checkout --detach <基线>` + 重编译"
        ],
        [],
    )


def sha256_file(path: str) -> str | None:
    """流式 sha256；文件不存在/读不了 ⇒ `None`（fail-closed 由调用方处置）。"""
    if not os.path.isfile(path):
        return None
    h = hashlib.sha256()
    try:
        with open(path, "rb") as fh:
            while True:
                chunk = fh.read(HASH_CHUNK)
                if not chunk:
                    break
                h.update(chunk)
    except OSError:
        return None
    return h.hexdigest()


# ---------------------------------------------------------------- R8 窗口槽位
def _powershell_path() -> str:
    for cand in (
        os.path.join(
            os.environ.get("SystemRoot", "C:\\Windows"),
            "System32",
            "WindowsPowerShell",
            "v1.0",
            "powershell.exe",
        ),
        "/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe",
    ):
        if cand and os.path.isfile(cand):
            return cand
    return ""


def _normalize_slot_probe(obj) -> "tuple[dict, list, str | None]":
    """把探针 JSON（真探的 PS 输出或注入件）归一成 `(ports={port:pid}, exes=[(pid,path)], err)`。"""
    ports: dict = {}
    exes: list = []
    if not isinstance(obj, dict):
        return {}, [], f"探针结构不符（顶层 {type(obj).__name__}，要 object）"
    try:
        raw_ports = obj.get("ports") or []
        if isinstance(raw_ports, dict):
            raw_ports = [raw_ports]
        for item in raw_ports:
            ports[str(int(item["LocalPort"]))] = int(item["OwningProcess"])
        raw_exes = obj.get("exes") or []
        if isinstance(raw_exes, dict):
            raw_exes = [raw_exes]
        for item in raw_exes:
            exes.append((int(item["ProcessId"]), str(item.get("ExecutablePath") or "")))
    except (KeyError, TypeError, ValueError) as exc:
        return {}, [], f"探针结构不符（{type(exc).__name__}: {exc}）"
    return ports, exes, None


def probe_window_slot(powershell: str = "") -> "tuple[dict, list, str | None]":
    """真探本机窗口槽位：LISTENING 游戏端口 + 在跑的 `game-svr.exe`。

    只读（`Get-NetTCPConnection` / `Get-CimInstance`），零起栈、零 kill。探不了 ⇒ `err` 非空，
    调用方按 INFO 跳过（不是 RED —— 干净检出/非 Windows 本来就没有栈可占端口）。
    """
    ps = powershell or _powershell_path()
    if not ps or not os.path.isfile(ps):
        return {}, [], "找不到 powershell（非 Windows / 干净检出 ⇒ 无栈可占端口）"
    script = (
        "$ports = Get-NetTCPConnection -State Listen -LocalPort "
        + ",".join(str(p) for p in GAME_PORTS)
        + " -ErrorAction SilentlyContinue | Select-Object LocalPort,OwningProcess; "
        "$procs = Get-CimInstance Win32_Process -Filter \"Name='game-svr.exe'\" | "
        "Select-Object ProcessId,ExecutablePath; "
        "ConvertTo-Json -Compress -Depth 4 -InputObject "
        "([pscustomobject]@{ports=@($ports); exes=@($procs)})"
    )
    try:
        proc = subprocess.run(
            [ps, "-NoProfile", "-Command", script], capture_output=True, timeout=60
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {}, [], f"探针失败（{type(exc).__name__}: {exc}）"
    if proc.returncode != 0:
        return {}, [], f"探针 rc={proc.returncode}: {decode_text(proc.stderr)[:200]}"
    raw = decode_text(proc.stdout).strip()
    if not raw:
        return {}, [], "探针无输出（Get-NetTCPConnection 不可用？）"
    try:
        obj = json.loads(raw)
    except ValueError as exc:
        return {}, [], f"探针输出不是 JSON（{exc}: {raw[:160]}）"
    return _normalize_slot_probe(obj)


def load_slot_probe(path: str) -> "tuple[dict, list, str | None]":
    """`--slot-probe-json`：自测/证据复算的注入探针（窗口机上禁用）。"""
    try:
        with open(path, "r", encoding="utf-8") as fh:
            obj = json.load(fh)
    except (OSError, ValueError) as exc:
        return {}, [], f"读不了注入探针 {path}（{exc}）"
    return _normalize_slot_probe(obj)


def load_slot_pid_file(slot_log_dir: str) -> "tuple[set, str | None]":
    """读 `<slot_log_dir>/rust-stack.pid.json` ⇒ `(pids, err)`；err 非空 = 读不了/认不出。"""
    if not slot_log_dir:
        return set(), "未给停栈目录（--slot-log-dir）"
    path = os.path.join(slot_log_dir, SLOT_PID_FILE_NAME)
    try:
        with open(path, "r", encoding="utf-8-sig") as fh:
            obj = json.load(fh)
    except OSError as exc:
        return set(), f"读不了 {path}（{exc}）"
    except ValueError as exc:
        return set(), f"{path} 不是合法 JSON（{exc}）"
    if not isinstance(obj, list):
        return set(), f"{path} 顶层应为 array（实为 {type(obj).__name__}）"
    pids: set = set()
    for item in obj:
        if not isinstance(item, dict):
            continue
        try:
            if item.get("pid") is not None:
                pids.add(int(item["pid"]))
        except (TypeError, ValueError):
            continue
    return pids, None


def slot_stop_dir_findings(
    occupied_pids, slot_log_dir: str, pid_file_pids, pid_file_err: str | None
) -> "tuple[list[str], list[str], list[str]]":
    """R8 附加牙（片63）：推荐的停栈 `-LogDir` 必须**真的持有**占用端口的那些 pid。

    `start_rust_stack.ps1 -Stop` 只读自己 `-LogDir` 下的 `rust-stack.pid.json` ⇒ 目录选错时它是
    静默 no-op（`NOTHING_TO_STOP`）。旧版把 `stack2` 写死 ⇒ 别的目录的泄漏栈会被误报成「已给停栈命令」。
    本函数 fail-closed：pid 文件读不了 / 不持有这些 pid ⇒ RED（不打印任何像「照抄即可」的命令）。
    无占用端口 ⇒ 不产生任何行（不误伤干净槽位）。
    """
    oks: list = []
    probs: list = []
    prereqs: list = []
    if not occupied_pids:
        return oks, probs, prereqs
    want = sorted({int(p) for p in occupied_pids})
    if pid_file_err:
        probs.append(
            f"RED R8: 占用端口的栈无法从推荐停栈目录 {slot_log_dir or '<未给>'} 找到 pid 文件"
            f"（{pid_file_err}）⇒ **不要照抄任何固定 -LogDir**；请用**持有这些 pid** 的窗口目录："
            f" {slot_stop_command('<持有这些 pid 的 rust-stack.pid.json 所在目录>')}"
        )
        return oks, probs, prereqs
    missing = sorted(set(want) - {int(p) for p in (pid_file_pids or [])})
    if missing:
        probs.append(
            f"RED R8: 推荐停栈目录 {slot_log_dir} 的 {SLOT_PID_FILE_NAME} 不持有占用端口的 pid"
            f" {missing} ⇒ 照抄该目录的 -Stop 会 NOTHING_TO_STOP（白烧窗）。"
            f"请改用**持有这些 pid** 的窗口目录："
            f" {slot_stop_command('<持有这些 pid 的 rust-stack.pid.json 所在目录>')}"
        )
        return oks, probs, prereqs
    oks.append(
        f"[OK] R8 停栈目录校验：{slot_log_dir}/{SLOT_PID_FILE_NAME} 持有全部占用端口的 pid（{want}）"
    )
    return oks, probs, prereqs


def window_slot_findings(
    ports: "dict | None",
    exes: "list | None",
    target_dir: str,
    slot_log_dir: str = "",
    pid_file_pids=None,
    pid_file_err: "str | None" = None,
) -> "tuple[list[str], list[str], list[str]]":
    """R8 的纯逻辑：`(ports, exes, target_dir, slot_log_dir, pid file)` ⇒ `(ok 行, problems, prereqs)`。

    - 任一游戏端口 LISTENING ⇒ RED（点名列 pid + **校验过**的停栈命令）；
    - 停栈 `-LogDir` 校验（片63）：pid 文件读不了/不持有占用 pid ⇒ 另加 RED（不打印误导命令）；
    - 在跑的 `game-svr.exe` 路径 != `<target_dir>/debug/game-svr.exe`（缺 target_dir 亦然）⇒ RED；
    - 两者都干净 ⇒ ok。不打印，便于 selftest 直测与调用方分类。
    """
    oks: list = []
    probs: list = []
    prereqs: list = []
    ports = ports or {}
    exes = list(exes or [])
    occupied = sorted((int(p), int(pid)) for p, pid in ports.items() if pid)
    if occupied:
        pretty = ", ".join(f"{p}(pid {pid})" for p, pid in occupied)
        want = {pid for _, pid in occupied}
        validated = (
            pid_file_err is None
            and pid_file_pids is not None
            and want <= {int(p) for p in pid_file_pids}
        )
        cmd = slot_stop_command(
            (slot_log_dir or SLOT_LOG_DIR_DEFAULT) if validated else "<持有这些 pid 的目录>"
        )
        probs.append(
            "RED R8: 窗口槽位被占 —— 游戏端口仍在 LISTENING: "
            f"{pretty} ⇒ 这是别的/修复前的栈；直接起窗会从旧 exe 抓包"
            "（2026-10-14 15:0x 的假回归即此）。先停栈："
            f" {cmd}"
        )
        v_oks, v_probs, v_pre = slot_stop_dir_findings(
            want, slot_log_dir or SLOT_LOG_DIR_DEFAULT, pid_file_pids, pid_file_err
        )
        oks.extend(v_oks)
        probs.extend(v_probs)
        prereqs.extend(v_pre)
    else:
        oks.append(f"[OK] R8 游戏端口全 DOWN（{len(GAME_PORTS)} 个；3306 共享 MySQL 不在门内）")
    if not exes:
        oks.append("[OK] R8 无 game-svr.exe 在跑")
        return oks, probs, prereqs
    if not target_dir:
        probs.append(
            f"RED R8: 有 game-svr.exe 在跑 {exes}，但未给 --target-dir ⇒ 无法证明它是本计划 exe"
        )
        return oks, probs, prereqs
    want = os.path.normcase(os.path.abspath(os.path.join(target_dir, "debug", "game-svr.exe")))
    ok_pids: list = []
    for pid, path in exes:
        got = os.path.normcase(os.path.abspath(path)) if path else ""
        if got != want:
            probs.append(
                f"RED R8: 在跑的 game-svr.exe（pid {pid}）来自 {path or '<未知>'}，"
                f"不是本计划 {os.path.join(target_dir, 'debug', 'game-svr.exe')}"
                " ⇒ 修复前/异源栈，窗口会测旧件"
            )
        else:
            ok_pids.append(pid)
    if ok_pids:
        oks.append(f"[OK] R8 在跑的 game-svr.exe（pid {', '.join(map(str, ok_pids))}）来自本计划 target")
    return oks, probs, prereqs


def record_provenance(
    repo: str, ref: str, target_dir: str, bins: "tuple[str, ...] | list[str]"
) -> tuple[dict | None, str | None]:
    """录制出处清单：`{"version", "source_head", "target_dir", "bins": {name: {"sha256", "bytes"}}}`。

    返回 `(manifest, error)`；`error` 非空 = 录不了（缺 ref / 缺 target-dir / 缺任一件）。
    """
    if not repo:
        return None, "缺 --repo"
    if not target_dir:
        return None, "缺 --target-dir"
    rc, oid = git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}")
    if rc != 0 or not oid:
        return None, f"解析不到 ref {ref!r}（rc={rc}）"
    entries: dict[str, dict] = {}
    missing: list[str] = []
    for name in bins:
        path = os.path.join(target_dir, "debug", name)
        digest = sha256_file(path)
        if digest is None:
            missing.append(name)
            entries[name] = {"sha256": None, "bytes": None}
            continue
        entries[name] = {"sha256": digest, "bytes": os.path.getsize(path)}
    if missing:
        return None, f"缺件（先 `cargo build … -p …`）：{', '.join(missing)}"
    return (
        {
            "version": PROVENANCE_VERSION,
            "source_head": oid,
            "target_dir": os.path.abspath(target_dir),
            "bins": entries,
        },
        None,
    )


def binary_provenance_findings(
    repo: str,
    ref: str,
    target_dir: str,
    provenance_path: str,
    bins: "tuple[str, ...] | list[str]",
) -> tuple[list[str], list[str], list[str]]:
    """R7 的纯逻辑：`target-dir/debug` 的 exe 必须与出处清单逐字节一致，且清单必须来自当前 `ref`。

    返回 `(ok 行, problems, prereqs)`；不打印，便于 selftest 直测。语义：
      * `target_dir` 为空或 `provenance_path` 为空 ⇒ INFO 跳过（向后兼容旧调用；窗口口径两者都要给）。
      * 清单缺失/坏 JSON/字段缺失 ⇒ prereq（fail-closed）。
      * 清单 `source_head` != 当前 `ref` oid ⇒ 若两者在 `RUST_BUILD_PATHS` 上零差异 ⇒ OK（只差非生产文件，
        cargo 不会重链，exe 仍逐字节有效）；有生产差异 ⇒ RED（exe 来自另一棵树 ⇒ 窗口会测旧件）。
      * 任一件 exe 缺失或其 sha256 != 清单 ⇒ RED。
    """
    if not target_dir or not provenance_path:
        return (["INFO R7 跳过（未给 --target-dir/--provenance；窗口口径两者都必须给）"], [], [])
    if not repo:
        return ([], [], ["PREREQ_RED R7: 缺 --repo（判不了 out 清单与源的对账）"])
    rc, oid = git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}")
    if rc != 0 or not oid:
        return ([], [], [f"PREREQ_RED R7: 解析不到被检 ref {ref!r}（rc={rc}）"])
    if not os.path.isfile(provenance_path):
        return (
            [],
            [],
            [
                f"PREREQ_RED R7: 缺出处清单 {provenance_path}"
                "（先编译 + 录制，再复跑本门）"
                f" {PROVENANCE_RECORD_CMD_MARK} "
                f"{provenance_record_command(repo, target_dir, provenance_path)}"
            ],
        )
    try:
        with open(provenance_path, "r", encoding="utf-8") as fh:
            manifest = json.load(fh)
    except (OSError, ValueError) as exc:
        return ([], [], [f"PREREQ_RED R7: 清单解析失败（{type(exc).__name__}: {exc}）"])
    if not isinstance(manifest, dict) or not isinstance(manifest.get("bins"), dict):
        return ([], [], [f"PREREQ_RED R7: 清单结构不符（缺 bins 映射）：{provenance_path}"])
    head = manifest.get("source_head")
    if not isinstance(head, str) or not head:
        return ([], [], [f"PREREQ_RED R7: 清单缺 source_head：{provenance_path}"])
    problems: list[str] = []
    infos: list[str] = []
    if head != oid:
        rc, out = git(repo, "diff", "--name-only", head, oid, "--", *RUST_BUILD_PATHS)
        if rc != 0:
            return ([], [], [f"PREREQ_RED R7: git diff（清单 head ↔ ref）失败（{out or 'rc=' + str(rc)}）"])
        moved = [ln for ln in out.splitlines() if ln.strip()]
        if moved:
            problems.append(
                f"RED R7: 出处清单 source_head={head[:10]} != 当前构建源 {ref}={oid[:10]}"
                f" 且在影响六件 exe 的路径上有 {len(moved)} 件差异（{', '.join(moved[:3])}"
                + (" …" if len(moved) > 3 else "")
                + "）⇒ 预构建 exe 来自另一棵树（窗口会测旧件）；先重编译 + 重录清单。"
                + PROVENANCE_RECORD_CMD_MARK
                + " "
                + provenance_record_command(repo, target_dir, provenance_path)
            )
        else:
            infos.append(
                f"[OK] R7 出处清单 source_head={head[:10]} != {ref}={oid[:10]}，但两者在"
                f" `mir2-rs/**` 零差异 ⇒ 六件 exe 仍有效（cargo 不会重链）"
            )
    recorded = manifest["bins"]
    bad: list[str] = []
    for name in bins:
        entry = recorded.get(name)
        want = entry.get("sha256") if isinstance(entry, dict) else None
        path = os.path.join(target_dir, "debug", name)
        got = sha256_file(path)
        if want is None:
            bad.append(f"{name}（清单无记录）")
        elif got is None:
            bad.append(f"{name}（缺件）")
        elif got != want:
            bad.append(f"{name}（sha256 {got[:10]} != 清单 {want[:10]}）")
    if bad:
        problems.append(
            f"RED R7: {len(bad)} 件 exe 与出处清单不一致（{', '.join(bad[:4])}"
            + (" …" if len(bad) > 4 else "")
            + ") ⇒ 窗口会从旧/异构 exe 起栈；先重编译 + 重录清单。"
            + PROVENANCE_RECORD_CMD_MARK
            + " "
            + provenance_record_command(repo, target_dir, provenance_path)
        )
    if not problems:
        # 末行必须报**清单里的** `source_head`（不是当前 ref 的 oid）：两者在
        # 「清单 head 滞后但生产路径零差异」的豁免支会不同，报 ref 会让人误以为清单本就来自该 head
        # （reviewer-5 `871cc797` O-1）。正面支两者相等 ⇒ 该改动逐字节不变。
        return (
            infos
            + [
                f"[OK] R7 出处清单 {os.path.basename(provenance_path)} source_head={head[:10]}，"
                f"{len(bins)} 件 exe sha256 逐字节一致"
            ],
            [],
            [],
        )
    return ([], problems, [])


def mapinfo_logical_name(content: str) -> str | None:
    """section `[...]` 内容 → 逻辑名（`crates/envir/src/mapinfo.rs` 的
    `get_valid_str_cap` + `split_map_name_and_file`：先按空格/逗号/制表取首段，再按 `|` 取前段）。"""
    token = ""
    for ch in content:
        if ch in _MAPINFO_SEPS:
            break
        token += ch
    if not token:
        return None
    if "|" in token:
        return token.split("|", 1)[0] or None
    m = re.search(r"<([^>]*)>(.*)", token)
    if m and m.group(1):
        return m.group(2) or None
    return token


def parse_mapinfo_names(text: str) -> list[str]:
    """按注册表口径列出**有效 section** 的逻辑名（顺序 = `map_id`）。"""
    names: list[str] = []
    for line in text.splitlines():
        if not line.startswith("["):
            continue
        m = SECTION_RE.match(line)
        if not m:
            continue
        name = mapinfo_logical_name(m.group(1))
        if name:
            names.append(name)
    return names


def find_format_line(main_rs_text: str) -> str:
    """返回第一条含 `vis=` 的版式行（给人看；空串 = 找不到）。"""
    for line in main_rs_text.splitlines():
        if "vis=" in line and "map=" in line:
            return line.strip()
    return ""


def parse_points_spec(spec: str) -> list[tuple[int, int, str]]:
    """`x,y:expect;...` → `[(x, y, expect)]`；格式错抛 `ValueError`。"""
    out: list[tuple[int, int, str]] = []
    for item in spec.split(";"):
        item = item.strip()
        if not item:
            continue
        coord, _, expect = item.partition(":")
        expect = expect.strip() or "report"
        if expect not in ("inside", "outside", "report"):
            raise ValueError(f"expect 只能是 inside/outside/report: {item!r}")
        xs, ys = coord.split(",")
        out.append((int(xs), int(ys), expect))
    return out


def _read(path: str) -> str | None:
    if not os.path.isfile(path):
        return None
    try:
        with open(path, "rb") as fh:
            return decode_text(fh.read())
    except OSError:
        return None


def run_mongen_checker(
    checker: str, mongen: str, map_name: str, x: int, y: int, expect: str
) -> tuple[int, str]:
    """调既有 `check_mongen_dense_points.py`（同一判据源，别在本工具里复制几何）。"""
    cmd = [
        sys.executable,
        "-I",
        "-B",
        checker,
        "--mongen",
        mongen,
        "--map",
        map_name,
        "--point",
        f"{x},{y}",
        "--expect",
        expect,
    ]
    try:
        proc = subprocess.run(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120
        )
    except (OSError, subprocess.SubprocessError) as exc:
        return 127, f"{type(exc).__name__}: {exc}"
    return proc.returncode, decode_text(proc.stdout).strip()


def selftest() -> int:
    ok = 0
    total = 0

    def chk(name: str, cond: bool, detail: str = "") -> None:
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
        print(f"[{'PASS' if cond else 'FAIL'}] selftest {name}{(' ' + detail) if detail else ''}")

    # --- MapInfo 解析（含 `|` 与制表/空格分隔） ---
    mapinfo = (
        ";comment\n"
        "[jiaren|0150]\n"
        "[DB|DM011]\n"
        "[G002 真爱质询屋]\n"
        "[0\t比奇省]\n"
    )
    names = parse_mapinfo_names(mapinfo)
    chk("mapinfo-names", names == ["jiaren", "DB", "G002", "0"], repr(names))
    chk("mapinfo-index", names.index("0") == 3, str(names.index("0")))
    chk("mapinfo-missing", "9" not in names)

    # 26 号（0-based）逻辑名是 `0`：合成 26 个前缀 + `0`。
    synth = "".join(f"[m{i} 图{i}]\n" for i in range(26)) + "[0\t比奇省]\n"
    s_names = parse_mapinfo_names(synth)
    chk("mapinfo-26-is-0", s_names[26] == "0", s_names[26])
    chk("mapinfo-count-27", len(s_names) == 27, str(len(s_names)))

    # --- main.rs 标记 ---
    good = f'"{DIAG_FORMAT_MARKER} same_map={{}} nearest={{}}" .filter(|v| v.flag {VIS_FILTER_MARKER})'
    bad_old = '" [{}({}) map={} vis={} same_map={} nearest={}]" .filter(|v| v.flag == VisibleFlag::Show)'
    chk("marker-good", DIAG_FORMAT_MARKER in good and VIS_FILTER_MARKER in good)
    chk("marker-old-red", DIAG_FORMAT_MARKER not in bad_old and VIS_FILTER_MARKER not in bad_old)
    chk("format-line-finder", "vis=" in find_format_line(good + "\n") and find_format_line(bad_old) != "")

    # --- 坐标 spec ---
    pts = parse_points_spec("291,666:outside;289,618:inside")
    chk("points-spec", pts == [(291, 666, "outside"), (289, 618, "inside")], repr(pts))
    try:
        parse_points_spec("1,2:bogus")
        chk("points-spec-bad", False)
    except ValueError:
        chk("points-spec-bad", True)

    # --- git ancestry（临时仓） ---
    with tempfile.TemporaryDirectory(prefix="rustpf-") as tmp:
        env = dict(os.environ)
        env["GIT_AUTHOR_NAME"] = env["GIT_COMMITTER_NAME"] = "selftest"
        env["GIT_AUTHOR_EMAIL"] = env["GIT_COMMITTER_EMAIL"] = "selftest@local"
        with open(os.path.join(tmp, "a.txt"), "w", encoding="utf-8") as fh:
            fh.write("a\n")
        for cmd in (
            ["git", "-C", tmp, "init", "-q"],
            ["git", "-C", tmp, "add", "-A"],
            ["git", "-C", tmp, "commit", "-q", "-m", "a"],
        ):
            subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        rc, first = git(tmp, "rev-parse", "HEAD")
        chk("git-init", rc == 0 and len(first) == 40, first)
        with open(os.path.join(tmp, "b.txt"), "w", encoding="utf-8") as fh:
            fh.write("b\n")
        for cmd in (
            ["git", "-C", tmp, "add", "-A"],
            ["git", "-C", tmp, "commit", "-q", "-m", "b"],
        ):
            subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        val, _ = is_ancestor(tmp, first, "HEAD")
        chk("git-ancestor-yes", val is True, repr(val))
        rc, second = git(tmp, "rev-parse", "HEAD")
        chk("git-second-oid", rc == 0 and len(second) == 40, second)
        val, _ = is_ancestor(tmp, second, first)
        chk("git-ancestor-no", val is False, repr(val))
        # R1 多修复提交（片㉝ 新增）：只含第一个 ⇒ 第二个必判 RED（真身 = 专用源只含片⑪、缺片㉜）。
        oks, probs, prereqs = required_fix_findings(
            tmp, first, [(first, "片⑪"), (second, "片㉜")]
        )
        chk(
            "fixes-second-missing-red",
            len(oks) == 1 and len(probs) == 1 and not prereqs,
            f"ok={len(oks)} prob={len(probs)} pre={len(prereqs)}",
        )
        oks, probs, prereqs = required_fix_findings(
            tmp, "HEAD", [(first, "片⑪"), (second, "片㉜")]
        )
        chk(
            "fixes-all-present",
            len(oks) == 2 and not probs and not prereqs,
            f"ok={len(oks)} prob={len(probs)} pre={len(prereqs)}",
        )
        oks, probs, prereqs = required_fix_findings(tmp, first, [(first, "片⑪"), ("0" * 40, "x")])
        chk(
            "fixes-unknown-prereq",
            len(oks) == 1 and not probs and len(prereqs) == 1,
            f"ok={len(oks)} prob={len(probs)} pre={len(prereqs)}",
        )
        chk(
            "required-fixes-default-five",
            [c for c, _ in REQUIRED_FIX_COMMITS]
            == [
                FIX_COMMIT_DIAG,
                FIX_COMMIT_STRUCK,
                FIX_COMMIT_53_TAIL,
                FIX_COMMIT_52_LAYOUT,
                FIX_COMMIT_STRUCK_LAYOUT,
            ],
            repr(REQUIRED_FIX_COMMITS),
        )
        # HIGHHP 家族三片必须都在默认必查集里（缺任一片 ⇒ 「测修复前镜像」类假绿；真身 =
        # 源 8e491f81e 含片⑪/㉜ 但缺 53 尾修，旧两片集判 GREEN）。
        chk(
            "required-fixes-include-highhp-family",
            {FIX_COMMIT_53_TAIL, FIX_COMMIT_52_LAYOUT, FIX_COMMIT_STRUCK_LAYOUT}
            <= {c for c, _ in REQUIRED_FIX_COMMITS},
            repr(REQUIRED_FIX_COMMITS),
        )
        val, _ = is_ancestor(tmp, "0" * 40, "HEAD")
        chk("git-ancestor-unknown-prereq", val is None, repr(val))
        val, _ = is_ancestor(os.path.join(tmp, "nope"), first, "HEAD")
        chk("git-not-a-repo-prereq", val is None, repr(val))

        # R6（片㊲）：构造「仅生产路径变化」的提交对，直测四态 + 两条 fail-closed。
        os.makedirs(os.path.join(tmp, "mir2-rs", "bins"), exist_ok=True)
        with open(os.path.join(tmp, "mir2-rs", "bins", "prod.rs"), "w", encoding="utf-8") as fh:
            fh.write("v1\n")
        for cmd in (
            ["git", "-C", tmp, "add", "-A"],
            ["git", "-C", tmp, "commit", "-q", "-m", "prod1"],
        ):
            subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        rc, third = git(tmp, "rev-parse", "HEAD")
        chk("git-third-oid", rc == 0 and len(third) == 40, third)
        with open(os.path.join(tmp, "mir2-rs", "bins", "prod.rs"), "w", encoding="utf-8") as fh:
            fh.write("v2\n")
        for cmd in (
            ["git", "-C", tmp, "add", "-A"],
            ["git", "-C", tmp, "commit", "-q", "-m", "prod2"],
        ):
            subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        rc, fourth = git(tmp, "rev-parse", "HEAD")
        chk("git-fourth-oid", rc == 0 and len(fourth) == 40, fourth)

        oks, probs, pre = baseline_freshness_findings(tmp, fourth, fourth)
        chk("r6-equal-green", len(oks) == 1 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        # ref=third 是 baseline=fourth 的**祖先**（真实形态：构建源停在旧 master）⇒ 落后必判 RED。
        oks, probs, pre = baseline_freshness_findings(tmp, third, fourth)
        chk("r6-behind-prod-red", not oks and len(probs) == 1 and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        oks, probs, pre = baseline_freshness_findings(tmp, fourth, second)
        chk("r6-ahead-green", len(oks) == 1 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        oks, probs, pre = baseline_freshness_findings(tmp, second, first)
        chk("r6-nonprod-diff-green", len(oks) == 1 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        oks, probs, pre = baseline_freshness_findings(tmp, fourth, "no-such-base-xyz")
        chk("r6-unknown-baseline-prereq", not oks and not probs and len(pre) == 1, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        oks, probs, pre = baseline_freshness_findings(tmp, fourth, "")
        chk("r6-empty-baseline-skip", len(oks) == 1 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        chk(
            "r6-default-baseline-origin-master",
            BASELINE_REF_DEFAULT == "origin/master",
            repr(BASELINE_REF_DEFAULT),
        )

        # R7（片㊻）：exe ↔ 源出处清单（收 R6 登记的界限）。
        def _commit_extra(path_rel: str, body: str, msg: str) -> str:
            full = os.path.join(tmp, path_rel)
            os.makedirs(os.path.dirname(full), exist_ok=True)
            with open(full, "w", encoding="utf-8") as fh:
                fh.write(body)
            for cmd in (
                ["git", "-C", tmp, "add", "-A"],
                ["git", "-C", tmp, "commit", "-q", "-m", msg],
            ):
                subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            rc, oid = git(tmp, "rev-parse", "HEAD")
            chk(f"git-{msg}-oid", rc == 0 and len(oid) == 40, oid)
            return oid

        fifth = _commit_extra("docs/x.txt", "x1\n", "fifth-nonprod")
        win_tgt = os.path.join(tmp, "tgt")
        win_dbg = os.path.join(win_tgt, "debug")
        os.makedirs(win_dbg, exist_ok=True)
        r7_bins = ("game-svr.exe", "db-srv.exe")
        for name, blob in (("game-svr.exe", b"gs"), ("db-srv.exe", b"db")):
            with open(os.path.join(win_dbg, name), "wb") as fh:
                fh.write(blob)
        prov = os.path.join(win_tgt, "window-provenance.json")
        man, err = record_provenance(tmp, "HEAD", win_tgt, r7_bins)
        chk("r7-record-ok", man is not None and not err, repr(err))
        chk(
            "r7-record-source-head",
            isinstance(man, dict) and man.get("source_head") == fifth,
            repr(man and man.get("source_head")),
        )
        with open(prov, "w", encoding="utf-8") as fh:
            json.dump(man, fh)
        # master 再前进一条**非生产**提交（只动 docs/）⇒ cargo 不重链 ⇒ exe 仍有效 ⇒ 不能假红。
        sixth = _commit_extra("docs/x.txt", "x2\n", "sixth-nonprod")
        oks, probs, pre = binary_provenance_findings(tmp, fifth, win_tgt, prov, r7_bins)
        chk("r7-green", len(oks) == 1 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        oks, probs, pre = binary_provenance_findings(tmp, sixth, win_tgt, prov, r7_bins)
        chk("r7-nonprod-advance-green", len(oks) == 2 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        # 豁免支的末行必须点名**清单**的 `source_head`（= fifth），而不是当前 ref（= sixth）。
        # 红检（改坏必红）：把末行改回 `source_head={oid[:10]}` ⇒ 本牙必红。
        last_ok = oks[-1] if oks else ""
        chk(
            "r7-nonprod-advance-ok-names-manifest-head",
            f"source_head={fifth[:10]}" in last_ok and f"source_head={sixth[:10]}" not in last_ok,
            repr(last_ok),
        )
        # 源前移且在 `mir2-rs/**` 上有生产差异（清单停在旧 head）⇒ RED（真身 = 刷源后不重编译）。
        oks, probs, pre = binary_provenance_findings(tmp, third, win_tgt, prov, r7_bins)
        chk("r7-source-head-mismatch-red", not oks and len(probs) == 1 and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        # 换掉一件 exe（暖 target 里是旧件）⇒ sha256 对不上 ⇒ RED。
        with open(os.path.join(win_dbg, "game-svr.exe"), "wb") as fh:
            fh.write(b"STALE")
        oks, probs, pre = binary_provenance_findings(tmp, fifth, win_tgt, prov, r7_bins)
        chk("r7-hash-mismatch-red", not oks and len(probs) == 1 and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        # 片64：R7 的 RED 必须自带**可照抄**的恢复命令（与 R8 的 `PREFLIGHT_STOP_CMD` 对等）。
        # 改坏必红：删掉 `binary_provenance_findings` 里那两处 `PROVENANCE_RECORD_CMD_MARK` 拼接
        # ⇒ `r7-red-names-record-cmd`（本牙）与 `r7-prereq-names-record-cmd` 必红。
        hash_red = " | ".join(probs)
        chk(
            "r7-red-names-record-cmd",
            PROVENANCE_RECORD_CMD_MARK in hash_red
            and "--record-provenance" in hash_red
            and f'--record-provenance "{prov}"' in hash_red,
            repr(hash_red[-220:]),
        )
        oks, probs, pre = binary_provenance_findings(tmp, third, win_tgt, prov, r7_bins)
        chk(
            "r7-stale-head-red-names-record-cmd",
            not oks and not pre
            and any(
                PROVENANCE_RECORD_CMD_MARK in p and "cargo build --offline" in p for p in probs
            ),
            repr(" | ".join(probs)[-220:]),
        )
        _rec_cmd = provenance_record_command(tmp, win_tgt, prov)
        chk(
            "r7-record-cmd-builds-six-pkgs",
            len(WINDOW_BUILD_PKGS) == 6
            and all(f"-p {p}" in _rec_cmd for p in WINDOW_BUILD_PKGS)
            and _rec_cmd.startswith(f'cd "{tmp}/mir2-rs"'),
            repr(_rec_cmd),
        )
        # fail-closed：录制必须**门在编译成功之后**（`cargo build` 非零 ⇒ `&&` 短路 ⇒ 不写清单）。
        chk(
            "r7-record-cmd-gated-on-build-success",
            "&&" in _rec_cmd and _rec_cmd.index("cargo build") < _rec_cmd.index("--record-provenance"),
            repr(_rec_cmd),
        )
        # 片65（reviewer-5 F-1②）：**执行性**牙 —— 把打印出来的命令尾部按 shell 词法切开，
        # 喂给本工具**自己的** argparse。旧版尾部是 `--record-provenance`（无值）⇒ SystemExit(2)，
        # 而子串牙（`--record-provenance in ...`）照样 PASS ⇒ 只有本牙能吹。
        _rec_argv = shlex.split(_rec_cmd.split("python -I -B ", 1)[1])[1:]
        try:
            _rec_ns = build_parser().parse_args(_rec_argv)
            _argv_ok = _rec_ns.record_provenance == prov
            _argv_why = f"record_provenance={_rec_ns.record_provenance!r} argv={_rec_argv}"
        except SystemExit as exc:  # argparse 在缺值时 SystemExit(2)
            _argv_ok = False
            _argv_why = f"argparse SystemExit({exc.code}) argv={_rec_argv}"
        chk("r7-record-cmd-argv-parses", _argv_ok, _argv_why)
        # 缺件（清单列了但磁盘没了）⇒ RED。
        os.remove(os.path.join(win_dbg, "db-srv.exe"))
        oks, probs, pre = binary_provenance_findings(tmp, fifth, win_tgt, prov, r7_bins)
        chk("r7-missing-bin-red", not oks and len(probs) == 1 and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        # 缺清单 ⇒ PREREQ_RED（fail-closed，不静默放行）。
        _missing_prov = os.path.join(win_tgt, "nope.json")
        oks, probs, pre = binary_provenance_findings(
            tmp, "HEAD", win_tgt, _missing_prov, r7_bins
        )
        chk("r7-missing-manifest-prereq", not oks and not probs and len(pre) == 1, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        chk(
            "r7-prereq-names-record-cmd",
            PROVENANCE_RECORD_CMD_MARK in pre[0] and "--record-provenance" in pre[0],
            repr(pre[0][-220:]),
        )
        # 片65（reviewer-5 F-2）：替身脚本用 `grep -oE 'PREFLIGHT_RECORD_CMD: .*'`（吃到行尾）提取 ⇒
        # 命令**不许**被后随全角括号 `）` 粘住（旧版 PREREQ 支 `…--record-provenance）` ⇒ 照抄带括号）。
        _pre_tail = pre[0].split(PROVENANCE_RECORD_CMD_MARK, 1)[-1].strip()
        chk(
            "r7-record-cmd-tail-clean",
            _pre_tail.endswith(f'"{_missing_prov}"') and "）" not in _pre_tail,
            repr(_pre_tail[-90:]),
        )
        # 未给 target-dir/provenance ⇒ INFO 跳过（向后兼容旧调用）。
        oks, probs, pre = binary_provenance_findings(tmp, "HEAD", "", "", r7_bins)
        chk("r7-empty-skip-info", len(oks) == 1 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
        # 录不出（缺 target-dir / 缺件）⇒ 返回错误（别写半份清单）。
        _, err_nt = record_provenance(tmp, "HEAD", "", r7_bins)
        chk("r7-record-needs-target", err_nt is not None, repr(err_nt))
        _, err_miss = record_provenance(tmp, "HEAD", win_tgt, ("game-svr.exe", "nope.exe"))
        chk("r7-record-missing-bin", err_miss is not None and "nope.exe" in err_miss, repr(err_miss))
        chk(
            "r7-window-bins-six",
            len(WINDOW_BINS) == 6 and WINDOW_BINS[0] == "game-svr.exe" and "db-srv.exe" in WINDOW_BINS,
            repr(WINDOW_BINS),
        )
        chk("r7-provenance-name", PROVENANCE_NAME == "window-provenance.json", PROVENANCE_NAME)
        chk(
            "r7-rust-build-paths",
            RUST_BUILD_PATHS[0] == "mir2-rs/bins" and "mir2-rs/Cargo.lock" in RUST_BUILD_PATHS,
            repr(RUST_BUILD_PATHS),
        )

    # R8（片60）：窗口槽位 fail-closed（真身 = 2026-10-14 泄漏栈占 9 端口 + 修复前 exe）。
    chk("r8-game-ports-nine-no-3306", len(GAME_PORTS) == 9 and 3306 not in GAME_PORTS, repr(GAME_PORTS))
    oks, probs, pre = window_slot_findings({}, [], "E:/tmp/plan-target")
    chk("r8-free-green", len(oks) == 2 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
    oks, probs, pre = window_slot_findings(
        {"5000": 30284},
        [(30284, "E:\\tmp\\openmir2-merge-target\\debug\\game-svr.exe")],
        "E:/tmp/m2svr-window-target",
        slot_log_dir="E:/tmp/client-e2e/stack2",
        pid_file_pids={30284, 33132},
    )
    joined = " | ".join(probs)
    chk(
        "r8-occupied-and-stale-exe-red",
        len(oks) == 1 and len(probs) == 2 and "5000" in joined and "不是本计划" in joined and not pre,
        f"ok={len(oks)} prob={len(probs)} pre={len(pre)}: {joined[:200]}",
    )
    chk(
        "r8-stop-dir-validated-names-owning-dir",
        "-LogDir E:/tmp/client-e2e/stack2 -Stop" in joined
        and "持有全部占用端口的 pid" in " | ".join(oks),
        f"ok={' | '.join(oks)[:160]}; prob={joined[:160]}",
    )
    # 片63：推荐停栈目录的 pid 文件**不持有**占用 pid ⇒ 另加 RED，且**不许**打印可照抄的固定目录。
    oks, probs, pre = window_slot_findings(
        {"5000": 30284},
        [],
        "E:/tmp/m2svr-window-target",
        slot_log_dir="E:/tmp/client-e2e/stack2",
        pid_file_pids={9999},
    )
    joined = " | ".join(probs)
    chk(
        "r8-stop-dir-mismatch-red",
        len(oks) == 1 and len(probs) == 2 and "不持有占用端口的 pid" in joined
        and "[30284]" in joined
        and "-LogDir E:/tmp/client-e2e/stack2 -Stop" not in joined
        and "<持有这些 pid" in joined,
        f"ok={len(oks)} prob={len(probs)} pre={len(pre)}: {joined[:240]}",
    )
    # pid 文件读不了/认不出 ⇒ 同样 fail-closed（不得静默把固定目录当答案）。
    oks, probs, pre = window_slot_findings(
        {"5000": 30284},
        [],
        "E:/tmp/m2svr-window-target",
        slot_log_dir="E:/tmp/client-e2e/stack2",
        pid_file_err="读不了 .../nope.json（FileNotFoundError）",
    )
    joined = " | ".join(probs)
    chk(
        "r8-stop-dir-missing-red",
        len(oks) == 1 and len(probs) == 2 and "找到 pid 文件" in joined
        and "-LogDir E:/tmp/client-e2e/stack2 -Stop" not in joined,
        f"ok={len(oks)} prob={len(probs)} pre={len(pre)}: {joined[:240]}",
    )
    # 纯函数直测：① 无占用 ⇒ 零行；② 持有全部 pid ⇒ OK + 命令用该目录。
    oks, probs, pre = slot_stop_dir_findings(set(), "E:/tmp/x", set(), None)
    chk("stop-dir-no-occupied-noop", not oks and not probs and not pre, f"ok={len(oks)} prob={len(probs)}")
    oks, probs, pre = slot_stop_dir_findings(
        {30284, 33132}, "E:/tmp/leak", {30284, 33132, 5}, None
    )
    chk(
        "stop-dir-owns-pids-ok",
        len(oks) == 1 and not probs and not pre
        and "-LogDir E:/tmp/leak -Stop" in slot_stop_command("E:/tmp/leak"),
        f"ok={len(oks)} prob={len(probs)} pre={len(pre)}",
    )
    # `load_slot_pid_file`：真文件能读出 pid；缺文件/坏 JSON ⇒ err（fail-closed）。
    _sd = tempfile.mkdtemp(prefix="rustpf-r8-pid-")
    with open(os.path.join(_sd, SLOT_PID_FILE_NAME), "w", encoding="utf-8") as fh:
        json.dump([{"name": "game-svr", "pid": 30284}], fh)
    _pids, _err = load_slot_pid_file(_sd)
    chk("load-pid-file-ok", _pids == {30284} and _err is None, f"pids={_pids} err={_err}")
    _pids, _err = load_slot_pid_file(os.path.join(_sd, "nope"))
    chk("load-pid-file-missing-err", not _pids and bool(_err), repr(_err))
    _bad_sd = os.path.join(_sd, "bad")
    os.mkdir(_bad_sd)
    with open(os.path.join(_bad_sd, SLOT_PID_FILE_NAME), "w", encoding="utf-8") as fh:
        fh.write("[ not-json")
    _pids, _err = load_slot_pid_file(_bad_sd)
    chk("load-pid-file-bad-json-err", not _pids and bool(_err), repr(_err))
    chk(
        "slot-log-dir-default-preserved",
        SLOT_LOG_DIR_DEFAULT == "E:/tmp/client-e2e/stack2" and SLOT_PID_FILE_NAME == "rust-stack.pid.json",
        f"{SLOT_LOG_DIR_DEFAULT} / {SLOT_PID_FILE_NAME}",
    )
    oks, probs, pre = window_slot_findings(
        {}, [(30284, "E:\\tmp\\m2svr-window-target\\debug\\game-svr.exe")], "E:/tmp/m2svr-window-target"
    )
    chk("r8-plan-exe-green", len(oks) == 2 and not probs and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
    oks, probs, pre = window_slot_findings({}, [(1, "C:\\other\\game-svr.exe")], "")
    chk("r8-no-target-dir-red", len(oks) == 1 and len(probs) == 1 and not pre, f"ok={len(oks)} prob={len(probs)} pre={len(pre)}")
    # 注入探针坏了必须 PREREQ（fail-closed），不能被当成「探不了 ⇒ INFO 跳过」而假绿。
    _bad = os.path.join(tempfile.mkdtemp(prefix="rustpf-r8-"), "bad.json")
    with open(_bad, "w", encoding="utf-8") as fh:
        fh.write("{\"ports\": [ not-json")
    _p, _x, _e = load_slot_probe(_bad)
    chk("r8-injected-probe-bad-prereq", not _p and not _x and bool(_e), repr(_e))
    _p, _x, _e = load_slot_probe(os.path.join(os.path.dirname(_bad), "nope.json"))
    chk("r8-injected-probe-missing-prereq", not _p and not _x and bool(_e), repr(_e))

    # --- 合成 mongen 检查器：验证调用与退出码传播 ---
    with tempfile.TemporaryDirectory(prefix="rustpf-mg-") as tmp:
        checker = os.path.join(tmp, "fake_checker.py")
        with open(checker, "w", encoding="utf-8") as fh:
            fh.write(
                "import sys\n"
                "args=sys.argv[1:]\n"
                "want=args[args.index('--expect')+1]\n"
                "print('FAKE', want)\n"
                "sys.exit(0 if want!='red' else 1)\n"
            )
        rc0, _ = run_mongen_checker(checker, "x", "0", 1, 2, "inside")
        chk("mongen-wire-green", rc0 == 0, str(rc0))
        rc1, _ = run_mongen_checker(checker, "x", "0", 1, 2, "red")
        chk("mongen-wire-red", rc1 == 1, str(rc1))
        rc2, _ = run_mongen_checker(os.path.join(tmp, "missing.py"), "x", "0", 1, 2, "inside")
        # 缺脚本时 `python <missing>` 自身以 2 退出 ⇒ 下游按 PREREQ_RED 处置（不是 0/1 的判据结论）。
        chk("mongen-wire-prereq", rc2 == 2, str(rc2))

    print(f"SELFTEST {'OK' if ok == total else 'FAILED'} ({ok}/{total})")
    return 0 if ok == total else 1


def build_parser() -> argparse.ArgumentParser:
    """本工具的 argparse（片65：抽成函数，供 selftest 把打印出的命令尾部**实际解析**一遍，
    即对『照抄可否跑』做执行性断言，而不是只查子串）。"""
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--repo", default="", help="Rust 构建源检出（默认从本脚本位置向上找 .git）")
    ap.add_argument("--ref", default="HEAD", help="被检查的 ref/oid（默认 HEAD）")
    ap.add_argument(
        "--fix-commit",
        action="append",
        default=None,
        help="必须为 --ref 祖先的修复提交（可重复；默认 = 片⑪+片㉜+HIGHHP 家族三帧；单值 = 只查那一个）",
    )
    ap.add_argument("--mirserver", default="E:/MirServer", help="oracle 部署根（默认 E:/MirServer）")
    ap.add_argument("--mongen", default="", help="MonGen.txt；默认 <mirserver>/M2GameSvr/Envir/MonGen.txt")
    ap.add_argument("--mapinfo", default="", help="MapInfo.txt；默认 <mirserver>/M2GameSvr/Envir/MapInfo.txt")
    ap.add_argument("--map-name", default="0", help="MonGen/角色记录用的逻辑地图名（默认 0）")
    ap.add_argument("--map-id", type=int, default=26, help="真机周期行里的 map_id（默认 26）")
    ap.add_argument(
        "--points",
        default="291,666:outside;291,651:outside;289,618:inside;287,620:inside",
        help="窗口坐标 `x,y:inside|outside;...`（默认 = ③a 反向 + ③b 框内）",
    )
    ap.add_argument(
        "--baseline-ref",
        default=BASELINE_REF_DEFAULT,
        help=(
            f"R6 基线 ref（默认 {BASELINE_REF_DEFAULT}；构建源必须含它在生产路径上的提交；"
            "空串 = 显式跳过，仅用于无 remote 的离线自测）"
        ),
    )
    ap.add_argument(
        "--target-dir",
        default="",
        help="R7 预构建目录（CARGO_TARGET_DIR；exe 在 <target-dir>/debug/）",
    )
    ap.add_argument(
        "--provenance",
        default="",
        help=f"R7 出处清单 JSON（默认 <target-dir>/{PROVENANCE_NAME}；空 = 跳过 R7）",
    )
    ap.add_argument(
        "--record-provenance",
        default="",
        help="录制 R7 出处清单到该路径后退出（每次 cargo build 之后跑）",
    )
    ap.add_argument(
        "--bin",
        action="append",
        default=None,
        help=f"R7 覆盖窗口件列表（可重复；默认六件 {WINDOW_BINS}）",
    )
    ap.add_argument(
        "--slot-probe-json",
        default="",
        help=(
            "R8 注入探针（仅自测/证据复算；窗口机禁用）：JSON "
            '{"ports":[{"LocalPort":n,"OwningProcess":pid}],'
            ' "exes":[{"ProcessId":pid,"ExecutablePath":"..."}]}'
        ),
    )
    ap.add_argument(
        "--slot-log-dir",
        default=SLOT_LOG_DIR_DEFAULT,
        help=(
            f"R8 停栈校验：占用端口的 `{SLOT_PID_FILE_NAME}` 应落在此目录（默认 "
            f"{SLOT_LOG_DIR_DEFAULT}）；该文件不持有占用 pid ⇒ RED（不打印误导命令）"
        ),
    )
    ap.add_argument("--selftest", action="store_true")
    return ap


def main(argv: list[str] | None = None) -> int:
    ap = build_parser()
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    script_dir = os.path.dirname(os.path.abspath(__file__))
    repo = args.repo or find_repo_root(script_dir) or ""
    bins = tuple(args.bin) if args.bin else WINDOW_BINS

    # R7 录制模式（`cargo build` 之后跑；写完清单 + 打一行可判据后退出）。
    if args.record_provenance:
        man, err = record_provenance(repo, args.ref, args.target_dir, bins)
        if man is None:
            print(f"PREREQ_RED R7-record: {err}")
            return 2
        try:
            with open(args.record_provenance, "w", encoding="utf-8") as fh:
                json.dump(man, fh, ensure_ascii=False, indent=1, sort_keys=True)
                fh.write("\n")
        except OSError as exc:
            print(f"PREREQ_RED R7-record: 写不了 {args.record_provenance}（{exc}）")
            return 2
        print(
            f"R7_PROVENANCE_RECORDED source_head={man['source_head']} "
            f"bins={len(man['bins'])} -> {args.record_provenance}"
        )
        return 0

    mongen = args.mongen or os.path.join(args.mirserver, "M2GameSvr", "Envir", "MonGen.txt")
    mapinfo = args.mapinfo or os.path.join(args.mirserver, "M2GameSvr", "Envir", "MapInfo.txt")

    problems: list[str] = []
    prereq: list[str] = []

    # R1 构建源含全部必需修复提交
    fixes = [(c, "") for c in args.fix_commit] if args.fix_commit else list(REQUIRED_FIX_COMMITS)
    if not repo:
        prereq.append("PREREQ_RED R1: 找不到 git 根（用 --repo 指明构建源检出）")
    else:
        oks, probs, pre = required_fix_findings(repo, args.ref, fixes)
        for line in oks:
            print(line)
        problems.extend(probs)
        prereq.extend(pre)

    # R2 源码含 ③ 读数标记
    main_rs = os.path.join(repo, MAIN_RS_REL) if repo else ""
    text = _read(main_rs) if main_rs else None
    if text is None:
        prereq.append(f"PREREQ_RED R2: 读不到 {main_rs or MAIN_RS_REL}")
    else:
        fmt_line = find_format_line(text)
        has_fmt = DIAG_FORMAT_MARKER in text
        has_filter = VIS_FILTER_MARKER in text
        if has_fmt and has_filter:
            print(f"[OK] R2 源码含 ③ 读数标记（版式 `{DIAG_FORMAT_MARKER}` + 可见集判据）")
        else:
            problems.append(
                f"RED R2: {main_rs} 缺标记 "
                f"(format={has_fmt} filter={has_filter}); 当前版式行 = {fmt_line or '<无>'}"
            )
        print(f"     main.rs 版式行 = {fmt_line or '<无 vis= 版式行>'}")

    # R3 map_id ↔ 逻辑名
    mi_text = _read(mapinfo) if mapinfo else None
    if mi_text is None:
        prereq.append(f"PREREQ_RED R3: 读不到 MapInfo: {mapinfo}")
    else:
        names = parse_mapinfo_names(mi_text)
        if not names:
            prereq.append(f"PREREQ_RED R3: {mapinfo} 解析不到任何 section")
        elif args.map_name not in names:
            prereq.append(f"PREREQ_RED R3: 逻辑名 {args.map_name!r} 不在 MapInfo（section 数 {len(names)}）")
        else:
            idx = names.index(args.map_name)
            if idx == args.map_id:
                print(f"[OK] R3 MapInfo 第 {idx} 个 section 的逻辑名 == {args.map_name!r}（= 真机 map_id {args.map_id}）")
            else:
                problems.append(
                    f"RED R3: MapInfo 第 {idx} 个 section 才是 {args.map_name!r}，"
                    f"但真机周期行 map_id={args.map_id} ⇒ MonGen 判据可能判错图"
                )

    # R4 坐标与生成框
    checker = os.path.join(script_dir, "check_mongen_dense_points.py")
    if not os.path.isfile(checker):
        prereq.append(f"PREREQ_RED R5: 缺 {checker}")
    else:
        try:
            pts = parse_points_spec(args.points)
        except ValueError as exc:
            prereq.append(f"PREREQ_RED R4: --points 解析失败（{exc}）")
            pts = []
        if pts and not os.path.isfile(mongen):
            prereq.append(f"PREREQ_RED R4: 读不到 MonGen: {mongen}")
        else:
            for (x, y, expect) in pts:
                rc, out = run_mongen_checker(checker, mongen, args.map_name, x, y, expect)
                tag = {0: "OK", 1: "RED", 2: "PREREQ", 127: "PREREQ"}.get(rc, "?")
                last = out.splitlines()[-1] if out else ""
                if rc == 0:
                    print(f"[OK] R4 ({x},{y}) --expect {expect} → rc=0")
                elif rc == 1:
                    problems.append(f"RED R4 ({x},{y}) --expect {expect} 不符: {last}")
                else:
                    prereq.append(f"PREREQ_RED R4 ({x},{y}) rc={rc}: {last or '<无输出>'}")

    # R5 工具齐备
    missing = [n for n in REQUIRED_TOOLS if not os.path.isfile(os.path.join(script_dir, n))]
    if missing:
        prereq.append(f"PREREQ_RED R5: 证据目录缺判据工具 {missing}")
    else:
        print(f"[OK] R5 判据工具齐备（{len(REQUIRED_TOOLS)} 件）")

    # R6 构建源必须是当前基线（默认 origin/master）——片㊲；R1 只管「含修复提交」不管「没落后」。
    if repo:
        oks, probs, pre = baseline_freshness_findings(repo, args.ref, args.baseline_ref)
        for line in oks:
            print(line)
        problems.extend(probs)
        prereq.extend(pre)

    # R7 预构建二进制 ↔ 构建源出处（片㊻）——R6 只比 git 差异，不比 exe。
    if repo:
        oks, probs, pre = binary_provenance_findings(
            repo, args.ref, args.target_dir, args.provenance, bins
        )
        for line in oks:
            print(line)
        problems.extend(probs)
        prereq.extend(pre)

    # R8 窗口槽位（片60）：R1..R7 只证「源/清单/exe 文件」，证不了「没有别的栈占着端口 / 在跑旧 exe」。
    if args.slot_probe_json:
        print(f"WARN R8 使用注入探针 {args.slot_probe_json}（仅自测/证据复算；窗口机上禁用）")
        slot_ports, slot_exes, slot_err = load_slot_probe(args.slot_probe_json)
        if slot_err:
            # 注入了却读不了 ⇒ fail-closed，不许当成「探不了 ⇒ 跳过」而假绿。
            prereq.append(f"PREREQ_RED R8: 注入探针不可用（{slot_err}）")
    else:
        slot_ports, slot_exes, slot_err = probe_window_slot()
        if slot_err:
            print(f"INFO R8 跳过，未探本机槽位（{slot_err}）")
    if not slot_err:
        # 片63：停栈 `-LogDir` 必须真的持有占用端口的 pid（旧版写死 stack2 ⇒ 别的目录会 NOTHING_TO_STOP）。
        slot_log_dir = args.slot_log_dir or SLOT_LOG_DIR_DEFAULT
        if slot_ports:
            pid_file_pids, pid_file_err = load_slot_pid_file(slot_log_dir)
        else:
            pid_file_pids, pid_file_err = None, None
        oks, probs, pre = window_slot_findings(
            slot_ports,
            slot_exes,
            args.target_dir,
            slot_log_dir=slot_log_dir,
            pid_file_pids=pid_file_pids,
            pid_file_err=pid_file_err,
        )
        for line in oks:
            print(line)
        problems.extend(probs)
        prereq.extend(pre)

    for line in problems:
        print(line)
    for line in prereq:
        print(line)
    rc = 0 if not problems and not prereq else (1 if problems else 2)
    print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
