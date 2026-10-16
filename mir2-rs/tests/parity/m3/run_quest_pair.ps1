#requires -Version 5
<#
.SYNOPSIS
  run_quest_pair.ps1 — M3⑥「两侧同序列」任务对拍的一条命令（卡 `openmir2-m3-quest-fixture-botsrv`）。

.DESCRIPTION
  目标（父卡 `openmir2-m3-quest-align` 的 DoD②③）：把**同一串任务动作**（点任务 NPC → 选 `@rw06` → `@06rw`
  → 再 `@rw06` 条件判定，两侧同一串 `CM_*` 字节）分别打在 C# 栈与 Rust 栈上 ⇒
  ① `quest_diff.py diff` 作用域内逐 step 字段级 diff = 0（帧种类 + `SM_MERCHANTSAY(643)` 说词字节 +
     **会话任务态位** QuestFlag/QuestUnit/QuestUnitOpen）；
  ② `scoped_diff.py diff --canonical --fixed-newchr` 在 DB 侧（`characters` + `characters_quest` 均 0 行）
     diff = 0（= oracle「任务态不落库」口径，见盘点报告 §1.F）。

  本脚本**不新造编排**：一次「侧运行」复用 `run_pair.ps1`（两相固定起点 `-PinStartPose` + 预热相
  `-StopAfterCharCreated` + BEFORE/AFTER 快照 + 收尾「等断线存档落库」settle 仪器），本脚本只加：
    ① 入口实现识别护栏（-Side 与 5000/7000/7100/7200 的实际监听进程必须对应）；
    ② `-QuestSequence` 步骤串参数透传 + `M3QUESTSEQ` 标记判据（序列没跑完整 ⇒ RED，绝不当 GREEN）；
    ③ 跨侧 `quest_diff.py`（任务记录）+ `scoped_diff.py`（DB 字段级）两道判据；
    ④ `-SelfCheckNoStack` 无栈自检（比较器 selftest + 场景锚门 check/selftest/mutation + 合成件正/负样本 + BotSrv 生产函数 selftest +
       入口/DiffOnly/AssembleOnly/preflight fail-closed + 零端口断言）。

  **任务态位**：BotSrv 客户端看不到任务数组（盘点报告 §4.1 第 2 条）⇒ 记录里的 `quest_flag_*` 由
  **服务端会话 dump**（`-State <file>`，窗口期由 ⑥-3/⑥-4 面提供）补入；缺它 ⇒ `quest_diff.py diff`
  一律 PREREQ（fail-closed），绝不静默 GREEN。

  真机跑要窗口（起栈禁接管道）：未给 `-Go` 时 real-run **fail-closed 拒跑（exit 2）**，不占端口。
  前置（T-27 同族口径）：Rust 侧 ⑥-3（脚本引擎接进 `bins/game-svr`：NPC 运行时 / `@Login` / 说词帧）
  落地前 `-Side rust` 机判 preflight 直接 PREREQ（exit 2），避免白占窗口。

  Mode：
    -Side csharp|rust [可选参数]     跑一侧（复用 run_pair.ps1）。
    -Side rust -PeerSession <C#会话> 跑 Rust 侧并跨侧 diff（= 本卡的正式判据）。
    -DiffOnly -CsharpRaw <f> -RustRaw <f>  只重比既有原始件（**不碰端口**）。
    -AssembleOnly -Markers <f> [-State <f>] -Out <f>  只把 BotSrv 的 `M3QUESTSEQ` 事件流（+服务端 dump）合并成记录。
    -SelfCheckNoStack                无栈自检：所有路径都必须 fail-closed。

  窗口纪律：除 -DiffOnly/-AssembleOnly/-SelfCheckNoStack 外都会连真栈；起两侧栈前先看
  `E:\MirServer\port-status.ps1`，与 M2/M3 其它窗口严格串行（**别并行起栈**）。
  本脚本**不启动/不停止任何服务**——栈的起停仍由既有脚本/人工负责。

