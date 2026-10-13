#requires -Version 5
<#
  run_drop_exp_pair.ps1 —— M3④ 掉落/经验对拍夹具骨架（卡 `openmir2-m3-drop-exp-align`）。

  产物口径：两侧各自跑「固定图 + 固定怪 + 固定起点 + 同种子 100 杀」，各落一份
  `drop_exp_raw.jsonl`（一次击杀一行；schema 见 drop_exp_scenario.json），再由
  `drop_exp_diff.py diff` 做跨侧比较（掉落列表多重集/逐槽位 + 金币 + 经验增量）。

  本片交付的是**夹具骨架**（不占窗口）：判据本体（比较器 + fail-closed + 改坏必红）已完整、
  可离线复跑；真机 100 杀需要窗口 + BotSrv 侧「掉落/经验序列」驱动（承接卡见报告），
  故本脚本的 real-run 分支**fail-closed 拒跑**（exit 2），绝不静默起栈。

  用法：
    # 离线自检（所有路径 fail-closed；含比较器 selftest + 纯度门 selftest + 改坏必红），零 `.rs`、不占端口
    powershell -ExecutionPolicy Bypass -File run_drop_exp_pair.ps1 -SelfCheckNoStack
    # 自检转录由脚本自己以 LF 落盘（避免 `> file` 的 CRLF 重定向）
    powershell -ExecutionPolicy Bypass -File run_drop_exp_pair.ps1 -SelfCheckNoStack -SelfCheckOut <path.txt>
    # 只比较两份既有原始件（离线；不需要栈）
    powershell -ExecutionPolicy Bypass -File run_drop_exp_pair.ps1 -DiffOnly -CsharpRaw <f> -RustRaw <f>
    # 只跑场景纯度门（whitelist T-35；离线，给该侧全场景抽数流水 + 消费者表）
    powershell -ExecutionPolicy Bypass -File run_drop_exp_pair.ps1 -PurityOnly -PuritySide csharp \
        -PurityStream <rng-stream.tsv> -PurityRaw <drop_exp_raw.jsonl> -ExpectedKills 100

  窗口纪律（owner 2026-10-13 03:35）：真机跑要 5000/7000/7100/7200；未获 owner 发放的
  `-Go` 之前**不许起栈/占端口**。起栈命令禁接管道（重定向到文件）。

  纯度门（whitelist T-35，owner comment c78bd13f 有条件批准）：口径 = 「除被测量路径
  （1 只 pin 怪 + 击杀→掉落/经验取数）外 RNG 消费者数 = 0」，墙钟策略取 (a) pin 抑制
  （`AnimalObject.Wondering` 必须为 0）。消费者表 `drop_exp_consumers.tsv` 是唯一真源；
  **认不出的站点 ⇒ 点名 RED**（fail-closed），窗口期按实测清单补表。
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Kills = 100,
  [int]$Seed = 42,
  [string]$Prefix = '',
  [string]$PeerSession = '',
  [string]$ScenarioFile = '',
  [string]$CsharpRaw = '',
  [string]$RustRaw = '',
  [string]$PortsCsv = '5000,7000,7100,7200',
  [switch]$DiffOnly,
  [switch]$Go,                     # owner 发放窗口的显式授权；未给 ⇒ real-run fail-closed（exit 2）
  [switch]$SelfCheckNoStack,
  [string]$SelfCheckOut = '',
  # ---- 场景纯度门（whitelist T-35，owner comment c78bd13f）----
  [switch]$PurityOnly,
  [ValidateSet('csharp', 'rust')][string]$PuritySide = 'csharp',
  [string]$PurityStream = '',      # 该侧全场景抽数流水（MIR2_RNG_LOG / Rust rng-stream.tsv）
  [string]$PurityRaw = '',         # 可选：该侧 drop_exp_raw.jsonl（校验击杀条数）
  [string]$PurityTable = '',       # 缺省 = 本目录 drop_exp_consumers.tsv
  [string]$PurityJson = ''
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$DropExpDiff = Join-Path $Here 'drop_exp_diff.py'
$DropExpPurity = Join-Path $Here 'drop_exp_purity_gate.py'
. (Join-Path (Split-Path -Parent $Here) 'm3hashutil.ps1')

if ([string]::IsNullOrEmpty($ScenarioFile)) { $ScenarioFile = Join-Path $Here 'drop_exp_scenario.json' }
if ([string]::IsNullOrEmpty($PurityTable)) { $PurityTable = Join-Path $Here 'drop_exp_consumers.tsv' }

$Ports = @($PortsCsv.Split(',') | ForEach-Object { [int]$_.Trim() })
$script:DexpTouchedPorts = $false   # 本脚本（含自检）**从不**绑定/连接任何端口；real-run 由外部驱动。

