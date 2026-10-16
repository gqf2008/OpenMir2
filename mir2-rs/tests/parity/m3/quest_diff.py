#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""quest_diff.py —— M3⑥ 任务（quest）跨侧对拍比较器（夹具骨架）。

卡：`openmir2-m3-quest-fixture-botsrv`（M3 逐系统对齐 ⑥ 的承接卡 5；父卡 `openmir2-m3-quest-align`）。
本文件是**判据本体**，只做离线比较：读两侧各自的 `quest_raw.jsonl`（一次交互一行），
逐 k 比较「op / NPC / 地图 / 标签 / 请求参数 / 帧序列（种类 + `SM_MERCHANTSAY(643)` 说词逐字节 +
声明的标量帧参数）/ 金币 / 背包多重集 / **会话任务态位（QuestFlag/QuestUnit/QuestUnitOpen）** /
失败分支」。真机跑由 `run_quest_pair.ps1`（等 ⑥-3 `openmir2-m3-quest-dialog-wiring` 落地 +
owner 发窗口）驱动。

口径（与卡内 `machine_checkable_acceptance` ③ 对齐，逐条对齐盘点报告 §4.1）：

1. **fail-closed**：缺文件 / 缺字段 / 字段类型错 / 非法 JSON / k 不连续 / 记录非对象 /
   **任务态位缺失或非法 hex** ⇒ 一律非零退出并点名（`PREREQ_RED`），**绝不静默 GREEN**。
2. **接/交/条件判定一致**：帧**种类序列**（`sm` id 顺序）严格逐项比；`SM_MERCHANTSAY(643)`
   的**说词字节逐字节**比（这是「脚本说词」的直接证据）；另外比对场景声明的**标量帧参数**。
   动态 id（npc actor / make index）**不**作判据（跨运行本来不同）。
3. **会话任务态一致**：`quest_flag_*` / `quest_unit_*` / `quest_open_*` 按**规范化 hex 字符串逐字节**比
   （`set [107] 1` ⇒ `QuestFlag[13] & 32 != 0` 这类位差必须被咬住）。**必填 `quest_flag_before/after`**；
   `quest_unit_*` / `quest_open_*` 若一侧有另一侧没有 ⇒ 判红（不许静默吞）。
   ⚠ 本判据的输入 = **服务端会话 dump**（BotSrv 客户端看不到任务数组，见盘点报告 §4.1 第 2 条）；
   `assemble --state <file>` 把两侧服务端 dump 合并进记录；**没有它 ⇒ diff 一律 PREREQ，绝不 GREEN**。
4. **多重集/计数式**：`bag_*` 一律按「每个不同元素的个数」比（`Counter`），**不做 set 差集**。
5. **金币原宽度**：`gold_before`/`gold_after` 按原始整数比（不得 i32 截断）。
6. **落库口径（= oracle「任务态不落库」）**：本比较器**不**判「重登后任务标志仍在」——oracle MySQL
   本身不持久任务态（盘点报告 §1.F）。DB 侧判据由 `scoped_diff.py --canonical` 在夹具入口里做
   （`characters_quest` 两侧均 0 行 + `characters` 无任务列变更），**不要**在记录里造「跨重登保留」。
7. **改坏必红**：`selftest` 对「op / npc / label / 帧种类 / 帧说词 / 帧标量 / 价格 / 金币 / 背包 /
   任务态一位 / branch / 记录数」逐个改坏 ⇒ 必红。

记录 schema（每行一个 JSON 对象；`quest_scenario.json` 里也写一份）：

    {
      "k": 0,                          # 交互序号（0 起，必须连续）
      "op": "click_npc",               # click_npc|dialog_select|take|give|set|check
      "npc": "任务管理",                # NPC 名（UserName；跨侧必须相同 —— 是选对 NPC 的直接证据）
      "npc_actor": 1234,               # 可选：NPC ActorId（动态，**不**作判据，只留审计）
      "map": "3",                      # 可选：地图名（跨侧必须相同）
      "label": "@rw06",                # 可选：本步 select 的脚本标签（click_npc 记 @main）
      "req": {"item": "无字天书", "n": 1},  # 请求参数（只比对 req_compare_keys 里的键）
      "frames": [                       # 该次交互收到的相关 SM_* 帧（有序）
        {"sm": 643, "text": "任务管理/苍月岛的首饰店老板正在找人帮忙"}
      ],
      "gold_before": 1000, "gold_after": 1000,
      "bag_before": ["无字天书#42"], "bag_after": [],
      "quest_flag_before": "0000...",  # 必填：服务端会话 QuestFlag[128]（hex，大小写不敏感）
      "quest_flag_after":  "0000...",
      "quest_unit_before": "0000...",  # 可选：QuestUnit[128]（一侧有另一侧没有 ⇒ 判红）
      "quest_unit_after":  "0000...",
      "quest_open_before": "0000...",  # 可选：QuestUnitOpen[128]
      "quest_open_after":  "0000...",
      "branch": "ok"                    # ok | fail_item | fail_level | fail_name_list |
                                        # fail_no_frame | fail_reject | ...
    }

