#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_mongen_dense_points.py —— 卡 `openmir2-client-e2e-enter-gaps` 缺口②「同屏没怪」
**离线判据**：给定地图名 + 若干坐标，判定该点是否落在 `Envir/MonGen.txt` 的**生成框**内，
并给出到最近生成框的 Chebyshev 距离（机判、可复跑；替代 `71-…txt` 那份一次性转录）。

oracle 锚点（生成框 = 正方形，不是圆/菱形）：
  `src/GameSrv/Word/WorldServer.MonGen.cs:825-826`
      nX = monGen.X - monGen.Range + Random(monGen.Range * 2 + 1);
      nY = monGen.Y - monGen.Range + Random(monGen.Range * 2 + 1);
  （另 `:849-850` 同式）⇒ 出生点均匀落在 `[X-R, X+R] x [Y-R, Y+R]` 的正方形内。
  ⇒ 「点在框外且 Chebyshev 距离 = d」表示**新出生**的怪至少离该点 d 格；
     观察者 `view_range < d` 时，该点窗口内不可能有「出生即看见」的怪（游走怪另算）。

用法：
  # 单点判定（inside/outside 必须与 --expect 一致才 GREEN）
  python -I -B check_mongen_dense_points.py --mongen <Envir/MonGen.txt> --map 0 \
      --point 291,666 --expect outside
  # 批量报告（只打印，不给判据）
  python -I -B check_mongen_dense_points.py --mongen <...> --map 0 \
      --points "291,666;287,620;289,618" --view-range 12
  # oracle 出生点是否在生成框内（新角色默认体验）
  python -I -B check_mongen_dense_points.py --mongen <...> --start-point <Envir/StartPoint.txt> --map 0
  python -I -B check_mongen_dense_points.py --selftest

