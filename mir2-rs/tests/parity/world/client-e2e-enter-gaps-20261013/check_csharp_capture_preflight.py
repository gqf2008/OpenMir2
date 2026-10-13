#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_csharp_capture_preflight.py —— 卡 `openmir2-client-e2e-enter-gaps` DoD③
**C# 侧抓包拓扑的离线前置门**（只读、零端口、零起栈）。

背景（`85-csharp-capture-topology-rootcause.md`）：C 线（2026-10-10）把「抓包拓扑下
`CM_LOGINNOTICEOK` 之后世界数据下不来」归因于「影子网关端口被挪」。按当前源码复核，
该归因不成立：
  * GameSvr 给客户端下发的网关端口只在**多机转移**路径用到（`WorldServer.SendChangeServer`
    → `SM_RECONNECT`，`WorldServer.cs:1063-1074`）；`!servertable.txt` 的唯一其它读者是
    `GameShare.GetMultiServerAddrPort`（`AppService.cs:221` 关机转移）。单机进世界流程不读它。
  * GameSvr 对网关的寻址按**槽位**（`TCPNetChannel._gateSlotByConnection` / `_gameGates`），
    世界侧用 `playObject.GateIdx` = 网关在认证串里自报的 `ServiceId`
    （`GameGate/Services/ClientSession.cs:1203`），与端口无关。
  * 同症状的确定根因是 S1 缺陷①「网关槽位按 TCP accept 序绑定 + `CloseGate` 不释放槽位」
    （`evidence/S1/S1-报告.md` §1），修复 `607e9b773`（2026-10-10 17:01），
    部署件 `M2GameSvr/!deployed-from.json` 的 `git_head` = `628d8024`（2026-10-10 23:54）含该修复。
  ⇒ 82 手册 §3/§4 的「只挪 game-gate + 代理占 7200」拓扑**有根因级依据**；窗口里仍须实拍确认。

本工具把窗口前的**静态前置**做成机判（fail-closed）：
  1. `M2GameSvr/!deployed-from.json.git_head` 必须是修复提交 `607e9b773` 的后代
     （`git merge-base --is-ancestor`）——旧的 MIXED/未修部署会让历史黑屏复现；
  2. `RunGate/config.conf`（GBK）的**活动网关数** `ServerWorkThread` 必须为 1：
     S1 §2.1/§8 登记的残留边界是「多网关不按声明编号顺序重连 ⇒ 最小空闲槽仍错配」，
     单活动网关（ServiceId 0 ↔ 槽位 0）是抓包窗口唯一安全形态；
  3. 真实网关监听端口 `GatePort1` == 代理要占的端口（默认 7200）——否则代理占不住/客户端连不上；
  4. `!servertable.txt` 的 GameGate 端口 == 代理端口（语义上客户端目标端口不因影子挪端口而变）；
  5. 上游 `ServerAddr1:ServerPort1` == GameSvr 监听地址（默认 127.0.0.1:5000）；
  6. 可选 `--shadow-config`：影子 RunGate 配置也必须是单活动网关 + `GatePort1 == --shadow-port`。

规范依据（别只信本文件注释）：`src/GameSrv/GameApp.cs:262-306`（LoadServerTable）、
`src/GameSrv/GameShare.cs:57-90`（GetMultiServerAddrPort）、`src/GameSrv/AppService.cs:221`、
`src/GameSrv/Word/WorldServer.cs:1063-1074`、`src/M2Server/Net/TCP/TCPNetChannel.cs:26-120`、
`src/GameGate/Conf/ConfigManager.cs:11-46`、`src/GameGate/Services/ServerManager.cs:38-41`、
`src/GameGate/Services/ClientSession.cs:1203`、`mir2-rs/tests/parity/evidence/S1/S1-报告.md` §1/§2.1/§8。

用法：
  python -I -B check_csharp_capture_preflight.py --repo <git 根> [--mirserver E:/MirServer] \
      [--proxy-port 7200] [--shadow-port 7600] [--shadow-config <影子 config.conf>] \
      [--fix-commit 607e9b77347f94820c1d57da7502d0a2826b680a] [--allow-multi-gate]
  python -I -B check_csharp_capture_preflight.py --selftest

