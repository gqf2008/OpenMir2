#requires -Version 5
<#
.SYNOPSIS
  run_bots.ps1 — 假人压测驱动：BotSrv 多开登录/选角/进游戏，断言成功率。

.DESCRIPTION
  用车间里现成的 BotSrv（src/BotSrv，完整客户端协议栈的 headless 机器人）。
  本脚本只做编排：构建 → 写 AppSetting.json → 起进程 → 读 BotSrv 的 load_stats.ndjson 计数 → 给判据。

  账号规则（BotSrv 内部约定）：账号名 = LoginAccount 前缀 + 递增序号，
  密码 = 账号名。-NewAccount 时机器人先走 CM_ADDNEWUSER 注册再登录。

  判据（默认全过才退出码 0）：
    - BotSrv 内部计数 load_stats.ndjson 的 login_ok == -Count（**不抓日志行**：
      LoginScene.ClientGetPasswordOk 在「有剩余时长」的两个分支只打「个人帐户的期限」，
      抓「帐号登录成功」会把这类账号误判成登录失败 ⇒ 假红）
    - 进程在统计窗口内无异常退出
  红检（-SelfTestRed）：故意指向未监听端口，必须报失败退出码 1。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_bots.ps1 -Count 200 -Prefix loadbot
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_bots.ps1 -SelfTestRed
#>
param(
  [int]$Count = 200,
  [string]$Prefix = "loadbot",
  [string]$ServerName = "热血传奇",
  [string]$Address = "127.0.0.1",
  [int]$Port = 7000,
  [switch]$NewAccount,
  [int]$TimeoutSec = 900,
  [switch]$SkipBuild,
  [string]$LogFile = "",          # 默认 <repo>/tests/botload/bots-<时间戳>.log
  [switch]$FixedNewChr,           # M3 夹具：固定建角 Job/Hair/Sex（默认不固定 = 原随机行为）
  [int]$NewChrJob = 0,            # 仅 -FixedNewChr 时生效（写进 AppSetting.json 的 BotPlay 段）
  [int]$NewChrHair = 0,           # 仅 -FixedNewChr 时生效
  [int]$NewChrSex = 0,            # 仅 -FixedNewChr 时生效
  [switch]$BagEquipSequence,      # M3 T-14 夹具：进世界后按固定顺序发背包/装备动作（拾取→装备→卸下→丢弃→用药）
  [int]$BagEquipStepMs = 1000,    # 仅 -BagEquipSequence 时生效：每步间隔
  [int]$BagEquipStartDelayMs = 3000, # 仅 -BagEquipSequence 时生效：进世界到第一步的额外等待
  [switch]$AttackSequence,           # M3②③ 夹具：进世界后按固定攻击序列发 CM_HIT（T-22 扩展）
  [int]$AttackCount = 120,           # 仅 -AttackSequence 时生效：攻击步数
  [int]$AttackStepMs = 500,          # 仅 -AttackSequence 时生效：每步间隔
  [int]$AttackStartDelayMs = 3000,   # 仅 -AttackSequence 时生效：进世界到第一步的额外等待
  [switch]$ShopSequence,             # M3⑤ 夹具：进世界后按固定顺序发 NPC/商店动作（开面板/买/卖/修理/存取；T-29 家族）
  [string]$ShopOps = 'sell,buy,query_sell_price,query_repair_cost,repair,storage,takeback',  # 仅 -ShopSequence 时生效
  [int]$ShopStepMs = 1000,           # 仅 -ShopSequence 时生效：每步间隔
  [int]$ShopStartDelayMs = 3000,     # 仅 -ShopSequence 时生效：进世界到第一步的额外等待
  [string]$ShopNpcName = '',         # 仅 -ShopSequence 时生效：目标 NPC 名（空 = 视图里第一个）
  [string]$ShopItemName = '',        # 仅 -ShopSequence 时生效：动作物品名（空 = 背包第一件）
  [int]$ShopItemMake = 0,            # 仅 -ShopSequence 时生效：物品 MakeIndex（0 = 只按名字取）
  [int]$ShopCount = 1,               # 仅 -ShopSequence 时生效：卖/买/存取数量
  [switch]$QuestSequence,            # M3⑥ 夹具：进世界后按固定步骤串点任务 NPC/选标签（T-41 家族扩展）
  [string]$QuestSteps = '',          # 仅 -QuestSequence 时生效：步骤串 `op|npc|label;...`
  [int]$QuestStepMs = 1000,          # 仅 -QuestSequence 时生效：每步间隔
  [int]$QuestStartDelayMs = 3000,    # 仅 -QuestSequence 时生效：进世界到第一步的额外等待
  [switch]$PairTradeSequence,        # M3⑦ 夹具：两个假人按固定步骤串发组队/交易动作（T-43 家族扩展）
  [string]$PairTradeSteps = '',      # 仅 -PairTradeSequence 时生效：步骤串 `role|op|param;...`
  [int]$PairTradeStepMs = 1000,      # 仅 -PairTradeSequence 时生效：每步间隔
  [int]$PairTradeStartDelayMs = 3000,# 仅 -PairTradeSequence 时生效：进世界到第一步的额外等待
  [int]$PairTradeRole = 0,           # 仅 -PairTradeSequence 时生效：actor 账号序号
  [int]$PairTradePeerRole = 1,       # 仅 -PairTradeSequence 时生效：peer 账号序号
  [switch]$SellOffSequence,          # M3⑩ 夹具：两个假人按固定步骤串发元宝寄售动作（T-44 家族扩展）
  [string]$SellOffSteps = '',        # 仅 -SellOffSequence 时生效：步骤串 `role|op|param;...`
  [int]$SellOffStepMs = 1000,        # 仅 -SellOffSequence 时生效：每步间隔
  [int]$SellOffStartDelayMs = 3000,  # 仅 -SellOffSequence 时生效：进世界到第一步的额外等待
  [int]$SellOffSellerRole = 0,       # 仅 -SellOffSequence 时生效：卖方账号序号
  [int]$SellOffBuyerRole = 1,        # 仅 -SellOffSequence 时生效：买方账号序号
  [int]$SoftCloseAfterWorldMs = 0,   # M1③ 退出路径夹具：进世界后发 CM_SOFTCLOSE(1009)；0=关（默认）
  [switch]$DisableAutoPlay,          # M1③ 退出路径夹具：关掉自动挂机（默认关=原行为）；见 ClientManager.RunAutoPlay
  [switch]$WriteConfigOnly,       # 只写 AppSetting.json 并退出（不构建/不起进程；供无栈自检与红检用）
  [switch]$RequireWorld,          # F4（T-12）：要求全部假人都有"本会话收到 SM_LOGON"的进世界证据（in_world == Count）才 GREEN
  [switch]$RequireCharCreated,    # 固定起点变体①预热相：等到 N 个「创建角色」标记（只建号，不等进世界）才收工
  [switch]$StopAfterCharCreated,  # M3 预热相夹具（白名单 T-29 扩展）：建角后停在选角界面、不发 CM_SELCHR（默认关=原行为）
                                  # 根因：预热假人进世界后 Stop-Process 会被 GameSvr 当真实下线并异步 SaveHumanRcd 覆盖 pin（owner 19:20）
  [switch]$SelfCheckNoStack,      # 离线自检：只测 M3 序列收尾计数（attack done / bag done），不构建/不起进程
  [switch]$SelfTestRed
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
$BotProj = Join-Path $RepoRoot "src\BotSrv\BotSrv.csproj"
$BotBinDir = Join-Path $RepoRoot "src\BotSrv\bin\Release"
$BotExe = Join-Path $BotBinDir "BotSrv.exe"

# M3 序列收尾标记计数（唯一口径，真实等待与离线自检共用）：
#   attack = BotSrv `M3ATTSEQ {"role":"done",...}`（RobotPlayer.cs:674；F-E 收尾证据）
#   bag    = `M3BAGSEQ done`（T-14）
function Get-M3SeqDoneCount {
    param([AllowEmptyString()][string]$Text, [ValidateSet('bag', 'attack', 'shop', 'quest', 'pairtrade', 'selloff')][string]$Kind)
    if ($Kind -eq 'bag') { return ([regex]::Matches($Text, 'M3BAGSEQ done')).Count }
    if ($Kind -eq 'shop') { return ([regex]::Matches($Text, 'M3SHOPSEQ \{[^\r\n]*"phase":"done"')).Count }
    if ($Kind -eq 'quest') { return ([regex]::Matches($Text, 'M3QUESTSEQ \{[^\r\n]*"phase":"done"')).Count }
    if ($Kind -eq 'pairtrade') { return ([regex]::Matches($Text, 'M3PAIRTRADESEQ \{[^\r\n]*"phase":"done"')).Count }
    if ($Kind -eq 'selloff') { return ([regex]::Matches($Text, 'M3SELLOFFSEQ \{[^\r\n]*"phase":"done"')).Count }
    return ([regex]::Matches($Text, 'M3ATTSEQ \{[^\r\n]*"role":"done"')).Count
}

# M3 序列「预期 done 数」的唯一口径（真实等待与离线自检共用）：
#   bag    = **每个**假人都发一条 `M3BAGSEQ done`（`RobotPlayer.BagEquipSequenceTick`）⇒ = $Bots。
#   attack = **只有 attacker 一个**假人武装（BotSrv `RobotOptions.AttackRole` 缺省 0 ⇒ exact LoginId =
#            LoginAccount+0；见 `RobotPlayer.ArmAttackSequence` / `MShare.IsAttackAttacker`）⇒ = **1**。
# F-E 补丁（m3bag 2026-10-12，第二轮 `-AttackCount 20` 真机冒烟）：旧实现把 attack 也写成 `>= $Count`
# ⇒ `-Bots 2` 时永远等不到第 2 条 done（实测 `attack_seq_done=1/2` 卡到超时/进程退出再报 RED），
# 把**已经跑完整**的序列误判成截断。
function Get-M3SeqExpectedDone {
    param([ValidateSet('bag', 'attack', 'shop', 'quest', 'pairtrade', 'selloff')][string]$Kind, [int]$Bots)
    if ($Kind -eq 'attack') { return 1 }
    return $Bots
}

# 收尾判据唯一口径：done 数达到本序列的预期 done 数。
function Test-M3SeqComplete {
    param([ValidateSet('bag', 'attack', 'shop', 'quest', 'pairtrade', 'selloff')][string]$Kind, [int]$Done, [int]$Bots)
    return ($Done -ge (Get-M3SeqExpectedDone -Kind $Kind -Bots $Bots))
}

if ($SelfCheckNoStack) {
    # F-E（m3cs tick21）：攻击序列「等 M3ATTSEQ done」的收尾计数离线自检（改坏正则 ⇒ 必红）。
    # F-E 补丁（m3bag 2026-10-12）：再加「预期 done 数 / 完备性」两向自检——把 attack 的预期改回 $Bots
    # ⇒ `attack_expected_not_1` + `attack_1of2_not_complete` 必红（1 个 attacker 跑完整就算完整）。
    $fail = @()
    $t2 = 'M3ATTSEQ {"role":"arm","id":"a","count":2}' + "`n" +
          'M3ATTSEQ {"role":"attack","k":1,"id":"a"}' + "`n" +
          'M3ATTSEQ {"role":"done","id":"a","attacks":2}' + "`n" +
          'M3ATTSEQ {"role":"done","id":"b","attacks":2}'
    if ((Get-M3SeqDoneCount -Text $t2 -Kind attack) -ne 2) { $fail += 'attack_done_2' }
    if ((Get-M3SeqDoneCount -Text 'M3ATTSEQ {"role":"done","id":"a","attacks":2}' -Kind attack) -ne 1) { $fail += 'attack_done_1' }
    if ((Get-M3SeqDoneCount -Text 'M3ATTSEQ {"role":"attack","k":1}' -Kind attack) -ne 0) { $fail += 'attack_done_0' }
    if ((Get-M3SeqDoneCount -Text "M3BAGSEQ done`nM3BAGSEQ done" -Kind bag) -ne 2) { $fail += 'bag_done_2' }
    if ((Get-M3SeqDoneCount -Text 'M3BAGSEQ step=0 op=CM_PICKUP' -Kind bag) -ne 0) { $fail += 'bag_done_0' }
    # M3⑤：shop 收尾标记（`M3SHOPSEQ {"phase":"done",...}`）
    if ((Get-M3SeqDoneCount -Text 'M3SHOPSEQ {"phase":"done","ops":7}' -Kind shop) -ne 1) { $fail += 'shop_done_1' }
    if ((Get-M3SeqDoneCount -Text 'M3SHOPSEQ {"phase":"send","k":0,"op":"sell"}' -Kind shop) -ne 0) { $fail += 'shop_done_0' }
    if ((Get-M3SeqExpectedDone -Kind shop -Bots 1) -ne 1) { $fail += 'shop_expected_not_bots' }
    # M3⑥：quest 收尾标记（`M3QUESTSEQ {"phase":"done",...}`）
    if ((Get-M3SeqDoneCount -Text 'M3QUESTSEQ {"phase":"done","steps":4}' -Kind quest) -ne 1) { $fail += 'quest_done_1' }
    if ((Get-M3SeqDoneCount -Text 'M3QUESTSEQ {"phase":"send","k":0,"op":"click_npc"}' -Kind quest) -ne 0) { $fail += 'quest_done_0' }
    if ((Get-M3SeqExpectedDone -Kind quest -Bots 2) -ne 2) { $fail += 'quest_expected_not_bots' }
    if (-not (Test-M3SeqComplete -Kind quest -Done 2 -Bots 2)) { $fail += 'quest_2of2_not_complete' }
    if (Test-M3SeqComplete -Kind quest -Done 1 -Bots 2) { $fail += 'quest_1of2_complete' }
    # M3⑦：组队/交易收尾标记（`M3PAIRTRADESEQ {"phase":"done",...}`；两个假人各一条 ⇒ 预期 = Bots）
    if ((Get-M3SeqDoneCount -Text 'M3PAIRTRADESEQ {"phase":"done","bot":"m3p0","steps":5}' -Kind pairtrade) -ne 1) { $fail += 'pairtrade_done_1' }
    if ((Get-M3SeqDoneCount -Text 'M3PAIRTRADESEQ {"phase":"send","bot":"m3p0","k":0,"op":"deal_try"}' -Kind pairtrade) -ne 0) { $fail += 'pairtrade_done_0' }
    if ((Get-M3SeqExpectedDone -Kind pairtrade -Bots 2) -ne 2) { $fail += 'pairtrade_expected_not_bots' }
    if (-not (Test-M3SeqComplete -Kind pairtrade -Done 2 -Bots 2)) { $fail += 'pairtrade_2of2_not_complete' }
    if (Test-M3SeqComplete -Kind pairtrade -Done 1 -Bots 2) { $fail += 'pairtrade_1of2_complete' }
    # M3⑩：元宝寄售收尾标记（`M3SELLOFFSEQ {"phase":"done",...}`；两个假人各一条 ⇒ 预期 = Bots）
    if ((Get-M3SeqDoneCount -Text 'M3SELLOFFSEQ {"phase":"done","bot":"m3s0","steps":5}' -Kind selloff) -ne 1) { $fail += 'selloff_done_1' }
    if ((Get-M3SeqDoneCount -Text 'M3SELLOFFSEQ {"phase":"send","bot":"m3s0","k":0,"op":"seller_end"}' -Kind selloff) -ne 0) { $fail += 'selloff_done_0' }
    if ((Get-M3SeqExpectedDone -Kind selloff -Bots 2) -ne 2) { $fail += 'selloff_expected_not_bots' }
    if (-not (Test-M3SeqComplete -Kind selloff -Done 2 -Bots 2)) { $fail += 'selloff_2of2_not_complete' }
    if (Test-M3SeqComplete -Kind selloff -Done 1 -Bots 2) { $fail += 'selloff_1of2_complete' }
    # 预期 done 数：attack=1（只有 attacker） / bag=$Bots。
    if ((Get-M3SeqExpectedDone -Kind attack -Bots 2) -ne 1) { $fail += 'attack_expected_not_1' }
    if ((Get-M3SeqExpectedDone -Kind attack -Bots 7) -ne 1) { $fail += 'attack_expected_not_1_multibot' }
    if ((Get-M3SeqExpectedDone -Kind bag -Bots 2) -ne 2) { $fail += 'bag_expected_not_bots' }
    # 完备性两向：2 假人里 1 条 attack done 就算完整；0 条不算；bag 必须 2 条。
    if (-not (Test-M3SeqComplete -Kind attack -Done 1 -Bots 2)) { $fail += 'attack_1of2_not_complete' }
    if (Test-M3SeqComplete -Kind attack -Done 0 -Bots 2) { $fail += 'attack_0of2_complete' }
    if (Test-M3SeqComplete -Kind bag -Done 2 -Bots 2) {} else { $fail += 'bag_2of2_not_complete' }
    if (Test-M3SeqComplete -Kind bag -Done 1 -Bots 2) { $fail += 'bag_1of2_complete' }
    if ($fail.Count -gt 0) {
        Write-Output ('SELFTEST_RED RED: ' + ($fail -join ', '))
        exit 1
    }
    Write-Output 'SELFTEST_RED GREEN: M3 序列收尾计数离线自检全过（attack done 2/1/0 + bag done 2/0 + 预期 attack=1/bag=Bots + 完备性两向）'
    exit 0
}

if ($SelfTestRed) {
    # 红检：指向一个必然不通的端口，驱动必须报失败（退出码 1）
    $Count = 3
    $Port = 17999
    $TimeoutSec = 40
    Write-Output "SELFTEST_RED: 指向 127.0.0.1:17999（无监听），预期失败"
}

if ($LogFile -eq "") {
    $outDir = Join-Path $RepoRoot "tests\botload"
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    $LogFile = Join-Path $outDir ("bots-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".log")
}

# 统计目录：**必须显式给 BotSrv**，不要让它回落到 exe 同级的 load_out.conf。
# 坑（C6 第 4 轮自查实测）：load_gate.ps1 会把 bin/Release/load_out.conf 写成「最后一档」目录且
# 跑完不清除；run_bots 不设 MIR2_BOT_OUT 时，BotSrv 的 LoadMetrics.ResolveOutDir 会读该 conf ⇒
# 3 假人冒烟用 FileMode.Create 覆写 tests/loadgate/load-*/tier-1000/load_stats.ndjson
#（022821 / 114831 / 121254 三处已发生，见 tests/loadgate/README.md「证据完整性告警」）。
# 设了 MIR2_BOT_OUT 后 AppService 把它当显式参数传给 LoadMetrics.Init，优先级压过 conf。
$statsDir = Join-Path (Split-Path -Parent $LogFile) ([System.IO.Path]::GetFileNameWithoutExtension($LogFile))
New-Item -ItemType Directory -Force -Path $statsDir | Out-Null
$statsPath = Join-Path $statsDir "load_stats.ndjson"
$env:MIR2_BOT_OUT = $statsDir

# ---- 1. 构建 ----
if (-not $SkipBuild -or -not (Test-Path $BotExe)) {
    Write-Output "BUILD BotSrv..."
    dotnet build $BotProj -c Release --nologo -v q | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "BotSrv 构建失败" }
}
if (-not (Test-Path $BotExe)) { throw "找不到 $BotExe" }

# ---- 2. 写配置 ----
$cfg = @{
  BotPlay = @{
    ServerName     = $ServerName
    Address        = $Address
    Port           = $Port
    ChrCount       = $Count        # 单批全放（BotSrv 内部还有 3s/个 的软 stagger）
    TotalChrCount  = $Count
    NewAccount     = [bool]$NewAccount
    LoginAccount   = $Prefix
    FixedNewChr    = [bool]$FixedNewChr
    NewChrJob      = $NewChrJob
    NewChrHair     = $NewChrHair
    NewChrSex      = $NewChrSex
    BagEquipSequence    = [bool]$BagEquipSequence
    BagEquipStepMs      = $BagEquipStepMs
    BagEquipStartDelayMs = $BagEquipStartDelayMs
    AttackSequence      = [bool]$AttackSequence
    AttackCount         = $AttackCount
    AttackStepMs        = $AttackStepMs
    AttackStartDelayMs  = $AttackStartDelayMs
    SoftCloseAfterWorldMs = $SoftCloseAfterWorldMs
    DisableAutoPlay     = [bool]$DisableAutoPlay
    StopAfterCharCreated = [bool]$StopAfterCharCreated
    ShopSequence        = [bool]$ShopSequence
    ShopOps             = $ShopOps
    ShopStepMs          = $ShopStepMs
    ShopStartDelayMs    = $ShopStartDelayMs
    ShopNpcName         = $ShopNpcName
    ShopItemName        = $ShopItemName
    ShopItemMake        = $ShopItemMake
    ShopCount           = $ShopCount
    QuestSequence       = [bool]$QuestSequence
    QuestSteps          = $QuestSteps
    QuestStepMs         = $QuestStepMs
    QuestStartDelayMs   = $QuestStartDelayMs
    PairTradeSequence   = [bool]$PairTradeSequence
    PairTradeSteps      = $PairTradeSteps
    PairTradeStepMs     = $PairTradeStepMs
    PairTradeStartDelayMs = $PairTradeStartDelayMs
    PairTradeRole       = $PairTradeRole
    PairTradePeerRole   = $PairTradePeerRole
    SellOffSequence     = [bool]$SellOffSequence
    SellOffSteps        = $SellOffSteps
    SellOffStepMs       = $SellOffStepMs
    SellOffStartDelayMs = $SellOffStartDelayMs
    SellOffSellerRole   = $SellOffSellerRole
    SellOffBuyerRole    = $SellOffBuyerRole
  }
} | ConvertTo-Json -Depth 4
New-Item -ItemType Directory -Force -Path $BotBinDir | Out-Null
[System.IO.File]::WriteAllText((Join-Path $BotBinDir "AppSetting.json"), $cfg, (New-Object System.Text.UTF8Encoding($false)))
Write-Output "CONFIG: $Count bots, prefix=$Prefix, target=$Address`:$Port, NewAccount=$NewAccount, FixedNewChr=$FixedNewChr"
if ($FixedNewChr) {
    Write-Output "CONFIG_FIXED_NEWCHR Job=$NewChrJob Hair=$NewChrHair Sex=$NewChrSex"
}
if ($BagEquipSequence) {
    Write-Output "CONFIG_BAGEQUIPSEQ step_ms=$BagEquipStepMs start_delay_ms=$BagEquipStartDelayMs"
}
if ($AttackSequence) {
    Write-Output "CONFIG_ATTACKSEQ count=$AttackCount step_ms=$AttackStepMs start_delay_ms=$AttackStartDelayMs"
}
if ($SoftCloseAfterWorldMs -gt 0) {
    Write-Output "CONFIG_SOFTCLOSE after_world_ms=$SoftCloseAfterWorldMs"
}
if ($DisableAutoPlay) {
    Write-Output "CONFIG_DISABLE_AUTOPLAY on"
}
if ($StopAfterCharCreated) {
    Write-Output "CONFIG_STOP_AFTER_CHARCREATED on（建角后不进世界）"
}
if ($ShopSequence) {
    Write-Output "CONFIG_SHOPSHQ ops=$ShopOps step_ms=$ShopStepMs start_delay_ms=$ShopStartDelayMs npc=$ShopNpcName item=$ShopItemName make=$ShopItemMake n=$ShopCount"
}
if ($QuestSequence) {
    Write-Output "CONFIG_QUESTSEQ steps=$QuestSteps step_ms=$QuestStepMs start_delay_ms=$QuestStartDelayMs"
}
if ($PairTradeSequence) {
    Write-Output "CONFIG_PAIRTRADESEQ steps=$PairTradeSteps step_ms=$PairTradeStepMs start_delay_ms=$PairTradeStartDelayMs actor=$PairTradeRole peer=$PairTradePeerRole"
}
if ($SellOffSequence) {
    Write-Output "CONFIG_SELLOFFSEQ steps=$SellOffSteps step_ms=$SellOffStepMs start_delay_ms=$SellOffStartDelayMs seller=$SellOffSellerRole buyer=$SellOffBuyerRole"
}

# 无栈自检/红检：只写配置即退（不构建、不起进程）。
if ($WriteConfigOnly) {
    Write-Output ("WRITE_CONFIG_ONLY " + (Join-Path $BotBinDir "AppSetting.json"))
    exit 0
}

# ---- 3. 起进程 ----
$proc = Start-Process -FilePath $BotExe -WorkingDirectory $BotBinDir -PassThru -WindowStyle Hidden `
  -RedirectStandardOutput $LogFile -RedirectStandardError "$LogFile.err"
Write-Output "BOTSRV_PID=$($proc.Id) LOG=$LogFile"

# ---- 4. 读 BotSrv 内部计数（load_stats.ndjson）----
# 判据来源 = BotSrv 自己的 LoadMetrics.LoginOk()（三个分支都计），与 load_gate 同源；
# 不再抓「帐号登录成功」日志行（理由见文件头 .DESCRIPTION）。
$deadline = (Get-Date).AddSeconds($TimeoutSec)
$success = 0
$spawned = 0
$worldOk = 0
# M3 T-14：-BagEquipSequence 开启时，等**整串** `M3BAGSEQ`（含逐个假人的 `M3BAGSEQ done`）落日志后再收工。
# 否则 run_bots 会在 `login_ok == Count` 处立即 break + kill（登录早于进世界/序列），
# 假人没跑完 拾取→装备→卸下→丢弃→用药 就被杀，run_pair 的 bag_equip_markers 判据必红（第九格从未在真栈跑过）。
# M3②③ F-E（m3cs tick21 真机 finding，2026-10-12 12:54）：-AttackSequence 同病——没有「等 M3ATTSEQ done」分支
# ⇒ 攻击序列在登录后立即被 kill，只到 12/120、`done=0`（原始件 E:/tmp/m3cs21-evidence/csharp-stack/run-csharp.txt）。
# 本分支与 -BagEquipSequence 同口径：等本序列的**预期 done 数**（attack=1 个 attacker；bag=$Count）才收工；
# 超时/未 done ⇒ 不 break，末尾判据如实报 attacks/done 并 RED（fail-closed）。
$bagDone = 0
$attackDone = 0
$shopDone = 0
$questDone = 0
$pairTradeDone = 0
$sellOffDone = 0
$attackExpected = Get-M3SeqExpectedDone -Kind attack -Bots $Count
$charCreated = 0
function Read-SharedText([string]$Path) {
  # BotSrv 的 StreamWriter 以 FileShare.ReadWrite 持有：读侧也必须放行写，否则打开即抛
  if (-not (Test-Path $Path)) { return "" }
  try {
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
      $sr = New-Object System.IO.StreamReader($fs)
      return $sr.ReadToEnd()
    } finally { $fs.Close() }
  } catch {
    Write-Output ("WARN: 读统计失败: " + $_.Exception.Message)
    return ""
  }
}
function Get-LastStats([string]$Path) {
  $txt = Read-SharedText $Path
  if ($txt -eq "") { return $null }
  $lines = $txt -split "`n" | Where-Object { $_.Trim() -ne "" }
  if ($lines.Count -eq 0) { return $null }
  try { return ($lines[-1] | ConvertFrom-Json) } catch { return $null }
}
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Seconds 5
  $st = Get-LastStats $statsPath
  if ($st -ne $null) {
    $spawned = [int]$st.spawned
    $success = [int]$st.login_ok
    if ($st.PSObject.Properties.Name -contains "in_world") { $worldOk = [int]$st.in_world }
  }
  if ($BagEquipSequence) {
    # 序列标记由 BotSrv 打到自己的 stdout（= $LogFile），读取同 stats 一样放行写侧。
    $bagDone = Get-M3SeqDoneCount -Text (Read-SharedText $LogFile) -Kind bag
  }
  if ($AttackSequence) {
    # F-E：攻击序列收尾证据 = 每个假人一条 `M3ATTSEQ {"role":"done",...}`；不判它就会 login_ok 处立即 kill。
    $attackDone = Get-M3SeqDoneCount -Text (Read-SharedText $LogFile) -Kind attack
  }
  if ($ShopSequence) {
    # M3⑤：商店序列收尾证据 = 每个假人一条 `M3SHOPSEQ {"phase":"done",...}`；不判它就会在 login_ok 处立即 kill。
    $shopDone = Get-M3SeqDoneCount -Text (Read-SharedText $LogFile) -Kind shop
  }
  if ($QuestSequence) {
    # M3⑥：任务序列收尾证据 = 每个假人一条 `M3QUESTSEQ {"phase":"done",...}`；不判它就会在 login_ok 处立即 kill。
    $questDone = Get-M3SeqDoneCount -Text (Read-SharedText $LogFile) -Kind quest
  }
  if ($PairTradeSequence) {
    # M3⑦：组队/交易序列收尾证据 = 每个假人一条 `M3PAIRTRADESEQ {"phase":"done",...}`；不判它就会在 login_ok 处立即 kill。
    $pairTradeDone = Get-M3SeqDoneCount -Text (Read-SharedText $LogFile) -Kind pairtrade
  }
  if ($SellOffSequence) {
    # M3⑩：元宝寄售序列收尾证据 = 每个假人一条 `M3SELLOFFSEQ {"phase":"done",...}`；不判它就会在 login_ok 处立即 kill。
    $sellOffDone = Get-M3SeqDoneCount -Text (Read-SharedText $LogFile) -Kind selloff
  }
  if ($RequireCharCreated) {
    # 固定起点预热：等「创建角色」标记（BotSrv `SelectChrScene.cs` 的 LogService.Info）
    $charCreated = ([regex]::Matches((Read-SharedText $LogFile), '创建角色')).Count
  }
  $bagNote = if ($BagEquipSequence) { " bag_seq_done=" + $bagDone + "/" + $Count } else { "" }
  $atkNote = if ($AttackSequence) { " attack_seq_done=" + $attackDone + "/" + $attackExpected } else { "" }
  $shopNote = if ($ShopSequence) { " shop_seq_done=" + $shopDone + "/" + $Count } else { "" }
  $questNote = if ($QuestSequence) { " quest_seq_done=" + $questDone + "/" + $Count } else { "" }
  $ptNote = if ($PairTradeSequence) { " pair_trade_seq_done=" + $pairTradeDone + "/" + $Count } else { "" }
  $soNote = if ($SellOffSequence) { " selloff_seq_done=" + $sellOffDone + "/" + $Count } else { "" }
  Write-Output ("PROGRESS spawned=" + $spawned + " login_ok=" + $success + "/" + $Count + " in_world=" + $worldOk + "/" + $Count + $bagNote + $atkNote + $shopNote + $questNote + $ptNote + $soNote + " (src=load_stats.ndjson)")
  if ($BagEquipSequence) {
    # T-14：序列跑完（每个假人一条 done）才算收工；否则继续等（超时上限仍是 TimeoutSec）。
    if ($success -ge $Count -and $bagDone -ge $Count) { break }
  } elseif ($ShopSequence) {
    # M3⑤：商店序列跑完（每个假人一条 done）才算收工。
    if ($success -ge $Count -and (Test-M3SeqComplete -Kind shop -Done $shopDone -Bots $Count)) { break }
  } elseif ($QuestSequence) {
    # M3⑥：任务序列跑完（每个假人一条 done）才算收工。
    if ($success -ge $Count -and (Test-M3SeqComplete -Kind quest -Done $questDone -Bots $Count)) { break }
  } elseif ($PairTradeSequence) {
    # M3⑦：组队/交易序列跑完（每个假人一条 done）才算收工。
    if ($success -ge $Count -and (Test-M3SeqComplete -Kind pairtrade -Done $pairTradeDone -Bots $Count)) { break }
  } elseif ($SellOffSequence) {
    # M3⑩：元宝寄售序列跑完（每个假人一条 done）才算收工。
    if ($success -ge $Count -and (Test-M3SeqComplete -Kind selloff -Done $sellOffDone -Bots $Count)) { break }
  } elseif ($AttackSequence) {
    # F-E：攻击序列跑完（attacker 的 done 到位）才算收工；否则继续等（超时上限仍是 TimeoutSec）。
    if ($success -ge $Count -and (Test-M3SeqComplete -Kind attack -Done $attackDone -Bots $Count)) { break }
  } elseif ($RequireCharCreated) {
    # 固定起点①预热：等到 N 个「创建角色」即 break（不要求进世界）。
    # 另等 load_stats 的 login_ok 到齐：stats 是 BotSrv 每 5s 采样一次，而「创建角色」标记
    # 在登录后立即落日志 ⇒ 只判 charCreated 会在采样边界提前 break，读到「上一拍」的
    # login_ok=0，末尾成功判据（$success -ge $Count）恒假红（T-14 两相 pin 实跑踩到）。
    if ($charCreated -ge $Count -and $success -ge $Count) { break }
  } elseif ($RequireWorld) {
    # F4（T-12）：登录不等于进世界；等"本会话收到 SM_LOGON"的进世界证据到齐。
    if ($worldOk -ge $Count) { break }
  } elseif ($success -ge $Count) { break }
  if ($proc.HasExited) { Write-Output "WARN: BotSrv 进程提前退出 code=$($proc.ExitCode)"; break }
}

