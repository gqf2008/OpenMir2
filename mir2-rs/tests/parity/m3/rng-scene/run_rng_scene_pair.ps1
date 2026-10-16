#requires -Version 5
<#
  run_rng_scene_pair.ps1 —— M3 RNG **场景驱动器**（卡 `openmir2-m3-rng-scene-driver`，R3 承接卡）。

  一句话：把「固定图 + 固定怪 + 固定走位 + 固定待机 + 同种子」的**单场景两相 trace** 两侧各跑一次，
  落 `<side>-scene.json`（canon `m3-rng-scene-descriptor-v1`）+ `<side>-rng-stream.tsv`（5 列，
  头 `INJECT_OK <seed>`），再交给既有消费者判 diff=0。

  与既有件的关系（**不重复造轮子**）：
    * 严格账本校验 + 单一比较入口 = `rng_trace_diff.py`（本片新增）；账本**形状**（头种子 / seq 1..N
      连续 / 5 列 / site 非空）在此之前是静默通道 ⇒ 这里钉成 fail-closed（重审 §2-4「断号/缺件即红」）。
    * 场景一致性门 = `rng-trace-scene/rng_scene_gate.py`（既有，不改）。
    * 组账本比较 = `rng-trace-probe/rng_ledger_diff.py groupdiff --fold --strict`（既有，不改）。
    * DoD① 单一入口 = `rng-trace-scene/run_rng_trace_pair.ps1 -SceneGroupDiff`（既有，本脚本**委托**它，
      不复制它的门→比逻辑）。

  用法：
    # 离线自检（零端口、零 .rs、不起栈）
    powershell -ExecutionPolicy Bypass -File run_rng_scene_pair.ps1 -SelfCheckNoStack
    powershell -ExecutionPolicy Bypass -File run_rng_scene_pair.ps1 -SelfCheckNoStack -SelfCheckOut <path.txt>
    # 只严格校验一侧流水形状（离线）
    powershell -ExecutionPolicy Bypass -File run_rng_scene_pair.ps1 -TraceCheckOnly -Side csharp \
      -AStream <csharp-rng-stream.tsv> -Seed 42
    # 场景一致性门 / DoD① 单一入口（离线；委托既有 runner）
    powershell -ExecutionPolicy Bypass -File run_rng_scene_pair.ps1 -GateOnly \
      -AScene <csharp-scene.json> -BScene <rust-scene.json> -AStream <csharp.tsv> -BStream <rust.tsv>
    powershell -ExecutionPolicy Bypass -File run_rng_scene_pair.ps1 -SceneGroupDiff \
      -AScene <csharp-scene.json> -BScene <rust-scene.json> -AStream <csharp.tsv> -BStream <rust.tsv>

  窗口纪律：真机两相 trace 需 5000/7000/7100/7200 独占窗口（起栈禁接管道）；未获 owner `-Go`
  前一律 fail-closed（exit 2），绝不静默起栈。两侧 spawn/dump 接线（crates/world + bins/game-svr）
  在承接卡写集外 ⇒ 本片只交「严格校验器 + 编排骨架 + BotSrv 侧固定场景序列（T-59 登记，默认关）」。
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Seed = 42,
  [string]$Prefix = '',
  [string]$PeerSession = '',
  [string]$ScenarioFile = '',
  [string]$AScene = '',
  [string]$BScene = '',
  [string]$AStream = '',
  [string]$BStream = '',
  [string]$PortsCsv = '5000,7000,7100,7200',
  [switch]$GateOnly,          # 委托既有 runner 的场景一致性前置门（离线）
  [switch]$SceneGroupDiff,    # 委托既有 runner 的 DoD① 单一入口（门→比；非 GREEN 拒比）
  [switch]$TraceCheckOnly,    # 只严格校验 -AStream 的形状（离线）
  [switch]$Go,                # owner 发放窗口的显式授权；未给 ⇒ real-run fail-closed（exit 2）
  [switch]$PinScene,          # 真机必须显式请求「按 scenario 钉住含怪场景」
  [switch]$SelfCheckNoStack,
  [string]$SelfCheckOut = ''
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
# 本脚本**已声明**的参数名（供「scenario runner 参数名必须已声明」机判用，M-1 同族）。
$script:SceneDeclaredParams = @($MyInvocation.MyCommand.Parameters.Keys)
$M3 = Split-Path -Parent $Here
$TraceDiff = Join-Path $Here 'rng_trace_diff.py'
$PeerRunner = Join-Path $M3 'rng-trace-scene/run_rng_trace_pair.ps1'
$PeerScenario = Join-Path $M3 'rng-trace-scene/rng_trace_scenario.json'
. (Join-Path $M3 'm3hashutil.ps1')