退出码：`0` = GREEN、`1` = RED（判据不符）、`2` = PREREQ_RED（缺输入/解析不到 git）。
`--selftest` 不依赖外部件（合成 `!servertable.txt` / `config.conf` 文本 + 合成 deploy 判定）。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys

# S1 缺陷① 的修复提交（`fix(oracle): 修 GameSvr 网关重连后进世界 NRE 死循环`）
FIX_COMMIT_DEFAULT = "607e9b77347f94820c1d57da7502d0a2826b680a"
# 部署件来源登记（`tools/oracle/deploy_gamesvr.ps1` 写）
DEPLOYED_FROM_REL = os.path.join("M2GameSvr", "!deployed-from.json")
SERVERTABLE_REL = os.path.join("M2GameSvr", "!servertable.txt")
GATE_CONF_REL = os.path.join("RunGate", "config.conf")


def decode_text(raw: bytes) -> str:
    """与 `crates/envir` 的 `decode_text` 同口径（部署件是 GBK；失败不抛）。"""
    for enc in ("gbk", "utf-8"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("gbk", errors="replace")


def parse_servertable(text: str) -> list[dict]:
    """`!servertable.txt`：`<idx> <selgateIP> <gameGateIP> <gameGatePort> [<ip> <port> ...]`。

    锚点 `src/GameSrv/GameApp.cs:280-300`：第一 token = idx、第二 = SelGateIP，
    随后交替抽 `GameGateIP[n]` / `GameGatePort[n]`。跳过 `;` 注释与不足 4 token 的行。
    """
    routes: list[dict] = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith(";"):
            continue
        toks = line.split()
        if len(toks) < 4:
            continue
        try:
            idx = int(toks[0])
        except ValueError:
            continue
        gates: list[tuple[str, int]] = []
        rest = toks[2:]
        for i in range(0, len(rest) - 1, 2):
            try:
                gates.append((rest[i], int(rest[i + 1])))
            except ValueError:
                continue
        if not gates:
            continue
        routes.append({"idx": idx, "selgate_ip": toks[1], "gates": gates})
    return routes


def parse_gate_conf(text: str) -> dict:
    """`RunGate/config.conf` 的 `[GameGate]` 段（键 `Key=Value`；GBK 文本已解码）。

    只取本判据需要的键；`ServerWorkThread` 是**活动网关数**
    （`ConfigManager.cs:31-45` 用它当循环上界；`ServerManager.cs:38-41` 用它 new 服务数组）。
    """
    conf: dict = {"servers": {}, "gate_ports": {}, "server_work_thread": None}
    section = ""
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith(";") or line.startswith("#"):
            continue
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1].strip()
            continue
        if section != "GameGate" or "=" not in line:
            continue
        k, v = line.split("=", 1)
        k = k.strip()
        v = v.strip()
        if k == "ServerWorkThread":
            try:
                conf["server_work_thread"] = int(v)
            except ValueError:
                conf["server_work_thread"] = -1
        m = re.fullmatch(r"(ServerAddr|ServerPort|GatePort)(\d+)", k)
        if not m:
            continue
        kind, n = m.group(1), int(m.group(2))
        if n <= 0:
            continue
        if kind == "GatePort":
            try:
                conf["gate_ports"][n] = int(v)
            except ValueError:
                conf["gate_ports"][n] = -1
        else:
            conf["servers"].setdefault(n, {})[kind] = v
    return conf


def deployed_head(json_text: str) -> str | None:
    """`!deployed-from.json` 的 `git_head`（缺失/坏 JSON ⇒ None）。"""
    try:
        obj = json.loads(json_text.lstrip("\ufeff"))
    except (ValueError, TypeError):
        return None
    head = obj.get("git_head")
    return head if isinstance(head, str) and head else None


def git_is_ancestor(repo: str, ancestor: str, descendant: str) -> tuple[bool | None, str]:
    """`git merge-base --is-ancestor`；返回 `(True/False/None, 诊断)`（None = 前置失败）。"""
    if not os.path.isdir(repo):
        return None, f"repo 目录不存在: {repo}"
    try:
        r = subprocess.run(
            ["git", "-C", repo, "merge-base", "--is-ancestor", ancestor, descendant],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60,
        )
    except (OSError, subprocess.SubprocessError) as ex:
        return None, f"git 执行失败: {ex}"
    if r.returncode == 0:
        return True, "ok"
    if r.returncode == 1:
        return False, f"{ancestor[:12]} 不是 {descendant[:12]} 的祖先"
    return None, f"git 退出码 {r.returncode}: {r.stderr.decode('utf-8', 'replace').strip()}"


