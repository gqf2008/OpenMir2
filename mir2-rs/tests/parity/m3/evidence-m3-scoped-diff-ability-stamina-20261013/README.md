# 证据包 — M3②③ follow-up：`characters_ablity.Hp` 唯一残留的**离线根因修**（体力/魔力点段）

- 卡：`openmir2-m3-scoped-diff-ability`（issue `05f9079a`，owner 2026-10-13 03:50 定位本修）
- 车道：`m2-svr`（principal `openmir2-worker-11`）／分支 `worktree-m2-stamina-healthtick`，base `master fa82187a`
- 上游：
  - F-2 归因片（已合入 master `fa82187a`）：`mir2-rs/tests/parity/m3/evidence-m3-scoped-diff-ability-20261013/**`
  - 真机两相（m3cs，master `55a29144`）：`mir2-rs/tests/parity/m3/evidence-m3-isolate-twophase-dod2-20261013/**`
- 写集：`mir2-rs/bins/game-svr/**`（`stamina.rs` 新增 + `main.rs`/`config.rs`/`lib.rs`/`attackwire.rs`/`movewire.rs` 接线与注释）+ 本目录
- **零 `src/**`/`sql/**`/`MirClient` 改动、零夹具改动、未起栈、未占端口**（真机两相复跑仍要窗口，见 §6）

## 1. 结论（TL;DR）

F-2 归因片把 7 处差异收敛到 **1 列真差**：`mir2_db.characters_ablity.Hp` 的**攻方**行
（C# `20668` / Rust `20669`），发生在 **run 内第 86 击**（服务端权威逐击 dump），不是存档路径。
**根因**（本片给出并按 oracle 逐行修）：

```
BaseObject.Run()（src/M2Server/Actor/BaseObject.Base.cs:25-110）的体力/魔力点段：
  每 50ms（UserProcessor : TimerScheduledService(50ms)）一次
  HealthTick/SpellTick += (GetTickCount() - AutoRecoveryTick) / 20   // :39-42  → 40/秒
  攻击成功 HealthTick -= 30（PlayObject.Attack.cs:955）              // 攻方每击净 -30
  HealthTick < -HealthFillTime(300) ⇒ HP -= 1（:105-109）           // 累计越界 ⇒ 掉 1
```

Rust 此前**只登记未移植**（`attackwire`/`movewire` 模块头都写着「`HealthTick`/`SpellTick` 未移植」）
⇒ 攻方体力点只减不回收、永远到不了 `-300` ⇒ HP 恒 `20669`。本片按 oracle 实现并在生产路径接线：

1. **新模块** `bins/game-svr/src/stamina.rs`：`Stamina`（`HealthTick`/`SpellTick`/`AutoRecoveryTick`）+
   按 **50ms 边界逐步重放** `Run()`（令整数除法截断与 oracle 一致：每 50ms `+2` = **40/秒**，
   不是 world tick 200ms 的 `(200/20)=10` = 50/秒）。
2. **消耗点接线**（`main.rs`）：攻击成功 `HealthTick -= 30; SpellTick -= 100; max(0,·)`（`:955-957`）；
   走路 `HealthTick -= 10`（`:1424`）；跑步/骑跑 `-= 60; SpellTick -= 10; max(0,·); PerHealth/PerSpell -= 1`
   （`:1352-1356`/`:1041-1045`）；受击 `HealthTick = 0; SpellTick = 0; Per*-=1`（`PlayObject.Message.cs:1587-1590`）。
3. **帧口径**（`main.rs::drain_stamina`）：`HealthSpellChanged()` 每调一次发一条
   `SM_HEALTHSPELLCHANGED`（`BaseObject.cs:700-710` ⇒ `SendUpdateMsg(RM_…)` ⇒
   `PlayObject.Message.cs:1736-1739` 用**当时** `WAbil` 拼包）。一次 `Run()` 可命中两次
   （HP 支 `:55` + MP 支 `:68`）⇒ **两条帧、内容不同**（HP 支那条是「MP 未加」的中间态）
   ⇒ `stamina_tick` 用 `changes: Vec<(HP,MP)>` 逐次记快照、调用方逐条发（**不合并、不用终态**）。
