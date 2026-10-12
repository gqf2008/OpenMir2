#requires -Version 5
<#
.SYNOPSIS
  selftest_gate.ps1 — M3 门禁（run_m3_gate.ps1）机判护栏的「判红/判绿」自检。

.DESCRIPTION
  护栏的唯一口径在 `gate_guards.ps1`（主流程与自检共用同一批函数，照
  `tools/m2-acceptance/selftest.ps1` + `portclassify.ps1` 的先例）。本脚本用**构造输入**
  把每条口径都跑一遍，断言「失败项代码」而不只看"有没有报错"——只看报错会把"脚本崩了"
  误当"判红"。**全部离线**：不起栈、不占端口、不碰 DB。

    ① Get-M3EntryGuard（-Side rust 必须是已知 Rust 入口 + 挂在本端口期望入口上）
       已知 Rust 三入口 ⇒ 绿；C#（E:\MirServer）⇒ rust_side_entry_not_known_rust；
       未知进程 foo.exe（旧 impl!='csharp' 会放行）⇒ 红；已知 Rust 接错端口 ⇒ 红；
       端口没听（impl=none）⇒ entry_ports_not_listening；同一 C# 端口集在 -Side csharp 下仍绿。
    ② Get-M3SessionDirFault：$null ⇒ 'session_dir_missing'；目录对象 ⇒ 无 fault。
    ③ Get-M3PeerScenarioGuard：场景一致 ⇒ 绿；不一致 ⇒ peer_scenario_mismatch；
       缺 baseline.json ⇒ peer_baseline_missing；缺 fixed_new_chr ⇒ peer_fixed_new_chr_missing；
       baseline.json 坏/半写（截断、非法 JSON）⇒ peer_baseline_invalid（**结构化判红、不抛异常中断**，
       P-1：reviewer-5 `3926cf99` / reviewer-3 `56f02a6e`）。
    ④ Get-M3PeerBagEquipScopeGuard（T-14 复跑口径）：两侧 baseline.json.bag_equip_sequence 必须
       同一组开关（step_ms/start_delay_ms/disable_autoplay）。旧「开着挂机」产物（无 disable_autoplay
       记录）⇒ peer_bag_equip_autoplay_on；step_ms 等不同 ⇒ peer_bag_equip_scope_mismatch；
       本侧 on + peer 缺字段 ⇒ peer_bag_equip_scope_missing；两侧 off ⇒ 绿。
    ⑤ Get-M3PeerStartPoseGuard（T-14 固定起点变体）：两侧 baseline.json.start_pose 必须逐字节相同；
       不同 ⇒ peer_start_pose_mismatch；本侧 pin + peer 缺字段 ⇒ peer_start_pose_missing；
       形态认不出 ⇒ peer_start_pose_invalid；两侧 none ⇒ 绿。

    ⑥ Get-M3PinHoldVerdict / Invoke-M3PinSettleHold（T-14 两相 pin 的 settle+hold；owner 2026-10-12 08:20）：
       跳过 settle 直接 pin ⇒ pin_settle_skipped；单次观测 ⇒ pin_hold_needs_consecutive_settle（不 hold）；
       连续两轮 settle 后到齐 ⇒ hold；存档覆盖 ⇒ pin_char/bag/abil_mismatch；循环本体的故障注入验证。

    ⑦ Get-M3SchemaColumns / Test-M3PinSqlColumns（pin SQL 别名/列存在性，不连库；m3cs tick16 真机 finding）：
       从冻结 schema `sql/mir2_db.sql` 解表列，校验 pin SQL 的 `alias.column`；`characters_ablity`
       无 `IncHealth`（属 `characters`）⇒ 历史 bug `a.IncHealth` 必被检出（红检）。

    ⑧ Test-M3SeedInjected（F-C：两侧种子注入证据断言，离线）：C# 读 `INJECT_OK <seed>`；Rust 读 `SEED_PROBE`
       行（另校 `RUST_SEED_INJECTED`）；缺失/种子不符/空路径 ⇒ ok=$false（fail-closed）。

    ⑨ Get-M3PinAttemptsGuard / Test-M3PinAttemptsComparable（pin 重试账本；owner 2026-10-12 16:30；
       **19:20 降级为 informational**）：只对「键在但值空/`(off)`、形态认不出、baseline 坏 JSON、会话缺失」
       fail-closed；attempts=2/4/1、expected 漂移、**旧会话（根本没有 pin_attempts 键）均绿**（只进 informational）。

    ⑩ Test-M3PrewarmNoWorldEntry（预热相不进世界；owner 2026-10-12 19:20 根因修；reviewer-6 M-1 正向断言）：
       读预热相 BotSrv 日志：出现 `进入游戏`/`成功进入游戏`/`准备进入游戏` ⇒ fail-closed；日志缺失/不可读也 fail-closed；
       **日志存在但零 marker（既无进世界标记、也无 `创建角色`/`预热建角完成` 正证）⇒ `prewarm_marker_missing` fail-closed**
       （M-1：否则「预热压根没跑到建角」会被误当合格）。
       （根因：预热假人进世界 → 被 Stop-Process ⇒ GameSvr `SaveHumanRcd` 异步落库覆盖 pin，
       见 `src/GameSrv/Word/WorldServer.cs:711-713` + `:1077-1091`。）

  「改坏必红 / 还原必绿」：把 `gate_guards.ps1` 对应护栏改回旧行为（放行）再跑本脚本，
  必须 exit 1；还原后必须 exit 0。原始输出落 `E:\tmp\openmir2-watch\`。

.PARAMETER OutDir
  证据/夹具目录（默认 `E:\tmp\openmir2-m3gs\selftest-<时间戳>`）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/selftest_gate.ps1
#>
param(
  [string]$OutDir = ''
)

$ErrorActionPreference = 'Stop'
$Here = $PSScriptRoot
# 生产护栏：与 run_m3_gate.ps1 共用同一文件（改坏本文件即可验证自检有牙齿）
. (Join-Path $Here 'gate_guards.ps1')
if ([string]::IsNullOrEmpty($OutDir)) {
  $OutDir = Join-Path 'E:\tmp\openmir2-m3gs' ('selftest-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Work = Join-Path $OutDir 'work'
New-Item -ItemType Directory -Force -Path $Work | Out-Null

$script:Total = 0
$script:Failed = 0
$script:Checks = New-Object System.Collections.Generic.List[object]

function Test-Case([string]$Name, [bool]$Ok, [string]$Detail) {
  $script:Total++
  if (-not $Ok) { $script:Failed++ }
  $tag = if ($Ok) { 'OK  ' } else { 'FAIL' }
  Write-Output ("  [{0}] {1,-52} {2}" -f $tag, $Name, $Detail)
  $script:Checks.Add([pscustomobject]@{ name = $Name; ok = $Ok; detail = $Detail })
}

function Test-EntryGuard([string]$Desc, [object[]]$Holders, [string]$Side, [int]$ExpectFailed, [string]$ExpectCode) {
  $g = Get-M3EntryGuard -Holders $Holders -Side $Side
  $codes = @($g.failed)
  $ok = ($codes.Count -eq $ExpectFailed) -and ([string]::IsNullOrEmpty($ExpectCode) -or ($codes -contains $ExpectCode))
  Test-Case ("entry/{0}" -f $Desc) $ok ("side={0} failed=[{1}]" -f $Side, ($codes -join ','))
}

function New-Baseline([string]$Dir, [string]$FixedNewChr, [switch]$OmitField, [string]$BagEquipSequence = '', [string]$StartPose = '', [string]$PinAttempts = '', [switch]$EmptyPinAttempts) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  $obj = [ordered]@{ session = (Split-Path $Dir -Leaf); side = 'csharp' }
  if (-not $OmitField) { $obj['fixed_new_chr'] = $FixedNewChr }
  if (-not [string]::IsNullOrEmpty($BagEquipSequence)) { $obj['bag_equip_sequence'] = $BagEquipSequence }
  if (-not [string]::IsNullOrEmpty($StartPose)) { $obj['start_pose'] = $StartPose }
  # -EmptyPinAttempts：键在但值为空（=仪器写了字段却没落有效记账）——与「键不存在（旧会话）」区分开。
  if ($EmptyPinAttempts) { $obj['pin_attempts'] = '' }
  elseif (-not [string]::IsNullOrEmpty($PinAttempts)) { $obj['pin_attempts'] = $PinAttempts }
  $obj | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $Dir 'baseline.json') -Encoding utf8
}

$RustDir = 'E:\tmp\openmir2-m3gs-target\debug'
function New-RustHolder([int]$Port, [string]$Name) {
  return New-M3EntryHolder -Port $Port -ProcessName ($Name + '.exe') -Path (Join-Path $RustDir ($Name + '.exe'))
}

Write-Output ("M3 门禁护栏自检  evidence=" + $OutDir)

# ---------------------------------------------------------------- ③ 场景标签口径
Write-Output '③ Format-M3Scenario（两侧 baseline.json 的 fixed_new_chr 同形态）'
Test-Case 'scenario/固定=Job=1 Hair=2 Sex=0' ((Format-M3Scenario -FixedNewChr $true -NewChrJob 1 -NewChrHair 2 -NewChrSex 0) -eq 'Job=1 Hair=2 Sex=0') ''
Test-Case 'scenario/默认=(off)' ((Format-M3Scenario -FixedNewChr $false) -eq '(off)') ''

# ---------------------------------------------------------------- ① 入口护栏
Write-Output '① Get-M3EntryGuard（-Side rust 必须是已知 Rust 入口 + 期望端口）'
$goodRust = @((New-RustHolder 7000 'login-gate'), (New-RustHolder 7100 'sel-gate'), (New-RustHolder 7200 'game-gate'))
Test-EntryGuard '已知Rust三入口=绿' $goodRust 'rust' 0 ''

$csharp = @(
  (New-M3EntryHolder -Port 7000 -ProcessName 'LoginGate.exe' -Path 'E:\MirServer\LoginGate\LoginGate.exe'),
  (New-M3EntryHolder -Port 7100 -ProcessName 'SelGate.exe' -Path 'E:\MirServer\SelGate\SelGate.exe'),
  (New-M3EntryHolder -Port 7200 -ProcessName 'GameGate.exe' -Path 'E:\MirServer\RunGate\GameGate.exe')
)
Test-EntryGuard 'C#实现@rust侧=红' $csharp 'rust' 1 'rust_side_entry_not_known_rust'
Test-EntryGuard 'C#实现@csharp侧=绿' $csharp 'csharp' 0 ''

$unknown = @(
  (New-M3EntryHolder -Port 7000 -ProcessName 'foo.exe' -Path 'C:\dev\foo.exe'),
  (New-RustHolder 7100 'sel-gate'),
  (New-RustHolder 7200 'game-gate')
)
Test-EntryGuard '未知进程foo.exe=红(旧护栏放行)' $unknown 'rust' 1 'rust_side_entry_not_known_rust'

$cross = @((New-RustHolder 7000 'game-gate'), (New-RustHolder 7100 'sel-gate'), (New-RustHolder 7200 'login-gate'))
Test-EntryGuard '已知Rust接错端口=红(旧护栏放行)' $cross 'rust' 1 'rust_side_entry_not_known_rust'

$none = @((New-M3EntryHolderNone -Port 7000), (New-RustHolder 7100 'sel-gate'), (New-RustHolder 7200 'game-gate'))
Test-EntryGuard '端口没听=红' $none 'rust' 1 'entry_ports_not_listening'

# ---------------------------------------------------------------- ② sessionDir 空值防线
Write-Output '② Get-M3SessionDirFault（本侧会话目录空值防线）'
$sdDir = Join-Path $Work 'session-ok'
New-Item -ItemType Directory -Force -Path $sdDir | Out-Null
Test-Case 'sessionDir/null=session_dir_missing' ((Get-M3SessionDirFault -SessionDir $null) -eq 'session_dir_missing') 'null'
Test-Case 'sessionDir/目录对象=无fault' ($null -eq (Get-M3SessionDirFault -SessionDir (Get-Item $sdDir))) 'DirectoryInfo'

# ---------------------------------------------------------------- ③ peer 场景一致性
Write-Output '③ Get-M3PeerScenarioGuard（参照会话建角场景一致性）'
$pMatch = Join-Path $Work 'peer-match'
New-Baseline $pMatch 'Job=1 Hair=2 Sex=0'
$g = Get-M3PeerScenarioGuard -PeerDir $pMatch -LocalScenario 'Job=1 Hair=2 Sex=0'
Test-Case 'peer/场景一致=绿' ((@($g.failed).Count -eq 0) -and ($g.peer_scenario -eq 'Job=1 Hair=2 Sex=0')) `
  ("failed=[{0}] peer='{1}'" -f (@($g.failed) -join ','), $g.peer_scenario)

