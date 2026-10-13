#requires -Version 5
<#
.SYNOPSIS
  run_hero_pet_pair.ps1 — M3⑨「两侧同序列」英雄/宠物（宝宝/下属 slave）对拍的一条命令
  （卡 `openmir2-m3-hero-pet-fixture-botsrv`；父卡 `openmir2-m3-hero-pet-align`）。

.DESCRIPTION
  目标（父卡 DoD②③）：把**同一串召唤/观测动作**（召唤 17/20/30/41 → 观测宝宝/名色 → 主人经验/等级）
  分别打在 C# 栈与 Rust 栈上 ⇒ ① `hero_pet_diff.py diff` 逐 (bot, k) 字段级 diff = 0
  （帧种类序列 + 声明标量 + 文本帧 + 主人 exp/level + 宝宝多重集 + 金币 + 背包多重集 + branch）；
  ② `scoped_diff.py diff --canonical` 在 DB 侧 diff = 0（= oracle「宝宝态不落库」口径，见盘点报告 §1.H）。

  本脚本**不新造编排**：一次「侧运行」复用 `run_pair.ps1`（两相固定起点 + 预热相 `-StopAfterCharCreated` +
  BEFORE/AFTER 快照 + 收尾「等断线存档落库」settle 仪器），本脚本只加：
    ① 入口实现识别护栏（-Side 与 5000/7000/7100/7200 的实际监听进程必须对应）；
    ② `-HeroPetSequence` 步骤串参数透传 + `M3HEROPETSEQ` 标记判据（**两个假人各自的 send/end/done**
       没跑完 ⇒ RED，绝不当 GREEN）；
    ③ 跨侧 `hero_pet_diff.py`（英雄/宠物记录）+ `scoped_diff.py`（DB 字段级）两道判据；
    ④ `-SelfCheckNoStack` 无栈自检（比较器 selftest + 合成件正/负样本 + BotSrv 生产函数 selftest +
       入口/DiffOnly/AssembleOnly/preflight fail-closed + 零端口断言）。

  两假人相邻固定起点：本夹具复用 `run_pair.ps1 -PinStartPose -AttackPinPair`（主人钉在 (X,Y) 朝向 2、
  peer 钉在右邻格 (X+1,Y)）——宝宝由服务端 `WorldEngine.RegenMonsterByName` 生成（`BaseObject.cs:1190`），
  随主人会话存活；星零 AI 消费者地图 `0137` 自身不刷怪 ⇒「宝宝击杀」档要窗口配合 MonGen 驱动 + pin 一只固定怪。

  真机跑要窗口（起栈禁接管道）：未给 `-Go` 时 real-run **fail-closed 拒跑（exit 2）**，不占端口。
  前置（T-27 同族口径）：Rust 侧 ⑨-2/⑨-3（`crates/world`/`bins/game-svr` 宝宝召唤 + 生命周期）落地前
  `-Side rust` 机判 preflight 直接 PREREQ（exit 2），避免白占窗口。

  Mode：
    -Side csharp|rust [可选参数]     跑一侧（复用 run_pair.ps1）。
    -Side rust -PeerSession <C#会话> 跑 Rust 侧并跨侧 diff（= 本卡的正式判据）。
    -DiffOnly -CsharpRaw <f> -RustRaw <f>  只重比既有原始件（**不碰端口**）。
    -AssembleOnly -Markers <f> -Out <f>  只把 BotSrv 的 `M3HEROPETSEQ` 事件流合并成记录。
    -SelfCheckNoStack                无栈自检：所有路径都必须 fail-closed。

  窗口纪律：除 -DiffOnly/-AssembleOnly/-SelfCheckNoStack 外都会连真栈；起两侧栈前先看
  `E:\MirServer\port-status.ps1`，与 M2/M3 其它窗口严格串行（**别并行起栈**）。
  本脚本**不启动/不停止任何服务**——栈的起停仍由既有脚本/人工负责。

.EXAMPLE
  # ① C# 基线（C# 栈在听；两假人相邻 pin + 固定召唤序列）
  powershell -ExecutionPolicy Bypass -File run_hero_pet_pair.ps1 -Side csharp -HeroPetCase summon_skeleton -Go
  # ② 换成 Rust 栈后跑 Rust 侧并与上面的 C# 会话对拍
  powershell -ExecutionPolicy Bypass -File run_hero_pet_pair.ps1 -Side rust -HeroPetCase summon_skeleton -PeerSession csharp-... -Go
  # ③ 只重比既有产物（不碰端口）
  powershell -ExecutionPolicy Bypass -File run_hero_pet_pair.ps1 -DiffOnly -CsharpRaw <f> -RustRaw <f>
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_hero_pet_pair.ps1 -SelfCheckNoStack
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 2,
  [string]$Prefix = '',
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$TimeoutSec = 600,
  [string]$PeerSession = '',
  [string]$CsharpSession = '',
  [string]$RustSession = '',
  [string]$CsharpRaw = '',
  [string]$RustRaw = '',
  [string]$ScenarioFile = '',
  [ValidateSet('summon_skeleton', 'summon_dragon', 'summon_angel', 'tame_beast', 'cap_one', 'mutiny_time_gate', 'master_vs_slave_exp')][string]$HeroPetCase = 'summon_skeleton',
  [string]$HeroPetSteps = '',    # 空 ⇒ 由场景 case.steps 拼成 `role|op|param;...`
  [int]$HeroPetStepMs = 1000,
  [int]$HeroPetStartDelayMs = 3000,
  [int]$HeroPetMasterRole = 0,
  [int]$HeroPetPeerRole = 1,
  [int]$PinLevel = 45,
  [string]$PortsCsv = '',          # 入口端口（逗号分隔）；默认 5000,7000,7100,7200
  [switch]$DiffOnly,
  [switch]$AssembleOnly,
  [string]$Markers = '',
  [string]$Out = '',
  [switch]$Go,                     # owner 发放窗口的显式授权；未给 ⇒ real-run fail-closed（exit 2）
  [switch]$SkipRustPreflight,      # 仅供调试：跳过「Rust 侧宝宝召唤未接线」前置（默认不跳）
  [switch]$SelfCheckNoStack,
  [string]$SelfCheckOut = ''
)

