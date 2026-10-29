# stop-csharp-graceful.ps1 — 给 C# 私有栈的 GameSvr 发 **CTRL_C**（优雅停），等它自己退出。
#
# 卡 openmir2-harness-stop-and-evidence ①(b)。原理与**实测边界**（v3，2026-10-29）：
#   `src/GameSrv/Program.cs:16` 注册 `Console.CancelKeyPress` ⇒ 仅 `cts.Cancel()`
#   （停网关），**不会**走 `AppService.StoppingAsync → OnShutdown → SavePlayer`
#   （AppService.cs:206/210）。实测：CTRL_C 后游戏照常运行（4s 后仍有 DBG 任务行）、
#   无「Application is stopping」/「保存玩家数据」日志行、进程 30s 不退、DB 零写入。
#   ⇒ 本工具只做「网关排水式停 GameSvr」，**不提供存档保证**；存档唯一保证 =
#   场景 settle + `require_changes`（runbook §0.15）。进程不退时按 STILL_ALIVE 处置。
#
# 发送方式（Windows 只能对**自己附着**的控制台发 CTRL_C）：经典三步
#   FreeConsole → AttachConsole(<GameSvr pid>) → SetConsoleCtrlHandler(NULL,$true)
#   → GenerateConsoleCtrlEvent(CTRL_C_EVENT=0, 0) → FreeConsole。
#   SetConsoleCtrlHandler 让**本进程**忽略这次 CTRL_C（否则把自己也停了）。
#   注意 `$PID` 是 PowerShell 只读自动变量 ⇒ 参数名用 `-GamePid`。
#
# 用法：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/stop-csharp-graceful.ps1 `
#       -PidFile <start-csharp 的 pid json> [-Name GameSvr] [-TimeoutSec 60]
#   powershell ... -GamePid <pid>                       # 直接给 pid
#
# 退出码：0 = GRACEFUL_EXIT（进程已退出）；3 = STILL_ALIVE（CTRL_C 已送达但超时未退——
#         关服倒计时/网关排水比存档慢，属预期；此时才允许人工 `Stop-Process -Force`，并必须
#         在证据里登记「未走优雅退出」）；4 = ATTACH_FAILED（CTRL_C **没送达**——目标没有
#         控制台/已退出；此时**不得**轮询存档，直接走人工处置）；2 = 参数/缺件错误。
param(
  [int]$GamePid = 0,
  [string]$PidFile = '',
  [string]$Name = 'GameSvr',
  [int]$TimeoutSec = 60
)

$ErrorActionPreference = 'Stop'

if ($GamePid -eq 0) {
  if (-not $PidFile -or -not (Test-Path -LiteralPath $PidFile)) {
    Write-Output 'USAGE_ERROR pid-file 缺失（start-csharp.ps1 的 start-csharp.pid.json）'
    exit 2
  }
  $items = @(Get-Content -LiteralPath $PidFile -Raw | ConvertFrom-Json)
  if ($items.Count -eq 1 -and ($items[0] -is [System.Array])) { $items = @($items[0]) }
  $hit = $items | Where-Object { $_.name -eq $Name } | Select-Object -First 1
  if (-not $hit) {
    Write-Output ("USAGE_ERROR pid 文件里没有 {0}（roles：{1}）" -f $Name, (($items | ForEach-Object { $_.name }) -join ','))
    exit 2
  }
  $GamePid = [int]$hit.pid
}

$proc = Get-Process -Id $GamePid -ErrorAction SilentlyContinue
if (-not $proc) {
  Write-Output ("ALREADY_GONE pid={0}" -f $GamePid)
  exit 0
}

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class CtrlCSender2 {
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool FreeConsole();
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool AttachConsole(uint dwProcessId);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool GenerateConsoleCtrlEvent(uint dwCtrlEvent, uint dwProcessGroupId);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool SetConsoleCtrlHandler(IntPtr handler, bool add);
}
"@

# 先忽略自己的 CTRL_C（否则连着目标控制台的我们会一起收到）；
# **再 FreeConsole**——不脱离自家控制台，AttachConsole(别的) 会 err=5。
[void][CtrlCSender2]::SetConsoleCtrlHandler([IntPtr]::Zero, $true)
[void][CtrlCSender2]::FreeConsole()
$attached = [CtrlCSender2]::AttachConsole([uint32]$GamePid)
if (-not $attached) {
  Write-Output ("ATTACH_FAILED pid={0} err={1}（CTRL_C 没送达：目标没有控制台/已退出；不得轮询存档）" -f $GamePid, [Runtime.InteropServices.Marshal]::GetLastWin32Error())
  exit 4
}
[void][CtrlCSender2]::GenerateConsoleCtrlEvent(0, 0)   # CTRL_C_EVENT
[void][CtrlCSender2]::FreeConsole()

Write-Output ("CTRL_C_SENT pid={0}" -f $GamePid)
$deadline = (Get-Date).AddSeconds($TimeoutSec)
while ((Get-Date) -lt $deadline) {
  if (-not (Get-Process -Id $GamePid -ErrorAction SilentlyContinue)) {
    Write-Output ("GRACEFUL_EXIT pid={0}" -f $GamePid)
    exit 0
  }
  Start-Sleep -Milliseconds 250
}
Write-Output ("STILL_ALIVE pid={0}（超时未退；此时才允许人工 Stop-Process -Force，并必须在证据里登记「未走优雅停」）" -f $GamePid)
exit 3
