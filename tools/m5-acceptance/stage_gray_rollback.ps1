#requires -Version 5
<#
.SYNOPSIS
  stage_gray_rollback.ps1 — M5④「Rust 服务逐个替换」的**灰度阶梯 + 回滚演练**编排脚本（含计时口径）。
  本文件在规格里也叫 `gray_step.ps1`（见 `docs/Rust重写设计文档.md` §6 M5④）；落地名取 `stage_gray_rollback.ps1`。

.DESCRIPTION
  按 `-ImplMatrix` 描述的服务实现矩阵，**一次只翻一个服务**（C# ⇄ Rust），每翻一步：停当前栈 → 起目标组合栈
  → 跑 `tools/e2e/run_e2e.ps1` → 记 GREEN/RED + 耗时；任一步 RED 即 fail-closed（停 + 整轮 RED + 非零 rc），
  并（默认）**只把该步翻回**（`-RollbackOnRed`），不继续后续步骤。

  服务分组（依赖/启动序；每组含一个或多个进程）：
    db    = DBSrv      端口 6000/5100/5700（Rust `db-srv.exe` / C# `DBSrv.exe`）
    login = LoginSrv   端口 5500/5600      （Rust `login-srv.exe` / C# `LoginSrv.exe`）
    game  = GameSvr    端口 5000           （Rust `game-svr.exe` / C# `GameSrv.exe`）
    gate  = 三网关     端口 7000/7100/7200 （Rust `login-gate/sel-gate/game-gate.exe` / C# `LoginGate/SelGate/GameGate.exe`）

  `-ImplMatrix "db=rust,login=rust,game=csharp,gate=csharp"` = **目标组合**；阶梯 = 从**全 C# 基线**出发，
  按依赖序把与目标不同的服务逐个翻过去（每步一个 rung）。`-Rollback` 反向：从矩阵出发，按**反序**逐个翻回
  全 C# 基线，末步（= 基线）必须 `run_e2e` 全绿。

  **为什么自带组合器（而不是直接调某个起栈器）**：现有两个起栈器各是**固定组合**
  （`tools/m2-acceptance/start_rust_stack.ps1` = C# DBSrv/LoginSrv + Rust game-svr + Rust 网关；
  `tools/m1-acceptance/start_boundary_stack.ps1` = 全 Rust 边界链，不含 game-svr/6000），
  都不支持「db=rust,login=rust,game=csharp,gate=csharp」这类**任意混搭**。所以本脚本自带一个按矩阵起停的组合器
  （每进程命令行**逐条取自**上面两个起栈器 + `E:\MirServer\start-all.ps1`）；两个固定组合起栈器仍是各自的快路径，
  M5 终点「全 Rust（含 game-svr）」用 `start_full_rust_stack.ps1`。

  复用关系：
    · 每步 E2E 直接调用 `tools/e2e/run_e2e.ps1`（GREEN/RED/SKIP + -SelfTestRed）；
    · Rust game-svr 的种子/状态/rng/隔离/布局注入 dot-source `tools/m2-acceptance/rust_seed_plumb.ps1`（与
      `start_rust_stack.ps1` 同口径），只对 Rust game-svr 注入并打 `RUST_*_INJECTED` 行；
    · 影子配置生成照 `tools/m1-acceptance/start_boundary_stack.ps1`（GBK 文件整体复制，不碰 `E:\MirServer`）。
  与 `start_full_rust_stack.ps1` 的明显重复：**影子配置生成块**（见本文件 `New-ShadowConfigs`）与它同源；
  待本工具转正时可抽成共用 `m5_stack_common.ps1`（见 rationale 的 reuse_map）。

  计时口径（`[System.Diagnostics.Stopwatch]`，单位 ms，全落盘）：
    stop_ms  = 停当前栈（含 2s settle）耗时
    start_ms = 起目标组合栈 + **等到全部目标端口就绪**的耗时
    e2e_ms   = `run_e2e.ps1` 子进程用时
    total_ms = stop_ms + start_ms + e2e_ms
  产物（`-OutDir` 下）：`gray_step.ndjson`（每步一行）、`gray_step.timing.json`（聚合）、`GRAY.md`（人读表）。

  判据/退出码：
    · 正常：每步 run_e2e rc==0（无 RED；SKIP 由 `-FailOnSkip` 折算）⇒ 阶梯 GREEN ⇒ exit 0；任一步 RED ⇒ 停 + 整轮 RED + exit 1。
    · 回滚：末步回到全 C# 基线且 run_e2e 全绿 ⇒ exit 0；否则 RED。
    · `-SelfTestRed`（改坏必红）：把 `run_e2e` 断成必红（透传 `-SelfTestRed -SkipFlow`，见 run_e2e.ps1:209-214）⇒
      首步必 RED ⇒ 阶梯 RED + 非零 rc；本模式断言「确实变红」⇒ 确认牙齿：打印 `SELFTEST_RED GREEN` 并 exit 0，否则 exit 1。
    · `-DryRun`：只枚举 rung 与逐条命令行、写 DRYRUN 计时壳，**不起任何进程/端口**（无机器时降级自证）。

  分级提醒：**脚本本身 = 工具/夹具（L 级，可零窗口写 + 自检）**；**实际演练（起真栈/跑真 E2E/回滚）= 起真进程 + 命中
  R「持久化落库」⇒ 须独占窗口**（`docs/Rust重写设计文档.md:357/359-360`、§17.1）。

