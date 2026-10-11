#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""count_m1_logs.py — M1⑤「两侧证据计数」的**单一口径**实现。

判据卡：`openmir2-m1-boundary-load-200` 的 ②（200 假人同登+选角，两侧都要给计数）。
本脚本被两条路复用，避免两处口径漂移：

1. `tools/m1-acceptance/run_m1_load.ps1` 的「两侧证据计数」段（活栈跑完后的现场计数）；
2. 只用**入库证据件**复算：直接把 `mir2-rs/tests/parity/m1-boundary-load/evidence/`
   里的四份 Rust stdout 与 BotSrv `bots.log` 传进来即可（评审/复现用这条）。

口径（每条计数 = 命中行数；日志文件缺失 ⇒ 该计数写 -1，并在 `missing_logs` 里列出）::

  bots.query_chr              BotSrv `查询角色.`            每次查询 1 行
  bots.select_chr             BotSrv `选择角色 `            假人发出 CM_SELCHR 的次数
  bots.enter_game             BotSrv `准备进入游戏`
  bots.enter_game_ok          BotSrv `成功进入游戏`
  rust_login_srv.login_ok     `[登录] 帐号=`                每次登录 1 行
  rust_login_srv.select_server`[选服] account=`             每次选服 1 行
  rust_db_srv.query_chr       `[QueryChr] 候选`             **每次查询恰好 1 行**（推荐口径）
  rust_db_srv.query_chr_lines `[QueryChr]`                  **每次查询共 5 行**（候选/GetBy/dataIndex/GetQryChar/debug）
  rust_db_srv.start_play      `玩家获取游戏网关信息`         SM_STARTPLAY
  rust_sel_gate.connects      `开始连接:`                   SelGate 接受 TCP 连接
  rust_login_gate.connects    `开始连接:`                   LoginGate 接受 TCP 连接
  panics.*                    `panicked`                    Rust 进程 panic 行

> 教训（F-2，reviewer-2 条目 `ec2471d4`）：上一版把 `[QueryChr]` 全行数当「查询次数」入到
> `rust_db_srv.query_chr`（1000 = 200 次 × 5 行），报告写「候选 200」⇒ 无法从入库件复现；
> 且 login-gate 的连接匹配串写成 `客户端连接`（真实输出是 `开始连接:`）⇒ 恒 0 的死指标。
> 本脚本把「每次查询 1 行」与「每次查询 5 行」**分成两个键**，并把每个 pattern 原样写进
> 输出 JSON 的 `patterns` 字段，让口径机读可查。

用法::

  python tools/m1-acceptance/count_m1_logs.py \
    --bots  mir2-rs/tests/parity/m1-boundary-load/evidence/22-botsrv-bots.txt \
    --login-srv mir2-rs/tests/parity/m1-boundary-load/evidence/12-rust-login-srv.out.txt \
    --db-srv    mir2-rs/tests/parity/m1-boundary-load/evidence/13-rust-db-srv.out.txt \
    --sel-gate  mir2-rs/tests/parity/m1-boundary-load/evidence/14-rust-sel-gate.out.txt \
    --login-gate mir2-rs/tests/parity/m1-boundary-load/evidence/15-rust-login-gate.out.txt \
    --tier 200 --hold-sec 60 --criteria login,conn \
    --out mir2-rs/tests/parity/m1-boundary-load/evidence/07-rust-side-counts.json

  python tools/m1-acceptance/count_m1_logs.py --selftest    # 口径自检（正例/反例）
