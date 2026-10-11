#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""M4② 关键 NPC 用例 runner（卡 `openmir2-m4-parity-cases`）。

一条命令：
  python -I tests/parity/M4/run_npc_cases.py

- `runner == "flow"` 的用例立即跑（不需两栈/端口）：两侧 harness =
  `tools/script-parity-cs run`（C# 参照）与 `mir2-script-tool flow-run`（Rust），
  同一脚本 + 同一 label + 同一初始状态，逐行 diff（复用已入库的 `tools/flow-diff.py`）。
- `runner == "window"` 的用例需要 M2 真机验收窗口释放（5000/7000/7100/7200）+ 两侧栈 +
  DB 作用域比对：本轮**只登记判据、不计通过**（§「等窗口」），避免抢占别车道的端口。

退出码：0 = 所有可跑用例一致且无红；1 = 有可跑用例不一致；2 = 用法/环境错误。
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

HERE = Path(__file__).resolve().parent            # tests/parity/M4
PARITY = HERE.parent
MIR2RS = PARITY.parents[1]
FLOW_DIFF = MIR2RS / "tools" / "flow-diff.py"
EVIDENCE = PARITY / "evidence" / "M4"
CASES = HERE / "npc-cases.json"
WINDOW_GATE = HERE / "npc_window_readiness.py"


def main():
    ap = argparse.ArgumentParser(description="M4② 关键 NPC 用例 runner")
    ap.add_argument("--cases", default=str(CASES))
    ap.add_argument("--out", default=str(EVIDENCE / "npc-cases.txt"))
    args = ap.parse_args()

    data = json.loads(Path(args.cases).read_text(encoding="utf-8"))
    cases = data["cases"]
    flow = [c for c in cases if c.get("runner") == "flow"]
    window = [c for c in cases if c.get("runner") == "window"]

    out = []
    out.append("# M4② 关键 NPC 用例（6 类：商店 / 传送 / 修理 / 仓库 / 行会 / 任务）")
    out.append("")
    out.append(f"- 用例定义：`{CASES.relative_to(MIR2RS).as_posix()}`")
    out.append(f"- 语料：`E:/MirServer/M2GameSvr/Envir`（内容冻结）")
    out.append("- 可跑（port-free）用例：两侧 harness 同脚本 + 同 label + 同初始状态，逐行 diff。")
    out.append("- 等窗口用例：需 M2 真机验收窗口 + 两侧栈 + DB 作用域比对，本轮登记判据、不计通过。")
    out.append("")

    bad = 0
    if flow:
        work = MIR2RS / "target" / "m4"
        work.mkdir(parents=True, exist_ok=True)
        tmp = work / "npc-cases-flow.json"
        payload = []
        for c in flow:
            item = {k: c[k] for k in ("name", "script", "label", "gold", "items", "player_items")
                    if k in c}
            payload.append(item)
        tmp.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8", newline="\n")
        out.append("## 可跑用例（port-free）")
        out.append("")
        out.append(f"```\npython -I tests/parity/M4/run_npc_cases.py\n# 等价于：python -I tools/flow-diff.py target/m4/{tmp.name}\n```")
        out.append("")
        r = subprocess.run([sys.executable, "-I", str(FLOW_DIFF), str(tmp)],
                           cwd=MIR2RS, capture_output=True, text=True,
                           encoding="utf-8", errors="replace")
        body = (r.stdout + r.stderr).rstrip()
        out.append("```")
        out.append(body)
        out.append("```")
        out.append("")
        if r.returncode != 0:
            bad = 1
        for c in flow:
            out.append(f"### {c['category']}：{c['name']}")
            out.append(f"- 失败怎么判红：{c['red_criteria']}")
            out.append(f"- 期望（登记）：{c.get('expected', [])}")
            out.append("")

    out.append("## 等窗口用例（本轮只登记，不计通过）")
    out.append("")
    out.append("> 三条窗口用例**当前不可跑**——可跑性由 `npc_window_readiness.py` 机判并登记在"
               "`evidence/M4/npc-window-readiness.txt`（两条路线的前置都未满足，见下）。")
    out.append("")
    for c in window:
        out.append(f"### {c['category']}：{c['name']}")
        out.append(f"- 脚本/label：`{c['script']}` `{c['label']}`")
        out.append(f"- 初始状态：gold={c.get('gold')} items={c.get('items')} player_items={c.get('player_items')}")
        out.append(f"- 期望副作用：{c.get('expected')}")
        out.append(f"- 为什么等窗口：{c['window_reason']}")
        out.append(f"- 失败怎么判红：{c['red_criteria']}")
        out.append("- 窗口释放后怎么跑：")
        out.append("  1. 见 `lane-status.log` + 端口，确认 5000/7000/7100/7200 空闲；")
        out.append("  2. 起两侧栈（C# 参照 / Rust），用与上表同一脚本、同一 label、同一初始状态交互；")
        out.append("  3. 行为 diff：逐行比 take/give/messagebox 序列；DB 作用域：")
        out.append("     `python -I tests/parity/m3/scoped_diff.py diff <a.jsonl> <b.jsonl> --canonical`；")
        out.append("  4. 把两侧原始输出与 DB 快照 hash 写入 `evidence/M4/npc-cases-window.txt`。")
        out.append("")

    # 窗口可跑性门（源码能力检查，快）：把「为什么现在跑不了」机判出来并登记。
    # 注意：这里用 **源码模式**（--no-run-cs）且输出到 target/（不覆盖全量证据
    # `evidence/M4/npc-window-readiness.txt`，那份由显式全量跑生成、含 C# 夹具行为复现）。
    out.append("## 窗口可跑性门（`npc_window_readiness.py --no-run-cs`）")
    out.append("")
    gate_out = MIR2RS / "target" / "m4" / "npc-window-readiness-src.txt"
    if WINDOW_GATE.is_file():
        gate_out.parent.mkdir(parents=True, exist_ok=True)
        try:
            rg = subprocess.run(
                [sys.executable, "-I", str(WINDOW_GATE), "--no-run-cs", "--out", str(gate_out)],
                cwd=MIR2RS, capture_output=True, text=True, encoding="utf-8", errors="replace",
                timeout=300)
            gtext = (rg.stdout or "").rstrip()
            verdict_line = next((l for l in reversed(gtext.splitlines())
                                 if l.startswith(("BLOCKED:", "READY:", "PREREQ_MISSING:"))), "")
            out.append(f"- 结果：{verdict_line or '(无裁决行)'}")
            out.append("- 完整证据（含 C# 夹具行为复现）："
                       "`tests/parity/evidence/M4/npc-window-readiness.txt`")
            if rg.returncode not in (0, 3):
                out.append(f"- 注意：门退出码 {rg.returncode}（非 0/3，可能是环境 PREREQ）")
        except Exception as e:  # noqa: BLE001
            out.append(f"- 门执行失败：{e}")
    else:
        out.append(f"- 缺门脚本 {WINDOW_GATE}")
    out.append("")

    text = "\n".join(out) + "\n"
    Path(args.out).write_text(text, encoding="utf-8", newline="\n")
    print(text)

    print(f"== M4② 结果：可跑用例 {len(flow)}（{'GREEN' if bad == 0 else 'RED'}）"
          f" / 等窗口用例 {len(window)}（PENDING WINDOW）==")
    return bad


if __name__ == "__main__":
    raise SystemExit(main())
