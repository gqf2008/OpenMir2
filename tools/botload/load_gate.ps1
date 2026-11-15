#requires -Version 5
<#
.SYNOPSIS
  load_gate.ps1 — 假人压测档位（C6 载荷门禁）：200/500/1000 档，采登录成功率 / 掉线 / tick P99 / 内存曲线。

.DESCRIPTION
  编排（不含业务）：写 BotSrv 配置 → 起 BotSrv（N 个假人）→ 等登录爬坡到 N → 稳态期按秒采内存曲线
  → 停 BotSrv → 用 tools/botload/load_report.py 归算该档指标 → 汇总。

  指标口径见 tools/botload/load_report.py 头部（与 M2 的 p99-baseline **同一份口径**）：
    · tick 服务时延 = 假人收到明文动作帧 `#+GD/<rtime>!` 的时刻 − 帧里带的 rtime
      （两者都是 Environment.TickCount，全机同域）⇒ 反映世界线程/发送队列被拖住的程度
    · 登录成功率 = 稳态窗内 login_ok 峰值 ÷ 档位人数（BotSrv 内部计数，不抓日志行）
    · 掉线 = 登录后 socket 关闭/超时（与"连不上"分开计），判据只看稳态窗
    · 内存 = 各服务进程 WS/私有字节/CPU 时间序列 + 末段斜率

  前置：
    · 服务端全栈已在跑（`E:\MirServer\start-all.ps1`）—— 或用 `-RestartStackPerTier` 让本脚本自己整栈重启
    · 目标账号已建好且**在 LoginSrv 启动之前**建好：
      `tools/client/account_provision.ps1 -Prefix loadbot -Count 1000`
      （LoginSrv 启动时把账号读进内存，之后再建它看不到）

  为什么多档连跑要 `-RestartStackPerTier`（stack_e2e 的多档路径会自动带上）：本工程实测过"网关连接抖动会让 GameSvr 网关槽位 UserList 置空 →
  进世界玩家在 SetGateUserList 上 NRE 死循环"（§C1 交付说明）。一档结束时**成百上千个假人同时断连**
  正是这种抖动 ⇒ 不重启就测下一档，会拿被污染的栈做基线。重启一次约 40s，换口径干净。

  内存护栏（2026-10-11 死机取证后补）：起 BotSrv 前注入进程级 GC 堆硬上限（`-BotHeapLimitGB`，默认 4GiB，
  起进程前打印审计行 `BOTGUARD_ENV …`）；稳态采样**含 BotSrv 自己**，任一被采样进程私有字节/工作集越过
  `-MemKillThresholdGB`（默认 8GiB）即杀 BotSrv + 打 RED `BOTGUARD_MEM_BREACH` 并收尾（mem.ndjson /
  报告照写，绝不静默）；`-MaxRunSec`/`-MaxMemSamples` 给无人值守跑一个显式上界。helper 见
  tools/botload/BotGuard.psm1，离线自检 tools/m5-acceptance/check_botguard.ps1。

  越界 = **环境级异常，提前终止整个门禁**（独立复验者 must_fix #2，2026-10-16）：越界（或全门禁墙钟上界）
  触发后 **不再跑剩余档位**（之前只 break 了内层稳态循环，外层 tier/repeat 会继续并**每档再杀一次 BotSrv**），
  在 `load_gate_summary.json` 的 `bot_guard.aborted_early/abort_reason/abort_note` 里写明"因护栏越界提前终止，
  剩余档位未跑"。

  保护体检（独立复验者 must_fix #1/#2，2026-10-16；B3 收口 2026-10-16）：`-BotHeapLimitGB` 与
  `-MemKillThresholdGB` 两项**都失效**⇒ **拒绝启动**（fail-closed，rc=2）；只失效一项 ⇒ 审计行与 summary
  显式标红该保护缺失。默认配置恒为"有保护"（FULL）。**"有效"不是"取值 > 0"**：一项护栏有效 = 阈值 <
  有效上界（= 机器推导天花板 = `本机提交上限 × 0.5`，或操作者给的更严上界）。`-MemKillThresholdCeilingGB`
  只能把上界调**更严**、**不能放大**（放大值被夹回机器推导值）——见 BotGuard.psm1 ④ 与 check_botguard.ps1 牙⑥。

  判据缺口（卡 `openmir2-loadgate-criteria-gaps`，2026-11-15）：
    · **internal_errors 进判据**（A1）：默认绝对上限 0；超阈判 RED，并打印 top 异常类型/堆栈首行
      （来源 bots.log；BotSrv 只打前 20 条，总量以 load_stats.ndjson 为准）。
    · **白名单逃生口**（总监 2026-11-15 追加）：`tools/botload/internal-errors-allowlist.json`
      按「异常类型 + 消息正则」给已评审的已知无害类别一个 max；max 内不红但报告仍列出，
      超 max / 未列入 / 文件缺失为空 ⇒ RED（容忍度 0）。白名单随卡面 patch 评审，运行时不得改；
      报告记录 path+sha256。当前仓库文件 entries=[]。
    · **BotSrv 崩溃检出**（A2）：进程自己非 0 退出 / `bots.err.log` 含 `Out of memory.` /
      Windows Application Error `0xe0434352` 任一命中 ⇒ 该档 RED；证据落 `bots_runtime.json` 并进产物清单。
      门禁自己 Stop-Process 的退出码不算崩溃。
    · **结论窗口**（A4）：`HoldSec >= 360s` 才是结论级窗口；进世界是串行载入（实测 ~5–7/s），
      <360s 只能当冒烟。report.json 的 `window.conclusion_valid` 写清这一点（不改 verdict）。
    · 每份结论必须显式列 `login / no_resp / internal_errors / OOM`（report.json 的 `required_fields`；
      stdout 每档打 `FIELDS` 行）。判据修好前的历史 GREEN 一律作废。
    红检：`-SelfTestCriteria`（伪注入，不需服务）、`-SelfTestRed`（无监听端口）、`load_report.py selftest`。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -Tiers 200 -HoldSec 60
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -Tiers '200,500,1000' -HoldSec 360 -RestartStackPerTier
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestRed   # 红检：指向无监听端口，必须判红
.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestCriteria  # 红检：OOM/NRE/退出码伪注入
#>
param(
  # 档位。**故意用 string 收**（同 tools/e2e/stack_e2e.ps1）：`powershell -File` 下
  # `-Tiers 200,500,1000` 会被当成带千分位的整数 2005001000（50 万假人）；string 拿到原样文本，
  # `-File` ⇒ "200,500,1000"、`-Command "& … -Tiers 200,500,1000"` ⇒ "200 500 1000"，
  # 都被下面的 `[,\s]+` 拆回数组。见 tests/loadgate/README.md。
  [string]$Tiers = "200,500,1000",
  [int]$HoldSec = 60,
  [int]$RampTimeoutSec = 600,
  [string]$Prefix = "loadbot",
  [string]$ServerName = "热血传奇",
  [string]$Address = "127.0.0.1",
  [int]$Port = 7000,
  [int]$StaggerMs = 20,
  [int]$MemSampleSec = 2,
  [string]$OutDir = "",
  [switch]$NewAccount,
  [switch]$RestartStackPerTier,
  [switch]$SkipBuild,
  [switch]$SelfTestRed,
  [switch]$SelfTestCriteria,             # A3（2026-11-15）：不启动服务，伪造 OOM/NRE/退出码/白名单走真实 load_report CLI，断言必红
  [string]$Criteria = "login,conn,tick,internal",  # 判据子集；M1 ⑤（只比登录+选角）用 "login,conn"。
                                         # internal（A1，2026-11-15）默认在判据里；去掉它的轮次不得作为压测结论
  [switch]$NoActionProbe,                # 关掉假人动作探针（默认开：否则采不到 #+GD ack ⇒ tick 无样本）
  [switch]$AllowMissingTick,             # 显式放行"本档没有 tick 样本"（报告里会写 missing_tick_note）；
                                         # 不加则缺样本一律判 RED——**--Criteria 不能替代本开关**
  [int]$ActionProbeMs = 1000,            # 探针频率：每个假人每 N 毫秒发一个 CM_TURN
  [int]$LoginResendMs = 0,               # T-11 诊断对照：登录包发出后 N ms 内没应答就在同一连接重发一次
                                         # （0=关；只在「残余丢包方向定位」那一轮开，正常门禁不开）
  [int]$Repeat = 1,                      # 同一档位同命令连跑次数（owner 裁决 11dbde39 ①：3 连跑取最差）
  [int]$LoginThresholdPct = 0,           # 登录成功率阈值（%）；0 = 按档位取默认（tier>=1000→95，其余 100）
  # ---- 内存护栏（2026-10-11 死机取证后补）----
  [int]$BotHeapLimitGB = 4,              # 起 BotSrv 前注入的进程级 GC 堆硬上限（GiB）；0=不注入
  [double]$MemKillThresholdGB = 8,       # 看门狗阈值（GiB）：任一被采样进程越过即杀 BotSrv + RED；0=关
  [double]$MemKillThresholdCeilingGB = 0,  # 保护体检上界（GiB）：可选，只能把"有效上界"调**更严**（0=不额外设限，用机推上限）；给成比机推上限大无用（被夹回，见 BotGuard.psm1 ④）
  [int]$MaxRunSec = 10800,               # 全门禁墙钟上界（秒）；到点正常停 BotSrv + 收尾；0=不限
  [int]$MaxMemSamples = 0                # 每档稳态采样迭代上界；0=仅由 HoldSec 决定
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path (Join-Path $Here "..\..")).Path
# 内存护栏共享 helper（进程级堆上限 / 看门狗 / 有界跑）；见 BotGuard.psm1 头部取证说明。
Import-Module (Join-Path $Here "BotGuard.psm1") -Force

