#requires -Version 5
<#
.SYNOPSIS
  invoke-bounded.ps1 —— 有界运行一个子进程：**输出落盘**、硬超时、超时杀进程树。

.DESCRIPTION
  背景（卡 `openmir2-mirserver-graceful-stop` B 面）：把常驻服务启动器接进管道截输出
  （`start-all.ps1 2>&1 | Select-Object -Last 15`）会让**子进程继承 stdout 持有管道**，
  调用方要等 EOF，任务 **10h12m 不返回**（2026-10-26 11:51:31 那条车道），直到对端强杀才结束。

  本脚本的契约：
    · 子进程的 stdout/stderr **重定向到文件**（`-OutFile` / `-ErrFile`），**不经过管道**；
    · 调用方只读**文件**，禁止 `| Select-Object -Last N` 这类管道截法；
    · 有硬超时 `-TimeoutSec`（默认 120）；超时用 `taskkill /T /F` 杀**整棵进程树**，
      rc=124（区别于「子进程自己返回的非零」）；
    · **不论子进程/孙进程是否还持有 stdout，本脚本都在 `-TimeoutSec` 内返回**
      （因为 stdout 是文件句柄而不是管道，不存在「等 EOF」）。

  退出码：子进程 exit code；超时 124；用法错 2；内部异常 3。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/invoke-bounded.ps1 `
    -FilePath powershell -Arguments @('-NoProfile','-ExecutionPolicy','Bypass','-File','E:\MirServer\start-all.ps1') `
    -OutFile E:\tmp\start-all.out.log -ErrFile E:\tmp\start-all.err.log -TimeoutSec 180
  Get-Content E:\tmp\start-all.out.log -Tail 40
#>
param(
  [string]$FilePath = '',
  [string[]]$Arguments = @(),
  [string]$OutFile = '',
  [string]$ErrFile = '',
  [int]$TimeoutSec = 120,
  [switch]$Selftest
)

$ErrorActionPreference = 'Stop'

function Invoke-Bounded([string]$exe, [string[]]$argv, [string]$outPath, [string]$errPath, [int]$timeoutSec) {
  # PS 5.1 坑：`Start-Process -RedirectStandardOutput ... -PassThru` 的 ExitCode 恒为空。
  # 因此走 `cmd /c "… > out 2> err"`：cmd 会把子进程的 ERRORLEVEL 作为自己的退出码透传，
  # 而重定向到**文件**（不是管道）正好是本卡要的形态——子进程/孙进程持有的是文件句柄。
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $specPath = Join-Path ([IO.Path]::GetTempPath()) ('bounded-spec-' + [guid]::NewGuid().ToString('N') + '.json')
  $runnerPath = Join-Path ([IO.Path]::GetTempPath()) ('bounded-run-' + [guid]::NewGuid().ToString('N') + '.ps1')
  $rcPath = $outPath + '.rc'
  $spec = [ordered]@{ file = $exe; args = @($argv) }
  [IO.File]::WriteAllText($specPath, ($spec | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding($false)))
  $runner = @'
param([string]$Spec, [string]$RcFile)
$s = Get-Content -LiteralPath $Spec -Raw -Encoding UTF8 | ConvertFrom-Json
& $s.file @($s.args)
$code = $LASTEXITCODE
if ($null -eq $code) { $code = 0 }
Set-Content -LiteralPath $RcFile -Value $code -Encoding ASCII
exit $code
'@
  [IO.File]::WriteAllText($runnerPath, $runner, (New-Object Text.UTF8Encoding($false)))
  $line = ('powershell -NoProfile -ExecutionPolicy Bypass -File "' + $runnerPath + '" -Spec "' + $specPath +
           '" -RcFile "' + $rcPath + '" > "' + $outPath + '" 2> "' + $errPath + '"')
  try {
    $p = Start-Process cmd.exe -ArgumentList '/c', $line -PassThru -WindowStyle Hidden
    $ok = $p.WaitForExit($timeoutSec * 1000)
    if (-not $ok) {
      # 超时：杀整棵树（子进程可能又起了孙进程持有句柄/端口）。
      try { taskkill /PID $p.Id /T /F | Out-Null } catch { }
      try { [void]$p.WaitForExit(5000) } catch { }
      $sw.Stop()
      Write-Host ("BOUNDED_RUN TIMEOUT rc=124 elapsed_ms=" + [int]$sw.Elapsed.TotalMilliseconds +
                  " out=" + $outPath + " err=" + $errPath)
      return 124
    }
    [void]$p.WaitForExit()
    # rc 通道：包装器把子进程的 $LASTEXITCODE 写进 <out>.rc；**优先读文件**
    # （PS 5.1 的 `Start-Process -Redirect*` 不给 ExitCode，见同卡 B1），cmd 退出码只作兜底。
    $rc = $null
    $rcPath = $outPath + '.rc'
    if (Test-Path -LiteralPath $rcPath) {
      $raw = (Get-Content -LiteralPath $rcPath -Raw).Trim()
      if ($raw -match '^-?[0-9]+$') { $rc = [int]$raw }
    }
    if ($null -eq $rc) { $rc = $p.ExitCode }
    if ($null -eq $rc) { $rc = 3 }
    $sw.Stop()
    Write-Host ("BOUNDED_RUN rc=" + $rc + " elapsed_ms=" + [int]$sw.Elapsed.TotalMilliseconds +
                " out=" + $outPath + " err=" + $errPath)
    return $rc
  } finally {
    Remove-Item -LiteralPath $specPath, $runnerPath -Force -ErrorAction SilentlyContinue
  }
}

