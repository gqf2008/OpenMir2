#requires -Version 5
<#
.SYNOPSIS
  mir_flow.ps1 — 真实客户端基线流程驱动（登录→选人→进游戏→走路→小退→再进）。

.DESCRIPTION
  从 E:\tmp\mirflow2.ps1 / amtest.ps1 收编而来：同一套 Win32 消息点击/打字/截图，
  增加了阶段标记（NDJSON，供日志时间线对齐）与参数化账号/输出目录。
  点击坐标沿用旧脚本已验证值（1024x768 窗口）。

  阶段序列（每阶段写一条 {"t_wall_ms","stage"} 到 -StageLog）：
    start → login_id → login_pwd → login_submit → charsel → enter_game → notice_wait
    → notice_poll → ingame → talk → talk_sent → see_others_wait → see_others
    → logout_alt_x → logout_confirm → back_charsel → reenter → reentered → done
    （-ClickNpc 时另有 npc_click；公告/坐标门的证据源见 -ProxyDir / -CapturePath。）
    四类玩家动作（卡 openmir2-mirflow-combat-pickup-stages，只追加）：pickup / npc_dialog /
    trade / combat_target / combat —— 由 -PickupStage / -NpcDialogStage / -TradeStage /
    -CombatStage 开启；证据落 <OutDir>/flow-stages-evidence.json（判据见 Get-*Evidence 注释）。

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File mir_flow.ps1 -OutDir E:\tmp\flow1

.EXAMPLE
  # live 证据（推荐）：proxy 运行中直接读 chunks.ndjson + conn-*.bin，无需等 export_s2c
  powershell -ExecutionPolicy Bypass -File mir_flow.ps1 `
    -OutDir E:\tmp\flow2 -ProxyDir E:\tmp\proxy -ExpectPos "29,33" -ClickNpc
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
  # F5：只允许 1。0 会与 1018 校验矛盾（不点却等 1018），>1 会把世界区走掉 ⇒ 都 fail-closed。
  [int]$NoticePollClicks = 1,        # 公告「确定」按钮点击次数：**只允许 1**（0 或 >1 直接拒绝）
  [string]$NoticeButton = "522,525",  # 公告确定按钮（客户端坐标；沿用旧脚本实测值）
  [string]$NoticeParkPoint = "522,760",  # 点完公告后鼠标停这里（底部面板区，不是世界区；只移动不点击）
  [int]$NoticeWaitSec = 4,           # 等公告出现：有证据源（-ProxyDir/-CapturePath）时按 s2c SM_SENDNOTICE(658) 判定
  [int]$NoticeVerifySec = 6,         # 点后等 c2s CM_LOGINNOTICEOK(1018)（仅证据源模式校验；按点击时刻定界）
  # F1：live 证据源 = proxy 的 --out 目录（运行中直接读 chunks.ndjson + conn-*.bin，无需等导出）。
  [string]$ProxyDir = "",            # mir2_proxy --out 目录；运行中切帧 + EDCode 解 ident/param/tag/recog
  # 离线/事后证据源 = export_s2c 产物。live 流程用它时必须是本场证据（F2：旧文件启动前就 RED）。
  [string]$CapturePath = "",
  [long]$EvidenceSinceMs = 0,        # 覆盖证据下界（离线自检用；0 = 按 proxy session / 流程启动墙钟自动）
  [string]$ExpectPos = "",           # "X,Y"：进入世界后、任何世界区点击前断言角色格；不一致 ⇒ POSITION_GATE RED + 中止
  [switch]$ClickNpc,                 # 进世界后点一次固定 NPC（坐标 -NpcClick）；必须先过坐标门（需要证据源）
  [string]$NpcClick = "512,263",     # 角色 (29,33) 时该点对应 NPC(29,32)；旧值 (512,227) 实际对应 y=31（两格北，空地）
  # F3：无证据源时，点击前必须有客户端侧信号；拿不到就只 Move-Mouse 不点击并 UNVERIFIED。
  [string]$NoticeProbe = "",         # 可选探针脚本/程序：exit 0=公告可见 / 1=不可见 / 其它=unknown
  [switch]$NoticeWindowProbe,        # 开启 Win32 顶层窗口探测（客户端进程除主窗口外的可见窗口 >0 ⇒ 可见）
  [string]$NoticePixelProbe = "",    # 可选截图像素探针 "clientX,clientY,RRGGBB,tol"（屏幕取色，tol=每通道容差）
  [switch]$FixtureDryRun,            # 离线自检：不启动客户端；-NoticePollSim / -ExpectPos / -NoticeEvidenceCheck
  [switch]$FixturePreflight,         # 与 -FixtureDryRun 联用：跑 live 证据源前置门（proxy 存活/旧 capture RED），仍不启客户端
  [string]$NoticePollSim = "",       # dismiss1 | slow | absent | nosignal：模拟公告轮询控制流（与 -FixtureDryRun 联用）
  [switch]$NoticeEvidenceCheck,      # 离线：只查证据源里本场的 s2c SM_SENDNOTICE(658)（F1/F2 自检）
  # ---- 四类玩家动作阶段（卡 openmir2-mirflow-combat-pickup-stages；只追加，既有 25 阶段名与相对顺序不动）----
  # 每类都产出 before/after 可机判证据（背包行数 / 金币 / 怪物 HP 或死亡 / 对话帧），
  # 落到 <OutDir>/flow-stages-evidence.json；判据见 Get-*Evidence 注释。
  # 真机验证需独占窗口（卡上先签 comment 声明时段），本轮只做离线性/构造级验证。
  [switch]$PickupStage,             # 捡物：点脚下物品 → 服务端入包（SM_ADDITEM）或金币变化
  [string]$PickupPixel = "",        # 物品的屏幕像素 "x,y"（服务端只处理**角色格**上的物品）
  [switch]$NpcDialogStage,          # NPC 对话：点 NPC → 脚本说词/菜单帧（643/767 且 body 非空）
  [string]$NpcDialogPixel = "",     # NPC 像素 "x,y"；留空回落到 -NpcClick
  [switch]$TradeStage,              # 买卖：先开商人对话，再点菜单项 → 成功帧 + 金币按方向变化
  [string]$TradeNpcPixel = "",      # 商人 NPC 像素 "x,y"
  [string]$TradeMenuPixel = "",     # 商人物品/出售菜单项像素 "x,y"
  [ValidateSet("buy", "sell")][string]$TradeKind = "buy",
  [switch]$CombatStage,             # 有目标战斗：profile 的 fixtures.monsters 前置刷怪 → 打向靶子 → 掉血/死亡
  [string]$Profile = "",            # card-window profile（读 fixtures.monsters；同 tools/window/profiles/*.json）
  [string]$PlayerMap = "",          # 角色所在图（经典协议线上没有图名；必须显式给，否则 combat RED）
  [int]$AttackRangeCells = 1,       # AP/近战攻击范围（切比雪夫格数）
  [string]$CombatTargetPixel = "",  # 靶子的屏幕像素 "x,y"；留空按格心投影 (512,304)+(dx*48,dy*32)
  [int]$ActionWaitSec = 8,          # 每个动作后等服务端副作用落地的秒数
  [int]$TradeDialogMaxAgeSec = 60,  # 商人对话必须在这个窗口内打开（否则算「前置缺失」）
  [string]$LogDir = "",             # 服务端日志目录（可选；**不在脚本里硬编码任何日志路径**）
  [switch]$RequireLogEvidence,      # 给了 -LogDir 但为空/无日志时 RED（否则显式 SKIP 并说明）
  [string]$EvidenceOut = "",        # 默认 <OutDir>/flow-stages-evidence.json
  [switch]$StageEvidenceCheck,      # 离线：不启动客户端，只按 -ProxyDir/-CapturePath 评估四类动作（牙靠它重放）
  # 动作前显式前置门（A2）：只读消费 `provision_real_machine_fixtures.py --gate` 的 JSON 输出。
  # 生成（窗口当天，只读、只连测试库）：
  #   python -I -B tools/client/provision_real_machine_fixtures.py --gate `
  #     --db mir2_test_db_r --char aaa --map 3 --profile <profile.json> > gate.json
  # 门不过（missing 非空）⇒ 对应阶段在「没生效」时归 UNDECIDABLE(missing_prereq) 并点名；
  # 未给本参数时：monster 夹具由 -Profile/-PlayerMap 自行推导，其余前置缺项不臆断（沿用 RED）。
  [string]$PrereqGateJson = ""
)

$ErrorActionPreference = "Stop"
# ==== 成因三档 + A7 用法错：退出码契约（卡 openmir2-mirflow-verdict-cause-split）====
# 裁决：能不能测 = UNDECIDABLE(missing_prereq)；该发生却没发生 = RED(spawn_failed/out_of_range)。
# 三者 kind 与 rc 都必须不同（A6）；用法错与任何 verdict 都必须不同（A7）。
#   rc 0 = PASS / 全 GREEN
#   rc 1 = RED，成因不属于下面两种具名成因（旧口径；与既有调用兼容）
#   rc 2 = USAGE（A7：参数名/取值错，脚本没按意图跑）
#   rc 6 = UNDECIDABLE + kind=missing_prereq（与 provision_real_machine_fixtures.py --gate 的 rc=6 对齐）
#   rc 7 = RED + kind=spawn_failed
#   rc 8 = RED + kind=out_of_range
$script:RC_PASS = 0
$script:RC_RED = 1
$script:RC_USAGE = 2
$script:RC_UNDECIDABLE = 6
$script:RC_SPAWN_FAILED = 7
$script:RC_OUT_OF_RANGE = 8
# A7：PowerShell **简单脚本**（无 [CmdletBinding()]）对未知命名参数**不报绑定错**，
# 而是塞进 $args 后继续执行（实测 `-StageEvidenceChek` 会静默落进 $args）。
# 这正是 verify_chunks 的老坑：参数写错 = 脚本根本没按意图跑，却与「校验红」同流。
# 在任何副作用之前把它判为 USAGE，并用独立 rc=2 退出（绝不落到任何 verdict）。
if ($args.Count -gt 0) {
  Write-Output ("MIRFLOW USAGE: 未知参数 " + ($args -join " ") +
    "（疑似参数名拼写错误；用法错 rc=2，不是任何 verdict。正确入口：-StageEvidenceCheck）")
  exit 2
}
# F2：本流程启动墙钟（在没有 proxy session.json 时作为证据下界）。必须在启动客户端之前取。
$script:flowStartMs = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
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
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "capture.jsonl 不存在（须是文件）：$path" }
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

# ==== 夹具加固：证据源（live proxy / 离线 capture）/ 公告轮询 / 坐标门 ==================
# F1（live 证据）：-ProxyDir 指向 mir2_proxy 的 --out 目录，**运行中**直接读
#   chunks.ndjson 的 data 事件 + conn-NNNN.{c2s,s2c}.bin：按线上帧界（c2s `#1…!`，
#   s2c `#…!$`，裸 `*` 心跳）切帧，用 EDCode 解每帧头块（16 编码字符 = 12B：
#   recog i32le / ident u16le / param u16le / tag u16le / series u16le）。
#   不再依赖会话结束后的 export_s2c/GoldenExport；capture.jsonl 只作离线/事后证据源。
# F2（证据新鲜度）：-ProxyDir 用 session.json 的 start_wall_ms 作下界；-CapturePath 用本流程
#   启动墙钟作下界（旧 capture 在启动客户端之前就被拒）；-EvidenceSinceMs 可覆盖（离线自检）。
#   所有判定只吃 ts_ms >= SinceMs 的帧；658/1018/3011/50/51 同理。
$script:EvidenceSlackMs = 2000   # 时钟/启动先后的小容差（2s）

function ConvertFrom-EDCode([byte[]]$src) {
  # 逐字节移植 EncryptUtil.Decode（XOR 0xAC + base 0x3C，3B→4C；len%4==1 的尾字节静默丢弃）。
  $len = $src.Length
  $cycles = [int][Math]::Floor($len / 4)
  $left = $len % 4
  $out = New-Object System.Collections.Generic.List[byte]
  for ($i = 0; $i -lt $cycles; $i++) {
    $b = $i * 4
    $rem = (([int]$src[$b + 3] - 0x3C) -band 0xFF)
    $t0 = (([int]$src[$b] - 0x3C) -band 0xFF)
    $c0 = (((($t0 -shl 2) -band 0xF0) -bor ($rem -band 0x0C) -bor ($t0 -band 0x03)) -band 0xFF)
    $out.Add([byte]($c0 -bxor 0xAC))
    $t1 = (([int]$src[$b + 1] - 0x3C) -band 0xFF)
    $c1 = (((($t1 -shl 2) -band 0xF0) -bor (($rem -shl 2) -band 0x0C) -bor ($t1 -band 0x03)) -band 0xFF)
    $out.Add([byte]($c1 -bxor 0xAC))
    $t2 = (([int]$src[$b + 2] - 0x3C) -band 0xFF)
    $c2 = (($t2 -bor (($rem -shl 2) -band 0xC0)) -band 0xFF)
    $out.Add([byte]($c2 -bxor 0xAC))
  }
  if ($left -eq 2) {
    $rem = (([int]$src[$len - 1] - 0x3C) -band 0xFF)
    $t0 = (([int]$src[$len - 2] - 0x3C) -band 0xFF)
    $c0 = (((($t0 -shl 2) -band 0xF0) -bor (($rem -shl 2) -band 0x0C) -bor ($t0 -band 0x03)) -band 0xFF)
    $out.Add([byte]($c0 -bxor 0xAC))
  } elseif ($left -eq 3) {
    $rem = (([int]$src[$len - 1] - 0x3C) -band 0xFF)
    $t0 = (([int]$src[$len - 3] - 0x3C) -band 0xFF)
    $c0 = (((($t0 -shl 2) -band 0xF0) -bor ($rem -band 0x0C) -bor ($t0 -band 0x03)) -band 0xFF)
    $out.Add([byte]($c0 -bxor 0xAC))
    $t1 = (([int]$src[$len - 2] - 0x3C) -band 0xFF)
    $c1 = (((($t1 -shl 2) -band 0xF0) -bor (($rem -shl 2) -band 0x0C) -bor ($t1 -band 0x03)) -band 0xFF)
    $out.Add([byte]($c1 -bxor 0xAC))
  }
  return ,$out.ToArray()
}

