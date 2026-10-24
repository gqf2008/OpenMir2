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
    2. 全仓**没有任何**针对 MapItem 的未注释 `ItemId = …` 赋值点（对象初始化器也没有）。
    3. `Envirnoment.AddItemToMap` 第一行就是 `if (mapItem.ItemId == 0) return false;`
       （`src/M2Server/Maps/Envirnoment.cs:88-93`）。
    ⇒ 所有 `new MapItem`（掉落 `BaseObject.cs:551/740`、脚本 `ExecutionProcessingSys.cs:5694/5730`）
      的 `ItemId` 恒 0 ⇒ `AddItemToMap` **恒 false** ⇒ 地面无物 ⇒ `ClientPickUpItem`
      （`PlayObject.Base.cs:55`）的 `Envir.GetItem(CurrX,CurrY,…)` 永远取不到 ⇒ `CM_PICKUP` 无物可拾。

  这不是 Rust 侧缺件：**C# 侧同样采不到** `m2-06-pickup`。故本脚本把该判定做成可复跑门禁，
  交给 owner 裁决②判据（是「6 张 + 夹具证据」还是别的最小改法）。

  **判据是类型/构造点感知的**（2026-10-24 修：原实现用 `ItemId\s*=[^=]` 全局匹配，
  把 `BotSrv` 的 `MShare.DropExpDropInfo.ItemId`（MShare.cs:923，另一结构体）也误判成
  MapItem 赋值 ⇒ master 上假红 rc=1）。现在只认指向 **MapItem** 的赋值点：

    A1) `new MapItem { … ItemId = … }`：在该**构造调用**的对象初始化器**顶层**找 ItemId 赋值
        （嵌套在初始化器里的其它结构体初始化器不算）。
    A2) 由 `new MapItem` / 目标类型 `new()` 得到的变量（`var x = new MapItem…`、
        `MapItem x = …` 的局部/字段/参数），其上 `x.ItemId = …` 形式的赋值。
    A3) `Get<MapItem>(…)` 返回后直接链式 `.ItemId = …`。
    A4) 经由 MapItem 字段/属性的链式写：`xxx.MapItem.ItemId = …`。
    A5) record `with { … ItemId = … }`（`MapItem` 是 `record struct`，MapItem.cs:5；
        左操作数是本文件 MapItem 变量时判定，review P-4）。
    A6) 数组/集合元素写：`a[i].ItemId = …`（名字来自 `MapItem[] a` /
        `List<MapItem>` 等声明；review P-5）。
    A2 还收 `var x = …Get<MapItem>(…)` 推导出的变量（review P-3）。
  其它结构体的 `.ItemId`（如 `MShare.DropExpDropInfo`）、以及注释/字符串里的同名文本
  都**不是** MapItem 赋值点，不参与判定。

  残留口径（故意保守）：类型追踪只覆盖显式声明/`new`/`Get<MapItem>` 结果与直接数组/集合
  元素写，不做完整语义分析。仍看不见：把 MapItem 塞进 `object`/`dynamic`、经字典值/跨文件
  属性链或 LINQ 中间量（如 `d.Values.First().ItemId = …`）再写字段。那种写法若出现，应在本
  脚本里补规则（A1–A6 之后继续加），而不是放宽匹配。宁可假红，不可假绿。

  判据/退出码（与 harness 其它脚本同口径）：
    `0` GREEN  — 证明成立：无任何指向 MapItem 的 ItemId 赋值点，且守卫存在 ⇒ pickup 不可达。
    `1` RED    — 证明被推翻：发现指向 MapItem 的未注释 `ItemId = …` 赋值点
                 （或 MapItem 构造点/守卫口径变化、初始化器无法配对解析）。
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

function Save-Evidence {
  if ($OutDir) {
    New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
    $log | Set-Content -Path (Join-Path $OutDir "pickup-unreachable.txt") -Encoding utf8
    Emit ("EVIDENCE " + (Join-Path $OutDir "pickup-unreachable.txt"))
  }
}

# ---------------------------------------------------------------------------
# 文本工具：注释/字符串遮罩（保留换行 ⇒ 行号对齐）、行号换算、括号/初始化器解析
# ---------------------------------------------------------------------------

