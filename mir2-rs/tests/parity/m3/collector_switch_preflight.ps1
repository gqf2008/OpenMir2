#requires -Version 5
<#
.SYNOPSIS
  collector_switch_preflight.ps1 — M3/M2 窗口运行的**采集开关前置门**（卡
  `openmir2-window-collector-preflight`，A4 主件 + A2 交叉断言）。

.DESCRIPTION
  「跑窗口前先把采集口径点齐」的 fail-closed 门。历史缺陷（假 RED/假 GREEN 源）：
    ① C# 侧 `MIR2_RNG_STATE_LOG` 只在 `-IsolateAttack` 里注入（已由 A1 解耦）；
    ② C# 启动器恒默认 seed 42、Rust 启动器「未传 ⇒ 不注入」（已由 A2 两侧默认 42 对称化）；
    ③ C# 只有 `MIR2_RNG_SITE=1` 才写流水第 5 列，手工起栈忘设 ⇒ 账本 marker 分段白跑（A3 门已 fail-closed）；
    ④ 各采集开关散落在 operator 的 shell 环境里，**没有一处**在起栈/跑对拍前断言「该开的都开了」。
  本门把 ④ 收口：给定本次**运行模式 / 所需开关集合**，逐项断言每个开关在**调用方进程环境**里
  Present 且取值合法；**缺任一项或取值非法 ⇒ 打印缺项/非法清单 + rc≠0（fail-closed，绝不静默起栈）**。

  开关登记表（10 项，`-Required all` / `-Mode all` = 全表）：
    MIR2_RNG_SEED            int   （种子；两侧必须同值）
    MIR2_RNG_LOG             path  （逐次 rng 流水）
    MIR2_RNG_SITE            on    （C# 写流水第 5 列 site 的开关；必须为「开」）
    MIR2_RNG_STATE_LOG       path  （服务端权威逐击状态 dump；目标可用性门两侧都要）
    MIR2_RNG_ISOLATE_ATTACK  on    （攻击路径专用种子流开关；必须为「开」）
    MIR2_RNG_ATTACK_LOG      path  （专用流流水）
    MIR2_RNG_ISOLATE_SITES   path  （C# 静态分流站点 dump）
    MIR2_CLOCK_MS            int   （Rust 可注入时钟起点；cross 断言的 CLOCK_PROBE 锚）
    MIR2_DROPEXP_EXT_FIELDS  on    （掉落/经验扩字段；必须为「开」）
    MIR2_CLIENT_ABILITY_LAYOUT oracle（帧 52/53 记录布局；③a/③b 必须 = 0|false）

  运行模式 → 所需开关集合（`-Mode`；`-Required` 显式 CSV 可覆盖）：
    attack     M3②③ 攻击对拍：SEED/RNG_LOG/RNG_SITE/STATE_LOG/ISOLATE_ATTACK/ATTACK_LOG/ISOLATE_SITES/CLOCK_MS
    drop_exp   M3④ 掉落经验：SEED/RNG_LOG/RNG_SITE/STATE_LOG/DROPEXP_EXT_FIELDS/CLOCK_MS
    acceptance M2③ 出包对账：CLIENT_ABILITY_LAYOUT/SEED/RNG_LOG/RNG_SITE/CLOCK_MS
    all        全 10 项（自检用 / 保守口径）

  「接不了」的开关（登记在案，不静默跳过）：
    * `MIR2_DROPEXP_EXT_FIELDS` 不列入 `attack`（攻击对拍不采集掉落字段）；
    * `MIR2_RNG_ISOLATE_ATTACK` / `MIR2_RNG_ATTACK_LOG` / `MIR2_RNG_ISOLATE_SITES` 不列入 `drop_exp`
      （隔离口径是 M3②③ 攻击专用；掉落场景不构造专用流）；
    * `MIR2_CLIENT_ABILITY_LAYOUT` 不列入 `attack` / `drop_exp`（它是 M2③ 帧布局口径，攻击/掉落对拍
      比的是数值与掉落表，不比对 52/53 字节）。

  接线表（`mir2-rs/tests/parity/m3/` 下 9 个 `run_*pair*.ps1`）：
    run_attack_pair.ps1                        已接（起头 `-Mode attack`；side-run 分支）——②③ 攻击对拍
    drop-exp/run_drop_exp_pair.ps1             已接（real-run 起头 `-Mode drop_exp`）——④ 掉落经验对拍
    run_bag_equip_pair.ps1                     未接：背包/装备是 DB 字段级对拍（scoped_diff），不消费本表
                                               的 rng 采集开关（SEED/LOG/SITE/STATE_LOG/CALC_MS 都不读）
    run_guild_castle_pair.ps1                  未接：公会/城堡是锚点 + DB 对拍，不消费 rng 采集开关
    run_hero_pet_pair.ps1                      未接：同上（锚点 + DB）
    run_npc_shop_pair.ps1                      未接：商店是 DB/动作序列对拍，不消费 rng 采集开关
    run_party_trade_pair.ps1                   未接：同上
    run_quest_pair.ps1                         未接：任务态位/DB 对拍，不消费 rng 采集开关
    run_selloff_pair.ps1                       未接：寄售/拍卖 DB 对拍，不消费 rng 采集开关
    run_pair.ps1                               底层复用件（被上面各 run_*pair 调用），不单点接门
  「未接」= 该场景的判据不依赖本表登记的同种子 rng 采集面（接了会要求 operator export 一堆用不上的开关，
  反而制造假 PREREQ）；**若将来某个 run_*pair 开始消费 rng 流水/状态 dump，必须同步把它接上**。

  A2 交叉断言（同一门里，`-NoSeedCross` 可关）：比对 C# 流水头行 `INJECT_OK <seed>` 与 Rust 侧自证行
  `RUST_SEED_INJECTED=<n>`（回落 `SEED_PROBE ... seed=<n>`），两侧都在 ⇒ **不等即 fail-closed 拒跑**；
  另校 Rust `CLOCK_PROBE origin=<n>` 与 `MIR2_CLOCK_MS` 一致（若两者都在）。单侧运行（另一侧日志不存在）
  ⇒ NOTE 放行（不臆造对侧证据），**绝不**把「单侧缺证据」当「两侧相等」。

  退出码：0 = GREEN；1 = RED（缺项/非法/交叉不符）；2 = PREREQ（模式未知且未给 -Required）。

