#requires -Version 5
<#
.SYNOPSIS
  mir_flow.ps1 — 真实客户端基线流程驱动（登录→选人→进游戏→走路→小退→再进）。

.DESCRIPTION
  从 E:\tmp\mirflow2.ps1 / amtest.ps1 收编而来：同一套 Win32 消息点击/打字/截图，
  增加了阶段标记（NDJSON，供日志时间线对齐）与参数化账号/输出目录。
  点击坐标沿用旧脚本已验证值（1024x768 窗口）。

  阶段序列（每阶段写一条 {"t_wall_ms","stage"} 到 -StageLog）：
    start → login_id → login_pwd → login_submit → charsel → enter_game
    → ingame → walk ×N → logout_alt_x → logout_confirm → back_charsel
    → reenter → reentered → done

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File mir_flow.ps1 -OutDir E:\tmp\flow1
#>
param(
  [string]$Account  = "mir2test",
  [string]$Password = "mir2pass",
  [string]$OutDir   = "E:/tmp/mirflow",
  [string]$StageLog = "",          # 默认 $OutDir/stages.ndjson
  [string]$RunDir   = "D:\MirClient-run",
  [int]$TypeDelayMs = 90,
  [int]$WalkClicks  = 2,           # 进游戏后点地走几步（0 跳过）
  [int]$WalkX       = 902,
  [int]$WalkY       = 650,
  [string]$WalkPath = "",          # 形如 "900,300;900,300;..." 的走位序列（每步截图）
  [int]$LoginSubmitClicks = 1,     # 提交按钮补点次数（偶发被吞；建角探测时可给 3）
  [int]$CharSlot = 1,              # 选角界面选第几个角色（1=左，2=右）
  [switch]$SkipReenter,
  [switch]$LeaveClientRunning,
  # ---- 建角阶段（用无角色的新账号走 创建角色 流程；坐标可用参数覆盖）----
  [switch]$CreateChar,
  [string]$CharName = "",
  [string]$NewChrBtn = "514,581",  # 选角界面「创建人物」按钮（实测：image(517,610) - origin(3,29)）
  [string]$ChrNameBox = "670,209", # 「新加入」姓名输入框（实测 image(673,238)）
  [string]$ChrJobBtn  = "598,267", # 职业=战士（image(601,296)）
  [string]$ChrSexBtn  = "632,340", # 性别=男（image(635,369)）
  [string]$ChrOkBtn = "670,466",   # 「新加入」提交（实测 image(673,495)）
  # ---- 自动挂机（Ctrl+Alt+X）：走路 + 打怪，覆盖 移动/攻击 阶段 ----
  [int]$AutoPlaySec = 0,
  # ---- C2：客户端进程内 dump（不改客户端文件；见 tools/capture/clienthook/）----
  [string]$InjectDll = "",   # Mir2ClientHook.dll 完整路径
  [string]$InjectExe = "",   # Inject.exe 完整路径
  [string]$HookOut   = "",   # dump 输出目录（作为环境变量给子进程，DLL 启动时读取）
  [int]$AttackClicks = 0,    # 进世界后绕角色点 N 下（点到怪=CM_HIT，点到地=CM_WALK）
  [string]$AttackRing = "",  # 覆盖默认点击环："dx,dy;dx,dy;..."（相对角色屏幕中心）
  # ---- 会话拆除（「小退」阶段的一种真实形态）----
  # 客户端自己的小退要走 AppLogout 的确认框（Alt+X → 确定 → CM_SOFTCLOSE 1009），
  # 合成按键打不穿那个模态框（实测 keybd_event / SendInput 都不行）；
  # 这里改用**同账号再登录**：本账号在别处登录会触发 LoginSrv 顶号 → 会话被踢 →
  # 客户端收到 SM_OUTOFCONNECTION(528)。前提：账号名 = <BotPrefix><序号>，
  # 这样 BotSrv（LoginId = 前缀+序号）才能用同一个账号登进来。
  [string]$KickVia = "",     # BotSrv 前缀（账号名必须是 <前缀>0，即本客户端用的账号）
  [int]$KickWaitSec = 30,
  [string]$BotRunner = ""    # tools/botload/run_bots.ps1 路径（默认自动定位）
)

