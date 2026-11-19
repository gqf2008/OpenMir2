# openmir2-hidden-npc-world-entities（NP-10 缺口卡；2026-11-20 实现并 verified）

> 卡：`openmir2-hidden-npc-world-entities`（NPC 线 NP-10 的缺口卡；tier=L）。
> 来源：NP-10（隐藏脚本 NPC 可点/可选）对拍时发现的**真差异**；按 opc-07 2026-11-20 裁定：
> 不用 `NpcHost` 接缝冒充语义，产品方向（隐藏 NPC 落成世界实体）**单开卡**，不在 NP-10 里顺手做。

## 1. oracle 事实

- `LocalDB.QMangeNpc()` / `QFunctionNpc()` / `RobotNpc()` 把 `MapQuest_def/QManage.txt`、
  `Market_Def/QFunction-0.txt`、`Robot_def/RobotManage.txt` 建为**隐藏** `Merchant`（`IsHide = true`）
  并 `AddQuestNpc` / `AddMerchant` 入册（`src/GameSrv/DB/LocalDB.cs:192/236/293`）。
- 入册的 NPC 有**真实 `ActorId`**，在 `QuestNpcList` / `MerchantList` 里；客户端/机器人用该 id
  发 `CM_CLICKNPC` / `CM_MERCHANTDLGSELECT` 可命中并执行其脚本（`NormNpc.Click` / `UserSelect`）。
- `IsHide` 只影响**视野门**（如 `ClientMerchantDlgSelect` 的 `npc.IsHide` 放行），不等于"无实体"。

## 2. Rust 现状（差异）

- 隐藏 NPC 只是 `NpcHost` 的**脚本宿主**（`bins/game-svr/src/npc.rs` 的 `NpcHost::goto_label`，
  `ActorId = 0`）；
- `handle_click_npc_cmd` / `handle_shop_cmd` 都按**世界实体 id** 解析（`merchant_lookup(world.entities.get(id))`）
  ⇒ 用 oracle 的隐藏 NPC actor id 点击/选择，在 Rust 落零帧；
- 影响面：隐藏商人 / 任务 NPC（QManage、QFunction、RobotManage）**不可交互**（脚本只能经宿主接缝触发，
  如 `@Login` / `@StdModeFunc*` / `QFunction` 固定标签）。

## 3. 设计要点（实现卡按此展开）

1. 装载链把三只隐藏 NPC 落成**世界实体**（`insert_merchant` 或 `insert_npc`，按 oracle 的列表归属：
   QManage/RobotManage → `QuestNpcList`；QFunction → `MerchantList`），并分配 `ActorId`；
2. `ActorId` 分配与 `NpcHost` 的脚本表索引对齐（脚本按 ChrName 装载，实体按 id 解析）；
3. 与视野门的关系（**实现时按 oracle 源码订正**）：`IsHide` **不参与**视野过滤
   （`BaseObject.ViewRange.cs` 的过滤器只读 `Death/Invisible/Ghost/FixedHideMode/ObMode`）。
   隐藏 NPC 不进他人 AOI 的原因是**挂图失败**：部署件 `E:/MirServer/M2GameSvr/Map/0.map` 的
   `(0,0)` 是 `HighWall`（实读：`wBkImg = 0x873C`，bit15 置位）⇒ `CanWalk` 假 ⇒
   `AddToMap` 不执行 ⇒ 不进任何格 `obj_list`。`IsHide` 的真实职责只有两条：
   ① `AddtoMapSuccess && !IsHide` —— 挂图失败时**不移除**（留在 `QuestNpcList`/`MerchantList`）；
   ② `ClientMerchantDlgSelect`/`ClientTakeDlgItem` 的 `npc.IsHide` **视距放行**门。
4. 与装载链的顺序：在 `LoadMapQuest` 之后、地图/商人初始化之前（对齐 `src/GameSrv/GameApp.cs:184` 的次序）。

## 4. 验收

