#requires -Version 5
<#
.SYNOPSIS
  check_pickup_unreachable.ps1 — 机器可判地证明「真实 oracle 栈里 CM_PICKUP 不可达」，
  从而判定 M2 验收② 的 `m2-06-pickup` 截图**在 oracle 上物理不可能采到**。

.DESCRIPTION
  为什么需要它：`tools/m2-acceptance/README.md` §2 的 7 张截图里第 6 张是「捡物」，
  判据要求「点击地面物品、物品消失或背包出现该物品」。但在本仓 oracle（C# 真身）里
  **地面物品从来没有被创建过**：

    1. `MapItem` 是 `record struct`，`ItemId` 字段默认 0；其构造器里唯一一处赋值是注释
       （`src/Modules/SystemModule/Data/MapItem.cs:41`：`// this.ItemId = M2Share.ActorMgr.GetNextIdentity();`）。
    2. 全仓**没有任何**未注释的 `ItemId = …` 赋值点（对象初始化器也没有）。
    3. `Envirnoment.AddItemToMap` 第一行就是 `if (mapItem.ItemId == 0) return false;`
       （`src/M2Server/Maps/Envirnoment.cs:88-93`）。
    ⇒ 所有 `new MapItem`（掉落 `BaseObject.cs:551/740`、脚本 `ExecutionProcessingSys.cs:5694/5730`）
      的 `ItemId` 恒 0 ⇒ `AddItemToMap` **恒 false** ⇒ 地面无物 ⇒ `ClientPickUpItem`
      （`PlayObject.Base.cs:55`）的 `Envir.GetItem(CurrX,CurrY,…)` 永远取不到 ⇒ `CM_PICKUP` 无物可拾。

  这不是 Rust 侧缺件：**C# 侧同样采不到** `m2-06-pickup`。故本脚本把该判定做成可复跑门禁，
  交给 owner 裁决②判据（是「6 张 + 夹具证据」还是别的最小改法）。

  判据/退出码（与 harness 其它脚本同口径）：
    `0` GREEN  — 证明成立：oracle 无任何 ItemId 赋值点，且守卫存在 ⇒ pickup 不可达。
    `1` RED    — 证明被推翻：发现未注释的 `ItemId = …` 赋值点（⇒ 地面物品可被创建，pickup 可能可达）。
    `2` PREREQ — 缺 oracle 源（`src/**` 不在本检出）⇒ 前置未满足，**绝不静默通过**。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_pickup_unreachable.ps1
