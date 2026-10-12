#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""session_target_availability.py — 从对拍会话件读出「哪几击有目标」与目标消失点。

owner 2026-10-12 22:35 第 ② 条：解释「为什么 C# 只有 3/120 进 AttackSuccess」。
本工具不改判据、不连网，只把会话件里的事实抽出来：

  * `attack_seq.jsonl`：逐 k 命中/伤害 + summary（`attacked/hits/death_k/min_hp/...`）；
  * `bot.log.txt`：`M3ATTSEQ role=struck` 行（那一击真出了 struck）与 `[ERR]` 行（与 target 消失同刻的
    客户端异常，用于排除/登记）；
  * 可选 `--gate-log <RunGate.out.log>`：会话窗口内是否有**连接断开**（排除「目标被踢下线」）。

输出：C# 侧「有目标」的击号、目标消失的第一击、目标是否死亡、连接是否保持。

退出码：0 = 正常；2 = 前置缺失。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


def read_seq(path: Path):
    rows, summary = [], None
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        line = line.strip()
        if not line:
            continue
        d = json.loads(line)
        if d.get("role") == "summary":
            summary = d
        else:
            rows.append(d)
    return rows, summary


STRIKE_RE = re.compile(r"M3ATTSEQ \{\"role\":\"struck\",(.*)\}")


def read_struck(path: Path):
    out = []
    if not path.exists():
        return out
    for line in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
        m = STRIKE_RE.search(line)
        if m:
            ts = line.split(" ", 1)[0]
            out.append((ts, "{" + m.group(1) + "}"))
    return out


def read_errs(path: Path):
    out = []
    if not path.exists():
        return out
    for line in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
        if "[ERR]" in line:
            out.append(line.strip())
    return out


def read_disconnects(path: Path):
    if not path or not path.exists():
        return None
    return [l.strip() for l in path.read_text(encoding="utf-8-sig", errors="replace").splitlines()
            if "断开" in l]


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="session target availability")
    ap.add_argument("session")
    ap.add_argument("--gate-log", default="")
    ap.add_argument("--label", default="")
    args = ap.parse_args(argv)
    sess = Path(args.session)
    seq_p = sess / "attack_seq.jsonl"
    if not seq_p.exists():
        print("PREREQ_MISSING: %s" % seq_p, file=sys.stderr)
        return 2
    rows, summary = read_seq(seq_p)
    struck = read_struck(sess / "bot.log.txt")
    errs = read_errs(sess / "bot.log.txt")
    dis = read_disconnects(Path(args.gate_log)) if args.gate_log else None

    target_ks = [r["k"] for r in rows if r.get("hit") or r.get("maxhp", 0) > 0]
    struck_k = len(struck)
    print("=== session %s %s ===" % (args.label or sess.name, ""))
    print("attacked=%s hits=%s dodges=%s death_k=%s min_hp=%s"
          % (summary.get("attacked"), summary.get("hits"), summary.get("dodges"),
             summary.get("death_k"), summary.get("min_hp")))
    print("逐 k 有目标（hit=true 或 maxhp>0）的击号 = %s" % (target_ks,))
    if target_ks:
        print("第一处无目标的击号 = %d" % (max(target_ks) + 1))
    print("bot.log role=struck 行数 = %d；[ERR] 行数 = %d" % (struck_k, len(errs)))
    for ts, s in struck:
        print("  STRUCK %s %s" % (ts, s))
    for e in errs[:8]:
        print("  ERR    %s" % e)
    if dis is not None:
        print("gate 连接断开行 = %s" % (dis,))
    return 0


if __name__ == "__main__":
    sys.exit(main())
