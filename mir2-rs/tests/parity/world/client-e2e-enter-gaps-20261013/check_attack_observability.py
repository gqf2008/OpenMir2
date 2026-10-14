#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_attack_observability.py — 冻结客户端「能攻击造成伤害」判据（DoD③ 第三半）。

卡：`openmir2-client-e2e-enter-gaps`（DoD③「屏幕见怪**并能攻击造成伤害**」）。

采集口径：真机 `tools/capture/mir2_proxy.py` 在 7200 透明代理 → 影子 game-gate(7600)
⇒ 代理 dump 的字节就是客户端实际收发的字节；`tools/m2-acceptance/export_s2c.ps1`
解码成 capture.jsonl（`dir` 区分 c2s/s2c）。

判据（单侧）：
  ① **攻击**：c2s `CM_HIT(3014)` 条数 ≥ `--min-hits`（默认 1）⇒ 客户端确实对目标发出了攻击
     （`mir_flow.ps1 -AttackClicks N` 绕角色点环；点到怪 = `CM_HIT`）。
  ② **伤害可见**：s2c `SM_STRUCK(31)` 条数 ≥ `--min-struck`（默认 1）**或** `SM_NOWDEATH(34)`
     条数 ≥ `--min-kills`（默认 1）⇒ 服务端把受击/死亡帧下发给客户端。
     oracle 路径：`BaseObject.Attack.cs:57` `SendStruckDelayMsg(RM_REFMESSAGE, …)` →
     受击者 `Run()` 的 `RM_REFMESSAGE` 支（`BaseObject.Base.cs:699-705`）
     `SendRefMsg(RM_STRUCK, …)` → `SendRefMsg`（`BaseObject.Message.cs:383+`，同图切比雪夫 ≤ 12、
     `Race==Play`、非 Ghost）→ 观察者 `RM_STRUCK` 处理器（`PlayObject.Message.cs:1562-1628`）
     回 `SM_STRUCK` ⇒ **怪物目标也广播**（怪物无连接，但视野内玩家收到）。

`--expect`：
  `attack` 只要求 ①；`damage` 只要求 ②（`SM_STRUCK`）；`kill` 只要求 ②（`SM_NOWDEATH`）；
  `any` 要求 ① **且** ②（②取 `SM_STRUCK` 或 `SM_NOWDEATH` 任一）——**默认**（= DoD「能攻击造成伤害」）；
  `none` 要求 ① == 0（无怪点的反向对照）。

`--left/--right`（两侧）：两侧各按同一 `--expect` 判（两侧对照，与 `aoi_visibility_pair.py` 同口径）。

