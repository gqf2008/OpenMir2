#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""capture_login_compare.py — 窗口受控登录夹具：两侧（C# oracle / Rust）入图 15 帧逐帧对账。

输入（每侧任选其一）：
  * 一个目录：优先读 `conn-*.s2c.bin`（mir2_proxy 的原始下行字节流，按 `!` 切帧、`$` 收尾）；
    目录里没有 bin 时回退到 `capture.jsonl`。
  * 一个 `capture.jsonl`：`export_s2c.ps1` / GoldenExport 的产物（schema 与金标准同口径）。
  * 一个 `conn-*.s2c.bin` / `*.bin` 文件。

判据（每帧三列，**只比这三列**，口径写死在报告里）：
  1. `ident`：12B 命令头的 ident 必须相等；15 条自帧序列默认
     `658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46`（`--expect-seq` 可换）。
  2. `body_len`：解出的 body 字节数必须相等（52 的 40/58 布局对除外，见白名单）。
  3. `body`：body 的**解码后字节**必须逐字节相等（52 的 40/58 布局对除外）。

白名单（只有两条；每条都在报告里显式列出，不是静默忽略）：
  * `recog`：12B 头里的 `CommandMessage.Recog` = 会话相关的 ActorId（`ActorMgr.GetNextIdentity()`），
    两侧必然不同。本工具**从不比较** recog/param/tag/series（只比 ident + body），并把 recog 差异
    作为 `whitelisted` 记录；`--strict-head` 时 param/tag/series 差异才升级为 RED（recog 仍白名单）。
  * `52 layout`：C# oracle 的 `SM_ABILITY` body = **40B**（`Ability.cs` 声明序），冻结客户端
    `TAbility`（`HIGHHP=True`）的 Rust 侧 body = **58B**（`record.rs::encode_ability_client`）。
    仅当两侧 body_len 恰为 {40,58} 时按布局白名单；**语义字段仍逐项比对**（level/ac/mac/dc/mc/sc/
    hp/mp/max_hp/max_mp/exp/max_exp/weight/max_weight/wear_weight/max_wear_weight/hand_weight/
    max_hand_weight），语义不一致 ⇒ RED。`--52-layout-only` 可退回「只看长度」，默认不推荐：
    此时报告显式写 `52_layout_only=true` / `52_semantic_compared=false` / `unverified=["52_semantic"]`，
    CLI 也打 WARN——该报告不能当语义等价证据。

分段帧：811 `SM_...` 是 3×20B 各自编码、用 `/` 连接（`frame.rs::body_layout` 的 Segmented 契约）。
本工具默认从**协议源码**派生布局表（复用 `tools/capture/dump_body_layout.py`，不抄第二份），
也可 `--layout <body_layout.json>` 指定现成表；派生失败且遇到分段帧 ⇒ PREREQ（不静默平铺解码）。

用法：
  python -I -B tools/loginfixture/capture_login_compare.py \
      --oracle E:/tmp/win/csharp/proxy --rust E:/tmp/win/rust/proxy
  # 或
  python -I -B tools/loginfixture/capture_login_compare.py \
      --oracle E:/tmp/win/csharp/capture.jsonl --rust E:/tmp/win/rust/capture.jsonl --out report.json
  python -I -B tools/loginfixture/capture_login_compare.py --selftest

退出码：0 = 无非白名单差异（GREEN）/ 1 = 有差异（RED）/ 2 = 前置不满足（PREREQ，绝不静默通过）。
`--selftest` 单独约定：0 = GREEN / 1 = RED / 2 = INCOMPLETE（离线金标准缺失，断言没跑满，绝不假绿）。