if ([string]::IsNullOrEmpty($ScenarioFile)) { $ScenarioFile = Join-Path $Here 'rng_scene_scenario.json' }

$Ports = @($PortsCsv.Split(',') | ForEach-Object { [int]$_.Trim() })
$script:SceneTouchedPorts = $false   # 本脚本（含自检）**从不**绑定/连接任何端口。

# 返回 [int]$LASTEXITCODE；子进程 stdout 走 -Out 引用，**不混进返回值**
# （历史 bug：`$rc = & python …` 会拿到 @(<stdout>, $LASTEXITCODE) ⇒ `-ne 0` 恒真）。
function Invoke-SceneTraceCheck {
  param([Parameter(Mandatory = $true)][string]$Stream, [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
        [int]$Seed = 42, [string]$LedgerOut = '', [Parameter(Mandatory = $true)][ref]$Out)
  $argv = @('-I', $TraceDiff, 'check', $Stream, '--side', $Side, '--seed', [string]$Seed)
  if (-not [string]::IsNullOrEmpty($LedgerOut)) { $argv += @('--ledger-out', $LedgerOut) }
  $raw = & python @argv 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function Invoke-SceneTraceDiff {
  param([Parameter(Mandatory = $true)][string]$A, [Parameter(Mandatory = $true)][string]$B,
        [int]$Seed = 42, [string]$Scenario = '', [Parameter(Mandatory = $true)][ref]$Out)
  $argv = @('-I', $TraceDiff, 'diff', $A, $B, '--a-side', 'csharp', '--b-side', 'rust', '--seed', [string]$Seed)
  if (-not [string]::IsNullOrEmpty($Scenario)) { $argv += @('--scenario', $Scenario) }
  $raw = & python @argv 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

# 委托既有 runner（`-GateOnly` / `-SceneGroupDiff`）—— **不复制**它的门→比逻辑。
function Invoke-PeerRunner {
  param([Parameter(Mandatory = $true)][string[]]$PeerArgs, [Parameter(Mandatory = $true)][ref]$Out)
  $raw = & powershell -NoProfile -ExecutionPolicy Bypass -File $PeerRunner @PeerArgs 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function New-SceneStream([string]$Path, [string[]]$Sites, [int]$InjectSeed = 42, [switch]$NoInject) {
  # 注意：PowerShell 逗号优先级高于 `+` ⇒ 逐行显式构造，绝不写 `@('a'+x, y)`（会被拼成空格分隔单串）。
  $lines = @()
  if (-not $NoInject) {
    $lines += ("1`tINJECT_OK`t{0}`t0" -f $InjectSeed)
    $lines += ("SEED_PROBE {0}" -f $InjectSeed)
  }
  $seq = 1
  foreach ($s in $Sites) {
    if ($s -eq '') { $lines += (@($seq, 'Next(max)', '100', '7') -join "`t") }  # 缺 site 列（4 列）
    else { $lines += (@($seq, 'Next(max)', '100', '7', $s) -join "`t") }
    $seq++
  }
  Write-M3LfLines -Path $Path -Lines $lines
}

function New-ScenePair([string]$Dir, [string]$Map, [int]$Seed, [string]$MonsterName,
                       [int]$MonX, [int]$MonY, [int]$MonDir, [int]$MonCount,
                       [string]$ExtraMonsterName = '') {
  $fixture = Join-Path $Dir 'mongen-map0137.tsv'
  Write-M3LfLines -Path $fixture -Lines @('# synthetic zero-row MonGen fixture')
  $mon = '{"name":"' + $MonsterName + '","x":' + $MonX + ',"y":' + $MonY + ',"dir":' + $MonDir + ',"count":' + $MonCount + '}'
  $descA = Join-Path $Dir 'csharp-scene.json'
  $descB = Join-Path $Dir 'rust-scene.json'
  Write-M3LfText -Path $descA -Text (@(
    '{', '  "canon": "m3-rng-scene-descriptor-v1", "side": "csharp",',
    ('  "map": "' + $Map + '", "seed": ' + $Seed + ','),
    '  "start_pose": {"x": 17, "y": 20, "dir": 2},',
    '  "mongen_rows": [],',
    ('  "monsters": [' + $mon + ']'), '}') -join "`n")
  $monB = $mon
  if (-not [string]::IsNullOrEmpty($ExtraMonsterName)) {
    $monB = $mon + ',{"name":"' + $ExtraMonsterName + '","x":19,"y":20,"dir":0,"count":1}'
  }
  Write-M3LfText -Path $descB -Text (@(
    '{', '  "canon": "m3-rng-scene-descriptor-v1", "side": "rust",',
    ('  "map": "' + $Map + '", "seed": ' + $Seed + ','),
    '  "start_pose": {"x": 17, "y": 20, "dir": 2},',
    '  "mongen_rows": [],',
    ('  "monsters": [' + $monB + ']'), '}') -join "`n")
  return [pscustomobject]@{ A = $descA; B = $descB; Fixture = $fixture }
}

# 站点常量（与 rng-trace-probe/consumer_groups.tsv 的 pattern 同源）。
$Script:SiteCtor = 'M2Server.Actor.BaseObject..ctor@BaseObject.cs:438'
$Script:SiteSpawnC = 'GameSrv.Word.WorldServer.CreateMonster@WorldServer.MonGen.cs:498'
$Script:SiteAiC = 'M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427'
$Script:SiteSpawnR = 'crates\world\src\mongen.rs:624'
$Script:SiteAiR = 'crates\world\src\monai.rs:164'
$Script:SiteDropC = 'GameSrv.Word.WorldServer.MonGetRandomItems@WorldServer.MonGen.cs:451'
$Script:SiteDropR = 'crates\formula\src\drop.rs:109'

# 场景件里 `trace.runner` 引用的 PowerShell 参数名，必须都是**本脚本已声明**的参数（M-1 同族）。
function Test-SceneRunnerParams {
  param([Parameter(Mandatory = $true)][string]$Path)
  $res = [ordered]@{ ok = $false; tokens = @(); missing = @(); detail = '' }
  if (-not (Test-Path -LiteralPath $Path)) { $res.detail = "scenario 不存在: $Path"; return [pscustomobject]$res }
  $sc = $null
  try { $sc = (Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json) }
  catch { $res.detail = "scenario 解析失败: $($_.Exception.Message)"; return [pscustomobject]$res }
  $runnerTexts = @()
  foreach ($v in @($sc.runner, $sc.trace.runner, $sc.artifact.runner)) {
    if ($v -is [string] -and -not [string]::IsNullOrEmpty($v)) { $runnerTexts += $v }
  }
  if ($runnerTexts.Count -eq 0) {
    $res.detail = 'scenario 未声明 runner（机判无从下手 ⇒ 判红，不许静默绿）'
    return [pscustomobject]$res
  }
  $tokens = @()
  foreach ($t in $runnerTexts) {
    foreach ($m in [regex]::Matches($t, '(?<![A-Za-z0-9_])-([A-Za-z][A-Za-z0-9_]*)')) { $tokens += $m.Groups[1].Value }
  }
  $tokens = @($tokens | Sort-Object -Unique)
  $declared = @($script:SceneDeclaredParams)
  $missing = @($tokens | Where-Object { $declared -notcontains $_ })
  $res.tokens = $tokens
  $res.missing = $missing
  $res.ok = ($tokens.Count -ge 1) -and ($missing.Count -eq 0)
  $res.detail = ("tokens=[{0}] missing=[{1}]" -f ($tokens -join ','), ($missing -join ','))
  return [pscustomobject]$res
}

# scenario 与「场景真源」逐字段相等（防两个 scenario 漂移；map 是字符串，不做数值化）。
function Test-SceneSourceConsistent {
  param([Parameter(Mandatory = $true)][string]$SceneSourcePath)
  $drv = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)
  $src = (Get-Content -LiteralPath $SceneSourcePath -Raw | ConvertFrom-Json)
  $ok = ($drv.map -eq $src.map) -and ([int]$drv.seed -eq [int]$src.seed) -and
        ([int]$drv.start_pose.x -eq [int]$src.start_pose.x) -and
        ([int]$drv.start_pose.y -eq [int]$src.start_pose.y) -and
        ([int]$drv.start_pose.dir -eq [int]$src.start_pose.dir) -and
        ([int]$drv.ticks -eq [int]$src.trace.ticks)
  $monDrv = (@($drv.monsters) | ForEach-Object { '{0}:{1},{2},{3},{4}' -f $_.name, $_.x, $_.y, $_.dir, $_.count }) -join '|'
  $monSrc = (@($src.monsters) | ForEach-Object { '{0}:{1},{2},{3},{4}' -f $_.name, $_.x, $_.y, $_.dir, $_.count }) -join '|'
  $ok = $ok -and ($monDrv -eq $monSrc)
  return [pscustomobject]@{ ok = $ok; detail = ("map={0}/{1} seed={2}/{3} ticks={4}/{5}" -f $drv.map, $src.map, $drv.seed, $src.seed, $drv.ticks, $src.trace.ticks) }
}

# =========================================================================== #
# Mode: SelfCheckOut —— 把无栈自检转录**由脚本自己**以 LF 落盘（避免 `> file` 的 CRLF 重定向）
# =========================================================================== #
if ($SelfCheckNoStack -and -not [string]::IsNullOrEmpty($SelfCheckOut)) {
  $selfTmp = Join-Path $env:TEMP ('rngscene-selfcheck-' + [guid]::NewGuid().ToString('N') + '.txt')
  $childArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $PSCommandPath, '-SelfCheckNoStack')
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
  Write-Output 'SELFTEST_NO_STACK: 无栈自检——严格账本校验 + 场景门 + 组账本，全路径都必须 fail-closed（缺失绝不静默 GREEN）'
  $script:ScenePass = 0
  $script:SceneFail = @()
  $tmpRoot = Join-Path $env:TEMP ('rngscene-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  function Add-Result([string]$Name, [bool]$Ok, [string]$Detail) {
    $suffix = if ([string]::IsNullOrEmpty($Detail)) { '' } else { " ($Detail)" }
    if ($Ok) { $script:ScenePass++; Write-Output ("SELFTEST ok {0}{1}" -f $Name, $suffix) }
    else { $script:SceneFail += $Name; Write-Output ("SELFTEST FAIL {0}{1}" -f $Name, $suffix) }
  }

  try {
    # ① driver scenario 固定输入
    $sc = $null
    try { $sc = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json) } catch { }
    $scOk = ($null -ne $sc) -and ($sc.canon -eq 'm3-rng-scene-driver-v1') -and
            ([int]$sc.seed -eq 42) -and ($sc.map -eq '0137') -and ([int]$sc.ticks -eq 600) -and
            ([int]$sc.start_pose.x -eq 17) -and ([int]$sc.start_pose.y -eq 20) -and ([int]$sc.start_pose.dir -eq 2) -and
            ($null -ne $sc.monsters) -and ($sc.monsters.Count -eq 1) -and ([int]$sc.monsters[0].x -eq 18) -and
            ($sc.trace.fold.construct -eq 'spawn') -and ($null -ne $sc.env.csharp) -and ($null -ne $sc.env.rust)
    Add-Result 'scenario-fixed-input' $scOk 'canon/seed/map/ticks/pose/monsters/fold/env'

    # ② driver scenario 与「场景真源」（rng-trace-scene scenario）逐字段一致 ⇒ 无第二真源漂移。
    $srcPath = Join-Path (Split-Path -Parent $Here) ($sc.scene_source.path -replace '^\.\./', '')
    if (-not (Test-Path -LiteralPath $srcPath)) { $srcPath = Join-Path $M3 'rng-trace-scene/rng_trace_scenario.json' }
    $srcCons = Test-SceneSourceConsistent -SceneSourcePath $srcPath
    Add-Result 'scene-source-consistent' $srcCons.ok $srcCons.detail
    # 改坏必红：把 driver scenario 的 map 改成别的图 ⇒ 一致性牙必红（样本由 JSON 字段改写构造）。
    $badSrc = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)
    $badSrc.map = '0000'
    $badSrcPath = Join-Path $tmpRoot 'driver-scenario-drift.json'
    Write-M3LfText -Path $badSrcPath -Text ($badSrc | ConvertTo-Json -Depth 20)
    $savedScenario = $ScenarioFile
    $ScenarioFile = $badSrcPath
    $srcConsBad = Test-SceneSourceConsistent -SceneSourcePath $srcPath
    $ScenarioFile = $savedScenario
    Add-Result 'scene-source-drift-red' (-not $srcConsBad.ok) $srcConsBad.detail

    # ③ scenario runner 参数名必须已声明（M-1 同族：doc-vs-code 漂移）。
    $rp = Test-SceneRunnerParams -Path $ScenarioFile
    Add-Result 'scenario-runner-params-declared' $rp.ok $rp.detail
    $badRun = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)
    $badRun.runner = ($badRun.runner + ' -NotARealParam')
    $badRunPath = Join-Path $tmpRoot 'scenario-runner-drift.json'
    Write-M3LfText -Path $badRunPath -Text ($badRun | ConvertTo-Json -Depth 20)
    $rpBad = Test-SceneRunnerParams -Path $badRunPath
    Add-Result 'scenario-runner-param-drift-red' ((-not $rpBad.ok) -and ($rpBad.missing -contains 'NotARealParam')) `
      ("missing=" + ($rpBad.missing -join ','))

    # ④ 严格账本校验器 selftest（断号/重复/头/形状/site/折叠次序；含改坏必红）
    $tdOut = ''
    $tdRaw = & python -I $TraceDiff selftest 2>&1
    $tdOut = ($tdRaw | Out-String)
    $tdRc = [int]$LASTEXITCODE
    $tdMatch = [regex]::Match($tdOut, 'SELFTEST GREEN: rng_trace_diff selftest (\d+)/(\d+)')
    Add-Result 'trace-diff-selftest' (($tdRc -eq 0) -and $tdMatch.Success -and
      ($tdMatch.Groups[1].Value -eq $tdMatch.Groups[2].Value)) ("rc=$tdRc")

    # ⑤ 两侧流水严格校验 GREEN（合成场景：同一固定怪 + 等价组账本）。
    $dirGood = Join-Path $tmpRoot 'good'; $dirBad = Join-Path $tmpRoot 'bad'
    New-Item -ItemType Directory -Force -Path $dirGood, $dirBad | Out-Null
    $pairGood = New-ScenePair -Dir $dirGood -Map '0137' -Seed 42 -MonsterName '稻草人' -MonX 18 -MonY 20 -MonDir 0 -MonCount 1
    $pairBad = New-ScenePair -Dir $dirBad -Map '0137' -Seed 42 -MonsterName '稻草人' -MonX 18 -MonY 20 -MonDir 0 -MonCount 1 -ExtraMonsterName '鸡'
    $sA = Join-Path $tmpRoot 'a.tsv'; $sB = Join-Path $tmpRoot 'b.tsv'
    New-SceneStream -Path $sA -Sites @($Script:SiteCtor, $Script:SiteSpawnC, $Script:SiteAiC, $Script:SiteDropC)
    New-SceneStream -Path $sB -Sites @($Script:SiteSpawnR, $Script:SiteSpawnR, $Script:SiteAiR, $Script:SiteDropR)
    $cA = ''; $rcA = Invoke-SceneTraceCheck -Stream $sA -Side csharp -Seed 42 -Out ([ref]$cA)
    $cB = ''; $rcB = Invoke-SceneTraceCheck -Stream $sB -Side rust -Seed 42 -Out ([ref]$cB)
    Add-Result 'trace-check-green-both' (($rcA -eq 0) -and ($rcB -eq 0)) ("rcA=$rcA rcB=$rcB")

    # ⑥ 断号必红（seq 跳号）；缺件 fail-closed（PREREQ）。
    $sGap = Join-Path $tmpRoot 'gap.tsv'
    # 注意：PowerShell 里逗号优先级高于 `+` ⇒ 逐行显式构造（`@('a'+x, y)` 会被拼成空格分隔的单串）。
    $gLines = @()
    $gLines += ("1`tINJECT_OK`t{0}`t0" -f 42)
    $gLines += ("1`tNext(max)`t100`t7`t{0}" -f $Script:SiteSpawnR)
    $gLines += ("3`tNext(max)`t100`t7`t{0}" -f $Script:SiteAiR)
    Write-M3LfLines -Path $sGap -Lines $gLines
    $cg = ''; $rcGap = Invoke-SceneTraceCheck -Stream $sGap -Side rust -Seed 42 -Out ([ref]$cg)
    Add-Result 'trace-check-seq-gap-red' (($rcGap -eq 1) -and ($cg -match 'seq-gap')) ("rc=$rcGap")
    $cm = ''; $rcMiss = Invoke-SceneTraceCheck -Stream (Join-Path $tmpRoot 'nope.tsv') -Side rust -Seed 42 -Out ([ref]$cm)
    Add-Result 'trace-check-missing-prereq' (($rcMiss -eq 2) -and ($cm -match 'stream-missing')) ("rc=$rcMiss")
    # 头异种子必红（描述件自报 42 不算证据）。
    $sSeed43 = Join-Path $tmpRoot 'seed43.tsv'
    New-SceneStream -Path $sSeed43 -Sites @($Script:SiteSpawnR) -InjectSeed 43
    $cs2 = ''; $rcSeed = Invoke-SceneTraceCheck -Stream $sSeed43 -Side rust -Seed 42 -Out ([ref]$cs2)
    Add-Result 'trace-check-seed-red' (($rcSeed -eq 1) -and ($cs2 -match 'header-seed-mismatch')) ("rc=$rcSeed")

    # ⑦ 场景一致性门（委托既有 runner）：同场景绿 / 怪集合漂移红。
    $gOut = ''
    $rcGate = Invoke-PeerRunner -PeerArgs @('-GateOnly', '-AScene', $pairGood.A, '-BScene', $pairGood.B,
      '-AStream', $sA, '-BStream', $sB) -Out ([ref]$gOut)
    Add-Result 'gate-identical-green' (($rcGate -eq 0) -and ($gOut -match 'VERDICT GREEN')) ("rc=$rcGate")
    $gOut2 = ''
    $rcGateBad = Invoke-PeerRunner -PeerArgs @('-GateOnly', '-AScene', $pairBad.A, '-BScene', $pairBad.B,
      '-AStream', $sA, '-BStream', $sB) -Out ([ref]$gOut2)
    Add-Result 'gate-monster-drift-red' (($rcGateBad -eq 1) -and ($gOut2 -match 'monsters-equal:a-vs-b')) ("rc=$rcGateBad")

    # ⑧ 本驱动器的严格校验器与既有比较器**串联**（单一入口）：同序 ⇒ IDENTICAL。
    $sdOut = ''
    $rcSd = Invoke-SceneTraceDiff -A $sA -B $sB -Seed 42 -Out ([ref]$sdOut)
    Add-Result 'trace-diff-identical-green' (($rcSd -eq 0) -and ($sdOut -match 'VERDICT IDENTICAL')) ("rc=$rcSd")
    # 反向对照：B 侧断号 ⇒ 严格校验器必须 **REFUSED**（不调用比较器、无组结论行）。
    $sdOut2 = ''
    $rcSd2 = Invoke-SceneTraceDiff -A $sA -B $sGap -Seed 42 -Out ([ref]$sdOut2)
    Add-Result 'trace-diff-refuses-shape-red' (($rcSd2 -eq 1) -and ($sdOut2 -match 'REFUSED') -and
      ($sdOut2 -notmatch 'RNG_CONSUMER_GROUPDIFF')) ("rc=$rcSd2")

    # ⑨ DoD① 单一入口（委托既有 runner `-SceneGroupDiff`）：门绿 + 账本等价 ⇒ IDENTICAL rc=0。
    $d1Out = ''
    $rcD1 = Invoke-PeerRunner -PeerArgs @('-SceneGroupDiff', '-AScene', $pairGood.A, '-BScene', $pairGood.B,
      '-AStream', $sA, '-BStream', $sB) -Out ([ref]$d1Out)
    Add-Result 'scene-groupdiff-identical-green' (($rcD1 -eq 0) -and ($d1Out -match 'GREEN\(scene\)') -and
      ($d1Out -match 'VERDICT IDENTICAL')) ("rc=$rcD1")
    # 改坏必红：场景漂移 ⇒ 整链拒比（无任何 RNG_CONSUMER_GROUPDIFF 结论行）。
    $d2Out = ''
    $rcD2 = Invoke-PeerRunner -PeerArgs @('-SceneGroupDiff', '-AScene', $pairBad.A, '-BScene', $pairBad.B,
      '-AStream', $sA, '-BStream', $sB) -Out ([ref]$d2Out)
    Add-Result 'scene-groupdiff-scene-drift-refused' (($rcD2 -eq 1) -and ($d2Out -match 'REFUSED') -and
      ($d2Out -notmatch 'RNG_CONSUMER_GROUPDIFF')) ("rc=$rcD2")

    # ⑩ 委托件在位（既有 runner / scenario 必须存在，否则本驱动器无从委托 ⇒ fail-closed）。
    Add-Result 'peer-runner-present' ((Test-Path -LiteralPath $PeerRunner) -and (Test-Path -LiteralPath $PeerScenario)) `
      ("runner=" + (Test-Path -LiteralPath $PeerRunner))

    # ⑪ real-run fail-closed（三条：未给 -Go / 给 -Go 未给 -PinScene / 给两者但驱动未接线）。
    $noGoRaw = & powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath 2>&1
    $noGoRc = $LASTEXITCODE
    Add-Result 'real-run-no-go-fail-closed' (($noGoRc -eq 2) -and (($noGoRaw | Out-String) -match 'PREREQ_MISSING')) `
      ("rc=$noGoRc")
    $goNoPinRaw = & powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -Go 2>&1
    $goNoPinRc = $LASTEXITCODE
    Add-Result 'real-run-go-no-pinscene-fail-closed' (($goNoPinRc -eq 2) -and (($goNoPinRaw | Out-String) -match 'PREREQ_MISSING')) `
      ("rc=$goNoPinRc")
    $goPinRaw = & powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -Go -PinScene 2>&1
    $goPinRc = $LASTEXITCODE
    Add-Result 'real-run-unwired-fail-closed' (($goPinRc -eq 2) -and (($goPinRaw | Out-String) -match 'PREREQ_MISSING')) `
      ("rc=$goPinRc")

    # ⑫ BotSrv 生产函数 selftest（T-59：固定动作序列解析 / 固定怪 key / 场景描述件行组装；
    #    默认关）—— 改坏必红由代码 review + 本断言钉住（片内红检另存证据）。
    $repoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..\..')).Path
    $botProj = Join-Path $repoRoot 'src\BotSrv\BotSrv.csproj'
    $botExe = Join-Path $repoRoot 'src\BotSrv\bin\Release\BotSrv.exe'
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $botBuildOut = (& dotnet build $botProj -c Release --nologo -v q 2>&1 | Out-String)
    $botBuildRc = $LASTEXITCODE
    if ($botBuildRc -ne 0) {
      Write-Output $botBuildOut
      Add-Result 'botsrv-sceneseq-selftest' $false ("build_rc=$botBuildRc")
    }
    else {
      $seqOut = (& $botExe --selftest-sceneseq 2>&1 | Out-String)
      $seqRc = $LASTEXITCODE
      if ($seqRc -ne 0) { Write-Output $seqOut }
      Add-Result 'botsrv-sceneseq-selftest' (($seqRc -eq 0) -and ($seqOut -match 'SCENESEQ SELFTEST GREEN')) ("rc=$seqRc")
    }
    $ErrorActionPreference = $prevEap

    # ⑬ LF 写出（生产者侧根因；EOL 陷阱同族）。
    $lf = Test-M3LfWriterSelfTest
    Add-Result 'lf-writer-selftest' ($lf.ok) ($lf.reason)

    # ⑭ 本脚本从未触碰端口（自检/门/比较全离线）。
    Add-Result 'no-port-touched' (-not $script:SceneTouchedPorts) 'offline-only'
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:ScenePass + $script:SceneFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:ScenePass, $total, ($script:SceneFail -join ','))
  if ($script:SceneFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（严格账本校验器 rng_trace_diff：头种子 / seq 1..N 连续 / 5 列 / site 非空，断号·缺件·异种子 fail-closed + 与既有 groupdiff 串联拒比 + 委托既有场景门/DoD① 单一入口 + scenario 与真源一致 + runner 参数名已声明 + real-run fail-closed + LF 写出 + 零端口）'
  exit 0
}

