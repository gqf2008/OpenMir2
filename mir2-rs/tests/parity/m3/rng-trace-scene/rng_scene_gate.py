#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""rng_scene_gate.py —— M3「同场景 / 同起点 / 同种子」RNG 站点组 trace 的**场景一致性前置门**。

## 为什么有这道门

卡 `openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）的判据 ① 明确要求：

> **场景一致性前置门**：两侧 trace 必须来自同场景/同起点/同种子（MonGen 与 Monster 集合同口径、
> k 段对齐），不满足 ⇒ `groupdiff` fail-closed 拒比（不许把场景差异当实现差异，也不许当绿）。

Claim B 基线报告（`M3全服RNG消费者结构一致性基线报告-20261013.md` §2.2）已经吃过一次教训：
现存唯一一对两侧活栈 trace 是**两个不同场景**各起的栈 ⇒ 组集合差可信，但**相对次序差不可作实现缺陷判据**。
本门就是把这条「先决条件」变成机判：**没有同场景证据，就不许拿 `groupdiff` 的次序结论下判**。

## 门比什么（两侧各产出一份「场景描述件」`*-scene.json`）

`canon = m3-rng-scene-descriptor-v1`：

    {
      "side": "csharp" | "rust",
      "canon": "m3-rng-scene-descriptor-v1",
      "map": "0137",
      "seed": 42,
      "start_pose": {"x": 17, "y": 20, "dir": 2},
      "mongen_rows": [["333", "268", "10"], ...],   # 该图在 Envir/MonGen.txt 里的刷怪行（原始列）
      "monsters": [{"name": "稻草人", "x": 18, "y": 20, "dir": 0, "count": 1}, ...],
      "trace": "…/rng-stream.tsv"                    # 可选：本侧全场景抽数流水
    }

门把 `mongen_rows` 与**冻结提取件**（`fixtures/mongen-map<map>.tsv`）逐行多重集对账
（= 「两侧 MonGen 集合同口径」，两侧都必须等于同一份冻结件），把 `monsters` 多重集
两侧互比且等于 scenario 声明（= 「Monster 集合同口径」），并核对 `map` / `seed` / `start_pose`。
任一项不等或缺失 ⇒ **fail-closed 点名 RED**，`groupdiff` 的次序结论不得采信。

**制品级同种子（`--a-stream` / `--b-stream`）**：描述件里的 `seed` 是**自报**，不是证据。
DoD① 要求「两侧 trace 必须来自同场景/同起点/**同种子**」⇒ 门在两份 trace 流水在场时，还要核对
**流水头**的 `INJECT_OK <seed>`（两侧生产者都写：C# `rng-hook` 与 Rust `crates/shared/src/rng_trace.rs`）：
缺该行 ⇒ 无法证明种子（fail-closed RED）；`INJECT_OK <seed>` ≠ scenario.seed ⇒ RED。
未给流水 ⇒ 该组检查不参与（保持既有描述件级门行为，向后兼容）。

## 用法

    python -I rng_scene_gate.py gate --scenario rng_trace_scenario.json \
        --a <csharp-scene.json> --b <rust-scene.json> \
        [--a-stream <csharp-rng-stream.tsv> --b-stream <rust-rng-stream.tsv>] [--json out.json]
    python -I rng_scene_gate.py scenario --scenario rng_trace_scenario.json [--json out.json]
    python -I rng_scene_gate.py selftest

退出码：0 = GREEN（同场景 / scenario 与代码同口径）；1 = RED（场景不一致 或 声明漂移，点名哪一项）；
2 = PREREQ（缺件 / 坏 JSON）。
"""

import argparse
import collections
import json
import os
import re
import sys

SCENARIO_CANON = "m3-rng-trace-scene-v1"
DESCRIPTOR_CANON = "m3-rng-scene-descriptor-v1"
HERE = os.path.dirname(os.path.abspath(__file__))

# 账本实现（`rng_consumer_groups.FOLD_RULES` / `KNOWN_GROUPS`）是 scenario 所声明
# 「folded 映射 / 组名清单」的**真源**。门要机判 scenario 声明与代码同口径（M-1 的 sibling：
# 同一族 doc-vs-code 漂移），故直接 import 同一份实现，而不是在手抄一份又会漂。
_PROBE_DIR = os.path.normpath(os.path.join(HERE, os.pardir, "rng-trace-probe"))
if _PROBE_DIR not in sys.path:
    sys.path.insert(0, _PROBE_DIR)
import rng_consumer_groups as rcg  # noqa: E402  (路径修正后再导入)


def _read_json(path):
    with open(path, "r", encoding="utf-8-sig") as fh:
        return json.load(fh)


def _as_int(v):
    if isinstance(v, bool):
        return None
    try:
        return int(v)
    except (TypeError, ValueError):
        return None


def parse_pose(obj):
    """`{x,y,dir}` → `(x, y, dir)`；任一项缺失/非法 ⇒ None。"""
    if not isinstance(obj, dict):
        return None
    x, y, d = _as_int(obj.get("x")), _as_int(obj.get("y")), _as_int(obj.get("dir"))
    if x is None or y is None or d is None:
        return None
    return (x, y, d)


def parse_monsters(obj):
    """monster 列表 → `Counter((name, x, y, dir) -> count)`；任一项非法 ⇒ None。

    name 必须非空；x/y/dir 必须是整数（缺省用 -1 表示「未声明」而不是猜一个值）。
    `count` 缺省 1。
    """
    if not isinstance(obj, list):
        return None
    out = collections.Counter()
    for m in obj:
        if not isinstance(m, dict):
            return None
        name = m.get("name")
        if not isinstance(name, str) or not name.strip():
            return None
        x = _as_int(m.get("x", -1))
        y = _as_int(m.get("y", -1))
        d = _as_int(m.get("dir", -1))
        c = _as_int(m.get("count", 1))
        if x is None or y is None or d is None or c is None or c <= 0:
            return None
        out[(name.strip(), x, y, d)] += c
    return out


def fixture_rows(path):
    """冻结 MonGen 提取件 → `Counter(tuple(row_columns))`（跳过空行 / `#` 注释）。"""
    rows = collections.Counter()
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            line = line.rstrip("\r\n")
            if not line or line.lstrip().startswith("#"):
                continue
            rows[tuple(line.split("\t"))] += 1
    return rows


