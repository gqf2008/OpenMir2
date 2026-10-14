#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_appearance_pairs.py —— 卡 `openmir2-client-e2e-enter-gaps` 缺口③ root 2026-10-14 00:4x
②「打印可见集大小与下发帧数（**怪物**出现帧数）」的**机器判据**。

`check_client_s2c_frames.py --min-appearance-pairs` 只数 `10 SM_TURN`+`41 SM_FEATURECHANGED`
的**对数**，数不出「这些 actor 是谁」——玩家与怪物的出现帧在计数上不可区分，而 root 问的恰是
**怪物**出现帧。本脚本把每对里 `SM_TURN` 的**帧体**按 oracle 形态解出来，读出 actor 的
`ShowName`（出现帧名尾 = `ShowName/Color`，`downlink::turn_pair` → `PlayObject.Message.cs:1549-1555`），
再用 `Feature` 位域（`feature.rs`：怪物 `MakeMonsterFeature` = `[raceImg:8][weapon:8][appr:16]`，
人类 `MakeHumanFeature` = `[raceImg:8][weapon:8][hair:8][dress:8]`）给出可核对的 actor 分类输入。

**口径（不许自造）**：
- 帧体解码复用协议层同一算法（`crates/protocol/src/edcode.rs` 的 `decode`，逐字节移植自
  `src/OpenMir2/EncryptUtil.cs` / Delphi `EDcode.pas`）；本脚本内联同一算法，**不 import 生产 crate**，
  这样证据在干净检出上可直接 `python -I -B` 复算（无 cargo/无构建）。
- `SM_TURN`(10) 的体布局 = `BodyLayout::StructThenRest { struct_len: 8 }`（`frame.rs::body_layout`）：
  头 16 字符 → 12B 命令头；体 = `Encode(8B CharDesc)` ++ `Encode(ShowName/Color)`
  （`smframe.rs::sm_turn`：两段各自编码后直接相接，故必须**按段长**解码，不能整段解）。
- `SM_FEATURECHANGED`(41) 无体（`body_len=0`）。
- 出现帧对 = s2c 流里**相邻**的 `ident==10` 且其**紧后**一条 `ident==41` 且两者 `recog` 相等。

判据（机判，退出码 0=GREEN / 1=RED / 2=PREREQ）：
- `--min-pairs N`：出现帧对 ≥ N；
- `--min-actors N`：不同 actor（recog）数 ≥ N；
- `--expect-names "稻草人=10,鹿=3"`：解出的 `ShowName` 直方图与期望**逐项相等**（多/少/改名都判红）。

