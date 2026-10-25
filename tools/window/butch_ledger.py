#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""butch 账本两抽判据（卡 `openmir2-butch-two-draws-unblock`，tier L 工具面）。

判据（oracle `src/M2Server/Player/PlayObject.Operate.cs:702 ClientGetButchItem`）：

  * `:722` `Random(16)+5` 减 `BodyLeathery` —— 账本里 **恰 1 条** `Next(max) max=16`
    且 site 属屠宰函数；
  * `:723` `Random(201)+100` 减 `MeatQuality` —— 同理 `max=201` **恰 1 条**；
  * 两条的 `seq` 必须**16 在前**（次序）；
  * `SM_BUTCH(637)`（`:749` 无条件发）**不是判据**，只作伴随证据。

**为什么必须按 site 过滤**：`max=201` 在真机账本里有几百条来自 `MonGen.cs:815`
（刷怪随机），只看 max 会把它们全算进来（run2 实测 206 条）。

账本格式（`seq \t kind \t a \t b \t site`，`MIR2_RNG_SITE=1` 才写 5 列）：

    python -I -B tools/window/butch_ledger.py <rng-stream.tsv> --side csharp|rust [--json OUT]
    python -I -B tools/window/butch_ledger.py --selftest

退出码：0 = 两抽成立；1 = 不成立（stdout 打印原因）；2 = 用法/文件错误。
"""
import io
import json
import os
import sys

SITE_MARK = {
    # C# 账本 site = `Class@File.cs:LINE`；Rust = `crates\...\file.rs:LINE`
    "csharp": ("ButchItem", "PlayObject.Operate.cs"),
    "rust": ("world.rs",),
}
EXPECT_LINE = {"csharp": {"16": 722, "201": 723}, "rust": {"16": 7756, "201": 7757}}


def parse(path):
    rows, bad = [], 0
    with io.open(path, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.rstrip("\r\n")
            if not line or line.startswith("SEED_PROBE"):
                continue
            parts = line.split("\t")
            if len(parts) < 5:            # INJECT_OK 等短行
                continue
            try:
                seq, a, b = int(parts[0]), int(parts[2]), int(parts[3])
            except ValueError:
                bad += 1
                continue
            rows.append({"seq": seq, "kind": parts[1], "a": a, "b": b, "site": parts[4]})
    return rows, bad


def analyse(path, side):
    rows, bad = parse(path)
    marks = SITE_MARK[side]
    out = {"ledger": path, "side": side, "rows": len(rows), "unparsable": bad, "draws": {}}
    for want in ("16", "201"):
        n = int(want)
        alln = [r for r in rows if r["kind"] == "Next(max)" and r["a"] == n]
        in_fn = [r for r in alln if any(m in r["site"] for m in marks)]
        out["draws"][want] = {
            "in_butch_fn": len(in_fn),
            "total": len(alln),
            "other_sites": sorted({r["site"] for r in alln if r not in in_fn})[:5],
            "hits": [{"seq": r["seq"], "site": r["site"], "value": r["b"]} for r in in_fn[:5]],
            "expect_line": EXPECT_LINE[side].get(want),
        }
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
        if out["values"]["n10"] < 5 or out["values"]["n10"] > 20:
            problems.append("n10=%d 越出 [5,20]" % out["values"]["n10"])
        if out["values"]["meat"] < 100 or out["values"]["meat"] > 300:
            problems.append("meat=%d 越出 [100,300]" % out["values"]["meat"])
    out["problems"] = problems
    out["verdict"] = "TWO_DRAWS_OK" if not problems else "TWO_DRAWS_FAIL"
    return out


# ---------------------------------------------------------------- selftest
_LEDGER_HEAD = "1\tINJECT_OK\t42\t0\nSEED_PROBE 1 2 3\n"
_MONGEN = "4054\tNext(max)\t201\t35\tGameSrv.Word.WorldServer.RegenMonsters@WorldServer.MonGen.cs:815\n"


def _case(rows, side="csharp"):
    import tempfile
    fd, path = tempfile.mkstemp(suffix=".tsv")
    try:
        with io.open(fd, "w", encoding="utf-8", newline="") as f:
            f.write(_LEDGER_HEAD + "".join(rows))
        return analyse(path, side)
    finally:
        os.remove(path)


def _selftest():
    checks = []

    def ok(name, cond, detail=""):
        checks.append((bool(cond), name, detail))

    good = _case([_MONGEN,
                  "5000\tNext(max)\t16\t7\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:722\n",
                  "5001\tNext(max)\t201\t123\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:723\n"])
    ok("a 两抽成立（含 206 条 MonGen 干扰中的 1 条）", good["verdict"] == "TWO_DRAWS_OK"
       and good["values"] == {"n10": 12, "meat": 223} and good["order_ok"], good.get("problems"))
    bad_order = _case(["5000\tNext(max)\t201\t123\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:723\n",
                       "5001\tNext(max)\t16\t7\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:722\n"])
    ok("b 次序反了 ⇒ FAIL", bad_order["verdict"] == "TWO_DRAWS_FAIL"
       and any("次序" in p for p in bad_order["problems"]), bad_order.get("problems"))
    twice = _case(["5000\tNext(max)\t16\t7\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:722\n",
                   "5001\tNext(max)\t16\t9\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:722\n",
                   "5002\tNext(max)\t201\t123\tGameSrv.Word.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:723\n"])
    ok("c max=16 出现 2 条 ⇒ FAIL", twice["verdict"] == "TWO_DRAWS_FAIL",
       twice.get("problems"))
    only_mongen = _case([_MONGEN])
    ok("d 只有 MonGen 的 max=201 ⇒ FAIL（不许把刷怪算成屠宰）",
       only_mongen["verdict"] == "TWO_DRAWS_FAIL"
       and only_mongen["draws"]["201"]["total"] == 1
       and only_mongen["draws"]["201"]["in_butch_fn"] == 0, only_mongen.get("problems"))
    # 站点串用真实形态的反斜杠（C# 账本是 `Class@File`，Rust 是 `crates\...`）；
    # 这里用 chr(92) 拼，避免源码里的转义在复制/编辑过程中丢层（曾触发 SyntaxWarning）。
    _bs = chr(92)
    _rust_site = lambda tail: _bs.join(["crates", "world", "src", tail])
    rust_ok = _case(["6442\tNext(max)\t201\t193\t" + _rust_site("mongen.rs:899") + "\n",
                     "7000\tNext(max)\t16\t3\t" + _rust_site("world.rs:7756") + "\n",
                     "7001\tNext(max)\t201\t55\t" + _rust_site("world.rs:7757") + "\n"],
                    side="rust")
    ok("e rust 侧同口径（world.rs 相邻两行）", rust_ok["verdict"] == "TWO_DRAWS_OK",
       rust_ok.get("problems") or rust_ok.get("sites"))
    bad = [c for c in checks if not c[0]]
    for cond, name, detail in checks:
        print("%s %-46s %s" % ("PASS" if cond else "FAIL", name, detail if not cond else ""))
    total = len(checks)
    print(("%s: butch_ledger selftest %d/%d" % ("RED" if bad else "GREEN", total - len(bad), total)))
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
    if side not in SITE_MARK:
        print("--side 只接受 csharp|rust：%s" % side)
        return 2
    if not os.path.exists(path):
        print("账本不存在: %s" % path)
        return 2
    out = analyse(path, side)
    if "--json" in argv:
        with io.open(argv[argv.index("--json") + 1], "w", encoding="utf-8", newline="") as f:
            json.dump(out, f, ensure_ascii=False, indent=1)
    print(json.dumps(out, ensure_ascii=False, indent=1))
    return 0 if out["verdict"] == "TWO_DRAWS_OK" else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
