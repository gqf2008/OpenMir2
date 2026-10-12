# gate_guards.ps1 — M3 正式门禁（run_m3_gate.ps1）三条机判护栏的**唯一**口径。
#
# 为什么单拉一个文件（照 tools/m2-acceptance/portclassify.ps1 的先例）：
#   `run_m3_gate.ps1` 的主流程与 `selftest_gate.ps1` 的构造用例**共用同一批函数**；
#   「改坏必红 / 还原必绿」才是作用在真正跑的生产代码上，而不是一份复制的判据。
#
# 三条护栏（两轮独立审查 findings：reviewer-4 `55116105` finding 1/2、reviewer-3 `4bffcf99` M3R-2）：
#   ① Get-M3PeerScenarioGuard：`-PeerSession` 的 baseline.json.fixed_new_chr 必须与本侧场景一致，
#      否则两侧不是同一输入序列，跨侧 diff 无仲裁力 ⇒ RED（旧实现只靠人核）。
#      P-1（reviewer-5 `3926cf99` / reviewer-3 `56f02a6e`）：peer 的 baseline.json 被写坏/半写时，
#      旧写法 `Get-Content … | ConvertFrom-Json` 抛异常；`run_m3_gate.ps1` 设 `$ErrorActionPreference='Stop'`
#      且调用点无 try/catch ⇒ 门禁**在落 RED 报告前中断**（同一 fail-open 家族）⇒ 解析失败必须是
#      `peer_baseline_invalid` 结构化失败，让主流程继续到第 4 步落报告、退出码非 0。
#   ② Get-M3SessionDirFault：本侧会话目录可能为 $null（run_pair 中途失败、目录已建但未写
#      baseline.json）。旧实现 `Join-Path $sessionDir.FullName …` 在 $ErrorActionPreference='Stop'
#      下抛异常中断、**连 RED 报告都不落**（fail-open）⇒ 进第 3 步前先判空。
#   ③ Get-M3EntryGuard：`-Side rust` 的入口护栏不能只判「不是 C#」——必须是「**已知 Rust 入口**
#      且挂在该端口期望入口上」（复用 Get-M2PortImpl / Get-M2EntryName / Get-M2EntryVerdict）。
#   ④ Get-M3PeerBagEquipScopeGuard（T-14 复跑口径，owner 2026-10-12 06:36）：`-PeerSession` 的
#      baseline.json.bag_equip_sequence 必须与本侧**同一组开关**（step_ms / start_delay_ms /
#      disable_autoplay）。背包/装备序列**必须**同时关掉客户端挂机（`DisableAutoPlay`）——挂机是
#      无种子 RNG 随机走位，两侧各走各的 ⇒ `characters.MapName/CX/CY` 逐字节比时必然出 diff
#      （且不是实现差异）。旧产物（如 `csharp-20261012-054849`）没有 `disable_autoplay=` 记录 ⇒
#      不能当同口径 peer，必须按新开关重跑 C# 基线。
#
# 依赖 portclassify.ps1；本文件自行 dot-source，调用方只需 `. gate_guards.ps1`。
# 注意：本文件被 dot-source，故用 $PSScriptRoot 定位自身（$MyInvocation 在 dot-source 下不可靠）。

$script:M3GuardsHere = $PSScriptRoot
$script:M3GuardsRepoRoot = (Resolve-Path (Join-Path $script:M3GuardsHere '..\..\..\..')).Path
. (Join-Path $script:M3GuardsRepoRoot 'tools\m2-acceptance\portclassify.ps1')

# 场景标签：与 run_pair.ps1 / run_m3_gate.ps1 写进 baseline.json 的 fixed_new_chr 同形态。
function Format-M3Scenario {
  param(
    [bool]$FixedNewChr = $false,
    [int]$NewChrJob = 0,
    [int]$NewChrHair = 2,
    [int]$NewChrSex = 0
  )
  if ($FixedNewChr) { return ("Job={0} Hair={1} Sex={2}" -f $NewChrJob, $NewChrHair, $NewChrSex) }
  return '(off)'
}

# 端口监听者记录：impl/entry/verdict 一律走 portclassify.ps1 的同一口径
# （run_m3_gate.ps1 的实况探测与 selftest_gate.ps1 的构造用例共用，避免口径漂移）。
function New-M3EntryHolder {
  param(
    [Parameter(Mandatory)][int]$Port,
    [string]$ProcessName = '',
    [string]$Path = '',
    [string]$CommandLine = ''
  )
  $impl = Get-M2PortImpl -Exe $Path -CommandLine $CommandLine -Name $ProcessName
  $entry = Get-M2EntryName -Exe $Path -CommandLine $CommandLine -Name $ProcessName
  $verdict = Get-M2EntryVerdict -Port $Port -EntryName $entry
  return [pscustomobject]@{
    port = $Port; process = $ProcessName; path = $Path
    impl = $impl; entry = $entry; verdict = $verdict
  }
}

function New-M3EntryHolderNone {
  param([Parameter(Mandatory)][int]$Port)
  return [pscustomobject]@{
    port = $Port; process = '(无监听)'; path = ''
    impl = 'none'; entry = ''; verdict = 'unknown'
  }
}