$g = Get-M3PeerScenarioGuard -PeerDir $pMatch -LocalScenario '(off)'
Test-Case 'peer/固定vs默认=红' (@($g.failed) -contains 'peer_scenario_mismatch') ("failed=[{0}]" -f (@($g.failed) -join ','))

$pOff = Join-Path $Work 'peer-off'
New-Baseline $pOff '(off)'
$g = Get-M3PeerScenarioGuard -PeerDir $pOff -LocalScenario '(off)'
Test-Case 'peer/两侧都off=绿' (@($g.failed).Count -eq 0) ("failed=[{0}]" -f (@($g.failed) -join ','))

$pNoBase = Join-Path $Work 'peer-nobase'
New-Item -ItemType Directory -Force -Path $pNoBase | Out-Null
$g = Get-M3PeerScenarioGuard -PeerDir $pNoBase -LocalScenario '(off)'
Test-Case 'peer/缺baseline.json=红' (@($g.failed) -contains 'peer_baseline_missing') ("failed=[{0}]" -f (@($g.failed) -join ','))

$pNoField = Join-Path $Work 'peer-nofield'
New-Baseline $pNoField '' -OmitField
$g = Get-M3PeerScenarioGuard -PeerDir $pNoField -LocalScenario '(off)'
Test-Case 'peer/缺fixed_new_chr=红' (@($g.failed) -contains 'peer_fixed_new_chr_missing') ("failed=[{0}]" -f (@($g.failed) -join ','))

