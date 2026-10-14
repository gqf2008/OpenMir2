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
     `git -C <repo> merge-base --is-ancestor <fix-commit> <ref>`；默认两个——片⑪ `d16b5199d`
     （推出 ③ 判据读数 `vr/new/af`）与片㉜ `0d0d62a69`（受击帧 `SM_STRUCK` 按 oracle `SendRefMsg`
     广播，DoD③ 第三半「能攻击造成伤害」必需）；`--fix-commit` 可重复覆盖（单值 = 只查那一个，
     向后兼容旧调用）；`ref` 默认 `HEAD`。只含片⑪、缺片㉜ 的专用源（实测 `m2svr-window-origin
     @6dba8719d`）必须判 RED —— 这正是「窗口构建源过期」的 fail-closed 门。
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

用法：
  python -I -B check_rust_window_preflight.py \
      --repo E:/Users/gxh/Documents/GitHub/OpenMir2 [--ref HEAD] \
      --mirserver E:/MirServer \
      --points "291,666:outside;291,651:outside;289,618:inside;287,620:inside" \
      [--fix-commit <oid>] [--map-name 0] [--map-id 26]
  python -I -B check_rust_window_preflight.py --selftest

退出码：`0` = GREEN、`1` = RED（判据不符）、`2` = PREREQ_RED（缺输入/解析不到 git）。
`--selftest` 不依赖外部件（临时 git 仓 + 合成 main.rs/MapInfo + 合成 mongen 检查器）。
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile

# 片⑪：`fix(game-svr): AOI 诊断 vis 语义（当前可见集，不含自身）+ vr/new/af 读数`
FIX_COMMIT_DIAG = "d16b5199d98029081daccd9252f81b94a9596eaf"
# 片㉜：`fix(game-svr): 受击帧按 oracle SendRefMsg(RM_STRUCK) 广播给视野内全体 Play`
#   —— DoD③ 第三半「屏幕见怪并能攻击造成伤害」的判据（`check_attack_observability.py`）要靠它。
FIX_COMMIT_STRUCK = "0d0d62a695082730dd2398e6d35bf40c8f70480a"
# R1 默认要求源同时含两片；`--fix-commit` 可重复覆盖。
REQUIRED_FIX_COMMITS: tuple[tuple[str, str], ...] = (
    (FIX_COMMIT_DIAG, "片⑪ AOI 诊断 vr/new/af"),
    (FIX_COMMIT_STRUCK, "片㉜ 受击帧 SM_STRUCK 广播"),
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
            "required-fixes-default-two",
            REQUIRED_FIX_COMMITS[0][0] == FIX_COMMIT_DIAG
            and REQUIRED_FIX_COMMITS[1][0] == FIX_COMMIT_STRUCK,
            repr(REQUIRED_FIX_COMMITS),
        )
        val, _ = is_ancestor(tmp, "0" * 40, "HEAD")
        chk("git-ancestor-unknown-prereq", val is None, repr(val))
        val, _ = is_ancestor(os.path.join(tmp, "nope"), first, "HEAD")
        chk("git-not-a-repo-prereq", val is None, repr(val))

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


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--repo", default="", help="Rust 构建源检出（默认从本脚本位置向上找 .git）")
    ap.add_argument("--ref", default="HEAD", help="被检查的 ref/oid（默认 HEAD）")
    ap.add_argument(
        "--fix-commit",
        action="append",
        default=None,
        help="必须为 --ref 祖先的修复提交（可重复；默认 = 片⑪+片㉜；单值 = 只查那一个）",
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
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    script_dir = os.path.dirname(os.path.abspath(__file__))
    mongen = args.mongen or os.path.join(args.mirserver, "M2GameSvr", "Envir", "MonGen.txt")
    mapinfo = args.mapinfo or os.path.join(args.mirserver, "M2GameSvr", "Envir", "MapInfo.txt")

    problems: list[str] = []
    prereq: list[str] = []

    # R1 构建源含全部必需修复提交
    fixes = [(c, "") for c in args.fix_commit] if args.fix_commit else list(REQUIRED_FIX_COMMITS)
    repo = args.repo or find_repo_root(script_dir) or ""
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

    for line in problems:
        print(line)
    for line in prereq:
        print(line)
    rc = 0 if not problems and not prereq else (1 if problems else 2)
    print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
