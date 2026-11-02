# tools/window/start-rust.ps1 — 窗口 Rust 私有栈（rust-root / 15xxx 端口）启动器，**仓库内事实源**。
#
# 背景（卡 openmir2-window-rust-starter-hips）：
#   `card_window.py` 的 alt profile 原先指向 `E:/tmp/conformance/start-rust.ps1`——该目录会被
#   外部清扫（同类事故已发生三次，见 tools/window/start-csharp.ps1 文件头）；且它的内层调用
#   `& powershell -NoProfile -ExecutionPolicy Bypass -File <starter>` 把 `-ExecutionPolicy`
#   带上命令行。本机 HIPS（火绒）对「WMI 派生链上的 powershell + -ExecutionPolicy」拒绝
#   （卡 openmir2-window-wmi-starter-fix：Create 回 0 有 pid 但内层脚本不执行）。本脚本把
#   启动面收进仓库，并改为 **PSExecutionPolicyPreference=Bypass 环境变量**下发进程级策略
#   （等价 `Get-ExecutionPolicy -Scope Process` = Bypass），命令行只留 `-NoProfile -File`
#   ——与 tools/window/start-stack-wmi.ps1 的修法同口径。
#
# 保留 WMI 派生（不得改回直连，见 card_window.py §1.2）：本脚本由
#   tools/window/start-stack-wmi.ps1 用 Win32_Process.Create 派生，父进程是 WmiPrvSE，
#   不继承 python↔powershell 的 stdout 管道 ⇒ stack 步不会因孙进程持有写端而 EOF 死锁。
#
# 用法：
#   powershell -NoProfile -File tools/window/start-rust.ps1
#   powershell ... -Plan      # 只打印将执行的命令/env，不启动、不需要部署就绪
#   powershell ... -Selftest  # 合成夹具自检（env 转发 / 退出码透传 / 命令行无 -ExecutionPolicy）
#
# 退出码：0 = starter 成功；非零 = starter 的退出码（透传，fail-closed）；
#         2 = 参数/缺件/自检红。**绝不**把未知退出码当成功。
param(
  [string]$ServerExe = '',
  [string]$Root = '',
  [string]$LogDir = '',
  [string]$StarterScript = '',
  [int]$WaitSeconds = 60,
  [switch]$Plan,
  [switch]$Selftest
)
$ErrorActionPreference = 'Stop'

$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Repo = (Resolve-Path (Join-Path $Here '..\..')).Path

# 部署根：MIR2_CONFORMANCE_ROOT 可覆盖（E:/tmp 会被外部清扫；路径不再写死）。
$Conf = [string]$env:MIR2_CONFORMANCE_ROOT
if ([string]::IsNullOrWhiteSpace($Conf)) { $Conf = 'E:\tmp\conformance' }
if ([string]::IsNullOrWhiteSpace($Root)) { $Root = Join-Path $Conf 'rust-root' }
if ([string]::IsNullOrWhiteSpace($LogDir)) { $LogDir = Join-Path $Conf 'rust-logs' }
if ([string]::IsNullOrWhiteSpace($ServerExe)) {
  $ServerExe = Join-Path $Repo 'mir2-rs\target\release\mir2-server.exe'
}
if ([string]::IsNullOrWhiteSpace($StarterScript)) {
  $StarterScript = Join-Path $Repo 'tools\m2-acceptance\start_mir2_server.ps1'
}

function Get-PowerShellExe {
  $ps = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
  if (Test-Path -LiteralPath $ps -PathType Leaf) { return $ps }
  return 'powershell'
}

function New-StarterArgv {
  # 命令行**绝不含 -ExecutionPolicy**（HIPS 拦截 WMI 派生链上的该开关）；
  # 进程级策略由 Invoke-Starter 用 PSExecutionPolicyPreference 环境变量下发。
  param([string]$Script, [string[]]$Extra)
  return @('-NoProfile', '-File', $Script) + @($Extra)
}

function Invoke-Starter {
  # 子进程继承 PSExecutionPolicyPreference ⇒ 进程级执行策略 = Bypass（无需命令行开关）。
  param([string]$Script, [string[]]$Extra)
  $psExe = Get-PowerShellExe
  $argv = New-StarterArgv -Script $Script -Extra $Extra
  $prev = $env:PSExecutionPolicyPreference
  $rc = $null
  try {
    $env:PSExecutionPolicyPreference = 'Bypass'
    & $psExe @argv
    $rc = $LASTEXITCODE
  } finally {
    if ($null -eq $prev) { Remove-Item Env:PSExecutionPolicyPreference -ErrorAction SilentlyContinue }
    else { $env:PSExecutionPolicyPreference = $prev }
  }
  if ($null -eq $rc) { return 1 }  # 防御：子进程没给出退出码 ⇒ 不冒充成功
  return [int]$rc
}

