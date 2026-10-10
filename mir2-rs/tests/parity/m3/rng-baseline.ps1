#requires -Version 5
<#
.SYNOPSIS
  rng-baseline.ps1 — 可复跑的 C# 端「种子注入 → RNG/掉落/经验序列落盘 + hash」基线。

.DESCRIPTION
  一条命令产出可复跑的 C# 基线，三块内容各自带 sha256：
    ① 世界级 RNG 流：影子副本 + RngSeedHook 注入固定种子，记录真实进程的每次取数；
       其中**确定性前缀**（前 PrefixCalls 次）在两次同种子运行之间必须逐字节相同
       —— 这是"hash 稳定"的判据（`-VerifyRepeat` 会跑两次并断言相等）。
    ② 掉落序列：C# oracle 公式（ParityGolden drop，原码逐字拷贝）在同一 fixture + 同一种子下
       100 次击杀的掉落列表。
    ③ 经验序列：C# oracle（ParityGolden exp，真实 ConfigFile 读 Exps.conf）的 1000 级曲线 + 标量。
  产物：<Session>/product.json（含全部 hash 与来源指纹）+ 各序列文件。

  冻结基线不受影响：影子副本建在会话目录内，改的是**副本**的 runtimeconfig/Server.conf；
  `E:\MirServer` 一字未动（脚本结尾会校验其关键文件的 mtime/hash 未变）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File rng-baseline.ps1 -VerifyRepeat
