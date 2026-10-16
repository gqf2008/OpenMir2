#requires -Version 5
<#
  run_drop_exp_pair.ps1 —— M3④ 掉落/经验对拍夹具（卡 `openmir2-m3-drop-exp-align` /
  `openmir2-m3-drop-exp-fixture-botsrv`）。

  产物口径：两侧各自跑「固定图 + 固定怪 + 固定起点 + 同种子 100 杀」，各落一份
  `drop_exp_raw.jsonl`（一次击杀一行；schema 见 drop_exp_scenario.json），再由
  `drop_exp_diff.py diff` 做跨侧比较（掉落列表多重集/逐槽位 + 金币 + 经验增量）。

  real-run 接线（DoD②，卡 `openmir2-m3-drop-exp-fixture-botsrv`）：`-Go` 后复用 `run_pair.ps1` 的
  两相 pin（预热建角 + SQL pin + 会话入库）并把 BotSrv 固定掉落/经验序列（`-DropExpSequence`）
  透传下去；未获 owner 发放的 `-Go` ⇒ fail-closed 拒跑（exit 2），绝不静默起栈。
  **仍缺的依赖（真实跑前必须先落地，归 shared 承接卡 `openmir2-m3-rng-scene-driver`）**：
  「0137 上运行时 pin 出单只固定怪」（场景件注释 + `rng-trace-scene/DRIVER-CONTRACT.md` §7）；
  单只怪未落地时 `run_pair` 的 DropExp 分支会在「0 杀 < -DropExpKills」处 RED（fail-closed）。

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
  [int]$Bots = 1,                  # 掉落/经验场景 = 单个 attacker（attacker 由 DropExpRole 精确选择）
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$TimeoutSec = 600,
  [int]$DropExpStepMs = 500,       # 仅 real-run：每击间隔（写进 BotSrv AppSetting）
  [int]$DropExpStartDelayMs = 3000,# 仅 real-run：进世界到第一击的额外等待
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
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..\..')).Path
. (Join-Path (Split-Path -Parent $Here) 'm3hashutil.ps1')
# DoD② 接线（卡 openmir2-m3-drop-exp-fixture-botsrv）：real-run 复用 run_pair.ps1 的两相 pin + 会话入库。
$RunPair = Join-Path (Split-Path -Parent $Here) 'run_pair.ps1'
$RunBots = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$Sessions = Join-Path (Split-Path -Parent $Here) 'sessions'
. (Join-Path (Split-Path -Parent $Here) 'gate_guards.ps1')

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

    # ⑫ 两侧实测站点表就位后：Rust 侧也有 `measured` 行 ⇒ 只含被测量路径（drop）+ 生成期构造（setup）
    #    的 Rust 流水绿灯（不再 PREREQ；本项与 ⑩/⑪ 同口径地证明门不是「Rust 恒 PREREQ」）。
    $pRustMeasuredSite = 'crates/formula/src/drop.rs:109'
    $pRustSetupSite = 'bins/game-svr/src/mongen.rs:289'
    $pRustWallSite = 'crates/world/src/monai.rs:172'
    $pRustClean = Join-Path $tmpRoot 'purity-rust-clean.tsv'
    $pRustWall = Join-Path $tmpRoot 'purity-rust-wall.tsv'
    New-DexpStream -Path $pRustClean -Sites @($pRustMeasuredSite, $pRustSetupSite)
    New-DexpStream -Path $pRustWall -Sites @($pRustMeasuredSite, $pRustWallSite)
    $outP4 = ''
    $rcP4 = Invoke-DexpPurityGate -Stream $pRustClean -Side rust -Table $PurityTable -Out ([ref]$outP4)
    Add-Result 'purity-rust-side-green' (($rcP4 -eq 0) -and ($outP4 -match 'GREEN: 除被测量路径外')) ("rc=$rcP4")

    # ⑬ Rust 侧 pin 抑制失效（monai.rs Wondering 出现）⇒ 必红且点名 wallclock（改坏必红）。
    $outP5 = ''
    $rcP5 = Invoke-DexpPurityGate -Stream $pRustWall -Side rust -Table $PurityTable -Out ([ref]$outP5)
    Add-Result 'purity-rust-wallclock-red' (($rcP5 -eq 1) -and ($outP5 -match 'wallclock_consumer')) ("rc=$rcP5")

    # ⑭ 未给 -Go ⇒ real-run fail-closed（exit 2），**不许**静默起栈/占端口。
    $null = & powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -Side csharp -PortsCsv 17985,17986,17987,17988 2>&1
    $rcNoGo = $LASTEXITCODE
    Add-Result 'no-go-realrun-prereq' ($rcNoGo -eq 2) ("rc=$rcNoGo")

    # ⑮ DoD② 接线（离线、零栈）：run_pair 的开关耦合 —— -DropExpSequence 必须一并传 -DisableAutoPlay
    #    与全部 DropExp* 参数；默认路径不得出现。
    $emitOnRaw = & powershell -NoProfile -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -DropExpSequence -DropExpKills 100 -DropExpStepMs 500 -DropExpStartDelayMs 3000 -DropExpDir 2 -DropExpRole 0 -DropExpMonster '稻草人'
    if ($LASTEXITCODE -ne 0) { Add-Result 'emit-dropexp-passthrough' $false 'emit_on_rc!=0' }
    else {
      $emitOnText = ((@($emitOnRaw) | Where-Object { $_ -notmatch '^\s*(WARN:|PIN_NO_MONSTER|MAP_HAS_NO_MONSTER|MAP_HAS_NO_AI_CONSUMERS)' }) -join "`n").Trim()
      $emitOn = ($emitOnText | ConvertFrom-Json)
      $onArgs = @($emitOn.bot_args)
      $onHas = ($onArgs -contains '-DropExpSequence') -and ($onArgs -contains '-DisableAutoPlay') -and ($onArgs -contains '-DropExpKills') -and ($onArgs -contains '-DropExpMonster') -and ($onArgs -contains '稻草人')
      $wantScope = 'on kills=100 step_ms=500 start_delay_ms=3000 dir=2 role=0 monster=稻草人 disable_autoplay=True'
      Add-Result 'emit-dropexp-passthrough' ($onHas -and ($emitOn.drop_exp_sequence -eq $wantScope)) ("scope=" + $emitOn.drop_exp_sequence)
    }

    $emitOffRaw = & powershell -NoProfile -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs
    if ($LASTEXITCODE -ne 0) { Add-Result 'emit-dropexp-default-off' $false 'emit_off_rc!=0' }
    else {
      $emitOffText = ((@($emitOffRaw) | Where-Object { $_ -notmatch '^\s*(WARN:|PIN_NO_MONSTER|MAP_HAS_NO_MONSTER|MAP_HAS_NO_AI_CONSUMERS)' }) -join "`n").Trim()
      $emitOff = ($emitOffText | ConvertFrom-Json)
      $offArgs = @($emitOff.bot_args)
      Add-Result 'emit-dropexp-default-off' ((-not ($offArgs -contains '-DropExpSequence')) -and (-not ($offArgs -contains '-DisableAutoPlay')) -and ($emitOff.drop_exp_sequence -eq '(off)')) ("scope=" + $emitOff.drop_exp_sequence)
    }

    # ⑮b real-run 的 pin 参数形态（场景固定输入）：-PinStartPose + map/(x,y)/level/dir 必须与 scenario 一致，
    #     否则 run_pair 缺省会落到 map 0 (289,618) ⇒ 场景件不是单一真源（改坏 PinMapName/PinCX 必红）。
    $scObj = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)
    $pinArgs = @('-EmitOpsArgs', '-FixedNewChr', '-PinStartPose', '-PinMapName', [string]$scObj.map,
                 '-PinCX', [int]$scObj.attacker.x, '-PinCY', [int]$scObj.attacker.y,
                 '-PinLevel', [int]$scObj.attacker.level, '-PinDir', [int]$scObj.attacker.dir, '-DropExpSequence')
    $pinRaw = & powershell -NoProfile -ExecutionPolicy Bypass -File $RunPair @pinArgs
    if ($LASTEXITCODE -ne 0) { Add-Result 'emit-dropexp-pin-args' $false 'emit_pin_rc!=0' }
    else {
      $pinText = ((@($pinRaw) | Where-Object { $_ -notmatch '^\s*(WARN:|PIN_NO_MONSTER|MAP_HAS_NO_MONSTER|MAP_HAS_NO_AI_CONSUMERS)' }) -join "`n").Trim()
      $pinObj = ($pinText | ConvertFrom-Json)
      $wantPosePrefix = ('map={0} home=0,289,618 pos={1},{2} level={3} dir={4}' -f $scObj.map, $scObj.attacker.x, $scObj.attacker.y, $scObj.attacker.level, $scObj.attacker.dir)
      Add-Result 'emit-dropexp-pin-args' ([string]$pinObj.start_pose -like ($wantPosePrefix + '*')) ("start_pose=" + $pinObj.start_pose)
    }

    # ⑮c 采集开关前置门接线元牙（卡 openmir2-window-collector-preflight）：
    #   旧实现是裸子串：`-Mode drop_exp -ExpectSeed` 出现在**自检自身的正则字面量**里、`collector_switch_preflight.ps1`
    #   出现在路径赋值/注释里 ⇒ 把真正的 `& powershell … -File $CollectorPreflight -Mode drop_exp …` 调用整行删掉仍绿。
    #   现由 gate_guards.ps1::Test-CollectorGateWired 只认**非注释的真实调用**行：真实文件 ⇒ true；
    #   删掉真调用行、**诱饵全留**（自检正则字面量 + 路径赋值）⇒ 必须 false。
    $dexpSrc = Get-Content -LiteralPath $PSCommandPath -Raw
    $dexpTokens = @('-Mode', 'drop_exp')
    $dexpWiredReal = Test-CollectorGateWired -Path $PSCommandPath -RequireTokens $dexpTokens
    $dexpMutText = Remove-M3CollectorGateCallLines -Content $dexpSrc
    $dexpDecoysKept = ($dexpMutText -match 'collector_switch_preflight\.ps1') -and ($dexpMutText -match '-Mode drop_exp')
    $dexpWiredMut = Test-CollectorGateWired -Content $dexpMutText -RequireTokens $dexpTokens
    $dexpWired = $dexpWiredReal -and (-not $dexpWiredMut) -and $dexpDecoysKept
    Add-Result 'collector-preflight-wired' $dexpWired ("real={0} decoysKept={1} mutated={2} => ok={3}" -f $dexpWiredReal, $dexpDecoysKept, $dexpWiredMut, $dexpWired)

    # ⑯ BotSrv 生产函数 selftest（attacker 精确选择 / 击杀窗口归类 / SM_WINEXP 字段抽取 / 行组装）
    #    —— 改坏必红由代码 review + 本断言钉住（片内红检另存证据）。
    $botProj = Join-Path $RepoRoot 'src\BotSrv\BotSrv.csproj'
    $botExe = Join-Path $RepoRoot 'src\BotSrv\bin\Release\BotSrv.exe'
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $botBuildOut = (& dotnet build $botProj -c Release --nologo -v q 2>&1 | Out-String)
    $botBuildRc = $LASTEXITCODE
    if ($botBuildRc -ne 0) {
      Write-Output $botBuildOut
      Add-Result 'botsrv-dropseq-selftest' $false ("build_rc=$botBuildRc")
    }
    else {
      $seqOut = (& $botExe --selftest-dropseq 2>&1 | Out-String)
      $seqRc = $LASTEXITCODE
      if ($seqRc -ne 0) { Write-Output $seqOut }
      Add-Result 'botsrv-dropseq-selftest' (($seqRc -eq 0) -and ($seqOut -match 'DROPEXPSEQ SELFTEST GREEN')) ("rc=$seqRc")
    }
    $ErrorActionPreference = $prevEap

    # ⑰ DoD② 接线（离线、零栈）：run_bots 的 DropExp* 配置透传 —— 开 ⇒ AppSetting.json 六个字段到位；关 ⇒ 默认 off。
    $appJson = Join-Path $RepoRoot 'src\BotSrv\bin\Release\AppSetting.json'
    $cfgOnOut = (& powershell -NoProfile -ExecutionPolicy Bypass -File $RunBots -DropExpSequence -DropExpKills 100 -DropExpStepMs 500 -DropExpStartDelayMs 3000 -DropExpDir 2 -DropExpRole 0 -DropExpMonster '稻草人' -WriteConfigOnly -SkipBuild 2>&1 | Out-String)
    $cfgOnRc = $LASTEXITCODE
    if ($cfgOnRc -ne 0 -or -not (Test-Path $appJson)) {
      Add-Result 'botsrv-dropexp-config-on' $false ("rc=$cfgOnRc")
    }
    else {
      $app = Get-Content $appJson -Raw | ConvertFrom-Json
      $ok = ($app.BotPlay.DropExpSequence -eq $true) -and ([int]$app.BotPlay.DropExpKills -eq 100) -and ([int]$app.BotPlay.DropExpStepMs -eq 500) -and ([int]$app.BotPlay.DropExpDir -eq 2) -and ([int]$app.BotPlay.DropExpRole -eq 0) -and ($app.BotPlay.DropExpMonster -eq '稻草人')
      Add-Result 'botsrv-dropexp-config-on' $ok ("DropExpSequence=" + $app.BotPlay.DropExpSequence + " kills=" + $app.BotPlay.DropExpKills)
    }
    $cfgOffOut = (& powershell -NoProfile -ExecutionPolicy Bypass -File $RunBots -WriteConfigOnly -SkipBuild 2>&1 | Out-String)
    $cfgOffRc = $LASTEXITCODE
    if ($cfgOffRc -ne 0 -or -not (Test-Path $appJson)) {
      Add-Result 'botsrv-dropexp-config-default-off' $false ("rc=$cfgOffRc")
    }
    else {
      $app2 = Get-Content $appJson -Raw | ConvertFrom-Json
      Add-Result 'botsrv-dropexp-config-default-off' ($app2.BotPlay.DropExpSequence -eq $false) ("DropExpSequence=" + $app2.BotPlay.DropExpSequence)
    }
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:DexpPass + $script:DexpFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:DexpPass, $total, ($script:DexpFail -join ','))
  if ($script:DexpFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（比较器 + 纯度门 fail-closed + 改坏必红 + LF 写出 + no-Go real-run fail-closed + DoD② run_pair DropExp 透传/默认关 + BotSrv 配置透传/默认关 + BotSrv --selftest-dropseq + 零端口）'
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
# Mode: real run（真机 100 杀）—— 未获 owner 窗口（-Go）前 fail-closed 拒跑
# ===========================================================================
# DoD②（卡 openmir2-m3-drop-exp-fixture-botsrv）：real-run 分支复用 `run_pair.ps1` 的两相 pin
# （预热建角 + SQL pin + 会话入库）并把 BotSrv 的固定掉落/经验序列（`-DropExpSequence`）透传下去；
# 每侧各落 `drop_exp_raw.jsonl`；`-Side rust -PeerSession <csharp 会话>` 时做 peer 护栏 + 跨侧 diff。
# 仍未接线的依赖（**真实跑前必须先落地**，归 shared 承接卡 `openmir2-m3-rng-scene-driver`）：
#   场景件注释/`rng-trace-scene/DRIVER-CONTRACT.md` §7 指出的「0137 上运行时 pin 出**单只固定怪**」
#   （C# 侧 GM/脚本 spawn + Rust 侧宿主 AI 模式开关）。本夹具本刻只交「驱动透传 + 两相 pin」接线；
#   单只怪未落地时，`run_pair` 的 DropExp 分支会在「0 杀 < -DropExpKills」处 RED（fail-closed，不静默绿）。
if (-not $Go) {
  Write-Output 'PREREQ_MISSING: 真机 100 杀需 owner 发放窗口（起栈禁接管道；未获 -Go 一律拒跑，不占端口）'
  Write-Output ("NEXT: 获窗口后按 README 的命令序列跑（-Side csharp 基线 → -Side rust -PeerSession <csharp 会话> → -DiffOnly）")
  exit 2
}

# --- 采集开关前置门（卡 openmir2-window-collector-preflight；A4 主件 + A2 交叉断言）---
# real-run 起头 fail-closed 断言本次掉落/经验对拍所需的采集开关（MIR2_RNG_SEED/LOG/SITE/STATE_LOG/
# DROPEXP_EXT_FIELDS/CLOCK_MS）都在调用方环境里，且 C# `INJECT_OK <seed>` 与 Rust 自证行一致；
# 缺任一项/取值非法/种子交叉不符 ⇒ 拒跑（绝不静默半采样跑一场）。
$CollectorPreflight = Join-Path (Split-Path -Parent $Here) 'collector_switch_preflight.ps1'
& powershell -ExecutionPolicy Bypass -File $CollectorPreflight -Mode drop_exp -ExpectSeed $Seed
if ($LASTEXITCODE -ne 0) {
  Write-Output 'RED: 采集开关前置门未过（缺项/非法/种子交叉不符）——拒跑（fail-closed）。修法：export 缺失开关后重跑；见 collector_switch_preflight.ps1'
  exit 1
}

# 场景固定输入（scenario 是单一真源；命令行 -Kills 必须与之一致，否则 fail-closed）。
$sc = $null
try { $sc = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json) } catch { }
if ($null -eq $sc) { Write-Output ("PREREQ_MISSING: 读不出场景件 {0}" -f $ScenarioFile); exit 2 }
if ([int]$sc.kills -ne $Kills) {
  Write-Output ("PREREQ_MISSING: -Kills {0} 与场景 kills={1} 不一致（场景是单一真源）" -f $Kills, $sc.kills)
  exit 2
}
$monsterName = [string]$sc.monster
$scMap = [string]$sc.map
$attackDir = [int]$sc.attacker.dir
$attackX = [int]$sc.attacker.x
$attackY = [int]$sc.attacker.y
$attackLevel = [int]$sc.attacker.level
Write-Output ("DROPEXP_REALRUN side={0} kills={1} seed={2} map={3} monster={4} pose=({5},{6}) dir={7} level={8}" -f `
  $Side, $Kills, $Seed, $scMap, $monsterName, $attackX, $attackY, $attackDir, $attackLevel)

# 前置：MySQL（两侧都要取两相 pin 快照/落库）。
$mysqlOk = $false
try {
  $c = New-Object System.Net.Sockets.TcpClient
  $iar = $c.BeginConnect('127.0.0.1', 3306, $null, $null)
  $mysqlOk = $iar.AsyncWaitHandle.WaitOne(1500)
  $c.Close()
} catch { $mysqlOk = $false }
if (-not $mysqlOk) { Write-Output 'PREREQ_MISSING: MySQL 127.0.0.1:3306 不可达'; exit 2 }
Write-Output 'PREFLIGHT MySQL 在听'

# 复用 run_pair.ps1 的两相 pin + 固定掉落/经验序列（两侧同开关、同起点）。
$pairArgs = @('-Side', $Side, '-Bots', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port, '-TimeoutSec', $TimeoutSec,
              '-FixedNewChr', '-PinStartPose', '-PinMapName', $scMap, '-PinCX', $attackX, '-PinCY', $attackY, '-PinLevel', $attackLevel, '-PinDir', $attackDir,
              '-DropExpSequence', '-DropExpKills', $Kills, '-DropExpStepMs', $DropExpStepMs, '-DropExpStartDelayMs', $DropExpStartDelayMs,
              '-DropExpDir', $attackDir, '-DropExpRole', 0, '-DropExpMonster', $monsterName)
& powershell -ExecutionPolicy Bypass -File $RunPair @pairArgs
$pairExit = $LASTEXITCODE
if ($pairExit -ne 0) {
  Write-Output ("RED: run_pair（{0} 侧）未通过（rc={1}）——两相 pin / 固定掉落经验序列 / 原始件未成立" -f $Side, $pairExit)
  exit 1
}

$mine = @(Get-ChildItem $Sessions -Directory -Filter ("{0}-*" -f $Side) -ErrorAction SilentlyContinue |
          Where-Object { Test-Path (Join-Path $_.FullName 'baseline.json') } |
          Sort-Object LastWriteTime -Descending) | Select-Object -First 1
if ($null -eq $mine) {
  Write-Output ("RED: 找不到 {0} 侧新会话目录（缺 baseline.json）" -f $Side)
  exit 1
}
Write-Output ("SESSION_{0} {1}" -f $Side.ToUpper(), $mine.Name)

$mineBaseline = Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json
$scopeLocal = [string]$mineBaseline.drop_exp_sequence
Write-Output ("DROPEXPSCOPE local='{0}'" -f $scopeLocal)
$rawMine = Join-Path $mine.FullName 'drop_exp_raw.jsonl'
if (-not (Test-Path -LiteralPath $rawMine)) {
  Write-Output ("PREREQ_MISSING: 本侧会话缺 drop_exp_raw.jsonl（run_pair assemble 未产出）-> {0}" -f $rawMine)
  exit 2
}

if ($Side -eq 'rust' -and -not [string]::IsNullOrEmpty($PeerSession)) {
  $peer = Join-Path $Sessions $PeerSession
  if (-not (Test-Path $peer)) { Write-Output ("PREREQ_MISSING: 找不到 C# 对照会话 {0}" -f $peer); exit 2 }
  $poseLocal = (Get-M3SessionStartPose -Dir $mine.FullName)
  $poseGuard = Get-M3PeerStartPoseGuard -PeerDir $peer -LocalPose $poseLocal
  foreach ($m in $poseGuard.messages) { Write-Output $m }
  if (@($poseGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧固定起点不同口径（{0}）——先按同一组 pin 重跑基线" -f (@($poseGuard.failed) -join ','))
    exit 2
  }
  Write-Output ("DROPEXPPOSE_OK peer='{0}' local='{1}'" -f $poseGuard.peer_pose, $poseLocal)
  $scopeGuard = Get-M3PeerDropExpScopeGuard -PeerDir $peer -LocalScope $scopeLocal
  foreach ($m in $scopeGuard.messages) { Write-Output $m }
  if (@($scopeGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧掉落/经验序列开关不同口径（{0}）——先按同一组开关重跑基线" -f (@($scopeGuard.failed) -join ','))
    exit 2
  }
  Write-Output ("DROPEXPSCOPE_OK peer='{0}' local='{1}'" -f $scopeGuard.peer_scope, $scopeLocal)
  $out = ''
  $rc = Invoke-DropExpDiff -Csharp (Join-Path $peer 'drop_exp_raw.jsonl') -Rust $rawMine -Out ([ref]$out)
  Write-Output $out
  if ($rc -eq 0) { Write-Output 'GREEN: 跨侧掉落/经验 diff = 0'; exit 0 }
  if ($rc -eq 2) { Write-Output 'PREREQ_MISSING: 掉落/经验原始件不可判（缺文件/字段/非法 JSON/k 不连续）'; exit 2 }
  Write-Output 'RED: 跨侧掉落/经验 diff != 0'
  exit 1
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_drop_exp_pair.ps1 -Side rust -Kills {0} -Seed {1} -Prefix <前缀> -PeerSession {2} -Go" -f $Kills, $Seed, $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与固定掉落/经验序列判据通过'
exit 0
