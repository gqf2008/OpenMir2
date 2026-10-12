#requires -Version 5
<#
  m3hashutil.ps1 — M3 夹具共用的环境无关 SHA256（.NET），且**能读被别的进程持有的活文件**。

  两个坑，都已在真机撞过：

  ① `Get-FileHash` 属 `Microsoft.PowerShell.Utility`。本机 PS 5.1 在 `PSModulePath` 被 PS7 污染时
     不导出该 cmdlet ⇒ `$ErrorActionPreference='Stop'` 下 `CommandNotFoundException` 崩溃
     （m3cs 首跑走 `Test-M3MonGenEmpty -VerifySource` 复现）。修法 = 用 .NET SHA256（不依赖模块解析，
     结果与 `(Get-FileHash -Algorithm SHA256).Hash` 逐字节相同；见 tools/oracle/hashutil.ps1、
     tools/m2-acceptance/preflight.ps1 同一修法）。

  ② `[System.IO.File]::ReadAllBytes` 以 `FileShare.Read` 打开；而 C# 影子 GameSvr 用
     `FileShare.ReadWrite` 持有 `rng-attack-stream.tsv` ⇒ Windows 共享模式下读被拒
     （`The process cannot access the file ... because it is being used by another process`；
     `start-csharp-hook.ps1 -IsolateAttack` 在写完 `ISOLATION_RECORDED` 前抛，此时栈已起/端口已就绪，
     m3cs tick22 F1 复现）⇒ 专用流 sha256 记不出来，只能置空。修法 = 显式
     `FileStream(FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete)` 流式读。

  `Test-M3Sha256HotFileSelfTest` 钉住②：holder 以 `FileShare.ReadWrite` 持有临时件 ⇒ 本函数必须算出
  标准 SHA256（锚 = 64 字节 0x00..0x3F = `fdeab9ac…1108`）；而旧的 `File.ReadAllBytes` 必须被拒。
  若「旧读法没被拒」⇒ 说明该环境没复现共享冲突 ⇒ 自检判红（不许假绿）。改回 `ReadAllBytes` ⇒ 自检必红。
#>

# 64 字节 0x00..0x3F 的标准 SHA256 —— 用作「算法/实现没被换掉」的锚点。
$script:M3Sha256Anchor = 'fdeab9acf3710362bd2658cdc9a29e8f9c757fcf9811603a8c447cd1d9151108'

function Get-M3Sha256Hex([string]$Path) {
  if ([string]::IsNullOrEmpty($Path)) { throw 'Get-M3Sha256Hex: 路径为空' }
  if (-not (Test-Path -LiteralPath $Path)) { throw "Get-M3Sha256Hex: 文件不存在: $Path" }
  $alg = [System.Security.Cryptography.SHA256]::Create()
  $share = ([System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete)
  $fs = $null
  try {
    # 关键：读活文件（被 GameSvr 以 FileShare.ReadWrite 持有）必须显式允许读写共享。
    $fs = [System.IO.FileStream]::new($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, $share)
    return ([System.BitConverter]::ToString($alg.ComputeHash($fs)) -replace '-', '').ToLowerInvariant()
  } finally {
    if ($fs) { $fs.Dispose() }
    $alg.Dispose()
  }
}

function Test-M3Sha256HotFileSelfTest {
  $result = [ordered]@{
    ok            = $true
    reason        = ''
    hot_hash      = ''
    expected      = $script:M3Sha256Anchor
    oldway_denied = $false
  }
  $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ('m3hash-hot-' + [System.Guid]::NewGuid().ToString('N'))
  $holder = $null
  try {
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $path = Join-Path $tmp 'hot-stream.tsv'
    $bytes = New-Object byte[] 64
    for ($i = 0; $i -lt 64; $i++) { $bytes[$i] = [byte]$i }
    [System.IO.File]::WriteAllBytes($path, $bytes)

    # holder 模拟 live GameSvr：以 FileShare.ReadWrite 持有（可读可写可共享）。
    $holder = [System.IO.FileStream]::new($path, [System.IO.FileMode]::Open,
      [System.IO.FileAccess]::ReadWrite,
      ([System.IO.FileShare]::ReadWrite -bor [System.IO.FileShare]::Delete))

    $result.hot_hash = Get-M3Sha256Hex -Path $path
    if ($result.hot_hash -ne $result.expected) {
      $result.ok = $false
      $result.reason = 'hot_hash_mismatch'
    }

    # 改坏必红：旧的 FileShare.Read 读法（File.ReadAllBytes）在 holder 的共享模式下必须被拒。
    $denied = $false
    try { [void][System.IO.File]::ReadAllBytes($path) } catch { $denied = $true }
    $result.oldway_denied = $denied
    if (-not $denied -and $result.ok) {
      $result.ok = $false
      $result.reason = 'sharing_violation_not_reproduced'
    }
  } catch {
    $result.ok = $false
    $result.reason = ('exception:' + $_.Exception.Message)
  } finally {
    if ($holder) { $holder.Dispose() }
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
  }
  return [pscustomobject]$result
}