# P-1（reviewer-5 `3926cf99` / reviewer-3 `56f02a6e`）：peer baseline.json 被写坏/半写（截断、非法 JSON）时，
# 旧写法在 $ErrorActionPreference='Stop' 下抛异常，run_m3_gate.ps1 在落 RED 报告前中断（fail-open）。
# 本用例断言：函数**不抛异常**、返回结构化 RED（failed 含 peer_baseline_invalid 且 message 含「解析失败」）。
# 注意 try/catch 只用来让「函数抛异常」表现为**该用例 FAIL**（而不是整个自检崩掉），这样改坏必红落在这条用例上。
$pBadJson = Join-Path $Work 'peer-badjson'
New-Item -ItemType Directory -Force -Path $pBadJson | Out-Null
Set-Content -Path (Join-Path $pBadJson 'baseline.json') -Encoding utf8 `
  -Value '{ "session": "peer-badjson", "side": "csharp", "fixed_new_chr": "(off'
$g = $null
$badJsonThrew = $null
try {
  $g = Get-M3PeerScenarioGuard -PeerDir $pBadJson -LocalScenario '(off)'
} catch {
  $badJsonThrew = $_
}
$badJsonCodes = if ($g) { @($g.failed) } else { @() }
$badJsonMessages = if ($g) { (@($g.messages) -join ' ') } else { '' }
# PS5.1 的 if 是语句不是表达式 ⇒ 先算 detail，再传参（不要写成 (if …)）。
$badJsonDetail = if ($badJsonThrew) { 'THREW: ' + $badJsonThrew.Exception.Message } else { "failed=[{0}]" -f ($badJsonCodes -join ',') }
Test-Case 'peer/baseline.json坏JSON=红(不中断)' `
  (($null -eq $badJsonThrew) -and ($badJsonCodes -contains 'peer_baseline_invalid') -and ($badJsonMessages -match '解析失败')) `
  $badJsonDetail

# ---------------------------------------------------------------- ④ peer 背包/装备开关一致性
Write-Output '④ Get-M3PeerBagEquipScopeGuard（参照会话背包/装备开关一致性；T-14 复跑口径）'
$LocalOn = Format-M3BagEquipScope -BagEquipSequence $true -BagEquipStepMs 1000 -BagEquipStartDelayMs 3000 -DisableAutoPlay $true

$bOn = Join-Path $Work 'bag-on'
New-Baseline $bOn 'Job=1 Hair=2 Sex=0' -BagEquipSequence $LocalOn
$g = Get-M3PeerBagEquipScopeGuard -PeerDir $bOn -LocalScope $LocalOn
Test-Case 'bagscope/两侧同开关(关挂机)=绿' ((@($g.failed).Count -eq 0) -and ($g.peer_scope -eq $LocalOn)) `
  ("failed=[{0}] peer='{1}'" -f (@($g.failed) -join ','), $g.peer_scope)

