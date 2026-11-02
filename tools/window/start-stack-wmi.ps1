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
# (5) **不把 `-ExecutionPolicy` 传给内层 powershell**（卡 openmir2-window-wmi-starter-fix，
#      2026-11-02）：本机 HIPS（火绒 HipsDaemon）对「WMI 派生 + powershell.exe
#      -ExecutionPolicy …」返回 Win32 错误 5（"Access is denied."）——`Win32_Process.Create`
#      仍回 0 并给出 pid，但内层脚本一行都没跑（WMI 尝试期间 start-csharp.step.log 无新的
#      `BEGIN roles`），旧实现只会静默等满 ReadyTimeoutSec + WarmSeconds，最后只报
#      "ready ports missing"（card_window 的 csharp/stack 步 rc=1 的真因）。复现：直连
#      `powershell -NoProfile -ExecutionPolicy Bypass -File …` rc=0；同一命令经 WMI 派生则
#      脚本不执行；把策略改由 `PSExecutionPolicyPreference=Bypass` 环境变量下发
#      （等价：`Get-ExecutionPolicy -Scope Process` = Bypass）后经 WMI 正常执行。
#      所以内层命令只留 `-NoProfile -File`；WMI 派生本身（断 stdout 继承、防死锁）不变。
#      派生失败的牙：Create 后立刻取进程句柄，进程在就绪端口前退出 ⇒ 立即 rc=1 并点名
#      pid + exit code + 缺失端口，不再静默等超时；`-Selftest` 复跑这组负例/正例。
#
# 用法（内置 profile 生成时拼装，见 tools/conformance/card_window.py 的 wmi_stack_argv）：
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/start-stack-wmi.ps1 `
#     -Script E:/tmp/conformance/start-csharp3.ps1 -ReadyPorts 15000,17000,17100,17200
#   # + pin 自证（window-magic 的 rust 侧）：
#   #   -PinChecks MIR2_CLIENT_ABILITY_LAYOUT=0 -PinLog E:/tmp/conformance/rust-logs/game-svr.out.log
#   # 冒烟/自检（不起栈、不派生、只做 pin 校验）：-CheckPinsOnly；
#   # 不等端口（配合真服务端单进程冒烟）：-SkipPortWait；
#   # 派生链牙（3 负例 + 1 正例，合成夹具，不碰真实端口/栈）：-Selftest；
#   # 负例端口运行时取空闲端口（本机固定端口被占也不假红），正例还断言内层
#   # `Get-ExecutionPolicy -Scope Process` = Bypass（删 PSExecutionPolicyPreference
#   # 转发 env 即红）——见下方 -Selftest 段与卡 `openmir2-wmi-selftest-registry`。
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
  [switch]$CheckPinsOnly,
  [switch]$Selftest
)

$ErrorActionPreference = 'Stop'

# ---- -Selftest：派生失败必须立刻点名（负例）+ 修好后内层脚本确实执行（正例）---------
# 只跑合成夹具（临时目录里的极小 .ps1），不碰真实端口/真实栈；子调用都走本脚本自身，
# 因此覆盖的正是「WMI 派生 + 就绪端口」这条链。子调用**不带** -ExecutionPolicy：本机
# HIPS 会拦 WMI 派生链上的该开关（见文件头 (5)）。
if ($Selftest) {
  $psExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  if (-not (Test-Path -LiteralPath $psExe -PathType Leaf)) { $psExe = 'powershell' }
  $self = $PSCommandPath
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('wmi-wrapper-selftest-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  $fails = New-Object System.Collections.Generic.List[string]
  function Write-TmpScript([string]$name, [string]$body) {
    $p = Join-Path $tmp $name
    [System.IO.File]::WriteAllText($p, $body, (New-Object System.Text.UTF8Encoding($false)))
    return $p
  }
  function Invoke-Wrapper([string]$inner, [string]$ports, [int]$timeout) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $out = & $psExe -NoProfile -File $self -Script $inner -ReadyPorts $ports `
      -ReadyTimeoutSec $timeout -WarmSeconds 0 2>&1
    $rc = $LASTEXITCODE
    $sw.Stop()
    return ,@($rc, $sw.Elapsed.TotalSeconds, (($out | ForEach-Object { [string]$_ }) -join "`n"))
  }
  function Assert-Case([bool]$cond, [string]$name, [string]$detail) {
    if ($cond) { Write-Host ('start-stack-wmi selftest: PASS ' + $name) }
    else {
      Write-Host ('start-stack-wmi selftest: FAIL ' + $name + ' :: ' + $detail)
      $script:fails.Add($name)
    }
  }
  # 负例端口**运行时取空闲端口**（旧实现写死 65000/65001：本机一旦有别的进程占用，
  # 「端口未就绪」这个前提就假掉，负例会假红/假绿）。绑 0 让内核分配，拿到后立刻释放。
  # 注意：Windows 允许非独占 socket 复用地址/端口，分配器**可能返回别人正在监听的端口**
  # （实测：65000/65001 被占用时 bind(0) 仍返回它们）⇒ 再用 Get-NetTCPConnection 显式排除并重试。
  function Get-FreeTcpPort {
    for ($i = 0; $i -lt 40; $i++) {
      $listener = New-Object System.Net.Sockets.TcpListener -ArgumentList ([System.Net.IPAddress]::Loopback), 0
      $listener.Start()
      $p = 0
      try { $p = [int]$listener.LocalEndpoint.Port } finally { $listener.Stop() }
      $busy = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
        Where-Object { $_.LocalPort -eq $p })
      if ($busy.Count -eq 0) { return $p }
    }
    throw 'Get-FreeTcpPort: 40 次都没有拿到未被监听的端口'
  }
  try {
    # N1 「Create 回 0 但进程立即消失」——真实事故形态：必须快速 rc=1 并点名退出，不得静默超时。
    $n1 = Write-TmpScript 'inner-instant.ps1' 'exit 7'
    $r = Invoke-Wrapper $n1 ([string](Get-FreeTcpPort)) 60
    Assert-Case ($r[0] -eq 1 -and $r[1] -lt 30 -and $r[2] -match '已退出|在就绪前退出') `
      'negative: instant death fails fast' ('rc=' + $r[0] + ' sec=' + [int]$r[1] + ' out=' + $r[2])

    # N2 内层延时非零退出：必须点名 exit=7，且仍在牙的时限内（不静默等满 60s）。
    $n2 = Write-TmpScript 'inner-exit7.ps1' ("Start-Sleep -Seconds 1" + "`n" + 'exit 7')
    $r = Invoke-Wrapper $n2 ([string](Get-FreeTcpPort)) 60
    Assert-Case ($r[0] -eq 1 -and $r[1] -lt 30 -and $r[2] -match 'exit=7') `
      'negative: nonzero exit names exit=7' ('rc=' + $r[0] + ' sec=' + [int]$r[1] + ' out=' + $r[2])

    # N3 内层 exit 0 但端口未就绪：不得因 exit=0 就当成功，必须 rc=1 且点名 exit=0。
    $n3 = Write-TmpScript 'inner-exit0.ps1' ("Start-Sleep -Seconds 1" + "`n" + 'exit 0')
    $r = Invoke-Wrapper $n3 ([string](Get-FreeTcpPort)) 60
    Assert-Case ($r[0] -eq 1 -and $r[1] -lt 30 -and $r[2] -match 'exit=0') `
      'negative: exit 0 without ports still fails' ('rc=' + $r[0] + ' sec=' + [int]$r[1] + ' out=' + $r[2])

    # P1 正例：内层脚本确实被执行（写 marker），且**进程作用域**策略 = Bypass。
    # 只验 marker 存在的话，删掉 wrapper 里的 `PSExecutionPolicyPreference` 转发仍然绿
    # （review 8f0d2433 Minor 1 的缺口）；这里让内层自己读 Get-ExecutionPolicy -Scope Process
    # 落盘再断言，删转发 ⇒ Undefined ⇒ 必红。
    $marker = Join-Path $tmp 'marker.txt'
    $policyFile = Join-Path $tmp 'policy.txt'
    $p1Body = ("Set-Content -LiteralPath '" + $marker + "' -Value OK") + "`n" +
      ("Set-Content -LiteralPath '" + $policyFile + "' -Value (Get-ExecutionPolicy -Scope Process)")
    $p1 = Write-TmpScript 'inner-marker.ps1' $p1Body
    $out = & $psExe -NoProfile -File $self -Script $p1 -ReadyPorts 0 -SkipPortWait 2>&1
    $rc = $LASTEXITCODE
    $deadline = (Get-Date).AddSeconds(10)
    while ((-not (Test-Path -LiteralPath $marker) -or -not (Test-Path -LiteralPath $policyFile)) -and
      (Get-Date) -lt $deadline) {
      Start-Sleep -Milliseconds 200
    }
    $policy = ''
    if (Test-Path -LiteralPath $policyFile) {
      $policy = ([string](Get-Content -LiteralPath $policyFile -Raw)).Trim()
    }
    Assert-Case ($rc -eq 0 -and (Test-Path -LiteralPath $marker) -and $policy -eq 'Bypass') `
      'positive: inner ran under WMI with process-scope Bypass' `
      ('rc=' + $rc + ' marker=' + (Test-Path -LiteralPath $marker) + ' policy=' + $policy)
  } finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }
  if ($fails.Count -gt 0) {
    Write-Host ('start-stack-wmi selftest: RED (' + $fails.Count + ' failed)')
    exit 1
  }
  Write-Host 'start-stack-wmi selftest: GREEN (3 negative + 1 positive)'
  exit 0
}

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
  # 2026-11-02（卡 openmir2-window-wmi-starter-fix）：**不传 `-ExecutionPolicy`**——
  # 本机 HIPS 对 WMI 派生的 `powershell -ExecutionPolicy …` 直接拒绝（Win32 error 5，
  # Create 仍回 0 但内层脚本未执行）。同一 process-scope 策略改由
  # PSExecutionPolicyPreference 环境变量下发（等价，见文件头 (5)）。
  $envPairs = @($envPairs | Where-Object { $_ -notmatch '^PSExecutionPolicyPreference=' })
  $envPairs += 'PSExecutionPolicyPreference=Bypass'
  $psExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  if (-not (Test-Path -LiteralPath $psExe -PathType Leaf)) { $psExe = 'powershell' }
  $cmd = '"' + $psExe + '" -NoProfile -File "' + $Script + '"'
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
  $startup.EnvironmentVariables = [string[]]$envPairs
  $wmiArgs['ProcessStartupInformation'] = $startup
  $r = $wmiProc.InvokeMethod('Create', $wmiArgs, $null)
  if ([int]$r['ReturnValue'] -ne 0) {
    Write-Host ('start-stack-wmi: WMI Win32_Process.Create 失败 rc=' + [int]$r['ReturnValue'])
    exit 1
  }
  $wmiPid = [int]$r['ProcessId']
  Write-Host ('start-stack-wmi: wmi pid=' + $wmiPid + ' ready_ports=' + ($ports -join ','))

  if (-not $SkipPortWait) {
    # 派生失败的牙（2026-11-02）：Create 返回 0 只证明「进程被创建」，不证明内层脚本在跑
    # （本机 HIPS 拦 -ExecutionPolicy 时正是 Create=0/有 pid、内层一行未执行）。立刻拿
    # 进程句柄；进程在就绪端口前退出 ⇒ 立即 rc=1 并点名 pid/exit code/缺失端口，绝不再
    # 静默等满 ReadyTimeoutSec + WarmSeconds 后才报 "ready ports missing"。
    $innerProc = $null
    try { $innerProc = [System.Diagnostics.Process]::GetProcessById($wmiPid) } catch { }
    if ($null -eq $innerProc) {
      Write-Host ('start-stack-wmi: WMI 派生进程 pid=' + $wmiPid +
        ' 已退出（Create 返回 0 但进程不存在）⇒ 内层脚本未执行；ready ports missing: ' +
        ($ports -join ','))
      exit 1
    }
    # 立刻打开进程句柄：.NET 的 ExitCode 只在句柄已打开时可用（否则进程退出后读到 $null）。
    try { $null = $innerProc.Handle } catch { }
    $deadline = (Get-Date).AddSeconds([Math]::Max(1, $ReadyTimeoutSec))
    while ((Get-Date) -lt $deadline) {
      $listening = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
        Where-Object { $ports -contains $_.LocalPort } |
        Select-Object -ExpandProperty LocalPort -Unique)
      if ($listening.Count -ge $ports.Count) { break }
      $exited = $false
      try { $exited = $innerProc.HasExited } catch { $exited = $true }
      if ($exited) {
        # 进程可能刚起完端口就正常退出：再测一次，端口齐了就算成功。
        $listening = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
          Where-Object { $ports -contains $_.LocalPort } |
          Select-Object -ExpandProperty LocalPort -Unique)
        if ($listening.Count -ge $ports.Count) { break }
        $code = 'unknown'
        try { if ($innerProc.HasExited) { $code = $innerProc.ExitCode } } catch { }
        $missing = @($ports | Where-Object { $listening -notcontains $_ }) -join ','
        Write-Host ('start-stack-wmi: WMI 派生进程 pid=' + $wmiPid + ' 在就绪前退出（exit=' +
          $code + '）⇒ 内层脚本失败；ready ports missing: ' + $missing)
        exit 1
      }
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