.EXAMPLE
  # 灰度：db+login 先换 Rust（game/gate 仍 C#）——每步跑一次 E2E
  powershell -ExecutionPolicy Bypass -File tools/m5-acceptance/stage_gray_rollback.ps1 `
    -ImplMatrix "db=rust,login=rust,game=csharp,gate=csharp" -SkipFlow
.EXAMPLE
  # 回滚演练：从全 Rust 反序走回全 C#（五步口径见 rationale），末步断言基线全绿
  powershell -ExecutionPolicy Bypass -File tools/m5-acceptance/stage_gray_rollback.ps1 `
    -ImplMatrix "db=rust,login=rust,game=rust,gate=rust" -Rollback
.EXAMPLE
  # 离线（无机器）：只打印阶梯计划与逐条命令行，不起栈
  powershell -ExecutionPolicy Bypass -File tools/m5-acceptance/stage_gray_rollback.ps1 `
    -ImplMatrix "db=rust,login=rust,game=csharp,gate=csharp" -DryRun
.EXAMPLE
  # 改坏必红（需能起栈）：透传 -SelfTestRed ⇒ 首步必 RED
  powershell -ExecutionPolicy Bypass -File tools/m5-acceptance/stage_gray_rollback.ps1 `
    -ImplMatrix "db=rust,login=csharp,game=csharp,gate=csharp" -SelfTestRed -SkipFlow
