#requires -Version 5
<#
.SYNOPSIS
  check_rng_log_plumb.ps1 — `start_rust_stack.ps1` 的 `MIR2_RNG_LOG` 逐次 rng 流水管线的
  **可判红/绿**判据（卡 `openmir2-m2-rust-rng-log-plumb`）。

.DESCRIPTION
  两侧 site/order 账本 diff（`attack_rng_ledger.py` / `rng_ledger_diff.py`）要 C# 与 Rust 两侧
  的同名逐次 rng 流水：C# 半由 rng-hook（`mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`）读
  `MIR2_RNG_LOG` 落 `rng-stream.tsv`；Rust 半由 `crates/shared/src/rng_trace.rs` 读同一个变量
  （`TRACE_ENV`，由 `RandomNumber::with_seed` → `enable_from_env()` 开启）。但启动器此前**没有这条
  管线** ⇒ 只有 operator 手工在起栈 shell 里设过变量才有 Rust 流水，否则账本拿不到 Rust 半。
  本脚本用构造输入把这条管线（`rust_seed_plumb.ps1`）的每条口径都跑一遍。

  检查项（**不起完整栈、不占真端口**：全部用临时目录 + 空闲端口起单个 game-svr，共 15 项）：
    ① `Resolve-Mir2RngLog`：未设/空串 ⇒ `Present=$false`；合法路径 ⇒ `Present` 且 `Path` 原样；
       环境变量回落；父目录不存在 ⇒ **抛错**（fail-closed）。
    ② 注入到位：`Start-SeededProcess -RngLog ...` 起 `cmd /c echo %MIR2_RNG_LOG%`（stdout 落文件）
       ⇒ 文件是该路径；不注入时（删掉 `$env:MIR2_RNG_LOG = ...` 的注入行）⇒ 文件不含路径 ⇒ 本项必红。
    ③ 真 game-svr 传 `-Seed 42` + `-RngLog` ⇒ 在该路径建出流水，且含 `INJECT_OK 42` 与 `SEED_PROBE`
       （rng_trace 要求 MIR2_RNG_SEED 同设，否则不落流水）。
    ④ 真 game-svr 只传种子、不传路径 ⇒ 正常启动且**不建**流水文件（与改动前一致）。
    ⑤ 起完还原：`Start-SeededProcess` 返回后调用方 `$env:MIR2_RNG_LOG` 不被污染；`Get-RngLogInjectedLine` 形态。
    ⑥ 结构接线：`start_rust_stack.ps1` 确实点源本管线、把 `-RngLog $rngLogInfo` 传给 game-svr，
       管线文件里确有 `$env:MIR2_RNG_LOG` 注入行；且**交叉 fail-closed 守卫**（RngLog 无 Seed）存在。

  任何一条不符 ⇒ 打印 `RNG_LOG_PLUMB FAIL` 并 exit 1；全绿 ⇒ `RNG_LOG_PLUMB GREEN` exit 0。
  缺 game-svr.exe ⇒ `PREREQ_MISSING` exit 2（**不静默通过**）。

.PARAMETER GameSvrExe
  被测的 `game-svr.exe`。默认按隔离 target（`$env:CARGO_TARGET_DIR\debug\game-svr.exe`），
  否则回落仓库默认 target（`mir2-rs\target\debug\game-svr.exe`）。

