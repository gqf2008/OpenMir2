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
    → ingame → talk → talk_sent → see_others_wait → see_others → walk ×N → logout_alt_x → logout_confirm → back_charsel
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
  # ---- 跑键（CM_RUN 3013）探针：在**目标点**按下左键并按住 ----------------
  # 冻结客户端唯一的发跑支 `ClMain.pas:2190 caRun`：`g_boCanStartRun or (g_nRunReadyCount>=1)` 门后
  #   · `CanRun`（Actor.pas:2162）要求 `HP >= RUN_MINHEALTH(10)` ⇒ 服务端 52 必须按客户端
  #     58B `TAbility` 布局发（`MIR2_CLIENT_ABILITY_LAYOUT`，默认开；根因与实测见 61-*）；
  #   · `GetDistance>=2` + `IsUnLockAction` + `g_PlayScene.CanRun`（两格路径可走，开阔地才成立）；
  #   · `g_ChrAction:=caRun` 只在「点 >1 格」时由 `_DXDrawMouseDown`（ClMain.pas:5458）置 ⇒
  #     必须在**目标点**上按下（`FormMouseMove` 不喂 `_DXDrawMouseDown`）。
  # `$RunHoldMs>0` 时绕角色 8 方向各在目标点按住这么多毫秒并截图。
  # 真机配对（61-*）：58B + 开阔地 ⇒ `CM_RUN(3013)` 出现；40B ⇒ `CM_RUN=0`。
  [int]$RunHoldMs = 0,             # >0 = 绕角色中心两圈 8 方向各按住拖动这么多毫秒并截图
  [int]$LoginSubmitClicks = 1,     # 提交按钮补点次数（偶发被吞；建角探测时可给 3）
  [int]$CharSlot = 1,              # 选角界面选第几个角色（1=左，2=右）
  [switch]$SkipReenter,
  [switch]$SkipLogout,          # 三路径 ④：跳过所有小退/顶号动作（disconnect/shutdown 需客户端留在世界里）
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
  # ---- 转向后攻击（片68 ③b 阳性对照：空地左键攻击不先发 CM_TURN ⇒ 服务端用旧朝向取前方格 ⇒ 必打空）----
  # -AttackTurn "dx,dy" = **格偏移**（"1,0"=东 / "0,-1"=北 / "1,-1"=东北）：每次攻击左键前先**右键**点该相邻格触发
  # CM_TURN（客户端只有右键支发 CM_TURN：ClMain.pas:5344 块内 :5454 SendMsg；服务端
  # ClientChangeDir 立即置 Dir ⇒ 随后 CM_HIT 的前方格即该向）。像素换算固定按**格心锚
  # (512,304)**（48x32 px/格），不是 -CenterX/-CenterY：259 是精灵中心标定，格点击锚 304（推导见
  # rationale.md）。仅与 -AttackClicks 联动；默认空 = 现状逐字节不变。
  [string]$AttackTurn = "",
  # ---- AttackTurn 像素映射标定与「新映射」（卡 openmir2-mirflow-attackturn-projection；tier=L）----
  # 现状（旧路径，默认逐字节不变）：右键/左键点 (512 + dx*48, 304 + dy*32)。
  #   · 锚点 (512,304) 有源码依据：512 = SCREENWIDTH div 2；304 = ShiftYOffset
  #     = Round((768-150)/32/2)*32-16（MShare.pas:4620，自格带 y∈[288,320)）。
  #   · **步长 48x32 只是假设**：本仓源码无法证实（MirClient 是外部冻结仓，
  #     CXYfromMouseXY 的逆映射不可见），也没有 oracle 侧等价物。
  #   · 已知失败证据（2026-10-19 缩窗 W0-1 第二轮，docs/window-shrink-run-20261019.md §F2）：
  #     Rust 侧同一配方 `0,-1`×2、`0,1`×1 **全部落成 CM_WALK**（角色走位），
  #     同一点改成按截图里怪的像素位置点（tools/client/mir_click.ps1）后**立刻出 CM_HIT**。
  #     说明「相邻格格心 = 锚点 + 48/32」在这套部署上不成立（具体逆映射待标定）。
  #
  # 标定模式（只有显式给 -AttackTurnCalibrate 才走；与 -AttackTurn/-AttackClicks 互斥）：
  #   -AttackTurnCalibrate "0,-1;0,1;1,0;-1,0;1,-1;-1,-1;1,1;-1,1"
  #   条目语法（三种，可混用）：
  #     dx,dy              ⇒ 右键/左键都用旧投影 (512+dx*48, 304+dy*32)（基线探针）
  #     dx,dy@x,y          ⇒ 右键/左键都用显式像素 (x,y)（扫不同步长/锚点用）
  #     dx,dy@rx,ry>x,y    ⇒ 右键 (rx,ry) 测 CM_TURN、左键 (x,y) 测 CM_HIT（分别标定）
  #   每个探针：右键 → 等 -CalibrateTurnGapMs → 左键 → 等 -CalibrateSettleMs，
  #   记录 [t_start_ms,t_end_ms]；随后把该窗口内 c2s 的
  #     CM_TURN(3010) / CM_HIT(3014) / CM_WALK(3011) / CM_RUN(3013)
  #   归到探针（需 -CalibrateCapture 指向 capture.jsonl；字段口径：
  #   Tag = 方向（0=Up..7=UpLeft），Recog = MakeLong(x,y)）。
  #   一次跑完全部探针，导出 <CalibrateOut>.json + 同名 .csv（格式见写表函数注释）。
  #   窗口判定成功的口径：某格偏移至少一个探针 verdict=turn+hit（CM_TURN 方向与请求一致
  #   且窗口内有 CM_HIT）；只有 turn_only 说明转向对了但左键没打到怪；walk 复现 F2 失败。
  #   capture.jsonl 通常要会话结束后用 tools/capture/GoldenExport 导出；届时用同一条
  #   -AttackTurnCalibrate 配方 + -AttackTurnDryRun + -CalibrateCapture 做离线回填（不启动客户端）。
  [string]$AttackTurnCalibrate = "",
  # 标定后的新映射（**只有显式给本参数才启用**；旧 -AttackTurn 路径不受任何影响）：
  #   -AttackTurn "dx,dy" -AttackTurnMap <attackturn-calibration.json>
  #   取表里 resolved["dx,dy"] 的 right_x/right_y 点右键、left_x/left_y 点左键；
  #   该偏移没有标定结果时 **fail-closed 抛错**（不静默退回旧投影）。
  #   resolved 只收「CM_TURN 方向与请求一致」的探针（turn+hit > turn_only 优先级）。
  [string]$AttackTurnMap = "",
  [string]$CalibrateOut = "",        # 默认 <OutDir>/attackturn-calibration.json（CSV 同名）
  [string]$CalibrateCapture = "",    # capture.jsonl（GoldenExport 产物）；给定则回填 observed_*
  [int]$CalibrateTurnGapMs = 400,    # 右键→左键间隔（与旧 -AttackTurn 的 400ms 节拍一致）
  [int]$CalibrateSettleMs = 700,     # 左键后观察窗尾（等客户端发包/落盘）
  [switch]$AttackTurnDryRun,         # 离线：不启动客户端，只做参数解析/标定表生成或回填
  [int]$CenterX = 512,       # 角色屏幕中心 X（攻击环 / 跑动拖拽基准；默认 512 = 现状，不破坏既有调用）
  [int]$CenterY = 384,       # 角色屏幕中心 Y（真机实测本部署角色屏幕中心约 y=259——底部 UI 遮挡；
                             #   命中环须配 -CenterY 259，否则整环偏下约 2 格，片67 受击步判读受此影响）
  # ---- 会话拆除（「小退」阶段的一种真实形态）----
  # 客户端自己的小退要走 AppLogout 的确认框（Alt+X → 确定 → CM_SOFTCLOSE 1009），
  # 合成按键打不穿那个模态框（实测 keybd_event / SendInput 都不行）；
  # 这里改用**同账号再登录**：本账号在别处登录会触发 LoginSrv 顶号 → 会话被踢 →
  # 客户端收到 SM_OUTOFCONNECTION(528)。前提：账号名 = <BotPrefix><序号>，
  # 这样 BotSrv（LoginId = 前缀+序号）才能用同一个账号登进来。
  [string]$KickVia = "",     # BotSrv 前缀（账号名必须是 <前缀>0，即本客户端用的账号）
  [int]$KickWaitSec = 30,
  [string]$BotRunner = "",    # tools/botload/run_bots.ps1 路径（默认自动定位）
  # ---- 说话阶段（M2 验收② `m2-03-talk`）：进图后开聊天框 → 打字 → 回车发送 → 截图 ----
  [switch]$SkipTalk,
  [string]$TalkText = "m2 talk ok",  # 发送的文本（ASCII，避开客户端 GBK 输入法歧义）
  [string]$TalkBox  = "",            # 可选：聊天输入框坐标 "x,y"；留空则只用回车开框（避免误点地面走路）
  [string]$TalkShot = "m2-03-talk.png",  # 直接落 M2 验收② 的规定名
  [int]$TalkWaitMs  = 1500,           # 回车发送后等服务端 `SM_HEAR` 回声 + 客户端渲染
  # ---- 看见别人（M2 验收② `m2-04-see-others`）----
  # >0 时在「说话」之后等这么多秒（让同屏的第二角色/假人走进 AOI，触发服务端
  # `SM_TURN(10)`+`SM_FEATURECHANGED(41)` 下行）再截一张规定名截图。
  # ⚠ 调用方负责先把第二角色放进同图：假人（`src/BotSrv`）的库坐标须落在观察者 AOI 内。
  # **别用 `tools/botload/run_bots.ps1` 常驻**——它判据达成后会杀 BotSrv（见
  # tools/m2-acceptance/README.md §2.1）；直接跑 `src/BotSrv/bin/Release/BotSrv.exe`。
  [int]$SeeOthersWaitSec = 0,
  [string]$SeeOthersShot = "m2-04-see-others.png",
  # ---- 夹具加固（卡 fixture-harden；tier=L）：公告轮询 / 坐标门 / NPC 点击 ----
  # 根因见 docs/npc-render-diagnosis-20261022.md §2.3：旧 notice_poll 在固定点 (522,525) 盲点 12 次，
  # 公告只吃第一次，其余 11 次落在世界区（每次都重设 g_nTargetX/Y）⇒ 角色从 (29,33) 自动走到 (29,44)
  # 并落库，国王 (29,32) 出视窗。修法：公告只点一次、点完把鼠标移出世界区、任何世界区点击前先过坐标门。
  [int]$NoticePollClicks = 1,        # 公告「确定」按钮点击次数：**上限 1**（>1 直接 fail-closed 拒绝）
  [string]$NoticeButton = "522,525",  # 公告确定按钮（客户端坐标；沿用旧脚本实测值）
  [string]$NoticeParkPoint = "522,760",  # 点完公告后鼠标停这里（底部面板区，不是世界区；只移动不点击）
  [int]$NoticeWaitSec = 4,           # 等公告出现：有 -CapturePath 时按 s2c SM_SENDNOTICE(658) 判定，否则固定等待
  [int]$NoticeVerifySec = 6,         # 等 c2s CM_LOGINNOTICEOK(1018)（仅 -CapturePath 模式校验）
  [string]$CapturePath = "",         # 可选 capture.jsonl（mir2_proxy/GoldenExport 产物）：公告判定 + 1018 校验 + 坐标门取证
  [string]$ExpectPos = "",           # "X,Y"：进入世界后、任何世界区点击前断言角色格；不一致 ⇒ POSITION_GATE RED + 中止
  [switch]$ClickNpc,                 # 进世界后点一次固定 NPC（坐标 -NpcClick）；必须先过坐标门（需要 -CapturePath）
  [string]$NpcClick = "512,263",     # 角色 (29,33) 时该点对应 NPC(29,32)；旧值 (512,227) 实际对应 y=31（两格北，空地）
  [switch]$FixtureDryRun,            # 离线自检：不启动客户端；-NoticePollSim / -ExpectPos 两条路径（供 mir_flow.selftest.ps1）
  [string]$NoticePollSim = ""        # dismiss1 | slow | absent：模拟公告轮询控制流（与 -FixtureDryRun 联用）
)

