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
2. `run_pair.ps1` 必须为 `COMPARATORS` 里每个序列都接上生产 fail-closed 牙
   `Assert-M3MarkerPrefix -Path ... -Marker '<M3*SEQ>'`（缺一个 ⇒ RED）；
   **牙只按代码行计**：被注释掉的牙 = 缺牙 ⇒ RED —— 整行 `#` 注释（O-1 收口，2026-10-14）
   与 Windows PowerShell 块注释 `<# ... #>`（**可嵌套**；O-1' 收口，2026-10-14）都算注释；
   且 `run_pair.ps1` 里出现的**每个**牙 Marker 都必须登记在 `COMPARATORS`
   **或** `GUARD_ONLY_MARKERS`（新夹具族漏登记 ⇒ `check`/`selftest` 双 RED，而不是只靠自检）；
   `GUARD_ONLY_MARKERS` 是「有 markers 流牙、但比较器消费 BotSrv 直写原始件」的显式例外清单；
3. `COMPARATORS` 里每个比较器的 `_marker_events` 必须仍按前缀扫描
   （`line.find("<M3*SEQ> ")`；缺一个 ⇒ RED）；
4. 运行时牙：同一份合法事件流**带前缀** ⇒ `assemble` rc=0；**去前缀（裸 JSON）** ⇒ rc=2；
   空文件 ⇒ rc=2（证明「前缀」是 assemble 成立的必要条件，不是可选项）。
5. **defect_2（同前缀非 JSON 诊断行）**：BotSrv 写侧已迁的 `SKIP_FAMILIES` 里不得再出现
   `MainOutMessage("<M3*SEQ> ... skip=...")` 形态的**裸前缀**诊断（一律改 `<M3*SEQ>_SKIP`）；
   并带运行时牙：带 `_SKIP` 前缀的诊断行不干扰 `assemble`（rc=0），裸前缀诊断行仍作事件被拒（rc=2）。
   约定：**新增夹具族时，把族名加进 `SKIP_FAMILIES`，并把该族 BotSrv 的 `skip=` 诊断改成 `_SKIP`**。

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
# BotSrv 假人侧的 markers 写侧源文件（诊断行不得混入事件前缀，见 SKIP_FAMILIES）。
BOTSRV_ROBOT_PLAYER = os.path.normpath(
    os.path.join(HERE, "..", "..", "..", "..", "src", "BotSrv", "Player", "RobotPlayer.cs"))

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
    # ⑧/⑨ 夹具已合入（guild_castle / hero_pet），按同族口径纳入本门。
    ("M3GUILDCSEQ", "guild_castle_diff.py"),
    ("M3HEROPETSEQ", "hero_pet_diff.py"),
]

# 有 markers 流牙、但比较器**不**按 `line.find("<M3*SEQ> ")` 扫前缀的显式例外。
# 这类族的比较器消费 BotSrv「直写原始件」（如 ④ 的 `drop_exp_raw.jsonl`），markers 流只用于
# 断言「序列确实跑过」。例外必须逐条登记：`run_pair.ps1` 里新出现一个未登记的牙 ⇒ 双 RED
# （`check` 与 `selftest`），防止未来新增夹具族时静默漏登记。
GUARD_ONLY_MARKERS = [
    # M3④ 掉落/经验：run_pair.ps1 的 `-DropExpSequence` 块断言 `M3DROPEXPSEQ` 前缀，
    # 但 `drop-exp/drop_exp_diff.py` 比的是 BotSrv `-DropExpOut` 直写的 `drop_exp_raw.jsonl`。
    "M3DROPEXPSEQ",
]

