#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""scenario_undecidable_audit — 封存场景的「登记缺失必红」牙（卡 openmir2-pickup-item-reachability）。

背景：`pickup_item` 的端到端成功支在 oracle 与 Rust 两侧都**结构性不可达**
（T-19/T-25：`MapItem.ItemId` 恒 0 ⇒ `Envir.AddItemToMap` 恒 false ⇒ 地面永远无物）。
本卡按 **乙** 路线把它登记为「已接受非目标」，而不是伪造成可达：

  1. `tools/probe/scenarios/pickup_item.json` 顶层必须带合法 `blocked`
     （`reason` + `missing_fixture`，`card_window.py` 见到即 rc=2、不起栈）；
  2. `blocked.reason` 必须引用 **T-19** 与 **T-25**（否则登记退化成一句无来源的说明）；
  3. §7.4 清单（`docs/window-batch-plan-20261023.md`）必须有一行同时出现
     `pickup_item` + `T-19`/`T-25` + `结构性不可判`（把「已封存」写回给人看的清单，
     否则清单仍把它当可跑场景）。

判据/退出码（与 tools/hygiene 其它门禁同口径）：
  0  GREEN — 三条登记都在；
  1  RED   — 任一登记缺失/退化（点名缺哪条）；
  2  PREREQ/usage — 文件不存在、JSON 坏、参数错。

`--selftest` 在临时目录跑 6 组变异，证明「删掉任一条登记 ⇒ RED」不是空话：
  G  good（三条齐全）⇒ GREEN；
  M1 删掉 `blocked`；
  M2 `blocked.reason` 不含 T-25；
  M3 `blocked.missing_fixture` 为空；
  M4 §7.4 行删掉「结构性不可判」；
  M5 §7.4 行不含 T-19/T-25。

只读 Python 标准库；`python -I tools/hygiene/scenario_undecidable_audit.py check` 可直接复跑。
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile

SCRIPT = os.path.abspath(__file__)
REPO = os.path.dirname(os.path.dirname(os.path.dirname(SCRIPT)))
SCENARIO = os.path.join("tools", "probe", "scenarios", "pickup_item.json")
DOC = os.path.join("docs", "window-batch-plan-20261023.md")
SCENE = "pickup_item"
ANCHORS = ("T-19", "T-25")
DOC_MARK = "结构性不可判"


def _load_json(path):
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def audit(scenario_path, doc_path):
    """返回 (errors, checked)；errors 为空 = GREEN。纯函数，便于 selftest。"""
    errors = []
    checked = {"scenario": scenario_path, "doc": doc_path}

    try:
        doc = _load_json(scenario_path)
    except FileNotFoundError:
        return ["scenario 文件不存在：%s" % scenario_path], checked
    except (OSError, ValueError) as ex:
        return ["scenario 无法读取/解析：%s（%s）" % (scenario_path, ex)], checked

    blocked = doc.get("blocked")
    if not isinstance(blocked, dict):
        errors.append("scenario 缺顶层 blocked 对象（登记缺失 ⇒ 场景仍被当作可跑）")
    else:
        reason = blocked.get("reason")
        fixture = blocked.get("missing_fixture")
        if not isinstance(reason, str) or not reason.strip():
            errors.append("scenario.blocked.reason 为空/非字符串")
        else:
            for anchor in ANCHORS:
                if anchor not in reason:
                    errors.append("scenario.blocked.reason 未引用 %s（证据链缺失）" % anchor)
        if not isinstance(fixture, str) or not fixture.strip():
            errors.append("scenario.blocked.missing_fixture 为空/非字符串")

    try:
        with open(doc_path, "r", encoding="utf-8") as fh:
            doc_lines = fh.read().split("\n")
    except FileNotFoundError:
        return errors + ["§7.4 文档不存在：%s" % doc_path], checked
    except OSError as ex:
        return errors + ["§7.4 文档无法读取：%s（%s）" % (doc_path, ex)], checked

    hits = [ln for ln in doc_lines if SCENE in ln and DOC_MARK in ln]
    if not hits:
        errors.append("§7.4 清单没有把 %r 标注为「%s」（登记缺失）" % (SCENE, DOC_MARK))
    else:
        anchored = [ln for ln in hits
                    if any(a in ln for a in ANCHORS)]
        if not anchored:
            errors.append("§7.4 的 %r 封存行未引用 %s（证据链缺失）"
                          % (SCENE, "/".join(ANCHORS)))
    return errors, checked


