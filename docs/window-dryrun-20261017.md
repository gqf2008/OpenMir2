# 开窗前「干跑」核查报告 — W0 两侧对照窗口（2026-10-17）

> ## ⚠ 复验订正（2026-10-17，独立复验判 **REFUTED**；本块**优先于**正文）
>
> **两条 NO-GO 硬阻塞经独立复现成立**（见 §1/§2 与 surviving）：
> ① 窗口前置门 RED——`R6` 窗口构建源 mirror `HEAD=da2df1fdb` 落后基线 `origin/master` 的生产路径 **65 件**、
> `R7` 缺 `E:/tmp/m2svr-window-target/window-provenance.json`，`start` 在 `run_guard || exit 1` 即退出；
> ② **RNG 两侧账本驱动未接线**——`run_rng_trace_pair.ps1 -Go -PinScene` 与 `run_rng_scene_pair.ps1 -Go -PinScene`
> 均 `rc=2 / PREREQ_MISSING`（`DRIVER-CONTRACT.md` §7「承接卡尚未开」）⇒ W0-6 与 §2e 在窗口内必然拒跑。
> ⇒ **整包 W0：NO-GO**；若缩窗为 W0-1..W0-5 并把 RNG 项显式移出范围，补齐 M1–M4、M7–M10 后可 GO（缩窗）。
>
> **正文中以下 6 处经复验订正（订正后结论以本块为准）**：
> 1. **§3.3/§2 行 3/§3.6 的「C# `DBSrv`/`LoginSrv` 二进制疑似过期、可证不含当前源码修复」= 错**。
>    .NET 字符串堆是 **UTF-16LE**，UTF-8 `grep` 必然为 0。按 UTF-16LE 复核：`DBSrv.dll` 含 `候选`、`[QueryChr] 候选`、
>    `GetBy -> result=`、`GetQryChar -> `、`dataIndex=`、`nChrCount=` 各 1；`LoginSrv.dll` 含 `[选服]`、`selServer=` 各 1
>    —— 均为 `29af578e4` 新增 ⇒ **不是过期二进制**。**M5 不再是阻塞项**，降级为「按 sha256 复核构建物出处」。
> 2. **M5 的验收命令不可达**：`grep -a -c "候选" …DBSrv.dll # 期望 1（当前 0）` 在 UTF-8 下永远为 0；须改 UTF-16LE 检索或用重编后 sha256 比对。
> 3. **§3.6-3 的引用错**：runbook 中**没有** `$w`/`openmir2-watch` 文本；§W0-7 实际写的是
>    `start_rust_stack.ps1 -LogDir … -Stop`。那行 `client-e2e-window.sh stop` 是**安装脚本 start 分支的 echo**（旧提示），
>    风险按「脚本提示与实际不一致」降级，不是 runbook 的错。
> 4. **「唯一写盘 = 本报告」不实**：本次自检（`-SelfCheckNoStack`）会触发 `dotnet build`，在**共享主检出**写入
>    **25 个 git-ignored 文件**（`src/BotSrv` 14 个：含 `BotSrv.dll/exe` 12:48:20、`OpenMir2.dll/pdb`、`AppSetting.json`、`obj/*`；
>    `tests/parity/m3/rng-hook` 11 个 `RngIsolateSelftest.*` 12:50:33）并新建空目录 `sessions/csharp-20261017-124844`；
>    全部被 `.gitignore` 命中 ⇒ `git status` 仍为 0。**这是该车道对「只读」纪律的违反**（未碰冻结面/未碰部署件/未碰端口）。
> 5. **小订正**：`added=1649` 除审计文档/看板外，也在 `mir2-rs/bins/game-svr/src/mapwire.rs:13,1105`；工具表实为 **18 行**；
>    「69 抽/18 站点 vs 68 抽/17 站点」**不矛盾**——后者是部署件原值，加入屏障 A 的 1 抽后为前者，卡文本已写明，**无需 owner 裁决**。
> 6. **补全**：§3.5 的 DB 查询只给了 SQL 片段，缺**确切命令行**（客户端路径/账号来源/主机端口）；
>    `fixture_chain_gate.py check` 的 `runner_ok=10` 在复验窗口内未复现（>2 分钟被终止）——该项以本仓 CI 口径另行复核。