# 旧产物：只有 `on step_ms=... start_delay_ms=...`，无 disable_autoplay 记录（= 开着挂机跑的）。
# 这是本机入库的 csharp-20261012-054849 的形状 —— 不能当同口径 peer。
$bLegacy = Join-Path $Work 'bag-legacy-autoplay'
New-Baseline $bLegacy 'Job=1 Hair=2 Sex=0' -BagEquipSequence 'on step_ms=1000 start_delay_ms=3000'
$g = Get-M3PeerBagEquipScopeGuard -PeerDir $bLegacy -LocalScope $LocalOn
Test-Case 'bagscope/旧开挂机peer=红(autoplay_on)' (@($g.failed) -contains 'peer_bag_equip_autoplay_on') `
  ("failed=[{0}] peer='{1}'" -f (@($g.failed) -join ','), $g.peer_scope)

$bStep = Join-Path $Work 'bag-stepmismatch'
New-Baseline $bStep 'Job=1 Hair=2 Sex=0' -BagEquipSequence 'on step_ms=500 start_delay_ms=3000 disable_autoplay=True'
$g = Get-M3PeerBagEquipScopeGuard -PeerDir $bStep -LocalScope $LocalOn
Test-Case 'bagscope/step_ms不同=红' (@($g.failed) -contains 'peer_bag_equip_scope_mismatch') `
  ("failed=[{0}]" -f (@($g.failed) -join ','))

$bOff = Join-Path $Work 'bag-off'
New-Baseline $bOff '(off)' -BagEquipSequence '(off)'
$g = Get-M3PeerBagEquipScopeGuard -PeerDir $bOff -LocalScope '(off)'
Test-Case 'bagscope/两侧off=绿' (@($g.failed).Count -eq 0) ("failed=[{0}]" -f (@($g.failed) -join ','))

$bNoBag = Join-Path $Work 'bag-nofield'
New-Baseline $bNoBag '(off)'
$g = Get-M3PeerBagEquipScopeGuard -PeerDir $bNoBag -LocalScope $LocalOn
Test-Case 'bagscope/本侧on+peer缺字段=红' (@($g.failed) -contains 'peer_bag_equip_scope_missing') `
  ("failed=[{0}]" -f (@($g.failed) -join ','))
$g = Get-M3PeerBagEquipScopeGuard -PeerDir $bNoBag -LocalScope '(off)'
Test-Case 'bagscope/本侧off+peer缺字段=绿(旧产物默认关)' (@($g.failed).Count -eq 0) `
  ("failed=[{0}]" -f (@($g.failed) -join ','))

# 坏 JSON：与 ③ 同类（peer_baseline_invalid，不抛异常中断）
$bBadBag = Join-Path $Work 'bag-badjson'
New-Item -ItemType Directory -Force -Path $bBadBag | Out-Null
Set-Content -Path (Join-Path $bBadBag 'baseline.json') -Encoding utf8 `
  -Value '{ "session": "bag-badjson", "side": "csharp", "bag_equip_sequence": "on step'
$g = $null
$badBagThrew = $null
try { $g = Get-M3PeerBagEquipScopeGuard -PeerDir $bBadBag -LocalScope $LocalOn } catch { $badBagThrew = $_ }
$badBagCodes = if ($g) { @($g.failed) } else { @() }
$badBagDetail = if ($badBagThrew) { 'THREW: ' + $badBagThrew.Exception.Message } else { "failed=[{0}]" -f ($badBagCodes -join ',') }
Test-Case 'bagscope/baseline.json坏JSON=红(不中断)' `
  (($null -eq $badBagThrew) -and ($badBagCodes -contains 'peer_baseline_invalid')) `
  $badBagDetail

# ---------------------------------------------------------------- ⑤ peer 固定起点一致性
Write-Output '⑤ Get-M3PeerStartPoseGuard（T-14 固定起点变体；owner 86478c55/d7798370）'
Test-Case 'pose/口径形态=map=0 home=0,289,618 pos=289,618 level=1 dir=0 bag=48@0,97@1,207@2,3@3 abil=1' `
  ((Format-M3StartPose -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -CX 289 -CY 618 -Level 1 -Dir 0) -eq 'map=0 home=0,289,618 pos=289,618 level=1 dir=0 bag=48@0,97@1,207@2,3@3 abil=1') ''
$LocalPose = Format-M3StartPose -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -CX 289 -CY 618 -Level 1 -Dir 0
$pPin = Join-Path $Work 'pose-pin'
New-Baseline $pPin '(off)' -StartPose $LocalPose
$g = Get-M3PeerStartPoseGuard -PeerDir $pPin -LocalPose $LocalPose
Test-Case 'pose/两侧同 pin=绿' ((@($g.failed).Count -eq 0) -and ($g.peer_pose -eq $LocalPose)) `
  ("failed=[{0}] peer='{1}'" -f (@($g.failed) -join ','), $g.peer_pose)
$pPin2 = Join-Path $Work 'pose-pin2'
New-Baseline $pPin2 '(off)' -StartPose (Format-M3StartPose -MapName '0' -HomeMap 0 -HomeX 289 -HomeY 618 -CX 650 -CY 618 -Level 1 -Dir 0)
$g = Get-M3PeerStartPoseGuard -PeerDir $pPin2 -LocalPose $LocalPose
Test-Case 'pose/peer pin 不同=红' (@($g.failed) -contains 'peer_start_pose_mismatch') `
  ("failed=[{0}]" -f (@($g.failed) -join ','))
