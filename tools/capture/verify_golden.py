#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""verify_golden.py — 金标准完整性校验（改坏必红的判官）。

对照 manifest.json 重新计算：
  1. 每条 conn-*.bin 流的字节数与 sha256；
  2. frames.ndjson 每帧行内字段自洽（off/len 连续覆盖流、sha256 与流内容一致）；
  3. 帧数、按方向/端口计数、frames_sha256 总 hash 与 manifest 一致。

任何一项不符 → 退出码 1 并打印第一个差异（金标准被改坏、缺文件、截断都会红）。

用法：python verify_golden.py --golden tests/golden/<session>
"""
import argparse
import hashlib
import json
import os
import sys


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def fail(msg: str):
    print(f"RED: {msg}")
    sys.exit(1)


def main():
    ap = argparse.ArgumentParser(description="金标准完整性校验")
    ap.add_argument("--golden", required=True, help="金标准目录（含 manifest.json）")
    args = ap.parse_args()
    g = args.golden

    mpath = os.path.join(g, "manifest.json")
    if not os.path.exists(mpath):
        fail(f"缺 manifest.json: {mpath}")
    with open(mpath, "r", encoding="utf-8") as f:
        manifest = json.load(f)

    # 1. 流文件对账
    streams = {}
    for st in manifest.get("streams", []):
        fname = st.get("file") or f"conn-{st['conn']:04d}.{st['dir']}.bin"
        path = os.path.join(g, fname)
        if not os.path.exists(path):
            fail(f"缺流文件 {fname}")
        with open(path, "rb") as f:
            data = f.read()
        if len(data) != st["bytes"]:
            fail(f"{fname} 字节数不符: 实际 {len(data)} != 清单 {st['bytes']}")
        h = sha256_bytes(data)
        if h != st["sha256"]:
            fail(f"{fname} hash 不符: 实际 {h[:16]}… != 清单 {st['sha256'][:16]}…")
        streams[(st["conn"], st["dir"])] = data

    # 2. 帧表逐行对账（帧必须连续无缝覆盖所属流）
    fpath = os.path.join(g, "frames.ndjson")
    if not os.path.exists(fpath):
        fail("缺 frames.ndjson")
    rows = []
    with open(fpath, "r", encoding="utf-8") as f:
        for line in f:
            rows.append(json.loads(line))
    if len(rows) != manifest["total_frames"]:
        fail(f"帧数不符: frames.ndjson {len(rows)} 行 != manifest {manifest['total_frames']}")
    cursor = {}
    for i, r in enumerate(rows):
        if r["seq"] != i + 1:
            fail(f"第 {i+1} 行 seq 断裂: {r['seq']}")
        key = (r["conn"], r["dir"])
        if key not in streams:
            fail(f"帧 {r['seq']} 引用了清单外的流 conn={r['conn']} {r['dir']}")
        expect_off = cursor.get(key, 0)
        if r["off"] != expect_off:
            fail(f"帧 {r['seq']} 偏移断裂: off={r['off']} != 期望 {expect_off}（流 conn={r['conn']} {r['dir']}）")
        data = streams[key]
        frame = data[r["off"]:r["off"] + r["len"]]
        if len(frame) != r["len"]:
            fail(f"帧 {r['seq']} 越界: off+len 超出流长")
        if sha256_bytes(frame) != r["sha256"]:
            fail(f"帧 {r['seq']} hash 不符（帧表或流被改）")
        if r["kind"] == "frame" and not (frame.endswith(b"!") or frame.endswith(b"$")):
            fail(f"帧 {r['seq']} 是闭合帧但不以 '!'/'$' 收尾")
        if r["kind"] == "heartbeat" and frame != b"*":
            fail(f"帧 {r['seq']} 标记 heartbeat 但不是单字节 '*'")
        cursor[key] = r["off"] + r["len"]
    for key, end in cursor.items():
        if end != len(streams[key]):
            fail(f"流 conn={key[0]} {key[1]} 未被帧表完全覆盖: 覆盖到 {end} / 总长 {len(streams[key])}")

    # 3. 总计数与总 hash
    by_dir = {}
    by_port = {}
    for r in rows:
        by_dir[r["dir"]] = by_dir.get(r["dir"], 0) + 1
        by_port[str(r["port"])] = by_port.get(str(r["port"]), 0) + 1
    if by_dir != manifest["by_direction"]:
        fail(f"by_direction 不符: {by_dir} != {manifest['by_direction']}")
    if by_port != manifest["by_port"]:
        fail(f"by_port 不符: {by_port} != {manifest['by_port']}")
    total = sha256_bytes("\n".join(r["sha256"] for r in rows).encode())
    if total != manifest["frames_sha256"]:
        fail(f"frames_sha256 总 hash 不符: {total[:16]}… != {manifest['frames_sha256'][:16]}…")

    print(f"GREEN: {len(rows)} 帧 / {len(streams)} 流全部对账一致 ({g})")
    sys.exit(0)


if __name__ == "__main__":
    main()