4. **配置**：`[Setup] HealthFillTime`/`SpellFillTime`（`ServerConf.cs:152-153`，缺省
   `300`/`800`，`GameSvrConf.cs:1323-1324`）；现网 `Server.conf:785-786` 实值 `300`/`800`。

**结果**：攻方 HP 在第 86 击附近 20669 → 20668，**与 C# 逐击轨迹逐项一致**（终值 + 单次掉血）。

## 2. 构造级判据（全部 GREEN）

| 用例 | 钉住什么 |
|---|---|
| `stamina::tests::attacker_loses_one_hp_when_health_tick_drops_below_fill_time` | **126 击内恰掉 1 点、终值 20668**（与真机 `17-*.txt` 同结论） |
| `stamina::tests::removing_the_negative_tick_branch_must_break_the_regression` | `:105-109` 的 `HP -= 1` 行为 + 帧快照 |
| `stamina::tests::health_tick_recovers_two_per_fifty_ms` | 回收率 = **40/秒**（50ms × +2） |
| `stamina::tests::recovery_is_fifty_ms_based_not_world_tick_based` | 排除 world tick 200ms 的 50/秒口径 |
| `stamina::tests::both_branches_in_one_run_produce_two_distinct_change_snapshots` | **帧口径**：两支同命中 ⇒ 两条帧、首条为中间态 |
| `stamina::tests::death_freezes_auto_recovery_tick` | `!Death` 外不更新 `AutoRecoveryTick`（`:39-40`） |
| `stamina::tests::struck_resets_stamina` | 受击清零（`Message.cs:1587-1588`） |
| `config::tests::setup_stamina_fill_times_and_defaults` | 配置解析/缺省 `300`/`800`/非法报错 |
| `tests::drain_stamina_drops_one_hp_writes_back_and_emits_frame` | 帧头 `recog=ActorId,param=HP,tag=MP,series=MaxHP` + **回写 `Entity.hp`** |
| `tests::move_run_consumes_stamina_and_per_pool` | 走路/跑步/骑跑三档消耗点（含 `SpellTick` 0 下限、`Per*` 扣减） |

## 3. 改坏必红（原始输出见 `3*-redcheck-*.txt`，改后逐次已还原并复跑 GREEN）

| 文件 | 变异 | 期望 | 实得 |
|---|---|---|---|
| `30-redcheck-negative-branch-removed.txt` | 负值扣血支条件改 `if false && …` | 2 红 | `attacker_loses…` + `removing_the_negative_tick_branch…` **FAILED**，`20669 != 20668` / `drops 0 != 1` |
| `31-redcheck-entity-writeback-removed.txt` | 删掉 `world.entities.get_mut(…).hp = …` | 1 红 | `drain_stamina…` **FAILED**，`Entity.hp 100 != 20668` |
| `32-redcheck-twoblockscollapsed-frame.txt` | 吞掉 HP 支的 `changes.push`（两帧合一） | 1 红 | `both_branches…` **FAILED**，`1 != 2` |
| `33-redcheck-run-perpool-removed.txt` | 删掉跑步的 `PerHealth/PerSpell -= 1` | 1 红 | `move_run_consumes…` **FAILED**，`5 != 4` |

还原核对：`diff -q` 对 `/e/tmp/m2svr-stamina-redcheck/*.bak` ⇒ **byte-identical**；复跑三组新用例全 GREEN。

## 4. 四门禁（原始件 `gates/**`，隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`）

- `gates/10-fmt.txt`：`cargo fmt --all -- --check` **rc=0**
- `gates/11-clippy.txt`：`cargo clippy --workspace --all-targets --offline -- -D warnings` **rc=0**
- `gates/12-msgcodegen.txt`：`cargo run -p msgcodegen --offline -- --check` **rc=0**（604）
- `gates/13-test-workspace.txt`：`cargo test --workspace --offline` **rc=0**（84 suites / **698 passed / 0 failed / 16 ignored**）
- `20-game-svr-tests.txt`：`cargo test -p game-svr --offline`（lib 88 passed / bin 157 passed / relay 13 / seed_inject 3）

## 5. oracle 行号（逐行，原始件 `02/03/04-*.txt`）

