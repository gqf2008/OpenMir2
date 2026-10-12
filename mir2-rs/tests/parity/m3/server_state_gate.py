#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""server_state_gate.py — M3②③ 夹具的**目标可用性 fail-closed 门**（owner 2026-10-12 23:50 第 ③ 条）。

卡：`openmir2-m3-attack-pair-fixture`。输入 = **两侧同形**的**服务端权威逐击状态 dump**
（`MIR2_RNG_STATE_LOG` → `SERVER_STATE <seq> <json 数组>`，每个玩家攻击起点一行）：
C# 半见 `rng-hook/StartupHook.cs::DumpActors`；Rust 半见 `mir2-rs/bins/game-svr/src/state_log.rs`
（m2-svr 的 `start_rust_stack.ps1 -StateLog` 管线）。夹具侧两侧都由
`run_attack_pair.ps1::Invoke-M3TargetAvailabilityGate` 强制（任一侧缺 dump ⇒ RED）。

**为什么需要它（本片定位出的真因）**：C# 侧「目标格 (18,20) 从 k=4 起为空」**不是格子空了**——
服务端 dump 证明受害者一直站在 `(18,20)`、map `0137`，只是它的 `Ghost` 标志从 k=4 起被某条
oracle 路径静默置 1；`Envir.GetMovingObject(x,y,true)` 对 `Ghost` 直接返回 null ⇒ `GetPoseCreate()`
返回 null ⇒ `_Attack`/`AttackSuccess` 提前返回（RNG 里 `attack_success` 只 3 次）。客户端视图
（`attack_state.jsonl`）看不到这个 ghost ⇒ **只靠客户端状态记录会漏判**，必须用服务端 dump 咬住。

判据（fail-closed，默认严格）：
  * 取 dump 里**最后 N 行**（N = `--attacks`，本次侧运行的玩家攻击数）；
  * 每一行里必须存在一个「**非 attacker 的玩家族对象**」满足：
      `x == --victim-x`、`y == --victim-y`、`ghost == 0`、`death == 0`；
  * 允许前 `--tolerance` 击缺（默认 1：受害晚于 attacker 进世界/首帧未到，夹具已知的起跑竞态）；
    超过 tolerance ⇒ **RED 并点名该第几击**（1-based，按 dump 段内序号）；
  * dump 行数 < N、JSON 破损、找不到 attacker ⇒ PREREQ（rc=2），**不静默放行**。

