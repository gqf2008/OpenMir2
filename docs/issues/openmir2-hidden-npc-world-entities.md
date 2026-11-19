# openmir2-hidden-npc-world-entities（NP-10 缺口卡，只写卡不实现）

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
3. 与视野门的关系：`IsHide` 实体**不进视野广播**（AOI 不向他人下发出现帧），但保持可被 click/select 命中；
4. 与装载链的顺序：在 `LoadMapQuest` 之后、地图/商人初始化之前（对齐 `src/GameSrv/GameApp.cs:184` 的次序）。

## 4. 验收

- 用 oracle 的隐藏 NPC `ActorId` 发 `CM_CLICKNPC` / `CM_MERCHANTDLGSELECT`：两侧同帧、同说词/菜单；
- 隐藏 NPC **不出现在**他人视野的 AOI 帧里（负例）；
- 清单 NP-10 升 `verified` 并在 notes 指向本卡与 golden 路径。

## 5. 牙（缺一不可）

1. 把隐藏 NPC 从实体表移除 ⇒ 点击用例零帧、必红；
2. 把 `IsHide` 置 false ⇒ AOI 负例红（出现帧不该出现）；
3. 乱配 `ActorId`（与脚本表索引错位）⇒ 点击命中错误脚本、必红。

## 6. 写集（预登记）

- `bins/game-svr` 的隐藏 NPC 装载/实体化与 click/select 路由；
- `crates/world-bevy` 的实体/AOI（仅隐藏可见性相关）；
- 测试：`bins/game-svr` 的 `#[cfg(test)]` 段 + `tests/parity/golden/npc/hidden_*.json`；
- **不碰** `crates/script`；与 `a2` 装载链同域，排期由 opc-07 定。