| 点 | 位置 |
|---|---|
| `Run()` 体力/魔力点段 | `src/M2Server/Actor/BaseObject.Base.cs:25-110`（`:37` `!Death`、`:39-40` 计时、`:44-56` HP 支、`:57-69` MP 支、`:74-81` 清零、`:105-109` 负值扣血） |
| `HealthSpellChanged()` | `src/M2Server/Actor/BaseObject.cs:700-710` |
| 字段/初值 | `BaseObject.cs:287-288`（`HealthTick`/`SpellTick`）、`:306`（`AutoRecoveryTick`）、`:445-449`（初值 0） |
| 攻击成功消耗 | `src/M2Server/Player/PlayObject.Attack.cs:955-959`（`HealthTick -= 30; SpellTick -= 100; max(0,·); PerHealth/PerSpell -= 2`） |
| 走/跑/骑跑消耗 | `Attack.cs:1424`（走）、`:1352-1356`（跑）、`:1041-1045`（骑跑） |
| 攻击成功门 | `Attack.cs:884` 的 `nX == CurrX && nY == CurrY`（Rust `attackwire::at_self`） |
| 受击清零 | `src/M2Server/Player/PlayObject.Message.cs:1587-1590` |
| 帧拼装 | `PlayObject.Message.cs:1736-1739`（`MakeMessage(SM_HEALTHSPELLCHANGED, ActorId, HP, MP, MaxHP)`） |
| 配置 | `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:170/174`（字段）、`:1323-1324`（缺省）、`src/Modules/SystemModule/Conf/ServerConf.cs:152-153`、现网 `E:/MirServer/M2GameSvr/Server.conf:785-786` |
| 节奏 | `src/GameSrv/Word/Threads/UserProcessor.cs:7`（`TimerScheduledService(50ms)`）；`WorldServer.cs:697-703`（`Run()` 之后周期存档） |

## 6. 残留 / 待窗口（**不预登记 §6.0-3，判据不放宽**）

1. **真机两相复跑仍要窗口**：本片是**离线根因修**（owner 03:50 定位、DoD③ 走「修到零」支）。
   「那 1 HP 差消失」的最终确认 = 在 owner 发的**两侧栈窗口**里重跑 `run_attack_pair` 两相 ⇒
   `scoped_diff --canonical --fixed-newchr` **= 0**。窗口按 owner 排期在 ④ 的 B/C/D 之后。
2. **660ms 有效节奏**：夹具 `-AttackStepMs` 写 500ms，但假人主循环 = 每轮 `RobotPlayer.Run()`
   （客户端场景处理）+ `Thread.Sleep(50)` ⇒ 实际两击间隔对齐到轮边界（c2 实测约 660ms）。
   真机 dump（`17-*.txt`）把「单次掉血、掉在第 86 击」钉住，反解 `Δ ≈ 662ms`。本判据只依赖
   **「掉一次、终值 20668」** 两个与节奏无关的结论 + 节奏区间；窗口复跑会把它坐实。
3. **同 tick 阶段顺序**（登记，见 `drain_stamina` 文档）：oracle 一次 `ProcessPlayObjectData` 为
   「恢复段 → 消息段（攻击/移动扣点）→ 体力点段」；本宿主玩家指令走 gate→sim 异步通道 ⇒
   本段相对同 tick 已处理指令最多差一个 world tick（200ms）⇒ 掉血跨过点可能与 oracle 差 1 击，
   **终值一致**（本卡判据 = 存档末态）。
4. **`SuperMan`（GM 无敌）分支**（`:31-36`）未建模（本宿主无 GM 态）；`if (HP==0) Die()`（`:70-73`）
   由 `crates/world` 的攻击结算负责，本段不重复结算；目标清理（`:111-131`）属 world 侧另卡。

## 7. 残留物清单

- `E:/tmp/m2svr-stamina-redcheck/*.bak`（4 次变异前的备份，用于还原核对；**未入库**）。
- `E:/tmp/openmir2-m2svr-target`（隔离 target；**未入库**）。
- 未起栈、未占端口：`5000/5100/5500/5600/5700/6000/7000/7100/7200` 本轮**未使用**；
  未连接 MySQL（本片零 DB 读取）。
- 本目录外**只改** `mir2-rs/bins/game-svr/**`（5 改 + 1 新增）。