# =========================================================================== #
# Mode: TraceCheckOnly —— 只严格校验一侧流水形状（离线，不需要栈）
# =========================================================================== #
if ($TraceCheckOnly) {
  if ([string]::IsNullOrEmpty($AStream)) {
    Write-Output 'PREREQ_MISSING: -TraceCheckOnly 需要 -AStream'
    exit 2
  }
  $out = ''
  $rc = Invoke-SceneTraceCheck -Stream $AStream -Side $Side -Seed $Seed -Out ([ref]$out)
  Write-Output $out
  exit $rc
}

# =========================================================================== #
# Mode: GateOnly —— 委托既有 runner 的场景一致性前置门（离线）
# =========================================================================== #
if ($GateOnly) {
  if ([string]::IsNullOrEmpty($AScene) -or [string]::IsNullOrEmpty($BScene)) {
    Write-Output 'PREREQ_MISSING: -GateOnly 需要 -AScene 与 -BScene'
    exit 2
  }
  $pa = @('-GateOnly', '-AScene', $AScene, '-BScene', $BScene)
  if (-not [string]::IsNullOrEmpty($AStream)) { $pa += @('-AStream', $AStream) }
  if (-not [string]::IsNullOrEmpty($BStream)) { $pa += @('-BStream', $BStream) }
  $out = ''
  $rc = Invoke-PeerRunner -PeerArgs $pa -Out ([ref]$out)
  Write-Output $out
  exit $rc
}