- 车道性质：**只读核查**。未起任何服务端/客户端/假人；未占用任何游戏端口；未写数据库；
  未改仓库（`git status --porcelain` = 0 行）；未改 `E:/MirServer/**`；唯一写盘 = 本文件
  `E:/tmp/window-dryrun-20261017.md`。
- 核查对象：`mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/82-two-side-window-runbook.md`
  §1 / §2d（W0-0..W0-8）/ §2e，以及看板 in-progress 卡的「窗口项」。
- 基线：`master = 2fa749471b08240b36d2d0200cd7b6097097b2be`（与 `origin/master` 相同）。
- 标注约定：**【事实】**= 本次实跑命令的真实输出；**【推断】**= 基于事实的判断，未直接实测。

---

## 1. 总结论

**NO-GO（整包 W0 窗口）**。

两个**硬阻塞**（不是「需要 owner 决定」，是开窗必卡）：

1. **窗口前置门现在是 RED**：`client-e2e-window.fixed.sh.txt guard` ⇒ `WINDOW_PREFLIGHT_RED`
   （R6 构建源落后基线 65 件 + R7 缺出处清单；`start` 在 `run_guard || exit 1` 即退出）。
2. **RNG 双侧账本驱动未接线**：`run_rng_trace_pair.ps1 -Go -PinScene` 与
   `run_rng_scene_pair.ps1 -Go -PinScene` 都 **rc=2 / PREREQ_MISSING**（「驱动尚未接线」）。
   ⇒ W0-6 与 §2e（召唤回落 RNG）在窗口内**跑不了**，不是「可能失败」而是**必然拒跑**。

若 owner 愿意**缩窗**为 W0-1..W0-5（③b 阳性对照 / Rust 同流程 / ③a 反向点 / GuardList 帧 / 怪打玩家探针），
在补完第 4 节 M1–M5、M9 后可以 **GO（缩窗）**；RNG 相关项必须显式移出本次窗口范围。

---

## 2. 逐条结论表

| # | 核查项 | 结论 | 一句话证据 |
|---|---|---|---|
| 1 | 端口基线（9 游戏端口 DOWN / 仅 3306 UP） | **具备** | `port-status.ps1`：缺失端口 = 5000,5100,5500,5600,5700,6000,7000,7100,7200；仅 `mysqld pid=21364 port=3306` |
| 2 | 工具就绪（自检/`--help` 级） | **具备（判据工具）/ 不具备（RNG 真机驱动）** | 15 件判据工具 selftest 全绿；两个 RNG 驱动 `-Go` 均 rc=2 拒跑 |
| 3 | 两侧产物/版本 | **C# 侧基本具备（1 处需核实）/ Rust 侧不具备** | 部署件 `628d8024` 与 runbook 期望一致；窗口 target `E:/tmp/m2svr-window-target` **不存在**；C# DBSrv/LoginSrv 二进制早于源码提交 |
| 4 | 基线重取步骤是否可执行 | **不具备** | runbook 无「重取基线」文字、无命令、无制品；两张卡写的期望值互相矛盾（69 抽/18 站点 vs 68 抽/17 站点） |
| 5 | 崩溃/内存前置 | **不具备** | runbook 全文 0 处「内存/崩溃/memory/crash」；系统 14 天内 2 次 bugcheck（0x3B / 0x12B） |
| 6 | 未知/阻塞（会空烧窗口） | **存在 7 项** | 见 §3.6 |

---

## 3. 逐项证据

### 3.1 端口基线 —— 具备【事实】

命令（只读脚本，无 `-Out`）：

```
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
```

输出片段：

```
## 3. 服务端进程 / 端口（基线是否还活着）
- mysqld      pid=21364   port=3306
- **缺失端口**: 5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200
```

补充核对（只读 `Get-Process`）：

命中：**无** BotSrv / game-svr / GameSvr / DBSrv / LoginSrv / 网关 / MirClinet。
命中：`cargo` ×1、`rustc` ×N —— 但命令行指向**别的车道**的 `E:/tmp/m821-fix-target`：

```
cargo.exe check -p game-svr --tests      (PID 23264, 12:46:39)
--out-dir E:/tmp/m821-fix-target\debug\deps
```

⇒ **端口面干净**；但 runbook §1 第 2 条「无残留 cargo/rustc」**当前不成立**（属车队构建，见 §3.6-5）。

DB 只读核对（`SELECT`，未写）：

```
SELECT MapName,CX,CY FROM mir2_db.characters WHERE ChrName='aaa';
0	291	652
SELECT c.ChrName,a.Hp,a.MaxHP ... WHERE c.ChrName='aaa';
aaa	19	19
```

