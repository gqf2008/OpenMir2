#requires -Version 5
<#
.SYNOPSIS
  selftest_verify_chunks.ps1 —— chunks.ndjson 指纹校验的**牙**（A4）。

.DESCRIPTION
  在 **$env:TEMP 下的假根** 里造一份 hook 产物（不碰任何生产/Capture 目录），断言：
    1. 正确指纹 ⇒ verify_chunks.ps1 GREEN（rc=0）；
    2. 篡改某个 bin 的 **1 个字节** ⇒ RED（rc=1），且输出点名该 conn/dir/off；
    3. `sha256` 写成空串（旧 DLL 行为）⇒ RED（rc=1），理由是「空串」；
    4. data 事件 0 条（空扫描）⇒ RED，不得假绿；
    5. 缺 chunks.ndjson ⇒ USAGE rc=2；
    6. **A5 牙①**：错参数名（`-Dir`，历史事故原样）⇒ 必须报 USAGE rc=2，**不得**落成 RED rc=1；
    7. **A5 牙②**：真篡改仍是 RED rc=1 且输出里没有 USAGE 标记（两态可区分）；
    8. **A3/F4 接线牙**：窗口 runbook（`docs/client-flow-stages-20261025.md` §2.1 + 操作单 3b）
       必须把 `verify_chunks.ps1 -CaptureDir` 写成**显式步骤**；删掉那一步 ⇒ 本自检必须红。

  退出码：0 全 PASS；1 有 FAIL。临时目录失败也会清理。
#>
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $PSCommandPath
$verifier = Join-Path $here "verify_chunks.ps1"
$script:pass = 0
$script:fails = @()

function Check([string]$name, [bool]$ok, [string]$detail = "") {
  if ($ok) { $script:pass++; Write-Output ("PASS " + $name) }
  else { $script:fails += $name; Write-Output ("FAIL " + $name + "  " + $detail) }
}

function Invoke-VerifierArgs([string[]]$argv) {
  $old = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $verifier @argv 2>&1
    $code = $LASTEXITCODE
  } finally { $ErrorActionPreference = $old }
  return [pscustomobject]@{ Code = $code; Out = (($out | ForEach-Object { [string]$_ }) -join "`n") }
}
function Invoke-Verifier([string]$dir) {
  return Invoke-VerifierArgs @("-CaptureDir", $dir)
}

function Get-Sha256Hex([byte[]]$bytes, [int]$off, [int]$n) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    $part = New-Object byte[] $n
    [Array]::Copy($bytes, $off, $part, 0, $n)
    return ([BitConverter]::ToString($sha.ComputeHash($part))).Replace('-', '').ToLowerInvariant()
  } finally { $sha.Dispose() }
}

function Test-RunbookStep([string]$path) {
  # 结构判据：`### 2.1 ... 指纹 ...` 小节里必须真有一行 verify_chunks.ps1 -CaptureDir <dir> 命令，
  # 且该小节文本同时出现 rc=2/USAGE 与 rc=1/RED（A5 的文档面）。
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    return [pscustomobject]@{ Ok = $false; RcTable = $false; Detail = ("缺文件：" + $path) }
  }
  $lines = [IO.File]::ReadAllLines($path, [Text.Encoding]::UTF8)
  $h = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^###\s+2\.1\s.*指纹') { $h = $i; break }
  }
  if ($h -lt 0) { return [pscustomobject]@{ Ok = $false; RcTable = $false; Detail = "2.1「指纹」小节标题缺失" } }
  $end = $lines.Count - 1
  for ($i = $h + 1; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^#{1,3}\s') { $end = $i - 1; break }
  }
  $cmd = -1
  for ($i = $h + 1; $i -le $end; $i++) {
    if ($lines[$i] -match 'verify_chunks\.ps1\s+-CaptureDir\s+\S') { $cmd = $i; break }
  }
  if ($cmd -lt 0) {
    return [pscustomobject]@{ Ok = $false; RcTable = $false; Detail = ("2.1 内没有 verify_chunks.ps1 -CaptureDir 命令行（行 " + ($h + 1) + "-" + ($end + 1) + "）") }
  }
  $sect = ($lines[$h..$end] -join "`n")
  $rc = ($sect.Contains("rc=2") -and $sect.Contains("USAGE") -and $sect.Contains("rc=1") -and $sect.Contains("RED"))
  return [pscustomobject]@{ Ok = $true; RcTable = $rc; Detail = ("标题行 " + ($h + 1) + " / 命令行 " + ($cmd + 1)) }
}