function ConvertFrom-ProxyFrame([byte[]]$raw, [string]$dir) {
  # 帧头解码；心跳/act 帧/字符串帧/短帧返回 $null。口径与 GoldenExport 一致。
  $start = 0
  if ($dir -eq "c2s" -and $raw.Length -gt 1 -and $raw[0] -eq 0x23 -and $raw[1] -eq 0x31) { $start = 2 }
  elseif ($raw.Length -gt 0 -and $raw[0] -eq 0x23) { $start = 1 }
  $end = $raw.Length
  while ($end -gt $start -and ($raw[$end - 1] -eq 0x21 -or $raw[$end - 1] -eq 0x24)) { $end-- }
  if (($end - $start) -lt 16) { return $null }
  if ($raw[$start] -eq 0x2B) { return $null }   # "+GD/…" 明文动作帧（没有 ident）
  $enc = New-Object byte[] 16
  [Array]::Copy($raw, $start, $enc, 0, 16)
  $head = ConvertFrom-EDCode $enc
  if ($head.Length -lt 12) { return $null }
  if ($head[0] -eq 0x2A -and $head[1] -eq 0x2A) { return $null }   # "**…" 登录字符串帧
  return [pscustomobject]@{
    recog  = [BitConverter]::ToInt32($head, 0)
    ident  = [int][BitConverter]::ToUInt16($head, 4)
    param  = [int][BitConverter]::ToUInt16($head, 6)
    tag    = [int][BitConverter]::ToUInt16($head, 8)
    series = [int][BitConverter]::ToUInt16($head, 10)
    # body 长度（编码后字节数）：header 固定 16 个编码字符（12B）。>0 = 有报文 body。
    # 643/767 的「脚本确实说了话」判据需要它；不能只看「某帧出现」。
    body_len = [int]($end - $start - 16)
  }
}

$script:ProxyCache = @{}

function Read-ProxySessionStart([string]$dir) {
  $sj = Join-Path $dir "session.json"
  if (-not (Test-Path -LiteralPath $sj)) { return [long]0 }
  try { $o = [IO.File]::ReadAllText($sj, [Text.Encoding]::UTF8) | ConvertFrom-Json } catch { return [long]0 }
  if ($null -ne $o.PSObject.Properties["start_wall_ms"] -and $null -ne $o.start_wall_ms) { return [long]$o.start_wall_ms }
  return [long]0
}

