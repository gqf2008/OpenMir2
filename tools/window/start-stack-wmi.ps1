# 内置 profile 的栈命令包装（卡 `openmir2-window-profile-env-hardening` + 卡
# `openmir2-window-wmi-env-forwarding`，tier=L）。
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
# 卡 `openmir2-window-wmi-env-forwarding` 的两处加固（53 分诊实锤：game-svr.out.log
# 05:57:06.620 `CLIENT_ABILITY_LAYOUT layout=client-58（MIR2_CLIENT_ABILITY_LAYOUT=<unset>）`
# ——比失败更危险的是不报错，所有 env-pin 场景静默跑错布局）：
#  (3) env 显式转发：WMI 派生的父进程是 WmiPrvSE，**不继承调用方环境**——python 调用面
#      设好的 `sides.<side>.env`（MIR2_CLIENT_ABILITY_LAYOUT / MIR2_RNG_SEED /
#      MIR2_RNG_STATE_LOG / MIR2_CLOCK_MS / CARD_WINDOW_* …）到 server 就丢了。修法：
#      `Win32_ProcessStartup` 的 `EnvironmentVariables` 显式下发**调用方全量环境**
#      （KEY=VALUE 数组；转发清单 = 调用方 env 全集，其中 MIR2_* / CARD_WINDOW_* 是
#      pin 消费面，grep 全集见本卡报告 docs/window-wmi-env-forwarding-20261024.md）。
#      注意：`Win32_ProcessStartup.EnvironmentValues` 是**整体替换**子进程环境块，
#      所以必须下发全量而不是只发 pin，否则 SystemRoot/PATH 等系统变量会反丢。
#  (4) pin 可观测（fail-closed）：`-PinChecks KEY=VAL,…` + `-PinLog <server out 日志>`——
#      起栈完成后扫描 server 日志的自证行（game-svr 启动即打
#      `CLIENT_ABILITY_LAYOUT layout=oracle-40（MIR2_CLIENT_ABILITY_LAYOUT=0）`，
#      见 mir2-rs/bins/game-svr/src/lib.rs 与 smframe.rs `client_layout_probe_line`）：
#      ① spawn **之前**先验 KEY 已在调用方 env 且值 == VAL（不一致直接 exit 1，不派生）；
#      ② 日志里必须出现 `KEY=VAL`（词边界正则），MIR2_CLIENT_ABILITY_LAYOUT 还要求
#         生效布局 token 对上（0|false ⇒ layout=oracle-40，否则 ⇒ layout=client-58）；
#      ③ 任一不满足 ⇒ 非零退出并打印观测到的自证行。
#
# 用法（内置 profile 生成时拼装，见 tools/conformance/card_window.py 的 wmi_stack_argv）：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/start-stack-wmi.ps1 `
#     -Script E:/tmp/conformance/start-csharp3.ps1 -ReadyPorts 15000,17000,17100,17200
#   # + pin 自证（window-magic 的 rust 侧）：
#   #   -PinChecks MIR2_CLIENT_ABILITY_LAYOUT=0 -PinLog E:/tmp/conformance/rust-logs/game-svr.out.log
#   # 冒烟/自检（不起栈、不派生、只做 pin 校验）：-CheckPinsOnly；
#   # 不等端口（配合真服务端单进程冒烟）：-SkipPortWait。
#
# 退出码：0 = WMI 派生成功且 $ReadyPorts 全部监听并完成暖机（-SkipPortWait 时免），
#         且 -PinChecks 全部在调用方 env 与 server 日志自证通过；
#         1 = 派生失败 / 参数非法 / 超时后仍有端口未监听 / pin 预检或日志自证不一致。
#         **绝不**因为「部分端口就绪」或「pin 大概率到了」就返回 0。
param(
  [string]$Script = '',
  [string]$ReadyPorts = '',
  [int]$ReadyTimeoutSec = 180,
  [int]$WarmSeconds = 45,
  [string]$InnerArgs = '',
  [string]$PinChecks = '',
  [string]$PinLog = '',
  [int]$PinWaitSec = 30,
  [switch]$SkipPortWait,
  [switch]$CheckPinsOnly
)

