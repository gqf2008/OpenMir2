# M2 验收运行手册（harness）

> 卡：`openmir2-m2-gamesvr-host`（DoD/S4 验收①②③）+ `openmir2-m2-enter-world-frames`（顺序判据②）。
> 本目录把「M2 最终验收」从一次人工探险变成**一条命令**：`run_acceptance.ps1`。
> 目标判据（设计文档 §6 M2 交付 + 卡内 `machine_checkable_acceptance`）：
>
> ① 7000/7100/7200（+ GameSvr 侧 5000）在验收时刻的监听者都是 **Rust 二进制**（`Get-NetTCPConnection` 实拍）；
> ② 真客户端可玩证据是 **7 张截图**（不许用假人日志替代）；
> ③ 同序列出包 **diff = 0**（或逐条白名单）+ 小退/掉线/服务关闭三条退出路径的 **DB 字段级 diff = 0**。

## 0. 一条命令

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/run_acceptance.ps1 `
  -ProxyDir   E:\tmp\m2acc\run1\proxy `
  -ShotsDir   E:\tmp\m2acc\run1\shots `
  -CsharpDbDir E:\tmp\m2acc\csharp `
  -RustDbDir   E:\tmp\m2acc\rust
```

**先跑前置门**（工具/输入/金标准；不碰栈与端口）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/preflight.ps1
# 0=前置齐备 / 2=PREREQ_MISSING（缺 python·dotnet·cargo·客户端入口·金标准）/ 1=RED（金标准身份或 schema 被改坏）
```

`run_acceptance.ps1` **不启动任何服务**（不占端口、不起客户端）——它把 ⓪ 前置门 + 四项机判合成一条命令，
只对「已跑着的栈 / 已采好的产物」取判据。每一步的退出码口径统一：

| 退出码 | 含义 | 整体判定 |
| --- | --- | --- |
| `0` | GREEN，判据成立 | 通过 |
| `1` | RED，判据不成立（有差异/有端口被 C# 占/截图不全） | **不通过** |
| `2` | `PREREQ_MISSING`，前置未满足（栈没起 / 产物没采） | **不成立（非 0 退出，绝不静默通过）** |

**无栈自检**（本片交付时自己跑过的就是这条）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/run_acceptance.ps1 -SelfCheckNoStack
# 预期：preflight(⓪)=GREEN（工具/输入/金标准齐备）、ports(①)=PREREQ、s2c-export(③a)=PREREQ、
#       s2c-diff(③b)=PREREQ、shots(②)=PREREQ、db-paths(③c)=PREREQ，SUMMARY green=1 red=0 prereq=5，整体 exit 2
```

证据落在 `E:\tmp\m2-acceptance\run-<ts>\`：`ports.json` / `s2c/capture.jsonl` / `s2c-diff.json` /
`shots.json` / `db-paths.json` / `acceptance.json`。

**先验证「每一步真的会红」**（构造夹具，不起栈/不占端口）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/selftest.ps1
# 47 条口径全 OK ⇒ exit 0；任何一条不符 ⇒ exit 1（见 §8）
```

---

## 0b. 起栈（`run_acceptance.ps1` 不自己起栈）—— `start_rust_stack.ps1`

`run_acceptance.ps1` 只对「已跑着的栈 / 已采好的产物」取判据；把入口四端口交给 **Rust**
（5000 `game-svr` / 7000 `login-gate` / 7100 `sel-gate` / 7200 `game-gate`）的起栈一条命令是：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:\tmp\openmir2-m2svr-target\debug
# 起：MySQL + C# DBSrv/LoginSrv（下盘 oracle）+ Rust game-svr + Rust 三网关；PID 落 E:\tmp\m2-acceptance\stack\rust-stack.pid.json
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Stop   # 收工停栈
```

**为什么 DBSrv/LoginSrv 仍是 C#**：Rust `bins/db-srv` 只实现了 5100 面，**没有** GameSvr 取档用的
**6000 DataService** 面（`bins/db-srv/src/main.rs` 开头已登记），而 Rust `game-svr` 进世界必须先经 6000
取 `CharacterData` ⇒「全 Rust 链」在 6000 面补齐前无法支撑真客户端进图。验收①只对 5000/7000/7100/7200
四端口取所有权，不受此影响。

起栈前若目标端口已被监听 ⇒ exit 1（**不静默混实现**）；缺 exe/配置 ⇒ `PREREQ_MISSING` exit 2。

> **R1-3（`-GameSvrConf` 测试形态的 `NoticeDir` 坑）**：658 `SM_SENDNOTICE` 的公告文本按
> `<base>/<NoticeDir>/Notice.txt` 读，`base` = **`Server.conf` 所在目录**（`mapwire::base_path`）；
> oracle 用 `M2Share.BasePath = AppContext.BaseDirectory`（**exe 目录**）。生产形态（conf 与 exe 同目录、
> `NoticeDir=Notice` 相对）两者等价；但**测试形态**（`-GameSvrConf` 指到别处 + 补键副本，例如
> `E:\tmp\m2-acceptance\stack\gsvr\Server.conf`）若仍写相对 `NoticeDir`，base 下就没有
> `Notice/Notice.txt`。**R1-1 之后**这不再表现为「658 不下发」，而是发一条 **header-only 658**
> （空尾）⇒ ③ 对账在第 1 条就红（金标准 #1 尾是 320B 公告原文，空尾对不上，且后续帧序仍从 `51` 起）。
> 要用真公告，必须把 `NoticeDir` 写成**绝对路径**（`NoticeDir=E:/MirServer/M2GameSvr/Notice`）。
> `start_rust_stack.ps1` 不生成补键副本（副本由操作者准备）⇒ 手工起测试栈时自己保证这一点。

### 0b.1 `-Seed` · Rust 半的 `MIR2_RNG_SEED` 管线（②③ 真机同种子攻击对拍的前置）

C# 半的种子由 .NET Startup Hook 读 `MIR2_RNG_SEED`；Rust 半的 `bins/game-svr` 生产路径也读同一个变数
（T-30）。起栈时用 `-Seed <int>`（或先在 shell 里 `$env:MIR2_RNG_SEED=<int>`）把该变量**注入 game-svr
子进程环境**，脚本会打一行可判据的 `RUST_SEED_INJECTED=<n>`：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42
# 启动日志出现：SEED: 将向 game-svr 注入 MIR2_RNG_SEED=42 … RUST_SEED_INJECTED=42
# game-svr.out.log 出现：RNG 同种子注入：seed=42 参照序列=SEED_PROBE 1434747710 302596119 …
```

- **不传种子 ⇒ 不注入、不打该行**（与改动前逐字节一致）。
- **非法 `-Seed`/环境变量 ⇒ fail-closed**：打印 `RED: MIR2_RNG_SEED/-Seed 不是合法 int: …` 并 **exit 1**，
  **不起栈**（不静默降级；同 C# 钩子 `TryParse` 失败 ⇒ `Fail/Exit(97)`）。
- 管线实现（`Resolve-Mir2Seed` / `Get-SeedInjectedLine` / `Start-SeededProcess`）在
  `tools/m2-acceptance/rust_seed_plumb.ps1`，与判据脚本**共用同一份**。
- 判红/判绿（不起栈、不占真端口）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_seed_plumb.ps1 `
  -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
# 15/15 ⇒ SEED_PLUMB GREEN (rc=0)；删掉 rust_seed_plumb.ps1 的注入行 ⇒ ②a/③a/③b/③c 必红 (rc=1)
```

证据：`tools/m2-acceptance/evidence/rust-stack-seed-plumb-20261012/`（卡 `openmir2-m3-rust-stack-seed-plumb`）。

### 0b.2 `-StateLog` · Rust 半的 `MIR2_RNG_STATE_LOG` 状态 dump 管线（目标可用性门两侧强制的 Rust 半）

M3②③ 隔离两相的目标可用性门（`mir2-rs/tests/parity/m3/server_state_gate.py`）要**两侧都强制**：
C# 半由 `start-csharp-hook.ps1 -StateLog` 写 `MIR2_RNG_STATE_LOG`；Rust 半由 `bins/game-svr`
（`bins/game-svr/src/state_log.rs`，卡 `openmir2-m2-rust-server-state-dump`）读同一个变量，
每个玩家攻击单元落一行 `SERVER_STATE <seq> <json 数组>`。起栈时用 `-StateLog <path>`（或先在 shell 里
`$env:MIR2_RNG_STATE_LOG=<path>`）把该变量**注入 game-svr 子进程环境**，脚本会打一行
`RUST_STATE_LOG_INJECTED=<path>`：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:\tmp\openmir2-m2svr-target\debug -StateLog E:\tmp\m3-rust\rng-server-state.tsv
# 启动日志出现：STATE_LOG: 将向 game-svr 注入 MIR2_RNG_STATE_LOG=… … RUST_STATE_LOG_INJECTED=…
# game-svr.out.log 出现：服务端逐击状态 dump 已开启（MIR2_RNG_STATE_LOG）…
```

- **不传路径 ⇒ 不注入、不打该行、不建文件**（与改动前逐字节一致）。
- **路径父目录不存在 ⇒ fail-closed**：打印 `RED: MIR2_RNG_STATE_LOG/-StateLog 的父目录不存在: …` 并 **exit 1**，
  **不起栈**（不静默起一个永远不落 dump 的栈）。
- 管线实现（`Resolve-Mir2StateLog` / `Get-StateLogInjectedLine` / `Start-SeededProcess`）在
  `tools/m2-acceptance/rust_seed_plumb.ps1`，与判据脚本**共用同一份**（与 `-Seed` 同文件、互不干扰）。
- 判红/判绿（不起栈、不占真端口）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_state_log_plumb.ps1 `
  -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
