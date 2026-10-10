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
  "layouts": {"10": {"kind": "struct_then_rest", "struct_len": 8},
              "201": {"kind": "segmented", "seg_len": 124, "sep": 47,
                      "trailing_sep": true, "count_kind": "header_series"},
              "811": {"kind": "segmented", "seg_len": 20, "sep": 47,
                      "trailing_sep": false, "count_kind": "fixed", "count": 3}, ...}
}

第三种形态（A 线 A4 冻结的契约，见 `crates/protocol/src/frame.rs` 的 `BodyLayout::Segmented`）：
体 = N 段各自编码、**用线上字面量分隔符连接**（811 固定 3 段无尾分隔；201 每件物品一段、有尾分隔、
段数取头里的 series）。段长/段数/分隔符/尾分隔符**全部由表给出** ⇒ 选错任一项都编不出原字节，
所以这是真判据而不是"搜出来的切法"。

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
    # 臂语法**两种都认**（本仓两种都出现过；只认一种就会静默少一档，见下守卫）：
    #   messages::X => { BodyLayout::Kind { f: v } }     // StructThenRest 家族（外层带大括号）
    #   messages::X => BodyLayout::Kind { f: v },        // Segmented 家族（无外层大括号）
    arm_re = re.compile(
        r"((?:messages::[A-Z0-9_]+\s*\|\s*)*messages::[A-Z0-9_]+)\s*=>\s*\{?\s*"
        r"BodyLayout::(StructThenRest|Segmented)\s*\{(.*?)\}\s*\}?", re.S)
    seen_kinds = {"StructThenRest": 0, "Segmented": 0}

    def _names_ok(names):
        for n in names:
            if n not in consts:
                raise SystemExit(f"解析失败：常量 {n} 不在 messages.rs 里（拼写变了？）")

    for mm in arm_re.finditer(body):
        names = re.findall(r"messages::([A-Z0-9_]+)", mm.group(1))
        kind = mm.group(2)
        spec = mm.group(3)
        seen_kinds[kind] += 1
        if kind == "StructThenRest":
            m2 = re.search(r"struct_len\s*:\s*(\d+)", spec)
            if not m2:
                raise SystemExit("解析失败：StructThenRest 缺 struct_len（%s）—— 形状变了，先人工核对" % (names,))
            entry = {"kind": "struct_then_rest", "struct_len": int(m2.group(1))}
        else:
            # Segmented（A4 冻结契约）：seg_len: N, sep: b'/', trailing_sep: bool, count: SegCount::…
            m2 = re.search(r"(?<![\w])seg_len\s*:\s*(\d+)", spec)
            if not m2:
                raise SystemExit("解析失败：Segmented 缺 seg_len（%s）—— 形状变了，先人工核对" % (names,))
            seg_len = int(m2.group(1))
            m2 = (re.search(r"(?<![\w])sep\s*:\s*b?'(.)'", spec)
                  or re.search(r'(?<![\w])sep\s*:\s*b"(.)"', spec))
            if not m2:
                raise SystemExit("解析失败：Segmented 缺 sep（%s）" % (names,))
            sep = ord(m2.group(1))
            m2 = re.search(r"trailing_sep\s*:\s*(true|false)", spec)
            if not m2:
                raise SystemExit("解析失败：Segmented 缺 trailing_sep（%s）" % (names,))
            trailing = m2.group(1) == "true"
            m2 = re.search(r"(?<![\w])count\s*:\s*SegCount::(Fixed\s*\(\s*(\d+)\s*\)|HeaderSeries)", spec)
            if not m2:
                raise SystemExit("解析失败：Segmented 缺 count/SegCount（%s）" % (names,))
            entry = {"kind": "segmented", "seg_len": seg_len, "sep": sep, "trailing_sep": trailing}
            if m2.group(2):
                entry["count_kind"] = "fixed"
                entry["count"] = int(m2.group(2))
            else:
                entry["count_kind"] = "header_series"
        _names_ok(names)
        for n in names:
            layouts[str(consts[n])] = dict(entry, name=n)

    # 见到某形态却一项都没解析出来 ⇒ 报错（不许静默少一档：那会导出一张"看着正常"的缺项错表）
    for kind in ("StructThenRest", "Segmented"):
        if f"BodyLayout::{kind}" in body and seen_kinds[kind] == 0:
            raise SystemExit("解析失败：body_layout() 里出现 BodyLayout::%s 但本工具一项都解析不出 "
                             "—— 形状变了（换行/字段名/类型），报错而不是静默少一档" % kind)
    if not layouts:
        raise SystemExit("解析失败：body_layout() 里没有任何 StructThenRest/Segmented 分支（表被清空或形状变了）")
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
    summary = {k: (v.get("struct_len") if v["kind"] == "struct_then_rest" else
                   "seg(L=%s,sep=%s,trail=%s,%s)" % (v.get("seg_len"), v.get("sep"),
                                                     v.get("trailing_sep"), v.get("count_kind")))
               for k, v in table["layouts"].items()}
    print("LAYOUT_OK head_block=%d act_prefix=%r layouts=%s -> %s" % (
        table["head_block"], table["act_prefix"], summary, args.out))


if __name__ == "__main__":
    main()