⇒ 角色存在但坐标已漂到 `(291,652)`（runbook §2 记的 2026-10-14 开工前是 `(297,629)`）；
`Hp==MaxHP==19` ⇒ **§2b 压血步骤本次必做**。

### 3.2 工具就绪 —— 判据工具全绿；RNG 真机驱动不通

全部只跑自检/`--help`（未 `-Go`、未起栈）：

| 工具 | 命令 | 结果【事实】 |
|---|---|---|
| `run_pair.ps1` | `-SelfTestRed` | `SELFTEST_RED GREEN: 入口 127.0.0.1:17998 无监听 => 夹具判失败成立`（rc=0）；**无 `-SelfCheckNoStack`** |
| `run_guild_castle_pair.ps1` | `-SelfCheckNoStack` | `SELFTEST ok=22/22 failures=` / `GREEN`（rc=0） |
| `run_hero_pet_pair.ps1` | `-SelfCheckNoStack` | `SELFTEST ok=23/23 failures=` / `GREEN`（rc=0） |
| `rng-trace-scene/run_rng_trace_pair.ps1` | `-SelfCheckNoStack` | `SELFTEST ok=37/37`（rc=0） |
| `rng-scene/run_rng_scene_pair.ps1` | `-SelfCheckNoStack` | `SELFTEST ok=23/23`（rc=0） |
| `rng-trace-scene/check_anchors.py` | 无参 | `GREEN: check_anchors 7/7 锚点保鲜`（rc=0） |
| `rng-trace-probe/analyze_rng_trace.py` | `selftest` | `SELFTEST GREEN: ... 5/5` |
| `check_rust_window_preflight.py` | `--selftest` | `SELFTEST OK (73/73)` |
| `check_csharp_capture_preflight.py` | `--selftest` | `SELFTEST OK (12/12)` |
| `aoi_visibility_pair.py` | `--selftest` | `SELFTEST OK (11/11)` |
| `check_attack_observability.py` | `--selftest` | `SELFTEST GREEN` |
| `check_reverse_point_clean.py` | `--selftest` | `SELFTEST GREEN` |
| `check_mongen_dense_points.py` | `--selftest` | `SELFTEST OK (60/60)` |
| `check_aoi_visibility_diag.py` | `--selftest` | `SELFTEST OK (9/9)` |
| `check_healthspellchanged_trigger.py` | `--selftest` | `SELFTEST OK (9/9)` |
| `check_client_s2c_frames.py` | `--selftest` | `SELFTEST GREEN` |
| `check_client_run_frames.py` | `--selftest` | `SELFTEST_PAIR OK (7/7)` |
| `fixture_chain_gate.py` | `check` | `FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0` |
| `mir2_proxy` / `mir_flow` / `export_s2c` | 存在性 | `tools/capture/mir2_proxy.py`、`tools/client/mir_flow.ps1`、`tools/m2-acceptance/export_s2c.ps1` 均在 |

**决定性反例【事实】**（本报告核心发现）：

```
powershell -File mir2-rs/tests/parity/m3/rng-trace-scene/run_rng_trace_pair.ps1 -Go -Side csharp -PinScene
PREREQ_MISSING: 两侧「场景描述件 + 全场景抽数流水」驱动尚未接线（承接卡见报告），真机序列无法产生可判原始件
RC=2

powershell -File mir2-rs/tests/parity/m3/rng-scene/run_rng_scene_pair.ps1 -Go -Side csharp -PinScene
PREREQ_MISSING: 两侧「场景描述件 + 全场景抽数流水」驱动尚未接线（承接卡见 rng-trace-scene/DRIVER-CONTRACT.md §7）
RC=2
```

`DRIVER-CONTRACT.md` §7 原文：**「承接卡尚未开（owner 动作；作者不自建卡）⇒ 两侧驱动未接线。」**
⇒ runbook §W0-6 / §2e 的命令**照抄即 rc=2**，不是环境问题。

### 3.3 两侧产物/版本

**oracle（C#）**

【事实】
- `E:/MirServer/M2GameSvr/GameSrv.exe` = 2026-10-10 23:54；`!deployed-from.json`：
  `git_head = 628d8024ab85...`、`git_dirty=false`、`deployed_at=2026-10-10 23:54:59`。
