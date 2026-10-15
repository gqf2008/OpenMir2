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
  ② **伤害可见（出站，victim ≠ 自己）**：s2c `SM_STRUCK(31)` 中**受击者不是本侧玩家**的条数
     ≥ `--min-struck`（默认 1），**或** s2c `SM_NOWDEATH(34)` 中 victim ≠ 自己的条数
     ≥ `--min-kills`（默认 1）⇒ 服务端把「本侧玩家打中别人」的受击/死亡帧下发给了客户端。
     oracle 路径：`BaseObject.Attack.cs:57` `SendStruckDelayMsg(RM_REFMESSAGE, …)` →
     受击者 `Run()` 的 `RM_REFMESSAGE` 支（`BaseObject.Base.cs:699-705`）
     `SendRefMsg(RM_STRUCK, …)` → `SendRefMsg`（`BaseObject.Message.cs:383+`，同图切比雪夫 ≤ 12、
     `Race==Play`、非 Ghost）→ 观察者 `RM_STRUCK` 处理器（`PlayObject.Message.cs:1562-1628`）
     回 `SM_STRUCK` ⇒ **怪物目标也广播**（怪物无连接，但视野内玩家收到）。

  **victim 过滤（2026-10-15 片68 加，改坏必红见下）**：`SM_STRUCK`/`SM_NOWDEATH` 也用于
  **入站**（怪打玩家 ⇒ 受击者 = 自己）。不区分方向会把「玩家被围殴」读成「玩家造成伤害」——
  片67 就是这么把 C# 侧 6 条 victim=自己 的 `SM_STRUCK` 判成「C# 有伤害可见」而 Rust 0 条判 RED 的。
  本侧玩家 ActorId（self）从 s2c 里「关于自己」的帧取（`SM_LOGON(50)` > `SM_NEWMAP(51)` >
  `SM_HEALTHSPELLCHANGED(53)`，见 `detect_self`）；取不到时 **fail-closed**（PREREQ_RED），
  除非显式 `--self-recog N` 或 `--allow-unknown-self`（后者退回旧口径，仅诊断用）。

`--expect`：
  `attack` 只要求 ①；`damage` 只要求 ②（出站 `SM_STRUCK`）；`kill` 只要求 ②（出站 `SM_NOWDEATH`）；
  `any` 要求 ① **且** ②（②取出站 `SM_STRUCK` 或出站 `SM_NOWDEATH` 任一）——**默认**（= DoD「能攻击造成伤害」）；
  `none` 要求 ① == 0（无怪点的反向对照）。

`--left/--right`（两侧）：两侧各按同一 `--expect` 判（两侧对照，与 `aoi_visibility_pair.py` 同口径）；
两侧各自解析 self（两侧玩家 ActorId 不同）。

退出码：`0` GREEN / `1` RED / `2` PREREQ_MISSING（缺 capture / 不可解析 / 参数不当 / self 未知）。
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
SM_LOGON = 50
SM_NEWMAP = 51
SM_HEALTHSPELLCHANGED = 53

# 「关于自己」的 s2c 帧：服务端发给收帧玩家本人、recog = 本人 ActorId（可靠性从高到低）。
SELF_ANCHORS = (SM_LOGON, SM_NEWMAP, SM_HEALTHSPELLCHANGED)


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


def detect_self(rows: list[dict]) -> int | None:
    """本侧玩家自己的 ActorId = s2c 里「关于自己」的帧的 recog（`SELF_ANCHORS` 顺序探测）。"""
    for ident in SELF_ANCHORS:
        for r in rows:
            if r.get("dir") == "s2c" and r.get("ident") == ident:
                recog = r.get("recog")
                if isinstance(recog, int):
                    return recog
    return None


def _count_split(rows: list[dict], ident: int, self_recog: int | None) -> tuple[int, int]:
    """s2c `ident` 帧按受击者拆：返回 (出站=受击者≠自己, 入站=受击者=自己)。

    `self_recog is None`（`--allow-unknown-self`）⇒ 全部计入出站（旧口径，仅诊断用）。
    """
    outgoing = incoming = 0
    for r in rows:
        if r.get("dir") != "s2c" or r.get("ident") != ident:
            continue
        if self_recog is not None and r.get("recog") == self_recog:
            incoming += 1
        else:
            outgoing += 1
    return outgoing, incoming