.EXAMPLE
  # 起栈/跑对拍前（operator 环境里已 export 各开关）：攻击模式点齐
  powershell -ExecutionPolicy Bypass -File collector_switch_preflight.ps1 -Mode attack
.EXAMPLE
  # 显式集合（自检口径）
  powershell -ExecutionPolicy Bypass -File collector_switch_preflight.ps1 -Required MIR2_RNG_SEED,MIR2_RNG_LOG
.EXAMPLE
  # 无栈自检（不起服务/不占端口）
  powershell -ExecutionPolicy Bypass -File collector_switch_preflight.ps1 -SelfCheckNoStack
#>
param(
  [string]$Mode = '',
  [string]$Required = '',
  [string]$CsharpSeedLog = '',
  [string]$RustSeedLog = '',
  [string]$RustStackLogDir = 'E:\tmp\m2-acceptance\stack',
  [int]$ExpectSeed = 42,
  [switch]$NoSeedCross,
  [switch]$SelfCheckNoStack,
  [string]$SelfCheckOut = '',
  [string]$OutDir = ''
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path

# ---------------------------------------------------------------------------
# 开关登记表（唯一真源）：名字 -> 取值类别（int | path | on | onoff | oracle）。
# ---------------------------------------------------------------------------
function Get-CollectorSwitchRegistry {
  return [ordered]@{
    'MIR2_RNG_SEED'              = 'int'
    'MIR2_RNG_LOG'               = 'path'
    'MIR2_RNG_SITE'              = 'on'
    'MIR2_RNG_STATE_LOG'         = 'path'
    'MIR2_RNG_ISOLATE_ATTACK'    = 'on'
    'MIR2_RNG_ATTACK_LOG'        = 'path'
    'MIR2_RNG_ISOLATE_SITES'     = 'path'
    'MIR2_CLOCK_MS'              = 'int'
    'MIR2_DROPEXP_EXT_FIELDS'    = 'on'
    'MIR2_CLIENT_ABILITY_LAYOUT' = 'oracle'
  }
}

# 模式 -> 所需开关集合（有序，便于打印）。
function Get-CollectorModeSet {
  param([Parameter(Mandatory)][string]$Mode)
  switch ($Mode) {
    'attack'     { return @('MIR2_RNG_SEED','MIR2_RNG_LOG','MIR2_RNG_SITE','MIR2_RNG_STATE_LOG','MIR2_RNG_ISOLATE_ATTACK','MIR2_RNG_ATTACK_LOG','MIR2_RNG_ISOLATE_SITES','MIR2_CLOCK_MS') }
    'drop_exp'   { return @('MIR2_RNG_SEED','MIR2_RNG_LOG','MIR2_RNG_SITE','MIR2_RNG_STATE_LOG','MIR2_DROPEXP_EXT_FIELDS','MIR2_CLOCK_MS') }
    'acceptance' { return @('MIR2_CLIENT_ABILITY_LAYOUT','MIR2_RNG_SEED','MIR2_RNG_LOG','MIR2_RNG_SITE','MIR2_CLOCK_MS') }
    'all'        { return @('MIR2_RNG_SEED','MIR2_RNG_LOG','MIR2_RNG_SITE','MIR2_RNG_STATE_LOG','MIR2_RNG_ISOLATE_ATTACK','MIR2_RNG_ATTACK_LOG','MIR2_RNG_ISOLATE_SITES','MIR2_CLOCK_MS','MIR2_DROPEXP_EXT_FIELDS','MIR2_CLIENT_ABILITY_LAYOUT') }
    default      { return $null }
  }
}

# 单个开关取值校验。返回：ok / reason。
#   口径与生产读取端逐字一致（来源：rng-hook/StartupHook.cs、crates/shared/src/rng.rs、
#   bins/game-svr/src/smframe.rs::client_layout_enabled、src/BotSrv/MShare.cs::ParseDropExpExtFieldsEnv）。
function Test-CollectorSwitchValue {
  param(
    [Parameter(Mandatory)][string]$Name,
    [AllowEmptyString()][AllowNull()][string]$Value = '',
    [Parameter(Mandatory)][string]$Kind
  )
  $raw = if ($null -eq $Value) { '' } else { $Value }
  $norm = $raw.Trim().ToLowerInvariant()
  switch ($Kind) {
    'int' {
      if ([string]::IsNullOrEmpty($raw)) { return [pscustomobject]@{ ok = $false; reason = '空/未设（须为十进制 int）' } }
      $v = [long]0
      if (-not [long]::TryParse($raw.Trim(), [ref]$v)) { return [pscustomobject]@{ ok = $false; reason = ("不是合法 int: '{0}'" -f $raw) } }
      return [pscustomobject]@{ ok = $true; reason = '' }
    }
    'path' {
      if ([string]::IsNullOrEmpty($raw)) { return [pscustomobject]@{ ok = $false; reason = '空/未设（须为路径）' } }
      return [pscustomobject]@{ ok = $true; reason = '' }
    }
    'on' {
      if ($norm -in @('1','true','on','yes')) { return [pscustomobject]@{ ok = $true; reason = '' } }
      if ($norm -in @('','0','false','off','no')) { return [pscustomobject]@{ ok = $false; reason = ("须为「开」（1/true/on/yes），实得 '{0}'" -f $raw) } }
      return [pscustomobject]@{ ok = $false; reason = ("取值认不出（既非开也非关）: '{0}'" -f $raw) }
    }
    'onoff' {
      if ($norm -in @('1','true','on','yes','0','false','off','no','')) { return [pscustomobject]@{ ok = $true; reason = '' } }
      return [pscustomobject]@{ ok = $false; reason = ("取值认不出: '{0}'" -f $raw) }
    }
    'oracle' {
      if (@('0','false','FALSE','False') -contains $raw.Trim()) { return [pscustomobject]@{ ok = $true; reason = '' } }
      return [pscustomobject]@{ ok = $false; reason = ("须为 oracle 布局 0|false|FALSE|False（帧 52/53 逐字节对拍），实得 '{0}'" -f $raw) }
    }
    default {
      return [pscustomobject]@{ ok = $false; reason = ("未知取值类别 '{0}'" -f $Kind) }
    }
  }
}

# 逐项断言（纯函数；-Env 注入便于自检）。返回：missing / invalid / checked。
function Test-CollectorSwitches {
  param(
    [Parameter(Mandatory)][hashtable]$Env,
    [Parameter(Mandatory)][string[]]$Required,
    [Parameter(Mandatory)][System.Collections.IDictionary]$Registry
  )
  $missing = New-Object System.Collections.Generic.List[string]
  $invalid = New-Object System.Collections.Generic.List[string]
  $checked = New-Object System.Collections.Generic.List[object]
  foreach ($name in $Required) {
    if (-not $Registry.Contains($name)) {
      $invalid.Add(("{0} [未知开关名]" -f $name))
      continue
    }
    $kind = [string]$Registry[$name]
    $present = $Env.ContainsKey($name) -and (-not [string]::IsNullOrEmpty([string]$Env[$name]))
    if (-not $present) {
      $missing.Add($name)
      $checked.Add([pscustomobject]@{ name = $name; state = 'missing'; value = '' })
      continue
    }
    $val = [string]$Env[$name]
    $vres = Test-CollectorSwitchValue -Name $name -Value $val -Kind $kind
    if ($vres.ok) {
      $checked.Add([pscustomobject]@{ name = $name; state = 'ok'; value = $val })
    } else {
      $invalid.Add(("{0} [值非法：{1}]" -f $name, $vres.reason))
      $checked.Add([pscustomobject]@{ name = $name; state = 'invalid'; value = $val })
    }
  }
  return [pscustomobject]@{
    missing = @($missing.ToArray())
    invalid = @($invalid.ToArray())
    checked = @($checked.ToArray())
  }
}

# ---------------------------------------------------------------------------
# A2 交叉断言：C# 流水头 `INJECT_OK <seed>` vs Rust 自证行 `RUST_SEED_INJECTED`/`SEED_PROBE`。
#   返回：ok / reason / csharp_seed / rust_seed / rust_clock / notes
# ---------------------------------------------------------------------------
function Get-CollectorCsharpSeed {
  param([AllowEmptyString()][string]$Path = '')
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) { return $null }
  $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
  try {
    foreach ($line in [System.IO.File]::ReadLines($Path)) {
      $f = $line -split "`t"
      if ($f.Count -ge 3 -and $f[1] -eq 'INJECT_OK') {
        $v = 0; if ([int]::TryParse($f[2].Trim(), [ref]$v)) { return $v }
      }
    }
  } finally { $ErrorActionPreference = $prev }
  return $null
}