- `check_csharp_capture_preflight.py --repo ... --mirserver E:/MirServer --shadow-config E:/tmp/window/cliente2e/shadow/run-gate/config.conf`
  ⇒ `RESULT: GREEN`（含影子：`Count=1` / `GatePort1=7600` / `ServerWorkThread=1`）。
- 部署后 master 上 **`src/GameSrv`、`src/M2Server`、`src/GameGate`、`src/LoginGate`、`src/SelGate`、`sql/` 改动 = 0**
  （`git log --since="2026-10-10 23:54" -- <这些路径> | wc -l` ⇒ 0）。
  ⇒ **oracle 游戏逻辑部署件与当前源码一致**（31 条 `src/` 提交**全部**在 `src/BotSrv`，是夹具驱动）。
- `BotSrv.exe/.dll` mtime = 2026-10-17 12:48 —— **本次自检构建的副产物**（`run_*_pair -SelfCheckNoStack` 内部会
  `dotnet build BotSrv`）；不改变游戏服务端行为。

【事实 + 推断】**C# DBSrv/LoginSrv 二进制疑似过期**：
- `src/DBSrv/bin/Release/DBSrv.dll` mtime `2026-10-09 23:09`；`src/LoginSrv/bin/Release/LoginSrv.dll` `23:02`。
- 提交 `29af578e4`（`2026-10-10 00:09`，晚于上述构建）改了
  `src/DBSrv/Services/Impl/UserService.cs`（**去掉“已玩过的角色从列表消失”的 SelectID 过滤** + 新日志）、
  `src/LoginSrv/Services/{ClientSession,SessionManager}.cs`（顶号释放 + 日志）。
- 用字符串做二进制取证：
  - `grep -a -c "候选" src/DBSrv/Services/Impl/UserService.cs` ⇒ `1`
  - `grep -a -c "候选" src/DBSrv/bin/Release/DBSrv.dll` ⇒ **`0`**
  - `grep -a -c "[选服]" src/LoginSrv/bin/Release/LoginSrv.dll` ⇒ **`0`**（源码有该日志）
  ⇒ **DBSrv 二进制可证不含当前源码的修复/日志**；LoginSrv 为混合证据（含 `DeleteByAccount` 但缺 `[选服]` 日志）。
- 窗口用的正是这两个目录：`start_rust_stack.ps1:204-207` 与 `start-csharp-hook.ps1:133-136` 都取
  `<repo>/src/{DBSrv,LoginSrv}/bin/Release`；`E:/MirServer/start-all.ps1:29-40` 同源。
- 【推断】若不重编，W0-0 登录后**角色 `aaa` 可能不出现在选角列表**（正是 `29af578e4` 修的那个 bug）⇒ **窗口白烧**。

**Rust 侧**

【事实】
- 窗口 target **不存在**：`E:/tmp/m2svr-window-target` → `No such file or directory`；
  `E:/tmp/m2svr-window-target/window-provenance.json` 不存在。
- 其它车道 target 里有 `game-svr.exe`（如 `E:/tmp/entershift-target/debug/game-svr.exe` 11:27），
  但**不能复用**（R7 出处清单绑定 target + 六件 exe）。
- 主检出 `mir2-rs/Cargo.lock` 与 mirror 的 `Cargo.lock` **逐字节相同**（`diff` rc=0）⇒ 离线构建不会因锁文件漂移失败。
- `mir2-rs/tests/parity/m3/rng-hook/bin/Release/net8.0/RngSeedHook.dll` 存在（10-13 02:39，晚于源码 01:33）。

### 3.4 基线重取要求 —— 不具备（缺步骤、缺制品、数字自相矛盾）

【事实】
- runbook 全文 `grep -n "重取基线|1649|13 抽|69 抽|68 抽"` ⇒ **0 命中**。
- 仓库内也搜不到「重取基线」：`grep -rn "重取基线" mir2-rs/tests/parity docs` ⇒ 0 命中。
- 看板两张卡对“构造段账本”的期望值**不一致**：
  - `openmir2-ctor-barrier-a-draw`：「构造段账本 **69 抽/18 站点**」「默认流水已变，须重取基线」
  - `openmir2-ctor-draws-fold`：「两侧同种子 seq/site/result 逐项 diff；期望 **68 抽/17 站点**」
  - 这两个数字**都不在仓库任何制品里**（`consumer_groups.tsv` 只是站点 vocabulary，不是数值基线）。
