#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""quest_state_producer.py —— M3⑥ 任务态 `quest_state.jsonl` **生产者**（构造级，oracle 锚定）。

卡：`openmir2-m3-quest-align`（判据加固 §2-3 ③）；承接卡 `openmir2-m3-quest-state-dump`；
本生产者登记 **whitelist T-58**。

背景：比较器 `quest_diff.py` 的 `assemble --state` / `diff` 需要两侧各一份**服务端会话任务态**
dump（`quest_state.jsonl`：每行 `{"k":N,"quest_flag_before":<hex128>,"quest_flag_after":<hex128>}`），
但此前全仓**无生产者**（`WRITER_FOUND=0`）⇒ 真机 quest 窗口必 PREREQ 空转。

本文件是**构造级**生产者：把场景（`quest_scenario.json` 某 case）里**逐步声明**的任务位操作
（`flag_ops`；**真源 = 冻结脚本 `E:/MirServer/M2GameSvr/Envir/Market_Def/任务NPC/任务-3.txt`
的 `set [n] v`**，逐字对齐）按 oracle 位语义（`nFlag-=1; idx=nFlag/8; bit=128>>(nFlag%8)`，
`src/M2Server/Player/PlayObject.cs:3966-4110`）作用到 128 字节位图，逐 k 落 `before/after`。

## oracle 锚定（关键：位语义不是本文件发明的）
`selftest` 把 oracle 真身跑出来的 **`quest-state-probe/vectors/quest_state_vectors.tsv`**
（`array==quest_flag` 的 set/read 序列）逐行**重放到本文件的 `apply_set`**，要求每一行 `read`
的 `get` 与 `bytes_hex` **逐字节复现** oracle 的输出。位语义一旦漂移 ⇒ RED。这样本文件里的
`idx=nFlag//8; bit=128>>(nFlag%8)` 是**被 oracle 钉住的**，不是重推的。

## 降格声明（**不得当服务端真值**）
每行带 `"construct_level": true`。本生产者只证明「脚本声明 → 位图」口径正确，**不**替代服务端
会话真值；`quest_diff.py` 默认把 `construct_level:true` 的行判 **PREREQ（拒比，fail-closed）**，
只有显式 `--allow-construct-state`（离线自检用）才接受。真机判据的服务端 dump 由承接卡
`openmir2-m3-quest-state-dump` 落（C# 影子 hook 半 + Rust `bins/game-svr` 半）——本线写集**不含**
服务端半（见 whitelist T-58 未做项）。

