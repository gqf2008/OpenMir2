#requires -Version 5
<#
.SYNOPSIS
  run_npc_shop_pair.ps1 — M3⑤「两侧同序列」NPC/商店对拍的一条命令（卡 `openmir2-m3-shop-fixture-botsrv`）。

.DESCRIPTION
  目标（父卡 `openmir2-m3-npc-shop-align` 的 DoD②③）：把**同一串商店操作**（开面板 → 卖 → 买回 →
  查卖价 → 查修理价 → 修理 → 存取，两侧同一串 `CM_*` 字节）分别打在 C# 栈与 Rust 栈上 ⇒
  ① `npc_shop_diff.py diff` 作用域内逐 op 字段级 diff = 0；② `scoped_diff.py diff --canonical --fixed-newchr`
  在 DB 侧（`characters` 金币/背包 + `characters_storageitem`）diff = 0。

  本脚本**不新造编排**：一次「侧运行」复用 `run_pair.ps1`（两相固定起点 `-PinStartPose` + 预热相
  `-StopAfterCharCreated` + BEFORE/AFTER 快照 + 收尾「等断线存档落库」settle 仪器），本脚本只加：
    ① 入口实现识别护栏（-Side 与 5000/7000/7100/7200 的实际监听进程必须对应）；
    ② `-ShopSequence` 序列参数透传 + `M3SHOPSEQ` 标记判据（序列没跑完整 ⇒ RED，绝不当 GREEN）；
    ③ 跨侧 `npc_shop_diff.py`（NPC/商店记录）+ `scoped_diff.py`（DB 字段级）两道判据；
    ④ `-SelfCheckNoStack` 无栈自检（比较器 selftest + 合成件正/负样本 + BotSrv 生产函数 selftest +
       入口/DiffOnly/preflight fail-closed + 零端口断言）。

  真机跑要窗口（起栈禁接管道）：未给 `-Go` 时 real-run **fail-closed 拒跑（exit 2）**，不占端口。
  前置（T-27 同族口径）：Rust 侧 ⑤-2/⑤-4/⑤-5（NPC 实体 / 8 条 `CM_*` 派发 / 仓库会话态）落地前
  `-Side rust` 机判 preflight 直接 PREREQ（exit 2），避免白占窗口。

  Mode：
    -Side csharp|rust [可选参数]     跑一侧（复用 run_pair.ps1）。
    -Side rust -PeerSession <C#会话> 跑 Rust 侧并跨侧 diff（= 本卡的正式判据）。
    -DiffOnly -CsharpRaw <f> -RustRaw <f>  只重比既有原始件（**不碰端口**）。
    -AssembleOnly -Markers <f> -Out <f>    只把 BotSrv 的 `M3SHOPSEQ` 事件流合并成记录。
    -SelfCheckNoStack                无栈自检：所有路径都必须 fail-closed。

  窗口纪律：除 -DiffOnly/-AssembleOnly/-SelfCheckNoStack 外都会连真栈；起两侧栈前先看
  `E:\MirServer\port-status.ps1`，与 M2/M3 其它窗口严格串行（**别并行起栈**）。
  本脚本**不启动/不停止任何服务**——栈的起停仍由既有脚本/人工负责。

