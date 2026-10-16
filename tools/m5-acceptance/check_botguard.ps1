#requires -Version 5
<#
.SYNOPSIS
  check_botguard.ps1 —— 离线自检 `tools/botload/BotGuard.psm1` 的五条护栏牙（改坏必红）。

.DESCRIPTION
  背景（2026-10-11 死机取证）：BotSrv.exe 长跑把提交内存顶到 106–109 GB，超本机 63.1 GB 提交上限。
  本批给假人驱动加了内存护栏（进程级堆上限 / 看门狗 / 有界跑），本脚本用**离线**方式（不连服务器、
  不占端口、不跑真 BotSrv 登录）证明这套护栏**真的长牙**：

    牙① 上限注入：`New-BotGuardEnvOverrides -HeapLimitGb 4` 必须得到 `DOTNET_GCHeapHardLimit=0x100000000`；
         且消费脚本（load_gate.ps1 / run_bots.ps1）里"起 BotSrv 的 Start-Process"**之前**必须有注入调用，
         且必须有 env 审计打印。
         改坏：把 hex 换算改错、或把消费脚本里的注入行删掉 ⇒ RED。
    牙② 看门狗：起一个**假进程**吃内存冒充"越界进程"，断言 `Find-MemBreach` 识别越界、`Format-BotGuardBreachRed`
         打出含 RED/进程名/实测值/阈值的行、`Stop-BotSrvForBreach` **真的把它杀掉**并确认进程已退出。
         改坏：让 `Test-MemBreach` 恒不触发 ⇒ 识别不到 ⇒ RED。
    牙③ 有界跑：`Test-BotRunBound` 在超时长/超迭代时必须返回 'time'/'iterations'、未到返回空；且一个受它
         约束的循环必须在迭代上界内停下。
         改坏：让 `Test-BotRunBound` 恒返回空 ⇒ 循环跑到兜底上限 ⇒ RED。

  另外做几条静态接线断言（load_gate 必须调用 Find-MemBreach/Stop-BotSrvForBreach/Test-BotRunBound，
  MemProcNames 必须含 BotSrv）——防止"模块定义了却没人用"的 orphan。

  牙④ 保护体检（独立复验者 must_fix #1，2026-10-16）：`-BotHeapLimitGB` 与 `-MemKillThresholdGB` 同时缺失
  （含阈值取到形同不设限的极大值）时，护栏原先**完全静默**（只剩一行 heap_limit=DISABLED），零保护配置可静默复发。
  断言：六种组合（两个都关 / 只关堆上限 / 只关看门狗 / 阈值取到极大[堆开/堆关] / 默认）逐一非静默——要么
  `refuse_start`（拒跑），要么审计行 `RED` + 可机判 verdict；**并真跑** load_gate/run_bots 的零护栏组合，
  断言 rc≠0 + `ZERO_PROTECTION_REFUSED`（fail-closed）；默认配置必须是 FULL。
  改坏：让 `Get-BotGuardProtection` 把 heap_on/watchdog_on 恒判为开 ⇒ 组合体检 + 端到端必红。

  牙⑤ 越界提前终止外层（独立复验者 must_fix #2，2026-10-16）：文档说"越界即 break 到收尾"，实测只 break 了
  内层稳态循环，外层 tier/repeat 仍继续、**每档再杀一次 BotSrv**。断言：load_gate 外层 repeat 与内层 tier 循环
  各带标签、越界块置 `mem_breach`、越界后 break 到外层（`break tierLoop` 在越界块之后 + `break runLoop`）、
  summary 写明"剩余档位未跑"。牙 = 对真实脚本的静态 AST 断言（同 check_stop_guard.ps1 范式）。
  改坏：删掉 `break runLoop` ⇒ 必红（见调用方留证）。

  牙⑥ 天花板由机器推导、不可由操作者放大（B3 复验者 must_fix #1，2026-10-16）：第一版把"护栏有效性"交给
  **操作者自定义的天花板** `-MemKillThresholdCeilingGB` 判定 ⇒ `-BotHeapLimitGB 1000 -MemKillThresholdGB 100000
  -MemKillThresholdCeilingGB 100001` 把空看门狗 + 空堆上限重新判成 FULL/protected，零保护长跑**静默**。
  修法：天花板 = 本机提交上限 × 0.5（`Get-MachineCommitLimitGb` 现场读 Win32_OperatingSystem）；操作者值只能
  更严、放大被夹回（`ceiling_clamped=yes`）。断言：机推上限可注入、复验者组合判 NONE/拒跑、操作者天花板单调
  不放大、**穷举 18 格组合表无静默格**；变异对照（改回只看>0 / 去掉夹取 ⇒ 组合变静默 ⇒ 必红）。
  牙⑦ 堆上限对称有效（B3 复验者 must_fix #2）：堆上限原只有"大于 0 即算开"的判定 ⇒ `-BotHeapLimitGB 1000
  -MemKillThresholdGB 0` 被误判 PARTIAL 而非 NONE/拒跑（实际保护同样为零）。断言：堆上限按"阈值 < 有效上界"
  判（1000GiB ⇒ 失效 ⇒ 与看门狗同时失效即 NONE/rc=2；4GiB ⇒ 仍有效 ⇒ PARTIAL）；变异对照（堆只看>0 ⇒ 必红）。
  牙⑧ -WriteConfigOnly 承诺"不构建"（B3 复验者 must_fix #3）：注释称"只写配置不构建/不起进程"，但该路径在
  exit 前仍会 `dotnet build`。断言：run_bots 的 dotnet build 被 `-WriteConfigOnly` 守卫（静态 AST）+ 真跑
  `-WriteConfigOnly`（临时仓库骨架，不碰真实 src/**）无 `BUILD BotSrv` 且 rc=0；变异对照（去守卫 ⇒ 又 BUILD ⇒ 必红）。

  只读消费脚本 + 起一个临时内存占用假进程；不执行被检脚本本身（牙④/⑦ 的端到端拒跑除外：那些只触发"拒绝启动"
  分支，在任何构建/起进程之前退出，无副作用；牙⑧ 的 -WriteConfigOnly 跑在临时仓库骨架里，配置只写临时目录）。

.EXAMPLE
  pwsh -File tools/m5-acceptance/check_botguard.ps1
.EXAMPLE
  # 变异对照（改坏必红）：把变量恒假的模块副本喂进来 ⇒ 必须 rc=1
  pwsh -File tools/m5-acceptance/check_botguard.ps1 -GuardModule /tmp/mut_watchdog.psm1
.EXAMPLE
  # 变异对照（改坏必红）：把删掉 break runLoop 的 load_gate 副本喂进来 ⇒ 牙⑤ 必红
  pwsh -File tools/m5-acceptance/check_botguard.ps1 -LoadGate /tmp/mut_load_gate.ps1
#>
param(
  [string]$GuardModule = (Join-Path (Split-Path -Parent $PSScriptRoot) 'botload\BotGuard.psm1'),
  [string]$LoadGate    = (Join-Path (Split-Path -Parent $PSScriptRoot) 'botload\load_gate.ps1'),
  [string]$RunBots     = (Join-Path (Split-Path -Parent $PSScriptRoot) 'botload\run_bots.ps1'),
  [int]$FakeMb = 384,      # 假进程要占的内存量（MB）；阈值取其一半
  [switch]$SkipRuntime     # 跳过牙②的起进程部分（仅静态/纯函数）
)

$ErrorActionPreference = 'Stop'
$script:fail = 0
$script:checks = 0
function Ok([string]$m)  { $script:checks++; Write-Output ("OK   " + $m) }
function Bad([string]$m) { $script:checks++; Write-Output ("RED: " + $m); $script:fail++ }

Write-Output ("GUARD_MODULE=" + $GuardModule)
if (-not (Test-Path $GuardModule)) { Write-Output ("RED: 护栏模块不存在: " + $GuardModule); exit 1 }
Import-Module (Resolve-Path $GuardModule).Path -Force

# 端到端牙需要一个 Windows PowerShell 宿主来跑消费脚本；缺则回退 pwsh（本机两者都有）。
$script:GuardShell = 'powershell'
if (-not (Get-Command powershell -ErrorAction SilentlyContinue)) { $script:GuardShell = 'pwsh' }

# ---------- 牙① 上限注入 ----------
# (a) 纯函数：GB -> hex
$h4 = ConvertTo-GCHeapHardLimitHex -HeapLimitGb 4
if ($h4 -eq '0x100000000') { Ok ("上限换算 4GiB -> " + $h4) } else { Bad ("4GiB 应得 0x100000000，实得 '" + $h4 + "'") }
$h1 = ConvertTo-GCHeapHardLimitHex -HeapLimitGb 1
if ($h1 -eq '0x40000000') { Ok ("上限换算 1GiB -> " + $h1) } else { Bad ("1GiB 应得 0x40000000，实得 '" + $h1 + "'") }
$h0 = ConvertTo-GCHeapHardLimitHex -HeapLimitGb 0
if ([string]::IsNullOrEmpty($h0)) { Ok '上限换算 0 -> 空（不注入）' } else { Bad ("0 应得空，实得 '" + $h0 + "'") }

$ov = New-BotGuardEnvOverrides -HeapLimitGb 4
if ($ov.Contains('DOTNET_GCHeapHardLimit') -and $ov['DOTNET_GCHeapHardLimit'] -eq '0x100000000') {
    Ok "注入表含 DOTNET_GCHeapHardLimit=0x100000000"
} else { Bad "注入表不含正确 DOTNET_GCHeapHardLimit" }

$audit = Format-BotGuardEnvAudit -Exe 'BotSrv.exe' -Overrides $ov -HeapLimitGb 4
if ($audit -match 'BOTGUARD_ENV' -and $audit -match 'DOTNET_GCHeapHardLimit=0x100000000') { Ok ("审计行: " + $audit) }
else { Bad ("审计行不合格: " + $audit) }

# (b) 静态 AST：起 BotSrv 前必须有注入 + 审计
function Get-CmdAsts($ast, [string[]]$names) {
    return @($ast.FindAll({
        param($n)
        ($n -is [System.Management.Automation.Language.CommandAst]) -and ($names -contains $n.GetCommandName())
    }, $true))
}
function Check-HeapInject([string]$path) {
    if (-not (Test-Path $path)) { Bad ("缺少消费者脚本: " + $path); return }
    $leaf = Split-Path -Leaf $path
    $tok = $null; $perr = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $path).Path, [ref]$tok, [ref]$perr)
    if ($perr.Count -gt 0) { Bad ($leaf + " 解析失败: " + $perr[0].Message); return }
    $starts = @($ast.FindAll({
        param($n)
        ($n -is [System.Management.Automation.Language.CommandAst]) -and
        ($n.GetCommandName() -eq 'Start-Process') -and ($n.Extent.Text -match '\$BotExe')
    }, $true))
    if ($starts.Count -eq 0) { Bad ($leaf + ' 未找到起 BotSrv 的 Start-Process($BotExe)'); return }
    $startPos = $starts[0].Extent.StartOffset
    $inject = @(Get-CmdAsts $ast @('New-BotGuardEnvOverrides', 'Set-BotGuardEnv') | Sort-Object { $_.Extent.StartOffset })
    $auditC = @(Get-CmdAsts $ast @('Format-BotGuardEnvAudit'))
    $modLoad = @($ast.FindAll({
        param($n)
        ($n -is [System.Management.Automation.Language.CommandAst]) -and
        ($n.GetCommandName() -eq 'Import-Module') -and ($n.Extent.Text -match 'BotGuard')
    }, $true))
    if ($modLoad.Count -eq 0) { Bad ($leaf + " 未用 Import-Module 加载 BotGuard.psm1（dot-source .psm1 不定义函数）"); return }
    Ok ($leaf + " 以 Import-Module 加载 BotGuard.psm1")
    if ($inject.Count -eq 0) { Bad ($leaf + " 缺堆上限注入（New-BotGuardEnvOverrides/Set-BotGuardEnv）"); return }
    if ($inject[0].Extent.StartOffset -ge $startPos) { Bad ($leaf + " 注入晚于起进程（@" + $inject[0].Extent.StartOffset + " >= 起进程@" + $startPos + "）"); return }
    if ($auditC.Count -eq 0) { Bad ($leaf + " 缺 env 审计打印（Format-BotGuardEnvAudit）"); return }
    Ok ($leaf + " 堆上限注入@" + $inject[0].Extent.StartOffset + " < 起 BotSrv@" + $startPos + "，含审计打印")
}
Check-HeapInject $LoadGate
Check-HeapInject $RunBots

# (c) 静态接线：模块别成 orphan
function Check-Calls([string]$path, [string[]]$names) {
    if (-not (Test-Path $path)) { Bad ("缺少脚本: " + $path); return }
    $leaf = Split-Path -Leaf $path
    $txt = Get-Content -Raw $path
    foreach ($n in $names) {
        if ($txt.Contains($n)) { Ok ($leaf + " 调用 " + $n) } else { Bad ($leaf + " 未调用 " + $n) }
    }
}
Check-Calls $LoadGate @('New-BotGuardEnvOverrides', 'Format-BotGuardEnvAudit', 'Find-MemBreach', 'Stop-BotSrvForBreach', 'Test-BotRunBound')
Check-Calls $RunBots  @('New-BotGuardEnvOverrides', 'Format-BotGuardEnvAudit', 'Find-MemBreach', 'Stop-BotSrvForBreach')

$lgText = Get-Content -Raw $LoadGate
if ($lgText -match '(?s)\$MemProcNames\s*=\s*@\([^)]*"BotSrv"') { Ok "load_gate \$MemProcNames 含 BotSrv（假人内存斜率可见）" }
else { Bad "load_gate \$MemProcNames 缺 BotSrv" }

# ---------- 牙② 看门狗（起假进程，断言真的杀掉）----------
if ($SkipRuntime) {
    Write-Output "SKIP 牙② 起进程部分（-SkipRuntime）；仅静态/纯函数已检"
} else {
    $fakeHz = [int64]$FakeMb * 1MB
    $threshold = [int64]($FakeMb / 2) * 1MB
    $tmpScript = Join-Path ([System.IO.Path]::GetTempPath()) ("botguard_fake_" + $PID + ".ps1")
    [System.IO.File]::WriteAllText($tmpScript, @'
$a = New-Object 'byte[]' ([int64]$env:FAKE_BYTES)
for ($i = 0; $i -lt $a.Length; $i += 4096) { $a[$i] = 1 }
Start-Sleep -Seconds 300
'@, (New-Object System.Text.UTF8Encoding($false)))
    $env:FAKE_BYTES = "$fakeHz"
    $fake = $null
    try {
        $fake = Start-Process -FilePath 'powershell' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $tmpScript) -PassThru -WindowStyle Hidden
        # 等假进程涨到阈值以上
        $deadline = (Get-Date).AddSeconds(30)
        $s = $null
        while ((Get-Date) -lt $deadline) {
            $s = Get-ProcMemSample -Name 'fakebot' -Pid $fake.Id
            if ($s -and $s.priv_bytes -ge $threshold) { break }
            Start-Sleep -Milliseconds 200
        }
        if (-not $s -or $s.priv_bytes -lt $threshold) {
            Bad ("假进程未涨到阈值（实测 " + [math]::Round(([double]$s.priv_bytes) / 1MB, 1) + "MB < " + [math]::Round($threshold / 1MB, 1) + "MB）")
        } else {
            $b = Find-MemBreach -Samples @($s) -ThresholdBytes $threshold
            if ($null -eq $b) {
                Bad "看门狗未识别越界（阈值判定失效）"
            } else {
                Ok ("看门狗识别越界 " + $b.proc + " pid=" + $b.pid + " " + $b.metric + "=" + [math]::Round($b.value_bytes / 1MB, 1) + "MB")
                $red = Format-BotGuardBreachRed -Breach $b
                if ($red -match '^RED ' -and $red -match 'BOTGUARD_MEM_BREACH' -and $red -match 'measured_mb=') { Ok ("RED 行: " + $red) }
                else { Bad ("RED 行不合格: " + $red) }
                $killed = @(Stop-BotSrvForBreach -Breach $b -BotPids @($fake.Id))
                Start-Sleep -Milliseconds 500
                $fake.Refresh()
                if ($killed.Count -ge 1 -and $fake.HasExited) { Ok ("看门狗杀掉越界进程 pid=" + $fake.Id + "（killed=" + $killed.Count + " exited=" + $fake.HasExited + "）") }
                else { Bad ("看门狗未杀掉越界进程（killed=" + $killed.Count + " exited=" + $fake.HasExited + "）") }
            }
        }
    } finally {
        if ($fake -and -not $fake.HasExited) { Stop-Process -Id $fake.Id -Force -ErrorAction SilentlyContinue }
        if (Test-Path $tmpScript) { Remove-Item $tmpScript -Force -ErrorAction SilentlyContinue }
    }
}

# ---------- 牙③ 有界跑 ----------
$r1 = Test-BotRunBound -ElapsedSec 10 -MaxRunSec 5 -Iteration 0 -MaxIterations 0
if ($r1 -eq 'time') { Ok "超时长 -> 'time'" } else { Bad ("超时应得 'time'，实得 '" + $r1 + "'") }
$r2 = Test-BotRunBound -ElapsedSec 0 -MaxRunSec 0 -Iteration 3 -MaxIterations 3
if ($r2 -eq 'iterations') { Ok "超迭代 -> 'iterations'" } else { Bad ("超迭代应得 'iterations'，实得 '" + $r2 + "'") }
$r3 = Test-BotRunBound -ElapsedSec 1 -MaxRunSec 5 -Iteration 1 -MaxIterations 5
if ([string]::IsNullOrEmpty($r3)) { Ok "界内 -> 空" } else { Bad ("界内应得空，实得 '" + $r3 + "'") }
$r4 = Test-BotRunBound -ElapsedSec 9999 -MaxRunSec 0 -Iteration 999 -MaxIterations 0
if ([string]::IsNullOrEmpty($r4)) { Ok "0=不限 -> 空" } else { Bad ("0=不限应得空，实得 '" + $r4 + "'") }

# 受约束的循环必须在迭代上界内停下（改坏 -> 跑到兜底 cap -> 判红）
$maxIter = 3; $cap = 500; $i = 0; $reason = $null
while ($true) {
    $i++
    $reason = Test-BotRunBound -ElapsedSec 0 -MaxRunSec 0 -Iteration $i -MaxIterations $maxIter
    if ($reason) { break }
    if ($i -ge $cap) { break }
}
if ($reason -eq 'iterations' -and $i -le $maxIter) { Ok ("受约束循环在第 " + $i + " 迭停下（上界 " + $maxIter + "）") }
else { Bad ("受约束循环未在迭代上界停下（i=" + $i + " reason='" + $reason + "'）") }

# ---------- 牙④ 保护体检：零护栏不许静默（must_fix #1）----------
# 六种组合逐一非静默：要么 refuse_start（拒跑），要么审计行 RED + 可机判 verdict；默认配置必须 FULL。
$combos = @(
  @{ name='两个都关';             heap=0; thr=0;     wantLevel='NONE';    wantRefuse=$true;  wantVerdict='ZERO_PROTECTION_REFUSED' },
  @{ name='只关堆上限';           heap=0; thr=8;     wantLevel='PARTIAL'; wantRefuse=$false; wantVerdict='PROTECTION_MISSING' },
  @{ name='只关看门狗';           heap=4; thr=0;     wantLevel='PARTIAL'; wantRefuse=$false; wantVerdict='PROTECTION_MISSING' },
  @{ name='阈值取到极大(堆也关)'; heap=0; thr=99999; wantLevel='NONE';    wantRefuse=$true;  wantVerdict='ZERO_PROTECTION_REFUSED' },
  @{ name='阈值取到极大(堆开)';   heap=4; thr=99999; wantLevel='PARTIAL'; wantRefuse=$false; wantVerdict='PROTECTION_MISSING' },
  @{ name='默认配置';             heap=4; thr=8;     wantLevel='FULL';    wantRefuse=$false; wantVerdict='protected' }
)
foreach ($c in $combos) {
    # 钉住机推提交上限 64GiB（机推天花板=32GiB）⇒ 组合期望与机器无关、可复现
    $p = Get-BotGuardProtection -HeapLimitGb $c.heap -MemKillThresholdGb $c.thr -MachineCommitLimitGb 64
    $line = Format-BotGuardProtectionAudit -Protection $p
    $ok = ($p.level -eq $c.wantLevel) -and ($p.refuse_start -eq $c.wantRefuse) -and ($line -match ('verdict=' + $c.wantVerdict))
    if ($c.wantLevel -eq 'FULL') { $ok = $ok -and ($line -notmatch '^RED ') }
    else { $ok = $ok -and ($line -match '^RED BOTGUARD_PROTECTION ') }
    if ($ok) { Ok ("保护体检[" + $c.name + "] heap=" + $c.heap + " thr=" + $c.thr + " -> " + $line) }
    else { Bad ("保护体检[" + $c.name + "] 不合格: " + $line) }
}

# 端到端 fail-closed：真跑消费脚本、两个开关都关 ⇒ 必须 rc≠0 + ZERO_PROTECTION_REFUSED（拒绝启动）。
# 该分支在任何构建/起进程之前退出 ⇒ 无副作用。默认配置下不可能走到这里（也就不会误拒）。
function Test-RefuseStart([string]$path, [string[]]$extraArgs, [string]$label) {
    if (-not (Test-Path $path)) { Bad ("缺少消费者脚本: " + $path); return }
    $psArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Resolve-Path $path).Path) + $extraArgs
    $out = & $script:GuardShell @psArgs 2>&1 | Out-String
    $rc = $LASTEXITCODE
    if ($rc -ne 0 -and $out -match 'ZERO_PROTECTION_REFUSED') {
        Ok ($label + " 零护栏拒绝启动 rc=" + $rc + "（fail-closed）")
    } else {
        Bad ($label + " 零护栏**未**拒绝启动（rc=" + $rc + "）：" + ($out.Trim()))
    }
}
Test-RefuseStart $LoadGate @('-Tiers', '3', '-BotHeapLimitGB', '0', '-MemKillThresholdGB', '0') 'load_gate'
Test-RefuseStart $RunBots  @('-Count', '3', '-BotHeapLimitGB', '0', '-MemKillThresholdGB', '0') 'run_bots'

# 端到端"部分保护必须响亮"：只关一项时（这里关堆上限），run_bots 的纯离线自检路径（-SelfCheckNoStack
# 不起进程、不算"长跑"故不拒绝启动）仍会打出 RED 审计行——保护缺失必须显式标红，绝不静默。
if (Test-Path $RunBots) {
    $psArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Resolve-Path $RunBots).Path,
                '-SelfCheckNoStack', '-BotHeapLimitGB', '0', '-MemKillThresholdGB', '8')
    $out = & $script:GuardShell @psArgs 2>&1 | Out-String
    if ($out -match 'RED BOTGUARD_PROTECTION level=PARTIAL verdict=PROTECTION_MISSING') { Ok "run_bots 只关堆上限：审计行显式标红（PARTIAL/PROTECTION_MISSING），非静默" }
    else { Bad ("run_bots 只关堆上限：未显式标红保护缺失（rc=" + $LASTEXITCODE + "）") }
} else { Bad ("缺少消费者脚本: " + $RunBots) }

# 静态接线：消费脚本必须调保护体检、在 refuse_start 时退出（拒绝启动），并把它写进 summary。
function Check-ProtectionWiring([string]$path) {
    if (-not (Test-Path $path)) { Bad ("缺少消费者脚本: " + $path); return }
    $leaf = Split-Path -Leaf $path
    $txt = Get-Content -Raw $path
    if ($txt.Contains('Get-BotGuardProtection') -and $txt.Contains('Format-BotGuardProtectionAudit')) { Ok ($leaf + " 调用保护体检（Get-BotGuardProtection / Format-BotGuardProtectionAudit）") }
    else { Bad ($leaf + " 未调用保护体检") }
    if ($txt -match '(?s)refuse_start.*exit\s+2') { Ok ($leaf + " 零护栏 fail-closed（refuse_start ⇒ exit 2）") }
    else { Bad ($leaf + " 缺零护栏 fail-closed（refuse_start ⇒ exit 2）") }
    if ($txt.Contains('protection_level')) { Ok ($leaf + " summary 记录 protection_level（可机判）") }
    else { Bad ($leaf + " summary 未记录 protection_level") }
}
Check-ProtectionWiring $LoadGate
Check-ProtectionWiring $RunBots

# ---------- 牙⑤ 越界提前终止外层档位循环（must_fix #2）----------
# 对真实 load_gate.ps1 的静态 AST 断言（同 check_stop_guard.ps1 范式）：去掉外层 break 即红。
# 注意：循环语句的 `.Label` 是 [string]，但 `break`/`continue` 的 `.Label` 是 [ExpressionAst]
# （StringConstantExpressionAst）——直接 `-eq 'x'` 恒 false，必须取 `.Extent.Text`（踩过一次）。
function Get-LabelText($node) {
    if ($null -eq $node.Label) { return '' }
    if ($node.Label -is [string]) { return $node.Label }
    return $node.Label.Extent.Text
}
function Check-OuterAbort([string]$path) {
    if (-not (Test-Path $path)) { Bad ("缺少消费者脚本: " + $path); return }
    $leaf = Split-Path -Leaf $path
    $tok = $null; $perr = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $path).Path, [ref]$tok, [ref]$perr)
    if ($perr.Count -gt 0) { Bad ($leaf + " 解析失败: " + $perr[0].Message); return }

    $runFor = @($ast.FindAll({ param($n) ($n -is [System.Management.Automation.Language.ForStatementAst]) -and ($n.Label -eq 'runLoop') }, $true))
    $tierFe = @($ast.FindAll({ param($n) ($n -is [System.Management.Automation.Language.ForEachStatementAst]) -and ($n.Label -eq 'tierLoop') }, $true))
    if ($runFor.Count -eq 1) { Ok ($leaf + " 外层 repeat 循环带标签 :runLoop") } else { Bad ($leaf + " 外层 repeat 循环缺 :runLoop 标签（外层 break 无从指向）") }
    if ($tierFe.Count -eq 1) { Ok ($leaf + " 内层 tier 循环带标签 :tierLoop") } else { Bad ($leaf + " 内层 tier 循环缺 :tierLoop 标签") }
    if ($runFor.Count -ne 1 -or $tierFe.Count -ne 1) { return }
    $tierLoop = $tierFe[0]

    # 越界处理块：条件含 $breach 且把原因置成 'mem_breach'
    $breachIf = @($tierLoop.FindAll({
        param($n)
        ($n -is [System.Management.Automation.Language.IfStatementAst]) -and
        ($n.Clauses.Count -ge 1) -and ($n.Clauses[0].Item1.Extent.Text -match '\$breach\b') -and
        ($n.Extent.Text -match 'mem_breach')
    }, $true))
    if ($breachIf.Count -ge 1) { Ok ($leaf + " 越界处理块置 abort_reason=mem_breach") } else { Bad ($leaf + " 越界处理块未置 abort_reason=mem_breach") }

    $brTier = @($tierLoop.FindAll({ param($n) ($n -is [System.Management.Automation.Language.BreakStatementAst]) -and ((Get-LabelText $n) -eq 'tierLoop') }, $true))
    $brRun  = @($runFor[0].FindAll({ param($n) ($n -is [System.Management.Automation.Language.BreakStatementAst]) -and ((Get-LabelText $n) -eq 'runLoop') }, $true))
    if ($brTier.Count -ge 1) { Ok ($leaf + " tier 循环内含 break tierLoop（越界后不再跑下一档）") } else { Bad ($leaf + " 缺 break tierLoop（越界后仍会跑下一档）") }
    if ($brRun.Count -ge 1) { Ok ($leaf + " repeat 循环内含 break runLoop（越界后不再跑后续 repeat）") } else { Bad ($leaf + " 缺 break runLoop（越界后仍会跑后续 repeat）") }
    if ($brTier.Count -ge 1 -and $breachIf.Count -ge 1) {
        if ($brTier[0].Extent.StartOffset -gt $breachIf[0].Extent.StartOffset) {
            Ok ($leaf + " break tierLoop 位于越界块之后（off " + $brTier[0].Extent.StartOffset + " > " + $breachIf[0].Extent.StartOffset + "）")
        } else {
            Bad ($leaf + " break tierLoop 早于越界块（越界档位自身也被跳过，收尾产物会缺）")
        }
    }

    $txt = Get-Content -Raw $path
    if ($txt.Contains('aborted_early') -and $txt.Contains('abort_reason') -and $txt.Contains('剩余档位未跑')) { Ok ($leaf + " summary 写明 aborted_early/abort_reason/剩余档位未跑") }
    else { Bad ($leaf + " summary 未写明提前终止（aborted_early/abort_reason/剩余档位未跑）") }
    if ($txt.Contains('tiers_run') -and $txt.Contains('tiers_planned')) { Ok ($leaf + " summary 记录 tiers_run/tiers_planned（剩余档位未跑可机判）") }
    else { Bad ($leaf + " summary 未记录 tiers_run/tiers_planned") }

    # 执行向构造（构造一次越界 ⇒ 断言剩余档位没有跑）：用 load_gate **真实**的 break 语句文本 + 真实循环标签，
    # 搭一个最小编排驱动，在"第 1 轮第 2 档"注入越界。跑到的档位必须恰为前两档（1:3,1:5）——
    # 若外层 break 缺失/被换成 continue，剩余档位会多跑 ⇒ 红。这是"断言语义"，不是恒真断言：
    # 删掉 `break runLoop`（见变异对照）时此判据随之失去可构造性（上文已先判红）。
    if ($brTier.Count -ge 1 -and $brRun.Count -ge 1) {
        $tmpl = @'
$ran = @()
:RUNL for ($run=1; $run -le 2; $run++) {
:TIERL foreach ($n in @(3,5,7)) {
  $ran += ("${run}:${n}")
  if ($run -eq 1 -and $n -eq 5) { $abort = 'mem_breach' }
  if ($abort) { BREAKTIER }
}
  if ($abort) { BREAKRUN }
}
$ran -join ','
'@
        $driver = $tmpl.Replace(':RUNL', ':' + $runFor[0].Label).Replace(':TIERL', ':' + $tierLoop.Label).Replace('BREAKTIER', $brTier[0].Extent.Text).Replace('BREAKRUN', $brRun[0].Extent.Text)
        $ran = $null
        try { $ran = & ([scriptblock]::Create($driver)) } catch { $ran = '<driver-error: ' + $_.Exception.Message + '>' }
        if ($ran -eq '1:3,1:5') { Ok ($leaf + " 越界构造：剩余档位未跑（ran=" + $ran + "，真实 break 文本=[" + $brTier[0].Extent.Text + "]/[" + $brRun[0].Extent.Text + "]）") }
        else { Bad ($leaf + " 越界构造：剩余档位仍跑了（ran=" + $ran + "）") }
    }
}
Check-OuterAbort $LoadGate


# ---------- 牙⑥ 天花板由机器推导、不可由操作者放大 + 穷举组合表无静默格（must_fix #1）----------
# 复验者复现（**未改动的真实脚本**）：`-BotHeapLimitGB 1000 -MemKillThresholdGB 100000
# -MemKillThresholdCeilingGB 100001` ⇒ 原实现把"操作者自定义的天花板"当权威 ⇒ 空看门狗 + 空堆上限被重新判成
# `level=FULL / verdict=protected`，无 RED、不拒跑，零保护长跑**静默**进行。
# 本牙：①纯函数断言"机推上限 = 本机提交上限 × 0.5，操作者值只能更严、放大值被夹回"；②穷举 18 格组合表
# （heap×thr×ceiling）逐格判"是否静默无保护"，必须**无静默格**；③变异对照（改坏必红）。
$script:tmpModDirs = New-Object System.Collections.Generic.List[string]
function Invoke-GuardModuleProbe {
    # 复制模块到临时目录、应用文本变异、在子宿主导入并跑 probe 源；返回 {out, rc}。临时目录登记待清。
    param([string]$SrcPath, [hashtable]$Edits, [string]$ProbeSrc)
    $raw = Get-Content -Raw $SrcPath
    foreach ($k in @($Edits.Keys)) {
        if (-not $raw.Contains($k)) { throw ("变异目标未找到: " + $k) }
        $raw = $raw.Replace($k, $Edits[$k])
    }
    $dir = Join-Path ([System.IO.Path]::GetTempPath()) ("botguard_mut_" + $PID + "_" + ([guid]::NewGuid().ToString('N').Substring(0,8)))
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $script:tmpModDirs.Add($dir)
    $enc = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText((Join-Path $dir 'BotGuard.psm1'), $raw, $enc)
    $probe = Join-Path $dir 'probe.ps1'
    [System.IO.File]::WriteAllText($probe, ("Import-Module '" + (Join-Path $dir 'BotGuard.psm1') + "' -Force`n" + $ProbeSrc), $enc)
    # 子进程的 stderr（如 dotnet/构建噪声）在 EAP=Stop 下会变成 terminating NativeCommandError ⇒ 临时放行
    $eap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try { $out = & $script:GuardShell -NoProfile -ExecutionPolicy Bypass -File $probe 2>&1 | Out-String }
    finally { $ErrorActionPreference = $eap }
    return [pscustomobject]@{ out = $out; rc = $LASTEXITCODE }
}

