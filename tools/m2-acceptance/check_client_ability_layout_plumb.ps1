#requires -Version 5
<#
.SYNOPSIS
  check_client_ability_layout_plumb.ps1 — `start_rust_stack.ps1` 的 `MIR2_CLIENT_ABILITY_LAYOUT`
  帧 52/53 记录布局管线的**可判红/绿**判据（卡 `openmir2-client-e2e-enter-gaps` 片 ㊷）。

.DESCRIPTION
  冻结客户端 `MirClient@c61d912d` 在 `Source/Common/Grobal2.pas:39` 写死 `HIGHHP = True`：
  帧 52 的 `TAbility` 是 **58B**、帧 53 的 `SM_HEALTHSPELLCHANGED` 从**尾**解 12B `TCharDesc`
  （`Source/MirClient/ClMain.pas:10220/10225`）⇒ 生产默认必须是**客户端布局**（否则 HUD `0/0`、跑不动，
  根因与真机实测见 `tests/parity/world/client-e2e-enter-gaps-20261013/61-*` / `183-*`）。

  但 M2 验收③（`s2c_diff.py` vs 金标准 `c-line-baseline-c3-20261010.jsonl`）比的是 `ident=52/53` 的
  `body_len/body_sha256/raw_hex`：金标准是 **oracle 布局**（52 `body_len=40`、53 `body_len=0`）。
  ⇒ ③a/③b 的 Rust 半**必须**以 `MIR2_CLIENT_ABILITY_LAYOUT=0` 起栈；而 `start_rust_stack.ps1` 此前
  **没有这条管线** ⇒ 只有 operator 手工在起栈 shell 里设过变量才切到 oracle 布局，否则 52/53 必红
  （与数值无关）。本脚本用构造输入把这条管线（`rust_seed_plumb.ps1`）的每条口径都跑一遍。

  检查项（**全部用临时目录 + 空闲端口；不占 5000/7000/7100/7200**）：
    ① `Resolve-Mir2ClientAbilityLayout`：未设/空串 ⇒ `Present=$false`；`0`/`FALSE` ⇒ oracle；
       `1`/`True` ⇒ client；`off`/`yes`/垃圾 ⇒ **抛错**（fail-closed；不接受 on/off/yes/no —— Rust 读端
       `smframe::client_layout_enabled()` 只把 `0|false|FALSE|False` 当 oracle）。
    ② 注入到位：`Start-SeededProcess -ClientAbilityLayout <oracle>` 起 `cmd /c echo [%VAR%]` ⇒ 读出原文；
       未指定 ⇒ 不注入；起完还原调用方环境。删掉注入行 ⇒ 本项必红。
    ③ 真 game-svr + 起栈自证行：`-ClientAbilityLayout 0` ⇒ 启动日志含
       `CLIENT_ABILITY_LAYOUT layout=oracle-40`；`1` ⇒ `layout=client-58`；**不传** ⇒ `layout=client-58`
       （生产默认）。三向都要对得上，证明环境确实到达生产读取点。
    ④ `Get-ClientAbilityLayoutInjectedLine` 形态：oracle ⇒ `RUST_CLIENT_ABILITY_LAYOUT_INJECTED=0`；
       client ⇒ `=1`；未给 ⇒ `$null`。
    ⑤ 结构接线：`start_rust_stack.ps1` 点源本管线、把 `-ClientAbilityLayout $layoutInfo` 传给 game-svr、
       管线文件确有 `$env:MIR2_CLIENT_ABILITY_LAYOUT` 注入行。

  任何一条不符 ⇒ 打印 `CLIENT_ABILITY_LAYOUT_PLUMB FAIL` 并 exit 1；全绿 ⇒ `... GREEN` exit 0。
  缺 game-svr.exe ⇒ `PREREQ_MISSING` exit 2（**不静默通过**）。

.PARAMETER GameSvrExe
  被测的 `game-svr.exe`。默认按隔离 target（`$env:CARGO_TARGET_DIR\debug\game-svr.exe`），
  否则回落仓库默认 target（`mir2-rs\target\debug\game-svr.exe`）。