# 写侧必须已把「同前缀非 JSON 诊断行」（defect_2）改成 `M3*SEQ_SKIP` 独立前缀的序列族。
# 约定：**新增夹具族时，把族名加进这里，并把该族 BotSrv 的 `skip=` 诊断行改成 `M3*SEQ_SKIP`**。
SKIP_FAMILIES = [
    "M3BAGSEQ", "M3SHOPSEQ", "M3QUESTSEQ",
    "M3PAIRTRADESEQ", "M3SELLOFFSEQ", "M3GUILDCSEQ", "M3HEROPETSEQ",
]
# 事件前缀后紧跟非 JSON 诊断（`M3XXXSEQ ...skip=`）的写侧形态；`M3XXXSEQ_SKIP` 不匹配（下划线）。
BOTSRV_BARE_SKIP_RE = re.compile(r'MainOutMessage\(\s*"(M3[A-Z]*SEQ) [^"{\n]')

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
    """返回 (行号, 去注释后的代码行) —— 用于 fail-closed 扫描。

    注释判定（两族，均按**代码行**计）：
      1. 整行 `#` 注释（`lstrip()` 后以 `#` 开头；`#>` 除外，它是块注释闭合符）；
      2. Windows PowerShell 块注释 `<# ... #>`，**支持嵌套**（`<#` / `#>` 均可能出现在行中段，
         跨多行）。块注释内容一律抹成空格、保留换行 ⇒ 藏在块注释里的牙 = 缺牙（O-1'，
         reviewer-5 `d853bd1a` 在 chain-gate 片登记的 Observation：`run_pair.ps1:2-34` 确有
         头部 `<# ... #>` 块注释，旧实现只跳整行 `#`，块注释里的同名调用会被当真牙 ⇒ fail-open）。

    fail-closed 取向：无法判定是否注释时按「注释」处理（宁可把真牙判成缺牙 ⇒ RED，
    也不把注释当代码 ⇒ 假绿）。本函数非完整词法分析：**行内 `#` 注释**（代码后跟 `#`）不在
    处理范围——当前 `run_pair.ps1` 的 7 处生产牙均为整行调用，无活洞；新增写侧一律整行成牙。
    """
    out = []
    depth = 0  # 块注释嵌套深度
    for i, raw in enumerate(text.splitlines(), 1):
        if depth == 0:
            s = raw.lstrip()
            if s.startswith("#") and not s.startswith("#>"):
                out.append((i, ""))
                continue
        res = []
        j = 0
        while j < len(raw):
            if raw.startswith("<#", j):
                depth += 1
                res.append("  ")
                j += 2
                continue
            if depth > 0 and raw.startswith("#>", j):
                depth -= 1
                res.append("  ")
                j += 2
                continue
            res.append(" " if depth > 0 else raw[j])
            j += 1
        out.append((i, "".join(res)))
    return out


def scan_strips(path):
    """scan 去前缀截断；返回 [(行号, 行文)]。"""
    with open(path, "r", encoding="utf-8") as fh:
        text = fh.read()
    return [(i, ln.strip()) for i, ln in _code_lines(text) if STRIP_RE.search(ln)]


def scan_guards(path):
    """scan 生产 fail-closed 牙；返回 [(marker, 变量名)]（按代码行顺序，逐行匹配）。

    O-1 收口（reviewer-6 `49993d1f` 的非阻塞 Observation）：旧写法对**整份文件文本**
    `GUARD_RE.finditer(text)`，于是**注释行**里的 `Assert-M3MarkerPrefix ... -Marker 'M3*SEQ'`
    也被当成真实牙 ⇒ 把某族的牙整行注释掉，门仍 GREEN（fail-open，假阴性）。
    改为逐 `_code_lines(text)` 匹配：整行 `#` 注释与 Windows PowerShell 块注释
    `<# ... #>`（可嵌套；O-1' 收口）里的牙 = 缺牙 ⇒ RED。
    """
    with open(path, "r", encoding="utf-8") as fh:
        text = fh.read()
    out = []
    for _i, line in _code_lines(text):
        for m in GUARD_RE.finditer(line):
            out.append((m.group(2), m.group(1)))
    return out


def _scan_botsrv_text(text):
    """从 BotSrv 源文本里找已迁族中残留的裸前缀 `skip=` 诊断；返回 [(行号, marker, 行文)]。"""
    out = []
    for i, line in _code_lines(text):
        m = BOTSRV_BARE_SKIP_RE.search(line)
        if m and m.group(1) in SKIP_FAMILIES and "skip=" in line:
            out.append((i, m.group(1), line.strip()))
    return out


