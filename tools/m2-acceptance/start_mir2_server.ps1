#requires -Version 5
<#
.SYNOPSIS
  start_mir2_server.ps1 — 用单二进制 mir2-server 统一启动六个角色（运维面，替代逐进程手起）。

.DESCRIPTION
  `mir2-server` 是「单二进制、多角色」入口：`mir2-server <role> [role argv...]`。
  本脚本只负责**编排**：按固定顺序把同一个 exe 以六个角色各拉一个进程，
  日志落到 `-LogDir`，pid/端口/停止序号写 `mir2-server.pid.json`，任一角色未就绪即
  fail-closed（RED + 反向回滚已起角色），不静默。

  固定启动顺序（MySQL 为外部依赖，本脚本不启动也不停止）：
      1 db-srv      2 login-srv   3 game-svr
      4 login-gate  5 sel-gate    6 game-gate
  停止顺序 = 上面的严格反向（game-gate -> sel-gate -> login-gate -> game-svr -> login-srv -> db-srv）。

  配置文件（相对 `-Root`；每角色按候选顺序取**第一份存在**的 conf。候选序按本脚本服务的
  默认根优先——本脚本是 Rust mir2-server 启动器，现役窗口测试链（card_window →
  start-rust.ps1）默认 `-Root rust-root`，故 login-srv 以 rust/测试系名 `logsrv.conf` 优先
  （卡 openmir2-g0-conf-map-option-a 返工，R22-C-F1）；`E:/MirServer` 生产根无 `logsrv.conf`
  ⇒ 生产解析（`LoginSrv\config.conf`）不变。多候选并存时打 WARN「ambiguous root」：
      db-srv     <Root>\DBServer\dbsvr.conf   （或 <Root>\DBSrv\dbsvr.conf）
      login-srv  <Root>\LoginSrv\logsrv.conf  （或 <Root>\LoginSrv\config.conf）
      game-svr   <Root>\M2GameSvr\Server.conf
      login-gate <Root>\LoginGate\config.conf
      sel-gate   <Root>\SelGate\config.conf
      game-gate  <Root>\RunGate\config.conf

  日志：
      <LogDir>\<role>.out.log / <LogDir>\<role>.err.log
      <LogDir>\mir2-server.pid.json   （roles[]: role/pid/ports/stop_order/exe/args）

  环境注入（口径对齐 start_rust_stack.ps1）：
      -Seed   / 调用方 $env:MIR2_RNG_SEED  ->  子进程 `MIR2_RNG_SEED`
              显式 -Seed > 环境变量 > 默认 42（A2 对称默认；Rust 生产默认随机源本就是 seed 42）。
      -DbName / 调用方 $env:MIR2_DB_NAME    ->  子进程 `MIR2_DB_NAME`
              只做**环境透传**，**当前全仓无消费者**（grep MIR2_DB_NAME 只命中本脚本与本 runbook）：
              本脚本不改写任何配置文件，库名仍由各 conf 的 ConnctionString 决定；
              `-Seed` 则与 start_rust_stack.ps1 同口径（game-svr 真消费）。

  -WhatIf：只打印将执行的 COMMAND 行（含每个角色忙检/就绪等待的端口）与顺序，
           不启动任何进程，也**不创建 LogDir / pid 文件**；缺任一配置 => RED + 退出 2。
           实跑同样在全部前置检查通过后才创建 LogDir。
           **开窗前核对（卡 openmir2-start-server-whatif-sha）**：输出固定含三行
             SERVER_EXE <绝对路径>
             SERVER_EXE_SHA256 <小写 hex；文件不存在时 MISSING>
             SERVER_EXE_BUILD  Release | Debug-Fallback | Explicit
           release 二进制不存在时**不再静默**：打印 WARNING 并选 debug（Debug-Fallback），
           加 -RequireRelease 则直接 RED + 退出 2。根解析只用 $PSScriptRoot，不依赖 cwd。

  端口剖面（备用端口窗口）：
      -PortsProfile <alt-port-profile.json>
          读 tools/window/alt_port_profile.py 生成的清单里的 `ports_by_role`，
          用它替换固定表里的忙检/就绪端口（15xxx/17xxx 窗口必须给，否则会误报旧端口忙）。
      -Ports @{ 'db-srv' = @(16000,15100,15700); ... }
          直接按角色覆盖端口；与 -PortsProfile 同时给时 -Ports 优先。
          两者都不给时沿用固定表的旧端口（5000/6000/7000/7200…）。

  旧六个 exe 仍然可用且行为不变；本脚本不依赖它们。

  过期 pid 文件（卡 openmir2-start-stale-pid-rc）：
      主机重启后 pid 文件可能残留（记录里的 pid 已被别的进程复用，如 svchost）。
      实跑前按**三条同时成立**判「可证明过期」，缺一条都不得放行：
          ① pid 文件存在且至少一条角色记录；
          ② 记录里的**每个端口**都 DOWN（Test-Port，与 stop_mir2_server.ps1 同口径）；
          ③ 记录里的**每个 pid** 要么进程不存在，要么存活但进程名与记录不符
             （记录口径：`mir2-server` | 记录 `exe` 的 basename，同 stop 脚本）。
      三条成立 ⇒ 打印 `STALE_PID_FILE` + `STALE_REASON`（逐条点名判定依据），把 pid 文件
      改名为 `<pid 文件>.stale-<yyyyMMdd-HHmmss>` 保留证据后继续起栈；`-ResetStalePid`
      只是对**同一判定**的显式确认（不放宽任何一条）。
      缺任一条 ⇒ RED 拒绝：有记录 pid 存活且进程名匹配（真栈还在）、任一记录端口仍 UP、
      pid 非法/JSON 损坏（无法确认）。本脚本**绝不**为此结束任何进程。

  退出码：0 = 全部角色就绪（或 -WhatIf 通过）；1 = RED（已有存活栈 / 端口忙 / MySQL 缺失 /
          角色未就绪）；2 = 参数或前置不满足（未知角色 / 缺配置 / 端口剖面非法 /
          pid 文件无法确认 / ServerExe 缺失）。**任何 RED/未确认都不返回 0**。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 -WhatIf

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 -Seed 42

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 -Roles db-srv,login-srv,game-svr

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
    -Root E:\tmp\altport-deploy -PortsProfile E:\tmp\altport-deploy\alt-port-profile.json -WhatIf

