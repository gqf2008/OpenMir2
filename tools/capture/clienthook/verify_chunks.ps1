#requires -Version 5
<#
.SYNOPSIS
  verify_chunks.ps1 —— 校验 clienthook 产物 chunks.ndjson 的 `sha256` 指纹。

.DESCRIPTION
  Mir2ClientHook.dll 为每个 data 事件写
    {"event":"data","conn":N,"port":P,"hop":H,"dir":"c2s|s2c","off":O,"n":L,"sha256":S,...}
  这里按同一口径复算：
    S == SHA256(conn-NNNN.<dir>.bin 的 [O, O+L) 字节) 的小写十六进制。

  判据（任何一条不满足 ⇒ RED，退出码 1）：
    · `sha256` 不得为空串（旧 DLL 恒空 ⇒ 拿它做「字节未变」断言会假绿）；
    · 区间必须在 bin 内；
    · 复算值必须与记录值一致（逐 data 事件，不是只看总量）。

  退出码：0 全绿；1 RED；2 用法错误/缺少 chunks.ndjson。
  只读；不写任何文件。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File verify_chunks.ps1 -CaptureDir E:\tmp\run1\hook
#>
param(
  [Parameter(Mandatory = $true)][string]$CaptureDir,
  [switch]$Quiet
)
$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $CaptureDir -PathType Container)) {
  Write-Output ("CHUNKS_VERIFY USAGE 目录不存在：" + $CaptureDir)
  exit 2
}
$full = (Resolve-Path -LiteralPath $CaptureDir).Path
$chunks = Join-Path $full "chunks.ndjson"
if (-not (Test-Path -LiteralPath $chunks -PathType Leaf)) {
  Write-Output ("CHUNKS_VERIFY USAGE 缺少 chunks.ndjson：" + $chunks)
  exit 2
}

$sha256 = [Security.Cryptography.SHA256]::Create()
$binCache = @{}
$bad = 0
$events = 0
$bytes = [long]0
$distinct = @{}

try {
  $lineNo = 0
  foreach ($line in [IO.File]::ReadLines($chunks)) {
    $lineNo++
    if ($line -notmatch '"event"\s*:\s*"data"') { continue }
    $r = $null
    try { $r = $line | ConvertFrom-Json } catch {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " 解析失败：" + $_.Exception.Message)
      continue
    }
    $missing = $null
    foreach ($k in @("conn", "dir", "off", "n")) {
      if ($null -eq $r.PSObject.Properties[$k]) { $missing = $k; break }
    }
    if ($null -ne $missing) {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " 缺字段 " + $missing)
      continue
    }
    $conn = [int]$r.conn
    $dir = [string]$r.dir
    $off = [long]$r.off
    $n = [long]$r.n
    $got = ""
    if ($null -ne $r.PSObject.Properties["sha256"] -and $null -ne $r.sha256) { $got = ([string]$r.sha256).Trim() }
    $events++
    $tag = "conn=" + $conn + " dir=" + $dir + " off=" + $off + " n=" + $n

    if ($dir -ne "c2s" -and $dir -ne "s2c") {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " " + $tag + " 未知 dir")
      continue
    }
    if ($n -le 0) {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " " + $tag + " n<=0")
      continue
    }
    if ([string]::IsNullOrEmpty($got)) {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " " + $tag + " sha256 为空串（旧产物/DLL 未计算）")
      continue
    }
    if ($got -notmatch '^[0-9A-Fa-f]{64}$') {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " " + $tag + " sha256 不是 64 位十六进制：" + $got)
      continue
    }

    $binName = ("conn-{0:0000}.{1}.bin" -f $conn, $dir)
    $binPath = Join-Path $full $binName
    if (-not $binCache.ContainsKey($binPath)) {
      if (Test-Path -LiteralPath $binPath -PathType Leaf) {
        $binCache[$binPath] = [IO.File]::ReadAllBytes($binPath)
      } else {
        $binCache[$binPath] = $null
      }
    }
    $data = $binCache[$binPath]
    if ($null -eq $data) {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " " + $tag + " bin 缺失：" + $binName)
      continue
    }
    $distinct[$binName] = $true
    if (($off + $n) -gt [long]$data.Length) {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " " + $tag + " 区间越界（bin=" + $data.Length + " B）")
      continue
    }
    $slice = New-Object byte[] ([int]$n)
    [Array]::Copy($data, [int]$off, $slice, 0, [int]$n)
    $actual = ([BitConverter]::ToString($sha256.ComputeHash($slice))).Replace('-', '').ToLowerInvariant()
    $bytes += $n
    if ($actual -ne $got.ToLowerInvariant()) {
      $bad++
      Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " " + $tag + " 指纹不符：记录=" + $got.Substring(0, [Math]::Min(16, $got.Length)) + "… 实际=" + $actual.Substring(0, 16) + "…")
    }
  }
}
finally {
  $sha256.Dispose()
}

if ($events -eq 0) {
  Write-Output ("CHUNKS_VERIFY RED data 事件 0 条（扫了 0 个对象 —— 不得当绿）：" + $chunks)
  exit 1
}

if ($bad -gt 0) {
  Write-Output ("CHUNKS_VERIFY RED bad=" + $bad + " events=" + $events + " bins=" + $distinct.Count + " bytes=" + $bytes + " dir=" + $full)
  exit 1
}
Write-Output ("CHUNKS_VERIFY GREEN events=" + $events + " bins=" + $distinct.Count + " bytes=" + $bytes + " dir=" + $full)
exit 0
