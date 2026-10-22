# 12 个孤儿出站编码器的触发源审计（oracle 发射点 × 触发链 × 可达性）

- 卡：`openmir2-orphan-encoder-trigger-audit`（issue `ca6dc0775eaf9271d04b9645077edb55a3b85e86`，tier=L）
- 作者：`openmir2-worker-32`（worktree `wt-worker-32` / 分支 `card/orphan-audit`）
- 基线：master `dbd03db97`；oracle = 本仓 `src/**`（C# 参照实现），行号均为该 commit 实测
- 写集：**仅本文件**。不改代码 / 编码器 / 规则表 / `whitelist.md`（结论以「建议条目」形式交后续卡落地）

---

## 0. 结论摘要

| 三态 | 帧数 | 帧 |
|---|---|---|
| **可直接接线**（上游生产者已在，只缺宿主映射/发射） | **1** | `SM_CHANGEFACE(1104)` |
| **需先补上游生产者**（生产链本身未建模） | **8** | `SM_BREAKWEAPON(1102)`、`SM_HIDEEVENT(805)`、`SM_LAMPCHANGEDURA(655)`、`SM_QUERYYBDEAL(23023)`、`SM_QUERYYBSELL(23021)`、`SM_RECONNECT(802)`、`SM_SHOWEVENT(804)`、`SM_SKELETON(33)` |
| **不可达（oracle 结构性，应登记不接线）** | **3** | `SM_CLOSEDOOR(614)`、`SM_ITEMSHOW(610)`、`SM_OPENDOOR_OK(612)` |

合计 12（`SM_DONATE_FAIL` 归卡 `openmir2-m3-guild-state`，不在本卡范围）。

**三条与既有台账不一致的实测结论**（详见 §9）：

1. **`SM_CHANGEFACE` 的上游「未实现」已过时**。`crates/world` 的 `monai::elf_warrior_run` + `World::flush_sculture_spawns` 已 1:1 建模 ELF 分裂支，并**已经在世界层发出 `RM_CHANGEFACE`**（`crates/world/src/monai.rs:2980-2988`）；内容面（`sql/mir2_data.sql` 的 `神兽1` race=114 / `神兽` race=113）齐备。缺的只是宿主 `drain_monster_ref_msgs` 的一条 match 臂。⇒ T-175 的前提作废，本帧归「可直接接线」。
2. **`SM_SHOWEVENT`/`SM_HIDEEVENT` 可达，但语义是退化的**：`MapEvent.Id` 全仓无赋值点（`MapEvent.cs:62` 被注释）⇒ 所有事件格子共用 `CellObjId = 0` ⇒ `CellObjectMgr` 里 key 0 **先到先得**、且 `DeleteFromMap` 的 `CellObjId > 0` 门让事件格子**永不移除**。结论不是「不可达」，而是「只有进程内第一个 MapEvent 能被解析」。逐条推导见 §4。
3. **`SM_RECONNECT` 有第三个生产者（GM 命令 `@Reconnection`）**，但现网 `Permission` 恒 0 ⇒ perm 10 的命令**在生产路径上被权限门挡死**；剩下的跨服搬家链（`SendChangeServer`/`TransferPlanesServer`）才是真生产面，且 `TransferPlanesServer` 的**唯一调用点已被注释**（`AppService.cs:179`）。⇒ 该帧在两侧现网**都不出帧**，属「需先补上游（G11 跨服）」而非「接线」；详见 §5。

---

## 1. 方法与穷举面（沿用门链 T-65 的标准）

### 1.1 为什么用「全树 grep + 逐点归类」而不是「逐面抽样」

门链那次被驳回的教训是「结论对但证据不足」。本卡把**可能的生产者面**定义成 8 面，然后用**对 `src/**` 全树**的常量扫描作为**超集证据**（全树扫描天然覆盖全部 8 面，不会漏面），再把每个命中点归类到面、说明「为何是/为何不是生产者」。