def stream_inject_seed(path):
    """从 RNG 流水**头部**读 `INJECT_OK <seed>`（制品级种子证据）。

    两侧生产者都在流水头写 `1\tINJECT_OK\t<seed>\t0`（C# `rng-hook` 与 Rust
    `crates/shared/src/rng_trace.rs`）。返回 `(found, seed, lineno)`：`found=False` 表示
    扫描窗口内没有该行 ⇒ 无法证明该侧 trace 的种子（调用方必须 fail-closed，不许静默绿）。
    文件不可读 ⇒ 抛 `OSError`（由调用方转 PREREQ）。
    """
    with open(path, "r", encoding="utf-8-sig", errors="replace") as fh:
        for lineno, line in enumerate(fh, 1):
            if lineno > 100:
                break
            parts = line.rstrip("\r\n").split("\t")
            if len(parts) >= 3 and parts[1].strip() == "INJECT_OK":
                seed = _as_int(parts[2])
                if seed is not None:
                    return True, seed, lineno
    return False, None, None


def _mongen_counter(obj):
    if not isinstance(obj, list):
        return None
    out = collections.Counter()
    for r in obj:
        if not isinstance(r, list) or not r:
            return None
        out[tuple(str(c) for c in r)] += 1
    return out


class GateResult(object):
    def __init__(self):
        self.checks = []          # [(name, ok, detail)]
        self.prereq = []          # 前置缺失（rc=2）
        self.scene = {}

    def check(self, name, ok, detail=""):
        self.checks.append((name, bool(ok), detail))

    def fail_prereq(self, name, detail):
        self.prereq.append((name, detail))
        self.checks.append((name, False, detail))

    @property
    def failed(self):
        return [(n, d) for n, ok, d in self.checks if not ok]


def scenario_trace_checks(scenario, res):
    """scenario 级「声明 vs 代码」一致性（M-1 sibling）。

    scenario 是人对夹具的**声明**，账本实现（`rcg.FOLD_RULES` / `rcg.KNOWN_GROUPS`）是**真源**。
    两者漂移 ⇒ `groupdiff --fold` 用的映射与 scenario 写的不是一回事、`groups_expected` 里
    写着账本根本不认识的组，而无人察觉。因此把这两项也做成机判（缺声明 ⇒ fail-closed RED，
    不许静默绿）。判据锚在代码真源上，不手抄常量。
    """
    trace = scenario.get("trace")
    res.check("scenario-trace-present", isinstance(trace, dict),
              "scenario.trace=%r（必须是对象）" % (trace,))
    if not isinstance(trace, dict):
        trace = {}

    # 代码侧折叠规则自身必须自洽（已知组→已知组、源≠目标）。
    fold_problems = rcg.validate_folds()
    res.check("scenario-fold-code-valid", not fold_problems, "; ".join(fold_problems))

    # scenario 声明的 folded 映射必须与代码 FOLD_RULES 逐项相等。
    declared = trace.get("fold")
    res.check("scenario-fold-declared", isinstance(declared, dict) and len(declared) >= 1,
              "scenario.trace.fold=%r（必须显式声明折叠映射）" % (declared,))
    res.check("scenario-fold-matches-code",
              isinstance(declared, dict)
              and {str(k): str(v) for k, v in declared.items()} == dict(rcg.FOLD_RULES),
              "scenario.trace.fold=%r code.FOLD_RULES=%r" % (declared, dict(rcg.FOLD_RULES)))

    # groups_expected 里每个组名都必须是账本已知组。
    groups = trace.get("groups_expected")
    res.check("scenario-groups-declared", isinstance(groups, list) and len(groups) >= 1,
              "scenario.trace.groups_expected=%r（必须显式声明组清单）" % (groups,))
    unknown = [g for g in groups if g not in rcg.KNOWN_GROUPS] if isinstance(groups, list) else []
    res.check("scenario-groups-known", isinstance(groups, list) and not unknown,
              "未知组 %s（已知：%s）" % (unknown, "、".join(rcg.KNOWN_GROUPS)))


