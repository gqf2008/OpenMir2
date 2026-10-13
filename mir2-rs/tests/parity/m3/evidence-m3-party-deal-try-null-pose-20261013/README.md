# evidence-m3-party-deal-try-null-pose-20261013

卡 `openmir2-m3-party-deal-try-null-pose` —— M3⑦-3 `openmir2-m3-party-trade-state` 的 review
Minor **M-1** 收口（reviewer-6，approve 条目 `fbb382aa056ade5466c44dfd98475369d14d8d09`）。

> 作者 `openmir2-worker-21`（worktree `m3pt`）。本片**不 merge、不自签 review**。
> oracle/客户端/表结构/脚本/数据格式零改动（`20-boundary.txt`）。

## 1. 为什么（缺陷口径）

reviewer-6 登记：`deal_try` 的宿主解析把 oracle 的 `GetPoseCreate() == null` 与
`poseObject == this` 两种情形**都**归为 `DealTryTarget::NotPlayer` ⇒ 发 `SM_DEALTRY_FAIL`；
而 oracle：

- `GetPoseCreate() == null` ⇒ `PlayObject.Operate.cs:933` 的 `poseObject.Race` 对 null 解引用 ⇒
  NullReferenceException，被 `WorldServer.ProcessPlayObjectData` 的 `catch`
  （`src/GameSrv/Word/WorldServer.cs:732-735`）吞掉 ⇒ **一个帧都不发**；
- `poseObject == this` ⇒ `:936` 的 `if (targetPlayObject != this)` **无 `else`** ⇒ **什么都不发**。

reviewer 把两者都判为「正常玩法下不可达」。**该判断对 `null` 分支不成立**：

- `GetPoseCreate()` = `GetFrontPosition()`（前方格）+ `Envir.GetMovingObject(nX,nY,true)`
  （`src/M2Server/Actor/BaseObject.cs:1253-1261`、`src/M2Server/Maps/Envirnoment.cs:1203-1234`）；
  前排格为空 ⇒ 返回 `null`。
- `CM_DEALTRY` 的派发**不使用客户端传的名字**（`sHumName` 在 `ClientDealTry` 内未使用），
  只走 `GetPoseCreate()`（`src/M2Server/Player/PlayObject.Message.cs:945-947` + `Operate.cs:911/932`）。

⇒ 玩家打开对方菜单后对方走开（或任何空发的 `CM_DEALTRY`）都命中该分支。属 §1 硬约束③
「行为等价优先于修 bug」的**可达**差异：oracle 是缺陷，但必须照搬，本片照搬。

## 2. oracle 锚

逐段原文 + `grep -n` 见 `01-oracle-anchors.txt`。要点（行号实测）：

| 位置 | 语义 |
| --- | --- |
| `PlayObject.Operate.cs:932-933` | `IActor poseObject = GetPoseCreate(); if (poseObject.Race == ActorRace.Play)`（`null` ⇒ NRE） |
| `PlayObject.Operate.cs:936` | `if (targetPlayObject != this)` 内层无 `else`（自身 ⇒ 静默） |
| `PlayObject.Operate.cs:954/960` | `SM_DEALTRY_FAIL` 只在前排「非玩家」或「未回指/对方在交易中」时发 |
| `BaseObject.cs:1253-1261` | `GetPoseCreate()` → `GetFrontPosition` + `Envir.GetMovingObject`（无物 ⇒ `null`） |
| `Envirnoment.cs:1203-1234` | `GetMovingObject` 无物 ⇒ `result` 保持 `null` |
| `GameSrv/Word/WorldServer.cs:732-735` | `ProcessPlayObjectData` 的 `catch` 吞 NRE |
| `BaseObject.Base.cs:27-31` | `Run()` 逐条 `Operate`，本地无 `try/catch`（异常上抛到 `:732`） |

## 3. 实现

完整 diff 见 `02-change-summary.txt`。

- `mir2-rs/bins/game-svr/src/trade.rs`：`DealTryTarget` 增 `Absent` 变体；`deal_try` 的
  `Absent` 支**不落任何帧**。其余分支（`NotPlayer`/`Other{faces_me}`）与所有前置守卫逐字节不变。
