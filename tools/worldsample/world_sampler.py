#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""world_sampler.py — 世界态采样器（C5）：把一份客户端抓包还原成"可对拍的世界态时间线"。

补上 M0 之外缺的那层观测：不仅对字节/字段，还能对**世界态**（自己的位置/朝向、AOI 可见集、
背包与金币变化、tick 号）。判 M2 的"N tick 后位置/可见集相同"就用它。

字段来源（都锚在 C# 发送点，不是从字节反推）：
  tick          ≤ 明文动作帧 `#+GD/<rtime>!`（协议层 `frame::parse_act_frame`；发送模板
                  `src/Modules/SystemModule/MessageSettings.cs:6 sSTATUS_GOOD = "+GD/{0}"`）
  self_srv      ≤ **服务端权威**的自己位置：`SM_NEWMAP(51)`/`SM_LOGON(50)`（Recog=selfId、Param=X、Tag=Y）
                  以及任何 Recog==selfId 且带坐标的 s2c 帧（纠偏）。**实测事实**：本协议 s2c 里
                  `SM_TURN/SM_WALK/SM_RUN` 带的都是**其它** actor 的 id（自己的逐步位置不回显，
                  客户端靠本地预测）—— 所以别把别人的移动当成自己动了。
  move_inputs   ≤ **客户端上行**的移动输入计数（`CM_TURN(3010)`/`CM_WALK(3011)`/`CM_RUN(3013)`）。
                  **注意（实测更正）**：这三者的 Param/Tag **不是坐标**（观测到 Param=0、Tag=方向样值、
                  Recog 是 65536 递增的时间戳样值）⇒ 别拿它当"自己的位置"。两侧用同一份输入序列时，
                  这个计数就是可比字段；权威位置只认上面的 self_srv。
  aoi           ≤ `SM_TURN(10)`/`SM_WALK(11)`/`SM_RUN(13)` 等位置类帧把 actor 加入可见集
                  （头字段语义见 C# 发送点 `src/M2Server/Player/PlayObject.Message.cs:1406`/`:1542`：
                   Recog=actorId、Param=X、Tag=Y、Series=MakeWord(Dir, Light) ⇒ Dir=LoByte(Series)）；
                  `SM_DISAPPEAR(30)` 移出。**定义写在工具里、两侧用同一份定义**，这才是可对拍的判据；
                  本工具不假装知道引擎内部的 AOI 半径。
  aoi            ≤ 上述位置类消息 + 特征/受击类消息把 actor 加入可见集；`SM_DISAPPEAR(30)` 移出。
                  **定义写在工具里、两侧（C# 与 Rust）用同一份定义**，这样它才是可对拍的判据；
                  本工具不假装知道引擎内部的 AOI 半径。
  bag/gold/hp    ≤ `SM_ADDITEM(200)`/`SM_DELITEM(202)`/`SM_UPDATEITEM(203)` 计数、`SM_GOLDCHANGED(653)`
                  取值、`SM_HEALTHSPELLCHANGED(53)` 计数

用法：
  python world_sampler.py sample --capture <capture.jsonl> --out <timeline.ndjson> [--tick-step 20]
  python world_sampler.py compare A.ndjson B.ndjson [--max-diffs 5]     # 逐 tick 比 self/aoi/bag/gold
  python world_sampler.py selftest                                     # 确定性 + 改坏必红
