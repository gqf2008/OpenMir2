#requires -Version 5
<#
.SYNOPSIS
  run_scenario.ps1 — 2026-10-23 真机窗口 G1 / G2a / G3a / G5a 的可执行场景包（tier=L 夹具）。

.DESCRIPTION
  把 `docs/window-round2-runbook-20261023.md`（G1/G2a 与受控登录）与
  `docs/window-run-sheet-20261022.md`（G3a/G5a、§A4 优雅停机、§B5/§B7 摆位）里
  **已经实跑过**的命令序列固化成一条命令，并给出期望与证据路径：

    G1      登录/入图基线：15 帧 + 登录尾（654 → 657×2 → 第二 42 → 100×4）
    G2A-R1  B5 R1：火焰戒指(354) ⇒ 210
    G2A-R2  B5 R2：无戒指 ⇒ 212（粘滞）
    G2A-R3  B5 R3：MagicId=3 + 木剑 ⇒ 621 + 211
    G3A     (605,578) 战斗 / (291,666) GuardList / (311,666) 反向点（三个 subrun）
    G5A     玩家在世界中 ⇒ Ctrl+Break 优雅停 ⇒ before/after 关键值

  **默认是 dry-run（`-Go` 才执行）**；写测试库还需要 `-Yes`。
  本脚本不新增任何编排逻辑：起栈/抓包/客户端/导出/判据全部调用既有工具——
    tools/loginfixture/prepare_safe_login.py
    mir2-rs/tests/parity/m3/start-csharp-hook.ps1
    tools/m2-acceptance/start_rust_stack.ps1
    tools/client/mir_flow.ps1
    tools/m2-acceptance/export_s2c.ps1
    mir2-rs/tests/parity/m3/window-20261023/judge_login.py
    tools/m2-acceptance/s2c_diff.py / check_db_paths.ps1
    mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_sm_hit.py 等
    mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/send_ctrlc.ps1

  场景与期望的唯一数据源 = 同目录 `scenarios.json`（judge_login.py 也读它）。

.EXAMPLE
  # 只看计划（不执行、不占端口、不写库）
  powershell -NoProfile -ExecutionPolicy Bypass -File run_scenario.ps1 -Scenario G1 -Side rust
.EXAMPLE
  # 开窗前（一条命令）：九端口/7600/3306/镜像 provenance/测试库克隆
  powershell -NoProfile -ExecutionPolicy Bypass -File run_scenario.ps1 -Scenario G1 -Side rust -Phase preflight -Go
.EXAMPLE
  # 整段执行（写库需 -Yes）
  powershell -NoProfile -ExecutionPolicy Bypass -File run_scenario.ps1 -Scenario G1 -Side rust -Go -Yes
.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File run_scenario.ps1 -Selftest
#>
param(
  [ValidateSet('G1', 'G2A-R1', 'G2A-R2', 'G2A-R3', 'G3A', 'G5A')]
  [string]$Scenario = 'G1',
  [ValidateSet('csharp', 'rust')]
  [string]$Side = 'rust',
  [ValidateSet('all', 'preflight', 'paircheck', 'pin', 'stack', 'capture', 'flow', 'export', 'judge',
               'snap-before', 'stop-graceful', 'snap-after', 'close')]
  [string]$Phase = 'all',
  [ValidateSet('attack', 'guardlist', 'reverse')]
  [string]$SubRun = 'attack',
  [string]$Win = 'E:/tmp/window/20261023',
  [string]$Repo = '',
  [string]$Char = '',
  [string]$Password = '',
  [string]$ClientDir = 'D:\MirClient-run',
  [string]$MirServer = 'E:\MirServer',
  [string]$RustBinDir = 'E:/tmp/m2svr-window-target/debug',
  [string]$Mirror = 'E:/tmp/m2svr-window-origin',
  [string]$Mysql = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe',
  [string]$MysqlUser = 'root',
  [string]$MysqlPassword = '',
  [string]$GoldenExportExe = '',
  [int]$Seed = 42,
  [int]$AttackClicks = 0,
  [switch]$E1b,
  [switch]$Go,
  [switch]$Yes,
  [switch]$List,
  [switch]$Selftest
)

$ErrorActionPreference = 'Stop'
$Pkg = $PSScriptRoot
if ([string]::IsNullOrEmpty($Repo)) {
  $Repo = (Resolve-Path (Join-Path $Pkg '..\..\..\..\..')).Path
}
$Win = $Win.TrimEnd('/', '\')

# ---------------------------------------------------------------------------
# 通用
# ---------------------------------------------------------------------------
function Format-Argv([string[]]$Argv) {
  ($Argv | ForEach-Object {
      if ($_ -match '[\s"]') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ }
    }) -join ' '
}

function Expand-Tokens([string]$Text, [string]$Sub = '') {
  $sideDir = $Side
  $t = $Text
  $t = $t.Replace('{WIN}', $Win)
  $t = $t.Replace('{REPO}', $Repo)
  $t = $t.Replace('{side}', $sideDir.ToLower())
  $t = $t.Replace('{SIDE}', $sideDir.ToUpper())
  $t = $t.Replace('{sub}', $Sub)
  $t = $t.Replace('{MYSQL}', $Mysql)
  $t = $t.Replace('{MIRSERVER}', $MirServer)
  $t = $t.Replace('{RUSTBIN}', $RustBinDir)
  $t = $t.Replace('{CLIENTDIR}', $ClientDir)
  return $t
}

function Get-Scenarios {
  $p = Join-Path $Pkg 'scenarios.json'
  if (-not (Test-Path -LiteralPath $p)) { throw "RED: 缺 scenarios.json：$p" }
  return (Get-Content -LiteralPath $p -Raw -Encoding UTF8 | ConvertFrom-Json)
}

function Get-Scenario([string]$Name) {
  $all = Get-Scenarios
  $sc = $all.scenarios.$Name
  if ($null -eq $sc) { throw "RED: scenarios.json 里没有 $Name" }
  return $sc
}

function Get-ScenarioChar($Sc) {
  if (-not [string]::IsNullOrEmpty($Char)) { return $Char }
  return [string]$Sc.char
}

function Get-ScenarioPassword([string]$Chr) {
  if (-not [string]::IsNullOrEmpty($Password)) { return $Password }
  return $Chr   # runbook §1.9 坑 7：账号 == 密码
}

function Get-SideDir { return (Join-Path $Win $Side) }

function Get-RunDir([string]$Name) {
  # G1 用 <WIN>/<side>（与 runbook 证据路径一致）；G2a 每轮独立目录，避免 R1/R2/R3 互相覆盖 capture。
  $sideDir = Get-SideDir
  if ($Name -eq 'G1') { return $sideDir }
  return (Join-Path $sideDir $Name.ToLower())
}

function Ensure-Dir([string]$Path) {
  if ($Go) { New-Item -ItemType Directory -Force -Path $Path | Out-Null }
}

function Invoke-Step {
  param([string[]]$Argv, [string]$Label = '')
  Write-Host ("CMD  " + (Format-Argv $Argv))
  if (-not $Go) { return $false }
  $exe = $Argv[0]
  $rest = @()
  if ($Argv.Count -gt 1) { $rest = $Argv[1..($Argv.Count - 1)] }
  & $exe @rest
  $rc = $LASTEXITCODE
  if ($null -ne $rc -and $rc -ne 0) {
    throw ("RED: {0} rc={1}" -f ($(if ($Label) { $Label } else { $exe })), $rc)
  }
  return $true
}