# 档位参数归一化 + 防线（2026-10-11 实测：`-File -Tiers 500,1000` 变 5001000，整档跑满 600s 才 RED）：
#   ① 把 string 拆回 [int[]]：`,  空格 都算分隔符 ⇒ `-File`/`-Command` 两种传法都正确；
#   ② 单档数值超出合理上限即**响亮失败**，不再静默把「逗号拼接」当 50 万人的档位跑掉。
$MaxPlausibleTier = 5000
$TierList = @()
if ($Tiers.Trim() -ne "") {
    $TierList = @($Tiers -split '[,\s]+' | Where-Object { $_ -ne "" } | ForEach-Object { [int]$_ })
}
if ($TierList.Count -eq 0) { throw "档位不能为空" }
if ($TierList.Count -eq 1 -and $TierList[0] -gt $MaxPlausibleTier) {
    throw ("档位 " + $TierList[0] + " 超出上限 " + $MaxPlausibleTier +
           "：疑似经 -File 传递时被逗号拼接（200,500,1000 -> 2005001000）。" +
           "请用 -Tiers '200,500,1000'（-File 下加引号）或 -Command ""& 'tools\botload\load_gate.ps1' -Tiers 200,500,1000""。")
}
if (@($TierList | Where-Object { $_ -le 0 }).Count -gt 0) { throw "档位必须为正整数" }
if ($Repeat -lt 1) { throw "-Repeat 必须 >= 1" }

# 登录判据：默认按档位取可复现下界（tier>=1000 为 95%，其余 100%）。
# 为什么 tier-1000 是 95%：被测栈是 C# 部署栈，1000 并发登录下有 ~0-5% 的连接在发出 CM_IDPASSWORD 后
# 服务端一个字节都不回（见 tests/loadgate/README.md《残余丢包方向》）；这是上游性质，不是 Rust 移植缺陷。
function Get-LoginThreshold([int]$tier) {
    if ($LoginThresholdPct -gt 0) { return [double]$LoginThresholdPct }
    if ($tier -ge 1000) { return 95.0 }
    return 100.0
}

$BotBinDir = Join-Path $RepoRoot "src\BotSrv\bin\Release"
$BotExe = Join-Path $BotBinDir "BotSrv.exe"
$StartAll = "E:\MirServer\start-all.ps1"
# BotSrv 也算被采样进程：M5 W2 需要"假人内存斜率"，且看门狗要能看见假人自己越界（2026-10-11 的 106GB 就是它）。
$MemProcNames = @("GameSrv", "LoginSrv", "DBSrv", "LoginGate", "SelGate", "GameGate", "mysqld", "BotSrv")
$MemKillThresholdBytes = if ($MemKillThresholdGB -gt 0) { [int64]($MemKillThresholdGB * 1GB) } else { [int64]0 }
$GuardStart = Get-Date

