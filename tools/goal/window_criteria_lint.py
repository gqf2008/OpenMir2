#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""窗口判据模板的**只告警**校验（卡 `openmir2-window-criteria-template`）。

模板见 `docs/window-criteria-template.md`（六字段）。本工具：

  python -I -B tools/goal/window_criteria_lint.py --text <文件|->   # 输出 MISSING 行
  python -I -B tools/goal/window_criteria_lint.py --selftest        # 自测（合格 0 warning / 不合格点名）

**契约**：只告警——退出码恒为 0；不改任何文件；不参与 `goal_gate` 的 PASS/FAIL。
"""

import argparse
import re
import sys

#: 六个字段的识别规则（任一命中即算「写了这一项」）。名字与模板 §1 一致。
FIELDS = [
    ("precondition", "① 前置（夹具/会话数/权限）",
     re.compile(r"前置|夹具|会话数|权限|profile|placement|inventory|magic|部署件", re.I)),
    ("command", "② 可复跑命令",
     re.compile(r"python|card_window|mir2_probe|powershell|ps1|cargo |场景待建", re.I)),
    ("expectation", "③ 期望（ident/字段/字节/DB）",
     re.compile(r"SM_[A-Z]+|ident|帧序|字节|字段级|characters_|DB 后态|recog=", re.I)),
    ("criterion", "④ 判据（verdict/rc）",
     re.compile(r"compare_runs|diffs\s*=\s*0|verdict|rc\s*[=≠]|PASS|FAIL|UNDECIDABLE", re.I)),
    ("mask", "⑤ 掩码/不可比字段",
     re.compile(r"掩码|mask|不可比|rules\.json|无掩码", re.I)),
    ("owner_status", "⑥ 负责人与状态",
     re.compile(r"负责人|owner|未跑|已跑|window_required|状态[：:]", re.I)),
]


def lint(text):
    """返回缺失字段的 [(key, label)] 列表（空 = 六项齐全）。"""
    missing = []
    for key, label, pat in FIELDS:
        if not pat.search(text or ""):
            missing.append((key, label))
    return missing


def report(text, out=None):
    out = out or sys.stdout
    missing = lint(text)
    for i, (_k, label) in enumerate(missing, start=1):
        print("MISSING %d: %s" % (i, label), file=out)
    if not missing:
        print("OK: 六字段齐全（本工具只告警，不参与门禁）", file=out)
    return missing


def selftest():
    good = (
        "窗口项（R）：登录帧序\n"
        "① 前置：profile=alt；placement map=0103 x=19 y=19；单会话；无需 inventory。\n"
        "② 命令：python -I -B tools/conformance/card_window.py --scenario login_only.json --profile alt\n"
        "③ 期望：SM_SENDUSEITEMS(621)/SM_SENDMYMAGIC(211) 位置一致；SM_LOGON recog=191 帧序逐字节。\n"
        "④ 判据：compare_runs verdict=PASS 且 diffs=0。\n"
        "⑤ 掩码：SM_TURN(10) 的 series 按 rules.json ident 10 掩掉。\n"
        "⑥ 负责人：worker-24；状态：未跑。\n"
    )
    bad = ("节拍门经定量核查无差异；余量=按评审口径的 HP 受控重采样（真机窗口），并入窗口项")
    checks = []
    m_good = lint(good)
    checks.append((not m_good, "① 合格样例 ⇒ 0 warning（实际缺 %r）" % (m_good,)))
    m_bad = lint(bad)
    keys = {k for k, _l in m_bad}
    checks.append(("command" in keys and "criterion" in keys and len(m_bad) >= 3,
                   "② 不合格样例 ⇒ 点名缺 command/criterion 且 ≥3 项（实际 %r）"
                   % (sorted(keys),)))
    # 只少一项时必须只报那一项
    partial = good.replace("⑤ 掩码：SM_TURN(10) 的 series 按 rules.json ident 10 掩掉。\n", "")
    m_part = lint(partial)
    checks.append(([k for k, _l in m_part] == ["mask"],
                   "③ 抽掉掩码一行 ⇒ 只报 ⑤（实际 %r）" % ([k for k, _l in m_part],)))
    failed = [msg for ok, msg in checks if not ok]
    for ok, msg in checks:
        print("  [%s] %s" % ("ok" if ok else "FAIL", msg))
    if failed:
        print("window_criteria_lint selftest: FAIL")
        return 2
    print("window_criteria_lint selftest: OK (%d checks)" % len(checks))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="窗口判据模板的只告警校验")
    ap.add_argument("--text", help="状态文本文件；`-` 表示 stdin")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if not args.text:
        ap.print_help()
        return 0
    if args.text == "-":
        text = sys.stdin.read()
    else:
        with open(args.text, encoding="utf-8") as fh:
            text = fh.read()
    report(text)
    return 0     # 只告警：永不因缺字段返回非零


if __name__ == "__main__":
    sys.exit(main())