$ErrorActionPreference = "Stop"
if ($StageLog -eq "") { $StageLog = Join-Path $OutDir "stages.ndjson" }

# ==== AttackTurn 标定/新映射：纯函数（离线可测；不触碰旧攻击路径）================
# 这些函数只被 -AttackTurnCalibrate / -AttackTurnMap / -AttackTurnDryRun 使用。
# 旧 -AttackTurn（未给 -AttackTurnMap）仍走攻击块里的字面量 512+dx*48 / 304+dy*32。

$script:TurnDirNames = @("Up", "UpRight", "Right", "DownRight", "Down", "DownLeft", "Left", "UpLeft")

function Test-TurnIntLiteral([string]$s) {
  return ($s -match '^[+-]?[0-9]+$')
}

function Get-TurnNowMs {
  return [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
}

function Get-TurnCellDirection([int]$dx, [int]$dy) {
  # Mir2 方向：0=Up 1=UpRight 2=Right 3=DownRight 4=Down 5=DownLeft 6=Left 7=UpLeft
  # （src/OpenMir2/Enums/Direction.cs；格坐标 +y = 屏幕向下 = Down）
  $ix = 0
  $iy = 0
  if ($dx -gt 0) { $ix = 1 } elseif ($dx -lt 0) { $ix = -1 }
  if ($dy -gt 0) { $iy = 1 } elseif ($dy -lt 0) { $iy = -1 }
  if ($ix -eq 0 -and $iy -eq -1) { return 0 }
  if ($ix -eq 1 -and $iy -eq -1) { return 1 }
  if ($ix -eq 1 -and $iy -eq 0) { return 2 }
  if ($ix -eq 1 -and $iy -eq 1) { return 3 }
  if ($ix -eq 0 -and $iy -eq 1) { return 4 }
  if ($ix -eq -1 -and $iy -eq 1) { return 5 }
  if ($ix -eq -1 -and $iy -eq 0) { return 6 }
  if ($ix -eq -1 -and $iy -eq -1) { return 7 }
  throw "无法从格偏移 ($dx,$dy) 得到方向"
}

function Get-TurnDirName($dir) {
  if ($null -eq $dir) { return "" }
  $i = [int]$dir
  if ($i -ge 0 -and $i -lt $script:TurnDirNames.Count) { return $script:TurnDirNames[$i] }
  return "?"
}

function Get-LegacyTurnPixel([int]$dx, [int]$dy) {
  # 旧投影：格心锚 (512,304) + 每格 (48,32)。仅用于标定基线/干跑输出；
  # 主攻击路径在未启用 -AttackTurnMap 时仍保留原来的字面量表达式。
  return [pscustomobject]@{ X = 512 + $dx * 48; Y = 304 + $dy * 32 }
}

function ConvertFrom-TurnCalibrateSpec([string]$spec) {
  # 条目语法：dx,dy | dx,dy@x,y | dx,dy@rx,ry>x,y（详见 param 注释）
  $rows = New-Object System.Collections.ArrayList
  $i = 0
  foreach ($raw in $spec.Split(";")) {
    $s = $raw.Trim()
    if ($s -eq "") { continue }
    $i++
    $cellPart = $s
    $rightPart = ""
    $leftPart = ""
    $at = $s.IndexOf("@")
    if ($at -ge 0) {
      $cellPart = $s.Substring(0, $at).Trim()
      $pix = $s.Substring($at + 1).Trim()
      $gt = $pix.IndexOf(">")
      if ($gt -ge 0) {
        $rightPart = $pix.Substring(0, $gt).Trim()
        $leftPart = $pix.Substring($gt + 1).Trim()
      } else {
        $rightPart = $pix
        $leftPart = $pix
      }
    }
    $c = $cellPart.Split(",")
    if ($c.Count -ne 2 -or -not (Test-TurnIntLiteral $c[0].Trim()) -or -not (Test-TurnIntLiteral $c[1].Trim())) {
      throw "标定条目 '$raw' 的格偏移须为 'dx,dy'（两个整数），当前：'$cellPart'"
    }
    $dx = [int]$c[0]; $dy = [int]$c[1]
    if ([Math]::Abs($dx) -gt 1 -or [Math]::Abs($dy) -gt 1) {
      throw "标定条目 '$raw' 须为相邻格（|dx|,|dy|<=1）"
    }
    if ($dx -eq 0 -and $dy -eq 0) {
      throw "标定条目 '$raw' 不能是 (0,0)（自格 ⇒ 无方向）"
    }
    $lp = Get-LegacyTurnPixel $dx $dy
    $rx = $lp.X; $ry = $lp.Y
    if ($rightPart -ne "") {
      $r = $rightPart.Split(",")
      if ($r.Count -ne 2 -or -not (Test-TurnIntLiteral $r[0].Trim()) -or -not (Test-TurnIntLiteral $r[1].Trim())) {
        throw "标定条目 '$raw' 的右键像素须为 'x,y'（两个整数）"
      }
      $rx = [int]$r[0]; $ry = [int]$r[1]
    }
    $lx = $rx; $ly = $ry
    if ($leftPart -ne "") {
      $l = $leftPart.Split(",")
      if ($l.Count -ne 2 -or -not (Test-TurnIntLiteral $l[0].Trim()) -or -not (Test-TurnIntLiteral $l[1].Trim())) {
        throw "标定条目 '$raw' 的左键像素须为 'x,y'（两个整数）"
      }
      $lx = [int]$l[0]; $ly = [int]$l[1]
    }
    [void]$rows.Add([pscustomobject][ordered]@{
      I = $i; CellDx = $dx; CellDy = $dy
      ExpectedDir = (Get-TurnCellDirection $dx $dy)
      RightX = $rx; RightY = $ry; LeftX = $lx; LeftY = $ly
    })
  }
  if ($rows.Count -eq 0) {
    throw "-AttackTurnCalibrate 没有有效条目（示例：""0,-1;0,1;1,0;-1,0""）"
  }
  return ,$rows.ToArray()
}

function New-TurnObserved {
  return [pscustomobject][ordered]@{
    analyzed = $false
    cm_turn = 0; turn_dirs = @(); turn_dir = $null; turn_x = $null; turn_y = $null
    cm_hit = 0; hit_dirs = @(); hit_dir = $null; hit_x = $null; hit_y = $null
    cm_walk = 0; cm_run = 0
    turn_matches = $false; hit_matches = $false
  }
}

function New-TurnCalibrationSummary([object[]]$probes) {
  $s = [ordered]@{
    probes = 0; turn_hit = 0; turn_only = 0; turn_mismatch = 0
    walk = 0; none = 0; unknown = 0; resolved = 0
  }
  foreach ($p in $probes) {
    if ($null -eq $p) { continue }
    $s.probes++
    switch ([string]$p.verdict) {
      "turn+hit" { $s.turn_hit++ }
      "turn_only" { $s.turn_only++ }
      "turn_mismatch" { $s.turn_mismatch++ }
      "walk" { $s.walk++ }
      "none" { $s.none++ }
      default { $s.unknown++ }
    }
  }
  return [pscustomobject]$s
}

function New-TurnCalibrationLedger([object[]]$probes, [string]$mode) {
  $rows = New-Object System.Collections.ArrayList
  foreach ($p in $probes) {
    [void]$rows.Add([pscustomobject][ordered]@{
      probe = [int]$p.I
      cell_dx = [int]$p.CellDx
      cell_dy = [int]$p.CellDy
      expected_dir = [int]$p.ExpectedDir
      expected_dir_name = (Get-TurnDirName $p.ExpectedDir)
      right_x = [int]$p.RightX
      right_y = [int]$p.RightY
      left_x = [int]$p.LeftX
      left_y = [int]$p.LeftY
      t_start_ms = $null
      t_end_ms = $null
      observed = (New-TurnObserved)
      verdict = "unknown"
    })
  }
  $arr = $rows.ToArray()
  return [pscustomobject][ordered]@{
    schema = "openmir2.attackturn-calibration/1"
    created_utc = [DateTimeOffset]::UtcNow.ToString("o")
    mode = $mode
    projection = [pscustomobject][ordered]@{
      anchor_x = 512; anchor_y = 304; step_x = 48; step_y = 32
      source = "anchor from MShare.pas:4620 / SCREENWIDTH div 2; step 48x32 was an assumption (unconfirmed inverse CXYfromMouseXY)"
    }
    capture = $CalibrateCapture
    probes = @($arr)
    resolved = [pscustomobject][ordered]@{}
    summary = (New-TurnCalibrationSummary @($arr))
  }
}

function Get-TurnLoWord([long]$v) {
  $lo = [long]($v -band 0xFFFF)
  if ($lo -ge 0x8000) { $lo -= 0x10000 }
  return [int]$lo
}

function Get-TurnHiWord([long]$v) {
  $hi = [long](($v -shr 16) -band 0xFFFF)
  if ($hi -ge 0x8000) { $hi -= 0x10000 }
  return [int]$hi
}

function Read-TurnCaptureFrames([string]$path, [long]$minTs, [long]$maxTs) {
  # 只解析 ts 落在总窗口内的 c2s 帧（capture.jsonl 可能整场几十万行）。
  if (-not (Test-Path -LiteralPath $path)) { throw "capture.jsonl 不存在：$path" }
  $full = (Resolve-Path -LiteralPath $path).Path
  $frames = New-Object System.Collections.ArrayList
  foreach ($line in [IO.File]::ReadLines($full)) {
    # capture.jsonl 的序列化器可能是紧凑或带空格的；两种都要认。
    if ($line -notmatch '"dir"\s*:\s*"c2s"') { continue }
    $m = [regex]::Match($line, '"ts_ms"\s*:\s*([0-9]+)')
    if (-not $m.Success) { continue }
    $ts = [long]$m.Groups[1].Value
    if ($ts -lt $minTs -or $ts -gt $maxTs) { continue }
    try { $r = $line | ConvertFrom-Json } catch { continue }
    if ($null -eq $r) { continue }
    if ($null -eq $r.PSObject.Properties["kind"] -or [string]$r.kind -ne "frame") { continue }
    if ($null -eq $r.PSObject.Properties["ident"]) { continue }
    [void]$frames.Add($r)
  }
  return ,$frames.ToArray()
}

# ==== 夹具加固：capture 取证 / 公告轮询 / 坐标门 / 客户端身份 ==================
# 纯读文件 + 纯计算；离线（-FixtureDryRun）与真机流程共用同一套口径。
# 自身坐标证据（capture.jsonl，字段口径同 GoldenExport）：
#   · c2s CM_WALK(3011)：recog = X | (Y<<16)（低字 X、高字 Y）= 该步**终点格**（走完即所在格）
#   · s2c SM_LOGON(50) / SM_NEWMAP(51)：param = X、tag = Y（进图位置）
# 取 ts_ms 最新的一条作为「角色当前格」；一条都没有 ⇒ 不可断言（调用方必须 fail-closed）。

function Get-SelfCellEvidence([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { throw "capture.jsonl 不存在：$path" }
  $full = (Resolve-Path -LiteralPath $path).Path
  $best = $null
  foreach ($line in [IO.File]::ReadLines($full)) {
    if ($line -notmatch '"kind"\s*:\s*"frame"') { continue }
    if ($line -notmatch '"ident"\s*:\s*(3011|50|51)(?!\d)') { continue }
    try { $r = $line | ConvertFrom-Json } catch { continue }
    if ($null -eq $r -or $null -eq $r.PSObject.Properties["ident"] -or $null -eq $r.ident) { continue }
    $ident = [int]$r.ident
    $dir = ""
    if ($null -ne $r.PSObject.Properties["dir"]) { $dir = [string]$r.dir }
    $x = $null; $y = $null; $src = ""
    if ($dir -eq "c2s" -and $ident -eq 3011) {
      if ($null -eq $r.PSObject.Properties["recog"] -or $null -eq $r.recog) { continue }
      $x = Get-TurnLoWord ([long]$r.recog); $y = Get-TurnHiWord ([long]$r.recog); $src = "c2s CM_WALK(3011)"
    } elseif ($dir -eq "s2c" -and ($ident -eq 50 -or $ident -eq 51)) {
      if ($null -eq $r.PSObject.Properties["param"] -or $null -eq $r.param) { continue }
      if ($null -eq $r.PSObject.Properties["tag"] -or $null -eq $r.tag) { continue }
      $x = [int]$r.param; $y = [int]$r.tag
      if ($ident -eq 50) { $src = "s2c SM_LOGON(50)" } else { $src = "s2c SM_NEWMAP(51)" }
    } else { continue }
    $ts = [long]0
    if ($null -ne $r.PSObject.Properties["ts_ms"] -and $null -ne $r.ts_ms) { $ts = [long]$r.ts_ms }
    if ($null -eq $best -or $ts -ge [long]$best.ts_ms) {
      $best = [pscustomobject][ordered]@{ X = [int]$x; Y = [int]$y; ts_ms = $ts; source = $src; seq = $r.seq }
    }
  }
  return $best
}

function Test-PositionGate([string]$path, [string]$expect) {
  # "X,Y" ⇒ 断言角色格。不一致 / 无证据 ⇒ Ok=$false（fail-closed，调用方必须中止）。
  $parts = $expect.Split(",")
  if ($parts.Count -ne 2 -or -not (Test-TurnIntLiteral $parts[0].Trim()) -or -not (Test-TurnIntLiteral $parts[1].Trim())) {
    return [pscustomobject]@{ Ok = $false; Detail = ("-ExpectPos 须为 'X,Y'（两个整数），当前：'" + $expect + "'") }
  }
  $ex = [int]$parts[0]; $ey = [int]$parts[1]
  $ev = Get-SelfCellEvidence $path
  if ($null -eq $ev) {
    return [pscustomobject]@{ Ok = $false; Detail = ("capture 中没有自身坐标证据（c2s CM_WALK(3011) / s2c SM_LOGON(50)/SM_NEWMAP(51)）；expect=" + $ex + "," + $ey) }
  }
  $ok = ([int]$ev.X -eq $ex -and [int]$ev.Y -eq $ey)
  $detail = ("actual=" + $ev.X + "," + $ev.Y + " expect=" + $ex + "," + $ey +
             " source='" + $ev.source + "' seq=" + $ev.seq + " ts_ms=" + $ev.ts_ms)
  return [pscustomobject]@{ Ok = $ok; ActualX = [int]$ev.X; ActualY = [int]$ev.Y; ExpectX = $ex; ExpectY = $ey; Detail = $detail }
}

function Wait-CaptureFrame([string]$path, [string]$dir, [int]$ident, [int]$timeoutSec) {
  # 轮询（capture 可能仍在增长）直到出现 dir/ident 的帧；超时返回 $null。先用正则预筛，避免整场 JSON 解析。
  $deadline = (Get-Date).AddSeconds([Math]::Max(0, $timeoutSec))
  $dirPat = '"dir"\s*:\s*"' + [regex]::Escape($dir) + '"'
  $identPat = '"ident"\s*:\s*' + $ident + '(?!\d)'
  while ($true) {
    if (Test-Path -LiteralPath $path) {
      $full = (Resolve-Path -LiteralPath $path).Path
      foreach ($line in [IO.File]::ReadLines($full)) {
        if ($line -notmatch $dirPat) { continue }
        if ($line -notmatch $identPat) { continue }
        try { return ($line | ConvertFrom-Json) } catch { continue }
      }
    }
    if ((Get-Date) -ge $deadline) { return $null }
    Start-Sleep -Milliseconds 250
  }
}

function Invoke-NoticePollSim([string]$mode, [int]$maxClicks) {
  # 离线模拟 notice_poll 的控制流（不碰客户端）：返回 Clicks / WorldClicks / Parked / Dismissed / Reason。
  #   dismiss1：公告可见 ⇒ 点 1 次即消失（正常路径）
  #   slow    ：公告要 3 次才消失 ⇒ 上限 1 只允许点 1 次，**不补点**（证明点击次数上限生效）
  #   absent  ：公告始终不出现 ⇒ 一次也不点世界区（超时后只把鼠标移出世界区）
  if ($mode -eq "dismiss1") {
    $c = 0
    if ($maxClicks -ge 1) { $c++ }
    return [pscustomobject][ordered]@{ Clicks = $c; WorldClicks = 0; Parked = $true; Dismissed = ($c -ge 1); Reason = "notice_clicked_once" }
  }
  if ($mode -eq "slow") {
    $c = 0
    if ($maxClicks -ge 1) { $c++ }
    return [pscustomobject][ordered]@{ Clicks = $c; WorldClicks = 0; Parked = $true; Dismissed = $false; Reason = "click_cap_reached" }
  }
  if ($mode -eq "absent") {
    return [pscustomobject][ordered]@{ Clicks = 0; WorldClicks = 0; Parked = $true; Dismissed = $false; Reason = "no_notice_timeout" }
  }
  throw ("-NoticePollSim 只支持 dismiss1 | slow | absent，当前：'" + $mode + "'")
}

function Get-Sha256Hex([string]$path) {
  # 不用 Get-FileHash：本机 Windows PowerShell 5.1 的 Microsoft.PowerShell.Utility 里没有该 cmdlet
  # （实测 CommandNotFoundException），直接用 .NET 实现，避免取证静默退化成 null。
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    $fs = [IO.File]::OpenRead($path)
    try { $bytes = $sha.ComputeHash($fs) } finally { $fs.Dispose() }
  } finally { $sha.Dispose() }
  return ([BitConverter]::ToString($bytes) -replace "-", "").ToLowerInvariant()
}

function Get-ClientProvenance([string]$exePath) {
  # 客户端身份取证（本轮误判的直接原因：manifest 只记了 proxy/端口，没记客户端二进制）。
  if ($exePath -eq "" -or -not (Test-Path -LiteralPath $exePath)) {
    return [pscustomobject][ordered]@{ path = $null; sha256 = $null; mtime = $null; size = $null }
  }
  $fi = Get-Item -LiteralPath $exePath
  return [pscustomobject][ordered]@{
    path = $fi.FullName
    sha256 = (Get-Sha256Hex $fi.FullName)
    mtime = $fi.LastWriteTimeUtc.ToString("o")
    size = [long]$fi.Length
  }
}

function Get-TurnCalibrationMapFromLedger($ledger) {
  # 每个格偏移挑一个最可信的探针：turn+hit 且 hit 方向也对 > turn+hit > turn_only。
  $groups = @{}
  foreach ($p in @($ledger.probes)) {
    if ($null -eq $p) { continue }
    if ($null -eq $p.observed) { continue }
    if (-not [bool]$p.observed.turn_matches) { continue }
    $v = [string]$p.verdict
    if ($v -ne "turn+hit" -and $v -ne "turn_only") { continue }
    $key = ([string]$p.cell_dx) + "," + ([string]$p.cell_dy)
    if (-not $groups.ContainsKey($key)) { $groups[$key] = New-Object System.Collections.ArrayList }
    [void]$groups[$key].Add($p)
  }
  $resolved = [ordered]@{}
  foreach ($key in @($groups.Keys | Sort-Object)) {
    $best = $null
    $rank = 0
    foreach ($p in $groups[$key]) {
      $r = 1
      if ([string]$p.verdict -eq "turn+hit") { $r = 2 }
      if ([string]$p.verdict -eq "turn+hit" -and [bool]$p.observed.hit_matches) { $r = 3 }
      if ($r -gt $rank) { $rank = $r; $best = $p }
    }
    if ($null -ne $best) {
      $conf = "turn_only"
      if ($rank -eq 3) { $conf = "turn+hit" } elseif ($rank -eq 2) { $conf = "turn+hit_dir_mismatch" }
      $resolved[$key] = [pscustomobject][ordered]@{
        right_x = [int]$best.right_x
        right_y = [int]$best.right_y
        left_x = [int]$best.left_x
        left_y = [int]$best.left_y
        from_probe = [int]$best.probe
        confidence = $conf
        expected_dir = [int]$best.expected_dir
      }
    }
  }
  return [pscustomobject]$resolved
}

function Update-TurnCalibrationLedger($ledger, [string]$capturePath) {
  # 把 capture.jsonl 的 c2s 帧按探针 [t_start_ms,t_end_ms] 窗口归属，回填 observed/verdict/resolved。
  # 时钟域：capture 的 ts_ms 与 -AttackTurnCalibrate 记录的 UTC epoch ms 同机同时钟
  # （proxy/hook 的 t_wall_ms = 本机 wall clock），窗口比较不需要额外换算。
  $minTs = [long]::MaxValue
  $maxTs = [long]::MinValue
  foreach ($p in @($ledger.probes)) {
    if ($null -eq $p) { continue }
    if ($null -eq $p.t_start_ms -or $null -eq $p.t_end_ms) { continue }
    if ([long]$p.t_start_ms -lt $minTs) { $minTs = [long]$p.t_start_ms }
    if ([long]$p.t_end_ms -gt $maxTs) { $maxTs = [long]$p.t_end_ms }
  }
  $frames = @()
  if ($minTs -le $maxTs) {
    $frames = Read-TurnCaptureFrames $capturePath $minTs $maxTs
  }
  foreach ($p in @($ledger.probes)) {
    if ($null -eq $p) { continue }
    $obs = New-TurnObserved
    $obs.analyzed = $true
    if ($null -eq $p.t_start_ms -or $null -eq $p.t_end_ms) {
      # 表里没有探针窗口（例如干跑生成的计划表）⇒ 无法归属，保持 unknown。
      $obs.analyzed = $false
      $p.observed = $obs
      $p.verdict = "unknown"
      continue
    }
    $t0 = [long]$p.t_start_ms
    $t1 = [long]$p.t_end_ms
    $turnDirs = New-Object System.Collections.ArrayList
    $hitDirs = New-Object System.Collections.ArrayList
    foreach ($f in $frames) {
      $ts = [long]$f.ts_ms
      if ($ts -lt $t0 -or $ts -gt $t1) { continue }
      $ident = [int]$f.ident
      $tag = 0
      if ($null -ne $f.PSObject.Properties["tag"] -and $null -ne $f.tag) { $tag = [int]$f.tag }
      $recog = $null
      if ($null -ne $f.PSObject.Properties["recog"] -and $null -ne $f.recog) { $recog = [long]$f.recog }
      switch ($ident) {
        3010 {
          $obs.cm_turn++
          [void]$turnDirs.Add($tag)
          $obs.turn_dir = $tag
          if ($null -ne $recog) { $obs.turn_x = Get-TurnLoWord $recog; $obs.turn_y = Get-TurnHiWord $recog }
        }
        3014 {
          $obs.cm_hit++
          [void]$hitDirs.Add($tag)
          $obs.hit_dir = $tag
          if ($null -ne $recog) { $obs.hit_x = Get-TurnLoWord $recog; $obs.hit_y = Get-TurnHiWord $recog }
        }
        3011 { $obs.cm_walk++ }
        3013 { $obs.cm_run++ }
      }
    }
    $obs.turn_dirs = @($turnDirs.ToArray())
    $obs.hit_dirs = @($hitDirs.ToArray())
    foreach ($d in $turnDirs) { if ([int]$d -eq [int]$p.expected_dir) { $obs.turn_matches = $true } }
    foreach ($d in $hitDirs) { if ([int]$d -eq [int]$p.expected_dir) { $obs.hit_matches = $true } }
    if ($obs.cm_turn -eq 0 -and $obs.cm_hit -eq 0 -and $obs.cm_walk -eq 0 -and $obs.cm_run -eq 0) {
      $p.verdict = "none"
    } elseif ($obs.turn_matches -and $obs.cm_hit -gt 0) {
      $p.verdict = "turn+hit"
    } elseif ($obs.turn_matches) {
      $p.verdict = "turn_only"
    } elseif ($obs.cm_turn -gt 0) {
      $p.verdict = "turn_mismatch"
    } elseif ($obs.cm_walk -gt 0 -or $obs.cm_run -gt 0) {
      $p.verdict = "walk"
    } else {
      $p.verdict = "none"
    }
    $p.observed = $obs
  }
  $ledger.capture = (Resolve-Path -LiteralPath $capturePath).Path
  $ledger.resolved = (Get-TurnCalibrationMapFromLedger $ledger)
  $ledger.summary = (New-TurnCalibrationSummary @($ledger.probes))
  $ledger.summary.resolved = @($ledger.resolved.PSObject.Properties).Count
  return $ledger
}

function Read-TurnCalibrationMap([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { throw "-AttackTurnMap 文件不存在：$path" }
  $raw = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $path).Path)
  $m = $raw | ConvertFrom-Json
  if ($null -eq $m.PSObject.Properties["schema"] -or [string]$m.schema -ne "openmir2.attackturn-calibration/1") {
    throw "-AttackTurnMap 不是 openmir2.attackturn-calibration/1 标定表：$path"
  }
  if ($null -eq $m.PSObject.Properties["resolved"] -or $null -eq $m.resolved) {
    throw "-AttackTurnMap 标定表缺少 resolved 段：$path"
  }
  $table = @{}
  foreach ($prop in $m.resolved.PSObject.Properties) { $table[$prop.Name] = $prop.Value }
  if ($table.Count -eq 0) {
    throw ("-AttackTurnMap 的 resolved 为空：没有探针达成 turn+hit/turn_only。" +
      "先在窗口跑 -AttackTurnCalibrate，并用 -CalibrateCapture 回填 capture.jsonl。")
  }
  return $table
}

