#requires -Version 5
<#
.SYNOPSIS
  check_rng_trace_purity.ps1 — Rust `MIR2_RNG_LOG` 逐次 rng 流水的**纯度**判据：
  game-svr 启动期的 `SEED_PROBE` **自证抽数**不得混进逐次流水。

.DESCRIPTION
  两侧 `site/order` 账本 diff（`attack_rng_ledger.py` / `rng_ledger_diff.py`）要求 Rust 半的
  逐次流水里**只有游戏消费者**。`bins/game-svr/src/main.rs` 的 `sim_loop` 在注入种子后会打一条
  `SEED_PROBE` 自证行；若那 8 项参照序列经 [`mir2_shared::rng::RandomNumber`]（被 trace 的入口）
  取数，就会在流水里落下 8 条 `bins/game-svr/src/main.rs` 记录——它们**不是**游戏消费者、**C# 侧
  不存在**，会把两侧账本的第一处分歧钉到 index 0（真机实测：Rust 独有站点 `main.rs:2856` × 8，
  见 `mir2-rs/tests/parity/m3/evidence-rng-ledger-diff-20261012/32-diff-full.txt`）。

  口径：`SEED_PROBE` 参照序列必须走**独立实例**（`DotNetRandom::new(seed)`，不消耗游戏流、不经
  `RandomNumber`）——与 C# 钩子（`rng-hook/StartupHook.cs` 的独立 `reference` 实例）以及
  `crates/shared/src/rng_trace.rs::enable` 写进流水头部的 `SEED_PROBE` 完全同口径。

  检查项（**不起完整栈、不占真端口**：临时目录 + 空闲端口起单个 game-svr）：
    ① 真 game-svr 传 `MIR2_RNG_SEED` + `MIR2_RNG_LOG` ⇒ 在指定路径建出流水，头部 =
       `1<TAB>INJECT_OK<TAB><seed><TAB>0` + `SEED_PROBE <8 项>`。
    ② 流水头部 `SEED_PROBE` 与 C# 钩子 seed=42 的实测序列逐项一致（自我标尺）。
    ③ **纯度门（核心）**：流水的「记录行」（`^<seq><TAB>Next`）里**没有任何** `bins\game-svr\src\main.rs`
       站点 ⇒ 启动期自证没有混进游戏流水（改坏：把 `sim_loop` 的参照序列改回经 `RandomNumber`
       取数 ⇒ 出现 8 条 `main.rs` 记录 ⇒ 本项必红）。
    ④ 两处独立自证互相一致：stdout 的 `SEED_PROBE` 与流水头部的 `SEED_PROBE` 逐项相同
       （独立实例 + trace 头部实例，都与生产注入的随机源同序列）。
    ⑤ 边界：未传种子/路径 ⇒ 不建流水（与改动前逐字节一致）。

  任何一条不符 ⇒ 打印 `RNG_TRACE_PURITY FAIL` 并 exit 1；全绿 ⇒ `RNG_TRACE_PURITY GREEN` exit 0。
  缺 game-svr.exe ⇒ `PREREQ_MISSING` exit 2（**不静默通过**）。

.PARAMETER GameSvrExe
  被测的 `game-svr.exe`。默认按隔离 target（`$env:CARGO_TARGET_DIR\debug\game-svr.exe`），
  否则回落仓库默认 target（`mir2-rs\target\debug\game-svr.exe`）。

.PARAMETER OutDir
  夹具/原始件目录（默认 `E:\tmp\m2-acceptance\rng-trace-purity-<时间戳>`）。

.PARAMETER Seed
  注入的 `MIR2_RNG_SEED`（默认 42，与 C# 钩子实测标尺一致）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_rng_trace_purity.ps1 `
    -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