$PROBE_KILLER = @'
$p = Get-BotGuardProtection -HeapLimitGb 1000 -MemKillThresholdGb 100000 -ThresholdCeilingGb 100001 -MachineCommitLimitGb 64
Write-Output ("PROBE level=" + $p.level + " refuse=" + $p.refuse_start + " wd_eff=" + $p.watchdog_effective)
'@
$PROBE_HEAP = @'
$p = Get-BotGuardProtection -HeapLimitGb 1000 -MemKillThresholdGb 0 -MachineCommitLimitGb 64
Write-Output ("PROBE level=" + $p.level + " refuse=" + $p.refuse_start + " heap_eff=" + $p.heap_limit_effective)
'@
$PROBE_ZERO = @'
$p = Get-BotGuardProtection -HeapLimitGb 0 -MemKillThresholdGb 0 -MachineCommitLimitGb 64
Write-Output ("PROBE level=" + $p.level + " refuse=" + $p.refuse_start)
'@

# (a) 机推上限来源
if ((Get-MachineCommitLimitGb -OverrideGb 64) -eq 64) { Ok "机推提交上限注入 64GiB 生效（Get-MachineCommitLimitGb -OverrideGb）" }
else { Bad "Get-MachineCommitLimitGb -OverrideGb 64 失效" }
$realCommit = Get-MachineCommitLimitGb
if ($realCommit -gt 16) { Ok ("本机提交上限现场可测 = " + [math]::Round($realCommit, 2) + "GiB（机推上限 = " + [math]::Round($realCommit * 0.5, 2) + "GiB）") }
else { Bad ("本机提交上限现场测得异常小: " + $realCommit + "（探测可能失效）") }

