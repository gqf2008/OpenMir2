#requires -Version 5
<#
.SYNOPSIS
  check_isolate_plumb.ps1 — `start_rust_stack.ps1` 的 `MIR2_RNG_ISOLATE_ATTACK` 攻击路径隔离管线的
  **可判红/绿**判据（卡 `openmir2-m2-rust-isolate-attack-plumb`）。

.DESCRIPTION
  M3②③ 的「活栈隔离口径」（owner 2026-10-12 21:15 §6.0-3 签字）要求两侧把**攻击路径取数**接到各自
  一条专用种子流：C# 半由 `mir2-rs/tests/parity/m3/start-csharp-hook.ps1 -IsolateAttack` 给 GameSvr
  进程加 `MIR2_RNG_ISOLATE_ATTACK=1`；Rust 半由 `crates/shared/src/rng.rs::attack_isolation_seed_from_env`
  读同一个变量、`crates/world` 在 `World::with_maps` 构造时分流（`attack_seed = MIR2_RNG_SEED + 0x9E37`）。
  但启动器此前**没有这条管线** ⇒ 只有 operator 手工在起栈 shell 里设过变量才有隔离，否则 Rust 半走
  全局流、C# 半走专用流 ⇒ 把「隔离后的逐 k=0」伪装成口径错误。本脚本用构造输入把这条管线
  （`rust_seed_plumb.ps1`）的每条口径都跑一遍。

  检查项（**不起完整栈、不占真端口**：全部用临时目录 + 空闲端口起单个 game-svr）：
    ① `Resolve-Mir2IsolateAttack`：未设/空串/关值 ⇒ `Present=$false`；显式开关 ⇒ 开；环境值 `YES`
       ⇒ 开；非法值 ⇒ **抛错**（fail-closed）。
    ② 注入到位：`Start-SeededProcess -IsolateAttack <on>` 起 `cmd /c echo %MIR2_RNG_ISOLATE_ATTACK%`
       ⇒ 读出 `1`；关时不注入；起完还原调用方环境。删掉注入行 ⇒ 本项必红。
    ③ 真 game-svr（+ `MIR2_RNG_LOG` 流水自证）：传 `-Seed 42` + 隔离开 ⇒ 在流水头出现
       `ISOLATE_ATTACK 40545`（= 42 + 0x9E37）⇒ 证明环境确实到达生产读取点且解析正确。
    ④ 只传种子、不开隔离 ⇒ 真 game-svr 正常启动，且流水**不含** `ISOLATE_ATTACK` 行（零行为差）。
    ⑤ `Get-IsolateAttackInjectedLine` 形态：开 ⇒ `RUST_ISOLATE_ATTACK_INJECTED=1`；关 ⇒ `$null`。
    ⑥ 结构接线：`start_rust_stack.ps1` 点源本管线、把 `-IsolateAttack $isolateInfo` 传给 game-svr、
       管线文件确有 `$env:MIR2_RNG_ISOLATE_ATTACK` 注入行，且**交叉 fail-closed 守卫**
       （隔离开但无种子）真值表成立并被 start_rust_stack 调用。

  任何一条不符 ⇒ 打印 `ISOLATE_PLUMB FAIL` 并 exit 1；全绿 ⇒ `ISOLATE_PLUMB GREEN` exit 0。
  缺 game-svr.exe ⇒ `PREREQ_MISSING` exit 2（**不静默通过**）。

.PARAMETER GameSvrExe
  被测的 `game-svr.exe`。默认按隔离 target（`$env:CARGO_TARGET_DIR\debug\game-svr.exe`），
  否则回落仓库默认 target（`mir2-rs\target\debug\game-svr.exe`）。

