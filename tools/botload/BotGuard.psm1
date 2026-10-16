#requires -Version 5
<#
  BotGuard.psm1 —— 假人驱动（BotSrv）的内存护栏共享 helper。
  ---------------------------------------------------------------------------
  背景（2026-10-11 死机取证）：`Resource-Exhaustion-Detector` (Event 2004) 四次点名
  `BotSrv.exe`（PID 36564/38108）**提交内存 106–109 GB**，而本机提交上限仅 63.1 GB
  （物理 31.1 + 页面文件 32，系统管理）⇒ 长跑假人把整机顶死。`src/BotSrv/BotSrv.csproj`
  开了 `<ServerGarbageCollection>true</ServerGarbageCollection>`（12 核 ⇒ 每核堆 + 保留），
  且全仓**没有**任何进程级堆上限，`tools/botload/load_gate.ps1` 的采样里也**没有 BotSrv**。
  本模块把四样东西集中到一处，供 `load_gate.ps1` / `run_bots.ps1` 复用：

    ① 进程级 GC 堆硬上限：`DOTNET_GCHeapHardLimit`（hex 字节）。
       实测口径（.NET 8，Server GC + 12 核，E:\tmp\heapsem）：`0x10000000`(256MiB) ⇒
       托管堆 ~248MB 处 OOM / 进程私有峰值 272MB；`0x20000000`(512MiB) ⇒ 496MB / 536MB。
       ⇒ 该 env 是**进程级总堆上限**（不是每核），正好把"顶死整机"变成"单进程干净 OOM 退出"。
       见 `Format-BotGuardEnvAudit` 的审计行（起进程前打印，可复核）。
    ② 内存看门狗：`Find-MemBreach` 在任何被采样进程越过阈值时给出越界描述；
       `Stop-BotSrvForBreach` 据此杀掉 BotSrv；`Format-BotGuardBreachRed` 打 RED。
       **绝不静默**：越界即 RED + 杀 BotSrv + 正常收尾（mem.ndjson 与报告照写）。
    ③ 有界跑：`Test-BotRunBound` 给"最长运行时长/最大迭代"一个显式上界，到点即收尾。
    ④ 保护体检（防"零护栏静默长跑"，独立复验者 2026-10-16 must_fix #1/#2）：`Get-BotGuardProtection`
       把「堆上限 / 看门狗」两项开关归一成 FULL / PARTIAL / NONE，`Format-BotGuardProtectionAudit`
       出可机判审计行。**只有 FULL 才允许无声通过**：PARTIAL 审计行必带 `RED` +
       `verdict=PROTECTION_MISSING`，NONE（两项都失效）⇒消费脚本**拒绝启动**（rc≠0）。

       有效性口径（B3 复验者 must_fix #1/#2 收口，2026-10-16）：一项护栏"有效" = 开关打开 **且**
       **阈值 < 有效上界**——否则在采样判越界之前整机先被顶死，等价于没开。有效上界由**机器推导**：
       `本机提交上限 × SafetyFactor(0.5)`（`Get-MachineCommitLimitGb` 现场读 Win32_OperatingSystem）。
       操作者给的 `-MemKillThresholdCeilingGB` **只能把上界调得更严、不能放大**（放大值被夹回机器推导值，
       审计行标 `ceiling_clamped=yes`）。堆上限与看门狗走**同一套**有效性判定（对称）——`-BotHeapLimitGB 1000`
       在 63GiB 提交上限的机器上大到永远不会触发 ⇒ 视为**关闭**，与看门狗同时失效即 NONE/拒跑。
       铁律：默认配置 = FULL；任何"零保护"组合都不能悄悄跑起来（穷举组合表见 check_botguard.ps1 牙⑥）。

  本模块的函数**要么是纯函数**（无副作用，便于离线自检），**要么只改 env / 杀进程并返回值**；
  它们**不往 PowerShell 输出流写日志**（避免 `$x = Fn` 把日志和返回值糊在一起），
  审计/日志行一律由调用方 `Write-Output` 打印。