def run_gate(scenario_path, a_path, b_path, a_stream=None, b_stream=None):
    res = GateResult()
    for label, path in (("scenario", scenario_path), ("a", a_path), ("b", b_path)):
        if not path or not os.path.exists(path):
            res.fail_prereq("file-exists:" + label, "找不到 %s" % (path or "(空)"))
    if res.prereq:
        return res

    try:
        scenario = _read_json(scenario_path)
    except (OSError, ValueError) as exc:
        res.fail_prereq("scenario-parse", str(exc))
        return res
    try:
        a = _read_json(a_path)
        b = _read_json(b_path)
    except (OSError, ValueError) as exc:
        res.fail_prereq("descriptor-parse", str(exc))
        return res

    sc_map = scenario.get("map")
    sc_seed = _as_int(scenario.get("seed"))
    sc_pose = parse_pose(scenario.get("start_pose"))
    sc_monsters = parse_monsters(scenario.get("monsters"))
    res.scene = {"map": sc_map, "seed": sc_seed,
                 "start_pose": sc_pose,
                 "monsters": _mset_to_json(sc_monsters)}

    res.check("scenario-canon", scenario.get("canon") == SCENARIO_CANON,
              "scenario.canon=%r" % scenario.get("canon"))
    res.check("scenario-fields", bool(sc_map) and sc_seed is not None
              and sc_pose is not None and sc_monsters is not None,
              "map/seed/start_pose/monsters 必须齐备且可解析")
    # scenario 声明的 folded 映射 / 组名清单必须与账本实现同口径（M-1 sibling）。
    scenario_trace_checks(scenario, res)

    # 两侧描述件自身可解析。
    for label, d in (("a", a), ("b", b)):
        res.check("descriptor-canon:" + label, d.get("canon") == DESCRIPTOR_CANON,
                  "%s.canon=%r" % (label, d.get("canon")))
        res.check("descriptor-side:" + label, d.get("side") in ("csharp", "rust"),
                  "%s.side=%r" % (label, d.get("side")))
        res.scene[label + "_side"] = d.get("side")
        res.scene[label + "_monsters"] = _mset_to_json(parse_monsters(d.get("monsters")))

    # side 两侧必须不同（一 csharp 一 rust）。
    res.check("sides-distinct", a.get("side") != b.get("side") and
              {a.get("side"), b.get("side")} == {"csharp", "rust"},
              "side a=%r b=%r" % (a.get("side"), b.get("side")))

    # map / seed / start_pose。
    for label, d in (("a", a), ("b", b)):
        res.check("map-equal:" + label, d.get("map") == sc_map,
                  "%s.map=%r scenario.map=%r" % (label, d.get("map"), sc_map))
        res.check("seed-equal:" + label, _as_int(d.get("seed")) == sc_seed,
                  "%s.seed=%r scenario.seed=%r" % (label, d.get("seed"), sc_seed))
        res.check("pose-equal:" + label, parse_pose(d.get("start_pose")) == sc_pose,
                  "%s.start_pose=%r scenario.start_pose=%r"
                  % (label, parse_pose(d.get("start_pose")), sc_pose))

    # Monster 集合同口径：a == b == scenario。
    ma, mb = parse_monsters(a.get("monsters")), parse_monsters(b.get("monsters"))
    res.check("monsters-parseable", ma is not None and mb is not None,
              "两侧 monsters 必须可解析")
    if ma is not None and sc_monsters is not None:
        res.check("monsters-equal:scenario-vs-a", ma == sc_monsters,
                  "a=%s scenario=%s" % (dict(ma), dict(sc_monsters)))
    if mb is not None and sc_monsters is not None:
        res.check("monsters-equal:scenario-vs-b", mb == sc_monsters,
                  "b=%s scenario=%s" % (dict(mb), dict(sc_monsters)))
    if ma is not None and mb is not None:
        res.check("monsters-equal:a-vs-b", ma == mb,
                  "a=%s b=%s" % (dict(ma), dict(mb)))

    # MonGen 集合同口径：两侧都等于同一份冻结提取件（scenario.mongen_fixture）。
    fixture_rel = scenario.get("mongen_fixture") or ""
    # 相对路径锚在 **scenario 文件所在目录**（scenario 自带相对引用；真实 scenario 写 `../fixtures/...`）。
    fixture_path = (fixture_rel if os.path.isabs(fixture_rel)
                    else os.path.join(os.path.dirname(os.path.abspath(scenario_path)), fixture_rel))
    fixture_path = os.path.normpath(fixture_path)
    if not fixture_rel or not os.path.exists(fixture_path):
        res.fail_prereq("mongen-fixture-exists", "找不到 MonGen 冻结件 %s" % fixture_path)
    else:
        try:
            fx = fixture_rows(fixture_path)
        except OSError as exc:
            res.fail_prereq("mongen-fixture-parse", str(exc))
            fx = None
        if fx is not None:
            ga, gb = _mongen_counter(a.get("mongen_rows")), _mongen_counter(b.get("mongen_rows"))
            res.check("mongen-parseable", ga is not None and gb is not None,
                      "两侧 mongen_rows 必须可解析（list of list）")
            if ga is not None:
                res.check("mongen-equal:a-vs-fixture", ga == fx,
                          "a_rows=%d fixture_rows=%d" % (sum(ga.values()), sum(fx.values())))
            if gb is not None:
                res.check("mongen-equal:b-vs-fixture", gb == fx,
                          "b_rows=%d fixture_rows=%d" % (sum(gb.values()), sum(fx.values())))
            if ga is not None and gb is not None:
                res.check("mongen-equal:a-vs-b", ga == gb,
                          "a_rows=%d b_rows=%d" % (sum(ga.values()), sum(gb.values())))
            res.scene["mongen_rows"] = sum(fx.values())
            try:
                res.scene["mongen_fixture"] = os.path.relpath(fixture_path, HERE)
            except ValueError:
                # 跨盘（冻结件在 scenario 同目录、脚本在另一盘）⇒ 直接记 scenario 声明的相对路径。
                res.scene["mongen_fixture"] = fixture_rel

    # 制品级同种子：描述件自报不算数，核两份 trace 流水头的 `INJECT_OK <seed>`。
    # 任一侧给了流水但缺该行 / 与 scenario.seed 不等 ⇒ fail-closed 点名（DoD① fail-closed 拒比）。
    if a_stream or b_stream:
        seeds = {}
        for label, stream in (("a", a_stream), ("b", b_stream)):
            if not stream:
                continue
            if not os.path.exists(stream):
                res.fail_prereq("stream-exists:" + label, "找不到流水 %s" % stream)
                continue
            try:
                found, seed, lineno = stream_inject_seed(stream)
            except OSError as exc:
                res.fail_prereq("stream-read:" + label, str(exc))
                continue
            res.scene[label + "_stream"] = stream
            res.scene[label + "_stream_seed"] = seed
            if not found:
                res.check("stream-inject-ok:" + label, False,
                          "%s 流水头 100 行内无 `INJECT_OK <seed>` ⇒ 无法证明该侧 trace 的种子（fail-closed）"
                          % stream)
            elif sc_seed is not None and seed != sc_seed:
                res.check("stream-seed-equal:" + label, False,
                          "%s 流水头 INJECT_OK seed=%r scenario.seed=%r" % (stream, seed, sc_seed))
            else:
                res.check("stream-seed-equal:" + label, True)
                seeds[label] = seed
        if "a" in seeds and "b" in seeds:
            res.check("stream-seed-equal:a-vs-b", seeds["a"] == seeds["b"],
                      "a=%r b=%r" % (seeds["a"], seeds["b"]))
    return res


