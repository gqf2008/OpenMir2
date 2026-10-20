#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gwcap_common.py — 网关影子抓包（C10#4）的公共读取 / 切帧 / 解码。

本模块只做三件事，供 `gate_shadow_proxy.py` 与 `gate_timer_gate.py` 共用：

1. **帧界切分**：复用 `tools/capture/segment_frames.py` 的 `segment()`（现网金标准
   同一套规则：s2c `'#' + EDCode + '!'`/`'!$'`，c2s `'#1' + EDCode + '!'`，裸 `'*'`
   为心跳），**不新写一套切帧规则**。
2. **帧时间戳**：把 `mir2_proxy` 风格的 `chunks.ndjson`（data 事件带 `off/n/t_wall_ms`）
   映射到帧：一帧的时间戳 = **完成该帧的那个 TCP 读块**的时间戳。因此时间精度是
   TCP 读粒度（通常远小于 1ms，最坏等于一次 read 的跨度），不得当成单帧发送时刻。
3. **EDCode / ident**：复用既有 C# `EncryptUtil` 移植
   `mir2-rs/tests/parity/loginsrv/oracle_capture.py`，解出 12B 头里的 `ident`
   （头布局：recog i32 / ident u16 / param u16 / tag u16 / series u16）。

另外提供两个**网关超时缺陷帧**的识别（见 `docs/window-run-sheet-20261021.md` C10#4）：

- LoginGate：`16B = '#' + 15 个 EDCode 字符`（C# 只发 `iLen` 字节，末字符与 `'!'` 丢失）；
- SelGate：`18B = 16 个 EDCode 字符 + 0x00 + '!'`（无 `'#'` 前导，中间夹 0）。

这两个形状是 oracle 既有缺陷的逐字节复刻。LoginGate 的 16B 形状**还必须**解出
`ident=528`（`SM_OUTOFCONNECTION`）才算真缺陷帧：普通 18B 正常帧被截成 16B 时
（如 ident=520）形状相同，只靠「len==16 + '#' + 可打印」会误判成超时帧。
SelGate 18B 形状（无 '#' 前导 + `0x00!` 收尾）在字节上已足够唯一。
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
EDCODE_IMPL = os.path.join(
    REPO, "mir2-rs", "tests", "parity", "loginsrv", "oracle_capture.py"
)
SEGMENT_IMPL = os.path.join(REPO, "tools", "capture", "segment_frames.py")

# C# `grobal2.PacketCode`（服务端内部帧 6B 头的前 4 字节，小端）。
PACKET_CODE = 0xAA9AAA9A
# LoginGate 60s 空闲超时帧的 ident（`grobal2.SM_OUTOFCONNECTION`）。
LOGIN_TIMEOUT_IDENT = 528

_edcode = None
_segment = None