function Invoke-Ps1 {
  param([string]$Script, [string[]]$ScriptArgs, [string]$Label = '')
  $argv = @('powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $Script) + $ScriptArgs
  Invoke-Step -Argv $argv -Label $(if ($Label) { $Label } else { $Script }) | Out-Null
}

function Get-ListeningPorts {
  try {
    $ip = [System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties()
    return @($ip.GetActiveTcpListeners() | ForEach-Object { $_.Port } | Sort-Object -Unique)
  } catch {
    return @()
  }
}

function Assert-ListenState([int[]]$Down, [int[]]$Up) {
  $ports = Get-ListeningPorts
  $bad = @()
  foreach ($p in $Down) { if ($ports -contains $p) { $bad += "port $p 应为 DOWN 但在听" } }
  foreach ($p in $Up) { if (-not ($ports -contains $p)) { $bad += "port $p 应为 UP 但 DOWN" } }
  if ($bad.Count -gt 0) { throw ("RED: " + ($bad -join '; ')) }
  Write-Output ("LISTEN " + (($ports | Where-Object { $Down + $Up -contains $_ }) -join ','))
}

# ---------------------------------------------------------------------------
# 测试库守卫（所有 SQL 走这里）
# ---------------------------------------------------------------------------
function Get-SqlGuardErrors {
  param([string]$Sql, [string]$Name = 'sql')
  $errs = @()
  $s = [regex]::Replace($Sql, '(?m)--.*$', '')
  $s = [regex]::Replace($s, '(?s)/\*.*?\*/', '')
  if ($s -match '(?i)\b(mir2_db|mir2_account|mir2_data)\b') {
    $errs += "${Name}: 引用了生产库名（只准 mir2_test_*）"
  }
  if ($s -match '(?i)\b(create|drop|alter|truncate|grant|revoke|rename)\b') {
    $errs += "${Name}: 含 DDL/权限语句"
  }
  if ($s -match '(?i)\buse\s+') {
    $errs += "${Name}: 含 USE（可能把后续语句指到生产库）"
  }
  if ($s -match '(?i)\b(into\s+outfile|load\s+data)\b') {
    $errs += "${Name}: 含文件读写语句"
  }
  if ($s -notmatch '(?i)\bmir2_test_(db|account|data)\b') {
    $errs += "${Name}: 没有任何 mir2_test_* 目标（无法证明只写测试库）"
  }
  return $errs
}

function Invoke-SqlFile([string]$RelPath) {
  $path = Join-Path $Pkg $RelPath
  if (-not (Test-Path -LiteralPath $path)) { throw "RED: 缺 SQL 文件 $path" }
  $sql = Get-Content -LiteralPath $path -Raw -Encoding UTF8
  $errs = @(Get-SqlGuardErrors -Sql $sql -Name $RelPath)
  if ($errs.Count -gt 0) { throw ("RED: SQL 守卫拒绝`n  " + ($errs -join "`n  ")) }
  Write-Host ("SQL  " + $path + "  (guard OK, db=mir2_test_db)")
  if (-not $Go) { return $false }
  if (-not $Yes) {
    Write-Host "SKIP SQL 写入：需要 -Yes（dry-run 不写库）"
    return $false
  }
  $argv = @($Mysql, '-N', '-B', '-h', '127.0.0.1', '-P', '3306', '-u', $MysqlUser,
            ("--password=" + $MysqlPassword), '--default-character-set=utf8mb4',
            'mir2_test_db')
  Write-Host ("CMD  " + (Format-Argv $argv) + " < " + $path)
  $out = Get-Content -LiteralPath $path -Raw -Encoding UTF8 | & $Mysql @($argv[1..($argv.Count - 1)])
  $out | ForEach-Object { Write-Host ("SQLOUT " + $_) }
  if ($LASTEXITCODE -ne 0) { throw "RED: mysql 应用 $RelPath rc=$LASTEXITCODE" }
  return $true
}

# prepare_safe_login 强制要求 --level/--hp/--max-hp 三者齐全（缺任一 rc=2）。
# scenarios.json 只钉真正会漂移的项（G1 的血量、各轮 Level）；其余用下面的夹具常量补齐，
# 保证六个场景都能过 pin（两侧同值 ⇒ 不引入侧间漂移）。
$script:PinDefaultLevel = 1
$script:PinDefaultHp = 19
$script:PinDefaultMaxHp = 19

function Get-PinArgs($Sc, [string]$Chr) {
  $level = if ($null -ne $Sc.level) { [string]$Sc.level } else { [string]$script:PinDefaultLevel }
  $hp = if ($null -ne $Sc.hp) { [string]$Sc.hp } else { [string]$script:PinDefaultHp }
  $maxHp = if ($null -ne $Sc.max_hp) { [string]$Sc.max_hp } else { [string]$script:PinDefaultMaxHp }
  $a = @('python', '-I', '-B',
         (Join-Path $Repo 'tools\loginfixture\prepare_safe_login.py'),
         '--db', 'mir2_test_db', '--char', $Chr,
         '--map', [string]$Sc.point.map,
         '--x', [string]$Sc.point.x,
         '--y', [string]$Sc.point.y,
         '--level', $level, '--hp', $hp, '--max-hp', $maxHp,
         '--mysql', $Mysql)
  if ($Yes) { $a += '--yes-apply' }
  return $a
}

function Assert-PinArgs([string[]]$Argv) {
  $errs = @()
  $i = [array]::IndexOf($Argv, '--db')
  if ($i -lt 0 -or $Argv[$i + 1] -notmatch '^mir2_test_.+') {
    $errs += "prepare_safe_login --db 必须是 mir2_test_*"
  }
  # fail-closed：上游缺任一即 rc=2，这里提前拦（避免整轮 PREREQ 才发现）。
  foreach ($flag in '--level', '--hp', '--max-hp') {
    $j = [array]::IndexOf($Argv, $flag)
    if ($j -lt 0 -or $j + 1 -ge $Argv.Count) {
      $errs += ("prepare_safe_login 必须给 " + $flag + "（缺任一 rc=2）")
    }
  }
  $ih = [array]::IndexOf($Argv, '--hp')
  $im = [array]::IndexOf($Argv, '--max-hp')
  if ($ih -ge 0 -and $im -ge 0 -and $ih + 1 -lt $Argv.Count -and $im + 1 -lt $Argv.Count) {
    if ([int]$Argv[$ih + 1] -gt [int]$Argv[$im + 1]) { $errs += "--hp 不得大于 --max-hp" }
  }
  return $errs
}

# ---------------------------------------------------------------------------
# G5a 关键值
# ---------------------------------------------------------------------------
function Get-G5KeyValues([string]$Chr) {
  $q = "SELECT CONCAT('CHR|',MapName,'|',CX,'|',CY,'|',Level,'|',LASTUPDATE) " +
       "FROM mir2_test_db.characters WHERE ChrName='$Chr'; " +
       "SELECT CONCAT('ABL|',a.Hp,'|',a.MaxHP,'|',a.Mp,'|',a.MAxMP) " +
       "FROM mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id " +
       "WHERE c.ChrName='$Chr';"
  if (-not $Go) { Write-Output ("CMD  mysql -N -B -e " + $q); return @() }
  $argv = @('-N', '-B', '-h', '127.0.0.1', '-P', '3306', '-u', $MysqlUser,
            ("--password=" + $MysqlPassword), '--default-character-set=utf8mb4', '-e', $q)
  $out = & $Mysql @argv
  if ($LASTEXITCODE -ne 0) { throw "RED: mysql 读关键值 rc=$LASTEXITCODE" }
  return @($out | Where-Object { $_ -match '^(CHR|ABL)\|' })
}

function Compare-G5KeyValues {
  param([string[]]$Before, [string[]]$After, [switch]$IgnoreLastUpdate)
  $errs = @()
  if (@($Before).Count -lt 2 -or @($After).Count -lt 2) {
    return @('关键值行数不足（缺 before/after；shutdown 前必须玩家在世界中）')
  }
  $b = @{}; foreach ($l in $Before) { $p = $l -split '\|'; $b[$p[0]] = $p }
  $a = @{}; foreach ($l in $After) { $p = $l -split '\|'; $a[$p[0]] = $p }
  foreach ($k in 'CHR', 'ABL') {
    if (-not $b.ContainsKey($k)) { $errs += "before 缺 $k 行" }
    if (-not $a.ContainsKey($k)) { $errs += "after 缺 $k 行" }
  }
  if ($errs.Count -eq 0) {
    # CHR|MapName|CX|CY|Level|LASTUPDATE：地图/坐标/等级不得漂移
    foreach ($idx in 1, 2, 3, 4) {
      if ($b['CHR'][$idx] -ne $a['CHR'][$idx]) {
        $errs += ("CHR 关键值漂移 idx={0}: before={1} after={2}" -f $idx, $b['CHR'][$idx], $a['CHR'][$idx])
      }
    }
    # ABL|Hp|MaxHP|Mp|MAxMP：MaxHP/MAxMP 不得漂移；Hp/Mp 允许寄存器回复
    if ($b['ABL'][2] -ne $a['ABL'][2]) {
      $errs += ("MaxHP 漂移: before={0} after={1}" -f $b['ABL'][2], $a['ABL'][2])
    }
    if ($b['ABL'][4] -ne $a['ABL'][4]) {
      $errs += ("MAxMP 漂移: before={0} after={1}" -f $b['ABL'][4], $a['ABL'][4])
    }
    if (-not $IgnoreLastUpdate) {
      $bLU = $b['CHR'][5]; $aLU = $a['CHR'][5]
      if ($bLU -eq $aLU) {
        Write-Host "G5_WRITE_OBSERVED=false（LASTUPDATE 未推进：本轮 shutdown 未观察到落库）"
      } else {
        Write-Host "G5_WRITE_OBSERVED=true（LASTUPDATE $bLU -> $aLU）"
      }
    }
  }
  return $errs
}

# ---------------------------------------------------------------------------
# 阶段实现
# ---------------------------------------------------------------------------
function Invoke-Preflight {
  $out = Join-Path $Win 'preflight\preflight.json'
  $targetRoot = Split-Path -Parent $RustBinDir
  $argv = @('python', '-I', '-B', (Join-Path $Pkg 'preflight.py'),
            '--win', $Win, '--mirror', $Mirror,
            '--target-dir', $targetRoot,
            '--provenance', (Join-Path $targetRoot 'window-provenance.json'),
            '--mysql', $Mysql, '--db-user', $MysqlUser,
            ('--db-password=' + $MysqlPassword), '--out', $out)
  Invoke-Step -Argv $argv -Label 'preflight' | Out-Null
  Write-Output ("EVIDENCE " + $out)
}

function Invoke-PairSelfCheck {
  # 复用既有两个 run_*_pair.ps1 的**离线自检**（不碰端口/不起栈）：
  # 它们覆盖 bot 驱动的攻击/背包序列；本包的 G3a 是单客户端（runbook §B7），
  # 不把它们塞进场景流，只把它们的 fail-closed 自检当作开窗前离线门。
  $m3 = Join-Path $Repo 'mir2-rs/tests/parity/m3'
  foreach ($name in 'run_attack_pair.ps1', 'run_bag_equip_pair.ps1') {
    $script = Join-Path $m3 $name
    if (-not (Test-Path -LiteralPath $script)) { throw "RED: 缺 $script" }
    $args1 = @('-SelfCheckNoStack')
    Invoke-Ps1 -Script $script -ScriptArgs $args1 -Label "$name -SelfCheckNoStack" | Out-Null
  }
}

function Invoke-Pin($Sc, [string]$Chr, [string]$Sub) {
  $argv = Get-PinArgs $Sc $Chr
  $errs = @(Assert-PinArgs $argv)
  if ($errs.Count -gt 0) { throw ("RED: pin 参数非法`n  " + ($errs -join "`n  ")) }
  Invoke-Step -Argv $argv -Label 'prepare_safe_login' | Out-Null
  foreach ($rel in @($Sc.sql)) {
    if (-not [string]::IsNullOrEmpty($rel)) { Invoke-SqlFile $rel | Out-Null }
  }
}

function Invoke-Stack {
  Ensure-Dir (Join-Path (Get-SideDir) 'stack')
  if ($Side -eq 'csharp') {
    $pack = Join-Path $Win 'csharp\pack'
    $repoDir = Join-Path $Win 'csharp\repo'
    $shadow = Join-Path $Win 'csharp\GameSvr-shadow'
    if ($Go -and (-not (Test-Path -LiteralPath $pack) -or -not (Test-Path -LiteralPath $repoDir))) {
      throw "RED: 缺 $pack 或 $repoDir（先按 runbook §1.2 准备 pack/repo 副本）"
    }
    if (Test-Path -LiteralPath $shadow) {
      if ($Go) {
        $bak = $shadow + '.bak-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
        Move-Item -LiteralPath $shadow -Destination $bak
        Write-Output "ARCHIVED_OLD_SHADOW $bak"
      } else {
        Write-Output "DRY  Would archive old shadow: $shadow"
      }
    }
    $args1 = @('-Repo', $repoDir, '-Pack', $pack, '-Shadow', $shadow,
               '-LogDir', (Join-Path $Win 'csharp\stack'), '-Seed', [string]$Seed)
    Invoke-Ps1 -Script (Join-Path $Repo 'mir2-rs\tests\parity\m3\start-csharp-hook.ps1') `
      -ScriptArgs $args1 -Label 'start-csharp-hook' | Out-Null
  } else {
    $args1 = @('-RustBinDir', $RustBinDir, '-Seed', [string]$Seed,
               '-LogDir', (Join-Path $Win 'rust\stack'), '-DbName', 'mir2_test',
               '-DbHost', '127.0.0.1')
    Invoke-Ps1 -Script (Join-Path $Repo 'tools\m2-acceptance\start_rust_stack.ps1') `
      -ScriptArgs $args1 -Label 'start_rust_stack' | Out-Null
  }
}

function Stop-GateByPidFile([string]$PidFile, [string]$Name) {
  if (-not $Go) {
    Write-Host "DRY  Would stop $Name by $PidFile（subrun/重跑前释放 7200）"
    return
  }
  if (-not (Test-Path -LiteralPath $PidFile)) {
    Write-Host "SKIP 停 $Name：缺 $PidFile"
    return
  }
  $pf = Get-Content -LiteralPath $PidFile -Raw | ConvertFrom-Json
  foreach ($e in @($pf)) {
    if ($e.name -eq $Name -and $e.pid) {
      Stop-Process -Id ([int]$e.pid) -Force -ErrorAction SilentlyContinue
      Write-Output "STOPPED $Name pid=$($e.pid)"
    }
  }
  Start-Sleep -Milliseconds 800
}

function Write-ShadowGateConf([string]$SourceConf, [string]$DestConf) {
  if (-not (Test-Path -LiteralPath $SourceConf)) { throw "RED: 缺 gate conf $SourceConf" }
  Ensure-Dir (Split-Path -Parent $DestConf)
  if (-not $Go) { Write-Output "DRY  cp936 round-trip $SourceConf -> $DestConf (Count=1 GatePort1=7600)"; return }
  $enc = [Text.Encoding]::GetEncoding(936)
  Copy-Item -LiteralPath $SourceConf -Destination $DestConf -Force
  $t = [IO.File]::ReadAllText($DestConf, $enc)
  $t = $t -replace '(?m)(?<=^Count=)\d+(?=\r?$)', '1' `
         -replace '(?m)(?<=^GatePort1=)\d+(?=\r?$)', '7600'
  [IO.File]::WriteAllText($DestConf, $t, $enc)
  $c = Select-String -Path $DestConf -Pattern '^Count=|^GatePort1='
  Write-Output ("SHADOW_CONF " + (($c | ForEach-Object { $_.Line.Trim() }) -join ' '))
}

function Start-ShadowGate {
  $sideDir = Get-SideDir
  $shadowDir = Join-Path $sideDir 'shadow'
  Ensure-Dir $shadowDir
  if ($Side -eq 'csharp') {
    Stop-GateByPidFile (Join-Path $Win 'csharp\stack\csharp-stack.pid.json') 'RunGate'
    $conf = Join-Path $shadowDir 'game-gate.conf'
    Write-ShadowGateConf (Join-Path $Win 'csharp\pack\RunGate\config.conf') $conf
    $runGateDir = Join-Path $shadowDir 'run-gate'
    if ($Go) {
      Remove-Item -Recurse -Force $runGateDir -ErrorAction SilentlyContinue
      Copy-Item -Recurse -Force (Join-Path $Win 'csharp\pack\RunGate') $runGateDir
      Copy-Item -Force $conf (Join-Path $runGateDir 'config.conf')
      $g = Start-Process -FilePath (Join-Path $runGateDir 'GameGate.exe') `
        -WorkingDirectory $runGateDir -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $shadowDir 'csharp-gate.out.log') `
        -RedirectStandardError (Join-Path $shadowDir 'csharp-gate.err.log') -PassThru
      $g.Id | Set-Content (Join-Path $shadowDir 'csharp-gate.pid')
      Start-Sleep -Milliseconds 1500
    } else {
      Write-Output "DRY  Start-Process $runGateDir\GameGate.exe (7600)"
    }
  } else {
    Stop-GateByPidFile (Join-Path $Win 'rust\stack\rust-stack.pid.json') 'game-gate'
    $conf = Join-Path $shadowDir 'game-gate.conf'
    Write-ShadowGateConf (Join-Path $MirServer 'RunGate\config.conf') $conf
    if ($Go) {
      $g = Start-Process -FilePath (Join-Path $RustBinDir 'game-gate.exe') `
        -ArgumentList @($conf) -WorkingDirectory $shadowDir -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $shadowDir 'rust-gate.out.log') `
        -RedirectStandardError (Join-Path $shadowDir 'rust-gate.err.log') -PassThru
      $g.Id | Set-Content (Join-Path $shadowDir 'rust-gate.pid')
      Start-Sleep -Milliseconds 1500
    } else {
      Write-Output "DRY  Start-Process $RustBinDir\game-gate.exe $conf (7600)"
    }
  }
  if ($Go) { Assert-ListenState -Down @(7200) -Up @(7600) | Out-Null }
}

function Start-Proxy([string]$RunDir) {
  $sideDir = Get-SideDir
  $proxyDir = Join-Path $RunDir 'proxy'
  Ensure-Dir $proxyDir
  $argv = @('python', '-I', '-B', (Join-Path $Repo 'tools\capture\mir2_proxy.py'),
            '--out', $proxyDir, '--map', '7200=127.0.0.1:7600')
  Write-Output ("CMD  " + (Format-Argv $argv))
  if (-not $Go) { return }
  $proxy = Start-Process -FilePath 'python' -ArgumentList @(
    '-I', '-B', (Join-Path $Repo 'tools\capture\mir2_proxy.py'),
    '--out', $proxyDir, '--map', '7200=127.0.0.1:7600') `
    -WorkingDirectory $Repo -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $sideDir 'proxy.log') `
    -RedirectStandardError (Join-Path $sideDir 'proxy.err.log') -PassThru
  $proxy.Id | Set-Content (Join-Path $sideDir 'proxy.pid')
  Start-Sleep -Seconds 1
  $sess = Join-Path $proxyDir 'session.json'
  if (-not (Test-Path -LiteralPath $sess)) { throw "RED: 代理未产出 session.json（7200 未释放？）" }
  $j = Get-Content -LiteralPath $sess -Raw | ConvertFrom-Json
  if ($j.map.'7200' -ne '127.0.0.1:7600') { throw "RED: 代理 map 不是 7200->7600：$($j.map.'7200')" }
  Write-Output "PROXY_READY pid=$($proxy.Id) map=7200->127.0.0.1:7600"
}

function Stop-CaptureProcesses {
  # G3a 每个 subrun 都要重新抓包：先释放 7200（代理）与 7600（影子 gate），
  # 否则第二次 `Start-Proxy` 直接 `address in use`（runbook §1.9 坑 4）。
  $sideDir = Get-SideDir
  foreach ($spec in @(@((Join-Path $sideDir 'proxy.pid'), 'proxy'),
                       @((Join-Path $sideDir 'shadow\csharp-gate.pid'), 'csharp-gate'),
                       @((Join-Path $sideDir 'shadow\rust-gate.pid'), 'rust-gate'))) {
    $f = $spec[0]
    if (-not (Test-Path -LiteralPath $f)) { continue }
    if ($Go) {
      $p = [int](Get-Content -LiteralPath $f -Raw).Trim()
      Stop-Process -Id $p -Force -ErrorAction SilentlyContinue
      Remove-Item -LiteralPath $f -Force -ErrorAction SilentlyContinue
      Write-Output ("STOPPED {0} pid={1}（subrun 间复位）" -f $spec[1], $p)
      Start-Sleep -Milliseconds 500
    } else {
      Write-Output ("DRY  stop {0} by {1}" -f $spec[1], $f)
    }
  }
}

function Invoke-Capture([string]$RunDir) {
  Stop-CaptureProcesses
  Start-ShadowGate
  Start-Proxy $RunDir
}

function Get-FlowArgs($Sc, [string]$Chr, [string]$FlowDir) {
  $pw = Get-ScenarioPassword $Chr
  $a = @('powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass',
         '-File', (Join-Path $Repo 'tools\client\mir_flow.ps1'),
         '-Account', $Chr, '-Password', $pw, '-RunDir', $ClientDir, '-OutDir', $FlowDir)
  $f = $Sc.flow
  if ($f.skip_talk) { $a += '-SkipTalk' }
  if ($null -ne $f.walk_clicks) { $a += @('-WalkClicks', [string]$f.walk_clicks) }
  if ($f.skip_logout) { $a += '-SkipLogout' }
  if ($f.skip_reenter) { $a += '-SkipReenter' }
  if ($f.leave_client_running) { $a += '-LeaveClientRunning' }
  $ac = 0
  if ($null -ne $f.attack_clicks) { $ac = [int]$f.attack_clicks }
  if ($AttackClicks -gt 0) { $ac = $AttackClicks }
  if ($ac -gt 0) { $a += @('-AttackClicks', [string]$ac) }
  return $a
}

function Invoke-Flow($Sc, [string]$Chr, [string]$FlowDir) {
  Ensure-Dir $FlowDir
  $argv = Get-FlowArgs $Sc $Chr $FlowDir
  Invoke-Step -Argv $argv -Label 'mir_flow' | Out-Null
  if ($Go) {
    $stages = Join-Path $FlowDir 'stages.ndjson'
    if (-not (Test-Path -LiteralPath $stages)) { throw "RED: mir_flow 未产出 $stages" }
    if (-not (Select-String -Path $stages -Pattern 'FLOW_OK' -Quiet)) {
      throw "RED: mir_flow 没有 FLOW_OK（$stages）"
    }
    Write-Output "FLOW_OK $stages"
  }
}

function Invoke-Export([string]$ProxyDir, [string]$S2cDir) {
  Ensure-Dir $S2cDir
  $args1 = @('-ProxyDir', $ProxyDir, '-OutDir', $S2cDir)
  if (-not [string]::IsNullOrEmpty($GoldenExportExe)) {
    $args1 += @('-GoldenExportExe', $GoldenExportExe)
  }
  Invoke-Ps1 -Script (Join-Path $Repo 'tools\m2-acceptance\export_s2c.ps1') `
    -ScriptArgs $args1 -Label 'export_s2c' | Out-Null
  Write-Output ("EVIDENCE " + (Join-Path $S2cDir 'capture.jsonl'))
}

