#requires -Version 5
<#
.SYNOPSIS
  build.ps1 —— 编译 C2 线的客户端侧 dump 工具（Delphi 10.4 / Win32）。

.DESCRIPTION
  MirClinet.exe 是 Win32（PE machine=014c），所以 hook DLL 与注入器都必须是 32 位；
  本机可用的 32 位编译器是 Embarcadero Delphi 10.4（dcc32），与仓库里
  E:\MirServer\build-client-104.ps1 用的是同一套 BDS。

  产物：tools/capture/clienthook/bin/Mir2ClientHook.dll、Inject.exe（不入库，见 .gitignore）

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File build.ps1
#>
param(
  [string]$Bds = "d:\Program Files (x86)\Embarcadero\Studio\21.0",
  [switch]$Force
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Bin = Join-Path $Here "bin"
New-Item -ItemType Directory -Force -Path $Bin | Out-Null

$rsvars = Join-Path $Bds "bin\rsvars.bat"
$dcc = Join-Path $Bds "bin\dcc32.exe"
if (-not (Test-Path $dcc)) { throw "找不到 dcc32.exe：$dcc（BDS 未安装？）" }

function Build-One([string]$Src, [string]$OutName) {
  $out = Join-Path $Bin $OutName
  $log = Join-Path $Bin ($OutName + ".build.log")
  $cmd = "call `"$rsvars`" >nul 2>&1 && dcc32 -B -NSSystem;Winapi;Vcl;Vcl.Win -E`"$Bin`" `"$(Join-Path $Here $Src)`""
  Write-Output ("BUILD " + $Src)
  & $env:ComSpec /c $cmd *> $log
  $code = $LASTEXITCODE
  if ($code -ne 0) {
    Write-Output ("  编译失败（exit " + $code + "），编译器输出（尾 25 行）:")
    Get-Content $log -Tail 25 | ForEach-Object { Write-Output ("  " + $_) }
    throw "编译失败：$Src"
  }
  if (-not (Test-Path $out)) { throw "未生成 $out" }
  $fi = Get-Item $out
  Write-Output ("  OK " + $fi.Name + " " + $fi.Length + " bytes (log: " + (Split-Path $log -Leaf) + ")")
}

Build-One "Mir2ClientHook.dpr" "Mir2ClientHook.dll"
Build-One "Inject.dpr" "Inject.exe"
Write-Output "BUILD_OK -> $Bin"
