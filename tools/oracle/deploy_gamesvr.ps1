#requires -Version 5
<#
.SYNOPSIS
  deploy_gamesvr.ps1 — 把仓库构建输出里的 GameSvr 组件集铺到 oracle 部署目录（S1 缺陷①/② 的落地步骤）。

.DESCRIPTION
  为什么需要它：`E:\MirServer\M2GameSvr` 历史上被"只拷了一部分"地刷新过
  （部署件里的 M2Server.dll/GameSrv.dll 是 2026-10-10 00:10 的构建，
   OpenMir2.dll/SystemModule.dll/ScriptSystem.dll/CommandSystem.dll/PlanesSystem.dll/GameSrv.exe
   还是 2026-10-09 的）—— 这就是 §13.4-5 登记的"部署件与源码版本漂移"。
  本脚本把**同一份构建**的组件集整体铺过去，使部署件回到 SINGLE-BUILD 状态
  （对账见 tools/oracle/reconcile_deployment.ps1）。

  只铺"本仓库构建出来的"文件（显式清单，清单里少一个就报错），不碰三方件、
  不碰 Envir/Map/Castle 等内容目录与 *.conf。

  被替换的文件会先备份到 $BackupDir（默认 %TEMP%\s1-deploy-backup-<时间戳>），路径会打印出来。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/oracle/deploy_gamesvr.ps1
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/oracle/deploy_gamesvr.ps1 -WhatIf
#>
param(
  [string]$RepoRoot = "",
  [string]$ServerRoot = "E:\MirServer",
  [string]$BuildDir = "",
  [string]$BackupDir = "",
  [switch]$SkipBuild,
  [switch]$WhatIf,
  [int]$BootTimeoutSec = 60
)

$ErrorActionPreference = "Stop"
if ($RepoRoot -eq "") { $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path }
if ($BuildDir -eq "") { $BuildDir = Join-Path $RepoRoot 'src\GameSrv\bin\Release' }
if ($BackupDir -eq "") { $BackupDir = Join-Path $env:TEMP ("s1-deploy-backup-" + (Get-Date -Format "yyyyMMdd-HHmmss")) }

$Target = Join-Path $ServerRoot 'M2GameSvr'
$LogDir = Join-Path $ServerRoot 'logs'
$TargetExe = Join-Path $Target 'GameSrv.exe'

# 本仓库构建出来的组件集（GameSrv 项目输出里的"自有程序集 + 启动件"）。
# 三方件（NLog/TouchSocket/Serilog/MySqlConnector…）与插件（IPLocal.dll）不在此列：
# 它们各版本间逐字节相同，且不属于"这份源码编出来的"这个命题。
$RepoBuiltFiles = @(
  'GameSrv.exe'
  'GameSrv.dll'
  'GameSrv.deps.json'
  'GameSrv.runtimeconfig.json'
  'M2Server.dll'
  'OpenMir2.dll'
  'SystemModule.dll'
  'ScriptSystem.dll'
  'CommandSystem.dll'
  'PlanesSystem.dll'
)

if (-not (Test-Path $BuildDir)) { throw "构建输出不存在: $BuildDir（先 dotnet build src/GameSrv/GameSrv.csproj -c Release）" }
if (-not (Test-Path $TargetExe)) { throw "部署目录不完整: 找不到 $TargetExe" }

# ---- 0. 先构建（默认行为）----
# 为什么默认要构建：这些自有文件是**多项目**产物，而 MSBuild 只在自己项目构建时才把引用拷进
# 下游输出目录。于是"改了 M2Server、只编了别的项目"时，`src/GameSrv/bin/Release/M2Server.dll`
# 还是**改动前**的那份 —— 直接铺过去就等于把修复悄悄丢掉（判据是日志行号会等于改动前的行号）。
if (-not $SkipBuild) {
  $proj = Join-Path $RepoRoot 'src\GameSrv\GameSrv.csproj'
  if (-not (Test-Path $proj)) { throw "找不到 $proj" }
  Write-Output "== 先构建 $proj（-SkipBuild 可跳过）"
  & dotnet build $proj -c Release -v q --nologo
  if ($LASTEXITCODE -ne 0) { throw "构建失败（exit $LASTEXITCODE），拒绝铺部署" }
}

