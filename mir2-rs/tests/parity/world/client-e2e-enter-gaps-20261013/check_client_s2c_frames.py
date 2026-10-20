#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_client_s2c_frames.py —— 卡 `openmir2-client-e2e-enter-gaps` 缺口①/② 的**客户端侧帧可见性**判据。

输入 = `tools/m2-acceptance/export_s2c.ps1` 产出的 `capture.jsonl`（真机 mir2_proxy 在 7200 抓包 +
segment_frames + GoldenExport 解码；s2c 方向 = 客户端**实际收到**的字节）。

判据（全部机判）：
  ① 入图自帧序列存在且**前缀**为 `658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46`
     （`--upto-ident 46` 的 15 条，与 `s2c_diff.py` 同口径）。默认行为不变；
     `--expect-enter-seq` 可换成别的序列（评审 B9）：逗号列表，元素可写 `211?` 表示可选；
     预设：`default`（15 条）、`equipped`（有武器/技能：`…52,752,621,211?,811,46`，211 可选）、
     `delmagic`（含可选 `212` 与 `621/211`，最多 18 条）、`g2`（含可选 `210/212`，最宽松）。
     **注意**：有 621/211 时真实抓包（s2c-621c）是 17 条；只有再带 212/210 时才是评审写的 18 条。
  ② `52 SM_ABILITY` 与 `752 SM_SUBABILITY` **都出现**（缺口①「未下发」的直接否证）；
     `--expect-52-body-len N` 时还要求 52 的 `body_len == N`（40=oracle 布局 / 58=客户端 TAbility）；
  ③ `--min-appearance-pairs N` 时要求至少 N 个 `10 SM_TURN + 41 SM_FEATURECHANGED` 相邻对
     （缺口②「同屏无怪」的直接否证；对同屏出现的每个 actor 各一对）。