.PARAMETER OutDir
  夹具/原始件目录（默认 `E:\tmp\m2-acceptance\client-ability-layout-plumb-<时间戳>`）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_client_ability_layout_plumb.ps1 `
    -GameSvrExe E:\tmp\m2svr-target\debug\game-svr.exe
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
  $OutDir = Join-Path 'E:\tmp\m2-acceptance' ('client-ability-layout-plumb-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
}
$Work = Join-Path $OutDir 'work'
# **干净起点**：同一 `-OutDir` 复用时会残留上一轮日志 ⇒ `Wait-LogContains` 可能读到陈旧证据而**假绿**。
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

function Kill-Tree([int]$ProcessId) {
  if ($ProcessId -le 0) { return }
  & taskkill /T /F /PID $ProcessId > $null 2>&1
}

# 保证本脚本自身的用例不被调用方 shell 的残留污染。
foreach ($v in @('MIR2_CLIENT_ABILITY_LAYOUT', 'MIR2_RNG_SEED', 'MIR2_RNG_ISOLATE_ATTACK', 'MIR2_RNG_STATE_LOG', 'MIR2_RNG_LOG')) {
  if (Test-Path ("Env:" + $v)) { Remove-Item ("Env:" + $v) -ErrorAction SilentlyContinue }
}

Write-Output ("CLIENT_ABILITY_LAYOUT_PLUMB 判据：game-svr=" + $GameSvrExe + "  evidence=" + $OutDir)

# ---- ① Resolve-Mir2ClientAbilityLayout ----
$unset = Resolve-Mir2ClientAbilityLayout -LayoutArg '' -EnvValue ''
Check '①a 未设/空串 ⇒ Present=$false（零行为差）' ((-not $unset.Present) -and ($null -eq $unset.Oracle)) ("Present=" + $unset.Present)

$oracle0 = Resolve-Mir2ClientAbilityLayout -LayoutArg '0' -EnvValue ''
Check '①b 显式 0 ⇒ oracle（Semantic=0）' (($oracle0.Present) -and ($oracle0.Oracle -eq $true) -and ($oracle0.Semantic -eq '0')) ("Oracle=" + $oracle0.Oracle + " Semantic=" + $oracle0.Semantic)

$oracleFalse = Resolve-Mir2ClientAbilityLayout -LayoutArg '' -EnvValue 'FALSE'
Check '①c 环境值 FALSE ⇒ oracle（原文大小写变体）' (($oracleFalse.Present) -and ($oracleFalse.Oracle -eq $true)) ("Oracle=" + $oracleFalse.Oracle)

$client1 = Resolve-Mir2ClientAbilityLayout -LayoutArg '1' -EnvValue ''
Check '①d 显式 1 ⇒ client（Semantic=1）' (($client1.Present) -and ($client1.Oracle -eq $false) -and ($client1.Semantic -eq '1')) ("Oracle=" + $client1.Oracle + " Semantic=" + $client1.Semantic)

$clientTrue = Resolve-Mir2ClientAbilityLayout -LayoutArg '' -EnvValue 'True'
Check '①e 环境值 True ⇒ client' (($clientTrue.Present) -and ($clientTrue.Oracle -eq $false)) ("Oracle=" + $clientTrue.Oracle)

$offThrew = $false
try { [void](Resolve-Mir2ClientAbilityLayout -LayoutArg 'off' -EnvValue '') } catch { $offThrew = $true }
Check '①f off ⇒ 抛错 fail-closed（Rust 读端只认 0|false 为 oracle，不许起栈器口径发散）' $offThrew '不接受 on/off/yes/no'

$junkThrew = $false
try { [void](Resolve-Mir2ClientAbilityLayout -LayoutArg 'maybe' -EnvValue '') } catch { $junkThrew = $true }
Check '①g 垃圾值 ⇒ 抛错 fail-closed' $junkThrew '既不静默当开也不静默当关'

# ---- ② 注入到位（cmd 子进程读环境；删掉注入行 ⇒ 本项必红）----
$envOutOracle = Join-Path $Work 'env-layout-oracle.txt'
$pEnvOracle = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo [%MIR2_CLIENT_ABILITY_LAYOUT%]') `
                -RedirectStandardOutput $envOutOracle -ClientAbilityLayout $oracle0
