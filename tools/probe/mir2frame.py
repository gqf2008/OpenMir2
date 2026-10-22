#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""mir2frame.py — Mir2 线缆帧编解码（协议无头对拍台的公共底座）。

口径来源（不重造，直接收敛）：
  * tools/m1-acceptance/probe_m1_chain.py 的 EDCode encode/decode（已在真机 M1 链路上验证）；
  * src/OpenMir2/EncryptUtil.cs / src/OpenMir2/EDcode.cs（EDCode 权威实现）；
  * src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs（CommandMessage 12B 头）；
  * mir2-rs/crates/protocol/src/frame.rs（线上帧格式说明）。

线上帧格式：
  * 客户端到服务端：'#' '1' <EDCode(12B 头 + 体)> '!'（头体同流编码；12 % 3 == 0，
    分编与同编字节相同）。
  * 服务端到客户端：'#' <EDCode(12B 头)> <EDCode(体)> '!' 或 '!$'（登录跳）。
  * 心跳：单字节 '*'；不在任何帧内。

EDCode 编码后的字节全部落在 0x3C..0x7B，因此 '!'(0x21)、'#'(0x23)、'*'(0x2A)
都不可能出现在已编码载荷里 —— 分帧可以安全地按这些字节切。

本模块 **fail-closed**：越界字节、长度不足、帧未闭合一律抛 FrameError，绝不静默跳过。

用法：
  python tools/probe/mir2frame.py selftest    # 离线自测（不连网）
退出码：0 绿；2 有断言失败（含"改坏必红"的负向用例）。
"""
import struct

SEED = 0xAC
BASE = 0x3C
ENC_MIN = BASE
ENC_MAX = BASE + 0x3F  # 0x7B

CLIENT_FRAME_TAG = b"1"
HEARTBEAT = b"*"
HEADER_SIZE = 12
TAIL_BANG = b"!"
TAIL_DOLLAR = b"!$"


class FrameError(Exception):
    """帧/编码层无法解析 —— 调用方必须按 UNCLASSIFIED 处理（退出码 2）。"""


def encode_bytes(src):
    """EDCode 编码（EncryptUtil.Encode 的纯 Python 等价）。"""
    if not isinstance(src, (bytes, bytearray)):
        raise TypeError("encode_bytes expects bytes")
    out = bytearray()
    no = 2
    rem = 0
    for b in src:
        c = b ^ SEED
        if no == 6:
            out.append(((c & 0x3F) + BASE) & 0xFF)
            rem |= (c >> 2) & 0x30
            out.append((rem + BASE) & 0xFF)
            rem = 0
        else:
            t = c >> 2
            out.append((((t & 0x3C) | (c & 3)) + BASE) & 0xFF)
            rem = ((rem << 2) | (t & 3)) & 0xFF
        no = no % 6 + 2
    if no != 2:
        out.append((rem + BASE) & 0xFF)
    return bytes(out)


def decode_bytes(src, strict=True):
    """EDCode 解码（EncryptUtil.Decode 的纯 Python 等价）。

    strict=True 时校验每个字节都在 0x3C..0x7B；越界即 FrameError。
    """
    if not isinstance(src, (bytes, bytearray)):
        raise TypeError("decode_bytes expects bytes")
    if strict:
        for i, b in enumerate(src):
            if b < ENC_MIN or b > ENC_MAX:
                raise FrameError(
                    "EDCode byte out of range at offset %d: 0x%02X not in 0x3C..0x7B" % (i, b))
    out = bytearray()
    n = len(src)
    cyc = n // 4
    left = n % 4

    def m(x):
        return (x - BASE) & 0xFF

    for i in range(cyc):
        b = i * 4
        r = m(src[b + 3])
        t = m(src[b])
        out.append(((((t << 2) & 0xF0) | (r & 0x0C) | (t & 3)) ^ SEED))
        t = m(src[b + 1])
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED))
        t = m(src[b + 2])
        out.append(((t | ((r << 2) & 0xC0)) ^ SEED))
    if left == 2:
        r = m(src[n - 1])
        t = m(src[n - 2])
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED))
    elif left == 3:
        r = m(src[n - 1])
        t = m(src[n - 3])
        out.append(((((t << 2) & 0xF0) | (r & 0x0C) | (t & 3)) ^ SEED))
        t = m(src[n - 2])
        out.append(((((t << 2) & 0xF0) | ((r << 2) & 0x0C) | (t & 3)) ^ SEED))
    elif left == 1:
        if strict:
            raise FrameError("EDCode stream length %d is 4n+1 (impossible)" % n)
    return bytes(out)


def encode_text(text, encoding="gbk"):
    """文本 -> GBK 字节 -> EDCode（C# EDCode.EncodeString 的线上口径）。"""
    if not isinstance(text, str):
        raise TypeError("encode_text expects str")
    try:
        raw = text.encode(encoding)
    except UnicodeEncodeError as ex:
        raise FrameError("text not encodable as %s: %s" % (encoding, ex)) from ex
    return encode_bytes(raw)