function Resolve-TurnMapPixel($map, [int]$dx, [int]$dy) {
  $key = ([string]$dx) + "," + ([string]$dy)
  if (-not $map.ContainsKey($key)) {
    $keys = @($map.Keys | Sort-Object) -join "; "
    throw "-AttackTurnMap 没有格偏移 ($key) 的标定结果；可用格偏移：$keys"
  }
  $v = $map[$key]
  return [pscustomobject]@{
    RightX = [int]$v.right_x; RightY = [int]$v.right_y
    LeftX = [int]$v.left_x; LeftY = [int]$v.left_y
  }
}

function ConvertTo-TurnCsvCell($v) {
  if ($null -eq $v) { return "" }
  if ($v -is [bool]) { if ($v) { return "true" } else { return "false" } }
  $s = [string]$v
  if ($s.IndexOf('"') -ge 0) { $s = $s.Replace('"', '""') }
  if ($s.IndexOf(',') -ge 0 -or $s.IndexOf('"') -ge 0 -or $s.IndexOf("`n") -ge 0 -or $s.IndexOf("`r") -ge 0) {
    return '"' + $s + '"'
  }
  return $s
}

function Write-TurnCalibrationTable($ledger, [string]$path) {
  # 标定表：JSON（机器读，含 resolved 段）+ 同名 CSV（26 列，便于窗口里人眼/表格对照）。
  # CSV 列：probe,cell_dx,cell_dy,expected_dir,expected_dir_name,right_x,right_y,
  #         left_x,left_y,t_start_ms,t_end_ms,cm_turn,turn_dirs,turn_dir,turn_x,turn_y,
  #         cm_hit,hit_dirs,hit_dir,hit_x,hit_y,cm_walk,cm_run,turn_matches,hit_matches,verdict
  $dir = Split-Path -Parent $path
  if ($dir -ne "" -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  $enc = New-Object System.Text.UTF8Encoding($false)
  $json = $ledger | ConvertTo-Json -Depth 12
  [IO.File]::WriteAllText($path, $json + "`r`n", $enc)
  $csvPath = [IO.Path]::ChangeExtension($path, ".csv")
  $cols = @("probe", "cell_dx", "cell_dy", "expected_dir", "expected_dir_name",
            "right_x", "right_y", "left_x", "left_y", "t_start_ms", "t_end_ms",
            "cm_turn", "turn_dirs", "turn_dir", "turn_x", "turn_y",
            "cm_hit", "hit_dirs", "hit_dir", "hit_x", "hit_y",
            "cm_walk", "cm_run", "turn_matches", "hit_matches", "verdict")
  $sb = New-Object System.Text.StringBuilder
  [void]$sb.Append(($cols -join ",") + "`r`n")
  foreach ($p in @($ledger.probes)) {
    if ($null -eq $p) { continue }
    $o = $p.observed
    $vals = @(
      $p.probe, $p.cell_dx, $p.cell_dy, $p.expected_dir, $p.expected_dir_name,
      $p.right_x, $p.right_y, $p.left_x, $p.left_y, $p.t_start_ms, $p.t_end_ms,
      $o.cm_turn, (@($o.turn_dirs) -join "/"), $o.turn_dir, $o.turn_x, $o.turn_y,
      $o.cm_hit, (@($o.hit_dirs) -join "/"), $o.hit_dir, $o.hit_x, $o.hit_y,
      $o.cm_walk, $o.cm_run, $o.turn_matches, $o.hit_matches, $p.verdict
    )
    $cells = @()
    foreach ($v in $vals) { $cells += (ConvertTo-TurnCsvCell $v) }
    [void]$sb.Append(($cells -join ",") + "`r`n")
  }
  [IO.File]::WriteAllText($csvPath, $sb.ToString(), $enc)
  return $csvPath
}

function Write-TurnCalibrationSummary($ledger, [string]$jsonPath, [string]$csvPath) {
  $s = $ledger.summary
  Write-Output ("CALIB_TABLE " + $jsonPath)
  Write-Output ("CALIB_CSV " + $csvPath)
  Write-Output ("CALIB_SUMMARY probes=" + $s.probes + " turn_hit=" + $s.turn_hit +
    " turn_only=" + $s.turn_only + " turn_mismatch=" + $s.turn_mismatch +
    " walk=" + $s.walk + " none=" + $s.none + " unknown=" + $s.unknown +
    " resolved=" + $s.resolved)
  foreach ($p in @($ledger.probes)) {
    if ($null -eq $p) { continue }
    $o = $p.observed
    Write-Output ("CALIB_PROBE i=" + $p.probe + " cell=" + $p.cell_dx + "," + $p.cell_dy +
      " expected_dir=" + $p.expected_dir + "(" + $p.expected_dir_name + ")" +
      " right=" + $p.right_x + "," + $p.right_y + " left=" + $p.left_x + "," + $p.left_y +
      " cm_turn=" + $o.cm_turn + " turn_dir=" + (Get-TurnDirName $o.turn_dir) +
      " cm_hit=" + $o.cm_hit + " hit_dir=" + (Get-TurnDirName $o.hit_dir) +
      " cm_walk=" + $o.cm_walk + " verdict=" + $p.verdict)
  }
  if ($null -ne $ledger.resolved) {
    foreach ($prop in $ledger.resolved.PSObject.Properties) {
      $v = $prop.Value
      Write-Output ("CALIB_RESOLVED cell=" + $prop.Name + " right=" + $v.right_x + "," + $v.right_y +
        " left=" + $v.left_x + "," + $v.left_y + " from_probe=" + $v.from_probe +
        " confidence=" + $v.confidence)
    }
  }
}

function Read-TurnCalibrationLedger([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { throw "标定表不存在：$path" }
  $ledger = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $path).Path) | ConvertFrom-Json
  if ($null -eq $ledger.PSObject.Properties["schema"] -or [string]$ledger.schema -ne "openmir2.attackturn-calibration/1") {
    throw "不是 openmir2.attackturn-calibration/1 标定表：$path"
  }
  return $ledger
}

