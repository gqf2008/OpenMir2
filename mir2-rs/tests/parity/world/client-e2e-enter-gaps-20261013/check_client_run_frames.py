#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_client_run_frames.py — 冻结客户端「跑键」上下行可见性判据。

卡：`openmir2-client-e2e-enter-gaps`（DoD②「能跑」/ reviewer not_verified ③
「CM_RUN(3013) 端到端抓包」）。

采集口径：真机 `tools/capture/mir2_proxy.py` 在 7200 透明代理 → 影子 game-gate(7600)
⇒ 代理 dump 的字节就是客户端实际收发的字节；`tools/m2-acceptance/export_s2c.ps1`
（segment_frames + GoldenExport）解码成 capture.jsonl（dir 区分 c2s/s2c）。

判据（对 `--capture <capture.jsonl>`）：
  ① c2s 至少有 1 条 `CM_WALK(3011)`：证明**移动路径是活的**（不是客户端根本没发包）。
  ② s2c 的 `52 SM_ABILITY` 尾长 == `--expect-52-body-len`（默认 58）：58 = 冻结客户端 58B
     `TAbility`（`HIGHHP=True`）⇒ `HP@22 = 19 ≥ Actor.pas RUN_MINHEALTH(10)` ⇒ 客户端
     `CanRun` 允许跑。40B（oracle 布局）会让客户端把 `HP` 读成 `Exp`（=0）⇒ 跑门被
     `CanRun` 拒绝（这就是「跑不动」的根因，见 61-*）。
  ③ c2s 的 `CM_RUN(3013)` 条数 **≥ `--min-run-count`**（默认 1）：证明「跑键」真的产生
     跑位。（注：本判据的**前置**是客户端在 58B 布局下 + 驱动用 `Send-HoldRun` 在**目标点**
     按下左键；早期版本从屏幕中心按住再靠 WM_MOUSEMOVE 挪永远设不上远目标 ⇒ 假 0，
     见 `tools/client/mir_flow.ps1::Send-HoldRun` 的注释与 61-*。）

退出码：0 = GREEN（三条口径全成立）；1 = RED；2 = PREREQ_MISSING（缺 capture / 不可解析）。

`--selftest` 不依赖任何外部件（合成 fixture：全绿 / 少 3011 ⇒ 红 / 多出期望之外的 3013 不改
结果 / 52 尾长不符 ⇒ 红 / 无 3013 ⇒ 红）。
"""

from __future__ import annotations

import argparse
import json
import os
import sys

CM_WALK = 3011
CM_RUN = 3013
SM_ABILITY = 52


def load_capture(path: str) -> list[dict]:
    if not os.path.isfile(path):
        raise FileNotFoundError(path)
    rows = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def ident_hist(rows: list[dict], direction: str) -> dict[int, int]:
    hist: dict[int, int] = {}
    for r in rows:
        if r.get("dir") == direction and isinstance(r.get("ident"), int):
            hist[r["ident"]] = hist.get(r["ident"], 0) + 1
    return hist


def ability_body_lens(rows: list[dict]) -> list[int]:
    out = []
    for r in rows:
        if r.get("dir") == "s2c" and r.get("ident") == SM_ABILITY:
            bl = r.get("body_len")
            if isinstance(bl, int):
                out.append(bl)
    return out


def check(rows: list[dict], expect_52: int, min_run: int) -> tuple[int, list[str]]:
    msgs: list[str] = []
    c2s = ident_hist(rows, "c2s")
    walks = c2s.get(CM_WALK, 0)
    runs = c2s.get(CM_RUN, 0)
    lens = ability_body_lens(rows)

    if walks < 1:
        msgs.append(f"RED ① 没有 CM_WALK({CM_WALK})：移动路径不通（c2s={sorted(c2s.items())}）")
    else:
        msgs.append(f"OK  ① c2s CM_WALK({CM_WALK})={walks}")

    if expect_52 == 0:
        msgs.append("OK  ② --expect-52-body-len=0：跳过 52 尾长门")
    elif not lens:
        msgs.append(f"RED ② s2c 里没有 52 SM_ABILITY（c2s/s2c 解不出能力帧）")
    elif lens != [expect_52]:
        msgs.append(f"RED ② 52 尾长={lens}，期望 [{expect_52}]")
    else:
        msgs.append(f"OK  ② 52 尾长={lens[0]}（客户端 58B TAbility ⇒ HP>0，可跑）")

    if runs < min_run:
        msgs.append(f"RED ③ CM_RUN({CM_RUN})={runs}，期望 >= {min_run}")
    else:
        msgs.append(f"OK  ③ CM_RUN({CM_RUN})={runs} >= {min_run}（客户端真的发跑位）")

    rc = 1 if any(m.startswith("RED") for m in msgs) else 0
    return rc, msgs


def selftest() -> int:
    def frame(direction: str, ident: int, body_len: int = 0) -> dict:
        return {"dir": direction, "ident": ident, "body_len": body_len}

    base = [frame("s2c", SM_ABILITY, 58), frame("c2s", CM_WALK), frame("c2s", CM_RUN)]
    cases = [
        # name, rows, expect_52, min_run, want_rc
        ("all-green", base, 58, 1, 0),
        ("run-0-red", [frame("s2c", SM_ABILITY, 58), frame("c2s", CM_WALK)], 58, 1, 1),
        ("run-many-ok", base + [frame("c2s", CM_RUN), frame("c2s", CM_RUN)], 58, 1, 0),
        ("52-len-40-red", [frame("s2c", SM_ABILITY, 40), frame("c2s", CM_WALK), frame("c2s", CM_RUN)], 58, 1, 1),
        ("no-walk-red", [frame("s2c", SM_ABILITY, 58), frame("c2s", CM_RUN)], 58, 1, 1),
        ("expect-run-0-before", [frame("s2c", SM_ABILITY, 40), frame("c2s", CM_WALK)], 40, 0, 0),
    ]
    bad = 0
    for name, rows, e52, minrun, want_rc in cases:
        rc, msgs = check(rows, e52, minrun)
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
    ap.add_argument("--capture", default="")
    ap.add_argument("--expect-52-body-len", type=int, default=58)
    ap.add_argument("--min-run-count", type=int, default=1)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if not args.capture:
        print("PREREQ_MISSING: 需要 --capture <capture.jsonl>（或 --selftest）")
        return 2
    try:
        rows = load_capture(args.capture)
    except FileNotFoundError:
        print(f"PREREQ_MISSING: capture 不存在: {args.capture}")
        return 2
    except (json.JSONDecodeError, OSError) as exc:
        print(f"PREREQ_MISSING: capture 不可解析: {exc}")
        return 2

    rc, msgs = check(rows, args.expect_52_body_len, args.min_run_count)
    if args.json:
        print(json.dumps({"rc": rc, "msgs": msgs}, ensure_ascii=False))
    else:
        for m in msgs:
            print(m)
        print("RESULT: " + ("GREEN" if rc == 0 else "RED"))
    return rc


if __name__ == "__main__":
    sys.exit(main())
