#requires -Version 5
<#
.SYNOPSIS
  run_attack_pair.ps1 — M3②③「双侧同序列」攻击对拍的一条命令
  （卡 openmir2-m3-attack-pair-fixture，issue 76cc08b8）。

.DESCRIPTION
  目标（卡内 DoD）：同一 bot 固定攻击序列在 C# / Rust 两侧各跑一次 ⇒ **逐项伤害序列**
  （≥100 次攻击，按「第 k 次」对齐）+ **命中/躲闪统计** 跨侧比对；期望 diff = 0
  （差异要么修到 0、要么按 §6.0-3 登记 whitelist）。

  本脚本**不新造编排**：一次「侧运行」直接复用 `run_pair.ps1`
  （两相固定起点 pin + settle+hold、会话入库、`scoped_diff` 口径）；本脚本只加三件：
    ① 入口实现识别护栏（-Side 与 5000/7000/7100/7200 的实际监听实现必须对应）；
    ② `-AttackSequence -AttackPinPair` 透传 + `M3ATTSEQ` 标记判据（序列没跑完整 ⇒ RED）；
    ③ 攻击序列跨侧口径：`attack_diff.py align/diff`（逐 k 伤害 + hits/dodges/attacked/sum）。

  度量口径（见卡 comment；**先用 `comment` 写进卡再动手**）：
    - 伤害来源 = 受击方收到的 `SM_STRUCK` 的 `Series`（= oracle
      `MakeMessage(SM_STRUCK, ActorId, HP, MaxHP, wParam)` 的 series=本次伤害；
      `src` = `MessageBodyWL.Tag1` = 命中者 ActorId）。不用 attacker 的 `SM_HIT`（无伤害数）；
      不用 DB 快照（怪物不入库、玩家 HP 只在断线存档落库，拿不到逐次）。
    - 命中/躲闪 = 按「第 k 次」：attacker 第 k 步发出后，受击方在下一步之前收到 `src==attacker`
      的 `SM_STRUCK` ⇒ hit；否则 dodge。比较 hits/dodges/attacked/sum 四项。
    - 两侧同种子（C# 用既有 `m3/rng-hook` 影子注入；Rust game-svr 侧种子为**前置**——不在本卡写集）。

  目标接法：`-AttackPinPair` 把 attacker（账号序号 0）钉在 (PinCX,PinCY) 面向 `AttackPinDir`，
  victim（序号 1）钉在 (PinCX+1,PinCY)；victim HP/MaxHP 抬高（防 100+ 次内死亡）。
  `-AttackSequence` 让 attacker 固定发 N 次 `CM_HIT`（`Recog=MakeLong(自己坐标)`、`Tag=dir`）。

  判红口径（逐步；任一不满足即退出码 != 0）：
    0 入口护栏   -Side rust 而入口仍是 C#（E:\MirServer）⇒ RED；-Side csharp 而入口不是 C# ⇒ RED。
    0.5 preflight `-Side rust` 时机判 Rust game-svr 是否已实现断线存档（whitelist T-27）；
                 未实现 ⇒ PREREQ（exit 2）。
    1 预检       入口端口不全在听 / MySQL 3306 不可读 ⇒ PREREQ（exit 2），不静默通过。
    2 同序列     run_pair -AttackSequence 后 bot.log 必须出现 `M3ATTSEQ`：`role=attack` k=1..N
                 且 `role=done attacks=N`；缺 ⇒ RED。
    3 跨侧口径   两侧 baseline.json 的 `attack_sequence` / `attack_pose` / `fixed_new_chr` 必须一致；
                 不一致 ⇒ PREREQ（exit 2）。
    4 逐次对拍   `attack_diff.py align` 两侧 ⇒ `attack_diff.py diff`；非 0 即 RED。
                 另 `scoped_diff.py diff --canonical --fixed-newchr`（DB 侧）非 0 即 RED。

  窗口纪律：除 -DiffOnly/-SelfCheckNoStack 外都会连真栈（5000/7000/7100/7200、MySQL 3306）。
  起栈前先看端口；与 M2 真机验收严格串行（**别并行起栈**）。本脚本不启动/不停止任何服务。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -Side rust -Bots 2 -AttackCount 120 -AttackPinPair -PeerSession csharp-...
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -DiffOnly -CsharpSession csharp-... -RustSession rust-...
  powershell -ExecutionPolicy Bypass -File run_attack_pair.ps1 -SelfCheckNoStack
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 2,
  [string]$Prefix = '',
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$TimeoutSec = 600,
  [int]$AttackCount = 120,
  [int]$AttackStepMs = 500,
  [int]$AttackStartDelayMs = 3000,
  [int]$AttackPinDir = 2,
  [int]$AttackVictimHp = 100000,
  [string]$PeerSession = '',
  [string]$CsharpSession = '',
  [string]$RustSession = '',
  [string]$PortsCsv = '',          # 入口端口（逗号分隔）；默认 5000,7000,7100,7200。自检时可指一列空闲端口
  [switch]$AttackPinPair,          # 攻击变体：两 bot 相邻固定起点（attacker/victim）
  [switch]$SkipSavePreflight,      # 仅供调试：跳过「Rust 侧断线存档未实现」前置
  [switch]$DiffOnly,
  [switch]$SelfCheckNoStack
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
$AttackDiff = Join-Path $Here 'attack_diff.py'
$RunBots = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$Sessions = Join-Path $Here 'sessions'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
# 攻击序列/起点口径与 peer 护栏的唯一实现（与 gate_guards 共用，自 dot-source）。
. (Join-Path $Here 'gate_guards.ps1')

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

