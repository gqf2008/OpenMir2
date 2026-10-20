#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_timing.py — 基于 capture.jsonl 帧时间戳的通用时序判据（评审 C4）。

现有 checker（check_client_s2c_frames / check_attack_observability /
check_healthspellchanged_trigger / aoi_visibility_pair / s2c_diff）只判计数或序列，
不读 `ts_ms`，所以 200ms 前导、1.2s 节拍、30s/60s 门、600/1200ms、3/5min 都没法机判。
本工具补这一类：

输入：
  --capture capture.jsonl     （`tools/m2-acceptance/export_s2c.ps1` 产出，帧带 ts_ms）
  --config  rules.json        （规则列表；--example-config 打印模板）

规则类型（每条一个 name，失败即红）：
  after     「某帧在另一帧后 X ms 内」：对每个 from 帧，要求存在一个 to 帧，
            满足 from.seq < to.seq 且 min_ms <= to.ts - from.ts <= max_ms。
            典型：SM_HIT(14) → SM_STRUCK(31) ≤250ms（200ms 前导）、
                  登录 → 某帧 ≤30s/60s、技能 → 效果 ≤3/5min。
  interval  「连续帧间隔在 [min_ms,max_ms]」：对 match 命中的帧按 ts 排序，
            相邻间隔逐对判；`skip` 可跳过前 N 个间隔（首帧常与前序无关）；
            `min_pairs` 要求至少判 N 个间隔（默认 1，防空绿）。
            典型：1.2s 节拍、600/1200ms。
  rate      「每 window_ms 内最多 max_count 帧」（滑动窗）。
            典型：50/s vs 40/s 的发送速率上限。

过滤器（match/from/to 通用）：
  {"dir":"s2c","ident":14}          ident 可写整数或数组 [14,31]
  {"recog":123,"conn":1,"port":7200,"hop":"game"}
  另支持 seq_min/seq_max。

示例（--example-config 同内容，按需删减/改 ident）：
  {"rules":[
    {"name":"sm_hit-leads-sm_struck-250ms","type":"after",
     "from":{"dir":"s2c","ident":14},"to":{"dir":"s2c","ident":31},
     "min_ms":0,"max_ms":250,"allowed_missing":0},
    {"name":"login-1200ms-cadence","type":"interval",
     "match":{"dir":"s2c","ident":53},"min_ms":1000,"max_ms":1400,"min_pairs":1},
    {"name":"gate-30s","type":"after",
     "from":{"dir":"c2s","ident":3014},"to":{"dir":"s2c","ident":31},
     "min_ms":0,"max_ms":30000,"allowed_missing":0},
    {"name":"gate-60s","type":"after",
     "from":{"dir":"c2s","ident":3014},"to":{"dir":"s2c","ident":34},
     "min_ms":0,"max_ms":60000,"allowed_missing":0},
    {"name":"ritual-600-1200ms","type":"interval",
     "match":{"dir":"s2c","ident":53},"min_ms":600,"max_ms":1200,"min_pairs":1},
    {"name":"corpse-3min","type":"after",
     "from":{"dir":"s2c","ident":34},"to":{"dir":"s2c","ident":35},
     "min_ms":0,"max_ms":180000,"allowed_missing":0},
    {"name":"corpse-5min","type":"after",
     "from":{"dir":"s2c","ident":34},"to":{"dir":"s2c","ident":36},
     "min_ms":0,"max_ms":300000,"allowed_missing":0},
    {"name":"send-rate-50-per-s","type":"rate",
     "match":{"dir":"s2c"},"window_ms":1000,"max_count":50}
  ]}

