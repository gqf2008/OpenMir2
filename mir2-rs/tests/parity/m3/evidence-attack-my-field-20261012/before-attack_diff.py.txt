#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""attack_diff.py — M3②③ 真机同种子攻击对拍夹具的**对账半段**。

卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）。设计见卡 comment（度量口径）。

一条「侧运行」（C# 或 Rust 栈）里，BotSrv 的固定攻击序列（白名单 T-22 扩展，默认关）
从**本假人视角**产出行式原始日志：

    M3ATTSEQ {"role":"attack","k":1,"op":"CM_HIT","x":289,"y":618,"dir":2,"my":101,"target":55}
    M3ATTSEQ {"role":"struck","dmg":12,"hp":7,"maxhp":19,"src":101,"actor":55}
    M3ATTSEQ {"role":"done","attacks":120}

- `role=attack`：attacker 每步发出一条 `CM_HIT` 时落一条（`k` 从 1 起单调）。
- `role=struck`：本假人收到的 `SM_STRUCK`（= 观察者视角；`dmg` = oracle `MakeMessage(SM_STRUCK,
  ActorId, HP, MaxHP, wParam)` 的 `series`=本次伤害，`src`=`MessageBodyWL.Tag1`=命中者 ActorId）。
- `role=done`：序列收尾。

本脚本做两件事（都不连网、不碰 DB）：

  align <raw.jsonl> --out <seq.jsonl>
      把一条侧运行的原始事件流**按「第 k 次」对齐**成逐次结果：
      遍历事件流，`role=attack` 更新 `current_k`；`role=struck` 且 `src == my` 归到最近一次
      尚未消费的 `k`（= hit，记 `dmg`）。`k` 无 struck ⇒ dodge（`dmg=0`）。
      输出每行 `{"k":k,"hit":bool,"dmg":int,"hp":int,"maxhp":int}`。
      末尾附一行 `{"role":"summary","attacked":N,"hits":H,"dodges":D,"sum":S}`。

  diff <a_seq.jsonl> <b_seq.jsonl>
      跨侧逐项比对（**判据本体**）：
        * 逐 k 比 `hit`/`dmg`（长度不等即 RED）；
        * 比统计四项 `attacked/hits/dodges/sum`；
        * **非平凡门**（防「恒 0/恒绿比较器」）：两侧都必须 `attacked>=1`、`hits>=1`、`sum>0`，
          否则 RED（没有一次真实命中 ⇒ 判据没有牙齿）。
      差异要么 0，要么按 §6.0-3 登记白名单（本脚本不静默放过任何一次差异）。

  selftest
      构造合成件自检「改坏必红 / 还原必绿」：
        (a) 两侧同一非平凡序列 ⇒ GREEN；
        (b) 改坏 a 侧某一次 dmg ⇒ RED；
        (c) 改坏 a 侧某一次 hit（hit→dodge）⇒ RED；
        (d) 两侧都全 0（attacked>0 但 hits=0）⇒ RED（非平凡门）；
        (e) 空序列 ⇒ RED；
        (f) 长度不等 ⇒ RED；
        (g) align 的窗口归属（struck 落到最近的 k）⇒ 期望逐次结果。
      真跑的是 align/diff 的生产函数；selftest 只喂合成输入（照 run_bag_equip_pair 自检口径）。

  pin_attempts <stdout.txt> [--expected N]
      从一次侧运行的 stdout 日志复算 pin 重试次数（owner 2026-10-12 16:30）：解析 `PIN_HOLD attempt=N`
      / `PIN_RETRY attempt=N` 行，输出与 `run_pair.ps1::Format-M3PinAttempts` 同形的记账串 + JSON。
      `attempts > expected` ⇒ rc=1（fail-closed，该运行与零重试运行不在同一全局 RNG 位置）；
      日志里无 `PIN_HOLD` ⇒ rc=2（pin 未 hold 住/日志截断）。历史会话（未落结构化字段）可用它复算。

