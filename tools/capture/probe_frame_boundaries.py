#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""probe_frame_boundaries.py — 帧边界与多段体取证（C 线工具，A-8 用）。

用途：当 replay 报「字节回放不符」时，判断该帧是**真实线上帧**还是**抓包侧切帧产物**，
并给出多段体候选的证据。三类检查：

1. **边界核验**：每帧是否 `#` 开头、`!`（或 `!$`）收尾，且帧内 `!` 是否只出现在结尾；
   再用"`!` 后必须是 `#`/`$`/EOF 才算帧尾"的替代规则重切一遍，比较帧数与长度分布。
   两者一致 ⇒ 切帧规则不是差异来源（差异只能在协议/编码侧）。
2. **多段体候选**：EDCode 单流编码的可产长度是 `n + ceil(n/3)`（12B 头 → 16 字符），
   所以 s2c 帧的**载荷长度 ≡1 (mod 4)** 且不是心跳时，单流编码产不出来 ⇒ 该帧体很可能是
   "多段分别编码后拼接"。注意 c2s 帧字面量 `#1` 前缀不属于编码体，量长度时要剥掉。

   **反例（务必保留，别删）**：口径不写死就会重新分叉 —— 2026-10-10 我就是用一版"没剥 `#1`"的
   临时脚本先跑出 **78** 条候选，把 38 条 c2s 单段帧（3010/3011/3014/1018/81/84/10431/10433，
   12B 头无体、编码体恰好 16 字符 ≡0）误判成多段体；换成正确口径后是 **40** 条（全 s2c：
   9 条 ident=10 + 31 条 `#+…!` 动作帧）。A 线侧的正确计数是 **33**（他们本来就按 `[2..^1]`/`[1..^1]` 剥）。
   ⇒ 见到 78 这个数就是又把 `#1` 计进去了。
3. **短帧明细**：列出"无 12B 头"的短帧（如 13 字符载荷 → 9 字节明文），给出明文 hex，
   便于按周期/内容去 C# 侧找发送点（例如网关的"小包"直发路径）。

用法：
  python probe_frame_boundaries.py --session <会话目录> [--dump-plain 的产物]

输入：会话目录需含 `capture.jsonl`（GoldenExport 产物）与 `proxy/conn-*.bin`。
"""
import argparse
import json
import os
from collections import Counter


def load_capture(session):
    path = os.path.join(session, "capture.jsonl")
    if not os.path.exists(path):
        raise SystemExit("缺 capture.jsonl（先跑 GoldenExport）：" + path)
    rows = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def encoded_len(raw: bytes, direction: str) -> int:
    """剥掉定界符后，**编码体**的长度（c2s 还有字面量 '1'，不属于编码体）。"""
    b = raw
    if b[:1] == b"#":
        b = b[1:]
    if direction == "c2s" and b[:1] == b"1":
        b = b[1:]
    while b[-1:] in (b"!", b"$"):
        b = b[:-1]
    return len(b)


def segment_strict(s: bytes):
    """'!' 只有其后是 '#'/'$'/EOF 才算帧尾；裸 '*' 为心跳。"""
    out, i, n = [], 0, len(s)
    while i < n:
        if s[i] == 0x2A:
            out.append((i, 1))
            i += 1
            continue
        start = i
        j = s.find(b"!", i)
        while j != -1 and s[j + 1:j + 2] not in (b"", b"#", b"$"):
            j = s.find(b"!", j + 1)
        if j == -1:
            out.append((start, n - start))
            break
        end = j + 1
        if s[end:end + 1] == b"$":
            end += 1
        out.append((start, end - start))
        i = end
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--session", required=True)
    ap.add_argument("--plain", default="", help="可选：GoldenExport --dump-plain 的产物，用于打印明文")
    args = ap.parse_args()

    rows = [r for r in load_capture(args.session) if r.get("kind") == "frame"]
    print("帧总数:", len(rows))

    # 1) 边界核验
    print("\n== 1. 边界核验（每帧 '!' 只应在结尾；替代规则重切应得到同样的切分）==")
    multi_bang = []
    for r in rows:
        raw = bytes.fromhex(r["raw_hex"])
        if raw[:1] != b"#":
            print("  异常：不以 '#' 开头 seq=%d %r" % (r["seq"], raw[:12]))
        if raw.count(b"!") > 1:
            multi_bang.append(r["seq"])
    print("  帧内含多个 '!' 的帧数:", len(multi_bang), multi_bang[:8])
    by_conn_dir = {}
    for r in rows:
        by_conn_dir.setdefault((r["conn"], r["dir"]), []).append(r)
    proxy_dir = os.path.join(args.session, "proxy")
    for (conn, direction), fr in sorted(by_conn_dir.items()):
        p = os.path.join(proxy_dir, "conn-%.4d.%s.bin" % (conn, direction))
        if not os.path.exists(p):
            continue
        s2 = open(p, "rb").read()
        old_lens = [len(bytes.fromhex(r["raw_hex"])) for r in fr]
        new_lens = [ln for _, ln in segment_strict(s2)]
        print("  conn-%.4d.%s: 旧规则 %d 帧 / 新规则 %d 帧 / 一致=%s" % (
            conn, direction, len(old_lens), len(new_lens), old_lens == new_lens))

    # 2) 多段体候选
    print("\n== 2. 多段体候选（编码体长度 ≡1 (mod 4)，单流编码产不出来）==")
    cand = []
    for r in rows:
        if r.get("string_frame"):
            continue
        raw = bytes.fromhex(r["raw_hex"])
        el = encoded_len(raw, r["dir"])
        if el % 4 == 1:
            cand.append((r, el))
    print("  候选帧数:", len(cand), " 按方向:", Counter(r["dir"] for r, _ in cand))
    print("  按 ident:", Counter(str(r.get("ident")) for r, _ in cand).most_common(8))
    print("  编码体长度分布:", Counter(el for _, el in cand).most_common(8))

    # 3) 短帧明细（无 12B 头）
    print("\n== 3. 短帧明细（编码体解码后 < 12B ⇒ 无 CommandMessage 头）==")
    shorts = [(r, el) for r, el in cand if el <= 17]
    print("  短帧数:", len(shorts))
    for r, el in shorts[:8]:
        raw = bytes.fromhex(r["raw_hex"])
        print("    seq=%-4d %s hop=%-5s ident=%-6s 编码体=%-3d raw=%r" % (
            r["seq"], r["dir"], r.get("hop"), r.get("ident"), el, raw))

    if args.plain and os.path.exists(args.plain):
        print("\n== 明文（来自 --dump-plain 产物，按 seq 对齐）==")
        want = {r["seq"] for r, _ in shorts[:8]}
        with open(args.plain, "r", encoding="utf-8", errors="replace") as f:
            pending = None
            for line in f:
                if line.startswith("  seq="):
                    pending = line.rstrip()
                elif pending and line.strip().startswith("text="):
                    import re
                    m = re.search(r"seq=(\d+)", pending)
                    if m and int(m.group(1)) in want:
                        print("  " + pending.strip()[:110])
                        print("    " + line.strip()[:110])
                    pending = None


if __name__ == "__main__":
    main()
