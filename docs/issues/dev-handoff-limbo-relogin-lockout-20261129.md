# DEV-HANDOFF-LIMBO — 失败迁移回滚后的会话僵尸态：断线后重登被永久拒（新登记，待裁决）

> 登记日：2026-11-29。登记来源：DEV-HANDOFF-GROUP 路线②实施期间按总监令单独登记
> （「僵尸会话问题不在本笔范围……单独登记新卡报我」）。板外文件卡（非 parity 台账，
> 非 `whitelist.md` / `parity-gap-*.json`）。
> 本文件与 `docs/issues/dev-handoff-group-loss-20261126.md` 同根缺陷族（失败跨区迁移），
> 但缺陷面不同（会话生命周期 vs 组关系），分卡追踪。

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

## 待裁决（修复候选，需总监定）

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