.EXAMPLE
  # 主机重启后残留 pid 文件被别的进程复用：三条判据成立即自动判过期并保留证据后起栈；
  # -ResetStalePid 仅作显式确认（判定依据相同，不放宽任何一条）。
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 -ResetStalePid
#>
param(
  [string]$Root = 'E:\MirServer',
  [string]$ServerExe = '',
  [string[]]$Roles = @('db-srv', 'login-srv', 'game-svr', 'login-gate', 'sel-gate', 'game-gate'),
  [string]$Seed = '',
  [string]$RngLog = '',
  [string]$DbName = '',
  [string]$LogDir = '',
  [int]$WaitSeconds = 60,
  [string]$PortsProfile = '',
  [hashtable]$Ports = $null,
  [switch]$WhatIf,
  [switch]$RequireRelease,
  [switch]$ResetStalePid
)

$ErrorActionPreference = 'Stop'

# 六个角色的固定表：启动顺序 = 数组顺序；stop_order 越大越先停（停止脚本按降序）。
# conf 候选（卡 openmir2-g0-conf-map-option-a）：login-srv 以 rust/测试系名 logsrv.conf 优先
# （本脚本是 Rust 启动器，现役测试链默认根 rust-root 两候选并存——生产族 config.conf 在后，
# 否则测试窗口 login-srv 解析到生产族值、撞 C# LoginSrv 端口并连生产库，R22-C-F1 Critical）；
# db-srv 维持生产实况在前（rust-root 无 DBServer、E:/MirServer 无 DBSrv，今日无混存根）。
# 多候选并存 ⇒ WARN「ambiguous root」（解析仍取第一份，告警让漂移可见）。
$RoleTable = @(
  [pscustomobject]@{ role = 'db-srv';     confCandidates = @('DBServer\dbsvr.conf', 'DBSrv\dbsvr.conf');      conf = ''; ports = @(6000, 5100, 5700); stop_order = 1 }
  [pscustomobject]@{ role = 'login-srv';  confCandidates = @('LoginSrv\logsrv.conf', 'LoginSrv\config.conf'); conf = ''; ports = @(5500, 5600);       stop_order = 2 }
  [pscustomobject]@{ role = 'game-svr';   confCandidates = @('M2GameSvr\Server.conf');  conf = ''; ports = @(5000);             stop_order = 3 }
  [pscustomobject]@{ role = 'login-gate'; confCandidates = @('LoginGate\config.conf');  conf = ''; ports = @(7000);             stop_order = 4 }
  [pscustomobject]@{ role = 'sel-gate';   confCandidates = @('SelGate\config.conf');    conf = ''; ports = @(7100);             stop_order = 5 }
  [pscustomobject]@{ role = 'game-gate';  confCandidates = @('RunGate\config.conf');    conf = ''; ports = @(7200, 7201, 7202); stop_order = 6 }
)