function Get-CollectorRustSeed {
  param([AllowEmptyString()][string]$Path = '')
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) { return $null }
  $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
  try {
    $inj = Select-String -LiteralPath $Path -Pattern 'RUST_SEED_INJECTED=(-?\d+)' -List -ErrorAction SilentlyContinue
    if ($inj) { $v = 0; if ([int]::TryParse($inj.Matches[0].Groups[1].Value, [ref]$v)) { return $v } }
    $probe = Select-String -LiteralPath $Path -Pattern 'seed=(-?\d+).*SEED_PROBE|SEED_PROBE.*seed=(-?\d+)' -List -ErrorAction SilentlyContinue
    if ($probe) {
      $g = $probe.Matches[0].Groups
      $t = if ($g[1].Success) { $g[1].Value } else { $g[2].Value }
      $v = 0; if ([int]::TryParse($t, [ref]$v)) { return $v }
    }
  } finally { $ErrorActionPreference = $prev }
  return $null
}

function Get-CollectorRustClock {
  param([AllowEmptyString()][string]$Path = '')
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) { return $null }
  $prev = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
  try {
    $hit = Select-String -LiteralPath $Path -Pattern 'CLOCK_PROBE\s+origin=(-?\d+)' -List -ErrorAction SilentlyContinue
    if ($hit) { return $hit.Matches[0].Groups[1].Value }
  } finally { $ErrorActionPreference = $prev }
  return $null
}