#>

# ---- ① 进程级堆上限 --------------------------------------------------------
function ConvertTo-GCHeapHardLimitHex {
    # GiB -> .NET 的 `DOTNET_GCHeapHardLimit` 十六进制字节串；<=0 表示不设限（返回 $null）
    param([Parameter(Mandatory)][int]$HeapLimitGb)
    if ($HeapLimitGb -le 0) { return $null }
    return ('0x{0:X}' -f ([int64]$HeapLimitGb * 1GB))
}

function New-BotGuardEnvOverrides {
    # 返回要注入 BotSrv 的 env 覆盖表（有序，便于打印）；HeapLimitGb<=0 ⇒ 空表
    param([int]$HeapLimitGb = 0)
    $o = [ordered]@{}
    $hex = ConvertTo-GCHeapHardLimitHex -HeapLimitGb $HeapLimitGb
    if ($hex) { $o['DOTNET_GCHeapHardLimit'] = $hex }
    return $o
}

function Set-BotGuardEnv {
    # 把覆盖表落到当前进程 env（子进程 Start-Process 时继承）。无返回值。
    param($Overrides)
    if ($null -eq $Overrides) { return }
    foreach ($k in @($Overrides.Keys)) {
        Set-Item -Path ('env:' + $k) -Value ([string]$Overrides[$k])
    }
}

function Format-BotGuardEnvAudit {
    # 起进程**之前**打印的审计行（最终生效的 heap 上限 + env），返回字符串
    param([string]$Exe, $Overrides, [int]$HeapLimitGb)
    $parts = @()
    if ($Overrides) { foreach ($k in @($Overrides.Keys)) { $parts += ('{0}={1}' -f $k, $Overrides[$k]) } }
    $desc = if ($HeapLimitGb -gt 0) { ('{0}GiB ({1} bytes)' -f $HeapLimitGb, ([int64]$HeapLimitGb * 1GB)) } else { 'DISABLED' }
    return ('BOTGUARD_ENV heap_limit={0} heap_limit_gb={1} env=[{2}] exe={3}' -f $desc, $HeapLimitGb, ($parts -join ' '), $Exe)
}

# ---- ② 内存看门狗 ----------------------------------------------------------
function Get-ProcMemSample {
    # 采一个进程的私有字节/工作集；进程不存在返回 $null
    param([string]$Name, [int]$Pid)
    try { $p = Get-Process -Id $Pid -ErrorAction Stop } catch { return $null }
    return [pscustomobject]@{
        proc       = $Name
        pid        = [int]$Pid
        priv_bytes = [int64]$p.PrivateMemorySize64
        ws_bytes   = [int64]$p.WorkingSet64
    }
}

function Test-MemBreach {
    # 单点判定：私有字节或工作集 >= 阈值即越界；阈值 <=0 表示看门狗关闭
    param([long]$PrivBytes, [long]$WsBytes, [long]$ThresholdBytes)
    if ($ThresholdBytes -le 0) { return $false }
    return ([math]::Max([int64]$PrivBytes, [int64]$WsBytes) -ge [int64]$ThresholdBytes)
}

function Find-MemBreach {
    # 在一批采样里找**第一个**越界者，返回描述对象（proc/pid/metric/value/threshold）；无越界返回 $null
    param([object[]]$Samples, [long]$ThresholdBytes)
    if ($ThresholdBytes -le 0) { return $null }
    foreach ($s in @($Samples)) {
        if ($null -eq $s) { continue }
        if (Test-MemBreach -PrivBytes ([int64]$s.priv_bytes) -WsBytes ([int64]$s.ws_bytes) -ThresholdBytes $ThresholdBytes) {
            $metric = if ([int64]$s.priv_bytes -ge [int64]$s.ws_bytes) { 'priv' } else { 'ws' }
            $val = if ($metric -eq 'priv') { [int64]$s.priv_bytes } else { [int64]$s.ws_bytes }
            return [pscustomobject]@{
                proc            = $s.proc
                pid             = [int]$s.pid
                metric          = $metric
                value_bytes     = $val
                threshold_bytes = [int64]$ThresholdBytes
            }
        }
    }
    return $null
}