def decode_text(body, encoding="gbk", errors="replace"):
    """EDCode 解码后的字节 -> 文本（展示用；比较器应比字节而不是比文本）。"""
    return body.decode(encoding, errors)


def pack_header(recog, ident, param=0, tag=0, series=0):
    """Recog(i32) | Ident(u16) | Param(u16) | Tag(u16) | Series(u16)，小端。"""
    if not isinstance(recog, int) or isinstance(recog, bool):
        raise FrameError("recog must be int, got %s" % type(recog).__name__)
    if recog < -0x80000000 or recog > 0x7FFFFFFF:
        raise FrameError("recog out of i32 range: %r" % (recog,))
    for name, v in (("ident", ident), ("param", param), ("tag", tag), ("series", series)):
        if not isinstance(v, int) or isinstance(v, bool) or v < 0 or v > 0xFFFF:
            raise FrameError("%s out of u16 range: %r" % (name, v))
    return struct.pack("<iHHHH", recog, ident, param, tag, series)


def unpack_header(header):
    """12B 头 -> dict；长度不是 12 即 FrameError。"""
    if len(header) != HEADER_SIZE:
        raise FrameError("header must be %d bytes, got %d" % (HEADER_SIZE, len(header)))
    recog, ident, param, tag, series = struct.unpack("<iHHHH", header)
    return {"recog": recog, "ident": ident, "param": param,
            "tag": tag, "series": series}


def build_c2s_frame(recog, ident, param=0, tag=0, series=0, body=b""):
    """构造 '#1<EDCode(12B 头 + 明文 body)>!'。

    与真实客户端一致：头与体在同一个编码流里（12 % 3 == 0，分编等价）。
    """
    if body is None:
        body = b""
    if not isinstance(body, (bytes, bytearray)):
        raise TypeError("body must be bytes")
    payload = pack_header(recog, ident, param, tag, series) + bytes(body)
    return b"#" + CLIENT_FRAME_TAG + encode_bytes(payload) + TAIL_BANG


def build_legacy_c2s_frame(body):
    """无 12B 头的旧式客户端帧（GameGate 登录串 '**account/char/...'）。

    线上形状：'#1<EDCode(body)>!'。
    """
    if not isinstance(body, (bytes, bytearray)):
        raise TypeError("body must be bytes")
    return b"#" + CLIENT_FRAME_TAG + encode_bytes(bytes(body)) + TAIL_BANG


def build_s2c_frame(recog, ident, param=0, tag=0, series=0, body=b"", tail=TAIL_BANG):
    """构造服务端帧（回放/自测用）：'#<EDCode(头)><EDCode(体)><tail>'。"""
    if tail not in (TAIL_BANG, TAIL_DOLLAR):
        raise FrameError("bad tail %r" % (tail,))
    head = encode_bytes(pack_header(recog, ident, param, tag, series))
    return b"#" + head + (encode_bytes(bytes(body)) if body else b"") + tail


