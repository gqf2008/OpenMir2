#requires -Version 5
<#
.SYNOPSIS
  rust_seed_plumb.ps1 — `start_rust_stack.ps1` 的 `MIR2_RNG_SEED` 种子管线（卡 `openmir2-m3-rust-stack-seed-plumb`）。

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

  只读约束：本文件不自己起栈、不占端口；进程级用例由 `check_seed_plumb.ps1` 驱动。
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
    [psobject]$Seed = $null
  )
  $inject = ($null -ne $Seed) -and ($Seed.Present -eq $true)

  # 快照调用方环境：只改 game-svr 这一个子进程的环境，起完立即还原（不影响后续网关等子进程，
  # 也不污染调用方 shell）。`Start-Process` 的子进程在**创建时**快照环境块 ⇒ 还原时机安全。
  $had = Test-Path Env:MIR2_RNG_SEED
  $prev = if ($had) { $env:MIR2_RNG_SEED } else { $null }

  $sp = @{ FilePath = $FilePath; WindowStyle = $WindowStyle; PassThru = $true }
  if ($ArgumentList.Count -gt 0) { $sp.ArgumentList = $ArgumentList }
  if (-not [string]::IsNullOrEmpty($WorkingDirectory)) { $sp.WorkingDirectory = $WorkingDirectory }
  if (-not [string]::IsNullOrEmpty($RedirectStandardOutput)) { $sp.RedirectStandardOutput = $RedirectStandardOutput }
  if (-not [string]::IsNullOrEmpty($RedirectStandardError)) { $sp.RedirectStandardError = $RedirectStandardError }

  try {
    # 注入**原文**（与 C# 半看到的字符串一致；game-svr 的 `seed::parse_seed` 同样 trim + 解析）。
    if ($inject) { $env:MIR2_RNG_SEED = [string]$Seed.Text }
    $p = Start-Process @sp
  } finally {
    if ($inject) {
      if ($had) { $env:MIR2_RNG_SEED = $prev } else { Remove-Item Env:MIR2_RNG_SEED -ErrorAction SilentlyContinue }
    }
  }
  # 只返回进程对象（`RUST_SEED_INJECTED` 行由调用方经 `Get-SeedInjectedLine` 打印）——
  # 保证 `$p = Start-SeededProcess ...` 拿到的是进程而不是数组。
  return $p
}