内置规则预设（C10#3；`--preset NAME`，可重复，`all` = 全部四条）：
  cadence-1.2s       玩家 `SearchViewRange` 节拍（SearchTime=1000ms，200ms tick ⇒ ≈1200ms）
  verify-maptime-30s `VerifyMapTime` 30s 门（严格 `>30s`，200ms tick ⇒ 30000..30400ms）
  delaymagic-600ms   `RM_DELAYMAGIC` 600ms 延时投递（到点 tick ⇒ 600..800ms）
  corpse-3min        `MakeGhostTime` 180s 尸体变鬼（严格 `>180s` ⇒ 180001..180200ms）
  四条预设都要求**服务端事件流**：capture.jsonl 每行需带整数 `ts_ms` 与 `event` 字段，
  事件名分别为 `search_view_range` / `verify_map_time` / `delay_magic_schedule` /
  `delay_magic_fire` / `death` / `make_ghost`；`--preset-var actor=<id>` 注入 `actor` 过滤
  （**没有 actor 过滤不许判节拍**，否则会混多个 actor 的序列）。普通 s2c capture 没有这些帧 ⇒
  预设如实判 `UNCLOSED`（rc=3），绝不因「没采到」而 GREEN。

退出码：0 GREEN / 1 RED / 2 PREREQ_MISSING（缺文件/JSON 坏/规则非法/帧缺 ts_ms/预设缺变量）/
        3 UNCLOSED（预设的观测物不在 capture 里，判不了；不是通过）。