function Test-CollectorSeedCross {
  param(
    [AllowEmptyString()][string]$CsharpPath = '',
    [AllowEmptyString()][string]$RustPath = '',
    [int]$Expected = 42,
    [AllowEmptyString()][string]$ClockEnv = ''
  )
  $notes = New-Object System.Collections.Generic.List[string]
  $csAvail = (-not [string]::IsNullOrWhiteSpace($CsharpPath)) -and (Test-Path -LiteralPath $CsharpPath)
  $rsAvail = (-not [string]::IsNullOrWhiteSpace($RustPath)) -and (Test-Path -LiteralPath $RustPath)
  $csSeed = $null; $rsSeed = $null; $rsClock = $null
  if ($csAvail) { $csSeed = Get-CollectorCsharpSeed -Path $CsharpPath }
  if ($rsAvail) { $rsSeed = Get-CollectorRustSeed -Path $RustPath; $rsClock = Get-CollectorRustClock -Path $RustPath }

  if (-not $csAvail -and -not $rsAvail) {
    $notes.Add('NOTE: 两侧种子日志都不可达 —— 交叉断言不可判（单侧运行？），放行')
    return [pscustomobject]@{ ok = $true; reason = ''; csharp_seed = $null; rust_seed = $null; rust_clock = $null; notes = @($notes.ToArray()) }
  }
  if (-not $csAvail -or -not $rsAvail) {
    $which = if (-not $csAvail) { 'C# 侧' } else { 'Rust 侧' }
    $notes.Add(("NOTE: 仅单侧可用（缺 {0} 种子日志）—— 交叉断言不可判，放行（不臆造对侧证据）" -f $which))
    $one = if ($csAvail) { $csSeed } else { $rsSeed }
    if ($null -ne $one -and $one -ne $Expected) {
      return [pscustomobject]@{ ok = $false; reason = ("可用侧注入种子 {0} != 期望 {1}" -f $one, $Expected); csharp_seed = $csSeed; rust_seed = $rsSeed; rust_clock = $rsClock; notes = @($notes.ToArray()) }
    }
    return [pscustomobject]@{ ok = $true; reason = ''; csharp_seed = $csSeed; rust_seed = $rsSeed; rust_clock = $rsClock; notes = @($notes.ToArray()) }
  }
  if ($null -eq $csSeed) {
    return [pscustomobject]@{ ok = $false; reason = ("C# 流水 {0} 无 INJECT_OK <seed> 头行（rng-hook 未注入种子？）" -f $CsharpPath); csharp_seed = $null; rust_seed = $rsSeed; rust_clock = $rsClock; notes = @($notes.ToArray()) }
  }
  if ($null -eq $rsSeed) {
    return [pscustomobject]@{ ok = $false; reason = ("Rust 日志 {0} 无 RUST_SEED_INJECTED/SEED_PROBE 自证行（未注入种子？）" -f $RustPath); csharp_seed = $csSeed; rust_seed = $null; rust_clock = $rsClock; notes = @($notes.ToArray()) }
  }
  if ($csSeed -ne $rsSeed) {
    return [pscustomobject]@{ ok = $false; reason = ("两侧种子不等：C# INJECT_OK={0} vs Rust={1}（假 RED/假 GREEN 源，fail-closed 拒跑）" -f $csSeed, $rsSeed); csharp_seed = $csSeed; rust_seed = $rsSeed; rust_clock = $rsClock; notes = @($notes.ToArray()) }
  }
  if ($null -ne $rsClock -and -not [string]::IsNullOrEmpty($ClockEnv)) {
    if ($rsClock.Trim() -ne $ClockEnv.Trim()) {
      return [pscustomobject]@{ ok = $false; reason = ("Rust CLOCK_PROBE origin={0} != 调用方 MIR2_CLOCK_MS={1}" -f $rsClock, $ClockEnv); csharp_seed = $csSeed; rust_seed = $rsSeed; rust_clock = $rsClock; notes = @($notes.ToArray()) }
    }
  } elseif ($null -ne $rsClock) {
    $notes.Add(("NOTE: Rust CLOCK_PROBE origin={0} 但调用方未设 MIR2_CLOCK_MS（时钟未钉）" -f $rsClock))
  }
  return [pscustomobject]@{ ok = $true; reason = ''; csharp_seed = $csSeed; rust_seed = $rsSeed; rust_clock = $rsClock; notes = @($notes.ToArray()) }
}