class Frame:
    """一条完整的线上帧。

    ``body`` 是**线上原样字节**（GameSvr 已编码/或"编码段 + 原样分隔符"的混合体，
    见 SM_ADJUST_BONUS 的 ``EncodeMessage(...) + '/' + ...``）；``body_decoded`` 只是
    对它做的**尽力** EDCode 解码（strict=False），供人看的文本预览/已知文本消息使用。
    比较器必须比 ``body``（== ndjson 的 body_hex），不要比 ``body_decoded``。
    """

    __slots__ = ("tail", "raw", "header", "body", "body_decoded", "recog", "ident",
                 "param", "tag", "series", "offset", "plain", "frame_class")

    def __init__(self, tail, raw, header, body, body_decoded=b"", offset=0,
                 plain=False, frame_class="protocol"):
        self.tail = tail
        self.raw = raw
        self.header = header
        self.body = body
        self.body_decoded = body_decoded
        self.offset = offset
        self.plain = plain
        self.frame_class = frame_class
        if plain:
            self.recog = self.ident = self.param = self.tag = self.series = None
        else:
            h = unpack_header(header)
            self.recog = h["recog"]
            self.ident = h["ident"]
            self.param = h["param"]
            self.tag = h["tag"]
            self.series = h["series"]

    def __repr__(self):
        if self.plain:
            return ("Frame(plain=%s, body=%dB, tail=%s)"
                    % (self.frame_class, len(self.body), self.tail.decode("ascii")))
        return ("Frame(ident=%d, recog=%d, param=%d, tag=%d, series=%d, body=%dB, tail=%s)"
                % (self.ident, self.recog, self.param, self.tag, self.series,
                   len(self.body), self.tail.decode("ascii")))


class Event:
    """切分事件：kind 属于 {frame, heartbeat, garbage}。"""

    __slots__ = ("kind", "frame", "data", "offset")

    def __init__(self, kind, frame=None, data=b"", offset=0):
        self.kind = kind
        self.frame = frame
        self.data = data
        self.offset = offset

    def __repr__(self):
        if self.kind == "frame":
            return "Event(frame=%r)" % (self.frame,)
        return "Event(%s, %dB)" % (self.kind, len(self.data))


class FrameSplitter:
    """增量分帧器：feed(bytes) -> [Event]。

    - '*' -> heartbeat 事件（不是帧）。
    - '#' 开帧，按 '!' / '!$' 收尾；前缀 '1' 表示客户端帧。
    - 帧外其它字节 -> garbage 事件（调用方应判 UNCLASSIFIED，不能当绿）。
    - 不完整帧留在缓冲区等下一次 feed；eof() 时若仍有残留即报错。
    """

    def __init__(self, max_frame_bytes=1 << 20):
        self._buf = bytearray()
        self._consumed = 0
        self.max_frame_bytes = max_frame_bytes
        self.pending = b""

    def feed(self, data):
        if not isinstance(data, (bytes, bytearray)):
            raise TypeError("feed expects bytes")
        self._buf.extend(data)
        events = []
        while self._buf:
            b0 = self._buf[0:1]
            if b0 == HEARTBEAT:
                events.append(Event("heartbeat", offset=self._consumed))
                self._consume(1)
                continue
            if b0 != b"#":
                idx = self._buf.find(b"#")
                if idx < 0:
                    events.append(Event("garbage", data=bytes(self._buf),
                                        offset=self._consumed))
                    self._consume(len(self._buf))
                    break
                events.append(Event("garbage", data=bytes(self._buf[:idx]),
                                    offset=self._consumed))
                self._consume(idx)
                continue
            bang = self._buf.find(b"!", 1)
            if bang < 0:
                if len(self._buf) > self.max_frame_bytes:
                    raise FrameError(
                        "unterminated frame exceeds %d bytes" % self.max_frame_bytes)
                break
            tail = TAIL_DOLLAR if self._buf[bang:bang + 2] == TAIL_DOLLAR else TAIL_BANG
            end = bang + len(tail)
            if end > self.max_frame_bytes:
                raise FrameError("frame exceeds %d bytes" % self.max_frame_bytes)
            raw = bytes(self._buf[:end])
            offset = self._consumed
            self._consume(end)
            events.append(Event("frame", frame=self._parse_frame(raw, offset),
                                offset=offset))
        self.pending = bytes(self._buf)
        return events

    def _consume(self, n):
        del self._buf[:n]
        self._consumed += n

    @staticmethod
    def _parse_frame(raw, offset=0):
        if not raw.startswith(b"#"):
            raise FrameError("frame at offset %d does not start with '#'" % offset)
        tail = TAIL_DOLLAR if raw.endswith(TAIL_DOLLAR) else TAIL_BANG
        start = 2 if raw[1:2] == CLIENT_FRAME_TAG else 1
        inner = raw[start:len(raw) - len(tail)]
        head_enc_len = len(encode_bytes(bytes(HEADER_SIZE)))
        if len(inner) < head_enc_len:
            raise FrameError("frame at offset %d: header payload %dB < %dB"
                             % (offset, len(inner), head_enc_len))
        try:
            header = decode_bytes(inner[:head_enc_len])
            if len(header) != HEADER_SIZE:
                raise FrameError("decoded header %dB != %dB" % (len(header), HEADER_SIZE))
        except FrameError as ex:
            # 服务端的动作 ack 是**明文**帧（BotSrv/LoadMetrics.cs:14）：
            #   '#+GD/<rtime>!'（Modules/SystemModule/MessageSettings.cs sSTATUS_GOOD）
            # 它不是 12B 头协议帧；显式分类，绝不静默当协议帧。
            if inner.startswith(b"+GD/") and inner[4:].isdigit():
                return Frame(tail, raw, b"", bytes(inner), bytes(inner), offset,
                             plain=True, frame_class="server_ack_time")
            raise FrameError("frame at offset %d: header is not EDCode (%s); raw=%s"
                             % (offset, ex, raw[:48].hex()))
        body = bytes(inner[head_enc_len:]) if len(inner) > head_enc_len else b""
        body_decoded = decode_bytes(body, strict=False) if body else b""
        return Frame(tail, raw, header, body, body_decoded, offset)

    def eof(self):
        """流结束：残留字节 = 未闭合帧/垃圾，fail-closed。"""
        if self._buf:
            raise FrameError("%d trailing byte(s) at stream end: %s"
                             % (len(self._buf), bytes(self._buf[:32]).hex()))
        return True