.PARAMETER OutDir
  夹具/原始件目录（默认 `E:\tmp\m2-acceptance\isolate-plumb-<时间戳>`）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_isolate_plumb.ps1 `
    -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
#>
param(
  [string]$GameSvrExe = "",
  [string]$OutDir = ""
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..')).Path

# 管线（被测对象）：与 `start_rust_stack.ps1` 共用同一份。
. (Join-Path $Here 'rust_seed_plumb.ps1')

if ([string]::IsNullOrEmpty($GameSvrExe)) {
  if (-not [string]::IsNullOrEmpty($env:CARGO_TARGET_DIR)) {
    $GameSvrExe = Join-Path $env:CARGO_TARGET_DIR 'debug\game-svr.exe'
  } else {
    $GameSvrExe = Join-Path $RepoRoot 'mir2-rs\target\debug\game-svr.exe'
  }
}
if (-not (Test-Path $GameSvrExe)) {
  Write-Output ("PREREQ_MISSING: 缺 game-svr.exe: " + $GameSvrExe + "（先 cargo build -p game-svr 或传 -GameSvrExe）")
  exit 2
}

if ([string]::IsNullOrEmpty($OutDir)) {
  $OutDir = Join-Path 'E:\tmp\m2-acceptance' ('isolate-plumb-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Work = Join-Path $OutDir 'work'
# **干净起点**：同一 `-OutDir` 复用时会残留上一轮的 `rng-stream.tsv` / 日志 ⇒ `Wait-FileMatch`
# 可能读到陈旧证据而**假绿**（红检 D 实测踩到：改坏注入值后 ③a 仍读上一轮的 ISOLATE_ATTACK 行）。
# 每轮先删 work 目录，保证判据只看本轮产物。
if (Test-Path -LiteralPath $Work) { Remove-Item -LiteralPath $Work -Recurse -Force -ErrorAction SilentlyContinue }
New-Item -ItemType Directory -Force -Path $Work | Out-Null

$script:Checks = New-Object System.Collections.Generic.List[object]
function Check([string]$Name, [bool]$Ok, [string]$Detail) {
  $tag = if ($Ok) { 'OK  ' } else { 'FAIL' }
  Write-Output ("  [{0}] {1}" -f $tag, $Name)
  if (-not [string]::IsNullOrEmpty($Detail)) { Write-Output ("        " + $Detail) }
  $script:Checks.Add([pscustomobject]@{ name = $Name; ok = $Ok; detail = $Detail })
}

function Free-Port {
  $l = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  $l.Start(); $p = $l.LocalEndpoint.Port; $l.Stop(); return $p
}

function Write-Conf([string]$Dir) {
  New-Item -ItemType Directory -Force -Path $Dir | Out-Null
  $conf = Join-Path $Dir 'Server.conf'
  $text = "[Server]`nGateAddr=127.0.0.1`nGatePort=$(Free-Port)`nCheckBlock=12288`n" +
          "IDSAddr=127.0.0.1`nIDSPort=$(Free-Port)`nDBAddr=127.0.0.1`nDBPort=$(Free-Port)`nWorldMaps=64x64`n"
  Set-Content -Path $conf -Value $text -Encoding ascii -NoNewline
  return $conf
}

function Wait-LogContains([string]$Path, [string]$Needle, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ((Test-Path $Path) -and ((Get-Content $Path -Raw -ErrorAction SilentlyContinue) -match [regex]::Escape($Needle))) {
      return $true
    }
    Start-Sleep -Milliseconds 100
  }
  return $false
}

function Wait-FileMatch([string]$Path, [string]$Pattern, [int]$TimeoutSec) {
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  while ((Get-Date) -lt $deadline) {
    if ((Test-Path $Path) -and ((Get-Content $Path -Raw -ErrorAction SilentlyContinue) -match $Pattern)) {
      return $true
    }
    Start-Sleep -Milliseconds 100
  }
  return $false
}

function Kill-Tree([int]$ProcessId) {
  if ($ProcessId -le 0) { return }
  & taskkill /T /F /PID $ProcessId > $null 2>&1
}

# 保证本脚本自身的用例不被调用方 shell 的残留污染。
foreach ($v in @('MIR2_RNG_SEED', 'MIR2_RNG_ISOLATE_ATTACK', 'MIR2_RNG_STATE_LOG', 'MIR2_RNG_LOG')) {
  if (Test-Path ("Env:" + $v)) { Remove-Item ("Env:" + $v) -ErrorAction SilentlyContinue }
}

Write-Output ("ISOLATE_PLUMB 判据：game-svr=" + $GameSvrExe + "  evidence=" + $OutDir)

# ---- ① Resolve-Mir2IsolateAttack ----
$unset = Resolve-Mir2IsolateAttack -IsolateSwitch:$false -EnvValue ''
Check '①a 未设/空串 ⇒ Present=$false' ((-not $unset.Present) -and ($unset.On -eq $false)) ("Present=" + $unset.Present)

$switchOn = Resolve-Mir2IsolateAttack -IsolateSwitch:$true -EnvValue ''
Check '①b 显式 -IsolateAttack ⇒ Present=$true' (($switchOn.Present) -and ($switchOn.On -eq $true)) ("Present=" + $switchOn.Present)

$envOn = Resolve-Mir2IsolateAttack -IsolateSwitch:$false -EnvValue 'YES'
Check '①c 环境值 YES ⇒ 开' (($envOn.Present) -and ($envOn.On -eq $true)) ("Present=" + $envOn.Present)

$envOff = Resolve-Mir2IsolateAttack -IsolateSwitch:$false -EnvValue 'off'
Check '①d 环境值 off ⇒ 关' ((-not $envOff.Present) -and ($envOff.On -eq $false)) ("Present=" + $envOff.Present)

