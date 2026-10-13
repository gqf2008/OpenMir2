#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""verify_client_ability_layout.py — 从**冻结客户端源码**独立复算 `TAbility` 布局。

卡：`openmir2-client-e2e-enter-gaps`（tier R）DoD ③：
`58B TAbility 与冻结 MirClient Delphi 记录布局的逐字节对位`。

本脚本不采信作者自述的偏移表：直接解析冻结客户端
`Source/Common/Grobal2.pas` 的 `{$IF HIGHHP}` / `TAbility = packed record`，
按 Delphi `packed record` 规则逐字段累加偏移，并与「期望布局」逐字段比对。
`packed` 记录 = 无对齐填充 ⇒ 偏移 = 前序字段尺寸之和。

用法：
  python verify_client_ability_layout.py [--source <Grobal2.pas>] [--json]
  python verify_client_ability_layout.py --selftest        # 不需要外部源码

退出码：0 = GREEN / 1 = RED（布局与期望不符）/ 2 = PREREQ_MISSING（找不到源码）
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys

# Delphi 基本类型尺寸（本记录只用到 Word/Integer/DWORD，其余为解析器自检用）。
TYPE_SIZE = {
    "Byte": 1, "Boolean": 1, "Char": 1, "ShortInt": 1,
    "Word": 2, "SmallInt": 2, "Short": 2,
    "Integer": 4, "Cardinal": 4, "LongWord": 4, "DWORD": 4, "LongInt": 4, "Single": 4,
    "Int64": 8, "Double": 8,
}

# 冻结客户端（HIGHHP=True）下的期望布局：字段 (名, 类型, 偏移)。
EXPECTED_TRUE = [
    ("Level", "Word", 0),
    ("AC", "Integer", 2),
    ("MAC", "Integer", 6),
    ("DC", "Integer", 10),
    ("MC", "Integer", 14),
    ("SC", "Integer", 18),
    ("HP", "DWORD", 22),
    ("MP", "DWORD", 26),
    ("MaxHP", "DWORD", 30),
    ("MaxMP", "DWORD", 34),
    ("Exp", "DWORD", 38),
    ("MaxExp", "DWORD", 42),
    ("Weight", "Word", 46),
    ("MaxWeight", "Word", 48),
    ("WearWeight", "Word", 50),
    ("MaxWearWeight", "Word", 52),
    ("HandWeight", "Word", 54),
    ("MaxHandWeight", "Word", 56),
]
EXPECTED_TRUE_SIZE = 58

DEFAULT_SOURCE = os.environ.get(
    "MIR2_CLIENT_GROBAL2",
    "E:/Users/gxh/Documents/GitHub/MirClient/Source/Common/Grobal2.pas",
)

# 自检用最小源码片段（不依赖外部 checkout）。
SELFTEST_TRUE = """
  {$IF HIGHHP}
  HIGHHP = True;
  {$ELSE}
  HIGHHP = False;
  {$IFEND}
  TAbility = packed record
    Level: Word;
    AC: Integer;
    MAC: Integer;
    DC: Integer;
    MC: Integer;
    SC: Integer;
{$IF HIGHHP}
    HP: DWORD;
    MP: DWORD;
    MaxHP: DWORD;
    MaxMP: DWORD;
{$ELSE}
    HP: Word;
    MP: Word;
    MaxHP: Word;
    MaxMP: Word;
{$IFEND}
    Exp: DWORD;
    MaxExp: DWORD;
    Weight: Word;
    MaxWeight: Word;
    WearWeight: Word;
    MaxWearWeight: Word;
    HandWeight: Word;
    MaxHandWeight: Word;
  end;
"""

SELFTEST_FALSE = SELFTEST_TRUE.replace("HIGHHP = True;", "HIGHHP = False;", 1)


def parse_highhp(text: str) -> bool:
    m = re.search(r"^\s*HIGHHP\s*=\s*(True|False)\s*;", text, re.MULTILINE | re.IGNORECASE)
    if m is None:
        raise ValueError("找不到 `HIGHHP = True/False;` 定义")
    return m.group(1).lower() == "true"


def parse_ability_record(text: str) -> list[tuple[str, str]]:
    """抽出 `TAbility = packed record … end;` 的**生效**字段序（处理 {$IF}/{$ELSE}/{$IFEND}）。"""
    m = re.search(r"TAbility\s*=\s*packed\s+record\b(.*?)^\s*end\s*;", text,
                  re.MULTILINE | re.DOTALL)
    if m is None:
        raise ValueError("找不到 `TAbility = packed record … end;`")
    body = m.group(1)

    fields: list[tuple[str, str]] = []
    # 条件栈：[(cond_bool, in_else)]；highhp 为唯一条件。
    stack: list[list[bool]] = []  # [cond_active, in_else]
    highhp = parse_highhp(text)

    def active() -> bool:
        for cond, in_else in stack:
            want = (not cond) if in_else else cond
            if not want:
                return False
        return True

    for raw in body.splitlines():
        line = raw.strip()
        if not line:
            continue
        up = line.upper()
        if up.startswith("{$IF ") or up.startswith("{$IFDEF "):
            token = up.split(None, 1)[1].rstrip("}")
            token = token.split()[0].rstrip("}")
            # 唯一条件符号 HIGHHP 取文件里 `HIGHHP = True/False;` 的实测值。
            stack.append([highhp if token == "HIGHHP" else True, False])
            continue
        if up.startswith("{$ELSE"):
            if not stack:
                raise ValueError("{$ELSE} 无匹配的 {$IF}")
            stack[-1][1] = not stack[-1][1]
            continue
        if up.startswith("{$IFEND") or up.startswith("{$ENDIF"):
            if stack:
                stack.pop()
            continue
        if line.startswith("{"):
            continue
        fm = re.match(r"^([A-Za-z_]\w*)\s*:\s*([A-Za-z_]\w*)\s*;", line)
        if fm:
            if active():
                fields.append((fm.group(1), fm.group(2)))
            continue
    return fields