# 护栏④（must_fix #1，2026-10-16）：保护体检 —— 默认须有保护，任何"零保护"组合都不许悄悄跑起来。
# 放在一切副作用（建目录 / 构建 / 起进程）之前：零护栏时**根本不进入执行**。
$guardProtection = Get-BotGuardProtection -HeapLimitGb $BotHeapLimitGB -MemKillThresholdGb $MemKillThresholdGB `
    -ThresholdCeilingGb $MemKillThresholdCeilingGB
Write-Output (Format-BotGuardProtectionAudit -Protection $guardProtection)
if ($guardProtection.refuse_start) {
    Write-Output ("RED: BotGuard 拒绝启动 —— heap_limit 与 watchdog 同时缺失（bot_heap_limit_gb=" +
                  $BotHeapLimitGB + " mem_kill_threshold_gb=" + $MemKillThresholdGB +
                  " 机推上限=" + $guardProtection.threshold_ceiling_gb + "GiB）；" +
                  "不允许在零护栏下长跑。请至少打开一项：-BotHeapLimitGB > 0 且 < 机推上限，或 0 < -MemKillThresholdGB < 机推上限（本机提交上限 × 0.5）。")
    exit 2
}

if ($SelfTestRed) {
    $TierList = @(3)
    $Port = 17999
    $RampTimeoutSec = 30
    $HoldSec = 5
    Write-Output "SELFTEST_RED: 指向 127.0.0.1:17999（无监听），预期判红"
}

if ($OutDir -eq "") {
    # 默认落在 tests/loadgate/（**可入库**：报告本身是交付物）；大日志在 .gitignore 里排除
    $OutDir = Join-Path $RepoRoot ("tests\loadgate\load-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# ---- A3（2026-11-15）判据红检：不启动任何服务，伪造 OOM/NRE/退出码，走真实 load_report CLI ----
# 目的：证明「注入一次 OOM/NRE 路径（或伪造日志/退出码）⇒ 判据必红；正常跑不变绿」。
# 放在构建/起进程之前 —— 本模式不需要 BotSrv、不需要栈、不需要窗口。
if ($SelfTestCriteria) {
    $stRoot = Join-Path $OutDir "selftest-criteria"
    New-Item -ItemType Directory -Force -Path $stRoot | Out-Null
    $pyReport = Join-Path $RepoRoot "tools\botload\load_report.py"
    $stFails = @()

    function New-FakeStats([string]$Path, [int]$InternalErrors, [bool]$OmitInternalField,
                           [bool]$OmitCensus = $false, [string]$CensusKey = "recv_packet_null",
                           [int]$CensusValue = -1, [string]$ExtraCensusKey = "",
                           [string]$DropOperationalKey = "",
                           [int]$SceneNull = 0, [int]$SceneRounds = 1000,
                           [bool]$OmitSceneRounds = $false, [int]$SceneList = 0,
                           [int]$SceneIndex = 0) {
        $zero = @(0) * 201
        $hist = @(0) * 201
        $hist[5] = 3
        # 卡 openmir2-botsrv-recv-thread-nre：错误类别普查（与 load_report.py / BotErrorCensus.cs 同集）。
        $censusKeys = @(
            "recv_packet_null", "recv_packet_index", "recv_packet_other",
            "run_round_null", "run_round_index", "run_round_other",
            "run_loop_exception", "socket_handler_exception",
            "socket_recv_fault", "socket_send_fault", "socket_connect_fault",
            "metric_flush_failure", "self_report_write_failure",
            "drop_exp_writer_open_failure", "drop_exp_writer_write_failure",
            "bag_item_decode_failure", "unhandled_exception", "unobserved_task_exception",
            "internal_error_unclassified", "teardown_race")
        $census = [ordered]@{}
        foreach ($k in $censusKeys) { $census[$k] = 0 }
        # 默认把「未分类总数」映射到 recv_packet_null，保证 internal_errors == Σ(feeds_internal)；
        # 用例可用 -CensusKey/-CensusValue 覆盖（如 operational 的 teardown_race、非 internal 的 metric_flush）。
        $cv = if ($CensusValue -ge 0) { $CensusValue } else { $InternalErrors }
        if ($census.Contains($CensusKey)) { $census[$CensusKey] = $cv }
        if ($ExtraCensusKey -ne "") { $census[$ExtraCensusKey] = 0 }
        $opKeys = @(
            "conn_refused", "conn_lost", "conn_err_refused", "conn_err_reset", "conn_err_timeout",
            "conn_err_other", "login_send_failed", "login_send_closed", "register_send_closed",
            "partial_frames", "chunks_without_hash", "ident_overflow", "logon_misdelivered",
            "in_world_suppressed", "autoplay_pre_world", "autoplay_skipped_pre_world")
        $rows = @(
            [ordered]@{ t_ms = 1000; login_ok = 0; tick_samples = 0; tick_sum_ms = 0; tick_max_ms = 0; conn_lost = 0; hist = $zero; scene_null_actor_skipped = 0; scene_list_race_skipped = 0; scene_index_race_skipped = 0; scene_rounds = 0 },
            [ordered]@{ t_ms = 2000; login_ok = 3; tick_samples = 0; tick_sum_ms = 0; tick_max_ms = 0; conn_lost = 0; hist = $zero; scene_null_actor_skipped = 0; scene_list_race_skipped = 0; scene_index_race_skipped = 0; scene_rounds = 0 },
            [ordered]@{ t_ms = 5000; login_ok = 3; tick_samples = 3; tick_sum_ms = 15; tick_max_ms = 5; conn_lost = 0; hist = $hist; scene_null_actor_skipped = $SceneNull; scene_list_race_skipped = $SceneList; scene_index_race_skipped = $SceneIndex; scene_rounds = $SceneRounds }
        )
        if ($OmitSceneRounds) { $rows[2].Remove("scene_rounds") }
        foreach ($row in $rows) {
            foreach ($k in $opKeys) {
                if ($k -ne $DropOperationalKey) { $row[$k] = 0 }
            }
            if ($DropOperationalKey -ne "" -and $row.Contains($DropOperationalKey)) {
                $row.Remove($DropOperationalKey)
            }
            if (-not $OmitCensus) {
                $row["error_census_schema"] = 1
                $row["error_census"] = $census
            }
        }
        if (-not $OmitInternalField) {
            $rows[0]["internal_errors"] = 0
            $rows[1]["internal_errors"] = 0
            $rows[2]["internal_errors"] = $InternalErrors
        }
        $lines = @($rows | ForEach-Object { $_ | ConvertTo-Json -Compress -Depth 6 })
        [System.IO.File]::WriteAllLines($Path, $lines, (New-Object System.Text.UTF8Encoding($false)))
    }

    function Invoke-FakeReport([string]$Name, [int]$InternalErrors, [string]$ErrLogText, $Runtime, [string]$BotsLogText, [bool]$OmitInternalField, [string]$AllowlistPath = "",
                               [bool]$OmitCensus = $false, [string]$CensusKey = "recv_packet_null",
                               [int]$CensusValue = -1, [string]$ExtraCensusKey = "",
                               [string]$DropOperationalKey = "",
                               [int]$SceneNull = 0, [int]$SceneRounds = 1000,
                               [bool]$OmitSceneRounds = $false, [int]$SceneList = 0,
                               [int]$SceneIndex = 0) {
        $d = Join-Path $stRoot $Name
        New-Item -ItemType Directory -Force -Path $d | Out-Null
        New-FakeStats (Join-Path $d "load_stats.ndjson") $InternalErrors $OmitInternalField $OmitCensus $CensusKey $CensusValue $ExtraCensusKey $DropOperationalKey $SceneNull $SceneRounds $OmitSceneRounds $SceneList $SceneIndex
        [System.IO.File]::WriteAllText((Join-Path $d "bots.err.log"), $ErrLogText, (New-Object System.Text.UTF8Encoding($false)))
        [System.IO.File]::WriteAllText((Join-Path $d "bots.log"), $BotsLogText, (New-Object System.Text.UTF8Encoding($false)))
        $rtPath = Join-Path $d "bots_runtime.json"
        [System.IO.File]::WriteAllText($rtPath, ($Runtime | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding($false)))
        $outPath = Join-Path $d "report.json"
        $alArgs = @()
        if ($AllowlistPath -ne "") { $alArgs = @("--allowlist", $AllowlistPath) }
        & python $pyReport summarize --stats (Join-Path $d "load_stats.ndjson") --count 3 `
            --hold-sec 360 --criteria "login,conn,tick,internal" `
            --bots-log (Join-Path $d "bots.log") --bots-runtime $rtPath @alArgs --out $outPath | Out-Null
        $rc = $LASTEXITCODE
        $rep = $null
        if (Test-Path $outPath) { $rep = Get-Content $outPath -Raw | ConvertFrom-Json }
        return [pscustomobject]@{ rc = $rc; report = $rep; dir = $d }
    }

    $cleanRuntime = [ordered]@{ self_exited = $false; stopped_by_gate = $true; exit_code = $null; err_log_oom = $false; err_log_oom_lines = @(); app_error_events = @() }
    $oomRuntime = [ordered]@{ self_exited = $false; stopped_by_gate = $true; exit_code = $null; err_log_oom = $true; err_log_oom_lines = @("Out of memory."); app_error_events = @() }
    $exitRuntime = [ordered]@{ self_exited = $true; stopped_by_gate = $false; exit_code = 5; exit_code_hex = "0x00000005"; err_log_oom = $false; err_log_oom_lines = @(); app_error_events = @() }
    $nreLog = "02:00:00.000 [WRN] 假人[loadbot1]异常被兜住[1]：System.NullReferenceException: Object reference not set`n" +
              "   at BotSrv.Scenes.Scene.PlayScene.BeginScene()`n"

    # 1) 干净对照：必须仍是 GREEN（注入不能把正常轮次翻红）
    $r = Invoke-FakeReport "clean" 0 "" $cleanRuntime ""
    if ($r.rc -ne 0 -or $r.report.verdict -ne "GREEN") {
        $stFails += ("clean 对照应 rc=0/GREEN，实得 rc=" + $r.rc + " verdict=" + $r.report.verdict + " fails=" + (($r.report.fails) -join ";"))
    }
    # 2) 伪造 OOM：必须 RED，且 fails 里点名 Out of memory
    $r = Invoke-FakeReport "oom" 0 "Out of memory.`n" $oomRuntime $nreLog
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "Out of memory")) {
        $stFails += ("OOM 伪注入必须 RED 且点名 Out of memory，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 3) 伪造 NRE（internal_errors=5 + 堆栈）：必须 RED，且打印 top 类型
    $r = Invoke-FakeReport "nre" 5 "" $cleanRuntime $nreLog
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "internal_errors 5") -or
        -not (@($r.report.internal_error_diag.types)[0].type -eq "System.NullReferenceException")) {
        $stFails += ("NRE 伪注入必须 RED 且点名 internal_errors/top 类型，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 4) 伪造非 0 退出：必须 RED
    $r = Invoke-FakeReport "exit5" 0 "" $exitRuntime ""
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "非 0 退出")) {
        $stFails += ("伪造非 0 退出必须 RED，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 5) 缺 internal_errors 字段：必须 fail-closed RED
    $r = Invoke-FakeReport "missing-field" 0 "" $cleanRuntime "" $true
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "缺 internal_errors")) {
        $stFails += ("缺 internal_errors 字段必须 fail-closed RED，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 6) A1 逃生口（总监 2026-11-15 追加）：白名单 ≤max 不红但报告列出；超 max/未列入/缺文件/条目非法 ⇒ RED
    $nreLog2 = $nreLog +
        "02:00:01.000 [WRN] 假人[loadbot2]异常被兜住[2]：System.NullReferenceException: Object reference not set`n" +
        "   at BotSrv.Scenes.Scene.PlayScene.BeginScene()`n"
    $oomLog2 = "02:00:01.000 [WRN] 假人[loadbot3]异常被兜住[1]：System.OutOfMemoryException: boom`n" +
               "02:00:01.100 [WRN] 假人[loadbot4]异常被兜住[2]：System.OutOfMemoryException: boom`n"
    $alPath = Join-Path $stRoot "allowlist-nre.json"
    $alDoc = [ordered]@{
        version = 1
        entries = @([ordered]@{
            id = "nre-known"; category = "System.NullReferenceException";
            message_pattern = "Object reference not set"; max = 2;
            reason = "selftest：已知无害"; owner = "selftest"; review_ref = "selftest" })
    }
    [System.IO.File]::WriteAllText($alPath, ($alDoc | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding($false)))
    $alBadPath = Join-Path $stRoot "allowlist-bad.json"
    [System.IO.File]::WriteAllText($alBadPath,
        '{"entries":[{"id":"bad","category":"X","message_pattern":"","max":1,"reason":"r","owner":"o","review_ref":"x"}]}',
        (New-Object System.Text.UTF8Encoding($false)))
    # 6a) 白名单内 ≤max ⇒ GREEN，且报告列出 类别/次数/上限
    $r = Invoke-FakeReport "allow-ok" 2 "" $cleanRuntime $nreLog2 $false $alPath
    $m = @($r.report.internal_error_allowlist.matched)
    if ($r.rc -ne 0 -or $r.report.verdict -ne "GREEN" -or $m.Count -lt 1 -or
        $m[0].type -ne "System.NullReferenceException" -or $m[0].max -ne 2 -or $m[0].count -ne 2) {
        $stFails += ("白名单内 2/2 应 GREEN 且列出 类别/次数/上限，实得 rc=" + $r.rc +
                     " verdict=" + $r.report.verdict + " matched=" + ($m | ConvertTo-Json -Compress))
    }
    # 6b) 白名单内 >max ⇒ RED
    $r = Invoke-FakeReport "allow-over" 5 "" $cleanRuntime $nreLog2 $false $alPath
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "白名单预算 2")) {
        $stFails += ("白名单内 5>max 2 必须 RED，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 6c) 未列入的类别 ⇒ RED（即使别的类别预算很大）
    $r = Invoke-FakeReport "allow-unlisted" 2 "" $cleanRuntime $oomLog2 $false $alPath
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "未列入白名单")) {
        $stFails += ("未列入白名单的类别必须 RED，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 6d) 白名单缺文件/为空 ⇒ 容忍度 0（available=false / budget=0 / 2 次也 RED）
    $r = Invoke-FakeReport "allow-missing" 2 "" $cleanRuntime $nreLog2 $false (Join-Path $stRoot "no-such-allowlist.json")
    $alMiss = $r.report.internal_error_allowlist
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "未列入白名单") -or
        $alMiss.available -ne $false -or $alMiss.budget -ne 0) {
        $stFails += ("白名单缺文件 ⇒ 容忍度 0（available=false/budget=0/2 次也 RED），实得 rc=" + $r.rc +
                     " available=" + $alMiss.available + " budget=" + $alMiss.budget +
                     " fails=" + (($r.report.fails) -join ";"))
    }
    # 6e) 条目非法（空 message_pattern）⇒ 不生效 + 报告 valid=false
    $r = Invoke-FakeReport "allow-invalid" 2 "" $cleanRuntime $nreLog2 $false $alBadPath
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or $r.report.internal_error_allowlist.valid -ne $false) {
        $stFails += ("白名单条目非法 ⇒ 不生效且 RED，实得 rc=" + $r.rc +
                     " valid=" + $r.report.internal_error_allowlist.valid)
    }

    # 7) 卡 openmir2-botsrv-recv-thread-nre：错误类别普查走真实 CLI 的四颗牙。
    # 7a) 缺 error_census ⇒ fail-closed（即使 internal_errors=0）
    $r = Invoke-FakeReport "census-missing" 0 "" $cleanRuntime "" $false "" $true
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "缺 error_census")) {
        $stFails += ("缺 error_census 必须 fail-closed RED，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 7b) 非 feeds_internal 的 fault 类别非 0 且 internal_errors=0 ⇒ RED 且点名类别
    $r = Invoke-FakeReport "census-nonzero" 0 "" $cleanRuntime "" $false "" $false "metric_flush_failure" 1
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "metric_flush_failure=1")) {
        $stFails += ("census 类别 metric_flush_failure=1 必须 RED 且点名，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 7c) 未登记类别（枚举漂移）⇒ RED
    $r = Invoke-FakeReport "census-extra" 0 "" $cleanRuntime "" $false "" $false "recv_packet_null" 0 "brand_new_fault"
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "未登记类别")) {
        $stFails += ("census 多出未登记类别必须 RED，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 7d) 既有事件计数缺失 ⇒ RED（枚举不完整）
    $r = Invoke-FakeReport "census-op-missing" 0 "" $cleanRuntime "" $false "" $false "recv_packet_null" -1 "" "conn_lost"
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or -not (@($r.report.fails) -join ";" -match "缺既有事件计数 conn_lost")) {
        $stFails += ("缺既有事件计数 conn_lost 必须 RED，实得 rc=" + $r.rc + " fails=" + (($r.report.fails) -join ";"))
    }
    # 7e) operational 类别（teardown_race）非 0 不得翻红，但必须在报告逐类列出
    $r = Invoke-FakeReport "census-operational" 0 "" $cleanRuntime "" $false "" $false "teardown_race" 5
    $opRow = @($r.report.error_census.categories | Where-Object { $_.category -eq "teardown_race" })
    if ($r.rc -ne 0 -or $r.report.verdict -ne "GREEN" -or $opRow.Count -ne 1 -or $opRow[0].count -ne 5) {
        $stFails += ("operational teardown_race=5 应 GREEN 且报告列出计数，实得 rc=" + $r.rc +
                     " verdict=" + $r.report.verdict + " row=" + ($opRow | ConvertTo-Json -Compress))
    }

    # 7f) 父卡 scene_null：0 也必须在 required_fields 显式列出；超过 0.5% 无解释 ⇒ RED；
    #     缺 scene_rounds（分母）⇒ RED；list/index 非 0 ⇒ RED。
    $r = Invoke-FakeReport "scene-null-zero" 0 "" $cleanRuntime "" $false "" -SceneNull 0 -SceneRounds 1000
    if ($r.rc -ne 0 -or $r.report.verdict -ne "GREEN" -or
        $r.report.required_fields.scene_null_actor_skipped -ne 0 -or
        $r.report.required_fields.scene_null_skip_status -notin @("within_upper_bound", "no_rounds")) {
        $stFails += ("scene_null=0 必须 GREEN 且 required_fields 显式列出（0 也列），实得 rc=" + $r.rc +
                     " field=" + $r.report.required_fields.scene_null_actor_skipped +
                     " status=" + $r.report.required_fields.scene_null_skip_status)
    }
    $r = Invoke-FakeReport "scene-null-over" 0 "" $cleanRuntime "" $false "" -SceneNull 10 -SceneRounds 1000
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or
        -not (@($r.report.fails) -join ";" -match "scene_null_actor_skipped 稳态占比")) {
        $stFails += ("scene_null 1% ≥ 0.5% 无解释必须 RED，实得 rc=" + $r.rc +
                     " fails=" + (($r.report.fails) -join ";"))
    }
    $r = Invoke-FakeReport "scene-rounds-missing" 0 "" $cleanRuntime "" $false "" -OmitSceneRounds $true
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or
        -not (@($r.report.fails) -join ";" -match "缺 scene 计数 scene_rounds")) {
        $stFails += ("缺 scene_rounds（上界分母）必须 RED，实得 rc=" + $r.rc +
                     " fails=" + (($r.report.fails) -join ";"))
    }
    $r = Invoke-FakeReport "scene-list-nonzero" 0 "" $cleanRuntime "" $false "" -SceneList 1
    if ($r.rc -ne 1 -or $r.report.verdict -ne "RED" -or
        -not (@($r.report.fails) -join ";" -match "scene_list_race_skipped=1")) {
        $stFails += ("scene_list_race_skipped=1 必须 RED，实得 rc=" + $r.rc +
                     " fails=" + (($r.report.fails) -join ";"))
    }

    if ($stFails.Count -gt 0) {
        Write-Output ("SELFTEST_CRITERIA RED: " + ($stFails -join "；"))
        exit 1
    }
    Write-Output "SELFTEST_CRITERIA CLI 注入通过（OOM/NRE/退出码/缺字段/白名单 4 牙/错误类别普查 5 牙/scene_null 4 牙/干净对照）；继续测 Get-BotSrvRuntime 取证"
}