# ① 入口护栏（-Side rust 必须是已知 Rust 入口且挂在本端口期望入口上）
function Get-M3EntryGuard {
  param(
    [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Holders,
    [Parameter(Mandatory)][ValidateSet('csharp', 'rust')][string]$Side
  )
  $failed = New-Object System.Collections.Generic.List[string]
  $messages = New-Object System.Collections.Generic.List[string]
  $impls = @($Holders | ForEach-Object { $_.impl } | Sort-Object -Unique)

  if ($impls -contains 'none') {
    $messages.Add('RED: 入口端口不全在听——服务端没起全，门禁不成立')
    $failed.Add('entry_ports_not_listening')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); impls = $impls }
  }

  if ($Side -eq 'rust') {
    # 旧护栏只判 impl != 'csharp'（'rust-or-other' 即放行）⇒「未知进程」/「已知 Rust 入口接错线」都能过。
    # 现要求 impl=rust **且** verdict=ok（= portclassify 端口表里的期望入口）。
    $bad = @($Holders | Where-Object { $_.impl -ne 'rust' -or $_.verdict -ne 'ok' })
    if ($bad.Count -gt 0) {
      foreach ($h in $bad) {
        $messages.Add(("-Side rust 要求入口是**已知 Rust 入口**且挂在本端口期望入口上；" +
            "端口 {0} <- process={1} impl={2} entry={3} verdict={4} path={5}" -f `
            $h.port, $h.process, $h.impl, $h.entry, $h.verdict, $h.path))
      }
      $messages.Insert(0, 'RED: -Side rust 的入口护栏未通过（不是「已知 Rust 入口」或接错端口）')
      $failed.Add('rust_side_entry_not_known_rust')
    }
  }

  $allCsharp = ($impls.Count -eq 1 -and $impls[0] -eq 'csharp')
  if ($Side -eq 'csharp' -and -not $allCsharp) {
    $messages.Add('RED: -Side csharp 但入口不是 E:\MirServer 的 C# 实现')
    $failed.Add('csharp_side_but_entry_is_not_csharp')
  }
  if (($impls -contains 'csharp') -and (@($impls | Where-Object { $_ -ne 'csharp' }).Count -gt 0)) {
    $messages.Add('RED: 入口端口被两种实现混着占（部分端口 C#、部分非 C#）——先统一')
    $failed.Add('mixed_implementations_on_entries')
  }

  return [pscustomobject]@{ failed = @($failed); messages = @($messages); impls = $impls }
}

# ② 本侧会话目录空值防线（null ⇒ 记 session_dir_missing，让主流程继续到落 RED 报告）
function Get-M3SessionDirFault {
  param([object]$SessionDir = $null)
  if ($null -eq $SessionDir) { return 'session_dir_missing' }
  return $null
}

# ③ 参照会话建角场景一致性（机判；含 peer baseline.json 解析失败防线，见 P-1）
function Get-M3PeerScenarioGuard {
  param(
    [Parameter(Mandatory)][string]$PeerDir,
    [Parameter(Mandatory)][AllowEmptyString()][string]$LocalScenario
  )
  $failed = New-Object System.Collections.Generic.List[string]
  $messages = New-Object System.Collections.Generic.List[string]
  $peerScenario = $null
  $baselinePath = Join-Path $PeerDir 'baseline.json'

  if (-not (Test-Path $baselinePath)) {
    $messages.Add(("RED: 参照会话缺 baseline.json: {0}" -f $baselinePath))
    $failed.Add('peer_baseline_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scenario = $peerScenario }
  }

  # P-1：peer baseline.json 被写坏/半写（截断、非法 JSON、空文件）时，旧写法在 Stop 下抛异常、
  # 门禁不落 RED 报告（fail-open）。这里局部 -ErrorAction Stop + try/catch，解析失败 ⇒ 结构化 RED
  # （peer_baseline_invalid），主流程继续到第 4 步。空/非对象（$null 结果）同样按解析失败处理。
  $baseline = $null
  $parseError = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch {
    $parseError = $_.Exception.Message
  }
  if ($null -eq $baseline) {
    if ([string]::IsNullOrWhiteSpace($parseError)) { $parseError = '内容为空或不是 JSON 对象（半写/截断）' }
    $messages.Add(("RED: 参照会话 baseline.json 解析失败: {0} —— {1}" -f $baselinePath, $parseError))
    $failed.Add('peer_baseline_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scenario = $peerScenario }
  }

  $peerScenario = [string]$baseline.fixed_new_chr
  if ([string]::IsNullOrEmpty($peerScenario)) {
    $messages.Add(("RED: 参照会话 baseline.json 缺 fixed_new_chr（旧产物/被裁剪）: {0}" -f $baselinePath))
    $failed.Add('peer_fixed_new_chr_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scenario = $peerScenario }
  }

  if ($peerScenario -ne $LocalScenario) {
    $messages.Add(("-PeerSession 建角场景与本侧不一致——peer.fixed_new_chr='{0}' vs local='{1}'；" +
        "两侧不是同一输入序列，跨侧 diff 无仲裁力（-FixedNewChr 必须两侧一致）" -f $peerScenario, $LocalScenario))
    $messages.Insert(0, 'RED: 参照会话场景与本侧不一致')
    $failed.Add('peer_scenario_mismatch')
  }

  return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scenario = $peerScenario }
}

# ④ 背包/装备序列口径（T-14 复跑，「两侧同一组开关」的唯一形态）。
#
# 形态：`(off)` 或 `on step_ms=<int> start_delay_ms=<int> disable_autoplay=<True|False>`。
# run_pair.ps1 / run_m3_gate.ps1 的 baseline/report 与 selftest 的构造输入共用本文件，避免口径漂移。
function Format-M3BagEquipScope {
  param(
    [bool]$BagEquipSequence = $false,
    [int]$BagEquipStepMs = 1000,
    [int]$BagEquipStartDelayMs = 3000,
    [bool]$DisableAutoPlay = $false
  )
  if (-not $BagEquipSequence) { return '(off)' }
  $ap = if ($DisableAutoPlay) { 'True' } else { 'False' }
  return ("on step_ms={0} start_delay_ms={1} disable_autoplay={2}" -f $BagEquipStepMs, $BagEquipStartDelayMs, $ap)
}

# 把 baseline.json 里**已存的** bag_equip_sequence 串规范化到与 Format-M3BagEquipScope 同形态。
# 旧产物没有 `disable_autoplay=` 位——缺省按 `False` 还原（旧产物就是「开着挂机」跑的），
# 这样旧 peer 与「关挂机」本侧的串**必然不等**、会被判红（这正是要的：不能当同口径 peer）。
# 返回 $null = 形态认不出（非空但不是本函数认识的标签）。
function ConvertTo-M3BagEquipScope {
  param([AllowEmptyString()][string]$Raw)
  if ([string]::IsNullOrWhiteSpace($Raw) -or $Raw.Trim() -eq '(off)') { return '(off)' }
  if ($Raw -notmatch '^on\b') { return $null }
  $step = if ($Raw -match 'step_ms=(\d+)') { [int]$Matches[1] } else { 1000 }
  $sd = if ($Raw -match 'start_delay_ms=(\d+)') { [int]$Matches[1] } else { 3000 }
  $ap = if ($Raw -match 'disable_autoplay=(True|False)') { $Matches[1] } else { 'False' }
  return ("on step_ms={0} start_delay_ms={1} disable_autoplay={2}" -f $step, $sd, $ap)
}

# ④ 参照会话背包/装备开关一致性（机判；含 peer baseline.json 解析失败防线，同 P-1）。
# LocalScope 由调用方用 Format-M3BagEquipScope 从**本侧实际开关**算出（canonical）。
function Get-M3PeerBagEquipScopeGuard {
  param(
    [Parameter(Mandatory)][string]$PeerDir,
    [Parameter(Mandatory)][AllowEmptyString()][string]$LocalScope
  )
  $failed = New-Object System.Collections.Generic.List[string]
  $messages = New-Object System.Collections.Generic.List[string]
  $peerScope = $null
  $baselinePath = Join-Path $PeerDir 'baseline.json'

  if (-not (Test-Path $baselinePath)) {
    $messages.Add(("RED: 参照会话缺 baseline.json: {0}" -f $baselinePath))
    $failed.Add('peer_baseline_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
  }

  $baseline = $null
  $parseError = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch {
    $parseError = $_.Exception.Message
  }
  if ($null -eq $baseline) {
    if ([string]::IsNullOrWhiteSpace($parseError)) { $parseError = '内容为空或不是 JSON 对象（半写/截断）' }
    $messages.Add(("RED: 参照会话 baseline.json 解析失败: {0} —— {1}" -f $baselinePath, $parseError))
    $failed.Add('peer_baseline_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
  }

  $localCanon = ConvertTo-M3BagEquipScope -Raw $LocalScope
  $peerRaw = [string]$baseline.bag_equip_sequence
  $peerCanon = ConvertTo-M3BagEquipScope -Raw $peerRaw
  $peerScope = $peerCanon

  if ($null -eq $peerCanon) {
    $messages.Add(("RED: 参照会话 bag_equip_sequence 形态认不出: '{0}'（{1}）" -f $peerRaw, $baselinePath))
    $failed.Add('peer_bag_equip_scope_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
  }
  if ([string]::IsNullOrEmpty($peerRaw)) {
    # 旧产物缺该字段：默认语义是 off。本侧 off ⇒ 等价；本侧 on ⇒ 无法证明同口径 ⇒ 判红。
    if ($localCanon -eq '(off)') { $peerCanon = '(off)'; $peerScope = '(off)' }
    else {
      $peerScope = '(missing)'
      $messages.Add(("RED: 参照会话 baseline.json 缺 bag_equip_sequence（旧产物/被裁剪）: {0}" -f $baselinePath))
      $failed.Add('peer_bag_equip_scope_missing')
      return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
    }
  }

  if ($peerCanon -ne $localCanon) {
    $msg = ("-PeerSession 背包/装备开关与本侧不一致——peer='{0}' vs local='{1}'；" +
        "两侧不是同一组夹具开关，跨侧 diff 无仲裁力（step_ms/start_delay_ms/disable_autoplay 必须一致）") -f $peerCanon, $localCanon
    $messages.Add($msg)
    # 具体归因：本侧关了挂机、peer 却是旧产物（无 disable_autoplay= ⇒ 开着挂机跑的）⇒
    # 其 characters.CX/CY 是随机走位值，必然与本侧出 diff。
    if (($localCanon -match 'disable_autoplay=True') -and ($peerCanon -match 'disable_autoplay=False') -and ($peerRaw -notmatch 'disable_autoplay=')) {
      $messages.Add('RED: 参照会话是「开着挂机」跑的旧产物（无 disable_autoplay 记录）——挂机随机走位会让 characters.MapName/CX/CY 必然出 diff；C# 基线必须按新开关（-BagEquipSequence 会一并传 -DisableAutoPlay）重跑一次')
      $failed.Add('peer_bag_equip_autoplay_on')
    } else {
      $failed.Add('peer_bag_equip_scope_mismatch')
    }
    $messages.Insert(0, 'RED: 参照会话背包/装备开关与本侧不一致')
  }

  return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
}

# 从一个会话目录的 baseline.json 读出背包/装备开关标签（canonical）。
# 缺 baseline.json / 解析失败 ⇒ $null（调用方当 PREREQ）；字段缺失 ⇒ '(off)'（旧产物默认关）。
function Get-M3SessionBagEquipScope {
  param([Parameter(Mandatory)][string]$Dir)
  $baselinePath = Join-Path $Dir 'baseline.json'
  if (-not (Test-Path $baselinePath)) { return $null }
  $baseline = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch { return $null }
  if ($null -eq $baseline) { return $null }
  return (ConvertTo-M3BagEquipScope -Raw ([string]$baseline.bag_equip_sequence))
}

# ⑤ 固定起点（T-14 固定起点变体，owner 2026-10-12 06:5x 的 `86478c55`/`d7798370`）。
#
# 背景：只关挂机还不够——`HomeMap` 空时 oracle 走出生点分支（`WorldServer.cs:249` +
# `GetRandHomeX/Y = Random(3)+Home-2`，`RandomNumber.cs:20` 无种子）⇒ 两侧起点天然不同。
# 固定起点变体 = 两相夹具：① 预热建角 ② SQL pin 角色的**初始态**——`MapName/HomeMap/HomeX/HomeY/
# CX/CY/Level/Dir` + `characters_bagitem`（4 件：蜡烛/布衣/木剑/金创药(小量)）+ `characters_item`（清空）
# + `characters_ablity`（Level=1 等）③ pin 之后再取 BEFORE 快照 ④ 计分跑走「选角」路径。
# `HomeMap` 非空 ⇒ oracle 跳过新角色分支（`WorldServer.cs:249`/`:258-276`），也就**不会**发新手 4 件、
# 不会走无种子出生点 ⇒ 必须由 fixture 把初始背包/Abil 一并 provision（owner 643459a7），否则五步无对象可操作。
# 不必放宽判据（14/14 自检保持绿）。
#
# 形态：`none` 或 `map=<m> home=<hm>,<hx>,<hy> pos=<cx>,<cy> level=<l> dir=<d> bag=<std@pos,..> abil=<lvl>`。
# bag/abil 段是 fixture 常量，写进标签 ⇒ 两侧一旦 provision 不一致，护栏就判红。
$script:M3PinBag = '48@0,97@1,207@2,3@3'
$script:M3PinAbilLevel = 1
function Format-M3StartPose {
  param(
    [string]$MapName = '0',
    [int]$HomeMap = 0, [int]$HomeX = 289, [int]$HomeY = 618,
    [int]$CX = 289, [int]$CY = 618, [int]$Level = 1, [int]$Dir = 0
  )
  return ("map={0} home={1},{2},{3} pos={4},{5} level={6} dir={7} bag={8} abil={9}" -f `
    $MapName, $HomeMap, $HomeX, $HomeY, $CX, $CY, $Level, $Dir, $script:M3PinBag, $script:M3PinAbilLevel)
}

# baseline.json 里已存的 start_pose 串 → canonical。空/`none` ⇒ 'none'；形态认不出 ⇒ $null。
function ConvertTo-M3StartPose {
  param([AllowEmptyString()][string]$Raw)
  if ([string]::IsNullOrWhiteSpace($Raw) -or $Raw.Trim() -eq 'none') { return 'none' }
  if ($Raw -notmatch '^map=(.+) home=(-?\d+),(-?\d+),(-?\d+) pos=(-?\d+),(-?\d+) level=(-?\d+) dir=(-?\d+) bag=([\d@,]+) abil=(-?\d+)$') {
    return $null
  }
  return ("map={0} home={1},{2},{3} pos={4},{5} level={6} dir={7} bag={8} abil={9}" -f `
    $Matches[1], $Matches[2], $Matches[3], $Matches[4], $Matches[5], $Matches[6], $Matches[7], $Matches[8], $Matches[9], $Matches[10])
}

# 参照会话固定起点一致性（机判；含 peer baseline.json 解析失败防线，同 P-1）。
function Get-M3PeerStartPoseGuard {
  param(
    [Parameter(Mandatory)][string]$PeerDir,
    [Parameter(Mandatory)][AllowEmptyString()][string]$LocalPose
  )
  $failed = New-Object System.Collections.Generic.List[string]
  $messages = New-Object System.Collections.Generic.List[string]
  $peerPose = $null
  $baselinePath = Join-Path $PeerDir 'baseline.json'
  if (-not (Test-Path $baselinePath)) {
    $messages.Add(("RED: 参照会话缺 baseline.json: {0}" -f $baselinePath))
    $failed.Add('peer_baseline_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  $baseline = $null
  $parseError = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch { $parseError = $_.Exception.Message }
  if ($null -eq $baseline) {
    if ([string]::IsNullOrWhiteSpace($parseError)) { $parseError = '内容为空或不是 JSON 对象（半写/截断）' }
    $messages.Add(("RED: 参照会话 baseline.json 解析失败: {0} —— {1}" -f $baselinePath, $parseError))
    $failed.Add('peer_baseline_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  $localCanon = ConvertTo-M3StartPose -Raw $LocalPose
  $peerRaw = [string]$baseline.start_pose
  $peerPose = ConvertTo-M3StartPose -Raw $peerRaw
  if ($null -eq $peerPose) {
    $messages.Add(("RED: 参照会话 start_pose 形态认不出: '{0}'（{1}）" -f $peerRaw, $baselinePath))
    $failed.Add('peer_start_pose_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  if ([string]::IsNullOrEmpty($peerRaw) -and $localCanon -ne 'none') {
    $peerPose = '(missing)'
    $messages.Add(("RED: 参照会话 baseline.json 缺 start_pose（旧产物）: {0}" -f $baselinePath))
    $failed.Add('peer_start_pose_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  if ($peerPose -ne $localCanon) {
    $msg = ("-PeerSession 固定起点与本侧不一致——peer='{0}' vs local='{1}'；" +
        "两侧不是同一初始状态，跨侧位置列 diff 无仲裁力（pin 值必须逐字节相同）") -f $peerPose, $localCanon
    $messages.Add($msg)
    $messages.Insert(0, 'RED: 参照会话固定起点与本侧不一致')
    $failed.Add('peer_start_pose_mismatch')
  }
  return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
}

# 从会话目录读 start_pose（canonical）；缺 baseline/解析失败 ⇒ $null；字段缺失 ⇒ 'none'。
function Get-M3SessionStartPose {
  param([Parameter(Mandatory)][string]$Dir)
  $baselinePath = Join-Path $Dir 'baseline.json'
  if (-not (Test-Path $baselinePath)) { return $null }
  $baseline = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch { return $null }
  if ($null -eq $baseline) { return $null }
  return (ConvertTo-M3StartPose -Raw ([string]$baseline.start_pose))
}

# 固定起点变体的 pin SQL（唯一口径；run_pair 与自检共用，避免 SQL 漂移）。
# 返回**多条**语句（按序执行）：
#   1. 清空该角色背包 46 槽；2-5. 写入 4 件固定初始物品（StdIndex 48/97/207/3；MakeIndex 固定）；
#   6. 清空穿戳槽 13 位；7. 写 Abil（Level=1 等固定值）。
# 为什么连背包/Abil 一起 pin（owner 643459a7）：`HomeMap` 非空 ⇒ oracle 跳过新角色分支 ⇒ 不发新手 4 件、
# 不走无种子出生点；若不 provision，五步（拾取→装备→卸下→丢弃→用药）无对象可操作。
function Get-M3StartPosePinSql {
  param([Parameter(Mandatory)][string]$ChrName)
  $safe = $ChrName.Replace("'", "''")
  $items = @(
    @{ Pos = 0; Make = 90000001; Std = 48;  Dura = 8000; DuraMax = 8000 }
    @{ Pos = 1; Make = 90000002; Std = 97;  Dura = 5000; DuraMax = 5000 }
    @{ Pos = 2; Make = 90000003; Std = 207; Dura = 4000; DuraMax = 4000 }
    @{ Pos = 3; Make = 90000004; Std = 3;   Dura = 0;    DuraMax = 1 }
  )
  $sql = New-Object System.Collections.Generic.List[string]
  $sql.Add("UPDATE mir2_db.characters_bagitem b JOIN mir2_db.characters c ON b.PlayerId=c.Id SET b.MakeIndex=0, b.StdIndex=0, b.Dura=0, b.DuraMax=0 WHERE c.ChrName='$safe'")
  foreach ($it in $items) {
    $sql.Add(("UPDATE mir2_db.characters_bagitem b JOIN mir2_db.characters c ON b.PlayerId=c.Id SET b.MakeIndex={0}, b.StdIndex={1}, b.Dura={2}, b.DuraMax={3} WHERE c.ChrName='{4}' AND b.Position={5}" -f `
      $it.Make, $it.Std, $it.Dura, $it.DuraMax, $safe, $it.Pos))
  }
  $sql.Add("UPDATE mir2_db.characters_item i JOIN mir2_db.characters c ON i.PlayerId=c.Id SET i.MakeIndex=0, i.StdIndex=0, i.Dura=0, i.DuraMax=0 WHERE c.ChrName='$safe'")
  $sql.Add("UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id SET a.Level=1, a.Ac=1, a.Hp=19, a.Mp=15, a.MaxHP=19, a.MAxMP=15, a.Exp=0, a.MaxExp=10, a.Weight=0, a.MaxWeight=50, a.WearWeight=0, a.MaxWearWeight=15, a.HandWeight=0, a.MaxHandWeight=12 WHERE c.ChrName='$safe'")
  return $sql.ToArray()
}

# 固定起点变体的 pin SQL（单条：characters 行的图/坐标；供 -EmitOpsArgs 展示）。
function Format-M3StartPoseSql {
  param(
    [Parameter(Mandatory)][string]$ChrName,
    [string]$MapName = '0',
    [int]$HomeMap = 0, [int]$HomeX = 289, [int]$HomeY = 618,
    [int]$CX = 289, [int]$CY = 618, [int]$Level = 1, [int]$Dir = 0
  )
  $safe = $ChrName.Replace("'", "''")
  return ("UPDATE mir2_db.characters SET MapName='{0}', CX={1}, CY={2}, HomeMap={3}, HomeX={4}, HomeY={5}, Level={6}, Dir={7} WHERE ChrName='{8}'" -f `
    $MapName, $CX, $CY, $HomeMap, $HomeX, $HomeY, $Level, $Dir, $safe)
}

# ===========================================================
# 攻击对拍夹具（M3②③；卡 openmir2-m3-attack-pair-fixture）
# ===========================================================
# 目标：两个固定起点 bot，attacker 在 (X,Y) 面向 Dir，victim 在相邻格 (X+1,Y)；
# attacker 固定发 N 次 CM_HIT（方向固定 = 指向 victim 格），victim 被打；伤害从**双方都能读到**的
# 下行解读（victim 自己收 SM_STRUCK；attacker 作为 AOI 观察者也收）。两侧同种子 + 同起点 + 同序列
# ⇒ 逐次伤害序列（按「第 k 次」）与 hits/dodges 应相等。
#
# 与单起点 pin 分开的原因：`run_pair` 的既有 pin 把所有 bot 钉在同一点（供背包/装备序列），
# 攻击需要 attacker/victim **相邻但不重叠**。victim 的 HP/MaxHP 抬高（防 100+ 次内死亡后无 struck）。
$script:M3AttackVictimHp = 100000
function Format-M3AttackPose {
  param(
    [string]$MapName = '0',
    [int]$AX = 289, [int]$AY = 618, [int]$Dir = 2,
    [int]$VX = 290, [int]$VY = 618,
    [int]$Level = 1, [int]$VictimHp = 100000
  )
  return ("pair map={0} attacker={1},{2},dir={3} victim={4},{5} level={6} vhp={7}" -f `
    $MapName, $AX, $AY, $Dir, $VX, $VY, $Level, $VictimHp)
}

function ConvertTo-M3AttackPose {
  param([AllowEmptyString()][string]$Raw)
  if ([string]::IsNullOrWhiteSpace($Raw) -or $Raw.Trim() -eq 'none') { return 'none' }
  if ($Raw -notmatch '^pair map=(\S+) attacker=(-?\d+),(-?\d+),dir=(-?\d+) victim=(-?\d+),(-?\d+) level=(-?\d+) vhp=(-?\d+)$') {
    return $null
  }
  return ("pair map={0} attacker={1},{2},dir={3} victim={4},{5} level={6} vhp={7}" -f `
    $Matches[1], $Matches[2], $Matches[3], $Matches[4], $Matches[5], $Matches[6], $Matches[7], $Matches[8])
}

function Get-M3PeerAttackPoseGuard {
  param(
    [Parameter(Mandatory)][string]$PeerDir,
    [Parameter(Mandatory)][AllowEmptyString()][string]$LocalPose
  )
  $failed = New-Object System.Collections.Generic.List[string]
  $messages = New-Object System.Collections.Generic.List[string]
  $peerPose = $null
  $baselinePath = Join-Path $PeerDir 'baseline.json'
  if (-not (Test-Path $baselinePath)) {
    $messages.Add(("RED: 参照会话缺 baseline.json: {0}" -f $baselinePath))
    $failed.Add('peer_baseline_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  $baseline = $null
  $parseError = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch { $parseError = $_.Exception.Message }
  if ($null -eq $baseline) {
    if ([string]::IsNullOrWhiteSpace($parseError)) { $parseError = '内容为空或不是 JSON 对象（半写/截断）' }
    $messages.Add(("RED: 参照会话 baseline.json 解析失败: {0} —— {1}" -f $baselinePath, $parseError))
    $failed.Add('peer_baseline_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  $localCanon = ConvertTo-M3AttackPose -Raw $LocalPose
  $peerRaw = [string]$baseline.attack_pose
  $peerPose = ConvertTo-M3AttackPose -Raw $peerRaw
  if ($null -eq $peerPose) {
    $messages.Add(("RED: 参照会话 attack_pose 形态认不出: '{0}'（{1}）" -f $peerRaw, $baselinePath))
    $failed.Add('peer_attack_pose_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  if ([string]::IsNullOrEmpty($peerRaw) -and $localCanon -ne 'none') {
    $peerPose = '(missing)'
    $messages.Add(("RED: 参照会话 baseline.json 缺 attack_pose（旧产物）: {0}" -f $baselinePath))
    $failed.Add('peer_attack_pose_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
  }
  if ($peerPose -ne $localCanon) {
    $messages.Add(("-PeerSession 攻击起点与本侧不一致——peer='{0}' vs local='{1}'；两侧不是同一初始状态，攻击序列 diff 无仲裁力（pair pin 值必须逐字节相同）") -f $peerPose, $localCanon)
    $messages.Insert(0, 'RED: 参照会话攻击起点与本侧不一致')
    $failed.Add('peer_attack_pose_mismatch')
  }
  return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_pose = $peerPose }
}

function Get-M3SessionAttackPose {
  param([Parameter(Mandatory)][string]$Dir)
  $baselinePath = Join-Path $Dir 'baseline.json'
  if (-not (Test-Path $baselinePath)) { return $null }
  $baseline = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch { return $null }
  if ($null -eq $baseline) { return $null }
  return (ConvertTo-M3AttackPose -Raw ([string]$baseline.attack_pose))
}

# 攻击变体的 pin SQL（按 character 返回多条）：图/坐标/朝向 + Abil（Hp=MaxHP 抬高）。
function Get-M3AttackPinSql {
  param(
    [Parameter(Mandatory)][string]$ChrName,
    [Parameter(Mandatory)][int]$CX,
    [Parameter(Mandatory)][int]$CY,
    [int]$Dir = 0,
    [string]$MapName = '0',
    [int]$HomeMap = 0, [int]$HomeX = 289, [int]$HomeY = 618,
    [int]$Level = 1,
    [int]$Hp = 100000
  )
  $safe = $ChrName.Replace("'", "''")
  $sql = New-Object System.Collections.Generic.List[string]
  $sql.Add(("UPDATE mir2_db.characters SET MapName='{0}', CX={1}, CY={2}, HomeMap={3}, HomeX={4}, HomeY={5}, Level={6}, Dir={7} WHERE ChrName='{8}'" -f `
    $MapName, $CX, $CY, $HomeMap, $HomeX, $HomeY, $Level, $Dir, $safe))
  $sql.Add(("UPDATE mir2_db.characters_ablity a JOIN mir2_db.characters c ON a.PlayerId=c.Id SET a.Level={0}, a.Hp={1}, a.MaxHP={1}, a.Mp=15, a.MAxMP=15, c.IncHealth=0, c.IncSpell=0 WHERE c.ChrName='{2}'" -f `
    $Level, $Hp, $safe))
  return $sql.ToArray()
}

# ---------------------------------------------------------------------------
# pin SQL 的「别名/列存在性」无栈校验（m3cs tick16 真机实跑 finding，2026-10-12 11:42）。
#
# 背景：`Get-M3AttackPinSql` 的生产 SQL 曾把 `IncHealth/IncSpell` 挂在 `characters_ablity`
# 别名 `a` 上（`a.IncHealth=0`），而这两列在**冻结 schema** `sql/mir2_db.sql` 里属于
# `characters`（别名 `c`）⇒ 真机 pin 抛 `ERROR 1054 (42S22) Unknown column 'a.IncHealth' in 'SET'`，
# 攻击对拍在 `run_pair.ps1:203` 整体 RED、一条 `CM_HIT` 都发不出去（m3cs 第十六格 C# 半，原始件
# `E:/tmp/m3cs16-evidence/csharp-stack/run-csharp.txt`）。
#
# 本校验**不连真库**（自检必须 -SelfCheckNoStack）：从冻结件 `sql/mir2_db.sql` 解析
# 「表 → 列集合」，再逐条核对 pin SQL 的 `alias.column` 赋值；越界即结构化 violations。
# 锚 schema 而非硬编码列名：表结构是冻结内容（三条硬约束之二），且 schema 一旦变该校验自动跟着走，
# 不会与 `sql/mir2_db.sql` 漂移。
function Get-M3SchemaColumns {
  param([Parameter(Mandatory)][string]$SchemaPath)
  if (-not (Test-Path $SchemaPath)) { throw ("找不到冻结 schema: {0}" -f $SchemaPath) }
  $map = @{}
  $current = $null
  foreach ($line in (Get-Content -Path $SchemaPath)) {
    if ($line -match '^\s*CREATE TABLE\s+`([^`]+)`') {
      $current = $Matches[1]
      $map[$current] = New-Object 'System.Collections.Generic.HashSet[string]'
      continue
    }
    if ($null -ne $current) {
      if ($line -match '^\s*\)\s*ENGINE') { $current = $null; continue }
      if ($line -match '^\s*`([^`]+)`\s') { [void]$map[$current].Add($Matches[1]) }
    }
  }
  return $map
}

# 校验一组 pin SQL 语句里的 `alias.column =` 赋值：别名必须在 `UPDATE`/`JOIN` 子句里出现过，
# 且该列必须属于别名指向的表。返回违规清单（空 = 合规）。
function Test-M3PinSqlColumns {
  param(
    [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$SqlStatements,
    [Parameter(Mandatory)]$SchemaColumns
  )
  $violations = New-Object System.Collections.Generic.List[string]
  foreach ($sql in @($SqlStatements)) {
    # 别名表：只从 `UPDATE/JOIN db.table alias` 形态取（`ON a.x=c.y` 里的比较不算）。
    $aliasToTable = @{}
    foreach ($m in [regex]::Matches($sql, '(?i)\b(?:UPDATE|JOIN)\s+`?[\w]+`?\s*\.\s*`?([\w]+)`?\s+(?:AS\s+)?`?([A-Za-z][A-Za-z0-9_]*)`?(?=\s)')) {
      $aliasToTable[$m.Groups[2].Value] = $m.Groups[1].Value
    }
    if ($aliasToTable.Count -eq 0) { continue }
    # 只在 SET 子句里找赋值：避免把 JOIN 的 `a.PlayerId=c.Id` 误当赋值（那些列也合法，
    # 但限定 SET 让违规定位更准）。
    $setClause = $sql
    if ($sql -match '(?is)\bSET\b(.*?)(?:\bWHERE\b|$)') { $setClause = $Matches[1] }
    foreach ($m in [regex]::Matches($setClause, '(?<![\w.])([A-Za-z][A-Za-z0-9_]*)\.([A-Za-z][A-Za-z0-9_]*)\s*=')) {
      $alias = $m.Groups[1].Value
      $col = $m.Groups[2].Value
      if (-not $aliasToTable.ContainsKey($alias)) { continue }
      $table = $aliasToTable[$alias]
      if (-not $SchemaColumns.ContainsKey($table)) {
        $violations.Add(("表不在 schema: {0}（{1}.{2}）" -f $table, $alias, $col))
        continue
      }
      if (-not $SchemaColumns[$table].Contains($col)) {
        $violations.Add(("列不存在: {0}.{1}（{2} 无此列）" -f $alias, $col, $table))
      }
    }
  }
  return $violations
}

# pin SQL 的「可执行性」极小门（F-B，m3cs tick16 真机 finding 的同类漏网；owner 2026-10-12 11:55）。
#
# 为什么需要（真机实跑反例）：`run_attack_pair.ps1 -SelfCheckNoStack` ⑥ 此前只对 pin SQL 做
# **字符串匹配**、从不真正送进 MySQL ⇒「列名挂错表」这类 **prepare 期**错误
# （`ERROR 1054 (42S22) Unknown column 'a.IncHealth' in 'SET'`）永远穿得过四门禁与自检，
# 真机一跑就在 `run_pair.ps1` 的 pin 步整体 RED、一条 `CM_HIT` 都发不出。
# 本门把每条 `UPDATE` **真的执行一次**：WHERE 指向不存在的角色名 ⇒ 0 行改动（不触碰任何
# 角色/内容），但 MySQL 仍会在 prepare 期校验表名/别名/列名 ⇒ 列挂错表必 `rc!=0`。
# 返回：rc（0=全绿，1=有语句不可执行，2=环境缺失如 mysql 客户端不在位）、failures、outputs。
function Test-M3PinSqlExecutable {
  param(
    [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$SqlStatements,
    [string]$MysqlExe = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe',
    [string]$DbHost = '127.0.0.1',
    [int]$DbPort = 3306,
    [string]$NoSuchChar = '__m3_no_such_char__'
  )
  $failures = New-Object System.Collections.Generic.List[string]
  $outputs = New-Object System.Collections.Generic.List[string]
  if (-not (Test-Path -LiteralPath $MysqlExe)) {
    $failures.Add(("找不到 mysql 客户端: {0}" -f $MysqlExe))
    return [pscustomobject]@{ rc = 2; failures = @($failures); outputs = @($outputs) }
  }
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $i = 0
    foreach ($sql in @($SqlStatements)) {
      $i++
      if ($sql -notmatch "ChrName='") {
        $failures.Add(("stmt#{0} 没有 ChrName='...' 子句，无法把 WHERE 指向不存在角色名（拒绝盲跑）" -f $i))
        continue
      }
      # 把所有 ChrName='...' 常量替换成不存在的名字：prepare 期照旧校验表/列，但 0 行落盘。
      $safeSql = [regex]::Replace($sql, "ChrName='(?:[^']|'')*'", ("ChrName='{0}'" -f $NoSuchChar))
      $out = (& $MysqlExe -h $DbHost -P $DbPort -u root '--password=' --batch --skip-column-names -e $safeSql 2>&1 | Out-String)
      $stmtRc = $LASTEXITCODE
      $outputs.Add(("stmt#{0} rc={1} sql={2}`n{3}" -f $i, $stmtRc, $safeSql, $out.Trim()))
      if ($stmtRc -ne 0) {
        # 优先取 MySQL 的 ERROR 行（`ERROR 1054 (42S22) ... Unknown column 'a.IncHealth' ...`），
        # 便于机判与人工定位；否则退回首条非空行。
        $lines = @($out.Trim() -split "`r?`n" | Where-Object { $_ })
        $firstLine = $lines | Where-Object { $_ -match 'ERROR\s+\d+' } | Select-Object -First 1
        if (-not $firstLine) { $firstLine = $lines | Select-Object -First 1 }
        $failures.Add(("stmt#{0} 不可执行(rc={1}): {2}" -f $i, $stmtRc, $firstLine))
      }
    }
  } finally {
    $ErrorActionPreference = $prevEap
  }
  $rcv = if ($failures.Count -eq 0) { 0 } else { 1 }
  return [pscustomobject]@{ rc = $rcv; failures = @($failures); outputs = @($outputs) }
}

# 种子注入证据断言（F-C；owner 2026-10-12 11:55）。
#
# 为什么需要：两侧同种子是 ②③ 对拍有仲裁力的前提；但 `run_attack_pair.ps1` 此前**不校验**两侧
# 是否真的注入种子 ⇒ 未设种子时只会得到「不可比的红」，却看不出来（C# 侧有 `INJECT_OK 42`，
# Rust 侧无对应断言）。
# 证据形式都是**进程内自证**（非脚本自述）：
#   C#   = rng-hook（`MIR2_RNG_LOG`）写出的 `1\tINJECT_OK\t<seed>\t0` 行（`rng-hook/StartupHook.cs:96`）；
#   Rust = game-svr 启动日志的 `RNG 同种子注入：seed=<seed> 参照序列=SEED_PROBE ...` 行
#          （T-30，`bins/game-svr/src/main.rs`），另可叠加启动器的 `RUST_SEED_INJECTED=<seed>`
#          （`openmir2-m3-rust-stack-seed-plumb`）；两者都出现时必须一致。
# 返回：ok / seed / reason / matched。日志缺失、无证据行、或与 ExpectSeed 不符 ⇒ ok=$false（fail-closed）。
function Test-M3SeedInjected {
  param(
    [Parameter(Mandatory)][ValidateSet('csharp', 'rust')][string]$Side,
    [Parameter(Mandatory)][AllowEmptyString()][string]$LogPath,
    [Parameter(Mandatory)][int]$ExpectSeed
  )
  if ([string]::IsNullOrWhiteSpace($LogPath)) {
    return [pscustomobject]@{ ok = $false; seed = $null; reason = '未提供种子日志路径（-CsharpSeedLog / $env:MIR2_RNG_LOG 或 -RustSeedLog）'; matched = '' }
  }
  if (-not (Test-Path -LiteralPath $LogPath)) {
    return [pscustomobject]@{ ok = $false; seed = $null; reason = ("找不到种子日志: {0}" -f $LogPath); matched = '' }
  }
  $prevEap = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    if ($Side -eq 'csharp') {
      # Select-String -List 只取首条、流式读大文件（rng-stream 可能百万行）。
      $hit = Select-String -LiteralPath $LogPath -Pattern 'INJECT_OK	(-?\d+)	' -List -ErrorAction SilentlyContinue
      if (-not $hit) {
        return [pscustomobject]@{ ok = $false; seed = $null; reason = ("C# 种子日志无 INJECT_OK 行（hooked GameSvr 未注入种子？）: {0}" -f $LogPath); matched = '' }
      }
      $seed = [int]$hit.Matches[0].Groups[1].Value
      if ($seed -ne $ExpectSeed) {
        return [pscustomobject]@{ ok = $false; seed = $seed; reason = ("C# 注入种子 {0} != 期望 {1}" -f $seed, $ExpectSeed); matched = $hit.Line }
      }
      return [pscustomobject]@{ ok = $true; seed = $seed; reason = ''; matched = $hit.Line }
    }
    # rust：game-svr 的 `seed=<n> ... SEED_PROBE` 行（T-30）。
    $hit = Select-String -LiteralPath $LogPath -Pattern 'seed=(-?\d+).*SEED_PROBE|SEED_PROBE.*seed=(-?\d+)' -List -ErrorAction SilentlyContinue
    if (-not $hit) {
      return [pscustomobject]@{ ok = $false; seed = $null; reason = ("Rust 启动日志无 SEED_PROBE 行（game-svr 未注入 MIR2_RNG_SEED？）: {0}" -f $LogPath); matched = '' }
    }
    $g = $hit.Matches[0].Groups
    $seed = if ($g[1].Success) { [int]$g[1].Value } else { [int]$g[2].Value }
    if ($seed -ne $ExpectSeed) {
      return [pscustomobject]@{ ok = $false; seed = $seed; reason = ("Rust 注入种子 {0} != 期望 {1}" -f $seed, $ExpectSeed); matched = $hit.Line }
    }
    # 若启动器打了 RUST_SEED_INJECTED=<n>（openmir2-m3-rust-stack-seed-plumb DoD②），必须一致。
    $launch = Select-String -LiteralPath $LogPath -Pattern 'RUST_SEED_INJECTED=(-?\d+)' -List -ErrorAction SilentlyContinue
    if ($launch) {
      $lseed = [int]$launch.Matches[0].Groups[1].Value
      if ($lseed -ne $ExpectSeed) {
        return [pscustomobject]@{ ok = $false; seed = $seed; reason = ("RUST_SEED_INJECTED={0} != 期望 {1}" -f $lseed, $ExpectSeed); matched = $launch.Line }
      }
    }
    return [pscustomobject]@{ ok = $true; seed = $seed; reason = ''; matched = $hit.Line }
  } finally {
    $ErrorActionPreference = $prevEap
  }
}

# ⑥ pin settle+hold 判据（T-14 两相固定起点；owner 2026-10-12 08:20 要求的「可判红自检」）。
#
# 失败模式（第十四格实跑踩到）：预热相的掉线存档（BotSrv 被杀 ⇒ GameSvr `SaveHumanRcd` ⇒ FrontEngine）
# 是**异步**的，落库时间**晚于** pin。若 pin 后**不做 settle 就立刻复验**，会读到刚写进去的 pin 值
# ⇒ **假 GREEN**；存档随后落库覆盖 pin ⇒ BEFORE/AFTER 快照拿到预热末态（实测 C# `CX/CY=285,622`）。
# ⇒「起点守卫」的 hold 不能只看某一瞬间三项计数到位，必须同时满足：
#   ① 至少一轮**真实 settle**（`SettleRounds >= 1`）；settle 脚本块必须报告自己**确实等了**
#      （返回 `$false` 的 settle 不计入轮数 ⇒ 跳过 settle 必被判 `pin_settle_skipped`）；
#   ② **连续两轮** settle 后的复验都到位（`PreviousMatch && Match`）——单次观测不足以证明
#      「异步存档不会随后覆盖 pin」（这正是「pin 早于落库静默」的防线）。
# 返回：hold / match / failed（诊断码）/ detail。本函数是 `run_pair.ps1` 真实 pin 循环与自检共用的唯一口径。

# 仪器的形态标签（写进 `-EmitOpsArgs` ⇒ 无栈自检可机判生产接线有没有被削弱）。
function Format-M3PinInstrument {
  param([int]$SettleSeconds = 3, [int]$MaxAttempts = 5, [int]$Consecutive = 2)
  return ("settle_seconds={0} max_attempts={1} consecutive={2}" -f $SettleSeconds, $MaxAttempts, $Consecutive)
}

function Get-M3PinHoldVerdict {
  param(
    [int]$SettleRounds = 0,
    [bool]$PreviousMatch = $false,
    [int]$Pinned = 0,
    [int]$BagPinned = 0,
    [int]$AbilPinned = 0,
    [Parameter(Mandatory)][int]$Bots,
    [int]$BagPerBot = 4
  )
  $match = ($Pinned -eq $Bots) -and ($BagPinned -eq ($BagPerBot * $Bots)) -and ($AbilPinned -eq $Bots)
  $failed = New-Object System.Collections.Generic.List[string]
  if ($SettleRounds -lt 1) { $failed.Add('pin_settle_skipped') }
  if (-not $match) {
    if ($Pinned -ne $Bots) { $failed.Add('pin_char_mismatch') }
    if ($BagPinned -ne ($BagPerBot * $Bots)) { $failed.Add('pin_bag_mismatch') }
    if ($AbilPinned -ne $Bots) { $failed.Add('pin_abil_mismatch') }
  } elseif (-not $PreviousMatch) {
    $failed.Add('pin_hold_needs_consecutive_settle')
  }
  $hold = ($match -and $PreviousMatch -and ($SettleRounds -ge 1))
  return [pscustomobject]@{
    hold   = $hold
    match  = $match
    failed = @($failed)
    detail = ("settle_rounds={0} prev_match={1} pinned={2}/{3} bag={4}/{5} abil={6}/{7}" -f `
        $SettleRounds, $PreviousMatch, $Pinned, $Bots, $BagPinned, ($BagPerBot * $Bots), $AbilPinned, $Bots)
  }
}

# settle+hold 循环本体（唯一口径）：`run_pair.ps1` 的真实 pin 路径与 `-SelfCheckNoStack` 的故障注入
# 共用同一函数（改坏本函数即可让自检变红 ⇒ 「仪器有没有被削弱」有牙齿）。
#   $Pin($ctx,$attempt)     执行一次 pin（写 SQL）
#   $Settle($ctx,$attempt)  等一个 settle 窗口；**返回 $true 表示确实等了**（否则该轮不计入 settle 轮数）
#   $Observe($ctx,$attempt) 返回 @{ Pinned=..; BagPinned=..; AbilPinned=.. } 的三项复验
# 返回 @{ Hold; Attempts; SettleRounds; Verdicts }。hold 不住 ⇒ Hold=$false（调用方 throw，fail-closed）。
# ⑦ 攻击序列口径（M3②③ 真机对拍夹具，卡 openmir2-m3-attack-pair-fixture）。
#
# 与 ④ 同形：`(off)` 或 `on count=<int> step_ms=<int> start_delay_ms=<int> disable_autoplay=<True|False>`。
# 攻击序列**必须**一并关挂机（同 T-14 理由：挂机是无种子 RNG 随机走位），两侧同开关才有仲裁力。
function Format-M3AttackScope {
  param(
    [bool]$AttackSequence = $false,
    [int]$AttackCount = 120,
    [int]$AttackStepMs = 500,
    [int]$AttackStartDelayMs = 3000,
    [bool]$DisableAutoPlay = $false
  )
  if (-not $AttackSequence) { return '(off)' }
  $ap = if ($DisableAutoPlay) { 'True' } else { 'False' }
  return ("on count={0} step_ms={1} start_delay_ms={2} disable_autoplay={3}" -f $AttackCount, $AttackStepMs, $AttackStartDelayMs, $ap)
}

# baseline.json 里已存的 attack_sequence 串 → canonical。旧产物缺 `disable_autoplay=` ⇒ False。
function ConvertTo-M3AttackScope {
  param([AllowEmptyString()][string]$Raw)
  if ([string]::IsNullOrWhiteSpace($Raw) -or $Raw.Trim() -eq '(off)') { return '(off)' }
  if ($Raw -notmatch '^on\b') { return $null }
  $count = if ($Raw -match 'count=(\d+)') { [int]$Matches[1] } else { 120 }
  $step = if ($Raw -match 'step_ms=(\d+)') { [int]$Matches[1] } else { 500 }
  $sd = if ($Raw -match 'start_delay_ms=(\d+)') { [int]$Matches[1] } else { 3000 }
  $ap = if ($Raw -match 'disable_autoplay=(True|False)') { $Matches[1] } else { 'False' }
  return ("on count={0} step_ms={1} start_delay_ms={2} disable_autoplay={3}" -f $count, $step, $sd, $ap)
}

# 参照会话攻击序列开关一致性（机判；含 peer baseline.json 解析失败防线，同 P-1）。
function Get-M3PeerAttackScopeGuard {
  param(
    [Parameter(Mandatory)][string]$PeerDir,
    [Parameter(Mandatory)][AllowEmptyString()][string]$LocalScope
  )
  $failed = New-Object System.Collections.Generic.List[string]
  $messages = New-Object System.Collections.Generic.List[string]
  $peerScope = $null
  $baselinePath = Join-Path $PeerDir 'baseline.json'
  if (-not (Test-Path $baselinePath)) {
    $messages.Add(("RED: 参照会话缺 baseline.json: {0}" -f $baselinePath))
    $failed.Add('peer_baseline_missing')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
  }
  $baseline = $null
  $parseError = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch { $parseError = $_.Exception.Message }
  if ($null -eq $baseline) {
    if ([string]::IsNullOrWhiteSpace($parseError)) { $parseError = '内容为空或不是 JSON 对象（半写/截断）' }
    $messages.Add(("RED: 参照会话 baseline.json 解析失败: {0} —— {1}" -f $baselinePath, $parseError))
    $failed.Add('peer_baseline_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
  }
  $localCanon = ConvertTo-M3AttackScope -Raw $LocalScope
  $peerRaw = [string]$baseline.attack_sequence
  $peerCanon = ConvertTo-M3AttackScope -Raw $peerRaw
  $peerScope = $peerCanon
  if ($null -eq $peerCanon) {
    $messages.Add(("RED: 参照会话 attack_sequence 形态认不出: '{0}'（{1}）" -f $peerRaw, $baselinePath))
    $failed.Add('peer_attack_scope_invalid')
    return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
  }
  if ([string]::IsNullOrEmpty($peerRaw)) {
    if ($localCanon -eq '(off)') { $peerCanon = '(off)'; $peerScope = '(off)' }
    else {
      $peerScope = '(missing)'
      $messages.Add(("RED: 参照会话 baseline.json 缺 attack_sequence（旧产物/被裁剪）: {0}" -f $baselinePath))
      $failed.Add('peer_attack_scope_missing')
      return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
    }
  }
  if ($peerCanon -ne $localCanon) {
    $messages.Add(("-PeerSession 攻击序列开关与本侧不一致——peer='{0}' vs local='{1}'；两侧不是同一组夹具开关，跨侧 diff 无仲裁力（count/step_ms/start_delay_ms/disable_autoplay 必须一致）") -f $peerCanon, $localCanon)
    if (($localCanon -match 'disable_autoplay=True') -and ($peerCanon -match 'disable_autoplay=False') -and ($peerRaw -notmatch 'disable_autoplay=')) {
      $messages.Add('RED: 参照会话是「开着挂机」跑的旧产物（无 disable_autoplay 记录）——挂机随机走位会让目标/坐标必然出 diff；C# 基线必须按新开关重跑一次')
      $failed.Add('peer_attack_autoplay_on')
    } else {
      $failed.Add('peer_attack_scope_mismatch')
    }
    $messages.Insert(0, 'RED: 参照会话攻击序列开关与本侧不一致')
  }
  return [pscustomobject]@{ failed = @($failed); messages = @($messages); peer_scope = $peerScope }
}

# 从会话目录 baseline.json 读攻击序列开关（canonical）；缺/坏 ⇒ $null。
function Get-M3SessionAttackScope {
  param([Parameter(Mandatory)][string]$Dir)
  $baselinePath = Join-Path $Dir 'baseline.json'
  if (-not (Test-Path $baselinePath)) { return $null }
  $baseline = $null
  try {
    $baseline = Get-Content -Path $baselinePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
  } catch { return $null }
  if ($null -eq $baseline) { return $null }
  return (ConvertTo-M3AttackScope -Raw ([string]$baseline.attack_sequence))
}

function Invoke-M3PinSettleHold {
  param(
    [Parameter(Mandatory)][scriptblock]$Pin,
    [Parameter(Mandatory)][scriptblock]$Settle,
    [Parameter(Mandatory)][scriptblock]$Observe,
    [Parameter(Mandatory)][int]$Bots,
    [int]$BagPerBot = 4,
    [int]$MaxAttempts = 5,
    [object]$Context = $null
  )
  $prevMatch = $false
  $settleRounds = 0
  # 注：不要用 `New-Object System.Collections.Generic.List[object]` + `@(...)` —— PS 5.1 下对该实例
  # 取 `@()` 会抛「Argument types do not match」（实测）；普通数组追加即可（MaxAttempts ≤ 5）。
  $verdicts = @()
  for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    & $Pin $Context $attempt
    if (& $Settle $Context $attempt) { $settleRounds++ }
    $o = & $Observe $Context $attempt
    $v = Get-M3PinHoldVerdict -SettleRounds $settleRounds -PreviousMatch $prevMatch `
      -Pinned ([int]$o.Pinned) -BagPinned ([int]$o.BagPinned) -AbilPinned ([int]$o.AbilPinned) -Bots $Bots -BagPerBot $BagPerBot
    $v | Add-Member -NotePropertyName attempt -NotePropertyValue $attempt -Force
    $verdicts += $v
    if ($v.hold) {
      return [pscustomobject]@{ Hold = $true; Attempts = $attempt; SettleRounds = $settleRounds; Verdicts = @($verdicts) }
    }
    $prevMatch = $v.match
  }
  return [pscustomobject]@{ Hold = $false; Attempts = $MaxAttempts; SettleRounds = $settleRounds; Verdicts = @($verdicts) }
}