def layout(fields: list[tuple[str, str]]) -> list[tuple[str, str, int]]:
    out = []
    off = 0
    for name, typ in fields:
        if typ not in TYPE_SIZE:
            raise ValueError(f"未知类型 `{typ}`（字段 {name}）")
        out.append((name, typ, off))
        off += TYPE_SIZE[typ]
    return out


def check(actual: list[tuple[str, str, int]], expected: list[tuple[str, str, int]],
          expected_size: int) -> list[str]:
    problems = []
    if len(actual) != len(expected):
        problems.append(f"字段数 {len(actual)} != 期望 {len(expected)}")
    size = sum(TYPE_SIZE[t] for _, t, _ in actual)
    if size != expected_size:
        problems.append(f"总长 {size} != 期望 {expected_size}")
    for (an, at, ao), (en, et, eo) in zip(actual, expected):
        if (an, at, ao) != (en, et, eo):
            problems.append(f"字段错位：实际 {an}:{at}@{ao} != 期望 {en}:{et}@{eo}")
    return problems


def render(actual: list[tuple[str, str, int]]) -> str:
    size = sum(TYPE_SIZE[t] for _, t, _ in actual)
    lines = [f"SizeOf(TAbility) = {size}"]
    for n, t, o in actual:
        lines.append(f"  offset {o:>2} : {n:<14}({t}) {'DWORD@' + str(o) if t == 'DWORD' else ''}")
    return "\n".join(lines)


def run(source_text: str, source_label: str, as_json: bool) -> int:
    highhp = parse_highhp(source_text)
    fields = parse_ability_record(source_text)
    actual = layout(fields)
    problems = check(actual, EXPECTED_TRUE, EXPECTED_TRUE_SIZE)
    size = sum(TYPE_SIZE[t] for _, t, _ in actual)
    hp = dict((n, (t, o)) for n, t, o in actual).get("HP")

    result = {
        "source": source_label,
        "highhp": highhp,
        "size": size,
        "expected_size": EXPECTED_TRUE_SIZE,
        "hp": {"type": hp[0], "offset": hp[1]} if hp else None,
        "fields": [{"name": n, "type": t, "offset": o} for n, t, o in actual],
        "problems": problems,
        "verdict": "RED" if problems else "GREEN",
    }
    if as_json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(f"# source = {source_label}")
        print(f"HIGHHP = {highhp}")
        print(render(actual))
        if hp:
            print(f"HP = {hp[0]} @ offset {hp[1]} "
                  f"(期望 DWORD@22)")
        if problems:
            print("RED:")
            for p in problems:
                print(f"  - {p}")
        else:
            print("GREEN: 与期望 58B 布局逐字段一致（HP/MP/MaxHP/MaxMP = DWORD@22/26/30/34）")
    return 1 if problems else 0


def selftest() -> int:
    rc = 0
    t_fields = parse_ability_record(SELFTEST_TRUE)
    t_layout = layout(t_fields)
    if check(t_layout, EXPECTED_TRUE, EXPECTED_TRUE_SIZE):
        print("[selftest] HIGHHP=True 内联片段与期望不符")
        print(render(t_layout))
        rc = 1
    else:
        print("[selftest] HIGHHP=True 片段 => 58B 布局 OK")
    f_fields = parse_ability_record(SELFTEST_FALSE)
    f_layout = layout(f_fields)
    f_size = sum(TYPE_SIZE[t] for _, t, _ in f_layout)
    f_hp = dict((n, (t, o)) for n, t, o in f_layout).get("HP")
    if (f_size, f_hp) != (50, ("Word", 22)):
        print(f"[selftest] HIGHHP=False 片段 => size={f_size} HP={f_hp}（期望 50 / Word@22）")
        rc = 1
    else:
        print("[selftest] HIGHHP=False 片段 => 50B / HP Word@22 OK（解析器条件分支正确）")
    # 解析器必须能区分两种分支：True 片段必须恰好 58（负向对照）
    if sum(TYPE_SIZE[t] for _, t, _ in t_layout) == f_size:
        print("[selftest] 两种条件分支尺寸相同 => 解析器未处理 {$IF} （RED）")
        rc = 1
    print("[selftest] " + ("GREEN" if rc == 0 else "RED"))
    return rc


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--source", default=DEFAULT_SOURCE,
                    help=f"冻结客户端 Grobal2.pas 路径（默认 {DEFAULT_SOURCE}）")
    ap.add_argument("--json", action="store_true", help="输出 JSON")
    ap.add_argument("--selftest", action="store_true", help="用内联片段自检（不需要外部源码）")
    args = ap.parse_args()

    if args.selftest:
        return selftest()

    if not os.path.isfile(args.source):
        print(f"PREREQ_MISSING: 找不到冻结客户端源码 `{args.source}`", file=sys.stderr)
        print("提示：设 MIR2_CLIENT_GROBAL2 或 --source 指向 mirbeta/MirClient 的 "
              "Source/Common/Grobal2.pas", file=sys.stderr)
        return 2
    try:
        with open(args.source, "r", encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError as exc:  # pragma: no cover
        print(f"PREREQ_MISSING: 读不到 `{args.source}`: {exc}", file=sys.stderr)
        return 2
    return run(text, args.source, args.json)


if __name__ == "__main__":
    raise SystemExit(main())