# 把注释与字符串/字符字面量的内容替换为空格；换行保留。
# 先匹配块注释，再行注释，再 verbatim/interpolated/普通字符串与字符字面量。
$MaskPattern = '(?s)/\*.*?\*/|//[^\r\n]*|@\$?"(?:[^"]|"")*"|\$@?"(?:[^"\\\r\n]|\\.)*"|"(?:[^"\\\r\n]|\\.)*"|''(?:[^''\\\r\n]|\\.)*'''
$MaskEval = {
  param($m)
  $chars = $m.Value.ToCharArray()
  for ($k = 0; $k -lt $chars.Length; $k++) {
    if ($chars[$k] -ne "`r" -and $chars[$k] -ne "`n") { $chars[$k] = [char]' ' }
  }
  return (New-Object string (,$chars))
}

function Get-MaskedSource([string]$raw) {
  return [regex]::Replace($raw, $MaskPattern, [System.Text.RegularExpressions.MatchEvaluator]$MaskEval)
}

function Get-LineNumber([string]$text, [int]$pos) {
  $n = 1
  $limit = [Math]::Min($pos, $text.Length)
  for ($i = 0; $i -lt $limit; $i++) {
    if ($text[$i] -eq "`n") { $n++ }
  }
  return $n
}

function Skip-Ws([string]$text, [int]$pos) {
  $i = $pos
  while ($i -lt $text.Length -and [char]::IsWhiteSpace($text[$i])) { $i++ }
  return $i
}

# $text[$pos] == '(' ⇒ 返回匹配 ')' 之后的位置；括号不配对返回 -1
function Skip-Parens([string]$text, [int]$pos) {
  $depth = 0
  for ($i = $pos; $i -lt $text.Length; $i++) {
    $c = $text[$i]
    if ($c -eq '(') { $depth++ }
    elseif ($c -eq ')') {
      $depth--
      if ($depth -eq 0) { return ($i + 1) }
    }
  }
  return -1
}

# $text[$openPos] == '{' ⇒ 返回 { Text = 仅初始化器顶层(depth==1)的字符(其余为空格),
#                                Closed = 是否找到配对的 '}', StartPos = $openPos }
function Get-TopLevelInitializer([string]$text, [int]$openPos) {
  $sb = New-Object System.Text.StringBuilder
  $depth = 0
  $closed = $false
  for ($i = $openPos; $i -lt $text.Length; $i++) {
    $c = $text[$i]
    if ($c -eq '{') {
      $depth++
      [void]$sb.Append(' ')
      continue
    }
    if ($c -eq '}') {
      $depth--
      if ($depth -le 0) { $closed = $true; break }
      [void]$sb.Append(' ')
      continue
    }
    if ($depth -eq 1) { [void]$sb.Append($c) } else { [void]$sb.Append(' ') }
  }
  return [pscustomobject]@{ Text = $sb.ToString(); Closed = $closed; StartPos = $openPos }
}

# 指向 ItemId 的赋值（含 `+=` 等复合赋值；排除 `==`/`!=` 比较）
$ItemIdAssign = '\bItemId\s*(?:[+\-*/%&|^]=|\?\?=|=)(?!=)'
# MapItem 构造点：`new MapItem`、`new SystemModule.Data.MapItem` 等
$NewMapItemRe = [regex]'new\s+(?:[A-Za-z_]\w*\s*\.\s*)*MapItem\b'
# 目标类型 new：`MapItem x = new(…)
$TargetTypedNewRe = [regex]'\bMapItem\s+(\w+)\s*=\s*new\s*\('
# 显式声明：`MapItem x`（局部/字段/参数/模式变量）
$MapItemDeclRe = [regex]'\bMapItem\s+(\w+)\b'
# `var x = new MapItem…`
$VarNewMapItemRe = [regex]'\bvar\s+(\w+)\s*=\s*new\s+(?:[A-Za-z_]\w*\s*\.\s*)*MapItem\b'
# `Get<MapItem>(…)` 直接链式写
$GetMapItemRe = [regex]'Get\s*<\s*(?:[A-Za-z_]\w*\s*\.\s*)*MapItem\s*>\s*\('
# 经由 MapItem 字段/属性的链式写：`x.MapItem.ItemId = …`
$FieldChainRe = [regex]'\.\s*MapItem\s*\.\s*ItemId\s*(?:[+\-*/%&|^]=|\?\?=|=)(?!=)'
# `var x = … Get<MapItem>(…)` 推导出的变量（review P-3）
$VarGetMapItemRe = [regex]'\bvar\s+(\w+)\s*=\s*[^;\r\n]*?Get\s*<\s*(?:[A-Za-z_]\w*\s*\.\s*)*MapItem\s*>\s*\('
# 数组/集合声明：`MapItem[] a` / `List<MapItem> xs` 等（review P-5）
$MapItemSeqDeclRe = [regex]'\b(?:MapItem\s*\[\s*\]|(?:List|IList|IReadOnlyList|HashSet|IEnumerable)\s*<\s*(?:[A-Za-z_]\w*\s*\.\s*)*MapItem\s*>)\s+(\w+)'
# record `with { … }`（review P-4）
$WithRe = [regex]'\bwith\s*\{'

