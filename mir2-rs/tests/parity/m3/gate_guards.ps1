# gate_guards.ps1 — M3 正式门禁（run_m3_gate.ps1）三条机判护栏的**唯一**口径。
#
# 为什么单拉一个文件（照 tools/m2-acceptance/portclassify.ps1 的先例）：
#   `run_m3_gate.ps1` 的主流程与 `selftest_gate.ps1` 的构造用例**共用同一批函数**；
#   「改坏必红 / 还原必绿」才是作用在真正跑的生产代码上，而不是一份复制的判据。
#
# 三条护栏（两轮独立审查 findings：reviewer-4 `55116105` finding 1/2、reviewer-3 `4bffcf99` M3R-2）：
#   ① Get-M3PeerScenarioGuard：`-PeerSession` 的 baseline.json.fixed_new_chr 必须与本侧场景一致，
#      否则两侧不是同一输入序列，跨侧 diff 无仲裁力 ⇒ RED（旧实现只靠人核）。
#   ② Get-M3SessionDirFault：本侧会话目录可能为 $null（run_pair 中途失败、目录已建但未写
#      baseline.json）。旧实现 `Join-Path $sessionDir.FullName …` 在 $ErrorActionPreference='Stop'
#      下抛异常中断、**连 RED 报告都不落**（fail-open）⇒ 进第 3 步前先判空。
#   ③ Get-M3EntryGuard：`-Side rust` 的入口护栏不能只判「不是 C#」——必须是「**已知 Rust 入口**
#      且挂在该端口期望入口上」（复用 Get-M2PortImpl / Get-M2EntryName / Get-M2EntryVerdict）。
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

# ③ 参照会话建角场景一致性（机判）
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

  $baseline = Get-Content -Path $baselinePath -Raw | ConvertFrom-Json
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
