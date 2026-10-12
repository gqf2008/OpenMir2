#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""rebuild_attack_my.py — 从带 `机器人:[id]` 前缀的 BotSrv 合并日志重建带 `my` 的 attack_raw.jsonl。

背景（owner 2026-10-12 17:35，残差③）：历史会话的 `attack_raw.jsonl` 由 `run_pair.ps1` 用
`M3ATTSEQ (\\{[^{}]*\\})` 从 BotSrv 合并日志里抽出，**丢掉了**每条日志行前的发射者
`机器人:[<LoginId>]`。而同一击会被 victim 与 attacker **两个假人各落一遍**（C# 栈还会各投递两次）
⇒ 判据既分不清施击者、又按同帧累加而重复计数。本脚本把发射者身份还原成 `my` 字段：

  * `role=attack` 行：`my` = **施击者 ActorId**（= 全流 struck 帧里唯一出现的 `src`）；
  * `role=struck` 行：`my` = **观察者自身 ActorId**（attacker 账号 ⇒ `src`，其余 ⇒ `actor`）。

**仅用于离线复算历史会话**；production 由 BotSrv 直接写 `my`
（`RobotPlayer.AttackSequenceTick` / `RobotPlayer.Operate.cs` 的 `SM_STRUCK` 分支，T-29 扩展）。
本脚本不改变任何事件语义，只补观察者/施击者身份，供 `attack_diff.py align` 的
`src == my` 过滤与事件键去重使用。

用法：
    python rebuild_attack_my.py <bots-merged-log.txt> <out.attack_raw.jsonl>

退出码：0 = 成功；2 = 输入不满足前提（无 arm 行 / struck 里 src 不唯一 / 观察者身份不一致）。
"""
import json
import re
import sys

LINE = re.compile(r"机器人:\[([^\]]+)\]\s+M3ATTSEQ\s+(\{[^{}]*\})")


def main(argv):
    if len(argv) != 3:
        print(__doc__)
        return 2
    src, out = argv[1], argv[2]
    rows = []
    with open(src, "r", encoding="utf-8-sig", errors="replace") as fh:
        for line in fh:
            m = LINE.search(line)
            if not m:
                continue
            bot, payload = m.group(1), m.group(2)
            try:
                ev = json.loads(payload)
            except ValueError:
                continue
            if isinstance(ev, dict) and "role" in ev:
                rows.append((bot, ev))

    # 施击者账号 = arm 行的 id；施击者 ActorId = struck 帧里唯一出现的 src。
    attacker_login = None
    for bot, ev in rows:
        if ev.get("role") == "arm":
            attacker_login = ev.get("id")
    if attacker_login is None:
        print("PREREQ_MISSING: 日志里没有 arm 行（无法确定 attacker 账号）", file=sys.stderr)
        return 2

    srcs = sorted({ev.get("src") for _, ev in rows if ev.get("role") == "struck" and isinstance(ev.get("src"), int)})
    if len(srcs) != 1:
        print("PREREQ_MISSING: struck 帧的 src 不唯一（%s）——无法唯一确定施击者 ActorId" % srcs, file=sys.stderr)
        return 2
    attacker_actor = srcs[0]

    # 每个假人自身的 ActorId：attacker ⇒ src；其余 ⇒ 其 struck 行的 actor（应唯一）。
    own = {attacker_login: attacker_actor}
    for bot, ev in rows:
        if bot == attacker_login or ev.get("role") != "struck":
            continue
        if bot in own:
            if own[bot] != ev.get("actor"):
                print("PREREQ_MISSING: 假人 %s 的 actor 不一致（%s vs %s）" % (bot, own[bot], ev.get("actor")), file=sys.stderr)
                return 2
        else:
            own[bot] = ev.get("actor")

    with open(out, "w", encoding="utf-8", newline="\n") as fh:
        for bot, ev in rows:
            ev = dict(ev)
            if ev.get("role") in ("attack", "struck"):
                ev["my"] = own.get(bot, attacker_actor)
            fh.write(json.dumps(ev, ensure_ascii=False) + "\n")

    attacks = sum(1 for _, ev in rows if ev.get("role") == "attack")
    struck = sum(1 for _, ev in rows if ev.get("role") == "struck")
    print("REBUILD_OK %s attacker_login=%s attacker_actor=%s observers=%s attacks=%d struck=%d" %
          (out, attacker_login, attacker_actor, json.dumps(own, ensure_ascii=False), attacks, struck))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
