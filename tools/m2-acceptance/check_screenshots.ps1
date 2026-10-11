#requires -Version 5
<#
.SYNOPSIS
  check_screenshots.ps1 — M2 验收②「真客户端可玩证据 = 7 张截图」的清单校验。

.DESCRIPTION
  入口：`D:\MirClient-run` + `run-release.cmd`（1024×768，参数 `127.0.0.1:7000`），
  驱动脚本复用仓库既有的 `tools/client/mir_flow.ps1`（Win32 消息点击/打字/截图 + stages.ndjson）。
  本脚本只做**清单校验**：7 个规定文件名是否都在 `-ShotsDir` 且非空。
  （画面内容由验收人按 README 的「画面/时机」逐张核对；机判只保证"交齐了"。）

  7 张与命名（详见 tools/m2-acceptance/README.md §2）：
    m2-01-enter-map.png   进图后（人物站在地图里，小地图/坐标可见）
    m2-02-walk.png        走路中（与 01 画面相比位置已变，不是黑屏/卡住）
    m2-03-talk.png        说话（聊天框出现本角色发出的文本）
    m2-04-see-others.png  看见别人（屏内出现另一个角色）
    m2-05-attack.png      打怪（攻击动作/怪物受击，血条或伤害可见）
    m2-06-pickup.png      捡物（地上物品消失或背包内出现该物品）
    m2-07-relogin.png     小退再进不黑屏（再次进入游戏画面，非登录/黑屏）

  红/绿：
    GREEN  = 7 张齐全且非空；
    PREREQ = 一张都没有 ⇒ 截图阶段未做，退出码 2（不静默通过）；
    RED    = 缺 1–6 张 / 存在 0 字节文件 ⇒ 退出码 1。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_screenshots.ps1 `
    -ShotsDir tools/m2-acceptance/shots
#>
param(
  [string]$ShotsDir = "",
  [int]$MinBytes = 1024,
  [string]$OutFile = ""
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($ShotsDir)) { $ShotsDir = Join-Path $Here "shots" }
if ([string]::IsNullOrEmpty($OutFile)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $OutFile = Join-Path "E:\tmp\m2-acceptance" ("shots-$stamp.json")
}

$Expected = @(
  @{ file = "m2-01-enter-map.png"; what = "进图后" },
  @{ file = "m2-02-walk.png"; what = "走路中" },
  @{ file = "m2-03-talk.png"; what = "说话" },
  @{ file = "m2-04-see-others.png"; what = "看见别人" },
  @{ file = "m2-05-attack.png"; what = "打怪" },
  @{ file = "m2-06-pickup.png"; what = "捡物" },
  @{ file = "m2-07-relogin.png"; what = "小退再进不黑屏" }
)

$present = @()
$missing = @()
$bad = @()
foreach ($e in $Expected) {
  $p = Join-Path $ShotsDir $e.file
  if (-not (Test-Path $p)) { $missing += $e; continue }
  $len = (Get-Item $p).Length
  if ($len -lt $MinBytes) { $bad += [pscustomobject]@{ file = $e.file; bytes = $len } }
  $present += [pscustomobject]@{ file = $e.file; what = $e.what; bytes = $len }
}

Write-Output ("M2 验收② 截图清单（入口 D:\MirClient-run + run-release.cmd；目录 " + $ShotsDir + "）")
foreach ($p in $present) { Write-Output ("  OK      {0,-22} {1} ({2} B)" -f $p.file, $p.what, $p.bytes) }
foreach ($m in $missing) { Write-Output ("  MISSING {0,-22} {1}" -f $m.file, $m.what) }

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutFile) | Out-Null
$evidence = [ordered]@{
  tool = "m2-acceptance/check_screenshots.ps1"
  ts = (Get-Date).ToString("o")
  shots_dir = $ShotsDir
  present = $present
  missing = @($missing | ForEach-Object { $_.file })
  too_small = @($bad | ForEach-Object { $_.file })
  pass = $false
}
$evidence | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
Write-Output ("EVIDENCE " + $OutFile)

if ($present.Count -eq 0) {
  Write-Output "PREREQ_MISSING: 一张截图都没有——真客户端验收阶段尚未运行"
  Write-Output "  按 tools/m2-acceptance/README.md §2 用 mir_flow.ps1 进入游戏并逐张截图；本工具不代替截图"
  exit 2
}
if ($missing.Count -gt 0 -or $bad.Count -gt 0) {
  Write-Output ("RED: 截图不全（缺 {0} 张、过小 {1} 张）" -f $missing.Count, $bad.Count)
  exit 1
}
$evidence.pass = $true
$evidence | ConvertTo-Json -Depth 6 | Set-Content -Path $OutFile -Encoding utf8
Write-Output "GREEN: 7 张验收截图齐全且非空（画面内容仍需按 README 逐张核对）"
exit 0