$ErrorActionPreference = 'Stop'

# ---- pin 期望表：KEY=VAL 逗号分隔；空 = 不校验 ---------------------------------
$pinPairs = @()
foreach ($tok in @($PinChecks.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
  $eq = $tok.IndexOf('=')
  if ($eq -lt 1 -or $eq -ge ($tok.Length - 1)) {
    Write-Host ('start-stack-wmi: PinChecks 非法（应为 KEY=VAL 逗号分隔）：' + $tok)
    exit 1
  }
  $pinPairs += ,@($tok.Substring(0, $eq), $tok.Substring($eq + 1))
}
$pinLogs = @($PinLog.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ })
if ($pinPairs.Count -gt 0 -and $pinLogs.Count -eq 0) {
  Write-Host 'start-stack-wmi: 给了 -PinChecks 但没给 -PinLog ⇒ 无从校验，拒绝（fail-closed）'
  exit 1
}

# ---- 调用方到达预检（spawn 之前就 fail-closed）--------------------------------
# 期望的 pin 必须已在**本进程**环境里且值一致——本进程的 env 就是 card_window 调用面
# 传下来的（subprocess env=sides.<side>.env）；这里缺/不一致 ⇒ 转发不可能正确 ⇒ 不派生。
foreach ($p in $pinPairs) {
  $have = [Environment]::GetEnvironmentVariable($p[0])
  if ($null -eq $have) {
    Write-Host ('start-stack-wmi: pin ' + $p[0] + ' 不在调用方环境里——转发无从谈起（先修调用面/profile env），fail-closed')
    exit 1
  }
  if ($have -ne $p[1]) {
    Write-Host ('start-stack-wmi: pin ' + $p[0] + ' 期望 ' + $p[1] + '，调用方实际 ' + $have + ' ⇒ env 与 -PinChecks 不一致，fail-closed')
    exit 1
  }
}

if ($CheckPinsOnly) {
  Write-Host 'start-stack-wmi: -CheckPinsOnly（只做 pin 校验，不派生/不等端口）'
} else {
  if ([string]::IsNullOrWhiteSpace($Script)) {
    Write-Host 'start-stack-wmi: 缺 -Script（内层栈脚本路径）'
    exit 1
  }
  if (-not (Test-Path -LiteralPath $Script -PathType Leaf)) {
    Write-Host ('start-stack-wmi: 内层栈脚本不存在：' + $Script)
    exit 1
  }
}

# ---- env 显式转发清单（调用方全量环境 → EnvironmentValues）--------------------
$envPairs = New-Object System.Collections.Generic.List[string]
$mir2Fwd = New-Object System.Collections.Generic.List[string]
$cwFwd = New-Object System.Collections.Generic.List[string]
foreach ($de in @(Get-ChildItem -Path Env:)) {
  $v = [string]$de.Value
  if ($v -match "`r|`n") { continue }  # 多行值会破坏环境块结构：跳过（本链无此用例）
  $envPairs.Add($de.Name + '=' + $v)
  if ($de.Name -like 'MIR2_*') { $mir2Fwd.Add($de.Name) }
  elseif ($de.Name -like 'CARD_WINDOW_*') { $cwFwd.Add($de.Name) }
}
if ($envPairs.Count -eq 0) {
  Write-Host 'start-stack-wmi: 调用方环境为空，拒绝转发（fail-closed）'
  exit 1
}
# 基座填齐：EnvironmentVariables 是整体替换，若调用方 env 被裁剪到缺系统基座
# （正常链路不会；防御被 sanitize 过的 orchestrator 环境），补 Windows 约定值，
# 不然 server 子进程会因缺 SystemRoot/ComSpec 出现与 pin 无关的二次坏。
$baseDefaults = @{
  'SystemRoot' = $env:SystemRoot; 'windir' = $env:windir; 'ComSpec' = $env:ComSpec;
  'PATHEXT' = $env:PATHEXT; 'TEMP' = $env:TEMP; 'TMP' = $env:TMP; 'PATH' = $env:PATH }