.PARAMETER OutDir
  夹具/原始件目录（默认 `E:\tmp\m2-acceptance\rng-log-plumb-<时间戳>`）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_rng_log_plumb.ps1 `
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
  $OutDir = Join-Path 'E:\tmp\m2-acceptance' ('rng-log-plumb-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Work = Join-Path $OutDir 'work'
New-Item -ItemType Directory -Force -Path $Work | Out-Null

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

function Wait-FileMatch([string]$Path, [string]$Pattern, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ((Test-Path $Path) -and ((Get-Content $Path -Raw -ErrorAction SilentlyContinue) -match $Pattern)) {
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

# 保证本脚本自身的 no-rng-log 用例不被调用方 shell 的残留污染。
if (Test-Path Env:MIR2_RNG_LOG) { Remove-Item Env:MIR2_RNG_LOG -ErrorAction SilentlyContinue }
if (Test-Path Env:MIR2_RNG_SEED) { Remove-Item Env:MIR2_RNG_SEED -ErrorAction SilentlyContinue }

Write-Output ("RNG_LOG_PLUMB 判据：game-svr=" + $GameSvrExe + "  evidence=" + $OutDir)

# ---- ① Resolve-Mir2RngLog ----
$unset = Resolve-Mir2RngLog -RngLogArg '' -EnvValue ''
Check '①a 未设/空串 ⇒ Present=$false' ((-not $unset.Present) -and ($unset.Path -eq '')) ("Present=" + $unset.Present)

$validPath = Join-Path $Work 'rng-stream.tsv'
$resolved = Resolve-Mir2RngLog -RngLogArg $validPath
Check '①b 合法路径 ⇒ Present 且 Path 原样' (($resolved.Present) -and ($resolved.Path -eq $validPath)) ("Path=" + $resolved.Path)

$envFallbackPath = Join-Path $Work 'env-fallback.tsv'
$envResolved = Resolve-Mir2RngLog -RngLogArg '' -EnvValue $envFallbackPath
Check '①c 环境变量回落 ⇒ 采用 env 值' (($envResolved.Present) -and ($envResolved.Path -eq $envFallbackPath)) ("Path=" + $envResolved.Path)

$invalidThrew = $false
foreach ($bad in @((Join-Path $Work 'no-such-dir-xyz\rng.tsv'))) {
  try { [void](Resolve-Mir2RngLog -RngLogArg $bad) } catch { $invalidThrew = $true }
}
Check '①d 父目录不存在 ⇒ 抛错 fail-closed' $invalidThrew '同 `-Seed` 非法值：调用方判红并 exit 1，不静默起一个不落流水的栈'

# ---- ② 注入到位（cmd 子进程读环境；删掉注入行 ⇒ 本项必红）----
$envOutSeeded = Join-Path $Work 'env-seeded.txt'
$pEnvSeeded = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo %MIR2_RNG_LOG%') `
                -RedirectStandardOutput $envOutSeeded -RngLog (Resolve-Mir2RngLog -RngLogArg $validPath)
[void]$pEnvSeeded.WaitForExit(10000)
$seededText = if (Test-Path $envOutSeeded) { [string](Get-Content $envOutSeeded -Raw) } else { '' }
Check '②a 传路径 ⇒ 子进程环境 MIR2_RNG_LOG=该路径' ($seededText.Trim() -eq $validPath) ("cmd 读出=[" + $seededText.Trim() + "]")

Check '②b 起完还原 ⇒ 调用方 $env:MIR2_RNG_LOG 未被设置' (-not (Test-Path Env:MIR2_RNG_LOG)) ('has_env=' + (Test-Path Env:MIR2_RNG_LOG))

$envOutUnseeded = Join-Path $Work 'env-unseeded.txt'
$pEnvUnseeded = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo %MIR2_RNG_LOG%') `
                  -RedirectStandardOutput $envOutUnseeded -RngLog $unset
[void]$pEnvUnseeded.WaitForExit(10000)
$unseededText = if (Test-Path $envOutUnseeded) { [string](Get-Content $envOutUnseeded -Raw) } else { '' }
Check '②c 不传路径 ⇒ 子进程读不到该路径（不注入）' (-not ($unseededText -match [regex]::Escape($validPath))) ("cmd 读出=[" + $unseededText.Trim() + "]")

# ---- ③ 真 game-svr：传种子 + 路径 ⇒ 建出流水且含 INJECT_OK/SEED_PROBE（rng_trace 要求两者同设）----
$gs1 = Join-Path $Work 'gs-rng-log'
$traceFile = Join-Path $gs1 'rng-stream.tsv'
$conf1 = Write-Conf $gs1
$log1 = Join-Path $gs1 'stdout.txt'
$err1 = Join-Path $gs1 'stderr.txt'
$pGs1 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf1) -WorkingDirectory $gs1 `
          -RedirectStandardOutput $log1 -RedirectStandardError $err1 `
          -Seed (Resolve-Mir2Seed -SeedArg '42') -RngLog (Resolve-Mir2RngLog -RngLogArg $traceFile)
$gotTrace = Wait-FileMatch $traceFile 'INJECT_OK\s+42' 25
$text1 = if (Test-Path $traceFile) { [string](Get-Content $traceFile -Raw) } else { '' }
Kill-Tree $pGs1.Id
Check '③a 传种子+路径 ⇒ 真 game-svr 在该路径建出 rng 流水' (Test-Path -LiteralPath $traceFile) ("file=" + $traceFile)
Check '③b 流水含 INJECT_OK 42 + SEED_PROBE（同种子自证）' ($gotTrace -and ($text1 -match 'SEED_PROBE')) ('has_inject_ok=' + $gotTrace + ' has_probe=' + ($text1 -match 'SEED_PROBE'))