$pNoPose = Join-Path $Work 'pose-nofield'
New-Baseline $pNoPose '(off)'
$g = Get-M3PeerStartPoseGuard -PeerDir $pNoPose -LocalPose $LocalPose
Test-Case 'pose/本侧 pin+peer 缺字段=红' (@($g.failed) -contains 'peer_start_pose_missing') `
  ("failed=[{0}]" -f (@($g.failed) -join ','))
$g = Get-M3PeerStartPoseGuard -PeerDir $pNoPose -LocalPose 'none'
Test-Case 'pose/两侧 none=绿' (@($g.failed).Count -eq 0) ("failed=[{0}]" -f (@($g.failed) -join ','))
$pBadPose = Join-Path $Work 'pose-invalid'
New-Baseline $pBadPose '(off)' -StartPose 'garbage'
$g = Get-M3PeerStartPoseGuard -PeerDir $pBadPose -LocalPose $LocalPose
Test-Case 'pose/形态认不出=红' (@($g.failed) -contains 'peer_start_pose_invalid') `
  ("failed=[{0}]" -f (@($g.failed) -join ','))

# ---------------------------------------------------------------- ⑥ pin settle+hold 判据
Write-Output '⑥ Get-M3PinHoldVerdict / Invoke-M3PinSettleHold（T-14 两相 pin 的 settle+hold；owner 2026-10-12 08:20）'
# 形态标签（无栈自检据此机判生产接线是否被削弱：settle_seconds>=1、consecutive>=2）
Test-Case 'pin/仪器形态=settle_seconds=3 max_attempts=5 consecutive=2' `
  ((Format-M3PinInstrument) -eq 'settle_seconds=3 max_attempts=5 consecutive=2') ''
# 跳过 settle 直接 pin ⇒ 必判红（假 GREEN 被拦）
$vSkip = Get-M3PinHoldVerdict -SettleRounds 0 -PreviousMatch $true -Pinned 1 -BagPinned 4 -AbilPinned 1 -Bots 1
Test-Case 'pin/跳过 settle=红(pin_settle_skipped)' ((@($vSkip.failed) -contains 'pin_settle_skipped') -and (-not $vSkip.hold)) `
  ("hold={0} failed=[{1}]" -f $vSkip.hold, (@($vSkip.failed) -join ','))
# 单次观测（pin 早于落库静默）⇒ 不足 hold，必须再来一轮（连续两轮）
$vOnce = Get-M3PinHoldVerdict -SettleRounds 1 -PreviousMatch $false -Pinned 1 -BagPinned 4 -AbilPinned 1 -Bots 1
Test-Case 'pin/单次观测=红(needs_consecutive)' ((@($vOnce.failed) -contains 'pin_hold_needs_consecutive_settle') -and (-not $vOnce.hold)) `
  ("hold={0} failed=[{1}]" -f $vOnce.hold, (@($vOnce.failed) -join ','))
# 连续两轮 settle 后到位 ⇒ 绿
$vHold = Get-M3PinHoldVerdict -SettleRounds 2 -PreviousMatch $true -Pinned 1 -BagPinned 4 -AbilPinned 1 -Bots 1
Test-Case 'pin/连续两轮到齐=绿(hold)' ($vHold.hold -and (@($vHold.failed).Count -eq 0)) `
  ("hold={0} failed=[{1}]" -f $vHold.hold, (@($vHold.failed) -join ','))
# 存档覆盖 pin（坐标/背包/Abil 不符）⇒ 红
$vOver = Get-M3PinHoldVerdict -SettleRounds 2 -PreviousMatch $true -Pinned 0 -BagPinned 0 -AbilPinned 0 -Bots 1
Test-Case 'pin/存档覆盖=红(pin_char/bag/abil_mismatch)' ((@($vOver.failed) -contains 'pin_char_mismatch') -and (@($vOver.failed) -contains 'pin_bag_mismatch') -and (@($vOver.failed) -contains 'pin_abil_mismatch') -and (-not $vOver.hold)) `
  ("hold={0} failed=[{1}]" -f $vOver.hold, (@($vOver.failed) -join ','))
# 循环本体：故障注入「跳过 settle」⇒ Hold=$false（同一函数即生产用的那个）
$loopSkip = Invoke-M3PinSettleHold -Bots 1 -MaxAttempts 3 `
  -Pin { param($c, $a) } `
  -Settle { param($c, $a) return $false } `
  -Observe { param($c, $a) return [pscustomobject]@{ Pinned = 1; BagPinned = 4; AbilPinned = 1 } }
Test-Case 'pin/循环·跳过 settle=红(fail-closed)' (-not $loopSkip.Hold) ("hold={0} attempts={1}" -f $loopSkip.Hold, $loopSkip.Attempts)
# 循环本体：正常路径 ⇒ attempt=2（连续两轮）
$loopOk = Invoke-M3PinSettleHold -Bots 1 -MaxAttempts 5 `
  -Pin { param($c, $a) } `
  -Settle { param($c, $a) Start-Sleep -Milliseconds 1; return $true } `
  -Observe { param($c, $a) return [pscustomobject]@{ Pinned = 1; BagPinned = 4; AbilPinned = 1 } }
Test-Case 'pin/循环·正常=绿(attempt=2)' ($loopOk.Hold -and $loopOk.Attempts -eq 2 -and $loopOk.SettleRounds -eq 2) `
  ("hold={0} attempts={1} settle_rounds={2}" -f $loopOk.Hold, $loopOk.Attempts, $loopOk.SettleRounds)

