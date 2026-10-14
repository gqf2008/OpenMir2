#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_healthspellchanged_trigger.py —— 卡 `openmir2-client-e2e-enter-gaps`
**片㊴（53 `SM_HEALTHSPELLCHANGED` 冻结客户端 `TCharDesc` 尾）R 窗口的机器判定/前置门**。

为什么需要（2026-10-14 实测到的真缺陷）：
  片㊴ 的真凶是 `SM_HEALTHSPELLCHANGED(53)` 按 oracle 发 **header-only**，而冻结客户端
  `MirClient@c61d912d` 的 `HIGHHP=True`（`Source/Common/Grobal2.pas:39`）在
  `Source/MirClient/ClMain.pas:10220` 的 53 支**不读 12B 命令头**，而是从**尾**解
  `SizeOf(TCharDesc)`=12B（`Grobal2.pas:3515`）⇒ oracle 的 header-only 帧让客户端把
  `HP/MP/MaxHP` 全解成 0（HUD `0/0`、`CanRun(HP>=10)` 拒绝跑）。
  本 R 窗口是「进图后就地不动、同一 socket」的场景，其中 53 由**周期恢复段**发出，其**前置为
  `HP < MaxHP`**（`src/M2Server/Actor/BaseObject.Base.cs:44-70` 的恢复段 → `HealthSpellChanged()`）。
  ⇒ **满血进图永不发 53**：既有 7 份真机 capture（`capture-client-*.jsonl` / `capture-run-ability-*.jsonl`）
  里 53 计数**全为 0**，窗口既证不了修复、也复现不了回归（白烧一个独占窗口）。
  且 `client-e2e-window.sh` / `check_rust_window_preflight.py` 都**没有**「保 `HP<MaxHP`」这条前置。

  ⚠ 措辞边界（审查 reviewer-5 O-1 / reviewer-6 O-2）：不要读成「53 **只**在 `HP<MaxHP` 时发出」。
  oracle 另有**不依赖该前置**的 53 发出点（不在本窗口场景内，只作边界记录）：
  `BaseObject.Base.cs:244`（`HP>MaxHP` 重算后钳到 `MaxHP-1`）、`:109`（饥饿掉血，守卫是 `HP>1`
  而非 `HP<MaxHP`）、`:628`（`RM_MAGSTRUCK` 受击）、`BaseObject.cs:973-975` `IncHealthSpell`
  （升级/属性重算，**无条件**调 `HealthSpellChanged()`）。⇒ T1 取 `hp<max_hp` 是本「进图不动」
  窗口的**充分前置**（有意更严），不是对全部 53 发点的等价刻画。

  已知边界（审查 Observation，不在本片扩）：
  · `derive_expect_body_len` 在「日志无已知 layout」时缺省 `12`（fail-closed 偏 RED 侧，不产生假绿）；
    若窗口用非 `client-58`/`oracle-40` 的新 layout 名，须显式 `--expect-body-len`（reviewer-5 O-2）。
  · T3 **只**判 53 尾的**长度**（12 vs 0），不判 12B 内容 = `[HP][MP][MaxHP]`；尾内容正确性由片㊴
    的 `smframe` 单测 + 金标准单独覆盖（reviewer-6 O-3）。

本工具把「53 触发前置 + 53 已发且尾长对」做成机判（fail-closed），三条判据：

  T1 `--log <game-svr.out.log>`：至少一条 `52/752 入图输入` 行的 `WAbil[...]` 里
     `hp < max_hp`（= 53 的触发前置）。满血进图 ⇒ RED（附可照抄的压血 SQL 提示）。
  T2 `--capture <capture.jsonl>`：s2c 至少 `--min-53` 条 `ident == 53`（= 53 真发出）。
  T3 `--capture`：53 的 `body_len == --expect-body-len`（默认从 T1 的 `layout=` 推：
     `client-58 ⇒ 12`（冻结客户端 `TCharDesc` 尾）、`oracle-40 ⇒ 0`（header-only））。
     **改坏必红**：拿修复前二进制（header-only）跑同一窗口 ⇒ T3 RED（`body_len=0`），
     而修复后 GREEN（`body_len=12`）。