Record ("== pickup 不可达证明（类型感知）@ " + (Get-Date).ToString("o") + " ==")
Record ("repo=" + $RepoRoot)

$csFiles = @(Get-ChildItem -Path $SrcRoot -Recurse -File -Filter *.cs -ErrorAction SilentlyContinue)
Record ("-- A) 扫描 " + $csFiles.Count + " 个 .cs 找**指向 MapItem** 的 ItemId 赋值点（注释/字符串已遮罩）")

$reds = New-Object System.Collections.Generic.List[string]
$constructions = New-Object System.Collections.Generic.List[string]
$declaredNames = New-Object System.Collections.Generic.List[string]
$namesByFile = @{}
# 关键字/上下文词过滤：`MapItem is …` 等会被声明正则误收，别拿它们当变量名
$NameKeywordRe = '^(is|as|new|in|out|ref|default|var|return|public|private|protected|internal|static|readonly|const|using|where|else|case|when|switch|return)$'

function Get-LineSnippet([string[]]$rawLines, [int]$ln) {
  if ($ln -ge 1 -and $ln -le $rawLines.Count) { return $rawLines[$ln - 1].Trim() }
  return ""
}

# 大小写敏感去重（review P-2：`Sort-Object -Unique` 在 PS 5.1 是大小写不敏感，
# 会把同一文件里的 `MapItem`/`mapItem` 两种拼写并掉，随后大小写敏感匹配就漏一半）。
function Get-UniqueOrdinal([object]$items) {
  $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
  $out = New-Object System.Collections.Generic.List[string]
  foreach ($i in @($items)) {
    $v = [string]$i
    if ($seen.Add($v)) { $out.Add($v) | Out-Null }
  }
  return $out.ToArray()
}

# RED 行按 (文件, 行) 去重（review P-6：A2/A4 会对同一行各报一次）。
function Get-DedupedRedLines([object]$lines) {
  $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
  $out = New-Object System.Collections.Generic.List[string]
  foreach ($l in @($lines)) {
    $text = [string]$l
    $m = [regex]::Match($text, '^(.*?):(\d+):')
    $key = if ($m.Success) { $m.Groups[1].Value + ':' + $m.Groups[2].Value } else { $text }
    if ($seen.Add($key)) { $out.Add($text) | Out-Null }
  }
  return $out.ToArray()
}

