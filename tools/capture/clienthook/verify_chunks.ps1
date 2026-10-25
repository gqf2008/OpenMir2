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

  退出码四态（A5：**「调用失败」必须与「校验 RED」可区分**）：
    0 = GREEN   全绿（扫到 ≥1 条 data 事件且逐条指纹一致）；
    1 = RED     **证据被证伪**：指纹不符 / 空串 / 越界 / 0 条 data 事件；
    2 = USAGE   **用法错，压根没校验**：未知参数名、缺参数值、目录不存在、缺 chunks.ndjson；
    3 = ERROR   **校验没跑完**：脚本内部异常（读文件失败等）。

  为什么没有 param() 块：PowerShell 的**参数绑定发生在脚本体执行之前**，绑定失败会
  直接以 rc=1 退出——与「指纹不符」的 rc=1 撞车（实测 `-Dir` 这种错参数名就是如此，
  脚本内部「目录不存在 ⇒ rc=2」根本没机会执行）。所以这里改为**手动解析 $args**，
  任何用法错误一律走到 rc=2 + `CHUNKS_VERIFY USAGE`。

  只读；不写任何文件。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File verify_chunks.ps1 -CaptureDir E:\tmp\run1\hook

.EXAMPLE
  # 调用方判定：$LASTEXITCODE -eq 0 才算「指纹已证明」；2/3 是「没校验」，不得当 RED 也不得当绿。
#>
$ErrorActionPreference = "Stop"

function Write-VerifierUsage([string]$why) {
  Write-Output ("CHUNKS_VERIFY USAGE " + $why)
  Write-Output "CHUNKS_VERIFY USAGE 用法：verify_chunks.ps1 -CaptureDir <hook 产物目录> [-Quiet]"
  Write-Output "CHUNKS_VERIFY USAGE 退出码：0=GREEN 1=RED(证据被证伪) 2=USAGE(用法错，未校验) 3=ERROR(内部异常)"
}

function Invoke-ChunkVerify([string]$captureDir) {
  if (-not (Test-Path -LiteralPath $captureDir -PathType Container)) {
    Write-VerifierUsage ("目录不存在：" + $captureDir)
    exit 2
  }
  $full = (Resolve-Path -LiteralPath $captureDir).Path
  $chunks = Join-Path $full "chunks.ndjson"
  if (-not (Test-Path -LiteralPath $chunks -PathType Leaf)) {
    Write-VerifierUsage ("缺少 chunks.ndjson：" + $chunks)
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
      try {
        $conn = [int]$r.conn
        $dir = [string]$r.dir
        $off = [long]$r.off
        $n = [long]$r.n
      } catch {
        $bad++
        Write-Output ("CHUNKS_VERIFY RED line=" + $lineNo + " 字段类型非法：" + $_.Exception.Message)
        continue
      }
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

  # 注意：函数用 Write-Output 打屏，**不能用 return <code> 传退出码**（返回值会被并进
  # 管道输出，调用方拿到的是数组）；退出码走脚本级 $script:verifyCode。
  if ($events -eq 0) {
    Write-Output ("CHUNKS_VERIFY RED data 事件 0 条（扫了 0 个对象 —— 不得当绿）：" + $chunks)
    $script:verifyCode = 1
    return
  }

  if ($bad -gt 0) {
    Write-Output ("CHUNKS_VERIFY RED bad=" + $bad + " events=" + $events + " bins=" + $distinct.Count + " bytes=" + $bytes + " dir=" + $full)
    $script:verifyCode = 1
    return
  }
  Write-Output ("CHUNKS_VERIFY GREEN events=" + $events + " bins=" + $distinct.Count + " bytes=" + $bytes + " dir=" + $full)
  $script:verifyCode = 0
}

# ---- 手动参数解析（无 param() 块 ⇒ 不存在「绑定阶段 rc=1」这条歧义路径）----
$captureDir = ""
$quiet = $false
for ($i = 0; $i -lt $args.Count; $i++) {
  $a = [string]$args[$i]
  if ($a -eq "-Quiet") { $quiet = $true; continue }
  if ($a -match '^-CaptureDir(?::(.*))?$') {
    if ($null -ne $Matches[1] -and $Matches[1] -ne "") {
      $captureDir = $Matches[1]
    } elseif ($i + 1 -lt $args.Count -and -not ([string]$args[$i + 1]).StartsWith("-")) {
      $i++
      $captureDir = [string]$args[$i]
    } else {
      Write-VerifierUsage "-CaptureDir 缺少取值"
      exit 2
    }
    continue
  }
  Write-VerifierUsage ("未知参数：" + $a + "（本脚本只接受 -CaptureDir / -Quiet；错参数名不当作校验结果）")
  exit 2
}
if ($captureDir -eq "") {
  Write-VerifierUsage "-CaptureDir 必填"
  exit 2
}

$script:verifyCode = 1
try {
  Invoke-ChunkVerify $captureDir
} catch {
  Write-Output ("CHUNKS_VERIFY ERROR 内部异常（校验没跑完，不等于 RED）：" + $_.Exception.Message)
  exit 3
}
exit $script:verifyCode
