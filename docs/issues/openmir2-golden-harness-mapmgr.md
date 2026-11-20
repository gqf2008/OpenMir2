# openmir2-golden-harness-mapmgr — 离线 harness 缺 MapMgr，阻塞走 SpaceMove 的召唤回召面

**状态**：**已实现并对拍**（`card/mapmgr-design`：OfflineMapSystem 替身 + SINSU cap_hit 回召面；两例 PASS、3 牙红、四门绿）。
**查证/设计 head**：base master `8ff6c91fb`。

## 1. 现象（NPE 栈）
`dotnet MagicGolden.dll --case sinsu`（含 cap_hit 回召支）抛 `System.NullReferenceException`：
```
M2Server.Actor.BaseObject.SpaceMove(String,Int16,Int16,Int32)  BaseObject.cs:1105
M2Server.Actor.BaseObject.RecallSlave(String)                 BaseObject.cs:1866
M2Server.Magic.MagicManager.MagMakeSinSuSlave(...)            MagicManager.cs:1302
```
根因：`SpaceMove` 首句 `SystemShare.MapMgr.FindMap(sMap)` —— harness 只设了 `SystemShare.WorldEngine = new OfflineWorldEngine()`（`Program.cs:675/:1697`），**未设 `SystemShare.MapMgr`（类型 `IMapSystem`）** ⇒ null ⇒ NPE。

## 2. 需要的最小替身
- 实现 `IMapSystem`（`src/Modules/SystemModule/SubSystem/IMapSystem.cs`）的 `IEnvirnoment FindMap(string)`，**返回 harness 已 `LoadMapData` 的那个 `Envirnoment`**（仿 `OfflineWorldEngine` 的最小替身），在 `Program.cs:675/:1697` 处一并设 `SystemShare.MapMgr`。
- **注意级联**：`SpaceMove` 过 `FindMap` 后还会 `Envir.DeleteFromMap(未入图的 pre-slave)`、`SpaceMoveGetRandXY`（**抽 RNG**）、`AddMapObject`、`SendMsg/SendRefMsg`。⇒ 替身不仅要非 null，还要这些不 NPE，且**其 RNG 抽取要进账本与 Rust 对齐**。属**影响全 harness 的 ③ 级改动**（任何走 `SpaceMove` 的路径都会变）。

## 3. 叶子处置（cap_hit 已解）
- **SINSU(30) `cap_hit`/回召**——**已解**（OfflineMapSystem 替身 + 预置 slave 经 `RecordingMonster` 镜像/同构 target 构造；cap_hit 两例对拍含 `recall_moved`/坐标/807 不对称登记）。
- 潜在同类：`AMYOUNSUL(6)` / `EARTHFIRE(22)` 等若路径含 `SpaceMove`（需逐条核）。
- 关联：**[[openmir2-golden-generator-drift]]**（golden schema/墙钟漂移）。

## 4. 最小替身形态（细化）
- **类型**：`SystemShare.MapMgr : IMapSystem`（`src/Modules/SystemModule/SubSystem/IMapSystem.cs`）。`IMapSystem` 10 成员：`Maps(get) / AddMapInfo / AddMapRoute / FindMap / GetDoorMapList / GetMapInfo / GetMapOfServerIndex / GetMineMaps / LoadMapDoor / MakeSafePkZone`。
- **最小替身 `OfflineMapSystem : IMapSystem`**（仿 `OfflineWorldEngine`）：
  - `FindMap(name)` ⇒ 返回 harness 已 `LoadMapData` 的那个 `Envirnoment`（按 `MapName` 匹配）；**name 不匹配 ⇒ 返回 null**（保持 oracle「找不到=null」语义，不静默给假图）。
  - **其余 9 成员一律抛 `NotImplementedException`**（触达即炸，比静默假值安全——同 `OfflineWorldEngine` 的 40-throw 约定）。
