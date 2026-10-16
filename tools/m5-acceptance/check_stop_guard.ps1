#requires -Version 5
<#
.SYNOPSIS
  check_stop_guard.ps1 —— 静态断言 `start_full_rust_stack.ps1` 的 `-Stop` 循环含 **MySQL 跳过守卫**。

.DESCRIPTION
  背景（2026-10-15 验证者 D2）：MySQL 是 M5 的共享基础设施（「保留 3306」）。起栈器可以「起」它，
  但 `-Stop` **绝不允许**把它带走——否则灰度/回滚演练会误停与本栈无关的共享 DB。
  范式：`stage_gray_rollback.ps1` 的 `SKIP-STOP MySQL`（`if ($g.name -eq 'MySQL') { ...; continue }`）。

  本自检**只做静态 AST 分析**（`[Parser]::ParseFile`），不起任何进程/栈/端口、不连库。
  判据（全部满足才 GREEN）：
    ① 目标脚本能解析（无语法错误）；
    ② 存在 `if ($Stop) { ... }` 块；
    ③ 块内存在 `foreach` 循环；
    ④ 循环体内存在一个条件里含 `MySQL` 的 `if`，且其语句块含 `continue`（= 跳过守卫）；
    ⑤ 该守卫的源码位置**早于**循环内第一处 `Stop-Process`（守卫必须在杀进程之前）。

  牙（改坏必红）：把 ④ 的守卫注掉/删掉 ⇒ 本脚本 rc=1，打印 `RED`。
  `-Target` 可指向任意副本，便于做变异对照（见调用方留证）。

.EXAMPLE
  pwsh -File tools/m5-acceptance/check_stop_guard.ps1
.EXAMPLE
  # 变异对照：注掉守卫的副本 ⇒ 必须 rc=1
  pwsh -File tools/m5-acceptance/check_stop_guard.ps1 -Target /tmp/mut_start_full.ps1
#>
param(
  [string]$Target = (Join-Path $PSScriptRoot 'start_full_rust_stack.ps1')
)

$ErrorActionPreference = 'Stop'
function Fail([string]$msg) {
  Write-Output ("RED: " + $msg)
  exit 1
}

if (-not (Test-Path $Target)) { Fail ("目标脚本不存在: " + $Target) }

$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
  (Resolve-Path $Target).Path, [ref]$tokens, [ref]$errors)
if ($errors.Count -gt 0) {
  Fail ("目标脚本解析失败（{0} 个语法错误），首条: {1}" -f $errors.Count, $errors[0].Message)
}
Write-Output ("PARSE OK " + (Split-Path -Leaf $Target))

# ② if ($Stop) { ... }
$stopIfs = @($ast.FindAll({
  param($n)
  $n -is [System.Management.Automation.Language.IfStatementAst] -and
  $n.Clauses.Count -ge 1 -and
  ($n.Clauses[0].Item1.Extent.Text -match '^\s*\$Stop\s*$')
}, $true))
if ($stopIfs.Count -eq 0) { Fail '未找到 `if ($Stop) { ... }` 块' }
$stopIf = $stopIfs[0]

# ③ 块内 foreach 循环
$loops = @($stopIf.FindAll({ param($n) $n -is [System.Management.Automation.Language.ForEachStatementAst] }, $true))
if ($loops.Count -eq 0) { Fail '`if ($Stop)` 块内未找到 foreach 循环' }

$foundGuard = $false
$guardPos = -1
$stopProcPos = -1
foreach ($loop in $loops) {
  # ④ 循环体内条件含 MySQL 的 if，且语句块含 continue
  $guards = @($loop.FindAll({
    param($n)
    $n -is [System.Management.Automation.Language.IfStatementAst] -and
    $n.Clauses.Count -ge 1 -and
    ($n.Clauses[0].Item1.Extent.Text -match 'MySQL') -and
    (@($n.FindAll({ param($c) $c -is [System.Management.Automation.Language.ContinueStatementAst] }, $true)).Count -gt 0)
  }, $true))
  if ($guards.Count -gt 0) {
    $foundGuard = $true
    $guardPos = $guards[0].Extent.StartOffset
    # ⑤ 同一循环内第一处 Stop-Process 的位置
    $sps = @($loop.FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'Stop-Process' }, $true))
    if ($sps.Count -gt 0) { $stopProcPos = $sps[0].Extent.StartOffset }
    break
  }
}

if (-not $foundGuard) {
  Fail "`-Stop` 循环缺少 MySQL 跳过守卫（`if (<cond 含 MySQL>) { ...; continue }`）—— 共享 DB 会被误停（D2）"
}
if ($stopProcPos -ge 0 -and $guardPos -gt $stopProcPos) {
  Fail ("MySQL 守卫位置({0})晚于首处 Stop-Process({1})——守卫被绕过，杀进程发生在守卫之前" -f $guardPos, $stopProcPos)
}

Write-Output ("PASS SKIP-STOP MySQL guard @offset={0} < first Stop-Process @offset={1}" -f $guardPos, $stopProcPos)
Write-Output 'GREEN: check_stop_guard 1/1'
exit 0