- `route added=1649` 只在审计文档/看板出现；runbook 无核法。日志字面量在
  `mir2-rs/bins/game-svr/src/main.rs:7993`：`"地图连接点已灌入（MapRoute）routes={} unresolved={} added={}"`。
- `openmir2-npc-ctor-draws` 还要求「窗口前先实测 `Robot.txt` 行数」，runbook 亦无该步骤。

⇒ **GAP 成立**。runbook 只有 W0-6 的“两侧现场各采一份、再 groupdiff”的原则，没有「先重取基线」的
对象、命令、落盘路径与判定值。且 69/18 与 68/17 谁是当前基线需 owner 裁决。

### 3.5 崩溃/内存风险前置 —— 不具备

【事实】runbook 全文 `grep -n "内存|memory|崩溃|crash|事件日志|长窗"` ⇒ **0 命中**。

【事实】近 14 天系统崩溃记录（`Get-WinEvent System Id=41,1001,6008`）：

```
2026/10/17 0:03:00   41  Kernel-Power        (非正常关机)
2026/10/17 0:03:10 6008  EventLog
2026/10/16 21:20:02 1001  WER-SystemErrorReporting  → bugcheck 0x0000003B (0xc0000005,...), dump: E:\Dumps\MEMORY.DMP
2026/10/16 21:19:45   41  Kernel-Power
2026/10/16 21:19:57 6008  EventLog
2026/10/11 21:11:40   41  Kernel-Power
2026/10/11 21:06:07   41  Kernel-Power
2026/10/11 20:30:12 1001  WER-SystemErrorReporting  → bugcheck 0x0000012B (0xffffffffc00002c4,...)
2026/10/11 20:29:56   41  Kernel-Power
2026/10/03 21:18:38   41  Kernel-Power
```

- 6 天内 **2 次 bugcheck**：`0x3B`（SYSTEM_SERVICE_EXCEPTION，`c0000005`）与 `0x12B`
  （参数 `0xc00002c4` = `STATUS_DATA_CHECKSUM_ERROR`；该码通常指向**页损坏/硬件内存**类问题）【推断：与内存稳定性相关，需硬件/驱动侧确认】。
- `E:\Dumps\MEMORY.DMP` 当前**不存在**；`E:\Dumps\Minidump\` 为空 ⇒ 现场 dump 不可再分析。
- 应用层近期崩溃（`Application Id=1000/1026`）包括 `BotSrv.exe` 的
  `KeyNotFoundException ... JsonElement.Get...`（10-16 12:01）——与 runbook 已登记的「BotSrv 进世界后
  在 `TMap.CanMove` 崩」同族，不新增阻塞。

【推断】长窗口（两侧各数十分钟 + 客户端 + 抓包）期间再次蓝屏 = 整个窗口作废且证据半截。
runbook 未把「先跑内存测试/稳定性冒烟」列为前置，属实测缺口。

**可执行的内存测试建议**（窗口**之前**单独占一个槽跑，勿与本窗口并行）：

```powershell
# A. 项目自带负载+内存斜率（会起全栈，属另一个窗口槽）
powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/run_m1_load.ps1 `
  -Tier 200 -HoldSec 60 -OutDir E:\tmp\openmir2-m1load\evidence\rust-200
# 判据：report-rust.json 的 slope_tail_mb_per_s 接近 0 / 无单调上升；四进程 WS 无阶梯

# B. 轻量驻留 soak（只起 Rust 栈，30 分钟采 WS）
#    起栈后另开一个只读采样循环（不占游戏端口）：
Get-Process game-svr,login-gate,sel-gate,game-gate -EA SilentlyContinue |
  Select-Object @{n='t';e={Get-Date -f HH:mm:ss}},ProcessName,Id,
    @{n='WS_MB';e={[math]::Round($_.WorkingSet64/1MB,1)}},
    @{n='Priv_MB';e={[math]::Round($_.PrivateMemorySize64/1MB,1)}}
# 每 10s 一次，落 E:\tmp\window\soak-mem.txt（用 Start-Transcript 或 Export-Csv -Append）
```

机器余量【事实】：物理内存 32,615,808 KB（≈31.1 GiB），空闲 16,453,372 KB（≈15.7 GiB）；
E: 盘空闲 2394.5 GB。⇒ 容量本身不是瓶颈，**稳定性**才是。

### 3.6 未知/阻塞（会让窗口空烧）