# ---------------------------------------------------------------- ⑦ pin SQL 别名/列存在性（不连库）
Write-Output '⑦ Get-M3SchemaColumns / Test-M3PinSqlColumns（pin SQL 别名/列；不连库；m3cs tick16 finding）'
$schemaCols = Get-M3SchemaColumns -SchemaPath (Join-Path $script:M3GuardsRepoRoot 'sql\mir2_db.sql')
Test-Case 'schema/characters 有 IncHealth/IncSpell' `
  ($schemaCols['characters'].Contains('IncHealth') -and $schemaCols['characters'].Contains('IncSpell')) ''
Test-Case 'schema/characters_ablity 无 IncHealth' (-not $schemaCols['characters_ablity'].Contains('IncHealth')) ''
$goodPin = @(Get-M3StartPosePinSql -ChrName 'm3gs0') + @(Get-M3AttackPinSql -ChrName 'm3gs0' -CX 289 -CY 618 -Dir 2 -MapName '0')
$goodVio = @(Test-M3PinSqlColumns -SqlStatements $goodPin -SchemaColumns $schemaCols)
Test-Case 'pin/生产 SQL 别名列全对=绿(0 violation)' ($goodVio.Count -eq 0) ("violations=[{0}]" -f ($goodVio -join ';'))
$badPin = @(Get-M3AttackPinSql -ChrName 'm3gs0' -CX 289 -CY 618 -Dir 2 -MapName '0' |
  ForEach-Object { $_ -replace 'c\.IncHealth=0, c\.IncSpell=0', 'a.IncHealth=0, a.IncSpell=0' })
$badPinVio = @(Test-M3PinSqlColumns -SqlStatements $badPin -SchemaColumns $schemaCols)
Test-Case 'pin/改坏 a.IncHealth=红' (($badPinVio.Count -gt 0) -and (($badPinVio -join ';') -match 'a\.IncHealth')) `
  ("violations=[{0}]" -f ($badPinVio -join ';'))

# ---------------------------------------------------------------- ⑧ 种子注入证据断言（F-C，离线）
Write-Output '⑧ Test-M3SeedInjected（两侧种子注入证据；不连库；owner 2026-10-12 11:55 F-C）'
$csGood = Join-Path $Work 'f-c-cs-good.tsv'
Set-Content -LiteralPath $csGood -Encoding ascii -Value "1`tINJECT_OK`t42`t0"
$csBad = Join-Path $Work 'f-c-cs-bad.tsv'
Set-Content -LiteralPath $csBad -Encoding ascii -Value "123`tNext(max)`t20`t5"
$csWrong = Join-Path $Work 'f-c-cs-wrong.tsv'
Set-Content -LiteralPath $csWrong -Encoding ascii -Value "1`tINJECT_OK`t7`t0"
$rsGood = Join-Path $Work 'f-c-rs-good.log'
Set-Content -LiteralPath $rsGood -Encoding utf8 -Value '12:00:00 [INF] RNG 同种子注入：seed=42 参照序列=SEED_PROBE 1434747710 302596119'
$rsBad = Join-Path $Work 'f-c-rs-bad.log'
Set-Content -LiteralPath $rsBad -Encoding utf8 -Value '12:00:00 [INF] 世界已加载'
$rsMismatch = Join-Path $Work 'f-c-rs-mismatch.log'
Set-Content -LiteralPath $rsMismatch -Encoding utf8 -Value "RUST_SEED_INJECTED=7`nRNG 同种子注入：seed=42 参照序列=SEED_PROBE 1434747710"
$csGoodR = Test-M3SeedInjected -Side csharp -LogPath $csGood -ExpectSeed 42
$csBadR = Test-M3SeedInjected -Side csharp -LogPath $csBad -ExpectSeed 42
$csWrongR = Test-M3SeedInjected -Side csharp -LogPath $csWrong -ExpectSeed 42
$csEmptyR = Test-M3SeedInjected -Side csharp -LogPath '' -ExpectSeed 42
$rsGoodR = Test-M3SeedInjected -Side rust -LogPath $rsGood -ExpectSeed 42
$rsBadR = Test-M3SeedInjected -Side rust -LogPath $rsBad -ExpectSeed 42
$rsMismatchR = Test-M3SeedInjected -Side rust -LogPath $rsMismatch -ExpectSeed 42
Test-Case 'seed/csharp INJECT_OK 42=绿' ($csGoodR.ok -and $csGoodR.seed -eq 42) ''
Test-Case 'seed/csharp 无 INJECT_OK=红' (-not $csBadR.ok) ("reason={0}" -f $csBadR.reason)
Test-Case 'seed/csharp 种子 7!=42=红' (-not $csWrongR.ok) ("reason={0}" -f $csWrongR.reason)
Test-Case 'seed/csharp 空路径=红(fail-closed)' (-not $csEmptyR.ok) ("reason={0}" -f $csEmptyR.reason)
Test-Case 'seed/rust SEED_PROBE 42=绿' ($rsGoodR.ok -and $rsGoodR.seed -eq 42) ''
Test-Case 'seed/rust 无 SEED_PROBE=红' (-not $rsBadR.ok) ("reason={0}" -f $rsBadR.reason)
Test-Case 'seed/rust 启动器行 7!=42=红' (-not $rsMismatchR.ok) ("reason={0}" -f $rsMismatchR.reason)

