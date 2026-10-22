# 12 个孤儿出站编码器的触发源审计（oracle 发射点 × 触发链 × 可达性）

- 卡：`openmir2-orphan-encoder-trigger-audit`（issue `ca6dc0775eaf9271d04b9645077edb55a3b85e86`，tier=L）
- 作者：`openmir2-worker-32`（worktree `wt-worker-32` / 分支 `card/orphan-audit`）
- 基线：master `dbd03db97`；oracle = 本仓 `src/**`（C# 参照实现），行号均为该 commit 实测；部署件 = `E:/MirServer/M2GameSvr/**`
- 写集：**仅本文件**。不改代码 / 编码器 / 规则表 / `whitelist.md`（结论以「建议条目」形式交后续卡落地）
- **v2（按 review `fd374560` 返工）**：C-1（`SM_CHANGEFACE` 三态判错）、C-2（事件族未穷举 `MapEvent` 家族）、I-1（编号撞号）、I-2（`SwitchData` 假证据）、I-3（655 字段写反）全部订正；M-1..M-4 一并修。

> **v3（按 review `2b3f2c75`）**：I-6 把「复活条件」由「改内容 + 接一臂 = S」改写为**三步、代价 M**（Rust 世界层的克隆名是硬编码常量 `DRAGON1_NAME`，只改内容会先造成 oracle/Rust 行为差异）；M-5 在 §4.3 相位表补「门」这一段（`MapDoor.DoorId` 同样恒 0，`LoadMapDoor` 在 `StartUp` 前就占住 key 0）；M-6 订正 P2 的注释（monsters 表无 `神兽10` ⇒ 该 MonGen 行是死条目）。

> **v1 的自纠**：v1 把 `SM_CHANGEFACE` 判成「可直接接线」，理由是「世界层已发 `RM_CHANGEFACE` + 宿主缺一臂」。代码观察本身没错，但**把「代码缺口」当成了「现网可出帧」**：现网部署 `Dragon1=神兽`（race 113），ELF 分裂支永远造不出 race-114，所以该帧在**两侧现网都不执行**。v1 的 §3.2 第 3 步与同段第 2 步自相矛盾，且错引 `pet_slave.rs:431` 作正向证据（它恰恰是反证）。v2 已改判为「不可达（部署级）」并撤回 v1 的优先级第 1 名与 B1 批。

---

## 0. 结论摘要

| 三态 | 帧数 | 帧 |
|---|---|---|
| **可直接接线**（上游生产者已在，只缺宿主映射/发射） | **0** | — |
| **需先补上游生产者**（生产链本身未建模） | **8** | `SM_BREAKWEAPON(1102)`、`SM_HIDEEVENT(805)`、`SM_LAMPCHANGEDURA(655)`、`SM_QUERYYBDEAL(23023)`、`SM_QUERYYBSELL(23021)`、`SM_RECONNECT(802)`、`SM_SHOWEVENT(804)`、`SM_SKELETON(33)` |
| **不可达（应登记、不接线）** | **4** | `SM_CLOSEDOOR(614)`、`SM_ITEMSHOW(610)`、`SM_OPENDOOR_OK(612)`（**oracle 结构性**）；`SM_CHANGEFACE(1104)`（**现网部署级**） |

合计 12（`SM_DONATE_FAIL` 归卡 `openmir2-m3-guild-state`，不在本卡范围）。

「不可达」再分两类，理由不同、复活条件不同：

| 子类 | 帧 | 不可达的根因 | 复活条件 |
|---|---|---|---|
| oracle 结构性 | `SM_CLOSEDOOR`/`SM_OPENDOOR_OK`（T-65）、`SM_ITEMSHOW`（T-19/T-25/T-32） | 冻结的 C# 代码里生产者恒不执行（守卫写反 / id 恒 0） | 除非改 oracle 源码（等于放弃逐行等价基准），否则永久 |
| **现网部署级** | `SM_CHANGEFACE` | 现网 `Dragon1=神兽`（race 113）⇒ `ElfMonster.Run` 的克隆永远是 113，`is ElfWarriorMonster` 恒假 ⇒ 全仓唯一的 `new ElfWarriorMonster()`（`WorldServer.MonGen.cs:666`）永不执行 | **三步、代价 M**（不是 S）：① 内容面 `Dragon1=神兽1`（须 owner 裁决）；② `crates/world` 把硬编码 `DRAGON1_NAME`（`monai.rs:3632`）改为随配置（`bins` 注入 `PetSlaveConfig.dragon1`）；③ 补 `bins/game-svr/src/lib.rs:15409` 臂。**只做 ①+③ ⇒ oracle 出 114、Rust 仍出 113，先出现行为差异且 Rust 永不出 1104**（见 §3.2） |

**四条与既有台账不一致的实测结论**（详见 §9）：

1. **`SM_CHANGEFACE`：世界层已建模、现网内容不可达**。`crates/world/src/monai.rs:2893` `flush_sculture_spawns` 的 `MakeClone` 支确已发出 `RM_CHANGEFACE`（`:2980-2988`，ident 在 `:2983`），宿主 `drain_monster_ref_msgs`（`lib.rs:15409`）确无该臂——但**现网造不出 race-114**（部署 `Dragon1=神兽`、Rust `DRAGON1_NAME="神兽"`、Envir 全树无 `神兽1` 精确 token、GM 面 perm 0 锁死）。⇒ T-175 的**结论**（不接线）成立，但其**前提表述**（「上游未移植」）应改为「世界层已建模、现网内容不可达」。
2. **`SM_SHOWEVENT`/`SM_HIDEEVENT` 可达，但语义退化，且需先把 `MapEvent` 家族穷举干净**：`MapEvent.Id` 全仓无赋值点（`MapEvent.cs:62` 被注释）⇒ 所有事件格共用 `CellObjId = 0`；`StoneMineEvent` 因 `AddToMapMineEvent` 的 `cellSuccess && !Valid` **逻辑不可满足**而**从不注册**（只对每个格子执行 `CellObjectMgr.Remove(0)`）⇒ 启动后 key 0 为空、由**第一发火墙/圣言/掘尸**占住。逐条推导见 §4。
3. **`SM_RECONNECT` 有第三个生产者（GM 命令 `@Reconnection`）**，但现网 `Permission` 恒 0 ⇒ perm 10 的命令在生产路径上被权限门挡死；跨服搬家链（`SendChangeServer`/`TransferPlanesServer`）才是真生产面，且 `TransferPlanesServer` 的**唯一调用点已被注释**（`AppService.cs:179`）。本仓**已有** `SwitchDataState`/`process_switch_data` 脚手架（`lib.rs:14731/14766`，生产恒 no-op）——G11 不是「从零补」。详见 §5。
4. **`SM_LAMPCHANGEDURA` 的线上字段是 `Recog=Dura`（不是 `Param`）**：处理器 `SendDefMessage(SM_LAMPCHANGEDURA, processMsg.nParam1, …)`（`PlayObject.Message.cs:1843`），编码器 `sm_lampchangedura` 已正确，v1 的正文写反，v2 订正（见 §3.6）。

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
| F8 内容/配置装载 | 地图对象注册（门/事件/矿物）、`*.conf`、`Envir/*.txt`、怪物表 | `src/GameSrv/Maps/**`、`src/M2Server/Maps/Envirnoment.cs`、`E:/MirServer/M2GameSvr/**` |

**证据命令**（可复现，见 §11）：对 12 个 `RM_*` 常量各做一次全树 grep；命中总数与归类见 §2 主表。C# 的 RM 泵只有 `SendMsg`/`SendRefMsg`/`SendDefMessage`/`MakeMessage` 四种入口，全部以 `Messages.RM_*` 字面量出现 ⇒ 全树 grep 是任何 F 面的超集。

### 1.2 三态定义（本卡口径）

- **可直接接线**：oracle 生产链在 Rust 侧**已经有等价生产点**（世界层已发同 ident 的 RM，或可直接调用的纯函数），**且该链在现网可达**，只差「RM → SM 的帧映射」或「函数调用点」。改动面 = `bins/game-svr/**`。
- **需先补上游生产者**：oracle 的触发链在 Rust 侧缺至少一环（状态字段 / 结算函数 / 子系统）。
- **不可达**：oracle 侧（或现网部署内容侧）因结构性原因永远不发帧 ⇒ 接线即制造 diff 或零收益。必须留痕登记，并写清复活条件。

> **v1 的教训（本卡自定的一条判据）**：「代码缺口」≠「现网可出帧」。判「可直接接线」必须同时满足：① 生产链在 Rust 存在；② **该链在现网（部署配置 + 冻结内容）能被执行**；③ 只差帧映射。缺 ② 就退到「不可达（部署级）」。

---

## 2. 台账主表（逐帧一行）