function Assert-TurnAttackTurn([string]$value) {
  # 与旧攻击块同一套校验（只用于干跑/参数解析；旧路径的校验原样保留）。
  $td = $value.Split(",") | ForEach-Object { [int]$_ }
  if ($td.Count -ne 2) { throw "-AttackTurn 须为 'dx,dy'（两个整数格偏移），当前：$value" }
  if ([Math]::Abs($td[0]) -gt 1 -or [Math]::Abs($td[1]) -gt 1) {
    throw "-AttackTurn 须为相邻格（|dx|,|dy|<=1），当前：$value"
  }
  if ($td[0] -eq 0 -and $td[1] -eq 0) {
    throw "-AttackTurn 不能为 (0,0)（自格 ⇒ tdir 退化），当前：$value"
  }
  return ,$td
}

if ($CalibrateOut -eq "") { $CalibrateOut = Join-Path $OutDir "attackturn-calibration.json" }
if ($AttackTurnCalibrate -ne "" -and $AttackTurn -ne "") {
  throw "-AttackTurnCalibrate 与 -AttackTurn 互斥（标定探针自己决定每个格偏移，不要混入旧映射配方）"
}
if ($AttackTurnCalibrate -ne "" -and $AttackClicks -gt 0) {
  throw "-AttackTurnCalibrate 与 -AttackClicks 互斥（避免两套驱动互相污染标定窗口）"
}
if ($AttackTurnMap -ne "" -and $AttackTurn -eq "") {
  throw "-AttackTurnMap 须与 -AttackTurn 'dx,dy' 联用（新映射按格偏移查表）"
}
if ($AttackTurnMap -ne "" -and $AttackTurnCalibrate -ne "") {
  throw "-AttackTurnMap 与 -AttackTurnCalibrate 互斥"
}
# ---- 夹具加固参数门（fail-closed；都在启动客户端之前）----
if ($NoticePollClicks -lt 0 -or $NoticePollClicks -gt 1) {
  throw ("-NoticePollClicks 只允许 0 或 1（上限 1）：重复盲点世界区会把角色走掉（docs/npc-render-diagnosis-20261022.md §2.3）")
}
if ($NoticeWaitSec -lt 0) { throw "-NoticeWaitSec 不能为负" }
if ($NoticeVerifySec -lt 0) { throw "-NoticeVerifySec 不能为负" }
if ($ExpectPos -ne "") {
  $ep = $ExpectPos.Split(",")
  if ($ep.Count -ne 2 -or -not (Test-TurnIntLiteral $ep[0].Trim()) -or -not (Test-TurnIntLiteral $ep[1].Trim())) {
    throw ("-ExpectPos 须为 'X,Y'（两个整数），当前：'" + $ExpectPos + "'")
  }
}
if ($ClickNpc -and $CapturePath -eq "") {
  throw "-ClickNpc 需要 -CapturePath：坐标门没有取证就不得盲点世界区（fail-closed）"
}
$turnMapTable = $null
if ($AttackTurnMap -ne "") { $turnMapTable = Read-TurnCalibrationMap $AttackTurnMap }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# ---- 离线干跑：不启动客户端（语法/参数解析/标定表格式自检都靠它）----
if ($AttackTurnDryRun) {
  if ($AttackTurnCalibrate -ne "") {
    $probes = ConvertFrom-TurnCalibrateSpec $AttackTurnCalibrate
    if ($CalibrateCapture -ne "" -and (Test-Path -LiteralPath $CalibrateOut)) {
      # 已有驱动阶段的表（带 t_start_ms/t_end_ms）⇒ 只做离线回填。
      $ledger = Read-TurnCalibrationLedger $CalibrateOut
      $ledger.mode = "dry-run-analyze"
      $ledger = Update-TurnCalibrationLedger $ledger $CalibrateCapture
    } else {
      $ledger = New-TurnCalibrationLedger $probes "dry-run"
      if ($CalibrateCapture -ne "") {
        # 没有探针窗口 ⇒ 无法归属，observed 保持 unknown。
        $ledger = Update-TurnCalibrationLedger $ledger $CalibrateCapture
      }
    }
    $csvPath = Write-TurnCalibrationTable $ledger $CalibrateOut
    Write-TurnCalibrationSummary $ledger $CalibrateOut $csvPath
    Write-Output "DRY_RUN_OK"
    exit 0
  }
  if ($AttackTurn -ne "") {
    $td = Assert-TurnAttackTurn $AttackTurn
    if ($turnMapTable) {
      $px = Resolve-TurnMapPixel $turnMapTable ([int]$td[0]) ([int]$td[1])
      Write-Output ("TURN_CLICK x=" + $px.LeftX + " y=" + $px.LeftY +
        " right_x=" + $px.RightX + " right_y=" + $px.RightY +
        " source=map cell=" + $td[0] + "," + $td[1])
    } else {
      $lp = Get-LegacyTurnPixel ([int]$td[0]) ([int]$td[1])
      Write-Output ("TURN_CLICK x=" + $lp.X + " y=" + $lp.Y +
        " source=legacy cell=" + $td[0] + "," + $td[1])
    }
    Write-Output "DRY_RUN_OK"
    exit 0
  }
  throw "-AttackTurnDryRun 需要 -AttackTurn 或 -AttackTurnCalibrate"
}