# ---------------------------------------------------------------------------
# 环境快照（进程环境 -> hashtable）。
# ---------------------------------------------------------------------------
function Get-CollectorEnv {
  $h = @{}
  foreach ($k in [System.Environment]::GetEnvironmentVariables().Keys) { $h[[string]$k] = [string][System.Environment]::GetEnvironmentVariable([string]$k) }
  return $h
}

# ---------------------------------------------------------------------------
# Mode: SelfCheckOut —— 把无栈自检转录由脚本自己以 LF 落盘
# ---------------------------------------------------------------------------
if ($SelfCheckNoStack -and -not [string]::IsNullOrEmpty($SelfCheckOut)) {
  $tmp = Join-Path $env:TEMP ('coll-preflight-' + [guid]::NewGuid().ToString('N') + '.txt')
  $childArgs = @('-NoProfile','-ExecutionPolicy','Bypass','-File',$PSCommandPath,'-SelfCheckNoStack')
  $rc = 0
  try {
    & powershell @childArgs *> $tmp
    $rc = $LASTEXITCODE
    $text = if (Test-Path $tmp) { [string](Get-Content $tmp -Raw) } else { '' }
    $text = $text -replace "`r`n", "`n"
    [System.IO.File]::WriteAllText($SelfCheckOut, $text, (New-Object System.Text.UTF8Encoding($false)))
    Write-Output ("SELFCHECK_OUT rc={0} path={1}" -f $rc, $SelfCheckOut)
  } finally { Remove-Item $tmp -Force -ErrorAction SilentlyContinue }
  exit $rc
}