- 用 oracle 的隐藏 NPC `ActorId` 发 `CM_CLICKNPC` / `CM_MERCHANTDLGSELECT`：两侧同帧、同说词/菜单；
- 隐藏 NPC **不出现在**他人视野的 AOI 帧里（负例）；
- 清单 NP-10 升 `verified` 并在 notes 指向本卡与 golden 路径。

## 5. 牙（缺一不可）

1. 把隐藏 NPC 从实体表移除 ⇒ 点击用例零帧、必红（T1，已跑红）；
2. 【订正】"把 `IsHide` 置 false ⇒ AOI 负例红"**咬不住**：墙格挂图失败时 `IsHide=false`
   的实体被整体移除，AOI 里同样没有它。实际落地的同位牙是**删 `attach_npc_to_map` 的
   `&& !is_hide` 保留门**（T4）⇒ 墙格隐藏实体被移除 ⇒ world-bevy 的 "留在世界" 断言红；
   `IsHide` 的语义牙放在**选择**侧：把 `handle_shop_cmd_inner` 的固定 NPC `mhide` 写回
   `false`（T2）⇒ 视距外 select 例零帧 ⇒ `np10_hidden_npc_matches_oracle_golden` 红；
3. 乱配 `ActorId`（与脚本表索引错位）⇒ 点击命中错误脚本、必红（T3：`set_actor_id` 写
   `actor_id + 1` ⇒ 脚本表查不到 ⇒ click 例零帧红）；
4. 【补】预置 ctor 抽改用重抽（T5）⇒ world-bevy 的 RNG 零抽数牙红。

## 6. 写集（预登记）

- `bins/game-svr` 的隐藏 NPC 装载/实体化与 click/select 路由；
- `crates/world-bevy` 的实体/AOI（仅隐藏可见性相关）；
- 测试：`bins/game-svr` 的 `#[cfg(test)]` 段 + `tests/parity/golden/npc/hidden_*.json`；
- **不碰** `crates/script`；与 `a2` 装载链同域，排期由 opc-07 定。

实际改动文件（分支 `card/hidden-npc-entities` @ `a7255cba9`）：
`bins/game-svr/src/{lib,npc,merchant,npcs,ctor_fold}.rs`、`bins/game-svr/tests/quests_parity.rs`、
`crates/world-bevy/src/{npc,components}.rs`、`tests/parity/csharp/NpcGolden/Program.cs`、
`tests/parity/golden/npc/np10_hidden_npc.json`。**未碰** `crates/script`。

## 7. 实现记录（2026-11-20）

### 7.1 落点

| 层 | 改动 | oracle 锚 |
| --- | --- | --- |
| world-bevy | `NpcState::is_hide`（挂图门 + select 视距门两个消费点，**不进视野过滤器**） | `WorldServer.Merchant.cs:47`、`PlayObject.Operate.cs:125` |
| world-bevy | `insert_npc_pre_drawn`：ctor 4 抽由构造段账本预置，**不重抽**共享 RNG | `LocalDB.cs:268/312` → `BaseObject.cs:439/441` |
| world-bevy | `attach_npc_to_map` 的 `AddtoMapSuccess && !IsHide` 门 | `WorldServer.Merchant.cs:47` |
| game-svr | `NpcDef` 改 `Rc` 共享：`NpcHost` 与 `MerchantScriptBook` 用**同一份** `ScriptList`（不二次解析 ⇒ 不重抽 `rename_rand`） | `src/Modules/ScriptEngine/ScriptParsers.cs:857` |
| game-svr | `insert_hidden_quest_npc`（npc 态 type 0、`IsHide` 按记录）+ `MerchantScriptBook::insert_host_def`（键 = 世界实体 id、注入 `Envir` 根） | `LocalDB.cs:268/317` |
| game-svr | `ctor_fold::hidden_npc_ctor_draws`（fail-closed 取账本 4 抽；`QFunction` 的 `RunTick` 复用同一实现） | `LocalDB.cs:210/268/312` |
| game-svr | sim_loop：`LoadMapQuest` 段建 QManage/RobotManage 实体（槽位**先于** `Npcs.txt`）；`NpCinitialize` 倒序挂图（RobotManage → QManage）；NpcHost 装载后按实体 id 登记脚本 | `src/GameSrv/GameApp.cs:184`、`WorldServer.Merchant.cs:37` |
| game-svr | `handle_shop_cmd_inner` 的固定 NPC `mhide` 取 `NpcState::is_hide`（原硬编码 `false`） | `PlayObject.Operate.cs:125` |