| # | 帧 | 常量 | 编码器（本仓） | oracle 发射点（逐点） | 触发链 | 三态 | 最小改动面（若可达） | tier |
|---|---|---|---|---|---|---|---|---|
| 1 | `SM_BREAKWEAPON` | 1102 | `smframe_ext.rs:101` `sm_breakweapon` | `PlayObject.cs:4121`（`CheckWeaponUpgrade`，`SendRefMsg`） | `AttackDir`（`PlayObject.Attack.cs:27-31`，`attackTarget != null` + 武器 `Desc[10]>0`）→ `CheckWeaponUpgrade`（`:4110`）→ `CheckWeaponUpgradeStatus`（`:4146`，`Desc[10]==1 ⇒ Index=0`）→ `SendRefMsg(RM_BREAKWEAPON)` → `Message.cs:2015` | 需补上游（结算函数未建模） | `crates/world`（升级结算）+ `bins/game-svr`（攻击泵发帧） | R |
| 2 | `SM_CHANGEFACE` | 1104 | `smframe_ext.rs:115` `sm_changeface` | `Monster/Monsters/ElfWarriorMonster.cs:149`（`SendRefMsg`） | `ElfWarriorMonster.Run`（`:114-160`）：`boChangeFace && now-DigDownTick>60s && 名末 '1'` → `MakeClone("神兽")` → `RM_DIGDOWN`+`RM_CHANGEFACE` → `Message.cs:2021`；**但 `ElfWarriorMonster` 实例本身在现网造不出来**（§3.2） | **不可达（部署级）** | —（复活条件见 §3.2/§6.1） | — |
| 3 | `SM_CLOSEDOOR` | 614 | `smframe_ext.rs:171` `sm_closedoor` | `GameSrv/Word/WorldServer.cs:1485`（`CloseDoor` → `SendDoorStatus`） | `ProcessMapDoor`（`:1522`，仅 `door.Status.Opened` 才 `CloseDoor`）；`Opened` 唯一置 true 点是 `OpenDoor(:1471)`，而 `OpenDoor` 的唯一调用点 `ClientOpenDoor`（`PlayObject.Operate.cs:363-376`）**守卫写反即早退** ⇒ 恒 false | **不可达**（oracle 结构性，引 T-65） | — | — |
| 4 | `SM_HIDEEVENT` | 805 | `smframe_ext.rs:201` `sm_hideevent` | `PlayObject.cs:3278`（`SearchViewRange` 差集）；镜像 `RobotSystem/Services/RobotPlayer.Base.cs:766` | `WorldServer.cs:663-666`（玩家 `SearchTime=1000ms`）→ `PlayObject.SearchViewRange`（`:3073`）→ `CellType.Event` → `Get<MapEvent>(0)` → `UpdateVisibleEvent`（`:5465`）→ `VisibleEvents` 差集 `Hidden` 支 → `RM_HIDEEVENT` → `Message.cs:1992` | 需补上游（事件格 + `VisibleEvents` 未建模） | `crates/world`（事件格 / 可见事件集）+ `bins/game-svr`（映射） | R |
| 5 | `SM_ITEMSHOW` | 610 | `smframe_ext.rs:271` `sm_itemshow` | ①`BaseObject.cs:570`（`DropItemDown`）②`BaseObject.cs:752`（`DropGoldDown`）③`PlayObject.cs:3264`（`SearchViewRange` 的 `VisibleItems`）④`ExecutionProcessingSys.cs:5703`（`ActionOfThrowitem` 金币支，**T-179 未列的第 4 生产者**） | ①②③④ 全部以 `Envir.AddItemToMap(...)==true` 为前置；`AddItemToMap`（`Envirnoment.cs:88-92`）首行 `if (mapItem.ItemId == 0) return false;`，而**全仓无 `MapItem.ItemId = …` 赋值点**（`MapItem.cs:41` 被注释）⇒ 恒 false | **不可达**（oracle 结构性；引 T-19/T-25/T-32/T-179） | — | — |
| 6 | `SM_LAMPCHANGEDURA` | 655 | `smframe_ext.rs:291` `sm_lampchangedura` | `PlayObject.Message.cs:2265`（燃尽 → `0`）、`:2274`（磨损 → `Dura`），同一函数 `UseLamp`（`:2233`） | `PlayObject.Run`（`:19`）的 `DecLightItemDrugTime` 周期块（`:257-262`，缺省 500ms）→ `UseLamp()` → 右手槽 `Dura-1`（`DecLampDura=true`）→ 千位变化发 `:2274`、燃尽发 `:2265` → 处理器 `:1843`：**`Recog = nParam1 = Dura`，`Param = 0`** | 需补上游（`UseLamp` 未建模） | `crates/world`（`use_lamp` 结算）+ `bins/game-svr`（Run 段调用 + 映射） | R |
| 7 | `SM_OPENDOOR_OK` | 612 | `smframe_ext.rs:365` `sm_opendoor_ok` | `WorldServer.cs:1473`（`OpenDoor` → `SendDoorStatus`） | 同 #3 的左半：`ClientOpenDoor` 守卫写反 ⇒ `OpenDoor` 不可达 | **不可达**（oracle 结构性，引 T-65） | — | — |
| 8 | `SM_QUERYYBDEAL` | 23023 | `smframe_ext.rs:454` `sm_queryybdeal` | `ExecutionProcessingSys.cs:2550`（`ActionOfQueryTrustDeal`，`:2468`） | 脚本动作 `ExecutionCode.QueryYbDeal=344`（注册 `:127`）→ `SaleDeal && SellOffInTime(1) && SellOffItemList.Count>0` → `EDCode.EncodeBuffer(ClientDealOffInfo)` → `SendMsg(normNpc, RM_QUERYYBDEAL, …, sSendStr)` → `Message.cs:2053` | 需补上游（脚本动作 344 未实现 + 字节尾通道） | `crates/script/src/engine/actions.rs` + `bins/game-svr/src/pickwire.rs`（+ `npc.rs` 的 `SendMsg` 需能载字节尾） | R |
| 9 | `SM_QUERYYBSELL` | 23021 | `smframe_ext.rs:475` `sm_queryybsell` | `ExecutionProcessingSys.cs:2443`（`ActionOfQuerySaleSell`，`:2363`） | 脚本动作 `ExecutionCode.QueryYbSell=343`（注册 `:126`）→ `SaleDeal && SellOffInTime(0) && SellOffItemList.Count>0` → 同上 → `Message.cs:2049` | 需补上游（同 #8） | 同 #8 | R |
| 10 | `SM_RECONNECT` | 802 | `smframe_ext.rs:496` `sm_reconnect` | ①`PlayObject.Base.cs:3417`（`TransferPlanesServer`，**唯一调用点被注释**：`AppService.cs:179`）②`WorldServer.cs:1074`（`SendChangeServer`，走 **F7 直接 `SendDefMessage(SM_RECONNECT)`**；调用点 `:366`/`:609`）③`GameCommand/Commands/ReconnectionCommand.cs:38`（GM `@Reconnection`，perm 10） | ②`UserEnter` 发现目标图不在本服（`MapMgr.GetMapInfo` 返回 null）→ `SendSwitchData` + `SendChangeServer`；需 `GetMultiServerAddrPort`（读 `!servertable.txt`）③现网 `Permission` 恒 0 | 需补上游（G11 跨服搬家；GM 面被权限锁死） | 多服路由 + 宿主会话迁移；本仓已有 `SwitchDataState` 脚手架（`lib.rs:14731/14766`），**不建议先接** | R |
| 11 | `SM_SHOWEVENT` | 804 | `smframe_ext.rs:523` `sm_showevent` | `PlayObject.cs:3284`（`SearchViewRange` 差集）；镜像 `RobotPlayer.Base.cs:777` | 同 #4 的 `Show` 支：`CellType.Event` → `Get<MapEvent>(0)` → `Visible` ⇒ `UpdateVisibleEvent` → `Show` 支 → `RM_SHOWEVENT` → `Message.cs:1995` | 需补上游（同 #4；须复刻 `Id==0` 折叠） | 同 #4 | R |
| 12 | `SM_SKELETON` | 33 | `smframe.rs:2012` `sm_skeleton` | ①`PlayObject.cs:3231`（`SearchViewRange` 的 `Death && Skeleton` 支）②`PlayObject.Operate.cs:736`（`ClientGetButchItem` 屠宰成骨架）③镜像 `RobotPlayer.Base.cs:661` | ①AOI 出现帧；②`CM_BUTCH`（`Message.cs:829`）→ `ClientGetButchItem`（`Operate.cs:702`）→ 门 `Death && !Skeleton && Animal`（`:720`）+ `BodyLeathery<=0` ⇒ `Skeleton=true` → `SendRefMsg(RM_SKELETON)` → `Message.cs:1655` | 需补上游（`Skeleton`/`BodyLeathery` 与屠宰效果体未建模） | `crates/world`（屠宰效果体 + `skeleton` 字段）+ `bins/game-svr`（appear/ref-msg 分派） | R |

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
| F1 CM 派发 | **间接 ✔**：`CM_HIT/CM_HEAVYHIT/CM_POWERHIT/CM_LONGHIT/CM_BIGHIT/CM_CRSHIT` → `AttackDir` → `:27-31`。没有「直接的」`CM_* ⇒ RM_BREAKWEAPON` 支 —— 唯一路径经攻击。 |
| F2 脚本动作 | — 全树 `RM_BREAKWEAPON` 仅 1 处；`ExecutionProcessingSys` 无命中。 |
| F3 GM 命令 | — `Modules/GameCommand/**` 无命中（`@smakeitem` 只打印 `Desc[10]`，见 `SmakeItemCommand.cs:67`）。 |
| F4 宿主 tick | — `PlayObject.Run`/`BaseObject.Run` 无命中。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | **✔ 唯一生产者**（`SendRefMsg` 泵）。 |
| F7 直接发帧 | — 无 `SendDefMessage(SM_BREAKWEAPON)`（只在处理器里）。 |
| F8 内容/配置装载 | — 不产帧；但**决定 `Desc[10]`**：`GameSrv/Npc/Merchant.cs:682-736` 是 `Desc[ItemAttr.WeaponUpgrade]` 的唯一写入点（升级 NPC `@getbackupgnow` 取回）。 |

- **触发链（完整，含两道门）**：
  1. 武器升级 NPC（`Merchant.cs:677-736`）把 `Desc[10] ∈ {1,10..13,20..23,30..33}` 写进 `WeaponUpgradeInfo.UserItem`，玩家取回后装备；
  2. 下一次**攻击**：`AttackDir`（`PlayObject.Attack.cs:25-31`）**先判 `attackTarget != null`**（空挥不算），再判 `UseItems[Weapon].Index>0 && Desc[10]>0` ⇒ `CheckWeaponUpgrade()`；
  3. `CheckWeaponUpgradeStatus`（`:4146`）按 `Desc[0..2]` 求和与 `Config.UpgradeWeaponMaxPoint`（缺省 20）比较 ⇒ `Index=0`（碎）或 `Desc[0/1/2] += Desc[10]-9/-19/-29`（升级成功），随后 `Desc[10]=0`；
  4. `Index==0` ⇒ `SysMsg(TheWeaponBroke)` + `SendDelItems` + **`SendRefMsg(RM_BREAKWEAPON)`** + `FeatureChanged()`；否则 `SendUpdateItem` + `RM_ABILITY/RM_SUBABILITY`。
- **Rust 现状**：`crates/world/src/entity.rs:80` 的 `UseItemSlot.desc: [u8;14]` 已在（装载/存档都带）；`crates/world/src/merchant.rs:1218/1231/1244` 已按 `Merchant.cs:677-733` 写 `desc[10]`；`crates/world/src/combat.rs:755` 已复刻 `StruckDamage` 的单格耐久磨损。**缺**：`CheckWeaponUpgrade` / `CheckWeaponUpgradeStatus` 两个结算函数与攻击泵的调用点（含 `attackTarget != null` 门）。
- **判定**：**需先补上游生产者**（结算函数）。数据面齐备 ⇒ 成本低于「全新子系统」。
- **最小改动面**：`crates/world/src/`（升级结算纯函数 + 攻击路径调用）+ `bins/game-svr/src/lib.rs`（攻击泵发 `sm_breakweapon`；同批的 `SM_UPDATEITEM/RM_ABILITY/RM_SUBABILITY` 见 T-79⑥/T-95①，**本卡不扩范围**）。
- **tier**：**R**（战斗数值 + `characters_item` 落库时点）。

### 3.2 `SM_CHANGEFACE(1104)` — **不可达（现网部署级）**（v2 改判，订正 v1）

- **编码器**：`smframe_ext.rs:115` `sm_changeface(recog, target_id, feature, status)`（头 `(Recog, LoWord(target), HiWord(target), 0)` + 尾 `Encode(CharDesc 8B)`）。
- **oracle 发射点（全树 1 命中）**：`src/M2Server/Monster/Monsters/ElfWarriorMonster.cs:149`
  `SendRefMsg(Messages.RM_CHANGEFACE, 0, ActorId, elfMon.ActorId, 0, "")`。
  处理器：`Message.cs:2021-2029`（`nParam1 != 0 && nParam2 != 0` 门；尾 `CharDesc` 取 `ActorMgr.Get(nParam2)` = 克隆体）。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 无 `CM_*` 直接触发换形；换形是怪物 `Run` 的内部状态机。 |
