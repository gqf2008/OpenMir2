#requires -Version 5
<#
  mir_click.ps1 —— 对**已在运行**的冻结客户端发单次/多次点击（窗口消息路径与 `mir_flow.ps1` 逐字节同一套）。

  为什么存在：`mir_flow.ps1` 的 `-AttackTurn "dx,dy"` 只能表达「按 48x32 像素步长的格偏移」，
  当需要**按截图里怪物的真实屏幕位置**点击时（方向不可预知、或该偏移不落在目标格），
  就得先拿到一张当帧截图，再按像素点。本工具只做三件事：截图 / 右键 / 左键。

  用法：
    powershell -File tools/client/mir_click.ps1 -Shot E:/tmp/x.png
    powershell -File tools/client/mir_click.ps1 -RightClick "430,255" -LeftClick "430,255"
  坐标是**客户端客户区**坐标（与 mir_flow 的 Send-Click 同口径）。
#>
param(
  [string]$Shot = "",
  [string]$RightClick = "",   # "x,y"
  [string]$LeftClick = "",    # "x,y"
  [string]$LeftClick2 = "",   # 第二下（可选；同点再补一击）
  [int]$Repeat = 1
)
$ErrorActionPreference = "Stop"

$src = @"
using System;
using System.Runtime.InteropServices;
public class MirClick {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
}
"@
Add-Type -TypeDefinition $src -ErrorAction SilentlyContinue
Add-Type -AssemblyName System.Drawing

$p = Get-Process MirClinet -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Output "NO_CLIENT"; exit 2 }
$hwnd = $p.MainWindowHandle

$pt = New-Object MirClick+POINT
[void][MirClick]::ClientToScreen($hwnd, [ref]$pt)

function Click([int]$x, [int]$y, [bool]$right) {
  [void][MirClick]::SetCursorPos(($pt.X + $x), ($pt.Y + $y))
  [void][MirClick]::SetForegroundWindow($hwnd)
  Start-Sleep -Milliseconds 200
  $lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
  [void][MirClick]::PostMessage($hwnd, 0x0200, [IntPtr]0, $lp)
  Start-Sleep -Milliseconds 90
  if ($right) {
    [void][MirClick]::PostMessage($hwnd, 0x0204, [IntPtr]2, $lp)
    Start-Sleep -Milliseconds 90
    [void][MirClick]::PostMessage($hwnd, 0x0205, [IntPtr]0, $lp)
  } else {
    [void][MirClick]::PostMessage($hwnd, 0x0201, [IntPtr]1, $lp)
    Start-Sleep -Milliseconds 90
    [void][MirClick]::PostMessage($hwnd, 0x0202, [IntPtr]0, $lp)
  }
  Start-Sleep -Milliseconds 220
}

function Take-Shot([string]$path) {
  $r = New-Object MirClick+RECT
  [void][MirClick]::GetWindowRect($hwnd, [ref]$r)
  $w = $r.R - $r.L; $h = $r.B - $r.T
  $bmp = New-Object System.Drawing.Bitmap($w, $h)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  [void][MirClick]::PrintWindow($hwnd, $hdc, 2)
  $g.ReleaseHdc($hdc); $g.Dispose()
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose()
  Write-Output ("SHOT " + $path)
}

if ($Shot -ne "") { Take-Shot $Shot }

for ($i = 0; $i -lt $Repeat; $i++) {
  if ($RightClick -ne "") {
    $c = $RightClick.Split(",") | ForEach-Object { [int]$_ }
    Click $c[0] $c[1] $true
  }
  if ($LeftClick -ne "") {
    $c = $LeftClick.Split(",") | ForEach-Object { [int]$_ }
    Click $c[0] $c[1] $false
  }
  if ($LeftClick2 -ne "") {
    Start-Sleep -Milliseconds 300
    $c = $LeftClick2.Split(",") | ForEach-Object { [int]$_ }
    Click $c[0] $c[1] $false
  }
}
Write-Output "CLICK_OK"
