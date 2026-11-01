# MCP 本机 admin 通道：真机窗口执行单（2026-11-01）

- 卡：`openmir2-mcp-admin-window-sheet`（tier=L，只写文档；**不跑窗口、不动 `E:\MirServer`、不起停栈**）
- 来源：台账 `openmir2-real-machine-verification-master` 评论（event=`mcp-admin-channel-slice2-merged-window-pending`，oid 0f9f11c0）
- 上游：`docs/mcp-gm-server-20261101.md` §7/§9、`mir2-rs/tests/parity/whitelist.md` 的 `T-273`、
  `docs/real-machine-window-plan-20261025.md`、`docs/window-runbook-20261023.md`
- 基线：`walgit/master` `f40235b1b`（本卡分支 `card/mcp-admin-window-sheet-dev2` 的 fork point）

> **状态：全部 `NOT_RUN`。** 本文件是执行单，不是证据。任何一项在真机上跑过之前，
> 卡、报告、台账都不得写成「已通过 / 已验收 / 无差异」。本卡作者**没有**执行下面任何一条窗口命令，
> 也没有改动过 `E:\MirServer` 的任何文件（只做了只读核对，见 §1）。

---

## 1. 本卡实测的现状（只读核对）

> 用途：开窗当天对照，确认「开的就是预期那棵树、那套内容」。**数字会漂**，开窗当天必须重取。

### 1.1 现网 `Server.conf`：**没有 `[Admin]` 段**

| 项 | 实测值（2026-11-01，只读） | 怎么取 |
|---|---|---|
| 路径 | `E:\MirServer\M2GameSvr\Server.conf` | — |
| 字节数 | 58080 | `(Get-Item -LiteralPath $p).Length` |
| sha256 | `ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2` | `sha256sum`（Git Bash）或 §3.1 的 .NET 写法 |
| 段数 | **14** | `grep -c '^\[' <conf>` |
| 段名 | `[Server] [Share] [Names] [Setup] [Hero] [HeroInfo] [NeiGua] [Client] [Magic] [NewItem] [MakeWine] [Skill_78] [SkillInfo] [ClientConfig]` | `grep -n '^\[' <conf>` |
| `[Admin]` | **0 次**（大小写不敏感地查 `adminport`/`adminbind` 也是 0） | `grep -c '^\[Admin\]' <conf>` |
| 编码 | UTF-8 **带 BOM**（`EF BB BF` 开头） | 首 3 字节 |
| 换行 | **全部 LF**（0 个 CRLF，3103 个 `\n`） | `b.count(b'\r\n')` |
| 文件末尾 | **没有换行**（最后一个字节是 `1`，`[ClientConfig]` 的 `Button_Piaoxue=1`） | 末字节 |

⇒ 缺段 ⇒ `AdminPort` 取缺省 **0** ⇒ game-svr **不监听**本机 admin 通道
（`mir2-rs/bins/game-svr/src/config.rs` 的 `[Admin]` 解析臂；`glog` 打印「本机 admin 通道未启用（[Admin] AdminPort=0；T-273）」）。

### 1.2 命令表事实：`@怪` / `@清除怪物` / `@LoadAdmin`

> **重要更正**：台账与上一版卡面写的 `@怪 <地图> <X> <Y> <数量>` **不是真实用法**。
> `@怪` 没有地图 / X / Y 参数，落点固定是**调用者的身前格**。
> `CommandDef.params` 是 **`Execute` 形参个数**（`GameCommand.cs:99-103` 的三选一），
> **不是**用户参数个数；用户参数个数看 Help 文案。三处定义都在
> `mir2-rs/bins/game-svr/src/gmcmd.rs` 的命令表（`CommandDef { name, perm_min, params, imp }`）。

| 命令 | 现网名 | `Imp` | `PermissionMin` | `CommandDef.params` | 用户参数（Help 原文） | 真实行为（oracle 照抄） |
|---|---|---:|---:|---:|---|---|
| 刷怪 | `怪` | `MobSpawn` | **10** | 2 | `命令格式: @怪 怪物名称 数量 等级(0-7)` | `怪物名称` **为空 ⇒ 回 Help**（`命令格式: @怪 怪物名称 数量 等级(0-7)`，`gmcmd.rs` 的 `if s_name.is_empty() { return red(MOB_SPAWN_HELP) }`）；红字 `怪物名称不正确或其它未问题!!!` 只用于「名字生不出来 / 说话者地图解析失败」这类失败支。`数量` 缺省 1、`<=0` 钳 1、上限 **64**；`等级` 缺省 0、`>10` 归 0（`(byte)` 截断后判）；落点 = **调用者身前格**（`world.self_front_pos()`），身前格不可站会走 oracle 的地形移位循环（抽 RNG）。 |
| 清怪 | `清除怪物` | `ClearMapMonster` | **10** | 2 | `命令格式: @清除怪物 地图号(* 为所有) 怪物名称(* 为所有) 掉物品(0,1)` | 三个用户参数**全必填**（缺一个 ⇒ 红 Help）；**杀怪循环整段注释**（`src/Modules/GameCommand/Commands/ClearMapMonsterCommand.cs` 第 40–73 行，`if (envir == null)` 在 74 行）⇒ `envir` 恒 null ⇒ **恒回红 `输入的地图不存在!!!`**，「已清除怪物数: 0」（79 行）不可达。⇒ 本命令**不改变世界状态、不抽 RNG**。见 `gmcmd.rs` 的常量 `CLEAR_MAP_MONSTER_HELP` / `CLEAR_MAP_MONSTER_NO_MAP` 与函数 `clear_map_monster`。 |
| 重载管理员 | `LoadAdmin` | `LoadAdmin` | **10** | 1 | 无用户参数 | 体内 `Permission < 6` 在门 10 之后是死支；`LocalDB.LoadAdminList()` 与 `SendServerGroupMsg(213,…)` **两条都被注释** ⇒ 只回绿 `管理员列表重新加载成功...`，**不改任何权限**。见 `gmcmd.rs` 的 `fn load_admin`。 |

三条命令都 **不在 perm 0 的 5 条里** ⇒ 客户端 `CM_SAY` 直呼一律被权限门拦成 `权限不够!!!`（红字，`Param = make_word16(255,56) = 0xFF | (0x38<<8) = 0x38FF = 14591`；
`smframe.rs` 的 `make_word16(low, high) = low | (high << 8)`，入参是 `(f_color, b_color)`，
`lib.rs` 的单测钉 `0x38FF`）。**不要**写成 `65336`（那是把 low/high 反着算出来的 `0xFF38`）。
可达路径只有两条，且都**临时**把会话权限抬到 10：

- **脚本 `GMEXECUTE`**：`ActionOfGmExecute` 抬权 → `ProcessUserLineMsg` → `finally` 还原（`PermissionGuard`，唯一抬权地址在 `mir2-rs/crates/script/src/engine/actions.rs`）；
- **admin 通道**：`execute_admin_gm_command` 抬权 → `gmcmd::execute` → **立刻还原**（`mir2-rs/bins/game-svr/src/lib.rs`，抬权常量 `ADMIN_GM_PERMISSION`），且与脚本/`CM_SAY` 走**同一个事件末收口** `command_event_tail_gm_drain`。

### 1.3 工具门清点（本卡实测）

```
python -I -B tools/hygiene/tool_selftest_gate.py --inventory
```

实测输出末行：`清点：66 个带 selftest 的工具；门禁内 61；已分类出域 5；未分类 0`，**rc=0**（2026-11-01，HEAD `f40235b1b`）。
与 `docs/real-machine-window-plan-20261025.md` 的认树数字 **61 项 / 清点 66 = 61 + 5 + 0** 一致。

> `goal_gate selftest`（计划文件写 85 条断言）与 `cargo test --workspace` 的三元组：**本卡未跑**（窗口期禁 cargo），
> 开窗当天按 §7 的清单重取，并把实际数字抄进本次证据的 `README.txt`。

### 1.4 二进制与端口（本卡实测，观察值）

- `tools/m2-acceptance/start_mir2_server.ps1 -Root 'E:\MirServer' -WhatIf`（只打印、不起进程）实测：

```text
SERVER_EXE E:\Users\gxh\Documents\GitHub\OpenMir2\.worktrees\dev-2\mir2-rs\target\release\mir2-server.exe
SERVER_EXE_SHA256 c01e864595fa2dd3bf91fb4ee27ef134b06dbc33a2ef8f2adc63f2363bb78bb2
SERVER_EXE_BUILD Release
```

  ⇒ 该 worktree 里有 release 构建；**窗口当天必须在要起栈的那棵树里重建**，再跑一次 `-WhatIf`，
  以打印的 sha256 为准（用旧二进制跑出来的结论没有意义，且它不会报错）。

