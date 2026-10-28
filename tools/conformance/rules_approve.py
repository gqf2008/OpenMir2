#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""把 `tools/conformance/rules.json` 的 `approved_by` 按 Leader 裁定翻正。

卡：`openmir2-rules-pending-adjudication`（tier L；改判据 = 改验收口径，必须独立复审）。

裁定（2026-10-25，Leader/coordinator）：
  * **转正 50 条**：`global` 4 + `rules` 里除 ident 10/50 的 45 条 + `db` 1；
  * **保留 PENDING 2 条**：`rules` 的 ident `10` / `50`（理由见卡面 decision）；
  * **2026-10-29 追加**：ident `14` 经 owner/coordinator 裁决转正（新 decision oid），
    保留 counts+shape 全等才剔除的判据（见 compare_runs 的 ignore_rule_mismatch）。

本工具**只改 approved_by**：`action`/`head_mask`/`mask_fields`/`tier`/`reason` 一个字都不动
（要改判据内容另开卡走评审）。

用法：
  python tools/conformance/rules_approve.py --check      # 只报告（有该翻未翻的 => rc 2）
  python tools/conformance/rules_approve.py --apply      # 写回（校验不过则一个字节都不写）
  python tools/conformance/rules_approve.py --selftest   # 离线自测

退出码：0 = 全部符合；2 = 校验失败/仍需翻转（点名到条目）。
"""
import argparse
import copy
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_RULES = os.path.join(HERE, "rules.json")

#: 本卡 issue oid（`walgit collab` 的 issue entry oid；裁定记录在卡线程里）。
DECISION_OID = "062972b6581e9fe4fd15370bdbbb4f8a4e3758ce"
#: 保留 PENDING 的 `rules` ident（int）。
KEEP_PENDING_IDENTS = frozenset({10, 50})
#: 翻正后的 approved_by 模板：保留原草稿人，格式与卡面一致。
APPROVED_TMPL = ("openmir2-orchestrator (coordinator review 2026-10-25, "
                 "decision=%s; draft: %%s)" % DECISION_OID)
#: 2026-10-29 的 ident14 追加裁决（只用于 ident 14）。
DECISION_OID_14 = "81f45a499d7a25227f93bc1e933cc6f086b3b669"
APPROVED_TMPL_14 = ("openmir2-orchestrator (coordinator review 2026-10-29, "
                    "decision=%s; draft: %%s)" % DECISION_OID_14)
#: `reason` 最短长度（卡面硬要求）。
MIN_REASON_LEN = 20


def _is_pending(value):
    return str(value or "").strip().upper().startswith("PENDING")


def _draft_of(value):
    """从 `PENDING owner/coordinator review (draft: X)` 里取草稿人；取不到 => 空串。"""
    s = str(value or "")
    i = s.lower().find("draft:")
    if i < 0:
        return ""
    tail = s[i + len("draft:"):].strip()
    tail = tail.rstrip(")").strip()
    return tail


def approved_ok(value, ident=None):
    """approved_by 是否是该 ident 的正式批准格式（按 ident 绑定决策模板）。

    ident 14 只接受 2026-10-29 追加裁决；其它条目只接受 2026-10-25 全量裁定。
    不按 ident 共用两套模板会把批准谓词放宽成可借用的软化通道。
    """
    s = str(value or "")
    if _is_pending(s) or not s or "draft:" not in s or not _draft_of(s):
        return False
    old_ok = (("coordinator review 2026-10-25" in s)
              and (("decision=%s" % DECISION_OID) in s))
    new_ok = (("coordinator review 2026-10-29" in s)
              and (("decision=%s" % DECISION_OID_14) in s))
    return new_ok if ident == 14 else old_ok


def _validate_entry(entry, where, keep_pending, ident=None):
    """返回 (ok, action, message)。action ∈ {flip, ok, keep, error}。"""
    if not isinstance(entry, dict):
        return False, "error", "%s: 条目必须是对象" % where
    tier = entry.get("tier")
    reason = entry.get("reason")
    ab = entry.get("approved_by")
    if keep_pending:
        if not _is_pending(ab):
            return False, "error", ("%s: 裁定保留 PENDING，但 approved_by 已是 %r"
                                    % (where, ab))
        if not isinstance(reason, str) or len(reason.strip()) < MIN_REASON_LEN:
            return False, "error", ("%s: 保留 PENDING 的条目必须写明原因（reason >= %d 字符）"
                                    % (where, MIN_REASON_LEN))
        return True, "keep", "%s: 保留 PENDING（reason %d 字符）" % (where, len(reason))
    if tier not in ("R", "L"):
        return False, "error", "%s: tier 必须是 R/L（实际 %r）" % (where, tier)
    if not isinstance(reason, str) or len(reason.strip()) < MIN_REASON_LEN:
        return False, "error", ("%s: reason 非空且长度须 >= %d 字符（实际 %r）"
                                % (where, MIN_REASON_LEN,
                                   len(reason.strip()) if isinstance(reason, str) else reason))
    if approved_ok(ab, ident):
        return True, "ok", "%s: 已是正式批准（草稿人 %r）" % (where, _draft_of(ab))
    if not _is_pending(ab):
        return False, "error", ("%s: approved_by 既不是 PENDING 也不是本裁定格式（%r）"
                                % (where, ab))
    draft = _draft_of(ab)
    if not draft:
        return False, "error", ("%s: PENDING 条目的 approved_by 里取不到草稿人（%r）"
                                % (where, ab))
    return True, "flip", "%s: 待翻正（草稿人 %r）" % (where, draft)


def iter_entries(rules):
    """产出 (namespace, ident_or_None, index, entry, keep_pending)。"""
    for i, entry in enumerate(rules.get("global") or []):
        yield "global", None, i, entry, False
    for i, entry in enumerate(rules.get("rules") or []):
        ident = entry.get("ident") if isinstance(entry, dict) else None
        yield "rules", ident, i, entry, ident in KEEP_PENDING_IDENTS
    yield "db", None, 0, rules.get("db"), False


def check_rules(rules):
    """校验全部条目；返回 (ok, rows, to_flip)。rows 每项含 action/message。"""
    rows = []
    to_flip = []
    ok = True
    for ns, ident, idx, entry, keep in iter_entries(rules):
        where = "%s[%s]" % (ns, ident if ident is not None else idx)
        good, action, msg = _validate_entry(entry, where, keep, ident)
        rows.append({"where": where, "action": action, "message": msg})
        if not good:
            ok = False
        elif action == "flip":
            to_flip.append((ns, idx, entry))
    return ok, rows, to_flip


def apply_rules(rules):
    """在 deepcopy 上翻正；校验不过 => 抛 ValueError（一个字节都不写）。"""
    new = copy.deepcopy(rules)
    ok, rows, to_flip = check_rules(new)
    if not ok:
        raise ValueError("\n".join(r["message"] for r in rows if r["action"] == "error"))
    for ns, idx, entry in to_flip:
        order = list(entry.keys())
        draft = _draft_of(entry.get("approved_by"))
        entry["approved_by"] = ((APPROVED_TMPL_14 if entry.get("ident") == 14
                                 else APPROVED_TMPL) % draft)
        # 键序保持（approved_by 已存在，赋值不改变位置）
        assert list(entry.keys()) == order
    return new, rows


def _load(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _dump(path, data):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
        f.write("\n")


def _report(rows, stream=sys.stdout):
    for r in rows:
        print("%-8s %s" % (r["action"], r["message"]), file=stream)


def selftest():
    """离线自测：校验/翻正/幂等/保留项不动/键序不变。

    核心牙（对应卡面核心自证）：把某条已批准的 approved_by 改回 PENDING ⇒ check 必失败。
    """
    base = {
        "format": "compare-rules/1",
        "note": "selftest",
        "global": [{"fields": ["recog"], "action": "mask_fields",
                    "reason": "x" * 30, "tier": "L",
                    "approved_by": "PENDING owner/coordinator review (draft: worker-1)"}],
        "rules": [
            {"ident": 11, "action": "compare", "reason": "y" * 30, "tier": "L",
             "approved_by": "PENDING owner/coordinator review (draft: worker-2)"},
            {"ident": 10, "action": "compare", "head_mask": ["series"],
             "reason": "z" * 30, "tier": "L",
             "approved_by": "PENDING owner/coordinator review (draft: worker-3)"},
            {"ident": 14, "action": "ignore", "reason": "w" * 30, "tier": "L",
             "approved_by": "PENDING owner/coordinator review (draft: worker-4)"},
        ],
        "db": {"time_mode": "presence", "reason": "v" * 30, "tier": "L",
               "approved_by": "PENDING owner/coordinator review (draft: worker-5)"},
    }
    checks = []

    def chk(cond, msg):
        checks.append((cond, msg))

    ok, rows, to_flip = check_rules(base)
    chk(ok and len(to_flip) == 4,
        "合法输入：global/rules(11/14)/db 待翻正，10 保留（rows=%d）" % len(rows))
    new, _ = apply_rules(base)
    chk(new["global"][0]["approved_by"].endswith("draft: worker-1)")
        and "coordinator review 2026-10-25" in new["global"][0]["approved_by"]
        and ("decision=%s" % DECISION_OID) in new["global"][0]["approved_by"],
        "翻正格式：coordinator review + decision oid + 原草稿人")
    chk(new["rules"][0]["approved_by"] != new["rules"][1]["approved_by"]
        and new["rules"][1]["approved_by"].startswith("PENDING")
        and "coordinator review 2026-10-29" in new["rules"][2]["approved_by"]
        and ("decision=%s" % DECISION_OID_14) in new["rules"][2]["approved_by"],
        "ident 10 保持 PENDING；14 按 2026-10-29 裁决翻正；其它翻正")
    ok2, rows2, to_flip2 = check_rules(new)
    chk(ok2 and not to_flip2, "幂等：翻正后再 check 无需再改")
    chk(list(new["rules"][0].keys()) == list(base["rules"][0].keys()),
        "键序不变（只改 approved_by 的值）")

    bad_tier = copy.deepcopy(base); bad_tier["rules"][0]["tier"] = "X"
    ok3, rows3, _ = check_rules(bad_tier)
    chk(not ok3 and any("tier" in r["message"] for r in rows3), "tier 非法 => 拒")

    short = copy.deepcopy(base); short["rules"][0]["reason"] = "too short"
    ok4, rows4, _ = check_rules(short)
    chk(not ok4 and any("reason" in r["message"] for r in rows4), "reason 过短 => 拒")

    no_draft = copy.deepcopy(base)
    no_draft["rules"][0]["approved_by"] = "PENDING"
    ok5, rows5, _ = check_rules(no_draft)
    chk(not ok5 and any("草稿人" in r["message"] for r in rows5), "PENDING 缺草稿人 => 拒")

    # 核心牙：已批准改回 PENDING ⇒ check 必须失败（且 --apply 不写）
    flip_back = copy.deepcopy(new)
    flip_back["rules"][0]["approved_by"] = (
        "PENDING owner/coordinator review (draft: worker-2)")
    ok6, rows6, flip6 = check_rules(flip_back)
    chk(ok6 and len(flip6) == 1, "把一条已批准改回 PENDING => 被识别为待翻正（非静默放过）")

    # keep-pending 条目若被误翻 => 拒
    wrong_keep = copy.deepcopy(new)
    wrong_keep["rules"][1]["approved_by"] = APPROVED_TMPL % "worker-3"
    ok7, rows7, _ = check_rules(wrong_keep)
    chk(not ok7 and any("保留 PENDING" in r["message"] for r in rows7),
        "保留项被翻正 => 拒")

    # F3 正向牙：按 ident 绑定的正确模板必须判 ok（ident14 → 10-29、非 14 → 10-25）。
    ok_p, rows_p, _ = check_rules(new)
    by_where = {r["where"]: r["action"] for r in rows_p}
    chk(ok_p and by_where.get("rules[14]") == "ok" and by_where.get("rules[11]") == "ok",
        "按 ident 绑定的正确模板（14→10-29、非 14→10-25）=> ok")

    # F3 反向牙 a：2026-10-29 的模板只能用于 ident14，贴到其它 ident => 拒。
    wrong_14 = copy.deepcopy(new)
    wrong_14["rules"][0]["approved_by"] = APPROVED_TMPL_14 % "worker-2"
    ok9, rows9, _ = check_rules(wrong_14)
    chk(not ok9 and any("既不是 PENDING" in r["message"] for r in rows9),
        "ident14 的 10-29 模板贴到非 14 条目 => 拒")

    # F3 反向牙 b：ident14 用 2026-10-25 模板（= 其它条目的模板）=> 拒。
    wrong_14_old = copy.deepcopy(new)
    wrong_14_old["rules"][2]["approved_by"] = APPROVED_TMPL % "worker-4"
    ok10, rows10, _ = check_rules(wrong_14_old)
    chk(not ok10 and any("既不是 PENDING" in r["message"] for r in rows10),
        "ident14 用 10-25 模板 => 拒")

    # 非 PENDING 且非本格式 => 拒（不许静默覆盖别人的批准）
    foreign = copy.deepcopy(base)
    foreign["rules"][0]["approved_by"] = "someone else approved"
    ok8, rows8, _ = check_rules(foreign)
    chk(not ok8 and any("既不是 PENDING" in r["message"] for r in rows8),
        "他人批准格式 => 拒（不覆盖）")

    for cond, msg in checks:
        print(("PASS " if cond else "FAIL ") + msg)
    failed = [m for c, m in checks if not c]
    print("rules_approve selftest: %s (%d checks)" %
          ("OK" if not failed else "FAIL", len(checks)))
    return 0 if not failed else 2


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--rules", default=DEFAULT_RULES)
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not (args.check or args.apply):
        ap.error("需要 --check / --apply / --selftest 之一")
    rules = _load(args.rules)
    ok, rows, to_flip = check_rules(rules)
    _report(rows)
    n_keep = sum(1 for r in rows if r["action"] == "keep")
    print("SUMMARY ok=%s flip=%d keep=%d errors=%d"
          % (ok, len(to_flip), n_keep,
             sum(1 for r in rows if r["action"] == "error")))
    if args.apply:
        if not ok:
            print("RED: 校验未过，未写入任何字节", file=sys.stderr)
            return 2
        new, _ = apply_rules(rules)
        _dump(args.rules, new)
        print("APPLIED: %d 条 approved_by 翻正 -> %s" % (len(to_flip), args.rules))
        ok2, rows2, flip2 = check_rules(_load(args.rules))
        print("RECHECK ok=%s flip=%d" % (ok2, len(flip2)))
        return 0 if ok2 and not flip2 else 2
    return 0 if (ok and not to_flip) else 2


if __name__ == "__main__":
    sys.exit(main())
