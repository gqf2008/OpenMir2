#requires -Version 5
<#
.SYNOPSIS
  capture_baseline.ps1 — 金标准抓包编排：影子网关 + 抓包代理 + 真实客户端流程。

.DESCRIPTION
  原则：E:\MirServer 与 D:\MirClient-run 只读不改（影子副本落在会话目录）。

  已知坑（2026-10-10 实机确认）：GameSvr 的网关表经不起「只重启网关」——
  旧网关断开会把槽位 UserList 置 null（M2Server/Net/TCP/TCPNetChannel.cs CloseGate），
  之后进世界的玩家在 SetGateUserList 上 NRE 死循环（NewHumanList 永远清不掉），
  客户端黑屏。因此本脚本必须做完整栈循环：
    1) start-all.ps1 -Stop            全停（含 MySQL）
    2) 起 MySQL → DBSrv → LoginSrv → GameSvr（与 start-all.ps1 同路径同顺序）
    3) 影子网关（端口 +10000，SelGate 端口是 IL 常量需打影子 DLL）+ 抓包代理占官方端口
    4) 真实客户端全流程（tools/client/mir_flow.ps1）
    5) start-all.ps1 -Stop → start-all.ps1  全量恢复正常栈
  无论成败都会执行第 5 步。

  产物（session 目录，默认 tests/golden/session-<时间戳>/）：
    proxy/            mir2_proxy 原始 dump（chunks.ndjson + conn-*.bin + session.json）
    frames.ndjson     逐帧表（seq/方向/端口/偏移/长度/sha256/ascii）
    manifest.json     金标准清单（N 帧、总 hash、复现命令）
    shots/            流程截图 + stages.ndjson 阶段时间线
    logs/             服务端日志切片 + 客户端 cl_trace 切片

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File capture_baseline.ps1
#>
param(
  [string]$GoldenRoot = "",       # 默认 <repo>/tests/golden
  [string]$Account    = "mir2test",
  [string]$Password   = "mir2pass",
  [string]$ServerPack = "E:\MirServer",
  [string]$Repo       = "E:\Users\gxh\Documents\GitHub\OpenMir2",
  [string]$Note       = "基线全程：登录→选人→进游戏→走路→小退→再进",
  [int]$WalkClicks    = 2,
  # 透传给 tools/client/mir_flow.ps1（建角 / 自动挂机覆盖阶段用）
  [switch]$CreateChar,
  [string]$CharName   = "",
  [int]$AutoPlaySec   = 0,
  [switch]$KeepShadow              # 保留影子网关副本（调试用；默认抓完删，副本含部署件二进制不入库）
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
if ($GoldenRoot -eq "") { $GoldenRoot = Join-Path $RepoRoot "tests\golden" }

$ProxyPy    = Join-Path $Here "mir2_proxy.py"
$SegmentPy  = Join-Path $Here "segment_frames.py"
$VerifyPy   = Join-Path $Here "verify_golden.py"
$FlowPs1    = Join-Path $RepoRoot "tools\client\mir_flow.ps1"
$StartAll   = Join-Path $ServerPack "start-all.ps1"
$MySqlHome  = 'D:\mysql\mariadb-10.11.19-winx64'
$MySqlData  = 'D:\mysql\data'
$LogDir     = Join-Path $ServerPack 'logs'

# 核心服务（与 start-all.ps1 同路径同顺序；网关由影子逻辑接管）
$CoreServers = @(
  @{ Name = 'DBSrv';    Dir = "$Repo\src\DBSrv\bin\Release";    Exe = 'DBSrv.exe';    Ports = @(6000, 5100, 5700) },
  @{ Name = 'LoginSrv'; Dir = "$Repo\src\LoginSrv\bin\Release"; Exe = 'LoginSrv.exe'; Ports = @(5500, 5600) },
  @{ Name = 'GameSvr';  Dir = "$ServerPack\M2GameSvr";          Exe = 'GameSrv.exe';  Ports = @(5000) }
)
# 网关定义：部署目录、exe、正式端口；影子的绑法见下方注释
$Gates = @(
  @{ Name = "LoginGate"; Exe = "LoginGate.exe"; Ports = @(7000) },
  @{ Name = "SelGate";   Exe = "SelGate.exe";   Ports = @(7100) },
  @{ Name = "RunGate";   Exe = "GameGate.exe";  Ports = @(7200, 7201, 7202) }
)
# 影子网关的插桩方式（2026-10-10 实测定型）：
#   SelGate —— 监听端口是编译期常量（GateShare.GatePort）且绑 IPAddress.Any，
#     config 里的 GatePort0 是死配置 ⇒ 只能改影子 DLL 的 IL 常量到 17100。
#   LoginGate / RunGate —— 改影子 config 的**监听端口** +10000（绑定地址保持 127.0.0.1）。
# 已知代价（写在这里免得后人重踩）：RunGate 挪到 17200 后，GameSvr 侧「进世界」的路由
# 按网关端口对齐（!servertable.txt 发给客户端的端口就是 7200），客户端能连上、收得到公告，
# 但回完 CM_LOGINNOTICEOK 之后世界数据再也下不来（抓包里 7200 跳 s2c 只有 445B 的公告）。
# 因此本拓扑抓到的金标准覆盖到「进世界（公告确认）」为止。
# 试过但**未走通**的替代拓扑（写在交付说明里，别人别再从头试）：把影子网关绑到 127.0.0.2、
# 端口保持原值、代理占 127.0.0.1 原端口 —— 理论上端口语义不变、世界数据应能下来，
# 实测卡在「影子 RunGate 仍占着 127.0.0.1:7200、代理绑不上」这一步
# （GameGate 的 GateAddress* 只有一个键，另外两个 Gate 条目回落默认 127.0.0.1）。
# ⇒ 要补齐 移动/攻击/小退 阶段，正路是让 oracle 自己就按可插桩的端口跑（见交付说明"解除判据"）。
$ShadowBindAddr = "127.0.0.1"
$ProxyMap = @{ 7000 = "127.0.0.1:17000"; 7100 = "127.0.0.1:17100"; 7200 = "127.0.0.1:17200" }

function Test-PortListen([int]$Port) {
  # 纯 .NET 查监听表：不开连接（TCP 连上去探活会在 GameGate 造幻影用户并触发
  # GameSvr 的 SetGateUserList NRE），不走 WMI（Get-NetTCPConnection 循环偶发挂死），
  # 也不 spawn netstat（流水线实测在收尾阶段卡住过）。
  try {
    foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
      if ($ep.Port -eq $Port) { return $true }
    }
  } catch { }
  return $false
}
function Wait-PortListen([int]$Port, [int]$TimeoutSec = 60) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if (Test-PortListen $Port) { return $true }
    Start-Sleep -Milliseconds 500
  }
  return $false
}
function Start-CoreServer($c) {
  $out = Join-Path $LogDir ($c.Name + '.out.log')
  $err = Join-Path $LogDir ($c.Name + '.err.log')
  $exe = Join-Path $c.Dir $c.Exe
  Start-Process -FilePath $exe -WorkingDirectory $c.Dir `
    -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle Hidden | Out-Null
  foreach ($port in $c.Ports) {
    if (-not (Wait-PortListen $port 90)) { throw "$($c.Name) 端口 $port 90 秒未监听（看 $err）" }
  }
  Write-Output ("CORE_UP " + $c.Name)
}
function Stop-ShadowGates {
  param([string]$ShadowRoot)
  $procs = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($ShadowRoot, [StringComparison]::OrdinalIgnoreCase) }
  foreach ($p in $procs) {
    Write-Output ("STOP shadow " + $p.ExecutablePath + " pid=" + $p.ProcessId)
    Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
  }
}
function Patch-GatePorts([string]$ConfPath) {
  # config.conf 是 GBK：按字节读写，避免 PS 默认编码吃掉中文 Title。
  # 只改**监听端口**（GatePort*）+10000；绑定地址与上游地址（ServerAddr*）一律不动——
  # LoginGate/RunGate 的 ServerAddr* 是它们各自连后端（LoginSrv / GameSvr）的地址，
  # 改了会把影子网关自己连丢（现象：影子网关刷"链接…拒绝链接"、客户端对应跳零字节）。
  $gbk = [System.Text.Encoding]::GetEncoding(936)
  $text = [System.IO.File]::ReadAllText($ConfPath, $gbk)
  $patched = [regex]::Replace($text, '(?m)^(GatePort\d*=)(\d+)\s*$', {
    param($m)
    $m.Groups[1].Value + ([int]$m.Groups[2].Value + 10000)
  })
  if ($patched -eq $text) { throw "config.conf 没找到 GatePort 行: $ConfPath" }
  [System.IO.File]::WriteAllText($ConfPath, $patched, $gbk)
  Write-Output ("PORT_PATCH " + (Split-Path (Split-Path $ConfPath -Parent) -Leaf) + " GatePort* +10000")
}
function Count-DllPortConst([string]$DllPath, [int]$Port) {
  $bytes = [System.IO.File]::ReadAllBytes($DllPath)
  $pat = [byte[]]@(0x20) + [BitConverter]::GetBytes($Port)   # ldc.i4 <port>
  $count = 0
  for ($i = 0; $i -le $bytes.Length - 5; $i++) {
    if ($bytes[$i] -eq $pat[0] -and $bytes[$i+1] -eq $pat[1] -and
        $bytes[$i+2] -eq $pat[2] -and $bytes[$i+3] -eq $pat[3] -and
        $bytes[$i+4] -eq $pat[4]) { $count++; $i += 4 }
  }
  return $count
}
function Patch-DllPortConst([string]$DllPath, [int]$From, [int]$To, [int]$ExpectedCount) {
  # SelGate 的监听端口是编译期常量（GateShare.GatePort，src/SelGate/GateShare.cs:15），
  # config.conf 的 GatePort0 是死配置。只能在影子副本的 DLL 里改 IL 常量。原部署文件不动。
  $bytes = [System.IO.File]::ReadAllBytes($DllPath)
  $fromBytes = [byte[]]@(0x20) + [BitConverter]::GetBytes($From)
  $toBytes   = [byte[]]@(0x20) + [BitConverter]::GetBytes($To)
  $count = 0
  for ($i = 0; $i -le $bytes.Length - 5; $i++) {
    if ($bytes[$i] -eq $fromBytes[0] -and $bytes[$i+1] -eq $fromBytes[1] -and
        $bytes[$i+2] -eq $fromBytes[2] -and $bytes[$i+3] -eq $fromBytes[3] -and
        $bytes[$i+4] -eq $fromBytes[4]) {
      for ($j = 0; $j -lt 5; $j++) { $bytes[$i+$j] = $toBytes[$j] }
      $count++
      $i += 4
    }
  }
  if ($count -ne $ExpectedCount) {
    throw "DLL 端口常量替换数不符: $DllPath 期望 $ExpectedCount 处 $From，实际 $count 处（部署版本变了？先核对 IL）"
  }
  [System.IO.File]::WriteAllBytes($DllPath, $bytes)
  Write-Output ("DLL_PATCH " + (Split-Path $DllPath -Leaf) + " port " + $From + "->" + $To + " (" + $count + " sites)")
}
function Invoke-StartAll([string[]]$ExtraArgs, [int]$TimeoutSec = 300) {
  # start-all.ps1 内部用 Get-NetTCPConnection 轮询端口，WMI 偶发卡死会让它一直不退出；
  # 这里给硬超时，超时就按端口实况继续（栈大概率已经起来了）。
  $argList = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $StartAll) + $ExtraArgs
  $tag = if ($ExtraArgs.Count -gt 0) { "stop" } else { "start" }
  $p = Start-Process -FilePath "powershell" -ArgumentList $argList -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $Sess ($tag + "all.out.log")) `
    -RedirectStandardError  (Join-Path $Sess ($tag + "all.err.log"))
  if (-not $p.WaitForExit($TimeoutSec * 1000)) {
    Write-Output ("WARN: start-all（" + ($ExtraArgs -join ' ') + "）超过 " + $TimeoutSec + " 秒未退出，脚本自身卡住；按端口实况继续")
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  }
}
function Stop-StackProcessesByPath {
  # start-all -Stop 自己会因 WMI 轮询卡住（已被硬超时兜住），卡住时进程还活着。
  # 这里按**绝对路径**兜底强杀本栈组件（只认本部署的 exe，不误伤别人起的同名进程）。
  $exes = @((Join-Path $MySqlHome 'bin\mysqld.exe'))
  foreach ($g in $Gates) { $exes += (Join-Path $ServerPack "$($g.Name)\$($g.Exe)") }
  foreach ($c in $CoreServers) { $exes += (Join-Path $c.Dir $c.Exe) }
  $procs = Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
    Where-Object { $_.ExecutablePath -and ($exes -contains $_.ExecutablePath) }
  foreach ($p in $procs) {
    Write-Output ("FORCE_KILL " + $p.ExecutablePath + " pid=" + $p.ProcessId)
    Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
  }
  Start-Sleep -Seconds 2
}
function Assert-ProxyPortsFree([int[]]$Ports, [string]$Why) {
  # 代理绑 127.0.0.1:<port>；影子网关在同一端口但绑 127.0.0.2，不算冲突。
  # 因此这里只判定「127.0.0.1 或 0.0.0.0 上是否已有人占」。
  $listen = [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()
  foreach ($p in $Ports) {
    $busy = $listen | Where-Object { $_.Port -eq $p -and ($_.Address.ToString() -eq "127.0.0.1" -or $_.Address.ToString() -eq "0.0.0.0") }
    if ($busy) { throw "$Why：127.0.0.1:$p 仍被占用（该被停掉的原网关还在？）" }
  }
}
function Stop-EntireStack {
  Invoke-StartAll @("-Stop") 180
  Stop-StackProcessesByPath
  Start-Sleep -Seconds 3
}
function Start-EntireStack {
  Invoke-StartAll @() 300
}

# ---------- 预检 ----------
foreach ($p in @(7000, 7100, 7200, 5500, 5600, 5100, 5000, 3306)) {
  if (-not (Test-PortListen $p)) { throw "预检失败：端口 $p 未监听（服务端没起全？先跑 E:\MirServer\start-all.cmd）" }
}
foreach ($p in @(17000, 17100, 17200)) {
  if (Test-PortListen $p) { throw "预检失败：影子端口 $p 已被占用（上次没恢复？先清理）" }
}
$selDll = Join-Path $ServerPack "SelGate\SelGate.dll"
$selHits = Count-DllPortConst $selDll 7100
if ($selHits -ne 3) { throw "SelGate.dll 中 ldc.i4 7100 有 $selHits 处（预期 3）——部署版本变了，先人工核对再打影子" }

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$Sess = Join-Path $GoldenRoot "session-$stamp"
$ShadowRoot = Join-Path $Sess "shadow"
$ProxyOut = Join-Path $Sess "proxy"
New-Item -ItemType Directory -Force -Path $Sess | Out-Null

$proxyProc = $null

try {
  # ---------- 1. 全停 ----------
  Write-Output "STOP_ALL"
  Stop-EntireStack

  # ---------- 2. 起核心服务 ----------
  if (-not (Test-PortListen 3306)) {
    Start-Process -FilePath (Join-Path $MySqlHome 'bin\mysqld.exe') `
      -ArgumentList "--datadir=$MySqlData", '--port=3306', '--bind-address=127.0.0.1', '--console' `
      -RedirectStandardOutput (Join-Path $LogDir 'MySQL.out.log') `
      -RedirectStandardError  (Join-Path $LogDir 'MySQL.err.log') -WindowStyle Hidden | Out-Null
    if (-not (Wait-PortListen 3306 60)) { throw "MySQL 60 秒未起来" }
    Write-Output "CORE_UP MySQL"
  }
  # 建角用的 scratch 账号：必须在起 LoginSrv 之前建好——LoginSrv 只在启动时把账号读进内存，
  # 后建的账号登录一律报「此帐号不存在，或出现未知错误」
  if ($CreateChar) {
    if ($Account.Length -gt 10) {
      # 客户端登录框把账号截断到 10 字符（实测：15 字符的 scratch 账号被截成前 10 位 → 登录失败）
      throw "账号 '$Account' 超过 10 字符：客户端会截断，请用 ≤10 字符的 scratch 账号"
    }
    $Provision = Join-Path $RepoRoot "tools\client\account_provision.ps1"
    & powershell -NoProfile -ExecutionPolicy Bypass -File $Provision -Account $Account -Password $Password |
      Tee-Object -FilePath (Join-Path $Sess "provision.log")
    if ($LASTEXITCODE -ne 0) { throw "scratch 账号开号失败" }
  }
  foreach ($c in $CoreServers) { Start-CoreServer $c }

  # ---------- 3. 影子网关 ----------
  foreach ($g in $Gates) {
    $src = Join-Path $ServerPack $g.Name
    $dst = Join-Path $ShadowRoot $g.Name
    New-Item -ItemType Directory -Force -Path $dst | Out-Null
    robocopy $src $dst /E /XF "*.log" /XD logs /NFL /NDL /NJH /NJS | Out-Null
    if ($LASTEXITCODE -gt 7) { throw "robocopy 失败 ($LASTEXITCODE): $src" }
    Patch-GatePorts (Join-Path $dst "config.conf")
    if ($g.Name -eq "SelGate") {
      Patch-DllPortConst (Join-Path $dst "SelGate.dll") 7100 17100 3
    }
    Start-Process -FilePath (Join-Path $dst $g.Exe) -WorkingDirectory $dst `
      -RedirectStandardOutput (Join-Path $Sess ("shadow-" + $g.Name + ".out.log")) `
      -RedirectStandardError  (Join-Path $Sess ("shadow-" + $g.Name + ".err.log")) -WindowStyle Hidden | Out-Null
    Write-Output ("SHADOW " + $g.Name + " -> " + $dst)
  }
  foreach ($p in @(17000, 17100, 17200)) {
    if (-not (Wait-PortListen $p 60)) { throw "影子网关端口 $p 60 秒未起来（看 shadow-*.err.log）" }
  }
  Write-Output "SHADOW_GATES_UP"

  # ---------- 4. 抓包代理 ----------
  # 代理要占官方端口：此刻必须真的空着（原网关已停、影子在别的地址上）
  Assert-ProxyPortsFree @(7000, 7100, 7200) "起代理前"
  New-Item -ItemType Directory -Force -Path $ProxyOut | Out-Null
  $mapArgs = @()
  foreach ($k in $ProxyMap.Keys) { $mapArgs += @("--map", "$k=$($ProxyMap[$k])") }
  $proxyProc = Start-Process -FilePath "python" -PassThru -WindowStyle Hidden `
    -ArgumentList (@($ProxyPy, "--out", $ProxyOut) + $mapArgs) `
    -RedirectStandardOutput (Join-Path $Sess "proxy.out.log") `
    -RedirectStandardError  (Join-Path $Sess "proxy.err.log")
  $ready = Join-Path $ProxyOut "ready.flag"
  $deadline = (Get-Date).AddSeconds(30)
  while (-not (Test-Path $ready) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
  if (-not (Test-Path $ready)) { throw "抓包代理 30 秒未就绪" }
  Write-Output "PROXY_UP pid=$($proxyProc.Id)"

  # ---------- 5. 记录日志切点 ----------
  $logMarks = @{}
  foreach ($f in (Get-ChildItem $LogDir -Filter *.out.log -ErrorAction SilentlyContinue)) {
    $logMarks[$f.FullName] = $f.Length
  }
  $clTrace = "D:\MirClient-run\cl_trace.txt"
  $clMark = 0
  if (Test-Path $clTrace) { $clMark = (Get-Item $clTrace).Length }

  # ---------- 6. 跑客户端流程 ----------
  $flowOut = Join-Path $Sess "shots"
  $flowArgs = @("-Account", $Account, "-Password", $Password, "-OutDir", $flowOut, "-WalkClicks", "$WalkClicks")
  if ($CreateChar) { $flowArgs += @("-CreateChar", "-CharName", $CharName) }
  if ($AutoPlaySec -gt 0) { $flowArgs += @("-AutoPlaySec", "$AutoPlaySec") }
  & powershell -NoProfile -ExecutionPolicy Bypass -File $FlowPs1 @flowArgs |
    Tee-Object -FilePath (Join-Path $Sess "flow.out.log")
  if ($LASTEXITCODE -ne 0) { throw "客户端流程脚本失败（$LASTEXITCODE），见 flow.out.log" }
  Start-Sleep -Seconds 3   # 等收尾流量落盘
}
finally {
  # ---------- 7. 恢复：停影子+核心，全量起正常栈 ----------
  if ($proxyProc -and -not $proxyProc.HasExited) {
    Stop-Process -Id $proxyProc.Id -Force -ErrorAction SilentlyContinue
  }
  Stop-ShadowGates -ShadowRoot $ShadowRoot
  Start-Sleep -Seconds 2
  Stop-EntireStack
  Write-Output "RESTORE: start-all"
  Start-EntireStack
  # 影子副本是部署件拷贝（含 E:\MirServer 的二进制），抓完即删；要留档加 -KeepShadow
  if (-not $KeepShadow -and (Test-Path $ShadowRoot)) {
    Remove-Item -Recurse -Force $ShadowRoot -ErrorAction SilentlyContinue
    Write-Output "SHADOW_CLEANED"
  }
  $ok = $true
  foreach ($p in @(3306, 5100, 5500, 5000, 7000, 7100, 7200)) {
    if (-not (Wait-PortListen $p 90)) { $ok = $false; Write-Output "RESTORE_WARN: 端口 $p 未恢复" }
  }
  if ($ok) { Write-Output "RESTORED: 完整栈已恢复" }
}

# ---------- 8. 切帧 + 校验 + 收日志 ----------
python $SegmentPy --session $ProxyOut --out $Sess --note $Note `
  --reproduce "powershell -ExecutionPolicy Bypass -File tools/capture/capture_baseline.ps1"
if ($LASTEXITCODE -ne 0) { throw "segment_frames 失败" }

$logsOut = Join-Path $Sess "logs"
New-Item -ItemType Directory -Force -Path $logsOut | Out-Null
foreach ($src in $logMarks.Keys) {
  if (-not (Test-Path $src)) { continue }
  $len = (Get-Item $src).Length - $logMarks[$src]
  if ($len -gt 0) {
    $fs = [System.IO.File]::Open($src, 'Open', 'Read', 'ReadWrite')
    try {
      $fs.Seek($logMarks[$src], 'Begin') | Out-Null
      $buf = New-Object byte[] $len
      [void]$fs.Read($buf, 0, $len)
      [System.IO.File]::WriteAllBytes((Join-Path $logsOut (Split-Path $src -Leaf)), $buf)
    } finally { $fs.Close() }
  }
}
if (Test-Path $clTrace) {
  $len = (Get-Item $clTrace).Length - $clMark
  if ($len -gt 0) {
    $fs = [System.IO.File]::Open($clTrace, 'Open', 'Read', 'ReadWrite')
    try {
      $fs.Seek($clMark, 'Begin') | Out-Null
      $buf = New-Object byte[] $len
      [void]$fs.Read($buf, 0, $len)
      [System.IO.File]::WriteAllBytes((Join-Path $logsOut "cl_trace.txt"), $buf)
    } finally { $fs.Close() }
  }
}

python $VerifyPy --golden $Sess
if ($LASTEXITCODE -ne 0) { throw "金标准自检失败（见上 RED 行）" }

# 指向最新会话（写仓库相对路径：绝对路径带工作树名，换机器/换 worktree 就失效）
$relSess = (Resolve-Path -Relative $Sess) -replace '^\.\\', '' -replace '\\', '/'
Set-Content -Path (Join-Path $GoldenRoot "LATEST") -Value $relSess -Encoding ascii
Write-Output ("GOLDEN_OK " + $Sess)