# 兼容 probe_m1_chain 里的旧函数名（口径只有一份，别复制两份）
enc = encode_bytes
dec = decode_bytes


def _selftest():
    fails = []

    def check(name, fn):
        try:
            fn()
        except AssertionError as ex:
            fails.append("%s: %s" % (name, ex))
        except Exception as ex:  # noqa: BLE001
            fails.append("%s: unexpected %s: %s" % (name, type(ex).__name__, ex))

    def t_roundtrip():
        for n in range(0, 97):
            src = bytes(((i * 37 + n * 11) & 0xFF) for i in range(n))
            got = decode_bytes(encode_bytes(src))
            assert got == src, "roundtrip len=%d" % n
        data = bytes(range(256))
        assert decode_bytes(encode_bytes(data)) == data

    # 已知答案向量：r9 真机抓包（E:/tmp/window/r9/proxy/conn-0002.c2s.bin）的原帧。
    # 任何 EDCode / 12B 头布局改动都会让这些断言变红。
    GOLDEN = {
        "click_npc": "23317364686764526b6b6464686b6464686b21",
        "dlg_kaiz": "23317364686764536b6b6464686b6464686b746f49796d724121",
        "dlg_ya5": "23317364686764536b6b6464686b6464686b74714979613e21",
        "dlg_ya52": "23317364686764536b6b6464686b6464686b7471497961624721",
    }

    def t_golden():
        got = {
            "click_npc": build_c2s_frame(119, 1010).hex(),
            "dlg_kaiz": build_c2s_frame(119, 1011, body=b"@kaiz").hex(),
            "dlg_ya5": build_c2s_frame(119, 1011, body=b"@ya5").hex(),
            "dlg_ya52": build_c2s_frame(119, 1011, body=b"@ya52").hex(),
        }
        for k, v in GOLDEN.items():
            assert got[k] == v, "%s: %s != %s" % (k, got[k], v)

    def t_header():
        h = pack_header(-1, 3030, 0xFFFF, 0, 7)
        assert len(h) == 12
        d = unpack_header(h)
        assert d == {"recog": -1, "ident": 3030, "param": 0xFFFF, "tag": 0, "series": 7}
        for bad in (b"", b"\x00" * 11, b"\x00" * 13):
            try:
                unpack_header(bad)
            except FrameError:
                pass
            else:
                raise AssertionError("unpack_header accepted %d bytes" % len(bad))

    def t_split_basic():
        a = build_c2s_frame(0, 3030, body=encode_text("@时间"))
        b = build_s2c_frame(6930, 100, param=65499, body="hello".encode("gbk"),
                            tail=TAIL_DOLLAR)
        sp = FrameSplitter()
        ev = sp.feed(b"*" + a + b"*" + b)
        kinds = [e.kind for e in ev]
        assert kinds == ["heartbeat", "frame", "heartbeat", "frame"], kinds
        assert ev[1].frame.ident == 3030 and ev[1].frame.recog == 0
        assert decode_text(ev[3].frame.body_decoded) == "hello"
        assert ev[3].frame.tail == TAIL_DOLLAR
        sp.eof()

    def t_split_reassembly():
        f = build_s2c_frame(7, 52, param=1, tag=2, series=3, body=bytes(range(40)))
        sp = FrameSplitter()
        got = []
        for i in range(0, len(f), 3):
            got += [e for e in sp.feed(f[i:i + 3]) if e.kind == "frame"]
        assert len(got) == 1 and got[0].frame.ident == 52
        assert got[0].frame.body_decoded == bytes(range(40))
        assert (got[0].frame.param, got[0].frame.tag, got[0].frame.series) == (1, 2, 3)
        sp.eof()

    def t_garbage():
        sp = FrameSplitter()
        ev = sp.feed(b"\x01\x02" + build_s2c_frame(1, 50))
        assert ev[0].kind == "garbage" and ev[0].data == b"\x01\x02", ev
        try:
            FrameSplitter().feed(b"#1!")
        except FrameError:
            pass
        else:
            raise AssertionError("short frame should raise FrameError")

    def t_tamper_red():
        """改坏必红：12B 头是 EDCode 必编区，把一个头字节改成越界值必须报错。

        注意：**body 不能**做这种断言 —— 线上 body 允许出现原样分隔符 '/'（SM_ADJUST_BONUS），
        只对头做 strict 校验才是正确的 fail-closed 口径。
        """
        good = bytearray(build_s2c_frame(1, 50, body=b"abc"))
        good[10] = 0x00  # '#' 后第 9 个字节属于编码头
        try:
            FrameSplitter().feed(bytes(good))
        except FrameError:
            return
        raise AssertionError("tampered header was accepted (selftest tooth is broken)")

    def t_tamper_printable():
        bad = bytearray(build_s2c_frame(1, 50, body=b"abc"))
        bad[10] = 0x7C  # EDCode 上界 0x7B
        try:
            FrameSplitter().feed(bytes(bad))
        except FrameError:
            return
        raise AssertionError("0x7C accepted in header by strict EDCode decode")

    def t_plain_ack():
        f = b"#+GD/1792667727392!"
        ev = FrameSplitter().feed(f)
        assert len(ev) == 1 and ev[0].kind == "frame"
        fr = ev[0].frame
        assert fr.plain and fr.frame_class == "server_ack_time"
        assert fr.ident is None and fr.body == b"+GD/1792667727392"
        try:
            FrameSplitter().feed(b"#hello world!")
        except FrameError:
            pass
        else:
            raise AssertionError("unknown plaintext frame should be fail-closed")

    def t_eof():
        sp = FrameSplitter()
        sp.feed(build_s2c_frame(1, 50)[:-1])
        try:
            sp.eof()
        except FrameError:
            return
        raise AssertionError("eof with pending frame did not fail")

    check("roundtrip", t_roundtrip)
    check("golden-vectors", t_golden)
    check("header", t_header)
    check("split-basic", t_split_basic)
    check("split-reassembly", t_split_reassembly)
    check("garbage-fail-closed", t_garbage)
    check("tamper-red", t_tamper_red)
    check("tamper-printable-red", t_tamper_printable)
    check("plain-ack-classified", t_plain_ack)
    check("eof-fail-closed", t_eof)

    if fails:
        print("mir2frame selftest: FAIL")
        for f in fails:
            print("  -", f)
        return 2
    print("mir2frame selftest: OK (10 checks, incl. 2 tamper teeth)")
    return 0


if __name__ == "__main__":
    import sys
    if len(sys.argv) == 2 and sys.argv[1] == "selftest":
        sys.exit(_selftest())
    print(__doc__)
    sys.exit(2)