def check(rows: list[dict], expect: str = "any", min_hits: int = 1,
          min_struck: int = 1, min_kills: int = 1,
          self_recog: int | None = None) -> tuple[list[str], dict]:
    """返回 (failures, facts)。单侧口径；伤害/击杀只数**出站**（victim ≠ 自己）帧。"""
    fails: list[str] = []
    hits = _count(rows, "c2s", CM_HIT)
    struck, struck_in = _count_split(rows, SM_STRUCK, self_recog)
    kills, kills_in = _count_split(rows, SM_NOWDEATH, self_recog)
    if expect == "attack":
        if hits < min_hits:
            fails.append("c2s CM_HIT(%d) 只有 %d 条 < 要求 %d" % (CM_HIT, hits, min_hits))
    elif expect == "damage":
        if struck < min_struck:
            fails.append("s2c 出站 SM_STRUCK(%d)（victim≠自己）只有 %d 条 < 要求 %d"
                         % (SM_STRUCK, struck, min_struck))
    elif expect == "kill":
        if kills < min_kills:
            fails.append("s2c 出站 SM_NOWDEATH(%d)（victim≠自己）只有 %d 条 < 要求 %d"
                         % (SM_NOWDEATH, kills, min_kills))
    elif expect == "any":
        if hits < min_hits:
            fails.append("c2s CM_HIT(%d) 只有 %d 条 < 要求 %d（客户端没打到目标）"
                         % (CM_HIT, hits, min_hits))
        if struck < min_struck and kills < min_kills:
            fails.append("s2c 既没有出站 SM_STRUCK(%d)（victim≠自己：%d<要求 %d）"
                         "也没有出站 SM_NOWDEATH(%d)（%d<要求 %d）⇒ 本侧玩家的攻击没有造成可见伤害"
                         "（入站受击不计：SM_STRUCK 入站 %d 条 / SM_NOWDEATH 入站 %d 条）"
                         % (SM_STRUCK, struck, min_struck, SM_NOWDEATH, kills, min_kills,
                            struck_in, kills_in))
    elif expect == "none":
        if hits != 0:
            fails.append("c2s CM_HIT(%d) = %d 条 ≠ 0（无怪点不应发出攻击）" % (CM_HIT, hits))
    else:
        raise ValueError("未知 --expect: %s" % expect)
    facts = {
        "cm_hit": hits,
        "sm_struck": struck,
        "sm_nowdeath": kills,
        "sm_struck_incoming": struck_in,
        "sm_nowdeath_incoming": kills_in,
        "self_recog": self_recog,
        "expect": expect,
    }
    return fails, facts


def _report(name: str, fails: list[str], facts: dict) -> None:
    print("[%s] cm_hit=%d 出站 sm_struck=%d sm_nowdeath=%d｜入站 sm_struck=%d sm_nowdeath=%d"
          "｜self=%s expect=%s"
          % (name, facts["cm_hit"], facts["sm_struck"], facts["sm_nowdeath"],
             facts["sm_struck_incoming"], facts["sm_nowdeath_incoming"],
             facts["self_recog"], facts["expect"]))
    for f in fails:
        print("  RED: " + f)


# 片67 C# 侧实测的玩家 ActorId（selftest 常量，非外部依赖）；OTHER = 任意「非自己」ActorId。
SELF = 1849558029
OTHER = 252595457


def synthetic(hits: int = 1, struck: int = 1, kills: int = 0,
              struck_incoming: int = 0, kills_incoming: int = 0,
              self_recog: int = SELF) -> list[dict]:
    """构造 capture 行：`struck`/`kills` = **出站**（victim=OTHER）；`*_incoming` = 入站（victim=self）。"""
    rows: list[dict] = []
    seq = 0

    def add(direction: str, ident: int, recog: int) -> None:
        nonlocal seq
        seq += 1
        rows.append({"kind": "frame", "dir": direction, "seq": seq, "ident": ident,
                     "recog": recog, "tag": 0, "series": 0, "raw_hex": "00"})

    for _ in range(hits):
        add("c2s", CM_HIT, 0)
    for _ in range(struck):
        add("s2c", SM_STRUCK, OTHER)
    for _ in range(kills):
        add("s2c", SM_NOWDEATH, OTHER)
    for _ in range(struck_incoming):
        add("s2c", SM_STRUCK, self_recog)
    for _ in range(kills_incoming):
        add("s2c", SM_NOWDEATH, self_recog)
    return rows


