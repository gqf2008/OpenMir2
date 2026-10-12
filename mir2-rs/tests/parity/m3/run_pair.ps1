#requires -Version 5
<#
.SYNOPSIS
  run_pair.ps1 — M3「同库同脚本对拍」夹具编排（D2 线）。

.DESCRIPTION
  一次"侧运行"= 同一份操作序列（假人驱动）打在同一套服务端上，产出可归档的基线：
    1. 预检（客户端入口端口在听 / DB 可读 / mysql 客户端在位）
    2. **全新前缀断言**：本次运行的账号/角色命名空间必须为空——
       这保证"同一初始状态"不必靠破坏性还原（活库里既有角色不受影响）
    2.5 预开号（tools/client/account_provision.ps1，account+account_protection 两表，
        否则 LoginSrv 的 INNER JOIN 取不到资料 —— 设计文档 §8.1）
    3. 快照 BEFORE（tools/dbsnap，逐表 sha256）
    4. 跑操作序列（tools/botload，账号=<Prefix><序号>，-NewAccount 先注册）
    5. 快照 AFTER
    6. 抽取本次命名空间内的行（scoped_diff extract）+ 改动清单（delta）
    7. 落 baseline.json：两侧快照 hash、作用域行数、改动清单、操作序列命令、git 提交、时间线

  操作序列判据：以假人日志标记为准（`帐号登录成功` 与 `成功进入游戏` 各 = N），
  **不看 runner 退出码**——BotSrv 进世界后会崩在 `TMap.CanMove`（工具侧已知阻塞，
  见 m3/README.md），崩溃发生在"进入世界之后"，不影响本段操作与 DB 落库。

  两侧对拍（M3 判据）：
    - 先跑 `-Side csharp` 记基线；Rust 侧同一命令 `-Side rust`（先把 7000/7100/7200 指向 Rust 实现）
    - 然后 `python scoped_diff.py diff <csharp>/scoped_after_norm.jsonl <rust>/scoped_after_norm.jsonl`
      **判据：作用域内字段级 diff = 0**（差异要么 0，要么进 tests/parity/whitelist.md）

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_pair.ps1 -Side csharp -Bots 5
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_pair.ps1 -SkipBots     # 只验夹具机制（不产生新行）
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File run_pair.ps1 -SelfTestRed # 红检：入口不通必须判失败
#>
param(
  [ValidateSet('csharp', 'rust')][string]$Side = 'csharp',
  [int]$Bots = 5,
  [string]$Prefix = '',
  [string]$Address = '127.0.0.1',
  [int]$Port = 7000,
  [int]$TimeoutSec = 600,
  [switch]$SkipBots,
  [switch]$KeepSnapshots,
  [switch]$ProvisionSql,
  [switch]$FixedNewChr,            # M3 固定建角场景（卡 openmir2-m3-botsrv-fixed-newchr）
  [int]$NewChrJob = 0,             # 仅 -FixedNewChr 时生效
  [int]$NewChrHair = 2,            # 仅 -FixedNewChr 时生效（2 = 原 Random(1) 恒走的发型）
  [int]$NewChrSex = 0,             # 仅 -FixedNewChr 时生效
  [switch]$BagEquipSequence,       # M3 T-14 夹具：BotSrv 进世界后按固定顺序发背包/装备动作（白名单 T-22）
  [int]$BagEquipStepMs = 1000,     # 仅 -BagEquipSequence 时生效
  [int]$BagEquipStartDelayMs = 3000, # 仅 -BagEquipSequence 时生效
  [switch]$EmitOpsArgs,            # 只打印 run_bots 命令组成（含 -DisableAutoPlay 耦合）后退出；无副作用，供无栈自检
  [switch]$PinStartPose,           # T-14 固定起点变体（owner 86478c55/d7798370）：预热建角 → SQL pin → BEFORE 快照 → 计分跑（选角；不带 -FixedNewChr/-NewAccount）
  [string]$PinMapName = '0',
  [int]$PinHomeMap = 0,
  [int]$PinHomeX = 289,
  [int]$PinHomeY = 618,
  [int]$PinCX = 289,
  [int]$PinCY = 618,
  [int]$PinLevel = 1,
  [int]$PinDir = 0,
  [int]$PinSettleSeconds = 3,      # 仅 -PinStartPose：每轮 pin 后的 settle 窗口（异步掉线存档落库，见 gate_guards.ps1 ⑥）
  [int]$PinMaxAttempts = 5,        # 仅 -PinStartPose：settle+hold 最多轮数（仍 hold 不住 ⇒ throw，fail-closed）
  [switch]$SelfTestRed
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
$Dbsnap = Join-Path $RepoRoot 'tools\dbsnap\dbsnap.py'
$Botload = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
$Scoped = Join-Path $Here 'scoped_diff.py'
# 背包/装备序列口径（T-14 复跑：两侧同一组开关）的唯一形态在 gate_guards.ps1（与 run_m3_gate/selftest 共用）。
. (Join-Path $Here 'gate_guards.ps1')

function Get-M3BotOpPlan {
  param([switch]$ForPrewarm)  # 固定起点变体的①预热相：强制 -FixedNewChr + -NewAccount
  # run_bots 命令组成的**唯一口径**：真实运行与 -EmitOpsArgs 自检共用（避免自检与实际漂移）。
  # 固定起点变体的**计分跑**走「选角」路径（角色已由预热相建出）⇒ 不带 -FixedNewChr/-NewAccount。
  $newAccount = if ($PinStartPose -and -not $ForPrewarm) { $false } else { $true }
  $fixed = if ($ForPrewarm) { $true } elseif ($PinStartPose) { $false } else { [bool]$FixedNewChr }
  $planArgs = @('-Count', $Bots, '-Prefix', $Prefix, '-Address', $Address, '-Port', $Port, '-TimeoutSec', $TimeoutSec)
  # runner 串保持与改动前逐字节一致的形态（-Prefix 后紧跟 -NewAccount 再 -Port），避免基线记录漂移。
  $runner = "run_bots.ps1 -Count $Bots -Prefix $Prefix"
  if ($newAccount) {
    $planArgs += '-NewAccount'
    $runner += ' -NewAccount'
  }
  $runner += " -Port $Port"
  if ($ForPrewarm) {
    # 预热相只需建号：等「创建角色」标记再收工（否则 run_bots 在 login_ok 就到、char 未建）。
    $planArgs += '-RequireCharCreated'
    $runner += ' -RequireCharCreated'
  }
  if ($fixed) {
    $runner += " -FixedNewChr -NewChrJob $NewChrJob -NewChrHair $NewChrHair -NewChrSex $NewChrSex"
    $planArgs += @('-FixedNewChr', '-NewChrJob', $NewChrJob, '-NewChrHair', $NewChrHair, '-NewChrSex', $NewChrSex)
  }
  if ($BagEquipSequence -and -not $ForPrewarm) {
    # T-14 复跑口径（owner 2026-10-12 06:36）：背包/装备序列**必须**一并关掉客户端挂机。
    # 挂机 = 无种子 RNG 的随机走位（ClientManager.RunAutoPlay）⇒ 两侧各走各的，
    # characters.MapName/CX/CY 逐字节比**必然**出 diff（且不是实现差异）。两侧同开关 ⇒ 序列才可比。
    $runner += " -BagEquipSequence -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay"
    $planArgs += @('-BagEquipSequence', '-BagEquipStepMs', $BagEquipStepMs, '-BagEquipStartDelayMs', $BagEquipStartDelayMs, '-DisableAutoPlay')
  }
  return [pscustomobject]@{ runner = $runner; botArgs = $planArgs }
}

function Get-M3StartPoseLabel {
  if (-not $PinStartPose) { return 'none' }
  return (Format-M3StartPose -MapName $PinMapName -HomeMap $PinHomeMap -HomeX $PinHomeX -HomeY $PinHomeY `
    -CX $PinCX -CY $PinCY -Level $PinLevel -Dir $PinDir)
}

if ($SelfTestRed) {
  $Port = 17998
  $Bots = 2
  Write-Output 'SELFTEST_RED: 入口指向 127.0.0.1:17998（无监听），预期夹具判失败'
}

if ([string]::IsNullOrEmpty($Prefix)) {
  # 账号名上限 10 字符（协议字段）：前缀 + 序号位数必须 <= 10
  $Prefix = 'm3' + (Get-Date -Format 'HHmmss')
}
if ($PinStartPose -and -not $FixedNewChr) {
  throw '-PinStartPose 必须与 -FixedNewChr 同用（预热建角用固定输入；计分跑仍标为固定建角场景以便 --fixed-newchr 比 Job/Hair/Sex）'
}

# -EmitOpsArgs：只把 run_bots 的命令组成打印成 JSON 供无栈自检核（-DisableAutoPlay 是否随
# -BagEquipSequence 一并传下去；固定起点变体的预热/计分两相），**不建会话目录/不预检/不取快照**。
if ($EmitOpsArgs) {
  $plan = Get-M3BotOpPlan
  $obj = [ordered]@{
    ops_runner        = $plan.runner
    bot_args          = $plan.botArgs
    bag_equip_scope   = Format-M3BagEquipScope -BagEquipSequence ([bool]$BagEquipSequence) -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay ([bool]$BagEquipSequence)
    start_pose        = (Get-M3StartPoseLabel)
  }
  if ($PinStartPose) {
    $pre = Get-M3BotOpPlan -ForPrewarm
    $obj['prewarm_runner'] = $pre.runner
    $obj['prewarm_bot_args'] = $pre.botArgs
    $obj['pin_sql'] = Format-M3StartPoseSql -ChrName "${Prefix}0" -MapName $PinMapName -HomeMap $PinHomeMap -HomeX $PinHomeX -HomeY $PinHomeY `
      -CX $PinCX -CY $PinCY -Level $PinLevel -Dir $PinDir
    $obj['pin_sql_all'] = @(Get-M3StartPosePinSql -ChrName "${Prefix}0")
    # pin 仪器形态（owner 2026-10-12 08:20）：无栈自检据此机判生产接线是否被削弱（settle_seconds>=1、consecutive>=2）。
    $obj['pin_instrument'] = (Format-M3PinInstrument -SettleSeconds $PinSettleSeconds -MaxAttempts $PinMaxAttempts -Consecutive 2)
  }
  [pscustomobject]$obj | ConvertTo-Json -Depth 4
  exit 0
}

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$SessionDir = Join-Path $RepoRoot ("mir2-rs\tests\parity\m3\sessions\{0}-{1}" -f $Side, $stamp)
New-Item -ItemType Directory -Force -Path $SessionDir | Out-Null
Write-Output ("SESSION " + $SessionDir)
Write-Output ("SIDE={0} PREFIX={1} BOTS={2} ENTRY={3}:{4}" -f $Side, $Prefix, $Bots, $Address, $Port)

function Invoke-Mysql([string]$Sql) {
  $out = & $MysqlExe -h 127.0.0.1 -P 3306 -u root '--password=' --batch --skip-column-names -e $Sql
  if ($LASTEXITCODE -ne 0) { throw "mysql 执行失败: $Sql" }
  return $out
}

function Git-Rev {
  Push-Location $RepoRoot
  try { return (git rev-parse HEAD).Trim() } finally { Pop-Location }
}

# ---- 1. 预检 ----
if (-not (Test-Path $MysqlExe)) { throw "找不到 mysql 客户端: $MysqlExe" }
if ($SkipBots) {
  Write-Output 'PREFLIGHT 跳过入口探测（-SkipBots）'
} else {
  $listening = [bool](Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
  if (-not $listening) {
    if ($SelfTestRed) {
      Write-Output ("SELFTEST_RED GREEN: 入口 {0}:{1} 无监听 => 夹具判失败成立" -f $Address, $Port)
      exit 0
    }
    Write-Output ("RED: 入口 {0}:{1} 没有监听——夹具无法产生操作序列" -f $Address, $Port)
    exit 1
  }
  Write-Output ("PREFLIGHT 入口 {0}:{1} 在听" -f $Address, $Port)
}
$dbOk = Invoke-Mysql 'SELECT 1'
if ($dbOk -notmatch '1') { throw 'DB 不可读' }

# ---- 1.8 运行态记录（放进基线：基线必须写清是哪几个二进制产出的） ----
# 实测（A2 勘察 + D2 复核）：运行的 LoginSrv/DBSrv 来自**仓库构建输出**
# `src/*/bin/Release/`，而 LoginGate/SelGate/GameGate/GameSvr 来自 `E:\MirServer\*`。
# 计划"整栈重启"时必须以这里的实际 exe 为准；LoginSrv 读的配置是
# `src/LoginSrv/bin/Release/logsrv.conf`（LogSrv.ini/config.conf 是遗留副本，不读）。
$runtimeProcs = @()
foreach ($n in @('mysqld', 'DBSrv', 'LoginSrv', 'GameSrv', 'LoginGate', 'SelGate', 'GameGate')) {
  $ps = Get-Process -Name $n -ErrorAction SilentlyContinue
  foreach ($proc in $ps) {
    $runtimeProcs += [pscustomobject]@{
      name = $proc.ProcessName
      path = $proc.Path
      start = $proc.StartTime.ToString('o')
    }
  }
}
$runtimeProcs | ForEach-Object { Write-Output ("RUNTIME {0} <- {1}" -f $_.name, $_.path) }

# ---- 1.9 账号名长度护栏（实测：超 10 字符被协议层截断，登录会报"帐号不存在"） ----
$maxAcct = 10
$longest = $Prefix.Length + ([string]($Bots - 1)).Length
if ($longest -gt $maxAcct) {
  Write-Output ("RED: 账号名最长 {0} 字符 > 上限 {1}（前缀 {2} + 序号）——协议层会静默截断，" -f $longest, $maxAcct, $Prefix)
  Write-Output '     表现为"帐号创建成功"但登录"此帐号不存在"。缩短 -Prefix。'
  exit 1
}
Write-Output ("ACCT_LEN_OK 最长账号名 {0} 字符（上限 {1}）" -f $longest, $maxAcct)

# ---- 2. 全新前缀断言 ----
$accHits = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_account.account WHERE Account LIKE '{0}%'" -f $Prefix)
$chrHits = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName LIKE '{0}%'" -f $Prefix)
Write-Output ("FRESH_CHECK account={0} character={1}" -f $accHits, $chrHits)
if ([int]$accHits -ne 0 -or [int]$chrHits -ne 0) {
  Write-Output ("RED: 前缀 {0} 已存在同名账号/角色——换前缀，或先清理（见 M3 对拍夹具.md 的清理 SQL）" -f $Prefix)
  exit 1
}

# ---- 2.5 固定起点变体（T-14，owner 86478c55/d7798370）：预热建角 → SQL pin → 再取 BEFORE 快照 ----
#   注：本机的 ORACLE C# 侧是冻结部署件（不能注入种子）⇒ 只关挂机仍不够；这一相把角色的
#   MapName/HomeMap/HomeX/Y/CX/CY 钉成固定值，使两侧走「记录里有图有坐标」的路径。
if ($PinStartPose) {
  Write-Output '--- 2.5) 固定起点：预热建角 + SQL pin ---'
  if ($SkipBots) { throw '-PinStartPose 不能与 -SkipBots 同用' }
  $prePlan = Get-M3BotOpPlan -ForPrewarm
  Write-Output ('PREWARM ' + $prePlan.runner)
  & powershell -ExecutionPolicy Bypass -File $Botload @($prePlan.botArgs) 2>&1 |
    Tee-Object -FilePath (Join-Path $SessionDir 'prewarm.log')
  if ($LASTEXITCODE -ne 0) { throw '预热建角失败（run_bots -RequireCharCreated 非 0）' }
  $chrCnt = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName LIKE '{0}%'" -f $Prefix)
  if ([int]$chrCnt -ne $Bots) { throw ("预热后 characters 行数={0}，期望 {1}——预热建角没成功，pin 无目标" -f $chrCnt, $Bots) }
  # 预热相的掉线存档是**异步**的（BotSrv 被杀 ⇒ GameSvr `SaveHumanRcd` → FrontEngine 异步落库）：
  # 实测（2026-10-12 T-14 复跑）pin 后立刻进图，C# 侧拿到的是**预热末态**（`CX/CY=285,622`、
  # 物品 `MakeIndex=36685073…`）而不是 pin 的 `289,618`/`90000001…`——预热存档晚于 pin 才落库、把 pin
  # 覆盖了（与 §5 AFTER 快照前的 settle 同因，注释见下）。
  # 判据统一到 `gate_guards.ps1::Invoke-M3PinSettleHold`（唯一口径，owner 2026-10-12 08:20）：
  #   ① 每轮 pin 后必须**真实 settle**（`Start-Sleep` 后返回 $true，计入 settle 轮数）；
  #   ② **连续两轮** settle 后的复验都到位才算 hold（单次观测挡不住「存档随后覆盖」的假 GREEN）。
  #   ⇒ 跳过 settle 或存档反复覆盖 ⇒ 必判红（throw，fail-closed）。可判红自检见
  #   `run_bag_equip_pair.ps1 -SelfCheckNoStack` ⑩（故障注入驱动同一函数）。
  $pinCtx = [pscustomobject]@{
    Prefix = $Prefix; Bots = $Bots; SettleSeconds = $PinSettleSeconds
    MapName = $PinMapName; HomeMap = $PinHomeMap; HomeX = $PinHomeX; HomeY = $PinHomeY
    CX = $PinCX; CY = $PinCY; Level = $PinLevel; Dir = $PinDir
  }
  $pinResult = Invoke-M3PinSettleHold -Bots $Bots -MaxAttempts $PinMaxAttempts -Context $pinCtx `
    -Pin {
      param($ctx, $attempt)
      for ($i = 0; $i -lt $ctx.Bots; $i++) {
        $pinChar = Format-M3StartPoseSql -ChrName "$($ctx.Prefix)$i" -MapName $ctx.MapName -HomeMap $ctx.HomeMap -HomeX $ctx.HomeX -HomeY $ctx.HomeY `
          -CX $ctx.CX -CY $ctx.CY -Level $ctx.Level -Dir $ctx.Dir
        Invoke-Mysql $pinChar | Out-Null
        # 连「初始背包 + Abil」一起 provision（owner 643459a7）：否则五步无对象可操作。
        foreach ($st in (Get-M3StartPosePinSql -ChrName "$($ctx.Prefix)$i")) { Invoke-Mysql $st | Out-Null }
        Write-Output ('PIN_CHAR ' + $pinChar)
      }
    } `
    -Settle { param($ctx, $attempt) Start-Sleep -Seconds ([int]$ctx.SettleSeconds); return $true } `
    -Observe {
      param($ctx, $attempt)
      # pin 后复验：图/坐标/Level + 4 件初始物品 + Abil Level（三项都到位才算本轮 match）。
      $pinned = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters WHERE ChrName LIKE '{0}%' AND MapName='{1}' AND CX={2} AND CY={3} AND Level={4}" -f $ctx.Prefix, $ctx.MapName, $ctx.CX, $ctx.CY, $ctx.Level)
      $bagPinned = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters_bagitem b JOIN mir2_db.characters c ON b.PlayerId=c.Id WHERE c.ChrName LIKE '{0}%' AND b.StdIndex<>0" -f $ctx.Prefix)
      $abilPinned = Invoke-Mysql ("SELECT COUNT(*) FROM mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id WHERE c.ChrName LIKE '{0}%' AND a.Level={1}" -f $ctx.Prefix, $M3PinAbilLevel)
      return [pscustomobject]@{ Pinned = [int]$pinned; BagPinned = [int]$bagPinned; AbilPinned = [int]$abilPinned }
    }
  foreach ($v in $pinResult.Verdicts) {
    if ($v.hold) { Write-Output ("PIN_HOLD attempt={0}（连续两轮 settle 复验均 hold 住）" -f $v.attempt) }
    else { Write-Output ("PIN_RETRY attempt={0} {1}（{2}）" -f $v.attempt, $v.detail, (@($v.failed) -join ',')) }
  }
  if (-not $pinResult.Hold) {
    $lastV = @($pinResult.Verdicts)[-1]
    throw ("pin 未 hold 住（跳过 settle 或预热掉线存档反复覆盖）：{0}" -f $lastV.detail)
  }
  Write-Output ('PIN_START_POSE ' + (Get-M3StartPoseLabel))
}

