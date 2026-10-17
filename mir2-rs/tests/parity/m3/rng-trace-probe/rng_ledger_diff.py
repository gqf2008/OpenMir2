#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""rng_ledger_diff.py — M3②③ 真机**两侧 RNG 账本**的站点 + 顺序 diff（owner 2026-10-12 18:25）。

背景：决策片（`evidence-rng-trace-20261012`）只跑了 **Rust 离线单线程** 的 trace，owner 指出
「Rust 生产活栈的账本仍缺失」⇒ 本工具把 **C# 活栈**（`rng-hook` 的 `rng-stream.tsv`）与
**Rust 活栈**（`crates/shared/src/rng_trace.rs` 的 `MIR2_RNG_LOG`）按 **site + 顺序** 对齐：

  summary <trace.tsv> [--side csharp|rust] [--json out]
      总数 + 攻击路径/噪声占比 + 全站点直方图（口径与 `attack_rng_ledger.py` 同表）。
  order  <trace.tsv> [--side csharp|rust] [--attack-only] [--out order.txt]
      把 trace 的 site 序列**规范化**成 canonical 名（见 `CANON_RULES`）后逐行输出；
      `--attack-only` 只留攻击路径（两侧可映射到同一 vocabulary）。
  diff   <a.tsv> <b.tsv> [--attack-only] [--json out]
      顺序敏感：按 canonical 序列**逐位**比，报**第一处分歧**（第几位、两侧各是什么 site；
      `--attack-only` 时两侧 vocabulary 同表，可直接判「缺消费者 / 次序不同」）；
      另报两侧 per-canonical 计数、只在 A / 只在 B 的 canonical（= 缺消费者的点名清单）。
      按站点组汇总（Claim B）：组集合 + 未归类 + **行级覆盖**（`rows_dropped`）。
      `--require-closed` = 未归类站点 **或** 解析不了的行 > 0 即 fail-closed（表/格式漂移不给假绿）。
  groupdiff <a.tsv> <b.tsv> [--include-attack] [--strict] [--fold] [--json out]
      两侧组账本：组集合 diff + **共享组相对次序** diff（仅两侧共有的组参与）+ run 序。
      `--strict` 下三种任一即红：GROUP_SET_DIFF / SHARED_REL_ORDER_DIFF / ROW_COVERAGE_DRIFT。
      口径：**组内 draw 计数本身不是判据**，只有组集合与共享组相对次序是。
  `--fold`（R-6，owner 2026-10-13 裁决）：先把两侧组序列经 `rng_consumer_groups.FOLD_RULES`
      折叠（现为 `construct -> spawn`，对应 Rust 把构造期抽数折叠进 `AnimalState::from_spawn_draws`），
      **在折叠后的序列上**重算组集合 + 组计数 + 共享组相对次序；折叠后**计数不等** ⇒ `FOLDED_COUNT_DIFF`。
      折叠不是豁免：只写白名单不校验、或折叠后计数/次序不等 ⇒ 照样 RED。缺省（不传 `--fold`）
      行为与既有逐字节相同（`construct` vs `spawn` 仍如实报 `GROUP_SET_DIFF`）。
  selftest
      合成件：同序 ⇒ 绿；插入一次取数（次序漂移）⇒ 必红且定位到第几位；缺一个站点 ⇒ 必红且点名。

退出码：0 = GREEN（分析完成 / 自检过）；1 = RED（自检失败）；2 = PREREQ（文件缺失/解析空）。