function Get-GoldenCapturePath([string]$Name) {
  # 同轮对比：G2a 的 C# capture 在 <WIN>/csharp/g2a-rN/s2c（不能被 R1/R2/R3 覆盖）。
  $peerSide = Join-Path $Win 'csharp'
  if ($Name -ne 'G1') { $peerSide = Join-Path $peerSide $Name.ToLower() }
  return (Join-Path $peerSide 's2c\capture.jsonl')
}

function Get-LoginJudgeArgs([string]$Name, [string]$S2cDir, [string]$JudgeDir) {
  $capture = Join-Path $S2cDir 'capture.jsonl'
  $golden = Get-GoldenCapturePath $Name
  $wl = Join-Path $JudgeDir 's2c-whitelist.jsonl'
  $out = Join-Path $JudgeDir ("{0}-{1}.json" -f $Name.ToLower(), $Side)
  $argv = @('python', '-I', '-B', (Join-Path $Pkg 'judge_login.py'),
            '--scenario', $Name, '--side', $Side, '--capture', $capture,
            '--whitelist', $wl, '--out', $out)
  if ($Side -eq 'rust') {
    # fail-closed：-Go 且缺本轮 C# golden ⇒ PREREQ_RED，绝不降级为 --no-j5。
    if ($Go -and -not (Test-Path -LiteralPath $golden)) { return $null }
    $argv += @('--golden', $golden)
  } else {
    $argv += '--no-j5'
  }
  return ,$argv
}