.EXAMPLE
  # ① C# 基线（C# 栈在听；单假人 + 两相 pin 到 NPC 旁 + 固定商店序列）
  powershell -ExecutionPolicy Bypass -File run_npc_shop_pair.ps1 -Side csharp -ShopCase buy_sell_repair -Go
  # ② 换成 Rust 栈后跑 Rust 侧并与上面的 C# 会话对拍
  powershell -ExecutionPolicy Bypass -File run_npc_shop_pair.ps1 -Side rust -ShopCase buy_sell_repair -PeerSession csharp-... -Go
  # ③ 只重比既有产物（不碰端口）
  powershell -ExecutionPolicy Bypass -File run_npc_shop_pair.ps1 -DiffOnly -CsharpRaw <f> -RustRaw <f>
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_npc_shop_pair.ps1 -SelfCheckNoStack
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
  [ValidateSet('buy_sell_repair', 'storage')][string]$ShopCase = 'buy_sell_repair',
  [string]$ShopOps = '',           # 空 ⇒ 取场景 case.ops
  [int]$ShopStepMs = 1000,
  [int]$ShopStartDelayMs = 3000,
  [string]$ShopNpcName = '',       # 空 ⇒ 取场景 case.npc
  [string]$ShopItemName = '',      # 空 ⇒ 取场景 case.item
  [int]$ShopItemMake = 0,
  [int]$ShopCount = 1,
  [string]$PortsCsv = '',          # 入口端口（逗号分隔）；默认 5000,7000,7100,7200
  [switch]$DiffOnly,
  [switch]$AssembleOnly,
  [string]$Markers = '',
  [string]$Out = '',
  [switch]$Go,                     # owner 发放窗口的显式授权；未给 ⇒ real-run fail-closed（exit 2）
  [switch]$SkipRustPreflight,      # 仅供调试：跳过「Rust 侧商店派发未实现」前置（默认不跳）
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
$ShopDiff = Join-Path $Here 'npc_shop_diff.py'
$RunBots = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$Sessions = Join-Path $Here 'sessions'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
if ([string]::IsNullOrEmpty($ScenarioFile)) { $ScenarioFile = Join-Path $Here 'npc_shop_scenario.json' }
. (Join-Path $Here 'gate_guards.ps1')
. (Join-Path $Here 'm3hashutil.ps1')

# 脚本作用域：本脚本（含自检）**从不**绑定/连接任何端口；real-run 由外部栈 + run_pair 负责。
$script:ShopTouchedPorts = $false

# ---- 场景固定输入（case.ops / case.npc / case.item 缺省填充） ----------------
function Get-NpcShopScenario {
  if (-not (Test-Path -LiteralPath $ScenarioFile)) { throw ("找不到场景文件: {0}" -f $ScenarioFile) }
  return (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)
}
$script:Scenario = $null
function Ensure-NpcShopDefaults {
  if ($null -eq $script:Scenario) { return }
  $case = $script:Scenario.cases.$ShopCase
  if ($null -eq $case) { throw ("场景 {0} 里没有 case '{1}'" -f $ScenarioFile, $ShopCase) }
  if ([string]::IsNullOrEmpty($script:ShopOpsArg)) { $script:ShopOpsArg = (@($case.ops) -join ',') }
  if ([string]::IsNullOrEmpty($script:ShopNpcArg)) { $script:ShopNpcArg = [string]$case.npc }
  if ([string]::IsNullOrEmpty($script:ShopItemArg)) { $script:ShopItemArg = [string]$case.item }
}

# 把 CLI 覆盖值拷进脚本作用域变量（供 Ensure-NpcShopDefaults 填充）。
$script:ShopOpsArg = $ShopOps
$script:ShopNpcArg = $ShopNpcName
$script:ShopItemArg = $ShopItemName

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

# preflight（T-27 同族口径）：⑤-2/⑤-4/⑤-5 落地前 `bins/game-svr` 不派发任何商店 `CM_*`
# ⇒ 跑下去 Rust 侧 after 快照仍是建角状态、跨侧 diff 必红。机判：game-svr 源码出现任一商店常量/消息号。
# 承接卡 openmir2-m3-merchant-entity-tick / -shop-dialog-wiring / -storage-persist。
function Test-RustShopDispatchImplemented {
  param([string]$SrcDir = '')
  if ([string]::IsNullOrEmpty($SrcDir)) { $SrcDir = Join-Path $RepoRoot 'mir2-rs\bins\game-svr\src' }
  if (-not (Test-Path $SrcDir)) { return $false }
  $hits = @(Get-ChildItem $SrcDir -Recurse -Filter '*.rs' -ErrorAction SilentlyContinue |
            Select-String -Pattern 'MERCHANTDLGSELECT|USERBUYITEM|USERSELLITEM|USERREPAIRITEM|USERSTORAGEITEM|TAKEBACKSTORAGEITEM' -ErrorAction SilentlyContinue)
  return ($hits.Count -gt 0)
}

