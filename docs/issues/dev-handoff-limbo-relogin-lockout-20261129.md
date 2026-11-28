# DEV-HANDOFF-LIMBO — 失败迁移回滚后的会话僵尸态：断线后重登被永久拒（**已修，路线①+②**）

> 登记日：2026-11-29。登记来源：DEV-HANDOFF-GROUP 路线②实施期间按总监令单独登记
> （「僵尸会话问题不在本笔范围……单独登记新卡报我」）。板外文件卡（非 parity 台账，
> 非 `whitelist.md` / `parity-gap-*.json`）。
> 本文件与 `docs/issues/dev-handoff-group-loss-20261126.md` 同根缺陷族（失败跨区迁移），
> 但缺陷面不同（会话生命周期 vs 组关系），分卡追踪。
>
> **收口状态（2026-11-29）**：总监确认生产级严重度并授权修复（路线①+②）；已落地送审，
> 见下方「修复」节；「现象/证据链/待裁决」为登记时原文（存档，「待裁决」已被授权裁定取代）。

## 修复（路线①+②，总监授权 2026-11-29；分支 `card/dev-handoff-limbo-fix`）

- **路线①（断线段）**：world-bevy `leave_world_inner` 新增 limbo 支——会话 `InWorld` 但实体
  缺席时，硬断线（`disconnect`）按移除门收口（`stage=Left` + `actor_id=None`，无广播无格摘）；
  `soft_close` 不走此支（ghost 语义需要实体，维持 `NotInWorld`）。oracle 无此态、纯自研
  机制无 parity 约束。
- **路线②（重登段，两半）**：宿主 `finish_login` 预检 `in_world` 与 world-bevy
  `enter_map_at` 的 `AlreadyInWorld` 守卫**同口径**带实体在场判据——「InWorld 但实体缺席」
  按「未在世界」放行重建（`enter_map_at` 的 F1 移除门收走死引用）。实体在场的真 InWorld
  仍按已登录处理（oracle 同形：关新+踢旧）。
- **路线③（`drain_kicks` 强清）：不需要**。理由：踢旧路径唯一 failing 步是 `world.disconnect`
  （limbo 时 Err），路线①已使其成功复位；`drain_kicks` 的 online 坐标前提在「能触发踢旧」
  的所有场景（online 项在场）天然满足——顶号踢旧走 `drain_kicks` → `disconnect`（路线①），
  stage 正常收口。
- **行为牙（锁死循环本体，修前红/修后绿）**：
  - `l2_handoff_tests::limbo_session_does_not_lock_out_relogin`：两区夹具失败迁移 → 回滚
    limbo → **真的区 k Closed 臂**（`handle_zone_closed`）→ 断言 stage 复位（路线①）→
    **真的进图口**（`enter_map_at`）重登放行（路线②守卫）+ `finish_login` 预检源码锚点。
  - `l2_handoff_tests::enter_map_allows_rebuild_over_limbo_session`：守卫半隔离牙（在线
    limbo 期间重复登录走的口）。
  - 修前红：`limbo_session_does_not_lock_out_relogin` rc=101（stage 停 InWorld，DBG 原文
    「关闭但未在世界」）。
  - 修后绿：2 passed / 0 failed。
  - 变异自证：M-L1 删路线① limbo 支 ⇒ rc=101（① 红 InWorld ≠ Left）；M-L2 预检删实体
    判据 ⇒ rc=101（源码锚红）；M-L3 守卫删实体判据 ⇒ rc=101（隔离牙红 AlreadyInWorld）。
- **顺带修复（诚实披露）**：全量门禁暴露 `entry_telemetry_wiring::ok_counter_sits_in_enter_map_ok_arm`
  红——系**已并的组修复**（18547534c）把 `apply_pending_group_restore_for` 插在 `enter_map_at`
  Ok 臂首位、中文注释 UTF-8 字节把 `record_ok` 推出该锚点牙的 800 字节窗；**master
  755363437 上即红**（上笔交付只复跑了定向牙、全量未跑，本笔认领）。修法：组恢复调用挪到
  `record_ok` 块之后（语义等价），锚点牙意图（`record_ok` 在 Ok 臂内、clock 基准）保持；
  修复后该牙绿。
