# window WMI 派生栈的环境变量转发 + pin 自证（2026-10-24）

- 卡：`openmir2-window-wmi-env-forwarding`（tier=L）；执行者 openmir2-worker-75；交付分支 `card/window-wmi-env-forwarding`。
- 基线：`walgit/master` 开工 = `fbd2fa306`；交付前两轮 rebase 跟到最新 `fa8113253`（Gold 通道 968ede63c、场景批2 11c024583、53 分诊合并），均零冲突重放；
  `git merge-tree --write-tree walgit/master HEAD` rc=0（tree=`7b1c8f677…`）。
- 背景（53 分诊实锤，`openmir2-compare-ident53-diff-triage` 线 worker-74 分诊 + orchestrator 背书）：
  game-svr.out.log `05:57:06.620 [INF] CLIENT_ABILITY_LAYOUT layout=client-58（MIR2_CLIENT_ABILITY_LAYOUT=<unset>）`
  ——`start-rust-warmup.ps1` 经 WMI `Win32_Process.Create` 派生 server，**父进程是 WmiPrvSE，不继承调用方环境**；
  profile `windowmagic-ignore14` 里 `sides.rust.env.MIR2_CLIENT_ABILITY_LAYOUT=0` 到 server 就丢。
  比失败更危险的是不报错：所有 env-pin 场景（combat_strike 要 pin=0、53 复跑前置等）**静默跑错布局**。
- **写集**：`tools/window/start-stack-wmi.ps1`（env 转发 + pin 自证）、`tools/window/smoke-wmi-env.ps1`（新增，冒烟）、
  `tools/conformance/card_window.py`（wmi_stack_argv 扩参 + window-magic 收紧 + 加载期守卫 + selftest 24 节）、
  `tools/conformance/preflight.py`（⑥ pin 步 + selftest ⑥）、本文件。
  **零** `src/**`/C# 改动（`git diff walgit/master --stat` 仅上列 4 文件 + 本报告）、
  **零**场景 JSON 改动、**零** rules.json/53 卡结论触碰、**零** Rust 变更。

## 1. 缺口 → 修法

| 环节 | 缺口 | 修法（本卡） |
|---|---|---|
| card_window 调用面 → WMI 包装 | 正常：`subprocess.run(env=…)` 把 `sides.<side>.env` + `CARD_WINDOW_*` 传到包装进程 | 不变 |
| WMI 包装 → 内层栈脚本 | `Win32_Process.Create` 父进程是 WmiPrvSE，**调用方 env 全丢** | `Win32_ProcessStartup.EnvironmentVariables` 显式下发**调用方全量环境**（该属性是整体替换，故必须全量，否则 SystemRoot/PATH 等系统变量反丢；另补基座填齐：调用方 env 缺 SystemRoot/ComSpec/PATHEXT/TEMP/TMP/PATH 时补 Windows 约定值，防被裁剪过的 orchestrator env 二次坏） |
| 内层栈脚本 → mir2-server | `Start-Process` 正常继承（start_mir2_server.ps1 的 Start-ChildProcess） | 不变 |
| pin 是否真到达 | **不可观测**——丢了也照常起栈 | 包装新增 `-PinChecks KEY=VAL,…` + `-PinLog <server out 日志>`：① spawn **之前**验 KEY 已在调用方 env 且值 == VAL（缺/不一致 exit 1，不派生）；② 起栈后扫 server 日志自证行：必须出现 `KEY=VAL`（词边界正则），`MIR2_CLIENT_ABILITY_LAYOUT` 另验生效布局 token（`0|false` ⇒ `layout=oracle-40`，否则 ⇒ `layout=client-58`）；③ 任一不满足 rc=1 并打印观测到的自证行。读日志走 `FileShare.ReadWrite` 容错（server 可能仍在写） |

实现注记（2026-10-24 实测）：CIM cmdlet 路线（`New-CimInstance -ClientOnly` + `Invoke-CimMethod`）
对 `Win32_ProcessStartup` 的 string[] 属性序列化有 bug（WBEM_E_NOT_FOUND: `root\cimv2:Win32_Process:String`）；
`[wmiclass].Create()` 短重载对带 startup 的调用返回 rc=21；**`GetMethodParameters` + `InvokeMethod` 可行**（wrapper 采用）。

