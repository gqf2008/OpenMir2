#requires -Version 5
<#
.SYNOPSIS
  redcheck_boundary.ps1 — M1⑤④ 红检：把「全 Rust 边界链」判据弄坏 ⇒ `check_boundary_ports.ps1` 必须变红。

.DESCRIPTION
  两种弄坏法（都留原始输出）：
    -Mode swap-csharp-selgate（默认，推荐）：停掉 Rust `sel-gate`，改起 **C# SelGate**
      （`E:\MirServer\SelGate\SelGate.exe`，5100/7100）⇒ 7100 监听者判 csharp ⇒ 判据 RED（exit 1）；
    -Mode stop-login-srv：直接停掉 Rust `login-srv` ⇒ 5500/5600 无监听 ⇒ PREREQ（exit 2）。

  牙齿必须有「还原必绿」：弄坏 → 判红 → 全栈停 → 全栈重起 → 判绿。任何一步不符合预期即 FAIL（exit 1）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/redcheck_boundary.ps1
#>
param(
  [ValidateSet('swap-csharp-selgate', 'stop-login-srv')]
  [string]$Mode = 'swap-csharp-selgate',
  [string]$EvidenceDir = 'E:\tmp\openmir2-m1load\evidence',
  [string]$StackLogDir = 'E:\tmp\openmir2-m1load\stack\logs',
  [string]$RustBinDir = 'E:\tmp\openmir2-m1load-target\debug',
  [string]$MirServer = 'E:\MirServer'
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Check = Join-Path $Here 'check_boundary_ports.ps1'
$Start = Join-Path $Here 'start_boundary_stack.ps1'
$PidFile = Join-Path $StackLogDir 'boundary-stack.pid.json'
New-Item -ItemType Directory -Force -Path $EvidenceDir | Out-Null

function Invoke-Check([string]$OutFile) {
  $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $Check -OutFile $OutFile 2>&1
  $rc = $LASTEXITCODE
  return @{ rc = $rc; output = $out }
}

# 调子脚本时**不要用管道**（`| ForEach-Object`）：起栈脚本会 Start-Process 出长驻服务，
# 管道写端被子进程继承 ⇒ 父进程已退出但管道不关，`ForEach-Object` 永远等不到 EOF（实测踩过）。
# 改成先重定向到文件（非管道），读完再回显。
function Invoke-ChildScript([string]$Path, [string]$LogPath, [string[]]$Extra = @()) {
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $Path) + $Extra
  Remove-Item $LogPath -Force -ErrorAction SilentlyContinue
  & powershell @a *> $LogPath
  $rc = $LASTEXITCODE
  if (Test-Path $LogPath) { Get-Content $LogPath | ForEach-Object { Write-Output $_ } }
  return $rc
}

function Get-PidEntry([string]$Name) {
  if (-not (Test-Path $PidFile)) { return $null }
  $saved = Get-Content $PidFile -Raw | ConvertFrom-Json
  $entries = @($saved)
  if ($entries.Count -eq 1 -and ($entries[0].PSObject.Properties.Name -contains 'procs')) { $entries = @($entries[0].procs) }
  return ($entries | Where-Object { $_.name -eq $Name } | Select-Object -First 1)
}

# ---- 0) 基线：必须已绿 ----
$green = Invoke-Check (Join-Path $EvidenceDir 'redcheck-0-green.json')
$green.output | Set-Content (Join-Path $EvidenceDir 'redcheck-0-green.log') -Encoding utf8
Write-Output '==== 0) 基线（期望 GREEN / rc=0）===='
$green.output | ForEach-Object { Write-Output $_ }
if ($green.rc -ne 0) {
  Write-Output ("FAIL: 基线不是 GREEN（rc={0}）——先起全 Rust 边界栈" -f $green.rc)
  exit 1
}

# ---- 1) 弄坏 ----
Write-Output ("==== 1) 弄坏（Mode={0}）====" -f $Mode)
$csharpPid = $null
if ($Mode -eq 'swap-csharp-selgate') {
  $e = Get-PidEntry 'sel-gate'
  if ($e) { Stop-Process -Id $e.pid -Force -ErrorAction SilentlyContinue; Write-Output ("STOPPED rust sel-gate pid={0}" -f $e.pid) }
  Start-Sleep -Seconds 1
  $exe = Join-Path $MirServer 'SelGate\SelGate.exe'
  if (-not (Test-Path $exe)) { Write-Output ("PREREQ_MISSING: " + $exe); exit 2 }
  $p = Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe -Parent) -PassThru -WindowStyle Hidden `
       -RedirectStandardOutput (Join-Path $EvidenceDir 'redcheck-csharp-selgate.out.log') `
       -RedirectStandardError (Join-Path $EvidenceDir 'redcheck-csharp-selgate.err.log')
  $csharpPid = $p.Id
  Write-Output ("STARTED C# SelGate pid={0}" -f $csharpPid)
  Start-Sleep -Seconds 3
} else {
  $e = Get-PidEntry 'login-srv'
  if ($e) { Stop-Process -Id $e.pid -Force -ErrorAction SilentlyContinue; Write-Output ("STOPPED rust login-srv pid={0}" -f $e.pid) }
  Start-Sleep -Seconds 2
}

$red = Invoke-Check (Join-Path $EvidenceDir ("redcheck-1-{0}.json" -f $Mode))
$red.output | Set-Content (Join-Path $EvidenceDir ("redcheck-1-{0}.log" -f $Mode)) -Encoding utf8
Write-Output '==== 1) 判据输出（期望非 0）===='
$red.output | ForEach-Object { Write-Output $_ }
$breakOk = $false
if ($Mode -eq 'swap-csharp-selgate') { $breakOk = ($red.rc -eq 1) } else { $breakOk = ($red.rc -eq 2) }
if (-not $breakOk) {
  Write-Output ("FAIL: 弄坏后判据未按预期变红（期望 rc={0}，实得 {1}）" -f $(if ($Mode -eq 'swap-csharp-selgate') { 1 } else { 2 }), $red.rc)
  if ($csharpPid) { Stop-Process -Id $csharpPid -Force -ErrorAction SilentlyContinue }
  exit 1
}
Write-Output ("REDCHECK OK: 弄坏后判据变红 rc={0}" -f $red.rc)

# ---- 2) 还原：停 C# 环 + 全栈重起 ----
if ($csharpPid) { Stop-Process -Id $csharpPid -Force -ErrorAction SilentlyContinue; Write-Output ("STOPPED C# SelGate pid={0}" -f $csharpPid) }
Write-Output '==== 2) 全栈停 → 重起 ===='
Invoke-ChildScript -Path $Start -LogPath (Join-Path $EvidenceDir 'redcheck-2-restart-stop.log') -Extra @('-Stop') | Out-Null
$rcStart = Invoke-ChildScript -Path $Start -LogPath (Join-Path $EvidenceDir 'redcheck-2-restart-start.log')
if ($rcStart -ne 0) { Write-Output ("FAIL: 还原起栈失败 rc={0}" -f $rcStart); exit 1 }

$green2 = Invoke-Check (Join-Path $EvidenceDir 'redcheck-2-restored.json')
$green2.output | Set-Content (Join-Path $EvidenceDir 'redcheck-2-restored.log') -Encoding utf8
Write-Output '==== 2) 还原后（期望 GREEN / rc=0）===='
$green2.output | ForEach-Object { Write-Output $_ }
if ($green2.rc -ne 0) { Write-Output ("FAIL: 还原后不是 GREEN（rc={0}）" -f $green2.rc); exit 1 }
Write-Output 'REDCHECK PASS: 弄坏必红、还原必绿'
exit 0