def scan_botsrv_bare_skips(path=BOTSRV_ROBOT_PLAYER):
    """scan BotSrv 写侧：已迁族是否还有 `M3*SEQ ... skip=` 的诊断行；返回 [(行号, marker, 行文)]。"""
    if not os.path.isfile(path):
        return []
    with open(path, "r", encoding="utf-8") as fh:
        return _scan_botsrv_text(fh.read())


def guards_wired(guard_markers):
    """生产牙集合必须恰 = `COMPARATORS` ∪ `GUARD_ONLY_MARKERS`（多一个未登记族 ⇒ RED）。"""
    expected = set(m for m, _ in COMPARATORS) | set(GUARD_ONLY_MARKERS)
    return set(guard_markers) == expected


def _drop_one_guard(guard_markers):
    """去掉一个真实牙，返回**去重排序后**的其余标记（空表 ⇒ []）。

    O-2 收口（reviewer-6 `4f5d3eeb1` / `a9f045dac3`）：旧写法 `guard_markers[1:]` 只切掉
    **首个出现**；若同一 marker 在 `run_pair.ps1` 里出现两次（同一族的牙写在两处是合法的），
    `set(markers[1:])` 仍等于全集 ⇒ 「少一个」向的牙会**假红**，把合法改动打红。
    本函数先 `set` 去重再排序，切一刀后必为**真子集**（恒少一个标记），
    不受扫描顺序或重复出现影响。
    """
    return sorted(set(guard_markers))[1:]


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