function Invoke-Selftest {
  $script:spass = 0
  $script:sfail = 0
  function Chk([bool]$ok, [string]$name, [string]$detail = '') {
    if ($ok) { $script:spass++; Write-Host ("PASS " + $name) }
    else { $script:sfail++; Write-Host ("FAIL " + $name + "  " + $detail) }
  }
  $tmp = Join-Path $env:TEMP ('bounded-selftest-' + [guid]::NewGuid().ToString('N').Substring(0, 8))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  try {
    # ① 正常子进程：输出落盘、rc 透传
    $o1 = Join-Path $tmp 's1.out'; $e1 = Join-Path $tmp 's1.err'
    $rc = Invoke-Bounded 'powershell' @('-NoProfile', '-Command', 'Write-Output hello-bounded') $o1 $e1 30
    $txt = if (Test-Path $o1) { [IO.File]::ReadAllText($o1) } else { '' }
    Chk ($rc -eq 0) '正常子进程 => rc=0' ("rc=" + $rc)
    Chk ($txt.Contains('hello-bounded')) '输出落到文件（不经过管道）' $txt

    # ② 子进程自己返回非零：rc 透传（不被当成超时）
    $o2 = Join-Path $tmp 's2.out'; $e2 = Join-Path $tmp 's2.err'
    $rc = Invoke-Bounded 'powershell' @('-NoProfile', '-Command', 'exit 7') $o2 $e2 30
    Chk ($rc -eq 7) '子进程 exit 7 => rc=7（透传，不与超时混淆）' ("rc=" + $rc)

    # ③ 长时间不退出的子进程：硬超时 ⇒ rc=124，且有界返回
    $o3 = Join-Path $tmp 's3.out'; $e3 = Join-Path $tmp 's3.err'
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $rc = Invoke-Bounded 'powershell' @('-NoProfile', '-Command', 'Start-Sleep 60') $o3 $e3 2
    $sw.Stop()
    Chk ($rc -eq 124) '长跑子进程 => rc=124（超时）' ("rc=" + $rc)
    Chk ($sw.Elapsed.TotalSeconds -lt 20) '有界返回（<20s，实际应 ~2s）' ("elapsed=" + [int]$sw.Elapsed.TotalSeconds)

    # ④ 孙进程持有 stdout：启动器立刻退出，但孙进程仍在写同一文件句柄。
    #    文件重定向下不存在「等 EOF」⇒ 必须立刻返回；把孙进程 pid 记下来供清理。
    $pidFile = Join-Path $tmp 'grandchild.pid'
    $o4 = Join-Path $tmp 's4.out'; $e4 = Join-Path $tmp 's4.err'
    $launcher = Join-Path $tmp 'launcher.ps1'
    [IO.File]::WriteAllText($launcher, @'
$gp = Start-Process -FilePath powershell -ArgumentList @('-NoProfile','-Command','Start-Sleep 60') -PassThru
Set-Content -LiteralPath '__PIDFILE__' -Value $gp.Id
Write-Output 'launcher-done'
'@.Replace('__PIDFILE__', $pidFile), (New-Object Text.UTF8Encoding($false)))
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $rc = Invoke-Bounded 'powershell' @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $launcher) $o4 $e4 10
    $sw.Stop()
    $gtxt = if (Test-Path $o4) { [IO.File]::ReadAllText($o4) } else { '' }
    Chk ($rc -eq 0) '孙进程持有 stdout 时启动器正常返回 rc=0' ("rc=" + $rc)
    Chk ($sw.Elapsed.TotalSeconds -lt 20) '有界返回（不等孙进程 EOF）' ("elapsed=" + [int]$sw.Elapsed.TotalSeconds)
    Chk ($gtxt.Contains('launcher-done')) '启动器输出已落盘' $gtxt
    if (Test-Path $pidFile) {
      $gpid = [int](Get-Content -LiteralPath $pidFile -Raw).Trim()
      try { taskkill /PID $gpid /T /F | Out-Null } catch { }
      # F1（reviewer-21）：这里是「清理」的唯一判据——必须**断言进程真的没了**。
      # 原来写的是 `Chk ($true)`：把 taskkill 整行删掉仍 9/9 全绿，孙进程被留在后台。
      $gone = $false
      for ($k = 0; $k -lt 25; $k++) {
        if (-not (Get-Process -Id $gpid -ErrorAction SilentlyContinue)) { $gone = $true; break }
        Start-Sleep -Milliseconds 200
      }
      Chk $gone 'F1: 清理后孙进程真的不在了' ("gpid=" + $gpid)
    } else {
      Chk $false 'F1: 启动器没写出孙进程 pid 文件（无法验证清理）'
    }
  } finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }
  Write-Host ("BOUNDED_SELFTEST_PASS=" + $script:spass + " FAIL=" + $script:sfail)
  if ($script:sfail -gt 0) { return 1 }
  Write-Host 'BOUNDED_SELFTEST_OK'
  return 0
}

if ($Selftest) { exit (Invoke-Selftest) }
if ($FilePath -eq '') { Write-Host 'BOUNDED_RUN USAGE -FilePath 必填'; exit 2 }
if ($OutFile -eq '') { $OutFile = Join-Path $env:TEMP ('bounded-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.out.log') }
if ($ErrFile -eq '') { $ErrFile = $OutFile + '.err' }
if ($TimeoutSec -lt 1) { Write-Host 'BOUNDED_RUN USAGE -TimeoutSec 必须 ≥1'; exit 2 }
exit (Invoke-Bounded $FilePath $Arguments $OutFile $ErrFile $TimeoutSec)