输入口径：`--capture` = `tools/m2-acceptance/export_s2c.ps1` 在 7200 抓包解码出的 `capture.jsonl`
（`body_len` 是**解码后的原始 body 字节数**：52 客户端布局=58、oracle=40；53 客户端布局=12、oracle=0）。

退出码：`0`=GREEN、`1`=RED、`2`=PREREQ_RED（没给任何输入 / 文件缺 / 日志无入图行 / 解析不到）。
`--selftest` 不依赖外部件（合成日志 + 合成 capture 自证红/绿两向）。

用法：
  python -I -B check_healthspellchanged_trigger.py \
      --log E:/tmp/window/cliente2e/rust/stack/game-svr.out.log \
      --capture E:/tmp/window/cliente2e/rust/s2c/capture.jsonl
  python -I -B check_healthspellchanged_trigger.py \
      --log <log> --capture <cap> --expect-body-len 0   # oracle-40 对拍口径
  python -I -B check_healthspellchanged_trigger.py --selftest
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys

SM_HEALTHSPELLCHANGED = 53
ENTER_MARK = "52/752 入图输入"

# `52/752 入图输入 ChrName=aaa layout=client-58 tail_len=58 WAbil[level=1 ac=0 ... hp=18 mp=15 max_hp=19 ...]`
_RE_LAYOUT = re.compile(r"layout=(\S+)")
_RE_WABIL = re.compile(r"WAbil\[([^\]]*)\]")
_RE_KV = re.compile(r"([a-z_]+)=(-?\d+)")

# `layout=` ⇒ 53 的 body 原始字节数（尾长）。
LAYOUT_TO_53_BODY_LEN = {
    "client-58": 12,  # 冻结客户端 `TCharDesc`（3 × Integer）
    "oracle-40": 0,   # oracle `SendSocket` header-only
}


class PrereqError(Exception):
    """缺输入 / 不可解析 ⇒ 退出码 2（fail-closed，绝不静默通过）。"""


def parse_enter_log(path: str) -> list[dict]:
    """返回每条 `52/752 入图输入` 行的 {layout, hp, mp, max_hp, line_no}。"""
    if not os.path.isfile(path):
        raise PrereqError(f"日志不存在：{path}")
    out: list[dict] = []
    with open(path, encoding="utf-8", errors="replace") as fh:
        for i, line in enumerate(fh, 1):
            if ENTER_MARK not in line:
                continue
            rec: dict = {"line_no": i}
            m = _RE_LAYOUT.search(line)
            rec["layout"] = m.group(1) if m else None
            mw = _RE_WABIL.search(line)
            if mw:
                rec.update({k: int(v) for k, v in _RE_KV.findall(mw.group(1))})
            out.append(rec)
    if not out:
        raise PrereqError(f"日志里没有 `{ENTER_MARK}` 行：{path}")
    return out


def load_capture(path: str) -> list[dict]:
    if not os.path.isfile(path):
        raise PrereqError(f"capture 不存在：{path}")
    rows: list[dict] = []
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise PrereqError(f"capture 不可解析（{path}）：{exc}") from exc
    if not rows:
        raise PrereqError(f"capture 为空：{path}")
    return rows


def derive_expect_body_len(log_recs: list[dict], explicit: int | None) -> tuple[int, str]:
    """53 期望 body_len：显式 > 最后一个 client-58/oracle-40 layout > 默认 12。"""
    if explicit is not None:
        return explicit, "显式 --expect-body-len"
    for rec in reversed(log_recs):
        if rec.get("layout") in LAYOUT_TO_53_BODY_LEN:
            return LAYOUT_TO_53_BODY_LEN[rec["layout"]], f"由日志 layout={rec['layout']} 推得"
    return 12, "默认（未给 --expect-body-len 且日志无已知 layout）"