function Format-BotGuardBreachRed {
    # 越界 RED 行：写明 哪个进程越界 / 实测值 / 阈值。返回字符串。
    param([Parameter(Mandatory)]$Breach)
    $mb = [math]::Round([double]$Breach.value_bytes / 1MB, 1)
    $thr = [math]::Round([double]$Breach.threshold_bytes / 1MB, 1)
    return ('RED BOTGUARD_MEM_BREACH proc={0} pid={1} metric={2} measured_mb={3} threshold_mb={4} action=kill_BotSrv' -f
            $Breach.proc, $Breach.pid, $Breach.metric, $mb, $thr)
}

function Stop-BotSrvForBreach {
    # 越界时杀掉指定的 BotSrv 进程（按 pid），返回被杀的 pid 数组（供调用方打印审计）
    param([Parameter(Mandatory)]$Breach, [int[]]$BotPids)
    if ($null -eq $Breach) { return @() }
    $killed = New-Object System.Collections.Generic.List[int]
    foreach ($id in @($BotPids)) {
        if ($null -eq $id -or [int]$id -le 0) { continue }
        $p = Get-Process -Id ([int]$id) -ErrorAction SilentlyContinue
        if ($p) {
            Stop-Process -Id ([int]$id) -Force -ErrorAction SilentlyContinue
            $killed.Add([int]$id)
        }
    }
    return $killed.ToArray()
}

# ---- ③ 有界跑 --------------------------------------------------------------
function Test-BotRunBound {
    # 到点上界则返回原因（'time' / 'iterations'），未到返回 $null。Max*<=0 表示该维不限。
    param([double]$ElapsedSec, [int]$MaxRunSec, [int]$Iteration, [int]$MaxIterations)
    if ($MaxRunSec -gt 0 -and $ElapsedSec -ge $MaxRunSec) { return 'time' }
    if ($MaxIterations -gt 0 -and $Iteration -ge $MaxIterations) { return 'iterations' }
    return $null
}

# ---- ④ 保护体检（防"零护栏静默长跑"）--------------------------------------
# 背景（独立复验者 2026-10-16 must_fix #1/#2）：原护栏在 `-BotHeapLimitGB 0` 且 `-MemKillThresholdGB 0`
# （或阈值取到形同不设限的极大值）时**完全静默**——假进程涨到 296MB 无一行 guard 输出、不杀任何东西，
# 只剩一行 `heap_limit=DISABLED` 审计 ⇒ 10/11 那套"顶死整机"的配置可以静默复发。
#
# B3 复验者补刀（2026-10-16，本批核心）：第一版把"有效性"交给**操作者自定义的天花板**
# `-MemKillThresholdCeilingGB` 判定 ⇒ 操作者把天花板一起调大（`-MemKillThresholdGB 100000
# -MemKillThresholdCeilingGB 100001`）即可把空看门狗重新判成"有效"，叠加空堆上限（`-BotHeapLimitGB 1000`）
# 后输出 `level=FULL / verdict=protected`，零保护长跑**静默**进行（复验者在未改动脚本上复现）。
# 修法（"这个上限真的会在整机被打死之前触发吗？"）：
#   · 天花板**由机器推导、不可由操作者放大**：`机推上限 = 本机提交上限 × SafetyFactor`。操作者给的
#     `-MemKillThresholdCeilingGB` 只允许**调更严**，放大值一律夹回机推上限（审计标 `ceiling_clamped=yes`）。
#   · `有效 = 开关打开 且 阈值 < 有效上界`。**堆上限与看门狗对称**：`-BotHeapLimitGB 1000` 在本机
#     （提交上限 ~63GiB ⇒ 机推上限 ~31.5GiB）大到永远不会触发 ⇒ 视为**关闭**（must_fix #2）。
#   · 两项都失效 ⇒ NONE ⇒ 消费脚本**拒绝启动**（rc=2）；只一项失效 ⇒ PARTIAL ⇒ 审计行显式标红；两项有效 ⇒ FULL。
# `threshold_ceiling_gb` = "阈值到此即与关闭等价"的**有效上界**（夹取后的值，非操作者原值）。
function Get-MachineCommitLimitGb {
    # 本机提交上限（GiB）= 物理内存 + 页面文件。优先 `Win32_OperatingSystem.TotalVirtualMemorySize`（KB）。
    # 取不到依次回退：WMI 同类 → `Win32_ComputerSystem` 物理内存 × 2（无页面文件信息，保守放大）→ 兜底 64GiB。
    # **这是"天花板"的唯一来源**：操作者不能把它调大（见 Get-BotGuardProtection）。
    # -OverrideGb 供离线自检注入固定值（不读 WMI）；否则读本机。返回值恒 > 0。
    param([string]$OverrideGb = '')
    if ($OverrideGb -ne '') { return [double]$OverrideGb }
    try {
        $os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
        if ($os -and $os.TotalVirtualMemorySize -gt 0) { return ([double]$os.TotalVirtualMemorySize / 1MB) }  # KB -> GiB
    } catch { }
    try {
        $os2 = Get-WmiObject -Class Win32_OperatingSystem -ErrorAction Stop
        if ($os2 -and $os2.TotalVirtualMemorySize -gt 0) { return ([double]$os2.TotalVirtualMemorySize / 1MB) }
    } catch { }
    try {
        $cs = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction Stop
        if ($cs -and $cs.TotalPhysicalMemory -gt 0) { return ([double]$cs.TotalPhysicalMemory * 2 / 1GB) }
    } catch { }
    return 64.0
}