退出码：`0` = GREEN / `1` = RED（--expect 不符）/ `2` = PREREQ_RED（缺输入/解析不到）。
`--selftest` 不依赖外部件（合成 MonGen/StartPoint 文本；覆盖 inside/outside/边界与缺件）。
"""

from __future__ import annotations

import argparse
import os
import sys

# MonGen.txt 头（GBK）：`;地图 x y 名称 范围 数量 时间 分钟比率 分钟模式 分钟点X 分钟点Y`
MONGEN_FIELDS = 6


def decode_text(raw: bytes) -> str:
    """与 `crates/envir` 的 `decode_text` 同口径（线上是 GBK；失败不抛）。"""
    for enc in ("gbk", "utf-8"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("gbk", errors="replace")


def parse_mongen(text: str, map_name: str) -> list[dict]:
    """抽 `map_name` 的所有生成框；跳过注释/空行/字段不足/范围解析失败的行。"""
    boxes: list[dict] = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith(";"):
            continue
        parts = line.split()
        if len(parts) < MONGEN_FIELDS:
            continue
        if parts[0] != map_name:
            continue
        try:
            x = int(parts[1])
            y = int(parts[2])
            rng = int(parts[4])
            count = int(parts[5])
        except ValueError:
            continue
        if rng < 0:
            continue
        boxes.append({"x": x, "y": y, "range": rng, "count": count, "name": parts[3]})
    return boxes


def parse_start_points(text: str, map_name: str) -> list[tuple[int, int]]:
    """`Envir/StartPoint.txt`：`<map> <x> <y>`（注释 `;`）。"""
    pts: list[tuple[int, int]] = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith(";"):
            continue
        parts = line.split()
        if len(parts) < 3 or parts[0] != map_name:
            continue
        try:
            pts.append((int(parts[1]), int(parts[2])))
        except ValueError:
            continue
    return pts


def cheb(ax: int, ay: int, bx: int, by: int) -> int:
    return max(abs(ax - bx), abs(ay - by))


def nearest_box(boxes: list[dict], x: int, y: int) -> tuple[int, dict | None]:
    """返回 `(Chebyshev 距离, 最近框)`；距离 0 ⇒ 框内（取 range 最小的那个框作代表）。"""
    best_d = None
    best = None
    for b in boxes:
        d = cheb(x, y, b["x"], b["y"]) - b["range"]
        if d < 0:
            d = 0
        if best_d is None or d < best_d or (d == best_d and b["range"] < best["range"]):
            best_d = d
            best = b
    return (-1 if best_d is None else best_d), best


def report_point(boxes: list[dict], x: int, y: int, view_range: int | None) -> tuple[bool, str]:
    d, b = nearest_box(boxes, x, y)
    if b is None:
        return False, f"({x},{y}) 无任何生成框（该图 MonGen 为空？）"
    inside = d == 0
    tag = "框内" if inside else "框外"
    line = (
        f"({x},{y}) {tag}；到最近生成框 Chebyshev={d} "
        f"（gen=({b['x']},{b['y']}) range={b['range']} count={b['count']} name={b['name']}）"
    )
    if view_range is not None:
        if d == 0:
            line += f"；view_range={view_range} 覆盖最近框 ⇒ 可能有出生怪在窗口内"
        elif d <= view_range:
            line += f"；view_range={view_range} >= d ⇒ **框边缘**，出生怪可能落在窗口内（需窗口实拍）"
        else:
            line += (
                f"；view_range={view_range} < d ⇒ 新出生的怪必在窗口外 "
                "（游走怪另算；真机 `nearest` 可能更小）"
            )
    return inside, line


def run(
    boxes: list[dict], points: list[tuple[int, int]], expect: str, view_range: int | None
) -> tuple[int, list[str]]:
    msgs: list[str] = []
    if not points:
        return 2, ["PREREQ_RED: 没有待判定的坐标（--point/--points/--start-point 至少一个）"]
    if not boxes:
        return 2, ["PREREQ_RED: 该地图在 MonGen 里解析不到任何生成框（地图名写错？）"]

    bad = 0
    for (x, y) in points:
        inside, line = report_point(boxes, x, y, view_range)
        msgs.append("INFO " + line)
        if expect == "inside" and not inside:
            bad += 1
            msgs.append(f"RED --expect inside 但 ({x},{y}) 在框外")
        elif expect == "outside" and inside:
            bad += 1
            msgs.append(f"RED --expect outside 但 ({x},{y}) 在框内")

    if bad:
        msgs.append(f"RED {bad} 个坐标与 --expect {expect} 不符")
        return 1, msgs
    if expect != "report":
        msgs.append(f"OK {len(points)} 个坐标全部与 --expect {expect} 一致")
    return 0, msgs


def selftest() -> int:
    # 合成 MonGen：一个 20 格正方框（以 (100,100) 为中心）+ 一个 5 格框（(200,200)）。
    mongen = (
        ";地图 x y 名称 范围 数量 时间\n"
        "0\t100\t100\t鹿\t20\t50\t1\n"
        "0\t200\t200\t稻草人\t5\t10\t1\n"
        "4\t1\t1\t别图\t9\t1\t1\n"
    )
    startpoint = ";安全区\n0\t110\t110\n1\t999\t999\n"
    boxes = parse_mongen(mongen, "0")
    assert len(boxes) == 2, boxes
    assert parse_mongen(mongen, "4")[0]["range"] == 9
    assert parse_mongen(";only comment\n", "0") == []

    sp = parse_start_points(startpoint, "0")
    assert sp == [(110, 110)], sp
    assert parse_start_points(startpoint, "9") == []

    cases = [
        # name, points, expect, view_range, want_rc
        ("inside-center", [(100, 100)], "inside", 12, 0),
        ("inside-corner", [(120, 120)], "inside", 12, 0),   # Chebyshev 20 == range
        ("outside-corner+1", [(121, 120)], "outside", 12, 0),
        ("outside-far", [(291, 666)], "outside", 12, 0),
        ("outside-but-expect-inside", [(291, 666)], "inside", 12, 1),
        ("inside-but-expect-outside", [(100, 100)], "outside", 12, 1),
        ("edge-viewrange-equal", [(125, 100)], "outside", 12, 0),  # d=5, view_range 12 >= d
        ("report-no-judge", [(0, 0)], "report", 12, 0),
    ]
    bad = 0
    for name, pts, expect, vr, want_rc in cases:
        rc, _ = run(boxes, pts, expect, vr)
        ok = rc == want_rc
        if not ok:
            bad += 1
        print(f"[{'PASS' if ok else 'FAIL'}] selftest {name}: rc={rc} (want {want_rc})")

    # 边界：空框/空点 ⇒ PREREQ(2)。
    rc, _ = run([], [(1, 1)], "inside", 12)
    ok = rc == 2
    bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest empty-boxes-prereq: rc={rc} (want 2)")
    rc, _ = run(boxes, [], "inside", 12)
    ok = rc == 2
    bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest no-points-prereq: rc={rc} (want 2)")

    # 最近框选择：距离相同时取 range 最小的框（避免把大框当代表）。
    d, b = nearest_box(boxes, 120, 120)
    assert d == 0 and b["range"] == 20, (d, b)
    d, b = nearest_box(boxes, 300, 300)
    assert d == 95 and b["range"] == 5, (d, b)  # (200,200,r5): cheb=100-5=95；(100,100,r20): 200-20=180

    total = len(cases) + 2
    print(f"SELFTEST {'OK' if bad == 0 else 'FAILED'} ({total - bad}/{total})")
    return 0 if bad == 0 else 1


def _read(path: str) -> str | None:
    if not os.path.isfile(path):
        return None
    try:
        with open(path, "rb") as fh:
            return decode_text(fh.read())
    except OSError:
        return None


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mongen", default="", help="Envir/MonGen.txt 路径")
    ap.add_argument("--start-point", default="", help="可选：Envir/StartPoint.txt（其点并入待判定集合）")
    ap.add_argument("--map", default="0", help="地图名（逻辑名，`|` 前；例如 `0`）")
    ap.add_argument("--point", default="", help="单点 `x,y`")
    ap.add_argument("--points", default="", help="多点 `x,y;x,y;...`")
    ap.add_argument("--expect", choices=["report", "inside", "outside"], default="report")
    ap.add_argument("--view-range", type=int, default=None, help="观察者视野半径（打印窗口关系）")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if not args.mongen:
        print("PREREQ_RED: 需要 --mongen <Envir/MonGen.txt>（或 --selftest）")
        return 2
    text = _read(args.mongen)
    if text is None:
        print(f"PREREQ_RED: MonGen 不存在/不可读: {args.mongen}")
        return 2

    boxes = parse_mongen(text, args.map)
    points: list[tuple[int, int]] = []
    for chunk in (args.point, args.points):
        for item in chunk.split(";"):
            item = item.strip()
            if not item:
                continue
            try:
                xs, ys = item.split(",")
                points.append((int(xs), int(ys)))
            except ValueError:
                print(f"PREREQ_RED: 坐标格式错误: {item!r}（应为 `x,y`）")
                return 2

    if args.start_point:
        sp_text = _read(args.start_point)
        if sp_text is None:
            print(f"PREREQ_RED: StartPoint 不存在/不可读: {args.start_point}")
            return 2
        for (x, y) in parse_start_points(sp_text, args.map):
            points.append((x, y))

    rc, msgs = run(boxes, points, args.expect, args.view_range)
    print(f"INFO map={args.map} 生成框 {len(boxes)} 个；待判定坐标 {len(points)} 个")
    for m in msgs:
        print(m)
    print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