def load_edcode(path: str = EDCODE_IMPL):
    """加载既有 EDCode 实现；缺失返回 None（降级为形状判据，不静默猜 ident）。"""
    global _edcode
    if _edcode is not None:
        return _edcode
    if not os.path.isfile(path):
        return None
    spec = importlib.util.spec_from_file_location("_gwcap_edcode", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    _edcode = mod
    return _edcode


def load_segmenter(path: str = SEGMENT_IMPL):
    global _segment
    if _segment is not None:
        return _segment
    if not os.path.isfile(path):
        raise FileNotFoundError(path)
    spec = importlib.util.spec_from_file_location("_gwcap_segment", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    _segment = mod
    return _segment


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def wire_shape(direction: str, frame: bytes) -> str:
    """按线上字节判帧形状（方向相对抓包代理：c2s / s2c）。"""
    f = frame
    if f == b"*":
        return "heartbeat"
    if direction == "s2c":
        # 先认两条缺陷帧，再认正常帧，避免 18B 缺陷帧被当成 s2c-other。
        # 16B 形状还必须 ident=528：普通 18B 帧截成 16B（ident=520 等）不是缺陷帧。
        if (len(f) == 16 and f[:1] == b"#" and all(0x20 <= b < 0x7F for b in f[1:])
                and frame_ident(direction, f) == LOGIN_TIMEOUT_IDENT):
            return "login-timeout-16"
        if len(f) == 18 and f[-2:] == b"\x00!" and f[:1] != b"#":
            return "sel-timeout-18"
        if f[:1] == b"#" and f[-2:] == b"!$":
            return "s2c-hash-bangdollar"
        if f[:1] == b"#" and f[-1:] == b"!":
            return "s2c-hash-bang"
        return "s2c-other"
    if f[:2] == b"#1" and f[-1:] == b"!":
        return "c2s-hash1-bang"
    if f[:1] == b"#" and f[-1:] == b"!":
        return "c2s-hash-bang"
    return "c2s-other"


def frame_ident(direction: str, frame: bytes):
    """尽力解出 `CommandMessage.Ident`；解不出返回 None（不猜）。"""
    ed = load_edcode()
    if ed is None:
        return None
    f = frame
    payload = None
    if direction == "s2c":
        if len(f) >= 3 and f[:1] == b"#" and f[-2:] == b"!$":
            payload = f[1:-2]
        elif len(f) >= 3 and f[:1] == b"#" and f[-1:] == b"!":
            payload = f[1:-1]
        elif len(f) == 16 and f[:1] == b"#":
            # LoginGate 超时缺陷帧：15 个编码字符（缺最后一个）；仍可解出前 9~11B。
            payload = f[1:]
        elif len(f) >= 18 and f[-2:] == b"\x00!":
            # SelGate 超时缺陷帧：16 个完整编码字符 + 0x00 + '!'。
            payload = f[:-2]
    else:
        if len(f) >= 4 and f[:2] == b"#1" and f[-1:] == b"!":
            payload = f[2:-1]
        elif len(f) >= 3 and f[:1] == b"#" and f[-1:] == b"!":
            payload = f[1:-1]
    if payload is None:
        return None
    try:
        dec = ed.edcode_decode(payload)
    except Exception:
        return None
    if len(dec) < 6:
        return None
    return int.from_bytes(dec[4:6], "little")


def _expand_login16_defects(items, direction):
    """把「16B LoginGate 缺陷帧 + 后续帧」从单个 '!' 帧里拆出来。

    16B 缺陷帧没有 `'!'` 收尾，`segment_frames.segment()` 会把它与下一帧粘成一块
    （流形如 `#<15 chars>#<16 chars>!`）。EDCode 输出字符恒 >= 0x3C，正常帧内部
    不可能出现 `'#'`/`'*'`，因此在偏移 16 处见到 `'#'`/`'*'` 即可确定是
    「16B 缺陷帧 + 下一帧」。尾包恰好 16B 时保持 `kind=partial`，由 `_frame_row`
    按 wire 形状升级为 `frame`（截断写是完整的一次写，只是缺 `'!'` 收尾）。
    """
    if direction != "s2c":
        return items  # LoginGate 缺陷帧只在网关→客户端方向
    out = []
    for off, frame, kind in items:
        pos = 0
        rest = frame
        while (len(rest) >= 16 and rest[:1] == b"#"
               and all(0x20 <= b < 0x7F for b in rest[1:16])):
            nxt = rest[16:17]
            if nxt in (b"#", b"*"):
                out.append((off + pos, rest[:16], "frame"))
                pos += 16
                rest = rest[16:]
                continue
            break
        if rest:
            out.append((off + pos, rest, kind))
    return out


def _frame_row(seq, conn, port, direction, off, frame, kind, t_wall_ms, t_rel_us):
    wire = wire_shape(direction, frame)
    if kind == "partial" and wire == "login-timeout-16":
        kind = "frame"  # 16B 截断写：缺 '!' 收尾，但已按形状+ident 识别为缺陷帧
    return {
        "seq": seq,
        "event": "frame",
        "t_wall_ms": t_wall_ms,
        "t_rel_us": t_rel_us,
        "conn": conn,
        "port": port,
        "dir": direction,
        "off": off,
        "len": len(frame),
        "sha256": sha256_bytes(frame),
        "kind": kind,
        "wire": wire,
        "ident": frame_ident(direction, frame),
        "hex": frame[:64].hex(),
    }


def new_segment_state(conn, port, direction):
    return {
        "conn": conn, "port": port, "direction": direction,
        "pending": bytearray(), "base": 0, "seq": 0, "last_t": (None, None),
    }


def feed_segmenter(state, buf, t_wall_ms, t_rel_us):
    """向一条 (conn,dir) 的流式切帧状态喂一个 TCP 读块；返回新完成的帧行。

    `state` = {"pending", "base", "seq", "last_t"}；`base` 是 pending 首字节的
    绝对偏移。未闭合尾包留在 pending，等下一块；流结束由 `flush_segmenter()` 落盘。
    """
    state["pending"].extend(buf)
    state["last_t"] = (t_wall_ms, t_rel_us)
    seg = load_segmenter()
    rows = []
    consumed = 0
    for off, frame, kind in _expand_login16_defects(
            seg.segment(bytes(state["pending"])), state["direction"]):
        if kind == "partial":
            consumed = off
            break
        state["seq"] += 1
        rows.append(_frame_row(
            state["seq"], state["conn"], state["port"], state["direction"],
            state["base"] + off, frame, kind, t_wall_ms, t_rel_us,
        ))
        consumed = off + len(frame)
    if consumed:
        del state["pending"][:consumed]
        state["base"] += consumed
    return rows


def flush_segmenter(state):
    """流结束：把未闭合的尾包落成 `kind=partial` 行（不丢字节）。"""
    if not state["pending"]:
        return []
    t_wall_ms, t_rel_us = state.get("last_t", (None, None))
    frame = bytes(state["pending"])
    state["seq"] += 1
    row = _frame_row(
        state["seq"], state["conn"], state["port"], state["direction"],
        state["base"], frame, "partial", t_wall_ms, t_rel_us,
    )
    state["pending"].clear()
    state["base"] += len(frame)
    return [row]


def segment_with_times(chunks, stream):
    """完整流 + data 块时间线 → 帧行（离线复算，与代理在线切帧同规则）。"""
    if chunks:
        first = chunks[0]
        state = new_segment_state(
            first.get("conn", 0), first.get("port"), first.get("dir", "s2c")
        )
    else:
        state = new_segment_state(0, None, "s2c")
    rows = []
    for ch in chunks:
        off, n = ch["off"], ch["n"]
        blob = stream[off:off + n]
        if len(blob) != n:
            raise ValueError(
                "chunks.ndjson 的 off/n 越界：off=%d n=%d stream=%d" % (off, n, len(stream))
            )
        rows.extend(feed_segmenter(state, blob, ch.get("t_wall_ms"), ch.get("t_rel_us")))
    rows.extend(flush_segmenter(state))
    return rows


def parse_serverdata_packets(chunks, stream):
    """解析 6B `ServerDataPacket` 流（SelGate↔DBSrv 等），带完成时间戳。

    返回 [{"t_wall_ms","t_rel_us","packet_code","packet_len","kind","body_hex"}]。
    失步时前向扫描 `PacketCode` 重新对齐；坏头不静默当作正常包。
    """
    rows = []
    pending = bytearray()
    tstate = {"t_wall_ms": None, "t_rel_us": None}
    magic = PACKET_CODE.to_bytes(4, "little")

    def drain():
        while len(pending) >= 6:
            code = int.from_bytes(pending[0:4], "little")
            plen = int.from_bytes(pending[4:6], "little")
            if code != PACKET_CODE:
                idx = pending.find(magic, 1)
                if idx < 0:
                    del pending[:-3]  # 保留可能是半个魔数的尾部
                    return
                del pending[:idx]
                continue
            total = 6 + plen
            if len(pending) < total:
                return
            body = bytes(pending[6:total])
            kind = body[1] if len(body) >= 2 and body[0] == 0x04 else None
            rows.append({
                "t_wall_ms": tstate["t_wall_ms"],
                "t_rel_us": tstate["t_rel_us"],
                "packet_code": code,
                "packet_len": plen,
                "kind": kind,
                "body_hex": body[:32].hex(),
            })
            del pending[:total]

    for ch in chunks:
        blob = stream[ch["off"]:ch["off"] + ch["n"]]
        if len(blob) != ch["n"]:
            raise ValueError("chunks.ndjson 的 off/n 越界")
        tstate["t_wall_ms"] = ch.get("t_wall_ms")
        tstate["t_rel_us"] = ch.get("t_rel_us")
        pending.extend(blob)
        drain()
    return rows


class Capture:
    """一个 `gate_shadow_proxy` / `mir2_proxy` 输出目录的只读视图。"""

    def __init__(self, capture_dir):
        self.dir = capture_dir
        self.chunks_path = os.path.join(capture_dir, "chunks.ndjson")
        if not os.path.isfile(self.chunks_path):
            raise FileNotFoundError("缺 chunks.ndjson：" + self.chunks_path)
        self.session = {}
        sj = os.path.join(capture_dir, "session.json")
        if os.path.isfile(sj):
            with open(sj, "r", encoding="utf-8") as fh:
                try:
                    self.session = json.load(fh)
                except json.JSONDecodeError as exc:
                    raise ValueError(
                        "session.json 无法解析（半截写？）：%s" % exc
                    ) from exc
        self.chunks = []
        with open(self.chunks_path, "r", encoding="utf-8") as fh:
            for lineno, line in enumerate(fh, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    self.chunks.append(json.loads(line))
                except json.JSONDecodeError as exc:
                    raise ValueError(
                        "chunks.ndjson 第 %d 行无法解析（半截写？）：%s"
                        % (lineno, exc)
                    ) from exc
        self.opens = {}
        self.closes = {}
        self.fails = {}
        self.data_chunks = {}
        for row in self.chunks:
            ev = row.get("event")
            if ev == "open":
                self.opens[row["conn"]] = row
            elif ev == "close":
                self.closes[row["conn"]] = row
            elif ev == "upstream_fail":
                self.fails[row["conn"]] = row
            elif ev == "data":
                key = (row["conn"], row["dir"])
                self.data_chunks.setdefault(key, []).append(row)
        for key in self.data_chunks:
            self.data_chunks[key].sort(key=lambda r: (r["off"], r["t_wall_ms"]))
        self.streams = {}
        for conn in self.opens:
            for direction in ("c2s", "s2c"):
                path = os.path.join(
                    capture_dir, "conn-%04d.%s.bin" % (conn, direction)
                )
                if os.path.isfile(path):
                    with open(path, "rb") as fh:
                        self.streams[(conn, direction)] = fh.read()
        self._frames = {}
        self._packets = {}

    def gate_port(self, conn):
        """该连接在 session.json 里映射到的上游端口；无映射则返回监听端口。"""
        listen = self.opens.get(conn, {}).get("port")
        mapping = self.session.get("map") or {}
        upstream = mapping.get(str(listen))
        if upstream:
            try:
                return int(upstream.rsplit(":", 1)[1])
            except (ValueError, IndexError):
                pass
        return listen

    def listen_port(self, conn):
        return self.opens.get(conn, {}).get("port")

    def stream(self, conn, direction):
        return self.streams.get((conn, direction), b"")

    def chunks_for(self, conn, direction):
        return self.data_chunks.get((conn, direction), [])

    def frames(self, conn, direction):
        key = (conn, direction)
        if key not in self._frames:
            self._frames[key] = segment_with_times(
                self.chunks_for(conn, direction), self.stream(conn, direction)
            )
        return self._frames[key]

    def packets(self, conn, direction):
        key = (conn, direction)
        if key not in self._packets:
            self._packets[key] = parse_serverdata_packets(
                self.chunks_for(conn, direction), self.stream(conn, direction)
            )
        return self._packets[key]

    def conns(self):
        return sorted(self.opens)

    def close_time(self, conn):
        row = self.closes.get(conn)
        return None if row is None else row.get("t_wall_ms")

    def end_time(self, conn):
        """连接终止时间：真实 close 或 upstream_fail（重连失败也计一次尝试结束）。"""
        if conn in self.closes:
            return self.closes[conn].get("t_wall_ms")
        if conn in self.fails:
            return self.fails[conn].get("t_wall_ms")
        return None

    def open_time(self, conn):
        row = self.opens.get(conn)
        return None if row is None else row.get("t_wall_ms")
