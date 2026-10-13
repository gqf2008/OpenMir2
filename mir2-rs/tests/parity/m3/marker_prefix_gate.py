#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""marker_prefix_gate.py — M3 夹具 markers 前缀口径 fail-closed 门。

卡：`openmir2-m3-fixture-marker-prefix-bug`。

## 为什么有这道门

真机窗口首轮实跑（2026-10-13，证据 `evidence-m3-npc-shop-align-window-20261013/`）抓到：
`run_pair.ps1` 把 `M3*SEQ {...}` 事件**去掉前缀**再写 `*_markers.jsonl`，而 4 个比较器的
`_marker_events` **只认带前缀**的行 ⇒ `*_markers.jsonl` 里是裸 JSON ⇒ 收集 0 条 ⇒
`assemble` 恒 `PREREQ_RED`（rc=2），⑤商店/⑥任务等**已实现**系统的真机判决被夹具阻断。

口径（owner 裁决，写侧统一为带前缀）：markers 文件 = bot.log 内**带前缀**的原始事件行
（场景 json 亦如此声明）。修法 = `run_pair.ps1` 保留前缀 + 写后 fail-closed 校验。

## 判据（任一不满足 ⇒ rc=1）

1. `run_pair.ps1` 的**代码行**里不得再出现去前缀截断
   `.Substring('<M3*SEQ> '.Length)`（注释行跳过；同族新块如 ⑧/⑨ 也会被咬住）；
2. `run_pair.ps1` 必须为 4 个序列都接上生产 fail-closed 牙
   `Assert-M3MarkerPrefix -Path ... -Marker '<M3*SEQ>'`（缺一个 ⇒ RED）；
3. 4 个比较器的 `_marker_events` 必须仍按前缀扫描
   （`line.find("<M3*SEQ> ")`；缺一个 ⇒ RED）；
4. 运行时牙：同一份合法事件流**带前缀** ⇒ `assemble` rc=0；**去前缀（裸 JSON）** ⇒ rc=2；
   空文件 ⇒ rc=2（证明「前缀」是 assemble 成立的必要条件，不是可选项）。

## 用法

    python -I marker_prefix_gate.py check [--json out]
    python -I marker_prefix_gate.py selftest

退出码：0 = GREEN；1 = RED。
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
RUN_PAIR = os.path.join(HERE, "run_pair.ps1")

# 去前缀截断（历史 bug 的形态）。只扫代码行，不扫注释。
STRIP_RE = re.compile(r"\.Substring\(\s*'M3[A-Z]*SEQ '\s*\.Length\s*\)")
# 生产 fail-closed 牙：Assert-M3MarkerPrefix -Path $xxMarkersPath -Marker 'M3XXXSEQ'
GUARD_RE = re.compile(r"Assert-M3MarkerPrefix\s+-Path\s+\$(\w+)\s+-Marker\s+'(M3[A-Z]*SEQ)'")

# (marker 前缀, 比较器文件)。⑧/⑨ 的 `guild_castle_diff.py` / `hero_pet_diff.py` 未合入时
# 不在表内；一旦合入，把对应行加进来即可（同族口径）。
COMPARATORS = [
    ("M3SHOPSEQ", "npc_shop_diff.py"),
    ("M3QUESTSEQ", "quest_diff.py"),
    ("M3PAIRTRADESEQ", "party_trade_diff.py"),
    ("M3SELLOFFSEQ", "selloff_diff.py"),
]

# 运行时牙用的事件流：取自 `npc_shop_diff.py` selftest 的合法 2-op 流（与基准记录同形）。
SHOP_EVENTS = [
    {"phase": "arm", "ops": ["sell", "buy"]},
    {"phase": "send", "k": 0, "op": "sell", "npc": "卫家店", "npc_actor": 101,
     "req": {"item": "金创药(小量)", "n": 1}, "gold_before": 1000,
     "bag_before": ["金创药(小量)#42", "木剑#43"]},
    {"phase": "frame", "k": 0, "frame": {"sm": 648, "a": 1010, "b": 0, "c": 0, "d": 0}},
    {"phase": "end", "k": 0, "gold_after": 1010, "bag_after": ["木剑#43"],
     "goods_after": ["金创药(小量)#42"], "price": 10, "branch": "ok"},
    {"phase": "send", "k": 1, "op": "buy", "npc": "卫家店", "npc_actor": 101,
     "req": {"item": "金创药(小量)", "n": 1}, "gold_before": 0,
     "bag_before": ["木剑#43"], "goods_before": ["金创药(小量)#42"]},
    {"phase": "frame", "k": 1, "frame": {"sm": 651, "a": 3, "b": 0, "c": 0, "d": 0}},
    {"phase": "end", "k": 1, "gold_after": 0, "bag_after": ["木剑#43"],
     "goods_after": ["金创药(小量)#42"], "branch": "fail_nomoney"},
    {"phase": "done", "ops": 2},
]


