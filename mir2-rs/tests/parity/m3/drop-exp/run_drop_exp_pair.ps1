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
    # 离线自检（所有路径 fail-closed；含比较器 selftest + 改坏必红），零 `.rs`、不占端口
    powershell -ExecutionPolicy Bypass -File run_drop_exp_pair.ps1 -SelfCheckNoStack
    # 自检转录由脚本自己以 LF 落盘（避免 `> file` 的 CRLF 重定向）
    powershell -ExecutionPolicy Bypass -File run_drop_exp_pair.ps1 -SelfCheckNoStack -SelfCheckOut <path.txt>
    # 只比较两份既有原始件（离线；不需要栈）
    powershell -ExecutionPolicy Bypass -File run_drop_exp_pair.ps1 -DiffOnly -CsharpRaw <f> -RustRaw <f>

  窗口纪律（owner 2026-10-13 03:35）：真机跑要 5000/7000/7100/7200；未获 owner 发放的
  `-Go` 之前**不许起栈/占端口**。起栈命令禁接管道（重定向到文件）。
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
  [string]$SelfCheckOut = ''
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$DropExpDiff = Join-Path $Here 'drop_exp_diff.py'
. (Join-Path (Split-Path -Parent $Here) 'm3hashutil.ps1')

if ([string]::IsNullOrEmpty($ScenarioFile)) { $ScenarioFile = Join-Path $Here 'drop_exp_scenario.json' }

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
  } finally {
    Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  }

  $total = $script:DexpPass + $script:DexpFail.Count
  Write-Output ("SELFTEST ok={0}/{1} failures={2}" -f $script:DexpPass, $total, ($script:DexpFail -join ','))
  if ($script:DexpFail.Count -gt 0) { exit 1 }
  Write-Output 'GREEN: 无栈自检全绿（比较器 fail-closed + 改坏必红 + LF 写出 + 零端口）'
  exit 0
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