# =========================================================================== #
# Mode: SceneGroupDiff —— 委托既有 runner 的 DoD① 单一入口（门非 GREEN ⇒ 拒比）
# =========================================================================== #
if ($SceneGroupDiff) {
  $missing = @()
  foreach ($n in @('AScene', 'BScene', 'AStream', 'BStream')) {
    if ([string]::IsNullOrEmpty((Get-Variable -Name $n -ValueOnly))) { $missing += ('-' + $n) }
  }
  if ($missing.Count -gt 0) {
    Write-Output ('PREREQ_MISSING: -SceneGroupDiff 需要 -AScene/-BScene/-AStream/-BStream（缺 ' + ($missing -join ',') + '）')
    exit 2
  }
  $out = ''
  $rc = Invoke-PeerRunner -PeerArgs @('-SceneGroupDiff', '-AScene', $AScene, '-BScene', $BScene,
    '-AStream', $AStream, '-BStream', $BStream) -Out ([ref]$out)
  Write-Output $out
  exit $rc
}

# =========================================================================== #
# Mode: real run（真机两相 trace）—— 未获 owner 窗口（-Go）前 fail-closed 拒跑
# =========================================================================== #
if (-not $Go) {
  Write-Output 'PREREQ_MISSING: 真机两相 trace 需 owner 发放窗口（起栈禁接管道；未获 -Go 一律拒跑，不占端口）'
  Write-Output "NEXT: 获窗口后 ① start-csharp-hook.ps1 -Seed $Seed -LogDir <会话>（写 csharp-rng-stream.tsv）→ 固定场景（-SceneSequence）→ dump csharp-scene.json ② start_rust_stack.ps1 -RngLog（写 rust-rng-stream.tsv）→ 同场景 → dump rust-scene.json ③ run_rng_scene_pair.ps1 -SceneGroupDiff（门非 GREEN 拒比；GREEN 才 groupdiff --fold --strict）"
  exit 2
}
$rpRun = Test-SceneRunnerParams -Path $ScenarioFile
if (-not $rpRun.ok) {
  Write-Output ("PREREQ_MISSING: scenario runner 参数名漂移（$($rpRun.detail)）—— 先让文档与脚本对齐再起真机")
  exit 2
}
if (-not $PinScene) {
  Write-Output 'PREREQ_MISSING: 真机两相 trace 需显式 -PinScene（按 scenario 钉住含怪场景；只给 -Go 不钉场景一律拒跑）'
  exit 2
}
Write-Output 'PREREQ_MISSING: 两侧「场景描述件 + 全场景抽数流水」驱动尚未接线（承接卡见 rng-trace-scene/DRIVER-CONTRACT.md §7）'
Write-Output ("PLANNED: -Side {0} -Seed {1} -Prefix {2} -PeerSession {3}" -f $Side, $Seed, $Prefix, $PeerSession)
exit 2
