#requires -Version 5
<#
.SYNOPSIS
  rust_seed_plumb.ps1 — `start_rust_stack.ps1` 起 game-svr 时的**环境变量注入管线**。
  目前承载三条：`MIR2_RNG_SEED`（种子，卡 `openmir2-m3-rust-stack-seed-plumb`）、
  `MIR2_RNG_STATE_LOG`（服务端权威逐击状态 dump，卡 `openmir2-m2-rust-state-log-plumb`）与
  `MIR2_RNG_LOG`（逐次 rng 流水 / site-order 账本，卡 `openmir2-m2-rust-rng-log-plumb`）。

.DESCRIPTION
  ②③ 真机同种子攻击对拍要求两岸同种子：
  - **C# 半**：.NET Startup Hook 读 `MIR2_RNG_SEED`（`mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`）；
  - **Rust 半**：`bins/game-svr` 生产路径读同一个变数（`bins/game-svr/src/seed.rs`，T-30）。

  但启动器此前**没有管线**：`start_rust_stack.ps1` 用 `Start-Process` 起 `game-svr.exe`、只继承调用方环境
  ⇒ 只有 operator 手工在起栈那个 shell 里设过变量才同种子；否则 C# 注入了、Rust 没注入（T-30 是 fail-soft：
  未设 ⇒ 默认源）⇒ 得到**不可比的伤害序列**且看不出原因。

  本文件把管线口径收敛成**三个纯/近似纯函数**，供 `start_rust_stack.ps1` 与 `check_seed_plumb.ps1` 共用：
    - `Resolve-Mir2Seed`      解析 `-Seed` / `MIR2_RNG_SEED`（照 C# `string.IsNullOrEmpty` + `int.TryParse`）；
    - `Get-SeedInjectedLine`  生成可判据的一行 `RUST_SEED_INJECTED=<n>`（未传种子 ⇒ `$null`，不打）；
    - `Start-SeededProcess`   起子进程时把种子注入其环境（起完还原），未传种子时**逐字节等于**原来的 `Start-Process`。

  三条硬约束（与卡 DoD 一一对应）：
    ① 不传种子（`-Seed` 空且 `MIR2_RNG_SEED` 未设/空）⇒ 既不注入、也不打该行 ⇒ 与现状逐字节一致；
    ② 传种子 ⇒ 子进程环境确为 `MIR2_RNG_SEED=<原文>`，且打印 `RUST_SEED_INJECTED=<解析值>`；
    ③ 非法值 ⇒ **fail-closed**（`Resolve-Mir2Seed` 抛错，调用方判红并退出），绝不静默降级
       （同 C# 钩子的 `Fail(...) => Environment.Exit(97)`）。

  `MIR2_RNG_STATE_LOG`（卡 `openmir2-m2-rust-state-log-plumb`）——与 `MIR2_RNG_SEED` 完全同构的一条：
  - **C# 半**：`mir2-rs/tests/parity/m3/start-csharp-hook.ps1 -StateLog` 写 `MIR2_RNG_STATE_LOG`，影子
    GameSvr 的 StartupHook 每个玩家攻击起点落一行 `SERVER_STATE`；
  - **Rust 半**：`bins/game-svr/src/state_log.rs` 读同一个变量，落同形一行（卡 `openmir2-m2-rust-server-state-dump`）；
  - 启动器此前同样**没有管线** ⇒ 只有 operator 手工在起栈 shell 里设过变量才会落盘，否则
    `run_attack_pair.ps1 -Side rust` 拿不到 Rust 侧 dump、目标可用性门只保护 C# 一半。

  三条硬约束同上：① 未指定 ⇒ 不注入、不打 `RUST_STATE_LOG_INJECTED` 行；② 指定 ⇒ 子进程环境确为该路径，
  且打印可判据一行；③ 路径父目录不存在 ⇒ **fail-closed**（`Resolve-Mir2StateLog` 抛错，调用方判红退出），
  不静默起一个永远不落 dump 的栈。

  `MIR2_RNG_LOG`（卡 `openmir2-m2-rust-rng-log-plumb`）——与上面两条完全同构的第三条（逐次 rng 流水）：
  - **C# 半**：`mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs` 读 `MIR2_RNG_LOG` 落 `rng-stream.tsv`
    （`seq \t kind \t max/arg \t result \t site`，头两行 `INJECT_OK <seed>` + `SEED_PROBE`）；
  - **Rust 半**：`crates/shared/src/rng_trace.rs` 的 `TRACE_ENV` = `MIR2_RNG_LOG`，由
    `RandomNumber::with_seed` → `enable_from_env()` 读同一个变量开流水（旁路仪器，不改值序列）；
  - 启动器此前同样**没有管线** ⇒ 只有 operator 手工在起栈 shell 里设过变量才有 Rust 流水，
    否则两侧账本（`attack_rng_ledger.py` / `rng_ledger_diff.py` 的 site/order diff）拿不到 Rust 半。
  三条硬约束同上：① 未指定 ⇒ 不注入、不打 `RUST_RNG_LOG_INJECTED` 行；② 指定 ⇒ 子进程环境确为该路径，
  且打印可判据一行；③ 路径父目录不存在 ⇒ **fail-closed**（`Resolve-Mir2RngLog` 抛错，调用方判红退出）。
  额外的**交叉 fail-closed**（由 `start_rust_stack.ps1` 执行，见其注释）：`MIR2_RNG_LOG` 已解析但
  `MIR2_RNG_SEED` 未解析 ⇒ 拒绝起栈——`rng_trace::enable_from_env` 在种子缺失时**关掉**流水并只打 stderr
  ⇒ 若不拦，栈看起来正常却永远不落 rng-stream，正是本管线要消灭的静默缺口。

  只读约束：本文件不自己起栈、不占端口；进程级用例由 `check_seed_plumb.ps1` / `check_state_log_plumb.ps1` 驱动。
