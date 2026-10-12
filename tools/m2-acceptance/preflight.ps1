#requires -Version 5
<#
.SYNOPSIS
  preflight.ps1 — M2 验收运行手册的「前置门」：栈还没起、产物还没采之前，先把**外部工具 + 仓库输入 + 金标准身份**一次点清。

.DESCRIPTION
  `run_acceptance.ps1` 的四步只在「栈已起 / 产物已采」时才判 RED/GREEN；它自己的前置（python/dotnet/cargo、
  抓包/解码/DB 工具、金标准）此前**没有任何一步在门内**：
    * `python`/`dotnet`/`cargo` 缺失 ⇒ 子脚本内部 `python ...` 直接抛终止错误，聚合器看到的是「RED」而非「前置未满足」；
    * 金标准被换成 A-9 旧口径件（`session-20261010-c3-hook/capture.jsonl`）时，③b 会在 811 出一对**假红**，
      而 README 里对金标准身份的两处 sha256 口径本身还互相矛盾（已在本片一并修正）。

  本脚本把上面前置显式化：逐项打印 `OK/MISSING/RED/INFO`，写证据文件，并据此退出：
    * `0`  = 必需前置齐备（栈/产物仍由 `run_acceptance.ps1` 判）；
    * `2`  = `PREREQ_MISSING`：有必需项缺失（缺工具/缺仓库输入/缺真客户端入口/金标准文件不在）——
             **绝不是 RED，也绝不静默通过**；
    * `1`  = `RED`：前置**在，但坏了**——金标准 sha256 与钉住值不符、或 schema 缺必需字段
             （这会让 ③b 的判据失真，属于仓库缺陷，不是"没准备好"）。

  只读：不起栈、不起客户端、不监听任何端口；MySQL 只做一次 TCP `connect`（不 bind）。
  `-SkipTools`/`-SkipClient`/`-SkipRepoInputs` 供自检构造机器无关用例。

.PARAMETER ExpectedGoldenSha
  当前口径金标准 `c-line-baseline-c3-20261010.jsonl` 的 sha256（**金标准身份即合同**）。
  不符 ⇒ RED（除非 `-AllowGoldenDrift`）。金标准若合法换代，必须同时改这里与 README §3。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/preflight.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/preflight.ps1 -OutFile E:\tmp\m2-acceptance\preflight.json