.EXAMPLE
  # ① C# 基线（C# 栈在听；单假人 + 两相 pin 到任务 NPC 旁 + 固定任务序列）
  powershell -ExecutionPolicy Bypass -File run_quest_pair.ps1 -Side csharp -QuestCase quest_accept -Go
  # ② 换成 Rust 栈后跑 Rust 侧并与上面的 C# 会话对拍
  powershell -ExecutionPolicy Bypass -File run_quest_pair.ps1 -Side rust -QuestCase quest_accept -PeerSession csharp-... -Go
  # ③ 只重比既有产物（不碰端口）
  powershell -ExecutionPolicy Bypass -File run_quest_pair.ps1 -DiffOnly -CsharpRaw <f> -RustRaw <f>
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_quest_pair.ps1 -SelfCheckNoStack
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 1,
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
  [ValidateSet('quest_accept', 'quest_main_only')][string]$QuestCase = 'quest_accept',
  [string]$QuestSteps = '',        # 空 ⇒ 由场景 case.steps 拼成 `op|npc|label;...`
  [int]$QuestStepMs = 1000,
  [int]$QuestStartDelayMs = 3000,
  [string]$State = '',             # 服务端会话任务态 dump（quest_state.jsonl）；空 ⇒ 会话目录默认名
  [string]$PortsCsv = '',          # 入口端口（逗号分隔）；默认 5000,7000,7100,7200
  [switch]$DiffOnly,
  [switch]$AssembleOnly,
  [string]$Markers = '',
  [string]$Out = '',
  [switch]$Go,                     # owner 发放窗口的显式授权；未给 ⇒ real-run fail-closed（exit 2）
  [switch]$SkipRustPreflight,      # 仅供调试：跳过「Rust 侧脚本运行时未接线」前置（默认不跳）
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
$QuestDiff = Join-Path $Here 'quest_diff.py'
$RunBots = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$Sessions = Join-Path $Here 'sessions'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
if ([string]::IsNullOrEmpty($ScenarioFile)) { $ScenarioFile = Join-Path $Here 'quest_scenario.json' }
. (Join-Path $Here 'gate_guards.ps1')
. (Join-Path $Here 'm3hashutil.ps1')

# 脚本作用域：本脚本（含自检）**从不**绑定/连接任何端口；real-run 由外部栈 + run_pair 负责。
$script:QuestTouchedPorts = $false

