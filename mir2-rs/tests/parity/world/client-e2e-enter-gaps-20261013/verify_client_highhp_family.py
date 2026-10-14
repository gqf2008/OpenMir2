#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""verify_client_highhp_family.py — 从**冻结客户端源码**枚举 `{$IF HIGHHP}` 家族，机判「已闭环集合」。

卡：`openmir2-client-e2e-enter-gaps`（tier R）。片㊴/㊼ 修好了 52/53，但 `HIGHHP` 是一个**编译期
记录布局开关**（`Source/Common/Grobal2.pas:39 HIGHHP = True`）：凡客户端在 `{$IF HIGHHP}` 下按
**另一种字节布局**解的服务端帧，都必须有「客户端布局」侧实现，否则冻结客户端把字段读成 0/错位
（与 52/53 同源）。本工具把这份「家族清单」做成 fail-closed 机判，堵两种漂移：

  1. **客户端漂移**：冻结客户端若新增/删除一个 `{$IF HIGHHP}` 站点（换客户端版本/改 HIGHHP），
     下面的「已登记集合」不再成立 ⇒ 必须重新判定（RED，认不出即红）。
  2. **Rust 漂移**：52/53/31 的客户端布局实现（`encode_ability_client` /
     `sm_healthspellchanged_client` / `sm_struck_client`）被删掉/改名 ⇒ RED。

它**不**改任何生产代码，只读源码（零起栈/零端口）。真机 R 判据仍归 root 独占窗口。

期望集合（2026-10-14 实测，锚点在 `213-highhp-family-audit.txt`）：

  Grobal2.pas 记录（`{$IF HIGHHP}` 改记录字段尺寸）：
    - `TMonInfo`   : 客户端 `Monster.DB` 侧记录，**不从服务端帧解**（非帧面）。
    - `TAbility`   : `SM_ABILITY(52)` 的载荷 —— Rust 侧 **已闭环**（`encode_ability_client`，片⑨/㊼）。
    - `TAddAbility`: 客户端死记录（全仓仅定义、零使用点），非帧面。
  ClMain.pas 处理器（`{$IF HIGHHP}` 改 HP/MP/MaxHP 的读取来源）：
    - `SM_HEALTHSPELLCHANGED(53)` : **已闭环**（`sm_healthspellchanged_client`，片㊴）。
    - `SM_STRUCK(31)`             : **已闭环**（`sm_struck_client`，片「SM_STRUCK 客户端布局」）——
                                    客户端从 `param/tag` 拼 32 位 HP、从尾 `MessageBodyWL.Param1`
                                    取 MaxHP；生产默认发客户端布局，`MIR2_CLIENT_ABILITY_LAYOUT=0`
                                    才回 oracle（`param=HP, tag=MaxHP, 尾.Param1=feature`）。
    - `SM_OPENHEALTH(1100)`       : Rust 生产路径**未发送**（C# `PlayMessage.cs:2010` 发 oracle 布局）。
    - `SM_INSTANCEHEALGUAGE(1103)`: Rust 生产路径**未发送**（C# `PlayMessage.cs:2019`）。

用法：
  python verify_client_highhp_family.py                       # 默认冻结客户端 checkout
  python verify_client_highhp_family.py --client-root <dir>   # 指定 MirClient 检出
  python verify_client_highhp_family.py --rust-dir <dir>      # 追加 Rust 侧两函数存在性检查
  python verify_client_highhp_family.py --selftest            # 不依赖外部件