# 15/15 ⇒ STATE_LOG_PLUMB GREEN (rc=0)；删掉 rust_seed_plumb.ps1 的注入行 ⇒ ②a/③a/③b 必红 (rc=1)
```

证据：`tools/m2-acceptance/evidence/m2svr-rust-state-log-plumb-20261013/`（卡 `openmir2-m2-rust-state-log-plumb`）。
**接线余量（属夹具写集）**：`run_attack_pair.ps1 -Side rust` 读 Rust 侧 dump 并调同一 gate（`run_attack_pair.ps1:1164`
的 NOTE 尚未去掉，归 m3bag 夹具）。本管线只保证 "Rust 栈起得来 dump"。

### 0b.3 `-IsolateAttack` · Rust 半的 `MIR2_RNG_ISOLATE_ATTACK` 攻击路径专用种子流（M3②③ 活栈隔离口径）

owner 2026-10-12 21:15 §6.0-3 签字的「活栈隔离口径」要求两侧把**攻击路径取数**接到各自一条专用种子流
（`attack_seed = MIR2_RNG_SEED + 0x9E37`）：C# 半由 `start-csharp-hook.ps1 -IsolateAttack` 写
`MIR2_RNG_ISOLATE_ATTACK=1`；Rust 半由 `crates/shared/src/rng.rs::attack_isolation_seed_from_env` 读同一变量、
`crates/world` 在 `World::with_maps` 构造时分流（`world.rs:161-166`）。起栈时用 `-IsolateAttack`（或先在 shell 里
`$env:MIR2_RNG_ISOLATE_ATTACK=1`）把该变量**注入 game-svr 子进程环境**，脚本打一行
`RUST_ISOLATE_ATTACK_INJECTED=1`：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42 -IsolateAttack
# 启动日志出现：ISOLATE_ATTACK: 将向 game-svr 注入 MIR2_RNG_ISOLATE_ATTACK=1 … RUST_ISOLATE_ATTACK_INJECTED=1
```

- **不传开关 ⇒ 不注入、不打该行**（与改动前逐字节一致）。
- **取值认不出 ⇒ fail-closed**（`Resolve-Mir2IsolateAttack` 抛错、`start_rust_stack` exit 1），不静默当开/当关。
- **交叉 fail-closed**：开了隔离但**无 `MIR2_RNG_SEED`** ⇒ 拒绝起栈（`attack_isolation_seed_from_env`
  开而无种子会 **panic**）⇒ 打印 `RED: … 无 MIR2_RNG_SEED/-Seed …` 并 exit 1，**不起任何进程**。
- 判红/判绿（不起栈、不占真端口）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_isolate_plumb.ps1 `
  -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
# 17/17 ⇒ ISOLATE_PLUMB GREEN (rc=0)；删注入行 / 非法值静默当关 / 删交叉守卫 / 注入值改 0 ⇒ 必红 (rc=1)
```

证据：`tools/m2-acceptance/evidence/m2svr-isolate-attack-plumb-20261013/`（卡 `openmir2-m2-rust-isolate-attack-plumb`）。
与 C# 半对称：Rust 只需 `MIR2_RNG_ISOLATE_ATTACK`（`MIR2_RNG_ISOLATE_SITES` 只有 C# 影子 hook 用）。

---

### 0b.3 `-RngLog` · Rust 半的 `MIR2_RNG_LOG` 逐次 rng 流水管线（两侧 site/order 账本 diff 的 Rust 半）

RNG 消费者账本（`mir2-rs/tests/parity/m3/attack_rng_ledger.py` / `rng_ledger_diff.py`）要 C# 与 Rust 两侧的
同名逐次 rng 流水：C# 半由 rng-hook（`mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`）读 `MIR2_RNG_LOG`
落 `rng-stream.tsv`（`seq · kind · max/arg · result · site`，头两行 `INJECT_OK <seed>` + `SEED_PROBE`）；
Rust 半由 `crates/shared/src/rng_trace.rs` 的 `TRACE_ENV` = `MIR2_RNG_LOG`，经 `RandomNumber::with_seed` →
`enable_from_env()` 开流水（旁路仪器，不改值序列）。起栈时用 `-RngLog <path>`（或先在 shell 里
`$env:MIR2_RNG_LOG=<path>`）把该变量**注入 game-svr 子进程环境**，脚本会打一行
`RUST_RNG_LOG_INJECTED=<path>`：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42 -RngLog E:\tmp\m2-acceptance\stack\rng-stream.tsv
# 启动日志出现：RNG_LOG: 将向 game-svr 注入 MIR2_RNG_LOG=… … RUST_RNG_LOG_INJECTED=…
# 该路径出现：1<TAB>INJECT_OK<TAB>42<TAB>0 / SEED_PROBE …  然后逐次 seq kind arg result site
```

- **不传路径 ⇒ 不注入、不打该行、不建文件**（与改动前逐字节一致）。
- **路径父目录不存在 ⇒ fail-closed**：打印 `RED: MIR2_RNG_LOG/-RngLog 的父目录不存在: …` 并 **exit 1**。
- **交叉 fail-closed**：`rng_trace::enable_from_env` 在 `MIR2_RNG_SEED` 缺失时**关闭流水**（只打 stderr）
  ⇒ 已给 `-RngLog` 但无 `-Seed` 时脚本**拒绝起栈**（`RED: … 无 MIR2_RNG_SEED/-Seed …`，exit 1），
  不静默起一个永远不落 rng-stream 的栈（这正是本管线要消灭的“静默空转”）。
- 管线实现（`Resolve-Mir2RngLog` / `Get-RngLogInjectedLine` / `Start-SeededProcess -RngLog`）在
  `tools/m2-acceptance/rust_seed_plumb.ps1`，与判据脚本**共用同一份**（与 `-Seed`/`-StateLog` 同文件、互不干扰）。
- 判红/判绿（不起栈、不占真端口；③ 用真 game-svr + 空闲端口临时 conf）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_rng_log_plumb.ps1 `
  -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
# 15/15 ⇒ RNG_LOG_PLUMB GREEN (rc=0)；删掉 rust_seed_plumb.ps1 的注入行 ⇒ ②a/③a/③b/⑥b 必红 (rc=1)
```

证据：`tools/m2-acceptance/evidence/m2svr-rust-rng-log-plumb-20261013/`（卡 `openmir2-m2-rust-rng-log-plumb`）。

### 0b.4 `-ClientAbilityLayout` · 帧 52/53 的记录布局（M2 验收③ 的前置；卡 `openmir2-client-e2e-enter-gaps` 片 ㊷）

冻结客户端 `MirClient@c61d912d` 在 `Source/Common/Grobal2.pas:39` 写死 `HIGHHP = True`：帧 52 的 `TAbility`
是 **58B**、帧 53 `SM_HEALTHSPELLCHANGED` 从**尾**解 12B `TCharDesc`（`Source/MirClient/ClMain.pas:10220`）。
⇒ **Rust 生产默认**走**冻结客户端布局**（否则真客户端 HUD `0/0`、跑不动；根因/真机见
`mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/61-*`、`183-*`）。

但 **M2 验收③**（`s2c_diff.py` vs 金标准 `c-line-baseline-c3-20261010.jsonl`）比的是 `ident=52/53` 的
`body_len/body_sha256/raw_hex`，金标准是 **oracle 布局**（52 `body_len=40`、53 `body_len=0`）
⇒ ③a/③b 的 Rust 半**必须**以 oracle 布局起栈：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:\tmp\openmir2-m2svr-target\debug -ClientAbilityLayout 0
# 启动日志出现：CLIENT_ABILITY_LAYOUT: 将向 game-svr 注入 MIR2_CLIENT_ABILITY_LAYOUT=0（oracle-40/header-only（逐字节对拍））
#              … RUST_CLIENT_ABILITY_LAYOUT_INJECTED=0
# game-svr.out.log 出现：CLIENT_ABILITY_LAYOUT layout=oracle-40（…）
```

- **不传 ⇒ 不注入、不打该行**（与改动前逐字节一致，生产默认 = 冻结客户端布局 = 真客户端可玩）。
- 取值：`0|false|FALSE|False` = oracle 逐字节（③ 对拍）；`1|true|TRUE|True` = 冻结客户端布局。
  **不接受 `on`/`off`/`yes`/`no`**（Rust 读端 `smframe::client_layout_enabled()` 只把 `0|false|FALSE|False`
  当 oracle，起栈器口径必须与生产读取逐字一致）⇒ 认不出的值 **fail-closed**（exit 1，不起栈）。
- 管线实现（`Resolve-Mir2ClientAbilityLayout` / `Get-ClientAbilityLayoutInjectedLine` /
  `Start-SeededProcess -ClientAbilityLayout`）在 `tools/m2-acceptance/rust_seed_plumb.ps1`，与判据脚本共用同一份。
- 判红/判绿（不占 5000/7000/7100/7200；用临时目录 + 空闲端口起单个 game-svr）：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_client_ability_layout_plumb.ps1 `
  -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
# 15/15 ⇒ CLIENT_ABILITY_LAYOUT_PLUMB GREEN (rc=0)；删掉注入行 ⇒ ②a/③a 必红 (rc=1)
```

> **③ 口径提醒（不改变判据，只补操作面）**：`52/53` 的客户端布局是 §6.0-3 意义上的**故意偏离**
> （客户端冻结 > oracle 逐字节）。因此 ③a/③b **必须** `-ClientAbilityLayout 0`；而真机①②（客户端截图）
> **必须**用默认（生产 = 客户端布局）。两种口径不可同时满足，别拿一个口径的读数判另一个窗口。