$ErrorActionPreference = "Stop"
if ($StageLog -eq "") { $StageLog = Join-Path $OutDir "stages.ndjson" }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$src = @"
using System;
using System.Runtime.InteropServices;
public class MirFlow {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, int dx, int dy, uint data, IntPtr extra);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, IntPtr extra);

  // SendInput：按扫描码注入真实键盘事件（Alt+X 这类组合键在部分 VCL 程序里
  // 用 keybd_event 不生效；SendInput 走驱动层，最接近真人按键）
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public KEYBDINPUT ki; public int pad1; public int pad2; }
  [DllImport("user32.dll", SetLastError = true)] public static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);
  public const uint INPUT_KEYBOARD = 1;
  public const uint KEYEVENTF_KEYUP = 0x0002;
  public const uint KEYEVENTF_SCANCODE = 0x0008;
  public static void SendAltX() {
    // 用虚拟键注入（wVk），不要只填 wScan 而不带 KEYEVENTF_SCANCODE：
    // 那样 wVk=0 的事件是无效的，Alt 根本不会按下，客户端读到的 Shift 里没有 ssAlt，
    // Alt+X 就落到"没按 Alt"的分支（实测：既不弹小退确认框，也不发 CM_SOFTCLOSE）。
    INPUT[] seq = new INPUT[4];
    seq[0].type = INPUT_KEYBOARD; seq[0].ki.wVk = 0x12;                                       // VK_MENU down
    seq[1].type = INPUT_KEYBOARD; seq[1].ki.wVk = 0x58;                                       // VK_X down
    seq[2].type = INPUT_KEYBOARD; seq[2].ki.wVk = 0x58; seq[2].ki.dwFlags = KEYEVENTF_KEYUP;  // X up
    seq[3].type = INPUT_KEYBOARD; seq[3].ki.wVk = 0x12; seq[3].ki.dwFlags = KEYEVENTF_KEYUP;  // Alt up
    SendInput(4, seq, Marshal.SizeOf(typeof(INPUT)));
  }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X,Y; }
}
"@
Add-Type -TypeDefinition $src -ErrorAction SilentlyContinue
Add-Type -AssemblyName System.Drawing

function Stage([string]$name) {
  $ms = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
  $line = '{"t_wall_ms":' + $ms + ',"stage":"' + $name + '"}'
  Add-Content -Path $StageLog -Value $line -Encoding utf8
  Write-Output ("STAGE " + $name + " @ " + $ms)
}

# ---- 启动客户端 ----
Stage "start"
Get-Process MirClinet -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
if ($HookOut -ne "") { $env:MIR2_HOOK_OUT = $HookOut }   # 必须在 Start-Process 之前：子进程继承
Start-Process -FilePath "$RunDir\run-release.cmd" -WorkingDirectory $RunDir
Start-Sleep -Seconds 14
$p = Get-Process MirClinet -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { throw "客户端进程 MirClinet 未起来" }
$script:hwnd = $p.MainWindowHandle
Write-Output "CLIENT_PID=$($p.Id)"

if ($InjectDll -ne "") {
  Stage "inject_hook"
  if ($InjectExe -eq "") { $InjectExe = Join-Path (Split-Path $InjectDll -Parent) "Inject.exe" }
  $injOut = & $InjectExe --pid $p.Id --dll $InjectDll 2>&1
  Write-Output ("INJECT: " + ($injOut -join " "))
  # 等 DLL 自己的就绪标记（不建连、不探测）：拿不到就明确失败，别让空 dump 混过去
  $flag = Join-Path $HookOut "hook-ready.flag"
  $deadline2 = (Get-Date).AddSeconds(10)
  while (-not (Test-Path $flag) -and (Get-Date) -lt $deadline2) { Start-Sleep -Milliseconds 300 }
  if (Test-Path $flag) { Write-Output "HOOK_READY" }
  else {
    if (Test-Path (Join-Path $HookOut "hook-failed.flag")) {
      throw "hook 注入后未就绪（hook-failed.flag：导入表里没找齐 wsock32 的 send/recv/connect/closesocket）"
    }
    throw "hook 未就绪：$flag 未出现（注入失败？）"
  }
}

$pt = New-Object MirFlow+POINT
[void][MirFlow]::ClientToScreen($script:hwnd, [ref]$pt)
# 选角界面两个角色的「选择」按钮 x 坐标（实测 image(279,577)/(831,577) - origin(3,29)）
$CharSlotX = if ($CharSlot -eq 2) { 828 } else { 276 }

function Send-Click([int]$x, [int]$y, [int]$times = 2) {
  [void][MirFlow]::SetCursorPos(($pt.X + $x), ($pt.Y + $y))
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 200
  $lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
  for ($i = 0; $i -lt $times; $i++) {
    [void][MirFlow]::PostMessage($script:hwnd, 0x0200, [IntPtr]0, $lp)
    [void][MirFlow]::PostMessage($script:hwnd, 0x0201, [IntPtr]1, $lp)
    Start-Sleep -Milliseconds 90
    [void][MirFlow]::PostMessage($script:hwnd, 0x0202, [IntPtr]0, $lp)
    Start-Sleep -Milliseconds 220
  }
}

