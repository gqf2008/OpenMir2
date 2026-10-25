# DROPGOLD 深体 RNG 取证 + ButchEffect RNG 合同（R 片）— 2026-10-25

- 卡：`openmir2-inbound-dispatch-red-batch`（**R 片**；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；
  分支 `card/inbound-dropgold-rng`）。
- 基线：本地 `master` `263e77d7e`（patch base；rebase 自 `f08496192`）。
- 写集：`mir2-rs/bins/game-svr/src/lib.rs`（仅 `#[cfg(test)]` 新增 4 用例 + 1 夹具）、本文件。
- 红线：未改 C# `src/**`、未改表/脚本/数据；未起栈、未占端口、未连库；不 merge、不自审。

## 0. 结论（一句话）

**`CM_DROPGOLD` 的深体没有 RNG**：oracle 的 `DropGoldDown` 因 `MapItem.ItemId` 恒 0 而**恒失败**，
`GetDropPosition` 是纯确定性格扫 ⇒ 该命令**零抽数**；此前「深体接线触 RNG」的判断是**基于函数名/位置
的推断**，本片以行号取证推翻它。真正占世界 RNG 流的是**同批 `CM_BUTCH` 的 ButchEffect**（两抽，
`Random(16)+5` → `Random(201)+100`），该实现已由 `openmir2-sm-skeleton-wiring` 接线；本片把两者都补上
**同 seed 逐位 R 合同**（零抽 / 恰两抽+流位）并各配牙。

## 1. 线缆形态（wire）

| 命令 | 12B 头字段来源 | 证据 |
|---|---|---|
| `CM_DROPGOLD(1016)` | `Recog` = **nGold**（`MakeMessage(CM_DROPGOLD, dropgold, 0, 0, 0)` ⇒ `nParam1 = Recog`） | `src/BotSrv/Player/RobotPlayer.cs:4896-4899`；`PlayObject.Message.cs:903-905`（`if (processMsg.nParam1 > 0)`） |
| `CM_BUTCH(1007)` | `Recog`=目标尸体 ActorId、`Param`=nX、`Tag`=nY、`Series`=(byte)btDir | `src/BotSrv/Player/RobotPlayer.cs:4766`；`PlayObject.Message.cs:829-831` |

## 2. `CM_DROPGOLD` 调用链与抽数普查

| # | oracle 行 | 内容 | 抽数 |
|---:|---|---|---:|
| 1 | `PlayObject.Message.cs:903-908` | `if (nParam1 > 0) ClientDropGold(nParam1)` | 0 |
| 2 | `PlayObject.Operate.cs:222-240` | 三道门：`InSafeDisableDrop && InSafeZone()` ⇒ `SM_MENU_OK`；`ControlDropItem && nGold < CanDropGold` ⇒ `SM_MENU_OK`；`!IsCanDrop || Envir.Flag.NoThrowItem` ⇒ `SM_MENU_OK` | 0 |
| 3 | `PlayObject.Operate.cs:241-243` | `if (nGold >= Gold) return;`（**丢全部被拒**，静默） | 0 |
| 4 | `PlayObject.Operate.cs:244-248` | `Gold -= nGold; if (!DropGoldDown(...)) Gold += nGold; GoldChanged();` | 0 |
| 5 | `BaseObject.cs:735-752` | `DropGoldDown`：`new MapItem { Name=金币, Count=nGold, Looks=GetGoldShape(nGold), … }`；`GetDropPosition(CurrX,CurrY,3,…)`；`AddItemToMap` 成功才 `SendRefMsg(RM_ITEMSHOW)` | 0 |
| 6 | `BaseObject.cs:470-527` | `GetDropPosition`：`for i<nRange` 菱形格扫 `GetItemEx` 找空位（纯确定性，**无 `Random`**） | 0 |
| 7 | `M2Server/Maps/Envirnoment.cs:88-96` | `AddItemToMap` **首行** `if (mapItem.ItemId == 0) return false;` | 0 |
| 8 | `SystemModule/Data/MapItem.cs:39-42` | `MapItem()` ctor 里 `// this.ItemId = M2Share.ActorMgr.GetNextIdentity();` **整行注释** ⇒ 结构体默认 `ItemId = 0` | 0 |
| 9 | `M2Share.cs:498-518` | `GetGoldShape`：阈值（30/70/300/1000…）→ 112..116，**无 RNG** | 0 |