$ErrorActionPreference = 'Stop'
$Ports = if ([string]::IsNullOrEmpty($PortsCsv)) {
  @(5000, 7000, 7100, 7200)
} else {
  @($PortsCsv.Split(',') | ForEach-Object { [int]$_.Trim() })
}
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
$RunPair = Join-Path $Here 'run_pair.ps1'
$Scoped = Join-Path $Here 'scoped_diff.py'
$PairDiff = Join-Path $Here 'hero_pet_diff.py'
$RunBots = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$Sessions = Join-Path $Here 'sessions'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
if ([string]::IsNullOrEmpty($ScenarioFile)) { $ScenarioFile = Join-Path $Here 'hero_pet_scenario.json' }
. (Join-Path $Here 'gate_guards.ps1')
. (Join-Path $Here 'm3hashutil.ps1')

# 脚本作用域：本脚本（含自检）**从不**绑定/连接任何端口；real-run 由外部栈 + run_pair 负责。
$script:HeroPetTouchedPorts = $false

# ---- 场景固定输入（case.steps → `role|op|param;...`） ------------------------
function Get-HeroPetScenario {
  if (-not (Test-Path -LiteralPath $ScenarioFile)) { throw ("找不到场景文件: {0}" -f $ScenarioFile) }
  return (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)
}
$script:Scenario = $null
$script:HeroPetStepsArg = $HeroPetSteps
function Ensure-HeroPetDefaults {
  if ($null -eq $script:Scenario) { return }
  $case = $script:Scenario.cases.$HeroPetCase
  if ($null -eq $case) { throw ("场景 {0} 里没有 case '{1}'" -f $ScenarioFile, $HeroPetCase) }
  if ([string]::IsNullOrEmpty($script:HeroPetStepsArg)) {
    $script:HeroPetStepsArg = (@($case.steps | ForEach-Object { "{0}|{1}|{2}" -f $_.role, $_.op, $_.param }) -join ';')
  }
}

function Get-EntryHolders {
  $list = @()
  foreach ($p in $Ports) {
    $conn = Get-NetTCPConnection -State Listen -LocalPort $p -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $conn) {
      $list += [pscustomobject]@{ port = $p; process = '(无监听)'; path = ''; impl = 'none' }
      continue
    }
    $proc = Get-Process -Id $conn.OwningProcess -ErrorAction SilentlyContinue
    $impl = 'rust-or-other'
    if ($proc -and $proc.Path -like 'E:\MirServer\*') { $impl = 'csharp' }
    $pname = if ($proc) { $proc.ProcessName } else { '(未知)' }
    $ppath = if ($proc) { $proc.Path } else { '' }
    $list += [pscustomobject]@{ port = $p; process = $pname; path = $ppath; impl = $impl }
  }
  return $list
}

function Test-MysqlTcp {
  try {
    $c = New-Object System.Net.Sockets.TcpClient
    $iar = $c.BeginConnect('127.0.0.1', 3306, $null, $null)
    $ok = $iar.AsyncWaitHandle.WaitOne(1500)
    $c.Close()
    return $ok
  } catch { return $false }
}

function Newest-Session([string]$side) {
  $dirs = @(Get-ChildItem $Sessions -Directory -Filter ("{0}-*" -f $side) -ErrorAction SilentlyContinue |
            Where-Object { Test-Path (Join-Path $_.FullName 'baseline.json') } |
            Sort-Object LastWriteTime -Descending)
  if ($dirs.Count -gt 0) { return $dirs[0] } else { return $null }
}

# preflight（T-27 同族口径）：⑨-2/⑨-3 落地前 `crates/world`/`bins/game-svr` 无宝宝召唤/生命周期
# ⇒ 跑下去 Rust 侧只会「召唤失败/无宝宝」、跨侧 diff 必红。机判：Rust 侧出现任一 slave 标记。
# 承接卡 openmir2-m3-pet-slave-summon / -pet-slave-lifecycle。
function Test-RustHeroPetDispatchImplemented {
  param([string[]]$SrcDirs = @())
  if ($SrcDirs.Count -eq 0) {
    $SrcDirs = @(
      (Join-Path $RepoRoot 'mir2-rs\crates\world\src'),
      (Join-Path $RepoRoot 'mir2-rs\bins\game-svr\src')
    )
  }
  foreach ($d in $SrcDirs) {
    if (-not (Test-Path $d)) { continue }
    $hits = @(Get-ChildItem $d -Recurse -Filter '*.rs' -ErrorAction SilentlyContinue |
              Select-String -Pattern 'make_slave|MakeSlave|slave_list|SlaveList|summon_slave|SummonSlave|CHECKROYALTY|check_royalty' -ErrorAction SilentlyContinue)
    if ($hits.Count -gt 0) { return $true }
  }
  return $false
}