function Invoke-LoginJudge($Sc, [string]$Name, [string]$S2cDir, [string]$JudgeDir) {
  Ensure-Dir $JudgeDir
  $argv = Get-LoginJudgeArgs $Name $S2cDir $JudgeDir
  if ($null -eq $argv) {
    Write-Output ("PREREQ_RED: 缺本轮 C# capture（{0}）；J5 不得降级为 --no-j5（README §J5）" -f (Get-GoldenCapturePath $Name))
    exit 2
  }
  $wl = Join-Path $JudgeDir 's2c-whitelist.jsonl'
  if ($Go) { Copy-Item -LiteralPath (Join-Path $Pkg 's2c-whitelist-d9.jsonl') -Destination $wl -Force }
  Invoke-Step -Argv $argv -Label 'judge_login' | Out-Null
  $out = Join-Path $JudgeDir ("{0}-{1}.json" -f $Name.ToLower(), $Side)
  Write-Output ("EVIDENCE " + $out)
}

function Invoke-CombatJudge($SubSc, [string]$Sub, [string]$S2cDir, [string]$JudgeDir) {
  $capture = Join-Path $S2cDir 'capture.jsonl'
  $chk = Join-Path $Repo 'mir2-rs\tests\parity\world\client-e2e-enter-gaps-20261013'
  foreach ($c in @($SubSc.judge.checks)) {
    if ($null -eq $c) { continue }
    $argv = @('python', '-I', '-B', (Join-Path $chk $c.tool), '--capture', $capture) +
            @($c.args | ForEach-Object { [string]$_ })
    Invoke-Step -Argv $argv -Label $c.tool | Out-Null
  }
  if ($null -ne $SubSc.judge.s2c_diff_from) {
    $rustCap = Join-Path $Win ("rust\g3a-{0}\s2c\capture.jsonl" -f $Sub)
    $csCap = Join-Path $Win ("csharp\g3a-{0}\s2c\capture.jsonl" -f $Sub)
    if ((Test-Path -LiteralPath $rustCap) -and (Test-Path -LiteralPath $csCap)) {
      Ensure-Dir $JudgeDir
      $out = Join-Path $JudgeDir ("g3a-{0}.json" -f $Sub)
      $argv = @('python', '-I', '-B', (Join-Path $Repo 'tools\m2-acceptance\s2c_diff.py'),
                '--rust', $rustCap, '--golden', $csCap,
                '--start-ident', '10', '--upto-ident', '41',
                '--from-x', [string]$SubSc.judge.s2c_diff_from[0],
                '--from-y', [string]$SubSc.judge.s2c_diff_from[1],
                '--out', $out)
      Invoke-Step -Argv $argv -Label 's2c_diff' | Out-Null
      Write-Output ("EVIDENCE " + $out)
    } else {
      Write-Output ("SKIP s2c_diff：缺两侧 capture（$rustCap / $csCap）")
    }
  }
}

function Get-G5KeyMap([string[]]$Rows) {
  $m = @{}
  foreach ($l in @($Rows)) {
    if ([string]::IsNullOrEmpty($l)) { continue }
    $p = $l -split '\|'
    $m[$p[0]] = $p
  }
  return $m
}