#>
param(
  [int]$Seed = 42,
  [string]$Session = '',
  [int]$RunSeconds = 30,
  [int]$PrefixCalls = 1500,
  [switch]$VerifyRepeat,
  [switch]$KeepShadow
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
$PackDir = 'E:\MirServer\M2GameSvr'
$ShadowGatePort = 15000

if ([string]::IsNullOrEmpty($Session)) {
  $Session = 'm3rng-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
}
$SessionDir = Join-Path $Here ("sessions\{0}" -f $Session)
New-Item -ItemType Directory -Force -Path $SessionDir | Out-Null
Write-Output ("SESSION " + $SessionDir)

function Get-Sha256([string]$Path) {
  return (Get-FileHash -Algorithm SHA256 -Path $Path).Hash.ToLower()
}

function Get-TextSha256([string]$Text) {
  $sha = [System.Security.Cryptography.SHA256]::Create()
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
  return ([BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLower()
}

# ---- 1. 构建钩子 ----
$HookProj = Join-Path $Here 'rng-hook/RngSeedHook.csproj'
& dotnet build $HookProj -c Release --nologo -v q | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'RngSeedHook 构建失败' }
$HookDll = Join-Path $Here 'rng-hook/bin/Release/net8.0/RngSeedHook.dll'
if (-not (Test-Path $HookDll)) { throw "找不到钩子产物: $HookDll" }

# ---- 2. 冻结基线指纹（运行前后各取一次，证明没被本脚本改动） ----
$PackFingerprintFiles = @('GameSrv.dll', 'OpenMir2.dll', 'M2Server.dll', 'GameSrv.runtimeconfig.json', 'Server.conf')
function Get-PackFingerprint {
  $t = @{}
  foreach ($f in $PackFingerprintFiles) {
    $p = Join-Path $PackDir $f
    if (Test-Path $p) { $t[$f] = Get-Sha256 $p } else { $t[$f] = '(缺失)' }
  }
  return $t
}
$packBefore = Get-PackFingerprint

function Invoke-OneRun([string]$RunTag) {
  $shadow = Join-Path $SessionDir ("shadow-{0}" -f $RunTag)
  $gs = Join-Path $shadow 'gamesvr'
  Write-Output ("RUN {0}: 建影子副本 {1}" -f $RunTag, $gs)
  New-Item -ItemType Directory -Force -Path $gs | Out-Null
  & robocopy $PackDir $gs /E /NFL /NDL /NJH /NJS /NP | Out-Null
  if (-not (Test-Path (Join-Path $gs 'GameSrv.exe'))) { throw "影子副本不完整: $gs" }

  # 影子 runtimeconfig：打开启动钩子（trimmed 发布默认关）
  $rcPath = Join-Path $gs 'GameSrv.runtimeconfig.json'
  $rc = Get-Content $rcPath -Raw | ConvertFrom-Json
  $rc.runtimeOptions.configProperties.'System.StartupHookProvider.IsSupported' = $true
  $rc | ConvertTo-Json -Depth 10 | Set-Content -Path $rcPath -Encoding utf8

  # 影子 Server.conf：GatePort 让开在跑的那份（字节级替换 + 命中数预检）
  $confPath = Join-Path $gs 'Server.conf'
  $needle = [System.Text.Encoding]::ASCII.GetBytes('GatePort=5000')
  $repl = [System.Text.Encoding]::ASCII.GetBytes('GatePort=15000')
  $bytes = [System.IO.File]::ReadAllBytes($confPath)
  $hits = 0
  for ($i = 0; $i -le $bytes.Length - $needle.Length; $i++) {
    $ok = $true
    for ($j = 0; $j -lt $needle.Length; $j++) { if ($bytes[$i + $j] -ne $needle[$j]) { $ok = $false; break } }
    if ($ok) { $hits++ }
  }
  if ($hits -ne 1) { throw ("Server.conf 中 GatePort=5000 命中 {0} 处（预期 1 处）——部署版本变了，先人工核对" -f $hits) }
  # iso-8859-1 逐字节映射，round-trip 不改动 GBK 内容（只替换 ASCII 端口串）
  $latin = [System.Text.Encoding]::GetEncoding('iso-8859-1')
  $text = $latin.GetString($bytes)
  $patched = $text.Replace('GatePort=5000', ("GatePort={0}" -f $ShadowGatePort))
  [System.IO.File]::WriteAllBytes($confPath, $latin.GetBytes($patched))

  Copy-Item $HookDll (Join-Path $gs 'RngSeedHook.dll') -Force

  # 起影子 GameSvr（固定种子 + 记录）
  $logPath = Join-Path $SessionDir ("rng-{0}.tsv" -f $RunTag)
  $stdout = Join-Path $SessionDir ("shadow-{0}.out.log" -f $RunTag)
  $env:DOTNET_STARTUP_HOOKS = Join-Path $gs 'RngSeedHook.dll'
  $env:MIR2_RNG_SEED = "$Seed"
  $env:MIR2_RNG_LOG = $logPath
  $env:MIR2_RNG_SITE = '1'
  try {
    $proc = Start-Process -FilePath (Join-Path $gs 'GameSrv.exe') -WorkingDirectory $gs -PassThru `
      -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError "$stdout.err"
    Write-Output ("RUN {0}: 影子 PID={1}，跑 {2}s" -f $RunTag, $proc.Id, $RunSeconds)
    Start-Sleep -Seconds $RunSeconds
  } finally {
    Remove-Item Env:DOTNET_STARTUP_HOOKS, Env:MIR2_RNG_SEED, Env:MIR2_RNG_LOG, Env:MIR2_RNG_SITE -ErrorAction SilentlyContinue
    Get-Process GameSrv -ErrorAction SilentlyContinue |
      Where-Object { $_.Path -like "$SessionDir*" } | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 2
  }
  if (-not (Test-Path $logPath)) { throw ("RUN {0}: 没生成 RNG 记录 {1}（钩子没生效？看 {2}）" -f $RunTag, $logPath, $stdout) }
  return [pscustomobject]@{ tag = $RunTag; shadow = $shadow; log = $logPath; stdout = $stdout }
}

function Get-StreamRows([string]$Path) {
  return @(Get-Content $Path | Where-Object { $_ -and -not $_.StartsWith('#') -and -not $_.StartsWith('SEED_PROBE') -and ($_ -split "`t")[1] -ne 'INJECT_OK' })
}

# ---- 3. 跑基线（可选跑两次验 hash 稳定） ----
$run1 = Invoke-OneRun 'run1'
$rows1 = Get-StreamRows $run1.log
$prefix1 = ($rows1 | Select-Object -First $PrefixCalls) -join "`n"
$report = [ordered]@{
  seed            = $Seed
  prefix_calls    = $PrefixCalls
  calls_run1      = $rows1.Count
  prefix_sha256   = Get-TextSha256 $prefix1
  stream_sha256   = Get-Sha256 $run1.log
  hook_sha256     = Get-Sha256 $HookDll
  pack_fingerprint = $packBefore
  stable          = $null
}
Write-Output ("RNG run1: calls={0} prefix_sha256={1}" -f $rows1.Count, $report.prefix_sha256)

if ($VerifyRepeat) {
  $run2 = Invoke-OneRun 'run2'
  $rows2 = Get-StreamRows $run2.log
  $prefix2 = ($rows2 | Select-Object -First $PrefixCalls) -join "`n"
  $report.calls_run2 = $rows2.Count
  $report.prefix_sha256_run2 = Get-TextSha256 $prefix2
  $report.stream_sha256_run2 = Get-Sha256 $run2.log
  # 实际公共前缀长度（比 PrefixCalls 更能说明"稳定到哪一步"）
  $common = 0
  $max = [Math]::Min($rows1.Count, $rows2.Count)
  while ($common -lt $max -and $rows1[$common] -eq $rows2[$common]) { $common++ }
  $report.common_prefix_calls = $common
  $report.stable = ($report.prefix_sha256 -eq $report.prefix_sha256_run2)
  Write-Output ("RNG run2: calls={0} prefix_sha256={1} 公共前缀={2}" -f $rows2.Count, $report.prefix_sha256_run2, $common)
  if (-not $report.stable) {
    Write-Output 'RED: 两次同种子运行的确定性前缀 hash 不一致——基线不可复跑'
    $report | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $SessionDir 'product.json') -Encoding utf8
    exit 1
  }
}

# ---- 4. 掉落 / 经验序列（C# oracle 公式，同一 fixture 与种子） ----
# 掉落/经验夹具在上一层（D 线 fixtures），不是 m3/fixtures（那里只有 RNG 记录流）
$fix = (Resolve-Path (Join-Path $Here '../fixtures')).Path
$pg = (Resolve-Path (Join-Path $Here '../csharp/ParityGolden/ParityGolden.csproj')).Path
$dropOut = Join-Path $SessionDir 'drop_100kills.txt'
$expOut = Join-Path $SessionDir 'exp_table.txt'
& dotnet run --project $pg -c Release -- drop (Join-Path $fix 'MonItems/jiangshi1.txt') (Join-Path $fix 'items_jiangshi1.json') $Seed 100 40 $dropOut | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'ParityGolden drop 失败' }
& dotnet run --project $pg -c Release -- exp (Join-Path $fix 'Exps.conf') $expOut | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'ParityGolden exp 失败' }
$report.drop_100kills_sha256 = Get-Sha256 $dropOut
$report.exp_table_sha256 = Get-Sha256 $expOut

# ---- 5. 冻结基线未被改动 ----
$packAfter = Get-PackFingerprint
$packChanged = @()
foreach ($k in $packBefore.Keys) { if ($packBefore[$k] -ne $packAfter[$k]) { $packChanged += $k } }
$report.pack_unchanged = ($packChanged.Count -eq 0)
if ($packChanged.Count -gt 0) {
  Write-Output ("RED: E:\MirServer 冻结基线被改动: {0}" -f ($packChanged -join ', '))
  exit 1
}

$report.finished_at = (Get-Date).ToString('o')
$report.git_rev = (& git -C $RepoRoot rev-parse HEAD).Trim()
$report | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $SessionDir 'product.json') -Encoding utf8

if (-not $KeepShadow) {
  Remove-Item -Recurse -Force (Join-Path $SessionDir 'shadow-run*') -ErrorAction SilentlyContinue
}
Write-Output ('BASELINE_OK ' + (Join-Path $SessionDir 'product.json'))
exit 0