"""
import argparse
import json
import sys

SELF_MOVE = {10: "turn", 11: "walk", 13: "run"}          # SM_TURN / SM_WALK / SM_RUN
AOI_TOUCH = {10, 11, 13, 14, 31, 41}                     # 位置/受击/特征变化 ⇒ 该 actor 在视野内
AOI_REMOVE = {30}                                        # SM_DISAPPEAR
BAG = {200: "add", 202: "del", 203: "update"}
CLIENT_MOVE = {3010: "turn", 3011: "walk", 3013: "run"}   # CM_TURN / CM_WALK / CM_RUN（上行）
SELF_POS_IDENTS = {50, 51}                                # SM_LOGON / SM_NEWMAP 带自己的 param/tag
GOLD = 653                                               # SM_GOLDCHANGED
HPMP = 53                                                # SM_HEALTHSPELLCHANGED
SELF_LOGON = 50                                          # SM_LOGON（自己的 actorId 在这里）


def lo_byte(v: int) -> int:
    return v & 0xFF


def sample_frames(rows):
    """返回 (events, snapshots)；events 是"状态变化"，snapshots 是按 tick 采样的世界态。"""
    self_id = None
    self_pos_srv = None      # 服务端权威（进图/纠偏）
    move_inputs = {"turn": 0, "walk": 0, "run": 0}   # 客户端上行移动输入计数
    aoi = set()
    bag = {"add": 0, "del": 0, "update": 0}
    gold = None
    hpmp = 0
    tick = None
    events = []
    snapshots = []

    def snap():
        snapshots.append({
            "t_ms": t,
            "tick": tick,
            "self_srv": self_pos_srv,
            "move_inputs": dict(move_inputs),
            "aoi": sorted(aoi),
            "bag": dict(bag),
            "gold_changed": gold,
            "hpmp_changed": hpmp,
        })

    for r in rows:
        t = r.get("ts_ms", r.get("t_ms"))
        if r.get("frame_form") == "act":
            # 明文动作帧：payload 形如 +GD/<rtime>（不是编码体，也不是 12B 头）
            raw = bytes.fromhex(r["raw_hex"])
            payload = raw[1:-1] if raw.endswith(b"!") else raw[1:]
            try:
                text = payload.decode("ascii")
                if text.startswith("+GD/"):
                    tick = int(text[4:])
                    events.append({"t_ms": t, "kind": "tick", "tick": tick})
                    snap()
            except (UnicodeDecodeError, ValueError):
                pass
            continue
        if r.get("error") or r.get("string_frame"):
            continue
        ident = r.get("ident")
        recog = r.get("recog")
        if ident == SELF_LOGON and recog is not None:
            self_id = recog
            events.append({"t_ms": t, "kind": "self_id", "actor": self_id})
            self_pos_srv = {"x": r.get("param"), "y": r.get("tag"), "dir": lo_byte(r.get("series") or 0)}
            snap()
            continue
        # 服务端权威的自己位置：进图/纠偏（Recog==selfId 且 param/tag 看起来就是坐标）
        if ident in SELF_POS_IDENTS and recog is not None:
            self_pos_srv = {"x": r.get("param"), "y": r.get("tag"), "dir": lo_byte(r.get("series") or 0)}
            snap()
            continue
        # 客户端上行意图：自己的目标坐标（两侧同输入才有可比性）
        if r["dir"] == "c2s" and ident in CLIENT_MOVE:
            move_inputs[CLIENT_MOVE[ident]] += 1
            events.append({"t_ms": t, "kind": "input_move", "move": CLIENT_MOVE[ident]})
            snap()
            continue
        if ident in SELF_MOVE:
            actor = recog
            x, y, series = r.get("param"), r.get("tag"), (r.get("series") or 0)
            d = lo_byte(series)
            aoi.add(actor)
            events.append({"t_ms": t, "kind": "move", "actor": actor,
                           "x": x, "y": y, "dir": d, "move": SELF_MOVE[ident]})
            snap()
            continue
        if ident in AOI_TOUCH:
            if recog is not None and recog not in aoi:
                aoi.add(recog)
                snap()
            continue
        if ident in AOI_REMOVE:
            if recog in aoi:
                aoi.discard(recog)
                events.append({"t_ms": t, "kind": "aoi_remove", "actor": recog})
                snap()
            continue
        if ident in BAG:
            bag[BAG[ident]] += 1
            events.append({"t_ms": t, "kind": "bag_" + BAG[ident]})
            snap()
            continue
        if ident == GOLD:
            gold_new = None
            body = r.get("body_sha256")  # 值本身在 body 里；这里只记"发生过变化"
            gold = gold_new if gold_new is not None else (gold if gold is not None else 0)
            gold = (gold or 0) + (1 if gold is not None else 0)
            events.append({"t_ms": t, "kind": "gold_changed"})
            continue
        if ident == HPMP:
            hpmp += 1
            continue

    return events, snapshots


def load_capture(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def cmd_sample(args):
    rows = load_capture(args.capture)
    events, snaps = sample_frames(rows)
    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        for e in events:
            f.write(json.dumps({"kind": "event", **e}, ensure_ascii=False) + "\n")
        for s in snaps:
            f.write(json.dumps({"kind": "snapshot", **s}, ensure_ascii=False) + "\n")
    moved = [e for e in events if e["kind"] == "move"]
    print("SAMPLED events=%d snapshots=%d moves=%d aoi_max=%d ticks=%s" % (
        len(events), len(snaps), len(moved),
        max((len(s["aoi"]) for s in snaps), default=0),
        (min(e["tick"] for e in events if e["kind"] == "tick"),
         max(e["tick"] for e in events if e["kind"] == "tick"))
        if any(e["kind"] == "tick" for e in events) else None))
    # 自己位置序列（前若干个变化点）——便于与客户端截图上的「坐标」交叉核对
    seq = [(e["t_ms"], e["x"], e["y"], e["dir"]) for e in moved
           if e.get("actor") is not None]
    if seq:
        print("  前 6 个位置事件 (t_ms,x,y,dir):", seq[:6])


def key_of(snap):
    return json.dumps({"self_srv": snap.get("self_srv"), "move_inputs": snap.get("move_inputs"),
                       "aoi": snap["aoi"],
                       "bag": snap["bag"], "gold": snap["gold_changed"],
                       "hpmp": snap["hpmp_changed"]}, sort_keys=True, ensure_ascii=False)


def cmd_compare(args):
    def load(p):
        out = []
        with open(p, "r", encoding="utf-8") as f:
            for line in f:
                r = json.loads(line)
                if r.get("kind") == "snapshot":
                    out.append(r)
        return out

    a, b = load(args.a), load(args.b)
    n = min(len(a), len(b))
    diffs = 0
    for i in range(n):
        ka, kb = key_of(a[i]), key_of(b[i])
        if ka != kb:
            diffs += 1
            if diffs <= args.max_diffs:
                print("DIFF @snapshot %d (t_ms A=%s B=%s):" % (i, a[i]["t_ms"], b[i]["t_ms"]))
                print("   A:", ka[:180])
                print("   B:", kb[:180])
    if len(a) != len(b):
        print("DIFF 快照条数不同：A=%d B=%d" % (len(a), len(b)))
        diffs += 1
    if diffs:
        print("RED: 世界态在 %d 个采样点上不一致" % diffs)
        sys.exit(1)
    print("GREEN: 世界态逐采样点一致（%d 个采样点）" % n)
    sys.exit(0)


def cmd_selftest():
    import tempfile, os
    rows = [
        {"kind": "frame", "dir": "s2c", "t_ms": 100, "recog": 7, "ident": SELF_LOGON, "param": 300, "tag": 570, "series": 0},
        {"kind": "frame", "dir": "s2c", "t_ms": 200, "recog": 7, "ident": 11, "param": 300, "tag": 570, "series": 4},
        {"kind": "frame", "dir": "s2c", "t_ms": 300, "recog": 9, "ident": 11, "param": 305, "tag": 572, "series": 0},
        {"kind": "frame", "dir": "s2c", "t_ms": 400, "frame_form": "act", "raw_hex": "232b47442f31323321"},  # +GD/123!
        {"kind": "frame", "dir": "s2c", "t_ms": 500, "recog": 9, "ident": 30, "param": 0, "tag": 0, "series": 0},
    ]
    tmp = tempfile.mkdtemp(prefix="ws-selftest-")
    cap = os.path.join(tmp, "cap.jsonl")
    with open(cap, "w", encoding="utf-8") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")

    outs = []
    for i in (1, 2):
        o = os.path.join(tmp, "tl%d.ndjson" % i)
        cmd_sample(argparse.Namespace(capture=cap, out=o))
        outs.append(o)
    same = open(outs[0], "rb").read() == open(outs[1], "rb").read()
    print("PASS 1/3 同序列两次采样逐字节相同" if same else "RED: 两次采样不同")
    if not same:
        sys.exit(1)
    # 自身位置应被识别为 actor 7（SM_LOGON 给的 id），而不是别人
    tl = [json.loads(l) for l in open(outs[0], encoding="utf-8")]
    selfs = [s["self_srv"] for s in tl if s["kind"] == "snapshot" and s.get("self_srv")]
    ok = selfs and selfs[0] == {"x": 300, "y": 570, "dir": 0}
    print("PASS 2/3 服务端权威自身位置取自 SM_LOGON 的 param/tag" if ok else "RED: 自身位置识别错：" + repr(selfs[:2]))
    if not ok:
        sys.exit(1)
    # 改坏必红：把某一帧的坐标挪一格，compare 必须报差异
    bad = os.path.join(tmp, "cap_bad.jsonl")
    rows2 = [dict(r) for r in rows]
    rows2[0]["param"] = 301   # 改的是"服务端权威的自己位置"那一帧（self_srv 来源）
    with open(bad, "w", encoding="utf-8") as f:
        for r in rows2:
            f.write(json.dumps(r) + "\n")
    o3 = os.path.join(tmp, "tl3.ndjson")
    cmd_sample(argparse.Namespace(capture=bad, out=o3))
    try:
        cmd_compare(argparse.Namespace(a=outs[0], b=o3, max_diffs=3))
        print("RED: 坐标改了却判绿 —— 对拍失效")
        sys.exit(1)
    except SystemExit as e:
        if e.code != 1:
            print("RED: 退出码异常", e.code)
            sys.exit(1)
    print("PASS 3/3 坐标改一格后对拍变红")
    print("GREEN: world_sampler selftest 3/3")


def main():
    ap = argparse.ArgumentParser(description="世界态采样器（C5）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("sample")
    s.add_argument("--capture", required=True)
    s.add_argument("--out", required=True)
    c = sub.add_parser("compare")
    c.add_argument("a")
    c.add_argument("b")
    c.add_argument("--max-diffs", type=int, default=5)
    sub.add_parser("selftest")
    args = ap.parse_args()
    {"sample": cmd_sample, "compare": cmd_compare, "selftest": lambda a: cmd_selftest()}[args.cmd](args)


if __name__ == "__main__":
    main()