function Update-ProxyFrameCache([string]$dir) {
  # 增量读：只消费 chunks.ndjson 已声明的 data 字节区间；未闭合的尾帧留到下次。
  $full = (Resolve-Path -LiteralPath $dir).Path
  $state = $script:ProxyCache[$full]
  if ($null -eq $state) {
    $state = [pscustomobject]@{
      Frames         = (New-Object System.Collections.ArrayList)
      Parsed         = @{}
      Events         = @{}
      ChunkOffset    = [long]0
      LatestDataTs   = [long]0
      SessionStartMs = (Read-ProxySessionStart $full)
    }
    $script:ProxyCache[$full] = $state
  }
  $chunks = Join-Path $full "chunks.ndjson"
  if (-not (Test-Path -LiteralPath $chunks)) { throw "proxy chunks.ndjson 不存在：$chunks" }
  # 增量读 chunks.ndjson：只解析本次新增的行（长会话里避免每次轮询重读整场）。
  $newBytes = New-Object byte[] 0
  $fsC = [IO.File]::Open($chunks, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
  try {
    if ($fsC.Length -lt [long]$state.ChunkOffset) {
      # 目录被复用/截断：重置增量状态（不静默拿旧 events 当真相）。
      $state.ChunkOffset = [long]0
      $state.Events = @{}
      $state.Parsed = @{}
      $state.Frames.Clear()
    }
    [void]$fsC.Seek([long]$state.ChunkOffset, [IO.SeekOrigin]::Begin)
    $ms = New-Object System.IO.MemoryStream
    try { $fsC.CopyTo($ms); $newBytes = $ms.ToArray() } finally { $ms.Dispose() }
  } finally { $fsC.Dispose() }
  $lineStart = 0
  while ($newBytes.Length -gt 0 -and ($nl = [Array]::IndexOf($newBytes, [byte]0x0A, $lineStart)) -ge 0) {
    $lineLen = $nl - $lineStart
    if ($lineLen -gt 0) {
      $line = [Text.Encoding]::UTF8.GetString($newBytes, $lineStart, $lineLen).TrimEnd([char]13)
      if ($line -match '"event"\s*:\s*"data"') {
        $r = $null
        try { $r = $line | ConvertFrom-Json } catch { $r = $null }
        if ($null -ne $r -and $null -ne $r.PSObject.Properties["conn"] -and $null -ne $r.PSObject.Properties["dir"]) {
          $k = ([int]$r.conn).ToString() + "/" + [string]$r.dir
          if (-not $state.Events.ContainsKey($k)) { $state.Events[$k] = New-Object System.Collections.ArrayList }
          $ev = [pscustomobject]@{ Off = [long]$r.off; N = [int]$r.n; Ts = [long]$r.t_wall_ms }
          [void]$state.Events[$k].Add($ev)
          if ($ev.Ts -gt [long]$state.LatestDataTs) { $state.LatestDataTs = $ev.Ts }
        }
      }
    }
    $lineStart = $nl + 1
  }
  $state.ChunkOffset = [long]$state.ChunkOffset + [long]$lineStart
  foreach ($k in $state.Events.Keys) {
    # 同一 (conn,dir) 的 data 事件按写入顺序追加，offset 单调递增，无需每次重排。
    $evs = @($state.Events[$k])
    $covered = 0
    foreach ($e in $evs) { if (($e.Off + $e.N) -gt $covered) { $covered = $e.Off + $e.N } }
    $parsed = 0
    if ($state.Parsed.ContainsKey($k)) { $parsed = [long]$state.Parsed[$k] }
    if ($covered -le $parsed) { continue }
    $pair = $k.Split("/")
    $conn = [int]$pair[0]; $dir2 = $pair[1]
    $bin = Join-Path $full ("conn-" + $conn.ToString("0000") + "." + $dir2 + ".bin")
    if (-not (Test-Path -LiteralPath $bin)) { continue }
    $need = [int]($covered - $parsed)
    $buf = New-Object byte[] $need
    $fs = [IO.File]::Open($bin, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try {
      [void]$fs.Seek($parsed, [IO.SeekOrigin]::Begin)
      $read = 0
      while ($read -lt $need) {
        $n = $fs.Read($buf, $read, $need - $read)
        if ($n -le 0) { break }
        $read += $n
      }
    } finally { $fs.Dispose() }
    if ($read -le 0) { continue }
    $i = 0
    $lastComplete = 0
    while ($i -lt $read) {
      if ($buf[$i] -eq 0x2A) { $i++; $lastComplete = $i; continue }   # '*' 心跳
      $idx = [Array]::IndexOf($buf, [byte]0x21, $i)
      if ($idx -lt 0 -or $idx -ge $read) { break }                     # 未闭合尾帧
      $end = $idx + 1
      if ($end -lt $read -and $buf[$end] -eq 0x24) { $end++ }
      $frameOff = $parsed + $i
      $raw = New-Object byte[] ($end - $i)
      [Array]::Copy($buf, $i, $raw, 0, ($end - $i))
      $dec = ConvertFrom-ProxyFrame $raw $dir2
      if ($null -ne $dec) {
        $ts = [long]0
        foreach ($e in $evs) { if ($frameOff -ge $e.Off -and $frameOff -lt ($e.Off + $e.N)) { $ts = $e.Ts; break } }
        [void]$state.Frames.Add([pscustomobject]@{
          dir = $dir2; ts_ms = $ts; conn = $conn; off = $frameOff
          ident = [int]$dec.ident; recog = [long]$dec.recog
          param = [int]$dec.param; tag = [int]$dec.tag; series = [int]$dec.series
          body_len = [int]$dec.body_len
        })
      }
      $i = $end
      $lastComplete = $i
    }
    $state.Parsed[$k] = $parsed + $lastComplete
  }
}

$script:CaptureCache = @{}

function Get-CaptureFrames([string]$path) {
  # 读 capture.jsonl（export_s2c 产物）；按 mtime+size 缓存，避免轮询反复解析大文件。
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "capture.jsonl 不存在（须是文件）：$path" }
  $fi = Get-Item -LiteralPath $path
  $key = $fi.FullName
  $stamp = $fi.LastWriteTimeUtc.Ticks.ToString() + ":" + $fi.Length
  $cached = $script:CaptureCache[$key]
  if ($null -ne $cached -and $cached.Stamp -eq $stamp) { return ,$cached.Frames }
  $frames = New-Object System.Collections.ArrayList
  foreach ($line in [IO.File]::ReadLines($fi.FullName)) {
    if ($line -notmatch '"kind"\s*:\s*"frame"') { continue }
    try { $r = $line | ConvertFrom-Json } catch { continue }
    if ($null -eq $r -or $null -eq $r.PSObject.Properties["ident"] -or $null -eq $r.ident) { continue }
    $dir = ""; $recog = [long]0; $ts = [long]0; $conn = 0
    $param = 0; $tag = 0; $series = 0
    if ($null -ne $r.PSObject.Properties["dir"]) { $dir = [string]$r.dir }
    if ($null -ne $r.PSObject.Properties["recog"] -and $null -ne $r.recog) { $recog = [long]$r.recog }
    if ($null -ne $r.PSObject.Properties["ts_ms"] -and $null -ne $r.ts_ms) { $ts = [long]$r.ts_ms }
    if ($null -ne $r.PSObject.Properties["conn"] -and $null -ne $r.conn) { $conn = [int]$r.conn }
    if ($null -ne $r.PSObject.Properties["param"] -and $null -ne $r.param) { $param = [int]$r.param }
    if ($null -ne $r.PSObject.Properties["tag"] -and $null -ne $r.tag) { $tag = [int]$r.tag }
    if ($null -ne $r.PSObject.Properties["series"] -and $null -ne $r.series) { $series = [int]$r.series }
    [void]$frames.Add([pscustomobject]@{
      dir = $dir; ts_ms = $ts; conn = $conn; off = [long]0
      ident = [int]$r.ident; recog = $recog; param = $param; tag = $tag; series = $series
      body_len = 0   # capture.jsonl 不带编码后 body 长度；对话帧判据需要 -ProxyDir（live）
    })
  }
  $script:CaptureCache[$key] = [pscustomobject]@{ Stamp = $stamp; Frames = $frames }
  return ,$frames
}

function Get-EvidenceFrames($source) {
  if ($source.Kind -eq "proxy") {
    Update-ProxyFrameCache $source.Path
    $full = (Resolve-Path -LiteralPath $source.Path).Path
    return ,$script:ProxyCache[$full].Frames
  }
  if ($source.Kind -eq "capture") { return ,(Get-CaptureFrames $source.Path) }
  throw "证据源未配置（需要 -ProxyDir 或 -CapturePath）"
}

function Assert-ProxyLive([string]$dir) {
  # live proxy 必须在跑：session.json + ready.flag + pid 存活。缺失 ⇒ 启动客户端前 fail-closed。
  $sj = Join-Path $dir "session.json"
  if (-not (Test-Path -LiteralPath $sj)) { throw "proxy session.json 不存在（proxy 没起？）：$sj" }
  if (-not (Test-Path -LiteralPath (Join-Path $dir "ready.flag"))) { throw "proxy ready.flag 不存在（proxy 没就绪？）：$dir" }
  try { $o = [IO.File]::ReadAllText($sj, [Text.Encoding]::UTF8) | ConvertFrom-Json } catch { throw "proxy session.json 坏：$sj" }
  if ($null -ne $o.PSObject.Properties["pid"] -and $o.pid) {
    $p = Get-Process -Id ([int]$o.pid) -ErrorAction SilentlyContinue
    if ($null -eq $p) { throw ("proxy 进程 pid=" + $o.pid + " 不在（session.json 与现场不符）") }
  }
}

function Assert-CaptureFresh([string]$path, [long]$sinceMs) {
  # F2：live 流程拿旧 capture ⇒ 在启动客户端之前就 RED（不让人“立刻看到 658/1018”）。
  $frames = Get-CaptureFrames $path
  $max = [long]0
  foreach ($f in $frames) { if ($f.ts_ms -gt $max) { $max = $f.ts_ms } }
  $fi = Get-Item -LiteralPath $path
  $mtimeMs = ([DateTimeOffset]$fi.LastWriteTimeUtc).ToUnixTimeMilliseconds()
  if ($max -le 0 -or $max -lt $sinceMs -or $mtimeMs -lt $sinceMs - $script:EvidenceSlackMs) {
    throw ("STALE_CAPTURE: capture.jsonl 最新帧 ts_ms=" + $max + " / mtime_ms=" + $mtimeMs +
      " < 本场 since=" + $sinceMs +
      "（旧证据一律 RED；live 请用 -ProxyDir 指向 proxy --out 目录）")
  }
}

function New-EvidenceSource([string]$proxyDir, [string]$capturePath, [long]$sinceOverride, [long]$flowStartMs, [switch]$Offline) {
  $source = [pscustomobject]@{ Kind = "none"; Path = ""; SinceMs = [long]0; SessionStartMs = [long]0 }
  if ($proxyDir -ne "" -and $capturePath -ne "") {
    throw "-ProxyDir 与 -CapturePath 互斥（证据源只能有一个；live 用 -ProxyDir，事后回填用 -CapturePath）"
  }
  if ($proxyDir -ne "") {
    if (-not (Test-Path -LiteralPath $proxyDir)) { throw "proxy 输出目录不存在：$proxyDir" }
    $full = (Resolve-Path -LiteralPath $proxyDir).Path
    if (-not (Test-Path -LiteralPath (Join-Path $full "chunks.ndjson"))) {
      throw ("proxy 目录里没有 chunks.ndjson：$full")
    }
    $sessionStart = Read-ProxySessionStart $full
    $since = $sessionStart
    if ($sinceOverride -gt 0) { $since = $sinceOverride }
    elseif ($sessionStart -gt 0) { $since = [Math]::Max([long]0, $sessionStart - $script:EvidenceSlackMs) }
    $source.Kind = "proxy"; $source.Path = $full; $source.SinceMs = [long]$since; $source.SessionStartMs = $sessionStart
    if (-not $Offline) { Assert-ProxyLive $full }
  } elseif ($capturePath -ne "") {
    if (-not (Test-Path -LiteralPath $capturePath -PathType Leaf)) { throw "capture.jsonl 不存在（须是文件）：$capturePath" }
    $full = (Resolve-Path -LiteralPath $capturePath).Path
    $since = [long]0
    if ($sinceOverride -gt 0) { $since = $sinceOverride }
    elseif (-not $Offline -and $flowStartMs -gt 0) { $since = [Math]::Max([long]0, $flowStartMs - $script:EvidenceSlackMs) }
    # 离线自检（-FixtureDryRun）不给 -EvidenceSinceMs 时不设下界；live 才用流程启动墙钟（F2）。
    $source.Kind = "capture"; $source.Path = $full; $source.SinceMs = [long]$since
    if (-not $Offline) { Assert-CaptureFresh $full $since }
  }
  return $source
}

function Assert-ProxyFresh($source, [long]$flowStartMs) {
  # F2：proxy 目录可能是上一场遗留（登录后才发现没证据 ⇒ 流程报废）。要求最新 data 事件
  # 落在本流程启动之后（小容差）。proxy 可比流程早启动，但本场登录一定会产生新 data。
  if ($source.Kind -ne "proxy") { return }
  $full = (Resolve-Path -LiteralPath $source.Path).Path
  Update-ProxyFrameCache $full
  $latest = [long]$script:ProxyCache[$full].LatestDataTs
  $floor = [Math]::Max([long]0, $flowStartMs - $script:EvidenceSlackMs)
  if ($latest -le 0 -or $latest -lt $floor) {
    throw ("PROXY_STALE: proxy 最新 data t_wall_ms=" + $latest + " < 本流程 since=" + $floor +
      "（目录不是本场？）")
  }
}

function Get-SelfCellEvidence($source, [long]$sinceMs) {
  # F6：c2s CM_WALK(3011) 的 recog 是**目标格**（点了就走，未必已到）。只把它当作
  #   “与上一条可信自身证据切比雪夫距离 <=1 的下一步”；跨度 >1 的目标不采纳，回退到
  #   s2c SM_LOGON(50)/SM_NEWMAP(51) 的锚点或上一格可信走位。
  $frames = Get-EvidenceFrames $source
  $ordered = @($frames | Where-Object { $_.ts_ms -ge $sinceMs } |
    Sort-Object @{ Expression = "ts_ms"; Ascending = $true }, @{ Expression = "conn"; Ascending = $true }, @{ Expression = "off"; Ascending = $true })
  $cell = $null
  $sourceName = ""
  $seq = $null
  $ts = [long]0
  $trustedWalks = 0
  $ignoredFar = 0
  foreach ($f in $ordered) {
    if ($f.dir -eq "s2c" -and ($f.ident -eq 50 -or $f.ident -eq 51)) {
      $cell = [pscustomobject]@{ X = [int]$f.param; Y = [int]$f.tag }
      $sourceName = if ($f.ident -eq 50) { "s2c SM_LOGON(50)" } else { "s2c SM_NEWMAP(51)" }
      $ts = [long]$f.ts_ms; $seq = $f.off
    } elseif ($f.dir -eq "c2s" -and $f.ident -eq 3011) {
      if ($null -eq $cell) { $ignoredFar++; continue }   # 没有锚点：不拿目标格当坐标
      $tx = Get-TurnLoWord ([long]$f.recog)
      $ty = Get-TurnHiWord ([long]$f.recog)
      $dist = [Math]::Max([Math]::Abs($tx - [int]$cell.X), [Math]::Abs($ty - [int]$cell.Y))
      if ($dist -le 1) {
        $cell = [pscustomobject]@{ X = $tx; Y = $ty }
        $sourceName = "c2s CM_WALK(3011)"
        $ts = [long]$f.ts_ms; $seq = $f.off
        $trustedWalks++
      } else {
        $ignoredFar++
      }
    }
  }
  if ($null -eq $cell) { return $null }
  return [pscustomobject][ordered]@{
    X = [int]$cell.X; Y = [int]$cell.Y; ts_ms = $ts; source = $sourceName; seq = $seq
    trusted_walks = $trustedWalks; ignored_far_walks = $ignoredFar
  }
}

function Test-PositionGate($source, [string]$expect, [long]$sinceMs) {
  # "X,Y" ⇒ 断言角色格。不一致 / 无证据 ⇒ Ok=$false（fail-closed，调用方必须中止）。
  $parts = $expect.Split(",")
  if ($parts.Count -ne 2 -or -not (Test-TurnIntLiteral $parts[0].Trim()) -or -not (Test-TurnIntLiteral $parts[1].Trim())) {
    return [pscustomobject]@{ Ok = $false; Detail = ("-ExpectPos 须为 'X,Y'（两个整数），当前：'" + $expect + "'") }
  }
  $ex = [int]$parts[0]; $ey = [int]$parts[1]
  $ev = Get-SelfCellEvidence $source $sinceMs
  if ($null -eq $ev) {
    return [pscustomobject]@{ Ok = $false; Detail = ("证据源中没有自身坐标证据（c2s CM_WALK(3011) 需有 SM_LOGON/51 锚点且步长<=1；或 s2c SM_LOGON(50)/SM_NEWMAP(51)）；expect=" + $ex + "," + $ey) }
  }
  $ok = ([int]$ev.X -eq $ex -and [int]$ev.Y -eq $ey)
  $detail = ("actual=" + $ev.X + "," + $ev.Y + " expect=" + $ex + "," + $ey +
             " source='" + $ev.source + "' seq=" + $ev.seq + " ts_ms=" + $ev.ts_ms +
             " trusted_walks=" + $ev.trusted_walks + " ignored_far_walks=" + $ev.ignored_far_walks)
  return [pscustomobject]@{ Ok = $ok; ActualX = [int]$ev.X; ActualY = [int]$ev.Y; ExpectX = $ex; ExpectY = $ey; Detail = $detail }
}

function Wait-EvidenceFrame($source, [string]$dir, [int]$ident, [int]$timeoutSec, [long]$sinceMs, [long]$afterMs = 0) {
  # 轮询证据源直到出现 dir/ident 且 ts_ms >= max(sinceMs, afterMs) 的帧；返回**最新**一条，
  # 超时返回 $null。取最新（而不是第一条）才能让“点后 1018”只认本次点击之后的帧。
  $floor = [Math]::Max([long]$sinceMs, [long]$afterMs)
  $deadline = (Get-Date).AddSeconds([Math]::Max(0, $timeoutSec))
  while ($true) {
    $best = $null
    foreach ($f in (Get-EvidenceFrames $source)) {
      if ($f.dir -ne $dir -or [int]$f.ident -ne $ident) { continue }
      if ([long]$f.ts_ms -lt $floor) { continue }
      if ($null -eq $best -or [long]$f.ts_ms -ge [long]$best.ts_ms) { $best = $f }
    }
    if ($null -ne $best) { return $best }
    if ((Get-Date) -ge $deadline) { return $null }
    Start-Sleep -Milliseconds 250
  }
}

# ==== 四类玩家动作阶段：证据评估（纯函数；live 与离线自检共用同一份实现）========
# 判据一律落在**可观察副作用**上（入包 SM_ADDITEM、金币 SM_GOLDCHANGED、怪物 SM_STRUCK/
# SM_DEATH、脚本说词/菜单帧且 body 非空），**不以「某帧出现」当成功**（5008/767/637 三例在案）。
# 协议口径（oracle，见 src/M2Server/Player/*.cs 与 src/GameSrv/Word/WorldServer.cs）：
#   CM_PICKUP(1001)      header param=ownX tag=ownY（服务端 `CurrX==nParam2 && CurrY==nParam3` 才处理，
#                        PlayObject.Message.cs CM_PICKUP）
#   CM_HIT(3014)         header recog=MakeLong(ownX,ownY) tag=Dir（服务端 ClientHitXY 要求 nX==CurrX；
#                        WorldServer.cs ProcessUserMessage 把 Tag→wParam、Recog→nParam1/2）
#   攻击目标解析         GetPoseCreate() = 角色**前方一格**（由 Dir 决定，BaseObject.cs GetFrontPosition）
#   SM_ADDITEM(200)      series=1；SM_BAGITEMS(201) series=ItemList.Count；SM_DELITEM(202) series=1
#   SM_GOLDCHANGED(653)  recog=当前 Gold（PlayObject.Message.cs:1834）
#   SM_STRUCK(31)        recog=受击者 param=HP(之后) tag=MaxHP series=本次伤害（:1601）
#   SM_DEATH(32)         recog=死亡者（:1644）
#   SM_LOGON(50)         recog=ActorId param=X tag=Y
#   CM_CLICKNPC(1010)    recog=NPC ActorId
#   SM_MERCHANTSAY(643) / SM_MENU_OK(767)：**body 非空**才说明脚本确实说了话/给了菜单。
#   CM_USERBUYITEM(1014) / CM_USERSELLITEM(1013)：买卖请求；成功帧 650 / 648。

function ConvertFrom-PixelSpec([string]$name, [string]$spec) {
  $p = ([string]$spec).Split(",")
  if ($p.Count -ne 2 -or -not (Test-TurnIntLiteral $p[0].Trim()) -or -not (Test-TurnIntLiteral $p[1].Trim())) {
    throw ("-" + $name + " 须为 'x,y'（两个整数），当前：'" + $spec + "'")
  }
  return ,@([int]$p[0].Trim(), [int]$p[1].Trim())
}

function Resolve-CombatTargetPixel([string]$explicitSpec, [int]$dx, [int]$dy) {
  # A1/F1：combat 用**显式像素**还是**48x32 格心投影**，必须由这**唯一一处**决定，
  # 并随证据/ stdout 记账 —— 真机 RED 时才能区分「靶子没刷出」与「未证实的投影点歪」。
  # Source ∈ explicit|projected（本函数不打印；调用方按各自输出格式记账）。
  if ([string]$explicitSpec -ne "") {
    $p = ConvertFrom-PixelSpec "CombatTargetPixel" $explicitSpec
    return [pscustomobject][ordered]@{ X = [int]$p[0]; Y = [int]$p[1]; Source = "explicit" }
  }
  return [pscustomobject][ordered]@{ X = [int](512 + $dx * 48); Y = [int](304 + $dy * 32); Source = "projected" }
}

function Get-StageFramesSorted($source, [long]$sinceMs, [long]$atMs) {
  # 只取 ts ∈ [sinceMs, atMs] 的帧，按 (ts, conn, off) 排序 —— 遍历顺序是**显式**的键，
  # 不依赖 Query/哈希顺序（确定性）。
  $rows = @((Get-EvidenceFrames $source) | Where-Object {
      [long]$_.ts_ms -ge $sinceMs -and [long]$_.ts_ms -le $atMs
    })
  return @($rows | Sort-Object @{ Expression = { [long]$_.ts_ms }; Ascending = $true },
                          @{ Expression = { [int]$_.conn }; Ascending = $true },
                          @{ Expression = { [long]$_.off }; Ascending = $true })
}

function Get-SelfStateAt($source, [long]$sinceMs, [long]$atMs) {
  # 与 Get-SelfCellEvidence 同口径，但**截到 atMs**（动作发生时刻）：SM_LOGON(50)/SM_NEWMAP(51)
  # 给出锚点；c2s CM_WALK(3011) 的 recog 是**目标格**，只接受切比雪夫 <=1 的下一步。
  $cell = $null
  $actor = 0
  $src = ""
  foreach ($f in (Get-StageFramesSorted $source $sinceMs $atMs)) {
    if ($f.dir -eq "s2c" -and ([int]$f.ident -eq 50 -or [int]$f.ident -eq 51)) {
      $actor = [int]$f.recog
      $cell = [pscustomobject]@{ X = [int]$f.param; Y = [int]$f.tag }
      $src = if ([int]$f.ident -eq 50) { "s2c SM_LOGON(50)" } else { "s2c SM_NEWMAP(51)" }
    } elseif ($f.dir -eq "c2s" -and [int]$f.ident -eq 3011 -and $null -ne $cell) {
      $tx = Get-TurnLoWord ([long]$f.recog)
      $ty = Get-TurnHiWord ([long]$f.recog)
      if ([Math]::Max([Math]::Abs($tx - [int]$cell.X), [Math]::Abs($ty - [int]$cell.Y)) -le 1) {
        $cell = [pscustomobject]@{ X = [int]$tx; Y = [int]$ty }
        $src = "c2s CM_WALK(3011)"
      }
    }
  }
  if ($null -eq $cell) { return $null }
  return [pscustomobject]@{ ActorId = $actor; X = [int]$cell.X; Y = [int]$cell.Y; Source = $src }
}

function Get-BagStateAt($source, [long]$sinceMs, [long]$atMs) {
  # 背包行数：SM_BAGITEMS(201).series 是登录/变更时的**全量行数**，SM_ADDITEM(200)/SM_DELITEM(202)
  # 各带 series（通常 1）。没有 201 基线时 Rows=$null（不拿 0 冒充已知）。
  $rows = $null
  $baseline = $false
  $adds = 0
  $dels = 0
  foreach ($f in (Get-StageFramesSorted $source $sinceMs $atMs)) {
    if ($f.dir -ne "s2c") { continue }
    switch ([int]$f.ident) {
      201 {
        $rows = [int]$f.series
        $baseline = $true
      }
      200 {
        $adds++
        if ($null -ne $rows) { $rows += [Math]::Max(1, [int]$f.series) }
      }
      202 {
        $dels++
        if ($null -ne $rows) {
          $rows -= [Math]::Max(1, [int]$f.series)
          if ($rows -lt 0) { $rows = 0 }
        }
      }
    }
  }
  return [pscustomobject]@{ Rows = $rows; Baseline = $baseline; Adds = $adds; Dels = $dels }
}

function Get-GoldAt($source, [long]$sinceMs, [long]$atMs) {
  # 金币基线有两个来源（都是 recog=Gold 的绝对值）：
  #   · s2c SM_ABILITY(52)      —— 登录 / 属性重算时下发（oracle PlayObject.Message.cs RM_ABILITY，
  #                                `MakeMessage(SM_ABILITY, Gold, MakeWord(Job,99), ...)`；实测登录帧在）；
  #   · s2c SM_GOLDCHANGED(653) —— 金币变化时下发。
  # 取窗口内**最后一条**，保证 before/after 都用同一口径。
  $gold = $null
  foreach ($f in (Get-StageFramesSorted $source $sinceMs $atMs)) {
    if ($f.dir -eq "s2c" -and ([int]$f.ident -eq 653 -or [int]$f.ident -eq 52)) { $gold = [long]$f.recog }
  }
  return $gold
}

function Read-MonsterFixtures([string]$path) {
  # 读 card-window profile 的 fixtures.monsters（schema 见 tools/conformance/card_window.py
  # `_validate_monsters`：map 非空 / x,y 0..65535 / name 非空可 GBK / count 1..100）。
  if ($path -eq "") { throw "combat 需要 -Profile（card-window profile）" }
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw ("profile 不存在：" + $path) }
  $doc = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $path).Path, [Text.Encoding]::UTF8) | ConvertFrom-Json
  $mon = $null
  if ($null -ne $doc.PSObject.Properties["fixtures"] -and $null -ne $doc.fixtures -and
      $null -ne $doc.fixtures.PSObject.Properties["monsters"]) {
    $mon = @($doc.fixtures.monsters)
  }
  if ($null -eq $mon) { throw ("profile 没有 fixtures.monsters：" + $path + "（有目标战斗必须先声明靶子）") }
  $out = New-Object System.Collections.ArrayList
  $i = 0
  foreach ($m in $mon) {
    $w = "fixtures.monsters[" + $i + "]"
    $i++
    foreach ($k in @("map", "x", "y", "name")) {
      if ($null -eq $m.PSObject.Properties[$k]) { throw ($w + " 缺字段 " + $k) }
    }
    $map = [string]$m.map
    if ($map.Trim() -eq "") { throw ($w + ".map 必须非空") }
    foreach ($k in @("x", "y")) {
      $v = $m.$k
      if ($null -eq $v -or [int]$v -lt 0 -or [int]$v -gt 65535) { throw ($w + "." + $k + " 越界") }
    }
    if ([string]$m.name -eq "") { throw ($w + ".name 必须非空") }
    $cnt = 1
    if ($null -ne $m.PSObject.Properties["count"] -and $null -ne $m.count) { $cnt = [int]$m.count }
    if ($cnt -lt 1 -or $cnt -gt 100) { throw ($w + ".count 越界") }
    [void]$out.Add([pscustomobject]@{ Map = $map; X = [int]$m.x; Y = [int]$m.y; Name = [string]$m.name; Count = $cnt })
  }
  return ,$out.ToArray()
}