- **门禁（本笔全量）**：fmt rc=0；`cargo test -p game-svr --lib` = **2664 passed / 0 failed**；
  `cargo test -p mir2-world-bevy --lib` = **945 passed / 0 failed**；clippy `-p game-svr`
  rc=0（修掉一处新牙的 `unnecessary_get_then_check`）；clippy `-p mir2-world-bevy` rc=0；
  msgcodegen rc=0（604 条）。

## 现象（登记时原文，存档）

## 现象（实施中观察到的玩家可见面）

失败跨区迁移回滚后，World 会话 `stage=InWorld`、`actor_id=Some(已释放实体的 id)`
（`restore_session` 经 `adopt_world_session` 写实），而实体在**两区都不存在**（源区 ack 时
已释放、目标区被 b1 回收）。该 limbo 态的两个玩家可见后果：

1. **在线冻结**：会话声称在世界、实体缺席 ⇒ 玩家命令无实体可落、AOI 无更新（在完成
   断线前玩家卡在无响应状态）；
2. **重登死锁（本卡核心）**：断线后重新登录**永远进不来**——
   - Closed 臂：宿主侧四容器照常清理，但 `world.disconnect` 因取不到实体坐标
     `Err(NotInWorld)`，**stage 不复位**（区 k 的处理臂对该 Err 只记 DBG 日志）；
   - 重登预检：`in_world=true`（stage 仍 InWorld）⇒ `handle_duplicate_enter(kick_old=true)`
     ⇒ **关掉新连接**；
   - 踢旧：`drain_kicks` 需要 online 坐标，而 online 项已在 Closed 臂摘除 ⇒
     「在线网关坐标未知，无法踢旧」，stage 依旧不复位；
   - 循环：每次登录尝试都撞同一 limbo ⇒ 该角色**永久登录被拒**（重启前无复位路径）。

## 证据链（2026-11-29 核查/实施中逐点确认；不贴行号，按符号定位）

- 回滚置 InWorld：`restore_session` → `adopt_world_session`（stage=InWorld、actor_id=Some(死 id)）；
- disconnect 对死实体 Err：world-bevy `session.rs` 的 `leave_world_inner`——取离开者坐标
  `.ok_or(SessionError::NotInWorld)`，实体缺席即 Err、stage 不动；
- 区 k Closed 臂对 Err 只记 DBG：`handle_zone_closed` 的 `Err(NotInWorld)` 臂；
- 重登预检关新+置踢旧：`finish_login` 的 `in_save || in_world` 分支 → `handle_duplicate_enter`；
- 踢旧失败：`drain_kicks` 首行 `online_take` 失败即 `continue`（stage 无第三处清理点）；
- `enter_map_at` 守卫：stage==InWorld ⇒ `AlreadyInWorld`（重登实际到不了这里——预检先分走；
  即便走到也会被拒）。

## 影响面与触发条件

触发 = 一次失败的跨区迁移（attach 被拒 / `router.assign` 失败 / 目标区无状态）+ 玩家断线。
触发后该角色永久无法登录。**limbo 态在 master 现状即存在**（b1 已并 d2ee5aa8a 后暴露），
不是路线②修复引入；路线②的组恢复依赖「重登重建」打通，limbo 修复是其自然互补。

## 待裁决（已被授权裁定取代；存档）

- 候选①：`disconnect` 对「会话在世界但实体缺席」的 limbo 支做 stage 复位（Left +
  actor_id=None；oracle 无此态、纯自研机制无 parity 约束）；
- 候选②：重登预检把「InWorld 但实体缺席」按未登录处理（放行 `enter_map_at` 重建；
  `enter_map_at` 自带 F1 收尸支，可顺带清死 id）；
- 候选③：`drain_kicks` 支持无 online 坐标时的 world 侧强制清理（按会话名直清 stage）。
- 三个候选都只动会话生命周期，不碰组数据；组关系恢复（DEV-HANDOFF-GROUP 路线②）在
  任一候选打通重登后即可生效。

## 关联

- `docs/issues/dev-handoff-group-loss-20261126.md`（同根缺陷族：失败迁移丢组关系，路线②已修）；
- `card/handoff-release-leak`（b1：attach 被拒释放目标区实体——limbo 的暴露点）；
- A35-b / Q1（会话搬迁与回滚语义；`adopt_world_session` 的 InWorld 写实点）。
