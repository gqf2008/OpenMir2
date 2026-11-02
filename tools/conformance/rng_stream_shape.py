#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""rng_stream_shape.py — runbook §6.1「RNG 流位类差异」形状检查器（离线）。

runbook `docs/window-run-1-20261024.md` §6.1 条件 1 的**可执行版**：输入两份
`frames.ndjson`，判断两侧差异形状是否**只能**由 m（m ≥ 1）个完整的
`(SM_TURN(10), SM_FEATURECHANGED(41))` 对解释：

  * 对齐后除差块外的其余帧（公共前缀 + 公共后缀）必须完全一致；
  * 差块只能出现在**同一侧**，是 m 个完整对，且每个 `10` 的**紧邻下一帧必须是 `41`**；
  * 两侧帧数差必须恰为 2m；
  * 不允许其它 ident 的多出/缺失、非成对单帧、或非随机帧的相对次序差。

**判等域不是 raw bytes**：直接复用 `compare_runs.py` 已登记的规则语义——
`RuleSet.mask_for()`（global masks `conn`/`recog`/`seq`/`t_ms` + ident 级
`head_mask`/`head_compare`）与 `compare_bodies()`（逐字节 / `ability_layout` /
`mask_body_ranges` 等 action）。规则表默认读同目录 `rules.json`，**不写死字段**。
`action=ignore` 的 ident 按比较器口径先剔除（逐帧计数登记，不静默）。
未登记 ident ⇒ **UNDECIDABLE**（fail-closed，绝不静默当绿）。

退出码（与 `compare_runs.py` 同口径）：
  0 = PASS（形状只由完整对解释；m 可为 0，即两侧在该判等域下全等）
  1 = FAIL（形状不成立：倒序/单帧/非成对/其它帧不一致……）
  2 = UNDECIDABLE（输入坏、规则缺失、未登记 ident、body 无法比较、空运行）

用法：
    python -I -B tools/conformance/rng_stream_shape.py \
        --oracle <csharp-frames.ndjson> --rust <rust-frames.ndjson> \
        [--rules tools/conformance/rules.json] [--json report.json]
    python -I -B tools/conformance/rng_stream_shape.py selftest

自测（全离线，不连网/不连库/不起进程）用仓库真实 fixture：
  * `mir2-rs/bins/game-svr/tests/fixtures/npc_click/`：run1 恰好 PASS（30/32、前缀 29、后缀 1、m=1）；
  * 同一 fixture 的五类反例：`(41,10)` 倒序 / 单 `10` / 单 `41` / 非成对 `41` / 改 643 body ⇒ FAIL；
  * `tools/conformance/fixtures/npc_click_w1c/`：w1c 真机证据的真实 FAIL（`643` 提前，index 19）⇒ FAIL。