def check(log_recs: list[dict] | None, rows: list[dict] | None,
          expect_body_len: int | None, min_53: int) -> tuple[int, list[str], dict]:
    msgs: list[str] = []
    facts: dict = {}
    red = False

    # ---- T1：53 触发前置（HP < MaxHP）----
    if log_recs is not None:
        pre = [r for r in log_recs if "hp" in r and "max_hp" in r and r["hp"] < r["max_hp"]]
        facts["enter_lines"] = len(log_recs)
        facts["enter_precondition_lines"] = len(pre)
        facts["enter_samples"] = [
            {"line": r["line_no"], "layout": r.get("layout"),
             "hp": r.get("hp"), "max_hp": r.get("max_hp")}
            for r in log_recs
        ]
        if not pre:
            red = True
            msgs.append(
                "RED T1 53 触发前置不满足：全部 %d 条 `%s` 都是 hp>=max_hp（满血进图 ⇒ 53 永不发）"
                "；请先压血再进图（见 82-two-side-window-runbook.md §2b 的 SQL），否则本窗口覆盖不到片㊴"
                % (len(log_recs), ENTER_MARK)
            )
        else:
            first = pre[0]
            msgs.append(
                "OK  T1 53 触发前置：%d/%d 条入图行 hp<max_hp（首条 hp=%s<%s max_hp=%s）"
                % (len(pre), len(log_recs), first["hp"], first["max_hp"], first["max_hp"])
            )

    # ---- T2/T3：53 已发 + 尾长 ----
    if rows is not None:
        f53 = [r for r in rows if r.get("dir") == "s2c" and r.get("ident") == SM_HEALTHSPELLCHANGED]
        lens = sorted({r.get("body_len") for r in f53})
        facts["s2c_53"] = len(f53)
        facts["s2c_53_body_lens"] = lens
        exp, why = derive_expect_body_len(log_recs or [], expect_body_len)
        facts["expect_body_len"] = exp
        facts["expect_body_len_why"] = why

        if len(f53) < min_53:
            red = True
            msgs.append(
                "RED T2 s2c 里 `%d SM_HEALTHSPELLCHANGED` 只有 %d 条 < 要求 %d：53 未触发/未下发"
                "（先查 T1 前置；既有 7 份 capture 全 0 条即全满血进图所致）"
                % (SM_HEALTHSPELLCHANGED, len(f53), min_53)
            )
        else:
            msgs.append("OK  T2 s2c 53 条数=%d >= %d（修复路径被真实覆盖）" % (len(f53), min_53))

        if f53 and lens != [exp]:
            red = True
            msgs.append(
                "RED T3 53 body_len=%s，期望 [%d]（%s）；body_len=0 = oracle header-only（冻结客户端"
                " HIGHHP=True 会把 HP/MP/MaxHP 解成 0 ⇒ HUD 0/0、跑门关）"
                % (lens, exp, why)
            )
        elif f53:
            msgs.append("OK  T3 53 body_len=%d（%s）" % (exp, why))
    else:
        # 只给 --log ⇒ T1 可判、T2/T3 无输入参与；显式报出未检查面，维持 fail-closed 体裁。
        # （reviewer-6 O-1：不打印会让人把 T1-only 的 GREEN 误读成「53 已发且尾长对」。）
        msgs.append("INFO T2/T3 NOT CHECKED（未给 --capture）")

    rc = 1 if red else 0
    return rc, msgs, facts


# ---------------------------------------------------------------------------
# selftest
# ---------------------------------------------------------------------------

def _log_line(layout: str, hp: int, max_hp: int) -> str:
    return (
        "07:01:43.222 [DBG] 52/752 入图输入 ChrName=aaa layout=%s tail_len=58 "
        "WAbil[level=1 ac=0 mac=0 dc=257 mc=0 sc=0 hp=%d mp=15 max_hp=%d max_mp=15 "
        "exp=0 max_exp=10 weight=0 max_weight=50 wear_weight=0 max_wear_weight=15 "
        "hand_weight=0 max_hand_weight=12] ability_tail_hex=00" % (layout, hp, max_hp)
    )


def _cap(ident: int, body_len: int, direction: str = "s2c") -> dict:
    return {"kind": "frame", "dir": direction, "ident": ident, "body_len": body_len,
            "param": 18, "tag": 15, "series": 19, "recog": 17886}


