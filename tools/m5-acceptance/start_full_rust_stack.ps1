#requires -Version 5
<#
.SYNOPSIS
  start_full_rust_stack.ps1 — 起**含 game-svr 的全 Rust 栈（六件）**。
  组合 = MySQL(3306) + Rust db-srv(6000/5100/5700) + Rust login-srv(5500/5600) + Rust game-svr(5000)
       + Rust login-gate(7000) + Rust sel-gate(7100) + Rust game-gate(7200/7201/7202)。

.DESCRIPTION
  这是 M5④「Rust 服务逐个替换」的**终点组合**起栈器（灰度阶梯见 `stage_gray_rollback.ps1`）。
  骨架照 `tools/m2-acceptance/start_rust_stack.ps1`（Test-Port / Get-ListenPids / Show-Status / PidFile /
  -Stop / -Status / -Force、fail-closed PREREQ_MISSING、端口占用即 exit 1、逐端口就绪检查），差异只有两点：

    ① DBSrv 用 Rust `db-srv.exe`（6000/5100/5700 三面）、LoginSrv 用 Rust `login-srv.exe`（5500/5600）；
    ② Rust db-srv / login-srv / 三网关的配置写到 `-StackRoot` **影子副本**（照
       `tools/m1-acceptance/start_boundary_stack.ps1`），不碰 `E:\MirServer` 的配置文件。

  **为什么现在能全 Rust（旧注释已过时）**：`tools/m2-acceptance/start_rust_stack.ps1:19-23` 头注释说
  「Rust `bins/db-srv` 只实现了 5100 面」，**已过时**——Rust db-srv 的 6000/5700 两面均已落地：
    · `mir2-rs/bins/db-srv/src/main.rs:83-87`：`data::run`(6000) + `data::cache_flush_task` + `market::run`(5700)；
    · `mir2-rs/bins/db-srv/src/config.rs:136/146/170`（ServerPort=6000 / GatePort=5100 / MarketPort=5700 键解析）
      + `config.rs:314-326`（现网 dbsvr.conf 断言 5100/5600/6000/5700）。
  同理 `tools/m1-acceptance/start_boundary_stack.ps1:15-16` 的「不含 6000」也已过时（本任务只登记，不改那两处注释）。
  Rust login-srv 的 5500/5600 见 `mir2-rs/bins/login-srv/src/main.rs:1/113-127`；game-svr 连 6000/5600 见
  `mir2-rs/bins/game-svr/src/config.rs:88/740`。

  **不静默回落 C#**：任一步端口未就绪 ⇒ 打印 `RED` 并 exit 1；缺 exe/配置 ⇒ `PREREQ_MISSING` 并 exit 2。
  本脚本**只起 Rust 件**，任何一步失败都不会偷偷换成 C# 实现（这是 M5 灰度第一步的目的：暴露真实接线缺口，
  例如 Rust login-gate↔Rust login-srv 5500、Rust game-svr 取档↔Rust db-srv 6000）。

  种子/状态/rng/隔离/布局注入与 `start_rust_stack.ps1` **同口径**：dot-source `tools/m2-acceptance/rust_seed_plumb.ps1`
  共用同一批 helper（`Resolve-Mir2Seed` / `Start-SeededProcess` / `Get-*-InjectedLine` / 交叉 fail-closed 守卫），
  只对 Rust game-svr 注入，并打 `RUST_*_INJECTED` 判据行。**不传 ⇒ 不注入、不打该行**（与改动前逐字节一致）。

  判据/退出码（与 `start_rust_stack.ps1` 对称）：
    `-Stop`   按 pid 文件停掉本脚本起的进程（**MySQL 条目跳过不杀**：共享 DB 永不由本脚本停，见 SKIP-STOP）；缺 pid 文件 ⇒ `NOTHING_TO_STOP`。
    `-Status` 只按端口实况打印，不改状态。
    起栈：任一步端口未就绪 ⇒ 打印 `RED` 并 exit 1；缺 exe/配置 ⇒ `PREREQ_MISSING` 并 exit 2；
          启动前若目标端口已被监听 ⇒ exit 1（**不静默混实现**，先用 `-Stop` 或查清楚谁占着）。
    成功 ⇒ 打印 `FULL_RUST_STACK_OK <pidfile>` 并 exit 0。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m5-acceptance/start_full_rust_stack.ps1 `
    -RustBinDir E:\tmp\openmir2-m2svr-target\debug
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m5-acceptance/start_full_rust_stack.ps1 -Stop
.EXAMPLE
  # ②③ 真机同种子对拍：全 Rust 栈 + 显式种子（注入 Rust game-svr 的子进程环境）
  powershell -ExecutionPolicy Bypass -File tools/m5-acceptance/start_full_rust_stack.ps1 `
    -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42