def _mset_to_json(counter):
    if counter is None:
        return None
    return {"|".join(str(x) for x in k): v for k, v in sorted(counter.items(), key=lambda kv: kv[0])}


def print_gate(res, scenario_path, a_path, b_path):
    total, failed = len(res.checks), res.failed
    print("RNG_SCENE_GATE scenario=%s a=%s b=%s checks=%d failed=%d"
          % (scenario_path, a_path, b_path, total, len(failed)))
    print("  SCENE %s" % json.dumps(res.scene, ensure_ascii=False, sort_keys=True))
    for name, ok, detail in res.checks:
        print("  CHECK %-34s %s%s" % (name, "ok" if ok else "FAIL",
                                      (" (%s)" % detail) if detail and not ok else ""))
    for name, detail in res.prereq:
        print("  PREREQ_MISSING %s: %s" % (name, detail))
    if failed:
        print("  VERDICT RED: 场景不一致 ⇒ groupdiff 次序结论不得采信（fail-closed）：%s"
              % ", ".join(n for n, _ in failed))
    else:
        print("  VERDICT GREEN: 两侧同场景/同起点/同种子（MonGen+Monster 集合同口径）")


def cmd_scenario(args):
    """只校验 scenario 件自身（不需要两侧描述件）：canon/字段 + 声明 vs 代码一致。

    供无栈自检在**离线**上锁住已入库的 `rng_trace_scenario.json`（不让它随代码漂移）；
    与 `gate` 的差别仅在于不检两侧描述件。退出码：0 GREEN / 1 RED / 2 PREREQ。
    """
    res = GateResult()
    if not args.scenario or not os.path.exists(args.scenario):
        res.fail_prereq("file-exists:scenario", "找不到 %s" % (args.scenario or "(空)"))
    else:
        try:
            scenario = _read_json(args.scenario)
        except (OSError, ValueError) as exc:
            res.fail_prereq("scenario-parse", str(exc))
        else:
            res.check("scenario-canon", scenario.get("canon") == SCENARIO_CANON,
                      "scenario.canon=%r" % scenario.get("canon"))
            res.check("scenario-fields",
                      bool(scenario.get("map")) and _as_int(scenario.get("seed")) is not None
                      and parse_pose(scenario.get("start_pose")) is not None
                      and parse_monsters(scenario.get("monsters")) is not None,
                      "map/seed/start_pose/monsters 必须齐备且可解析")
            scenario_trace_checks(scenario, res)
    total, failed = len(res.checks), res.failed
    print("RNG_SCENE_SCENARIO scenario=%s checks=%d failed=%d" % (args.scenario, total, len(failed)))
    for name, ok, detail in res.checks:
        print("  CHECK %-34s %s%s" % (name, "ok" if ok else "FAIL",
                                        (" (%s)" % detail) if detail and not ok else ""))
    for name, detail in res.prereq:
        print("  PREREQ_MISSING %s: %s" % (name, detail))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"scenario": args.scenario,
                       "checks": [{"name": n, "ok": ok, "detail": d} for n, ok, d in res.checks],
                       "prereq": [{"name": n, "detail": d} for n, d in res.prereq],
                       "verdict": "PREREQ" if res.prereq else ("RED" if res.failed else "GREEN")},
                      fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    if res.prereq:
        print("  VERDICT PREREQ: scenario 件缺失/不可解析（fail-closed）")
        return 2
    if res.failed:
        print("  VERDICT RED: scenario 声明与代码真源不一致（fail-closed）：%s"
              % ", ".join(n for n, _ in failed))
        return 1
    print("  VERDICT GREEN: scenario 自洽且 folded 映射/组清单与代码同口径")
    return 0


