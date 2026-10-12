# evidence-pin-attempts-info-20261012

owner 2026-10-12 19:20 派（卡 `openmir2-m3-attack-pair-fixture`）：
① 把 `pin_attempts` 门从 fail-closed 数值门**降级为 informational**（保留「字段缺失/不可解析 ⇒ fail-closed」这条牙）；
② 查「首轮只 pin 1/2、12 轮从未到 attempts==2」的根因并修；③ `patch` + `status: needs-review`，不 merge、不自签、离线不起栈。

## 1. 本片改了什么

### ① `pin_attempts` 降级为 informational（依据 owner 19:20 判决性实验）

判决性实验（`evidence-rng-ledger-diff-20261012/41-pin-experiment.txt` + `42-exp-runs.txt`）：3 次 C# 基线各自新起栈、
不筛选 attempts ⇒ `attempts=4/5/5`；attempts 不同（4↔5）与相同（5↔5）时 `attack_seq` 与攻击路径取数值序列**都不同**，
取数条数也变（275/275/273）；结合旧证 `c1≡c3`（attempts 3/4 逐字节全等）⇒ **attempts 既不充分也不必要**，
只是观测到的协变量（根因 = 全局 `M2Share.RandomNumber` 被并发噪声消费者共享）。

- `mir2-rs/tests/parity/m3/gate_guards.ps1`
  - `Get-M3PinAttemptsGuard` / `Test-M3PinAttemptsComparable`：`failed` 只保留
    `pin_attempts_missing` / `_invalid` / `_baseline_invalid` / `_session_missing`；
    `attempts >/< expected`、`expected` 漂移 ⇒ 只进 `informational`（`retried` 字段仍保留供汇总）。
- `mir2-rs/tests/parity/m3/attack_diff.py`
  - `pin_attempts` 子命令：解析出 `PIN_HOLD` 即 `rc=0`（`informational`/`retried` 仍打印）；无 `PIN_HOLD` 仍 `rc=2`。
- `mir2-rs/tests/parity/m3/run_attack_pair.ps1`
  - 跨侧 `Invoke-AttackCrossSideDiff` 的 ①.5 与本侧 `-Side rust` 后的守卫：只对「字段缺失/不可解析」判 `PREREQ`；
    数值 attempts 打 `PIN_ATTEMPTS_INFO`（不再 `PIN_ATTEMPTS_OK` 拒比）。
  - `-SelfCheckNoStack` ⑱ 重写：attempts=2/3/期望漂移/嵌套对象 ⇒ 绿；缺字段/形态坏 ⇒ 红；
    **改坏必红 = 删掉 `pin_attempts` 字段**（真跑后还原）。
- `mir2-rs/tests/parity/m3/selftest_gate.ps1` ⑨ 同步重写（同上口径）。
- `mir2-rs/tests/parity/whitelist.md` T-29 第七轮：登记本降级 + 根因修。

### ② 预建角夹具：预热相不进世界（根因修）

根因（详见 `05-prewarm-world-entry-root-cause.txt`）：预热相假人**进了世界**；`run_bots` 的 `Stop-Process`
被 GameSvr 当真实下线 ⇒ `WorldServer.SaveHumanRcd`（`src/GameSrv/Word/WorldServer.cs:711-713`）→
`MShare.FrontEngine.AddToSaveRcdList`（`:1077-1091`，**异步**）把预热末态写回、覆盖随后的 SQL pin
⇒ 首轮只 pin 1/2、要 3~5 轮才收敛。真机原件：`bots-20261012-150104.log`（本目录 `05-*` 有摘录 + sha256）。

- `src/BotSrv/RobotOptions.cs` / `MShare.cs` / `AppService.cs`：新增 `StopAfterCharCreated`（默认 false = 原行为）。
- `src/BotSrv/Scenes/Scene/SelectChrScene.cs`：`nChrCount > 0` 且开关打开时打印
  `预热建角完成（StopAfterCharCreated；不进世界）` 并 `return`（不发 `CM_SELCHR`）。
- `tools/botload/run_bots.ps1`：`-StopAfterCharCreated` 开关，写进 `AppSetting.json.BotPlay`。
- `mir2-rs/tests/parity/m3/run_pair.ps1`：预热相强制带 `-StopAfterCharCreated`（`Get-M3BotOpPlan -ForPrewarm`）；
  预热 BotSrv 日志落 `<会话>/prewarm-bot.log`；新门 fail-closed；结果落 `baseline.json.prewarm_no_world_entry`。
- `mir2-rs/tests/parity/m3/gate_guards.ps1`：`Test-M3PrewarmNoWorldEntry`（有进世界标记/日志缺失 ⇒ 红）。

## 2. 证据清单

| 文件 | 内容 | 结果 |
|---|---|---|
| `01-selftest-gate.txt` | `selftest_gate.ps1`（含 ⑨ pinacct informational 双向 + ⑩ prewarm 三向 + 改坏必红） | `total=61 failed=0` GREEN |
| `02-attack-pair-selftest.txt` | `run_attack_pair.ps1 -SelfCheckNoStack`（含 ②b 透传 + ⑱ 降级 + ⑲ 缺 my） | `SELFTEST_RED GREEN` rc=0 |
| `03-attack-diff-selftest.txt` | `attack_diff.py selftest`（含 k-pin-retry **仍绿**） | `SELFTEST GREEN 30/30` |
| `04-pin-attempts-real-csharp-185128.txt` | C# 侧 `pin_hold.txt` 原文（PIN_HOLD attempt=4） | 输入 |
| `04-pin-attempts-recompute-before-after.txt` | 同一输入：改前 `rc=1` / 改后 `rc=0` / 无 PIN_HOLD `rc=2` | 改前/改后 |
| `05-prewarm-world-entry-root-cause.txt` | 根因 + oracle 行号 + 真机预热日志摘录 + 修法 | 根因 |
| `06-commands.txt` | 复现命令（含 PSModulePath 注记） | — |
| `SHA256SUMS.txt` | 本目录文件 hash（LF 归一 / git blob 口径） | 干净 clone 可复现 |

C# 可编译性：`dotnet build src/BotSrv/BotSrv.csproj -c Release` = 0 error（`06-commands.txt` §4）。

## 3. 残留 / 未做（如实登记）

- **未起真栈**验证「修复后 attempts 回到 2」：owner 19:20 明确「离线可判的别起栈」，且 attempts 已不再是判据；
  真机复跑由 m3cs 取窗执行。本片**零端口占用**。
- `-StopAfterCharCreated` 若被误放进**计分相**，会因收不到 `SM_LOGON` 而不武装序列 ⇒ 既有
  `M3ATTSEQ done`/`M3BAGSEQ done` 收尾判据会 RED（fail-closed，不会静默绿）。
- 预热相仍依赖 `run_bots -RequireCharCreated` 的「创建角色」标记（建角请求时落，不是服务端确认）——
  现有 `run_pair` 用 `SELECT COUNT(*) FROM characters` 复核建角成功，本片未改这一口径。
