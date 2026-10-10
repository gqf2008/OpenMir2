#requires -Version 5
<#
.SYNOPSIS
  reconcile_deployment.ps1 — E:\MirServer 部署件 ↔ 源码构建输出 的版本对账（S1 缺陷②）。

.DESCRIPTION
  组件清单以 `E:\MirServer\start-all.ps1` 的 $Components 为准（那是"谁是 oracle"的唯一权威声明），
  本脚本把它镜像一份（start-all.ps1 改了这里要同步改），然后逐组件对每个受管产物
  （*.dll / *.exe / 无扩展名 apphost）做三件事：

    1. 判宿主形态：Windows PE（MZ）还是别的（例：macOS 的 Mach-O apphost —— 本机根本跑不起来）；
    2. 与仓库源码构建输出（src/<项目>/bin/Release）按 SHA256 逐字节对账：MATCH / DRIFT / DEPLOY-ONLY；
    3. 给出组件级结论：SINGLE-BUILD（受管产物全部出自同一份构建）/ MIXED / FOREIGN / NO-SOURCE。

  另外扫描服务端根目录下**未声明**的部署式目录（如 Mir200），给出它的真实血统
  （PE/Mach-O、deps.json 的 runtimeTarget、编译期源码路径），避免"看起来像部署件"的目录再骗人。

  退出码：任一契约组件判 MIXED / FOREIGN / NO-SOURCE 即 1（说明部署件不是这份源码编出来的）。

.PARAMETER SelfTestRed
  红检：在临时目录里复制一组产物、故意改坏一个字节，要求比较器必须判 DRIFT（否则判红退出）。

.PARAMETER OutDir
  报告输出目录（默认 <repo>/mir2-rs/tests/parity/evidence/S1）。写 reconcile.json + reconcile.md。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/oracle/reconcile_deployment.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/oracle/reconcile_deployment.ps1 -SelfTestRed
#>
param(
  [string]$ServerRoot = "E:\MirServer",
  [string]$RepoRoot = "",
  [string]$OutDir = "",
  [switch]$SelfTestRed
)

$ErrorActionPreference = "Stop"
if ($RepoRoot -eq "") { $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path }
if ($OutDir -eq "") { $OutDir = Join-Path $RepoRoot "mir2-rs\tests\parity\evidence\S1" }

# 「跑着的那一套」用的是哪个检出 —— 权威来源是 start-all.ps1 的 $Repo（DBSrv/LoginSrv 直接从
# 它的 bin/Release 启动）。不读它、硬套本脚本所在检出，会在 worktree 里对账时把这两个组件
# 误判成"部署目录不存在"。
$HostRepoRoot = ''
$startAll = Join-Path $ServerRoot 'start-all.ps1'
if (Test-Path -LiteralPath $startAll) {
  $m = [regex]::Match((Get-Content -LiteralPath $startAll -Raw), '(?m)^\s*\$Repo\s*=\s*[''"]([^''"]+)[''"]')
  if ($m.Success) { $HostRepoRoot = $m.Groups[1].Value }
}
if ($HostRepoRoot -eq '') { $HostRepoRoot = $RepoRoot }

# GameSvr 的比对基准取自它自己的「部署来源登记」：.NET 把源码路径编进 MVID，
# 跨检出重建字节必不同；只有对着"产出它的那个检出"比，逐字节相等才是有意义的判据。
$DeployedRepoRoot = $RepoRoot
$provenancePath = Join-Path $ServerRoot 'M2GameSvr\!deployed-from.json'
$provenance = $null
if (Test-Path -LiteralPath $provenancePath) {
  try { $provenance = Get-Content -LiteralPath $provenancePath -Raw -Encoding UTF8 | ConvertFrom-Json } catch { }
  if ($provenance -and $provenance.repo_root) { $DeployedRepoRoot = $provenance.repo_root }
}