def cmd_gate(args):
    res = run_gate(args.scenario, args.a, args.b,
                   a_stream=getattr(args, "a_stream", ""), b_stream=getattr(args, "b_stream", ""))
    print_gate(res, args.scenario, args.a, args.b)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"scenario": args.scenario, "a": args.a, "b": args.b,
                       "scene": res.scene,
                       "checks": [{"name": n, "ok": ok, "detail": d} for n, ok, d in res.checks],
                       "prereq": [{"name": n, "detail": d} for n, d in res.prereq],
                       "verdict": "PREREQ" if res.prereq else ("RED" if res.failed else "GREEN")},
                      fh, ensure_ascii=False, indent=2)
            fh.write("\n")
    if res.prereq:
        return 2
    return 1 if res.failed else 0


# --------------------------------------------------------------------------- #
# selftest —— 合成件正/负样本 + 改坏必红 + 缺件 fail-closed（离线可跑、零端口）
# --------------------------------------------------------------------------- #

def _write_json(path, obj):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(obj, fh, ensure_ascii=False, indent=2)
        fh.write("\n")


def _mk_scenario(path, mongen_rel):
    _write_json(path, {
        "canon": SCENARIO_CANON,
        "status": "skeleton-window-pending",
        "seed": 42,
        "map": "0137",
        "start_pose": {"x": 17, "y": 20, "dir": 2},
        "monsters": [{"name": "稻草人", "x": 18, "y": 20, "dir": 0, "count": 1}],
        "mongen_fixture": mongen_rel,
        # scenario 声明的 folded 映射/组清单必须与代码真源同口径（机判）；合成件直接用真源值。
        "trace": {"fold": dict(rcg.FOLD_RULES),
                  "groups_expected": ["monster_ai", "spawn", "drop"]},
    })