# ---- 0.1 新鲜度断言：构建输出不得比它自己的源码旧 ----
# 直接钉住上面那个坑：每个自有产物必须比"产出它的项目的源码"新。分不清归属就跳过该项。
$ArtifactSourceMap = @{
  'M2Server.dll'      = 'src\M2Server'
  'OpenMir2.dll'      = 'src\OpenMir2'
  'SystemModule.dll'  = 'src\Modules\SystemModule'
  'ScriptSystem.dll'  = 'src\Modules\ScriptEngine'
  'CommandSystem.dll' = 'src\Modules\GameCommand'
  'PlanesSystem.dll'  = 'src\Modules\PlanesSystem'
  'GameSrv.dll'       = 'src\GameSrv'
  'GameSrv.exe'       = 'src\GameSrv'
}
$stale = @()
foreach ($name in $RepoBuiltFiles) {
  if (-not $ArtifactSourceMap.ContainsKey($name)) { continue }
  $srcDir = Join-Path $RepoRoot $ArtifactSourceMap[$name]
  if (-not (Test-Path $srcDir)) { continue }
  $newestSource = Get-ChildItem $srcDir -Recurse -File -Include *.cs -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch '\\(bin|obj)\\' } |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
  if (-not $newestSource) { continue }
  $artifact = Get-Item (Join-Path $BuildDir $name)
  if ($artifact.LastWriteTime -lt $newestSource.LastWriteTime) {
    $stale += ("{0}（产物 {1} < 源码 {2} {3}）" -f $name, $artifact.LastWriteTime.ToString('HH:mm:ss'), $newestSource.LastWriteTime.ToString('HH:mm:ss'), $newestSource.Name)
  }
}
if ($stale.Count -gt 0) {
  throw ("构建输出比源码旧，拒绝铺部署（多半是改了代码没重建这个项目）：`n  " + ($stale -join "`n  "))
}
Write-Output '   新鲜度断言：构建输出都不比源码旧'

$missing = @($RepoBuiltFiles | Where-Object { -not (Test-Path (Join-Path $BuildDir $_)) })
if ($missing.Count -gt 0) { throw "构建输出缺少清单里的文件（清单过期了？）: $($missing -join ', ')" }

Write-Output ("== deploy_gamesvr: $BuildDir  ->  $Target")
New-Item -ItemType Directory -Force -Path $BackupDir | Out-Null

# ---- 1. 备份将被替换的文件 ----
foreach ($name in $RepoBuiltFiles) {
  $dst = Join-Path $Target $name
  if (Test-Path $dst) { Copy-Item -LiteralPath $dst -Destination (Join-Path $BackupDir $name) -Force }
  $pdb = [System.IO.Path]::ChangeExtension($name, '.pdb')
  $dstPdb = Join-Path $Target $pdb
  if ((Test-Path $dstPdb) -and (Test-Path (Join-Path $BuildDir $pdb))) {
    Copy-Item -LiteralPath $dstPdb -Destination (Join-Path $BackupDir $pdb) -Force
  }
}
Write-Output ("   备份: $BackupDir")

if ($WhatIf) {
  Write-Output '   -WhatIf：只列将要替换的文件，不落盘、不重启'
  foreach ($name in $RepoBuiltFiles) {
    $src = Join-Path $BuildDir $name
    $dst = Join-Path $Target $name
    $same = (Test-Path $dst) -and ((Get-FileHash -LiteralPath $src).Hash -eq (Get-FileHash -LiteralPath $dst).Hash)
    Write-Output ("     {0,-32} {1}" -f $name, $(if ($same) { '已一致' } else { '将替换' }))
  }
  exit 0
}