退出码：0 = GREEN（逐项相同）；1 = RED（有差异）；2 = PREREQ（缺件/坏件，fail-closed）。

子命令：
  assemble <markers.jsonl> --out <raw.jsonl> [--state <state.jsonl>] [--side ?]
      把 BotSrv 的 `M3QUESTSEQ {json}` 事件流（phase=arm/send/frame/end/done）合并成记录；
      `--state` 给的是两侧各自的**服务端会话任务态 dump**（每行 `{"k":N,...quest_*_before/after}`），
      合并后记录才带 `quest_flag_*`（否则 diff 阶段 fail-closed）。
  diff <csharp_raw.jsonl> <rust_raw.jsonl> [--frame-scalars JSON] [--text-sms 643[,..]]
      逐 k 比较两侧记录。
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

OPS = ("click_npc", "dialog_select", "take", "give", "set", "check")

# 结构必填（不依赖任务态 dump 也在）。
REQUIRED_FIELDS = ("k", "op", "npc", "frames", "gold_before", "gold_after", "branch")
# 任务态必填（diff 阶段强制；assemble 阶段可缺，交给 diff 判 PREREQ）。
REQUIRED_STATE_FIELDS = ("quest_flag_before", "quest_flag_after")
# 任务态可选（一侧有另一侧没有 ⇒ 判红，见 compare）。
OPTIONAL_STATE_FIELDS = (
    "quest_unit_before",
    "quest_unit_after",
    "quest_open_before",
    "quest_open_after",
)
OPTIONAL_LIST_FIELDS = ("bag_before", "bag_after")
FRAME_FIELDS = ("sm", "a", "b", "c", "d")

# 请求参数里**参与判据**的键（其余键如 make/index 是动态 id，不作判据）。
REQ_COMPARE_KEYS = ("item", "n")

# 需要逐字节比对**说词文本**的 SM（默认只有 SM_MERCHANTSAY(643)；窗口期可按真机实测补）。
DEFAULT_TEXT_SMS = (643,)

# 场景声明的**标量帧参数**：只有这些 (sm, 参数名) 参与判据。默认空（任务链的强判据是
# s m 种类序列 + 643 说词文本 + 会话任务态位）；窗口期可按真机实测补/删（改场景即改判据）。
# 例：MESSAGEBOX 走 `SM_*` 时若其正文以整数参数承载，可在此声明。
DEFAULT_FRAME_SCALAR_FIELDS = {}


class Prereq(Exception):
    """缺件/坏件（fail-closed，rc=2）。"""


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def _norm_hex(v):
    """规范 hex（去空白、小写）。非法 ⇒ 抛 ValueError。空串合法（= 全 0 的等价空）。"""
    if not isinstance(v, str):
        raise ValueError("非字符串: %r" % (v,))
    s = v.strip().lower()
    if s == "":
        return ""
    if len(s) % 2 != 0:
        raise ValueError("hex 长度为奇数: %r" % (v,))
    int(s, 16)  # 非 hex 会抛
    return s


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


def _validate_frames(frames, side, k, path, text_sms):
    if not isinstance(frames, list):
        raise Prereq("%s: k=%s frames 非数组 (%s)" % (side, k, path))
    for i, fr in enumerate(frames):
        if not isinstance(fr, dict):
            raise Prereq("%s: k=%s frame[%d] 非对象 (%s)" % (side, k, i, path))
        if "sm" not in fr:
            raise Prereq("%s: k=%s frame[%d] 缺 sm (%s)" % (side, k, i, path))
        if not _is_int(fr["sm"]):
            raise Prereq("%s: k=%s frame[%d] sm 非整数 (%s)" % (side, k, i, path))
        if fr["sm"] in text_sms:
            if "text" not in fr:
                raise Prereq("%s: k=%s frame[%d] sm=%s 是文本帧但缺 text (%s)" % (side, k, i, fr["sm"], path))
            if not isinstance(fr["text"], str):
                raise Prereq("%s: k=%s frame[%d] text 非字符串 (%s)" % (side, k, i, path))
        for fld in FRAME_FIELDS[1:]:
            if fld in fr and not _is_int(fr[fld]):
                raise Prereq("%s: k=%s frame[%d] %s 非整数 (%s)" % (side, k, i, fld, path))