function Get-TurnFrontCell([int]$x, [int]$y, [int]$dir) {
  # 服务端 GetFrontPosition（BaseObject.cs:977）的 8 方向；0=Up..7=UpLeft。
  $dx = 0
  $dy = 0
  switch ($dir) {
    0 { $dy = -1 }
    1 { $dx = 1; $dy = -1 }
    2 { $dx = 1 }
    3 { $dx = 1; $dy = 1 }
    4 { $dy = 1 }
    5 { $dx = -1; $dy = 1 }
    6 { $dx = -1 }
    7 { $dx = -1; $dy = -1 }
    default { throw ("非法方向：" + $dir) }
  }
  return [pscustomobject]@{ X = $x + $dx; Y = $y + $dy }
}

# ==== 成因三档（卡 openmir2-mirflow-verdict-cause-split）======================
# 纯函数；离线可测。kind -> rc 一一对应，且三档 rc 互不相同（A6）。
function Get-VerdictRc([string]$kind) {
  switch ($kind) {
    "ok"             { return 0 }
    "missing_prereq" { return 6 }
    "spawn_failed"   { return 7 }
    "out_of_range"   { return 8 }
    default          { return 1 }
  }
}

function Resolve-CombatCause([bool]$prereqMissing, [bool]$outOfRange, [bool]$spawnFailed) {
  # A6 的「三因」在这里汇合：三个输入由调用方**各自独立求值**，本函数只做优先级选择。
  # 显式优先级：missing_prereq > out_of_range > spawn_failed > ok。
  #   理由：① 没声明夹具就无从判后两档（UNDECIDABLE 优先，避免把「没测」记成 FAIL）；
  #         ② 声明了但够不着时，「没掉血」由距离解释，不能记成 spawn_failed（否则真机会改错地方）。
  # 单因子矩阵下每行恰好一个 true；两两组合按上面的优先级短路，结果见 selftest 的 8 行真值表。
  # 删掉任意一行只会改变「该行对应那一档」或让它回落到下一档/PASS，不会连带另外两档。
  if ($prereqMissing) { return "missing_prereq" }
  if ($outOfRange)    { return "out_of_range" }
  if ($spawnFailed)   { return "spawn_failed" }
  return "ok"
}

function Set-EvVerdict($ev, [string]$kind, [string]$missing, [string]$reason) {
  # 统一写 verdict/kind/rc/missing/reason，避免四处各自拼字符串漂移。
  $ev.verdict_kind = $kind
  $ev.rc = Get-VerdictRc $kind
  switch ($kind) {
    "ok"             { $ev.verdict = "GREEN"; $ev.ok = $true }
    "missing_prereq" { $ev.verdict = "UNDECIDABLE"; $ev.ok = $false }
    default          { $ev.verdict = "RED"; $ev.ok = $false }
  }
  if ($missing -eq "") { $ev.missing = @() } else { $ev.missing = @($missing) }
  $ev.reason = $reason
}

function New-PrereqGate {
  # 前置门的可机判载体。source=none => 未提供 provision --gate 产物。
  return [ordered]@{
    source = "none"; path = ""; kind = ""; verdict = ""
    need = @(); missing = @(); reasons = [ordered]@{}
  }
}

function Read-PrereqGateJson([string]$path) {
  # 只读对接 `provision_real_machine_fixtures.py --gate` 的 JSON（schema 见该脚本 prereq_gate）。
  # 不连库、不改它；只读白名单字段 need/missing/reasons/kind/verdict。
  $gate = New-PrereqGate
  if ($path -eq "") { return $gate }
  $doc = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $path).Path, [Text.Encoding]::UTF8) | ConvertFrom-Json
  $gate.source = "provision_gate_json"
  $gate.path = $path
  if ($null -ne $doc.PSObject.Properties["kind"]) { $gate.kind = [string]$doc.kind }
  if ($null -ne $doc.PSObject.Properties["verdict"]) { $gate.verdict = [string]$doc.verdict }
  if ($null -ne $doc.PSObject.Properties["need"]) { $gate.need = @($doc.need | ForEach-Object { [string]$_ }) }
  if ($null -ne $doc.PSObject.Properties["missing"]) { $gate.missing = @($doc.missing | ForEach-Object { [string]$_ }) }
  if ($null -ne $doc.PSObject.Properties["reasons"]) {
    foreach ($p in $doc.reasons.PSObject.Properties) { $gate.reasons[$p.Name] = [string]$p.Value }
  }
  return $gate
}

function Test-PrereqMissing($gate, [string]$item) {
  # 只有 provision --gate 明确点名才算「缺」；未提供 gate => 不臆断（返回 $false）。
  if ($null -eq $gate) { return $false }
  return (@($gate.missing) -contains $item)
}

function Get-FirstMissingPrereq($gate, [string[]]$items) {
  foreach ($it in $items) { if (Test-PrereqMissing $gate $it) { return $it } }
  return ""
}

function Test-FloorItemMissing($gate) {
  # 地面物：96ad82a37 裁决「结构性不可达」=> 默认恒缺（显式 missing_prereq(structural)）。
  # 只有 gate **明确查过** floor_item 且未列 missing（罕见，需新证据推翻裁决）才视为备齐。
  if ($null -eq $gate -or $gate.source -eq "none") { return $true }
  if (@($gate.missing) -contains "floor_item") { return $true }
  if (@($gate.need) -contains "floor_item") { return $false }
  return $true
}

function Get-MissingText($missing) {
  if ($null -eq $missing) { return "" }
  return (@($missing) -join ",")
}

function Get-RcAggregate([int[]]$rcs) {
  # 一次跑多类时取最具体/最严重的一档；顺序即优先级：
  #   out_of_range(8) > spawn_failed(7) > 其它 RED(1) > UNDECIDABLE(6) > PASS(0)。
  # 理由：只要有一类「该发生却没发生」，整体就不能算「判不了」。
  if ($null -eq $rcs) { return 0 }
  foreach ($want in @(8, 7, 1, 6)) {
    if (@($rcs) -contains $want) { return $want }
  }
  return 0
}

function Stop-Usage([string]$msg) {
  # A7：用法错固定 rc=2（与任何 verdict 不同码）。
  Write-Output ("MIRFLOW USAGE: " + $msg + "（用法错 rc=2，不是任何 verdict）")
  exit 2
}

function Stop-StageEvidence($ev) {
  # live 路径：证据判失败时按该类的 rc 退出（不再一律 throw => rc=1）。
  Write-Output ("STAGE_VERDICT category=" + $ev.category + " verdict=" + $ev.verdict + " kind=" + $ev.verdict_kind +
    " rc=" + $ev.rc + " missing=" + (Get-MissingText $ev.missing))
  exit ([int]$ev.rc)
}

function Get-PickupEvidence($source, [long]$sinceMs, [long]$actionFloorMs, [long]$atMs, $prereqGate = $null) {
  # 前提：客户端发出 CM_PICKUP(1001) 且目标格 == 角色格（否则服务端直接 return，什么都不会发生）。
  # 判据：动作之后出现 SM_ADDITEM(200)（背包行数**增加**）或 SM_GOLDCHANGED(653)（金币变化）。
  $ev = [ordered]@{
    category = "pickup"; verdict = "RED"; verdict_kind = "no_effect"; ok = $false; rc = 1; reason = ""; missing = @()
    action = $null; before = [ordered]@{}; after = [ordered]@{}; effect = $null
  }
  $picks = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "c2s" -and [int]$_.ident -eq 1001 -and [long]$_.ts_ms -ge $actionFloorMs
    })
  if ($picks.Count -eq 0) { Set-EvVerdict $ev "action_missing" "" "无 c2s CM_PICKUP(1001)（未发出取物请求）"; return $ev }
  $pick = $picks[$picks.Count - 1]
  $self = Get-SelfStateAt $source $sinceMs ([long]$pick.ts_ms)
  $ev.action = [ordered]@{ ident = 1001; ts_ms = [long]$pick.ts_ms; param_x = [int]$pick.param; param_y = [int]$pick.tag; own = $null }
  if ($null -eq $self) { Set-EvVerdict $ev "no_position" "" "无自身坐标证据（SM_LOGON/51），无法核对 CM_PICKUP 目标格"; return $ev }
  $ev.action.own = [ordered]@{ actor_id = [int]$self.ActorId; x = [int]$self.X; y = [int]$self.Y; source = $self.Source }
  if ([int]$pick.param -ne [int]$self.X -or [int]$pick.tag -ne [int]$self.Y) {
    Set-EvVerdict $ev "wrong_cell" "" ("CM_PICKUP 目标格 (" + [int]$pick.param + "," + [int]$pick.tag + ") != 角色格 (" +
      [int]$self.X + "," + [int]$self.Y + ")（服务端 oracle 会直接忽略）")
    return $ev
  }
  $b0 = Get-BagStateAt $source $sinceMs ([long]$pick.ts_ms)
  $b1 = Get-BagStateAt $source $sinceMs $atMs
  $g0 = Get-GoldAt $source $sinceMs ([long]$pick.ts_ms)
  $g1 = Get-GoldAt $source $sinceMs $atMs
  $ev.before = [ordered]@{ bag_rows = $b0.Rows; bag_baseline = $b0.Baseline; gold = $g0 }
  $ev.after = [ordered]@{ bag_rows = $b1.Rows; bag_baseline = $b1.Baseline; gold = $g1 }

  $adds = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "s2c" -and [int]$_.ident -eq 200 -and [long]$_.ts_ms -ge [long]$pick.ts_ms
    })
  $goldFrames = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "s2c" -and [int]$_.ident -eq 653 -and [long]$_.ts_ms -ge [long]$pick.ts_ms
    })
  $goldChanged = ($null -ne $g0 -and $null -ne $g1 -and [long]$g0 -ne [long]$g1)
  if ($adds.Count -eq 0 -and -not $goldChanged) {
    if (Test-FloorItemMissing $prereqGate) {
      $why = ""
      if ($null -ne $prereqGate -and $null -ne $prereqGate.reasons -and
          $prereqGate.reasons.Contains("floor_item")) {
        $why = "：" + [string]$prereqGate.reasons["floor_item"]
      }
      Set-EvVerdict $ev "missing_prereq" "floor_item" (
        "取物后无入包也无金币变化，且地面物前置缺失（floor_item；96ad82a37 裁决：结构性不可达）=> 本轮不可判" + $why)
      return $ev
    }
    Set-EvVerdict $ev "no_effect" "" "取物后无入包（SM_ADDITEM 200）也无金币变化（SM_GOLDCHANGED 653）"
    return $ev
  }
  if ($adds.Count -gt 0) {
    if ($b0.Baseline -and $null -ne $b0.Rows -and $null -ne $b1.Rows -and [int]$b1.Rows -le [int]$b0.Rows) {
      Set-EvVerdict $ev "bag_add_no_rows" "" ("有 SM_ADDITEM 但背包行数未增加（before=" + $b0.Rows + " after=" + $b1.Rows + "）")
      return $ev
    }
    $ev.effect = [ordered]@{ kind = "bag_add"; ident = 200; ts_ms = [long]$adds[0].ts_ms; count = $adds.Count }
  } else {
    $ev.effect = [ordered]@{ kind = "gold_changed"; ident = 653; ts_ms = [long]$goldFrames[0].ts_ms; delta = [long]$g1 - [long]$g0 }
  }
  Set-EvVerdict $ev "ok" "" "取物副作用成立"
  return $ev
}

function Get-NpcDialogEvidence($source, [long]$sinceMs, [long]$actionFloorMs, [long]$atMs, $prereqGate = $null) {
  # 前提：客户端确实发出 CM_CLICKNPC(1010)（点 NPC 的前置动作）。
  # 判据：点击之后出现 **body 非空** 的 SM_MERCHANTSAY(643)（脚本说词）或 SM_MENU_OK(767)（菜单文本）。
  #       767 本身「成功/失败共用」（不能当成功帧），这里只把它当**对话内容帧**：
  #       body 非空 = 服务端确实给了脚本说词/菜单，而不是空回帧。
  $ev = [ordered]@{
    category = "npcdialog"; verdict = "RED"; verdict_kind = "no_effect"; ok = $false; rc = 1; reason = ""; missing = @()
    click = $null; dialogue = [ordered]@{ say_frames = 0; menu_frames = 0; max_body_len = 0; first_ts_ms = $null }
  }
  $clicks = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "c2s" -and [int]$_.ident -eq 1010 -and [long]$_.ts_ms -ge $actionFloorMs
    })
  if ($clicks.Count -eq 0) {
    if (Test-PrereqMissing $prereqGate "npc") {
      Set-EvVerdict $ev "missing_prereq" "npc" "点 NPC 前置缺失（npc）=> 不可判：未发出 c2s CM_CLICKNPC(1010)"
    } else {
      Set-EvVerdict $ev "action_missing" "" "未发出 c2s CM_CLICKNPC(1010)（点 NPC 前置动作缺失）"
    }
    return $ev
  }
  $click = $clicks[$clicks.Count - 1]
  $ev.click = [ordered]@{ ident = 1010; ts_ms = [long]$click.ts_ms; npc_actor = [int]$click.recog }
  $dlg = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "s2c" -and ([int]$_.ident -eq 643 -or [int]$_.ident -eq 767) -and
      [int]$_.body_len -gt 0 -and [long]$_.ts_ms -ge [long]$click.ts_ms
    })
  if ($dlg.Count -eq 0) {
    if (Test-PrereqMissing $prereqGate "npc") {
      Set-EvVerdict $ev "missing_prereq" "npc" "NPC 前置缺失（npc）=> 不可判：CM_CLICKNPC 后无脚本说词/菜单帧（643/767 且 body_len>0）"
    } else {
      Set-EvVerdict $ev "no_dialogue" "" "CM_CLICKNPC 后无脚本说词/菜单帧（643/767 且 body_len>0）"
    }
    return $ev
  }
  $say = 0
  $menu = 0
  $maxBody = 0
  $firstTs = [long]::MaxValue
  foreach ($f in $dlg) {
    if ([int]$f.ident -eq 643) { $say++ } else { $menu++ }
    if ([int]$f.body_len -gt $maxBody) { $maxBody = [int]$f.body_len }
    if ([long]$f.ts_ms -lt $firstTs) { $firstTs = [long]$f.ts_ms }
  }
  $ev.dialogue = [ordered]@{ say_frames = $say; menu_frames = $menu; max_body_len = $maxBody; first_ts_ms = $firstTs }
  Set-EvVerdict $ev "ok" "" "点 NPC 后出现脚本说词/菜单帧"
  return $ev
}