#>

function Resolve-Mir2Seed {
  [CmdletBinding()]
  param(
    # 显式种子（`start_rust_stack.ps1 -Seed <int>` 或 `check_seed_plumb.ps1 -Seed <int>`）。空串 = 未给。
    [string]$SeedArg = '',
    # 调用方环境里的 `MIR2_RNG_SEED` 值（`start_rust_stack.ps1` 传 `$env:MIR2_RNG_SEED`）。
    [string]$EnvValue = ''
  )
  # 优先级：显式 `-Seed` > 环境变量（两者都给时显式胜，operator 直觉一致）。
  $text = if (-not [string]::IsNullOrEmpty($SeedArg)) { $SeedArg } else { $EnvValue }
  if ([string]::IsNullOrEmpty($text)) {
    # C# `String.IsNullOrEmpty`：未设与空串同义 ⇒ 不启用。
    return [pscustomobject]@{ Present = $false; Text = ''; Value = $null }
  }
  # C# `int.TryParse`（十进制、允许前后空白与前导正负号）；失败 ⇒ 抛错（fail-closed，不静默降级）。
  $parsed = 0
  if (-not [int]::TryParse($text, [ref]$parsed)) {
    throw "MIR2_RNG_SEED/-Seed 不是合法 int: $text"
  }
  return [pscustomobject]@{ Present = $true; Text = $text; Value = $parsed }
}

function Get-SeedInjectedLine {
  [CmdletBinding()]
  param(
    # `Resolve-Mir2Seed` 的返回值；`$null` 或 `Present=$false` ⇒ 不打该行（返回 `$null`）。
    [psobject]$Seed = $null
  )
  if (($null -ne $Seed) -and ($Seed.Present -eq $true)) {
    return ("RUST_SEED_INJECTED={0}" -f $Seed.Value)
  }
  return $null
}

function Resolve-Mir2StateLog {
  [CmdletBinding()]
  param(
    # 显式路径（`start_rust_stack.ps1 -StateLog <path>` 或 `check_state_log_plumb.ps1 -StateLog <path>`）。空串 = 未给。
    [string]$StateLogArg = '',
    # 调用方环境里的 `MIR2_RNG_STATE_LOG` 值（`start_rust_stack.ps1` 传 `$env:MIR2_RNG_STATE_LOG`）。
    [string]$EnvValue = ''
  )
  # 优先级：显式 `-StateLog` > 环境变量（与 `Resolve-Mir2Seed` 同口径）。
  $text = if (-not [string]::IsNullOrEmpty($StateLogArg)) { $StateLogArg } else { $EnvValue }
  if ([string]::IsNullOrEmpty($text)) {
    # C# `String.IsNullOrEmpty`：未设与空串同义 ⇒ 不启用。
    return [pscustomobject]@{ Present = $false; Path = '' }
  }
  # fail-closed：父目录必须已存在（否则 game-svr 静默关闭 dump、栈看起来正常却永远不落盘）。
  # 无目录分量（相对文件名）⇒ `Split-Path -Parent` 为空，跳过（等同当前工作目录）。
  $parent = Split-Path -Parent $text
  if (-not [string]::IsNullOrEmpty($parent) -and -not (Test-Path -LiteralPath $parent -PathType Container)) {
    throw "MIR2_RNG_STATE_LOG/-StateLog 的父目录不存在: $parent"
  }
  return [pscustomobject]@{ Present = $true; Path = $text }
}