$rate = if ($Count -gt 0) { [math]::Round(100.0 * $success / $Count, 2) } else { 0 }
$worldRate = if ($Count -gt 0) { [math]::Round(100.0 * $worldOk / $Count, 2) } else { 0 }
$summary = [pscustomobject]@{
  count = $Count; spawned = $spawned; login_ok = $success; in_world = $worldOk
  bag_seq_done = $bagDone; bag_equip_sequence = [bool]$BagEquipSequence
  attack_seq_done = $attackDone; attack_sequence = [bool]$AttackSequence
  shop_seq_done = $shopDone; shop_sequence = [bool]$ShopSequence
  quest_seq_done = $questDone; quest_sequence = [bool]$QuestSequence
  pair_trade_seq_done = $pairTradeDone; pair_trade_sequence = [bool]$PairTradeSequence
  selloff_seq_done = $sellOffDone; selloff_sequence = [bool]$SellOffSequence
  char_created = $charCreated; require_char_created = [bool]$RequireCharCreated
  success_rate_pct = $rate; world_rate_pct = $worldRate; require_world = [bool]$RequireWorld
  timeout_sec = $TimeoutSec; log = $LogFile; stats = $statsPath
}
$summaryPath = [System.IO.Path]::ChangeExtension($LogFile, ".summary.json")
$summary | ConvertTo-Json | Set-Content -Path $summaryPath -Encoding utf8