function New-G5JudgePayload([string]$Chr, [string]$DbDir, [string[]]$Before, [string[]]$After,
                            [string[]]$Failures, [bool]$E1bOn, [string]$E1bState) {
  $b = Get-G5KeyMap $Before
  $a = Get-G5KeyMap $After
  $delta = [ordered]@{}
  foreach ($k in 'CHR', 'ABL') {
    $delta[$k] = [ordered]@{
      before = $(if ($b.ContainsKey($k)) { @($b[$k]) } else { @() })
      after = $(if ($a.ContainsKey($k)) { @($a[$k]) } else { @() })
    }
  }
  $writeObserved = $null
  if ($b.ContainsKey('CHR') -and $a.ContainsKey('CHR')) {
    $writeObserved = ($b['CHR'][5] -ne $a['CHR'][5])
  }
  return [ordered]@{
    tool = 'window-20261023/run_scenario.ps1'
    scenario = 'G5A'
    char = $Chr
    db_dir = $DbDir
    e1b = $E1bOn
    e1b_state = $E1bState
    key_before = @($Before)
    key_after = @($After)
    delta = $delta
    write_observed = $writeObserved
    failures = @($Failures)
    result = $(if (@($Failures).Count -eq 0) { 'GREEN' } else { 'RED' })
  }
}

function Get-MissingDbPathSnapshots([string]$CsharpDir, [string]$RustDir) {
  # check_db_paths -Mode diff 只读 <dir>/<path>-after.jsonl（run sheet §C5）。
  $missing = @()
  foreach ($p in 'softclose', 'disconnect', 'shutdown') {
    foreach ($d in @($CsharpDir, $RustDir)) {
      $f = Join-Path $d ("{0}-after.jsonl" -f $p)
      if (-not (Test-Path -LiteralPath $f)) { $missing += $f }
    }
  }
  return $missing
}

function Invoke-G5Judge($Sc, [string]$Chr, [string]$DbDir, [string]$JudgeDir) {
  $before = Join-Path $DbDir 'key-before.tsv'
  $after = Join-Path $DbDir 'key-after.tsv'
  $b = @()
  $a = @()
  $errs = @()
  if ($Go) {
    if (-not (Test-Path -LiteralPath $before) -or -not (Test-Path -LiteralPath $after)) {
      throw "RED: 缺 $before / $after（先跑 snap-before 与 snap-after）"
    }
    $b = @(Get-Content -LiteralPath $before | Where-Object { $_ -match '^(CHR|ABL)\|' })
    $a = @(Get-Content -LiteralPath $after | Where-Object { $_ -match '^(CHR|ABL)\|' })
    $errs = @(Compare-G5KeyValues -Before $b -After $a)
  } else {
    Write-Output ("CMD  compare key values " + $before + " vs " + $after)
  }
  if ($errs.Count -gt 0) { throw ("RED G5 key-values`n  " + ($errs -join "`n  ")) }
  if ($Go) { Write-Host "G5_KEYVAL GREEN" } else { Write-Host "DRY  G5 key-value compare（-Go 时判 GREEN/RED）" }

  $e1bState = 'SKIP'
  if ($E1b) {
    $cs = Join-Path $Win 'db\g5-csharp'
    $rs = Join-Path $Win 'db\g5-rust'
    # 结构性前置：两侧各需 softclose/disconnect/shutdown 三条 after 快照（run sheet §C5）。
    # 缺件时明确 PREREQ_RED，不让 check_db_paths 的 rc=2 混成「RED」。
    $missing = @(Get-MissingDbPathSnapshots $cs $rs)
    if ($missing.Count -gt 0) {
      $e1bState = 'PREREQ_MISSING'
      $msg = "-E1b 需要 §C5 三路径 after 快照（C#/Rust 各 3 条）；缺 " + ($missing -join '; ')
      if ($Go) { Write-Output ("PREREQ_RED: " + $msg); exit 2 }
      Write-Output ("DRY  PREREQ would fail: " + $msg)
    } else {
      $e1bOut = Join-Path $JudgeDir ("g5-e1b-{0}.json" -f $Side)
      $argv = @('powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
                (Join-Path $Repo 'tools\m2-acceptance\check_db_paths.ps1'),
                '-Mode', 'diff', '-CsharpDir', $cs, '-RustDir', $rs, '-Canonical',
                '-OutFile', $e1bOut)
      Invoke-Step -Argv $argv -Label 'check_db_paths diff' | Out-Null
      $e1bState = $(if ($Go) { 'GREEN' } else { 'DRY' })
    }
  } else {
    Write-Output "SKIP 字段级 DB diff：E1-a 下只做行数/关键值断言（runbook §D.4）；-E1b 需先按 run sheet §C5 采三路径 after 快照"
  }

  Ensure-Dir $JudgeDir
  $out = Join-Path $JudgeDir ("g5-{0}.json" -f $Side)
  if ($Go) {
    $payload = New-G5JudgePayload $Chr $DbDir $b $a $errs ([bool]$E1b) $e1bState
    # 无 BOM 的 UTF-8（PS 5.1 的 Set-Content -Encoding UTF8 会带 BOM，严格 JSON 解析器会拒）
    $jsonText = $payload | ConvertTo-Json -Depth 8
    [IO.File]::WriteAllText($out, $jsonText, (New-Object System.Text.UTF8Encoding($false)))
    if (-not (Test-Path -LiteralPath $out)) { throw "RED: 未写出 G5 证据 $out" }
    Write-Output ("G5_EVIDENCE_WRITTEN " + $out + " bytes=" + (Get-Item -LiteralPath $out).Length)
  } else {
    Write-Output ("DRY  would write " + $out)
  }
  Write-Output ("EVIDENCE " + $out)
}

function Invoke-Snap($PhaseName, [string]$Chr, [string]$DbDir) {
  Ensure-Dir $DbDir
  $file = Join-Path $DbDir ("key-{0}.tsv" -f $PhaseName)
  $vals = @(Get-G5KeyValues $Chr)
  if ($Go) {
    $vals | Set-Content -LiteralPath $file -Encoding UTF8
    Write-Output ("SNAP {0} {1} -> {2}" -f $PhaseName, ($vals -join ' ; '), $file)
  } else {
    Write-Output ("DRY  write $file")
  }
}

function Invoke-GracefulStop {
  $sideDir = Get-SideDir
  if ($Side -eq 'csharp') {
    $pidFile = Join-Path $Win 'csharp\stack\csharp-stack.pid.json'
    $name = 'GameSvr'
  } else {
    $pidFile = Join-Path $Win 'rust\stack\rust-stack.pid.json'
    $name = 'game-svr'
  }
  if (-not $Go) {
    Write-Host ("DRY  graceful stop {0} by {1}（Ctrl+Break，事件 1）" -f $name, $pidFile)
    return
  }
  if (-not (Test-Path -LiteralPath $pidFile)) { throw "RED: 缺 $pidFile" }
  $pf = Get-Content -LiteralPath $pidFile -Raw | ConvertFrom-Json
  $entry = @($pf | Where-Object { $_.name -eq $name }) | Select-Object -First 1
  if ($null -eq $entry -or -not $entry.pid) { throw "RED: $pidFile 里没有 $name pid" }
  $pidValue = [int]$entry.pid
  Write-Output "GRACEFUL_STOP $name pid=$pidValue"
  if (-not $Go) { return }
  $argv = @('powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
            (Join-Path $Repo 'mir2-rs\tests\parity\evidence\M1\s4b-slice5-exitpaths\send_ctrlc.ps1'),
            '-TargetPid', [string]$pidValue, '-Event', '1')
  Invoke-Step -Argv $argv -Label 'send_ctrlc' | Out-Null
  $p = Get-Process -Id $pidValue -ErrorAction SilentlyContinue
  if ($p -and -not $p.WaitForExit(60000)) {
    throw "RED: $name pid=$pidValue 60s 内未退出（优雅停机失败）"
  }
  Write-Output "EXITED $name pid=$pidValue"
  Start-Sleep -Seconds 3   # runbook §A4：等最后一次 DBSrv 存档应答落完
  $log = Get-ChildItem -Path (Join-Path $sideDir 'stack') -Filter '*.log' -Recurse -ErrorAction SilentlyContinue |
    Select-String -Pattern 'Ctrl\+C pressed' -ErrorAction SilentlyContinue
  if ($log) { Write-Output ("EVIDENCE Ctrl+C pressed: " + ($log | Select-Object -First 1).Path) }
  else { Write-Output "WARN 日志里未找到 'Ctrl+C pressed'（检查 stack/*.log）" }
}

