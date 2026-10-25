#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""butch 账本两抽判据（卡 `openmir2-butch-two-draws-unblock`，tier R 的工具面）。

判据（oracle `src/M2Server/Player/PlayObject.Operate.cs:702 ClientGetButchItem`）：

  * `:722` `Random(16)+5` 减 `BodyLeathery` —— 账本里 **恰 1 条** `Next(max) max=16`，且 site 属**屠宰函数**；
  * `:723` `Random(201)+100` 减 `MeatQuality` —— 同理 `max=201` **恰 1 条**；
  * 两条的 `seq` 必须**16 在前**，值域 `[5,20]` / `[100,300]`；
  * `SM_BUTCH(637)`（`:749` 无条件发）**不是判据**，只作伴随证据。

三个「不可判」条件（**exit 2**，不是 FAIL）：

  1. **site 未开**：C# 账本在 `MIR2_RNG_SITE=1` 未设时只写 4 列（无 site）⇒ 无法归属到屠宰函数；
  2. **有坏行**：任一行无法解析 ⇒ 账本完整性存疑，不给结论；
  3. **拿不到屠宰函数 span**（Rust 侧）：Rust 账本的 site 只有 `file:line`、没有函数名，
     必须把行号对到 `butch_effect_now` 的定义区间；读不到源码 ⇒ 判不了，**不许退化成「文件对就行」**。

**为什么 Rust 必须对 span**（reviewer-21 F2）：早先只要求 site 里含 `world.rs`，于是把两抽写成
`world.rs:1234`（不在屠宰函数、次序正确）也判 `TWO_DRAWS_OK`。今天生产代码里两种模数各只 1 处，
但将来 `world.rs` 多一处同模抽就会「没屠宰也算过」。

账本格式（`seq \\t kind \\t a \\t b \\t site`，`MIR2_RNG_SITE=1` 才写 5 列）：

    python -I -B tools/window/butch_ledger.py <rng-stream.tsv> --side csharp|rust [--json OUT]
    python -I -B tools/window/butch_ledger.py --selftest