function Get-TradeEvidence($source, [long]$sinceMs, [long]$actionFloorMs, [string]$kind, [int]$dialogMaxAgeSec, [long]$atMs, $prereqGate = $null) {
  # 前提（A3 牙齿）：买卖请求之前**必须**先打开商人对话 —— c2s CM_CLICKNPC(1010)/CM_MERCHANTDLGSELECT(1011)
  #        且 s2c 643/767 body 非空，二者都在 dialogMaxAgeSec 窗口内。
  # 判据：成功帧（买 650 / 卖 648）+ 金币按方向变化（买减少 / 卖增加）。
  $ev = [ordered]@{
    category = "trade"; verdict = "RED"; verdict_kind = "no_effect"; ok = $false; rc = 1; reason = ""; missing = @()
    kind = $kind; request = $null; dialogue = [ordered]@{ opens = 0; dialogue_frames = 0 }
    success = $null; before = [ordered]@{}; after = [ordered]@{}; gold_delta = $null; item_effect = $null
  }
  # 动作前前置门（A2）：买要金币、卖要可卖物品，两者都要可交易 NPC。门不过且没生效 => UNDECIDABLE。
  $prereqItems = @()
  if ($kind -eq "sell") { $prereqItems += "sellable_item" } else { $prereqItems += "gold" }
  $prereqItems += "npc"
  $prereqMiss = Get-FirstMissingPrereq $prereqGate $prereqItems
  $reqs = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "c2s" -and ([int]$_.ident -eq 1014 -or [int]$_.ident -eq 1013) -and [long]$_.ts_ms -ge $actionFloorMs
    })
  if ($reqs.Count -eq 0) {
    if ($prereqMiss -ne "") {
      Set-EvVerdict $ev "missing_prereq" $prereqMiss ("前置缺失（" + $prereqMiss + "）=> 不可判：无买卖请求（CM_USERBUYITEM 1014 / CM_USERSELLITEM 1013）")
    } else {
      Set-EvVerdict $ev "action_missing" "" "无买卖请求（CM_USERBUYITEM 1014 / CM_USERSELLITEM 1013）"
    }
    return $ev
  }
  $req = $reqs[$reqs.Count - 1]
  $reqIdent = [int]$req.ident
  $wantIdent = if ($kind -eq "sell") { 1013 } else { 1014 }
  $ev.request = [ordered]@{ ident = $reqIdent; ts_ms = [long]$req.ts_ms }
  if ($reqIdent -ne $wantIdent) {
    Set-EvVerdict $ev "kind_mismatch" "" ("交易请求 ident=" + $reqIdent + " 与 -TradeKind=" + $kind + " 不符")
    return $ev
  }
  $maxAgeMs = [long]$dialogMaxAgeSec * 1000
  $lo = [Math]::Max([long]$sinceMs, [long]$req.ts_ms - $maxAgeMs)
  $opens = @(Get-StageFramesSorted $source $sinceMs ([long]$req.ts_ms) | Where-Object {
      $_.dir -eq "c2s" -and ([int]$_.ident -eq 1010 -or [int]$_.ident -eq 1011) -and [long]$_.ts_ms -ge $lo
    })
  $dlgs = @(Get-StageFramesSorted $source $sinceMs ([long]$req.ts_ms) | Where-Object {
      $_.dir -eq "s2c" -and ([int]$_.ident -eq 643 -or [int]$_.ident -eq 767) -and
      [int]$_.body_len -gt 0 -and [long]$_.ts_ms -ge $lo
    })
  $ev.dialogue = [ordered]@{ opens = $opens.Count; dialogue_frames = $dlgs.Count; window_ms = $maxAgeMs }
  if ($opens.Count -eq 0 -or $dlgs.Count -eq 0) {
    if ($prereqMiss -ne "") {
      Set-EvVerdict $ev "missing_prereq" $prereqMiss ("前置缺失（" + $prereqMiss + "）=> 不可判：未先打开商人对话（不点 NPC 直接交易 / 无脚本说词帧）")
    } else {
      Set-EvVerdict $ev "no_dialogue" "" "未先打开商人对话（前置缺失：不点 NPC 直接交易 / 无脚本说词帧）"
    }
    return $ev
  }
  $successIdent = if ($kind -eq "sell") { 648 } else { 650 }
  $success = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "s2c" -and [int]$_.ident -eq $successIdent -and [long]$_.ts_ms -ge [long]$req.ts_ms
    })
  if ($success.Count -eq 0) {
    if ($prereqMiss -ne "") {
      Set-EvVerdict $ev "missing_prereq" $prereqMiss ("前置缺失（" + $prereqMiss + "）=> 不可判：无交易成功帧（ident=" + $successIdent + "）")
    } else {
      Set-EvVerdict $ev "no_success" "" ("无交易成功帧（ident=" + $successIdent + "）")
    }
    return $ev
  }
  $ev.success = [ordered]@{ ident = $successIdent; ts_ms = [long]$success[0].ts_ms }
  $g0 = Get-GoldAt $source $sinceMs ([long]$req.ts_ms)
  $g1 = Get-GoldAt $source $sinceMs $atMs
  $ev.before = [ordered]@{ gold = $g0 }
  $ev.after = [ordered]@{ gold = $g1 }
  if ($null -eq $g0 -or $null -eq $g1) {
    if ($prereqMiss -ne "") {
      Set-EvVerdict $ev "missing_prereq" $prereqMiss ("前置缺失（" + $prereqMiss + "）=> 不可判：缺金币证据（SM_GOLDCHANGED 653 基线或变化）")
    } else {
      Set-EvVerdict $ev "no_gold_evidence" "" "缺金币证据（SM_GOLDCHANGED 653 基线或变化）"
    }
    return $ev
  }
  $delta = [long]$g1 - [long]$g0
  $ev.gold_delta = $delta
  if ($kind -eq "buy" -and $delta -ge 0) {
    if ($prereqMiss -ne "") {
      Set-EvVerdict $ev "missing_prereq" $prereqMiss ("前置缺失（" + $prereqMiss + "）=> 不可判：买后金币未减少（before=" + $g0 + " after=" + $g1 + "）")
    } else {
      Set-EvVerdict $ev "no_gold_change" "" ("买后金币未减少（before=" + $g0 + " after=" + $g1 + "）")
    }
    return $ev
  }
  if ($kind -eq "sell" -and $delta -le 0) {
    if ($prereqMiss -ne "") {
      Set-EvVerdict $ev "missing_prereq" $prereqMiss ("前置缺失（" + $prereqMiss + "）=> 不可判：卖后金币未增加（before=" + $g0 + " after=" + $g1 + "）")
    } else {
      Set-EvVerdict $ev "no_gold_change" "" ("卖后金币未增加（before=" + $g0 + " after=" + $g1 + "）")
    }
    return $ev
  }
  $itemIdent = if ($kind -eq "sell") { 202 } else { 200 }
  $items = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
      $_.dir -eq "s2c" -and [int]$_.ident -eq $itemIdent -and [long]$_.ts_ms -ge [long]$req.ts_ms
    })
  $ev.item_effect = [ordered]@{ ident = $itemIdent; count = $items.Count }
  Set-EvVerdict $ev "ok" "" "商人对白前置成立 + 成功帧 + 金币按方向变化"
  return $ev
}

function Get-CombatEvidence($source, [long]$sinceMs, [long]$actionFloorMs, [string]$profile, [string]$playerMap, [int]$rangeCells, [long]$atMs, [string]$explicitPixelSpec = "") {
  # 前提（A2/A3）：profile 的 fixtures.monsters 必须有**同图**靶子（刷不出即 RED，不静默跳过）；
  #   且这次 CM_HIT 的 Dir 必须让「前方一格」正好落在某个同图靶子上（服务端 GetPoseCreate 解析），
  #   距离必须 <= AP 范围（默认 1 格近战）。
  # 判据：CM_HIT 之后，**非自己**的 actor 出现 SM_STRUCK（HP 下降 / 伤害>0）或 SM_DEATH。
  #
  # 成因三档（卡 openmir2-mirflow-verdict-cause-split）：下面是三个**独立谓词** + 一个显式分类点。
  #   $prereqMissing : 夹具/配置没备齐（能不能测）
  #   $outOfRange    : 已声明但不在 AP / 不同图
  #   $spawnFailed   : 已声明且在 AP 且被指向，但没有副作用
  # 优先级（Resolve-CombatCause，显式）：missing_prereq > out_of_range > spawn_failed > ok。
  # 单因子矩阵下每行恰好一个谓词为真；删掉任一谓词的赋值只会让该行回落到下一档或 PASS。
  $ev = [ordered]@{
    category = "combat"; verdict = "RED"; verdict_kind = "no_effect"; ok = $false; rc = 1; reason = ""; missing = @()
    profile = $profile; player_map = $playerMap; range_cells = $rangeCells
    target = $null; hit = $null; effect = $null
    hp_before = $null; hp_after = $null; max_hp = $null; damage = $null; actor = $null
    # A1/F1：这次 combat 用的是显式像素还是 48x32 投影（走到靶子匹配后回填；live 侧
    # 由调用方用**实际点击**的像素覆盖）。unknown = 没走到靶子匹配，无法给出。
    pixel_source = "unknown"; pixel = $null
  }
  $prereqMissing = $false   # 谓词 1：夹具/配置没备齐
  $outOfRange = $false      # 谓词 3：已声明但不在 AP / 不同图
  $spawnFailed = $false     # 谓词 2：已声明且在 AP 且被指向，但没有副作用
  $aimMismatch = $false     # 非三档：AP 内有靶子但攻击方向不对（确定性驱动错误）
  $prereqName = ""
  $prereqReason = ""
  $outOfRangeReason = ""
  $spawnFailedReason = ""
  $aimMismatchReason = ""
  $target = $null
  $fx = $null

  # ---- 谓词 1（独立）：前置 / 声明 ----
  if ($playerMap -eq "") {
    $prereqMissing = $true; $prereqName = "monster_fixture"
    $prereqReason = "-PlayerMap 未给出（经典协议线上没有图名，无法证明靶子同图）=> 不可判"
  } else {
    # 注意：Read-MonsterFixtures 用 `return ,$arr` 保住空数组；这里**不能再套 @()**，
    # 否则会变成「数组套数组」——多靶子时会退化成 1 个元素（Count=1 / 成员枚举假象）。
    try { $fx = Read-MonsterFixtures $profile }
    catch {
      $prereqMissing = $true; $prereqName = "monster_fixture"
      $prereqReason = "夹具声明缺失：profile/fixtures.monsters 读不出或未声明（" + $_.Exception.Message + "）=> 不可判"
    }
    if (-not $prereqMissing -and ($null -eq $fx -or @($fx).Count -eq 0)) {
      $prereqMissing = $true; $prereqName = "monster_fixture"
      $prereqReason = "夹具声明缺失：fixtures.monsters 为空（没有声明任何靶子）=> 不可判"
    }
  }

  $same = @()
  if ($null -ne $fx) { $same = @($fx | Where-Object { $_.Map -ieq $playerMap }) }

  # ---- 谓词 3a（独立）：已声明但不在本图（不同图）----
  # 只在「确有声明」时成立：没有任何声明属于谓词 1，不能在这里顺带算成 out_of_range。
  if (-not $prereqMissing -and $null -ne $fx -and @($fx).Count -gt 0 -and $same.Count -eq 0) {
    $outOfRange = $true
    $maps = (@($fx | ForEach-Object { [string]$_.Map } | Sort-Object -Unique) -join ",")
    $outOfRangeReason = "靶子已声明但只在 map=" + $maps + "，与 -PlayerMap=" + $playerMap + " 不同图（刷出也不在可测位置）"
  }

  # ---- 动作 + 谓词 2/3b：CM_HIT 之后才能判「刷不出」或「AP 外」 ----
  if (-not $prereqMissing -and -not $outOfRange) {
    $hits = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
        $_.dir -eq "c2s" -and [int]$_.ident -eq 3014 -and [long]$_.ts_ms -ge $actionFloorMs
      })
    if ($hits.Count -eq 0) {
      Set-EvVerdict $ev "action_missing" "" "无 c2s CM_HIT(3014)（攻击未发出）"
      return $ev
    }
    $hit = $hits[$hits.Count - 1]
    $ownX = Get-TurnLoWord ([long]$hit.recog)
    $ownY = Get-TurnHiWord ([long]$hit.recog)
    $dir = [int]$hit.tag
    $front = Get-TurnFrontCell $ownX $ownY $dir
    $ev.hit = [ordered]@{ ident = 3014; ts_ms = [long]$hit.ts_ms; own_x = $ownX; own_y = $ownY; dir = $dir; front_x = [int]$front.X; front_y = [int]$front.Y }
    $match = @($same | Where-Object { [int]$_.X -eq [int]$front.X -and [int]$_.Y -eq [int]$front.Y })

    if ($match.Count -eq 0) {
      # 前方格没有靶子：先分「AP 外」与「方向不对」。
      $nearest = $null
      $nearestDist = [int]::MaxValue
      foreach ($m in $same) {
        $md = [Math]::Max([Math]::Abs([int]$m.X - $ownX), [Math]::Abs([int]$m.Y - $ownY))
        if ($md -lt $nearestDist) { $nearestDist = $md; $nearest = $m }
      }
      if ($null -ne $nearest) {
        $ev.target = [ordered]@{ map = $nearest.Map; x = [int]$nearest.X; y = [int]$nearest.Y; name = $nearest.Name; count = $nearest.Count; distance = $nearestDist }
      }
      if ($null -ne $nearest -and $nearestDist -gt $rangeCells) {
        $outOfRange = $true
        $outOfRangeReason = "靶子已声明但在 AP 外：最近同图靶子 (" + $nearest.X + "," + $nearest.Y + ") 距角色 " + $nearestDist + " 格 > AP " + $rangeCells + " 格（CM_HIT 前方格 " + $front.X + "," + $front.Y + "）"
      } elseif ($null -ne $nearest) {
        $aimMismatch = $true
        $aimMismatchReason = "CM_HIT 朝向 dir=" + $dir + " 的前方格 (" + $front.X + "," + $front.Y + ") 与同图靶子不匹配（攻击未指向靶子）"
      }
    } else {
      $target = $match[0]
      $dist = [Math]::Max([Math]::Abs([int]$target.X - $ownX), [Math]::Abs([int]$target.Y - $ownY))
      $ev.target = [ordered]@{ map = $target.Map; x = [int]$target.X; y = [int]$target.Y; name = $target.Name; count = [int]$target.Count; distance = $dist }
      if ($dist -gt $rangeCells) {
        $outOfRange = $true
        $outOfRangeReason = "靶子距角色 " + $dist + " 格 > AP 范围 " + $rangeCells
      } else {
        # A1/F1：像素决策与 live 路径共用 Resolve-CombatTargetPixel（check 模式按 CM_HIT 自身坐标回放；
        # live 模式以实际点击像素为准，调用方会再用 click 的 X/Y 覆盖这两个字段）。
        $pix = Resolve-CombatTargetPixel $explicitPixelSpec ([int]$target.X - $ownX) ([int]$target.Y - $ownY)
        $ev.pixel_source = $pix.Source
        $ev.pixel = [ordered]@{ x = [int]$pix.X; y = [int]$pix.Y }
        $selfActor = 0
        foreach ($f in (Get-StageFramesSorted $source $sinceMs ([long]$hit.ts_ms))) {
          if ($f.dir -eq "s2c" -and [int]$f.ident -eq 50) { $selfActor = [int]$f.recog }
        }
        $struck = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
            $_.dir -eq "s2c" -and [int]$_.ident -eq 31 -and [long]$_.ts_ms -ge [long]$hit.ts_ms -and
            ($selfActor -eq 0 -or [int]$_.recog -ne $selfActor) -and
            ([long]$_.series -gt 0 -or [int]$_.param -lt [int]$_.tag)
          })
        $death = @(Get-StageFramesSorted $source $sinceMs $atMs | Where-Object {
            $_.dir -eq "s2c" -and [int]$_.ident -eq 32 -and [long]$_.ts_ms -ge [long]$hit.ts_ms -and
            ($selfActor -eq 0 -or [int]$_.recog -ne $selfActor)
          })
        if ($struck.Count -eq 0 -and $death.Count -eq 0) {
          $spawnFailed = $true
          $spawnFailedReason = "夹具已声明在 (" + $target.X + "," + $target.Y + ") 却无掉血(SM_STRUCK 31)也无死亡(SM_DEATH 32)（声明了却刷不出）"
        } else {
          $useDeath = ($death.Count -gt 0 -and ($struck.Count -eq 0 -or [long]$death[0].ts_ms -le [long]$struck[0].ts_ms))
          if ($useDeath) {
            $ev.effect = "death"
            $ev.actor = [int]$death[0].recog
            $ev.hp_after = 0
            $ev.hp_before = $null
            $ev.damage = $null
            $ev.effect_ts_ms = [long]$death[0].ts_ms
          } else {
            $s = $struck[0]
            $ev.effect = "hp"
            $ev.actor = [int]$s.recog
            $ev.hp_after = [int]$s.param
            $ev.max_hp = [int]$s.tag
            $ev.damage = [int]$s.series
            $ev.hp_before = [int]$s.param + [int]$s.series
            $ev.effect_ts_ms = [long]$s.ts_ms
          }
        }
      }
    }
  }

  # ---- 单一分类点：三个谓词 -> Resolve-CombatCause（显式优先级）----
  $cause = Resolve-CombatCause $prereqMissing $outOfRange $spawnFailed
  switch ($cause) {
    "missing_prereq" { Set-EvVerdict $ev "missing_prereq" $prereqName $prereqReason; return $ev }
    "out_of_range"   { Set-EvVerdict $ev "out_of_range" "" $outOfRangeReason; return $ev }
    "spawn_failed"   { Set-EvVerdict $ev "spawn_failed" "" $spawnFailedReason; return $ev }
  }
  if ($aimMismatch) { Set-EvVerdict $ev "aim_mismatch" "" $aimMismatchReason; return $ev }
  Set-EvVerdict $ev "ok" "" "靶子同图且在 AP 内 + 攻击指向靶子 + 目标掉血/死亡"
  return $ev
}