# ===========================================================================
# Mode: SelfCheckNoStack
# ===========================================================================
if ($SelfCheckNoStack) {
  $script:Pass = 0; $script:Fail = 0
  function Chk([string]$Name, [bool]$Ok, [string]$Detail) {
    $tag = if ($Ok) { 'OK  ' } else { 'FAIL' }
    Write-Output ("  [{0}] {1}" -f $tag, $Name)
    if (-not [string]::IsNullOrEmpty($Detail)) { Write-Output ("        " + $Detail) }
    if ($Ok) { $script:Pass++ } else { $script:Fail++ }
  }
  $reg = Get-CollectorSwitchRegistry
  $allNames = @($reg.Keys)
  $sample = @{
    'MIR2_RNG_SEED' = '42'; 'MIR2_RNG_LOG' = 'X\rng.tsv'; 'MIR2_RNG_SITE' = '1'
    'MIR2_RNG_STATE_LOG' = 'X\state.tsv'; 'MIR2_RNG_ISOLATE_ATTACK' = '1'
    'MIR2_RNG_ATTACK_LOG' = 'X\atk.tsv'; 'MIR2_RNG_ISOLATE_SITES' = 'X\sites.txt'
    'MIR2_CLOCK_MS' = '1700000000000'; 'MIR2_DROPEXP_EXT_FIELDS' = '1'
    'MIR2_CLIENT_ABILITY_LAYOUT' = '0'
  }
  Write-Output '--- collector_switch_preflight 无栈自检（不起服务/不占端口） ---'

  $r = Test-CollectorSwitches -Env $sample -Required $allNames -Registry $reg
  Chk '1 全 10 项齐 => missing=0 invalid=0' (($r.missing.Count -eq 0) -and ($r.invalid.Count -eq 0)) ("missing=[" + ($r.missing -join ',') + "] invalid=[" + ($r.invalid -join ',') + "]")

  foreach ($n in $allNames) {
    $e = @{}; foreach ($k in $sample.Keys) { if ($k -ne $n) { $e[$k] = $sample[$k] } }
    $rr = Test-CollectorSwitches -Env $e -Required $allNames -Registry $reg
    $ok = ($rr.missing.Count -eq 1) -and ($rr.missing[0] -eq $n) -and ($rr.invalid.Count -eq 0)
    Chk ("2 删掉 {0} => 缺项清单正好点名它" -f $n) $ok ("missing=[" + ($rr.missing -join ',') + "]")
  }

  $badCases = @(
    @{ n = 'MIR2_RNG_SEED'; v = 'abc' },
    @{ n = 'MIR2_CLOCK_MS'; v = 'not-int' },
    @{ n = 'MIR2_RNG_SITE'; v = '0' },
    @{ n = 'MIR2_RNG_ISOLATE_ATTACK'; v = 'off' },
    @{ n = 'MIR2_DROPEXP_EXT_FIELDS'; v = 'maybe' },
    @{ n = 'MIR2_CLIENT_ABILITY_LAYOUT'; v = '1' }
  )
  foreach ($c in $badCases) {
    $e = @{}; foreach ($k in $sample.Keys) { $e[$k] = $sample[$k] }
    $e[$c.n] = $c.v
    $rr = Test-CollectorSwitches -Env $e -Required $allNames -Registry $reg
    $ok = ($rr.invalid.Count -eq 1) -and ($rr.invalid[0] -like ($c.n + '*'))
    Chk ("3 值非法 {0}='{1}' => invalid 点名" -f $c.n, $c.v) $ok ("invalid=[" + ($rr.invalid -join ';') + "]")
  }

  Chk '4a -Mode attack 集合' ((@(Get-CollectorModeSet -Mode attack) -join ',') -eq 'MIR2_RNG_SEED,MIR2_RNG_LOG,MIR2_RNG_SITE,MIR2_RNG_STATE_LOG,MIR2_RNG_ISOLATE_ATTACK,MIR2_RNG_ATTACK_LOG,MIR2_RNG_ISOLATE_SITES,MIR2_CLOCK_MS') ('set=' + (@(Get-CollectorModeSet -Mode attack) -join ','))
  Chk '4b -Mode all 集合 = 全 10' (@(Get-CollectorModeSet -Mode all).Count -eq 10) ('n=' + @(Get-CollectorModeSet -Mode all).Count)
  Chk '4c 未知模式 => $null' ($null -eq (Get-CollectorModeSet -Mode nope)) ''

  $tmp = Join-Path $env:TEMP ('coll-cross-' + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  $csLog = Join-Path $tmp 'csharp-rng-stream.tsv'
  $rsLog = Join-Path $tmp 'game-svr.out.log'
  $null = New-Item -ItemType File -Force -Path $csLog
  $null = New-Item -ItemType File -Force -Path $rsLog
  function W([string]$p, [string]$t) { [System.IO.File]::WriteAllText($p, $t, (New-Object System.Text.UTF8Encoding($false))) }
  W $csLog "1`tINJECT_OK`t42`t0`nSEED_PROBE 1 2 3`n"
  W $rsLog "RUST_SEED_INJECTED=42`nRNG 同种子注入：seed=42 参照序列=SEED_PROBE 1 2 3`n"
  $x = Test-CollectorSeedCross -CsharpPath $csLog -RustPath $rsLog -Expected 42
  Chk '5a 两侧种子都 42 => ok' ($x.ok) ($x.reason)
  W $rsLog "RUST_SEED_INJECTED=7`nRNG 同种子注入：seed=7 参照序列=SEED_PROBE 1 2 3`n"
  $x = Test-CollectorSeedCross -CsharpPath $csLog -RustPath $rsLog -Expected 42
  Chk '5b 两侧种子不等（42 vs 7）=> fail' ((-not $x.ok) -and ($x.reason -like '*不等*')) ($x.reason)
  W $rsLog "RNG 未注入（游戏服务启动 INF）`n"
  $x = Test-CollectorSeedCross -CsharpPath $csLog -RustPath $rsLog -Expected 42
  Chk '5c Rust 无自证行 => fail' ((-not $x.ok) -and ($x.reason -like '*RUST_SEED_INJECTED*')) ($x.reason)
  W $rsLog "RUST_SEED_INJECTED=42`nCLOCK_PROBE origin=1700000000000`n"
  $xGood = Test-CollectorSeedCross -CsharpPath $csLog -RustPath $rsLog -Expected 42 -ClockEnv '1700000000000'
  Chk '5d CLOCK_PROBE 与 MIR2_CLOCK_MS 一致 => ok' ($xGood.ok) ($xGood.reason)
  $xBad = Test-CollectorSeedCross -CsharpPath $csLog -RustPath $rsLog -Expected 42 -ClockEnv '1'
  Chk '5e CLOCK_PROBE 与 MIR2_CLOCK_MS 不一致 => fail' ((-not $xBad.ok) -and ($xBad.reason -like '*CLOCK_PROBE*')) ($xBad.reason)
  $xOne = Test-CollectorSeedCross -CsharpPath $csLog -RustPath (Join-Path $tmp 'nope.log') -Expected 42
  Chk '5f 单侧可用 => NOTE 放行（ok）' ($xOne.ok) ($xOne.notes -join ' | ')
  Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue

  Write-Output '  --- 6 端到端（子进程 env）---'
  $prevEnv = @{}
  foreach ($n in $allNames) { $prevEnv[$n] = [System.Environment]::GetEnvironmentVariable($n) }
  try {
    foreach ($n in $allNames) { [System.Environment]::SetEnvironmentVariable($n, [string]$sample[$n]) }
    & powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -Mode all -NoSeedCross *> $null
    $rcAll = $LASTEXITCODE
    Chk '6a 全齐子进程 => rc=0' ($rcAll -eq 0) ('rc=' + $rcAll)
    [System.Environment]::SetEnvironmentVariable('MIR2_RNG_SITE', $null)
    $out = (& powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -Mode all -NoSeedCross 2>&1 | Out-String)
    $rcOne = $LASTEXITCODE
    Chk '6b 删 MIR2_RNG_SITE 子进程 => rc=1 且点名' (($rcOne -eq 1) -and ($out -match 'MISSING\s+MIR2_RNG_SITE')) ('rc=' + $rcOne)
  } finally {
    foreach ($n in $allNames) { [System.Environment]::SetEnvironmentVariable($n, $prevEnv[$n]) }
  }

  Write-Output ("COLLECTOR_PREFLIGHT_SELFTEST pass={0} fail={1}" -f $script:Pass, $script:Fail)
  if ($script:Fail -gt 0) { Write-Output 'SELFTEST_RED'; exit 1 }
  Write-Output 'SELFTEST_RED GREEN: collector_switch_preflight 无栈自检全过'
  exit 0
}

# ===========================================================================
# Mode: 门（默认）
# ===========================================================================
$Registry = Get-CollectorSwitchRegistry

if (-not [string]::IsNullOrEmpty($Required)) {
  $requiredList = @($Required.Split(',') | ForEach-Object { $_.Trim() } | Where-Object { $_ -ne '' })
} elseif (-not [string]::IsNullOrEmpty($Mode)) {
  $requiredList = Get-CollectorModeSet -Mode $Mode
  if ($null -eq $requiredList) {
    Write-Output ("PREREQ_MISSING: 未知 -Mode '{0}'（可 attack/drop_exp/acceptance/all；或用 -Required <csv>）" -f $Mode)
    exit 2
  }
} else {
  Write-Output 'PREREQ_MISSING: 需 -Mode <attack|drop_exp|acceptance|all> 或 -Required <csv>'
  exit 2
}

$envSnapshot = Get-CollectorEnv
$res = Test-CollectorSwitches -Env $envSnapshot -Required $requiredList -Registry $Registry
$label = if (-not [string]::IsNullOrEmpty($Mode)) { $Mode } else { '(explicit)' }
foreach ($c in $res.checked) {
  Write-Output ("CHECK {0,-28} {1,-7} {2}" -f $c.name, $c.state, $c.value)
}
$rc = 0
if ($res.missing.Count -gt 0) {
  foreach ($m in $res.missing) { Write-Output ("MISSING {0}" -f $m) }
  $rc = 1
}
if ($res.invalid.Count -gt 0) {
  foreach ($i in $res.invalid) { Write-Output ("INVALID {0}" -f $i) }
  $rc = 1
}

if (-not $NoSeedCross) {
  if ([string]::IsNullOrEmpty($CsharpSeedLog) -and -not [string]::IsNullOrEmpty($env:MIR2_RNG_LOG)) { $CsharpSeedLog = $env:MIR2_RNG_LOG }
  if ([string]::IsNullOrEmpty($RustSeedLog)) { $RustSeedLog = Join-Path $RustStackLogDir 'game-svr.out.log' }
  $x = Test-CollectorSeedCross -CsharpPath $CsharpSeedLog -RustPath $RustSeedLog -Expected $ExpectSeed -ClockEnv ([string]$env:MIR2_CLOCK_MS)
  foreach ($n in @($x.notes)) { Write-Output $n }
  Write-Output ("SEED_CROSS csharp={0} rust={1} clock={2}" -f $x.csharp_seed, $x.rust_seed, $x.rust_clock)
  if (-not $x.ok) {
    Write-Output ("RED: 种子交叉断言失败 —— {0}" -f $x.reason)
    $rc = 1
  }
}

if ($rc -ne 0) {
  Write-Output ("COLLECTOR_PREFLIGHT FAIL mode={0} required={1} missing={2} invalid={3}" -f $label, $requiredList.Count, $res.missing.Count, $res.invalid.Count)
  exit 1
}
Write-Output ("COLLECTOR_PREFLIGHT GREEN mode={0} required={1}" -f $label, $requiredList.Count)
exit 0