# ---- 端口剖面：-PortsProfile（alt_port_profile.json 的 ports_by_role）/ -Ports 覆盖。----
# 忙检（实跑）与就绪等待（Wait-RolePorts）都只读 $RoleTable[].ports，因此在这里一次性替换。
# 未知角色 / 缺角色 / 空端口 / 非 int => fail-closed = 退出 2。
$portsSource = 'default（旧端口 5000/6000/7000/7100/7200…）'
$knownRoleNames = @($RoleTable | ForEach-Object { $_.role })
if (-not [string]::IsNullOrWhiteSpace($PortsProfile)) {
  if (-not (Test-Path -LiteralPath $PortsProfile -PathType Leaf)) {
    Write-Output ("RED: -PortsProfile 不存在：{0}" -f $PortsProfile)
    exit 2
  }
  try {
    $profileDoc = Get-Content -LiteralPath $PortsProfile -Raw -ErrorAction Stop | ConvertFrom-Json
  } catch {
    Write-Output ("RED: -PortsProfile 不是合法 JSON：{0}" -f $PortsProfile)
    exit 2
  }
  if ($null -eq $profileDoc -or -not ($profileDoc.PSObject.Properties.Name -contains 'ports_by_role')) {
    Write-Output ("RED: -PortsProfile 缺 ports_by_role：{0}（需 alt_port_profile.py 生成的清单）" -f $PortsProfile)
    exit 2
  }
  $profileMap = @{}
  foreach ($prop in $profileDoc.ports_by_role.PSObject.Properties) {
    $vals = @($prop.Value | ForEach-Object { [int]$_ })
    if ($vals.Count -eq 0) {
      Write-Output ("RED: -PortsProfile 角色 {0} 端口为空" -f $prop.Name)
      exit 2
    }
    foreach ($v in $vals) {
      if ($v -lt 1 -or $v -gt 65535) {
        Write-Output ("RED: -PortsProfile 角色 {0} 端口非法：{1}" -f $prop.Name, $v)
        exit 2
      }
    }
    $profileMap[[string]$prop.Name] = $vals
  }
  $extra = @($profileMap.Keys | Where-Object { -not ($knownRoleNames -contains $_) })
  if ($extra.Count -gt 0) {
    Write-Output ("RED: -PortsProfile 含未知角色：{0}" -f ($extra -join ','))
    exit 2
  }
  foreach ($p in $RoleTable) {
    if (-not $profileMap.ContainsKey($p.role)) {
      Write-Output ("RED: -PortsProfile 缺角色 {0}（允许：{1}）" -f $p.role, ($knownRoleNames -join '|'))
      exit 2
    }
    $p.ports = @($profileMap[$p.role])
  }
  $portsSource = ("profile:" + $PortsProfile)
}
if ($null -ne $Ports) {
  $portKeys = @($Ports.Keys | ForEach-Object { [string]$_ })
  $extra = @($portKeys | Where-Object { -not ($knownRoleNames -contains $_) })
  if ($extra.Count -gt 0) {
    Write-Output ("RED: -Ports 含未知角色：{0}" -f ($extra -join ','))
    exit 2
  }
  foreach ($p in $RoleTable) {
    if ($portKeys -contains $p.role) {
      $vals = @($Ports[$p.role] | ForEach-Object { [int]$_ })
      if ($vals.Count -eq 0) {
        Write-Output ("RED: -Ports 角色 {0} 端口为空" -f $p.role)
        exit 2
      }
      foreach ($v in $vals) {
        if ($v -lt 1 -or $v -gt 65535) {
          Write-Output ("RED: -Ports 角色 {0} 端口非法：{1}" -f $p.role, $v)
          exit 2
        }
      }
      $p.ports = $vals
    }
  }
  $portsSource = 'commandline:-Ports'
}

if ([string]::IsNullOrWhiteSpace($LogDir)) {
  $LogDir = Join-Path $Root 'logs\mir2-server'
}
$PidFile = Join-Path $LogDir 'mir2-server.pid.json'