| F2 脚本动作 | — 无命中。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 生产者所在面**：`ElfWarriorMonster.Run`（`:114-160`）由怪物 tick 驱动。 |
| F5 事件/魔法 | — `RM_CHANGEFACE` 全树只此 1 处；但**召唤链是 race-114 的候选来源**，已并入 F8 追踪（P5）。 |
| F6 对象广播 | **✔**（`SendRefMsg`）。 |
| F7 直接发帧 | — 无。 |
| F8 内容/配置装载 | **✔ 决定性面**：`ElfWarriorMonster` 的**唯一构造点** = `WorldServer.MonGen.cs:666`（按怪物表 `Race == 114` 分派）⇒ 只有「刷新出一个 race-114 怪」才会有实例。见下方逐点追踪。 |

- **race-114 能否生成的穷举（决定本帧存亡）**：

| # | 候选生成路径 | 逐点核对 | 结论 |
|---|---|---|---|
| P1 | 怪物表本身 | `sql/mir2_data.sql:372` `神兽1`(147, **Race=114**)，全表 705 行里 race-114 仅此一条 | 行存在（≠ 会被刷新） |
| P2 | MonGen 直接刷 | 部署 `E:/MirServer/M2GameSvr/Envir/MonGen.txt` 全树 **GBK 精确 token** 搜 `神兽1` = **0 命中**（`:2139` 唯一子串命中是 **`神兽10`**，而 monsters 表**没有** `神兽10`（只有 `神兽`/`神兽0`/`神兽1`/`神兽2`/`神兽3`）⇒ 该 MonGen 行是**死刷怪条目**：`RegenMonsterByName("神兽10")` 查不到怪物定义，恒 null；与 race-114 无关） | **✘** |
| P3 | 脚本刷怪（`MONSTER`/`RECALLMOB` 等） | 部署 `Envir/**`（`*.txt/*.ini/*.conf`）全树 GBK 精确 token `神兽1` = **0 命中** | **✘** |
| P4 | 神兽跟随态（113）→攻击态转换 | `ElfMonster.Run`（`ElfMonster.cs:43-83`）`MakeClone(SystemShare.Config.Dragon1, this)`（`:72`）；部署 `Server.conf:130` / `Server1.conf:130` **`Dragon1=神兽`**（113）⇒ 克隆出 113，`if (elfMon is ElfWarriorMonster monster)`（`:76`）**恒假** | **✘** |
| P5 | 召唤神兽魔法 | `MagMakeSinSuSlave`（`MagicManager.cs:1270-1301`）：默认名 = `Config.Dragon`（= 神兽）；`nExpLevel>4` 支拼 `Dragon + (Level/4+4)`（≥5，永不等于 1）；`DragonArray[i].sMonName` 来自 `[Names] Dragon{0..9}`（部署 `Server.conf:129-138` 全为 `神兽` 或空） | **✘** |
| P6 | Rust 世界层 | `crates/world/src/monai.rs:3632` `DRAGON1_NAME = "神兽"`（`elf_monster_run` `:3789` 用它）；`crates/data/src/pet_slave.rs:431` 断言「现网 Dragon1 覆盖初值 神兽1」——**这条是反证，不是正证** | **✘** |
| P7 | GM 命令 | 现网 `Permission` 恒 0（T-156③ / `gmcmd.rs` 设计件 §2），无召 114 的可达命令 | **✘** |

⇒ **现网两侧都没有任何路径生成 race-114 实例** ⇒ `ElfWarriorMonster.Run` 不执行 ⇒ `RM_CHANGEFACE` 永不发出（oracle 与 Rust 一致）。

- **代码缺口仍然存在（v1 的这一半是对的，保留）**：
  - 世界层已建模并已发帧：`crates/world/src/monai.rs:2893` `flush_sculture_spawns` 的 `MakeClone` 支执行 `apply_make_clone` + `elf_monster_appear_now` + `send_ref_msg(RM_DIGDOWN)` + **`send_ref_msg(RM_CHANGEFACE, 0, req.owner_id, child, 0)`（`:2980-2988`，ident 在 `:2983`）**；
  - 分支已 1:1 建模：`elf_warrior_run`（`monai.rs:3815-3880`）复刻 `boChangeFace` 三条件、`> 60s` 严格大于门、名字去 `'1'`、`else { DigDownTick = now }` 作用域陷阱（改坏必红用例 `mb7_elf_warrior_dig_down_tick_not_refreshed_when_change_face_true`）；
  - 宿主 `drain_monster_ref_msgs`（`lib.rs:15409`）**无 `RM_CHANGEFACE` 臂**，落 `:15527` 的 `_ => continue`；
  - ⇒ 一旦内容面把 `Dragon1` 改回 `神兽1`，**oracle 侧**立刻会克隆出 114；但 **Rust 侧还不会**——世界层的克隆名是硬编码常量，见下面的三步复活条件。
- **判定**：**不可达（现网部署级）**。与 `SM_ITEMSHOW`/门链的「oracle 结构性」不同：这里 oracle 代码会正常执行，只是**冻结的部署内容**（`Server.conf` 的 `Dragon1` + `MonGen` + `Envir` 脚本）不会触发它。
- **不接线理由**：接线后现网仍零帧（两侧都不发），收益为 0，却会让人误以为「已支持换形」。真正的动作是**让内容侧先有 race-114**（须 owner 裁决，因为 `Server.conf`/`Envir` 属内容冻结面）。
- **复活条件（写给将来的卡）：三步、代价 M（跨 crate），不是 S**。
  1. **内容面**：把 `Server.conf` / `Server1.conf` 的 `Dragon1` 改回 `神兽1`（内容冻结面 ⇒ **须 owner 裁决**）；
  2. **`crates/world`**：把硬编码常量 `DRAGON1_NAME`（`monai.rs:3632`，`:3789` 的 `MakeClone` 用它；其注释自述「宿主配置通路不在本车道写集 … 若换了 `Server.conf` 的 `Dragon1`，克隆目标类会随之变化」）改为**随配置** —— `World` 增 `dragon1` 字段（或在 `elf_monster_run` 路径传入 `PetSlaveConfig`），由 `bins` 从 `crates/data/src/pet_slave.rs`（`:112` 字段 / `:241` 解析）注入；**注意** `crates/world/src/slave.rs:252` 的**召唤**路径已消费 `cfg.dragon1`，但**克隆路径没走它**；
  3. **`bins/game-svr/src/lib.rs:15409`**：补 `RM_CHANGEFACE` 臂（`feature/status` 按 `world.entities.get(m.n_param2)` 在排水时取；oracle 处理器是收件人下一 Run 才读）。
  - **只做 ①+③ 的后果**：oracle 克隆出 114（`AppearNow` 攻击形态）、Rust 仍克隆 113 ⇒ **先出现怪物行为差异**（race / 形态 / 攻击数值），且 Rust 永不发 1104 ⇒ 双侧 diff 从这里裂开。
  - **真机锚**：114 出现 + `AppearNow` 攻击形态 + 1104 帧。
- **tier**：不适用（不接线）。

### 3.3 `SM_CLOSEDOOR(614)` / 3.7 `SM_OPENDOOR_OK(612)` — 不可达（引 T-65 + 独立复核 + 逐帧面表）

- **编码器**：`smframe_ext.rs:171` `sm_closedoor(n_param1, n_param2)`、`:365` `sm_opendoor_ok(n_param1, n_param2)`。
- **oracle 发射点（各 1 命中）**：`GameSrv/Word/WorldServer.cs:1485`（`CloseDoor`）、`:1473`（`OpenDoor`），均经 `SendDoorStatus`（`:1488`）→ `SendRefMsg`；处理器 `Message.cs:1750-1756`。
- **逐帧面表（补 v1 缺失的逐帧穷举）**：

`SM_OPENDOOR_OK`（生产者 = `OpenDoor`）：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | **间接 ✔**：`CM_OPENDOOR(1002)` → `ClientOpenDoor`（`PlayObject.Operate.cs:363`）→ 唯一调用点，但**守卫写反早退** ⇒ 不进 `OpenDoor`。 |
| F2 脚本动作 | — 无 `RM_DOOROPEN` 命中。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | — `ProcessMapDoor` 只调 `CloseDoor`，不调 `OpenDoor`。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | **✔ 唯一生产者**（`OpenDoor`→`SendDoorStatus`→`SendRefMsg`）。 |
| F7 直接发帧 | — 无 `SendDefMessage(SM_OPENDOOR_OK)`。 |
| F8 内容/配置装载 | — 门由地图文件装载（`mapfile.rs` 定案⑥），装载本身不产帧。 |

`SM_CLOSEDOOR`（生产者 = `CloseDoor`）：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 无 `CM_*` 直调 `CloseDoor`。 |
| F2 脚本动作 | — 无 `RM_DOORCLOSE` 命中。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 唯一调用面**：`ProcessMapDoor`（`:1522`，5s 到期）—— 但门 `if (door.Status.Opened)` 恒假。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | **✔ 唯一生产者**（`CloseDoor`→`SendDoorStatus`→`SendRefMsg`）。 |
| F7 直接发帧 | — 无。 |
| F8 内容/配置装载 | — 不产帧。 |

- **独立复核**：
  - `door.Status.Opened = true` 全仓唯一写点 = `OpenDoor`（`:1471`）；
  - `OpenDoor` 唯一调用点 = `ClientOpenDoor`，其首行 `if (Envir.GetDoor(nX, nY, ref door)) { return; }` —— **门存在即早退**（守卫写反，T-65）；
  - ⇒ `Opened` 恒 false ⇒ `CloseDoor` 首行 `if (!door.Status.Opened) return;` 恒早退，`ProcessMapDoor` 也恒不进 ⇒ **两帧生产者均不可达**。
  - 干扰项：`CastleDoor.Open/Close`（`Monster/Monsters/CastleDoor.cs:46-77`）发的是 `RM_DIGUP/RM_DIGDOWN`，**不是** `RM_DOOROPEN/RM_DOORCLOSE`，且用另一字段 `IsOpened`。
- **判定**：**不可达**（oracle 结构性），沿用 T-65（`whitelist.md:150`）与 T-176。**不做接线**。

### 3.4 `SM_HIDEEVENT(805)` / 3.11 `SM_SHOWEVENT(804)` — 需补上游（可达但退化；`MapEvent` 家族已穷举）