function Send-RealClick([int]$x, [int]$y) {
  # 真鼠标事件（mouse_event）：Delphi 自绘按钮对 PostMessage 偶发不响应时用它兜底
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 200
  [void][MirFlow]::SetCursorPos(($pt.X + $x), ($pt.Y + $y))
  Start-Sleep -Milliseconds 200
  [MirFlow]::mouse_event(0x02, 0, 0, 0, [IntPtr]0)   # LEFTDOWN
  Start-Sleep -Milliseconds 90
  [MirFlow]::mouse_event(0x04, 0, 0, 0, [IntPtr]0)   # LEFTUP
  Start-Sleep -Milliseconds 250
}

function Send-Text([string]$text) {
  foreach ($ch in $text.ToCharArray()) {
    [void][MirFlow]::SendMessage($script:hwnd, 0x0102, [IntPtr][int]$ch, [IntPtr]1)
    Start-Sleep -Milliseconds $TypeDelayMs
  }
}

function Send-Bs([int]$times = 25) {
  for ($i = 0; $i -lt $times; $i++) {
    [void][MirFlow]::SendMessage($script:hwnd, 0x0102, [IntPtr]8, [IntPtr]1)
    Start-Sleep -Milliseconds 30
  }
}

function Shot([string]$name) {
  $r = New-Object MirFlow+RECT
  [void][MirFlow]::GetWindowRect($script:hwnd, [ref]$r)
  $bmp = New-Object System.Drawing.Bitmap(($r.R - $r.L), ($r.B - $r.T))
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $dc = $g.GetHdc()
  [void][MirFlow]::PrintWindow($script:hwnd, $dc, 2)
  $g.ReleaseHdc($dc); $g.Dispose()
  $path = Join-Path $OutDir $name
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  Write-Output ("SHOT " + $path)
}

# ---- 登录 ----
Shot "01_login.png"
Stage "login_id"
Send-Click 530 350
Send-Bs 25
Send-Text $Account
Shot "02_id.png"
Stage "login_pwd"
Send-Click 530 380
Send-Bs 25
Send-Text $Password
Shot "03_pwd.png"
Stage "login_submit"
# 提交按钮：坐标 575,435 是实测能过的那一点；偶发被吞时按 LoginSubmitClicks 补点
for ($r = 0; $r -lt $LoginSubmitClicks; $r++) {
  Send-Click 575 435
  Start-Sleep -Seconds 5
}

# ---- 选人（可选：建角）----
Stage "charsel"
Shot "04_charsel.png"

if ($CreateChar) {
  $nb = $NewChrBtn.Split(",") | ForEach-Object { [int]$_ }
  $nbox = $ChrNameBox.Split(",") | ForEach-Object { [int]$_ }
  $okb = $ChrOkBtn.Split(",") | ForEach-Object { [int]$_ }
  if ($CharName -eq "") { $CharName = "g" + (Get-Random -Minimum 1000 -Maximum 9999) }
  Stage "create_char_open"
  Send-Click $nb[0] $nb[1] 1
  Start-Sleep -Seconds 2
  Shot "04b_newchr_panel.png"
  Stage "create_char_name"
  Send-Click $nbox[0] $nbox[1] 1
  Start-Sleep -Milliseconds 400
  Send-Bs 20
  Send-Text $CharName
  Shot "04c_newchr_name.png"
  if ($ChrJobBtn) {
    $jb = $ChrJobBtn.Split(",") | ForEach-Object { [int]$_ }
    Send-Click $jb[0] $jb[1] 1
    Start-Sleep -Milliseconds 400
  }
  if ($ChrSexBtn) {
    $sb = $ChrSexBtn.Split(",") | ForEach-Object { [int]$_ }
    Send-Click $sb[0] $sb[1] 1
    Start-Sleep -Milliseconds 400
  }
  Stage "create_char_submit"
  Send-Click $okb[0] $okb[1] 1
  Start-Sleep -Seconds 5
  Shot "04d_newchr_done.png"
  # 建角成功后回到选角界面，重新点选新角色
  Send-Click $CharSlotX 550
  Start-Sleep -Seconds 1
}

