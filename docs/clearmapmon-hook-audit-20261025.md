# CLEARMAPMON host hook no-op 审计（tier L，取证优先）

- 卡：`openmir2-clearmapmon-hook-audit`（tier **L**，owner `openmir2-dev-2`，branch `card/clearmapmon-hook-audit`）。
- 基线：`master = 4681c685b`（2026-10-25）。
- 写集：本报告（未改代码/规则/卡片状态）。
- 结论：**部署有 2 处使用**（不是 0）⇒ **不能登记为 no-op 等价**；需实现，且触世界状态/时序/AOI
  帧面 ⇒ 建议 **升 R**（本卡不实现）。

## 1. oracle 语义（逐行）

### 1.1 动作注册与解析

| 位置 | 内容 |
|---|---|
| `src/Modules/ScriptEngine/Consts/ExecutionCode.cs:436-437` | `[ScriptDefName("CLEARMAPMON")] ClearMapMon`（枚举值 215） |
| `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:90` | `ProcessExecutionMessage[(int)ExecutionCode.ClearMapMon] = ActionOfClearMapMon;` |
| `ExecutionProcessingSys.cs:2983-3003` | `ActionOfClearMapMon(normNpc, playerActor, questActionInfo, …)`：`sParam1` = 地图名；**无 `sParam2` 解析**（动作本身不带范围/数量参数） |

### 1.2 清怪算法（`ExecutionProcessingSys.cs:2983-3003`）

```csharp
IList<IActor> monList = new List<IActor>();
int monsterCount = WorldEngine.GetMapMonster(MapMgr.FindMap(questActionInfo.sParam1), monList);
for (i = 0..monsterCount) {
    IActor mon = monList[i];
    if (mon.Master != null) continue;                 // 跳过宠物/召唤物（Master != null）
    if (SystemShare.GetNoClearMonList(mon.ChrName)) continue;  // 部署恒 false（见 1.3）
    mon.NoItem = true;                                 // 禁止掉落
    // mon.WAbil.HP = 0;                               // 注释：不改 HP
    mon.MakeGhost();                                   // 置 Ghost + GhostTick + DisappearA
}
```

- `GetMapMonster`（`src/GameSrv/Word/WorldServer.cs:1639-1674`）：遍历
  `MonGenInfoThreadMap` 的每个 `MonGenInfo.CertList`，只收
  `!mon.Death && !mon.Ghost && mon.Envir == envir` 的怪；`envir == null`（`FindMap` 失败）⇒ 返回 0
  （**地图名不存在 = 静默 no-op**）。
- `GetNoClearMonList`（`src/Modules/SystemModule/SystemShare.cs:1694-1706`；镜像
  `src/M2Server/M2Share.cs:1782-1794`）：**整段循环被注释，恒返回 `false`** ⇒ 该过滤实际不生效。

### 1.3 帧/状态效应

- `MakeGhost()`（`src/M2Server/Actor/BaseObject.Base.cs:484-496`）：
  `CanReAlive` ⇒ `Invisible = true`；否则 `Ghost = true`；`GhostTick = GetTickCount()`；
  随后 `DisappearA()`。
- `DisappearA()`（`src/M2Server/Actor/BaseObject.cs:1519-1523`）：
  `Envir.DeleteFromMap(CurrX, CurrY, CellType, ActorId, this)` +
  `SendRefMsg(RM_DISAPPEAR, 0,0,0,0,"")`（观察者收到 `SM_DISAPPEAR(30)`；
  `PlayObject.Message.cs:1651-1654`）。删除/入队均在**当次动作内**完成。
- `NoItem = true` 影响后续掉落（`BaseObject.cs:2455-2459`：`!NoItem || !Envir.Flag.NoDropItem`）；
  **不是**立即删实体：`MonGenInfo.CertList` 条目保留，后续 `MonGen` tick 按 Ghost/`GhostTick`
  清理/复活（时序相关）。
- **无 RNG 直接抽数**；但世界实体集合/AOI/战斗目标/Boss 存活状态与 `SM_DISAPPEAR` 出队时序会变。

## 2. Rust 现状（逐点）

| 位置 | 现状 |
|---|---|
| `mir2-rs/crates/script/src/engine/mod.rs:687-688` | `fn clear_map_monsters(&mut self, _map: &str) {}` —— **trait 默认 no-op** |
| `mir2-rs/crates/script/src/engine/actions.rs:2024-2026` | `ActionOfClearMapMon` 分支调用 `self.player.clear_map_monsters(&info.s_param1)`（参数即 `sParam1`） |
| `mir2-rs/crates/script/src/codes.rs:447` / `handlers.rs:210` | `CLEARMAPMON` code=215 / `ActionOfClearMapMon` 已注册 |
| `mir2-rs/bins/game-svr/src/npc.rs` | **`NpcPlayer` 未覆写 `clear_map_monsters`**（`grep -rn clear_map_monsters mir2-rs/bins mir2-rs/crates/world` ⇒ 0）⇒ 落到默认 no-op |
| `mir2-rs/tests/parity/coverage/script_host_noop_audit.json:1795-1826` | 审计条目：`overridden=false`、`verdict=needs_card`、consequence「CLEARMAPMON 类动作不生成/不清除怪物」 |
| `mir2-rs/tests/parity/m3/audit-script-actions-20261018.md:390` | 旧盘点：CLEARMAPMON「已实现」指**动作分派/参数**，但 host hook 空 ⇒ 只到 no-op |