function Test-WindowSheetWiring([string]$path) {
  # 操作单的收窗步骤必须指向同一条接线（含 hook 目录的 verify 命令 + 指回 2.1 的四态判读）。
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    return [pscustomobject]@{ Ok = $false; Detail = ("缺文件：" + $path) }
  }
  $text = [IO.File]::ReadAllText($path, [Text.Encoding]::UTF8)
  $hasCmd = [regex]::IsMatch($text, 'verify_chunks\.ps1\s+-CaptureDir\s+\S')
  $hasRef = $text.Contains("client-flow-stages-20261025.md") -and $text.Contains("§2.1")
  $hasRc = $text.Contains("rc=0") -and $text.Contains("rc=2")
  return [pscustomobject]@{ Ok = ($hasCmd -and $hasRef -and $hasRc); Detail = ("cmd=" + $hasCmd + " ref=" + $hasRef + " rc=" + $hasRc) }
}

function New-FakeChunks([string]$dir, [string]$connDir, [byte[]]$payload, [string]$sha) {
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  [IO.File]::WriteAllBytes((Join-Path $dir ("conn-0001." + $connDir + ".bin")), $payload)
  $line = '{"event":"data","conn":1,"port":7200,"hop":"game","dir":"' + $connDir + '","off":0,"n":' +
          $payload.Length + ',"sha256":"' + $sha + '","t_wall_ms":1000}'
  [IO.File]::WriteAllText((Join-Path $dir "chunks.ndjson"), $line + "`n", (New-Object Text.UTF8Encoding($false)))
}

