# tools/window/stop-rust.ps1 — 窗口 Rust 私有栈停止器（仓库内事实源）
#
# 卡 `openmir2-window-tmp-hardening`：原 stop 只在 `E:/tmp/conformance/stop-rust.ps1`，
# 外部清扫（历史三次）后 alt 窗口连停栈都跑不了。本脚本收进仓库，委托
# `tools/m2-acceptance/stop_mir2_server.ps1`（读 <LogDir>/mir2-server.pid.json 按
# stop_order 降序停），部署根只作为**参数**（默认按 MIR2_CONFORMANCE_ROOT 解析）。
#
# 用法：
#   powershell -NoProfile -File tools/window/stop-rust.ps1
#   powershell ... -Root E:/tmp/conformance-copy/rust-root -LogDir E:/tmp/conformance-copy/rust-logs
#   powershell ... -Selftest    # 合成夹具（假 stopper）：退出码透传 + 缺件 fail-closed
#
# 退出码：0 = 已停（或本来就没有进程）；非零 = stopper 的退出码（透传）；
#         2 = 参数/缺件/自检红（fail-closed，绝不冒充已停）。
param(
  [string]$Root = '',
  [string]$LogDir = '',
  [string]$Stopper = '',
  [switch]$Selftest
)
$ErrorActionPreference = 'Stop'

$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Repo = (Resolve-Path (Join-Path $Here '..\..')).Path
function Get-ConformanceRoot {
  $v = [string]$env:MIR2_CONFORMANCE_ROOT
  if ([string]::IsNullOrWhiteSpace($v)) { return 'E:\tmp\conformance' }
  return $v
}
if ([string]::IsNullOrWhiteSpace($Root)) { $Root = Join-Path (Get-ConformanceRoot) 'rust-root' }
if ([string]::IsNullOrWhiteSpace($LogDir)) { $LogDir = Join-Path (Get-ConformanceRoot) 'rust-logs' }
if ([string]::IsNullOrWhiteSpace($Stopper)) {
  $Stopper = Join-Path $Repo 'tools\m2-acceptance\stop_mir2_server.ps1'
}

function Invoke-Stopper {
  # 直接派生（不经 WMI），仍用进程级策略 env 下发：与 start-rust.ps1 同口径，
  # 命令行不留 `-ExecutionPolicy`（本机 HIPS 对派生链上的该开关敏感）。
  param([string]$Script, [string]$RootDir, [string]$LogDirArg)
  if (-not (Test-Path -LiteralPath $Script -PathType Leaf)) {
    Write-Host ("RED: 缺 stopper 脚本：{0}" -f $Script)
    return 2
  }
  $psExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  if (-not (Test-Path -LiteralPath $psExe -PathType Leaf)) { $psExe = 'powershell' }
  $argv = @('-NoProfile', '-File', $Script, '-Root', $RootDir, '-LogDir', $LogDirArg)
  $prev = $env:PSExecutionPolicyPreference
  $rc = $null
  try {
    $env:PSExecutionPolicyPreference = 'Bypass'
    & $psExe @argv
    $rc = $LASTEXITCODE
  } finally {
    if ($null -eq $prev) { Remove-Item Env:PSExecutionPolicyPreference -ErrorAction SilentlyContinue }
    else { $env:PSExecutionPolicyPreference = $prev }
  }
  if ($null -eq $rc) { return 1 }   # 防御：子进程没给退出码 ⇒ 不冒充成功
  return [int]$rc
}

if ($Selftest) {
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('stop-rust-selftest-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  $fails = New-Object System.Collections.Generic.List[string]
  function Assert-Case([bool]$cond, [string]$name, [string]$detail) {
    if ($cond) { Write-Output ('stop-rust selftest: PASS ' + $name) }
    else { Write-Output ('stop-rust selftest: FAIL ' + $name + ' :: ' + $detail); $script:fails.Add($name) }
  }
  try {
    # 牙 1：缺 stopper ⇒ rc=2 + 点名（绝不冒充已停）
    $rcMissing = Invoke-Stopper -Script (Join-Path $tmp 'no-such.ps1') -RootDir $Root -LogDirArg $LogDir
    Assert-Case ($rcMissing -eq 2) 'negative: missing stopper => rc=2' ('rc=' + $rcMissing)

    # 牙 2：stopper 退出码透传（合成脚本 exit 7）
    $stub = Join-Path $tmp 'stub-stop.ps1'
    [System.IO.File]::WriteAllText($stub, 'exit 7', (New-Object System.Text.UTF8Encoding($false)))
    $rc7 = Invoke-Stopper -Script $stub -RootDir $Root -LogDirArg $LogDir
    Assert-Case ($rc7 -eq 7) 'negative: stopper exit code propagates (rc=7)' ('rc=' + $rc7)

    # 牙 3：默认 stopper 在本检出内（删掉 Join-Path $Repo ⇒ 回落外部路径时必红）
    $defaultOk = $Stopper.StartsWith($Repo, [System.StringComparison]::OrdinalIgnoreCase)
    Assert-Case $defaultOk 'positive: default stopper is repo-internal' ('stopper=' + $Stopper)
  } finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }
  if ($fails.Count -gt 0) {
    Write-Output ('stop-rust selftest: RED (' + $fails.Count + ' failed)')
    exit 1
  }
  Write-Output 'stop-rust selftest: GREEN (1 positive + 2 negative)'
  exit 0
}

if (-not (Test-Path -LiteralPath $Stopper -PathType Leaf)) {
  Write-Output ("RED: 缺 stopper 脚本：{0}（checkout 不完整？）" -f $Stopper)
  exit 2
}
if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
  Write-Output ("RED: 缺部署根：{0}（MIR2_CONFORMANCE_ROOT={1}；外部清扫后需重建）" -f $Root, (Get-ConformanceRoot))
  exit 2
}
$rc = Invoke-Stopper -Script $Stopper -RootDir $Root -LogDirArg $LogDir
exit $rc