# ---- 离线夹具自检：不启动客户端（卡 fixture-harden；mir_flow.selftest.ps1 调用）----
if ($FixtureDryRun) {
  $did = $false
  if ($NoticePollSim -ne "") {
    $sim = Invoke-NoticePollSim $NoticePollSim $NoticePollClicks
    Write-Output ("NOTICE_POLL_SIM mode=" + $NoticePollSim + " clicks=" + $sim.Clicks +
      " world_clicks=" + $sim.WorldClicks + " parked=" + $sim.Parked +
      " dismissed=" + $sim.Dismissed + " reason=" + $sim.Reason +
      " cap=" + $NoticePollClicks)
    $did = $true
  }
  if ($ExpectPos -ne "") {
    if ($CapturePath -eq "") {
      Write-Output "POSITION_GATE RED missing -CapturePath（坐标门取证缺失）"
      throw "-FixtureDryRun -ExpectPos 需要 -CapturePath（坐标门取证）"
    }
    $gate = Test-PositionGate $CapturePath $ExpectPos
    if ($gate.Ok) {
      Write-Output ("POSITION_GATE GREEN " + $gate.Detail)
    } else {
      Write-Output ("POSITION_GATE RED " + $gate.Detail)
      exit 1
    }
    $did = $true
  }
  if (-not $did) {
    throw "-FixtureDryRun 需要 -NoticePollSim 或 -ExpectPos（离线夹具自检）"
  }
  Write-Output "FIXTURE_DRY_RUN_OK"
  exit 0
}

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
# ---- 客户端身份取证（卡 fixture-harden）----
# 本轮误判的直接原因：抓包 manifest 只记了 proxy/端口映射，没记跑的到底是哪台客户端
# （r5 实际是冻结 Delphi MirClinet.exe，报障却写成 MirClientZig）。这里在启动前把
# client_exe / sha256 / mtime / size 落到 <OutDir>/manifest.json，事后可复核。
$clientCandidates = @((Join-Path $RunDir "MirClinet.exe"), (Join-Path $RunDir "MirClientZig.exe"))
$clientExe = $clientCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $clientExe) { $clientExe = "" }
$clientProv = Get-ClientProvenance $clientExe
$clientManifest = [ordered]@{
  schema = "openmir2.mirflow-manifest/1"
  created_utc = [DateTimeOffset]::UtcNow.ToString("o")
  run_dir = $RunDir
  launcher = (Join-Path $RunDir "run-release.cmd")
  client_exe = $clientProv.path
  client_exe_sha256 = $clientProv.sha256
  client_exe_mtime = $clientProv.mtime
  client_exe_size = $clientProv.size
}
[IO.File]::WriteAllText((Join-Path $OutDir "manifest.json"),
  (($clientManifest | ConvertTo-Json -Depth 6) + "`r`n"), (New-Object Text.UTF8Encoding($false)))