退出码：0 = 两抽成立；1 = 不成立（stdout 打印原因）；2 = 不可判 / 用法错误。
"""
import io
import json
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RUST_SRC = os.path.join(REPO_ROOT, "mir2-rs", "crates", "world", "src", "world.rs")
RUST_FN_SIG = "fn butch_effect_now("
C_SRC = os.path.join(REPO_ROOT, "src", "M2Server", "Player", "PlayObject.Operate.cs")
C_FN_SIG = "ClientGetButchItem("

#: C# 账本的 site 是 `Class.Method@File.cs:LINE` ⇒ **函数名本身就是身份**，不需要再对 span。
C_SITE_MARK = ("ButchItem", "PlayObject.Operate.cs")


def _strip_code(text):
    """去掉注释与字符串字面量（保留换行），供括号配平用。

    注意：**裸 grep 会误报**——注释里就写着 `Random(16)`/`Random(201)`（reviewer-21 提醒过），
    所以任何「按源码判站点」的牙都必须先剥注释/字符串。
    """
    out = []
    i, n = 0, len(text)
    state = None                      # None | "line" | "block" | '"' | "'"
    while i < n:
        ch = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if state is None:
            if ch == "/" and nxt == "/":
                state = "line"
                i += 2
                continue
            if ch == "/" and nxt == "*":
                state = "block"
                i += 2
                continue
            if ch in ('"', "'"):
                state = ch
                out.append(" ")
                i += 1
                continue
            out.append(ch)
            i += 1
            continue
        if state == "line":
            if ch == "\n":
                state = None
                out.append(ch)
            i += 1
            continue
        if state == "block":
            if ch == "*" and nxt == "/":
                state = None
                i += 2
                continue
            if ch == "\n":
                out.append(ch)
            i += 1
            continue
        # 字符串字面量内部
        if ch == "\\":
            i += 2
            continue
        if ch == state:
            state = None
        i += 1
    return "".join(out)


def fn_span(path, sig):
    """返回 `sig` 所在函数/方法在**剥注释后**的行区间 `(start_line, end_line)`（1-based）。

    找不到文件/签名/配平失败 ⇒ `None`（调用方按「不可判」处理，fail-closed）。
    """
    try:
        raw = io.open(path, "r", encoding="utf-8", errors="replace").read()
    except OSError:
        return None
    text = _strip_code(raw)
    lines = text.split("\n")
    start = None
    for k, line in enumerate(lines):
        if sig in line:
            start = k
            break
    if start is None:
        return None
    depth = 0
    seen = False
    for k in range(start, len(lines)):
        for ch in lines[k]:
            if ch == "{":
                depth += 1
                seen = True
            elif ch == "}":
                depth -= 1
                if seen and depth == 0:
                    return (start + 1, k + 1)
    return None


def resolve_span(side):
    if side == "rust":
        return fn_span(RUST_SRC, RUST_FN_SIG)
    return fn_span(C_SRC, C_FN_SIG)


def _site_line(site):
    m = re.search(r":(\d+)\s*$", site or "")
    return int(m.group(1)) if m else None


def parse(path):
    rows, stats = [], {"no_site": 0, "unparsable": 0}
    with io.open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\r\n")
            if not line or line.startswith("SEED_PROBE"):
                continue
            parts = line.split("\t")
            if len(parts) >= 2 and parts[1] == "INJECT_OK":
                continue                  # 头部自证行（4 列），不是「site 未开」
            if len(parts) == 4:           # MIR2_RNG_SITE=1 未开：无 site 列
                stats["no_site"] += 1
                continue
            if len(parts) < 5:
                stats["unparsable"] += 1
                continue
            try:
                seq, a, b = int(parts[0]), int(parts[2]), int(parts[3])
            except ValueError:
                stats["unparsable"] += 1
                continue
            rows.append({"seq": seq, "kind": parts[1], "a": a, "b": b, "site": parts[4]})
    return rows, stats


def analyse(path, side, span=None, span_resolved=True):
    """返回结论 dict。`span` 可注入（自测用）；`span_resolved=False` 表示拿不到 span。"""
    rows, stats = parse(path)
    out = {"ledger": path, "side": side, "rows": len(rows),
           "no_site_rows": stats["no_site"], "unparsable": stats["unparsable"],
           "span": list(span) if span else None, "draws": {}}

    def in_fn(r):
        if side == "csharp":
            return any(m in r["site"] for m in C_SITE_MARK)
        ln = _site_line(r["site"])
        return bool(span) and ln is not None and span[0] <= ln <= span[1]

    for want in ("16", "201"):
        n = int(want)
        alln = [r for r in rows if r["kind"] == "Next(max)" and r["a"] == n]
        hit = [r for r in alln if in_fn(r)]
        out["draws"][want] = {
            "in_butch_fn": len(hit),
            "total": len(alln),
            "other_sites": sorted({r["site"] for r in alln if r not in hit})[:5],
            "hits": [{"seq": r["seq"], "site": r["site"], "value": r["b"]} for r in hit[:5]],
        }

    # ---- 不可判（exit 2）：先判，不能和 FAIL 混 ----
    und = []
    if side == "rust" and span is None:
        und.append("拿不到 `butch_effect_now` 的定义区间（源码缺失/签名未找到/配平失败）⇒ "
                   "Rust 账本的 site 只有 file:line，无法排除「同文件其它函数同模抽」")
    if stats["no_site"] > 0:
        und.append("账本有 %d 行**没有 site 列**（`MIR2_RNG_SITE=1` 未开）⇒ 无法归属屠宰函数"
                   % stats["no_site"])
    if stats["unparsable"] > 0:
        und.append("账本有 %d 行无法解析 ⇒ 完整性存疑" % stats["unparsable"])
    if und:
        out["problems"] = und
        out["verdict"] = "UNDECIDABLE"
        return out

    problems = []
    for want in ("16", "201"):
        got = out["draws"][want]["in_butch_fn"]
        if got != 1:
            problems.append("max=%s 在屠宰函数内出现 %d 条（应为恰 1 条）" % (want, got))
    if not problems:
        s16 = out["draws"]["16"]["hits"][0]
        s201 = out["draws"]["201"]["hits"][0]
        out["values"] = {"n10": s16["value"] + 5, "meat": s201["value"] + 100}
        out["order_ok"] = s16["seq"] < s201["seq"]
        out["sites"] = {"16": s16["site"], "201": s201["site"]}
        if not out["order_ok"]:
            problems.append("两抽次序反了（max=16 seq=%d 应早于 max=201 seq=%d）"
                            % (s16["seq"], s201["seq"]))
        if not 5 <= out["values"]["n10"] <= 20:
            problems.append("n10=%d 越出 [5,20]" % out["values"]["n10"])
        if not 100 <= out["values"]["meat"] <= 300:
            problems.append("meat=%d 越出 [100,300]" % out["values"]["meat"])
    out["problems"] = problems
    out["verdict"] = "TWO_DRAWS_OK" if not problems else "TWO_DRAWS_FAIL"
    return out


# ---------------------------------------------------------------- selftest
_H = "1\tINJECT_OK\t42\t0\nSEED_PROBE 1 2 3\n"
_MG = "4054\tNext(max)\t201\t35\tGameSrv.Word.WorldServer.RegenMonsters@WorldServer.MonGen.cs:815\n"
_C16 = "5000\tNext(max)\t16\t7\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:722\n"
_C201 = "5001\tNext(max)\t201\t123\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:723\n"
_BS = chr(92)


def _rust_site(tail):
    return _BS.join(["crates", "world", "src", tail])


def _case(rows, side="csharp", span=(100, 200)):
    import tempfile
    fd, path = tempfile.mkstemp(suffix=".tsv")
    try:
        with io.open(fd, "w", encoding="utf-8", newline="") as f:
            f.write(_H + "".join(rows))
        return analyse(path, side, span=span)
    finally:
        os.remove(path)


def _selftest():
    checks = []

    def ok(name, cond, detail=""):
        checks.append((bool(cond), name, detail))

    good = _case([_MG, _C16, _C201])
    ok("a 两抽成立（含 MonGen 干扰）", good["verdict"] == "TWO_DRAWS_OK"
       and good["values"] == {"n10": 12, "meat": 223} and good["order_ok"], good.get("problems"))
    # 次序判定看的是账本里的 **seq**，不是列表顺序 ⇒ 这里显式把 seq 反写（201 在前）。
    _C201_early = "4999\tNext(max)\t201\t123\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:723\n"
    bad_order = _case([_C201_early, _C16])
    ok("b 次序反了 ⇒ FAIL", bad_order["verdict"] == "TWO_DRAWS_FAIL"
       and any("次序" in p for p in bad_order["problems"]), bad_order.get("problems"))
    twice = _case([_C16, _C16, _C201])
    ok("c max=16 两条 ⇒ FAIL", twice["verdict"] == "TWO_DRAWS_FAIL", twice.get("problems"))
    only_mg = _case([_MG])
    ok("d 只有 MonGen 的 max=201 ⇒ FAIL", only_mg["verdict"] == "TWO_DRAWS_FAIL",
       only_mg.get("problems"))
    rust_ok = _case(["6442\tNext(max)\t201\t193\t" + _rust_site("mongen.rs:899") + "\n",
                     "7000\tNext(max)\t16\t3\t" + _rust_site("world.rs:150") + "\n",
                     "7001\tNext(max)\t201\t55\t" + _rust_site("world.rs:151") + "\n"],
                    side="rust", span=(100, 200))
    ok("e Rust 侧两抽落在 span 内 ⇒ OK", rust_ok["verdict"] == "TWO_DRAWS_OK",
       rust_ok.get("problems") or rust_ok.get("sites"))
    # F2 的洞：同文件但不在屠宰函数（行号落在 span 外）⇒ 必须 FAIL
    outside = _case(["7000\tNext(max)\t16\t3\t" + _rust_site("world.rs:1234") + "\n",
                     "7001\tNext(max)\t201\t55\t" + _rust_site("world.rs:1235") + "\n"],
                    side="rust", span=(100, 200))
    ok("f Rust 同文件但 span 外 ⇒ FAIL（旧实现会判 OK）",
       outside["verdict"] == "TWO_DRAWS_FAIL", outside.get("problems"))
    # F1：site 未开（4 列账本）⇒ 不可判（exit 2），不是 FAIL
    nos_site = _case([])
    import tempfile
    fd, p4 = tempfile.mkstemp(suffix=".tsv")
    try:
        with io.open(fd, "w", encoding="utf-8", newline="") as f:
            f.write("1\tINJECT_OK\t42\t0\n5000\tNext(max)\t16\t7\n5001\tNext(max)\t201\t123\n")
        nos_site = analyse(p4, "csharp", span=(100, 200))
    finally:
        os.remove(p4)
    ok("g site 未开（4 列）⇒ UNDECIDABLE", nos_site["verdict"] == "UNDECIDABLE"
       and nos_site["no_site_rows"] == 2, nos_site.get("problems"))
    # F3：一条坏行 ⇒ 不可判
    broken = _case([_C16, _C201, "5002\tNext(max)\txx\t7\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:724\n"])
    ok("h 账本有坏行 ⇒ UNDECIDABLE", broken["verdict"] == "UNDECIDABLE"
       and broken["unparsable"] == 1, broken.get("problems"))
    # 拿不到 span ⇒ 不可判（fail-closed，不退化成「文件对就行」）
    nospan = _case(["7000\tNext(max)\t16\t3\t" + _rust_site("world.rs:7756") + "\n",
                    "7001\tNext(max)\t201\t55\t" + _rust_site("world.rs:7757") + "\n"],
                   side="rust", span=None)
    ok("i Rust 拿不到 span ⇒ UNDECIDABLE", nospan["verdict"] == "UNDECIDABLE",
       nospan.get("problems"))
    # 真实源码的 span 必须能解析出来（否则工具在真机上恒 UNDECIDABLE）
    real = fn_span(RUST_SRC, RUST_FN_SIG)
    ok("j 仓库源码里能解析出 butch_effect_now 的区间", bool(real) and real[1] > real[0],
       real)

    # F2 修法 (a)：**静态牙**——剥注释/字符串后，生产代码里两种模数的抽数必须唯一且同函数。
    # 裸 grep 会误报（注释里就写着 Random(16)/Random(201)），所以必须走 _strip_code。
    def _occurrences(path, span, pattern):
        try:
            text = _strip_code(io.open(path, "r", encoding="utf-8", errors="replace").read())
        except OSError:
            return None, None
        # 测试模块（`#[cfg(test)]` 起）不算生产代码——reviewer-21 提到的「10184+ 都在 cfg(test)」正是这里。
        cut = text.find("#[cfg(test)]")
        if cut != -1:
            text = text[:cut]
        lines = text.split(chr(10))
        rx = re.compile(pattern)
        total = sum(len(rx.findall(l)) for l in lines)
        inside = sum(len(rx.findall(l)) for i, l in enumerate(lines)
                     if span and span[0] - 1 <= i <= span[1] - 1)
        return total, inside

    _rx16 = "random_below" + chr(92) + "(" + chr(92) + "s*16" + chr(92) + "s*" + chr(92) + ")"
    _rx201 = "random_below" + chr(92) + "(" + chr(92) + "s*201" + chr(92) + "s*" + chr(92) + ")"
    r16, r16_in = _occurrences(RUST_SRC, real, _rx16)
    r201, r201_in = _occurrences(RUST_SRC, real, _rx201)
    ok("k 静态牙（Rust）：剥注释后 random_below(16)/(201) 全文件各恰 1 处且在 butch_effect_now 内",
       r16 == 1 and r16_in == 1 and r201 == 1 and r201_in == 1, (r16, r16_in, r201, r201_in))

    _cx16 = "Random" + chr(92) + "(" + chr(92) + "s*16" + chr(92) + "s*" + chr(92) + ")"
    _cx201 = "Random" + chr(92) + "(" + chr(92) + "s*201" + chr(92) + "s*" + chr(92) + ")"
    cspan = fn_span(C_SRC, C_FN_SIG)
    c16, c16_in = _occurrences(C_SRC, cspan, _cx16)
    c201, c201_in = _occurrences(C_SRC, cspan, _cx201)
    ok("l 静态牙（C#）：屠宰函数内 Random(16)/(201) 各恰 1 处（文件内其它 Random(201) 不算）",
       cspan is not None and c16_in == 1 and c201_in == 1, (c16, c16_in, c201, c201_in, cspan))

    bad = [c for c in checks if not c[0]]
    for cond, name, detail in checks:
        print("%s %-50s %s" % ("PASS" if cond else "FAIL", name, detail if not cond else ""))
    total = len(checks)
    print("%s: butch_ledger selftest %d/%d"
          % ("RED" if bad else "GREEN", total - len(bad), total))
    return 1 if bad else 0


def main(argv):
    if "--selftest" in argv:
        return _selftest()
    if len(argv) < 2:
        print(__doc__)
        return 2
    path = argv[1]
    side = "csharp"
    if "--side" in argv:
        side = argv[argv.index("--side") + 1]
    if side not in ("csharp", "rust"):
        print("--side 只接受 csharp|rust：%s" % side)
        return 2
    if not os.path.exists(path):
        print("账本不存在: %s" % path)
        return 2
    span = resolve_span(side)
    out = analyse(path, side, span=span)
    if "--json" in argv:
        with io.open(argv[argv.index("--json") + 1], "w", encoding="utf-8", newline="") as f:
            json.dump(out, f, ensure_ascii=False, indent=1)
    print(json.dumps(out, ensure_ascii=False, indent=1))
    if out["verdict"] == "TWO_DRAWS_OK":
        return 0
    return 2 if out["verdict"] == "UNDECIDABLE" else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