退出码：0 = GREEN；1 = RED；2 = PREREQ（缺件/坏件，fail-closed）。
"""

import argparse
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_SCENARIO = os.path.join(HERE, "quest_scenario.json")
DEFAULT_VECTORS = os.path.join(HERE, "quest-state-probe", "vectors", "quest_state_vectors.tsv")
QUEST_BYTES = 128
HEX_LEN = QUEST_BYTES * 2


class Prereq(Exception):
    """缺件/坏件（fail-closed，rc=2）。"""


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def _zero_bitmap():
    return bytearray(QUEST_BYTES)


def apply_set(bitmap, flag, value):
    """oracle 位语义（`PlayObject.cs:3966-4110`，`nFlag-=1; idx=nFlag/8; bit=128>>(nFlag%8)`）。
    越界（idx<0 / idx>=128）⇒ no-op（oracle 同款）。`value!=0` ⇒ 置位；否则清位。"""
    n = flag - 1
    idx = n // 8 if n >= 0 else -1
    if idx < 0 or idx >= QUEST_BYTES:
        return
    bit = 128 >> (n % 8)
    if value != 0:
        bitmap[idx] |= bit
    else:
        bitmap[idx] &= (0xFF ^ bit) & 0xFF


def get_flag(bitmap, flag):
    n = flag - 1
    idx = n // 8 if n >= 0 else -1
    if idx < 0 or idx >= QUEST_BYTES:
        return 0
    bit = 128 >> (n % 8)
    return 1 if (bitmap[idx] & bit) else 0


def apply_ops(bitmap, ops):
    """把一步的 op 列表（`[{"flag":n,"value":v}...]`）依次作用到 bitmap（原地）。"""
    if ops is None:
        return
    for op in ops:
        if not isinstance(op, dict) or not _is_int(op.get("flag")) or not _is_int(op.get("value")):
            raise Prereq("flag_op 非法（需 {flag:int,value:int}）: %r" % (op,))
        apply_set(bitmap, op["flag"], op["value"])


def hex128(bitmap):
    if len(bitmap) != QUEST_BYTES:
        raise Prereq("位图长度 != %d" % QUEST_BYTES)
    return "".join("%02x" % b for b in bitmap)


def load_case(scenario_path, case):
    if not os.path.isfile(scenario_path):
        raise Prereq("缺场景: %s" % scenario_path)
    with open(scenario_path, "r", encoding="utf-8") as fh:
        sc = json.load(fh)
    cases = sc.get("cases") or {}
    if case not in cases:
        raise Prereq("场景内无 case=%s" % case)
    c = cases[case]
    steps = c.get("steps")
    if not isinstance(steps, list):
        raise Prereq("case=%s 缺 steps" % case)
    ops = c.get("flag_ops")
    if ops is None:
        ops = [[] for _ in steps]
    if not isinstance(ops, list) or len(ops) != len(steps):
        raise Prereq("case=%s flag_ops 长度(%s) != steps 长度(%d)"
                     % (case, len(ops) if isinstance(ops, list) else "?", len(steps)))
    return steps, ops


def produce(steps, ops):
    """逐 k 造记录：before = 该步前的位图，after = 应用该步 ops 后的位图。返回 (records, bitmap)。"""
    bitmap = _zero_bitmap()
    recs = []
    for k in range(len(steps)):
        before = hex128(bitmap)
        apply_ops(bitmap, ops[k])
        recs.append({
            "k": k,
            "quest_flag_before": before,
            "quest_flag_after": hex128(bitmap),
            "construct_level": True,
        })
    return recs, bitmap


def cmd_emit(args):
    try:
        steps, ops = load_case(args.scenario, args.case)
        recs, _ = produce(steps, ops)
    except Prereq as exc:
        sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
        return 2
    lines = [json.dumps(r, ensure_ascii=False, sort_keys=True) for r in recs]
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
        sys.stdout.write("PRODUCED k=%d out=%s case=%s construct_level=true\n"
                         % (len(recs), args.out, args.case))
    else:
        sys.stdout.write("\n".join(lines) + "\n")
    return 0


# ---- oracle 锚：重放 quest-state-probe 的 oracle 真身向量 --------------------


def _load_flag_vectors(path):
    """读 `quest_state_vectors.tsv` 的 `array==quest_flag` 行 -> [(op,flag,value,get,bytes_hex)]。"""
    if not os.path.isfile(path):
        raise Prereq("缺 oracle 向量: %s" % path)
    rows = []
    with open(path, "r", encoding="utf-8") as fh:
        for raw in fh:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) != 6:
                raise Prereq("向量行列数异常: %r" % line)
            arr, op, flag, value, get, bhex = parts
            if arr != "quest_flag":
                continue
            if not (_is_int(int(flag)) and _is_int(int(value)) and _is_int(int(get))):
                raise Prereq("向量行列非整数: %r" % line)
            if len(bhex) != HEX_LEN:
                raise Prereq("向量 hex 长度异常(%d): %r" % (len(bhex), line))
            rows.append((op, int(flag), int(value), int(get), bhex))
    if not rows:
        raise Prereq("向量无 quest_flag 行: %s" % path)
    return rows


def replay_vectors(rows):
    """把 oracle 向量序列重放：每一步后断言 `get`/`bytes` 与 oracle 逐字节一致。"""
    bitmap = _zero_bitmap()
    diffs = []
    for i, (op, flag, value, want_get, want_hex) in enumerate(rows):
        if op == "set":
            apply_set(bitmap, flag, value)
        got = get_flag(bitmap, flag)
        if got != want_get:
            diffs.append((i, op, flag, "get", want_get, got))
        if hex128(bitmap) != want_hex:
            diffs.append((i, op, flag, "bytes", want_hex, hex128(bitmap)))
            break
    return diffs


def _prereq_rc(fn):
    try:
        fn()
        return "no-prereq"
    except Prereq:
        return 2


def cmd_selftest(args):
    tmp = tempfile.mkdtemp(prefix="quest-state-producer-selftest-")
    results = []

    def case(name, expected_rc, fn):
        try:
            got = fn()
        except Exception as exc:  # noqa: BLE001
            got = "exception:%s" % exc
        results.append((name, expected_rc, got, got == expected_rc))

    def _selftest_scenario():
        sc_path = os.path.join(tmp, "scenario.json")
        sc = {
            "cases": {
                "q_ok": {"steps": [{"op": "a"}, {"op": "b"}, {"op": "c"}, {"op": "d"}],
                         "flag_ops": [[], [], [{"flag": 107, "value": 1}], []]},
                "q_bad_len": {"steps": [{"op": "a"}], "flag_ops": []},
            }
        }
        with open(sc_path, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(sc, fh, ensure_ascii=False)
        return sc_path

    try:
        sc_path = _selftest_scenario()

        # 绿：正常 case 产出 4 行，k 连续；step2(set[107]1) 之后 byte13 = 0x20。
        def _emit_ok():
            steps, ops = load_case(sc_path, "q_ok")
            recs, _ = produce(steps, ops)
            if [r["k"] for r in recs] != [0, 1, 2, 3]:
                return "kbad"
            if recs[1]["quest_flag_before"] != recs[0]["quest_flag_after"]:
                return "before-after-mismatch"
            if recs[3]["quest_flag_after"][26:28] != "20":  # byte13 hi nibble
                return "byte13=%s" % recs[3]["quest_flag_after"][26:28]
            # step2 是 set[107]1 ⇒ 该步 before != after；step3 无 op ⇒ before == 上一步 after。
            if recs[2]["quest_flag_before"] == recs[2]["quest_flag_after"]:
                return "step2-not-changed"
            if recs[3]["quest_flag_before"] != recs[2]["quest_flag_after"]:
                return "step3-before-mismatch"
            return 0
        case("emit-ok", 0, _emit_ok)

        # 改坏必红：flag_ops 长度 != steps ⇒ PREREQ
        case("flag-ops-len-prereq", 2, lambda: _prereq_rc(lambda: load_case(sc_path, "q_bad_len")))

        # 缺 case / 缺场景 ⇒ PREREQ
        case("missing-case-prereq", 2, lambda: _prereq_rc(lambda: load_case(sc_path, "nope")))
        case("missing-scenario-prereq", 2,
             lambda: _prereq_rc(lambda: load_case(os.path.join(tmp, "nope.json"), "q_ok")))

        # oracle 锚：重放真身向量必须逐字节一致
        def _anchor():
            rows = _load_flag_vectors(args.vectors)
            diffs = replay_vectors(rows)
            return 0 if not diffs else "diff:%r" % (diffs[:2],)
        case("oracle-vector-anchor", 0, _anchor)

        # 改坏必红：把位语义从 `128>>(n%8)` 改成 `1<<(n%8)` ⇒ oracle 锚必红
        def _mutate_semantics():
            rows = _load_flag_vectors(args.vectors)
            bitmap = _zero_bitmap()
            for (op, flag, value, want_get, want_hex) in rows:
                if op != "set":
                    continue
                n = flag - 1
                idx = n // 8 if n >= 0 else -1
                if idx < 0 or idx >= QUEST_BYTES:
                    continue
                if value != 0:
                    bitmap[idx] |= (1 << (n % 8))
                else:
                    bitmap[idx] &= 0xFF ^ (1 << (n % 8))
            return "mutated-differs" if hex128(bitmap) != rows[-1][4] else "unexpected-equal"
        case("mutate-bit-semantics-red", "mutated-differs", _mutate_semantics)

        # 改坏必红：`--out` 写出后重读，改 op 值必得不同 hex（生产者非恒等）
        def _emit_differs():
            steps, ops = load_case(sc_path, "q_ok")
            recs0, _ = produce(steps, ops)
            ops2 = [[], [], [{"flag": 107, "value": 0}], []]
            recs1, _ = produce(steps, ops2)
            return 0 if recs0[-1]["quest_flag_after"] != recs1[-1]["quest_flag_after"] else "identical"
        case("op-value-changes-state", 0, _emit_differs)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    failed = [(n, e, g) for (n, e, g, ok) in results if not ok]
    for name, expected, got, ok in results:
        sys.stdout.write("SELFTEST %s %s expect=%r got=%r\n"
                         % ("ok" if ok else "FAIL", name, expected, got))
    sys.stdout.write("SELFTEST ok=%d/%d\n" % (len(results) - len(failed), len(results)))
    return 1 if failed else 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3⑥ 任务态 quest_state.jsonl 生产者（构造级）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("emit")
    e.add_argument("--scenario", default=DEFAULT_SCENARIO)
    e.add_argument("--case", default="quest_accept")
    e.add_argument("--out", default="")
    e.set_defaults(func=cmd_emit)
    s = sub.add_parser("selftest")
    s.add_argument("--vectors", default=DEFAULT_VECTORS)
    s.set_defaults(func=cmd_selftest)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