**影响面**：脚本执行 `CLEARMAPMON <地图>` 时**不产生任何副作用**——目标地图的存活怪不会被置
`NoItem`/`Ghost`、不删格、不发 `SM_DISAPPEAR`；随后 `MONGENEX …` 仍照常刷怪（脚本本意是
「清场再刷一只」）。`ActionOfMarry`/`ActionOfUnMarry` 也间接调用同一 hook（审计 JSON
`indirect_handlers`），同样受影响（本卡只审 CLEARMAPMON）。

## 3. 部署语料使用面（`E:/MirServer/M2GameSvr/Envir`）

`grep -rni "CLEARMAPMON"`（只读）：

| # | 文件:行 | 行内容 | 目标地图 | MapInfo |
|---:|---|---|---|---|
| 1 | `Market_Def/任务NPC/武士-0114.txt:145` | `CLEARMAPMON T118` | `T118` | `MapInfo.txt:17 [T118 …]` ✔ |
| 2 | `Market_Def/任务NPC/老人-1.txt:49` | `CLEARMAPMON Q001` | `Q001` | `MapInfo.txt:16 [Q001 …]` ✔ |

- 计数：**2 个文件 / 2 行**（大小写不敏感；无其它拼写/变体）。
- 两处都在 `#act` 后紧接 `MONGENEX <同一地图> …` + `map <同一地图>`（任务地图清场再刷一只）。
- 地图存在 ⇒ 不是「地图名不存在 ⇒ FindMap null 静默 no-op」的豁免路径。

## 4. 判定与实现方案（本卡不实现）

### 4.1 判定

- **部署 2 处使用** ⇒ **不能**把 no-op 登记为等价；需实现。
- **tier 建议 R**：实现会改世界实体集合/AOI/战斗目标与 `SM_DISAPPEAR` 出队时序（虽无直接 RNG
  抽数）；按「判不准按 R」+ 判据涉及世界状态与帧序，走真机窗口。**不在本卡实现**。

### 4.2 实现方案（给后续 R 卡）

1. **world 层**（`crates/world`）新增 `World::clear_map_monsters(map_id) -> Vec<ClearMapMonRecord>`：
   - 遍历 `World.entities` 中 `mon_gen.is_some()` 的实体，过滤 `!death && !ghost && map_id`；
   - 跳过 `entity.master.is_some()`；`NoClearMonList` 按 oracle 恒 false（Rust
     `crates/envir/src/noclearmonlist.rs` 已是 no-op，保持一致）；
   - 置 `entity.no_item = true`；调用 `monai::make_ghost` 的等价逻辑（当前私有，需提
     `pub(crate)`/复用 `make_ghost`），保留 `GhostTick = GetTickCount()` 口径；
   - 从地图格子删除（复用 `DeleteFromMap` 等价物）并向观察者入队 `RM_DISAPPEAR`（走既有
     `downlink` 的 `SM_DISAPPEAR(30)`）。
2. **host 桥**（`bins/game-svr`）：`ScriptPlayer` 无 `&mut World`。与 `world_broadcasts` 同法，
   在 `NpcPlayer` 收集 `clear_map_requests: Vec<String>`，`run_merchant_goto` 脚本跑完后由宿主
   用 `MapRegistry` 解析名→`map_id` 并调用 world 方法；`FindMap` 失败 ⇒ 零操作（oracle 同）。
3. **合同测试**：过滤（Death/Ghost/Master/异图不删）、`no_item`/`ghost`/`GhostTick` 写值、
   `SM_DISAPPEAR` 帧集合与收件人、`MonGenInfo.CertList` 保留但后续 tick 清理。
4. **R 窗口项**：T118/Q001 真机清场后两侧实体集合/AOI/`SM_DISAPPEAR` 帧序 diff=0；随后
   `MONGENEX` 刷怪数与重刷时序 diff=0。

## 5. 门禁 / 残留

- 纯取证文档：卫生三门禁 + `git diff --check` + merge-tree（脚本输出）见 patch body。
- 未改代码、未改 `script_host_noop_audit.json`（生成物）、未改卡片状态。
- 残留：无。不 merge、不自审。
