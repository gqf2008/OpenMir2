#requires -Version 5
<#
  run_rng_trace_pair.ps1 —— M3 RNG 站点组「同场景 / 同起点 / 同种子」两相 trace 夹具骨架
  （卡 `openmir2-m3-rng-trace-scene-groups`，issue d9d03f47）。

  产物口径：两侧各自跑「固定图 + 固定怪集 + 固定起点 + 同种子」的全场景抽数流水
  （5 列 TSV：`seq \t kind \t max \t result \t site`）+ 一份场景描述件 `<side>-scene.json`，
  再由
    ① `rng_scene_gate.py gate` 做**场景一致性前置门**（map/seed/start_pose + MonGen 冻结件 +
       Monster 集合两侧同口径）；不满足 ⇒ fail-closed 拒比；
    ② `rng_ledger_diff.py groupdiff --fold --strict` 做组集合 + 折叠计数 + 共享组相对次序 diff
       （R-6：C# `construct` 折叠到 Rust `spawn`）。

  本片交付的是**夹具骨架**（不占窗口）：门 + 比较器 + `-SelfCheckNoStack` 已完整、可离线复跑；
  真机两相 trace 需要窗口（起栈禁接管道），故 real-run 分支 **fail-closed 拒跑**（exit 2），
  绝不静默起栈。owner 会把它与 ④ 真机 100 杀编排在**同一窗口批次**（串行）。
  真机命令里的每个参数名（`-Go`/`-Side`/`-PinScene`/`-PeerSession`）都是**本脚本已声明**的参数，
  且由无栈自检 `scenario-runner-params-declared` 对 scenario JSON 的 `trace.runner` 做机判（M-1）。

  用法：
    # 离线自检（门 + 账本 + folded 映射全跑一遍；零 .rs、不占端口）
    powershell -ExecutionPolicy Bypass -File run_rng_trace_pair.ps1 -SelfCheckNoStack
    powershell -ExecutionPolicy Bypass -File run_rng_trace_pair.ps1 -SelfCheckNoStack -SelfCheckOut <path.txt>
    # 只跑场景一致性前置门（离线；两份场景描述件）
    powershell -ExecutionPolicy Bypass -File run_rng_trace_pair.ps1 -GateOnly -AScene <csharp-scene.json> -BScene <rust-scene.json>
    # 只跑两侧组账本 diff（离线；两份抽数流水）
    powershell -ExecutionPolicy Bypass -File run_rng_trace_pair.ps1 -GroupDiffOnly -AStream <csharp.tsv> -BStream <rust.tsv>
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
  [switch]$GateOnly,
  [switch]$GroupDiffOnly,
  [switch]$Go,                     # owner 发放窗口的显式授权；未给 ⇒ real-run fail-closed（exit 2）
  # 真机两相 trace 必须**显式**请求「按 scenario 钉住含怪场景」（承接卡接线后生效）。
  # 只给 -Go 不给 -PinScene ⇒ 仍 fail-closed（防止误跑未钉场景）。
  # 该参数名同时被 scenario JSON 的 trace.runner 引用；两者由机判「scenario-runner-params-declared」对齐（M-1）。
  [switch]$PinScene,
  [switch]$SelfCheckNoStack,
  [string]$SelfCheckOut = ''
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
# 本脚本**已声明**的参数名（供「scenario runner 参数名必须已声明」机判用；见 Test-RngScenarioRunnerParams）。
$script:RngDeclaredParams = @($MyInvocation.MyCommand.Parameters.Keys)
$SceneGate = Join-Path $Here 'rng_scene_gate.py'
$LedgerDiff = Join-Path (Split-Path -Parent $Here) 'rng-trace-probe/rng_ledger_diff.py'
. (Join-Path (Split-Path -Parent $Here) 'm3hashutil.ps1')

if ([string]::IsNullOrEmpty($ScenarioFile)) { $ScenarioFile = Join-Path $Here 'rng_trace_scenario.json' }

$Ports = @($PortsCsv.Split(',') | ForEach-Object { [int]$_.Trim() })
$script:RngSceneTouchedPorts = $false   # 本脚本（含自检）**从不**绑定/连接任何端口。

