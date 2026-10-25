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
    9. 成因三档（卡 openmir2-mirflow-verdict-cause-split）：同一假产物只改成因 =>
       missing_prereq(6)/spawn_failed(7)/out_of_range(8)；单因子矩阵 + Resolve-CombatCause 真值表；
       A7 参数名写错 => rc=2 USAGE（不落任何 verdict）。
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

  # ---- 9. 夹具加固（卡 fixture-harden）：notice_poll 只点 1 次 + F4 共享控制流 ----
  # 事故：旧实现 for ($i=0; $i -lt 12) 在 (522,525) 盲点，公告只吃第一次，其余 11 次走位。
  # sim 与真机共用 Invoke-NoticePollCore（F4），这里断言输出形状 + 源码引用同一函数。
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
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "nosignal", "-OutDir", $fixDir)
  Check "fixture: no client-side signal => 0 clicks + no_client_signal" (
    $r.Code -eq 0 -and $r.Out.Contains("NOTICE_POLL_SIM mode=nosignal clicks=0 world_clicks=0") -and
    $r.Out.Contains("reason=no_client_signal")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "dismiss1", "-NoticePollClicks", "2", "-OutDir", $fixDir)
  Check "fixture: NoticePollClicks>1 rejected before client (fail-closed)" (
    $r.Code -ne 0 -and $r.Out.Contains("NoticePollClicks")) ($r.Out + " rc=" + $r.Code)
  # F5：0 与 1018 校验矛盾 ⇒ 也必须拒绝。
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "dismiss1", "-NoticePollClicks", "0", "-OutDir", $fixDir)
  Check "fixture: NoticePollClicks=0 rejected (F5 contradiction)" (
    $r.Code -ne 0 -and $r.Out.Contains("NoticePollClicks")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "bogus", "-OutDir", $fixDir)
  Check "fixture: unknown notice sim mode fails closed" ($r.Code -ne 0) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-NoticePollSim", "dismiss1", "-OutDir", $fixDir)
  Check "fixture: -NoticePollSim without -FixtureDryRun rejected (would launch client)" (
    $r.Code -ne 0 -and $r.Out.Contains("FixtureDryRun")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-NoticeEvidenceCheck", "-OutDir", $fixDir)
  Check "fixture: -NoticeEvidenceCheck without -FixtureDryRun rejected" (
    $r.Code -ne 0 -and $r.Out.Contains("FixtureDryRun")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "dismiss1", "-NoticeButton", "abc", "-OutDir", $fixDir)
  Check "fixture: malformed NoticeButton rejected before client" (
    $r.Code -ne 0 -and $r.Out.Contains("NoticeButton")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticePollSim", "dismiss1", "-NoticePixelProbe", "1,2,ZZZZZZ,3", "-OutDir", $fixDir)
  Check "fixture: malformed NoticePixelProbe rejected before client" (
    $r.Code -ne 0 -and $r.Out.Contains("NoticePixelProbe")) ($r.Out + " rc=" + $r.Code)

  function New-Word([int]$x, [int]$y) { return [long](($x -band 0xFFFF) -bor (($y -band 0xFFFF) -shl 16)) }
  function New-CapLine([string]$dir, [int]$ident, [long]$ts, [long]$recog = 0, [int]$param = 0, [int]$tag = 0, [int]$series = 0) {
    $o = [ordered]@{ kind = "frame"; dir = $dir; ts_ms = $ts; ident = $ident }
    if ($ident -eq 3011) { $o.recog = $recog; $o.tag = $tag }
    elseif ($ident -eq 50 -or $ident -eq 51) { $o.param = $param; $o.tag = $tag; $o.series = $series; $o.recog = 268 }
    return ($o | ConvertTo-Json -Compress)
  }
  function Write-CaptureFile([string]$path, [object[]]$lines) {
    [IO.File]::WriteAllText($path, (($lines -join "`r`n") + "`r`n"), (New-Object Text.UTF8Encoding($false)))
    return $path
  }

  # ---- 10. 坐标门（F6）：CM_WALK 是目标格，只接受与上一条可信证据的相邻步 ----
  $farCap = Join-Path $fixDir "capture-far.jsonl"
  Write-CaptureFile $farCap @(
    (New-CapLine "s2c" 50 1000 0 29 33 0),
    (New-CapLine "c2s" 3011 5000 (New-Word 29 44) 0 4 0)
  ) | Out-Null
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,33", "-CapturePath", $farCap, "-OutDir", $fixDir)
  Check "fixture F6: far CM_WALK target ignored, fallback to SM_LOGON (29,33 GREEN)" (
    $r.Code -eq 0 -and $r.Out.Contains("POSITION_GATE GREEN actual=29,33 expect=29,33") -and
    $r.Out.Contains("ignored_far_walks=1")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,44", "-CapturePath", $farCap, "-OutDir", $fixDir)
  Check "fixture F6: multi-cell target cannot pass gate (expect 29,44 RED)" (
    $r.Code -ne 0 -and $r.Out.Contains("POSITION_GATE RED actual=29,33 expect=29,44")) ($r.Out + " rc=" + $r.Code)

  $chainLines = @((New-CapLine "s2c" 51 1000 0 29 33 1))
  for ($y = 34; $y -le 44; $y++) {
    $chainLines += (New-CapLine "c2s" 3011 (1000 + ($y - 33) * 100) (New-Word 29 $y) 0 4 0)
  }
  $chainCap = Join-Path $fixDir "capture-chain.jsonl"
  Write-CaptureFile $chainCap $chainLines | Out-Null
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,44", "-CapturePath", $chainCap, "-OutDir", $fixDir)
  Check "fixture F6: chained adjacent walks advance to real cell (29,44 GREEN)" (
    $r.Code -eq 0 -and $r.Out.Contains("POSITION_GATE GREEN actual=29,44 expect=29,44") -and
    $r.Out.Contains("trusted_walks=11")) ($r.Out + " rc=" + $r.Code)

  $noAnchorCap = Join-Path $fixDir "capture-noanchor.jsonl"
  Write-CaptureFile $noAnchorCap @((New-CapLine "c2s" 3011 1000 (New-Word 29 34) 0 4 0)) | Out-Null
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,34", "-CapturePath", $noAnchorCap, "-OutDir", $fixDir)
  Check "fixture F6: CM_WALK without SM_LOGON/NEWMAP anchor is not evidence (fail-closed)" (
    $r.Code -ne 0 -and $r.Out.Contains("POSITION_GATE RED")) ($r.Out + " rc=" + $r.Code)

  # ---- 11. 证据新鲜度（F2）：旧 capture / 旧 proxy 一律 RED ----
  $oldCap = Join-Path $fixDir "capture-old.jsonl"
  Write-CaptureFile $oldCap @(
    (New-CapLine "s2c" 50 1000 0 29 33 0),
    (New-CapLine "c2s" 3011 1500 (New-Word 29 34) 0 4 0)
  ) | Out-Null
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,34", "-CapturePath", $oldCap, "-EvidenceSinceMs", "2000", "-OutDir", $fixDir)
  Check "fixture F2: capture evidence older than SinceMs is RED" (
    $r.Code -ne 0 -and $r.Out.Contains("POSITION_GATE RED")) ($r.Out + " rc=" + $r.Code)
  $newCap = Join-Path $fixDir "capture-new.jsonl"
  Write-CaptureFile $newCap @(
    (New-CapLine "s2c" 50 3000 0 29 33 0),
    (New-CapLine "c2s" 3011 3500 (New-Word 29 34) 0 4 0)
  ) | Out-Null
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,34", "-CapturePath", $newCap, "-EvidenceSinceMs", "2000", "-OutDir", $fixDir)
  Check "fixture F2: capture evidence newer than SinceMs is GREEN" (
    $r.Code -eq 0 -and $r.Out.Contains("POSITION_GATE GREEN actual=29,34")) ($r.Out + " rc=" + $r.Code)
  # live 前置门（-FixturePreflight 不启客户端）：旧 capture 在启动客户端前就被拒。
  $r = Invoke-Flow @("-FixtureDryRun", "-FixturePreflight", "-ExpectPos", "29,34", "-CapturePath", $oldCap, "-EvidenceSinceMs", "2000", "-OutDir", $fixDir)
  Check "fixture F2: live preflight rejects stale capture before client launch" (
    $r.Code -ne 0 -and $r.Out.Contains("STALE_CAPTURE")) ($r.Out + " rc=" + $r.Code)

  # ---- 12. live proxy 证据源（F1）：chunks.ndjson + conn-*.bin + EDCode 头解码 ----
  function ConvertTo-EDCode([byte[]]$src) {
    $no = 2; $rem = 0
    $out = New-Object System.Collections.Generic.List[byte]
    foreach ($b in $src) {
      $c = ([int]$b -bxor 0xAC) -band 0xFF
      if ($no -eq 6) {
        $out.Add([byte](($c -band 0x3F) + 0x3C))
        $rem = $rem -bor (($c -shr 2) -band 0x30)
        $out.Add([byte]($rem + 0x3C))
        $rem = 0
      } else {
        $temp = ($c -shr 2) -band 0xFF
        $out.Add([byte](((($temp -band 0x3C) -bor ($c -band 0x03)) + 0x3C) -band 0xFF))
        $rem = (($rem -shl 2) -bor ($temp -band 0x03)) -band 0xFF
      }
      $no = ($no % 6) + 2
    }
    if ($no -ne 2) { $out.Add([byte](($rem + 0x3C) -band 0xFF)) }
    return ,$out.ToArray()
  }
  function Add-ProxyFrame($list, [string]$dir, [int]$ident, [int]$recog, [int]$param, [int]$tag, [int]$series) {
    $h = New-Object byte[] 12
    [Array]::Copy([BitConverter]::GetBytes([int]$recog), 0, $h, 0, 4)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$ident), 0, $h, 4, 2)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$param), 0, $h, 6, 2)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$tag), 0, $h, 8, 2)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$series), 0, $h, 10, 2)
    $enc = ConvertTo-EDCode $h
    $list.Add([byte]0x23)
    if ($dir -eq "c2s") { $list.Add([byte]0x31) }
    foreach ($x in $enc) { $list.Add([byte]$x) }
    $list.Add([byte]0x21)
    if ($dir -eq "s2c") { $list.Add([byte]0x24) }
  }
  function Write-FixtureProxy([string]$dir, [long]$startMs, [long]$s2cTs, [long]$c2sTs, [switch]$NoReady) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $s2c = New-Object System.Collections.Generic.List[byte]
    Add-ProxyFrame $s2c "s2c" 51 268 29 33 1
    Add-ProxyFrame $s2c "s2c" 658 0 0 0 0
    $c2s = New-Object System.Collections.Generic.List[byte]
    Add-ProxyFrame $c2s "c2s" 1018 0 0 0 0
    Add-ProxyFrame $c2s "c2s" 3011 ([int](New-Word 29 34)) 0 4 0
    [IO.File]::WriteAllBytes((Join-Path $dir "conn-0001.s2c.bin"), $s2c.ToArray())
    [IO.File]::WriteAllBytes((Join-Path $dir "conn-0001.c2s.bin"), $c2s.ToArray())
    $rows = @(
      [ordered]@{ event = "open"; conn = 1; port = 7200; peer = "127.0.0.1:5555"; upstream = "127.0.0.1:7600"; t_wall_ms = $startMs; t_rel_us = 0 },
      [ordered]@{ event = "data"; conn = 1; port = 7200; dir = "s2c"; off = 0; n = $s2c.Count; sha256 = "fixture"; t_wall_ms = $s2cTs; t_rel_us = 10 },
      [ordered]@{ event = "data"; conn = 1; port = 7200; dir = "c2s"; off = 0; n = $c2s.Count; sha256 = "fixture"; t_wall_ms = $c2sTs; t_rel_us = 20 }
    )
    $nd = ($rows | ForEach-Object { $_ | ConvertTo-Json -Compress }) -join "`n"
    [IO.File]::WriteAllText((Join-Path $dir "chunks.ndjson"), $nd + "`n", (New-Object Text.UTF8Encoding($false)))
    $session = [ordered]@{
      tool = "mir2_proxy"; start_wall_ms = $startMs; start_iso = "2026-10-22T00:00:00"
      bind = "127.0.0.1"; map = [ordered]@{ "7200" = "127.0.0.1:7600" }; pid = 0
    }
    [IO.File]::WriteAllText((Join-Path $dir "session.json"), ($session | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding($false)))
    if (-not $NoReady) { [IO.File]::WriteAllText((Join-Path $dir "ready.flag"), "0") }
    return $dir
  }

  $proxyDir = Join-Path $fixDir "proxy"
  Write-FixtureProxy $proxyDir 900 900 1400 | Out-Null
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticeEvidenceCheck", "-ProxyDir", $proxyDir, "-OutDir", $fixDir)
  Check "fixture F1: live proxy decodes s2c SM_SENDNOTICE(658)" (
    $r.Code -eq 0 -and $r.Out.Contains("NOTICE_EVIDENCE GREEN ident=658 ts_ms=900")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,34", "-ProxyDir", $proxyDir, "-OutDir", $fixDir)
  Check "fixture F1: live proxy decodes s2c 51 + c2s CM_WALK (29,34 GREEN)" (
    $r.Code -eq 0 -and $r.Out.Contains("POSITION_GATE GREEN actual=29,34 expect=29,34") -and
    $r.Out.Contains("source='c2s CM_WALK(3011)'")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,34", "-ProxyDir", $proxyDir, "-EvidenceSinceMs", "1300", "-OutDir", $fixDir)
  Check "fixture F2: live proxy evidence older than SinceMs is RED" (
    $r.Code -ne 0 -and $r.Out.Contains("POSITION_GATE RED")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-NoticeEvidenceCheck", "-ProxyDir", $proxyDir, "-EvidenceSinceMs", "2000", "-OutDir", $fixDir)
  Check "fixture F2: live proxy 658 older than SinceMs is RED" (
    $r.Code -ne 0 -and $r.Out.Contains("NOTICE_EVIDENCE RED")) ($r.Out + " rc=" + $r.Code)
  $noReadyDir = Join-Path $fixDir "proxy-noready"
  Write-FixtureProxy $noReadyDir 900 900 1000 -NoReady | Out-Null
  $r = Invoke-Flow @("-FixtureDryRun", "-FixturePreflight", "-ExpectPos", "29,34", "-ProxyDir", $noReadyDir, "-OutDir", $fixDir)
  Check "fixture F1: live preflight rejects proxy without ready.flag" (
    $r.Code -ne 0 -and $r.Out.Contains("ready.flag")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,34", "-ProxyDir", $proxyDir, "-CapturePath", $newCap, "-OutDir", $fixDir)
  Check "fixture F1: -ProxyDir and -CapturePath are mutually exclusive" (
    $r.Code -ne 0 -and $r.Out.Contains("互斥")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-FixtureDryRun", "-ExpectPos", "29,33", "-OutDir", $fixDir)
  Check "fixture: position gate requires evidence source (fail-closed)" (
    $r.Code -ne 0 -and $r.Out.Contains("ProxyDir")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-ClickNpc", "-OutDir", $fixDir)
  Check "fixture: ClickNpc without evidence source rejected (no blind click)" (
    $r.Code -ne 0 -and $r.Out.Contains("ClickNpc")) ($r.Out + " rc=" + $r.Code)

  # ---- 12b. 运行中增长（F1）：同一进程内两次 Update-ProxyFrameCache，只消费新增 bytes ----
  $growDir = Join-Path $fixDir "proxy-grow"
  New-Item -ItemType Directory -Force -Path $growDir | Out-Null
  $gS2c = New-Object System.Collections.Generic.List[byte]
  Add-ProxyFrame $gS2c "s2c" 51 268 29 33 1
  Add-ProxyFrame $gS2c "s2c" 658 0 0 0 0
  [IO.File]::WriteAllBytes((Join-Path $growDir "conn-0001.s2c.bin"), $gS2c.ToArray())
  $gRows = @(
    [ordered]@{ event = "open"; conn = 1; port = 7200; t_wall_ms = 900 },
    [ordered]@{ event = "data"; conn = 1; port = 7200; dir = "s2c"; off = 0; n = $gS2c.Count; t_wall_ms = 900 }
  )
  $gNd = ($gRows | ForEach-Object { $_ | ConvertTo-Json -Compress }) -join "`n"
  [IO.File]::WriteAllText((Join-Path $growDir "chunks.ndjson"), $gNd + "`n", (New-Object Text.UTF8Encoding($false)))
  [IO.File]::WriteAllText((Join-Path $growDir "session.json"), '{"start_wall_ms":900,"pid":0}', (New-Object Text.UTF8Encoding($false)))
  # 把 mir_flow 的函数抽出来在当前进程调用（不执行主流程；离线、零客户端）
  $tAst = $null; $eAst = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile($flow, [ref]$tAst, [ref]$eAst)
  $funcs = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true)
  Invoke-Expression (($funcs | ForEach-Object { $_.Extent.Text }) -join "`n")
  $script:ProxyCache = @{}; $script:CaptureCache = @{}; $script:EvidenceSlackMs = 2000
  $gSrc = New-EvidenceSource $growDir "" 0 0 -Offline
  Update-ProxyFrameCache $growDir
  $g1 = Get-EvidenceFrames $gSrc
  $g1Count = [int]$g1.Count   # 列表是活引用（运行中会增长）：先记下第一次的读数
  $gC2s = New-Object System.Collections.Generic.List[byte]
  Add-ProxyFrame $gC2s "c2s" 1018 0 0 0 0
  Add-ProxyFrame $gC2s "c2s" 3011 ([int](New-Word 29 34)) 0 4 0
  [IO.File]::WriteAllBytes((Join-Path $growDir "conn-0001.c2s.bin"), $gC2s.ToArray())
  $gRow = [ordered]@{ event = "data"; conn = 1; port = 7200; dir = "c2s"; off = 0; n = $gC2s.Count; t_wall_ms = 1400 } | ConvertTo-Json -Compress
  [IO.File]::AppendAllText((Join-Path $growDir "chunks.ndjson"), $gRow + "`n", (New-Object Text.UTF8Encoding($false)))
  Update-ProxyFrameCache $growDir
  $g2 = Get-EvidenceFrames $gSrc
  $gEv = Get-SelfCellEvidence $gSrc 0
  Check "fixture F1: growing proxy consumed incrementally (2 -> 4 frames, cell 29,34)" (
    $g1Count -eq 2 -and $g2.Count -eq 4 -and [int]$gEv.X -eq 29 -and [int]$gEv.Y -eq 34) (
    "g1=" + $g1Count + " g2=" + $g2.Count + " cell=" + $gEv.X + "," + $gEv.Y)

  # ---- 12c. 截断 Low 的牙：1018 必须晚于点击时刻（旧 1018 不能假通过） ----
  # 旧 Wait-CaptureFrame 取首条匹配且不看点击时刻；新 Wait-EvidenceFrame 取最新帧 + afterMs。
  $old1018 = Join-Path $fixDir "capture-1018-old.jsonl"
  Write-CaptureFile $old1018 @((New-CapLine "c2s" 1018 1000)) | Out-Null
  $old1018Src = New-EvidenceSource "" $old1018 0 0 -Offline
  $rOld = Wait-EvidenceFrame $old1018Src "c2s" 1018 0 $old1018Src.SinceMs 2000
  Check "fixture Low(truncated): 1018 older than click time is not accepted" (
    $null -eq $rOld) ("rOld=" + $rOld)
  $new1018 = Join-Path $fixDir "capture-1018-new.jsonl"
  Write-CaptureFile $new1018 @((New-CapLine "c2s" 1018 3000)) | Out-Null
  $new1018Src = New-EvidenceSource "" $new1018 0 0 -Offline
  $rNew = Wait-EvidenceFrame $new1018Src "c2s" 1018 0 $new1018Src.SinceMs 2000
  Check "fixture Low(truncated): 1018 after click time is accepted" (
    $null -ne $rNew -and [long]$rNew.ts_ms -eq 3000) ("rNew=" + ($rNew | ConvertTo-Json -Compress))

  # ---- 14. 四类玩家动作阶段（卡 openmir2-mirflow-combat-pickup-stages）----
  # 全部在 **$env:TEMP 假根** 里造 hook 产物（chunks.ndjson + conn-*.bin），不碰生产/真实 capture。
  # 牙（A3）：删掉该类的前置动作/副作用 ⇒ 对应判据 RED 且点名类别；四类各自独立。
  function Add-ProxyFrameBody($list, [string]$dir, [int]$ident, [long]$recog, [int]$param, [int]$tag, [int]$series, [int]$bodyLen = 0) {
    $h = New-Object byte[] 12
    [Array]::Copy([BitConverter]::GetBytes([int]$recog), 0, $h, 0, 4)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$ident), 0, $h, 4, 2)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$param), 0, $h, 6, 2)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$tag), 0, $h, 8, 2)
    [Array]::Copy([BitConverter]::GetBytes([uint16]$series), 0, $h, 10, 2)
    $enc = ConvertTo-EDCode $h
    $list.Add([byte]0x23)
    if ($dir -eq "c2s") { $list.Add([byte]0x31) }
    foreach ($x in $enc) { $list.Add([byte]$x) }
    for ($bi = 0; $bi -lt $bodyLen; $bi++) { $list.Add([byte]0x41) }   # 'A' = 非空 body
    $list.Add([byte]0x21)
    if ($dir -eq "s2c") { $list.Add([byte]0x24) }
  }
  function New-StageProxyDir([string]$dir, [object[]]$frames) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $bins = @{}
    $rows = New-Object System.Collections.ArrayList
    foreach ($f in $frames) {
      $key = [string]$f.dir
      if (-not $bins.ContainsKey($key)) { $bins[$key] = New-Object System.Collections.Generic.List[byte] }
      $lst = $bins[$key]
      $off = [long]$lst.Count
      $bodyLen = 0
      if ($f.ContainsKey("body")) { $bodyLen = [int]$f.body }
      Add-ProxyFrameBody $lst ([string]$f.dir) ([int]$f.ident) ([long]$f.recog) ([int]$f.param) ([int]$f.tag) ([int]$f.series) $bodyLen
      $n = [long]$lst.Count - $off
      [void]$rows.Add([ordered]@{ event = "data"; conn = 1; port = 7200; hop = "game"; dir = [string]$f.dir; off = $off; n = $n; sha256 = "fixture"; t_wall_ms = [long]$f.ts })
    }
    foreach ($k in $bins.Keys) {
      [IO.File]::WriteAllBytes((Join-Path $dir ("conn-0001." + $k + ".bin")), $bins[$k].ToArray())
    }
    $sb = New-Object Text.StringBuilder
    foreach ($r in $rows) { [void]$sb.Append(($r | ConvertTo-Json -Compress)); [void]$sb.Append("`n") }
    [IO.File]::WriteAllText((Join-Path $dir "chunks.ndjson"), $sb.ToString(), (New-Object Text.UTF8Encoding($false)))
  }
  function New-StageOutDir([string]$name) { return (New-TestDir ("stage-" + $name)) }
  function Get-EvidenceDoc([string]$outDir) {
    $p = Join-Path $outDir "flow-stages-evidence.json"
    if (-not (Test-Path -LiteralPath $p)) { return $null }
    return ([IO.File]::ReadAllText($p, [Text.Encoding]::UTF8) | ConvertFrom-Json)
  }
  $ownW = [long](New-Word 330 331)

  # ---- 14a. 捡物 ----
  $pkGreen = New-TestDir "pickup-green"
  New-StageProxyDir $pkGreen @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "s2c"; ident = 201; recog = 6496; param = 0; tag = 0; series = 5; ts = 1100 },
    @{ dir = "s2c"; ident = 653; recog = 1000; param = 0; tag = 0; series = 0; ts = 1200 },
    @{ dir = "c2s"; ident = 1001; recog = 0; param = 330; tag = 331; series = 0; ts = 2000 },
    @{ dir = "s2c"; ident = 200; recog = 777; param = 0; tag = 0; series = 1; body = 40; ts = 2200 })
  $pkOut = New-StageOutDir "pickup-green"
  $r = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $pkGreen, "-OutDir", $pkOut)
  Check "stages pickup: GREEN with SM_ADDITEM after CM_PICKUP" (
    $r.Code -eq 0 -and $r.Out.Contains("STAGE_EVIDENCE category=pickup verdict=GREEN")) ($r.Out + " rc=" + $r.Code)
  $doc = Get-EvidenceDoc $pkOut
  Check "stages pickup: before/after bag rows 5 -> 6" (
    $null -ne $doc -and [int]$doc.categories.pickup.before.bag_rows -eq 5 -and
    [int]$doc.categories.pickup.after.bag_rows -eq 6) (($doc.categories.pickup | ConvertTo-Json -Compress -Depth 6))

  # 牙 1：删掉副作用（有 CM_PICKUP，但服务端没有入包/金币变化）
  $pkRed = New-TestDir "pickup-noeffect"
  New-StageProxyDir $pkRed @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "s2c"; ident = 201; recog = 6496; param = 0; tag = 0; series = 5; ts = 1100 },
    @{ dir = "c2s"; ident = 1001; recog = 0; param = 330; tag = 331; series = 0; ts = 2000 })
  $r = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $pkRed, "-OutDir", (New-StageOutDir "pickup-red"))
  # 卡 openmir2-mirflow-verdict-cause-split：地面物按 96ad82a37 裁决结构性不可达 ⇒「没生效」且前置缺失
  # 必须是 UNDECIDABLE(missing_prereq, floor_item)，不是 RED（那是把环境伪差异记成 FAIL）。
  Check "stages pickup: no effect + structural floor_item => UNDECIDABLE (kind=missing_prereq rc=6)" (
    $r.Code -eq 6 -and
    $r.Out.Contains("category=pickup verdict=UNDECIDABLE kind=missing_prereq rc=6 missing=floor_item") -and
    $r.Out.Contains("无入包") -and -not $r.Out.Contains("verdict=GREEN")) ($r.Out + " rc=" + $r.Code)

  # 牙 2：CM_PICKUP 目标格 != 角色格（服务端 oracle 直接忽略）
  $pkCell = New-TestDir "pickup-cell"
  New-StageProxyDir $pkCell @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "s2c"; ident = 201; recog = 6496; param = 0; tag = 0; series = 5; ts = 1100 },
    @{ dir = "c2s"; ident = 1001; recog = 0; param = 331; tag = 331; series = 0; ts = 2000 },
    @{ dir = "s2c"; ident = 200; recog = 777; param = 0; tag = 0; series = 1; body = 40; ts = 2200 })
  $r = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $pkCell, "-OutDir", (New-StageOutDir "pickup-cell-red"))
  Check "stages pickup TOOTH: pickup at wrong cell => RED" (
    $r.Code -eq 1 -and $r.Out.Contains("category=pickup verdict=RED") -and $r.Out.Contains("!= 角色格")) ($r.Out + " rc=" + $r.Code)

  # ---- 14b. NPC 对话 ----
  $ndGreen = New-TestDir "npcdialog-green"
  New-StageProxyDir $ndGreen @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "c2s"; ident = 1010; recog = 5001; param = 0; tag = 0; series = 0; ts = 2000 },
    @{ dir = "s2c"; ident = 643; recog = 5001; param = 0; tag = 0; series = 1; body = 60; ts = 2100 })
  $ndOut = New-StageOutDir "npcdialog-green"
  $r = Invoke-Flow @("-StageEvidenceCheck", "-NpcDialogStage", "-ProxyDir", $ndGreen, "-OutDir", $ndOut)
  Check "stages npcdialog: GREEN with script say after CM_CLICKNPC" (
    $r.Code -eq 0 -and $r.Out.Contains("STAGE_EVIDENCE category=npcdialog verdict=GREEN")) ($r.Out + " rc=" + $r.Code)
  $doc = Get-EvidenceDoc $ndOut
  Check "stages npcdialog: say_frames=1 max_body_len=60" (
    $null -ne $doc -and [int]$doc.categories.npcdialog.dialogue.say_frames -eq 1 -and
    [int]$doc.categories.npcdialog.dialogue.max_body_len -eq 60) (($doc.categories.npcdialog | ConvertTo-Json -Compress -Depth 6))

  # 牙：删掉前置动作（有对话帧，但客户端没发 CM_CLICKNPC）
  $ndRed = New-TestDir "npcdialog-noclick"
  New-StageProxyDir $ndRed @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "s2c"; ident = 643; recog = 5001; param = 0; tag = 0; series = 1; body = 60; ts = 2100 })
  $r = Invoke-Flow @("-StageEvidenceCheck", "-NpcDialogStage", "-ProxyDir", $ndRed, "-OutDir", (New-StageOutDir "npcdialog-red"))
  Check "stages npcdialog TOOTH: no CM_CLICKNPC => RED named npcdialog" (
    $r.Code -eq 1 -and $r.Out.Contains("category=npcdialog verdict=RED") -and
    $r.Out.Contains("未发出 c2s CM_CLICKNPC")) ($r.Out + " rc=" + $r.Code)

  # ---- 14c. 买卖 ----
  $trGreen = New-TestDir "trade-green"
  New-StageProxyDir $trGreen @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "s2c"; ident = 653; recog = 1000; param = 0; tag = 0; series = 0; ts = 1100 },
    @{ dir = "c2s"; ident = 1010; recog = 5001; param = 0; tag = 0; series = 0; ts = 2000 },
    @{ dir = "s2c"; ident = 643; recog = 5001; param = 0; tag = 0; series = 1; body = 60; ts = 2100 },
    @{ dir = "c2s"; ident = 1014; recog = 5001; param = 207; tag = 1; series = 0; ts = 3000 },
    @{ dir = "s2c"; ident = 650; recog = 0; param = 0; tag = 0; series = 0; body = 10; ts = 3200 },
    @{ dir = "s2c"; ident = 653; recog = 900; param = 0; tag = 0; series = 0; ts = 3300 })
  $trOut = New-StageOutDir "trade-green"
  $r = Invoke-Flow @("-StageEvidenceCheck", "-TradeStage", "-ProxyDir", $trGreen, "-OutDir", $trOut)
  Check "stages trade: GREEN with dialogue + success + gold delta" (
    $r.Code -eq 0 -and $r.Out.Contains("STAGE_EVIDENCE category=trade verdict=GREEN")) ($r.Out + " rc=" + $r.Code)
  $doc = Get-EvidenceDoc $trOut
  Check "stages trade: gold_delta=-100, opens>=1" (
    $null -ne $doc -and [long]$doc.categories.trade.gold_delta -eq -100 -and
    [int]$doc.categories.trade.dialogue.opens -ge 1) (($doc.categories.trade | ConvertTo-Json -Compress -Depth 6))

  # 牙：删掉前置动作（不点 NPC 直接交易）
  $trRed = New-TestDir "trade-nodialog"
  New-StageProxyDir $trRed @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "s2c"; ident = 653; recog = 1000; param = 0; tag = 0; series = 0; ts = 1100 },
    @{ dir = "c2s"; ident = 1014; recog = 5001; param = 207; tag = 1; series = 0; ts = 3000 },
    @{ dir = "s2c"; ident = 650; recog = 0; param = 0; tag = 0; series = 0; body = 10; ts = 3200 },
    @{ dir = "s2c"; ident = 653; recog = 900; param = 0; tag = 0; series = 0; ts = 3300 })
  $r = Invoke-Flow @("-StageEvidenceCheck", "-TradeStage", "-ProxyDir", $trRed, "-OutDir", (New-StageOutDir "trade-red"))
  Check "stages trade TOOTH: no merchant dialogue => RED named trade" (
    $r.Code -eq 1 -and $r.Out.Contains("category=trade verdict=RED") -and
    $r.Out.Contains("未先打开商人对话")) ($r.Out + " rc=" + $r.Code)

  # 牙：对话 + 成功帧都在，但金币没变化
  $trGold = New-TestDir "trade-nogold"
  New-StageProxyDir $trGold @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "s2c"; ident = 653; recog = 1000; param = 0; tag = 0; series = 0; ts = 1100 },
    @{ dir = "c2s"; ident = 1010; recog = 5001; param = 0; tag = 0; series = 0; ts = 2000 },
    @{ dir = "s2c"; ident = 643; recog = 5001; param = 0; tag = 0; series = 1; body = 60; ts = 2100 },
    @{ dir = "c2s"; ident = 1014; recog = 5001; param = 207; tag = 1; series = 0; ts = 3000 },
    @{ dir = "s2c"; ident = 650; recog = 0; param = 0; tag = 0; series = 0; body = 10; ts = 3200 })
  $r = Invoke-Flow @("-StageEvidenceCheck", "-TradeStage", "-ProxyDir", $trGold, "-OutDir", (New-StageOutDir "trade-gold-red"))
  Check "stages trade TOOTH: no gold change => RED" (
    $r.Code -eq 1 -and $r.Out.Contains("category=trade verdict=RED") -and
    $r.Out.Contains("金币未减少")) ($r.Out + " rc=" + $r.Code)

  # ---- 14d. 有目标战斗 ----
  $profGood = Join-Path (New-TestDir "profile-good") "profile.json"
  [IO.File]::WriteAllText($profGood, '{"fixtures":{"monsters":[{"map":"0","x":331,"y":331,"name":"鸡","count":4}]}}', (New-Object Text.UTF8Encoding($false)))
  $profOther = Join-Path (New-TestDir "profile-other") "profile.json"
  [IO.File]::WriteAllText($profOther, '{"fixtures":{"monsters":[{"map":"1","x":331,"y":331,"name":"鸡","count":4}]}}', (New-Object Text.UTF8Encoding($false)))
  $cbGreen = New-TestDir "combat-green"
  New-StageProxyDir $cbGreen @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "c2s"; ident = 3014; recog = $ownW; param = 0; tag = 2; series = 0; ts = 2000 },
    @{ dir = "s2c"; ident = 31; recog = 8888; param = 8; tag = 10; series = 2; ts = 2200 })
  $cbOut = New-StageOutDir "combat-green"
  $r = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbGreen, "-OutDir", $cbOut,
    "-Profile", $profGood, "-PlayerMap", "0")
  Check "stages combat: GREEN with fixture target + front cell + SM_STRUCK" (
    $r.Code -eq 0 -and $r.Out.Contains("STAGE_EVIDENCE category=combat verdict=GREEN")) ($r.Out + " rc=" + $r.Code)
  $doc = Get-EvidenceDoc $cbOut
  Check "stages combat: target 331,331 / count 4 / hp 10 -> 8" (
    $null -ne $doc -and [int]$doc.categories.combat.target.x -eq 331 -and
    [int]$doc.categories.combat.target.count -eq 4 -and
    [int]$doc.categories.combat.hp_before -eq 10 -and [int]$doc.categories.combat.hp_after -eq 8) (
    ($doc.categories.combat | ConvertTo-Json -Compress -Depth 6))

  # A1/F1：combat 证据必须记录这次用的是**显式像素**还是**48x32 格心投影**。
  # 没有它，真机 combat RED 时分不清「靶子没刷出」与「未证实的投影点歪」。
  Check "stages combat A1: default path records pixel_source=projected (560,304)" (
    $null -ne $doc -and [string]$doc.categories.combat.pixel_source -eq "projected" -and
    [int]$doc.categories.combat.pixel.x -eq 560 -and [int]$doc.categories.combat.pixel.y -eq 304) (
    ($doc.categories.combat | ConvertTo-Json -Compress -Depth 6))
  Check "stages combat A1: stdout prints pixel_source=projected" (
    $r.Out.Contains("COMBAT_PIXEL pixel_source=projected x=560 y=304")) $r.Out
  $cbPixOut = New-StageOutDir "combat-pixel-explicit"
  $r = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbGreen, "-OutDir", $cbPixOut,
    "-Profile", $profGood, "-PlayerMap", "0", "-CombatTargetPixel", "600,320")
  $docPix = Get-EvidenceDoc $cbPixOut
  Check "stages combat A1: -CombatTargetPixel => pixel_source=explicit (600,320)" (
    $r.Code -eq 0 -and $null -ne $docPix -and
    [string]$docPix.categories.combat.pixel_source -eq "explicit" -and
    [int]$docPix.categories.combat.pixel.x -eq 600 -and [int]$docPix.categories.combat.pixel.y -eq 320 -and
    $r.Out.Contains("COMBAT_PIXEL pixel_source=explicit x=600 y=320")) (
    $r.Out + " rc=" + $r.Code + " " + ($docPix.categories.combat | ConvertTo-Json -Compress -Depth 6))
  Check "stages combat A1 TOOTH: explicit vs projected pixels actually differ" (
    [int]$doc.categories.combat.pixel.x -ne [int]$docPix.categories.combat.pixel.x) (
    "projected=" + $doc.categories.combat.pixel.x + " explicit=" + $docPix.categories.combat.pixel.x)

  # 牙：不刷怪（profile 的靶子在别的图）⇒ RED，点名 combat
  $r = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbGreen, "-OutDir", (New-StageOutDir "combat-notarget"),
    "-Profile", $profOther, "-PlayerMap", "0")
  # 声明在、但只在别的图 ⇒ 刷出也不在可测位置：RED(out_of_range, rc=8)，不是 missing_prereq。
  Check "stages combat TOOTH: declared only on other map => RED out_of_range (rc=8)" (
    $r.Code -eq 8 -and
    $r.Out.Contains("category=combat verdict=RED kind=out_of_range rc=8") -and
    $r.Out.Contains("不同图") -and -not $r.Out.Contains("kind=spawn_failed")) ($r.Out + " rc=" + $r.Code)

  # 牙：攻击方向没指向靶子
  $cbDir = New-TestDir "combat-wrongdir"
  New-StageProxyDir $cbDir @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "c2s"; ident = 3014; recog = $ownW; param = 0; tag = 6; series = 0; ts = 2000 },
    @{ dir = "s2c"; ident = 31; recog = 8888; param = 8; tag = 10; series = 2; ts = 2200 })
  $r = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbDir, "-OutDir", (New-StageOutDir "combat-dir-red"),
    "-Profile", $profGood, "-PlayerMap", "0")
  Check "stages combat TOOTH: hit not aimed at an in-AP target => RED aim_mismatch (rc=1)" (
    $r.Code -eq 1 -and $r.Out.Contains("category=combat verdict=RED kind=aim_mismatch rc=1") -and
    $r.Out.Contains("未指向靶子") -and -not $r.Out.Contains("kind=spawn_failed") -and
    -not $r.Out.Contains("kind=out_of_range")) ($r.Out + " rc=" + $r.Code)

  # 牙：靶子没刷出（CM_HIT 指向靶子格，但没有任何掉血/死亡）
  $cbNoFx = New-TestDir "combat-noeffect"
  New-StageProxyDir $cbNoFx @(
    @{ dir = "s2c"; ident = 50; recog = 6496; param = 330; tag = 331; series = 0; ts = 1000 },
    @{ dir = "c2s"; ident = 3014; recog = $ownW; param = 0; tag = 2; series = 0; ts = 2000 })
  $r = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbNoFx, "-OutDir", (New-StageOutDir "combat-fx-red"),
    "-Profile", $profGood, "-PlayerMap", "0")
  # 夹具已声明（正前方）却无掉血/死亡 ⇒ 控输入没兑现：RED(spawn_failed, rc=7)。
  Check "stages combat TOOTH: declared but not spawned => RED spawn_failed (rc=7)" (
    $r.Code -eq 7 -and
    $r.Out.Contains("category=combat verdict=RED kind=spawn_failed rc=7") -and
    $r.Out.Contains("无掉血") -and -not $r.Out.Contains("kind=out_of_range")) ($r.Out + " rc=" + $r.Code)

  # ---- 14e. A6：日志目录参数化（空目录 ⇒ 显式 SKIP；-RequireLogEvidence ⇒ RED）----
  $emptyLog = New-TestDir "log-empty"
  $r = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $pkGreen, "-OutDir", (New-StageOutDir "log-skip"),
    "-LogDir", $emptyLog)
  Check "stages A6: empty LogDir => explicit SKIP (not silent)" (
    $r.Code -eq 0 -and $r.Out.Contains("LOG_EVIDENCE status=SKIP") -and
    $r.Out.Contains("空日志目录")) ($r.Out + " rc=" + $r.Code)
  $r = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $pkGreen, "-OutDir", (New-StageOutDir "log-red"),
    "-LogDir", $emptyLog, "-RequireLogEvidence")
  Check "stages A6 TOOTH: empty LogDir + RequireLogEvidence => RED" (
    $r.Code -eq 1 -and $r.Out.Contains("log=RED")) ($r.Out + " rc=" + $r.Code)

  $errLog = New-TestDir "log-err"
  [IO.File]::WriteAllText((Join-Path $errLog "game-svr.out.log"), "[ERR] boom`n", (New-Object Text.UTF8Encoding($false)))
  $r = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $pkGreen, "-OutDir", (New-StageOutDir "log-err-red"),
    "-LogDir", $errLog)
  Check "stages A6 TOOTH: [ERR] in log => LOG_EVIDENCE RED" (
    $r.Code -eq 0 -and $r.Out.Contains("LOG_EVIDENCE status=RED") -and $r.Out.Contains("err=1")) ($r.Out + " rc=" + $r.Code)

  # ---- 14f. A5：既有 25 阶段名称与相对顺序不变（新阶段只能追加）----
  # F3（A2）：`IndexOf` 只给**首次出现**的位置 ⇒ 只有「该 needle 恰出现 1 次」时位置才有意义。
  # 只要有人在注释/文档字符串里先写一句同名 `Stage "pickup"`，位置就会前移、顺序牙静默错判。
  # 所以位置一律经 Get-StageOccurrence（Count≠1 ⇒ Ok=$false），顺序判据只在 Ok 时成立。
  $srcStages = [IO.File]::ReadAllText($flow, [Text.Encoding]::UTF8)
  function Get-StageNeedles {
    $l = @()
    foreach ($n in @("start", "inject_hook", "login_id", "login_pwd", "login_submit", "charsel", "enter_game",
                     "notice_wait", "notice_poll", "ingame", "talk", "talk_sent")) { $l += ('Stage "' + $n + '"') }
    $l += 'Stage ("attack_" + ($i + 1))'
    $l += 'Stage ("walk_" + $i)'
    foreach ($n in @("logout_button", "logout_alt_x", "logout_menu", "logout_confirm", "back_charsel",
                     "reenter", "reentered", "done")) { $l += ('Stage "' + $n + '"') }
    return ,$l
  }
  function Get-StageOccurrence([string]$src, [string]$needle) {
    if ([string]::IsNullOrEmpty($needle)) { return [pscustomobject][ordered]@{ Ok = $false; Pos = -1; Count = 0 } }
    $n = ([regex]::Matches($src, [regex]::Escape($needle))).Count
    if ($n -ne 1) { return [pscustomobject][ordered]@{ Ok = $false; Pos = -1; Count = $n } }
    return [pscustomobject][ordered]@{ Ok = $true; Pos = $src.IndexOf($needle); Count = 1 }
  }
  function Measure-StageOrder([string]$src) {
    $positions = @()
    $bad = @()
    foreach ($nd in (Get-StageNeedles)) {
      $o = Get-StageOccurrence $src $nd
      if (-not $o.Ok) { $bad += ($nd + " x" + $o.Count) }
      $positions += $o.Pos
    }
    if ($bad.Count -gt 0) {
      return [pscustomobject][ordered]@{ Ok = $false; Reason = "non_unique"; Detail = ($bad -join "; "); Positions = $positions }
    }
    foreach ($p in $positions) {
      if ($p -lt 0) { return [pscustomobject][ordered]@{ Ok = $false; Reason = "missing"; Detail = ""; Positions = $positions } }
    }
    for ($i = 1; $i -lt $positions.Count; $i++) {
      if ($positions[$i] -le $positions[$i - 1]) {
        return [pscustomobject][ordered]@{ Ok = $false; Reason = "out_of_order"; Detail = ""; Positions = $positions }
      }
    }
    return [pscustomobject][ordered]@{ Ok = $true; Reason = "ok"; Detail = ""; Positions = $positions }
  }
  $order = Measure-StageOrder $srcStages
  Check "stages A5: canonical 25-stage subsequence present and ordered" $order.Ok (
    $order.Reason + " " + ($order.Positions -join ","))
  $nonUnique = @()
  foreach ($nd in (Get-StageNeedles)) {
    $o = Get-StageOccurrence $srcStages $nd
    if ($o.Count -ne 1) { $nonUnique += ($nd + " x" + $o.Count) }
  }
  Check "stages A5: every stage needle occurs exactly once (IndexOf precondition)" ($nonUnique.Count -eq 0) (
    $nonUnique -join "; ")
  $attackPos = (Get-StageOccurrence $srcStages 'Stage ("attack_" + ($i + 1))').Pos
  $walkPos = (Get-StageOccurrence $srcStages 'Stage ("walk_" + $i)').Pos
  $newPos = @()
  $newBad = @()
  foreach ($nd in @('Stage "pickup"', 'Stage "npc_dialog"', 'Stage "trade"',
                    'Stage "combat_target"', 'Stage "combat"')) {
    $o = Get-StageOccurrence $srcStages $nd
    $newPos += $o.Pos
    if (-not $o.Ok -or $o.Pos -lt 0 -or $o.Pos -lt $attackPos -or $o.Pos -gt $walkPos) {
      $newBad += ($nd + " x" + $o.Count + " pos=" + $o.Pos)
    }
  }
  Check "stages A5: new stages appended after attack, before walk (25-order intact)" ($newBad.Count -eq 0) (
    ($newBad -join "; ") + " | positions=" + ($newPos -join ","))
  # 牙（F3）：构造一个**含重复阶段名**的假源 ⇒ 唯一性判据必须红。
  # 重复行插在真出现**之前**且只是一条注释：朴素 IndexOf 会把它当成首次出现，
  # 但因为紧挨着真行，顺序判据**仍会绿** ⇒ 这颗牙抓的是「唯一性」本身，不是顺序。
  # （新阶段 pickup/npc_dialog/trade/combat_target/combat 的唯一性由上面 newPos 循环各自断言。）
  $dupRx = [regex]([regex]::Escape('Stage "talk"'))
  $dupSrc = $dupRx.Replace($srcStages, ('# Stage "talk"（重复出现的注释）' + "`n" + 'Stage "talk"'), 1)
  $dupOrder = Measure-StageOrder $dupSrc
  Check "stages A5 TOOTH: duplicated stage name => uniqueness RED" (
    -not $dupOrder.Ok -and $dupOrder.Reason -eq "non_unique" -and
    $dupOrder.Detail.Contains('Stage "talk" x2')) ($dupOrder.Reason + " " + $dupOrder.Detail)
  $rawPos = @()
  foreach ($nd in (Get-StageNeedles)) { $rawPos += $dupSrc.IndexOf($nd) }
  $rawOrdered = $true
  foreach ($p in $rawPos) { if ($p -lt 0) { $rawOrdered = $false } }
  for ($i = 1; $i -lt $rawPos.Count; $i++) { if ($rawPos[$i] -le $rawPos[$i - 1]) { $rawOrdered = $false } }
  Check "stages A5 TOOTH control: naive IndexOf would still call the duplicate ordered" $rawOrdered (
    $rawPos -join ",")
  Check "stages A6 source: no hardcoded MirServer logs path" (
    -not ($srcStages -match 'E:[/\\]MirServer[/\\]logs') -and -not ($srcStages -match 'MirServer\\\\logs')) $null

  # ---- 14g. 成因三档（卡 openmir2-mirflow-verdict-cause-split）----
  # A6：**同一份假产物（$cbNoFx 的帧完全相同），只改一个成因（profile）**，三种结果各自成立。
  #   帧：own=(330,331) CM_HIT dir=2(Right) -> front=(331,331)；没有任何 SM_STRUCK/SM_DEATH。
  $profNoDecl = Join-Path (New-TestDir "verdict-nodecl") "profile.json"
  [IO.File]::WriteAllText($profNoDecl, '{"fixtures":{"monsters":[]}}', (New-Object Text.UTF8Encoding($false)))
  $profFar = Join-Path (New-TestDir "verdict-far") "profile.json"
  [IO.File]::WriteAllText($profFar, '{"fixtures":{"monsters":[{"map":"0","x":338,"y":331,"name":"鸡","count":1}]}}', (New-Object Text.UTF8Encoding($false)))
  $v1 = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbNoFx, "-OutDir", (New-StageOutDir "verdict-nodecl"), "-Profile", $profNoDecl, "-PlayerMap", "0")
  $v2 = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbNoFx, "-OutDir", (New-StageOutDir "verdict-spawn"), "-Profile", $profGood, "-PlayerMap", "0")
  $v3 = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbNoFx, "-OutDir", (New-StageOutDir "verdict-far"), "-Profile", $profFar, "-PlayerMap", "0")
  Write-Output ("A6_CAUSE_SPLIT cause=no_declaration      verdict=UNDECIDABLE kind=missing_prereq rc=" + $v1.Code)
  Write-Output ("A6_CAUSE_SPLIT cause=declared_not_spawned verdict=RED kind=spawn_failed rc=" + $v2.Code)
  Write-Output ("A6_CAUSE_SPLIT cause=declared_out_of_ap  verdict=RED kind=out_of_range rc=" + $v3.Code)
  Check "A6 same artifact, 3 causes => rc 6/7/8 all distinct" (
    $v1.Code -eq 6 -and $v2.Code -eq 7 -and $v3.Code -eq 8) ("rc=" + $v1.Code + "/" + $v2.Code + "/" + $v3.Code)
  $kinds3 = @()
  if ($v1.Out.Contains("kind=missing_prereq")) { $kinds3 += "missing_prereq" }
  if ($v2.Out.Contains("kind=spawn_failed")) { $kinds3 += "spawn_failed" }
  if ($v3.Out.Contains("kind=out_of_range")) { $kinds3 += "out_of_range" }
  $rcs3 = @($v1.Code, $v2.Code, $v3.Code)
  Check "A6 three kinds and three rc are pairwise distinct" (
    $kinds3.Count -eq 3 -and (@($kinds3 | Select-Object -Unique)).Count -eq 3 -and
    (@($rcs3 | Select-Object -Unique)).Count -eq 3) ("kinds=" + ($kinds3 -join ",") + " rc=" + ($rcs3 -join ","))

  # A3 三颗独立牙（互不覆盖）：每颗都断言另外两档没出现。
  # 牙 1：删掉夹具声明 => UNDECIDABLE 且点名（不是 RED、不是绿）。
  Check "A3 TOOTH1: delete fixture declaration => UNDECIDABLE named, not RED/GREEN" (
    $v1.Code -eq 6 -and
    $v1.Out.Contains("category=combat verdict=UNDECIDABLE kind=missing_prereq rc=6 missing=monster_fixture") -and
    -not $v1.Out.Contains("verdict=RED") -and -not $v1.Out.Contains("verdict=GREEN")) $v1.Out
  # 牙 2：声明了却刷不出 => RED spawn_failed。
  Check "A3 TOOTH2: declared but not spawned => RED spawn_failed, not out_of_range" (
    $v2.Code -eq 7 -and
    $v2.Out.Contains("category=combat verdict=RED kind=spawn_failed rc=7") -and
    $v2.Out.Contains("无掉血") -and -not $v2.Out.Contains("kind=out_of_range") -and
    -not $v2.Out.Contains("verdict=UNDECIDABLE")) $v2.Out
  # 牙 3：声明了但在 AP 外 => RED out_of_range。
  Check "A3 TOOTH3: declared but outside AP => RED out_of_range, not spawn_failed" (
    $v3.Code -eq 8 -and
    $v3.Out.Contains("category=combat verdict=RED kind=out_of_range rc=8") -and
    $v3.Out.Contains("AP 外") -and -not $v3.Out.Contains("kind=spawn_failed") -and
    -not $v3.Out.Contains("verdict=UNDECIDABLE")) $v3.Out

  # A6 第 4 行：三因全假（声明在 AP 内 + 有副作用）=> PASS。
  $v4 = Invoke-Flow @("-StageEvidenceCheck", "-CombatStage", "-ProxyDir", $cbGreen, "-OutDir", (New-StageOutDir "verdict-pass"), "-Profile", $profGood, "-PlayerMap", "0")
  Check "A6 row4: all three causes false => PASS (rc=0, kind=ok)" (
    $v4.Code -eq 0 -and $v4.Out.Contains("category=combat verdict=GREEN kind=ok rc=0")) $v4.Out

  # A6/step-5 的结构前提：三因是三个**独立输入**，由 Resolve-CombatCause 纯函数汇合。
  # 真值表直接证明「只开一因」时只落对应那一档（审者会按此矩阵做单因子变异）。
  # 8 行真值表：单因子 + 两两/三三组合，验证「显式优先级」而不是「任一为真都落同一档」。
  $truth = @(
    @{ P = $false; O = $false; S = $false; Want = "ok" },
    @{ P = $true;  O = $false; S = $false; Want = "missing_prereq" },
    @{ P = $false; O = $true;  S = $false; Want = "out_of_range" },
    @{ P = $false; O = $false; S = $true;  Want = "spawn_failed" },
    @{ P = $true;  O = $true;  S = $false; Want = "missing_prereq" },
    @{ P = $true;  O = $false; S = $true;  Want = "missing_prereq" },
    @{ P = $false; O = $true;  S = $true;  Want = "out_of_range" },
    @{ P = $true;  O = $true;  S = $true;  Want = "missing_prereq" }
  )
  $truthBad = @()
  foreach ($t in $truth) {
    $got = Resolve-CombatCause $t.P $t.O $t.S
    if ($got -ne $t.Want) {
      $truthBad += ("P" + [int]$t.P + "O" + [int]$t.O + "S" + [int]$t.S + "=" + $got + "!=" + $t.Want)
    }
  }
  Check "A6 resolver 8-row truth table (priority prereq>range>spawn>ok)" ($truthBad.Count -eq 0) ($truthBad -join "; ")
  # 单因子矩阵：每行只允许出现自己的 kind。
  $rowKinds = @(
    @{ Name = "row1-missing"; Out = $v1.Out; Want = "missing_prereq"; Other = @("spawn_failed", "out_of_range") },
    @{ Name = "row2-spawn";   Out = $v2.Out; Want = "spawn_failed";   Other = @("missing_prereq", "out_of_range") },
    @{ Name = "row3-range";   Out = $v3.Out; Want = "out_of_range";   Other = @("missing_prereq", "spawn_failed") },
    @{ Name = "row4-pass";    Out = $v4.Out; Want = "ok";             Other = @("missing_prereq", "spawn_failed", "out_of_range") }
  )
  $matrixBad = @()
  foreach ($rk in $rowKinds) {
    if (-not $rk.Out.Contains("kind=" + $rk.Want)) { $matrixBad += ($rk.Name + ":missing-" + $rk.Want) }
    foreach ($o in $rk.Other) {
      if ($rk.Out.Contains("kind=" + $o)) { $matrixBad += ($rk.Name + ":leaked-" + $o) }
    }
  }
  Check "A6 single-factor matrix: each row carries only its own kind" ($matrixBad.Count -eq 0) ($matrixBad -join "; ")
  Check "source: three combat causes are independently-evaluated booleans + one resolver" (
    $srcStages.Contains('function Resolve-CombatCause') -and
    $srcStages.Contains('$prereqMissing = $false') -and $srcStages.Contains('$outOfRange = $false') -and
    $srcStages.Contains('$spawnFailed = $false')) $null

  # ---- 14h. A7：用法错与任何 verdict 区分（rc=2 + 显式 USAGE）----
  $vTypo = Invoke-Flow @("-StageEvidenceChek", "-ProxyDir", $cbNoFx, "-OutDir", (New-StageOutDir "a7-typo"))
  Check "A7 TOOTH: mistyped -StageEvidenceChek => rc=2 USAGE, no verdict" (
    $vTypo.Code -eq 2 -and $vTypo.Out.Contains("MIRFLOW USAGE") -and
    -not $vTypo.Out.Contains("STAGE_EVIDENCE category=") -and
    -not $vTypo.Out.Contains("kind=") -and
    -not $vTypo.Out.Contains("verdict=")) ($vTypo.Out + " rc=" + $vTypo.Code)
  Check "A7 control: correct entry is not USAGE" (
    $v2.Code -ne 2 -and $v2.Out.Contains("STAGE_EVIDENCE category=combat")) ("rc=" + $v2.Code)
  $vMissArg = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-OutDir", (New-StageOutDir "a7-missing-proxy"))
  Check "A7 TOOTH2: missing required arg => rc=2 USAGE (not RED)" (
    $vMissArg.Code -eq 2 -and $vMissArg.Out.Contains("MIRFLOW USAGE") -and
    -not $vMissArg.Out.Contains("verdict=")) ($vMissArg.Out + " rc=" + $vMissArg.Code)

  # ---- 14i. A2：动作前显式前置门（只读 provision --gate 的 JSON）----
  $gateTrade = Join-Path (New-TestDir "gate-trade") "gate.json"
  [IO.File]::WriteAllText($gateTrade,
    '{"kind":"missing_prereq","verdict":"UNDECIDABLE","need":["gold","sellable_item","npc","floor_item","monsters"],"missing":["gold","npc"],"reasons":{"gold":"characters.Gold=0 < 500","npc":"no npc"}}',
    (New-Object Text.UTF8Encoding($false)))
  # 14c 的「不点 NPC 直接交易」帧（$trRed）+ 门说 gold/npc 缺 ⇒ UNDECIDABLE 并点名 gold。
  $vGateTrade = Invoke-Flow @("-StageEvidenceCheck", "-TradeStage", "-ProxyDir", $trRed, "-OutDir", (New-StageOutDir "gate-trade"),
    "-PrereqGateJson", $gateTrade)
  Check "A2 gate TOOTH: missing gold => trade UNDECIDABLE named (rc=6)" (
    $vGateTrade.Code -eq 6 -and
    $vGateTrade.Out.Contains("category=trade verdict=UNDECIDABLE kind=missing_prereq rc=6 missing=gold")) $vGateTrade.Out
  # 负例：门说全备齐（missing=[]）时，同样的失败帧必须仍是 RED（门不得掩盖真 RED）。
  $gateOk = Join-Path (New-TestDir "gate-ok") "gate.json"
  [IO.File]::WriteAllText($gateOk,
    '{"kind":"prereq_ok","verdict":"PASS","need":["gold","sellable_item","npc","floor_item","monsters"],"missing":[],"reasons":{}}',
    (New-Object Text.UTF8Encoding($false)))
  $vGateOk = Invoke-Flow @("-StageEvidenceCheck", "-TradeStage", "-ProxyDir", $trRed, "-OutDir", (New-StageOutDir "gate-ok"),
    "-PrereqGateJson", $gateOk)
  Check "A2 gate NEGATIVE: gate present => still RED (must not mask a real RED)" (
    $vGateOk.Code -eq 1 -and $vGateOk.Out.Contains("verdict=RED") -and
    -not $vGateOk.Out.Contains("kind=missing_prereq")) $vGateOk.Out
  # NPC 门：没有 npc 时，点不出对话 => UNDECIDABLE 且点名 npc。
  $gateNpc = Join-Path (New-TestDir "gate-npc") "gate.json"
  [IO.File]::WriteAllText($gateNpc,
    '{"kind":"missing_prereq","verdict":"UNDECIDABLE","need":["npc"],"missing":["npc"],"reasons":{"npc":"no scripted npc"}}',
    (New-Object Text.UTF8Encoding($false)))
  $vGateNpc = Invoke-Flow @("-StageEvidenceCheck", "-NpcDialogStage", "-ProxyDir", $ndRed, "-OutDir", (New-StageOutDir "gate-npc"),
    "-PrereqGateJson", $gateNpc)
  Check "A2 gate TOOTH: missing npc => npcdialog UNDECIDABLE named (rc=6)" (
    $vGateNpc.Code -eq 6 -and
    $vGateNpc.Out.Contains("category=npcdialog verdict=UNDECIDABLE kind=missing_prereq rc=6 missing=npc")) $vGateNpc.Out
  # 门文件不存在 => 用法错 rc=2（不能静默当作「门全过」）。
  $vGateMissing = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $pkGreen, "-OutDir", (New-StageOutDir "gate-missing"),
    "-PrereqGateJson", (Join-Path (New-TestDir "gate-none") "nope.json"))
  Check "A2 gate USAGE: missing gate file => rc=2 (not silently pass)" (
    $vGateMissing.Code -eq 2 -and $vGateMissing.Out.Contains("MIRFLOW USAGE")) $vGateMissing.Out

  # ---- 14j. B1/F1/N3（review ab7054f2 返工）----
  # B1：「读不到输入」= USAGE(rc=2)，不是 rc=1 崩溃、也不是任何 verdict（rc=1 只留给人判出的 RED）。
  $b1NoPath = Join-Path (New-TestDir "b1-nopath") "no-such-proxy"
  $b1a = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $b1NoPath, "-OutDir", (New-StageOutDir "b1-nopath"))
  Check "B1 TOOTH1: nonexistent -ProxyDir => rc=2 USAGE, no kind" (
    $b1a.Code -eq 2 -and $b1a.Out.Contains("MIRFLOW USAGE") -and $b1a.Out.Contains("不存在") -and
    -not $b1a.Out.Contains("kind=") -and -not $b1a.Out.Contains("STAGE_EVIDENCE category=")) $b1a.Out
  $b1Empty = New-TestDir "b1-empty-proxy"
  $b1b = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $b1Empty, "-OutDir", (New-StageOutDir "b1-nochunks"))
  Check "B1 TOOTH2: proxy dir without chunks.ndjson => rc=2 USAGE, no kind" (
    $b1b.Code -eq 2 -and $b1b.Out.Contains("MIRFLOW USAGE") -and $b1b.Out.Contains("chunks.ndjson") -and
    -not $b1b.Out.Contains("kind=")) $b1b.Out
  # 同类输入错不许两套并存：capture 不存在 / 两个证据源互斥，也必须是 USAGE。
  $b1c = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-CapturePath", (Join-Path (New-TestDir "b1-nocap") "no.jsonl"), "-OutDir", (New-StageOutDir "b1-nocap"))
  Check "B1 TOOTH3: nonexistent -CapturePath => rc=2 USAGE" (
    $b1c.Code -eq 2 -and $b1c.Out.Contains("MIRFLOW USAGE") -and -not $b1c.Out.Contains("kind=")) $b1c.Out
  $b1d = Invoke-Flow @("-StageEvidenceCheck", "-PickupStage", "-ProxyDir", $b1Empty, "-CapturePath", "E:/x.jsonl", "-OutDir", (New-StageOutDir "b1-mutex"))
  Check "B1 TOOTH4: -ProxyDir + -CapturePath mutex => rc=2 USAGE" (
    $b1d.Code -eq 2 -and $b1d.Out.Contains("MIRFLOW USAGE") -and -not $b1d.Out.Contains("kind=")) $b1d.Out

  # F1/N3：门在动作之前求值并落进证据（path + sha256 + 生成命令 + 缺项名）。
  $gateEvDir = New-TestDir "gate-evidence"
  $gateEvPath = Join-Path $gateEvDir "gate.json"
  [IO.File]::WriteAllText($gateEvPath,
    '{"kind":"missing_prereq","verdict":"UNDECIDABLE","need":["gold","npc"],"missing":["gold"],"reasons":{"gold":"Gold=0"}}',
    (New-Object Text.UTF8Encoding($false)))
  $gateEvCmd = "python -I -B tools/client/provision_real_machine_fixtures.py --gate --db mir2_test_db_r --char aaa --map 3 --profile p.json"
  $gateEvOut = New-StageOutDir "gate-evidence"
  $rGateEv = Invoke-Flow @("-StageEvidenceCheck", "-TradeStage", "-ProxyDir", $trRed, "-OutDir", $gateEvOut,
    "-PrereqGateJson", $gateEvPath, "-PrereqGateCmd", $gateEvCmd)
  $gd = Get-EvidenceDoc $gateEvOut
  $wantGateSha = Get-Sha256Hex $gateEvPath
  Check "N3 TOOTH: evidence records gate path + sha256 + generation command" (
    $null -ne $gd -and $null -ne $gd.prereq_gate -and
    ([string]$gd.prereq_gate.path -eq (Resolve-Path -LiteralPath $gateEvPath).Path) -and
    ([string]$gd.prereq_gate.sha256 -eq $wantGateSha) -and
    ([string]$gd.prereq_gate.generated_by -eq $gateEvCmd) -and
    ([string]$gd.prereq_gate.generated_by_source -eq "operator") -and
    (@($gd.prereq_gate.missing) -contains "gold")) (
    ($gd.prereq_gate | ConvertTo-Json -Compress -Depth 6))
  Check "F1 TOOTH: gate evaluated before actions, recorded with missing names" (
    $null -ne $gd -and $null -ne $gd.prereq_gate -and
    [bool]$gd.prereq_gate.evaluated_before_actions -and
    [long]$gd.prereq_gate.evaluated_at_wall_ms -gt 0 -and
    (@($gd.prereq_gate.missing) -contains "gold")) (
    ($gd.prereq_gate | ConvertTo-Json -Compress -Depth 6))
  # F1 源序：门求值行必须出现在 live 动作块之前（不是动作失败后回头补算）。
  $gateEvalPos = $srcStages.IndexOf('$script:prereqGate = Read-PrereqGateJson')
  $liveActionPos = $srcStages.IndexOf('Stage "pickup"')
  Check "F1 source order: gate evaluated before the live action block" (
    $gateEvalPos -ge 0 -and $liveActionPos -gt $gateEvalPos) ("gate_pos=" + $gateEvalPos + " live_pos=" + $liveActionPos)
  Check "F1 stdout order: PREREQ_GATE precedes STAGE_EVIDENCE" (
    $rGateEv.Out.IndexOf("PREREQ_GATE") -ge 0 -and
    $rGateEv.Out.IndexOf("STAGE_EVIDENCE category=") -gt $rGateEv.Out.IndexOf("PREREQ_GATE")) $rGateEv.Out

  # ---- 13. 源码/参数断言 ----
  $src2 = [IO.File]::ReadAllText($flow, [Text.Encoding]::UTF8)
  Check "source: 12x blind notice loop removed" (-not ($src2 -match 'for \(\$i = 0; \$i -lt 12; \$i\+\+\)')) $null
  Check "source: NoticePollClicks default = 1" ($src2.Contains('$NoticePollClicks = 1')) $null
  Check "source: NoticePollClicks only allows 1" ($src2.Contains('if ($NoticePollClicks -ne 1)')) $null
  Check "source: NPC click default 512,263" ($src2.Contains('$NpcClick = "512,263"')) $null
  Check "source: no code click at 512,227 (comment-only mention allowed)" (
    -not ($src2 -match 'Send-Click\s+512\s+227') -and -not $src2.Contains('$NpcClick = "512,227"')) $null
  # A1/F1 的 live 面只能靠源码锚（离线跑不出真客户端点击）：stdout 必须带 pixel_source，
  # 证据字段必须来自同一个 resolver（点击与证据不允许两处各自算像素）。
  Check "source: verdict rc contract (0/1/2/6/7/8) present" (
    $src2.Contains('$script:RC_UNDECIDABLE = 6') -and $src2.Contains('$script:RC_SPAWN_FAILED = 7') -and
    $src2.Contains('$script:RC_OUT_OF_RANGE = 8') -and $src2.Contains('$script:RC_USAGE = 2')) $null
  Check "source: A7 unknown named params are a USAGE error before any side effect" (
    $src2.Contains('if ($args.Count -gt 0)') -and $src2.Contains('MIRFLOW USAGE')) $null
  Check "source: cause split helpers present" (
    $src2.Contains('function Get-VerdictRc') -and $src2.Contains('function Set-EvVerdict') -and
    $src2.Contains('function Read-PrereqGateJson') -and $src2.Contains('function Get-RcAggregate')) $null
  Check "source: A1 combat prints pixel_source and reuses one resolver" (
    $src2.Contains('$tp = Resolve-CombatTargetPixel $CombatTargetPixel $dx $dy') -and
    $src2.Contains('" pixel_source=" + $tp.Source') -and
    $src2.Contains('WARN COMBAT_PIXEL_SOURCE projected') -and
    $src2.Contains('$ev["pixel_source"] = $tp.Source') -and
    $src2.Contains('COMBAT_PIXEL pixel_source=')) $null
  Check "source: mir_flow manifest records client sha256" (
    $src2.Contains('client_exe_sha256') -and $src2.Contains('Get-ClientProvenance')) $null
  Check "source: live proxy evidence source present (-ProxyDir + EDCode)" (
    $src2.Contains('$ProxyDir = ""') -and $src2.Contains('function ConvertFrom-EDCode') -and
    $src2.Contains('function Update-ProxyFrameCache')) $null
  Check "source: F4 sim shares real control flow (Invoke-NoticePollCore)" (
    $src2.Contains('function Invoke-NoticePollCore') -and
    ([regex]::Matches($src2, 'Invoke-NoticePollCore')).Count -ge 3) $null
  Check "source: F6 gate rejects non-adjacent CM_WALK targets" (
    $src2.Contains('ignored_far_walks') -and $src2.Contains('$dist -le 1')) $null
  Check "source: F3 no client signal => no click" (
    $src2.Contains('no_client_signal') -and $src2.Contains('function Get-NoticeClientSignal')) $null
  Check "source: stage list includes notice_wait/notice_poll" (
    $src2.Contains('Stage "notice_wait"') -and $src2.Contains('Stage "notice_poll"') -and
    $src2.Contains('→ notice_wait')) $null
  $exportSrc = [IO.File]::ReadAllText((Join-Path $here "..\m2-acceptance\export_s2c.ps1"), [Text.Encoding]::UTF8)
  Check "source: export_s2c records client_exe_sha256 in manifest" (
    $exportSrc.Contains('client_exe_sha256') -and $exportSrc.Contains('MANIFEST_CLIENT')) $null
  Check "source: export_s2c records proxy session start" (
    $exportSrc.Contains('proxy_session_start_wall_ms')) $null
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