#>
param(
  [string]$GoldenPath = "",
  [string]$ExpectedGoldenSha = "f34a6d111892912436eeb0d50cf2effc7e6f2915bf6ebfbf583104d3409a7ed9",
  [switch]$AllowGoldenDrift,
  [string]$ClientDir = "D:\MirClient-run",
  [string]$ClientEntryFile = "run-release.cmd",
  [string]$PythonCmd = "python",
  [string]$DotnetCmd = "dotnet",
  [string]$CargoCmd = "cargo",
  [switch]$SkipTools,
  [switch]$SkipClient,
  [switch]$SkipRepoInputs,
  [switch]$RequireMysql,
  [string]$MysqlHost = "127.0.0.1",
  [int]$MysqlPort = 3306,
  [string]$OutFile = ""
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
if ([string]::IsNullOrEmpty($GoldenPath)) {
  $GoldenPath = Join-Path $RepoRoot "mir2-rs\tests\golden\c-line-baseline-c3-20261010.jsonl"
}
if ([string]::IsNullOrEmpty($OutFile)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $OutFile = Join-Path "E:\tmp\m2-acceptance" ("preflight-$stamp.json")
}

# ③b 对账器读这两处既有实现；缺任一个 ⇒ 对账/解码不可用。
$RepoInputs = @(
  @{ rel = "tools\capture\mir2_proxy.py";                       why = "抓包（客户端↔7200）" },
  @{ rel = "tools\capture\segment_frames.py";                   why = "切帧" },
  @{ rel = "tools\capture\dump_body_layout.py";                 why = "当前分段布局（防 A-9 假红）" },
  @{ rel = "tools\capture\GoldenExport\GoldenExport.csproj";    why = "解码成 capture.jsonl" },
  @{ rel = "tools\client\mir_flow.ps1";                         why = "真客户端驱动/截图" },
  @{ rel = "tools\dbsnap\dbsnap.py";                            why = "13 表快照" },
  @{ rel = "mir2-rs\tests\parity\m3\scoped_diff.py";            why = "作用域字段级 diff（DB 三路径）" },
  @{ rel = "mir2-rs\tests\parity\loginsrv\oracle_capture.py";   why = "既有 EDCode（s2c recog 归一化复用）" },
  @{ rel = "mir2-rs\tools\replay\Cargo.toml";                   why = "既有 replay 自洽门禁" }
)
# ③b 要求的字段口径（brief §3：dir/seq/port/hop/conn/raw_hex/ident/param/tag/series）。
$RequiredGoldenKeys = @(
  "dir", "seq", "port", "hop", "conn", "raw_hex", "ident", "param", "tag", "series"
)

$items = New-Object System.Collections.Generic.List[object]
function Add-Item([string]$Kind, [string]$Name, [string]$State, [string]$Detail) {
  $items.Add([pscustomobject]@{ kind = $Kind; name = $Name; state = $State; detail = $Detail })
  Write-Output ("  [{0,-7}] {1,-34} {2}" -f $State, $Name, $Detail)
}

Write-Output ("M2 验收前置检查（preflight；只读，不起栈/不占端口）  repo=" + $RepoRoot)

# ---------------------------------------------------------------- 外部工具
if (-not $SkipTools) {
  Write-Output "tool:"
  foreach ($t in @(
      @{ cmd = $PythonCmd; name = "python" },
      @{ cmd = $DotnetCmd; name = "dotnet" },
      @{ cmd = $CargoCmd;  name = "cargo" }
    )) {
    $found = Get-Command $t.cmd -ErrorAction SilentlyContinue
    if ($found) { Add-Item "tool" ("tool:" + $t.name) "OK" $found.Source }
    else { Add-Item "tool" ("tool:" + $t.name) "MISSING" ("找不到命令 " + $t.cmd) }
  }
  # dotnet 只在 GoldenExport 未预构建时才必需（export_s2c.ps1 会回落到 `dotnet run`）。
  $prebuilt = Join-Path $RepoRoot "tools\capture\GoldenExport\bin\Release\net8.0\GoldenExport.exe"
  $hasDotnet = [bool](Get-Command $DotnetCmd -ErrorAction SilentlyContinue)
  if ((Test-Path $prebuilt) -or $hasDotnet) {
    $geDetail = "dotnet 在（可 dotnet run）"
    if (Test-Path $prebuilt) { $geDetail = "预构建 exe 在" }
    Add-Item "tool" "tool:GoldenExport可用" "OK" $geDetail
  } else {
    Add-Item "tool" "tool:GoldenExport可用" "MISSING" "既无预构建 exe 也无 dotnet —— ③a 无法解码出 capture.jsonl"
  }
}

# ---------------------------------------------------------------- 仓库输入
if (-not $SkipRepoInputs) {
  Write-Output "input:"
  foreach ($ri in $RepoInputs) {
    $p = Join-Path $RepoRoot $ri.rel
    if (Test-Path $p) { Add-Item "input" ("input:" + $ri.rel) "OK" $ri.why }
    else { Add-Item "input" ("input:" + $ri.rel) "MISSING" ("缺仓库工具（" + $ri.why + "）") }
  }
}

# ---------------------------------------------------------------- 金标准（文件 + schema + 身份）
Write-Output "golden:"
$goldenPresent = Test-Path $GoldenPath
if (-not $goldenPresent) {
  Add-Item "golden" "golden:文件" "MISSING" ("金标准不存在 " + $GoldenPath)
} else {
  # 用 .NET 算 sha256，**不要**用 `Get-FileHash`：后者属 `Microsoft.PowerShell.Utility`，在父进程把
  # `PSModulePath` 指成 PS7 优先的环境里，5.1 子进程会加载 PS7 的 Utility（不导出该 cmdlet）⇒ 脚本**崩溃**
  # 并被上层（selftest/run_acceptance）误判为 RED。.NET 不依赖模块解析，结果与 `-Algorithm SHA256` 逐字节相同。
  $shaAlg = [System.Security.Cryptography.SHA256]::Create()
  try {
    $sha = ([System.BitConverter]::ToString($shaAlg.ComputeHash([System.IO.File]::ReadAllBytes($GoldenPath))) -replace '-', '').ToLowerInvariant()
  } finally { $shaAlg.Dispose() }
  Add-Item "golden" "golden:文件" "OK" ("{0} sha256={1}" -f $GoldenPath, $sha)
  # 身份：金标准即合同。换成 A-9 旧件会在 811 出假红（brief/README 明写）。
  if ($sha -eq $ExpectedGoldenSha.ToLowerInvariant()) {
    Add-Item "golden" "golden:sha256" "OK" $sha
  } elseif ($AllowGoldenDrift) {
    Add-Item "golden" "golden:sha256" "INFO" ("{0} != 钉住 {1}（-AllowGoldenDrift 放行，需登记）" -f $sha, $ExpectedGoldenSha)
  } else {
    Add-Item "golden" "golden:sha256" "RED" ("{0} != 钉住 {1}（金标准身份不符；如属合法换代须同步 preflight+README）" -f $sha, $ExpectedGoldenSha)
  }
  # schema：brief 要求的字段口径必须在（否则 ③b 的判据会静默少比字段）。
  $schemaOk = $false
  $schemaDetail = ""
  try {
    $firstLine = (Get-Content -Path $GoldenPath -TotalCount 1)
    $obj = $firstLine | ConvertFrom-Json
    $names = @($obj.PSObject.Properties.Name)
    $missKeys = @($RequiredGoldenKeys | Where-Object { $names -notcontains $_ })
    if ($missKeys.Count -eq 0) { $schemaOk = $true; $schemaDetail = ($RequiredGoldenKeys -join ",") }
    else { $schemaDetail = ("缺字段: " + ($missKeys -join ",")) }
  } catch {
    $schemaDetail = ("首行不是合法 JSON：" + $_.Exception.Message)
  }
  if ($schemaOk) { Add-Item "golden" "golden:schema" "OK" $schemaDetail }
  else { Add-Item "golden" "golden:schema" "RED" $schemaDetail }
}

# ---------------------------------------------------------------- 真客户端入口
if (-not $SkipClient) {
  Write-Output "client:"
  $entry = Join-Path $ClientDir $ClientEntryFile
  if (Test-Path $entry) {
    Add-Item "client" "client:entry" "OK" $entry
  } else {
    Add-Item "client" "client:entry" "MISSING" ("找不到真客户端入口 " + $entry + "（验收②截图需要）")
  }
}

# ---------------------------------------------------------------- MySQL（默认只登记，-RequireMysql 才计入必需）
Write-Output "mysql:"
$mysqlOk = $false
try {
  $c = New-Object System.Net.Sockets.TcpClient
  $iar = $c.BeginConnect($MysqlHost, $MysqlPort, $null, $null)
  $mysqlOk = $iar.AsyncWaitHandle.WaitOne(1500)
  $c.Close()
} catch { $mysqlOk = $false }
if ($mysqlOk) {
  Add-Item "mysql" "mysql:reachable" "OK" ("{0}:{1} 可连" -f $MysqlHost, $MysqlPort)
} elseif ($RequireMysql) {
  Add-Item "mysql" "mysql:reachable" "MISSING" ("{0}:{1} 不可达（DB 三路径快照需要）" -f $MysqlHost, $MysqlPort)
} else {
  Add-Item "mysql" "mysql:reachable" "INFO" ("{0}:{1} 不可达（DB 步将 PREREQ；需要时加 -RequireMysql 计入必需）" -f $MysqlHost, $MysqlPort)
}

# ---------------------------------------------------------------- 汇总
$red = @($items | Where-Object { $_.state -eq "RED" }).Count
$missing = @($items | Where-Object { $_.state -eq "MISSING" }).Count
$ok = @($items | Where-Object { $_.state -eq "OK" }).Count
$info = @($items | Where-Object { $_.state -eq "INFO" }).Count

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null
[ordered]@{
  tool = "m2-acceptance/preflight.ps1"
  ts = (Get-Date).ToString("o")
  repo = $RepoRoot
  golden = $GoldenPath
  expected_golden_sha = $ExpectedGoldenSha
  required_golden_keys = $RequiredGoldenKeys
  items = $items
  ok = $ok; missing = $missing; red = $red; info = $info
  pass = ($red -eq 0 -and $missing -eq 0)
} | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
Write-Output ("EVIDENCE " + $OutFile)
Write-Output ("SUMMARY ok={0} missing={1} red={2} info={3}" -f $ok, $missing, $red, $info)

if ($red -gt 0) {
  Write-Output "RED: 前置在但已损坏（金标准身份/schema 不符）——修好前验收③b 的判据不可信"
  exit 1
}
if ($missing -gt 0) {
  $missingNames = @($items | Where-Object { $_.state -eq "MISSING" } | ForEach-Object { $_.name })
  Write-Output ("PREREQ_MISSING: 有必需前置缺失 -> " + ($missingNames -join ", "))
  Write-Output "  齐备后再跑 run_acceptance.ps1 取验收①②③的活栈判据"
  exit 2
}
Write-Output "GREEN: 前置齐备（外部工具/仓库输入/金标准身份与 schema；栈与产物仍由 run_acceptance.ps1 判）"
exit 0
