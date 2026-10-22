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

  # ---- 13. 源码/参数断言 ----
  $src2 = [IO.File]::ReadAllText($flow, [Text.Encoding]::UTF8)
  Check "source: 12x blind notice loop removed" (-not ($src2 -match 'for \(\$i = 0; \$i -lt 12; \$i\+\+\)')) $null
  Check "source: NoticePollClicks default = 1" ($src2.Contains('$NoticePollClicks = 1')) $null
  Check "source: NoticePollClicks only allows 1" ($src2.Contains('if ($NoticePollClicks -ne 1)')) $null
  Check "source: NPC click default 512,263" ($src2.Contains('$NpcClick = "512,263"')) $null
  Check "source: no code click at 512,227 (comment-only mention allowed)" (
    -not ($src2 -match 'Send-Click\s+512\s+227') -and -not $src2.Contains('$NpcClick = "512,227"')) $null
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