# 返回 [int]$LASTEXITCODE；子进程 stdout 走 -Out 引用，**不混进返回值**
# （历史 bug：`$rc = & python …` 会拿到 @(<stdout>, $LASTEXITCODE) ⇒ `-ne 0` 恒真）。
function Invoke-RngSceneGate {
  param([Parameter(Mandatory = $true)][string]$A, [Parameter(Mandatory = $true)][string]$B,
        [string]$Scenario = '', [string]$Json = '', [Parameter(Mandatory = $true)][ref]$Out)
  if ([string]::IsNullOrEmpty($Scenario)) { $Scenario = $ScenarioFile }
  $argv = @('-I', $SceneGate, 'gate', '--scenario', $Scenario, '--a', $A, '--b', $B)
  if (-not [string]::IsNullOrEmpty($Json)) { $argv += @('--json', $Json) }
  $raw = & python @argv 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function Invoke-RngSceneScenario {
  param([Parameter(Mandatory = $true)][string]$Scenario, [Parameter(Mandatory = $true)][ref]$Out)
  $raw = & python -I $SceneGate scenario --scenario $Scenario 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function Invoke-RngGroupDiff {
  param([Parameter(Mandatory = $true)][string]$A, [Parameter(Mandatory = $true)][string]$B,
        [Parameter(Mandatory = $true)][ref]$Out)
  $raw = & python -I $LedgerDiff groupdiff $A $B --a-side csharp --b-side rust --fold --strict 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function Invoke-RngLedgerSelfTest {
  param([Parameter(Mandatory = $true)][ref]$Out)
  $raw = & python -I $LedgerDiff selftest 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function New-RngStream([string]$Path, [string[]]$Sites) {
  $lines = @('1' + "`t" + 'INJECT_OK' + "`t" + '42' + "`t" + '0', 'SEED_PROBE 42')
  $seq = 2
  foreach ($s in $Sites) {
    $lines += (@($seq, 'Next(max)', '100', '7', $s) -join "`t")
    $seq++
  }
  Write-M3LfLines -Path $Path -Lines $lines
}

function New-RngSceneFiles([string]$Dir, [string]$FixtureName, [string]$Map, [int]$Seed, [string]$MonsterName,
                           [int]$MonX, [int]$MonY, [int]$MonDir, [int]$MonCount,
                           [string]$ExtraMonsterName = '') {
  $fixture = Join-Path $Dir $FixtureName
  Write-M3LfLines -Path $fixture -Lines @('# synthetic zero-row MonGen fixture')
  $scen = Join-Path $Dir 'scenario.json'
  $mon = '{"name":"' + $MonsterName + '","x":' + $MonX + ',"y":' + $MonY + ',"dir":' + $MonDir + ',"count":' + $MonCount + '}'
  Write-M3LfText -Path $scen -Text (@(
    '{',
    '  "canon": "m3-rng-trace-scene-v1",',
    ('  "seed": ' + $Seed + ','),
    ('  "map": "' + $Map + '",'),
    '  "start_pose": {"x": 17, "y": 20, "dir": 2},',
    ('  "monsters": [' + $mon + '],'),
    ('  "mongen_fixture": "' + $FixtureName + '",'),
    '  "trace": { "fold": { "construct": "spawn" }, "groups_expected": ["monster_ai", "spawn", "drop"] }',
    '}') -join "`n")
  $descA = Join-Path $Dir 'a-scene.json'
  $descB = Join-Path $Dir 'b-scene.json'
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
  return [pscustomobject]@{ Scenario = $scen; A = $descA; B = $descB }
}

# 站点常量（与 rng-trace-probe/consumer_groups.tsv 的 pattern 同源）。
$Script:SiteCtor = 'M2Server.Actor.BaseObject..ctor@BaseObject.cs:438'
$Script:SiteSpawnC = 'GameSrv.Word.WorldServer.CreateMonster@WorldServer.MonGen.cs:498'
$Script:SiteAiC = 'M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427'
$Script:SiteSpawnR = 'crates\world\src\mongen.rs:624'
$Script:SiteAiR = 'crates\world\src\monai.rs:164'

# 从 scenario JSON **解析成对象**、做一次字段级改写后另存 —— 构造 drift 样本时不再对原始文本做
# 正则替换（M-1 sibling 收口：原实现用 `-replace '"construct"\s*:\s*"spawn"'` 之类匹配 JSON
# 字面拼写，scenario 一旦重排格式 / 换引号风格，替换会空转 ⇒ drift-red 用例自曝 RED（fail-loud），
# 掩盖「drift 构造本身失效」）。用 ConvertFrom-Json 定位字段 ⇒ 与文本格式无关。
function New-RngDriftScenario {
  param([Parameter(Mandatory = $true)][string]$Source, [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][scriptblock]$Mutate)
  $sc = (Get-Content -LiteralPath $Source -Raw | ConvertFrom-Json)
  & $Mutate $sc
  $json = $sc | ConvertTo-Json -Depth 20
  Write-M3LfText -Path $Path -Text $json
  return $Path
}

# 场景件里 `trace.runner` / `artifact.runner` 引用的 PowerShell 参数名，必须都是**本脚本已声明**的参数
# （M-1：`-PinScene` 曾只写在 scenario JSON、脚本未声明 ⇒ 照文档字面执行会在参数绑定阶段就报错，
#  永远到不了 -Go 的 fail-closed 分支）。返回 @{ ok; tokens; missing; detail }，不抛异常、不写 stdout。
function Test-RngScenarioRunnerParams {
  param([Parameter(Mandatory = $true)][string]$Path)
  $res = [ordered]@{ ok = $false; tokens = @(); missing = @(); detail = '' }
  if (-not (Test-Path -LiteralPath $Path)) { $res.detail = "scenario 不存在: $Path"; return [pscustomobject]$res }
  $sc = $null
  try { $sc = (Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json) }
  catch { $res.detail = "scenario 解析失败: $($_.Exception.Message)"; return [pscustomobject]$res }
  $runnerTexts = @()
  foreach ($v in @($sc.trace.runner, $sc.artifact.runner)) {
    if ($v -is [string] -and -not [string]::IsNullOrEmpty($v)) { $runnerTexts += $v }
  }
  if ($runnerTexts.Count -eq 0) {
    $res.detail = 'scenario 未声明 trace.runner/artifact.runner（机判无从下手 ⇒ 判红，不许静默绿）'
    return [pscustomobject]$res
  }
  $tokens = @()
  foreach ($t in $runnerTexts) {
    # 只取「前面不是标识符字符」的单横线参数名 ⇒ 不会把路径里的 `rng-trace-scene` 错读成 `-trace`/`-scene`。
    foreach ($m in [regex]::Matches($t, '(?<![A-Za-z0-9_])-([A-Za-z][A-Za-z0-9_]*)')) { $tokens += $m.Groups[1].Value }
  }
  $tokens = @($tokens | Sort-Object -Unique)
  $declared = @($script:RngDeclaredParams)
  $missing = @($tokens | Where-Object { $declared -notcontains $_ })
  $res.tokens = $tokens
  $res.missing = $missing
  # 非空（否则机判空转）+ 无缺项 ⇒ 绿。
  $res.ok = ($tokens.Count -ge 1) -and ($missing.Count -eq 0)
  $res.detail = ("tokens=[{0}] missing=[{1}]" -f ($tokens -join ','), ($missing -join ','))
  return [pscustomobject]$res
}

# =========================================================================== #
# Mode: SelfCheckOut —— 把无栈自检转录**由脚本自己**以 LF 落盘
# =========================================================================== #
if ($SelfCheckNoStack -and -not [string]::IsNullOrEmpty($SelfCheckOut)) {
  $selfTmp = Join-Path $env:TEMP ('rngtrace-selfcheck-' + [guid]::NewGuid().ToString('N') + '.txt')
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
  Write-Output 'SELFTEST_NO_STACK: 无栈自检——场景门 + 账本 + folded 映射全路径都必须 fail-closed（缺失绝不静默 GREEN）'
  $script:RngPass = 0
  $script:RngFail = @()
  $tmpRoot = Join-Path $env:TEMP ('rngtrace-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  # 只打印 + 记账，**不返回值**（否则 `if (Check ...)` 会被函数输出的字符串污染成恒真）。
  function Add-Result([string]$Name, [bool]$Ok, [string]$Detail) {
    $suffix = if ([string]::IsNullOrEmpty($Detail)) { '' } else { " ($Detail)" }
    if ($Ok) { $script:RngPass++; Write-Output ("SELFTEST ok {0}{1}" -f $Name, $suffix) }
    else { $script:RngFail += $Name; Write-Output ("SELFTEST FAIL {0}{1}" -f $Name, $suffix) }
  }

  try {
    # ① 场景固定输入
    $sc = $null
    try { $sc = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json) } catch { }
    $scOk = ($null -ne $sc) -and ($sc.canon -eq 'm3-rng-trace-scene-v1') -and
            ([int]$sc.seed -eq 42) -and ($sc.map -eq '0137') -and
            ([int]$sc.start_pose.x -eq 17) -and ([int]$sc.start_pose.y -eq 20) -and
            ([int]$sc.start_pose.dir -eq 2) -and ($null -ne $sc.monsters) -and
            ($sc.monsters.Count -eq 1) -and ($sc.trace.fold.construct -eq 'spawn') -and
            (-not [string]::IsNullOrEmpty($sc.mongen_fixture))
    Add-Result 'scenario-fixed-input' $scOk 'canon/seed/map/pose/monsters/fold/mongen_fixture'

    # ② 冻结 MonGen 提取件存在且（0137）零行
    $fx = Join-Path $Here $sc.mongen_fixture
    if (-not (Test-Path -LiteralPath $fx)) { $fx = Join-Path (Split-Path -Parent $Here) ($sc.mongen_fixture -replace '^\.\./', '') }
    $fxOk = $false
    if (Test-Path -LiteralPath $fx) {
      $fxRows = @(Get-Content -LiteralPath $fx | Where-Object { $_.Trim() -ne '' -and -not $_.StartsWith('#') })
      $fxOk = ($fxRows.Count -eq 0)
    }
    Add-Result 'mongen-fixture-zero-rows' $fxOk ("path=$fx")

    # ③ 场景前置门 selftest 必须 GREEN
    $gOut = ''
    $gRaw = & python -I $SceneGate selftest 2>&1
    $gOut = ($gRaw | Out-String)
    $gRc = [int]$LASTEXITCODE
    $gMatch = [regex]::Match($gOut, 'SELFTEST ok=(\d+)/(\d+)')
    Add-Result 'scene-gate-selftest' (($gRc -eq 0) -and $gMatch.Success -and
      ($gMatch.Groups[1].Value -eq $gMatch.Groups[2].Value)) ("rc=$gRc")
    Write-Output $gOut

    # ④/⑤ 场景门进程内集成：同场景 ⇒ 绿；怪集合不一致 ⇒ 必红且点名。
    $sf = New-RngSceneFiles -Dir $tmpRoot -FixtureName 'mongen-map0137.tsv' -Map '0137' -Seed 42 `
          -MonsterName '稻草人' -MonX 18 -MonY 20 -MonDir 0 -MonCount 1
    $out4 = ''
    $rc4 = Invoke-RngSceneGate -A $sf.A -B $sf.B -Scenario $sf.Scenario -Out ([ref]$out4)
    Add-Result 'scene-gate-identical-green' (($rc4 -eq 0) -and ($out4 -match 'VERDICT GREEN')) ("rc=$rc4")

    $sfBad = New-RngSceneFiles -Dir $tmpRoot -FixtureName 'mongen-map0137b.tsv' -Map '0137' -Seed 42 `
             -MonsterName '稻草人' -MonX 18 -MonY 20 -MonDir 0 -MonCount 1 -ExtraMonsterName '鸡'
    $out5 = ''
    $rc5 = Invoke-RngSceneGate -A $sfBad.A -B $sfBad.B -Scenario $sfBad.Scenario -Out ([ref]$out5)
    Add-Result 'scene-gate-monster-drift-red' (($rc5 -eq 1) -and ($out5 -match 'monsters-equal:a-vs-b')) ("rc=$rc5")

    # ⑥ 账本工具 selftest（含 R-6 folded 映射：等价绿 / 折叠计数·次序不等必红）
    $stOut = ''
    $stRc = Invoke-RngLedgerSelfTest -Out ([ref]$stOut)
    $stMatch = [regex]::Match($stOut, 'SELFTEST GREEN: rng_ledger_diff selftest (\d+)/(\d+)')
    Add-Result 'ledger-selftest' (($stRc -eq 0) -and $stMatch.Success -and
      ($stMatch.Groups[1].Value -eq $stMatch.Groups[2].Value)) ("rc=$stRc")

    # ⑦ folded 等价 ⇒ groupdiff --fold --strict GREEN
    $ta = Join-Path $tmpRoot 'fold-a.tsv'; $tb = Join-Path $tmpRoot 'fold-b.tsv'
    New-RngStream -Path $ta -Sites @($Script:SiteCtor, $Script:SiteSpawnC, $Script:SiteAiC)
    New-RngStream -Path $tb -Sites @($Script:SiteSpawnR, $Script:SiteSpawnR, $Script:SiteAiR)
    $out7 = ''
    $rc7 = Invoke-RngGroupDiff -A $ta -B $tb -Out ([ref]$out7)
    Add-Result 'folded-equivalent-green' (($rc7 -eq 0) -and ($out7 -match 'VERDICT IDENTICAL') -and
      ($out7 -match 'FOLDED_COUNT_EQUAL')) ("rc=$rc7")

    # ⑧ 折叠计数不等 ⇒ 必红且点名 FOLDED_COUNT_DIFF
    New-RngStream -Path $ta -Sites @($Script:SiteCtor, $Script:SiteSpawnC)
    New-RngStream -Path $tb -Sites @($Script:SiteSpawnR)
    $out8 = ''
    $rc8 = Invoke-RngGroupDiff -A $ta -B $tb -Out ([ref]$out8)
    Add-Result 'folded-count-red' (($rc8 -eq 1) -and ($out8 -match 'FOLDED_COUNT_DIFF')) ("rc=$rc8")

    # ⑨ 折叠后次序不等（计数相等）⇒ 必红且定位
    New-RngStream -Path $ta -Sites @($Script:SiteCtor, $Script:SiteAiC, $Script:SiteSpawnC)
    New-RngStream -Path $tb -Sites @($Script:SiteSpawnR, $Script:SiteSpawnR, $Script:SiteAiR)
    $out9 = ''
    $rc9 = Invoke-RngGroupDiff -A $ta -B $tb -Out ([ref]$out9)
    Add-Result 'folded-order-red' (($rc9 -eq 1) -and ($out9 -match 'SHARED_REL_ORDER_DIFF') -and
      ($out9 -notmatch 'FOLDED_COUNT_DIFF')) ("rc=$rc9")

    # ⑩ 缺场景件 ⇒ 门 PREREQ（rc=2，fail-closed）
    $out10 = ''
    $rc10 = Invoke-RngSceneGate -A $sf.A -B (Join-Path $tmpRoot 'nope.json') -Scenario $sf.Scenario -Out ([ref]$out10)
    Add-Result 'scene-gate-missing-prereq' ($rc10 -eq 2) ("rc=$rc10")

    # ⑪ LF 写出（生产者侧根因；EOL 陷阱同族）
    $lf = Test-M3LfWriterSelfTest
    Add-Result 'lf-writer-selftest' ($lf.ok) ($lf.reason)

    # ⑫ 场景件 runner 参数名必须是**本脚本已声明**的参数（M-1：doc-vs-code 漂移机判）。
    $rp = Test-RngScenarioRunnerParams -Path $ScenarioFile
    Add-Result 'scenario-runner-params-declared' ($rp.ok) ($rp.detail)
    # 改坏必红：把 scenario 的 runner 文本塞一个未声明参数 ⇒ 必须点名该参数。
    # 构造走 New-RngDriftScenario（JSON 字段改写），不再对原始文本做正则替换（M-1 sibling 收口）。
    $badScen = New-RngDriftScenario -Source $ScenarioFile -Path (Join-Path $tmpRoot 'scenario-runner-drift.json') `
      -Mutate { param($s) $s.trace.runner = ($s.trace.runner + ' -NotARealParam') }
    $rpBad = Test-RngScenarioRunnerParams -Path $badScen
    Add-Result 'scenario-runner-param-drift-red' ((-not $rpBad.ok) -and ($rpBad.missing -contains 'NotARealParam')) `
      ("missing=" + ($rpBad.missing -join ','))

    # ⑬ scenario 声明 vs 代码真源：已入库 scenario 的 folded 映射 + 组清单必须与账本实现同口径
    #    （M-1 的 sibling：同一族 doc-vs-code 漂移）。
    $scOut = ''
    $scRc = Invoke-RngSceneScenario -Scenario $ScenarioFile -Out ([ref]$scOut)
    Add-Result 'scenario-fold-group-consistency-green' (($scRc -eq 0) -and ($scOut -match 'VERDICT GREEN')) ("rc=$scRc")
    # 改坏必红 A：scenario 声明的 folded 映射与 FOLD_RULES 漂移 ⇒ 必红且点名 scenario-fold-matches-code。
    # 构造走 New-RngDriftScenario（JSON 字段改写），不再对原始文本做正则替换（M-1 sibling 收口）。
    $driftScen = New-RngDriftScenario -Source $ScenarioFile -Path (Join-Path $tmpRoot 'scenario-fold-drift.json') `
      -Mutate { param($s) $s.trace.fold.construct = 'monster_ai' }
    $scOut2 = ''
    $scRc2 = Invoke-RngSceneScenario -Scenario $driftScen -Out ([ref]$scOut2)
    Add-Result 'scenario-fold-drift-red' (($scRc2 -eq 1) -and ($scOut2 -match 'scenario-fold-matches-code')) ("rc=$scRc2")
    # 改坏必红 B：groups_expected 塞入账本未知组 ⇒ 必红且点名 scenario-groups-known。
    $grpScen = New-RngDriftScenario -Source $ScenarioFile -Path (Join-Path $tmpRoot 'scenario-groups-unknown.json') `
      -Mutate { param($s) $s.trace.groups_expected = @($s.trace.groups_expected) + 'no_such_group' }
    $scOut3 = ''
    $scRc3 = Invoke-RngSceneScenario -Scenario $grpScen -Out ([ref]$scOut3)
    Add-Result 'scenario-groups-unknown-red' (($scRc3 -eq 1) -and ($scOut3 -match 'scenario-groups-known')) ("rc=$scRc3")

    # ⑰ M-1 sibling 收口：三条 drift 样本均由 New-RngDriftScenario 以 **JSON 字段改写**构造
    #    （不再对原始文本做正则替换）⇒ 样本必须是合法 JSON 且与源 scenario 结构不同；
    #    构造一旦空转（样本 == 源、drift 用例其实没漂移）⇒ 必红。
    # O-1（reviewer-6 b53a7451，Minor）：base 值**在牙内部**自行解析，去掉对外层 ⑪ 块 `$sc` 的
    #    跨块隐式依赖（那个块一旦重构/改名，`$sc` 退化为 $null ⇒ `-ne` 恒真、fold 半断言空转）。
    $srcBack = $null
    try { $srcBack = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json) } catch { }
    $badBack = (Get-Content -LiteralPath $badScen -Raw | ConvertFrom-Json)
    $driftBack = (Get-Content -LiteralPath $driftScen -Raw | ConvertFrom-Json)
    $grpBack = (Get-Content -LiteralPath $grpScen -Raw | ConvertFrom-Json)
    $driftNativeOk = ($null -ne $srcBack) -and
                     ($badBack.trace.runner -match '-NotARealParam') -and
                     ($driftBack.trace.fold.construct -ne $srcBack.trace.fold.construct) -and
                     (@($grpBack.trace.groups_expected) -contains 'no_such_group')
    Add-Result 'scenario-drift-samples-json-native' $driftNativeOk 'runner/fold/groups drift 样本均为 JSON 字段改写且与源不同（base 牙内自解析，无跨块依赖）'

    # ⑱ O-1 的 tooth-robustness 对照：同一 fold 谓词对「空转样本」（mutation 什么都不改）必须为
    #    **false** ⇒ 证明 ⑰ 不是恒真、drift 构造一旦失效会被咬住（这条正是 O-1 要防的「空转」）。
    $noopScen = New-RngDriftScenario -Source $ScenarioFile -Path (Join-Path $tmpRoot 'scenario-noop.json') `
      -Mutate { param($s) $null = $s }
    $noopBack = (Get-Content -LiteralPath $noopScen -Raw | ConvertFrom-Json)
    $toothBites = ($null -ne $srcBack) -and (-not ($noopBack.trace.fold.construct -ne $srcBack.trace.fold.construct))
    Add-Result 'scenario-drift-tooth-not-vacuous' $toothBites '空转样本 ⇒ 同一 fold 谓词为 false（牙非恒真、不空转）'

    # ⑯ 本脚本从未触碰端口（自检/门/比较全离线）
    Add-Result 'no-port-touched' (-not $script:RngSceneTouchedPorts) 'offline-only'
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:RngPass + $script:RngFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:RngPass, $total, ($script:RngFail -join ','))
  if ($script:RngFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（场景前置门 fail-closed + scenario 声明/folded 映射与代码同口径 + folded 等价绿/计数·次序必红 + LF 写出 + 零端口）'
  exit 0
}

# =========================================================================== #
# Mode: GateOnly —— 只跑场景一致性前置门（离线，不需要栈）
# =========================================================================== #
if ($GateOnly) {
  if ([string]::IsNullOrEmpty($AScene) -or [string]::IsNullOrEmpty($BScene)) {
    Write-Output 'PREREQ_MISSING: -GateOnly 需要 -AScene 与 -BScene'
    exit 2
  }
  $out = ''
  $rc = Invoke-RngSceneGate -A $AScene -B $BScene -Out ([ref]$out)
  Write-Output $out
  if ($rc -eq 0) { Write-Output 'GREEN: 两侧同场景/同起点/同种子（MonGen+Monster 集合同口径）' }
  exit $rc
}

# =========================================================================== #
# Mode: GroupDiffOnly —— 离线比较两侧组账本（--fold --strict）
# =========================================================================== #
if ($GroupDiffOnly) {
  if ([string]::IsNullOrEmpty($AStream) -or [string]::IsNullOrEmpty($BStream)) {
    Write-Output 'PREREQ_MISSING: -GroupDiffOnly 需要 -AStream 与 -BStream'
    exit 2
  }
  $out = ''
  $rc = Invoke-RngGroupDiff -A $AStream -B $BStream -Out ([ref]$out)
  Write-Output $out
  if ($rc -eq 0) { Write-Output 'GREEN: 组集合 + 折叠计数 + 共享组相对次序全等' }
  exit $rc
}

# =========================================================================== #
# Mode: real run（真机两相 trace）—— 本片只交骨架 ⇒ fail-closed 拒跑
# =========================================================================== #
if (-not $Go) {
  Write-Output 'PREREQ_MISSING: 真机两相 trace 需 owner 发放窗口（起栈禁接管道；本片只交夹具骨架，不占端口）'
  Write-Output ("NEXT: 获窗口后跑 -Side csharp 基线 → -Side rust -PeerSession <csharp 会话> → -GateOnly → -GroupDiffOnly")
  exit 2
}
$rpRun = Test-RngScenarioRunnerParams -Path $ScenarioFile
if (-not $rpRun.ok) {
  Write-Output ("PREREQ_MISSING: scenario runner 参数名漂移（$($rpRun.detail)）—— 先让文档与脚本对齐再起真机")
  exit 2
}
if (-not $PinScene) {
  Write-Output 'PREREQ_MISSING: 真机两相 trace 需显式 -PinScene（按 scenario 钉住含怪场景；只给 -Go 不钉场景一律拒跑）'
  exit 2
}
Write-Output 'PREREQ_MISSING: 两侧「场景描述件 + 全场景抽数流水」驱动尚未接线（承接卡见报告），真机序列无法产生可判原始件'
Write-Output ("PLANNED: -Side {0} -Seed {1} -Prefix {2} -PeerSession {3}" -f $Side, $Seed, $Prefix, $PeerSession)
exit 2