`--selftest` 不依赖外部文件。
"""

from __future__ import annotations

import argparse
import json
import os
import sys

REQUIRED_TS = "ts_ms"


def load_capture(path):
    if not os.path.isfile(path):
        raise FileNotFoundError(path)
    rows = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def _as_set(v):
    if isinstance(v, list):
        return set(v)
    return {v}


def frame_matches(frame, flt):
    if not flt:
        return True
    for key, want in flt.items():
        if key in ("seq_min", "seq_max"):
            continue
        got = frame.get(key)
        if isinstance(want, list):
            if got not in want:
                return False
        elif got != want:
            return False
    if "seq_min" in flt and frame.get("seq", -1) < flt["seq_min"]:
        return False
    if "seq_max" in flt and frame.get("seq", 10**18) > flt["seq_max"]:
        return False
    return True


def select(rows, flt):
    out = [r for r in rows if frame_matches(r, flt)]
    if out and any(not isinstance(r.get(REQUIRED_TS), int) for r in out):
        raise ValueError("命中帧缺整数 %s，无法判时序" % REQUIRED_TS)
    return out


def _window(rule):
    max_ms = rule.get("max_ms", rule.get("within_ms"))
    if max_ms is None:
        raise ValueError("规则 %r 缺 max_ms/within_ms" % rule.get("name"))
    min_ms = rule.get("min_ms", 0)
    if min_ms > max_ms:
        raise ValueError("规则 %r min_ms > max_ms" % rule.get("name"))
    return min_ms, max_ms


def rule_after(rows, rule):
    min_ms, max_ms = _window(rule)
    froms = select(rows, rule.get("from"))
    tos = select(rows, rule.get("to"))
    allowed = int(rule.get("allowed_missing", 0))
    missing = []
    matched = 0
    for f in froms:
        ok = False
        for t in tos:
            f_seq = f.get("seq")
            t_seq = t.get("seq")
            if isinstance(f_seq, int) and isinstance(t_seq, int) and t_seq <= f_seq:
                continue
            if f.get("conn") is not None and t.get("conn") != f.get("conn"):
                continue
            dt = t[REQUIRED_TS] - f[REQUIRED_TS]
            if min_ms <= dt <= max_ms:
                ok = True
                break
        if ok:
            matched += 1
        else:
            missing.append({"from_seq": f.get("seq"), "from_ts": f.get(REQUIRED_TS),
                            "conn": f.get("conn")})
    fails = []
    if not froms:
        fails.append("规则 %s：from 帧 0 条（空判据 fail-closed；确认过滤条件）"
                     % rule.get("name"))
    min_matched = int(rule.get("min_matched", 1))
    if len(missing) > allowed:
        fails.append("规则 %s：%d/%d 条 from 帧在 %d..%dms 内没有对应 to 帧（允许缺 %d）"
                     % (rule.get("name"), len(missing), len(froms), min_ms, max_ms, allowed))
    if matched < min_matched:
        fails.append("规则 %s：只匹配到 %d 条 < min_matched %d（防空绿）"
                     % (rule.get("name"), matched, min_matched))
    facts = {"type": "after", "from": len(froms), "to": len(tos),
             "matched": matched, "missing": len(missing), "missing_examples": missing[:5],
             "min_ms": min_ms, "max_ms": max_ms}
    return fails, facts


def rule_interval(rows, rule):
    min_ms, max_ms = _window(rule)
    frames = sorted(select(rows, rule.get("match")), key=lambda r: r[REQUIRED_TS])
    skip = int(rule.get("skip", 0))
    min_pairs = int(rule.get("min_pairs", 1))
    gaps = [frames[i][REQUIRED_TS] - frames[i - 1][REQUIRED_TS]
            for i in range(1, len(frames))]
    gaps = gaps[skip:]
    bad = [g for g in gaps if not (min_ms <= g <= max_ms)]
    fails = []
    if len(gaps) < min_pairs:
        fails.append("规则 %s：可比间隔只有 %d 个 < min_pairs %d（帧数 %d，skip %d）"
                     % (rule.get("name"), len(gaps), min_pairs, len(frames), skip))
    if bad:
        fails.append("规则 %s：%d/%d 个间隔不在 %d..%dms（首个越界 %dms）"
                     % (rule.get("name"), len(bad), len(gaps), min_ms, max_ms, bad[0]))
    facts = {"type": "interval", "frames": len(frames), "gaps": len(gaps),
             "bad": len(bad), "bad_examples": bad[:10],
             "min_ms": min_ms, "max_ms": max_ms, "skip": skip}
    return fails, facts


def rule_rate(rows, rule):
    frames = sorted(select(rows, rule.get("match")), key=lambda r: r[REQUIRED_TS])
    window_ms = int(rule.get("window_ms", 1000))
    max_count = int(rule.get("max_count", 0))
    if window_ms <= 0 or max_count <= 0:
        raise ValueError("规则 %r 的 window_ms/max_count 必须 > 0" % rule.get("name"))
    worst = 0
    worst_at = None
    j = 0
    for i in range(len(frames)):
        if j < i:
            j = i
        while j < len(frames) and frames[j][REQUIRED_TS] - frames[i][REQUIRED_TS] <= window_ms:
            j += 1
        count = j - i
        if count > worst:
            worst = count
            worst_at = frames[i][REQUIRED_TS]
    fails = []
    if len(frames) == 0:
        fails.append("规则 %s：match 帧 0 条（空判据 fail-closed）" % rule.get("name"))
    elif worst > max_count:
        fails.append("规则 %s：%dms 滑动窗内最多 %d 帧 > 上限 %d（起始 ts=%s）"
                     % (rule.get("name"), window_ms, worst, max_count, worst_at))
    facts = {"type": "rate", "frames": len(frames), "window_ms": window_ms,
             "max_count": max_count, "worst": worst, "worst_at_ts": worst_at}
    return fails, facts


RULE_KINDS = {"after": rule_after, "interval": rule_interval, "rate": rule_rate}


def run_rules(rows, rules):
    fails = []
    results = []
    for i, rule in enumerate(rules):
        name = rule.get("name") or "rule%d" % i
        rule = dict(rule)
        rule["name"] = name
        kind = rule.get("type")
        if kind not in RULE_KINDS:
            raise ValueError("规则 %s 的 type=%r 非法（支持 %s）"
                             % (name, kind, ",".join(sorted(RULE_KINDS))))
        f, facts = RULE_KINDS[kind](rows, rule)
        fails.extend(f)
        results.append({"name": name, "type": kind, "pass": not f,
                        "failures": f, "facts": facts})
    return fails, results


# ---------------------------------------------------------------------------
# 规则预设（C10#3）：把 1.2s / 30s / 600ms / 3min 的服务端节拍写成可复用规则。
# 预设只声明「规则 + 观测物来源」；capture 里没有对应事件时必须判 UNCLOSED（rc=3）。
# ---------------------------------------------------------------------------

PRESETS = {
    "cadence-1.2s": {
        "source": "server-log",
        "anchor": "event=search_view_range（玩家 SearchViewRange；SearchTime=1000ms，200ms tick ⇒ ≈1200ms）",
        "requires_vars": ["actor"],
        "rules": [
            {"name": "player-searchviewrange-1200ms", "type": "interval",
             "match": {"event": "search_view_range", "actor": "{{actor}}"},
             "min_ms": 1000, "max_ms": 1400, "min_pairs": 2},
        ],
    },
    "verify-maptime-30s": {
        "source": "server-log",
        "anchor": "event=verify_map_time（BaseObject.Run 的 `(now-VerifyTick) > 30s` 门）",
        "requires_vars": ["actor"],
        "rules": [
            {"name": "verifymaptime-30s-gate", "type": "interval",
             "match": {"event": "verify_map_time", "actor": "{{actor}}"},
             "min_ms": 30000, "max_ms": 30400, "min_pairs": 1},
        ],
    },
    "delaymagic-600ms": {
        "source": "server-log",
        "anchor": "event=delay_magic_schedule → delay_magic_fire（RM_DELAYMAGIC 600ms，200ms tick）",
        "requires_vars": ["actor"],
        "rules": [
            {"name": "rm-delaymagic-600ms", "type": "after",
             "from": {"event": "delay_magic_schedule", "actor": "{{actor}}"},
             "to": {"event": "delay_magic_fire", "actor": "{{actor}}"},
             "min_ms": 600, "max_ms": 800, "allowed_missing": 0},
        ],
    },
    "corpse-3min": {
        "source": "server-log",
        "anchor": "event=death → make_ghost（Config.MakeGhostTime 默认 180000ms，严格 >）",
        "requires_vars": ["actor"],
        "rules": [
            {"name": "makeghost-180s-gate", "type": "after",
             "from": {"event": "death", "actor": "{{actor}}"},
             "to": {"event": "make_ghost", "actor": "{{actor}}"},
             "min_ms": 180001, "max_ms": 180200, "allowed_missing": 0},
        ],
    },
}


def _substitute_vars(value, variables):
    """把过滤器里的 `{{key}}` 换掉；缺变量 ⇒ ValueError（fail-closed）。"""
    if isinstance(value, str) and value.startswith("{{") and value.endswith("}}"):
        key = value[2:-2]
        if key not in variables:
            raise ValueError("预设缺变量 %s（用 --preset-var %s=<值> 提供）" % (key, key))
        return variables[key]
    return value


def expand_preset_rule(rule, variables):
    out = json.loads(json.dumps(rule))
    for key in ("match", "from", "to"):
        flt = out.get(key)
        if isinstance(flt, dict):
            out[key] = {k: _substitute_vars(v, variables) for k, v in flt.items()}
    return out


def preset_anchor_count(rows, rule):
    flt = rule.get("from") if rule.get("type") == "after" else rule.get("match")
    return len(select(rows, flt))


def run_preset(rows, name, variables):
    """返回 dict：status ∈ GREEN/RED/UNCLOSED/PREREQ；failures/results/facts。"""
    if name not in PRESETS:
        raise ValueError("未知预设 %r（可用：%s）" % (name, ",".join(sorted(PRESETS))))
    spec = PRESETS[name]
    missing = [v for v in spec["requires_vars"] if v not in variables]
    if missing:
        return {"name": name, "status": "PREREQ", "source": spec["source"],
                "anchor": spec["anchor"], "failures": [
                    "预设 %s 缺变量 %s（--preset-var %s=<值>）"
                    % (name, ",".join(missing), ",".join(missing))],
                "results": [], "facts": {}}
    rules = [expand_preset_rule(r, variables) for r in spec["rules"]]
    counts = [preset_anchor_count(rows, r) for r in rules]
    if all(c == 0 for c in counts):
        return {"name": name, "status": "UNCLOSED", "source": spec["source"],
                "anchor": spec["anchor"], "failures": [
                    "UNCLOSED: 预设 %s 的观测帧 0 条（%s）—— capture 里没有服务端事件流，"
                    "不是通过；需要服务端日志/事件导出后重判" % (name, spec["anchor"])],
                "results": [], "facts": {"anchor_counts": counts}}
    fails, results = run_rules(rows, rules)
    return {"name": name, "status": ("RED" if fails else "GREEN"), "source": spec["source"],
            "anchor": spec["anchor"], "failures": fails, "results": results,
            "facts": {"anchor_counts": counts}}


# ---------------------------------------------------------------------------
# selftest：合成帧
# ---------------------------------------------------------------------------

def _frames(specs, direction="s2c", conn=1):
    """specs = [(seq, ts, ident), ...]"""
    return [{"kind": "frame", "dir": direction, "seq": s, "ts_ms": t, "ident": ident,
             "conn": conn, "port": 7200, "hop": "game", "recog": 0,
             "param": 0, "tag": 0, "series": 0, "body_len": 0, "raw_hex": "00"}
            for s, t, ident in specs]


def selftest():
    ok = True

    def expect(name, got, want):
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-62s %s (rc=%s, want %s)" % (name, "OK" if good else "FAIL", got, want))

    def rc(fails):
        return 1 if fails else 0

    rows = _frames([(1, 1000, 14), (2, 1200, 31)])
    rf = [{"name": "lead", "type": "after", "from": {"ident": 14},
           "to": {"ident": 31}, "min_ms": 0, "max_ms": 250}]
    expect("after：200ms 前导 => GREEN", rc(run_rules(rows, rf)[0]), 0)
    rf_bad = [dict(rf[0], max_ms=150)]
    expect("after：200ms > 上限 150 => RED", rc(run_rules(rows, rf_bad)[0]), 1)
    rf_min = [dict(rf[0], min_ms=250)]
    expect("after：200ms < 下限 250 => RED", rc(run_rules(rows, rf_min)[0]), 1)
    expect("after：from 帧 0 条 => RED（fail-closed）",
           rc(run_rules(_frames([(1, 1000, 31)]), rf)[0]), 1)
    rf_loose = [dict(rf[0], max_ms=250, allowed_missing=1, min_matched=1)]
    expect("after：唯一 from 未匹配、允许缺 1 => RED（min_matched 防空绿）",
           rc(run_rules(_frames([(9, 9000, 14)]), rf_loose)[0]), 1)

    cad = _frames([(1, 0, 53), (2, 1200, 53), (3, 2400, 53)])
    ri = [{"name": "cad", "type": "interval", "match": {"ident": 53},
           "min_ms": 1000, "max_ms": 1400}]
    expect("interval：1200ms 节拍 => GREEN", rc(run_rules(cad, ri)[0]), 0)
    cad_bad = _frames([(1, 0, 53), (2, 1200, 53), (3, 1900, 53)])
    expect("interval：出现 700ms => RED", rc(run_rules(cad_bad, ri)[0]), 1)
    expect("interval：帧数不足 min_pairs => RED",
           rc(run_rules(_frames([(1, 0, 53)]), ri)[0]), 1)
    skip = [{"name": "cad-skip", "type": "interval", "match": {"ident": 53},
             "min_ms": 1000, "max_ms": 1400, "skip": 1, "min_pairs": 1}]
    expect("interval：skip=1 跳首间隔 => GREEN",
           rc(run_rules(_frames([(1, 0, 53), (2, 100, 53), (3, 1300, 53)]), skip)[0]), 0)

    rate = _frames([(i, i * 100, 100) for i in range(1, 6)])
    rr = [{"name": "rate", "type": "rate", "match": {"ident": 100},
           "window_ms": 1000, "max_count": 4}]
    expect("rate：1s 内 5 帧 > 上限 4 => RED", rc(run_rules(rate, rr)[0]), 1)
    rr_ok = [dict(rr[0], max_count=5)]
    expect("rate：1s 内 5 帧 = 上限 5 => GREEN", rc(run_rules(rate, rr_ok)[0]), 0)
    expect("rate：match 0 帧 => RED（fail-closed）",
           rc(run_rules(_frames([(1, 0, 99)]), rr)[0]), 1)

    expect("未知 type => ValueError",
           1 if _raises(lambda: run_rules(rows, [{"name": "x", "type": "bogus"}])) else 0, 1)
    expect("缺 max_ms => ValueError",
           1 if _raises(lambda: run_rules(rows, [{"name": "x", "type": "after",
                                                   "from": {}, "to": {}}])) else 0, 1)
    expect("帧缺 ts_ms => ValueError",
           1 if _raises(lambda: run_rules([{"ident": 14, "seq": 1}], rf)) else 0, 1)

    # ---- 预设（C10#3）：每条都要证明「会绿、会红、没观测物时 UNCLOSED」----
    def ev(specs, actor=1):
        return [{"kind": "server-event", "seq": i, "ts_ms": t, "event": name, "actor": actor}
                for i, (t, name) in enumerate(specs)]

    def pstatus(rows, name, variables):
        return run_preset(rows, name, variables).get("status")

    cad_ok = ev([(0, "search_view_range"), (1200, "search_view_range"),
                 (2400, "search_view_range")])
    expect("preset cadence-1.2s：1200ms ⇒ GREEN",
           pstatus(cad_ok, "cadence-1.2s", {"actor": 1}), "GREEN")
    cad_bad = ev([(0, "search_view_range"), (700, "search_view_range"),
                  (1400, "search_view_range")])
    expect("preset cadence-1.2s：700ms ⇒ RED",
           pstatus(cad_bad, "cadence-1.2s", {"actor": 1}), "RED")
    mixed = ev([(0, "search_view_range"), (1200, "search_view_range"),
                (2400, "search_view_range")], actor=1) + \
            ev([(0, "search_view_range"), (300, "search_view_range"),
                (600, "search_view_range")], actor=2)
    expect("preset cadence-1.2s：actor 过滤只判本人（混入 actor=2 仍 GREEN）",
           pstatus(mixed, "cadence-1.2s", {"actor": 1}), "GREEN")
    expect("preset cadence-1.2s：actor 过滤有牙（按 actor=2 判 ⇒ RED）",
           pstatus(mixed, "cadence-1.2s", {"actor": 2}), "RED")
    expect("preset 缺 actor 变量 ⇒ PREREQ（绝不默认判）",
           pstatus(cad_ok, "cadence-1.2s", {}), "PREREQ")
    expect("preset 在普通 s2c capture（无事件帧）⇒ UNCLOSED 而不是 GREEN",
           pstatus(_frames([(1, 0, 53), (2, 1200, 53)]), "cadence-1.2s", {"actor": 1}),
           "UNCLOSED")
    expect("preset 在完全空 capture ⇒ UNCLOSED",
           pstatus([], "cadence-1.2s", {"actor": 1}), "UNCLOSED")

    ver_ok = ev([(0, "verify_map_time"), (30100, "verify_map_time")])
    expect("preset verify-maptime-30s：30100ms ⇒ GREEN",
           pstatus(ver_ok, "verify-maptime-30s", {"actor": 1}), "GREEN")
    ver_bad = ev([(0, "verify_map_time"), (20000, "verify_map_time")])
    expect("preset verify-maptime-30s：20000ms ⇒ RED",
           pstatus(ver_bad, "verify-maptime-30s", {"actor": 1}), "RED")

    dm_ok = ev([(0, "delay_magic_schedule"), (650, "delay_magic_fire")])
    expect("preset delaymagic-600ms：650ms ⇒ GREEN",
           pstatus(dm_ok, "delaymagic-600ms", {"actor": 1}), "GREEN")
    dm_bad = ev([(0, "delay_magic_schedule"), (400, "delay_magic_fire")])
    expect("preset delaymagic-600ms：400ms ⇒ RED",
           pstatus(dm_bad, "delaymagic-600ms", {"actor": 1}), "RED")

    cg_ok = ev([(0, "death"), (180100, "make_ghost")])
    expect("preset corpse-3min：180100ms ⇒ GREEN",
           pstatus(cg_ok, "corpse-3min", {"actor": 1}), "GREEN")
    cg_bad = ev([(0, "death"), (100000, "make_ghost")])
    expect("preset corpse-3min：100000ms ⇒ RED",
           pstatus(cg_bad, "corpse-3min", {"actor": 1}), "RED")
    expect("预设目录：四条且名字固定",
           sorted(PRESETS.keys()),
           sorted(["cadence-1.2s", "verify-maptime-30s", "delaymagic-600ms", "corpse-3min"]))
    expect("预设变量替换：非占位值原样保留",
           expand_preset_rule({"match": {"event": "x", "actor": "{{actor}}"}}, {"actor": 7})["match"],
           {"event": "x", "actor": 7})

    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def _raises(fn):
    try:
        fn()
    except ValueError:
        return True
    return False


EXAMPLE_CONFIG = {
    "rules": [
        {"name": "sm_hit-leads-sm_struck-250ms", "type": "after",
         "from": {"dir": "s2c", "ident": 14}, "to": {"dir": "s2c", "ident": 31},
         "min_ms": 0, "max_ms": 250, "allowed_missing": 0},
        {"name": "login-1200ms-cadence", "type": "interval",
         "match": {"dir": "s2c", "ident": 53}, "min_ms": 1000, "max_ms": 1400,
         "min_pairs": 1},
        {"name": "gate-30s", "type": "after",
         "from": {"dir": "c2s", "ident": 3014}, "to": {"dir": "s2c", "ident": 31},
         "min_ms": 0, "max_ms": 30000, "allowed_missing": 0},
        {"name": "gate-60s", "type": "after",
         "from": {"dir": "c2s", "ident": 3014}, "to": {"dir": "s2c", "ident": 34},
         "min_ms": 0, "max_ms": 60000, "allowed_missing": 0},
        {"name": "ritual-600-1200ms", "type": "interval",
         "match": {"dir": "s2c", "ident": 53}, "min_ms": 600, "max_ms": 1200,
         "min_pairs": 1},
        {"name": "corpse-3min", "type": "after",
         "from": {"dir": "s2c", "ident": 34}, "to": {"dir": "s2c", "ident": 35},
         "min_ms": 0, "max_ms": 180000, "allowed_missing": 0},
        {"name": "corpse-5min", "type": "after",
         "from": {"dir": "s2c", "ident": 34}, "to": {"dir": "s2c", "ident": 36},
         "min_ms": 0, "max_ms": 300000, "allowed_missing": 0},
        {"name": "send-rate-50-per-s", "type": "rate",
         "match": {"dir": "s2c"}, "window_ms": 1000, "max_count": 50},
    ]
}


def main(argv=None):
    ap = argparse.ArgumentParser(description="capture.jsonl 时间戳判据（after/interval/rate）")
    ap.add_argument("--capture", default="", help="capture.jsonl")
    ap.add_argument("--config", default="", help="规则 JSON（{\"rules\":[...]}）")
    ap.add_argument("--preset", action="append", default=[],
                    help="内置预设名（可重复；all=四条全跑）：%s" % ",".join(sorted(PRESETS)))
    ap.add_argument("--preset-var", action="append", default=[], metavar="KEY=VALUE",
                    help="预设变量（如 actor=275）；数字自动转 int")
    ap.add_argument("--list-presets", action="store_true", help="列出内置预设后退出")
    ap.add_argument("--json", action="store_true", help="JSON 输出")
    ap.add_argument("--example-config", action="store_true", help="打印规则模板后退出")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.list_presets:
        print(json.dumps({k: {"source": v["source"], "anchor": v["anchor"],
                              "requires_vars": v["requires_vars"]}
                          for k, v in PRESETS.items()}, ensure_ascii=False, indent=2))
        return 0
    if args.example_config:
        print(json.dumps(EXAMPLE_CONFIG, ensure_ascii=False, indent=2))
        return 0

    if args.preset and args.config:
        print("PREREQ_MISSING: --config 与 --preset 不能同时给（两套口径不混用）")
        print("RESULT: PREREQ_RED")
        return 2
    if not args.capture or (not args.config and not args.preset):
        print("PREREQ_MISSING: 必须给 --capture 与 --config（或 --preset/--selftest）")
        print("RESULT: PREREQ_RED")
        return 2

    if args.preset:
        names = []
        for item in args.preset:
            for name in item.split(","):
                name = name.strip()
                if name == "all":
                    names.extend(sorted(PRESETS.keys()))
                elif name:
                    names.append(name)
        variables = {}
        try:
            for item in args.preset_var:
                if "=" not in item:
                    raise ValueError("--preset-var 必须是 KEY=VALUE：%r" % item)
                key, value = item.split("=", 1)
                variables[key] = int(value) if value.lstrip("-").isdigit() else value
            rows = load_capture(args.capture)
            reports = [run_preset(rows, name, variables) for name in names]
        except FileNotFoundError as exc:
            print("PREREQ_MISSING: 文件不存在 %s" % exc)
            print("RESULT: PREREQ_RED")
            return 2
        except (ValueError, json.JSONDecodeError) as exc:
            print("PREREQ_MISSING: 配置/capture 不可解析：%s" % exc)
            print("RESULT: PREREQ_RED")
            return 2
        statuses = [r["status"] for r in reports]
        if "RED" in statuses:
            overall = "RED"
        elif "PREREQ" in statuses:
            overall = "PREREQ_RED"
        elif "UNCLOSED" in statuses:
            overall = "UNCLOSED"
        else:
            overall = "GREEN"
        if args.json:
            print(json.dumps({"capture": args.capture, "presets": reports,
                              "result": overall}, ensure_ascii=False))
        else:
            print("capture=%s presets=%d" % (args.capture, len(reports)))
            for r in reports:
                print("[%s] %-24s source=%s" % (r["status"], r["name"], r["source"]))
                for f in r["failures"]:
                    print("  %s" % f)
            print("RESULT: %s" % overall)
        if overall == "GREEN":
            return 0
        if overall == "RED":
            return 1
        if overall == "PREREQ_RED":
            return 2
        return 3
    try:
        rows = load_capture(args.capture)
        with open(args.config, encoding="utf-8") as fh:
            conf = json.load(fh)
        rules = conf.get("rules") if isinstance(conf, dict) else conf
        if not isinstance(rules, list) or not rules:
            raise ValueError("config 必须是 {\"rules\":[...]} 或非空规则数组")
        fails, results = run_rules(rows, rules)
    except FileNotFoundError as exc:
        print("PREREQ_MISSING: 文件不存在 %s" % exc)
        print("RESULT: PREREQ_RED")
        return 2
    except (ValueError, json.JSONDecodeError) as exc:
        print("PREREQ_MISSING: 配置/capture 不可解析：%s" % exc)
        print("RESULT: PREREQ_RED")
        return 2

    if args.json:
        print(json.dumps({"capture": args.capture, "config": args.config,
                          "failures": fails, "results": results}, ensure_ascii=False))
    else:
        print("capture=%s config=%s rules=%d" % (args.capture, args.config, len(results)))
        for r in results:
            print("[%s] %-32s %s" % ("PASS" if r["pass"] else "FAIL", r["name"], r["facts"]))
            for f in r["failures"]:
                print("  RED: %s" % f)
    print("RESULT: %s" % ("RED" if fails else "GREEN"))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
