#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""selloff_diff.py —— M3⑩ 元宝寄售（goldsales sell-off）跨侧对拍比较器（夹具骨架）。

卡：`openmir2-m3-selloff-fixture-botsrv`（M3 逐系统对齐 ⑩ 的承接卡 3；父卡 `openmir2-m3-mail-auction-align`）。
本文件是**判据本体**，只做离线比较：读两侧各自的 `selloff_raw.jsonl`（每条 = 一个假人的一步交互），
逐 (bot, k) 比较「op / 角色 / 是否执行 / 参数 / 帧种类序列 / 声明的标量帧参数 / 文本帧逐字节 /
元宝(GameGold) / 金币(Gold) / 背包多重集 / 失败分支」。真机跑由 `run_selloff_pair.ps1`
（等 ⑩-1 `openmir2-m3-selloff-consignment-world` 世界侧派发落地 + owner 发窗口）驱动。

口径（逐条对齐盘点报告 `M3⑩邮件拍卖现状盘点报告-20261013.md` §4.1）：

1. **fail-closed**：缺文件 / 缺字段 / 字段类型错 / 非法 JSON / k 不连续 / 记录非对象 /
   bot 集合两侧不一致 ⇒ 一律非零退出并点名（`PREREQ_RED`），**绝不静默 GREEN**。
2. **寄售状态机一致（帧证据）**：帧**种类序列**（`sm` id 顺序）严格逐项比——这是「上架 / 删除 /
   成交 / 取消 / 退回」的直接证据。动态 id（actor / make index）**不**作判据（跨运行本来就不同）。
3. **两条假人（卖方 / 买方）**：按 **(bot, k)** 对齐——两侧必须有**同一组 bot**、每个 bot 有**同一串 k**；
   某侧少一个 bot / 某 bot 少一步 ⇒ 判红。
4. **元宝 / 金币一致（原宽度）**：`gamegold_before/after`（= `GameGold`，元宝）与 `gold_before/after`
   （= `Gold`，金刚石特判路径）按原始整数比（不得 i32 截断）。
5. **背包一致**：`bag_before/bag_after` 按**多重集**（`Counter`）比，**不做 set 差集**。
6. **落库口径（= oracle「寄售不落库」）**：oracle 的 `SaveSellOffItemList` 调用点全部被注释、
   读列名写错（D-5/B-5）⇒ 两侧都不写、都加载 0 行。本比较器**不**在记录里造跨重登保留；
   DB 侧判据由 `run_selloff_pair.ps1` 里的 `scoped_diff.py --canonical` + `goldsales` 表快照承担（增量必须为 0）。
7. **改坏必红**：`selftest` 对「op / bot / k / 帧种类 / 帧文本 / 帧标量 / 元宝 / 金币 / 背包 / branch /
   记录数 / bot 集合」逐个改坏 ⇒ 必红。

记录 schema（每行一个 JSON 对象；`selloff_scenario.json` 里也写一份）：

    {
      "bot": "m3s1230",          # 假人角色名（ChrName = LoginAccount+序号；跨侧必须相同）
      "k": 0,                    # 交互序号（0 起，每个 bot 内连续）
      "role": "a",               # 步骤声明的角色：a=seller / b=buyer / x=both
      "op": "seller_add_item",   # 见 OPS
      "ran": true,               # 本 bot 是否实际执行本步（false = 角色不符，跳过）
      "param": "peer",           # 步骤参数（peer 记号 / 元宝数 / 物品名；原样比）
      "frames": [                # 本步收到的相关 SM_* 帧（有序）
        {"sm": 23003}
      ],
      "gamegold_before": 100000, "gamegold_after": 100000,
      "gold_before": 0, "gold_after": 0,
      "bag_before": ["木剑#43"], "bag_after": ["木剑#43"],
      "branch": "ok"             # ok | fail_reject | skip_other_role |
                                 # fail_no_frame | frames_unclassified
    }

退出码：0 = GREEN（逐项相同）；1 = RED（有差异）；2 = PREREQ（缺件/坏件，fail-closed）。

子命令：
  assemble <markers.jsonl> --out <raw.jsonl> [--side ?]
      把 BotSrv 的 `M3SELLOFFSEQ {json}` 事件流（phase=arm/send/frame/end/done）按 (bot, k) 合并成记录。
  diff <csharp_raw.jsonl> <rust_raw.jsonl> [--frame-scalars JSON] [--text-sms 767[,..]]
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

