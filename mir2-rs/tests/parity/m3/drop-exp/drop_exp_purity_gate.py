#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""drop_exp_purity_gate.py —— M3④ 掉落/经验场景的「除被测量路径外 RNG 消费者 = 0」fail-closed 门。

## 为什么有这道门

卡 `openmir2-m3-drop-exp-align` 的 whitelist **T-35** 是「隔离口径降级登记」，owner 裁决
（comment `c78bd13f`，2026-10-13）为**有条件批准**，明确要求：

1. 口径必须写成「场景内除被测量路径（1 只 pin 怪 + 击杀→掉落/经验取数）外，**RNG 消费者数 = 0**」，
   并给**证据**（逐消费者清单 + 计数）——**不许**拿「离刷怪圈远」「图里 MonGen 零行」「只 spawn 一只」
   任何一条单独当可比性充分条件（②③ 的原教训：协变量不是充要条件）。
2. **必须带纯度门**（与 ②③ 的 `attack_stream_gate.py` 同族）：按被测量路径收窄 + fail-closed
   （认不出 / 缺字段 ⇒ **点名 RED**）+ **改坏必红**；不许用裸断言。
3. **必须处理墙钟驱动的消费者**：`AnimalObject.Wondering` 是墙钟节奏抽数（②③ 实测它凭一己之力
   把 C# 流水 99.98% 变噪声）。本夹具采用 **owner 给的两条之一 —— (a) pin 抑制**：把怪钉住
   （不游走 / 不搜索目标），门**必须能证明抑制生效**（墙钟站点计数 == 0）。替代方案 (b)
   「把掉落/经验取数接到专用流」属实现卡改动，本门给出 fail-closed 占位（`--wallclock-policy dedicated`
   未实现 ⇒ PREREQ 拒跑）。

本门是**下游 fail-closed 牙齿**：即便场景搭建/实现侧收窄失效，也当场判红、拒比，绝不让污染流水
进入 `drop_exp_diff.py` 得出假结论。

## 判据（任一不满足 ⇒ rc=1）

把该侧**全场景抽数流水**（`MIR2_RNG_LOG` / Rust `rng-stream.tsv`，5 列带 site）逐行按
`drop_exp_consumers.tsv`（**唯一真源**）分类：

| class | 语义 | 要求 |
| --- | --- | --- |
| `measured` | 被测量路径（击杀 → 掉落取数；经验为确定式不抽数） | **> 0**（证明取数真的发生过） |
| `setup` | 被测量路径的**场景构建**（owner：「1 只 pin 怪」；spawn/构造抽数） | **≤ max**（`max` 列；`*` = 暂不设上限，报告里标 `setup_unbounded`） |
| `wallclock` | 墙钟驱动的 AI 消费者（`AnimalObject.Wondering` 等） | **== 0**（pin 抑制生效证据） |
| `other` | 其余任何 RNG 消费者 | **== 0** |
| （未命中任何行） | 认不出 | **点名 RED**（fail-closed，不许静默放行） |

另：空流水 ⇒ RED（**空集不是纯度证据**）；同一 site 命中**互斥的多个 class** ⇒ RED（表有歧义）；
该 side 在表里**没有 `measured` 行** ⇒ PREREQ（认不出被测量路径，拒跑而不是假绿）；
给了 `--raw` 时逐 k 校验击杀记录（缺字段 / k 不连续 / 条数 ≠ `--expected-kills` ⇒ 判红）。

## 数据源

两侧同形 TSV（与 `attack_rng_ledger.py` 同口径）：

    <seq> \t <kind> \t <max> \t <result> \t <site>