# ---------------------------------------------------------------- ⑨ pin 重试账本（owner 2026-10-12 16:30；19:20 降级 informational）
Write-Output '⑨ Get-M3PinAttemptsGuard / Test-M3PinAttemptsComparable（pin 重试账本；informational，owner 19:20）'
Test-Case 'pinacct/informational 基准常量 >= 1' ((Get-M3PinExpectedAttempts) -ge 1) ("expected={0}" -f (Get-M3PinExpectedAttempts))
Test-Case 'pinacct/形态 attempts=2 expected=2 settle_rounds=2 max_attempts=5' `
  ((Format-M3PinAttempts -Attempts 2 -SettleRounds 2 -MaxAttempts 5) -eq 'attempts=2 expected=2 settle_rounds=2 max_attempts=5') ''
$paOk = Join-Path $Work 'pin-ok'
New-Baseline $paOk '(off)' -PinAttempts (Format-M3PinAttempts -Attempts 2 -SettleRounds 2 -MaxAttempts 5)
$g = Get-M3PinAttemptsGuard -PlayerDir $paOk
Test-Case 'pinacct/attempts=2=绿' (@($g.failed).Count -eq 0) ("failed=[{0}]" -f (@($g.failed) -join ','))
$paRetry = Join-Path $Work 'pin-retry'
New-Baseline $paRetry '(off)' -PinAttempts (Format-M3PinAttempts -Attempts 4 -SettleRounds 4 -MaxAttempts 5)
$g = Get-M3PinAttemptsGuard -PlayerDir $paRetry
Test-Case 'pinacct/attempts=4=绿(informational，不再拒比)' `
  ((@($g.failed).Count -eq 0) -and $g.retried -and (@($g.informational) -match 'retried|重试').Count -gt 0) ("failed=[{0}] info=[{1}]" -f (@($g.failed) -join ','), (@($g.informational) -join ';'))