#>
param(
  [string]$CsharpRepo = 'E:\Users\gxh\Documents\GitHub\OpenMir2',
  [string]$MirServer  = 'E:\MirServer',
  [string]$RustBinDir = 'E:\tmp\openmir2-m2svr-target\debug',
  [string]$StackRoot  = 'E:\tmp\m5-acceptance\stack',
  [string]$LogDir     = 'E:\tmp\m5-acceptance\stack\logs',
  # 真 game-svr `Server.conf`：默认用现网 `E:\MirServer\M2GameSvr\Server.conf`（`WorldMaps` 已降为可选键）。
  # game-svr 的 BasePath = Server.conf 所在目录（`bins/game-svr/src/main.rs:289-290`）⇒ **故意不复制到影子目录**
  # （它还要在同目录找地图/`string.conf` 等数据文件）；本脚本对 game-svr 只读该真配置，不写回。
  [string]$GameSvrConf = '',
  # 历史绕过：master 的 game-gate 已含 GB2312 配置修复 ⇒ 仅作显式覆盖。
  [string]$GameGateExe = '',
  [int]$WaitSeconds   = 60,
  # ②③ 真机同种子对拍的显式种子（空 ⇒ 回落读调用方 `MIR2_RNG_SEED`；两者皆空 ⇒ 不注入）。
  [string]$Seed       = '',
  # ②③ 真机目标可用性门的显式服务端 dump 路径（空 ⇒ 回落读 `MIR2_RNG_STATE_LOG`；两者皆空 ⇒ 不注入）。
  [string]$StateLog   = '',
  # ②③/Claim B 逐次 rng 流水的显式路径（空 ⇒ 回落读 `MIR2_RNG_LOG`；两者皆空 ⇒ 不注入）。
  [string]$RngLog     = '',
  # ②③ 活栈隔离口径的攻击路径专用种子流开关（显式 switch；或读 `MIR2_RNG_ISOLATE_ATTACK`）。
  [switch]$IsolateAttack,
  # 帧 52/53 记录布局：`0|false|FALSE|False` = oracle 逐字节（③ 对拍），`1|...` = 冻结客户端布局（生产默认）；
  # 空 ⇒ 回落读 `MIR2_CLIENT_ABILITY_LAYOUT`，两者皆空 ⇒ 不注入（生产默认）。
  [string]$ClientAbilityLayout = '',
  [switch]$Stop,
  [switch]$Status,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
# 种子/状态/rng/隔离/布局注入管线：与 `start_rust_stack.ps1` 共用同一份 helper。
if (Test-Path (Join-Path $Here 'rust_seed_plumb.ps1')) {
  . (Join-Path $Here 'rust_seed_plumb.ps1')
} else {
  . (Join-Path $CsharpRepo 'tools\m2-acceptance\rust_seed_plumb.ps1')
}
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
New-Item -ItemType Directory -Force -Path $StackRoot | Out-Null
$PidFile = Join-Path $LogDir 'full-rust-stack.pid.json'

function Test-Port([int]$Port) {
  # 用 .NET 查监听表：不开连接、不走 WMI。`Get-NetTCPConnection` 在本机偶发挂死（m3/M2 两条线都踩过）。
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
    if ($ep.Port -eq $Port) { return $true }
  }
  return $false
}