# CrossSideDiff 的退出码（脚本作用域）：不能把带 Write-Output 的函数直接写进 `exit (...)`。
$script:HeroPetDiffRc = 2
function Invoke-HeroPetRawDiff([string]$csharpPath, [string]$rustPath) {
  $script:HeroPetDiffRc = 2
  if (-not (Test-Path -LiteralPath $csharpPath)) { Write-Output ("PREREQ_MISSING: C# 原始件缺失 -> {0}" -f $csharpPath); return }
  if (-not (Test-Path -LiteralPath $rustPath)) { Write-Output ("PREREQ_MISSING: Rust 原始件缺失 -> {0}" -f $rustPath); return }
  Write-Output '--- hero_pet_diff.py diff（英雄/宠物逐 (bot,k) 字段级 + 帧序列 + 宝宝多重集 + exp/level） ---'
  & python -I $PairDiff diff $csharpPath $rustPath
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: 英雄/宠物跨侧逐 (bot,k) diff = 0'; $script:HeroPetDiffRc = 0; return }
  if ($LASTEXITCODE -eq 2) { Write-Output 'PREREQ: hero_pet_diff 判缺件/坏件 —— 不得当 GREEN'; $script:HeroPetDiffRc = 2; return }
  Write-Output 'RED: 英雄/宠物跨侧 diff != 0（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）'
  $script:HeroPetDiffRc = 1
}

$script:HeroPetScopedRc = 2
function Invoke-HeroPetScopedDiff([string]$csharpDir, [string]$rustDir) {
  $script:HeroPetScopedRc = 2
  $a = Join-Path $csharpDir 'scoped_after_norm.jsonl'
  $b = Join-Path $rustDir 'scoped_after_norm.jsonl'
  if (-not (Test-Path -LiteralPath $a)) { Write-Output ("PREREQ_MISSING: C# 会话缺 scoped_after_norm.jsonl -> {0}" -f $a); return }
  if (-not (Test-Path -LiteralPath $b)) { Write-Output ("PREREQ_MISSING: Rust 会话缺 scoped_after_norm.jsonl -> {0}" -f $b); return }
  Write-Output '--- 跨侧作用域 DB 字段级 diff（scoped_diff --canonical；宝宝态不落库口径） ---'
  & python -I $Scoped diff $a $b --canonical --fixed-newchr
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: 跨侧作用域内 DB 字段级 diff = 0（宝宝态不落库口径）'; $script:HeroPetScopedRc = 0; return }
  Write-Output 'RED: 跨侧 DB diff != 0'
  $script:HeroPetScopedRc = 1
}

# =========================================================================== #
# Mode: SelfCheckOut —— 把无栈自检转录**由脚本自己**以 LF 落盘
# =========================================================================== #
if ($SelfCheckNoStack -and -not [string]::IsNullOrEmpty($SelfCheckOut)) {
  $selfTmp = Join-Path $env:TEMP ('heropet-selfcheck-' + [guid]::NewGuid().ToString('N') + '.txt')
  $childArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath, '-SelfCheckNoStack')
  if (-not [string]::IsNullOrEmpty($PortsCsv)) { $childArgs += @('-PortsCsv', $PortsCsv) }
  try {
    & powershell @childArgs *> $selfTmp
    $selfRc = $LASTEXITCODE
    $selfText = [System.IO.File]::ReadAllText($selfTmp)
    Write-M3LfText -Path $SelfCheckOut -Text $selfText
    [Console]::Out.Write((ConvertTo-M3LfText $selfText))
    exit $selfRc
  } finally {
    Remove-Item -LiteralPath $selfTmp -Force -ErrorAction SilentlyContinue
  }
}