#>
param(
  # 目标（或回滚起点）实现矩阵：`svc=impl` 逗号分隔；svc∈{db,login,game,gate}，impl∈{rust,csharp}。
  # 省略某 svc ⇒ 该 svc 取 C#（基线）。**故意用 string 收**：`-File` 下逗号可能被当分组符（同 load_gate/stack_e2e 的坑）。
  [string]$ImplMatrix = "db=csharp,login=csharp,game=csharp,gate=csharp",
  [switch]$SkipFlow,
  [switch]$SkipBots,
  [switch]$Rollback,
  [string]$OutDir = 'E:\tmp\m5-acceptance\gray',
  [string]$Seed   = '',
  [switch]$SelfTestRed,
  [switch]$FailOnSkip,
  [switch]$DryRun,
  # 任一步 RED 时，是否「只把该步翻回」再停（默认开；任务口径「单步 RED ⇒ 只回滚该步」）。
  [bool]$RollbackOnRed = $true,
  [string]$CsharpRepo = 'E:\Users\gxh\Documents\GitHub\OpenMir2',
  [string]$MirServer  = 'E:\MirServer',
  [string]$RustBinDir = 'E:\tmp\openmir2-m2svr-target\debug',
  [string]$StackRoot  = 'E:\tmp\m5-acceptance\stack',
  [string]$GameSvrConf = '',
  [string]$GameGateExe = '',
  [int]$WaitSeconds = 60
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path
$E2eScript = Join-Path $RepoRoot 'tools\e2e\run_e2e.ps1'

# 种子注入管线：与 start_rust_stack.ps1 同口径（只注入 Rust game-svr）。
if (Test-Path (Join-Path $Here 'rust_seed_plumb.ps1')) {
  . (Join-Path $Here 'rust_seed_plumb.ps1')
} elseif (Test-Path (Join-Path $CsharpRepo 'tools\m2-acceptance\rust_seed_plumb.ps1')) {
  . (Join-Path $CsharpRepo 'tools\m2-acceptance\rust_seed_plumb.ps1')
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
New-Item -ItemType Directory -Force -Path $StackRoot | Out-Null
$PidFile     = Join-Path $OutDir 'gray-stack.pid.json'
$NdjsonPath  = Join-Path $OutDir 'gray_step.ndjson'
$TimingPath  = Join-Path $OutDir 'gray_step.timing.json'
$GrayMdPath  = Join-Path $OutDir 'GRAY.md'
Remove-Item $NdjsonPath -Force -ErrorAction SilentlyContinue

# ---- 0) 服务模型（依赖/启动序；每组一个或多个进程）----
$SvcOrder = @('db', 'login', 'game', 'gate')
$SvcPorts = @{ db = @(6000, 5100, 5700); login = @(5500, 5600); game = @(5000); gate = @(7000, 7100, 7200) }
$AllPorts = @(6000, 5100, 5700, 5500, 5600, 5000, 7000, 7100, 7200)

function Test-Port([int]$Port) {
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
    if ($ep.Port -eq $Port) { return $true }
  }
  return $false
}
function Get-ListenPids {
  $map = @{}
  foreach ($line in (& netstat -ano 2>$null)) {
    if ($line -match '^\s*TCP\s+\S+:(\d+)\s+\S+\s+LISTENING\s+(\d+)\s*$') {
      $p = [int]$Matches[1]; $procId = [int]$Matches[2]
      if (-not $map.ContainsKey($p)) { $map[$p] = $procId }
    }
  }
  return $map
}
function Wait-Ports([int[]]$Want) {
  $deadline = (Get-Date).AddSeconds($WaitSeconds)
  while ((Get-Date) -lt $deadline) {
    if (@($Want | Where-Object { Test-Port $_ }).Count -eq $Want.Count) { return $true }
    Start-Sleep -Milliseconds 500
  }
  return (@($Want | Where-Object { Test-Port $_ }).Count -eq $Want.Count)
}

# ---- 1) 解析矩阵 ----
function Parse-Matrix([string]$Text) {
  $m = @{ db = 'csharp'; login = 'csharp'; game = 'csharp'; gate = 'csharp' }
  foreach ($tok in ($Text -split '[,\s]+' | Where-Object { $_ -ne '' })) {
    $kv = $tok -split '='
    if ($kv.Count -ne 2) { throw ("-ImplMatrix 令牌 '$tok' 不是 svc=impl 形式") }
    $svc = $kv[0].Trim().ToLower(); $impl = $kv[1].Trim().ToLower()
    if (-not $m.ContainsKey($svc)) { throw ("-ImplMatrix 未知服务 '$svc'（允许 db/login/game/gate）") }
    if ($impl -ne 'rust' -and $impl -ne 'csharp') { throw ("-ImplMatrix 服务 '$svc' 的实现 '$impl' 认不出（允许 rust/csharp）") }
    $m[$svc] = $impl
  }
  return $m
}
function Format-Combo($combo) {
  # 稳定序输出（便于逐字对拍/日志）
  return (($SvcOrder | ForEach-Object { "$_=$($combo[$_])" }) -join ',')
}
function Combos-Equal($a, $b) {
  foreach ($svc in $SvcOrder) { if ($a[$svc] -ne $b[$svc]) { return $false } }
  return $true
}
function Clone-Combo($combo) { $c = @{}; foreach ($svc in $SvcOrder) { $c[$svc] = $combo[$svc] }; return $c }

$Matrix = Parse-Matrix $ImplMatrix
$Baseline = Parse-Matrix "db=csharp,login=csharp,game=csharp,gate=csharp"

# 构造阶梯 rung 列表：每 rung = 翻一个服务（from→to）。
$rungs = @()
if ($Rollback) {
  # 回滚：起点 = Matrix，终点 = Baseline，按**反序**逐个翻回 C#。
  $cur = Clone-Combo $Matrix
  $revOrder = @($SvcOrder); [array]::Reverse($revOrder)
  foreach ($svc in $revOrder) {
    if ($cur[$svc] -ne $Baseline[$svc]) {
      $from = $cur[$svc]; $cur[$svc] = $Baseline[$svc]
      $rungs += [pscustomobject]@{ service = $svc; from = $from; to = $Baseline[$svc]; combo = (Format-Combo $cur) }
    }
  }
  $mode = 'rollback'
} else {
  # 灰度：起点 = Baseline，终点 = Matrix，按**依赖序**逐个翻到目标。
  $cur = Clone-Combo $Baseline
  foreach ($svc in $SvcOrder) {
    if ($cur[$svc] -ne $Matrix[$svc]) {
      $from = $cur[$svc]; $cur[$svc] = $Matrix[$svc]
      $rungs += [pscustomobject]@{ service = $svc; from = $from; to = $Matrix[$svc]; combo = (Format-Combo $cur) }
    }
  }
  $mode = 'forward'
}

# ---- 2) 前置：exe/配置就位（Rust 件 + C# 件 + 配置源）----
$mysqlExe  = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe'
$mysqlData = 'D:\mysql\data'
$gsvrConf  = if ([string]::IsNullOrEmpty($GameSvrConf)) { Join-Path $MirServer 'M2GameSvr\Server.conf' } else { $GameSvrConf }
$ggRustExe = if ([string]::IsNullOrEmpty($GameGateExe)) { Join-Path $RustBinDir 'game-gate.exe' } else { $GameGateExe }

# 需要哪些 exe：矩阵里出现哪种 impl 就要求哪套（Rust 件 + C# 件都可能在混搭里用到）。
$needExes = @($mysqlExe)
foreach ($svc in $SvcOrder) {
  $impls = @($Baseline[$svc], $Matrix[$svc]) | Select-Object -Unique
  foreach ($impl in $impls) {
    switch ("$svc/$impl") {
      'db/rust'     { $needExes += (Join-Path $RustBinDir 'db-srv.exe') }
      'db/csharp'   { $needExes += (Join-Path $CsharpRepo 'src\DBSrv\bin\Release\DBSrv.exe') }
      'login/rust'  { $needExes += (Join-Path $RustBinDir 'login-srv.exe') }
      'login/csharp'{ $needExes += (Join-Path $CsharpRepo 'src\LoginSrv\bin\Release\LoginSrv.exe') }
      'game/rust'   { $needExes += (Join-Path $RustBinDir 'game-svr.exe') }
      'game/csharp' { $needExes += (Join-Path $MirServer 'M2GameSvr\GameSrv.exe') }
      'gate/rust'   { $needExes += (Join-Path $RustBinDir 'login-gate.exe'); $needExes += (Join-Path $RustBinDir 'sel-gate.exe'); $needExes += $ggRustExe }
      'gate/csharp' { $needExes += (Join-Path $MirServer 'LoginGate\LoginGate.exe'); $needExes += (Join-Path $MirServer 'SelGate\SelGate.exe'); $needExes += (Join-Path $MirServer 'RunGate\GameGate.exe') }
    }
  }
}
$needExes += $gsvrConf
foreach ($c in @((Join-Path $MirServer 'LoginGate\config.conf'), (Join-Path $MirServer 'SelGate\config.conf'), (Join-Path $MirServer 'RunGate\config.conf'))) { $needExes += $c }

# ---- 3) 影子配置（照 m1 start_boundary_stack；不在 DryRun 里落盘）----
function New-ShadowConfigs {
  $dbsDir = Join-Path $StackRoot 'db-srv'
  $lsvDir = Join-Path $StackRoot 'login-srv'
  $lgsDir = Join-Path $StackRoot 'login-gate'
  $sgsDir = Join-Path $StackRoot 'sel-gate'
  $ggsDir = Join-Path $StackRoot 'game-gate'
  foreach ($d in @($dbsDir, $lsvDir, $lgsDir, $sgsDir, $ggsDir)) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
  Copy-Item (Join-Path $MirServer 'LoginGate\config.conf') (Join-Path $lgsDir 'config.conf') -Force
  Copy-Item (Join-Path $MirServer 'SelGate\config.conf')   (Join-Path $sgsDir 'config.conf') -Force
  Copy-Item (Join-Path $MirServer 'RunGate\config.conf')   (Join-Path $ggsDir 'config.conf') -Force
  $srcLoginSrv = Join-Path $CsharpRepo 'src\LoginSrv\bin\Release'
  if (-not (Test-Path (Join-Path $srcLoginSrv 'logsrv.conf'))) { $srcLoginSrv = Join-Path $MirServer 'LoginSrv' }
  Copy-Item (Join-Path $srcLoginSrv 'logsrv.conf')   (Join-Path $lsvDir 'logsrv.conf')   -Force
  Copy-Item (Join-Path $srcLoginSrv 'AddrTable.txt') (Join-Path $lsvDir 'AddrTable.txt') -Force
  Copy-Item (Join-Path $MirServer 'LoginSrv\UserLimit.txt') (Join-Path $lsvDir 'UserLimit.txt') -Force
  [System.IO.File]::WriteAllText((Join-Path $lsvDir 'ServerAddr.txt'), "127.0.0.1`r`n", (New-Object System.Text.ASCIIEncoding))
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
}

# ---- 4) 进程规格：每个 (svc, impl) 展开成一条或多条启动项（命令行**逐条取自**三个既有脚本）----
function Get-ProcSpecs([string]$Svc, [string]$Impl) {
  # 返回 [pscustomobject] 数组：name/exe/args/workdir/ports/stop_order/seed_inject
  $dbsDir = Join-Path $StackRoot 'db-srv'
  $lsvDir = Join-Path $StackRoot 'login-srv'
  $lgsDir = Join-Path $StackRoot 'login-gate'
  $sgsDir = Join-Path $StackRoot 'sel-gate'
  $ggsDir = Join-Path $StackRoot 'game-gate'
  $specs = @()
  switch ("$Svc/$Impl") {
    'db/rust' {
      $specs += [pscustomobject]@{ name = 'db-srv'; exe = (Join-Path $RustBinDir 'db-srv.exe'); args = @((Join-Path $dbsDir 'dbsvr.conf')); workdir = $dbsDir; ports = @(6000, 5100, 5700); stop_order = 7; seed_inject = $false }
    }
    'db/csharp' {
      $specs += [pscustomobject]@{ name = 'DBSrv'; exe = (Join-Path $CsharpRepo 'src\DBSrv\bin\Release\DBSrv.exe'); args = @(); workdir = (Join-Path $CsharpRepo 'src\DBSrv\bin\Release'); ports = @(6000, 5100, 5700); stop_order = 7; seed_inject = $false }
    }
    'login/rust' {
      $specs += [pscustomobject]@{ name = 'login-srv'; exe = (Join-Path $RustBinDir 'login-srv.exe'); args = @((Join-Path $lsvDir 'logsrv.conf')); workdir = $lsvDir; ports = @(5500, 5600); stop_order = 6; seed_inject = $false }
    }
    'login/csharp' {
      $specs += [pscustomobject]@{ name = 'LoginSrv'; exe = (Join-Path $CsharpRepo 'src\LoginSrv\bin\Release\LoginSrv.exe'); args = @(); workdir = (Join-Path $CsharpRepo 'src\LoginSrv\bin\Release'); ports = @(5500, 5600); stop_order = 6; seed_inject = $false }
    }
    'game/rust' {
      $specs += [pscustomobject]@{ name = 'game-svr'; exe = (Join-Path $RustBinDir 'game-svr.exe'); args = @($gsvrConf); workdir = (Split-Path $gsvrConf -Parent); ports = @(5000); stop_order = 5; seed_inject = $true }
    }
    'game/csharp' {
      $specs += [pscustomobject]@{ name = 'GameSrv'; exe = (Join-Path $MirServer 'M2GameSvr\GameSrv.exe'); args = @(); workdir = (Join-Path $MirServer 'M2GameSvr'); ports = @(5000); stop_order = 5; seed_inject = $false }
    }
    'gate/rust' {
      $specs += [pscustomobject]@{ name = 'login-gate'; exe = (Join-Path $RustBinDir 'login-gate.exe'); args = @((Join-Path $lgsDir 'config.conf')); workdir = $lgsDir; ports = @(7000); stop_order = 1; seed_inject = $false }
      $specs += [pscustomobject]@{ name = 'sel-gate';   exe = (Join-Path $RustBinDir 'sel-gate.exe');   args = @((Join-Path $sgsDir 'config.conf')); workdir = $sgsDir; ports = @(7100); stop_order = 2; seed_inject = $false }
      $specs += [pscustomobject]@{ name = 'game-gate';  exe = $ggRustExe;                             args = @((Join-Path $ggsDir 'config.conf')); workdir = $ggsDir; ports = @(7200, 7201, 7202); stop_order = 3; seed_inject = $false }
    }
    'gate/csharp' {
      $specs += [pscustomobject]@{ name = 'LoginGate'; exe = (Join-Path $MirServer 'LoginGate\LoginGate.exe'); args = @(); workdir = (Join-Path $MirServer 'LoginGate'); ports = @(7000); stop_order = 1; seed_inject = $false }
      $specs += [pscustomobject]@{ name = 'SelGate';   exe = (Join-Path $MirServer 'SelGate\SelGate.exe');     args = @(); workdir = (Join-Path $MirServer 'SelGate');   ports = @(7100); stop_order = 2; seed_inject = $false }
      $specs += [pscustomobject]@{ name = 'RunGate';   exe = (Join-Path $MirServer 'RunGate\GameGate.exe');   args = @(); workdir = (Join-Path $MirServer 'RunGate');   ports = @(7200); stop_order = 3; seed_inject = $false }
    }
    default { throw "未知服务/实现组合：$Svc/$Impl" }
  }
  return $specs
}

function Format-CmdLine($spec) {
  $argTxt = if ($spec.args.Count -gt 0) { ' ' + (($spec.args | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' ') } else { '' }
  return ($spec.exe + $argTxt)
}

# ---- 5) 组合器：起/停当前栈 ----
$script:started = @()
function Start-Combo([hashtable]$combo, [psobject]$seedInfo) {
  $script:started = @()
  $logDir = Join-Path $OutDir 'logs'
  New-Item -ItemType Directory -Force -Path $logDir | Out-Null
  # MySQL（先起；已在跑则复用）
  if (-not (Test-Port 3306)) {
    Write-Output 'START MySQL (3306)'
    $p = Start-Process -FilePath $mysqlExe -ArgumentList @("--datadir=$mysqlData", '--port=3306', '--bind-address=127.0.0.1', '--console') `
          -WorkingDirectory 'D:\mysql\mariadb-10.11.19-winx64' -WindowStyle Hidden -PassThru `
          -RedirectStandardOutput (Join-Path $logDir 'MySQL.out.log') -RedirectStandardError (Join-Path $logDir 'MySQL.err.log')
    $script:started += [pscustomobject]@{ name = 'MySQL'; pid = $p.Id; ports = @(3306); stop_order = 9; exe = $mysqlExe }
  } else { Write-Output 'MYSQL already up (3306)' }
  # 各服务（按依赖序 db→login→game→gate；网关最后，因为网关启动时就建下游连接）
  foreach ($svc in $SvcOrder) {
    foreach ($spec in (Get-ProcSpecs $svc $combo[$svc])) {
      Write-Output ("START {0} [{1}]  {2}" -f $spec.name, $spec.ports -join ',', (Format-CmdLine $spec))
      $out = Join-Path $logDir ($spec.name + '.out.log')
      $err = Join-Path $logDir ($spec.name + '.err.log')
      Remove-Item $out, $err -Force -ErrorAction SilentlyContinue
      if ($spec.seed_inject -and $null -ne $seedInfo -and (Test-Command -Name 'Start-SeededProcess')) {
        $p = Start-SeededProcess -FilePath $spec.exe -ArgumentList $spec.args -WorkingDirectory $spec.workdir `
               -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle 'Hidden' -Seed $seedInfo
        $line = Get-SeedInjectedLine -Seed $seedInfo
        if (-not [string]::IsNullOrEmpty($line)) { Write-Output $line }
      } else {
        $sp = @{ FilePath = $spec.exe; WorkingDirectory = $spec.workdir; WindowStyle = 'Hidden'; PassThru = $true;
                 RedirectStandardOutput = $out; RedirectStandardError = $err }
        if ($spec.args.Count -gt 0) { $sp.ArgumentList = $spec.args }
        $p = Start-Process @sp
      }
      $script:started += [pscustomobject]@{ name = $spec.name; pid = $p.Id; ports = $spec.ports; stop_order = $spec.stop_order; exe = $spec.exe }
    }
  }
  $script:started | ConvertTo-Json -Depth 5 | Set-Content -Path $PidFile -Encoding utf8
  Write-Output ("PIDFILE " + $PidFile)
}
function Test-Command([string]$Name) { return [bool](Get-Command $Name -ErrorAction SilentlyContinue) }

function Stop-Combo {
  if (-not (Test-Path $PidFile)) { Write-Output 'NOTHING_TO_STOP (无 pid 文件)'; return }
  $saved = Get-Content $PidFile -Raw | ConvertFrom-Json
  $entries = @($saved)
  if ($entries.Count -eq 1 -and ($entries[0].PSObject.Properties.Name -contains 'procs')) { $entries = @($entries[0].procs) }
  foreach ($g in ($entries | Sort-Object { [int]$_.stop_order })) {
    # MySQL 是共享基础设施（M5 计划「保留 3306」）：本脚本可以「起」它（3306 down 时自动拉起并登记），
    # 但**永不停**它 —— 否则灰度/回滚演练会把和它无关的 DB 一起带走（2026-10-15 验证者 D2）。
    if ($g.name -eq 'MySQL') { Write-Output ("SKIP-STOP MySQL pid={0}（共享 DB 永不由本脚本停）" -f $g.pid); continue }
    if (-not $g -or $null -eq $g.pid) { continue }
    $p = Get-Process -Id $g.pid -ErrorAction SilentlyContinue
    if ($p) { Stop-Process -Id $g.pid -Force; Write-Output ("STOPPED {0} pid={1}" -f $g.name, $g.pid) }
    else { Write-Output ("ALREADY_GONE {0} pid={1}" -f $g.name, $g.pid) }
  }
  Start-Sleep -Seconds 2
  Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
}

function Invoke-E2E([string[]]$Extra) {
  # 子进程 stderr 在 $ErrorActionPreference="Stop" 下会变终止错误（stack_e2e 同源坑）⇒ 临时放宽为 Continue。
  $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
  try {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $E2eScript @Extra 2>&1
    return [pscustomobject]@{ rc = $LASTEXITCODE; out = @($out) }
  } finally { $ErrorActionPreference = $prev }
}

# ---- 6) 计时步进 ----
$steps = New-Object System.Collections.Generic.List[object]
$sentinelPath = $NdjsonPath

function Write-Step($rec) {
  $script:steps.Add($rec)
  ($rec | ConvertTo-Json -Compress -Depth 6) | Add-Content -Path $NdjsonPath -Encoding utf8
  Write-Output ("  STEP[{0}] svc={1} {2}->{3} combo={4} state={5} stop={6}ms start={7}ms e2e={8}ms total={9}ms rc={10}" -f `
    $rec.idx, $rec.service, $rec.from, $rec.to, $rec.combo, $rec.state, $rec.stop_ms, $rec.start_ms, $rec.e2e_ms, $rec.total_ms, $rec.e2e_rc)
}

function Emit-GrayReport([string]$MdMode, [string]$StartCombo, [string]$EndCombo, [string]$Result, [int]$Rc) {
  $md = New-Object System.Collections.Generic.List[string]
  $md.Add("# M5④ 灰度阶梯 / 回滚演练报告（$MdMode）")
  $md.Add('')
  $md.Add(("- 矩阵（起点→目标）：{0} → {1}" -f $StartCombo, $EndCombo))
  $md.Add(("- 结果：**{0}**（rc={1}）；rungs={2}；SelfTestRed={3}" -f $Result, $Rc, $rungs.Count, [bool]$SelfTestRed))
  $md.Add('')
  $md.Add('| # | kind | svc | from→to | combo | state | stop_ms | start_ms | e2e_ms | total_ms | e2e_rc |')
  $md.Add('|---|------|-----|---------|-------|-------|---------|----------|--------|----------|--------|')
  foreach ($st in $steps) {
    $kind = if ($null -ne $st.PSObject.Properties['kind']) { $st.kind } else { 'rung' }
    $md.Add(("| {0} | {1} | {2} | {3}→{4} | {5} | {6} | {7} | {8} | {9} | {10} | {11} |" -f `
      $st.idx, $kind, $st.service, $st.from, $st.to, $st.combo, $st.state, $st.stop_ms, $st.start_ms, $st.e2e_ms, $st.total_ms, $st.e2e_rc))
  }
  $md.Add('')
  $md.Add(("计时口径：stop_ms=停栈(含 2s settle)；start_ms=起目标组合栈+等全端口就绪；e2e_ms=run_e2e.ps1 用时；total_ms=三者之和。产物：{0}" -f $TimingPath))
  ($md -join "`r`n") | Set-Content -Path $GrayMdPath -Encoding utf8
}

# ---- 7) DryRun：只打印计划（不起进程/端口）----
if ($DryRun) {
  Write-Output ("DRYRUN mode={0} matrix={1} rungs={2}" -f $mode, (Format-Combo $Matrix), $rungs.Count)
  $idx = 0
  foreach ($r in $rungs) {
    $idx++
    $combo = Parse-Matrix $r.combo
    $cmds = @()
    foreach ($svc in $SvcOrder) { foreach ($spec in (Get-ProcSpecs $svc $combo[$svc])) { $cmds += ("    " + (Format-CmdLine $spec)) } }
    Write-Output ("  RUNG#{0} svc={1} {2}->{3} combo={4}" -f $idx, $r.service, $r.from, $r.to, $r.combo)
    $cmds | ForEach-Object { Write-Output $_ }
    Write-Step ([pscustomobject]@{ idx = $idx; service = $r.service; from = $r.from; to = $r.to; combo = $r.combo;
      state = 'DRYRUN'; stop_ms = 0; start_ms = 0; e2e_ms = 0; total_ms = 0; e2e_rc = $null; ports_ok = $null })
  }
  $dryTarget = if ($Rollback) { (Format-Combo $Baseline) } else { (Format-Combo $Matrix) }
  $dryStart  = if ($Rollback) { (Format-Combo $Matrix) } else { (Format-Combo $Baseline) }
  $agg = [pscustomobject]@{ mode = $mode; matrix = (Format-Combo $Matrix); baseline = (Format-Combo $Baseline);
    target = $dryTarget; self_test_red = [bool]$SelfTestRed;
    dry_run = $true; rungs = $rungs.Count; steps = $steps.ToArray(); result = 'DRYRUN'; rc = 0 }
  $agg | ConvertTo-Json -Depth 6 | Set-Content -Path $TimingPath -Encoding utf8
  Emit-GrayReport $mode $dryStart $dryTarget 'DRYRUN' 0
  Write-Output ("DRYRUN done -> " + $TimingPath + " ; " + $GrayMdPath)
  exit 0
}

if ($rungs.Count -eq 0) {
  Write-Output ("NO_RUNGS: 矩阵 {0} 与基线一致（{1}），无步可走" -f (Format-Combo $Matrix), (Format-Combo $Baseline))
  exit 0
}

# ---- 8) 前置：端口必须空（不静默混实现）----
$busy = @($AllPorts | Where-Object { Test-Port $_ })
if ($busy.Count -gt 0) {
  Write-Output ("RED: 目标端口已被监听 {0} —— 先停掉现有栈再起（用**持有该端口的栈自己的起栈器 -Stop**：tools/m2-acceptance/start_rust_stack.ps1 -LogDir <同一起栈的 -LogDir> -Stop，或本脚本 -Stop）；**不要**用 E:\MirServer\start-all.ps1 -Stop —— 它的 targets 含 mysqld，会停掉共享 DB（2026-10-15 验证者 D1）" -f ($busy -join ','))
  exit 1
}
$missing = @($needExes | Where-Object { -not (Test-Path $_) } | Select-Object -Unique)
if ($missing.Count -gt 0) {
  Write-Output 'PREREQ_MISSING: 缺以下 exe/配置：'
  $missing | ForEach-Object { Write-Output ("  " + $_) }
  exit 2
}
New-ShadowConfigs

# 种子解析（fail-closed）：仅注入 Rust game-svr。
$seedInfo = $null
try { $seedInfo = Resolve-Mir2Seed -SeedArg $Seed -EnvValue $env:MIR2_RNG_SEED }
catch { Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法种子下起栈（fail-closed）"); exit 1 }

# ---- 9) 阶梯主循环 ----
$e2eExtra = @()
if ($SkipFlow) { $e2eExtra += '-SkipFlow' }
if ($SkipBots) { $e2eExtra += '-SkipBots' }
if ($FailOnSkip) { $e2eExtra += '-FailOnSkip' }
if ($SelfTestRed) { $e2eExtra += '-SelfTestRed' }
# 2026-10-15 验证者 I1：原「-SelfTestRed ⇒ 自动补 -SkipFlow」会制造恒真牙（见文件末红检块），已删除。

$overallRed = $false
$idx = 0
foreach ($r in $rungs) {
  $idx++
  $combo = Parse-Matrix $r.combo
  Write-Output ("=== RUNG #{0}/{1}  svc={2} {3}->{4}  combo={5} ===" -f $idx, $rungs.Count, $r.service, $r.from, $r.to, $r.combo)

  # stop
  $s = [System.Diagnostics.Stopwatch]::StartNew(); Stop-Combo; $s.Stop(); $stopMs = [int]$s.ElapsedMilliseconds
  # start
  $s = [System.Diagnostics.Stopwatch]::StartNew(); Start-Combo $combo $seedInfo; Start-Sleep -Seconds 3
  $portsOk = $true
  foreach ($svc in $SvcOrder) {
    if (-not (Wait-Ports $SvcPorts[$svc])) {
      $miss = @($SvcPorts[$svc] | Where-Object { -not (Test-Port $_) })
      Write-Output ("RED: rung#{0} {1} 端口未就绪（{2}）" -f $idx, $svc, ($miss -join ','))
      $portsOk = $false
    }
  }
  $s.Stop(); $startMs = [int]$s.ElapsedMilliseconds

  $e2eRc = $null
  if (-not $portsOk) {
    $e2eMs = 0; $state = 'RED'
  } else {
    $s = [System.Diagnostics.Stopwatch]::StartNew()
    $r2 = Invoke-E2E $e2eExtra
    $s.Stop(); $e2eMs = [int]$s.ElapsedMilliseconds; $e2eRc = $r2.rc
    $r2.out | ForEach-Object { Write-Output ("    | " + $_) }
    $state = if ($e2eRc -eq 0) { 'GREEN' } else { 'RED' }
  }
  $totalMs = $stopMs + $startMs + $e2eMs
  Write-Step ([pscustomobject]@{ idx = $idx; service = $r.service; from = $r.from; to = $r.to; combo = $r.combo;
    state = $state; stop_ms = $stopMs; start_ms = $startMs; e2e_ms = $e2eMs; total_ms = $totalMs; e2e_rc = $e2eRc; ports_ok = $portsOk })

  if ($state -eq 'RED') {
    $overallRed = $true
    Write-Output ("RED: rung#{0} 失败（svc={1}）—— 停止阶梯（fail-closed）" -f $idx, $r.service)
    if ($RollbackOnRed) {
      # 只把该步翻回：从 r.combo 恢复到 r.from（即上一组合）。
      $back = Parse-Matrix $r.combo; $back[$r.service] = $r.from
      Write-Output ("ROLLBACK_OF_RUNG svc={0} {1}->{2} combo={3}" -f $r.service, $r.to, $r.from, (Format-Combo $back))
      $s = [System.Diagnostics.Stopwatch]::StartNew(); Stop-Combo; $rbStop = [int]$s.ElapsedMilliseconds
      $s = [System.Diagnostics.Stopwatch]::StartNew(); Start-Combo $back $seedInfo; Start-Sleep -Seconds 3
      $rbOk = $true
      foreach ($svc in $SvcOrder) { if (-not (Wait-Ports $SvcPorts[$svc])) { $rbOk = $false } }
      $s.Stop(); $rbStart = [int]$s.ElapsedMilliseconds
      Write-Step ([pscustomobject]@{ idx = $idx; kind = 'rollback_of_rung'; service = $r.service; from = $r.to; to = $r.from;
        combo = (Format-Combo $back); state = $(if ($rbOk) { 'GREEN' } else { 'RED' });
        stop_ms = $rbStop; start_ms = $rbStart; e2e_ms = 0; total_ms = $rbStop + $rbStart; e2e_rc = $null; ports_ok = $rbOk })
    }
    break
  }
}

# ---- 10) 汇总 + 产物 ----
$result = if ($overallRed) { 'RED' } else { 'GREEN' }
$startComboStr = if ($Rollback) { (Format-Combo $Matrix) } else { (Format-Combo $Baseline) }
$endComboStr   = if ($Rollback) { (Format-Combo $Baseline) } else { (Format-Combo $Matrix) }
$rcInt         = if ($overallRed) { 1 } else { 0 }
$agg = [pscustomobject]@{ mode = $mode; matrix = (Format-Combo $Matrix); baseline = (Format-Combo $Baseline);
  target = $endComboStr; start_combo = $startComboStr; self_test_red = [bool]$SelfTestRed;
  dry_run = $false; rungs = $rungs.Count; steps = $steps.ToArray(); result = $result; rc = $rcInt }
$agg | ConvertTo-Json -Depth 6 | Set-Content -Path $TimingPath -Encoding utf8

# GRAY.md（人读表）
Emit-GrayReport $mode $startComboStr $endComboStr $result $rcInt

Write-Output ("==== gray_step: {0}  ({1}, rungs={2}) ====" -f $result, $mode, $rungs.Count)
Write-Output ("ARTIFACTS: " + $TimingPath + " ; " + $GrayMdPath + " ; " + $NdjsonPath)

if ($SelfTestRed) {
  # 红检（2026-10-15 验证者 I1 修）：**不接受「任何非零 rc」** —— 在 -SkipFlow 下 run_e2e 找不到 flow-e2e 项
  # 本身就 rc!=0，那是恒真牙。真红检必须是「注入的坏步被 E2E 判红」，故要求三条同时成立：
  #   ① 未用 -SkipFlow（有真实 flow 相位）；② 汇总 RED；③ 至少一个 rung 的 e2e_rc != 0 且该步 ports_ok = true
  #      （把「端口未就绪」这类环境性失败排除在外）。任一条不满足 ⇒ fail-closed（不自称改坏必红）。
  if ($SkipFlow) {
    Write-Output 'SELFTEST_RED RED: -SelfTestRed 与 -SkipFlow 互斥（无 flow 相位的非零 rc 不是改坏必红证据，见验证者 I1）；真机红检归 R 窗口'
    exit 2
  }
  $e2eRedSeen = @($steps | Where-Object { $null -ne $_.e2e_rc -and $_.e2e_rc -ne 0 -and $_.ports_ok -eq $true }).Count -gt 0
  if ($overallRed -and $e2eRedSeen) {
    Write-Output 'SELFTEST_RED GREEN: 注入步的 E2E 判红与汇总一致（改坏必红成立）'
    exit 0
  }
  Write-Output 'SELFTEST_RED RED: 未观测到「注入步 E2E 判红」（仅 rc!=0 / 端口未就绪不算）——真机红检归 R 窗口'
  exit 1
}
if ($overallRed) { exit 1 }
exit 0