if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }

if ($SelfTestRed) {
  if ($success -lt $Count) {
    Write-Output "SELFTEST_RED GREEN: 连不通 => 判失败成立（login_ok=$success/$Count）"
    exit 0   # 红检本身按预期变红 = 自测通过
  }
  Write-Output "SELFTEST_RED RED: 连不通却报成功——判据失效！"
  exit 1
}

if ($success -ge $Count -and (-not $RequireWorld -or $worldOk -ge $Count) -and (-not $BagEquipSequence -or $bagDone -ge $Count) -and (-not $ShopSequence -or (Test-M3SeqComplete -Kind shop -Done $shopDone -Bots $Count)) -and (-not $QuestSequence -or (Test-M3SeqComplete -Kind quest -Done $questDone -Bots $Count)) -and (-not $PairTradeSequence -or (Test-M3SeqComplete -Kind pairtrade -Done $pairTradeDone -Bots $Count)) -and (-not $SellOffSequence -or (Test-M3SeqComplete -Kind selloff -Done $sellOffDone -Bots $Count)) -and (-not $AttackSequence -or $attackDone -ge $attackExpected) -and (-not $RequireCharCreated -or $charCreated -ge $Count)) {
  Write-Output ("GREEN: " + $success + "/" + $Count + " 登录成功 (100%)" + $(if ($RequireWorld) { "；in_world=" + $worldOk + "/" + $Count } else { "" }) + $(if ($BagEquipSequence) { "；bag_seq_done=" + $bagDone + "/" + $Count } else { "" }) + $(if ($ShopSequence) { "；shop_seq_done=" + $shopDone + "/" + $Count } else { "" }) + $(if ($QuestSequence) { "；quest_seq_done=" + $questDone + "/" + $Count } else { "" }) + $(if ($PairTradeSequence) { "；pair_trade_seq_done=" + $pairTradeDone + "/" + $Count } else { "" }) + $(if ($SellOffSequence) { "；selloff_seq_done=" + $sellOffDone + "/" + $Count } else { "" }) + $(if ($AttackSequence) { "；attack_seq_done=" + $attackDone + "/" + $attackExpected } else { "" }) + $(if ($RequireCharCreated) { "；char_created=" + $charCreated + "/" + $Count } else { "" }))
  exit 0
}
if ($SellOffSequence -and -not (Test-M3SeqComplete -Kind selloff -Done $sellOffDone -Bots $Count)) {
  Write-Output ("RED: " + $Count + " 个假人的元宝寄售序列只跑完 " + $sellOffDone + "/" + $Count + "（缺 M3SELLOFFSEQ phase=done）——序列不完整，不能当『同一序列』")
  exit 1
}
if ($PairTradeSequence -and -not (Test-M3SeqComplete -Kind pairtrade -Done $pairTradeDone -Bots $Count)) {
  Write-Output ("RED: " + $Count + " 个假人的组队/交易序列只跑完 " + $pairTradeDone + "/" + $Count + "（缺 M3PAIRTRADESEQ phase=done）——序列不完整，不能当『同一序列』")
  exit 1
}
if ($QuestSequence -and -not (Test-M3SeqComplete -Kind quest -Done $questDone -Bots $Count)) {
  Write-Output ("RED: " + $Count + " 个假人的任务序列只跑完 " + $questDone + "/" + $Count + "（缺 M3QUESTSEQ phase=done）——序列不完整，不能当『同一序列』")
  exit 1
}
if ($ShopSequence -and -not (Test-M3SeqComplete -Kind shop -Done $shopDone -Bots $Count)) {
  Write-Output ("RED: " + $Count + " 个假人的商店序列只跑完 " + $shopDone + "/" + $Count + "（缺 M3SHOPSEQ phase=done）——序列不完整，不能当『同一序列』")
  exit 1
}
if ($BagEquipSequence -and $bagDone -lt $Count) {
  Write-Output ("RED: " + $Count + " 个假人的背包/装备序列只跑完 " + $bagDone + "/" + $Count + "（缺 M3BAGSEQ done）——序列不完整，不能当『同一序列』")
  exit 1
}
if ($AttackSequence -and $attackDone -lt $attackExpected) {
  Write-Output ("RED: 攻击序列只跑完 " + $attackDone + "/" + $attackExpected + "（缺 M3ATTSEQ role=done；count=" + $AttackCount + "）——序列不完整，不能当『同一序列』（attack 预期 done 数 = attacker 数 = 1）")
  exit 1
}
if ($RequireWorld -and $worldOk -lt $Count) {
  Write-Output ("RED: 进世界证据 in_world=" + $worldOk + "/" + $Count + "（" + $worldRate + "%）未达标（-RequireWorld：要求每个假人本会话都收到 SM_LOGON）")
  exit 1
}
if ($RequireCharCreated -and $charCreated -lt $Count) {
  Write-Output ("RED: 建角证据 char_created=" + $charCreated + "/" + $Count + " 未达标（-RequireCharCreated：预热建角没完成）")
  exit 1
}
Write-Output ("RED: 登录成功 " + $success + "/" + $Count + " (" + $rate + "%) 未达标")
exit 1