def validate(raw, path, side, require_state=True, text_sms=DEFAULT_TEXT_SMS):
    """逐条校验 schema；任何缺失/类型错误 ⇒ Prereq（点名 side/k/字段）。

    `require_state=True`（diff 阶段）：`quest_flag_before/after` 必须存在且为合法 hex；
    缺失 ⇒ PREREQ —— 没有服务端会话 dump 就**不能**判任务态一致（绝不静默 GREEN）。
    `require_state=False`（assemble 阶段）：只校验结构（k/op/npc/frames/gold/branch）。
    """
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
        if "map" in rec and not isinstance(rec["map"], str):
            raise Prereq("%s: k=%s map 非字符串 (%s)" % (side, k, path))
        if "label" in rec and not isinstance(rec["label"], str):
            raise Prereq("%s: k=%s label 非字符串 (%s)" % (side, k, path))
        if "req" in rec and not isinstance(rec["req"], dict):
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
        _validate_frames(rec["frames"], side, k, path, text_sms)
        for fld in OPTIONAL_LIST_FIELDS:
            if fld in rec and rec[fld] is not None and not isinstance(rec[fld], list):
                raise Prereq("%s: k=%s 字段 %s 非数组 (%s)" % (side, k, fld, path))
        if require_state:
            for fld in REQUIRED_STATE_FIELDS:
                if fld not in rec:
                    raise Prereq(
                        "%s: k=%s 缺任务态字段 %s —— 没有服务端会话 dump（assemble --state）就"
                        "不能判『任务态一致』（盘点报告 §4.1 第 2 条；绝不静默 GREEN）(%s)"
                        % (side, k, fld, path))
                try:
                    _norm_hex(rec[fld])
                except ValueError as exc:
                    raise Prereq("%s: k=%s 字段 %s 非法 hex: %s (%s)" % (side, k, fld, exc, path))
        for fld in OPTIONAL_STATE_FIELDS:
            if fld in rec and rec[fld] is not None:
                try:
                    _norm_hex(rec[fld])
                except ValueError as exc:
                    raise Prereq("%s: k=%s 字段 %s 非法 hex: %s (%s)" % (side, k, fld, exc, path))


# ---- assemble（BotSrv M3QUESTSEQ 事件流 + 服务端 dump → 记录） ---------------


def _marker_events(lines):
    """从文本行里抽出 `M3QUESTSEQ {json}` 事件（保持顺序）。非法 JSON ⇒ Prereq。"""
    out = []
    for lineno, line in enumerate(lines, 1):
        idx = line.find("M3QUESTSEQ ")
        if idx < 0:
            continue
        payload = line[idx + len("M3QUESTSEQ "):].strip()
        try:
            ev = json.loads(payload)
        except json.JSONDecodeError as exc:
            raise Prereq("M3QUESTSEQ 事件非法 JSON（第 %d 行）: %s" % (lineno, exc))
        if not isinstance(ev, dict):
            raise Prereq("M3QUESTSEQ 事件非对象（第 %d 行）" % lineno)
        out.append(ev)
    if not out:
        raise Prereq("无 M3QUESTSEQ 事件（驱动未生效/日志未含标记）")
    return out


def load_state(path, allow_construct=False):
    """读服务端会话任务态 dump（JSONL，每行含整数 k + quest_*_before/after）。

    `construct_level:true`（构造级生产者，见 `quest_state_producer.py` / whitelist T-58）**默认拒收**
    （Prereq，fail-closed）——防止把「按场景声明推出来的位图」当服务端真值判绿。只有显式
    `allow_construct=True`（离线自检用）才接受。"""
    if not os.path.isfile(path):
        raise Prereq("缺任务态 dump: %s" % path)
    by_k = {}
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.strip()
            if line == "":
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError as exc:
                raise Prereq("任务态 dump 非法 JSON %s:%d: %s" % (path, lineno, exc))
            if not isinstance(rec, dict) or not _is_int(rec.get("k")):
                raise Prereq("任务态 dump 第 %d 行缺整数 k (%s)" % (lineno, path))
            k = rec["k"]
            if k in by_k:
                raise Prereq("任务态 dump k=%d 重复 (%s)" % (k, path))
            if rec.get("construct_level") and not allow_construct:
                raise Prereq("任务态 dump 第 %d 行是构造级(construct_level) —— 非服务端真值，"
                             "默认拒收（要用于离线自检请显式 --allow-construct-state）(%s)" % (lineno, path))
            by_k[k] = rec
    if not by_k:
        raise Prereq("任务态 dump 无记录: %s" % path)
    return by_k