退出码：0=GREEN / 1=RED / 2=PREREQ_MISSING（文件缺/不可解析；绝不静默通过）。
`--selftest` 不依赖外部文件：造合成 capture 自证「有 52/752 ⇒ GREEN、删掉 52 ⇒ RED、缺序列前缀 ⇒ RED」。
"""
import argparse
import json
import os
import sys

ENTER_SEQ = [658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46]

# 评审 B9：可变入图序。`211?` = 可选帧（argparse 值里写 `211?` 或 `[211]`）。
ENTER_SEQ_PRESETS = {
    "default": list(ENTER_SEQ),
    "equipped": [658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106,
                 52, 752, 621, "211?", 811, 46],
    "delmagic": [658, "212?", 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106,
                 52, 752, "621?", "211?", 811, 46],
    "g2": [658, "210?", "212?", 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106,
           52, 752, "621?", "211?", 811, 46],
}


def parse_enter_seq(spec):
    """逗号列表 -> [(ident, optional)]；也接受 ENTER_SEQ_PRESETS 的键。"""
    if not spec:
        spec = "default"
    key = spec.strip()
    seq = ENTER_SEQ_PRESETS.get(key.lower())
    if seq is None:
        seq = [x.strip() for x in key.split(",") if x.strip()]
    out = []
    for tok in seq:
        if isinstance(tok, str):
            tok = tok.strip()
            optional = tok.endswith("?") or (tok.startswith("[") and tok.endswith("]"))
            if tok.startswith("[") and tok.endswith("]"):
                tok = tok[1:-1]
            elif tok.endswith("?"):
                tok = tok[:-1]
            out.append((int(tok, 0), optional))
        else:
            out.append((int(tok), False))
    if not out:
        raise ValueError("--expect-enter-seq 为空")
    return out


def seq_matches(got, expected):
    """got 的 ident 列表是否与 expected（含可选帧）完全匹配。"""
    i = 0
    for val, optional in expected:
        if i < len(got) and got[i] == val:
            i += 1
        elif optional:
            continue
        else:
            return False, i
    return i == len(got), i


def load_capture(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def s2c(rows):
    return [r for r in rows if r.get("dir") == "s2c"]


def enter_prefix(rows):
    """从首个 658 到首个 46（含）的带 ident 帧。"""
    out = []
    started = False
    for r in rows:
        if not started:
            if r.get("ident") == 658:
                started = True
                out.append(r)
            continue
        if "ident" not in r:
            continue
        out.append(r)
        if r["ident"] == 46:
            break
    return out


def appearance_pairs(rows):
    out = []
    i = 0
    while i < len(rows):
        if rows[i].get("ident") == 10 and i + 1 < len(rows) and rows[i + 1].get("ident") == 41:
            out.append((rows[i]["recog"] & 0xFFFFFFFF, rows[i + 1]["recog"] & 0xFFFFFFFF))
            i += 2
        else:
            i += 1
    return out


def check(rows, expect_52_body_len=None, min_appearance_pairs=0, expect_enter_seq=None):
    """返回 (failures:list[str], facts:dict)。

    `expect_enter_seq`：None/空 = 现有 15 帧 `ENTER_SEQ`；字符串预设名或逗号列表；
    列表元素 `211?`/`[211]` 表示可选帧。
    """
    fails = []
    s = s2c(rows)
    ent = enter_prefix(s)
    got = [r["ident"] for r in ent]
    expected = parse_enter_seq(expect_enter_seq or "")
    ok, _ = seq_matches(got, expected)
    if not ok:
        want = [str(v) + ("?" if opt else "") for v, opt in expected]
        fails.append("入图自帧序列不符：got=%s expect=%s" % (got, want))
    f52 = next((r for r in ent if r.get("ident") == 52), None)
    f752 = next((r for r in ent if r.get("ident") == 752), None)
    if f52 is None:
        fails.append("入图帧里没有 52 SM_ABILITY")
    if f752 is None:
        fails.append("入图帧里没有 752 SM_SUBABILITY")
    if f52 is not None and expect_52_body_len is not None and f52.get("body_len") != expect_52_body_len:
        fails.append("52 body_len=%s 期望 %s" % (f52.get("body_len"), expect_52_body_len))
    pairs = appearance_pairs(s)
    if len(pairs) < min_appearance_pairs:
        fails.append("10+41 同屏出现对只有 %d 个 < 要求 %d 个" % (len(pairs), min_appearance_pairs))
    facts = {
        "s2c_frames": len(s),
        "enter_seq": got,
        "expect_enter_seq": [str(v) + ("?" if opt else "") for v, opt in expected],
        "52_present": f52 is not None,
        "52_body_len": f52.get("body_len") if f52 else None,
        "752_present": f752 is not None,
        "appearance_pairs": len(pairs),
        "appearance_recogs": ["0x%08x" % a for a, _ in pairs],
    }
    return fails, facts


def synthetic(with_52=True, ordered=True):
    rows = [{"kind": "frame", "dir": "s2c", "seq": i, "ident": ident,
             "recog": 0, "tag": 0, "series": 0, "body_len": 0, "raw_hex": "00"}
            for i, ident in enumerate(ENTER_SEQ[:-1] if not with_52 else ENTER_SEQ)]
    if not with_52:
        rows = [r for r in rows if r["ident"] != 52]
    if not ordered:
        rows = rows[::-1]
    return rows


def synthetic_equipped(with_211=True, with_212=False):
    """有武器/技能：基线 15 帧 + 可选 212（658 后）+ 621 + 可选 211（752 后）。"""
    seq = list(ENTER_SEQ)
    out = []
    for ident in seq:
        if ident == 51 and with_212:
            out.append(212)
        if ident == 811:
            out.append(621)
            if with_211:
                out.append(211)
        out.append(ident)
    if not with_211:
        out = [x for x in out if x != 211]
    return [{"kind": "frame", "dir": "s2c", "seq": i, "ident": ident,
             "recog": 0, "tag": 0, "series": 0, "body_len": 0, "raw_hex": "00"}
            for i, ident in enumerate(out)]


def selftest():
    ok = True

    def expect(name, got, want):
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-46s %s (rc=%d, want %d)" % (name, "OK" if good else "FAIL", got, want))

    def rc_of(fails):
        return 1 if fails else 0

    expect("合成 capture 含 52/752 ⇒ GREEN", rc_of(check(synthetic(True))[0]), 0)
    expect("合成 capture 去掉 52 ⇒ RED", rc_of(check(synthetic(False))[0]), 1)
    expect("合成 capture 序列乱序 ⇒ RED", rc_of(check(synthetic(True, ordered=False))[0]), 1)
    expect("52 body_len 门：40 期望 58 ⇒ RED",
           rc_of(check(synthetic(True), expect_52_body_len=58)[0]), 1)
    expect("同屏对门：0 对 期望 1 ⇒ RED",
           rc_of(check(synthetic(True), min_appearance_pairs=1)[0]), 1)
    # —— 评审 B9：--expect-enter-seq ——
    expect("equipped @@ 有 621/211、默认 15 帧 ⇒ RED",
           rc_of(check(synthetic_equipped(True))[0]), 1)
    expect("equipped 预设（621+211，17 条）⇒ GREEN",
           rc_of(check(synthetic_equipped(True), expect_enter_seq="equipped")[0]), 0)
    expect("equipped 预设（无 211，211? 可选）⇒ GREEN",
           rc_of(check(synthetic_equipped(False), expect_enter_seq="equipped")[0]), 0)
    expect("g2 预设（含可选 210/212）无 210/212 ⇒ GREEN",
           rc_of(check(synthetic_equipped(True), expect_enter_seq="g2")[0]), 0)
    expect("delmagic 预设（含 212+621+211，18 条）⇒ GREEN",
           rc_of(check(synthetic_equipped(True, with_212=True),
                       expect_enter_seq="delmagic")[0]), 0)
    expect("默认序列配 equipped 抓包 ⇒ RED（不能静默通过）",
           rc_of(check(synthetic_equipped(True), expect_enter_seq="default")[0]), 1)
    expect("自定义逗号序列（显式 211）⇒ GREEN",
           rc_of(check(synthetic_equipped(True),
                       expect_enter_seq="658,51,50,41,213,11029,42,766,54,5008,1106,"
                                        "52,752,621,211,811,46")[0]), 0)
    expect("自定义序列 == 15 帧默认 ⇒ GREEN",
           rc_of(check(synthetic(True), expect_enter_seq="default")[0]), 0)
    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", default="")
    ap.add_argument("--expect-52-body-len", type=int, default=None)
    ap.add_argument("--min-appearance-pairs", type=int, default=0)
    ap.add_argument("--expect-enter-seq", default="",
                    help="入图序：逗号列表（元素可写 211? 表可选）或预设 "
                         "default/equipped/delmagic/g2；默认仍是现有 15 帧")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    if not a.capture:
        print("PREREQ_MISSING: 必须给 --capture <capture.jsonl>（或 --selftest）")
        return 2
    if not os.path.isfile(a.capture):
        print("PREREQ_MISSING: %s 不存在" % a.capture)
        return 2
    try:
        rows = load_capture(a.capture)
    except Exception as e:  # noqa: BLE001
        print("PREREQ_MISSING: %s 解析失败：%s" % (a.capture, e))
        return 2
    try:
        fails, facts = check(rows, a.expect_52_body_len, a.min_appearance_pairs,
                             a.expect_enter_seq)
    except ValueError as e:
        print("PREREQ_MISSING: --expect-enter-seq 非法：%s" % e)
        return 2
    if a.json:
        print(json.dumps({"capture": a.capture, "failures": fails, "facts": facts}, ensure_ascii=False))
    else:
        print("capture=%s" % a.capture)
        print("  s2c_frames=%s" % facts["s2c_frames"])
        print("  enter_seq=%s" % ",".join(str(x) for x in facts["enter_seq"]))
        print("  expect_enter_seq=%s" % ",".join(facts["expect_enter_seq"]))
        print("  52_present=%s body_len=%s" % (facts["52_present"], facts["52_body_len"]))
        print("  752_present=%s" % facts["752_present"])
        print("  appearance_pairs=%s recogs=%s" % (facts["appearance_pairs"], facts["appearance_recogs"]))
        for f in fails:
            print("  RED: %s" % f)
    print("RESULT: %s" % ("RED" if fails else "GREEN"))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