# ---- 3. 快照 BEFORE ----
$snapBefore = Join-Path $SessionDir 'snap_before'
python $Dbsnap snap --out $snapBefore | Tee-Object -FilePath (Join-Path $SessionDir 'dbsnap_before.log')
if ($LASTEXITCODE -ne 0) { throw 'snap_before 失败' }

# ---- 3.5 账号准备 ----
# 默认走**服务器自建号**（假人 -NewAccount 注册，LoginSrv 的账号表在线更新，
# account_protection 行由服务端一并写入 —— 实测可登录）。
# `-ProvisionSql` 改为 SQL 直开号，但**必须重启 LoginSrv 后才有用**：
# `AccountStorage.Index()` 查的是启动时加载的内存账号表（实测：SQL 新开的号登录报"此帐号不存在"）。
$provLog = Join-Path $SessionDir 'provision.log'
$Provision = Join-Path $RepoRoot 'tools/client/account_provision.ps1'
if ($SkipBots) {
  'SKIPPED (-SkipBots)' | Set-Content -Path $provLog -Encoding utf8
} elseif ($ProvisionSql) {
  if (-not (Test-Path $Provision)) { throw "找不到开号脚本: $Provision" }
  for ($i = 0; $i -lt $Bots; $i++) {
    $acct = "$Prefix$i"
    & powershell -ExecutionPolicy Bypass -File $Provision -Account $acct 2>&1 |
      Tee-Object -FilePath $provLog -Append
    if ($LASTEXITCODE -ne 0) { throw "开号失败: $acct" }
  }
  Write-Output 'PROVISION_SQL 已 SQL 开号 —— 注意：需重启 LoginSrv（内存账号表）后才可登录'
} else {
  'SERVER-SIDE (-NewAccount during ops)' | Set-Content -Path $provLog -Encoding utf8
}