⇒ **`DropGoldDown` 恒 `false`**（第 7 步首行短路）⇒ 不落图、不 `RM_ITEMSHOW`；金币「减了又加回」净不变，
但 `GoldChanged()`（`PlayObject.Message.cs:1833-1835` ⇒ `SM_GOLDCHANGED(653)`，`Recog=Gold`）**照发**。
本仓 `inbound::client_drop_gold` 的 BUG-REPLICA 2 口径（`inbound.rs:459-482`）在**帧面与状态面**与之一致。

**可达性补充（相邻项，已登记）**：`CM_DROPITEM`（丢**物品**）走 `DropItemDown`（`BaseObject.cs:529+`），
其中 `StdMode==45` 的 `GetRandomLook` = `Random(Shape)` **一抽**（`M2Share.cs:524-527`）；该抽数
**已由 P21 路径建模**（`crates/world/src/death_drop.rs:137-138`、`bins/game-svr/src/ground.rs:780+`），
不属本卡缺口。

## 3. `CM_BUTCH` / ButchEffect RNG

| oracle | 内容 | Rust |
|---|---|---|
| `PlayObject.Message.cs:829-831` | `ClientGetButchItem(nParam1, nParam2, nParam3, (byte)wParam, …)` | `inbound::client_butch` + 契约门 `butch_target_gates` |
| `PlayObject.Operate.cs:702-715` | 限速门（`!CloseSpeedHackCheck` 才生效；`TurnIntervalTime=600`，部署 `Server.conf:329`） | `close_speed_hack_check: true` **硬编码**（部署未设该键 ⇒ `GameSvrConf.cs:1920` 缺省 `true` ⇒ 门跳过；写入件改 FALSE 时的分叉登记在 §5） |
| `:716-720` | `|Δ|<=2` ∧ `IsValidObject(nX,nY,2,尸体)` ∧ `Death && !Skeleton && Animal` | `World::butch_effect_now` 门（`world.rs:7712+`） |
| `:722` | **第 1 抽** `n10 = (byte)(Random(16) + 5)` | `rng.random_below(16) + 5` |
| `:723` | **第 2 抽** `meatQuality = (ushort)(Random(201) + 100)` | `rng.random_below(201) + 100` |
| `:724-736` | `BodyLeathery -= n10`；`MeatQuality -= meatQuality`（≤0 清 0）；`BodyLeathery<=0` 且 race∈[50,80) ⇒ `Skeleton=true` + `RM_SKELETON` | 同（`ApplyMeatQuality`/`TakeBagItems` 未建模，T-206 登记） |
| `:738-745` | `BodyLeathery=50`；`DeathTick=GetTickCount()`（**屠宰者**） | 同 |
| `:747` / `:749` | `Dir = btDir`（仅 `|Δ|<=2` 时）；**无条件** `SendRefMsg(RM_BUTCH, Dir, CurrX, CurrY, 0, "")` | 同（`Command::Butch` ⇒ `SM_BUTCH(637)`） |

两抽与 DROPGOLD 同一条 `M2Share.RandomNumber` ⇒ 这就是本卡 R 判据的落点。

> **口径漂移备忘（卡 `openmir2-butch-637-criterion-fix`）**：本文 §? 的 `:749` 一行**早就写明** `SendRefMsg(RM_BUTCH, …)` 是**无条件**发的；后续文档复述时把 `SM_BUTCH(637)` 当成了「屠宰成功/落点」，是本轮真机实验（账本 `max=16` 0 条、`max=201` 全来自 `MonGen.cs:815`）才把它纠正回来——**已记录的原文没错，是复述漂移**。

## 4. 判据（实测）

| 用例（`bins/game-svr/src/lib.rs`，均从 `handle_inbound_cmd` 生产派发面进入） | 钉什么 |
|---|---|
| `cm_dropgold_deep_body_draws_zero_rng_and_emits_goldchanged` | **零抽数**（同 seed 克隆逐位比对 16 抽）、653 `Recog=净金币=1000`、会话金币不变、`param/tag/series` 不参与（三值互异） |
| `cm_dropgold_gates_draw_zero_rng` | `nGold>=Gold` ⇒ 零帧零抽；`ControlDropItem && 500<1000` ⇒ 一条 `SM_MENU_OK(767)`、零抽、金币不变 |
| `cm_butch_effect_two_draw_sequence_is_bit_exact` | 同 seed：`d1=Random(16)+5`、`d2=Random(201)+100` **逐位同值**；`BodyLeathery=255-d1`、`MeatQuality=2000-d2`；流位**恰停在第二抽之后**（再取 8 抽与克隆一致） |
| `cm_butch_gate_fail_draws_zero_rng` | `|Δ|=10>2` ⇒ 零抽、尸体状态不变，`RM_BUTCH` 仍发（`Operate.cs:749` 无条件） |