退出码：0 = GREEN；1 = RED（有差异/非平凡门未过）；2 = PREREQ（文件缺失/空）。
"""
import argparse
import json
import os
import re
import sys


def _die(msg, rc=2):
    print(msg, file=sys.stderr)
    return rc


def read_raw(path):
    """读原始行式事件流：每行是 `M3ATTSEQ {json}` 或纯 `{json}`（PS 抽取后）。

    用 `utf-8-sig`：PowerShell 5.1 的 `Set-Content -Encoding utf8` / `Out-File` 会写 BOM，
    而 BOM 不是 JSON 的一部分（不因 BOM 误判成 RED）。"""
    events = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            idx = line.find("{")
            if idx < 0:
                continue
            payload = line[idx:]
            try:
                obj = json.loads(payload)
            except ValueError:
                continue
            if isinstance(obj, dict) and "role" in obj:
                events.append(obj)
    return events


def align_events(events):
    """把事件流对齐成逐次结果 + 统计。返回 (rows, summary)。

    归属规则（唯一口径，写进报告）：按**文件序**消费——`role=attack` 推进 `current_k`；
    `role=struck` 归给当前 `current_k`（且只认 `src == my`，即本假人自己打出的）。
    `current_k` 为 None（还没发第一步）时出现的 struck 丢弃（不计入任何 k）。

    F-F（option ③）：`death_k` = 首个命中后 `hp<=0` 的 k（受害者死亡）；两侧必须死在同一个 k，
    死亡后的步（无 struck）算 dodge（hit=False/dmg=0），同样逐项比对 ⇒ 死亡被**显式建模**而非忽略。
    """
    by_k = {}
    current_k = None
    my = None
    for ev in events:
        role = ev.get("role")
        if role == "attack":
            k = ev.get("k")
            if not isinstance(k, int) or k <= 0:
                continue
            current_k = k
            my = ev.get("my", my)
            by_k.setdefault(k, {"k": k, "hit": False, "dmg": 0, "hp": 0, "maxhp": 0})
        elif role == "struck":
            if current_k is None:
                continue
            if my is not None and ev.get("src") is not None and ev.get("src") != my:
                # 别人打的（不是本假人发出的攻击）——不计入本次序列。
                continue
            row = by_k.setdefault(current_k, {"k": current_k, "hit": False, "dmg": 0, "hp": 0, "maxhp": 0})
            dmg = ev.get("dmg")
            dmg = int(dmg) if isinstance(dmg, (int, float)) else 0
            # 同一个 k 收到多帧 struck（罕见）：累加伤害，取最后一帧的 HP。
            row["dmg"] += dmg
            row["hit"] = True
            row["hp"] = int(ev.get("hp", 0) or 0)
            row["maxhp"] = int(ev.get("maxhp", 0) or 0)
    rows = [by_k[k] for k in sorted(by_k.keys())]
    hits = sum(1 for r in rows if r["hit"])
    death_k, min_hp = death_stats(rows)
    summary = {
        "role": "summary",
        "attacked": len(rows),
        "hits": hits,
        "dodges": len(rows) - hits,
        "sum": sum(r["dmg"] for r in rows),
        "death_k": death_k,
        "min_hp": min_hp,
    }
    return rows, summary


def death_stats(rows):
    """F-F option ③ 的唯一死亡口径：首个「命中且 hp<=0」的 k；min_hp = 命中行的最小 HP。"""
    hit_rows = [r for r in rows if r.get("hit")]
    hp_vals = [int(r.get("hp", 0) or 0) for r in hit_rows]
    death_k = None
    for r in hit_rows:
        if int(r.get("hp", 0) or 0) <= 0:
            death_k = int(r.get("k", 0))
            break
    return death_k, (min(hp_vals) if hp_vals else 0)


def load_seq(path):
    rows, summary = [], None
    with open(path, "r", encoding="utf-8-sig") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            if obj.get("role") == "summary":
                summary = obj
            else:
                rows.append(obj)
    return rows, summary


def nontrivial(summary):
    """非平凡门：没有一次真实命中 ⇒ 判据没有牙齿（防恒 0 比较器）。"""
    if summary is None:
        return False, "缺 summary 行"
    if summary.get("attacked", 0) < 1:
        return False, "attacked=0（序列一次都没发出去）"
    if summary.get("hits", 0) < 1:
        return False, "hits=0（一次都没命中；无法区分实现与空序列）"
    if summary.get("sum", 0) <= 0:
        return False, "sum<=0（伤害全为 0；比较器无仲裁力）"
    return True, ""


def diff_seq(a_rows, a_sum, b_rows, b_sum):
    problems = []
    if a_sum is None or b_sum is None:
        return ["缺 summary 行（align 未跑或输入非本脚本产物）"]
    if len(a_rows) != len(b_rows):
        problems.append("长度不等: a=%d b=%d" % (len(a_rows), len(b_rows)))
    for i in range(min(len(a_rows), len(b_rows))):
        ra, rb = a_rows[i], b_rows[i]
        if ra.get("k") != rb.get("k"):
            problems.append("第 %d 项 k 不等: a=%s b=%s" % (i, ra.get("k"), rb.get("k")))
        if bool(ra.get("hit")) != bool(rb.get("hit")):
            problems.append("k=%s hit 不等: a=%s b=%s" % (ra.get("k"), ra.get("hit"), rb.get("hit")))
        if int(ra.get("dmg", 0)) != int(rb.get("dmg", 0)):
            problems.append("k=%s dmg 不等: a=%s b=%s" % (ra.get("k"), ra.get("dmg"), rb.get("dmg")))
        # F-F option ③：受害者 HP/MaxHP 逐项比（死亡轨迹也是判据的一部分，不只看 hit/dmg）。
        if int(ra.get("hp", 0) or 0) != int(rb.get("hp", 0) or 0):
            problems.append("k=%s hp 不等: a=%s b=%s" % (ra.get("k"), ra.get("hp"), rb.get("hp")))
        if int(ra.get("maxhp", 0) or 0) != int(rb.get("maxhp", 0) or 0):
            problems.append("k=%s maxhp 不等: a=%s b=%s" % (ra.get("k"), ra.get("maxhp"), rb.get("maxhp")))
    for key in ("attacked", "hits", "dodges", "sum", "death_k"):
        if a_sum.get(key) != b_sum.get(key):
            problems.append("统计 %s 不等: a=%s b=%s" % (key, a_sum.get(key), b_sum.get(key)))
    return problems


def cmd_align(args):
    if not os.path.exists(args.raw):
        return _die("PREREQ_MISSING: 找不到原始事件流 %s" % args.raw)
    events = read_raw(args.raw)
    if not events:
        return _die("PREREQ_MISSING: %s 里没有 M3ATTSEQ 事件（驱动没跑/开关没开）" % args.raw)
    rows, summary = align_events(events)
    out = args.out or (os.path.splitext(args.raw)[0] + ".seq.jsonl")
    with open(out, "w", encoding="utf-8", newline="\n") as fh:
        for r in rows:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")
        fh.write(json.dumps(summary, ensure_ascii=False) + "\n")
    print("ALIGN_OK %s attacked=%d hits=%d dodges=%d sum=%d" %
          (out, summary["attacked"], summary["hits"], summary["dodges"], summary["sum"]))
    return 0


def cmd_diff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到逐次序列 %s" % p)
    a_rows, a_sum = load_seq(args.a)
    b_rows, b_sum = load_seq(args.b)
    if not a_rows or not b_rows:
        print("RED: 空序列（a=%d b=%d）" % (len(a_rows), len(b_rows)))
        return 1
    ok_a, why_a = nontrivial(a_sum)
    ok_b, why_b = nontrivial(b_sum)
    print("NONTRIVIAL a=%s(%s) b=%s(%s)" % (ok_a, why_a, ok_b, why_b))
    if not ok_a or not ok_b:
        print("RED: 非平凡门未过——不容许用恒 0/恒绿比较器（%s / %s）" % (why_a, why_b))
        return 1
    problems = diff_seq(a_rows, a_sum, b_rows, b_sum)
    if not problems:
        print("GREEN: 跨侧攻击序列 diff = 0（attacked=%d hits=%d dodges=%d sum=%d）" %
              (a_sum["attacked"], a_sum["hits"], a_sum["dodges"], a_sum["sum"]))
        return 0
    print("RED: 跨侧攻击序列差异 %d 项（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）" %
          len(problems))
    for p in problems[: args.max_diffs]:
        print("  - " + p)
    if len(problems) > args.max_diffs:
        print("  ... 另有 %d 项（--max-diffs 截断）" % (len(problems) - args.max_diffs))
    return 1


def cmd_mutate(args):
    """把某一侧的逐次序列改坏（**仅供自检/红检构造反例**，不参与生产判据）。

    --field dmg --value V  ：把 k 行的 dmg 改成 V（并同步 summary.sum）。
    --field hit --value 0/1：把 k 行改成 dodge(0)/hit(1)（同步 hits/dodges/sum）。
    --field hp --value V   ：把 k 行的 hp 改成 V（并重算 summary.death_k/min_hp；F-F option ③ 红检）。
    --field maxhp --value V：把 k 行的 maxhp 改成 V。
    --field death_k --value V（V<0 表示 None）：直改 summary.death_k（死亡时点反例）。
    """
    if not os.path.exists(args.seq):
        return _die("PREREQ_MISSING: 找不到序列文件 %s" % args.seq)
    rows, summary = load_seq(args.seq)
    target = None
    for r in rows:
        if int(r.get("k", -1)) == args.k:
            target = r
            break
    if target is None:
        return _die("PREREQ_MISSING: 序列里没有 k=%d" % args.k)
    if args.field == "dmg":
        old = int(target.get("dmg", 0))
        target["dmg"] = args.value
        if summary is not None:
            summary["sum"] = summary.get("sum", 0) - old + args.value
    elif args.field == "hit":
        was = bool(target.get("hit"))
        now = bool(args.value)
        if was != now:
            if summary is not None:
                summary["hits"] = summary.get("hits", 0) + (1 if now else -1)
                summary["dodges"] = summary.get("dodges", 0) + (-1 if now else 1)
                if not now:
                    summary["sum"] = summary.get("sum", 0) - int(target.get("dmg", 0))
            target["hit"] = now
            if not now:
                target["dmg"] = 0
    elif args.field in ("hp", "maxhp"):
        target[args.field] = args.value
        if summary is not None:
            dk, mh = death_stats(rows)
            summary["death_k"] = dk
            summary["min_hp"] = mh
    elif args.field == "death_k":
        if summary is not None:
            summary["death_k"] = args.value if args.value >= 0 else None
    else:
        return _die("PREREQ_MISSING: --field 只支持 dmg|hit|hp|maxhp|death_k")
    with open(args.seq, "w", encoding="utf-8", newline="\n") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")
        if summary is not None:
            fh.write(json.dumps(summary) + "\n")
    print("MUTATE_OK %s k=%d field=%s value=%s" % (args.seq, args.k, args.field, args.value))
    return 0


def _write_raw(path, events):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        for ev in events:
            fh.write("M3ATTSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")


def _mk_events(n, my, base_dmg):
    events = []
    for k in range(1, n + 1):
        events.append({"role": "attack", "k": k, "op": "CM_HIT", "x": 289, "y": 618, "dir": 2, "my": my})
        # 第 3 次故意 miss（无 struck），其余命中。
        if k != 3:
            events.append({"role": "struck", "dmg": base_dmg + (k % 5), "hp": 100 - k, "maxhp": 100, "src": my, "actor": 55})
    events.append({"role": "done", "attacks": n})
    return events


def cmd_selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="attack-diff-selftest-")
    failures = []
    checks = []

    def check(name, cond, detail=""):
        checks.append(name)
        if not cond:
            failures.append(name + (" (%s)" % detail if detail else ""))

    # (g) align 窗口归属：struck 归到最近一次 k
    rawa = os.path.join(tmp, "a.raw.jsonl")
    _write_raw(rawa, _mk_events(10, my=101, base_dmg=20))
    rows, summ = align_events(read_raw(rawa))
    check("g-align-k", [r["k"] for r in rows] == list(range(1, 11)), str([r["k"] for r in rows]))
    check("g-align-miss3", rows[2]["hit"] is False and rows[2]["dmg"] == 0, str(rows[2]))
    check("g-align-hits", summ["hits"] == 9 and summ["dodges"] == 1, str(summ))
    check("g-align-sum", summ["sum"] == sum(20 + (k % 5) for k in range(1, 11) if k != 3), str(summ["sum"]))

    # (a) 同一非平凡序列 ⇒ GREEN
    a_seq = os.path.join(tmp, "a.seq.jsonl")
    b_seq = os.path.join(tmp, "b.seq.jsonl")
    with open(a_seq, "w", encoding="utf-8", newline="\n") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")
        fh.write(json.dumps(summ) + "\n")
    with open(b_seq, "w", encoding="utf-8", newline="\n") as fh:
        for r in rows:
            fh.write(json.dumps(r) + "\n")
        fh.write(json.dumps(summ) + "\n")
    rc = cmd_diff(argparse.Namespace(a=a_seq, b=b_seq, max_diffs=20))
    check("a-identical-green", rc == 0, "rc=%d" % rc)

    # (b) 改坏一次 dmg ⇒ RED
    b_rows, b_sum = load_seq(b_seq)
    b_rows[4]["dmg"] += 1
    with open(b_seq, "w", encoding="utf-8", newline="\n") as fh:
        for r in b_rows:
            fh.write(json.dumps(r) + "\n")
        fh.write(json.dumps(b_sum) + "\n")
    rc = cmd_diff(argparse.Namespace(a=a_seq, b=b_seq, max_diffs=20))
    check("b-dmg-mutation-red", rc == 1, "rc=%d" % rc)

    # (c) 改坏一次 hit ⇒ RED（还原 dmg 后再把 k=5 改成 dodge）
    b_rows[4]["dmg"] -= 1
    b_rows[4]["hit"] = False
    b_rows[4]["dmg"] = 0
    bad_sum = dict(b_sum)
    bad_sum["hits"] -= 1
    bad_sum["dodges"] += 1
    bad_sum["sum"] -= 20 + (5 % 5)
    with open(b_seq, "w", encoding="utf-8", newline="\n") as fh:
        for r in b_rows:
            fh.write(json.dumps(r) + "\n")
        fh.write(json.dumps(bad_sum) + "\n")
    rc = cmd_diff(argparse.Namespace(a=a_seq, b=b_seq, max_diffs=20))
    check("c-hit-mutation-red", rc == 1, "rc=%d" % rc)

    # (d) 两侧都全 0 ⇒ 非平凡门 RED
    zero = os.path.join(tmp, "zero.seq.jsonl")
    with open(zero, "w", encoding="utf-8", newline="\n") as fh:
        for k in range(1, 4):
            fh.write(json.dumps({"k": k, "hit": False, "dmg": 0, "hp": 0, "maxhp": 0}) + "\n")
        fh.write(json.dumps({"role": "summary", "attacked": 3, "hits": 0, "dodges": 3, "sum": 0}) + "\n")
    rc = cmd_diff(argparse.Namespace(a=zero, b=zero, max_diffs=20))
    check("d-all-zero-red", rc == 1, "rc=%d" % rc)

    # (e) 空序列 ⇒ RED
    empty = os.path.join(tmp, "empty.seq.jsonl")
    with open(empty, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(json.dumps({"role": "summary", "attacked": 0, "hits": 0, "dodges": 0, "sum": 0}) + "\n")
    rc = cmd_diff(argparse.Namespace(a=empty, b=b_seq, max_diffs=20))
    check("e-empty-red", rc == 1, "rc=%d" % rc)

    # (f) 长度不等 ⇒ RED
    short = os.path.join(tmp, "short.seq.jsonl")
    with open(short, "w", encoding="utf-8", newline="\n") as fh:
        for r in rows[:5]:
            fh.write(json.dumps(r) + "\n")
        fh.write(json.dumps({"role": "summary", "attacked": 5, "hits": 5, "dodges": 0, "sum": 99}) + "\n")
    rc = cmd_diff(argparse.Namespace(a=a_seq, b=short, max_diffs=20))
    check("f-length-mismatch-red", rc == 1, "rc=%d" % rc)

    # (h) F-F option ③：死亡容忍（两侧同种子在 k=7 死，k=8..10 无 struck）⇒ GREEN
    death_events = []
    for k in range(1, 11):
        death_events.append({"role": "attack", "k": k, "op": "CM_HIT", "x": 1, "y": 1, "dir": 2, "my": 101})
        if k <= 7:
            death_events.append({"role": "struck", "dmg": 3, "hp": max(0, 21 - 3 * k), "maxhp": 19, "src": 101, "actor": 55})
    death_events.append({"role": "done", "attacks": 10})
    draw = os.path.join(tmp, "death.raw.jsonl")
    _write_raw(draw, death_events)
    d_rows, d_sum = align_events(read_raw(draw))
    d_a = os.path.join(tmp, "death-a.seq.jsonl")
    d_b = os.path.join(tmp, "death-b.seq.jsonl")
    for p in (d_a, d_b):
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            for r in d_rows:
                fh.write(json.dumps(r) + "\n")
            fh.write(json.dumps(d_sum) + "\n")
    check("h-death-k7", d_sum.get("death_k") == 7, str(d_sum))
    rc = cmd_diff(argparse.Namespace(a=d_a, b=d_b, max_diffs=20))
    check("h-death-symmetric-green", rc == 0, "rc=%d" % rc)

    # (i) 死亡时点不同 ⇒ RED（只改 summary.death_k，行 hp 不动）
    rc = cmd_mutate(argparse.Namespace(seq=d_b, k=7, field="death_k", value=8))
    check("i-death-k-mutate-rc", rc == 0, "rc=%d" % rc)
    rc = cmd_diff(argparse.Namespace(a=d_a, b=d_b, max_diffs=20))
    check("i-death-k-mismatch-red", rc == 1, "rc=%d" % rc)

    # (j) 单步 HP 不等 ⇒ RED（死亡轨迹逐项比）
    for p in (d_b,):
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            for r in d_rows:
                fh.write(json.dumps(r) + "\n")
            fh.write(json.dumps(d_sum) + "\n")
    rc = cmd_mutate(argparse.Namespace(seq=d_b, k=3, field="hp", value=999))
    check("j-hp-mutate-rc", rc == 0, "rc=%d" % rc)
    rc = cmd_diff(argparse.Namespace(a=d_a, b=d_b, max_diffs=20))
    check("j-hp-mismatch-red", rc == 1, "rc=%d" % rc)

    # (k) pin_attempts 复算（owner 2026-10-12 16:30）：干净日志 attempts=2 ⇒ 绿；多一次重试 ⇒ 红；无 PIN_HOLD ⇒ PREREQ
    clean_log = os.path.join(tmp, "pin-clean.txt")
    with open(clean_log, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("PIN_RETRY attempt=1 settle_rounds=1 prev_match=False pinned=2/2（pin_hold_needs_consecutive_settle）\n")
        fh.write("PIN_HOLD attempt=2（连续两轮 settle 复验均 hold 住）\n")
    rc = cmd_pin_attempts(argparse.Namespace(log=clean_log, expected=2, max_attempts=5))
    check("k-pin-clean-green", rc == 0, "rc=%d" % rc)
    retry_log = os.path.join(tmp, "pin-retry.txt")
    with open(retry_log, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("PIN_RETRY attempt=1 settle_rounds=1 prev_match=False pinned=1/2（pin_char_mismatch）\n")
        fh.write("PIN_RETRY attempt=2 settle_rounds=2 prev_match=False pinned=1/2（pin_char_mismatch）\n")
        fh.write("PIN_RETRY attempt=3 settle_rounds=3 prev_match=False pinned=2/2（pin_hold_needs_consecutive_settle）\n")
        fh.write("PIN_HOLD attempt=4（连续两轮 settle 复验均 hold 住）\n")
    rc = cmd_pin_attempts(argparse.Namespace(log=retry_log, expected=2, max_attempts=5))
    check("k-pin-retry-red", rc == 1, "rc=%d" % rc)
    rc = cmd_pin_attempts(argparse.Namespace(log=os.path.join(tmp, "pin-nope.txt"), expected=2, max_attempts=5))
    check("k-pin-missing-prereq", rc == 2, "rc=%d" % rc)

    # 清理
    for name in os.listdir(tmp):
        try:
            os.remove(os.path.join(tmp, name))
        except OSError:
            pass
    os.rmdir(tmp)

    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: attack_diff selftest %d/%d（align 窗口归属 + 同序列绿 + 改坏 dmg/hit 必红 + "
          "全 0 非平凡门 + 空序列 + 长度不等 + 死亡容忍/死亡时点/HP 逐项比（F-F option ③） + "
          "pin_attempts 复算（干净=绿 / 多一次重试=红 / 缺 PIN_HOLD=PREREQ））" % (len(checks), len(checks)))
    return 0


def cmd_pin_attempts(args):
    """从一次侧运行的 stdout 日志复算 pin 重试次数（owner 2026-10-12 16:30）。

    日志行形如（run_pair.ps1 的 pin 循环原样打印）：
        PIN_RETRY attempt=1 settle_rounds=1 prev_match=False pinned=1/2 ...（pin_char_mismatch）
        PIN_HOLD attempt=2（连续两轮 settle 复验均 hold 住）
    期望值口径 = 2（`Get-M3PinHoldVerdict` 的「连续两轮 settle」结构性最小 attempts）。
    每次 `PIN_RETRY` = 多跑一轮 pin+settle+observe ⇒ 推移攻击序列起始的全局 RNG 位置。
    """
    if not os.path.exists(args.log):
        return _die("PREREQ_MISSING: 找不到日志 %s" % args.log)
    hold_attempt = None
    retry_lines = 0
    max_retry_attempt = 0
    with open(args.log, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            m = re.search(r"PIN_HOLD\s+attempt=(\d+)", line)
            if m:
                hold_attempt = int(m.group(1))
                continue
            m = re.search(r"PIN_RETRY\s+attempt=(\d+)", line)
            if m:
                retry_lines += 1
                max_retry_attempt = max(max_retry_attempt, int(m.group(1)))
    if hold_attempt is None:
        print("PREREQ_MISSING: %s 里没有 PIN_HOLD 行（pin 未 hold 住/日志被截断）" % args.log)
        return 2
    attempts = hold_attempt
    expected = args.expected
    extra = max(0, attempts - expected)
    ok = (attempts == expected)
    # 生产 Settle 脚本块恒 return $true ⇒ settle_rounds == attempts（与 run_pair 的 Format-M3PinAttempts 同源）。
    canonical = "attempts=%d expected=%d settle_rounds=%d max_attempts=%d" % (attempts, expected, attempts, args.max_attempts)
    print("PIN_ATTEMPTS %s hold_attempt=%d pin_retry_lines=%d extra_retries=%d ok=%s" %
          (canonical, hold_attempt, retry_lines, extra, ok))
    print(json.dumps({
        "attempts": attempts, "expected": expected, "hold_attempt": hold_attempt,
        "pin_retry_lines": retry_lines, "max_retry_attempt": max_retry_attempt,
        "extra_retries": extra, "ok": ok, "log": args.log,
    }, ensure_ascii=False))
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 attack pair diff")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_align = sub.add_parser("align")
    p_align.add_argument("raw")
    p_align.add_argument("--out", default="")
    p_align.set_defaults(func=cmd_align)

    p_diff = sub.add_parser("diff")
    p_diff.add_argument("a")
    p_diff.add_argument("b")
    p_diff.add_argument("--max-diffs", type=int, default=20)
    p_diff.set_defaults(func=cmd_diff)

    p_st = sub.add_parser("selftest")
    p_st.set_defaults(func=cmd_selftest)

    p_mu = sub.add_parser("mutate")
    p_mu.add_argument("seq")
    p_mu.add_argument("--k", type=int, required=True)
    p_mu.add_argument("--field", required=True)
    p_mu.add_argument("--value", type=int, required=True)
    p_mu.set_defaults(func=cmd_mutate)

    p_pa = sub.add_parser("pin_attempts")
    p_pa.add_argument("log")
    p_pa.add_argument("--expected", type=int, default=2)
    p_pa.add_argument("--max-attempts", type=int, default=5)
    p_pa.set_defaults(func=cmd_pin_attempts)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