function Get-ListenPids {
  # netstat -ano 一次性取 端口->PID（只用于展示，不作判据）
  $map = @{}
  foreach ($line in (& netstat -ano 2>$null)) {
    if ($line -match '^\s*TCP\s+\S+:(\d+)\s+\S+\s+LISTENING\s+(\d+)\s*$') {
      $p = [int]$Matches[1]; $procId = [int]$Matches[2]
      if (-not $map.ContainsKey($p)) { $map[$p] = $procId }
    }
  }
  return $map
}

function Get-Owner([int]$Port) {
  $map = Get-ListenPids
  if (-not $map.ContainsKey($Port)) { return $null }
  return (Get-CimInstance Win32_Process -Filter ("ProcessId=$($map[$Port])") -ErrorAction SilentlyContinue)
}

$ShowPorts = @(3306, 6000, 5100, 5700, 5500, 5600, 5000, 7000, 7100, 7200)
function Show-Status {
  Write-Output '=== M5 全 Rust 栈 端口实况 ==='
  foreach ($p in $ShowPorts) {
    $o = Get-Owner $p
    if ($o) { Write-Output ("  {0,-5} <- pid={1} {2}  {3}" -f $p, $o.ProcessId, $o.Name, $o.ExecutablePath) }
    else    { Write-Output ("  {0,-5} DOWN" -f $p) }
  }
}

if ($Stop) {
  if (-not (Test-Path $PidFile)) { Write-Output 'NOTHING_TO_STOP (无 pid 文件)'; exit 0 }
  $saved = Get-Content $PidFile -Raw | ConvertFrom-Json
  $entries = @($saved)
  if ($entries.Count -eq 1 -and ($entries[0].PSObject.Properties.Name -contains 'procs')) { $entries = @($entries[0].procs) }
  # 逆启动序停：网关 → game-svr → login-srv → db-srv → MySQL
  foreach ($g in ($entries | Sort-Object { [int]$_.stop_order })) {
    if (-not $g -or $null -eq $g.pid) { continue }
    # MySQL 是共享基础设施（M5 计划「保留 3306」）：本脚本可以「起」它（3306 down 时自动拉起并登记），
    # 但**永不停**它 —— 否则 -Stop 会把和本栈无关的共享 DB 一起带走（2026-10-15 验证者 D2）。
    # 范式同 tools/m5-acceptance/stage_gray_rollback.ps1 的 SKIP-STOP MySQL 分支。
    if ($g.name -eq 'MySQL') { Write-Output ("SKIP-STOP MySQL pid={0}（共享 DB 永不由本脚本停）" -f $g.pid); continue }
    $p = Get-Process -Id $g.pid -ErrorAction SilentlyContinue
    if ($p) { Stop-Process -Id $g.pid -Force; Write-Output ("STOPPED {0} pid={1}" -f $g.name, $g.pid) }
    else { Write-Output ("ALREADY_GONE {0} pid={1}" -f $g.name, $g.pid) }
  }
  Start-Sleep -Seconds 2
  Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
  Show-Status
  exit 0
}

if ($Status) { Show-Status; exit 0 }

# ---- 0) 前置：端口必须空（不静默混实现）----
$wanted = @(6000, 5100, 5700, 5500, 5600, 5000, 7000, 7100, 7200)
if (-not $Force) {
  $busy = @($wanted | Where-Object { Test-Port $_ })
  if ($busy.Count -gt 0) {
    Write-Output ("RED: 目标端口已被监听 {0} —— 先停掉现有栈（本脚本 -Stop，或对应 Rust 起栈器 start_rust_stack.ps1 -LogDir <同一起栈目录> -Stop）再起；**不要**用 E:\MirServer\start-all.ps1 -Stop（其 targets 含 mysqld，会停共享 DB，2026-10-15 验证者 D1）" -f ($busy -join ','))
    Show-Status
    exit 1
  }
}

