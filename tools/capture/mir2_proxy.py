#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""mir2_proxy.py — Mir2 抓包代理：TCP 透传 + 原始字节 dump + 时间戳。

在客户端与服务端之间透传字节流，按连接分别落盘：
  <out>/session.json          会话元信息（端口映射、起始墙钟、启动命令）
  <out>/chunks.ndjson         逐块时间线（open/close/data 事件，data 带方向内偏移与 sha256）
  <out>/conn-NNNN.c2s.bin     客户端→服务端 原始字节流（字节真值）
  <out>/conn-NNNN.s2c.bin     服务端→客户端 原始字节流（字节真值）

用法：
  python mir2_proxy.py --out DIR --map 7000=127.0.0.1:17000 [--map 7100=127.0.0.1:17100 ...]
                       [--bind 127.0.0.1] [--spin-source-ip]

--spin-source-ip：上行连接轮流绑定 127.0.0.2~127.0.0.250 源地址，
  用于假人压测时绕开网关「单 IP 连接数」类限制（对抓包无影响）。
"""
import argparse
import asyncio
import hashlib
import json
import os
import sys
import time


class Dump:
    """单会话落盘器：chunks.ndjson + 每连接双向 .bin。"""

    def __init__(self, out_dir: str):
        self.out_dir = out_dir
        os.makedirs(out_dir, exist_ok=True)
        self._ndjson = open(os.path.join(out_dir, "chunks.ndjson"), "w", encoding="utf-8", buffering=1)
        self._t0_perf = time.perf_counter_ns()
        self.t0_wall_ms = int(time.time() * 1000)
        self._streams = {}  # (conn, dir) -> {"fh": file, "off": int, "n": int}

    def rel_us(self) -> int:
        return (time.perf_counter_ns() - self._t0_perf) // 1000

    def _row(self, row: dict):
        row.setdefault("t_wall_ms", int(time.time() * 1000))
        row.setdefault("t_rel_us", self.rel_us())
        self._ndjson.write(json.dumps(row, ensure_ascii=False) + "\n")

    def event(self, kind: str, **kw):
        self._row({"event": kind, **kw})

    def data(self, conn: int, port: int, direction: str, buf: bytes):
        key = (conn, direction)
        st = self._streams.get(key)
        if st is None:
            fh = open(os.path.join(self.out_dir, f"conn-{conn:04d}.{direction}.bin"), "wb")
            st = {"fh": fh, "off": 0, "n": 0}
            self._streams[key] = st
        st["fh"].write(buf)
        st["fh"].flush()
        self._row({
            "event": "data", "conn": conn, "port": port, "dir": direction,
            "off": st["off"], "n": len(buf),
            "sha256": hashlib.sha256(buf).hexdigest(),
        })
        st["off"] += len(buf)
        st["n"] += len(buf)

    def stream_bytes(self, conn: int, direction: str) -> int:
        st = self._streams.get((conn, direction))
        return st["n"] if st else 0

    def close(self):
        for st in self._streams.values():
            st["fh"].close()
        self._ndjson.close()


class Proxy:
    def __init__(self, bind: str, port_map: dict, dump: Dump, spin_source_ip: bool):
        self.bind = bind
        self.port_map = port_map  # listen_port -> (host, port)
        self.dump = dump
        self.spin = spin_source_ip
        self._spin_next = 2
        self._conn_seq = 0
        self._servers = []

    def _next_source_ip(self) -> str:
        ip = f"127.0.0.{self._spin_next}"
        self._spin_next += 1
        if self._spin_next > 250:
            self._spin_next = 2
        return ip

    async def _pipe(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter,
                    conn: int, port: int, direction: str):
        try:
            while True:
                buf = await reader.read(65536)
                if not buf:
                    break
                self.dump.data(conn, port, direction, buf)
                writer.write(buf)
                await writer.drain()
        except (ConnectionResetError, BrokenPipeError, ConnectionAbortedError):
            pass
        except Exception as ex:  # 透传层不吞未知异常：记录后继续关
            self.dump.event("pipe_error", conn=conn, port=port, dir=direction, error=repr(ex))
        finally:
            try:
                writer.close()
            except Exception:
                pass

    async def _handle(self, listen_port: int, client_reader: asyncio.StreamReader,
                      client_writer: asyncio.StreamWriter):
        self._conn_seq += 1
        conn = self._conn_seq
        peer = client_writer.get_extra_info("peername")
        up_host, up_port = self.port_map[listen_port]
        self.dump.event("open", conn=conn, port=listen_port, peer=f"{peer[0]}:{peer[1]}",
                        upstream=f"{up_host}:{up_port}")
        try:
            local_addr = (self._next_source_ip(), 0) if self.spin else None
            up_reader, up_writer = await asyncio.open_connection(
                up_host, up_port, local_addr=local_addr)
        except Exception as ex:
            self.dump.event("upstream_fail", conn=conn, port=listen_port, error=repr(ex))
            client_writer.close()
            return
        t1 = asyncio.ensure_future(self._pipe(client_reader, up_writer, conn, listen_port, "c2s"))
        t2 = asyncio.ensure_future(self._pipe(up_reader, client_writer, conn, listen_port, "s2c"))
        await asyncio.wait({t1, t2}, return_when=asyncio.ALL_COMPLETED)
        self.dump.event("close", conn=conn, port=listen_port,
                        bytes_c2s=self.dump.stream_bytes(conn, "c2s"),
                        bytes_s2c=self.dump.stream_bytes(conn, "s2c"))

    async def run(self):
        for listen_port in sorted(self.port_map):
            async def handler(r, w, p=listen_port):
                await self._handle(p, r, w)
            srv = await asyncio.start_server(handler, self.bind, listen_port)
            self._servers.append(srv)
            up = self.port_map[listen_port]
            print(f"LISTEN {self.bind}:{listen_port} -> {up[0]}:{up[1]}", flush=True)
        # 就绪标记：harness 靠它判断可以开始发流量（避免用探测连接污染抓包）
        with open(os.path.join(self.dump.out_dir, "ready.flag"), "w") as f:
            f.write(str(os.getpid()))
        print("READY", flush=True)
        await asyncio.gather(*(s.serve_forever() for s in self._servers))


def parse_map(spec: str):
    listen, _, target = spec.partition("=")
    host, _, port = target.rpartition(":")
    if not listen or not host or not port:
        raise ValueError(f"--map 格式应为 listen=host:port，收到: {spec!r}")
    return int(listen), (host, int(port))


def main():
    ap = argparse.ArgumentParser(description="Mir2 抓包代理（TCP 透传 + dump）")
    ap.add_argument("--out", required=True, help="输出目录")
    ap.add_argument("--map", action="append", required=True,
                    help="listen_port=upstream_host:upstream_port，可多次")
    ap.add_argument("--bind", default="127.0.0.1", help="监听地址（默认 127.0.0.1）")
    ap.add_argument("--spin-source-ip", action="store_true",
                    help="上行连接轮流绑定 127.0.0.2~250（压测绕开单 IP 限制）")
    args = ap.parse_args()

    port_map = dict(parse_map(s) for s in args.map)
    dump = Dump(args.out)
    with open(os.path.join(args.out, "session.json"), "w", encoding="utf-8") as f:
        json.dump({
            "tool": "mir2_proxy",
            "argv": sys.argv,
            "start_wall_ms": dump.t0_wall_ms,
            "start_iso": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime()),
            "bind": args.bind,
            "map": {str(k): f"{v[0]}:{v[1]}" for k, v in sorted(port_map.items())},
            "spin_source_ip": args.spin_source_ip,
            "pid": os.getpid(),
        }, f, ensure_ascii=False, indent=2)

    proxy = Proxy(args.bind, port_map, dump, args.spin_source_ip)
    try:
        asyncio.run(proxy.run())
    except KeyboardInterrupt:
        pass
    finally:
        dump.close()


if __name__ == "__main__":
    main()
