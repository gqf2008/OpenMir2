# M3②③ F-N1/F-N2 —— C# 专用流「只有被测量攻方」+ `-Repo` 回归收口（m3bag，2026-10-13）

> 卡：`openmir2-m3-attack-pair-fixture`（夹具卡）。
> 来源：owner 2026-10-12 22:15 ①（`atk_power_base` C#/Rust 不对成因） + m3cs 2026-10-13
> `M3②③活栈隔离口径F-F后两相实跑报告-20261013.md` §5/§7 的 **F-N1** / **F-N2**。
> 本片**零 `.rs`**、**未起栈、未占端口**、**未 merge、未自签 review**。

## 1. 结论（一句话）

C# 侧「玩家有目标击数」已由 F-F 对齐（两侧 120/120）；剩下的跨侧逐 k≠0 的**直接成因**是
**C# 专用流被进程内并发非玩家攻方污染**（调用点级 divert 不按实体）。本片把 hook 的隔离按
**被测量攻方线程**收窄，并在夹具下游加一道 **fail-closed 纯度门**（污染流 ⇒ 点名并**拒比**）；
另修 m3cs 现场撞到的 `start-csharp-hook.ps1 -Repo` 空绑定回归（F-N2）。

## 2. F-N1 —— 专用流被并发非玩家攻方污染

### 现象（m3cs 真机，证据只读）

`requests/40-analyze-clean.txt`：C# 专用流**首抽**就是 `atk_power_base`（非 PlayObject 攻方），
120 击里多出 `base=11`、`hit_gate 131 vs 120` ⇒ 被测量玩家攻方的流位置被整体推移
（`cA vs cB=242`、`c1 vs c2=328`，C# 不可复现）。`evidence-rng-isolate-attack-twophase-20261012/
provenance/csharp-attack-stream-stack2.tsv` 里 base=16、首条即 base（本片 05 直接判红）。

### 根因

`rng-hook` 的隔离按**调用点**分流（`divert-sites.txt` = `attack_sites.tsv` 的 `divert=1` 行），
**不按攻方实体**；同一 `GameSvr` 进程里**别图的怪物/守卫 AI**（跑在独立 `MobThread`，
`src/GameSrv/Word/WorldServer.MonGen.cs`）走同一批站点时也抽 `Random(attack_seed)`。

### 修法（m3bag 写集；不动谓词表接口）

- `mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`：新增 `RouteToAttackStream(site, inDivertSet, threadId)`
  ——玩家攻击**首抽**（`M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929` / `:2918`）把
  当前线程认领为「被测量攻方线程」；**只有该线程**的取数进专用流，怪物 `MobThread` 仍走原全局流。
  谓词表/站点集合不变（`check_predicates.py table-vs-csharp` 仍绿）。关闭开关时逐字节不变
  （`_attack == null` 时 `Route` 恒不回专用流）。
- `mir2-rs/tests/parity/m3/attack_stream_gate.py`（新）：C# 专用流**纯度门**——首单元必须是玩家攻方、
  `player 单元数 == -AttackCount`、`base 单元数 == 0`；否则点名（含专用流 1-based 行号）并判红。
- `run_attack_pair.ps1`：隔离开启且 `-Side csharp` 时强制跑该门；缺专用流文件 ⇒ RED。owner 23:50 ③
  「play 单元数 < 期望 ⇒ 直接拒跑」。
- `run_attack_pair.ps1 -SelfCheckNoStack` ⑫c（纯度门：干净绿 / 混入 base 单元·player 数不符 三向必红）
  与 ⑫d（F-N2）。
- `rng-isolate-attack/README.md` §6 记录 F-N1/F-N2。

### 判据 / 证据

| 项 | 结果 | 原始件 |
| --- | --- | --- |
| hook 离线自检 | **GREEN 55/55** | `01-hook-selftest-green.txt` |
| 改坏（去掉线程过滤） | **RED 5 FAIL / rc=1** | `02-hook-mutation-thread-filter-red.txt` |
| 纯度门自检 | **GREEN 11/11**（含 CLI rc 0/1/2） | `06-gate-selftest-green.txt` |
| 纯度门 × 真机污染流 | **RED / rc=1**（`base_units=16`、首单元 base、点名行号） | `05-gate-on-real-polluted-stream-red.txt` |
| 夹具无栈自检 | **GREEN / rc=0**（含 22c/22d） | `07-selfcheck-nostack.txt` |
| 四门禁 | fmt 0 / clippy 0 / **84 suites 682 passed 0 failed 16 ignored** / msgcodegen 604 | `08-gates.txt` |

## 3. F-N2 —— `start-csharp-hook.ps1` 传 `-Repo` 时空绑定（脚本回归）

`mir2-rs/tests/parity/m3/start-csharp-hook.ps1:54` 的 `. (Join-Path $Here 'm3hashutil.ps1')` 用了 `$Here`，
而 `$Here` 只在 `-Repo` 为空时赋值（F1 修复 `44849fa3` 引入）⇒ 传 `-Repo` 抛
`Cannot bind argument to parameter 'Path' because it is null`。**修法**：把
`$Here = Split-Path -Parent $MyInvocation.MyCommand.Path` 移出 `if`（无条件赋值）。
自检 22d：`-Repo <repo> -Status` rc=0（绿，`03-fn2-status-repo-green.txt`）；
把赋值移回 `if` 内 ⇒ rc=1 且原样复现该报错（红，`04-fn2-mutation-here-null-red.txt`）。

## 4. 边界 / 未做（如实登记）

- **并发攻方若也是 PlayObject**（两玩家同进程抢同一 `Random(attack_seed)`）：线程收窄不能区分
  （同一 `UserProcessor` 线程）；本片未覆盖。当前夹具只有 1 个 attacker（另一 bot 防御）。
  纯度门会对 base 单元点名，但「另一玩家」的单元仍是 player ⇒ 门不误报也不漏报 base，属**已知盲区**。
- **触发污染的具体怪物/地图未钉死**（m3cs 判为「别图怪/守卫」，本片沿用该判读；未取窗复现）。
- **真机两相复跑不在本片**：hook 改动的真机确认需重新起 C# 栈（本片未占窗口）。建议 m3cs 合并后
  取窗复跑隔离两相（受害 pin 仍 L255/20669）：此时 C# 专用流应**首单元即玩家**、`base=0`；
  若仍非 0，纯度门会当场判红并点名（fail-closed，不放宽判据）。
- 合并次序：本片只依赖已合入 master 的 `attack_sites.tsv` / `attack_stream_gate.py` 面；
  与 m2-svr 的 `4ba36368` / `7a41aebc`（Rust 侧 dump/管线）**无耦合**。

## 5. 文件索引

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 全部原始命令 |
| `01-hook-selftest-green.txt` | hook 离线自检 GREEN 55/55 |
| `02-hook-mutation-thread-filter-red.txt` | F-N1 改坏必红（5 FAIL / rc=1） |
| `03-fn2-status-repo-green.txt` | F-N2 绿（`-Repo -Status` rc=0） |
| `04-fn2-mutation-here-null-red.txt` | F-N2 改坏必红（null 绑定 / rc=1） |
| `05-gate-on-real-polluted-stream-red.txt` | 纯度门 × 真机污染流 ⇒ RED 点名 |
| `06-gate-selftest-green.txt` | 纯度门自检 GREEN 11/11 |
| `07-selfcheck-nostack.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` rc=0（含 22c/22d） |
| `08-gates.txt` | 四门禁原始输出 |
