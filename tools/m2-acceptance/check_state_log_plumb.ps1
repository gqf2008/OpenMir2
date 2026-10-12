#requires -Version 5
<#
.SYNOPSIS
  check_state_log_plumb.ps1 — `start_rust_stack.ps1` 的 `MIR2_RNG_STATE_LOG` 状态 dump 管线的
  **可判红/绿**判据（卡 `openmir2-m2-rust-state-log-plumb`）。

.DESCRIPTION
  夹具的目标可用性门（`mir2-rs/tests/parity/m3/server_state_gate.py`）要**两侧都强制**：C# 半由
  `start-csharp-hook.ps1 -StateLog` 写 `MIR2_RNG_STATE_LOG`；Rust 半由 `bins/game-svr` 读同一个变量
  （卡 `openmir2-m2-rust-server-state-dump`）。但启动器此前**没有这条管线** ⇒ 只有 operator 手工在起栈
  shell 里设过变量才落盘，否则 `run_attack_pair.ps1 -Side rust` 拿不到 Rust dump、门只保护 C# 一半。
  本脚本用构造输入把这条管线（`rust_seed_plumb.ps1`）的每条口径都跑一遍。

  检查项（**不起完整栈、不占真端口**：全部用临时目录 + 空闲端口起单个 game-svr）：
    ① `Resolve-Mir2StateLog`：未设/空串 ⇒ `Present=$false`；合法路径 ⇒ `Present` 且 `Path` 原样；
       环境变量回落；父目录不存在 ⇒ **抛错**（fail-closed）。
    ② 注入到位：`Start-SeededProcess -StateLog ...` 起 `cmd /c echo %MIR2_RNG_STATE_LOG%`（stdout 落文件）
       ⇒ 文件是该路径；不注入时（删掉 `$env:MIR2_RNG_STATE_LOG = ...` 的注入行）⇒ 文件不含路径 ⇒ 本项必红。
    ③ 与 C# 同口径：用**真 game-svr.exe** + 空闲端口的临时 `Server.conf` 起进程 ⇒ 其日志出现
       「服务端逐击状态 dump 已开启」且**该路径被建出文件**（`File::create`）。
    ④ 不传路径 ⇒ 真 game-svr 正常启动，日志**没有**该行、也不建文件（与改动前一致）。
    ⑤ 起完还原：`Start-SeededProcess` 返回后调用方 `$env:MIR2_RNG_STATE_LOG` 不被污染。
    ⑥ 结构接线：`start_rust_stack.ps1` 确实点源本管线、把 `-StateLog $stateLogInfo` 传给 game-svr；
       管线文件里确有 `$env:MIR2_RNG_STATE_LOG` 注入行。

  任何一条不符 ⇒ 打印 `STATE_LOG_PLUMB FAIL` 并 exit 1；全绿 ⇒ `STATE_LOG_PLUMB GREEN` exit 0。
  缺 game-svr.exe ⇒ `PREREQ_MISSING` exit 2（**不静默通过**）。

.PARAMETER GameSvrExe
  被测的 `game-svr.exe`。默认按隔离 target（`$env:CARGO_TARGET_DIR\debug\game-svr.exe`），
  否则回落仓库默认 target（`mir2-rs\target\debug\game-svr.exe`）。

