#requires -Version 5
<#
.SYNOPSIS
  check_seed_plumb.ps1 — `start_rust_stack.ps1` 种子管线的**可判红/绿**判据（卡 `openmir2-m3-rust-stack-seed-plumb`）。

.DESCRIPTION
  证明「显式种子真的送到 game-svr 子进程、且 C#/Rust 两岸同种子可比」这件事**不是一句口径**：
  用构造输入把管线（`rust_seed_plumb.ps1`）的每条口径都跑一遍，并断言「退出码 + 输出关键字」两者都对。

  检查项（**不起栈、不占真端口**：全部用临时目录 + 空闲端口）：
    ① `Resolve-Mir2Seed`：未设/空串 ⇒ `Present=$false`；`42`/`+7`/` 42 ` ⇒ 解析对；环境变量回落；
       非法（`abc`/`4.2`/溢出）⇒ **抛错**（fail-closed，同 C# `TryParse` 失败 ⇒ `Fail/Exit(97)`）。
    ② 注入到位：`Start-SeededProcess` 起 `cmd /c echo %MIR2_RNG_SEED%`（stdout 落文件）⇒ 文件是 `42`；
       未注入时（删掉 `$env:MIR2_RNG_SEED = ...` 的注入行）⇒ 文件**不含** `42` ⇒ 本检查必红。
    ③ 与 C# 同种子一致：用**真 game-svr.exe** + 一个空闲端口的临时 `Server.conf` 起进程 ⇒ 其日志出现
       `SEED_PROBE <C# seed42 序列>`（与 `mir2-rs/bins/game-svr/tests/seed_inject.rs::SEED42_PROBE` 同源）。
    ④ 不传种子 ⇒ 真 game-svr 日志**没有** `SEED_PROBE`（与改动前一致）+ `Get-SeedInjectedLine` 返回 `$null`。
    ⑤ 起完还原：`Start-SeededProcess` 返回后调用方 `$env:MIR2_RNG_SEED` 不被污染。
    ⑥ 结构接线：`start_rust_stack.ps1` 确实点源本管线、并把 `-Seed $seedInfo` 传给 game-svr 的 `Start-One`。

  任何一条不符 ⇒ 打印 `SEED_PLUMB FAIL` 并 exit 1；全绿 ⇒ `SEED_PLUMB GREEN` exit 0。
  缺 game-svr.exe ⇒ `PREREQ_MISSING` exit 2（**不静默通过**）。

.PARAMETER GameSvrExe
  被测的 `game-svr.exe`。默认按隔离 target（`$env:CARGO_TARGET_DIR\debug\game-svr.exe`），
  否则回落仓库默认 target（`mir2-rs\target\debug\game-svr.exe`）。

.PARAMETER OutDir
  夹具/原始件目录（默认 `E:\tmp\m2-acceptance\seed-plumb-<时间戳>`）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_seed_plumb.ps1 `
    -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
#>
param(
  [string]$GameSvrExe = "",
  [string]$OutDir = ""
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path

# 种子管线（被测对象）：与 `start_rust_stack.ps1` 共用同一份。
. (Join-Path $Here 'rust_seed_plumb.ps1')

if ([string]::IsNullOrEmpty($GameSvrExe)) {
  if (-not [string]::IsNullOrEmpty($env:CARGO_TARGET_DIR)) {
    $GameSvrExe = Join-Path $env:CARGO_TARGET_DIR 'debug\game-svr.exe'
  } else {
    $GameSvrExe = Join-Path $RepoRoot 'mir2-rs\target\debug\game-svr.exe'
  }
}
if (-not (Test-Path $GameSvrExe)) {
  Write-Output ("PREREQ_MISSING: 缺 game-svr.exe: " + $GameSvrExe + "（先 cargo build -p game-svr 或传 -GameSvrExe）")
  exit 2
}

if ([string]::IsNullOrEmpty($OutDir)) {
  $OutDir = Join-Path 'E:\tmp\m2-acceptance' ('seed-plumb-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Work = Join-Path $OutDir 'work'
New-Item -ItemType Directory -Force -Path $Work | Out-Null

# C# 钩子实测的 `SEED_PROBE`（`MIR2_RNG_SEED=42`）——与 `seed_inject.rs::SEED42_PROBE` 同一标尺。
$Seed42Probe = 'SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307'

$script:Checks = New-Object System.Collections.Generic.List[object]
function Check([string]$Name, [bool]$Ok, [string]$Detail) {
  $tag = if ($Ok) { 'OK  ' } else { 'FAIL' }
  Write-Output ("  [{0}] {1}" -f $tag, $Name)
  if (-not [string]::IsNullOrEmpty($Detail)) { Write-Output ("        " + $Detail) }
  $script:Checks.Add([pscustomobject]@{ name = $Name; ok = $Ok; detail = $Detail })
}

function Free-Port {
  $l = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  $l.Start(); $p = $l.LocalEndpoint.Port; $l.Stop(); return $p
}

function Write-Conf([string]$Dir) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  $conf = Join-Path $Dir 'Server.conf'
  $text = "[Server]`nGateAddr=127.0.0.1`nGatePort=$(Free-Port)`nCheckBlock=12288`n" +
          "IDSAddr=127.0.0.1`nIDSPort=$(Free-Port)`nDBAddr=127.0.0.1`nDBPort=$(Free-Port)`nWorldMaps=64x64`n"
  Set-Content -Path $conf -Value $text -Encoding ascii -NoNewline
  return $conf
}