function Get-LogEvidence([string]$dir, [bool]$require) {
  # A6：日志目录**参数化**（脚本内不出现任何硬编码日志路径）。
  #   未给 -LogDir            ⇒ SKIP（只靠协议证据）并写明原因；
  #   给了但目录不存在/0 个 *.log ⇒ 默认 SKIP 并说明；-RequireLogEvidence 时 RED（空扫描不得当绿）；
  #   有日志 ⇒ 扫 [ERR]/panic（同时报 files_scanned/lines_scanned，防止「扫了 0 个」假绿）。
  $r = [ordered]@{
    schema = "openmir2.mirflow-log-evidence/1"; status = "SKIP"; dir = $dir
    files_scanned = 0; lines_scanned = 0; err_lines = 0; samples = @(); reason = ""
  }
  if ($dir -eq "") {
    $r.reason = "-LogDir 未提供（阶段脚本不硬编码日志路径；本判据只用协议证据）"
    return $r
  }
  if (-not (Test-Path -LiteralPath $dir -PathType Container)) {
    if ($require) { $r.status = "RED"; $r.reason = ("日志目录不存在：" + $dir) }
    else { $r.reason = ("日志目录不存在（SKIP）：" + $dir) }
    return $r
  }
  $files = @(Get-ChildItem -LiteralPath $dir -File -Filter "*.log" -ErrorAction SilentlyContinue)
  if ($files.Count -eq 0) {
    if ($require) {
      $r.status = "RED"
      $r.reason = ("空日志目录：0 个 *.log（扫了 0 个对象，不得当绿）：" + $dir)
    } else {
      $r.reason = ("空日志目录：0 个 *.log（SKIP，未使用日志证据）：" + $dir)
    }
    return $r
  }
  $err = 0
  $lines = 0
  foreach ($f in $files) {
    foreach ($line in [IO.File]::ReadLines($f.FullName)) {
      $lines++
      if ($line -match '\[ERR\]|panic') {
        $err++
        if ($r.samples.Count -lt 5) { $r.samples += ($f.Name + ": " + $line) }
      }
    }
  }
  $r.files_scanned = $files.Count
  $r.lines_scanned = $lines
  $r.err_lines = $err
  if ($err -gt 0) {
    $r.status = "RED"
    $r.reason = ("日志出现 " + $err + " 行 [ERR]/panic（扫了 " + $files.Count + " 个文件 / " + $lines + " 行）")
  } else {
    $r.status = "GREEN"
    $r.reason = ("日志无 [ERR]/panic（扫了 " + $files.Count + " 个文件 / " + $lines + " 行）")
  }
  return $r
}

function Add-StageEvidence([string]$category, $ev) {
  $script:stageEvidence[$category] = $ev
  Write-Output ("STAGE_EVIDENCE category=" + $category + " verdict=" + $ev.verdict +
    " kind=" + $ev.verdict_kind + " rc=" + $ev.rc + " missing=" + (Get-MissingText $ev.missing) + " reason=" + $ev.reason)
}

function Write-StageEvidenceFile([string]$mode, $source, [string]$profilePath, [string]$playerMap, [int]$rangeCells) {
  $doc = [ordered]@{
    schema = "openmir2.mirflow-stages/1"
    created_utc = [DateTimeOffset]::UtcNow.ToString("o")
    mode = $mode
    verdict_model = [ordered]@{
      cause_driven = $true
      kinds = @("missing_prereq", "spawn_failed", "out_of_range")
      rc = [ordered]@{ pass = 0; red = 1; usage = 2; missing_prereq = 6; spawn_failed = 7; out_of_range = 8 }
    }
    evidence_source = [ordered]@{ kind = $source.Kind; path = $source.Path; since_ms = [long]$source.SinceMs }
    profile = [ordered]@{ path = $profilePath; player_map = $playerMap; range_cells = $rangeCells }
    log_evidence = $script:logEvidence
    categories = $script:stageEvidence
  }
  [IO.File]::WriteAllText($EvidenceOut, (($doc | ConvertTo-Json -Depth 12) + "`r`n"), (New-Object Text.UTF8Encoding($false)))
  Write-Output ("STAGE_EVIDENCE_FILE " + $EvidenceOut)
}

function Invoke-NoticePollCore {
  # F4：公告控制流只有这一份实现；真机与 -NoticePollSim 都走它（sim 只注入 Click/Verify/Park）。
  # 约束：最多点 MaxClicks 次（本项目只允许 1）；公告不可见时一次也不点；点后只校验，不补点。
  param(
    [bool]$NoticeVisible,
    [int]$MaxClicks,
    [scriptblock]$ClickAction,
    [scriptblock]$VerifyDismiss,
    [scriptblock]$ParkAction
  )
  $clicks = 0
  $parked = $false
  $dismissed = $false
  $reason = ""
  if (-not $NoticeVisible) {
    if ($ParkAction) { & $ParkAction; $parked = $true }
    return [pscustomobject][ordered]@{ Clicks = 0; WorldClicks = 0; Parked = $parked; Dismissed = $false; Reason = "no_notice_visible" }
  }
  if ($MaxClicks -lt 1) {
    if ($ParkAction) { & $ParkAction; $parked = $true }
    return [pscustomobject][ordered]@{ Clicks = 0; WorldClicks = 0; Parked = $parked; Dismissed = $false; Reason = "click_cap_zero" }
  }
  & $ClickAction
  $clicks = 1
  if ($ParkAction) { & $ParkAction; $parked = $true }
  $d = & $VerifyDismiss $clicks
  if ($d -eq $true) { $dismissed = $true; $reason = "notice_clicked_once" }
  elseif ($null -eq $d) { $reason = "unverified_no_evidence" }
  elseif ($clicks -ge $MaxClicks) { $reason = "click_cap_reached" }
  else { $reason = "click_not_dismissed" }
  return [pscustomobject][ordered]@{ Clicks = $clicks; WorldClicks = 0; Parked = $parked; Dismissed = $dismissed; Reason = $reason }
}