function Get-BotGuardProtection {
    # 把两项护栏开关归一成可机判的体检结果（纯函数，无副作用；-MachineCommitLimitGb <=0 时现场探测本机提交上限）。
    # 返回 [pscustomobject]：
    #   level                 : FULL | PARTIAL | NONE
    #   missing               : 失效项名列表（'heap_limit' / 'watchdog'）
    #   refuse_start          : 零护栏（两项都失效）⇒ $true（调用方必须拒绝启动，rc=2）
    #   threshold_ceiling_gb  : 有效上界（夹取后；非操作者原值）
    # 有效性：`开关打开 且 阈值 < 有效上界`。有效上界 = min(机推上限, 操作者上界)；操作者上界**不能放大**它。
    param(
        [int]$HeapLimitGb = 0,
        [double]$MemKillThresholdGb = 0,
        [double]$ThresholdCeilingGb = 0,   # 操作者可选"更严"上界（GiB）；<=0 = 不额外设限（用机推上限）
        [double]$MachineCommitLimitGb = 0, # 本机提交上限（GiB）；<=0 ⇒ 现场探测（Get-MachineCommitLimitGb）
        [double]$SafetyFactor = 0.5        # 有效阈值必须 < 提交上限 × 该系数
    )
    $commitGb = $MachineCommitLimitGb
    $commitSource = 'given'
    if ($commitGb -le 0) { $commitGb = Get-MachineCommitLimitGb; $commitSource = 'detected' }
    if ($SafetyFactor -le 0) { $SafetyFactor = 0.5 }
    $machineCeilingGb = $commitGb * $SafetyFactor

    # 有效上界：默认 = 机推上限；操作者只能给得更严；试图放大的一律夹回（**静默不许**，记 clamped）
    $effCeilingGb = $machineCeilingGb
    $clamped = $false
    if ($ThresholdCeilingGb -gt 0) {
        if ($ThresholdCeilingGb -lt $machineCeilingGb) { $effCeilingGb = $ThresholdCeilingGb }
        elseif ($ThresholdCeilingGb -gt $machineCeilingGb) { $clamped = $true }
    }

    $heapOn = [bool]($HeapLimitGb -gt 0)
    $watchdogOn = [bool]($MemKillThresholdGb -gt 0)
    # 对称有效性：堆上限与看门狗都按"阈值 < 有效上界"判；大到永远不触发 == 关闭
    $heapEffective = [bool]($heapOn -and ([double]$HeapLimitGb -lt $effCeilingGb))
    $watchdogEffective = [bool]($watchdogOn -and ($MemKillThresholdGb -lt $effCeilingGb))

    $missing = New-Object System.Collections.Generic.List[string]
    if (-not $heapEffective) { $missing.Add('heap_limit') }
    if (-not $watchdogEffective) { $missing.Add('watchdog') }
    $level = 'FULL'
    if ($missing.Count -eq 1) { $level = 'PARTIAL' }
    elseif ($missing.Count -ge 2) { $level = 'NONE' }
    return [pscustomobject]@{
        level                   = $level
        heap_limit_gb           = [int]$HeapLimitGb
        mem_kill_threshold_gb   = [double]$MemKillThresholdGb
        threshold_ceiling_gb    = [double]$effCeilingGb
        requested_ceiling_gb    = [double]$ThresholdCeilingGb
        machine_commit_limit_gb = [double]$commitGb
        commit_limit_source     = $commitSource
        safety_factor           = [double]$SafetyFactor
        ceiling_clamped         = [bool]$clamped
        heap_limit_on           = $heapOn
        heap_limit_effective    = $heapEffective
        watchdog_on             = $watchdogOn
        watchdog_effective      = $watchdogEffective
        missing                 = @($missing)
        refuse_start            = [bool]($level -eq 'NONE')
    }
}