# ---- 2. 停 GameSvr（只停这个部署目录里的那个进程） ----
$procs = @(Get-Process GameSrv -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $TargetExe })
foreach ($p in $procs) {
  Write-Output ("   停 GameSvr pid=" + $p.Id)
  Stop-Process -Id $p.Id -Force
  $p.WaitForExit(15000) | Out-Null
}
Start-Sleep -Seconds 2

# ---- 3. 铺文件 ----
foreach ($name in $RepoBuiltFiles) {
  Copy-Item -LiteralPath (Join-Path $BuildDir $name) -Destination (Join-Path $Target $name) -Force
  $pdb = [System.IO.Path]::ChangeExtension($name, '.pdb')
  if (Test-Path (Join-Path $BuildDir $pdb)) {
    Copy-Item -LiteralPath (Join-Path $BuildDir $pdb) -Destination (Join-Path $Target $pdb) -Force
  }
}
Write-Output ("   已铺 " + $RepoBuiltFiles.Count + " 个自有文件（+ 同名 .pdb）")

# ---- 4. 起 GameSvr（工作目录/日志与 start-all.ps1 一致） ----
# 必须走 detached 帮手：PowerShell 级 -RedirectStandardOutput 会让长命的 GameSvr 占住本脚本的
# stdout 管道，调用方读不到 EOF（症状：脚本跑完了却不回、会话不退）。见 detached.ps1 的实测三档。
. (Join-Path $PSScriptRoot 'detached.ps1')
Start-DetachedProcess -Exe $TargetExe -WorkDir $Target `
  -OutLog (Join-Path $LogDir 'GameSvr.out.log') -ErrLog (Join-Path $LogDir 'GameSvr.err.log')

$deadline = (Get-Date).AddSeconds($BootTimeoutSec)
$listening = $false
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Seconds 3
  $text = (netstat -ano | Out-String)
  if ($text -match ':5000\s.*LISTENING') { $listening = $true; break }
}
if ($listening) { Write-Output '   端口 5000 已监听（GameSvr 起来了）' } else { Write-Output '   [警告] 60s 内未见端口 5000 监听，查 logs\GameSvr.err.log' }

Write-Output '   铺后对账：'
$fileHashes = [ordered]@{}
foreach ($name in $RepoBuiltFiles) {
  $src = (Get-FileHash -LiteralPath (Join-Path $BuildDir $name)).Hash.Substring(0, 16)
  $dst = (Get-FileHash -LiteralPath (Join-Path $Target $name)).Hash.Substring(0, 16)
  $fileHashes[$name] = $dst
  $tag = 'OK '
  if ($src -ne $dst) { $tag = 'RED' }
  Write-Output ("     {0} {1,-32} src={2} dst={3}" -f $tag, $name, $src, $dst)
}

# ---- 5. 部署来源登记（缺陷②的"对账凭据"）----
# 为什么必须登记：.NET 编译把源码绝对路径写进 PDB/MVID，**换个检出目录重建出来的 DLL 字节不同**。
# 所以"部署件与源码构建输出逐字节一致"这句话只对"产出它的那个检出"成立；
# 不把来源钉下来，换个检出跑对账就会看到假漂移。
$head = ''
$branch = ''
$dirty = $false
try { $head = (& git -C $RepoRoot rev-parse HEAD 2>$null) } catch { }
try { $branch = (& git -C $RepoRoot rev-parse --abbrev-ref HEAD 2>$null) } catch { }
try { $dirty = (@(& git -C $RepoRoot status --porcelain 2>$null).Count -gt 0) } catch { }
$provenance = [pscustomobject]@{
  deployed_at = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss')
  repo_root   = $RepoRoot
  git_head    = $head
  git_branch  = $branch
  git_dirty   = $dirty
  build_dir   = $BuildDir
  files       = $fileHashes
}
$provenance | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $Target '!deployed-from.json') -Encoding UTF8
Write-Output ("   部署来源登记: $Target\!deployed-from.json  ($branch@$head dirty=$dirty)")

exit $(if ($listening) { 0 } else { 1 })