# ---- 1) exe 存在性（Rust 六件 + MySQL + 配置源）----
$mysqlExe  = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe'
$mysqlData = 'D:\mysql\data'
$dbsrvExe  = Join-Path $RustBinDir 'db-srv.exe'
$lsrvExe   = Join-Path $RustBinDir 'login-srv.exe'
$gsvrExe   = Join-Path $RustBinDir 'game-svr.exe'
$lgExe     = Join-Path $RustBinDir 'login-gate.exe'
$sgExe     = Join-Path $RustBinDir 'sel-gate.exe'
$ggExe     = if ([string]::IsNullOrEmpty($GameGateExe)) { Join-Path $RustBinDir 'game-gate.exe' } else { $GameGateExe }
$gsvrConf  = if ([string]::IsNullOrEmpty($GameSvrConf)) { Join-Path $MirServer 'M2GameSvr\Server.conf' } else { $GameSvrConf }
# 网关/LoginSrv 的 GBK 配置源（整体复制，不重建）：LoginGate / SelGate / RunGate 的现网 config.conf。
$lgConfSrc = Join-Path $MirServer 'LoginGate\config.conf'
$sgConfSrc = Join-Path $MirServer 'SelGate\config.conf'
$ggConfSrc = Join-Path $MirServer 'RunGate\config.conf'

$missing = @()
foreach ($p in @($mysqlExe, $dbsrvExe, $lsrvExe, $gsvrExe, $lgExe, $sgExe, $ggExe, $gsvrConf, $lgConfSrc, $sgConfSrc, $ggConfSrc)) {
  if (-not (Test-Path $p)) { $missing += $p }
}
if ($missing.Count -gt 0) {
  Write-Output 'PREREQ_MISSING: 缺以下 exe/配置：'
  $missing | ForEach-Object { Write-Output ("  " + $_) }
  exit 2
}

# ---- 2) 生成影子配置（不碰 `E:\MirServer` 的配置；照 m1 -StackRoot 做法）----
$dbsDir = Join-Path $StackRoot 'db-srv'
$lsvDir = Join-Path $StackRoot 'login-srv'
$lgsDir = Join-Path $StackRoot 'login-gate'
$sgsDir = Join-Path $StackRoot 'sel-gate'
$ggsDir = Join-Path $StackRoot 'game-gate'
foreach ($d in @($dbsDir, $lsvDir, $lgsDir, $sgsDir, $ggsDir)) { New-Item -ItemType Directory -Force -Path $d | Out-Null }

# 网关配置：GBK **整体复制**（端口不变：login-gate→5500、sel-gate→5100、game-gate→5000）。
Copy-Item $lgConfSrc (Join-Path $lgsDir 'config.conf') -Force
Copy-Item $sgConfSrc (Join-Path $sgsDir 'config.conf') -Force
Copy-Item $ggConfSrc (Join-Path $ggsDir 'config.conf') -Force

# login-srv（Rust）：logsrv.conf + AddrTable.txt（GBK 整体复制；缺则回落 E:\MirServer\LoginSrv）+ UserLimit.txt。
$srcLoginSrv = Join-Path $CsharpRepo 'src\LoginSrv\bin\Release'
if (-not (Test-Path (Join-Path $srcLoginSrv 'logsrv.conf'))) { $srcLoginSrv = Join-Path $MirServer 'LoginSrv' }
$lsrvMissing = @()
foreach ($p in @((Join-Path $srcLoginSrv 'logsrv.conf'), (Join-Path $srcLoginSrv 'AddrTable.txt'), (Join-Path $MirServer 'LoginSrv\UserLimit.txt'))) {
  if (-not (Test-Path $p)) { $lsrvMissing += $p }
}
if ($lsrvMissing.Count -gt 0) {
  Write-Output 'PREREQ_MISSING: login-srv 影子配置源缺失：'
  $lsrvMissing | ForEach-Object { Write-Output ("  " + $_) }
  exit 2
}
Copy-Item (Join-Path $srcLoginSrv 'logsrv.conf')    (Join-Path $lsvDir 'logsrv.conf')    -Force
Copy-Item (Join-Path $srcLoginSrv 'AddrTable.txt')  (Join-Path $lsvDir 'AddrTable.txt')  -Force
Copy-Item (Join-Path $MirServer 'LoginSrv\UserLimit.txt') (Join-Path $lsvDir 'UserLimit.txt') -Force
# 5600 允许接入的 IP 白名单（db-srv 从 127.0.0.1 连）
[System.IO.File]::WriteAllText((Join-Path $lsvDir 'ServerAddr.txt'), "127.0.0.1`r`n", (New-Object System.Text.ASCIIEncoding))

