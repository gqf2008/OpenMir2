#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""aoi_visibility_pair.py —— 卡 `openmir2-client-e2e-enter-gaps` 缺口②「同屏没怪」的
**两侧（Rust vs C#）客户端可见性对照**判据（root 2026-10-14 00:4x ③ 收口的「两侧对照」半段）。

口径（与 `70-aoi-visibility-verdict.txt` §E 的 recipe 同）：两侧**同一角色、同一坐标、同一
`viewrange`**、同一冻结客户端；各自经 `mir2_proxy.py`(7200) 抓包 + `export_s2c.ps1` 解码，得到
**客户端实际收到**的 s2c `capture.jsonl`。本工具只消费两侧 capture，抽出
`SM_TURN(10) + SM_FEATURECHANGED(41)` **相邻出现对** —— 每对 = 一个 actor 在该侧进入同屏
（`check_client_s2c_frames.py::appearance_pairs`，本工具**直接 import 复用**该逻辑，杜绝口径漂移）。

判据（全机判）：
  * `--expect none`     ：两侧出现对都为 0 ⇒ GREEN（该点确实无怪）。
  * `--expect visible`  ：两侧出现对都 >= `--min-pairs`（默认 1）⇒ GREEN（同屏出现怪）。
  * `--expect report`   ：只报告，不判绿红。
  * `--strict-recogs`   ：额外要求两侧 actor `recog` 集合**相等**（`70 §E step 3` 的假设：
                          同图同起点下两侧 ActorId 相同）。**默认关**——该假设尚未在真机两侧
                          同时验证过，先只作信息行；窗口验证后再按需开启作为强判据。

退出码：`0`=GREEN / `1`=RED / `2`=PREREQ（缺文件或不可解析；**绝不静默通过**）。
`--selftest` 不依赖外部件：造合成 capture 自证 none/visible/计数/missing/recog 等 8 条口径。
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile

# 与 `check_client_s2c_frames.py` 同一目录；import 复用其 load_capture/s2c/appearance_pairs。
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_client_s2c_frames as ccsf  # noqa: E402


def extract(path: str):
    """返回 (s2c_rows, pairs, recog_set)。pairs = [(recog10, recog41), ...]（出现序）。"""
    rows = ccsf.load_capture(path)
    s = ccsf.s2c(rows)
    pairs = ccsf.appearance_pairs(s)
    recogs = sorted({a & 0xFFFFFFFF for a, _ in pairs})
    return s, pairs, recogs