1. **guard 当前 RED（硬阻塞）**【事实】
   ```
   bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard
   ...
   RED R6: 构建源 HEAD=da2df1fdbd 落后于基线 origin/master@2fa749471b 的生产路径（65 件 ...）
   PREREQ_RED R7: 缺出处清单 E:/tmp/m2svr-window-target/window-provenance.json
   RESULT: RED
   WINDOW_PREFLIGHT_RED  （拒绝起窗：...）
   PREFLIGHT_RECORD_CMD: cd "E:/tmp/m2svr-window-origin/mir2-rs" && CARGO_TARGET_DIR="E:/tmp/m2svr-window-target" cargo build --offline -p game-svr ... && python ... --record-provenance ...
   ```
   - mirror `E:/tmp/m2svr-window-origin`：`HEAD = da2df1fdb`（2026-10-14），`origin/master = 2fa749471`（已 fetch 到）。
   - 同一工具对**主检出**跑：R6 `[OK] HEAD=2fa749471b == 基线`，**只剩 R7**（rc=2）。
   ⇒ 修法二选一：把脚本 `repo` 指到主检出，或先把 mirror 更新到基线再构建。

2. **RNG 驱动未接线（硬阻塞）**：见 §3.2。W0-6 与 §2e 不可能在本次窗口产出数据。

3. **入库脚本 ≠ 已安装脚本**【事实】
   - 仓库 `client-e2e-window.fixed.sh.txt`：`repo=/e/tmp/m2svr-window-origin`、`tgt='E:\tmp\m2svr-window-target'`、
     **有 `guard`**、`stop [LogDir]`。
   - 已安装 `/e/tmp/openmir2-watch/client-e2e-window.sh`（10-13 21:49）：`repo=/e/Users/gxh/Documents/GitHub/OpenMir2`、
     `tgt='E:\tmp\openmir2-merge-target'`、**没有 `guard`**、`stop` 不接受 LogDir。
   - runbook §W0-7 收窗仍写 `bash $w/client-e2e-window.sh stop`（= **旧脚本**）。
   ⇒ 若操作员按 runbook 收窗、却按 §1b 用仓库脚本起窗，起/停可能不是同一套（LogDir 默认值恰好同为
     `E:\tmp\client-e2e\stack`，所以当前不会立刻炸；但 `guard` 会被旧脚本绕过）。

4. **target 路径不一致**【事实】
   - runbook §3.1：`-RustBinDir E:/tmp/openmir2-window-target/debug`
   - 窗口脚本：`tgt='E:\tmp\m2svr-window-target'`
   ⇒ 照 runbook 建/起会指向不存在的 bin 目录（或另建一份未录出处的 exe）。

5. **车队暂停机制与当前编排不匹配**【事实】
   - `lanes-pause.sh` 只 `Stop-Process` **`node.exe` 且命令行含 `Tag: <tag>`**；当前进程表里 **没有 node**
     （编排是 claude.exe：PID 2408/4980/7892）。
   - `lanes.conf` 最后修改 10-14，活动车道标签只有 `window|rev|rev2|merge|m2svr`；
     看板显示在跑的 `m3bag`、`m821` 等**不在其中**。
   - 当前确有 `cargo check -p game-svr`（`E:/tmp/m821-fix-target`）在跑。
   ⇒ 「暂停车队」既不会杀这些构建，也不会覆盖新车道；窗口期间可能出现**并发编译 + 并发写同一 MySQL**。

6. **C# DBSrv/LoginSrv 二进制早于源码提交**：见 §3.3（可证 DBSrv 缺当前源码字符串）。

7. **runbook 内部小漂移**：§2 记的开工前坐标 `(297,629)` 与当前实际 `(291,652)` 不同（runbook 已用
   “以 §1a before 为准”兜底，不算阻塞）；`mir_flow.ps1` 无自检入口，问题只能在真机暴露。

补充【事实】：影子网关目录**已存在**且合格（`E:/tmp/window/cliente2e/shadow/run-gate/config.conf`：
`Count=1` / `GatePort1=7600` / `ServerWorkThread=1`），C# 抓包前置 **GREEN**，这一项不用重做。

---

## 4. 开窗前必须补的 N 件事（可执行到命令级）

> 顺序即依赖序。M1–M4 是「能起窗」的门槛；M5 是「不起也白起」；M6–M9 是「起了也交不了差」。