def assemble(events, side="?", state_by_k=None):
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
        n = done.get("steps")
        if _is_int(n) and n != len(order):
            raise Prereq("%s: done.steps=%d 与 send 事件数 %d 不一致" % (side, n, len(order)))
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
            "frames": frames.get(k, []),
            "gold_before": se.get("gold_before"),
            "gold_after": en.get("gold_after"),
            "branch": en.get("branch"),
        }
        for key in ("npc_actor", "map", "label"):
            if key in se:
                rec[key] = se[key]
        if isinstance(se.get("req"), dict):
            rec["req"] = se["req"]
        for key in OPTIONAL_LIST_FIELDS:
            v = en.get(key, se.get(key))
            if v is not None:
                rec[key] = v
        if "price" in en:
            rec["price"] = en["price"]
        if state_by_k is not None and k in state_by_k:
            st = state_by_k[k]
            for key in REQUIRED_STATE_FIELDS + OPTIONAL_STATE_FIELDS:
                if key in st and st[key] is not None:
                    rec[key] = st[key]
        records.append(rec)
    return records


def cmd_assemble(args):
    if not os.path.isfile(args.markers):
        sys.stdout.write("RED: PREREQ_RED 缺文件: %s\n" % args.markers)
        return 2
    try:
        with open(args.markers, "r", encoding="utf-8") as fh:
            events = _marker_events(fh.read().splitlines())
        state_by_k = load_state(args.state, allow_construct=getattr(args, "allow_construct_state", False)) if args.state else None
        recs = assemble(events, side=args.side, state_by_k=state_by_k)
        validate(recs, args.markers, args.side, require_state=False)
    except Prereq as exc:
        sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
        return 2
    missing_state = [r["k"] for r in recs if "quest_flag_before" not in r or "quest_flag_after" not in r]
    if args.out:
        with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
            for rec in recs:
                fh.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")
        sys.stdout.write("ASSEMBLED ok ops=%d out=%s state=%s\n"
                         % (len(recs), args.out, "yes" if state_by_k else "no"))
    else:
        for rec in recs:
            sys.stdout.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")
    if missing_state:
        sys.stdout.write("NOTE: %d/%d 条记录缺任务态（未给 --state）—— diff 阶段会 PREREQ\n"
                         % (len(missing_state), len(recs)))
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


def _frame_text_seq(frames, sm):
    return [f.get("text") for f in frames if f["sm"] == sm]


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


def compare(cs, rs, frame_scalars=None, text_sms=DEFAULT_TEXT_SMS, req_keys=REQ_COMPARE_KEYS):
    """返回 (diffs, stats)。diffs 为空 ⇒ GREEN。"""
    frame_scalars = frame_scalars or DEFAULT_FRAME_SCALAR_FIELDS
    diffs = []
    stats = {"ops": len(cs), "frames": 0, "ok": 0, "fail": 0, "quotes": 0}
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
        for fld in ("op", "npc", "map", "label", "branch"):
            if a.get(fld) != b.get(fld):
                diffs.append((k, fld, a.get(fld), b.get(fld)))
        # req：只比声明键
        ra = _norm_req(a.get("req") or {})
        rb = _norm_req(b.get("req") or {})
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
        # 帧种类序列（接/交/条件判定的身份）
        sa, sb = _frame_sm_seq(a.get("frames") or []), _frame_sm_seq(b.get("frames") or [])
        if sa != sb:
            diffs.append((k, "frames.sm", sa, sb))
        # 说词文本逐字节（默认 SM_MERCHANTSAY(643)）
        for sm in sorted(text_sms):
            va = _frame_text_seq(a.get("frames") or [], sm)
            vb = _frame_text_seq(b.get("frames") or [], sm)
            stats["quotes"] += len(va)
            if va != vb:
                diffs.append((k, "frames.text[%d]" % sm, va, vb))
        # 帧标量参数（声明的 (sm,字段)）
        for sm, fields in sorted(frame_scalars.items()):
            va = _frame_scalars(a.get("frames") or [], sm, fields)
            vb = _frame_scalars(b.get("frames") or [], sm, fields)
            if va != vb:
                diffs.append((k, "frames.scalar[%d]" % sm, va, vb))
        # 会话任务态位（逐字节 hex）
        for fld in REQUIRED_STATE_FIELDS + OPTIONAL_STATE_FIELDS:
            ha, hb = a.get(fld), b.get(fld)
            if ha is None and hb is None:
                continue
            na = _norm_hex(ha) if ha is not None else None
            nb = _norm_hex(hb) if hb is not None else None
            if na != nb:
                diffs.append((k, fld, na, nb))
        # 多重集：背包
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
        lines.append("QUEST %s RED %s csharp=%r rust=%r" % (loc, what, av, bv))
    lines.append(
        "STATS ops=%d frames=%d quotes=%d ok=%d fail=%d"
        % (stats["ops"], stats["frames"], stats["quotes"], stats["ok"], stats["fail"])
    )
    lines.append("CSHARP=%s" % cs_path)
    lines.append("RUST=%s" % rs_path)
    lines.append("DIFF %d" % len(diffs))
    return "\n".join(lines)


