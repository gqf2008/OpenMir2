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

④ `--expect-52-weight N` / `--expect-52-max-exp N`：**52 body 字段级解码**（C10#1 / B2）。
   按 `body_len` 自动识别布局：40 = oracle `EncodeMessage(WAbil)`（C# `Ability.cs` 声明序），
   58 = 冻结客户端 `TAbility`（`HIGHHP=True`，`record.rs::encode_ability_client`）。
   `Weight@32 / MaxExp@28`（40B）与 `Weight@46 / MaxExp@42`（58B）逐字节解出；
   `--expect-52-layout oracle40|client58` 可把布局也钉死。入图前缀里**每个** 52 都必须匹配。
⑤ `--expect-enter-seq auto`：按捕获里实际出现的可选帧自动选预设（C10#10）——
   见 210 ⇒ `g2`；否则见 212 ⇒ `delmagic`；否则见 621/211 ⇒ `equipped`；否则 `default`。
   自动只选预设，**不放宽**任何帧（预设本身的顺序/必选帧仍逐条判；选错或位置错仍 RED）。

退出码：0=GREEN / 1=RED / 2=PREREQ_MISSING（文件缺/不可解析；绝不静默通过）。
`--selftest` 不依赖外部文件：造合成 capture 自证「有 52/752 ⇒ GREEN、删掉 52 ⇒ RED、缺序列前缀 ⇒ RED」，
并覆盖 40B/58B 两套 52 字段解码、auto 预设选择与误选必红。
"""
import argparse
import json
import os
import struct
import sys

ENTER_SEQ = [658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46]

# ---- 帧体解码（edcode.rs / EncryptUtil.cs 的逐字节移植；本文件内联，供干净检出自足复算）----
SEED = 0xAC
BASE = 0x3C
HEAD_BLOCK = 16          # encode(12B 命令头) 恒为 16 字符


def decode(src):
    src = bytes(src)
    n = len(src)
    out = bytearray()
    for i in range(n // 4):
        b = i * 4
        r = (src[b + 3] - BASE) & 0xFF
        t = (src[b] - BASE) & 0xFF
        out.append(((((t << 2) & 0xF0) | (r & 0x0C) | (t & 0x03)) ^ SEED) & 0xFF)
        t = (src[b + 1] - BASE) & 0xFF
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)) ^ SEED) & 0xFF)
        t = (src[b + 2] - BASE) & 0xFF
        out.append((((t | ((r << 2) & 0xC0)) ^ SEED) & 0xFF))
    left = n % 4
    if left == 2:
        r = (src[n - 1] - BASE) & 0xFF
        t = (src[n - 2] - BASE) & 0xFF
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)) ^ SEED) & 0xFF)
    elif left == 3:
        r = (src[n - 1] - BASE) & 0xFF
        t = (src[n - 3] - BASE) & 0xFF
        out.append(((((t << 2) & 0xF0) | (r & 0x0C) | (t & 0x03)) ^ SEED) & 0xFF)
        t = (src[n - 2] - BASE) & 0xFF
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)) ^ SEED) & 0xFF)
    return bytes(out)


def encode(src):
    src = bytes(src)
    out = bytearray()
    no = 2
    remainder = 0
    for b in src:
        c = b ^ SEED
        if no == 6:
            out.append(((c & 0x3F) + BASE) & 0xFF)
            remainder |= (c >> 2) & 0x30
            out.append((remainder + BASE) & 0xFF)
            remainder = 0
        else:
            temp = c >> 2
            out.append((((temp & 0x3C) | (c & 0x03)) + BASE) & 0xFF)
            remainder = ((remainder << 2) | (temp & 0x03)) & 0xFF
        no = no % 6 + 2
    if no != 2:
        out.append((remainder + BASE) & 0xFF)
    return bytes(out)


def command_head(ident, recog=0, param=0, tag=0, series=0):
    return struct.pack("<iHHHH", recog, ident, param, tag, series)


# 52 body 的两套布局（字段偏移；见 Ability.cs:13-46 与 record.rs::encode_ability_client 文档）。
#   oracle40 = C# `Ability` 声明序（SerializeLayout.Sequential, Size=40）：ExpCount/ExpMaxCount
#              在 Reserved2 之后、Exp 之前；Weight/MaxExp 是 16 位字段。
#   client58 = 冻结客户端 `TAbility`（HIGHHP=True）：AC..SC 与 HP..MaxMP、Exp/MaxExp 均为 32 位，
#              Weight 及以下 6 个负重字段均为 Word。
ABILITY_LAYOUTS = {
    40: {
        "name": "oracle40",
        "level": ("<B", 0), "exp": ("<i", 24), "max_exp": ("<i", 28),
        "weight": ("<H", 32), "max_weight": ("<H", 34),
        "wear_weight": ("<B", 36), "max_wear_weight": ("<B", 37),
        "hand_weight": ("<B", 38), "max_hand_weight": ("<B", 39),
        "hp": ("<H", 12), "max_hp": ("<H", 16),
    },
    58: {
        "name": "client58",
        "level": ("<H", 0), "exp": ("<i", 38), "max_exp": ("<i", 42),
        "weight": ("<H", 46), "max_weight": ("<H", 48),
        "wear_weight": ("<H", 50), "max_wear_weight": ("<H", 52),
        "hand_weight": ("<H", 54), "max_hand_weight": ("<H", 56),
        "hp": ("<i", 22), "max_hp": ("<i", 30),
    },
}
OWNS_ABILITY_FIELDS = ("level", "exp", "max_exp", "weight", "max_weight", "wear_weight",
                       "max_wear_weight", "hand_weight", "max_hand_weight", "hp", "max_hp")

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


class BadAbilityFrame(Exception):
    pass


def decode_ability_body(body):
    """按 body_len 自动识别 40B/58B 布局并解出字段（未知长度即抛 BadAbilityFrame）。"""
    layout = ABILITY_LAYOUTS.get(len(body))
    if layout is None:
        raise BadAbilityFrame("未知的 52 body 长度 %d（只登记 40=oracle40 / 58=client58）" % len(body))
    out = {"layout": layout["name"], "body_len": len(body)}
    for field in OWNS_ABILITY_FIELDS:
        fmt, off = layout[field]
        out[field] = struct.unpack_from(fmt, body, off)[0]
    return out


def decode_ability_frame(row):
    """从 capture 行解 52 body：`#` + encode(12B 头) + encode(body) + `!`。"""
    raw_hex = row.get("raw_hex") or ""
    try:
        raw = bytes.fromhex(raw_hex)
    except ValueError:
        raise BadAbilityFrame("raw_hex 不是合法十六进制")
    if len(raw) < HEAD_BLOCK + 2 or raw[0:1] != b"#" or not raw.endswith(b"!"):
        raise BadAbilityFrame("帧形状不符（需 # 开头 / ! 结尾 / 头 16 字符）")
    inner = raw[1:-1]
    if len(inner) <= HEAD_BLOCK:
        raise BadAbilityFrame("52 帧没有 body 段")
    body = decode(inner[HEAD_BLOCK:])
    want = row.get("body_len")
    if want is not None and want != len(body):
        raise BadAbilityFrame("解出 body_len=%d 与帧声明 body_len=%s 不符" % (len(body), want))
    return decode_ability_body(body)


OPTIONAL_ENTER_IDS = (210, 212, 621, 211)


def auto_preset(rows):
    """按捕获状态选最贴合（且仍严格）的预设：210⇒g2 / 212⇒delmagic / 621/211⇒equipped。"""
    idents = [r.get("ident") for r in rows if "ident" in r]
    if 210 in idents:
        return "g2"
    if 212 in idents:
        return "delmagic"
    if 621 in idents or 211 in idents:
        return "equipped"
    return "default"


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


def check(rows, expect_52_body_len=None, min_appearance_pairs=0, expect_enter_seq=None,
          expect_52_weight=None, expect_52_max_exp=None, expect_52_layout=None):
    """返回 (failures:list[str], facts:dict)。

    `expect_enter_seq`：None/空 = 现有 15 帧 `ENTER_SEQ`；字符串预设名/逗号列表/`auto`；
    列表元素 `211?`/`[211]` 表示可选帧。
    `expect_52_*`：52 body 字段级门（Weight/MaxExp/布局），对入图前缀里的**每个** 52 生效。
    """
    fails = []
    s = s2c(rows)
    ent = enter_prefix(s)
    got = [r["ident"] for r in ent]
    spec = (expect_enter_seq or "").strip()
    auto_used = None
    if spec.lower() == "auto":
        auto_used = auto_preset(ent)
        spec = auto_used
    expected = parse_enter_seq(spec)
    ok, _ = seq_matches(got, expected)
    if not ok:
        want = [str(v) + ("?" if opt else "") for v, opt in expected]
        fails.append("入图自帧序列不符：got=%s expect=%s" % (got, want))
    f52s = [r for r in ent if r.get("ident") == 52]
    f52 = f52s[0] if f52s else None
    f752 = next((r for r in ent if r.get("ident") == 752), None)
    if f52 is None:
        fails.append("入图帧里没有 52 SM_ABILITY")
    if f752 is None:
        fails.append("入图帧里没有 752 SM_SUBABILITY")
    if f52 is not None and expect_52_body_len is not None and f52.get("body_len") != expect_52_body_len:
        fails.append("52 body_len=%s 期望 %s" % (f52.get("body_len"), expect_52_body_len))
    # ---- 52 body 字段级解码（C10#1 / B2）：有期望时逐帧解，解不出即 RED ----
    want_fields = (expect_52_weight is not None or expect_52_max_exp is not None
                   or expect_52_layout is not None)
    decoded = []
    decode_errors = []
    for r in f52s:
        try:
            decoded.append(decode_ability_frame(r))
        except BadAbilityFrame as e:
            decode_errors.append(str(e))
    if want_fields:
        if not f52s:
            fails.append("52 字段门缺少 52 帧")
        for e in decode_errors:
            fails.append("52 body 解码失败：%s" % e)
        for d in decoded:
            if expect_52_weight is not None and d["weight"] != expect_52_weight:
                fails.append("52 Weight=%d 期望 %d（layout=%s）" %
                             (d["weight"], expect_52_weight, d["layout"]))
            if expect_52_max_exp is not None and d["max_exp"] != expect_52_max_exp:
                fails.append("52 MaxExp=%d 期望 %d（layout=%s）" %
                             (d["max_exp"], expect_52_max_exp, d["layout"]))
            if expect_52_layout is not None and d["layout"] != expect_52_layout:
                fails.append("52 layout=%s 期望 %s" % (d["layout"], expect_52_layout))
    pairs = appearance_pairs(s)
    if len(pairs) < min_appearance_pairs:
        fails.append("10+41 同屏出现对只有 %d 个 < 要求 %d 个" % (len(pairs), min_appearance_pairs))
    facts = {
        "s2c_frames": len(s),
        "enter_seq": got,
        "expect_enter_seq": [str(v) + ("?" if opt else "") for v, opt in expected],
        "auto_preset": auto_used,
        "52_present": f52 is not None,
        "52_body_len": f52.get("body_len") if f52 else None,
        "52_frames": len(f52s),
        "52_decoded": decoded[0] if decoded else None,
        "52_decode_errors": decode_errors,
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


def frame_row(ident, body=b"", recog=0, param=0, tag=0, seq=0):
    raw = b"#" + encode(command_head(ident, recog, param, tag)) + \
        (encode(body) if body else b"") + b"!"
    return {"kind": "frame", "dir": "s2c", "seq": seq, "ident": ident,
            "recog": recog, "param": param, "tag": tag, "series": 0,
            "body_len": len(body), "body_sha256": "", "raw_hex": raw.hex()}


def ability_body(layout, **over):
    """构造 40B(oracle)/58B(client) 的 52 body；默认值 = 部署新号（Weight=14, MaxExp=10）。"""
    v = dict(level=1, ac=0, mac=0, dc=257, mc=0, sc=0, hp=19, mp=15,
             max_hp=19, max_mp=15, exp=0, max_exp=10, weight=14,
             max_weight=50, wear_weight=0, max_wear_weight=15, hand_weight=0,
             max_hand_weight=12)
    v.update(over)
    if layout == 40:
        return struct.pack("<BBHHHHHHHHHhbbiiHHBBBB",
                           v["level"], 0,
                           v["ac"], v["mac"], v["dc"], v["mc"], v["sc"],
                           v["hp"], v["mp"], v["max_hp"], v["max_mp"],
                           0, 0, 0, v["exp"], v["max_exp"],
                           v["weight"], v["max_weight"],
                           v["wear_weight"], v["max_wear_weight"],
                           v["hand_weight"], v["max_hand_weight"])
    if layout == 58:
        return struct.pack("<H" + "i" * 11 + "H" * 6,
                           v["level"],
                           v["ac"], v["mac"], v["dc"], v["mc"], v["sc"],
                           v["hp"], v["mp"], v["max_hp"], v["max_mp"],
                           v["exp"], v["max_exp"],
                           v["weight"], v["max_weight"],
                           v["wear_weight"], v["max_wear_weight"],
                           v["hand_weight"], v["max_hand_weight"])
    raise ValueError("layout must be 40 or 58")


def synthetic_ability(layout=40, **vals):
    """基线 15 帧，但 52 换成真实可解的 body。"""
    out = []
    for r in synthetic(True):
        if r["ident"] == 52:
            out.append(frame_row(52, ability_body(layout, **vals), seq=r["seq"]))
        else:
            out.append(r)
    return out


def synthetic_g2(with_210=True, with_212=True, with_211=True):
    seq = [658]
    if with_210:
        seq.append(210)
    if with_212:
        seq.append(212)
    seq += [51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 621]
    if with_211:
        seq.append(211)
    seq += [811, 46]
    return [{"kind": "frame", "dir": "s2c", "seq": i, "ident": ident,
             "recog": 0, "tag": 0, "series": 0, "body_len": 0, "raw_hex": "00"}
            for i, ident in enumerate(seq)]


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
    # —— C10#1 / B2：52 body 字段级解码（40B oracle / 58B client）——
    expect("oracle40 Weight=14 ⇒ GREEN",
           rc_of(check(synthetic_ability(40), expect_52_weight=14)[0]), 0)
    expect("oracle40 MaxExp=10 ⇒ GREEN",
           rc_of(check(synthetic_ability(40), expect_52_max_exp=10)[0]), 0)
    expect("oracle40 解出 Weight=14，期望 10（构造级夹具）⇒ RED",
           rc_of(check(synthetic_ability(40), expect_52_weight=10)[0]), 1)
    expect("oracle40 MaxExp 期望 11 ⇒ RED",
           rc_of(check(synthetic_ability(40), expect_52_max_exp=11)[0]), 1)
    expect("client58 Weight=14 ⇒ GREEN",
           rc_of(check(synthetic_ability(58), expect_52_weight=14)[0]), 0)
    expect("client58 Weight=15 ⇒ RED",
           rc_of(check(synthetic_ability(58), expect_52_weight=15)[0]), 1)
    expect("oracle40 布局钉死 oracle40 ⇒ GREEN",
           rc_of(check(synthetic_ability(40), expect_52_layout="oracle40")[0]), 0)
    expect("oracle40 布局钉死 client58 ⇒ RED",
           rc_of(check(synthetic_ability(40), expect_52_layout="client58")[0]), 1)
    # body 声明长度与解出长度不符 ⇒ 解码失败必红（不是静默跳过）。
    bad = synthetic_ability(40)
    for r in bad:
        if r["ident"] == 52:
            r["body_len"] = 41
    expect("52 body_len 声明与解出不符 ⇒ RED",
           rc_of(check(bad, expect_52_weight=14)[0]), 1)
    # —— C10#10：auto 预设选择 ——
    f, facts = check(synthetic(True), expect_enter_seq="auto")
    expect("auto：无可选帧 ⇒ default 且 GREEN",
           0 if not f and facts["auto_preset"] == "default" else 1, 0)
    f, facts = check(synthetic_equipped(True), expect_enter_seq="auto")
    expect("auto：621/211 ⇒ equipped 且 GREEN",
           0 if not f and facts["auto_preset"] == "equipped" else 1, 0)
    f, facts = check(synthetic_equipped(True, with_212=True), expect_enter_seq="auto")
    expect("auto：212 ⇒ delmagic 且 GREEN",
           0 if not f and facts["auto_preset"] == "delmagic" else 1, 0)
    f, facts = check(synthetic_g2(), expect_enter_seq="auto")
    expect("auto：210 ⇒ g2 且 GREEN",
           0 if not f and facts["auto_preset"] == "g2" else 1, 0)
    # auto 不放宽：621 位置错（跑到 51 后）仍必须 RED。
    wrong = synthetic(True)
    wrong.insert(2, {"kind": "frame", "dir": "s2c", "seq": 0, "ident": 621,
                     "recog": 0, "tag": 0, "series": 0, "body_len": 0, "raw_hex": "00"})
    expect("auto：621 位置错 ⇒ RED（自动选择不放宽）",
           rc_of(check(wrong, expect_enter_seq="auto")[0]), 1)
    # auto 不放宽：只有 211、没有 621 ⇒ 选 equipped，但必选 621 缺失 ⇒ RED。
    only211 = [r for r in synthetic_equipped(True) if r["ident"] != 621]
    expect("auto：只有 211 无 621 ⇒ RED（不降级到宽松预设）",
           rc_of(check(only211, expect_enter_seq="auto")[0]), 1)
    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", default="")
    ap.add_argument("--expect-52-body-len", type=int, default=None)
    ap.add_argument("--min-appearance-pairs", type=int, default=0)
    ap.add_argument("--expect-enter-seq", default="",
                    help="入图序：逗号列表（元素可写 211? 表可选）或预设 "
                         "default/equipped/delmagic/g2/auto；默认仍是现有 15 帧")
    ap.add_argument("--expect-52-weight", type=int, default=None,
                    help="52 body 的 Weight 字段级门（40B@32 / 58B@46）")
    ap.add_argument("--expect-52-max-exp", type=int, default=None,
                    help="52 body 的 MaxExp 字段级门（40B@28 / 58B@42）")
    ap.add_argument("--expect-52-layout", default=None,
                    help="钉死 52 body 布局：oracle40 | client58")
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
                             a.expect_enter_seq, a.expect_52_weight,
                             a.expect_52_max_exp, a.expect_52_layout)
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
        print("  auto_preset=%s" % facts["auto_preset"])
        print("  52_present=%s body_len=%s frames=%s" %
              (facts["52_present"], facts["52_body_len"], facts["52_frames"]))
        if facts["52_decoded"]:
            d = facts["52_decoded"]
            print("  52_decoded layout=%s Level=%s HP=%s/%s Exp=%s MaxExp=%s "
                  "Weight=%s/%s" %
                  (d["layout"], d["level"], d["hp"], d["max_hp"], d["exp"], d["max_exp"],
                   d["weight"], d["max_weight"]))
        for e in facts["52_decode_errors"]:
            print("  52_decode_error=%s" % e)
        print("  752_present=%s" % facts["752_present"])
        print("  appearance_pairs=%s recogs=%s" % (facts["appearance_pairs"], facts["appearance_recogs"]))
        for f in fails:
            print("  RED: %s" % f)
    print("RESULT: %s" % ("RED" if fails else "GREEN"))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