def _mk_descriptor(path, side, *, seed=42, map_name="0137", pose=(17, 20, 2),
                   monsters=None, mongen=None):
    if monsters is None:
        monsters = [{"name": "稻草人", "x": 18, "y": 20, "dir": 0, "count": 1}]
    if mongen is None:
        mongen = []
    _write_json(path, {
        "canon": DESCRIPTOR_CANON,
        "side": side,
        "map": map_name,
        "seed": seed,
        "start_pose": {"x": pose[0], "y": pose[1], "dir": pose[2]},
        "mongen_rows": mongen,
        "monsters": monsters,
        "trace": "%s-rng-stream.tsv" % side,
    })


def cmd_selftest(_args):
    import contextlib
    import io
    import tempfile

    results = []

    def check(name, ok, detail=""):
        results.append((name, bool(ok), detail))

    tmp = tempfile.mkdtemp(prefix="rng-scene-gate-selftest-")
    try:
        # 冻结件：合成目录里造两个 map 的 MonGen 提取件（含 0 行的 0137）。
        fx0137 = os.path.join(tmp, "mongen-map0137.tsv")
        with open(fx0137, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("# map=0137 零刷怪\n")
        fxR = os.path.join(tmp, "mongen-mapR.tsv")
        with open(fxR, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("# map=R\n7\t9\t15\n7\t9\t15\n")

        scen = os.path.join(tmp, "scenario.json")
        _mk_scenario(scen, "mongen-map0137.tsv")
        a1 = os.path.join(tmp, "a-ok.json")
        b1 = os.path.join(tmp, "b-ok.json")
        _mk_descriptor(a1, "csharp")
        _mk_descriptor(b1, "rust")

        def run(scen_p, a_p, b_p, a_stream="", b_stream=""):
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
                rc = cmd_gate(argparse.Namespace(scenario=scen_p, a=a_p, b=b_p,
                                                 a_stream=a_stream, b_stream=b_stream, json=""))
            return rc, buf.getvalue()

        def run_scen(scen_p):
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
                rc = cmd_scenario(argparse.Namespace(scenario=scen_p, json=""))
            return rc, buf.getvalue()

        rc, out = run(scen, a1, b1)
        check("identical-scene-green", rc == 0 and "VERDICT GREEN" in out, out[:300])
        check("green-has-checks", "CHECK map-equal:a" in out and "CHECK mongen-equal:a-vs-fixture" in out,
              out[:400])

        # 改坏①：两侧 seed 不同 ⇒ 必红且点名 seed-equal。
        b2 = os.path.join(tmp, "b-seed.json")
        _mk_descriptor(b2, "rust", seed=43)
        rc, out = run(scen, a1, b2)
        check("seed-diff-red", rc == 1 and "seed-equal:b" in out and "VERDICT RED" in out, out[:500])

        # 改坏②：怪集合不同（多一只）⇒ 必红且点名 monsters。
        b3 = os.path.join(tmp, "b-monsters.json")
        _mk_descriptor(b3, "rust", monsters=[
            {"name": "稻草人", "x": 18, "y": 20, "dir": 0, "count": 1},
            {"name": "鸡", "x": 19, "y": 20, "dir": 0, "count": 1}])
        rc, out = run(scen, a1, b3)
        check("monster-set-diff-red",
              rc == 1 and "monsters-equal:a-vs-b" in out and "monsters-equal:scenario-vs-b" in out,
              out[:500])

        # 改坏③：起点不同 ⇒ 必红。
        b4 = os.path.join(tmp, "b-pose.json")
        _mk_descriptor(b4, "rust", pose=(17, 21, 2))
        rc, out = run(scen, a1, b4)
        check("pose-diff-red", rc == 1 and "pose-equal:b" in out, out[:400])

        # 改坏④：MonGen 集合同口径被破坏（map=R，两侧声明 2 行 vs 冻结件 2 行；但 scenario 指向 0137）
        # ⇒ map-equal + mongen 都必红。
        b5 = os.path.join(tmp, "b-mongen.json")
        _mk_descriptor(b5, "rust", map_name="R", mongen=[["7", "9", "15"], ["7", "9", "15"]])
        rc, out = run(scen, a1, b5)
        check("map-diff-red", rc == 1 and "map-equal:b" in out, out[:400])

        # 正向覆盖：scenario 指向含行冻结件 map=R；两侧声明同样的 2 行 ⇒ GREEN（证明门不是恒红）。
        scen_r = os.path.join(tmp, "scenario-r.json")
        _write_json(scen_r, {"canon": SCENARIO_CANON, "seed": 42, "map": "R",
                             "start_pose": {"x": 7, "y": 9, "dir": 0},
                             "monsters": [{"name": "僵尸1", "x": 7, "y": 9, "dir": 0, "count": 1}],
                             "mongen_fixture": "mongen-mapR.tsv",
                             "trace": {"fold": dict(rcg.FOLD_RULES),
                                       "groups_expected": ["monster_ai", "spawn", "drop"]}})
        ar = os.path.join(tmp, "a-r.json")
        br = os.path.join(tmp, "b-r.json")
        _mk_descriptor(ar, "csharp", map_name="R", pose=(7, 9, 0),
                       monsters=[{"name": "僵尸1", "x": 7, "y": 9, "dir": 0, "count": 1}],
                       mongen=[["7", "9", "15"], ["7", "9", "15"]])
        _mk_descriptor(br, "rust", map_name="R", pose=(7, 9, 0),
                       monsters=[{"name": "僵尸1", "x": 7, "y": 9, "dir": 0, "count": 1}],
                       mongen=[["7", "9", "15"], ["7", "9", "15"]])
        rc, out = run(scen_r, ar, br)
        check("mongen-nonempty-green", rc == 0 and "VERDICT GREEN" in out, out[:300])

        # 改坏⑤：一侧 MonGen 行数与冻结件不等 ⇒ 必红且点名 mongen-equal。
        br_bad = os.path.join(tmp, "b-r-bad.json")
        _mk_descriptor(br_bad, "rust", map_name="R", pose=(7, 9, 0),
                       monsters=[{"name": "僵尸1", "x": 7, "y": 9, "dir": 0, "count": 1}],
                       mongen=[["7", "9", "15"]])
        rc, out = run(scen_r, ar, br_bad)
        check("mongen-drift-red", rc == 1 and "mongen-equal:b-vs-fixture" in out, out[:400])

        # 缺件 ⇒ PREREQ rc=2（fail-closed，不静默绿）。
        rc, out = run(scen, a1, os.path.join(tmp, "does-not-exist.json"))
        check("missing-descriptor-prereq", rc == 2 and "PREREQ_MISSING" in out, out[:300])

        # 坏 JSON ⇒ PREREQ rc=2。
        badjson = os.path.join(tmp, "bad.json")
        with open(badjson, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("{ not json")
        rc, out = run(scen, a1, badjson)
        check("bad-json-prereq", rc == 2 and "PREREQ_MISSING" in out, out[:300])

        # 同一 side（都 csharp）⇒ 必红（不能拿同侧去比）。
        b_same = os.path.join(tmp, "b-same-side.json")
        _mk_descriptor(b_same, "csharp")
        rc, out = run(scen, a1, b_same)
        check("same-side-red", rc == 1 and "sides-distinct" in out, out[:300])

        # 制品级同种子（DoD①「两侧 trace 同种子」的**流水头**证据，而非描述件自报）。
        def mk_stream(path, seed, inject=True):
            lines = []
            if inject:
                lines.append("1\tINJECT_OK\t%d\t0" % seed)
                lines.append("SEED_PROBE 1 2 3 4 5 6 7 8")
            lines.append("1\tNext(max)\t10\t7\tMeasured.Drop@Measured.cs:10")
            with open(path, "w", encoding="utf-8", newline="\n") as fh:
                fh.write("\n".join(lines) + "\n")

        sa = os.path.join(tmp, "a-stream.tsv")
        sb = os.path.join(tmp, "b-stream.tsv")
        mk_stream(sa, 42)
        mk_stream(sb, 42)
        rc, out = run(scen, a1, b1, sa, sb)
        check("stream-seed-green",
              rc == 0 and "CHECK stream-seed-equal:a" in out and "CHECK stream-seed-equal:b" in out
              and "VERDICT GREEN" in out, out[:500])

        sb_wrong = os.path.join(tmp, "b-stream-wrong.tsv")
        mk_stream(sb_wrong, 43)
        rc, out = run(scen, a1, b1, sa, sb_wrong)
        check("stream-seed-mismatch-red",
              rc == 1 and "stream-seed-equal:b" in out and "VERDICT RED" in out, out[:500])

        sb_noheader = os.path.join(tmp, "b-stream-noheader.tsv")
        mk_stream(sb_noheader, 42, inject=False)
        rc, out = run(scen, a1, b1, sa, sb_noheader)
        check("stream-inject-header-missing-red",
              rc == 1 and "stream-inject-ok:b" in out and "VERDICT RED" in out, out[:500])

        rc, out = run(scen, a1, b1, sa, os.path.join(tmp, "no-such-stream.tsv"))
        check("stream-missing-prereq", rc == 2 and "PREREQ_MISSING" in out, out[:300])

        # 未给流水 ⇒ 保持既有描述件级门行为（stream 检查不参与；向后兼容）。
        rc, out = run(scen, a1, b1)
        check("stream-absent-backward-compatible",
              rc == 0 and "stream-seed-equal" not in out and "stream-inject-ok" not in out, out[:300])

        # `scenario` 模式（离线锁住已入库 scenario）：正样本绿 / 缺件 PREREQ。
        rc, out = run_scen(scen)
        check("scenario-mode-green", rc == 0 and "VERDICT GREEN" in out, out[:300])
        rc, out = run_scen(os.path.join(tmp, "scenario-does-not-exist.json"))
        check("scenario-mode-prereq", rc == 2 and "PREREQ_MISSING" in out, out[:200])

        # 改坏⑥：scenario 声明的 folded 映射与代码 FOLD_RULES 漂移 ⇒ 必红且点名。
        scen_fold_drift = os.path.join(tmp, "scenario-fold-drift.json")
        scen_obj = _read_json(scen)
        scen_obj["trace"] = {"fold": {"construct": "monster_ai"},   # 已知组但 ≠ 代码映射
                             "groups_expected": ["monster_ai", "spawn", "drop"]}
        _write_json(scen_fold_drift, scen_obj)
        rc, out = run(scen_fold_drift, a1, b1)
        check("scenario-fold-drift-red",
              rc == 1 and "scenario-fold-matches-code" in out and "VERDICT RED" in out, out[:500])

        # 改坏⑦：scenario 缺 trace.fold 声明 ⇒ fail-closed 红（不许静默绿）。
        scen_fold_missing = os.path.join(tmp, "scenario-fold-missing.json")
        scen_obj = _read_json(scen)
        scen_obj["trace"] = {"groups_expected": ["monster_ai"]}
        _write_json(scen_fold_missing, scen_obj)
        rc, out = run(scen_fold_missing, a1, b1)
        check("scenario-fold-missing-red",
              rc == 1 and "scenario-fold-declared" in out, out[:400])

        # 改坏⑧：groups_expected 里出现账本不认识的组 ⇒ 必红且点名。
        scen_grp_bad = os.path.join(tmp, "scenario-groups-unknown.json")
        scen_obj = _read_json(scen)
        scen_obj["trace"] = {"fold": dict(rcg.FOLD_RULES),
                             "groups_expected": ["monster_ai", "no_such_group"]}
        _write_json(scen_grp_bad, scen_obj)
        rc, out = run(scen_grp_bad, a1, b1)
        check("scenario-groups-unknown-red",
              rc == 1 and "scenario-groups-known" in out, out[:400])
    finally:
        for name in os.listdir(tmp):
            try:
                os.remove(os.path.join(tmp, name))
            except OSError:
                pass
        os.rmdir(tmp)

    failed = [n for n, ok, _d in results if not ok]
    sys.stdout.write("SELFTEST ok=%d/%d\n" % (len(results) - len(failed), len(results)))
    for n, ok, d in results:
        if not ok:
            sys.stdout.write("  FAIL %s (%s)\n" % (n, d))
    if failed:
        return 1
    sys.stdout.write("GREEN: 场景前置门 selftest 全绿（同场景绿 + seed/怪集合/起点/map/MonGen 漂移必红 "
                     "+ 流水头同种子制品级门（同种子绿 / 异种子·缺 INJECT_OK 必红 / 缺件 PREREQ / 未给流水向后兼容） "
                     "+ scenario 声明 vs 代码 folded 映射/组清单漂移必红 + 缺件/坏 JSON PREREQ fail-closed）\n")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 RNG trace scene consistency pre-gate")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("scenario", help="只校验 scenario 件自身（声明 vs 代码真源）")
    p.add_argument("--scenario", required=True)
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_scenario)

    p = sub.add_parser("gate")
    p.add_argument("--scenario", required=True)
    p.add_argument("--a", required=True, help="csharp 侧场景描述件 (-scene.json)")
    p.add_argument("--b", required=True, help="rust 侧场景描述件 (-scene.json)")
    p.add_argument("--a-stream", default="", help="csharp 侧 RNG 流水（核流水头 INJECT_OK 种子）")
    p.add_argument("--b-stream", default="", help="rust 侧 RNG 流水（核流水头 INJECT_OK 种子）")
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_gate)

    p = sub.add_parser("selftest")
    p.set_defaults(func=cmd_selftest)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