退出码：0 = GREEN（集合与登记一致）/ 1 = RED（漂移或 Rust 缺件）/ 2 = PREREQ_MISSING（找不到源码）。
"""
from __future__ import annotations

import argparse
import os
import re
import sys

DEFAULT_CLIENT_ROOT = os.environ.get(
    "MIR2_CLIENT_ROOT", "E:/Users/gxh/Documents/GitHub/MirClient")

# 已登记集合：`(文件, 上下文名)` -> 状态。上下文名 = Grobal2 的 record 名 / ClMain 的 `SM_*:` 标签。
EXPECTED = {
    ("Grobal2.pas", "TMonInfo"): "client-db-not-a-frame",
    ("Grobal2.pas", "TAbility"): "closed:encode_ability_client",
    ("Grobal2.pas", "TAddAbility"): "dead-record-no-uses",
    ("ClMain.pas", "SM_HEALTHSPELLCHANGED"): "closed:sm_healthspellchanged_client",
    ("ClMain.pas", "SM_STRUCK"): "closed:sm_struck_client",
    ("ClMain.pas", "SM_OPENHEALTH"): "not-sent-by-rust",
    ("ClMain.pas", "SM_INSTANCEHEALGUAGE"): "not-sent-by-rust",
}
# Rust 侧必须存在的客户端布局实现函数（缺 ⇒ RED）。
RUST_REQUIRED = (
    "encode_ability_client",
    "sm_healthspellchanged_client",
    "sm_struck_client",
)

_SITE_RE = re.compile(r"\{\$IF\s+HIGHHP\b", re.IGNORECASE)
_RECORD_RE = re.compile(r"^\s*([A-Za-z_]\w*)\s*=\s*(?:packed\s+)?record\b", re.IGNORECASE)
_CASE_RE = re.compile(r"^\s*(SM_[A-Z0-9_]+|RM_[A-Z0-9_]+)\s*:", re.IGNORECASE)
_HIGHHP_DEF_RE = re.compile(r"^\s*HIGHHP\s*=\s*(True|False)\s*;", re.MULTILINE | re.IGNORECASE)


def parse_highhp(text: str) -> bool | None:
    m = _HIGHHP_DEF_RE.search(text)
    return None if m is None else m.group(1).lower() == "true"


def enclosing_record(lines: list[str], idx: int) -> str | None:
    """向上找最近的 `X = record`。"""
    for j in range(idx, -1, -1):
        m = _RECORD_RE.match(lines[j])
        if m:
            return m.group(1)
    return None


def enclosing_case(lines: list[str], idx: int) -> str | None:
    """向上找最近的 `SM_*/RM_*:` 标签。"""
    for j in range(idx, -1, -1):
        m = _CASE_RE.match(lines[j])
        if m:
            return m.group(1).upper()
    return None


def enumerate_sites(text: str, filename: str) -> list[tuple[str, int]]:
    lines = text.splitlines()
    out: list[tuple[str, int]] = []
    for i, line in enumerate(lines):
        if not _SITE_RE.search(line):
            continue
        ctx = enclosing_case(lines, i) if filename.endswith("ClMain.pas") else enclosing_record(lines, i)
        out.append((ctx or f"<unknown@{i + 1}>", i + 1))
    return out


def judge(grobal2_text: str, clmain_text: str) -> tuple[int, list[str], list[tuple[str, int, str]]]:
    problems: list[str] = []
    table: list[tuple[str, int, str]] = []

    highhp = parse_highhp(grobal2_text)
    if highhp is not True:
        problems.append(f"Grobal2.pas 的 `HIGHHP` = {highhp}（期望 True；整个家族判定的前提）")

    seen: set[tuple[str, str]] = set()
    for filename, text in (("Grobal2.pas", grobal2_text), ("ClMain.pas", clmain_text)):
        for ctx, line in enumerate_sites(text, filename):
            key = (filename, ctx)
            status = EXPECTED.get(key)
            seen.add(key)
            if status is None:
                problems.append(f"未登记的 HIGHHP 站点：{filename}:{line} 上下文 `{ctx}`（认不出即红）")
                status = "UNREGISTERED"
            table.append((filename, line, f"{ctx} -> {status}"))

    for key, status in EXPECTED.items():
        if key not in seen:
            problems.append(f"登记站点缺失（客户端漂移）：{key[0]} `{key[1]}`（{status}）")

    return (1 if problems else 0), problems, table


def check_rust(rust_dir: str) -> list[str]:
    problems: list[str] = []
    blob = ""
    for root, _dirs, files in os.walk(rust_dir):
        for fn in files:
            if fn.endswith(".rs"):
                p = os.path.join(root, fn)
                try:
                    with open(p, "r", encoding="utf-8", errors="replace") as fh:
                        blob += fh.read()
                except OSError:
                    pass
    for sym in RUST_REQUIRED:
        # 必须是**函数定义** `fn <sym>`（子串匹配会把 `<sym>_REMOVED` 之类误当存在）。
        if not re.search(r"\bfn\s+" + re.escape(sym) + r"\b", blob):
            problems.append(f"Rust 侧缺客户端布局实现 `{sym}`（`--rust-dir {rust_dir}`）")
    return problems


# ------------------------------------------------------------------ selftest
SELFTEST_GROBAL2 = """
  HIGHHP = True;
  TMonInfo = packed record
    wLevel: Word;
{$IF HIGHHP}
    wMP: DWORD;
    wHP: DWORD;
{$ELSE}
    wMP: Word;
    wHP: Word;
{$IFEND}
  end;
  TAbility = packed record
{$IF HIGHHP}
    HP: DWORD;
{$IFEND}
  end;
  TAddAbility = record
{$IF HIGHHP}
    wHP: DWORD;
{$IFEND}
  end;
