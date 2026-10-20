#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""attack_rng_ledger.py — M3②③ 攻击对拍的**同种子抽数账本**机判（owner 2026-10-12 17:35，残差①）。

背景：`attack_diff.py diff` 在 k=2 会话上仍差 80+ 项，owner 已裁决「RNG 流位置未对齐」是结论、
不是定位。本脚本把两侧同种子的 `Random` 消费序列按**第 k 次攻击**分段逐项对齐，指出
**具体在第几步、多抽还是少抽**（D2 口径：`RNG同种子与记录回放方案.md`）。

数据源（两侧同形）：
  * 抽数流水 = C# rng-hook（`MIR2_RNG_SEED`/`MIR2_RNG_LOG`/`MIR2_RNG_SITE=1`）写出的 TSV：
        seq \t kind \t max \t result \t site
    形如 `223799\tNext(max)\t1\t0\tM2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929`。
    首两行是 `1\tINJECT_OK\t42\t0` 与 `SEED_PROBE ...`（跳过）。
  * 分段锚 = 攻击路径里**每次攻击恰调用一次**的站点（C# 侧 = `PlayObject.GetAttackPower@PlayObject.cs:2929`，
    真机 120 击实测恰 120 次）。每个锚开启一个 segment = 第 k 次攻击的取数窗口。

两侧（`--side csharp|rust`，见下）：C# 站点表与 m3cs 决策片的 Rust 站点表**同口径**，
缺省（不给 `--side`）逐字节保持旧行为 = C#；站点分隔符（反斜杠 vs 正斜杠）在比对时归一。

子命令（都不连网/不碰 DB/不起栈）：
  summary <trace.tsv> [--side csharp|rust] [--attack-re RE] [--top N]
      总数 + 攻击路径取数 vs **噪声取数**（怪 AI `AnimalObject.Wondering`、`Merchant.Run`、
      `MonGetRandomItems` …）的占比与 Top 站点。噪声占比高 ⇒ 「同种子 ⇒ 逐 k 相同」的前提不成立。
      `--side rust` 时按 Rust 站点表（`crates/world/src/combat.rs:…`）分类。
  segment <trace.tsv> [--marker <site>] [--side csharp|rust] [--attack-re RE] [--json out.jsonl]
      按 marker 分段（`--side rust` 时缺省 marker = `crates\world\src\combat.rs:109`），逐段输出
      `{"seg":k,"draws_total":N,"draws_attack":A,"draws_noise":M,"first_seq":…,"last_seq":…}`。
  diff <a.segments.jsonl> <b.segments.jsonl> [--max-diffs N]
      跨侧逐 k 对齐：`draws_total/attack/noise` 任一不等即 RED，打印「a 多抽/少抽 X」。长度不等即 RED。
  selftest
      合成件：同账本 ⇒ 绿；某段多一次/少一次 ⇒ 必红且报多抽/少抽；段数不等 ⇒ 必红。