foreach ($f in $csFiles) {
  $rel = $f.FullName.Substring($RepoRoot.Length).TrimStart('\', '/')
  $raw = [IO.File]::ReadAllText($f.FullName)
  $masked = Get-MaskedSource $raw
  $rawLines = $raw -split "`r?`n"
  $fileNames = New-Object System.Collections.Generic.List[string]

  # A1) `new MapItem …`（含可选构造实参）后紧跟对象初始化器 ⇒ 只查顶层 ItemId 赋值
  foreach ($m in $NewMapItemRe.Matches($masked)) {
    $p = Skip-Ws $masked ($m.Index + $m.Length)
    if ($p -lt $masked.Length -and $masked[$p] -eq '(') {
      $p2 = Skip-Parens $masked $p
      if ($p2 -lt 0) {
        $ln = Get-LineNumber $masked $m.Index
        $reds.Add(("{0}:{1}: new MapItem 构造实参括号不配对，无法判定 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
        continue
      }
      $p = Skip-Ws $masked $p2
    }
    if ($p -lt $masked.Length -and $masked[$p] -eq '{') {
      $init = Get-TopLevelInitializer $masked $p
      if (-not $init.Closed) {
        $ln = Get-LineNumber $masked $m.Index
        $reds.Add(("{0}:{1}: new MapItem 初始化器括号不配对，无法判定 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
        continue
      }
      $hit = [regex]::Match($init.Text, $ItemIdAssign)
      if ($hit.Success) {
        $ln = Get-LineNumber $masked ($init.StartPos + $hit.Index)
        $reds.Add(("{0}:{1}: new MapItem 初始化器顶层给 ItemId 赋值 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
      }
    }
    $constructions.Add(("{0}:{1}: new MapItem" -f $rel, (Get-LineNumber $masked $m.Index))) | Out-Null
  }

  # A2-prep) 目标类型 new：`MapItem x = new(…) { … }` —— 同样是构造点
  foreach ($m in $TargetTypedNewRe.Matches($masked)) {
    $fileNames.Add($m.Groups[1].Value) | Out-Null
    $newPos = $m.Index + $m.Value.IndexOf('new')
    $p2 = Skip-Parens $masked ($m.Index + $m.Length - 1)
    if ($p2 -lt 0) {
      $ln = Get-LineNumber $masked $newPos
      $reds.Add(("{0}:{1}: 目标类型 new MapItem 实参括号不配对，无法判定 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
      continue
    }
    $p = Skip-Ws $masked $p2
    if ($p -lt $masked.Length -and $masked[$p] -eq '{') {
      $init = Get-TopLevelInitializer $masked $p
      if (-not $init.Closed) {
        $ln = Get-LineNumber $masked $newPos
        $reds.Add(("{0}:{1}: 目标类型 new MapItem 初始化器括号不配对，无法判定 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
        continue
      }
      $hit = [regex]::Match($init.Text, $ItemIdAssign)
      if ($hit.Success) {
        $ln = Get-LineNumber $masked ($init.StartPos + $hit.Index)
        $reds.Add(("{0}:{1}: 目标类型 new MapItem 初始化器顶层给 ItemId 赋值 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
      }
    }
    $constructions.Add(("{0}:{1}: new MapItem (target-typed)" -f $rel, (Get-LineNumber $masked $newPos))) | Out-Null
  }

  # A2) 收集 MapItem 类型变量名（局部/字段/参数/模式变量）与 `var x = new MapItem`
  foreach ($m in $MapItemDeclRe.Matches($masked)) { $fileNames.Add($m.Groups[1].Value) | Out-Null }
  foreach ($m in $VarNewMapItemRe.Matches($masked)) { $fileNames.Add($m.Groups[1].Value) | Out-Null }
  foreach ($m in $VarGetMapItemRe.Matches($masked)) { $fileNames.Add($m.Groups[1].Value) | Out-Null }
  foreach ($m in $MapItemSeqDeclRe.Matches($masked)) { $fileNames.Add($m.Groups[1].Value) | Out-Null }
  $fileNames = @(Get-UniqueOrdinal @($fileNames | Where-Object { $_ -notmatch $NameKeywordRe }))
  $namesByFile[$f.FullName] = $fileNames
  foreach ($n in $fileNames) { $declaredNames.Add($n) | Out-Null }

  # A4) 字段/属性链：`xxx.MapItem.ItemId = …`
  foreach ($m in $FieldChainRe.Matches($masked)) {
    $ln = Get-LineNumber $masked $m.Index
    $reds.Add(("{0}:{1}: 经 .MapItem 字段/属性链给 ItemId 赋值 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
  }

  # A5) record `with { … }`：MapItem 是 record struct（review P-4）。
  #     只认左操作数是本文件 MapItem 变量（或邻近出现 new MapItem / Get<MapItem>）的 with，
  #     避免把其它 record 的 ItemId 误判回原假红类。
  foreach ($m in $WithRe.Matches($masked)) {
    $before = $masked.Substring(0, $m.Index)
    $tail = $before.Substring([Math]::Max(0, $before.Length - 200))
    $op = [regex]::Match($tail, '(\w+)\s*$')
    $operand = if ($op.Success) { $op.Groups[1].Value } else { '' }
    $isMapItem = $operand -and ($fileNames -ccontains $operand)
    if (-not $isMapItem) {
      if ($tail -notmatch '(?:new\s+(?:[A-Za-z_]\w*\s*\.\s*)*MapItem|Get\s*<\s*(?:[A-Za-z_]\w*\s*\.\s*)*MapItem\s*>)') { continue }
    }
    $p = Skip-Ws $masked ($m.Index + $m.Length - 1)
    if ($p -ge $masked.Length -or $masked[$p] -ne '{') { continue }
    $init = Get-TopLevelInitializer $masked $p
    if (-not $init.Closed) {
      $ln = Get-LineNumber $masked $m.Index
      $reds.Add(("{0}:{1}: record with 表达式括号不配对，无法判定 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
      continue
    }
    $hit = [regex]::Match($init.Text, $ItemIdAssign)
    if ($hit.Success) {
      $ln = Get-LineNumber $masked ($init.StartPos + $hit.Index)
      $reds.Add(("{0}:{1}: record with 表达式给 ItemId 赋值（MapItem 是 record struct） | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
    }
  }
}

# A2 检查（按文件各自的 MapItem 类型变量名匹配，避免跨文件同名误配）
$allNames = @(Get-UniqueOrdinal $declaredNames)
Record ("  MapItem 类型变量名（全仓去重）" + $allNames.Count + " 个：" + ($allNames -join ", "))
foreach ($f in $csFiles) {
  $rel = $f.FullName.Substring($RepoRoot.Length).TrimStart('\', '/')
  $raw = [IO.File]::ReadAllText($f.FullName)
  $masked = Get-MaskedSource $raw
  $rawLines = $raw -split "`r?`n"
  $names = @()
  if ($namesByFile.ContainsKey($f.FullName)) { $names = $namesByFile[$f.FullName] }
  foreach ($name in $names) {
    $pat = '(?<![\w])' + [regex]::Escape($name) + '\s*\.\s*ItemId\s*(?:[+\-*/%&|^]=|\?\?=|=)(?!=)'
    foreach ($m in [regex]::Matches($masked, $pat)) {
      $ln = Get-LineNumber $masked $m.Index
      $reds.Add(("{0}:{1}: MapItem 变量 {2} 的 ItemId 赋值 | {3}" -f $rel, $ln, $name, (Get-LineSnippet $rawLines $ln))) | Out-Null
    }
    # A6) 同一变量的数组/集合元素写：`a[i].ItemId = …`（review P-5）
    $patElem = '(?<![\w])' + [regex]::Escape($name) + '\s*\[[^\]]*\]\s*\.\s*ItemId\s*(?:[+\-*/%&|^]=|\?\?=|=)(?!=)'
    foreach ($m in [regex]::Matches($masked, $patElem)) {
      $ln = Get-LineNumber $masked $m.Index
      $reds.Add(("{0}:{1}: MapItem 集合/数组元素 {2}[…].ItemId 赋值 | {3}" -f $rel, $ln, $name, (Get-LineSnippet $rawLines $ln))) | Out-Null
    }
  }
  # A3) `Get<MapItem>(…).ItemId = …`
  foreach ($m in $GetMapItemRe.Matches($masked)) {
    $p2 = Skip-Parens $masked ($m.Index + $m.Length - 1)
    if ($p2 -lt 0) { continue }
    $p = Skip-Ws $masked $p2
    if ($p -lt $masked.Length -and $masked[$p] -eq '.') {
      $p = Skip-Ws $masked ($p + 1)
      if ($p -lt $masked.Length -and $masked.Substring($p) -match '^ItemId\s*(?:[+\-*/%&|^]=|\?\?=|=)(?!=)') {
        $ln = Get-LineNumber $masked $m.Index
        $reds.Add(("{0}:{1}: Get<MapItem>(…) 链式 ItemId 赋值 | {2}" -f $rel, $ln, (Get-LineSnippet $rawLines $ln))) | Out-Null
      }
    }
  }
}

$reds = @(Get-DedupedRedLines $reds)
if ($reds.Count -gt 0) {
  Record "RED: 发现指向 MapItem 的未注释 ItemId 赋值点 —— pickup 不可达的证明被推翻："
  foreach ($a in $reds) { Record ("  " + $a) }
  Save-Evidence
  exit 1
}
Record "  OK: 0 个指向 MapItem 的未注释 ItemId 赋值点（其它结构体的 .ItemId 不计；唯一 MapItem 赋值是 MapItem.cs 注释）"

# ---- B) MapItem.cs 的构造器赋值必须是注释 ----
$mapItem = Join-Path $SrcRoot "Modules\SystemModule\Data\MapItem.cs"
if (-not (Test-Path $mapItem)) { Emit "PREREQ_MISSING: 缺 $mapItem"; exit 2 }
$mapItemLines = Get-Content -LiteralPath $mapItem
$commented = @($mapItemLines | Where-Object { $_ -match '^\s*//\s*this\.ItemId\s*=' })
if ($commented.Count -eq 0) {
  Record "RED: MapItem.cs 里找不到被注释的 ItemId 赋值（口径已变，需重判）"
  Save-Evidence
  exit 1
}
Record ("  OK: MapItem.cs 构造器里 ItemId 赋值仍是注释 -> " + $commented[0].Trim())

# B2) review P-1：B 段不能只验「那行注释还在」——构造器/类型体里新增未注释的
#     `this.ItemId = …`（或 `ItemId = …`）就是推翻核心命题最自然的一行改动，必须 RED。
$mapItemRaw = [IO.File]::ReadAllText($mapItem)
$mapItemMasked = Get-MaskedSource $mapItemRaw
$mapItemRawLines = $mapItemRaw -split "`r?`n"
$ctorReds = New-Object System.Collections.Generic.List[string]
foreach ($m in [regex]::Matches($mapItemMasked, '(?:this\s*\.\s*ItemId|\bItemId)\s*(?:[+\-*/%&|^]=|\?\?=|=)(?!=)')) {
  $ln = Get-LineNumber $mapItemMasked $m.Index
  $ctorReds.Add(("src/Modules/SystemModule/Data/MapItem.cs:{0}: MapItem 类型体内未注释的 ItemId 赋值 | {1}" -f $ln, (Get-LineSnippet $mapItemRawLines $ln))) | Out-Null
}
if ($ctorReds.Count -gt 0) {
  Record "RED: MapItem 类型体里出现未注释的 ItemId 赋值（MapItem.ItemId 恒 0 被推翻）："
  foreach ($a in $ctorReds) { Record ("  " + $a) }
  Save-Evidence
  exit 1
}
Record "  OK: MapItem 类型体内没有未注释的 ItemId 赋值（构造器/字段初始化都无）"

# ---- C) Envirnoment.AddItemToMap 首行守卫必须在 ----
$envir = Join-Path $SrcRoot "M2Server\Maps\Envirnoment.cs"
if (-not (Test-Path $envir)) { Emit "PREREQ_MISSING: 缺 $envir"; exit 2 }
$envirText = Get-Content -LiteralPath $envir -Raw
if ($envirText -notmatch 'if\s*\(\s*mapItem\.ItemId\s*==\s*0\s*\)') {
  Record 'RED: Envirnoment.AddItemToMap 的 ItemId==0 守卫不见了（口径已变，需重判）'
  Save-Evidence
  exit 1
}
Record '  OK: Envirnoment.AddItemToMap 首行 if (mapItem.ItemId == 0) return false; 在'

# ---- D) MapItem 构造点清单（证据）----
Record ("  OK: 全仓 " + $constructions.Count + " 个 MapItem 构造点（new MapItem / 目标类型 new），均无 ItemId 赋值（A 已覆盖）")
foreach ($c in ($constructions | Sort-Object -Unique)) { Record ("    " + $c) }

Record "GREEN: oracle 侧 MapItem.ItemId 恒 0 ⇒ AddItemToMap 恒 false ⇒ 地面无物 ⇒ CM_PICKUP 不可达。"
Record "        ⇒ m2-06-pickup 在 C# 与 Rust 两侧都物理采不到（判据待 owner 裁决，见 README §2.2）。"

Save-Evidence
exit 0