站点表与 m3cs 决策片 `analyze_rng_trace.py` / m3bag `attack_rng_ledger.py` **同口径**
（C# = `PlayObject.GetAttackPower@PlayObject.cs:2929` 为每次攻击恰一次的 marker；
Rust = `crates\world\src\combat.rs:109`）。
"""
import argparse
import collections
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rng_consumer_groups as rcg  # noqa: E402

# C# 站点（含 `@类名.cs:行号` 全名）。
CSHARP_ATTACK_RE = re.compile(
    r"(PlayObject\.GetAttackPower@|BaseObject\._Attack@|PlayObject\.StruckDamage@|"
    r"BaseObject\.GetHitStruckDamage@|PlayObject\.AttackSuccess@|PlayObject\.AttackDir@|"
    r"BaseObject\.GetAttackPower@)")
# Rust 站点（世界层调用点）。
RUST_ATTACK_RE = re.compile(r"combat\.rs:(67|109|111|250|579|587|601|606|608|609|628|630|736|758|831)")

CSHARP_MARKER = "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929"
RUST_MARKER = r"crates\world\src\combat.rs:109"

# canonical 映射：两侧语义一一对应的站点 → 同一名（顺序敏感比对才有仲裁力）。
CANON_RULES = [
    # C#（oracle）
    (re.compile(r"M2Server\.Player\.PlayObject\.GetAttackPower@PlayObject\.cs:2929$"), "atk_power_a"),
    (re.compile(r"M2Server\.Player\.PlayObject\.GetAttackPower@PlayObject\.cs:2934$"), "atk_power_b"),
    (re.compile(r"M2Server\.Actor\.BaseObject\.GetAttackPower@BaseObject\.Base\.cs:892$"), "atk_power_base"),
    (re.compile(r"M2Server\.Actor\.BaseObject\._Attack@BaseObject\.Attack\.cs:41$"), "hit_gate"),
    (re.compile(r"M2Server\.Player\.PlayObject\.AttackSuccess@PlayObject\.Attack\.cs:223$"), "attack_success"),
    (re.compile(r"M2Server\.Player\.PlayObject\.StruckDamage@PlayObject\.cs:2957$"), "struck_durability"),
    (re.compile(r"M2Server\.Actor\.BaseObject\.GetHitStruckDamage@BaseObject\.Base\.cs:792$"), "hit_struck_damage"),
    # Rust（世界层）
    (re.compile(r"crates/world/src/combat\.rs:109$"), "atk_power_a"),
    (re.compile(r"crates/world/src/combat\.rs:111$"), "atk_power_b"),
    (re.compile(r"crates/world/src/combat\.rs:67$"), "atk_power_base"),
    (re.compile(r"crates/world/src/combat\.rs:758$"), "hit_gate"),
    (re.compile(r"crates/world/src/combat\.rs:628$"), "hit_struck_damage"),
    (re.compile(r"crates/world/src/combat\.rs:630$"), "hit_struck_damage2"),
    (re.compile(r"crates/world/src/combat\.rs:601$"), "struck_durability"),
    (re.compile(r"crates/world/src/combat\.rs:609$"), "struck_durability_slot"),
    (re.compile(r"crates/world/src/combat\.rs:831$"), "attack_success"),
    # 旧代码 trace 的行号别名（本片把 attack_success/attack_dir 加进 combat.rs 后行号位移；
    # 2026-10-12 之前采集的 Rust 活栈 trace 仍按旧行号落盘，别名保证旧件可复算）。
    (re.compile(r"crates/world/src/combat\.rs:736$"), "hit_gate"),
    (re.compile(r"crates/world/src/combat\.rs:606$"), "hit_struck_damage"),
    (re.compile(r"crates/world/src/combat\.rs:608$"), "hit_struck_damage2"),
    (re.compile(r"crates/world/src/combat\.rs:579$"), "struck_durability"),
    (re.compile(r"crates/world/src/combat\.rs:587$"), "struck_durability_slot"),
    (re.compile(r"crates/world/src/combat\.rs:250$"), "attack_skill_point_count"),
]

ATTACK_CANON = {
    "atk_power_a", "atk_power_b", "atk_power_base", "hit_gate", "attack_success",
    "hit_struck_damage", "hit_struck_damage2", "struck_durability", "struck_durability_slot",
    "attack_skill_point_count",
}

SIDES = {"csharp": CSHARP_ATTACK_RE, "rust": RUST_ATTACK_RE}


def norm(site):
    return (site or "").replace("\\", "/")


def canonical(site):
    s = norm(site)
    for rx, name in CANON_RULES:
        if rx.search(s):
            return name
    return s


def parse_trace(path):
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


def is_attack(site, side):
    rx = SIDES[side]
    return bool(rx.search(norm(site)))


def cmd_summary(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    side = args.side
    attack = sum(1 for r in rows if is_attack(r[4], side))
    hist = collections.Counter(norm(r[4]) or "(no-site)" for r in rows)
    canon = collections.Counter(canonical(r[4]) for r in rows)
    print("RNG_LEDGER %s side=%s total=%d attack_path=%d noise=%d noise_ratio=%.4f distinct_sites=%d"
          % (args.trace, side, len(rows), attack, len(rows) - attack,
             (len(rows) - attack) / len(rows), len(hist)))
    print("  CANONICAL %s" % dict(sorted(canon.items(), key=lambda kv: -kv[1])))
    for site, n in hist.most_common(args.top):
        print("  SITE %8d  %s" % (n, site))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"trace": args.trace, "side": side, "total": len(rows), "attack_path": attack,
                       "noise": len(rows) - attack, "sites": dict(hist), "canonical": dict(canon)},
                      fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def cmd_order(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    rows = parse_trace(args.trace)
    if not rows:
        return _die("PREREQ_MISSING: %s 解析为空" % args.trace)
    seq = []
    for _seq, _kind, _arg, _res, site in rows:
        if args.attack_only and not is_attack(site, args.side):
            continue
        seq.append(canonical(site))
    out = args.out or (os.path.splitext(args.trace)[0] + (".attack.order.txt" if args.attack_only else ".order.txt"))
    with open(out, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(seq) + ("\n" if seq else ""))
    print("RNG_ORDER %s side=%s attack_only=%s draws=%d -> %s" % (args.trace, args.side, args.attack_only, len(seq), out))
    return 0


def _segments(path, side):
    """把攻击路径按 marker（`atk_power_a`，每次攻击恰一次）逐击分段。

    返回 `[(seg_idx, (canonical, ...)), ...]`；marker 之前的预热/怪物初始化取数不计入任何段。
    """
    rows = parse_trace(path)
    segs = []
    cur = None
    for _seq, _kind, _arg, _res, site in rows:
        if not is_attack(site, side):
            continue
        c = canonical(site)
        if c == "atk_power_a":
            if cur is not None:
                segs.append((len(segs) + 1, tuple(cur)))
            cur = [c]
        elif cur is not None:
            cur.append(c)
    if cur is not None:
        segs.append((len(segs) + 1, tuple(cur)))
    return segs


def cmd_segments(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    segs = _segments(args.trace, args.side)
    print("RNG_SEGMENTS %s side=%s segments=%d" % (args.trace, args.side, len(segs)))
    shapes = collections.Counter(s for _k, s in segs)
    for shape, n in shapes.most_common():
        print("  SHAPE x%-4d %s" % (n, list(shape)))
    for k, s in segs[: args.head]:
        print("  SEG k=%d %s" % (k, list(s)))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"trace": args.trace, "side": args.side, "segments": len(segs),
                       "shapes": {"|".join(k): v for k, v in shapes.items()},
                       "per_seg": [{"k": k, "shape": list(s)} for k, s in segs]},
                      fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def cmd_segdiff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到 %s" % p)
    sa = _segments(args.a, args.a_side)
    sb = _segments(args.b, args.b_side)
    if not sa or not sb:
        print("RED: 空段账本（a=%d b=%d）" % (len(sa), len(sb)))
        return 1
    shapes_a = collections.Counter(s for _k, s in sa)
    shapes_b = collections.Counter(s for _k, s in sb)
    only_a = {k: v for k, v in shapes_a.items() if k not in shapes_b}
    only_b = {k: v for k, v in shapes_b.items() if k not in shapes_a}
    first = None
    for i in range(min(len(sa), len(sb))):
        if sa[i][1] != sb[i][1]:
            first = i
            break
    print("RNG_SEGDIFF a=%s(%d segments) b=%s(%d segments)" % (args.a, len(sa), args.b, len(sb)))
    print("  A_SHAPES %s" % {_shape_name(k): v for k, v in shapes_a.most_common()})
    print("  B_SHAPES %s" % {_shape_name(k): v for k, v in shapes_b.most_common()})
    print("  SHAPES_ONLY_IN_A %s" % {_shape_name(k): v for k, v in only_a.items()})
    print("  SHAPES_ONLY_IN_B %s" % {_shape_name(k): v for k, v in only_b.items()})
    if first is None:
        print("  FIRST_SEG_DIVERGENCE none（段形状逐位全等至 min=%d）" % min(len(sa), len(sb)))
        verdict = "SEG_SHAPES_IDENTICAL_PREFIX" if len(sa) != len(sb) else "SEG_IDENTICAL"
    else:
        print("  FIRST_SEG_DIVERGENCE seg=%d  a=%s  b=%s"
              % (first + 1, list(sa[first][1]), list(sb[first][1])))
        verdict = "SEG_SHAPE_DIVERGENCE"
    print("  SEG_COUNT a=%d b=%d verdict=%s" % (len(sa), len(sb), verdict))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({
                "a": args.a, "b": args.b, "a_segments": len(sa), "b_segments": len(sb),
                "first_seg_divergence": first,
                "a_shapes": {"|".join(k): v for k, v in shapes_a.items()},
                "b_shapes": {"|".join(k): v for k, v in shapes_b.items()},
                "shapes_only_in_a": {"|".join(k): v for k, v in only_a.items()},
                "shapes_only_in_b": {"|".join(k): v for k, v in only_b.items()},
                "verdict": verdict,
            }, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def _shape_name(shape):
    return ">".join(shape)


def _load_seq(path, side, attack_only):
    rows = parse_trace(path)
    seq = []
    for _seq, _kind, _arg, _res, site in rows:
        if attack_only and not is_attack(site, side):
            continue
        seq.append((canonical(site), norm(site)))
    return seq


def cmd_diff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到 %s" % p)
    a = _load_seq(args.a, args.a_side, args.attack_only)
    b = _load_seq(args.b, args.b_side, args.attack_only)
    if not a or not b:
        print("RED: 空账本（a=%d b=%d）" % (len(a), len(b)))
        return 1
    ca = collections.Counter(x[0] for x in a)
    cb = collections.Counter(x[0] for x in b)
    only_a = {k: v for k, v in ca.items() if k not in cb}
    only_b = {k: v for k, v in cb.items() if k not in ca}
    # 顺序敏感：逐位比，找第一处分歧。
    first = None
    for i in range(min(len(a), len(b))):
        if a[i][0] != b[i][0]:
            first = i
            break
    print("RNG_LEDGER_DIFF attack_only=%s a=%s(%d) b=%s(%d)"
          % (args.attack_only, args.a, len(a), args.b, len(b)))
    print("  A_CANONICAL %s" % dict(sorted(ca.items(), key=lambda kv: -kv[1])))
    print("  B_CANONICAL %s" % dict(sorted(cb.items(), key=lambda kv: -kv[1])))
    print("  ONLY_IN_A(missing_in_B) %s" % dict(sorted(only_a.items(), key=lambda kv: -kv[1])))
    print("  ONLY_IN_B(extra_in_B)   %s" % dict(sorted(only_b.items(), key=lambda kv: -kv[1])))
    if first is None:
        if len(a) == len(b):
            print("  FIRST_DIVERGENCE none（逐位全等，len=%d）" % len(a))
            verdict = "IDENTICAL"
        else:
            print("  FIRST_DIVERGENCE 前缀全等；a 长 %d、b 长 %d（b 在 index %d 之后缺/多）"
                  % (len(a), len(b), min(len(a), len(b))))
            verdict = "PREFIX_EQUAL_LENGTH_DIFF"
    else:
        print("  FIRST_DIVERGENCE index=%d  a=%s  b=%s" % (first, a[first][1], b[first][1]))
        print("    context a[%d:%d]=%s" % (max(0, first - 2), first + 2,
                                          [x[0] for x in a[max(0, first - 2):first + 2]]))
        print("    context b[%d:%d]=%s" % (max(0, first - 2), first + 2,
                                          [x[0] for x in b[max(0, first - 2):first + 2]]))
        verdict = "ORDER_DIVERGENCE"
    print("  VERDICT %s" % verdict)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({
                "attack_only": bool(args.attack_only), "a": args.a, "b": args.b,
                "a_len": len(a), "b_len": len(b), "first_divergence": first,
                "a_canonical": dict(ca), "b_canonical": dict(cb),
                "only_in_a": only_a, "only_in_b": only_b, "verdict": verdict,
            }, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    return 0


def _load_groups(path):
    try:
        return rcg.load(path or None)
    except (OSError, ValueError) as exc:
        print("PREREQ_MISSING: 消费者组表加载失败: %s" % exc, file=sys.stderr)
        return None


def cmd_groups(args):
    if not os.path.exists(args.trace):
        return _die("PREREQ_MISSING: 找不到 %s" % args.trace)
    table = _load_groups(args.groups_table)
    if table is None:
        return 2
    row_stats = {}
    counts, unclassified, rows_total = rcg.summarize(
        args.trace, args.side, table, is_attack, include_attack=not args.no_attack, stats=row_stats)
    rows_dropped = row_stats.get("rows_dropped", 0)
    kept = sum(counts.values())
    print("RNG_CONSUMER_GROUPS %s side=%s rows_total=%d rows_dropped=%d kept=%d include_attack=%s groups=%d"
          % (args.trace, args.side, rows_total, rows_dropped, kept, not args.no_attack, len(counts)))
    for g, n in counts.most_common():
        print("  GROUP %9d  %s" % (n, g))
    if unclassified:
        print("  GROUP_COVERAGE DRIFT unclassified=%d" % sum(unclassified.values()))
        for site, n in unclassified.most_common(args.top):
            print("  UNCLASSIFIED %7d  %s" % (n, site))
    else:
        print("  GROUP_COVERAGE closed")
    # reviewer-5 O-1：`closed` 只证明「已解析行全归类」；行级覆盖必须单独报。
    if rows_dropped:
        print("  ROW_COVERAGE DRIFT rows_dropped=%d（非空、非已知 marker、但解析不了 ⇒ 行级覆盖 fail-closed）"
              % rows_dropped)
    else:
        print("  ROW_COVERAGE closed rows_dropped=0")
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"trace": args.trace, "side": args.side, "rows_total": rows_total,
                       "rows_dropped": rows_dropped,
                       "kept": kept, "include_attack": not args.no_attack,
                       "groups": dict(counts), "unclassified": dict(unclassified),
                       "table": table.path}, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    if args.require_closed and (unclassified or rows_dropped):
        if unclassified:
            print("RED: 有 %d 个未归类站点 ⇒ 组表漂移（--require-closed 拒给假绿）" % sum(unclassified.values()))
        if rows_dropped:
            print("RED: 有 %d 行解析不了（既非站点也非已知 marker）⇒ 行级覆盖 fail-closed" % rows_dropped)
        return 1
    return 0


def _scenario_group_expectations(path):
    """从 scenario JSON 读 DoD② 的机判输入（`trace.groups_expected` / `min_monster_ai_draws`）。

    返回 `(expected: list[str], min_counts: dict[str, int])`；字段缺省 ⇒ 空。
    这两项是**声明**，必须由本工具在**真实账本**上机判「真的出现 / 抽数达标」，否则它们只是注释。
    """
    with open(path, "r", encoding="utf-8-sig") as fh:
        obj = json.load(fh)
    trace = obj.get("trace") if isinstance(obj, dict) else None
    if not isinstance(trace, dict):
        return [], {}
    raw = trace.get("groups_expected")
    expected = [str(g) for g in raw] if isinstance(raw, list) else []
    min_counts = {}
    mgd = trace.get("min_group_draws")
    if isinstance(mgd, dict):
        for g, n in mgd.items():
            try:
                min_counts[str(g)] = int(n)
            except (TypeError, ValueError):
                raise ValueError("trace.min_group_draws[%r] 不是整数: %r" % (g, n))
    n = trace.get("min_monster_ai_draws")
    if n is not None:
        try:
            min_counts["monster_ai"] = int(n)
        except (TypeError, ValueError):
            raise ValueError("trace.min_monster_ai_draws 不是整数: %r" % (n,))
    return expected, min_counts


def cmd_groupdiff(args):
    for p in (args.a, args.b):
        if not os.path.exists(p):
            return _die("PREREQ_MISSING: 找不到 %s" % p)
    # DoD② 的声明源：scenario JSON（可选）。给了就必须能读、组名必须在册、账本里必须真的出现。
    scenario = getattr(args, "scenario", "") or ""
    expected, min_counts = [], {}
    if scenario:
        if not os.path.exists(scenario):
            return _die("PREREQ_MISSING: 找不到 scenario %s" % scenario)
        try:
            expected, min_counts = _scenario_group_expectations(scenario)
        except (OSError, ValueError) as exc:
            return _die("PREREQ_MISSING: scenario 读取失败 %s: %s" % (scenario, exc))
        unknown = sorted({g for g in list(expected) + list(min_counts) if g not in rcg.KNOWN_GROUPS})
        if unknown:
            return _die("PREREQ_MISSING: scenario 声明了账本不认识的组 %s（已知：%s）"
                        % (unknown, "、".join(rcg.KNOWN_GROUPS)))
    table = _load_groups(args.groups_table)
    if table is None:
        return 2
    include_attack = bool(args.include_attack)
    stats_a, stats_b = {}, {}
    a = rcg.sequence(args.a, args.a_side, table, is_attack, include_attack=include_attack, stats=stats_a)
    b = rcg.sequence(args.b, args.b_side, table, is_attack, include_attack=include_attack, stats=stats_b)
    dropped_a = stats_a.get("rows_dropped", 0)
    dropped_b = stats_b.get("rows_dropped", 0)
    if not a or not b:
        print("RED: 空组账本（a=%d b=%d）" % (len(a), len(b)))
        return 1
    # R-6 folded 映射（owner 2026-10-13）：显式开关；关时逐字节保持既有行为。
    fold = bool(getattr(args, "fold", False))
    if fold:
        # 折叠后的序列（供相对次序判据）与折叠后的计数都从**共享模块**的同一组规则来：
        # 计数走 `rcg.folded_counts()`（而不是就地 `Counter(folded(seq))`），让「折叠→计数」只有
        # 一处实现（reviewer-6 residual Minor：此前 `folded_counts()` 是未被调用的死辅助函数）。
        raw_a, raw_b = a, b
        a = rcg.folded(a)
        b = rcg.folded(b)
        ca = rcg.folded_counts(collections.Counter(raw_a))
        cb = rcg.folded_counts(collections.Counter(raw_b))
    else:
        ca, cb = collections.Counter(a), collections.Counter(b)
    only_a = {k: v for k, v in ca.items() if k not in cb}
    only_b = {k: v for k, v in cb.items() if k not in ca}
    shared = set(ca) & set(cb)
    # 相对次序：只在**两侧都出现的组**上比（缺组由 GROUP_SET 报）。
    fa = [g for g in a if g in shared]
    fb = [g for g in b if g in shared]
    first_shared = rcg.first_divergence(fa, fb)
    ra, rb = rcg.rle(a), rcg.rle(b)
    runs_a = [g for g, _n in ra]
    runs_b = [g for g, _n in rb]
    first_run = rcg.first_divergence(runs_a, runs_b)
    # R-6 要求的「折叠计数」：折叠后逐组计数必须相等（白名单豁免不算达成）。
    folded_count_diff = {}
    if fold:
        for g in sorted(set(ca) | set(cb)):
            na, nb = ca.get(g, 0), cb.get(g, 0)
            if na != nb:
                folded_count_diff[g] = (na, nb)
    set_ok = not only_a and not only_b
    order_ok = (len(fa) == len(fb)) and first_shared is None
    # DoD②：scenario 声明的 `groups_expected` / `min_monster_ai_draws` 必须在**真实账本**里成立。
    # 「两侧都没有 monster_ai/drop」在旧口径下是真空 IDENTICAL（组集合彼此相等）⇒ 假绿。
    expected_missing = {}
    if expected:
        for label, cnt in (("a", ca), ("b", cb)):
            miss = [g for g in expected if cnt.get(g, 0) <= 0]
            if miss:
                expected_missing[label] = miss
    min_violations = {}
    if min_counts:
        for g in sorted(min_counts):
            want = min_counts[g]
            na, nb = ca.get(g, 0), cb.get(g, 0)
            if na < want or nb < want:
                min_violations[g] = {"want": want, "a": na, "b": nb}
    flags = []
    if not set_ok:
        flags.append("GROUP_SET_DIFF")
    if not order_ok:
        flags.append("SHARED_REL_ORDER_DIFF")
    if folded_count_diff:
        flags.append("FOLDED_COUNT_DIFF")
    if expected_missing:
        flags.append("MISSING_EXPECTED_GROUPS")
    if min_violations:
        flags.append("MIN_GROUP_DRAWS")
    # reviewer-5 O-1：行级覆盖也是结构判据的一部分（解析不了的行 = 被吞掉的消费者）。
    if dropped_a or dropped_b:
        flags.append("ROW_COVERAGE_DRIFT")
    verdict = "IDENTICAL" if not flags else "+".join(flags)
    print("RNG_CONSUMER_GROUPDIFF include_attack=%s fold=%s a=%s(%d) b=%s(%d)"
          % (include_attack, fold, args.a, len(a), args.b, len(b)))
    if fold:
        print("  FOLD_MAP %s（R-6：C# construct 折叠到 Rust spawn；折叠后计数+相对次序必须相等）"
              % dict(sorted(rcg.FOLD_RULES.items())))
    print("  A_GROUPS %s" % dict(sorted(ca.items(), key=lambda kv: -kv[1])))
    print("  B_GROUPS %s" % dict(sorted(cb.items(), key=lambda kv: -kv[1])))
    print("  ONLY_IN_A(missing_in_B) %s" % dict(sorted(only_a.items(), key=lambda kv: -kv[1])))
    print("  ONLY_IN_B(extra_in_B)   %s" % dict(sorted(only_b.items(), key=lambda kv: -kv[1])))
    print("  SHARED_GROUPS %s" % sorted(shared))
    if fold:
        if folded_count_diff:
            print("  FOLDED_COUNT_DIFF %s（折叠后逐组计数必须相等；不等 ⇒ 折叠不成立）"
                  % {g: {"a": na, "b": nb} for g, (na, nb) in folded_count_diff.items()})
        else:
            print("  FOLDED_COUNT_EQUAL %s" % dict(sorted(ca.items(), key=lambda kv: -kv[1])))
    if first_shared is None and len(fa) == len(fb):
        print("  FIRST_SHARED_ORDER_DIVERGENCE none（共享组相对次序逐位全等，len=%d）" % len(fa))
    else:
        print("  FIRST_SHARED_ORDER_DIVERGENCE index=%s a=%s b=%s"
              % (first_shared, fa[first_shared] if first_shared is not None else "(len差)",
                 fb[first_shared] if first_shared is not None else "(len差)"))
    print("  RUNS a=%d b=%d" % (len(ra), len(rb)))
    print("  ROW_COVERAGE a_dropped=%d b_dropped=%d" % (dropped_a, dropped_b))
    if expected:
        print("  EXPECTED_GROUPS %s（scenario 声明；DoD② 要求两侧都出现）" % list(expected))
        if expected_missing:
            print("  MISSING_EXPECTED_GROUPS %s（声明的组未在账本出现 ⇒ fail-closed）" % expected_missing)
        else:
            print("  EXPECTED_GROUPS_PRESENT both %s" % list(expected))
    if min_counts:
        if min_violations:
            print("  MIN_GROUP_DRAWS %s（声明的最小抽数未达 ⇒ fail-closed）" % min_violations)
        else:
            print("  MIN_GROUP_DRAWS_OK %s"
                  % {g: {"want": min_counts[g], "a": ca.get(g, 0), "b": cb.get(g, 0)}
                     for g in sorted(min_counts)})
    if first_run is None and len(runs_a) == len(runs_b):
        print("  FIRST_RUN_DIVERGENCE none（组 run 序逐位全等）")
    else:
        print("  FIRST_RUN_DIVERGENCE run=%s a=%s b=%s"
              % (first_run, runs_a[first_run] if first_run is not None else "(len差)",
                 runs_b[first_run] if first_run is not None else "(len差)"))
    print("  VERDICT %s" % verdict)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"include_attack": include_attack, "fold": fold,
                       "fold_rules": dict(sorted(rcg.FOLD_RULES.items())),
                       "folded_count_diff": {g: {"a": v[0], "b": v[1]}
                                             for g, v in folded_count_diff.items()},
                       "a": args.a, "b": args.b,
                       "a_len": len(a), "b_len": len(b),
                       "a_dropped": dropped_a, "b_dropped": dropped_b,
                       "a_groups": dict(ca), "b_groups": dict(cb),
                       "only_in_a": only_a, "only_in_b": only_b,
                       "shared_groups": sorted(shared),
                       "first_shared_order_divergence": first_shared,
                       "a_runs": len(ra), "b_runs": len(rb),
                       "first_run_divergence": first_run,
                       "scenario": scenario,
                       "expected_groups": list(expected),
                       "expected_groups_missing": expected_missing,
                       "min_group_draws": {g: {"want": min_counts[g], "a": ca.get(g, 0),
                                                "b": cb.get(g, 0)} for g in sorted(min_counts)},
                       "verdict": verdict, "table": table.path},
                      fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    if args.strict and flags:
        print("RED: 组结构不一致 ⇒ %s（--strict）" % verdict)
        return 1
    return 0


def _write_trace(path, sites):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        for i, site in enumerate(sites, 1):
            fh.write("%d\tNext(max)\t10\t%d\t%s\n" % (i, i % 10, site))


def cmd_selftest(args):
    import contextlib
    import io
    import tempfile
    tmp = tempfile.mkdtemp(prefix="rng-ledger-diff-selftest-")
    checks, failures = [], []

    def check(name, cond, detail=""):
        checks.append(name)
        if not cond:
            failures.append(name + (" (%s)" % detail if detail else ""))

    def cap(fn):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
            rc = fn()
        return rc, buf.getvalue()

    # C# 侧 3 次攻击，每次 [a,b,hit_gate]；Rust 侧同序。
    C = ["M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929",
         "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934",
         "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41"]
    R = [r"crates\world\src\combat.rs:109", r"crates\world\src\combat.rs:111",
         r"crates\world\src\combat.rs:758"]
    sites_a = C * 3
    sites_b = R * 3
    ta = os.path.join(tmp, "a.tsv")
    tb = os.path.join(tmp, "b.tsv")
    _write_trace(ta, sites_a)
    _write_trace(tb, sites_b)
    rc, out = cap(lambda: cmd_diff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", attack_only=True, json="")))
    check("identical-green", rc == 0 and "IDENTICAL" in out, out[:300])

    # 次序漂移：b 在 index 1 处插入一条 hit_gate（取数次序被推移）⇒ 第一处分歧 index=1。
    _write_trace(tb, R[:1] + [R[2]] + R[1:] + R * 2)
    rc, out = cap(lambda: cmd_diff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", attack_only=True, json="")))
    check("order-shift-red", rc == 0 and "FIRST_DIVERGENCE index=1" in out, out[:400])

    # 缺消费者：b 少一个 struck_durability ⇒ only_in_a 点名 + 第一处分歧。
    _write_trace(tb, R * 3)
    _write_trace(ta, C[:2] + ["M2Server.Player.PlayObject.StruckDamage@PlayObject.cs:2957"] + C[2:] * 3)
    rc, out = cap(lambda: cmd_diff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", attack_only=True, json="")))
    check("missing-consumer-red", rc == 0 and "struck_durability" in out and "FIRST_DIVERGENCE" in out, out[:400])

    # 逐段：a 段形状 [a,b,hit_gate]，b 段形状 [a,b,hit_gate,hit_struck_damage] ⇒ 第一段即分歧。
    _write_trace(ta, C * 2)
    _write_trace(tb, (R + [r"crates\world\src\combat.rs:628"]) * 2)
    rc, out = cap(lambda: cmd_segdiff(argparse.Namespace(
        a=ta, b=tb, a_side="csharp", b_side="rust", json="")))
    check("seg-shape-red", rc == 0 and "FIRST_SEG_DIVERGENCE seg=1" in out and "SEG_SHAPE_DIVERGENCE" in out, out[:500])
    check("seg-count", "SEG_COUNT a=2 b=2" in out, out[:500])

    # owner 2026-10-12 19:20 裁决②：`combat.rs:831`（`AttackSuccess` 的 `Random(5)`）必须被
    # canonical 认成 `attack_success`，且属攻击路径 —— 否则 Rust 侧新加的站点会被当噪声。
    check("attack-success-mapped",
          canonical(r"crates\world\src\combat.rs:831") == "attack_success"
          and is_attack(r"crates\world\src\combat.rs:831", "rust"))

    # ---- Claim B（owner 2026-10-13 09:35）：非攻击站点组口径（groups / groupdiff）----
    W = "M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427"
    D = "GameSrv.Word.WorldServer.MonGetRandomItems@WorldServer.MonGen.cs:451"
    MR = "GameSrv.Npc.Merchant.Run@Merchant.cs:132"
    SP = "GameSrv.Word.WorldServer.CreateMonster@WorldServer.MonGen.cs:740"
    CT = "M2Server.Actor.BaseObject..ctor@BaseObject.cs:438"
    HM = "GameSrv.Word.WorldServer.GetHomeInfo@WorldServer.cs:1392"
    # Rust 侧真实站点（`\` 在 trace 里，归一时要能命中）
    RW = "crates\\world\\src\\monai.rs:164"
    RSP = "crates\\world\\src\\mongen.rs:624"
    RDR = "crates\\formula\\src\\drop.rs:109"
    RST = "bins\\game-svr\\src\\main.rs:2856"
    RHM = "bins\\game-svr\\src\\homemap.rs:130"
    RMR = "crates\\world\\src\\merchant.rs:1"

    tbl = rcg.load()
    check("groups-table-valid", tbl.validate() == [], str(tbl.validate()[:2]))
    check("group-classify-csharp", tbl.group_of(W, "csharp") == "monster_ai",
          "W -> %s" % tbl.group_of(W, "csharp"))
    check("group-classify-rust", tbl.group_of(RW, "rust") == "monster_ai",
          "RW -> %s" % tbl.group_of(RW, "rust"))
    check("group-classify-merchant-rust", tbl.group_of(RMR, "rust") == "merchant")
    # 表改坏必红：删掉 drop 的 csharp 行 ⇒ validate 必须点名 CORE 缺员
    bad = [r for r in tbl.rows if not (r[0] == "drop" and r[1] == "csharp")]
    check("groups-table-mutation-red", bool(rcg.GroupsTable(bad, "mem").validate()),
          "drop/csharp removal must be flagged")
    # reviewer-5 O-3：AUX 组整组被删也必须被 validate 咬住（不只 CORE）
    bad_aux = [r for r in tbl.rows if not (r[0] == "revive" and r[1] == "csharp")]
    check("groups-table-aux-mutation-red", bool(rcg.GroupsTable(bad_aux, "mem").validate()),
          "revive/csharp removal must be flagged")

    # groups：合成 C# trace ⇒ 分组正确 + 全集 closed
    tg = os.path.join(tmp, "cg.tsv")
    _write_trace(tg, [CT, SP, W, W, D, MR, HM, SP, W])
    out = cap(lambda: cmd_groups(argparse.Namespace(
        trace=tg, side="csharp", groups_table="", no_attack=False, require_closed=True,
        top=5, json="")))
    check("groups-rc", out[0] == 0, out[1][:300])
    check("groups-closed", "GROUP_COVERAGE closed" in out[1], out[1][:400])
    check("groups-monster_ai-3", "3  monster_ai" in out[1], out[1][:400])
    check("groups-merchant-1", "1  merchant" in out[1], out[1][:400])
    # 改坏必红：插入未归类站点 ⇒ UNCLASSIFIED + require-closed rc=1
    _write_trace(tg, [CT, "crates/foo.rs:1", W])
    out = cap(lambda: cmd_groups(argparse.Namespace(
        trace=tg, side="csharp", groups_table="", no_attack=False, require_closed=True,
        top=5, json="")))
    check("groups-drift-red", out[0] == 1 and "UNCLASSIFIED" in out[1] and "DRIFT" in out[1],
          out[1][:400])
    # reviewer-5 O-1：行级覆盖 —— 非空但解析不了的行必须被计数，且在 --require-closed 下 fail-closed；
    # 即使可解析站点已全部归类（GROUP_COVERAGE closed），rows_dropped>0 也必须红。
    tdrop = os.path.join(tmp, "cg-drop.tsv")
    with open(tdrop, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t42\t0\n")
        fh.write("SEED_PROBE 1 2 3 4 5 6 7 8\n")
        fh.write("3\tNext(max)\t10\t0\t%s\n" % W)
        fh.write("not-an-int\tNext(max)\t10\t0\tcrates/foo.rs:1\n")  # 首列非整数 ⇒ dropped
        fh.write("9\tNext(max)\n")                                   # 列<4 ⇒ dropped
    out = cap(lambda: cmd_groups(argparse.Namespace(
        trace=tdrop, side="csharp", groups_table="", no_attack=False, require_closed=True,
        top=5, json="")))
    check("groups-rowcoverage-red",
          out[0] == 1 and "rows_dropped=2" in out[1] and "ROW_COVERAGE DRIFT" in out[1],
          out[1][:500])
    check("groups-rowcoverage-classified",
          "GROUP_COVERAGE closed" in out[1] and "1  monster_ai" in out[1], out[1][:500])
    out = cap(lambda: cmd_groups(argparse.Namespace(
        trace=tdrop, side="csharp", groups_table="", no_attack=False, require_closed=False,
        top=5, json="")))
    check("groups-rowcoverage-informational", out[0] == 0 and "rows_dropped=2" in out[1], out[1][:400])

    # groupdiff：同组序 ⇒ 绿；改坏（多抽/换序/缺组）⇒ 必红且定位
    ga = os.path.join(tmp, "ga.tsv")
    gb = os.path.join(tmp, "gb.tsv")
    _write_trace(ga, [RST, RHM, RSP, RW, RW, RDR])
    _write_trace(gb, [RST, RHM, RSP, RW, RW, RDR])
    gdiff = lambda strict=True: cap(lambda: cmd_groupdiff(argparse.Namespace(
        a=ga, b=gb, a_side="rust", b_side="rust", include_attack=False, strict=strict,
        groups_table="", json="")))
    out = gdiff()
    check("groupdiff-identical-green", out[0] == 0 and "VERDICT IDENTICAL" in out[1], out[1][:400])
    # 多抽一次（同组）⇒ 共享组序列长度不等 ⇒ 必红（多抽/少抽口径）
    _write_trace(gb, [RST, RHM, RSP, RW, RW, RW, RDR])
    out = gdiff()
    check("groupdiff-extra-draw-red", out[0] == 1 and "SHARED_REL_ORDER_DIFF" in out[1], out[1][:400])
    # 换序（homemap/spawn）⇒ 必红且定位 index=1
    _write_trace(gb, [RST, RSP, RHM, RW, RW, RDR])
    out = gdiff()
    check("groupdiff-order-red", out[0] == 1 and "FIRST_SHARED_ORDER_DIVERGENCE index=1" in out[1], out[1][:400])
    # 只在 B 出现的组（merchant）⇒ GROUP_SET_DIFF 必红
    _write_trace(gb, [RST, RHM, RSP, RW, RW, RDR, RMR])
    out = gdiff()
    check("groupdiff-set-red", out[0] == 1 and "GROUP_SET_DIFF" in out[1] and "merchant" in out[1], out[1][:400])
    # reviewer-5 O-1：组序完全相同，但一侧有解析不了的行 ⇒ --strict 必红（ROW_COVERAGE_DRIFT）
    gbad = os.path.join(tmp, "gb-drop.tsv")
    _write_trace(ga, [RST, RHM, RSP, RW, RW, RDR])
    _write_trace(gbad, [RST, RHM, RSP, RW, RW, RDR])
    with open(gbad, "a", encoding="utf-8", newline="\n") as fh:
        fh.write("bad\tNext(max)\t10\t0\n")
    out = cap(lambda: cmd_groupdiff(argparse.Namespace(
        a=ga, b=gbad, a_side="rust", b_side="rust", include_attack=False, strict=True,
        groups_table="", json="")))
    check("groupdiff-rowcoverage-red",
          out[0] == 1 and "ROW_COVERAGE_DRIFT" in out[1]
          and "ROW_COVERAGE a_dropped=0 b_dropped=1" in out[1], out[1][:500])

    # ---- R-6 folded 映射（owner 2026-10-13 裁决）----
    # 规则完整性 + `construct -> spawn` 语义。
    check("fold-rule-valid", rcg.validate_folds() == [], str(rcg.validate_folds()))
    check("fold-map-construct-spawn",
          rcg.fold_group("construct") == "spawn" and rcg.fold_group("drop") == "drop")
    # 改坏必红：折叠目标改成未知组 ⇒ `validate_folds` 必须点名（改完恢复原规则）。
    saved_folds = dict(rcg.FOLD_RULES)
    rcg.FOLD_RULES["construct"] = "no_such_group"
    try:
        bad_folds = rcg.validate_folds()
    finally:
        rcg.FOLD_RULES.clear()
        rcg.FOLD_RULES.update(saved_folds)
    check("fold-table-mutation-red", bool(bad_folds), "unknown fold target must be flagged")

    def fold_gdiff(a_sites, b_sites, strict=True):
        _write_trace(ga, a_sites)
        _write_trace(gb, b_sites)
        return cap(lambda: cmd_groupdiff(argparse.Namespace(
            a=ga, b=gb, a_side="csharp", b_side="rust", include_attack=False, strict=strict,
            groups_table="", json="", fold=True)))

    # 折叠等价：C# [construct, spawn, monster_ai] → [spawn, spawn, monster_ai]；Rust 同序 ⇒ 绿。
    out = fold_gdiff([CT, SP, W], [RSP, RSP, RW])
    check("folded-equivalent-green",
          out[0] == 0 and "VERDICT IDENTICAL" in out[1] and "FOLDED_COUNT_EQUAL" in out[1],
          out[1][:600])
    # 折叠计数不等（C# construct+spawn = 2 vs Rust spawn = 1）⇒ 必红且点名 FOLDED_COUNT_DIFF。
    out = fold_gdiff([CT, SP], [RSP])
    check("folded-count-red",
          out[0] == 1 and "FOLDED_COUNT_DIFF" in out[1] and "spawn" in out[1], out[1][:600])
    # 折叠后相对次序不等（计数相等）⇒ 必红且定位（且不是计数差）。
    out = fold_gdiff([CT, W, SP], [RSP, RSP, RW])
    check("folded-order-red",
          out[0] == 1 and "SHARED_REL_ORDER_DIFF" in out[1]
          and "FOLDED_COUNT_DIFF" not in out[1], out[1][:600])
    # 显式开关：不传 `--fold` 时 construct vs spawn 仍如实 GROUP_SET_DIFF（折叠不是全局豁免）。
    _write_trace(ga, [CT])
    _write_trace(gb, [RSP])
    out = cap(lambda: cmd_groupdiff(argparse.Namespace(
        a=ga, b=gb, a_side="csharp", b_side="rust", include_attack=False, strict=True,
        groups_table="", json="", fold=False)))
    check("fold-off-still-red", out[0] == 1 and "GROUP_SET_DIFF" in out[1], out[1][:400])

    # ---- DoD②（本片新增）：scenario 声明的 groups_expected / min_monster_ai_draws 必须**真的出现**。
    # 旧口径下「两侧都缺 monster_ai/drop」= 组集合彼此相等 ⇒ 真空 IDENTICAL 假绿；传 --scenario 后
    # 必须 fail-closed。
    def write_scenario(path, groups, min_ai=None):
        obj = {"trace": {"groups_expected": list(groups)}}
        if min_ai is not None:
            obj["trace"]["min_monster_ai_draws"] = min_ai
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(obj, fh)
        return path

    def scen_gdiff(a_sites, b_sites, scenario):
        _write_trace(ga, a_sites)
        _write_trace(gb, b_sites)
        return cap(lambda: cmd_groupdiff(argparse.Namespace(
            a=ga, b=gb, a_side="csharp", b_side="rust", include_attack=False, strict=True,
            groups_table="", json="", fold=True, scenario=scenario)))

    scen_all = write_scenario(os.path.join(tmp, "scen-all.json"),
                              ["monster_ai", "spawn", "drop"], min_ai=1)
    # 正样本：三组两侧都出现且 monster_ai=1 ⇒ 绿。
    out = scen_gdiff([CT, SP, W, D], [RSP, RSP, RW, RDR], scen_all)
    check("scenario-expected-green",
          out[0] == 0 and "VERDICT IDENTICAL" in out[1]
          and "EXPECTED_GROUPS_PRESENT" in out[1] and "MIN_GROUP_DRAWS_OK" in out[1], out[1][:700])
    # 改坏必红①：两侧都缺 drop（账本彼此全等 ⇒ 旧口径假绿）⇒ MISSING_EXPECTED_GROUPS 且点名 drop。
    out = scen_gdiff([CT, SP, W], [RSP, RSP, RW], scen_all)
    check("scenario-expected-missing-red",
          out[0] == 1 and "MISSING_EXPECTED_GROUPS" in out[1] and "drop" in out[1], out[1][:700])
    # 改坏必红②：组都在但 monster_ai 抽数 0 < min=1 ⇒ MIN_GROUP_DRAWS 且点名 monster_ai。
    scen_min = write_scenario(os.path.join(tmp, "scen-min.json"), ["spawn", "drop"], min_ai=1)
    out = scen_gdiff([CT, SP, D], [RSP, RSP, RDR], scen_min)
    check("scenario-min-monster-ai-red",
          out[0] == 1 and "MIN_GROUP_DRAWS" in out[1] and "monster_ai" in out[1], out[1][:700])
    # 缺 scenario 件 ⇒ PREREQ fail-closed（rc=2，不许静默降级成不校验）。
    out = scen_gdiff([CT, SP, W, D], [RSP, RSP, RW, RDR], os.path.join(tmp, "no-such-scenario.json"))
    check("scenario-missing-prereq", out[0] == 2 and "PREREQ_MISSING" in out[1], out[1][:400])
    # 不传 --scenario ⇒ 行为不变（向后兼容；上面的纯 groupdiff 用例即证）。
    out = scen_gdiff([CT, SP], [RSP], "")
    check("scenario-absent-backward-compatible", "MISSING_EXPECTED_GROUPS" not in out[1], out[1][:400])

    for name in os.listdir(tmp):
        try:
            os.remove(os.path.join(tmp, name))
        except OSError:
            pass
    os.rmdir(tmp)
    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: rng_ledger_diff selftest %d/%d（同序绿 + 次序漂移必红且定位 + 缺消费者必红且点名 + Claim B 站点组：同组序绿 / 多抽·换序·缺组必红且定位 / 组表改坏必红 / 未归类 fail-closed / 行级覆盖 rows_dropped fail-closed（O-1）+ AUX 组改坏必红（O-3）+ R-6 folded：等价绿 / 折叠计数不等·次序不等必红且定位 / 关时仍 GROUP_SET_DIFF + DoD② scenario 声明：三组齐备绿 / 缺组·monster_ai 抽数不足必红且点名 / 缺 scenario PREREQ / 不传向后兼容）"
          % (len(checks), len(checks)))
    return 0


def _die(msg, rc=2):
    print(msg, file=sys.stderr)
    return rc


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 two-sided RNG ledger site+order diff")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("summary")
    p.add_argument("trace")
    p.add_argument("--side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--top", type=int, default=15)
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_summary)

    p = sub.add_parser("order")
    p.add_argument("trace")
    p.add_argument("--side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--attack-only", action="store_true")
    p.add_argument("--out", default="")
    p.set_defaults(func=cmd_order)

    p = sub.add_parser("diff")
    p.add_argument("a")
    p.add_argument("b")
    p.add_argument("--a-side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--b-side", choices=["csharp", "rust"], default="rust")
    p.add_argument("--attack-only", action="store_true")
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_diff)

    p = sub.add_parser("segments")
    p.add_argument("trace")
    p.add_argument("--side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--head", type=int, default=3)
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_segments)

    p = sub.add_parser("segdiff")
    p.add_argument("a")
    p.add_argument("b")
    p.add_argument("--a-side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--b-side", choices=["csharp", "rust"], default="rust")
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_segdiff)

    p = sub.add_parser("groups")
    p.add_argument("trace")
    p.add_argument("--side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--groups-table", default="", help="消费者组表（缺省 = 同目录 consumer_groups.tsv）")
    p.add_argument("--no-attack", action="store_true", help="剔除攻击组（Claim B 只看非攻击消费者）")
    p.add_argument("--require-closed", action="store_true", help="有未归类站点 ⇒ 判红（表漂移 fail-closed）")
    p.add_argument("--top", type=int, default=10)
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_groups)

    p = sub.add_parser("groupdiff")
    p.add_argument("a")
    p.add_argument("b")
    p.add_argument("--a-side", choices=["csharp", "rust"], default="csharp")
    p.add_argument("--b-side", choices=["csharp", "rust"], default="rust")
    p.add_argument("--include-attack", action="store_true", help="把攻击组也纳入组序（缺省只看非攻击消费者）")
    p.add_argument("--groups-table", default="")
    p.add_argument("--strict", action="store_true", help="组集/相对次序不一致 ⇒ 判红")
    p.add_argument("--fold", action="store_true",
                   help="R-6 folded 映射：construct->spawn 后比组集合/计数/相对次序（缺省关，行为不变）")
    p.add_argument("--scenario", default="",
                   help="DoD②：scenario JSON 的 trace.groups_expected/min_monster_ai_draws 必须在"
                        "真实账本里成立（缺组 / 抽数不足 ⇒ fail-closed RED；缺省不校验，行为不变）")
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_groupdiff)

    p = sub.add_parser("selftest")
    p.set_defaults(func=cmd_selftest)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