function Format-BotGuardProtectionAudit {
    # 保护体检审计行。**只有 FULL 不带 RED**；PARTIAL/NONE 一律 `RED BOTGUARD_PROTECTION …` +
    # 可机判 verdict（`PROTECTION_MISSING` / `ZERO_PROTECTION_REFUSED`）。返回字符串。
    # 尾部追加机器推导口径（machine_commit_limit_gb / ceiling_clamped / *_effective / commit_source），可机判。
    param([Parameter(Mandatory)]$Protection)
    $verdict = 'UNKNOWN'
    if ($Protection.level -eq 'FULL') { $verdict = 'protected' }
    elseif ($Protection.level -eq 'PARTIAL') { $verdict = 'PROTECTION_MISSING' }
    elseif ($Protection.level -eq 'NONE') { $verdict = 'ZERO_PROTECTION_REFUSED' }
    $prefix = 'BOTGUARD_PROTECTION'
    if ($Protection.level -ne 'FULL') { $prefix = 'RED BOTGUARD_PROTECTION' }
    $miss = 'none'
    if (@($Protection.missing).Count -gt 0) { $miss = (@($Protection.missing) -join '+') }
    $clamp = 'no'; if ($Protection.ceiling_clamped) { $clamp = 'yes' }
    return ('{0} level={1} verdict={2} heap_limit_gb={3} mem_kill_threshold_gb={4} threshold_ceiling_gb={5} missing={6} machine_commit_limit_gb={7} ceiling_clamped={8} heap_effective={9} watchdog_effective={10} commit_source={11}' -f
            $prefix, $Protection.level, $verdict, $Protection.heap_limit_gb,
            $Protection.mem_kill_threshold_gb, $Protection.threshold_ceiling_gb, $miss,
            $Protection.machine_commit_limit_gb, $clamp, $Protection.heap_limit_effective,
            $Protection.watchdog_effective, $Protection.commit_limit_source)
}


Export-ModuleMember -Function ConvertTo-GCHeapHardLimitHex, New-BotGuardEnvOverrides, Set-BotGuardEnv, `
    Format-BotGuardEnvAudit, Get-ProcMemSample, Test-MemBreach, Find-MemBreach, `
    Format-BotGuardBreachRed, Stop-BotSrvForBreach, Test-BotRunBound, `
    Get-MachineCommitLimitGb, Get-BotGuardProtection, Format-BotGuardProtectionAudit