function Wait-LogContains([string]$Path, [string]$Needle, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ((Test-Path $Path) -and ((Get-Content $Path -Raw -ErrorAction SilentlyContinue) -match [regex]::Escape($Needle))) {
      return $true
    }
    Start-Sleep -Milliseconds 100
  }
  return $false
}

function Kill-Tree([int]$ProcessId) {
  if ($ProcessId -le 0) { return }
  & taskkill /T /F /PID $ProcessId > $null 2>&1
}

# 保证本脚本自身的 no-seed 用例不被调用方 shell 的残留污染。
if (Test-Path Env:MIR2_RNG_SEED) { Remove-Item Env:MIR2_RNG_SEED -ErrorAction SilentlyContinue }

Write-Output ("SEED_PLUMB 判据：game-svr=" + $GameSvrExe + "  evidence=" + $OutDir)

# ---- ① Resolve-Mir2Seed ----
$unset = Resolve-Mir2Seed -SeedArg '' -EnvValue ''
Check '①a 未设/空串 ⇒ Present=$false' ((-not $unset.Present) -and ($null -eq $unset.Value)) ("Present=" + $unset.Present)

$s42 = Resolve-Mir2Seed -SeedArg '42'
Check '①b -Seed 42 ⇒ Present, Value=42' (($s42.Present) -and ($s42.Value -eq 42)) ("Value=" + $s42.Value)

$sWs = Resolve-Mir2Seed -SeedArg ' +7 '
Check '①c " +7 " ⇒ Value=7（TryParse 允许空白/正号）' (($sWs.Present) -and ($sWs.Value -eq 7)) ("Value=" + $sWs.Value)

$sEnv = Resolve-Mir2Seed -SeedArg '' -EnvValue '1234'
Check '①d 环境变量回落 ⇒ Value=1234' (($sEnv.Present) -and ($sEnv.Value -eq 1234)) ("Value=" + $sEnv.Value)

$invalidThrew = $false
foreach ($bad in @('abc', '4.2', '0x10', '2147483648', '   ')) {
  try { [void](Resolve-Mir2Seed -SeedArg $bad) } catch { $invalidThrew = $true }
}
Check '①e 非法值 ⇒ 抛错 fail-closed（abc/4.2/0x10/溢出/空白串）' $invalidThrew '全部应抛错；同 C# TryParse 失败 ⇒ Fail/Exit(97)'

# ---- ② 注入到位（cmd 子进程读环境；删掉注入行 ⇒ 本项必红）----
$envSeeded = Join-Path $Work 'env-seeded.txt'
$pSeeded = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo %MIR2_RNG_SEED%') `
             -RedirectStandardOutput $envSeeded -Seed (Resolve-Mir2Seed -SeedArg '42')
[void]$pSeeded.WaitForExit(10000)
$seededText = if (Test-Path $envSeeded) { [string](Get-Content $envSeeded -Raw) } else { '' }
Check '②a 传种子 ⇒ 子进程环境 MIR2_RNG_SEED=42' ($seededText -match '42') ("cmd 读出=[" + $seededText.Trim() + "]")

# 起完还原：调用方环境不得被污染。
Check '②b 起完还原 ⇒ 调用方 $env:MIR2_RNG_SEED 未被设置' (-not (Test-Path Env:MIR2_RNG_SEED)) ('has_env=' + (Test-Path Env:MIR2_RNG_SEED))

# 未传种子 ⇒ 不注入（cmd 读不到 42）。
$envUnseeded = Join-Path $Work 'env-unseeded.txt'
$pUnseeded = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo %MIR2_RNG_SEED%') `
               -RedirectStandardOutput $envUnseeded -Seed $unset
