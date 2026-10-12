#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""attack_stream_gate.py — C# 攻击**专用流**的「只有被测量攻方」fail-closed 门。

## 为什么有这道门

owner 2026-10-12 22:15 ① / 2026-10-13 m3cs F-N1：
C# 影子 hook（`rng-hook/StartupHook.cs`）的隔离按**调用点**分流（`divert-sites.txt`），
不按攻方实体。同一 `GameSvr` 进程里**别图的怪物/守卫 AI**走同一批站点时也抽
`Random(attack_seed)` ⇒ 非玩家攻方的取数被吸进同一条专用流 ⇒ 被测量玩家攻方的流位置
被整体推移（C# 不可复现、跨侧逐 k≠0）。

hook 侧已按「被测量攻方线程」收窄（F-N1 修法）。本门是**下游 fail-closed 牙齿**：即便
收窄失效（或别的路径把非玩家攻方塞进专用流），也**当场判红、拒比**，绝不让污染流进入
`attack_diff.py` 得出假结论（owner 23:50 ③「play 单元数 < 期望 ⇒ 直接拒跑」）。

## 判据（任一不满足 ⇒ rc=1）

1. 专用流**首单元**必须是**玩家攻方单元**（首站点 = `PlayObject.GetAttackPower@PlayObject.cs:2929`
   或 `:2918`，即 `atk_power_a` / `atk_power_luck_a`）——污染流的首条往往是
   `atk_power_base`（非 PlayObject 攻方），一票否决；
2. **player 单元数 == --expected**（本侧固定攻击序列的击数）；
3. **base 单元数 == 0**（`BaseObject.GetAttackPower@BaseObject.Base.cs:892` = 非玩家攻方；
   PlayObject 覆写了 `GetAttackPower` ⇒ 被测量玩家永不产生 base 单元）——>0 即点名该单元
   在专用流里的 1-based 行号。

## 数据源

C# 侧 `MIR2_RNG_ATTACK_LOG`（= `-AttackStreamLog`，缺省 `E:\tmp\m3-csharp-hook\rng-attack-stream.tsv`）
的 4 列 TSV（**只有被分流到专用流的取数**，无 seq 列）：

    <kind> \t <arg> \t <result> \t <site>