- **编码器**：`smframe_ext.rs:201` `sm_hideevent(n_param1, w_param, n_param2, n_param3)`、`:523` `sm_showevent(...)`（后者尾 = `Encode(ShortMessage 4B)`）。
- **oracle 发射点（各 1 主 + 1 机器人镜像）**：
  - `src/M2Server/Player/PlayObject.cs:3278`（`RM_HIDEEVENT`）、`:3284`（`RM_SHOWEVENT`），同一段 `VisibleEvents` 差集循环；
  - 镜像：`RobotSystem/Services/RobotPlayer.Base.cs:766/777`（机器人玩家，`@AddRobotPlay` 代码段被注释 ⇒ 现网不产生，不计入生产面）。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 无「直接发帧」的 `CM_*`；但 `CM_*` 造成的位移/进图会间接驱动下一次 `SearchViewRange`（见 F4）。 |
| F2 脚本动作 | — 无命中（脚本的 `MOBFIREBURN` 只经 F5 注册事件，不发帧）。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 唯一驱动面**：`WorldServer.cs:663-666` 玩家 `SearchTime=1000ms` 节流 → `PlayObject.SearchViewRange()`（`PlayObject.cs:3073`）。 |
| F5 事件/魔法 | **✔ 事件格来源**：火墙 / 圣言 / 掘尸三支（逐支见 §4 表），`EventProcessor`（`GameSvr/Word/Threads/EventProcessor.cs`，20ms）跑 `Run()/Close()`。 |
| F6 对象广播 | — 帧不上 `SendRefMsg`，走自身 `MsgQueue`（只投给自己）。 |
| F7 直接发帧 | — 无 `SendDefMessage(SM_SHOWEVENT/SM_HIDEEVENT)`。 |
| F8 内容/配置装载 | **✔ 反向证据（决定可达性）**：`SafeEvent` 的注册点 `MapManager.MakeSafePkZone` 因门条件写错**从不构造**；`StoneMineEvent` 因 `AddToMapMineEvent` 的 `cellSuccess && !Valid` **逻辑不可满足**而**从不注册**（推理见 §4）。⇒ 启动期**没有任何事件格、`CellObjectMgr` key 0 为空**。 |

- **触发链**：位移/进图 → `WorldServer` 1000ms 节流 → `SearchViewRange` → 逐格扫 `CellType.Event` → `Get<MapEvent>(CellObjId=0)` → `Visible ? UpdateVisibleEvent(nX,nY,e)` → 扫完做 `Hidden/Show` 差集 → `RM_SHOWEVENT` / `RM_HIDEEVENT` → 处理器 `Message.cs:1992/1995`。
- **Rust 现状**：`crates/world/src/aoi.rs` 只建模 `VisibleActors`（`UpdateVisibleGay`）；**无** `CellType.Event`、**无** 观察者侧 `VisibleEvents`；`crates/world/src/world.rs:3882` 的 `run_map_events` 只跑火墙伤害/掘尸**模拟**，不产可见性事件。
- **判定**：**需先补上游生产者**（事件格 + 可见事件集 + 差集），且必须**连 `Id==0` 折叠语义与 key-0 绑定史一起复刻**（否则接线即 diff）。
- **最小改动面**：`crates/world/src/aoi.rs`（事件格 + `VisibleEvents` 差集）+ `crates/world/src/world.rs`（事件注册与 key-0 折叠口径）+ `bins/game-svr/src/lib.rs`（映射 `sm_showevent`/`sm_hideevent`）。
- **tier**：**R**（AOI 时序 + 观察者差集序；折叠语义须真机核）。

### 3.5 `SM_ITEMSHOW(610)` — 不可达（oracle 结构性）

- **编码器**：`smframe_ext.rs:271` `sm_itemshow(...)`（头 + `EncodeString(Msg)`）。
- **oracle 发射点（全树 4 命中，比 T-179 多 1 个）**：
  1. `src/M2Server/Actor/BaseObject.cs:570` —— `DropItemDown`（`:529`）成功支；
  2. `src/M2Server/Actor/BaseObject.cs:752` —— `DropGoldDown`（`:735`）成功支；
  3. `src/M2Server/Player/PlayObject.cs:3264` —— `SearchViewRange` 的 `VisibleItems` `Show` 支（上游 `UpdateVisibleItem`，被 `:3155` 的 `if (mapItem.ItemId == 0) continue;` 挡住）；
  4. **`src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:5703`** —— `ActionOfThrowitem`（`:5638`，`ExecutionCode 341`）金币支（`:5756` 的物品支已整段注释）。
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
| F8 内容/配置装载 | 反证：**全仓无 `MapItem.ItemId = …` 赋值点**（`Modules/SystemModule/Data/MapItem.cs:41` 是注释）。 |

- **不可达证明（独立复核）**：`AddItemToMap`（`Envirnoment.cs:88-92`）首行 `if (mapItem.ItemId == 0) return false;`；`MapItem` 构造（`MapItem.cs:39-42`）的 `ItemId` 赋值被注释 ⇒ ①②④ 的 `if (AddItemToMap(...))` 恒 false；③ 的前置 `if (mapItem.ItemId == 0) continue;`（`PlayObject.cs:3155`）恒真 ⇒ `UpdateVisibleItem` 从不被调用。
- **判定**：**不可达**（oracle 结构性）。沿用 T-19/T-25/T-32/T-179，本卡补第 4 生产者并逐条复核。
- **注意**：Rust 侧 `ground.rs` 已 1:1 复刻「`item_id == 0 ⇒ 不落地`」（T-19 白名单）；本帧接线会**直接制造 diff**。

### 3.6 `SM_LAMPCHANGEDURA(655)` — 需补上游

- **编码器**：`smframe_ext.rs:291` `sm_lampchangedura(n_param1: i32)` —— `command_header(n_param1, SM_LAMPCHANGEDURA, 0, 0, 0)` ⇒ **第 1 字段（`Recog`）= `n_param1`**，`Param/Tag/Series` 全 0。
- **线上字段口径（v2 订正 v1 的错）**：oracle `SendDefMessage(short wIdent, int nRecog, int nParam, int nTag, int nSeries)`（`PlayObject.Socket.cs:17`）⇒ 第 2 实参是 **`Recog`**。处理器 `:1843` 写的是 `SendDefMessage(SM_LAMPCHANGEDURA, processMsg.nParam1, 0, 0, 0)` ⇒ **`Recog = Dura`，`Param = 0`**（BotSrv `RobotPlayer.Operate.cs:494-497` 也读 `msg.Recog`）。v1 正文写成「`Param = Dura`」是错的；编码器本身一直是对的。
- **oracle 发射点（1 函数内 2 处）**：`PlayObject.Message.cs:2265`（燃尽：`SendMsg(RM_LAMPCHANGEDURA, 0, 0, 0, 0)` ⇒ `Recog=0`）、`:2274`（磨损：`SendMsg(RM_LAMPCHANGEDURA, 0, UseItems[RighThand].Dura, 0, 0)` ⇒ `Recog=Dura`）；同一函数 `UseLamp`（`:2233`）。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — `CM_*` 无直接支；`CM_TAKEONITEM` 只改装备、不结算耐久。 |
| F2 脚本动作 | — 无命中。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | **✔ 唯一驱动面**：`PlayObject.Run`（`:19`）的 `DecLightItemDrugTime` 周期块（`:257-262`，缺省 **500ms**，`GameSvrConf.cs:1398`）→ `UseLamp()`（`:2233`）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | — **不广播**：`SendMsg` 只投给本人（`MsgQueue`），无 AOI。 |
| F7 直接发帧 | — 无。 |
| F8 内容/配置装载 | **✔ 前提**：右手槽 `StdItem.SpecialPwr == 0` 才是火把/蜡烛；`Config.DecLampDura` 缺省 **true**（`GameSvrConf.cs:1504`）。 |

- **触发链**：装备右手火把 → 每 500ms `UseLamp()` → `nDura = Dura-1`（`DecLampDura`）⇒ ①`nDura<=0`：`Dura=0` + `SendDelItems` + `Index=0` + `Light=GetMyLight()` + `RM_CHANGELIGHT` + **655（`Recog=0`）** + `RecalcAbilitys()`；②`Round(旧/1000) != Round(新/1000)`：**655（`Recog=新 Dura`）**。
- **Rust 现状**：`crates/world/src/entity.rs:667` 的 `use_items[UseItemSlot{dura,desc,…}]` 已在；`crates/world/src/world.rs:827` 已有 `dec_light_item_drug_time_ms`，`check_pk_status`（`:1503`）已用同一周期节流（**同一 oracle 分支的另一半已落地**）；`bins/game-svr/src/downlink.rs:53` 的 `my_light` 已实现 `GetMyLight`（含右手槽）。**缺**：`UseLamp` 本体（`SpecialPwr==0` 判定 + 耐久扣减 + 燃尽清槽）与调用点；`DecLightItemDrugTime` 键未解析（`lib.rs:17024` 用缺省 500 并已登记）。
- **判定**：**需先补上游生产者**（一个纯函数 + 一个调用点）。
- **最小改动面**：`crates/world/src/`（`use_lamp` 纯函数，按 oracle 逐行）+ `bins/game-svr/src/lib.rs`（Run 段同周期分支调它并映射 `sm_lampchangedura`）。
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
| F1 CM 派发 | — 直接支只有 `CM_SELLOFF*`（7 条），**不产** 23021/23023；只更新 `SellOffItemList/SaleDeal`。 |
| F2 脚本动作 | **✔ 唯一生产者面**（343/344）。 |
| F3 GM 命令 | — 无命中。 |
| F4 宿主 tick | — 无命中（无定时器触发查询）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | — 走 `SendMsg(npc, player)` 单投（不发 `SendRefMsg`）。 |
| F7 直接发帧 | — 无 `SendDefMessage(SM_QUERYYB*)`。 |
| F8 内容/配置装载 | — 不产帧；**状态前提**：`PlayObject.SaleDeal` + `M2Share.SellOffItemList` + `SellOffInTime(0/1)`。 |

- **触发链**：NPC 脚本执行到 `QUERYYBSELL`/`QUERYYBDEAL` → `ActionOfQuerySaleSell/TrustDeal` → 门 `SaleDeal && SellOffInTime(code) && SellOffItemList.Count > 0` → 组 `ClientDealOffInfo{UseItems[9], DealChrName, BuyChrName, SellDateTime, SellGold, N}` → `EDCode.EncodeBuffer(...)` → `SendMsg(normNpc, RM_QUERYYB*, 0,0,0,0, sSendStr)` → 玩家下一 `Run` 出帧。
- **Rust 现状（分层）**：
  - **状态面已就绪**：`bins/game-svr/src/selloff.rs`（`SellOffBook` / `SellOffState{sale_deal}` / `sell_off_in_time`（`:1027`）/ 7 个 `CM_SELLOFF*`）+ 宿主会话同步（`lib.rs:5767-5788`）；
  - **脚本执行器已存在**：`crates/script/src/engine/actions.rs` + `dispatch.rs` 的 `IMPLEMENTED_EXECUTIONS`（70 条，含 `ActionOfOpenSaleDeal`，**不含** `ActionOfQuerySaleSell/QueryTrustDeal`）；
  - **脚本→帧的管道已存在**：`bins/game-svr/src/pickwire.rs:151` `script_frame`（已映射 `RM_GOLDCHANGED`/`RM_SCRIPT_GAMEGOLDCHANGED`/`RM_SYSMESSAGE`/`RM_SCRIPT_PLAYDICE`，其余 `_ => None`）+ `lib.rs:6210` `emit_script_frames`；
  - **缺 ①**：两个动作体（`ClientDealOffInfo` 逐字段装配 + `EDCode.EncodeBuffer`）；
  - **缺 ②**：尾是**字节缓冲**，而 `bins/game-svr/src/npc.rs:109-116` 的 `SendMsg.msg: Option<String>` 载不了任意字节 ⇒ 需要给脚本帧加字节尾通道（或让动作直接产 `Vec<u8>` 帧）。②是结构性改动，也是本族成本的主要来源。