"""
SELFTEST_CLMAIN = """
    SM_HEALTHSPELLCHANGED:
      begin
{$IF HIGHHP}
        x := 1;
{$IFEND}
      end;
    SM_STRUCK:
      begin
{$IF HIGHHP}
        x := 1;
{$IFEND}
      end;
    SM_OPENHEALTH:
      begin
{$IF HIGHHP}
        x := 1;
{$IFEND}
      end;
    SM_INSTANCEHEALGUAGE:
      begin
{$IF HIGHHP}
        x := 1;
{$IFEND}
      end;
"""


def selftest() -> int:
    rc = 0

    def case(name: str, g: str, c: str, want_rc: int) -> None:
        nonlocal rc
        r, problems, _ = judge(g, c)
        ok = r == want_rc
        print(f"[PASS] selftest {name}: rc={r} (want {want_rc})"
              + ("" if ok else f" problems={problems}"))
        if not ok:
            rc = 1

    case("baseline-green", SELFTEST_GROBAL2, SELFTEST_CLMAIN, 0)
    case("highhp-false-red", SELFTEST_GROBAL2.replace("HIGHHP = True;", "HIGHHP = False;", 1),
         SELFTEST_CLMAIN, 1)
    # 删掉一个已登记站点 ⇒ 缺登记 ⇒ RED（客户端漂移）
    case("drop-site-red", SELFTEST_GROBAL2.replace("  TAbility = packed record\n{$IF HIGHHP}\n    HP: DWORD;\n{$IFEND}\n  end;\n", ""),
         SELFTEST_CLMAIN, 1)
    # 新增一个未登记站点 ⇒ 认不出即红
    case("extra-site-red", SELFTEST_GROBAL2,
         SELFTEST_CLMAIN + "\n    SM_NEWFRAME:\n      begin\n{$IF HIGHHP}\n        x := 1;\n{$IFEND}\n      end;\n", 1)
    # Rust 侧缺件（用临时空目录 + 合成最小 .rs）
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        with open(os.path.join(tmp, "x.rs"), "w", encoding="utf-8") as fh:
            fh.write("fn encode_ability_client() {}\n")  # 缺 sm_healthspellchanged_client / sm_struck_client
        missing = check_rust(tmp)
        ok = len(missing) == 2 and "sm_healthspellchanged_client" in missing[0]\
            and "sm_struck_client" in missing[1]
        print(f"[PASS] selftest rust-missing-red: missing={missing}" if ok
              else f"[FAIL] selftest rust-missing-red: missing={missing}")
        if not ok:
            rc = 1
        with open(os.path.join(tmp, "y.rs"), "w", encoding="utf-8") as fh:
            fh.write("fn sm_healthspellchanged_client() {}\nfn sm_struck_client() {}\n")
        ok = check_rust(tmp) == []
        print("[PASS] selftest rust-present-green" if ok else "[FAIL] selftest rust-present-green")
        if not ok:
            rc = 1

    print("SELFTEST " + ("OK (6/6)" if rc == 0 else "FAILED"))
    return rc


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--client-root", default=DEFAULT_CLIENT_ROOT,
                    help=f"冻结 MirClient 检出（默认 {DEFAULT_CLIENT_ROOT}）")
    ap.add_argument("--rust-dir", default=None,
                    help="可选：Rust 侧目录（追加两函数存在性检查）")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest()

    grobal2 = os.path.join(args.client_root, "Source/Common/Grobal2.pas")
    clmain = os.path.join(args.client_root, "Source/MirClient/ClMain.pas")
    for p in (grobal2, clmain):
        if not os.path.isfile(p):
            print(f"PREREQ_MISSING: 找不到冻结客户端源码 {p}")
            return 2

    with open(grobal2, "r", encoding="utf-8", errors="replace") as fh:
        g_text = fh.read()
    with open(clmain, "r", encoding="utf-8", errors="replace") as fh:
        c_text = fh.read()

    rc, problems, table = judge(g_text, c_text)
    highhp = parse_highhp(g_text)
    print(f"# client-root = {args.client_root}")
    print(f"HIGHHP = {highhp}")
    print("HIGHHP 站点（文件:行 上下文 -> 状态）：")
    for filename, line, desc in table:
        print(f"  {filename}:{line} {desc}")
    if args.rust_dir:
        rust_problems = check_rust(args.rust_dir)
        if rust_problems:
            problems.extend(rust_problems)
            rc = 1
        else:
            print(f"Rust 侧客户端布局实现齐备：{', '.join(RUST_REQUIRED)}")
    if problems:
        print("RED:")
        for p in problems:
            print(f"  - {p}")
        return 1
    print(f"GREEN: {len(table)} 个 HIGHHP 站点与登记集合逐项一致（3 闭环 / 1 客户端库 / 1 死记录 / 2 未发送）")
    return rc


if __name__ == "__main__":
    sys.exit(main())