# preflight（T-27）：bins/game-svr 是否已有 DB_SAVEHUMANRCD(101) 请求常量（断线存档）。
function Test-RustDisconnectSaveImplemented {
  param([string]$SrcDir = '')
  if ([string]::IsNullOrEmpty($SrcDir)) { $SrcDir = Join-Path $RepoRoot 'mir2-rs\bins\game-svr\src' }
  if (-not (Test-Path $SrcDir)) { return $false }
  $hits = @(Get-ChildItem $SrcDir -Recurse -Filter '*.rs' -ErrorAction SilentlyContinue |
            Select-String -Pattern 'DB_SAVEHUMANRCD' -SimpleMatch -ErrorAction SilentlyContinue)
  return ($hits.Count -gt 0)
}

# 把会话的 attack_raw.jsonl 对齐成 attack_seq.jsonl（幂等；不连网）。
function Invoke-AttackAlign([string]$Dir) {
  $raw = Join-Path $Dir 'attack_raw.jsonl'
  if (-not (Test-Path $raw)) { Write-Output ("PREREQ_MISSING: 会话缺 attack_raw.jsonl -> {0}" -f $raw); return 2 }
  $seq = Join-Path $Dir 'attack_seq.jsonl'
  & python $AttackDiff align $raw --out $seq
  return $LASTEXITCODE
}