本车道（tier=L，tools/）**没有**在真机窗口里跑过本脚本；`--selftest` 只用合成数据，另有
「离线金标准」互验（`tests/golden/session-20261010-c3-hook/proxy` ↔ `mir2-rs/tests/golden/
c-line-baseline-c3-20261010.jsonl`），不连库、不起栈、不抓包。
"""
import argparse
import glob
import hashlib
import importlib.util
import json
import os
import struct
import sys
import tempfile

SEED = 0xAC
BASE = 0x3C
HEAD_BLOCK = 16

# 入图自帧 15 条（`s2c_diff.py` / `check_client_s2c_frames.py` 同口径）。
ENTER_SEQ = [658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46]

# `frame.rs::body_layout` 里**非 Single** 的消息号：没有布局表就不能平铺解码。
#   - 811 SM_ADJUST_BONUS：3×20B 分段，`/` 连接、无尾分隔；
#   - 201 SM_BAGITEMS：每件 124B 一段 + 尾 `/`，段数取头部 series；
#   - 6/7/9/10 SM_RUSH/SM_RUSHKUNG/SM_BACKSTEP/SM_TURN：StructThenRest{8}（两段各自编码）。
# 缺表时若遇到这些帧（例如 811 被平铺成 62B 仍判 GREEN），必须 PREREQ ⇒ fail-closed。
LAYOUT_REQUIRED_IDENTS = frozenset({6, 7, 9, 10, 201, 811})

# 52 body 两套布局（与 check_client_s2c_frames.py 的 ABILITY_LAYOUTS 同偏移；此处内联是为了
# 本工具在干净检出上可自足复算，不 import 另一个验收脚本）。
ABILITY_LAYOUTS = {
    40: {
        "name": "oracle40",
        "level": ("<B", 0), "ac": ("<H", 2), "mac": ("<H", 4), "dc": ("<H", 6),
        "mc": ("<H", 8), "sc": ("<H", 10), "hp": ("<H", 12), "mp": ("<H", 14),
        "max_hp": ("<H", 16), "max_mp": ("<H", 18),
        "exp": ("<i", 24), "max_exp": ("<i", 28),
        "weight": ("<H", 32), "max_weight": ("<H", 34),
        "wear_weight": ("<B", 36), "max_wear_weight": ("<B", 37),
        "hand_weight": ("<B", 38), "max_hand_weight": ("<B", 39),
    },
    58: {
        "name": "client58",
        "level": ("<H", 0), "ac": ("<i", 2), "mac": ("<i", 6), "dc": ("<i", 10),
        "mc": ("<i", 14), "sc": ("<i", 18), "hp": ("<i", 22), "mp": ("<i", 26),
        "max_hp": ("<i", 30), "max_mp": ("<i", 34),
        "exp": ("<i", 38), "max_exp": ("<i", 42),
        "weight": ("<H", 46), "max_weight": ("<H", 48),
        "wear_weight": ("<H", 50), "max_wear_weight": ("<H", 52),
        "hand_weight": ("<H", 54), "max_hand_weight": ("<H", 56),
    },
}
ABILITY_FIELDS = ("level", "ac", "mac", "dc", "mc", "sc", "hp", "mp", "max_hp", "max_mp",
                  "exp", "max_exp", "weight", "max_weight", "wear_weight", "max_wear_weight",
                  "hand_weight", "max_hand_weight")

NORMALIZATION = {
    "recog": "CommandMessage.Recog = ActorId（会话相关；不参与比较，差异记 whitelisted）",
    "head_other": "param/tag/series 不参与默认判据；--strict-head 时差异升级为 RED（recog 仍白名单）",
    "52_layout": "oracle40 (40B) vs client58 (58B) 的 body 字节差异按布局白名单；语义字段仍逐项比",
    "52_layout_only": "--52-layout-only 时 52 只比 40/58 长度；18 个语义字段未比较"
                      "（报告 52_semantic_compared=false / unverified=[52_semantic]）",
}


class Prereq(Exception):
    pass


class DecodeError(Exception):
    pass


def decode(src):
    """EDCode 解码（EncryptUtil.cs 的逐字节移植；与 oracle_capture.py 同实现）。"""
    src = bytes(src)
    n = len(src)
    out = bytearray()
    m = lambda x: (x - BASE) & 0xFF  # noqa: E731
    for i in range(n // 4):
        b = i * 4
        r = m(src[b + 3])
        t = m(src[b])
        out.append((((t << 2) & 0xF0) | (r & 0x0C) | (t & 0x03)) ^ SEED)
        t = m(src[b + 1])
        out.append((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)) ^ SEED)
        t = m(src[b + 2])
        out.append((t | ((r << 2) & 0xC0)) ^ SEED)
    left = n % 4
    if left == 2:
        r = m(src[n - 1])
        t = m(src[n - 2])
        out.append((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)) ^ SEED)
    elif left == 3:
        r = m(src[n - 1])
        t = m(src[n - 3])
        out.append((((t << 2) & 0xF0) | (r & 0x0C) | (t & 0x03)) ^ SEED)
        t = m(src[n - 2])
        out.append((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 0x03)) ^ SEED)
    return bytes(out)


def encode(src):
    """EDCode 编码（只用于 selftest 造合成帧与报文；生产路径只解码）。"""
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


def encoded_len(plain_len):
    return (plain_len // 3) * 4 + {0: 0, 1: 2, 2: 3}[plain_len % 3]


def command_head(ident, recog=0, param=0, tag=0, series=0):
    return struct.pack("<iHHHH", recog, ident, param, tag, series)


class Frame(object):
    __slots__ = ("ident", "recog", "param", "tag", "series", "body", "body_len",
                 "layout_kind", "form", "declared_body_len", "declared_sha",
                 "declared_layout", "decode_error", "conn", "seq", "source")

    def __init__(self):
        self.ident = None
        self.recog = 0
        self.param = 0
        self.tag = 0
        self.series = 0
        self.body = b""
        self.body_len = 0
        self.layout_kind = "none"
        self.form = "command"
        self.declared_body_len = None
        self.declared_sha = None
        self.declared_layout = None
        self.decode_error = None
        self.conn = None
        self.seq = None
        self.source = None


def sha256(b):
    return hashlib.sha256(b).hexdigest()


# ---------------------------------------------------------------- body layout / decode

def load_layout_module():
    here = os.path.dirname(os.path.abspath(__file__))
    repo = os.path.abspath(os.path.join(here, "..", ".."))
    tool = os.path.join(repo, "tools", "capture", "dump_body_layout.py")
    if not os.path.exists(tool):
        return None
    spec = importlib.util.spec_from_file_location("_openmir2_body_layout", tool)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod, repo


def repo_layout():
    """从协议源码派生布局表；失败返回 (None, 原因)。"""
    try:
        loaded = load_layout_module()
        if loaded is None:
            return None, "找不到 tools/capture/dump_body_layout.py"
        mod, repo = loaded
        frame_rs = os.path.join(repo, "mir2-rs", "crates", "protocol", "src", "frame.rs")
        consts = os.path.join(repo, "mir2-rs", "crates", "protocol", "src", "messages.rs")
        for p in (frame_rs, consts):
            if not os.path.exists(p):
                return None, "找不到协议源码 %s" % p
        consts_map = mod.parse_messages(consts)
        table = mod.parse_body_layout(frame_rs, consts_map)
        return table, "derived from %s" % frame_rs
    except SystemExit as e:
        return None, "dump_body_layout 解析失败：%s" % e
    except Exception as e:  # noqa: BLE001 - 派生是可选加速，失败要转成 PREREQ 信息
        return None, "派生布局表异常：%s: %s" % (type(e).__name__, e)


class BodyDecoder(object):
    def __init__(self, table, source):
        self.table = table or {}
        self.source = source
        self.layouts = self.table.get("layouts", {}) if table else {}

    def decode_body(self, ident, series, enc):
        spec = self.layouts.get(str(ident))
        if not spec:
            if ident in LAYOUT_REQUIRED_IDENTS:
                if not self.layouts:
                    why = "布局表为空（%s）" % self.source
                else:
                    why = "布局表（%s）缺 ident=%d 条目" % (self.source, ident)
                # Prereq 不会被 parse_wire_frame 的 DecodeError 分支吞掉：一路传到 main ⇒ rc=2。
                raise Prereq("ident=%d 的体必须按分段布局解码，否则会平铺出错误字节：%s；"
                             "请恢复 tools/capture/dump_body_layout.py 派生或用 --layout 指定"
                             " body_layout.json" % (ident, why))
            return decode(enc), "single"
        kind = spec.get("kind")
        if kind == "segmented":
            sep = int(spec.get("sep", 0x2F))
            if 0x3C <= sep <= 0x7B:
                raise DecodeError("分段分隔符 0x%02x 落在 EDCode 编码字节域内，拒绝按字节切分" % sep)
            parts = enc.split(bytes([sep]))
            if spec.get("trailing_sep") and parts and parts[-1] == b"":
                parts = parts[:-1]
            if spec.get("count_kind") == "header_series":
                want = series
            else:
                want = int(spec.get("count", 0))
            if len(parts) != want:
                raise DecodeError("ident=%d 分 %d 段 != 期望 %d 段" % (ident, len(parts), want))
            seg_len = int(spec.get("seg_len", 0))
            segs = [decode(p) for p in parts]
            for i, s in enumerate(segs):
                if len(s) != seg_len:
                    raise DecodeError("ident=%d 第 %d 段解出 %d B != 期望 %d B"
                                      % (ident, i, len(s), seg_len))
            return b"".join(segs), "segmented"
        if kind == "struct_then_rest":
            n = encoded_len(int(spec.get("struct_len", 0)))
            if len(enc) < n:
                raise DecodeError("ident=%d struct_then_rest 编码体只有 %d B < %d B"
                                  % (ident, len(enc), n))
            return decode(enc[:n]) + decode(enc[n:]), "struct_then_rest"
        return decode(enc), "single"


# ---------------------------------------------------------------- wire / jsonl parsing

def split_wire_frames(data):
    """按实测帧界切分：`*` 心跳单字节；其余到 `!` 收尾，末尾可带 `$`。"""
    out = []
    off = 0
    n = len(data)
    while off < n:
        if data[off] == 0x2A:  # '*'
            out.append((off, data[off:off + 1], "heartbeat"))
            off += 1
            continue
        idx = data.find(b"!", off)
        if idx < 0:
            out.append((off, data[off:], "partial"))
            break
        end = idx + 1
        if end < n and data[end] == 0x24:  # '$'
            end += 1
        out.append((off, data[off:end], "frame"))
        off = end
    return out


def parse_wire_frame(raw, decoder):
    f = Frame()
    if not raw or raw[:1] != b"#":
        f.form = "non_frame"
        return f
    if raw.endswith(b"!$"):
        inner = raw[1:-2]
    elif raw.endswith(b"!"):
        inner = raw[1:-1]
    else:
        inner = raw[1:]
    if inner.startswith(b"+GD/"):
        f.form = "act"
        f.body = inner
        f.body_len = len(inner)
        return f
    if len(inner) < HEAD_BLOCK:
        f.form = "short"
        return f
    head = decode(inner[:HEAD_BLOCK])
    if len(head) < 12:
        f.form = "short"
        return f
    f.form = "command"
    f.recog, f.ident, f.param, f.tag, f.series = struct.unpack("<iHHHH", head[:12])
    enc = inner[HEAD_BLOCK:]
    try:
        f.body, f.layout_kind = decoder.decode_body(f.ident, f.series, enc)
        f.body_len = len(f.body)
    except DecodeError as e:
        f.decode_error = str(e)
        f.body = b""
        f.body_len = -1
        f.layout_kind = "decode_error"
    return f


def parse_bin_bytes(data, decoder, source, conn=None):
    frames = []
    seq = 0
    partial = 0
    for _off, raw, kind in split_wire_frames(data):
        if kind == "heartbeat":
            continue
        if kind == "partial":
            partial += 1
            continue
        seq += 1
        f = parse_wire_frame(raw, decoder)
        f.seq = seq
        f.source = source
        f.conn = conn
        if f.form == "command" or f.form in ("act", "short"):
            frames.append(f)
    return frames, partial


def parse_bin_file(path, decoder):
    with open(path, "rb") as fh:
        data = fh.read()
    conn = None
    base = os.path.basename(path)
    if base.startswith("conn-") and ".s2c.bin" in base:
        try:
            conn = int(base.split("-")[1].split(".")[0])
        except (IndexError, ValueError):
            pass
    return parse_bin_bytes(data, decoder, path, conn=conn)


def parse_capture_jsonl(path, decoder, port=7200):
    streams = {}
    skipped = 0
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                skipped += 1
                continue
            if row.get("kind") not in (None, "frame"):
                continue
            if row.get("dir") != "s2c":
                continue
            if port is not None and row.get("port") not in (None, port):
                continue
            if row.get("frame_form") == "act" or row.get("string_frame"):
                continue
            raw_hex = row.get("raw_hex") or ""
            try:
                raw = bytes.fromhex(raw_hex)
            except ValueError:
                skipped += 1
                continue
            f = parse_wire_frame(raw, decoder)
            f.seq = row.get("seq")
            f.source = path
            f.conn = row.get("conn")
            f.declared_body_len = row.get("body_len")
            f.declared_sha = row.get("body_sha256")
            f.declared_layout = row.get("layout")
            if f.form == "command":
                if (str(f.declared_layout or "").startswith("segmented")
                        and f.layout_kind != "segmented" and f.decode_error is None):
                    f.decode_error = ("capture 声明 segmented，但布局表未命中（用 --layout "
                                      "或让协议源码派生成功）")
                # 自洽校验：解出的 body 必须与捕获声明的 body_len/body_sha256 一致。
                if f.declared_body_len is not None and f.decode_error is None:
                    if f.declared_body_len != f.body_len:
                        f.decode_error = ("解出 body_len=%d != 捕获声明 %s"
                                          % (f.body_len, f.declared_body_len))
                if f.declared_sha and f.decode_error is None:
                    if f.declared_sha != sha256(f.body):
                        f.decode_error = "解出 body_sha256 != 捕获声明"
            streams.setdefault(f.conn if f.conn is not None else 0, []).append(f)
    return streams, skipped


def load_side(path, decoder, port=7200):
    """返回 (streams: name -> [Frame], kind, warnings)。"""
    warnings = []
    if os.path.isdir(path):
        bins = sorted(glob.glob(os.path.join(path, "conn-*.s2c.bin")))
        if bins:
            streams = {}
            for p in bins:
                frames, partial = parse_bin_file(p, decoder)
                streams[os.path.basename(p)] = frames
                if partial:
                    warnings.append("%s 有 %d 个未闭合尾包（已跳过）" % (os.path.basename(p), partial))
            return streams, "bin-dir", warnings
        cap = os.path.join(path, "capture.jsonl")
        if os.path.exists(cap):
            streams, skipped = parse_capture_jsonl(cap, decoder, port=port)
            if skipped:
                warnings.append("%s 跳过 %d 行不可解析记录" % (cap, skipped))
            return streams, "jsonl", warnings
        raise Prereq("目录里既没有 conn-*.s2c.bin 也没有 capture.jsonl：%s" % path)
    if not os.path.exists(path):
        raise Prereq("输入不存在：%s" % path)
    low = path.lower()
    if low.endswith(".jsonl") or low.endswith(".ndjson"):
        streams, skipped = parse_capture_jsonl(path, decoder, port=port)
        if skipped:
            warnings.append("%s 跳过 %d 行不可解析记录" % (path, skipped))
        return streams, "jsonl", warnings
    if low.endswith(".bin"):
        frames, partial = parse_bin_file(path, decoder)
        if partial:
            warnings.append("%s 有 %d 个未闭合尾包（已跳过）" % (os.path.basename(path), partial))
        return {os.path.basename(path): frames}, "bin", warnings
    # 兜底：按 JSONL 试读（有些 capture 文件不带扩展名）。
    try:
        streams, skipped = parse_capture_jsonl(path, decoder, port=port)
        if streams:
            return streams, "jsonl", warnings
    except (UnicodeDecodeError, OSError):
        pass
    raise Prereq("不认识的输入（要目录/conn-*.s2c.bin/.bin/capture.jsonl）：%s" % path)


def find_enter_sequence(frames, start, upto):
    for i, f in enumerate(frames):
        if f.form == "command" and f.ident == start:
            out = []
            for g in frames[i:]:
                if g.form != "command":
                    continue
                out.append(g)
                if g.ident == upto:
                    return out
            return None
    return None


def choose_stream(streams, start, upto, conn=None):
    candidates = []
    for key, frames in streams.items():
        if conn is not None:
            match = (key == conn) or (str(key) == str(conn))
            if not match:
                continue
        seq = find_enter_sequence(frames, start, upto)
        if seq:
            candidates.append((key, seq))
    if not candidates:
        keys = ", ".join(str(k) for k in streams)
        return None, "找不到 ident=%d→%d 的入图序列（流：%s）" % (start, upto, keys or "<空>")
    if len(candidates) > 1:
        keys = ", ".join(str(k) for k, _ in candidates)
        return None, ("多条流都含 ident=%d→%d（%s）；用 --conn 指定"
                      % (start, upto, keys))
    return candidates[0], None


# ---------------------------------------------------------------- compare

def decode_ability(body):
    layout = ABILITY_LAYOUTS.get(len(body))
    if layout is None:
        return None
    out = {"layout": layout["name"], "body_len": len(body)}
    try:
        for name in ABILITY_FIELDS:
            fmt, off = layout[name]
            out[name] = struct.unpack_from(fmt, body, off)[0]
    except struct.error:
        return None
    return out


def first_body_diff(a, b):
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return i
    return min(len(a), len(b))


def compare_sequences(o_seq, r_seq, expect, strict_head=False, layout_only_52=False):
    diffs = []
    whitelisted = []
    head_diag = []
    rows = []

    def add_diff(idx, ident, field, oracle, rust, kind):
        diffs.append({"idx": idx, "ident": ident, "field": field, "kind": kind,
                      "oracle": str(oracle)[:200], "rust": str(rust)[:200]})

    if len(o_seq) != len(r_seq):
        add_diff(None, None, "frame_count", len(o_seq), len(r_seq), "count")
    if [f.ident for f in o_seq] != expect:
        add_diff(None, None, "sequence", [f.ident for f in o_seq], expect, "oracle_sequence")
    if [f.ident for f in r_seq] != expect:
        add_diff(None, None, "sequence", [f.ident for f in r_seq], expect, "rust_sequence")

    for i in range(min(len(o_seq), len(r_seq))):
        o, r = o_seq[i], r_seq[i]
        ident = o.ident if o.ident is not None else r.ident
        body_state = "equal"
        notes = []
        if o.form != "command" or r.form != "command":
            add_diff(i, ident, "form", o.form, r.form, "form")
            body_state = "DIFF"
        # ident
        if o.ident != r.ident:
            add_diff(i, ident, "ident", o.ident, r.ident, "ident")
            body_state = "DIFF"
        # decode errors（fail closed）
        if o.decode_error or r.decode_error:
            add_diff(i, ident, "decode", o.decode_error, r.decode_error, "decode")
            body_state = "DIFF"
        # head diagnostics / optional strict head
        for field in ("recog", "param", "tag", "series"):
            ov, rv = getattr(o, field), getattr(r, field)
            if ov == rv:
                continue
            if field == "recog":
                whitelisted.append({
                    "idx": i, "ident": ident, "field": "recog",
                    "oracle": ov, "rust": rv,
                    "reason": "CommandMessage.Recog = 会话相关 ActorId（白名单）",
                })
            elif strict_head:
                add_diff(i, ident, field, ov, rv, "head")
                body_state = "DIFF"
            else:
                head_diag.append({"idx": i, "ident": ident, "field": field,
                                  "oracle": ov, "rust": rv,
                                  "note": "不参与默认判据（--strict-head 才 RED）"})
        # body_len + body
        if o.decode_error is None and r.decode_error is None:
            layout_pair = (o.body_len, r.body_len) in ((40, 58), (58, 40))
            if o.ident == 52 and r.ident == 52 and layout_pair:
                if layout_only_52:
                    body_state = "WL52"
                    whitelisted.append({
                        "idx": i, "ident": 52, "field": "52_layout",
                        "oracle": o.body_len, "rust": r.body_len,
                        "reason": "oracle40/client58 布局差（--52-layout-only：不看语义）",
                    })
                else:
                    od = decode_ability(o.body)
                    rd = decode_ability(r.body)
                    if od is None or rd is None:
                        add_diff(i, 52, "52_decode", o.body_len, r.body_len, "52_decode")
                        body_state = "DIFF"
                    else:
                        mism = [(k, od[k], rd[k]) for k in ABILITY_FIELDS if od[k] != rd[k]]
                        if mism:
                            add_diff(i, 52, "52_semantic",
                                     ", ".join("%s=%s" % (k, ov) for k, ov, _ in mism),
                                     ", ".join("%s=%s" % (k, rv) for k, _, rv in mism),
                                     "52_semantic")
                            body_state = "DIFF"
                        else:
                            body_state = "WL52"
                            whitelisted.append({
                                "idx": i, "ident": 52, "field": "52_layout",
                                "oracle": o.body_len, "rust": r.body_len,
                                "reason": "oracle40/client58 布局差；18 个语义字段逐项相等",
                            })
            else:
                if o.body_len != r.body_len:
                    add_diff(i, ident, "body_len", o.body_len, r.body_len, "body_len")
                    body_state = "DIFF"
                elif o.body != r.body:
                    off = first_body_diff(o.body, r.body)
                    add_diff(i, ident, "body", "sha=%s off=%d" % (sha256(o.body)[:16], off),
                             "sha=%s off=%d" % (sha256(r.body)[:16], off), "body")
                    body_state = "DIFF"
        if o.layout_kind != r.layout_kind:
            notes.append("layout %s/%s" % (o.layout_kind, r.layout_kind))
        elif o.layout_kind not in ("single", "none"):
            notes.append("layout=%s" % o.layout_kind)
        if o.body_len < 0 or r.body_len < 0:
            notes.append("decode_error")
        rows.append({
            "idx": i,
            "oracle_ident": o.ident, "rust_ident": r.ident,
            "oracle_body_len": o.body_len, "rust_body_len": r.body_len,
            "body": body_state,
            "note": "; ".join(notes),
        })
    return {
        "tool": "loginfixture/capture_login_compare.py",
        "expect_seq": expect,
        "normalization": NORMALIZATION,
        "frames": rows,
        "diffs": diffs,
        "whitelisted": whitelisted,
        "head_diag": head_diag,
        "52_layout_only": bool(layout_only_52),
        "52_semantic_compared": not layout_only_52,
        "unverified": ["52_semantic"] if layout_only_52 else [],
        "pass": len(diffs) == 0,
    }


def compare_paths(oracle_path, rust_path, args, decoder, layout_source):
    o_streams, o_kind, o_warn = load_side(oracle_path, decoder, port=args.port)
    r_streams, r_kind, r_warn = load_side(rust_path, decoder, port=args.port)
    expect = parse_expect(args.expect_seq)
    start = args.start_ident if args.start_ident is not None else expect[0]
    upto = args.upto_ident if args.upto_ident is not None else expect[-1]
    o_sel, o_err = choose_stream(o_streams, start, upto, conn=args.conn)
    if o_sel is None:
        raise Prereq("oracle 侧：%s" % o_err)
    r_sel, r_err = choose_stream(r_streams, start, upto, conn=args.conn)
    if r_sel is None:
        raise Prereq("rust 侧：%s" % r_err)
    o_name, o_seq = o_sel
    r_name, r_seq = r_sel
    report = compare_sequences(o_seq, r_seq, expect, strict_head=args.strict_head,
                               layout_only_52=args.layout_only_52)
    report.update({
        "oracle": os.path.abspath(oracle_path),
        "rust": os.path.abspath(rust_path),
        "oracle_kind": o_kind, "rust_kind": r_kind,
        "oracle_stream": str(o_name), "rust_stream": str(r_name),
        "port": args.port,
        "layout_source": layout_source,
        "warnings": o_warn + r_warn,
        "strict_head": args.strict_head,
    })
    return report


def parse_expect(spec):
    if not spec:
        return list(ENTER_SEQ)
    out = []
    for tok in spec.split(","):
        tok = tok.strip()
        if tok:
            out.append(int(tok, 0))
    if not out:
        raise Prereq("--expect-seq 为空")
    return out


def emit(report):
    print("TOOL %s" % report["tool"])
    print("ORACLE %s (%s, stream=%s)" % (report["oracle"], report["oracle_kind"],
                                         report["oracle_stream"]))
    print("RUST   %s (%s, stream=%s)" % (report["rust"], report["rust_kind"],
                                         report["rust_stream"]))
    print("LAYOUT_SOURCE %s" % report["layout_source"])
    if report.get("52_layout_only"):
        print("WARN 52_layout_only=true：52 只按 40/58 长度白名单，18 个语义字段【未比较】"
              "（unverified=[52_semantic]）——本报告不能当语义等价证据")
    print("EXPECT_SEQ %s" % ",".join(str(x) for x in report["expect_seq"]))
    for w in report.get("warnings", []):
        print("WARN %s" % w)
    print("%3s %6s %6s %9s %9s %-8s %s"
          % ("idx", "o_ident", "r_ident", "o_blen", "r_blen", "body", "note"))
    for row in report["frames"]:
        print("%3d %6s %6s %9s %9s %-8s %s"
              % (row["idx"], row["oracle_ident"], row["rust_ident"],
                 row["oracle_body_len"], row["rust_body_len"], row["body"], row["note"]))
    for w in report["whitelisted"]:
        print("WHITELISTED idx=%s ident=%s field=%s (%s -> %s)"
              % (w["idx"], w["ident"], w["field"], w["oracle"], w["rust"]))
    for d in report["head_diag"]:
        print("HEAD_DIAG idx=%s ident=%s %s (%s -> %s) [not judged]"
              % (d["idx"], d["ident"], d["field"], d["oracle"], d["rust"]))
    for d in report["diffs"]:
        print("DIFF idx=%s ident=%s field=%s kind=%s oracle=%s rust=%s"
              % (d["idx"], d["ident"], d["field"], d["kind"], d["oracle"], d["rust"]))
    if report["pass"]:
        print("RESULT GREEN: %d/%d 帧，0 非白名单差异（whitelisted=%d，head_diag=%d）"
              % (len(report["frames"]), len(report["expect_seq"]),
                 len(report["whitelisted"]), len(report["head_diag"])))
    else:
        print("RESULT RED: %d 处非白名单差异" % len(report["diffs"]))


def main(argv=None):
    ap = build_parser()
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.oracle or not args.rust:
        print("PREREQ_MISSING: 必须给 --oracle 与 --rust（或 --selftest）", file=sys.stderr)
        return 2
    table = None
    if args.layout:
        if not os.path.exists(args.layout):
            print("PREREQ_MISSING: --layout 文件不存在：%s" % args.layout, file=sys.stderr)
            return 2
        with open(args.layout, "r", encoding="utf-8") as f:
            table = json.load(f)
        layout_source = os.path.abspath(args.layout)
    elif args.no_layout:
        table = None
        layout_source = "none (--no-layout)"
    else:
        table, why = repo_layout()
        layout_source = why
        if table is None:
            print("WARN: 未能从协议源码派生 body 布局表（%s）；分段帧会 PREREQ" % why,
                  file=sys.stderr)
    decoder = BodyDecoder(table, layout_source)
    try:
        report = compare_paths(args.oracle, args.rust, args, decoder, layout_source)
    except Prereq as e:
        print("PREREQ: %s" % e, file=sys.stderr)
        return 2
    emit(report)
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as f:
            json.dump(report, f, ensure_ascii=False, indent=1)
        print("REPORT %s" % os.path.abspath(args.out))
    return 0 if report["pass"] else 1


def build_parser():
    ap = argparse.ArgumentParser(description="窗口受控登录夹具：两侧入图 15 帧逐帧对账")
    ap.add_argument("--oracle", default="", help="C# 侧目录 / conn-*.s2c.bin / capture.jsonl")
    ap.add_argument("--rust", default="", help="Rust 侧目录 / conn-*.s2c.bin / capture.jsonl")
    ap.add_argument("--port", type=int, default=7200, help="只取该端口（capture.jsonl 用；默认 7200）")
    ap.add_argument("--conn", default=None, help="多流时指定连接（bin 文件名或 jsonl 的 conn）")
    ap.add_argument("--expect-seq", default=",".join(str(x) for x in ENTER_SEQ),
                    help="期望 ident 序列（默认 15 条入图序）")
    ap.add_argument("--start-ident", type=int, default=None)
    ap.add_argument("--upto-ident", type=int, default=None)
    ap.add_argument("--strict-head", action="store_true",
                    help="param/tag/series 差异升级为 RED（recog 仍白名单）")
    ap.add_argument("--52-layout-only", action="store_true", dest="layout_only_52",
                    help="52 只按 40/58 长度白名单，不比语义（不推荐）")
    ap.add_argument("--layout", default="", help="body_layout.json（默认从协议源码派生）")
    ap.add_argument("--no-layout", action="store_true", help="不派生布局表（分段帧会 PREREQ）")
    ap.add_argument("--out", default="", help="落 JSON 报告")
    ap.add_argument("--selftest", action="store_true")
    return ap


# ---------------------------------------------------------------- selftest

def ability_body(layout, **over):
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


# 入图 15 帧的 body 长度（与金标准一致；811 是 3×20 分段）。
BODY_LENS = {658: 320, 51: 1, 50: 16, 41: 0, 213: 0, 11029: 23, 42: 5,
             766: 0, 54: 6, 5008: 9, 1106: 4, 52: 40, 752: 0, 811: 60, 46: 0}


def body_for(ident, layout52=40, hp=None, mutate=None):
    if ident == 52:
        kw = {"hp": hp} if hp is not None else {}
        return ability_body(layout52, **kw)
    n = BODY_LENS[ident]
    body = bytes(((i * 7 + ident) & 0xFF) for i in range(n))
    if mutate and ident == mutate[0]:
        i, val = mutate[1], mutate[2]
        body = body[:i] + bytes([val]) + body[i + 1:]
    return body


# 合成侧的头部参数（真实抓包里的代表值；recog 两侧故意不同）。
HEAD_PARAMS = {
    658: (2000, 0, 0, 0), 51: (0x1111, 287, 618, 0), 50: (0x1111, 287, 618, 0),
    41: (0x1111, 0, 2, 0), 213: (0, 0, 0, 0), 11029: (0, 0, 0, 0),
    42: (0x1111, 255, 0, 0), 766: (0, 0, 0, 0), 54: (-1, 0, 0, 0),
    5008: (0, 0, 0, 0), 1106: (0, 0, 0, 0), 52: (0, 25344, 0, 0),
    752: (1, 3845, 0, 0), 811: (0, 0, 0, 0), 46: (0, 1, 0, 0),
}


def build_frame(ident, body, recog, param, tag, series, decoder):
    head = encode(command_head(ident, recog, param, tag, series))
    spec = decoder.layouts.get(str(ident)) if decoder else None
    if spec and spec.get("kind") == "segmented":
        sep = bytes([int(spec.get("sep", 0x2F))])
        seg_len = int(spec.get("seg_len", 0))
        segs = [body[i:i + seg_len] for i in range(0, len(body), seg_len)]
        enc = sep.join(encode(s) for s in segs)
        if spec.get("trailing_sep"):
            enc += sep
    else:
        enc = encode(body) if body else b""
    return b"#" + head + enc + b"!"


def synthetic_frames(layout52=40, hp=None, mutate=None, recog_base=0x2000, ident_reorder=None):
    frames = []
    for ident in ENTER_SEQ:
        rec, param, tag, series = HEAD_PARAMS[ident]
        body = body_for(ident, layout52, hp=hp, mutate=mutate)
        # 让 recog 两侧不同：oracle 用基础值，rust 用基础值+0x1000（真实会话也是不同 ActorId）。
        use = rec
        if rec != 0 and rec != -1:
            use = rec + recog_base
        frames.append({"ident": ident, "body": body, "recog": use, "param": param,
                       "tag": tag, "series": series})
    if ident_reorder:
        a, b = ident_reorder
        ia = next(i for i, f in enumerate(frames) if f["ident"] == a)
        ib = next(i for i, f in enumerate(frames) if f["ident"] == b)
        frames[ia], frames[ib] = frames[ib], frames[ia]
    return frames


def write_jsonl(path, frames, decoder, conn=3):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        for i, fr in enumerate(frames):
            raw = build_frame(fr["ident"], fr["body"], fr["recog"], fr["param"],
                              fr["tag"], fr["series"], decoder)
            spec = decoder.layouts.get(str(fr["ident"])) if decoder else None
            layout = spec.get("kind") if spec else "single"
            row = {
                "kind": "frame", "dir": "s2c", "port": 7200, "seq": i, "conn": conn,
                "ident": fr["ident"], "recog": fr["recog"], "param": fr["param"],
                "tag": fr["tag"], "series": fr["series"], "layout": layout,
                "body_len": len(fr["body"]), "body_sha256": sha256(fr["body"]),
                "raw_hex": raw.hex(),
            }
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


def write_bin(path, frames, decoder, act_between=None):
    out = bytearray()
    for i, fr in enumerate(frames):
        if act_between is not None and i == act_between:
            out += b"#+GD/123456!"
        out += build_frame(fr["ident"], fr["body"], fr["recog"], fr["param"],
                           fr["tag"], fr["series"], decoder) + b"$"
    with open(path, "wb") as f:
        f.write(bytes(out))


def crosscheck_ability_layouts():
    """与 check_client_s2c_frames.py 的 ABILITY_LAYOUTS 逐字段对账（Minor 3）。

    返回 (导入的模块或 None, 问题列表)。两处偏移/格式必须逐字段相等；对方只登记自己
    用到的字段子集时，只比对交集（同名布局 length/name 仍要比）。
    """
    here = os.path.dirname(os.path.abspath(__file__))
    repo = os.path.abspath(os.path.join(here, "..", ".."))
    other = os.path.join(repo, "mir2-rs", "tests", "parity", "world",
                         "client-e2e-enter-gaps-20261013", "check_client_s2c_frames.py")
    if not os.path.exists(other):
        return None, ["找不到交叉校验对象：%s" % other]
    spec = importlib.util.spec_from_file_location("_openmir2_check_frames", other)
    if spec is None or spec.loader is None:
        return None, ["无法加载 %s" % other]
    mod = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(mod)
    except Exception as e:  # noqa: BLE001 - selftest 要把导入失败变成 RED 而不是栈回溯
        return None, ["导入 %s 失败：%s: %s" % (other, type(e).__name__, e)]
    theirs_all = getattr(mod, "ABILITY_LAYOUTS", None)
    if not isinstance(theirs_all, dict):
        return mod, ["%s 没有 ABILITY_LAYOUTS 字典" % other]
    problems = []
    for size in sorted(set(ABILITY_LAYOUTS) | set(theirs_all)):
        mine = ABILITY_LAYOUTS.get(size)
        theirs = theirs_all.get(size)
        if mine is None or theirs is None:
            problems.append("布局 %dB 只在一侧存在" % size)
            continue
        if mine.get("name") != theirs.get("name"):
            problems.append("%dB.name %r != %r" % (size, mine.get("name"), theirs.get("name")))
        for field in ABILITY_FIELDS:
            if field not in theirs:
                continue  # 对方只登记自己使用的字段子集
            if field not in mine:
                problems.append("%dB.%s 只在对方存在" % (size, field))
            elif tuple(mine[field]) != tuple(theirs[field]):
                problems.append("%dB.%s %s != %s" % (size, field, mine[field], theirs[field]))
    return mod, problems


def selftest():
    ok = True
    incomplete = False

    def expect(name, cond, detail=""):
        nonlocal ok
        good = bool(cond)
        ok = ok and good
        print("%-70s %s%s" % (name, "OK" if good else "FAIL",
                              (" " + str(detail)) if (detail and not good) else ""))

    table, why = repo_layout()
    if table is None:
        print("SELFTEST PREREQ: 无法派生协议布局表：%s" % why)
        return 2
    decoder = BodyDecoder(table, why)

    def args_for(*extra):
        return build_parser().parse_args(["--oracle", "x", "--rust", "x"] + list(extra))

    def compare_pair(o_path, r_path, *extra):
        a = args_for(*extra)
        return compare_paths(o_path, r_path, a, decoder, why)

    with tempfile.TemporaryDirectory(prefix="loginfix-") as td:
        # 1) 52 40 vs 58、recog 不同 ⇒ GREEN
        p_o = os.path.join(td, "o.jsonl")
        p_r = os.path.join(td, "r.jsonl")
        write_jsonl(p_o, synthetic_frames(40, recog_base=0), decoder)
        write_jsonl(p_r, synthetic_frames(58, recog_base=0x5000), decoder)
        rep = compare_pair(p_o, p_r)
        expect("52 40/58 语义相等 + recog 不同 ⇒ GREEN", rep["pass"],
               rep["diffs"][:2])
        expect("15 帧全部纳入", len(rep["frames"]) == 15, len(rep["frames"]))
        expect("recog 差异记为白名单", any(w["field"] == "recog" for w in rep["whitelisted"]))
        expect("52 布局差异记为白名单", any(w["field"] == "52_layout" for w in rep["whitelisted"]))

        # 2) 原始 bin（含 811 分段 + 中间插明文 act 帧）⇒ GREEN
        b_o = os.path.join(td, "conn-0003.s2c.bin")
        b_r = os.path.join(td, "conn-0004.s2c.bin")
        write_bin(b_o, synthetic_frames(40, recog_base=0), decoder, act_between=3)
        write_bin(b_r, synthetic_frames(58, recog_base=0x5000), decoder, act_between=10)
        rep = compare_pair(b_o, b_r)
        expect("bin 输入（811 分段 + act 帧跳过）⇒ GREEN", rep["pass"], rep["diffs"][:3])
        expect("811 按 3×20 段解出 60B",
               rep["frames"][13]["oracle_body_len"] == 60
               and rep["frames"][13]["rust_body_len"] == 60,
               rep["frames"][13])

        # 3) 目录输入
        d_o = os.path.join(td, "o_dir")
        d_r = os.path.join(td, "r_dir")
        os.makedirs(d_o)
        os.makedirs(d_r)
        write_bin(os.path.join(d_o, "conn-0003.s2c.bin"), synthetic_frames(40, recog_base=0), decoder)
        write_bin(os.path.join(d_r, "conn-0003.s2c.bin"), synthetic_frames(58, recog_base=0x5000), decoder)
        rep = compare_pair(d_o, d_r)
        expect("目录输入 ⇒ GREEN", rep["pass"], rep["diffs"][:2])

        # 4) 非 52 的 body 改 1 字节 ⇒ RED
        p_mut = os.path.join(td, "r_mut_body.jsonl")
        write_jsonl(p_mut, synthetic_frames(58, recog_base=0x5000, mutate=(54, 2, 0xFF)), decoder)
        rep = compare_pair(p_o, p_mut)
        expect("非 52 body 改 1 字节 ⇒ RED", not rep["pass"]
               and any(d["ident"] == 54 and d["field"] == "body" for d in rep["diffs"]),
               rep["diffs"][:3])

        # 5) 52 语义改 hp ⇒ RED（布局白名单不能盖住语义差）
        p_sem = os.path.join(td, "r_mut_sem.jsonl")
        write_jsonl(p_sem, synthetic_frames(58, recog_base=0x5000, hp=20), decoder)
        rep = compare_pair(p_o, p_sem)
        expect("52 40/58 但 hp 不同 ⇒ RED", not rep["pass"]
               and any(d["field"] == "52_semantic" and "hp" in d["oracle"] for d in rep["diffs"]),
               rep["diffs"][:3])

        # 6) 两侧同为 40 但 body 不同 ⇒ RED（白名单只对布局对生效）
        p_40 = os.path.join(td, "o40.jsonl")
        p_40b = os.path.join(td, "r40.jsonl")
        write_jsonl(p_40, synthetic_frames(40, recog_base=0), decoder)
        write_jsonl(p_40b, synthetic_frames(40, recog_base=0x5000, hp=20), decoder)
        rep = compare_pair(p_40, p_40b)
        expect("52 两侧同布局但 body 不同 ⇒ RED", not rep["pass"]
               and any(d["ident"] == 52 and d["field"] in ("body", "body_len")
                       for d in rep["diffs"]), rep["diffs"][:3])

        # 7) ident 顺序错 ⇒ RED
        p_wrong = os.path.join(td, "r_wrong_order.jsonl")
        write_jsonl(p_wrong, synthetic_frames(58, recog_base=0x5000,
                                              ident_reorder=(51, 50)), decoder)
        rep = compare_pair(p_o, p_wrong)
        expect("ident 顺序错 ⇒ RED", not rep["pass"]
               and any(d["field"] == "sequence" for d in rep["diffs"]), rep["diffs"][:3])

        # 8) 缺 46（序列不完整）⇒ PREREQ
        p_short = os.path.join(td, "r_short.jsonl")
        write_jsonl(p_short, synthetic_frames(58, recog_base=0x5000)[:-1], decoder)
        try:
            compare_pair(p_o, p_short)
            expect("缺 46 ⇒ PREREQ", False, "却返回了报告")
        except Prereq:
            expect("缺 46 ⇒ PREREQ", True)

        # 9) 头部 param 差异：默认只诊断，--strict-head 才 RED
        p_head = os.path.join(td, "r_head.jsonl")
        fr = synthetic_frames(58, recog_base=0x5000)
        fr[-1]["param"] = 2  # 46 的 Bright
        write_jsonl(p_head, fr, decoder)
        rep = compare_pair(p_o, p_head)
        expect("param 差异默认不 RED", rep["pass"], rep["diffs"][:2])
        expect("param 差异记 head_diag", any(d["field"] == "param" for d in rep["head_diag"]))
        rep = compare_pair(p_o, p_head, "--strict-head")
        expect("--strict-head 后 param 差异 ⇒ RED", not rep["pass"]
               and any(d["field"] == "param" for d in rep["diffs"]), rep["diffs"][:3])

        # 10) 52 白名单不得外溢到别的 ident（54 也是 40 vs 58）
        p_54a = os.path.join(td, "o54.jsonl")
        p_54b = os.path.join(td, "r54.jsonl")
        fa = synthetic_frames(40, recog_base=0)
        fb = synthetic_frames(58, recog_base=0x5000)
        # 把 54 的 body 拉长到 58B（模拟有人把布局差归错 ident）
        for f in fa:
            if f["ident"] == 54:
                f["body"] = b"X" * 40
        for f in fb:
            if f["ident"] == 54:
                f["body"] = b"X" * 58
        write_jsonl(p_54a, fa, decoder)
        write_jsonl(p_54b, fb, decoder)
        rep = compare_pair(p_54a, p_54b)
        expect("非 52 的 40/58 长度差 ⇒ RED", not rep["pass"]
               and any(d["ident"] == 54 and d["field"] == "body_len" for d in rep["diffs"]),
               rep["diffs"][:3])

        # 11) 缺布局表 + 分段帧 ⇒ PREREQ（fail-closed；不许把 811 平铺成 62B 还判 GREEN）
        empty_decoder = BodyDecoder({}, "selftest empty table")
        try:
            compare_paths(b_o, b_r, args_for(), empty_decoder, "none (selftest empty table)")
            expect("缺布局表 + 811 分段帧 ⇒ PREREQ", False, "却返回了报告")
        except Prereq as e:
            expect("缺布局表 + 811 分段帧 ⇒ PREREQ", "811" in str(e) or "分段" in str(e), e)

        # 12) --52-layout-only 是显式降级：仍 GREEN，但报告必须记录口径与「语义未比较」
        rep = compare_pair(p_o, p_sem, "--52-layout-only")
        expect("--52-layout-only hp 不同仍 GREEN（降级口径）", rep["pass"], rep["diffs"][:3])
        expect("报告记录 52_layout_only=true / semantic=false",
               rep.get("52_layout_only") is True and rep.get("52_semantic_compared") is False,
               (rep.get("52_layout_only"), rep.get("52_semantic_compared")))
        expect("报告 unverified 含 52_semantic", "52_semantic" in rep.get("unverified", []),
               rep.get("unverified"))
        expect("默认报告 52_layout_only=false", compare_pair(p_o, p_r).get("52_layout_only") is False)

        # 13) ABILITY_LAYOUTS 与 check_client_s2c_frames.py 逐字段一致（不抄第二份口径）
        other_mod, problems = crosscheck_ability_layouts()
        expect("ABILITY_LAYOUTS 与 check_client_s2c_frames.py 逐字段一致",
               other_mod is not None and not problems, problems[:3])

        # 14) 离线金标准互验：真实 bin（c3-hook）↔ 真实 capture.jsonl 必须 GREEN
        here = os.path.dirname(os.path.abspath(__file__))
        repo = os.path.abspath(os.path.join(here, "..", ".."))
        gold_bin = os.path.join(repo, "tests", "golden",
                                "session-20261010-c3-hook", "proxy")
        gold_jsonl = os.path.join(repo, "mir2-rs", "tests", "golden",
                                  "c-line-baseline-c3-20261010.jsonl")
        if os.path.isdir(gold_bin) and os.path.exists(gold_jsonl):
            rep = compare_pair(gold_bin, gold_jsonl)
            expect("离线金标准 bin ↔ capture.jsonl ⇒ GREEN", rep["pass"], rep["diffs"][:3])
            expect("离线金标准 15 帧 ident 全等",
                   [r["oracle_ident"] for r in rep["frames"]] == ENTER_SEQ,
                   [r["oracle_ident"] for r in rep["frames"]])
        else:
            incomplete = True
            print("%-70s SKIP（仓库里没有离线金标准文件）" % "离线金标准互验")

    if not ok:
        print("SELFTEST RED")
        return 1
    if incomplete:
        # SKIP 不许假绿：断言没跑满就是非零（2 = INCOMPLETE）。
        print("SELFTEST INCOMPLETE（离线金标准缺失，交叉互验断言未执行）")
        return 2
    print("SELFTEST GREEN")
    return 0


if __name__ == "__main__":
    sys.exit(main())