def compare(
    left: str,
    right: str,
    left_label: str,
    right_label: str,
    expect: str,
    min_pairs: int,
    strict_recogs: bool,
):
    """返回 (rc, msgs:list[str], facts:dict)。rc: 0 GREEN / 1 RED / 2 PREREQ。"""
    msgs: list[str] = []
    for label, path in ((left_label, left), (right_label, right)):
        if not path:
            return 2, [f"PREREQ_RED: 缺 {label} 侧 capture 路径（--{'left' if label == left_label else 'right'}）"], {}
        if not os.path.isfile(path):
            return 2, [f"PREREQ_RED: {label} 侧 capture 不存在: {path}"], {}

    try:
        ls, lpairs, lrecogs = extract(left)
        rs, rpairs, rrecogs = extract(right)
    except Exception as exc:  # noqa: BLE001
        return 2, [f"PREREQ_RED: capture 解析失败：{exc}"], {}

    if not ls:
        return 2, [f"PREREQ_RED: {left_label} 侧 capture 无 s2c 帧（{left}）"], {}
    if not rs:
        return 2, [f"PREREQ_RED: {right_label} 侧 capture 无 s2c 帧（{right}）"], {}

    lset, rset = set(lrecogs), set(rrecogs)
    both = sorted(lset & rset)
    only_l = sorted(lset - rset)
    only_r = sorted(rset - lset)

    facts = {
        "left_label": left_label,
        "right_label": right_label,
        "left_capture": left,
        "right_capture": right,
        "left_pairs": len(lpairs),
        "right_pairs": len(rpairs),
        "left_recogs": ["0x%08x" % r for r in lrecogs],
        "right_recogs": ["0x%08x" % r for r in rrecogs],
        "recog_overlap": {
            "both": ["0x%08x" % r for r in both],
            "only_left": ["0x%08x" % r for r in only_l],
            "only_right": ["0x%08x" % r for r in only_r],
        },
        "expect": expect,
        "min_pairs": min_pairs,
        "strict_recogs": strict_recogs,
    }

    msgs.append(
        f"{left_label}: pairs={len(lpairs)} recogs={facts['left_recogs']} | "
        f"{right_label}: pairs={len(rpairs)} recogs={facts['right_recogs']}"
    )
    msgs.append(
        "INFO recog 集合对照（仅信息，默认不判红；--strict-recogs 才判）："
        f"both={len(both)} only_{left_label}={len(only_l)} only_{right_label}={len(only_r)}"
    )
    if only_l or only_r:
        msgs.append(
            "INFO 注意：两侧 recog 集合不等。若两侧「可见/不可见」结论一致而 recog 不同，"
            "说明 `70 §E step 3` 的 ActorId 相同假设在该场景不成立——以计数/截图为判据并登记。"
        )

    red = False
    if expect == "none":
        if len(lpairs) != 0:
            msgs.append(f"RED --expect none：{left_label} 出现对={len(lpairs)} != 0")
            red = True
        if len(rpairs) != 0:
            msgs.append(f"RED --expect none：{right_label} 出现对={len(rpairs)} != 0")
            red = True
        if not red:
            msgs.append("OK  --expect none：两侧出现对都为 0（该点窗口内无怪）")
    elif expect == "visible":
        if len(lpairs) < min_pairs:
            msgs.append(f"RED --expect visible：{left_label} 出现对={len(lpairs)} < --min-pairs {min_pairs}")
            red = True
        if len(rpairs) < min_pairs:
            msgs.append(f"RED --expect visible：{right_label} 出现对={len(rpairs)} < --min-pairs {min_pairs}")
            red = True
        if not red:
            msgs.append(
                f"OK  --expect visible：两侧出现对均 >= {min_pairs}"
                f"（{left_label}={len(lpairs)} / {right_label}={len(rpairs)}）"
            )
    else:  # report
        msgs.append(
            f"INFO --expect report：{left_label}={len(lpairs)} / {right_label}={len(rpairs)}（不判绿红）"
        )

    if strict_recogs:
        if lset != rset:
            msgs.append("RED --strict-recogs：两侧 recog 集合不等（见上 only_* 列表）")
            red = True
        else:
            msgs.append("OK  --strict-recogs：两侧 recog 集合相等")

    return (1 if red else 0), msgs, facts


# ---------------------------------------------------------------- selftest

def _enter_rows():
    """一份「有 s2c 帧但没有 10/41 出现对」的基线 capture（模拟该点真的无怪）。"""
    return [
        {"kind": "frame", "dir": "s2c", "seq": 0, "ident": 658, "recog": 0, "body_len": 0,
         "raw_hex": "00"},
        {"kind": "frame", "dir": "s2c", "seq": 1, "ident": 46, "recog": 0, "body_len": 0,
         "raw_hex": "00"},
    ]


def _pair_rows(recogs):
    rows = []
    seq = 0
    for r in recogs:
        rows.append({"kind": "frame", "dir": "s2c", "seq": seq, "ident": 10, "recog": r,
                     "body_len": 14, "raw_hex": "00"})
        seq += 1
        rows.append({"kind": "frame", "dir": "s2c", "seq": seq, "ident": 41, "recog": r,
                     "body_len": 0, "raw_hex": "00"})
        seq += 1
    return rows


def _write(path, rows):
    with open(path, "w", encoding="utf-8") as fh:
        for r in rows:
            fh.write(json.dumps(r, ensure_ascii=False) + "\n")


