# tools/oracle/detached.ps1 —— 起「长命子进程」又不占住本脚本 stdout/stderr 管道的公共帮手。
#
# 坑（2026-10-10 S2 实测；用户报的症状是"产物齐全，但 stdout 不回、会话不退"）：
#   PowerShell 5.1 的
#       Start-Process -FilePath <长命服务> -RedirectStandardOutput a -RedirectStandardError b
#   是用 `CreateProcess(bInheritHandles=true)` 起的子进程 —— 子进程会把父脚本的
#   **stdout/stderr 管道句柄一起复制过去**（只多一份句柄，并不改它自己的 stdout，日志照旧进文件）。
#   只要子进程活着，调用方（管道 / IDE / 别的 agent 的 harness）就读不到 EOF：
#   脚本自己的工作早已做完，表现为"明明跑完了却不回、会话不退"，服务端是长命进程 ⇒ 等于永远卡住。
#
#   实测（子进程故意活 12s、父脚本立刻返回；测量管道多久才收到 EOF）：
#       Start-Process + PowerShell 级重定向（原写法）                    → 11~12s（= 子进程寿命）✗
#       同一句去掉 -WindowStyle Hidden                                   → 12s ✗
#       Start-Process cmd.exe + 子进程内重定向（cmd 自己解析 > 和 2>）    → 11s ✗（Start-Process 仍走继承路径）
#       .NET ProcessStartInfo + UseShellExecute=$true（本函数）          → 0s  ✓
#
#   纠偏：**不要在 PowerShell 层给长命服务重定向，也不要用 Start-Process 起它**。
#   改成 .NET `ProcessStartInfo{ UseShellExecute = $true }`（走 ShellExecuteEx，句柄完全不继承），
#   把 `> out 2> err` 写进 cmd 自己解析的那一段命令行，日志照样落文件。
#
# 已知边界（故意做成"响亮失败"而不是静默不启动）：
#   该命令行形式经过 **cmd 的一层引号解析**，路径里带空格时不可靠（实测：可执行路径与日志路径
#   都带空格时会静默起不来）。本仓库的部署路径（E:\MirServer\...）不含空格；
#   一旦传进来的路径含空格，本函数直接报错退出，绝不让调用方拿到"看起来成功其实没起"的结果。
#
# 用法：
#   . (Join-Path $PSScriptRoot 'detached.ps1')
#   Start-DetachedProcess -Exe $exe -WorkDir $dir -OutLog $out.log -ErrLog $err.log [-Arguments @('a')]

function Start-DetachedProcess {
    param(
        [Parameter(Mandatory = $true)][string]$Exe,
        [Parameter(Mandatory = $true)][string]$WorkDir,
        [Parameter(Mandatory = $true)][string]$OutLog,
        [Parameter(Mandatory = $true)][string]$ErrLog,
        [string[]]$Arguments = @()
    )

    foreach ($p in @($Exe, $WorkDir, $OutLog, $ErrLog)) {
        if ($p -match '\s') {
            throw "Start-DetachedProcess：路径含空格不可靠（cmd 引号层）：$p —— 请换掉 ServerRoot/日志目录"
        }
    }
    foreach ($log in @($OutLog, $ErrLog)) {
        $dir = Split-Path -Parent $log
        if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
        if (Test-Path -LiteralPath $log) { Remove-Item -LiteralPath $log -Force }   # 与旧行为一致：每次重启截断
    }

    # cmd 官方推荐的"双写引号"形式：/c ""<exe>" args > "<out>" 2> "<err>""
    $quoted = @()
    foreach ($a in $Arguments) { $quoted += ('"' + $a + '"') }
    $cmdline = '/c ""' + $Exe + '" ' + ($quoted -join ' ') + ' > "' + $OutLog + '" 2> "' + $ErrLog + '""'

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = 'cmd.exe'
    $psi.Arguments = $cmdline
    $psi.WorkingDirectory = $WorkDir
    $psi.UseShellExecute = $true          # ← 关键：句柄不继承，管道立刻还给调用方
    $psi.WindowStyle = 'Hidden'
    [void][System.Diagnostics.Process]::Start($psi)
}

# 连子进程树一起收掉：普通 Stop-Process 只杀父进程，留下的孩子同样可能占着管道/句柄
function Stop-ProcessTree {
    param([Parameter(Mandatory = $true)][int]$Id)
    & taskkill /T /F /PID $Id 2>&1 | Out-Null
}