$root = Join-Path $env:TEMP ("mir2-chunks-selftest-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Force -Path $root | Out-Null
try {
  $payload = [Text.Encoding]::ASCII.GetBytes("ABCDEFGH")
  $goodSha = Get-Sha256Hex $payload 0 $payload.Length

  # 1. 正确指纹 ⇒ GREEN
  $d1 = Join-Path $root "good"
  New-FakeChunks $d1 "s2c" $payload $goodSha
  $r = Invoke-Verifier $d1
  Check "good chunk => GREEN rc=0" ($r.Code -eq 0 -and $r.Out.Contains("CHUNKS_VERIFY GREEN events=1")) ($r.Out + " rc=" + $r.Code)

  # 2. 篡改 1 个字节 ⇒ RED，且点名 conn/dir/off
  $d2 = Join-Path $root "tampered"
  New-FakeChunks $d2 "s2c" $payload $goodSha
  $tampered = [byte[]]$payload.Clone()
  $tampered[3] = [byte]($tampered[3] -bxor 0x01)     # 只翻 1 个字节
  [IO.File]::WriteAllBytes((Join-Path $d2 "conn-0001.s2c.bin"), $tampered)
  $r = Invoke-Verifier $d2
  Check "1-byte tamper => RED rc=1 (A5 tooth 2)" ($r.Code -eq 1 -and $r.Out.Contains("CHUNKS_VERIFY RED")) ($r.Out + " rc=" + $r.Code)
  Check "1-byte tamper names conn=1 dir=s2c off=0" ($r.Out.Contains("conn=1 dir=s2c off=0") -and $r.Out.Contains("指纹不符")) $r.Out
  Check "1-byte tamper RED carries no USAGE marker" (-not $r.Out.Contains("CHUNKS_VERIFY USAGE")) $r.Out

  # 3. 空 sha256（旧 DLL 行为）⇒ RED
  $d3 = Join-Path $root "emptysha"
  New-FakeChunks $d3 "c2s" $payload ""
  $r = Invoke-Verifier $d3
  Check "empty sha256 => RED rc=1" ($r.Code -eq 1 -and $r.Out.Contains("sha256 为空串")) ($r.Out + " rc=" + $r.Code)

  # 4. 0 条 data 事件 ⇒ RED（空扫描不得假绿）
  $d4 = Join-Path $root "emptylog"
  New-Item -ItemType Directory -Force -Path $d4 | Out-Null
  [IO.File]::WriteAllText((Join-Path $d4 "chunks.ndjson"), '{"event":"open","conn":1,"port":7200}' + "`n", (New-Object Text.UTF8Encoding($false)))
  $r = Invoke-Verifier $d4
  Check "0 data events => RED rc=1" ($r.Code -eq 1 -and $r.Out.Contains("data 事件 0 条")) ($r.Out + " rc=" + $r.Code)

  # 5. 缺少 chunks.ndjson ⇒ USAGE rc=2
  $d5 = Join-Path $root "missing"
  New-Item -ItemType Directory -Force -Path $d5 | Out-Null
  $r = Invoke-Verifier $d5
  Check "missing chunks.ndjson => USAGE rc=2" ($r.Code -eq 2 -and $r.Out.Contains("CHUNKS_VERIFY USAGE")) ($r.Out + " rc=" + $r.Code)

  # 6. A5 牙①：错参数名（历史事故原样 `-Dir ...`）⇒ USAGE rc=2，绝不许落成 RED rc=1。
  #    旧版是 param() **绑定阶段** rc=1，与「指纹不符」撞车；现在脚本无 param() 块、手动解析 $args。
  $r = Invoke-VerifierArgs @("-Dir", $d1)
  Check "A5 TOOTH 1: wrong param name (-Dir) => USAGE rc=2 (not RED rc=1)" (
    $r.Code -eq 2 -and $r.Out.Contains("CHUNKS_VERIFY USAGE") -and
    -not $r.Out.Contains("CHUNKS_VERIFY RED")) ($r.Out + " rc=" + $r.Code)
  Check "A5 TOOTH 1: usage error names the offending parameter" ($r.Out.Contains("-Dir")) $r.Out

  # 7. A5 牙①（续）：-CaptureDir 缺取值 ⇒ USAGE rc=2（不是「目录不存在」也不是 RED）
  $r = Invoke-VerifierArgs @("-CaptureDir")
  Check "A5 TOOTH 1b: -CaptureDir without value => USAGE rc=2" (
    $r.Code -eq 2 -and $r.Out.Contains("CHUNKS_VERIFY USAGE") -and
    -not $r.Out.Contains("CHUNKS_VERIFY RED")) ($r.Out + " rc=" + $r.Code)

  # 7b. F4：`-Quiet` 必须是**真静默**（此前被解析但全文未使用 = 死参数）。
  #     判据：rc 与 loud 相同、逐事件明细消失、摘要行仍在；GREEN 侧同样只剩摘要。
  $loud = Invoke-VerifierArgs @("-CaptureDir", $d2)
  $quiet = Invoke-VerifierArgs @("-CaptureDir", $d2, "-Quiet")
  $loudLines = @(($loud.Out -split "`n") | Where-Object { $_.Trim() -ne "" })
  $quietLines = @(($quiet.Out -split "`n") | Where-Object { $_.Trim() -ne "" })
  Check "F4 quiet RED: rc 与非 quiet 相同（仍 1）" ($quiet.Code -eq 1 -and $loud.Code -eq 1) ("quiet=" + $quiet.Code + " loud=" + $loud.Code)
  Check "F4 quiet RED: 逐事件明细消失（无 conn=1 dir=s2c off=0）" (-not $quiet.Out.Contains("conn=1 dir=s2c off=0")) $quiet.Out
  Check "F4 quiet RED: 行数少于非 quiet" ($quietLines.Count -lt $loudLines.Count) ("quiet=" + $quietLines.Count + " loud=" + $loudLines.Count)
  Check "F4 quiet RED: 摘要行仍在（CHUNKS_VERIFY RED bad=）" ($quiet.Out.Contains("CHUNKS_VERIFY RED bad=")) $quiet.Out
  $qg = Invoke-VerifierArgs @("-CaptureDir", $d1, "-Quiet")
  Check "F4 quiet GREEN: rc=0 且只打摘要" ($qg.Code -eq 0 -and $qg.Out.Contains("CHUNKS_VERIFY GREEN events=1")) ($qg.Out + " rc=" + $qg.Code)
  $r = Invoke-VerifierArgs @("-Bogus")
  Check "F4 quiet: USAGE 串仍文档化 -Quiet" ($r.Out.Contains("-Quiet") -and $r.Out.Contains("只打摘要")) $r.Out

  # 8. A3/F4：窗口 runbook 必须把「跑完校验指纹」写成**显式步骤**（机器判据，不靠人眼）。
  #    判据落在结构上：指定小节标题 + 该小节内真的有一行 `verify_chunks.ps1 -CaptureDir <dir>` 命令，
  #    并且那一节写清 rc=2(USAGE) 与 rc=1(RED) 的区别（A5 的文档面）。
  $docsRoot = Join-Path $here "..\..\..\docs"
  $rb = Test-RunbookStep (Join-Path $docsRoot "client-flow-stages-20261025.md")
  Check "A3 runbook: client-flow-stages 2.1 has verify_chunks -CaptureDir step" $rb.Ok $rb.Detail
  Check "A3 runbook: step documents rc=2 USAGE vs rc=1 RED" $rb.RcTable $rb.Detail
  $sh = Test-WindowSheetWiring (Join-Path $docsRoot "window-exec-sheet-20261025.md")
  Check "A3 runbook: window exec sheet closing step cross-links it" $sh.Ok $sh.Detail

  # 8b. F1/F2 的**文档面**回归牙（卡 openmir2-window-criteria-doc-followups）：
  #     阈值不得退回恒真的 `success.ident -eq`；A4「同图」必须要求来源断言且缺来源判 UNDECIDABLE。
  #     （可执行语义另见该卡的 doc-snippet 复验；这里只钉文档不退回旧写法。）
  $docOps = Join-Path $docsRoot "window-criteria-operational-20261026.md"
  $dt = [IO.File]::ReadAllText($docOps, [Text.Encoding]::UTF8)
  Check "F2 doc: 阈值不再用恒真的 success.ident" (-not [regex]::IsMatch($dt, 'success\.ident\s+-eq')) "仍出现 success.ident -eq"
  # 钉**具体代码行**，不钉「全文出现过某文件名字符串」（那样把阈值改坏也照样绿）。
  Check "F1 doc: 阈值从断言文件取来源" ($dt.Contains('$pmaPath = "$W/player-map-assertion-A4.json"')) "pmaPath 行缺失/被改"
  Check "F1 doc: 缺来源落 UNDECIDABLE（三态判词）" ($dt.Contains("`$a4Verdict = if (-not `$pmaOk) { 'UNDECIDABLE' }")) "三态判词行缺失/被改"
  Check "F1 doc: 新增 P14 记录「同图是操作者断言」" $dt.Contains("### P14 A4") "P14 缺失"
  Check "F4 doc: -Quiet 不再被写成死参数" (-not $dt.Contains("死参数")) "仍写死参数"
}
finally {
  Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output ("VERIFY_CHUNKS_SELFTEST_PASS=" + $script:pass + " FAIL=" + $script:fails.Count)
foreach ($n in $script:fails) { Write-Output ("FAILED: " + $n) }
if ($script:fails.Count -gt 0) { exit 1 }
Write-Output "VERIFY_CHUNKS_SELFTEST_OK"
exit 0