# 注意：除 GameSvr 外，其余部署组件的比对基准一律取"跑着的那套检出"（$HostRepoRoot），
# 而不是本脚本所在检出 —— 否则在 worktree 里跑对账会把它们误判成 NO-SOURCE，
# 门禁就会因为"在哪个目录跑的"而变色。GameSvr 单独用登记里的产出检出（见上）。
$Components = @(
  [pscustomobject]@{ Name = 'DBSrv';     Dir = (Join-Path $HostRepoRoot 'src\DBSrv\bin\Release');     Exe = 'DBSrv.exe';     Src = (Join-Path $HostRepoRoot 'src\DBSrv\bin\Release');     SelfHosted = $true }
  [pscustomobject]@{ Name = 'LoginSrv';  Dir = (Join-Path $HostRepoRoot 'src\LoginSrv\bin\Release');  Exe = 'LoginSrv.exe';  Src = (Join-Path $HostRepoRoot 'src\LoginSrv\bin\Release');  SelfHosted = $true }
  [pscustomobject]@{ Name = 'GameSvr';   Dir = (Join-Path $ServerRoot 'M2GameSvr');                  Exe = 'GameSrv.exe';   Src = (Join-Path $DeployedRepoRoot 'src\GameSrv\bin\Release'); SelfHosted = $false }
  [pscustomobject]@{ Name = 'LoginGate'; Dir = (Join-Path $ServerRoot 'LoginGate');                  Exe = 'LoginGate.exe'; Src = (Join-Path $HostRepoRoot 'src\LoginGate\bin\Release'); SelfHosted = $false }
  [pscustomobject]@{ Name = 'SelGate';   Dir = (Join-Path $ServerRoot 'SelGate');                    Exe = 'SelGate.exe';   Src = (Join-Path $HostRepoRoot 'src\SelGate\bin\Release');   SelfHosted = $false }
  [pscustomobject]@{ Name = 'RunGate';   Dir = (Join-Path $ServerRoot 'RunGate');                    Exe = 'GameGate.exe';  Src = (Join-Path $HostRepoRoot 'src\GameGate\bin\Release');  SelfHosted = $false }
)

function Get-HostKind([string]$Path) {
  # 只读头 4 字节判宿主形态：本机能不能直接跑，就看这一条
  try {
    $fs = [System.IO.File]::OpenRead($Path)
    try {
      $buf = New-Object byte[] 4
      $n = $fs.Read($buf, 0, 4)
      if ($n -lt 4) { return 'SHORT' }
      $hex = ($buf | ForEach-Object { $_.ToString('x2') }) -join ''
      if ($hex.Substring(0, 4) -eq '4d5a') { return 'PE' }
      if ($hex -eq 'cffaedfe' -or $hex -eq 'cefaedfe') { return 'MACHO' }
      if ($hex -eq 'feedfacf' -or $hex -eq 'feedface') { return 'MACHO-BE' }
      if ($hex -eq 'cafebabe') { return 'MACHO-FAT' }
      return "OTHER($hex)"
    } finally { $fs.Dispose() }
  } catch { return "ERR" }
}

function Test-Absent([string]$Path) {
  # PS 5.1 的 Test-Path '' 会直接抛错，所有存在性判断都走这里
  if ([string]::IsNullOrEmpty($Path)) { return $true }
  return (-not (Test-Path -LiteralPath $Path))
}

function Get-FileHash16([string]$Path) {
  if (Test-Absent $Path) { return $null }
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.Substring(0, 16)
}

function Compare-Artifact([string]$DeployPath, [string]$SrcPath) {
  # 返回 MATCH / DRIFT / DEPLOY-ONLY / SRC-ONLY / ABSENT —— 纯函数，可被 -SelfTestRed 直接验
  $deployAbsent = Test-Absent $DeployPath
  $srcAbsent = Test-Absent $SrcPath
  if ($deployAbsent) {
    if ($srcAbsent) { return 'ABSENT' }
    return 'SRC-ONLY'
  }
  if ($srcAbsent) { return 'DEPLOY-ONLY' }
  if ((Get-FileHash16 $DeployPath) -eq (Get-FileHash16 $SrcPath)) { return 'MATCH' }
  return 'DRIFT'
}