def selftest() -> int:
    cases: list[tuple[str, list[str] | None, list[dict] | None, int | None, int, int, str | None]] = [
        # name, log lines, capture rows, expect_body_len, min_53, want_rc, want_msg(substring)
        ("green-client58", [_log_line("client-58", 18, 19)],
         [_cap(52, 58), _cap(53, 12)], None, 1, 0, None),
        ("green-oracle40-derived", [_log_line("oracle-40", 18, 19)],
         [_cap(52, 40), _cap(53, 0)], None, 1, 0, None),
        ("red-precondition-full-hp", [_log_line("client-58", 19, 19)],
         [_cap(52, 58), _cap(53, 12)], None, 1, 1, None),
        ("red-53-absent", [_log_line("client-58", 18, 19)],
         [_cap(52, 58)], None, 1, 1, None),
        ("red-53-header-only", [_log_line("client-58", 18, 19)],
         [_cap(52, 58), _cap(53, 0)], None, 1, 1, None),
        ("red-53-mixed-lens", [_log_line("client-58", 18, 19)],
         [_cap(53, 12), _cap(53, 0)], None, 1, 1, None),
        ("red-min53-2", [_log_line("client-58", 18, 19)],
         [_cap(53, 12)], None, 2, 1, None),
        ("green-explicit-0", None, [_cap(53, 0)], 0, 1, 0, None),
        # reviewer-6 O-1 牙：只给 --log（无 capture）⇒ T1 过即 GREEN，但必须显式报 T2/T3 未检查
        ("green-log-only-t23-skipped", [_log_line("client-58", 18, 19)],
         None, None, 1, 0, "INFO T2/T3 NOT CHECKED"),
    ]
    bad = 0
    for name, log_lines, rows, exp, min53, want, want_msg in cases:
        log_recs = None
        if log_lines is not None:
            # 走真实的解析路径（合成到临时文件），确保 selftest 咬住解析而非绕过
            import tempfile
            with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".log",
                                             delete=False) as fh:
                fh.write("\n".join(log_lines) + "\n")
                tmp = fh.name
            try:
                log_recs = parse_enter_log(tmp)
            finally:
                os.unlink(tmp)
        rc, _msgs, _facts = check(log_recs, rows, exp, min53)
        ok = rc == want
        if want_msg is not None:
            # 文案牙：INFO 必须在输出里，否则「T1-only 静默 GREEN」会退化回来
            ok = ok and any(want_msg in m for m in _msgs)
        bad += 0 if ok else 1
        print("[%s] selftest %-28s rc=%d (want %d%s)"
              % ("PASS" if ok else "FAIL", name, rc, want,
                 "" if want_msg is None else ", msg~=%r" % want_msg))
    print("SELFTEST %s (%d/%d)" % ("OK" if bad == 0 else "FAILED", len(cases) - bad, len(cases)))
    return 0 if bad == 0 else 1


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--log", default=None, help="Rust game-svr 日志（含 `52/752 入图输入` 行）")
    ap.add_argument("--capture", default=None, help="export_s2c.ps1 产出的 capture.jsonl")
    ap.add_argument("--expect-body-len", type=int, default=None,
                    help="53 期望 body_len（12=client-58 / 0=oracle；缺省由日志 layout 推）")
    ap.add_argument("--min-53", type=int, default=1, help="s2c 53 最少条数（默认 1）")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()
    if not args.log and not args.capture:
        print("PREREQ_RED 必须给 --log 和/或 --capture")
        return 2
    try:
        log_recs = parse_enter_log(args.log) if args.log else None
        rows = load_capture(args.capture) if args.capture else None
    except PrereqError as exc:
        print(f"PREREQ_RED {exc}")
        return 2

    rc, msgs, facts = check(log_recs, rows, args.expect_body_len, args.min_53)
    for m in msgs:
        print(m)
    print("facts " + json.dumps(facts, ensure_ascii=False, sort_keys=True))
    print("VERDICT " + ("GREEN" if rc == 0 else "RED"))
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