# ---- 构建 ----
if (-not $SkipBuild -and -not $SelfTestCriteria) {
    Write-Output "BUILD BotSrv..."
    dotnet build (Join-Path $RepoRoot "src\BotSrv\BotSrv.csproj") -c Release --nologo -v q | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "BotSrv 构建失败" }
}
if (-not $SelfTestCriteria -and -not (Test-Path $BotExe)) { throw "找不到 $BotExe（先构建 BotSrv）" }

function Read-SharedText([string]$Path) {
    if (-not (Test-Path $Path)) { return "" }
    try {
        $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
        try { return (New-Object System.IO.StreamReader($fs)).ReadToEnd() } finally { $fs.Close() }
    } catch { return "" }
}

function Get-LastStats([string]$Path) {
    $txt = Read-SharedText $Path
    if ($txt -eq "") { return $null }
    $lines = $txt -split "`n" | Where-Object { $_.Trim() -ne "" }
    if ($lines.Count -eq 0) { return $null }
    try { return ($lines[-1] | ConvertFrom-Json) } catch { return $null }
}

function Wait-Port([int]$P, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if (Get-NetTCPConnection -State Listen -LocalPort $P -ErrorAction SilentlyContinue) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return [bool](Get-NetTCPConnection -State Listen -LocalPort $P -ErrorAction SilentlyContinue)
}

