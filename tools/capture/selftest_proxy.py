#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""selftest_proxy.py — 抓包代理自测（改坏必红）。

三项断言，任何一项失败退出码 1：
  1. 透传保真：经代理发的字节与 echo 回来的字节必须逐字节等于直连结果；
  2. dump 复原：chunks.ndjson 记录的 (dir, off, n, sha256) 必须能无损重建两条流；
  3. 红检：故意篡改 conn-*.bin 一个字节后，verify 逻辑必须报 RED（此处用内置
     对账函数模拟，等价于 verify_golden.py 对金标准做的事）。

用法：python selftest_proxy.py
"""
import asyncio
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def sha256(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


async def echo_server(port: int):
    async def on_conn(r: asyncio.StreamReader, w: asyncio.StreamWriter):
        try:
            while True:
                data = await r.read(65536)
                if not data:
                    break
                w.write(data)
                await w.drain()
        except ConnectionResetError:
            pass
        finally:
            w.close()
    srv = await asyncio.start_server(on_conn, "127.0.0.1", port)
    return srv


async def exchange(port: int, payloads: list, read_chunk: int = 65536) -> bytes:
    """发 payloads，读回等长字节。"""
    r, w = await asyncio.open_connection("127.0.0.1", port)
    got = b""
    total = sum(len(p) for p in payloads)
    for p in payloads:
        w.write(p)
        await w.drain()
    while len(got) < total:
        chunk = await r.read(read_chunk)
        if not chunk:
            break
        got += chunk
    w.close()
    return got


def free_port() -> int:
    import socket
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def rebuild_streams(session_dir: str):
    """从 chunks.ndjson + .bin 对账：每条流的 data 事件必须无缝覆盖且 hash 相符。"""
    events = []
    with open(os.path.join(session_dir, "chunks.ndjson"), "r", encoding="utf-8") as f:
        for line in f:
            events.append(json.loads(line))
    ok = True
    for fname in os.listdir(session_dir):
        if not fname.startswith("conn-") or not fname.endswith(".bin"):
            continue
        conn = int(fname.split("-")[1].split(".")[0])
        direction = fname.rsplit(".", 2)[1]
        with open(os.path.join(session_dir, fname), "rb") as f:
            stream = f.read()
        cursor = 0
        for ev in events:
            if ev.get("event") != "data" or ev.get("conn") != conn or ev.get("dir") != direction:
                continue
            if ev["off"] != cursor:
                print(f"  RED: {fname} 第 {cursor} 字节处偏移断裂 (off={ev['off']})")
                ok = False
                break
            chunk = stream[ev["off"]:ev["off"] + ev["n"]]
            if len(chunk) != ev["n"] or sha256(chunk) != ev["sha256"]:
                print(f"  RED: {fname} off={ev['off']} 块 hash/长度不符")
                ok = False
                break
            cursor = ev["off"] + ev["n"]
        if ok and cursor != len(stream):
            print(f"  RED: {fname} 流未被事件完全覆盖 ({cursor}/{len(stream)})")
            ok = False
    return ok


def main():
    tmp = tempfile.mkdtemp(prefix="mir2proxy-selftest-")
    echo_port = free_port()
    proxy_port = free_port()
    session_dir = os.path.join(tmp, "sess")
    proxy = None
    rc = 1
    try:
        # 1. 起 echo + 代理
        loop = asyncio.new_event_loop()
        srv = loop.run_until_complete(echo_server(echo_port))
        proxy = subprocess.Popen(
            [sys.executable, os.path.join(HERE, "mir2_proxy.py"),
             "--out", session_dir, "--map", f"{proxy_port}=127.0.0.1:{echo_port}"],
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        deadline = time.time() + 10
        ready = False
        first_lines = []
        while time.time() < deadline:
            line = proxy.stdout.readline()
            if not line:
                break
            first_lines.append(line.rstrip())
            if line.strip() == "READY":
                ready = True
                break
        if not ready:
            print("RED: 代理未就绪:", first_lines)
            return 1

        # 2. 直连 echo 取参照；再经代理取被测
        payloads = [bytes(range(256)) * 3, b"Mir2!" * 100, b"\x00\xff" * 4097]
        expect = loop.run_until_complete(exchange(echo_port, payloads))
        got = loop.run_until_complete(exchange(proxy_port, payloads))
        if expect != b"".join(payloads):
            print("RED: echo 参照自身不保真（测试环境坏了）")
            return 1
        if got != expect:
            print(f"RED: 透传不保真: 直连 {len(expect)}B vs 经代理 {len(got)}B 或内容不同")
            return 1
        print("PASS 1/3 透传逐字节保真")

        # 等代理把 close 事件落盘
        time.sleep(0.5)

        # 3. dump 复原对账
        if not rebuild_streams(session_dir):
            print("RED: chunks.ndjson 无法无损重建流")
            return 1
        print("PASS 2/3 dump 事件可无损重建字节流")

        # 4. 红检：篡改 .bin 一个字节，对账必须变红
        victim = None
        for fname in os.listdir(session_dir):
            if fname.endswith(".bin") and os.path.getsize(os.path.join(session_dir, fname)) > 0:
                victim = os.path.join(session_dir, fname)
                break
        if victim is None:
            print("RED: 自测没抓到任何流文件（代理没 dump？）")
            return 1
        with open(victim, "r+b") as f:
            b = bytearray(f.read(1))
            b[0] ^= 0xFF
            f.seek(0)
            f.write(bytes(b))
        if rebuild_streams(session_dir):
            print("RED: 篡改 .bin 后对账仍然绿——红检失效")
            return 1
        print("PASS 3/3 篡改 .bin 后对账变红（改坏必红成立）")

        print("GREEN: proxy selftest 3/3")
        rc = 0
        return rc
    finally:
        if proxy is not None:
            proxy.terminate()
            try:
                proxy.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proxy.kill()
        try:
            srv.close()
            loop.run_until_complete(srv.wait_closed())
            loop.run_until_complete(asyncio.sleep(0.2))
            loop.close()
        except Exception:
            pass
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