def _code_lines(text):
    """返回 (行号, 原文) —— 跳过纯注释行（防止注释里的字面量造成假阳性）。"""
    out = []
    for i, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("#"):
            continue
        out.append((i, line))
    return out


def scan_strips(path):
    """scan 去前缀截断；返回 [(行号, 行文)]。"""
    with open(path, "r", encoding="utf-8") as fh:
        text = fh.read()
    return [(i, ln.strip()) for i, ln in _code_lines(text) if STRIP_RE.search(ln)]


def scan_guards(path):
    """scan 生产 fail-closed 牙；返回 [(marker, 变量名)]。"""
    with open(path, "r", encoding="utf-8") as fh:
        text = fh.read()
    return [(m.group(2), m.group(1)) for m in GUARD_RE.finditer(text)]


def comparator_requires_prefix(path, marker):
    with open(path, "r", encoding="utf-8") as fh:
        text = fh.read()
    return ('line.find("%s ")' % marker) in text


def _assemble_rc(comparator, markers_path):
    p = subprocess.run(
        [sys.executable, "-I", comparator, "assemble", markers_path, "--side", "csharp"],
        capture_output=True, text=True)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def runtime_teeth(tmpdir):
    """同一合法事件流：带前缀 ⇒ 0；裸 JSON ⇒ 2；空文件 ⇒ 2。"""
    cmp_path = os.path.join(HERE, "npc_shop_diff.py")
    bare = os.path.join(tmpdir, "shop_bare.jsonl")
    pref = os.path.join(tmpdir, "shop_prefixed.jsonl")
    empty = os.path.join(tmpdir, "shop_empty.jsonl")
    with open(bare, "w", encoding="utf-8", newline="\n") as fh:
        for ev in SHOP_EVENTS:
            fh.write(json.dumps(ev, ensure_ascii=False) + "\n")
    with open(pref, "w", encoding="utf-8", newline="\n") as fh:
        for ev in SHOP_EVENTS:
            fh.write("M3SHOPSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
    open(empty, "w", encoding="utf-8", newline="\n").close()
    return (_assemble_rc(cmp_path, bare), _assemble_rc(cmp_path, pref),
            _assemble_rc(cmp_path, empty))


def _evaluate(run_pair= RUN_PAIR):
    findings = []
    strips = scan_strips(run_pair)
    for ln, text in strips:
        findings.append("run_pair.ps1:%d 仍有去前缀截断: %s" % (ln, text))

    guards = scan_guards(run_pair)
    guard_markers = set(m for m, _ in guards)
    for marker, _cmp in COMPARATORS:
        if marker not in guard_markers:
            findings.append("run_pair.ps1 缺生产 fail-closed 牙 Assert-M3MarkerPrefix -Marker '%s'" % marker)

    for marker, cmp in COMPARATORS:
        cpath = os.path.join(HERE, cmp)
        if not os.path.isfile(cpath):
            findings.append("缺比较器: %s" % cmp)
            continue
        if not comparator_requires_prefix(cpath, marker):
            findings.append("%s 不再按前缀扫描（缺 line.find(\"%s \")）" % (cmp, marker))

    with tempfile.TemporaryDirectory(prefix="marker-prefix-teeth-") as tmp:
        (bare_rc, bare_out), (pref_rc, pref_out), (empty_rc, _) = runtime_teeth(tmp)
    if bare_rc != 2:
        findings.append("裸 JSON markers 应 PREREQ(2)，实得 rc=%d: %s" % (bare_rc, bare_out.strip()))
    if pref_rc != 0:
        findings.append("带前缀 markers 应 assemble OK(0)，实得 rc=%d: %s" % (pref_rc, pref_out.strip()))
    if empty_rc != 2:
        findings.append("空 markers 应 PREREQ(2)，实得 rc=%d" % empty_rc)

    return findings, {"strips": strips, "guards": guards,
                      "bare_rc": bare_rc, "prefixed_rc": pref_rc, "empty_rc": empty_rc}


# ---- 改坏必红（真实变异 run_pair.ps1 的临时副本） --------------------------------

SHOP_OK = b"    $shopEvents = @($shopSeqMarkers)"
SHOP_STRIP = (b"    $shopEvents = @($shopSeqMarkers | ForEach-Object "
              b"{ $_.Substring('M3SHOPSEQ '.Length).Trim() })")
SHOP_GUARD = b"    Assert-M3MarkerPrefix -Path $shopMarkersPath -Marker 'M3SHOPSEQ'"


def _mutations(dest_dir):
    """在 dest_dir 生成两个真实变异体，返回 [(name, path, findings)]。

    - `run_pair_strip.ps1`：把 shop 块改回历史「去前缀截断」写法；
    - `run_pair_noguard.ps1`：删掉 shop 块的 fail-closed 牙（模拟未来新块漏接）。
    """
    with open(RUN_PAIR, "rb") as fh:
        src = fh.read()
    eol = b"\r\n" if b"\r\n" in src else b"\n"
    ok = SHOP_OK + eol
    strip = SHOP_STRIP + eol
    guard = SHOP_GUARD + eol
    if src.count(ok) != 1:
        raise AssertionError("SHOP_OK 在 run_pair.ps1 里不是唯一匹配（%d）" % src.count(ok))
    if src.count(guard) != 1:
        raise AssertionError("SHOP_GUARD 在 run_pair.ps1 里不是唯一匹配（%d）" % src.count(guard))
    variants = {
        "run_pair_strip.ps1": src.replace(ok, strip),
        "run_pair_noguard.ps1": src.replace(guard, b""),
    }
    out = []
    for name, data in variants.items():
        p = os.path.join(dest_dir, name)
        with open(p, "wb") as fh:
            fh.write(data)
        findings, _info = _evaluate(run_pair=p)
        out.append((name, p, findings))
    return out


# PowerShell 侧牙：从 run_pair.ps1 里抽出 Assert-M3MarkerPrefix 真身，分别喂合法/非法 markers。
PS_ASSERT_SCRIPT = r"""param([Parameter(Mandatory=$true)][string]$Mode)
$ErrorActionPreference = 'Stop'
$src = Get-Content -Raw '%(run_pair)s'
$m = [regex]::Match($src, 'function Assert-M3MarkerPrefix \{[\s\S]*?\r?\n\}')
if (-not $m.Success) { Write-Output 'RED: Assert-M3MarkerPrefix 未在 run_pair.ps1 找到'; exit 3 }
Invoke-Expression $m.Value
$good = Join-Path '%(out)s' 'helper-good.jsonl'
$bad = Join-Path '%(out)s' 'helper-bad.jsonl'
Set-Content -Path $good -Value @('M3SHOPSEQ {"phase":"arm"}', 'M3SHOPSEQ {"phase":"done","ops":0}') -Encoding utf8
Set-Content -Path $bad -Value @('{"phase":"arm"}', 'M3SHOPSEQ {"phase":"done","ops":0}') -Encoding utf8
if ($Mode -eq 'good') {
  Assert-M3MarkerPrefix -Path $good -Marker 'M3SHOPSEQ'
  Write-Output 'GREEN: 带前缀 markers 通过 Assert-M3MarkerPrefix'
} else {
  Assert-M3MarkerPrefix -Path $bad -Marker 'M3SHOPSEQ'
  Write-Output 'BUG: 去前缀 markers 本应被拒绝'
  exit 9
}
"""


def ps_assert_teeth(outdir):
    """跑 PowerShell 真身牙：good ⇒ rc0；bad ⇒ rc1 且点名缺前缀。返回 (script, good, bad)。"""
    script = os.path.join(outdir, "ps-assert-teeth.ps1")
    with open(script, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(PS_ASSERT_SCRIPT % {
            "run_pair": RUN_PAIR.replace("'", "''"),
            "out": outdir.replace("'", "''"),
        })
    def run(mode):
        return subprocess.run(
            ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script, "-Mode", mode],
            capture_output=True, text=True)
    return script, run("good"), run("bad")


def cmd_mutation(args):
    out = args.out or tempfile.mkdtemp(prefix="marker-prefix-mut-")
    os.makedirs(out, exist_ok=True)
    ok = True
    for name, path, findings in _mutations(out):
        red = bool(findings)
        if name.endswith("strip.ps1"):
            want = any("去前缀截断" in f for f in findings)
        else:
            want = any("缺生产 fail-closed 牙" in f and "M3SHOPSEQ" in f for f in findings)
        sys.stdout.write("%s: %s rc=%d (%s)\n" % (name, "RED" if red else "GREEN", 1 if red else 0, path))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        ok = ok and red and want

    script, good, bad = ps_assert_teeth(out)
    good_out = ((good.stdout or "") + (good.stderr or "")).strip()
    bad_out = ((bad.stdout or "") + (bad.stderr or "")).strip()
    sys.stdout.write("ps-assert good: rc=%d %s\n" % (good.returncode, good_out))
    sys.stdout.write("ps-assert bad: rc=%d %s\n" % (bad.returncode, bad_out))
    ps_ok = good.returncode == 0 and bad.returncode == 1 and "缺前缀" in bad_out
    ok = ok and ps_ok
    sys.stdout.write("MUTATION %s (%s)\n" % ("GREEN" if ok else "RED", script))
    return 0 if ok else 1


def cmd_check(args):
    findings, info = _evaluate()
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"findings": findings, "info": info}, fh, ensure_ascii=False, indent=2)
    if findings:
        sys.stdout.write("RED: markers 前缀口径门失败（%d 项）\n" % len(findings))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        return 1
    sys.stdout.write(
        "GREEN: markers 前缀口径一致（run_pair 无去前缀截断 + 4 生产牙 + 4 比较器按前缀扫描；"
        "teeth bare=%d prefixed=%d empty=%d）\n"
        % (info["bare_rc"], info["prefixed_rc"], info["empty_rc"]))
    return 0


