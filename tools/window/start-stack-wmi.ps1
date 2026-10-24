# 内置 profile 的栈命令包装（卡 `openmir2-window-profile-env-hardening`，tier=L）。
#
# 为什么要有这个包装（window-run-1 §1 三处环境阻塞里的两处，任何 Windows 机器首跑都会踩）：
#  (1) 栈步死锁：直接子进程方式起栈会被孙进程继承 python↔powershell 的 stdout 管道写端，
#      card_window 的 stack 步 `subprocess.run(timeout=600)` 超时 kill 后 `communicate()`
#      等 EOF 永不到来（2026-10-24 实测 stack 步 13 分钟无返回；`E:/tmp/window/run1/pipe_repro.py`
#      最小复现）。修法：WMI `Invoke-CimMethod Win32_Process Create` 派生——父进程是
#      WmiPrvSE，不继承本管道 ⇒ stack 步能返回。
#  (2) GameGate 幽灵会话：card_window 的 `wait_ready` 对网关端口（C# 17200 / Rust 17600，
#      默认端口栈 7200）的连接探针（3ms 开关）会占住 user 会话槽 `SessionId:[0]`，
#      596ms 后 probe 的真实连接复用同一槽被丢 ⇒ `no SM_LOGON(50)`。所以 profile 的
#      `ready_ports` **不含网关端口**；网关就绪改由本包装兜底：等**全部**就绪端口监听
#      + 暖机 $WarmSeconds 秒；到点缺端口 ⇒ 非零退出（fail-closed，card_window 判起栈失败）。
#
# 用法（内置 profile 生成时拼装，见 tools/conformance/card_window.py 的 wmi_stack_argv）：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/start-stack-wmi.ps1 `
#     -Script E:/tmp/conformance/start-csharp3.ps1 -ReadyPorts 15000,17000,17100,17200
#
# 退出码：0 = WMI 派生成功且 $ReadyPorts 全部监听并完成暖机；1 = 派生失败 / 参数非法 /
#         超时后仍有端口未监听。**绝不**因为「部分端口就绪」就返回 0。
param(
  [Parameter(Mandatory = $true)][string]$Script,
  [Parameter(Mandatory = $true)][string]$ReadyPorts,
  [int]$ReadyTimeoutSec = 180,
  [int]$WarmSeconds = 45,
  [string]$InnerArgs = ''
)

$ErrorActionPreference = 'Stop'

$ports = @($ReadyPorts.Split(',') | ForEach-Object { [int]($_.Trim()) } |
  Where-Object { $_ -gt 0 } | Select-Object -Unique)
if ($ports.Count -eq 0) {
  Write-Host ('start-stack-wmi: ReadyPorts 非法（应为逗号分隔端口表）：' + $ReadyPorts)
  exit 1
}
if (-not (Test-Path -LiteralPath $Script -PathType Leaf)) {
  Write-Host ('start-stack-wmi: 内层栈脚本不存在：' + $Script)
  exit 1
}

# WMI 派生（本包装自身由 card_window 捕获 stdout；内层栈从这里起就与该管道无关了）。
$cmd = 'powershell -NoProfile -ExecutionPolicy Bypass -File "' + $Script + '"'
if (-not [string]::IsNullOrWhiteSpace($InnerArgs)) { $cmd += ' ' + $InnerArgs }
$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $cmd }
if ($r.ReturnValue -ne 0) {
  Write-Host ('start-stack-wmi: WMI Win32_Process.Create 失败 rc=' + $r.ReturnValue)
  exit 1
}
Write-Host ('start-stack-wmi: wmi pid=' + $r.ProcessId + ' ready_ports=' + ($ports -join ','))

$deadline = (Get-Date).AddSeconds([Math]::Max(1, $ReadyTimeoutSec))
while ((Get-Date) -lt $deadline) {
  $listening = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
    Where-Object { $ports -contains $_.LocalPort } |
    Select-Object -ExpandProperty LocalPort -Unique)
  if ($listening.Count -ge $ports.Count) { break }
  Start-Sleep -Seconds 2
}

# 暖机：端口齐了再等一会（GameSrv 会话表就位；13:00 实测起后即刻 probe 会丢 enter-world，
# 暖 45s 后与基线一致——window-run-1 §1.3）。
if ($WarmSeconds -gt 0) { Start-Sleep -Seconds $WarmSeconds }

$listening = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
  Where-Object { $ports -contains $_.LocalPort } |
  Select-Object -ExpandProperty LocalPort -Unique)
if ($listening.Count -lt $ports.Count) {
  $missing = @($ports | Where-Object { $listening -notcontains $_ }) -join ','
  Write-Host ('start-stack-wmi: ready ports missing: ' + $missing)
  exit 1
}
Write-Host ('start-stack-wmi: stack warm (' + $WarmSeconds + 's after ready ports)')
exit 0