# =========================================================================== #
# Mode: SelfCheckNoStack —— 无栈自检（机器可判，不碰真端口）
# =========================================================================== #
if ($SelfCheckNoStack) {
  Write-Output 'SELFTEST_NO_STACK: 无栈自检——所有路径都必须 fail-closed（缺失绝不静默 GREEN）'
  $script:PtPass = 0
  $script:PtFail = @()
  $tmpRoot = Join-Path $env:TEMP ('heropet-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  function Add-Result([string]$Name, [bool]$Ok, [string]$Detail) {
    $suffix = if ([string]::IsNullOrEmpty($Detail)) { '' } else { " ($Detail)" }
    if ($Ok) {
      $script:PtPass++
      Write-Output ("SELFTEST ok {0}{1}" -f $Name, $suffix)
    } else {
      $script:PtFail += $Name
      Write-Output ("SELFTEST FAIL {0}{1}" -f $Name, $suffix)
    }
  }

  function New-HeroPetRaw([string]$Path) {
    $records = @(
      '{"bot":"m3hp0","k":0,"role":"a","op":"observe","ran":true,"param":"","map":"0137","frames":[],"slaves_before":[],"slaves_after":[],"slave_count_before":0,"slave_count_after":0,"exp_before":100,"exp_after":100,"level_before":1,"level_after":1,"gold_before":0,"gold_after":0,"bag_before":[],"bag_after":[],"branch":"ok"}',
      '{"bot":"m3hp0","k":1,"role":"a","op":"summon","ran":true,"param":"17","map":"0137","frames":[{"sm":17,"a":0,"b":0,"c":0,"d":0}],"slaves_before":[],"slaves_after":["(@m3hp0)骷髅#0"],"slave_count_before":0,"slave_count_after":1,"exp_before":100,"exp_after":100,"level_before":1,"level_after":1,"gold_before":0,"gold_after":0,"bag_before":[],"bag_after":[],"branch":"ok"}',
      '{"bot":"m3hp1","k":0,"role":"b","op":"observe","ran":true,"param":"","map":"0137","frames":[],"slaves_before":[],"slaves_after":[],"slave_count_before":0,"slave_count_after":0,"exp_before":100,"exp_after":100,"level_before":1,"level_after":1,"gold_before":0,"gold_after":0,"bag_before":[],"bag_after":[],"branch":"ok"}',
      '{"bot":"m3hp1","k":1,"role":"a","op":"summon","ran":false,"param":"17","map":"0137","frames":[],"slaves_before":[],"slaves_after":[],"slave_count_before":0,"slave_count_after":0,"exp_before":100,"exp_after":100,"level_before":1,"level_after":1,"gold_before":0,"gold_after":0,"bag_before":[],"bag_after":[],"branch":"skip_other_role"}'
    )
    Write-M3LfLines -Path $Path -Lines $records
  }

  try {
    # ① 场景固定输入（case 表 + 默认 case + 两假人 master/peer 序号 + steps 可拼）
    $scOk = $false
    try {
      $sc = Get-HeroPetScenario
      $case = $sc.cases.$($sc.default_case)
      $scOk = ($sc.canon -eq 'm3-hero-pet-v1') -and ($null -ne $case) -and
              (@($case.steps).Count -ge 1) -and ($sc.bots -eq 2) -and
              ($sc.master_role -eq 0) -and ($sc.peer_role -eq 1) -and
              ($null -ne $sc.cases.summon_dragon) -and ($null -ne $sc.cases.cap_one)
    } catch { $scOk = $false }
    Add-Result 'scenario-fixed-input' $scOk 'canon/cases/summon_skeleton/summon_dragon/cap_one'

    # ② 比较器 selftest（fail-closed + 改坏必红）必须 GREEN
    $stOut = ''
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $stOut = (& python -I $PairDiff selftest 2>&1 | Out-String)
    $stRc = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    $stMatch = [regex]::Match($stOut, 'SELFTEST ok=(\d+)/(\d+)')
    $stOk = ($stRc -eq 0) -and $stMatch.Success -and ($stMatch.Groups[1].Value -eq $stMatch.Groups[2].Value)
    Add-Result 'comparator-selftest' $stOk ("rc=$stRc")
    if (-not $stOk) { Write-Output $stOut }

    # ③ 逐项相同 ⇒ 绿（进程内集成，走生产 diff 函数）
    $csGood = Join-Path $tmpRoot 'cs-good.jsonl'
    $rsGood = Join-Path $tmpRoot 'rs-good.jsonl'
    New-HeroPetRaw -Path $csGood
    New-HeroPetRaw -Path $rsGood
    $null = Invoke-HeroPetRawDiff -csharpPath $csGood -rustPath $rsGood
    Add-Result 'identical-green' ($script:HeroPetDiffRc -eq 0) ("rc=$($script:HeroPetDiffRc)")

    # ④ 改坏失败分支 ⇒ 必红
    $rsBranch = Join-Path $tmpRoot 'rs-branch.jsonl'
    New-HeroPetRaw -Path $rsBranch
    $branchLines = @(Get-Content -LiteralPath $rsBranch)
    $branchLines[1] = $branchLines[1] -replace '"branch":"ok"', '"branch":"fail_reject"'
    Write-M3LfLines -Path $rsBranch -Lines $branchLines
    $null = Invoke-HeroPetRawDiff -csharpPath $csGood -rustPath $rsBranch
    Add-Result 'mutate-branch-red' ($script:HeroPetDiffRc -eq 1) ("rc=$($script:HeroPetDiffRc)")

    # ⑤ 少一个假人（bot 集合不一致）⇒ 必红
    $rsOneBot = Join-Path $tmpRoot 'rs-onebot.jsonl'
    Write-M3LfLines -Path $rsOneBot -Lines @((Get-Content -LiteralPath $csGood | Select-Object -First 2))
    $null = Invoke-HeroPetRawDiff -csharpPath $csGood -rustPath $rsOneBot
    Add-Result 'mutate-bot-set-red' ($script:HeroPetDiffRc -eq 1) ("rc=$($script:HeroPetDiffRc)")

    # ⑥ 缺文件 ⇒ PREREQ fail-closed（rc=2）
    $null = Invoke-HeroPetRawDiff -csharpPath (Join-Path $tmpRoot 'nope.jsonl') -rustPath $rsGood
    Add-Result 'missing-file-prereq' ($script:HeroPetDiffRc -eq 2) ("rc=$($script:HeroPetDiffRc)")

    # ⑦ run_pair 的开关耦合：-HeroPetSequence 必须**一并传 -DisableAutoPlay**；默认路径不得出现。
    $emitOnRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -HeroPetSequence -HeroPetSteps 'a|observe|;a|summon|17' -HeroPetStepMs 1000 -HeroPetStartDelayMs 3000 -HeroPetMasterRole 0 -HeroPetPeerRole 1
    if ($LASTEXITCODE -ne 0) { $script:PtFail += 'emit_on_rc=non-zero' }
    else {
      $emitOn = (($emitOnRaw | Out-String).Trim() | ConvertFrom-Json)
      $has = (@($emitOn.bot_args) -contains '-DisableAutoPlay')
      if (-not $has) { $script:PtFail += 'hero_pet_sequence_does_not_pass_disable_autoplay' }
      if ($emitOn.ops_runner -notmatch '-DisableAutoPlay') { $script:PtFail += 'ops_runner_missing_disable_autoplay' }
      if ($emitOn.hero_pet_sequence -notmatch '^on steps=a\|observe\|;a\|summon\|17 step_ms=1000 start_delay_ms=3000 master=0 peer=1 disable_autoplay=True$') { $script:PtFail += ('hero_pet_scope_on=' + $emitOn.hero_pet_sequence) }
      if ($null -eq $emitOn.save_settle_instrument) { $script:PtFail += 'hero_pet_on_missing_save_settle_instrument' }
      Add-Result 'emit-on-coupling' $true ("scope={0}" -f $emitOn.hero_pet_sequence)
    }
    $emitOffRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs
    if ($LASTEXITCODE -ne 0) { $script:PtFail += 'emit_off_rc=non-zero' }
    else {
      $emitOff = (($emitOffRaw | Out-String).Trim() | ConvertFrom-Json)
      $hasOff = (@($emitOff.bot_args) -contains '-DisableAutoPlay')
      if ($hasOff) { $script:PtFail += 'default_path_unexpectedly_passes_disable_autoplay' }
      if ($emitOff.hero_pet_sequence -ne '(off)') { $script:PtFail += ('hero_pet_scope_off=' + $emitOff.hero_pet_sequence) }
      Add-Result 'emit-off-default' $true ("scope={0}" -f $emitOff.hero_pet_sequence)
    }

    # ⑧ peer 口径护栏有牙齿：两侧 steps 不同 ⇒ 必红
    $legacyDir = Join-Path $tmpRoot 'peer'
    New-Item -ItemType Directory -Force -Path $legacyDir | Out-Null
    $peerBaseline = [pscustomobject]@{ hero_pet_sequence = 'on steps=a|observe| step_ms=1000 start_delay_ms=3000 master=0 peer=1 disable_autoplay=True' }
    $peerBaseline | ConvertTo-Json | Set-Content -Path (Join-Path $legacyDir 'baseline.json') -Encoding utf8
    $g = Get-M3PeerHeroPetScopeGuard -PeerDir $legacyDir -LocalScope ('on steps=a|observe|;a|summon|17 step_ms=1000 start_delay_ms=3000 master=0 peer=1 disable_autoplay=True')
    Add-Result 'peer-scope-red' ((@($g.failed) -contains 'peer_hero_pet_scope_mismatch')) ("codes=" + (@($g.failed) -join ','))
    $g2 = Get-M3PeerHeroPetScopeGuard -PeerDir $legacyDir -LocalScope $peerBaseline.hero_pet_sequence
    Add-Result 'peer-scope-green' (@($g2.failed).Count -eq 0) ("codes=" + (@($g2.failed) -join ','))

    # ⑨ 入口护栏 fail-closed：一列空闲端口（无监听）⇒ 必须 PREREQ（exit 2），不能静默 GREEN。
    $portProbeOut = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -Side rust -PortsCsv 17981,17982,17983,17984 -Go 2>&1
    $rcPort = $LASTEXITCODE
    if ($rcPort -ne 2) { $script:PtFail += ("no_stack_port_guard_rc=$rcPort(expect 2)") }
    Add-Result 'no-stack-port-guard' ($rcPort -eq 2) ("rc=$rcPort")

    # ⑩ -DiffOnly 缺原始件 ⇒ PREREQ（exit 2）
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -DiffOnly -CsharpRaw (Join-Path $tmpRoot 'nope.jsonl') -RustRaw $rsGood 2>&1
    $rcDiff = $LASTEXITCODE
    if ($rcDiff -ne 2) { $script:PtFail += ("diffonly_missing_raw_rc=$rcDiff(expect 2)") }
    Add-Result 'diffonly-missing-prereq' ($rcDiff -eq 2) ("rc=$rcDiff")

    # ⑪ -AssembleOnly 缺 markers ⇒ PREREQ（exit 2）
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -AssembleOnly -Markers (Join-Path $tmpRoot 'nope.jsonl') -Out (Join-Path $tmpRoot 'o.jsonl') 2>&1
    $rcAsm = $LASTEXITCODE
    if ($rcAsm -ne 2) { $script:PtFail += ("assembleonly_missing_rc=$rcAsm(expect 2)") }
    Add-Result 'assembleonly-missing-prereq' ($rcAsm -eq 2) ("rc=$rcAsm")

    # ⑫ Rust preflight 有牙齿：空目录 ⇒ false；写了 slave 常量函数名 ⇒ true。
    $tmpSrc = Join-Path $tmpRoot 'hp-probe'
    New-Item -ItemType Directory -Force -Path $tmpSrc | Out-Null
    if (Test-RustHeroPetDispatchImplemented -SrcDirs @($tmpSrc)) { $script:PtFail += 'hp_preflight_empty_dir_true' }
    Set-Content -Path (Join-Path $tmpSrc 'x.rs') -Value 'pub fn make_slave() {}' -Encoding utf8
    if (-not (Test-RustHeroPetDispatchImplemented -SrcDirs @($tmpSrc))) { $script:PtFail += 'hp_preflight_hit_false' }
    Add-Result 'rust-preflight-teeth' (($script:PtFail -notcontains 'hp_preflight_empty_dir_true') -and ($script:PtFail -notcontains 'hp_preflight_hit_false')) 'empty=false hit=true'

    # ⑬ 未给 -Go ⇒ real-run fail-closed（exit 2），**不许**静默起栈/占端口。
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -Side csharp -PortsCsv 17985,17986,17987,17988 2>&1
    $rcNoGo = $LASTEXITCODE
    if ($rcNoGo -ne 2) { $script:PtFail += ("no_go_realrun_rc=$rcNoGo(expect 2)") }
    Add-Result 'no-go-realrun-prereq' ($rcNoGo -eq 2) ("rc=$rcNoGo")

    # ⑭ BotSrv 生产函数 selftest（步解析 / 角色判定 / 帧 id / 分支归类）——改坏必红（由代码 review + 本断言钉住）。
    $botProj = Join-Path $RepoRoot 'src\BotSrv\BotSrv.csproj'
    $botExe = Join-Path $RepoRoot 'src\BotSrv\bin\Release\BotSrv.exe'
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $botBuildOut = (& dotnet build $botProj -c Release --nologo -v q 2>&1 | Out-String)
    $botBuildRc = $LASTEXITCODE
    if ($botBuildRc -ne 0) { $script:PtFail += ('botsrv_build_rc=' + $botBuildRc); Write-Output $botBuildOut }
    else {
      $seqOut = (& $botExe --selftest-heropet 2>&1 | Out-String)
      $seqRc = $LASTEXITCODE
      if ($seqRc -ne 0) { $script:PtFail += ('heropetseq_selftest_rc=' + $seqRc); Write-Output $seqOut }
      if ($seqOut -notmatch 'HEROPETSEQ SELFTEST GREEN') { $script:PtFail += 'heropetseq_selftest_not_green' }
      Add-Result 'botsrv-heropetseq-selftest' ($seqRc -eq 0) ("rc=$seqRc")
    }
    $ErrorActionPreference = $prevEap

    # ⑮ run_bots 序列收尾计数离线自检（heropet kind 有牙齿）
    $rbOut = ''
    $ErrorActionPreference = 'Continue'
    $rbOut = (& powershell -ExecutionPolicy Bypass -File $RunBots -SelfCheckNoStack 2>&1 | Out-String)
    $rbRc = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    Add-Result 'runbots-selftest' ($rbRc -eq 0) ("rc=$rbRc")
    if ($rbRc -ne 0) { Write-Output $rbOut }

    # ⑯ LF 写出（生产者侧根因；EOL 陷阱同族）
    $lf = Test-M3LfWriterSelfTest
    Add-Result 'lf-writer-selftest' ($lf.ok) ($lf.reason)

    # ⑰ 本脚本从未触碰端口（自检/比较全离线）
    Add-Result 'no-port-touched' (-not $script:HeroPetTouchedPorts) 'offline-only'
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:PtPass + $script:PtFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:PtPass, $total, ($script:PtFail -join ','))
  if ($script:PtFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（比较器 fail-closed + 改坏必红 + bot 集合 + 开关耦合两向 + peer 护栏两向 + BotSrv 生产函数 selftest + run_bots 计数 + 入口/DiffOnly/AssembleOnly/preflight/no-Go fail-closed + LF + 零端口）'
  exit 0
}

# =========================================================================== #
# Mode: AssembleOnly —— 只把 BotSrv M3HEROPETSEQ 事件流合并成 hero_pet_raw.jsonl
# =========================================================================== #
if ($AssembleOnly) {
  if ([string]::IsNullOrEmpty($Markers)) { Write-Output 'PREREQ_MISSING: -AssembleOnly 需要 -Markers <file>'; exit 2 }
  if (-not (Test-Path -LiteralPath $Markers)) { Write-Output ("PREREQ_MISSING: 找不到 markers {0}" -f $Markers); exit 2 }
  $argv = @('-I', $PairDiff, 'assemble', $Markers)
  if (-not [string]::IsNullOrEmpty($Out)) { $argv += @('--out', $Out) }
  $argv += @('--side', $Side)
  & python @argv
  exit $LASTEXITCODE
}

# =========================================================================== #
# Mode: DiffOnly —— 只重比既有产物（不碰端口/库）
# =========================================================================== #
if ($DiffOnly) {
  $csPath = $CsharpRaw
  $rsPath = $RustRaw
  if ([string]::IsNullOrEmpty($csPath) -and -not [string]::IsNullOrEmpty($CsharpSession)) {
    $csPath = Join-Path (Join-Path $Sessions $CsharpSession) 'hero_pet_raw.jsonl'
  }
  if ([string]::IsNullOrEmpty($rsPath) -and -not [string]::IsNullOrEmpty($RustSession)) {
    $rsPath = Join-Path (Join-Path $Sessions $RustSession) 'hero_pet_raw.jsonl'
  }
  if ([string]::IsNullOrEmpty($csPath) -or [string]::IsNullOrEmpty($rsPath)) {
    Write-Output 'PREREQ_MISSING: -DiffOnly 需要 -CsharpRaw/-RustRaw（或 -CsharpSession/-RustSession）'
    exit 2
  }
  Invoke-HeroPetRawDiff -csharpPath $csPath -rustPath $rsPath
  $rcRaw = $script:HeroPetDiffRc
  if ($rcRaw -ne 0) { Write-Output ("RED: hero_pet_diff 未通过（rc={0}）" -f $rcRaw); exit $rcRaw }
  if (-not [string]::IsNullOrEmpty($CsharpSession) -and -not [string]::IsNullOrEmpty($RustSession)) {
    Invoke-HeroPetScopedDiff -csharpDir (Join-Path $Sessions $CsharpSession) -rustDir (Join-Path $Sessions $RustSession)
    if ($script:HeroPetScopedRc -ne 0) { Write-Output ("RED: DB 作用域 diff 未通过（rc={0}）" -f $script:HeroPetScopedRc); exit $script:HeroPetScopedRc }
  }
  Write-Output 'GREEN: 英雄/宠物跨侧 diff = 0'
  exit 0
}

# =========================================================================== #
# Mode: real run（真机两侧同序列）—— 未获 owner 窗口（-Go）前 fail-closed 拒跑
# =========================================================================== #
if (-not $Go) {
  Write-Output 'PREREQ_MISSING: 真机两侧同序列需 owner 发放窗口（起栈禁接管道；-Go 显式授权）'
  Write-Output ("NEXT: 获窗口后：`n  powershell -File run_hero_pet_pair.ps1 -Side csharp -HeroPetCase {0} -Go`n  powershell -File run_hero_pet_pair.ps1 -Side rust -HeroPetCase {0} -PeerSession <csharp 会话> -Go" -f $HeroPetCase)
  exit 2
}

$script:Scenario = Get-HeroPetScenario
Ensure-HeroPetDefaults
$HeroPetSteps = $script:HeroPetStepsArg
$caseDef = $script:Scenario.cases.$HeroPetCase
if ($Bots -ne 2) {
  Write-Output ("RED: 本夹具是『主人 + peer』两假人场景，-Bots 必须 = 2（收到 {0}）" -f $Bots)
  exit 1
}

if ([string]::IsNullOrEmpty($Prefix)) {
  $Prefix = 'm3hp' + (Get-Date -Format 'HHmmss')
}

Write-Output '--- 0) 入口实现识别护栏 ---'
$holders = Get-EntryHolders
foreach ($h in $holders) { Write-Output ("ENTRY {0} <- {1} [{2}] {3}" -f $h.port, $h.process, $h.impl, $h.path) }
$impls = @($holders | ForEach-Object { $_.impl } | Sort-Object -Unique)
if ($impls -contains 'none') {
  Write-Output 'PREREQ_MISSING: 入口端口不全在听（5000/7000/7100/7200）——先起全栈；本脚本不起服务'
  exit 2
}
if ($Side -eq 'rust' -and ($impls -ne @('rust-or-other'))) {
  Write-Output 'RED: -Side rust 但入口仍是 C#（E:\MirServer）——这是拿 C# 跟 C# 比，门禁恒绿'
  exit 1
}
if ($Side -eq 'csharp' -and ($impls -ne @('csharp'))) {
  Write-Output 'RED: -Side csharp 但入口不是 E:\MirServer 的 C# 实现'
  exit 1
}
if ($impls -contains 'csharp' -and $impls -contains 'rust-or-other') {
  Write-Output 'RED: 入口端口被两种实现混占（部分 C#、部分非 C#）——先统一'
  exit 1
}

# --- 0.5) preflight（T-27 同族）：Rust 侧宝宝召唤/生命周期是否已接线 ---
Write-Output '--- 0.5) preflight：Rust 宝宝召唤/生命周期（T-27 同族；⑨-2/⑨-3） ---'
if ($Side -eq 'rust' -and -not $SkipRustPreflight) {
  if (-not (Test-RustHeroPetDispatchImplemented)) {
    Write-Output 'PREREQ_MISSING: Rust crates/world / bins/game-svr 未见宝宝召唤/生命周期（make_slave/slave_list/check_royalty）——跑下去 Rust 侧召唤必失败、跨侧 diff 必红（承接卡 openmir2-m3-pet-slave-summon / -pet-slave-lifecycle；-SkipRustPreflight 仅调试用）'
    exit 2
  }
  Write-Output 'PREFLIGHT Rust 宝宝召唤/生命周期：已接线（命中 slave 标记）'
} else {
  Write-Output 'PREFLIGHT Rust 宝宝召唤/生命周期：跳过（-Side csharp 或 -SkipRustPreflight）'
}

Write-Output '--- 1) 预检：MySQL ---'
if (-not (Test-MysqlTcp)) {
  Write-Output 'PREREQ_MISSING: MySQL 127.0.0.1:3306 不可达——dbsnap 取不到快照'
  exit 2
}
Write-Output 'PREFLIGHT MySQL 在听'

Write-Output '--- 2) 侧运行（复用 run_pair.ps1；两相 pin + 两假人相邻 + 固定召唤序列） ---'
$pairArgs = @('-Side', $Side, '-Bots', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port,
              '-TimeoutSec', $TimeoutSec, '-FixedNewChr', '-PinStartPose', '-AttackPinPair',
              '-HeroPetSequence', '-HeroPetSteps', $HeroPetSteps,
              '-HeroPetStepMs', $HeroPetStepMs, '-HeroPetStartDelayMs', $HeroPetStartDelayMs,
              '-HeroPetMasterRole', $HeroPetMasterRole, '-HeroPetPeerRole', $HeroPetPeerRole,
              '-PinMapName', [string]$caseDef.map,
              '-PinHomeMap', [int]$caseDef.home_map, '-PinHomeX', [int]$caseDef.home_x, '-PinHomeY', [int]$caseDef.home_y,
              '-PinCX', [int]$caseDef.player_x, '-PinCY', [int]$caseDef.player_y, '-PinDir', [int]$caseDef.player_dir,
              '-AttackPinDx', [int]$caseDef.peer_dx,
              '-AttackVictimLevel', [int]$caseDef.pin_level)
& powershell -ExecutionPolicy Bypass -File $RunPair @pairArgs
$pairExit = $LASTEXITCODE
if ($pairExit -ne 0) {
  Write-Output ("RED: run_pair（{0} 侧）未通过（rc={1}）——同序列/快照/抽取未成立" -f $Side, $pairExit)
  exit 1
}

$mine = Newest-Session $Side
if ($null -eq $mine) {
  Write-Output ("RED: 找不到 {0} 侧新会话目录（缺 baseline.json）" -f $Side)
  exit 1
}
Write-Output ("SESSION_{0} {1}" -f $Side.ToUpper(), $mine.Name)

$baseline = Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json
$markers = @($baseline.hero_pet_markers)
$hasDone = @($markers | Where-Object { $_ -match '"phase"\s*:\s*"done"' }).Count -ge 2
if ($markers.Count -eq 0 -or -not $hasDone) {
  Write-Output ("RED: 同序列判据不成立（hero_pet_markers={0} 条，done>=2={1}）——驱动没跑完整串/没两个假人" -f $markers.Count, $hasDone)
  exit 1
}
Write-Output ("HEROPETSEQ_OK markers={0} done>=2=True" -f $markers.Count)

$rawMine = Join-Path $mine.FullName 'hero_pet_raw.jsonl'
if (-not (Test-Path -LiteralPath $rawMine)) {
  Write-Output ("PREREQ_MISSING: 本侧会话缺 hero_pet_raw.jsonl（run_pair assemble 未产出）-> {0}" -f $rawMine)
  exit 2
}

if ($Side -eq 'rust' -and -not [string]::IsNullOrEmpty($PeerSession)) {
  $peer = Join-Path $Sessions $PeerSession
  if (-not (Test-Path $peer)) { Write-Output ("PREREQ_MISSING: 找不到 C# 对照会话 {0}" -f $peer); exit 2 }
  $localScope = Get-M3SessionHeroPetScope -Dir $mine.FullName
  if ($null -eq $localScope) { Write-Output ("PREREQ_MISSING: 本侧会话 baseline.json 缺/坏，读不出英雄/宠物序列开关: {0}" -f $mine.FullName); exit 2 }
  $scopeGuard = Get-M3PeerHeroPetScopeGuard -PeerDir $peer -LocalScope $localScope
  foreach ($m in $scopeGuard.messages) { Write-Output $m }
  if (@($scopeGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧英雄/宠物序列开关不同口径（{0}）——先按同一组开关重跑基线" -f (@($scopeGuard.failed) -join ','))
    exit 2
  }
  Write-Output ("HEROPETSCOPE_OK peer='{0}' local='{1}'" -f $scopeGuard.peer_scope, $localScope)
  Invoke-HeroPetRawDiff -csharpPath (Join-Path $peer 'hero_pet_raw.jsonl') -rustPath $rawMine
  $rcRaw = $script:HeroPetDiffRc
  Invoke-HeroPetScopedDiff -csharpDir $peer -rustDir $mine.FullName
  $rcScoped = $script:HeroPetScopedRc
  if ($rcRaw -eq 2 -or $rcScoped -eq 2) { exit 2 }
  if ($rcRaw -ne 0 -or $rcScoped -ne 0) {
    Write-Output ("RED: 跨侧判据未全绿（raw={0} scoped={1}）" -f $rcRaw, $rcScoped)
    exit 1
  }
  Write-Output 'GREEN: 两侧同序列 —— 英雄/宠物逐 (bot,k) diff = 0 且 DB 作用域字段级 diff = 0（宝宝态不落库口径）'
  exit 0
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_hero_pet_pair.ps1 -Side rust -HeroPetCase {0} -PeerSession {1} -Go" -f $HeroPetCase, $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与同序列判据通过'
exit 0
