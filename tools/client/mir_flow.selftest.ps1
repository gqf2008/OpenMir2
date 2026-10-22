#requires -Version 5
<#
  mir_flow.selftest.ps1 — mir_flow.ps1「AttackTurn 标定/新映射」的**离线**自检。
  不启动客户端、不碰 E:/MirServer、不改冻结面。

  运行：
    powershell -NoProfile -ExecutionPolicy Bypass -File tools/client/mir_flow.selftest.ps1

  覆盖：
    1. 语法：Parser 解析 mir_flow.ps1（0 errors）
    2. 旧口径参数解析/干跑：0,-1 ⇒ 512,272；1,0 ⇒ 560,304；-1,1 ⇒ 464,336
    3. 标定条目语法：dx,dy | dx,dy@x,y | dx,dy@rx,ry>x,y；非法条目 fail-closed
    4. 互斥参数：Calibrate×AttackTurn / Calibrate×AttackClicks / Map 无 AttackTurn
    5. 标定表格式：JSON schema/字段 + CSV 26 列表头/行数（干跑可离线生成）
    6. capture.jsonl 回填：CM_TURN/CM_HIT/CM_WALK 按探针窗口归属，verdict/resolved 正确
    7. 牙（隔离）：新映射被改坏（dx/dy 交换）时，未启用 -AttackTurnMap 的旧路径输出不变；
       新映射缺格偏移时 fail-closed。
    8. 源码静态检查：旧攻击块的字面量 512+dx*48 / 304+dy*32 仍在（未被新映射替换）。
  退出码：0 全 PASS；1 有 FAIL。
#>
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $PSCommandPath
$flow = Join-Path $here "mir_flow.ps1"
$psExe = "powershell"
$script:pass = 0
$script:failNames = @()
$script:testDirs = @()

function Check([string]$name, [bool]$ok, [string]$detail = "") {
  if ($ok) {
    $script:pass++
    Write-Output ("PASS " + $name)
  } else {
    $script:failNames += $name
    Write-Output ("FAIL " + $name + "  " + $detail)
  }
}

function Invoke-Flow([string[]]$flowArgs) {
  # 负例会把异常写到 stderr；EAP=Stop 会把 native stderr 当终止错误，这里局部放宽。
  $oldEap = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $out = & $psExe -NoProfile -ExecutionPolicy Bypass -File $flow @flowArgs 2>&1
    $code = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $oldEap
  }
  return [pscustomobject]@{ Code = $code; Out = (($out | ForEach-Object { [string]$_ }) -join "`n") }
}

