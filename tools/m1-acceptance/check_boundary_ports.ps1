#requires -Version 5
<#
.SYNOPSIS
  check_boundary_ports.ps1 — M1⑤①「全 Rust 边界链」端口所有权实拍（携带牙齿）。

.DESCRIPTION
  对 M1 边界链的**五个端口**逐一取实际监听者的 PID + 命令行，判定：

    GREEN  = 五个端口都在听，且每个端口的监听者都判为 **Rust**，且**入口就是该端口期望的那个 Rust 入口**；
    RED    = 端口在听但监听者判为 C#(`E:\MirServer\*`)/未知 ⇒ 「全 Rust」不成立（退出码 1）；
             或入口是已知 Rust 入口但挂在非期望端口（接错线）⇒ 退出码 1；
    PREREQ = 有端口不在听 ⇒ 前置未满足（退出码 2，不静默通过）。

  实现归属分类**复用** `tools/m2-acceptance/portclassify.ps1` 的 `Get-M2PortImpl`
  （同一口径：C# 根优先 → Rust 构建目录 → 可执行基名回落）。本脚本只额外补一张
  **边界链端口表**（`M1-边界服务移植计划.md` §1 链路表），因为 M2 的 `$M2PortEntries`
  只覆盖 5000/7000/7100/7200。

  为什么必须有第二层牙齿（入口归属）：只判「是 Rust」挡不住**接错线**——
  例如把 `db-srv` 起在 5500、把 `login-srv` 起在 5100，五端口仍全是 Rust、旧判据照样 GREEN，
  但 §1 链路表（7000 login-gate / 5500+5600 login-srv / 7100 sel-gate / 5100 db-srv）已不成立。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/check_boundary_ports.ps1
