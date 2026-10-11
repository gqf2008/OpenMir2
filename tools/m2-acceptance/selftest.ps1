#requires -Version 5
<#
.SYNOPSIS
  selftest.ps1 — M2 验收手册的「判红/判绿」自检：证明每一步**真的会红**，而不是只有一句口径。

.DESCRIPTION
  `run_acceptance.ps1` 的四步（①端口所有权 / ②真客户端截图 / ③同序列出包 diff / ③三路径 DB diff）
  各自都有「失败怎么判红」的书面口径。本脚本用**构造输入**把每条口径都跑一遍，断言
  「退出码 + 输出关键字」两者都对（**只看退出码会把"脚本崩了"误当"判红"**）：

    ② check_screenshots.ps1   0 张⇒2 / 缺 1 张⇒1 / 有 <1KiB⇒1 / 7 张齐全⇒0
    ③b s2c_diff.py            金标准自比⇒0(GREEN) / 尾字节改坏⇒1(RED) / 只改 recog⇒0 / 缺文件⇒2
    ③c check_db_paths.ps1     三路径字段级相同⇒0(GREEN) / 改一个被比对字段⇒1(RED) / 缺 after⇒2
    ⓪ preflight.ps1           在库前置齐备⇒0 / 缺工具或缺客户端入口⇒2(PREREQ) / 金标准身份或 schema 坏⇒1(RED)
    ①  check_ports.ps1        对**本机实况端口**取判：全 Rust⇒0 / 有非 Rust 在听⇒1 / 有端口没听⇒2
   ①a portclassify.ps1       实现归属分类器（与 check_ports.ps1 共用）：C#根/target路径/基名回落/unknown 5 例
    ⇢  run_acceptance.ps1     全 PREREQ⇒2 / 有 RED⇒1（聚合口径，不许把红说成前置）

  全部是**只读/构造夹具**：不起栈、不起客户端、不占端口（①只 `Get-NetTCPConnection` 取实况）。
  任何一条与期望不符 ⇒ 本脚本判红（exit 1）并打印 `SELFTEST FAIL`。

.PARAMETER OutDir
  证据/夹具目录（默认 `E:\tmp\m2-acceptance\selftest-<时间戳>`）。

.PARAMETER LivePorts
  ①实况端口，默认 5000/7000/7100/7200。

.PARAMETER SkipLivePorts
  跳过①实况（例如端口正被别的车道占用、只想跑构造夹具时）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/selftest.ps1