# CrossSideDiff 的退出码（脚本作用域）：不能把带 Write-Output 的函数直接写进 `exit (...)`——
# 括起来的子表达式会把输出流全部吃掉，exit 拿到数组/字符串 ⇒ 退出码变 0、消息也不打印（实跑踩到）。
$script:ShopDiffRc = 2
function Invoke-ShopRawDiff([string]$csharpPath, [string]$rustPath) {
  $script:ShopDiffRc = 2
  if (-not (Test-Path -LiteralPath $csharpPath)) { Write-Output ("PREREQ_MISSING: C# 原始件缺失 -> {0}" -f $csharpPath); return }
  if (-not (Test-Path -LiteralPath $rustPath)) { Write-Output ("PREREQ_MISSING: Rust 原始件缺失 -> {0}" -f $rustPath); return }
  Write-Output '--- npc_shop_diff.py diff（NPC/商店逐 op 字段级） ---'
  & python -I $ShopDiff diff $csharpPath $rustPath
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: NPC/商店跨侧逐 op diff = 0'; $script:ShopDiffRc = 0; return }
  Write-Output 'RED: NPC/商店跨侧 diff != 0（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）'
  $script:ShopDiffRc = 1
}

$script:ShopScopedRc = 2
function Invoke-ShopScopedDiff([string]$csharpDir, [string]$rustDir) {
  $script:ShopScopedRc = 2
  $a = Join-Path $csharpDir 'scoped_after_norm.jsonl'
  $b = Join-Path $rustDir 'scoped_after_norm.jsonl'
  if (-not (Test-Path -LiteralPath $a)) { Write-Output ("PREREQ_MISSING: C# 会话缺 scoped_after_norm.jsonl -> {0}" -f $a); return }
  if (-not (Test-Path -LiteralPath $b)) { Write-Output ("PREREQ_MISSING: Rust 会话缺 scoped_after_norm.jsonl -> {0}" -f $b); return }
  Write-Output '--- 跨侧作用域 DB 字段级 diff（scoped_diff --canonical --fixed-newchr） ---'
  & python -I $Scoped diff $a $b --canonical --fixed-newchr
  if ($LASTEXITCODE -eq 0) { Write-Output 'GREEN: 跨侧作用域内 DB 字段级 diff = 0'; $script:ShopScopedRc = 0; return }
  Write-Output 'RED: 跨侧 DB diff != 0'
  $script:ShopScopedRc = 1
}

