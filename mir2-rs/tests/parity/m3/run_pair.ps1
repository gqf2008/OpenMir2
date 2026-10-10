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
  [switch]$SelfTestRed
)

$ErrorActionPreference = 'Stop'
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here '..\..\..\..')).Path
$Dbsnap = Join-Path $RepoRoot 'tools\dbsnap\dbsnap.py'
$Botload = Join-Path $RepoRoot 'tools\botload\run_bots.ps1'
$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
$Scoped = Join-Path $Here 'scoped_diff.py'

if ($SelfTestRed) {
  $Port = 17998
  $Bots = 2
  Write-Output 'SELFTEST_RED: 入口指向 127.0.0.1:17998（无监听），预期夹具判失败'
}

if ([string]::IsNullOrEmpty($Prefix)) {
  # 账号名上限 10 字符（协议字段）：前缀 + 序号位数必须 <= 10
  $Prefix = 'm3' + (Get-Date -Format 'HHmmss')
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
if (-not $SkipBots) {
  $opsRunner = "run_bots.ps1 -Count $Bots -Prefix $Prefix -NewAccount -Port $Port"
  Write-Output ("OPS " + $opsRunner)
  & powershell -ExecutionPolicy Bypass -File $Botload -Count $Bots -Prefix $Prefix -Address $Address -Port $Port -NewAccount -TimeoutSec $TimeoutSec 2>&1 |
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