if ($null -eq $clientProv.path) {
  Write-Output ("CLIENT_EXE_MISSING 在 " + $RunDir + " 未找到 MirClinet.exe / MirClientZig.exe；manifest 记为 null")
} else {
  Write-Output ("CLIENT_EXE=" + $clientProv.path)
  Write-Output ("CLIENT_SHA256=" + $clientProv.sha256)
  Write-Output ("CLIENT_MTIME=" + $clientProv.mtime)
}
Get-Process MirClinet -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
if ($HookOut -ne "") { $env:MIR2_HOOK_OUT = $HookOut }   # 必须在 Start-Process 之前：子进程继承
Start-Process -FilePath "$RunDir\run-release.cmd" -WorkingDirectory $RunDir
Start-Sleep -Seconds 14
# 客户端可能起得慢（机器忙时 14 秒不够）：轮询等进程出现，最多再等 50 秒
$p = $null
$deadlineW = (Get-Date).AddSeconds(50)
while ((Get-Date) -lt $deadlineW) {
  $p = Get-Process MirClinet -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($p) { break }
  Start-Sleep -Milliseconds 500
}
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

function Send-RightClick([int]$x, [int]$y) {
  # 右键单击（CM_TURN 专用）。右键相邻格是唯一可控/确定性的 CM_TURN 触发点（另有 ProcessActionMessages 被挡转向与挂机随机转向，均不可用于驱动）——CM_TURN 亦见于 _DXDrawMouseDown 的 ssRight 支发出（ClMain.pas:5344 块内
  # :5444 相邻判定→ :5450 GetNextDirection → :5454 SendMsg(CM_TURN, 自格, tdir)）；左键支（:5467 起）
  # 只会 caHit/caWalk——这正是片68「空地攻击不先发 CM_TURN ⇒ 服务端用旧朝向取前方格 ⇒ 必打空」的根因。
  # 消息序列照 Send-Click 的已验证路径：mousemove → WM_RBUTTONDOWN(0x0204, wparam=MK_RBUTTON=2) →
  # WM_RBUTTONUP(0x0205)。wparam=2 ⇒ VCL Shift=[ssRight]（ClMain.pas:5384 有 Shift=[ssRight] 精确比较，
  # 不能混入其它键态）；FormMouseDown 的 120ms 重入门（ClMain.pas:4748）由内部节拍满足。
  [void][MirFlow]::SetCursorPos(($pt.X + $x), ($pt.Y + $y))
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 200
  $lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
  [void][MirFlow]::PostMessage($script:hwnd, 0x0200, [IntPtr]0, $lp)   # mousemove（VCL 位置跟踪）
  Start-Sleep -Milliseconds 90
  [void][MirFlow]::PostMessage($script:hwnd, 0x0204, [IntPtr]2, $lp)   # rbuttondown（MK_RBUTTON）
  Start-Sleep -Milliseconds 90
  [void][MirFlow]::PostMessage($script:hwnd, 0x0205, [IntPtr]0, $lp)   # rbuttonup
  Start-Sleep -Milliseconds 220
}