#>
param(
  [string]$OutDir = "",
  [int[]]$LivePorts = @(5000, 7000, 7100, 7200),
  [switch]$SkipLivePorts
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$Golden = Join-Path $RepoRoot "mir2-rs\tests\golden\c-line-baseline-c3-20261010.jsonl"
# R2-1：①的实现归属分类器与 check_ports.ps1 共用同一文件（不再内联一套更窄的判据）。
. (Join-Path $Here "portclassify.ps1")
if ([string]::IsNullOrEmpty($OutDir)) {
  $OutDir = Join-Path "E:\tmp\m2-acceptance" ("selftest-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
}
$Work = Join-Path $OutDir "work"
New-Item -ItemType Directory -Force -Path $Work | Out-Null

$script:Total = 0
$script:Failed = 0
$script:Checks = New-Object System.Collections.Generic.List[object]

function Test-Case([string]$Name, [int]$Expected, [psobject]$Run, [string]$Pattern, [string]$Detail) {
  $script:Total++
  $rcOk = ($Expected -eq $Run.rc)
  $outOk = [string]::IsNullOrEmpty($Pattern) -or ($Run.out -match $Pattern)
  $ok = ($rcOk -and $outOk)
  if (-not $ok) { $script:Failed++ }
  $tag = if ($ok) { "OK  " } else { "FAIL" }
  $why = if (-not $rcOk) { "rc" } elseif (-not $outOk) { "out" } else { "" }
  Write-Output ("  [{0}] {1,-40} expect_rc={2} actual_rc={3} out~/{4}/ {5}  {6}" -f $tag, $Name, $Expected, $Run.rc, $Pattern, $why, $Detail)
  $script:Checks.Add([pscustomobject]@{
      name = $Name; expect_rc = $Expected; actual_rc = $Run.rc
      expect_out = $Pattern; ok = $ok; detail = $Detail
    })
}

function Run-Ps([string]$ScriptPath, [string[]]$ChildArgs) {
  $prev = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  try {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $ScriptPath @ChildArgs 2>&1 | Out-String
    return [pscustomobject]@{ rc = $LASTEXITCODE; out = $out }
  } finally { $ErrorActionPreference = $prev }
}

function Run-Py([string[]]$PyArgs) {
  $prev = $ErrorActionPreference; $ErrorActionPreference = "Continue"
  try {
    $out = & python @PyArgs 2>&1 | Out-String
    return [pscustomobject]@{ rc = $LASTEXITCODE; out = $out }
  } finally { $ErrorActionPreference = $prev }
}

function New-Shots([string]$Dir, [string[]]$Names, [int]$Bytes, [string]$TinyName) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  $blob = New-Object byte[] ($Bytes + 4)
  [byte[]]$png = 0x89, 0x50, 0x4E, 0x47
  [Array]::Copy($png, 0, $blob, 0, 4)
  foreach ($n in $Names) {
    if ($n -eq $TinyName) {
      [System.IO.File]::WriteAllBytes((Join-Path $Dir $n), (New-Object byte[] 10))
    } else {
      [System.IO.File]::WriteAllBytes((Join-Path $Dir $n), $blob)
    }
  }
}

function Write-Jsonl([string]$Path, [object[]]$Rows) {
  $lines = $Rows | ForEach-Object { $_ | ConvertTo-Json -Compress }
  $text = (($lines -join "`r`n") + "`r`n")
  # 注意：PS 5.1 的 Set-Content -Encoding utf8 会写 BOM，python json 会拒收 ⇒ 用无 BOM 的 .NET 写。
  [System.IO.File]::WriteAllText($Path, $text, (New-Object System.Text.UTF8Encoding($false)))
}

function New-DbFixture([string]$Path, [bool]$Changed) {
  $lines = @("m2acc1`t1`tA", "m2acc2`t2`tB")
  if ($Changed) { $lines[1] = "m2acc2`t2`tCHANGED" }
  $rows = foreach ($l in $lines) {
    $sha = [System.BitConverter]::ToString(
      [System.Security.Cryptography.SHA256]::Create().ComputeHash(
        [System.Text.Encoding]::UTF8.GetBytes($l))).Replace("-", "").ToLower()
    [pscustomobject]@{ table = "mir2_db.characters"; row_sha256 = $sha; line = $l }
  }
  Write-Jsonl $Path $rows
}

$ShotsAll = @(
  "m2-01-enter-map.png", "m2-02-walk.png", "m2-03-talk.png", "m2-04-see-others.png",
  "m2-05-attack.png", "m2-06-pickup.png", "m2-07-relogin.png"
)

Write-Output ("M2 验收手册自检  evidence=" + $OutDir)

# ---------------------------------------------------------------- ② 截图清单
Write-Output "② check_screenshots.ps1"
$sEmpty = Join-Path $Work "shots-empty"; New-Item -ItemType Directory -Force -Path $sEmpty | Out-Null
$s6 = Join-Path $Work "shots-6of7"; New-Shots $s6 $ShotsAll[0..5] 2048 ""
$sTiny = Join-Path $Work "shots-tiny"; New-Shots $sTiny $ShotsAll 2048 "m2-07-relogin.png"
$s7 = Join-Path $Work "shots-7ok"; New-Shots $s7 $ShotsAll 2048 ""
$ck = Join-Path $Here "check_screenshots.ps1"
Test-Case "shots/0张=PREREQ" 2 (Run-Ps $ck @("-ShotsDir", $sEmpty, "-OutFile", (Join-Path $Work "shots-empty.json"))) "PREREQ_MISSING" "空目录"
Test-Case "shots/缺1张=RED" 1 (Run-Ps $ck @("-ShotsDir", $s6, "-OutFile", (Join-Path $Work "shots-6.json"))) "RED: 截图不全" "6/7"
Test-Case "shots/有<1KiB=RED" 1 (Run-Ps $ck @("-ShotsDir", $sTiny, "-OutFile", (Join-Path $Work "shots-tiny.json"))) "RED: 截图不全" "1 张 10B"
Test-Case "shots/7张齐全=GREEN" 0 (Run-Ps $ck @("-ShotsDir", $s7, "-OutFile", (Join-Path $Work "shots-7.json"))) "GREEN: 7 张" "7/7 非空"

# ---------------------------------------------------------------- ③b s2c 对账
Write-Output "③b s2c_diff.py"
$diff = Join-Path $Here "s2c_diff.py"
$mut = Join-Path $Here "mutate_capture.py"
Test-Case "s2c/金标准自比=GREEN" 0 (Run-Py @($diff, "--rust", $Golden, "--out", (Join-Path $Work "s2c-self.json"))) "GREEN: 同序列" "ident 658..10"
$tail = Join-Path $Work "capture-tail.jsonl"
Run-Py @($mut, "--in", $Golden, "--out", $tail, "--ident", "51", "--mutate", "tail") | Out-Null
Test-Case "s2c/尾字节改坏=RED" 1 (Run-Py @($diff, "--rust", $tail, "--out", (Join-Path $Work "s2c-tail.json"))) "RED: .*未登记差异" "非 recog 字节必红"
$recog = Join-Path $Work "capture-recog.jsonl"
Run-Py @($mut, "--in", $Golden, "--out", $recog, "--ident", "51", "--mutate", "recog") | Out-Null
Test-Case "s2c/只改recog=GREEN" 0 (Run-Py @($diff, "--rust", $recog, "--out", (Join-Path $Work "s2c-recog.json"))) "GREEN: 同序列" "归一化清单只 recog"
Test-Case "s2c/缺输入=PREREQ" 2 (Run-Py @($diff, "--rust", (Join-Path $Work "nope.jsonl"))) "PREREQ_MISSING" "文件不存在"

# ---------------------------------------------------------------- ③c 三路径 DB
Write-Output "③c check_db_paths.ps1 -Mode diff"
$csharp = Join-Path $Work "db-csharp"; $rust = Join-Path $Work "db-rust"
New-Item -ItemType Directory -Force -Path $csharp, $rust | Out-Null
foreach ($p in @("softclose", "disconnect", "shutdown")) {
  New-DbFixture (Join-Path $csharp ("{0}-after.jsonl" -f $p)) $false
  New-DbFixture (Join-Path $rust ("{0}-after.jsonl" -f $p)) $false
}
$dbPaths = Join-Path $Here "check_db_paths.ps1"
Test-Case "db/三路径相同=GREEN" 0 (Run-Ps $dbPaths @("-Mode", "diff", "-CsharpDir", $csharp, "-RustDir", $rust, "-OutFile", (Join-Path $Work "db-same.json"))) "GREEN: 小退/掉线" "字段级 diff=0"
New-DbFixture (Join-Path $rust "shutdown-after.jsonl") $true
Test-Case "db/改一字段=RED" 1 (Run-Ps $dbPaths @("-Mode", "diff", "-CsharpDir", $csharp, "-RustDir", $rust, "-OutFile", (Join-Path $Work "db-changed.json"))) "RED: 路径 DB diff 未归零" "shutdown 差异"
Remove-Item -Force (Join-Path $rust "shutdown-after.jsonl")
Test-Case "db/缺抽取件=PREREQ" 2 (Run-Ps $dbPaths @("-Mode", "diff", "-CsharpDir", $csharp, "-RustDir", $rust, "-OutFile", (Join-Path $Work "db-missing.json"))) "PREREQ_MISSING" "shutdown 缺 after"

# ---------------------------------------------------------------- ① 分类器口径（R2-1：与 check_ports.ps1 同一函数）
Write-Output "① portclassify.ps1（实现归属分类器：check_ports.ps1 与 selftest.ps1 共用同一口径）"
function Test-Impl([string]$Desc, [string]$Exe, [string]$Cmd, [string]$Name, [string]$Expect) {
  $script:Total++
  $got = Get-M2PortImpl -Exe $Exe -CommandLine $Cmd -Name $Name
  $ok = ($got -eq $Expect)
  if (-not $ok) { $script:Failed++ }
  $tag = if ($ok) { "OK  " } else { "FAIL" }
  Write-Output ("  [{0}] {1,-40} expect={2} actual={3}" -f $tag, $Desc, $Expect, $got)
  $script:Checks.Add([pscustomobject]@{
      name = $Desc; expect_rc = $Expect; actual_rc = $got
      expect_out = ""; ok = $ok; detail = "impl classifier"
    })
}
Test-Impl "impl/C# 根目录=csharp"        "E:\MirServer\GameSvr.exe"                    "" "GameSvr.exe" "csharp"
Test-Impl "impl/mir2-rs target=rust"     "E:\repo\mir2-rs\target\debug\game-svr.exe"  "" "game-svr.exe" "rust"
Test-Impl "impl/隔离 target(m2svr)=rust" "E:\tmp\openmir2-m2svr-target\debug\game-svr.exe" "" "game-svr.exe" "rust"
Test-Impl "impl/基名回落 rust"           "C:\dev\game-svr.exe"                        "" "game-svr.exe" "rust"
Test-Impl "impl/未知基名=unknown"        "C:\dev\foo.exe"                             "" "foo.exe" "unknown"
# 真机回归（本片新增）：Rust 入口正常用现网 `E:\MirServer\...\config.conf` 作参数，
# 整条命令行里含 `E:\MirServer\`。旧分类器把整个命令行当探针 ⇒ 误判 csharp（验收①假红）。
Test-Impl "impl/Rust 入口带现网配置参数仍=rust" "E:\tmp\openmir2-m2svr-target\debug\login-gate.exe" "E:\MirServer\LoginGate\config.conf" "login-gate.exe" "rust"
Test-Impl "impl/C# 入口（Exe 在 MirServer）=csharp" "E:\MirServer\LoginGate.exe" "E:\MirServer\LoginGate\config.conf" "LoginGate.exe" "csharp"

# ---------------------------------------------------------------- ① 端口表 -> 期望入口（第三层牙齿）
Write-Output "① 端口表 -> 期望入口（端口归属：不只看是否 Rust，还要看是不是该端口的入口）"
function Test-Entry([string]$Desc, [string]$Got, [string]$Expect) {
  $script:Total++
  $ok = ($Got -eq $Expect)
  if (-not $ok) { $script:Failed++ }
  $tag = if ($ok) { "OK  " } else { "FAIL" }
  Write-Output ("  [{0}] {1,-40} expect={2} actual={3}" -f $tag, $Desc, $Expect, $Got)
  $script:Checks.Add([pscustomobject]@{
      name = $Desc; expect_rc = $Expect; actual_rc = $Got
      expect_out = ""; ok = $ok; detail = "port entry table"
    })
}
Test-Entry "entry/表 5000=game-svr"     (Get-M2ExpectedEntry -Port 5000) "game-svr"
Test-Entry "entry/表 7000=login-gate"   (Get-M2ExpectedEntry -Port 7000) "login-gate"
Test-Entry "entry/表 7100=sel-gate"     (Get-M2ExpectedEntry -Port 7100) "sel-gate"
Test-Entry "entry/表 7200=game-gate"    (Get-M2ExpectedEntry -Port 7200) "game-gate"
Test-Entry "entry/基名取 game-svr.exe"  (Get-M2EntryName -Exe "E:\x\target\debug\game-svr.exe" -Name "game-svr.exe") "game-svr"
Test-Entry "entry/5000+game-svr=ok"     (Get-M2EntryVerdict -Port 5000 -EntryName "game-svr") "ok"
Test-Entry "entry/5000+login-gate=cross" (Get-M2EntryVerdict -Port 5000 -EntryName "login-gate") "cross"
Test-Entry "entry/7200+game-svr=cross"  (Get-M2EntryVerdict -Port 7200 -EntryName "game-svr") "cross"

# ---------------------------------------------------------------- ① 端口实况（只读）
if (-not $SkipLivePorts) {
  Write-Output ("① check_ports.ps1（只读实况端口 " + ($LivePorts -join "/") + "）")
  $portsScript = Join-Path $Here "check_ports.ps1"
  $portsOut = Join-Path $Work "ports-live.json"
  # 先跑一次并落证据，再从**这一次**的证据反推期望退出码。
  # 不在跑之前另起一次 Get-NetTCPConnection：端口会变（别的车道正在起/停栈），两次探测会假 FAIL。
  $portsRun = Run-Ps $portsScript @("-OutFile", $portsOut, "-PortsCsv", ($LivePorts -join ","))
  $expectPorts = 2; $expectPat = "PREREQ_MISSING"; $note = "缺证据文件（脚本未落盘）"
  if (Test-Path $portsOut) {
    $ev = Get-Content -Raw -Path $portsOut | ConvertFrom-Json
    $notListening = @($ev.not_listening); $nonRust = @($ev.not_rust); $crossed = @($ev.cross_entry)
    if ($notListening.Count -gt 0) { $expectPorts = 2; $expectPat = "PREREQ_MISSING" }
    elseif ($nonRust.Count -gt 0 -or $crossed.Count -gt 0) { $expectPorts = 1; $expectPat = "RED: 端口" }
    else { $expectPorts = 0; $expectPat = "GREEN: 四个端口" }
    $note = "notListening={0} nonRust={1} crossed={2}" -f ($notListening -join "/"), ($nonRust -join "/"), ($crossed -join "/")
  }
  Test-Case "ports/实况判属（Rust绿/C#红/缺前置）" $expectPorts $portsRun $expectPat $note
}

# ---------------------------------------------------------------- ⓪ 前置门
# 注意：全部用 `-SkipTools`/`-SkipClient` 或注入不存在的路径/命令，保持**机器无关**（真机工具版本不在判据内）。
Write-Output "⓪ preflight.ps1（外部工具/仓库输入/金标准身份+schema）"
$pf = Join-Path $Here "preflight.ps1"
Test-Case "preflight/在库前置齐备=GREEN" 0 (Run-Ps $pf @("-SkipTools", "-SkipClient", "-OutFile", (Join-Path $Work "preflight-ok.json"))) "GREEN: 前置齐备" "仓库输入 + 金标准 schema/身份"
Test-Case "preflight/缺工具=PREREQ" 2 (Run-Ps $pf @("-PythonCmd", "m2-no-such-python-xyz", "-SkipClient", "-OutFile", (Join-Path $Work "preflight-badtool.json"))) "PREREQ_MISSING" "注入不存在的 python"
Test-Case "preflight/缺客户端入口=PREREQ" 2 (Run-Ps $pf @("-SkipTools", "-ClientDir", (Join-Path $Work "no-client"), "-OutFile", (Join-Path $Work "preflight-noclient.json"))) "PREREQ_MISSING" "客户端入口路径不存在"
$badGolden = Join-Path $Work "golden-bad-schema.jsonl"
Write-Jsonl $badGolden @([pscustomobject]@{ kind = "frame"; dir = "s2c"; seq = 1 })
Test-Case "preflight/金标准 schema 坏=RED" 1 (Run-Ps $pf @("-SkipTools", "-SkipClient", "-AllowGoldenDrift", "-GoldenPath", $badGolden, "-OutFile", (Join-Path $Work "preflight-badgolden.json"))) "RED: 前置在但已损坏" "缺 8 个必需字段（-AllowGoldenDrift 隔离出 schema 独立牙齿）"

# ---------------------------------------------------------------- 聚合器
Write-Output "run_acceptance.ps1（聚合口径）"
$run = Join-Path $Here "run_acceptance.ps1"
Test-Case "agg/全PREREQ=2（不静默通过）" 2 (Run-Ps $run @("-SelfCheckNoStack")) "PREREQ_MISSING" "无栈自检"
Test-Case "agg/有RED=1（红不许被说成前置）" 1 (Run-Ps $run @("-SkipPorts", "-SkipS2c", "-SkipDb", "-ShotsDir", $s6)) "RED: 有步骤判红" "shots 6/7 判红"

# ---------------------------------------------------------------- 汇总
$report = [ordered]@{
  tool = "m2-acceptance/selftest.ps1"
  ts = (Get-Date).ToString("o")
  golden = $Golden
  live_ports = $LivePorts
  total = $script:Total
  failed = $script:Failed
  checks = $script:Checks
  pass = ($script:Failed -eq 0)
}
$reportPath = Join-Path $OutDir "selftest.json"
$report | ConvertTo-Json -Depth 6 | Set-Content -Path $reportPath -Encoding utf8
Write-Output ("SELFTEST_REPORT " + $reportPath)
Write-Output ("SUMMARY total={0} failed={1}" -f $script:Total, $script:Failed)
if ($script:Failed -eq 0) {
  Write-Output "GREEN: 手册每一步的判红/判绿口径都经构造输入验证（不含真栈验收本身）"
  exit 0
}
Write-Output "SELFTEST FAIL: 有判据口径与实现不符（见上面 FAIL 行）"
exit 1