# db-srv（Rust）：dbsvr.conf（含 6000/5100/5700 + MySQL）+ AddrTable/ServerInfo/SelectID/DenyChrName。
$dbsConf = @"
[Setup]
ShowDebug=1
ServerAddr=0.0.0.0
ServerPort=6000
GateAddr=0.0.0.0
GatePort=5100
ServerName=LegendOfMir2
DenyChrName=1
DELMaxLevel=30
DynamicIPMode=0
MapFile=

[Server]
IDSAddr=127.0.0.1
IDSPort=5600
MarketAddr=127.0.0.1
MarketPort=5700
PushMarketInterval=30000

[DataBase]
Storeage=MySQL
ConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_db;

[DBClear]
Interval=3000
"@
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'dbsvr.conf'), $dbsConf, (New-Object System.Text.ASCIIEncoding))
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'AddrTable.txt'), "127.0.0.1`r`n", (New-Object System.Text.ASCIIEncoding))
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'ServerInfo.txt'), "127.0.0.1 127.0.0.1 7200`r`n", (New-Object System.Text.ASCIIEncoding))
[System.IO.File]::WriteAllText((Join-Path $dbsDir 'SelectID.txt'), "127.0.0.1 1`r`n", (New-Object System.Text.ASCIIEncoding))
$denySrc = Join-Path $MirServer 'DBServer\DenyChrName.txt'
if (Test-Path $denySrc) { Copy-Item $denySrc (Join-Path $dbsDir 'DenyChrName.txt') -Force }
else { [System.IO.File]::WriteAllText((Join-Path $dbsDir 'DenyChrName.txt'), "", (New-Object System.Text.ASCIIEncoding)) }

