#!/usr/bin/env python3
"""同窗口批次前提机判：本卡（RNG 站点组）与 ④（drop-exp 100 杀）的两个场景件
必须真的是**同图 / 同种子 / 同起点**——把「同窗口同跑」从散文变成 fail-closed 机判。

背景（为什么需要这条牙）
------------------------
卡 DoD④（`openmir2-m3-rng-trace-scene-groups`）与 `README.md §4` 把两套**判据对象不同**的
夹具编排在**同一个独占窗口批次**里串行跑（本轮 RNG 站点组 trace → 停栈 → ④ 100 杀）：
- 本卡比「掉落点的 RNG 取数账本」（`MonGetRandomItems` 的抽数/次序）；
- ④ 比「死亡掉落的落地产物」（无地面物 / 无 `RM_ITEMSHOW` / 无 `SM_ITEMSHOW(610)`）。
二者**不共享判据**、可以同窗口同跑，**前提**是两侧场景件声明的是
**同图 + 同种子 + 同起点**（`rng_trace_scenario.json` 的 `map/seed/start_pose`
与 `drop_exp_scenario.json` 的 `map/seed/attacker.{x,y,dir}`）。

但这条前提此前**只有散文**：
- `rng_scene_gate.py` 只比**同一张卡**的两侧 trace（scene a vs scene b）；
- `drop_exp_purity_gate.py` 只看 ④ 的**纯度**（除被测量路径外消费者 = 0）。
⇒ 任一 scenario 漂移（换图 / 换种子 / 挪起点）时，批次变成**不可比**，而两个门都会
  各自 GREEN（各自内部自洽）。这正是 owner 禁的「只写白名单不校验」。

本工具把它变成机判：逐项比 `map` / `seed` / `x` / `y` / `dir`，
任一项不等 ⇒ rc=1 **逐项点名**（带两侧值）；缺件 / 坏 JSON / 缺字段 / 自比 / 传错件 ⇒ rc=2
PREREQ fail-closed（绝不静默 GREEN）。

用法
----
    python -I check_window_batch.py check \
        --rng-scenario  rng-trace-scene/rng_trace_scenario.json \
        --drop-scenario drop-exp/drop_exp_scenario.json
    python -I check_window_batch.py selftest

退出码：0 = GREEN（逐项相等）；1 = RED（不等，逐项点名）；2 = PREREQ（缺件/坏输入/fail-closed）。
"""

import argparse
import json
import os
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_RNG_SCENARIO = os.path.join(HERE, "rng_trace_scenario.json")
DEFAULT_DROP_SCENARIO = os.path.normpath(os.path.join(HERE, "..", "drop-exp", "drop_exp_scenario.json"))

# 两个夹具各自的 canon：传错件（拿同一个 canon / 别的 canon 顶替）必须 fail-closed。
RNG_CANON = "m3-rng-trace-scene-v1"
DROP_CANON = "m3-drop-exp-v1"

# (项目名, rng 侧取值路径, drop 侧取值路径, 归一化类型)：路径用元组，逐级下钻。
COMPARED_ITEMS = [
    ("map", ("map",), ("map",), "str"),
    ("seed", ("seed",), ("seed",), "int"),
    ("x", ("start_pose", "x"), ("attacker", "x"), "int"),
    ("y", ("start_pose", "y"), ("attacker", "y"), "int"),
    ("dir", ("start_pose", "dir"), ("attacker", "dir"), "int"),
]



def _die(rc, msg):
    print(msg)
    return rc


def _load_json(path, label):
    """返回 (obj, err)。err 非空 ⇒ PREREQ fail-closed。"""
    if not os.path.isfile(path):
        return None, "PREREQ_MISSING: {0} 不存在：{1}".format(label, path)
    try:
        with open(path, "r", encoding="utf-8") as fh:
            obj = json.load(fh)
    except Exception as exc:  # noqa: BLE001 - fail-closed 要吞所有读/解析错
        return None, "PREREQ_MISSING: {0} JSON 不可解析（{1}）：{2}".format(label, path, exc)
    if not isinstance(obj, dict):
        return None, "PREREQ_MISSING: {0} 顶层不是 JSON 对象：{1}".format(label, path)
    return obj, ""


def _dig(obj, keys):
    """按元组路径下钻；缺任何一级返 (None, False)。"""
    cur = obj
    for k in keys:
        if not isinstance(cur, dict) or k not in cur:
            return None, False
        cur = cur[k]
    return cur, True