# 返回 [int]$LASTEXITCODE；子进程 stdout 走 -Out 引用，**不混进返回值**
# （历史 bug：`$rc = & python …` 会拿到 @(<stdout>, $LASTEXITCODE) ⇒ `-ne 0` 恒真）。
function Invoke-DropExpDiff {
  param(
    [Parameter(Mandatory = $true)][string]$Csharp,
    [Parameter(Mandatory = $true)][string]$Rust,
    [Parameter(Mandatory = $true)][ref]$Out
  )
  $raw = & python -I $DropExpDiff diff $Csharp $Rust 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function Invoke-DropExpSelfTest {
  param([Parameter(Mandatory = $true)][ref]$Out)
  $raw = & python -I $DropExpDiff selftest 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

# 返回 [int]$LASTEXITCODE；子进程 stdout 走 -Out 引用，**不混进返回值**（同 Invoke-DropExpDiff）。
function Invoke-DexpPurityGate {
  param(
    [Parameter(Mandatory = $true)][string]$Stream,
    [string]$Side = 'csharp',
    [string]$Table = '',
    [string]$Raw = '',
    [int]$ExpectedKills = -1,
    [string]$Json = '',
    [Parameter(Mandatory = $true)][ref]$Out
  )
  $argv = @('-I', $DropExpPurity, 'gate', '--side', $Side, '--stream', $Stream)
  if (-not [string]::IsNullOrEmpty($Table)) { $argv += @('--table', $Table) }
  if (-not [string]::IsNullOrEmpty($Raw)) { $argv += @('--raw', $Raw) }
  if ($ExpectedKills -ge 0) { $argv += @('--expected-kills', [string]$ExpectedKills) }
  if (-not [string]::IsNullOrEmpty($Json)) { $argv += @('--json', $Json) }
  $raw = & python @argv 2>&1
  $Out.Value = ($raw | Out-String)
  return [int]$LASTEXITCODE
}

function New-DexpSynthetic([string]$Path, [string]$DropName, [int]$ExpDelta) {
  $lines = @(
    ('{"k":0,"monster":"Dexp","x":17,"y":20,"gold":10,"drops":[{"name":"' + $DropName + '","index":2,"count":1,"dura":13,"dura_max":20}],"exp_delta":' + $ExpDelta + ',"exp_total":' + $ExpDelta + ',"level":1}')
  )
  Write-M3LfLines -Path $Path -Lines $lines
}

# =========================================================================== #
# Mode: SelfCheckOut —— 把无栈自检转录**由脚本自己**以 LF 落盘
# =========================================================================== #
if ($SelfCheckNoStack -and -not [string]::IsNullOrEmpty($SelfCheckOut)) {
  $selfTmp = Join-Path $env:TEMP ('dropexp-selfcheck-' + [guid]::NewGuid().ToString('N') + '.txt')
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
  $script:DexpPass = 0
  $script:DexpFail = @()
  $tmpRoot = Join-Path $env:TEMP ('dropexp-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  # 只打印 + 记账，**不返回值**（否则 `if (Check ...)` 会被函数输出的字符串污染成恒真）。
  function Add-Result([string]$Name, [bool]$Ok, [string]$Detail) {
    $suffix = if ([string]::IsNullOrEmpty($Detail)) { '' } else { " ($Detail)" }
    if ($Ok) {
      $script:DexpPass++
      Write-Output ("SELFTEST ok {0}{1}" -f $Name, $suffix)
    } else {
      $script:DexpFail += $Name
      Write-Output ("SELFTEST FAIL {0}{1}" -f $Name, $suffix)
    }
  }

  try {
    # ① 场景固定输入
    $sc = $null
    try { $sc = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json) } catch { }
    $scOk = ($null -ne $sc) -and ($sc.canon -eq 'm3-drop-exp-v1') -and ([int]$sc.kills -eq 100) -and
            ([int]$sc.seed -eq 42) -and ($sc.map -eq '0137') -and
            (-not [string]::IsNullOrEmpty($sc.monster)) -and ($null -ne $sc.attacker) -and
            ([int]$sc.attacker.x -eq 17) -and ([int]$sc.attacker.y -eq 20) -and ([int]$sc.attacker.dir -eq 2)
    Add-Result 'scenario-fixed-input' $scOk 'canon/kills/seed/map/monster/pose'

    # ② 比较器 selftest（fail-closed + 改坏必红）必须 GREEN
    $stOut = ''
    $stRc = Invoke-DropExpSelfTest -Out ([ref]$stOut)
    $stMatch = [regex]::Match($stOut, 'SELFTEST ok=(\d+)/(\d+)')
    $stOk = ($stRc -eq 0) -and $stMatch.Success -and ($stMatch.Groups[1].Value -eq $stMatch.Groups[2].Value)
    Add-Result 'comparator-selftest' $stOk ("rc=$stRc")
    Write-Output $stOut

    # ③ 逐项相同 ⇒ 绿（进程内集成，走生产 diff 函数）
    $csGood = Join-Path $tmpRoot 'cs-goods.jsonl'
    $rsGood = Join-Path $tmpRoot 'rs-goods.jsonl'
    New-DexpSynthetic -Path $csGood -DropName 'sword' -ExpDelta 12
    New-DexpSynthetic -Path $rsGood -DropName 'sword' -ExpDelta 12
    $out1 = ''
    $rc1 = Invoke-DropExpDiff -Csharp $csGood -Rust $rsGood -Out ([ref]$out1)
    Add-Result 'identical-green' ($rc1 -eq 0) ("rc=$rc1")

    # ④ 改坏一次经验 ⇒ 必红且点名 exp_delta
    $rsBad = Join-Path $tmpRoot 'rs-badexp.jsonl'
    New-DexpSynthetic -Path $rsBad -DropName 'sword' -ExpDelta 13
    $out2 = ''
    $rc2 = Invoke-DropExpDiff -Csharp $csGood -Rust $rsBad -Out ([ref]$out2)
    $mutOk = ($rc2 -eq 1) -and ($out2 -match 'exp_delta')
    Add-Result 'mutate-exp-red' $mutOk ("rc=$rc2")

    # ⑤ 缺文件 ⇒ PREREQ fail-closed（rc=2）
    $out3 = ''
    $rc3 = Invoke-DropExpDiff -Csharp (Join-Path $tmpRoot 'nope.jsonl') -Rust $rsGood -Out ([ref]$out3)
    Add-Result 'missing-file-prereq' ($rc3 -eq 2) ("rc=$rc3")

    # ⑥ LF 写出（生产者侧根因；EOL 陷阱同族）
    $lf = Test-M3LfWriterSelfTest
    Add-Result 'lf-writer-selftest' ($lf.ok) ($lf.reason)

    # ⑦ 本脚本从未触碰端口（自检/比较全离线）
    Add-Result 'no-port-touched' (-not $script:DexpTouchedPorts) 'offline-only'

    # ---- 场景纯度门（whitelist T-35，owner comment c78bd13f）---- #
    # 数据源口径：全场景抽数流水 5 列 TSV（seq/kind/max/result/site）。
    $pMeasuredSite = 'GameSrv.Word.WorldServer.MonGetRandomItems@WorldServer.MonGen.cs:451'
    $pWallSite = 'M2Server.Actor.AnimalObject.Wondering@AnimalObject.cs:427'
    $pUnknownSite = 'Mystery.Thing@Mystery.cs:1'
    function New-DexpStream([string]$Path, [string[]]$Sites) {
      $lines = @('1' + "`t" + 'INJECT_OK' + "`t" + '42' + "`t" + '0', 'SEED_PROBE 42')
      $seq = 2
      foreach ($s in $Sites) {
        $lines += (@($seq, 'Next(max)', '100', '7', $s) -join "`t")
        $seq++
      }
      Write-M3LfLines -Path $Path -Lines $lines
    }
    $pClean = Join-Path $tmpRoot 'purity-clean.tsv'
    $pWall = Join-Path $tmpRoot 'purity-wall.tsv'
    $pUnknown = Join-Path $tmpRoot 'purity-unknown.tsv'
    New-DexpStream -Path $pClean -Sites @($pMeasuredSite, $pMeasuredSite)
    New-DexpStream -Path $pWall -Sites @($pMeasuredSite, $pWallSite)
    New-DexpStream -Path $pUnknown -Sites @($pMeasuredSite, $pUnknownSite)

    # ⑧ 纯度门 selftest（fail-closed + 改坏必红）必须 GREEN。
    $pgOut = ''
    $pgRaw = & python -I $DropExpPurity selftest 2>&1
    $pgOut = ($pgRaw | Out-String)
    $pgRc = [int]$LASTEXITCODE
    $pgMatch = [regex]::Match($pgOut, 'SELFTEST ok=(\d+)/(\d+)')
    $pgOk = ($pgRc -eq 0) -and $pgMatch.Success -and ($pgMatch.Groups[1].Value -eq $pgMatch.Groups[2].Value)
    Add-Result 'purity-gate-selftest' $pgOk ("rc=$pgRc")

    # ⑨ 只有被测量路径 ⇒ 绿（证明门不是恒红）。
    $outP1 = ''
    $rcP1 = Invoke-DexpPurityGate -Stream $pClean -Side csharp -Table $PurityTable -Out ([ref]$outP1)
    Add-Result 'purity-clean-green' (($rcP1 -eq 0) -and ($outP1 -match 'GREEN: 除被测量路径外')) ("rc=$rcP1")

    # ⑩ 墙钟消费者（AnimalObject.Wondering）⇒ 必红且点名 wallclock；改坏（删表的 wallclock 行）亦红。
    $outP2 = ''
    $rcP2 = Invoke-DexpPurityGate -Stream $pWall -Side csharp -Table $PurityTable -Out ([ref]$outP2)
    Add-Result 'purity-wallclock-red' (($rcP2 -eq 1) -and ($outP2 -match 'wallclock_consumer')) ("rc=$rcP2")

    # ⑪ 认不出的站点 ⇒ 必红且点名 unknown（fail-closed，不静默放行）。
    $outP3 = ''
    $rcP3 = Invoke-DexpPurityGate -Stream $pUnknown -Side csharp -Table $PurityTable -Out ([ref]$outP3)
    Add-Result 'purity-unknown-red' (($rcP3 -eq 1) -and ($outP3 -match 'unknown_consumer')) ("rc=$rcP3")

    # ⑫ Rust 侧表里尚无 measured 行（④-D 未落地）⇒ PREREQ 拒跑（fail-closed，**不许**假绿）。
    $outP4 = ''
    $rcP4 = Invoke-DexpPurityGate -Stream $pClean -Side rust -Table $PurityTable -Out ([ref]$outP4)
    Add-Result 'purity-pending-side-prereq' (($rcP4 -eq 2) -and ($outP4 -match '没有 \`measured\` 行')) ("rc=$rcP4")
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:DexpPass + $script:DexpFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:DexpPass, $total, ($script:DexpFail -join ','))
  if ($script:DexpFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（比较器 + 纯度门 fail-closed + 改坏必红 + LF 写出 + 零端口）'
  exit 0
}

# =========================================================================== #
# Mode: PurityOnly —— 只跑场景纯度门（whitelist T-35；离线，不需要栈）
# ===========================================================================
if ($PurityOnly) {
  if ([string]::IsNullOrEmpty($PurityStream)) {
    Write-Output 'PREREQ_MISSING: -PurityOnly 需要 -PurityStream（该侧全场景抽数流水）'
    exit 2
  }
  if (-not (Test-Path -LiteralPath $PurityStream)) {
    Write-Output ("PREREQ_MISSING: 找不到抽数流水 {0}" -f $PurityStream)
    exit 2
  }
  $out = ''
  $ek = [int]$Kills
  $rc = Invoke-DexpPurityGate -Stream $PurityStream -Side $PuritySide -Table $PurityTable `
        -Raw $PurityRaw -ExpectedKills $ek -Json $PurityJson -Out ([ref]$out)
  Write-Output $out
  exit $rc
}

# =========================================================================== #
# Mode: DiffOnly —— 离线比较两份既有原始件
# =========================================================================== #
if ($DiffOnly) {
  if ([string]::IsNullOrEmpty($CsharpRaw) -or [string]::IsNullOrEmpty($RustRaw)) {
    Write-Output 'PREREQ_MISSING: -DiffOnly 需要 -CsharpRaw 与 -RustRaw'
    exit 2
  }
  $out = ''
  $rc = Invoke-DropExpDiff -Csharp $CsharpRaw -Rust $RustRaw -Out ([ref]$out)
  Write-Output $out
  if ($rc -eq 0) { Write-Output 'GREEN: 掉落/经验跨侧 diff = 0' }
  exit $rc
}

# =========================================================================== #
# Mode: real run（真机 100 杀）—— 本片只交骨架 ⇒ fail-closed 拒跑
# =========================================================================== #
if (-not $Go) {
  Write-Output 'PREREQ_MISSING: 真机 100 杀需 owner 发放窗口（起栈禁接管道；本片只交夹具骨架，不占端口）'
  Write-Output ("NEXT: 获窗口后按 README 的命令序列跑（-Side csharp 基线 → -Side rust -PeerSession <csharp 会话> → -DiffOnly）")
  exit 2
}
Write-Output 'PREREQ_MISSING: BotSrv 尚无「掉落/经验序列」驱动（承接卡见报告），真机序列无法产生可判原始件'
Write-Output ("PLANNED: -Side {0} -Kills {1} -Seed {2} -Prefix {3} -PeerSession {4}" -f $Side, $Kills, $Seed, $Prefix, $PeerSession)
exit 2