### M1. 把窗口构建源对齐基线（解 R6）
二选一（推荐 A，避免维护两棵树）：

```bash
# A. 直接把窗口脚本指向主检出（改的是 /e/tmp 下的运行副本；入库脚本由 owner 决定是否同步）
#    或：在 runbook/脚本里把 repo 改成 E:/Users/gxh/Documents/GitHub/OpenMir2
# B. 更新 mirror 到当前基线
git -C /e/tmp/m2svr-window-origin fetch origin
git -C /e/tmp/m2svr-window-origin checkout --detach "$(git -C /e/Users/gxh/Documents/GitHub/OpenMir2 rev-parse origin/master)"
python -I -B /e/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py \
  --repo /e/tmp/m2svr-window-origin --ref HEAD --target-dir /e/tmp/m2svr-window-target \
  --provenance /e/tmp/m2svr-window-target/window-provenance.json --baseline-ref origin/master
# 期望：只剩 R7（PREREQ_RED rc=2），R6 [OK]
```

### M2. 构建六件 + 录出处（解 R7）
**照抄 guard 打印的那一行**（`&&` 串联，禁止编译失败后仍录制）：

```bash
cd "<源>/mir2-rs" && CARGO_TARGET_DIR="E:/tmp/m2svr-window-target" \
  cargo build --offline -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv && \
python -I -B "<证据目录>/check_rust_window_preflight.py" --repo "<源>" --ref HEAD \
  --target-dir "E:/tmp/m2svr-window-target" --record-provenance "E:/tmp/m2svr-window-target/window-provenance.json"
```
（`<源>` 与整条命令由工具自己打印；用 guard 的输出，不要手拼。当前主检出作为源时，工具打印的就是
`cd "E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs" ...`。）

### M3. 统一 target 路径
把 runbook §3.1 的 `E:/tmp/openmir2-window-target/debug` 改成与脚本一致的
`E:/tmp/m2svr-window-target/debug`（或在脚本里改回前者——**只允许一个真源**）。

### M4. 统一起/停脚本
- 起窗：只用入库件 `client-e2e-window.fixed.sh.txt start`（带 guard）。
- 收窗：用**同一份**入库件的 `stop`，并显式传本趟 LogDir：
  ```bash
  bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt stop "E:/tmp/window/cliente2e/rust/stack"
  ```
- 若要保留 `/e/tmp/openmir2-watch/client-e2e-window.sh` 作为习惯入口，先把入库件同步过去（owner 动作）。

### M5. 重编/验证 C# DBSrv 与 LoginSrv（防「角色列表空」）
```bash
dotnet build "E:/Users/gxh/Documents/GitHub/OpenMir2/src/DBSrv/DBSrv.csproj" -c Release --nologo
dotnet build "E:/Users/gxh/Documents/GitHub/OpenMir2/src/LoginSrv/LoginSrv.csproj" -c Release --nologo
# 验证（源码有、二进制也应有）：
grep -a -c "候选" "E:/Users/gxh/Documents/GitHub/OpenMir2/src/DBSrv/bin/Release/DBSrv.dll"   # 期望 1（当前 0）
```
起栈前再做一个**只读**登录冒烟：用真客户端进到选角界面，确认 `aaa` 在列表里；不通过就不开窗。

### M6. 处理 RNG 驱动缺口（决定窗口范围）
- 要么 **开承接卡**按 `rng-trace-scene/DRIVER-CONTRACT.md` §2/§6/§7 接线（两侧场景描述件 + 全场景抽数流水）；
- 要么在 runbook/卡上**显式**写：「本次窗口不含 W0-6 与 §2e RNG 项」，并从收窗 DoD 中摘除，
  避免窗口结束时才发现“有五项没数据”。

### M7. 把「重取基线」落成可执行步骤（补进 runbook，非本车道代写）
至少要有：① 基线对象（构造段账本 / route / 刷怪位置）；② 采集命令（同种子 `-Seed 42` 两侧）；
③ 落盘路径（如 `E:/tmp/window/cliente2e/ledger/<side>-ctor.tsv`）；④ 判定值（先由 owner 裁定 69/18 还是 68/17）。
另外补两条机判命令：
```powershell
# route 落格：期望 routes=1658 unresolved=0 added=1649
Select-String -Path E:/tmp/window/cliente2e/rust/stack/game-svr.out.log -Pattern '地图连接点已灌入（MapRoute）'
# Robot.txt 行数（构造段账本头部 = 行数×4 抽）
(Get-Content E:/MirServer/M2GameSvr/Envir/Robot.txt | Measure-Object -Line).Lines
```