$invalidThrew = $false
try { [void](Resolve-Mir2IsolateAttack -IsolateSwitch:$false -EnvValue 'maybe') } catch { $invalidThrew = $true }
Check '①e 非法值 ⇒ 抛错 fail-closed' $invalidThrew '既不静默当开也不静默当关（同 Rust attack_isolation_seed_from_env 的 panic 口径）'

# ---- ② 注入到位（cmd 子进程读环境；删掉注入行 ⇒ 本项必红）----
$envOutOn = Join-Path $Work 'env-iso-on.txt'
$pEnvOn = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo %MIR2_RNG_ISOLATE_ATTACK%') `
            -RedirectStandardOutput $envOutOn -IsolateAttack $switchOn
[void]$pEnvOn.WaitForExit(10000)
$onText = if (Test-Path $envOutOn) { [string](Get-Content $envOutOn -Raw) } else { '' }
Check '②a 开隔离 ⇒ 子进程环境 MIR2_RNG_ISOLATE_ATTACK=1' ($onText.Trim() -eq '1') ("cmd 读出=[" + $onText.Trim() + "]")

Check '②b 起完还原 ⇒ 调用方 $env:MIR2_RNG_ISOLATE_ATTACK 未被设置' (-not (Test-Path Env:MIR2_RNG_ISOLATE_ATTACK)) ('has_env=' + (Test-Path Env:MIR2_RNG_ISOLATE_ATTACK))

$envOutOff = Join-Path $Work 'env-iso-off.txt'
$pEnvOff = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo [%MIR2_RNG_ISOLATE_ATTACK%]') `
             -RedirectStandardOutput $envOutOff -IsolateAttack $unset
[void]$pEnvOff.WaitForExit(10000)
$offText = if (Test-Path $envOutOff) { [string](Get-Content $envOutOff -Raw) } else { '' }
# 注：`cmd /c` 对**未定义**的 `%VAR%` 原样保留（不展开为空）⇒ 未注入时的规范形态是字面量；
# 若注入泄漏进调用方环境，这里会读出 `[1]` ⇒ 本项判红。
Check '②c 关/未给 ⇒ 子进程读到未定义（不注入）' ($offText.Trim() -eq '[%MIR2_RNG_ISOLATE_ATTACK%]') ("cmd 读出=[" + $offText.Trim() + "]")

# ---- ③ 真 game-svr：传种子 + 隔离开 + rng 流水 ⇒ 流水含 ISOLATE_ATTACK 40545 ----
# 流水（`MIR2_RNG_LOG`）在此构造层由调用方直接设进本进程环境（子进程继承）；这条不是被测管线本身，
# 只用它证明「隔离开关确实到达生产读取点且解析为 42+0x9E37」。
$gs1 = Join-Path $Work 'gs-isolate'
$trace1 = Join-Path $gs1 'rng-stream.tsv'
New-Item -ItemType Directory -Force -Path $gs1 | Out-Null
$conf1 = Write-Conf $gs1
$log1 = Join-Path $gs1 'stdout.txt'
$err1 = Join-Path $gs1 'stderr.txt'
$env:MIR2_RNG_LOG = $trace1
try {
  $pGs1 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf1) -WorkingDirectory $gs1 `
            -RedirectStandardOutput $log1 -RedirectStandardError $err1 `
            -Seed (Resolve-Mir2Seed -SeedArg '42') -IsolateAttack $switchOn
} finally {
  Remove-Item Env:MIR2_RNG_LOG -ErrorAction SilentlyContinue
}
$gotIso = Wait-FileMatch $trace1 'ISOLATE_ATTACK\s+40545' 25
$text1 = if (Test-Path $trace1) { [string](Get-Content $trace1 -Raw) } else { '' }
Kill-Tree $pGs1.Id
Check '③a 传种子+隔离 ⇒ 真 game-svr 流水含 ISOLATE_ATTACK 40545' $gotIso ("file=" + $trace1)
Check '③b 流水同时含 INJECT_OK 42 + SEED_PROBE（同一种子自证，未被隔离路径吞掉）' (($text1 -match 'INJECT_OK\s+42') -and ($text1 -match 'SEED_PROBE')) ('inject_ok=' + ($text1 -match 'INJECT_OK\s+42') + ' probe=' + ($text1 -match 'SEED_PROBE'))