$baseAdded = @()
foreach ($k in @('SystemRoot', 'windir', 'ComSpec', 'PATHEXT', 'TEMP', 'TMP', 'PATH')) {
  $have = [Environment]::GetEnvironmentVariable($k)
  if ([string]::IsNullOrEmpty($have)) {
    if (-not [string]::IsNullOrEmpty([string]$baseDefaults[$k])) {
      $envPairs.Add($k + '=' + [string]$baseDefaults[$k])
      $baseAdded += $k
    }
  }
}
if ($baseAdded.Count -gt 0) {
  Write-Host ('start-stack-wmi: 调用方 env 缺系统基座，已补齐：' + ($baseAdded -join ','))
}

# ---- pin 日志自证（-CheckPinsOnly / 派生后都走这里）---------------------------
function Get-LogText {
  # server 可能仍在写日志（冒烟/起栈期），FileShare.ReadWrite 容错读；
  # 瞬时占用异常 ⇒ 返回 $null（调用方本轮当作没读到，等下轮重试）。
  param([string]$Path)
  try {
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open,
      [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
      $sr = New-Object System.IO.StreamReader($fs, [System.Text.Encoding]::UTF8)
      try { return $sr.ReadToEnd() } finally { $sr.Dispose() }
    } finally { $fs.Dispose() }
  } catch { return $null }
}
function Test-PinInLog {
  param([string[]]$Logs, [string]$Key, [string]$Val)
  $pat = '(?<![0-9A-Za-z_])' + [regex]::Escape($Key) + '=' + [regex]::Escape($Val) + '(?![0-9A-Za-z_])'
  foreach ($log in $Logs) {
    if (-not (Test-Path -LiteralPath $log -PathType Leaf)) { continue }
    $text = Get-LogText -Path $log
    if ($null -eq $text) { continue }
    if ([regex]::IsMatch($text, $pat)) { return $log }
  }
  return $null
}
function Get-ExpectedLayoutToken {
  param([string]$Key, [string]$Val)
  if ($Key -ne 'MIR2_CLIENT_ABILITY_LAYOUT') { return $null }
  if ($Val -match '^(0|false)$') { return 'layout=oracle-40' }
  return 'layout=client-58'
}
function Assert-PinLogs {
  param([object[]]$Pairs, [string[]]$Logs, [int]$WaitSec)
  $deadline = (Get-Date).AddSeconds([Math]::Max(1, $WaitSec))
  $missing = $Pairs
  while ($true) {
    $missing = @($Pairs | Where-Object { -not (Test-PinInLog -Logs $Logs -Key $_[0] -Val $_[1]) })
    if ($missing.Count -eq 0) { break }
    if ((Get-Date) -ge $deadline) { break }
    Start-Sleep -Seconds 2
  }
  $failures = @()
  foreach ($p in $missing) {
    $observed = ''
    foreach ($log in $Logs) {
      if (-not (Test-Path -LiteralPath $log -PathType Leaf)) {
        $observed = '(日志不存在: ' + $log + ')'
        continue
      }
      $text = Get-LogText -Path $log
      if ($null -eq $text) { continue }
      $m = [regex]::Match($text, 'CLIENT_ABILITY_LAYOUT[^\r\n]{0,120}')
      if ($m.Success) { $observed = $m.Value; break }
    }
    if (-not $observed) { $observed = '(日志里没有自证行)' }
    $failures += ('pin ' + $p[0] + '=' + $p[1] + ' 未在 server 日志自证（观测: ' + $observed + '）')
  }
  foreach ($p in $Pairs) {
    $tok = Get-ExpectedLayoutToken -Key $p[0] -Val $p[1]
    if (-not $tok) { continue }
    $tokFound = $false
    foreach ($log in $Logs) {
      $text = Get-LogText -Path $log
      if ($null -ne $text -and $text.Contains($tok)) { $tokFound = $true; break }
    }
    if (-not $tokFound) {
      $failures += ('pin ' + $p[0] + '=' + $p[1] + ' 的生效布局应为 ' + $tok + '，日志未见该行')
    }
  }
  if ($failures.Count -gt 0) {
    foreach ($f in $failures) { Write-Host ('start-stack-wmi: ' + $f) }
    exit 1
  }
  Write-Host ('start-stack-wmi: pin 自证通过（' +
    (@($Pairs | ForEach-Object { $_[0] + '=' + $_[1] }) -join '；') + '）')
}

