#requires -Version 5
<#
.SYNOPSIS
  check_db_paths.ps1 — M2 验收③「小退/掉线/服务关闭 三条退出路径的 DB 字段级 diff=0」。

.DESCRIPTION
  复用仓库既有 DB 工具，不新写一套：
    * 快照/字段级抽取：`tools/dbsnap/dbsnap.py`（对 13 张表确定性导出 + sha256 manifest）
    * 作用域字段级比对：`mir2-rs/tests/parity/m3/scoped_diff.py`（M3 对拍判据本体）

  三条退出路径（S4 DoD ③）：
    softclose   客户端点「小退」（CM_SOFTCLOSE 1009）回到选角/登录
    disconnect  客户端掉线/断连（进程被杀或网络断）
    shutdown    服务端被关闭（GameSvr/整栈停）

  一次「侧运行」的产物布局（每侧每路径一组）：
    <SideDir>\<path>-before.jsonl          操作前的命名空间内行（normalized）
    <SideDir>\<path>-after.jsonl           操作后的命名空间内行（normalized）
    <SideDir>\snap-<path>-before\          dbsnap 快照目录（13 表 + manifest.json）
    <SideDir>\snap-<path>-after\
  C# 基线与 Rust 侧必须用**同一个前缀**（`<prefix><序号>` 命名空间），路径名一致。

  Mode：
    snap    `-Path <softclose|disconnect|shutdown> -Phase <before|after> -Prefix <run> -SnapDir <dir> -OutDir <dir>`
            对当前库做一次快照并抽取本次命名空间内的行。**前置**：MySQL 3306 在跑。
    diff    `-CsharpDir <dir> -RustDir <dir>` 对三条路径逐条 `scoped_diff.py diff`，任一 != 0 即红。

  红/绿：
    GREEN  = 三条路径的 after 作用域字段级 diff 全为 0；
    PREREQ = MySQL 不可达 / 侧目录或某个 `<path>-after.jsonl` 缺失 ⇒ 退出码 2（不静默通过）；
    RED    = 任一路径 diff != 0 ⇒ 退出码 1。

.EXAMPLE
  # 采基线（C# 栈）：对三条路径各做 before/after（中间由验收人执行对应退出动作）
  powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase before -Prefix m2acc1 -SnapDir E:\tmp\.. -OutDir E:\tmp\m2acc\csharp
  # Rust 侧同前缀同路径
  ...
  # 对拍
  powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode diff -CsharpDir E:\tmp\m2acc\csharp -RustDir E:\tmp\m2acc\rust
#>
param(
  [ValidateSet("diff", "snap")][string]$Mode = "diff",
  [ValidateSet("softclose", "disconnect", "shutdown")][string]$Path = "softclose",
  [ValidateSet("before", "after")][string]$Phase = "before",
  [string]$Prefix = "",
  [string]$SnapDir = "",
  [string]$OutDir = "",
  [string]$CsharpDir = "",
  [string]$RustDir = "",
  [string]$OutFile = "",
  [string]$MysqlHost = "127.0.0.1",
  [int]$MysqlPort = 3306,
  [string]$MysqlUser = "root",
  [string]$MysqlPassword = ""
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$Dbsnap = Join-Path $RepoRoot "tools\dbsnap\dbsnap.py"
$Scoped = Join-Path $RepoRoot "mir2-rs\tests\parity\m3\scoped_diff.py"
$Paths = @("softclose", "disconnect", "shutdown")

# 两个模式都会 shell out 到 python（dbsnap/scoped_diff）。缺 python 会抛终止错误、被聚合器误当 RED，
# 故先显式报 PREREQ（preflight.ps1 也先行拦一道）。
if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
  Write-Output "PREREQ_MISSING: 找不到 python（dbsnap/scoped_diff 需要；见 preflight.ps1）"
  exit 2
}

function Test-Mysql {
  try {
    $c = New-Object System.Net.Sockets.TcpClient
    $iar = $c.BeginConnect($MysqlHost, $MysqlPort, $null, $null)
    $ok = $iar.AsyncWaitHandle.WaitOne(1500)
    $c.Close()
    return $ok
  } catch { return $false }
}