# (b) 复验者那组组合：必须被判 NONE/拒跑，且操作者天花板被夹回机推值（不放大）
$k = Get-BotGuardProtection -HeapLimitGb 1000 -MemKillThresholdGb 100000 -ThresholdCeilingGb 100001 -MachineCommitLimitGb 64
if ($k.level -eq 'NONE' -and $k.refuse_start -and (-not $k.watchdog_effective) -and $k.ceiling_clamped -and ([math]::Abs($k.threshold_ceiling_gb - 32) -lt 1e-9)) {
    Ok ("复验者组合(heap=1000,thr=100000,ceil=100001)判 NONE/拒跑，天花板夹回 32GiB（原 100001，clamped=" + $k.ceiling_clamped + "）")
} else {
    Bad ("复验者组合未被判 NONE/夹取: level=" + $k.level + " refuse=" + $k.refuse_start + " ceil=" + $k.threshold_ceiling_gb + " clamped=" + $k.ceiling_clamped)
}

# (c) 单调不放大：操作者天花板不能把"本已失效"的看门狗重新判成有效
$lo = Get-BotGuardProtection -HeapLimitGb 4 -MemKillThresholdGb 40 -ThresholdCeilingGb 0      -MachineCommitLimitGb 64
$hi = Get-BotGuardProtection -HeapLimitGb 4 -MemKillThresholdGb 40 -ThresholdCeilingGb 100001 -MachineCommitLimitGb 64
if ((-not $lo.watchdog_effective) -and (-not $hi.watchdog_effective) -and ($hi.threshold_ceiling_gb -le $lo.threshold_ceiling_gb + 1e-9)) {
    Ok "操作者天花板不可放大（thr=40GiB>32 在低/高天花板下都判看门狗失效）"
} else {
    Bad ("操作者天花板可放大: lo_eff=" + $lo.watchdog_effective + " hi_eff=" + $hi.watchdog_effective + " lo_ceil=" + $lo.threshold_ceiling_gb + " hi_ceil=" + $hi.threshold_ceiling_gb)
}