# 步骤声明的角色：a=seller（本批 SellerRole） / b=buyer（BuyerRole） / x=both。
ROLES = ("a", "b", "x")

OPS = (
    # 卖方窗内动作（PlayObject.Sale.cs:60/127/431；CM 23002/23007/23012）
    "seller_add_item", "seller_del_item", "seller_cancel",
    # 上架（PlayObject.Sale.cs:516；CM 23015）
    "seller_end",
    # 取消正在寄售（出售人）（PlayObject.Sale.cs:185；CM 23024）
    "seller_cancel_ing",
    # 买方（PlayObject.Sale.cs:254/281；CM 23025/23026）
    "buyer_buy_cancel", "buyer_buy",
    # 掉线中断（CM_SOFTCLOSE(1009) ⇒ 走 WorldServer 断线路径）
    "softclose",
)

# 结构必填。
REQUIRED_FIELDS = ("bot", "k", "op", "ran", "frames", "gamegold_before", "gamegold_after",
                   "gold_before", "gold_after", "branch")
# 可选列表（多重集比）。
OPTIONAL_LIST_FIELDS = ("bag_before", "bag_after")
# 可选整数（原宽度比；一侧有另一侧没有 ⇒ 判红）。
OPTIONAL_INT_FIELDS = ()
FRAME_FIELDS = ("sm", "a", "b", "c", "d")

# 需要逐字节比对**文本**的 SM（`SM_MENU_OK(767)` = 寄售的 `[成功]/[错误]` 文案）。
DEFAULT_TEXT_SMS = (767,)

# 场景声明的**标量帧参数**：只有这些 (sm, 参数名) 参与判据。默认空；窗口期可按真机实测补/删。
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

    另外强制：每个 bot 的 k 必须 0 起连续（寄售序列不完整 ⇒ fail-closed）。
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
        for fld in ("gamegold_before", "gamegold_after", "gold_before", "gold_after"):
            if not _is_int(rec[fld]):
                raise Prereq("%s: bot=%s k=%s 字段 %s 非整数 (%s)" % (side, bot, k, fld, path))
        for fld in OPTIONAL_INT_FIELDS:
            if fld in rec and rec[fld] is not None and not _is_int(rec[fld]):
                raise Prereq("%s: bot=%s k=%s 字段 %s 非整数/null (%s)" % (side, bot, k, fld, path))
        _validate_frames(rec["frames"], side, bot, k, path, text_sms)
        for fld in OPTIONAL_LIST_FIELDS:
            if fld in rec and rec[fld] is not None and not isinstance(rec[fld], list):
                raise Prereq("%s: bot=%s k=%s 字段 %s 非数组 (%s)" % (side, bot, k, fld, path))


# ---- assemble（BotSrv M3SELLOFFSEQ 事件流 → 记录） ---------------------------


def _marker_events(lines):
    """从文本行里抽出 `M3SELLOFFSEQ {json}` 事件（保持顺序）。非法 JSON ⇒ Prereq。"""
    out = []
    for lineno, line in enumerate(lines, 1):
        idx = line.find("M3SELLOFFSEQ ")
        if idx < 0:
            continue
        payload = line[idx + len("M3SELLOFFSEQ "):].strip()
        if payload.startswith("skip="):
            # 非 JSON 的跳过行（NPC 不在视野 / op 未实装）：结构化 warning，不当作事件。
            continue
        try:
            ev = json.loads(payload)
        except json.JSONDecodeError as exc:
            raise Prereq("M3SELLOFFSEQ 事件非法 JSON（第 %d 行）: %s" % (lineno, exc))
        if not isinstance(ev, dict):
            raise Prereq("M3SELLOFFSEQ 事件非对象（第 %d 行）" % lineno)
        out.append(ev)
    if not out:
        raise Prereq("无 M3SELLOFFSEQ 事件（驱动未生效/日志未含标记）")
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
                "gamegold_before": se.get("gamegold_before"),
                "gamegold_after": en.get("gamegold_after"),
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
            for fld in OPTIONAL_INT_FIELDS:
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


