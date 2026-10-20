#requires -Version 5
<#
.SYNOPSIS
  start_rust_stack.ps1 — 把 M2 验收需要的**整栈**起来：MySQL + DBSrv + LoginSrv + Rust GameSvr(5000) + Rust 三网关(7000/7100/7200)。

.DESCRIPTION
  这是 `run_acceptance.ps1` 的**起栈前置**（`run_acceptance.ps1` 自己不起栈）。

  进程构成（路径/顺序照 `E:\MirServer\start-all.ps1`，只把「入口四端口」换成 Rust 实现）：

    MySQL      3306            （D:\mysql\mariadb-10.11.19-winx64）
    DBSrv      6000/5100/5700  （**C#**：`<CsharpRepo>\src\DBSrv\bin\Release`）
    LoginSrv   5500/5600       （**C#**：`<CsharpRepo>\src\LoginSrv\bin\Release`）
    game-svr   5000            （**Rust**：`bins/game-svr`，配置 `E:\MirServer\M2GameSvr\Server.conf`）
    login-gate 7000            （**Rust**：`bins/login-gate`，配置 `E:\MirServer\LoginGate\config.conf`）
    sel-gate   7100            （**Rust**：`bins/sel-gate`，配置 `E:\MirServer\SelGate\config.conf`）
    game-gate  7200/7201/7202  （**Rust**：`bins/game-gate`，配置 `E:\MirServer\RunGate\config.conf`）

  **为什么 DBSrv/LoginSrv 仍是 C#**：Rust `bins/db-srv` 只实现了 5100 面（SelGate 取角色表），
  **没有** GameSvr 取档用的 **6000 DataService** 面（`bins/db-srv/src/main.rs` 开头已登记），
  而 Rust game-svr 进世界必须先经 6000 取 `CharacterData`。所以「全 Rust 链」在 6000 面补齐前
  无法支撑真客户端进图；本脚本用 C# DBSrv/LoginSrv 作为 oracle 下盘（与 M3 首验同形，仅把 game-svr
  与三网关换成 Rust）。判据侧：M2 验收①只对 5000/7000/7100/7200 四端口取所有权，不受此影响。

  种子管线（卡 `openmir2-m3-rust-stack-seed-plumb`，②③ 真机同种子攻击对拍的前置）：
    `-Seed <int>` 显式种子（或读调用方 `MIR2_RNG_SEED`）⇒ 起 `game-svr.exe` 时把该变量
    **注入子进程环境**，并在启动日志打一行可判据的 `RUST_SEED_INJECTED=<n>`；game-svr 自身会打
    `SEED_PROBE` 行，两者须能对上。
    **A2（卡 `openmir2-window-collector-preflight`）默认种子对称**：`-Seed` 与 `MIR2_RNG_SEED` 都未给时
    **回落默认 42**（与 C# 启动器 `start-csharp-hook.ps1` 的 `[int]$Seed = 42` 对称）。理由：Rust 生产默认
    随机源**本就是 seed 42**（`crates/world/src/world.rs::with_maps` 的 `RandomNumber::with_seed(42)`，未设
    `MIR2_RNG_SEED` 即沿用）⇒ 回落 42 **RNG 行为零变化**，只把「声明层」补齐（两侧都有种子自证行，消除
    「C# 恒 42 / Rust 未传不注入」的假不对称 ⇒ 假 RED/假 GREEN 源）。显式 `-Seed` / `$env:MIR2_RNG_SEED` 仍优先。
    非法种子（`-Seed`/环境变量不是 int）⇒ fail-closed：打印 `RED` 并 exit 1（不起栈，不静默降级）。

  状态 dump 管线（卡 `openmir2-m2-rust-state-log-plumb`，夹具目标可用性门两侧强制的 Rust 半）：
    `-StateLog <path>` 显式路径（或读调用方 `MIR2_RNG_STATE_LOG`）⇒ 起 `game-svr.exe` 时把该变量
    **注入子进程环境**，并打一行 `RUST_STATE_LOG_INJECTED=<path>`；game-svr 自身会打「服务端逐击
    状态 dump 已开启」并在该路径建文件。**不传 ⇒ 不注入、不打该行**（与改动前逐字节一致）。
    路径父目录不存在 ⇒ fail-closed：打印 `RED` 并 exit 1（不静默起一个永远不落 dump 的栈）。
    与 C# 半同名变量（`start-csharp-hook.ps1 -StateLog`）⇒ 两侧可同口径采集。

  逐次 rng 流水管线（卡 `openmir2-m2-rust-rng-log-plumb`，两侧 site/order 账本 diff 的 Rust 半）：
    `-RngLog <path>` 显式路径（或读调用方 `MIR2_RNG_LOG`）⇒ 起 `game-svr.exe` 时把该变量
    **注入子进程环境**，并打一行 `RUST_RNG_LOG_INJECTED=<path>`；game-svr（`crates/shared` 的
    `rng_trace`）在该路径落 `seq kind arg result site` 流水，头两行 `INJECT_OK <seed>` + `SEED_PROBE`。
    **不传 ⇒ 不注入、不打该行**（与改动前逐字节一致）。路径父目录不存在 ⇒ fail-closed。
    与 C# 半同名变量（`rng-hook` 的 `MIR2_RNG_LOG`）⇒ 两侧账本可同口径采集。
    **交叉 fail-closed**：`rng_trace` 在 `MIR2_RNG_SEED` 缺失时关掉流水（只打 stderr）⇒ 若已给 `-RngLog`
    但无种子，本脚本**拒绝起栈**（exit 1），不静默起一个永远不落 rng-stream 的栈。
  攻击路径隔离管线（卡 `openmir2-m2-rust-isolate-attack-plumb`，M3②③ 活栈隔离口径的 Rust 半）：
    `-IsolateAttack`（或读调用方 `MIR2_RNG_ISOLATE_ATTACK`）⇒ 起 `game-svr.exe` 时把该变量
    **注入子进程环境（值 `1`）**，并打一行可判据的 `RUST_ISOLATE_ATTACK_INJECTED=1`；game-svr
    （`crates/world`）据此把攻击路径取数分流到 `MIR2_RNG_SEED + 0x9E37` 的专用流。**不传 ⇒ 不注入、
    不打该行**（与改动前逐字节一致）。取值认不出（既非开也非关）⇒ fail-closed：打印 `RED` 并 exit 1。
    **交叉 fail-closed**：隔离开但无 `MIR2_RNG_SEED` ⇒ 拒绝起栈（`attack_isolation_seed_from_env`
    开而无种子会 panic）。与 C# 半同名变量（`start-csharp-hook.ps1 -IsolateAttack`）⇒ 两侧同口径。

  帧 52/53 记录布局管线（卡 `openmir2-client-e2e-enter-gaps` 片 ㊷，M2 验收③ 的前置）：
    `-ClientAbilityLayout <0|1|true|false>`（或读调用方 `MIR2_CLIENT_ABILITY_LAYOUT`）⇒ 起 `game-svr.exe`
    时把该变量**注入子进程环境（原文）**，并打一行可判据的 `RUST_CLIENT_ABILITY_LAYOUT_INJECTED=<0|1>`
    （`0` = oracle 逐字节 / `1` = 冻结客户端）；game-svr 自身启动时打
    `CLIENT_ABILITY_LAYOUT layout=oracle-40|client-58`，两者须能对上。**不传 ⇒ 不注入、不打该行**
    （与改动前逐字节一致，**生产默认 = 冻结客户端布局**，真客户端可玩）。
    **为什么需要**：M2 验收③ `s2c_diff.py` 比的是 `ident=52/53` 的 `body_len/body_sha256/raw_hex`，
    金标准是 oracle 布局（52 `body_len=40`、53 `body_len=0`）；生产默认发 58/12 ⇒ **③a/③b 必须
    以 `-ClientAbilityLayout 0` 起 Rust 栈**，否则 52/53 必红（与数值无关；§6.0-3 的客户端冻结偏离）。
    取值认不出（含 `on`/`off`/`yes`/`no`）⇒ fail-closed：打印 `RED` 并 exit 1（不静默当开或当关；
    Rust 读端只把 `0|false|FALSE|False` 当 oracle，起栈器口径必须与生产读取逐字一致）。详见 README §0b.4。

  判据/退出码：
    `-Stop`   按 pid 文件停掉本脚本起的进程；缺 pid 文件 ⇒ `NOTHING_TO_STOP`。
    `-Status` 只按端口实况打印，不改状态。
    起栈：任一步端口未就绪 ⇒ 打印 `RED` 并 exit 1；缺 exe/配置 ⇒ `PREREQ_MISSING` 并 exit 2；
          启动前若目标端口已被监听 ⇒ exit 1（**不静默混实现**，先用 `-Stop` 或查清楚谁占着）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
    -RustBinDir E:\tmp\openmir2-m2svr-target\debug
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Stop
.EXAMPLE
  # ②③ 真机同种子对拍：给 Rust 半显式注入与 C# 半相同的种子
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
    -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42