# (d) 穷举组合表：heap∈{0,4,1000} × thr∈{0,8,100000} × ceiling∈{默认,100001}；逐格判"是否静默无保护"。
# 静默 := 不拒跑 且 审计行不带 RED 且 并非两项都有效（真保护）。**必须无静默格**。
$gridCommit = $realCommit
$gridSilent = 0
foreach ($gh in @(0, 4, 1000)) {
    foreach ($gt in @(0, 8, 100000)) {
        foreach ($gcName in @('默认', '100001')) {
            $gc = if ($gcName -eq '默认') { 0 } else { 100001 }
            $gp = Get-BotGuardProtection -HeapLimitGb $gh -MemKillThresholdGb $gt -ThresholdCeilingGb $gc -MachineCommitLimitGb $gridCommit
            $gline = Format-BotGuardProtectionAudit -Protection $gp
            $gred = [bool]($gline -match '^RED ')
            $grc = if ($gp.refuse_start) { 2 } else { 0 }
            $gprotected = [bool]($gp.heap_limit_effective -and $gp.watchdog_effective)
            $gsilent = [bool]((-not $gp.refuse_start) -and (-not $gred) -and (-not $gprotected))
            if ($gsilent) { $gridSilent++ }
            $gverdict = if ($gp.level -eq 'FULL') { 'protected' } elseif ($gp.level -eq 'PARTIAL') { 'PROTECTION_MISSING' } else { 'ZERO_PROTECTION_REFUSED' }
            Ok ("GRID heap=" + $gh + " thr=" + $gt + " ceil=" + $gcName + " -> level=" + $gp.level + " verdict=" + $gverdict + " rc=" + $grc + " silent=" + $gsilent)
        }
    }
}
if ($gridSilent -eq 0) { Ok ("穷举组合表 18 格无静默无保护格（commit=" + [math]::Round($gridCommit, 2) + "GiB）") }
else { Bad ("穷举组合表存在 " + $gridSilent + " 个静默无保护格") }