[void]$pEnvOracle.WaitForExit(10000)
$oracleText = if (Test-Path $envOutOracle) { [string](Get-Content $envOutOracle -Raw) } else { '' }
Check '②a oracle ⇒ 子进程环境 MIR2_CLIENT_ABILITY_LAYOUT=0（原文）' ($oracleText.Trim() -eq '[0]') ("cmd 读出=[" + $oracleText.Trim() + "]")

Check '②b 起完还原 ⇒ 调用方 $env:MIR2_CLIENT_ABILITY_LAYOUT 未被设置' (-not (Test-Path Env:MIR2_CLIENT_ABILITY_LAYOUT)) ('has_env=' + (Test-Path Env:MIR2_CLIENT_ABILITY_LAYOUT))

$envOutClient = Join-Path $Work 'env-layout-client.txt'
$pEnvClient = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo [%MIR2_CLIENT_ABILITY_LAYOUT%]') `
                -RedirectStandardOutput $envOutClient -ClientAbilityLayout $clientTrue
[void]$pEnvClient.WaitForExit(10000)
$clientText = if (Test-Path $envOutClient) { [string](Get-Content $envOutClient -Raw) } else { '' }
Check '②c client ⇒ 子进程环境注入原文 True（与 Rust 读端口径逐字一致）' ($clientText.Trim() -eq '[True]') ("cmd 读出=[" + $clientText.Trim() + "]")

$envOutOff = Join-Path $Work 'env-layout-off.txt'
$pEnvOff = Start-SeededProcess -FilePath $env:ComSpec -ArgumentList @('/c', 'echo [%MIR2_CLIENT_ABILITY_LAYOUT%]') `
             -RedirectStandardOutput $envOutOff -ClientAbilityLayout $unset
[void]$pEnvOff.WaitForExit(10000)
$offText = if (Test-Path $envOutOff) { [string](Get-Content $envOutOff -Raw) } else { '' }
# 注：`cmd /c` 对**未定义**的 `%VAR%` 原样保留（不展开为空）⇒ 未注入时的规范形态是字面量。
Check '②d 未给 ⇒ 子进程读到未定义（不注入；生产默认冻结客户端）' ($offText.Trim() -eq '[%MIR2_CLIENT_ABILITY_LAYOUT%]') ("cmd 读出=[" + $offText.Trim() + "]")

# ---- ③ 真 game-svr：三向起栈自证行（证明环境到达生产读取点）----
function Start-Gs([string]$Tag, [psobject]$Layout) {
  $dir = Join-Path $Work $Tag
  New-Item -ItemType Directory -Force -Path $dir | Out-Null
  $conf = Write-Conf $dir
  $log = Join-Path $dir 'stdout.txt'
  $err = Join-Path $dir 'stderr.txt'
  $p = Start-SeededProcess -FilePath $GameSvrExe -ArgumentList @($conf) -WorkingDirectory $dir `
         -RedirectStandardOutput $log -RedirectStandardError $err -ClientAbilityLayout $Layout
  return [pscustomobject]@{ p = $p; log = $log; err = $err }
}

$gsOracle = Start-Gs 'gs-oracle' $oracle0
$okOracle = Wait-LogContains $gsOracle.log 'CLIENT_ABILITY_LAYOUT layout=oracle-40' 30
$startedOracle = Wait-LogContains $gsOracle.log '游戏服务启动' 15
Kill-Tree $gsOracle.p.Id
Check '③a -ClientAbilityLayout 0 ⇒ 真 game-svr 打 layout=oracle-40（③ 逐字节对拍口径）' $okOracle ("log=" + $gsOracle.log)
Check '③b 同时打「游戏服务启动」⇒ 栈真起到监听步（不是早期退出）' $startedOracle ("log=" + $gsOracle.log)