# =========================================================================== #
# Mode: SelfCheckOut —— 把无栈自检转录**由脚本自己**以 LF 落盘
# =========================================================================== #
if ($SelfCheckNoStack -and -not [string]::IsNullOrEmpty($SelfCheckOut)) {
  $selfTmp = Join-Path $env:TEMP ('npcshop-selfcheck-' + [guid]::NewGuid().ToString('N') + '.txt')
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
  $script:ShopPass = 0
  $script:ShopFail = @()
  $tmpRoot = Join-Path $env:TEMP ('npcshop-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  function Add-Result([string]$Name, [bool]$Ok, [string]$Detail) {
    $suffix = if ([string]::IsNullOrEmpty($Detail)) { '' } else { " ($Detail)" }
    if ($Ok) {
      $script:ShopPass++
      Write-Output ("SELFTEST ok {0}{1}" -f $Name, $suffix)
    } else {
      $script:ShopFail += $Name
      Write-Output ("SELFTEST FAIL {0}{1}" -f $Name, $suffix)
    }
  }

  function New-ShopRaw([string]$Path, [string]$Branch, [int]$FrameSm, [int]$FrameA) {
    $lines = @(
      ('{"k":0,"op":"sell","npc":"卫家店","npc_actor":101,"req":{"item":"sword","n":1},"frames":[{"sm":' + $FrameSm + ',"a":' + $FrameA + ',"b":0,"c":0,"d":0}],"gold_before":1000,"gold_after":1010,"bag_before":["sword#42","candle#41"],"bag_after":["candle#41"],"branch":"' + $Branch + '"}')
    )
    Write-M3LfLines -Path $Path -Lines $lines
  }

  try {
    # ① 场景固定输入（case 表 + 默认 case）
    $scOk = $false
    try {
      $sc = Get-NpcShopScenario
      $case = $sc.cases.$($sc.default_case)
      $scOk = ($sc.canon -eq 'm3-npc-shop-v1') -and ($null -ne $case) -and
              (-not [string]::IsNullOrEmpty($case.npc)) -and (@($case.ops).Count -ge 2) -and
              ($null -ne $sc.cases.storage)
    } catch { $scOk = $false }
    Add-Result 'scenario-fixed-input' $scOk 'canon/cases/buy_sell_repair/storage'

    # ② 比较器 selftest（fail-closed + 改坏必红）必须 GREEN
    $stOut = ''
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $stOut = (& python -I $ShopDiff selftest 2>&1 | Out-String)
    $stRc = $LASTEXITCODE
    $ErrorActionPreference = $prevEap
    $stMatch = [regex]::Match($stOut, 'SELFTEST ok=(\d+)/(\d+)')
    $stOk = ($stRc -eq 0) -and $stMatch.Success -and ($stMatch.Groups[1].Value -eq $stMatch.Groups[2].Value)
    Add-Result 'comparator-selftest' $stOk ("rc=$stRc")
    if (-not $stOk) { Write-Output $stOut }

    # ③ 逐项相同 ⇒ 绿（进程内集成，走生产 diff 函数）
    $csGood = Join-Path $tmpRoot 'cs-good.jsonl'
    $rsGood = Join-Path $tmpRoot 'rs-good.jsonl'
    New-ShopRaw -Path $csGood -Branch 'ok' -FrameSm 648 -FrameA 1010
    New-ShopRaw -Path $rsGood -Branch 'ok' -FrameSm 648 -FrameA 1010
    $rc1 = Invoke-ShopRawDiff -csharpPath $csGood -rustPath $rsGood
    Add-Result 'identical-green' ($script:ShopDiffRc -eq 0) ("rc=$($script:ShopDiffRc)")

    # ④ 改坏失败分支 ⇒ 必红且点名 branch/帧种类
    $rsBad = Join-Path $tmpRoot 'rs-branch.jsonl'
    New-ShopRaw -Path $rsBad -Branch 'fail_nomoney' -FrameSm 651 -FrameA 3
    $null = Invoke-ShopRawDiff -csharpPath $csGood -rustPath $rsBad
    Add-Result 'mutate-branch-red' ($script:ShopDiffRc -eq 1) ("rc=$($script:ShopDiffRc)")

    # ⑤ 缺文件 ⇒ PREREQ fail-closed（rc=2）
    $null = Invoke-ShopRawDiff -csharpPath (Join-Path $tmpRoot 'nope.jsonl') -rustPath $rsGood
    Add-Result 'missing-file-prereq' ($script:ShopDiffRc -eq 2) ("rc=$($script:ShopDiffRc)")

    # ⑥ run_pair 的开关耦合：-ShopSequence 必须**一并传 -DisableAutoPlay**；默认路径不得出现。
    $emitOnRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -ShopSequence -ShopOps 'sell,buy' -ShopStepMs 1000 -ShopStartDelayMs 3000 -ShopNpcName '卫家店' -ShopItemName '木剑' -ShopItemMake 0 -ShopCount 1
    if ($LASTEXITCODE -ne 0) { $script:ShopFail += 'emit_on_rc=non-zero' }
    else {
      $emitOn = (($emitOnRaw | Out-String).Trim() | ConvertFrom-Json)
      $has = (@($emitOn.bot_args) -contains '-DisableAutoPlay')
      if (-not $has) { $script:ShopFail += 'shop_sequence_does_not_pass_disable_autoplay' }
      if ($emitOn.ops_runner -notmatch '-DisableAutoPlay') { $script:ShopFail += 'ops_runner_missing_disable_autoplay' }
      if ($emitOn.shop_sequence -notmatch '^on ops=sell,buy step_ms=1000 start_delay_ms=3000 npc=卫家店 item=木剑 make=0 n=1 disable_autoplay=True$') { $script:ShopFail += ('shop_scope_on=' + $emitOn.shop_sequence) }
      if ($null -eq $emitOn.save_settle_instrument) { $script:ShopFail += 'shop_on_missing_save_settle_instrument' }
      Add-Result 'emit-on-coupling' $true ("scope={0}" -f $emitOn.shop_sequence)
    }
    $emitOffRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs
    if ($LASTEXITCODE -ne 0) { $script:ShopFail += 'emit_off_rc=non-zero' }
    else {
      $emitOff = (($emitOffRaw | Out-String).Trim() | ConvertFrom-Json)
      $hasOff = (@($emitOff.bot_args) -contains '-DisableAutoPlay')
      if ($hasOff) { $script:ShopFail += 'default_path_unexpectedly_passes_disable_autoplay' }
      if ($emitOff.shop_sequence -ne '(off)') { $script:ShopFail += ('shop_scope_off=' + $emitOff.shop_sequence) }
      Add-Result 'emit-off-default' $true ("scope={0}" -f $emitOff.shop_sequence)
    }

    # ⑦ peer 口径护栏有牙齿：两侧 ops 不同 ⇒ 必红
    $legacyDir = Join-Path $tmpRoot 'peer'
    New-Item -ItemType Directory -Force -Path $legacyDir | Out-Null
    $peerBaseline = [pscustomobject]@{ shop_sequence = 'on ops=sell step_ms=1000 start_delay_ms=3000 npc=卫家店 item=木剑 make=0 n=1 disable_autoplay=True' }
    $peerBaseline | ConvertTo-Json | Set-Content -Path (Join-Path $legacyDir 'baseline.json') -Encoding utf8
    $g = Get-M3PeerShopScopeGuard -PeerDir $legacyDir -LocalScope ('on ops=sell,buy step_ms=1000 start_delay_ms=3000 npc=卫家店 item=木剑 make=0 n=1 disable_autoplay=True')
    Add-Result 'peer-scope-red' ((@($g.failed) -contains 'peer_shop_scope_mismatch')) ("codes=" + (@($g.failed) -join ','))
    $g2 = Get-M3PeerShopScopeGuard -PeerDir $legacyDir -LocalScope $peerBaseline.shop_sequence
    Add-Result 'peer-scope-green' (@($g2.failed).Count -eq 0) ("codes=" + (@($g2.failed) -join ','))

    # ⑧ 入口护栏 fail-closed：一列空闲端口（无监听）⇒ 必须 PREREQ（exit 2），不能静默 GREEN。
    $portProbeOut = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -Side rust -PortsCsv 17991,17992,17993,17994 -Go 2>&1
    $rcPort = $LASTEXITCODE
    if ($rcPort -ne 2) { $script:ShopFail += ("no_stack_port_guard_rc=$rcPort(expect 2)") }
    Add-Result 'no-stack-port-guard' ($rcPort -eq 2) ("rc=$rcPort")

    # ⑨ -DiffOnly 缺原始件 ⇒ PREREQ（exit 2）
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -DiffOnly -CsharpRaw (Join-Path $tmpRoot 'nope.jsonl') -RustRaw $rsGood 2>&1
    $rcDiff = $LASTEXITCODE
    if ($rcDiff -ne 2) { $script:ShopFail += ("diffonly_missing_raw_rc=$rcDiff(expect 2)") }
    Add-Result 'diffonly-missing-prereq' ($rcDiff -eq 2) ("rc=$rcDiff")

    # ⑩ -AssembleOnly 缺 markers ⇒ PREREQ（exit 2）
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -AssembleOnly -Markers (Join-Path $tmpRoot 'nope.jsonl') -Out (Join-Path $tmpRoot 'o.jsonl') 2>&1
    $rcAsm = $LASTEXITCODE
    if ($rcAsm -ne 2) { $script:ShopFail += ("assembleonly_missing_rc=$rcAsm(expect 2)") }
    Add-Result 'assembleonly-missing-prereq' ($rcAsm -eq 2) ("rc=$rcAsm")

    # ⑪ Rust preflight 有牙齿：空目录 ⇒ false；写了商店常量 ⇒ true。
    $tmpSrc = Join-Path $tmpRoot 'shop-probe'
    New-Item -ItemType Directory -Force -Path $tmpSrc | Out-Null
    if (Test-RustShopDispatchImplemented -SrcDir $tmpSrc) { $script:ShopFail += 'shop_preflight_empty_dir_true' }
    Set-Content -Path (Join-Path $tmpSrc 'x.rs') -Value 'pub const CM_USERBUYITEM: u16 = 1014;' -Encoding utf8
    if (-not (Test-RustShopDispatchImplemented -SrcDir $tmpSrc)) { $script:ShopFail += 'shop_preflight_hit_false' }
    Add-Result 'rust-preflight-teeth' (($script:ShopFail -notcontains 'shop_preflight_empty_dir_true') -and ($script:ShopFail -notcontains 'shop_preflight_hit_false')) 'empty=false hit=true'

    # ⑫ 未给 -Go ⇒ real-run fail-closed（exit 2），**不许**静默起栈/占端口。
    $null = & powershell -ExecutionPolicy Bypass -File $PSCommandPath -Side csharp -PortsCsv 17995,17996,17997,17998 2>&1
    $rcNoGo = $LASTEXITCODE
    if ($rcNoGo -ne 2) { $script:ShopFail += ("no_go_realrun_rc=$rcNoGo(expect 2)") }
    Add-Result 'no-go-realrun-prereq' ($rcNoGo -eq 2) ("rc=$rcNoGo")

    # ⑬ BotSrv 生产函数 selftest（op 解析 / 分支归类 / 价格抽取）——改坏必红（red-on-mutation 由代码 review + 本断言钉住）。
    $botProj = Join-Path $RepoRoot 'src\BotSrv\BotSrv.csproj'
    $botExe = Join-Path $RepoRoot 'src\BotSrv\bin\Release\BotSrv.exe'
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $botBuildOut = (& dotnet build $botProj -c Release --nologo -v q 2>&1 | Out-String)
    $botBuildRc = $LASTEXITCODE
    if ($botBuildRc -ne 0) { $script:ShopFail += ('botsrv_build_rc=' + $botBuildRc); Write-Output $botBuildOut }
    else {
      $seqOut = (& $botExe --selftest-shopseq 2>&1 | Out-String)
      $seqRc = $LASTEXITCODE
      if ($seqRc -ne 0) { $script:ShopFail += ('shopseq_selftest_rc=' + $seqRc); Write-Output $seqOut }
      if ($seqOut -notmatch 'SHOPSEQ SELFTEST GREEN') { $script:ShopFail += 'shopseq_selftest_not_green' }
      Add-Result 'botsrv-shopseq-selftest' ($seqRc -eq 0) ("rc=$seqRc")
    }
    $ErrorActionPreference = $prevEap

    # ⑭ LF 写出（生产者侧根因；EOL 陷阱同族）
    $lf = Test-M3LfWriterSelfTest
    Add-Result 'lf-writer-selftest' ($lf.ok) ($lf.reason)

    # ⑮ 本脚本从未触碰端口（自检/比较全离线）
    Add-Result 'no-port-touched' (-not $script:ShopTouchedPorts) 'offline-only'
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:ShopPass + $script:ShopFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:ShopPass, $total, ($script:ShopFail -join ','))
  if ($script:ShopFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（比较器 fail-closed + 改坏必红 + 开关耦合两向 + peer 护栏两向 + BotSrv 生产函数 selftest + 入口/DiffOnly/AssembleOnly/preflight/no-Go fail-closed + LF + 零端口）'
  exit 0
}

# =========================================================================== #
# Mode: AssembleOnly —— 只把 BotSrv M3SHOPSEQ 事件流合并成 npc_shop_raw.jsonl
# =========================================================================== #
if ($AssembleOnly) {
  if ([string]::IsNullOrEmpty($Markers)) { Write-Output 'PREREQ_MISSING: -AssembleOnly 需要 -Markers <file>'; exit 2 }
  if (-not (Test-Path -LiteralPath $Markers)) { Write-Output ("PREREQ_MISSING: 找不到 markers {0}" -f $Markers); exit 2 }
  $argv = @('-I', $ShopDiff, 'assemble', $Markers)
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
    $csPath = Join-Path (Join-Path $Sessions $CsharpSession) 'npc_shop_raw.jsonl'
  }
  if ([string]::IsNullOrEmpty($rsPath) -and -not [string]::IsNullOrEmpty($RustSession)) {
    $rsPath = Join-Path (Join-Path $Sessions $RustSession) 'npc_shop_raw.jsonl'
  }
  if ([string]::IsNullOrEmpty($csPath) -or [string]::IsNullOrEmpty($rsPath)) {
    Write-Output 'PREREQ_MISSING: -DiffOnly 需要 -CsharpRaw/-RustRaw（或 -CsharpSession/-RustSession）'
    exit 2
  }
  Invoke-ShopRawDiff -csharpPath $csPath -rustPath $rsPath
  $rcRaw = $script:ShopDiffRc
  if ($rcRaw -ne 0) { Write-Output ("RED: npc_shop_diff 未通过（rc={0}）" -f $rcRaw); exit $rcRaw }
  # 若两侧给了会话目录，再比一次 DB 作用域
  if (-not [string]::IsNullOrEmpty($CsharpSession) -and -not [string]::IsNullOrEmpty($RustSession)) {
    Invoke-ShopScopedDiff -csharpDir (Join-Path $Sessions $CsharpSession) -rustDir (Join-Path $Sessions $RustSession)
    if ($script:ShopScopedRc -ne 0) { Write-Output ("RED: DB 作用域 diff 未通过（rc={0}）" -f $script:ShopScopedRc); exit $script:ShopScopedRc }
  }
  Write-Output 'GREEN: NPC/商店跨侧 diff = 0'
  exit 0
}

