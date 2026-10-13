#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_aoi_visibility_diag.py —— 卡 `openmir2-client-e2e-enter-gaps` 缺口②「同屏没怪」的
**服务端 AOI 诊断判据**（root 2026-10-14 判读误伤的机器化收口）。

输入 = Rust `bins/game-svr` 的 `game-svr.out.log`（周期行，每 50 tick 一条）：

    sim: tick #4100 clock=820000ms 处理 0 项（本地第 4100 次）实体内 27030 展示表 26899 在
    世界会话 1 [aaa(26971) map=26 vr=12 vis=0 new=0 af=0 same_map=1726 nearest=23]

字段口径（`main.rs::SessionAoiDiag`）：
  `vr`  = 观察者 `view_range`（`PlayObject.cs:904` = 12；AOI 扫描窗口 = 边长 `2*vr+1` 的正方形，
          `BaseObject.ViewRange.cs:71-74`）；
  `vis` = **当前可见集**（`visible_actors` 里 `Show|Invisible`，即 `!= Hidden`）；
  `new` = **本轮新增**可见（`Show`；出现帧触发批）；
  `af`  = 本会话**累计**实发 AOI 出现帧数（`SM_TURN` + `SM_FEATURECHANGED`，`drain_aoi_downlink` 实发）。

判据：
  ① **fail-closed 版式门**：至少解析到 1 条带 `vr=/new=/af=` 的采样；一条都没有 ⇒ `PREREQ_RED`
     （防老二进制/老日志蒙混：老格式没有这三个字段，且老 `vis` 语义是「本轮新增」不是可见集）。
  ② `--expect none`：所有采样 `vis == 0` 且 `af == 0` ⇒ GREEN（「该点确实无怪」）。
  ③ `--expect visible --min-vis N`：至少一个采样 `vis >= N` 且存在 `af >= 2` ⇒ GREEN
     （= 同屏出现帧真的发过；每个新出现 actor 一对 `SM_TURN`+`SM_FEATURECHANGED`）。
  ④ 内部一致性：`new <= vis`、`vis <= same_map + 1`（self 最多 +1），违反 ⇒ RED。
  ⑤ 对每个采样打印 `nearest` vs `vr` 的窗口判读（`nearest > vr` ⇒ 窗口外 ⇒ `vis=0` 自洽），
     便于人/机复核「无怪」结论。

退出码：`0` = GREEN / `1` = RED / `2` = PREREQ_RED（缺输入或版式不达标；绝不静默通过）。
`--selftest` 不依赖外部件（合成 fixture：none/visible 各自的绿与红、老版式 ⇒ 2、`new>vis` ⇒ 1）。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys

# `[aaa(22017) map=26 vr=12 vis=0 new=0 af=0 same_map=1726 nearest=23]`
SAMPLE_RE = re.compile(
    r"\[(?P<name>[^()\[\]\s]+)\((?P<aid>-?\d+)\)\s+"
    r"map=(?P<map>\d+)\s+vr=(?P<vr>\d+)\s+vis=(?P<vis>\d+)\s+new=(?P<new>\d+)\s+"
    r"af=(?P<af>\d+)\s+same_map=(?P<same>\d+)\s+nearest=(?P<near>-?\d+)\]"
)
# `进世界 ChrName:aaa CurMap:0 (291,666) Dir:4 entity=22017`
ENTER_RE = re.compile(
    r"进世界 ChrName:(?P<name>\S+)\s+CurMap:(?P<map>\S+)\s+\((?P<x>-?\d+),(?P<y>-?\d+)\)"
)


def parse_log(text: str) -> tuple[list[dict], list[dict]]:
    samples: list[dict] = []
    enters: list[dict] = []
    for line in text.splitlines():
        m = SAMPLE_RE.search(line)
        if m:
            samples.append(
                {
                    "name": m.group("name"),
                    "actor_id": int(m.group("aid")),
                    "map": int(m.group("map")),
                    "vr": int(m.group("vr")),
                    "vis": int(m.group("vis")),
                    "new": int(m.group("new")),
                    "af": int(m.group("af")),
                    "same_map": int(m.group("same")),
                    "nearest": int(m.group("near")),
                }
            )
            continue
        e = ENTER_RE.search(line)
        if e:
            enters.append(
                {
                    "name": e.group("name"),
                    "cur_map": e.group("map"),
                    "x": int(e.group("x")),
                    "y": int(e.group("y")),
                }
            )
    return samples, enters


