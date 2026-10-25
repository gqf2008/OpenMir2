#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""run_batch 的外层 rc 归因（卡 `openmir2-runbatch-rc-attribution`）。

背景：`card_window.main` 曾用 `int(summary.get("exit_code") or 2)`，PASS（0）也会因
`0 or 2` 退出 2 ⇒ run_batch 的 outer_rc 恒为 2（真机探针见
`docs/runbatch-rc-attribution-20261025.md`）。修好之后 outer_rc 恢复语义，本模块把
「外层 rc」纳入判定：

  * `outer_rc == 124` => `TIMEOUT`；`137` => `KILLED`（不论自证如何）；
  * `verdict == PASS` 且（`outer_rc ∉ {0, 2}`，或 `outer_rc == 2`）：
      - 自证 `exit_code == 0 且 compare_rc == 0` ⇒ `PASS(outer_rc_exception)` + note
        （防御性规则，防 falsy-zero 类回归再次静默给绿）；
      - 否则 ⇒ `UNDECIDABLE` + note（不得裸判 PASS）；
  * 其它情况保持 `verdict` 不变。
"""
import argparse
import io
import json
import os
import sys


def classify(outer_rc, self_exit, verdict, compare_rc):
    """返回 `(verdict2, note)`；note 为空串表示无例外。"""
    if outer_rc == 124:
        return "TIMEOUT", "outer_rc=124（外层超时；card_window 自证 verdict=%s）" % verdict
    if outer_rc == 137:
        return "KILLED", "outer_rc=137（被强杀；card_window 自证 verdict=%s）" % verdict
    if verdict == "PASS" and outer_rc != 0:
        if self_exit == 0 and compare_rc == 0:
            return ("PASS(outer_rc_exception)",
                    "outer_rc=%r 但自证 exit=0/compare_rc=0 ⇒ 判 PASS(outer_rc_exception)"
                    "（历史上 card_window 的 `exit_code or 2` falsy-zero bug 会让 PASS 也退 2）"
                    % (outer_rc,))
        return ("UNDECIDABLE",
                "outer_rc=%r 且自证 exit=%r/compare_rc=%r ⇒ 外层失败未被自证解释，拒判 PASS"
                % (outer_rc, self_exit, compare_rc))
    return verdict or "-", ""


TOTAL_KEYS = ("PASS", "FAIL", "UNDECIDABLE", "NOT_RUN", "TIMEOUT", "KILLED", "OTHER")


def totals_for(verdicts):
    """按 verdict 列表计数（review F1：未知字符串必须落到 OTHER，不能兜进已有桶）。

    `PASS(...)`（例外 PASS）计入 PASS；`-`（无自证）计入 UNDECIDABLE；
    其余未识别字符串计入 OTHER —— 调用方见 `OTHER>0` 即视为异常并以非 0 退出。
    """
    tot = {k: 0 for k in TOTAL_KEYS}
    for v in verdicts:
        if v in tot and v != "OTHER":
            tot[v] += 1
        elif isinstance(v, str) and v.startswith("PASS("):
            tot["PASS"] += 1
        elif v == "-":
            tot["UNDECIDABLE"] += 1
        else:
            # **None（缺 verdict 键）与未知字符串都算 OTHER**：行本身畸形，必须判红
            # （review `33309b67` F1 的语义；`-` 才是「无自证」的合法值）。
            tot["OTHER"] += 1
    return tot


def _int_or_none(v):
    try:
        return int(v)
    except (TypeError, ValueError):
        return None


def selftest():
    out = []

    def chk(cond, msg):
        out.append((bool(cond), msg))

    v, note = classify(124, 0, "PASS", 0)
    chk(v == "TIMEOUT" and "124" in note, "RCA1 outer_rc=124 ⇒ TIMEOUT（即使自证 PASS）")
    v, note = classify(137, 2, "FAIL", 1)
    chk(v == "KILLED" and "137" in note, "RCA2 outer_rc=137 ⇒ KILLED")
    v, note = classify(2, 0, "PASS", 0)
    chk(v == "PASS(outer_rc_exception)" and "falsy-zero" in note,
        "RCA 牙1 outer_rc=2 + 自证 exit=0/compare=0 ⇒ PASS(outer_rc_exception)（防回归）")
    v, note = classify(2, 2, "PASS", 0)
    chk(v == "UNDECIDABLE" and "拒判 PASS" in note,
        "RCA 牙2 outer_rc=2 但自证 exit=2 ⇒ UNDECIDABLE（不得裸判 PASS）")
    v, note = classify(3, 0, "PASS", 0)
    chk(v == "PASS(outer_rc_exception)" and note, "RCA 牙3 outer_rc=3（非 0/2）+ 自证绿 ⇒ 例外 PASS")
    v, note = classify(0, 0, "PASS", 0)
    chk(v == "PASS" and note == "", "RCA 牙4 outer_rc=0 ⇒ 原样保持")
    v, note = classify(0, 2, "FAIL", 1)
    chk(v == "FAIL" and note == "", "RCA 牙5 FAIL 且 outer_rc=0 ⇒ 原样保持（不升也不降）")
    vs = ["PASS", "FAIL", "UNDECIDABLE", "NOT_RUN", "TIMEOUT", "KILLED", "PASS(x)", "-"]
    tot = totals_for(vs)
    chk(sum(tot.values()) == len(vs) and tot["OTHER"] == 0
        and tot["PASS"] == 2 and tot["UNDECIDABLE"] == 2,
        "RCA 牙6 totals：已知 verdict（含 PASS(...)/-）分桶自洽、OTHER=0")
    bad = totals_for(["PASS", "WAT", "PASSX"])
    chk(bad["OTHER"] == 2 and sum(bad.values()) == 3,
        "RCA 牙7 未知 verdict 字符串 ⇒ OTHER 计数（守卫不得恒真）")
    here = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    out.extend(_builder_guard_tooth(here))
    passed = sum(1 for c, _ in out if c)
    for c, msg in out:
        print(("PASS " if c else "FAIL ") + msg)
    print(("%s: rc_attribution selftest %d/%d"
           % ("GREEN" if passed == len(out) else "RED", passed, len(out))))
    return 0 if passed == len(out) else 2


def _builder_guard_tooth(repo_root):
    """牙：**驱动 run_batch.sh 里真实的 summary-builder heredoc**（不是仿制品）。

    做法：从 `tools/window/run_batch.sh` 抽出 heredoc 原文 → 写成临时脚本 →
    对两组合成 batch.jsonl 运行：
      * 含 `verdict="BOGUS"` ⇒ 必须以 **3** 退出且**不写** batch-summary.json；
      * 全部合法状态 ⇒ rc=0 且 `totals_consistent is True`。
    """
    import os
    import re
    import shutil
    import subprocess
    import sys
    import tempfile

    rb = os.path.join(repo_root, "tools", "window", "run_batch.sh")
    src = io.open(rb, encoding="utf-8").read()
    NL = "\n"
    m = re.search(r"summary-build\.log\" 2>&1 <<'PY'" + NL + "(.*?)\nPY\n", src, re.S)
    if not m:
        return [(False, "RCA 牙8 无法从 run_batch.sh 抽出 summary-builder heredoc")]
    builder_src = m.group(1)
    tmp = tempfile.mkdtemp(prefix="rc-builder-tooth-")
    out = []
    try:
        bpath = os.path.join(tmp, "builder.py")
        with open(bpath, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(builder_src + "\n")
        names = ["a", "b"]
        manifest = os.path.join(tmp, "manifest.json")
        with open(manifest, "w", encoding="utf-8") as fh:
            json.dump({"scenarios": [{"name": n} for n in names]}, fh)

        def run_case(tag, rows):
            rd = os.path.join(tmp, tag)
            os.makedirs(rd)
            with open(os.path.join(rd, "batch.jsonl"), "w", encoding="utf-8") as fh:
                for r in rows:
                    fh.write(json.dumps(r) + "\n")
            try:
                p = subprocess.run([sys.executable, "-I", "-B", bpath, rd, manifest,
                                    "card", "head"], capture_output=True, text=True,
                                   stdin=subprocess.DEVNULL, timeout=120, cwd=repo_root)
            except subprocess.TimeoutExpired:
                class _Timeout:  # 与真实 builder 的 rc 通道同形
                    returncode = 124
                    stdout = ""
                    stderr = "TIMEOUT>120s"
                p = _Timeout()
            return p, os.path.join(rd, "batch-summary.json")

        bad = [{"n": 1, "name": "a", "verdict": "BOGUS", "skipped": False},
               {"n": 2, "name": "b", "verdict": "PASS", "skipped": False}]
        p, sp = run_case("bad", bad)
        out.append((p.returncode == 3 and not os.path.exists(sp),
                    "RCA 牙8 真实 builder：verdict=BOGUS ⇒ rc=3 且不写 batch-summary.json"))
        good = [{"n": 1, "name": "a", "verdict": "PASS", "skipped": False},
                {"n": 2, "name": "b", "verdict": "TIMEOUT", "skipped": False}]
        p, sp = run_case("good", good)
        ok = False
        if p.returncode == 0 and os.path.exists(sp):
            d = json.load(open(sp, encoding="utf-8"))
            ok = (d.get("totals_consistent") is True and d.get("totals", {}).get("TIMEOUT") == 1)
        out.append((ok, "RCA 牙8b 真实 builder：合法状态 ⇒ rc=0、totals_consistent=true、TIMEOUT 成桶"))
        for n in names:
            pass
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return out


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--outer-rc", required=False)
    ap.add_argument("--self-exit", required=False)
    ap.add_argument("--verdict", required=False)
    ap.add_argument("--compare-rc", required=False)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    v, note = classify(_int_or_none(args.outer_rc),
                       _int_or_none(args.self_exit),
                       args.verdict or "-",
                       _int_or_none(args.compare_rc))
    print("%s\t%s" % (v, note))
    return 0


if __name__ == "__main__":
    sys.exit(main())
