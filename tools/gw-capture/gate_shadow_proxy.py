#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""gate_shadow_proxy.py — LoginGate / SelGate 影子 TCP 抓包代理（C10#4，tier=L）。

与 `tools/capture/mir2_proxy.py` 同一套捕获格式，额外产出逐帧时间线：

  <out>/session.json          会话元信息（端口映射、上游端口→网关标签、方向过滤、启动命令）
  <out>/chunks.ndjson         逐块时间线（open/close/data 事件；**与 mir2_proxy 同 schema**）
  <out>/conn-NNNN.c2s.bin     客户端→网关（或网关→下游）原始字节流
  <out>/conn-NNNN.s2c.bin     网关→客户端（或下游→网关）原始字节流
  <out>/frames.ndjson         逐帧（seq/conn/port/dir/off/len/sha256/kind/wire/ident/时间戳/hex）
  <out>/ready.flag            就绪标记（与 mir2_proxy 一致）

用法（影子端口示例；真实 7000/7100 由网关占用时**不要**直接绑）：

  # 客户端侧：影子 7001 -> 真 LoginGate 7000
  python -I -B gate_shadow_proxy.py --out E:/tmp/g7/login \
      --map 7001=127.0.0.1:7000 --gate login
  # 客户端侧：影子 7101 -> 真 SelGate 7100
  python -I -B gate_shadow_proxy.py --out E:/tmp/g7/sel \
      --map 7101=127.0.0.1:7100 --gate sel
  # 下游侧：SelGate 的 ServerPort0 指向影子 5510 -> 真 DBSrv 5100（看 10s 心跳/重连）
  python -I -B gate_shadow_proxy.py --out E:/tmp/g7/sel-db \
      --map 5510=127.0.0.1:5100 --gate sel-downstream

`--direction s2c|c2s` 只落盘一个方向（另一边仍**照常透传**；不改变网关行为），
用于「单向下行/上行帧」取证。`--verify DIR` 对既有抓包做 chunks 与 frames 的无损对账。

退出码：0 GREEN / 1 RED（verify 失败或参数非法）/ 2 PREREQ（缺依赖/目录）。
不绑定 7000/7100、不连接 E:/MirServer：影子端口必须由调用方显式给出。
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
PROXY_IMPL = os.path.join(REPO, "tools", "capture", "mir2_proxy.py")


def _load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _deps():
    if not os.path.isfile(PROXY_IMPL):
        print("PREREQ_MISSING: 找不到 %s" % PROXY_IMPL, file=sys.stderr)
        sys.exit(2)
    mir2_proxy = _load("_gwcap_mir2_proxy", PROXY_IMPL)
    gwcap = _load("_gwcap_common", os.path.join(HERE, "gwcap_common.py"))
    return mir2_proxy, gwcap


def infer_gate(upstream_port, explicit=None):
    if explicit and explicit != "auto":
        return explicit
    if upstream_port == 7000:
        return "login"
    if upstream_port == 7100:
        return "sel"
    if upstream_port == 5100:
        return "sel-downstream"
    return "other"


def make_classes(mir2_proxy, gwcap):
    class GateDump(mir2_proxy.Dump):
        """mir2_proxy.Dump 的超集：方向过滤 + frames.ndjson + 网关标签。"""

        def __init__(self, out_dir, direction="both", frames=True, port_meta=None):
            super().__init__(out_dir)
            self.direction = direction
            self.frames_enabled = frames
            self.port_meta = port_meta or {}
            self.total = {}
            self._seg = {}
            self._frame_seq = 0
            self._frames_fh = None
            if frames:
                self._frames_fh = open(
                    os.path.join(out_dir, "frames.ndjson"), "w",
                    encoding="utf-8", buffering=1,
                )

        def _recording(self, direction):
            return self.direction == "both" or self.direction == direction

        def data(self, conn, port, direction, buf):
            key = (conn, direction)
            self.total[key] = self.total.get(key, 0) + len(buf)
            if not self._recording(direction):
                return
            t_wall_ms = int(time.time() * 1000)
            t_rel_us = self.rel_us()
            super().data(conn, port, direction, buf)
            if self._frames_fh is None:
                return
            st = self._seg.get(key)
            if st is None:
                st = gwcap.new_segment_state(conn, port, direction)
                self._seg[key] = st
            st["port"] = port
            for row in gwcap.feed_segmenter(st, buf, t_wall_ms, t_rel_us):
                self._emit(row)

        def _emit(self, row):
            self._frame_seq += 1
            row["seq"] = self._frame_seq
            row["gate"] = self.port_meta.get(row["port"], {}).get("gate", "other")
            self._frames_fh.write(json.dumps(row, ensure_ascii=False) + "\n")

        def _flush_conn(self, conn):
            if self._frames_fh is None:
                return
            for direction in ("c2s", "s2c"):
                st = self._seg.pop((conn, direction), None)
                if st is None:
                    continue
                for row in gwcap.flush_segmenter(st):
                    self._emit(row)

        def event(self, kind, **kw):
            if kind == "close" and "conn" in kw:
                self._flush_conn(kw["conn"])
            super().event(kind, **kw)

        def stream_bytes(self, conn, direction):
            return self.total.get((conn, direction), 0)

        def close(self):
            for key in list(self._seg):
                self._flush_conn(key[0])
            if self._frames_fh is not None:
                self._frames_fh.close()
                self._frames_fh = None
            super().close()

    class GateProxy(mir2_proxy.Proxy):
        pass

    return GateDump, GateProxy