# ---- 选人进游戏 ----
Stage "enter_game"
Send-Click $CharSlotX 550
Send-Click 517 552
# 进图后服务端会推「公告」模态框（SM_SENDNOTICE 658），客户端点掉后回 CM_LOGINNOTICEOK(1018)，
# 服务端才继续下发世界数据。**窗口只有 10 秒**：PlayObject.RunNotice 里
# `(GetTickCount - WaitLoginNoticeOkTick) > 10*1000 ⇒ BoEmergencyClose`，
# 超时角色会被踢、世界数据永远不来（症状=黑屏 + 只收到 445B 公告）。
# 因此这里不能"等 13 秒再点一次"，要**从进图就开始高频点确定**，覆盖公告出现的时刻。
Stage "notice_poll"
$noticeClicked = $false
for ($i = 0; $i -lt 12; $i++) {
  Send-Click 522 525 1
  Start-Sleep -Milliseconds 1200
  if ($i -eq 2) { Shot "05_ingame.png" }
  if ($i -ge 6 -and -not $noticeClicked) { $noticeClicked = $true; Shot "05b_after_notice.png" }
}
if (-not $noticeClicked) { Shot "05b_after_notice.png" }
Stage "ingame"
Start-Sleep -Seconds 2
Shot "05c_ingame_ready.png"

# ---- 自动挂机：Ctrl+Alt+X 开，跑 N 秒（走路+打怪），再关 ----
if ($AutoPlaySec -gt 0) {
  Stage "autoplay_on"
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 300
  [MirFlow]::keybd_event(0x11, 0, 0, [IntPtr]0)   # Ctrl
  [MirFlow]::keybd_event(0x12, 0, 0, [IntPtr]0)   # Alt
  Start-Sleep -Milliseconds 150
  [MirFlow]::keybd_event(0x58, 0, 0, [IntPtr]0)   # X
  Start-Sleep -Milliseconds 100
  [MirFlow]::keybd_event(0x58, 0, 2, [IntPtr]0)
  Start-Sleep -Milliseconds 100
  [MirFlow]::keybd_event(0x12, 0, 2, [IntPtr]0)
  [MirFlow]::keybd_event(0x11, 0, 2, [IntPtr]0)
  for ($t = 0; $t -lt $AutoPlaySec; $t += 10) {
    Start-Sleep -Seconds 10
    Shot ("05c_autoplay_" + $t + "s.png")
  }
  Stage "autoplay_off"
  [MirFlow]::keybd_event(0x11, 0, 0, [IntPtr]0)
  [MirFlow]::keybd_event(0x12, 0, 0, [IntPtr]0)
  Start-Sleep -Milliseconds 150
  [MirFlow]::keybd_event(0x58, 0, 0, [IntPtr]0)
  Start-Sleep -Milliseconds 100
  [MirFlow]::keybd_event(0x58, 0, 2, [IntPtr]0)
  Start-Sleep -Milliseconds 100
  [MirFlow]::keybd_event(0x12, 0, 2, [IntPtr]0)
  [MirFlow]::keybd_event(0x11, 0, 2, [IntPtr]0)
  Start-Sleep -Seconds 1
  Shot "05d_autoplay_off.png"
}

# ---- 攻击：在角色周围按环状点几下（比奇省刷怪区怪物密集，点到即出 CM_HIT）----
if ($AttackClicks -gt 0) {
  $ring = @("0,-40", "40,0", "0,40", "-40,0", "28,-28", "28,28", "-28,28", "-28,-28",
            "70,0", "0,70", "-70,0", "0,-70")
  if ($AttackRing -ne "") { $ring = $AttackRing.Split(";") }
  for ($i = 0; $i -lt [Math]::Min($AttackClicks, $ring.Count); $i++) {
    $d = $ring[$i].Split(",") | ForEach-Object { [int]$_ }
    Stage ("attack_" + ($i + 1))
    Send-Click (512 + $d[0]) (384 + $d[1]) 1
    Start-Sleep -Milliseconds 900
  }
  Shot "06_attack.png"
}

# ---- 走路 ----
for ($i = 1; $i -le $WalkClicks; $i++) {
  Stage ("walk_" + $i)
  Send-Click $WalkX $WalkY 1
  Start-Sleep -Seconds 2
}
if ($WalkClicks -gt 0) { Shot "06_walk.png" }

# ---- 可脚本化走位（探怪/攻击用）：-WalkPath "x1,y1;x2,y2;..."，每步后截图 ----
if ($WalkPath) {
  $step = 0
  foreach ($pt2 in $WalkPath.Split(";")) {
    if ($pt2.Trim() -eq "") { continue }
    $step++
    $xy = $pt2.Split(",") | ForEach-Object { [int]$_ }
    Stage ("path_" + $step)
    Send-Click $xy[0] $xy[1] 1
    Start-Sleep -Seconds 3
    Shot ("06path_" + $step + ".png")
  }
}

