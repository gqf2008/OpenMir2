# 真图门禁：对拍部署件读数（0.map / 0129.map / 0100.map）。
# 用法：powershell -ExecutionPolicy Bypass -File scripts\gate-real-maps.ps1
# 可选：-MapDir <目录> -TargetDir <隔离 target 目录>
param(
    [string]$MapDir = 'E:\MirServer\M2GameSvr\Map',
    [string]$TargetDir = 'E:\tmp\openmir2-mapload-target'
)

$ErrorActionPreference = 'Stop'
$env:MIR2_MAP_DIR = $MapDir
$env:CARGO_TARGET_DIR = $TargetDir

$mir2rs = Resolve-Path (Join-Path $PSScriptRoot '..\..\..')
Push-Location $mir2rs
try {
    Write-Host "MIR2_MAP_DIR=$MapDir"
    cargo test -p mir2-envir --test real_maps --offline -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
