#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dump_body_layout.py — 从协议层源码派生「体分段布局表」为 JSON，供 GoldenExport 消费。

**为什么从源码派生、而不是在导出器里再写一份表**：分段表是 A 线协议层的**唯一真值**
（`mir2-rs/crates/protocol/src/frame.rs` 的 `body_layout()`），导出器只是消费者。
把表抄成第二份 = 口径分叉（本项目反复栽过），所以这里每次现读现导，并在解析结果上做断言：
一旦源码里 `body_layout` 的形状变了（新增枚举分支、改了常量名），本工具**直接报错**而不是静默导出一张错表。

解析出的表结构（写进 --out）：
{
  "source": "<frame.rs 路径>",
  "head_block": 16,                 # enc(12B 头) 恒为 16 字符
  "act_prefix": "+",                 # 明文动作帧 payload 首字符（is_act_payload）
  "known_idents": {"SM_TURN": 10, ...},   # 从 mirrors 常量表抓到的名字→值（仅用于人读）
  "layouts": {"10": {"kind": "struct_then_rest", "struct_len": 8}, ...}
}

用法：python dump_body_layout.py --frame-rs <...>frame.rs --constants <...>messages.rs --out layout.json
"""
import argparse
import json
import os
import re
import sys


def parse_messages(constants_path):
    """messages.rs 里的 `pub const NAME: u16 = N;` → {NAME: N}"""
    text = open(constants_path, encoding="utf-8", errors="replace").read()
    out = {}
    for m in re.finditer(r"pub const ([A-Z0-9_]+): u16 = (\d+);", text):
        out[m.group(1)] = int(m.group(2))
    return out


def parse_body_layout(frame_rs, consts):
    text = open(frame_rs, encoding="utf-8", errors="replace").read()

    # head_block：decode_server_payload 里的 `const HEAD_BLOCK: usize = 16;`
    m = re.search(r"const HEAD_BLOCK:\s*usize\s*=\s*(\d+);", text)
    if not m:
        raise SystemExit("解析失败：frame.rs 里找不到 HEAD_BLOCK 常量（decode_server_payload 被改过？）")
    head_block = int(m.group(1))

    # act 前缀：`pub const STATUS_GOOD_PREFIX: &[u8] = b"+GD/";` → 取首字符
    m = re.search(r'pub const STATUS_GOOD_PREFIX:\s*&\[u8\]\s*=\s*b"([^"]+)";', text)
    if not m:
        raise SystemExit("解析失败：frame.rs 里找不到 STATUS_GOOD_PREFIX（act 帧模型被改过？）")
    act_prefix = m.group(1)[:1]

    # 布局：body_layout() 的 match 分支
    m = re.search(r"pub fn body_layout\(ident: u16\) -> BodyLayout \{(.*?)\n\}", text, re.S)
    if not m:
        raise SystemExit("解析失败：找不到 body_layout() 函数体")
    body = m.group(1)
    layouts = {}
    struct_re = re.compile(
        r"((?:messages::[A-Z0-9_]+\s*\|\s*)*messages::[A-Z0-9_]+)\s*=>\s*\{\s*"
        r"BodyLayout::StructThenRest\s*\{\s*struct_len:\s*(\d+)\s*\}", re.S)
    for mm in struct_re.finditer(body):
        names = re.findall(r"messages::([A-Z0-9_]+)", mm.group(1))
        slen = int(mm.group(2))
        for n in names:
            if n not in consts:
                raise SystemExit(f"解析失败：常量 {n} 不在 messages.rs 里（拼写变了？）")
            layouts[str(consts[n])] = {"kind": "struct_then_rest", "struct_len": slen,
                                       "name": n}
    if not layouts:
        raise SystemExit("解析失败：body_layout() 里没有任何 StructThenRest 分支（表被清空或形状变了）")
    if "=> BodyLayout::Single" not in body.replace(" ", " ") and "_ => BodyLayout::Single" not in body:
        print("警告：body_layout() 里没看到 `_ => BodyLayout::Single` 兜底分支，请人工确认", file=sys.stderr)
    return {"head_block": head_block, "act_prefix": act_prefix, "layouts": layouts}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--frame-rs", required=True)
    ap.add_argument("--constants", required=True, help="messages.rs（把常量名映射成值）")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    consts = parse_messages(args.constants)
    table = parse_body_layout(args.frame_rs, consts)
    table["source"] = os.path.abspath(args.frame_rs)
    table["constants_source"] = os.path.abspath(args.constants)
    table["known_idents"] = {v["name"]: int(k) for k, v in table["layouts"].items()}

    with open(args.out, "w", encoding="utf-8", newline="\n") as f:
        json.dump(table, f, ensure_ascii=False, indent=2, sort_keys=True)
    print("LAYOUT_OK head_block=%d act_prefix=%r struct_then_rest=%s -> %s" % (
        table["head_block"], table["act_prefix"],
        {k: v["struct_len"] for k, v in table["layouts"].items()}, args.out))


if __name__ == "__main__":
    main()