证据：`tools/m2-acceptance/evidence/m2svr-client-ability-layout-plumb-20261014/`。

---

## 1. ① 端口所有权实拍 —— `check_ports.ps1`

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_ports.ps1
```

判据：对 `5000/7000/7100/7200` 逐个 `Get-NetTCPConnection -State Listen` 拿 `OwningProcess`，
再用 `Get-CimInstance Win32_Process` 取 **PID + 命令行**，判定实现归属**与该端口期望的入口**：

- GREEN：四端口都在听，命令行/可执行体判定为 Rust（`*\mir2-rs\target\*` 或已知 Rust 二进制名），
  **且入口基名对得上该端口**（`5000→game-svr` / `7000→login-gate` / `7100→sel-gate` / `7200→game-gate`）；
- RED：在听但判为 `csharp`（`E:\MirServer\*`）或 `unknown` ⇒ 验收①不成立；
  或四端口**都是 Rust 但入口接错线**（已知 Rust 入口挂在非期望端口，如 `game-svr` 起在 7200）⇒ 也判红
  （可用 `-AllowCrossEntry` 显式放行，默认不放松）；
- PREREQ：有端口不在听 ⇒ 退出码 2。

> **两层牙齿**：第一层只证明“有个 Rust 进程占着端口”（R2-1 的分类器）；第二层证明
> “是这个端口该的那个 Rust 入口”（`portclassify.ps1` 的 `Get-M2ExpectedEntry`/`Get-M2EntryVerdict`，端口表
> 来自设计文档 §2 + 本节）。两端口都是 Rust 但接错线，旧判据会 GREEN，新判据会 RED。

> 复用既有口径：`mir2-rs/tests/parity/m3/run_m3_gate.ps1` 的「实现识别护栏」同源；本脚本把 GameSvr 侧
> `5000` 也纳入，并要求四个端口**全为** Rust（混着 C# 即红）。

**失败怎么判红**：任意端口 `impl != rust` 或 `entry_verdict = cross` ⇒ exit 1；端口缺失 ⇒ exit 2。两者都不是 GREEN。

---

## 2. ② 真客户端 7 张截图 —— `check_screenshots.ps1`

入口：`D:\MirClient-run` + `run-release.cmd`（1024×768，参数 `127.0.0.1:7000`）。
驱动复用既有 `tools/client/mir_flow.ps1`（Win32 消息点击/打字/截图，阶段写 `stages.ndjson`）。

7 张画面 / 命名 / 截图时机（**画面内容由验收人逐张核对**；本工具只机判"交齐 + 非空"）：

| # | 文件 | 画面 | 截图时机 |
| --- | --- | --- | --- |
| 1 | `m2-01-enter-map.png` | 进图后 | 客户端进游戏、`SM_NEWMAP` 渲染完成（人物站在地图里，非黑屏）后 2–3s |
| 2 | `m2-02-walk.png` | 走路中 | 按住方向键走出数格、坐标已变化时 |
| 3 | `m2-03-talk.png` | 说话 | 聊天框输入并回车，本角色文本出现在聊天框后 |
| 4 | `m2-04-see-others.png` | 看见别人 | 另一角色进入屏内时（`mir_flow.ps1 -SeeOthersWaitSec N`，见下方复现命令） |
| 5 | `m2-05-attack.png` | 打怪 | 发起攻击、怪物受击（血条/伤害可见）时 |
| 6 | `m2-06-pickup.png` | 捡物（**oracle 不可达 ⇒ 默认不要求**） | 点击地面物品、物品消失或背包出现该物品后（**oracle 无地面物，此动作采不到**；见 §2.2 / T-25） |
| 7 | `m2-07-relogin.png` | 小退再进不黑屏 | 小退（CM_SOFTCLOSE）回选角 → 重进游戏、画面正常渲染后 |

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_screenshots.ps1 -ShotsDir E:\tmp\m2acc\run1\shots
# 默认必需 = 6 张（除 `m2-06-pickup` 外）；`-RequirePickup` 保留 7 张 legacy 口径。
```

**失败怎么判红**：`0` 张 ⇒ `PREREQ_MISSING`(exit 2)；必需张（默认 6）缺或存在 <1KiB 的图 ⇒ exit 1；必需张齐全非空 ⇒ exit 0。
（owner 裁决 `073f0dac`：`m2-06-pickup` 在 oracle 上不可达，详见 §2.2）

### 2.1 `m2-04-see-others` 的复现（2026-10-12 实测）

「看见别人」需要**第二角色落在观察者的 AOI 内**；最省的是 `tools/botload` 假人。两条实测坑：

1. **`run_bots.ps1` 判据达成即杀 BotSrv**（`login_ok`/`in_world` 达标后 `Stop-Process`）⇒ 做见人时
   **不能**用它常驻假人（会「先进世界又先掉线」）。直接跑 `src/BotSrv/bin/Release/BotSrv.exe`（cwd=该目录，
   读同目录 `AppSetting.json`：`ChrCount=1` / `LoginAccount=loadbot`），跑完手动收。
2. **假人角色坐标**取自 `mir2_db.characters`（假人登录后停在库里的 `CX/CY`）；要让它与观察者同 AOI，
   选一个库坐标相近的角色（如观察者 `bbb`@289,618 对 `loadbot0`@287,620，相距 (2,2)）。

```powershell
# 观察者用 mir2test 的第二角色（slot 2），进图后等 20s 再截 m2-04-see-others.png
powershell -ExecutionPolicy Bypass -File tools/client/mir_flow.ps1 `
  -Account mir2test -Password mir2pass -CharSlot 2 -OutDir E:\tmp\m2acc\seeothers `
  -SeeOthersWaitSec 20 -SkipTalk -SkipReenter -LeaveClientRunning
```

判据（owner §22）：第二角色进屏后观察者应收到 `SM_TURN(10)`+`SM_FEATURECHANGED(41)`（AOI 下行）；
真机证据 = 截图里**两个角色同屏**（客户端只经该帧渲染其它 actor），服务端侧 `同屏展示表登记` + 两人同时在世界的日志为佐证。

> 目录占位见 `tools/m2-acceptance/shots/`（大图归档到 `E:\tmp`，不入库）。

### 2.2 `m2-06-pickup` 在 oracle 上**不可达**（2026-10-12，owner 裁决 `073f0dac` ⇒ 判据已改写）

第 6 张「捡物」要求「点击地面物品、物品消失或背包出现该物品」。但**真实 oracle 栈里地面从来没有物品**，
所以这张图在 **C# 与 Rust 两侧都物理采不到**（不是 Rust 缺件）：

- `MapItem.ItemId` 在全仓**无任何未注释赋值点**（`MapItem.cs:41` 那唯一一行是注释）⇒ 恒 0；
- `Envirnoment.AddItemToMap` 首行 `if (mapItem.ItemId == 0) return false;` ⇒ 掉落/脚本造物**恒进不了地图**
  （与 `m3bag` 的 **T-19** 同源）；
- ⇒ `ClientPickUpItem` 的 `Envir.GetItem(CurrX,CurrY,ref mapItem)` 永远取不到 ⇒ `CM_PICKUP` 无物可拾。

**owner 裁决（`073f0dac`，2026-10-12）**：

- **不批** `crates/world` 写分片（回写会让 Rust 删掉一件 oracle 从未落到地上的物，反而制造偏离）；
- ② 第 7 格判据**改写为**「**无地面物**这一唯一可达形态下，两侧对 `CM_PICKUP` 的处理一致：
  不产生任何下行帧、不进包、不崩、不改变会话态势」；
- 登记 **T-25**（见 `mir2-rs/tests/parity/whitelist.md`）。

落地：`check_screenshots.ps1` 默认**不要求** `m2-06-pickup`（`-RequirePickup` 保留 legacy）；
可达面两侧逐字段一致由 `m3bag` 的 `ground-probe`（23 用例，`ItemId` 由夹具显式给值）+ `ground.rs::matches_oracle_ground_fixture` 钉住。