#>
param(
  [string]$GameSvrExe = "",
  [string]$OutDir = "",
  [int]$Seed = 42
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path

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
  $OutDir = Join-Path 'E:\tmp\m2-acceptance' ('rng-trace-purity-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Work = Join-Path $OutDir 'work'
New-Item -ItemType Directory -Force -Path $Work | Out-Null

# C# 钩子实测的 seed=42 `SEED_PROBE`（= `mir2-rs/bins/game-svr/tests/seed_inject.rs::SEED42_PROBE`
# = `check_seed_plumb.ps1::$Seed42Probe`）。三处同一标尺，改动任一即红。
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

# 保证本脚本自身的用例不被调用方 shell 的残留污染。
if (Test-Path Env:MIR2_RNG_LOG) { Remove-Item Env:MIR2_RNG_LOG -ErrorAction SilentlyContinue }
if (Test-Path Env:MIR2_RNG_SEED) { Remove-Item Env:MIR2_RNG_SEED -ErrorAction SilentlyContinue }

Write-Output ("RNG_TRACE_PURITY 判据：game-svr=" + $GameSvrExe + "  evidence=" + $OutDir + "  seed=" + $Seed)

# ---- ① + ② + ③ + ④ 真 game-svr：传种子 + 路径 ⇒ 建流水 + 纯度门 ----
$gs1 = Join-Path $Work 'gs-rng-trace'
$traceFile = Join-Path $gs1 'rng-stream.tsv'
$conf1 = Write-Conf $gs1
$log1 = Join-Path $gs1 'stdout.txt'
$err1 = Join-Path $gs1 'stderr.txt'

# 直接注入子进程环境（本脚本不依赖 `start_rust_stack.ps1` 的 -RngLog 管线，保证在 master 基线可跑）；
# `Start-Process` 在创建子进程时快照环境块 ⇒ 起完立即还原，不污染调用方 shell。
$hadLog = Test-Path Env:MIR2_RNG_LOG
$prevLog = if ($hadLog) { $env:MIR2_RNG_LOG } else { $null }
$hadSeed = Test-Path Env:MIR2_RNG_SEED
$prevSeed = if ($hadSeed) { $env:MIR2_RNG_SEED } else { $null }
try {
  $env:MIR2_RNG_SEED = [string]$Seed
  $env:MIR2_RNG_LOG = $traceFile
  $pGs1 = Start-Process -FilePath $GameSvrExe -ArgumentList @($conf1) -WorkingDirectory $gs1 `
            -WindowStyle Hidden -PassThru -RedirectStandardOutput $log1 -RedirectStandardError $err1
} finally {
  if ($hadLog) { $env:MIR2_RNG_LOG = $prevLog } else { Remove-Item Env:MIR2_RNG_LOG -ErrorAction SilentlyContinue }
  if ($hadSeed) { $env:MIR2_RNG_SEED = $prevSeed } else { Remove-Item Env:MIR2_RNG_SEED -ErrorAction SilentlyContinue }
}
# 自证行在 `sim_loop` 注入后立刻打 ⇒ 等到它就说明那 8 项（若有）已经取数/已落流水。
$gotStdoutProbe = Wait-LogContains $log1 'SEED_PROBE' 25
Start-Sleep -Milliseconds 500
Kill-Tree $pGs1.Id
Start-Sleep -Milliseconds 300

$traceText = if (Test-Path -LiteralPath $traceFile) { [string](Get-Content -LiteralPath $traceFile -Raw) } else { '' }
$traceLines = if (Test-Path -LiteralPath $traceFile) { @(Get-Content -LiteralPath $traceFile) } else { @() }
$stdoutText = if (Test-Path $log1) { [string](Get-Content $log1 -Raw) } else { '' }

$line1 = if ($traceLines.Count -ge 1) { $traceLines[0] } else { '' }
$line2 = if ($traceLines.Count -ge 2) { $traceLines[1] } else { '' }
Check '①a 传种子+路径 ⇒ 真 game-svr 在指定路径建出 rng 流水' (Test-Path -LiteralPath $traceFile) ("file=" + $traceFile)
$headInject = ($traceLines.Count -ge 1 -and $line1 -match ("^1\tINJECT_OK\t" + [regex]::Escape([string]$Seed) + "\t0$"))
Check '①b 流水头部第 1 行 = INJECT_OK <seed> 0' $headInject ("line1=[" + $line1 + "]")
$headProbe = ($traceLines.Count -ge 2 -and $line2 -match '^SEED_PROBE ')
Check '①c 流水头部第 2 行 = SEED_PROBE <8 项>' $headProbe ("line2=[" + $line2 + "]")

$headProbeVals = if ($headProbe) { ($line2 -replace '^SEED_PROBE\s+', '') } else { '' }
Check '② 流水头部 SEED_PROBE 与 C# 钩子 seed=42 序列逐项一致' ($headProbeVals.Trim() -eq ($Seed42Probe -replace '^SEED_PROBE\s+', '')) ("header=[" + $headProbeVals.Trim() + "]")

# ③ 核心纯度门：记录行（`^<seq><TAB>Next`）里不得出现 bins/game-svr/src/main.rs 站点。
$records = @($traceLines | Where-Object { $_ -match '^\d+\tNext' })
$mainSites = @($records | Where-Object { $_ -match 'main\.rs' })
$mainDetail = "records=" + $records.Count + " main_records=" + $mainSites.Count
if ($mainSites.Count -gt 0) { $mainDetail = $mainDetail + " first=[" + $mainSites[0] + "]" }
Check '③ 流水记录行不含启动期自证站点 bins\game-svr\src\main.rs' ($mainSites.Count -eq 0) $mainDetail
Check '③b `SEED_PROBE` 自证已发生（门的前提：那 8 项确实取过数）' $gotStdoutProbe ('stdout_seed_probe=' + $gotStdoutProbe)

# ④ 两处独立自证互相一致：stdout 的 SEED_PROBE vs 流水头部。
$stdoutProbeMatch = [regex]::Match($stdoutText, 'SEED_PROBE\s+([0-9\- ]+)')
$stdoutProbeVals = if ($stdoutProbeMatch.Success) { ($stdoutProbeMatch.Groups[1].Value.Trim()) } else { '' }
Check '④ stdout 的 SEED_PROBE 与流水头部逐项相同（两处独立实例一致）' `
      (($stdoutProbeVals -ne '') -and ($stdoutProbeVals -eq $headProbeVals.Trim())) `
      ("stdout=[" + $stdoutProbeVals + "]")

# ---- ⑤ 只传种子、不传路径 ⇒ 正常起、不建流水（与改动前一致）----
$gs2 = Join-Path $Work 'gs-no-rng-log'
$conf2 = Write-Conf $gs2
$log2 = Join-Path $gs2 'stdout.txt'
$err2 = Join-Path $gs2 'stderr.txt'
$leakFile = Join-Path $gs2 'rng-stream.tsv'
$hadSeed2 = Test-Path Env:MIR2_RNG_SEED
$prevSeed2 = if ($hadSeed2) { $env:MIR2_RNG_SEED } else { $null }
try {
  $env:MIR2_RNG_SEED = [string]$Seed
  $pGs2 = Start-Process -FilePath $GameSvrExe -ArgumentList @($conf2) -WorkingDirectory $gs2 `
            -WindowStyle Hidden -PassThru -RedirectStandardOutput $log2 -RedirectStandardError $err2
} finally {
  if ($hadSeed2) { $env:MIR2_RNG_SEED = $prevSeed2 } else { Remove-Item Env:MIR2_RNG_SEED -ErrorAction SilentlyContinue }
}
$started2 = Wait-LogContains $log2 'SEED_PROBE' 25
Kill-Tree $pGs2.Id
Start-Sleep -Milliseconds 300
Check '⑤a 只传种子、不传路径 ⇒ 真 game-svr 正常启动且不建流水文件' ($started2 -and (-not (Test-Path -LiteralPath $leakFile))) ('started=' + $started2 + ' leaked=' + (Test-Path -LiteralPath $leakFile))

# ---- 汇总 ----
$failed = @($script:Checks | Where-Object { -not $_.ok })
$ok = $script:Checks.Count - $failed.Count
Write-Output ("SUMMARY total={0} ok={1} fail={2}" -f $script:Checks.Count, $ok, $failed.Count)
if ($failed.Count -eq 0) {
  Write-Output 'RNG_TRACE_PURITY GREEN'
  exit 0
}
Write-Output ('RNG_TRACE_PURITY FAIL: ' + (($failed | ForEach-Object { $_.name }) -join ' | '))
exit 1