.EXAMPLE
  # 端口所有权红检（把链里任一环换回 C# 或停掉）时，期望退出码 1/2
  powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/check_boundary_ports.ps1 `
      -OutFile E:\tmp\openmir2-m1load\evidence\redcheck-ports.json
#>
param(
  [int[]]$Ports = @(7000, 5500, 7100, 5100, 5600),
  [string]$OutFile = "",
  [switch]$AllowUnknownImpl,  # 只要求"端口在听"，实现归属交给人判（默认不放松）
  [switch]$AllowCrossEntry    # 允许已识别的 Rust 入口挂在非期望端口（默认不放松）
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
if ([string]::IsNullOrEmpty($OutFile)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $OutFile = Join-Path "E:\tmp\openmir2-m1load\evidence" ("boundary-ports-" + $stamp + ".json")
}

# 复用 M2 的实现归属分类口径（rust / csharp / unknown）。
. (Join-Path $Here "..\m2-acceptance\portclassify.ps1")

# 边界链端口表（`mir2-rs/docs/M1-边界服务移植计划.md` §1）。
# 注意：DBSrv 的 **5100** 是 SelGate 面；**6000** 是 GameSvr 取档面（Rust 未实现、不属于本卡）。
# 注意：必须用普通 hashtable（`@{}`）而不是 `[ordered]@{}`——OrderedDictionary 的 `[$int]` 会绑到
# `get_Item(int index)`（按序号取值），与整型键冲突 ⇒ `$T[7000]` 恒为 $null（实测踩过）。
$script:M1BoundaryEntries = @{
  7000 = "login-gate"   # 客户端 ↔ LoginGate
  5500 = "login-srv"    # LoginGate ↔ LoginSrv
  7100 = "sel-gate"     # 客户端 ↔ SelGate
  5100 = "db-srv"       # SelGate ↔ DBSrv(UserService)
  5600 = "login-srv"    # DBSrv → LoginSrv（监听方是 LoginSrv）
}

function Get-M1BoundaryExpected {
  param([int]$Port)
  if ($script:M1BoundaryEntries.Contains($Port)) { return $script:M1BoundaryEntries[$Port] }
  return ""
}

# Rust 入口全集（含 login-srv/db-srv；portclassify 的 $M2RustNames 已含它们，这里显式再列一次，
# 以便改坏必红：把入口名写成别的已知 Rust 入口 ⇒ cross）。
function Get-M1BoundaryEntryVerdict {
  param([int]$Port, [string]$EntryName)
  if ([string]::IsNullOrEmpty($EntryName)) { return "unknown" }
  $expected = Get-M1BoundaryExpected -Port $Port
  if ([string]::IsNullOrEmpty($expected)) { return "unknown" }
  if ($EntryName -eq $expected) { return "ok" }
  if ($script:M2RustNames -contains $EntryName) { return "cross" }
  return "unknown"
}

function Get-PortOwner([int]$Port) {
  $conn = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if (-not $conn) {
    return [pscustomobject]@{
      port = $Port; listening = $false; pid = $null; name = ""; path = ""; command_line = ""
      impl = "none"; entry = ""; expected_entry = (Get-M1BoundaryExpected -Port $Port)
      entry_verdict = "unknown"
    }
  }
  $cim = Get-CimInstance Win32_Process -Filter ("ProcessId=$($conn.OwningProcess)") -ErrorAction SilentlyContinue
  $name = if ($cim) { $cim.Name } else { "" }
  $exe = if ($cim) { $cim.ExecutablePath } else { "" }
  $cmd = if ($cim) { $cim.CommandLine } else { "" }
  $impl = Get-M2PortImpl -Exe $exe -CommandLine $cmd -Name $name
  $entry = Get-M2EntryName -Exe $exe -CommandLine $cmd -Name $name
  $verdict = Get-M1BoundaryEntryVerdict -Port $Port -EntryName $entry
  return [pscustomobject]@{
    port = $Port; listening = $true; pid = $conn.OwningProcess; name = $name; path = $exe
    command_line = $cmd; impl = $impl; entry = $entry
    expected_entry = (Get-M1BoundaryExpected -Port $Port); entry_verdict = $verdict
  }
}

Write-Output "M1⑤① 全 Rust 边界链端口所有权实拍（repo=$RepoRoot）"
$holders = @()
foreach ($p in $Ports) {
  $h = Get-PortOwner $p
  $holders += $h
  if (-not $h.listening) {
    Write-Output ("PORT {0,-5} (无监听)  impl=none  expected={1}" -f $h.port, $h.expected_entry)
  } else {
    Write-Output ("PORT {0,-5} <- pid={1} [{2}] {3}" -f $h.port, $h.pid, $h.impl, $h.path)
    Write-Output ("            cmd: {0}" -f $h.command_line)
    Write-Output ("            entry={0} expected={1} verdict={2}" -f $h.entry, $h.expected_entry, $h.entry_verdict)
  }
}

$notListening = @($holders | Where-Object { -not $_.listening })
$notRust = @($holders | Where-Object { $_.listening -and $_.impl -ne "rust" })
$crossed = @($holders | Where-Object { $_.listening -and $_.entry_verdict -eq "cross" })

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null
$evidence = [ordered]@{
  tool = "m1-acceptance/check_boundary_ports.ps1"
  ts = (Get-Date).ToString("o")
  repo = $RepoRoot
  chain = "7000 login-gate / 5500+5600 login-srv / 7100 sel-gate / 5100 db-srv (M1-边界服务移植计划.md §1)"
  ports = $holders
  not_listening = @($notListening | ForEach-Object { $_.port })
  not_rust = @($notRust | ForEach-Object { $_.port })
  cross_entry = @($crossed | ForEach-Object { $_.port })
  pass = $false
}
try {
  $evidence | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
} catch { Write-Output ("WARN: 证据写盘失败: {0}" -f $_) }
Write-Output ("EVIDENCE " + $OutFile)

if ($notListening.Count -gt 0) {
  Write-Output ("PREREQ_MISSING: 边界链端口不在听 -> " + (($notListening | ForEach-Object { $_.port }) -join ", "))
  Write-Output "  先起全 Rust 边界栈（tools/m1-acceptance/start_boundary_stack.ps1）再跑本步"
  exit 2
}
if ($notRust.Count -gt 0 -and -not $AllowUnknownImpl) {
  foreach ($h in $notRust) {
    Write-Output ("RED: 端口 {0} 的监听者不是 Rust（impl={1}）: {2}" -f $h.port, $h.impl, $h.command_line)
  }
  Write-Output "RED: 「全 Rust 边界链」不成立——有端口被非 Rust 实现占着（C# 栈或未知入口）"
  exit 1
}
if ($crossed.Count -gt 0 -and -not $AllowCrossEntry) {
  foreach ($h in $crossed) {
    Write-Output ("RED: 端口 {0} 期望入口 {1}，实际是 {2}（已知 Rust 入口，但接错线）" -f $h.port, $h.expected_entry, $h.entry)
  }
  Write-Output "RED: 「全 Rust 边界链」不成立——Rust 入口挂在了非期望端口上（起错入口/端口表漂移）"
  exit 1
}

$evidence.pass = $true
$evidence | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
Write-Output "GREEN: 五端口（7000/5500/7100/5100/5600）的所有权均为期望的 Rust 入口（验收①可判）"
exit 0