- **判定**：**需先补上游生产者**（动作体 + 字节尾通道）。
- **最小改动面**：`crates/script/src/engine/actions.rs` + `dispatch.rs`（登记）+ `bins/game-svr/src/npc.rs`（`SendMsg` 字节尾）+ `bins/game-svr/src/pickwire.rs`（两条映射）。
- **tier**：**R**（`EDCode.EncodeBuffer` 逐字节尾 + 寄售列表状态）。

### 3.10 `SM_RECONNECT(802)` — 需补上游（另见 §5 专章）

- **编码器**：`smframe_ext.rs:496` `sm_reconnect(msg: &[u8])`（头全 0 + 尾 `EncodeString(Msg)`）。
- **oracle 发射点（全树 3 命中 + 1 处绕过 RM 泵）**：
  1. `src/M2Server/Player/PlayObject.Base.cs:3417` —— `TransferPlanesServer()`：`SendMsg(RM_RECONNECTION, …, serveraddr + '/' + gamePort)`；**唯一调用点 `GameSrv/AppService.cs:179` 被注释**；
  2. `src/GameSrv/Word/WorldServer.cs:1074` —— `SendChangeServer()`：**直接** `SendDefMessage(SM_RECONNECT, 0,0,0,0, "{ip}/{port}")`（**F7 绕过 RM 泵**，不经 `:1988` 处理器）；调用点 `:366` / `:609`；
  3. `src/Modules/GameCommand/Commands/ReconnectionCommand.cs:38` —— GM `@Reconnection`：`SendMsg(PlayerActor, RM_RECONNECTION, …, sIPaddr + '/' + sPort)`。
- **面穷举**：

| 面 | 结论 |
|---|---|
| F1 CM 派发 | — 无 `CM_*` 支（`CM_SOFTCLOSE` 只置 `BoReconnection/BoSoftClose`，不发 802）。 |
| F2 脚本动作 | — 无命中。 |
| F3 GM 命令 | **✔**（③）但 `[Command(..., 10)]` + 现网 `Permission` 恒 0 ⇒ **被权限门挡死**（`gmcmd.rs` 设计件 §2 / T-156③）。 |
| F4 宿主 tick | **✔**（② 的 `:366` 在 `UserEnter`、`:609` 在 SwitchData 回写轮询）。 |
| F5 事件/魔法 | — 无命中。 |
| F6 对象广播 | — `TransferPlanesServer` 用 `SendMsg` 单投；`SendChangeServer` 用 `SendDefMessage` 单投。 |
| F7 直接发帧 | **✔**（② 的 `SendDefMessage(SM_RECONNECT)`）。 |
| F8 内容/配置装载 | **✔**：`!servertable.txt`（`GameApp.cs:267`）→ `GetMultiServerAddrPort`（`GameShare.cs:57` / `M2Share.cs:246`）；`MapManager.GetMapOfServerIndex` 决定「是否跨服」。 |

- **触发链**：玩家进图请求的地图**不在本服** ⇒ `MapMgr.GetMapInfo(ServerIndex, MapName) == null`（`WorldServer.cs:352`）⇒ 回滚会话字段 + `SendSwitchData` + `SendChangeServer`（`:366`）⇒ `GetMultiServerAddrPort(ServerIndex)` 成功 ⇒ `BoReconnection = true` + `SendDefMessage(SM_RECONNECT, ip/port)`；随即客户端回 `SwitchDataOk` 触发 `:609` 的第二发（100ms 后）。
- **判定**：**需先补上游生产者**（G11 跨服搬家），不是「接线一行的活」。
- **tier**：**R**。

### 3.12 `SM_SKELETON(33)` — 需补上游

- **编码器**：`smframe.rs:2012` `sm_skeleton(actor_id, x, y, dir, char_desc)`（头 + 尾 `Encode(CharDesc 8B)`）。
- **oracle 发射点（全树 3 命中：2 生产 + 1 机器人镜像）**：
  1. `src/M2Server/Player/PlayObject.cs:3231` —— `SearchViewRange` 的 `Death && Skeleton` 支；
  2. `src/M2Server/Player/PlayObject.Operate.cs:736` —— `ClientGetButchItem`（`:702`）屠宰成骨架；
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
| F8 内容/配置装载 | — 不产帧；`Skeleton` 是实体状态字段。 |

- **`Skeleton` 的唯一 setter（关键，含 v2 补的门）**：`M2Server` 全仓 `Skeleton = true` 只在 `ClientGetButchItem`（`PlayObject.Operate.cs:734`），前置是**三道门**：`baseObject.Death && !baseObject.Skeleton && baseObject.Animal`（`:720`），然后 `BodyLeathery -= Random(16)+5` 后 `BodyLeathery <= 0` **且** `Race ∈ [Animal, Monster)`（`:727-737`）。⇒ 可屠宰集合 = `Animal == true` 的实体。
  - `Animal = true` 的**全部生产点**（v2 实测，不止一个）：`WorldServer.MonGen.cs:521/531/540/549`（race `AnimalChicken`/`AnimalDeer`/`AnimalWolf` 的刷怪支）、`BaseObject.cs:2602`（Appr 分派 `case 54`）、`StickMonster.cs:23`、`SpitSpider.cs:15`、`Scorpion.cs:7`、`GasAttackMonster.cs:12`（ctor）。**后续卡必须建模 `Animal` 字段与这些生产点**，不能只看 `Race` 区间。
  - 另：`BotSrv/Objects/TActor.cs:529` 是机器人镜像的另一套字段，与本帧生产面无关（v1 的「全仓」应读作「`M2Server` 全仓」）。
- **Rust 现状**：`crates/world/src/entity.rs` 有 `ai.meat_quality`（`mongen.rs:434-451` 生产），**无** `skeleton`、**无** `body_leathery`、**无** `Animal` 标志；`bins/game-svr/src/inbound.rs` 的 `client_butch` 只做**决策**（`target_butcherable` 由调用方喂），**效果体（两次 RNG + MeatQuality/BodyLeathery/Skeleton）未建模**（`inbound.rs:49` 已登记）；`bins/game-svr/src/downlink.rs:543-556` 的 `appear_frames` 只有 `dead ⇒ SM_DEATH` 一支，明确登记 `SM_SKELETON` 为 `deferred`。
- **判定**：**需先补上游生产者**（`Animal`/`BodyLeathery`/`Skeleton` 与屠宰效果体）。
- **最小改动面**：`crates/world/src/`（`Entity` 加 `animal`/`skeleton`/`body_leathery`；屠宰效果体按 `Operate.cs:720-742`）+ `bins/game-svr/src/inbound.rs` + `bins/game-svr/src/downlink.rs:550`（`appear_frames` 加 `skeleton` 支）+ `bins/game-svr/src/lib.rs`（`RM_SKELETON` 映射）。
- **tier**：**R**（两次 `Random` 抽数顺序 + `MeatQuality`/`BodyLeathery` 数值）。

---

## 4. 事件族专章：`MapEvent` 全子类穷举 + `Id == 0` 折叠 + key-0 终态

v1 只证明了「`Id` 恒 0 ⇒ 折叠」，没证明「key 0 的终态」，因此「可达」站不住。v2 补齐：先穷举 `MapEvent` 的**全部子类**与**全部注册点**，再推 key 0 的终态。

### 4.1 `MapEvent` 全子类与注册点（全树 `: MapEvent` / `new MapEvent(`）

| 子类 | 注册点（文件:行） | 基类 `Visible` | 是否建 `CellType.Event` 格 | 现网是否发生 |
|---|---|---|---|---|
| `FireBurnEvent`（火墙） | `MagicManager.cs:1074/1079/1084/1089/1094`（玩家施法，5 格）、`FireMonster.cs:32/37/42/47/52` | **true**（`FireBurnEvent.cs:17` `base(…, true)`） | **✔**（基类 ctor `MapEvent.cs:79` `Envirnoment.AddMapEvent`） | **✔ 会发生** |
| `HolyCurtainEvent`（圣言） | `MagicManager.cs:1179-1201`（8 格） | **true**（`HolyCurtainEvent.cs:11`） | **✔** | **✔ 会发生** |
| `MapEvent`（掘尸，无子类） | `DigOutZombi.cs:59`（`new MapEvent(…, 5*60*1000, true)`） | **true** | **✔** | **✔ 会发生**（DigOutZombi 出土时） |
| `SafeEvent`（安全区光环） | `MapManager.cs:45`（`MakeSafePkZone`） | true（`SafeEvent.cs:13`） | ✔（若能构造） | **✘ 从不构造**：门 `string.IsNullOrEmpty(startPoint.MapName) && startPoint.Type > 0` 成立时 `FindMap("")` 必 null ⇒ 内层 `if (envir != null)` 恒假 |
| `StoneMineEvent`（矿） | `Maps.cs:439`（`MakeStoneMines`，矿图**逐格**） | **false**（`StoneMineEvent.cs:15` `base(…, false)`） | **✔（若注册成功）** —— 子类自建：`Envirnoment.AddMineToEvent`（`:785`）/`AddToMapMineEvent`（`:806`） | **✘ 从不注册**（证明见 4.2） |

⇒ 启动期**唯一**可能建事件格的是「矿」这一支，而它必然失败；其余三支都在运行期由施法/怪物驱动。

**同一 `CellObjectMgr` 命名空间里的非 `MapEvent` 占用者（v2 补）**：`AddMapDoor`（`Envirnoment.cs:193-215`）对 `CellType.Door` 也执行 `CellObjectMgr.Add(cellObject.CellObjId /* = MapDoor.DoorId = 0 */, mapDoor)`（`:215`）。它不建事件格，但会**抢 key 0**，相位见 §4.3 的 ②。

### 4.2 `StoneMineEvent` 从不注册（C-2 的核心证明，全静态可复现）