def check(samples: list[dict], expect: str, min_vis: int) -> tuple[int, list[str]]:
    msgs: list[str] = []
    if not samples:
        return 2, [
            "PREREQ_RED ① 没有解析到任何带 `vr=/new=/af=` 的 AOI 采样 —— "
            "多半是老二进制/老日志（老格式没有这三个字段）。请用本片之后的 game-svr 重跑。"
        ]

    max_vis = max(s["vis"] for s in samples)
    max_af = max(s["af"] for s in samples)
    # ⑤ 逐采样窗口判读。
    for s in samples:
        outside = s["nearest"] == -1 or s["nearest"] > s["vr"]
        if s["vis"] == 0:
            sync = "vis==0 与窗口外自洽" if outside else (
                "vis==0 但 nearest<=vr（窗口内却不可见）**需查 AOI**"
            )
        else:
            sync = "vis>0 与窗口内自洽" if not outside else (
                "vis>0 但 nearest>vr（窗口外却可见）**需查 AOI**"
            )
        msgs.append(
            "INFO 采样 {name}({aid}) map={m} vr={vr} vis={vis} new={new} af={af} "
            "same_map={same} nearest={near} ⇒ nearest{rel}vr（窗口{inout}）；{sync}".format(
                name=s["name"],
                aid=s["actor_id"],
                m=s["map"],
                vr=s["vr"],
                vis=s["vis"],
                new=s["new"],
                af=s["af"],
                same=s["same_map"],
                near=s["nearest"],
                rel=">" if outside else "<=",
                inout="外" if outside else "内",
                sync=sync,
            )
        )

    # ④ 内部一致性。
    bad = [s for s in samples if s["new"] > s["vis"] or s["vis"] > s["same_map"] + 1]
    if bad:
        msgs.append(f"RED ④ 采样内部不一致（new<=vis、vis<=same_map+1 必须成立）：{bad[:3]}")

    # ② / ③ 模式门。
    if expect == "none":
        nonzero = [s for s in samples if s["vis"] != 0]
        if nonzero:
            msgs.append(f"RED ② --expect none 但有采样 vis != 0（{len(nonzero)} 条，max={max_vis}）")
        elif max_af > 0:
            msgs.append(
                f"RED ② --expect none 且所有采样 vis==0，但累计出现帧 af={max_af}>0 "
                "⇒ 该会话曾看见过怪（判据要求「从未看见」）；检查采样窗口/坐标"
            )
        else:
            msgs.append(f"OK  ② --expect none：{len(samples)} 条采样 vis==0 且 af==0（该点窗口内无怪）")
    elif expect == "visible":
        if max_vis < min_vis:
            msgs.append(f"RED ③ --expect visible：max vis={max_vis} < --min-vis {min_vis}")
        elif max_af < 2:
            msgs.append(
                f"RED ③ --expect visible：vis 达标但 af={max_af} < 2 ⇒ 没有实发出现帧对"
                "（SM_TURN+SM_FEATURECHANGED）"
            )
        else:
            msgs.append(
                f"OK  ③ --expect visible：max vis={max_vis} >= {min_vis}，max af={max_af} >= 2"
            )
    else:  # report
        msgs.append(f"INFO 仅报告：samples={len(samples)} max_vis={max_vis} max_af={max_af}")

    rc = 2 if any(m.startswith("PREREQ_RED") for m in msgs) else (
        1 if any(m.startswith("RED") for m in msgs) else 0
    )
    return rc, msgs


def _line(name: str, aid: int, m: int, vr: int, vis: int, new: int, af: int, same: int, near: int) -> str:
    return (
        f"sim: tick #50 clock=10000ms 处理 0 项（本地第 50 次）实体内 100 展示表 90 "
        f"在世界会话 1 [{name}({aid}) map={m} vr={vr} vis={vis} new={new} af={af} "
        f"same_map={same} nearest={near}]"
    )


def selftest() -> int:
    cases = [
        # name, log-text, expect, min_vis, want_rc
        ("none-green", _line("aaa", 7, 26, 12, 0, 0, 0, 1726, 23), "none", 1, 0),
        ("none-red-seen", _line("aaa", 7, 26, 12, 13, 13, 26, 1726, 2), "none", 1, 1),
        ("none-red-af", _line("aaa", 7, 26, 12, 0, 0, 2, 1726, 23), "none", 1, 1),
        ("visible-green", _line("aaa", 7, 26, 12, 13, 13, 26, 1726, 2), "visible", 1, 0),
        ("visible-red-empty", _line("aaa", 7, 26, 12, 0, 0, 0, 1726, 23), "visible", 1, 1),
        (
            "visible-red-no-frames",
            _line("aaa", 7, 26, 12, 3, 0, 0, 1726, 4),
            "visible",
            1,
            1,
        ),
        (
            "prereq-old-format",
            "sim: tick #50 clock=10000ms 实体内 100 展示表 90 在世界会话 1 "
            "[aaa(7) map=26 vis=0 same_map=1726 nearest=23]",
            "none",
            1,
            2,
        ),
        ("inconsistent-new-gt-vis", _line("aaa", 7, 26, 12, 1, 3, 3, 1726, 4), "report", 1, 1),
        ("report-green", _line("aaa", 7, 26, 12, 5, 0, 10, 1726, 4), "report", 1, 0),
    ]
    bad = 0
    for name, text, expect, min_vis, want_rc in cases:
        samples, _ = parse_log(text)
        rc, msgs = check(samples, expect, min_vis)
        ok = rc == want_rc
        if not ok:
            bad += 1
        print(f"[{'PASS' if ok else 'FAIL'}] selftest {name}: rc={rc} (want {want_rc})")
        for m in msgs:
            print("        " + m)
    print(f"SELFTEST {'OK' if bad == 0 else 'FAILED'} ({len(cases) - bad}/{len(cases)})")
    return 0 if bad == 0 else 1


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--log", default="", help="game-svr.out.log 路径")
    ap.add_argument("--expect", choices=["report", "none", "visible"], default="report")
    ap.add_argument("--min-vis", type=int, default=1, help="--expect visible 时的最小可见集大小")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if not args.log:
        print("PREREQ_RED: 需要 --log <game-svr.out.log>（或 --selftest）")
        return 2
    if not os.path.isfile(args.log):
        print(f"PREREQ_RED: 日志不存在: {args.log}")
        return 2
    try:
        with open(args.log, "r", encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:
        print(f"PREREQ_RED: 日志不可读: {exc}")
        return 2

    samples, enters = parse_log(text)
    rc, msgs = check(samples, args.expect, args.min_vis)
    if args.json:
        print(json.dumps({"rc": rc, "samples": samples, "enters": enters, "msgs": msgs}, ensure_ascii=False))
    else:
        for e in enters:
            print(f"INFO 进世界 {e['name']} CurMap={e['cur_map']} ({e['x']},{e['y']})")
        for m in msgs:
            print(m)
        print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