def compare(cs, rs, frame_scalars=None, text_sms=DEFAULT_TEXT_SMS):
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
        # 元宝 / 金币（原宽度整数）
        for fld in ("gamegold_before", "gamegold_after", "gold_before", "gold_after"):
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
        for fld in OPTIONAL_INT_FIELDS:
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
        lines.append("SELLOFF %s RED %s csharp=%r rust=%r" % (loc, what, av, bv))
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
        sys.stdout.write("SELLOFF RED diff=%d\n" % len(diffs))
        return 1
    sys.stdout.write("SELLOFF GREEN diff=0\n")
    return 0


# ---- selftest --------------------------------------------------------------


def _write_jsonl(path, records):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        for rec in records:
            fh.write(json.dumps(rec, ensure_ascii=False, sort_keys=True) + "\n")


def _rec(bot, k, op="seller_add_item", role="a", ran=True, frames=None, param="",
         gg=100000, gold=0, bag=None, branch="ok"):
    return {
        "bot": bot, "k": k, "op": op, "role": role, "ran": ran,
        "param": param, "frames": frames if frames is not None else [],
        "gamegold_before": gg, "gamegold_after": gg,
        "gold_before": gold, "gold_after": gold,
        "bag_before": bag if bag is not None else [], "bag_after": bag if bag is not None else [],
        "branch": branch,
    }


