#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""selftest_gate_shadow.py — 影子抓包代理自测（改坏必红，全部本地假 socket）。

不绑定 7000/7100、不连接 E:/MirServer：上游是本地 echo/超时假网关（临时端口）。
四项断言，任一失败退出码 1：

  1. 透传保真：经代理的上行字节与下行字节逐字节等于假网关直连的期望；
  2. chunks 对账：chunks.ndjson 的 (dir, off, n, sha256) 能无损重建两条 .bin；
  3. frames 对账：frames.ndjson 逐帧覆盖整条流，wire/时间戳与实算一致，
     且能认出 LoginGate 16B 截断超时帧；
  4. 红检：篡改 frames 的 wire 或 .bin 一个字节 ⇒ `--verify` 必须变红；
     `--direction s2c` 只落盘下行，上行仍透传。

用法：python -I -B selftest_gate_shadow.py
"""

from __future__ import annotations

import asyncio
import importlib.util
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
PROXY = os.path.join(HERE, "gate_shadow_proxy.py")


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def make_header(ident):
    return (0).to_bytes(4, "little") + ident.to_bytes(2, "little") + b"\x00" * 6


def login_timeout_frame():
    ed = load_module(
        "_gwcap_selftest_ed",
        os.path.join(HERE, "..", "..", "mir2-rs", "tests", "parity", "loginsrv",
                     "oracle_capture.py"),
    )
    encoded = ed.edcode_encode(make_header(528))
    return b"#" + encoded[:-1]  # C# 只发 iLen：'#' + 15 个字符（16B）


async def fake_login_gate(reader, writer, timeout_frame):
    try:
        req = await reader.read(65536)
        if not req:
            return
        writer.write(req)  # echo
        await writer.drain()
        await asyncio.sleep(0.05)
        writer.write(timeout_frame)
        await writer.drain()
        await asyncio.sleep(0.1)
    except (ConnectionResetError, BrokenPipeError):
        pass
    finally:
        writer.close()


async def client_roundtrip(port, payload):
    reader, writer = await asyncio.open_connection("127.0.0.1", port)
    writer.write(payload)
    await writer.drain()
    got = b""
    while True:
        chunk = await reader.read(65536)
        if not chunk:
            break
        got += chunk
    writer.close()
    return got


def wait_ready(out_dir, proc, timeout=10.0):
    flag = os.path.join(out_dir, "ready.flag")
    deadline = time.time() + timeout
    while time.time() < deadline:
        if os.path.isfile(flag):
            return True
        if proc.poll() is not None:
            return False
        time.sleep(0.05)
    return False


def wait_close_event(out_dir, timeout=3.0):
    path = os.path.join(out_dir, "chunks.ndjson")
    deadline = time.time() + timeout
    while time.time() < deadline:
        if os.path.isfile(path):
            with open(path, "r", encoding="utf-8") as fh:
                for line in fh:
                    try:
                        row = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    if row.get("event") == "close":
                        return True
        time.sleep(0.05)
    return False


def _read_frames(out_dir):
    rows = []
    path = os.path.join(out_dir, "frames.ndjson")
    with open(path, "r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def _run_verify(out_dir):
    return subprocess.run(
        [sys.executable, "-I", "-B", PROXY, "--verify", out_dir],
        capture_output=True, text=True,
    )


def _run_proxy(out_dir, pairs, direction="both", gate="login"):
    cmd = [sys.executable, "-I", "-B", PROXY, "--out", out_dir,
           "--direction", direction, "--gate", gate]
    for listen, upstream in pairs:
        cmd += ["--map", "%d=127.0.0.1:%d" % (listen, upstream)]
    return subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)


def _stop(proc):
    if proc is None:
        return
    if proc.poll() is None:
        proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def main():
    tmp = tempfile.mkdtemp(prefix="gwcap-selftest-")
    loop = asyncio.new_event_loop()
    srv = None
    proxies = []
    rc = 1
    try:
        timeout_frame = login_timeout_frame()
        assert len(timeout_frame) == 16 and timeout_frame[:1] == b"#"
        up_port = free_port()

        async def handler(reader, writer):
            await fake_login_gate(reader, writer, timeout_frame)

        srv = loop.run_until_complete(asyncio.start_server(handler, "127.0.0.1", up_port))

        # --- 1/2/3: both 方向 ---
        out1 = os.path.join(tmp, "both")
        p1 = free_port()
        proc1 = _run_proxy(out1, [(p1, up_port)])
        proxies.append(proc1)
        if not wait_ready(out1, proc1):
            print("RED: 代理未就绪")
            return 1
        payload = b"#1HELLO-MIR2!"
        got = loop.run_until_complete(client_roundtrip(p1, payload))
        expect = payload + timeout_frame
        if got != expect:
            print("RED: 透传不保真：期望 %dB，收到 %dB / 内容不同" % (len(expect), len(got)))
            return 1
        print("PASS 1/4 透传逐字节保真（上下行）")

        if not wait_close_event(out1):
            print("RED: 未等到 close 事件（frames 不会 flush）")
            return 1
        _stop(proc1)
        proxies.remove(proc1)

        check = _run_verify(out1)
        if check.returncode != 0:
            print("RED: verify 未通过\n" + check.stdout + check.stderr)
            return 1
        rows = _read_frames(out1)
        s2c_timeout = [
            r for r in rows
            if r["dir"] == "s2c" and r["wire"] == "login-timeout-16"
        ]
        c2s_req = [
            r for r in rows
            if r["dir"] == "c2s" and r["wire"] == "c2s-hash1-bang"
        ]
        if not s2c_timeout or not c2s_req:
            print("RED: frames 缺 LoginGate 超时帧或上行帧：%r" % rows)
            return 1
        if any(r.get("gate") != "login" for r in rows):
            print("RED: 网关标签未标成 login（check --gate auto/upstream）")
            return 1
        if not all(isinstance(r.get("t_wall_ms"), int) for r in rows):
            print("RED: frames 缺整数 t_wall_ms")
            return 1
        print("PASS 2/4 chunks 可无损重建 .bin")
        print("PASS 3/4 frames 逐帧覆盖 + 认出 16B 截断超时帧")

        # --- 4a: 方向过滤 ---
        out2 = os.path.join(tmp, "s2c-only")
        p2 = free_port()
        proc2 = _run_proxy(out2, [(p2, up_port)], direction="s2c")
        proxies.append(proc2)
        if not wait_ready(out2, proc2):
            print("RED: 方向过滤代理未就绪")
            return 1
        got2 = loop.run_until_complete(client_roundtrip(p2, payload))
        if got2 != expect:
            print("RED: --direction s2c 时上行透传被破坏")
            return 1
        if not wait_close_event(out2):
            print("RED: 方向过滤未等到 close")
            return 1
        _stop(proc2)
        proxies.remove(proc2)
        if any(f.endswith(".c2s.bin") for f in os.listdir(out2)):
            print("RED: --direction s2c 仍落盘了 c2s .bin")
            return 1
        rows2 = _read_frames(out2)
        if any(r["dir"] != "s2c" for r in rows2):
            print("RED: --direction s2c 仍落盘了 c2s 帧")
            return 1
        if _run_verify(out2).returncode != 0:
            print("RED: 方向过滤 capture verify 失败")
            return 1

        # --- 4b: 红检（wire 篡改 / .bin 篡改）---
        victim = os.path.join(tmp, "tamper-wire")
        shutil.copytree(out1, victim)
        fpath = os.path.join(victim, "frames.ndjson")
        with open(fpath, "r", encoding="utf-8") as fh:
            lines = [json.loads(x) for x in fh if x.strip()]
        lines[0]["wire"] = "tampered-wire"
        with open(fpath, "w", encoding="utf-8") as fh:
            for row in lines:
                fh.write(json.dumps(row, ensure_ascii=False) + "\n")
        if _run_verify(victim).returncode == 0:
            print("RED: 篡改 frames.wire 后 verify 仍绿——红检失效")
            return 1

        victim2 = os.path.join(tmp, "tamper-bin")
        shutil.copytree(out1, victim2)
        bins = [f for f in os.listdir(victim2) if f.endswith(".bin")]
        if not bins:
            print("RED: 抓包没有 .bin")
            return 1
        bpath = os.path.join(victim2, sorted(bins)[0])
        with open(bpath, "r+b") as fh:
            first = bytearray(fh.read(1))
            first[0] ^= 0xFF
            fh.seek(0)
            fh.write(bytes(first))
        if _run_verify(victim2).returncode == 0:
            print("RED: 篡改 .bin 后 verify 仍绿——红检失效")
            return 1
        print("PASS 4/4 篡改 wire/.bin 后 verify 变红（改坏必红成立）")
        print("GREEN: gate shadow proxy selftest 4/4")
        rc = 0
        return rc
    finally:
        for proc in list(proxies):
            _stop(proc)
        if srv is not None:
            srv.close()
            try:
                loop.run_until_complete(srv.wait_closed())
                loop.run_until_complete(asyncio.sleep(0.05))
            except Exception:
                pass
        try:
            loop.close()
        except Exception:
            pass
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