"""
import argparse
import hashlib
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    # `python -I` 不把脚本目录放进 sys.path，必须显式加，才能 import 同目录比较器。
    sys.path.insert(0, HERE)

import compare_runs as base  # noqa: E402

TOOL = "tools/conformance/rng_stream_shape.py"
DEFAULT_RULES = os.path.join(HERE, "rules.json")
FORMAT = "rng-stream-shape/1"


class Undecidable(Exception):
    """判不了（输入/规则/语义不完整）——绝不当 PASS。"""


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_rules(path):
    try:
        return base.load_rules(path)
    except base.InputError as e:
        raise Undecidable(str(e))


def load_stream_file(path, label):
    """读一份 frames.ndjson；输入语义（控制帧过滤/body/sha 校验）复用 compare_runs。

    `compare_runs.load_stream()` 只接受**目录**并读其中 `frames.ndjson`。真实对拍产物
    通常正是 `<run>/frames.ndjson` ⇒ 直接复用。fixture 文件名是 `csharp-frames.ndjson`
    这类，则在其旁建临时目录硬链（跨卷退化为复制）后仍交给同一个 loader，
    避免在检查器里抄第二份解析/校验逻辑。
    """
    path = os.path.abspath(path)
    if not os.path.isfile(path):
        raise Undecidable("--%s 文件不存在：%s" % (label, path))
    if os.path.basename(path) == "frames.ndjson":
        try:
            return base.load_stream(os.path.dirname(path), label)
        except base.Undecidable as e:
            raise Undecidable(str(e))
    dirpath = os.path.dirname(path)
    with tempfile.TemporaryDirectory(prefix="mir2-shape-") as td:
        dst = os.path.join(td, "frames.ndjson")
        try:
            os.link(path, dst)
        except OSError:
            shutil.copyfile(path, dst)
        try:
            stream = base.load_stream(td, label)
        except base.Undecidable as e:
            raise Undecidable(str(e))
    stream.dirpath = dirpath
    stream.path = path
    return stream


def partition_ignored(rules, frames):
    """按规则表 `action=ignore` 先剔除（与 compare_runs 的 split() 同口径）。"""
    kept, dropped = [], []
    for fr in frames:
        rule = rules.by_ident.get(fr["ident"])
        if rule and rule.get("action") == "ignore":
            dropped.append(fr)
        else:
            kept.append(fr)
    return kept, dropped


def frame_status(rules, fr_a, fr_b):
    """两帧在**比较器判等域**下是否相等。

    返回 `(status, detail)`：status ∈ {"equal","diff","undecidable"}；
    detail 用中性键 `a`/`b`（调用方知道哪侧是 oracle/rust）。
    头部与 body 的判等全走 compare_runs：`mask_for` + `compare_bodies`，
    本检查器不另写掩码，也不做 raw bytes 比较。
    """
    if fr_a["ident"] != fr_b["ident"]:
        return "diff", {"field": "ident",
                        "reason": "ident 不同（%r vs %r）" % (fr_a["ident"], fr_b["ident"]),
                        "a": fr_a["ident"], "b": fr_b["ident"]}
    ident = fr_a["ident"]
    masks = rules.mask_for(ident)
    for field in base.STREAM_FIELDS:
        av, bv = fr_a.get(field), fr_b.get(field)
        # 先查缺失再走掩码：掩码只免「值不同」，不免「根本没给」（与 compare_runs 同）。
        if av is None or bv is None:
            if av is None and bv is None:
                continue
            return "undecidable", {"field": field,
                                   "reason": "字段 %s 只有一侧提供" % field}
        if field in masks:
            continue
        if av != bv:
            return "diff", {"field": field,
                            "reason": "头部 %s 不同（%r vs %r）" % (field, av, bv),
                            "a": av, "b": bv}
    diffs, whitelisted, undecidable, masked_note = [], [], [], []
    try:
        base.compare_bodies(fr_a, fr_b, rules.by_ident.get(ident),
                            diffs, whitelisted, undecidable, masked_note)
    except (base.Undecidable, base.InputError) as e:
        return "undecidable", {"field": "body", "reason": str(e)}
    if diffs:
        d = diffs[0]
        return "diff", {"field": d.get("field", "body"),
                        "reason": d.get("detail") or "body 不同",
                        "offset": d.get("offset")}
    if undecidable:
        d = undecidable[0]
        return "undecidable", {"field": d.get("field", "body"),
                               "reason": d.get("detail") or "body 无法比较"}
    return "equal", {}


def pair_block_status(rules, block):
    """差块是否是 m 个完整 `(turn_ident, feature_ident)` 对（ident 从 rules.json 读）。"""
    rp = rules.random_branch_pairs
    if rp is None:
        raise Undecidable("规则表缺 random_branch_pairs，无法识别「完整 (10,41) 对」")
    n = len(block)
    if n % 2:
        return False, {"field": "frame_count",
                       "reason": "帧数差 %d 为奇数，不能由完整 (turn,feature) 对解释" % n}
    for k in range(0, n, 2):
        turn, feat = block[k], block[k + 1]
        if turn["ident"] != rp["turn_ident"]:
            return False, {"field": "ident", "ident": turn["ident"], "offset_in_block": k,
                           "reason": "差块第 %d 帧 ident=%d，不是 turn_ident=%d"
                                     "（完整对必须从 turn 开始）"
                                     % (k, turn["ident"], rp["turn_ident"])}
        # 核心牙：turn 的**紧邻下一帧**必须是 feature（10 必须紧随 41）。
        if feat["ident"] != rp["feature_ident"]:
            return False, {"field": "ident", "ident": feat["ident"], "offset_in_block": k + 1,
                           "reason": "turn_ident=%d 的紧邻下一帧 ident=%d，不是 feature_ident=%d"
                                     "（10 必须紧随 41）"
                                     % (turn["ident"], feat["ident"], rp["feature_ident"])}
        # body 长度也按规则登记值校验（与 compare_runs.extract_random_pairs 同口径）：
        # 带名尾的登录期 TURN（body_len>11）不是随机分支对，不能当差块解释掉。
        if rp.get("turn_body_len") is not None:
            if turn["body"] is None or len(turn["body"]) != rp["turn_body_len"]:
                return False, {"field": "body_len", "ident": turn["ident"], "offset_in_block": k,
                               "reason": "turn body 长度不符登记的 turn_body_len=%d"
                                         "（带名尾 TURN 不属于随机分支对）"
                                         % rp["turn_body_len"]}
        if rp.get("feature_body_len") is not None:
            if feat["body"] is not None and len(feat["body"]) != rp["feature_body_len"]:
                return False, {"field": "body_len", "ident": feat["ident"],
                               "offset_in_block": k + 1,
                               "reason": "feature body 长度不符登记的 feature_body_len=%d"
                                         % rp["feature_body_len"]}
    return True, {}


def _frame_ref(fr, side):
    return {"side": side, "frame_index": fr["index"], "line": fr["line"],
            "ident": fr["ident"]}


def _prefix_suffix_tables(rules, A, B, d):
    """A 比 B 长 d 帧。返回同位置/错位 d 的逐帧状态与前后缀布尔表。"""
    same = [frame_status(rules, A[k], B[k]) for k in range(len(B))]
    shift = [frame_status(rules, A[k + d], B[k]) for k in range(len(B))]

    prefix_ok = [True] * (len(B) + 1)
    for k in range(len(B)):
        prefix_ok[k + 1] = prefix_ok[k] and same[k][0] == "equal"

    suffix_ok = [True] * (len(B) + 1)
    for k in range(len(B) - 1, -1, -1):
        suffix_ok[k] = suffix_ok[k + 1] and shift[k][0] == "equal"
    return shift, prefix_ok, suffix_ok


def _fail(reason, field, frame, **extra):
    out = {"verdict": "FAIL", "exit_code": 1, "m": 0,
           "first_fail": {"reason": reason, "field": field, "frame": frame}}
    out["first_fail"].update(extra)
    return out


def analyze(oracle_frames, rust_frames, rules):
    """核心判定。输入是**已解析且已剔除 ignore** 的帧列表。"""
    if not oracle_frames and not rust_frames:
        return {"verdict": "UNDECIDABLE", "exit_code": 2, "kind": "empty_run",
                "detail": "两侧可比较帧都是 0（空运行）— 不是 PASS"}
    if not oracle_frames or not rust_frames:
        return {"verdict": "UNDECIDABLE", "exit_code": 2, "kind": "one_side_empty",
                "detail": "两侧一侧 0 帧、另一侧非 0：没有任何公共帧可验证形状 — 不是 PASS"}

    if len(oracle_frames) > len(rust_frames):
        long_side, A, B = "oracle", oracle_frames, rust_frames
    elif len(rust_frames) > len(oracle_frames):
        long_side, A, B = "rust", rust_frames, oracle_frames
    else:
        long_side = None
        A, B = oracle_frames, rust_frames
    side_a = "oracle" if A is oracle_frames else "rust"
    side_b = "rust" if side_a == "oracle" else "oracle"
    d = len(A) - len(B)

    shift, prefix_ok, suffix_ok = _prefix_suffix_tables(rules, A, B, d)

    def lcp():
        p = 0
        while p < len(B) and prefix_ok[p + 1]:
            p += 1
        return p

    # 条件 1：帧数差必须恰为 2m。奇数 ⇒ 直接 FAIL（不能由完整对解释）。
    if d % 2:
        p = lcp()
        frame = A[p] if p < len(A) else A[-1]
        return _fail("帧数差 %d 为奇数，不能由完整 (turn,feature) 对解释" % d,
                     "frame_count", _frame_ref(frame, side_a), ident=frame["ident"])

    valid = []            # i —— 前缀/后缀全等且差块是完整对
    block_bad_ok = []     # (i, detail) —— 前后缀全等但差块不合法
    for i in range(len(B), -1, -1):
        if not prefix_ok[i] or not suffix_ok[i]:
            continue
        ok, detail = pair_block_status(rules, A[i:i + d])
        if ok:
            valid.append(i)
        else:
            block_bad_ok.append((i, detail))

    if valid:
        i = valid[0]  # 取最大公共前缀（与 runbook §6.3 的「公共前缀 29」同口径）
        return {"verdict": "PASS", "exit_code": 0, "m": d // 2,
                "long_side": long_side, "common_prefix": i,
                "common_suffix": len(B) - i,
                "pairs_side": long_side,
                "detail": ("差块 = %d 个完整 (turn_ident=%d, feature_ident=%d) 对，只在 %s 侧；"
                           "其余帧在比较器判等域下全等"
                           % (d // 2, rules.random_branch_pairs["turn_ident"],
                              rules.random_branch_pairs["feature_ident"], long_side))}

    if block_bad_ok:
        # 前后缀都能对上，只有差块形状不成立：就是形状本身的判定，最直白。
        i, detail = block_bad_ok[0]
        return _fail(detail.get("reason", "差块不是 m 个完整 (10,41) 对"),
                     detail.get("field"), _frame_ref(A[i], side_a),
                     ident=detail.get("ident"))

    # 前后缀本身就对不上：定位第一处。
    p = lcp()
    # 帧数差是偶数：先看 p 处的差块（LCP 之后），再看差块之后的错位后缀。
    block_ok, block_detail = pair_block_status(rules, A[p:p + d])
    if not block_ok:
        return _fail(block_detail.get("reason", "差块不是完整对"),
                     block_detail.get("field"), _frame_ref(A[p], side_a),
                     ident=block_detail.get("ident"))

    # 差块本身合法，但差块之外的帧在判等域下不一致。
    for k in range(p, len(B)):
        status, detail = shift[k]
        if status == "equal":
            continue
        if status == "undecidable":
            return {"verdict": "UNDECIDABLE", "exit_code": 2, "kind": "frame_undecidable",
                    "long_side": long_side,
                    "first_fail": {"reason": detail.get("reason", "帧无法比较"),
                                   "field": detail.get("field"),
                                   "frame": _frame_ref(A[k + d], side_a),
                                   "counterpart": _frame_ref(B[k], side_b)},
                    "detail": "差块之外存在无法比较的帧 ⇒ 判不了（不当 FAIL，也不当 PASS）"}
        return _fail("差块之外的帧不一致：%s" % detail.get("reason", ""),
                     detail.get("field"), _frame_ref(A[k + d], side_a),
                     counterpart=_frame_ref(B[k], side_b))

    # 理论上不可达（前缀/后缀都能对上且差块合法 ⇒ valid 非空）。
    return {"verdict": "UNDECIDABLE", "exit_code": 2, "kind": "no_alignment",
            "long_side": long_side,
            "detail": "找不到可判定的对齐（内部不变式被破坏）"}


def _report(rules, rules_path, o_stream, r_stream, result, o_ign, r_ign):
    rp = rules.random_branch_pairs
    return {
        "format": FORMAT,
        "verdict": result.get("verdict"),
        "exit_code": result.get("exit_code"),
        "m": result.get("m", 0),
        "common_prefix": result.get("common_prefix"),
        "common_suffix": result.get("common_suffix"),
        "pairs_side": result.get("pairs_side"),
        "long_side": result.get("long_side"),
        "kind": result.get("kind"),
        "detail": result.get("detail"),
        "first_fail": result.get("first_fail"),
        "rules": {"path": os.path.abspath(rules_path), "sha256": sha256_file(rules_path)},
        "shape_rule": None if rp is None else {
            "turn_ident": rp.get("turn_ident"),
            "feature_ident": rp.get("feature_ident"),
            "tier": rp.get("tier"),
            "approved_by": rp.get("approved_by"),
            "pending": str(rp.get("approved_by", "")).upper().startswith("PENDING"),
        },
        "frames": {
            "oracle": {
                "path": o_stream.path, "sha256": sha256_file(o_stream.path),
                "frames": len(o_stream.frames), "ignored": len(o_ign),
                "filtered_control": len(o_stream.filtered_control),
            },
            "rust": {
                "path": r_stream.path, "sha256": sha256_file(r_stream.path),
                "frames": len(r_stream.frames), "ignored": len(r_ign),
                "filtered_control": len(r_stream.filtered_control),
            },
        },
        "analyzed_frames": {
            "oracle": len(o_stream.frames) - len(o_ign),
            "rust": len(r_stream.frames) - len(r_ign),
        },
    }


def check(oracle_path, rust_path, rules_path=DEFAULT_RULES):
    rules = load_rules(rules_path)
    o_stream = load_stream_file(oracle_path, "oracle")
    r_stream = load_stream_file(rust_path, "rust")

    # 未登记 ident ⇒ 判不了（与 compare_runs 的 UNCLASSIFIED 同口径）。
    for label, frames in (("oracle", o_stream.frames), ("rust", r_stream.frames)):
        for fr in frames:
            if fr["ident"] not in rules.by_ident:
                return _report(rules, rules_path, o_stream, r_stream,
                               {"verdict": "UNDECIDABLE", "exit_code": 2,
                                "kind": "unclassified",
                                "detail": "%s 侧出现未登记 ident=%d（index=%d）⇒ 判等域不完整"
                                          % (label, fr["ident"], fr["index"])},
                               [], [])

    o_kept, o_ign = partition_ignored(rules, o_stream.frames)
    r_kept, r_ign = partition_ignored(rules, r_stream.frames)
    try:
        result = analyze(o_kept, r_kept, rules)
    except Undecidable as e:
        result = {"verdict": "UNDECIDABLE", "exit_code": 2, "kind": "prereq",
                  "detail": str(e)}
    return _report(rules, rules_path, o_stream, r_stream, result, o_ign, r_ign)


def human_summary(rep):
    v = rep.get("verdict")
    if v == "PASS":
        head = ("RNG_SHAPE PASS m=%s common_prefix=%s common_suffix=%s pairs_side=%s"
                % (rep.get("m"), rep.get("common_prefix"),
                   rep.get("common_suffix"), rep.get("pairs_side")))
    else:
        head = "RNG_SHAPE %s" % v
        if rep.get("kind"):
            head += " kind=%s" % rep["kind"]
        ff = rep.get("first_fail") or {}
        if ff.get("frame"):
            fr = ff["frame"]
            head += (" first_fail=%s[%s] ident=%s field=%s"
                     % (fr.get("side"), fr.get("frame_index"),
                        fr.get("ident"), ff.get("field")))
    lines = [head]
    f = rep.get("frames") or {}
    if f.get("oracle") and f.get("rust"):
        lines.append("frames: oracle=%d (ignored %d, control %d) rust=%d (ignored %d, control %d)"
                     % (f["oracle"]["frames"], f["oracle"]["ignored"],
                        f["oracle"]["filtered_control"], f["rust"]["frames"],
                        f["rust"]["ignored"], f["rust"]["filtered_control"]))
    sr = rep.get("shape_rule")
    if sr:
        lines.append("shape_rule: turn=%s feature=%s tier=%s pending=%s"
                     % (sr.get("turn_ident"), sr.get("feature_ident"),
                        sr.get("tier"), sr.get("pending")))
    if rep.get("detail"):
        lines.append("detail: %s" % rep["detail"])
    ff = rep.get("first_fail") or {}
    if ff.get("reason"):
        lines.append("first_fail_reason: %s" % ff["reason"])
    if ff.get("counterpart"):
        c = ff["counterpart"]
        lines.append("counterpart: %s[%s] ident=%s"
                     % (c.get("side"), c.get("frame_index"), c.get("ident")))
    r = rep.get("rules") or {}
    if r.get("sha256"):
        lines.append("rules: %s sha256=%s" % (r.get("path"), r["sha256"][:16]))
    return "\n".join(lines)


def build_parser():
    p = argparse.ArgumentParser(
        prog="rng_stream_shape.py",
        description="runbook §6.1 RNG 流位类差异形状检查器（比对两份 frames.ndjson）")
    p.add_argument("--oracle", metavar="PATH", help="oracle/C# 侧 frames.ndjson")
    p.add_argument("--rust", metavar="PATH", help="rust 侧 frames.ndjson")
    p.add_argument("--rules", default=DEFAULT_RULES, help="规则表（默认 tools/conformance/rules.json）")
    p.add_argument("--json", dest="json_out", metavar="PATH", help="把完整报告写成 JSON")
    p.add_argument("--quiet", action="store_true", help="不打印终端摘要")
    return p


def write_report(rep, path):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(rep, f, ensure_ascii=False, indent=2)
        f.write("\n")


# ---------------------------------------------------------------------------
# 自测（真实 fixture；离线、确定性）
# ---------------------------------------------------------------------------
def _renumber(frames):
    """浅拷贝并把 index 改成新序列位置（构造反例后位置才有意义）。"""
    return [dict(fr, index=i) for i, fr in enumerate(frames)]


def _with_body(fr, body):
    x = dict(fr)
    x["body"] = body
    x["body_sha256"] = base.sha256_bytes(body)
    return x


def _load_pair(csharp_path, rust_path):
    o = load_stream_file(csharp_path, "oracle").frames
    r = load_stream_file(rust_path, "rust").frames
    return _renumber(o), _renumber(r)


def cmd_selftest(argv=None):
    ap = argparse.ArgumentParser(prog="rng_stream_shape.py selftest")
    ap.add_argument("--rules", default=DEFAULT_RULES)
    args = ap.parse_args(argv or [])
    rules = load_rules(args.rules)

    repo = os.path.dirname(os.path.dirname(HERE))
    fx = os.path.join(repo, "mir2-rs", "bins", "game-svr", "tests", "fixtures", "npc_click")
    csharp_path = os.path.join(fx, "csharp-frames.ndjson")
    rust_path = os.path.join(fx, "rust-frames.ndjson")
    w1c_dir = os.path.join(HERE, "fixtures", "npc_click_w1c")

    pass_n = fail_n = 0

    def chk(cond, name, detail=""):
        nonlocal pass_n, fail_n
        if cond:
            pass_n += 1
            print("PASS %s" % name)
        else:
            fail_n += 1
            print("FAIL %s %s" % (name, detail))

    cs, rs = _load_pair(csharp_path, rust_path)
    chk(len(cs) == 30 and len(rs) == 32, "fixture: run1 帧数 30/32",
        "cs=%d rs=%d" % (len(cs), len(rs)))
    # 判等域**不是 raw bytes** 的负对照：这三处原始值确实不同，run1 仍应 PASS。
    chk(cs[15]["body"] != rs[15]["body"],
        "control: index 15 body raw 不同（判等域非 raw bytes；52 走 ability_layout）")
    chk(cs[19]["series"] != rs[19]["series"],
        "control: index 19 series raw 不同（ident 10 掩 series）")
    chk(cs[29]["recog"] != rs[31]["recog"],
        "control: 末帧 643 recog raw 不同（global recog 掩码）")

    # ① run1 恰好 PASS。
    res = analyze(cs, rs, rules)
    chk(res.get("verdict") == "PASS" and res.get("m") == 1
        and res.get("common_prefix") == 29 and res.get("common_suffix") == 1
        and res.get("pairs_side") == "rust",
        "run1: PASS m=1 common_prefix=29 common_suffix=1 pairs_side=rust", repr(res))

    # ② (41,10) 倒序 ⇒ FAIL。
    rev = _renumber(rs[:29] + [rs[30], rs[29]] + rs[31:])
    res = analyze(cs, rev, rules)
    chk(res.get("verdict") == "FAIL"
        and res.get("first_fail", {}).get("frame", {}).get("frame_index") == 29
        and res.get("first_fail", {}).get("ident") == 41,
        "(41,10) 倒序 ⇒ FAIL at index 29 ident 41", repr(res.get("first_fail")))

    # ③ 单 10 ⇒ FAIL。
    one10 = _renumber(rs[:29] + [rs[29]] + rs[31:])
    res = analyze(cs, one10, rules)
    chk(res.get("verdict") == "FAIL"
        and res.get("first_fail", {}).get("frame", {}).get("frame_index") == 29
        and res.get("first_fail", {}).get("ident") == 10,
        "单 10 ⇒ FAIL at index 29 ident 10", repr(res.get("first_fail")))

    # ④ 单 41 ⇒ FAIL。
    one41 = _renumber(rs[:29] + [rs[30]] + rs[31:])
    res = analyze(cs, one41, rules)
    chk(res.get("verdict") == "FAIL"
        and res.get("first_fail", {}).get("frame", {}).get("frame_index") == 29
        and res.get("first_fail", {}).get("ident") == 41,
        "单 41 ⇒ FAIL at index 29 ident 41", repr(res.get("first_fail")))

    # ⑤ 非成对 41（两个 41，没有前导 10）⇒ FAIL。
    stray41 = _renumber(rs[:29] + [rs[30], rs[30]] + rs[31:])
    res = analyze(cs, stray41, rules)
    chk(res.get("verdict") == "FAIL"
        and res.get("first_fail", {}).get("frame", {}).get("frame_index") == 29
        and res.get("first_fail", {}).get("ident") == 41,
        "非成对 41（[41,41]）⇒ FAIL at index 29 ident 41", repr(res.get("first_fail")))

    # ⑥ 差块之外改 body（643）⇒ FAIL，且定位到 643 而不是差块起点。
    b = rs[31]["body"]
    bad643 = _with_body(rs[31], bytes([b[0] ^ 0xFF]) + b[1:])
    changed = _renumber(rs[:31] + [bad643])
    res = analyze(cs, changed, rules)
    ff = res.get("first_fail") or {}
    chk(res.get("verdict") == "FAIL"
        and ff.get("frame", {}).get("ident") == 643
        and ff.get("frame", {}).get("frame_index") == 31
        and ff.get("counterpart", {}).get("frame_index") == 29,
        "改 643 body ⇒ FAIL at rust[31] vs oracle[29] ident 643", repr(ff))

    # ⑦ w1c 真实 FAIL：643 相对其它帧提前（oracle[19]=10 vs rust[19]=643）。
    wcs, wrs = _load_pair(os.path.join(w1c_dir, "csharp-frames.ndjson"),
                          os.path.join(w1c_dir, "rust-frames.ndjson"))
    chk(len(wcs) == 30 and len(wrs) == 30, "fixture: w1c 帧数 30/30",
        "cs=%d rs=%d" % (len(wcs), len(wrs)))
    res = analyze(wcs, wrs, rules)
    ff = res.get("first_fail") or {}
    chk(res.get("verdict") == "FAIL"
        and ff.get("frame", {}).get("frame_index") == 19
        and ff.get("frame", {}).get("ident") == 10
        and ff.get("counterpart", {}).get("ident") == 643,
        "w1c 真实 FAIL ⇒ FAIL at index 19（oracle 10 vs rust 643）", repr(ff))

    print("RNG_SHAPE_SELFTEST_PASS=%d FAIL=%d" % (pass_n, fail_n))
    return 0 if fail_n == 0 else 1


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv and argv[0] == "selftest":
        return cmd_selftest(argv[1:])
    parser = build_parser()
    args = parser.parse_args(argv)
    if not args.oracle or not args.rust:
        parser.error("--oracle 与 --rust 必填（或使用 `selftest`）")
    try:
        rep = check(args.oracle, args.rust, args.rules)
    except Undecidable as e:
        rep = {"format": FORMAT, "verdict": "UNDECIDABLE", "exit_code": 2,
               "kind": "input", "detail": str(e), "m": 0, "first_fail": None}
    if args.json_out:
        write_report(rep, args.json_out)
    if not args.quiet:
        print(human_summary(rep))
    return rep.get("exit_code", 2)


if __name__ == "__main__":
    sys.exit(main())
