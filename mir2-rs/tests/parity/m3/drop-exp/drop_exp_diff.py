#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""drop_exp_diff.py —— M3④ 掉落/经验跨侧对拍比较器（夹具骨架）。

卡：`openmir2-m3-drop-exp-align`（M3 逐系统对齐 ④ 掉落/经验）。本文件是**判据本体**，
只做离线比较：读两侧各自的 `drop_exp_raw.jsonl`（一次击杀一行），逐 k 比较
「掉落列表 + 金币 + 经验增量」。真机跑由 `run_drop_exp_pair.ps1`（等 owner 发窗口）驱动。

口径（与卡内 `machine_checkable_acceptance` ②③ 对齐）：

1. **fail-closed**：缺文件 / 缺字段 / 非法 JSON / k 不连续 / 记录非对象 ⇒ 一律非零退出并点名
   （`PREREQ_RED`），**绝不静默 GREEN**。
2. **多重集/计数式**：掉落列表按「每个不同掉落项的个数」比（`Counter`），**不做 set 差集**；
   同时逐槽位（slot）比，用于把差异定位到「第 k 次击杀 / 第几个掉落槽」。
3. **不得 i32 截断**：金额/经验一律按 Python 任意精度整数**原宽度**比较；`selftest` 里有一条
   「只在高 32 位不同的两个值必须判红」的用例钉住。
4. **改坏必红**：`selftest` 对「掉落项名/掉落项数/金币/经验/掉落条数/记录数」逐个改坏 ⇒ 必红。

记录 schema（每行一个 JSON 对象；`drop_exp_scenario.json` 里也写一份）：

    {
      "k": 0,                       # 击杀序号（0 起，必须连续）
      "monster": "稻草人",           # 可选：被击杀怪物名
      "x": 17, "y": 20,             # 可选：击杀时怪物坐标
      "gold": 37,                   # 本次掉落金币（原宽度 int）
      "drops": [                    # 本次落地物品（有序；比较用多重集 + 逐槽位）
        {"name": "金币", "count": 37},
        {"name": "木剑", "index": 2, "count": 1, "dura": 13, "dura_max": 20}
      ],
      "exp_delta": 120,             # 本次击杀归属者获得的经验增量
      "exp_total": 120,             # 击杀后归属者 Abil.Exp
      "level": 1,                   # 击杀后归属者 Abil.Level
      "hero_exp_delta": 0           # 可选：英雄/下属路径经验增量（oracle 无独立英雄流，见 README）
    }

退出码：0 = GREEN（逐项相同）；1 = RED（有差异）；2 = PREREQ（缺件/坏件，fail-closed）。

采集面（T-57，判据加固 §2-2；BotSrv 侧扩采）：

  BotSrv 真机 `drops` 的**可采集上限** = `SM_ITEMSHOW(610)` 帧携带的字段 + 槽位序。该帧
  （oracle `BaseObject.cs:570`/`:752` → `PlayObject.Message.cs:1744-1745` RM→SM 直转）
  **只带** `Looks/ItemId/dx/dy/Name`，**不带** `MakeIndex/Dura/DuraMax`（地面物耐久客户端不可见）。

  * 扩采**关**（默认，`MIR2_DROPEXP_EXT_FIELDS` 未设/0/false/off/no）⇒ 每件 `{"name":..,"count":1}`
    （逐字节与既有输出相同）。此口径下「掉落逐项相同」= **名称多重集 + 槽位顺序**（**显式降格声明**，
    不再是隐式）。
  * 扩采**开** ⇒ 每件五字段 `{"name":..,"index":<槽位序>,"count":1,"dura":null,"dura_max":null}`
    （`index` = 槽位序；`dura/dura_max` = `null`，如实「未采集」，**绝不伪造成值**）。

  本比较器把 `null` 当「该侧未采集」：两侧都 `null` ⇒ 相等；一侧 `null`、一侧有真值 ⇒ **RED**
  （不得把「未采集」当「相等」）。`drop_key` = `(name,index,count,dura,dura_max)` 五字段全进多重集/逐槽位比。