### M8. 把稳定性前置写进 runbook + 先跑一次内存测试
```powershell
powershell -ExecutionPolicy Bypass -File tools/m1-acceptance/run_m1_load.ps1 `
  -Tier 200 -HoldSec 60 -OutDir E:\tmp\openmir2-m1load\evidence\rust-200
# 判据：report-rust.json slope_tail_mb_per_s ≈ 0；否则先查内存/驱动，别开长窗
```
并把「本机 6 天内 2 次 bugcheck（0x3B/0x12B）」写进 runbook 风险节。

### M9. 窗口前的「真暂停」清单（人工/脚本都要）
```powershell
# 1) 列出所有可能抢端口/抢 DB 的进程
Get-CimInstance Win32_Process |
  ? { $_.Name -match 'claude|node|cargo|rustc|GameSrv|game-svr|DBSrv|BotSrv' } |
  Select ProcessId,ParentProcessId,Name,CreationDate,CommandLine
# 2) 确认没有 cargo/rustc（当前有 cargo 在跑 m821-fix-target）
# 3) 确认没有别的 lane 正在写 mir2_db（看板 in-progress 卡 + 进程）
# 4) lanes-pause 之后复查：它只杀 node.exe；claude/cargo 需另行确认
```

### M10. 开窗首步固定动作（把已具备的项固化）
```powershell
powershell -File E:/MirServer/port-status.ps1                 # 9 DOWN / 3306 UP
bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard   # 期望 WINDOW_GUARD_OK
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_csharp_capture_preflight.py `
  --repo E:/Users/gxh/Documents/GitHub/OpenMir2 --mirserver E:/MirServer `
  --shadow-config E:/tmp/window/cliente2e/shadow/run-gate/config.conf        # 期望 GREEN（当前已 GREEN）
```

---

## 5. 我没核到的面

1. **没有真机起栈**：所有“能不能跑”的判断来自 fail-closed 路径与二进制字符串取证，未验证真实登录/进图/抓包。
2. **没跑 `cargo test` / 门禁**（`cargo fmt/clippy/test`、`msgcodegen --check`）：避免与在跑的 `cargo check`
   争用；这些属合并门禁，不属开窗前置。
3. **没跑 §2e 构造级 `cargo test -p mir2-world --lib recall_`**：同理，未验证 6 条 recall 用例当前是否仍绿。
4. **没读完整的 73 张 in-progress 卡**：只按板面 work 行与代表性卡（ctor / mapcell / recall / npc-ctor）核对窗口项。
5. **没有逐台核对 `E:/MirServer` 每个 dll 的出处**：只核了 `GameSrv` 部署清单与 `DBSrv/LoginSrv` 的字符串证据。
6. **没有验证 `start_rust_stack.ps1` 起栈后 Rust 服务端能否正常读取 `E:/MirServer/M2GameSvr/Server.conf`**
   （需要起栈，超出只读车道）。
7. **没有裁决 69/18 vs 68/17 哪个是当前基线**（需 owner；本报告只指出矛盾）。
8. **没有分析 bugcheck dump**（`MEMORY.DMP` 已不存在，Minidump 目录为空）；0x12B 的“内存/硬件”归因是推断。
9. **没有验证 BotSrv 的 `-SelfCheckNoStack` 是否覆盖 W0 用到的全部动作序列**（只跑了入口自检）。
10. **没有核 `preflight.ps1`（起栈后那一步）的判据**（需活栈）。

---

## 6. 残余物 / 本车道副作用

- 唯一写盘：本报告 `/e/tmp/window-dryrun-20261017.md`。
- 自检副产物（既有行为，非本车道新增文件内容）：
  - `run_pair.ps1 -SelfTestRed` 建了空目录 `mir2-rs/tests/parity/m3/sessions/csharp-20261017-124844/`（空，git 不跟踪）。
  - `run_guild_castle_pair / hero_pet_pair` 自检触发 `dotnet build src/BotSrv`，把
    `src/BotSrv/bin/Release/BotSrv.{exe,dll}` 刷新到 2026-10-17 12:48（bin 为忽略目录）。
  - `git status --porcelain` 复核 = 0 行（仓库干净）。
- 未删除任何文件；未起任何服务/客户端/假人；未写 DB。