def _norm(v, label, kind="int"):
    """归一化：int 按整数、str 按去空白字符串。不可归一 ⇒ (None, reason)。"""
    if isinstance(v, bool):
        return None, "{0} 是 bool（不合法）".format(label)
    if kind == "str":
        if not isinstance(v, str):
            return None, "{0} 应为字符串（实为 {1}）".format(label, type(v).__name__)
        s = v.strip()
        if s == "":
            return None, "{0} 是空串".format(label)
        return s, ""
    if isinstance(v, int):
        return v, ""
    if isinstance(v, float) and float(v).is_integer():
        return int(v), ""
    if isinstance(v, str) and re.fullmatch(r"-?\d+", v.strip()):
        return int(v.strip()), ""
    return None, "{0} 不是整数（{1}）".format(label, type(v).__name__)


def compare(rng_obj, drop_obj):
    """返回 (rc, lines)。rc: 0 GREEN / 1 RED / 2 PREREQ。"""
    lines = []
    # 先钉「传的是这两个夹具」：canon 逐字节相等（错件 ⇒ PREREQ）。
    rc_canon = rng_obj.get("canon")
    dc_canon = drop_obj.get("canon")
    if rc_canon != RNG_CANON:
        return 2, ["PREREQ_MISSING: --rng-scenario canon 应为 {0}，实为 {1!r}".format(RNG_CANON, rc_canon)]
    if dc_canon != DROP_CANON:
        return 2, ["PREREQ_MISSING: --drop-scenario canon 应为 {0}，实为 {1!r}".format(DROP_CANON, dc_canon)]

    mismatches = []
    ok_items = []
    for name, rpath, dpath, kind in COMPARED_ITEMS:
        rv, rok = _dig(rng_obj, rpath)
        dv, dok = _dig(drop_obj, dpath)
        if not rok:
            return 2, ["PREREQ_MISSING: rng scenario 缺字段 {0}".format(".".join(rpath))]
        if not dok:
            return 2, ["PREREQ_MISSING: drop scenario 缺字段 {0}".format(".".join(dpath))]
        rn, rreason = _norm(rv, "rng.{0}".format(".".join(rpath)), kind)
        if rreason:
            return 2, ["PREREQ_MISSING: {0}".format(rreason)]
        dn, dreason = _norm(dv, "drop.{0}".format(".".join(dpath)), kind)
        if dreason:
            return 2, ["PREREQ_MISSING: {0}".format(dreason)]
        if rn != dn:
            mismatches.append("MISMATCH {0}: rng={1!r} drop={2!r}".format(name, rn, dn))
        else:
            ok_items.append("{0}={1!r}".format(name, rn))

    if mismatches:
        lines.append("VERDICT RED: WINDOW_BATCH_MISMATCH（两个场景件不是同图/同种子/同起点 ⇒ 批次不可比，拒跑）")
        lines.extend(mismatches)
        return 1, lines

    lines.append("VERDICT GREEN: 同窗口批次前提 OK（map/seed/起点 逐项相等）")
    lines.append("OK " + " ".join(ok_items))
    return 0, lines


def cmd_check(rng_path, drop_path):
    if os.path.realpath(rng_path) == os.path.realpath(drop_path):
        return _die(2, "PREREQ_MISSING: --rng-scenario 与 --drop-scenario 指向同一文件（自比恒真，拒跑）：{0}".format(rng_path))
    rng_obj, err = _load_json(rng_path, "--rng-scenario")
    if err:
        return _die(2, err)
    drop_obj, err = _load_json(drop_path, "--drop-scenario")
    if err:
        return _die(2, err)
    rc, lines = compare(rng_obj, drop_obj)
    for ln in lines:
        print(ln)
    return rc


# --------------------------------------------------------------------------- #
# selftest：改坏必红（换图 / 换种子 / 挪 x/y/dir）+ PREREQ fail-closed
# --------------------------------------------------------------------------- #
def _write_json(path, obj):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(obj, fh, ensure_ascii=False, indent=2)
        fh.write("\n")


def _case(name, rc, out, want_rc, needle):
    """want_rc 为期望退出码；needle 为必须点名的子串（None 表示不限）。"""
    ok = (rc == want_rc) and ((needle is None) or (needle in out))
    detail = "rc={0} want={1}".format(rc, want_rc)
    if needle is not None:
        detail += " needle={0!r}".format(needle)
    if ok:
        print("SELFTEST ok {0} ({1})".format(name, detail))
    else:
        print("SELFTEST FAIL {0} ({1})".format(name, detail))
    return ok