"""

import argparse
import json
import os
import re
import sys
import tempfile

# pattern 是**契约**：改动即改口径，必须在报告里同步。
PATTERNS = {
    "bots.query_chr": r"查询角色\.",
    "bots.select_chr": r"选择角色 ",
    "bots.enter_game": r"准备进入游戏",
    "bots.enter_game_ok": r"成功进入游戏",
    "rust_login_srv.login_ok": r"\[登录\] 帐号=",
    "rust_login_srv.select_server": r"\[选服\] account=",
    "rust_db_srv.query_chr": r"\[QueryChr\] 候选",
    "rust_db_srv.query_chr_lines": r"\[QueryChr\]",
    "rust_db_srv.start_play": r"玩家获取游戏网关信息",
    "rust_sel_gate.connects": r"开始连接:",
    "rust_login_gate.connects": r"开始连接:",
    "panics.login_gate": r"panicked",
    "panics.login_srv": r"panicked",
    "panics.sel_gate": r"panicked",
    "panics.db_srv": r"panicked",
}

# 每个计数走哪个 key 读哪份日志（key = PATTERNS 的前缀，value = 参数名）。
LOG_KEY_TO_ARG = {
    "bots.query_chr": "bots",
    "bots.select_chr": "bots",
    "bots.enter_game": "bots",
    "bots.enter_game_ok": "bots",
    "rust_login_srv.login_ok": "login_srv",
    "rust_login_srv.select_server": "login_srv",
    "rust_db_srv.query_chr": "db_srv",
    "rust_db_srv.query_chr_lines": "db_srv",
    "rust_db_srv.start_play": "db_srv",
    "rust_sel_gate.connects": "sel_gate",
    "rust_login_gate.connects": "login_gate",
    "panics.login_gate": "login_gate",
    "panics.login_srv": "login_srv",
    "panics.sel_gate": "sel_gate",
    "panics.db_srv": "db_srv",
}
def read_lines(path):
    """共享读（日志在活栈里被进程持有；Python 的 open 用 _SH_DENYNO，可读正在写的文件）。

    返回 None ⇒ 文件不存在（计数写 -1）。
    """
    if not path or not os.path.exists(path):
        return None
    with open(path, "rb") as fh:
        data = fh.read()
    # 线上是 GB2312/GBK 字节；计数只关心 ASCII/中文 pattern，统一按 utf-8 容错解码。
    # 解码失败用 errors=replace：pattern 若是纯 ASCII 仍能命中，中文 pattern 在这类日志里
    # 由 Rust log crate 写的是 UTF-8（源码字面量），不会被替换掉。
    return data.decode("utf-8", errors="replace").splitlines()


def count_lines(lines, pattern):
    if lines is None:
        return -1
    rx = re.compile(pattern)
    return sum(1 for ln in lines if rx.search(ln))


def build(paths_by_arg, tier, hold_sec, criteria, load_rc):
    logs = {arg: read_lines(p) for arg, p in paths_by_arg.items()}
    missing = sorted(arg for arg, p in paths_by_arg.items() if not p or not os.path.exists(p))
    counts = {}
    for pat_key, pattern in PATTERNS.items():
        arg = LOG_KEY_TO_ARG[pat_key]
        counts[pat_key] = count_lines(logs.get(arg), pattern)

    def take(prefix):
        return {k[len(prefix) + 1 :]: v for k, v in counts.items() if k.startswith(prefix + ".")}

    return {
        "tier": tier,
        "hold_sec": hold_sec,
        "criteria": criteria,
        "load_gate_rc": load_rc,
        "bots": dict(
            log=paths_by_arg.get("bots", ""),
            **take("bots"),
        ),
        "rust_login_srv": dict(
            log=paths_by_arg.get("login_srv", ""),
            **take("rust_login_srv"),
        ),
        "rust_db_srv": dict(
            log=paths_by_arg.get("db_srv", ""),
            **take("rust_db_srv"),
        ),
        "rust_sel_gate": dict(
            log=paths_by_arg.get("sel_gate", ""),
            **take("rust_sel_gate"),
        ),
        "rust_login_gate": dict(
            log=paths_by_arg.get("login_gate", ""),
            **take("rust_login_gate"),
        ),
        "panics": take("panics"),
        "patterns": dict(PATTERNS),
        "missing_logs": missing,
        "count_note": (
            "query_chr=[QueryChr] 候选（每次查询 1 行）；query_chr_lines=[QueryChr] 全部日志行"
            "（每次查询共 5 行：候选/GetBy/dataIndex/GetQryChar/debug）"
        ),
    }


def selftest():
    """正例+反例：口径必须能分辨「候选」与全部行，且必须命中真实输出 `开始连接:`。"""
    ok = True

    def expect(cond, msg):
        nonlocal ok
        if not cond:
            ok = False
            print("SELFTEST_RED: " + msg)

    db = "\n".join(
        [
            "[QueryChr] 候选 account=a i=0 name=[a] quickIndex=1",
            "[QueryChr]   GetBy -> result=0",
            "[QueryChr]   dataIndex=0 nChrCount=1",
            "[QueryChr]   GetQryChar -> True",
            "[QueryChr]:[a] nChrCount=1",
            "[QueryChr] 候选 account=b i=0 name=[b] quickIndex=2",
        ]
    )
    gate = "\n".join(["开始连接: 127.0.0.1:1", "开始连接: 127.0.0.1:2", "客户端连接 foo"])
    d = tempfile.mkdtemp(prefix="m1-count-selftest-")
    p_db = os.path.join(d, "db.txt")
    p_gate = os.path.join(d, "gate.txt")
    open(p_db, "w", encoding="utf-8").write(db)
    open(p_gate, "w", encoding="utf-8").write(gate)
    res = build(
        {"db_srv": p_db, "login_gate": p_gate, "bots": "", "login_srv": "", "sel_gate": ""},
        tier=999,
        hold_sec=0,
        criteria="login,conn",
        load_rc=None,
    )
    expect(res["rust_db_srv"]["query_chr"] == 2, "query_chr 应=2（候选行），实际 %r" % res["rust_db_srv"]["query_chr"])
    expect(
        res["rust_db_srv"]["query_chr_lines"] == 6,
        "query_chr_lines 应=6（全部 [QueryChr] 行），实际 %r" % res["rust_db_srv"]["query_chr_lines"],
    )
    expect(
        res["rust_login_gate"]["connects"] == 2,
        "login_gate.connects 应命中 `开始连接:` 2 行，实际 %r" % res["rust_login_gate"]["connects"],
    )
    expect("客户端连接" not in res["patterns"]["rust_login_gate.connects"], "死指标 pattern `客户端连接` 未清掉")
    expect(res["missing_logs"] == ["bots", "login_srv", "sel_gate"], "缺失日志应如实登记，实际 %r" % res["missing_logs"])
    if ok:
        print("SELFTEST_GREEN: count_m1_logs 口径自检通过")
    return 0 if ok else 1


def main():
    ap = argparse.ArgumentParser(description="M1⑤ 两侧证据计数（单一口径）")
    ap.add_argument("--bots", default="")
    ap.add_argument("--login-srv", dest="login_srv", default="")
    ap.add_argument("--db-srv", dest="db_srv", default="")
    ap.add_argument("--sel-gate", dest="sel_gate", default="")
    ap.add_argument("--login-gate", dest="login_gate", default="")
    ap.add_argument("--tier", type=int, default=0)
    ap.add_argument("--hold-sec", type=int, default=0)
    ap.add_argument("--criteria", default="login,conn")
    ap.add_argument("--load-rc", type=int, default=None, help="load_gate.ps1 的返回码（未跑留空）")
    ap.add_argument("--out", default="")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest()

    a = args
    paths = {
        "bots": a.bots,
        "login_srv": a.login_srv,
        "db_srv": a.db_srv,
        "sel_gate": a.sel_gate,
        "login_gate": a.login_gate,
    }
    out = build(paths, a.tier, a.hold_sec, a.criteria, a.load_rc)
    text = json.dumps(out, ensure_ascii=False, indent=4)
    if a.out:
        os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
        with open(a.out, "w", encoding="utf-8") as fh:
            fh.write(text + "\n")
        print("COUNTS " + a.out)
    else:
        print(text)
    if out["missing_logs"]:
        print("WRN 缺失日志: " + ", ".join(out["missing_logs"]), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