1. `ET_MINE = 2`（`OpenMir2/Grobal2.cs:42`）⇒ `StoneMineEvent` ctor 走 `else` 支（`StoneMineEvent.cs:32-53`）⇒ 调 `envir.AddToMapMineEvent(nX, nY, this)`（55/56/57 才走 `AddMineToEvent`）。
2. `AddToMapMineEvent`（`Envirnoment.cs:806-845`）的门（`:806-812`）：`ref MapCellInfo cellInfo = ref GetCellInfo(nX, nY, out bool cellSuccess); if (cellSuccess && !cellInfo.Valid)`。
3. `GetCellInfo`（`:322-336`）**只有 `cellInfo.Valid` 为真才 `success = true`**（`:327-331`），否则 `success = false`。
4. `MapCellInfo.Valid => Attribute == CellAttribute.Walk`（`Modules/SystemModule/Data/MapCellInfo.cs:19`）。
5. ⇒ `cellSuccess ⇒ Valid ⇒ !Valid == false` ⇒ 门**逻辑不可满足** ⇒ `AddToMapMineEvent` **恒 false** ⇒ `CellObjectMgr.Add(0, mine)` **永不执行**。
6. ⇒ 每个矿格都落到失败支 `Maps.cs:440-443`：`CellObjectMgr.Remove(mine.Id /* 0 */); mine.Dispose();`；`MapEvent.Dispose(bool)`（`MapEvent.cs:126-152`）只清 `OwnBaseObject` + `IsDispose`，**不动** `CellObjectMgr`。

**结论**：v1 担心的替代情形（「key 0 被一个 `Visible=false` 的矿事件占住 ⇒ 804/805 永不发」）**不成立**；实际效果相反——`MakeStoneMines` 是**逐格 `Remove(0)` 的清扫器**。

### 4.3 key-0 的生命周期（部署唯一矿图：`Envir/MapInfo.txt:636` `[D401 废矿入口] MINE`）

| 阶段 | key 0 的持有者 | 依据 |
|---|---|---|
| ① 地图装载（`GameApp.Initialize` → `Map.LoadMapInfo`，`GameApp.cs:124`） | **空**（`SafeEvent` 不构造、`StoneMineEvent` 不注册、事件尚未发生、门还没装载） | 4.1 / 4.2 |
| ② `InitializeWorld` → `MapMgr.LoadMapDoor()`（`GameApp.cs:226`，在 `StartUp` **之前**） | **首个 `MapDoor`** —— `MapDoor.DoorId` 的 `GetNextIdentity()` 同样被注释（`src/Modules/SystemModule/Data/MapDoor.cs:15`）⇒ 恒 0；`LoadMapDoor`（`MapManager.cs:182-189`）→ `AddDoorToMap`（`Envirnoment.cs:298-305`）→ `AddMapDoor`（`:193-215`，要求 walkable）→ `CellObjectMgr.Add(0, door)`（`:215`）。第一个成功登记的门占住 key 0 | `MapManager.cs:182-189` + `Envirnoment.cs:193-215/298-305` + `MapDoor.cs:15` |
| ③ `StartedAsync` → `StartUp` → `Map.StartMakeStoneThread()`（`Maps.cs:7-13`，**后台线程**） | 扫 D401 每一格 ⇒ 每格一次 `Remove(0)`：**第一格就把 ② 的门删掉**，此后 key 0 **空**；期间若有火墙占了 key 0，也会被后续格子扫掉 | `Maps.cs:439-443` |
| ④ 清扫结束 · 第一发任意 `Visible` 事件 | `Add(0, e)` 成功（key 空）⇒ key 0 = 第一个 `FireBurnEvent`/`HolyCurtainEvent`/掘尸事件（`Visible=true`） | `MapEvent.cs:79` + `CellObjectMgr.cs:10-16`（`TryAdd`） |
| ⑤ 该事件 `Close()` 之后 | **仍是它**（`Visible=false`）；`DeleteFromMap` 的 `CellObjectMgr.Remove(cellId)` 只在 `CellObjId > 0` 支（`Envirnoment.cs:652-667`），事件 cellId 恒 0 ⇒ key 0 **永不移除** | `MapEvent.cs:109-123` + `Envirnoment.cs:652` |
| ⑥ 之后的新事件 | `TryAdd` 恒失败 ⇒ 再也进不了 key 0 | `CellObjectMgr.cs:10` |

> 订正 v1 的两句话：
> 1. 事件**格子对象**并非「永不移除」——`DeleteFromMap` 对 `CellObjId == 0` 走 `else` 支会把该格 id-0 的 cell 直接 `Remove(nIdx)`（`:676-687`）。永不移除的是 **`CellObjectMgr` 的 key 0**。
> 2. v1 写「地图装载后 key 0 为空」**不成立**：`LoadMapDoor` 在 `StartUp` 之前就把 **首个 `MapDoor`** 放进 key 0（`DoorId` 同样恒 0）。稳态结论不变，因为矿清扫的第一格就把它删掉。
> 3. **门占位期间的强转风险**：若此刻已存在 `CellType.Event` 格（需要「门已装载 + 还没被矿清扫删掉 + 已有人施火墙」三者同时成立），`CellObjectMgr.Get<MapEvent>(0)` 会把 `MapDoor` 强转成 `MapEvent`（`CellObjectMgr.cs:18-22` 的 `(T)cell`）⇒ `InvalidCastException`，被 `SearchViewRange` 的 `catch` 吞掉并 **`KickException()`**（`PlayObject.cs:3288-3293`）。本部署因矿清扫在登录前完成而**不可观察**，列入 §4.5 真机锚。

### 4.4 由此得到的可判结论

- (a) **可达，但有窗口**：只要「第一个 `Visible` 事件」尚未 `Close()`，任何玩家**首次**走进任意 `CellType.Event` 格（哪张图都行 —— `Get<MapEvent>(0)` 与格子所在图无关）都会收到**一条** `SM_SHOWEVENT`；离开后收到**一条** `SM_HIDEEVENT`（`PlayObject.cs:3271-3287` 的差集）。
- (b) **退化**：所有事件格共用同一对象 ⇒ 同一玩家在任一事件格只会 `Show` 一次；`EventType` 恒为「第一个事件」的类型；`nX/nY` 是**当前格子**坐标而非事件注册坐标；`SM_HIDEEVENT` 的 `nParam1`（= `mapEvent.Id`）= 0。
- (c) **窗口 = 第一个事件的生命周期**：火墙 `ContinueTime = time*1000`，`Close()` 后 `Visible=false` ⇒ 此后**永不**再发 804/805。
- (d) **启动清扫竞态（须真机核）**：`MakeStoneMines` 是后台线程，扫 D401 每一格都 `Remove(0)`。玩家若在清扫期间施火墙，key 0 可能被扫掉、再由下一发火墙重新占住 ⇒「第一个事件」到底是谁取决于清扫何时结束。
- (e) **Rust 必须照抄**：若按「每个事件一个 cell id + 正常差集」实现，**第二次进事件格会多出一条 804**（oracle 不发）⇒ 直接 diff。建议在 `crates/world` 显式建模「`first_event_only`：全进程唯一事件槽 + 关闭后不再替换」，并登记该 oracle 缺陷。

### 4.5 还没证明的部分（如实标注）

- **哪些格子会成为 `CellType.Event`**：取决于火墙 5 格是否 walkable（`AddMapEvent` 要求 `GetCellInfo` 成功）。构造级判据取「施法中心格确定 walkable」即可覆盖。
- **`GetNextIdentity` 注释家族（全树只有 3 处被注释）**：`MapItem.ItemId`（`MapItem.cs:41`）、`MapEvent.Id`（`MapEvent.cs:62`）、`MapDoor.DoorId`（`MapDoor.cs:15`）—— 恰好就是争 `CellObjectMgr` key 的三种对象；而**活着的**调用只剩 `ActorEntity.ActorId`（`ActorEntity.cs:20`）与 `MapManager.RouteId`（`MapManager.cs:134`），两者都是非零序列，不碰 key 0。`StoneMineEvent` 也吃 `Id = 0`（`AddMineToEvent`/`AddToMapMineEvent` 都用 `stoneMineEvent.Id`），只因逻辑不可满足而不显形。建议合并成一条总条目（§10 的 T-201）。
- **真机锚（建议后续卡做一次）**：(i) `LoadMapDoor` 之后 key 0 的类型（预期 = 首个 `MapDoor`）；(ii) 矿清扫之后 key 0 是否变空；(iii) 一发火墙后 key 0 的类型与 `Visible`；(iv) 首个事件关闭后是否还有 804；(v) 门占位期间若存在事件格是否触发 `InvalidCastException`/`KickException`（§4.3 第 3 条）。

---

## 5. `SM_RECONNECT` 专项核查（卡面特别要求）

卡面问三件事：① 是否与登录/断线重连链路有关；② 本仓是否已有等价路径；③ 结论。

**① 语义澄清：不是「客户端断线重连」，而是「服务端让客户端换一个网关地址」**

- 载荷 = `"{ip}/{port}"`（`PlayObject.Base.cs:3417`、`WorldServer.cs:1074`、`ReconnectionCommand.cs:38`）；客户端收到后主动断开并用新 IP/端口重连（客户端冻结，不改）。
- 它**不参与**「掉线保活/会话恢复」：那条链是 `BoSoftClose`/`BoReconnection` + 存档（`PlayObject.Message.cs:380`），Rust 侧已实现（`lib.rs:4210-4260` 的 `CM_SOFTCLOSE`、`guild_runtime.rs:224` 的 `!bo_reconnection && bo_soft_close` 路径 A）。两者只共享 `BoReconnection` 这个**标志**。

**② 本仓等价路径核查（v2 订正：v1 的「`grep SwitchData` 零命中」是假证据）**

| 检查项 | 结果 |
|---|---|
| `!servertable.txt` 装载 | **无**（oracle 在 `GameApp.cs:267`；Rust 无对应装载） |
| `GetMultiServerAddrPort` | **无**（Rust 无多服地址表） |
| **`SwitchData` 换服状态机** | **已有脚手架**：`SessionGround.switch_state: SwitchDataState`（`lib.rs:14689-14695`）+ `struct SwitchDataState`（`lib.rs:14731`）+ `fn process_switch_data`（`lib.rs:14766`，1:1 复刻 `WorldServer.cs:592-610` 的换服存档门）——**生产闭包恒 false ⇒ 今日无出帧**，结论不变，但 G11 卡应**复用**它而不是「从零补」 |
| `TransferPlanesServer` | 无（且 oracle 唯一调用点已注释：`AppService.cs:179`） |
| 多服地图归属 | **部分有**：`bins/game-svr/src/castle.rs:116-130` 的 `server_index` 只用于城堡归属门（单服降级为 0） |
| GM `@Reconnection` | **表项存在、命令体未实现**：`gmcmd.rs:2500` `CommandDef{ name:"Reconnection", perm_min:10, params:2, imp: Imp::Unimplemented }` |
| 等价出帧 | 无（`sm_reconnect` 只有编码器定义与自测） |