def _scan_synthetic(source):
    return [ln for _, ln in _code_lines(source) if STRIP_RE.search(ln)]


def cmd_selftest(args):
    results = []

    def check(name, ok):
        results.append((name, bool(ok)))

    good = "    $shopEvents = @($shopSeqMarkers)\n"
    bad = ("    $shopEvents = @($shopSeqMarkers | ForEach-Object "
           "{ $_.Substring('M3SHOPSEQ '.Length).Trim() })\n")
    comment = "    # $_.Substring('M3SHOPSEQ '.Length)\n"
    check("scan-good-clean", not _scan_synthetic(good))
    check("scan-catches-strip", len(_scan_synthetic(bad)) == 1)
    check("scan-ignores-comment", not _scan_synthetic(comment))

    findings, info = _evaluate()
    check("real-run-pair-no-strip", not info["strips"])
    check("real-guards-wired", set(m for m, _ in info["guards"]) == set(m for m, _ in COMPARATORS))
    for marker, cmp in COMPARATORS:
        check("comparator-requires-prefix-%s" % marker,
              comparator_requires_prefix(os.path.join(HERE, cmp), marker))
    check("teeth-bare-prereq", info["bare_rc"] == 2)
    check("teeth-prefixed-ok", info["prefixed_rc"] == 0)
    check("teeth-empty-prereq", info["empty_rc"] == 2)
    check("check-green", not findings)

    with tempfile.TemporaryDirectory(prefix="marker-prefix-mutselftest-") as tmp:
        muts = _mutations(tmp)
    by_name = dict((n, f) for n, _p, f in muts)
    check("mutation-strip-red", any("去前缀截断" in f for f in by_name.get("run_pair_strip.ps1", [])))
    check("mutation-guard-red", any("缺生产 fail-closed 牙" in f and "M3SHOPSEQ" in f
                                     for f in by_name.get("run_pair_noguard.ps1", [])))

    ok = sum(1 for _n, v in results if v)
    total = len(results)
    for name, v in results:
        sys.stdout.write("SELFTEST %s %s\n" % ("ok" if v else "FAIL", name))
    sys.stdout.write("SELFTEST ok=%d/%d\n" % (ok, total))
    return 0 if ok == total else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description="M3 夹具 markers 前缀口径 fail-closed 门")
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("check", help="扫 run_pair/比较器并跑运行时牙")
    c.add_argument("--json", default="")
    c.set_defaults(func=cmd_check)
    s = sub.add_parser("selftest")
    s.set_defaults(func=cmd_selftest)
    m = sub.add_parser("mutation", help="改坏必红：真实变异 run_pair.ps1 副本 + PowerShell 真身牙")
    m.add_argument("--out", default="", help="变异体与辅助脚本的落点目录（默认临时目录）")
    m.set_defaults(func=cmd_mutation)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
