#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""npc_shop_diff.py —— M3⑤ NPC/商店跨侧对拍比较器（夹具骨架）。

卡：`openmir2-m3-shop-fixture-botsrv`（M3 逐系统对齐 ⑤ 的承接卡 6；父卡 `openmir2-m3-npc-shop-align`）。
本文件是**判据本体**，只做离线比较：读两侧各自的 `npc_shop_raw.jsonl`（一次交互一行），
逐 k 比较「op / NPC / 请求参数 / 帧序列（种类 + 声明的标量参数）/ 金币 / 背包多重集 /
仓库多重集 / 货架多重集 / 价格 / 失败分支」。真机跑由 `run_npc_shop_pair.ps1`（等 ⑤-4/⑤-5
落地 + owner 发窗口）驱动。

口径（与卡内 `machine_checkable_acceptance` ③ 对齐，逐条对齐盘点报告 §4.1）：

1. **fail-closed**：缺文件 / 缺字段 / 字段类型错 / 非法 JSON / k 不连续 / 记录非对象
   ⇒ 一律非零退出并点名（`PREREQ_RED`），**绝不静默 GREEN**。
2. **失败分支一致**：帧**种类序列**（`sm` id 顺序）严格逐项比 —— 「钱不够 / 负重满 /
   背包满 / 仓库满 / 无帧」不可互相顶替；另外比对场景声明的**标量帧参数**
   （如 `SM_SENDBUYPRICE.a` 卖价、`SM_BUYITEM_FAIL.a` 的 `n1C` 2/3、`SM_SENDREPAIRCOST.a` 修理价）。
   动态 id（merchant/make index）**不**作判据（跨运行本来不同）。
3. **多重集/计数式**：`bag_*` / `storage_*` / `goods_*` 一律按「每个不同元素的个数」比（`Counter`），
   **不做 set 差集**；同时保留逐元素下标定位。
4. **价格/金币一致**：`price` 与 `gold_before`/`gold_after` 按原始整数**原宽度**比（不得 i32 截断）。
5. **改坏必红**：`selftest` 对「op / npc / req / 帧种类 / 帧标量 / 价格 / 金币 / 背包 / 仓库 /
   货架 / branch / 记录数」逐个改坏 ⇒ 必红。

记录 schema（每行一个 JSON 对象；`npc_shop_scenario.json` 里也写一份）：

    {
      "k": 0,                       # 交互序号（0 起，必须连续）
      "op": "buy",                  # buy|sell|repair|query_sell_price|query_repair_cost|storage|takeback
      "npc": "卫家店",               # NPC 名（UserName；跨侧必须相同 —— 是选对 NPC 的直接证据）
      "npc_actor": 1234,            # 可选：NPC ActorId（动态，**不**作判据，只留审计）
      "req": {"item": "金创药(小量)", "n": 1},   # 请求参数（只比对 req_compare_keys 里的键）
      "frames": [                   # 该次交互收到的相关 SM_* 帧（有序）
        {"sm": 650, "a": 1000, "b": 1, "c": 0, "d": 0}
      ],
      "gold_before": 1000,          # 交互前玩家金币
      "gold_after": 990,            # 交互后玩家金币
      "bag_before": ["金创药(小量)#42"],   # 可选：背包多重集（元素为稳定键串）
      "bag_after": ["金创药(小量)#43"],
      "storage_before": [],         # 可选：仓库多重集
      "storage_after": [],
      "goods_before": [],           # 可选：商人货架多重集
      "goods_after": [],
      "price": 10,                  # 可选：本次交互的价格（卖价/修理价/成交价）
      "branch": "ok"                # ok|fail_sold_out|fail_overweight|fail_nomoney|fail_sell_reject|
                                    # fail_repair_reject|fail_storage_full|fail_storage_reject|
                                    # fail_bagfull|fail_takeback_reject|fail_no_frame
    }

退出码：0 = GREEN（逐项相同）；1 = RED（有差异）；2 = PREREQ（缺件/坏件，fail-closed）。

子命令：
  assemble <markers.jsonl> --out <raw.jsonl>
      把 BotSrv 的 `M3SHOPSEQ {json}` 事件流（phase=arm/send/frame/end/done）合并成上面的记录。
  diff <csharp_raw.jsonl> <rust_raw.jsonl> [--scenario <scenario.json>]
      逐 k 比较两侧记录。`--scenario` 给定时，标量帧参数表由场景的
      `frame_scalar_fields` 定义（单一真源；缺键/非法 ⇒ PREREQ）。
  selftest
      fail-closed + 改坏必红（离线）。