.PARAMETER OutDir
  夹具/原始件目录（默认 `E:\tmp\m2-acceptance\state-log-plumb-<时间戳>`）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_state_log_plumb.ps1 `
    -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
#>
param(
  [string]$GameSvrExe = "",
  [string]$OutDir = ""
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path

# 管线（被测对象）：与 `start_rust_stack.ps1` 共用同一份。
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
  $OutDir = Join-Path 'E:\tmp\m2-acceptance' ('state-log-plumb-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Work = Join-Path $OutDir 'work'
New-Item -ItemType Directory -Force -Path $Work | Out-Null

# game-svr 开启 dump 的自证行（`bins/game-svr/src/main.rs`：`state_log.enabled()` 时唯一一句 INF）。
$EnabledMarker = '服务端逐击状态 dump 已开启'

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

# 保证本脚本自身的 no-state-log 用例不被调用方 shell 的残留污染。
if (Test-Path Env:MIR2_RNG_STATE_LOG) { Remove-Item Env:MIR2_RNG_STATE_LOG -ErrorAction SilentlyContinue }

Write-Output ("STATE_LOG_PLUMB 判据：game-svr=" + $GameSvrExe + "  evidence=" + $OutDir)

# ---- ① Resolve-Mir2StateLog ----
$unset = Resolve-Mir2StateLog -StateLogArg '' -EnvValue ''
Check '①a 未设/空串 ⇒ Present=$false' ((-not $unset.Present) -and ($unset.Path -eq '')) ("Present=" + $unset.Present)

$validPath = Join-Path $Work 'rng-server-state.tsv'
$resolved = Resolve-Mir2StateLog -StateLogArg $validPath
Check '①b 合法路径 ⇒ Present 且 Path 原样' (($resolved.Present) -and ($resolved.Path -eq $validPath)) ("Path=" + $resolved.Path)

$envFallbackPath = Join-Path $Work 'env-fallback.tsv'
$envResolved = Resolve-Mir2StateLog -StateLogArg '' -EnvValue $envFallbackPath
# 环境回落只看「显式空 + env 非空 ⇒ 采用 env」这一条（父目录存在由 ①d 单独判）。
Check '①c 环境变量回落 ⇒ 采用 env 值' (($envResolved.Present) -and ($envResolved.Path -eq $envFallbackPath)) ("Path=" + $envResolved.Path)

$invalidThrew = $false
foreach ($bad in @((Join-Path $Work 'no-such-dir-xyz\rng.tsv'))) {
  try { [void](Resolve-Mir2StateLog -StateLogArg $bad) } catch { $invalidThrew = $true }
}
Check '①d 父目录不存在 ⇒ 抛错 fail-closed' $invalidThrew '同 `-Seed` 非法值：调用方判红并 exit 1，不静默起一个不落 dump 的栈'

# ---- ② 注入到位（cmd 子进程读环境；删掉注入行 ⇒ 本项必红）----
$envOutSeeded = Join-Path $Work 'env-seeded.txt'
$pEnvSeeded = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo %MIR2_RNG_STATE_LOG%') `
                -RedirectStandardOutput $envOutSeeded -StateLog (Resolve-Mir2StateLog -StateLogArg $validPath)
[void]$pEnvSeeded.WaitForExit(10000)
$seededText = if (Test-Path $envOutSeeded) { [string](Get-Content $envOutSeeded -Raw) } else { '' }
Check '②a 传路径 ⇒ 子进程环境 MIR2_RNG_STATE_LOG=该路径' ($seededText.Trim() -eq $validPath) ("cmd 读出=[" + $seededText.Trim() + "]")

Check '②b 起完还原 ⇒ 调用方 $env:MIR2_RNG_STATE_LOG 未被设置' (-not (Test-Path Env:MIR2_RNG_STATE_LOG)) ('has_env=' + (Test-Path Env:MIR2_RNG_STATE_LOG))

$envOutUnseeded = Join-Path $Work 'env-unseeded.txt'
$pEnvUnseeded = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo %MIR2_RNG_STATE_LOG%') `
                  -RedirectStandardOutput $envOutUnseeded -StateLog $unset
[void]$pEnvUnseeded.WaitForExit(10000)
$unseededText = if (Test-Path $envOutUnseeded) { [string](Get-Content $envOutUnseeded -Raw) } else { '' }
Check '②c 不传路径 ⇒ 子进程读不到该路径（不注入）' (-not ($unseededText -match [regex]::Escape($validPath))) ("cmd 读出=[" + $unseededText.Trim() + "]")