# (e) 变异对照：有效性判定改回"只看 >0" ⇒ 复验者组合重新变 FULL/不拒跑 ⇒ 必红
$rReal = Invoke-GuardModuleProbe -SrcPath $GuardModule -Edits @{} -ProbeSrc $PROBE_KILLER
if ($rReal.out -match 'level=NONE' -and $rReal.out -match 'refuse=True') { Ok ("对照：未变异模块同组合判 NONE/拒跑（" + $rReal.out.Trim() + "）") }
else { Bad ("对照失败（未变异模块同组合未判 NONE/拒跑）: " + $rReal.out.Trim()) }
$mutEff = @{
    '$heapEffective = [bool]($heapOn -and ([double]$HeapLimitGb -lt $effCeilingGb))'         = '$heapEffective = $heapOn'
    '$watchdogEffective = [bool]($watchdogOn -and ($MemKillThresholdGb -lt $effCeilingGb))' = '$watchdogEffective = $watchdogOn'
}
$rEff = Invoke-GuardModuleProbe -SrcPath $GuardModule -Edits $mutEff -ProbeSrc $PROBE_KILLER
if ($rEff.out -match 'level=FULL' -and $rEff.out -match 'refuse=False') { Ok "改坏必红：有效性改回只看>0 ⇒ 复验者组合变 FULL/不拒跑（牙⑥ 能咬住）" }
else { Bad ("改坏必红失效（有效性只看>0 未使组合变静默）: " + $rEff.out.Trim()) }
# (f) 变异对照：去掉天花板夹取 ⇒ 复验者组合变 FULL/不拒跑 ⇒ 必红
$mutClamp = @{
    'elseif ($ThresholdCeilingGb -gt $machineCeilingGb) { $clamped = $true }' = 'elseif ($ThresholdCeilingGb -gt $machineCeilingGb) { $effCeilingGb = $ThresholdCeilingGb; $clamped = $true }'
}
$rClamp = Invoke-GuardModuleProbe -SrcPath $GuardModule -Edits $mutClamp -ProbeSrc $PROBE_KILLER
if ($rClamp.out -match 'level=FULL' -and $rClamp.out -match 'refuse=False') { Ok "改坏必红：去掉天花板夹取 ⇒ 复验者组合变 FULL/不拒跑（牙⑥ 能咬住）" }
else { Bad ("改坏必红失效（去夹取未使组合变静默）: " + $rClamp.out.Trim()) }