### 牙（逐条实跑：变异 → 目标用例红 → 还原绿）

| # | 变异 | 实际红 |
|---:|---|---|
| 1 | `CM_DROPGOLD` 派发臂插一次 `world.rng.random_below(2)` | `..._deep_body_draws_zero_rng...` 红（「必须零抽数」） |
| 2 | `client_drop_gold` 净金币改 `gold - n_gold`（真扣） | 同案红（`Recog=净金币`） |
| 3 | 删 `nGold >= Gold` 门 | `cm_dropgold_gates_draw_zero_rng` 红（第一段多一条 653）＋既有 `inbound::tests::cm_dropgold_gates` 红 |
| 4 | `butch_effect_now` 两抽对调 | `cm_butch_effect_two_draw_sequence_is_bit_exact` 红（「第 1 抽」） |
| 5 | `butch_effect_now` 第二抽改常量 | 同案红（「第 2 抽」；流位断言亦会红） |

## 5. 已知偏差 / R 级窗口项（`window_required: true`）

1. **零抽数属结论而非缺口**：若将来 oracle 侧把 `MapItem.ItemId` 的 ctor 注释恢复（上游修 bug），
   `DropGoldDown` 会开始成功并产出 `RM_ITEMSHOW`+地面金币；届时本仓必须重评（**行为以当前 oracle 为准**）。
2. **`CloseSpeedHackCheck` 硬编码 `true`**：部署 `Server.conf` 未设该键 ⇒ 默认 `true`（门跳过），当前等价；
   若部署件改成 FALSE，oracle 会走 `TurnTick` 节流（`TurnIntervalTime=600`，首次 `TurnTick=0` ⇒ `GetTickCount()`
   巨大 ⇒ 首轮必过），本仓需接 `TurnTick` 状态。属**时序面**，登记为窗口观察项。
3. 拒绝支的 `SM_MENU_OK` 尾文案（`CanotDropGoldMsg` 等）未建模（既有登记）；帧本身逐字节已对。
4. `ApplyMeatQuality`（改尸体 `ItemList` 中 `StdMode==40` 物品 `Dura`）与 `TakeBagItems` 未建模（T-206）。

**窗口项（真机，R 级）**：
- `CM_DROPGOLD`：客户端丢金币（≥`CanDropGold`、且 `< Gold`）⇒ 两侧 **653 一条**、`Recog` 相同、
  **地面无金币**、后续动作的 RNG 流水账本**逐位相同**（零抽）。
- `CM_BUTCH`：对动物尸体屠宰到骨架 ⇒ 两侧 `SM_SKELETON(33)`→`SM_BUTCH(637)` 帧序、尸体
  `BodyLeathery/MeatQuality/Skeleton` 落值、以及本次会话**后续抽数**（同 seed 流水账本）逐位相同。
- 命令模板：`python -I -B tools/gw-capture/gate_shadow_proxy.py --out E:/tmp/dropgold/sel --map 7101=127.0.0.1:7100 --gate sel --direction s2c`，
  跑完 `--verify`；同命令在 C# 栈重跑，按帧 sha256 与 RNG 账本对拍。

## 6. 门禁

fmt / clippy（`-D warnings`）/ `cargo test --workspace`（**3217 passed**，0 failed）/ `msgcodegen --check` 全 rc=0；
7 项卫生门禁全 rc=0；`script_host_noop_audit.py check`（含 `--strict-lines`）/ `selftest` rc=0；`git diff --check` rc=0；
`git merge-tree` 见 patch body（`tools/hygiene/merge_tree_helper.py`）。

## 7. 残余

- 本片**无生产代码改动**（取证结论：DROPGOLD 深体零抽数、ButchEffect 已接线）——新增的是 4 条 R 合同
  用例与 1 个夹具；如实登记，不虚构「深体实现」。
- 真机窗口（§5）未跑 ⇒ 合并后保持 `in-progress`。
- 无临时物；`E:/MirServer` 未改动。