$script:AttackDiffRc = 2
function Invoke-AttackCrossSideDiff([string]$csharpDir, [string]$rustDir, [string]$LocalScope, [string]$LocalPose, [string]$LocalScenario) {
  $script:AttackDiffRc = 2
  # ① 场景/开关/起点口径护栏（两侧同一输入才有仲裁力）。
  $scenarioGuard = Get-M3PeerScenarioGuard -PeerDir $csharpDir -LocalScenario $LocalScenario
  foreach ($m in $scenarioGuard.messages) { Write-Output $m }
  if (@($scenarioGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧建角场景不同口径（{0}）" -f (@($scenarioGuard.failed) -join ','))
    return
  }
  $scopeGuard = Get-M3PeerAttackScopeGuard -PeerDir $csharpDir -LocalScope $LocalScope
  foreach ($m in $scopeGuard.messages) { Write-Output $m }
  if (@($scopeGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧攻击序列开关不同口径（{0}）" -f (@($scopeGuard.failed) -join ','))
    return
  }
  Write-Output ("ATTACK_SCOPE_OK peer='{0}' local='{1}'" -f $scopeGuard.peer_scope, $LocalScope)
  $poseGuard = Get-M3PeerAttackPoseGuard -PeerDir $csharpDir -LocalPose $LocalPose
  foreach ($m in $poseGuard.messages) { Write-Output $m }
  if (@($poseGuard.failed).Count -gt 0) {
    Write-Output ("PREREQ_MISSING: 两侧攻击起点不同口径（{0}）" -f (@($poseGuard.failed) -join ','))
    return
  }
  Write-Output ("ATTACK_POSE_OK peer='{0}' local='{1}'" -f $poseGuard.peer_pose, $LocalPose)
  # ② 两侧各自 align（若尚未生成）。
  foreach ($d in @($csharpDir, $rustDir)) {
    $rc = Invoke-AttackAlign $d
    if ($rc -ne 0) { Write-Output ("PREREQ_MISSING: align 失败（rc={0}）: {1}" -f $rc, $d); return }
  }
  # ③ 逐次对拍（判据本体）。
  Write-Output '--- 跨侧攻击序列比对（attack_diff.py diff） ---'
  & python $AttackDiff diff (Join-Path $csharpDir 'attack_seq.jsonl') (Join-Path $rustDir 'attack_seq.jsonl')
  if ($LASTEXITCODE -ne 0) {
    Write-Output 'RED: 跨侧攻击序列 diff != 0（差异要么修到 0，要么按 §6.0-3 登记 tests/parity/whitelist.md）'
    $script:AttackDiffRc = 1
    return
  }
  # ④ DB 侧作用域 diff（复用既有判据）。
  Write-Output '--- 跨侧作用域字段级 diff（--canonical --fixed-newchr） ---'
  & python $Scoped diff (Join-Path $csharpDir 'scoped_after_norm.jsonl') (Join-Path $rustDir 'scoped_after_norm.jsonl') --canonical --fixed-newchr
  if ($LASTEXITCODE -eq 0) {
    Write-Output 'GREEN: 跨侧攻击序列 diff = 0 且作用域字段级 diff = 0'
    $script:AttackDiffRc = 0
    return
  }
  Write-Output 'RED: 跨侧作用域字段级 diff != 0'
  $script:AttackDiffRc = 1
}

# ===========================================================================
# Mode: SelfCheckNoStack —— 无栈自检（机器可判，不碰真端口）
# ===========================================================================
if ($SelfCheckNoStack) {
  Write-Output 'SELFTEST_NO_STACK: 无栈自检——所有路径都必须 fail-closed（缺失绝不静默 GREEN）'
  $failures = @()
  $tmpRoot = Join-Path $env:TEMP ('m3attack-selftest-' + (Get-Date -Format 'HHmmssfff'))
  New-Item -ItemType Directory -Force -Path $tmpRoot | Out-Null

  # ① 配置透传（T-22 扩展）：-AttackSequence/-AttackCount/... 必须落进 AppSetting.json；默认关。
  $cfgPath = Join-Path $RepoRoot 'src\BotSrv\bin\Release\AppSetting.json'
  & powershell -ExecutionPolicy Bypass -File $RunBots -AttackSequence -AttackCount 120 -AttackStepMs 500 -AttackStartDelayMs 3000 -WriteConfigOnly -SkipBuild | Out-Null
  if ($LASTEXITCODE -ne 0) {
    $failures += 'attackseq_config_on_writerc=non-zero'
  } else {
    $on = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
    if ($on.AttackSequence -ne $true) { $failures += 'attack_sequence_not_written_when_on' }
    if ($on.AttackCount -ne 120) { $failures += ('attack_count_on=' + $on.AttackCount) }
    Write-Output ("SELFTEST ① 配置透传 on=true? AttackSequence={0} AttackCount={1} StepMs={2}" -f $on.AttackSequence, $on.AttackCount, $on.AttackStepMs)
  }
  & powershell -ExecutionPolicy Bypass -File $RunBots -WriteConfigOnly -SkipBuild | Out-Null
  $off = (Get-Content $cfgPath -Raw | ConvertFrom-Json).BotPlay
  if ($off.AttackSequence -ne $false) { $failures += 'attack_sequence_default_not_off' }
  Write-Output ("SELFTEST ② 默认关 off=false? AttackSequence={0}" -f $off.AttackSequence)

  # ③ 无栈入口护栏：一列空闲端口 ⇒ 必须 PREREQ（exit 2），不能静默 GREEN。
  & powershell -ExecutionPolicy Bypass -File $MyInvocation.MyCommand.Path -Side rust -PortsCsv 17981,17982,17983,17984 | Out-Null
  $rcPort = $LASTEXITCODE
  if ($rcPort -ne 2) { $failures += ("no_stack_port_guard_rc=$rcPort(expect 2)") }
  Write-Output ("SELFTEST ③ 无栈入口护栏 rc={0}（期望 2=PREREQ）" -f $rcPort)

  # ④ -DiffOnly 缺会话 ⇒ PREREQ（exit 2）。
  & powershell -ExecutionPolicy Bypass -File $MyInvocation.MyCommand.Path -DiffOnly -CsharpSession '__none__' -RustSession '__none__' | Out-Null
  $rcDiff = $LASTEXITCODE
  if ($rcDiff -ne 2) { $failures += ("diffonly_missing_sessions_rc=$rcDiff(expect 2)") }
  Write-Output ("SELFTEST ④ -DiffOnly 缺会话 rc={0}（期望 2=PREREQ）" -f $rcDiff)

  # ⑤ run_pair 的开关耦合：-AttackSequence 必须一并传 -DisableAutoPlay；默认不得出现。
  $emitOnRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -AttackSequence -AttackCount 120 -AttackStepMs 500 -AttackStartDelayMs 3000
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_on_rc=non-zero' }
  else {
    $emitOn = (($emitOnRaw | Out-String).Trim() | ConvertFrom-Json)
    $has = (@($emitOn.bot_args) -contains '-DisableAutoPlay')
    if (-not $has) { $failures += 'attack_sequence_does_not_pass_disable_autoplay' }
    $wantScope = 'on count=120 step_ms=500 start_delay_ms=3000 disable_autoplay=True'
    if ($emitOn.attack_sequence -ne $wantScope) { $failures += ('attack_scope_on=' + $emitOn.attack_sequence) }
    Write-Output ("SELFTEST ⑤a run_pair -AttackSequence 传 -DisableAutoPlay? {0}（scope={1}）" -f $has, $emitOn.attack_sequence)
  }
  $emitOffRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_off_rc=non-zero' }
  else {
    $emitOff = (($emitOffRaw | Out-String).Trim() | ConvertFrom-Json)
    if (@($emitOff.bot_args) -contains '-DisableAutoPlay') { $failures += 'default_path_unexpectedly_passes_disable_autoplay' }
    if ($emitOff.attack_sequence -ne '(off)') { $failures += ('attack_scope_off=' + $emitOff.attack_sequence) }
    Write-Output ("SELFTEST ⑤b 默认路径不带 -DisableAutoPlay 且 scope=(off)：{0}" -f $emitOff.attack_sequence)
  }

  # ⑥ 攻击变体两相 pin：-AttackPinPair 的 attack_pose + 每 bot SQL（attacker/victim 相邻）。
  $emitPairRaw = & powershell -ExecutionPolicy Bypass -File $RunPair -EmitOpsArgs -FixedNewChr -PinStartPose -AttackPinPair -AttackSequence -Bots 2
  if ($LASTEXITCODE -ne 0) { $failures += 'emit_pair_rc=non-zero' }
  else {
    $pair = (($emitPairRaw | Out-String).Trim() | ConvertFrom-Json)
    $wantPose = 'pair map=0 attacker=289,618,dir=2 victim=290,618 level=1 vhp=100000'
    if ($pair.attack_pose -ne $wantPose) { $failures += ('attack_pose=' + $pair.attack_pose) }
    $all = @($pair.attack_pin_sql_all)
    if ($all.Count -lt 4) { $failures += ('attack_pin_sql_all_count=' + $all.Count) }
    $joined = ($all -join ' || ')
    if ($joined -notmatch 'ChrName=''m3[^'']*0''') { $failures += 'attack_pin_missing_attacker_row' }
    if ($joined -notmatch 'CX=290') { $failures += 'attack_pin_missing_victim_cell_290' }
    if ($joined -notmatch 'a\.Hp=100000, a\.MaxHP=100000') { $failures += 'attack_pin_missing_victim_hp' }
    Write-Output ("SELFTEST ⑥ -AttackPinPair pose='{0}'；pin_sql_all={1} 条" -f $pair.attack_pose, $all.Count)
  }

  # ⑦ attack_diff selftest：同序列绿 + 改坏 dmg/hit 必红 + 非平凡门 + 空序列。
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $attackStOut = (& python $AttackDiff selftest 2>&1 | Out-String)
  $attackStRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($attackStRc -ne 0) { $failures += ("attack_diff_selftest_rc=$attackStRc") }
  if ($attackStOut -notmatch 'SELFTEST GREEN') { $failures += 'attack_diff_selftest_not_green' }
  Write-Output ("SELFTEST ⑦ attack_diff selftest rc={0}" -f $attackStRc)

  # ⑧ 生产函数级「改坏必红」：合成两侧 raw → align → diff 绿；改坏一次 dmg ⇒ 红；改坏一次 hit ⇒ 红。
  $rawA = Join-Path $tmpRoot 'a.raw.jsonl'
  $rawB = Join-Path $tmpRoot 'b.raw.jsonl'
  $events = New-Object System.Collections.Generic.List[string]
  for ($k = 1; $k -le 10; $k++) {
    $events.Add('M3ATTSEQ {"role":"attack","k":' + $k + ',"op":"CM_HIT","x":289,"y":618,"dir":2}')
    if ($k -ne 3) { $events.Add('M3ATTSEQ {"role":"struck","dmg":' + (20 + ($k % 5)) + ',"hp":' + (100 - $k) + ',"maxhp":100,"src":101,"actor":77}') }
  }
  $events.Add('M3ATTSEQ {"role":"done","attacks":10}')
  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllLines($rawA, $events, $utf8NoBom)
  [System.IO.File]::WriteAllLines($rawB, $events, $utf8NoBom)
  $seqA = Join-Path $tmpRoot 'a.seq.jsonl'
  $seqB = Join-Path $tmpRoot 'b.seq.jsonl'
  & python $AttackDiff align $rawA --out $seqA | Out-Null
  $alignRc = $LASTEXITCODE
  & python $AttackDiff align $rawB --out $seqB | Out-Null
  if ($alignRc -ne 0) { $failures += ('align_synthetic_rc=' + $alignRc) }
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $greenOut = (& python $AttackDiff diff $seqA $seqB 2>&1 | Out-String)
  $ErrorActionPreference = $prevEap
  $greenRc = $LASTEXITCODE
  if ($greenRc -ne 0) { $failures += ('synthetic_identical_rc=' + $greenRc) }
  # 改坏 b 侧某一次 dmg（走生产的 `mutate` 子命令，不靠字符串替换）⇒ 必红
  & python $AttackDiff mutate $seqB --k 5 --field dmg --value 999 | Out-Null
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $dmgRedOut = (& python $AttackDiff diff $seqA $seqB 2>&1 | Out-String)
  $ErrorActionPreference = $prevEap
  $dmgRedRc = $LASTEXITCODE
  if ($dmgRedRc -ne 1) { $failures += ('dmg_mutation_not_red_rc=' + $dmgRedRc) }
  # 重生成 b.seq（清掉上一步），再改坏 k=5 的 hit（hit→dodge）⇒ 必红
  & python $AttackDiff align $rawB --out $seqB | Out-Null
  & python $AttackDiff mutate $seqB --k 5 --field hit --value 0 | Out-Null
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $hitRedOut = (& python $AttackDiff diff $seqA $seqB 2>&1 | Out-String)
  $ErrorActionPreference = $prevEap
  $hitRedRc = $LASTEXITCODE
  if ($hitRedRc -ne 1) { $failures += ('hit_mutation_not_red_rc=' + $hitRedRc) }
  # 绿的那次必须真的一行差异都没有（证明「必红」不是语法/BOM 引起的假红）
  if ($greenOut -notmatch 'GREEN') { $failures += 'synthetic_green_not_green' }
  if ($dmgRedOut -notmatch 'dmg 不等') { $failures += 'dmg_mutation_not_diff_shaped' }
  if ($hitRedOut -notmatch 'hit 不等') { $failures += 'hit_mutation_not_diff_shaped' }
  Write-Output 'SELFTEST ⑧ 合成两侧 → align/diff：'
  Write-Output ('  [绿] rc={0}: {1}' -f $greenRc, (($greenOut.Trim() -split "`n")[-1]))
  Write-Output ('  [改坏 dmg] rc={0}: {1}' -f $dmgRedRc, (($dmgRedOut.Trim() -split "`n") | Where-Object { $_ -match 'dmg 不等' } | Select-Object -First 1))
  Write-Output ('  [改坏 hit] rc={0}: {1}' -f $hitRedRc, (($hitRedOut.Trim() -split "`n") | Where-Object { $_ -match 'hit 不等' } | Select-Object -First 1))
  if ($greenRc -ne 0 -or $dmgRedRc -ne 1 -or $hitRedRc -ne 1) {
    Write-Output '  --- 原始输出（绿） ---'; Write-Output $greenOut
    Write-Output '  --- 原始输出（改坏 dmg） ---'; Write-Output $dmgRedOut
    Write-Output '  --- 原始输出（改坏 hit） ---'; Write-Output $hitRedOut
  }

  # ⑨ scoped_diff selftest 仍须绿（DB 侧判据未被削弱）。
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  $scopedOut = (& python $Scoped selftest 2>&1 | Out-String)
  $scopedRc = $LASTEXITCODE
  $ErrorActionPreference = $prevEap
  if ($scopedRc -ne 0) { $failures += ("scoped_diff_selftest_rc=$scopedRc") }
  if ($scopedOut -notmatch 'selftest 14/14') { $failures += 'scoped_diff_selftest_missing' }
  Write-Output ("SELFTEST ⑨ scoped_diff selftest rc={0}（14/14）" -f $scopedRc)

  Remove-Item -Recurse -Force $tmpRoot -ErrorAction SilentlyContinue
  if ($failures.Count -eq 0) {
    Write-Output 'SELFTEST_RED GREEN: 无栈自检全过（攻击配置透传/默认关 + run_pair -DisableAutoPlay 耦合两向 + 攻击两相 pin pose/SQL + 入口/DiffOnly fail-closed + attack_diff selftest + 改坏 dmg/hit 必红 + scoped_diff 14/14）'
    exit 0
  }
  Write-Output ('SELFTEST_RED RED: ' + ($failures -join ', '))
  exit 1
}

# ===========================================================================
# Mode: DiffOnly —— 只重比既有产物（不碰端口/库）
# ===========================================================================
if ($DiffOnly) {
  if ([string]::IsNullOrEmpty($CsharpSession) -or [string]::IsNullOrEmpty($RustSession)) {
    Write-Output 'PREREQ_MISSING: -DiffOnly 需要 -CsharpSession <名> 与 -RustSession <名>'
    exit 2
  }
  $cd = Join-Path $Sessions $CsharpSession
  $rd = Join-Path $Sessions $RustSession
  if (-not (Test-Path $cd)) { Write-Output ("PREREQ_MISSING: 找不到 C# 会话目录 {0}" -f $cd); exit 2 }
  if (-not (Test-Path $rd)) { Write-Output ("PREREQ_MISSING: 找不到 Rust 会话目录 {0}" -f $rd); exit 2 }
  $localScope = Get-M3SessionAttackScope -Dir $rd
  if ($null -eq $localScope) { Write-Output ("PREREQ_MISSING: Rust 会话读不出攻击序列开关: {0}" -f $rd); exit 2 }
  $localPose = Get-M3SessionAttackPose -Dir $rd
  if ($null -eq $localPose) { Write-Output ("PREREQ_MISSING: Rust 会话读不出攻击起点: {0}" -f $rd); exit 2 }
  $localScenario = (Get-Content (Join-Path $rd 'baseline.json') -Raw | ConvertFrom-Json).fixed_new_chr
  Invoke-AttackCrossSideDiff $cd $rd $localScope $localPose $localScenario
  exit $script:AttackDiffRc
}

# ===========================================================================
# Mode: 侧运行（csharp / rust）
# ===========================================================================
if ([string]::IsNullOrEmpty($Prefix)) {
  $Prefix = 'm3a' + (Get-Date -Format 'HHmmss')
}
if (-not $AttackPinPair) {
  Write-Output 'PREREQ_MISSING: 本脚本要求 -AttackPinPair（attacker/victim 必须相邻固定起点）——见卡 comment 度量口径'
  exit 2
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
  Write-Output 'RED: 入口端口被两种实现混占——先统一'
  exit 1
}

Write-Output '--- 0.5) preflight：Rust 断线存档（T-27） ---'
if ($Side -eq 'rust' -and -not $SkipSavePreflight) {
  if (-not (Test-RustDisconnectSaveImplemented)) {
    Write-Output 'PREREQ_MISSING: Rust game-svr 未实现断线/进世界存档（无 DB_SAVEHUMANRCD）——after 快照必是建角状态；见 whitelist T-27'
    exit 2
  }
  Write-Output 'PREFLIGHT Rust 断线存档：已实现'
} else {
  Write-Output 'PREFLIGHT Rust 断线存档：跳过'
}

Write-Output '--- 1) 预检：MySQL / 前缀 ---'
if (-not (Test-MysqlTcp)) {
  Write-Output 'PREREQ_MISSING: MySQL 127.0.0.1:3306 不可达'
  exit 2
}
Write-Output 'PREFLIGHT MySQL 在听'

Write-Output '--- 2) 侧运行（复用 run_pair.ps1；同序列 = -FixedNewChr + -AttackSequence + -AttackPinPair） ---'
$pairArgs = @('-Side', $Side, '-Bots', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port,
              '-TimeoutSec', $TimeoutSec, '-FixedNewChr', '-PinStartPose', '-AttackPinPair',
              '-AttackSequence', '-AttackCount', $AttackCount, '-AttackStepMs', $AttackStepMs, '-AttackStartDelayMs', $AttackStartDelayMs,
              '-AttackPinDir', $AttackPinDir, '-AttackVictimHp', $AttackVictimHp)
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

# 同序列判据：baseline.json 的 attack_markers 必须存在（run_pair 已机判完整串）。
$baseline = Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json
$markers = @($baseline.attack_markers)
if ($markers.Count -eq 0 -or -not (Test-Path (Join-Path $mine.FullName 'attack_raw.jsonl'))) {
  Write-Output ("RED: 同序列判据不成立（attack_markers={0} 条 / attack_raw.jsonl 缺失）——驱动没跑完整串" -f $markers.Count)
  exit 1
}
Write-Output ("ATTACKSEQ_OK markers={0} raw=True" -f $markers.Count)

$alignRc = Invoke-AttackAlign $mine.FullName
if ($alignRc -ne 0) {
  Write-Output ("RED: attack_diff align 失败（rc={0}）" -f $alignRc)
  exit 1
}

if ($Side -eq 'rust' -and -not [string]::IsNullOrEmpty($PeerSession)) {
  $peer = Join-Path $Sessions $PeerSession
  if (-not (Test-Path $peer)) { Write-Output ("PREREQ_MISSING: 找不到 C# 对照会话 {0}" -f $peer); exit 2 }
  $localScope = Format-M3AttackScope -AttackSequence $true -AttackCount $AttackCount -AttackStepMs $AttackStepMs -AttackStartDelayMs $AttackStartDelayMs -DisableAutoPlay $true
  $ownScope = Get-M3SessionAttackScope -Dir $mine.FullName
  if ($ownScope -ne $localScope) {
    Write-Output ("RED: 本侧 baseline.json 记录的开关与实际不一致——baseline='{0}' vs 实际='{1}'" -f $ownScope, $localScope)
    exit 1
  }
  $localPose = Get-M3SessionAttackPose -Dir $mine.FullName
  if ($null -eq $localPose) { Write-Output ("PREREQ_MISSING: 本侧会话读不出攻击起点: {0}" -f $mine.FullName); exit 2 }
  $localScenario = (Get-Content (Join-Path $mine.FullName 'baseline.json') -Raw | ConvertFrom-Json).fixed_new_chr
  Invoke-AttackCrossSideDiff $peer $mine.FullName $localScope $localPose $localScenario
  exit $script:AttackDiffRc
}

if ($Side -eq 'csharp') {
  Write-Output ("NEXT: 换 Rust 栈后跑  run_attack_pair.ps1 -Side rust -Bots {0} -AttackCount {1} -AttackPinPair -PeerSession {2}" -f $Bots, $AttackCount, $mine.Name)
} else {
  Write-Output 'NOTE: 未给 -PeerSession，跳过跨侧 diff（-Side rust 的正式判据要 C# 对照会话）'
}
Write-Output 'GREEN: 侧运行与同序列判据通过'
exit 0