# ---------- 牙⑦ 堆上限对称有效 + NONE 拒跑（must_fix #2 / #1）----------
# 堆上限原只有"大于 0 即算开"的判定 ⇒ `-BotHeapLimitGB 1000 -MemKillThresholdGB 0` 被误判 PARTIAL（只打 RED）
# 而非 **NONE/拒跑**，可实际保护同样为零。修法：堆上限与看门狗走同一套"阈值 < 有效上界"判定。
$hp = Get-BotGuardProtection -HeapLimitGb 1000 -MemKillThresholdGb 0 -MachineCommitLimitGb 64
if ($hp.level -eq 'NONE' -and $hp.refuse_start -and $hp.heap_limit_on -and (-not $hp.heap_limit_effective)) {
    Ok "堆上限 1000GiB 在 32GiB 机推上限下判失效 ⇒ 与看门狗同时失效即 NONE/拒跑（对称）"
} else {
    Bad ("堆上限对称判定失效: level=" + $hp.level + " refuse=" + $hp.refuse_start + " heap_on=" + $hp.heap_limit_on + " heap_eff=" + $hp.heap_limit_effective)
}
$hp2 = Get-BotGuardProtection -HeapLimitGb 4 -MemKillThresholdGb 0 -MachineCommitLimitGb 64
if ($hp2.level -eq 'PARTIAL' -and (-not $hp2.refuse_start) -and $hp2.heap_limit_effective) {
    Ok "堆上限 4GiB 仍判有效 ⇒ 只缺看门狗 = PARTIAL（不误判为关闭）"
} else {
    Bad ("堆上限 4GiB 误判: level=" + $hp2.level + " heap_eff=" + $hp2.heap_limit_effective)
}
# 端到端：两种消费者脚本、两种"对称零保护"组合都必须拒跑（fail-closed，在任何构建/起进程之前退出）
Test-RefuseStart $LoadGate @('-Tiers', '3', '-BotHeapLimitGB', '1000', '-MemKillThresholdGB', '0') 'load_gate'
Test-RefuseStart $RunBots  @('-Count', '3', '-BotHeapLimitGB', '1000', '-MemKillThresholdGB', '0') 'run_bots'
Test-RefuseStart $LoadGate @('-Tiers', '3', '-BotHeapLimitGB', '1000', '-MemKillThresholdGB', '100000', '-MemKillThresholdCeilingGB', '100001') 'load_gate'
Test-RefuseStart $RunBots  @('-Count', '3', '-BotHeapLimitGB', '1000', '-MemKillThresholdGB', '100000', '-MemKillThresholdCeilingGB', '100001') 'run_bots'
# 变异对照：堆有效性改回"只看 >0" ⇒ 对称零保护组合变 PARTIAL/不拒跑 ⇒ 必红
$mutHeap = @{ '$heapEffective = [bool]($heapOn -and ([double]$HeapLimitGb -lt $effCeilingGb))' = '$heapEffective = $heapOn' }
$rHeap = Invoke-GuardModuleProbe -SrcPath $GuardModule -Edits $mutHeap -ProbeSrc $PROBE_HEAP
if ($rHeap.out -match 'level=PARTIAL' -and $rHeap.out -match 'refuse=False' -and $rHeap.out -match 'heap_eff=True') {
    Ok "改坏必红：堆有效性改回只看>0 ⇒ 对称零保护组合变 PARTIAL/不拒跑（牙⑦ 能咬住）"
} else { Bad ("改坏必红失效（堆只看>0 未使组合变静默）: " + $rHeap.out.Trim()) }
# 变异对照：把 NONE 的拒跑改成继续跑 ⇒ 零护栏组合 refuse=False ⇒ 必红
$mutRefuse = @{ '[bool]($level -eq ''NONE'')' = '[bool]$false' }
$rRefuse = Invoke-GuardModuleProbe -SrcPath $GuardModule -Edits $mutRefuse -ProbeSrc $PROBE_ZERO
if ($rRefuse.out -match 'level=NONE' -and $rRefuse.out -match 'refuse=False') {
    Ok "改坏必红：NONE 的拒跑改成继续跑 ⇒ 零护栏组合 refuse=False（牙④/⑦ 能咬住）"
} else { Bad ("改坏必红失效（拒跑改继续跑未生效）: " + $rRefuse.out.Trim()) }