# =========================================================================== #
# Mode: real run（真机两侧同序列）—— 未获 owner 窗口（-Go）前 fail-closed 拒跑
# =========================================================================== #
if (-not $Go) {
  Write-Output 'PREREQ_MISSING: 真机两侧同序列需 owner 发放窗口（起栈禁接管道；-Go 显式授权）'
  Write-Output ("NEXT: 获窗口后：`n  powershell -File run_npc_shop_pair.ps1 -Side csharp -ShopCase {0} -Go`n  powershell -File run_npc_shop_pair.ps1 -Side rust -ShopCase {0} -PeerSession <csharp 会话> -Go" -f $ShopCase)
  exit 2
}

$script:Scenario = Get-NpcShopScenario
Ensure-NpcShopDefaults
$ShopOps = $script:ShopOpsArg
$ShopNpcName = $script:ShopNpcArg
$ShopItemName = $script:ShopItemArg
$caseDef = $script:Scenario.cases.$ShopCase

if ([string]::IsNullOrEmpty($Prefix)) {
  $Prefix = 'm3s' + (Get-Date -Format 'HHmmss')
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

# --- 0.5) preflight（T-27 同族）：Rust 侧商店派发是否已实现 ---
Write-Output '--- 0.5) preflight：Rust 商店派发（T-27 同族；⑤-2/⑤-4/⑤-5） ---'
if ($Side -eq 'rust' -and -not $SkipRustPreflight) {
  if (-not (Test-RustShopDispatchImplemented)) {
    Write-Output 'PREREQ_MISSING: Rust game-svr 未实现 NPC/商店 CM_* 派发（bins/game-svr/src 无 MERCHANTDLGSELECT/USERBUYITEM/USERSELLITEM/USERREPAIRITEM/USERSTORAGEITEM/TAKEBACKSTORAGEITEM）——跑下去 after 快照必是建角状态、跨侧 diff 必红（承接卡 openmir2-m3-merchant-entity-tick / -shop-dialog-wiring / -storage-persist；-SkipRustPreflight 仅调试用）'
    exit 2
  }
  Write-Output 'PREFLIGHT Rust 商店派发：已实现（game-svr 命中商店 CM_* 常量）'
} else {
  Write-Output 'PREFLIGHT Rust 商店派发：跳过（-Side csharp 或 -SkipRustPreflight）'
}

Write-Output '--- 1) 预检：MySQL ---'
if (-not (Test-MysqlTcp)) {
  Write-Output 'PREREQ_MISSING: MySQL 127.0.0.1:3306 不可达——dbsnap 取不到快照'
  exit 2
}
Write-Output 'PREFLIGHT MySQL 在听'

Write-Output '--- 2) 侧运行（复用 run_pair.ps1；两相 pin + 固定商店序列） ---'
$pairArgs = @('-Side', $Side, '-Bots', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port,
              '-TimeoutSec', $TimeoutSec, '-FixedNewChr', '-PinStartPose', '-ShopSequence',
              '-ShopOps', $ShopOps, '-ShopStepMs', $ShopStepMs, '-ShopStartDelayMs', $ShopStartDelayMs,
              '-ShopNpcName', $ShopNpcName, '-ShopItemName', $ShopItemName, '-ShopItemMake', $ShopItemMake,
              '-ShopCount', $ShopCount,
              '-PinMapName', [string]$caseDef.map, '-PinHomeMap', [int]$caseDef.home_map,
              '-PinHomeX', [int]$caseDef.player_x, '-PinHomeY', [int]$caseDef.player_y,
              '-PinCX', [int]$caseDef.player_x, '-PinCY', [int]$caseDef.player_y, '-PinDir', [int]$caseDef.player_dir)
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
$markers = @($baseline.shop_markers)
$hasDone = @($markers | Where-Object { $_ -match '"phase"\s*:\s*"done"' }).Count -ge 1
if ($markers.Count -eq 0 -or -not $hasDone) {
  Write-Output ("RED: 同序列判据不成立（shop_markers={0} 条，done={1}）——驱动没跑完整串" -f $markers.Count, $hasDone)
  exit 1
}
Write-Output ("SHOPSEQ_OK markers={0} done=True" -f $markers.Count)

$rawMine = Join-Path $mine.FullName 'npc_shop_raw.jsonl'
if (-not (Test-Path -LiteralPath $rawMine)) {
  Write-Output ("PREREQ_MISSING: 本侧会话缺 npc_shop_raw.jsonl（run_pair assemble 未产出）-> {0}" -f $rawMine)
  exit 2
}

if ($Side -eq 'rust' -and -not [string]::IsNullOrEmpty($PeerSession)) {
  $peer = Join-Path $Sessions $PeerSession
  if (-not (Test-Path $peer)) { Write-Output ("PREREQ_MISSING: 找不到 C# 对照会话 {0}" -f $peer); exit 2 }
  $localScope = Get-M3SessionShopScope -Dir $mine.FullName
  if ($null -eq $localScope) { Write-Output ("PREREQ_MISSING: 本侧会话 baseline.json 缺/坏，读不出商店序列开关: {0}" -f $mine.FullName); exit 2 }
  $scopeGuard = Get-M3PeerShopScopeGuard -PeerDir $peer -LocalScope $localScope
  foreach ($m in $scopeGuard.messages) { Write-Output $m }
  if (@($scopeGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧商店序列开关不同口径（{0}）——先按同一组开关重跑基线" -f (@($scopeGuard.failed) -join ','))
    exit 2
  }
  Write-Output ("SHOPSCOPE_OK peer='{0}' local='{1}'" -f $scopeGuard.peer_scope, $localScope)
  Invoke-ShopRawDiff -csharpPath (Join-Path $peer 'npc_shop_raw.jsonl') -rustPath $rawMine
  $rcRaw = $script:ShopDiffRc
  Invoke-ShopScopedDiff -csharpDir $peer -rustDir $mine.FullName
  $rcScoped = $script:ShopScopedRc
  if ($rcRaw -eq 2 -or $rcScoped -eq 2) { exit 2 }
  if ($rcRaw -ne 0 -or $rcScoped -ne 0) {
    Write-Output ("RED: 跨侧判据未全绿（raw={0} scoped={1}）" -f $rcRaw, $rcScoped)
    exit 1
  }
  Write-Output 'GREEN: 两侧同序列 —— NPC/商店逐 op diff = 0 且 DB 作用域字段级 diff = 0'
  exit 0
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_npc_shop_pair.ps1 -Side rust -ShopCase {0} -PeerSession {1} -Go" -f $ShopCase, $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与同序列判据通过'
exit 0