# ---- ④ 只传种子、不传路径 ⇒ 正常启动、不建流水（与改动前一致）----
$gs2 = Join-Path $Work 'gs-no-rng-log'
$conf2 = Write-Conf $gs2
$log2 = Join-Path $gs2 'stdout.txt'
$err2 = Join-Path $gs2 'stderr.txt'
$leakFile = Join-Path $gs2 'rng-stream.tsv'
$pGs2 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf2) -WorkingDirectory $gs2 `
          -RedirectStandardOutput $log2 -RedirectStandardError $err2 `
          -Seed (Resolve-Mir2Seed -SeedArg '42') -RngLog $unset
$started2 = Wait-LogContains $log2 '游戏服务启动' 25
Kill-Tree $pGs2.Id
Check '④a 不传路径 ⇒ 真 game-svr 正常启动且不建流水文件' ($started2 -and (-not (Test-Path -LiteralPath $leakFile))) ('started=' + $started2 + ' leaked=' + (Test-Path -LiteralPath $leakFile))

# ---- ⑤ Get-RngLogInjectedLine ----
$lineOn = Get-RngLogInjectedLine -RngLog $resolved
$lineOff = Get-RngLogInjectedLine -RngLog $unset
Check '⑤a 传路径 ⇒ 行 = RUST_RNG_LOG_INJECTED=<path>' ($lineOn -eq ("RUST_RNG_LOG_INJECTED={0}" -f $validPath)) ("line=[" + $lineOn + "]")
Check '⑤b 不传路径 ⇒ 行 = $null（不打）' ($null -eq $lineOff) ("line=[" + $lineOff + "]")

# ---- ⑥ 结构接线：start_rust_stack.ps1 真点源本管线并把路径喂给 game-svr；管线文件真有注入行 + 交叉守卫 ----
$stackSrc = Get-Content (Join-Path $Here 'start_rust_stack.ps1') -Raw
$wired = ($stackSrc -match 'rust_seed_plumb\.ps1') -and ($stackSrc -match ("game-svr'[^\r\n]*-RngLog " + [regex]::Escape('$rngLogInfo')))
Check '⑥a start_rust_stack.ps1 点源管线并把 -RngLog $rngLogInfo 传给 game-svr' $wired '结构检查（端到端见证据目录的实栈 run）'
$plumbSrc = Get-Content (Join-Path $Here 'rust_seed_plumb.ps1') -Raw
$hasInjectLine = $plumbSrc -match '\$env:MIR2_RNG_LOG = \[string\]\$RngLog\.Path'
Check '⑥b 管线文件含 $env:MIR2_RNG_LOG 注入行' $hasInjectLine '删掉该行 ⇒ ②a/③a/③b 必红'
$seed42 = Resolve-Mir2Seed -SeedArg '42'
$guardTt = ((Assert-Mir2RngLogSeed -RngLog $unset -Seed $unset) -eq $true) -and
           ((Assert-Mir2RngLogSeed -RngLog $resolved -Seed $seed42) -eq $true) -and
           ((Assert-Mir2RngLogSeed -RngLog $resolved -Seed $unset) -eq $false) -and
           ((Assert-Mir2RngLogSeed -RngLog $resolved -Seed $null) -eq $false)
$callsGuard = $stackSrc -match 'Assert-Mir2RngLogSeed'
Check '⑥c 交叉守卫真值表（RngLog 有 Seed/无 Seed/未设）+ start_rust_stack 调用本例' ($guardTt -and $callsGuard) ('tt=' + $guardTt + ' called=' + $callsGuard + '；rng_trace 无种子不落流水 ⇒ RngLog 有而 Seed 无必拦')

# ---- 汇总 ----
$failed = @($script:Checks | Where-Object { -not $_.ok })
Set-Content -Path (Join-Path $OutDir 'checks.txt') -Encoding utf8 -Value (
  ($script:Checks | ForEach-Object { "{0}`t{1}`t{2}" -f $(if ($_.ok) { 'OK' } else { 'FAIL' }), $_.name, $_.detail }) -join "`n"
)
Write-Output ("SUMMARY total={0} ok={1} fail={2}" -f $script:Checks.Count, ($script:Checks.Count - $failed.Count), $failed.Count)
if ($failed.Count -gt 0) {
  Write-Output 'RNG_LOG_PLUMB FAIL'
  exit 1
}
Write-Output 'RNG_LOG_PLUMB GREEN'
exit 0