def cmd_selftest():
    real_rng = DEFAULT_RNG_SCENARIO
    real_drop = DEFAULT_DROP_SCENARIO
    rng_obj, err = _load_json(real_rng, "rng")
    if err:
        return _die(2, err)
    drop_obj, err = _load_json(real_drop, "drop")
    if err:
        return _die(2, err)

    tmp = tempfile.mkdtemp(prefix="window-batch-selftest-")
    passed = 0
    total = 0

    def run(pa, pb):
        import io
        from contextlib import redirect_stdout
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = cmd_check(pa, pb)
        return rc, buf.getvalue()

    def clone(obj):
        return json.loads(json.dumps(obj))

    def scratch(name, obj):
        p = os.path.join(tmp, name)
        _write_json(p, obj)
        return p

    a_real = scratch("rng-real.json", rng_obj)
    b_real = scratch("drop-real.json", drop_obj)
    missing = os.path.join(tmp, "does-not-exist.json")

    # ① 真件 ⇒ GREEN
    total += 1
    passed += _case("real-pair-green", *run(a_real, b_real), want_rc=0, needle="VERDICT GREEN")
    # ② map 漂移 ⇒ 必红且点名 map
    d = clone(drop_obj); d["map"] = "0100"
    total += 1
    passed += _case("map-drift-red", *run(a_real, scratch("d-map.json", d)), want_rc=1, needle="MISMATCH map")
    # ③ seed 漂移 ⇒ 必红且点名 seed
    d = clone(drop_obj); d["seed"] = 43
    total += 1
    passed += _case("seed-drift-red", *run(a_real, scratch("d-seed.json", d)), want_rc=1, needle="MISMATCH seed")
    # ④ x 漂移 ⇒ 必红
    d = clone(drop_obj); d["attacker"]["x"] = 18
    total += 1
    passed += _case("x-drift-red", *run(a_real, scratch("d-x.json", d)), want_rc=1, needle="MISMATCH x")
    # ⑤ y 漂移 ⇒ 必红
    d = clone(drop_obj); d["attacker"]["y"] = 21
    total += 1
    passed += _case("y-drift-red", *run(a_real, scratch("d-y.json", d)), want_rc=1, needle="MISMATCH y")
    # ⑥ dir 漂移 ⇒ 必红
    d = clone(drop_obj); d["attacker"]["dir"] = 0
    total += 1
    passed += _case("dir-drift-red", *run(a_real, scratch("d-dir.json", d)), want_rc=1, needle="MISMATCH dir")

    # ⑦ 缺 rng 件 ⇒ PREREQ
    total += 1
    passed += _case("missing-rng-prereq", *run(missing, b_real), want_rc=2, needle="PREREQ_MISSING")
    # ⑧ 缺 drop 件 ⇒ PREREQ
    total += 1
    passed += _case("missing-drop-prereq", *run(a_real, missing), want_rc=2, needle="PREREQ_MISSING")
    # ⑨ 坏 JSON ⇒ PREREQ
    bad = os.path.join(tmp, "bad.json")
    with open(bad, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("{ not json")
    total += 1
    passed += _case("bad-json-prereq", *run(a_real, bad), want_rc=2, needle="不可解析")
    # ⑩ drop 缺起点字段 ⇒ PREREQ
    d = clone(drop_obj); del d["attacker"]["dir"]
    total += 1
    passed += _case("missing-pose-field-prereq", *run(a_real, scratch("d-nodir.json", d)), want_rc=2, needle="缺字段 attacker.dir")
    # ⑪ 自比（同一文件）⇒ PREREQ
    total += 1
    passed += _case("self-compare-prereq", *run(a_real, a_real), want_rc=2, needle="同一文件")
    # ⑫ 传错件（canon 不对）⇒ PREREQ
    a = clone(rng_obj); a["canon"] = "someone-else"
    total += 1
    passed += _case("canon-mismatch-prereq", *run(scratch("a-canon.json", a), b_real), want_rc=2, needle="canon")
    # ⑬ map 类型漂移（字符串 → 整数）⇒ PREREQ（"0137" 与 137 不得被当同一个图）
    d = clone(drop_obj); d["map"] = 137
    total += 1
    passed += _case("map-type-prereq", *run(a_real, scratch("d-mapint.json", d)), want_rc=2, needle="应为字符串")

    import shutil
    shutil.rmtree(tmp, ignore_errors=True)

    if passed == total:
        print("SELFTEST GREEN: check_window_batch selftest {0}/{0}（真件绿 + 换图/换种子/挪 x/y/dir 必红且点名 + 缺件/坏 JSON/缺字段/自比/传错件 PREREQ fail-closed）".format(total))
    else:
        print("SELFTEST RED: check_window_batch selftest {0}/{1}".format(passed, total))
    return 0 if passed == total else 1


def main(argv):
    ap = argparse.ArgumentParser(description="同窗口批次前提机判（本卡 scenario vs ④ drop-exp scenario）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check", help="逐项比 map/seed/起点")
    c.add_argument("--rng-scenario", default=DEFAULT_RNG_SCENARIO)
    c.add_argument("--drop-scenario", default=DEFAULT_DROP_SCENARIO)
    sub.add_parser("selftest", help="改坏必红 + PREREQ 自检")
    args = ap.parse_args(argv)
    if args.cmd == "check":
        return cmd_check(args.rng_scenario, args.drop_scenario)
    if args.cmd == "selftest":
        return cmd_selftest()
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