function Compare-Component([pscustomobject]$Comp) {
  $rows = @()
  if (-not (Test-Path $Comp.Dir)) {
    return [pscustomobject]@{ Component = $Comp.Name; Verdict = 'NO-DEPLOY-DIR'; Rows = @(); Reason = "部署目录不存在: $($Comp.Dir)" }
  }
  if (-not (Test-Path $Comp.Src)) {
    return [pscustomobject]@{ Component = $Comp.Name; Verdict = 'NO-SOURCE'; Rows = @(); Reason = "源码构建输出不存在: $($Comp.Src)（先在仓库里 dotnet build -c Release）" }
  }
  if ($Comp.SelfHosted) {
    # 进程直接从源码构建输出目录启动，没有独立部署副本 —— 不存在"漂移"这个面
    return [pscustomobject]@{ Component = $Comp.Name; Verdict = 'SELF-HOSTED'; Rows = @(); Reason = "按 start-all.ps1 直接从 $($Comp.Dir) 启动，无独立部署副本" }
  }

  # 受管产物 = 两侧所有 *.dll / *.exe + 与组件同名的无扩展名 apphost
  $names = New-Object System.Collections.Generic.HashSet[string]
  foreach ($side in @($Comp.Dir, $Comp.Src)) {
    foreach ($f in (Get-ChildItem $side -File -ErrorAction SilentlyContinue)) {
      if ($f.Extension -eq '.dll' -or $f.Extension -eq '.exe') { [void]$names.Add($f.Name) }
    }
  }
  foreach ($hostName in @($Comp.Name, [System.IO.Path]::GetFileNameWithoutExtension($Comp.Exe))) {
    if ((Test-Path -LiteralPath (Join-Path $Comp.Dir $hostName)) -or (Test-Path -LiteralPath (Join-Path $Comp.Src $hostName))) {
      [void]$names.Add($hostName)
    }
  }

  foreach ($name in ($names | Sort-Object)) {
    $deployPath = Join-Path $Comp.Dir $name
    $srcPath = Join-Path $Comp.Src $name
    $verdict = Compare-Artifact $deployPath $srcPath
    # 部署侧没有、源码构建侧有 = 部署漏文件（真漂移）；反之只是三方件/插件（信息项）
    if ($verdict -eq 'DEPLOY-ONLY') { $verdict = 'EXTRA' }
    if ($verdict -eq 'SRC-ONLY') { $verdict = 'MISSING' }
    $rows += [pscustomobject]@{
      Artifact = $name
      Verdict  = $verdict
      Host     = (Get-HostKind $deployPath)
      Deploy   = (Get-FileHash16 $deployPath)
      Src      = (Get-FileHash16 $srcPath)
      DeployTime = $(if (Test-Absent $deployPath) { '' } else { (Get-Item -LiteralPath $deployPath).LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss') })
      SrcTime    = $(if (Test-Absent $srcPath) { '' } else { (Get-Item -LiteralPath $srcPath).LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss') })
    }
  }

  $exeRow = $rows | Where-Object { $_.Artifact -eq $Comp.Exe } | Select-Object -First 1
  $verdictComp = 'SINGLE-BUILD'
  $reason = '受管产物全部与源码构建输出逐字节一致'
  if ($exeRow -and ($exeRow.Host -ne 'PE')) {
    $verdictComp = 'FOREIGN'
    $reason = "启动件 $($Comp.Exe) 不是 Windows PE（实测 $($exeRow.Host)）"
  } else {
    $driftRows = @($rows | Where-Object { $_.Verdict -eq 'DRIFT' -or $_.Verdict -eq 'MISSING' -or $_.Verdict -eq 'ABSENT' })
    if ($driftRows.Count -gt 0) {
      $verdictComp = 'MIXED'
      $reason = "有产物与源码构建输出不一致（不是同一份构建）: " + (($driftRows | ForEach-Object { "$($_.Artifact)($($_.Verdict))" }) -join ', ')
    }
  }
  return [pscustomobject]@{ Component = $Comp.Name; Verdict = $verdictComp; Rows = $rows; Reason = $reason }
}

function Get-UndeclaredTrees([string]$Root, [string[]]$Declared) {
  # 服务端根目录下"看起来像部署件"但不在契约里的目录：给出真实血统，免得再被当成可跑的部署
  $out = @()
  foreach ($d in (Get-ChildItem $Root -Directory -ErrorAction SilentlyContinue)) {
    if ($Declared -contains $d.Name) { continue }
    $apphost = Get-ChildItem $d.FullName -File -ErrorAction SilentlyContinue |
      Where-Object { $_.Extension -eq '' -and $_.Length -gt 4096 } | Select-Object -First 1
    $depsFile = Get-ChildItem $d.FullName -File -Filter '*.deps.json' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $apphost -and $null -eq $depsFile) { continue }
    $target = ''
    if ($depsFile) {
      try {
        $json = Get-Content $depsFile.FullName -Raw | ConvertFrom-Json
        $target = $json.runtimeTarget.name
      } catch { $target = 'unparsable' }
    }
    $out += [pscustomobject]@{
      Dir          = $d.Name
      Apphost      = $(if ($apphost) { $apphost.Name } else { '' })
      HostKind     = $(if ($apphost) { Get-HostKind $apphost.FullName } else { '' })
      RuntimeTarget = $target
      Note         = '未声明为 oracle 部署件（start-all.ps1 的 $Components 里没有）'
    }
  }
  return $out
}

# ---------------- 红检：比较器必须能把"改坏一个字节"判成 DRIFT ----------------
if ($SelfTestRed) {
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("s1-reconcile-red-" + (Get-Date -Format 'yyyyMMddHHmmss'))
  New-Item -ItemType Directory -Force -Path $tmp | Out-Null
  $same = Join-Path $tmp 'same.bin'
  $mutated = Join-Path $tmp 'mutated.bin'
  $bytes = (New-Object byte[] 64)
  for ($i = 0; $i -lt 64; $i++) { $bytes[$i] = [byte]$i }
  [System.IO.File]::WriteAllBytes($same, $bytes)
  $bytes[32] = $bytes[32] -bxor 0xff
  [System.IO.File]::WriteAllBytes($mutated, $bytes)

  $v1 = Compare-Artifact $same $same
  $v2 = Compare-Artifact $mutated $same
  $v3 = Compare-Artifact $mutated $null
  $ok = ($v1 -eq 'MATCH') -and ($v2 -eq 'DRIFT') -and ($v3 -eq 'DEPLOY-ONLY')
  Write-Output ("SELFTEST identity={0} bitflip={1} missing-src={2}" -f $v1, $v2, $v3)
  Remove-Item -Recurse -Force $tmp
  if ($ok) { Write-Output 'SELFTEST_RED GREEN: 改坏一个字节必判 DRIFT（比较器可证伪）'; exit 0 }
  Write-Output 'SELFTEST_RED RED: 比较器对篡改不敏感——门禁失效！'
  exit 1
}

# ---------------- 正跑 ----------------
$results = @()
foreach ($c in $Components) { $results += (Compare-Component $c) }
$contractDirs = @('M2GameSvr', 'LoginGate', 'SelGate', 'RunGate')
$undeclared = Get-UndeclaredTrees $ServerRoot $contractDirs

$bad = @($results | Where-Object { $_.Verdict -ne 'SINGLE-BUILD' -and $_.Verdict -ne 'SELF-HOSTED' })
Write-Output "== oracle 部署件 ↔ 源码构建输出 对账（ServerRoot=$ServerRoot）"
Write-Output ("   跑着的那一套取自检出（start-all.ps1 的 `$Repo）：{0}" -f $HostRepoRoot)
if ($provenance) {
  Write-Output ("   部署来源登记：{0}@{1} dirty={2}（{3}）" -f $provenance.git_branch, $provenance.git_head, $provenance.git_dirty, $provenance.deployed_at)
  Write-Output ("   GameSvr 比对基准：{0}（取自登记）" -f $DeployedRepoRoot)
  if ($DeployedRepoRoot -ne $RepoRoot) {
    Write-Output ("   [注意] 本次对账检出（{0}）≠ 产出该部署的检出：.NET 把源码路径编进 MVID，" -f $RepoRoot)
    Write-Output ("          跨检出重建的 DLL 字节必然不同 —— 所以 GameSvr 一律对着登记里的检出比。")
  }
} else {
  Write-Output "   [注意] 没有 !deployed-from.json（部署早于 S1 登记）：GameSvr 只能按本次检出逐字节比。"
}
foreach ($r in $results) {
  $tag = 'OK  '
  if ($r.Verdict -ne 'SINGLE-BUILD' -and $r.Verdict -ne 'SELF-HOSTED') { $tag = 'RED ' }
  Write-Output ("{0} {1,-9} {2,-12} {3}" -f $tag, $r.Component, $r.Verdict, $r.Reason)
  foreach ($row in ($r.Rows | Where-Object { $_.Verdict -eq 'DRIFT' -or $_.Verdict -eq 'MISSING' -or $_.Verdict -eq 'ABSENT' })) {
    Write-Output ("      {0,-34} {1,-12} host={2,-6} deploy={3} src={4}" -f $row.Artifact, $row.Verdict, $row.Host, $row.Deploy, $row.Src)
  }
  $extra = @($r.Rows | Where-Object { $_.Verdict -eq 'EXTRA' })
  if ($extra.Count -gt 0) {
    Write-Output ("      [信息] 部署侧独有（三方件/插件，非漂移）: " + (($extra | ForEach-Object { $_.Artifact }) -join ', '))
  }
}
Write-Output ""
Write-Output "== 未声明为部署件的目录（血统核查）"
foreach ($u in $undeclared) {
  Write-Output ("    {0,-10} apphost={1,-14} host={2,-6} runtimeTarget={3}" -f $u.Dir, $u.Apphost, $u.HostKind, $u.RuntimeTarget)
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$report = [pscustomobject]@{
  generated_at = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
  server_root  = $ServerRoot
  repo_root    = $RepoRoot
  host_repo_root = $HostRepoRoot
  deployed_repo_root = $DeployedRepoRoot
  provenance   = $provenance
  components   = $results
  undeclared   = $undeclared
  red_count    = $bad.Count
}
$report | ConvertTo-Json -Depth 6 | Set-Content -Path (Join-Path $OutDir 'reconcile.json') -Encoding UTF8

$md = New-Object System.Collections.Generic.List[string]
$md.Add('# oracle 部署件 ↔ 源码构建输出 对账')
$md.Add('')
$md.Add("- 生成时间：$($report.generated_at)")
$md.Add("- ServerRoot：``$ServerRoot``；RepoRoot：``$RepoRoot``")
$md.Add("- 命令：``powershell -ExecutionPolicy Bypass -File tools/oracle/reconcile_deployment.ps1``（红检 ``-SelfTestRed``）")
if ($provenance) {
  $md.Add("- 部署来源登记：``$($provenance.repo_root)`` @ ``$($provenance.git_branch)`` ``$($provenance.git_head)`` dirty=$($provenance.git_dirty)（$($provenance.deployed_at)）")
  if ($provenance.repo_root -ne $RepoRoot) {
    $md.Add("- **[注意]** 本次对账检出（``$RepoRoot``）≠ 产出该部署的检出：.NET 把源码路径编进 MVID，跨检出重建的 DLL 字节必然不同，MIXED 未必是漂移")
  }
} else {
  $md.Add("- **[注意]** 无 ``!deployed-from.json``（部署早于 S1 登记），只能按本次检出逐字节比")
}
$md.Add('')
$md.Add('| 组件 | 结论 | 依据 |')
$md.Add('| --- | --- | --- |')
foreach ($r in $results) { $md.Add("| $($r.Component) | **$($r.Verdict)** | $($r.Reason) |") }
$md.Add('')
foreach ($r in $results) {
  $md.Add("## $($r.Component) — $($r.Verdict)")
  $md.Add('')
  $md.Add('| 产物 | 判定 | 宿主 | 部署件 sha256[0:16] | 源码 sha256[0:16] | 部署时间 | 源码构建时间 |')
  $md.Add('| --- | --- | --- | --- | --- | --- | --- |')
  foreach ($row in $r.Rows) {
    $md.Add("| $($row.Artifact) | $($row.Verdict) | $($row.Host) | $($row.Deploy) | $($row.Src) | $($row.DeployTime) | $($row.SrcTime) |")
  }
  $md.Add('')
}
$md.Add('## 未声明为部署件的目录')
$md.Add('')
$md.Add('| 目录 | apphost | 宿主 | deps runtimeTarget | 说明 |')
$md.Add('| --- | --- | --- | --- | --- |')
foreach ($u in $undeclared) { $md.Add("| $($u.Dir) | $($u.Apphost) | $($u.HostKind) | $($u.RuntimeTarget) | $($u.Note) |") }
$md.Add('')
$md | Set-Content -Path (Join-Path $OutDir 'reconcile.md') -Encoding UTF8

Write-Output ""
$summary = "==== reconcile: {0} 组件 / {1} 判红；报告 {2}" -f $results.Count, $bad.Count, (Join-Path $OutDir 'reconcile.md')
Write-Output $summary
exit $(if ($bad.Count -eq 0) { 0 } else { 1 })