# ---- 2b) 种子/状态/rng/隔离/布局解析（fail-closed：非法 ⇒ 不起栈 exit 1；未传 ⇒ 零行为差）----
$seedInfo = $null
try { $seedInfo = Resolve-Mir2Seed -SeedArg $Seed -EnvValue $env:MIR2_RNG_SEED }
catch { Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法种子下起栈（fail-closed）"); exit 1 }
if ($seedInfo.Present) { Write-Output ("SEED: 将向 game-svr 注入 MIR2_RNG_SEED={0}" -f $seedInfo.Value) }
else { Write-Output 'SEED: 未指定种子（不注入，与改动前行为一致）' }

$stateLogInfo = $null
try { $stateLogInfo = Resolve-Mir2StateLog -StateLogArg $StateLog -EnvValue $env:MIR2_RNG_STATE_LOG }
catch { Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法状态 dump 路径下起栈（fail-closed）"); exit 1 }
if ($stateLogInfo.Present) { Write-Output ("STATE_LOG: 将向 game-svr 注入 MIR2_RNG_STATE_LOG={0}" -f $stateLogInfo.Path) }
else { Write-Output 'STATE_LOG: 未指定服务端逐击状态 dump（不注入，与改动前行为一致）' }

$rngLogInfo = $null
try { $rngLogInfo = Resolve-Mir2RngLog -RngLogArg $RngLog -EnvValue $env:MIR2_RNG_LOG }
catch { Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法 rng 流水路径下起栈（fail-closed）"); exit 1 }
if ($rngLogInfo.Present) {
  if (-not (Assert-Mir2RngLogSeed -RngLog $rngLogInfo -Seed $seedInfo)) {
    Write-Output 'RED: 已给 MIR2_RNG_LOG/-RngLog 但无 MIR2_RNG_SEED/-Seed —— rng_trace 无种子不落流水（exit 1，拒绝静默空转）'; exit 1
  }
  Write-Output ("RNG_LOG: 将向 game-svr 注入 MIR2_RNG_LOG={0}" -f $rngLogInfo.Path)
} else { Write-Output 'RNG_LOG: 未指定逐次 rng 流水（不注入，与改动前行为一致）' }

$isolateInfo = $null
try { $isolateInfo = Resolve-Mir2IsolateAttack -IsolateSwitch:$IsolateAttack -EnvValue $env:MIR2_RNG_ISOLATE_ATTACK }
catch { Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法隔离开关取值下起栈（fail-closed）"); exit 1 }
if ($isolateInfo.Present) {
  if (-not (Assert-Mir2IsolateAttackSeed -IsolateAttack $isolateInfo -Seed $seedInfo)) {
    Write-Output 'RED: 已给 -IsolateAttack/MIR2_RNG_ISOLATE_ATTACK 但无 MIR2_RNG_SEED/-Seed —— attack_isolation_seed_from_env 开而无种子会 panic（exit 1）'; exit 1
  }
  Write-Output 'ISOLATE_ATTACK: 将向 game-svr 注入 MIR2_RNG_ISOLATE_ATTACK=1（攻击路径专用流）'
} else { Write-Output 'ISOLATE_ATTACK: 未指定攻击路径专用流（不注入，与改动前行为一致）' }

$layoutInfo = $null
try { $layoutInfo = Resolve-Mir2ClientAbilityLayout -LayoutArg $ClientAbilityLayout -EnvValue $env:MIR2_CLIENT_ABILITY_LAYOUT }
catch { Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法布局取值下起栈（fail-closed）"); exit 1 }
if ($layoutInfo.Present) {
  $layoutName = if ($layoutInfo.Oracle) { 'oracle-40/header-only（逐字节对拍）' } else { 'client-58/TCharDesc 尾（冻结客户端）' }
  Write-Output ("CLIENT_ABILITY_LAYOUT: 将向 game-svr 注入 MIR2_CLIENT_ABILITY_LAYOUT={0}（{1}）" -f $layoutInfo.Text, $layoutName)
} else { Write-Output 'CLIENT_ABILITY_LAYOUT: 未指定（不注入，生产默认 = 冻结客户端布局）' }

$started = @()
function Start-One([string]$Name, [string]$Exe, [string[]]$ExeArgs, [string]$WorkDir, [int[]]$Ports, [int]$StopOrder, [psobject]$Seed = $null, [psobject]$StateLog = $null, [psobject]$RngLog = $null, [psobject]$IsolateAttack = $null, [psobject]$ClientAbilityLayout = $null) {
  $out = Join-Path $LogDir ($Name + '.out.log')
  $err = Join-Path $LogDir ($Name + '.err.log')
  Remove-Item $out, $err -Force -ErrorAction SilentlyContinue
  $p = Start-SeededProcess -FilePath $Exe -ArgumentList $ExeArgs -WorkingDirectory $WorkDir `
         -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle 'Hidden' -Seed $Seed -StateLog $StateLog -RngLog $RngLog -IsolateAttack $IsolateAttack -ClientAbilityLayout $ClientAbilityLayout
  $script:started += [pscustomobject]@{ name = $Name; pid = $p.Id; ports = $Ports; stop_order = $StopOrder; exe = $Exe }
  Write-Output ("START {0} pid={1} ports={2}" -f $Name, $p.Id, ($Ports -join ','))
  foreach ($line in @((Get-SeedInjectedLine -Seed $Seed), (Get-StateLogInjectedLine -StateLog $StateLog),
                      (Get-RngLogInjectedLine -RngLog $RngLog), (Get-IsolateAttackInjectedLine -IsolateAttack $IsolateAttack),
                      (Get-ClientAbilityLayoutInjectedLine -ClientAbilityLayout $ClientAbilityLayout))) {
    if (-not [string]::IsNullOrEmpty($line)) { Write-Output $line }
  }
}

function Wait-Ports([int[]]$Want) {
  $deadline = (Get-Date).AddSeconds($WaitSeconds)
  while ((Get-Date) -lt $deadline) {
    if (@($Want | Where-Object { Test-Port $_ }).Count -eq $Want.Count) { return $true }
    Start-Sleep -Milliseconds 500
  }
  return (@($Want | Where-Object { Test-Port $_ }).Count -eq $Want.Count)
}

# ---- 3) MySQL ----
if (Test-Port 3306) { Write-Output 'MYSQL already up (3306)' }
else {
  Start-One -Name 'MySQL' -Exe $mysqlExe -ExeArgs @("--datadir=$mysqlData", '--port=3306', '--bind-address=127.0.0.1', '--console') -WorkDir 'D:\mysql\mariadb-10.11.19-winx64' -Ports @(3306) -StopOrder 9
}

# ---- 4) Rust db-srv / login-srv（全 Rust；不回落 C#）----
Start-One -Name 'db-srv'    -Exe $dbsrvExe -ExeArgs @((Join-Path $dbsDir 'dbsvr.conf'))  -WorkDir $dbsDir -Ports @(6000, 5100, 5700) -StopOrder 7
Start-One -Name 'login-srv' -Exe $lsrvExe  -ExeArgs @((Join-Path $lsvDir 'logsrv.conf')) -WorkDir $lsvDir -Ports @(5500, 5600)       -StopOrder 6

# ---- 5) Rust game-svr（先于网关：网关注册需要 GameSvr 在）----
Start-One -Name 'game-svr'   -Exe $gsvrExe -ExeArgs @($gsvrConf) -WorkDir (Split-Path $gsvrConf -Parent) -Ports @(5000)           -StopOrder 5 -Seed $seedInfo -StateLog $stateLogInfo -RngLog $rngLogInfo -IsolateAttack $isolateInfo -ClientAbilityLayout $layoutInfo
# ---- 6) Rust 三网关 ----
Start-One -Name 'login-gate' -Exe $lgExe   -ExeArgs @((Join-Path $lgsDir 'config.conf')) -WorkDir $lgsDir -Ports @(7000)           -StopOrder 1
Start-One -Name 'sel-gate'   -Exe $sgExe   -ExeArgs @((Join-Path $sgsDir 'config.conf')) -WorkDir $sgsDir -Ports @(7100)           -StopOrder 2
Start-One -Name 'game-gate'  -Exe $ggExe   -ExeArgs @((Join-Path $ggsDir 'config.conf')) -WorkDir $ggsDir -Ports @(7200, 7201, 7202) -StopOrder 3

$started | ConvertTo-Json -Depth 5 | Set-Content -Path $PidFile -Encoding utf8
Write-Output ("PIDFILE " + $PidFile)

# ---- 7) 等端口就绪（逐个逐个检查，缺一即 RED）----
Start-Sleep -Seconds 3
$expect = @(
  @{ name = 'MySQL';      ports = @(3306) }
  @{ name = 'db-srv';     ports = @(6000, 5100, 5700) }
  @{ name = 'login-srv';  ports = @(5500, 5600) }
  @{ name = 'game-svr';   ports = @(5000) }
  @{ name = 'login-gate'; ports = @(7000) }
  @{ name = 'sel-gate';   ports = @(7100) }
  @{ name = 'game-gate';  ports = @(7200) }
)
$fail = @()
foreach ($e in $expect) {
  if (-not (Wait-Ports $e.ports)) {
    $miss = @($e.ports | Where-Object { -not (Test-Port $_) })
    $fail += ("{0} 缺 {1}" -f $e.name, ($miss -join ','))
    Write-Output ("RED: {0} 端口未就绪（{1}）看 {2}" -f $e.name, ($miss -join ','), (Join-Path $LogDir ($e.name + '.err.log')))
  } else {
    Write-Output ("CORE_UP {0} ports={1}" -f $e.name, ($e.ports -join ','))
  }
}

Show-Status
if ($fail.Count -gt 0) {
  Write-Output ("RED: 全 Rust 栈未就绪：" + ($fail -join '; '))
  exit 1
}
Write-Output ('FULL_RUST_STACK_OK ' + $PidFile)
exit 0