首两行 `INJECT_OK <seed>` / `SEED_PROBE …`、`#` 注释行、空行跳过。**缺 site 列 ⇒ 判红**
（无法证明纯度）。站点分隔符 `\` / `/` 归一后比（与 ②③ 同口径）。

## 用法

    python -I drop_exp_purity_gate.py gate --side csharp --stream <rng-stream.tsv> \
        --expected-kills 100 --raw <drop_exp_raw.jsonl> [--table <drop_exp_consumers.tsv>] [--json out.json]
    python -I drop_exp_purity_gate.py selftest

退出码：0 = GREEN；1 = RED（污染 / 计数不符 / 认不出）；2 = PREREQ（缺件 / 表未含被测量行 / 坏记录）。
"""

import argparse
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_TABLE = os.path.join(HERE, "drop_exp_consumers.tsv")

CLASSES = ("measured", "setup", "wallclock", "other")


class Prereq(Exception):
    """缺件 / 表不可用 / 坏记录（fail-closed，rc=2）。"""


def norm_site(site):
    """`crates\\world\\src\\x.rs:1` / `M2Server\\...` 统一成 `/` 再比（与 ②③ 同口径）。"""
    return (site or "").replace("\\", "/").strip()


# ---- 消费者表（唯一真源） ---------------------------------------------------


def load_table(path):
    """读 `drop_exp_consumers.tsv` ⇒ {"rows": [...], "by_side": {side: [row…]}}。

    列：`side \t class \t pattern \t max \t oracle_ref \t note`（首行可 `#` 注释；缺列 ⇒ Prereq）。
    pattern 是**正则**（在归一后的 site 上 `search`）；非法正则 ⇒ Prereq。
    """
    if not os.path.isfile(path):
        raise Prereq("缺消费者表: %s" % path)
    rows = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for lineno, raw in enumerate(fh, start=1):
            line = raw.rstrip("\r\n")
            if line.strip() == "" or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 6:
                raise Prereq("表第 %d 行不足 6 列（side/class/pattern/max/oracle_ref/note）: %s" % (lineno, line))
            side, cls, pattern, maxv, oracle_ref, note = parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
            if side not in ("csharp", "rust"):
                raise Prereq("表第 %d 行 side 非法: %r" % (lineno, side))
            if cls not in CLASSES:
                raise Prereq("表第 %d 行 class 非法: %r（可 %s）" % (lineno, cls, "/".join(CLASSES)))
            try:
                rx = re.compile(pattern)
            except re.error as exc:
                raise Prereq("表第 %d 行 pattern 非法正则: %s" % (lineno, exc))
            rows.append(
                {
                    "lineno": lineno,
                    "side": side,
                    "class": cls,
                    "pattern": pattern,
                    "rx": rx,
                    "max": maxv,
                    "oracle_ref": oracle_ref,
                    "note": note,
                }
            )
    if not rows:
        raise Prereq("消费者表为空: %s" % path)
    by_side = collections.defaultdict(list)
    for r in rows:
        by_side[r["side"]].append(r)
    return {"path": path, "rows": rows, "by_side": by_side}


def classify(site, table_rows):
    """返回命中该 site 的 class 集（可能空 / 多）。

    - 空 ⇒ 认不出（调用方判红）；
    - 多个**互斥** class ⇒ 表有歧义（调用方判红）。
    """
    hits = set()
    for r in table_rows:
        if r["rx"].search(site):
            hits.add(r["class"])
    return hits


# ---- 流水解析 ---------------------------------------------------------------


def parse_stream(path):
    """读全场景抽数流水 ⇒ [(lineno, site, has_site)]（1-based 行号便于点名）。

    跳过 `#` 注释 / `INJECT_OK` / `SEED_PROBE` / 空行。**列数 < 5 ⇒ has_site=False**
    （调用方按「无法证明纯度」判红，而不是静默丢弃）。
    """
    rows = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for lineno, raw in enumerate(fh, start=1):
            line = raw.rstrip("\r\n")
            if line == "" or line.lstrip().startswith("#"):
                continue
            if "INJECT_OK" in line or line.startswith("SEED_PROBE"):
                continue
            parts = line.split("\t")
            if len(parts) >= 5:
                rows.append((lineno, norm_site(parts[4]), True))
            else:
                # 只有 4 列（专用流口径）或缺列 ⇒ 无 site，判红。
                rows.append((lineno, norm_site(parts[3]) if len(parts) >= 4 else "", False))
    return rows


def load_raw_kills(path):
    """读 `drop_exp_raw.jsonl` ⇒ 击杀条数（逐 k 校验：必须 0 起连续、记录为对象）。"""
    if not os.path.isfile(path):
        raise Prereq("缺击杀原始件: %s" % path)
    ks = []
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.strip()
            if line == "":
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError as exc:
                raise Prereq("非法 JSON %s:%d: %s" % (path, lineno, exc))
            if not isinstance(rec, dict) or "k" not in rec:
                raise Prereq("击杀记录非法（应为含 k 的对象）%s:%d" % (path, lineno))
            if not isinstance(rec["k"], int) or isinstance(rec["k"], bool):
                raise Prereq("击杀记录 k 非整数 %s:%d" % (path, lineno))
            ks.append(rec["k"])
    if not ks:
        raise Prereq("击杀原始件无记录: %s" % path)
    for i, k in enumerate(ks):
        if k != i:
            raise Prereq("k 不连续/非 0 起（第 %d 条 k=%r）(%s)" % (i, k, path))
    return len(ks)


# ---- 判据 -------------------------------------------------------------------


def judge(side, rows, table_rows, expected_kills=None, raw_kills=None):
    """返回 (verdict, problems, counts, histogram)。verdict ∈ GREEN/RED。"""
    problems = []
    counts = collections.Counter()
    per_site = collections.Counter()
    row_hits = collections.Counter()   # table row lineno -> 命中次数（用于 max 校验）
    unknown_detail = []
    wallclock_detail = []
    other_detail = []
    ambiguous_detail = []

    for lineno, site, has_site in rows:
        if not has_site:
            problems.append(
                "stream_missing_site: line=%d 该行没有 site 列 ⇒ 无法证明纯度（fail-closed）" % lineno)
            counts["missing_site"] += 1
            continue
        if site == "":
            problems.append("stream_empty_site: line=%d site 为空 ⇒ 无法证明纯度（fail-closed）" % lineno)
            counts["missing_site"] += 1
            continue
        per_site[site] += 1
        hits = classify(site, table_rows)
        if not hits:
            counts["unknown"] += 1
            unknown_detail.append((lineno, site))
            continue
        if len(hits) > 1:
            counts["ambiguous"] += 1
            ambiguous_detail.append((lineno, site, sorted(hits)))
            continue
        cls = next(iter(hits))
        counts[cls] += 1
        for r in table_rows:
            if r["class"] == cls and r["rx"].search(site):
                row_hits[r["lineno"]] += 1
        if cls == "wallclock":
            wallclock_detail.append((lineno, site))
        elif cls == "other":
            other_detail.append((lineno, site))

    if not rows:
        problems.append("empty_stream: 流水为空 ⇒ 空集不是纯度证据（fail-closed 拒比）")
    if unknown_detail:
        problems.append(
            "unknown_consumer: %d 处 site 不在 `drop_exp_consumers.tsv` 里（首个 line=%d site=%s）；"
            "认不出 ⇒ 拒比，必须在表里显式分类（measured/setup/wallclock/other）"
            % (len(unknown_detail), unknown_detail[0][0], unknown_detail[0][1]))
    if ambiguous_detail:
        lineno, site, cls = ambiguous_detail[0]
        problems.append(
            "ambiguous_consumer_class: line=%d site=%s 同时命中多个互斥 class %s ⇒ 表有歧义"
            % (lineno, site, ",".join(cls)))
    if wallclock_detail:
        problems.append(
            "wallclock_consumer: %d 处墙钟驱动抽数（首个 line=%d site=%s）⇒ pin 抑制未生效；"
            "需 (a) 把怪钉住（不游走/不搜索目标）或 (b) 接专用流（实现卡）"
            % (len(wallclock_detail), wallclock_detail[0][0], wallclock_detail[0][1]))
    if other_detail:
        problems.append(
            "other_consumer: %d 处非被测量 RNG 消费者（首个 line=%d site=%s）⇒ 场景不纯"
            % (len(other_detail), other_detail[0][0], other_detail[0][1]))
    if counts["measured"] == 0 and rows:
        problems.append("no_measured_draws: 流水里没有被测量路径（掉落取数）的抽数 ⇒ 取数未发生")

    if raw_kills is not None and expected_kills is not None and raw_kills != expected_kills:
        problems.append(
            "kill_count_mismatch: raw 记录数=%d 期望=%d（--expected-kills）" % (raw_kills, expected_kills))

    # setup 类按表里的 `max` 设上限（`*` = 暂不设上限，报告里单独标 unbounded 供复核）。
    for r in table_rows:
        if r["class"] != "setup":
            continue
        if r["max"] == "*":
            if row_hits.get(r["lineno"], 0) > 0:
                counts["setup_unbounded"] += 1
            continue
        try:
            cap = int(r["max"])
        except ValueError:
            raise Prereq("表第 %d 行 max 非法（整数或 *）: %r" % (r["lineno"], r["max"]))
        if row_hits.get(r["lineno"], 0) > cap:
            problems.append(
                "setup_over_max: line=%d pattern=%s 命中=%d 上限=%d"
                % (r["lineno"], r["pattern"], row_hits[r["lineno"]], cap))

    return ("GREEN" if not problems else "RED"), problems, counts, per_site


def render(payload, counts, per_site, out=None):
    lines = []
    lines.append("=== 场景纯度（drop_exp_purity_gate）===")
    lines.append("  side=%s stream=%s" % (payload["side"], payload["stream"]))
    lines.append(
        "  draws=%d measured=%d setup=%d wallclock=%d other=%d unknown=%d missing_site=%d ambiguous=%d"
        % (payload["counts"]["draws"], counts["measured"], counts["setup"], counts["wallclock"],
           counts["other"], counts["unknown"], counts["missing_site"], counts["ambiguous"]))
    if counts["setup_unbounded"]:
        lines.append(
            "  NOTE: %d 条 setup 行 max=* ⇒ 未设上限（窗口期按实测收紧；不影响 measured/wallclock/other 判据）"
            % counts["setup_unbounded"])
    if payload.get("expected_kills") is not None:
        lines.append("  expected_kills=%s raw_kills=%s" % (payload["expected_kills"], payload.get("raw_kills")))
    lines.append("  --- 逐消费者清单（site 计数，降序）---")
    for site, n in per_site.most_common():
        lines.append("  %8d  %s" % (n, site))
    if payload["verdict"] == "GREEN":
        lines.append("  GREEN: 除被测量路径外 RNG 消费者 = 0（wallclock=0 / other=0 / unknown=0）")
    else:
        for p in payload["problems"]:
            lines.append("  RED: %s" % p)
        lines.append("  结论：场景不纯 / 认不出 ⇒ **拒比**（fail-closed，不放宽判据）")
    text = "\n".join(lines)
    sys.stdout.write(text + "\n")
    if out:
        with open(out, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(payload, fh, ensure_ascii=False, indent=1, sort_keys=True)
            fh.write("\n")
    return text


def cmd_gate(args):
    if args.wallclock_policy != "pinned":
        sys.stdout.write(
            "PREREQ_MISSING: --wallclock-policy %s 未实现（(b) 专用流要落实现卡）；本夹具采用 "
            "(a) pin 抑制，请用 `--wallclock-policy pinned`\n" % args.wallclock_policy)
        return 2
    try:
        table = load_table(args.table)
    except Prereq as exc:
        sys.stdout.write("PREREQ_MISSING: %s\n" % exc)
        return 2
    table_rows = table["by_side"].get(args.side, [])
    if not any(r["class"] == "measured" for r in table_rows):
        sys.stdout.write(
            "PREREQ_MISSING: consumers 表里 side=%s 没有 `measured` 行 ⇒ 认不出被测量路径，拒跑"
            "（不许假绿；实现卡落地后按实测站点补表）\n" % args.side)
        return 2
    if not os.path.isfile(args.stream):
        sys.stdout.write("PREREQ_MISSING: 找不到抽数流水 %s\n" % args.stream)
        return 2
    rows = parse_stream(args.stream)
    raw_kills = None
    if args.raw:
        try:
            raw_kills = load_raw_kills(args.raw)
        except Prereq as exc:
            sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
            return 2
    verdict, problems, counts, per_site = judge(
        args.side, rows, table_rows, expected_kills=args.expected_kills, raw_kills=raw_kills)
    payload = {
        "schema": "m3-drop-exp-purity-v1",
        "side": args.side,
        "stream": args.stream,
        "table": table["path"],
        "wallclock_policy": args.wallclock_policy,
        "expected_kills": args.expected_kills,
        "raw_kills": raw_kills,
        "verdict": verdict,
        "counts": {
            "draws": len(rows),
            "measured": counts["measured"],
            "setup": counts["setup"],
            "setup_unbounded_rows": counts["setup_unbounded"],
            "wallclock": counts["wallclock"],
            "other": counts["other"],
            "unknown": counts["unknown"],
            "missing_site": counts["missing_site"],
            "ambiguous": counts["ambiguous"],
        },
        "per_site": dict(per_site),
        "problems": problems,
    }
    render(payload, counts, per_site, out=args.json)
    return 0 if verdict == "GREEN" else 1


# ---- selftest（fail-closed + 改坏必红） -------------------------------------

_SELFTEST_TABLE = """# side\tclass\tpattern\tmax\toracle_ref\tnote
csharp\tmeasured\t^Measured\\.Drop@Measured\\.cs:10$	*\tsynthetic\t合成：被测量路径
csharp\twallclock\t^Noise\\.Wondering@Noise\\.cs:20$	0\tsynthetic\t合成：墙钟消费者（必须 0）
csharp\tother\t^Other\\.Merchant@Other\\.cs:30$	0\tsynthetic\t合成：其它消费者（必须 0）
csharp\tother\t^Measured\\.Drop@Measured\\.cs:99$	0\tsynthetic\t合成：与 measured 冲突（歧义）
"""


def _write_text(path, text):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)


def _stream_text(sites, with_site=True):
    out = []
    out.append("1\tINJECT_OK\t42\t0")
    out.append("SEED_PROBE 42")
    for i, s in enumerate(sites, start=1):
        if with_site:
            out.append("%d\tNext(max)\t100\t7\t%s" % (i, s))
        else:
            out.append("%d\tNext(max)\t100\t7" % i)
    return "".join(x + "\n" for x in out)


def _raw_text(n):
    return "".join('{"k":%d,"gold":0,"drops":[],"exp_delta":1,"exp_total":1,"level":1}\n' % i
                   for i in range(n))


def _judge_file(table, stream, side="csharp", expected_kills=None, raw=None):
    t = load_table(table)
    rows = parse_stream(stream)
    raw_kills = load_raw_kills(raw) if raw else None
    verdict, problems, counts, per_site = judge(
        side, rows, t["by_side"].get(side, []), expected_kills=expected_kills, raw_kills=raw_kills)
    return verdict, problems, counts


def cmd_selftest(_args):
    import shutil
    import tempfile

    tmp = tempfile.mkdtemp(prefix="drop-exp-purity-selftest-")
    results = []

    def case(name, expected, fn):
        try:
            got = fn()
        except (Prereq, OSError) as exc:
            got = "exc:%s" % exc
        results.append((name, expected, got, got == expected))

    try:
        table = os.path.join(tmp, "consumers.tsv")
        _write_text(table, _SELFTEST_TABLE)

        measured = ["Measured.Drop@Measured.cs:10"]
        clean = os.path.join(tmp, "clean.tsv")
        _write_text(clean, _stream_text(measured * 3))
        raw = os.path.join(tmp, "raw.jsonl")
        _write_text(raw, _raw_text(2))

        # ① 干净（只有被测量路径）⇒ GREEN。
        case("clean-green", "GREEN",
             lambda: _judge_file(table, clean, expected_kills=2, raw=raw)[0])

        # ② 注入一次墙钟抽数 ⇒ RED 且点名 wallclock。
        wall = os.path.join(tmp, "wall.tsv")
        _write_text(wall, _stream_text(measured + ["Noise.Wondering@Noise.cs:20"]))
        case("wallclock-red", "RED",
             lambda: _judge_file(table, wall)[0])
        case("wallclock-names-problem", True,
             lambda: any("wallclock_consumer" in p for p in _judge_file(table, wall)[1]))

        # ③ 注入一次其它消费者 ⇒ RED 且点名 other。
        other = os.path.join(tmp, "other.tsv")
        _write_text(other, _stream_text(measured + ["Other.Merchant@Other.cs:30"]))
        case("other-red", "RED", lambda: _judge_file(table, other)[0])
        case("other-names-problem", True,
             lambda: any("other_consumer" in p for p in _judge_file(table, other)[1]))

        # ④ 认不出（表里没有）⇒ RED（fail-closed，不许静默放行）。
        unknown = os.path.join(tmp, "unknown.tsv")
        _write_text(unknown, _stream_text(measured + ["Mystery.Thing@Mystery.cs:1"]))
        case("unknown-red", "RED", lambda: _judge_file(table, unknown)[0])
        case("unknown-names-problem", True,
             lambda: any("unknown_consumer" in p for p in _judge_file(table, unknown)[1]))

        # ⑤ 表有歧义（同一 site 命中 measured 与 other）⇒ RED。
        ambiguous = os.path.join(tmp, "ambiguous.tsv")
        _write_text(ambiguous, _stream_text(["Measured.Drop@Measured.cs:99"]))
        case("ambiguous-class-red", "RED", lambda: _judge_file(table, ambiguous)[0])

        # ⑥ 空流水 ⇒ RED（空集不是纯度证据）。
        empty = os.path.join(tmp, "empty.tsv")
        _write_text(empty, "# only comments\n")
        case("empty-red", "RED", lambda: _judge_file(table, empty)[0])

        # ⑦ 缺 site 列 ⇒ RED（无法证明纯度）。
        nos = os.path.join(tmp, "nos.tsv")
        _write_text(nos, _stream_text(measured, with_site=False))
        case("missing-site-red", "RED", lambda: _judge_file(table, nos)[0])

        # ⑧ 该 side 表里没有 measured 行 ⇒ PREREQ（认不出被测量路径，拒跑）。
        case("no-measured-row-prereq", 2,
             lambda: _gate_rc(table, clean, side="rust"))

        # ⑨ 缺流水 ⇒ PREREQ。
        case("missing-stream-prereq", 2,
             lambda: _gate_rc(table, os.path.join(tmp, "nope.tsv")))

        # ⑩ raw 记录数 ≠ expected-kills ⇒ RED。
        case("kill-count-mismatch-red", "RED",
             lambda: _judge_file(table, clean, expected_kills=3, raw=raw)[0])

        # ⑪ raw k 不连续 ⇒ PREREQ（坏记录 fail-closed）。
        badraw = os.path.join(tmp, "badraw.jsonl")
        _write_text(badraw, '{"k":0}\n{"k":2}\n')
        case("raw-k-gap-prereq", 2,
             lambda: _gate_rc(table, clean, raw=badraw, expected_kills=2))

        # ⑫ 改坏必红（表是承重的）：把 measured 行删掉 ⇒ 原本绿的流水变认不出 ⇒ RED。
        broken = os.path.join(tmp, "broken.tsv")
        _write_text(broken,
                    "csharp\twallclock\t^Noise\\\\.Wondering@Noise\\\\.cs:20$\t0\tsynthetic\tx\n")
        case("table-measured-removed-red", 2,
             lambda: _gate_rc(broken, clean))

        # ⑬ 改坏必红（门是承重的）：把 clean 流水换成注入墙钟的 ⇒ rc=1。
        case("cli-wallclock-rc1", 1, lambda: _gate_rc(table, wall))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    failed = [(n, e, g) for (n, e, g, ok) in results if not ok]
    for name, expected, got, ok in results:
        sys.stdout.write("SELFTEST %s %s expect=%r got=%r\n"
                         % ("ok" if ok else "FAIL", name, expected, got))
    sys.stdout.write("SELFTEST ok=%d/%d\n" % (len(results) - len(failed), len(results)))
    return 1 if failed else 0


def _gate_rc(table, stream, side="csharp", expected_kills=None, raw=None):
    args = argparse.Namespace(
        side=side, stream=stream, table=table, expected_kills=expected_kills,
        raw=raw, wallclock_policy="pinned", json="")
    return cmd_gate(args)


# ---- CLI -------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3④ 掉落/经验场景纯度 fail-closed 门（T-35）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    g = sub.add_parser("gate", help="判一条全场景抽数流水是否只含被测量路径")
    g.add_argument("--side", required=True, choices=("csharp", "rust"))
    g.add_argument("--stream", required=True)
    g.add_argument("--table", default=DEFAULT_TABLE)
    g.add_argument("--expected-kills", type=int, default=None)
    g.add_argument("--raw", default="")
    g.add_argument("--wallclock-policy", default="pinned", choices=("pinned", "dedicated"))
    g.add_argument("--json", default="")
    g.set_defaults(func=cmd_gate)
    s = sub.add_parser("selftest", help="fail-closed + 改坏必红自检")
    s.set_defaults(func=cmd_selftest)
    args = ap.parse_args(argv)
    res = args.func(args)
    return res


if __name__ == "__main__":
    sys.exit(main())