# ---------- 牙⑧ -WriteConfigOnly 声称"不构建"必须与行为一致（must_fix #3）----------
# 复验者：param 注释称"只写 AppSetting.json 并退出（不构建/不起进程）"，但脚本在 exit 前**仍会 dotnet build**
# （约 :276-278，除非 -SkipBuild 且 exe 已存在）。修法：该路径强制跳过构建，行为与注释对齐。
# 静态：run_bots 里的 dotnet build 必须被 `-WriteConfigOnly` 守卫；行为：-WriteConfigOnly 不得构建、须 rc=0。
function Test-BuildGuardedByWriteConfigOnly([string]$path) {
    if (-not (Test-Path $path)) { Bad ("缺少脚本: " + $path); return }
    $leaf = Split-Path -Leaf $path
    $tok = $null; $perr = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $path).Path, [ref]$tok, [ref]$perr)
    if ($perr.Count -gt 0) { Bad ($leaf + " 解析失败: " + $perr[0].Message); return }
    $builds = @($ast.FindAll({
        param($n)
        ($n -is [System.Management.Automation.Language.CommandAst]) -and
        ($n.GetCommandName() -eq 'dotnet') -and ($n.Extent.Text -match 'build')
    }, $true))
    if ($builds.Count -eq 0) { Bad ($leaf + " 未找到 dotnet build"); return }
    $guarded = $false
    foreach ($b in $builds) {
        $anc = $b
        while ($null -ne $anc.Parent) {
            $anc = $anc.Parent
            if (($anc -is [System.Management.Automation.Language.IfStatementAst]) -and
                ($anc.Clauses.Count -ge 1) -and ($anc.Clauses[0].Item1.Extent.Text -match '\$WriteConfigOnly')) { $guarded = $true; break }
        }
        if ($guarded) { break }
    }
    if ($guarded) { Ok ($leaf + " 的 dotnet build 被 -WriteConfigOnly 守卫（该路径不构建）") }
    else { Bad ($leaf + " 的 dotnet build 未被 -WriteConfigOnly 守卫（声称不构建却会构建）") }
}
Test-BuildGuardedByWriteConfigOnly $RunBots

