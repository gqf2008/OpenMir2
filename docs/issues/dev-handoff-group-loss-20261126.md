# DEV-HANDOFF-GROUP — 失败迁移是否静默丢掉组关系（**缺陷确认，路线②已修**）

> 登记日：2026-11-26。登记来源：`card/handoff-release-leak`（b1）的裁决 A。
> **本文件不是 parity 台账（`mir2-rs/tests/parity/whitelist.md`）也不是覆盖率机器清单
> （`docs/issues/parity-gap-*.json`）**，是这两者之外的单独缺陷记录；核实后如需要，再按
> 流程取 `T-` 号并入台账。
>
> **收口状态（2026-11-29）**：核查判「丢」（缺陷确认，总监采信）；修复路线由总监裁定为
> **②（会话表恢复）**并已落地送审。下方「判定」「修复」两节为收口记录；
> 「现象」「已知口径」「最小验证方法」「关联」为登记时原文（存档，其中「已知口径」的
> A38 前提已被 b1 推翻，以「判定」节为准）。

## 判定（2026-11-29 核查收官）

- 核查牙 `game-svr::l2_handoff_tests::failed_migration_drops_group_state_silently`
  （提交 `f1b5019a9`，双远端同 sha）：两区夹具，leader+member 组队 → leader 跨区 handoff →
  ack.ok 源区释放实体 → attach 被拒（目标区同 chr 冲突）→ b1 回收目标区实体 → 回滚源区。
  用例绿（1 passed）⇒ 按卡判据 `group(id)==None` **判「丢」**。
- 六点证据链（核查提交信息全文可查）：
  1. ack.ok ⇒ 源区先捕组再 `release_entity`——源区实体与组组件同灭；
  2. `attach_session` 把捕获的组写到目标区实体——唯一幸存副本；
  3. attach 拒 / router 失败 ⇒ b1 释放目标区实体——最后副本消失；
  4. `restore_session` 原口径「有意不回写、随包丢弃」且不重建实体 ⇒ 会话 limbo
     （stage=InWorld、actor_id 指死实体）；
  5. 实体唯一重建路径=再登录，组四字段全缺省（`EntitySpawn::new`；存档只恢复
     `allow_group_re_call`）⇒ 重建也不恢复；
  6. member 侧残影无人清理、失败路径无任何组帧 ⇒ 「静默」；`MirrorSpec` 无组字段
     （镜像疑点同证）。

## 修复（路线②：会话表恢复，总监裁定 2026-11-29）

- **语义**：`restore_session` 不再丢 `m.group`——连同暂存时刻的旧 actor id 写入 World 会话
  （`Session::pending_group_restore`，七表外、会话侧；会话对象从不删除 ⇒ 暂存跨重登存活）；
  登录重建实体（`finish_login` 的 `enter_map_at` 成功臂，区 0 与区 k 共用）由
  `HostLoop::apply_pending_group_restore_for` **恰一次**消费：组三字段写上新实体，并把
  **组长自引用重映射到新 id**（owner 与名单内的旧自 id → 新 id）。
- **为什么重映射必须有**：actor id 单调不复用是成文不变量 ⇒ 重建必拿新 id；不重映射则
  owner 判定（`party_group_owner_of != actor`）失败，重建后的组长无法管理本组（加人/踢人
  全被拒）。
- **已知残差（对裁定要求「member 侧残影随重建自愈」前提的显式修正，已单独报总监）**：
  member 侧 `group_owner` 旧指针不在恢复面——组长新 id 无从写进 member 实体（跨实体/跨区
  写超出路线②边界）。清理路径：组长（重建后）断线触发整组散（区 0 已接线的 Disappear
  语义）或重新组队；**member 自己断线不清**（`party_del_member` 需 owner 实体的名单可读，
  owner 指旧死 id 时整支 no-op）。
- **`allow_group_re_call` 不在恢复面**：oracle 口径来自存档记录（`apply_login_record_scalars`）。
- **行为牙**：`failed_migration_stashes_group_and_restores_on_rebuild`（核查牙反向改写）。
  - 修前红：`f1b5019a9` 上核查牙绿=判「丢」（六点链，rc=0 但语义为缺陷确认）；
  - 修后绿：`1 passed; 0 failed`（暂存断言 + 重建断言 + 消费恰一次 + 残差如实断言）；
  - 变异自证：M1 删暂存行 ⇒ 红 rc=101（`None ≠ Some(PendingGroupRestore{…})`）；
    M2 删自引用重映射 ⇒ 红 rc=101（owner 停旧 id 4194305 ≠ 新 id 4194307）。
- **提交链**：`f1b5019a9`（核查牙）→ `18547534c`（路线②修复 + 行为牙）→ 本提交（本卡 +
  limbo 卡）。门禁（定向）：fmt rc=0；clippy `-p game-svr` rc=0；clippy `-p mir2-world-bevy`
  rc=0；world-bevy session 用例 8 passed；msgcodegen rc=0（604 条）。工作区全量留 reviewer
  在钉住 commit 自跑。
- **边界（总监令）**：limbo 态（stage=InWorld 指死实体）**不在本笔范围**——实施中观察到其
  玩家可见面（断线后重登被永久拒），已按令单独登记
  `docs/issues/dev-handoff-limbo-relogin-lockout-20261129.md` 报总监。

## 现象（登记时原文，存档）

`card/handoff-release-leak` 让「attach 被拒 / `router.assign` 失败」时释放目标区那份**已提升为
真实体**的实体（无会话的真实体会永久扫描、刷 `missing_online`，见 b1 卡）。释放的结果是：

- 目标区实体（连同它的 `GroupState`）消失；
- 会话整包回滚到**源区**，但源区在 ack 时已经 `release_entity` 过该实体，
  回滚只恢复七表 + World 会话，**不重建实体**。

⇒ 需要核实：**源区后续重建实体时，组关系（`allow_group` / `group_owner` /
`group_members`）是否还在？**（2026-11-29 核查答案：不在——见「判定」节。）

## 已知口径（存档；A38 前提已被 b1 推翻）

- A38（2026-11-24）原口径：attach 被拒时组关系**留在目标区实体**上、恰一份。该口径以实体
  保留为前提；b1 裁决 A 推翻前提，组关系随实体消失——本卡即是那个「已知代价」的追踪项。
- 原口径不阻塞 b1：b1 的验收是「目标区不留无会话 PLAY 实体」，本卡是它的**副作用**。

## 最小验证方法（已照做；存档）

1. 两区夹具：A 区两个玩家 `leader`/`member` 组队；
2. 让 `leader` 触发一次必然被拒的 handoff（attach 冲突注入）；
3. 断言（逐项）源区 online 回滚、`world.entities.group(id)` 下落、member 观察面；
4. `group(id)` 为 `None` ⇒ 确认「失败迁移静默退组」（2026-11-29 即此结果）。

## 关联（存档）

- `card/handoff-release-leak`（b1 卡，释放目标区实体 + 计数 `handoff.target_entity_released`）；
- `card/mirror-aoi-observer-fixture`（`ec02d1bcc`，镜像会扫描、会产 hides 的证据）；
- A38 原始裁决（组关系随实体、`restore_session` 有意不回写组关系——前提已废，见「判定」节）；
- `docs/issues/dev-handoff-limbo-relogin-lockout-20261129.md`（同根缺陷族的 limbo 面，另卡追踪）。