- `mir2-rs/bins/game-svr/src/main.rs`：`CM_DEALTRY` 目标判定抽为纯函数
  `deal_try_target_of(world, displays, actor_id, pose_id)`：`None`/自身 ⇒ `Absent`；
  非玩家 ⇒ `NotPlayer`；玩家 ⇒ `Other{faces_me}`（对方 `GetPoseCreate()` 是否回指自己）。
  `handle_deal_cmd` 改为调用该函数。
- 前置守卫顺序不变：`DisableDeal`/`Dealing`/`TryDealTime`/`IsCanDeal` 在目标判定**之前**
  （`Operate.cs:913-926` 在 `:932` 之前）；顺序与 oracle 一致，零并行。

## 4. 判据 + 改坏必红

4 条新用例（`03-tests.txt`，全 `ok`）：

1. `trade::tests::deal_try_absent_pose_emits_nothing` —— `Absent` ⇒ 事件为空、不开对话、不改写
   `DealCreat`/`DealLastTick`。
2. `trade::tests::deal_try_absent_pose_still_obeys_guards_first` —— `Absent` 也先吃四条守卫
   （`DisableDeal`/`Dealing`/`TryDealTime`/`!IsCanDeal`）。
3. `tests::deal_try_target_distinguishes_absent_from_non_player` —— 空前排 ⇒ `Absent`；
   前排是怪 ⇒ `NotPlayer`；前排是玩家且回指/不回指 ⇒ `Other{faces_me:true/false}`。
4. `tests::deal_cm_try_no_front_object_emits_no_frame_on_production_path` —— 走**生产函数**
   `handle_deal_cmd` 的 `CM_DEALTRY` 支：空前排 ⇒ **零帧**；前排放怪 ⇒ `SM_DEALTRY_FAIL`
   （**非空转对照**）。

**改坏必红 2 条**（`redchecks/`；mutated 必红，`cp` 还原后 sha256 一致复绿）：

| 红检 | 改坏点 | 结果 |
| --- | --- | --- |
| RC1 | `deal_try_target_of` 的 `None` 支改回 `NotPlayer` | 用例 ③/④ FAILED，`left: (None, NotPlayer)` vs `right: (None, Absent)`，rc=101 |
| RC2 | `deal_try` 的 `Absent => {}` 改成 `em.now(def0(..., SM_DEALTRY_FAIL))` | 用例 ① FAILED，实得 `Def { target: 1, ident: 674, ... }`，rc=101 |

还原读数：`main.rs` sha256 `00c808a70e994f236877f198d8d1e8b164b8a663ea56d9687861269f99f1a9af`、
`trade.rs` sha256 `8259678d8d0f59e3f8b7e672602ab6efc5ab14c6a1f05ead216035bdd7f42bd7`（与改坏前一致）。

## 5. 未建模（登记，非差异）

oracle 的 NRE 还会**中断**该 `PlayObject.Run()` 本轮剩余的 `Operate` 以及
`ProcessPlayObjectData` 本轮剩余的玩家（异常上抛到 `WorldServer.cs:732`）。Rust 是「按命令派发」的
模型，没有「本轮 tick 中断」这一概念。两侧的**可观测契约**（帧流 / DB / 可观测世界态）在本分支
都是「不发帧」——本片只对齐该契约；tick 中断属结构性/时序差异，与 T-40（雇佣命令静默无效果）、
T-42（丢墙钟预算）同族口径。本片**未引入**新偏离（旧实现发 `SM_DEALTRY_FAIL` 才是偏离），
故不改 `tests/parity/whitelist.md`（属独立注册表车道）；此处如实登记，另在卡片线程 `comment` 留痕。

## 6. 四门禁 + 边界

`gates/`（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3pt-target`）：

| 门 | 读数 |
| --- | --- |
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 |
| `cargo test --workspace` | **98 suites / 1042 passed / 0 failed / 21 ignored**，rc=0（基线 master `029d23eae` = 98/1038/0/21 ⇒ 本片 **+4**） |
| `cargo run -p msgcodegen -- --check` | **604** 条一致，rc=0 |

边界（`20-boundary.txt`）：`git diff --name-only origin/master -- src sql MirClient` **空**；
改动面仅 `mir2-rs/bins/game-svr/src/{main.rs,trade.rs}`；`merge-tree origin/master HEAD` rc=0。

## 7. 件清单

`SHA256SUMS.txt`（仓库 LF/git-blob 口径）覆盖本目录除 `SHA256SUMS.txt` 自身外的全部件。