def selftest() -> int:
    ok = True

    def expect_case(name: str, got: int, want: int) -> None:
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-56s %s (rc=%d, want %d)" % (name, "OK" if good else "FAIL", got, want))

    def rc_of(fails: list[str]) -> int:
        return 1 if fails else 0

    expect_case("攻击+出站受击帧 ⇒ GREEN",
                rc_of(check(synthetic(1, 1, 0), "any", self_recog=SELF)[0]), 0)
    expect_case("攻击+出站击杀帧（无 struck）⇒ GREEN",
                rc_of(check(synthetic(1, 0, 1), "any", self_recog=SELF)[0]), 0)
    expect_case("只攻击无伤害帧 ⇒ RED",
                rc_of(check(synthetic(1, 0, 0), "any", self_recog=SELF)[0]), 1)
    expect_case("有伤害帧但无攻击（异常）⇒ RED",
                rc_of(check(synthetic(0, 1, 0), "any", self_recog=SELF)[0]), 1)
    expect_case("expect=attack 无 CM_HIT ⇒ RED",
                rc_of(check(synthetic(0, 1, 0), "attack", self_recog=SELF)[0]), 1)
    expect_case("expect=damage 无出站 SM_STRUCK（有出站击杀不算）⇒ RED",
                rc_of(check(synthetic(1, 0, 1), "damage", self_recog=SELF)[0]), 1)
    expect_case("expect=kill 有出站 SM_NOWDEATH ⇒ GREEN",
                rc_of(check(synthetic(1, 0, 1), "kill", self_recog=SELF)[0]), 0)
    expect_case("expect=none 无 CM_HIT ⇒ GREEN",
                rc_of(check(synthetic(0, 0, 0), "none", self_recog=SELF)[0]), 0)
    expect_case("expect=none 有 CM_HIT ⇒ RED",
                rc_of(check(synthetic(1, 0, 0), "none", self_recog=SELF)[0]), 1)
    expect_case("min_hits=2 只 1 条 ⇒ RED",
                rc_of(check(synthetic(1, 1, 0), "attack", min_hits=2, self_recog=SELF)[0]), 1)
    expect_case("min_struck=2 只 1 条 ⇒ RED",
                rc_of(check(synthetic(1, 1, 0), "damage", min_struck=2, self_recog=SELF)[0]), 1)
    # —— 2026-10-15 片68：victim 过滤（改坏必红本体）——
    expect_case("★入站受击（victim=自己）不计入伤害 ⇒ damage RED",
                rc_of(check(synthetic(1, 0, 0, struck_incoming=6), "damage", self_recog=SELF)[0]), 1)
    expect_case("★同 capture 旧口径（--allow-unknown-self）⇒ 假 GREEN（证明过滤器是判别式）",
                rc_of(check(synthetic(1, 0, 0, struck_incoming=6), "damage")[0]), 0)
    expect_case("★入站击杀（victim=自己）不算击杀 ⇒ kill RED",
                rc_of(check(synthetic(1, 0, 0, kills_incoming=1), "kill", self_recog=SELF)[0]), 1)
    expect_case("★入站不计、出站照计 ⇒ any GREEN",
                rc_of(check(synthetic(1, 1, 0, struck_incoming=3, kills_incoming=1),
                            "any", self_recog=SELF)[0]), 0)
    expect_case("★detect_self 从 SM_LOGON 取 self",
                0 if detect_self(synthetic(0, 0, 0) + [{"kind": "frame", "dir": "s2c",
                                                        "ident": SM_LOGON, "recog": SELF}]) == SELF else 1, 0)
    expect_case("★detect_self 无锚 ⇒ None",
                0 if detect_self(synthetic(1, 1, 0)) is None else 1, 0)
    expect_case("事实计数逐项（出站/入站分离）",
                0 if check(synthetic(3, 2, 4, struck_incoming=5, kills_incoming=1),
                           "any", self_recog=SELF)[1] ==
                {"cm_hit": 3, "sm_struck": 2, "sm_nowdeath": 4,
                 "sm_struck_incoming": 5, "sm_nowdeath_incoming": 1,
                 "self_recog": SELF, "expect": "any"} else 1, 0)
    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def _resolve_self(rows: list[dict], self_recog: int | None, allow_unknown: bool,
                  name: str) -> tuple[int | None, str | None]:
    """返回 (self, 错误信息)。显式 --self-recog 优先；否则探测；探测失败 fail-closed。"""
    if self_recog is not None:
        return self_recog, None
    detected = detect_self(rows)
    if detected is not None:
        return detected, None
    if allow_unknown:
        print("[%s] WARN：self ActorId 未探测到且给了 --allow-unknown-self ⇒ 退回旧口径"
              "（入站受击会被误计为伤害，仅诊断用）" % name)
        return None, None
    return None, ("[%s] self ActorId 未探测到（s2c 缺 SM_LOGON(50)/SM_NEWMAP(51)/SM_HEALTHSPELLCHANGED(53)）"
                  "⇒ 无法区分入站/出站受击；给 --self-recog N 或 --allow-unknown-self" % name)


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
    ap.add_argument("--self-recog", type=int, default=None,
                    help="显式指定本侧玩家 ActorId（跳过探测）")
    ap.add_argument("--allow-unknown-self", action="store_true",
                    help="探测不到 self 时退回旧口径（计入所有 s2c 受击；仅诊断，正式判据禁用）")
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
                self_recog, err = _resolve_self(rows, args.self_recog,
                                                args.allow_unknown_self, name)
                if err:
                    print("PREREQ_MISSING: " + err)
                    print("RESULT: PREREQ_RED")
                    return 2
                fails, facts = check(rows, args.expect, args.min_hits, args.min_struck,
                                     args.min_kills, self_recog)
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
    self_recog, err = _resolve_self(rows, args.self_recog, args.allow_unknown_self, "single")
    if err:
        print("PREREQ_MISSING: " + err)
        print("RESULT: PREREQ_RED")
        return 2
    fails, facts = check(rows, args.expect, args.min_hits, args.min_struck, args.min_kills,
                         self_recog)
    _report("single", fails, facts)
    if fails:
        print("RESULT: RED")
        return 1
    print("RESULT: GREEN")
    return 0


if __name__ == "__main__":
    sys.exit(main())