# ---- 场景固定输入（case.steps → `op|npc|label;...`） -------------------------
function Get-QuestScenario {
  if (-not (Test-Path -LiteralPath $ScenarioFile)) { throw ("找不到场景文件: {0}" -f $ScenarioFile) }
  return (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)
}
$script:Scenario = $null
$script:QuestStepsArg = $QuestSteps
function Ensure-QuestDefaults {
  if ($null -eq $script:Scenario) { return }
  $case = $script:Scenario.cases.$QuestCase
  if ($null -eq $case) { throw ("场景 {0} 里没有 case '{1}'" -f $ScenarioFile, $QuestCase) }
  if ([string]::IsNullOrEmpty($script:QuestStepsArg)) {
    $script:QuestStepsArg = (@($case.steps | ForEach-Object { "{0}|{1}|{2}" -f $_.op, $_.npc, $_.label }) -join ';')
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

# preflight（T-27 同族口径）：⑥-3 落地前 `bins/game-svr` 不接脚本引擎（不派发任务 `CM_*`）
# ⇒ 跑下去 Rust 侧 after 快照仍是建角状态、跨侧 diff 必红。机判：game-svr 源码出现任一任务常量/脚本依赖。
# 承接卡 openmir2-m3-quest-dialog-wiring / -quest-state / -quest-persist-observability。
function Test-RustQuestDispatchImplemented {
  param([string]$SrcDir = '')
  if ([string]::IsNullOrEmpty($SrcDir)) { $SrcDir = Join-Path $RepoRoot 'mir2-rs\bins\game-svr\src' }
  if (-not (Test-Path $SrcDir)) { return $false }
  $hits = @(Get-ChildItem $SrcDir -Recurse -Filter '*.rs' -ErrorAction SilentlyContinue |
            Select-String -Pattern 'CM_CLICKNPC|MERCHANTDLGSELECT|SM_MERCHANTSAY|mir2_script|mir2-script' -ErrorAction SilentlyContinue)
  return ($hits.Count -gt 0)
}

# CrossSideDiff 的退出码（脚本作用域）：不能把带 Write-Output 的函数直接写进 `exit (...)`。
$script:QuestDiffRc = 2
function Invoke-QuestRawDiff([string]$csharpPath, [string]$rustPath) {
  $script:QuestDiffRc = 2
  if (-not (Test-Path -LiteralPath $csharpPath)) { Write-Output ("PREREQ_MISSING: C# 原始件缺失 -> {0}" -f $csharpPath); return }
  if (-not (Test-Path -LiteralPath $rustPath)) { Write-Output ("PREREQ_MISSING: Rust 原始件缺失 -> {0}" -f $rustPath); return }
  Write-Output '--- quest_diff.py diff（任务逐 step 字段级 + 说词 + 任务态位） ---'
  & python -I $QuestDiff diff $csharpPath $rustPath
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: 任务跨侧逐 step diff = 0'; $script:QuestDiffRc = 0; return }
  if ($LASTEXITCODE -eq 2) { Write-Output 'PREREQ: quest_diff 判缺件/坏件（含缺任务态 dump）—— 不得当 GREEN'; $script:QuestDiffRc = 2; return }
  Write-Output 'RED: 任务跨侧 diff != 0（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）'
  $script:QuestDiffRc = 1
}

$script:QuestScopedRc = 2
function Invoke-QuestScopedDiff([string]$csharpDir, [string]$rustDir) {
  $script:QuestScopedRc = 2
  $a = Join-Path $csharpDir 'scoped_after_norm.jsonl'
  $b = Join-Path $rustDir 'scoped_after_norm.jsonl'
  if (-not (Test-Path -LiteralPath $a)) { Write-Output ("PREREQ_MISSING: C# 会话缺 scoped_after_norm.jsonl -> {0}" -f $a); return }
  if (-not (Test-Path -LiteralPath $b)) { Write-Output ("PREREQ_MISSING: Rust 会话缺 scoped_after_norm.jsonl -> {0}" -f $b); return }
  Write-Output '--- 跨侧作用域 DB 字段级 diff（scoped_diff --canonical --fixed-newchr；含 characters_quest 两侧 0 行） ---'
  & python -I $Scoped diff $a $b --canonical --fixed-newchr
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: 跨侧作用域内 DB 字段级 diff = 0（任务态不落库口径）'; $script:QuestScopedRc = 0; return }
  Write-Output 'RED: 跨侧 DB diff != 0'
  $script:QuestScopedRc = 1
}

# =========================================================================== #
# Mode: SelfCheckOut —— 把无栈自检转录**由脚本自己**以 LF 落盘
# =========================================================================== #
if ($SelfCheckNoStack -and -not [string]::IsNullOrEmpty($SelfCheckOut)) {
  $selfTmp = Join-Path $env:TEMP ('quest-selfcheck-' + [guid]::NewGuid().ToString('N') + '.txt')
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
  $script:QuestPass = 0
  $script:QuestFail = @()
  $tmpRoot = Join-Path $env:TEMP ('quest-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  function Add-Result([string]$Name, [bool]$Ok, [string]$Detail) {
    $suffix = if ([string]::IsNullOrEmpty($Detail)) { '' } else { " ($Detail)" }
    if ($Ok) {
      $script:QuestPass++
      Write-Output ("SELFTEST ok {0}{1}" -f $Name, $suffix)
    } else {
      $script:QuestFail += $Name
      Write-Output ("SELFTEST FAIL {0}{1}" -f $Name, $suffix)
    }
  }

  function New-QuestRaw([string]$Path, [string]$Branch, [int]$FrameSm, [string]$Text, [string]$FlagAfter) {
    $textEsc = $Text.Replace('\', '\\').Replace('"', '\"')
    $rec = '{"k":0,"op":"dialog_select","npc":"任务管理","npc_actor":101,"map":"3","label":"@rw06","req":{"item":"","n":0},"frames":[{"sm":' + $FrameSm + ',"text":"' + $textEsc + '"}],"gold_before":1000,"gold_after":1000,"bag_before":["木剑#43"],"bag_after":["木剑#43"],"quest_flag_before":"' + ('00' * 128) + '","quest_flag_after":"' + $FlagAfter + '","branch":"' + $Branch + '"}'
    Write-M3LfLines -Path $Path -Lines @($rec)
  }

  try {
    # ① 场景固定输入（case 表 + 默认 case + steps 可拼）
    $scOk = $false
    try {
      $sc = Get-QuestScenario
      $case = $sc.cases.$($sc.default_case)
      $scOk = ($sc.canon -eq 'm3-quest-v1') -and ($null -ne $case) -and
              (-not [string]::IsNullOrEmpty($case.npc)) -and (@($case.steps).Count -ge 1) -and
              ($null -ne $sc.cases.quest_main_only)
    } catch { $scOk = $false }
    Add-Result 'scenario-fixed-input' $scOk 'canon/cases/quest_accept/quest_main_only'

    # ② 比较器 selftest（fail-closed + 改坏必红）必须 GREEN
    $stOut = ''
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $stOut = (& python -I $QuestDiff selftest 2>&1 | Out-String)
    $stRc = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    $stMatch = [regex]::Match($stOut, 'SELFTEST ok=(\d+)/(\d+)')
    $stOk = ($stRc -eq 0) -and $stMatch.Success -and ($stMatch.Groups[1].Value -eq $stMatch.Groups[2].Value)
    Add-Result 'comparator-selftest' $stOk ("rc=$stRc")
    if (-not $stOk) { Write-Output $stOut }
    # ②″ 任务态生产者（构造级，T-58）：selftest 必须 GREEN；emit 出 construct_level:true。
    $prod = Join-Path $Here 'quest_state_producer.py'
    $prodOut = ''
    $ErrorActionPreference = 'Continue'
    $prodOut = (& python -I $prod selftest 2>&1 | Out-String)
    $prodRc = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    $prodOk = ($prodRc -eq 0) -and ($prodOut -match 'SELFTEST ok=')
    Add-Result 'producer-selftest' $prodOk ("rc=$prodRc")
    if (-not $prodOk) { Write-Output $prodOut }

    $prodState = Join-Path $tmpRoot 'quest_state_construct.jsonl'
    $prodEmit = ''
    $ErrorActionPreference = 'Continue'
    $prodEmit = (& python -I $prod emit --scenario (Join-Path $Here 'quest_scenario.json') --case quest_accept --out $prodState 2>&1 | Out-String)
    $prodEmitRc = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    $prodEmitOk = ($prodEmitRc -eq 0) -and ($prodEmit -match 'construct_level=true') -and (Test-Path $prodState)
    Add-Result 'producer-emit-construct' $prodEmitOk ("rc=$prodEmitRc")


    # ②′ 场景锚门（quest_anchor_gate.py）：oracle_anchors 覆盖面 + 每条冻结 oracle 锚的
    # 行点对冻结面（⑥ 场景锚此前无人行级复核；oracle「任务态不落库」的缺陷本体 token 一并锁死）。
    $AnchorGate = Join-Path $Here 'quest_anchor_gate.py'
    $ErrorActionPreference = 'Continue'
    $agOut = (& python -I $AnchorGate check 2>&1 | Out-String)
    $agRc = $LASTEXITCODE
    $agOk = ($agRc -eq 0) -and ($agOut -match '(?m)^GREEN:')
    Add-Result 'anchor-gate-check' $agOk ("rc=$agRc")
    if (-not $agOk) { Write-Output $agOut }
    $agsOut = (& python -I $AnchorGate selftest 2>&1 | Out-String)
    $agsRc = $LASTEXITCODE
    $agsOk = ($agsRc -eq 0) -and ($agsOut -match '(?m)^SELFTEST GREEN')
    Add-Result 'anchor-gate-selftest' $agsOk ("rc=$agsRc")
    if (-not $agsOk) { Write-Output $agsOut }
    $agmOut = (& python -I $AnchorGate mutation 2>&1 | Out-String)
    $agmRc = $LASTEXITCODE
    $agmOk = ($agmRc -eq 0) -and ($agmOut -match '(?m)^MUTATION GREEN')
    Add-Result 'anchor-gate-mutation-red' $agmOk ("rc=$agmRc")
    if (-not $agmOk) { Write-Output $agmOut }
    $ErrorActionPreference = $prevEap

    # ③ 逐项相同 ⇒ 绿（进程内集成，走生产 diff 函数）
    $csGood = Join-Path $tmpRoot 'cs-good.jsonl'
    $rsGood = Join-Path $tmpRoot 'rs-good.jsonl'
    New-QuestRaw -Path $csGood -Branch 'ok_say' -FrameSm 643 -Text '任务管理/确定开始' -FlagAfter ('00' * 128)
    New-QuestRaw -Path $rsGood -Branch 'ok_say' -FrameSm 643 -Text '任务管理/确定开始' -FlagAfter ('00' * 128)
    $null = Invoke-QuestRawDiff -csharpPath $csGood -rustPath $rsGood
    Add-Result 'identical-green' ($script:QuestDiffRc -eq 0) ("rc=$($script:QuestDiffRc)")

    # ④ 改坏失败分支/说词 ⇒ 必红
    $rsBranch = Join-Path $tmpRoot 'rs-branch.jsonl'
    New-QuestRaw -Path $rsBranch -Branch 'ok_messagebox' -FrameSm 767 -Text '苍月岛的首饰店老板正在找人帮忙' -FlagAfter ('00' * 128)
    $null = Invoke-QuestRawDiff -csharpPath $csGood -rustPath $rsBranch
    Add-Result 'mutate-branch-red' ($script:QuestDiffRc -eq 1) ("rc=$($script:QuestDiffRc)")

    $rsText = Join-Path $tmpRoot 'rs-text.jsonl'
    New-QuestRaw -Path $rsText -Branch 'ok_say' -FrameSm 643 -Text '任务管理/确定开始 ' -FlagAfter ('00' * 128)
    $null = Invoke-QuestRawDiff -csharpPath $csGood -rustPath $rsText
    Add-Result 'mutate-say-text-red' ($script:QuestDiffRc -eq 1) ("rc=$($script:QuestDiffRc)")

    # ⑤ 改任务态位（set [107] 1 ⇒ idx13 bit32）⇒ 必红
    $rsFlag = Join-Path $tmpRoot 'rs-flag.jsonl'
    New-QuestRaw -Path $rsFlag -Branch 'ok_say' -FrameSm 643 -Text '任务管理/确定开始' -FlagAfter (('00' * 13) + '20' + ('00' * 114))
    $null = Invoke-QuestRawDiff -csharpPath $csGood -rustPath $rsFlag
    Add-Result 'mutate-quest-flag-red' ($script:QuestDiffRc -eq 1) ("rc=$($script:QuestDiffRc)")

    # ⑥ 缺文件 ⇒ PREREQ fail-closed（rc=2）
    $null = Invoke-QuestRawDiff -csharpPath (Join-Path $tmpRoot 'nope.jsonl') -rustPath $rsGood
    Add-Result 'missing-file-prereq' ($script:QuestDiffRc -eq 2) ("rc=$($script:QuestDiffRc)")

    # ⑦ run_pair 的开关耦合：-QuestSequence 必须**一并传 -DisableAutoPlay**；默认路径不得出现。
    $emitOnRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -QuestSequence -QuestSteps 'click_npc|任务管理|@main;dialog_select|任务管理|@rw06' -QuestStepMs 1000 -QuestStartDelayMs 3000
    if ($LASTEXITCODE -ne 0) { $script:QuestFail += 'emit_on_rc=non-zero' }
    else {
      $emitOn = (($emitOnRaw | Out-String).Trim() | ConvertFrom-Json)
      $has = (@($emitOn.bot_args) -contains '-DisableAutoPlay')
      if (-not $has) { $script:QuestFail += 'quest_sequence_does_not_pass_disable_autoplay' }
      if ($emitOn.ops_runner -notmatch '-DisableAutoPlay') { $script:QuestFail += 'ops_runner_missing_disable_autoplay' }
      if ($emitOn.quest_sequence -notmatch '^on steps=click_npc\|任务管理\|@main;dialog_select\|任务管理\|@rw06 step_ms=1000 start_delay_ms=3000 disable_autoplay=True$') { $script:QuestFail += ('quest_scope_on=' + $emitOn.quest_sequence) }
      if ($null -eq $emitOn.save_settle_instrument) { $script:QuestFail += 'quest_on_missing_save_settle_instrument' }
      Add-Result 'emit-on-coupling' $true ("scope={0}" -f $emitOn.quest_sequence)
    }
    $emitOffRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs
    if ($LASTEXITCODE -ne 0) { $script:QuestFail += 'emit_off_rc=non-zero' }
    else {
      $emitOff = (($emitOffRaw | Out-String).Trim() | ConvertFrom-Json)
      $hasOff = (@($emitOff.bot_args) -contains '-DisableAutoPlay')
      if ($hasOff) { $script:QuestFail += 'default_path_unexpectedly_passes_disable_autoplay' }
      if ($emitOff.quest_sequence -ne '(off)') { $script:QuestFail += ('quest_scope_off=' + $emitOff.quest_sequence) }
      Add-Result 'emit-off-default' $true ("scope={0}" -f $emitOff.quest_sequence)
    }

    # ⑧ peer 口径护栏有牙齿：两侧 steps 不同 ⇒ 必红
    $legacyDir = Join-Path $tmpRoot 'peer'
    New-Item -ItemType Directory -Force -Path $legacyDir | Out-Null
    $peerBaseline = [pscustomobject]@{ quest_sequence = 'on steps=click_npc|任务管理|@main step_ms=1000 start_delay_ms=3000 disable_autoplay=True' }
    $peerBaseline | ConvertTo-Json | Set-Content -Path (Join-Path $legacyDir 'baseline.json') -Encoding utf8
    $g = Get-M3PeerQuestScopeGuard -PeerDir $legacyDir -LocalScope ('on steps=click_npc|任务管理|@main;dialog_select|任务管理|@rw06 step_ms=1000 start_delay_ms=3000 disable_autoplay=True')
    Add-Result 'peer-scope-red' ((@($g.failed) -contains 'peer_quest_scope_mismatch')) ("codes=" + (@($g.failed) -join ','))
    $g2 = Get-M3PeerQuestScopeGuard -PeerDir $legacyDir -LocalScope $peerBaseline.quest_sequence
    Add-Result 'peer-scope-green' (@($g2.failed).Count -eq 0) ("codes=" + (@($g2.failed) -join ','))

    # ⑨ 入口护栏 fail-closed：一列空闲端口（无监听）⇒ 必须 PREREQ（exit 2），不能静默 GREEN。
    $portProbeOut = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -Side rust -PortsCsv 17991,17992,17993,17994 -Go 2>&1
    $rcPort = $LASTEXITCODE
    if ($rcPort -ne 2) { $script:QuestFail += ("no_stack_port_guard_rc=$rcPort(expect 2)") }
    Add-Result 'no-stack-port-guard' ($rcPort -eq 2) ("rc=$rcPort")

    # ⑩ -DiffOnly 缺原始件 ⇒ PREREQ（exit 2）
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -DiffOnly -CsharpRaw (Join-Path $tmpRoot 'nope.jsonl') -RustRaw $rsGood 2>&1
    $rcDiff = $LASTEXITCODE
    if ($rcDiff -ne 2) { $script:QuestFail += ("diffonly_missing_raw_rc=$rcDiff(expect 2)") }
    Add-Result 'diffonly-missing-prereq' ($rcDiff -eq 2) ("rc=$rcDiff")

    # ⑪ -AssembleOnly 缺 markers ⇒ PREREQ（exit 2）
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -AssembleOnly -Markers (Join-Path $tmpRoot 'nope.jsonl') -Out (Join-Path $tmpRoot 'o.jsonl') 2>&1
    $rcAsm = $LASTEXITCODE
    if ($rcAsm -ne 2) { $script:QuestFail += ("assembleonly_missing_rc=$rcAsm(expect 2)") }
    Add-Result 'assembleonly-missing-prereq' ($rcAsm -eq 2) ("rc=$rcAsm")

    # ⑫ Rust preflight 有牙齿：空目录 ⇒ false；写了任务/脚本常量 ⇒ true。
    $tmpSrc = Join-Path $tmpRoot 'quest-probe'
    New-Item -ItemType Directory -Force -Path $tmpSrc | Out-Null
    if (Test-RustQuestDispatchImplemented -SrcDir $tmpSrc) { $script:QuestFail += 'quest_preflight_empty_dir_true' }
    Set-Content -Path (Join-Path $tmpSrc 'x.rs') -Value 'pub const CM_CLICKNPC: u16 = 1010;' -Encoding utf8
    if (-not (Test-RustQuestDispatchImplemented -SrcDir $tmpSrc)) { $script:QuestFail += 'quest_preflight_hit_false' }
    Add-Result 'rust-preflight-teeth' (($script:QuestFail -notcontains 'quest_preflight_empty_dir_true') -and ($script:QuestFail -notcontains 'quest_preflight_hit_false')) 'empty=false hit=true'

    # ⑬ 未给 -Go ⇒ real-run fail-closed（exit 2），**不许**静默起栈/占端口。
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -Side csharp -PortsCsv 17995,17996,17997,17998 2>&1
    $rcNoGo = $LASTEXITCODE
    if ($rcNoGo -ne 2) { $script:QuestFail += ("no_go_realrun_rc=$rcNoGo(expect 2)") }
    Add-Result 'no-go-realrun-prereq' ($rcNoGo -eq 2) ("rc=$rcNoGo")

    # ⑭ BotSrv 生产函数 selftest（步解析 / 分支归类 / 说词抽取）——改坏必红（red-on-mutation 由代码 review + 本断言钉住）。
    $botProj = Join-Path $RepoRoot 'src\BotSrv\BotSrv.csproj'
    $botExe = Join-Path $RepoRoot 'src\BotSrv\bin\Release\BotSrv.exe'
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $botBuildOut = (& dotnet build $botProj -c Release --nologo -v q 2>&1 | Out-String)
    $botBuildRc = $LASTEXITCODE
    if ($botBuildRc -ne 0) { $script:QuestFail += ('botsrv_build_rc=' + $botBuildRc); Write-Output $botBuildOut }
    else {
      $seqOut = (& $botExe --selftest-questseq 2>&1 | Out-String)
      $seqRc = $LASTEXITCODE
      if ($seqRc -ne 0) { $script:QuestFail += ('questseq_selftest_rc=' + $seqRc); Write-Output $seqOut }
      if ($seqOut -notmatch 'QUESTSEQ SELFTEST GREEN') { $script:QuestFail += 'questseq_selftest_not_green' }
      Add-Result 'botsrv-questseq-selftest' ($seqRc -eq 0) ("rc=$seqRc")
    }
    $ErrorActionPreference = $prevEap

    # ⑮ LF 写出（生产者侧根因；EOL 陷阱同族）
    $lf = Test-M3LfWriterSelfTest
    Add-Result 'lf-writer-selftest' ($lf.ok) ($lf.reason)

    # ⑯ 本脚本从未触碰端口（自检/比较全离线）
    Add-Result 'no-port-touched' (-not $script:QuestTouchedPorts) 'offline-only'
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:QuestPass + $script:QuestFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:QuestPass, $total, ($script:QuestFail -join ','))
  if ($script:QuestFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（比较器 fail-closed + 改坏必红 + 开关耦合两向 + peer 护栏两向 + BotSrv 生产函数 selftest + 入口/DiffOnly/AssembleOnly/preflight/no-Go fail-closed + LF + 零端口）'
  exit 0
}

# =========================================================================== #
# Mode: AssembleOnly —— 只把 BotSrv M3QUESTSEQ 事件流（+服务端 dump）合并成 quest_raw.jsonl
# =========================================================================== #
if ($AssembleOnly) {
  if ([string]::IsNullOrEmpty($Markers)) { Write-Output 'PREREQ_MISSING: -AssembleOnly 需要 -Markers <file>'; exit 2 }
  if (-not (Test-Path -LiteralPath $Markers)) { Write-Output ("PREREQ_MISSING: 找不到 markers {0}" -f $Markers); exit 2 }
  $argv = @('-I', $QuestDiff, 'assemble', $Markers)
  if (-not [string]::IsNullOrEmpty($State)) { $argv += @('--state', $State) }
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
    $csPath = Join-Path (Join-Path $Sessions $CsharpSession) 'quest_raw.jsonl'
  }
  if ([string]::IsNullOrEmpty($rsPath) -and -not [string]::IsNullOrEmpty($RustSession)) {
    $rsPath = Join-Path (Join-Path $Sessions $RustSession) 'quest_raw.jsonl'
  }
  if ([string]::IsNullOrEmpty($csPath) -or [string]::IsNullOrEmpty($rsPath)) {
    Write-Output 'PREREQ_MISSING: -DiffOnly 需要 -CsharpRaw/-RustRaw（或 -CsharpSession/-RustSession）'
    exit 2
  }
  Invoke-QuestRawDiff -csharpPath $csPath -rustPath $rsPath
  $rcRaw = $script:QuestDiffRc
  if ($rcRaw -ne 0) { Write-Output ("RED: quest_diff 未通过（rc={0}）" -f $rcRaw); exit $rcRaw }
  # 若两侧给了会话目录，再比一次 DB 作用域
  if (-not [string]::IsNullOrEmpty($CsharpSession) -and -not [string]::IsNullOrEmpty($RustSession)) {
    Invoke-QuestScopedDiff -csharpDir (Join-Path $Sessions $CsharpSession) -rustDir (Join-Path $Sessions $RustSession)
    if ($script:QuestScopedRc -ne 0) { Write-Output ("RED: DB 作用域 diff 未通过（rc={0}）" -f $script:QuestScopedRc); exit $script:QuestScopedRc }
  }
  Write-Output 'GREEN: 任务跨侧 diff = 0'
  exit 0
}

# =========================================================================== #
# Mode: real run（真机两侧同序列）—— 未获 owner 窗口（-Go）前 fail-closed 拒跑
# =========================================================================== #
if (-not $Go) {
  Write-Output 'PREREQ_MISSING: 真机两侧同序列需 owner 发放窗口（起栈禁接管道；-Go 显式授权）'
  Write-Output ("NEXT: 获窗口后：`n  powershell -File run_quest_pair.ps1 -Side csharp -QuestCase {0} -Go`n  powershell -File run_quest_pair.ps1 -Side rust -QuestCase {0} -PeerSession <csharp 会话> -Go" -f $QuestCase)
  exit 2
}

$script:Scenario = Get-QuestScenario
Ensure-QuestDefaults
$QuestSteps = $script:QuestStepsArg
$caseDef = $script:Scenario.cases.$QuestCase

if ([string]::IsNullOrEmpty($Prefix)) {
  $Prefix = 'm3q' + (Get-Date -Format 'HHmmss')
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

# --- 0.5) preflight（T-27 同族）：Rust 侧脚本运行时是否已接线 ---
Write-Output '--- 0.5) preflight：Rust 任务/脚本运行时（T-27 同族；⑥-3） ---'
if ($Side -eq 'rust' -and -not $SkipRustPreflight) {
  if (-not (Test-RustQuestDispatchImplemented)) {
    Write-Output 'PREREQ_MISSING: Rust game-svr 未接脚本运行时（bins/game-svr/src 无 CM_CLICKNPC/MERCHANTDLGSELECT/SM_MERCHANTSAY/mir2_script）——跑下去 after 快照必是建角状态、跨侧 diff 必红（承接卡 openmir2-m3-quest-dialog-wiring / -quest-state / -quest-persist-observability；-SkipRustPreflight 仅调试用）'
    exit 2
  }
  Write-Output 'PREFLIGHT Rust 任务/脚本运行时：已接线（game-svr 命中任务/脚本常量）'
} else {
  Write-Output 'PREFLIGHT Rust 任务/脚本运行时：跳过（-Side csharp 或 -SkipRustPreflight）'
}

Write-Output '--- 1) 预检：MySQL ---'
if (-not (Test-MysqlTcp)) {
  Write-Output 'PREREQ_MISSING: MySQL 127.0.0.1:3306 不可达——dbsnap 取不到快照'
  exit 2
}
Write-Output 'PREFLIGHT MySQL 在听'

Write-Output '--- 2) 侧运行（复用 run_pair.ps1；两相 pin + 固定任务序列） ---'
$pairArgs = @('-Side', $Side, '-Bots', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port,
              '-TimeoutSec', $TimeoutSec, '-FixedNewChr', '-PinStartPose', '-QuestSequence',
              '-QuestSteps', $QuestSteps, '-QuestStepMs', $QuestStepMs, '-QuestStartDelayMs', $QuestStartDelayMs,
              '-PinMapName', [string]$caseDef.map, '-PinHomeMap', [int]$caseDef.home_map,
              '-PinHomeX', [int]$caseDef.player_x, '-PinHomeY', [int]$caseDef.player_y,
              '-PinCX', [int]$caseDef.player_x, '-PinCY', [int]$caseDef.player_y, '-PinDir', [int]$caseDef.player_dir,
              '-PinLevel', [int]$caseDef.pin_level)
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
$markers = @($baseline.quest_markers)
$hasDone = @($markers | Where-Object { $_ -match '"phase"\s*:\s*"done"' }).Count -ge 1
if ($markers.Count -eq 0 -or -not $hasDone) {
  Write-Output ("RED: 同序列判据不成立（quest_markers={0} 条，done={1}）——驱动没跑完整串" -f $markers.Count, $hasDone)
  exit 1
}
Write-Output ("QUESTSEQ_OK markers={0} done=True" -f $markers.Count)

$rawMine = Join-Path $mine.FullName 'quest_raw.jsonl'
if (-not (Test-Path -LiteralPath $rawMine)) {
  Write-Output ("PREREQ_MISSING: 本侧会话缺 quest_raw.jsonl（run_pair assemble 未产出）-> {0}" -f $rawMine)
  exit 2
}
# 任务态 dump：缺它 ⇒ 本侧记录缺 quest_flag_* ⇒ 跨侧 diff 会 PREREQ（fail-closed）
$stateMine = Join-Path $mine.FullName 'quest_state.jsonl'
if (-not (Test-Path -LiteralPath $stateMine)) {
  Write-Output ("PREREQ_MISSING: 本侧会话缺服务端任务态 dump（quest_state.jsonl）—— quest_raw.jsonl 缺 quest_flag_*，跨侧 diff 无法判『任务态一致』（承接卡 openmir2-m3-quest-dialog-wiring / -persist-observability）-> {0}" -f $stateMine)
  exit 2
}

if ($Side -eq 'rust' -and -not [string]::IsNullOrEmpty($PeerSession)) {
  $peer = Join-Path $Sessions $PeerSession
  if (-not (Test-Path $peer)) { Write-Output ("PREREQ_MISSING: 找不到 C# 对照会话 {0}" -f $peer); exit 2 }
  $localScope = Get-M3SessionQuestScope -Dir $mine.FullName
  if ($null -eq $localScope) { Write-Output ("PREREQ_MISSING: 本侧会话 baseline.json 缺/坏，读不出任务序列开关: {0}" -f $mine.FullName); exit 2 }
  $scopeGuard = Get-M3PeerQuestScopeGuard -PeerDir $peer -LocalScope $localScope
  foreach ($m in $scopeGuard.messages) { Write-Output $m }
  if (@($scopeGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧任务序列开关不同口径（{0}）——先按同一组开关重跑基线" -f (@($scopeGuard.failed) -join ','))
    exit 2
  }
  Write-Output ("QUESTSCOPE_OK peer='{0}' local='{1}'" -f $scopeGuard.peer_scope, $localScope)
  Invoke-QuestRawDiff -csharpPath (Join-Path $peer 'quest_raw.jsonl') -rustPath $rawMine
  $rcRaw = $script:QuestDiffRc
  Invoke-QuestScopedDiff -csharpDir $peer -rustDir $mine.FullName
  $rcScoped = $script:QuestScopedRc
  if ($rcRaw -eq 2 -or $rcScoped -eq 2) { exit 2 }
  if ($rcRaw -ne 0 -or $rcScoped -ne 0) {
    Write-Output ("RED: 跨侧判据未全绿（raw={0} scoped={1}）" -f $rcRaw, $rcScoped)
    exit 1
  }
  Write-Output 'GREEN: 两侧同序列 —— 任务逐 step diff = 0 且 DB 作用域字段级 diff = 0（任务态不落库口径）'
  exit 0
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_quest_pair.ps1 -Side rust -QuestCase {0} -PeerSession {1} -Go" -f $QuestCase, $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与同序列判据通过'
exit 0