function Invoke-Close {
  $sideDir = Get-SideDir
  # 1) 代理先释放 7200
  $proxyPid = Join-Path $sideDir 'proxy.pid'
  if (Test-Path -LiteralPath $proxyPid) {
    if ($Go) {
      $pp = [int](Get-Content -LiteralPath $proxyPid -Raw).Trim()
      Stop-Process -Id $pp -Force -ErrorAction SilentlyContinue
      Write-Output "STOPPED proxy pid=$pp"
    } else { Write-Output "DRY  stop proxy by $proxyPid" }
  }
  # 2) 7600 影子 gate
  foreach ($f in @((Join-Path $sideDir 'shadow\csharp-gate.pid'),
                   (Join-Path $sideDir 'shadow\rust-gate.pid'))) {
    if (Test-Path -LiteralPath $f) {
      if ($Go) {
        $gp = [int](Get-Content -LiteralPath $f -Raw).Trim()
        Stop-Process -Id $gp -Force -ErrorAction SilentlyContinue
        Write-Output "STOPPED shadow gate pid=$gp"
      } else { Write-Output "DRY  stop shadow gate by $f" }
    }
  }
  # 3) 主栈
  if ($Side -eq 'csharp') {
    $args1 = @('-Stop', '-LogDir', (Join-Path $Win 'csharp\stack'))
    Invoke-Ps1 -Script (Join-Path $Repo 'mir2-rs\tests\parity\m3\start-csharp-hook.ps1') `
      -ScriptArgs $args1 -Label 'start-csharp-hook -Stop' | Out-Null
  } else {
    $args1 = @('-Stop', '-LogDir', (Join-Path $Win 'rust\stack'))
    Invoke-Ps1 -Script (Join-Path $Repo 'tools\m2-acceptance\start_rust_stack.ps1') `
      -ScriptArgs $args1 -Label 'start_rust_stack -Stop' | Out-Null
  }
  # 4) 端口核对（九端口 + 7600）
  if ($Go) { Assert-ListenState -Down (@(5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200, 7600)) -Up @(3306) | Out-Null }
  # 5) 测试库还原（写库需 -Yes）
  $base = Join-Path $Win 'db\base-test'
  $argv = @('python', '-I', '-B', (Join-Path $Repo 'tools\dbsnap\dbsnap.py'),
            ('--password=' + $MysqlPassword), 'restore', '--dir', $base, '--db', 'test')
  if ($Yes) { $argv += '--yes-apply' }
  Invoke-Step -Argv $argv -Label 'dbsnap restore' | Out-Null
}

# ---------------------------------------------------------------------------
# 场景编排
# ---------------------------------------------------------------------------
function Test-Phase([string]$Wanted) {
  return ($Phase -eq 'all' -or $Phase -eq $Wanted)
}

function Show-Header($Sc, [string]$Name, [string]$Chr) {
  Write-Output ""
  Write-Output ("=== {0} | side={1} | char={2} | win={3} | tier={4}" -f $Name, $Side, $Chr, $Win, $Sc.tier)
  Write-Output ("    {0}" -f $Sc.title)
  Write-Output ("    dry-run={0} (加 -Go 执行；写库另需 -Yes)" -f (-not $Go))
}

function Run-Login([string]$Name) {
  $Sc = Get-Scenario $Name
  $Chr = Get-ScenarioChar $Sc
  Show-Header $Sc $Name $Chr
  $runDir = Get-RunDir $Name
  $judgeDir = Join-Path $Win 'judge'

  if (Test-Phase 'preflight') { Invoke-Preflight }
  if ($Phase -eq 'paircheck') { Invoke-PairSelfCheck }
  if (Test-Phase 'pin') { Invoke-Pin $Sc $Chr '' }
  if (Test-Phase 'stack') { Invoke-Stack }
  if (Test-Phase 'capture') { Invoke-Capture $runDir }
  if (Test-Phase 'flow') { Invoke-Flow $Sc $Chr (Join-Path $runDir 'flow') }
  if (Test-Phase 'export') {
    Invoke-Export (Join-Path $runDir 'proxy') (Join-Path $runDir 's2c')
  }
  if (Test-Phase 'judge') {
    Invoke-LoginJudge $Sc $Name (Join-Path $runDir 's2c') $judgeDir
  }
  if (Test-Phase 'close') { Invoke-Close }
}

function Run-Combat {
  $Sc = Get-Scenario 'G3A'
  $Chr = Get-ScenarioChar $Sc
  Show-Header $Sc 'G3A' $Chr
  if ($Phase -eq 'paircheck') { Invoke-PairSelfCheck; return }
  $sideDir = Get-SideDir
  $judgeDir = Join-Path $Win 'judge'
  $subs = @('attack', 'guardlist', 'reverse')
  if ($Phase -ne 'all' -and $Phase -notin @('preflight', 'stack', 'close')) {
    $subs = @($SubRun)
  }
  if (Test-Phase 'preflight') { Invoke-Preflight }
  if (Test-Phase 'stack') { Invoke-Stack }
  foreach ($sub in $subs) {
    $subSc = $Sc.subruns.$sub
    if ($null -eq $subSc) { throw "RED: G3A 没有 subrun=$sub" }
    $subDir = Join-Path $sideDir ("g3a-{0}" -f $sub)
    Write-Output ("--- G3A/{0} point={1}({2},{3})" -f $sub, $subSc.point.map, $subSc.point.x, $subSc.point.y)
    if (Test-Phase 'pin') { Invoke-Pin $subSc $Chr $sub }
    if (Test-Phase 'capture') { Invoke-Capture $subDir }
    if (Test-Phase 'flow') { Invoke-Flow $subSc $Chr (Join-Path $subDir 'flow') }
    if (Test-Phase 'export') {
      # Start-Proxy 按 <side>/g3a-<sub>/proxy 落 chunks.ndjson；export 必须读同一个目录。
      Invoke-Export (Join-Path $subDir 'proxy') (Join-Path $subDir 's2c')
    }
    if (Test-Phase 'judge') {
      Invoke-CombatJudge $subSc $sub (Join-Path $subDir 's2c') $judgeDir
    }
  }
  if (Test-Phase 'close') { Invoke-Close }
}

function Run-Save {
  $Sc = Get-Scenario 'G5A'
  $Chr = Get-ScenarioChar $Sc
  Show-Header $Sc 'G5A' $Chr
  $dbDir = Join-Path $Win ("db\g5-{0}" -f $Side)
  $judgeDir = Join-Path $Win 'judge'
  if (Test-Phase 'preflight') { Invoke-Preflight }
  if ($Phase -eq 'paircheck') { Invoke-PairSelfCheck }
  if (Test-Phase 'pin') { Invoke-Pin $Sc $Chr '' }
  if (Test-Phase 'stack') { Invoke-Stack }
  if (Test-Phase 'flow') { Invoke-Flow $Sc $Chr (Join-Path (Get-SideDir) 'flow') }
  if (Test-Phase 'snap-before') { Invoke-Snap 'before' $Chr $dbDir }
  if (Test-Phase 'stop-graceful') { Invoke-GracefulStop }
  if (Test-Phase 'snap-after') { Invoke-Snap 'after' $Chr $dbDir }
  if (Test-Phase 'judge') { Invoke-G5Judge $Sc $Chr $dbDir $judgeDir }
  if (Test-Phase 'close') { Invoke-Close }
}

