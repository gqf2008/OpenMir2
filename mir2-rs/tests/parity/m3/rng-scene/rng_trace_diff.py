#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""rng_trace_diff.py — M3 RNG 场景驱动器（卡 `openmir2-m3-rng-scene-driver`）的
**站点/次序账本身份证 + 严格校验器 + 单一比较入口**。

背景（`docs/移植计划重审-20261015.md` §1-R3 / §2-4）：RNG 双侧 trace 的既有消费者
（`rng-trace-scene/rng_scene_gate.py` 场景门 + `rng-trace-probe/rng_ledger_diff.py groupdiff`）
已就绪，但**生产者**（固定种子单场景两侧各跑一次、落 `<side>-rng-stream.tsv`）此前不存在。
本工具是驱动器的 **账本侧**：在把两侧流水交给 `groupdiff --fold --strict` **之前**，
先按「同序列对齐键 + 断号/缺件即红」的通用规约（重审 §2-4）把每一份流水**严格**过一遍 ——
既不新增判据阈值、也不改既有门的口径，只把「流水形状本身是否可信」钉成 fail-closed。

严格口径（都是既有消费者**静默放过**、但会让「组集合相等」变成假绿的通道）：

| 通道 | 既有行为 | 本工具 |
| --- | --- | --- |
| 首列非整数 / 列数 < 4 | `iter_sites` 计入 `rows_dropped`（只在 `--strict` 的被调用方里落红） | `row-shape` RED（逐行点名行号） |
| **seq 断号 / 重复** | **完全不看 seq**（`parse_trace` 只用来做 key，判据不看） | `seq-gap` / `seq-dupe` RED（点名缺失值/首个重复位） |
| 流水头 `INJECT_OK <seed>` 缺失/异值 | 只有场景门（给了两份流水时）核 | `header-inject-missing` / `header-seed-mismatch` RED |
| 第 5 列（site）缺失 | `iter_sites` 返回空串 ⇒ 归到 `other`/空站 | `site-missing` RED（场景驱动器必须 `MIR2_RNG_SITE=1`） |
| 数据行数为 0 | `parse_trace`/`iter_sites` 退化为空 ⇒ 上层 `PREREQ` | `no-data-rows` PREREQ（rc=2） |

**seq 口径**（为何从 1 连续）：两侧生产者同形 —— 头行写死 `1\tINJECT_OK\t<seed>\t0`，
其后每条取数 `seq++`。C# `StartupHook.Record` 的 `_seq++` 与写盘在**同一把 `lock (Sync)`**
里、Rust `rng_trace::record` 的 `SEQ.fetch_add` 是原子 ⇒ 主流水数据行的 seq 必然是 **1..N 连续**。
断号只可能来自「写盘被打断 / 行被截断 / 两个生产者拼错」⇒ 必须红，不许被「组集合恰好相等」掩盖。

子命令：

    check <stream.tsv> --side csharp|rust --seed N [--ledger-out PATH] [--json PATH]
        严格校验单侧流水；GREEN 时可选把**规范化账本**（seq 重编号 1..N）落盘作证据件。
    diff <a.tsv> <b.tsv> --a-side S --b-side S --seed N [--scenario PATH]
        单一入口：先对两侧各跑 `check`（任一不 GREEN ⇒ **拒比**），两者都 GREEN 才把**原始流水**
        交给 `rng_ledger_diff.py groupdiff --fold --strict`，并原样传播其 rc/文本（结论作者 = 既有比较器）。
    selftest
        合成件：同序绿 / 断号必红 / 重复 seq 必红 / 头缺失必红 / 头异种子必红 / 行形状必红 /
        缺 site 列必红 / 缺件 PREREQ / 折叠次序不等必红。每条牙自带「改坏必红」对照。

退出码：0 = GREEN；1 = RED（点名哪一条）；2 = PREREQ（缺件 / 空流水 / 坏输入，fail-closed）。