def runtime_skip_teeth(tmpdir):
    """诊断行前缀的运行时牙（同一合法事件流，两种诊断写法）：

    - `M3SHOPSEQ_SKIP ...`（新写法）⇒ 不被 `_marker_events` 当事件 ⇒ assemble **rc=0**；
    - `M3SHOPSEQ ...skip=...`（旧写法/裸前缀）⇒ 被当事件 ⇒ 非法 JSON ⇒ **rc=2**（fail-closed 保持）。
    """
    cmp_path = os.path.join(HERE, "npc_shop_diff.py")
    pref = os.path.join(tmpdir, "shop_skip_prefixed.jsonl")
    bare = os.path.join(tmpdir, "shop_skip_bare.jsonl")
    with open(pref, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("M3SHOPSEQ_SKIP op=sell skip=no_npc_in_view\n")
        for ev in SHOP_EVENTS:
            fh.write("M3SHOPSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
    with open(bare, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("M3SHOPSEQ op=sell skip=no_npc_in_view\n")
        for ev in SHOP_EVENTS:
            fh.write("M3SHOPSEQ " + json.dumps(ev, ensure_ascii=False) + "\n")
    return _assemble_rc(cmp_path, pref), _assemble_rc(cmp_path, bare)


def _evaluate(run_pair=RUN_PAIR, botsrv=BOTSRV_ROBOT_PLAYER):
    findings = []
    strips = scan_strips(run_pair)
    for ln, text in strips:
        findings.append("run_pair.ps1:%d 仍有去前缀截断: %s" % (ln, text))

    bare_skips = scan_botsrv_bare_skips(botsrv)
    for ln, marker, text in bare_skips:
        findings.append(
            "%s:%d 仍有裸前缀 skip= 诊断行（应改 `%s_SKIP`）: %s"
            % (os.path.basename(botsrv), ln, marker, text))

    guards = scan_guards(run_pair)
    guard_markers = set(m for m, _ in guards)
    registered = set(m for m, _ in COMPARATORS) | set(GUARD_ONLY_MARKERS)
    for marker, var in guards:
        if marker not in registered:
            findings.append(
                "run_pair.ps1 生产牙 Marker '%s'（$%s）未登记：新增夹具族必须加进 "
                "COMPARATORS 或 GUARD_ONLY_MARKERS" % (marker, var))
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
        (skip_pref_rc, _), (skip_bare_rc, skip_bare_out) = runtime_skip_teeth(tmp)
    if skip_pref_rc != 0:
        findings.append("带 `_SKIP` 前缀的诊断行不应干扰 assemble，实得 rc=%d" % skip_pref_rc)
    if skip_bare_rc != 2:
        findings.append("裸前缀 `skip=` 诊断行应 PREREQ(2)，实得 rc=%d: %s"
                        % (skip_bare_rc, skip_bare_out.strip()))
    if bare_rc != 2:
        findings.append("裸 JSON markers 应 PREREQ(2)，实得 rc=%d: %s" % (bare_rc, bare_out.strip()))
    if pref_rc != 0:
        findings.append("带前缀 markers 应 assemble OK(0)，实得 rc=%d: %s" % (pref_rc, pref_out.strip()))
    if empty_rc != 2:
        findings.append("空 markers 应 PREREQ(2)，实得 rc=%d" % empty_rc)

    return findings, {"strips": strips, "guards": guards, "bare_skips": bare_skips,
                      "bare_rc": bare_rc, "prefixed_rc": pref_rc, "empty_rc": empty_rc,
                      "skip_prefixed_rc": skip_pref_rc, "skip_bare_rc": skip_bare_rc}


# ---- 改坏必红（真实变异 run_pair.ps1 的临时副本） --------------------------------

SHOP_OK = b"    $shopEvents = @($shopSeqMarkers)"
SHOP_STRIP = (b"    $shopEvents = @($shopSeqMarkers | ForEach-Object "
              b"{ $_.Substring('M3SHOPSEQ '.Length).Trim() })")
SHOP_GUARD = b"    Assert-M3MarkerPrefix -Path $shopMarkersPath -Marker 'M3SHOPSEQ'"

# BotSrv 写侧变异：把一条已迁诊断改回裸事件前缀（defect_2 回归）。
BOTSRV_SKIP_OK = b'MainOutMessage("M3SHOPSEQ_SKIP op="'
BOTSRV_SKIP_BARE = b'MainOutMessage("M3SHOPSEQ op="'


def _mutations(dest_dir):
    """在 dest_dir 生成真实变异体，返回 [(name, path, findings)]。

    - `run_pair_strip.ps1`：把 shop 块改回历史「去前缀截断」写法；
    - `run_pair_noguard.ps1`：删掉 shop 块的 fail-closed 牙（模拟未来新块漏接）；
    - `run_pair_guard_commented.ps1`：把 shop 块的 fail-closed 牙**整行注释掉**（O-1 根因复现：
      注释掉的牙必须等价于缺牙 ⇒ RED，而旧 `scan_guards` 扫全文会把它当真牙、保持 GREEN）；
    - `run_pair_guard_blockcommented.ps1`：把 shop 块的 fail-closed 牙裹进 Windows PowerShell
      **块注释** `<# ... #>`（O-1' 根因复现：块注释里的牙同样必须等价于缺牙 ⇒ RED；
      只跳整行 `#` 的旧 `_code_lines` 会把它当真牙、保持 GREEN）；
    - `RobotPlayer_bareskip.cs`：把一条已迁诊断改回裸事件前缀（模拟 defect_2 回归）。
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
    with open(BOTSRV_ROBOT_PLAYER, "rb") as fh:
        botsrc = fh.read()
    if botsrc.count(BOTSRV_SKIP_OK) != 1:
        raise AssertionError("BOTSRV_SKIP_OK 在 RobotPlayer.cs 里不是唯一匹配（%d）"
                             % botsrc.count(BOTSRV_SKIP_OK))
    variants = {
        "run_pair_strip.ps1": (src.replace(ok, strip), "ps"),
        "run_pair_noguard.ps1": (src.replace(guard, b""), "ps"),
        "run_pair_guard_commented.ps1": (src.replace(guard, b"#" + guard), "ps"),
        "run_pair_guard_blockcommented.ps1": (src.replace(guard, b"<#" + eol + guard + eol + b"#>"), "ps"),
        "RobotPlayer_bareskip.cs": (botsrc.replace(BOTSRV_SKIP_OK, BOTSRV_SKIP_BARE), "cs"),
    }
    out = []
    for name, (data, kind) in variants.items():
        p = os.path.join(dest_dir, name)
        with open(p, "wb") as fh:
            fh.write(data)
        findings, _info = _evaluate(run_pair=p) if kind == "ps" else _evaluate(botsrv=p)
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
        elif name.endswith(".cs"):
            want = any("裸前缀 skip=" in f for f in findings)
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
        "GREEN: markers 前缀口径一致（run_pair 无去前缀截断 + %d 生产牙（含 %d 个 guard-only 例外）"
        "+ %d 比较器按前缀扫描；BotSrv 已迁 %d 族无裸前缀 skip 诊断；"
        "teeth bare=%d prefixed=%d empty=%d skip_prefixed=%d skip_bare=%d）\n"
        % (len(COMPARATORS) + len(GUARD_ONLY_MARKERS), len(GUARD_ONLY_MARKERS),
           len(COMPARATORS), len(SKIP_FAMILIES), info["bare_rc"],
           info["prefixed_rc"], info["empty_rc"],
           info["skip_prefixed_rc"], info["skip_bare_rc"]))
    return 0


def _scan_synthetic(source):
    return [ln for _, ln in _code_lines(source) if STRIP_RE.search(ln)]


def _scan_guards_synthetic(source):
    """与 `scan_guards` 同口径的纯函数（逐代码行匹配，跳过 `#` 注释行）。"""
    out = []
    for _i, line in _code_lines(source):
        for m in GUARD_RE.finditer(line):
            out.append((m.group(2), m.group(1)))
    return out


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

    # O-1 收口（reviewer-6 `49993d1f` 的非阻塞 Observation）：牙也必须按代码行扫。
    # 旧写法扫全文 ⇒ 注释掉的 `Assert-M3MarkerPrefix` 仍被当成真牙（fail-open）。
    g_real = "    Assert-M3MarkerPrefix -Path $shopMarkersPath -Marker 'M3SHOPSEQ'\n"
    g_cmt = "#    Assert-M3MarkerPrefix -Path $shopMarkersPath -Marker 'M3SHOPSEQ'\n"
    check("scan-guards-catches-real",
          _scan_guards_synthetic(g_real) == [("M3SHOPSEQ", "shopMarkersPath")])
    check("scan-guards-ignores-comment", not _scan_guards_synthetic(g_cmt))

    # O-1' 收口（reviewer-5 `d853bd1a` 的非阻塞 Observation）：`_code_lines()` 只跳整行 `#`
    # ⇒ `run_pair.ps1:2-34` 的 `<# ... #>` 块注释里的同名牙会被当真牙（fail-open）。
    # 下面的牙把块注释（含跨行/嵌套/同行段）也纳入「注释」：牙藏于块注释 = 缺牙 ⇒ RED。
    g_blk = "<# " + g_real.strip() + " #>\n"
    g_blk_ml = "<#\n" + g_real + "#>\n"
    g_blk_nested = "<# outer <# " + g_real.strip() + " #> still outer #>\n"
    g_blk_reopen = "<# c #> " + g_real
    check("scan-guards-ignores-blockcomment-inline", not _scan_guards_synthetic(g_blk))
    check("scan-guards-ignores-blockcomment-multiline", not _scan_guards_synthetic(g_blk_ml))
    check("scan-guards-ignores-blockcomment-nested", not _scan_guards_synthetic(g_blk_nested))
    check("scan-guards-catches-after-blockclose",
          _scan_guards_synthetic(g_blk_reopen) == [("M3SHOPSEQ", "shopMarkersPath")])
    check("scan-ignores-strip-in-blockcomment",
          not _scan_synthetic("<# " + bad.strip() + " #>\n"))
    check("code-lines-blanks-blockcomment-body",
          [ln.strip() for _i, ln in _code_lines("<#\n" + g_real + "#>\n")] == ["", "", ""])
    check("code-lines-keeps-code-after-blockclose",
          [ln.strip() for _i, ln in _code_lines(g_blk_reopen)]
          == ["Assert-M3MarkerPrefix -Path $shopMarkersPath -Marker 'M3SHOPSEQ'"])
    # 真身复现：`run_pair.ps1:2-34` 确有 `<# ... #>` 头部块注释（订正旧片 README 的「未使用」）。
    with open(RUN_PAIR, "r", encoding="utf-8") as fh:
        rp_text = fh.read()
    check("real-run-pair-has-blockcomment", ("<#" in rp_text) and ("#>" in rp_text))

    findings, info = _evaluate()
    check("real-run-pair-no-strip", not info["strips"])
    guard_markers = [m for m, _ in info["guards"]]
    check("real-guards-wired", guards_wired(guard_markers))
    check("guards-extra-unregistered-red",
          not guards_wired(guard_markers + ["M3NEWSEQ"]))
    # O-2 收口（reviewer-6 `49993d1f`）：旧 `guards-guard-only-ok` 是**恒真项** —— 它喂给
    #   `guards_wired` 的集合恰等于该函数内部算出的 expected，永远 True、无法失败（冗余断言）。
    #   改成**可失败向**：把真实扫描到的牙集合去掉一个 ⇒ 必须判「未接线」，证 `guards_wired`
    #   的等值判据是双向的（`guards-extra-unregistered-red` 从「多」向咬合，本牙从「少」向咬合）。
    #   O-2 硬化：用 `_drop_one_guard`（`set` 去重后切一刀）而非 `guard_markers[1:]`，
    #   使牙对「同一 marker 重复出现」不假红（见 `_drop_one_guard` docstring）。
    check("guards-missing-real-guard-red",
          bool(_drop_one_guard(guard_markers)) and not guards_wired(_drop_one_guard(guard_markers)))
    check("drop-one-guard-dedup-proper-subset（重复标记下仍恰少一个）",
          _drop_one_guard(["A", "A", "B", "C"]) == ["B", "C"])
    check("naive-slice-not-proper-subset-under-dup（O-2 根因复现）",
          set(["A", "A", "B", "C"][1:]) == set(["A", "A", "B", "C"]))
    for marker, cmp in COMPARATORS:
        check("comparator-requires-prefix-%s" % marker,
              comparator_requires_prefix(os.path.join(HERE, cmp), marker))
    check("teeth-bare-prereq", info["bare_rc"] == 2)
    check("teeth-prefixed-ok", info["prefixed_rc"] == 0)
    check("teeth-empty-prereq", info["empty_rc"] == 2)
    check("teeth-skip-prefixed-ok", info["skip_prefixed_rc"] == 0)
    check("teeth-skip-bare-prereq", info["skip_bare_rc"] == 2)
    check("scan-botsrv-catches-bare-skip",
          len(_scan_botsrv_text('MainOutMessage("M3SHOPSEQ op=" + op + " skip=no_npc_in_view");\n')) == 1)
    check("scan-botsrv-ignores-skip-prefix",
          not _scan_botsrv_text('MainOutMessage("M3SHOPSEQ_SKIP op=" + op + " skip=no_npc_in_view");\n'))
    check("scan-botsrv-ignores-done-marker",
          not _scan_botsrv_text('MainOutMessage("M3BAGSEQ done");\n'))
    check("real-botsrv-no-bare-skip", not info["bare_skips"])
    check("check-green", not findings)

    with tempfile.TemporaryDirectory(prefix="marker-prefix-mutselftest-") as tmp:
        muts = _mutations(tmp)
    by_name = dict((n, f) for n, _p, f in muts)
    check("mutation-strip-red", any("去前缀截断" in f for f in by_name.get("run_pair_strip.ps1", [])))
    check("mutation-guard-red", any("缺生产 fail-closed 牙" in f and "M3SHOPSEQ" in f
                                     for f in by_name.get("run_pair_noguard.ps1", [])))
    check("mutation-guard-commented-red", any("缺生产 fail-closed 牙" in f and "M3SHOPSEQ" in f
                                               for f in by_name.get("run_pair_guard_commented.ps1", [])))
    check("mutation-guard-blockcommented-red",
          any("缺生产 fail-closed 牙" in f and "M3SHOPSEQ" in f
              for f in by_name.get("run_pair_guard_blockcommented.ps1", [])))
    check("mutation-bareskip-red", any("裸前缀 skip=" in f
                                        for f in by_name.get("RobotPlayer_bareskip.cs", [])))

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
