# 交付披露（opc-07 要求 #2 / #3）

## #2 门控缺陷（`0>=0` 误判升级）完整披露

- **本分支曾有一版头部 `816aa3064` 实际是红的**：`cargo test -p game-svr --lib` 有 5 例失败
  （eat_drug_dispatches_and_updates_session / eat_sun_water_shape1_shape3_survive_p21_and_save_hp70_mp140 /
  recovery_tick_consumes_pool_and_broadcasts / eat_dead_rejected_636_entity_stays_zero_and_p21_heals_projection /
  eat_stdmode31_unbind_dispatches_six_additem_frames）。该红由**全量 `--lib`** 抓到，**scoped 证据漏过**。
- **引入 / 被修提交与修法**：引入 `34823f8ce`（升级块 `let leveled = exp >= max_exp` 无条件计算）；
  修复 `9af4f3bd4`（改为 `let leveled = out.exp_delta.is_some() && exp >= max_exp`，对齐 oracle —— `GetExp`
  只对授经验物品调用）。红→绿原文见 `gate-fix.txt`。
- **为什么 scoped 没抓到（决定 scoped 清单是否要扩）**：scoped 跑的是 `tests::it08_levelup_host_parity`
  与 `useitem::tests`（Shape13 / 升级路径，`exp_delta=Some`，`leveled` 恒被正确置真）。门控 bug 只在
  **`exp_delta=None` 的非授经验物品**（药品 Shape 0/1/3、解包 StdMode31、死亡拒绝）上以 `0>=0` 触发，
  这些用例不在 scoped 集合内。**结论：凡改"共享判定逻辑"（如 `leveled`）必须跑全量 `--lib` 或显式纳入
  受影响的非目标分支用例；不能只跑目标分支的 scoped 用例。**

> **章程 §6.2 对应（opc-07 2026-11-20 增）**：scoped 须覆盖"被改逻辑**取假值**的反向分支"——改共享
> 判定逻辑（如 `leveled`）时，要么显式纳入条件为假时的受影响用例、要么跑全量；交付须回答"该条件在
> 哪些调用方取假值、由哪条用例覆盖"。本条即：`leveled` 在 `exp_delta=None` 的药品(Shape0/1/3)/解包(StdMode31)/
> 死亡拒绝调用方**取假值**，由 `eat_drug_*`/`eat_sun_water_*`/`eat_stdmode31_unbind_*`/`eat_dead_rejected_*` 覆盖
> （scoped 未纳入 ⇒ 漏；全量 `--lib` 抓）。

### `--lib` 三元组（`-p game-svr --lib`）
`passed 1992 / failed 0 / ignored 25`；**总数 2017（=1992+25+0）**。
> ignored=25 是 **`-p game-svr --lib` 这一个测试二进制**的数。若在别处见过 49/52，那是**别的 package/测试二进制**
> （如 world-bevy 等）的 ignored 计数，不是本条；本条只认 `-p game-svr --lib` = 25。

## #3 `SM_WEIGHTCHANGED(622)` 是"每次成功吃都发的通用帧"，非本次为绿而改金标准

**oracle 证据（符号锚 + golden）**：
- 发射点（吃物品通用尾）：`BaseObject.WeightChanged()` = `WAbil.Weight = RecalcBagWeight();
  SendUpdateMsg(RM_WEIGHTCHANGED,…)`；被 `PlayObject.Base.cs` 的吃物品成功路径调用（多处 `WeightChanged();`）。
- 泵出：`PlayObject.Message.cs` case `RM_WEIGHTCHANGED` ⇒ `SendDefMessage(SM_WEIGHTCHANGED, WAbil.Weight,
  WearWeight, HandWeight, checksum)`。
- **golden（非升级的普通吃也入队 10115）**：`eat_ok_shape1_fixed` notices=[10052,10115]、
  `eat_ok_shape3_percent` notices=[10052,10115]、`eat_ok_stdmode31_unbind_success` notices=[10115]。
  ⇒ oracle 在**任何成功吃**上都 `SendMsg(RM_WEIGHTCHANGED)` ⇒ 泵出 622。622 **不是**升级支独有。
- **一次真实吃的 `immediate ++ pumped` 原文**（升级 case，探针同通道捕获）：
  `[(635,0,0,0,0),(53,7777,52,32,52),(622,0,0,0,0,36683),(44,90,100,0,0),(45,90,6,0,0),(52,0,25344,0,0),(752,1,3845,0,0)]`
  —— 622 按优先级 131 落在 53(68) 与 44(139) 之间，与 oracle 优先级一致。

**定性**：`622 = 通用吃物品帧`（每次成功吃都发）。本次只是在**夹具/pump 侧**把此前漏采的 622 纳入 golden，
属证据面增强，**不是**改金标准以让测试变绿（改前/改后测试都验证 Rust 与 oracle 同一帧序，622 来自 oracle 实跑）。