function Get-StateLogInjectedLine {
  [CmdletBinding()]
  param(
    # `Resolve-Mir2StateLog` 的返回值；`$null` 或 `Present=$false` ⇒ 不打该行（返回 `$null`）。
    [psobject]$StateLog = $null
  )
  if (($null -ne $StateLog) -and ($StateLog.Present -eq $true)) {
    return ("RUST_STATE_LOG_INJECTED={0}" -f $StateLog.Path)
  }
  return $null
}

function Resolve-Mir2RngLog {
  [CmdletBinding()]
  param(
    # 显式路径（`start_rust_stack.ps1 -RngLog <path>` 或 `check_rng_log_plumb.ps1 -RngLog <path>`）。空串 = 未给。
    [string]$RngLogArg = '',
    # 调用方环境里的 `MIR2_RNG_LOG` 值（`start_rust_stack.ps1` 传 `$env:MIR2_RNG_LOG`）。
    [string]$EnvValue = ''
  )
  # 优先级：显式 `-RngLog` > 环境变量（与 `Resolve-Mir2Seed` / `Resolve-Mir2StateLog` 同口径）。
  $text = if (-not [string]::IsNullOrEmpty($RngLogArg)) { $RngLogArg } else { $EnvValue }
  if ([string]::IsNullOrEmpty($text)) {
    # C# `String.IsNullOrEmpty`：未设与空串同义 ⇒ 不启用。
    return [pscustomobject]@{ Present = $false; Path = '' }
  }
  # fail-closed：父目录必须已存在（否则 rng_trace 打不开文件、只在 stderr 报错，栈看起来正常却永远不落流水）。
  # 无目录分量（相对文件名）⇒ `Split-Path -Parent` 为空，跳过（等同当前工作目录）。
  $parent = Split-Path -Parent $text
  if (-not [string]::IsNullOrEmpty($parent) -and -not (Test-Path -LiteralPath $parent -PathType Container)) {
    throw "MIR2_RNG_LOG/-RngLog 的父目录不存在: $parent"
  }
  return [pscustomobject]@{ Present = $true; Path = $text }
}

function Get-RngLogInjectedLine {
  [CmdletBinding()]
  param(
    # `Resolve-Mir2RngLog` 的返回值；`$null` 或 `Present=$false` ⇒ 不打该行（返回 `$null`）。
    [psobject]$RngLog = $null
  )
  if (($null -ne $RngLog) -and ($RngLog.Present -eq $true)) {
    return ("RUST_RNG_LOG_INJECTED={0}" -f $RngLog.Path)
  }
  return $null
}

function Assert-Mir2RngLogSeed {
  <#
    交叉 fail-closed 守卫：`MIR2_RNG_LOG` 已解析但种子 `MIR2_RNG_SEED` 未解析 ⇒ 返回 `$false`
    （调用方 `start_rust_stack.ps1` 据此打印 RED 并 exit 1）。

    为什么是错：`crates/shared/src/rng_trace.rs::enable_from_env` 在 `MIR2_RNG_SEED` 未设/为空时
    **关闭流水**（只打 stderr），且 `RandomNumber::with_seed` 才调它 ⇒ 若无种子，栈看起来正常却永远不
    落 rng-stream。这种“静默空转”必须在起栈前拦住（与 `-Seed` 非法值、状态 dump 父目录不存在同口径）。

    真值表：RngLog 未设 ⇒ `$true`（与现状一致）；RngLog 设且 Seed 设 ⇒ `$true`；RngLog 设而 Seed 未设 ⇒ `$false`。
  #>
  [CmdletBinding()]
  param(
    [psobject]$RngLog = $null,
    [psobject]$Seed = $null
  )
  $rngOn = ($null -ne $RngLog) -and ($RngLog.Present -eq $true)
  $seedOn = ($null -ne $Seed) -and ($Seed.Present -eq $true)
  if ($rngOn -and -not $seedOn) { return $false }
  return $true
}