# ---- -Selftest：合成夹具（不启动真实栈/不触真实端口）----------------------------
# 牙 1（env 转发）：清掉调用方 env 后，子进程仍必须看到 PSExecutionPolicyPreference=Bypass。
#   删掉 Invoke-Starter 里的 `$env:PSExecutionPolicyPreference = 'Bypass'` ⇒ 本断言必红
#   （子进程看到空值 / Get-ExecutionPolicy -Scope Process = Undefined）。
# 牙 2（退出码透传）：合成 starter `exit 7` ⇒ 本脚本必须回 7（非零不吞）。
# 牙 3（命令行）：New-StarterArgv 不得含 -ExecutionPolicy（HIPS 拦截的就是它）；
#   把它加回命令行 ⇒ 本断言必红。
if ($Selftest) {
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('start-rust-selftest-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  $fails = New-Object System.Collections.Generic.List[string]
  function Write-TmpScript([string]$name, [string]$body) {
    $p = Join-Path $tmp $name
    [System.IO.File]::WriteAllText($p, $body, (New-Object System.Text.UTF8Encoding($false)))
    return $p
  }
  function Assert-Case([bool]$cond, [string]$name, [string]$detail) {
    if ($cond) { Write-Output ('start-rust selftest: PASS ' + $name) }
    else {
      Write-Output ('start-rust selftest: FAIL ' + $name + ' :: ' + $detail)
      $script:fails.Add($name)
    }
  }
  $savedPolicy = $env:PSExecutionPolicyPreference
  try {
    # 模拟「调用方没有该 env」的最坏情况，确保牙咬在**本脚本自己的下发**上。
    Remove-Item Env:PSExecutionPolicyPreference -ErrorAction SilentlyContinue

    $marker = Join-Path $tmp 'policy.txt'
    $probeBody = "`$p = [string]`$env:PSExecutionPolicyPreference`n" +
                 "`$s = [string](Get-ExecutionPolicy -Scope Process)`n" +
                 "Set-Content -LiteralPath '" + $marker + "' -Value ('policy=' + `$p + ';scope=' + `$s) -Encoding ascii`n" +
                 "exit 0"
    $probe = Write-TmpScript 'probe.ps1' $probeBody
    $rc = Invoke-Starter -Script $probe -Extra @()
    $text = ''
    if (Test-Path -LiteralPath $marker) { $text = [string](Get-Content -LiteralPath $marker -Raw) }
    Assert-Case ($rc -eq 0 -and $text -match 'policy=Bypass' -and $text -match 'scope=Bypass') `
      'positive: PSExecutionPolicyPreference=Bypass reaches child' `
      ('rc=' + $rc + ' marker=' + $text.Trim())

    $fail = Write-TmpScript 'rc7.ps1' 'exit 7'
    $rc7 = Invoke-Starter -Script $fail -Extra @()
    Assert-Case ($rc7 -eq 7) 'negative: starter exit code propagates (rc=7)' ('rc=' + $rc7)

    $argv = New-StarterArgv -Script $StarterScript -Extra @('-Root', $Root)
    $hasExecPolicy = @($argv | Where-Object { ([string]$_).ToLowerInvariant().StartsWith('-executionpolicy') }).Count -gt 0
    Assert-Case ((-not $hasExecPolicy) -and ($argv.Count -ge 3) -and ($argv[0] -eq '-NoProfile') -and ($argv[1] -eq '-File')) `
      'negative: argv has no -ExecutionPolicy' ('argv=' + ($argv -join ' '))

    $defaultOk = $StarterScript.StartsWith($Repo, [System.StringComparison]::OrdinalIgnoreCase)
    Assert-Case $defaultOk 'positive: default starter is repo-internal' ('starter=' + $StarterScript)
  } finally {
    if ($null -eq $savedPolicy) { Remove-Item Env:PSExecutionPolicyPreference -ErrorAction SilentlyContinue }
    else { $env:PSExecutionPolicyPreference = $savedPolicy }
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }
  if ($fails.Count -gt 0) {
    Write-Output ('start-rust selftest: RED (' + $fails.Count + ' failed)')
    exit 1
  }
  Write-Output 'start-rust selftest: GREEN (2 positive + 2 negative)'
  exit 0
}

# ---- 主路径：委托 tools/m2-acceptance/start_mir2_server.ps1（单二进制六角色）--------
$portsProfile = Join-Path $Root 'alt-port-profile.json'
$extra = @('-Root', $Root, '-LogDir', $LogDir, '-PortsProfile', $portsProfile,
           '-Seed', '42', '-ServerExe', $ServerExe, '-WaitSeconds', [string]$WaitSeconds)
$argv = New-StarterArgv -Script $StarterScript -Extra $extra

if ($Plan) {
  Write-Output ("REPO {0}" -f $Repo)
  Write-Output ("STARTER {0}" -f $StarterScript)
  Write-Output ("SERVER_EXE {0}" -f $ServerExe)
  Write-Output ("PORTS_PROFILE {0}" -f $portsProfile)
  Write-Output 'ENV PSExecutionPolicyPreference=Bypass'
  Write-Output ("PLAN " + (Get-PowerShellExe) + ' ' + ($argv -join ' '))
  exit 0
}

if (-not (Test-Path -LiteralPath $StarterScript -PathType Leaf)) {
  Write-Output ("RED: 缺 starter 脚本：{0}" -f $StarterScript)
  exit 2
}
if (-not (Test-Path -LiteralPath $ServerExe -PathType Leaf)) {
  Write-Output ("RED: 缺 ServerExe：{0}（先构建 mir2-server，或 -ServerExe 指定）" -f $ServerExe)
  exit 2
}

$rc = Invoke-Starter -Script $StarterScript -Extra $extra
exit $rc