`--selftest` 不依赖外部文件：用本脚本内的 `encode`（`edcode.rs::encode` 的移植）造合成 capture
（含名尾的出现帧对）⇒ 解码/直方图/计数自证；改坏（SEED 错、漏尾、少一对）⇒ 必红。
"""
import argparse
import collections
import json
import os
import sys

SEED = 0xAC
BASE = 0x3C
HEAD_BLOCK = 16          # encode(12B) 恒为 16 字符（frame.rs::decode_server_payload）
TURN_STRUCT_LEN = 8      # SM_TURN 首段 = CharDesc(Feature i32 + Status i32)


# ---------------------------------------------------------------------------
# EDCode（edcode.rs / EncryptUtil.cs 的逐字节移植；本文件内联，供干净检出自足复算）
# ---------------------------------------------------------------------------
def decode(src):
    """等价 `edcode::decode`：3 字节 ← 4 字符；len%4∈{2,3} 补 1/2 字节；len%4==1 丢尾。"""
    src = bytes(src)
    n = len(src)
    cycles = n // 4
    left = n % 4
    out = bytearray()
    for i in range(cycles):
        b = i * 4
        r = (src[b + 3] - BASE) & 0xFF
        t = (src[b] - BASE) & 0xFF
        c = ((t << 2) & 0xF0) | (r & 0x0C) | (t & 0x03)
        out.append((c ^ SEED) & 0xFF)
        t = (src[b + 1] - BASE) & 0xFF
        c = ((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)
        out.append((c ^ SEED) & 0xFF)
        t = (src[b + 2] - BASE) & 0xFF
        c = (t | ((r << 2) & 0xC0)) & 0xFF
        out.append((c ^ SEED) & 0xFF)
    if left == 2:
        r = (src[n - 1] - BASE) & 0xFF
        t = (src[n - 2] - BASE) & 0xFF
        c = ((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)
        out.append((c ^ SEED) & 0xFF)
    elif left == 3:
        r = (src[n - 1] - BASE) & 0xFF
        t = (src[n - 3] - BASE) & 0xFF
        c = ((t << 2) & 0xF0) | (r & 0x0C) | (t & 0x03)
        out.append((c ^ SEED) & 0xFF)
        t = (src[n - 2] - BASE) & 0xFF
        c = ((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)
        out.append((c ^ SEED) & 0xFF)
    return bytes(out)


def encode(src):
    """等价 `edcode::encode`（`edcode::encode_into`）：3 字节 → 4 字符，余 1/2 字节各补 1 字符。"""
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


def encoded_len(n):
    return n + (n + 2) // 3


# ---------------------------------------------------------------------------
# 帧体解码
# ---------------------------------------------------------------------------
class BadFrame(Exception):
    pass


def payload_of(raw_hex):
    """线上帧 = `#` + payload + `!` ⇒ 返回 payload 字节；形状不符即抛。"""
    b = bytes.fromhex(raw_hex)
    if len(b) < HEAD_BLOCK + 2 or b[0] != 0x23 or b[-1] != 0x21:
        raise BadFrame("帧形状不符（无 #/! 或过短）：len=%d" % len(b))
    return b[1:-1]


def decode_turn_body(payload):
    """`SM_TURN` 体 = decode(首 8B 段) ++ decode(余下名尾)；返回 (desc8, msg)。"""
    if len(payload) <= HEAD_BLOCK:
        return b"", b""
    rest = payload[HEAD_BLOCK:]
    first = encoded_len(TURN_STRUCT_LEN)
    if len(rest) <= first:
        return decode(rest), b""
    return decode(rest[:first]), decode(rest[first:])


def split_name(msg):
    """名尾 = `<ShowName>/<Color>`（GBK 字节 + ASCII 数字）；返回 (name_bytes, color_str)。"""
    i = msg.rfind(b"/")
    if i < 0:
        return msg, ""
    return msg[:i], msg[i + 1:].decode("ascii", "replace")


def parse_capture(path):
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def pairs_of(rows):
    """返回 [(turn_row, fc_row)]：s2c 里相邻的 10 → 41 且 recog 相等的对。"""
    s = [r for r in rows if r.get("dir") == "s2c"]
    out = []
    i = 0
    while i < len(s):
        if s[i].get("ident") == 10 and i + 1 < len(s) and s[i + 1].get("ident") == 41 \
                and (s[i].get("recog", -1) & 0xFFFFFFFF) == (s[i + 1].get("recog", -2) & 0xFFFFFFFF):
            out.append((s[i], s[i + 1]))
            i += 2
        else:
            i += 1
    return out


def analyze(rows):
    """解出每个出现帧对的 actor：recog / (x,y) / feature / raceImg / ShowName。"""
    facts = []
    for turn, _fc in pairs_of(rows):
        payload = payload_of(turn["raw_hex"])
        desc, msg = decode_turn_body(payload)
        feat = int.from_bytes(desc[0:4], "little") if len(desc) >= 4 else -1
        status = int.from_bytes(desc[4:8], "little") if len(desc) >= 8 else -1
        name_b, color = split_name(msg)
        facts.append({
            "recog": turn["recog"] & 0xFFFFFFFF,
            "x": turn.get("param"),
            "y": turn.get("tag"),
            "feature": feat,
            "race_img": (feat & 0xFF) if feat >= 0 else -1,
            "status": status,
            "name_gbk": name_b.decode("gbk", "replace"),
            "name_hex": name_b.hex(),
            "color": color,
            "body_len": len(desc) + len(msg),
            "body_len_declared": turn.get("body_len"),
        })
    return facts


def judge(facts, min_pairs=0, min_actors=0, expect_names=None):
    fails = []
    names = collections.Counter(f["name_gbk"] for f in facts)
    actors = len({f["recog"] for f in facts})
    if len(facts) < min_pairs:
        fails.append("出现帧对 %d 个 < 要求 %d 个" % (len(facts), min_pairs))
    if actors < min_actors:
        fails.append("不同 actor %d 个 < 要求 %d 个" % (actors, min_actors))
    if expect_names is not None:
        want = collections.Counter(expect_names)
        if dict(names) != dict(want):
            fails.append("ShowName 直方图不同：got=%s expect=%s"
                         % (dict(sorted(names.items())), dict(sorted(want.items()))))
    for f in facts:
        if f["body_len"] != f["body_len_declared"]:
            fails.append("actor 0x%08x 体长解码 %d != 帧声明 %s"
                         % (f["recog"], f["body_len"], f["body_len_declared"]))
    return fails, {
        "appearance_pairs": len(facts),
        "actors": actors,
        "frames": 2 * len(facts),
        "names": dict(sorted(names.items())),
        "facts": facts,
    }


def parse_names(spec):
    if not spec:
        return None
    out = collections.Counter()
    for item in spec.split(","):
        item = item.strip()
        if not item:
            continue
        if "=" not in item:
            raise ValueError("--expect-names 形如 名=数,名=数：%r" % item)
        k, v = item.split("=", 1)
        out[k.strip()] = int(v)
    return out


# ---------------------------------------------------------------------------
# selftest（合成 capture；不依赖外部文件）
# ---------------------------------------------------------------------------
def build_turn(recog, x, y, feature, msg):
    head = (recog & 0xFFFFFFFF).to_bytes(4, "little") + (10).to_bytes(2, "little") \
        + (x & 0xFFFF).to_bytes(2, "little") + (y & 0xFFFF).to_bytes(2, "little") \
        + (0).to_bytes(2, "little")
    desc = (feature & 0xFFFFFFFF).to_bytes(4, "little") + (0).to_bytes(4, "little")
    payload = encode(head) + encode(desc) + encode(msg)
    return (b"#" + payload + b"!").hex()


def build_fc(recog, feature):
    head = (recog & 0xFFFFFFFF).to_bytes(4, "little") + (41).to_bytes(2, "little") \
        + (feature & 0xFFFF).to_bytes(2, "little") + ((feature >> 16) & 0xFFFF).to_bytes(2, "little") \
        + (0).to_bytes(2, "little")
    return (b"#" + encode(head) + b"!").hex()


def synthetic(monsters=2, deer=1, drop_fc=False):
    rows = []
    seq = 1
    for i in range(monsters):
        msg = "\u7a3b\u8349\u4eba/255".encode("gbk")
        rows.append({"kind": "frame", "dir": "s2c", "seq": seq, "ident": 10,
                     "recog": 1000 + i, "param": 287 + i, "tag": 620,
                     "body_len": 8 + len(msg), "raw_hex": build_turn(1000 + i, 287 + i, 620, 0x001B0012, msg)})
        if not drop_fc:
            rows.append({"kind": "frame", "dir": "s2c", "seq": seq + 1, "ident": 41,
                         "recog": 1000 + i, "param": 18, "tag": 27, "body_len": 0,
                         "raw_hex": build_fc(1000 + i, 0x001B0012)})
        seq += 2
    for i in range(deer):
        msg = "\u9e7f/255".encode("gbk")
        rows.append({"kind": "frame", "dir": "s2c", "seq": seq, "ident": 10,
                     "recog": 2000 + i, "param": 279, "tag": 621,
                     "body_len": 8 + len(msg), "raw_hex": build_turn(2000 + i, 279, 621, 0x00A1000B, msg)})
        rows.append({"kind": "frame", "dir": "s2c", "seq": seq + 1, "ident": 41,
                     "recog": 2000 + i, "param": 11, "tag": 161, "body_len": 0,
                     "raw_hex": build_fc(2000 + i, 0x00A1000B)})
        seq += 2
    return rows


def selftest():
    ok = True

    def expect(name, got, want):
        nonlocal ok
        good = got == want
        ok = ok and good
        print("%-58s %s (rc=%d, want %d)" % (name, "OK" if good else "FAIL", got, want))

    def rc(rows, **kw):
        return 1 if judge(analyze(rows), **kw)[0] else 0

    expect("合成 2 稻草人 + 1 鹿 ⇒ 计数/直方图 GREEN",
           rc(synthetic(), min_pairs=3, min_actors=3, expect_names=parse_names("\u7a3b\u8349\u4eba=2,\u9e7f=1")), 0)
    expect("期望名写错（鹿=2）⇒ RED",
           rc(synthetic(), expect_names=parse_names("\u7a3b\u8349\u4eba=2,\u9e7f=2")), 1)
    expect("--min-pairs 4 ⇒ RED", rc(synthetic(), min_pairs=4), 1)
    expect("缺 41 帧 ⇒ 对数 0 ⇒ RED", rc(synthetic(monsters=2, deer=0, drop_fc=True), min_pairs=1), 1)
    # 改坏必红：先按正确密钥造好帧，再把解码密钥换成 0xAD ⇒ 名尾解不出 ⇒ 直方图不匹配。
    # （必须先生成 rows 再改 SEED：synthetic() 内部 encode 也读 SEED，边造边改会自洽掩掉。）
    good_rows = synthetic()
    global SEED
    keep = SEED
    SEED = 0xAD
    try:
        expect("解码密钥错(SEED=0xAD) ⇒ RED",
               rc(good_rows, expect_names=parse_names("\u7a3b\u8349\u4eba=2,\u9e7f=1")), 1)
    finally:
        SEED = keep
    print("SELFTEST %s" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", default="")
    ap.add_argument("--min-pairs", type=int, default=0)
    ap.add_argument("--min-actors", type=int, default=0)
    ap.add_argument("--expect-names", default="")
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
        rows = parse_capture(a.capture)
    except Exception as e:  # noqa: BLE001
        print("PREREQ_MISSING: %s 解析失败：%s" % (a.capture, e))
        return 2
    try:
        fails, facts = judge(analyze(rows), a.min_pairs, a.min_actors, parse_names(a.expect_names))
    except BadFrame as e:
        print("PREREQ_MISSING: 帧形状不符：%s" % e)
        return 2
    if a.json:
        print(json.dumps({"capture": a.capture, "failures": fails, "facts": facts}, ensure_ascii=False))
    else:
        print("capture=%s" % a.capture)
        print("  appearance_pairs=%d actors=%d frames=%d"
              % (facts["appearance_pairs"], facts["actors"], facts["frames"]))
        print("  names=%s" % json.dumps(facts["names"], ensure_ascii=False))
        for f in facts["facts"]:
            print("  actor=0x%08x (%s,%s) feature=0x%08x raceImg=%d name=%s/%s"
                  % (f["recog"], f["x"], f["y"], f["feature"] & 0xFFFFFFFF, f["race_img"],
                     f["name_gbk"], f["color"]))
        for f in fails:
            print("  RED: %s" % f)
    print("RESULT: %s" % ("RED" if fails else "GREEN"))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