# ---- ④ 只传种子、不开隔离 ⇒ 正常启动且流水不含 ISOLATE_ATTACK 行（零行为差）----
$gs2 = Join-Path $Work 'gs-no-isolate'
$trace2 = Join-Path $gs2 'rng-stream.tsv'
New-Item -ItemType Directory -Force -Path $gs2 | Out-Null
$conf2 = Write-Conf $gs2
$log2 = Join-Path $gs2 'stdout.txt'
$err2 = Join-Path $gs2 'stderr.txt'
$env:MIR2_RNG_LOG = $trace2
try {
  $pGs2 = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf2) -WorkingDirectory $gs2 `
            -RedirectStandardOutput $log2 -RedirectStandardError $err2 `
            -Seed (Resolve-Mir2Seed -SeedArg '42') -IsolateAttack $unset
} finally {
  Remove-Item Env:MIR2_RNG_LOG -ErrorAction SilentlyContinue
}
$started2 = Wait-LogContains $log2 '游戏服务启动' 25
$gotTrace2 = Wait-FileMatch $trace2 'SEED_PROBE' 15
$text2 = if (Test-Path $trace2) { [string](Get-Content $trace2 -Raw) } else { '' }
Kill-Tree $pGs2.Id
Check '④a 不开隔离 ⇒ 真 game-svr 正常启动 + 流水仍落（只有种子）' ($started2 -and $gotTrace2) ('started=' + $started2 + ' trace=' + $gotTrace2)
Check '④b 不开隔离 ⇒ 流水不含 ISOLATE_ATTACK 行（零行为差）' (-not ($text2 -match 'ISOLATE_ATTACK')) ('has_iso_line=' + ($text2 -match 'ISOLATE_ATTACK'))

# ---- ⑤ Get-IsolateAttackInjectedLine ----
$lineOn = Get-IsolateAttackInjectedLine -IsolateAttack $switchOn
$lineOff = Get-IsolateAttackInjectedLine -IsolateAttack $unset
Check '⑤a 开 ⇒ 行 = RUST_ISOLATE_ATTACK_INJECTED=1' ($lineOn -eq 'RUST_ISOLATE_ATTACK_INJECTED=1') ("line=[" + $lineOn + "]")
Check '⑤b 关/未给 ⇒ 行 = $null（不打）' ($null -eq $lineOff) ("line=[" + $lineOff + "]")

# ---- ⑥ 结构接线：start_rust_stack.ps1 真点源本管线并把开关喂给 game-svr；管线真有注入行 + 交叉守卫 ----
$stackSrc = Get-Content (Join-Path $Here 'start_rust_stack.ps1') -Raw
$wired = ($stackSrc -match 'rust_seed_plumb\.ps1') -and ($stackSrc -match ("game-svr'[^\r\n]*-IsolateAttack " + [regex]::Escape('$isolateInfo')))
Check '⑥a start_rust_stack.ps1 点源管线并把 -IsolateAttack $isolateInfo 传给 game-svr' $wired '结构检查（端到端见证据目录的实栈 run）'
$plumbSrc = Get-Content (Join-Path $Here 'rust_seed_plumb.ps1') -Raw
$hasInjectLine = $plumbSrc -match "\`$env:MIR2_RNG_ISOLATE_ATTACK = '1'"
Check '⑥b 管线文件含 $env:MIR2_RNG_ISOLATE_ATTACK 注入行' $hasInjectLine '删掉该行 ⇒ ②a/③a 必红'
$seed42 = Resolve-Mir2Seed -SeedArg '42'
$guardTt = ((Assert-Mir2IsolateAttackSeed -IsolateAttack $unset -Seed $unset) -eq $true) -and
           ((Assert-Mir2IsolateAttackSeed -IsolateAttack $switchOn -Seed $seed42) -eq $true) -and
           ((Assert-Mir2IsolateAttackSeed -IsolateAttack $switchOn -Seed $unset) -eq $false) -and
           ((Assert-Mir2IsolateAttackSeed -IsolateAttack $envOn -Seed $null) -eq $false)
$callsGuard = $stackSrc -match 'Assert-Mir2IsolateAttackSeed'
Check '⑥c 交叉守卫真值表（开+有种子/开+无种子/未开）+ start_rust_stack 调用本例' ($guardTt -and $callsGuard) ('tt=' + $guardTt + ' called=' + $callsGuard + '；attack_isolation_seed_from_env 开而无种子会 panic ⇒ 开而无关必拦')

# ---- 汇总 ----
$failed = @($script:Checks | Where-Object { -not $_.ok })
Set-Content -Path (Join-Path $OutDir 'checks.txt') -Encoding utf8 -Value (
  ($script:Checks | ForEach-Object { "{0}`t{1}`t{2}" -f $(if ($_.ok) { 'OK' } else { 'FAIL' }), $_.name, $_.detail }) -join "`n"
)
Write-Output ("SUMMARY total={0} ok={1} fail={2}" -f $script:Checks.Count, ($script:Checks.Count - $failed.Count), $failed.Count)
if ($failed.Count -gt 0) {
  Write-Output 'ISOLATE_PLUMB FAIL'
  exit 1
}
Write-Output 'ISOLATE_PLUMB GREEN'
exit 0