$gsClient = Start-Gs 'gs-client' $client1
$okClient = Wait-LogContains $gsClient.log 'CLIENT_ABILITY_LAYOUT layout=client-58' 30
Kill-Tree $gsClient.p.Id
Check '③c -ClientAbilityLayout 1 ⇒ 真 game-svr 打 layout=client-58（冻结客户端口径）' $okClient ("log=" + $gsClient.log)

$gsDefault = Start-Gs 'gs-default' $unset
$okDefault = Wait-LogContains $gsDefault.log 'CLIENT_ABILITY_LAYOUT layout=client-58' 30
Kill-Tree $gsDefault.p.Id
Check '③d 不传 ⇒ 生产默认仍 layout=client-58（真客户端可玩，零行为差）' $okDefault ("log=" + $gsDefault.log)

# ---- ④ Get-ClientAbilityLayoutInjectedLine ----
$lineOracle = Get-ClientAbilityLayoutInjectedLine -ClientAbilityLayout $oracle0
$lineFalse = Get-ClientAbilityLayoutInjectedLine -ClientAbilityLayout $oracleFalse
$lineClient = Get-ClientAbilityLayoutInjectedLine -ClientAbilityLayout $client1
$lineUnset = Get-ClientAbilityLayoutInjectedLine -ClientAbilityLayout $unset
Check '④a oracle ⇒ 行 = RUST_CLIENT_ABILITY_LAYOUT_INJECTED=0（原文 FALSE 也归一为 0）' (($lineOracle -eq 'RUST_CLIENT_ABILITY_LAYOUT_INJECTED=0') -and ($lineFalse -eq 'RUST_CLIENT_ABILITY_LAYOUT_INJECTED=0')) ("oracle=[" + $lineOracle + "] FALSE=[" + $lineFalse + "]")
Check '④b client ⇒ 行 = RUST_CLIENT_ABILITY_LAYOUT_INJECTED=1' ($lineClient -eq 'RUST_CLIENT_ABILITY_LAYOUT_INJECTED=1') ("line=[" + $lineClient + "]")
Check '④c 未给 ⇒ 行 = $null（不打）' ($null -eq $lineUnset) ("line=[" + $lineUnset + "]")

# ---- ⑤ 结构接线：start_rust_stack.ps1 真点源本管线并把布局喂给 game-svr；管线真有注入行 ----
$stackSrc = Get-Content (Join-Path $Here 'start_rust_stack.ps1') -Raw
$wired = ($stackSrc -match 'rust_seed_plumb\.ps1') -and ($stackSrc -match ("game-svr'[^\r\n]*-ClientAbilityLayout " + [regex]::Escape('$layoutInfo')))
Check '⑤a start_rust_stack.ps1 点源管线并把 -ClientAbilityLayout $layoutInfo 传给 game-svr' $wired '结构检查（端到端见 ③ 的三个真 game-svr 起栈）'
$plumbSrc = Get-Content (Join-Path $Here 'rust_seed_plumb.ps1') -Raw
$hasInjectLine = $plumbSrc -match "\`$env:MIR2_CLIENT_ABILITY_LAYOUT = \[string\]\`$ClientAbilityLayout\.Text"
Check '⑤b 管线文件含 $env:MIR2_CLIENT_ABILITY_LAYOUT 注入行' $hasInjectLine '删掉该行 ⇒ ②a/③a 必红'

# ---- 汇总 ----
$failed = @($script:Checks | Where-Object { -not $_.ok })
Set-Content -Path (Join-Path $OutDir 'checks.txt') -Encoding utf8 -Value (
  ($script:Checks | ForEach-Object { "{0}`t{1}`t{2}" -f $(if ($_.ok) { 'OK' } else { 'FAIL' }), $_.name, $_.detail }) -join "`n"
)
Write-Output ("SUMMARY total={0} ok={1} fail={2}" -f $script:Checks.Count, ($script:Checks.Count - $failed.Count), $failed.Count)
if ($failed.Count -gt 0) {
  Write-Output 'CLIENT_ABILITY_LAYOUT_PLUMB FAIL'
  exit 1
}
Write-Output 'CLIENT_ABILITY_LAYOUT_PLUMB GREEN'
exit 0