function Start-SeededProcess {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$ArgumentList = @(),
    [string]$WorkingDirectory = '',
    [string]$RedirectStandardOutput = '',
    [string]$RedirectStandardError = '',
    [string]$WindowStyle = 'Hidden',
    # `Resolve-Mir2Seed` 的返回值；`$null` / `Present=$false` ⇒ 不注入（与改动前逐字节相同）。
    [psobject]$Seed = $null,
    # `Resolve-Mir2StateLog` 的返回值；`$null` / `Present=$false` ⇒ 不注入（与改动前逐字节相同）。
    [psobject]$StateLog = $null,
    # `Resolve-Mir2RngLog` 的返回值；`$null` / `Present=$false` ⇒ 不注入（与改动前逐字节相同）。
    [psobject]$RngLog = $null
  )
  $inject = ($null -ne $Seed) -and ($Seed.Present -eq $true)
  $injectState = ($null -ne $StateLog) -and ($StateLog.Present -eq $true)
  $injectRng = ($null -ne $RngLog) -and ($RngLog.Present -eq $true)

  # 快照调用方环境：只改 game-svr 这一个子进程的环境，起完立即还原（不影响后续网关等子进程，
  # 也不污染调用方 shell）。`Start-Process` 的子进程在**创建时**快照环境块 ⇒ 还原时机安全。
  $had = Test-Path Env:MIR2_RNG_SEED
  $prev = if ($had) { $env:MIR2_RNG_SEED } else { $null }
  $hadState = Test-Path Env:MIR2_RNG_STATE_LOG
  $prevState = if ($hadState) { $env:MIR2_RNG_STATE_LOG } else { $null }
  $hadRng = Test-Path Env:MIR2_RNG_LOG
  $prevRng = if ($hadRng) { $env:MIR2_RNG_LOG } else { $null }

  $sp = @{ FilePath = $FilePath; WindowStyle = $WindowStyle; PassThru = $true }
  if ($ArgumentList.Count -gt 0) { $sp.ArgumentList = $ArgumentList }
  if (-not [string]::IsNullOrEmpty($WorkingDirectory)) { $sp.WorkingDirectory = $WorkingDirectory }
  if (-not [string]::IsNullOrEmpty($RedirectStandardOutput)) { $sp.RedirectStandardOutput = $RedirectStandardOutput }
  if (-not [string]::IsNullOrEmpty($RedirectStandardError)) { $sp.RedirectStandardError = $RedirectStandardError }

  try {
    # 注入**原文**（与 C# 半看到的字符串一致；game-svr 的 `seed::parse_seed` 同样 trim + 解析）。
    if ($inject) { $env:MIR2_RNG_SEED = [string]$Seed.Text }
    # 路径按**原文**注入（game-svr 的 `state_log::from_env` 原样用作 `File::create` 目标）。
    if ($injectState) { $env:MIR2_RNG_STATE_LOG = [string]$StateLog.Path }
    # 路径按**原文**注入（game-svr 的 `rng_trace::enable_from_env` 原样用作 `File::create` 目标）。
    if ($injectRng) { $env:MIR2_RNG_LOG = [string]$RngLog.Path }
    $p = Start-Process @sp
  } finally {
    if ($inject) {
      if ($had) { $env:MIR2_RNG_SEED = $prev } else { Remove-Item Env:MIR2_RNG_SEED -ErrorAction SilentlyContinue }
    }
    if ($injectState) {
      if ($hadState) { $env:MIR2_RNG_STATE_LOG = $prevState } else { Remove-Item Env:MIR2_RNG_STATE_LOG -ErrorAction SilentlyContinue }
    }
    if ($injectRng) {
      if ($hadRng) { $env:MIR2_RNG_LOG = $prevRng } else { Remove-Item Env:MIR2_RNG_LOG -ErrorAction SilentlyContinue }
    }
  }
  # 只返回进程对象（`RUST_SEED_INJECTED` / `RUST_STATE_LOG_INJECTED` / `RUST_RNG_LOG_INJECTED` 行由调用方
  # 经 `Get-*-InjectedLine` 打印）——保证 `$p = Start-SeededProcess ...` 拿到的是进程而不是数组。
  return $p
}
