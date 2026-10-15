#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_reverse_point_clean.py — ③a 反向对照点「净空」fail-closed 判据（片68，2026-10-15）。

卡：`openmir2-client-e2e-enter-gaps`。

## 为什么需要
③a 反向对照点的判据是「该点两侧都不应看到怪（`--expect none`）」。片67 的 (291,666)
在 map 0 的 76 个 MonGen 生成框**之外**，却仍被 C# 侧判出 1 对出现帧——
actor `0x0f336101` 经归因为 **`Envir/GuardList.txt` 第 28 行『卫士 0 281, 661 : 5』的确定性守卫**
（SM_TURN param=281/tag=661 逐格同、series 低字节 Dir=5 同、全程无移动帧）；
Rust 侧未实现 GuardList 加载 ⇒ 该点对 Rust「净空」、对 C# 不净空 ⇒ 反向对照被已知缺口污染。

⇒ 反向对照点必须**机械地**证明「净空」：距 map 上**全部**确定性对象（MonGen 生成框、
GuardList 卫士、Merchant 商人、Npcs）都 > vr（AOI 视距，切比雪夫）。本工具就是这个判据。

## 用法
  python -I -B check_reverse_point_clean.py --point 291,666 --map 0 --vr 12
  python -I -B check_reverse_point_clean.py --scan-box 280,650,310,690 --map 0 --vr 12
  python -I -B check_reverse_point_clean.py --selftest

退出码：`0` CLEAN / `1` NOT_CLEAN（或 fail-closed RED）/ `2` PREREQ_RED。
`--selftest` 不依赖任何外部件（临时目录自造 envir）。

## 口径
- 距离 = **切比雪夫**（`max(|dx|,|dy|)`），与 AOI 视距同口径。
- MonGen：`地图 x y 怪物名 范围 数量 …`（`LocalDB.cs:532-557`，列序见 `crates/envir/src/mongen.rs`）
  ⇒ 生成框 = 以 (x,y) 为心、切比雪夫半径 `range`；`range=0` 视为单格。
- GuardList：`怪物名 地图 "X, Y : dir"`（`LocalDB.cs:100-141`）；卫士静止、无范围。
- Merchant：`NPC名 地图 x y …`；Npcs（若存在）同形。
- `loadgen` 子文件**不展开**：oracle 的判定实参写反 ⇒ 子文件包含实际不可达（BUG-REPLICA，
  见 `crates/envir/src/mongen.rs` 模块头）⇒ 与 oracle 一致地**不**展开。