## 2. 转发清单（acceptance ①）

转发**实现** = 调用方全量 env（这天然是「仓内已用 pin」的超集，不会漏）。清单按消费面 grep 固化：

- **rust 消费面（28 个，`git grep -oE '"MIR2_[A-Z0-9_]+"' -- mir2-rs/bins mir2-rs/crates` 去重）**：
  MIR2_ACCOUNT_DSN、MIR2_ACCOUNT_TEST_DSN、MIR2_BASE_DIR、MIR2_CLIENT_ABILITY_LAYOUT、
  MIR2_CLIENT_MOVEFAIL_LAYOUT、MIR2_CLOCK_MS、MIR2_DATA_URL、MIR2_DBSRV_PORT、MIR2_ENVIR_DIR、
  MIR2_FLOW_DUMP、MIR2_GAMESVR_DIR、MIR2_GAME_GATE_CHECK_SERVER_TIMEOUT_MS、MIR2_GAME_GATE_KEEPALIVE_MS、
  MIR2_GAME_GATE_MAX_SESSION、MIR2_GAME_GATE_SESSION_IDLE_MS、MIR2_GAME_GATE_SESSION_SWEEP_MS、
  MIR2_MAP_DIR、MIR2_MONGEN_DIR、MIR2_MONITEMS_ENVIR、MIR2_ORACLE_DBSRV_DIR、MIR2_ORACLE_SELGATE_DIR、
  MIR2_REAL_MAPINFO、MIR2_REAL_SERVERCONF、MIR2_RNG_ISOLATE_ATTACK、MIR2_RNG_LOG、MIR2_RNG_SEED、
  MIR2_RNG_STATE_LOG、MIR2_TEST_DB_DSN；
  外加调用面注入的 `CARD_WINDOW_SIDE/_SIDE_DIR/_OUT/_DB_DIR/_SCENARIO/_CARD/_GIT_BASH`（7 个）。
  （probe/bot 侧的 MIR2_BOT_*、MIR2_CLIENT_EXE 等不经本 WMI 链，全量转发同样覆盖。）
- **可观测接线（有 server 日志自证行的 pin）**：`MIR2_CLIENT_ABILITY_LAYOUT`（game-svr 启动自证行
  `CLIENT_ABILITY_LAYOUT layout=…（MIR2_CLIENT_ABILITY_LAYOUT=…）`，真源 `smframe::client_layout_probe_line`）。
  其余 pin 的到达由全量转发保证 + `MIR2_RNG_SEED`/`MIR2_CLOCK_MS` 本就有各自的自证日志行可人查。

## 3. profile 接线与加载期守卫

- `wmi_stack_argv(inner, ports, pin_checks=…, pin_log=…)` 追加 `-PinChecks/-PinLog`；
  `with_wmi_pin_checks(argv, …)` 在既有 WMI argv 上幂等接线；`wmi_pin_checks(argv)` 读回（非 WMI 链 ⇒ None）。
- `window-magic` 收紧：布局 pin 只设 **rust** 侧（C# 是 oracle，不消费该 env，两侧都设是噪音），
  且 rust stack 自动接线 `-PinChecks MIR2_CLIENT_ABILITY_LAYOUT=0 -PinLog E:/tmp/conformance/rust-logs/game-svr.out.log`。
- 加载期守卫（fail-closed）：side 声明了 `MIR2_CLIENT_ABILITY_LAYOUT` 且 stack 是 WMI 包装 ⇒ 必须接线
  `-PinChecks`，否则 ConfigError（点名静默跑错布局缺口）。非 WMI 链（本地适配/桩）不受此限。
  preflight ⑥ `check_pin_wiring` 同口径独立复核（check_bash 先例的双保险）。

## 4. 验收证据（命令 + rc）