⇒ 本仓**没有**产生 802 的路径；但**已有换服状态机的骨架**，缺口集中在「多服路由表 + 会话迁移 + 该状态机的生产闭包」。

**③ 判定与建议**

- **判定**：**需先补上游生产者**（G11 跨服搬家），**不接线**。
- 为什么不「先接 GM 那条」：`@Reconnection` 需 `Permission >= 10`，而本仓 1:1 复刻了「`GetHumPermission` 被注释 ⇒ 现网 `Permission` 恒 0」⇒ 生产路径永远停在 `权限不够!!!`。接了只能在测试里触发，对真机对拍零收益。
- **真生产面的判据（G11 卡的入口）**：`MapMgr.GetMapInfo(ServerIndex, MapName) == null` ⇒ `SendSwitchData` + `SendChangeServer`；要求 `!servertable.txt` 里该 `ServerIndex` 有 `IP/Port`，并复用 `process_switch_data`。

---

## 6. 两个特殊项的单独说明

### 6.1 `SM_CHANGEFACE`：功能在代码里、**不在现网内容里**

- 卡面要求「先有功能才有帧」，且不得混进可直接接线清单。v2 的结论与卡面**同向**，但把「功能有没有」拆成两层，因为它决定将来怎么复活：
  1. **代码层：有（但有一处硬编码）**。`crates/world/src/monai.rs:2893` `flush_sculture_spawns` 的 `MakeClone` 支确已在世界层发 `RM_CHANGEFACE`（`:2980-2988`，ident `:2983`）；`elf_warrior_run`（`:3815-3880`）已 1:1 复刻分裂支。宿主 `drain_monster_ref_msgs`（`lib.rs:15409`）缺臂，落 `:15527` `_ => continue`。**注意**：克隆名走的是 `pub const DRAGON1_NAME = "神兽"`（`:3632`），**不随配置变** ⇒ 只改内容不改这里，oracle 会造 114 而 Rust 仍造 113。
  2. **现网内容层：没有**。race-114（`ElfWarriorMonster`）的唯一构造点是 `WorldServer.MonGen.cs:666`，而现网没有任何路径生成 race-114（§3.2 的 P1..P7 七面全 ✘：`Dragon1=神兽`、MonGen/Envir 无 `神兽1` 精确 token、召唤链产出 113、GM perm 0）。
- ⇒ **现网两侧都不出帧** ⇒ 三态 = **不可达（现网部署级）**，不接线。
- **与 `SM_SKELETON` 的区别**：SKELETON 是「代码层就没有」（`Animal`/`BodyLeathery`/`Skeleton` + 效果体全缺）；CHANGEFACE 是「代码层都有、内容层不触发」。⇒ 前者要开卡补生产链，后者只需在内容面复活时补一条映射。
- **给 owner 的一句话**：若要真正支持换形（神兽攻击/跟随两态），需要三步：①把 `Server.conf` 的 `Dragon1` 改回 `神兽1`（内容面，须 owner 裁决）；②让 `crates/world` 的克隆名随配置（拆掉 `DRAGON1_NAME` 硬编码）；③补宿主臂。**只做①会在 oracle 与 Rust 之间先制造行为差异**，所以①②③要同批。

### 6.2 `SM_SKELETON`：确实「先有功能才有帧」，功能未实现

- `Skeleton` 是实体状态字段，`M2Server` 全仓唯一 setter = `ClientGetButchItem`（`PlayObject.Operate.cs:734`），三道门前置（`Death && !Skeleton && Animal`，`:720`）+ `BodyLeathery` 归零 + `Race ∈ [Animal, Monster)`（`:727-737`）。
- Rust 侧：`Entity` 无 `skeleton`/`body_leathery`/`animal`；`inbound::client_butch` 只做决策，效果体（`Random(16)+5`/`Random(201)+100`/`MeatQuality`/`BodyLeathery`/`Skeleton`）整段未移植。
- ⇒ **先有屠宰效果体，才有 `SM_SKELETON`**。**不得**用「已死就发 33」的近似替代（oracle 是 `Death && !Skeleton ⇒ 32`，`Death && Skeleton ⇒ 33`；近似会改变出现帧的 ident）。
- 与 `SM_CHANGEFACE` 不能合并归类（理由见 6.1 末）。

---

## 7. 优先级表（接线代价 × 可达性）

代价口径：**S** = 单文件、`bins` 内、无状态语义；**M** = 一个纯函数/一次结构扩展 + 一处映射 + 真实窗口；**L** = 跨 crate 契约或需新增字节通道；**XL** = 整子系统。
排序同时要求「现网可达」（§1.2 / v1 教训）。

| 名次 | 帧 | 现网可达性 | 现状缺口 | 代价 | 综合 |
|---|---|---|---|---|---|
| 1 | `SM_LAMPCHANGEDURA` 655 | **高**（装备右手火把即每 500ms 结算，人人可复现） | `UseLamp` 纯函数 + 调用点 | **M** | 单函数、自投递、无 AOI；**同窗口可带上 BREAKWEAPON** |
| 2 | `SM_SHOWEVENT` 804 / `SM_HIDEEVENT` 805 | **中**（一发火墙即建事件格；但只有「第一个 Visible 事件」的生命周期内有效） | 事件格 + `VisibleEvents` + `Id==0` 折叠 + key-0 绑定史 | **M**（世界侧） | 必须先按 §4 建模缺陷；真机锚 3 条待做 |
| 3 | `SM_BREAKWEAPON` 1102 | **中**（需先经升级 NPC 拿到 `Desc[10]`，再打一次且有目标） | `CheckWeaponUpgrade(Status)` + 攻击泵（含 `attackTarget != null` 门） | **M** | 同批牵扯 203/52/752 ⇒ 扩面风险 |
| 4 | `SM_SKELETON` 33 | **中**（屠宰 `Animal` 实体多次） | `Animal`/`BodyLeathery`/`Skeleton` + 屠宰效果体 | **M + R RNG** | 两次 `Random` 序要做 trace |
| 5 | `SM_QUERYYBSELL` 23021 / `SM_QUERYYBDEAL` 23023 | **中低**（需脚本 `QUERYYB*` 动作 + 已开通元宝） | 2 个动作体 + `ClientDealOffInfo` 编码 + **字节尾通道** | **L** | 受 `SendMsg.msg: String` 限制 |
| 6 | `SM_RECONNECT` 802 | **低/无**（G11 未移植；GM 面被 perm 0 锁死） | 多服路由 + 会话迁移（`process_switch_data` 已有） | **XL** | 挂 G11，不单开 |
| — | `SM_CHANGEFACE` 1104 | **无**（部署级：`Dragon1=神兽`） | 代码齐但克隆名硬编码（`DRAGON1_NAME`）+ 宿主缺一臂；**内容不触发** | 复活需三步、代价 **M**（见 §3.2） | 登记 + 复活条件（§6.1）；**不排期** |
| — | `SM_CLOSEDOOR` 614 | **无**（T-65） | — | — | 登记，不动 |
| — | `SM_OPENDOOR_OK` 612 | **无**（T-65） | — | — | 登记，不动 |
| — | `SM_ITEMSHOW` 610 | **无**（`ItemId` 恒 0） | — | — | 登记，不动 |

## 8. 建议分批

| 批 | 内容 | tier | 窗口 | 备注 |
|---|---|---|---|---|
| **B1（耐久族）** | `SM_LAMPCHANGEDURA` + `SM_BREAKWEAPON` | R | **同一窗口** | 两帧同属「耐久扣减 ⇒ 落库 ⇒ 帧」；一次窗口覆盖；顺带处理 `RM_CHANGELIGHT(654)`/`SM_UPDATEITEM(203)` 的关系（与 T-79/T-95 对齐，**不扩范围**） |
| **B2（事件族）** | `SM_SHOWEVENT` + `SM_HIDEEVENT` | R | 需窗口 | **先按 §4 建模 `first_event_only`**；判据钉死「第二次进格不出 804」+ 首个事件关闭后不出帧；真机锚 3 条 |
| **B3（屠宰族）** | `SM_SKELETON`（含 `CM_BUTCH` 效果体 + `Animal` 门） | R | 需窗口 | RNG 抽数序（2 次）与 `MeatQuality`/`BodyLeathery` 数值；建议独立卡（写集不同） |
| **B4（寄售族）** | `SM_QUERYYBSELL` + `SM_QUERYYBDEAL` | R | 需窗口 | 前置：脚本帧的**字节尾通道**（建议先单开一张基础卡） |
| **B5** | `SM_RECONNECT` | — | — | **不做**；挂 G11 跨服搬家卡，复用 `process_switch_data` |
| — | `SM_CHANGEFACE` | — | — | **不排批**：复活需三步同批（①内容 `Dragon1=神兽1` 须 owner 裁决；②`crates/world` 拆 `DRAGON1_NAME` 硬编码；③补宿主臂），代价 **M**；只做①+③ 会先制造行为差异 |

---

## 9. 与既有台账的差异（建议订正）

| 条目 | 既有表述 | 本卡实测 | 建议 |
|---|---|---|---|
| **T-175**（`SM_CHANGEFACE`） | 「上游 `ElfWarrior` 分裂支未移植」 | 两半都不准：**世界层已建模并已发 `RM_CHANGEFACE`**（`monai.rs:2893`/`:2980-2988`/`:3815-3880`），但**现网内容造不出 race-114**（`Dragon1=神兽`、MonGen/Envir 无 `神兽1`、召唤链→113、GM perm 0） | 改成「世界层已建模、现网内容不可达；宿主缺臂 + 克隆名硬编码（`DRAGON1_NAME`），复活需三步（§3.2）、代价 M」；**不接线** |
| **T-178**（`SM_SHOWEVENT/HIDEEVENT`） | 「`MapEvent` 可见性未建模」 | 成立，且必须补三条硬约束：① `MapEvent.Id` 恒 0 ⇒ 折叠；② `StoneMineEvent` 因 `AddToMapMineEvent` 逻辑不可满足而**从不注册**（`Envirnoment.cs:327/812`）；③ key 0 空 ⇒ 首个 `Visible` 事件占住且**关闭后永不移除** | 增补 §4 为接线**硬判据** |
| **T-179**（`SM_ITEMSHOW`） | 列 3 个生产者（`BaseObject.cs:570/752`、`PlayObject.cs:3264`） | **漏第 4 个**：`ExecutionProcessingSys.cs:5703`（`ActionOfThrowitem` 金币支） | 补生产者清单；结论不变 |
| **T-182**（`SM_RECONNECT`） | 「`TransferPlanesServer`/`SendChangeServer` 未移植」 | 漏三点：①`TransferPlanesServer` 唯一调用点已注释（`AppService.cs:179`）；②另有 GM `@Reconnection`（perm 10，现网 perm 0）；③`SendChangeServer` 走 `SendDefMessage` **绕过 RM 泵**；且本仓**已有** `SwitchDataState`/`process_switch_data` 脚手架 | 补齐；结论不变（不接线） |
| **T-65 / T-176**（门） | 门链不可达 | 独立复核一致 | 沿用，不新增 |
| **T-79⑤**（脚本帧不可达） | 「宿主 `NpcPlayer.frames` 没有通用 drain/映射」 | **已不成立**：`emit_script_frames`（`lib.rs:6210`）+ `pickwire::script_frame`（`:151`）已在，已映射 4 个 ident | YB 族真实缺口改为「动作体 + 字节尾通道」 |
| **T-79② / T-95①**（`SM_BREAKWEAPON`） | 登记「未建模武器升级」 | 一致；本卡补**两道门**：`attackTarget != null`（`Attack.cs:25`）+ `Desc[10]>0` | 沿用 |