退出码：0 = GREEN；1 = RED（有差异）；2 = PREREQ（文件缺失/无 marker/解析空）。
"""
import argparse
import collections
import json
import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "rng-trace-probe"))
import rng_ledger_diff as rld  # noqa: E402

# 攻击路径站点（C# 分类用；命中即「有仲裁力的取数」）。与 oracle 攻击链一一对应。
CSHARP_ATTACK_RE = re.compile(
    r"(PlayObject\.GetAttackPower@|BaseObject\._Attack@|PlayObject\.StruckDamage@|"
    r"BaseObject\.GetHitStruckDamage@|PlayObject\.AttackSuccess@|PlayObject\.AttackDir@|"
    r"BaseObject\.GetAttackPower@)"
)
# Rust 侧攻击路径站点（世界层调用点，**不是** `crates/shared/src/rng.rs` 的包装层）。
# 与 m3cs 决策片 `rng-trace-probe/rng_ledger_diff.py` 的 `RUST_ATTACK_RE` **逐字节同一张表**
# （`evidence-rng-trace-20261012/00-commands.txt` §5），保证两侧「攻击路径」同口径。
# m3cs tick22 F2：旧表漏 758/628/630/601/609/831/250 ⇒ 同一条 Rust 流水把攻击路径取数
# 算成噪声、报 attack=2/段；本表由 selftest 钉住（新站点必须分类为 attack）。
# 2026-10-17（magic-draws-move，`e7d432189`）：补当时活栈实际取数行
# 103/106/763/771/790/792/936/1018/1126——**936 = 命中骰**、**1018 = `AttackSuccess` 的
# `Random(5)`**、**1126 = 毒门**。旧行号保留为旧 trace 别名。
# 2026-10-19（`bbf9694aa`）：三处取数漂到**当前活栈** 1014（命中骰）/ 1150（`Random(5)`）/
# 1258（毒门）。**1018 从来不是命中骰**（它只是 `e7d432189` 当时的 `Random(5)`）；命中骰历史
# 是 178/190/211/315/517/685/736/758/812/936/995/1002/1007→1014（`6a774ea79` 只从 1002 到
# 1007，`bbf9694aa` 才到 1014）。三行都必须分类为 attack，否则玩家攻击链的两次
# `AttackSuccess` 取数在 segment/summary 里被算成 noise。
# 两处 `RUST_ATTACK_RE` 由 selftest 钉成**逐字同一张表**；selftest 另从
# `crates/world/src/combat.rs` **现读**这三处取数的物理行，表漏行/代码搬家未同步 ⇒ 必红
# （不再靠人记行号）。
# **MB-4 F4**：补 `struck_damage_durability` / `get_hit_struck_damage` 的当前取数行
# 829/837/856/858（CowKing `Attack` 打带装备玩家会触达；改前落 UNCLASSIFIED）。
RUST_ATTACK_RE = re.compile(
    r"combat\.rs:(67|103|106|109|111|250|579|587|601|606|608|609|628|630|736|758|763|771|790|792|829|831|837|856|858|936|1014|1018|1126|1150|1258)"
)

# 每侧缺省：分段锚（每次攻击恰调用一次的站点）+ 攻击路径正则。
CSHARP_MARKER = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"
RUST_MARKER = r"crates\world\src\combat.rs:109"
SIDE_DEFAULTS = {
    "csharp": (CSHARP_MARKER, CSHARP_ATTACK_RE),
    "rust": (RUST_MARKER, RUST_ATTACK_RE),
}


def norm_site(site):
    """Windows 落盘 `crates\\world\\src\\combat.rs:109` ⇒ 统一成 `/` 再比（与 m3cs 同口径）。"""
    return (site or "").replace("\\", "/")


def select_side(args):
    """解析 `--side`/`--marker`/`--attack-re` ⇒ `(side, marker, attack_re)`。

    缺省（既无 `--side` 也无 `--attack-re`）逐字节保持旧行为 = C# 口径；
    `--side` 只给缺省（可被显式 `--marker`/`--attack-re` 覆盖）；未知 side ⇒ attack_re=None
    （调用方 fail-closed）。
    """
    side = getattr(args, "side", None)
    marker = getattr(args, "marker", "") or ""
    attack_re = getattr(args, "attack_re", "") or ""
    if side:
        if side not in SIDE_DEFAULTS:
            return side, marker, None
        default_marker, default_rx = SIDE_DEFAULTS[side]
        marker = marker or default_marker
        rx = re.compile(attack_re) if attack_re else default_rx
    else:
        rx = re.compile(attack_re) if attack_re else CSHARP_ATTACK_RE
    return side, marker, rx


def parse_trace(path):
    """读 rng-hook TSV ⇒ [(seq, kind, max, result, site)]；跳过 INJECT_OK/SEED_PROBE/空行。"""
    rows = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\r\n")
            if not line or line.startswith("SEED_PROBE") or "INJECT_OK" in line:
                continue
            parts = line.split("\t")
            if len(parts) < 4:
                continue
            try:
                seq = int(parts[0])
            except ValueError:
                continue
            site = parts[4] if len(parts) >= 5 else ""
            rows.append((seq, parts[1], parts[2], parts[3], site))
    return rows


def is_attack(site, rx=None):
    return bool((rx or CSHARP_ATTACK_RE).search(site or ""))

def check_trace_shape(path, marker=""):
    """A3（卡 openmir2-window-collector-preflight）：流水形态 fail-closed 断言。

    历史缺陷：`parse_trace` 对 <4 列的行 `continue` 静默丢弃、对恰好 4 列的行把 site 置空
    ⇒ C# 忘设 `MIR2_RNG_SITE=1`（rng-hook 只写 4 列 `seq/kind/max/result`）时**静默退化**，
    下游只在「无 marker」处 PREREQ（白跑一场）。现改为 fail-closed：任一数据行不是**恰好 5 列**
    （`seq	kind	max/arg	result	site`，site 非空）⇒ 收进 problems 并**点名缺列**；给了 marker
    时还要求流水里**出现** marker。返回 `(ok, problems)`，调用方据此 rc≠0（缺列不静默退化）。

    跳过（不算数据行）：空行、含 `INJECT_OK` 的头行、`SEED_PROBE` 头行、首字段非 int 的其它行
    （如 `ISOLATE_ATTACK <seed> isolate_sites=...`）。与 `parse_trace` 的过滤口径逐条一致。
    """
    problems = []
    data_rows = 0
    marker_n = norm_site(marker) if marker else ""
    marker_seen = False
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for lineno, line in enumerate(fh, 1):
            line = line.rstrip("\r\n")
            if not line or line.startswith("SEED_PROBE") or "INJECT_OK" in line:
                continue
            parts = line.split("\t")
            try:
                int(parts[0])
            except ValueError:
                continue  # 非数据行（头/其它）
            data_rows += 1
            if len(parts) != 5:
                problems.append("line %d: got %d cols (need 5: seq/kind/max/result/site)" % (lineno, len(parts)))
                continue
            if not parts[4]:
                problems.append("line %d: site col empty (need non-empty site; MIR2_RNG_SITE=1 忘设?)" % lineno)
            if marker_n and norm_site(parts[4]) == marker_n:
                marker_seen = True
    if data_rows == 0:
        problems.append("no data rows (流水为空 / 全是头行)")
    if marker_n and not marker_seen:
        problems.append("no marker: %s (MIR2_RNG_SITE 未开? 站点列缺失?)" % marker)
    return (len(problems) == 0), problems




def cmd_summary(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到抽数流水 %s" % args.trace)
    side0, marker0, _rx0 = select_side(args)
    ok_shape, shape_problems = check_trace_shape(args.trace, marker0)
    if not ok_shape:
        for pr in shape_problems:
            print("SHAPE %s" % pr, file=sys.stderr)
        return _die("RED: 流水形态 fail-closed（缺列/缺 marker）——%s：%s" % (args.trace, "; ".join(shape_problems)))
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空（钩子没开/格式不符）" % args.trace)
    side, _marker, rx = select_side(args)
    if rx is None:
        return _die("PREREQ_MISSING: 未知 --side %r（可 csharp/rust）" % side)
    attack = sum(1 for r in rows if is_attack(r[4], rx))
    noise = len(rows) - attack
    hist = collections.Counter(r[4] or "(no-site)" for r in rows if not is_attack(r[4], rx))
    print("RNG_LEDGER_SUMMARY %s total=%d attack_path=%d noise=%d noise_ratio=%.4f" %
          (args.trace, len(rows), attack, noise, (noise / len(rows)) if rows else 0.0))
    for site, n in hist.most_common(args.top):
        print("  NOISE %7d  %s" % (n, site))
    return 0


def cmd_segment(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到抽数流水 %s" % args.trace)
    side, marker, rx = select_side(args)
    ok_shape, shape_problems = check_trace_shape(args.trace, marker)
    if not ok_shape:
        for pr in shape_problems:
            print("SHAPE %s" % pr, file=sys.stderr)
        return _die("RED: 流水形态 fail-closed（缺列/缺 marker）——%s：%s" % (args.trace, "; ".join(shape_problems)))
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    if rx is None:
        return _die("PREREQ_MISSING: 未知 --side %r（可 csharp/rust）" % side)
    if not marker:
        return _die("PREREQ_MISSING: 未给 --marker（且 --side 无缺省锚）")
    marker_n = norm_site(marker)  # 分隔符归一：正/反斜杠 marker 都能命中
    segments = []
    cur = None
    for seq, kind, mx, res, site in rows:
        if norm_site(site) == marker_n:
            # marker 自身算作新段的第 1 次取数（每次攻击恰一次 ⇒ 段号 = 第 k 次攻击）。
            cur = {"seg": len(segments) + 1, "draws_total": 0, "draws_attack": 0,
                   "draws_noise": 0, "first_seq": seq, "last_seq": seq,
                   "noise_sites": collections.Counter()}
            segments.append(cur)
        if cur is None:
            continue  # marker 之前的预热/登录取数不计入任何攻击段
        cur["draws_total"] += 1
        cur["last_seq"] = seq
        if is_attack(site, rx):
            cur["draws_attack"] += 1
        else:
            cur["draws_noise"] += 1
            cur["noise_sites"][site or "(no-site)"] += 1
    if not segments:
        return _die("PREREQ_MISSING: %s 里没有 marker `%s`（无攻击段）" % (args.trace, marker))
    out = args.json or (os.path.splitext(args.trace)[0] + ".segments.jsonl")
    with open(out, "w", encoding="utf-8", newline="\n") as fh:
        for s in segments:
            top = s["noise_sites"].most_common(1)
            fh.write(json.dumps({
                "seg": s["seg"], "draws_total": s["draws_total"],
                "draws_attack": s["draws_attack"], "draws_noise": s["draws_noise"],
                "first_seq": s["first_seq"], "last_seq": s["last_seq"],
                "top_noise": (top[0][0] if top else ""),
            }, ensure_ascii=False) + "\n")
    print("RNG_LEDGER_SEGMENTS %s marker=%s segments=%d" % (out, marker, len(segments)))
    for s in segments[:3]:
        print("  SEG k=%d total=%d attack=%d noise=%d first_seq=%d" %
              (s["seg"], s["draws_total"], s["draws_attack"], s["draws_noise"], s["first_seq"]))
    return 0


def load_segments(path):
    segs = []
    with open(path, "r", encoding="utf-8-sig") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            segs.append(json.loads(line))
    return segs


def cmd_diff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到分段账本 %s" % p)
    a = load_segments(args.a)
    b = load_segments(args.b)
    if not a or not b:
        print("RED: 空账本（a=%d b=%d）" % (len(a), len(b)))
        return 1
    problems = []
    if len(a) != len(b):
        problems.append("段数不等: a=%d b=%d" % (len(a), len(b)))
    for i in range(min(len(a), len(b))):
        ra, rb = a[i], b[i]
        for key in ("draws_total", "draws_attack", "draws_noise"):
            va, vb = int(ra.get(key, 0)), int(rb.get(key, 0))
            if va != vb:
                d = vb - va
                problems.append("k=%s %s 不等: a=%d b=%d（b %s %d）" %
                                (ra.get("seg"), key, va, vb, "多抽" if d > 0 else "少抽", abs(d)))
    if not problems:
        print("GREEN: 跨侧抽数账本逐 k 一致（segments=%d；attack=%d noise=%d）" %
              (len(a), a[0].get("draws_attack", 0), a[0].get("draws_noise", 0)))
        return 0
    print("RED: 跨侧抽数账本差异 %d 项（定位到「第几步 / 多抽还是少抽」）" % len(problems))
    for p in problems[:args.max_diffs]:
        print("  - " + p)
    if len(problems) > args.max_diffs:
        print("  ... 另有 %d 项（--max-diffs 截断）" % (len(problems) - args.max_diffs))
    return 1


def cmd_groups(args):
    """Claim B（owner 2026-10-13 09:35）：按**站点组**汇总两侧非攻击 RNG 消费者。

    口径与 `rng-trace-probe/rng_ledger_diff.py groups` **逐字节同源**（直接委派，不另抄一份）。
    消费者组表 = `rng-trace-probe/consumer_groups.tsv`（唯一真源）。
    """
    ns = argparse.Namespace(
        trace=args.trace, side=args.side, groups_table=args.groups_table,
        no_attack=args.no_attack, require_closed=args.require_closed,
        top=args.top, json=args.json)
    return rld.cmd_groups(ns)


def _write_trace(path, sites):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        for i, site in enumerate(sites, 1):
            fh.write("%d\tNext(max)\t10\t%d\t%s\n" % (i, i % 10, site))


def cmd_selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="rng-ledger-selftest-")
    checks, failures = [], []

    def check(name, cond, detail=""):
        checks.append(name)
        if not cond:
            failures.append(name + (" (%s)" % detail if detail else ""))

    M = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"
    N = "M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427"
    A = "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"
    # 3 次攻击：每次 marker + 2 攻击路径 + 3 噪声（噪声数故意逐次不同）
    sites_a = [M, A, M, N, N, N, M, A, M, N, N, M, A, M, N, N, N, N]
    # b 与 a 完全同形 ⇒ 绿
    sites_b = list(sites_a)
    ta = os.path.join(tmp, "a.tsv")
    tb = os.path.join(tmp, "b.tsv")
    _write_trace(ta, sites_a)
    _write_trace(tb, sites_b)
    sa = os.path.join(tmp, "a.seg.jsonl")
    sb = os.path.join(tmp, "b.seg.jsonl")
    check("segment-a-rc", cmd_segment(argparse.Namespace(trace=ta, marker=M, json=sa)) == 0)
    check("segment-b-rc", cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb)) == 0)
    rc = cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20))
    check("identical-ledger-green", rc == 0, "rc=%d" % rc)

    # 改坏：给 b 的 k=2 段多插一次噪声 ⇒ 必红且报「b 多抽 1」
    _write_trace(tb, sites_b[:6] + [N] + sites_b[6:])
    cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb))
    out = _capture(lambda: cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20)))
    check("extra-draw-red", out[0] == 1, "rc=%d" % out[0])
    check("extra-draw-shaped", "多抽 1" in out[1], out[1][:200])

    # 改坏：给 b 的 k=1 段少一次攻击路径取数 ⇒ 必红且报「b 少抽」
    _write_trace(tb, [sites_b[0], sites_b[2]] + sites_b[3:])  # 去掉 sites_b[1]（seg1 的攻击路径取数）
    cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb))
    out = _capture(lambda: cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20)))
    check("missing-draw-red", out[0] == 1, "rc=%d" % out[0])
    check("missing-draw-shaped", "少抽" in out[1], out[1][:200])

    # 段数不等 ⇒ 必红
    _write_trace(tb, sites_b[:-1])
    cmd_segment(argparse.Namespace(trace=tb, marker=M, json=sb))
    out = _capture(lambda: cmd_diff(argparse.Namespace(a=sa, b=sb, max_diffs=20)))
    check("segcount-mismatch-red", out[0] == 1, "rc=%d" % out[0])

    # ---- Rust 侧（--side rust）：同形 TSV + 反斜杠站点 + 同表攻击路径 ----
    RM = r"crates\world\src\combat.rs:109"
    RA = r"crates\world\src\combat.rs:111"
    RB = r"crates\world\src\combat.rs:736"
    RN = r"crates\world\src\animal.rs:10"  # 非攻击路径（噪声）
    # 3 次攻击，每次 = marker + 2 攻击路径取数（连续块）；外加 1 条噪声
    sites_r = [RM, RA, RB, RM, RA, RB, RN, RM, RA, RB]
    tr = os.path.join(tmp, "rust.tsv")
    tr_base = os.path.join(tmp, "rust_base.tsv")
    sr = os.path.join(tmp, "rust.seg.jsonl")
    sr_base = os.path.join(tmp, "rust_base.seg.jsonl")
    _write_trace(tr_base, sites_r)
    _write_trace(tr, sites_r)
    out = _capture(lambda: cmd_summary(argparse.Namespace(trace=tr, top=5, side="rust", attack_re="")))
    check("rust-summary-rc", out[0] == 0, out[1][:200])
    check("rust-summary-attack", "attack_path=9" in out[1] and "noise=1" in out[1], out[1][:200])
    # --side rust 缺省 marker（反斜杠）分段： 3 段，每段 3 抽（marker + 2 路径）
    check("rust-segment-default-marker-rc",
          cmd_segment(argparse.Namespace(trace=tr, marker="", json=sr_base, side="rust", attack_re="")) == 0)
    segs = load_segments(sr_base)
    check("rust-segments-3", len(segs) == 3, "n=%d" % len(segs))
    check("rust-seg-shape", bool(segs) and segs[0]["draws_total"] == 3
          and segs[0]["draws_attack"] == 3 and segs[0]["draws_noise"] == 0, str(segs[:1]))
    # 归一：显式正斜杠 marker 也能命中反斜杠 trace（m3cs 口径）；单独落盘，勿覆盖基准
    sr_norm = os.path.join(tmp, "rust_norm.seg.jsonl")
    check("rust-marker-normalized-rc",
          cmd_segment(argparse.Namespace(trace=tr, marker="crates/world/src/combat.rs:109",
                                         json=sr_norm, side=None, attack_re="")) == 0)
    check("rust-marker-normalized-3", len(load_segments(sr_norm)) == 3)
    # 改坏：Rust 侧 k=2 段多抽一次噪声 ⇒ 必红且报「b 多抽 1」
    _write_trace(tr, sites_r[:4] + [RN] + sites_r[4:])
    cmd_segment(argparse.Namespace(trace=tr, marker="", json=sr, side="rust", attack_re=""))
    out = _capture(lambda: cmd_diff(argparse.Namespace(a=sr_base, b=sr, max_diffs=20)))
    check("rust-extra-draw-red", out[0] == 1, "rc=%d" % out[0])
    check("rust-extra-draw-shaped", "多抽 1" in out[1], out[1][:200])
    # 未知 --side ⇒ fail-closed（不静默降级成不过滤）
    out = _capture(lambda: cmd_summary(argparse.Namespace(trace=tr, top=5, side="bogus", attack_re="")))
    check("unknown-side-failclosed", out[0] == 2, "rc=%d" % out[0])

    # ---- F2（m3cs tick22）：Rust 攻击路径站点表必须含当前活栈站点（旧表漏 758/628/630/601/609/831/250）
    for line in (
        "758", "628", "630", "601", "609", "831", "250",
        # 2026-10-17 活栈行（magic-draws-move 补齐；936=命中骰 / 1018=Random(5) / 1126=毒门）
        "103", "106", "763", "771", "790", "792", "936", "1014", "1018", "1126",
        # 2026-10-19（bbf9694aa）起的三处当前活栈：命中骰 / AttackSuccess Random(5) / 毒门
        "1150", "1258",
    ):
        site = "crates\\world\\src\\combat.rs:" + line
        check("rust-site-%s-is-attack" % line, bool(RUST_ATTACK_RE.search(site)), site)
    check("rust-nonsite-not-attack",
          not bool(RUST_ATTACK_RE.search("crates\\world\\src\\animal.rs:10")),
          "animal.rs:10")
    # C# 侧同表：atk_power_base（BaseObject.GetAttackPower）必须算攻击路径。
    check("csharp-atk-power-base-is-attack",
          bool(CSHARP_ATTACK_RE.search(
              "M2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892")))

    # ---- 2026-10-17（卡 openmir2-magic-draws-move）：两处 `RUST_ATTACK_RE` **同表**自检 ----
    # 两文件各自维护同一张表（本文件 + `rng-trace-probe/rng_ledger_diff.py`）；只改单侧
    # 会让「攻击路径」口径在 groups / segment / diff 之间漂移。正文（`re.Pattern.pattern`）
    # 必须逐字相等 —— 改任一侧 ⇒ 本条红。
    check("rust-attack-re-identical-both-files",
          RUST_ATTACK_RE.pattern == rld.RUST_ATTACK_RE.pattern,
          "attack_rng_ledger=%r rng_ledger_diff=%r"
          % (RUST_ATTACK_RE.pattern, rld.RUST_ATTACK_RE.pattern))

    # ---- T-141（2026-10-21，卡 hit-site）：攻击链三处取数的**当前活栈**行必须归 attack ----
    # 表在 2026-10-17（`e7d432189` magic-draws-move）登记的 936/1018/1126 是**当时**的活栈：
    # 936 = `_Attack` 命中骰、1018 = `AttackSuccess` 的 `Random(5)`、1126 = 毒门。
    # `bbf9694aa`（2026-10-19）后三处分别在 1014 / 1150 / 1258；`6a774ea79` 只把命中骰从
    # 1002 移到 1007。**1018 从来不是命中骰行**。牙齿：从源码**现读**三处调用的物理行
    # （不写死行号），表漏行（含未来再漂移未同步）⇒ 必红；再用真 CLI（`summary --side rust`）
    # 跑 trace 夹具，证明当前活栈 + 历史别名全部被归 `attack_path`、无 noise。
    _combat_rs = os.path.normpath(os.path.join(
        os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "..",
        "mir2-rs", "crates", "world", "src", "combat.rs"))
    _needles = (
        ("hit-dice", "rng.random_byte(target.speed_point)"),
        ("attack-success-random5", "(rng.random_below(5) + 2) as u16"),
        ("attack-success-poison",
         "rng.random_below(i32::from(p.anti_poison) + config.attack_poison_rate)"),
    )
    _src_lines = []
    try:
        with open(_combat_rs, "r", encoding="utf-8", errors="replace") as _fh:
            _src_lines = list(_fh)
    except OSError:
        _src_lines = []
    _live_sites = []
    for _tag, _needle in _needles:
        _hits = [i for i, ln in enumerate(_src_lines, 1) if _needle in ln]
        check("rust-%s-live-line-found" % _tag, len(_hits) == 1,
              "combat.rs 中 `%s` 应恰有 1 处（实际行号 %r）" % (_needle, _hits))
        if len(_hits) != 1:
            continue
        _site = r"crates\world\src\combat.rs:%d" % _hits[0]
        _live_sites.append(_site)
        check("rust-%s-live-line-is-attack" % _tag,
              bool(RUST_ATTACK_RE.search(_site)), _site)
        _want_canon = "hit_gate" if _tag == "hit-dice" else "attack_success"
        check("rust-%s-live-line-canonical-%s" % (_tag, _want_canon),
              rld.canonical(_site) == _want_canon, _site)
    # 历史别名按 git 事实归类（`e7d432189` 时代）：936=hit_gate、1018/1126=attack_success。
    for _line, _want_canon in (("936", "hit_gate"), ("1018", "attack_success"),
                               ("1126", "attack_success")):
        _legacy = r"crates\world\src\combat.rs:" + _line
        check("rust-legacy-%s-is-attack" % _line,
              bool(RUST_ATTACK_RE.search(_legacy)), _legacy)
        check("rust-legacy-%s-canonical-%s" % (_line, _want_canon),
              rld.canonical(_legacy) == _want_canon, _legacy)
    # 真 CLI：marker + 三处当前活栈 + 三处历史别名 ⇒ 全部 attack_path、noise=0。
    _sites_trace = os.path.join(tmp, "attack_sites.tsv")
    _write_trace(_sites_trace, [RM] + _live_sites + [
        r"crates\world\src\combat.rs:936",
        r"crates\world\src\combat.rs:1018",
        r"crates\world\src\combat.rs:1126",
    ])
    _sites_out = _capture(lambda: cmd_summary(argparse.Namespace(
        trace=_sites_trace, top=8, side="rust", attack_re="")))
    _sites_want = "attack_path=%d" % (1 + len(_live_sites) + 3)
    check("rust-attack-sites-trace-all-attack",
          _sites_out[0] == 0 and _sites_want in _sites_out[1]
          and "noise=0" in _sites_out[1], _sites_out[1][:300])

    # ---- C# `AnimalObject.Struck@AnimalObject.cs:342` 的定位（2026-10-17）----
    # 它是**受害者**在 `Struck` 里重选目标的 `Random(6)`（`Operate(RM_STRUCK)` 在受害者消息段），
    # 不是攻方 `_Attack`/`GetAttackPower` 的伤害账本 ⇒ 不得匹配 `CSHARP_ATTACK_RE`；
    # 由 consumer_groups.tsv 的**精确行**归 `monster_ai`，保证含 :342 的 C# trace 不会
    # 成为无解释的 RED（与 Rust 侧 `monai.rs::animal_struck` 同组）。
    struck342 = "M2Server.Actor.AnimalObject.Struck@AnimalObject.cs:342"
    check("csharp-struck-342-not-attack",
          not bool(CSHARP_ATTACK_RE.search(struck342)),
          "AnimalObject.Struck@:342 不得匹配 CSHARP_ATTACK_RE")
    _tbl342 = rld._load_groups("")
    check("csharp-struck-342-group-monster_ai",
          _tbl342 is not None and _tbl342.group_of(struck342, "csharp") == "monster_ai",
          "AnimalObject.Struck@:342 应归 monster_ai")

    # ---- Claim B（owner 2026-10-13 09:35）：非攻击站点组口径（groups 子命令，与 rng_ledger_diff 同源）----
    RW = r"crates\world\src\monai.rs:164"
    RSP = r"crates\world\src\mongen.rs:624"
    RDR = r"crates\formula\src\drop.rs:109"
    RST = r"bins\game-svr\src\main.rs:2856"
    tg = os.path.join(tmp, "grp.tsv")
    _write_trace(tg, [RST, RSP, RW, RW, RDR])
    out = _capture(lambda: cmd_groups(argparse.Namespace(
        trace=tg, side="rust", groups_table="", no_attack=False, require_closed=True,
        top=5, json="")))
    check("groups-rust-rc", out[0] == 0, out[1][:300])
    check("groups-rust-closed", "GROUP_COVERAGE closed" in out[1], out[1][:400])
    check("groups-rust-monster_ai", "2  monster_ai" in out[1], out[1][:400])
    # 改坏必红：未归类站点 ⇒ UNCLASSIFIED + require-closed rc=1
    _write_trace(tg, [RST, "crates/foo.rs:1"])
    out = _capture(lambda: cmd_groups(argparse.Namespace(
        trace=tg, side="rust", groups_table="", no_attack=False, require_closed=True,
        top=5, json="")))
    check("groups-rust-drift-red", out[0] == 1 and "UNCLASSIFIED" in out[1], out[1][:400])
    # reviewer-5 O-1（委派同一实现）：解析不了的行 ⇒ rows_dropped 计入且 require-closed 判红
    tdrop = os.path.join(tmp, "grp-drop.tsv")
    with open(tdrop, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        fh.write("5\tNext(max)\t10\t0\t%s\n" % RW)
        fh.write("oops\tNext(max)\t10\t0\tcrates/foo.rs:1\n")
    out = _capture(lambda: cmd_groups(argparse.Namespace(
        trace=tdrop, side="rust", groups_table="", no_attack=False, require_closed=True,
        top=5, json="")))
    check("groups-rust-rowcoverage-red",
          out[0] == 1 and "rows_dropped=1" in out[1] and "ROW_COVERAGE DRIFT" in out[1],
          out[1][:500])

    # ---- A3（卡 openmir2-window-collector-preflight）：流水形态 fail-closed（缺列/缺 marker）----
    t4col = os.path.join(tmp, "shape4.tsv")
    with open(t4col, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        fh.write("2\tNext(max)\t10\t3\n")            # 4 列：手工起 C# 忘设 MIR2_RNG_SITE
    rc_s, _so_s, se_s = _capture_both(lambda: cmd_summary(
        argparse.Namespace(trace=t4col, top=5, side="csharp", attack_re="")))
    check("A3-summary-4col-red", rc_s != 0, "rc=%d" % rc_s)
    check("A3-summary-4col-names-cols", "got 4 cols" in se_s, se_s[:200].replace("\n", " "))
    rc_g, _so_g, se_g = _capture_both(lambda: cmd_segment(
        argparse.Namespace(trace=t4col, marker=M, json=os.path.join(tmp, "s4.jsonl"),
                           side=None, attack_re="")))
    check("A3-segment-4col-red", rc_g != 0, "rc=%d" % rc_g)
    check("A3-segment-4col-names-cols", "got 4 cols" in se_g, se_g[:200].replace("\n", " "))
    # 5 列但无 marker ⇒ fail-closed 点名 marker
    t5nm = os.path.join(tmp, "shape5nomarker.tsv")
    with open(t5nm, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        fh.write("2\tNext(max)\t10\t3\tSome.Other@x.cs:1\n")
    rc_m, _so_m, se_m = _capture_both(lambda: cmd_segment(
        argparse.Namespace(trace=t5nm, marker=M, json=os.path.join(tmp, "s5.jsonl"),
                           side=None, attack_re="")))
    check("A3-no-marker-red", rc_m != 0, "rc=%d" % rc_m)
    check("A3-no-marker-named", "no marker" in se_m, se_m[:200].replace("\n", " "))
    # 对照：干净 5 列 + marker ⇒ 绿
    rc_ok, _so_ok, _se_ok = _capture_both(lambda: cmd_summary(
        argparse.Namespace(trace=ta, top=5, side="csharp", attack_re="")))
    check("A3-clean-5col-green", rc_ok == 0, "rc=%d" % rc_ok)
    for name in os.listdir(tmp):
        try:
            os.remove(os.path.join(tmp, name))
        except OSError:
            pass
    os.rmdir(tmp)
    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: attack_rng_ledger selftest %d/%d（C#+Rust 两侧：同账本绿 + 多抽/少抽必红且定位 + 段数不等必红 + 未知 side fail-closed + F2 站点表同表：新站点分类为 attack / 非站点为 noise + Claim B 站点组：closed 绿 / 未归类必红 / 行级覆盖 rows_dropped fail-closed（O-1）+ 两处 RUST_ATTACK_RE 逐字同表（改单侧必红）+ C# AnimalObject.Struck@:342 非攻击且归 monster_ai）"
          % (len(checks), len(checks)))
    return 0


def _capture(fn):
    import io
    import contextlib
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = fn()
    return rc, buf.getvalue()


def _capture_both(fn):
    """同上，但同时捕获 stderr（A3 形态门的诊断走 stderr）。"""
    import io
    import contextlib
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        rc = fn()
    return rc, out.getvalue(), err.getvalue()


def _die(msg, rc=2):
    print(msg, file=sys.stderr)
    return rc


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 attack RNG draw ledger")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_sum = sub.add_parser("summary")
    p_sum.add_argument("trace")
    p_sum.add_argument("--side", choices=("csharp", "rust"), default=None,
                       help="站点表口径（缺省 csharp；rust 用 m3cs 决策片同表）")
    p_sum.add_argument("--attack-re", default="", help="覆盖攻击路径正则")
    p_sum.add_argument("--top", type=int, default=8)
    p_sum.set_defaults(func=cmd_summary)

    p_seg = sub.add_parser("segment")
    p_seg.add_argument("trace")
    p_seg.add_argument("--marker", default="", help="分段锚（--side rust 时缺省 combat.rs:109）")
    p_seg.add_argument("--side", choices=("csharp", "rust"), default=None)
    p_seg.add_argument("--attack-re", default="", help="覆盖攻击路径正则")
    p_seg.add_argument("--json", default="")
    p_seg.set_defaults(func=cmd_segment)

    p_diff = sub.add_parser("diff")
    p_diff.add_argument("a")
    p_diff.add_argument("b")
    p_diff.add_argument("--max-diffs", type=int, default=20)
    p_diff.set_defaults(func=cmd_diff)

    p_st = sub.add_parser("selftest")
    p_st.set_defaults(func=cmd_selftest)

    p_grp = sub.add_parser("groups")
    p_grp.add_argument("trace")
    p_grp.add_argument("--side", choices=("csharp", "rust"), default="csharp",
                       help="站点表口径（缺省 csharp）")
    p_grp.add_argument("--groups-table", default="")
    p_grp.add_argument("--no-attack", action="store_true")
    p_grp.add_argument("--require-closed", action="store_true")
    p_grp.add_argument("--top", type=int, default=10)
    p_grp.add_argument("--json", default="")
    p_grp.set_defaults(func=cmd_groups)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