def evaluate(
    routes: list[dict],
    real_conf: dict,
    shadow_conf: dict | None,
    fix_ok: bool | None,
    proxy_port: int,
    shadow_port: int,
    game_svr_ip: str,
    game_svr_port: int,
    allow_multi_gate: bool,
) -> tuple[int, list[str]]:
    """纯判定（不碰文件系统，便于 selftest）。`fix_ok=None` ⇒ 前置失败。"""
    msgs: list[str] = []
    if not routes:
        return 2, ["PREREQ_RED: !servertable.txt 解析不到任何路由（格式变了？）"]
    if real_conf.get("server_work_thread") is None:
        return 2, ["PREREQ_RED: RunGate/config.conf 没有 [GameGate] ServerWorkThread 键"]
    if fix_ok is None:
        return 2, ["PREREQ_RED: 部署件修复提交判定失败（--repo 不可用？）"]

    red = 0

    # 1. 部署件含 S1 缺陷① 修复
    if fix_ok:
        msgs.append("OK  部署件 git_head 含修复提交 607e9b773（S1 缺陷①）")
    else:
        red += 1
        msgs.append("RED 部署件 git_head 不含修复提交 607e9b773 ⇒ 历史黑屏会复现，先按 tools/oracle/deploy_gamesvr.ps1 重铺")

    # 2/3/5. 真实配置：单活动网关 + 监听端口/上游
    nwt = real_conf["server_work_thread"]
    if nwt == 1:
        msgs.append("OK  真实 RunGate ServerWorkThread=1（ServiceId 0 ↔ GameSvr 槽位 0，无多网关错配）")
    elif allow_multi_gate:
        msgs.append(f"WARN 真实 RunGate ServerWorkThread={nwt}（>1）——S1 §2.1 残留边界仍活，仅因 --allow-multi-gate 放行")
    else:
        red += 1
        msgs.append(f"RED 真实 RunGate ServerWorkThread={nwt}（>1）⇒ 多网关重连可错配槽位（S1 §2.1/§8）；抓包窗口须单活动网关")

    g1 = real_conf["gate_ports"].get(1)
    if g1 == proxy_port:
        msgs.append(f"OK  真实 GatePort1={g1} == 代理端口")
    else:
        red += 1
        msgs.append(f"RED 真实 GatePort1={g1} != 代理端口 {proxy_port}（代理会绑不上/客户端连不上）")

    srv1 = real_conf["servers"].get(1, {})
    if srv1.get("ServerAddr") == game_svr_ip and str(srv1.get("ServerPort")) == str(game_svr_port):
        msgs.append(f"OK  真实上游 {srv1.get('ServerAddr')}:{srv1.get('ServerPort')} == GameSvr 监听")
    else:
        red += 1
        msgs.append(f"RED 真实上游 {srv1.get('ServerAddr')}:{srv1.get('ServerPort')} != GameSvr {game_svr_ip}:{game_svr_port}")

    # 4. !servertable.txt 的 GameGate 端口
    seen_ports = {p for r in routes for (_, p) in r["gates"]}
    if seen_ports == {proxy_port}:
        msgs.append(f"OK  !servertable.txt GameGate 端口 {sorted(seen_ports)} == 代理端口")
    else:
        red += 1
        msgs.append(f"RED !servertable.txt GameGate 端口 {sorted(seen_ports)} != {{代理端口 {proxy_port}}}")

    # 6. 可选影子配置
    if shadow_conf is not None:
        snwt = shadow_conf.get("server_work_thread")
        if snwt == 1:
            msgs.append("OK  影子 RunGate ServerWorkThread=1")
        elif allow_multi_gate:
            msgs.append(f"WARN 影子 RunGate ServerWorkThread={snwt}（>1）——仅因 --allow-multi-gate 放行")
        else:
            red += 1
            msgs.append(f"RED 影子 RunGate ServerWorkThread={snwt}（>1）⇒ 影子也必须单活动网关")
        sg1 = shadow_conf["gate_ports"].get(1)
        if sg1 == shadow_port:
            msgs.append(f"OK  影子 GatePort1={sg1} == 影子端口")
        else:
            red += 1
            msgs.append(f"RED 影子 GatePort1={sg1} != 影子端口 {shadow_port}")
        ssrv1 = shadow_conf["servers"].get(1, {})
        if ssrv1.get("ServerAddr") == game_svr_ip and str(ssrv1.get("ServerPort")) == str(game_svr_port):
            msgs.append(f"OK  影子上游 {ssrv1.get('ServerAddr')}:{ssrv1.get('ServerPort')} == GameSvr 监听")
        else:
            red += 1
            msgs.append(f"RED 影子上游 {ssrv1.get('ServerAddr')}:{ssrv1.get('ServerPort')} != GameSvr {game_svr_ip}:{game_svr_port}")

    if red:
        msgs.append(f"RED {red} 条前置不符")
        return 1, msgs
    msgs.append("OK  C# 侧抓包拓扑前置全部满足")
    return 0, msgs


