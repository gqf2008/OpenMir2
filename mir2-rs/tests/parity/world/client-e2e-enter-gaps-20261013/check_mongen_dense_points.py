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
  # 窗口选点（root ③①「换怪密点」）：按生成框中心列出窗口内期望出生怪数 Top-N
  python -I -B check_mongen_dense_points.py --mongen <...> --map 0 --view-range 12 --rank 10
  python -I -B check_mongen_dense_points.py --mongen <...> --map 0 --view-range 12 --best-point
  # 跨**所有地图**排行（root ③①「MonGen 里密度高的图」）：同口径合并各图候选点
  python -I -B check_mongen_dense_points.py --mongen <...> --all-maps --view-range 12 --rank 20
  python -I -B check_mongen_dense_points.py --mongen <...> --all-maps --view-range 12 --best-point
  # 机判：该点窗口内期望出生怪数 >= / <= 阈值（③b「有怪」/ ③a「确实无怪」）
  python -I -B check_mongen_dense_points.py --mongen <...> --map 0 --view-range 12 \
      --point 291,666 --max-expected 0
  python -I -B check_mongen_dense_points.py --mongen <...> --map 0 --view-range 12 \
      --points "289,618;287,620" --min-expected 1
  # 可进入性（cross-map 候选「可进入性未证」的机判）：按 oracle `FindMap` 口径
  # （逻辑名 = `|` 前）在 `Envir/MapInfo.txt` 查表；缺 --mapinfo 时输出与旧版逐字节相同。
  python -I -B check_mongen_dense_points.py --mongen <...> --mapinfo <Envir/MapInfo.txt> \
      --all-maps --view-range 12 --rank 20
  # 机判：排行里出现 MapInfo 未加载的图 ⇒ RED（防窗口选到不可进入的图）
  python -I -B check_mongen_dense_points.py --mongen <...> --mapinfo <...> \
      --all-maps --view-range 12 --rank 20 --require-enterable
  python -I -B check_mongen_dense_points.py --selftest

