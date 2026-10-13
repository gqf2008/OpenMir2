#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""party_trade_diff.py —— M3⑦ 组队（party）/ 交易（trade）跨侧对拍比较器（夹具骨架）。

卡：`openmir2-m3-party-trade-fixture-botsrv`（M3 逐系统对齐 ⑦ 的承接卡 5；父卡 `openmir2-m3-party-trade-align`）。
本文件是**判据本体**，只做离线比较：读两侧各自的 `party_trade_raw.jsonl`（每条 = 一个假人的一步交互），
逐 (bot, k) 比较「op / 角色 / 是否执行 / 参数 / 帧种类序列 / 声明的标量帧参数 / 文本帧逐字节 /
金币 / 背包多重集 / 归属·分配扩展位 / 失败分支」。真机跑由 `run_party_trade_pair.ps1`
（等 ⑦-2 `openmir2-m3-party-group-state` + ⑦-3 `openmir2-m3-party-trade-state` 落地 + owner 发窗口）驱动。

口径（与卡内 `machine_checkable_acceptance` ③ 对齐，逐条对齐盘点报告 `M3⑦组队交易现状盘点报告-20261013.md` §4.1）：

1. **fail-closed**：缺文件 / 缺字段 / 字段类型错 / 非法 JSON / k 不连续 / 记录非对象 /
   bot 集合两侧不一致 ⇒ 一律非零退出并点名（`PREREQ_RED`），**绝不静默 GREEN**。
2. **组队/交易状态机一致（帧证据）**：帧**种类序列**（`sm` id 顺序）严格逐项比；这是「建队/加人/退队/散组」
   与「发起/放入/锁定/确认/取消」的直接证据。动态 id（actor / make index）**不**作判据（跨运行本来就不同）。
3. **两假人（两人及以上）**：本比较器按 **(bot, k)** 对齐——两侧必须有**同一组 bot**、每个 bot 有**同一串 k**；
   某侧少一个 bot / 某 bot 少一步 ⇒ 判红。这是 §6「两人及以上」判据的机器化落点。
4. **分配一致（可选强判据）**：`exp_before/after`、`level_before/after`（组队 `GainExp` 分配）与
   `pickup_ok`（拾取归属 `is_of_group`）若一侧有另一侧没有 ⇒ 判红（不许静默吞）；`bag_*` 一律按
   **多重集**（`Counter`）比，**不做 set 差集**。
5. **金币原宽度**：`gold_before`/`gold_after` 按原始整数比（不得 i32 截断）。
6. **落库口径（= oracle「组队/交易态不落库」）**：本比较器**不**判「重登后组队/交易态仍在」——oracle
   MySQL 本身不持久组队/交易态（盘点报告 §1.M）。DB 侧判据由 `scoped_diff.py --canonical` 在夹具入口里做
   （`characters_bagitem`/`Gold` = 交易副产物；`characters` 无组队/交易专列），**不要**在记录里造跨重登保留。
7. **改坏必红**：`selftest` 对「op / bot / k / 帧种类 / 帧文本 / 帧标量 / 金币 / 背包 / exp / branch /
   记录数 / bot 集合」逐个改坏 ⇒ 必红。

记录 schema（每行一个 JSON 对象；`party_trade_scenario.json` 里也写一份）：

    {
      "bot": "m3p1230",          # 假人角色名（ChrName = LoginAccount+序号；跨侧必须相同）
      "k": 0,                    # 交互序号（0 起，每个 bot 内连续）
      "role": "a",               # 步骤声明的角色：a=actor / b=peer / x=both
      "op": "group_mode_on",     # 见 OPS
      "ran": true,               # 本 bot 是否实际执行本步（false = 角色不符，跳过）
      "param": "peer",           # 步骤参数（peer 记号 / 金币数 / 物品名；原样比）
      "frames": [                # 本步收到的相关 SM_* 帧（有序）
        {"sm": 660}
      ],
      "gold_before": 1000, "gold_after": 1000,
      "bag_before": ["木剑#43"], "bag_after": ["木剑#43"],
      "exp_before": 0, "exp_after": 0,          # 可选：组队经验分配（一侧有另一侧没有 ⇒ 判红）
      "level_before": 1, "level_after": 1,      # 可选：等级
      "pickup_ok": true,                        # 可选：归属判定（is_self||is_of_group）
      "branch": "ok"                            # ok | fail_reject | skip_other_role |
                                                # fail_no_frame | frames_unclassified
    }

退出码：0 = GREEN（逐项相同）；1 = RED（有差异）；2 = PREREQ（缺件/坏件，fail-closed）。