.EXAMPLE
  # ②③ 真机隔离两相：给 Rust 半开服务端权威逐击状态 dump（server_state_gate.py 的 Rust 半）
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
    -RustBinDir E:\tmp\openmir2-m2svr-target\debug -StateLog E:\tmp\m3-rust\rng-server-state.tsv
.EXAMPLE
  # ②③/Claim B 逐次 rng 账本：给 Rust 半开 rng 流水（需同时给种子，否则拒绝起栈）
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
    -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42 -RngLog E:\tmp\m2-acceptance\stack\rng-stream.tsv
  # ②③ 活栈隔离口径：给 Rust 半开攻击路径专用种子流（需同时给种子，否则拒绝起栈）
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
    -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42 -IsolateAttack
#>
param(
  [string]$CsharpRepo = 'E:\Users\gxh\Documents\GitHub\OpenMir2',
  [string]$MirServer  = 'E:\MirServer',
  [string]$RustBinDir = 'E:\tmp\openmir2-m2svr-target\debug',
  [string]$LogDir     = 'E:\tmp\m2-acceptance\stack',
  # 历史绕过的显式覆盖（默认空 ⇒ 用上面的标准路径）：
  #   -GameSvrConf：真 `Server.conf` 现已是合法输入（`WorldMaps` 已降为可选键 + fail-closed，见 README §5.2b）
  #                 ⇒ 本参数仅作显式覆盖（指向补了 WorldMaps 的副本等），不再是起真配置的必需品
  #   -GameGateExe：master 的 game-gate 已含 GB2312 配置修复（`677678c2`）⇒ 同样仅作显式覆盖
  [string]$GameSvrConf = '',
  [string]$GameGateExe = '',
  [int]$WaitSeconds   = 60,
  # ②③ 真机同种子对拍的显式种子（空 ⇒ 回落读调用方 `MIR2_RNG_SEED`；两者皆空 ⇒ 不注入）。
  [string]$Seed       = '',
  # ②③ 真机目标可用性门的显式服务端 dump 路径（空 ⇒ 回落读调用方 `MIR2_RNG_STATE_LOG`；两者皆空 ⇒ 不注入）。
  [string]$StateLog   = '',
  # ②③/Claim B 逐次 rng 流水的显式路径（空 ⇒ 回落读调用方 `MIR2_RNG_LOG`；两者皆空 ⇒ 不注入）。
  # 注：与 `-Seed` / `MIR2_RNG_SEED` 联动（rng_trace 无种子不落流水）⇒ 已给 RngLog 但无种子会 exit 1。
  [string]$RngLog     = '',
  # ②③ 活栈隔离口径的攻击路径专用种子流开关（显式 switch；或读调用方 `MIR2_RNG_ISOLATE_ATTACK`）。
  [switch]$IsolateAttack,
  # 帧 52/53 记录布局（`openmir2-client-e2e-enter-gaps` 片 ㊷）：`0|false|FALSE|False` = oracle 逐字节（③ 对拍），
  # `1|true|TRUE|True` = 冻结客户端布局（真客户端可玩，生产默认）；空 ⇒ 回落读调用方 `MIR2_CLIENT_ABILITY_LAYOUT`，
  # 两者皆空 ⇒ 不注入（生产默认）。取值认不出 ⇒ fail-closed。
  [string]$ClientAbilityLayout = '',
  # ---- C10#8：把服务端指向测试库（克隆 `mir2_test_*`）------------------------------------
  # 只有给了任一 `-Db*` 才启用；未给 ⇒ **零调用**、配置/命令行逐字节不变（selftest 钉住）。
  # `-DbName` 是前缀（`mir2_test` → `mir2_test_db/_account/_data`），也接受 `mir2_test_*` 模板
  # 或具体库名 `mir2_test_db`（自动剥 `_db/_account/_data` 后缀仍按前缀展开三库）。
  # 每个组件只覆盖给定字段；未给的 host/port/user/password 沿用该组件自己的配置文件。
  [string]$DbHost = '',
  [string]$DbPort = '',
  [string]$DbUser = '',
  [string]$DbPassword = '',
  [string]$DbName = '',
  # 只打印将要执行的进程计划（JSON）后退出；不查端口、不起进程。给 -Db* 时会先生成影子配置，
  # 便于离线核对「到底把哪个文件指到了哪个库」。自检用它钉默认命令行。
  [switch]$EmitPlan,
  [switch]$Stop,
  [switch]$Status,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
# 种子管线（`Resolve-Mir2Seed` / `Get-SeedInjectedLine` / `Start-SeededProcess`）与 `check_seed_plumb.ps1` 共用同一份。
. (Join-Path $Here 'rust_seed_plumb.ps1')
# C10#8：测试库目标（纯函数；未给 -Db* ⇒ Present=$false，下面所有分支都不进）。
. (Join-Path $Here 'db_target_plumb.ps1')
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$PidFile = Join-Path $LogDir 'rust-stack.pid.json'

function Test-Port([int]$Port) {
  # 用 .NET 查监听表：不开连接、不走 WMI。`Get-NetTCPConnection` 在本机偶发挂死
  # （m3/M2 两条线都踩过），轮询端口时不能用它。
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

function Show-Status {
  Write-Output '=== M2 验收栈 端口实况 ==='
  foreach ($p in @(3306, 6000, 5100, 5700, 5500, 5600, 5000, 7000, 7100, 7200)) {
    $o = Get-Owner $p
    if ($o) { Write-Output ("  {0,-5} <- pid={1} {2}  {3}" -f $p, $o.ProcessId, $o.Name, $o.ExecutablePath) }
    else    { Write-Output ("  {0,-5} DOWN" -f $p) }
  }
}

function Stop-Gracefully([int]$procId, [string]$name) {
  # game-svr 的优雅关服（卡 `openmir2-save-on-shutdown`）：连它的关服命名管道
  # `\.\pipe\openmir2-gamesvr-shutdown-<pid>` 写 `SHUTDOWN`，让进程先存档再退出。
  # 其它进程没有这条通道 ⇒ 直接回 $false（调用方走 Force）。
  # 8s 上限：game-svr 内部存档等待上限 5s + 收尾余量；超时仍活着 ⇒ Force。
  if ($name -ne 'game-svr') { return $false }
  try {
    $client = [System.IO.Pipes.NamedPipeClientStream]::new(
      '.', ("openmir2-gamesvr-shutdown-{0}" -f $procId), [System.IO.Pipes.PipeDirection]::Out)
    $client.Connect(1000)
    $bytes = [System.Text.Encoding]::ASCII.GetBytes('SHUTDOWN')
    $client.Write($bytes, 0, $bytes.Length)
    $client.Flush()
    $client.Dispose()
  } catch {
    return $false
  }
  $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
  if ($p) { $null = $p.WaitForExit(8000) }
  return -not [bool](Get-Process -Id $procId -ErrorAction SilentlyContinue)
}

if ($Stop) {
  if (-not (Test-Path $PidFile)) { Write-Output 'NOTHING_TO_STOP (无 pid 文件)'; exit 0 }
  $saved = Get-Content $PidFile -Raw | ConvertFrom-Json
  $entries = @($saved)
  if ($entries.Count -eq 1 -and ($entries[0].PSObject.Properties.Name -contains 'procs')) { $entries = @($entries[0].procs) }
  # 逆启动序停：网关 → game-svr → LoginSrv → DBSrv → MySQL
  foreach ($g in ($entries | Sort-Object { [int]$_.stop_order })) {
    if (-not $g -or $null -eq $g.pid) { continue }
    $p = Get-Process -Id $g.pid -ErrorAction SilentlyContinue
    if ($p) {
      if (Stop-Gracefully $g.pid $g.name) {
        Write-Output ("STOPPED {0} pid={1} graceful" -f $g.name, $g.pid)
      } else {
        Stop-Process -Id $g.pid -Force
        Write-Output ("STOPPED {0} pid={1} force" -f $g.name, $g.pid)
      }
    }
    else { Write-Output ("ALREADY_GONE {0} pid={1}" -f $g.name, $g.pid) }
  }
  Start-Sleep -Seconds 2
  Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
  Show-Status
  exit 0
}

if ($Status) { Show-Status; exit 0 }
# ---- 测试库目标（C10#8）：`-Stop`/`-Status` 不受 -Db* 校验影响；起栈路径 fail-closed。----
$dbInfo = $null
try {
  $dbInfo = Resolve-Mir2DbTarget -DbHost $DbHost -DbPort $DbPort -DbUser $DbUser -DbPassword $DbPassword -DbName $DbName
} catch {
  Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法测试库目标下起栈（fail-closed）")
  exit 1
}


# ---- 0) 前置：exe / 配置 / 端口必须就位 ----
$wanted = @(6000, 5100, 5700, 5500, 5600, 5000, 7000, 7100, 7200)
if (-not $Force -and -not $EmitPlan) {
  $busy = @($wanted | Where-Object { Test-Port $_ })
  if ($busy.Count -gt 0) {
    Write-Output ("RED: 目标端口已被监听 {0} —— 先停掉现有栈（E:\MirServer\start-all.ps1 -Stop 或本脚本 -Stop）再起" -f ($busy -join ','))
    Show-Status
    exit 1
  }
}

$mysqlExe  = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe'
$mysqlData = 'D:\mysql\data'
$dbsrvDir  = Join-Path $CsharpRepo 'src\DBSrv\bin\Release'
$dbsrvExe  = Join-Path $dbsrvDir 'DBSrv.exe'
$lsrvDir   = Join-Path $CsharpRepo 'src\LoginSrv\bin\Release'
$lsrvExe   = Join-Path $lsrvDir 'LoginSrv.exe'
$gsvrExe   = Join-Path $RustBinDir 'game-svr.exe'
$lgExe     = Join-Path $RustBinDir 'login-gate.exe'
$sgExe     = Join-Path $RustBinDir 'sel-gate.exe'
$ggExe     = if ([string]::IsNullOrEmpty($GameGateExe)) { Join-Path $RustBinDir 'game-gate.exe' } else { $GameGateExe }
$gsvrConf  = if ([string]::IsNullOrEmpty($GameSvrConf)) { Join-Path $MirServer 'M2GameSvr\Server.conf' } else { $GameSvrConf }
$lgConf    = Join-Path $MirServer 'LoginGate\config.conf'
$sgConf    = Join-Path $MirServer 'SelGate\config.conf'
$ggConf    = Join-Path $MirServer 'RunGate\config.conf'

if (-not $EmitPlan) {
  $missing = @()
  foreach ($p in @($mysqlExe, $dbsrvExe, $lsrvExe, $gsvrExe, $lgExe, $sgExe, $ggExe, $gsvrConf, $lgConf, $sgConf, $ggConf)) {
    if (-not (Test-Path $p)) { $missing += $p }
  }
  if ($missing.Count -gt 0) {
    Write-Output 'PREREQ_MISSING: 缺以下 exe/配置：'
    $missing | ForEach-Object { Write-Output ("  " + $_) }
    exit 2
  }
}

# ---- 0a) C10#8：测试库影子（不改 `E:\MirServer`、不改主检出；未给 -Db* ⇒ 全走原路径）----
$dbsrvAppDir = $dbsrvDir
$lsrvAppDir  = $lsrvDir
$gsvrConfRun = $gsvrConf
$gsvrWorkDir = Split-Path $gsvrConf -Parent
$dbShadowRoot = Join-Path $LogDir 'db-target'
if ($dbInfo.Present) {
  try {
    $dbsrvAppDir = Join-Path $dbShadowRoot 'DBSrv'
    $lsrvAppDir  = Join-Path $dbShadowRoot 'LoginSrv'
    $gsvrSrcDir  = Split-Path $gsvrConf -Parent
    $gsvrConfRun = Join-Path (Join-Path $dbShadowRoot 'M2GameSvr') 'Server.conf'
    $gsvrWorkDir = Split-Path $gsvrConfRun -Parent

    $dbsName = Expand-Mir2DbName $dbInfo 'db'
    $accName = Expand-Mir2DbName $dbInfo 'account'
    $datName = Expand-Mir2DbName $dbInfo 'data'

    New-Mir2ShadowAppDir -SourceDir $dbsrvDir -DestDir $dbsrvAppDir
    $dbsConfPath = Join-Path $dbsrvAppDir 'dbsvr.conf'
    $dbsConn = Update-Mir2ConnString -ConnString (Get-Mir2IniConnString -Path $dbsConfPath) `
      -ServerHost $dbInfo.Host -Port $dbInfo.Port -User $dbInfo.User -Password $dbInfo.Password -Database $dbsName
    Set-Mir2IniConnString -Path $dbsConfPath -ConnString $dbsConn

    New-Mir2ShadowAppDir -SourceDir $lsrvDir -DestDir $lsrvAppDir
    $lsrvConfPath = Join-Path $lsrvAppDir 'logsrv.conf'
    $lsrvConn = Update-Mir2ConnString -ConnString (Get-Mir2IniConnString -Path $lsrvConfPath) `
      -ServerHost $dbInfo.Host -Port $dbInfo.Port -User $dbInfo.User -Password $dbInfo.Password -Database $accName
    Set-Mir2IniConnString -Path $lsrvConfPath -ConnString $lsrvConn

    $gsvrConn = Update-Mir2ConnString -ConnString (Get-Mir2IniConnString -Path $gsvrConf) `
      -ServerHost $dbInfo.Host -Port $dbInfo.Port -User $dbInfo.User -Password $dbInfo.Password -Database $datName
    New-Mir2GameSvrShadowBase -SourceDir $gsvrSrcDir -DestDir (Split-Path $gsvrConfRun -Parent) -ConnString $gsvrConn

    $dbsrvExe = Join-Path $dbsrvAppDir 'DBSrv.exe'
    $lsrvExe  = Join-Path $lsrvAppDir 'LoginSrv.exe'
    $gsvrConf = $gsvrConfRun
    if (-not $EmitPlan) {
      Write-Output ("DB_TARGET DBSrv database={0}" -f $(if ($dbsName) { $dbsName } else { '<沿用配置>' }))
      Write-Output ("DB_TARGET LoginSrv database={0}" -f $(if ($accName) { $accName } else { '<沿用配置>' }))
      Write-Output ("DB_TARGET game-svr database={0}" -f $(if ($datName) { $datName } else { '<沿用配置>' }))
      Write-Output ("DB_TARGET_SHADOW " + $dbShadowRoot)
    }
  } catch {
    Write-Output ("RED: 测试库影子生成失败：" + $_.Exception.Message + "（fail-closed，未起栈）")
    exit 1
  }
}

if ($EmitPlan) {
  $plan = [ordered]@{
    mode     = 'start_rust_stack'
    pidfile  = $PidFile
    db_target = [ordered]@{
      present = $dbInfo.Present
      host    = $dbInfo.Host
      port    = $dbInfo.Port
      user    = $dbInfo.User
      db_db   = (Expand-Mir2DbName $dbInfo 'db')
      db_account = (Expand-Mir2DbName $dbInfo 'account')
      db_data = (Expand-Mir2DbName $dbInfo 'data')
    }
    procs = @(
      [ordered]@{ name='MySQL';     exe=$mysqlExe; args=@("--datadir=$mysqlData",'--port=3306','--bind-address=127.0.0.1','--console'); workdir='D:\mysql\mariadb-10.11.19-winx64'; ports=@(3306); stop_order=9 }
      [ordered]@{ name='DBSrv';     exe=$dbsrvExe; args=@(); workdir=$dbsrvAppDir; ports=@(6000,5100,5700); stop_order=7 }
      [ordered]@{ name='LoginSrv';  exe=$lsrvExe;  args=@(); workdir=$lsrvAppDir;  ports=@(5500,5600);       stop_order=6 }
      [ordered]@{ name='game-svr';  exe=$gsvrExe;  args=@($gsvrConf); workdir=$gsvrWorkDir; ports=@(5000);     stop_order=5 }
      [ordered]@{ name='login-gate'; exe=$lgExe;   args=@($lgConf); workdir=(Split-Path $lgConf -Parent); ports=@(7000); stop_order=1 }
      [ordered]@{ name='sel-gate';   exe=$sgExe;   args=@($sgConf); workdir=(Split-Path $sgConf -Parent); ports=@(7100); stop_order=2 }
      [ordered]@{ name='game-gate';  exe=$ggExe;   args=@($ggConf); workdir=(Split-Path $ggConf -Parent); ports=@(7200,7201,7202); stop_order=3 }
    )
  }
  $plan | ConvertTo-Json -Depth 8
  exit 0
}

# ---- 0b) 种子解析（fail-closed：非法值 ⇒ 不起栈，exit 1；未传 ⇒ `Present=$false`，零行为差）----
$seedInfo = $null
try {
  $seedInfo = Resolve-Mir2Seed -SeedArg $Seed -EnvValue $env:MIR2_RNG_SEED
} catch {
  Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法种子下起栈（fail-closed）")
  exit 1
}
# A2（卡 openmir2-window-collector-preflight）：两侧默认种子对称 = 42。既未给 `-Seed` 也未给
# `$env:MIR2_RNG_SEED` ⇒ 回落 42（显式/env 仍优先）。Rust 生产默认随机源本就是 `with_maps` 的
# `with_seed(42)` ⇒ **RNG 行为零变化**，只补「声明层」自证行，消除与 C# 启动器默认 42 的假不对称。
if (-not $seedInfo.Present) {
  $seedInfo = Resolve-Mir2Seed -SeedArg '42'
  Write-Output 'SEED: 未显式指定（-Seed / MIR2_RNG_SEED 均空）⇒ 回落默认 42（与 C# 启动器对称；World::with_maps 生产默认即 seed 42）'
}
if ($seedInfo.Present) {
  Write-Output ("SEED: 将向 game-svr 注入 MIR2_RNG_SEED={0}" -f $seedInfo.Value)
} else {
  Write-Output 'SEED: 未指定种子（不注入，与改动前行为一致）'
}

# ---- 0c) 状态 dump 路径解析（fail-closed：父目录不存在 ⇒ 不起栈，exit 1；未传 ⇒ `Present=$false`，零行为差）----
$stateLogInfo = $null
try {
  $stateLogInfo = Resolve-Mir2StateLog -StateLogArg $StateLog -EnvValue $env:MIR2_RNG_STATE_LOG
} catch {
  Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法状态 dump 路径下起栈（fail-closed）")
  exit 1
}
if ($stateLogInfo.Present) {
  Write-Output ("STATE_LOG: 将向 game-svr 注入 MIR2_RNG_STATE_LOG={0}" -f $stateLogInfo.Path)
} else {
  Write-Output 'STATE_LOG: 未指定服务端逐击状态 dump（不注入，与改动前行为一致）'
}

# ---- 0d) rng 流水路径解析（fail-closed：父目录不存在 ⇒ 不起栈，exit 1；未传 ⇒ `Present=$false`，零行为差）----
$rngLogInfo = $null
try {
  $rngLogInfo = Resolve-Mir2RngLog -RngLogArg $RngLog -EnvValue $env:MIR2_RNG_LOG
} catch {
  Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法 rng 流水路径下起栈（fail-closed）")
  exit 1
}
if ($rngLogInfo.Present) {
  # 交叉 fail-closed：`rng_trace::enable_from_env` 在 `MIR2_RNG_SEED` 缺失时关闭流水（仅 stderr），
  # 栈会看起来正常却永远不落 rng-stream ⇒ 这种“静默空转”必须在起栈前拦住（判定已提为可单测的 helper）。
  if (-not (Assert-Mir2RngLogSeed -RngLog $rngLogInfo -Seed $seedInfo)) {
    Write-Output 'RED: 已给 MIR2_RNG_LOG/-RngLog 但无 MIR2_RNG_SEED/-Seed —— rng_trace 无种子不落流水（exit 1，拒绝静默空转）'
    exit 1
  }
  Write-Output ("RNG_LOG: 将向 game-svr 注入 MIR2_RNG_LOG={0}" -f $rngLogInfo.Path)
} else {
  Write-Output 'RNG_LOG: 未指定逐次 rng 流水（不注入，与改动前行为一致）'
}

# ---- 0d) 攻击路径专用种子流开关（M3②③ 隔离口径；未传 ⇒ `Present=$false`，零行为差）----
$isolateInfo = $null
try {
  $isolateInfo = Resolve-Mir2IsolateAttack -IsolateSwitch:$IsolateAttack -EnvValue $env:MIR2_RNG_ISOLATE_ATTACK
} catch {
  Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法隔离开关取值下起栈（fail-closed）")
  exit 1
}
if ($isolateInfo.Present) {
  # 交叉 fail-closed：`attack_isolation_seed_from_env` 在开而无种子时 panic（专用流种子无法与对侧对齐）
  # ⇒ 必须在起栈前拦住（判定已提为可单测的 helper）。
  if (-not (Assert-Mir2IsolateAttackSeed -IsolateAttack $isolateInfo -Seed $seedInfo)) {
    Write-Output 'RED: 已给 -IsolateAttack/MIR2_RNG_ISOLATE_ATTACK 但无 MIR2_RNG_SEED/-Seed —— attack_isolation_seed_from_env 开而无种子会 panic（exit 1，拒绝带 panic 起栈）'
    exit 1
  }
  Write-Output 'ISOLATE_ATTACK: 将向 game-svr 注入 MIR2_RNG_ISOLATE_ATTACK=1（攻击路径专用流，attack_seed = MIR2_RNG_SEED + 0x9E37）'
} else {
  Write-Output 'ISOLATE_ATTACK: 未指定攻击路径专用流（不注入，与改动前行为一致）'
}

# ---- 0e) 帧 52/53 记录布局（M2 验收③ 前置；未传 ⇒ `Present=$false`，零行为差 = 生产默认冻结客户端）----
$layoutInfo = $null
try {
  $layoutInfo = Resolve-Mir2ClientAbilityLayout -LayoutArg $ClientAbilityLayout -EnvValue $env:MIR2_CLIENT_ABILITY_LAYOUT
} catch {
  Write-Output ("RED: " + $_.Exception.Message + " —— 拒绝在非法布局取值下起栈（fail-closed）")
  exit 1
}
if ($layoutInfo.Present) {
  $layoutName = if ($layoutInfo.Oracle) { 'oracle-40/header-only（逐字节对拍）' } else { 'client-58/TCharDesc 尾（冻结客户端）' }
  Write-Output ("CLIENT_ABILITY_LAYOUT: 将向 game-svr 注入 MIR2_CLIENT_ABILITY_LAYOUT={0}（{1}）" -f $layoutInfo.Text, $layoutName)
} else {
  Write-Output 'CLIENT_ABILITY_LAYOUT: 未指定（不注入，生产默认 = 冻结客户端布局）'
}

$started = @()
function Start-One([string]$Name, [string]$Exe, [string[]]$ExeArgs, [string]$WorkDir, [int[]]$Ports, [int]$StopOrder, [psobject]$Seed = $null, [psobject]$StateLog = $null, [psobject]$RngLog = $null, [psobject]$IsolateAttack = $null, [psobject]$ClientAbilityLayout = $null) {
  $out = Join-Path $LogDir ($Name + '.out.log')
  $err = Join-Path $LogDir ($Name + '.err.log')
  Remove-Item $out, $err -Force -ErrorAction SilentlyContinue
  # `Start-SeededProcess` = 原 `Start-Process @sp`；仅当 `$Seed.Present` / `$StateLog.Present` / `$RngLog.Present` /
  # `$IsolateAttack.Present` 时在子进程环境里注入。
  $p = Start-SeededProcess -FilePath $Exe -ArgumentList $ExeArgs -WorkingDirectory $WorkDir `
         -RedirectStandardOutput $out -RedirectStandardError $err -WindowStyle 'Hidden' -Seed $Seed -StateLog $StateLog -RngLog $RngLog -IsolateAttack $IsolateAttack -ClientAbilityLayout $ClientAbilityLayout
  $script:started += [pscustomobject]@{ name = $Name; pid = $p.Id; ports = $Ports; stop_order = $StopOrder; exe = $Exe }
  Write-Output ("START {0} pid={1} ports={2}" -f $Name, $p.Id, ($Ports -join ','))
  # 传种子 ⇒ 打一行可判据的 `RUST_SEED_INJECTED=<n>`（不传 ⇒ `Get-SeedInjectedLine` 返回 `$null`，不打）。
  $seedLine = Get-SeedInjectedLine -Seed $Seed
  if (-not [string]::IsNullOrEmpty($seedLine)) { Write-Output $seedLine }
  # 传状态 dump 路径 ⇒ 打一行 `RUST_STATE_LOG_INJECTED=<path>`（不传 ⇒ `$null`，不打）。
  $stateLine = Get-StateLogInjectedLine -StateLog $StateLog
  if (-not [string]::IsNullOrEmpty($stateLine)) { Write-Output $stateLine }
  # 传 rng 流水路径 ⇒ 打一行 `RUST_RNG_LOG_INJECTED=<path>`（不传 ⇒ `$null`，不打）。
  $rngLine = Get-RngLogInjectedLine -RngLog $RngLog
  if (-not [string]::IsNullOrEmpty($rngLine)) { Write-Output $rngLine }
  # 传隔离开关 ⇒ 打一行 `RUST_ISOLATE_ATTACK_INJECTED=1`（不传 ⇒ `$null`，不打）。
  $isoLine = Get-IsolateAttackInjectedLine -IsolateAttack $IsolateAttack
  if (-not [string]::IsNullOrEmpty($isoLine)) { Write-Output $isoLine }
  # 传布局 ⇒ 打一行 `RUST_CLIENT_ABILITY_LAYOUT_INJECTED=<0|1>`（不传 ⇒ `$null`，不打）。
  $layoutLine = Get-ClientAbilityLayoutInjectedLine -ClientAbilityLayout $ClientAbilityLayout
  if (-not [string]::IsNullOrEmpty($layoutLine)) { Write-Output $layoutLine }
}

# ---- 1) MySQL ----
if (Test-Port 3306) { Write-Output 'MYSQL already up (3306)' }
else {
  Start-One -Name 'MySQL' -Exe $mysqlExe -ExeArgs @("--datadir=$mysqlData", '--port=3306', '--bind-address=127.0.0.1', '--console') -WorkDir 'D:\mysql\mariadb-10.11.19-winx64' -Ports @(3306) -StopOrder 9
}

function Wait-Ports([string]$Name, [int[]]$Ports) {
  $deadline = (Get-Date).AddSeconds($WaitSeconds)
  while ((Get-Date) -lt $deadline) {
    if (@($Ports | Where-Object { Test-Port $_ }).Count -eq $Ports.Count) { return $true }
    Start-Sleep -Milliseconds 500
  }
  return (@($Ports | Where-Object { Test-Port $_ }).Count -eq $Ports.Count)
}

# ---- 2) C# DBSrv / LoginSrv ----
# workdir 与 -EmitPlan 一致：给 -Db* 时 exe/workdir 都在 StackRoot/LogDir 的影子目录（源 Release 不受写）。
Start-One -Name 'DBSrv'    -Exe $dbsrvExe -ExeArgs @() -WorkDir $dbsrvAppDir -Ports @(6000, 5100, 5700) -StopOrder 7
Start-One -Name 'LoginSrv' -Exe $lsrvExe  -ExeArgs @() -WorkDir $lsrvAppDir  -Ports @(5500, 5600)       -StopOrder 6

# ---- 3) Rust game-svr（先于网关：网关注册需要 GameSvr 在） ----
Start-One -Name 'game-svr'   -Exe $gsvrExe -ExeArgs @($gsvrConf) -WorkDir (Split-Path $gsvrConf -Parent) -Ports @(5000)           -StopOrder 5 -Seed $seedInfo -StateLog $stateLogInfo -RngLog $rngLogInfo -IsolateAttack $isolateInfo -ClientAbilityLayout $layoutInfo
# ---- 4) Rust 三网关 ----
Start-One -Name 'login-gate' -Exe $lgExe   -ExeArgs @($lgConf)   -WorkDir (Split-Path $lgConf  -Parent) -Ports @(7000)           -StopOrder 1
Start-One -Name 'sel-gate'   -Exe $sgExe   -ExeArgs @($sgConf)   -WorkDir (Split-Path $sgConf  -Parent) -Ports @(7100)           -StopOrder 2
Start-One -Name 'game-gate'  -Exe $ggExe   -ExeArgs @($ggConf)   -WorkDir (Split-Path $ggConf  -Parent) -Ports @(7200, 7201, 7202) -StopOrder 3

$started | ConvertTo-Json -Depth 5 | Set-Content -Path $PidFile -Encoding utf8
Write-Output ("PIDFILE " + $PidFile)

# ---- 5) 等端口就绪（逐个逐个检查，缺一即 RED） ----
Start-Sleep -Seconds 3
$expect = @(
  @{ name = 'MySQL'; ports = @(3306) }
  @{ name = 'DBSrv'; ports = @(6000, 5100, 5700) }
  @{ name = 'LoginSrv'; ports = @(5500, 5600) }
  @{ name = 'game-svr'; ports = @(5000) }
  @{ name = 'login-gate'; ports = @(7000) }
  @{ name = 'sel-gate'; ports = @(7100) }
  @{ name = 'game-gate'; ports = @(7200) }
)
$fail = @()
foreach ($e in $expect) {
  if (-not (Wait-Ports $e.name $e.ports)) {
    $miss = @($e.ports | Where-Object { -not (Test-Port $_) })
    $fail += ("{0} 缺 {1}" -f $e.name, ($miss -join ','))
    $p = $script:started | Where-Object { $_.name -eq $e.name } | Select-Object -First 1
    $err = if ($p) { Join-Path $LogDir ($e.name + '.err.log') } else { '' }
    Write-Output ("RED: {0} 端口未就绪（{1}）看 {2}" -f $e.name, ($miss -join ','), $err)
  } else {
    Write-Output ("CORE_UP {0} ports={1}" -f $e.name, ($e.ports -join ','))
  }
}

Show-Status
if ($fail.Count -gt 0) {
  Write-Output ("RED: 栈未就绪：" + ($fail -join '; '))
  exit 1
}
Write-Output ('RUST_STACK_OK ' + $PidFile)
exit 0