退出码：0 = GREEN（目标在窗口内始终可用）；1 = RED（目标消失/ghost）；2 = PREREQ。
"""
import argparse
import json
import os
import sys


def parse_dump(path):
    """读 `SERVER_STATE <seq> <json>` 行 → [(seq, [actor,...] | None)]；非本格式行忽略。"""
    rows = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line.startswith("SERVER_STATE "):
                continue
            rest = line[len("SERVER_STATE "):]
            sp = rest.find(" ")
            if sp < 0:
                rows.append((int(rest or 0), None))
                continue
            seq = int(rest[:sp])
            payload = rest[sp + 1:].strip()
            try:
                actors = json.loads(payload)
            except ValueError:
                actors = None
            rows.append((seq, actors))
    return rows


def victim_available(actors, attacker_id, vx, vy):
    """本行是否有一个非 attacker 的玩家族活体站在 (vx,vy)。"""
    if not isinstance(actors, list):
        return None  # dump 本身坏了
    for a in actors:
        if not isinstance(a, dict):
            continue
        if a.get("id") == attacker_id:
            continue
        if int(a.get("x", -1)) == vx and int(a.get("y", -1)) == vy \
                and int(a.get("ghost", 1)) == 0 and int(a.get("death", 1)) == 0:
            return a
    return None


def gate(path, attacks, attacker_id, vx, vy, tolerance):
    rows = parse_dump(path)
    if len(rows) < attacks:
        return 2, "PREREQ: 服务端状态 dump 行数 %d < 本次攻击数 %d（%s）" % (len(rows), attacks, path), []
    seg = rows[-attacks:]
    bad = []
    for idx, (seq, actors) in enumerate(seg, 1):
        if actors is None:
            return 2, "PREREQ: 第 %d 行 dump 非法 JSON（seq=%s）" % (idx, seq), []
        if victim_available(actors, attacker_id, vx, vy) is None:
            bad.append((idx, seq))
    if len(bad) > tolerance:
        detail = ", ".join("#%d(seq=%s)" % (i, s) for i, s in bad[:8])
        return 1, ("RED: 目标在 %d/%d 击不可用（> tolerance %d）——受害不在 (%d,%d) 或 ghost/death=1；"
                   "前几处：%s" % (len(bad), attacks, tolerance, vx, vy, detail)), bad
    return 0, ("GREEN: 目标可用性门通过（%d 击里 %d 击受害在场且 !ghost && !death，tolerance=%d）"
               % (attacks, attacks - len(bad), tolerance)), bad


def _mk_line(seq, actors):
    return "SERVER_STATE %d %s" % (seq, json.dumps(actors, ensure_ascii=False))


def selftest():
    import tempfile
    tmp = tempfile.mkdtemp(prefix="server-state-gate-selftest-")
    checks = []

    def write(name, lines):
        p = os.path.join(tmp, name)
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
        return p

    def scene(ghost_from=None, victim_cell=(18, 20), n=5, atk=100):
        out = []
        for k in range(1, n + 1):
            ghost = 1 if (ghost_from is not None and k >= ghost_from) else 0
            out.append(_mk_line(k, [
                {"id": atk, "name": "a", "x": 17, "y": 20, "dir": 2, "ghost": 0, "death": 0, "hp": 19, "maxhp": 19, "map": "0137"},
                {"id": 200, "name": "v", "x": victim_cell[0], "y": victim_cell[1], "dir": 6, "ghost": ghost, "death": 0, "hp": 18, "maxhp": 19, "map": "0137"},
            ]))
        return out

    # (a) 生产形态（前 1 击受害未入场）⇒ GREEN
    p = write("ok.txt", scene(n=5, ghost_from=None))
    # 让第 1 击受害不在场
    lines = scene(n=5)
    lines[0] = _mk_line(1, [{"id": 100, "x": 17, "y": 20, "ghost": 0, "death": 0}])
    p = write("ok2.txt", lines)
    rc, msg, _ = gate(p, 5, 100, 18, 20, 1)
    checks.append(("green-with-1-tolerance", rc == 0, msg))

    # (b) 改坏：受害从第 4 击起 ghost=1 ⇒ RED 且点名 #4
    p = write("ghost.txt", scene(n=5, ghost_from=4))
    rc, msg, bad = gate(p, 5, 100, 18, 20, 1)
    checks.append(("mutation-ghost-red-names-k4", rc == 1 and bad and bad[0][0] == 4, msg))

    # (c) 改坏：受害移到别格 ⇒ RED
    p = write("moved.txt", scene(n=5, victim_cell=(19, 20)))
    rc, msg, _ = gate(p, 5, 100, 18, 20, 1)
    checks.append(("mutation-moved-red", rc == 1, msg))

    # (d) 行数不足 ⇒ PREREQ 2
    p = write("short.txt", scene(n=3))
    rc, msg, _ = gate(p, 5, 100, 18, 20, 1)
    checks.append(("prereq-short", rc == 2, msg))

    # (e) 非法 JSON ⇒ PREREQ 2
    p = write("badjson.txt", ["SERVER_STATE 1 [oops"] + scene(n=5)[1:])
    rc, msg, _ = gate(p, 5, 100, 18, 20, 1)
    checks.append(("prereq-badjson", rc == 2, msg))

    # (f) tolerance=0 时首击缺 ⇒ RED（牙：不许默认把缺口全吃掉）
    lines = scene(n=5)
    lines[0] = _mk_line(1, [{"id": 100, "x": 17, "y": 20, "ghost": 0, "death": 0}])
    p = write("tol0.txt", lines)
    rc, msg, _ = gate(p, 5, 100, 18, 20, 0)
    checks.append(("tolerance0-first-miss-red", rc == 1, msg))

    # (g) EOL：生产写出件必须 LF（owner 2026-10-13 01:15，EOL 陷阱同族第 5 次的根因修）。
    #     本文件的落盘一律 `open(..., newline="\n")`；改回默认（Windows 上 CRLF）⇒ 本项必红。
    with open(p, "rb") as fh:
        raw = fh.read()
    checks.append(("writer-lf-no-crlf", b"\r\n" not in raw,
                   "written bytes contain CRLF" if b"\r\n" in raw else "LF"))

    failed = [n for n, ok, _ in checks if not ok]
    for n, ok, msg in checks:
        print("%s %s :: %s" % ("PASS" if ok else "FAIL", n, msg))
    if failed:
        print("SELFTEST RED %d/%d failed=%s" % (len(failed), len(checks), ",".join(failed)))
        return 1
    print("SELFTEST GREEN %d/%d" % (len(checks), len(checks)))
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--state", help="SERVER_STATE dump 路径")
    ap.add_argument("--attacks", type=int, help="本次侧运行的玩家攻击数（取 dump 最后 N 行）")
    ap.add_argument("--attacker-id", type=int, help="攻方 ActorId（从 attack_raw.jsonl 的 role=attack.my 取）")
    ap.add_argument("--victim-x", type=int, help="受害应在的格 X（= 攻方前方格）")
    ap.add_argument("--victim-y", type=int, help="受害应在的格 Y")
    ap.add_argument("--tolerance", type=int, default=1, help="允许前几击缺目标（默认 1，起跑竞态）")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    if not args.state or args.attacks is None or args.attacker_id is None or args.victim_x is None or args.victim_y is None:
        print("PREREQ: 需要 --state/--attacks/--attacker-id/--victim-x/--victim-y（或 --selftest）", file=sys.stderr)
        return 2
    if not os.path.exists(args.state):
        print("PREREQ: 找不到服务端状态 dump %s" % args.state, file=sys.stderr)
        return 2
    rc, msg, _ = gate(args.state, args.attacks, args.attacker_id, args.victim_x, args.victim_y, args.tolerance)
    print(msg)
    return rc


if __name__ == "__main__":
    sys.exit(main())