- **明确不做**：不建图、不载 `MapInfo.txt`、不刷怪、不触发地图事件、不做城门/安全区。
- **实现后的边界（已落地验证）**：`SpaceMove` 过 `FindMap` 后还会 `DeleteFromMap/SpaceMoveGetRandXY/AddMapObject/SendMsg/SendRefMsg`。**曾在此 NPE**（`RM_SPACEMOVE_SHOW → baseObject.GetFeature`，`PlayObject.Message.cs:1975`，对裸 `MonsterObject`）；**已由「镜像/同构 target 构造」消解**——预置 slave 改用 `RecordingMonster`，与 target 怪物夹具**同形**（`Program.cs:1815`/`:1882-1891`：均为 **harness 手填** Race/Abil/WAbil/Envir/ActorMgr/AddMapObject，**非生产 `RegenMonsterByName`**），git 实测 `cap_hit` `recall_moved=true`（神兽 (0,0)→施法者面前）。**`RecordingMonster` 之外的其余 9 个 `IMapSystem` 成员仍触达即红（抛 NotImplementedException，边界不扩）**。另：C# 侧为 faithful 也把 caster 面向设成施法方向（`caster.Dir=GetNextDirection(caster→click)`），使回召落点与 Rust 一致。
- **RNG**：`SpaceMoveGetRandXY` 真走 MapGrid 抽数 ⇒ 回召面的 Rng 与 oracle 同源（不再有「替身 0 抽」偏移）。

## 5. 影响面（设了 MapMgr 后哪些既有 golden 可能变）
- **直达 `SpaceMove`/`Move` 的效果（C# `MagicManager.cs`）**：SINSU(30) `RecallSlave`(:1302)、SPACEMOVE(21) `MagSaceMove`(:876→:407)、push：FIREWIND(8)(:260) / ENERGYREPULSOR(37)(:602) 经 `MagPushArround`(:24)。
- **既有 golden 是否有重生成风险**：本仓现有 **36** 个 `golden/magic/*.json` **都在 MapMgr=null 下成功生成** ⇒ 它们运行期**未触达 `SpaceMove/FindMap`**（否则当场 NPE、出不了 golden）。⇒ **设 MapMgr 原则上不改变这 36 份既有 golden 的行为**（同场景同分支）。
- **判定方法**：对每个既有 golden 的场景，grep 其魔号 effect 在该场景分支是否调用 `SpaceMove/FindMap`；只有触达者才有重生成风险。当前触达者 = 上面四类，其中 **SINSU cap_hit / SPACEMOVE 尚无 golden（正是要解锁的）**；push(8/37) 若 golden 走 push 分支需核（`Move` 是否经 `FindMap`）。
- **结论**：**不是"全部既有 golden 都重生成风险"**。风险面 = 触达 `FindMap` 的叶子；既有 36 份（未触达）稳定。**建议**：替身上线后**只对触达者（SINSU cap_hit / SPACEMOVE）**生成/复跑，其余 36 份不动；若某既有 golden 重生成出 diff，则它之前是靠「未触达」避开的，按触发点单独审。

## 6. 验收口径 + 牙
- 替身上线后，SINSU `cap_hit` 的 `recall_moved` 从**当前「空集恒假」**变成**真断言**：预置 slave 存在 ⇒ `RecallSlave` 移它到面前 ⇒ `pre_slave_xy_before ≠ after`、`recall_moved=true`；负向（预置名不匹配 ⇒ 空操作）`recall_moved=false`（**空操作 vs 移动的分界**）。
- **该红在哪里的牙**：
  - 牙① 删/短路 `RecallSlave` 调用（或恒不调）⇒ cap_hit 的 pre-slave 坐标 `before==after` ⇒ `recall_moved=true` 断言**红**；
  - 牙②（边界）预置名改成不匹配（非 `神兽`/`神兽1`，如 `神兽0`）⇒ 空操作 ⇒ 若断言硬要求 `recall_moved=true` 则红——界定「空操作」也即「名字精确匹配」这条 oracle 事实。
  - 附：`SpaceMoveGetRandXY` 抽 RNG ⇒ cap_hit 的 Rng 与 oracle 同源，**应纳入 cap_hit 的 Rng 逐值比较**（区别于 SINSU summon_ok 的 Rng 不可比）。

## 7. 不做的事
- 不为了让断言变绿去改产品（`SpaceMove`/`RecallSlave`/`Maps`/`MagMake*` 等产品代码**不动**）。
- 不顺手扩到其它叶子（AMYOUNSUL/EARTHFIRE 等）；本卡只解 SINSU `cap_hit` 的 `SpaceMove` 依赖；其它叶子另评估。
- 不为了让既有 golden"看起来一致"而重生成整批；只动触达者（见 §5）。