# A2（2026-11-15）：BotSrv 崩溃取证。任命中 ⇒ 判据判 RED，证据写进档目录（归档件）：
#   ① 进程自己以非 0 退出（门禁 Stop-Process 的不算）；
#   ② bots.err.log 含 'Out of memory.'；
#   ③ Windows Application Error 0xe0434352（事件日志，时间窗从本档启动开始，进程名 BotSrv.exe）。
# 事件日志查不了时只记 app_error_checked=false + 原因，不单独判红（「没查」≠「没有」）。
function Get-BotSrvRuntime([System.Diagnostics.Process]$Proc, [string]$ErrLogPath, [datetime]$TierStart,
                           [bool]$StoppedByGate) {
    $selfExited = $false
    $exitCode = $null
    $exitHex = $null
    try {
        $Proc.Refresh()
        if ($Proc.HasExited) {
            $selfExited = -not $StoppedByGate
            if ($selfExited) {
                $exitCode = $Proc.ExitCode
                $exitHex = ('0x{0:X8}' -f [uint32](([int64]$exitCode) -band 0xFFFFFFFF))
            }
        }
    } catch { }
    $errText = Read-SharedText $ErrLogPath
    $oomLines = @()
    foreach ($ln in ($errText -split "`r?`n")) {
        if ($ln -match '(?i)out of memory') { $oomLines += $ln.Trim() }
    }
    $appEvents = @()
    $appChecked = $false
    $appErr = $null
    # 自行退出时 WER 可能晚 1–2s 才写事件 ⇒ 查两次（第二次前等 2s）；门禁自停的不重试。
    $appAttempts = if ($selfExited) { 2 } else { 1 }
    for ($appTry = 0; $appTry -lt $appAttempts; $appTry++) {
        if ($appTry -gt 0) { Start-Sleep -Seconds 2 }
        try {
            $evs = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; ProviderName = 'Application Error'; StartTime = $TierStart } -ErrorAction Stop)
            $appChecked = $true
            foreach ($e in $evs) {
                $msg = [string]$e.Message
                if ($msg -match '(?i)BotSrv\.exe' -and $msg -match '(?i)0xe0434352') {
                    $appEvents += [ordered]@{
                        time_created = $e.TimeCreated.ToString('o')
                        id           = $e.Id
                        code         = '0xe0434352'
                        message_head = (($msg -split "`r?`n") | Select-Object -First 4) -join ' | '
                    }
                }
            }
            if ($appEvents.Count -gt 0) { break }
        } catch {
            if ($_.Exception.Message -match '(?i)No events were found') {
                $appChecked = $true   # 查询成功但没有匹配事件
            } else {
                $appErr = $_.Exception.Message
            }
        }
    }
    $oomHits = $oomLines.Count + $appEvents.Count
    $crash = (($selfExited) -and ($null -ne $exitCode) -and ([int64]$exitCode -ne 0)) -or
             ($oomLines.Count -gt 0) -or ($appEvents.Count -gt 0)
    return [ordered]@{
        source                = 'load_gate'
        pid                   = $Proc.Id
        exe                   = $BotExe
        tier_start            = $TierStart.ToString('o')
        checked_at            = (Get-Date).ToString('o')
        self_exited           = $selfExited
        stopped_by_gate       = $StoppedByGate
        exit_code             = $exitCode
        exit_code_hex         = $exitHex
        err_log               = $ErrLogPath
        err_log_oom           = ($oomLines.Count -gt 0)
        err_log_oom_lines     = @($oomLines | Select-Object -First 5)
        app_error_checked     = $appChecked
        app_error_check_error = $appErr
        app_error_events      = @($appEvents)
        oom_hits              = $oomHits
        crash                 = [bool]$crash
    }
}

function Save-BotSrvRuntime([string]$Dir, $Runtime) {
    $p = Join-Path $Dir 'bots_runtime.json'
    [System.IO.File]::WriteAllText($p, ($Runtime | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding($false)))
    return $p
}

# A3 第二段（2026-11-15）：用真实进程测 Get-BotSrvRuntime 的取证接线——
# 退出码、OOM 日志、以及「门禁自己 Stop-Process 不算崩溃」。不启动服务。
if ($SelfTestCriteria) {
    $rcFails = @()
    $tmpd = Join-Path $OutDir "selftest-criteria\runtime-collector"
    New-Item -ItemType Directory -Force -Path $tmpd | Out-Null
    $cleanErr = Join-Path $tmpd "clean.err.log"
    [System.IO.File]::WriteAllText($cleanErr, "", (New-Object System.Text.UTF8Encoding($false)))
    # 事件日志时间窗放到未来 ⇒ 不让本机其它车道的 BotSrv 事件混进自测（隔离）。
    $future = (Get-Date).AddDays(1)
    function Start-ExitProbe([int]$Code) {
        $p = Start-Process -FilePath "cmd.exe" -ArgumentList @("/c", "exit $Code") -PassThru -WindowStyle Hidden
        $p.WaitForExit()
        return $p
    }
    $p0 = Start-ExitProbe 0
    $rt0 = Get-BotSrvRuntime -Proc $p0 -ErrLogPath $cleanErr -TierStart $future -StoppedByGate $false
    if ($rt0.crash -or (-not $rt0.self_exited) -or ($rt0.exit_code -ne 0)) {
        $rcFails += ("exit0 应为 self_exited=true/exit_code=0/crash=false，实得 " + ($rt0 | ConvertTo-Json -Compress))
    }
    $p5 = Start-ExitProbe 5
    $rt5 = Get-BotSrvRuntime -Proc $p5 -ErrLogPath $cleanErr -TierStart $future -StoppedByGate $false
    if ((-not $rt5.crash) -or ($rt5.exit_code -ne 5)) {
        $rcFails += ("exit5 应为 crash=true/exit_code=5，实得 " + ($rt5 | ConvertTo-Json -Compress))
    }
    $oomErr = Join-Path $tmpd "oom.err.log"
    [System.IO.File]::WriteAllText($oomErr, "Out of memory.`n", (New-Object System.Text.UTF8Encoding($false)))
    $rtOom = Get-BotSrvRuntime -Proc $p0 -ErrLogPath $oomErr -TierStart $future -StoppedByGate $true
    if ((-not $rtOom.crash) -or (-not $rtOom.err_log_oom) -or ($rtOom.oom_hits -lt 1)) {
        $rcFails += ("OOM 日志应 crash=true/err_log_oom=true/oom_hits>=1，实得 " + ($rtOom | ConvertTo-Json -Compress))
    }
    $p6 = Start-ExitProbe 6
    $rtGate = Get-BotSrvRuntime -Proc $p6 -ErrLogPath $cleanErr -TierStart $future -StoppedByGate $true
    if ($rtGate.crash -or $rtGate.self_exited) {
        $rcFails += ("stopped_by_gate=true 时不得判崩溃/不得记 self_exited，实得 " + ($rtGate | ConvertTo-Json -Compress))
    }
    if ($rcFails.Count -gt 0) {
        Write-Output ("SELFTEST_CRITERIA RED: " + ($rcFails -join "；"))
        exit 1
    }
    Write-Output ("SELFTEST_CRITERIA GREEN: OOM/NRE/退出码注入均判红，白名单 4 牙（≤max/超max/未列入/缺文件）通过，" +
                  "缺字段 fail-closed，干净对照仍 GREEN；Get-BotSrvRuntime 取证（exit0/exit5/OOM/门禁自停）通过")
    exit 0
}