def _selftest(args):
    import tempfile
    tmp = tempfile.mkdtemp(prefix="selloff-selftest-")
    results = []

    def check(name, cond):
        results.append((name, bool(cond)))

    # --- fail-closed 族 -------------------------------------------------------
    def _run_diff(cs, rs, **kw):
        d, _s = compare(cs, rs, **kw)
        return len(d) == 0

    base = [_rec("m3s0", 0), _rec("m3s1", 0, op="buyer_buy", role="b")]
    check("identical-green", _run_diff(base, list(base)))
    # 改坏必红（逐字段）
    mut = json.loads(json.dumps(base)); mut[0]["op"] = "seller_del_item"
    check("mutate-op-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["role"] = "b"
    check("mutate-role-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["ran"] = False
    check("mutate-ran-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["param"] = "other"
    check("mutate-param-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["branch"] = "fail_reject"
    check("mutate-branch-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["gamegold_before"] = 99999
    check("mutate-gamegold-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["gold_after"] = 5
    check("mutate-gold-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["bag_after"] = ["木剑#43"]
    check("mutate-bag-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["frames"] = [{"sm": 23003}]
    check("mutate-frame-sm-red", not _run_diff(base, mut))
    mut = json.loads(json.dumps(base)); mut[0]["frames"] = [{"sm": 767, "text": "x"}]
    base_txt = json.loads(json.dumps(base)); base_txt[0]["frames"] = [{"sm": 767, "text": "y"}]
    check("mutate-frame-text-red", not _run_diff(base_txt, mut))
    # 记录数 / bot 集合
    check("mutate-records-red", not _run_diff(base, base[:1]))
    check("mutate-bots-red", not _run_diff(base, [_rec("m3s0", 0), _rec("m3s2", 0)]))
    # 多重集次序绿、count 红
    b1 = [_rec("m3s0", 0, bag=["a", "b"])]
    b2 = [_rec("m3s0", 0, bag=["b", "a"])]
    check("bag-multiset-order-green", _run_diff(b1, b2))
    b3 = [_rec("m3s0", 0, bag=["a"])]
    check("bag-multiset-count-red", not _run_diff(b1, b3))
    # 帧标量（声明后参与判据）
    fs = {23016: ["a"]}
    c = [_rec("m3s0", 0, frames=[{"sm": 23016, "a": 1}])]
    r = [_rec("m3s0", 0, frames=[{"sm": 23016, "a": 2}])]
    check("frame-scalar-red", not _run_diff(c, r, frame_scalars=fs))
    check("frame-scalar-green", _run_diff(c, list(c), frame_scalars=fs))
    # 同帧序列不同顺序 ⇒ 红
    o1 = [_rec("m3s0", 0, frames=[{"sm": 23003}, {"sm": 23016}])]
    o2 = [_rec("m3s0", 0, frames=[{"sm": 23016}, {"sm": 23003}])]
    check("frame-order-red", not _run_diff(o1, o2))

    # --- validate / assemble fail-closed -------------------------------------
    def _validate_rc(records):
        try:
            validate(records, "<mem>", "csharp")
            return 0
        except Prereq:
            return 2

    def _validate_txt(records):
        try:
            validate(records, "<mem>", "csharp", text_sms=(767,))
            return 0
        except Prereq:
            return 2

    check("validate-ok", _validate_rc(base) == 0)
    check("validate-kgap-red", _validate_rc([_rec("m3s0", 1)]) == 2)
    check("validate-badop-red", _validate_rc([_rec("m3s0", 0, op="frobnicate")]) == 2)
    check("validate-nonint-k-red", _validate_rc([_rec("m3s0", "0")]) == 2)
    check("validate-missing-text-red", _validate_txt([_rec("m3s0", 0, frames=[{"sm": 767}])]) == 2)
    check("validate-frame-sm-nonint-red",
          _validate_rc([_rec("m3s0", 0, frames=[{"sm": "23003"}])]) == 2)

    def _assemble_rc(events):
        try:
            assemble(events, side="csharp")
            return 0
        except Prereq:
            return 2

    ev_ok = [
        {"phase": "arm", "bot": "m3s0", "steps": 1},
        {"phase": "send", "bot": "m3s0", "k": 0, "op": "seller_add_item", "ran": True,
         "gamegold_before": 1, "gold_before": 0},
        {"phase": "end", "bot": "m3s0", "k": 0, "gamegold_after": 1, "gold_after": 0, "branch": "ok"},
        {"phase": "done", "bot": "m3s0", "steps": 1},
    ]
    check("assemble-ok", _assemble_rc(ev_ok) == 0)
    check("assemble-nosend-red", _assemble_rc([{"phase": "arm", "bot": "m3s0"}]) == 2)
    check("assemble-noend-red", _assemble_rc([e for e in ev_ok if e["phase"] != "end"]) == 2)
    check("assemble-done-mismatch-red",
          _assemble_rc(ev_ok[:-1] + [{"phase": "done", "bot": "m3s0", "steps": 2}]) == 2)
    check("assemble-unknown-phase-red", _assemble_rc(ev_ok + [{"phase": "bogus", "bot": "m3s0"}]) == 2)
    check("assemble-no-events-red", _assemble_rc([]) == 2)

    # --- CLI 端到端（写盘 + 缺件） -------------------------------------------
    cs_path = os.path.join(tmp, "csharp.jsonl")
    rs_path = os.path.join(tmp, "rust.jsonl")
    _write_jsonl(cs_path, base)
    _write_jsonl(rs_path, base)
    rc_green = cmd_diff(_ns(csharp=cs_path, rust=rs_path, frame_scalars=None, text_sms=None))
    check("cli-diff-green", rc_green == 0)
    mut2 = json.loads(json.dumps(base)); mut2[0]["op"] = "seller_end"
    _write_jsonl(rs_path, mut2)
    check("cli-diff-red", cmd_diff(_ns(csharp=cs_path, rust=rs_path, frame_scalars=None, text_sms=None)) == 1)
    check("cli-diff-missing-prereq",
          cmd_diff(_ns(csharp=cs_path, rust=os.path.join(tmp, "nope.jsonl"),
                        frame_scalars=None, text_sms=None)) == 2)

    ok = sum(1 for _n, v in results if v)
    total = len(results)
    for name, v in results:
        sys.stdout.write("%s %s\n" % ("ok" if v else "FAIL", name))
    sys.stdout.write("SELFTEST ok=%d/%d\n" % (ok, total))
    return 0 if ok == total else 1


class _ns(object):
    def __init__(self, **kw):
        self.__dict__.update(kw)


def main(argv=None):
    parser = argparse.ArgumentParser(description="M3⑩ 元宝寄售跨侧比较器（夹具骨架）")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_as = sub.add_parser("assemble")
    p_as.add_argument("markers")
    p_as.add_argument("--out", default=None)
    p_as.add_argument("--side", default="?")

    p_df = sub.add_parser("diff")
    p_df.add_argument("csharp")
    p_df.add_argument("rust")
    p_df.add_argument("--frame-scalars", default=None)
    p_df.add_argument("--text-sms", default=None)

    sub.add_parser("selftest")

    args = parser.parse_args(argv)
    if args.cmd == "assemble":
        return cmd_assemble(args)
    if args.cmd == "diff":
        return cmd_diff(args)
    return _selftest(args)


if __name__ == "__main__":
    sys.exit(main())