机器可判门禁与证据：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/check_pickup_unreachable.ps1
# 0=证明成立 / 1=证明被推翻（发现新的 ItemId 赋值点） / 2=PREREQ（缺 src/**）
```

证据：`tools/m2-acceptance/evidence/m2svr-pickup-unreachable-20261012/**`（含改坏必红原始件 +
部署 C# 栈上 bot 的实拍 `CM_PICKUP`：无地面物、无 `SM_ADDITEM`）。

---

## 3. ③ 同序列出包 diff —— `export_s2c.ps1` + `s2c_diff.py`

**复用既有流水线，不新写一套导出器/解码器**：

```
客户端 ↔ 7200 经 tools/capture/mir2_proxy.py 抓包        （dump 目录：chunks.ndjson + conn-*.bin）
  → python tools/capture/segment_frames.py               （→ frames.ndjson + manifest.json）
  → python tools/capture/dump_body_layout.py             （→ body_layout.json，当前分段口径；防 A-9 假红）
  → dotnet run tools/capture/GoldenExport --layout … --verify-roundtrip  （→ capture.jsonl，schema 与金标准同口径）
  → cargo run -p replay -- capture.jsonl --no-coverage    （既有：逐字节往返 + 字段对拍，自洽门禁）
  → python tools/m2-acceptance/s2c_diff.py                （与金标准按入图序配对 + 归一化 + 差异报告）
```

```powershell
# ③a 导出 + replay 自洽
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/export_s2c.ps1 `
  -ProxyDir E:\tmp\m2acc\run1\proxy -OutDir E:\tmp\m2acc\run1\rust
# ③b 与金标准对账（同序 + 逐字节，diff 必须 0 或全部进白名单）
#    口径 = owner §22 裁决：入图序 = 658→46（15 条自帧）；10 SM_TURN 是 AOI 帧，单列判（见 §2.1）。
python tools/m2-acceptance/s2c_diff.py `
  --rust E:\tmp\m2acc\run1\rust\capture.jsonl `
  --golden mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl `
  --start-ident 658 --upto-ident 46 --out E:\tmp\m2acc\run1\s2c-diff.json
```

> **`--upto-ident 46`，不是 `10`**（owner §22，2026-10-12）：金标准 `#16 ident=10` 的 `recog=0x7AC11879 ≠ ActorId
> 0x7C303998`，oracle 唯一发点在 `PlayObject.Message.cs:1542` 的 `if(baseObject!=this)` ⇒ **`10` 是「看见另一个 actor」的
> AOI 出包，不是自身入图帧**；拿 `658→10` 全序判「入图序」会把一个 AOI 事件算成入图帧。故入图序只判 `658→46`，
> `10` 由 §2.1 的 see-others 那一格单独判。**不许**把 `10` 塞进 `--upto-ident 10` 再用白名单糊过去。
>
> **墙钟字段**：`46 SM_DAYCHANGING` 的 `param`(=Bright) 与 `51` 的 `series`(=DayBright) 由 oracle 按**真实小时**算
> （`SystemProcessor.GetGameTime`）⇒ 金标准（09:28 采，白天=1）与夜里重跑（=3）必然不同；这**不是** Rust 偏差，
> 要么同刻采、要么按 §6.0-3 逐条白名单（**owner 签字**）。

> **金标准用哪份**：默认是 A 线登记、`golden_fresh_check.py` 保活的当前口径金标准
> `mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl`（sha256=f34a6d11…；与当前
> `segment_frames.py` + `GoldenExport --layout` 重导件逐字节相同）。卡上提的
> `tests/golden/session-20261010-c3-hook/capture.jsonl` 是 **A-9 旧口径**（多段帧 811 被当 single，
> `body_len/body_sha256` 与分段解码不一致），拿它做金标准会在 811 出一对**假红**；
> A-9 已登记在 `mir2-rs/tests/parity/whitelist.md`，真要拿旧件对就先白名单该帧。

对拍口径（`s2c_diff.py`）：

- **同序**：`--start-ident 658` 起、到 `--upto-ident 46`（`SM_DAYCHANGING`）止，两侧 ident 序列逐项相等；
  （`10 SM_TURN` 是 AOI 帧，不在入图序内——见上条 owner §22 与 §2.1）
- **等长**：逐帧 `body_len` 相等（同序天然等长）；
- **逐字节**：`raw_hex` 在**只归一化 `recog`**（=oracle `ActorId`，会话相关随机正数）后逐字节相等；
  归一化由既有 `mir2-rs/tests/parity/loginsrv/oracle_capture.py` 的 EDCode 完成（importlib 复用）；
  另比 `ident/param/tag/series/body_sha256`（把**尾**钉死，与 `recog` 无关）。
- **归一化清单**写进报告 `normalization`：本卡只允许 `recog` 一项。要加字段必须进 `--whitelist`，
  且每条必须带 `reason` + `reviewer`（§6.0-3：差异要么 0，要么逐条登记"差异点/为什么无害/谁审的"）。
- **白名单**：仓库内登记件 `tools/m2-acceptance/whitelist.jsonl`（`run_acceptance.ps1` 默认带上；
  文件缺失则不传，按 0 差异判红）。当前登记 4 行，两条根因（owner §25.3 裁决）：
  `ident=46 field=param`、`ident=50 field=series`；每条的 `raw_hex` 行是**同一字节差异的投影**，
  故同因同审各多一行（避免靠改判据判绿）。**两侧采集口径**：金标准 2026-10-10 **09:28**（白天
  `Bright=1`，角色 `Dir=0`）/ Rust 侧 2026-10-12 **01:28**（夜间 `Bright=3`，观察者 `Dir=4`）。
  证据见 `evidence/m2svr-whitelist-20261012/**`。

> **52 复查（owner `f963c1b3`，2026-10-12 03:xx）**：`m3bag` 的 `52 WAbil` 修复
> （`3434f219`+`511d575b`）随 master 合入后重跑 ③ ⇒ **同序 + 等长 + 逐字节 diff = 0**，
> 只剩 `46.param`/`46.raw_hex` 两条（夜间 `Bright=3`）。`52` 的 `max_hp/max_mp/dc` 已与金标准相同。
> **夹具对齐**：`52` 的 `hp`（`BaseObject.Abil.cs:21-24` 保留记录 `Hp`）与 `weight`
> （`BaseObject.cs:1356` `RecalcBagWeight()`）取自角色记录 ⇒ 金标准采集时该角色 `Hp=15`、背包有模板重 14 的物件，
> 故重跑前把 `mir2test/aaa` 的 `CX/CY=287,618`、`Dir=0`、`Hp=15`、背包 1 件 `StdIndex=15`（重 14）对齐，
> **收工已还原**。未对齐时的 RED 对照与逐字段表见
> `evidence/m2svr-whitelist-rerun-20261012/**`（含 `s2c-diff-fixture-unaligned.*` 与 `field-diff-52.txt`）。

**失败怎么判红**：缺 dump / 缺 dotnet ⇒ exit 2；切帧/导出/replay 失败 ⇒ exit 1；
对账有**未登记**差异 ⇒ exit 1；0 差异或全部落在白名单 ⇒ exit 0。
金标准的**身份**由 `preflight.ps1` 现场校验（不靠 README 抄 hash）：本步金标准是
`c-line-baseline-c3-20261010.jsonl`（`sha256=f34a6d11…`）；`session-20261010-c3-hook/capture.jsonl` 的
`3052C778…` 是 A-9 旧口径，**不是**本步金标准（拿它会在 811 出假红）。

---

## 4. ③ 三路径 DB 字段级 diff —— `check_db_paths.ps1`

复用既有 DB 工具：`tools/dbsnap/dbsnap.py`（13 表快照 + sha256 manifest）与
`mir2-rs/tests/parity/m3/scoped_diff.py`（M3 对拍判据本体：作用域内字段级 diff）。

三条退出路径：`softclose`（小退）/ `disconnect`（掉线）/ `shutdown`（服务关闭）。

一次「侧运行」（C# 与 Rust 各一遍，**同一前缀**）的采法（`-Mode snap`，在每步动作前后各跑一次）：

```powershell
# 动作前
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase before `
  -Prefix m2acc1 -SnapDir E:\tmp\m2acc\csharp\snap-softclose-before -OutDir E:\tmp\m2acc\csharp
#   ← 验收人执行动作：真客户端点「小退」
# 动作后
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode snap -Path softclose -Phase after `
  -Prefix m2acc1 -SnapDir E:\tmp\m2acc\csharp\snap-softclose-after -OutDir E:\tmp\m2acc\csharp
# disconnect / shutdown 同理（掉线=杀客户端进程；服务关闭=停栈）
```

`snap` 模式内部就是既有工具的两条命令（不新写）：
`dbsnap.py snap --out <SnapDir>` → `scoped_diff.py extract <SnapDir> <Prefix> <OutDir>\<path>-<phase>.jsonl --normalize`。
Rust 侧用**同一个 `-Prefix`**、`-OutDir E:\tmp\m2acc\rust` 再采一遍。

对拍：

```powershell
powershell -File tools/m2-acceptance/check_db_paths.ps1 -Mode diff `
  -CsharpDir E:\tmp\m2acc\csharp -RustDir E:\tmp\m2acc\rust -Canonical
# 内部对三条路径各跑：python …/scoped_diff.py diff <csharp>-after.jsonl <rust>-after.jsonl --canonical
```

> ⚠ **`-Canonical` 必须带**（2026-10-12 真机验证）：两次「侧运行」的**墙钟/id 自增**列必然不同
> （`account.LastLoginTime`、`characters.LASTUPDATE`、`characters_indexes.ModifyDate`），不 canonical ⇒ 真机 ④ **恒 RED**（假红）。
> `--canonical` 是 M3 对拍本体一直用的口径（`run_m3_gate.ps1:179`、`run_bag_equip_pair.ps1:121`），只把
> `scoped_diff.VOLATILE_COLUMNS`（逐列有理由）置占位；**行为列照比**（`selftest.ps1` 的 `db/canonical 行为列不同=RED` 有牙齿）。
> `run_acceptance.ps1` 的 ③c 步骤已自动带上 `-Canonical`。
>
> ⚠ 另一条已修缺陷：本机 MySQL 无口令时 `--password ""` 会被 **PowerShell 5.1 丢掉空实参** ⇒ 退化成
> `--password snap …` ⇒ `dbsnap.py` 报错、④ **假红**；现改为 `--password=<值>`。

**真机结果（2026-10-12）**：三条路径 **全 GREEN（11 张表逐行一致）**；不带 `-Canonical` 的同批数据**全 RED**（差异全在墙钟列）。
证据：`tools/m2-acceptance/evidence/m2svr-dbpaths-20261012/**`。

**期望**：三条路径的 `scoped_diff.py diff` 全部 **exit 0（字段级 diff = 0）**；任何差异要么修到 0，
要么按 §6.0-3 登记 `mir2-rs/tests/parity/whitelist.md`。

**失败怎么判红**：缺 python（`PREREQ_MISSING`，由本脚本开头 + `preflight.ps1` 先行拦）/
MySQL 3306 不可达 / 缺快照抽取件 ⇒ exit 2；任一路径 diff != 0 ⇒ exit 1。

---

## 5. 顺序判据②（本片状态）

卡上原文：**入图序到 `SM_TURN` 同序且长度相等**。已在编码层落地并入库（`55358fc2`）：
`mir2-rs/bins/game-svr/src/smframe.rs` 的 `EnterWorldInputs::frames()` 按金标准入图序
`658,51,50,41,213,11029,42,766,54,5008,1106,52,752,811,46,10` 产出 16 条；
用例 `enter_world_sequence_matches_golden_order_and_bytes` 用**金标准真字节回填输入**后断言
「ident 序相等 + 16 条 `client_frame` 逐字节相等」（长度自然相等）。
`s2c_diff.py --upto-ident 46`（owner §22 收窄后）就是这条判据在**整栈验收**时的同一口径。

**生产路径**已下发 **658**（公告，本片新接）+ **51**（`SM_NEWMAP`）；其余 14 条由 `EnterWorldInputs`
的 `Option` 字段留空 ⇒ 不产出（**登记，不猜字节**，逐条见 §5.1 与本片新增台账
`docs/M2顺序判据②-对账台账.md`）。

> **2026-10-11 晚更新（本节的 658/51 口径已过期，authoritative 以台账为准）**：后续切片已把生产路径
> 推进到 **13/16** `{658,51,50,41,213,11029,42,766,54,5008,1106,811,46}`（依次：配置层/地图 flag 片、
> 42/46 片、50/41/213 同组片）。余 3 条 `52/752/10` 阻塞在 `RecalcAbilitys`（`WAbil`）与物品形状表。
> 每条的「缺哪个输入 / 归哪张卡 / 红检」以 `docs/M2顺序判据②-对账台账.md` §3/§4 为准；§5.1 表保留作
> 2026-10-11 早间的基线快照。

### 5.1 其余入图帧「缺哪个输入 / 由哪张卡提供 / 还差哪一步」

> **2026-10-11 更新**：两条依赖卡**已合入 master**——`openmir2-m2-map-loading`（`2a2e8d03`，`crates/envir`）
> 与 `openmir2-m2-record-fields`（`353a28e2`，`bins/game-svr/src/record.rs` 全字段）。下表据此重判
> 「还差哪一步」；**已合入 ≠ 已接线**（接线见 §5.2）。

| ident | 帧 | 缺的运行时输入 | 由哪张卡提供 | 还差哪一步（2026-10-11 重判） |
| --- | --- | --- | --- | --- |
| 658 | `SM_SENDNOTICE` | 公告文本（`NoticeMgr.GetNoticeMsg`） | ✅ 配置层（已接） | **已接线**：`notice.rs` 读 `<base>/<NoticeDir>/Notice.txt`、按 `ReadLine` 切行 + `\x20\x1B\x1B` 连接（金标准 #1 尾巴 320B 锚定）；`[Share] NoticeDir`（缺省 `Notice`）入 `config.rs`；`main.rs` 进世界时放入 `EnterWorldInputs.notice`。**R1-1**：空/缺/读失败 ⇒ 仍发 **header-only 658**（照 oracle 空尾分支）；**R1-2**：1:1 搬 `Replace("/r/n/r/n ", "")`（字面量正斜杠，真实语料 no-op）。**红检**：分隔符改 2B ⇒ `notice_roundtrip_matches_golden_658_tail` 必红；空公告跳过 658 ⇒ `empty_notice_still_sends_header_only_658` + 进程级 `enter_world_emits_sm_newmap_byte_equal_to_golden` 必红；去掉 `apply_notice_replace` ⇒ `literal_slash_r_n_replace_is_carried_over` 必红 |
| 51 | `SM_NEWMAP` | ✅ 逻辑名+坐标（记录）；`map_id`/尺寸 ✅（envir）；**`DayBright()`** | `Env.Flag`（envir **未解析**） | envir 卡只给 cells/doors/points/尺寸/map_id，**没有 `Envir.Flag.DayLight/boDarkness`** ⇒ `DayBright()` 仍缺；占位 0 |
| 50 | `SM_LOGON` | `MessageBodyWL=(GetFeatureToLong,CharStatus,Tag1,Tag2)` | 物品/装备层（未开卡）+ world | `GetFeatureToLong` 要 `UseItems[Dress/Weapon]` 的 `StdItem.Shape`（`PlayObject.cs:3395`）⇒ 需物品系统 |
| 41 | `SM_FEATURECHANGED` | `GetFeatureToLong()` | 同上 | 同 50 |
| 213 | `SM_ATTACKMODE` | `AttatckMode` | ✅ `record.rs` 全字段（已合） | 只剩把 `CharacterData.attatck_mode` 带进 `EnterWorldInputs`（属 §5.2 的下一片） |
| 11029 | `SM_SERVERCONFIG` | `ClientConf` 序列化原文 | 配置层（未开卡） | `SystemShare.Config` 落地 + `EncodeMessage` |
| 42 | `SM_USERNAME` | `(targetId,nameColor,showName)` | ✅ `record.rs` + world | `GetChrColor`/展示名属 world 侧，未接线 |
| 766 | `SM_AREASTATE` | `Env.Flag.FightZone/SafeArea` + `InGuildWarArea` | `Env.Flag`（envir **未解析**） | envir 卡未解析 MapInfo 的 Flag 行 ⇒ 缺输入 |
| 54 | `SM_MAPDESCRIPTION` | `Envir.MapDesc` + `nMusicid=Flag.Music?MusicId:-1` | `Env.Flag`（envir **未解析**） | `MapInfoEntry.desc` 只是原文，Flag/音乐 id 未解析 ⇒ 缺输入 |
| 5008 | `SM_GAMEGOLDNAME` | `GameGold/GamePoint` ✅（记录）；元宝名/金币名 | 记录字段（已合）/ 配置层 | 只剩金币名配置（未开卡） |
| 1106 | `SM_VERSION_FAIL` | `ClientFile1_CRC/ClientFile2_CRC` | 配置层（未开卡） | `Config.ClientFile*_CRC` 落地 |
| 52 | `SM_ABILITY` | `Gold/Job/GameGold` ✅（记录）；`WAbil` 编码 | ✅ 记录字段（已合）+ 编码接缝（**本片补**） | **接缝已补**（`record::encode_ability`，序列化后丢成员数首字节，金标准 #12 锚定）；仍差**触发点接线**（`RM_ABILITY`/`RM_LEVELUP` 世界事件 → 下发） |
| 752 | `SM_SUBABILITY` | `AntiMagic/HitPoint/SpeedPoint/AntiPoison/PoisonRecover/HealthRecover/SpellRecover` | ❌ 这些是 **world 侧 `PlayObject`/`BaseObject` 状态**（`BaseObject.cs:105-121`，由 `RecalcAbilitys` 算出），并不在 `CharacterData`；`WAbil` 编码接缝已补 | 同 52 的触发点，且这 7 个值需先建 world 侧 `RecalcAbilitys` 产出（未建）⇒ 仍缺输入 |
| 811 | `SM_ADJUST_BONUS` | `BonusPoint` + A/B/C 模板 | ✅ `record.rs`（`bonus_point`/`bonus_abil`）+ 配置层 | A/B/C 取 `Config.BonusAbilofXxx/NakedAbilofXxx`（配置层未开卡） |
| 46 | `SM_DAYCHANGING` | `Bright/DayBright` | `Env.Flag`（envir **未解析**） | 同 51 的 `DayBright()`；`Bright` 属全局 |
| 10 | `SM_TURN` | `CharDesc(Feature+Status)` + `msg` | 物品层（未开卡）+ world | `Feature` 同 50 的 `GetFeatureToLong` ⇒ 需物品系统 |

> **本片新查明的两条精确阻塞**（比「哪张卡」更可操作）：
> 1. `crates/envir` **只解析格属性/门/点位/尺寸/`map_id`**，**没有** `Envir.Flag`（`FightZone/SafeArea/DayLight/boDarkness/Music/MusicId`）与 `MapDesc` 的语义 —— 帧 `51/766/54/46` 卡在这里；要么给 envir 补 `MapInfo` Flag 解析（触及 `crates/envir/**`，本片禁改），要么新开卡。
> 2. `record.rs` 全字段已入库，但**没有公开的 `Ability` 编码**（只有私有 `encode_opt_ability`）—— 帧 `52/752` 卡在这里（`EncodeMessage` 需「序列化后丢首字节」的半开区间口径）。
>    **2026-10-11 后续**：本片已补上公开 `record::encode_ability`（金标准 #12 逐字节锚定）⇒ **52 的输入接缝已解除**；752 另需 world 侧 `RecalcAbilitys` 产出（见上表）。
> 表中「配置层」（`11029/1106/5008/811`）尚无独立卡：需要在 M2 后续切片里开卡，或评估归并。
> `658` 已在本片接线（`notice.rs` + `[Share] NoticeDir`），不再是阻塞项。

### 5.2 本片接线：`CurMap`（逻辑名）→ `map_id` + 真地图尺寸（集成点 ④）

`openmir2-m2-map-loading` 卡面 ④「集成到 `bins/game-svr`（`WorldMaps` 真尺寸 / `CurMap`→`map_id`）」本片落地：

- `bins/game-svr/src/mapwire.rs`（新增）：`base_path`（`Server.conf` 所在目录 = 生产下的可执行体目录，
  oracle `M2Share.BasePath`）/ `load_registry`（`base/<EnvirDir>/MapInfo.txt` + `base/<MapDir>/*.map`）/
  `sizes`（`map_id` = 注册表下标）/ `resolve_map_id`（`require_id`，找不到 ⇒ `Err`）；
- `config.rs`：新增 `[Share] EnvirDir`/`MapDir`（oracle `ServerConf.cs:75-76` 的键，缺省 `Envir`/`Map`，**不发明键名**）；
- `main.rs`：启动时加载注册表（有 ⇒ 用其尺寸建世界并把 `CurMap` 换成 `map_id`；无 ⇒ 保留 S1 `WorldMaps` 占位并记 WRN）。
  **有注册表时找不到 `CurMap` ⇒ 拒绝进图并记 WRN**（照卡面④「不许静默占位」）；
- `WorldMaps` 占位键**降为可选键**（无 Envir 且写了它时的回退；两者都没有 ⇒ fail-closed，见 §5.2b）。

判据（`mapwire::tests`，不需要栈）：合成 `Envir/MapInfo.txt` + 合成 `.map` ⇒ 两张入表、`map_id` 按 section 序、
`[0129|0100]` 认**逻辑名** `0129`、未知名 ⇒ `Err`、无 Envir ⇒ `None`+`Ok(0)` 回退。
**红检**：把 `resolve_map_id` 改成 `id_of(cur_map).unwrap_or(0)` ⇒ `unknown_cur_map_is_rejected` 必红
（原始输出 `E:\tmp\openmir2-watch\pi-m2svr-wire-redcheck.log`）。

**R3 收口（reviewer-3 对 308396cd 的 4 条 Minor，2026-10-11 后续片）**：

| id | 问题 | 处置 | 判据（含红检） |
| --- | --- | --- | --- |
| R3-1 | `Placeholder` 同时吞掉「无 `MapInfo.txt`」与「文件在但 0 条有效 section」 | `load_registry` 用 `MapLoader::map_info_path().exists()` 区分：文件在但空表/全解析失败 ⇒ `Broken`（`main` 拒启），`Placeholder` 只留给**无文件** | `mapwire::tests::empty_map_info_file_is_broken_not_placeholder`；红检 `if info_path.exists()`→`if false` ⇒ 用例必红（`pi-m2svr-r3-redcheck-r31.log`） |
| R3-2 | `[Share]`/`EnvirDir` 按 ASCII 精确匹配，oracle `ConfigFile` 是 `StringComparer.OrdinalIgnoreCase` | section 与键名统一 `to_ascii_uppercase()` 后再匹配（**值不动**） | `config::tests::section_and_key_matching_is_ascii_case_insensitive`；红检 `"SHARE"`→`"Share"` ⇒ 必红（`pi-m2svr-r3-redcheck-r32.log`） |
| R3-3 | 显式相对 `--config`（`Server.conf`）⇒ `parent()=Some("")`，`Envir`/`Map` 相对 cwd | `base_path` 先把相对路径规范化到当前目录再取 parent（对齐 oracle `AppContext.BaseDirectory`） | `mapwire::tests::relative_conf_base_is_normalized_to_cwd`；红检回退 `conf.parent()` ⇒ 必红（`pi-m2svr-r3-redcheck-r33.log`） |
| R3-4 | `main.rs` 进世界分支缩进浅 4 格（长 `glog!` 使 rustfmt 跳过该块） | 手工重排缩进 + 把长 `glog!` 消息拆成 `concat!` 两段 | `cargo fmt --all -- --check` exit 0（改前 rustfmt 对该块**无输出**=stable，改后仍 stable） |

### 5.2b `WorldMaps` 可选化 + fail-closed（owner `272582e1` 裁决；本片）

真 `E:\MirServer\M2GameSvr\Server.conf` 的 `[Server]` 段**没有** `WorldMaps`（owner 已逐行核过；
`grep WorldMaps src/` 为空，它是 Rust S1 引入的占位），真地图表在 `[Share] EnvirDir`（现网 `Envir`）下。
故 `WorldMaps` 从**必需键降为可选键**（不改现网配置——内容冻结），三态口径：

1. `Envir/MapInfo.txt` 可读 ⇒ 用**真注册表**尺寸（已实现）；
2. Envir 不可读**且**有 `WorldMaps` ⇒ 用占位尺寸并记 `WRN`（开发/测试形态）；
3. **两者都没有 ⇒ 仍必须非零退出**（fail-closed：`mapwire::placeholder_sizes` 返回 `Err`，`main` 在起
   sim 线程/绑端口前 `?` 掉），不许静默起空世界 / 回落 0 号占位图。

代码面：`config.rs`（`WorldMaps` 缺失 ⇒ 空表；**写了但非法**如 `10x0` 仍报错）、`mapwire.rs`
（新增 `placeholder_sizes`）、`main.rs`（把「注册表优先，否则占位且非空」算好再传给 sim）。

判据 + 红检（三条都要"改回必红"；原始输出 `E:\tmp\openmir2-watch\pi-m2svr-wm-redcheck1|2|3.log`）：

| 红检 | 用例 | 改回什么 ⇒ 必红 |
| --- | --- | --- |
| ① 真配置（无 `WorldMaps`）+ 真 Envir ⇒ 起得来 | `config::tests::deployed_conf_without_worldmaps_parses`、`mapwire::tests::envir_registry_covers_missing_worldmaps`、`relay.rs::starts_without_worldmaps_when_envir_present`（进程级，真起监听） | `WorldMaps` 降回必需键 ⇒ 三个全红（进程级实测 `未在预期时间内监听`） |
| ② 无 Envir + 无 `WorldMaps` ⇒ 非零退出 | `mapwire::tests::no_envir_and_no_worldmaps_is_fail_closed`、`relay.rs::missing_worldmaps_and_envir_exits_nonzero`（10s 内必须自退，超时=静默放行也判红） | `placeholder_sizes` 静默放行 ⇒ 两个全红 |
| ③ `WorldMaps` 值非法 ⇒ 仍报错 | `config::tests::bad_worldmaps_value_is_still_fatal` | 去掉 `w<=0||h<=0` 校验 ⇒ 必红 |

### 5.3 顺序判据② 的「生产路径覆盖率」与触发组口径（本片登记）

生产路径只下发**整个 oracle 触发组**输入都已齐备的帧；登记时容易漏掉「组」这一层：

- `SM_LOGON(50)` / `SM_FEATURECHANGED(41)` / `SM_ATTACKMODE(213)` 在 oracle 里是**同一个** `PlayObject.SendLogon()`
  （`PlayObject.cs:1127-1147`）里连发的三条；`213` 的输入（`AttatckMode`）虽已可从记录取到，
  但 `50/41` 的 `GetFeatureToLong()` 仍需物品/装备系统 ⇒ **本片不单独下发 213**（否则会在客户端上
  出现“有攻击模式、无登录帧”的半组，比不下发更坏），登记为**整组就绪后再发**。
- `SM_ABILITY(52)`/`SM_SUBABILITY(752)` 在 oracle 由 `RM_ABILITY`/`RM_LEVELUP` 触发（`PlayObject.Message.cs:1673/1732`）；
  52 的记录输入与编码接缝已齐，但触发点属 S3 世界事件接线；752 还缺 world 侧状态。
- 因此本片**只新增 658 一条**（公告，配置层输入已就绪）；`51` 早先已在生产集合内。`EnterWorldInputs::frames()` 的 16 条
  顺序判据是编码层判据，生产覆盖率按“组”推进。

> 结论口径：**“已合入/已备输入”不是接口“已下发”**。以后每接一帧，都要先确认它所在的
> oracle 触发组能产出的成员与顺序，再改生产集合，并同步 §5.1 与本表。

---

## 6. 门禁（改本目录后必须全绿）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
cargo run -p msgcodegen -- --check
```

本目录是脚本/手册，不引入 Rust 代码；R4-1 单测在 `mir2-rs/bins/game-svr/src/main.rs`（game-svr 包），
52 输入接缝的单测在 `mir2-rs/bins/game-svr/src/record.rs`（`encode_ability_matches_golden_frame_12_tail`）。

## 7. 本片自检证据（无栈）与残留物清单

本片**不起栈、不起客户端、不占端口**；已跑过的自检（判据 + 原始输出）：

| 自检 | 命令 | 结果 | 原始输出 |
| --- | --- | --- | --- |
| 无栈总自检 | `run_acceptance.ps1 -SelfCheckNoStack` | 5 步全 `PREREQ`、整体 exit 2（不静默通过） | `E:\tmp\openmir2-watch\pi-m2svr-nostack-selfcheck.log` |
| s2c 导出+replay（用 M0 金标准裸 dump） | `export_s2c.ps1 -ProxyDir tests/golden/…/proxy` | replay `RESULT: GREEN`（byte 0 / field 0） | `E:\tmp\openmir2-watch\pi-m2svr-s2c-selftest.log` |
| s2c 对账（同序） | `s2c_diff.py --rust <重导件>` | `GREEN`（16 条同序 + 逐字节 0，归一化仅 recog） | 同上 |
| s2c 红检 | 改 recog ⇒ 仍 GREEN；改尾一字节 ⇒ RED | 归一化只吸 recog、字节差异必红 | 同日志 / `E:\tmp\rust-body-broken.jsonl` |
| 三路径 DB diff 驱动 | `check_db_paths.ps1 -Mode diff` 合成夹具 | 同件 GREEN / 改一字段 RED（复用 `scoped_diff.py`） | 控制台（rc 0 / rc 1） |
| **手册判红自检** | `selftest.ps1` | **33/33 OK**：②截图 4 例、③b s2c 4 例（尾字节改坏⇒RED、只改 recog⇒GREEN）、③c DB 3 例、①端口实况判属 1 例、①a 分类器 7 例（R2-1 + 本片真机回归 2 例）、①b 端口表→期望入口 8 例、⓪ preflight 4 例、聚合器 2 例 | `E:\tmp\openmir2-watch\pi-m2svr-real-selftest.log` |
| **R3e-1 端口表补全** | `Test-Entry` 补 `7000=login-gate` / `7100=sel-gate`（原只断言 5000/7200） | 表漂移（`7000→login-srv`）⇒ 对应用例 **FAILED**（`expect=login-gate actual=login-srv`）；还原后全绿 | `E:\tmp\openmir2-watch\pi-m2svr-pf-redcheck-porttable.log` |
| **真机回归·端口实现归属（本片）** | `portclassify.ps1`：C# 根只认**可执行体路径**（Rust 入口正常用 `E:\MirServer\...\config.conf` 参数，旧实现把整条命令行当探针 ⇒ 误判 csharp） | 改回 `$probe -like "*E:\MirServer\*"` ⇒ `impl/Rust 入口带现网配置参数仍=rust` **FAILED**（`expect=rust actual=csharp`）、`total=33 failed=1`；还原后 33/33 | `E:\tmp\openmir2-watch\pi-m2svr-real-redcheck-portclassify.log` |
| **真机回归·网关线号（本片）** | `bins/game-svr` `GateSlots::alloc` 把「按连接唯一分配」改回恒 `Some(0)` | `gate_slots_are_unique_per_connection_and_reusable` **FAILED**（第二个连接拿到 0 而非 1）；还原后 58 单测全绿 | `E:\tmp\openmir2-watch\pi-m2svr-real-redcheck-gateslot.log` |
| **⓪ 前置门（本片）** | `preflight.ps1`（工具/输入/金标准身份+schema） | 本机齐备 ⇒ `SUMMARY ok=18 missing=0 red=0`、exit 0；`-PythonCmd m2-no-such-python-xyz` ⇒ exit 2(`PREREQ_MISSING`)；`-SkipTools -ClientDir <不存在>` ⇒ exit 2；坏 schema 金标准（`-AllowGoldenDrift` 隔离）⇒ exit 1(`RED`) | `E:\tmp\openmir2-watch\pi-m2svr-pf-preflight.log` |
| **⓪ 红检·schema** | 把 `golden:schema` 强制 `OK` | `preflight/金标准 schema 坏=RED` **FAILED**（`expect_rc=1 actual_rc=0`）⇒ `SUMMARY total=28 failed=1`；还原后 29/29 | `E:\tmp\openmir2-watch\pi-m2svr-pf-redcheck-schema.log` |
| **⓪ 红检·工具** | 把 `Get-Command` 探针强制命中 | `preflight/缺工具=PREREQ` **FAILED**（`expect_rc=2 actual_rc=0`）⇒ `SUMMARY total=28 failed=1`；还原后 29/29 | `E:\tmp\openmir2-watch\pi-m2svr-pf-redcheck-tool.log` |
| **①实况判属去竞态（本片）** | `selftest.ps1` 的实况用例改为「先跑 `check_ports.ps1` 并读**这一次**的证据 JSON 反推期望」 | 不再另起一次 `Get-NetTCPConnection`；别的车道正在起/停栈时不再假 FAIL（本次实测四端口被 C# 占：`nonRust=5000/7000/7100/7200`、exit 1，判据仍自洽） | `E:\tmp\openmir2-watch\pi-m2svr-pf-selftest.log` |
| **端口表→入口 红检（前一片）** | 把 `Get-M2EntryVerdict` 的 `cross` 分支改回 `"ok"` | `entry/5000+login-gate=cross` 与 `entry/7200+game-svr=cross` 两条 **FAILED**（`SUMMARY total=25 failed=2`、`SELFTEST FAIL`）；还原后 25/25 | `E:\tmp\openmir2-watch\pi-m2svr-portentry-redcheck.log` |
| 52 输入接缝 | `cargo test -p game-svr --offline encode_ability` | 1/1 OK（`encode_ability` 逐字节等于金标准 #12 的 `EncodeMessage(WAbil)` 体） | `E:\tmp\openmir2-watch\pi-m2svr-ability-test.log` |
| R4-1 红检 | 改回 `unwrap_or(0)` | `route_to_gate_rejects_out_of_range_gate_idx` FAILED；还原后 passed | `E:\tmp\openmir2-watch\pi-m2svr-r41-redcheck.log` |
| **CurMap→map_id 接线单测** | `cargo test -p game-svr --offline mapwire` | 3/3 OK（逻辑名→id / 未知名必拒 / 无 Envir 回退） | 含在 `pi-m2svr-wire-test.log` |
| **CurMap→map_id 红检** | 把 `resolve_map_id` 改成 `id_of(cur_map).unwrap_or(0)` | `unknown_cur_map_is_rejected` FAILED（rc=101）；还原后 3/3 OK | `E:\tmp\openmir2-watch\pi-m2svr-wire-redcheck.log` |
| **R3-1/2/3 单测** | `cargo test -p game-svr --offline`（mapwire 6 / config 4） | 全 `ok`；新增 3 条（空 MapInfo ⇒ Broken / 大小写不敏感 / 相对 conf 规范化到 cwd） | `E:\tmp\openmir2-watch\pi-m2svr-r3-unittests.log` |
| **R3-1/2/3 红检** | 分别注入 `if false` / `"SHARE"→"Share"` / 回退 `conf.parent()` | 三条新用例各自 **FAILED**（rc=101）；还原后全绿 | `pi-m2svr-r3-redcheck-r31/r32/r33.log` |
| **R3-4 rustfmt** | `cargo fmt --all -- --check`（改前对进世界块无输出 ⇒ 只靠手工重排） | 改后 exit 0 | 门禁日志 |
| 四门禁（接线后 head） | fmt / clippy `-D warnings` / `test --workspace --offline` / msgcodegen `--check` | fmt 0 / clippy 0 / **51 目标 355 passed 0 failed 11 ignored** 0 / msgcodegen 604 OK 0 | `pi-m2svr-wire-test.log`、`pi-m2svr-wire-gates.log` |

残留物清单：

- `tools/m2-acceptance/selftest.ps1`、`tools/m2-acceptance/mutate_capture.py`（本片新增：判红/判绿自检 + s2c 红检夹具生成器）；
- `tools/m2-acceptance/portclassify.ps1`（R2-1 的共享分类器；本片加「端口表→期望入口」的第二层牙齿：`Get-M2ExpectedEntry`/`Get-M2EntryName`/`Get-M2EntryVerdict`）；
- `tools/m2-acceptance/preflight.ps1`（**本片新增：⓪ 前置门**——工具/仓库输入/金标准身份+schema）；`run_acceptance.ps1`/`export_s2c.ps1`/`check_db_paths.ps1`（本片：接入 ⓪、缺 python 显式报 `PREREQ_MISSING`）；`selftest.ps1`（本片：+4 条 ⓪ 用例、实况判属改读同一次证据去竞态）；
- `E:\tmp\m2-acceptance\`：无栈自检产生的 `ports-*.json`/`shots-*.json`/`run-*`/`s2c-selftest/` 等，保留作证据；
- `E:\tmp\m2-acceptance\selftest-*\`：`selftest.ps1` 的 33 条构造夹具与 `selftest.json`（证据，保留）；
- `E:\tmp\m2-acceptance\preflight-*.json`：本片 `preflight.ps1` 的齐备/缺工具/坏 schema 三组证据（保留）；
- `E:\tmp\openmir2-watch\pi-m2svr-pf-*.log`：本片前置门、29 条自检、无栈自检、两条红检原始输出（证据，保留）；
- `E:\tmp\openmir2-watch\pi-m2svr-*.log`：四门禁、红检、自检原始输出；
- `E:\tmp\rust-recog-changed.jsonl` / `rust-body-broken.jsonl`：s2c 归一化红检输入（证据）；
- `E:\tmp\openmir2-watch\pi-m2svr-wire-redcheck.log`：`CurMap→map_id` 接线红检原始输出（证据，保留）；
- `E:\tmp\openmir2-watch\pi-m2svr-wire-test.log` / `pi-m2svr-wire-gates.log`：接线后四门禁原始输出（证据，保留）；
- `E:\tmp\openmir2-watch\pi-m2svr-portentry-selftest.log` / `pi-m2svr-portentry-redcheck.log`：本片端口表→入口自检与红检原始输出（证据，保留）；
- 未新增/未修改进程、端口、库内行（本片全程只读外部状态）。

---

## 8. 每一步都验过会红：`selftest.ps1`

`run_acceptance.ps1` 的 ⓪ 前置门 + 四步各有「失败怎么判红」的书面口径。本脚本用**构造输入**把每条口径都跑一遍，
断言「退出码 **和** 输出关键字」两者都对——**只看退出码会把“脚本崩了”误当“判红”**：

```powershell
powershell -ExecutionPolicy Bypass -File tools/m2-acceptance/selftest.ps1
# 47 条全 OK ⇒ exit 0；任何一条不符 ⇒ exit 1（SELFTEST FAIL）
```

| # | 步骤 | 构造输入 | 期望 |
| --- | --- | --- | --- |
| 0a | ⓪ | `preflight.ps1 -SkipTools -SkipClient`（在库前置） | `0` + `GREEN: 前置齐备` |
| 0b | ⓪ | `-PythonCmd m2-no-such-python-xyz`（缺工具） | `2` + `PREREQ_MISSING` |
| 0c | ⓪ | `-SkipTools -ClientDir <不存在>`（缺客户端入口） | `2` + `PREREQ_MISSING` |
| 0d | ⓪ | 坏 schema 金标准 + `-AllowGoldenDrift`（隔离 schema） | `1` + `RED: 前置在但已损坏` |
| 1 | ② | 空截图目录 | `2` + `PREREQ_MISSING` |
| 2 | ② | 只有 6/7 张 | `1` + `RED: 截图不全` |
| 3 | ② | 7 张但有一张 <1KiB | `1` + `RED: 截图不全` |
| 4 | ② | 7 张齐全非空 | `0` + `GREEN: 7 张` |
| 5 | ③b | 当前口径金标准自比 | `0` + `GREEN: 同序列` |
| 6 | ③b | `SM_NEWMAP(51)` 尾翻 1 字节（非 recog） | `1` + `RED: …未登记差异` |
| 7 | ③b | 只改 `Recog`（归一化清单唯一字段） | `0` + `GREEN: 同序列` |
| 8 | ③b | `--rust` 指向不存在的文件 | `2` + `PREREQ_MISSING` |
| 9 | ③c | 三路径字段级相同 | `0` + `GREEN: 小退/掉线…` |
| 10 | ③c | `shutdown` 改一个被比对字段 | `1` + `RED: 路径 DB diff 未归零` |
| 11 | ③c | 缺 `shutdown-after.jsonl` | `2` + `PREREQ_MISSING` |
| 12 | ① | **本机实况** 5000/7000/7100/7200 | 全 Rust `0` / 有非 Rust 在听 `1` / 有端口没听 `2`（期望由**同一次运行** `ports.json` 证据里的 `not_listening/not_rust/cross_entry` 反推，不两次探测、不写死，避开别的车道起/停栈的竞态） |
| 12a | ①a | 分类器：`E:\MirServer\GameSvr.exe` | `csharp` |
| 12b | ①a | 分类器：`…\mir2-rs\target\…\game-svr.exe` | `rust` |
| 12c | ①a | 分类器：`E:\tmp\openmir2-m2svr-target\…\game-svr.exe` | `rust` |
| 12d | ①a | 分类器：`C:\dev\game-svr.exe`（基名回落） | `rust`（**R2-1**：旧内联判据在此会算成非 Rust） |
| 12e | ①a | 分类器：`C:\dev\foo.exe` | `unknown` |
| 12k | ①a | **真机回归**：`E:\tmp\openmir2-m2svr-target\debug\login-gate.exe` + 参数 `E:\MirServer\LoginGate\config.conf` | `rust`（旧实现看整条命令行会误判 `csharp`） |
| 12l | ①a | `E:\MirServer\LoginGate.exe`（Exe 在 C# 根） | `csharp` |
| 12f | ①b | 端口表：`5000` / `7000` / `7100` / `7200`（4 例；**R3e-1** 补全 7000/7100） | `game-svr` / `login-gate` / `sel-gate` / `game-gate` |
| 12g | ①b | 入口基名：`…\target\debug\game-svr.exe` | `game-svr`（去目录/去扩展名/小写） |
| 12h | ①b | 归属：`5000` + `game-svr` | `ok` |
| 12i | ①b | 归属：`5000` + `login-gate`（接错线） | `cross`（→ `check_ports.ps1` 判红） |
| 12j | ①b | 归属：`7200` + `game-svr`（接错线） | `cross`（→ 判红） |
| 12m | ④ | 夹具 offline：旧日志含账号级 kick 文案 | `0` + `old_received=KICKUSER_SYSTEM_MSG` |
| 12n | ④ | 夹具 offline：新日志含 `成功进入游戏` | `0` + `new_verdict=ADMITTED` |
| 12o | ④ | 夹具 offline：新日志「进入游戏」后 `关闭连接` | `0` + `new_verdict=CLOSED` |
| 12p | ④ | 夹具 offline：先 `成功进入游戏`、之后再 `关闭连接` | `0` + `new_verdict=ADMITTED`（成功优先，不误报 CLOSED） |
| 12q | ④ | 夹具 offline：新日志无任何判定标记 | `1` + `new_verdict=TIMEOUT`（不静默通过） |
| 12r | ④ | 夹具 offline：新进程已退出 | `1` + `new_verdict=PROCESS_EXITED` |
| 12s | ④ | 夹具 offline：旧日志无 kick 文案 | `0` + `old_received=NONE_OBSERVED` |
| 12t | ④ | 夹具 offline：旧日志 `服务器连接被强行中断` | `0` + `old_received=SM_OUTOFCONNECTION` |
| 12u | ④ | 夹具 offline：旧日志仅 `关闭连接` | `0` + `old_received=CONNECTION_CLOSED` |
| 13 | 聚合 | `run_acceptance.ps1 -SelfCheckNoStack` | `2` + `PREREQ_MISSING` |
| 14 | 聚合 | `-SkipPorts -SkipS2c -SkipDb -ShotsDir <6/7>` | `1` + `RED: 有步骤判红`（**红不许被说成前置**） |

全部是**只读/构造夹具**：不起栈、不起客户端、不占端口（①只 `Get-NetTCPConnection` 取实况）。
证据：`E:\tmp\m2-acceptance\selftest-*\selftest.json`（逐条 name/expected/actual）。

**④ 会话生命周期判定（DoD ① 的『夹具自检』）**：`session_lifecycle_fixture.ps1` 的观测逻辑
（`Get-KickObservation` / `Get-NewSessionVerdict`）是纯文本分类，**offline 与真机共用同一实现**；
上面 12m–12u 九条不另写口径，而是直接调它的 offline 模式（`-OfflineLogA <旧日志> -OfflineLogB <新日志>`，
不起栈、不占端口）。真机 ①（需 5000/7000/7100/7200）跑之前，这九条就是它的「判据在起栈前已成立」的牙齿；
起栈后可直接 `-OfflineLogA/-OfflineLogB` 对已采日志复判。

**红检（夹具自检本身会红）**：把 `Get-NewSessionVerdict` 里的 `"成功进入游戏"` 匹配串改坏 ⇒
`life/新进世界=ADMITTED` 等 4 条必 `FAIL`、`SUMMARY total=47 failed=4`、exit 1（还原后 47/47）。
原始输出：`tools/m2-acceptance/evidence/session-lifecycle-selftest-20261012/`。

> **踩过的坑（写进脚本防复发）**：夹具 `.jsonl` 必须**无 BOM** 写。PS 5.1 的
> `Set-Content -Encoding utf8` 会写 UTF-8 BOM，python `json.loads` 直接 `Unexpected UTF-8 BOM` 抛错，
> 于是「相同件」也拿到 rc=1——**看起来像判红，其实是夹具崩了**（只看退出码就会假绿这条自检）。
> 脚本内改用 `[System.IO.File]::WriteAllText(..., New-Object System.Text.UTF8Encoding($false))`，
> 并同时断言输出关键字，正是为了区分「真判红」与「脚本崩」。

---

## 9. 真机首跑（2026-10-11）—— 结论与残留

首次把入口四端口交给 Rust 跑真机验收（`start_rust_stack.ps1` + `check_ports.ps1` + 真客户端
`mir_flow.ps1` + `mir2_proxy` 抓包 + `s2c_diff.py`）。完整报告：
[`docs/M2验收真机首跑报告-20261011.md`](../../docs/M2验收真机首跑报告-20261011.md)；证据：
`tools/m2-acceptance/evidence/real-run-20261011/`。

| 验收项 | 结果 | 说明 |
| --- | --- | --- |
| ① 端口所有权 | **GREEN** | 5000/7000/7100/7200 的监听均为 Rust 且入口对得上（`ports.json`） |
| ② 真客户端 7 图 | **RED** | 客户端能登录/选角/进世界（认证与取档经 C# DBSrv 6000 走通），但进世界后**黑屏**：生产路径只下发 **1/16** 条入图帧（`SM_NEWMAP` 一条），且断线未清世界（重登同一角色报 `AlreadyInWorld`） |
| ③ 同序 s2c diff | **RED** | Rust 侧 s2c 只抓到 1 条（`ident=51 SM_NEWMAP`），金标准序列起点 `ident=658 SM_SENDNOTICE` 不存在 ⇒ `s2c_diff.py` 直接判红 |
| ④ 三路径 DB diff | 未采 | 依赖②先成立，本轮登记 |

真机顺带暴露并收口的两条缺陷（详见报告 §3）：

1. **`bins/game-svr` 网关线号恒 0**（本片修）：三条 GameGate 连接都登记成 0 号线 ⇒ `gates` 表只剩最后一条，
   入图帧按 `gate_idx` 路由打到别的连接、`UserList` 找不到 socket ⇒ **静默丢弃顶包（客户端黑屏）**。
   照 oracle `TCPNetChannel.AllocGateSlot` 改为「按连接唯一分配 + 断线释放」。
2. **`portclassify.ps1` 误判**（本片修）：旧实现把**整条命令行**当探针，Rust 入口用现网
   `E:\MirServer\...\config.conf` 作参数时含 `E:\MirServer\` ⇒ 误判 csharp（验收①假红）。

尚待 owner/其它卡解除的前置（本轮用显式绕过参数，未改冻结部署）：

- ~~真 `E:\MirServer\M2GameSvr\Server.conf` **没有** `WorldMaps` 键~~ **已解除**（§5.2b：`WorldMaps` 降为可选键，
  `Envir` 在就用真注册表）。`-GameSvrConf` 仍保留为显式覆盖，但**不再是起真配置的必需品**。
- 本片起栈时分支头（`3ba17d11`）尚未包含 `openmir2-m2-gamegate` 的 GB2312 配置修复（`677678c2`，已在 master）
  ⇒ 本轮用 `-GameGateExe` 指向该分支构建件。**合并 master 后 head `9f5b2dd1` 自带修复**，
  `-GameGateExe` 已非必需（用 head 的 `game-gate.exe` + 真 GBK 配置实测能解析启动）。