def _read(path: str) -> str | None:
    if not os.path.isfile(path):
        return None
    try:
        with open(path, "rb") as fh:
            return decode_text(fh.read())
    except OSError:
        return None


def _read_json(path: str) -> str | None:
    """`!deployed-from.json` 是 UTF-8(+BOM)，不能走 GBK 优先的 `decode_text`。"""
    if not os.path.isfile(path):
        return None
    try:
        with open(path, "rb") as fh:
            raw = fh.read()
    except OSError:
        return None
    for enc in ("utf-8-sig", "utf-8"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return decode_text(raw)


def _conf(work_thread: int, gate_port: int, addr: str = "127.0.0.1", port: int = 5000) -> dict:
    return {
        "server_work_thread": work_thread,
        "gate_ports": {1: gate_port},
        "servers": {1: {"ServerAddr": addr, "ServerPort": str(port)}},
    }


def selftest() -> int:
    st = ";server table\n0 127.0.0.1 127.0.0.1 7200\n"
    routes = parse_servertable(st)
    assert routes == [{"idx": 0, "selgate_ip": "127.0.0.1", "gates": [("127.0.0.1", 7200)]}], routes
    assert parse_servertable(";c\n0 127.0.0.1\n") == []
    assert parse_servertable("0 127.0.0.1 127.0.0.1 7200 127.0.0.1 7201\n")[0]["gates"] == [
        ("127.0.0.1", 7200),
        ("127.0.0.1", 7201),
    ]

    conf_text = (
        "[GameGate]\nCount=3\nServerAddr1=127.0.0.1\nServerPort1=5000\n"
        "GatePort1=7200\nGatePort2=7201\nServerWorkThread=1\n\n[Integer]\nMaxUser=500\n"
    )
    conf = parse_gate_conf(conf_text)
    assert conf["server_work_thread"] == 1, conf
    assert conf["gate_ports"] == {1: 7200, 2: 7201}, conf
    assert conf["servers"][1]["ServerPort"] == "5000"

    assert deployed_head('{"git_head":"abc123"}') == "abc123"
    assert deployed_head("\ufeff{\"git_head\":\"abc\"}") == "abc"
    assert deployed_head("not json") is None

    ok_conf = _conf(1, 7200)
    cases = [
        ("green", routes, ok_conf, None, True, 0),
        ("no-fix", routes, ok_conf, None, False, 1),
        ("multi-gate", routes, _conf(3, 7200), None, True, 1),
        ("multi-gate-allowed", routes, _conf(3, 7200), None, True, 0),
        ("wrong-port", routes, _conf(1, 7201), None, True, 1),
        ("wrong-upstream", routes, _conf(1, 7200, addr="10.0.0.9"), None, True, 1),
        ("fix-unknown-prereq", routes, ok_conf, None, None, 2),
        ("empty-routes-prereq", [], ok_conf, None, True, 2),
        ("shadow-green", routes, ok_conf, _conf(1, 7600), True, 0),
        ("shadow-multi-gate", routes, ok_conf, _conf(3, 7600), True, 1),
        ("shadow-wrong-port", routes, ok_conf, _conf(1, 7601), True, 1),
    ]
    bad = 0
    for name, rt, rc_, sc, fix_ok, want in cases:
        allow = name.endswith("allowed")
        rc, _ = evaluate(rt, rc_, sc, fix_ok, 7200, 7600, "127.0.0.1", 5000, allow)
        ok = rc == want
        bad += 0 if ok else 1
        print(f"[{'PASS' if ok else 'FAIL'}] selftest {name}: rc={rc} (want {want})")

    # 缺 ServerWorkThread 键 ⇒ 前置失败（不是静默当 1）
    no_wt = {"gate_ports": {1: 7200}, "servers": {1: {"ServerAddr": "127.0.0.1", "ServerPort": "5000"}}}
    rc, _ = evaluate(routes, no_wt, None, True, 7200, 7600, "127.0.0.1", 5000, False)
    ok = rc == 2
    bad += 0 if ok else 1
    print(f"[{'PASS' if ok else 'FAIL'}] selftest missing-workthread-prereq: rc={rc} (want 2)")

    total = len(cases) + 1
    print(f"SELFTEST {'OK' if bad == 0 else 'FAILED'} ({total - bad}/{total})")
    return 0 if bad == 0 else 1


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mirserver", default="E:/MirServer", help="oracle 部署根（默认 E:/MirServer）")
    ap.add_argument("--repo", default="", help="git 仓库根（判 deployed-from.git_head 是否含修复提交）")
    ap.add_argument("--fix-commit", default=FIX_COMMIT_DEFAULT)
    ap.add_argument("--proxy-port", type=int, default=7200, help="真实 GameGate 端口 = 代理要占的端口")
    ap.add_argument("--shadow-port", type=int, default=7600, help="影子 GameGate 监听端口")
    ap.add_argument("--shadow-config", default="", help="可选：影子 RunGate/config.conf 路径")
    ap.add_argument("--game-svr-ip", default="127.0.0.1")
    ap.add_argument("--game-svr-port", type=int, default=5000)
    ap.add_argument("--allow-multi-gate", action="store_true", help="把多活动网关从 RED 降为 WARN（不建议）")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    st_text = _read(os.path.join(args.mirserver, SERVERTABLE_REL))
    conf_text = _read(os.path.join(args.mirserver, GATE_CONF_REL))
    if st_text is None:
        print(f"PREREQ_RED: 缺 !servertable.txt: {os.path.join(args.mirserver, SERVERTABLE_REL)}")
        return 2
    if conf_text is None:
        print(f"PREREQ_RED: 缺 RunGate/config.conf: {os.path.join(args.mirserver, GATE_CONF_REL)}")
        return 2

    routes = parse_servertable(st_text)
    real_conf = parse_gate_conf(conf_text)

    shadow_conf = None
    if args.shadow_config:
        sc_text = _read(args.shadow_config)
        if sc_text is None:
            print(f"PREREQ_RED: 影子 config 不存在/不可读: {args.shadow_config}")
            return 2
        shadow_conf = parse_gate_conf(sc_text)

    # 部署件判定
    df_text = _read_json(os.path.join(args.mirserver, DEPLOYED_FROM_REL))
    if df_text is None:
        print(f"WARN 缺部署来源登记 {DEPLOYED_FROM_REL}（无法判修复提交；按 --repo={args.repo or '(未给)'} 判 head）")
        head = None
    else:
        head = deployed_head(df_text)
    if not args.repo:
        print("PREREQ_RED: 需要 --repo <git 根> 才能判 deployed-from.git_head 是否含修复提交")
        return 2
    if head is None:
        print(f"PREREQ_RED: 部署来源登记缺 git_head: {os.path.join(args.mirserver, DEPLOYED_FROM_REL)}")
        return 2
    fix_ok, diag = git_is_ancestor(args.repo, args.fix_commit, head)
    print(f"INFO deployed git_head={head[:12]} fix={args.fix_commit[:12]} ancestor={fix_ok} ({diag})")

    rc, msgs = evaluate(
        routes,
        real_conf,
        shadow_conf,
        fix_ok,
        args.proxy_port,
        args.shadow_port,
        args.game_svr_ip,
        args.game_svr_port,
        args.allow_multi_gate,
    )
    print(f"INFO mirserver={args.mirserver} proxy_port={args.proxy_port} shadow_port={args.shadow_port}")
    for m in msgs:
        print(m)
    print("RESULT: " + ("GREEN" if rc == 0 else ("PREREQ_RED" if rc == 2 else "RED")))
    return rc


if __name__ == "__main__":
    sys.exit(main())