if ($Mode -eq "snap") {
  if ([string]::IsNullOrEmpty($Prefix) -or [string]::IsNullOrEmpty($SnapDir) -or [string]::IsNullOrEmpty($OutDir)) {
    Write-Output "PREREQ_MISSING: snap 模式需要 -Prefix <run> -SnapDir <dir> -OutDir <dir>"
    exit 2
  }
  if (-not (Test-Mysql)) {
    Write-Output ("PREREQ_MISSING: MySQL {0}:{1} 不可达——先起库再取快照" -f $MysqlHost, $MysqlPort)
    exit 2
  }
  New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
  Write-Output ("--- dbsnap snap [{0}/{1}] -> {2} ---" -f $Path, $Phase, $SnapDir)
  # 注意：dbsnap.py 的 --host/--port/--user/--password 是**顶层**选项，必须在子命令 snap 之前。
  python $Dbsnap --host $MysqlHost --port $MysqlPort --user $MysqlUser --password $MysqlPassword snap --out $SnapDir
  if ($LASTEXITCODE -ne 0) { Write-Output "RED: dbsnap snap 失败"; exit 1 }
  $outJsonl = Join-Path $OutDir ("{0}-{1}.jsonl" -f $Path, $Phase)
  Write-Output ("--- scoped_diff extract prefix={0} -> {1} ---" -f $Prefix, $outJsonl)
  python $Scoped extract $SnapDir $Prefix $outJsonl --normalize --allow-empty
  if ($LASTEXITCODE -ne 0) { Write-Output "RED: scoped_diff extract 失败"; exit 1 }
  Write-Output ("GREEN: 已落 {0}" -f $outJsonl)
  exit 0
}

# ---- Mode=diff ----
if ([string]::IsNullOrEmpty($CsharpDir) -or [string]::IsNullOrEmpty($RustDir)) {
  Write-Output "PREREQ_MISSING: diff 模式需要 -CsharpDir <dir> 与 -RustDir <dir>"
  exit 2
}
if ([string]::IsNullOrEmpty($OutFile)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $OutFile = Join-Path "E:\tmp\m2-acceptance" ("db-paths-$stamp.json")
}
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null

$results = @()
$failed = @()
foreach ($p in $Paths) {
  $a = Join-Path $CsharpDir ("{0}-after.jsonl" -f $p)
  $b = Join-Path $RustDir ("{0}-after.jsonl" -f $p)
  if (-not (Test-Path $a) -or -not (Test-Path $b)) {
    Write-Output ("PREREQ_MISSING: 路径 {0} 缺快照抽取件（{1} / {2}）" -f $p, $a, $b)
    $results += [pscustomobject]@{ path = $p; state = "PREREQ_MISSING"; rc = 2 }
    continue
  }
  Write-Output ("--- scoped_diff diff [{0}] C# vs Rust ---" -f $p)
  python $Scoped diff $a $b --summary-out (Join-Path (Split-Path -Parent $OutFile) ("{0}-diff.json" -f $p))
  $rc = $LASTEXITCODE
  if ($rc -eq 0) {
    $results += [pscustomobject]@{ path = $p; state = "GREEN"; rc = 0 }
  } else {
    Write-Output ("RED: 路径 {0} 的 DB 字段级 diff != 0（差异要么修到 0，要么按 §6.0-3 登记白名单）" -f $p)
    $results += [pscustomobject]@{ path = $p; state = "RED"; rc = $rc }
    $failed += $p
  }
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null
[ordered]@{
  tool = "m2-acceptance/check_db_paths.ps1"
  ts = (Get-Date).ToString("o")
  csharp_dir = $CsharpDir; rust_dir = $RustDir
  results = $results
  failed = $failed
  pass = ($failed.Count -eq 0 -and @($results | Where-Object { $_.state -eq "PREREQ_MISSING" }).Count -eq 0)
} | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
Write-Output ("EVIDENCE " + $OutFile)

if (@($results | Where-Object { $_.state -eq "PREREQ_MISSING" }).Count -gt 0) {
  Write-Output "PREREQ_MISSING: 三条退出路径的 DB 快照尚未全部采集，无法判 diff"
  exit 2
}
if ($failed.Count -gt 0) { Write-Output ("RED: 路径 DB diff 未归零: " + ($failed -join ", ")); exit 1 }
Write-Output "GREEN: 小退/掉线/服务关闭 三条路径的 DB 字段级 diff = 0"
exit 0
