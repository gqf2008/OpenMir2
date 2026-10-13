#!/usr/bin/env python3
"""M3 夹具矩阵与就绪度门（fixture_matrix.py）——十项系统 + Claim B 的夹具三件套单一真源。

背景：M3「逐系统对齐」每个系统都有一副动作序列夹具（`run_*_pair.ps1` + 比较器
+ 场景），它们**分散在不同 branch / 不同 merge 片**里，合并顺序又相互叠加（后一张
的夹具常基于前一张未合的 head）。owner 每次做「夹具/判据链未劣化」复核都要**手敲
七条命令**；M3 总验收（`openmir2-m3-final-acceptance`）也需要一个**只看一条命令**就能
回答「哪些系统夹具已就绪、哪些还在途中、有没有哪一副坏掉」的机判入口。

本文件就是那个入口：把「夹具三件套在哪、对应哪张卡 / 哪个 whitelist 登记号」写成
一份**登记表**（`MATRIX`，本文件内联，避免第二真源），并对当前树做**只读**核对：

  * 三件套（runner + comparator + scenario）**全在场** ⇒ `READY`，且比较器 `selftest`
    必须 `rc=0`（fail-closed：在场却坏 = `RED`）；
  * 三件套**全不在场** ⇒ `PENDING`（尚未合并；默认只登记，`--require-all` 时判 `RED`）；
  * **部分在场**（三件套缺一） ⇒ `INCONSISTENT` = `RED`（半拉子合并/漏提交，绝不当绿）。

命令：
  python -I fixture_matrix.py check [--require-all] [--json OUT]
  python -I fixture_matrix.py list  [--json OUT]
  python -I fixture_matrix.py selftest

口径：**只读**（check 只读文件 + 跑各比较器自带的 `selftest`，不写任何仓库文件、
不起栈、不占端口）；`selftest` 在临时目录里造合成件，含 fail-closed 与改坏必红。
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))

# ---------------------------------------------------------------------------
# 单一真源：M3 夹具矩阵（十项系统 + Claim B 场景账本）
#
# 每项：
#   key        §6 编号（1..10 + "B"）
#   system     人读系统名
#   card       夹具承接卡（thread id）
#   t          whitelist.md 里的登记号（None = 该夹具无独立 T-*，不检查）
#   runner     actions-sequence 驱动（-SelfCheckNoStack 无栈自检）
#   comparator 比较器/门（selftest 必须 rc=0）
#   scenario   场景/种子定义（可为 None）
#   note       一句话口径
# ---------------------------------------------------------------------------
MATRIX = [
    {
        "key": "1", "system": "背包 / 装备", "card": "openmir2-m3-bag-equip-alignment",
        "t": None,
        "runner": "run_bag_equip_pair.ps1", "comparator": "scoped_diff.py",
        "scenario": None,
        "note": "拾取→装备→卸下→丢弃→用药 同序列跨侧 scoped_diff=0（T-14/T-22）",
    },
    {
        "key": "2/3", "system": "战斗 / 技能", "card": "openmir2-m3-attack-pair-fixture",
        "t": None,
        "runner": "run_attack_pair.ps1", "comparator": "attack_diff.py",
        "scenario": None,
        "note": "固定种子隔离两相攻击序列逐 k=0（图 0137、L255 pin、专用流纯度门）",
    },
    {
        "key": "4", "system": "掉落 / 经验", "card": "openmir2-m3-drop-exp-align",
        "t": None,
        "runner": "drop-exp/run_drop_exp_pair.ps1",
        "comparator": "drop-exp/drop_exp_diff.py",
        "scenario": "drop-exp/drop_exp_scenario.json",
        "note": "100 杀掉落逐项 + 经验曲线逐级 + 掉落点 RNG 账本（不落地口径 T-32/T-34/T-35）",
    },
    {
        "key": "5", "system": "NPC / 商店", "card": "openmir2-m3-shop-fixture-botsrv",
        "t": "T-41",
        "runner": "run_npc_shop_pair.ps1", "comparator": "npc_shop_diff.py",
        "scenario": "npc_shop_scenario.json",
        "note": "开面板/买/卖/修理/仓库 + 失败分支 同序列帧/文本/多重集 diff=0",
    },
    {
        "key": "6", "system": "任务", "card": "openmir2-m3-quest-fixture-botsrv",
        "t": "T-42",
        "runner": "run_quest_pair.ps1", "comparator": "quest_diff.py",
        "scenario": "quest_scenario.json",
        "note": "接/交/条件判定 + 任务态不落库（QuestFlag/CHECKOPEN/CHECKUNIT）",
    },
    {
        "key": "7", "system": "组队 / 交易", "card": "openmir2-m3-party-trade-fixture-botsrv",
        "t": "T-43",
        "runner": "run_party_trade_pair.ps1", "comparator": "party_trade_diff.py",
        "scenario": "party_trade_scenario.json",
        "note": "两人归属/分配/取消/掉线中断（组队生命周期 + 交易生命周期）",
    },
    {
        "key": "8", "system": "公会 / 攻城", "card": "openmir2-m3-guild-castle-fixture-botsrv",
        "t": "T-46",
        "runner": "run_guild_castle_pair.ps1", "comparator": "guild_castle_diff.py",
        "scenario": "guild_castle_scenario.json",
        "note": "建会/成员/封号/联盟/宣战/城门/修门 同序列帧+标量+多重集（N1/N2/N5 登记）",
    },
    {
        "key": "9", "system": "英雄 / 宠物", "card": "openmir2-m3-hero-pet-fixture-botsrv",
        "t": "T-45",
        "runner": "run_hero_pet_pair.ps1", "comparator": "hero_pet_diff.py",
        "scenario": "hero_pet_scenario.json",
        "note": "召唤/上限/叛变时间门/主人-宝宝经验（T-33 ExpHitter.Master）",
    },
    {
        "key": "10", "system": "拍卖（元宝寄售 SellOff）", "card": "openmir2-m3-selloff-fixture-botsrv",
        "t": "T-44",
        "runner": "run_selloff_pair.ps1", "comparator": "selloff_diff.py",
        "scenario": "selloff_scenario.json",
        "note": "寄售上架/购买/取消/失败分支 两假人（10a 邮件 = oracle_absent，无夹具）",
    },
    {
        "key": "B", "system": "全服 RNG 消费者账本（Claim B）", "card": "openmir2-m3-rng-trace-scene-groups",
        "t": None,
        "runner": "rng-trace-scene/run_rng_trace_pair.ps1",
        "comparator": "rng-trace-scene/rng_scene_gate.py",
        "scenario": "rng-trace-scene/rng_trace_scenario.json",
        "note": "同场景两侧 trace + 站点组序 diff→0（R-6 folded 映射机判）",
    },
]

# 三件套里除 runner/comparator/scenario 外，卡显式声明的额外在场件
EXTRA_ARTIFACTS = {
    "openmir2-m3-drop-exp-align": ["drop-exp/drop_exp_purity_gate.py"],
}


def _artifacts(entry):
    names = [entry["runner"], entry["comparator"]]
    if entry.get("scenario"):
        names.append(entry["scenario"])
    names.extend(EXTRA_ARTIFACTS.get(entry["card"], []))
    return names


def classify(root, entry):
    """⇒ (status, present, missing)；status ∈ READY/PENDING/INCONSISTENT。"""
    names = _artifacts(entry)
    present = [n for n in names if os.path.isfile(os.path.join(root, n))]
    missing = [n for n in names if n not in present]
    if not present:
        return "PENDING", present, missing
    if missing:
        return "INCONSISTENT", present, missing
    return "READY", present, missing


def _default_run_comparator(root, comparator):
    """跑比较器自带 selftest（只读，零端口）。⇒ (rc, last_line)。"""
    cmd = [sys.executable, "-I", comparator, "selftest"]
    try:
        p = subprocess.run(cmd, cwd=root, capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=600)
    except (OSError, subprocess.SubprocessError) as exc:  # pragma: no cover - 环境异常
        return 127, "无法运行比较器 selftest: {0}".format(exc)
    tail = ""
    for line in (p.stdout or "").splitlines()[::-1]:
        if line.strip():
            tail = line.strip()
            break
    return p.returncode, tail


def evaluate(root, matrix, run_comparator, require_all=False, check_whitelist_path=None):
    """纯函数（用于 selftest 注入）：⇒ receipts 列表 + reds 列表。"""
    receipts = []
    reds = []
    for entry in matrix:
        status, present, missing = classify(root, entry)
        rec = {
            "key": entry["key"], "system": entry["system"], "card": entry["card"],
            "t": entry.get("t"), "status": status,
            "present": present, "missing": missing,
            "comparator_rc": None, "comparator_tail": "",
        }
        if status == "READY":
            rc, tail = run_comparator(root, entry["comparator"])
            rec["comparator_rc"] = rc
            rec["comparator_tail"] = tail
            if rc != 0:
                status = "RED"
                rec["status"] = "RED"
                reds.append("{0} {1}：比较器 selftest rc={2}（{3}）".format(
                    entry["key"], entry["system"], rc, tail))
        elif status == "INCONSISTENT":
            reds.append("{0} {1}：三件套部分在场（缺 {2}）⇒ 半拉子合并，绝不当绿".format(
                entry["key"], entry["system"], ",".join(missing)))
        elif status == "PENDING" and require_all:
            reds.append("{0} {1}：夹具尚未合并（{2}）⇒ --require-all 判 RED".format(
                entry["key"], entry["system"], ",".join(missing)))
        # whitelist 登记号（可选；只在 --require-all 时判红，避免未合卡的正常 PENDING 噪声）
        if check_whitelist_path and entry.get("t") and require_all:
            tok = "| {0} |".format(entry["t"])
            try:
                with open(check_whitelist_path, "r", encoding="utf-8", errors="replace") as fh:
                    blob = fh.read()
            except OSError as exc:
                reds.append("{0}：无法读 whitelist（{1}）".format(entry["key"], exc))
                blob = ""
            if blob and tok not in blob:
                reds.append("{0} {1}：whitelist 缺登记号 {2}".format(
                    entry["key"], entry["system"], entry["t"]))
        receipts.append(rec)
    return receipts, reds


# ---------------------------------------------------------------------------
# selftest：合成树 + fail-closed + 改坏必红
# ---------------------------------------------------------------------------
def _write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)


def cmd_selftest(_args):
    tmp = tempfile.mkdtemp(prefix="fixture-matrix-selftest-")
    ok = 0
    total = 0

    def expect(name, cond):
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
            print("  SELFTEST ok  {0}".format(name))
        else:
            print("  SELFTEST FAIL {0}".format(name))

    entries = [
        {"key": "A", "system": "present", "card": "card-a", "t": None,
         "runner": "run_a.ps1", "comparator": "a_diff.py", "scenario": "a.json"},
        {"key": "B", "system": "missing", "card": "card-b", "t": None,
         "runner": "run_b.ps1", "comparator": "b_diff.py", "scenario": "b.json"},
        {"key": "C", "system": "partial", "card": "card-c", "t": None,
         "runner": "run_c.ps1", "comparator": "c_diff.py", "scenario": "c.json"},
        {"key": "D", "system": "broken", "card": "card-d", "t": None,
         "runner": "run_d.ps1", "comparator": "d_diff.py", "scenario": "d.json"},
        {"key": "E", "system": "tagged", "card": "card-e", "t": "T-99",
         "runner": "run_e.ps1", "comparator": "e_diff.py", "scenario": "e.json"},
    ]
    # A 全在场；B 全不源（PENDING）；C 仅 runner（INCONSISTENT）；D 全在场但比较器坏；E 全在场 + T-99
    _write(os.path.join(tmp, "run_a.ps1"), "a")
    _write(os.path.join(tmp, "a_diff.py"), "a")
    _write(os.path.join(tmp, "a.json"), "{}")
    _write(os.path.join(tmp, "run_c.ps1"), "c")
    _write(os.path.join(tmp, "run_d.ps1"), "d")
    _write(os.path.join(tmp, "d_diff.py"), "d")
    _write(os.path.join(tmp, "d.json"), "{}")
    _write(os.path.join(tmp, "run_e.ps1"), "e")
    _write(os.path.join(tmp, "e_diff.py"), "e")
    _write(os.path.join(tmp, "e.json"), "{}")

    def fake_run(_root, comparator):
        if comparator == "d_diff.py":
            return 1, "SYNTHETIC RED"
        return 0, "SYNTHETIC GREEN"

    # 1) 默认：无 RED（PENDING 只登记）
    receipts, reds = evaluate(tmp, entries, fake_run, require_all=False)
    by = {r["key"]: r for r in receipts}
    expect("A READY + comparator rc=0", by["A"]["status"] == "READY" and by["A"]["comparator_rc"] == 0)
    expect("B PENDING", by["B"]["status"] == "PENDING")
    expect("C INCONSISTENT", by["C"]["status"] == "INCONSISTENT")
    expect("D RED（比较器坏）", by["D"]["status"] == "RED" and by["D"]["comparator_rc"] == 1)
    expect("E READY", by["E"]["status"] == "READY")
    expect("默认无 RED：只有 D + C ⇒ 2 条", len(reds) == 2 and any("比较器" in r for r in reds))
    expect("默认 PENDING 不判红", not any("尚未合并" in r for r in reds))

    # 2) --require-all：PENDING 也判红
    receipts2, reds2 = evaluate(tmp, entries, fake_run, require_all=True)
    expect("require-all 把 B 判红", any("B" == r.split(" ")[0] and "尚未合并" in r for r in reds2))

    # 3) whitelist 登记号：缺 T-99 ⇒ 判红；有 ⇒ 不判红
    wl_bad = os.path.join(tmp, "whitelist-bad.md")
    _write(wl_bad, "| T-41 | x |\n")
    _, reds_wl_bad = evaluate(tmp, entries, fake_run, require_all=True, check_whitelist_path=wl_bad)
    expect("require-all + whitelist 缺 T-99 ⇒ 判红", any("缺登记号 T-99" in r for r in reds_wl_bad))
    wl_ok = os.path.join(tmp, "whitelist-ok.md")
    _write(wl_ok, "| T-41 | x |\n| T-99 | y |\n")
    _, reds_wl_ok = evaluate(tmp, entries, fake_run, require_all=True, check_whitelist_path=wl_ok)
    expect("whitelist 有 T-99 ⇒ 不判红", not any("缺登记号" in r for r in reds_wl_ok))

    # 4) 改坏必红：把 A 的三件套删一件 ⇒ 由 READY 变 INCONSISTENT
    os.remove(os.path.join(tmp, "a.json"))
    receipts3, _ = evaluate(tmp, entries, fake_run, require_all=False)
    by3 = {r["key"]: r for r in receipts3}
    expect("删 A 的 scenario ⇒ INCONSISTENT（改坏必红）", by3["A"]["status"] == "INCONSISTENT")

    # 5) 真源自检：注册表无重复 key、三件套齐、runner 以 run_ 开头
    keys = [e["key"] for e in MATRIX]
    expect("注册表 key 无重复", len(keys) == len(set(keys)))
    numbered = set()
    for e in MATRIX:
        for part in e["key"].split("/"):
            if part.isdigit():
                numbered.add(int(part))
    expect("注册表覆盖 §6 十项（1..10）", numbered == set(range(1, 11)))
    expect("注册表含 Claim B 场景账本", any(e["key"] == "B" for e in MATRIX))
    expect("每项都有 runner/comparator/card", all(
        e.get("runner") and e.get("comparator") and e.get("card") for e in MATRIX))
    expect("runner 命名合规", all(e["runner"].startswith("run_") or "/run_" in e["runner"] for e in MATRIX))
    expect("comparator 命名合规", all(e["comparator"].endswith((".py",)) for e in MATRIX))

    print("SELFTEST {0}: fixture_matrix selftest {1}/{2}".format(
        "GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


# ---------------------------------------------------------------------------
# check / list
# ---------------------------------------------------------------------------
def _print_table(receipts):
    print("== M3 夹具矩阵 ==")
    print("{0:<5} {1:<22} {2:<12} {3:<7} {4}".format("#", "系统", "状态", "T", "卡"))
    for r in receipts:
        print("{0:<5} {1:<22} {2:<12} {3:<7} {4}".format(
            r["key"], r["system"], r["status"], r["t"] or "-", r["card"]))
        if r["status"] == "INCONSISTENT":
            print("       缺件: {0}".format(",".join(r["missing"])))
        if r["status"] == "READY":
            print("       比较器 selftest rc={0} :: {1}".format(
                r["comparator_rc"], r["comparator_tail"][:96]))


def cmd_check(args):
    root = os.path.abspath(args.root)
    wl = os.path.join(root, os.pardir, "whitelist.md")
    wl = wl if os.path.isfile(wl) else None
    receipts, reds = evaluate(root, MATRIX, _default_run_comparator,
                              require_all=args.require_all, check_whitelist_path=wl)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"require_all": bool(args.require_all), "receipts": receipts, "reds": reds},
                      fh, ensure_ascii=False, indent=2, sort_keys=True)
            fh.write("\n")
    _print_table(receipts)
    ready = sum(1 for r in receipts if r["status"] == "READY")
    pending = sum(1 for r in receipts if r["status"] == "PENDING")
    if reds:
        print("FIXTURE_MATRIX RED: ready={0} pending={1} red={2}".format(ready, pending, len(reds)))
        for r in reds:
            print("  RED " + r)
        return 1
    print("FIXTURE_MATRIX GREEN: ready={0} pending={1} red=0{2}".format(
        ready, pending, "（--require-all）" if args.require_all else ""))
    return 0


def cmd_list(args):
    rows = []
    for e in MATRIX:
        rows.append({"key": e["key"], "system": e["system"], "card": e["card"],
                     "t": e.get("t"), "runner": e["runner"],
                     "comparator": e["comparator"], "scenario": e.get("scenario"),
                     "note": e["note"]})
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(rows, fh, ensure_ascii=False, indent=2, sort_keys=True)
            fh.write("\n")
    print("| # | 系统 | 夹具驱动 | 比较器 | 场景 | T | 承接卡 |")
    print("| --- | --- | --- | --- | --- | --- | --- |")
    for r in rows:
        print("| {0} | {1} | `{2}` | `{3}` | {4} | {5} | `{6}` |".format(
            r["key"], r["system"], r["runner"], r["comparator"],
            "`{0}`".format(r["scenario"]) if r["scenario"] else "-",
            r["t"] or "-", r["card"]))
    return 0


def main():
    ap = argparse.ArgumentParser(description="M3 夹具矩阵与就绪度门（只读，零端口）")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p_check = sub.add_parser("check", help="核对当前树的三件套在场/比较器 selftest")
    p_check.add_argument("--root", default=HERE)
    p_check.add_argument("--require-all", action="store_true",
                         help="PENDING（尚未合并）也判 RED；并检查 whitelist 登记号")
    p_check.add_argument("--json", default=None)
    p_check.set_defaults(func=cmd_check)
    p_list = sub.add_parser("list", help="打印矩阵（markdown 表）")
    p_list.add_argument("--json", default=None)
    p_list.set_defaults(func=cmd_list)
    p_self = sub.add_parser("selftest", help="合成件 fail-closed + 改坏必红自检")
    p_self.set_defaults(func=cmd_selftest)
    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