# ---- ③ 真 game-svr：传路径 ⇒ 打自证行 + 建出 dump 文件 ----
$gs1 = Join-Path $Work 'gs-state-log'
$stateFile = Join-Path $gs1 'rng-server-state.tsv'
$conf1 = Write-Conf $gs1
$log1 = Join-Path $gs1 'stdout.txt'
$err1 = Join-Path $gs1 'stderr.txt'
$pGs1 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf1) -WorkingDirectory $gs1 `
          -RedirectStandardOutput $log1 -RedirectStandardError $err1 -StateLog (Resolve-Mir2StateLog -StateLogArg $stateFile)
$gotMarker = Wait-LogContains $log1 $EnabledMarker 20
$text1 = if (Test-Path $log1) { [string](Get-Content $log1 -Raw) } else { '' }
$created = Test-Path -LiteralPath $stateFile
Kill-Tree $pGs1.Id
Check '③a 传路径 ⇒ 真 game-svr 打出「服务端逐击状态 dump 已开启」' $gotMarker ("log=" + $log1)
Check '③b 传路径 ⇒ game-svr 在该路径建出 dump 文件（File::create）' $created ("file=" + $stateFile)

# ---- ④ 不传路径 ⇒ 真 game-svr 正常启动，无自证行、不建文件（与改动前一致）----
$gs2 = Join-Path $Work 'gs-no-state-log'
$conf2 = Write-Conf $gs2
$log2 = Join-Path $gs2 'stdout.txt'
$err2 = Join-Path $gs2 'stderr.txt'
$leakFile = Join-Path $gs2 'rng-server-state.tsv'
$pGs2 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf2) -WorkingDirectory $gs2 `
          -RedirectStandardOutput $log2 -RedirectStandardError $err2 -StateLog $unset
# 等它至少打出启动 INF（证明它真的起来了），再断言无自证行。
$started2 = Wait-LogContains $log2 '游戏服务启动' 20
$text2 = if (Test-Path $log2) { [string](Get-Content $log2 -Raw) } else { '' }
Kill-Tree $pGs2.Id
Check '④a 不传路径 ⇒ 真 game-svr 正常启动且无自证行' ($started2 -and (-not ($text2 -match [regex]::Escape($EnabledMarker)))) ('started=' + $started2 + ' has_marker=' + ($text2 -match [regex]::Escape($EnabledMarker)))
Check '④b 不传路径 ⇒ 不建 dump 文件' (-not (Test-Path -LiteralPath $leakFile)) ("file=" + $leakFile)

# ---- ⑤ Get-StateLogInjectedLine ----
$lineOn = Get-StateLogInjectedLine -StateLog $resolved
$lineOff = Get-StateLogInjectedLine -StateLog $unset
Check '⑤a 传路径 ⇒ 行 = RUST_STATE_LOG_INJECTED=<path>' ($lineOn -eq ("RUST_STATE_LOG_INJECTED={0}" -f $validPath)) ("line=[" + $lineOn + "]")
Check '⑤b 不传路径 ⇒ 行 = $null（不打）' ($null -eq $lineOff) ("line=[" + $lineOff + "]")

# ---- ⑥ 结构接线：start_rust_stack.ps1 真点源本管线并把路径喂给 game-svr；管线文件真有注入行 ----
$stackSrc = Get-Content (Join-Path $Here 'start_rust_stack.ps1') -Raw
$wired = ($stackSrc -match 'rust_seed_plumb\.ps1') -and ($stackSrc -match ("game-svr'[^\r\n]*-StateLog " + [regex]::Escape('$stateLogInfo')))
Check '⑥a start_rust_stack.ps1 点源管线并把 -StateLog $stateLogInfo 传给 game-svr' $wired '结构检查（端到端见证据目录的实栈 run）'
$plumbSrc = Get-Content (Join-Path $Here 'rust_seed_plumb.ps1') -Raw
$hasInjectLine = $plumbSrc -match '\$env:MIR2_RNG_STATE_LOG = \[string\]\$StateLog\.Path'
Check '⑥b 管线文件含 $env:MIR2_RNG_STATE_LOG 注入行' $hasInjectLine '删掉该行 ⇒ ②a/③a/③b 必红'

# ---- 汇总 ----
$failed = @($script:Checks | Where-Object { -not $_.ok })
Set-Content -Path (Join-Path $OutDir 'checks.txt') -Encoding utf8 -Value (
  ($script:Checks | ForEach-Object { "{0}`t{1}`t{2}" -f $(if ($_.ok) { 'OK' } else { 'FAIL' }), $_.name, $_.detail }) -join "`n"
)
Write-Output ("SUMMARY total={0} ok={1} fail={2}" -f $script:Checks.Count, ($script:Checks.Count - $failed.Count), $failed.Count)
if ($failed.Count -gt 0) {
  Write-Output 'STATE_LOG_PLUMB FAIL'
  exit 1
}
Write-Output 'STATE_LOG_PLUMB GREEN'
exit 0
