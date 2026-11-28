# LOGIN-ALLOWGROUP-LOAD-GAP — 登录不恢复 `AllowGroup` 开关（oracle 恢复；**已修，路线照卡内候选**）

> 登记日：2026-11-29。登记来源：DEV-HANDOFF-GROUP 残差机检（总监令第 2 条「member 重登组关系
> 是否从 DB 恢复」）的衍生发现。板外文件卡（非 parity 台账；合并后按流程取 `T-` 号）。
>
> **收口状态（2026-11-29）**：总监核三锚后**授权修复**（「真 parity 缺口，影响所有重登玩家」）。
> 已照卡内候选落地：`LoginScalarTarget` 增 `set_login_allow_group`（oracle `:1146` 同形），
> 见下方「修复」节；「现象/机检证据/修复候选」为登记时原文（存档）。

## 修复（总监授权 2026-11-29；分支 `card/login-allowgroup-load-gap`，已并 master `420930ca0`）

- **落点**：`LoginScalarTarget` trait 增 `set_login_allow_group`；`EntitySpawn`/`EntityMut`
  两 impl 各加一字段装载（`allow_group` / `group.allow_group`）；
  `apply_login_record_scalars` 尾加
  `ent.set_login_allow_group(data.is_some_and(|d| d.allow_group > 0))`
  （oracle `WorldServer.cs:1146` 同形：`> 0` 语义、无记录不置位）。
  trait 文档补 `AllowGroup` 条目（`:1146` 出处、存侧 T-272 交叉引用）。
- **行为牙（修前钉缺口 → 修复翻绿）**：
  - 修前：`login_does_not_load_allow_group_switch_from_record` 绿（缺口确认，1 passed）；
  - 修后：改名 `login_loads_allow_group_switch_from_record` 翻断言——`allow_group=1` ⇒
    实体 true（1 passed）；反面 `allow_group=0` ⇒ false（`>0` 语义）；无记录 ⇒ false
    （ctor 缺省）；组本体不恢复（oracle 离线即退组一致）保持断言。
- **端到端牙（总监节奏提示）**：`relogin_restores_allow_group_and_reinvite_succeeds`——
  全链生产件：成员在场（旧实体开关 false）→ 断线收口 → 登录装载（录 `allow_group=1`）→
  进图 ⇒ **组长重邀成功面**：`party_create_group` 得 `CreateGroupOk`、无 `CreateGroupFail`
  （**修前此面是 -4**）；双方 owner 指组长（组面自洽）。
- **变异自证（M-AG）**：删 `set_login_allow_group` 装载行 ⇒ 双牙齐红
  （`login_loads_allow_group_switch_from_record` + `relogin_restores_allow_group_and_reinvite_succeeds`
  均 panic，rc=101，6 passed / 2 failed）。
- **门禁（全量，按本日教训）**：fmt rc=0；`cargo test -p game-svr --lib` 全绿；
  clippy `-p game-svr --all-targets -D warnings` rc=0；msgcodegen rc=0（604 条）；散文锚 GREEN。
  （world-bevy 本笔零改动——与 master 相同，master 侧已全量验证。）

## 现象（登记时原文，存档）

- **oracle**：登录恢复 `AllowGroup` 开关——`src/GameSrv/Word/WorldServer.cs:1146`
  `playObject.AllowGroup = humData.AllowGroup > 0;`（存侧 `:1294` 回写；Rust 存侧已于
  T-272/卡 `openmir2-save-allowgroup-writeback` 修复，merge `a46d3e3c3`）。
- **Rust**：登录装载器 `apply_login_record_scalars`（bins/game-svr）只装
  `Dir`/`AttatckMode`（WorldServer.cs:1148）/`AllowGroupReCall`（`:1159`）三项；
  `record::CharacterData.allow_group`（storage/playerdata.rs 有该字段、存档正常带回）
  **无载入点** ⇒ 重登后 `allow_group=false`。

## 玩家可见后果

重登后的角色「允许组队」恒关：组长对其发起组队（`CM_CREATEGROUP`/`CM_ADDGROUPMEMBER`）
被 `-4` 拒（`party_create_group`/`party_add_group_member` 的
`entities.get(target).allow_group` 门），该角色必须**手动重发 `CM_GROUPMODE`** 才能被邀。
oracle 里重登即可被邀。影响所有重登玩家（不限 handoff 场景）。

## 机检证据（2026-11-29）

- 结构面：`record::CharacterData` **无** `group_owner`/`group_members` 字段
  （storage/playerdata.rs）⇒ 组关系本体结构性不在 DB，任何路径都不可能从存档恢复成员关系；
  member 重登无组与 oracle 一致（**离线即退组**，`PlayObject.Message.cs:2138-2142`
  `MakeGhost` 内 `groupOwnerPlay.DelMember(this)`；重邀即回）——**这半边不是缺陷**。
- 载侧：`apply_login_record_scalars` 无 `allow_group` 装载（bins/game-svr，源码可 grep）；
  全仓该字段的登录写点不存在。
- 行为牙：`game-svr::…::login_does_not_load_allow_group_switch_from_record`
  （分支 `card/login-allowgroup-load-gap`）——记录 `allow_group=1` 登录后实体仍 false；
  对照面 `allow_group_re_call=true` 证明装载器本身在跑（排除夹具错）。

## 与 DEV-HANDOFF-GROUP 残差定性的关系

总监问题「member 重登组关系是否从 DB 恢复」的完整答案：
1. **组关系本体**：不恢复，且 oracle 同语义（离线即退组）⇒ 残差按「oracle 一致」定性，
   DEV-HANDOFF-GROUP 卡可收（残差有界：组长侧名单残条在整组散/重新组队时清理）。
2. **`allow_group` 开关**：oracle 恢复、Rust 不恢复 ⇒ 本卡。它把「重邀即回」变成
   「重邀被 -4 挡、需成员手动配合」——若不修，组关系的实际恢复路径（重邀）被削弱。

## 修复候选（待裁决，未落码）

`apply_login_record_scalars`（`LoginScalarTarget` trait）增加
`set_login_allow_group(data.is_some_and(|d| d.allow_group > 0))`（oracle `:1146` 同形），
`EntitySpawn`/`EntityMut` 两 impl 各加一字段装载；行为牙第二段翻为绿。
注意：改动影响所有登录路径的 `allow_group` 初值（oracle 同源，非 handoff 专属），按登录
parity 面送审。

## 关联

- `docs/issues/dev-handoff-group-loss-20261126.md`（残差定性来源；本卡是其机检第 2 问的答案）；
- T-272（whitelist）：存侧 `AllowGroup` 回写已修（a46d3e3c3）——本卡是其载侧的另一半；
- `docs/issues/dev-handoff-limbo-relogin-lockout-20261129.md`（同族 limbo 面，另卡）。
