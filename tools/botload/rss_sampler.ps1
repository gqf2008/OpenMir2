# RSS 采样器（L2-5 ③ 段 4 编排的辅助进程；由 run_l25_slice.ps1 以 Start-Process 起）。
#
# 为什么不是 Start-Job：外层用 `& pwsh -File ... 2>&1` 捕获输出时会等**孙进程**退出才返回
# （实测 8s probe：脚本立刻打印 DONE，捕获却等到孙进程睡醒）。独立进程 + 文件产物最稳。
#
# 输入：`-PidFile` = start_mir2_server.ps1 写出的 pid json（每角色 {role,pid,ports,...}）。
# 输出：`-Out` NDJSON，每行 {ts_ms, role, pid, ws_bytes, priv_bytes}。
# 退出：`-StopFile` 出现后（最多再采一轮）退出。
param(
  [Parameter(Mandatory = $true)][string]$PidFile,
  [Parameter(Mandatory = $true)][string]$Out,
  [Parameter(Mandatory = $true)][string]$StopFile,
  [int]$EverySec = 2
)

$ErrorActionPreference = 'Continue'
while (-not (Test-Path -LiteralPath $StopFile)) {
  try {
    $doc = Get-Content -LiteralPath $PidFile -Raw | ConvertFrom-Json
    # start_mir2_server.ps1 的 pid 文件形如 { generated_at, root, server_exe, entries:[...] }；
    # 也兼容裸数组/单条（旧夹具）。
    $entries = if ($null -ne $doc.PSObject.Properties["roles"]) { @($doc.roles) } elseif ($null -ne $doc.PSObject.Properties["entries"]) { @($doc.entries) } else { @($doc) }
    if ($entries.Count -eq 1 -and $entries[0] -is [array]) { $entries = @($entries[0]) }
    foreach ($e in $entries) {
      $proc = Get-Process -Id ([int]$e.pid) -ErrorAction SilentlyContinue
      if ($null -eq $proc) { continue }
      $row = [pscustomobject]@{
        ts_ms      = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        role       = [string]$e.role
        pid        = [int]$e.pid
        ws_bytes   = [int64]$proc.WorkingSet64
        priv_bytes = [int64]$proc.PrivateMemorySize64
      }
      Add-Content -LiteralPath $Out -Value ($row | ConvertTo-Json -Compress)
    }
  } catch { }
  Start-Sleep -Seconds $EverySec
}
exit 0
