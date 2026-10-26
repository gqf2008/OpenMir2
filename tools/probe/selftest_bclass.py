#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""B 类场景判据牙（卡 `openmir2-probe-scenario-bclass-fixes` 原写于 `E:/tmp`；卡
`openmir2-tooth-hardening-batch` A 项把它落进仓库）。

只读加载 **本 checkout** 的 `tools/probe/mir2_probe.py` 与 `tools/probe/scenarios/` 下两个场景 JSON，
用合成帧流断言（不连网、不起栈、不占端口）：

  ① 判据被钉住：gamepoint 的 wait idents 恰为 `[643],[643],[5008]`（**不含**兑换后的 643）；
     dropgold 恰为 `[653]`——谁把「等不到的 643」加回来，这条立刻红。
  ② 缺真实回执 ⇒ 超时 `None`（RED）。
  ③ 无关帧（含随机帧 10/41/100/200/622）在场 ⇒ **不得**被判为匹配。
  ④ 正控：喂真实帧 ⇒ 命中。

路径**从本文件推导**（`tools/probe/` 的上一级），不写死任何 worktree ⇒ 在任意 checkout 上
用同一条命令即可复现：

    python -I -B tools/probe/selftest_bclass.py       # 期望：TOOTH GREEN (0 fail)
    python -I -B tools/probe/selftest_bclass.py --repo <checkout>   # 显式指定（调试用）

退出码：0 = 全 PASS；1 = 有 FAIL。
"""
from __future__ import annotations

import argparse
import importlib.util
import io
import json
import os
import shutil
import sys
import tempfile

SM_TURN, SM_FEATURECHANGED, SM_SYSMESSAGE = 10, 41, 100
SM_ADDITEM, SM_BAGITEMS, SM_MERCHANTSAY, SM_GOLDCHANGED, SM_GAMEGOLDNAME = 200, 201, 643, 653, 5008
RANDOM_IDENTS = [SM_TURN, SM_FEATURECHANGED, SM_SYSMESSAGE, SM_ADDITEM, SM_BAGITEMS,
                 SM_MERCHANTSAY, SM_GOLDCHANGED]


def repo_root_default():
    """本文件在 <repo>/tools/probe/ ⇒ 仓库根是上两级。"""
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.dirname(os.path.dirname(here))


def main(argv=None):
    ap = argparse.ArgumentParser(description="B 类场景判据牙（离线、只读）")
    ap.add_argument("--repo", default=repo_root_default(),
                    help="checkout 根（默认由本文件位置推导：%s）" % repo_root_default())
    args = ap.parse_args(argv)

    repo = os.path.abspath(args.repo)
    probe_path = os.path.join(repo, "tools", "probe", "mir2_probe.py")
    gp = os.path.join(repo, "tools", "probe", "scenarios", "gamepoint_exchange.json")
    dg = os.path.join(repo, "tools", "probe", "scenarios", "dropgold_zero_draw.json")
    for p in (probe_path, gp, dg):
        if not os.path.isfile(p):
            print("TOOTH RED (0 fail checks ran)：缺文件 %s（--repo=%s）" % (p, repo))
            return 1

    spec = importlib.util.spec_from_file_location("probe_bclass", probe_path)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)

    fails = []

    def chk(cond, name):
        print("%s %s" % ("PASS" if cond else "FAIL", name))
        if not cond:
            fails.append(name)

    def feed(frames):
        tmp = tempfile.mkdtemp(prefix="tooth-bclass-")
        run_args = argparse.Namespace(out=tmp, no_auto_handshake=True, timeout_ms=8000)
        run = m.Run(run_args)
        conn = m.Conn("gamegate", run.recorder, on_frame=run.on_frame)
        conn.phase = "gamegate"
        conn.sock = m._FakeSock(list(frames))
        conn.pump(100)
        return conn, tmp

    def wait_idents(scenario_path, params):
        data = json.load(io.open(scenario_path, encoding="utf-8"))
        plans, _params = m.expand_scenario(data, params)
        return [p["idents"] for p in plans if p.get("kind") == "wait_for"]

    gp_waits = wait_idents(gp, {"npc_actor": 1})
    dg_waits = wait_idents(dg, {})

    # ① 判据钉住
    chk(gp_waits == [[SM_MERCHANTSAY], [SM_MERCHANTSAY], [SM_GAMEGOLDNAME]],
        "判据钉住 gamepoint: waits == [[643],[643],[5008]]（兑换后不再等 643），实际 %r" % (gp_waits,))
    chk(dg_waits == [[SM_GOLDCHANGED]], "判据钉住 dropgold: waits == [[653]]，实际 %r" % (dg_waits,))

    # ② 缺真实回执 ⇒ 超时 None；③ 无关/随机帧不算匹配
    for name, waits in (("gamepoint", gp_waits), ("dropgold", dg_waits)):
        for idents in waits:
            pool = [i for i in RANDOM_IDENTS if i not in set(idents)]
            frames_pool = [m.build_s2c_frame(119, i, 0, 0, 0, b"") for i in pool]
            conn, tmp = feed(frames_pool)
            try:
                got = conn.wait_for(list(idents), 50, 0)
                chk(got is None,
                    "牙(%s): 只有无关帧时 wait %r ⇒ 超时 None（删掉真实回执必红）" % (name, idents))
            finally:
                shutil.rmtree(tmp, ignore_errors=True)

    # ④ 正控
    for ident in (SM_GAMEGOLDNAME, SM_GOLDCHANGED, SM_MERCHANTSAY):
        conn, tmp = feed([m.build_s2c_frame(1, ident, 0, 0, 0, b"")])
        try:
            got = conn.wait_for([ident], 50, 0)
            chk(got is not None and got[1].ident == ident, "正控: 真实 %d 在场 ⇒ 命中" % ident)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    print("TOOTH %s (%d fail)" % ("GREEN" if not fails else "RED", len(fails)))
    return 0 if not fails else 1


if __name__ == "__main__":
    sys.exit(main())