def _selftest():
    """变异必红：每条登记单独删掉都必须 RED；好样本必须 GREEN。"""
    good_scene = {
        "name": SCENE,
        "blocked": {
            "reason": "%s（T-19/T-25）：oracle MapItem.ItemId 恒 0。" % DOC_MARK,
            "missing_fixture": "需要 ItemId != 0 的地物；oracle 无生产路径。",
        },
        "steps": [],
    }
    good_doc = "# 7.4\n| `%s` | 封存 | %s（T-19/T-25）|\n" % (SCENE, DOC_MARK)

    cases = []
    cases.append(("G good", good_scene, good_doc, []))
    m1 = json.loads(json.dumps(good_scene))
    del m1["blocked"]
    cases.append(("M1 删 blocked", m1, good_doc, ["缺顶层 blocked"]))
    m2 = json.loads(json.dumps(good_scene))
    m2["blocked"]["reason"] = "%s：只引 T-19" % DOC_MARK
    cases.append(("M2 reason 缺 T-25", m2, good_doc, ["未引用 T-25"]))
    m3 = json.loads(json.dumps(good_scene))
    m3["blocked"]["missing_fixture"] = "   "
    cases.append(("M3 missing_fixture 空", m3, good_doc, ["missing_fixture 为空"]))
    bad_doc_1 = "# 7.4\n| `%s` | 封存 | T-19/T-25 |\n" % SCENE
    cases.append(("M4 §7.4 缺「%s」" % DOC_MARK, good_scene, bad_doc_1,
                  ["没有把"]))
    bad_doc_2 = "# 7.4\n| `%s` | 封存 | %s |\n" % (SCENE, DOC_MARK)
    cases.append(("M5 §7.4 缺 T-19/T-25", good_scene, bad_doc_2,
                  ["未引用 T-19/T-25"]))

    tmp = tempfile.mkdtemp(prefix="scenario-undecidable-selftest-")
    try:
        failures = []
        for label, scene_doc, doc_text, needles in cases:
            sp = os.path.join(tmp, "scene-%d.json" % len(failures))
            dp = os.path.join(tmp, "doc-%d.md" % len(failures))
            with open(sp, "w", encoding="utf-8", newline="\n") as fh:
                json.dump(scene_doc, fh, ensure_ascii=False)
            with open(dp, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(doc_text)
            errors, _ = audit(sp, dp)
            if not needles:
                ok = not errors
                detail = "errors=%r" % (errors,)
            else:
                ok = all(any(n in e for e in errors) for n in needles)
                detail = "errors=%r" % (errors,)
            print("%-28s %s  %s" % (label, "PASS" if ok else "FAIL", detail))
            if not ok:
                failures.append(label)
        print("selftest: %d/%d PASS" % (len(cases) - len(failures), len(cases)))
        return 0 if not failures else 1
    finally:
        for name in os.listdir(tmp):
            try:
                os.unlink(os.path.join(tmp, name))
            except OSError:
                pass
        os.rmdir(tmp)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("mode", nargs="?", default="check", choices=("check", "selftest"))
    ap.add_argument("--scenario", default=os.path.join(REPO, SCENARIO))
    ap.add_argument("--doc", default=os.path.join(REPO, DOC))
    args = ap.parse_args(argv)

    if args.mode == "selftest":
        return _selftest()

    errors, checked = audit(args.scenario, args.doc)
    print("scenario: %s" % checked["scenario"])
    print("doc:      %s" % checked["doc"])
    if errors:
        for e in errors:
            print("RED: %s" % e)
        return 1
    print("GREEN: %r 封存登记完整（blocked + T-19/T-25 + §7.4 勾注）" % SCENE)
    return 0


if __name__ == "__main__":
    sys.exit(main())