```text
card_window.py --selftest        GREEN 279/279（rebase 后 master 基数 272/272 + 本卡 24 节净增 7；
                                 开工时旧基线为 255；15d 同步改钉新契约：rust 侧 pin=0 + 接线 + csharp 侧不设）
preflight.py   --selftest        OK (22 checks)（18 + pin 接线 4：不接线红/接线绿/非 WMI 链不适用/未声明 pin 不适用）
preflight.py   --profile alt     六步全 [OK] ⇒ PREFLIGHT PASS rc=0（pin：0 侧已接线；未声明 pin 的侧不适用）
preflight.py   --profile window-magic   PREFLIGHT PASS rc=0（pin：1 侧已接线）

WMI env 转发冒烟（桩子进程 dump env）：
  MIR2_CLIENT_ABILITY_LAYOUT=0 MIR2_TEST_PIN=hello powershell -File tools/window/start-stack-wmi.ps1
    -Script envdump.ps1 -ReadyPorts 0 -SkipPortWait   ⇒ rc=0，孙进程 env dump 105 vars，
    MIR2_CLIENT_ABILITY_LAYOUT=0 / MIR2_TEST_PIN=hello / SYSTEMROOT/WINDIR 在场（大小写不敏感）

真服务端单进程冒烟（acceptance ③；tools/window/smoke-wmi-env.ps1，不起完整栈）：
  powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/smoke-wmi-env.ps1
  ⇒ PIN MIR2_CLIENT_ABILITY_LAYOUT=0；SERVER_EXE <worktree>/mir2-rs/target/release/mir2-server.exe；
    CONF E:/tmp/conformance/rust-root/M2GameSvr/Server.conf (GatePort=15000)
  ⇒ wrapper| env forwarding: 107 vars (MIR2_*: MIR2_CLIENT_ABILITY_LAYOUT; CARD_WINDOW_*: )
  ⇒ log| 10:03:58.883 [INF] CLIENT_ABILITY_LAYOUT layout=oracle-40（MIR2_CLIENT_ABILITY_LAYOUT=0）
  ⇒ SMOKE OK rc=0；inner-result.txt found=True line=CLIENT_ABILITY_LAYOUT layout=oracle-40（…=0） exited=True
  （对照 53 分诊实锤形态 layout=client-58（<unset>）⇒ 因果闭环：同一链路、同一日志行，
    pin 显式转发后 layout 翻成 oracle-40。）

pin 校验牙（-CheckPinsOnly 离线，selftest 24 行为层同款）：
  一致绿（pin=0 + 日志 oracle-40）rc=0；不一致红（日志 <unset> / 调用方缺 pin / 调用方 pin=1 期望 0）rc=1×3，
  消息分别点名观测行、「不在调用方环境里」「env 与 -PinChecks 不一致」。
```

## 5. 门禁（rebase 到 11c024583 后复跑）

```text
stale_pointer_gate                 OK rc=0（158 files / 37273 lines；ref=fa8113253）
conflict_marker_gate               OK（0 markers across 7492 tracked files）
dynamic_count_stamp_gate           OK rc=0（0 缺戳；158 markdown）
git diff --check                   rc=0
git merge-tree --write-tree walgit/master HEAD   rc=0（tree=0dfcbff7f…）
Rust 零变更                          git diff walgit/master --stat 仅 tools/* + 本报告
```

## 6. 残留物清单

| 路径 | 是什么 | 处置 |
|---|---|---|
| `E:/tmp/w75-smoke-run/` | 真服务端冒烟证据（game-svr.out/err.log、inner-result.txt、inner-server-pid.txt、inner 脚本） | 留作 §4 冒烟复跑件；下次窗口可清 |
| `E:/tmp/w75-smoke/` | 冒烟开发中间件（good/bad 日志桩、envdump、wmitest*、frag*、mergetree 输出） | 本卡使命已毕，可清 |
| 默认栈 | 未触碰（`7200` 的 C# 栈全程原样；十端口状态与开工前一致；mysqld 未动） | — |
| worktree `E:/tmp/wt-worker-75/mir2-rs/target/release/mir2-server.exe` | preflight binary 步用的本树 release 构建（sha256=df527c383bfe…） | worktree 产物，不进 git |
