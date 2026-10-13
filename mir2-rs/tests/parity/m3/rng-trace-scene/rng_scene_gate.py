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

## 用法

    python -I rng_scene_gate.py gate --scenario rng_trace_scenario.json \
        --a <csharp-scene.json> --b <rust-scene.json> [--json out.json]
    python -I rng_scene_gate.py selftest

退出码：0 = GREEN（同场景）；1 = RED（场景不一致，点名哪一项）；2 = PREREQ（缺件 / 坏 JSON）。
"""

import argparse
import collections
import json
import os
import sys

SCENARIO_CANON = "m3-rng-trace-scene-v1"
DESCRIPTOR_CANON = "m3-rng-scene-descriptor-v1"
HERE = os.path.dirname(os.path.abspath(__file__))


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


def run_gate(scenario_path, a_path, b_path):
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


def cmd_gate(args):
    res = run_gate(args.scenario, args.a, args.b)
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

        def run(scen_p, a_p, b_p):
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
                rc = cmd_gate(argparse.Namespace(scenario=scen_p, a=a_p, b=b_p, json=""))
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
                             "mongen_fixture": "mongen-mapR.tsv"})
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
    sys.stdout.write("GREEN: 场景前置门 selftest 全绿（同场景绿 + seed/怪集合/起点/map/MonGen 漂移必红 + 缺件/坏 JSON PREREQ fail-closed）\n")
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 RNG trace scene consistency pre-gate")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("gate")
    p.add_argument("--scenario", required=True)
    p.add_argument("--a", required=True, help="csharp 侧场景描述件 (-scene.json)")
    p.add_argument("--b", required=True, help="rust 侧场景描述件 (-scene.json)")
    p.add_argument("--json", default="")
    p.set_defaults(func=cmd_gate)

    p = sub.add_parser("selftest")
    p.set_defaults(func=cmd_selftest)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
