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
  ② s2c 的 `52 SM_ABILITY` 尾长 == `--expect-52-body-len`（默认 58 = `MIR2_CLIENT_ABILITY_LAYOUT=1`）
     ⇒ 客户端侧 `TAbility` HP 落在 DWORD@22 = 19（≥ `Actor.pas RUN_MINHEALTH=10`）⇒
     「跑不动」**不是** 52 布局造成的。
  ③ c2s 的 `CM_RUN(3013)` 条数 == `--expect-run-count`（默认 0）：实测冻结客户端在
     单击 / 8 方向拖动 / 双圈拖动 / 渐变拖动下**从不**发 CM_RUN（成因与文件锚见
     `51-client-run-gate-anchors.txt`）。这条把实测行为机判化：一旦哪天服务端/客户端
     行为变了（真的发出 3013），判据立刻翻红，逼人重新评估。

退出码：0 = GREEN（三条口径全成立）；1 = RED；2 = PREREQ_MISSING（缺 capture / 不可解析）。

`--selftest` 不依赖任何外部件（合成三份 fixture：全绿 / 多出 3013 ⇒ 红 / 52 尾长不符 ⇒ 红）。
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


def check(rows: list[dict], expect_52: int, expect_run: int) -> tuple[int, list[str]]:
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
        msgs.append(f"OK  ② 52 尾长={lens[0]}（客户端 58B TAbility ⇒ HP>0，跑不动与 52 布局无关）")

    if runs != expect_run:
        msgs.append(f"RED ③ CM_RUN({CM_RUN})={runs}，期望 {expect_run}")
    else:
        msgs.append(f"OK  ③ CM_RUN({CM_RUN})={runs}（与实测行为一致）")

    rc = 1 if any(m.startswith("RED") for m in msgs) else 0
    return rc, msgs


def _fixture(rows: list[dict]) -> list[dict]:
    return rows


def selftest() -> int:
    def frame(direction: str, ident: int, body_len: int = 0) -> dict:
        return {"dir": direction, "ident": ident, "body_len": body_len}

    base = [frame("s2c", SM_ABILITY, 58), frame("c2s", CM_WALK)]
    cases = [
        ("all-green", base, 58, 0, 0),
        ("extra-3013", base + [frame("c2s", CM_RUN)], 58, 0, 1),
        ("52-len-40", [frame("s2c", SM_ABILITY, 40), frame("c2s", CM_WALK)], 58, 0, 1),
        ("no-walk", [frame("s2c", SM_ABILITY, 58)], 58, 0, 1),
        ("expect-run-1-mismatch", base, 58, 1, 1),
    ]
    bad = 0
    for name, rows, e52, erun, want_rc in cases:
        rc, msgs = check(_fixture(rows), e52, erun)
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
    ap.add_argument("--expect-run-count", type=int, default=0)
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

    rc, msgs = check(rows, args.expect_52_body_len, args.expect_run_count)
    if args.json:
        print(json.dumps({"rc": rc, "msgs": msgs}, ensure_ascii=False))
    else:
        for m in msgs:
            print(m)
        print("RESULT: " + ("GREEN" if rc == 0 else "RED"))
    return rc


if __name__ == "__main__":
    sys.exit(main())