退出码：`0` = GREEN / `1` = RED（--expect 或 --min/max-expected 不符）/ `2` = PREREQ_RED（缺输入/解析不到）。
`--rank`/`--best-point` 是**报告**模式（只打印候选与期望值，`0`/`2`）；期望值口径见
`expected_in_window()`（只算**出生**，不含游走）。`--selftest` 不依赖外部件（合成 MonGen/StartPoint
文本；覆盖 inside/outside/边界、缺件、期望值、排行与 min/max 判据）。
"""

from __future__ import annotations

import argparse
import os
import re
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


def parse_mapinfo(text: str) -> set[str]:
    """`Envir/MapInfo.txt` 的**可进入逻辑名**集合：`[...]` 行首字段 `|` 前的部分。

    oracle 口径：`MapMgr.FindMap(MapName)` 按逻辑名（`|` 前）查表
    （`[DB|DM011]` ⇒ 逻辑名 `DB`；见 `docs/` ⑤-2 裁决）。跨图选点若命中不在本集合的图，
    oracle 侧不会加载它 ⇒ 窗口里选到就是「该图无怪」，属可离线预防的烧窗。
    """
    keys: set[str] = set()
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("["):
            continue
        m = re.match(r"\[([^\]\s]+)", line)
        if not m:
            continue
        keys.add(m.group(1).split("|")[0])
    return keys


def is_enterable(map_name: str, mapinfo_keys: set[str]) -> bool:
    """MonGen 的图字段是否在 MapInfo 逻辑名里（`|` 前口径，与 oracle `FindMap` 同）。"""
    return map_name.split("|")[0] in mapinfo_keys


def enterable_suffix(map_name: str, mapinfo_keys: set[str] | None) -> str:
    """追加 `enterable=yes/no`；未给 `--mapinfo` 时返回空串（旧输出逐字节不变）。"""
    if mapinfo_keys is None:
        return ""
    return " enterable=" + ("yes" if is_enterable(map_name, mapinfo_keys) else "no")


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


def expected_in_window(boxes: list[dict], x: int, y: int, view_range: int) -> float:
    """**出生视角**的窗口内期望怪数（非实机读数）：

    每个生成框按 oracle `WorldServer.MonGen.cs:825-826` 在 `[X-R, X+R] x [Y-R, Y+R]`
    的正方形上**均匀**出生、维持 `Count` 只；观察者窗口 = 以 `(x,y)` 为中心、半径 `view_range`
    的**方格**（同一 Chebyshev 口径）。⇒

        E = Σ_box  Count * |窗口 ∩ 框| / |框|

    纯几何、确定性、可复算（无 RNG）。**只算出生**：游走怪可能把实机 `nearest` 压得更小，
    故 `E == 0` 只表示「窗口内不会有**新出生**的怪」，不排除游走怪（见 `70-…txt` §C）。
    """
    total = 0.0
    for b in boxes:
        r = b["range"]
        side = 2 * r + 1
        ox = min(x + view_range, b["x"] + r) - max(x - view_range, b["x"] - r) + 1
        oy = min(y + view_range, b["y"] + r) - max(y - view_range, b["y"] - r) + 1
        if ox <= 0 or oy <= 0:
            continue
        total += b["count"] * (ox * oy) / (side * side)
    return total


def rank_points(boxes: list[dict], view_range: int) -> list[tuple[float, int, int, dict]]:
    """候选点 = 每个**去重后的生成框中心**；按窗口内期望怪数降序（并列按 x,y 升序）。

    返回 `[(expected, x, y, box), …]`，确定性（不依赖 dict/set 迭代序）。
    """
    centers: list[tuple[int, int]] = []
    seen: set[tuple[int, int]] = set()
    for b in sorted(boxes, key=lambda b: (b["x"], b["y"], b["range"], b["count"])):
        c = (b["x"], b["y"])
        if c not in seen:
            seen.add(c)
            centers.append(c)
    ranked = []
    for (x, y) in centers:
        e = expected_in_window(boxes, x, y, view_range)
        d, b = nearest_box(boxes, x, y)
        ranked.append((e, x, y, b))
    ranked.sort(key=lambda t: (-t[0], t[1], t[2]))
    return ranked


def all_map_names(text: str) -> list[str]:
    """MonGen.txt 里出现过的地图名（`|` 前的原样字段），**字典序**去重。

    与 `parse_mongen` 同口径：跳过注释/空行/字段不足/范围解析失败的行；
    这样 `--all-maps` 的图集合与逐图 `parse_mongen` 的可行集一致。
    """
    names: set[str] = set()
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith(";"):
            continue
        parts = line.split()
        if len(parts) < MONGEN_FIELDS:
            continue
        try:
            rng = int(parts[4])
            int(parts[5])
        except ValueError:
            continue
        if rng < 0:
            continue
        names.add(parts[0])
    return sorted(names)


def rank_all_maps(text: str, view_range: int) -> list[tuple[float, str, int, int, dict]]:
    """跨**所有地图**的窗口内期望出生怪数排行（root ③①「MonGen 里密度高的图」）。

    每图取 `rank_points`（按生成框中心），合并后按 `(-E, map, x, y)` 降序 ——
    **确定性**：不依赖 set/dict 迭代序、也不依赖 MonGen 行序（图名先字典序、再坐标）。
    返回 `[(expected, map_name, x, y, box), …]`。
    """
    ranked: list[tuple[float, str, int, int, dict]] = []
    for mp in all_map_names(text):
        boxes = parse_mongen(text, mp)
        if not boxes:
            continue
        for e, x, y, b in rank_points(boxes, view_range):
            ranked.append((e, mp, x, y, b))
    ranked.sort(key=lambda t: (-t[0], t[1], t[2], t[3]))
    return ranked


def require_enterable_check(
    map_names: list[str], mapinfo_keys: set[str] | None, require: bool
) -> tuple[int, list[str]]:
    """`--require-enterable` 机判：任一候选图不在 MapInfo 逻辑名 ⇒ RED(1)；未给 mapinfo ⇒ PREREQ(2)。

    只对**被报告/被选中的**候选图判定（避免把全库 200+ 图都拉进来）。
    """
    if not require:
        return 0, []
    if mapinfo_keys is None:
        return 2, ["PREREQ_RED: --require-enterable 需要 --mapinfo <Envir/MapInfo.txt>"]
    uniq = sorted(set(map_names))
    missing = [m for m in uniq if not is_enterable(m, mapinfo_keys)]
    if missing:
        return 1, [f"RED --require-enterable：候选图不在 MapInfo 逻辑名内: {missing}"]
    return 0, [f"OK  --require-enterable：候选图全部在 MapInfo 逻辑名内（{len(uniq)} 个）"]


def run(
    boxes: list[dict],
    points: list[tuple[int, int]],
    expect: str,
    view_range: int | None,
    min_expected: float | None = None,
    max_expected: float | None = None,
) -> tuple[int, list[str]]:
    msgs: list[str] = []
    if not points:
        return 2, ["PREREQ_RED: 没有待判定的坐标（--point/--points/--start-point 至少一个）"]
    if not boxes:
        return 2, ["PREREQ_RED: 该地图在 MonGen 里解析不到任何生成框（地图名写错？）"]
    if (min_expected is not None or max_expected is not None) and view_range is None:
        return 2, ["PREREQ_RED: --min-expected/--max-expected 需要 --view-range"]

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
        if view_range is not None and (min_expected is not None or max_expected is not None):
            e = expected_in_window(boxes, x, y, view_range)
            msgs.append(
                f"INFO ({x},{y}) 窗口内期望出生怪数 E={e:.3f}（view_range={view_range}；只算出生）"
            )
            if min_expected is not None and e < min_expected:
                bad += 1
                msgs.append(f"RED ({x},{y}) E={e:.3f} < --min-expected {min_expected}")
            if max_expected is not None and e > max_expected:
                bad += 1
                msgs.append(f"RED ({x},{y}) E={e:.3f} > --max-expected {max_expected}")

    if bad:
        msgs.append(f"RED {bad} 个坐标与判据不符")
        return 1, msgs
    if expect != "report":
        msgs.append(f"OK {len(points)} 个坐标全部与 --expect {expect} 一致")
    elif min_expected is None and max_expected is None:
        pass
    else:
        msgs.append("OK 期望出生怪数判据全部通过")
    return 0, msgs


def selftest() -> int:
    # 合成 MonGen：一个 20 格正方框（以 (100,100) 为中心）+ 一个 5 格框（(200,200)）。
    mongen = (
        ";地图 x y 名称 范围 数量 时间\n"
        "0\t100\t100\t鹿\t20\t50\t1\n"
        "0\t200\t200\t稻草人\t5\t10\t1\n"
        "4\t1\t1\t别图\t9\t1\t1\n"
        "7\t300\t300\t骷髅\t0\t200\t1\n"
        "1\t100\t100\t鹿\t20\t50\t1\n"
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

    # 跨图排行（root ③①「MonGen 里密度高的图」）：map 7 count=200 比 map 0 更密 ⇒ 全局 Top 属 map 7；
    # map 0/map 1 在 (100,100) 期望值并列 50 ⇒ 按图名字典序 map 0 先于 map 1（确定性并列口径）。
    am_bad = 0
    names = all_map_names(mongen)
    ok = names == ["0", "1", "4", "7"]
    am_bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest all-map-names={names} (want 0,1,4,7)")
    all_rk = rank_all_maps(mongen, 25)
    order_ok = (
        len(all_rk) >= 4
        and all_rk[0][1] == "7"
        and (all_rk[0][2], all_rk[0][3]) == (300, 300)
        and abs(all_rk[0][0] - 200.0) < 1e-9
        and all_rk[1][1] == "0"
        and all_rk[2][1] == "1"
    )
    am_bad += 0 if order_ok else 1
    print(
        "[{}] selftest all-maps-rank-order top={} (want map 7>(200), then map 0, then map 1)".format(
            "PASS" if order_ok else "FAIL",
            [(round(e, 3), mp, x, y) for e, mp, x, y, _ in all_rk[:4]],
        )
    )
    # 全局序必须非增（确定性降序）。
    mono_ok = all(all_rk[i][0] >= all_rk[i + 1][0] for i in range(len(all_rk) - 1))
    am_bad += 0 if mono_ok else 1
    print(f"[{'PASS' if mono_ok else 'FAIL'}] selftest all-maps-monotonic")

    # 最近框选择：距离相同时取 range 最小的框（避免把大框当代表）。
    d, b = nearest_box(boxes, 120, 120)
    assert d == 0 and b["range"] == 20, (d, b)
    d, b = nearest_box(boxes, 300, 300)
    assert d == 95 and b["range"] == 5, (d, b)  # (200,200,r5): cheb=100-5=95；(100,100,r20): 200-20=180

    # 期望出生怪数：合成框 0(100,100,r20,count50) + 0(200,200,r5,count10)。
    exp_cases = [
        ("expected-full-cover", (100, 100, 25), 50.0),      # 窗口覆盖整框 ⇒ E=count
        ("expected-no-overlap", (291, 666, 12), 0.0),       # 窗口不碰任何框 ⇒ E=0
        ("expected-partial", (200, 200, 5), 10.0),          # 半径 5 覆盖 (200,200,r5) 整格
        ("expected-single-tile", (200, 200, 0), 10.0 / 121.0),  # 单格中心
    ]
    exp_bad = 0
    for name, (x, y, vr), want in exp_cases:
        got = expected_in_window(boxes, x, y, vr)
        ok = abs(got - want) < 1e-9
        exp_bad += 0 if ok else 1
        print(f"[{'PASS' if ok else 'FAIL'}] selftest {name}: E={got:.6f} (want {want:.6f})")
    rk = rank_points(boxes, 12)
    ok = bool(rk) and rk[0][1:3] == (100, 100) and all(
        rk[i][0] >= rk[i + 1][0] for i in range(len(rk) - 1)
    )
    exp_bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest rank-order top={rk[0][1:3] if rk else None}")

    # 期望判据：min/max 的绿/红。
    exp_judge = [
        ("minmax-max0-pass", [(291, 666)], "report", 12, None, 0.0, 0),
        ("minmax-min-fail", [(291, 666)], "report", 12, 0.5, None, 1),
        ("minmax-min-pass", [(100, 100)], "report", 25, 50.0, None, 0),
        ("minmax-no-vr-prereq", [(100, 100)], "report", None, 1.0, None, 2),
    ]
    for name, pts, expect, vr, mn, mx, want_rc in exp_judge:
        rc, _ = run(boxes, pts, expect, vr, min_expected=mn, max_expected=mx)
        ok = rc == want_rc
        exp_bad += 0 if ok else 1
        print(f"[{'PASS' if ok else 'FAIL'}] selftest {name}: rc={rc} (want {want_rc})")

    # MapInfo 可进入性（逻辑名 = `|` 前，对齐 oracle `FindMap`）。
    mi_text = (
        "; comment\n"
        "[0 比奇省]\n"
        "[DB|DM011 尸王殿]\n"
        "[ZM6|D5071 祖玛六层]\n"
        "[jiaren|0150 家]\n"
    )
    mi = parse_mapinfo(mi_text)
    mi_bad = 0
    ok = mi == {"0", "DB", "ZM6", "jiaren"}
    mi_bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest mapinfo-keys={sorted(mi)}")
    ok = is_enterable("DB", mi) and is_enterable("ZM6|D5071", mi) and not is_enterable("D10071", mi)
    mi_bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest mapinfo-is-enterable")
    ok = (enterable_suffix("0", None) == ""
          and enterable_suffix("0", mi) == " enterable=yes"
          and enterable_suffix("D10071", mi) == " enterable=no")
    mi_bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest mapinfo-suffix-unchanged-when-absent")
    gate_cases = [
        ("require-enterable-pass", ["0", "DB"], mi, True, 0),
        ("require-enterable-red", ["0", "D10071"], mi, True, 1),
        ("require-enterable-no-mapinfo-prereq", ["0"], None, True, 2),
        ("no-require-always-green", ["D10071"], mi, False, 0),
    ]
    for name, names, keys, require, want_rc in gate_cases:
        rc, _ = require_enterable_check(names, keys, require)
        ok = rc == want_rc
        mi_bad += 0 if ok else 1
        print(f"[{'PASS' if ok else 'FAIL'}] selftest {name}: rc={rc} (want {want_rc})")
    # 未给 --mapinfo 的旧输出不受影响：`enterable=` 不得出现在 all-maps 渲染里。
    legacy_lines = [
        f"RANK ({mp}) ({x},{y}) E={e:.3f} box=({b['x']},{b['y']}) "
        f"range={b['range']} count={b['count']} name={b['name']}"
        f"{enterable_suffix(mp, None)}"
        for e, mp, x, y, b in all_rk[:2]
    ]
    ok = all("enterable=" not in ln for ln in legacy_lines)
    mi_bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest mapinfo-legacy-output-clean")

    total = (len(cases) + 2 + 3 + len(exp_cases) + 1 + len(exp_judge)
             + 3 + len(gate_cases) + 1)
    failed = bad + am_bad + exp_bad + mi_bad
    print(f"SELFTEST {'OK' if failed == 0 else 'FAILED'} ({total - failed}/{total})")
    return 0 if failed == 0 else 1


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
    ap.add_argument("--min-expected", type=float, default=None,
                    help="机判：每个 --point 的窗口内期望出生怪数须 >= 该值（需 --view-range）")
    ap.add_argument("--max-expected", type=float, default=None,
                    help="机判：每个 --point 的窗口内期望出生怪数须 <= 该值（需 --view-range）")
    ap.add_argument("--rank", type=int, default=0, metavar="N",
                    help="按生成框中心列出窗口内期望出生怪数 Top-N（回 root ③①「换怪密点」选点）")
    ap.add_argument("--best-point", action="store_true",
                    help="只打印期望出生怪数最高的候选点 `x,y`（需 --view-range）")
    ap.add_argument("--all-maps", action="store_true",
                    help="跨 MonGen 所有地图排行（root ③①「MonGen 里密度高的图」）；"
                         "与 --rank/--best-point 同用，忽略 --map")
    ap.add_argument("--mapinfo", default="",
                    help="可选：Envir/MapInfo.txt；给定时每条排行/选点追加 `enterable=yes/no`"
                         "（逻辑名 = `|` 前，对齐 oracle `FindMap`）。不给时旧输出逐字节不变")
    ap.add_argument("--require-enterable", action="store_true",
                    help="机判：被报告/选中的候选图必须在 MapInfo 逻辑名内；否则 RED（需 --mapinfo）")
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

    mapinfo_keys: set[str] | None = None
    if args.mapinfo:
        mi_text = _read(args.mapinfo)
        if mi_text is None:
            print(f"PREREQ_RED: MapInfo 不存在/不可读: {args.mapinfo}")
            return 2
        mapinfo_keys = parse_mapinfo(mi_text)
        if not mapinfo_keys:
            print(f"PREREQ_RED: MapInfo 里解析不到任何地图键: {args.mapinfo}")
            return 2
    elif args.require_enterable:
        print("PREREQ_RED: --require-enterable 需要 --mapinfo <Envir/MapInfo.txt>")
        return 2

    boxes = parse_mongen(text, args.map)

    if args.rank and args.best_point:
        print("PREREQ_RED: --rank 与 --best-point 互斥")
        return 2
    if (args.rank or args.best_point) and args.view_range is None:
        print("PREREQ_RED: --rank/--best-point 需要 --view-range")
        return 2
    if args.all_maps:
        if args.view_range is None:
            print("PREREQ_RED: --all-maps 需要 --view-range")
            return 2
        rk = rank_all_maps(text, args.view_range)
        if not rk:
            print("PREREQ_RED: MonGen 里解析不到任何生成框")
            return 2
        if args.best_point:
            e, mp, x, y, b = rk[0]
            print(f"BEST ({mp}) ({x},{y}) E={e:.3f} view_range={args.view_range} "
                  f"box=({b['x']},{b['y']}) range={b['range']} count={b['count']} name={b['name']}"
                  f"{enterable_suffix(mp, mapinfo_keys)}")
            rc, msgs = require_enterable_check([mp], mapinfo_keys, args.require_enterable)
            for m in msgs:
                print(m)
            return rc
        n = max(1, min(args.rank or 10, len(rk)))
        shown = rk[:n]
        print(f"INFO all-maps 有生成框的图 {len(all_map_names(text))} 个；窗口半径 {args.view_range}；"
              f"候选中心 {len(rk)} 个（按期望出生怪数降序）"
              + (f"；MapInfo 逻辑名 {len(mapinfo_keys)} 个" if mapinfo_keys is not None else ""))
        for e, mp, x, y, b in shown:
            print(f"RANK ({mp}) ({x},{y}) E={e:.3f} box=({b['x']},{b['y']}) "
                  f"range={b['range']} count={b['count']} name={b['name']}"
                  f"{enterable_suffix(mp, mapinfo_keys)}")
        rc, msgs = require_enterable_check([mp for _, mp, _, _, _ in shown], mapinfo_keys,
                                           args.require_enterable)
        for m in msgs:
            print(m)
        if rc:
            print("RESULT: " + ("PREREQ_RED" if rc == 2 else "RED"))
            return rc
        print(f"RESULT: GREEN (Top-{n})")
        return 0

    if args.rank or args.best_point:
        if not boxes:
            print(f"PREREQ_RED: map={args.map} 在 MonGen 里解析不到任何生成框")
            return 2
        rk = rank_points(boxes, args.view_range)
        if args.best_point:
            e, x, y, b = rk[0]
            print(f"BEST ({x},{y}) E={e:.3f} view_range={args.view_range} "
                  f"box=({b['x']},{b['y']}) range={b['range']} count={b['count']} name={b['name']}"
                  f"{enterable_suffix(args.map, mapinfo_keys)}")
            rc, msgs = require_enterable_check([args.map], mapinfo_keys, args.require_enterable)
            for m in msgs:
                print(m)
            return rc
        n = max(1, min(args.rank, len(rk)))
        print(f"INFO map={args.map} 生成框 {len(boxes)} 个；窗口半径 {args.view_range}；"
              f"候选中心 {len(rk)} 个（按期望出生怪数降序）"
              f"{enterable_suffix(args.map, mapinfo_keys)}")
        for e, x, y, b in rk[:n]:
            print(f"RANK ({x},{y}) E={e:.3f} box=({b['x']},{b['y']}) "
                  f"range={b['range']} count={b['count']} name={b['name']}")
        rc, msgs = require_enterable_check([args.map], mapinfo_keys, args.require_enterable)
        for m in msgs:
            print(m)
        if rc:
            print("RESULT: " + ("PREREQ_RED" if rc == 2 else "RED"))
            return rc
        print(f"RESULT: GREEN (Top-{n})")
        return 0

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

    rc, msgs = run(
        boxes,
        points,
        args.expect,
        args.view_range,
        min_expected=args.min_expected,
        max_expected=args.max_expected,
    )
    print(f"INFO map={args.map} 生成框 {len(boxes)} 个；待判定坐标 {len(points)} 个"
          f"{enterable_suffix(args.map, mapinfo_keys)}")
    for m in msgs:
        print(m)
    g_rc, g_msgs = require_enterable_check([args.map], mapinfo_keys, args.require_enterable)
    for m in g_msgs:
        print(m)
    if rc == 0 and g_rc:
        rc = g_rc
    print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