退出码：`0` GREEN / `1` RED / `2` PREREQ_MISSING（缺 capture / 不可解析 / 参数不当）。
`--selftest` 不依赖任何外部件。
"""

from __future__ import annotations

import argparse
import json
import os
import sys

CM_HIT = 3014
SM_STRUCK = 31
SM_NOWDEATH = 34


def load_capture(path: str) -> list[dict]:
    if not os.path.isfile(path):
        raise FileNotFoundError(path)
    rows: list[dict] = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def _count(rows: list[dict], direction: str, ident: int) -> int:
    return sum(1 for r in rows if r.get("dir") == direction and r.get("ident") == ident)


def check(rows: list[dict], expect: str = "any", min_hits: int = 1,
          min_struck: int = 1, min_kills: int = 1) -> tuple[list[str], dict]:
    """返回 (failures, facts)。单侧口径。"""
    fails: list[str] = []
    hits = _count(rows, "c2s", CM_HIT)
    struck = _count(rows, "s2c", SM_STRUCK)
    kills = _count(rows, "s2c", SM_NOWDEATH)
    if expect == "attack":
        if hits < min_hits:
            fails.append("c2s CM_HIT(%d) 只有 %d 条 < 要求 %d" % (CM_HIT, hits, min_hits))
    elif expect == "damage":
        if struck < min_struck:
            fails.append("s2c SM_STRUCK(%d) 只有 %d 条 < 要求 %d" % (SM_STRUCK, struck, min_struck))
    elif expect == "kill":
        if kills < min_kills:
            fails.append("s2c SM_NOWDEATH(%d) 只有 %d 条 < 要求 %d" % (SM_NOWDEATH, kills, min_kills))
    elif expect == "any":
        if hits < min_hits:
            fails.append("c2s CM_HIT(%d) 只有 %d 条 < 要求 %d（客户端没打到目标）"
                         % (CM_HIT, hits, min_hits))
        if struck < min_struck and kills < min_kills:
            fails.append("s2c 既没有 SM_STRUCK(%d)（%d<要求 %d）也没有 SM_NOWDEATH(%d)（%d<要求 %d）"
                         "⇒ 伤害不可见（或未击杀）"
                         % (SM_STRUCK, struck, min_struck, SM_NOWDEATH, kills, min_kills))
    elif expect == "none":
        if hits != 0:
            fails.append("c2s CM_HIT(%d) = %d 条 ≠ 0（无怪点不应发出攻击）" % (CM_HIT, hits))
    else:
        raise ValueError("未知 --expect: %s" % expect)
    facts = {
        "cm_hit": hits,
        "sm_struck": struck,
        "sm_nowdeath": kills,
        "expect": expect,
    }
    return fails, facts


def _report(name: str, fails: list[str], facts: dict) -> None:
    print("[%s] cm_hit=%d sm_struck=%d sm_nowdeath=%d expect=%s"
          % (name, facts["cm_hit"], facts["sm_struck"], facts["sm_nowdeath"], facts["expect"]))
    for f in fails:
        print("  RED: " + f)


def synthetic(hits: int = 1, struck: int = 1, kills: int = 0) -> list[dict]:
    rows: list[dict] = []
    seq = 0
    for _ in range(hits):
        seq += 1
        rows.append({"kind": "frame", "dir": "c2s", "seq": seq, "ident": CM_HIT,
                     "recog": 0, "tag": 0, "series": 0, "raw_hex": "00"})
    for _ in range(struck):
        seq += 1
        rows.append({"kind": "frame", "dir": "s2c", "seq": seq, "ident": SM_STRUCK,
                     "recog": 0, "tag": 0, "series": 0, "raw_hex": "00"})
    for _ in range(kills):
        seq += 1
        rows.append({"kind": "frame", "dir": "s2c", "seq": seq, "ident": SM_NOWDEATH,
                     "recog": 0, "tag": 0, "series": 0, "raw_hex": "00"})
    return rows


def selftest() -> int:
    ok = True

    def expect_case(name: str, got: int, want: int) -> None:
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-52s %s (rc=%d, want %d)" % (name, "OK" if good else "FAIL", got, want))

    def rc_of(fails: list[str]) -> int:
        return 1 if fails else 0

    expect_case("攻击+受击帧 ⇒ GREEN",
                rc_of(check(synthetic(1, 1, 0), "any")[0]), 0)
    expect_case("攻击+击杀帧（无 struck）⇒ GREEN",
                rc_of(check(synthetic(1, 0, 1), "any")[0]), 0)
    expect_case("只攻击无伤害帧 ⇒ RED",
                rc_of(check(synthetic(1, 0, 0), "any")[0]), 1)
    expect_case("有伤害帧但无攻击（异常）⇒ RED",
                rc_of(check(synthetic(0, 1, 0), "any")[0]), 1)
    expect_case("expect=attack 无 CM_HIT ⇒ RED",
                rc_of(check(synthetic(0, 1, 0), "attack")[0]), 1)
    expect_case("expect=damage 无 SM_STRUCK（有击杀不算）⇒ RED",
                rc_of(check(synthetic(1, 0, 1), "damage")[0]), 1)
    expect_case("expect=kill 有 SM_NOWDEATH ⇒ GREEN",
                rc_of(check(synthetic(1, 0, 1), "kill")[0]), 0)
    expect_case("expect=none 无 CM_HIT ⇒ GREEN",
                rc_of(check(synthetic(0, 0, 0), "none")[0]), 0)
    expect_case("expect=none 有 CM_HIT ⇒ RED",
                rc_of(check(synthetic(1, 0, 0), "none")[0]), 1)
    expect_case("min_hits=2 只 1 条 ⇒ RED",
                rc_of(check(synthetic(1, 1, 0), "attack", min_hits=2)[0]), 1)
    expect_case("min_struck=2 只 1 条 ⇒ RED",
                rc_of(check(synthetic(1, 1, 0), "damage", min_struck=2)[0]), 1)
    expect_case("事实计数逐项",
                0 if check(synthetic(3, 2, 4), "any")[1] ==
                {"cm_hit": 3, "sm_struck": 2, "sm_nowdeath": 4, "expect": "any"} else 1, 0)
    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="冻结客户端「能攻击造成伤害」判据")
    ap.add_argument("--capture", default="", help="单侧 capture.jsonl")
    ap.add_argument("--left", default="", help="两侧：左（Rust）capture.jsonl")
    ap.add_argument("--right", default="", help="两侧：右（C#）capture.jsonl")
    ap.add_argument("--expect", default="any",
                    choices=["attack", "damage", "kill", "any", "none"])
    ap.add_argument("--min-hits", type=int, default=1)
    ap.add_argument("--min-struck", type=int, default=1)
    ap.add_argument("--min-kills", type=int, default=1)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    if args.left or args.right:
        if not args.left or not args.right:
            print("PREREQ_MISSING: 两侧模式必须同时给 --left 与 --right")
            print("RESULT: PREREQ_RED")
            return 2
        all_fails: list[str] = []
        try:
            for name, path in (("left", args.left), ("right", args.right)):
                rows = load_capture(path)
                fails, facts = check(rows, args.expect, args.min_hits, args.min_struck, args.min_kills)
                _report(name, fails, facts)
                all_fails.extend("%s: %s" % (name, f) for f in fails)
        except FileNotFoundError as exc:
            print("PREREQ_MISSING: 找不到 capture %s" % exc)
            print("RESULT: PREREQ_RED")
            return 2
        except (ValueError, json.JSONDecodeError) as exc:
            print("PREREQ_MISSING: capture 不可解析：%s" % exc)
            print("RESULT: PREREQ_RED")
            return 2
        if all_fails:
            print("RED: 两侧攻击/伤害判据有 %d 条不成立" % len(all_fails))
            print("RESULT: RED")
            return 1
        print("GREEN: 两侧都满足 expect=%s" % args.expect)
        print("RESULT: GREEN")
        return 0

    if not args.capture:
        print("PREREQ_MISSING: 必须给 --capture（或 --left/--right）")
        print("RESULT: PREREQ_RED")
        return 2
    try:
        rows = load_capture(args.capture)
    except FileNotFoundError as exc:
        print("PREREQ_MISSING: 找不到 capture %s" % exc)
        print("RESULT: PREREQ_RED")
        return 2
    except (ValueError, json.JSONDecodeError) as exc:
        print("PREREQ_MISSING: capture 不可解析：%s" % exc)
        print("RESULT: PREREQ_RED")
        return 2
    fails, facts = check(rows, args.expect, args.min_hits, args.min_struck, args.min_kills)
    _report("single", fails, facts)
    if fails:
        print("RESULT: RED")
        return 1
    print("RESULT: GREEN")
    return 0


if __name__ == "__main__":
    sys.exit(main())