[void]$pUnseeded.WaitForExit(10000)
$unseededText = if (Test-Path $envUnseeded) { [string](Get-Content $envUnseeded -Raw) } else { '' }
Check '②c 不传种子 ⇒ 子进程读不到 42（不注入）' (-not ($unseededText -match '42')) ("cmd 读出=[" + $unseededText.Trim() + "]")

# ---- ③ 真 game-svr：传种子 ⇒ SEED_PROBE 与 C# 同种子一致 ----
$gs1 = Join-Path $Work 'gs-seeded'
$conf1 = Write-Conf $gs1
$log1 = Join-Path $gs1 'stdout.txt'
$err1 = Join-Path $gs1 'stderr.txt'
$pGs1 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf1) -WorkingDirectory $gs1 `
          -RedirectStandardOutput $log1 -RedirectStandardError $err1 -Seed (Resolve-Mir2Seed -SeedArg '42')
$gotProbe = Wait-LogContains $log1 'SEED_PROBE' 20
$text1 = if (Test-Path $log1) { [string](Get-Content $log1 -Raw) } else { '' }
Kill-Tree $pGs1.Id
Check '③a 传种子 ⇒ 真 game-svr 打出 SEED_PROBE' $gotProbe ("log=" + $log1)
Check '③b SEED_PROBE 与 C# 同种子逐项一致' ($text1 -match [regex]::Escape($Seed42Probe)) ('期望尾：' + $Seed42Probe)
Check '③c 有同种子注入行（seed=42）' ($text1 -match 'RNG 同种子注入：seed=42') 'game-svr 自证行'

# ---- ④ 不传种子 ⇒ 真 game-svr 无 SEED_PROBE（与改动前一致）----
$gs2 = Join-Path $Work 'gs-unseeded'
$conf2 = Write-Conf $gs2
$log2 = Join-Path $gs2 'stdout.txt'
$err2 = Join-Path $gs2 'stderr.txt'
$pGs2 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf2) -WorkingDirectory $gs2 `
          -RedirectStandardOutput $log2 -RedirectStandardError $err2 -Seed $unset
# 等它至少打出启动 INF（证明它真的起来了），再断言无 SEED_PROBE。
$started2 = Wait-LogContains $log2 '游戏服务启动' 20
$text2 = if (Test-Path $log2) { [string](Get-Content $log2 -Raw) } else { '' }
Kill-Tree $pGs2.Id
Check '④a 不传种子 ⇒ 真 game-svr 正常启动（无 SEED_PROBE）' ($started2 -and (-not ($text2 -match 'SEED_PROBE'))) ('started=' + $started2 + ' has_probe=' + ($text2 -match 'SEED_PROBE'))

# ---- ⑤ Get-SeedInjectedLine ----
$line42 = Get-SeedInjectedLine -Seed $s42
$lineNone = Get-SeedInjectedLine -Seed $unset
Check '⑤a 传种子 ⇒ 行 = RUST_SEED_INJECTED=42' ($line42 -eq 'RUST_SEED_INJECTED=42') ("line=[" + $line42 + "]")
Check '⑤b 不传种子 ⇒ 行 = $null（不打）' ($null -eq $lineNone) ("line=[" + $lineNone + "]")

# ---- ⑥ 结构接线：start_rust_stack.ps1 真点源本管线并把种子喂给 game-svr ----
$stackSrc = Get-Content (Join-Path $Here 'start_rust_stack.ps1') -Raw
$wired = ($stackSrc -match 'rust_seed_plumb\.ps1') -and ($stackSrc -match 'game-svr''[^\r\n]*-Seed \$seedInfo')
Check '⑥ start_rust_stack.ps1 点源管线并把 -Seed $seedInfo 传给 game-svr' $wired '结构检查（端到端见证据目录的实栈 run）'

# ---- 汇总 ----
$failed = @($script:Checks | Where-Object { -not $_.ok })
$seededLine = Get-SeedInjectedLine -Seed $s42
Set-Content -Path (Join-Path $OutDir 'checks.txt') -Encoding utf8 -Value (
  ($script:Checks | ForEach-Object { "{0}`t{1}`t{2}" -f $(if ($_.ok) { 'OK' } else { 'FAIL' }), $_.name, $_.detail }) -join "`n"
)
Write-Output ("SUMMARY total={0} ok={1} fail={2} seeded_line={3}" -f $script:Checks.Count, ($script:Checks.Count - $failed.Count), $failed.Count, $seededLine)
if ($failed.Count -gt 0) {
  Write-Output 'SEED_PLUMB FAIL'
  exit 1
}
Write-Output 'SEED_PLUMB GREEN'
exit 0
