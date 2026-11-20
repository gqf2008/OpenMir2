# openmir2-golden-harness-mapmgr — 离线 harness 缺 MapMgr，阻塞走 SpaceMove 的召唤回召面

**状态**：事实登记（只读查证）；**阻塞项**（SINSU cap_hit 等）。
**查证 head**：master `2692d0a26`。

## 1. 现象（NPE 栈）
`dotnet MagicGolden.dll --case sinsu`（含 cap_hit 回召支）抛 `System.NullReferenceException`：
```
M2Server.Actor.BaseObject.SpaceMove(String,Int16,Int16,Int32)  BaseObject.cs:1105
M2Server.Actor.BaseObject.RecallSlave(String)                 BaseObject.cs:1866
M2Server.Magic.MagicManager.MagMakeSinSuSlave(...)            MagicManager.cs:1302
```
根因：`SpaceMove` 首句 `SystemShare.MapMgr.FindMap(sMap)` —— harness 只设了 `SystemShare.WorldEngine = new OfflineWorldEngine()`（`Program.cs:116-118`），**未设 `SystemShare.MapMgr`（类型 `IMapSystem`）** ⇒ null ⇒ NPE。

## 2. 需要的最小替身
- 实现 `IMapSystem`（`src/Modules/SystemModule/SubSystem/IMapSystem.cs`）的 `IEnvirnoment FindMap(string)`，**返回 harness 已 `LoadMapData` 的那个 `Envirnoment`**（仿 `OfflineWorldEngine` 的最小替身），在 `Program.cs:116-118` 处一并设 `SystemShare.MapMgr`。
- **注意级联**：`SpaceMove` 过 `FindMap` 后还会 `Envir.DeleteFromMap(未入图的 pre-slave)`、`SpaceMoveGetRandXY`（**抽 RNG**）、`AddMapObject`、`SendMsg/SendRefMsg`。⇒ 替身不仅要非 null，还要这些不 NPE，且**其 RNG 抽取要进账本与 Rust 对齐**。属**影响全 harness 的 ③ 级改动**（任何走 `SpaceMove` 的路径都会变）。

## 3. 被阻塞的叶子
- **SINSU(30) `cap_hit`/回召**（`RecallSlave→SpaceMove`）——MG-41 首批因此只做 `summon_ok`。
- 潜在同类：`AMYOUNSUL(6)` / `EARTHFIRE(22)` 等若路径含 `SpaceMove`（需逐条核）。
- 关联：**[[openmir2-golden-generator-drift]]**（golden schema/墙钟漂移）。
