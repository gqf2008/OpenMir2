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

`--pair-before <a.jsonl> --pair-after <b.jsonl>`（本仓入库配对，见 `62-run-ability-pair-inrepo.txt`）
把片⑨的**真机配对**做成一条可复跑判据（本仓两份原始 capture，无需起栈）：
  A) 两侧 `11029 SM_SERVERCONFIG` 的 `raw_hex` **逐字节相同** ⇒ 本配对里 ClientConf 一字未动
     （排除「ClientConf 体布局」假说）；
  B) BEFORE（oracle-40）：52 尾 `== 40` 且 `CM_RUN == 0`（「改回 40B ⇒ 跑门关」的基线侧）；
  C) AFTER （client-58）：52 尾 `== 58` 且 `CM_RUN >= --min-run-count`（默认 1）。

`--selftest` 不依赖任何外部件（合成 fixture：全绿 / 少 3011 ⇒ 红 / 多出期望之外的 3013 不改
结果 / 52 尾长不符 ⇒ 红 / 无 3013 ⇒ 红；配对侧另有 swapped / 11029 变化 / 缺 11029 / BEFORE 带跑 ⇒ 红）。
"""

from __future__ import annotations

import argparse
import json
import os
import sys

CM_WALK = 3011
CM_RUN = 3013
SM_ABILITY = 52
SM_SERVERCONFIG = 11029


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


def s2c_raw_hexes(rows: list[dict], ident: int) -> set[str]:
    return {
        r["raw_hex"]
        for r in rows
        if r.get("dir") == "s2c" and r.get("ident") == ident and isinstance(r.get("raw_hex"), str)
    }


def check_pair(
    before: list[dict],
    after: list[dict],
    min_run: int,
    expect_before_len: int = 40,
    expect_after_len: int = 58,
) -> tuple[int, list[str]]:
    """真机配对判据（唯一变量 = 帧 52 尾布局）。见 `62-run-ability-pair-inrepo.txt`。"""
    msgs: list[str] = []

    # A) 11029 SM_SERVERCONFIG 两侧逐字节相同 ⇒ ClientConf 未动（排除 ClientConf 假说）。
    hb = s2c_raw_hexes(before, SM_SERVERCONFIG)
    ha = s2c_raw_hexes(after, SM_SERVERCONFIG)
    if not hb or not ha:
        msgs.append(
            f"RED A 11029 SM_SERVERCONFIG 缺失（before={len(hb)} after={len(ha)}）⇒ 无法排除 ClientConf 假说"
        )
    elif hb != ha:
        msgs.append("RED A 11029 body 两侧不同 ⇒ 本配对的唯一变量不止 52 布局（ClientConf 也变了）")
    else:
        msgs.append(f"OK  A 11029 body 两侧逐字节相同（{len(next(iter(hb))) // 2}B）⇒ ClientConf 未动")

    cb = ident_hist(before, "c2s")
    ca = ident_hist(after, "c2s")
    lb = ability_body_lens(before)
    la = ability_body_lens(after)
    msgs.append(
        f"--  BEFORE(oracle-40): CM_WALK={cb.get(CM_WALK, 0)} 52={lb} CM_RUN={cb.get(CM_RUN, 0)}"
    )
    msgs.append(
        f"--  AFTER (client-58): CM_WALK={ca.get(CM_WALK, 0)} 52={la} CM_RUN={ca.get(CM_RUN, 0)}"
    )

    # B) BEFORE = oracle-40 + 跑门关（「改回 ⇒ CM_RUN 归 0」的基线侧）。
    ok_b = True
    if cb.get(CM_WALK, 0) < 1:
        msgs.append(f"RED B BEFORE 无 CM_WALK({CM_WALK})：移动路径不通")
        ok_b = False
    if lb != [expect_before_len]:
        msgs.append(f"RED B BEFORE 52 尾长={lb}，期望 [{expect_before_len}]")
        ok_b = False
    if cb.get(CM_RUN, 0) != 0:
        msgs.append(f"RED B BEFORE CM_RUN={cb.get(CM_RUN, 0)}，期望 0（oracle-40 基线必须跑不动）")
        ok_b = False
    if ok_b:
        msgs.append(f"OK  B BEFORE 52 尾={expect_before_len} + CM_RUN=0（oracle-40 ⇒ CanRun 拒跑）")

    # C) AFTER = client-58 + 跑门开（本卡 DoD③ 的正向侧）。
    ok_c = True
    if ca.get(CM_WALK, 0) < 1:
        msgs.append(f"RED C AFTER 无 CM_WALK({CM_WALK})：移动路径不通")
        ok_c = False
    if la != [expect_after_len]:
        msgs.append(f"RED C AFTER 52 尾长={la}，期望 [{expect_after_len}]")
        ok_c = False
    if ca.get(CM_RUN, 0) < min_run:
        msgs.append(f"RED C AFTER CM_RUN={ca.get(CM_RUN, 0)}，期望 >= {min_run}")
        ok_c = False
    if ok_c:
        msgs.append(
            f"OK  C AFTER 52 尾={expect_after_len} + CM_RUN={ca.get(CM_RUN, 0)} >= {min_run}（client-58 ⇒ CanRun 放行）"
        )

    rc = 1 if any(m.startswith("RED") for m in msgs) else 0
    return rc, msgs


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


def selftest_pair() -> int:
    def frame(direction: str, ident: int, body_len: int = 0, raw_hex: str = "") -> dict:
        return {"dir": direction, "ident": ident, "body_len": body_len, "raw_hex": raw_hex}

    def before_rows(len52: int = 40, run: int = 0, conf: str = "c0ffee") -> list[dict]:
        rows = [
            frame("s2c", SM_SERVERCONFIG, 0, conf),
            frame("s2c", SM_ABILITY, len52),
            frame("c2s", CM_WALK),
        ]
        rows += [frame("c2s", CM_RUN) for _ in range(run)]
        return rows

    cases = [
        # name, before, after, min_run, want_rc
        ("pair-ok", before_rows(), before_rows(58, 3), 1, 0),
        ("pair-swapped-red", before_rows(58, 3), before_rows(), 1, 1),
        ("pair-before-has-run-red", before_rows(40, 1), before_rows(58, 3), 1, 1),
        ("pair-11029-changed-red", before_rows(), before_rows(58, 3, conf="deadbeef"), 1, 1),
        ("pair-missing-11029-red", [f for f in before_rows() if f["ident"] != SM_SERVERCONFIG], before_rows(58, 3), 1, 1),
        ("pair-after-40-red", before_rows(), before_rows(40, 3), 1, 1),
        ("pair-minrun-4-red", before_rows(), before_rows(58, 3), 4, 1),
    ]
    bad = 0
    for name, b, a, minrun, want_rc in cases:
        rc, msgs = check_pair(b, a, minrun)
        ok = rc == want_rc
        if not ok:
            bad += 1
        print(f"[{'PASS' if ok else 'FAIL'}] selftest {name}: rc={rc} (want {want_rc})")
        for m in msgs:
            print("        " + m)
    print(f"SELFTEST_PAIR {'OK' if bad == 0 else 'FAILED'} ({len(cases) - bad}/{len(cases)})")
    return 0 if bad == 0 else 1


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
    if bad != 0:
        return 1
    return selftest_pair()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--capture", default="")
    ap.add_argument("--expect-52-body-len", type=int, default=58)
    ap.add_argument("--min-run-count", type=int, default=1)
    ap.add_argument("--pair-before", default="", help="配对判据 BEFORE（oracle-40）capture.jsonl")
    ap.add_argument("--pair-after", default="", help="配对判据 AFTER（client-58）capture.jsonl")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if args.pair_before or args.pair_after:
        if not (args.pair_before and args.pair_after):
            print("PREREQ_MISSING: 配对判据要同时给 --pair-before 与 --pair-after")
            return 2
        try:
            pbefore = load_capture(args.pair_before)
            pafter = load_capture(args.pair_after)
        except FileNotFoundError as exc:
            print(f"PREREQ_MISSING: capture 不存在: {exc}")
            return 2
        except (json.JSONDecodeError, OSError) as exc:
            print(f"PREREQ_MISSING: capture 不可解析: {exc}")
            return 2
        rc, msgs = check_pair(pbefore, pafter, args.min_run_count)
        if args.json:
            print(json.dumps({"rc": rc, "msgs": msgs}, ensure_ascii=False))
        else:
            for m in msgs:
                print(m)
            print("RESULT: " + ("GREEN" if rc == 0 else "RED"))
        return rc

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