子命令：
  assemble <markers.jsonl> --out <raw.jsonl> [--side ?]
      把 BotSrv 的 `M3PAIRTRADESEQ {json}` 事件流（phase=arm/send/frame/end/done）按 (bot, k) 合并成记录。
  diff <csharp_raw.jsonl> <rust_raw.jsonl> [--frame-scalars JSON] [--text-sms 101[,..]]
      逐 (bot, k) 比较两侧记录。
  selftest
      fail-closed + 改坏必红（离线）。
"""

import argparse
import collections
import json
import os
import sys

# ---- schema ----------------------------------------------------------------

# 步骤声明的角色：a=actor（PairTradeRole） / b=peer（PairTradePeerRole） / x=both。
ROLES = ("a", "b", "x")

OPS = (
    # 组队（PlayObject.Message.cs:914-935 / PlayObject.Operate.cs:787-909）
    "group_mode_on", "group_mode_off", "create_group", "add_member", "del_member",
    # 交易（PlayObject.Message.cs:946-960 / PlayObject.Operate.cs:911-1203）
    "deal_try", "deal_add_item", "deal_del_item", "deal_chg_gold", "deal_end", "deal_cancel",
    # 掉线中断（CM_SOFTCLOSE(1009) = 小退 ⇒ 走 WorldServer 断线路径 DealCancelA/DelMember）
    "softclose",
)

# 结构必填。
REQUIRED_FIELDS = ("bot", "k", "op", "ran", "frames", "gold_before", "gold_after", "branch")
# 可选列表（多重集比）。
OPTIONAL_LIST_FIELDS = ("bag_before", "bag_after")
# 可选整数（原宽度比；一侧有另一侧没有 ⇒ 判红）。
OPTIONAL_INT_FIELDS = ("exp_before", "exp_after", "level_before", "level_after")
# 可选布尔（一侧有另一侧没有 ⇒ 判红）。
OPTIONAL_BOOL_FIELDS = ("pickup_ok",)
FRAME_FIELDS = ("sm", "a", "b", "c", "d")

# 需要逐字节比对**文本**的 SM（默认 `SM_GROUPMESSAGE(101)` = 组队广播；窗口期可按真机实测补）。
DEFAULT_TEXT_SMS = (101,)

# 场景声明的**标量帧参数**：只有这些 (sm, 参数名) 参与判据。默认空；窗口期可按真机实测补/删
# （改场景即改判据）。强判据已经是「sm 种类序列 + branch + 金币/背包」。
DEFAULT_FRAME_SCALAR_FIELDS = {}


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


def _validate_frames(frames, side, bot, k, path, text_sms):
    if not isinstance(frames, list):
        raise Prereq("%s: bot=%s k=%s frames 非数组 (%s)" % (side, bot, k, path))
    for i, fr in enumerate(frames):
        if not isinstance(fr, dict):
            raise Prereq("%s: bot=%s k=%s frame[%d] 非对象 (%s)" % (side, bot, k, i, path))
        if "sm" not in fr:
            raise Prereq("%s: bot=%s k=%s frame[%d] 缺 sm (%s)" % (side, bot, k, i, path))
        if not _is_int(fr["sm"]):
            raise Prereq("%s: bot=%s k=%s frame[%d] sm 非整数 (%s)" % (side, bot, k, i, path))
        if fr["sm"] in text_sms:
            if "text" not in fr:
                raise Prereq("%s: bot=%s k=%s frame[%d] sm=%s 是文本帧但缺 text (%s)"
                             % (side, bot, k, i, fr["sm"], path))
            if not isinstance(fr["text"], str):
                raise Prereq("%s: bot=%s k=%s frame[%d] text 非字符串 (%s)" % (side, bot, k, i, path))
        for fld in FRAME_FIELDS[1:]:
            if fld in fr and not _is_int(fr[fld]):
                raise Prereq("%s: bot=%s k=%s frame[%d] %s 非整数 (%s)" % (side, bot, k, i, fld, path))


def validate(raw, path, side, text_sms=DEFAULT_TEXT_SMS):
    """逐条校验 schema；任何缺失/类型错误 ⇒ Prereq（点名 side/bot/k/字段）。

    另外强制：每个 bot 的 k 必须 0 起连续（组队/交易序列不完整 ⇒ fail-closed）。
    """
    if not raw:
        raise Prereq("%s: 无记录 (%s)" % (side, path))
    per_bot = collections.defaultdict(list)
    for i, rec in enumerate(raw):
        if not isinstance(rec, dict):
            raise Prereq("%s: 第 %d 条非对象 (%s)" % (side, i, path))
        for field in ("bot", "k"):
            if field not in rec:
                raise Prereq("%s: 第 %d 条缺字段 %s (%s)" % (side, i, field, path))
        if not isinstance(rec["bot"], str) or rec["bot"] == "":
            raise Prereq("%s: 第 %d 条 bot 非空字符串 (%s)" % (side, i, path))
        if not _is_int(rec["k"]):
            raise Prereq("%s: bot=%s k 非整数（第 %d 条=%r）(%s)" % (side, rec["bot"], i, rec["k"], path))
        per_bot[rec["bot"]].append(rec["k"])
    for bot, ks in per_bot.items():
        if sorted(ks) != list(range(len(ks))):
            raise Prereq("%s: bot=%s 的 k 不连续/非 0 起: %r (%s)" % (side, bot, sorted(ks), path))
    for rec in raw:
        bot, k = rec["bot"], rec["k"]
        for field in REQUIRED_FIELDS:
            if field not in rec:
                raise Prereq("%s: bot=%s k=%s 缺字段 %s (%s)" % (side, bot, k, field, path))
        if rec["op"] not in OPS:
            raise Prereq("%s: bot=%s k=%s op 非法 %r（允许 %s）(%s）"
                         % (side, bot, k, rec["op"], "/".join(OPS), path))
        if not isinstance(rec["ran"], bool):
            raise Prereq("%s: bot=%s k=%s ran 非布尔 (%s)" % (side, bot, k, path))
        if "role" in rec and rec["role"] not in ROLES:
            raise Prereq("%s: bot=%s k=%s role 非法 %r (%s)" % (side, bot, k, rec["role"], path))
        if "param" in rec and rec["param"] is not None and not isinstance(rec["param"], str):
            raise Prereq("%s: bot=%s k=%s param 非字符串/null (%s)" % (side, bot, k, path))
        if "map" in rec and not isinstance(rec["map"], str):
            raise Prereq("%s: bot=%s k=%s map 非字符串 (%s)" % (side, bot, k, path))
        if not isinstance(rec["branch"], str) or rec["branch"] == "":
            raise Prereq("%s: bot=%s k=%s branch 非空字符串 (%s)" % (side, bot, k, path))
        for fld in ("gold_before", "gold_after"):
            if not _is_int(rec[fld]):
                raise Prereq("%s: bot=%s k=%s 字段 %s 非整数 (%s)" % (side, bot, k, fld, path))
        for fld in OPTIONAL_INT_FIELDS:
            if fld in rec and rec[fld] is not None and not _is_int(rec[fld]):
                raise Prereq("%s: bot=%s k=%s 字段 %s 非整数/null (%s)" % (side, bot, k, fld, path))
        for fld in OPTIONAL_BOOL_FIELDS:
            if fld in rec and rec[fld] is not None and not isinstance(rec[fld], bool):
                raise Prereq("%s: bot=%s k=%s 字段 %s 非布尔/null (%s)" % (side, bot, k, fld, path))
        _validate_frames(rec["frames"], side, bot, k, path, text_sms)
        for fld in OPTIONAL_LIST_FIELDS:
            if fld in rec and rec[fld] is not None and not isinstance(rec[fld], list):
                raise Prereq("%s: bot=%s k=%s 字段 %s 非数组 (%s)" % (side, bot, k, fld, path))


# ---- assemble（BotSrv M3PAIRTRADESEQ 事件流 → 记录） -------------------------


def _marker_events(lines):
    """从文本行里抽出 `M3PAIRTRADESEQ {json}` 事件（保持顺序）。非法 JSON ⇒ Prereq。"""
    out = []
    for lineno, line in enumerate(lines, 1):
        idx = line.find("M3PAIRTRADESEQ ")
        if idx < 0:
            continue
        payload = line[idx + len("M3PAIRTRADESEQ "):].strip()
        try:
            ev = json.loads(payload)
        except json.JSONDecodeError as exc:
            raise Prereq("M3PAIRTRADESEQ 事件非法 JSON（第 %d 行）: %s" % (lineno, exc))
        if not isinstance(ev, dict):
            raise Prereq("M3PAIRTRADESEQ 事件非对象（第 %d 行）" % lineno)
        out.append(ev)
    if not out:
        raise Prereq("无 M3PAIRTRADESEQ 事件（驱动未生效/日志未含标记）")
    return out


def assemble(events, side="?"):
    """把 arm/send/frame/end/done 事件流按 (bot, k) 合并成记录列表（fail-closed）。"""
    sends = {}
    ends = {}
    frames = collections.defaultdict(list)
    order = []           # (bot, k) 首次 send 的顺序
    bots = []            # bot 首次出现顺序
    done = collections.defaultdict(int)
    for ev in events:
        phase = ev.get("phase")
        if phase == "arm":
            bot = ev.get("bot")
            if isinstance(bot, str) and bot not in bots:
                bots.append(bot)
            continue
        if phase == "done":
            bot = ev.get("bot")
            if isinstance(bot, str):
                n = ev.get("steps")
                if _is_int(n):
                    done[bot] = n
            continue
        if phase in ("send", "frame", "end"):
            bot = ev.get("bot")
            if not isinstance(bot, str) or bot == "":
                raise Prereq("%s: %s 事件缺 bot 字符串: %r" % (side, phase, ev))
            k = ev.get("k")
            if not _is_int(k):
                raise Prereq("%s: bot=%s %s 事件缺整数 k: %r" % (side, bot, phase, ev))
            if bot not in bots:
                bots.append(bot)
            if phase == "send":
                key = (bot, k)
                if key in sends:
                    raise Prereq("%s: bot=%s k=%d 重复 send 事件" % (side, bot, k))
                sends[key] = ev
                order.append(key)
            elif phase == "frame":
                if "frame" not in ev or not isinstance(ev["frame"], dict):
                    raise Prereq("%s: bot=%s k=%d frame 事件缺 frame 对象" % (side, bot, k))
                frames[(bot, k)].append(ev["frame"])
            else:  # end
                key = (bot, k)
                if key in ends:
                    raise Prereq("%s: bot=%s k=%d 重复 end 事件" % (side, bot, k))
                ends[key] = ev
            continue
        raise Prereq("%s: 未知 phase=%r" % (side, phase))
    if not sends:
        raise Prereq("%s: 无 send 事件（序列没跑）" % side)
    for bot, n in done.items():
        got = sum(1 for (b, _k) in sends if b == bot)
        if n != got:
            raise Prereq("%s: bot=%s done.steps=%d 与 send 事件数 %d 不一致" % (side, bot, n, got))
    # 按 bot 首次出现顺序 + 各 bot k 升序
    records = []
    for bot in bots:
        for k in range(len([1 for (b, _k) in order if b == bot])):
            key = (bot, k)
            if key not in sends:
                raise Prereq("%s: bot=%s k=%d 有 end/frame 无 send（序列断档）" % (side, bot, k))
            if key not in ends:
                raise Prereq("%s: bot=%s k=%d 有 send 无 end（交互未收尾）—— fail-closed" % (side, bot, k))
            se = sends[key]
            en = ends[key]
            rec = {
                "bot": bot,
                "k": k,
                "op": se.get("op"),
                "ran": se.get("ran"),
                "frames": frames.get(key, []),
                "gold_before": se.get("gold_before"),
                "gold_after": en.get("gold_after"),
                "branch": en.get("branch"),
            }
            for fld in ("role", "param", "map"):
                if fld in se:
                    rec[fld] = se[fld]
            for fld in OPTIONAL_LIST_FIELDS:
                v = en.get(fld, se.get(fld))
                if v is not None:
                    rec[fld] = v
            for fld in OPTIONAL_INT_FIELDS + OPTIONAL_BOOL_FIELDS:
                v = en.get(fld, se.get(fld))
                if v is not None:
                    rec[fld] = v
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
        sys.stdout.write("ASSEMBLED ok records=%d bots=%d out=%s\n"
                         % (len(recs), len(set(r["bot"] for r in recs)), args.out))
    else:
        for rec in recs:
            sys.stdout.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")
    return 0


# ---- comparison ------------------------------------------------------------


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


def _by_bot_k(records):
    out = {}
    for rec in records:
        out[(rec["bot"], rec["k"])] = rec
    return out


def compare(cs, rs, frame_scalars=None, text_sms=DEFAULT_TEXT_SMS,
            optional_int_fields=OPTIONAL_INT_FIELDS, optional_bool_fields=OPTIONAL_BOOL_FIELDS):
    """返回 (diffs, stats)。diffs 为空 ⇒ GREEN。"""
    frame_scalars = frame_scalars or DEFAULT_FRAME_SCALAR_FIELDS
    diffs = []
    stats = {"records": len(cs), "bots": len(set(r["bot"] for r in cs)),
             "frames": 0, "ok": 0, "fail": 0, "texts": 0}
    cbots = sorted(set(r["bot"] for r in cs))
    rbots = sorted(set(r["bot"] for r in rs))
    if cbots != rbots:
        diffs.append((None, None, "bots", cbots, rbots))
        return diffs, stats
    a_map, b_map = _by_bot_k(cs), _by_bot_k(rs)
    if len(cs) != len(rs):
        diffs.append((None, None, "记录条数", len(cs), len(rs)))
        return diffs, stats
    # 每个 bot 的 k 集合必须相同
    for bot in cbots:
        ck = sorted(k for (b, k) in a_map if b == bot)
        rk = sorted(k for (b, k) in b_map if b == bot)
        if ck != rk:
            diffs.append((bot, None, "k 集合", ck, rk))
    for (bot, k) in sorted(a_map):
        a = a_map.get((bot, k))
        b = b_map.get((bot, k))
        if a is None or b is None:
            continue
        stats["frames"] += len(a.get("frames") or [])
        if a.get("branch") == "ok":
            stats["ok"] += 1
        elif a.get("branch") == "skip_other_role":
            pass
        else:
            stats["fail"] += 1
        for fld in ("op", "role", "ran", "param", "branch"):
            if a.get(fld) != b.get(fld):
                diffs.append((bot, k, fld, a.get(fld), b.get(fld)))
        # 金币（原宽度整数）
        for fld in ("gold_before", "gold_after"):
            if a.get(fld) != b.get(fld):
                diffs.append((bot, k, fld, a.get(fld), b.get(fld)))
        # 帧种类序列（状态机身份）
        sa, sb = _frame_sm_seq(a.get("frames") or []), _frame_sm_seq(b.get("frames") or [])
        if sa != sb:
            diffs.append((bot, k, "frames.sm", sa, sb))
        # 文本帧逐字节
        for sm in sorted(text_sms):
            va = _frame_text_seq(a.get("frames") or [], sm)
            vb = _frame_text_seq(b.get("frames") or [], sm)
            stats["texts"] += len(va)
            if va != vb:
                diffs.append((bot, k, "frames.text[%d]" % sm, va, vb))
        # 帧标量参数（声明的 (sm,字段)）
        for sm, fields in sorted(frame_scalars.items()):
            va = _frame_scalars(a.get("frames") or [], sm, fields)
            vb = _frame_scalars(b.get("frames") or [], sm, fields)
            if va != vb:
                diffs.append((bot, k, "frames.scalar[%d]" % sm, va, vb))
        # 可选整数（exp/level；一侧有另一侧没有 ⇒ 判红）
        for fld in optional_int_fields:
            if a.get(fld) != b.get(fld):
                diffs.append((bot, k, fld, a.get(fld), b.get(fld)))
        # 可选布尔（归属）
        for fld in optional_bool_fields:
            if a.get(fld) != b.get(fld):
                diffs.append((bot, k, fld, a.get(fld), b.get(fld)))
        # 多重集：背包
        for fld in OPTIONAL_LIST_FIELDS:
            ma, mb = _multiset(a.get(fld) or []), _multiset(b.get(fld) or [])
            if ma != mb:
                missing = dict((str(x), v) for x, v in (ma - mb).items())
                extra = dict((str(x), v) for x, v in (mb - ma).items())
                diffs.append((bot, k, "%s.multiset" % fld, missing, extra))
    return diffs, stats


def render(diffs, stats, cs_path, rs_path):
    lines = []
    for bot, k, what, av, bv in diffs:
        loc = "bot=%s k=%s" % ("-" if bot is None else bot, "-" if k is None else k)
        lines.append("PAIRTRADE %s RED %s csharp=%r rust=%r" % (loc, what, av, bv))
    lines.append(
        "STATS records=%d bots=%d frames=%d texts=%d ok=%d fail=%d"
        % (stats["records"], stats["bots"], stats["frames"], stats["texts"], stats["ok"], stats["fail"]))
    lines.append("CROSS csharp=%s rust=%s" % (cs_path, rs_path))
    return lines


def cmd_diff(args):
    frame_scalars = dict(DEFAULT_FRAME_SCALAR_FIELDS)
    if args.frame_scalars:
        try:
            frame_scalars = json.loads(args.frame_scalars)
        except json.JSONDecodeError as exc:
            sys.stdout.write("RED: PREREQ_RED --frame-scalars 非法 JSON: %s\n" % exc)
            return 2
        if not isinstance(frame_scalars, dict):
            sys.stdout.write("RED: PREREQ_RED --frame-scalars 必须是对象 {sm: [fields]}\n")
            return 2
        frame_scalars = dict((int(k), list(v)) for k, v in frame_scalars.items())
    text_sms = DEFAULT_TEXT_SMS
    if args.text_sms is not None:
        try:
            text_sms = tuple(int(x) for x in args.text_sms.split(",") if x.strip() != "")
        except ValueError:
            sys.stdout.write("RED: PREREQ_RED --text-sms 必须逗号分隔整数\n")
            return 2
    try:
        cs = load_raw(args.csharp)
        rs = load_raw(args.rust)
        validate(cs, args.csharp, "csharp", text_sms=text_sms)
        validate(rs, args.rust, "rust", text_sms=text_sms)
    except Prereq as exc:
        sys.stdout.write("RED: PREREQ_RED %s\n" % exc)
        return 2
    diffs, stats = compare(cs, rs, frame_scalars=frame_scalars, text_sms=text_sms)
    for line in render(diffs, stats, args.csharp, args.rust):
        sys.stdout.write(line + "\n")
    if diffs:
        sys.stdout.write("PAIRTRADE RED diff=%d\n" % len(diffs))
        return 1
    sys.stdout.write("PAIRTRADE GREEN diff=0\n")
    return 0


# ---- selftest --------------------------------------------------------------


def _write_jsonl(path, records):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        for rec in records:
            fh.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")


def _rec(bot, k, op="group_mode_on", role="a", ran=True, frames=None, param="",
         gold=1000, bag=None, branch="ok"):
    return {
        "bot": bot, "k": k, "op": op, "role": role, "ran": ran,
        "param": param, "frames": frames if frames is not None else [],
        "gold_before": gold, "gold_after": gold,
        "bag_before": bag if bag is not None else [], "bag_after": bag if bag is not None else [],
        "branch": branch,
    }


def cmd_selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="pairtrade-selftest-")
    ok = 0
    total = 0
    failed = []

    def case(name, expected_rc, fn):
        nonlocal ok, total
        total += 1
        try:
            rc = fn()
        except Prereq:
            rc = 2
        except Exception as exc:  # noqa: BLE001
            rc = -1
            failed.append("%s: exception %s" % (name, exc))
        if rc == expected_rc:
            ok += 1
        else:
            failed.append("%s: rc=%s expect=%s" % (name, rc, expected_rc))

    def run_diff(cs_path, rs_path, **kw):
        try:
            cs = load_raw(cs_path)
            rs = load_raw(rs_path)
            validate(cs, cs_path, "csharp")
            validate(rs, rs_path, "rust")
        except Prereq:
            return 2
        diffs, _stats = compare(cs, rs, **kw)
        return 1 if diffs else 0

    base = [_rec("m3p0", 0), _rec("m3p0", 1, op="create_group", param="m3p1"),
            _rec("m3p1", 0, role="b", ran=False, branch="skip_other_role"),
            _rec("m3p1", 1, role="b", ran=False, branch="skip_other_role")]
    good_a = os.path.join(tmp, "good-a.jsonl")
    good_b = os.path.join(tmp, "good-b.jsonl")
    _write_jsonl(good_a, base)
    _write_jsonl(good_b, base)
    case("identical-green", 0, lambda: run_diff(good_a, good_b))

    def mutate(field, value, exact=None):
        recs = [dict(r) for r in base]
        recs[1] = dict(recs[1])
        recs[1][field] = value
        p = os.path.join(tmp, "mut-%s-%s.jsonl" % (field, str(value).replace("/", "_")))
        _write_jsonl(p, recs)
        return run_diff(good_a, p)

    case("mutate-op-red", 1, lambda: mutate("op", "deal_try"))
    case("mutate-param-red", 1, lambda: mutate("param", "m3pX"))
    case("mutate-branch-red", 1, lambda: mutate("branch", "fail_reject"))
    case("mutate-role-red", 1, lambda: mutate("role", "b"))
    case("mutate-ran-red", 1, lambda: mutate("ran", False))
    case("mutate-gold-red", 1, lambda: mutate("gold_after", 1001))

    def mutate_frames(sm):
        recs = [dict(r) for r in base]
        recs[1] = dict(recs[1])
        recs[1]["frames"] = [{"sm": sm}]
        p = os.path.join(tmp, "mut-frames-%d.jsonl" % sm)
        _write_jsonl(p, recs)
        return run_diff(good_a, p)

    case("mutate-frames-sm-red", 1, lambda: mutate_frames(660))
    case("mutate-frames-scalar-red", 1, lambda: run_diff(
        _nonce(tmp, "sc", lambda recs: recs[1].update({"frames": [{"sm": 661, "a": -3}]})),
        _nonce(tmp, "sc2", lambda recs: recs[1].update({"frames": [{"sm": 661, "a": -4}]})),
        frame_scalars={661: ["a"]},
    ))

    def mutate_text():
        a = _nonce(tmp, "tx-a", lambda recs: recs[1].update({"frames": [{"sm": 101, "text": "你已加入编组"}]}))
        b = _nonce(tmp, "tx-b", lambda recs: recs[1].update({"frames": [{"sm": 101, "text": "你已加入编组 "}]}))
        return run_diff(a, b, text_sms=(101,))

    case("mutate-frames-text-red", 1, mutate_text)

    def mutate_bag():
        a = _nonce(tmp, "bag-a", lambda recs: recs[1].update({"bag_after": ["木剑#1", "金创药#2"]}))
        b = _nonce(tmp, "bag-b", lambda recs: recs[1].update({"bag_after": ["金创药#2", "木剑#1"]}))
        return run_diff(a, b)

    case("bag-multiset-order-green", 0, mutate_bag)

    def mutate_bag_count():
        a = _nonce(tmp, "bagc-a", lambda recs: recs[1].update({"bag_after": ["木剑#1", "木剑#1"]}))
        b = _nonce(tmp, "bagc-b", lambda recs: recs[1].update({"bag_after": ["木剑#1"]}))
        return run_diff(a, b)

    case("bag-multiset-count-red", 1, mutate_bag_count)

    def mutate_exp():
        a = _nonce(tmp, "exp-a", lambda recs: recs[1].update({"exp_after": 100}))
        b = _nonce(tmp, "exp-b", lambda recs: recs[1].update({"exp_after": 101}))
        return run_diff(a, b)

    case("mutate-exp-red", 1, mutate_exp)

    def exp_missing_one_side():
        a = _nonce(tmp, "expm-a", lambda recs: recs[1].update({"exp_after": 100}))
        b = _nonce(tmp, "expm-b", lambda recs: None)
        return run_diff(a, b)

    case("exp-one-side-red", 1, exp_missing_one_side)

    def mutate_pickup():
        a = _nonce(tmp, "pk-a", lambda recs: recs[1].update({"pickup_ok": True}))
        b = _nonce(tmp, "pk-b", lambda recs: recs[1].update({"pickup_ok": False}))
        return run_diff(a, b)

    case("mutate-pickup-red", 1, mutate_pickup)

    def mutate_bot_set():
        recs = [dict(r) for r in base]
        p = os.path.join(tmp, "mut-botset.jsonl")
        _write_jsonl(p, [r for r in recs if r["bot"] != "m3p1"])
        return run_diff(good_a, p)

    case("mutate-bot-set-red", 1, mutate_bot_set)

    def mutate_record_count():
        recs = [dict(r) for r in base]
        p = os.path.join(tmp, "mut-count.jsonl")
        _write_jsonl(p, recs[:-1])
        return run_diff(good_a, p)

    case("mutate-record-count-red", 1, mutate_record_count)

    def k_gap():
        recs = [dict(r) for r in base]
        recs[1] = dict(recs[1])
        recs[1]["k"] = 5
        p = os.path.join(tmp, "mut-kgap.jsonl")
        _write_jsonl(p, recs)
        return run_diff(good_a, p)

    case("k-gap-prereq", 2, k_gap)

    case("missing-file-prereq", 2, lambda: run_diff(
        os.path.join(tmp, "nope.jsonl"), good_b))

    def bad_frame_no_text():
        a = _nonce(tmp, "bft-a", lambda recs: recs[1].update({"frames": [{"sm": 101}]}))
        return run_diff(a, good_b, text_sms=(101,))

    case("text-frame-missing-text-prereq", 2, bad_frame_no_text)

    def bad_frame_sm_type():
        a = _nonce(tmp, "bfs-a", lambda recs: recs[1].update({"frames": [{"sm": "x"}]}))
        return run_diff(a, good_b)

    case("frame-sm-non-int-prereq", 2, bad_frame_sm_type)

    def bad_op():
        a = _nonce(tmp, "bo-a", lambda recs: recs[1].update({"op": "frobnicate"}))
        return run_diff(a, good_b)

    case("bad-op-prereq", 2, bad_op)

    def bad_json():
        p = os.path.join(tmp, "bad-json.jsonl")
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("{not json}\n")
        return run_diff(good_a, p)

    case("bad-json-prereq", 2, bad_json)

    # assemble：事件流 → 记录（含 ran=false 的 skip 步）
    def assemble_ok():
        lines = [
            'M3PAIRTRADESEQ {"phase":"arm","bot":"m3p0","steps":2}',
            'M3PAIRTRADESEQ {"phase":"send","bot":"m3p0","k":0,"op":"group_mode_on","ran":true,"role":"x","param":"","gold_before":1000,"bag_before":[]}',
            'M3PAIRTRADESEQ {"phase":"frame","bot":"m3p0","k":0,"frame":{"sm":659,"a":1}}',
            'M3PAIRTRADESEQ {"phase":"end","bot":"m3p0","k":0,"gold_after":1000,"bag_after":[],"branch":"ok"}',
            'M3PAIRTRADESEQ {"phase":"send","bot":"m3p0","k":1,"op":"deal_try","ran":false,"role":"b","param":"peer","gold_before":1000,"bag_before":[]}',
            'M3PAIRTRADESEQ {"phase":"end","bot":"m3p0","k":1,"gold_after":1000,"bag_after":[],"branch":"skip_other_role"}',
            'M3PAIRTRADESEQ {"phase":"done","bot":"m3p0","steps":2}',
        ]
        p = os.path.join(tmp, "events-ok.jsonl")
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
        out = os.path.join(tmp, "events-ok-raw.jsonl")
        rc = cmd_assemble(argparse.Namespace(markers=p, out=out, side="csharp"))
        if rc != 0:
            return rc
        recs = load_raw(out)
        validate(recs, out, "csharp")
        if len(recs) != 2 or recs[0]["op"] != "group_mode_on" or recs[1]["branch"] != "skip_other_role":
            return 1
        return 0

    case("assemble-ok", 0, assemble_ok)

    def assemble_done_mismatch():
        lines = [
            'M3PAIRTRADESEQ {"phase":"send","bot":"m3p0","k":0,"op":"group_mode_on","ran":true,"role":"x","param":"","gold_before":1000,"bag_before":[]}',
            'M3PAIRTRADESEQ {"phase":"end","bot":"m3p0","k":0,"gold_after":1000,"bag_after":[],"branch":"ok"}',
            'M3PAIRTRADESEQ {"phase":"done","bot":"m3p0","steps":9}',
        ]
        p = os.path.join(tmp, "events-bad.jsonl")
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
        return cmd_assemble(argparse.Namespace(markers=p, out=os.path.join(tmp, "x.jsonl"), side="csharp"))

    case("assemble-done-mismatch-prereq", 2, assemble_done_mismatch)

    def assemble_send_without_end():
        lines = [
            'M3PAIRTRADESEQ {"phase":"send","bot":"m3p0","k":0,"op":"group_mode_on","ran":true,"role":"x","param":"","gold_before":1000,"bag_before":[]}',
            'M3PAIRTRADESEQ {"phase":"done","bot":"m3p0","steps":1}',
        ]
        p = os.path.join(tmp, "events-noend.jsonl")
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(lines) + "\n")
        return cmd_assemble(argparse.Namespace(markers=p, out=os.path.join(tmp, "x.jsonl"), side="csharp"))

    case("assemble-send-without-end-prereq", 2, assemble_send_without_end)

    def assemble_no_events():
        p = os.path.join(tmp, "events-empty.jsonl")
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("no markers here\n")
        return cmd_assemble(argparse.Namespace(markers=p, out=os.path.join(tmp, "x.jsonl"), side="csharp"))

    case("assemble-no-events-prereq", 2, assemble_no_events)

    # 清理
    import shutil
    shutil.rmtree(tmp, ignore_errors=True)

    sys.stdout.write("SELFTEST ok=%d/%d\n" % (ok, total))
    for f in failed:
        sys.stdout.write("SELFTEST FAIL %s\n" % f)
    return 0 if ok == total else 1


def _nonce(tmp, name, mutate):
    """在 base 上应用 mutate，写出一个临时 JSONL，返回路径。"""
    # 复用模块级 base：通过闭包读不到，改为重建
    recs = [
        _rec("m3p0", 0),
        _rec("m3p0", 1, op="create_group", param="m3p1"),
        _rec("m3p1", 0, role="b", ran=False, branch="skip_other_role"),
        _rec("m3p1", 1, role="b", ran=False, branch="skip_other_role"),
    ]
    mutate(recs)
    p = os.path.join(tmp, name + ".jsonl")
    _write_jsonl(p, recs)
    return p


def main(argv=None):
    parser = argparse.ArgumentParser(description="M3⑦ 组队/交易跨侧对拍比较器")
    sub = parser.add_subparsers(dest="cmd")

    p_asm = sub.add_parser("assemble")
    p_asm.add_argument("markers")
    p_asm.add_argument("--out", default="")
    p_asm.add_argument("--side", default="?")
    p_asm.set_defaults(func=cmd_assemble)

    p_diff = sub.add_parser("diff")
    p_diff.add_argument("csharp")
    p_diff.add_argument("rust")
    p_diff.add_argument("--frame-scalars", default="")
    p_diff.add_argument("--text-sms", default=None)
    p_diff.set_defaults(func=cmd_diff)

    p_st = sub.add_parser("selftest")
    p_st.set_defaults(func=cmd_selftest)

    args = parser.parse_args(argv)
    if not getattr(args, "func", None):
        parser.print_help()
        return 2
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