# ---- 小退：Alt+X -> 确定 ----
# ---- 会话拆除（顶号）：本账号在 BotSrv 侧再登录一次 ----
if ($KickVia -ne "") {
  Stage "kick_relogin"
  if ($BotRunner -eq "") { $BotRunner = Join-Path (Split-Path $PSCommandPath -Parent) "..\botload\run_bots.ps1" }
  $kickLog = Join-Path $OutDir "kick.out.log"
  $kickProc = Start-Process -FilePath "powershell" -PassThru -WindowStyle Hidden `
    -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $BotRunner,
                    "-Count", "1", "-Prefix", $KickVia, "-SkipBuild", "-TimeoutSec", "60") `
    -RedirectStandardOutput $kickLog -RedirectStandardError "$kickLog.err"
  Start-Sleep -Seconds $KickWaitSec
  Shot "06b_after_kick.png"
  Get-Process BotSrv -ErrorAction SilentlyContinue | Stop-Process -Force
  if ($kickProc -and -not $kickProc.HasExited) { Stop-Process -Id $kickProc.Id -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 2
}

Stage "logout_alt_x"
# 小退入口有两条，依次试（客户端 ClMain.pas 的 `Word('X')` 分支 vs FState.pas 的
# DBotLogout 按钮 → 两者最终都走 TfrmMain.AppLogout → 确认框 → CM_SOFTCLOSE(1009)）：
#   ① Alt+X：必须用 keybd_event（PostMessage 不更新系统键盘状态，客户端读不到 ssAlt）
#   ② F12 打开选项面板 → 点「小退」按钮（FState.pas:2522 算出 Left=754, Top=104）
[void][MirFlow]::SetForegroundWindow($script:hwnd)
Start-Sleep -Milliseconds 400
for ($i = 0; $i -lt 3; $i++) {
  [MirFlow]::keybd_event(0x1B, 0, 0, [IntPtr]0)
  Start-Sleep -Milliseconds 80
  [MirFlow]::keybd_event(0x1B, 0, 2, [IntPtr]0)
  Start-Sleep -Milliseconds 250
}
# 关键：先把键盘焦点从聊天输入框拿回来——
# 输入框有焦点时 X 会被当成字符吃掉（实测曾发出 CM_SAY 3030 而不是小退）。
# 两手都上：① 点地图；② 回车键（聊天框开着=发送并收起；已收起再按=打开，
# 所以成对按两次，最终状态必为"收起"）。
Send-Click 512 200 1
Start-Sleep -Milliseconds 500
[MirFlow]::keybd_event(0x0D, 0, 0, [IntPtr]0)
Start-Sleep -Milliseconds 120
[MirFlow]::keybd_event(0x0D, 0, 2, [IntPtr]0)
Start-Sleep -Milliseconds 400
[MirFlow]::keybd_event(0x0D, 0, 0, [IntPtr]0)
Start-Sleep -Milliseconds 120
[MirFlow]::keybd_event(0x0D, 0, 2, [IntPtr]0)
Start-Sleep -Milliseconds 600
for ($k = 0; $k -lt 3; $k++) {
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 300
  [MirFlow]::SendAltX()
  Start-Sleep -Seconds 2
  Shot ("07_logout_altx" + $k + ".png")
}
# ② 选项面板路径：F12 → 小退按钮 → 确认
Stage "logout_menu"
[void][MirFlow]::SetForegroundWindow($script:hwnd)
Start-Sleep -Milliseconds 250
[MirFlow]::keybd_event(0x7B, 0, 0, [IntPtr]0)   # F12
Start-Sleep -Milliseconds 120
[MirFlow]::keybd_event(0x7B, 0, 2, [IntPtr]0)
Start-Sleep -Seconds 2
Shot "07_logout_menu_open.png"
Send-Click 754 104 1
Start-Sleep -Seconds 2
Shot "07_logoutdlg.png"
Stage "logout_confirm"
Send-Click 554 435
Start-Sleep -Seconds 8
Stage "back_charsel"
Shot "08_back_charsel.png"

if (-not $SkipReenter) {
  Stage "reenter"
  Send-Click $CharSlotX 550
  Send-Click 517 552
  Start-Sleep -Seconds 14
  Stage "reentered"
  Shot "09_reenter.png"
}

Stage "done"
if (-not $LeaveClientRunning) {
  Get-Process MirClinet -ErrorAction SilentlyContinue | Stop-Process -Force
  Write-Output "CLIENT_STOPPED"
}
Write-Output "FLOW_OK"