def cmd_diff(args):
    scalars = DEFAULT_FRAME_SCALAR_FIELDS
    if args.frame_scalars:
        try:
            raw = json.loads(args.frame_scalars)
            scalars = {int(k): list(v) for k, v in raw.items()}
        except (ValueError, TypeError) as exc:
            sys.stdout.write("RED: PREREQ_RED --frame-scalars 非法: %s\n" % exc)
            return 2
    text_sms = DEFAULT_TEXT_SMS
    if args.text_sms:
        try:
            text_sms = tuple(int(x.strip()) for x in args.text_sms.split(",") if x.strip() != "")
        except ValueError as exc:
            sys.stdout.write("RED: PREREQ_RED --text-sms 非法: %s\n" % exc)
            return 2
    try:
        cs = load_raw(args.csharp)
        rs = load_raw(args.rust)
        validate(cs, args.csharp, "csharp", require_state=True, text_sms=text_sms)
        validate(rs, args.rust, "rust", require_state=True, text_sms=text_sms)
    except Prereq as exc:
        sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
        return 2
    diffs, stats = compare(cs, rs, frame_scalars=scalars, text_sms=text_sms)
    sys.stdout.write(render(diffs, stats, args.csharp, args.rust) + "\n")
    if diffs:
        sys.stdout.write("RED: 任务跨侧差异 %d 项（定位见上）\n" % len(diffs))
        return 1
    sys.stdout.write(
        "GREEN: 任务逐项相同（ops=%d 帧=%d 说词=%d 失败分支=%d 任务态位逐字节相同）\n"
        % (stats["ops"], stats["frames"], stats["quotes"], stats["fail"])
    )
    return 0


# ---- selftest（fail-closed + 改坏必红） -------------------------------------


def _write_jsonl(path, records):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        for rec in records:
            fh.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")


def _zbytes(n=128):
    return "00" * n


def _base_records():
    return [
        {
            "k": 0,
            "op": "click_npc",
            "npc": "任务管理",
            "npc_actor": 101,
            "map": "3",
            "label": "@main",
            "req": {"item": "", "n": 0, "make": 0},
            "frames": [
                {"sm": 643, "text": "任务管理/任务名称 任务要求等级 任务可接次数"},
            ],
            "gold_before": 1000,
            "gold_after": 1000,
            "bag_before": ["木剑#43"],
            "bag_after": ["木剑#43"],
            "quest_flag_before": _zbytes(),
            "quest_flag_after": _zbytes(),
            "branch": "ok",
        },
        {
            "k": 1,
            "op": "dialog_select",
            "npc": "任务管理",
            "npc_actor": 101,
            "map": "3",
            "label": "@rw06",
            "req": {"item": "", "n": 0, "make": 0},
            "frames": [
                {"sm": 643, "text": "任务管理/确定开始[神秘洞口]任务吗？"},
            ],
            "gold_before": 1000,
            "gold_after": 1000,
            "bag_before": ["木剑#43"],
            "bag_after": ["木剑#43"],
            "quest_flag_before": _zbytes(),
            "quest_flag_after": _zbytes(),
            "branch": "ok",
        },
        {
            "k": 2,
            "op": "dialog_select",
            "npc": "任务管理",
            "npc_actor": 101,
            "map": "3",
            "label": "@06rw",
            "req": {"item": "", "n": 0, "make": 0},
            "frames": [
                {"sm": 643, "text": "任务管理/苍月岛的首饰店老板正在找人帮忙"},
            ],
            "gold_before": 1000,
            "gold_after": 1000,
            "bag_before": ["木剑#43"],
            "bag_after": ["木剑#43"],
            # set [107] 1 ⇒ idx 13（0 起），bit = 128>>(107-1)%8 = 32 ⇒ 第 13 字节 0x20
            "quest_flag_before": _zbytes(),
            "quest_flag_after": "00" * 13 + "20" + "00" * 114,
            "branch": "ok",
        },
    ]


def _state_records_from(records):
    """把基准记录的 quest_* 抽成服务端 dump 行（供 assemble --state 自检）。"""
    out = []
    for rec in records:
        row = {"k": rec["k"]}
        for key in REQUIRED_STATE_FIELDS + OPTIONAL_STATE_FIELDS:
            if key in rec:
                row[key] = rec[key]
        out.append(row)
    return out