function Invoke-NoticePollSim([string]$mode, [int]$maxClicks) {
  # 离线模拟：只断言控制流形状（与真机共用 Invoke-NoticePollCore），不作为行为证据。
  #   dismiss1：公告可见，点 1 次即消失
  #   slow    ：公告要 3 次才消失 ⇒ 上限 1 只允许点 1 次，不补点
  #   absent  ：公告始终不出现（证据源超时）⇒ 0 次点击
  #   nosignal：没有客户端侧信号（无证据源且窗口/探针都没看到公告）⇒ 0 次点击 + UNVERIFIED
  $visible = $false; $afterClickDismissed = $false; $reasonOverride = ""
  if ($mode -eq "dismiss1") { $visible = $true; $afterClickDismissed = $true }
  elseif ($mode -eq "slow") { $visible = $true; $afterClickDismissed = $false }
  elseif ($mode -eq "absent") { $visible = $false; $reasonOverride = "no_notice_timeout" }
  elseif ($mode -eq "nosignal") { $visible = $false; $reasonOverride = "no_client_signal" }
  else { throw ("-NoticePollSim 只支持 dismiss1 | slow | absent | nosignal，当前：'" + $mode + "'") }
  $clickAction = { param($n) }.GetNewClosure()
  $verify = { param($n) if ($afterClickDismissed) { return $true } return $false }.GetNewClosure()
  $park = { }.GetNewClosure()
  $core = Invoke-NoticePollCore -NoticeVisible $visible -MaxClicks $maxClicks `
    -ClickAction $clickAction -VerifyDismiss $verify -ParkAction $park
  if ($reasonOverride -ne "" -and $core.Clicks -eq 0) { $core.Reason = $reasonOverride }
  return $core
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
# F5：只允许 1。0 时既不等 1018 又不点，语义自相矛盾；>1 会重复盲点世界区。
if ($NoticePollClicks -ne 1) {
  throw ("-NoticePollClicks 只允许 1（0 与 1018 校验矛盾；>1 会重复盲点世界区把角色走掉，见 docs/npc-render-diagnosis-20261022.md §2.3）")
}
if ($NoticeWaitSec -lt 0) { throw "-NoticeWaitSec 不能为负" }
if ($NoticeVerifySec -lt 0) { throw "-NoticeVerifySec 不能为负" }
if ($EvidenceSinceMs -lt 0) { throw "-EvidenceSinceMs 不能为负" }
# 坐标参数在启动客户端前就校验（公告按钮/停靠点/NPC 点），避免流程跑到一半才发现格式错。
foreach ($spec in @(@("NoticeButton", $NoticeButton), @("NoticeParkPoint", $NoticeParkPoint), @("NpcClick", $NpcClick))) {
  $sp = ([string]$spec[1]).Split(",")
  if ($sp.Count -ne 2 -or -not (Test-TurnIntLiteral $sp[0].Trim()) -or -not (Test-TurnIntLiteral $sp[1].Trim())) {
    throw ("-" + $spec[0] + " 须为 'x,y'（两个整数），当前：'" + $spec[1] + "'")
  }
}
if ($NoticePixelProbe -ne "") {
  $np = $NoticePixelProbe.Split(",")
  if ($np.Count -ne 4 -or $np[2].TrimStart("#") -notmatch '^[0-9A-Fa-f]{6}$' -or -not (Test-TurnIntLiteral $np[3].Trim())) {
    throw ("-NoticePixelProbe 须为 'x,y,RRGGBB,tol'，当前：'" + $NoticePixelProbe + "'")
  }
}
if ($ExpectPos -ne "") {
  $ep = $ExpectPos.Split(",")
  if ($ep.Count -ne 2 -or -not (Test-TurnIntLiteral $ep[0].Trim()) -or -not (Test-TurnIntLiteral $ep[1].Trim())) {
    throw ("-ExpectPos 须为 'X,Y'（两个整数），当前：'" + $ExpectPos + "'")
  }
}
if ($ProxyDir -ne "" -and $CapturePath -ne "") {
  throw "-ProxyDir 与 -CapturePath 互斥（live 用 -ProxyDir；事后回填/离线才用 -CapturePath）"
}
if ($FixturePreflight -and -not $FixtureDryRun) { throw "-FixturePreflight 只能与 -FixtureDryRun 联用（不启动客户端）" }
if ($NoticeEvidenceCheck -and -not $FixtureDryRun) { throw "-NoticeEvidenceCheck 只能与 -FixtureDryRun 联用（不启动客户端）" }
if ($NoticePollSim -ne "" -and -not $FixtureDryRun) { throw "-NoticePollSim 只能与 -FixtureDryRun 联用（不启动客户端）" }
if ($ClickNpc -and $ProxyDir -eq "" -and $CapturePath -eq "") {
  throw "-ClickNpc 需要证据源 -ProxyDir 或 -CapturePath：坐标门没有取证就不得盲点世界区（fail-closed）"
}
# 证据源在启动客户端之前就建好：CapturePath 若是旧证据会在这里 RED（F2），不会等登录完才报废。
$evidenceOffline = ($FixtureDryRun -and -not $FixturePreflight) -or $StageEvidenceCheck
$evidenceSource = New-EvidenceSource $ProxyDir $CapturePath $EvidenceSinceMs $script:flowStartMs -Offline:$evidenceOffline
$turnMapTable = $null
if ($AttackTurnMap -ne "") { $turnMapTable = Read-TurnCalibrationMap $AttackTurnMap }

# ---- 四类动作阶段参数门（fail-closed；都在启动客户端之前）----
$script:stageEvidence = [ordered]@{}
$script:logEvidence = $null
if ($EvidenceOut -eq "") { $EvidenceOut = Join-Path $OutDir "flow-stages-evidence.json" }
if ($ActionWaitSec -lt 1) { Stop-Usage "-ActionWaitSec 至少 1（要给服务端副作用落地时间）" }
if ($AttackRangeCells -lt 1 -or $AttackRangeCells -gt 8) { Stop-Usage "-AttackRangeCells 须在 1..8" }
if ($TradeDialogMaxAgeSec -lt 1) { Stop-Usage "-TradeDialogMaxAgeSec 至少 1" }
foreach ($spec in @(@("PickupPixel", $PickupPixel), @("NpcDialogPixel", $NpcDialogPixel),
                    @("TradeNpcPixel", $TradeNpcPixel), @("TradeMenuPixel", $TradeMenuPixel),
                    @("CombatTargetPixel", $CombatTargetPixel))) {
  if ([string]$spec[1] -ne "") {
    try { [void](ConvertFrom-PixelSpec ([string]$spec[0]) ([string]$spec[1])) }
    catch { Stop-Usage ("-" + $spec[0] + " 像素格式错：" + $_.Exception.Message) }
  }
}
if (-not $StageEvidenceCheck) {
  # 像素坐标只在**真机驱动**时需要；-StageEvidenceCheck 只读证据，不点任何东西。
  if ($PickupStage -and $PickupPixel -eq "") { Stop-Usage "-PickupStage 需要 -PickupPixel 'x,y'（脚下物品的屏幕像素）" }
  if ($NpcDialogStage -and $NpcDialogPixel -eq "" -and $NpcClick -eq "") {
    Stop-Usage "-NpcDialogStage 需要 -NpcDialogPixel 或 -NpcClick"
  }
  if ($TradeStage -and ($TradeNpcPixel -eq "" -or $TradeMenuPixel -eq "")) {
    Stop-Usage "-TradeStage 需要 -TradeNpcPixel 与 -TradeMenuPixel（先开商人对话，再点菜单项）"
  }
}
if ($StageEvidenceCheck) {
  if ($ProxyDir -eq "" -and $CapturePath -eq "") { Stop-Usage "-StageEvidenceCheck 需要 -ProxyDir 或 -CapturePath" }
  if (-not ($PickupStage -or $TradeStage -or $NpcDialogStage -or $CombatStage)) {
    Stop-Usage "-StageEvidenceCheck 至少要开一类阶段开关（-PickupStage/-NpcDialogStage/-TradeStage/-CombatStage）"
  }
}
if (-not $StageEvidenceCheck -and ($PickupStage -or $TradeStage -or $NpcDialogStage -or $CombatStage)) {
  if ($evidenceSource.Kind -eq "none") {
    Stop-Usage "四类动作阶段需要证据源 -ProxyDir（没有证据就无法机判，fail-closed）"
  }
  if ($evidenceSource.Kind -ne "proxy") {
    Stop-Usage "四类动作阶段需要 -ProxyDir（live hook dump）：capture.jsonl 不含编码后 body 长度，无法判对话/交易前置"
  }
  if ($CombatStage -and $Profile -eq "") { Stop-Usage "-CombatStage 需要 -Profile（fixtures.monsters）" }
  if ($CombatStage -and $PlayerMap -eq "") { Stop-Usage "-CombatStage 需要 -PlayerMap（经典协议线上没有图名）" }
}

# ---- 动作前显式前置门（A2）：只读消费 provision --gate 的 JSON ----
# 文件不存在是用法错（不能静默当作「门全过」）；存在则只在「没生效」时用于归类。
if ($PrereqGateJson -ne "" -and -not (Test-Path -LiteralPath $PrereqGateJson -PathType Leaf)) {
  Stop-Usage ("-PrereqGateJson 不存在：" + $PrereqGateJson)
}
$script:prereqGate = Read-PrereqGateJson $PrereqGateJson

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

# ---- 离线证据自检：按证据源评估四类动作（不启动客户端；A3 牙齿靠它重放）----
if ($StageEvidenceCheck) {
  $threshold = [long]$evidenceSource.SinceMs
  $atMs = [long]::MaxValue
  $script:logEvidence = Get-LogEvidence $LogDir ([bool]$RequireLogEvidence)
  Write-Output ("LOG_EVIDENCE status=" + $script:logEvidence.status + " files=" + $script:logEvidence.files_scanned +
    " lines=" + $script:logEvidence.lines_scanned + " err=" + $script:logEvidence.err_lines +
    " reason=" + $script:logEvidence.reason)
  Write-Output ("PREREQ_GATE source=" + $script:prereqGate.source + " missing=" +
    (Get-MissingText $script:prereqGate.missing) + " kind=" + $script:prereqGate.kind +
    " path=" + $script:prereqGate.path)
  $rcs = @()
  $red = 0
  $und = 0
  if ($PickupStage) {
    $ev = Get-PickupEvidence $evidenceSource $threshold $threshold $atMs $script:prereqGate
    Add-StageEvidence "pickup" $ev
    $rcs += [int]$ev.rc
    if ($ev.verdict -eq "RED") { $red++ } elseif ($ev.verdict -eq "UNDECIDABLE") { $und++ }
  }
  if ($NpcDialogStage) {
    $ev = Get-NpcDialogEvidence $evidenceSource $threshold $threshold $atMs $script:prereqGate
    Add-StageEvidence "npcdialog" $ev
    $rcs += [int]$ev.rc
    if ($ev.verdict -eq "RED") { $red++ } elseif ($ev.verdict -eq "UNDECIDABLE") { $und++ }
  }
  if ($TradeStage) {
    $ev = Get-TradeEvidence $evidenceSource $threshold $threshold $TradeKind $TradeDialogMaxAgeSec $atMs $script:prereqGate
    Add-StageEvidence "trade" $ev
    $rcs += [int]$ev.rc
    if ($ev.verdict -eq "RED") { $red++ } elseif ($ev.verdict -eq "UNDECIDABLE") { $und++ }
  }
  if ($CombatStage) {
    $ev = Get-CombatEvidence $evidenceSource $threshold $threshold $Profile $PlayerMap $AttackRangeCells $atMs $CombatTargetPixel
    if ($null -ne $ev.pixel) {
      Write-Output ("COMBAT_PIXEL pixel_source=" + $ev.pixel_source + " x=" + $ev.pixel.x + " y=" + $ev.pixel.y)
    } else {
      Write-Output "COMBAT_PIXEL pixel_source=unknown（未走到靶子匹配，无法给出本次像素）"
    }
    Add-StageEvidence "combat" $ev
    $rcs += [int]$ev.rc
    if ($ev.verdict -eq "RED") { $red++ } elseif ($ev.verdict -eq "UNDECIDABLE") { $und++ }
  }
  Write-StageEvidenceFile "evidence-check" $evidenceSource $Profile $PlayerMap $AttackRangeCells
  $rc = Get-RcAggregate ([int[]]$rcs)
  if ($RequireLogEvidence -and $script:logEvidence.status -eq "RED") {
    if ($rc -eq 0) { $rc = 1 }
    Write-Output ("STAGE_EVIDENCE_TOTAL RED=" + $red + " UNDECIDABLE=" + $und + " rc=" + $rc +
      " log=RED（-RequireLogEvidence：日志证据 RED 即整体 RED）")
    exit $rc
  }
  Write-Output ("STAGE_EVIDENCE_TOTAL RED=" + $red + " UNDECIDABLE=" + $und + " rc=" + $rc + " log=" + $script:logEvidence.status)
  exit $rc
}


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
  if ($NoticeEvidenceCheck) {
    if ($evidenceSource.Kind -eq "none") {
      Write-Output "NOTICE_EVIDENCE RED missing -ProxyDir/-CapturePath（证据源缺失）"
      throw "-FixtureDryRun -NoticeEvidenceCheck 需要 -ProxyDir 或 -CapturePath"
    }
    $nf = $null
    foreach ($f in (Get-EvidenceFrames $evidenceSource)) {
      if ($f.dir -ne "s2c" -or [int]$f.ident -ne 658) { continue }
      if ([long]$f.ts_ms -lt [long]$evidenceSource.SinceMs) { continue }
      if ($null -eq $nf -or [long]$f.ts_ms -ge [long]$nf.ts_ms) { $nf = $f }
    }
    if ($null -eq $nf) {
      Write-Output ("NOTICE_EVIDENCE RED ident=658 since_ms=" + $evidenceSource.SinceMs + "（本场证据里没有 s2c SM_SENDNOTICE）")
      exit 1
    }
    Write-Output ("NOTICE_EVIDENCE GREEN ident=658 ts_ms=" + $nf.ts_ms + " conn=" + $nf.conn + " since_ms=" + $evidenceSource.SinceMs)
    $did = $true
  }
  if ($ExpectPos -ne "") {
    if ($evidenceSource.Kind -eq "none") {
      Write-Output "POSITION_GATE RED missing -ProxyDir/-CapturePath（坐标门取证缺失）"
      throw "-FixtureDryRun -ExpectPos 需要 -ProxyDir 或 -CapturePath（坐标门取证）"
    }
    $gate = Test-PositionGate $evidenceSource $ExpectPos $evidenceSource.SinceMs
    if ($gate.Ok) {
      Write-Output ("POSITION_GATE GREEN " + $gate.Detail)
    } else {
      Write-Output ("POSITION_GATE RED " + $gate.Detail)
      exit 1
    }
    $did = $true
  }
  if (-not $did) {
    throw "-FixtureDryRun 需要 -NoticePollSim / -NoticeEvidenceCheck / -ExpectPos（离线夹具自检）"
  }
  Write-Output "FIXTURE_DRY_RUN_OK"
  exit 0
}

$src = @"
using System;
using System.Collections.Generic;
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
  // F3：客户端侧信号——枚举本进程的可见顶层窗口（公告对话框通常是主窗口之外的第二个窗体）。
  private delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] private static extern int GetWindowTextLength(IntPtr hWnd);
  public static IntPtr[] VisibleTopLevelWindows(uint pid) {
    var list = new List<IntPtr>();
    EnumWindows(delegate(IntPtr h, IntPtr l) {
      uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid && IsWindowVisible(h) && GetWindowTextLength(h) > 0) list.Add(h);
      return true;
    }, IntPtr.Zero);
    return list.ToArray();
  }

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

function Get-NoticePixelProbe {
  # "x,y,RRGGBB,tol"：把客户端坐标换算到屏幕，取 1x1 像素与期望色比较（每通道 <= tol）。
  # 这是**截图/屏幕像素**判定，属于客户端侧信号；没有标定值时不要开启（默认空）。
  if ($NoticePixelProbe -eq "") { return $null }
  $parts = $NoticePixelProbe.Split(",")
  if ($parts.Count -ne 4) { throw "-NoticePixelProbe 须为 'x,y,RRGGBB,tol'，当前：$NoticePixelProbe" }
  $px = [int]$parts[0]; $py = [int]$parts[1]
  $hex = $parts[2].TrimStart("#")
  if ($hex -notmatch '^[0-9A-Fa-f]{6}$') { throw "-NoticePixelProbe 颜色须为 RRGGBB：$($parts[2])" }
  $tol = [int]$parts[3]
  $wantR = [Convert]::ToInt32($hex.Substring(0, 2), 16)
  $wantG = [Convert]::ToInt32($hex.Substring(2, 2), 16)
  $wantB = [Convert]::ToInt32($hex.Substring(4, 2), 16)
  $pt2 = New-Object MirFlow+POINT
  $pt2.X = $px; $pt2.Y = $py
  [void][MirFlow]::ClientToScreen($script:hwnd, [ref]$pt2)
  $bmp = New-Object System.Drawing.Bitmap(1, 1)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try { $g.CopyFromScreen($pt2.X, $pt2.Y, 0, 0, (New-Object System.Drawing.Size(1, 1))) }
  finally { $g.Dispose() }
  try { $c = $bmp.GetPixel(0, 0) } finally { $bmp.Dispose() }
  $ok = ([Math]::Abs([int]$c.R - $wantR) -le $tol -and
         [Math]::Abs([int]$c.G - $wantG) -le $tol -and
         [Math]::Abs([int]$c.B - $wantB) -le $tol)
  return [pscustomobject]@{ Visible = $ok; Actual = ("{0:X2}{1:X2}{2:X2}" -f $c.R, $c.G, $c.B) }
}

function Get-NoticeClientSignal {
  # F3：点击前的客户端侧信号。优先级：像素探针 > 外部探针脚本 > 顶层窗口探测（需显式开关）。
  # 返回 $true / $false / $null（unknown）。unknown 时调用方不得点击。
  if ($NoticePixelProbe -ne "") {
    $r = Get-NoticePixelProbe
    if ($null -ne $r) {
      Write-Output ("NOTICE_SIGNAL pixel actual=#" + $r.Actual + " visible=" + $r.Visible)
      return [bool]$r.Visible
    }
  }
  if ($NoticeProbe -ne "") {
    if (-not (Test-Path -LiteralPath $NoticeProbe)) { return $null }
    $out = & $NoticeProbe 2>&1
    $code = $LASTEXITCODE
    Write-Output ("NOTICE_SIGNAL probe exit=" + $code + " out=" + (($out | ForEach-Object { [string]$_ }) -join " "))
    if ($code -eq 0) { return $true }
    if ($code -eq 1) { return $false }
    return $null
  }
  if ($NoticeWindowProbe -and $script:clientPid -gt 0) {
    $wins = [MirFlow]::VisibleTopLevelWindows([uint32]$script:clientPid)
    $visible = ([int]$wins.Count -gt 1)
    Write-Output ("NOTICE_SIGNAL windows pid=" + $script:clientPid + " visible_titled=" + $wins.Count + " visible=" + $visible)
    return $visible
  }
  return $null
}

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
$script:clientPid = [int]$p.Id
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
# ---- 公告轮询（卡 fixture-harden 修正；F1/F2/F3/F4/F5）----
# 旧实现：在固定点 (522,525) 盲点 12 次（每 1.2s）。公告只吃**第一次**，其余 11 次落在世界区，
# 每次都把 g_nTargetX/Y 重设到鼠标格 ⇒ 角色自动走位（r5 实测 (29,33)→(29,44)）并落库，
# 国王 (29,32) 出视窗（docs/npc-render-diagnosis-20261022.md §2.3）。
# 新实现：证据源（live proxy / 本场 capture）看到本场 658 ⇒ 最多点 1 次 ⇒ 按**点击时刻**
#   等本场 1018；无证据源 ⇒ 必须有客户端侧信号（像素/探针/窗口），否则只停鼠标、不点击。
# 服务端窗口仍只有 10 秒（PlayObject.RunNotice：>10s ⇒ BoEmergencyClose），
# 所以等待取值要小于它：默认 NoticeWaitSec=4 + NoticeVerifySec=6。
Stage "notice_wait"
$noticeNb = $NoticeButton.Split(",") | ForEach-Object { [int]$_ }
$noticePark = $NoticeParkPoint.Split(",") | ForEach-Object { [int]$_ }
# F2：live proxy 必须是本场（登录后已有 data 事件）——不是上一场遗留目录。
if ($evidenceSource.Kind -eq "proxy") { Assert-ProxyFresh $evidenceSource $script:flowStartMs }
$noticeSeen = $null
$noticeVisible = $false
$noticeSignal = "none"
if ($evidenceSource.Kind -ne "none") {
  $noticeSeen = Wait-EvidenceFrame $evidenceSource "s2c" 658 $NoticeWaitSec $evidenceSource.SinceMs
  if ($null -eq $noticeSeen) {
    Write-Output ("NOTICE_POLL RED：" + $NoticeWaitSec + "s 内未见本场 s2c SM_SENDNOTICE(658)（mode=" +
      $evidenceSource.Kind + " since_ms=" + $evidenceSource.SinceMs + "；fail-closed：一次也不点世界区）")
    throw "NOTICE_POLL RED: 公告未出现（本场证据里无 SM_SENDNOTICE 658，不点世界区）"
  }
  $noticeVisible = $true
  $noticeSignal = "evidence:658 ts_ms=" + $noticeSeen.ts_ms
} else {
  # F3：没有证据源时，点击前必须有客户端侧信号；拿不到就只 Move-Mouse 不点击。
  $deadlineN = (Get-Date).AddSeconds($NoticeWaitSec)
  while ($true) {
    $sig = Get-NoticeClientSignal
    if ($sig -eq $true) { $noticeVisible = $true; $noticeSignal = "client_signal"; break }
    if ((Get-Date) -ge $deadlineN) { $noticeVisible = $false; $noticeSignal = "none"; break }
    Start-Sleep -Milliseconds 250
  }
}
Shot "05_ingame.png"
Stage "notice_poll"
$script:noticeClickMs = [long]0
$noticePoll = $null
if ($noticeVisible) {
  $clickAction = {
    $script:noticeClickMs = [long]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
    Send-Click $noticeNb[0] $noticeNb[1] 1
  }.GetNewClosure()
  $verifyDismiss = {
    param($n)
    if ($evidenceSource.Kind -eq "none") { return $null }   # 无证据 ⇒ 只报 UNVERIFIED，不假装知道
    $floor = [Math]::Max([long]$evidenceSource.SinceMs, [long]$script:noticeClickMs - $script:EvidenceSlackMs)
    $ok = Wait-EvidenceFrame $evidenceSource "c2s" 1018 $NoticeVerifySec $evidenceSource.SinceMs $floor
    if ($null -eq $ok) { return $false }
    return $true
  }.GetNewClosure()
  $parkAction = { Move-Mouse $noticePark[0] $noticePark[1] }.GetNewClosure()
  $noticePoll = Invoke-NoticePollCore -NoticeVisible $true -MaxClicks $NoticePollClicks `
    -ClickAction $clickAction -VerifyDismiss $verifyDismiss -ParkAction $parkAction
} else {
  $parkAction = { Move-Mouse $noticePark[0] $noticePark[1] }.GetNewClosure()
  $noop = { }.GetNewClosure()
  $unknown = { param($n) return $null }.GetNewClosure()
  $noticePoll = Invoke-NoticePollCore -NoticeVisible $false -MaxClicks $NoticePollClicks `
    -ClickAction $noop -VerifyDismiss $unknown -ParkAction $parkAction
  $noticePoll.Reason = "no_client_signal"
}
if ($evidenceSource.Kind -ne "none") {
  if (-not $noticePoll.Dismissed) {
    Write-Output ("NOTICE_POLL RED：单击后 " + $NoticeVerifySec + "s 内未见本场 c2s CM_LOGINNOTICEOK(1018)（公告可能没被点掉；服务端 10s 会踢人）")
    throw "NOTICE_POLL RED: CM_LOGINNOTICEOK(1018) 未出网（fail-closed：不再点世界区）"
  }
  Write-Output ("NOTICE_POLL OK clicks=" + $noticePoll.Clicks + " dismissed=1018 signal='" + $noticeSignal + "' mode=" + $evidenceSource.Kind)
} else {
  Write-Output ("NOTICE_POLL UNVERIFIED clicks=" + $noticePoll.Clicks + " world_clicks=" + $noticePoll.WorldClicks +
    " signal='" + $noticeSignal + "' reason=" + $noticePoll.Reason + " mode=no_evidence")
}
Shot "05b_after_notice.png"
Stage "ingame"
Start-Sleep -Seconds 2
Shot "05c_ingame_ready.png"

# ---- 坐标门（卡 fixture-harden）：任何世界区点击之前先断言角色格 ----
# 依据：docs/npc-render-diagnosis-20261022.md §2.3/§2.4。角色一旦被夹具走远，
# 所有屏幕点都会换算到空地 ⇒ CM_WALK（而不是 CM_CLICKNPC），且国王出视窗。
# 取证：证据源里 ts_ms >= 本场下界 的「c2s CM_WALK(3011) recog=X|Y<<16」或
#       「s2c SM_LOGON(50)/SM_NEWMAP(51) param=X tag=Y」。F6：CM_WALK 是**目标格**，
#       只接受与上一条可信证据切比雪夫距离 <=1 的步；跨度 >1 的目标忽略并回退到锚点。
# -ClickNpc 未显式给 -ExpectPos 时按比奇国王场景默认 (29,33)（NPC 在 (29,32)）。
$gateExpect = $ExpectPos
if ($gateExpect -eq "" -and $ClickNpc) { $gateExpect = "29,33" }
if ($gateExpect -ne "") {
  if ($evidenceSource.Kind -eq "none") {
    Write-Output ("POSITION_GATE RED missing -ProxyDir/-CapturePath（expect=" + $gateExpect + "，无取证不得继续世界区点击）")
    throw "POSITION_GATE RED: -ExpectPos 需要证据源 -ProxyDir 或 -CapturePath（fail-closed）"
  }
  $gate = Test-PositionGate $evidenceSource $gateExpect $evidenceSource.SinceMs
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

# ==== 四类玩家动作阶段（卡 openmir2-mirflow-combat-pickup-stages；只追加）============
# 既有 25 阶段的名称与相对顺序不变；本块只新增阶段名（pickup / npc_dialog / trade /
# combat_target / combat）。每个阶段都按证据判 RED/GREEN，RED ⇒ throw（不在坏状态上继续）。
if ($PickupStage -or $TradeStage -or $NpcDialogStage -or $CombatStage) {
  $script:logEvidence = Get-LogEvidence $LogDir ([bool]$RequireLogEvidence)
  Write-Output ("LOG_EVIDENCE status=" + $script:logEvidence.status + " files=" + $script:logEvidence.files_scanned +
    " lines=" + $script:logEvidence.lines_scanned + " err=" + $script:logEvidence.err_lines +
    " reason=" + $script:logEvidence.reason)
  if ($RequireLogEvidence -and $script:logEvidence.status -eq "RED") {
    throw ("LOG_EVIDENCE RED " + $script:logEvidence.reason)
  }
}

if ($PickupStage) {
  Stage "pickup"
  $px = ConvertFrom-PixelSpec "PickupPixel" $PickupPixel
  Write-Output ("PICKUP_ACTION x=" + $px[0] + " y=" + $px[1] + "（物品必须在角色格上）")
  $actMs = Get-TurnNowMs
  Send-Click $px[0] $px[1] 1
  Start-Sleep -Seconds $ActionWaitSec
  $ev = Get-PickupEvidence $evidenceSource $evidenceSource.SinceMs $actMs (Get-TurnNowMs) $script:prereqGate
  Add-StageEvidence "pickup" $ev
  Write-StageEvidenceFile "live" $evidenceSource $Profile $PlayerMap $AttackRangeCells
  if (-not $ev.ok) { Stop-StageEvidence $ev }
  Shot "08a_pickup.png"
  Move-Mouse $noticePark[0] $noticePark[1]
}

if ($NpcDialogStage) {
  Stage "npc_dialog"
  $np = if ($NpcDialogPixel -ne "") { $NpcDialogPixel } else { $NpcClick }
  $px = ConvertFrom-PixelSpec "NpcDialogPixel" $np
  Write-Output ("NPC_DIALOG_ACTION x=" + $px[0] + " y=" + $px[1])
  $actMs = Get-TurnNowMs
  Send-Click $px[0] $px[1] 1
  Start-Sleep -Seconds $ActionWaitSec
  $ev = Get-NpcDialogEvidence $evidenceSource $evidenceSource.SinceMs $actMs (Get-TurnNowMs) $script:prereqGate
  Add-StageEvidence "npcdialog" $ev
  Write-StageEvidenceFile "live" $evidenceSource $Profile $PlayerMap $AttackRangeCells
  if (-not $ev.ok) { Stop-StageEvidence $ev }
  Shot "08b_npc_dialog.png"
  Move-Mouse $noticePark[0] $noticePark[1]
}

if ($TradeStage) {
  Stage "trade"
  $npcPx = ConvertFrom-PixelSpec "TradeNpcPixel" $TradeNpcPixel
  $menuPx = ConvertFrom-PixelSpec "TradeMenuPixel" $TradeMenuPixel
  Write-Output ("TRADE_ACTION kind=" + $TradeKind + " npc=" + $npcPx[0] + "," + $npcPx[1] +
    " menu=" + $menuPx[0] + "," + $menuPx[1])
  $actMs = Get-TurnNowMs
  # 前置动作 1：先点 NPC 打开商人对话（A3 牙齿：删掉这一步 ⇒ trade 必 RED）
  Send-Click $npcPx[0] $npcPx[1] 1
  Start-Sleep -Milliseconds 1500
  # 前置动作 2：点菜单项/物品触发 CM_USERBUYITEM(1014) 或 CM_USERSELLITEM(1013)
  Send-Click $menuPx[0] $menuPx[1] 1
  Start-Sleep -Seconds $ActionWaitSec
  $ev = Get-TradeEvidence $evidenceSource $evidenceSource.SinceMs $actMs $TradeKind $TradeDialogMaxAgeSec (Get-TurnNowMs) $script:prereqGate
  Add-StageEvidence "trade" $ev
  Write-StageEvidenceFile "live" $evidenceSource $Profile $PlayerMap $AttackRangeCells
  if (-not $ev.ok) { Stop-StageEvidence $ev }
  Shot "08c_trade.png"
  Move-Mouse $noticePark[0] $noticePark[1]
}

if ($CombatStage) {
  Stage "combat_target"
  # A2/A1：动作前的显式前置门（与 -StageEvidenceCheck 同一套成因分类）：
  #   没声明夹具 => UNDECIDABLE(missing_prereq)；声明在别的图 / AP 外 => RED(out_of_range)。
  if ($PlayerMap -eq "") {
    Write-Output "STAGE_EVIDENCE category=combat verdict=UNDECIDABLE kind=missing_prereq rc=6 missing=monster_fixture reason=-PlayerMap 未给出（无法证明同图）=> 不可判"
    exit 6
  }
  $fx = $null
  try { $fx = Read-MonsterFixtures $Profile }   # 不要套 @()（见 Get-CombatEvidence 的说明）
  catch {
    Write-Output ("STAGE_EVIDENCE category=combat verdict=UNDECIDABLE kind=missing_prereq rc=6 missing=monster_fixture reason=夹具声明缺失：" + $_.Exception.Message)
    exit 6
  }
  if ($null -eq $fx -or @($fx).Count -eq 0) {
    Write-Output "STAGE_EVIDENCE category=combat verdict=UNDECIDABLE kind=missing_prereq rc=6 missing=monster_fixture reason=fixtures.monsters 为空（没有声明任何靶子）"
    exit 6
  }
  # 自身坐标可能比 proxy 落盘晚一拍：轮询最多 5s，仍没有就 RED（fail-closed，不盲点）。
  $self = $null
  $selfDeadline = (Get-Date).AddSeconds(5)
  while ($true) {
    $self = Get-SelfStateAt $evidenceSource $evidenceSource.SinceMs (Get-TurnNowMs)
    if ($null -ne $self -or (Get-Date) -ge $selfDeadline) { break }
    Start-Sleep -Milliseconds 250
  }
  if ($null -eq $self) {
    Write-Output "STAGE_EVIDENCE category=combat verdict=RED kind=no_position rc=1 reason=证据源里没有自身坐标（s2c SM_LOGON(50)/SM_NEWMAP(51)）"
    exit 1
  }
  $same = @($fx | Where-Object { $_.Map -ieq $PlayerMap })
  if ($same.Count -eq 0) {
    $maps = (@($fx | ForEach-Object { [string]$_.Map } | Sort-Object -Unique) -join ",")
    Write-Output ("STAGE_EVIDENCE category=combat verdict=RED kind=out_of_range rc=8 reason=靶子只在 map=" + $maps + "，与 -PlayerMap=" + $PlayerMap + " 不同图")
    exit 8
  }
  $target = $null
  $bestDist = [int]::MaxValue
  foreach ($m in $same) {
    $d = [Math]::Max([Math]::Abs([int]$m.X - [int]$self.X), [Math]::Abs([int]$m.Y - [int]$self.Y))
    if ($d -le $AttackRangeCells -and $d -lt $bestDist) { $bestDist = $d; $target = $m }
  }
  if ($null -eq $target) {
    $nearestDist = [int]::MaxValue
    foreach ($m in $same) {
      $d = [Math]::Max([Math]::Abs([int]$m.X - [int]$self.X), [Math]::Abs([int]$m.Y - [int]$self.Y))
      if ($d -lt $nearestDist) { $nearestDist = $d }
    }
    Write-Output ("STAGE_EVIDENCE category=combat verdict=RED kind=out_of_range rc=8 reason=角色(" + $self.X + "," + $self.Y + ") AP<=" + $AttackRangeCells + " 格内没有同图靶子（最近 " + $nearestDist + " 格）")
    exit 8
  }
  $dx = [int]$target.X - [int]$self.X
  $dy = [int]$target.Y - [int]$self.Y
  if ($dx -eq 0 -and $dy -eq 0) {
    throw ("STAGE_EVIDENCE RED category=combat 靶子与角色同格（" + $target.X + "," + $target.Y + "），无法定向攻击")
  }
  Write-Output ("COMBAT_TARGET map=" + $target.Map + " x=" + $target.X + " y=" + $target.Y +
    " name=" + $target.Name + " own=" + $self.X + "," + $self.Y + " dist=" + $bestDist)
  # 像素：显式 -CombatTargetPixel 优先；否则沿用格心投影（512,304）+ (dx*48, dy*32)。
  # ⚠ 48x32 步长在本部署未被证实（docs/window-shrink-run-20261019.md §F2）；真机建议给显式像素。
  # A1/F1：pixel_source(explicit|projected) 必须同时出现在 stdout 与证据里，否则真机 combat RED
  # 时没法区分「靶子没刷出」与「投影点歪」。
  $tp = Resolve-CombatTargetPixel $CombatTargetPixel $dx $dy
  Stage "combat"
  Write-Output ("COMBAT_ACTION x=" + $tp.X + " y=" + $tp.Y + " pixel_source=" + $tp.Source + "（先右键转向，再左键攻击）")
  if ($tp.Source -eq "projected") {
    Write-Output "WARN COMBAT_PIXEL_SOURCE projected（48x32 步长未证实 —— 见 docs/client-flow-stages-20261025.md §7.3；真机建议给 -CombatTargetPixel）"
  }
  $actMs = Get-TurnNowMs
  Send-RightClick $tp.X $tp.Y
  Start-Sleep -Milliseconds 400
  Send-Click $tp.X $tp.Y 1
  Start-Sleep -Seconds $ActionWaitSec
  $ev = Get-CombatEvidence $evidenceSource $evidenceSource.SinceMs $actMs $Profile $PlayerMap $AttackRangeCells (Get-TurnNowMs) $CombatTargetPixel
  # 证据里的像素必须是**这次真正点下去**的那一个（check 模式按 CM_HIT 坐标回放，可能与 live 不同 ⇒ 先提示再覆盖）。
  if ($null -ne $ev.pixel -and ([int]$ev.pixel.x -ne [int]$tp.X -or [int]$ev.pixel.y -ne [int]$tp.Y)) {
    Write-Output ("WARN COMBAT_PIXEL 回放=" + $ev.pixel.x + "," + $ev.pixel.y + " 实际点击=" + $tp.X + "," + $tp.Y + "（SM_LOGON 与 CM_HIT 之间角色移动？以点击为准）")
  }
  $ev["pixel_source"] = $tp.Source
  $ev["pixel"] = [ordered]@{ x = [int]$tp.X; y = [int]$tp.Y }
  Add-StageEvidence "combat" $ev
  Write-StageEvidenceFile "live" $evidenceSource $Profile $PlayerMap $AttackRangeCells
  if (-not $ev.ok) { Stop-StageEvidence $ev }
  Shot "08d_combat.png"
  Move-Mouse $noticePark[0] $noticePark[1]
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
