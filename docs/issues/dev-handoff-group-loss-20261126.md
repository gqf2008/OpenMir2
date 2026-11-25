# DEV-HANDOFF-GROUP — 失败迁移是否静默丢掉组关系（待核实）

> 登记日：2026-11-26。登记来源：`card/handoff-release-leak`（b1）的裁决 A。
> **本文件不是 parity 台账（`mir2-rs/tests/parity/whitelist.md`）也不是覆盖率机器清单
> （`docs/issues/parity-gap-*.json`）**，是这两者之外的单独缺陷记录；核实后如需要，再按
> 流程取 `T-` 号并入台账。

## 现象

`card/handoff-release-leak` 让「attach 被拒 / `router.assign` 失败」时释放目标区那份**已提升为
真实体**的实体（无会话的真实体会永久扫描、刷 `missing_online`，见 b1 卡）。释放的结果是：

- 目标区实体（连同它的 `GroupState`）消失；
- 会话整包回滚到**源区**，但源区在 ack 时已经 `release_entity` 过该实体（`handoff.rs:207`），
  回滚只恢复七表 + World 会话（`stage=InWorld`、`actor_id=Some(id)`），**不重建实体**。

⇒ 需要核实：**源区后续重建实体时，组关系（`allow_group` / `group_owner` /
`group_members`）是否还在？** 若源区那份实体是镜像（`Flags.mirror`）且镜像不携带组关系，
则失败迁移会表现为**静默退组**——玩家可感知（队友列表里消失、队长离线散组规则提前触发），
而不只是"一个内部实体被回收"。

## 已知口径

- A38（2026-11-24）原口径：attach 被拒时组关系**留在目标区实体**上、恰一份（"实体是唯一活着的
  实体，回退=抹掉=丢数据"）。该口径以实体保留为前提；b1 裁决 A 推翻前提，组关系随实体消失，
  本记录即是那个"已知代价"的追踪项。
- 本条**不阻塞** b1：b1 的验收是"目标区不留无会话 PLAY 实体"，本记录是它的**副作用**。

## 最小验证方法（两区夹具）

1. 两区夹具（`region_loop` 或 `two_zone_state`）：A 区两个玩家 `leader`/`member` 组队
   （`GroupState.allow_group=true`、`group_owner=leader`、`group_members` 含两人）；
2. 让 `leader` 触发一次**必然被拒**的 handoff：
   - 目标区已存在同 `chr` 会话（attach 冲突），或
   - 目标区 `router.assign` 失败（源区在线项 `socket=0`）；
3. 断言（逐项）：
   - 源区 `online.find_by_chr("leader")` 仍存在（回滚成功）；
   - 源区**重建/保留**的实体上 `world.entities.group(id)` 是否仍等于迁移前的 `GroupState`；
   - `member` 观察到的组关系帧（组队列表）是否变化——若退组，是否有任何帧提示。
4. 若第 3 步为 `None` ⇒ 确认"失败迁移静默退组"，按缺陷处理（修法候选：回滚时把
   `GroupState` 随 `PendingSessionMigration.group` 写回源区重建的实体，或在源区实体缺席期间
   把组关系暂存在会话上）。

## 关联

- `card/handoff-release-leak`（b1 卡，释放目标区实体 + 计数 `handoff.target_entity_released`）；
- `card/mirror-aoi-observer-fixture`（`ec02d1bcc`，镜像会扫描、会产 hides 的证据）；
- A38 原始裁决（组关系随实体、`restore_session` 有意不回写组关系）。
