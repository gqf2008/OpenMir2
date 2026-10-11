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
  if ($Exe -like "$script:M2CsharpRoot\*" -or $probe -like "*$script:M2CsharpRoot\*") {
    return "csharp"
  }
  foreach ($p in $script:M2RustTargetPatterns) {
    if ($Exe -like $p -or $probe -like $p) { return "rust" }
  }
  $base = [System.IO.Path]::GetFileNameWithoutExtension($Name)
  if ($script:M2RustNames -contains $base) { return "rust" }
  return "unknown"
}
