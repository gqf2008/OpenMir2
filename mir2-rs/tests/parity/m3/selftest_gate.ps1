#requires -Version 5
<#
.SYNOPSIS
  selftest_gate.ps1 — M3 门禁（run_m3_gate.ps1）三条机判护栏的「判红/判绿」自检。

.DESCRIPTION
  三条护栏的唯一口径在 `gate_guards.ps1`（主流程与自检共用同一批函数，照
  `tools/m2-acceptance/selftest.ps1` + `portclassify.ps1` 的先例）。本脚本用**构造输入**
  把每条口径都跑一遍，断言「失败项代码」而不只看"有没有报错"——只看报错会把"脚本崩了"
  误当"判红"。**全部离线**：不起栈、不占端口、不碰 DB。

    ① Get-M3EntryGuard（-Side rust 必须是已知 Rust 入口 + 挂在本端口期望入口上）
       已知 Rust 三入口 ⇒ 绿；C#（E:\MirServer）⇒ rust_side_entry_not_known_rust；
       未知进程 foo.exe（旧 impl!='csharp' 会放行）⇒ 红；已知 Rust 接错端口 ⇒ 红；
       端口没听（impl=none）⇒ entry_ports_not_listening；同一 C# 端口集在 -Side csharp 下仍绿。
    ② Get-M3SessionDirFault：$null ⇒ 'session_dir_missing'；目录对象 ⇒ 无 fault。
    ③ Get-M3PeerScenarioGuard：场景一致 ⇒ 绿；不一致 ⇒ peer_scenario_mismatch；
       缺 baseline.json ⇒ peer_baseline_missing；缺 fixed_new_chr ⇒ peer_fixed_new_chr_missing。

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

function New-Baseline([string]$Dir, [string]$FixedNewChr, [switch]$OmitField) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  $obj = [ordered]@{ session = (Split-Path $Dir -Leaf); side = 'csharp' }
  if (-not $OmitField) { $obj['fixed_new_chr'] = $FixedNewChr }
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
  Write-Output 'GREEN: 三条机判护栏的判红/判绿口径都经构造输入验证'
  exit 0
}
Write-Output 'SELFTEST FAIL: 有护栏口径与实现不符（见上面 FAIL 行）'
exit 1