# 行为：把 run_bots + BotGuard 复制进一个临时仓库骨架（$RepoRoot = 该临时目录），真跑 -WriteConfigOnly。
# 期望 rc=0、输出无 `BUILD BotSrv`、有 `WRITE_CONFIG_ONLY`（配置只写进临时目录，不碰真实 src/**）。
function New-TempRepoWith([string]$scriptPath, [string]$modulePath) {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("botguard_rc_" + $PID + "_" + ([guid]::NewGuid().ToString('N').Substring(0,8)))
    New-Item -ItemType Directory -Force -Path (Join-Path $root 'tools\botload') | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $root 'src\BotSrv\bin\Release') | Out-Null
    Copy-Item $scriptPath (Join-Path $root 'tools\botload\run_bots.ps1') -Force
    Copy-Item $modulePath (Join-Path $root 'tools\botload\BotGuard.psm1') -Force
    $script:tmpModDirs.Add($root)
    return (Join-Path $root 'tools\botload\run_bots.ps1')
}
$wcScript = New-TempRepoWith (Resolve-Path $RunBots).Path (Resolve-Path $GuardModule).Path
$eap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
try { $wcOut = & $script:GuardShell -NoProfile -ExecutionPolicy Bypass -File $wcScript -WriteConfigOnly 2>&1 | Out-String } finally { $ErrorActionPreference = $eap }
$wcRc = $LASTEXITCODE
if ($wcRc -eq 0 -and ($wcOut -notmatch 'BUILD BotSrv') -and ($wcOut -match 'WRITE_CONFIG_ONLY')) {
    Ok ("-WriteConfigOnly 真跑：无 BUILD、rc=0（行为与注释一致；配置写进临时目录）")
} else {
    Bad ("-WriteConfigOnly 行为与注释不符：rc=" + $wcRc + " out=" + $wcOut.Trim())
}
# 变异对照：去掉 -WriteConfigOnly 构建守卫 ⇒ 该路径又 BUILD BotSrv ⇒ 必红
$mutScript = New-TempRepoWith (Resolve-Path $RunBots).Path (Resolve-Path $GuardModule).Path
$mtxt = Get-Content -Raw $mutScript
$mtxt2 = $mtxt.Replace('if (-not $WriteConfigOnly -and (-not $SkipBuild -or -not (Test-Path $BotExe))) {', 'if (-not $SkipBuild -or -not (Test-Path $BotExe)) {')
if ($mtxt2 -ne $mtxt) {
    [System.IO.File]::WriteAllText($mutScript, $mtxt2, (New-Object System.Text.UTF8Encoding($false)))
    $eap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try { $mOut = & $script:GuardShell -NoProfile -ExecutionPolicy Bypass -File $mutScript -WriteConfigOnly 2>&1 | Out-String } finally { $ErrorActionPreference = $eap }
    if ($mOut -match 'BUILD BotSrv') { Ok "改坏必红：去掉 -WriteConfigOnly 构建守卫 ⇒ 该路径又会 BUILD BotSrv（牙⑧ 能咬住）" }
    else { Bad ("改坏必红失效（去守卫后仍未构建）: " + $mOut.Trim()) }
} else {
    Bad "变异目标未找到（run_bots 的 -WriteConfigOnly 构建守卫文本已变）"
}

# 清理本牙产生的所有临时物（模块副本 / 临时仓库 / probe 脚本）
foreach ($d in @($script:tmpModDirs)) { Remove-Item $d -Recurse -Force -ErrorAction SilentlyContinue }

# ---------- 结论 ----------
if ($script:fail -eq 0) {
    Write-Output ("GREEN: check_botguard " + $script:checks + "/" + $script:checks + " 全过（牙①②③④⑤ 及接线 + 牙⑥⑦⑧ B3 收口：机推天花板/对称有效性/WriteConfigOnly）")
    exit 0
}
Write-Output ("RED: check_botguard " + $script:fail + "/" + $script:checks + " 项未过")
exit 1