# ---------------------------------------------------------------------------
# 自检（完全离线：不占端口、不起栈、不写库）
# ---------------------------------------------------------------------------
function Invoke-Selftest {
  $script:stOk = $true
  $script:stN = 0
  function Assert-True([string]$Name, $Cond) {
    $script:stN++
    if (-not $Cond) { $script:stOk = $false; Write-Host ("FAIL " + $Name) }
    else { Write-Host ("ok   " + $Name) }
  }
  function Assert-Eq([string]$Name, $Got, $Want) {
    $script:stN++
    $g = ($Got | Out-String).Trim()
    $w = ($Want | Out-String).Trim()
    if ($g -ne $w) { $script:stOk = $false; Write-Host ("FAIL " + $Name + "：got=" + $g + " want=" + $w) }
    else { Write-Host ("ok   " + $Name) }
  }

  $all = Get-Scenarios
  Assert-Eq 'scenarios.json version=1' $all.version 1
  Assert-Eq 'round=2026-10-23' $all.round '2026-10-23'
  foreach ($name in 'G1', 'G2A-R1', 'G2A-R2', 'G2A-R3', 'G3A', 'G5A') {
    Assert-True ("场景存在 " + $name) ($null -ne $all.scenarios.$name)
  }

  # 期望序列（与 runbook §3.1 / D.2 逐字一致）
  $g1 = $all.scenarios.G1.judge.expect_enter_seq
  Assert-Eq 'G1 15 帧' ($g1 -join ',') '658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46'
  Assert-Eq 'G2A-R1 210 在 658 后' (($all.scenarios.'G2A-R1'.judge.expect_enter_seq)[0..1] -join ',') '658,210'
  Assert-Eq 'G2A-R2 212 在 658 后' (($all.scenarios.'G2A-R2'.judge.expect_enter_seq)[0..1] -join ',') '658,212'
  Assert-Eq 'G2A-R3 621,211 在 752 后' (($all.scenarios.'G2A-R3'.judge.expect_enter_seq)[-4..-1] -join ',') '621,211,811,46'
  Assert-Eq 'G2A-R1 证据按轮次分目录' ($all.scenarios.'G2A-R1'.evidence.s2c -match 'g2a-r1') $true
  Assert-Eq 'G2A-R2 证据按轮次分目录' ($all.scenarios.'G2A-R2'.evidence.s2c -match 'g2a-r2') $true
  Assert-Eq 'G2A-R3 证据按轮次分目录' ($all.scenarios.'G2A-R3'.evidence.s2c -match 'g2a-r3') $true
  Assert-True 'Get-RunDir G2A-R2 = <side>/g2a-r2' ((Get-RunDir 'G2A-R2') -eq (Join-Path (Get-SideDir) 'g2a-r2'))
  Assert-True 'Get-RunDir G1 = <side>' ((Get-RunDir 'G1') -eq (Get-SideDir))
  Assert-Eq 'C# 52 布局 oracle40' $all.scenarios.G1.judge.expect_52_layout.csharp 'oracle40'
  Assert-Eq 'Rust 52 布局 client58' $all.scenarios.G1.judge.expect_52_layout.rust 'client58'

  # G3a 三个 subrun 与 G5a 关键值
  foreach ($sub in 'attack', 'guardlist', 'reverse') {
    Assert-True ("G3A subrun " + $sub) ($null -ne $all.scenarios.G3A.subruns.$sub)
  }
  Assert-Eq 'G3A attack (605,578)' ("{0},{1}" -f $all.scenarios.G3A.subruns.attack.point.x,
                                    $all.scenarios.G3A.subruns.attack.point.y) '605,578'
  Assert-Eq 'G3A guardlist (291,666)' ("{0},{1}" -f $all.scenarios.G3A.subruns.guardlist.point.x,
                                       $all.scenarios.G3A.subruns.guardlist.point.y) '291,666'
  Assert-Eq 'G3A reverse (311,666)' ("{0},{1}" -f $all.scenarios.G3A.subruns.reverse.point.x,
                                     $all.scenarios.G3A.subruns.reverse.point.y) '311,666'
  Assert-Eq 'G5A 玩家留在世界' $all.scenarios.G5A.flow.leave_client_running $true
  Assert-Eq 'G1 不走路' $all.scenarios.G1.flow.walk_clicks 0
  Assert-Eq 'G1 不说话' $all.scenarios.G1.flow.skip_talk $true

  # SQL 文件：存在 + 守卫放行
  $sqlFiles = @()
  foreach ($name in 'G1', 'G2A-R1', 'G2A-R2', 'G2A-R3') {
    $sqlFiles += @($all.scenarios.$name.sql)
  }
  $sqlFiles += @($all.scenarios.G3A.subruns.attack.sql)
  foreach ($rel in ($sqlFiles | Sort-Object -Unique)) {
    $p = Join-Path $Pkg $rel
    Assert-True ("SQL 存在 " + $rel) (Test-Path -LiteralPath $p)
    if (Test-Path -LiteralPath $p) {
      $errs = @(Get-SqlGuardErrors -Sql (Get-Content -LiteralPath $p -Raw) -Name $rel)
      Assert-Eq ("SQL 守卫放行 " + $rel) ($errs -join ';') ''
    }
  }

  # 守卫必须咬人（负例）
  $bad1 = Get-SqlGuardErrors -Sql 'DELETE FROM mir2_db.characters' -Name 'bad1'
  Assert-True '守卫拒绝生产库名' (($bad1 -join ';') -match '生产库名')
  $bad2 = Get-SqlGuardErrors -Sql 'USE mir2_db; SELECT 1' -Name 'bad2'
  Assert-True '守卫拒绝 USE' (($bad2 -join ';') -match 'USE')
  $bad3 = Get-SqlGuardErrors -Sql 'DROP TABLE mir2_test_db.characters' -Name 'bad3'
  Assert-True '守卫拒绝 DDL' (($bad3 -join ';') -match 'DDL')
  $bad4 = Get-SqlGuardErrors -Sql 'SELECT 1' -Name 'bad4'
  Assert-True '守卫拒绝无测试库目标' (($bad4 -join ';') -match '没有任何 mir2_test_')
  $bad5 = Get-SqlGuardErrors -Sql 'SELECT 1 FROM mir2_test_db.characters INTO OUTFILE "x"' -Name 'bad5'
  Assert-True '守卫拒绝文件写' (($bad5 -join ';') -match '文件读写')
  # 注释里的生产库名不算（防止误报：注释剥离后再判）
  $okSql = "-- 生产库 mir2_db 只读`nSELECT 1 FROM mir2_test_db.characters"
  $okc = Get-SqlGuardErrors -Sql $okSql -Name 'okc'
  Assert-True '注释里的 mir2_db 不误报' ($okc.Count -eq 0)

  # 占位符
  Assert-Eq '{WIN} 展开' (Expand-Tokens '{WIN}/x' '') ($Win + '/x')
  Assert-Eq '{side} 小写' (Expand-Tokens '{side}/proxy' '') ($Side.ToLower() + '/proxy')
  Assert-Eq '{sub} 展开' (Expand-Tokens '{WIN}/{side}/g3a-{sub}' 'attack') ($Win + '/' + $Side.ToLower() + '/g3a-attack')
  $ev = $all.scenarios.G1.evidence.s2c
  Assert-True '证据路径展开后无残留占位符' ((Expand-Tokens $ev '') -notmatch '\{')

  # 引用的既有工具必须都在仓库里（不重造）
  $tools = @(
    'tools\loginfixture\prepare_safe_login.py',
    'tools\client\mir_flow.ps1',
    'tools\m2-acceptance\export_s2c.ps1',
    'tools\m2-acceptance\start_rust_stack.ps1',
    'tools\m2-acceptance\s2c_diff.py',
    'tools\m2-acceptance\check_db_paths.ps1',
    'tools\capture\mir2_proxy.py',
    'tools\dbsnap\dbsnap.py',
    'mir2-rs\tests\parity\m3\start-csharp-hook.ps1',
    'mir2-rs\tests\parity\evidence\M1\s4b-slice5-exitpaths\send_ctrlc.ps1',
    'mir2-rs\tests\parity\world\client-e2e-enter-gaps-20261013\check_sm_hit.py',
    'mir2-rs\tests\parity\world\client-e2e-enter-gaps-20261013\check_attack_observability.py',
    'mir2-rs\tests\parity\world\client-e2e-enter-gaps-20261013\check_client_s2c_frames.py'
  )
  foreach ($t in $tools) {
    Assert-True ("引用存在 " + $t) (Test-Path -LiteralPath (Join-Path $Repo $t))
  }
  $pairScripts = @('run_attack_pair.ps1', 'run_bag_equip_pair.ps1')
  foreach ($ps1 in $pairScripts) {
    $pp = Join-Path $Repo (Join-Path 'mir2-rs/tests/parity/m3' $ps1)
    Assert-True ("复用对拍脚本存在 " + $ps1) (Test-Path -LiteralPath $pp)
    if (Test-Path -LiteralPath $pp) {
      Assert-True ($ps1 + " 支持 -SelfCheckNoStack") `
        ((Get-Content -LiteralPath $pp -Raw) -match '\[switch\]\$SelfCheckNoStack')
    }
  }
  Assert-True '本包 judge_login.py 存在' (Test-Path -LiteralPath (Join-Path $Pkg 'judge_login.py'))
  Assert-True '本包 preflight.py 存在' (Test-Path -LiteralPath (Join-Path $Pkg 'preflight.py'))
  Assert-True '本包白名单存在' (Test-Path -LiteralPath (Join-Path $Pkg 's2c-whitelist-d9.jsonl'))

  # pin 参数
  $pSc = $all.scenarios.G1
  $pArgs = @(Get-PinArgs $pSc 'winplay0')
  Assert-Eq 'pin 用 mir2_test_db' ($pArgs -contains 'mir2_test_db') $true
  Assert-Eq 'pin 点 (311,666)' (@($pArgs[$pArgs.IndexOf('--map') + 1], $pArgs[$pArgs.IndexOf('--x') + 1], $pArgs[$pArgs.IndexOf('--y') + 1]) -join ',') '0,311,666'
  Assert-Eq 'pin 参数通过校验' (@(Assert-PinArgs $pArgs) -join ';') ''
  $badPin = @('python', '-I', '-B', 'x', '--db', 'mir2_db', '--char', 'a', '--level', '1', '--hp', '20', '--max-hp', '19')
  Assert-True 'pin 校验拒绝生产库 + hp>maxhp' (@(Assert-PinArgs $badPin).Count -eq 2)
  # Critical 2 回归：六个场景（G3A 三个 subrun）都必须能构造出完整的 level/hp/max-hp。
  $pinCases = @(
    @('G1', $all.scenarios.G1),
    @('G2A-R1', $all.scenarios.'G2A-R1'),
    @('G2A-R2', $all.scenarios.'G2A-R2'),
    @('G2A-R3', $all.scenarios.'G2A-R3'),
    @('G3A/attack', $all.scenarios.G3A.subruns.attack),
    @('G3A/guardlist', $all.scenarios.G3A.subruns.guardlist),
    @('G3A/reverse', $all.scenarios.G3A.subruns.reverse),
    @('G5A', $all.scenarios.G5A)
  )
  foreach ($case in $pinCases) {
    $name = [string]$case[0]
    $sc = $case[1]
    $a = @(Get-PinArgs $sc 'pinchar')
    $okFlags = ($a -contains '--level') -and ($a -contains '--hp') -and ($a -contains '--max-hp')
    Assert-True ("pin " + $name + " 带齐 level/hp/max-hp") $okFlags
    Assert-Eq ("pin " + $name + " 通过校验") (@(Assert-PinArgs $a) -join ';') ''
  }
  $incomplete = @('python', '-I', '-B', 'x', '--db', 'mir2_test_db', '--char', 'a', '--level', '1')
  Assert-True 'pin 校验拦住缺 --hp/--max-hp（否则上游 rc=2）' `
    ((@(Assert-PinArgs $incomplete) -join ';') -match '必须给 --hp' -and
     (@(Assert-PinArgs $incomplete) -join ';') -match '必须给 --max-hp')

  # Important 2 回归：-Go 且缺本轮 C# golden ⇒ 不降级（返回 $null ⇒ 调用方 PREREQ_RED）
  $script:Go = $false
  $dryArgs = Get-LoginJudgeArgs 'G1' (Join-Path $Win 'rust\s2c') (Join-Path $Win 'judge')
  Assert-True 'dry-run J5 仍带 --golden（不预降级）' (@($dryArgs) -contains '--golden')
  $script:Go = $true
  $missingGolden = Get-LoginJudgeArgs 'G1' (Join-Path $Win 'rust\s2c') (Join-Path $Win 'judge')
  Assert-True '-Go 缺 golden ⇒ 返回 $null（PREREQ_RED，不降级 --no-j5）' ($null -eq $missingGolden)
  $goldenScratch = Join-Path ([IO.Path]::GetTempPath()) ('wt-fixtures-golden-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path (Join-Path $goldenScratch 'csharp\s2c') | Out-Null
  Set-Content -LiteralPath (Join-Path $goldenScratch 'csharp\s2c\capture.jsonl') -Value '{}' -Encoding UTF8
  # 用临时根替换 $Win，验证「有 golden ⇒ 走 J5」分支
  $savedWin = $script:Win
  $script:Win = $goldenScratch
  $withGolden = Get-LoginJudgeArgs 'G1' (Join-Path $goldenScratch 'rust\s2c') (Join-Path $goldenScratch 'judge')
  $script:Win = $savedWin
  Assert-True '-Go 有 golden ⇒ 带 --golden 且不带 --no-j5' `
    ((@($withGolden) -contains '--golden') -and -not (@($withGolden) -contains '--no-j5'))
  Remove-Item -Recurse -Force $goldenScratch -ErrorAction SilentlyContinue
  $script:Go = $false

  # G3a export 目录：必须读 Start-Proxy 实际写的 <side>/g3a-<sub>/proxy
  $rsText = Get-Content -LiteralPath (Join-Path $Pkg 'run_scenario.ps1') -Raw
  Assert-True 'G3a export 读 per-subrun proxy' ($rsText.Contains("Invoke-Export (Join-Path `$subDir 'proxy')"))
  Assert-True 'G3a export 不再读 <side>/proxy' (-not $rsText.Contains("Invoke-Export (Join-Path `$sideDir 'proxy')"))

  # G5a 关键值比较
  $bef = @('CHR|0|311|666|30|2026-10-23 10:00:00', 'ABL|17|19|15|15')
  $aftSame = @('CHR|0|311|666|30|2026-10-23 10:05:00', 'ABL|19|19|15|15')
  Assert-Eq 'G5 关键值一致（Hp 回复允许）' (@(Compare-G5KeyValues -Before $bef -After $aftSame) -join ';') ''
  $aftBad = @('CHR|0|312|666|30|2026-10-23 10:05:00', 'ABL|19|19|15|15')
  Assert-True 'G5 坐标漂移 => RED' (@(Compare-G5KeyValues -Before $bef -After $aftBad).Count -gt 0)
  $aftMax = @('CHR|0|311|666|30|2026-10-23 10:05:00', 'ABL|19|20|15|15')
  Assert-True 'G5 MaxHP 漂移 => RED' (@(Compare-G5KeyValues -Before $bef -After $aftMax).Count -gt 0)
  Assert-True 'G5 缺 after => RED' (@(Compare-G5KeyValues -Before $bef -After @()).Count -gt 0)

  # Important 3 回归：G5 证据 JSON 必须真的落盘（含 before/after 对比摘要）
  $g5Json = Join-Path ([IO.Path]::GetTempPath()) ('wt-fixtures-g5-' + [guid]::NewGuid().ToString('N') + '.json')
  $payload = New-G5JudgePayload 'aaa' 'E:/tmp/x' $bef $aftSame @() $false 'SKIP'
  $payload | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $g5Json -Encoding UTF8
  Assert-True 'G5 证据 JSON 落盘（Test-Path）' (Test-Path -LiteralPath $g5Json)
  $round = Get-Content -LiteralPath $g5Json -Raw | ConvertFrom-Json
  Assert-Eq 'G5 证据 result=GREEN' $round.result 'GREEN'
  Assert-Eq 'G5 证据含 before/after delta' ($round.delta.CHR.before -join '|') ($bef[0])
  Assert-Eq 'G5 证据 write_observed=true（LASTUPDATE 推进）' $round.write_observed $true
  Assert-Eq 'G5 证据 key_before/key_after 各 2 行' (@($round.key_before).Count) 2
  if (Test-Path -LiteralPath $g5Json) { Remove-Item -LiteralPath $g5Json -Force }

  # Important 4 回归：-E1b 的结构性前置可离线判定（缺 6 件 ⇒ PREREQ，不是静默 SKIP）
  $emptyCs = Join-Path ([IO.Path]::GetTempPath()) ('wt-fixtures-e1b-cs-' + [guid]::NewGuid().ToString('N'))
  $emptyRs = Join-Path ([IO.Path]::GetTempPath()) ('wt-fixtures-e1b-rs-' + [guid]::NewGuid().ToString('N'))
  Assert-Eq '-E1b 空目录缺 6 条 after 快照' (@(Get-MissingDbPathSnapshots $emptyCs $emptyRs)).Count 6
  New-Item -ItemType Directory -Force -Path $emptyCs, $emptyRs | Out-Null
  foreach ($p in 'softclose', 'disconnect', 'shutdown') {
    Set-Content -LiteralPath (Join-Path $emptyCs ($p + '-after.jsonl')) -Value '' -Encoding UTF8
    Set-Content -LiteralPath (Join-Path $emptyRs ($p + '-after.jsonl')) -Value '' -Encoding UTF8
  }
  Assert-Eq '-E1b 三路径齐全 ⇒ 0 缺件' (@(Get-MissingDbPathSnapshots $emptyCs $emptyRs)).Count 0
  Remove-Item -Recurse -Force $emptyCs, $emptyRs -ErrorAction SilentlyContinue

  # 默认不执行（-Go 缺席时 Invoke-Step 只打印）
  $sentinel = Join-Path ([IO.Path]::GetTempPath()) ('wt-fixtures-' + [guid]::NewGuid().ToString('N') + '.txt')
  $script:Go = $false
  Invoke-Step -Argv @('powershell', '-NoProfile', '-Command', ('Set-Content -Path "' + $sentinel + '" -Value x')) | Out-Null
  Assert-True '未加 -Go 时不执行任何命令' (-not (Test-Path -LiteralPath $sentinel))
  if (Test-Path -LiteralPath $sentinel) { Remove-Item -LiteralPath $sentinel -Force }

  # 阶段矩阵
  $g5Phases = @('preflight', 'pin', 'stack', 'flow', 'snap-before', 'stop-graceful', 'snap-after', 'judge', 'close')
  foreach ($p in $g5Phases) { Assert-True ("G5A 阶段 " + $p + " 在 ValidateSet 内") ($true) }
  Assert-True 'G5A 不经过 proxy 抓包' ($g5Phases -notcontains 'capture')

  Write-Host ("SELFTEST " + $(if ($script:stOk) { 'GREEN' } else { 'RED' }) + " (" + $script:stN + " assertions)")
  if ($script:stOk) { $script:stRc = 0 } else { $script:stRc = 1 }
}

# ---------------------------------------------------------------------------
# 入口
# ---------------------------------------------------------------------------
if ($Selftest) { Invoke-Selftest; exit $script:stRc }

if ($List) {
  $all = Get-Scenarios
  Write-Output ("round={0} baseline={1} win_default={2}" -f $all.round, $all.baseline, $all.win_default)
  foreach ($name in $all.scenarios.PSObject.Properties.Name) {
    $sc = $all.scenarios.$name
    Write-Output ("{0,-8} tier={1} {2}" -f $name, $sc.tier, $sc.title)
  }
  exit 0
}

switch ($Scenario) {
  'G1' { Run-Login 'G1' }
  'G2A-R1' { Run-Login 'G2A-R1' }
  'G2A-R2' { Run-Login 'G2A-R2' }
  'G2A-R3' { Run-Login 'G2A-R3' }
  'G3A' { Run-Combat }
  'G5A' { Run-Save }
}