"""

import argparse
import collections
import json
import os
import shutil
import sys
import tempfile

# ---- schema ----------------------------------------------------------------

OPS = (
    "buy",
    "sell",
    "repair",
    "query_sell_price",
    "query_repair_cost",
    "storage",
    "takeback",
)

REQUIRED_FIELDS = ("k", "op", "npc", "req", "frames", "gold_before", "gold_after", "branch")
OPTIONAL_LIST_FIELDS = (
    "bag_before",
    "bag_after",
    "storage_before",
    "storage_after",
    "goods_before",
    "goods_after",
)
FRAME_FIELDS = ("sm", "a", "b", "c", "d")

# 请求参数里**参与判据**的键（其余键如 make_index/index 是动态 id，不作判据）。
REQ_COMPARE_KEYS = ("item", "n", "label")

# 内建的**标量帧参数**表（无 `--scenario` 时的回退值；runner 总是传 `--scenario` ⇒ 以场景为准）。
# 依据盘点报告 §1.H 的 RM_*→SM_* 映射（参数语义为价格/金币/耐久/失败码，跨运行稳定）；
# 动态 id（如 merchant actor / make index）一律不列。窗口期按真机实测补/删应改**场景**（`--scenario`）。
DEFAULT_FRAME_SCALAR_FIELDS = {
    647: ["a"],          # SM_SENDBUYPRICE  : a = 卖价
    649: ["a"],          # SM_USERSELLITEM_FAIL
    651: ["a"],          # SM_BUYITEM_FAIL  : a = n1C（2=负重不足/背包满，3=钱不够）—— 失败分支的核心
    669: ["a", "b", "c"],  # SM_USERREPAIRITEM_OK : a=Gold b=Dura c=DuraMax
    670: ["a"],          # SM_USERREPAIRITEM_FAIL
    671: ["a"],          # SM_SENDREPAIRCOST : a = 修理价
}


class Prereq(Exception):
    """缺件/坏件（fail-closed，rc=2）。"""


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


# 场景声明的判据键（单一真源；`--scenario` 给定后必须存在，否则 PREREQ）。
SCENARIO_SCALAR_KEY = "frame_scalar_fields"


def load_scenario_frame_scalars(path):
    """从 `*_scenario.json` 读**声明的标量帧参数表**（单一真源，fail-closed）。

    reviewer-6 的 M-3：标量表曾在 `npc_shop_scenario.json` 与
    `DEFAULT_FRAME_SCALAR_FIELDS` 各存一份，runner 又不透传 ⇒「改场景即改判据」是**假**的。
    这里让比较器显式读场景，缺文件 / 非 JSON / 缺键 / 类型错一律 Prereq（rc=2），绝不静默退回内建表。
    """
    if not os.path.isfile(path):
        raise Prereq("缺场景文件: %s" % path)
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
    except json.JSONDecodeError as exc:
        raise Prereq("场景非法 JSON %s: %s" % (path, exc))
    if not isinstance(data, dict):
        raise Prereq("场景应为对象: %s" % path)
    if SCENARIO_SCALAR_KEY not in data:
        raise Prereq("场景缺 `%s`（判据真源必须显式声明）: %s" % (SCENARIO_SCALAR_KEY, path))
    raw_fs = data[SCENARIO_SCALAR_KEY]
    if not isinstance(raw_fs, dict):
        raise Prereq("场景 `%s` 应为对象: %s" % (SCENARIO_SCALAR_KEY, path))
    scalars = {}
    for key, val in raw_fs.items():
        try:
            sm = int(key)
        except (TypeError, ValueError):
            raise Prereq("场景 `%s` 键非整数: %r (%s)" % (SCENARIO_SCALAR_KEY, key, path))
        if not isinstance(val, list) or not all(isinstance(x, str) for x in val):
            raise Prereq("场景 `%s[%s]` 应为字符串数组: %r (%s)"
                         % (SCENARIO_SCALAR_KEY, key, val, path))
        scalars[sm] = list(val)
    return scalars


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


def _validate_frames(frames, side, k, path):
    if not isinstance(frames, list):
        raise Prereq("%s: k=%s frames 非数组 (%s)" % (side, k, path))
    for i, fr in enumerate(frames):
        if not isinstance(fr, dict):
            raise Prereq("%s: k=%s frame[%d] 非对象 (%s)" % (side, k, i, path))
        if "sm" not in fr:
            raise Prereq("%s: k=%s frame[%d] 缺 sm (%s)" % (side, k, i, path))
        if not _is_int(fr["sm"]):
            raise Prereq("%s: k=%s frame[%d] sm 非整数 (%s)" % (side, k, i, path))
        for fld in FRAME_FIELDS[1:]:
            if fld in fr and not _is_int(fr[fld]):
                raise Prereq("%s: k=%s frame[%d] %s 非整数 (%s)" % (side, k, i, fld, path))


def validate(raw, path, side):
    """逐条校验 schema；任何缺失/类型错误 ⇒ Prereq（点名 side/k/字段）。"""
    if not raw:
        raise Prereq("%s: 无记录 (%s)" % (side, path))
    for i, rec in enumerate(raw):
        if "k" not in rec:
            raise Prereq("%s: 第 %d 条缺字段 k (%s)" % (side, i, path))
        if not _is_int(rec["k"]):
            raise Prereq("%s: k 非整数（第 %d 条=%r）(%s)" % (side, i, rec["k"], path))
        if rec["k"] != i:
            raise Prereq("%s: k 不连续/非 0 起（第 %d 条 k=%r）(%s)" % (side, i, rec["k"], path))
    for rec in raw:
        k = rec["k"]
        for field in REQUIRED_FIELDS:
            if field not in rec:
                raise Prereq("%s: k=%s 缺字段 %s (%s)" % (side, k, field, path))
        if rec["op"] not in OPS:
            raise Prereq("%s: k=%s op 非法 %r（允许 %s）(%s)" % (side, k, rec["op"], "/".join(OPS), path))
        if not isinstance(rec["npc"], str):
            raise Prereq("%s: k=%s npc 非字符串 (%s)" % (side, k, path))
        if not isinstance(rec["req"], dict):
            raise Prereq("%s: k=%s req 非对象 (%s)" % (side, k, path))
        if not isinstance(rec["branch"], str) or rec["branch"] == "":
            raise Prereq("%s: k=%s branch 非空字符串 (%s)" % (side, k, path))
        for fld in ("gold_before", "gold_after"):
            if not _is_int(rec[fld]):
                raise Prereq("%s: k=%s 字段 %s 非整数 (%s)" % (side, k, fld, path))
        if "price" in rec and rec["price"] is not None and not _is_int(rec["price"]):
            raise Prereq("%s: k=%s 字段 price 非整数/null (%s)" % (side, k, path))
        if "npc_actor" in rec and not _is_int(rec["npc_actor"]):
            raise Prereq("%s: k=%s 字段 npc_actor 非整数 (%s)" % (side, k, path))
        _validate_frames(rec["frames"], side, k, path)
        for fld in OPTIONAL_LIST_FIELDS:
            if fld in rec and rec[fld] is not None and not isinstance(rec[fld], list):
                raise Prereq("%s: k=%s 字段 %s 非数组 (%s)" % (side, k, fld, path))


# ---- assemble（BotSrv M3SHOPSEQ 事件流 → 记录） -----------------------------


def _marker_events(lines):
    """从文本行里抽出 `M3SHOPSEQ {json}` 事件（保持顺序）。非法 JSON ⇒ Prereq。"""
    out = []
    for lineno, line in enumerate(lines, 1):
        idx = line.find("M3SHOPSEQ ")
        if idx < 0:
            continue
        payload = line[idx + len("M3SHOPSEQ "):].strip()
        # 事件行形态：`M3SHOPSEQ { ... }`（单行 JSON）
        try:
            ev = json.loads(payload)
        except json.JSONDecodeError as exc:
            raise Prereq("M3SHOPSEQ 事件非法 JSON（第 %d 行）: %s" % (lineno, exc))
        if not isinstance(ev, dict):
            raise Prereq("M3SHOPSEQ 事件非对象（第 %d 行）" % lineno)
        out.append(ev)
    if not out:
        raise Prereq("无 M3SHOPSEQ 事件（驱动未生效/日志未含标记）")
    return out


def assemble(events, side="?"):
    """把 arm/send/frame/end/done 事件流合并成记录列表（fail-closed）。"""
    sends = {}
    ends = {}
    frames = collections.defaultdict(list)
    order = []
    done = None
    for ev in events:
        phase = ev.get("phase")
        if phase == "arm":
            continue
        if phase == "done":
            done = ev
            continue
        if phase == "send":
            k = ev.get("k")
            if not _is_int(k):
                raise Prereq("%s: send 事件缺整数 k: %r" % (side, ev))
            if k in sends:
                raise Prereq("%s: k=%d 重复 send 事件" % (side, k))
            sends[k] = ev
            order.append(k)
            continue
        if phase == "frame":
            k = ev.get("k")
            if not _is_int(k):
                raise Prereq("%s: frame 事件缺整数 k: %r" % (side, ev))
            if "frame" not in ev or not isinstance(ev["frame"], dict):
                raise Prereq("%s: k=%d frame 事件缺 frame 对象" % (side, k))
            frames[k].append(ev["frame"])
            continue
        if phase == "end":
            k = ev.get("k")
            if not _is_int(k):
                raise Prereq("%s: end 事件缺整数 k: %r" % (side, ev))
            if k in ends:
                raise Prereq("%s: k=%d 重复 end 事件" % (side, k))
            ends[k] = ev
            continue
        raise Prereq("%s: 未知 phase=%r" % (side, phase))
    if done is not None:
        n = done.get("ops")
        if _is_int(n) and n != len(order):
            raise Prereq("%s: done.ops=%d 与 send 事件数 %d 不一致" % (side, n, len(order)))
    records = []
    for k in order:
        if k not in ends:
            raise Prereq("%s: k=%d 有 send 无 end（交互未收尾）—— fail-closed" % (side, k))
        se = sends[k]
        en = ends[k]
        rec = {
            "k": k,
            "op": se.get("op"),
            "npc": se.get("npc"),
            "req": se.get("req") if isinstance(se.get("req"), dict) else {},
            "frames": frames.get(k, []),
            "gold_before": se.get("gold_before"),
            "gold_after": en.get("gold_after"),
            "branch": en.get("branch"),
        }
        if "npc_actor" in se:
            rec["npc_actor"] = se["npc_actor"]
        for key in OPTIONAL_LIST_FIELDS:
            v = en.get(key, se.get(key))
            if v is not None:
                rec[key] = v
        if "price" in en:
            rec["price"] = en["price"]
        records.append(rec)
    return records


def cmd_assemble(args):
    if not os.path.isfile(args.markers):
        sys.stdout.write("RED: PREREQ_RED 缺文件: %s\n" % args.markers)
        return 2
    try:
        with open(args.markers, "r", encoding="utf-8") as fh:
            events = _marker_events(fh.read().splitlines())
        recs = assemble(events, side=args.side)
        validate(recs, args.markers, args.side)
    except Prereq as exc:
        sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
        return 2
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
            for rec in recs:
                fh.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")
        sys.stdout.write("ASSEMBLED ok ops=%d out=%s\n" % (len(recs), args.out))
    else:
        for rec in recs:
            sys.stdout.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")
    return 0


# ---- comparison ------------------------------------------------------------


def _norm_req(req):
    out = {}
    for key in REQ_COMPARE_KEYS:
        if key in req:
            out[key] = req[key]
    return out


def _frame_sm_seq(frames):
    return [f["sm"] for f in frames]


def _frame_scalars(frames, sm, fields):
    out = {}
    for f in frames:
        if f["sm"] != sm:
            continue
        for fld in fields:
            if fld in f:
                out["%d.%s" % (sm, fld)] = f[fld]
    return out


def _multiset(lst):
    return collections.Counter(lst)


def compare(cs, rs, frame_scalars=None, req_keys=REQ_COMPARE_KEYS):
    """返回 (diffs, stats)。diffs 为空 ⇒ GREEN。"""
    frame_scalars = frame_scalars or DEFAULT_FRAME_SCALAR_FIELDS
    diffs = []
    stats = {"ops": len(cs), "frames": 0, "ok": 0, "fail": 0}
    if len(cs) != len(rs):
        diffs.append(
            (None, "记录条数", "csharp=%d" % len(cs), "rust=%d" % len(rs))
        )
        return diffs, stats
    for k in range(len(cs)):
        a, b = cs[k], rs[k]
        stats["frames"] += len(a.get("frames") or [])
        if str(a.get("branch", "")).startswith("ok"):
            stats["ok"] += 1
        else:
            stats["fail"] += 1
        for fld in ("op", "npc", "branch"):
            if a.get(fld) != b.get(fld):
                diffs.append((k, fld, a.get(fld), b.get(fld)))
        # req：只比声明键
        ra, rb = _norm_req(a.get("req") or {}), _norm_req(b.get("req") or {})
        ra = {f: ra[f] for f in req_keys if f in ra}
        rb = {f: rb[f] for f in req_keys if f in rb}
        if ra != rb:
            diffs.append((k, "req", ra, rb))
        # 金币（原宽度整数）
        for fld in ("gold_before", "gold_after"):
            if a.get(fld) != b.get(fld):
                diffs.append((k, fld, a.get(fld), b.get(fld)))
        # 价格
        if a.get("price") != b.get("price"):
            diffs.append((k, "price", a.get("price"), b.get("price")))
        # 帧种类序列（失败分支的身份）
        sa, sb = _frame_sm_seq(a.get("frames") or []), _frame_sm_seq(b.get("frames") or [])
        if sa != sb:
            diffs.append((k, "frames.sm", sa, sb))
        # 帧标量参数（声明的 (sm,字段)）
        for sm, fields in sorted(frame_scalars.items()):
            va = _frame_scalars(a.get("frames") or [], sm, fields)
            vb = _frame_scalars(b.get("frames") or [], sm, fields)
            if va != vb:
                diffs.append((k, "frames.scalar[%d]" % sm, va, vb))
        # 多重集：背包 / 仓库 / 货架
        for fld in OPTIONAL_LIST_FIELDS:
            ma, mb = _multiset(a.get(fld) or []), _multiset(b.get(fld) or [])
            if ma != mb:
                missing = dict((str(x), v) for x, v in (ma - mb).items())
                extra = dict((str(x), v) for x, v in (mb - ma).items())
                diffs.append((k, "%s.multiset" % fld, missing, extra))
    return diffs, stats


def render(diffs, stats, cs_path, rs_path):
    lines = []
    for k, what, av, bv in diffs:
        loc = "k=%s" % ("-" if k is None else k)
        lines.append("SHOP %s RED %s csharp=%r rust=%r" % (loc, what, av, bv))
    lines.append(
        "STATS ops=%d frames=%d ok=%d fail=%d" % (stats["ops"], stats["frames"], stats["ok"], stats["fail"])
    )
    lines.append("CSHARP=%s" % cs_path)
    lines.append("RUST=%s" % rs_path)
    lines.append("DIFF %d" % len(diffs))
    return "\n".join(lines)


def cmd_diff(args):
    scalars = DEFAULT_FRAME_SCALAR_FIELDS
    frame_scalars_arg = getattr(args, "frame_scalars", "")
    scenario = getattr(args, "scenario", None)
    if scenario:
        # 判据真源只能有一个：场景 vs 显式 JSON 同时给 ⇒ fail-closed（不猜）。
        if frame_scalars_arg:
            sys.stdout.write("RED: PREREQ_RED --scenario 与 --frame-scalars 互斥（判据真源只能一个）\n")
            return 2
        try:
            scalars = load_scenario_frame_scalars(scenario)
        except Prereq as exc:
            sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
            return 2
    elif frame_scalars_arg:
        try:
            raw = json.loads(frame_scalars_arg)
            scalars = {int(k): list(v) for k, v in raw.items()}
        except (ValueError, TypeError) as exc:
            sys.stdout.write("RED: PREREQ_RED --frame-scalars 非法: %s\n" % exc)
            return 2
    try:
        cs = load_raw(args.csharp)
        rs = load_raw(args.rust)
        validate(cs, args.csharp, "csharp")
        validate(rs, args.rust, "rust")
    except Prereq as exc:
        sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
        return 2
    diffs, stats = compare(cs, rs, frame_scalars=scalars)
    sys.stdout.write(render(diffs, stats, args.csharp, args.rust) + "\n")
    if diffs:
        sys.stdout.write("RED: NPC/商店跨侧差异 %d 项（定位见上）\n" % len(diffs))
        return 1
    sys.stdout.write(
        "GREEN: NPC/商店逐项相同（ops=%d 帧=%d 失败分支=%d）\n"
        % (stats["ops"], stats["frames"], stats["fail"])
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
            "op": "sell",
            "npc": "卫家店",
            "npc_actor": 101,
            "req": {"item": "金创药(小量)", "n": 1, "index": 42},
            "frames": [{"sm": 648, "a": 1010, "b": 0, "c": 0, "d": 0}],
            "gold_before": 1000,
            "gold_after": 1010,
            "bag_before": ["金创药(小量)#42", "木剑#43"],
            "bag_after": ["木剑#43"],
            "storage_before": [],
            "storage_after": [],
            "goods_before": [],
            "goods_after": ["金创药(小量)#42"],
            "price": 10,
            "branch": "ok",
        },
        {
            "k": 1,
            "op": "buy",
            "npc": "卫家店",
            "npc_actor": 101,
            "req": {"item": "金创药(小量)", "n": 1, "index": 0},
            "frames": [{"sm": 651, "a": 3, "b": 0, "c": 0, "d": 0}],
            "gold_before": 0,
            "gold_after": 0,
            "bag_before": ["木剑#43"],
            "bag_after": ["木剑#43"],
            "goods_before": ["金创药(小量)#42"],
            "goods_after": ["金创药(小量)#42"],
            "branch": "fail_nomoney",
        },
    ]


def _run_diff(cs_path, rs_path):
    return cmd_diff(argparse.Namespace(csharp=cs_path, rust=rs_path, frame_scalars=None, scenario=None))


def _run_diff_scenario(cs_path, rs_path, scenario_path):
    return cmd_diff(argparse.Namespace(csharp=cs_path, rust=rs_path, frame_scalars=None,
                                       scenario=scenario_path))


def cmd_selftest(args):
    tmp = tempfile.mkdtemp(prefix="npc-shop-selftest-")
    results = []

    def case(name, expected_rc, fn):
        try:
            got = fn()
        except Exception as exc:  # noqa: BLE001 —— 自检要如实报红，不吞
            got = "exception:%s" % exc
        ok = got == expected_rc
        results.append((name, expected_rc, got, ok))

    try:
        cs0 = os.path.join(tmp, "cs0.jsonl")
        rs0 = os.path.join(tmp, "rs0.jsonl")
        _write_jsonl(cs0, _base_records())
        _write_jsonl(rs0, _base_records())
        case("identical-green", 0, lambda: _run_diff(cs0, rs0))

        # 红：改 op
        p = os.path.join(tmp, "rs_op.jsonl")
        rec = _base_records(); rec[1]["op"] = "storage"
        _write_jsonl(p, rec)
        case("mutate-op-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 npc 名
        p = os.path.join(tmp, "rs_npc.jsonl")
        rec = _base_records(); rec[0]["npc"] = "仓库保管员"
        _write_jsonl(p, rec)
        case("mutate-npc-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 req.item
        p = os.path.join(tmp, "rs_req.jsonl")
        rec = _base_records(); rec[0]["req"]["item"] = "木剑"
        _write_jsonl(p, rec)
        case("mutate-req-item-red", 1, lambda: _run_diff(cs0, p))

        # 绿：req 里**不**参与判据的动态键不同 ⇒ 仍绿（不得对动态 id 判红）
        p = os.path.join(tmp, "rs_req_dyn.jsonl")
        rec = _base_records(); rec[0]["req"]["index"] = 999; rec[0]["npc_actor"] = 777
        _write_jsonl(p, rec)
        case("dynamic-id-ignored-green", 0, lambda: _run_diff(cs0, p))

        # 红：改帧种类序列（失败分支身份）
        p = os.path.join(tmp, "rs_sm.jsonl")
        rec = _base_records(); rec[1]["frames"] = [{"sm": 650, "a": 0, "b": 0, "c": 0, "d": 0}]
        _write_jsonl(p, rec)
        case("mutate-frame-kind-red", 1, lambda: _run_diff(cs0, p))

        # 红：改帧标量参数（SM_BUYITEM_FAIL.a 的 n1C 2 与 3 不能混）
        p = os.path.join(tmp, "rs_scalar.jsonl")
        rec = _base_records(); rec[1]["frames"][0]["a"] = 2
        _write_jsonl(p, rec)
        case("mutate-frame-scalar-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 price
        p = os.path.join(tmp, "rs_price.jsonl")
        rec = _base_records(); rec[0]["price"] = 11
        _write_jsonl(p, rec)
        case("mutate-price-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 gold_after
        p = os.path.join(tmp, "rs_gold.jsonl")
        rec = _base_records(); rec[0]["gold_after"] = 1011
        _write_jsonl(p, rec)
        case("mutate-gold-red", 1, lambda: _run_diff(cs0, p))

        # 红：改背包多重集（同元素个数不同 ⇒ 计数式必须咬住）
        p = os.path.join(tmp, "rs_bag.jsonl")
        rec = _base_records(); rec[0]["bag_after"] = ["木剑#43", "木剑#43"]
        _write_jsonl(p, rec)
        case("mutate-bag-multiset-red", 1, lambda: _run_diff(cs0, p))

        # 红：改仓库多重集
        p = os.path.join(tmp, "rs_storage.jsonl")
        rec = _base_records(); rec[0]["storage_after"] = ["木剑#43"]
        _write_jsonl(p, rec)
        case("mutate-storage-multiset-red", 1, lambda: _run_diff(cs0, p))

        # 红：改货架多重集
        p = os.path.join(tmp, "rs_goods.jsonl")
        rec = _base_records(); rec[0]["goods_after"] = []
        _write_jsonl(p, rec)
        case("mutate-goods-multiset-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 branch（ok ↔ 失败分支不能顶替）
        p = os.path.join(tmp, "rs_branch.jsonl")
        rec = _base_records(); rec[1]["branch"] = "ok"
        _write_jsonl(p, rec)
        case("mutate-branch-red", 1, lambda: _run_diff(cs0, p))

        # 红：记录条数不同
        p = os.path.join(tmp, "rs_len.jsonl")
        _write_jsonl(p, _base_records()[:1])
        case("mutate-op-count-red", 1, lambda: _run_diff(cs0, p))

        # 红：不得 i32 截断（只在高 32 位不同的金币必须判红）
        cs_big = os.path.join(tmp, "cs_big.jsonl")
        rs_big = os.path.join(tmp, "rs_big.jsonl")
        rec = _base_records(); rec[0]["gold_after"] = 3000000000
        _write_jsonl(cs_big, rec)
        rec2 = _base_records(); rec2[0]["gold_after"] = 3000000000 + (1 << 32)
        _write_jsonl(rs_big, rec2)
        case("i32-truncation-red", 1, lambda: _run_diff(cs_big, rs_big))

        # ---- 场景判据真源（reviewer-6 M-3：改场景即改判据）----
        # rs_scalar2 只把 k=1 的 651.a 从 3 改成 2（其余逐字节同 cs0）。
        p_scalar2 = os.path.join(tmp, "rs_scalar2.jsonl")
        rec = _base_records(); rec[1]["frames"][0]["a"] = 2
        _write_jsonl(p_scalar2, rec)

        scen_full = os.path.join(tmp, "scen_full.json")
        with open(scen_full, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"frame_scalar_fields": {str(k): list(v)
                                                for k, v in DEFAULT_FRAME_SCALAR_FIELDS.items()}},
                      fh, ensure_ascii=False)
        # 场景声明 651.a ⇒ 该差异是判据 ⇒ 红。
        case("scenario-declared-scalar-red", 1,
             lambda: _run_diff_scenario(cs0, p_scalar2, scen_full))

        # 场景**不声明** 651.a ⇒ 该差异不再参与判据 ⇒ 绿（证明场景真的改判据）。
        scen_narrow = os.path.join(tmp, "scen_narrow.json")
        with open(scen_narrow, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"frame_scalar_fields": {"647": ["a"], "649": ["a"]}},
                      fh, ensure_ascii=False)
        case("scenario-authority-green", 0,
             lambda: _run_diff_scenario(cs0, p_scalar2, scen_narrow))

        # PREREQ：场景缺 frame_scalar_fields（不得静默退回内建表）
        scen_nokey = os.path.join(tmp, "scen_nokey.json")
        with open(scen_nokey, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"cases": {}}, fh, ensure_ascii=False)
        case("scenario-missing-key-prereq", 2,
             lambda: _run_diff_scenario(cs0, rs0, scen_nokey))

        # PREREQ：场景非法 JSON
        scen_bad = os.path.join(tmp, "scen_bad.json")
        with open(scen_bad, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("{not json\n")
        case("scenario-bad-json-prereq", 2,
             lambda: _run_diff_scenario(cs0, rs0, scen_bad))

        # PREREQ：场景与 --frame-scalars 互斥
        case("scenario-mutex-prereq", 2,
             lambda: cmd_diff(argparse.Namespace(csharp=cs0, rust=rs0,
                                                 frame_scalars='{"651":["a"]}',
                                                 scenario=scen_full)))

        # PREREQ：缺文件
        case("missing-file-prereq", 2,
             lambda: _run_diff(os.path.join(tmp, "nope.jsonl"), rs0))

        # PREREQ：缺字段
        p = os.path.join(tmp, "rs_missing.jsonl")
        rec = _base_records(); del rec[0]["branch"]
        _write_jsonl(p, rec)
        case("missing-field-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：非法 JSON
        p = os.path.join(tmp, "rs_bad.jsonl")
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("{not json\n")
        case("invalid-json-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：k 不连续
        p = os.path.join(tmp, "rs_gap.jsonl")
        rec = _base_records(); rec[1]["k"] = 5
        _write_jsonl(p, rec)
        case("k-gap-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：op 非法
        p = os.path.join(tmp, "rs_badop.jsonl")
        rec = _base_records(); rec[0]["op"] = "teleport"
        _write_jsonl(p, rec)
        case("invalid-op-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：frames 非数组
        p = os.path.join(tmp, "rs_badframes.jsonl")
        rec = _base_records(); rec[0]["frames"] = {"sm": 648}
        _write_jsonl(p, rec)
        case("invalid-frames-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：空文件
        p = os.path.join(tmp, "rs_empty.jsonl")
        open(p, "w", encoding="utf-8", newline="\n").close()
        case("empty-file-prereq", 2, lambda: _run_diff(cs0, p))

        # ---- assemble 子命令（BotSrv 事件流 → 记录）----
        markers = os.path.join(tmp, "markers.jsonl")
        evs = [
            {"phase": "arm", "ops": ["sell", "buy"]},
            {"phase": "send", "k": 0, "op": "sell", "npc": "卫家店", "npc_actor": 101,
             "req": {"item": "金创药(小量)", "n": 1}, "gold_before": 1000,
             "bag_before": ["金创药(小量)#42", "木剑#43"]},
            {"phase": "frame", "k": 0, "frame": {"sm": 648, "a": 1010, "b": 0, "c": 0, "d": 0}},
            {"phase": "end", "k": 0, "gold_after": 1010, "bag_after": ["木剑#43"],
             "goods_after": ["金创药(小量)#42"], "price": 10, "branch": "ok"},
            {"phase": "send", "k": 1, "op": "buy", "npc": "卫家店", "npc_actor": 101,
             "req": {"item": "金创药(小量)", "n": 1}, "gold_before": 0,
             "bag_before": ["木剑#43"], "goods_before": ["金创药(小量)#42"]},
            {"phase": "frame", "k": 1, "frame": {"sm": 651, "a": 3, "b": 0, "c": 0, "d": 0}},
            {"phase": "end", "k": 1, "gold_after": 0, "bag_after": ["木剑#43"],
             "goods_after": ["金创药(小量)#42"], "branch": "fail_nomoney"},
            {"phase": "done", "ops": 2},
        ]
        with open(markers, "w", encoding="utf-8", newline="\n") as fh:
            for ev in evs:
                fh.write("robot says: " + "M3SHOPSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
        out = os.path.join(tmp, "assembled.jsonl")

        def _assemble():
            return cmd_assemble(argparse.Namespace(markers=markers, out=out, side="csharp"))

        case("assemble-ok", 0, _assemble)

        # assemble 出来的记录必须与基准记录同形（sort_keys 后逐字节相同）→ diff 绿
        def _assemble_then_diff():
            rc = _assemble()
            if rc != 0:
                return rc
            return _run_diff(cs0, out)

        case("assemble-then-diff-green", 0, _assemble_then_diff)

        # assemble：有 send 无 end ⇒ PREREQ（交互未收尾，fail-closed）
        markers_missing_end = os.path.join(tmp, "markers_noend.jsonl")
        with open(markers_missing_end, "w", encoding="utf-8", newline="\n") as fh:
            for ev in evs[:-2]:  # 去掉最后 end + done
                fh.write("M3SHOPSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
        case("assemble-missing-end-prereq", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers_missing_end, out="", side="csharp")))

        # assemble：无事件 ⇒ PREREQ
        markers_empty = os.path.join(tmp, "markers_empty.jsonl")
        with open(markers_empty, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("no markers here\n")
        case("assemble-no-events-prereq", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers_empty, out="", side="csharp")))

        # assemble：done.ops 与 send 数不一致 ⇒ PREREQ
        markers_baddone = os.path.join(tmp, "markers_baddone.jsonl")
        with open(markers_baddone, "w", encoding="utf-8", newline="\n") as fh:
            for ev in evs:
                if ev.get("phase") == "done":
                    ev = {"phase": "done", "ops": 5}
                fh.write("M3SHOPSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
        case("assemble-done-count-prereq", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers_baddone, out="", side="csharp")))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    order = ["GREEN", "RED", "PREREQ"]
    failed = [(n, e, g) for (n, e, g, ok) in results if not ok]
    for name, expected, got, ok in results:
        shown = order[got] if _is_int(got) and got in (0, 1, 2) else got
        sys.stdout.write("SELFTEST %s %s expect=%s got=%s\n"
                         % ("ok" if ok else "FAIL", name, order[expected], shown))
    sys.stdout.write("SELFTEST ok=%d/%d\n" % (len(results) - len(failed), len(results)))
    return 1 if failed else 0


# ---- CLI -------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3⑤ NPC/商店跨侧比较器（夹具骨架）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("assemble", help="BotSrv M3SHOPSEQ 事件流 → npc_shop_raw.jsonl")
    a.add_argument("markers")
    a.add_argument("--out", default="")
    a.add_argument("--side", default="?")
    a.set_defaults(func=cmd_assemble)
    d = sub.add_parser("diff", help="比较两侧 npc_shop_raw.jsonl")
    d.add_argument("csharp")
    d.add_argument("rust")
    d.add_argument("--frame-scalars", default="",
                   help='可选 JSON: {"647":["a"],...} 覆盖默认标量帧参数表'
                        '（与 --scenario 互斥）')
    d.add_argument("--scenario", default="",
                   help="场景 JSON；其 `frame_scalar_fields` 为判据单一真源（缺键 ⇒ PREREQ）")
    d.set_defaults(func=cmd_diff)
    s = sub.add_parser("selftest", help="fail-closed + 改坏必红自检")
    s.set_defaults(func=cmd_selftest)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