function Move-Mouse([int]$x, [int]$y) {
  # 只移动鼠标（SetCursorPos + WM_MOUSEMOVE），**不按下任何键**。
  # 用途：点完公告后把指针停到面板区（-NoticeParkPoint），避免后续误点把 g_nTargetX/Y 落到世界格。
  [void][MirFlow]::SetCursorPos(($pt.X + $x), ($pt.Y + $y))
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 120
  $lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
  [void][MirFlow]::PostMessage($script:hwnd, 0x0200, [IntPtr]0, $lp)
  Start-Sleep -Milliseconds 80
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

function Send-HoldRun([int]$x, [int]$y, [int]$ms) {
  # 「跑」探针：必须在**目标点**上按下左键。
  # 冻结客户端的 `caRun` 只在「点 >1 格」时置：`_DXDrawMouseDown`（ClMain.pas:5446-5463）
  # 在 `abs(Curr-mouse)>1` 时置 `g_ChrAction:=caRun` + `g_nTargetX:=mouseX`。而
  # `FormMouseMove`（ClMain.pas:4648）只把移动喂 UI（`g_DWinMan.MouseMove`），**不**调
  # `_DXDrawMouseDown` ⇒ 从中心按下再靠 WM_MOUSEMOVE 挪永远设不上远目标（片⑥ R5..R7 的
  # 0 条 3013 即此故）。这里直接在目标点 PostMessage WM_LBUTTONDOWN（与 Send-Click 同路径、
  # 不依赖窗口是否在前台），并每 150ms 重按一次（`FormMouseDown` 的 120ms 重入门）保持
  # `g_ChrAction:=caRun`，让 `ProcessActionMessages`（ClMain.pas:1494，5ms 节拍）把 caRun 支走完。
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 200
  [void][MirFlow]::SetCursorPos(($pt.X + $x), ($pt.Y + $y))
  $lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
  [void][MirFlow]::PostMessage($script:hwnd, 0x0200, [IntPtr]0, $lp)   # mousemove at target
  Start-Sleep -Milliseconds 120
  $end = (Get-Date).AddMilliseconds($ms)
  while ((Get-Date) -lt $end) {
    [void][MirFlow]::PostMessage($script:hwnd, 0x0201, [IntPtr]1, $lp) # lbdown at target
    Start-Sleep -Milliseconds 150
  }
  [void][MirFlow]::PostMessage($script:hwnd, 0x0202, [IntPtr]0, $lp)   # lbup
  Start-Sleep -Milliseconds 300
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
  # 每次都重取主窗口句柄：客户端切场景/重建渲染设备后会换句柄，
  # 用旧句柄 GetWindowRect 会给无效矩形 → Bitmap 构造抛 "Parameter is not valid"。
  $pf = Get-Process MirClinet -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
  if ($pf) { $script:hwnd = $pf.MainWindowHandle }
  $r = New-Object MirFlow+RECT
  [void][MirFlow]::GetWindowRect($script:hwnd, [ref]$r)
  if (($r.R - $r.L) -le 0 -or ($r.B - $r.T) -le 0) {
    Write-Output ("SHOT_SKIP " + $name + "（窗口矩形无效，客户端可能已退出）")
    return
  }
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
# ---- 公告轮询（卡 fixture-harden 修正）----
# 旧实现：在固定点 (522,525) 盲点 12 次（每 1.2s）。公告只吃**第一次**，其余 11 次落在世界区，
# 每次都把 g_nTargetX/Y 重设到鼠标格 ⇒ 角色自动走位（r5 实测 (29,33)→(29,44)）并落库，
# 国王 (29,32) 出视窗（docs/npc-render-diagnosis-20261022.md §2.3）。
# 新实现：等公告 → **只点一次** → 校验 CM_LOGINNOTICEOK(1018)（仅 -CapturePath）→
#   把鼠标移到面板区（-NoticeParkPoint，只移动不点击）；公告未出现时超时也不点世界区。
# 服务端窗口仍只有 10 秒（PlayObject.RunNotice：>10s ⇒ BoEmergencyClose），
# 所以等待取值要小于它：默认 NoticeWaitSec=4 + NoticeVerifySec=6。
Stage "notice_wait"
$noticeNb = $NoticeButton.Split(",") | ForEach-Object { [int]$_ }
$noticePark = $NoticeParkPoint.Split(",") | ForEach-Object { [int]$_ }
$noticeSeen = $null
if ($CapturePath -ne "") {
  $noticeSeen = Wait-CaptureFrame $CapturePath "s2c" 658 $NoticeWaitSec
} else {
  # 无 capture 时无法判定公告是否已出现：按 oracle「进图即推 SM_SENDNOTICE」的既有事实，
  # 固定等待后单击一次（**绝不循环补点**；补点就是本轮事故的成因）。
  Start-Sleep -Seconds $NoticeWaitSec
}
Shot "05_ingame.png"
if ($CapturePath -ne "" -and $null -eq $noticeSeen) {
  # 有取证却看不到公告：不能拿世界区当公告按钮盲点（那就是本轮事故），直接中止（fail-closed）。
  Write-Output ("NOTICE_POLL RED：" + $NoticeWaitSec + "s 内未见 s2c SM_SENDNOTICE(658)（fail-closed：一次也不点世界区）")
  throw "NOTICE_POLL RED: 公告未出现（-CapturePath 中无 SM_SENDNOTICE 658，不点世界区）"
} else {
  Stage "notice_poll"
  if ($NoticePollClicks -gt 0) {
    Send-Click $noticeNb[0] $noticeNb[1] 1
  }
  Move-Mouse $noticePark[0] $noticePark[1]
  if ($CapturePath -ne "") {
    $noticeOk = Wait-CaptureFrame $CapturePath "c2s" 1018 $NoticeVerifySec
    if ($null -eq $noticeOk) {
      Write-Output ("NOTICE_POLL RED：单击后 " + $NoticeVerifySec + "s 内未见 c2s CM_LOGINNOTICEOK(1018)（公告可能没被点掉；服务端 10s 会踢人）")
      throw "NOTICE_POLL RED: CM_LOGINNOTICEOK(1018) 未出网（fail-closed：不再点世界区）"
    }
    Write-Output ("NOTICE_POLL OK clicks=" + $NoticePollClicks + " dismissed=1018 mode=capture")
  } else {
    Write-Output ("NOTICE_POLL UNVERIFIED clicks=" + $NoticePollClicks + " mode=no_capture（无 -CapturePath，无法校验 1018）")
  }
  Shot "05b_after_notice.png"
}
Stage "ingame"
Start-Sleep -Seconds 2
Shot "05c_ingame_ready.png"

# ---- 坐标门（卡 fixture-harden）：任何世界区点击之前先断言角色格 ----
# 依据：docs/npc-render-diagnosis-20261022.md §2.3/§2.4。角色一旦被夹具走远，
# 所有屏幕点都会换算到空地 ⇒ CM_WALK（而不是 CM_CLICKNPC），且国王出视窗。
# 取证：-CapturePath 里 ts_ms 最新的一条「c2s CM_WALK(3011) recog=X|Y<<16」或
#       「s2c SM_LOGON(50)/SM_NEWMAP(51) param=X tag=Y」。不一致 / 无证据 ⇒ RED + 中止。
# -ClickNpc 未显式给 -ExpectPos 时按比奇国王场景默认 (29,33)（NPC 在 (29,32)）。
$gateExpect = $ExpectPos
if ($gateExpect -eq "" -and $ClickNpc) { $gateExpect = "29,33" }
if ($gateExpect -ne "") {
  if ($CapturePath -eq "") {
    Write-Output ("POSITION_GATE RED missing -CapturePath（expect=" + $gateExpect + "，无取证不得继续世界区点击）")
    throw "POSITION_GATE RED: -ExpectPos 需要 -CapturePath（fail-closed）"
  }
  $gate = Test-PositionGate $CapturePath $gateExpect
  if (-not $gate.Ok) {
    Write-Output ("POSITION_GATE RED " + $gate.Detail)
    throw ("POSITION_GATE RED: 角色坐标与期望不符，中止（不继续盲点世界区）。" + $gate.Detail)
  }
  Write-Output ("POSITION_GATE GREEN " + $gate.Detail)
}

# ---- 固定 NPC 点击（可选；先过坐标门）----
# 默认 (512,263)：角色在 (29,33) 时 CXYfromMouseXY 逆映射 ⇒ 格 (29,32)（比奇国王所在格）。
# 旧值 (512,227) 换算成 (29,31)，是**两格北的空地**，点下去只会 CM_WALK —— 本轮「点国王变走位」的一部分。
if ($ClickNpc) {
  Stage "npc_click"
  $npcPx = $NpcClick.Split(",") | ForEach-Object { [int]$_ }
  Write-Output ("NPC_CLICK x=" + $npcPx[0] + " y=" + $npcPx[1] + " expect=" + $gateExpect)
  Send-Click $npcPx[0] $npcPx[1] 1
  Start-Sleep -Seconds 2
  Shot "05d_npc_click.png"
  Move-Mouse $noticePark[0] $noticePark[1]
}

# ---- 说话（M2 验收② `m2-03-talk`）：聊天框输入并回车，本角色文本出现在聊天框后 ----
# 回车是 toggle（收起=打开 / 打开=发送并收起）：先成对按两次保证「收起」，再按一次打开，
# 注入文本后回车发送。服务端 `SM_HEAR(40)` 会回声给说话者自己（oracle `SendRefMsg` 含 self），
# 客户端靠这条回声渲染本角色的话。
if (-not $SkipTalk) {
  Stage "talk"
  [void][MirFlow]::SetForegroundWindow($script:hwnd)
  Start-Sleep -Milliseconds 300
  if ($TalkBox -ne "") {
    $tb = $TalkBox.Split(",") | ForEach-Object { [int]$_ }
    Send-Click $tb[0] $tb[1] 1
    Start-Sleep -Milliseconds 400
  }
  for ($i = 0; $i -lt 3; $i++) {
    [MirFlow]::keybd_event(0x0D, 0, 0, [IntPtr]0)
    Start-Sleep -Milliseconds 120
    [MirFlow]::keybd_event(0x0D, 0, 2, [IntPtr]0)
    Start-Sleep -Milliseconds 400
  }
  Send-Text $TalkText
  Start-Sleep -Milliseconds 400
  [MirFlow]::keybd_event(0x0D, 0, 0, [IntPtr]0)
  Start-Sleep -Milliseconds 120
  [MirFlow]::keybd_event(0x0D, 0, 2, [IntPtr]0)
  Start-Sleep -Milliseconds $TalkWaitMs
  Stage "talk_sent"
  Shot $TalkShot
}

# ---- 看见别人（M2 验收② `m2-04-see-others`）：等同屏第二角色进入 AOI 后截图 ----
# 判据（owner §22）：第二角色进屏 ⇒ 观察者应收到 `SM_TURN(10)`+`SM_FEATURECHANGED(41)`
# （AOI 下行；进程级用例在 `bins/game-svr/src/downlink.rs`）。本阶段只负责「等待 + 截图」，
# 假人由调用方在进图前放进同图同 AOI（**常驻跑 BotSrv.exe**，不用 `run_bots.ps1`：后者达成计数即杀）。
if ($SeeOthersWaitSec -gt 0) {
  Stage "see_others_wait"
  Start-Sleep -Seconds $SeeOthersWaitSec
  Stage "see_others"
  Shot $SeeOthersShot
}

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

# ---- AttackTurn 标定（只有显式 -AttackTurnCalibrate 才走这里）----
# 现状 48x32 步长与已知失败证据见 param 注释与 docs/window-shrink-run-20261019.md §F2。
# 本块只驱动「右键→左键」探针并记录 [t_start_ms,t_end_ms]；CM_TURN/CM_HIT 的观测值
# 由 -CalibrateCapture（capture.jsonl）回填：窗口内一次跑完全部格偏移，导出 JSON+CSV。
# 旧的 -AttackTurn / -AttackRing 路径在下面，完全不经过本块。
if ($AttackTurnCalibrate -ne "") {
  $calProbes = ConvertFrom-TurnCalibrateSpec $AttackTurnCalibrate
  $ledger = New-TurnCalibrationLedger $calProbes "drive"
  $calIdx = 0
  foreach ($p in $calProbes) {
    Stage ("calibrate_turn_" + $p.I)
    $row = $ledger.probes[$calIdx]
    $row.t_start_ms = Get-TurnNowMs
    Send-RightClick $p.RightX $p.RightY
    Start-Sleep -Milliseconds $CalibrateTurnGapMs
    Stage ("calibrate_hit_" + $p.I)
    Send-Click $p.LeftX $p.LeftY 1
    Start-Sleep -Milliseconds $CalibrateSettleMs
    $row.t_end_ms = Get-TurnNowMs
    $calIdx++
  }
  Shot "10_attackturn_calibration.png"
  if ($CalibrateCapture -ne "") {
    if (Test-Path -LiteralPath $CalibrateCapture) {
      $ledger = Update-TurnCalibrationLedger $ledger $CalibrateCapture
    } else {
      Write-Output ("CALIB_CAPTURE_MISSING " + $CalibrateCapture +
        "（会话结束后用 GoldenExport 导出 capture.jsonl，再用 -AttackTurnDryRun -CalibrateCapture 回填）")
    }
  }
  $csvPath = Write-TurnCalibrationTable $ledger $CalibrateOut
  Write-TurnCalibrationSummary $ledger $CalibrateOut $csvPath
  Stage "calibrate_done"
}

# ---- 攻击：在角色周围按环状点几下（比奇省刷怪区怪物密集，点到即出 CM_HIT）----
# 旧口径（默认）仍然是 (512 + dx*48, 304 + dy*32)；48x32 只是**未被证实的假设**：
# 2026-10-19 缩窗 W0-1 第二轮 Rust 侧同配方 3 次（0,-1×2、0,1×1）全部落成 CM_WALK，
# 同点改按截图像素点（mir_click.ps1）立刻出 CM_HIT —— 见 docs/window-shrink-run-20261019.md §F2。
# 新映射只在显式给 -AttackTurnMap 时启用（按标定表 resolved 的像素点）；默认路径逐字节不变。
if ($AttackClicks -gt 0) {
  $ring = @("0,-40", "40,0", "0,40", "-40,0", "28,-28", "28,28", "-28,28", "-28,-28",
            "70,0", "0,70", "-70,0", "0,-70")
  if ($AttackRing -ne "") { $ring = $AttackRing.Split(";") }
  $turnCell = $null
  if ($AttackTurn -ne "") {
    # 校验：客户端分支要求 abs(Δ格)<=1（ClMain.pas:5444；>1 会落进 caRun 支不发 CM_TURN）
    $td = $AttackTurn.Split(",") | ForEach-Object { [int]$_ }
    if ($td.Count -ne 2) { throw "-AttackTurn 须为 'dx,dy'（两个整数格偏移），当前：$AttackTurn" }
    if ([Math]::Abs($td[0]) -gt 1 -or [Math]::Abs($td[1]) -gt 1) { throw "-AttackTurn 须为相邻格（|dx|,|dy|<=1），当前：$AttackTurn" }
    if ($td[0] -eq 0 -and $td[1] -eq 0) { throw "-AttackTurn 不能为 (0,0)（自格 ⇒ tdir 退化），当前：$AttackTurn" }
    $turnCell = $td
  }
  # -AttackTurnMap：标定后的新映射（只有显式给本参数才启用；否则下面保持旧字面量）。
  $turnPx = $null
  if ($turnCell -and $turnMapTable) {
    $turnPx = Resolve-TurnMapPixel $turnMapTable $turnCell[0] $turnCell[1]
  }
  # 格心锚（idle 时 CXYfromMouseXY 的逆映射原点；与 -CenterX/-CenterY 无关，后者是**精灵**中心标定）：
  #   x=512=SCREENWIDTH div 2；y=304=ShiftYOffset=Round((768-150)/32/2)*32-16（MShare.pas:4620，自格带 y∈[288,320)）
  $TurnCellCx = 512
  $TurnCellCy = 304
  for ($i = 0; $i -lt [Math]::Min($AttackClicks, $ring.Count); $i++) {
    $d = $ring[$i].Split(",") | ForEach-Object { [int]$_ }
    if ($turnCell) {
      Stage ("attack_turn_" + ($i + 1))
      # 旧口径：右键相邻格格心 512+dx*48 / 304+dy*32（未证实假设）；新映射：标定表 resolved 像素。
      if ($turnPx) {
        Send-RightClick $turnPx.RightX $turnPx.RightY
      } else {
        Send-RightClick ($TurnCellCx + $turnCell[0] * 48) ($TurnCellCy + $turnCell[1] * 32)
      }
      # oracle TurnIntervalTime=600ms（GameSvrConf.cs:1814，连续 CM_TURN 更快会被拒；Rust 侧未接限速，
      # 但驱动必须按 oracle 节拍）；客户端 CanNextAction/ServerAcceptNextAction
      # （ClMain.pas:5452）也要求上一动作收尾。右键助手内部已 ~510ms，再 400ms ⇒ turn→hit ~0.9s、
      # 连续 turn >2.3s，均在 oracle 节拍内（HitIntervalTime=900ms 不变）。
      Start-Sleep -Milliseconds 400
    }
    Stage ("attack_" + ($i + 1))
    # F-W0A（2026-10-18，缩窗 W0-1 实测订正）：给了 -AttackTurn 且**未显式给 -AttackRing** 时，
    # 左键必须点在**转向的目标格中心**（格心锚 512/304 + 偏移×48/32）——否则仍按旧环位置点
    # （默认环第 1 项 = 北侧 -40px），会点空/点地（CM_WALK/RUN），永远打不到刚转向的怪。
    # 显式 -AttackRing 仍按调用方给的像素偏移（高级用法，逐字节不变）。
    if ($turnCell -and $AttackRing -eq "") {
      if ($turnPx) {
        $clickX = $turnPx.LeftX
        $clickY = $turnPx.LeftY
      } else {
        $clickX = $TurnCellCx + $turnCell[0] * 48
        $clickY = $TurnCellCy + $turnCell[1] * 32
      }
    } else {
      $clickX = $CenterX + $d[0]
      $clickY = $CenterY + $d[1]
    }
    Send-Click $clickX $clickY 1
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

# ---- 跑键（CM_RUN 3013）：在目标点按住左键（见 param 注释；Send-HoldRun）----
if ($RunHoldMs -gt 0) {
  # 绕角色中心两圈 8 方向各拖一次（PlayScn.pas:3101 CanRun 还会检**全目标格**
  # CanWalkEx(ex,ey)：远点落在墙/房时必回退走 ⇒ 用近点 2~4 格重复）
  $ri = 0
  foreach ($rad in (110, 210)) {
    foreach ($ang in (0, 45, 90, 135, 180, 225, 270, 315)) {
      $rr = $ang * [Math]::PI / 180.0
      $px = [int]($CenterX + $rad * [Math]::Cos($rr))
      $py = [int]($CenterY + $rad * [Math]::Sin($rr))
      $ri++
      Stage ("run_hold_" + $ri)
      Send-HoldRun $px $py $RunHoldMs
      Shot ("06run_" + $ri + ".png")
    }
  }
}

# ---- 小退：Alt+X -> 确定 ----
# ---- 会话拆除（顶号）：本账号在 BotSrv 侧再登录一次 ----
if ($SkipLogout) {
  # 三路径 ④ 采集：disconnect/shutdown 需要客户端**留在世界里**（不触发 CM_SOFTCLOSE 1009）。
  Stage "skip_logout"
  Shot "07b_skip_logout_inworld.png"
} else {
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

# ---- 小退首选路径：点底部条的「小退」按钮（客户端自带入口，鼠标 PostMessage）----
# 依据：DBotLogoutClick → TfrmMain.AppLogout → 确认框 → CM_SOFTCLOSE(1009)。
# 坐标由客户端布局代码算出：DBottom.Top = SCREENHEIGHT-251（FState.pas:2446），
# 「小退」按钮 DBotLogout 相对条内 (754,104) ⇒ 绝对 (754,621)（「退出」DBotExit 是 784,621）。
# 先悬停截图（悬停会弹提示「小退(ALT-X)」，FState.pas:16428），作为"点对位置"的证据。
Stage "logout_button"
$lb = "754,621".Split(",") | ForEach-Object { [int]$_ }
[void][MirFlow]::SetForegroundWindow($script:hwnd)
Start-Sleep -Milliseconds 300
[void][MirFlow]::SetCursorPos(($pt.X + $lb[0]), ($pt.Y + $lb[1]))
$lpBtn = [IntPtr](($lb[1] -shl 16) -bor ($lb[0] -band 0xFFFF))
[void][MirFlow]::PostMessage($script:hwnd, 0x0200, [IntPtr]0, $lpBtn)
Start-Sleep -Milliseconds 800
Shot "07a_logout_btn_hover.png"
Send-Click $lb[0] $lb[1] 1
Start-Sleep -Seconds 2
Shot "07b_logout_confirm_dlg.png"

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
}

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
