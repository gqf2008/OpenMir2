#requires -Version 5
<#
  send_ctrlc.ps1 — 给指定 pid 的控制台进程发 CTRL_C_EVENT(0) / CTRL_BREAK_EVENT(1)。

  机理：AttachConsole(pid) 挂到目标进程的控制台，SetConsoleCtrlHandler(NULL,true) 让本进程
  忽略该事件（否则自己先死），GenerateConsoleCtrlEvent(event, 0) 把事件发给该控制台上的所有
  进程。本脚本是**一次性进程**，跑完即退，不影响调用者的控制台。

  M1③ shutdown 路径用它触发 GameSvr 的 host 优雅停机（.NET `UseConsoleLifetime` +
  `Console.CancelKeyPress` ⇒ cts.Cancel ⇒ StoppingAsync ⇒ OnShutdown → SavePlayer）。
  实测：CTRL_BREAK_EVENT(1) 生效（CTRL_C_EVENT 被该进程忽略/不透传，见证据 README）。
#>
param(
  [Parameter(Mandatory = $true)][int]$TargetPid,
  [int]$Event = 1
)
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class CtrlShim {
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool FreeConsole();
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool AttachConsole(uint dwProcessId);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetConsoleCtrlHandler(IntPtr handler, bool add);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GenerateConsoleCtrlEvent(uint dwCtrlEvent, uint dwProcessGroupId);
}
"@
[void][CtrlShim]::FreeConsole()
$attached = [CtrlShim]::AttachConsole([uint32]$TargetPid)
Write-Output ("ATTACH pid={0} ok={1}" -f $TargetPid, $attached)
if (-not $attached) { exit 2 }
[void][CtrlShim]::SetConsoleCtrlHandler([IntPtr]::Zero, $true)
$sent = [CtrlShim]::GenerateConsoleCtrlEvent([uint32]$Event, 0)
Write-Output ("CTRL_EVENT event={0} sent={1}" -f $Event, $sent)
Start-Sleep -Seconds 5
[void][CtrlShim]::FreeConsole()
exit 0
