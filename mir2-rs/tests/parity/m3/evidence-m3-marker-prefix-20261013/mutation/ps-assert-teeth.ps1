param([Parameter(Mandatory=$true)][string]$Mode)
$ErrorActionPreference = 'Stop'
$src = Get-Content -Raw 'E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m3cs\mir2-rs\tests\parity\m3\run_pair.ps1'
$m = [regex]::Match($src, 'function Assert-M3MarkerPrefix \{[\s\S]*?\r?\n\}')
if (-not $m.Success) { Write-Output 'RED: Assert-M3MarkerPrefix 未在 run_pair.ps1 找到'; exit 3 }
Invoke-Expression $m.Value
$good = Join-Path 'E:/Users/gxh/Documents/GitHub/OpenMir2/.claude/worktrees/m3cs/mir2-rs/tests/parity/m3/evidence-m3-marker-prefix-20261013/mutation' 'helper-good.jsonl'
$bad = Join-Path 'E:/Users/gxh/Documents/GitHub/OpenMir2/.claude/worktrees/m3cs/mir2-rs/tests/parity/m3/evidence-m3-marker-prefix-20261013/mutation' 'helper-bad.jsonl'
Set-Content -Path $good -Value @('M3SHOPSEQ {"phase":"arm"}', 'M3SHOPSEQ {"phase":"done","ops":0}') -Encoding utf8
Set-Content -Path $bad -Value @('{"phase":"arm"}', 'M3SHOPSEQ {"phase":"done","ops":0}') -Encoding utf8
if ($Mode -eq 'good') {
  Assert-M3MarkerPrefix -Path $good -Marker 'M3SHOPSEQ'
  Write-Output 'GREEN: 带前缀 markers 通过 Assert-M3MarkerPrefix'
} else {
  Assert-M3MarkerPrefix -Path $bad -Marker 'M3SHOPSEQ'
  Write-Output 'BUG: 去前缀 markers 本应被拒绝'
  exit 9
}
