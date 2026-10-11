# portclassify.ps1 — M2 验收①「端口监听者的实现归属」的**唯一**分类口径。
#
# 为什么单拉一个文件（reviewer-2 的 R2-1）：`selftest.ps1` 第①条原先在脚本里**内联**算期望值，
# 而那段内联判据比 `check_ports.ps1` 的真实分类器**窄**——它只认 `*\mir2-rs\target\*` /
# `*\m2svr-target\*`，没有 `check_ports.ps1` 的「C# 根目录」与「可执行文件基名 ∈ RustNames」两条回落。
# 于是一旦本机某个端口被 `game-svr.exe`（路径不含 target）听着，`check_ports.ps1` 判 rust，
# 内联判据判非 rust ⇒ 期望退出码算成 1、实际 0 ⇒ 自检 FAIL（假红）。
#
# 现在两端都 dot-source 本文件、共用 `Get-M2PortImpl`，口径不会再漂：
#   - C# 根（`E:\MirServer`）优先判 csharp（现网 C# 栈占端口 = 验收①失败）；
#   - 路径/命令行含 Rust 构建目录（`\mir2-rs\target\` 或隔离的 `\openmir2-*-target\`）判 rust；
#   - 否则回落到**可执行文件基名** ∈ RustNames（`cargo run` / 安装后的产物名）判 rust；
#   - 都不匹配 = unknown（验收①不成立）。

$script:M2CsharpRoot = "E:\MirServer"
$script:M2RustTargetPatterns = @("*\mir2-rs\target\*", "*\m2svr-target\*", "*\openmir2-*-target\*")
$script:M2RustNames = @("login-gate", "sel-gate", "login-srv", "db-srv", "game-gate", "game-svr")

function Get-M2PortImpl {
  param(
    [string]$Exe = "",
    [string]$CommandLine = "",
    [string]$Name = ""
  )
  $probe = ($Exe + " " + $CommandLine)
  # C# 根只认**可执行体路径**（Exe 取不到时才回落命令行首 token）。
  # 不要把整条命令行当探针：Rust 入口正常会拿现网 `E:\MirServer\...\config.conf` 作参数，
  # 整条命令行里就含 `E:\MirServer\` ⇒ 真机实测会把 login-gate/sel-gate/game-gate 误判 csharp（验收①假红）。
  $exeProbe = $Exe
  if ([string]::IsNullOrEmpty($exeProbe) -and -not [string]::IsNullOrEmpty($CommandLine)) {
    $exeProbe = ($CommandLine.Trim() -split '\s+')[0].Trim('"')
  }
  if ($exeProbe -like "$script:M2CsharpRoot\*") {
    return "csharp"
  }
  foreach ($p in $script:M2RustTargetPatterns) {
    if ($Exe -like $p -or $probe -like $p) { return "rust" }
  }
  $base = [System.IO.Path]::GetFileNameWithoutExtension($Name)
  if ($script:M2RustNames -contains $base) { return "rust" }
  return "unknown"
}

# ----------------------------------------------------------------------------
# 验收① 的**第二层牙齿**（「证明是 Rust 入口在监听」）：端口 -> 期望入口。
#
# 第一层（`Get-M2PortImpl`）只证明「某个 Rust 进程」占着端口；它挡不住**接错线**——
# 例如把 `game-svr` 起在 7200、把 `game-gate` 起在 5000，四端口仍全是 Rust、旧判据照样 GREEN，
# 但验收①的语义（设计文档 §2 进程/端口表）已经不成立。这里按端口表判入口归属：
#   ok      = 入口基名 == 该端口期望入口；
#   cross   = 入口是**已知 Rust 入口**但挂在别的端口上（接错线，要判红）；
#   unknown = 认不出的入口名（可能是改名/包装脚本）——只登记不判红，交给验收人。
# 端口表来源：设计文档 §2「进程/端口表」+ 本目录 README §1（5000 GameSvr / 7000 LoginGate /
# 7100 SelGate / 7200 GameGate）。
$script:M2PortEntries = @{ 5000 = "game-svr"; 7000 = "login-gate"; 7100 = "sel-gate"; 7200 = "game-gate" }

function Get-M2ExpectedEntry {
  param([int]$Port)
  if ($script:M2PortEntries.ContainsKey($Port)) { return $script:M2PortEntries[$Port] }
  return ""
}

function Get-M2EntryName {
  param(
    [string]$Exe = "",
    [string]$CommandLine = "",
    [string]$Name = ""
  )
  $raw = ""
  if (-not [string]::IsNullOrEmpty($Name)) { $raw = $Name }
  elseif (-not [string]::IsNullOrEmpty($Exe)) { $raw = $Exe }
  if ([string]::IsNullOrEmpty($raw)) { return "" }
  return [System.IO.Path]::GetFileNameWithoutExtension($raw).ToLowerInvariant()
}

function Get-M2EntryVerdict {
  param([int]$Port, [string]$EntryName = "")
  if ([string]::IsNullOrEmpty($EntryName)) { return "unknown" }
  $expected = Get-M2ExpectedEntry -Port $Port
  if ([string]::IsNullOrEmpty($expected)) { return "unknown" }
  if ($EntryName -eq $expected) { return "ok" }
  if ($script:M2RustNames -contains $EntryName) { return "cross" }
  return "unknown"
}