def verify_capture(gwcap, out_dir, require_frames=True):
    """对账：chunks.ndjson 与 frames.ndjson 都必须能无损重建 .bin 字节流。

    返回错误列表（空 = GREEN）。红检对应用例：篡改 .bin / frames 的 sha 或 wire ⇒ 必红。
    这是本工具“抓包可信”的牙：没有它，frames.ndjson 只是自说自话。
    """
    errors = []
    chunks_path = os.path.join(out_dir, "chunks.ndjson")
    if not os.path.isfile(chunks_path):
        return ["缺 chunks.ndjson：%s" % chunks_path]
    events = []
    with open(chunks_path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                events.append(json.loads(line))
    data_evs = {}
    for ev in events:
        if ev.get("event") == "data":
            data_evs.setdefault((ev["conn"], ev["dir"]), []).append(ev)

    bins = {}
    for fname in sorted(os.listdir(out_dir)):
        if not (fname.startswith("conn-") and fname.endswith(".bin")):
            continue
        parts = fname.split(".")
        conn = int(parts[0].split("-")[1])
        direction = parts[1]
        with open(os.path.join(out_dir, fname), "rb") as fh:
            bins[(conn, direction)] = fh.read()

    # chunks 里每个 data 方向都必须在 .bin 里有对应真值；否则删掉整条方向的
    # .bin 与 frames 仍会「无处可对账」而假绿。
    for key in sorted(data_evs):
        if key not in bins:
            errors.append(
                "%s 有 chunks data 事件但缺 .bin（off=%d..%d）"
                % (key, data_evs[key][0]["off"],
                   max(e["off"] + e["n"] for e in data_evs[key]))
            )

    for key, stream in sorted(bins.items()):
        cursor = 0
        for ev in sorted(data_evs.get(key, []), key=lambda r: r["off"]):
            if ev["off"] != cursor:
                errors.append(
                    "%s 偏移断裂：期望 %d，chunks 记录 %d" % (key, cursor, ev["off"])
                )
                break
            chunk = stream[ev["off"]:ev["off"] + ev["n"]]
            if len(chunk) != ev["n"] or gwcap.sha256_bytes(chunk) != ev["sha256"]:
                errors.append("%s off=%d 块 hash/长度不符" % (key, ev["off"]))
                break
            cursor = ev["off"] + ev["n"]
        if cursor != len(stream):
            errors.append("%s 未被 chunks 完全覆盖（%d/%d）" % (key, cursor, len(stream)))

    frames_path = os.path.join(out_dir, "frames.ndjson")
    if not os.path.isfile(frames_path):
        if require_frames:
            errors.append("缺 frames.ndjson：%s" % frames_path)
        return errors
    groups = {}
    with open(frames_path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            groups.setdefault((row["conn"], row["dir"]), []).append(row)
    for key, rows in sorted(groups.items()):
        stream = bins.get(key, b"")
        cursor = 0
        last_t = None
        for row in sorted(rows, key=lambda r: r["seq"]):
            if row["off"] != cursor:
                errors.append(
                    "frames %s 偏移断裂：期望 %d，记录 %d" % (key, cursor, row["off"])
                )
                break
            blob = stream[row["off"]:row["off"] + row["len"]]
            if len(blob) != row["len"] or gwcap.sha256_bytes(blob) != row["sha256"]:
                errors.append("frames %s off=%d 帧 hash/长度不符" % (key, row["off"]))
                break
            want = gwcap.wire_shape(row["dir"], blob)
            if row.get("wire") != want:
                errors.append(
                    "frames %s off=%d wire 记录 %r != 实算 %r"
                    % (key, row["off"], row.get("wire"), want)
                )
                break
            want_ident = gwcap.frame_ident(row["dir"], blob)
            if row.get("ident") != want_ident:
                errors.append(
                    "frames %s off=%d ident 记录 %r != 实算 %r"
                    % (key, row["off"], row.get("ident"), want_ident)
                )
                break
            if last_t is not None and row.get("t_wall_ms") is not None:
                if row["t_wall_ms"] < last_t:
                    errors.append("frames %s 时间戳倒退（off=%d）" % (key, row["off"]))
                    break
            if row.get("t_wall_ms") is not None:
                last_t = row["t_wall_ms"]
            cursor = row["off"] + row["len"]
        if cursor != len(stream):
            errors.append(
                "frames %s 未覆盖整条流（%d/%d）" % (key, cursor, len(stream))
            )
    for key in bins:
        if key not in groups:
            errors.append("frames 缺方向 %s（有 .bin 无帧记录）" % (key,))
    return errors


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="LoginGate/SelGate 影子 TCP 抓包代理（C10#4；与 mir2_proxy 同 capture 格式）"
    )
    ap.add_argument("--out", help="输出目录（含 session/chunks/frames/conn-*.bin）")
    ap.add_argument("--map", action="append", default=[],
                    help="listen_port=upstream_host:upstream_port，可多次（影子端口→真网关）")
    ap.add_argument("--bind", default="127.0.0.1", help="监听地址（默认 127.0.0.1）")
    ap.add_argument("--direction", choices=("both", "c2s", "s2c"), default="both",
                    help="只落盘一个方向（另一方向仍透传；默认 both）")
    ap.add_argument("--gate", default="auto",
                    choices=("auto", "login", "sel", "sel-downstream", "other"),
                    help="网关标签；auto 按上游端口推断（7000=login / 7100=sel / 5100=sel-downstream）")
    ap.add_argument("--no-frames", action="store_true", help="不写 frames.ndjson（只留 chunks/.bin）")
    ap.add_argument("--verify", metavar="DIR", help="对既有抓包目录做 chunks/frames 无损对账后退出")
    ap.add_argument("--selftest", action="store_true", help="跑本工具的本地假 socket 自测后退出")
    args = ap.parse_args(argv)

    if args.selftest:
        selftest = _load(
            "_gwcap_selftest", os.path.join(HERE, "selftest_gate_shadow.py")
        )
        return selftest.main()

    mir2_proxy, gwcap = _deps()
    if args.verify:
        try:
            errors = verify_capture(gwcap, args.verify)
        except ValueError as exc:
            print("PREREQ_MISSING: 抓包文件半截/无法解析：%s" % exc, file=sys.stderr)
            return 2
        if errors:
            for err in errors:
                print("RED: " + err)
            return 1
        print("GREEN: capture 对账通过（chunks 与 frames 均可无损重建 .bin，ident 实算一致）")
        return 0

    if not args.out or not args.map:
        print("RED: 需要 --out 与至少一个 --map（或 --verify DIR / --selftest）", file=sys.stderr)
        return 1
    try:
        port_map = {}
        for spec in args.map:
            listen, upstream = mir2_proxy.parse_map(spec)
            port_map[listen] = upstream
    except ValueError as exc:
        print("RED: %s" % exc, file=sys.stderr)
        return 1

    port_meta = {}
    for listen, (host, up_port) in port_map.items():
        port_meta[listen] = {
            "upstream": "%s:%d" % (host, up_port),
            "upstream_port": up_port,
            "gate": infer_gate(up_port, args.gate),
        }
    GateDump, GateProxy = make_classes(mir2_proxy, gwcap)
    dump = GateDump(
        args.out, direction=args.direction,
        frames=not args.no_frames, port_meta=port_meta,
    )
    with open(os.path.join(args.out, "session.json"), "w", encoding="utf-8") as fh:
        json.dump({
            "tool": "gate_shadow_proxy",
            "format": "mir2_proxy-compatible-v1",
            "argv": sys.argv if argv is None else ["gate_shadow_proxy.py"] + list(argv),
            "start_wall_ms": dump.t0_wall_ms,
            "start_iso": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime()),
            "bind": args.bind,
            "map": {str(k): f"{v[0]}:{v[1]}" for k, v in sorted(port_map.items())},
            "port_meta": {str(k): v for k, v in sorted(port_meta.items())},
            "direction": args.direction,
            "frames": not args.no_frames,
            "pid": os.getpid(),
        }, fh, ensure_ascii=False, indent=2)

    proxy = GateProxy(args.bind, port_map, dump, False)
    try:
        import asyncio
        asyncio.run(proxy.run())
    except KeyboardInterrupt:
        pass
    finally:
        dump.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
