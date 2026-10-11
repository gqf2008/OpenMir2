#requires -Version 5
<#
.SYNOPSIS
  check_screenshots.ps1 — M2 验收②「真客户端可玩证据」的截图清单校验。

  默认 = 6 张必需（除 `m2-06-pickup` 外；该张 oracle 不可达，见下）+ 可选 pickup；`-RequirePickup` 回 7 张口径。

.DESCRIPTION
  入口：`D:\MirClient-run` + `run-release.cmd`（1024×768，参数 `127.0.0.1:7000`），
  驱动脚本复用仓库既有的 `tools/client/mir_flow.ps1`（Win32 消息点击/打字/截图 + stages.ndjson）。
  本脚本只做**清单校验**：7 个规定文件名是否都在 `-ShotsDir` 且非空。
  （画面内容由验收人按 README 的「画面/时机」逐张核对；机判只保证"交齐了"。）

  7 张与命名（详见 tools/m2-acceptance/README.md §2；**`m2-06-pickup` 已由 owner 裁决为 oracle 不可达**——
  见 README §2.2 + `check_pickup_unreachable.ps1`，默认**不要求**该张，传 -RequirePickup 才要求）：
    m2-01-enter-map.png   进图后（人物站在地图里，小地图/坐标可见）
    m2-02-walk.png        走路中（与 01 画面相比位置已变，不是黑屏/卡住）
    m2-03-talk.png        说话（聊天框出现本角色发出的文本）
    m2-04-see-others.png  看见别人（屏内出现另一个角色）
    m2-05-attack.png      打怪（攻击动作/怪物受击，血条或伤害可见）
    m2-06-pickup.png      捡物（**oracle 不可达 ⇒ 默认不要求**；见 §2.2 与 T-25）
    m2-07-relogin.png     小退再进不黑屏（再次进入游戏画面，非登录/黑屏）

  红/绿：
    GREEN  = 必需张齐全且非空（默认 = 除 pickup 外的 6 张）；
    PREREQ = 一张都没有 ⇒ 截图阶段未做，退出码 2（不静默通过）；
    RED    = 必需张缺 / 存在过小文件 ⇒ 退出码 1。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_screenshots.ps1 `
    -ShotsDir tools/m2-acceptance/shots
#>
param(
  [string]$ShotsDir = "",
  [int]$MinBytes = 1024,
  [string]$OutFile = "",
  # 保留 legacy 口径：仍把 m2-06-pickup 当必需（仅当部署栈上证明地面物可达、owner 改判 branch B 时用）
  [switch]$RequirePickup
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($ShotsDir)) { $ShotsDir = Join-Path $Here "shots" }
if ([string]::IsNullOrEmpty($OutFile)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $OutFile = Join-Path "E:\tmp\m2-acceptance" ("shots-$stamp.json")
}

$ExpectedAll = @(
  @{ file = "m2-01-enter-map.png"; what = "进图后" },
  @{ file = "m2-02-walk.png"; what = "走路中" },
  @{ file = "m2-03-talk.png"; what = "说话" },
  @{ file = "m2-04-see-others.png"; what = "看见别人" },
  @{ file = "m2-05-attack.png"; what = "打怪" },
  @{ file = "m2-06-pickup.png"; what = "捡物" },
  @{ file = "m2-07-relogin.png"; what = "小退再进不黑屏" }
)
# owner 裁决（卡 comment `073f0dac`，2026-10-12）：oracle 无法让任何物品落地 ⇒ `m2-06-pickup` 不可达
# ⇒ 默认不作为必需（可达形态判据见 `check_pickup_unreachable.ps1`）；`-RequirePickup` 保留 legacy。
$PickupFile = "m2-06-pickup.png"
$Expected = @($ExpectedAll | Where-Object { $RequirePickup -or $_.file -ne $PickupFile })

$present = @()
$missing = @()
$bad = @()
foreach ($e in $ExpectedAll) {
  $p = Join-Path $ShotsDir $e.file
  if (-not (Test-Path $p)) { continue }
  $len = (Get-Item $p).Length
  $present += [pscustomobject]@{ file = $e.file; what = $e.what; bytes = $len }
}
foreach ($e in $Expected) {
  $p = Join-Path $ShotsDir $e.file
  if (-not (Test-Path $p)) { $missing += $e; continue }
  $len = (Get-Item $p).Length
  if ($len -lt $MinBytes) { $bad += [pscustomobject]@{ file = $e.file; bytes = $len } }
}
$pickupPresent = @($present | Where-Object { $_.file -eq $PickupFile }).Count -gt 0

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
  require_pickup = [bool]$RequirePickup
  pickup_present = [bool]$pickupPresent
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
if (-not $RequirePickup -and -not $pickupPresent) {
  Write-Output ("GREEN: 6 张验收截图齐全且非空（m2-06-pickup 按 oracle 不可达不计——见 README §2.2 / T-25；画面内容仍需逐张核对）")
} else {
  Write-Output ("GREEN: 7 张验收截图齐全且非空（画面内容仍需按 README 逐张核对）")
}
exit 0
