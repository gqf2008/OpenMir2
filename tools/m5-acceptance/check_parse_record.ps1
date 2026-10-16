#requires -Version 5
<#
.SYNOPSIS
  check_parse_record.ps1 —— 校验 `parse-check.txt` 记录的 bytes/sha256 与目录内脚本**当前实况**一致。

.DESCRIPTION
  背景（片 91 发现）：`parse-check.txt` 记录的是**生成时刻**的 bytes/sha256；脚本事后被改而记录不刷新，
  就会出现「同一 commit 内自相矛盾」的证据漂移（旧记录 18165/28333 vs 实况 20983/32821）。
  本自检把「记录 ↔ 实况」对齐变成可机判判据：任何一处不符即 RED（rc=1）。

  只读：不执行被检脚本、不起进程/栈/端口。

  牙（改坏必红）：把记录里任一 sha256 改一位（或改 bytes）⇒ rc=1。

.EXAMPLE
  pwsh -File tools/m5-acceptance/check_parse_record.ps1
.EXAMPLE
  # 变异对照：篡改后的记录副本 ⇒ 必须 rc=1
  pwsh -File tools/m5-acceptance/check_parse_record.ps1 -Record /tmp/mut_parse-check.txt
#>
param(
  [string]$Record = (Join-Path $PSScriptRoot 'parse-check.txt'),
  [string]$Dir    = $PSScriptRoot
)

$ErrorActionPreference = 'Stop'

function Fail([string]$msg) { Write-Output ("RED: " + $msg); exit 1 }

if (-not (Test-Path $Record)) { Fail ("记录文件不存在: " + $Record) }

$re = '^(?<name>[^:]+\.ps1):\s+PARSE OK\s+bytes=(?<bytes>\d+)\s+sha256=(?<sha>[0-9a-fA-F]{64})\s*$'
$records = @()
foreach ($line in (Get-Content -Raw $Record).Split("`n")) {
  $m = [regex]::Match($line, $re)
  if ($m.Success) {
    $records += [pscustomobject]@{
      name  = $m.Groups['name'].Value.Trim()
      bytes = [int64]$m.Groups['bytes'].Value
      sha   = $m.Groups['sha'].Value.ToLower()
    }
  }
}
if ($records.Count -eq 0) { Fail ("记录中未找到任何 PARSE OK 行: " + $Record) }

$bad = 0
foreach ($r in $records) {
  $p = Join-Path $Dir $r.name
  if (-not (Test-Path $p)) { Write-Output ("DRIFT {0}: 记录存在但文件缺失" -f $r.name); $bad++; continue }
  $abytes = (Get-Item $p).Length
  $asha = (Get-FileHash -Algorithm SHA256 $p).Hash.ToLower()
  if ($abytes -ne $r.bytes -or $asha -ne $r.sha) {
    Write-Output ("DRIFT {0}: 记录 bytes={1} sha256={2} / 实况 bytes={3} sha256={4}" -f `
      $r.name, $r.bytes, $r.sha.Substring(0,16), $abytes, $asha.Substring(0,16))
    $bad++
  } else {
    Write-Output ("OK {0}: bytes={1} sha256={2}…" -f $r.name, $abytes, $asha.Substring(0,16))
  }
}
if ($bad -gt 0) { Fail ("{0}/{1} 条记录与实况不符（证据漂移）" -f $bad, $records.Count) }

Write-Output ("GREEN: check_parse_record {0}/{0} 记录与实况一致" -f $records.Count)
exit 0