---

## 10. 建议条目（交后续卡落地，本卡**不改** `whitelist.md`）

> **编号口径（v2 修正 v1 的撞号）**：本卡基线 `dbd03db97` 的 `whitelist.md` **最大已是 T-191**（T-184 searchtick、T-186..T-191 GM 第 4 批，T-185 空缺）；master `0119e47b9` 已到 **T-192**；**T-193 已被另一张卡预定**。⇒ 本文件用 **T-194 起**。合并时若仍有冲突，以「标题 + file:line」为准重新编号。

- **T-194 `SM_LAMPCHANGEDURA` 上游 `UseLamp`**：`PlayObject.Message.cs:2233`（emit `:2265`/`:2274`）；线上字段 = `Recog: Dura`（处理器 `:1843` + `SendDefMessage` 第 2 实参是 `nRecog`）；`DecLampDura` 缺省 true、`DecLightItemDrugTime` 缺省 500ms（本仓 `lib.rs:17024` 用缺省并已登记）。tier=R。
- **T-195 `SM_BREAKWEAPON` 上游 `CheckWeaponUpgrade`**：`PlayObject.cs:4110`/`:4146`；调用点 `PlayObject.Attack.cs:25-31`（**含 `attackTarget != null` 门**）；`Desc[10]` 生产面 = `Merchant.cs:682-736`（Rust `merchant.rs:1218` 已有写点，取回后是**死数据**）。tier=R。
- **T-196 事件族 + `MapEvent` 家族全穷举**：§4 的 4.1 表 + 4.2 的 `StoneMineEvent` 不可注册证明 + 4.3 的 key-0 生命周期；接线必须以「`first_event_only` + 关闭后不再替换」建模，否则多出 804。真机锚 3 条见 §4.5。tier=R。
- **T-197 `SM_SKELETON` 上游屠宰效果体**：`Operate.cs:720-742`（门 `Death && !Skeleton && Animal`，`Random(16)+5`/`Random(201)+100`/`BodyLeathery`/`Skeleton`）；`Animal` 的全部生产点见 §3.12；AOI 支 `PlayObject.cs:3231` 与 `RM_SKELETON` 支 `Operate.cs:736`。tier=R。
- **T-198 寄售族脚本动作 + 字节尾通道**：`ExecutionProcessingSys.cs:2443/2550`；阻塞点 = `bins/game-svr/src/npc.rs:109-116` 的 `SendMsg.msg: Option<String>` 载不了 `EDCode.EncodeBuffer` 的任意字节。tier=R。
- **T-199 `SM_RECONNECT` 三面穷举 + 既有脚手架**：①`TransferPlanesServer` 调用点注释（`AppService.cs:179`）；②`SendChangeServer` 走 `SendDefMessage` 绕过 RM 泵、依赖 `!servertable.txt`；③GM `@Reconnection` perm 10 被现网 perm 0 锁死；④**本仓已有** `SwitchDataState`/`process_switch_data`（`lib.rs:14731/14766`，生产恒 no-op）⇒ G11 卡复用。现网两侧皆不出帧。
- **T-200 `SM_CHANGEFACE` 现网内容级不可达（订正 T-175）**：代码层齐（`monai.rs:2893`/`:2980-2988`/`:3815-3880`；宿主 `lib.rs:15409` 缺臂）；内容层断（部署 `Server.conf:130`/`Server1.conf:130` `Dragon1=神兽`；MonGen/Envir 无 `神兽1` 精确 token；召唤链 `MagMakeSinSuSlave` 只产 113；GM perm 0）。复活 = **三步、代价 M**：①内容 `Dragon1=神兽1`（须 owner 裁决）②`crates/world` 的 `DRAGON1_NAME` 硬编码改为随配置注入（`slave.rs:252` 已有 `cfg.dragon1` 通路，克隆路径未走）③补宿主臂。**不是一条臂的活**。
- **T-201（合并建议）「`GetNextIdentity` 注释家族」**：全树只有 **3 处**被注释 —— `MapItem.ItemId`（`MapItem.cs:41`，T-19/T-25/T-32）、`MapEvent.Id`（`MapEvent.cs:62`，本卡；`StoneMineEvent` 同吃）、`MapDoor.DoorId`（`MapDoor.cs:15`，本卡 §4.3）⇒ 恰好是争 `CellObjectMgr` key 的三种对象（前两者恒 0、门也恒 0 ⇒ 互相抢 key 0）。合并成一条总条目，附「任何依赖这两个 id 的帧 + `CellObjectMgr` key 0 的三类占用者（Item/Event/Door）」白名单，避免逐帧重复发现。

---

## 11. 复现命令（证据四件套之一）

```sh
# ① 派发对账门
cd mir2-rs && python tests/parity/coverage/recon_gates.py dispatch --repo ..

# ② 全树常量扫描（面穷举的超集证据）
cd <repo>; for c in RM_BREAKWEAPON RM_CHANGEFACE RM_DOORCLOSE RM_HIDEEVENT RM_ITEMSHOW \
    RM_LAMPCHANGEDURA RM_DOOROPEN RM_QUERYYBDEAL RM_QUERYYBSELL RM_RECONNECTION \
    RM_SHOWEVENT RM_SKELETON; do echo "=== $c ==="; grep -rn "$c" src/ --include=*.cs; done

# ③ SM_CHANGEFACE：现网内容能否造出 race-114
sed -n '129,138p' /e/MirServer/M2GameSvr/Server.conf          # Dragon0..9 全 = 神兽
grep -n "Dragon1" /e/MirServer/M2GameSvr/Server1.conf
sed -n '1270,1301p' src/M2Server/Magic/MagicManager.cs        # MagMakeSinSuSlave -> 只产 神兽
python -c "import io,re;t='神兽1'.encode('gbk');p=re.compile(rb'(?:^|[\s\"\x27])'+re.escape(t)+rb'(?=[\s\"\x27!]|$)');print('hits',sum(len(p.findall(io.open(f,'rb').read())) for f in __import__('glob').glob('/e/MirServer/M2GameSvr/Envir/**/*.txt',recursive=True)))"
grep -n "new ElfWarriorMonster" src/GameSrv/Word/WorldServer.MonGen.cs   # 唯一构造点 :666

# ④ 事件族：MapEvent 家族 + StoneMineEvent 不可注册
grep -rn ": MapEvent\|new MapEvent(" src/ --include=*.cs
sed -n '322,336p' src/M2Server/Maps/Envirnoment.cs   # GetCellInfo: success 只在 Valid
sed -n '806,815p' src/M2Server/Maps/Envirnoment.cs   # AddToMapMineEvent: cellSuccess && !Valid
sed -n '32,53p' src/Modules/SystemModule/MagicEvent/Events/StoneMineEvent.cs
sed -n '430,447p' src/GameSrv/Maps/Maps.cs           # 逐格 new + 失败支 Remove(0)
sed -n '7,13p'   src/GameSrv/Maps/Maps.cs            # 后台线程
grep -n "MINE" /e/MirServer/M2GameSvr/Envir/MapInfo.txt   # 唯一矿图 D401 (line 636)

# ⑤ key-0 的三个占用者与相位
grep -rn "GetNextIdentity" src/ --include=*.cs          # ItemId / MapEvent.Id / MapDoor.DoorId 全被注释
sed -n '8,17p' src/Modules/SystemModule/Data/MapDoor.cs
sed -n '182,189p' src/GameSrv/Maps/MapManager.cs         # LoadMapDoor
sed -n '193,215p' src/M2Server/Maps/Envirnoment.cs       # AddMapDoor -> CellObjectMgr.Add(0, door)
sed -n '222,232p' src/GameSrv/GameApp.cs                 # InitializeWorld -> LoadMapDoor
sed -n '18,22p' src/M2Server/CellObjectMgr.cs            # Get<T> 的 (T)cell 强转

# ⑤b 不可达证明（id 恒 0 家族）
grep -rn "\.ItemId *=" src/ --include=*.cs
grep -o "INSERT INTO \`monsters\` VALUES ([0-9]*, '神兽[^']*'" sql/mir2_data.sql   # 无 神兽10
grep -n  "//Id = M2Share.ActorMgr.GetNextIdentity" src/Modules/SystemModule/MagicEvent/MapEvent.cs
sed -n '88,92p' src/M2Server/Maps/Envirnoment.cs

# ⑥ 门链（T-65）独立复核
grep -rn "SendDoorStatus\|Opened = true" src/ --include=*.cs
sed -n '363,377p' src/M2Server/Player/PlayObject.Operate.cs

# ⑦ 本仓侧缺口
cd mir2-rs
grep -n "match m.ident" bins/game-svr/src/lib.rs              # 15409
grep -n "_ => continue," bins/game-svr/src/lib.rs             # 15527
grep -rn "RM_CHANGEFACE" crates/world/src/monai.rs            # 2983（ident）
grep -n "fn flush_sculture_spawns\|fn elf_warrior_run" crates/world/src/monai.rs   # 2893 / 3815
grep -n "fn script_frame" bins/game-svr/src/pickwire.rs
grep -n "Reconnection" bins/game-svr/src/gmcmd.rs             # 2500
grep -n "struct SwitchDataState\|fn process_switch_data" bins/game-svr/src/lib.rs  # 14731/14766
grep -n "DRAGON1_NAME" crates/world/src/monai.rs              # 3632 = 神兽

# ⑧ 编号口径
grep -o "T-[0-9]\+" mir2-rs/tests/parity/whitelist.md | sed 's/T-//' | sort -n | tail -3
```

---

## 12. 残留物清单

- 本卡只新增 `docs/orphan-encoder-trigger-audit-20261023.md`（本文件）；**未改**任何代码 / 编码器 / 规则表 / `whitelist.md`。
- 部署件只**读**（`E:/MirServer/M2GameSvr/{Server.conf,Server1.conf,Envir/**}`），未写、未起服务端。
- worktree：`E:/tmp/wt-worker-32`（分支 `card/orphan-audit`），收尾由合并方按 DoD 清理。
- 未起任何服务端/客户端/假人进程；未占用 `E:\MirServer` 端口。
- 临时文件：无（grep/sed/python 输出仅在会话内，未落盘）。
