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
    5. 缺 chunks.ndjson ⇒ USAGE rc=2。

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

function Invoke-Verifier([string]$dir) {
  $old = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $verifier -CaptureDir $dir 2>&1
    $code = $LASTEXITCODE
  } finally { $ErrorActionPreference = $old }
  return [pscustomobject]@{ Code = $code; Out = (($out | ForEach-Object { [string]$_ }) -join "`n") }
}

function Get-Sha256Hex([byte[]]$bytes, [int]$off, [int]$n) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    $part = New-Object byte[] $n
    [Array]::Copy($bytes, $off, $part, 0, $n)
    return ([BitConverter]::ToString($sha.ComputeHash($part))).Replace('-', '').ToLowerInvariant()
  } finally { $sha.Dispose() }
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
  Check "1-byte tamper => RED rc=1" ($r.Code -eq 1 -and $r.Out.Contains("CHUNKS_VERIFY RED")) ($r.Out + " rc=" + $r.Code)
  Check "1-byte tamper names conn=1 dir=s2c off=0" ($r.Out.Contains("conn=1 dir=s2c off=0") -and $r.Out.Contains("指纹不符")) $r.Out

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
}
finally {
  Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output ("VERIFY_CHUNKS_SELFTEST_PASS=" + $script:pass + " FAIL=" + $script:fails.Count)
foreach ($n in $script:fails) { Write-Output ("FAILED: " + $n) }
if ($script:fails.Count -gt 0) { exit 1 }
Write-Output "VERIFY_CHUNKS_SELFTEST_OK"
exit 0
