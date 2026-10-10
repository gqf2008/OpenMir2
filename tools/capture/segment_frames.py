#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""segment_frames.py — 把 mir2_proxy 的原始字节流按 Mir2 帧界切分，产出金标准帧表。

帧界规则（按 C# 网关源码核对，LoginGate/Services/ClientSession.cs:197、
GameGate/Services/ClientSession.cs:529-535）：
  S→C（服务端下行）：'#' + 编码负载 + '!'
  C→S（客户端上行）：编码负载 + '!'
切分策略对两方向一律按 '!' 收尾切帧，并记录每帧是否带 '#' 头——
不依赖任何解码逻辑（解码是 A 线协议层的活），字节原样保留。

产出（写入 --out 目录，默认与 session 同目录）：
  frames.ndjson   每帧一行：seq/conn/port/dir/off/len/sha256/ascii（可打印时）
  manifest.json   金标准清单：帧数、按方向/端口计数、逐帧 hash 的总 hash、
                  每条流的字节数与 hash、复现命令

用法：python segment_frames.py --session DIR [--out DIR] [--note "基线全程"]
"""
import argparse
import glob
import hashlib
import json
import os
import re
import sys


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def ascii_preview(buf: bytes, limit: int = 512) -> str:
    if all(0x20 <= c < 0x7F for c in buf):
        s = buf.decode("ascii")
        return s if len(s) <= limit else s[:limit] + "…"
    return ""


def load_conn_ports(session_dir: str) -> dict:
    """chunks.ndjson 的 open 事件 → conn -> (listen_port, peer, upstream, t_wall_ms)。"""
    ports = {}
    path = os.path.join(session_dir, "chunks.ndjson")
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            row = json.loads(line)
            if row.get("event") == "open":
                ports[row["conn"]] = {
                    "port": row["port"], "peer": row.get("peer", ""),
                    "upstream": row.get("upstream", ""), "t_open_wall_ms": row["t_wall_ms"],
                }
    return ports


def segment(stream: bytes):
    """按实测帧界切帧：'#' 开头、'!' 收尾（s2c 附带 '$'），裸 '*' 为心跳。
    返回 [(off, frame_bytes, kind)]，kind ∈ {"frame", "heartbeat", "partial"}。"""
    frames = []
    off = 0
    n = len(stream)
    while off < n:
        if stream[off] == 0x2A:  # '*' 心跳，单字节
            frames.append((off, stream[off:off + 1], "heartbeat"))
            off += 1
            continue
        idx = stream.find(b"!", off)
        if idx < 0:
            frames.append((off, stream[off:], "partial"))  # 尾包未闭合
            break
        end = idx + 1
        if end < n and stream[end] == 0x24:  # '$'
            end += 1
        frames.append((off, stream[off:end], "frame"))
        off = end
    return frames


def main():
    ap = argparse.ArgumentParser(description="Mir2 抓包帧切分 → 金标准帧表")
    ap.add_argument("--session", required=True, help="mir2_proxy 的输出目录")
    ap.add_argument("--out", default=None, help="frames/manifest 输出目录（默认同 session）")
    ap.add_argument("--note", default="", help="写入 manifest 的说明（如抓的是哪条流程）")
    ap.add_argument("--reproduce", default="", help="复现命令（写入 manifest）")
    args = ap.parse_args()

    out_dir = args.out or args.session
    os.makedirs(out_dir, exist_ok=True)
    conn_meta = load_conn_ports(args.session)

    bins = sorted(glob.glob(os.path.join(args.session, "conn-*.bin")))
    if not bins:
        print("ERROR: 没有找到 conn-*.bin，session 目录不对？", file=sys.stderr)
        sys.exit(2)

    frames_rows = []
    streams_meta = []
    seq = 0
    pat = re.compile(r"conn-(\d{4})\.(c2s|s2c)\.bin$")
    for path in bins:
        m = pat.search(os.path.basename(path))
        if not m:
            continue
        conn, direction = int(m.group(1)), m.group(2)
        with open(path, "rb") as f:
            data = f.read()
        meta = conn_meta.get(conn, {})
        streams_meta.append({
            "conn": conn, "dir": direction, "port": meta.get("port"),
            "peer": meta.get("peer", ""), "bytes": len(data), "sha256": sha256_bytes(data),
            "file": os.path.relpath(path, out_dir).replace("\\", "/"),
        })
        for off, frame, kind in segment(data):
            seq += 1
            frames_rows.append({
                "seq": seq, "conn": conn, "port": meta.get("port"), "dir": direction,
                "off": off, "len": len(frame), "sha256": sha256_bytes(frame),
                "kind": kind,
                "ascii": ascii_preview(frame),
            })

    with open(os.path.join(out_dir, "frames.ndjson"), "w", encoding="utf-8", newline="\n") as f:
        for row in frames_rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    by_dir = {}
    by_port = {}
    for r in frames_rows:
        by_dir[r["dir"]] = by_dir.get(r["dir"], 0) + 1
        key = str(r["port"])
        by_port[key] = by_port.get(key, 0) + 1

    session_json = {}
    sj = os.path.join(args.session, "session.json")
    if os.path.exists(sj):
        with open(sj, "r", encoding="utf-8") as f:
            session_json = json.load(f)

    manifest = {
        "tool": "segment_frames",
        "note": args.note,
        "session": session_json,
        "reproduce": args.reproduce,
        "total_frames": len(frames_rows),
        "by_direction": by_dir,
        "by_port": by_port,
        "unclosed_tail_frames": sum(1 for r in frames_rows if r["kind"] == "partial"),
        "heartbeats": sum(1 for r in frames_rows if r["kind"] == "heartbeat"),
        "frames_sha256": sha256_bytes("\n".join(r["sha256"] for r in frames_rows).encode()),
        "streams": streams_meta,
    }
    with open(os.path.join(out_dir, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)

    print(f"FRAMES={len(frames_rows)} by_dir={by_dir} by_port={by_port}")
    print(f"FRAMES_SHA256={manifest['frames_sha256']}")
    print(f"OUT={out_dir}")


if __name__ == "__main__":
    main()