### 7.2 证据

- golden：`mir2-rs/tests/parity/golden/npc/np10_hidden_npc.json`（生成器 `csharp/NpcGolden`，
  新增 `ScriptDir`/`IsHide` 两个 CaseSpec 字段）。四例：`hidden_click_main`（同格 ⇒ 643，
  说词尾 `QManage/任务管理：隐藏入口\ <离开/@exit>`）、`hidden_click_far`（>15 格 ⇒ 零帧）、
  `hidden_select_far_ishide`（视距外 select，`IsHide` 放行 ⇒ 643 + `can_jmp=@exit`）、
  `hidden_select_far_not_hidden`（同输入 `IsHide=false` ⇒ 零帧）。
- 重录纪律：np01..np09 重跑后经 `tests/parity/npc/golden_recheck.py` 逐文件
  `BEHAVIOR_IDENTICAL`（行为字段零变化），np10 为新增文件。
- Rust 比较器：`np10_hidden_npc_matches_oracle_golden`（比对 ident/param/tag/series/len
  （线缆 len = body len + 20）+ `recog == 实体 id` + 643 说词尾；空 `sm` 例断言 click 返回 false）。
- AOI 面（oracle 无地图装置的间接证据）：world-bevy 用例
  `hidden_npc_stays_world_on_wall_cell_and_is_absent_from_aoi` —— 墙格隐藏 NPC 留世界、
  `add_to_mapped=false`、玩家扫描看不到；**同用例内可走格 NPC 必须可见**（反虚空对照）。
- world-bevy RNG 牙：`insert_npc_pre_drawn_uses_given_draws_and_does_not_consume_rng`。

### 7.3 牙（逐条实跑，恢复后 sha 相等 + 复跑绿）

| 牙 | 变异 | 红面 |
| --- | --- | --- |
| T1 | `insert_hidden_quest_npc` 恒返回 `None`（不入实体表） | `np10_hidden_npc_matches_oracle_golden` 红 |
| T2 | `handle_shop_cmd_inner` 的 `mhide` 写回 `false` | 同上，`hidden_select_far_ishide` 帧数 0 ≠ 1 |
| T3 | `set_actor_id` 写 `actor_id + 1` | 同上，`hidden_click_main` 帧数 0 ≠ 1 |
| T4 | 删 `attach_npc_to_map` 的 `&& !is_hide` | world-bevy 墙格用例红（click 例仍绿：rig 的 (0,0) 可走，实体正常落格 —— 见卡面口径） |
| T5 | `insert_npc_pre_drawn` 丢弃预置抽、走重抽 | world-bevy RNG 零抽数用例红 |

### 7.4 剩余（未在本卡范围，登记）

- `MapQuest.txt` 的 `sQuest` NPC（`src/GameSrv/Maps/MapQuestManager.cs:40-58`）与 `MapInfo` `CHECKQUEST`
  的 NPC（`src/GameSrv/Maps/Maps.cs:463`）同为隐藏 `QuestNpcList` 记录，**仍未建世界实体**（`NpcHost` 只装载脚本）。
  它们同样可被 oracle 用 `ActorId` 点到 ⇒ 另开卡或并入本线下一批。
- `NpcHost::goto_label`（`@Login`/`@StdModeFunc` 入口）仍按 `ActorId=0`/`Envir=null` 口径
  （登记；click/select 入口已用真实实体 id + `Envir` 根）。
- `NpcHost::load` 的装载次序（正序 = `MapQuest.txt` 记录 → QManage → RobotManage → QFunction）
  与 oracle `NpCinitialize` 的**倒序**装载次序不同；仅在重名 label 触发 `rename_rand` 抽数时可见
  （部署语料未观察到该抽数），登记为口径差。