$g = Test-M3PinAttemptsComparable -Dirs @($paOk, $paRetry)
Test-Case 'pinacct/两侧一侧重试=绿(informational)' (@($g.failed).Count -eq 0) ("failed=[{0}] info=[{1}]" -f (@($g.failed) -join ','), (@($g.informational) -join ';'))
# 旧会话（baseline.json 根本没有 pin_attempts 键）= 绿（informational）——收掉旧登记 Minor「无字段旧会话被拒比」。
$paNoField = Join-Path $Work 'pin-nofield'
New-Baseline $paNoField '(off)'
$g = Get-M3PinAttemptsGuard -PlayerDir $paNoField
Test-Case 'pinacct/旧会话无字段键=绿(informational，收旧 Minor)' `
  ((@($g.failed).Count -eq 0) -and (@($g.informational) -match '旧产物').Count -gt 0) ("failed=[{0}] info=[{1}]" -f (@($g.failed) -join ','), (@($g.informational) -join ';'))
# 键在但值空 / `(off)` = 红（保留的牙：仪器写了字段却没落有效 attempts）。
$paEmpty = Join-Path $Work 'pin-empty'
New-Baseline $paEmpty '(off)' -EmptyPinAttempts
$g = Get-M3PinAttemptsGuard -PlayerDir $paEmpty
Test-Case 'pinacct/键在但值空=红(missing=保留的牙)' (@($g.failed) -contains 'pin_attempts_missing') ("failed=[{0}]" -f (@($g.failed) -join ','))
$paOff = Join-Path $Work 'pin-off'
New-Baseline $paOff '(off)' -PinAttempts '(off)'
$g = Get-M3PinAttemptsGuard -PlayerDir $paOff
Test-Case 'pinacct/键在但值 (off)=红(missing)' (@($g.failed) -contains 'pin_attempts_missing') ("failed=[{0}]" -f (@($g.failed) -join ','))
$paInvalid = Join-Path $Work 'pin-invalid'
New-Baseline $paInvalid '(off)' -PinAttempts 'attempts=x'
$g = Get-M3PinAttemptsGuard -PlayerDir $paInvalid
Test-Case 'pinacct/形态认不出=红(invalid)' (@($g.failed) -contains 'pin_attempts_invalid') ("failed=[{0}]" -f (@($g.failed) -join ','))
$paUnder = Join-Path $Work 'pin-under'
New-Baseline $paUnder '(off)' -PinAttempts (Format-M3PinAttempts -Attempts 1 -SettleRounds 1 -MaxAttempts 5)
$g = Get-M3PinAttemptsGuard -PlayerDir $paUnder
Test-Case 'pinacct/attempts=1=绿(informational)' ((@($g.failed).Count -eq 0) -and (@($g.informational).Count -gt 0)) ("failed=[{0}]" -f (@($g.failed) -join ','))
$paDrift = Join-Path $Work 'pin-drift'
New-Baseline $paDrift '(off)' -PinAttempts (Format-M3PinAttempts -Attempts 2 -Expected 3 -SettleRounds 2 -MaxAttempts 5)
$g = Get-M3PinAttemptsGuard -PlayerDir $paDrift
Test-Case 'pinacct/期望漂移=绿(informational)' ((@($g.failed).Count -eq 0) -and (@($g.informational).Count -gt 0)) ("failed=[{0}]" -f (@($g.failed) -join ','))
$paBad = Join-Path $Work 'pin-badjson'
New-Item -ItemType Directory -Force -Path $paBad | Out-Null
Set-Content -Path (Join-Path $paBad 'baseline.json') -Encoding utf8 -Value '{ "pin_attempts": "attempts=2'
$g = Get-M3PinAttemptsGuard -PlayerDir $paBad
Test-Case 'pinacct/baseline 坏 JSON=红(不中断)' (@($g.failed) -contains 'pin_attempts_baseline_invalid') ("failed=[{0}]" -f (@($g.failed) -join ','))
# 改坏必红：把 attempts=2 会话的 pin_attempts 值清空（=仪器写了字段却没落有效记账）⇒ 必红；还原 ⇒ 绿。
$paMutFile = Join-Path $paOk 'baseline.json'
$paMutOrig = Get-Content $paMutFile -Raw
$paPreGreen = (@((Get-M3PinAttemptsGuard -PlayerDir $paOk).failed).Count -eq 0)
Set-Content -Path $paMutFile -Encoding utf8 -Value '{ "session": "pin-ok", "side": "csharp", "pin_attempts": "" }'
$paMutRed = (@((Get-M3PinAttemptsGuard -PlayerDir $paOk).failed) -contains 'pin_attempts_missing')
Set-Content -Path $paMutFile -Encoding utf8 -Value $paMutOrig
$paRestoredGreen = (@((Get-M3PinAttemptsGuard -PlayerDir $paOk).failed).Count -eq 0)
Test-Case 'pinacct/改坏必红：键值清空=红，还原=绿' ($paPreGreen -and $paMutRed -and $paRestoredGreen) ("pre={0} mutRed={1} restored={2}" -f $paPreGreen, $paMutRed, $paRestoredGreen)

# ---------------------------------------------------------------- ⑩ 预热相不进世界（owner 2026-10-12 19:20 根因修；reviewer-6 M-1 正向断言）
Write-Output '⑩ Test-M3PrewarmNoWorldEntry（预热相 bot 不得进世界——防 GameSvr 异步存档覆盖 pin）'
$pwNoWorld = Join-Path $Work 'prewarm-noworld.log'
Set-Content -Path $pwNoWorld -Encoding utf8 -Value "15:01:16 [INF] 创建角色 m3x0`n15:01:16 [INF] 预热建角完成（StopAfterCharCreated；不进世界）"
$pwWorld = Join-Path $Work 'prewarm-world.log'
Set-Content -Path $pwWorld -Encoding utf8 -Value "15:01:16 [INF] 创建角色 m3x0`n15:01:16 [INF] 选择角色 m3x0`n15:01:16 [INF] 准备进入游戏`n15:01:16 [INF] 机器人:[m3x0] 进入游戏`n15:01:18 [INF] 机器人:[m3x0] 成功进入游戏"
# reviewer-6 M-1：日志存在但零 marker（既无进世界标记、也无建角正证）⇒ 必须红（fail-closed）。
$pwEmpty = Join-Path $Work 'prewarm-empty.log'
Set-Content -Path $pwEmpty -Encoding utf8 -Value "15:01:16 [INF] 登录成功`n15:01:16 [INF] 正在查询角色"
$pwNoWorldR = Test-M3PrewarmNoWorldEntry -LogPath $pwNoWorld
$pwWorldR = Test-M3PrewarmNoWorldEntry -LogPath $pwWorld
$pwMissingR = Test-M3PrewarmNoWorldEntry -LogPath (Join-Path $Work 'prewarm-none.log')
$pwEmptyR = Test-M3PrewarmNoWorldEntry -LogPath $pwEmpty
Test-Case 'prewarm/无进世界标记+有建角正证=绿' ($pwNoWorldR.ok -and $pwNoWorldR.markers.Count -eq 0 -and $pwNoWorldR.positive.Count -gt 0) ("ok={0} positive=[{1}]" -f $pwNoWorldR.ok, (@($pwNoWorldR.positive) -join ','))
Test-Case 'prewarm/有进世界标记=红(fail-closed)' ((-not $pwWorldR.ok) -and $pwWorldR.reason -eq 'prewarm_entered_world') ("reason={0} markers=[{1}]" -f $pwWorldR.reason, (@($pwWorldR.markers) -join ','))
Test-Case 'prewarm/日志缺失=红(fail-closed)' ((-not $pwMissingR.ok) -and $pwMissingR.reason -eq 'log_missing') ("reason={0}" -f $pwMissingR.reason)
Test-Case 'prewarm/日志存在但零 marker=红(fail-closed；M-1)' ((-not $pwEmptyR.ok) -and $pwEmptyR.reason -eq 'prewarm_marker_missing') ("reason={0} markers=[{1}] positive=[{2}]" -f $pwEmptyR.reason, (@($pwEmptyR.markers) -join ','), (@($pwEmptyR.positive) -join ','))

# ---------------------------------------------------------------- 汇总
$report = [ordered]@{
  tool = 'mir2-rs/tests/parity/m3/selftest_gate.ps1'
  ts = (Get-Date).ToString('o')
  guards = (Resolve-Path (Join-Path $Here 'gate_guards.ps1')).Path
  total = $script:Total
  failed = $script:Failed
  checks = $script:Checks
  pass = ($script:Failed -eq 0)
}
$reportPath = Join-Path $OutDir 'selftest.json'
$report | ConvertTo-Json -Depth 6 | Set-Content -Path $reportPath -Encoding utf8
Write-Output ('SELFTEST_REPORT ' + $reportPath)
Write-Output ('SUMMARY total={0} failed={1}' -f $script:Total, $script:Failed)
if ($script:Failed -eq 0) {
  Write-Output 'GREEN: 十条机判护栏的判红/判绿口径都经构造输入验证'
  exit 0
}
Write-Output 'SELFTEST FAIL: 有护栏口径与实现不符（见上面 FAIL 行）'
exit 1