# ---- 4. 操作序列 ----
$opsLog = Join-Path $SessionDir 'ops.log'
$opsRunner = 'none'
$opsExit = 0
$bagSeqMarkers = @()
if (-not $SkipBots) {
  $plan = Get-M3BotOpPlan
  $opsRunner = $plan.runner
  $botArgs = $plan.botArgs
  if ($FixedNewChr) {
    Write-Output ("OPS_FIXED_NEWCHR Job={0} Hair={1} Sex={2}" -f $NewChrJob, $NewChrHair, $NewChrSex)
  }
  if ($BagEquipSequence) {
    Write-Output ("OPS_BAGEQUIPSEQ step_ms={0} start_delay_ms={1} disable_autoplay=True" -f $BagEquipStepMs, $BagEquipStartDelayMs)
  }
  Write-Output ("OPS " + $opsRunner)
  & powershell -ExecutionPolicy Bypass -File $Botload @botArgs 2>&1 |
    Tee-Object -FilePath $opsLog
  $opsExit = $LASTEXITCODE
  Write-Output ("OPS_EXIT " + $opsExit)
  if ($SelfTestRed) {
    if ($opsExit -ne 0) { Write-Output 'SELFTEST_RED GREEN: 入口不通 => 夹具判失败成立'; exit 0 }
    Write-Output 'SELFTEST_RED RED: 入口不通却报成功——判据失效'; exit 1
  }
  # 判据取日志标记而不是 runner 退出码：BotSrv 进入世界后崩溃在 TMap.CanMove
  #（工具侧已知问题，见 m3/README.md「已知阻塞」），但目前"登录+建角+进世界"这段是可靠的。
  $botLog = (Get-ChildItem (Join-Path $RepoRoot 'tests/botload') -Filter 'bots-*.log' |
             Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
  Copy-Item $botLog (Join-Path $SessionDir 'bot.log') -Force
  $botText = Get-Content $botLog -Raw
  $acct = ([regex]::Matches($botText, '您的帐号创建成功|帐号登录成功')).Count
  $login = ([regex]::Matches($botText, '帐号登录成功')).Count
  $charc = ([regex]::Matches($botText, '创建角色')).Count
  $enter = ([regex]::Matches($botText, '成功进入游戏')).Count
  Write-Output ("OPS_MARKERS login_ok={0}/{1} char_created={2}/{1} enter_world={3}/{1} bot_log={4}" -f $login, $Bots, $charc, $enter, (Split-Path $botLog -Leaf))
  # M3 T-14（T-22）：固定背包/装备序列的执行证据（每一步一条 `M3BAGSEQ` 标记，落 bot.log）。
  # 判据：开了 -BagEquipSequence 就必须看到整串（含 `done`）——否则驱动没跑完，
  # 两侧“同一序列”不成立，绝不能当 GREEN（否则拿一个空序列去 diff 会恒绿）。
  $bagSeqMarkers = @([regex]::Matches($botText, 'M3BAGSEQ [^\r\n]*') | ForEach-Object { $_.Value })
  Write-Output ("OPS_BAGSEQ_MARKERS count={0}" -f $bagSeqMarkers.Count)
  foreach ($m in $bagSeqMarkers) { Write-Output ("BAGSEQ " + $m) }
  if ($BagEquipSequence) {
    $doneCount = ([regex]::Matches($botText, 'M3BAGSEQ done')).Count
    if ($bagSeqMarkers.Count -eq 0) {
      Write-Output 'RED: -BagEquipSequence 开了但 bot.log 里一条 M3BAGSEQ 标记都没有 —— 固定序列没跑（驱动未生效/假人未进世界）'
      exit 1
    }
    if ($doneCount -lt $Bots) {
      Write-Output ("RED: -BagEquipSequence 开了但只有 {0}/{1} 个假人跑完整串（缺 M3BAGSEQ done）——序列不完整，不能当同一序列" -f $doneCount, $Bots)
      exit 1
    }
  }
  # 判据取"可靠段"：账号创建 + 登录成功 = N（实测稳定）。
  # 进世界之后的段位今日不可靠：BotSrv 会在 `TMap.CanMove` 崩溃并带走整个进程
  #（工具侧已知阻塞，见 m3/README.md「已知阻塞」），崩溃点与时序相关（有时停在"确认游戏公告"）。
  # 因此 enter_world 只记录、不判红；M3 要用"打怪/掉落/小退"操作序列前必须先修该崩溃。
  if ($login -lt $Bots) {
    Write-Output ("RED: 假人登录成功 {0}/{1} 未达标，基线不成立" -f $login, $Bots)
    exit 1
  }
  if ($enter -lt $Bots) {
    Write-Output ("WARN: 进世界 {0}/{1} —— BotSrv 崩溃（工具侧已知阻塞），本次基线只覆盖到「登录+建角」" -f $enter, $Bots)
  }
} else {
  ('SKIPPED (-SkipBots)') | Set-Content -Path $opsLog -Encoding utf8
}

# ---- 5. 快照 AFTER ----
# M3 T-14：run_bots 收工时 kill BotSrv ⇒ 服务端从掉线路径走 `SaveHumanRcd`（WorldServer 的非活跃分支），
# 把本段 in-world 改动（装备/卸下/用药）落库。给保存线程一点时间再取 AFTER 快照，否则会漏掉刚发生的改动。
# 仅 -BagEquipSequence 时等待；其余路径逐字节不变（不加额外等待）。
if ($BagEquipSequence) {
  Write-Output 'SETTLE_BAGSEQ 等掉线存档落库（3s）'
  Start-Sleep -Seconds 3
}
$snapAfter = Join-Path $SessionDir 'snap_after'
python $Dbsnap snap --out $snapAfter | Tee-Object -FilePath (Join-Path $SessionDir 'dbsnap_after.log')
if ($LASTEXITCODE -ne 0) { throw 'snap_after 失败' }

# ---- 6. 作用域抽取 + 改动清单 ----
$scopedBefore = Join-Path $SessionDir 'scoped_before.jsonl'
$scopedAfter = Join-Path $SessionDir 'scoped_after.jsonl'
$scopedAfterNorm = Join-Path $SessionDir 'scoped_after_norm.jsonl'
$deltaJson = Join-Path $SessionDir 'delta.json'

python $Scoped extract $snapBefore $Prefix $scopedBefore --allow-empty | Tee-Object -FilePath (Join-Path $SessionDir 'extract_before.log')
if ($LASTEXITCODE -ne 0) { throw 'extract_before 失败' }
python $Scoped extract $snapAfter $Prefix $scopedAfter | Tee-Object -FilePath (Join-Path $SessionDir 'extract_after.log')
if ($LASTEXITCODE -ne 0) { throw 'extract_after 失败' }
python $Scoped extract $snapAfter $Prefix $scopedAfterNorm --normalize | Out-Null
python $Scoped delta $scopedBefore $scopedAfter --out $deltaJson | Tee-Object -FilePath (Join-Path $SessionDir 'delta.log')

# ---- 7. 基线清单 ----
function Snap-Hashes([string]$Dir) {
  $man = Get-Content (Join-Path $Dir 'manifest.json') -Raw | ConvertFrom-Json
  $t = @{}
  foreach ($e in $man.tables) { $t[$e.table] = [pscustomobject]@{ rows = $e.rows; sha256 = $e.sha256 } }
  return $t
}
$beforeHashes = Snap-Hashes $snapBefore
$afterHashes = Snap-Hashes $snapAfter
$delta = Get-Content $deltaJson -Raw | ConvertFrom-Json

$baseline = [pscustomobject]@{
  session      = (Split-Path $SessionDir -Leaf)
  side         = $Side
  prefix       = $Prefix
  bots         = $Bots
  entry        = "$Address`:$Port"
  ops_runner   = $opsRunner
  ops_exit     = $opsExit
  fixed_new_chr = if ($FixedNewChr) { "Job=$NewChrJob Hair=$NewChrHair Sex=$NewChrSex" } else { "(off)" }
  start_pose = (Get-M3StartPoseLabel)
  bag_equip_sequence = (Format-M3BagEquipScope -BagEquipSequence ([bool]$BagEquipSequence) -BagEquipStepMs $BagEquipStepMs -BagEquipStartDelayMs $BagEquipStartDelayMs -DisableAutoPlay ([bool]$BagEquipSequence))
  bag_equip_markers = $bagSeqMarkers
  git_rev      = (Git-Rev)
  dbsnap       = $beforeHashes
  dbsnap_after = $afterHashes
  delta        = $delta
  scoped_rows  = @{
    before = (Get-Content $scopedBefore | Measure-Object -Line).Lines
    after  = (Get-Content $scopedAfter | Measure-Object -Line).Lines
  }
  runtime_procs = $runtimeProcs
  finished_at  = (Get-Date).ToString('o')
}
$baselinePath = Join-Path $SessionDir 'baseline.json'
$baseline | ConvertTo-Json -Depth 8 | Set-Content -Path $baselinePath -Encoding utf8

# 快照 TSV 体积大（活库全表，单次 ~36MB）且可随时重生：默认只留 manifest（sha256 判据本体）。
# 需要原始 TSV 做人工核查时加 -KeepSnapshots。
if (-not $KeepSnapshots) {
  foreach ($d in @($snapBefore, $snapAfter)) {
    Copy-Item (Join-Path $d 'manifest.json') (Join-Path (Split-Path $d -Parent) ((Split-Path $d -Leaf) + '.manifest.json')) -Force
    Remove-Item -Recurse -Force $d -ErrorAction SilentlyContinue
  }
  Write-Output 'SNAPSHOT_TRIMMED 已把 snap_before/snap_after 瘦身为 *.manifest.json（-KeepSnapshots 可保留原始 TSV）'
}

Write-Output ('BASELINE_OK ' + $baselinePath)
Write-Output ('SNAPSHOT_HASHES_BEFORE tables={0}; AFTER tables={1}' -f $beforeHashes.Count, $afterHashes.Count)
exit 0