def selftest() -> int:
    ok = True
    with tempfile.TemporaryDirectory() as td:
        none_f = os.path.join(td, "none.jsonl")
        vis2_f = os.path.join(td, "vis2.jsonl")
        vis3_f = os.path.join(td, "vis3.jsonl")
        other_f = os.path.join(td, "other.jsonl")
        _write(none_f, _enter_rows())
        _write(vis2_f, _pair_rows([101, 202]))
        _write(vis3_f, _pair_rows([101, 202, 303]))
        _write(other_f, _pair_rows([101, 202, 404]))
        missing_f = os.path.join(td, "nope.jsonl")

        cases = [
            # name, (left, right), expect, min_pairs, strict, want_rc
            ("none-green", (none_f, none_f), "none", 1, False, 0),
            ("none-red-left-visible", (vis2_f, none_f), "none", 1, False, 1),
            ("none-red-right-visible", (none_f, vis2_f), "none", 1, False, 1),
            ("visible-green-both", (vis2_f, vis3_f), "visible", 1, False, 0),
            ("visible-red-right-empty", (vis2_f, none_f), "visible", 1, False, 1),
            ("visible-red-min-pairs", (vis2_f, vis3_f), "visible", 3, False, 1),
            ("strict-recogs-red", (vis2_f, other_f), "visible", 1, True, 1),
            ("non-strict-recogs-green", (vis2_f, other_f), "visible", 1, False, 0),
            ("report-always-green", (vis2_f, none_f), "report", 1, False, 0),
            ("prereq-missing", (vis2_f, missing_f), "visible", 1, False, 2),
        ]
        bad = 0
        for name, (l, r), expect, min_pairs, strict, want in cases:
            rc, msgs, _ = compare(l, r, "rust", "csharp", expect, min_pairs, strict)
            good = rc == want
            if not good:
                bad += 1
            print(f"[{'PASS' if good else 'FAIL'}] selftest {name}: rc={rc} (want {want})")
            for m in msgs:
                print("        " + m)
        # 追加：抽取逻辑必须与 check_client_s2c_frames 同口径（相邻 10→41 才算一对）。
        _write(os.path.join(td, "gap.jsonl"),
               [{"kind": "frame", "dir": "s2c", "seq": 0, "ident": 10, "recog": 1, "body_len": 14,
                 "raw_hex": "00"},
                {"kind": "frame", "dir": "s2c", "seq": 1, "ident": 10, "recog": 2, "body_len": 14,
                 "raw_hex": "00"},
                {"kind": "frame", "dir": "s2c", "seq": 2, "ident": 41, "recog": 2, "body_len": 0,
                 "raw_hex": "00"}])
        _, pairs, _ = extract(os.path.join(td, "gap.jsonl"))
        good = len(pairs) == 1
        ok = ok and good
        print(f"[{'PASS' if good else 'FAIL'}] selftest 10-10-41 只算一对（相邻 10→41）：pairs={len(pairs)}")
        if not good:
            bad += 1

        print(f"SELFTEST {'OK' if bad == 0 and ok else 'FAILED'} ({len(cases) + 1 - bad}/{len(cases) + 1})")
        return 0 if (bad == 0 and ok) else 1


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--left", default="", help="左/首侧 capture.jsonl（惯例 rust）")
    ap.add_argument("--right", default="", help="右/次侧 capture.jsonl（惯例 csharp）")
    ap.add_argument("--left-label", default="rust")
    ap.add_argument("--right-label", default="csharp")
    ap.add_argument("--expect", choices=["report", "none", "visible"], default="visible")
    ap.add_argument("--min-pairs", type=int, default=1)
    ap.add_argument("--strict-recogs", action="store_true")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args(argv)

    if a.selftest:
        return selftest()

    rc, msgs, facts = compare(a.left, a.right, a.left_label, a.right_label,
                              a.expect, a.min_pairs, a.strict_recogs)
    if a.json:
        print(json.dumps({"result": "RED" if rc == 1 else ("PREREQ_RED" if rc == 2 else "GREEN"),
                          "rc": rc, "msgs": msgs, "facts": facts}, ensure_ascii=False))
    else:
        for m in msgs:
            print(m)
        print("RESULT: %s" % ("RED" if rc == 1 else ("PREREQ_RED" if rc == 2 else "GREEN")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
