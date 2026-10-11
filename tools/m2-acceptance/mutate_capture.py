#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""mutate_capture.py — M2 验收③「判红/判绿自检」的夹具生成器（**只用于红检，不参与验收**）。

从一份既有 `capture.jsonl`（字段口径同金标准）派生出**故意改坏**的一份，用来证明
`s2c_diff.py` 的判据有牙齿：

  --mutate tail   在前 12B 命令头**之外的尾**上翻 1 个字节 ⇒ `s2c_diff.py` 必须判红（exit 1）
                  （body_len/body_sha256 也随之不同；只允许归一化 recog，尾字节绝不许被吸掉）
  --mutate recog  只把解码后 12B 头里的 `Recog`（=oracle `ActorId`，会话相关随机正数）改掉、
                  再原样 EDCode 重编码 ⇒ `s2c_diff.py` 必须仍判绿（exit 0，归一化只吸 recog）

不新写 EDCode：复用既有 `mir2-rs/tests/parity/loginsrv/oracle_capture.py` 的实现（importlib）。

用法：
  python mutate_capture.py --in <in.jsonl> --out <out.jsonl> --ident 51 --mutate tail
  python mutate_capture.py --in <in.jsonl> --out <out.jsonl> --ident 51 --mutate recog

退出码：0 = 已落派生件；2 = 前置未满足（找不到输入 / 找不到可改的帧）。
"""
import argparse
import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
_EDCODE_PATH = os.path.join(REPO, "mir2-rs", "tests", "parity", "loginsrv", "oracle_capture.py")


def _load_edcode():
    if not os.path.exists(_EDCODE_PATH):
        print("PREREQ_MISSING: 找不到既有 EDCode 实现 %s" % _EDCODE_PATH, file=sys.stderr)
        sys.exit(2)
    spec = importlib.util.spec_from_file_location("_mutate_edcode", _EDCODE_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _split(raw: bytes):
    """`#` + Encode(头12B) + 尾。返回 (前缀 '#', 16B 编码头, 尾字节)。"""
    if len(raw) < 18 or raw[0:1] != b"#":
        raise ValueError("不是 #+Encode(头) 的 s2c 帧")
    if raw[-2:] == b"!$":
        tail = b"!$"
    elif raw[-1:] == b"!":
        tail = b"!"
    else:
        raise ValueError("帧尾不是 !/!$")
    body = raw[1 : len(raw) - len(tail)]
    if len(body) < 16:
        raise ValueError("body 短于 16B 编码头")
    return body[:16], body[16:], tail


def mutate_tail(raw_hex: str) -> str:
    raw = bytearray(bytes.fromhex(raw_hex))
    tail_len = 2 if raw[-2:] == b"!$" else 1
    i = len(raw) - tail_len - 1  # 尾的最后一个字节（非帧尾 !）
    raw[i] ^= 0x01
    return bytes(raw).hex()


def mutate_recog(raw_hex: str, ed) -> str:
    raw = bytes.fromhex(raw_hex)
    enc_head, body, tail = _split(raw)
    head = bytearray(ed.edcode_decode(enc_head))
    head[0:4] = ((int.from_bytes(head[0:4], "little", signed=True) ^ 0x01010101) & 0xFFFFFFFF).to_bytes(
        4, "little", signed=False
    )
    out = b"#" + ed.edcode_encode(bytes(head)) + body + tail
    return out.hex()


def main():
    ap = argparse.ArgumentParser(description="M2 验收③红检夹具生成器")
    ap.add_argument("--in", dest="src", required=True)
    ap.add_argument("--out", dest="dst", required=True)
    ap.add_argument("--ident", type=int, default=51)
    ap.add_argument("--port", type=int, default=7200)
    ap.add_argument("--dir", default="s2c")
    ap.add_argument("--mutate", choices=("tail", "recog"), default="tail")
    args = ap.parse_args()

    if not os.path.exists(args.src):
        print("PREREQ_MISSING: 找不到输入 %s" % args.src, file=sys.stderr)
        return 2

    ed = _load_edcode() if args.mutate == "recog" else None

    rows = []
    hit = 0
    with open(args.src, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            r = json.loads(line)
            if (
                hit == 0
                and r.get("kind") == "frame"
                and r.get("port") == args.port
                and r.get("dir") == args.dir
                and r.get("ident") == args.ident
            ):
                old = r.get("raw_hex", "")
                try:
                    new = mutate_tail(old) if args.mutate == "tail" else mutate_recog(old, ed)
                except ValueError as e:
                    print("PREREQ_MISSING: ident=%d 的帧不可改（%s）" % (args.ident, e), file=sys.stderr)
                    return 2
                r["raw_hex"] = new
                if args.mutate == "recog":
                    # 回填 recog 字段（仅作记录；s2c_diff 不比对 recog 字段，靠 raw_hex 归一化）
                    head = ed.edcode_decode(bytes.fromhex(new)[1:17])
                    r["recog"] = int.from_bytes(head[0:4], "little", signed=True)
                hit += 1
            rows.append(r)

    if hit == 0:
        print(
            "PREREQ_MISSING: %s 里没有 port=%d dir=%s ident=%d 的帧" % (args.src, args.port, args.dir, args.ident),
            file=sys.stderr,
        )
        return 2

    with open(args.dst, "w", encoding="utf-8", newline="\n") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print("MUTATED ident=%d mutate=%s -> %s" % (args.ident, args.mutate, args.dst))
    return 0


if __name__ == "__main__":
    sys.exit(main())