站点用 `\` 或 `/` 均可（归一后比）。

## 用法

    python -I attack_stream_gate.py gate --stream <rng-attack-stream.tsv> --expected 120 [--json out.json]
    python -I attack_stream_gate.py selftest

退出码：0 = GREEN；1 = RED（污染/计数不符）；2 = PREREQ（文件缺失/解析空）。
"""
import argparse
import json
import os
import sys

# 玩家攻方**首抽**站点（unit 起点）：Luck<=0 ⇒ :2929；Luck>0 ⇒ :2918。
# 第二抽（:2934 / :2924）同属该 unit，不作为 unit 起点（否则每击会被数成两个单元）。
PLAYER_UNIT_START_SITES = {
    "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929",
    "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2918",
}
# 非玩家攻方（BaseObject.GetAttackPower）——PlayObject 覆写 ⇒ 被测量玩家永不产生 base 单元。
BASE_UNIT_START_SITES = {
    "M2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892",
}


def norm_site(site):
    """`crates\\...` / `M2Server...` 统一成 `/` 再比。"""
    return (site or "").replace("\\", "/")


def _player_start(site):
    return norm_site(site) in PLAYER_UNIT_START_SITES


def _base_start(site):
    return norm_site(site) in BASE_UNIT_START_SITES


def parse_stream(path):
    """读 4 列专用流 ⇒ [(lineno, kind, arg, result, site)]（1-based 行号便于点名）。"""
    rows = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for lineno, line in enumerate(fh, start=1):
            line = line.rstrip("\r\n")
            if not line:
                continue
            parts = line.split("\t")
            if len(parts) < 4:
                continue
            site = parts[3] if len(parts) >= 4 else ""
            rows.append((lineno, parts[0], parts[1], parts[2], site))
    return rows


def decompose_units(rows):
    """按首站点把专用流切成攻方单元。

    返回 `(units, problems)`：unit = `{"kind": "player"|"base"|"unknown", "start_line": n,
    "start_site": s, "draws": m}`。首行不是 player/base 起点 ⇒ 一个 unknown 单元（也判红）。
    """
    units = []
    problems = []
    cur = None
    for idx, row in enumerate(rows, start=1):
        if len(row) == 5:
            lineno, _kind, _arg, _result, site = row
        else:  # 合成行（selftest）= (kind, arg, result, site)
            lineno = idx
            _kind, _arg, _result, site = row
        if _player_start(site):
            cur = {"kind": "player", "start_line": lineno, "start_site": norm_site(site), "draws": 0}
            units.append(cur)
        elif _base_start(site):
            cur = {"kind": "base", "start_line": lineno, "start_site": norm_site(site), "draws": 0}
            units.append(cur)
        elif cur is None:
            cur = {"kind": "unknown", "start_line": lineno, "start_site": norm_site(site), "draws": 0}
            units.append(cur)
        cur["draws"] += 1
    return units, problems


def judge(rows, expected):
    """返回 `(verdict, problems, counts)`；`verdict` ∈ GREEN/RED。"""
    problems = []
    units, _ = decompose_units(rows)
    player_units = [u for u in units if u["kind"] == "player"]
    base_units = [u for u in units if u["kind"] == "base"]
    unknown_units = [u for u in units if u["kind"] == "unknown"]
    counts = {
        "draws": len(rows),
        "units": len(units),
        "player_units": len(player_units),
        "base_units": len(base_units),
        "unknown_units": len(unknown_units),
        "expected_player_units": expected,
        "first_kind": (units[0]["kind"] if units else "none"),
        "first_site": (units[0]["start_site"] if units else ""),
    }
    if not rows:
        problems.append("empty_stream")
    if units and units[0]["kind"] != "player":
        problems.append(
            "first_unit_not_player: kind=%s line=%d site=%s（首条不是玩家攻方起点 ⇒ 污染）"
            % (units[0]["kind"], units[0]["start_line"], units[0]["start_site"]))
    if base_units:
        problems.append(
            "non_player_attacker_unit: base_units=%d 首个 line=%d site=%s（非玩家并发攻方混入专用流）"
            % (len(base_units), base_units[0]["start_line"], base_units[0]["start_site"]))
    if unknown_units:
        problems.append(
            "unknown_leading_unit: unknown_units=%d 首个 line=%d site=%s"
            % (len(unknown_units), unknown_units[0]["start_line"], unknown_units[0]["start_site"]))
    if expected is not None and len(player_units) != expected:
        problems.append(
            "player_units_mismatch: got=%d expected=%d（owner 23:50 ③：play 单元数 < 期望 ⇒ 拒跑）"
            % (len(player_units), expected))
    return ("GREEN" if not problems else "RED"), problems, counts


def _read_cmd(args):
    if not os.path.exists(args.stream):
        return 2, "PREREQ_MISSING: 找不到专用流 %s" % args.stream
    rows = parse_stream(args.stream)
    verdict, problems, counts = judge(rows, args.expected)
    payload = {
        "stream": args.stream,
        "expected_player_units": args.expected,
        "verdict": verdict,
        "counts": counts,
        "problems": problems,
    }
    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump(payload, fh, ensure_ascii=False, indent=1)
            fh.write("\n")
    print("=== 专用流纯度（attack_stream_gate）===")
    print("  stream=%s draws=%d units=%d player=%d base=%d unknown=%d expected=%s"
          % (args.stream, counts["draws"], counts["units"], counts["player_units"],
             counts["base_units"], counts["unknown_units"], args.expected))
    print("  首单元 kind=%s site=%s" % (counts["first_kind"], counts["first_site"]))
    if verdict == "GREEN":
        print("  GREEN: 专用流只含被测量玩家攻方单元（player=%d == expected，base=0）"
              % counts["player_units"])
        return 0, "GREEN"
    for p in problems:
        print("  RED: %s" % p)
    print("  结论：专用流被非玩家并发攻方污染 / 单元数不符 ⇒ **拒比**（fail-closed，不放宽判据）")
    return 1, "RED"


def _lines(rows):
    return "".join("%s\t%s\t%s\t%s\n" % (k, a, r, s) for (k, a, r, s) in rows)


def _clean_rows(n):
    """合成 n 个玩家单元（躲闪模板：a→b→gate→succ）。"""
    out = []
    for _ in range(n):
        out.append(("Next(max)", "1", "0", "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"))
        out.append(("Next(max)", "10", "3", "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934"))
        out.append(("Next(max)", "15", "7", "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"))
        out.append(("Next(max)", "5", "2", "M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223"))
    return out


def _cmd_selftest(_args):
    failed = 0

    def check(ok, name):
        nonlocal failed
        print(("PASS " if ok else "FAIL ") + name)
        if not ok:
            failed += 1

    # ① 干净流：expected=3 ⇒ GREEN。
    v, p, c = judge(_clean_rows(3), 3)
    check(v == "GREEN" and c["player_units"] == 3 and c["base_units"] == 0, "clean-green")

    # ② 注入一个 base 单元（模拟别图怪 AI 被吸进专用流）⇒ RED 且点名行号/site。
    polluted = list(_clean_rows(2))
    polluted.append(("Next(max)", "1", "0", "M2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892"))
    polluted.append(("Next(max)", "8", "3", "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"))
    polluted += _clean_rows(2)
    v2, p2, c2 = judge(polluted, 4)
    check(v2 == "RED" and c2["base_units"] == 1, "base-unit-red")
    check(any("non_player_attacker_unit" in x for x in p2), "base-unit-names-problem")

    # ③ 污染在**首条**（真机实测：C# 专用流首抽就是 atk_power_base）⇒ first_unit_not_player 必红。
    lead = [("Next(max)", "1", "0", "M2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892")]
    lead += _clean_rows(2)
    v3, p3, c3 = judge(lead, 2)
    check(v3 == "RED" and any("first_unit_not_player" in x for x in p3), "leading-base-red")

    # ④ player 单元数 < 期望（owner ③ 的例子）⇒ RED。
    v4, p4, c4 = judge(_clean_rows(2), 3)
    check(v4 == "RED" and any("player_units_mismatch" in x for x in p4), "player-count-short-red")
    # 反例：单元数多也算不符（不给静默放行）。
    v5, p5, _ = judge(_clean_rows(3), 2)
    check(v5 == "RED" and any("player_units_mismatch" in x for x in p5), "player-count-long-red")

    # ⑤ 空流 ⇒ RED（不许空流拿绿）。
    v6, p6, _ = judge([], 1)
    check(v6 == "RED" and any("empty_stream" in x for x in p6), "empty-red")

    # ⑥ Luck>0 首抽 :2918 也是合法 unit 起点（不误判 unknown）。
    luck = [("Next(max)", "1", "0", "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2918"),
            ("Next(max)", "10", "3", "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2924")]
    v7, p7, c7 = judge(luck, 1)
    check(v7 == "GREEN" and c7["player_units"] == 1, "luck-start-green")

    # ⑦ 端到端：污染流写盘 ⇒ rc=1；干净流 ⇒ rc=0（CLI 真跑）。
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        good = os.path.join(td, "good.tsv")
        bad = os.path.join(td, "bad.tsv")
        with open(good, "w", encoding="utf-8") as fh:
            fh.write(_lines(_clean_rows(3)))
        with open(bad, "w", encoding="utf-8") as fh:
            fh.write(_lines(lead + _clean_rows(2)))
        rc_good, _ = _read_cmd(argparse.Namespace(stream=good, expected=3, json=""))
        rc_bad, _ = _read_cmd(argparse.Namespace(stream=bad, expected=2, json=""))
        rc_missing, _ = _read_cmd(argparse.Namespace(stream=os.path.join(td, "nope.tsv"), expected=1, json=""))
    check(rc_good == 0, "cli-good-rc0")
    check(rc_bad == 1, "cli-bad-rc1")
    check(rc_missing == 2, "cli-missing-rc2")

    # 输出被上面 CLI 打印了；这里再单独打印汇总（避免被误当 stdout 污染）。
    print("SELFTEST " + ("GREEN" if failed == 0 else ("RED failed=%d" % failed)))
    return 0 if failed == 0 else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description="C# 攻击专用流纯度 fail-closed 门（F-N1）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("gate", help="判一条专用流是否只含被测量玩家攻方单元")
    g.add_argument("--stream", required=True)
    g.add_argument("--expected", type=int, required=True, help="期望的玩家攻方单元数（= -AttackCount）")
    g.add_argument("--json", default="")
    g.set_defaults(func=_read_cmd)
    s = sub.add_parser("selftest")
    s.set_defaults(func=_cmd_selftest)
    args = ap.parse_args(argv)
    res = args.func(args)
    return res[0] if isinstance(res, tuple) else res


if __name__ == "__main__":
    sys.exit(main())