- fail-closed：四个源文件里有缺失 ⇒ RED（`--allow-missing-sources` 可降级为 WARN，仅诊断用）。
"""

from __future__ import annotations

import argparse
import os
import sys
import tempfile

SOURCES = ("MonGen.txt", "GuardList.txt", "Merchant.txt", "Npcs.txt")


def _read_lines(path: str) -> list[str]:
    # Envir 文本是 GB2312/GBK 字节流；只取数字与 ASCII 结构 ⇒ errors='replace' 足够。
    with open(path, "rb") as fh:
        raw = fh.read()
    return raw.decode("gbk", "replace").splitlines()


def _tokens(line: str) -> list[str]:
    return [t for t in line.replace("\t", " ").split(" ") if t != ""]


def _int(tok: str, default: int = 0) -> int:
    # GuardList 的坐标 token 带尾逗号（`281, 661`）⇒ 先剥掉 ,:" 再转。
    t = tok.strip().strip(',:"').strip()
    try:
        return int(t)
    except (TypeError, ValueError):
        return default


def parse_mongen(lines: list[str], map_name: str) -> list[tuple[int, int, int]]:
    """⇒ [(x, y, range)]。列序：地图 x y 怪物名 范围 …（`LocalDB.cs:532-557`）。"""
    out: list[tuple[int, int, int]] = []
    for line in lines:
        if not line or line.startswith(";"):
            continue
        tk = _tokens(line)
        if len(tk) < 5 or tk[0] != map_name:
            continue
        x, y = _int(tk[1]), _int(tk[2])
        rng = _int(tk[4])
        out.append((x, y, max(rng, 0)))
    return out


def parse_guardlist(lines: list[str], map_name: str) -> list[tuple[int, int, int]]:
    """`怪物名 地图 "X, Y : dir"` ⇒ [(x, y, 0)]。"""
    out: list[tuple[int, int, int]] = []
    for line in lines:
        if not line or line.startswith(";"):
            continue
        tk = _tokens(line)
        if len(tk) < 4 or tk[1] != map_name:
            continue
        # "281," "661" ": " "5" —— 取前两个纯数字 token
        nums = [_int(t, -1) for t in tk[2:]]
        nums = [n for n in nums if n >= 0]
        if len(nums) >= 2:
            out.append((nums[0], nums[1], 0))
    return out


def parse_merchant(lines: list[str], map_name: str) -> list[tuple[int, int, int]]:
    """`NPC名 地图 x y …` ⇒ [(x, y, 0)]。"""
    out: list[tuple[int, int, int]] = []
    for line in lines:
        if not line or line.startswith(";"):
            continue
        tk = _tokens(line)
        if len(tk) < 4 or tk[1] != map_name:
            continue
        out.append((_int(tk[2]), _int(tk[3]), 0))
    return out


PARSERS = {
    "MonGen.txt": parse_mongen,
    "GuardList.txt": parse_guardlist,
    "Merchant.txt": parse_merchant,
    "Npcs.txt": parse_merchant,
}


def cheb(ax: int, ay: int, bx: int, by: int) -> int:
    return max(abs(ax - bx), abs(ay - by))


def load_entries(envir_dir: str, map_name: str,
                 allow_missing: bool) -> tuple[list[tuple[str, int, int, int]] | None, list[str]]:
    """⇒ ([(source, x, y, radius)], 错误/告警列表)。缺源文件 = fail-closed（除非 allow_missing）。"""
    entries: list[tuple[str, int, int, int]] = []
    problems: list[str] = []
    if not os.path.isdir(envir_dir):
        return None, ["envir 目录不存在：%s" % envir_dir]
    for name in SOURCES:
        path = os.path.join(envir_dir, name)
        if not os.path.isfile(path):
            if name == "Npcs.txt":
                continue  # 部署可不带该文件（它不在 GuardList/Merchant 的必查面内）
            msg = "缺源文件 %s（清空判据依赖全部确定性对象的完整清单）" % path
            if allow_missing:
                problems.append("WARN: " + msg)
            else:
                return None, [msg]
            continue
        for (x, y, rng) in PARSERS[name](_read_lines(path), map_name):
            entries.append((name, x, y, rng))
    return entries, problems


def nearest(entries: list[tuple[str, int, int, int]], x: int, y: int) -> tuple[str, int, int, int, int] | None:
    """⇒ (source, ex, ey, radius, effective_distance) 取「有效距离 = 切比雪夫 − 半径」最小者。"""
    best = None
    for (src, ex, ey, rng) in entries:
        d = cheb(x, y, ex, ey) - rng
        if best is None or d < best[4]:
            best = (src, ex, ey, rng, d)
    return best


def verdict(entries: list[tuple[str, int, int, int]], x: int, y: int, vr: int) -> tuple[bool, str]:
    n = nearest(entries, x, y)
    if n is None:
        return True, "无任何确定性对象（trivially clean）"
    src, ex, ey, rng, d = n
    inside = d <= vr
    desc = ("最近对象 = %s (%d,%d) range=%d，切比雪夫=%d；有效距离=%d vs vr=%d"
            % (src, ex, ey, rng, d + rng, d, vr))
    return (not inside), desc


def selftest() -> int:
    ok = True

    def case(name: str, got: bool, want: bool) -> None:
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-58s %s" % (name, "OK" if good else "FAIL"))

    with tempfile.TemporaryDirectory() as td:
        def write(name: str, text: str) -> None:
            with open(os.path.join(td, name), "w", encoding="gbk") as fh:
                fh.write(text)

        # 片67 实况子集：卫士 (281,661) dir 5；MonGen 框 (300,700) range 5；商人 (290,690)
        write("GuardList.txt", ";comment\n\u536b\u58eb   0        281, 661    : 5\n\u536b\u58eb   0        275, 212    : 7\n")
        write("MonGen.txt", ";\u5934\n0\t300\t700\t\u9e70\t5\t10\t360\n")
        write("Merchant.txt", "NPC\t0\t290\t690\tx\t0\t27\n")
        entries, problems = load_entries(td, "0", False)
        case("四源齐备 ⇒ 无 problem", entries is not None and not problems, True)
        assert entries is not None
        clean, desc = verdict(entries, 291, 666, 12)
        case("★片67 反向点 (291,666) ⇒ NOT_CLEAN（卫士 (281,661) d=10 ≤ 12）", clean, False)
        case("★not-clean 描述点名 GuardList", "GuardList" in desc, True)
        clean, _ = verdict(entries, 341, 666, 12)
        case("远处点 (341,666) ⇒ CLEAN", clean, True)
        clean, desc2 = verdict(entries, 303, 697, 12)
        case("★MonGen 框内点 (303,697) ⇒ NOT_CLEAN（框半径参与）", clean, False)
        case("★not-clean 描述点名 MonGen", "MonGen" in desc2, True)

        # 牙（改坏必红本体）：把 GuardList 拿掉 ⇒ (291,666) 变 CLEAN ⇒ 证明该源是承重的
        with tempfile.TemporaryDirectory() as td2:
            for nm in ("MonGen.txt", "Merchant.txt"):
                with open(os.path.join(td, nm), "rb") as s, open(os.path.join(td2, nm), "wb") as d:
                    d.write(s.read())
            with open(os.path.join(td2, "GuardList.txt"), "w", encoding="gbk") as fh:
                fh.write(";empty\n")
            e2, _ = load_entries(td2, "0", False)
            assert e2 is not None
            clean, _ = verdict(e2, 291, 666, 12)
            case("★牙：抽掉 GuardList ⇒ (291,666) 变 CLEAN（该源承重）", clean, True)

        # fail-closed：缺 MonGen.txt ⇒ RED；--allow-missing-sources ⇒ 降级
        with tempfile.TemporaryDirectory() as td3:
            with open(os.path.join(td3, "GuardList.txt"), "w", encoding="gbk") as fh:
                fh.write(";x\n")
            e3, probs3 = load_entries(td3, "0", False)
            case("★缺 MonGen.txt ⇒ fail-closed（entries=None）", e3 is None and bool(probs3), True)
            e4, probs4 = load_entries(td3, "0", True)
            case("★同缺 + allow-missing ⇒ 降级为 WARN", e4 is not None and any("WARN" in p for p in probs4), True)

    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="③a 反向对照点净空判据（fail-closed）")
    ap.add_argument("--point", default="", help="反向对照点 \"x,y\"")
    ap.add_argument("--scan-box", default="", help="在矩形内找净空点：\"x0,y0,x1,y1\"")
    ap.add_argument("--map", dest="map_name", default="0")
    ap.add_argument("--vr", type=int, default=12, help="AOI 视距（切比雪夫，默认 12）")
    ap.add_argument("--envir-dir", default="E:/MirServer/M2GameSvr/Envir")
    ap.add_argument("--max-suggest", type=int, default=8)
    ap.add_argument("--allow-missing-sources", action="store_true",
                    help="缺源文件降级为 WARN（仅诊断；正式判据禁用）")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    entries, problems = load_entries(args.envir_dir, args.map_name, args.allow_missing_sources)
    for p in problems:
        print(p)
    if entries is None:
        print("PREREQ_MISSING: %s" % (problems[0] if problems else "envir 加载失败"))
        print("RESULT: PREREQ_RED")
        return 2

    if args.point:
        try:
            x, y = [int(v) for v in args.point.split(",")[:2]]
        except ValueError:
            print("PREREQ_MISSING: --point 需 \"x,y\"")
            print("RESULT: PREREQ_RED")
            return 2
        clean, desc = verdict(entries, x, y, args.vr)
        n = nearest(entries, x, y)
        print("点 (%d,%d) map=%s vr=%d：%s" % (x, y, args.map_name, args.vr, desc))
        if n is not None:
            print("  最近对象明细：source=%s (%d,%d) range=%d 有效距离=%d" % n)
        if clean:
            print("VERDICT: CLEAN")
            print("RESULT: CLEAN")
            return 0
        print("VERDICT: NOT_CLEAN（该点会被已知确定性对象污染，不能作反向对照点）")
        print("RESULT: NOT_CLEAN")
        return 1

    if args.scan_box:
        try:
            x0, y0, x1, y1 = [int(v) for v in args.scan_box.split(",")[:4]]
        except ValueError:
            print("PREREQ_MISSING: --scan-box 需 \"x0,y0,x1,y1\"")
            print("RESULT: PREREQ_RED")
            return 2
        cands: list[tuple[int, int, int]] = []
        for x in range(x0, x1 + 1):
            for y in range(y0, y1 + 1):
                n = nearest(entries, x, y)
                eff = 10 ** 9 if n is None else n[4]
                if eff > args.vr:
                    cands.append((eff, x, y))
        cands.sort(reverse=True)
        print("scan-box (%d,%d)-(%d,%d) map=%s vr=%d：净空格 %d 个"
              % (x0, y0, x1, y1, args.map_name, args.vr, len(cands)))
        for eff, x, y in cands[:args.max_suggest]:
            print("  SUGGEST (%d,%d) 有效距离=%d" % (x, y, eff))
        if not cands:
            print("RESULT: NOT_CLEAN（该矩形内无净空点）")
            return 1
        print("RESULT: CLEAN")
        return 0

    print("PREREQ_MISSING: 需给 --point 或 --scan-box（或 --selftest）")
    print("RESULT: PREREQ_RED")
    return 2


if __name__ == "__main__":
    sys.exit(main())