function Test-Port([int]$Port) {
  # 只读监听表；不用 Get-NetTCPConnection（本机偶发挂死，见 start_rust_stack.ps1 注释）。
  foreach ($ep in [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners()) {
    if ($ep.Port -eq $Port) { return $true }
  }
  return $false
}

function ConvertTo-ArgToken([string]$a) {
  # Start-Process 的 ArgumentList 不做自动引用；含空白/双引号时手动加引号。
  if ($a -match '[\s"]') { return ('"' + ($a -replace '"', '\"') + '"') }
  return $a
}

function Start-ChildProcess {
  param(
    [string]$Exe,
    [string[]]$ArgList,
    [string]$WorkDir,
    [string]$Out,
    [string]$Err,
    [hashtable]$ChildEnv
  )
  $keys = @($ChildEnv.Keys)
  $prev = @{}
  foreach ($k in $keys) {
    $prev[$k] = if (Test-Path -LiteralPath ("Env:" + $k)) { (Get-Item -LiteralPath ("Env:" + $k)).Value } else { $null }
  }
  try {
    foreach ($k in $keys) { Set-Item -Path ("Env:" + $k) -Value ([string]$ChildEnv[$k]) }
    $quoted = @($ArgList | ForEach-Object { ConvertTo-ArgToken $_ })
    return Start-Process -FilePath $Exe -ArgumentList $quoted -WorkingDirectory $WorkDir `
      -RedirectStandardOutput $Out -RedirectStandardError $Err -WindowStyle Hidden -PassThru
  } finally {
    foreach ($k in $keys) {
      if ($null -eq $prev[$k]) { Remove-Item -Path ("Env:" + $k) -ErrorAction SilentlyContinue }
      else { Set-Item -Path ("Env:" + $k) -Value ([string]$prev[$k]) }
    }
  }
}

function Wait-RolePorts {
  param([object]$Proc, [int[]]$Ports, [int]$Seconds)
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    $missing = @($Ports | Where-Object { -not (Test-Port $_) })
    if ($missing.Count -eq 0) { return $true }
    if ($Proc) {
      try { if ($Proc.HasExited) { return $false } } catch { return $false }
    }
    Start-Sleep -Milliseconds 500
  }
  return (@($Ports | Where-Object { -not (Test-Port $_) }).Count -eq 0)
}

function Stop-Gracefully {
  # 与 start_rust_stack.ps1 / game-svr 的 `openmir2-gamesvr-shutdown-<pid>` 管道同口径；
  # 其它角色没有该通道 => 直接返回 $false（调用方 Force）。
  param([object]$Entry, [object]$Proc)
  if ($Entry.role -ne 'game-svr') { return $false }
  try {
    $client = [System.IO.Pipes.NamedPipeClientStream]::new(
      '.', ("openmir2-gamesvr-shutdown-{0}" -f $Entry.pid), [System.IO.Pipes.PipeDirection]::Out)
    $client.Connect(1000)
    $bytes = [System.Text.Encoding]::ASCII.GetBytes('SHUTDOWN')
    $client.Write($bytes, 0, $bytes.Length)
    $client.Flush()
    $client.Dispose()
  } catch {
    return $false
  }
  $null = $Proc.WaitForExit(8000)
  return -not [bool](Get-Process -Id $Entry.pid -ErrorAction SilentlyContinue)
}

function Stop-RoleProcesses {
  param([object[]]$Entries)
  foreach ($e in @($Entries | Sort-Object { [int]$_.stop_order } -Descending)) {
    if ($null -eq $e -or $null -eq $e.pid) { continue }
    $procId = [int]$e.pid
    $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
    if (-not $p) {
      Write-Output ("ALREADY_GONE {0} pid={1}" -f $e.role, $procId)
      continue
    }
    if (Stop-Gracefully $e $p) {
      Write-Output ("STOPPED {0} pid={1} graceful" -f $e.role, $procId)
    } else {
      Stop-Process -Id $procId -Force -ErrorAction SilentlyContinue
      $null = $p.WaitForExit(8000)
      Write-Output ("STOPPED {0} pid={1} force" -f $e.role, $procId)
    }
  }
}

function Save-PidFile {
  param([object[]]$Entries)
  $payload = [pscustomobject]@{
    generated_at = (Get-Date).ToString('o')
    root         = $Root
    server_exe   = $ServerExe
    ports_source = $portsSource
    roles        = @($Entries)
  }
  $payload | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $PidFile -Encoding utf8
}

function Get-ExpectedProcessNames {
  # 与 stop_mir2_server.ps1 同口径：`mir2-server` | 记录 `exe` 的 basename（防止 pid 复用误判）。
  param([object]$Entry)
  $names = @('mir2-server')
  if ($Entry -and ($Entry.PSObject.Properties.Name -contains 'exe') -and $Entry.exe) {
    try {
      $base = [System.IO.Path]::GetFileNameWithoutExtension([string]$Entry.exe)
      if ($base) { $names += $base }
    } catch { }
  }
  return @($names | Where-Object { $_ } | Select-Object -Unique)
}

function Get-PidFileState {
  # 把 pid 文件的每条记录分成三态（卡 openmir2-start-stale-pid-rc）：
  #   live        = pid 存活且进程名与记录一致 ⇒ 我们的栈还在（不得起第二个）
  #   stale       = pid 不存在，或存活但进程名不符（典型：重启后 pid 被 svchost 复用）
  #   unconfirmed = pid 非法（无法判定）
  # 另算 portsUp = 记录里的端口当前仍被监听（Test-Port，与 stop 脚本同口径）。
  # 「可证明过期」= 有条目 且 live 为空 且 unconfirmed 为空 且 portsUp 为空（见调用点）。
  param([string]$Path)
  $state = [pscustomobject]@{
    exists      = $false
    parseOk     = $true
    entries     = @()
    live        = @()
    stale       = @()
    unconfirmed = @()
    portsUp     = @()
  }
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $state }
  $state.exists = $true
  $raw = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop
  if ([string]::IsNullOrWhiteSpace($raw)) { return $state }
  try { $parsed = $raw | ConvertFrom-Json } catch { $state.parseOk = $false; return $state }
  $entries = @()
  if ($null -ne $parsed) {
    if ($parsed.PSObject.Properties.Name -contains 'roles') { $entries = @($parsed.roles) }
    elseif ($parsed -is [array]) { $entries = @($parsed) }
    else { $entries = @($parsed) }
  }
  $entries = @($entries | Where-Object { $_ -and ($_.PSObject.Properties.Name -contains 'role') -and ($_.PSObject.Properties.Name -contains 'pid') })
  $state.entries = $entries
  foreach ($e in $entries) {
    $procId = 0
    if (-not [int]::TryParse([string]$e.pid, [ref]$procId)) {
      $state.unconfirmed += ("{0}(pid 非法:{1})" -f $e.role, $e.pid)
    } else {
      $p = Get-Process -Id $procId -ErrorAction SilentlyContinue
      if (-not $p) {
        $state.stale += ("{0}(pid={1} 进程不存在)" -f $e.role, $procId)
      } else {
        $expected = Get-ExpectedProcessNames $e
        if ($expected -notcontains $p.ProcessName) {
          $state.stale += ("{0}(pid={1} 进程名={2} 与记录 {3} 不符)" -f $e.role, $procId, $p.ProcessName, ($expected -join '|'))
        } else {
          $state.live += ("{0}(pid={1} 进程名={2} 存活)" -f $e.role, $procId, $p.ProcessName)
        }
      }
    }
    if (($e.PSObject.Properties.Name -contains 'ports') -and $null -ne $e.ports) {
      foreach ($port in @($e.ports | ForEach-Object { [int]$_ })) {
        $tag = ("{0}:{1}" -f $e.role, $port)
        if ((Test-Port ([int]$port)) -and ($state.portsUp -notcontains $tag)) { $state.portsUp += $tag }
      }
    }
  }
  return $state
}

# ---- ServerExe 默认值：优先 release，其次 debug（均相对本仓库的 mir2-rs/target）。----
# 卡 openmir2-start-server-whatif-sha：-WhatIf 必须能核对 sha256；release 缺失**不许静默**。
# 根解析只认 $PSScriptRoot（不依赖 cwd）：在 worktree 里跑就指向**该 worktree** 的 target。
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$exeWasExplicit = -not [string]::IsNullOrWhiteSpace($ServerExe)
$releaseExe = Join-Path $RepoRoot 'mir2-rs\target\release\mir2-server.exe'
$debugExe   = Join-Path $RepoRoot 'mir2-rs\target\debug\mir2-server.exe'
if ($exeWasExplicit) {
  $ServerExe = [System.IO.Path]::GetFullPath($ServerExe)
  $BuildType = 'Explicit'
} elseif (Test-Path -LiteralPath $releaseExe) {
  $ServerExe = $releaseExe
  $BuildType = 'Release'
} else {
  $ServerExe = $debugExe
  $BuildType = 'Debug-Fallback'
}
$ServerExeExists = Test-Path -LiteralPath $ServerExe -PathType Leaf
# 本机 PowerShell 5.1 上 Get-FileHash 不可用（实测 CommandNotFound）⇒ 用 .NET SHA256，任何 PS 版本都可跑。
$ServerExeSha256 = 'MISSING'
if ($ServerExeExists) {
  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  $fs = $null
  try {
    $fs = [System.IO.File]::OpenRead($ServerExe)
    $ServerExeSha256 = ([System.BitConverter]::ToString($sha256.ComputeHash($fs)) -replace '-', '').ToLowerInvariant()
  } finally {
    if ($fs) { $fs.Dispose() }
    $sha256.Dispose()
  }
}

# ---- 角色选择（按固定表顺序；未知角色 fail-closed = 退出 2）。----
$wanted = New-Object System.Collections.Generic.List[string]
foreach ($r in $Roles) {
  foreach ($x in ([string]$r -split ',')) {
    $t = $x.Trim()
    if ($t -and -not $wanted.Contains($t)) { $wanted.Add($t) }
  }
}
$knownRoles = @($RoleTable | ForEach-Object { $_.role })
$unknown = @($wanted | Where-Object { -not ($knownRoles -contains $_) })
if ($unknown.Count -gt 0) {
  Write-Output ("RED: 未知角色 {0}（允许：{1}）" -f ($unknown -join ','), ($knownRoles -join '|'))
  exit 2
}
$plan = @($RoleTable | Where-Object { $wanted -contains $_.role })
if ($plan.Count -eq 0) {
  Write-Output 'RED: -Roles 为空（至少给一个角色）'
  exit 2
}

# ---- 配置存在性（WhatIf 与实跑同一判据；缺一即退出 2）。----
# 每角色按 confCandidates 顺序解析：第一份存在的候选生效；多候选并存 ⇒ WARN
# 「ambiguous root」（解析仍取第一份，告警让根漂移可见）；全部缺失才 RED 并点名全部
# 候选（卡 openmir2-g0-conf-map-option-a）。
$missing = @()
$ambiguous = @()
foreach ($p in $plan) {
  $resolved = $null
  $resolvedOthers = @()
  foreach ($cand in $p.confCandidates) {
    $confPath = Join-Path $Root $cand
    if (Test-Path -LiteralPath $confPath -PathType Leaf) {
      if ($null -eq $resolved) { $resolved = $cand }
      else { $resolvedOthers += $cand }
    }
  }
  if ($null -eq $resolved) {
    $missing += ("{0} -> {1}" -f $p.role, (($p.confCandidates | ForEach-Object { Join-Path $Root $_ }) -join ' OR '))
  } else {
    $p.conf = $resolved
    if ($resolvedOthers.Count -gt 0) {
      $ambiguous += ("{0} -> 取 {1}（还有 {2}）" -f $p.role, $resolved, ($resolvedOthers -join ', '))
    }
  }
}
foreach ($a in $ambiguous) { Write-Output ("WARN: ambiguous root: " + $a) }
if ($missing.Count -gt 0) {
  Write-Output ('RED: 缺配置：' + ($missing -join '; '))
  Write-Output ('RED: -Root={0}；本工具要求六角色配置按候选映射存在（见 -WhatIf 用法）' -f $Root)
  exit 2
}

# ---- 种子 / 库名（口径对齐 start_rust_stack.ps1；非法值 fail-closed = 退出 2）。----
$seedText = if (-not [string]::IsNullOrWhiteSpace($Seed)) { $Seed.Trim() }
            elseif (-not [string]::IsNullOrWhiteSpace($env:MIR2_RNG_SEED)) { $env:MIR2_RNG_SEED.Trim() }
            else { '42' }
$seedValue = 0
if (-not [int]::TryParse($seedText, [ref]$seedValue)) {
  Write-Output ("RED: -Seed/MIR2_RNG_SEED 不是合法 int：{0}" -f $seedText)
  exit 2
}

$dbNameText = if (-not [string]::IsNullOrWhiteSpace($DbName)) { $DbName.Trim() }
              elseif (-not [string]::IsNullOrWhiteSpace($env:MIR2_DB_NAME)) { $env:MIR2_DB_NAME.Trim() }
              else { '' }
if ($dbNameText -and ($dbNameText -notmatch '^[A-Za-z0-9_]+(\*)?$')) {
  Write-Output ("RED: -DbName/MIR2_DB_NAME 非法（只允许字母/数字/下划线，可带一个尾部 *）：{0}" -f $dbNameText)
  exit 2
}

$childEnv = @{ 'MIR2_RNG_SEED' = $seedText }
if ($dbNameText) { $childEnv['MIR2_DB_NAME'] = $dbNameText }

# ---- RNG 流水（卡 openmir2-window-kit-p0-unblock）：显式 -RngLog > 环境 MIR2_RNG_LOG。----
# 与 start_rust_stack.ps1 同口径：给了流水就必须有种子（Seed 恒有默认 42）；
# 父目录必须已存在（证据目录由调用方建好），否则 fail-closed = 退出 2。
$rngLogText = if (-not [string]::IsNullOrWhiteSpace($RngLog)) { $RngLog.Trim() }
              elseif (-not [string]::IsNullOrWhiteSpace($env:MIR2_RNG_LOG)) { $env:MIR2_RNG_LOG.Trim() }
              else { '' }
if ($rngLogText) {
  $rngParent = Split-Path -Parent $rngLogText
  if ([string]::IsNullOrWhiteSpace($rngParent) -or -not (Test-Path -LiteralPath $rngParent -PathType Container)) {
    Write-Output ("RED: -RngLog 的父目录不存在：{0}（先建证据目录，fail-closed）" -f $rngLogText)
    exit 2
  }
  $childEnv['MIR2_RNG_LOG'] = $rngLogText
  # C# rng-hook 只有 MIR2_RNG_SITE=1 才写 5 列（含 site）；不设则 4 列、账本校验必红。
  $childEnv['MIR2_RNG_SITE'] = '1'
}

Write-Output ("ROOT {0}" -f $Root)
Write-Output ("SERVER_EXE {0}" -f $ServerExe)
Write-Output ("SERVER_EXE_SHA256 {0}" -f $ServerExeSha256)
Write-Output ("SERVER_EXE_BUILD {0}" -f $BuildType)
if ($BuildType -eq 'Debug-Fallback') {
  Write-Output ("WARNING: release 二进制不存在（{0}），将使用 DEBUG：{1} —— 本次窗口证据会记 debug 构建；先 cargo build --release -p mir2-server，或加 -RequireRelease 让它直接失败。" -f $releaseExe, $ServerExe)
}
if (-not $ServerExeExists) {
  Write-Output ("WARNING: SERVER_EXE 不存在：{0}（sha256 无法计算）" -f $ServerExe)
}
if ($RequireRelease -and $BuildType -ne 'Release') {
  Write-Output ("RED: -RequireRelease 要求 release 构建，实际 {0}：{1}" -f $BuildType, $ServerExe)
  exit 2
}
Write-Output ("LOG_DIR {0}" -f $LogDir)
Write-Output ("PORTS_SOURCE {0}" -f $portsSource)
foreach ($p in $RoleTable) {
  Write-Output ("PORTS {0}: {1}" -f $p.role, (($p.ports) -join ','))
}
Write-Output ("SEED: 将向角色注入 MIR2_RNG_SEED={0}（显式 -Seed > 环境变量 > 默认 42，口径同 start_rust_stack.ps1）" -f $seedText)
Write-Output ("RNG_LOG: " + $(if ($rngLogText) { "将向角色注入 MIR2_RNG_LOG=" + $rngLogText + "（流水由 rng_trace 写 TSV：seq/kind/max/result/site；同时注入 MIR2_RNG_SITE=1 供 C# 钩子写 5 列）" } else { "(none；未给 -RngLog/MIR2_RNG_LOG，不落流水)" }))
if ($dbNameText) {
  Write-Output ("DB_NAME: 将向角色注入 MIR2_DB_NAME={0}（仅环境透传，当前无消费者；库名仍由各配置文件决定）" -f $dbNameText)
} else {
  Write-Output 'DB_NAME: 未指定（不注入 MIR2_DB_NAME；库名由各配置文件决定）'
}

# ---- -WhatIf：只打印计划，不启动任何进程。----
if ($WhatIf) {
  if (-not $ServerExeExists) {
    Write-Output ("RED: ServerExe 不存在：{0}（-WhatIf 无法核对 sha256；先构建 mir2-server 或用 -ServerExe 指定）" -f $ServerExe)
    exit 2
  }
  $order = 1
  foreach ($p in $plan) {
    $confPath = Join-Path $Root $p.conf
    Write-Output ('COMMAND {0} {1} [ports {2}]: "{3}" {1} "{4}"' -f $order, $p.role, (($p.ports) -join ','), $ServerExe, $confPath)
    $order++
  }
  Write-Output 'MYSQL_EXTERNAL: 3306（外部依赖，不由本脚本启动/停止）'
  Write-Output ('WHATIF: {0} 个角色；启动顺序 {1}；停止顺序 {2}' -f $plan.Count,
    (($plan | ForEach-Object { $_.role }) -join ' -> '),
    ((@($plan | Sort-Object { [int]$_.stop_order } -Descending | ForEach-Object { $_.role })) -join ' -> '))
  exit 0
}

# ---- 实跑前置：exe 存在、pid 文件无存活实例、MySQL 外部依赖在、目标端口空闲。----
if (-not (Test-Path -LiteralPath $ServerExe -PathType Leaf)) {
  Write-Output ("RED: ServerExe 不存在：{0}（先构建 mir2-server，或显式 -ServerExe）" -f $ServerExe)
  exit 2
}

# ---- 现有 pid 文件：live / port-up / unconfirmed / 可证明过期 四选一（卡 openmir2-start-stale-pid-rc）。----
# 「可证明过期」必须**三条同时成立**（缺一条都不得放行）：
#   ① pid 文件存在且至少一条角色记录；
#   ② 记录里的每个端口都 DOWN；
#   ③ 记录里的每个 pid 都不存在，或存活但进程名与记录不符。
# 其余一律 RED（非零退出）：live => 1；unconfirmed(pid 非法/JSON 坏) => 2；portsUp => 1。
$pidState = Get-PidFileState $PidFile
if (-not $pidState.parseOk) {
  Write-Output ("RED: pid 文件不是合法 JSON：{0} —— 无法确认是否有存活栈，不猜、不静默（-ResetStalePid 不绕过未确认）" -f $PidFile)
  exit 2
}
if ($pidState.live.Count -gt 0) {
  Write-Output ("RED: 已有存活角色（{0}）记录在 {1} —— 先跑 stop_mir2_server.ps1 再起" -f (($pidState.live) -join ','), $PidFile)
  if ($pidState.portsUp.Count -gt 0) { Write-Output ("RED: 记录端口仍 UP：{0}" -f ($pidState.portsUp -join ',')) }
  exit 1
}
if ($pidState.unconfirmed.Count -gt 0) {
  Write-Output ("RED: pid 文件有无法确认的记录：{0} —— 无法证明栈已停，不猜（先跑 stop_mir2_server.ps1 或人工核对 {1}）" -f (($pidState.unconfirmed) -join '; '), $PidFile)
  exit 2
}
if ($pidState.portsUp.Count -gt 0) {
  Write-Output ("RED: pid 文件记录的端口仍被监听：{0} —— 不能判为过期（先停现有栈或核查占用者；本脚本不杀别人的进程）" -f ($pidState.portsUp -join ','))
  exit 1
}
if ($pidState.entries.Count -gt 0) {
  # 三条同时成立 => 可证明过期。逐条点名判定依据（运维需要能复核），备份后继续起栈。
  Write-Output ("STALE_PID_FILE {0}" -f $PidFile)
  Write-Output ("STALE_REASON ① pid 文件存在（{0} 条角色记录）；② 记录端口全 DOWN；③ 记录进程均不存在或进程名不符：{1}" -f $pidState.entries.Count, (($pidState.stale) -join '; '))
  if ($ResetStalePid) { Write-Output 'STALE_RESET explicit -ResetStalePid（显式确认；判定依据与自动路径相同，不额外放宽）' }
  $staleBackup = $PidFile + '.stale-' + (Get-Date).ToString('yyyyMMdd-HHmmss')
  Move-Item -LiteralPath $PidFile -Destination $staleBackup -Force -ErrorAction Stop
  Write-Output ("STALE_PID_FILE_MOVED {0}（保留证据；确认无用后可删）" -f $staleBackup)
}

if (-not (Test-Port 3306)) {
  Write-Output 'RED: MySQL 3306 未监听（外部依赖；本脚本不启动 MySQL）—— 先由外部起库再起栈'
  exit 1
}
Write-Output 'MYSQL_EXTERNAL 3306 UP'

$busy = @()
foreach ($p in $plan) {
  foreach ($port in $p.ports) { if (Test-Port ([int]$port)) { $busy += ("{0}:{1}" -f $p.role, $port) } }
}
if ($busy.Count -gt 0) {
  Write-Output ("RED: 目标端口已被监听：{0} —— 先停现有栈再起（本脚本不回滚别人的进程）" -f ($busy -join ','))
  exit 1
}

# ---- 起栈：逐角色等待端口就绪；任一失败 => RED + 反向回滚已起角色。----
# LogDir 只在实跑分支创建（-WhatIf 与前置失败都不得在部署根留目录/文件）。
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$started = @()
try {
  foreach ($p in $plan) {
    $confPath = Join-Path $Root $p.conf
    $workDir  = Split-Path -Parent $confPath
    $out = Join-Path $LogDir ($p.role + '.out.log')
    $err = Join-Path $LogDir ($p.role + '.err.log')
    Remove-Item -LiteralPath $out, $err -Force -ErrorAction SilentlyContinue
    $argList = @($p.role, $confPath)
    $proc = Start-ChildProcess -Exe $ServerExe -ArgList $argList -WorkDir $workDir -Out $out -Err $err -ChildEnv $childEnv
    $entry = [pscustomobject]@{
      role       = $p.role
      pid        = [int]$proc.Id
      ports      = @($p.ports | ForEach-Object { [int]$_ })
      stop_order = [int]$p.stop_order
      exe        = $ServerExe
      args       = @($argList)
    }
    $started += $entry
    Save-PidFile -Entries $started
    Write-Output ("START {0} pid={1} ports={2}" -f $p.role, $proc.Id, (($p.ports) -join ','))
    if (-not (Wait-RolePorts -Proc $proc -Ports @($p.ports) -Seconds $WaitSeconds)) {
      $alive = -not [bool]$proc.HasExited
      $miss = @($p.ports | Where-Object { -not (Test-Port ([int]$_)) })
      Write-Output ("RED: {0} 未就绪（缺端口 {1}；进程存活={2}）；err 日志：{3}" -f $p.role, ($miss -join ','), $alive, $err)
      throw ("角色 {0} 启动失败（fail-closed）" -f $p.role)
    }
    Write-Output ("CORE_UP {0} ports={1}" -f $p.role, (($p.ports) -join ','))
  }
} catch {
  Write-Output ("RED: " + $_.Exception.Message)
  Write-Output 'RED: 回滚已启动角色（停止顺序 = 启动反向）'
  Stop-RoleProcesses -Entries $started
  Remove-Item -LiteralPath $PidFile -Force -ErrorAction SilentlyContinue
  exit 1
}

Save-PidFile -Entries $started
Write-Output ("PIDFILE " + $PidFile)
Write-Output ('MIR2_SERVER_STACK_OK ' + $PidFile)
exit 0