function New-TestDir([string]$name) {
  $d = Join-Path $env:TEMP ("mirflow-selftest-" + $name + "-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))
  New-Item -ItemType Directory -Force -Path $d | Out-Null
  $script:testDirs += $d
  return $d
}

$tmpRoot = New-TestDir "root"
try {
  # ---- 1. 语法 ----
  $tokens = $null
  $errors = $null
  [void][System.Management.Automation.Language.Parser]::ParseFile($flow, [ref]$tokens, [ref]$errors)
  Check "syntax: mir_flow.ps1 parses (errors=$($errors.Count))" ($errors.Count -eq 0) ($errors | ForEach-Object { $_.Message } | Select-Object -First 3 | Out-String)

  # ---- 2. 旧口径干跑（未启用任何新参数）----
  $legacyDir = New-TestDir "legacy"
  $cases = @(
    @{ Cell = "0,-1"; X = 512; Y = 272 },
    @{ Cell = "1,0";  X = 560; Y = 304 },
    @{ Cell = "-1,1"; X = 464; Y = 336 }
  )
  foreach ($c in $cases) {
    $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurn", $c.Cell, "-OutDir", $legacyDir)
    $want = "TURN_CLICK x=" + $c.X + " y=" + $c.Y + " source=legacy cell=" + $c.Cell
    Check ("legacy dry-run " + $c.Cell + " => " + $c.X + "," + $c.Y) ($r.Code -eq 0 -and $r.Out.Contains($want)) ($r.Out + " rc=" + $r.Code)
  }

  # ---- 3. 标定条目语法 ----
  $calDir = New-TestDir "calib"
  $spec = "0,-1;1,0;0,1@520,280>530,290"
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurnCalibrate", $spec, "-OutDir", $calDir)
  Check "calibrate spec parse: rc=0 / 3 probes" ($r.Code -eq 0 -and $r.Out.Contains("CALIB_SUMMARY probes=3")) ($r.Out + " rc=" + $r.Code)
  Check "calibrate 0,-1 legacy pixel 512,272" ($r.Out.Contains("cell=0,-1 expected_dir=0(Up) right=512,272 left=512,272")) $r.Out
  Check "calibrate 1,0 legacy pixel 560,304" ($r.Out.Contains("cell=1,0 expected_dir=2(Right) right=560,304 left=560,304")) $r.Out
  Check "calibrate explicit right>left pixels" ($r.Out.Contains("cell=0,1 expected_dir=4(Down) right=520,280 left=530,290")) $r.Out
  Check "calibrate dry-run writes DRY_RUN_OK" ($r.Out.Contains("DRY_RUN_OK")) $r.Out

  foreach ($bad in @("0,0", "2,0", "0,-1,1,0", "x,1")) {
    $rb = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurnCalibrate", $bad, "-OutDir", $calDir)
    Check ("calibrate invalid spec '" + $bad + "' fails closed") ($rb.Code -ne 0) ($rb.Out + " rc=" + $rb.Code)
  }

  # ---- 4. 互斥参数（都在启动客户端之前 fail-closed）----
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurnCalibrate", "0,-1", "-AttackTurn", "0,-1", "-OutDir", $calDir)
  Check "mutex: calibrate + AttackTurn rejected" ($r.Code -ne 0) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurnCalibrate", "0,-1", "-AttackClicks", "3", "-OutDir", $calDir)
  Check "mutex: calibrate + AttackClicks rejected" ($r.Code -ne 0) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurnMap", (Join-Path $calDir "nope.json"), "-OutDir", $calDir)
  Check "mutex: AttackTurnMap without AttackTurn rejected" ($r.Code -ne 0) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-AttackTurnDryRun", "-OutDir", $calDir)
  Check "dry-run without target rejected" ($r.Code -ne 0) ($r.Out + " rc=" + $r.Code)

  # ---- 5. 标定表格式 ----
  $jsonPath = Join-Path $calDir "attackturn-calibration.json"
  $csvPath = Join-Path $calDir "attackturn-calibration.csv"
  Check "table: JSON exists" (Test-Path -LiteralPath $jsonPath) $jsonPath
  Check "table: CSV exists" (Test-Path -LiteralPath $csvPath) $csvPath
  $ledger = [IO.File]::ReadAllText($jsonPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
  Check "table: schema" ([string]$ledger.schema -eq "openmir2.attackturn-calibration/1") ([string]$ledger.schema)
  Check "table: probes count=3" (@($ledger.probes).Count -eq 3) ([string]@($ledger.probes).Count)
  Check "table: projection anchor/step" (
    [int]$ledger.projection.anchor_x -eq 512 -and [int]$ledger.projection.anchor_y -eq 304 -and
    [int]$ledger.projection.step_x -eq 48 -and [int]$ledger.projection.step_y -eq 32) ([string]$ledger.projection)
  $p1 = @($ledger.probes)[0]
  $hasKeys = ($null -ne $p1.PSObject.Properties["right_x"] -and $null -ne $p1.PSObject.Properties["left_y"] -and
              $null -ne $p1.PSObject.Properties["observed"] -and $null -ne $p1.PSObject.Properties["verdict"])
  Check "table: probe fields present" $hasKeys ($p1 | ConvertTo-Json -Depth 4)
  Check "table: dry-run observed unknown" ([string]$p1.verdict -eq "unknown" -and [int]$p1.observed.cm_turn -eq 0) ([string]$p1.verdict)
  $csvLines = [IO.File]::ReadAllLines($csvPath, [Text.Encoding]::UTF8)
  $wantHeader = "probe,cell_dx,cell_dy,expected_dir,expected_dir_name,right_x,right_y,left_x,left_y,t_start_ms,t_end_ms,cm_turn,turn_dirs,turn_dir,turn_x,turn_y,cm_hit,hit_dirs,hit_dir,hit_x,hit_y,cm_walk,cm_run,turn_matches,hit_matches,verdict"
  Check "table: CSV header (26 cols)" ($csvLines.Count -ge 1 -and $csvLines[0] -eq $wantHeader) ($csvLines[0])
  Check "table: CSV rows = 3" ($csvLines.Count -eq 4) ([string]$csvLines.Count)
  Check "table: CSV row1 legacy pixel" ($csvLines[1].StartsWith("1,0,-1,0,Up,512,272,512,272,")) ($csvLines[1])

  # ---- 6. capture.jsonl 回填（CM_TURN/CM_HIT/CM_WALK 归属）----
  $anDir = New-TestDir "analyze"
  $spec2 = "0,-1;1,0;0,1"
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurnCalibrate", $spec2, "-OutDir", $anDir)
  Check "analyze: base ledger generated" ($r.Code -eq 0) ($r.Out + " rc=" + $r.Code)
  $anJson = Join-Path $anDir "attackturn-calibration.json"
  $ledger = [IO.File]::ReadAllText($anJson, [Text.Encoding]::UTF8) | ConvertFrom-Json
  $windows = @(@(1000, 1999), @(2000, 2999), @(3000, 3999))
  for ($i = 0; $i -lt 3; $i++) {
    @($ledger.probes)[$i].t_start_ms = $windows[$i][0]
    @($ledger.probes)[$i].t_end_ms = $windows[$i][1]
  }
  [IO.File]::WriteAllText($anJson, (($ledger | ConvertTo-Json -Depth 12) + "`r`n"), (New-Object Text.UTF8Encoding($false)))

  function New-Word([int]$x, [int]$y) { return [long](($x -band 0xFFFF) -bor (($y -band 0xFFFF) -shl 16)) }
  $frames = @(
    [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 1500; ident = 3010; tag = 0; recog = (New-Word 100 200) },
    [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 1600; ident = 3014; tag = 0; recog = (New-Word 101 199) },
    [ordered]@{ kind = "frame"; dir = "s2c"; ts_ms = 1650; ident = 3014; tag = 0; recog = (New-Word 101 199) },
    [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 2500; ident = 3010; tag = 4; recog = (New-Word 100 200) },
    [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 2600; ident = 3011; tag = 4; recog = (New-Word 100 201) },
    [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 3500; ident = 3010; tag = 4; recog = (New-Word 100 200) },
    [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 3600; ident = 3014; tag = 4; recog = (New-Word 101 200) },
    [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 5000; ident = 3014; tag = 0; recog = (New-Word 1 1) }
  )
  $capLines = @()
  foreach ($f in $frames) { $capLines += ($f | ConvertTo-Json -Compress) }
  $capPath = Join-Path $anDir "capture.jsonl"
  [IO.File]::WriteAllText($capPath, (($capLines -join "`r`n") + "`r`n"), (New-Object Text.UTF8Encoding($false)))

  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurnCalibrate", $spec2, "-CalibrateCapture", $capPath, "-OutDir", $anDir)
  Check "analyze: rc=0" ($r.Code -eq 0) ($r.Out + " rc=" + $r.Code)
  Check "analyze: probe1 turn+hit" ($r.Out.Contains("cell=0,-1 expected_dir=0(Up)") -and $r.Out.Contains("cm_turn=1 turn_dir=Up cm_hit=1 hit_dir=Up cm_walk=0 verdict=turn+hit")) $r.Out
  Check "analyze: probe2 turn_mismatch + walk" ($r.Out.Contains("cell=1,0 expected_dir=2(Right)") -and $r.Out.Contains("cm_turn=1 turn_dir=Down cm_hit=0 hit_dir= cm_walk=1 verdict=turn_mismatch")) $r.Out
  Check "analyze: probe3 turn+hit (Down)" ($r.Out.Contains("cell=0,1 expected_dir=4(Down)") -and $r.Out.Contains("cm_turn=1 turn_dir=Down cm_hit=1 hit_dir=Down cm_walk=0 verdict=turn+hit")) $r.Out
  Check "analyze: summary counts" ($r.Out.Contains("CALIB_SUMMARY probes=3 turn_hit=2 turn_only=0 turn_mismatch=1 walk=0 none=0 unknown=0 resolved=2")) $r.Out
  Check "analyze: resolved has 0,-1" ($r.Out.Contains("CALIB_RESOLVED cell=0,-1") -and $r.Out.Contains("confidence=turn+hit")) $r.Out
  Check "analyze: resolved has 0,1" ($r.Out.Contains("CALIB_RESOLVED cell=0,1") -and $r.Out.Contains("confidence=turn+hit")) $r.Out
  Check "analyze: resolved excludes mismatched 1,0" (-not $r.Out.Contains("CALIB_RESOLVED cell=1,0")) $r.Out

  $ledger2 = [IO.File]::ReadAllText($anJson, [Text.Encoding]::UTF8) | ConvertFrom-Json
  $a1 = @($ledger2.probes)[0]
  Check "analyze: recog lo/hi words (turn 100,200)" ([int]$a1.observed.turn_x -eq 100 -and [int]$a1.observed.turn_y -eq 200) ($a1.observed | ConvertTo-Json -Depth 3)
  Check "analyze: recog lo/hi words (hit 101,199)" ([int]$a1.observed.hit_x -eq 101 -and [int]$a1.observed.hit_y -eq 199) ($a1.observed | ConvertTo-Json -Depth 3)
  Check "analyze: s2c frame ignored" ([int]$a1.observed.cm_hit -eq 1) ([string]$a1.observed.cm_hit)
  Check "analyze: out-of-window frame ignored" ([int]@($ledger2.probes)[1].observed.cm_hit -eq 0) ([string]@($ledger2.probes)[1].observed.cm_hit)
  Check "analyze: mode=dry-run-analyze" ([string]$ledger2.mode -eq "dry-run-analyze") ([string]$ledger2.mode)
  Check "analyze: resolved confidence" ([string]$ledger2.resolved.'0,-1'.confidence -eq "turn+hit") ([string]$ledger2.resolved.'0,-1'.confidence)

  # ---- 7. 牙：新映射改坏（dx/dy 交换）不影响未启用新参数的路径 ----
  $mapPath = Join-Path $anDir "broken-map.json"
  $broken = [ordered]@{
    schema = "openmir2.attackturn-calibration/1"
    resolved = [ordered]@{
      "0,-1" = [ordered]@{ right_x = 560; right_y = 304; left_x = 560; left_y = 304
                           from_probe = 99; confidence = "turn+hit"; expected_dir = 2 }
    }
  }
  [IO.File]::WriteAllText($mapPath, (($broken | ConvertTo-Json -Depth 6) + "`r`n"), (New-Object Text.UTF8Encoding($false)))
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurn", "0,-1", "-OutDir", $anDir)
  Check "teeth: legacy path still 512,272 with broken map on disk" ($r.Code -eq 0 -and $r.Out.Contains("TURN_CLICK x=512 y=272 source=legacy cell=0,-1")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurn", "0,-1", "-AttackTurnMap", $mapPath, "-OutDir", $anDir)
  Check "teeth: broken new map is actually used (560,304)" ($r.Code -eq 0 -and $r.Out.Contains("TURN_CLICK x=560 y=304") -and $r.Out.Contains("source=map cell=0,-1")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurn", "1,0", "-AttackTurnMap", $mapPath, "-OutDir", $anDir)
  Check "teeth: map missing cell fails closed" ($r.Code -ne 0 -and $r.Out.Contains("AttackTurnMap")) ($r.Out + " rc=" + $r.Code)
  $emptyMap = Join-Path $anDir "empty-map.json"
  [IO.File]::WriteAllText($emptyMap, '{"schema":"openmir2.attackturn-calibration/1","resolved":{}}' + "`r`n", (New-Object Text.UTF8Encoding($false)))
  $r = Invoke-Flow @("-AttackTurnDryRun", "-AttackTurn", "0,-1", "-AttackTurnMap", $emptyMap, "-OutDir", $anDir)
  Check "teeth: empty resolved fails closed" ($r.Code -ne 0) ($r.Out + " rc=" + $r.Code)

  # ---- 8. 源码静态检查：旧字面量仍在（新映射只加分支，不替换旧口径）----
  $src = [IO.File]::ReadAllText($flow, [Text.Encoding]::UTF8)
  $hasRight = $src.Contains('Send-RightClick ($TurnCellCx + $turnCell[0] * 48) ($TurnCellCy + $turnCell[1] * 32)')
  $hasClick = $src.Contains('$clickX = $TurnCellCx + $turnCell[0] * 48')
  Check "source: legacy right-click literal intact" $hasRight
  Check "source: legacy left-click literal intact" $hasClick
  Check "source: new mapping gated by turnMapTable" ($src.Contains('if ($turnCell -and $turnMapTable)'))

  # ---- 9. 夹具加固（卡 fixture-harden）：notice_poll 点击次数上限 = 1 ----
  # 事故：旧实现 for ($i=0; $i -lt 12) 在 (522,525) 盲点，公告只吃第一次，其余 11 次走位。
  # 这里既做源码/参数断言，也跑 -FixtureDryRun 的模拟控制流（不启动客户端）。
  $fixDir = New-TestDir "fixture"
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "dismiss1", "-OutDir", $fixDir)
  Check "fixture: notice dismiss1 => clicks=1 world_clicks=0" (
    $r.Code -eq 0 -and $r.Out.Contains("NOTICE_POLL_SIM mode=dismiss1 clicks=1 world_clicks=0") -and
    $r.Out.Contains("cap=1")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "slow", "-OutDir", $fixDir)
  Check "fixture: notice needing 3 clicks still clicks only 1 (cap)" (
    $r.Code -eq 0 -and $r.Out.Contains("NOTICE_POLL_SIM mode=slow clicks=1 world_clicks=0") -and
    $r.Out.Contains("dismissed=False") -and $r.Out.Contains("reason=click_cap_reached")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "absent", "-OutDir", $fixDir)
  Check "fixture: notice absent => 0 clicks into world" (
    $r.Code -eq 0 -and $r.Out.Contains("NOTICE_POLL_SIM mode=absent clicks=0 world_clicks=0") -and
    $r.Out.Contains("reason=no_notice_timeout")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "dismiss1", "-NoticePollClicks", "2", "-OutDir", $fixDir)
  Check "fixture: NoticePollClicks>1 rejected before client (fail-closed)" (
    $r.Code -ne 0 -and $r.Out.Contains("NoticePollClicks")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "bogus", "-OutDir", $fixDir)
  Check "fixture: unknown notice sim mode fails closed" ($r.Code -ne 0) ($r.Out + " rc=" + $r.Code)

  # ---- 10. 夹具加固：坐标门 fail-closed（假 capture 文件；不启动客户端）----
  function New-FixtureCapture([int]$x, [int]$y) {
    $p = Join-Path $fixDir ("capture-" + $x + "-" + $y + ".jsonl")
    $logon = [ordered]@{ kind = "frame"; dir = "s2c"; ts_ms = 1000; ident = 50; recog = 268; param = $x; tag = $y; series = 0 }
    $walk = [ordered]@{ kind = "frame"; dir = "c2s"; ts_ms = 5000; ident = 3011; tag = 4
                        recog = [long](($x -band 0xFFFF) -bor (($y -band 0xFFFF) -shl 16)) }
    $lines = @(($logon | ConvertTo-Json -Compress), ($walk | ConvertTo-Json -Compress))
    [IO.File]::WriteAllText($p, (($lines -join "`r`n") + "`r`n"), (New-Object Text.UTF8Encoding($false)))
    return $p
  }
  $okCap = New-FixtureCapture 29 33
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,33", "-CapturePath", $okCap, "-OutDir", $fixDir)
  Check "fixture: position gate GREEN when actual==expect" (
    $r.Code -eq 0 -and $r.Out.Contains("POSITION_GATE GREEN actual=29,33 expect=29,33")) ($r.Out + " rc=" + $r.Code)
  $badCap = New-FixtureCapture 29 44
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,33", "-CapturePath", $badCap, "-OutDir", $fixDir)
  Check "fixture: position gate RED + abort when actual!=expect" (
    $r.Code -ne 0 -and $r.Out.Contains("POSITION_GATE RED actual=29,44 expect=29,33")) ($r.Out + " rc=" + $r.Code)
  $noCap = Join-Path $fixDir "capture-empty.jsonl"
  [IO.File]::WriteAllText($noCap, "", (New-Object Text.UTF8Encoding($false)))
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,33", "-CapturePath", $noCap, "-OutDir", $fixDir)
  Check "fixture: position gate fail-closed with no evidence" (
    $r.Code -ne 0 -and $r.Out.Contains("POSITION_GATE RED")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,33", "-OutDir", $fixDir)
  Check "fixture: position gate requires -CapturePath (fail-closed)" (
    $r.Code -ne 0 -and $r.Out.Contains("CapturePath")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-ClickNpc", "-OutDir", $fixDir)
  Check "fixture: ClickNpc without -CapturePath rejected (no blind click)" (
    $r.Code -ne 0 -and $r.Out.Contains("ClickNpc")) ($r.Out + " rc=" + $r.Code)

  # ---- 11. 夹具加固：源码断言（盲点循环已删 / NPC 默认坐标 / manifest 客户端字段）----
  $src2 = [IO.File]::ReadAllText($flow, [Text.Encoding]::UTF8)
  Check "source: 12x blind notice loop removed" (-not ($src2 -match 'for \(\$i = 0; \$i -lt 12; \$i\+\+\)')) $null
  Check "source: NoticePollClicks default = 1" ($src2.Contains('$NoticePollClicks = 1')) $null
  Check "source: NPC click default 512,263" ($src2.Contains('$NpcClick = "512,263"')) $null
  Check "source: no code click at 512,227 (comment-only mention allowed)" (
    -not ($src2 -match 'Send-Click\s+512\s+227') -and -not $src2.Contains('$NpcClick = "512,227"')) $null
  Check "source: mir_flow manifest records client sha256" (
    $src2.Contains('client_exe_sha256') -and $src2.Contains('Get-ClientProvenance')) $null
  $exportSrc = [IO.File]::ReadAllText((Join-Path $here "..\m2-acceptance\export_s2c.ps1"), [Text.Encoding]::UTF8)
  Check "source: export_s2c records client_exe_sha256 in manifest" (
    $exportSrc.Contains('client_exe_sha256') -and $exportSrc.Contains('MANIFEST_CLIENT')) $null
}
finally {
  foreach ($d in $script:testDirs) {
    Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue
  }
}

Write-Output ("SELFTEST_PASS=" + $script:pass + " SELFTEST_FAIL=" + $script:failNames.Count)
foreach ($n in $script:failNames) { Write-Output ("FAILED: " + $n) }
if ($script:failNames.Count -gt 0) { exit 1 }
Write-Output "SELFTEST_OK"
exit 0
