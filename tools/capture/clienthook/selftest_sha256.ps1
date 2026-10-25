#requires -Version 5
<#
.SYNOPSIS
  selftest_sha256.ps1 —— Sha256.pas 的**构造级**自检（不注入客户端、不碰生产路径）。

.DESCRIPTION
  Mir2ClientHook.dll 里 chunks.ndjson 的 `sha256` 由 Sha256.pas 计算。本自检：
    1. 把 Sha256.pas 拷到 **$env:TEMP 下的临时目录**，写一个一次性 console 程序；
    2. 用 dcc32（与 build.ps1 同一 BDS）编译并运行；
    3. 对照 FIPS 180-4 已知向量 + 由 .NET SHA256 现算的动态向量（1000 字节 'a'、
       0..255 循环的二进制 1000 字节）；
    4. 清理临时目录（失败也清理）。

  退出码：0 全 PASS；1 有 FAIL；2 SKIP（找不到 dcc32，未能验证——**不是绿**）。

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File selftest_sha256.ps1
#>
param(
  [string]$Bds = "D:\Program Files (x86)\Embarcadero\Studio\21.0"
)
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $PSCommandPath
$dcc = Join-Path $Bds "bin\dcc32.exe"
$rsv = Join-Path $Bds "bin\rsvars.bat"
if (-not (Test-Path -LiteralPath $dcc) -or -not (Test-Path -LiteralPath $rsv)) {
  Write-Output ("SHA256_SELFTEST SKIP dcc32 不存在：" + $dcc + "（未验证，不得当绿）")
  exit 2
}

function Get-Sha256Hex([byte[]]$bytes) {
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant() }
  finally { $sha.Dispose() }
}

$pass = 0
$fails = @()
function Check([string]$name, [bool]$ok, [string]$detail = "") {
  if ($ok) { $script:pass++; Write-Output ("PASS " + $name) }
  else { $script:fails += $name; Write-Output ("FAIL " + $name + "  " + $detail) }
}

$td = Join-Path $env:TEMP ("mir2-sha256-selftest-" + [Guid]::NewGuid().ToString("N").Substring(0, 8))
New-Item -ItemType Directory -Force -Path $td | Out-Null
try {
  Copy-Item -LiteralPath (Join-Path $here "Sha256.pas") -Destination (Join-Path $td "Sha256.pas") -Force

  # 动态期望值：用 .NET 的 SHA256 现算，避免把常量抄错。
  $kiloBytes = [Text.Encoding]::ASCII.GetBytes(('a' * 1000))
  $binBytes = New-Object byte[] 1000
  for ($i = 0; $i -lt 1000; $i++) { $binBytes[$i] = [byte]($i % 256) }
  $expKilo = Get-Sha256Hex $kiloBytes
  $expBin = Get-Sha256Hex $binBytes

  $dpr = @"
program T;
{`$APPTYPE CONSOLE}
uses SysUtils, Sha256;
var b: array[0..999] of Byte; i: Integer; s: AnsiString;
begin
  WriteLn('empty=', Sha256HexStr(''));
  WriteLn('abc=', Sha256HexStr('abc'));
  WriteLn('long=', Sha256HexStr('abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq'));
  s := '';
  for i := 1 to 1000 do s := s + 'a';
  WriteLn('kilo=', Sha256HexStr(s));
  for i := 0 to 999 do b[i] := i mod 256;
  WriteLn('bin=', Sha256Hex(b, 1000));
end.
"@
  [IO.File]::WriteAllText((Join-Path $td "T.dpr"), $dpr, (New-Object Text.ASCIIEncoding))

  Push-Location $td
  $buildLog = Join-Path $td "build.log"
  try {
    cmd /c "`"$rsv`" >nul 2>&1 && dcc32 -B -NSSystem;Winapi;Vcl;Vcl.Win T.dpr" *> $buildLog
    $buildCode = $LASTEXITCODE
  } finally { Pop-Location }
  Check "sha256 unit compiles (dcc32 rc=0)" ($buildCode -eq 0) ((Get-Content $buildLog -Tail 6) -join "`n")
  if ($buildCode -ne 0) { throw "编译失败，后续向量无法验证" }

  $out = & (Join-Path $td "T.exe")
  $runCode = $LASTEXITCODE
  Check "sha256 test program runs" ($runCode -eq 0) ("rc=" + $runCode)
  $map = @{}
  foreach ($line in $out) {
    $s = [string]$line
    $i = $s.IndexOf('=')
    if ($i -gt 0) { $map[$s.Substring(0, $i)] = $s.Substring($i + 1) }
  }
  Check "sha256('') FIPS vector" ($map['empty'] -eq 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855') ([string]$map['empty'])
  Check "sha256('abc') FIPS vector" ($map['abc'] -eq 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad') ([string]$map['abc'])
  Check "sha256(56-byte FIPS vector)" ($map['long'] -eq '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1') ([string]$map['long'])
  Check "sha256(1000 x 'a') == .NET" ($map['kilo'] -eq $expKilo) ([string]$map['kilo'] + " != " + $expKilo)
  Check "sha256(binary 0..255 x4) == .NET" ($map['bin'] -eq $expBin) ([string]$map['bin'] + " != " + $expBin)
}
finally {
  Remove-Item -LiteralPath $td -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Output ("SHA256_SELFTEST_PASS=" + $pass + " FAIL=" + $fails.Count)
foreach ($n in $fails) { Write-Output ("FAILED: " + $n) }
if ($fails.Count -gt 0) { exit 1 }
Write-Output "SHA256_SELFTEST_OK"
exit 0
