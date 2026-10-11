#requires -Version 5
<#
.SYNOPSIS
  check_ports.ps1 — M2 验收①「端口所有权实拍」。

.DESCRIPTION
  对 GameSvr 侧与入口三端口逐一取**实际监听者**的 PID + 命令行，判定是不是 Rust 二进制。
  这是 reviewer 认可的判据口径：`Get-NetTCPConnection -State Listen` 拿 OwningProcess，
  再用 `Get-CimInstance Win32_Process` 拿 CommandLine（**命令行**比 Path 更能证明"跑的是哪个入口"）。

  端口（设计文档 §2「进程/端口表」）：
    5000  GameSvr    （Rust `bins/game-svr`；C# 是 `E:\MirServer\GameSvr.exe`）
    7000  LoginGate  （Rust `bins/login-gate`）
    7100  SelGate    （Rust `bins/sel-gate`）
    7200  GameGate   （Rust `bins/game-gate`）

  红/绿：
    GREEN  = 四个端口都在听，且 OwningProcess 都判为 Rust；
    RED    = 端口在听但监听者判为 C#(`E:\MirServer\*`) 或未知 ⇒ 验收①不成立（退出码 1）；
    PREREQ = 有端口不在听 ⇒ 前置未满足（退出码 2，不静默通过）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_ports.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_ports.ps1 -OutFile E:\tmp\m2-acceptance\ports.json
#>
param(
  [int[]]$Ports = @(5000, 7000, 7100, 7200),
  [string]$PortsCsv = "",
  [string]$OutFile = "",
  [switch]$AllowUnknownImpl   # 只要求"端口在听"，实现归属交给人判（默认不放松）
)

$ErrorActionPreference = "Stop"
if (-not [string]::IsNullOrEmpty($PortsCsv)) {
  $Ports = @($PortsCsv.Split(",") | ForEach-Object { [int]$_.Trim() })
}
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
if ([string]::IsNullOrEmpty($OutFile)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $OutFile = Join-Path "E:\tmp\m2-acceptance" ("ports-$stamp.json")
}

# 已知的 Rust 二进制名（cargo run / target 下的产物），用于命令行判定。
$RustNames = @("login-gate", "sel-gate", "login-srv", "db-srv", "game-gate", "game-svr")
# 现网 C# 服务端目录（端口被它占着 = 验收①失败）。
$CsharpRoot = "E:\MirServer"

function Get-PortOwner([int]$Port) {
  $conn = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if (-not $conn) {
    return [pscustomobject]@{ port = $Port; listening = $false; pid = $null; name = ""; path = ""; command_line = ""; impl = "none" }
  }
  $cim = Get-CimInstance Win32_Process -Filter ("ProcessId=$($conn.OwningProcess)") -ErrorAction SilentlyContinue
  $name = if ($cim) { $cim.Name } else { "" }
  $exe = if ($cim) { $cim.ExecutablePath } else { "" }
  $cmd = if ($cim) { $cim.CommandLine } else { "" }

  $impl = "unknown"
  $probe = ($exe + " " + $cmd)
  if ($exe -like "$CsharpRoot\*" -or $probe -like "*$CsharpRoot\*") {
    $impl = "csharp"
  }
  elseif ($exe -like "*\mir2-rs\target\*" -or $probe -like "*\mir2-rs\target\*") {
    $impl = "rust"
  }
  else {
    $base = [System.IO.Path]::GetFileNameWithoutExtension($name)
    if ($RustNames -contains $base) { $impl = "rust" }
  }
  return [pscustomobject]@{
    port = $Port; listening = $true; pid = $conn.OwningProcess
    name = $name; path = $exe; command_line = $cmd; impl = $impl
  }
}

Write-Output "M2 验收① 端口所有权实拍（repo=$RepoRoot）"
$holders = @()
foreach ($p in $Ports) {
  $h = Get-PortOwner $p
  $holders += $h
  if (-not $h.listening) {
    Write-Output ("PORT {0,-5} (无监听)  impl=none" -f $h.port)
  }
  else {
    Write-Output ("PORT {0,-5} <- pid={1} [{2}] {3}" -f $h.port, $h.pid, $h.impl, $h.path)
    Write-Output ("            cmd: {0}" -f $h.command_line)
  }
}

$notListening = @($holders | Where-Object { -not $_.listening })
$notRust = @($holders | Where-Object { $_.listening -and $_.impl -ne "rust" })

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null
$evidence = [ordered]@{
  tool = "m2-acceptance/check_ports.ps1"
  ts = (Get-Date).ToString("o")
  repo = $RepoRoot
  ports = $holders
  not_listening = @($notListening | ForEach-Object { $_.port })
  not_rust = @($notRust | ForEach-Object { $_.port })
  pass = $false
}
try {
  $evidence | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
} catch { Write-Output ("WARN: 证据写盘失败: {0}" -f $_) }
Write-Output ("EVIDENCE " + $OutFile)

if ($notListening.Count -gt 0) {
  Write-Output ("PREREQ_MISSING: 端口不在听 -> " + (($notListening | ForEach-Object { $_.port }) -join ", "))
  Write-Output "  先起 Rust 链路（5000 game-svr / 7000 login-gate / 7100 sel-gate / 7200 game-gate）再跑本步"
  exit 2
}
if ($notRust.Count -gt 0 -and -not $AllowUnknownImpl) {
  foreach ($h in $notRust) {
    Write-Output ("RED: 端口 {0} 的监听者不是 Rust（impl={1}）: {2}" -f $h.port, $h.impl, $h.command_line)
  }
  Write-Output "RED: 验收①不成立——有端口被非 Rust 实现占着（很可能是 E:\MirServer 的 C# 栈）"
  exit 1
}

$evidence.pass = $true
$evidence | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
Write-Output "GREEN: 四个端口的所有权均为 Rust 二进制（验收①可判）"
exit 0