| 面 | 定义 | 载体 |
|---|---|---|
| F1 CM 派发 | 客户端消息 → `case CM_*` → `ClientXxx(...)` | `src/M2Server/Player/PlayObject.Message.cs` 的 `ProcessMsg` |
| F2 脚本动作 | `ProcessExecutionMessage[code]` → `ActionOf*` | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs` |
| F3 GM 命令 | `[Command]/[ExecuteCommand]` | `src/Modules/GameCommand/Commands/**` |
| F4 宿主 tick | `WorldServer` 主循环 / `PlayObject.Run` / `BaseObject.Run` / `EventProcessor` / MonGen | `src/GameSrv/Word/**`、`src/M2Server/Player/PlayObject.Message.cs:19` |
| F5 事件/魔法子系统 | `MagicManager` → `EventMgr.AddEvent(MapEvent)` | `src/M2Server/Magic/MagicManager.cs`、`src/M2Server/Event/**` |
| F6 其它对象广播 | `SendRefMsg` 的 ±N 格 AOI 泵；`BaseObject` 的掉落/屠宰/门 | `src/M2Server/Actor/**` |
| F7 直接发帧 | `SendDefMessage` / `MakeMessage` **绕过 RM 泵** | `src/GameSrv/Word/WorldServer.cs:1074` 等 |
| F8 内容装载 | 地图对象注册（门/事件/矿物） | `src/GameSrv/Maps/MapManager.cs`、`src/M2Server/Maps/Envirnoment.cs` |

**证据命令**（可复现，见 §11）：对 12 个 `RM_*` 常量各做一次全树 grep；命中总数与归类见 §2 主表。任何 F 面若存在发射点，**必然**出现在该 grep 结果里（C# 的 RM 泵只有 `SendMsg`/`SendRefMsg`/`SendDefMessage`/`MakeMessage` 四种入口，全部以 `Messages.RM_*` 字面量出现）。

### 1.2 三态定义（本卡口径）

- **可直接接线**：oracle 生产链在 Rust 侧**已经有等价生产点**（世界层已发同 ident 的 RM，或可直接调用的纯函数），只差「RM → SM 的帧映射」或「函数调用点」。改动面 = `bins/game-svr/**`（不进 `crates/world`）。
- **需先补上游生产者**：oracle 的触发链在 Rust 侧缺至少一环（状态字段 / 结算函数 / 子系统），必须先补生产链再谈接线。
- **不可达**：oracle 侧因「id 恒 0 / 守卫写反 / 调用点被注释 / 权限锁死」等**结构性原因**永远不发帧 ⇒ 接线即制造 diff。必须留痕登记。

---

## 2. 台账主表（逐帧一行）

| # | 帧 | 常量 | 编码器（本仓） | oracle 发射点（逐点） | 触发链 | 三态 | 最小改动面（若可达） | tier |
|---|---|---|---|---|---|---|---|---|
| 1 | `SM_BREAKWEAPON` | 1102 | `smframe_ext.rs:101` `sm_breakweapon` | `PlayObject.cs:4121`（`CheckWeaponUpgrade`，`SendRefMsg`） | `AttackDir`（`PlayObject.Attack.cs:27-31`，武器 `Desc[10]>0`）→ `CheckWeaponUpgrade`（`:4110`）→ `CheckWeaponUpgradeStatus`（`:4146`，`Desc[10]==1 ⇒ Index=0`）→ `SendRefMsg(RM_BREAKWEAPON)` → `Message.cs:2015` | 需补上游（结算函数未建模） | `crates/world`（升级结算）+ `bins/game-svr`（攻击泵发帧） | R |
| 2 | `SM_CHANGEFACE` | 1104 | `smframe_ext.rs:115` `sm_changeface` | `Monster/Monsters/ElfWarriorMonster.cs:149`（`SendRefMsg`） | `ElfWarriorMonster.Run`（`:114-160`）`boChangeFace && now-DigDownTick>60s && 名末 '1'` → `MakeClone("神兽")` → `RM_DIGDOWN`+`RM_CHANGEFACE` → `Message.cs:2021` | **可直接接线** | `bins/game-svr/src/lib.rs:15409`（`drain_monster_ref_msgs` 加臂） | L |
| 3 | `SM_CLOSEDOOR` | 614 | `smframe_ext.rs:171` `sm_closedoor` | `GameSrv/Word/WorldServer.cs:1485`（`CloseDoor` → `SendDoorStatus`） | `ProcessMapDoor`（`:1522`，仅 `door.Status.Opened` 才 `CloseDoor`）；`Opened` 唯一置 true 点是 `OpenDoor(:1471)`，而 `OpenDoor` 的唯一调用点 `ClientOpenDoor`（`PlayObject.Operate.cs:363-376`）**守卫写反即早退** ⇒ 恒 false | **不可达**（引 T-65） | — | — |
| 4 | `SM_HIDEEVENT` | 805 | `smframe_ext.rs:201` `sm_hideevent` | `PlayObject.cs:3278`（`SearchViewRange` 差集）；镜像 `RobotSystem/Services/RobotPlayer.Base.cs:766` | `WorldServer.cs:663-666`（玩家 `SearchTime=1000ms`）→ `PlayObject.SearchViewRange`（`:3073`）→ `CellType.Event` → `UpdateVisibleEvent`（`:5465`）→ `VisibleEvents` 差集 `Hidden` 支 → `RM_HIDEEVENT` → `Message.cs:1992` | 需补上游（事件格子 + `VisibleEvents` 未建模） | `crates/world`（aoi 事件格 / 观察者可见事件集）+ `bins/game-svr`（映射） | R |
| 5 | `SM_ITEMSHOW` | 610 | `smframe_ext.rs:271` `sm_itemshow` | ①`BaseObject.cs:570`（`DropItemDown`）②`BaseObject.cs:752`（`DropGoldDown`）③`PlayObject.cs:3264`（`SearchViewRange` 的 `VisibleItems`）④**`ExecutionProcessingSys.cs:5703`（`ActionOfThrowitem` 金币支）**（④为既有 T-179 未列的第 4 生产者） | ①②③④ 全部以 `Envir.AddItemToMap(...)==true` 为前置；`AddItemToMap`（`Envirnoment.cs:88-92`）首行 `if (mapItem.ItemId == 0) return false;`，而**全仓无 `MapItem.ItemId = …` 赋值点**（`MapItem.cs:41` 被注释）⇒ 恒 false | **不可达**（引 T-19/T-25/T-32/T-179；本卡补第 4 生产者 + 独立复核） | — | — |
| 6 | `SM_LAMPCHANGEDURA` | 655 | `smframe_ext.rs:291` `sm_lampchangedura` | `PlayObject.Message.cs:2265`、`:2274`（同一函数 `UseLamp`，`:2233`） | `PlayObject.Run`（`:19`）的 `DecLightItemDrugTime`（`:257-262`，缺省 500ms）→ `UseLamp()` → 右手槽 `Dura-1`（`DecLampDura=true`）→ 千位变化发 `:2274`、燃尽发 `:2265` → `Message.cs:1842` | 需补上游（`UseLamp` 未建模） | `crates/world`（`use_lamp` 结算）+ `bins/game-svr`（Run 段调用 + 映射） | R |
| 7 | `SM_OPENDOOR_OK` | 612 | `smframe_ext.rs:365` `sm_opendoor_ok` | `WorldServer.cs:1473`（`OpenDoor` → `SendDoorStatus`） | 同 #3 的左半：`ClientOpenDoor` 守卫写反 ⇒ `OpenDoor` 不可达 | **不可达**（引 T-65） | — | — |
| 8 | `SM_QUERYYBDEAL` | 23023 | `smframe_ext.rs:454` `sm_queryybdeal` | `ExecutionProcessingSys.cs:2550`（`ActionOfQueryTrustDeal`，`:2468`） | 脚本动作 `ExecutionCode.QueryYbDeal=344`（注册 `:127`）→ `SaleDeal && SellOffInTime(1) && SellOffItemList.Count>0` → `EDCode.EncodeBuffer(ClientDealOffInfo)` → `SendMsg(normNpc, RM_QUERYYBDEAL, …, sSendStr)` → `Message.cs:2053` | 需补上游（脚本动作 344 未实现 + 字节尾通道） | `crates/script/src/engine/actions.rs` + `bins/game-svr/src/pickwire.rs`（+ `npc.rs` 的 `SendMsg` 需能载字节尾） | R |
| 9 | `SM_QUERYYBSELL` | 23021 | `smframe_ext.rs:475` `sm_queryybsell` | `ExecutionProcessingSys.cs:2443`（`ActionOfQuerySaleSell`，`:2363`） | 脚本动作 `ExecutionCode.QueryYbSell=343`（注册 `:126`）→ `SaleDeal && SellOffInTime(0) && SellOffItemList.Count>0` → 同上 → `Message.cs:2049` | 需补上游（同 #8） | 同 #8 | R |
| 10 | `SM_RECONNECT` | 802 | `smframe_ext.rs:496` `sm_reconnect` | ①`PlayObject.Base.cs:3417`（`TransferPlanesServer`，**唯一调用点被注释**：`AppService.cs:179`）②`WorldServer.cs:1074`（`SendChangeServer`，走 **F7 直接 `SendDefMessage(SM_RECONNECT)`**；调用点 `:366`/`:609`）③`GameCommand/Commands/ReconnectionCommand.cs:38`（GM `@Reconnection`，perm 10） | ②`UserEnter` 发现目标图不在本服（`MapMgr.GetMapInfo` 返回 null）→ `SendSwitchData` + `SendChangeServer`；`SendChangeServer` 需 `M2Share.GetMultiServerAddrPort`（读 `!servertable.txt`）③现网 `Permission` 恒 0 | 需补上游（G11 跨服搬家；GM 面被权限锁死） | 多服路由 + 宿主会话迁移；不建议先接 | R |
| 11 | `SM_SHOWEVENT` | 804 | `smframe_ext.rs:523` `sm_showevent` | `PlayObject.cs:3284`（`SearchViewRange` 差集）；镜像 `RobotPlayer.Base.cs:777` | 同 #4 的 `Show` 支：`CellType.Event` → `Get<MapEvent>(CellObjId/*恒 0*/)` → `Visible` ⇒ `UpdateVisibleEvent` → `Show` 支 → `RM_SHOWEVENT` → `Message.cs:1995` | 需补上游（同 #4；且需复刻 `Id==0` 折叠语义） | 同 #4 | R |
| 12 | `SM_SKELETON` | 33 | `smframe.rs:2012` `sm_skeleton` | ①`PlayObject.cs:3231`（`SearchViewRange` 的 `Death && Skeleton` 支）②`PlayObject.Operate.cs:736`（`ClientGetButchItem` 屠宰成骨架）③镜像 `RobotPlayer.Base.cs:661` | ①AOI 出现帧；②`CM_BUTCH`（`Message.cs:829`）→ `ClientGetButchItem`（`Operate.cs:702`）→ `BodyLeathery<=0 ⇒ Skeleton=true` → `SendRefMsg(RM_SKELETON)` → `Message.cs:1655` | 需补上游（`Skeleton`/`BodyLeathery` 与屠宰效果体未建模） | `crates/world`（屠宰效果体 + `skeleton` 字段）+ `bins/game-svr`（appear/ref-msg 分派） | R |

---

## 3. 逐帧详证

> 每帧按「编码器 → oracle 发射点（全树命中，逐点归类）→ 8 面排除/确认 → 触发链 → 判定 → 最小改动面」给出。
> 「面」列 `✔`=该面确认存在生产者；`—`=该面无生产者（附一句原因）。

### 3.1 `SM_BREAKWEAPON(1102)` — 需补上游

- **编码器**：`mir2-rs/bins/game-svr/src/smframe_ext.rs:101` `sm_breakweapon(actor_id: i32) -> Vec<u8>`（header-only，12B；`Recog` = 断武器角色 ActorId）。构造用例 `header_only_frames_self_computed_golden`。
- **oracle 发射点（全树 1 命中）**：`src/M2Server/Player/PlayObject.cs:4121`
  `SendRefMsg(Messages.RM_BREAKWEAPON, 0, 0, 0, 0, "")`（在 `CheckWeaponUpgrade()` 内，`:4110`）。
  处理器：`src/M2Server/Player/PlayObject.Message.cs:2015-2017` ⇒ `SendDefMessage(SM_BREAKWEAPON, processMsg.ActorId, 0, 0, 0)`。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | **间接 ✔**：`CM_HIT/CM_HEAVYHIT/CM_POWERHIT/CM_LONGHIT/CM_BIGHIT/CM_CRSHIT` → `AttackDir` → `:27-31` 的 `CheckWeaponUpgrade()`。没有「直接的」`CM_* ⇒ RM_BREAKWEAPON` 支 —— 唯一路径经攻击。 |
| F2 脚本动作 | — 全树 `RM_BREAKWEAPON` 仅 1 处；`ExecutionProcessingSys` 无命中。 |
| F3 GM 命令 | — `Modules/GameCommand/**` 无命中（`@smakeitem` 只打印 `Desc[10]`，见 `SmakeItemCommand.cs:67`）。 |
| F4 宿主 tick | — `PlayObject.Run`/`BaseObject.Run` 无命中。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | **✔ 唯一生产者**（`SendRefMsg` 泵）。 |
| F7 直接发帧 | — 无 `SendDefMessage(SM_BREAKWEAPON)`（只在处理器里）。 |
| F8 内容装载 | — 不产帧；但**内容决定 `Desc[10]`**：`GameSrv/Npc/Merchant.cs:682-736` 是 `Desc[ItemAttr.WeaponUpgrade]` 的唯一写入点（升级 NPC `@getbackupgnow` 取回）。 |

- **触发链（完整）**：
  1. 武器升级 NPC（`Merchant.cs:677-736`）把 `Desc[10] ∈ {1,10..13,20..23,30..33}` 写进 `WeaponUpgradeInfo.UserItem`，玩家取回后装备；
  2. 下一次**攻击**（`AttackDir`，`PlayObject.Attack.cs:27`）门 `UseItems[Weapon].Index>0 && Desc[10]>0` 成立 ⇒ `CheckWeaponUpgrade()`；
  3. `CheckWeaponUpgradeStatus`（`:4146`）按 `Desc[0..2]` 求和与 `Config.UpgradeWeaponMaxPoint`（缺省 20）比较 ⇒ `Index=0`（碎）或 `Desc[0/1/2] += Desc[10]-9/-19/-29`（升级成功），随后 `Desc[10]=0`；
  4. `Index==0` ⇒ `SysMsg(TheWeaponBroke)` + `SendDelItems` + **`SendRefMsg(RM_BREAKWEAPON)`** + `FeatureChanged()`；否则 `SendUpdateItem` + `RM_ABILITY/RM_SUBABILITY`。
- **Rust 现状**：`crates/world/src/entity.rs:80` 的 `UseItemSlot.desc: [u8;14]` 已在（装载/存档都带）；`crates/world/src/merchant.rs:1218/1231/1244` 已按 `Merchant.cs:677-733` 写 `desc[10]`；`crates/world/src/combat.rs:755` 已复刻 `StruckDamage` 的单格耐久磨损。**缺**：`CheckWeaponUpgrade` / `CheckWeaponUpgradeStatus` 两个结算函数与攻击泵的调用点。
- **判定**：**需先补上游生产者**（结算函数）。数据面齐备 ⇒ 成本低于「全新子系统」。
- **最小改动面**：`crates/world/src/`（新增升级结算纯函数 + 在攻击路径调用）+ `bins/game-svr/src/lib.rs`（攻击泵发 `sm_breakweapon`；同批的 `SM_UPDATEITEM/RM_ABILITY/RM_SUBABILITY` 见 T-79⑥/T-95①，**本卡不扩范围**）。
- **tier**：**R**（战斗数值 + `characters_item` 落库时点）。

### 3.2 `SM_CHANGEFACE(1104)` — **可直接接线**（订正 T-175）

- **编码器**：`smframe_ext.rs:115` `sm_changeface(recog, target_id, feature, status)`（头 `(Recog, LoWord(target), HiWord(target), 0)` + 尾 `Encode(CharDesc 8B)`）。
- **oracle 发射点（全树 1 命中）**：`src/M2Server/Monster/Monsters/ElfWarriorMonster.cs:149`
  `SendRefMsg(Messages.RM_CHANGEFACE, 0, ActorId, elfMon.ActorId, 0, "")`。
  处理器：`Message.cs:2021-2029` —— 有 `if (nParam1 != 0 && nParam2 != 0)` 门，尾 `CharDesc{Feature,Status}` 取 **`ActorMgr.Get(nParam2)`（= 克隆体）** 的 `GetFeature(this)`/`CharStatus`。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 无 `CM_*` 直接触发换形；换形是怪物 `Run` 的内部状态机。 |
| F2 脚本动作 | — 无命中。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 生产者所在面**：`ElfWarriorMonster.Run`（`:114-160`）由怪物 tick 驱动。 |
| F5 事件/魔法 | — 无命中（`RM_CHANGEFACE` 全树只此 1 处）。 |
| F6 对象广播 | **✔**（`SendRefMsg`）。 |
| F7 直接发帧 | — 无。 |
| F8 内容装载 | **✔ 可达性的内容前提**：`sql/mir2_data.sql:371` `神兽`(146, Race=113=ElfMonster)、`:372` `神兽1`(147, Race=114=ElfWarriorMon)——**全表 705 行里 race 114 只有这一条，且名字正好以 `'1'` 结尾**。 |

- **触发链（完整、内容可达）**：
  1. 玩家 `召唤神兽`（`magics` 表 id 30）得到 `神兽`(113, `ElfMonster`)；
  2. `ElfMonster.Run`（`ElfMonster.cs:43-83`；`MakeClone(Dragon1)` 在 `:72`）：`TargetCret != null` 或主人有目标 ⇒ `MakeClone(Config.Dragon1, this)`；`Config.Dragon1` 缺省 `神兽1`（`GameSvrConf.cs:1759`，部署 `Server.conf` 覆盖为 `神兽`——见 `crates/data/src/pet_slave.rs:431` 的断言「现网 Dragon1 覆盖初值 神兽1」）；
  3. 克隆出的 `神兽1`（114, `ElfWarriorMonster`）进入攻击形态 `AppearNow()`；
  4. `神兽1` **失去目标超过 60s**（`now - DigDownTick > 6*10*1000`）且名字以 `'1'` 结尾 ⇒ `MakeClone("神兽")` 成功 ⇒ `RM_DIGDOWN` + **`RM_CHANGEFACE`**（`:149`）⇒ 变回跟随形态。
- **Rust 现状（关键）**：
  - 分支已建模：`crates/world/src/monai.rs:3815-3880` 的 `elf_warrior_run` 完成 `boChangeFace` / 60s 门 / 名字去 `'1'` / 投递 `ScultureSpawnRequest{kind: MakeClone}`；
  - 克隆与收尾已建模：`crates/world/src/monai.rs:2898-3005` 的 `flush_sculture_spawns` 对 `MakeClone` 支执行 `apply_make_clone` + `elf_monster_appear_now` + **`send_ref_msg(RM_DIGDOWN)` + `send_ref_msg(RM_CHANGEFACE)`（`:2971-2988`，`RM_CHANGEFACE` 在 `:2983`）**；
  - 宿主已排水：`bins/game-svr/src/lib.rs:16648-16659`（`tick_world_head_with_specials` 用生产 `WorldSpecialSpawner` 调 `flush_sculture_spawns`），随后 `drain_monster_ref_msgs`（`:15345`）；
  - **缺**：`drain_monster_ref_msgs` 的 `match m.ident`（`:15409`）**没有 `RM_CHANGEFACE` 臂**，落到 `:15527` 的 `_ => continue` ⇒ 帧被丢弃。
- **判定**：**可直接接线**。改一个臂 + 克隆体 `Feature/Status` 的取值。
- **最小改动面**：`bins/game-svr/src/lib.rs:15409`（新增 `RM_CHANGEFACE =>` 臂，调 `smframe_ext::sm_changeface(m.actor_id, m.n_param2, feature, status)`）；`feature/status` 建议在排水时按 `world.entities.get(m.n_param2)` 取（oracle 处理器是**收件人下一 Run 才读**，排水时取比发帧时快照更接近），若要严格快照语义则给 `MonsterRefMsg` 加两个字段（会进 `crates/world` 写集，与 G11/M2 车道争用，故列为备选）。相位差按 T-89 族登记。
- **tier**：**L**（纯帧映射；无 RNG、无落库、无时序抽数）。

### 3.3 `SM_CLOSEDOOR(614)` / 3.7 `SM_OPENDOOR_OK(612)` — 不可达（引 T-65 + 独立复核）

- **编码器**：`smframe_ext.rs:171` `sm_closedoor(n_param1, n_param2)`、`:365` `sm_opendoor_ok(n_param1, n_param2)`。
- **oracle 发射点（各 1 命中）**：`GameSrv/Word/WorldServer.cs:1485`（`CloseDoor`）、`:1473`（`OpenDoor`），均经 `SendDoorStatus`（`:1488`）→ `SendRefMsg`。
- **独立复核（本卡重跑，不采信自述）**：
  - `door.Status.Opened = true` 全仓唯一写点 = `OpenDoor`（`:1471`）；
  - `OpenDoor` 唯一调用点 = `ClientOpenDoor`（`PlayObject.Operate.cs:363-376`），其首行 `if (Envir.GetDoor(nX, nY, ref door)) { return; }` —— **门存在即早退**（守卫写反，T-65）；
  - ⇒ `Opened` 恒 false ⇒ `CloseDoor` 的 `if (!door.Status.Opened) return;` 恒早退，`ProcessMapDoor`（`:1522`，5s 自动关门）也恒不进 ⇒ **两帧生产者均不可达**。
  - 注意同名干扰项：`CastleDoor.Open/Close`（`Monster/Monsters/CastleDoor.cs:46-77`）发的是 `RM_DIGUP/RM_DIGDOWN`，**不是** `RM_DOOROPEN/RM_DOORCLOSE`，且用的是另一个字段 `IsOpened`。
- **判定**：**不可达**，沿用 T-65（`whitelist.md:150`）与 T-176。**不做接线**。

### 3.4 `SM_HIDEEVENT(805)` / 3.11 `SM_SHOWEVENT(804)` — 需补上游（可达但退化）

- **编码器**：`smframe_ext.rs:201` `sm_hideevent(n_param1, w_param, n_param2, n_param3)`、`:523` `sm_showevent(...)`（后者尾 = `Encode(ShortMessage 4B)`）。
- **oracle 发射点（各 1 主 + 1 机器人镜像）**：
  - `src/M2Server/Player/PlayObject.cs:3278`（`RM_HIDEEVENT`）、`:3284`（`RM_SHOWEVENT`），均在同一段 `VisibleEvents` 差集循环内；
  - 镜像：`src/Modules/RobotSystem/Services/RobotPlayer.Base.cs:766/777`（机器人玩家，`@AddRobotPlay` 代码段被注释 ⇒ 现网不产生 —— F6 子例，不计入生产面）。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 无「直接发帧」的 `CM_*`；但 `CM_*` 造成的位移/进图会间接驱动下一次 `SearchViewRange`（见 F4）。 |
| F2 脚本动作 | — 无命中（脚本的 `MOBFIREBURN` 只经 F5 注册事件，不发帧）。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 唯一驱动面**：`WorldServer.cs:663-666` 玩家 `SearchTime=1000ms` 节流 → `PlayObject.SearchViewRange()`（`PlayObject.cs:3073`）。 |
| F5 事件/魔法 | **✔ 事件格子来源**：`MagicManager.cs:1074-1095`（火墙 `FireBurnEvent`，`base(...,true)` ⇒ `Visible=true` ⇒ `Envirnoment.AddMapEvent` 建 `CellType.Event` 格）、`:1179-1201`（圣言 `HolyCurtainEvent`）、`DigOutZombi.cs:59`（`ET_DIGOUTZOMBI`，`Visible=true`）、`FireMonster.cs:32-52`；`EventProcessor`（`GameSrv/Word/Threads/EventProcessor.cs`，20ms 定时）跑 `Run()/Close()`。 |
| F6 对象广播 | — 帧不上 `SendRefMsg`，走自身 `MsgQueue`（`SendMsg(Messages.RM_SHOWEVENT, …)` 只投递给自己）。 |
| F7 直接发帧 | — 无 `SendDefMessage(SM_SHOWEVENT/SM_HIDEEVENT)`。 |
| F8 内容装载 | **✘（反向证据）**：`MapManager.MakeSafePkZone`（`MapManager.cs:28-52`）本是安全区光环的注册点，但门条件 `string.IsNullOrEmpty(startPoint.MapName)` 成立时 `FindMap("")` 必为 null ⇒ **安全区事件从不注册**。⇒ 事件格子只来自 F5（火墙/圣言/掘尸），不来自地图装载。 |

- **关键退化（本卡新增的证据，见 §4）**：`MapEvent.Id` 全仓无赋值（`Modules/SystemModule/MagicEvent/MapEvent.cs:62` 是注释 `//Id = …`）⇒ `AddMapEvent` 存 `CellObjId = 0`（`Envirnoment.cs:237-244`）⇒ `CellObjectMgr` 的 key 0 先到先得（`CellObjectMgr.cs:10` `TryAdd`）+ `DeleteFromMap` 只在 `CellObjId > 0` 时删除（`Envirnoment.cs:652`）⇒ **只有「进程内第一个 MapEvent」能被 `Get<MapEvent>(0)` 解析**。
- **触发链**：`CM_WALK/CM_RUN/CM_TURN…`（或任一位移）→ `WorldServer` 1000ms 节流 → `SearchViewRange` → 逐格扫 `CellType.Event` → `Get<MapEvent>(CellObjId=0)` → `Visible ? UpdateVisibleEvent(nX,nY,e)` → 扫完做 `Hidden/Show` 差集 → `RM_SHOWEVENT` / `RM_HIDEEVENT`。
- **Rust 现状**：`crates/world/src/aoi.rs` 只建模 `VisibleActors`（`UpdateVisibleGay`）；**无** `CellType.Event`、**无** 观察者侧 `VisibleEvents`；`crates/world/src/world.rs:3882` 的 `run_map_events` 只跑火墙伤害/掘尸**模拟**，不产可见性事件。
- **判定**：**需先补上游生产者**（事件格 + 可见事件集 + 差集），且必须**连 `Id==0` 折叠语义一起复刻**（否则接线即制造 diff）。
- **最小改动面**：`crates/world/src/aoi.rs`（事件格 + `VisibleEvents` 差集）+ `crates/world/src/world.rs`（事件注册表暴露 `Id=0` 折叠口径）+ `bins/game-svr/src/lib.rs`（映射到 `sm_showevent`/`sm_hideevent`）。
- **tier**：**R**（AOI 时序 + 观察者差集序；且折叠语义必须真机核）。

### 3.5 `SM_ITEMSHOW(610)` — 不可达

- **编码器**：`smframe_ext.rs:271` `sm_itemshow(...)`（头 + `EncodeString(Msg)`）。
- **oracle 发射点（全树 4 命中，本卡比既有 T-179 多找到 1 个）**：
  1. `src/M2Server/Actor/BaseObject.cs:570` —— `DropItemDown`（`:529`）成功支；
  2. `src/M2Server/Actor/BaseObject.cs:752` —— `DropGoldDown`（`:735`）成功支；
  3. `src/M2Server/Player/PlayObject.cs:3264` —— `SearchViewRange` 的 `VisibleItems` `Show` 支（上游 `UpdateVisibleItem`，被 `:3155` 的 `if (mapItem.ItemId == 0) continue;` 前置挡住）；
  4. **`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:5703`** —— `ActionOfThrowitem`（`:5638`，`ExecutionCode 341`）的金币支（`:5756` 的物品支已整段注释）。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | **间接 ✔**：`CM_DROPITEM` → `ClientDropItem` → `DropItemDown`（生产者 ①）。 |
| F2 脚本动作 | **✔**：`THROWITEM`（`ExecutionCode 341`）→ 生产者 ④。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **间接 ✔**：`SearchViewRange` 的 `VisibleItems` 扫描（生产者 ③）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | **✔**（①②③ 均为 `SendRefMsg`）。 |
| F7 直接发帧 | — 无。 |
| F8 内容装载 | 反证：**全仓无 `MapItem.ItemId = …` 赋值点**（`Modules/SystemModule/Data/MapItem.cs:41` 是注释）。 |

- **不可达证明（独立复核）**：
  - `Envirnoment.AddItemToMap`（`Envirnoment.cs:88-92`）首行 `if (mapItem.ItemId == 0) return false;`；
  - `MapItem` 的构造（`MapItem.cs:39-42`）里 `ItemId` 赋值被注释 ⇒ 所有 `new MapItem()` 的 `ItemId` 恒 0；
  - ⇒ ①②④ 的 `if (AddItemToMap(...))` 恒 false ⇒ 三条分支都**不进**；
  - ③ 的前置 `if (mapItem.ItemId == 0) continue;`（`PlayObject.cs:3155`）恒真 ⇒ `UpdateVisibleItem` 从不被调用。
- **判定**：**不可达**。沿用 T-19/T-25/T-32/T-179 的结论，本卡补第 4 生产者并逐条复核。
- **注意（复用口径）**：Rust 侧 `ground.rs` 已 1:1 复刻「`item_id == 0 ⇒ 不落地`」（T-19 白名单）；本帧接线会**直接制造 diff**。

### 3.6 `SM_LAMPCHANGEDURA(655)` — 需补上游

- **编码器**：`smframe_ext.rs:291` `sm_lampchangedura(n_param1: i32)`（header-only，`Param = Dura`）。
- **oracle 发射点（1 函数内 2 处）**：`src/M2Server/Player/PlayObject.Message.cs:2265`（燃尽：`SendMsg(RM_LAMPCHANGEDURA, 0, 0, 0, 0)`）、`:2274`（磨损：`SendMsg(RM_LAMPCHANGEDURA, 0, UseItems[RighThand].Dura, 0, 0)`）。处理器 `:1842-1844`（`SendDefMessage(SM_LAMPCHANGEDURA, processMsg.nParam1, 0, 0, 0)`）。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — `CM_*` 无直接支；`CM_TAKEONITEM` 只改装备、不结算耐久。 |
| F2 脚本动作 | — 无命中。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 唯一驱动面**：`PlayObject.Run`（`:19`）的 `DecLightItemDrugTime` 周期块（`:257-262`，缺省 **500ms**，`GameSvrConf.cs:1398`）→ `UseLamp()`（`:2233`）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | — **不广播**：`SendMsg` 只投给本人（`MsgQueue`），无需 AOI。 |
| F7 直接发帧 | — 无。 |
| F8 内容装载 | **✔ 内容前提**：右手槽 `StdItem.SpecialPwr == 0` 才是「火把/蜡烛」；`Config.DecLampDura` 缺省 **true**（`GameSvrConf.cs:1504`）。 |

- **触发链**：玩家装备右手火把 → 每 500ms `UseLamp()` → `nDura = Dura-1`（`DecLampDura`）⇒ ①`nDura<=0`：`Dura=0` + `SendDelItems` + `Index=0` + `Light=GetMyLight()` + `RM_CHANGELIGHT` + **655（Param=0）** + `RecalcAbilitys()`；②`Round(旧/1000) != Round(新/1000)`：**655（Param=新 Dura）**。
- **Rust 现状**：`crates/world/src/entity.rs:667` 的 `use_items[UseItemSlot{dura,desc,…}]` 已在；`crates/world/src/world.rs:827` 已有 `dec_light_item_drug_time_ms`，且 `check_pk_status`（`:1503`）已用同一周期节流（**同一 oracle 分支的另一半已落地**）；`bins/game-svr/src/downlink.rs:53` 的 `my_light` 已实现 `GetMyLight`（含右手槽）。**缺**：`UseLamp` 本体（`SpecialPwr==0` 判定 + 耐久扣减 + 燃尽清槽）与其调用点；`DecLightItemDrugTime` 键**未解析**（`bins/game-svr/src/lib.rs:17024` 用缺省 500 并已登记）。
- **判定**：**需先补上游生产者**（一个纯函数 + 一个调用点）。
- **最小改动面**：`crates/world/src/`（新增 `use_lamp` 纯函数，按 oracle 逐行）+ `bins/game-svr/src/lib.rs`（Run 段的 `DecLightItemDrugTick` 分支调它并映射 `sm_lampchangedura`；`RM_CHANGELIGHT` 的 654 已有编码器 `sm_changelight` 但同样未接）。
- **tier**：**R**（耐久数值 + 落库 + 与 `CheckPkStatus` 同分支的时序）。

### 3.8 `SM_QUERYYBDEAL(23023)` / 3.9 `SM_QUERYYBSELL(23021)` — 需补上游

- **编码器**：`smframe_ext.rs:454` / `:475`（头 `(Recog, Param, Tag, Series)` + **尾原样透传** `processMsg.Msg`）。
- **oracle 发射点（各 1 命中）**：
  - `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2550`（`ActionOfQueryTrustDeal`，`:2468`）；
  - `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2443`（`ActionOfQuerySaleSell`，`:2363`）；
  - 注册：`:125-127`（`ExecutionCode.OpenYbDeal=342 / QueryYbSell=343 / QueryYbDeal=344`）；
  - 处理器：`Message.cs:2049-2056`（`MakeMessage(SM_QUERYYB…) + SendSocket(ClientMsg, processMsg.Msg)`）。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 直接支只有 `CM_SELLOFF*`（7 条），它们**不产** 23021/23023；只更新 `SellOffItemList/SaleDeal`。 |
| F2 脚本动作 | **✔ 唯一生产者面**（343/344）。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | — 无命中（无定时器触发查询）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | — 走 `SendMsg(npc, player)` 单投（不发 `SendRefMsg`）。 |
| F7 直接发帧 | — 无 `SendDefMessage(SM_QUERYYB*)`。 |
| F8 内容装载 | — 不产帧；**状态前提**：`PlayObject.SaleDeal` + `M2Share.SellOffItemList` + `SellOffInTime(0/1)`。 |

- **触发链**：NPC 脚本执行到 `QUERYYBSELL`/`QUERYYBDEAL` 动作 → `ActionOfQuerySaleSell/TrustDeal` → 门 `SaleDeal && SellOffInTime(code) && SellOffItemList.Count > 0` → 组 `ClientDealOffInfo{UseItems[9], DealChrName, BuyChrName, SellDateTime, SellGold, N}` → `EDCode.EncodeBuffer(...)` → `SendMsg(normNpc, RM_QUERYYB*, 0,0,0,0, sSendStr)` → 玩家下一 `Run` 出帧。
- **Rust 现状（分层）**：
  - **状态面已就绪**：`bins/game-svr/src/selloff.rs`（`SellOffBook` / `SellOffState{sale_deal}` / `sell_off_in_time`（`:1027`）/ 7 个 `CM_SELLOFF*`）+ 宿主会话同步（`lib.rs:5767-5788`）；
  - **脚本执行器已存在**：`crates/script/src/engine/actions.rs` + `dispatch.rs` 的 `IMPLEMENTED_EXECUTIONS`（70 条，含 `ActionOfOpenSaleDeal`，**不含** `ActionOfQuerySaleSell/QueryTrustDeal`）；
  - **脚本→帧的管道已存在**：`bins/game-svr/src/pickwire.rs:151` `script_frame`（已映射 `RM_GOLDCHANGED` / `RM_SCRIPT_GAMEGOLDCHANGED` / `RM_SYSMESSAGE` / `RM_SCRIPT_PLAYDICE`，其余 `_ => None`）；
  - **缺 ①**：两个动作体（含 `ClientDealOffInfo` 的逐字段装配与 `EDCode.EncodeBuffer`）；
  - **缺 ②**：尾是**字节缓冲**，而 `bins/game-svr/src/npc.rs:109-116` 的 `SendMsg.msg: Option<String>` 载不了任意字节 ⇒ 需要给脚本帧加字节尾通道（或让动作直接产 `Vec<u8>` 帧）。②是结构性改动，也是本族成本的主要来源。
- **判定**：**需先补上游生产者**（脚本动作体 + 字节尾通道）。
- **最小改动面**：`crates/script/src/engine/actions.rs`（343/344 动作体）+ `crates/script/src/engine/dispatch.rs`（登记）+ `bins/game-svr/src/npc.rs`（`SendMsg` 字节尾）+ `bins/game-svr/src/pickwire.rs`（两条映射）。
- **tier**：**R**（`EDCode.EncodeBuffer` 的逐字节尾 + 寄售列表状态）。

### 3.10 `SM_RECONNECT(802)` — 需补上游（另见 §5 专章）

- **编码器**：`smframe_ext.rs:496` `sm_reconnect(msg: &[u8])`（头全 0 + 尾 `EncodeString(Msg)`）。
- **oracle 发射点（全树 3 命中，另有 1 处绕过 RM 泵）**：
  1. `src/M2Server/Player/PlayObject.Base.cs:3417` —— `TransferPlanesServer()`：`SendMsg(RM_RECONNECTION, 0,0,0,0, serveraddr + '/' + gamePort)`；**唯一调用点 `GameSrv/AppService.cs:179` 被注释**；
  2. `src/GameSrv/Word/WorldServer.cs:1074` —— `SendChangeServer()`：**直接** `SendDefMessage(SM_RECONNECT, 0,0,0,0, "{ip}/{port}")`（**F7 绕过 RM 泵**，不经 `:1988` 的处理器）；调用点 `:366` / `:609`；
  3. `src/Modules/GameCommand/Commands/ReconnectionCommand.cs:38` —— GM `@Reconnection`：`SendMsg(PlayerActor, RM_RECONNECTION, …, sIPaddr + '/' + sPort)`。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 无 `CM_*` 支（`CM_SOFTCLOSE` 只置 `BoReconnection/BoSoftClose`，不发 802）。 |
| F2 脚本动作 | — 无命中。 |
| F3 GM 命令 | **✔**（③）但 `[Command(..., 10)]` + 现网 `Permission` 恒 0 ⇒ **被权限门挡死**（本仓 `gmcmd.rs` 设计件 §2 已证）。 |
| F4 宿主 tick | **✔**（② 的 `:366` 在 `UserEnter`、`:609` 在 SwitchData 回写轮询）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | — `TransferPlanesServer` 用 `SendMsg` 单投；`SendChangeServer` 用 `SendDefMessage` 单投。 |
| F7 直接发帧 | **✔**（② 的 `SendDefMessage(SM_RECONNECT)`）。 |
| F8 内容装载 | **✔**：`!servertable.txt`（`GameApp.cs:267`）→ `GetMultiServerAddrPort`（`GameShare.cs:57` / `M2Share.cs:246`）；`MapManager.GetMapOfServerIndex` 决定「是否跨服」。 |

- **触发链**：玩家进图请求的地图**不在本服** ⇒ `MapMgr.GetMapInfo(ServerIndex, MapName) == null`（`WorldServer.cs:352`）⇒ 回滚会话字段 + `SendSwitchData` + `SendChangeServer`（`:366`）⇒ `GetMultiServerAddrPort(ServerIndex)` 成功 ⇒ `BoReconnection = true` + `SendDefMessage(SM_RECONNECT, ip/port)`；随后客户端回 `SwitchDataOk` 触发 `:609` 的第二发（100ms 后）。
- **判定**：**需先补上游生产者**（G11 跨服搬家），**不是**「接线一行的活」。
- **tier**：**R**。

### 3.12 `SM_SKELETON(33)` — 需补上游

- **编码器**：`smframe.rs:2012` `sm_skeleton(actor_id, x, y, dir, char_desc)`（头 + 尾 `Encode(CharDesc 8B)`）。
- **oracle 发射点（全树 3 命中：2 生产 + 1 机器人镜像）**：
  1. `src/M2Server/Player/PlayObject.cs:3231` —— `SearchViewRange` 的 `Death && Skeleton` 支（`SendMsg(baseObject, RM_SKELETON, Dir, CurrX, CurrY, 0)`）；
  2. `src/M2Server/Player/PlayObject.Operate.cs:736` —— `ClientGetButchItem`（`:702`）屠宰成骨架（`baseObject.SendRefMsg(RM_SKELETON, Dir, CurrX, CurrY, 0, "")`）；
  3. 镜像 `RobotSystem/Services/RobotPlayer.Base.cs:661`（不计入生产面）；
  - 处理器 `Message.cs:1655-1661`：`MakeMessage(SM_SKELETON, processMsg.ActorId, nParam1, nParam2, wParam)` + 尾 `CharDesc{Feature = baseObject.GetFeature(this), Status = baseObject.CharStatus}`。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | **✔ 间接**：`CM_BUTCH(1007)`（`Message.cs:829`）→ `ClientGetButchItem`。 |
| F2 脚本动作 | — 无命中。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 间接**：AOI 出现帧路径（`SearchViewRange`）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | **✔**（`SendRefMsg`，第二条路径）。 |
| F7 直接发帧 | — 无。 |
| F8 内容装载 | — 不产帧；`Skeleton` 是实体状态字段。 |

- **`Skeleton` 的唯一 setter（关键）**：全仓 `Skeleton = true` 只在 `ClientGetButchItem`（`PlayObject.Operate.cs:734`），条件是 `BodyLeathery -= Random(16)+5` 后 `<= 0` 且 `Race ∈ [Animal, Monster)`（`:720-742`）。⇒ **SM_SKELETON 的可达性完全依赖屠宰效果体**。
- **Rust 现状**：`crates/world/src/entity.rs` 有 `ai.meat_quality`（`mongen.rs:434-451` 生产），**无** `skeleton`、**无** `body_leathery`；`bins/game-svr/src/inbound.rs` 的 `client_butch` 只做**决策**（`target_butcherable` 由调用方喂），**效果体（两次 RNG + MeatQuality/BodyLeathery/Skeleton）未建模**（`inbound.rs:49` 已登记）；`bins/game-svr/src/downlink.rs:543-556` 的 `appear_frames` 只有 `dead ⇒ SM_DEATH` 一支，明确登记 `SM_SKELETON` 为 `deferred`。
- **判定**：**需先补上游生产者**（`BodyLeathery`/`Skeleton` 字段 + 屠宰效果体）。
- **最小改动面**：`crates/world/src/`（`Entity` 加 `skeleton`/`body_leathery`；屠宰效果体按 `Operate.cs:720-742`）+ `bins/game-svr/src/inbound.rs`（效果体接线）+ `bins/game-svr/src/downlink.rs:550`（`appear_frames` 加 `skeleton` 支）+ `bins/game-svr/src/lib.rs`（`RM_SKELETON` 映射）。
- **tier**：**R**（两次 `Random` 抽数顺序 + `MeatQuality` 数值）。

---

## 4. 事件族专章：`MapEvent.Id == 0` 的折叠语义

这不是「顺带发现」，它决定 `SM_SHOWEVENT`/`SM_HIDEEVENT` 的**可达性与帧内容**，必须写进接线判据。

**证据链（逐行）**

1. `src/Modules/SystemModule/MagicEvent/MapEvent.cs:62`：`//Id = M2Share.ActorMgr.GetNextIdentity();` —— **被注释**；`public readonly int Id;` 再无赋值点 ⇒ **`Id` 恒 0**。
2. `Envirnoment.AddMapEvent`（`Envirnoment.cs:220-245`）：`CellObject{CellType = CellType.Event, CellObjId = mapEvent.Id /*0*/}` + `M2Share.CellObjectMgr.Add(0, mapEvent)`。
3. `CellObjectMgr.Add`（`CellObjectMgr.cs:10-16`）用 `TryAdd` ⇒ 只有**第一个** MapEvent 占住 key 0，后续全部静默失败（失败日志也被注释）。
4. `Envirnoment.DeleteFromMap`（`Envirnoment.cs:635-667`）只处理 `cellObject.CellObjId > 0` ⇒ 事件格**永不移除**；`CellObjectMgr.Remove(0)` 因此**永不发生** ⇒ key 0 永久指向进程内第一个 MapEvent。
5. `PlayObject.SearchViewRange`（`PlayObject.cs:3185-3194`）：`CellType.Event ⇒ Get<MapEvent>(0)`；`null ⇒ continue`；`Visible ⇒ UpdateVisibleEvent(nX, nY, e)`（**注意：把「格子坐标」写进那个「第一个事件」对象**）。
6. `UpdateVisibleEvent`（`:5465-5487`）：已在列表 ⇒ `Invisible`（**不出帧**）；不在 ⇒ `Show` + 加入列表。
7. 差集出口（`:3271-3287`）：`Hidden ⇒ RM_HIDEEVENT`、`Show ⇒ RM_SHOWEVENT`。

**由此得到的四条可判结论**

- (a) **可达**：只要「进程内第一个 MapEvent」还 `Visible`（例如第一发火墙的存活期内），任何玩家首次走进任意 `CellType.Event` 格都会收到**一条** `SM_SHOWEVENT`；离开后收到**一条** `SM_HIDEEVENT`。
- (b) **退化**：所有事件格共用同一对象 ⇒ 同一玩家在任一事件格上只会 `Show` 一次；`EventType` 恒为「第一个事件」的类型；`nX/nY` 是**当前格子**坐标而非事件注册坐标。
- (c) **窗口极短**：第一个 MapEvent 一旦 `Close()`（`MapEvent.cs:109-123`，含 `Visible=false`），`Visible` 门关闭 ⇒ 此后**再也不出 804/805**（后续事件进不了 key 0）。
- (d) **Rust 必须照抄**：接线时若按「每个事件一个 cell id + 正常差集」实现，**会在第二次进事件格时多出 804**（oracle 不发）⇒ 直接 diff。⇒ 建议在 `crates/world` 用一个 `first_event_only` 的口径显式建模，并在 `whitelist.md` 登记该 oracle 缺陷。

> 参考：同类「id 恒 0」家族已有两条 —— `MapItem.ItemId`（T-19/T-25/T-32）与 `MapEvent.Id`（本条）。二者机制同源（`ActorMgr.GetNextIdentity()` 调用点被注释），建议合并成一条「**GetNextIdentity 注释家族**」总条目（见 §10 的 T-191）。

---

## 5. `SM_RECONNECT` 专项核查（卡面特别要求）

卡面问三件事：① 是否与登录/断线重连链路有关；② 本仓是否已有等价路径；③ 结论。

**① 语义澄清：它不是「客户端断线重连」，而是「服务端让客户端换一个网关地址」**

- oracle 侧 `SM_RECONNECT(802)` 的载荷 = `"{ip}/{port}"`（`PlayObject.Base.cs:3417`、`WorldServer.cs:1074`、`ReconnectionCommand.cs:38`）。客户端收到后**主动断开并用新 IP/端口重连**（客户端冻结，不改）。
- 它**不参与**「掉线保活/会话恢复」：那条链在 oracle 里是 `BoSoftClose`/`BoReconnection` + 存档（`PlayObject.Message.cs:380`），Rust 侧已实现（`lib.rs:4210-4260` 的 `CM_SOFTCLOSE`、`guild_runtime.rs:224` 的 `!bo_reconnection && bo_soft_close` 路径 A）。两者只共享 `BoReconnection` 这个**标志**，不共享帧。

**② 本仓等价路径核查**

| 检查项 | 结果 |
|---|---|
| `!servertable.txt` 装载 | **无**（`grep -rni servertable` 在 `mir2-rs/` 零命中；oracle 在 `GameApp.cs:267`） |
| `GetMultiServerAddrPort` | **无**（Rust 无多服地址表） |
| `SendChangeServer` / `SendSwitchData` / `SwitchData` 回写 | **无**（`grep SwitchData` 零命中；`chat.rs:35` 已把 `BanShout` 的第二写点标为「跨服 `SwitchData`（未移植）」） |
| `TransferPlanesServer` | 无（且 oracle 唯一调用点已注释） |
| 多服地图归属 | **部分有**：`bins/game-svr/src/castle.rs:116-130` 的 `server_index` 只用于城堡归属门（降级为单服 `0`） |
| GM `@Reconnection` | **表项存在、命令体未实现**：`bins/game-svr/src/gmcmd.rs:2500` `CommandDef{ name:"Reconnection", perm_min:10, params:2, imp: Imp::Unimplemented }` |
| 等价出帧 | 无（`grep sm_reconnect` 在 `bins/**` 只有编码器定义与自测） |

⇒ **本仓没有任何等价路径**；`SM_RECONNECT` 既无生产点也无替代链路。

**③ 判定与建议**

- **判定**：**需先补上游生产者**（G11 跨服搬家），**不接线**。
- 为什么不「先接 GM 那条」：`@Reconnection` 需要 `Permission >= 10`，而本仓按设计件 §2 1:1 复刻了「`GetHumPermission` 被注释 ⇒ 现网 `Permission` 恒 0」⇒ 生产路径永远停在 `权限不够!!!`。接了也只能在**测试**里用 `permission=10` 触发，对真机对拍**零收益**，却会引入一条 oracle 现网不存在的可达面。
  - 若 owner 明确要求「覆盖所有构造可达面」，那这条应作为**测试专用**接线（tier=L，构造用例 + 声明「生产不可达」），并**不得**在报告里表述为「已支持跨服」。
- **真生产面的判据（将来 G11 卡的入口）**：`MapMgr.GetMapInfo(ServerIndex, MapName) == null` ⇒ `SendSwitchData` + `SendChangeServer`；要求 `!servertable.txt` 里该 `ServerIndex` 有 `IP/Port`。

---

## 6. 两个特殊项的单独说明

### 6.1 `SM_CHANGEFACE`：历史结论（T-175「上游未移植」）已过时 —— 「先有功能才有帧」，而**功能已有**

- 卡面要求「不得混进可直接接线清单」。**本卡据实测证据把它列进可直接接线**，理由如下（可复核，不是口味问题）：
  1. **世界层已发同 ident 的 RM**：`crates/world/src/monai.rs:2980-2988` `send_ref_msg(…, RM_CHANGEFACE, 0, req.owner_id, child, 0)` —— 与 oracle `ElfWarriorMonster.cs:149` 的实参逐位对应（ident / wParam=0 / nParam1=自己 / nParam2=克隆体）；
  2. **分支条件已 1:1 建模**：`elf_warrior_run`（`monai.rs:3815-3880`）复刻了 `boChangeFace` 三条件、`> 60s` 严格大于门、名字末位 `'1'` 去尾、`else { DigDownTick = now }` 的作用域陷阱，且有改坏必红用例（`mb7_elf_warrior_dig_down_tick_not_refreshed_when_change_face_true`）；
  3. **克隆已建模**：`flush_sculture_spawns` 的 `MakeClone` 支执行 `apply_make_clone`（字段复制 6 步）+ `elf_monster_appear_now` + `Master=null` + `KickException`；
  4. **内容可达**：`sql/mir2_data.sql:372` 的 `神兽1`（race **114 = ElfWarriorMon**，全表唯一）与 `:371` 的 `神兽`（race 113）都在；`Config.Dragon1` 由 `Server.conf` 覆盖（`crates/data/src/pet_slave.rs:431` 断言现网值 `神兽`）；
  5. **唯一缺口**：宿主 `drain_monster_ref_msgs`（`lib.rs:15409`）没有 `RM_CHANGEFACE` 臂。
- **因此**：本帧「先有功能才有帧」的功能**已经存在**；剩下的只是帧映射。**若审查者坚持以 T-175 的旧结论为准，请以此处 5 条 file:line 反驳或订正 T-175** —— 这正是本卡存在的意义。
- **残留不确定**：克隆体的 `Feature/Status` 取「排水时」还是「发帧时快照」—— oracle 在**收件人下一 Run** 读 `ActorMgr.Get(nParam2)`，故排水时读更接近；若实测有差，按 T-89 族登记相位差，不属于「接线代价」问题。

### 6.2 `SM_SKELETON`：确实「先有功能才有帧」，功能未实现

- `Skeleton` 是 `BaseObject` 的**状态字段**，oracle 全仓唯一 setter = `ClientGetButchItem`（`PlayObject.Operate.cs:734`），由 `BodyLeathery`（屠宰累积伤害）归零触发。
- Rust 侧：`Entity` 无 `skeleton`、无 `body_leathery`；`inbound::client_butch` 只做**决策**（消费 `target_butcherable` 入参），效果体（`Random(16)+5` / `Random(201)+100` / `MeatQuality` / `BodyLeathery` / `Skeleton`）**整段未移植**（`inbound.rs:49` 自述）。
- ⇒ **先有屠宰效果体，才有 `SM_SKELETON`**。本帧**不得**当作「可直接接线」，且**不得**用「已死就发 33」的近似替代（oracle 是 `Death && !Skeleton ⇒ 32`，`Death && Skeleton ⇒ 33`；近似会直接改变出现帧的 ident）。
- 与 `SM_CHANGEFACE` 的**区别在于证据**：CHANGEFACE 的分支 + 克隆 + 内容都在（缺一臂映射）；SKELETON 的生产链（效果体 + 字段）整体不在。二者不能一起归类。

---

## 7. 优先级表（接线代价 × 可达性）

代价口径：**S** = 单文件、`bins` 内、无状态语义；**M** = 一个纯函数/一次结构扩展 + 一处映射 + 真实窗口；**L** = 跨 crate 契约或需新增字节通道；**XL** = 整子系统。

| 名次 | 帧 | 可达性 | 现状缺口 | 代价 | 综合 |
|---|---|---|---|---|---|
| 1 | `SM_CHANGEFACE` 1104 | **高**（内容表 `神兽1`/`神兽` + 召唤链 + 60s 门；世界层已发 RM） | 宿主 match 缺 1 臂 | **S** | **立刻可做**（tier=L） |
| 2 | `SM_LAMPCHANGEDURA` 655 | **高**（装备右手火把即每 500ms 结算） | `UseLamp` 纯函数 + 调用点 | **M** | 单函数、自投递、无 AOI |
| 3 | `SM_SHOWEVENT` 804 / `SM_HIDEEVENT` 805 | **中**（火墙/圣言/掘尸建事件格；但**仅进程首个事件**有效） | 事件格 + `VisibleEvents` + `Id==0` 折叠 | **M**（世界侧） | 语义要照抄缺陷，判据好写 |
| 4 | `SM_BREAKWEAPON` 1102 | **中**（需先经升级 NPC 得到 `Desc[10]`，再打一次） | `CheckWeaponUpgrade(Status)` + 攻击泵 | **M** | 同批还牵扯 203/52/752 ⇒ 扩面风险 |
| 5 | `SM_SKELETON` 33 | **中**（屠宰同一只动物多次） | `BodyLeathery`/`Skeleton` + 屠宰效果体 | **M + R RNG** | 抽数序要做 trace |
| 6 | `SM_QUERYYBSELL` 23021 / `SM_QUERYYBDEAL` 23023 | **中低**（需脚本 `QUERYYB*` 动作 + 已开通元宝） | 2 个动作体 + `ClientDealOffInfo` 编码 + **字节尾通道** | **L** | 受 `SendMsg.msg: String` 限制 |
| 7 | `SM_RECONNECT` 802 | **低/无**（G11 未移植；GM 面被 perm 0 锁死） | 多服路由 + 会话迁移 | **XL** | 建议挂 G11，不单开 |
| — | `SM_CLOSEDOOR` 614 | **无**（T-65） | — | — | 登记，不动 |
| — | `SM_OPENDOOR_OK` 612 | **无**（T-65） | — | — | 登记，不动 |
| — | `SM_ITEMSHOW` 610 | **无**（`ItemId` 恒 0） | — | — | 登记，不动 |

## 8. 建议分批

| 批 | 内容 | tier | 窗口 | 备注 |
|---|---|---|---|---|
| **B1** | `SM_CHANGEFACE`（顺带确认 `SM_DIGDOWN` 已在映射内） | L | 不占 | 单臂改动；抽样冒烟 = 构造一发 `神兽1` → 60s 分裂 |
| **B2（耐久族）** | `SM_LAMPCHANGEDURA` + `SM_BREAKWEAPON` | R | **同一窗口** | 两帧同属「耐久扣减 ⇒ 落库 ⇒ 帧」；一次窗口覆盖；顺带处理 `RM_CHANGELIGHT(654)`/`SM_UPDATEITEM(203)` 的关系（与 T-79/T-95 对齐，**不扩范围**） |
| **B3（事件族）** | `SM_SHOWEVENT` + `SM_HIDEEVENT` | R | 需窗口 | 判据必须钉住 §4 的折叠：多事件场景下「第二次进格不出 804」 |
| **B4（屠宰族）** | `SM_SKELETON`（含 `CM_BUTCH` 效果体） | R | 需窗口 | RNG 抽数序（2 次）与 `MeatQuality` 数值；与 B2 可共窗口但**建议独立卡**（写集不同） |
| **B5（寄售族）** | `SM_QUERYYBSELL` + `SM_QUERYYBDEAL` | R | 需窗口 | 前置：脚本帧的**字节尾通道**（建议先单开一条基础卡） |
| **B6** | `SM_RECONNECT` | — | — | **不做**；挂 G11 跨服搬家卡，届时一并做 |

---

## 9. 与既有台账的差异（建议订正）

| 条目 | 既有表述 | 本卡实测 | 建议 |
|---|---|---|---|
| **T-175**（`SM_CHANGEFACE`） | 「上游 `ElfWarrior` 分裂支未移植」 | 分裂支/克隆/收尾/`RM_CHANGEFACE` **全部已建模**（`monai.rs:2898-3005`、`:3815-3880`），内容 `神兽1`(114) 在表 | 前提作废 ⇒ 改判「宿主映射缺失，可直接接线」 |
| **T-178**（`SM_SHOWEVENT/HIDEEVENT`） | 「`MapEvent` 可见性未建模」 | 成立，但**必须补充**：`MapEvent.Id` 恒 0 ⇒ 只有进程首个事件可达，且事件格永不移除（§4） | 增补折叠语义为接线**硬判据** |
| **T-179**（`SM_ITEMSHOW`） | 列 3 个生产者（`BaseObject.cs:570/752`、`PlayObject.cs:3264`） | **漏第 4 个**：`ExecutionProcessingSys.cs:5703`（`ActionOfThrowitem` 金币支） | 补生产者清单；结论不变（`AddItemToMap` 恒 false） |
| **T-182**（`SM_RECONNECT`） | 「`TransferPlanesServer`/`SendChangeServer` 未移植」 | 漏三点：①`TransferPlanesServer` 的**唯一调用点已注释**（`AppService.cs:179`）；②另有 GM `@Reconnection` 生产者（perm 10，现网 perm 0 锁死）；③`SendChangeServer` 走 `SendDefMessage` **绕过 RM 泵** | 三处补充；结论不变（不接线） |
| **T-65 / T-176**（门） | 门链不可达 | 独立复核一致（`Opened` 唯一置 true 点不可达） | 沿用，不新增 |
| **T-79⑤**（脚本帧不可达） | 「宿主 `NpcPlayer.frames` 没有通用 drain/映射」 | **已不成立**：`emit_script_frames`（`lib.rs:6210`）+ `pickwire::script_frame`（`:151`）已存在，已映射 4 个 ident | YB 族的真实缺口改为「动作体 + 字节尾通道」 |

---

## 10. 建议条目（交后续卡落地，本卡**不改** `whitelist.md`）

> 编号沿用 `whitelist.md` 现有最大值递增（当前最大 T-183）。若与并行卡撞号，以「标题 + file:line」为准重新编号。

- **T-184 `SM_CHANGEFACE` 可直接接线**：生产点 `crates/world/src/monai.rs:2980-2988`（世界层已发 `RM_CHANGEFACE`）；缺口 = `bins/game-svr/src/lib.rs:15409` 的 match 缺臂。判据：构造 `神兽1`(race 114) 断目标 60s ⇒ 附近玩家收到 1 条 `SM_CHANGEFACE`（含克隆体 `CharDesc`），且同 tick 的 `SM_DIGDOWN` 在其前。**订正 T-175 的前提**。
- **T-185 `SM_LAMPCHANGEDURA` 上游 `UseLamp`**：`PlayObject.Message.cs:2233`（emit `:2265`/`:2274`）；`DecLampDura` 缺省 true、`DecLightItemDrugTime` 缺省 500ms（本仓未解析，`lib.rs:17024` 已登记）。tier=R。
- **T-186 `SM_BREAKWEAPON` 上游 `CheckWeaponUpgrade`**：`PlayObject.cs:4110`/`:4146`，调用点 `PlayObject.Attack.cs:27-31`；`Desc[10]` 生产面 = `Merchant.cs:682-736`（Rust `merchant.rs:1218` 已有写点，取回后是**死数据**）。tier=R。
- **T-187 事件族 + `MapEvent.Id` 恒 0 折叠**：§4 全链；接线必须以「进程首个事件」口径建模，否则多出 804。tier=R。
- **T-188 `SM_SKELETON` 上游屠宰效果体**：`Operate.cs:720-742`（`Random(16)+5`/`Random(201)+100`/`BodyLeathery`/`Skeleton`）；另需 AOI 支 `PlayObject.cs:3231` 与 `RM_SKELETON` 支 `Operate.cs:736`。tier=R。
- **T-189 寄售族脚本动作 + 字节尾通道**：`ExecutionProcessingSys.cs:2443/2550`；阻塞点 = `bins/game-svr/src/npc.rs:109` 的 `SendMsg.msg: Option<String>` 载不了 `EDCode.EncodeBuffer` 的任意字节。tier=R。
- **T-190 `SM_RECONNECT` 三面穷举结论**：①`TransferPlanesServer` 调用点注释；②`SendChangeServer` 走 `SendDefMessage`（绕过 RM 泵）且依赖 `!servertable.txt`；③GM `@Reconnection` perm 10 被现网 perm 0 锁死 ⇒ 现网两侧皆不出帧。挂 G11。
- **T-191（合并建议）「`GetNextIdentity` 注释家族」**：`MapItem.ItemId`（T-19/T-25/T-32）与 `MapEvent.Id`（本卡）同源（`ActorMgr.GetNextIdentity()` 调用点被注释）⇒ 建议合并成一条总条目，附「任何依赖这两个 id 的帧」白名单，避免逐帧重复发现。

---

## 11. 复现命令（证据四件套之一）

```sh
# ① 派发对账门（12 孤儿 + SM_DONATE_FAIL 的权威口径）
cd mir2-rs && python tests/parity/coverage/recon_gates.py dispatch --repo ..

# ② 全树常量扫描（面穷举的超集证据；逐帧各一次）
cd <repo>; for c in RM_BREAKWEAPON RM_CHANGEFACE RM_DOORCLOSE RM_HIDEEVENT RM_ITEMSHOW \
    RM_LAMPCHANGEDURA RM_DOOROPEN RM_QUERYYBDEAL RM_QUERYYBSELL RM_RECONNECTION \
    RM_SHOWEVENT RM_SKELETON; do echo "=== $c ==="; grep -rn "$c" src/ --include=*.cs; done

# ③ 不可达证明（id 恒 0 家族）
grep -rn "\.ItemId *=" src/ --include=*.cs          # 只有比较；唯一赋值点被注释（MapItem.cs:41）
grep -n  "//Id = M2Share.ActorMgr.GetNextIdentity" src/Modules/SystemModule/MagicEvent/MapEvent.cs
sed -n '88,92p' src/M2Server/Maps/Envirnoment.cs    # AddItemToMap 首行 ItemId==0 门

# ④ 门链（T-65）独立复核
grep -rn "SendDoorStatus\|Opened = true" src/ --include=*.cs
sed -n '363,377p' src/M2Server/Player/PlayObject.Operate.cs   # ClientOpenDoor 守卫写反

# ⑤ 本仓侧缺口的定位
cd mir2-rs
grep -n "match m.ident" bins/game-svr/src/lib.rs               # 15409
grep -n "_ => continue," bins/game-svr/src/lib.rs              # 15527（RM_CHANGEFACE 落这里）
grep -rn "RM_CHANGEFACE" crates/world/src/monai.rs             # 世界层生产点（2981）
grep -n "fn script_frame" bins/game-svr/src/pickwire.rs        # 脚本→帧映射（YB 族缺口）
grep -n "Reconnection" bins/game-svr/src/gmcmd.rs              # 2500 Imp::Unimplemented
grep -rni "servertable\|SwitchData" bins/ crates/              # 零命中（跨服未移植）
grep -n "fn elf_warrior_run" -A 60 crates/world/src/monai.rs   # ELF 分裂支已建模
grep -n "MySqlDB\|monsters" -l sql/mir2_data.sql; grep -n "'神兽1'" sql/mir2_data.sql
```

---

## 12. 残留物清单

- 本卡只新增 `docs/orphan-encoder-trigger-audit-20261023.md`（本文件）；**未改**任何代码 / 编码器 / 规则表 / `whitelist.md`。
- worktree：`E:/tmp/wt-worker-32`（分支 `card/orphan-audit`），收尾由合并方按 DoD 清理。
- 未起任何服务端/客户端/假人进程；未占用 `E:\MirServer` 端口。
- 临时文件：无（所有 grep/sed 输出仅在会话内，未落盘）。