# 本机负载探针（owner 裁决 82f8e0c3 ④；口径照 tickbench 的 `probe_load_window`：
# CPU 均值（%）+ 重型进程数 rustc/cargo/cargo-clippy/msbuild）。它只**标注**不判红：
# tick P99 在共享机器上不可归属（C# 参考栈 vs 同机并发编译），需要空载窗口才能定。
function Get-HostLoadLine([long]$ts) {
    $cpu = -1
    try { $cpu = [int]((Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average) } catch { $cpu = -1 }
    $heavy = @(Get-Process -Name rustc,cargo,cargo-clippy,msbuild -ErrorAction SilentlyContinue)
    $names = (@($heavy | ForEach-Object { $_.ProcessName }) | Sort-Object -Unique) -join ","
    return ('{{"t_ms":{0},"cpu_pct":{1},"heavy_procs":{2},"heavy_names":"{3}"}}' -f $ts, $cpu, $heavy.Count, $names)
}

function Restart-Stack {
    if (-not (Test-Path $StartAll)) { throw "找不到 $StartAll（-RestartStackPerTier 需要它）" }
    Write-Output "STACK restart: stop -> start"
    # 坑（2026-10-11 实测）：**不要把 start-all.ps1 的输出接成管道再等它退出**——它用 Start-Process
    # 起的七个服务进程会继承那个管道的写端，父进程退出后管道仍不关闭 ⇒ 调用方永久挂住
    # （上一批的"多档连跑卡死/要杀任务树"就是它）。做法改成：
    #   ① stop 重定向到文件；② start 用 Start-Process 起在独立进程 + 输出重定向到文件；
    #   ③ 只等**端口就绪**（栈健康与否本来就看端口，不等子进程树）。
    & powershell -ExecutionPolicy Bypass -File $StartAll -Stop *> (Join-Path $OutDir "stack-stop.log")
    Start-Process -FilePath "powershell" `
        -ArgumentList @('-ExecutionPolicy', 'Bypass', '-File', $StartAll) -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $OutDir "stack-start.out.log") `
        -RedirectStandardError (Join-Path $OutDir "stack-start.err.log") | Out-Null
    if (-not (Wait-Port $Port 150)) { throw "整栈重启后端口 $Port 未就绪（见 $OutDir\stack-start.*.log）" }
}

$guardBreachAny = $false   # 任一档触发内存看门狗（越界）
$guardBoundAny  = $false   # 任一档触发有界跑上界
$crashAny       = $false   # 任一档检出 BotSrv 崩溃证据（A2，2026-11-15）
# 越界 / 全门禁墙钟上界 = 环境级异常 ⇒ 提前终止整个门禁（停掉后续档位），而不是只 break 内层稳态循环
# （must_fix #2：原实现只 break 内层，外层 tier/repeat 仍继续、每档再杀一次 BotSrv）。
$guardAbortReason = $null  # 'mem_breach' | 'wall_bound' | $null
$tierReports = @()
$guardTiersRun = 0
:runLoop for ($run = 1; $run -le $Repeat; $run++) {
if ($Repeat -gt 1) { Write-Output ("===== REPEAT " + $run + "/" + $Repeat + " =====") }
:tierLoop foreach ($n in $TierList) {
    $guardTiersRun++
    # 三档基线一律从**干净栈**开始（含第一档）：上一批的实测教训是"同一条栈连跑两档，
    # 第二档假人能登录但进不了世界"。整栈重启 ~60s，比拿被污染的栈当基线便宜。
    if ($RestartStackPerTier) { Restart-Stack }

    $tierDir = if ($Repeat -gt 1) { Join-Path $OutDir ("run-" + $run + "\tier-" + $n) } else { Join-Path $OutDir ("tier-" + $n) }
    New-Item -ItemType Directory -Force -Path $tierDir | Out-Null
    $loginThr = Get-LoginThreshold $n
    $rampTarget = [int][math]::Ceiling($n * $loginThr / 100.0)
    Write-Output ("==== TIER " + $n + " (stagger=" + $StaggerMs + "ms, hold=" + $HoldSec + "s, login_threshold=" + $loginThr + "% => ramp_target=" + $rampTarget + "/" + $n + ") -> " + $tierDir)

    # ---- BotSrv 配置 ----
    $cfg = @{
      BotPlay = @{
        ServerName       = $ServerName
        Address          = $Address
        Port             = $Port
        ChrCount         = $n
        TotalChrCount    = $n
        NewAccount       = [bool]$NewAccount
        LoginAccount     = $Prefix
        ConnectStaggerMs = $StaggerMs
      }
    } | ConvertTo-Json -Depth 4
    [System.IO.File]::WriteAllText((Join-Path $BotBinDir "AppSetting.json"), $cfg, (New-Object System.Text.UTF8Encoding($false)))
    # 输出目录：环境变量 + exe 同级 load_out.conf 双保险（经验：Env 在托管启动下不一定继承）
    [System.IO.File]::WriteAllText((Join-Path $BotBinDir "load_out.conf"), $tierDir, (New-Object System.Text.UTF8Encoding($false)))
    $env:MIR2_BOT_OUT = $tierDir
    # 动作探针（tick 样本来源）：默认开。BotSrv 的挂机/移动层是跨假人共享全局态，N 假人动不了，
    # 探针只依赖假人自己的 socket + 场景状态，进世界后每 $ActionProbeMs 发一个 CM_TURN ⇒ 服务端回 #+GD/<rtime>!
    if ($NoActionProbe) { $env:MIR2_BOT_ACTION_PROBE = "0" } else { $env:MIR2_BOT_ACTION_PROBE = "1" }
    $env:MIR2_BOT_ACTION_PROBE_MS = "$ActionProbeMs"
    # T-11 登录重发对照（默认 0=关，保持正常门禁行为不变）
    $env:MIR2_BOT_LOGIN_RESEND_MS = "$LoginResendMs"

    # 护栏①：起进程前注入进程级 GC 堆硬上限，并把最终生效的 env 打出来（可审计）
    $envOverrides = New-BotGuardEnvOverrides -HeapLimitGb $BotHeapLimitGB
    Set-BotGuardEnv -Overrides $envOverrides
    Write-Output ("  " + (Format-BotGuardEnvAudit -Exe $BotExe -Overrides $envOverrides -HeapLimitGb $BotHeapLimitGB))

    $botLog = Join-Path $tierDir "bots.log"
    $botErrLog = Join-Path $tierDir "bots.err.log"
    $tierStart = Get-Date
    $proc = Start-Process -FilePath $BotExe -WorkingDirectory $BotBinDir -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput $botLog -RedirectStandardError $botErrLog
    Write-Output ("BOTSRV_PID=" + $proc.Id)

    # ---- 爬坡：等登录数到 N ----
    $statsPath = Join-Path $tierDir "load_stats.ndjson"
    $hostLoadPath = Join-Path $tierDir "hostload.ndjson"
    $hostLoadLines = New-Object System.Collections.Generic.List[string]
    $rampDeadline = (Get-Date).AddSeconds($RampTimeoutSec)
    $rampOk = $false
    while ((Get-Date) -lt $rampDeadline) {
        Start-Sleep -Seconds 2
        $tsRamp = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
        $hostLoadLines.Add((Get-HostLoadLine $tsRamp))
        $st = Get-LastStats $statsPath
        if ($st -ne $null) {
            Write-Output ("  RAMP login_ok=" + $st.login_ok + "/" + $n + " spawned=" + $st.spawned + " tick_samples=" + $st.tick_samples)
            if ([int]$st.login_ok -ge $rampTarget) { $rampOk = $true; break }
        }
        # A2（2026-11-15）：爬坡期 BotSrv 自己退出 ⇒ RED（旧实现只打 WARN）。
        # 证据在档末统一取证（bots_runtime.json），这里先停止等待。
        if ($proc.HasExited) {
            Write-Output ("  RED BotSrv 在爬坡期退出 code=" + $proc.ExitCode + "（崩溃判据见档末 bots_runtime.json）")
            break
        }
        # 护栏④：有界跑——墙钟上界（爬坡阶段也算），到点即停 BotSrv 收尾
        $elapsedRamp = ((Get-Date) - $GuardStart).TotalSeconds
        if (Test-BotRunBound -ElapsedSec $elapsedRamp -MaxRunSec $MaxRunSec -Iteration 0 -MaxIterations 0) {
            $guardBoundAny = $true
            $guardAbortReason = 'wall_bound'
            Write-Output ("  BOTGUARD_BOUND wall-clock during ramp: elapsed=" + [int]$elapsedRamp + "s max=" + $MaxRunSec + "s")
            if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
            break
        }
    }
    if (-not $rampOk) { Write-Output ("  RED 爬坡未达阈值（" + $RampTimeoutSec + "s 内未到 " + $rampTarget + "/" + $n + "）") }

    # ---- 稳态采样：内存曲线（tick 由 BotSrv 自己持续落盘）----
    $memPath = Join-Path $tierDir "mem.ndjson"
    $memLines = New-Object System.Collections.Generic.List[string]
    $holdDeadline = (Get-Date).AddSeconds($HoldSec)
    $memSampleIter = 0
    $memBreach = $null
    while ((Get-Date) -lt $holdDeadline) {
        # A2（2026-11-15）：稳态采样期间也必须看进程死活。旧实现只在爬坡循环看 ⇒
        # supp-hold360 的 BotSrv 在 02:59:39 OOM 后，门禁又采样了 ~90s 并用 tick P99 给 RED，OOM 无痕迹。
        try { $proc.Refresh() } catch { }
        if ($proc.HasExited) {
            Write-Output ("  RED BotSrv 在稳态期退出 code=" + $proc.ExitCode + "（崩溃判据见档末 bots_runtime.json）")
            break
        }
        $ts = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
        $hostLoadLines.Add((Get-HostLoadLine $ts))
        # 本轮采样：既落 mem.ndjson（含 BotSrv），也喂给看门狗做越界判定
        $iterSamples = New-Object System.Collections.Generic.List[object]
        foreach ($name in $MemProcNames) {
            $ps = Get-Process -Name $name -ErrorAction SilentlyContinue
            foreach ($p in $ps) {
                $ws = [math]::Round($p.WorkingSet64 / 1MB, 2)
                $pv = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
                $cpu = $p.TotalProcessorTime.TotalSeconds
                $memLines.Add(('{{"t_ms":{0},"proc":"{1}","pid":{2},"ws_mb":{3},"priv_mb":{4},"cpu_s":{5}}}' -f $ts, $name, $p.Id, $ws, $pv, [math]::Round($cpu, 2)))
                $iterSamples.Add([pscustomobject]@{ proc = $name; pid = $p.Id; priv_bytes = [int64]$p.PrivateMemorySize64; ws_bytes = [int64]$p.WorkingSet64 })
            }
        }
        $memSampleIter++
        # 护栏②：看门狗——任一被采样进程越界 ⇒ 杀 BotSrv + RED + 提前终止外层档位循环（绝不静默）
        $breach = Find-MemBreach -Samples $iterSamples -ThresholdBytes $MemKillThresholdBytes
        if ($breach) {
            $memBreach = $breach
            $guardBreachAny = $true
            $guardAbortReason = 'mem_breach'
            Write-Output ("  " + (Format-BotGuardBreachRed -Breach $breach))
            foreach ($kid in @(Stop-BotSrvForBreach -Breach $breach -BotPids @($proc.Id))) { Write-Output ("  BOTGUARD_KILL pid=" + $kid) }
            break
        }
        # 护栏④：有界跑——每档稳态采样迭代上界
        $iterBound = Test-BotRunBound -ElapsedSec 0 -MaxRunSec 0 -Iteration $memSampleIter -MaxIterations $MaxMemSamples
        if ($iterBound) {
            $guardBoundAny = $true
            Write-Output ("  BOTGUARD_BOUND iterations: sample_iter=" + $memSampleIter + " max=" + $MaxMemSamples)
            break
        }
        # 护栏④：有界跑——全门禁墙钟上界
        $elapsed = ((Get-Date) - $GuardStart).TotalSeconds
        $wallBound = Test-BotRunBound -ElapsedSec $elapsed -MaxRunSec $MaxRunSec -Iteration 0 -MaxIterations 0
        if ($wallBound) {
            $guardBoundAny = $true
            $guardAbortReason = 'wall_bound'
            Write-Output ("  BOTGUARD_BOUND wall-clock: elapsed=" + [int]$elapsed + "s max=" + $MaxRunSec + "s")
            if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
            break
        }
        Start-Sleep -Seconds $MemSampleSec
    }
    [System.IO.File]::WriteAllLines($memPath, $memLines)
    [System.IO.File]::WriteAllLines($hostLoadPath, $hostLoadLines)

    # ---- 停 BotSrv（收尾前再读一次统计）----
    # A2：门禁自己杀的不算「非 0 退出」；但 bots.err.log / Application Error 仍要查（OOM 可能刚发生）。
    $stoppedByGate = $false
    try { $proc.Refresh() } catch { }
    if (-not $proc.HasExited) {
        $stoppedByGate = $true
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    }
    Start-Sleep -Seconds 1
    $botRuntime = Get-BotSrvRuntime -Proc $proc -ErrLogPath $botErrLog -TierStart $tierStart -StoppedByGate $stoppedByGate
    $botRuntimePath = Save-BotSrvRuntime $tierDir $botRuntime
    if ($botRuntime.crash) {
        $crashAny = $true
        Write-Output ("  RED BOTSRV_CRASH self_exited=" + $botRuntime.self_exited +
                      " exit_code=" + $botRuntime.exit_code + " (" + $botRuntime.exit_code_hex + ")" +
                      " oom_hits=" + $botRuntime.oom_hits +
                      " app_error_events=" + @($botRuntime.app_error_events).Count +
                      " evidence=" + $botRuntimePath)
    }

    # ---- 归算 ----
    $reportPath = Join-Path $tierDir "report.json"
    $criteriaArgs = @("--criteria", $Criteria, "--login-threshold", "$loginThr",
                      "--bots-runtime", $botRuntimePath)
    if (Test-Path $botLog) { $criteriaArgs += @("--bots-log", $botLog) }
    if ($Criteria -notmatch '(^|,)\s*internal\s*(,|$)') {
        Write-Output "  WARN criteria 未含 internal —— 本轮不得作为压测结论（GREEN 无效）"
    }
    $loginConnsPath = Join-Path $tierDir "login_conns.ndjson"
    if (Test-Path $loginConnsPath) { $criteriaArgs += @("--login-conns", $loginConnsPath) }
    if (Test-Path $hostLoadPath) { $criteriaArgs += @("--hostload", $hostLoadPath) }
    if ($AllowMissingTick) { $criteriaArgs += "--allow-missing-tick" }
    python (Join-Path $RepoRoot "tools\botload\load_report.py") summarize --stats $statsPath --mem $memPath `
        --count $n --stagger-ms $StaggerMs --hold-sec $HoldSec @criteriaArgs --out $reportPath | Out-Null
    $tierRc = $LASTEXITCODE
    $tierJson = $null
    if (Test-Path $reportPath) { $tierJson = Get-Content $reportPath -Raw | ConvertFrom-Json }
    if ($tierJson -ne $null) {
        $tierJson | Add-Member -NotePropertyName run -NotePropertyValue $run -Force
        Write-Output ("  RESULT tier=" + $n + " verdict=" + $tierJson.verdict +
                      " login=" + $tierJson.login_success_pct + "% conn_lost_steady=" + $tierJson.conn_lost_steady +
                      " internal_errors=" + $tierJson.internal_errors +
                      " tick_p50=" + $tierJson.tick_p50_ms + " p90=" + $tierJson.tick_p90_ms +
                      " p99=" + $tierJson.tick_p99_ms + "ms samples=" + $tierJson.samples +
                      " in_world=" + $tierJson.world_ok + " probe_sent=" + $tierJson.probe_sent)
        # 总监硬规矩（2026-11-15）：每份结论必须显式列 login / no_resp / internal_errors / OOM。
        $rf = $tierJson.required_fields
        $noResp = if ($null -ne $rf -and $null -ne $rf.login_conn_no_resp) { [string]$rf.login_conn_no_resp } else { "NA(证据缺)" }
        $oomHits = if ($null -ne $rf -and $null -ne $rf.oom_hits) { [string]$rf.oom_hits } else { "NA(证据缺)" }
        Write-Output ("  FIELDS tier=" + $n + " login=" + $tierJson.login_success_pct + "% no_resp=" + $noResp +
                      " internal_errors=" + $tierJson.internal_errors + " OOM=" + $oomHits +
                      " allowlist_budget=" + $(if ($null -ne $rf) { [string]$rf.internal_errors_allowlist_budget } else { "NA" }) +
                      " internal_excused=" + $(if ($null -ne $rf) { [string]$rf.internal_errors_excused } else { "NA" }) +
                      " hold_sec=" + $tierJson.hold_sec + " window_valid=" + $tierJson.window.conclusion_valid +
                      " verdict=" + $tierJson.verdict)
        if ($tierJson.host_load -ne $null) {
            $hl = $tierJson.host_load
            Write-Output ("  HOSTLOAD cpu_mean=" + $hl.cpu_mean_pct + "% cpu_peak=" + $hl.cpu_peak_pct +
                          "% heavy_max=" + $hl.heavy_procs_max + " names=" + ($hl.heavy_names_seen -join ","))
        }
        if ($tierJson.mem.PSObject.Properties.Name -contains "GameSrv") {
            $g = $tierJson.mem.GameSrv
            Write-Output ("  MEM GameSvr ws_peak=" + $g.ws_peak_mb + "MB mean=" + $g.ws_mean_mb +
                          "MB slope_tail=" + $g.slope_tail_mb_per_s + "MB/s")
        }
        $tierReports += $tierJson
    } else {
        Write-Output "  RED 归算没产出 report.json"
    }
    if ($tierRc -ne 0) { Write-Output ("  (tier rc=" + $tierRc + ")") }
    # 越界 / 墙钟上界 ⇒ 停掉后续档位（环境级异常；不再对下一档重复起停 BotSrv）
    if ($guardAbortReason) { break tierLoop }
}
if ($guardAbortReason) { break runLoop }
}

# ---- 汇总 ----
# 同命令多连跑取最差（owner 裁决 11dbde39 ①）：按档位把 N 轮的登录率取 min，阈值不变。
$worstByTier = [ordered]@{}
foreach ($t in $tierReports) {
    $key = [string][int]$t.count
    if (-not $worstByTier.Contains($key)) {
        $worstByTier[$key] = @{
            count = [int]$t.count
            threshold = (Get-LoginThreshold ([int]$t.count))
            worst_login_pct = [double]$t.login_success_pct
            runs = @()
        }
    }
    $e = $worstByTier[$key]
    $lp = [double]$t.login_success_pct
    if ($lp -lt $e.worst_login_pct) { $e.worst_login_pct = $lp }
    $e.runs += @{
        run = $t.run
        login_success_pct = $lp
        verdict = $t.verdict
        fails = @($t.fails)
        conn_lost_steady = $t.conn_lost_steady
        internal_errors = $t.internal_errors
        internal_errors_steady = $t.internal_errors_steady
        login_conn_no_resp = $(if ($t.login_conn_diag) { $t.login_conn_diag.no_resp } else { $null })
        oom_hits = $(if ($t.required_fields) { $t.required_fields.oom_hits } else { $null })
        botsrv_crash = $(if ($t.botsrv) { $t.botsrv.crash } else { $null })
        window_conclusion_valid = $(if ($t.window) { $t.window.conclusion_valid } else { $null })
        samples = $t.samples
        tick_p99_ms = $t.tick_p99_ms
        in_world = $t.world_ok
        world_ok_pct = $t.world_ok_pct
        host_cpu_peak_pct = $(if ($t.host_load) { $t.host_load.cpu_peak_pct } else { $null })
        host_heavy_max = $(if ($t.host_load) { $t.host_load.heavy_procs_max } else { $null })
    }
}
foreach ($key in @($worstByTier.Keys)) {
    $e = $worstByTier[$key]
    $ok = ($e.worst_login_pct -ge $e.threshold)
    foreach ($r in $e.runs) { if ($r.verdict -ne "GREEN") { $ok = $false } }
    $e.verdict = if ($ok) { "GREEN" } else { "RED" }
}

# F9（C6 原卡 reviewer-4）：产物 {file,sha256} 清单——供复核者核对引用件未被后续冒烟覆写。
$artifactFiles = @()
foreach ($pat in @("report.json", "load_stats.ndjson", "mem.ndjson", "hostload.ndjson", "login_conns.ndjson",
                   "bots.err.log", "bots_runtime.json")) {
    $artifactFiles += @(Get-ChildItem -Path $OutDir -Recurse -Filter $pat -ErrorAction SilentlyContinue)
}
$artifactManifest = @()
foreach ($f in ($artifactFiles | Sort-Object FullName)) {
    $rel = $f.FullName.Substring($OutDir.Length).TrimStart('\', '/')
    $artifactManifest += @{ file = $rel; sha256 = (Get-FileHash -Algorithm SHA256 -Path $f.FullName).Hash.ToLower() }
}
@{ generated_ms = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()); files = $artifactManifest } |
    ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $OutDir "artifacts.sha256.json") -Encoding utf8

$summaryPath = Join-Path $OutDir "load_gate_summary.json"
@{
  out_dir    = $OutDir
  tiers      = $TierList
  repeat     = $Repeat
  stagger_ms = $StaggerMs
  hold_sec   = $HoldSec
  caliber    = "tick 服务时延 = BotSrv 收到 `#+GD/<rtime>! 的时刻 − rtime（Environment.TickCount，全机同域）；详见 tools/botload/load_report.py"
  criteria   = $Criteria
  criteria_complete = [bool]($Criteria -match '(^|,)\s*internal\s*(,|$)')
  criteria_note = "压测结论必须含 internal（A1，2026-11-15）；缺 internal 的轮次 GREEN 无效"
  max_internal_errors = "0（默认；唯一放宽通道 = tools/botload/internal-errors-allowlist.json，随卡面评审）"
  internal_errors_allowlist = "tools/botload/internal-errors-allowlist.json（当前 entries=[] ⇒ 容忍度 0；报告 internal_error_allowlist 带 path+sha256）"
  crash_any  = [bool]$crashAny
  required_fields_note = "每份结论必须显式列 login / no_resp / internal_errors / OOM；None=证据缺，不得当作 0（见各档 report.json.required_fields）"
  window_note = "结论级窗口 HoldSec>=360s；进世界串行载入实测 ~5–7/s；report.json.window.conclusion_valid=false 的轮次只可冒烟"
  allow_missing_tick = [bool]$AllowMissingTick
  bot_guard = @{
    protection_level       = $guardProtection.level
    protection_missing     = @($guardProtection.missing)
    refuse_start           = [bool]$guardProtection.refuse_start
    threshold_ceiling_gb   = [double]$guardProtection.threshold_ceiling_gb
    requested_ceiling_gb   = [double]$guardProtection.requested_ceiling_gb
    machine_commit_limit_gb= [double]$guardProtection.machine_commit_limit_gb
    ceiling_clamped        = [bool]$guardProtection.ceiling_clamped
    bot_heap_limit_gb      = $BotHeapLimitGB
    mem_kill_threshold_gb  = $MemKillThresholdGB
    max_run_sec            = $MaxRunSec
    max_mem_samples        = $MaxMemSamples
    mem_proc_names         = $MemProcNames
    breach_tripped         = [bool]$guardBreachAny
    bound_tripped          = [bool]$guardBoundAny
    aborted_early          = [bool](-not [string]::IsNullOrEmpty($guardAbortReason))
    abort_reason           = $guardAbortReason
    tiers_planned          = ($TierList.Count * $Repeat)
    tiers_run              = $guardTiersRun
    abort_note             = $(if ($guardAbortReason -eq 'mem_breach') { "因护栏越界提前终止，剩余档位未跑（abort_reason=mem_breach）" } elseif ($guardAbortReason) { "因护栏墙钟上界提前终止，剩余档位未跑（abort_reason=" + $guardAbortReason + "）" } else { "" })
  }
  action_probe = (-not $NoActionProbe)
  action_probe_ms = $ActionProbeMs
  login_resend_ms = $LoginResendMs
  login_threshold_pct = $LoginThresholdPct
  worst_of_runs_rule = "每档取 N 轮里最低的登录成功率与阈值比；且每一轮的 conn/tick/internal 判据都必须 GREEN"
  host_load_caliber = "CPU=Win32_Processor.LoadPercentage 均值(%)；heavy=rustc/cargo/cargo-clippy/msbuild 进程数（叧 tickbench）；只标注不判红"
  host_load_note = "tick P99 在共享机器上不可归属（C# 参考栈 vs 同机并发编译）；CPU 峰≥40% 或 heavy 峰≥4 ⇒ 该轮 P99 未归属，需空载窗口复测"
  artifacts  = $artifactManifest
  worst_by_tier = $worstByTier
  reports    = $tierReports
} | ConvertTo-Json -Depth 8 -WarningAction SilentlyContinue | Set-Content -Path $summaryPath -Encoding utf8
Write-Output ("SUMMARY " + $summaryPath)

$allGreen = $tierReports.Count -gt 0
foreach ($key in @($worstByTier.Keys)) { if ($worstByTier[$key].verdict -ne "GREEN") { $allGreen = $false } }
foreach ($e in @($worstByTier.Values)) {
    Write-Output ("  WORST tier=" + $e.count + " runs=" + @($e.runs).Count + " worst_login=" + $e.worst_login_pct + "% threshold=" + $e.threshold + "% verdict=" + $e.verdict)
}

if ($SelfTestRed) {
    if (-not $allGreen) {
        Write-Output "SELFTEST_RED GREEN: 连不通 => 判红成立（红检本身按预期变红 = 自测通过）"
        exit 0
    }
    Write-Output "SELFTEST_RED RED: 连不通却判绿 —— 判据失效！"
    exit 1
}

if ($tierReports.Count -eq 0) { Write-Output "RED: 一档都没跑成"; exit 1 }
if ($crashAny) {
    Write-Output ("RED: BotSrv 崩溃证据命中（非 0 退出 / bots.err.log 'Out of memory.' / Application Error 0xe0434352）—— " +
                  "见各档 bots_runtime.json 与 bots.err.log；该档判据必红")
    exit 1
}
if ($guardBreachAny) {
    Write-Output ("RED: BotGuard 内存看门狗触发（有进程越界，已杀 BotSrv，提前终止；tiers_run=" + $guardTiersRun + "/" +
                  ($TierList.Count * $Repeat) + "，剩余档位未跑）—— 见各档 mem.ndjson 与上面的 RED BOTGUARD_MEM_BREACH 行")
    exit 1
}
if ($guardBoundAny) {
    Write-Output ("RED: BotGuard 有界跑上界触发（MaxRunSec=" + $MaxRunSec + " / MaxMemSamples=" + $MaxMemSamples +
                  "；tiers_run=" + $guardTiersRun + "/" + ($TierList.Count * $Repeat) + "）—— 本轮未跑完，判不 GREEN")
    exit 1
}
if ($allGreen) {
    $windowAllValid = $true
    foreach ($t in $tierReports) {
        if (-not $t.window -or -not $t.window.conclusion_valid) { $windowAllValid = $false }
    }
    if ($windowAllValid) {
        Write-Output ("GREEN: " + $tierReports.Count + " 轮次全过（登录 ≥ 档位阈值 / 稳态无掉线 / tick P99 ≤ 100ms / internal_errors 未超白名单预算 / 无 BotSrv 崩溃；HoldSec=" + $HoldSec + " ≥ 360s）")
    } else {
        # A4：测量判据全过但窗口不足 ⇒ 只能冒烟；不打印 GREEN:（避免被 grep 成结论）。
        Write-Output ("GREEN_SMOKE_ONLY: " + $tierReports.Count + " 轮次测量判据全过，但 HoldSec=" + $HoldSec +
                      "s < 360s（window.conclusion_valid=false）—— 只可冒烟，不得作为压测 GREEN 结论")
    }
    exit 0
}
Write-Output "RED: 有档位未达标（见各 tier 的 report.json 的 fails；结论四项必列 login/no_resp/internal_errors/OOM）"
exit 1