口径真源：组 vocabulary / 折叠规则取自 `rng-trace-probe/rng_consumer_groups.py`（不手抄）。
本工具只读流水，不写共享检出、不起栈、不占端口。
"""
import argparse
import collections
import json
import os
import subprocess
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
# <repo>/mir2-rs/tests/parity/m3/rng-scene/rng_trace_diff.py → 上溯 3 级 = <repo>/mir2-rs/tests/parity/m3
_M3 = os.path.abspath(os.path.join(_HERE, ".."))
_PROBE = os.path.join(_M3, "rng-trace-probe")
sys.path.insert(0, _PROBE)
import rng_consumer_groups as rcg  # noqa: E402

# 站点常量（与 `rng-trace-scene/run_rng_trace_pair.ps1` / consumer_groups.tsv 同源）。
SITE_CTOR_C = "M2Server.Actor.BaseObject..ctor@BaseObject.cs:438"      # csharp construct
SITE_AI_C = "M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427"  # csharp monster_ai
SITE_SPAWN_R = "crates\\world\\src\\mongen.rs:624"                      # rust spawn
SITE_AI_R = "crates\\world\\src\\monai.rs:164"                          # rust monster_ai

EXIT_GREEN, EXIT_RED, EXIT_PREREQ = 0, 1, 2


def _norm(site):
    return (site or "").replace("\\", "/")


def read_stream(path):
    """返回 `(header_seed, rows, problems)`。

    `header_seed` = 流水头 `INJECT_OK <seed>` 的 seed（无该行 ⇒ None，记进 problems）。
    `rows` = 数据行 `[(seq, kind, mx, result, site, lineno), ...]`（**不跳过**任何行，
             形状不符进 problems）。
    problems 是字符串去重前的有序列表（逐条带行号）。
    """
    header_seed = None
    rows = []
    problems = []
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\r\n")
            if not line:
                continue
            if line.startswith("SEED_PROBE") or line.startswith("ISOLATE_ATTACK") \
                    or line.startswith("SERVER_STATE"):
                continue
            parts = line.split("\t")
            # 头行：`1\tINJECT_OK\t<seed>\t0`（两侧生产者同形）。
            if len(parts) >= 2 and parts[1] == "INJECT_OK":
                try:
                    header_seed = int(parts[2])
                except (IndexError, ValueError):
                    problems.append("header-seed-unparsable:%d" % lineno)
                continue
            if len(parts) not in (4, 5):
                problems.append("row-shape:%d(cols=%d)" % (lineno, len(parts)))
                continue
            try:
                seq = int(parts[0])
            except ValueError:
                problems.append("row-shape:%d(seq=%r)" % (lineno, parts[0]))
                continue
            site = parts[4] if len(parts) == 5 else ""
            if not site:
                # 4 列 / 空 site：C# 钩子在 `MIR2_RNG_SITE` 未开时如此；场景驱动器**必须**有 site ⇒ 红。
                # 仍计入 rows（保持 seq 连续性判定不被这条噪声带偏），但不写进账本（账本只在 GREEN 落盘）。
                problems.append("site-missing:%d" % lineno)
            rows.append((seq, parts[1], parts[2], parts[3], site, lineno))
    return header_seed, rows, problems


def check_stream(path, side, seed):
    """严格校验单侧流水。返回 `(rc, report_dict)`；report 里 problems 逐条点名。"""
    rep = {
        "path": path, "side": side, "seed": seed,
        "header_seed": None, "rows": 0, "problems": [],
        "groups": {}, "first_site": "", "last_site": "", "verdict": "PREREQ",
    }
    if not path or not os.path.exists(path):
        rep["problems"].append("stream-missing")
        return EXIT_PREREQ, rep
    header_seed, rows, shape_problems = read_stream(path)
    rep["header_seed"] = header_seed
    rep["rows"] = len(rows)
    problems = list(shape_problems)

    # 头行自证种子（缺 ⇒ 无法证明种子；异 ⇒ 描述件自报价不算证据）。
    if header_seed is None:
        problems.append("header-inject-missing")
    elif seed is not None and header_seed != seed:
        problems.append("header-seed-mismatch(want=%d got=%d)" % (seed, header_seed))

    # 断号 / 重复：数据行 seq 必须是 1..N 连续（两侧生产者同形，见模块 docstring）。
    seqs = [r[0] for r in rows]
    if seqs:
        if len(set(seqs)) != len(seqs):
            seen = set()
            first_dup = None
            for s in seqs:
                if s in seen:
                    first_dup = s
                    break
                seen.add(s)
            problems.append("seq-dupe(first=%s)" % first_dup)
        # 期望 1..N：以实际行数 N 判定（行已被 seq 断号截断时，最小/最大仍暴露缺口）。
        want = set(range(1, len(seqs) + 1))
        got = set(seqs)
        missing = sorted(want - got)
        extra = sorted(got - want)
        if missing:
            problems.append("seq-gap(missing=%s)" % ",".join(str(m) for m in missing[:8]))
        if extra:
            problems.append("seq-out-of-range(extra=%s)" % ",".join(str(e) for e in extra[:8]))

    if rows:
        rep["first_site"] = _norm(rows[0][4])
        rep["last_site"] = _norm(rows[-1][4])
        tbl = rcg.load()
        grp = collections.Counter()
        for _seq, _k, _m, _res, site, _ln in rows:
            g = rcg.classify(site, side, tbl, lambda s, sd: False)
            grp[g] += 1
        rep["groups"] = dict(sorted(grp.items()))

    if not rows:
        problems.append("no-data-rows")

    rep["problems"] = problems
    # 分类：`stream-missing` ⇒ PREREQ；有**结构性问题**（断号/重复/头/形状/site）⇒ RED；
    # 仅在「无任何结构性问题、但确实没有数据行」时才 PREREQ（真空流水，fail-closed 不判绿）。
    if "stream-missing" in problems:
        rep["verdict"] = "PREREQ"
        return EXIT_PREREQ, rep
    red = [p for p in problems if not p.startswith("no-data-rows")]
    if red:
        rep["verdict"] = "RED"
        return EXIT_RED, rep
    if not rows:
        rep["verdict"] = "PREREQ"
        return EXIT_PREREQ, rep
    rep["verdict"] = "GREEN"
    return EXIT_GREEN, rep


def write_ledger(path, rows, seed):
    """规范化账本：seq 重编号 1..N，保留 5 列（`seq kind max result site`）+ 头行。"""
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("1\tINJECT_OK\t%d\t0\n" % seed)
        for i, (_seq, kind, mx, res, site, _ln) in enumerate(rows, 1):
            fh.write("%d\t%s\t%s\t%s\t%s\n" % (i, kind, mx, res, site))


def cmd_check(args):
    rc, rep = check_stream(args.stream, args.side, args.seed)
    print("RNG_TRACE_CHECK %s side=%s rows=%d header_seed=%s verdict=%s"
          % (args.stream, args.side, rep["rows"], rep["header_seed"], rep["verdict"]))
    if rep["groups"]:
        print("  GROUPS %s" % json.dumps(rep["groups"], ensure_ascii=False, sort_keys=True))
    for p in rep["problems"]:
        print("  PROBLEM %s" % p)
    if rc == EXIT_GREEN and args.ledger_out:
        _hs, rows, _pr = read_stream(args.stream)
        write_ledger(args.ledger_out, rows, args.seed)
        print("  LEDGER %s rows=%d" % (args.ledger_out, len(rows)))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(rep, fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    if rc == EXIT_GREEN:
        print("GREEN: 单侧流水形状自洽（头种子 / seq 1..N 连续 / 5 列 / site 非空）")
    elif rc == EXIT_RED:
        print("RED: 单侧流水**形状不可信**（断号/重复/头缺失或异种子/缺 site/坏行）")
    else:
        print("PREREQ_MISSING: 流水缺失或空（fail-closed）")
    return rc


def cmd_diff(args):
    # ① 两侧各跑严格校验：任一不 GREEN ⇒ **拒比**（不调用比较器、不输出任何组结论）。
    rc_a, rep_a = check_stream(args.a, args.a_side, args.seed)
    rc_b, rep_b = check_stream(args.b, args.b_side, args.seed)
    print("RNG_TRACE_DIFF a=%s(%s) rc=%d rows=%d ; b=%s(%s) rc=%d rows=%d"
          % (args.a, args.a_side, rc_a, rep_a["rows"], args.b, args.b_side, rc_b, rep_b["rows"]))
    for tag, rep in (("a", rep_a), ("b", rep_b)):
        for p in rep["problems"]:
            print("  PROBLEM[%s] %s" % (tag, p))
    if rc_a != EXIT_GREEN or rc_b != EXIT_GREEN:
        print("REFUSED: 流水形状不可信（rc_a=%d rc_b=%d）⇒ 不调用 groupdiff（拒绝比；不许把形状差异当实现差异）"
              % (rc_a, rc_b))
        return EXIT_RED if EXIT_RED in (rc_a, rc_b) else EXIT_PREREQ

    # ② 两者都 GREEN 才把**原始流水**交给既有比较器（结论作者 = rng_ledger_diff）。
    argv = [sys.executable, os.path.join(_PROBE, "rng_ledger_diff.py"), "groupdiff",
            args.a, args.b, "--a-side", args.a_side, "--b-side", args.b_side, "--fold", "--strict"]
    if args.scenario:
        argv += ["--scenario", args.scenario]
    proc = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8", errors="replace")
    sys.stdout.write(proc.stdout)
    if proc.stderr:
        sys.stderr.write(proc.stderr)
    return proc.returncode


# --------------------------------------------------------------------------- #
# selftest
# --------------------------------------------------------------------------- #
def _write(path, lines):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")


def _stream_lines(seed, sites, seq_start=1, gap_after=None, dupe_at=None, header=True,
                  drop_site_at=None):
    """造一份合成流水。`sites` = site 列表（各一次 `Next(max)` 取数）。"""
    lines = []
    if header:
        lines.append("1\tINJECT_OK\t%d\t0" % seed)
        lines.append("SEED_PROBE 1 2 3 4 5 6 7 8")
    seq = seq_start
    for i, s in enumerate(sites):
        row_seq = seq
        if dupe_at is not None and i == dupe_at:
            row_seq = seq - 1  # 与前一行同 seq ⇒ dup
        cols = ["%d" % row_seq, "Next(max)", "100", "7"]
        if drop_site_at is not None and i == drop_site_at:
            pass  # 4 列（缺 site）
        else:
            cols.append(s)
        lines.append("\t".join(cols))
        seq += 1
        if gap_after is not None and i == gap_after:
            seq += 1  # 跳一个号 ⇒ gap
    return lines


def cmd_selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="rngtrace-diff-selftest-")
    checks = []

    def add(name, ok, detail=""):
        checks.append((name, ok, detail))

    # ① 同序绿：check 两侧 GREEN + diff IDENTICAL（construct 折叠到 spawn）。
    fa = os.path.join(tmp, "a.tsv")
    fb = os.path.join(tmp, "b.tsv")
    _write(fa, _stream_lines(42, [SITE_CTOR_C, SITE_AI_C]))
    _write(fb, _stream_lines(42, [SITE_SPAWN_R, SITE_AI_R]))
    rc_a, rep_a = check_stream(fa, "csharp", 42)
    rc_b, rep_b = check_stream(fb, "rust", 42)
    add("check-green", rc_a == 0 and rc_b == 0, "rc_a=%d rc_b=%d" % (rc_a, rc_b))
    d = cmd_diff(argparse.Namespace(a=fa, b=fb, a_side="csharp", b_side="rust", seed=42, scenario=None))
    add("diff-identical-green", d == 0, "rc=%d" % d)

    # ② 断号必红。
    fgap = os.path.join(tmp, "gap.tsv")
    _write(fgap, _stream_lines(42, ["a", "b", "c"], gap_after=0))
    rc, rep = check_stream(fgap, "csharp", 42)
    add("seq-gap-red", rc == 1 and any(p.startswith("seq-gap") for p in rep["problems"]),
        ",".join(rep["problems"]))

    # ③ 重复 seq 必红。
    fdup = os.path.join(tmp, "dup.tsv")
    _write(fdup, _stream_lines(42, ["a", "b", "c"], dupe_at=2))
    rc, rep = check_stream(fdup, "csharp", 42)
    add("seq-dupe-red", rc == 1 and any(p.startswith("seq-dupe") for p in rep["problems"]),
        ",".join(rep["problems"]))

    # ④ 头缺失必红。
    fnh = os.path.join(tmp, "noheader.tsv")
    _write(fnh, _stream_lines(42, ["a"], header=False))
    rc, rep = check_stream(fnh, "csharp", 42)
    add("header-inject-missing-red", rc == 1 and "header-inject-missing" in rep["problems"],
        ",".join(rep["problems"]))

    # ⑤ 头异种子必红。
    fhs = os.path.join(tmp, "seed43.tsv")
    _write(fhs, _stream_lines(43, ["a"]))
    rc, rep = check_stream(fhs, "csharp", 42)
    add("header-seed-mismatch-red", rc == 1 and any(p.startswith("header-seed-mismatch") for p in rep["problems"]),
        ",".join(rep["problems"]))

    # ⑥ 行形状必红（3 列）。
    fbad = os.path.join(tmp, "badshape.tsv")
    _write(fbad, ["1\tINJECT_OK\t42\t0", "1\tNext(max)\t100"])
    rc, rep = check_stream(fbad, "csharp", 42)
    add("row-shape-red", rc == 1 and any(p.startswith("row-shape") for p in rep["problems"]),
        ",".join(rep["problems"]))

    # ⑦ 缺 site 列必红（4 列；MIR2_RNG_SITE 未开）。
    fnosite = os.path.join(tmp, "nosite.tsv")
    _write(fnosite, _stream_lines(42, ["a", "b"], drop_site_at=0))
    rc, rep = check_stream(fnosite, "csharp", 42)
    add("site-missing-red", rc == 1 and any(p.startswith("site-missing") for p in rep["problems"]),
        ",".join(rep["problems"]))

    # ⑧ 缺件 PREREQ。
    rc, rep = check_stream(os.path.join(tmp, "nope.tsv"), "csharp", 42)
    add("missing-file-prereq", rc == 2 and "stream-missing" in rep["problems"], ",".join(rep["problems"]))

    # ⑨ 折叠次序不等（计数相等）⇒ diff 必红且点名 SHARED_REL_ORDER_DIFF。
    #    折叠后：A = [spawn, monster_ai, spawn]；B = [spawn, spawn, monster_ai]（计数相等、次序不同）。
    foA = os.path.join(tmp, "orderA.tsv")
    foB = os.path.join(tmp, "orderB.tsv")
    _write(foA, _stream_lines(42, [SITE_CTOR_C, SITE_AI_C, "GameSrv.Word.WorldServer.CreateMonster@WorldServer.MonGen.cs:498"]))
    _write(foB, _stream_lines(42, [SITE_SPAWN_R, SITE_SPAWN_R, SITE_AI_R]))
    out = subprocess.run([sys.executable, os.path.abspath(__file__), "diff", foA, foB,
                          "--a-side", "csharp", "--b-side", "rust", "--seed", "42"],
                         capture_output=True, text=True, encoding="utf-8", errors="replace")
    add("fold-order-diff-red", out.returncode == 1 and "SHARED_REL_ORDER_DIFF" in out.stdout,
        "rc=%d" % out.returncode)

    # ⑩ 拒比对照：B 侧断号 ⇒ diff 必须 **REFUSED**（不调用比较器、无组结论行）。
    fref = os.path.join(tmp, "refB.tsv")
    _write(fref, _stream_lines(42, [SITE_SPAWN_R, SITE_AI_R], gap_after=0))
    out2 = subprocess.run([sys.executable, os.path.abspath(__file__), "diff", fa, fref,
                           "--a-side", "csharp", "--b-side", "rust", "--seed", "42"],
                          capture_output=True, text=True, encoding="utf-8", errors="replace")
    add("diff-refuses-shape-red", out2.returncode == 1 and "REFUSED" in out2.stdout
        and "RNG_CONSUMER_GROUPDIFF" not in out2.stdout, "rc=%d" % out2.returncode)

    import shutil
    shutil.rmtree(tmp, ignore_errors=True)

    passed = sum(1 for _n, ok, _d in checks if ok)
    for name, ok, detail in checks:
        print("SELFTEST %s %s %s" % ("ok" if ok else "FAIL", name, detail))
    print("SELFTEST GREEN: rng_trace_diff selftest %d/%d" % (passed, len(checks)))
    return 0 if passed == len(checks) else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 RNG 场景驱动器：站点/次序账本严格校验 + 单一比较入口")
    sub = ap.add_subparsers(dest="cmd", required=True)

    pc = sub.add_parser("check", help="严格校验单侧流水")
    pc.add_argument("stream")
    pc.add_argument("--side", required=True, choices=("csharp", "rust"))
    pc.add_argument("--seed", type=int, default=None)
    pc.add_argument("--ledger-out", default="")
    pc.add_argument("--json", default="")
    pc.set_defaults(func=cmd_check)

    pd = sub.add_parser("diff", help="单一入口：先严格校验两侧，GREEN 才 groupdiff --fold --strict")
    pd.add_argument("a")
    pd.add_argument("b")
    pd.add_argument("--a-side", required=True, choices=("csharp", "rust"))
    pd.add_argument("--b-side", required=True, choices=("csharp", "rust"))
    pd.add_argument("--seed", type=int, default=None)
    pd.add_argument("--scenario", default="")
    pd.set_defaults(func=cmd_diff)

    ps = sub.add_parser("selftest", help="合成件自检（含改坏必红）")
    ps.set_defaults(func=cmd_selftest)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