- 2026-11-01 本机观察：十端口（3306 + 5000/5100/5500/5600/5700/6000/7000/7100/7200）**全部在听**，
  且 5000–7200 的进程是 **`mir2-server.exe` 六角色**（来自主检出 `E:\Users\gxh\Documents\GitHub\OpenMir2\mir2-rs\target\release\`），
  即当前在跑的是 Rust 栈、根 = `E:\MirServer`、pid 文件 `E:\MirServer\logs\mir2-server\mir2-server.pid.json`。
  → 开窗时要停的**就是这一套**，用 `tools/m2-acceptance/stop_mir2_server.ps1 -Root 'E:\MirServer'`。

- 部署库里角色 `winplay0` 存在（`mir2_db.characters`），账号 `winplay0` 在 `mir2_account.account`、State=0。
  口令由执行人从部署账号表读取并**只**经环境变量传入；**不得**写进 `README.txt` / 证据正文
  （`mir2_probe` 的 `run.json` 用 `masked_argv` 掩掉 `--password`，但仍按此纪律处理）。

---

## 2. 前置与纪律（W4）

### 2.1 开窗前逐条核对（缺一不跑）

| # | 判据 | 命令 | 期望 |
|---:|---|---|---|
| 1 | 仓库干净、对的是预期树 | `cd <本次要起栈的树> && git status --porcelain && git rev-parse HEAD` | 空输出；HEAD = 本条窗口钉住的那个提交（写进 README） |
| 2 | 工具门清点 | `python -I -B tools/hygiene/tool_selftest_gate.py --inventory; echo rc=$?` | 末行 `清点：66 … 未分类 0`，rc=0（§1.3 实测值；只有增减工具时才允许变） |
| 3 | 工具自测 | `python -I -B tools/hygiene/tool_selftest_gate.py; echo rc=$?` | 全绿、rc=0（约 +120s） |
| 4 | goal 认树 | `python -I -B tools/goal/goal_gate.py selftest; echo rc=$?` | 断言全过，rc=0（计划文件记 85 条；以当天输出为准） |
| 5 | 二进制 | `start_mir2_server.ps1 -Root 'E:\MirServer' -WhatIf -RequireRelease` | 三行 `SERVER_EXE / SERVER_EXE_SHA256 / SERVER_EXE_BUILD`；BUILD 必须是 `Release`，sha 抄进 README |
| 6 | 端口基线 | `powershell -NoProfile -ExecutionPolicy Bypass -File E:\MirServer\port-status.ps1 -Out <证据目录>\port-before.txt` | 十端口在听 + 3306 UP；落盘后再读文件（不要管道截输出） |
| 7 | 规则登记齐 | 见 `docs/window-runbook-20261023.md` §1 第 5 条的 `missing: []` 自查 | `missing: []` |
| 8 | 方案确认 | 本文件的 `NOT_RUN` 项在台账里仍是「未跑」 | 台账/看板一致 |
| 9 | 桌面可用（三信号，缺一不可） | 见 §2.3：开窗前跑 ① / ②，开窗后、采集前跑 ③ | `SIG1_OK` + `SIG2_OK` + 客户端日志新增 `key_char`；任一不满足 ⇒ `NOT_RUN (desktop-locked)`，**窗口不开** |

### 2.2 窗口期硬纪律

1. **窗口期禁 cargo**：构建在开窗**之前**完成。窗口内跑 `cargo` 会让真机进程出现
   `0xC0000142` 之类与判据无关的失败，且抢内存（历史事故见
   `docs/window-runbook-20261023.md` 与本仓 memory 的 `window-no-cargo-rule`）。
   非跑不可 ⇒ 先把本次窗口**判作废**、停栈、重建、重开窗。
2. **起停栈只用** `tools/m2-acceptance/start_mir2_server.ps1` / `stop_mir2_server.ps1`，
   `-Root 'E:\MirServer'`（默认 `-LogDir E:\MirServer\logs\mir2-server`，与现网 pid 文件一致）。
   **不要**用 `E:\MirServer\start-all.ps1 -Stop`（它会强杀 mysqld；见 runbook §0.13）。
3. **mysqld（3306）不停**：它是外部依赖，`start_mir2_server.ps1` 显式不碰。
4. **一次只跑一套栈**：同一时刻只允许一个 `mir2-server` 六角色栈占用 5000–7200；
   起栈前 `stop_mir2_server.ps1` 必须打印 `MIR2_SERVER_STACK_STOPPED` 且 rc=0。
5. **改动只有两类，且都要 before/after sha + 备份 + 还原**：
   ① `E:\MirServer\M2GameSvr\Server.conf` 的 `[Admin]` 段（§3.1）；
   ② `E:\MirServer\M2GameSvr\Envir\Market_Def\沙巴克\药店-0153.txt` 的脚本夹具（§4.1）。
   其它文件一律不碰。
6. **与客户端侧会话的协调**：本窗口要停生产端口上的栈，会让在线客户端断线。
   开窗前在台账/看板发一条签名 `comment`（窗口编号 + 起止时间 + 「5000–7200 将短暂不可用」），
   并由 owner 明确「此刻无其他人占用客户端会话 / 没有别的车道在跑真机批次」。
   窗口内**不再起**任何第二套 conformance 栈（同库并发会互相污染 DB 后态）。
7. **证据目录**：`E:\tmp\admin-window-20261101\`（本次窗口新建；收窗把路径登记进交付报告 §残留物）。
8. **不许把「没跑到」写成「通过」**：见 §6。

### 2.3 开窗前的桌面可用性检查（机判，三信号缺一不可）

**为什么**：锁屏（或前台是安全桌面）时，客户端 SDL 收不到键盘焦点与鼠标 motion，采到的帧面 / RNG 证据**不可信**。
2026-11-02 对端实测：`OpenInputDesktop` 返回 0 / err=5（ACCESS_DENIED），`LockApp`(11472) 与 `LogonUI`(7328) 在场、
`GetForegroundWindow()` 为空。本条把「桌面是否真的可用」固化成**开窗前机判**，不靠人回忆；**不许写成可选**。

**判据（三条全部为真才算可用，缺一不可）**

| # | 信号 | 通过条件 | 失败通常意味着 |
|---:|---|---|---|
| ① | 输入桌面可访问 | `OpenInputDesktop(0,false,DESKTOP_READOBJECTS)` 返回非 0（调用后立即 `CloseDesktop`） | 锁屏 / 无交互会话：返回 0，`GetLastWin32Error()` 常见 5（ACCESS_DENIED） |
| ② | 前台窗口不是安全桌面 | `GetForegroundWindow()` 非空，且其所属进程名不是 `LockApp` / `LogonUI` | 锁屏（前台属 `LockApp`）或登录界面（`LogonUI`） |
| ③ | 客户端确实收到字符输入 | 向客户端窗口 `PostMessage(WM_CHAR)` 后，客户端日志新增一行 `key_char` | 桌面看着没锁，但 SDL 没拿到键盘焦点（② 会漏掉这一类） |

> **不要把 ① 当唯一判据**：2026-11-02 在 dev-1 车道同一锁屏会话上实测，① 仍返回非 0
> （`SIG1_OK hWnd=2016`），而 ② 判出前台属 `LockApp`。所以三条必须同时为真。
>
> **① 也有假阴方向**：可用桌面上 ① 仍可能返回 0（如 `SIG1_FAIL err=5`），只看 ① 会把正常环境误判成锁屏
> 而中止开窗。这是**安全方向**（不会采到不可信证据），但会白等窗口。区分「① 单独假阴」与「真锁屏」看另外两条：
> ② 前台属主不是 `LockApp`/`LogonUI`（`SIG2_OK`）且 ③ `WM_CHAR` 后客户端日志新增 `key_char`（`SIG3_OK`）
> ⇒ 桌面其实可用，属 ① 单独假阴；② 属主是 `LockApp`/`LogonUI` 或 ③ 无 `key_char` ⇒ 真锁屏。
> ② 可直接补跑；③ 需要客户端已在运行，能跑则跑。

#### ① 输入桌面可访问（逐字可复制）

```powershell
if (-not ('Win32.InputDesktop' -as [type])) {
Add-Type -Namespace Win32 -Name InputDesktop -MemberDefinition @'
[DllImport("user32.dll", SetLastError=true)]
public static extern System.IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
[DllImport("user32.dll", SetLastError=true)]
public static extern bool CloseDesktop(System.IntPtr hDesktop);
'@
}
$DESKTOP_READOBJECTS = 0x0001
$h = [Win32.InputDesktop]::OpenInputDesktop(0, $false, $DESKTOP_READOBJECTS)
if ($h -eq [System.IntPtr]::Zero) {
  $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
  "SIG1_FAIL err=$err"
} else {
  [void][Win32.InputDesktop]::CloseDesktop($h)
  "SIG1_OK hWnd=$h"
}
```

期望末行 `SIG1_OK ...`；出现 `SIG1_FAIL ...` ⇒ 中止（见本节末「处置」）。

#### ② 前台窗口不是安全桌面（逐字可复制）

```powershell
if (-not ('Win32.Foreground' -as [type])) {
Add-Type -Namespace Win32 -Name Foreground -MemberDefinition @'
[DllImport("user32.dll")]
public static extern System.IntPtr GetForegroundWindow();
[DllImport("user32.dll")]
public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint lpdwProcessId);
'@
}
$fg = [Win32.Foreground]::GetForegroundWindow()
if ($fg -eq [System.IntPtr]::Zero) {
  "SIG2_FAIL err=foreground-window-empty"
} else {
  $procId = 0   # 不要用 $pid：PowerShell 只读自动变量
  [void][Win32.Foreground]::GetWindowThreadProcessId($fg, [ref]$procId)
  $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
  if (-not $proc) {
    "SIG2_FAIL err=foreground-owner-gone pid=$procId"
  } else {
    "SIG2_INFO hwnd=$fg pid=$procId name=$($proc.ProcessName)"
    if ($proc.ProcessName -in @('LockApp','LogonUI')) {
      "SIG2_FAIL err=secure-desktop-owner name=$($proc.ProcessName)"
    } else {
      "SIG2_OK"
    }
  }
}
# 诊断（不参与判定）：LockApp / LogonUI 是否在跑
$secure = @(Get-Process -Name LockApp,LogonUI -ErrorAction SilentlyContinue)
if ($secure) { "SIG2_DIAG secure_process_present=" + (($secure | ForEach-Object { "$($_.ProcessName):$($_.Id)" }) -join ',') }
```

期望同时出现 `SIG2_OK`（且没有 `SIG2_FAIL`）。

#### ③ WM_CHAR 探针（开窗后、采集前）

③ 需要**客户端已经起来**，所以放在开窗之后、任何帧面 / RNG 采集之前。它依赖客户端侧 harness 把收到的字符
写进日志（事件名 `key_char`）；harness 由对端维护，本仓只消费它的日志，日志路径以对端当轮给出的为准
（下面用 `<客户端日志路径>` 占位，替换后再跑）。片段用 pwsh 7 / PowerShell 5.1 均可（只用 `Add-Type` 与 `Get-Process`）。

```powershell
$ProcName   = 'MirClinet'          # 用 MirClientZig 时改成 MirClientZig
$LogPath    = '<客户端日志路径>'
$CharCode   = 0x41                 # 'A'
$TimeoutSec = 5
if (-not ('Win32.CharProbe' -as [type])) {
Add-Type -Namespace Win32 -Name CharProbe -MemberDefinition @'
[DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
public static extern bool PostMessageW(System.IntPtr hWnd, uint Msg, System.IntPtr wParam, System.IntPtr lParam);
'@
}
$WM_CHAR = 0x0102
if (-not (Test-Path -LiteralPath $LogPath)) { "SIG3_FAIL err=log-missing path=$LogPath"; return }
$p = Get-Process -Name $ProcName -ErrorAction SilentlyContinue |
     Where-Object { $_.MainWindowHandle -ne [System.IntPtr]::Zero } |
     Select-Object -First 1
if (-not $p) { "SIG3_FAIL err=no-client-window process=$ProcName"; return }
$hwnd = $p.MainWindowHandle
"SIG3_TARGET hwnd=$hwnd pid=$($p.Id)"
$before = @(Select-String -LiteralPath $LogPath -Pattern 'key_char' -ErrorAction SilentlyContinue).Count
if (-not [Win32.CharProbe]::PostMessageW($hwnd, $WM_CHAR, [System.IntPtr]$CharCode, [System.IntPtr]1)) {
  "SIG3_FAIL err=PostMessage rc=$([System.Runtime.InteropServices.Marshal]::GetLastWin32Error())"; return
}
$deadline = (Get-Date).AddSeconds($TimeoutSec)
$after = $before
while ($after -le $before -and (Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds 250
  $after = @(Select-String -LiteralPath $LogPath -Pattern 'key_char' -ErrorAction SilentlyContinue).Count
}
if ($after -gt $before) { "SIG3_OK key_char +$($after - $before)" }
else { "SIG3_FAIL err=no-key_char timeout=${TimeoutSec}s log=$LogPath" }
```

期望末行 `SIG3_OK key_char +1`（或更多）；出现 `SIG3_FAIL err=no-key_char` ⇒ 桌面看着没锁、
但客户端没拿到键盘输入 ⇒ 中止。

#### 处置（任一不满足）

- **窗口不开**；台账该项记 **`NOT_RUN (desktop-locked)`**，不采集任何帧面 / RNG 证据。
- 台账条目要注明**触发信号**（如 `sig1=SIG1_FAIL`、`sig2=secure-desktop-owner`、`sig3=no-key_char`）
  与是否属「① 单独假阴」，便于事后区分**环境问题**（真锁屏）与**判据问题**（① 误报）；
  处置口径不变：任一不满足即不开窗。
- **不得**记「未通过」（那是环境不满足、不是判据失败，口径同 §6 第一行），也**不得**「跳过即通过」（不跑 ≠ 通过）。
- 先解决桌面（解锁 / 换到有交互的会话 / 等锁屏退出），再从头跑 ①②；③ 在客户端起来后重跑。

> 片段自测（2026-11-02，dev-1 车道，只跑只读探测、未起客户端 / 栈）：① 输出 `SIG1_OK hWnd=2016`；
> ② 在同一锁屏会话上输出 `SIG2_INFO ... name=LockApp` + `SIG2_FAIL err=secure-desktop-owner`；
> ③ 只做了编译与「日志缺失 / 无客户端窗口」两个失败分支的自测（`SIG3_FAIL err=log-missing`、
> `SIG3_FAIL err=no-client-window`），未向任何窗口发键。

---

## 3. W1：启用与还原（`[Admin]`）

### 3.1 改前取基线（备份 + sha256）

> 本机 PowerShell 5.1 没有 `Get-FileHash`；窗口统一用 **pwsh 7**，或下面这段 .NET 写法（5.1/7 都能跑）。

```powershell
$dir = 'E:\tmp\admin-window-20261101'
New-Item -ItemType Directory -Force -Path "$dir\conf" | Out-Null
$p = 'E:\MirServer\M2GameSvr\Server.conf'
$bak = "$dir\conf\Server.conf.before"
Copy-Item -LiteralPath $p -Destination $bak -Force
function Get-Sha256([string]$Path) {
  $h = [System.Security.Cryptography.SHA256]::Create()
  $fs = [System.IO.File]::OpenRead($Path)
  try { ([System.BitConverter]::ToString($h.ComputeHash($fs)) -replace '-','').ToLowerInvariant() }
  finally { $fs.Dispose(); $h.Dispose() }
}
$before = Get-Sha256 $p
"BEFORE_SHA256 $before"
"BEFORE_SIZE " + (Get-Item -LiteralPath $p).Length
"BEFORE_SECTIONS " + ((Get-Content -LiteralPath $p -Raw) -split "`n" | Where-Object { $_ -match '^\[' }).Count
$backupSha = Get-Sha256 $bak
if ($backupSha -ne $before) { throw "备份 sha 与源不一致：$backupSha != $before" }
"BACKUP_SHA256 $backupSha"
```

- 期望（2026-11-01 实测）：`BEFORE_SHA256 ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2`、
  `BEFORE_SIZE 58080`、`BEFORE_SECTIONS 14`。
- 若当天与这些值不同 ⇒ 先停下，确认是不是内容漂移（`docs/window-runbook-20261023.md` §0.6 零触碰口径），
  把实际值写进 README 再继续。

### 3.2 启用：逐字片段 + 追加命令

要加的**就是这三行**（`[Admin]` 段；oracle 没有这段，按 `T-273` 登记）：

```ini
[Admin]
AdminPort=7946
AdminBind=127.0.0.1
```

**为什么是 7946**：本机未占用即可；`AdminBind` 只接受回环 IP 字面量（`127.0.0.0/8` 或 `::1`），
`0.0.0.0` / 局域网 / `localhost` 在**配置解析期**直接拒绝启动（`mir2-rs/bins/game-svr/src/config.rs`
的函数 `parse_admin_bind`）。

现网文件是 UTF-8 BOM + **LF** + **末尾无换行**，所以用「字节追加」而不是 `Add-Content`
（`Add-Content` 在 5.1 会按平台默认编码/换行写，破坏 BOM/CRLF 口径）：

```powershell
$p = 'E:\MirServer\M2GameSvr\Server.conf'
$add = [System.Text.Encoding]::ASCII.GetBytes("`n[Admin]`nAdminPort=7946`nAdminBind=127.0.0.1")
$b = [System.IO.File]::ReadAllBytes($p)
if ([System.Text.Encoding]::ASCII.GetString($b) -match '(?m)^\[Admin\]') { throw '已存在 [Admin] 段，先还原再重来' }
[System.IO.File]::WriteAllBytes($p, [byte[]]($b + $add))
$after = Get-Sha256 $p
"AFTER_SHA256 $after"
"AFTER_SIZE " + (Get-Item -LiteralPath $p).Length
```

- 追加片段 = **43 字节**（`\n` + `[Admin]` + `\n` + `AdminPort=7946` + `\n` + `AdminBind=127.0.0.1`）。
- **期望 after_sha（基于 §3.1 的 before_sha 与同一段追加字节，可复算）**：
  `63d8e49243c9a5e39fa72043c938a48d36815b269a2275b1fb0ef66d923e9526`，`AFTER_SIZE 58123`。
  > 这只是「同一 before 文件 + 同一追加字节」的算式校验；**开窗当天以当天实测为准**，
  > 不要为了凑这个值去改文件。

### 3.3 生效自证（起栈后）

```powershell
# 起栈（窗口主流程见 §4.0；这里只示启用后的自证）
Get-NetTCPConnection -State Listen -LocalPort 7946 -ErrorAction SilentlyContinue |
  Select-Object LocalAddress,LocalPort,OwningProcess
```

- 期望：一行 `127.0.0.1 7946 <game-svr pid>`（`LocalAddress` 必须是 `127.0.0.1`，不是 `0.0.0.0`）。
- 另取一条 `AdminPort=0` 的对照（§4.0 的 OFF 配置）：该端口**无监听**。
- 服务端日志（`E:\MirServer\logs\mir2-server\game-svr.out.log`）在 `AdminPort=0` 时应出现
  「本机 admin 通道未启用（[Admin] AdminPort=0；T-273）」；非 0 时**不**出现该行。
  日志一律「落盘再读」，不要 `| Select-Object -Last N`。

### 3.4 负例：非回环 `AdminBind` 必须拒绝启动

```powershell
# 把 AdminBind 临时改成 0.0.0.0（仍保留 AdminPort=7946），然后**只起 game-svr**验证拒绝启动
$p = 'E:\MirServer\M2GameSvr\Server.conf'
Copy-Item -LiteralPath $p -Destination "$dir\conf\Server.conf.bind-negative.before" -Force
(Get-Content -LiteralPath $p -Raw) -replace 'AdminBind=127\.0\.0\.1','AdminBind=0.0.0.0' |
  Set-Content -LiteralPath $p -NoNewline -Encoding utf8
# 用 -Roles 只起 game-svr；它必须先于其它角色失败 ⇒ 启动器 fail-closed 回滚
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root 'E:\MirServer' -Roles game-svr -RequireRelease *> "$dir\logs\bind-negative.txt"
"EXIT $LASTEXITCODE"
Get-Content -LiteralPath "$dir\logs\bind-negative.txt" -Tail 20
```

- 期望 `EXIT` 非 0，且输出点名 `AdminBind 只接受回环地址` / `拒绝在非本机地址上暴露 GM 执行面`；
  7946 与 5000 **都不在听**。
- **该负例写完立刻还原**（§3.5），再继续后面的项。
- 注意：`Set-Content -Encoding utf8` 在 7 会写 **无 BOM**、在 5.1 会写 **带 BOM**。
  这里只是临时负例文件，跑完就用备份逐字节覆盖回去，不影响 §3.1/§3.5 的 sha 口径。

### 3.5 还原（删段 + 核 sha 回原值）

```powershell
$p   = 'E:\MirServer\M2GameSvr\Server.conf'
$bak = 'E:\tmp\admin-window-20261101\conf\Server.conf.before'
Copy-Item -LiteralPath $bak -Destination $p -Force
$restored = Get-Sha256 $p
"RESTORED_SHA256 $restored"
if ($restored -ne (Get-Sha256 $bak)) { throw "还原失败：$restored != 备份" }
if ((Get-Content -LiteralPath $p -Raw) -match '(?m)^\[Admin\]') { throw '还原后仍有 [Admin] 段' }
```

- 期望：`RESTORED_SHA256` **等于 §3.1 的 `before`**，且 `^[Admin]` 命中 0。
- 也可以用「文本删段」代替整文件覆盖，但**必须**同时满足：字节数回到 58080、sha256 回到 `before`。
  只删段不核 sha 不算还原。
- 还原后再跑一次 `port-status.ps1 -Out`，证明十端口回原样（§7 第 6 条）。

---

## 4. W2：三条判据的可执行命令

### 4.0 公共：起栈 / 停栈 / 取帧

窗口统一用 `-Root 'E:\MirServer'`（配置读生产根，**DB 也是生产库 `mir2_db` / `mir2_account`**）。

> **数据面风险声明**：本执行单里的三条 GM 命令**都不写角色状态**——
> `@怪` 只在内存里刷怪（世界 tick 内、不落库），`@清除怪物` 恒回红（不杀怪、不落库），
> `@LoadAdmin` 只回文本（`LoadAdminList` 被注释），`@Level0` 是**被拒绝的负向对照**（权限不够，不写库）。
> 仍然会在登录/存档链上产生正常的玩家簿记写入 ⇒ 必须按 §4.4 取 `characters_ablity.Level` 前后值，
> 并把「除 Level 外的簿记差异」登记成已知差异，不得声称「DB 零差异」。

**起栈（ON：`AdminPort=7946`；OFF：无 `[Admin]` 段）**

```powershell
$dir = 'E:\tmp\admin-window-20261101'
New-Item -ItemType Directory -Force -Path "$dir\rng","$dir\logs" | Out-Null

# 先停旧栈（必须 rc=0 且打印 MIR2_SERVER_STACK_STOPPED）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/stop_mir2_server.ps1 `
  -Root 'E:\MirServer' *> "$dir\logs\stop-before.txt"
Get-Content -LiteralPath "$dir\logs\stop-before.txt" -Tail 20

# 起栈：种子固定 42；RNG 流水落 <run>.tsv（-RngLog 的父目录必须已存在）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root 'E:\MirServer' -Seed 42 -RequireRelease `
  -RngLog "$dir\rng\<RUN>.tsv" *> "$dir\logs\start-<RUN>.txt"
"EXIT $LASTEXITCODE"
Get-Content -LiteralPath "$dir\logs\start-<RUN>.txt" -Tail 30
```

- `-RngLog` 会把 `MIR2_RNG_LOG=<path>` 与 `MIR2_RNG_SITE=1` 注入六个角色子进程
  （`start_mir2_server.ps1` 的 RNG 流水段；与 `start_rust_stack.ps1` 同口径），
  同时按 `-Seed 42` 注入 `MIR2_RNG_SEED=42`。
  **两个变量缺一，Rust 侧不落流水**（`mir2-rs/crates/shared/src/rng_trace.rs` 的 `enable_from_env`）。
- 流水格式（两侧同形，5 列）：`seq \t kind \t max/arg \t result \t site`；
  头两行是 `INJECT_OK <seed>` 与 `SEED_PROBE …`。
  列里 **没有名叫 `order` 的列**——「顺序」就是 `seq`（写作 `site/order` 时的 order 即指它的先后）。
- 起栈成功判据：脚本打印六角色就绪、rc=0；`Get-NetTCPConnection -State Listen` 能看到 5000/7000/7100/7200。

**取帧（probe，无头客户端）**

```powershell
$env:RUN_PASSWORD = '<从部署账号表读取，不要写进任何文件>'
python -I -B tools/probe/mir2_probe.py `
  --login-gate 127.0.0.1:7000 --account winplay0 --password $env:RUN_PASSWORD `
  --server-name 热血传奇 --char-name winplay0 `
  --scenario "$dir\scenarios\<SCENARIO>.json" `
  --out "$dir\<RUN>" --timeout-ms 20000
"EXIT $LASTEXITCODE"
```

- 产物：`<RUN>\frames.ndjson`（s2c 帧，每行含 `seq/ident/recog/param/tag/series/body_len/body_hex/body_sha256/body_text/frame_class`）、
  `<RUN>\c2s.ndjson`、`<RUN>\run.json`（逐 step 记录；**注意 `sleep` 步只有 `index/kind/ms`，不记 `t_ms`**，
  `send`/`wait_*`/`click_npc`/`dlg_select` 才记）。
- **停栈**（每轮结束、还原配置之前）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/stop_mir2_server.ps1 `
  -Root 'E:\MirServer' *> "$dir\logs\stop-<RUN>.txt"
Get-Content -LiteralPath "$dir\logs\stop-<RUN>.txt" -Tail 20
```

### 4.1 判据①：脚本 `GMEXECUTE` vs admin 通道（同一 Rust 构建、同一 seed）

> **口径澄清**：admin 通道是**本仓新增能力**，C# oracle 没有它，所以这一项的「两侧」是
> **同一 Rust 构建的两次起栈**（run-A = 脚本路径；run-B = admin 路径），不是 C# vs Rust。
> 跨栈（C# 脚本 `GMEXECUTE` vs Rust 脚本 `GMEXECUTE`）属于另一条判据，不在本卡。

**为什么拆成 4 次跑**：两条命令的窗口必须互不重叠，否则「哪几帧/哪几次抽数属于哪条命令」说不清。
每次跑只让一条命令落地，窗口就只有一个。

| run | 触发路径 | 命令 | 说明 |
|---|---|---|---|
| `A-spawn` | 脚本 `GMEXECUTE` | `怪 鸡 1` | 与 `B-spawn` 配对 |
| `B-spawn` | admin `{"chr","cmd"}` | `@怪 鸡 1` | 与 `A-spawn` 配对 |
| `A-clear` | 脚本 `GMEXECUTE` | `清除怪物 3 * 0` | 与 `B-clear` 配对 |
| `B-clear` | admin `{"chr","cmd"}` | `@清除怪物 3 * 0` | 与 `A-clear` 配对 |

四个 run 全部用 **`AdminPort=7946`** 的配置、`-Seed 42`、同一脚本夹具、同一 placement。

#### 4.1.1 脚本夹具（改 `药店-0153.txt`，带 before/after sha）

现网该文件：GBK + **CRLF**、805 字节、sha256
`99f3f0fdf265ca590b5d012154136bfd4c0aabaad882cf03f2f6ce1aeb19052a`（2026-11-01 实测，**当天重取**）。

```powershell
$dir = 'E:\tmp\admin-window-20261101'
New-Item -ItemType Directory -Force -Path "$dir\fixture" | Out-Null
$f = 'E:\MirServer\M2GameSvr\Envir\Market_Def\沙巴克\药店-0153.txt'
Copy-Item -LiteralPath $f -Destination "$dir\fixture\药店-0153.txt.before" -Force
function Get-Sha256([string]$Path) {
  $h=[System.Security.Cryptography.SHA256]::Create(); $fs=[System.IO.File]::OpenRead($Path)
  try { ([System.BitConverter]::ToString($h.ComputeHash($fs)) -replace '-','').ToLowerInvariant() }
  finally { $fs.Dispose(); $h.Dispose() }
}
"FIXTURE_BEFORE_SHA256 " + (Get-Sha256 $f)
"FIXTURE_BEFORE_SIZE " + (Get-Item -LiteralPath $f).Length
```

**注入（不改编码、不改换行）**：在 `[@main]` 之后插两行链接，在文件尾追加两个动作标签。
下面这段在 **Latin1（1 字节=1 字符，逐字节无损）**空间里做插入，中文命令体按 **GBK** 编码后写入，
不会碰原文件的任何其它字节：

```powershell
$f = 'E:\MirServer\M2GameSvr\Envir\Market_Def\沙巴克\药店-0153.txt'
$L = [Text.Encoding]::GetEncoding(28591)   # Latin1：逐字节映射
$G = [Text.Encoding]::GetEncoding(936)     # GBK：脚本正文编码
$s = $L.GetString([IO.File]::ReadAllBytes($f))
$needle = "[@main]`r`n"
if (([regex]::Matches($s, [regex]::Escape($needle))).Count -ne 1) { throw '[@main] 不是恰好一处，先停下' }
$i = $s.IndexOf($needle) + $needle.Length
$link = $L.GetString($G.GetBytes("<GM刷怪/@gmspawn>" + "`r`n" + "<GM清怪/@gmclear>" + "`r`n"))
$s = $s.Insert($i, $link)
$tail = $L.GetString($G.GetBytes(
  "[@gmspawn]`r`n#ACT`r`nGMEXECUTE 怪 鸡 1`r`n" +
  "[@gmclear]`r`n#ACT`r`nGMEXECUTE 清除怪物 3 * 0`r`n"))
$s = $s.TrimEnd() + "`r`n`r`n" + $tail
[IO.File]::WriteAllBytes($f, $L.GetBytes($s))
"FIXTURE_AFTER_SHA256 " + (Get-Sha256 $f)
"FIXTURE_AFTER_SIZE " + (Get-Item -LiteralPath $f).Length
```

- 自检（必须看到）：`Market_Def` 里 `[@main]` 之后紧跟 `<GM刷怪/@gmspawn>` 与 `<GM清怪/@gmclear>`；
  文件末尾是 `[@gmclear] #ACT GMEXECUTE 清除怪物 3 * 0`；**换行仍是 CRLF**、**不含 BOM**。
- **为什么一条 `#ACT` 只放一条 GMEXECUTE**：脚本 `#ACT` 里注册命令执行后会立刻返回，
  一个 `#ACT` 放两条会在第一条之后停下（本仓 memory `script-act-only-first-action-runs`）。
  所以两个动作拆成两个标签、两次 `dlg_select`。
- 还原：

```powershell
Copy-Item -LiteralPath 'E:\tmp\admin-window-20261101\fixture\药店-0153.txt.before' `
  -Destination 'E:\MirServer\M2GameSvr\Envir\Market_Def\沙巴克\药店-0153.txt' -Force
"FIXTURE_RESTORED_SHA256 " + (Get-Sha256 'E:\MirServer\M2GameSvr\Envir\Market_Def\沙巴克\药店-0153.txt')
```

  期望 = `FIXTURE_BEFORE_SHA256`。

#### 4.1.2 场景（窗口时写在 `E:\tmp`，不入库）

`E:\tmp\admin-window-20261101\scenarios\script_spawn.json`：

```json
{
  "name": "admin_sheet_script_spawn",
  "placement": {"map": "0153", "x": 15, "y": 7,
    "note": "城堡药店（MerChant.txt 第 90 行：沙巴克/药店 0153 14 7）正东邻格；0153.map 实测 (15,7) 可走、(15,8) 可走、(16,7) 阻挡"},
  "params": {"self_pos": 458767, "turn_dir": 4},
  "steps": [
    {"send": {"ident": 3010, "recog": "self_pos", "tag": "turn_dir"},
     "note": "CM_TURN 朝下（Direction.Down=4）：身前格 (15,8) 可走 —— @怪 落点就是它"},
    {"sleep": 400},
    {"click_npc": "pos:14,7"},
    {"wait_for": 643, "timeout_ms": 3000},
    {"dlg_select": "@gmspawn", "note": "脚本 #ACT 执行 GMEXECUTE 怪 鸡 1"},
    {"sleep": 2500},
    {"send": {"ident": 3014, "recog": "self_pos", "param": 0, "tag": "turn_dir"},
     "note": "CM_HIT 朝身前格砍一刀（战斗帧面；鸡可能已走开，是否命中不作判据）"},
    {"sleep": 1200}
  ]
}
```

`...\script_clear.json`（只是 label 不同）：

```json
{
  "name": "admin_sheet_script_clear",
  "placement": {"map": "0153", "x": 15, "y": 7},
  "params": {"self_pos": 458767, "turn_dir": 4},
  "steps": [
    {"send": {"ident": 3010, "recog": "self_pos", "tag": "turn_dir"}},
    {"sleep": 400},
    {"click_npc": "pos:14,7"},
    {"wait_for": 643, "timeout_ms": 3000},
    {"dlg_select": "@gmclear", "note": "脚本 #ACT 执行 GMEXECUTE 清除怪物 3 * 0"},
    {"sleep": 2000}
  ]
}
```

`...\admin_spawn.json` / `...\admin_clear.json`（admin 路径；客户端做**同样的点 NPC / 643**
——选帧要拿第一条 643 当锚；命令由外部进程投递）：

```json
{
  "name": "admin_sheet_admin_spawn",
  "placement": {"map": "0153", "x": 15, "y": 7},
  "params": {"self_pos": 458767, "turn_dir": 4},
  "steps": [
    {"send": {"ident": 3010, "recog": "self_pos", "tag": "turn_dir"},
     "note": "CM_TURN 朝下：身前格 (15,8) 可走"},
    {"sleep": 400},
    {"click_npc": "pos:14,7",
     "note": "与脚本路径同一步：选帧要的 643 页锚点，两条路径都产生同一张 @main 页"},
    {"wait_for": 643, "timeout_ms": 3000},
    {"sleep": 6000, "note": "窗口：外部 admin 客户端在此期间投递 @怪 鸡 1（先等 @时间 ok 证明在线）"},
    {"send": {"ident": 3014, "recog": "self_pos", "param": 0, "tag": "turn_dir"}},
    {"sleep": 1200}
  ]
}
```

`admin_clear.json` 同形：`sleep 6000` 窗口里改投 `@清除怪物 3 * 0`，末尾不加 CM_HIT（换成 `sleep 1200`）。

admin 版 `clear` 同形（窗口内投递 `@清除怪物 3 * 0`，窗口后不加 CM_HIT）。

**开跑前离线校验场景**（不连网，rc=0 才算能用；`self_pos` 必须与 placement 对得上）：

```powershell
python -I -B tools/probe/mir2_probe.py --check-scenario --scenario "$dir\scenarios\script_spawn.json"
"EXIT $LASTEXITCODE"
```

#### 4.1.3 admin 通道投递（原始 TCP 一行 JSON）

协议（`docs/mcp-gm-server-20261101.md` §7；实现 `serve_admin_connection`）：回环 TCP、**逐行 JSON**；
请求 `{"chr":"<角色名>","cmd":"@<完整命令行>"}`，响应一行
`{"ok":bool,"error":?string,"frames":N,"note":?string}`。行尾必须换行。

```powershell
function Send-AdminCmd([string]$Chr,[string]$Cmd,[int]$Port = 7946) {
  $c = New-Object System.Net.Sockets.TcpClient
  $c.Connect('127.0.0.1',$Port)
  $s = $c.GetStream()
  $b = [Text.Encoding]::UTF8.GetBytes((@{chr=$Chr;cmd=$Cmd} | ConvertTo-Json -Compress) + "`n")
  $s.Write($b,0,$b.Length); $s.Flush()
  $sr = New-Object IO.StreamReader($s,[Text.Encoding]::UTF8)
  $line = $sr.ReadLine()
  $c.Close()
  return $line
}
```

**等待角色在线**（`unknown_character` 说明还没登录上）：先用**只读、零副作用**的 `@时间` 轮询，
再投递真命令。**绝不要**用 `@怪` 本身轮询——重试会刷出多只怪，污染判据。

```powershell
$dir = 'E:\tmp\admin-window-20261101'
$log = "$dir\logs\admin-spawn.jsonl"
$deadline = (Get-Date).AddSeconds(30)
do {
  $r = Send-AdminCmd 'winplay0' '@时间'
  if ($r -match '"ok":\s*true') { break }
  "WAIT $r" | Out-File -FilePath $log -Append -Encoding utf8
  Start-Sleep -Milliseconds 300
} while ((Get-Date) -lt $deadline)
if ($r -notmatch '"ok":\s*true') { throw "角色一直不在线：$r" }
$spawn = Send-AdminCmd 'winplay0' '@怪 鸡 1'
"SPAWN $spawn" | Out-File -FilePath $log -Append -Encoding utf8
$spawn
```

- `SPAWN` 期望 `{"ok": true, "error": null, "frames": 0, "note": null}`：
  `frames` 只数**直接应答帧**，不含事件末 drain 产生的刷怪广播帧（`AdminGmReply` 的文档口径）。
  **`frames=0` 不代表没刷**——刷怪帧是发给客户端的，要看 probe 的 `frames.ndjson`。
- `@清除怪物` 期望 `{"ok": true, ..., "frames": 1, "note": "输入的地图不存在!!!"}`（红字由 drain 之外的回帧路径发）。

#### 4.1.4 帧面比对（按「帧归属」选帧，逐字段）

两条路径的客户端动作不同（脚本路径多一次点 NPC / 对话框），所以**不是整条流对比**。

**不能用 step 时间窗切**：`mir2_probe.py` 的 `sleep` 步只记 `{"index","kind","ms"}`、**不记 `t_ms`**
（驱动器的 `if kind == "sleep"` 分支），按 sleep 下标取 `steps[i]["t_ms"]` 会 `KeyError`；
而且两条路径的对话框帧本来就不同，时间窗即使能取到也对不齐。

⇒ 改成**按帧归属选帧**（与时间、step 类型都无关）：

- 先定位本 run 的「第一张商人页」：`frames.ndjson` 里第一条 `ident == 643` 的下标 `k`；
- `pre` = 该帧之前出现过的所有 `recog`（= 进场快照里已有的对象）；
- **`@怪`**：选 `k` 之后、`ident ∈ {10,41}` 且 `recog ∉ pre` 的帧 —— 即**新生成怪物**的出生帧组；
- **`@清除怪物`**：选 `k` 之后、`ident == 100` 且 `param == 14591`（`0x38FF` 红字）的帧 —— 即命令回的红字。

选择器（一行命令；参数 = run 目录、输出目录、模式 `spawn|clear`）：

```powershell
python -I -c "import json,io,os,sys;d,o,m=sys.argv[1],sys.argv[2],sys.argv[3];fr=[json.loads(l) for l in io.open(d+'/frames.ndjson',encoding='utf-8') if l.strip()];k=next((i for i,f in enumerate(fr) if f.get('ident')==643),None);assert k is not None,'no SM_MERCHANTSAY(643): click did not produce a page';pre={f.get('recog') for f in fr[:k]};sel=[f for f in fr[k+1:] if ((m=='spawn' and f.get('ident') in (10,41) and f.get('recog') not in pre) or (m=='clear' and f.get('ident')==100 and f.get('param')==14591))];os.makedirs(o,exist_ok=True);io.open(o+'/frames.ndjson','w',encoding='utf-8').write(''.join(json.dumps(f,ensure_ascii=False)+chr(10) for f in sel));print('SELECT',o,'mode',m,'frames',len(sel))" `
  "$dir\A-spawn" "$dir\wins\A-spawn\rust" spawn
```

- 期望 `frames >= 1`；两边都 ≥1 才有可比性。`frames 0` ⇒ 命令没落地 / 怪物名不可生成 / 红字没出现
  ⇒ **UNDECIDABLE**（按 §6 记），**不要**拿「零 == 零」当等价。
- 选帧目录放成 `wins\<对比名>\rust` 与 `wins\<对比名>\csharp` 两个子目录，再跑比较器：

```powershell
python -I -B tools/conformance/compare_runs.py compare `
  --rust "$dir\wins\A-spawn\rust" --oracle "$dir\wins\B-spawn\csharp" `
  --out "$dir\wins\A-vs-B-spawn\report.json"
"EXIT $LASTEXITCODE"
```

- 默认**已掩码**：`recog`（会话相关 ActorId）、`t_ms`（墙钟）、`seq`（驱动器本地序号）、`conn`；
  body 与 `param/tag/series/ident` **逐字节比**。掩码都会在 `report.json` 的
  `whitelisted` / `coverage.global_masks` 里显式登记，不是静默放行。
- 判据：`EXIT 0`（PASS）**且** `report.json` 的 `undecidable` 为空。
  `EXIT 2` = 判不了（含 `UNCLASSIFIED`/空集合/缺字段），**不是通过**。
- **`@清除怪物` 的特别口径**：它**恒回红**（杀怪循环整段注释），所以本项的帧判据只要求
  「两侧各恰好 1 条红字 `SM_SYSMESSAGE(100)`，`Param=14591`，正文 `输入的地图不存在!!!`」，
  **不得**把它写成「清怪成功 / 怪物被清掉」。RNG 面**不作判据**（该命令不抽数，空账本不能自证零抽数）。

#### 4.1.5 RNG 账本比对（`site` + 顺序）

工具与口径（本仓既有，**不要自造参数**）：

- 账本：`<run>.tsv`（§4.0 的 `-RngLog`）。列 = `seq / kind / max / result / site`。
- 站点分类与顺序比对：`mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py`
  （同目录 `rng_consumer_groups.py` + `consumer_groups.tsv` 是唯一站点组表）。

```powershell
$rl = 'mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py'

# 1) 组账本必须闭合（有解析不了的行/未归类站点 ⇒ rc≠0，不许当绿）
python -I -B $rl groups "$dir\rng\A-spawn.tsv" --side rust --require-closed --top 20
"EXIT $LASTEXITCODE"
python -I -B $rl groups "$dir\rng\B-spawn.tsv" --side rust --require-closed --top 20
"EXIT $LASTEXITCODE"

# 2) 组集合 + 共享组相对次序（--strict：不一致即 rc=1；--fold：construct->spawn 的登记折叠）
python -I -B $rl groupdiff "$dir\rng\A-spawn.tsv" "$dir\rng\B-spawn.tsv" `
  --a-side rust --b-side rust --strict --fold --include-attack --json "$dir\rng\A-vs-B-spawn.json"
"EXIT $LASTEXITCODE"

# 3) 更强一档：整条站点序列逐位比（若 IDENTICAL，直接升级为「顺序全等」证据）
python -I -B $rl diff "$dir\rng\A-spawn.tsv" "$dir\rng\B-spawn.tsv" `
  --a-side rust --b-side rust --json "$dir\rng\A-vs-B-spawn.diff.json"
"EXIT $LASTEXITCODE"
```

判据（**三条同时满足**才算这一面绿）：

1. 两个 `groups --require-closed` 都 rc=0，且输出里 `GROUP … spawn` 计数 **≥1**（否则这条命令没抽到数，
   本项变成空判据 ⇒ UNDECIDABLE，先查命令是否落地/怪物名是否可生成）；
2. `groupdiff --strict --fold` **rc=0** 且 `VERDICT IDENTICAL`（`GROUP_SET_DIFF` / `SHARED_REL_ORDER_DIFF` /
   `FOLDED_COUNT_DIFF` / `ROW_COVERAGE_DRIFT` 任一出现即红）；
3. `diff` 的 `VERDICT IDENTICAL` 记为**加强证据**；若它是 `ORDER_DIVERGENCE` 但首处分歧落在
   **非 spawn 组**（AI/商人等噪声站点），把该事实与首个分歧下标原样记进证据，
   并**重复**本 run 一次；仍不一致 ⇒ 本面记 **UNDECIDABLE（噪声不可分离）**，不得记通过。

> `@清除怪物` 这一对（A-clear / B-clear）的 RNG 期望是「两侧都**没有**新增抽数」。
> 空账本不能自证「零抽数」——所以本对**不以 RNG 判据**，以帧面 + §4.1.4 的红字为准，
> 并在证据里写明「oracle 杀怪循环整段注释 ⇒ 本命令不抽数、不改变世界」。

### 4.2 判据②：`AdminPort=0` 与 `=7946` 两种配置下客户端帧面 diff = 0

- **ON**：`[Admin] AdminPort=7946` + 脚本夹具，跑 `script_spawn.json`（含 CM_TURN / 点 NPC / 刷怪 / CM_HIT），
  这条流同时覆盖**登录 + 移动（转向）+ 战斗（CM_HIT）**。记作 `run-on`。
- **OFF**：§3.5 还原配置后（**没有 `[Admin]` 段**，即 `AdminPort=0`）用同一脚本夹具、同一场景、
  同一 `-Seed 42` 再跑一次。记作 `run-off`。
- 比对**整条**帧流（不做选帧/不做窗口）：

```powershell
python -I -B tools/conformance/compare_runs.py compare `
  --rust "$dir\run-on" --oracle "$dir\run-off" --out "$dir\run-on-vs-off\report.json"
"EXIT $LASTEXITCODE"
Get-Content -LiteralPath "$dir\run-on-vs-off\report.json" -TotalCount 40
```

- 判据：`EXIT 0` 且 `undecidable` 为空且两侧帧数相等。
- 两轮之间必须：停栈 → 改/还原 `Server.conf` → 重新起栈。**不要**在同一栈内改配置
  （`AdminPort` 只在**启动装载**时读）。
- 配套自证（同一次 ON run 里取）：
  - ON：`Get-NetTCPConnection -State Listen -LocalPort 7946` 有 `127.0.0.1:7946`；
  - OFF：无该监听，且 `Send-AdminCmd` 连 7946 **连不上**（`admin_connect_failed` 级别的连接失败）；
  - 非回环拒绝启动：§3.4 的负例（`AdminBind=0.0.0.0` ⇒ 起不来）。

### 4.3 判据③：`@LoadAdmin` 后退出重登，会话权限回原值

- 权限来源（`mir2-rs/bins/game-svr/src/gmcmd.rs`）：`CommandDef { name: "LoadAdmin", perm_min: 10, params: 1, imp: Imp::LoadAdmin }`；
  命令体只看 `sess.permission < 6`（门 10 之后是死支），成功回绿 `管理员列表重新加载成功...`。
  admin 通道在**这次调用内**把权限抬到 10，**执行完立刻还原**（`execute_admin_gm_command`：`old` → `ADMIN_GM_PERMISSION` → `execute` → `old`）。
- **可观测的「权限」= 权限门本身**：用 `@Level0 5`（`ChangeLevel`，`PermissionMin=10`）做探针。
  三次都要**被拦**（红字 `SM_SYSMESSAGE(100)`、`Param=14591`、正文 `权限不够!!!`）：
  1. **基线（重登前）**：证明这条会话本来就没有权限；
  2. **单次调用还原（同会话、softclose 之前）**：`@LoadAdmin` 走完 admin 通道后**立刻**再发一条，
     证明「抬权只作用于那一次调用」——这一条不能省：**重登会从 DB 重载权限**，
     只看重登后无法区分「单次还原」和「根本没抬过权」；
  3. **重登后**：证明没有把权限写进持久状态。
- 另配 `characters_ablity.Level` 前后不变（三次都被拦 ⇒ 命令体从未执行 ⇒ 等级不该动）。

场景 `E:\tmp\admin-window-20261101\scenarios\perm_roundtrip.json`：

```json
{
  "name": "admin_sheet_perm_roundtrip",
  "placement": {"map": "0153", "x": 15, "y": 7},
  "params": {"self_pos": 458767, "turn_dir": 4},
  "steps": [
    {"say": "@Level0 5", "note": "① 基线：权限门必须拦（不该进命令体）"},
    {"wait_for": 100, "timeout_ms": 3000},
    {"sleep": 6000, "note": "窗口：外部 admin 客户端在此期间投递 @LoadAdmin（先等 @时间 ok 证明在线）"},
    {"say": "@Level0 5", "note": "② 同会话、softclose 之前：证明单次调用已还原（不是靠重登重载）"},
    {"wait_for": 100, "timeout_ms": 3000},
    {"send": {"ident": 1009, "recog": 0, "param": 0}, "note": "CM_SOFTCLOSE 小退"},
    {"sleep": 300},
    {"relogin": {"settle_ms": 1500, "assert_same_actor": false}},
    {"say": "@Level0 5", "note": "③ 重登后：权限必须仍是原值（仍被拦）"},
    {"wait_for": 100, "timeout_ms": 3000},
    {"sleep": 800}
  ]
}
```

执行顺序（外部投递与 probe **同时**开始；投递器先轮询 `@时间` 等在线，再发 `@LoadAdmin`）：

```powershell
$job = Start-Job -ScriptBlock {
  param($chr,$port,$log)
  function Send-AdminCmd([string]$Chr,[string]$Cmd,[int]$Port) {
    $c = New-Object System.Net.Sockets.TcpClient
    $c.Connect('127.0.0.1',$Port)
    $s = $c.GetStream()
    $b = [Text.Encoding]::UTF8.GetBytes((@{chr=$Chr;cmd=$Cmd} | ConvertTo-Json -Compress) + "`n")
    $s.Write($b,0,$b.Length); $s.Flush()
    $sr = New-Object IO.StreamReader($s,[Text.Encoding]::UTF8)
    $line = $sr.ReadLine(); $c.Close(); return $line
  }
  $deadline = (Get-Date).AddSeconds(40)
  do {
    $r = Send-AdminCmd $chr '@时间' $port
    if ($r -match '"ok":\s*true') { break }
    Start-Sleep -Milliseconds 300
  } while ((Get-Date) -lt $deadline)
  $r2 = Send-AdminCmd $chr '@LoadAdmin' $port
  "LOADADMIN $r2" | Out-File -FilePath $log -Append -Encoding utf8
  $r2
} -ArgumentList 'winplay0',7946,"$dir\logs\admin-loadadmin.jsonl"

python -I -B tools/probe/mir2_probe.py --login-gate 127.0.0.1:7000 --account winplay0 `
  --password $env:RUN_PASSWORD --server-name 热血传奇 --char-name winplay0 `
  --scenario "$dir\scenarios\perm_roundtrip.json" --out "$dir\RUN-perm" --timeout-ms 20000
"PROBE_EXIT $LASTEXITCODE"
Receive-Job -Job $job -Wait
Remove-Job -Job $job
```

- `LOADADMIN` 期望 `{"ok": true, "error": null, "frames": 1, "note": "管理员列表重新加载成功..."}`。
  回 `unknown_character` ⇒ 投递早了（探针还没进世界），重跑，**不要**改判据。
- 帧面判据（整条流，单侧）：

```powershell
python -I -c "import json,io;fr=[json.loads(l) for l in io.open(r'$dir\RUN-perm\frames.ndjson',encoding='utf-8') if l.strip()];s=[f for f in fr if f.get('ident')==100 and f.get('param')==14591];bad=[f.get('ident') for f in fr if f.get('ident') in (200,202,622,45,52,752)];print('RED100_COUNT',len(s));print('RED100_TEXTS',[f.get('body_text') for f in s]);print('FORBIDDEN',bad);print('VERDICT', 'PASS' if len(s)==3 and all('权限不够!!!' in (f.get('body_text') or '') for f in s) and not bad else 'FAIL')"
```

  期望：`RED100_COUNT 3`（基线 / 单次调用后 / 重登后各一条）、三条正文都含 `权限不够!!!`、
  `FORBIDDEN []`、`VERDICT PASS`。任何一条 45/52/752（等级/属性帧）出现 ⇒ 权限门被绕过 ⇒ FAIL。
  **必须按 `param == 14591` 过滤**：`ident == 100` 的帧还包含登录/公告类 SysMsg（例如绿字 `Param=65499`），
  不按 param 过滤会把它们算进条数，判据恒不成立。
- DB 面（前后各取一次，跑完比对）：`characters_ablity` 只有 `PlayerId / Level / ModifyTime`，
  **没有 `ChrName`、没有 `LastUpdate`**（`DESCRIBE mir2_db.characters_ablity` 实测）⇒ 必须 join `characters`：

```powershell
$mysql = 'D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe'
$sql = "SELECT c.ChrName, a.Level, a.ModifyTime FROM mir2_db.characters c JOIN mir2_db.characters_ablity a ON a.PlayerId = c.Id WHERE c.ChrName='winplay0';"
& $mysql -h127.0.0.1 -P3306 -uroot -N -e $sql | Tee-Object -FilePath "$dir\db\level-before.txt"
# ……跑完……
& $mysql -h127.0.0.1 -P3306 -uroot -N -e $sql | Tee-Object -FilePath "$dir\db\level-after.txt"
Compare-Object (Get-Content "$dir\db\level-before.txt") (Get-Content "$dir\db\level-after.txt")
```

  - `Level` 必须相同（期望 1）。
  - `ModifyTime` 会因正常登录/存档链变化 —— 记成**已知簿记差异**，不作为判据，也不能据此说「DB 无差异」。
- `tools/window/gm_gate_check.py` **不能直接用于本项**：它按 `<run-dir>/csharp|<side>/frames.ndjson`
  取两侧、且 G1 要求 **恰 1 条** 100；本项是单侧、三条 100（基线 / 单次调用后 / 重登后）。
  如要复用，只能作为「G2/G3 口径参考」，结论以上面的单侧断言为准。
  > **已登记的工具偏差（不在本卡写集，单独开卡）**：`gm_gate_check.py` 的
  > `RED_PARAM = (255 << 8) | 56 = 65336` 与本机实现相反——`smframe.rs` 的
  > `make_word16(f_color, b_color) = f_color | (b_color << 8)`，红字实际是 **`0x38FF = 14591`**
  > （`lib.rs` 单测逐个钉 `0x38FF`）。⇒ 该工具的 G1 判据按现网帧**不可能通过**；
  > 本执行单所有判据一律用 `14591`，不要用 `65336`。

### 4.4 收窗顺序（每次窗口都一样）

1. 停栈（`stop_mir2_server.ps1`，rc=0 且打印 `MIR2_SERVER_STACK_STOPPED`）；
2. 还原 `药店-0153.txt`（§4.1.1 末段），核 `FIXTURE_RESTORED_SHA256 == FIXTURE_BEFORE_SHA256`；
3. 还原 `Server.conf`（§3.5），核 sha 回 `before`、`^[Admin]` = 0；
4. 重跑 `port-status.ps1 -Out <dir>\port-after.txt`，与 `port-before.txt` 对齐十端口；
5. 按需重新起生产栈（用窗口同一条命令，`-RngLog` 不带）；
6. 清 `E:\tmp\admin-window-20261101` 以外的临时物，或把该目录登记进交付报告「残留物清单」。

---

## 5. W3：证据四件套与记录

每条判据的每个 run 在 `E:\tmp\admin-window-20261101\<RUN>\` 下落**四件套**
（命令原文 / 输出 / 路径 / sha256）：

| 文件 | 内容 |
|---|---|
| `cmd.txt` | 本 run **逐字**执行过的命令（含 `-Root` / `-Seed` / `-RngLog` / `--scenario` 全参），一行一条 |
| `*.out.txt` / `*.err.txt` | 起栈/停栈/probe 的原始 stdout/stderr（**落盘再读**，禁止管道截断） |
| 产物 | `frames.ndjson`、`c2s.ndjson`、`run.json`、`<RUN>.tsv`（RNG 账本）、`report.json`（比较器）、`*.diff.json`（RNG） |
| `SHA256SUMS` | 上面每个文件的 sha256，一行一个（`<hex>  <相对路径>`） |

生成与校验：

```powershell
# 记 sha（Git Bash）
cd /e/tmp/admin-window-20261101 && sha256sum -b $(find . -type f ! -name SHA256SUMS) > SHA256SUMS
# 校验
cd /e/tmp/admin-window-20261101 && sha256sum -c SHA256SUMS
```

**`README.txt`（窗口级，唯一入口）** 必须写死这几行：

```text
WINDOW admin-window-20261101
TREE      <仓库绝对路径>
HEAD      <git rev-parse HEAD>
SERVER_EXE <路径>
SERVER_EXE_SHA256 <小写 hex>
BUILD     Release
SEED      42
CONF_BEFORE_SHA256 ce5e3ce98b9bb71cbb18e6be1501e20e05642fd3eeaaffd9326caef8d9a4c0e2
CONF_AFTER_SHA256  <启用后的实测值>
FIXTURE_BEFORE_SHA256 99f3f0fdf265ca590b5d012154136bfd4c0aabaad882cf03f2f6ce1aeb19052a
FIXTURE_AFTER_SHA256  <注入后的实测值>
TOOLGATE_INVENTORY 66 = 61 + 5 + 0（rc=0）
NOTES     <口令等凭据不得出现在本文件>
```

**台账里怎么引用**：在 `openmir2-real-machine-verification-master` 下签一条 `comment`，逐项写
`window admin-window-20261101 / run <RUN> / 证据根 E:\tmp\admin-window-20261101\<RUN> / SHA256SUMS 的 sha256 / 结论`。
**不要把结论写进 `status` 之外的散文**；卡片状态只在真正变化时签。

### 5.1 每条判据的最小证据清单

| 判据 | 必须有的文件 |
|---|---|
| ① `@怪` 脚本 vs admin | `A-spawn/`、`B-spawn/`、`wins/A-vs-B-spawn/report.json`、`rng/A-spawn.tsv`、`rng/B-spawn.tsv`、`rng/A-vs-B-spawn.json`（groupdiff）、`rng/A-vs-B-spawn.diff.json`、两端 `SHA256SUMS` |
| ① `@清除怪物` 脚本 vs admin | `A-clear/`、`B-clear/`、`wins/A-vs-B-clear/report.json`；两端 `groups` 输出**只作观测、不作判据**（该命令不抽数，空账本不能自证零抽数，见 §4.1.5） |
| ② OFF vs ON | `run-on/`、`run-off/`、`run-on-vs-off/report.json`、7946 监听对照（ON 有 / OFF 无）、§3.4 负例 `logs/bind-negative.txt` |
| ③ `@LoadAdmin` 重登 | `RUN-perm/`、`logs/admin-loadadmin.jsonl`、`db/level-before.txt`、`db/level-after.txt`、单侧断言输出 |
| 配置改动 | `conf/Server.conf.before`、启用后 `Server.conf` 副本、`conf/RESTORED_SHA256` |
| 脚本夹具 | `fixture/药店-0153.txt.before`、注入后副本、`FIXTURE_*` 两组 sha |

---

## 6. 失败 / 半通过怎么记（不许写「语义等价」）

| 现象 | 记法 |
|---|---|
| 命令没落地（`unknown_character` / 角色不在线 / 场景超时） | **`NOT_RUN`（前置不满足）** + 原始回包与日志路径。**不是失败，也不是通过。** |
| 选帧为空 / 账本无 spawn 抽数（空判据） | **`UNDECIDABLE(empty)`** + 说明「命令是否落地」的证据。**绝不**把「零 == 零」记成等价。 |
| `compare_runs` rc=2 | **`UNDECIDABLE`**，把 `report.json` 的 `undecidable[]` 逐条抄出来；不降级成 PASS。 |
| `compare_runs` rc=1 | **`FAIL`**，逐帧写清 ident / 帧序号 / 字节偏移 / 首个不同字节（工具已给）。 |
| RNG `groupdiff` 出旗 | **`FAIL`**，点名 `GROUP_SET_DIFF` / `SHARED_REL_ORDER_DIFF` / `FOLDED_COUNT_DIFF` / `ROW_COVERAGE_DRIFT` 与首个分歧。 |
| RNG 首处分歧在噪声站点 | **`UNDECIDABLE(noise)`**，写下首个分歧下标、站点名、重跑次数。**不许**写「语义等价 / 看起来一样 / 与 oracle 同构」。 |
| 权限门被绕过（出现 45/52/752） | **`FAIL`**（`T-273` 的「抬权只作用于单次调用」被证伪），保留现场，转 `needs-human`。 |
| 非回环 `AdminBind` 竟然起来了 | **`FAIL`**（安全边界被证伪），立即停栈，转 `needs-human`。 |
| DB `Level` 变了 | **`FAIL`**；`ModifyTime` 等簿记列变化单列为**已知差异**。 |
| 中途停了 cargo / 有人动了树 | 本次窗口**判作废**（`VOID`），重开窗，不挑「好的那几条」上报。 |

**红线**：
- 不许用「语义等价 / 代码看起来一样 / 与 oracle 同构」盖掉帧面或抽数差异；
- 不许只跑一半就写「基本通过」；
- 不许在台账/看板把本卡或 `T-273` 写成「已验收」。

---

## 7. 开窗当天用这些数字认树（重取，不抄）

| 项 | 命令 | 2026-11-01 本卡实测 / 计划文件值 |
|---|---|---|
| HEAD | `git rev-parse HEAD` | 本条窗口钉住的提交（写进 README） |
| 工具门清点 | `python -I -B tools/hygiene/tool_selftest_gate.py --inventory` | **66 = 61 + 5 + 0**，rc=0（**本卡实测**） |
| 工具门自测 | `python -I -B tools/hygiene/tool_selftest_gate.py` | 61 项全绿（计划文件口径；本卡未跑） |
| goal 自测 | `python -I -B tools/goal/goal_gate.py selftest` | 85 条断言（计划文件口径；本卡未跑） |
| workspace 测试 | `cargo test --workspace` | 以 tester 车道证据条目为准（**本卡未跑；窗口期禁 cargo**） |
| 二进制 | `start_mir2_server.ps1 -Root 'E:\MirServer' -WhatIf -RequireRelease` | 本卡实测 `Release` + sha `c01e864595fa2dd3bf91fb4ee27ef134b06dbc33a2ef8f2adc63f2363bb78bb2`（**当天重建后重取**） |
| 端口 | `E:\MirServer\port-status.ps1 -Out <文件>` | 十端口在听 + 3306 UP（本卡观察：在听，且跑的是 `mir2-server.exe` 六角色） |
| 现网 `[Admin]` | `grep -c '^\[Admin\]' 'E:/MirServer/M2GameSvr/Server.conf'` | **0**（本卡实测；开窗改前也必须是 0，否则先还原） |

---

## 8. 本文件覆盖不到的（明说，不假装）

1. **跨栈（C# 脚本 `GMEXECUTE` vs Rust 脚本 `GMEXECUTE`）的抽数/帧面对拍** —— 不在本执行单范围，
   且 C# 侧需要 rng-hook（`MIR2_RNG_SITE=1`）才有 5 列账本；另开卡。
2. **恢复后 `Castle/List.txt` 的 600s 重写漂移** —— runbook §0.9/§0.10 已覆盖，本单只在收窗核对
   十端口与两份 sha，不重复处理 castle 文件。
3. **MCP 客户端侧（`mir2-mcp` 的 `gm_execute`）本身的端到端** —— 已有构造级用例；
   真机本单用**原始 TCP** 走同一协议，`gm_execute` 的 stdio 链路不在这里验。
4. **`@怪` 的怪物名不存在 / race 不可生成时「零抽数 + 红字」的分支** —— 本单固定用 `鸡`（Race 51，可生成）；
   该负例属构造级，不在真机项内。

---

## 9. 一句话交接

开窗当天顺序：**认树 → 备份 + 记 sha → 注入脚本夹具 → 停栈 → ON 起栈 → 跑 A/B-spawn、A/B-clear、
run-on、RUN-perm → 停栈 → 删 `[Admin]` → OFF 起栈 → 跑 run-off → 停栈 → 负例 `AdminBind=0.0.0.0` →
还原两份文件并核 sha → port-after → 写 README + SHA256SUMS → 台账 comment**。

全部 `NOT_RUN`，跑过之前不得写成已验收。