def _run_diff(cs_path, rs_path, **kw):
    return cmd_diff(argparse.Namespace(csharp=cs_path, rust=rs_path, frame_scalars=None,
                                       text_sms=",".join(str(x) for x in kw.get("text_sms", DEFAULT_TEXT_SMS))))


def cmd_selftest(args):
    tmp = tempfile.mkdtemp(prefix="quest-selftest-")
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
        rec = _base_records(); rec[1]["op"] = "take"
        _write_jsonl(p, rec)
        case("mutate-op-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 npc 名
        p = os.path.join(tmp, "rs_npc.jsonl")
        rec = _base_records(); rec[0]["npc"] = "首饰店老板"
        _write_jsonl(p, rec)
        case("mutate-npc-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 label
        p = os.path.join(tmp, "rs_label.jsonl")
        rec = _base_records(); rec[1]["label"] = "@05rw"
        _write_jsonl(p, rec)
        case("mutate-label-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 map
        p = os.path.join(tmp, "rs_map.jsonl")
        rec = _base_records(); rec[0]["map"] = "0"
        _write_jsonl(p, rec)
        case("mutate-map-red", 1, lambda: _run_diff(cs0, p))

        # 绿：req 里**不**参与判据的动态键 / npc_actor 不同 ⇒ 仍绿
        p = os.path.join(tmp, "rs_dyn.jsonl")
        rec = _base_records(); rec[0]["req"]["make"] = 999; rec[0]["npc_actor"] = 777
        _write_jsonl(p, rec)
        case("dynamic-id-ignored-green", 0, lambda: _run_diff(cs0, p))

        # 红：改帧种类序列
        p = os.path.join(tmp, "rs_sm.jsonl")
        rec = _base_records(); rec[1]["frames"] = [{"sm": 644, "a": 0}]
        _write_jsonl(p, rec)
        case("mutate-frame-kind-red", 1, lambda: _run_diff(cs0, p))

        # 红：改说词文本（逐字节 —— 少一个空格都必须红）
        p = os.path.join(tmp, "rs_text.jsonl")
        rec = _base_records(); rec[2]["frames"][0]["text"] = "任务管理/苍月岛的首饰店老板正在找人帮忙 "
        _write_jsonl(p, rec)
        case("mutate-frame-text-red", 1, lambda: _run_diff(cs0, p))

        # 红：改帧标量参数（需声明 (sm,字段)；用 --frame-scalars 显式打开）
        p = os.path.join(tmp, "rs_scalar.jsonl")
        rec = _base_records(); rec[0]["frames"][0]["a"] = 7
        _write_jsonl(p, rec)

        def _scalar_diff():
            return cmd_diff(argparse.Namespace(csharp=cs0, rust=p,
                                               frame_scalars='{"643":["a"]}', text_sms="643"))

        case("mutate-frame-scalar-red", 1, _scalar_diff)

        # 红：改 price
        p = os.path.join(tmp, "rs_price.jsonl")
        rec = _base_records(); rec[0]["price"] = 11
        _write_jsonl(p, rec)
        case("mutate-price-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 gold_after
        p = os.path.join(tmp, "rs_gold.jsonl")
        rec = _base_records(); rec[0]["gold_after"] = 1001
        _write_jsonl(p, rec)
        case("mutate-gold-red", 1, lambda: _run_diff(cs0, p))

        # 红：改背包多重集（同元素个数不同 ⇒ 计数式必须咬住）
        p = os.path.join(tmp, "rs_bag.jsonl")
        rec = _base_records(); rec[0]["bag_after"] = ["木剑#43", "木剑#43"]
        _write_jsonl(p, rec)
        case("mutate-bag-multiset-red", 1, lambda: _run_diff(cs0, p))

        # 红：改任务态位（只差一位 ⇒ 必红）
        p = os.path.join(tmp, "rs_qflag.jsonl")
        rec = _base_records(); rec[2]["quest_flag_after"] = "00" * 13 + "10" + "00" * 114
        _write_jsonl(p, rec)
        case("mutate-quest-flag-bit-red", 1, lambda: _run_diff(cs0, p))

        # 红：任务态 hex 大小写不同 ⇒ 视为相同（规范化）—— 绿
        p = os.path.join(tmp, "rs_qflag_case.jsonl")
        rec = _base_records(); rec[2]["quest_flag_after"] = ("00" * 13 + "20" + "00" * 114).upper()
        _write_jsonl(p, rec)
        case("quest-flag-case-insensitive-green", 0, lambda: _run_diff(cs0, p))

        # 红：一侧有 quest_unit、另一侧没有 ⇒ 判红（不许静默吞）
        p = os.path.join(tmp, "rs_qunit.jsonl")
        rec = _base_records(); rec[0]["quest_unit_before"] = _zbytes(); rec[0]["quest_unit_after"] = _zbytes()
        _write_jsonl(p, rec)
        case("mutate-quest-unit-presence-red", 1, lambda: _run_diff(cs0, p))

        # 红：改 branch（ok ↔ 失败分支不能顶替）
        p = os.path.join(tmp, "rs_branch.jsonl")
        rec = _base_records(); rec[2]["branch"] = "fail_level"
        _write_jsonl(p, rec)
        case("mutate-branch-red", 1, lambda: _run_diff(cs0, p))

        # 红：记录条数不同
        p = os.path.join(tmp, "rs_len.jsonl")
        _write_jsonl(p, _base_records()[:2])
        case("mutate-op-count-red", 1, lambda: _run_diff(cs0, p))

        # 红：不得 i32 截断（只在高 32 位不同的金币必须判红）
        cs_big = os.path.join(tmp, "cs_big.jsonl")
        rs_big = os.path.join(tmp, "rs_big.jsonl")
        rec = _base_records(); rec[0]["gold_after"] = 3000000000
        _write_jsonl(cs_big, rec)
        rec2 = _base_records(); rec2[0]["gold_after"] = 3000000000 + (1 << 32)
        _write_jsonl(rs_big, rec2)
        case("i32-truncation-red", 1, lambda: _run_diff(cs_big, rs_big))

        # PREREQ：缺文件
        case("missing-file-prereq", 2,
             lambda: _run_diff(os.path.join(tmp, "nope.jsonl"), rs0))

        # PREREQ：缺结构字段
        p = os.path.join(tmp, "rs_missing.jsonl")
        rec = _base_records(); del rec[0]["branch"]
        _write_jsonl(p, rec)
        case("missing-field-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：缺任务态字段（没有服务端 dump ⇒ fail-closed，绝不 GREEN）
        p = os.path.join(tmp, "rs_nostate.jsonl")
        rec = _base_records(); del rec[1]["quest_flag_before"]; del rec[1]["quest_flag_after"]
        _write_jsonl(p, rec)
        case("missing-quest-state-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：非法 hex
        p = os.path.join(tmp, "rs_badhex.jsonl")
        rec = _base_records(); rec[0]["quest_flag_after"] = "zz"
        _write_jsonl(p, rec)
        case("invalid-state-hex-prereq", 2, lambda: _run_diff(cs0, p))

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
        rec = _base_records(); rec[0]["frames"] = {"sm": 643}
        _write_jsonl(p, rec)
        case("invalid-frames-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：文本帧缺 text
        p = os.path.join(tmp, "rs_notext.jsonl")
        rec = _base_records(); del rec[0]["frames"][0]["text"]
        _write_jsonl(p, rec)
        case("missing-frame-text-prereq", 2, lambda: _run_diff(cs0, p))

        # PREREQ：空文件
        p = os.path.join(tmp, "rs_empty.jsonl")
        open(p, "w", encoding="utf-8", newline="\n").close()
        case("empty-file-prereq", 2, lambda: _run_diff(cs0, p))

        # ---- assemble 子命令（BotSrv 事件流 + 服务端 dump → 记录）----
        base = _base_records()
        markers = os.path.join(tmp, "markers.jsonl")
        evs = [{"phase": "arm", "steps": 3, "step_ms": 1000}]
        for rec in base:
            se = {"phase": "send", "k": rec["k"], "op": rec["op"], "npc": rec["npc"],
                  "npc_actor": rec["npc_actor"], "map": rec["map"], "label": rec["label"],
                  "req": rec["req"], "gold_before": rec["gold_before"], "bag_before": rec["bag_before"]}
            evs.append(se)
            for fr in rec["frames"]:
                evs.append({"phase": "frame", "k": rec["k"], "frame": fr})
            evs.append({"phase": "end", "k": rec["k"], "gold_after": rec["gold_after"],
                        "bag_after": rec["bag_after"], "branch": rec["branch"]})
        evs.append({"phase": "done", "steps": len(base)})
        with open(markers, "w", encoding="utf-8", newline="\n") as fh:
            for ev in evs:
                fh.write("robot says: " + "M3QUESTSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
        state = os.path.join(tmp, "state.jsonl")
        _write_jsonl(state, _state_records_from(base))
        out = os.path.join(tmp, "assembled.jsonl")

        def _assemble():
            return cmd_assemble(argparse.Namespace(markers=markers, out=out, side="csharp", state=state))

        case("assemble-ok", 0, _assemble)

        # assemble 出来的记录必须与基准记录同形（sort_keys 后逐字节相同）→ diff 绿
        def _assemble_then_diff():
            rc = _assemble()
            if rc != 0:
                return rc
            return _run_diff(cs0, out)

        case("assemble-then-diff-green", 0, _assemble_then_diff)

        # assemble 不带 --state ⇒ 成功但记录缺任务态；再 diff ⇒ PREREQ（fail-closed）
        out_nostate = os.path.join(tmp, "assembled_nostate.jsonl")

        def _assemble_nostate():
            return cmd_assemble(argparse.Namespace(markers=markers, out=out_nostate, side="rust", state=""))

        case("assemble-without-state-ok", 0, _assemble_nostate)
        # T-58：构造级任务态默认拒收（fail-closed，绝不把「按场景推的位图」当服务端真值判绿）；
        # 显式 --allow-construct-state（离线自检）才接受。
        state_construct = os.path.join(tmp, "state_construct.jsonl")
        with open(state_construct, "w", encoding="utf-8", newline=chr(10)) as fh:
            for i0 in range(len(base)):
                fh.write(json.dumps({"k": i0, "quest_flag_before": base[i0]["quest_flag_before"],
                                     "quest_flag_after": base[i0]["quest_flag_after"],
                                     "construct_level": True}, ensure_ascii=False, sort_keys=True))
                fh.write(chr(10))
        case("assemble-construct-state-rejected", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers, out="", side="csharp",
                                                     state=state_construct)))

        def _assemble_construct_ok():
            return cmd_assemble(argparse.Namespace(markers=markers, out="", side="csharp",
                                                   state=state_construct, allow_construct_state=True))
        case("assemble-construct-state-allowed", 0, _assemble_construct_ok)

        def _nostate_then_diff():
            rc = _assemble_nostate()
            if rc != 0:
                return rc
            return _run_diff(cs0, out_nostate)

        case("assemble-without-state-then-diff-prereq", 2, _nostate_then_diff)

        # assemble：有 send 无 end ⇒ PREREQ（交互未收尾，fail-closed）
        markers_missing_end = os.path.join(tmp, "markers_noend.jsonl")
        with open(markers_missing_end, "w", encoding="utf-8", newline="\n") as fh:
            for ev in evs[:-2]:  # 去掉最后 end + done
                fh.write("M3QUESTSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
        case("assemble-missing-end-prereq", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers_missing_end, out="", side="csharp", state=state)))

        # assemble：无事件 ⇒ PREREQ
        markers_empty = os.path.join(tmp, "markers_empty.jsonl")
        with open(markers_empty, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("no markers here\n")
        case("assemble-no-events-prereq", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers_empty, out="", side="csharp", state=state)))

        # assemble：done.steps 与 send 数不一致 ⇒ PREREQ
        markers_baddone = os.path.join(tmp, "markers_baddone.jsonl")
        with open(markers_baddone, "w", encoding="utf-8", newline="\n") as fh:
            for ev in evs:
                if ev.get("phase") == "done":
                    ev = {"phase": "done", "steps": 9}
                fh.write("M3QUESTSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
        case("assemble-done-count-prereq", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers_baddone, out="", side="csharp", state=state)))

        # assemble：缺 --state 文件 ⇒ PREREQ
        case("assemble-missing-state-prereq", 2,
             lambda: cmd_assemble(argparse.Namespace(markers=markers, out="", side="csharp",
                                                     state=os.path.join(tmp, "nope-state.jsonl"))))
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
    ap = argparse.ArgumentParser(description="M3⑥ 任务跨侧比较器（夹具骨架）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("assemble", help="BotSrv M3QUESTSEQ 事件流 (+ 服务端 dump) → quest_raw.jsonl")
    a.add_argument("markers")
    a.add_argument("--out", default="")
    a.add_argument("--state", default="", help="服务端会话任务态 dump（JSONL，每行含整数 k + quest_*_before/after）")
    a.add_argument("--side", default="?")
    a.add_argument("--allow-construct-state", dest="allow_construct_state", action="store_true",
                   help="离线自检用：接受 construct_level:true 的构造级任务态（默认拒收，fail-closed）")
    a.set_defaults(func=cmd_assemble)
    d = sub.add_parser("diff", help="比较两侧 quest_raw.jsonl")
    d.add_argument("csharp")
    d.add_argument("rust")
    d.add_argument("--frame-scalars", default="",
                   help='可选 JSON: {"643":["a"],...} 覆盖默认标量帧参数表')
    d.add_argument("--text-sms", default="",
                   help="可选：参与说词逐字节比对的说词帧 sm 列表（默认 643）")
    d.set_defaults(func=cmd_diff)
    s = sub.add_parser("selftest", help="fail-closed + 改坏必红自检")
    s.set_defaults(func=cmd_selftest)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