if (-not $CheckPinsOnly) {
  $ports = @()
  if (-not $SkipPortWait) {
    $ports = @($ReadyPorts.Split(',') | ForEach-Object { [int]($_.Trim()) } |
      Where-Object { $_ -gt 0 } | Select-Object -Unique)
    if ($ports.Count -eq 0) {
      Write-Host ('start-stack-wmi: ReadyPorts 非法（应为逗号分隔端口表）：' + $ReadyPorts)
      exit 1
    }
  }

  # WMI 派生（本包装自身由 card_window 捕获 stdout；内层栈从这里起就与该管道无关了）。
  # env 转发：WmiPrvSE 不继承调用方环境 ⇒ 用 Win32_ProcessStartup.EnvironmentValues
  # 显式下发全量（该属性是整体替换，故必须全量，见文件头 (3)）。
  $cmd = 'powershell -NoProfile -ExecutionPolicy Bypass -File "' + $Script + '"'
  if (-not [string]::IsNullOrWhiteSpace($InnerArgs)) { $cmd += ' ' + $InnerArgs }
  $fwdMsg = 'start-stack-wmi: env forwarding: ' + $envPairs.Count + ' vars (MIR2_*: ' + (($mir2Fwd | Sort-Object) -join ',') + '; CARD_WINDOW_*: ' + (($cwFwd | Sort-Object) -join ',') + ')'
  Write-Host $fwdMsg
  # 实现注记（2026-10-24 实测）：CIM cmdlet 路线（New-CimInstance -ClientOnly +
  # Invoke-CimMethod）对 Win32_ProcessStartup 的 string[] 属性序列化有 bug
  # （WBEM_E_NOT_FOUND: root\cimv2:Win32_Process:String）；[wmiclass].Create(短重载)
  # 对带 startup 的调用返回 rc=21。GetMethodParameters + InvokeMethod 三者皆通。
  $wmiProc = [wmiclass]'Win32_Process'
  $wmiArgs = $wmiProc.GetMethodParameters('Create')
  $wmiArgs['CommandLine'] = $cmd
  $startup = ([wmiclass]'Win32_ProcessStartup').CreateInstance()
  $startup.EnvironmentVariables = [string[]]$envPairs.ToArray()
  $wmiArgs['ProcessStartupInformation'] = $startup
  $r = $wmiProc.InvokeMethod('Create', $wmiArgs, $null)
  if ([int]$r['ReturnValue'] -ne 0) {
    Write-Host ('start-stack-wmi: WMI Win32_Process.Create 失败 rc=' + [int]$r['ReturnValue'])
    exit 1
  }
  Write-Host ('start-stack-wmi: wmi pid=' + $r['ProcessId'] + ' ready_ports=' + ($ports -join ','))

  if (-not $SkipPortWait) {
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
  } else {
    Write-Host 'start-stack-wmi: -SkipPortWait（不等端口；真服务端单进程冒烟/自检用）'
  }
}

if ($pinPairs.Count -gt 0) {
  Assert-PinLogs -Pairs $pinPairs -Logs $pinLogs -WaitSec $PinWaitSec
}
exit 0