"""

import argparse
import collections
import json
import os
import shutil
import subprocess
import sys
import tempfile

# ---- schema ----------------------------------------------------------------

REQUIRED_KILL_FIELDS = ("k", "gold", "drops", "exp_delta", "exp_total", "level")
OPTIONAL_KILL_FIELDS = ("monster", "x", "y", "hero_exp_delta")
SCALAR_FIELDS = ("gold", "exp_delta", "exp_total", "level", "hero_exp_delta")
DROP_KEY_FIELDS = ("name", "index", "count", "dura", "dura_max")


class Prereq(Exception):
    """缺件/坏件（fail-closed，rc=2）。"""


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def load_raw(path):
    """读 JSONL。缺文件 / 非法 JSON / 非对象 / 无记录 ⇒ Prereq。"""
    if not os.path.isfile(path):
        raise Prereq("缺文件: %s" % path)
    out = []
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.strip()
            if line == "":
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError as exc:
                raise Prereq("非法 JSON %s:%d: %s" % (path, lineno, exc))
            if not isinstance(rec, dict):
                raise Prereq("非法 JSON 记录（应为对象）%s:%d" % (path, lineno))
            out.append(rec)
    if not out:
        raise Prereq("无记录: %s" % path)
    return out


def validate(raw, path, side):
    """逐条校验 schema；任何缺失/类型错误 ⇒ Prereq（点名 side/k/字段）。"""
    ks = []
    for rec in raw:
        if "k" not in rec:
            raise Prereq("%s: 记录缺字段 k (%s)" % (side, path))
        ks.append(rec["k"])
    for i, k in enumerate(ks):
        if not _is_int(k):
            raise Prereq("%s: k 非整数 (%s)" % (side, path))
        if k != i:
            raise Prereq("%s: k 不连续/非 0 起（第 %d 条 k=%r）(%s)" % (side, i, k, path))
    for rec in raw:
        k = rec["k"]
        for field in REQUIRED_KILL_FIELDS:
            if field not in rec:
                raise Prereq("%s: k=%d 缺字段 %s (%s)" % (side, k, field, path))
        for field in ("gold", "exp_delta", "exp_total", "level"):
            if not _is_int(rec[field]):
                raise Prereq("%s: k=%d 字段 %s 非整数 (%s)" % (side, k, field, path))
        if "hero_exp_delta" in rec and not _is_int(rec["hero_exp_delta"]):
            raise Prereq("%s: k=%d 字段 hero_exp_delta 非整数 (%s)" % (side, k, path))
        drops = rec["drops"]
        if not isinstance(drops, list):
            raise Prereq("%s: k=%d drops 非数组 (%s)" % (side, k, path))
        for slot, drop in enumerate(drops):
            if not isinstance(drop, dict):
                raise Prereq("%s: k=%d slot=%d 掉落项非对象 (%s)" % (side, k, slot, path))
            if "name" not in drop:
                raise Prereq("%s: k=%d slot=%d 掉落项缺 name (%s)" % (side, k, slot, path))
            if "count" not in drop:
                raise Prereq("%s: k=%d slot=%d 掉落项缺 count (%s)" % (side, k, slot, path))
            if not _is_int(drop["count"]):
                raise Prereq("%s: k=%d slot=%d count 非整数 (%s)" % (side, k, slot, path))


# ---- comparison ------------------------------------------------------------


def drop_key(drop):
    return tuple(drop.get(f) for f in DROP_KEY_FIELDS)


def _scalar_diffs(a, b, side_keys):
    """逐标量字段比；hero_exp_delta 缺省视作 0（两侧都缺 ⇒ 相等）。"""
    out = []
    for field in SCALAR_FIELDS:
        if field == "hero_exp_delta":
            if field not in a and field not in b:
                continue
        av = a.get(field, 0)
        bv = b.get(field, 0)
        if av != bv:
            out.append((field, av, bv))
    return out


def compare(cs, rs):
    """返回 (diffs, stats)。diffs 为空 ⇒ GREEN。"""
    diffs = []
    stats = {"kills": len(cs), "drop_slots": 0, "gold_total": 0, "exp_total": 0}
    if len(cs) != len(rs):
        diffs.append(
            (None, None, "记录条数不同", "csharp=%d" % len(cs), "rust=%d" % len(rs))
        )
        return diffs, stats
    for k in range(len(cs)):
        a, b = cs[k], rs[k]
        for field, av, bv in _scalar_diffs(a, b, None):
            diffs.append((k, None, "字段 %s" % field, av, bv))
        da = a.get("drops") or []
        db = b.get("drops") or []
        stats["drop_slots"] += len(da)
        stats["gold_total"] += a.get("gold", 0)
        stats["exp_total"] += a.get("exp_delta", 0)
        # 逐槽位（用于定位「第几个掉落槽」）
        n = min(len(da), len(db))
        for slot in range(n):
            if drop_key(da[slot]) != drop_key(db[slot]):
                diffs.append(
                    (k, slot, "掉落槽", drop_key(da[slot]), drop_key(db[slot]))
                )
        if len(da) != len(db):
            diffs.append((k, None, "掉落条数", len(da), len(db)))
        # 多重集/计数式（不许 set 差集）
        ca = collections.Counter(drop_key(d) for d in da)
        cb = collections.Counter(drop_key(d) for d in db)
        if ca != cb:
            missing = dict((str(x), v) for x, v in (ca - cb).items())
            extra = dict((str(x), v) for x, v in (cb - ca).items())
            if missing or extra:
                diffs.append((k, None, "掉落多重集", missing, extra))
    return diffs, stats


def render(diffs, stats, cs_path, rs_path):
    lines = []
    for k, slot, what, av, bv in diffs:
        loc = "k=%s" % ("-" if k is None else k)
        slot_s = "" if slot is None else " slot=%d" % slot
        lines.append("KILL %s%s RED %s csharp=%r rust=%r" % (loc, slot_s, what, av, bv))
    lines.append(
        "STATS kills=%d drop_slots=%d gold_total=%d exp_delta_total=%d"
        % (stats["kills"], stats["drop_slots"], stats["gold_total"], stats["exp_total"])
    )
    lines.append("CSHARP=%s" % cs_path)
    lines.append("RUST=%s" % rs_path)
    if diffs:
        lines.append("DIFF n=%d" % len(diffs))
    else:
        lines.append("DIFF 0")
    return "\n".join(lines)


def cmd_diff(args):
    try:
        cs = load_raw(args.csharp)
        rs = load_raw(args.rust)
        validate(cs, args.csharp, "csharp")
        validate(rs, args.rust, "rust")
    except Prereq as exc:
        sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
        return 2
    diffs, stats = compare(cs, rs)
    sys.stdout.write(render(diffs, stats, args.csharp, args.rust) + "\n")
    if diffs:
        sys.stdout.write("RED: 掉落/经验跨侧差异 %d 项（定位见上）\n" % len(diffs))
        return 1
    sys.stdout.write(
        "GREEN: 掉落列表逐项相同 + 经验逐级相同（kills=%d）\n" % stats["kills"]
    )
    return 0


# ---- selftest（fail-closed + 改坏必红） -------------------------------------


def _write_jsonl(path, records):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        for rec in records:
            fh.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")


def _base_records():
    return [
        {
            "k": 0,
            "monster": "稻草人",
            "x": 17,
            "y": 20,
            "gold": 37,
            "drops": [
                {"name": "金币", "count": 37},
                {"name": "木剑", "index": 2, "count": 1, "dura": 13, "dura_max": 20},
            ],
            "exp_delta": 12,
            "exp_total": 12,
            "level": 1,
        },
        {
            "k": 1,
            "monster": "稻草人",
            "x": 17,
            "y": 20,
            "gold": 0,
            "drops": [],
            "exp_delta": 12,
            "exp_total": 24,
            "level": 1,
        },
    ]


def _run_diff(cs_path, rs_path):
    args = argparse.Namespace(csharp=cs_path, rust=rs_path)
    return cmd_diff(args)


def cmd_selftest(args):
    tmp = tempfile.mkdtemp(prefix="drop-exp-selftest-")
    results = []

    def case(name, expected_rc, fn):
        try:
            got = fn()
        except Exception as exc:  # noqa: BLE001 —— 自检要如实报红，不吞
            got = "exception:%s" % exc
        ok = got == expected_rc
        results.append((name, expected_rc, got, ok))

    try:
        # 绿：两侧逐字节/逐项相同
        cs0 = os.path.join(tmp, "cs0.jsonl")
        rs0 = os.path.join(tmp, "rs0.jsonl")
        _write_jsonl(cs0, _base_records())
        _write_jsonl(rs0, _base_records())
        case("identical-green", 0, lambda: _run_diff(cs0, rs0))

        # 红：改一个掉落项名
        rs_name = os.path.join(tmp, "rs_name.jsonl")
        rec = _base_records()
        rec[0]["drops"][1]["name"] = "铁剑"
        _write_jsonl(rs_name, rec)
        case("mutate-drop-name-red", 1, lambda: _run_diff(cs0, rs_name))

        # 红：改金币（掉落数量）
        rs_gold = os.path.join(tmp, "rs_gold.jsonl")
        rec = _base_records()
        rec[0]["gold"] = 38
        rec[0]["drops"][0]["count"] = 38
        _write_jsonl(rs_gold, rec)
        case("mutate-gold-red", 1, lambda: _run_diff(cs0, rs_gold))

        # 红：改经验增量
        rs_exp = os.path.join(tmp, "rs_exp.jsonl")
        rec = _base_records()
        rec[1]["exp_delta"] = 13
        _write_jsonl(rs_exp, rec)
        case("mutate-exp-red", 1, lambda: _run_diff(cs0, rs_exp))

        # 红：改掉落条数（多重集/计数式必须能咬住）
        rs_cnt = os.path.join(tmp, "rs_cnt.jsonl")
        rec = _base_records()
        rec[0]["drops"] = rec[0]["drops"] + [{"name": "布衣", "index": 5, "count": 1}]
        _write_jsonl(rs_cnt, rec)
        case("mutate-drop-count-red", 1, lambda: _run_diff(cs0, rs_cnt))

        # 红：记录条数不同
        rs_len = os.path.join(tmp, "rs_len.jsonl")
        _write_jsonl(rs_len, _base_records()[:1])
        case("mutate-kill-count-red", 1, lambda: _run_diff(cs0, rs_len))

        # 红：不得 i32 截断 —— 只在高 32 位不同的两个值必须判红
        cs_big = os.path.join(tmp, "cs_big.jsonl")
        rs_big = os.path.join(tmp, "rs_big.jsonl")
        rec = _base_records()
        rec[0]["gold"] = 3000000000  # < 2^31
        _write_jsonl(cs_big, rec)
        rec2 = _base_records()
        rec2[0]["gold"] = 3000000000 + (1 << 32)  # 低 32 位与上面完全相同
        _write_jsonl(rs_big, rec2)
        case("i32-truncation-red", 1, lambda: _run_diff(cs_big, rs_big))

        # 绿：两侧同一个大值（原宽度相同）⇒ 不是「见到大数就红」
        cs_big2 = os.path.join(tmp, "cs_big2.jsonl")
        rs_big2 = os.path.join(tmp, "rs_big2.jsonl")
        big = 3000000000 + (1 << 32)
        rec = _base_records()
        rec[0]["gold"] = big
        _write_jsonl(cs_big2, rec)
        rec3 = _base_records()
        rec3[0]["gold"] = big
        _write_jsonl(rs_big2, rec3)
        case("big-value-equal-green", 0, lambda: _run_diff(cs_big2, rs_big2))

        # PREREQ（fail-closed）：缺文件
        case(
            "missing-file-prereq",
            2,
            lambda: _run_diff(os.path.join(tmp, "nope.jsonl"), rs0),
        )

        # PREREQ：缺字段
        rs_missing = os.path.join(tmp, "rs_missing.jsonl")
        rec = _base_records()
        del rec[0]["exp_delta"]
        _write_jsonl(rs_missing, rec)
        case("missing-field-prereq", 2, lambda: _run_diff(cs0, rs_missing))

        # PREREQ：非法 JSON
        rs_bad = os.path.join(tmp, "rs_bad.jsonl")
        with open(rs_bad, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("{not json\n")
        case("invalid-json-prereq", 2, lambda: _run_diff(cs0, rs_bad))

        # T-57 扩采（五字段 + 槽位序 index）：两侧相同（含 dura/dura_max=null）⇒ 绿
        def _ex_records(dura=None, idx=(0, 1)):
            r = _base_records()
            r[0]["drops"] = [
                {"name": "金币", "index": idx[0], "count": 1, "dura": None, "dura_max": None},
                {"name": "木剑", "index": idx[1], "count": 1, "dura": dura, "dura_max": None},
            ]
            return r
        cs_ex = os.path.join(tmp, "cs_ex.jsonl")
        rs_ex = os.path.join(tmp, "rs_ex.jsonl")
        _write_jsonl(cs_ex, _ex_records())
        _write_jsonl(rs_ex, _ex_records())
        case("ext-fields-null-green", 0, lambda: _run_diff(cs_ex, rs_ex))

        # 扩采：一侧 dura=null（未采集）对一侧 dura=13（真值）⇒ 必红（不得把「未采集」当「相等」）
        rs_exv = os.path.join(tmp, "rs_exv.jsonl")
        _write_jsonl(rs_exv, _ex_records(dura=13))
        case("ext-null-vs-value-red", 1, lambda: _run_diff(cs_ex, rs_exv))

        # 扩采：槽位序 index 不同 ⇒ 必红（index 进 drop_key）
        rs_exi = os.path.join(tmp, "rs_exi.jsonl")
        _write_jsonl(rs_exi, _ex_records(idx=(9, 1)))
        case("ext-index-mismatch-red", 1, lambda: _run_diff(cs_ex, rs_exi))

        # 扩采 vs legacy（name/count 同、index 缺席）⇒ 必红（字段面不同不可当同）
        rs_leg = os.path.join(tmp, "rs_leg.jsonl")
        rl = _base_records()
        rl[0]["drops"] = [{"name": "金币", "count": 1}, {"name": "木剑", "count": 1}]
        _write_jsonl(rs_leg, rl)
        case("ext-vs-legacy-red", 1, lambda: _run_diff(cs_ex, rs_leg))

        # PREREQ：k 不连续
        rs_gap = os.path.join(tmp, "rs_gap.jsonl")
        rec = _base_records()
        rec[1]["k"] = 5
        _write_jsonl(rs_gap, rec)
        case("k-gap-prereq", 2, lambda: _run_diff(cs0, rs_gap))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    order = ["GREEN", "RED", "PREREQ"]
    failed = [(n, e, g) for (n, e, g, ok) in results if not ok]
    for name, expected, got, ok in results:
        sys.stdout.write(
            "SELFTEST %s %s expect=%s got=%s\n"
            % ("ok" if ok else "FAIL", name, order[expected], order[got] if _is_int(got) and got in (0, 1, 2) else got)
        )
    sys.stdout.write("SELFTEST ok=%d/%d\n" % (len(results) - len(failed), len(results)))
    return 1 if failed else 0


# ---- CLI -------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3④ 掉落/经验跨侧比较器")
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("diff", help="比较两侧 drop_exp_raw.jsonl")
    d.add_argument("csharp")
    d.add_argument("rust")
    d.set_defaults(func=cmd_diff)
    s = sub.add_parser("selftest", help="fail-closed + 改坏必红自检")
    s.set_defaults(func=cmd_selftest)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