#>
param(
  [string]$RepoRoot = "",
  # 留档目录（缺省不落盘；“看怎么红的”请给一个目录）
  [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
if ([string]::IsNullOrEmpty($RepoRoot)) {
  $Here = Split-Path -Parent $MyInvocation.MyCommand.Path
  $RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
}
$SrcRoot = Join-Path $RepoRoot "src"

function Emit([string]$text) { Write-Output $text }

if (-not (Test-Path $SrcRoot)) {
  Emit "PREREQ_MISSING: 找不到 oracle 源 $SrcRoot（需在含 src/** 的检出里跑）"
  exit 2
}

$log = New-Object System.Collections.Generic.List[string]
function Record([string]$line) { $log.Add($line) | Out-Null; Emit $line }

Record ("== pickup 不可达证明 @ " + (Get-Date).ToString("o") + " ==")
Record ("repo=" + $RepoRoot)

# ---- A) ItemId 赋值点（含对象初始化器）必须为空（排除纯注释行）----
$csFiles = Get-ChildItem -Path $SrcRoot -Recurse -File -Filter *.cs -ErrorAction SilentlyContinue
Record ("-- A) 扫描 " + $csFiles.Count + " 个 .cs 找未注释的 ItemId 赋值点")
$assignments = @()
foreach ($f in $csFiles) {
  $n = 0
  foreach ($line in (Get-Content -LiteralPath $f.FullName -ErrorAction SilentlyContinue)) {
    $n++
    if ($line -match 'ItemId\s*=[^=]') {
      $trim = $line.TrimStart()
      if ($trim.StartsWith("//") -or $trim.StartsWith("*") -or $trim.StartsWith("///")) { continue }
      $rel = $f.FullName.Substring($RepoRoot.Length).TrimStart('\', '/')
      $assignments += ("{0}:{1}: {2}" -f $rel, $n, $line.Trim())
    }
  }
}
if ($assignments.Count -gt 0) {
  Record "RED: 发现未注释的 ItemId 赋值点 —— pickup 不可达的证明被推翻："
  foreach ($a in $assignments) { Record ("  " + $a) }
  if ($OutDir) { New-Item -ItemType Directory -Force -Path $OutDir | Out-Null; $log | Set-Content -Path (Join-Path $OutDir "pickup-unreachable.txt") -Encoding utf8 }
  exit 1
}
Record "  OK: 0 个未注释的 ItemId 赋值点（唯一一处是 MapItem.cs 注释）"

# ---- B) MapItem.cs 的构造器赋值必须是注释 ----
$mapItem = Join-Path $SrcRoot "Modules\SystemModule\Data\MapItem.cs"
if (-not (Test-Path $mapItem)) { Emit "PREREQ_MISSING: 缺 $mapItem"; exit 2 }
$mapItemLines = Get-Content -LiteralPath $mapItem
$commented = @($mapItemLines | Where-Object { $_ -match '^\s*//\s*this\.ItemId\s*=' })
if ($commented.Count -eq 0) {
  Record "RED: MapItem.cs 里找不到被注释的 ItemId 赋值（口径已变，需重判）"
  if ($OutDir) { New-Item -ItemType Directory -Force -Path $OutDir | Out-Null; $log | Set-Content -Path (Join-Path $OutDir "pickup-unreachable.txt") -Encoding utf8 }
  exit 1
}
Record ("  OK: MapItem.cs 构造器里 ItemId 赋值仍是注释 -> " + $commented[0].Trim())

# ---- C) Envirnoment.AddItemToMap 首行守卫必须在 ----
$envir = Join-Path $SrcRoot "M2Server\Maps\Envirnoment.cs"
if (-not (Test-Path $envir)) { Emit "PREREQ_MISSING: 缺 $envir"; exit 2 }
$envirText = Get-Content -LiteralPath $envir -Raw
if ($envirText -notmatch 'if\s*\(\s*mapItem\.ItemId\s*==\s*0\s*\)') {
  Record 'RED: Envirnoment.AddItemToMap 的 ItemId==0 守卫不见了（口径已变，需重判）'
  if ($OutDir) { New-Item -ItemType Directory -Force -Path $OutDir | Out-Null; $log | Set-Content -Path (Join-Path $OutDir "pickup-unreachable.txt") -Encoding utf8 }
  exit 1
}
Record '  OK: Envirnoment.AddItemToMap 首行 if (mapItem.ItemId == 0) return false; 在'

# ---- D) 所有 new MapItem 构造点都不设 ItemId（再钉一层）----
$news = @($csFiles | Where-Object { $_.FullName -like "*M2Server*" -or $_.FullName -like "*ScriptEngine*" } |
  ForEach-Object { Select-String -LiteralPath $_.FullName -Pattern 'new MapItem' -SimpleMatch -ErrorAction SilentlyContinue })
Record ("  OK: 全仓 " + $news.Count + " 个 new MapItem 构造点，均无 ItemId 赋值（A 已覆盖）")

Record "GREEN: oracle 侧 MapItem.ItemId 恒 0 ⇒ AddItemToMap 恒 false ⇒ 地面无物 ⇒ CM_PICKUP 不可达。"
Record "        ⇒ m2-06-pickup 在 C# 与 Rust 两侧都物理采不到（判据待 owner 裁决，见 README §2.2）。"

if ($OutDir) {
  New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
  $log | Set-Content -Path (Join-Path $OutDir "pickup-unreachable.txt") -Encoding utf8
  Record ("EVIDENCE " + (Join-Path $OutDir "pickup-unreachable.txt"))
}
exit 0
