# 设计件：主地图门（MapDoor）与 MagicEvent/MapEvent 定时场（2026-10-16）

> 车道 **S3**（设计件，**不写代码**）。仓根 = `E:/Users/gxh/Documents/GitHub/OpenMir2`；本件写于 worktree
> `E:/tmp/doc-door`（`detached HEAD = master b08e065fc`），**只新增本文件，零改已有文件**。
> 覆盖原始 finding：`V4-04`（门 tick/对象未建模）、`V4-05`（MagicEvent/MapEvent 无 tick/无对应物）；
> 另参考 `E:/tmp/spec/critic.json` 的 `NL1`（主地图门整块未建模）。**不改任何内容/协议/客户端。**
>
> **本件最重要的一条是纠偏**（见 §0 与 §2.2）：oracle 的 `CM_OPENDOOR` **本身就是一个结构性 no-op**
> （开门口被一条写反的守卫堵死），Rust 侧现有的 `client_open_door(false)` 恒零出帧**已经是等价的**。
> 因此 V4-04 / NL1 把「门永远打不开」当成 Rust 缺口，**部分是假缺口**；真正残余的只是
> `ArroundDoorOpened` 对 MapRoute（传送点）的门控，而那要等 L1/TransMap 落地才可达。
> **建议登记条文草案（可直接粘进 `whitelist.md`）见 §2.2.1；本轮 S3-FIX 修正复核（fix#1–#4）见 §8。**

---

## 0. 一句话结论

- **门（MapDoor）**：oracle 的门子系统**除了「邻格有未开的门就挡住 MapRoute 传送」这一条之外全是死代码**
  （开门口 `ClientOpenDoor` 守卫写反 ⇒ 门永不开；`ProcessMapDoor`/`SendDoorStatus`/`SM_OPENDOOR_OK`/`SM_CLOSEDDOOR`
  因此全部不可达）。Rust 的 `.map` 门解析**已存在**（`crates/envir/src/mapfile.rs`，真图 `0.map` = 79 门/21 status），
  但**未接进 `crates/world`、无消费点**。⇒ 建议：**不实现真开门**（会偏离 oracle），只在 L1 落地时补
  `arround_door_opened` 那一条；其余按「oracle 本就 no-op 的照搬」登记 `whitelist`（tier=L 收口）——
  **可直接粘进 `whitelist.md` 的登记条文草案见 §2.2.1**。
- **MagicEvent/MapEvent**：oracle 有**两套**系统——**活的**是 `SystemShare.EventMgr` + `EventProcessor`(20ms)
  驱动的 `MapEvent` 家族（火墙/困魔咒/矿/立柱/挖出僵尸），**死的**是 `WorldServer.MagicEventList` + `ProcessEvents`(1s)
  （唯一填表点 `MagicManager.cs:1203` 被注释）。Rust 侧**两者都无**（只有 `CellType::MapEvent=5` 枚举占位与脚本名表），
  这是**真缺口**，承 R 级（火墙伤害每跳走 `GetMagStruckDamage` 消耗全局 RNG，涉 RNG + 时序 + 战斗数值）。

---

## 1. 范围、来源与实物清单

### 1.1 原始数据（只读，未改）

| 文件 | sha256 | 说明 |
| --- | --- | --- |
| `E:/tmp/spec/redlist2.json` | `30a5958e269dcb26e760959d7bd978cf94f6bbba9b414ee1a7ddcb80ab53d74e` | 五车道 60 finding 全量 JSON（V4-04/V4-05 原样在内） |
| `E:/tmp/spec/critic.json` | `dc16a19621b0096b203589511ea3abe11391df9c6f640b48393f5b688ef8c469` | 缺口界定批判者结论（NL1 主地图门；来源独立，勿与本件混淆） |

### 1.2 部署件真值（只读，本机实读）

| 路径 | 实测 | 结论 |
| --- | --- | --- |
| `E:/MirServer/M2GameSvr/Envir/MapDoor.txt` | **不存在**（`ls` → No such file） | oracle **不**从 `MapDoor.txt` 读门；`MapManager.LoadMapDoor()`（`MapManager.cs:182-189`）只是把**已从 `.map` 解出的**门 `AddDoorToMap()`（**非独立内容件**，与 `recon_gates.py:138` 的 `kind="other"` 口径一致） |
| `E:/MirServer/M2GameSvr/Map/0.map` | 700×700，**门 79 条 / 21 个 DoorStatus**（`crates/envir/tests/real_maps.rs:66-67`，由 owner 侧 `mapgen.dll probe` 定案） | 门是**真图实物**，不是空想 |
| `E:/MirServer/M2GameSvr/Envir/MapEvent.txt` | **存在但 0 字节** | oracle 不从此文件加载地图魔法场（无 loader 命中） |
| `E:/MirServer/M2GameSvr/Map/*.map`（`btDoorIndex` bit7） | 真图有门（见上） | 门的**唯一来源**是二进制 `.map` 的每格 `btDoorIndex` |

### 1.3 oracle 源（行号均本仓 `src/` 实测，sha256 见 §6）

门链：`src/M2Server/Maps/Envirnoment.cs`、`src/Modules/SystemModule/Data/MapDoor.cs`、`src/OpenMir2/Data/DoorStatus.cs`、
`src/GameSrv/Word/WorldServer.cs`、`src/GameSrv/Maps/MapManager.cs`、`src/GameSrv/GameApp.cs`、
`src/M2Server/Player/PlayObject.Message.cs`、`:Operate.cs`、`:PlayObject.cs`、`src/OpenMir2/Messages.cs`。

事件链：`src/GameSrv/Word/Threads/EventProcessor.cs`、`src/GameSrv/Word/Threads/SystemProcessor.cs`、
`src/M2Server/Event/EventManager.cs`、`src/Modules/SystemModule/MagicEvent/MapEvent.cs`（+`Events/`）、
`src/M2Server/Event/Events/`、`src/M2Server/Maps/Envirnoment.cs`、`src/M2Server/Magic/MagicManager.cs`、
`src/GameSrv/Word/WorldServer.cs`、`src/OpenMir2/Grobal2.cs`、
`src/Modules/ScriptEngine/Consts/ExecutionCode.cs`、`src/Modules/ScriptEngine/ScriptEngine.cs`。

### 1.4 Rust 现状源（行号本仓 `mir2-rs/` 实测，sha256 见 §6）

`crates/envir/src/mapfile.rs`、`crates/envir/src/registry.rs`、`crates/world/src/map.rs`、`crates/world/src/world.rs`、
`crates/script/src/codes.rs`、`bins/game-svr/src/inbound.rs`、`bins/game-svr/src/smframe_ext.rs`、
`bins/game-svr/src/main.rs`、`crates/protocol/src/messages.rs`。

---

## 2. 主地图门 MapDoor

### 2.1 oracle 全链（加载 → 对象 → 开/关 → tick → 帧 → 通行）

| 阶段 | oracle 锚（file:line） | 要点 |
| --- | --- | --- |
| **门对象** | `src/Modules/SystemModule/Data/MapDoor.cs:5-17` | `record struct MapDoor { int DoorId; short nX; short nY; DoorStatus Status; int n08; }`（`DoorId` 构造恒 0——`GetNextIdentity` 那行被注释） |
| **门状态** | `src/OpenMir2/Data/DoorStatus.cs:3-7` | `class DoorStatus { bool Opened; int OpenTick; }`（**引用类型 ⇒ 同组门可共享同一实例**） |
| **每图门表** | `src/M2Server/Maps/Envirnoment.cs:48`（`DoorList`）、`:64`（`new List<MapDoor>()`） | `IList<MapDoor> DoorList`（每图一份） |
| **从 .map 解门** | `Envirnoment.cs:934-991` | 每格读 `btDoorIndex`(`:936`/`:951`)：`btDoorIndex & 0x80 != 0` 且 `(idx & 0x7F) > 0` ⇒ 建 `MapDoor`(`:962`)；**Status 合并**：同图内 `|dx|<=10 && |dy|<=10 && n08 相同` ⇒ 复用同一条 `DoorStatus`(`:969-979`)；否则 `new DoorStatus{Opened=false,OpenTick=0}`(`:983-989`)；`DoorList.Add`(`:991`) |
| **门落格（可见）** | `Envirnoment.cs:193-218`（`AddMapDoor`）、`:298-304`（`AddDoorToMap`） | 往格 `ObjList` 追加 `CellObject{ CellType=CellType.Door(6), CellObjId=mapDoor.DoorId, AddTime=now }`（`:208-211`），并登记 `CellObjectMgr`(`:213-216`) |
| **门表注册** | `src/GameSrv/Maps/MapManager.cs:14`（`_mapDoorList`）、`:106-108`（`DoorList.Count>0` 才收）、`:59-62`（`GetDoorMapList`）、`:182-189`（`LoadMapDoor`）；`src/GameSrv/GameApp.cs:226` 调用 | 只有**有门**的图才进 `_mapDoorList`（`ProcessMapDoor` 只扫这张表） |
| **开门口（收包）** | `src/M2Server/Player/PlayObject.Message.cs:817-819` | `case CM_OPENDOOR: ClientOpenDoor(processMsg.nParam2, processMsg.nParam3)` |
| **开门口（实现）** | `src/M2Server/Player/PlayObject.Operate.cs:363-376` | `ClientOpenDoor(nX,nY)`：**`if (Envir.GetDoor(nX,nY,ref door)) return;`** → castle 门支 → `WorldEngine.OpenDoor(Envir,nX,nY)`（见 §2.2：守卫写反） |
| **开/关（引擎）** | `src/GameSrv/Word/WorldServer.cs:1445-1453`（`OpenDoor`）、`:1456-1465`（`CloseDoor`）、`:1467-1492`（`SendDoorStatus`） | `OpenDoor`：`if (envir.GetDoor(...) && !door.Status.Opened){ Opened=true; OpenTick=now; SendDoorStatus(...,RM_DOOROPEN,0,nX,nY); }`；`CloseDoor`：`if(!Opened)return; Opened=false; SendDoorStatus(...,RM_DOORCLOSE,...)` |
| **自动关 tick** | `src/GameSrv/Word/WorldServer.cs:155-159`（500ms 门）、`:1497-1515`（`ProcessMapDoor`） | 每 500ms（`GetTickCount()-ProcessMapDoorTick>500`）扫 `_mapDoorList` 的每图 `DoorList`：`door.Status.Opened && (now-OpenTick)>5*1000 ⇒ CloseDoor` |
| **tick 宿主** | `src/GameSrv/Word/Threads/SystemProcessor.cs:12`（50ms）、`:38`（`WorldEngine.Run()`） | `SystemProcessor` 每 50ms 调 `WorldServer.Run()`（`WorldServer.cs:131`）；`Run` 内 500ms 门触发 `ProcessMapDoor`（有效节奏 ≈550ms） |
| **RM→SM 出帧** | `src/M2Server/Player/PlayObject.Message.cs:1750-1755` | `RM_DOOROPEN(8046)` → `SendDefMessage(SM_OPENDOOR_OK(612),0,nParam1,nParam2,0)`；`RM_DOORCLOSE(8047)` → `SendDefMessage(SM_CLOSEDDOOR(614),…)` |
| **帧常量** | `src/OpenMir2/Messages.cs:281-283`（`SM_OPENDOOR_OK=612`/`SM_OPENDOOR_LOCK=613`/`SM_CLOSEDDOOR=614`）、`:673-674`（`RM_DOOROPEN=8046`/`RM_DOORCLOSE=8047`） | 两跳：引擎发 **RM_**（服务器内部 RefMsg，经玩家自己的消息段）→ 玩家处理段回 **SM_** 给客户端 |
| **通行门控** | `src/M2Server/Player/PlayObject.cs:1868-1892`（`Walk` 的 `CellType.MapRoute` 支）、`Envirnoment.cs:1185-1203`（`ArroundDoorOpened`） | **唯一活耦合**，且是**三级嵌套门**（缺一即分叉）：① `:1872` `ArroundDoorOpened(CurrX,CurrY)`（±1 内有未开门 ⇒ false ⇒ 挡住）；② `:1874` `(!Flag.boNEEDHOLE) \|\| (EventMgr.GetEvent(Envir,CurrX,CurrY,ET_DIGOUTZOMBI) != null)`；③ `:1876` `M2Share.ServerIndex == mapRoute.Envir.ServerIndex` ⇒ 相等走 `EnterAnotherMap`(`:1878`)、不等走 `DisappearA/ChangeSpaceMove`(`:1885-1887`) |
| **查门** | `Envirnoment.cs:1241-1254`（`GetDoor`） | 坐标**精确**匹配 `DoorList`，命中即 `ref door` 返回 true |
| **可走判定** | `Envirnoment.cs:442-486`（`CanWalk`）、`:334-431`（`MoveToMovingObject`） | **只**对 `cellObject.ActorObject==true` 的活体做阻挡（`CastleDoor` 有特判）；`CellType.Door` 的 CellObject **`ActorObject=false` ⇒ 不挡路** |

**结论（§2.1 汇总）**：门的「挡路」只发生在 `ArroundDoorOpened` 对 **MapRoute** 的门控上；**门格本身不挡走**。

### 2.2 oracle 行为真值：`CM_OPENDOOR` 是结构性 no-op（本件核心纠偏）

源码逐行（`PlayObject.Operate.cs:363-376`）：

```csharp
private void ClientOpenDoor(int nX, int nY) {
    MapDoor door = default;
    if (Envir.GetDoor(nX, nY, ref door)) { return; }   // ← 守卫写反：门存在即 return
    ... castle ...
    SystemShare.WorldEngine.OpenDoor(Envir, nX, nY);   // ← 到这里的路径，目标格必然「无门」
}
```

`WorldServer.OpenDoor`（`:1445-1453`）内再判 `if (envir.GetDoor(nX, nY, ref door) && !door.Status.Opened)` ⇒
**要求「有门」**。两条合起来：

- 目标格**有门** ⇒ `ClientOpenDoor` 第一行 `return`（不调 OpenDoor）；
- 目标格**无门** ⇒ 走到 `OpenDoor`，而 `OpenDoor` 内的 `GetDoor` 必失败 ⇒ **不置位**。

⇒ **`CM_OPENDOOR` 永远不开任何门**。推论：

1. **门永远 `Opened=false`**（全仓 `OpenDoor` 的生产调用者**唯一**是 `PlayObject.Operate.cs:373`；`grep` 证实）。
2. ⇒ **`ProcessMapDoor`(500ms) 永不关任何门**（只关 `Opened==true` 的门）；即 **500ms 门 tick 是死代码**。
3. ⇒ **`SendDoorStatus` 永不发帧**；`SM_OPENDOOR_OK(612)`/`SM_CLOSEDDOOR(614)` **永不出**。
4. ⇒ **唯一可观测后果** = `ArroundDoorOpened`（门恒未开）**永久挡住「与门相邻（±1）的 MapRoute 传送」**。

这就是 Rust `inbound.rs:22-30` 自陈的 **BUG-REPLICA 1**，且 Rust `client_open_door(false) → OpenDoorCallNoop`
（`inbound.rs:169-176`）**已经忠实复刻**该 no-op（零出帧）。**故原 finding 的「门打不开 = Rust 缺口」不成立。**

验证命令（只读）：

```text
grep -rn "ClientOpenDoor" src --include=*.cs      → 仅 Message.cs:818(调用) + Operate.cs:363(定义)
grep -rn "\.OpenDoor(" src --include=*.cs | grep -v BotSrv  → 仅 Operate.cs:373（唯一生产调用者）
grep -rn "SendDoorStatus" src --include=*.cs      → 仅 WorldServer.cs:1452/1464(调用) + 1467(定义)
grep -rn "MagicEventList" src --include=*.cs      → 仅 WorldServer.cs(字段/init/ProcessEvents) + MagicManager.cs:1203(注释)
```

> 保守口径：以上为**源码级**断言。**真机窗口**应以「门相邻的传送点场景」为首验（见 §2.6 W1），
> 若真机实测发现某条别的路径能开门（如未读到的反射/脚本），本节结论作废——故登记为 §2.8 首验项。

### 2.2.1 建议登记条文草案（**可直接粘进 `whitelist.md`**；本件**不写**该文件）

> 承 §2.2 结论（`CM_OPENDOOR` 结构性 no-op）。下面一行**照 `T-38`/`T-52` 体例**写就，可直接粘贴。
> **本件只把它写在这里，不去改 `mir2-rs/tests/parity/whitelist.md`**（该文件由别的车道在改）。
> 落地时**编号取当前 `whitelist.md` 最大 T 号 +1**（写作时其最大 = **T-59**，故草案先用 **T-60**；
> 若撞号按「后注册者让号」顺延，先例见 `whitelist.md` 的 T-51/T-54/T-55 撞号记录）。

| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |
| --- | --- | --- | --- |
| T-60 | **V4-04 / NL1 的「门打不开」不是 Rust 缺口，而是 oracle 的结构性 no-op 1:1 照搬**：`CM_OPENDOOR(1002)` 的 `ClientOpenDoor`（`src/M2Server/Player/PlayObject.Operate.cs:363-376`）**守卫写反**——`if (Envir.GetDoor(nX, nY, ref door)) { return; }`（`:366`，**门存在即 return**）；而 `WorldServer.OpenDoor`（`src/GameSrv/Word/WorldServer.cs:1445-1453`）内再判 `if (envir.GetDoor(...) && !door.Status.Opened)`（**要求有门**）。两条合起来 ⇒ 目标格有门则第一行 return、无门则 `OpenDoor` 内 `GetDoor` 必失败 ⇒ **`CM_OPENDOOR` 永不开任何门**（`OpenDoor` 的全仓唯一生产调用者是 `PlayObject.Operate.cs:373`）。推论：门永远 `Opened=false` ⇒ `ProcessMapDoor`（`WorldServer.cs:1497-1515`，500ms 门）**永不关任何门** ⇒ `SendDoorStatus`（`:1467-1492`）/`SM_OPENDOOR_OK(612)`/`SM_CLOSEDDOOR(614)` **永不出帧**。 | **不是差异，是 oracle 冻结 bug 1:1 照搬**（AGENTS.md 硬约束③「行为等价优先于修 bug」）：Rust 侧 `bins/game-svr` 维持 `client_open_door(false)` ⇒ `OpenDoorCallNoop`（`bins/game-svr/src/inbound.rs:169-176`，注释 BUG-REPLICA 1），**零出帧 = 与 oracle 逐字节等价**，**不许**「修好」成真开门（会与冻结客户端 + oracle 分叉）。`SM_OPENDOOR_OK`/`SM_CLOSEDDOOR` 编码器（`bins/game-svr/src/smframe_ext.rs:331-343`/`:139-149`）**登记为 orphan**（无生产发射，与 V2-01 同类）。**唯一活耦合** = `ArroundDoorOpened`（门恒未开）**永久挡住「与门相邻（±1）的 `MapRoute` 传送」**（`PlayObject.cs:1872`）；其 Rust 等价物 + `CellType::MapRoute` 支须与 L1/TransMap 同批落地，tier=**R**（时序）。举证要求：向量断言「`CM_OPENDOOR` 零出帧 + 门状态恒 `Opened=false`」+ **改坏必红**（把 `client_open_door` 的 `false` 改成 `true`/真开门 ⇒ 必红）。将来要修另开 `S-*`。 | 作者登记（`openmir2-worker-5`，S3 设计车道）；**待独立 principal 复核本登记行** |

> 同族可顺延单列（§7 已列，非本件写集）：`ProcessMapDoor`/`SendDoorStatus` 不可达（本行已含）；`WorldServer.ProcessEvents`(1s)+`MagicEventList` 死代码；`SafeEvent`/`MakeSafePkZone` 的 `MapName` 空守卫；`ADDMAPMAGICEVENT`/`RANDOMADDMAPMAGICEVENT` 无处理器。

### 2.3 Rust 现状（逐点，含**比 finding 更准确**的更正）

| 面 | Rust 锚 | 现状 |
| --- | --- | --- |
| `.map` 门解析 | `crates/envir/src/mapfile.rs:83-103`（`DoorStatus`/`MapDoor`）、`:265-296`（解析+Status 合并）、`:36-40`（常量） | **已实现**：`MapDoor{x,y,n08,door_id,status_id}` + `DoorStatus{opened,open_tick}` + 同组共享（`status_id` = 实例身份）。真图 `0.map`=79 门/21 status（`tests/real_maps.rs:66-67`） |
| 门数据保留 | `crates/envir/src/registry.rs:137-140`（`MapRegistry{maps:Vec<MapFile>}`）、`:220`（`maps()`）、`:258`（`get`） | **已装载并保留**（`MapFile.doors` 在 registry 内，生产宿主 `sim_loop` 拿得到 `registry`） |
| 门数据消费 | `grep "\.doors\|door_status\|MapDoor\|status_id" crates bins` | **零生产消费**（仅 `crates/envir/tests/*` 断言） |
| 门进 world | `crates/world/src/map.rs:16-31`（`CellType`，含 `MapEvent=5`/`Door=6`/`CastleDoor=11`）、`:169-172`（`MapGrid`）、`:188-200`（`to_grid`） | `MapGrid` **无门表**；`MapFile::to_grid()` **只拷属性、丢弃 doors** |
| world 无门逻辑 | `grep "ProcessMapDoor\|map_door\|arround_door\|door_open" crates/world bins` | 无 `try_step` 门支、无 500ms 门 tick、无 `ArroundDoorOpened` 等价物 |
| 开门口硬编码 | `bins/game-svr/src/main.rs:1463-1464`（`client_open_door(false)`） | 传 `false` ⇒ `OpenDoorCallNoop`，**零出帧**（注释：本宿主无门注册表；BUG-REPLICA 1） |
| 开门口逻辑 | `bins/game-svr/src/inbound.rs:22-40`（模块头 BUG-REPLICA 1）、`:169-176`（`client_open_door`） | 与 oracle 等价（见 §2.2） |
| SM 帧编码器 | `bins/game-svr/src/smframe_ext.rs:139-149`（`sm_closedoor`）、`:331-343`（`sm_opendoor_ok`） | **编码器已存在但 orphan**（无生产发射）——属 V2-01「幽灵帧」同类 |
| 协议常量 | `crates/protocol/src/messages.rs:41`（`CM_OPENDOOR=1002`）、`:258-260`（612/613/614）、`:524-525`（8046/8047） | 常量齐 |
| 对账门 | `tests/parity/coverage/recon_gates.py:138`（`LoadMapDoor: kind="other"`，无卡） | 门缺口对 content/dispatch 门**隐形**（critic false_negatives 已点名） |

**更正 finding 的措辞**：V4-04 说「crates/world map.rs … 无门对象」——**准确**；但「无门对象」**只在 crates/world**，
`crates/envir` **有**门对象且真图已解出 79 条。NL1 说「Rust… 无 `add_map_door`/`get_door`/`process_map_door`」——
在 `crates/world` 成立，但**少了**「envir 已解门」这一半。⇒ 设计只需**把已有的门数据接进 world + 补通行门控**，
**不需要**重写解析器。

### 2.4 差异判定：真缺口 vs 假缺口

| 条目 | 判定 | 理由 |
| --- | --- | --- |
| 开门不出帧 | **假缺口（Rust 已等价）** | oracle `CM_OPENDOOR` 亦不开门（§2.2）；Rust 零出帧 = 逐字节等价 |
| `ProcessMapDoor` 500ms 自动关门 | **假缺口（死代码等价）** | 无门会 `Opened=true` ⇒ 永不触发；Rust 不做 = 等价 |
| `SendDoorStatus` / `SM_OPENDOOR_OK` / `SM_CLOSEDDOOR` | **假缺口（不可达等价）** | 只被开/关调用，开/关不可达 |
| 门对象 + `btDoorIndex` 解析 | **已实现（envir）** | `crates/envir/src/mapfile.rs` 全链 |
| 门数据进 / 出 world | **真缺口（接线）** | `MapGrid` 无门表；`to_grid` 丢门 |
| `ArroundDoorOpened` 门控 MapRoute | **真缺口（但耦合 L1）** | oracle 活耦合；Rust 侧 **枚举已有、`try_step` 分支未建模**：`CellType::MapRoute = 4` **已存在**（`crates/world/src/map.rs:20`），但 `try_step` 无处理该变体的支、也无 `EnterAnotherMap`/`ChangeSpaceMove` 等价物 ⇒ 该耦合暂不可达。**实测 grep**（`grep -rni "MapRoute\|bo_needhole" mir2-rs/crates mir2-rs/bins --include=*.rs`）命中 **7 行**：`crates/envir/src/mapinfo.rs:83/149/428`（`bo_needhole` 字段/初始化/置位）+ `crates/envir/tests/csharp_parity.rs:68`（测试）+ `crates/world/src/map.rs:20`（`CellType::MapRoute` 枚举变体）+ `crates/world/src/session.rs:7` 注释 + `bins/game-svr/src/main.rs:5796` 注释（**末两处是注释、非代码**）。**原稿「仅 mapinfo.rs 的 flag 解析」为高估（漏枚举与注释），本轮 fix#3 订正** |
| 门格挡路 | **非缺口** | oracle `CanWalk`/`MoveToMovingObject` 只挡活体，门格不挡（§2.1） |

⇒ **门的真实残余 = （a）把 door 数据接进 world；（b）`ArroundDoorOpened` 等价物——但须与 L1/TransMap 同批。**
**不建议**实现真开门/真 500ms 关门（会偏离 oracle 的冻结 bug）。

### 2.5 Rust 侧建议模型（细到可照做）

> 原则：**只补「活耦合」`ArroundDoorOpened`**；其余以「oracle 本就 no-op 的照搬」登记，不写运行时逻辑。

**A. `crates/world/src/map.rs` —— 门表（接 `crates/envir` 已解出的数据）**

```rust
/// 复用 `crates/envir::mapfile::{MapDoor, DoorStatus}`（已是纯数据、`Copy`）。
/// 为避免 world→envir 反向依赖，按仓内既有做法在 world 定义**同形镜像**，由宿主搬运：
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoorStatus { pub opened: bool, pub open_tick: i32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapDoor { pub x: i16, pub y: i16, pub n08: u8, pub door_id: i32, pub status_id: usize }

pub struct MapGrid {
    // ... 既有 width/height/cells ...
    pub doors: Vec<MapDoor>,
    pub door_status: Vec<DoorStatus>,
}

impl MapGrid {
    /// 宿主 `bins/game-svr` 把 `registry.maps()[map_id].doors/door_status` 搬进来。
    /// 缺省空 ⇒ 行为与改动前逐字节相同（`arround_door_opened` 恒 true）。
    pub fn set_doors(&mut self, doors: Vec<MapDoor>, status: Vec<DoorStatus>);

    /// `Envirnoment.GetDoor`（`:1241`）：坐标精确匹配。
    pub fn get_door(&self, x: i32, y: i32) -> Option<MapDoor>;

    /// `Envirnoment.ArroundDoorOpened`（`:1185-1203`）：|dx|<=1 && |dy|<=1 的门有未开 ⇒ false。
    /// **本函数是门的唯一活耦合**（`try_step` 的 MapRoute 支用）。
    pub fn arround_door_opened(&self, x: i32, y: i32) -> bool;

    /// **严格照 oracle**（恒 false，登记为「冻结 no-op」）：
    /// `WorldServer.OpenDoor`（`:1445`）要求 `GetDoor && !Opened`，而唯一调用者已被 §2.2 守卫堵死。
    /// 保留签名与语义以便将来 owner 裁决「修 bug」时一字不动地启用。
    pub fn open_door(&mut self, x: i32, y: i32, now: i64) -> bool;

    /// `WorldServer.ProcessMapDoor`（`:1497-1515`）：500ms tick 关闭 `Opened && now-OpenTick>5000` 的门。
    /// **照搬**；因门恒未开 ⇒ 生产恒空。返回本 tick 关掉的门（供出 `RM_DOORCLOSE`→`SM_CLOSEDDOOR`）。
    pub fn process_map_door(&mut self, now: i64) -> Vec<(i32, i32)>;
}
```

> **同源性核对（本轮 fix#2）**：镜像字段与 `crates/envir/src/mapfile.rs:83-102` 的既有结构**逐字段同型**，避免口径分叉：
> `DoorStatus { opened: bool, open_tick: i32 }`——`open_tick` **是 `i32` 不是 `i64`**，对齐 envir 的
> `mapfile.rs:87`（`pub open_tick: i32`，对应 C# `DoorStatus.OpenTick` 的 `int`）；此前写 `i64` 是**把 world 的
> `clock_ms: i64` 时钟类型误套到状态字段上**（`world.rs:77`/`:134` 的时钟才是 `i64`），**已修正**。
> `MapDoor { x: i16, y: i16, n08: u8, door_id: i32, status_id: usize }`——与 `mapfile.rs:95-102` 逐字段同型
> （`x/y`=`i16`、`n08`=`u8`、`door_id`=`i32`、`status_id`=`usize`）。
> 只读函数参数 `now: i64` 与 `MapGrid` 的 `width/height: i32`、坐标 `i32`（`map.rs:170`/`:198`）**不受影响**：
> `now` 传的是 `clock_ms`（`i64`）；仅**状态字段** `open_tick` 保持 `i32`，比较时显式升位
> `now - i64::from(status.open_tick) > 5000`。

**B. `crates/world/src/world.rs::try_step` —— 接入 `CellType::MapRoute` 支（含三级门控）**

现状 `try_step`（`:1281-1322`）**只**做活体阻挡 + `move_object`，**无** `CellType` 分派（oracle `PlayObject.Walk`
的 `MapRoute`/`Event` 两支都没建模）。faithful 落地需要**先有 MapRoute**（属 L1）。落地后在此处加
**完整**的 `MapRoute` 支——**这是该功能唯一的设计口径**，逐条照 oracle `PlayObject.cs:1868-1892`。
门控**缺一即分叉**（原稿只写了 `arround_door_opened` 一条；**漏了批判者点名的三重条件——`boNEEDHOLE` / `ET_DIGOUTZOMBI` 事件门 / `ServerIndex` 相等检查——本轮 fix#1 逐条补全**）：

| 级 | oracle file:line | 条件 | 不满足时 |
| --- | --- | --- | --- |
| ① 门控 | `PlayObject.cs:1872` | `if (Envir.ArroundDoorOpened(CurrX, CurrY))`（±1 内有未开门 ⇒ false） | **整支跳过，不传送**（走位照常） |
| ② 事件门 | `PlayObject.cs:1874` | `if ((!mapRoute.Envir.Flag.boNEEDHOLE) \|\| (SystemShare.EventMgr.GetEvent(Envir, CurrX, CurrY, Grobal2.ET_DIGOUTZOMBI) != null))` | 跳过（目标图设了 `boNEEDHOLE` 且脚下无 `ET_DIGOUTZOMBI=1` 事件 ⇒ 不开洞、不传送） |
| ③ 服相等 | `PlayObject.cs:1876` | `if (M2Share.ServerIndex == mapRoute.Envir.ServerIndex)` | 相等 ⇒ ④a；**不等** ⇒ ④b |
| ④a 同服 | `PlayObject.cs:1878` | `if (!EnterAnotherMap(mapRoute.Envir, mapRoute.X, mapRoute.Y))` | 失败 ⇒ `result = false` |
| ④b 跨服 | `PlayObject.cs:1885-1887` | `DisappearA(); SpaceMoved = true; ChangeSpaceMove(mapRoute.Envir, mapRoute.X, mapRoute.Y);` | — |

> 注：③ 的**不等支**（④b）与**相等支**（④a）是**两套落地动作**（同服 `EnterAnotherMap` 换图；跨服 `DisappearA` + `ChangeSpaceMove`），
> 都不可省。② 依赖已建模的 `MapInfoFlag.bo_needhole`（`crates/envir/src/mapinfo.rs:83`）与 `ET_DIGOUTZOMBI`
> 事件注册表（§3 的 `EventRegistry`）；故 §2.5 B 的**完整落地须排在 §3 之后**（门控本身可先落）。

```rust
// oracle PlayObject.cs:1868-1892（Walk 的 CellType.MapRoute 支）——**须与 L1 同批**：
//   for cell in map.cell(e.x, e.y).obj_list where cell.cell_type == CellType::MapRoute:
//       let route = cell_obj_mgr.get_map_route(cell.cell_obj_id);   // :1869
//       if route.envir.is_null() { continue; }                      // :1870
//       if !map.arround_door_opened(e.x, e.y) { continue; }         // ① 门控 :1872（唯一活耦合；门恒未开 ⇒ 恒 false ⇒ 挡住）
//       if !(!route.envir.flag.bo_needhole                          // ② 事件门 :1874
//            || event_registry.get_event(map_id, e.x, e.y, ET_DIGOUTZOMBI).is_some()) { continue; }
//       if server_index == route.envir.server_index {               // ③ 服相等 :1876
//           if !enter_another_map(route.envir, route.x, route.y) { result = false; }  // ④a :1878
//       } else {
//           disappear_a();                                          // ④b :1885
//           space_moved = true;                                     //      :1886
//           change_space_move(route.envir, route.x, route.y);       //      :1887
//       }
```

**C. 500ms 门 tick —— 挂点与节奏**

- 挂点：`World::tick`（`world.rs:468`）里，在 **命令 FIFO 之后、`settle_deaths` 之前**（与 oracle
  `WorldServer.Run` 的 `ProcessMapDoor` 段同相位）。加 `door_pass_tick: i64` 字段（门 = `clock_ms - door_pass_tick > 500`）。
- 节奏失配（**未定项 U2**）：oracle `SystemProcessor` 50ms 宿主 + 500ms 内门 ⇒ 有效 ≈550ms；Rust `clock_ms`
  **每 tick +200ms**，`>500` 门会在 **第 3 tick（600ms）** 触发 ⇒ 与 oracle 差 ≤100ms。因门恒未开，**本 tick 在生产恒空**，
  此失配**当前不可观测**；仅在 owner 决定「修 bug 真开门」时才需对齐。

**D. 帧装配点（`bins/game-svr/src/main.rs::sim_loop`）**

- **开门帧**：**不接**。`main.rs:1463-1464` 维持 `client_open_door(false)`（= oracle 零出帧）。
  若要「照抄 oracle 分支」以留痕：把 `false` 换成 `world.maps[map_id].get_door(nX,nY).is_some()`，
  但仍**不发射** `sm_opendoor_ok`（因 `OpenDoorCallNoop` 恒不出帧）——**无净收益，建议保持现状**。
- **关门帧**：`World::tick` 后 `<门 tick 段>` 若 `process_map_door` 返回非空（生产恒空）⇒ 逐门
  `send_refmsg` 给 ±12 范围内玩家 → 玩家消息段 `RM_DOORCLOSE` → 回 `smframe_ext::sm_closedoor(nX,nY)`。
  编码器**已在**（`smframe_ext.rs:139-149`）；本段可**预接但不发射**（与既有 orphan 编码器同口径）。
- **关门广播范围**：oracle `SendDoorStatus`（`:1467-1492`）扫 `[nX-12, nX+12] × [nY-12, nY+12]`，对每个
  `Race==Play && !Ghost` 的活体 `SendMsg(自己, RM_DOOROPEN/CLOSE, 0, nDoorX, nDoorY, 0)`。

### 2.6 判据

**离线（tier=L 可收口；Rust 侧新增单测）**

- L-d1：`arround_door_opened(x,y)` —— 构造「door at (x+1,y) 未开」⇒ false；该门 `Opened=true` ⇒ true；
  距离 2 的门 ⇒ 不影响。对齐 `Envirnoment.cs:1185-1203` 逐行（±1 Chebyshev）。
- L-d2：`get_door` 坐标精确匹配（`Envirnoment.cs:1241-1254`）；无门 ⇒ None。
- L-d3：`set_doors` 空表 ⇒ `arround_door_opened` 恒 true（回归保护：改动前行为不变）。
- L-d4：`process_map_door(now)` 在「无门开」时恒返回空；构造「门 Opened 且 `now-OpenTick>5000`」⇒ 返回该门并置 `Opened=false`。
- L-d5：`.map` door 数据搬运保序（`registry.maps()[i].doors` → `MapGrid.doors` 顺序、`status_id` 身份不变）。
- **抽样冒烟（L 级要求 ≥1 条构造级）**：合成一张「门 + 相邻 MapRoute」小图，`try_step` 到 MapRoute 格：
  门未开 ⇒ 不传送；门开（测试注入）⇒ 传送。（**须先有 MapRoute**，否则只测到 `arround_door_opened` 单函数。）

**窗口（tier=R，占窗口；仅在「L1+门」同批落地后才需要）**

- W1（§2.8 首验）：真机 pin 一张**门相邻传送点**的图，两侧各跑「站到传送点」序列，逐帧比
  `SM_CHANGEMAP/SM_CLEAROBJECTS` 是否**同样被门挡住**。这是**唯一**能暴露门是否真活的判据。
- W2：门相邻**非传送**格走位——两侧应**都能走上门格**（门不挡路，`CanWalk` 只挡活体）。

### 2.7 未定项（需 owner / L1 车道裁）

- **U1**：是否**永久保留** `CM_OPENDOOR` 的 no-op（推荐：保留 + `whitelist` 登记，照 T-38/T-52 体例），
  还是单开批次「修 bug 真开门」（⇒ 需对齐 `SM_OPENDOOR_OK`/5s 自动关，且**会与冻结客户端行为分叉**，
  必须 owner 裁决）。
- **U2**：500ms 门 tick 的节奏对齐（200ms world tick vs 500ms 门，见 §2.5 C）。当前不可观测，**先不做**。
- **U3**：门数据搬进 `MapGrid` 的时机——每图一次性 `set_doors`（推荐）vs 懒加载。
- **U4**：`MapDoor.door_id` 恒 0（oracle 同），门身份用 `(x,y)`（`GetDoor` 精确坐标）——确认无需唯一 id。

### 2.8 must-verify-elsewhere（本件不能定论，须真机/他处确认）

1. **W1 首验**：真机确认 oracle 确实无任何其他开门路径（本结论基于 `grep` 全仓；BotSrv 有独立 `TMap.OpenDoor`
   但那是**机器人客户端模拟**，非 GameSvr 服务端逻辑，**不计入**）。
2. **门相邻传送点的存在性**：`0.map` 有 79 门；但「有门**且**相邻有 MapRoute」的图需真机枚举（决定 §2.4 最后一行是否真影响玩法）。
3. **`Envirnoment.GetEvent` 在门格上的语义**（`GetEvent` 只看 `CellType.Event`，与门无关）——若要复核门-事件共格。

---

## 3. MagicEvent / MapEvent 定时场

### 3.1 oracle 有两套系统：一活一死（本件纠偏之二）

| 系统 | 容器 | 驱动力 | 状态 |
| --- | --- | --- | --- |
| **活** `SystemShare.EventMgr` | `IEventSystem`（`src/M2Server/Event/EventManager.cs:5-41`），`Events`/`ClosedEvents` | `EventProcessor` **20ms**（`src/GameSrv/Word/Threads/EventProcessor.cs:7`） | 被火墙/困魔咒/矿/立柱/挖出僵尸/安全区**实际填充** |
| **死** `WorldServer.MagicEventList` | `IList<MagicEvent>`（`WorldServer.cs:48/86`），`MagicEvent{ObjectList,StartTick,Time,Events[8]}`（`src/M2Server/Event/Events/MagicEvent.cs:6-17`） | `WorldServer.ProcessEvents` **1s**（`WorldServer.cs:147-153`/`:1517-1556`） | **唯一填表点 `MagicManager.cs:1203` 被注释** ⇒ 列表恒空 ⇒ `ProcessEvents` 恒空转 |

> **纠偏**：V4-05 把「`EventProcessor`(20ms) 与 `WorldServer.ProcessEvents`(1s)」并列成一条，
> 但 **1s 那一半是死代码**（`MagicEventList` 永空）。Rust 只需复刻**活的** `EventMgr`+20ms 那套。
> （`WorldServer.ProcessEvents` 的 `Close()`/`Events[8]` 逻辑与 `MagicManager.cs:1203` 注释线，可整体登记为死代码。）

### 3.2 oracle 活系统全链

| 阶段 | oracle 锚 | 要点 |
| --- | --- | --- |
| **基类字段** | `src/Modules/SystemModule/MagicEvent/MapEvent.cs:14-58` | `Id; VisibleFlag; Envirnoment; nX; nY; EventType; EventParam; OpenStartTick(protected); ContinueTime(私有/readonly); CloseTick; Damage; OwnBaseObject; RunStart; RunTick; Closed; Visible; Active; IsDispose` |
| **构造** | `MapEvent.cs:60-85` | `OpenStartTick=now`；`ContinueTime=dwETime`；`Visible=boVisible`；`Active=true`；`RunStart=now`；**`RunTick=500`**；`if (Envirnoment!=null && Visible) Envirnoment.AddMapEvent(nX,nY,this) else Visible=false` |
| **Run** | `MapEvent.cs:96-107` | `if (now-OpenStartTick)>ContinueTime { Closed=true; Close(); }`；`if (OwnBaseObject!=null && (Ghost||Death)) OwnBaseObject=null;` |
| **Close** | `MapEvent.cs:109-123` | `CloseTick=now`；`if(!Visible)return;` `Visible=false;` `Envirnoment.DeleteFromMap(nX,nY,CellType.Event,Id,null); Envirnoment=null;` |
| **落格** | `src/M2Server/Maps/Envirnoment.cs:220-246`（`AddMapEvent`） | 往格 `ObjList` 追 `CellObject{CellType=CellType.Event(1), CellObjId=mapEvent.Id}`，登记 `CellObjectMgr` |
| **删格** | `Envirnoment.cs:635`（`DeleteFromMap`） | 按 `(CellType,CellObjId)` 命中删格 + `CellObjectMgr.Remove(id)`（`cellType != Monster` 时） |
| **查格** | `Envirnoment.cs:1344-1360`（`GetEvent`，只看 `CellType.Event`）、`src/M2Server/Event/EventManager.cs:21-35`（`GetEvent(envir,x,y,type)`） | 两处查事件（格子级 / 注册表级） |
| **20ms 驱** | `src/GameSrv/Word/Threads/EventProcessor.cs:27-60` | 倒序遍历 `Events`：`Active && (now-RunStart)>RunTick` ⇒ `RunStart=now; Run()`；`Closed` ⇒ 移入 `ClosedEvents` 并删；再倒序清 `ClosedEvents` 里 `now-CloseTick>5min` 的 |
| **火墙** | `src/Modules/SystemModule/MagicEvent/Events/FireBurnEvent.cs:9-44` | 内层门 `now-FireRunTick>3000`（**每 3s 一次**）：`Envir.GetBaseObjects(nX,nY,true,ObjectList)` 对每个 `OwnBaseObject.IsProperTarget(t)` 的 t `SendMsg(Owner, RM_MAGSTRUCK_MINE(8030), 0, Damage, 0, 0)`；末调 `base.Run()` |
| **困魔咒** | `src/M2Server/Event/Events/HolyCurtainEvent.cs:9-15` | 仅构造（无 Run 覆写）⇒ 只走 `base.Run` 的到点关闭 |
| **矿** | `src/Modules/SystemModule/MagicEvent/Events/StoneMineEvent.cs:9-56` | 构造里按 `nType∈{55,56,57}` 走 `AddMineToEvent`/否则 `AddToMapMineEvent`；`MineCount=Random(2000)+300`（或 `Random(200)+1`）、`_addStoneCount=Random(800)+100`（或 `Random(80)+1`），`Active=false`（**不参与 20ms 驱**） |
| **安全区** | `src/M2Server/Event/Events/SafeEvent.cs:10-22` + `MapManager.cs:25-53`（`MakeSafePkZone`） | `Run()` 每次把 `OpenStartTick=now` 再 `base.Run` ⇒ 永不关闭；**但** `MakeSafePkZone` 守卫 `if (IsNullOrEmpty(startPoint.MapName) && Type>0)` 后 `FindMap(MapName)` ⇒ **MapName 空 ⇒ envir==null ⇒ 从不添加**（又一处死路，登记） |
| **立柱** | `src/M2Server/Event/Events/PileStones.cs:6-20` + `PlayObject.Base.cs:1538-1568`（`PileStones`） | `EventParam=1`；挖矿时 `Random(MakeMineHitRate)==0` 才建，且 `AddEventParam()`（<5 时 ++） |
| **挖出僵尸/石雕** | `src/M2Server/Monster/Monsters/DigOutZombi.cs:59-63`、`:Monsters/ScultureKingMonster.cs:24-31` | `new MapEvent(Envir,CurrX,CurrY,ET_DIGOUTZOMBI/6, 5*60*1000, true)` + `AddEvent` |
| **火墙创建** | `src/M2Server/Magic/MagicManager.cs:1064-1096`（`MagMakeFireCross`） | 安全区门 ⇒ 在 `(x,y-1)(x-1,y)(x,y)(x+1,y)(x,y+1)` 五格各建 1 个 `FireBurnEvent`（`GetEvent==null` 才建），`AddEvent` |
| **困魔咒创建** | `MagicManager.cs:1146-1205`（`MagMakeHolyCurtain`） | 8 个别位建 `HolyCurtainEvent` + `AddEvent`（`:1180-1201`）；**`MagicEventList.Add` 被注释**（`:1203`） |
| **走上门格受伤** | `src/M2Server/Player/PlayObject.cs:1893-1910`、`src/M2Server/Actor/BaseObject.cs:1551-1566` | 走上 `CellType.Event` 格：`mapEvent.OwnBaseObject!=null && IsProperTarget(self)` ⇒ `SendMsg(Owner, RM_MAGSTRUCK_MINE, 0, mapEvent.Damage,0,0)` |
| **受击落点** | `src/M2Server/Actor/BaseObject.Base.cs:618-670` | `RM_MAGSTRUCK(_MINE)` ⇒ `GetMagStruckDamage(null,param1)`（`:812-821`，**含 `RandomNumber.Random(...)`**）⇒ `StruckDamage` ⇒ `RM_STRUCK_MAG` |
| **事件类型** | `src/OpenMir2/Grobal2.cs:41-49` | `ET_DIGOUTZOMBI=1; ET_MINE=2; ET_PILESTONES=3; ET_HOLYCURTAIN=4; ET_FIRE=5; ET_SCULPEICE=6` |
| **脚本命令** | `src/Modules/ScriptEngine/Consts/ExecutionCode.cs:586-589`（`ADDMAPMAGICEVENT`/`RANDOMADDMAPMAGICEVENT`） | **注册表 `ProcessExecutionMessage` 无对应项**（`grep -ri addmapmagicevent src --include=*.cs` 仅命中这 2 行）⇒ 该脚本命令在 oracle **无处理器 = no-op**（照搬登记） |

### 3.3 Rust 现状

| 面 | Rust 锚 | 现状 |
| --- | --- | --- |
| 事件类型枚举 | `crates/world/src/map.rs:21`（`CellType::MapEvent=5`） | **仅枚举占位**（oracle `CellType.MapEvent` 也**从未被使用**；事件格用 `CellType.Event=1`） |
| `CellType::Event` | `crates/world/src/map.rs:17` | 枚举在；**无**事件落格/删格/查询逻辑 |
| 脚本名表 | `crates/script/src/codes.rs:522-523`（`ADDMAPMAGICEVENT`/`RANDOMADDMAPMAGICEVENT`） | 名表登记；**无处理器**（与 oracle 一致 = no-op） |
| MapEvent 对象 | `grep -rniE "MagicEvent\|magic_event\|EventMgr\|MapEvent\|FireBurn\|HOLYCURTAIN\|ET_FIRE" crates bins` | 仅 `codes.rs:522-523` + `map.rs:21`；**无对象/注册表/tick** |
| 20ms / 1s 驱 | `grep "EventProcessor\|ProcessEvents" crates bins` | 无 |
| 火墙伤害帧 | `crates/protocol/src/messages.rs:515`（`RM_MAGSTRUCK_MINE=8030`）、`:522`（`RM_MAGSTRUCK=8044`）、`:581`（`RM_MAKEHOLYSEIZEMODE=60005`） | 常量在；`RM_MAKEHOLYSEIZEMODE` 无编码器（登记在 `main.rs:5130`） |
| 技能侧 | `crates/world/src/magic.rs:263`（`power_fire_wall`）、`:232`（`power_holy_shield`） | 只有**取数**（RNG 计算）；**无** `FireBurnEvent`/`HolyCurtainEvent` 对象创建 |

### 3.4 Rust 侧建议模型（细到可照做）

**A. 新模块 `crates/world/src/map_event.rs`（或并入 `map.rs`）**

```rust
pub const ET_DIGOUTZOMBI: u8 = 1;
pub const ET_MINE: u8 = 2;
pub const ET_PILESTONES: u8 = 3;
pub const ET_HOLYCURTAIN: u8 = 4;
pub const ET_FIRE: u8 = 5;
pub const ET_SCULPEICE: u8 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapEventKind { Fire, HolyCurtain, Mine, PileStones, Plain /*+ Safe 登记为死*/ }

#[derive(Clone, Debug)]
pub struct MapEvent {
    pub id: i32,
    pub map_id: usize,
    pub x: i16, pub y: i16,
    pub visible_flag: u8,
    pub event_type: u8,             // ET_*
    pub event_param: i32,           // PileStones 用
    pub open_start_tick: i64,       // 构造时 = clock_ms
    pub continue_time: i32,         // dwETime
    pub close_tick: i64,
    pub damage: i32,
    pub own_base_object: Option<i32>,
    pub run_start: i64,
    pub run_tick: i32,              // 缺省 500
    pub kind: MapEventKind,
    pub fire_run_tick: i64,         // Fire 专用（缺省 0；每 3000ms 一次）
    pub mine_count: i32, pub add_stone_count: i32, // Mine
    pub closed: bool, pub visible: bool, pub active: bool,
}

#[derive(Default)]
pub struct EventRegistry { events: Vec<MapEvent>, closed_events: Vec<MapEvent> }

impl EventRegistry {
    /// `EventManager.AddEvent`（`:37-40`）：**尾插**（顺序 = 加入序）。
    pub fn add_event(&mut self, e: MapEvent);
    /// `EventManager.GetEvent(envir,x,y,type)`（`:21-35`）：倒序首个匹配。
    pub fn get_event(&self, map_id: usize, x: i32, y: i32, et: u8) -> Option<&MapEvent>;
}

impl World {
    /// `EventProcessor.ExecuteInternal`（`EventProcessor.cs:27-60`）：**20ms 节奏**。
    /// 倒序遍历：`active && clock_ms-run_start > run_tick` ⇒ `run_start=clock_ms; run_map_event(i)`；
    /// `closed` ⇒ 移 `closed_events`；再清 `closed_events` 里 `clock_ms-close_tick > 300_000` 的。
    /// 返回本 tick 需出帧/落格/删格的动作。
    pub fn event_processor_tick(&mut self) -> Vec<EventAction>;

    /// 单个 `MapEvent.Run`（`MapEvent.cs:96-107`）+ 类型特化（Fire 的 3000ms 内层门）。**含 RNG**（火墙伤害）。
    fn run_map_event(&mut self, i: usize);

    /// `Envir.AddMapEvent`/`DeleteFromMap`（`Envirnoment.cs:220-246`/`:635`）：Event 格增删。
    fn add_event_cell(&mut self, map_id: usize, x: i32, y: i32, id: i32);
    fn del_event_cell(&mut self, map_id: usize, x: i32, y: i32, id: i32);
}
```

**B. 20ms 节奏 vs 200ms world tick（未定项 U2，R 级）**

oracle `EventProcessor` 是 **20ms** 独立线程；Rust `World::tick` 是 **200ms**。若在 world tick 里直接门 `>500ms`：

- `MapEvent.Run`（`run_tick=500`）有效周期 = **600ms**（3 tick）vs oracle ≈500–520ms；
- `FireBurnEvent` 的 3000ms 内层门 = **3000/3200ms** vs oracle ≈3000–3020ms。

⇒ **建议（对齐更紧）**：在 `World::tick` 内把 200ms 切成 **10 个 20ms 子步**（`for sub in 0..10 { clock += 20; event_processor_tick() }`），
使 `RunStart`/`FireRunTick` 的推进与 oracle 同粒度。**但**这会改 `clock_ms` 的推进口径（现为 200 步进）——
**须 owner 裁**（U2）。**保守备选**：不做子步，直接 200ms 门（RNG 面因火墙伤害取数会**整体错位**，见 §3.6 U5）。

**C. 事件落格（AOI/走格）**

- 建 `MapEvent` 且 `visible` ⇒ `add_event_cell`（`CellType::Event`）。
- `Close()` ⇒ `del_event_cell`。
- **走上门格受伤**：`try_step` 落格后，`if cell has CellType::Event { let ev = ...; if ev.own_base_object.is_some() && is_proper_target(owner, self) { emit RM_MAGSTRUCK_MINE→(SM_?) } }`。
  （oracle `PlayObject.cs:1893-1910` / `BaseObject.cs:1551-1566`。）

**D. 帧装配点（`bins/game-svr/src/main.rs::sim_loop`）**

- `World::tick` 后新增 `drain_event_downlink(&rec, &world, &online, &gates)`：把本 tick 的
  `RM_MAGSTRUCK_MINE`（8030）按 oracle 次序（先 `Run` 伤害、后走格伤害）出帧；火墙受击落点见
  `BaseObject.Base.cs:618-670` → `RM_STRUCK_MAG`（→ 客户端 `SM_STRUCK` 族，复用既有战斗下行）。
- 顺序：oracle 中 20ms 事件线程与玩家 `Run` 线程**交错不可复现**（与 M3⑤ 商人 tick 同由 owner 裁决「不在判据内」）；
  Rust 建议：**放在 `settle_deaths` 之后、`monster_ai_tick` 之前**（与商人/AI 段并列，定序显式）。

**E. 火墙创建（技能侧）** `crates/world/src/magic.rs`

- `power_fire_wall`（`:263`）已算伤害；补 `MagMakeFireCross` 等价物：安全区门 + 五格 `GetEvent==null` 才建
  `FireBurnEvent`（`MagicManager.cs:1064-1096`）；困魔咒补 `MagMakeHolyCurtain`（8 位，`:1146-1205`）。

### 3.5 判据

**离线（tier=L 可覆盖逻辑；Rust 新增单测 + 夹具）**

- L-e1：`MapEvent.Run` 到点关闭（`now-OpenStartTick>ContinueTime` ⇒ `Closed=true`）逐行对齐 `MapEvent.cs:96-102`。
- L-e2：`event_processor_tick` 的倒序遍历 + `RunStart` 重置 + `Closed` 搬移 + 5min 清 —— 对齐 `EventProcessor.cs:27-60`（含遍历方向）。
- L-e3：`add_event_cell`/`del_event_cell` 的格子增删（`AddMapEvent`/`DeleteFromMap`）——格内 `(CellType,CellObjId)` 命中。
- L-e4：`FireBurnEvent` 内层 3000ms 门（`FireBurnEvent.cs:25`）——到点才伤、未到点空转。
- L-e5：`ET_*` 常量与 `MapEvent` 字段默认值（`RunTick=500`/`Active=true`/`Closed=false`）对齐 `MapEvent.cs:60-85`。

**窗口（tier=R，占窗口）**

- W3（**RNG + 数值 + 时序三占**）：真机放火墙 → 站入火格 → 逐帧比 `RM_MAGSTRUCK_MINE(8030)→SM_STRUCK` 的
  **伤害值 + 每跳时刻 + RNG 抽数位置**（`GetMagStruckDamage` 每受击抽 1 次 `Random`）。**必须钉子种子**（RNG 逐次流水对拍）。
- W4：走上门格受伤的**时序**（`PlayObject.Walk` 落格当帧发 RM_MAGSTRUCK_MINE）与 `Run` 伤害的**相对次序**。
- W5：矿（`StoneMineEvent` 的 `Random(2000)+300`/`Random(800)+100`）与立柱（`Random(MakeMineHitRate)`）的
  **建场 RNG 抽数**位置（这两处在**建场**时抽，属「生成时预算」，与 D3 掉落同性质）。

### 3.6 未定项与风险

- **U5**：20ms 子步 vs 200ms 门（§3.4 B）——**R 级 RNG 位点风险**：不做子步 ⇒ 火墙每跳时刻整体偏 ≤200ms ⇒
  `GetMagStruckDamage` 的 RNG 抽数**与其他系统的 RNG 流交错次序**可能错位。**强烈建议先做子步或显式声明窗口口径**。
- **U6**：`MapEvent.Id` 的唯一性——oracle `Id = GetNextIdentity()` 被注释 ⇒ `Id` 恒 **0**（构造里 `//Id = ...`）。
  但 `AddMapEvent` 用 `mapEvent.Id` 作 `CellObjId`、`CellObjectMgr.Add(0, ...)` ⇒ **多条事件同 Id=0 冲突**。
  Rust 是否照搬「Id 恒 0」需裁（照搬 ⇒ 格子级 `GetEvent` 会拿到错误对象——**这本身是 oracle 的既有 bug**，建议照搬+登记）。
- **U7**：`SafeEvent`/`MakeSafePkZone` 的 `MapName` 空守卫使其**从不生效** ⇒ 建议**照搬 no-op 并登记**（同 NL3/NL4 型）。
- **U8**：`WorldServer.ProcessEvents`(1s) + `MagicEventList` 死代码 —— 是否也照搬（建议：**不实现**，whitelist 登记「oracle 死代码」）。

### 3.7 must-verify-elsewhere

1. `MapEvent.Id` 恒 0 是否真导致 `CellObjectMgr` 冲突（需真机或 C# 侧单测复核；`ActorMgr.GetNextIdentity` 被注释是源码事实）。
2. 火墙 `IsProperTarget` 的完整判据（含 PK 模式/安全区）在 Rust 的对应（复用 `crates/world/src/combat.rs:589` `is_proper_target`）。
3. `GetBaseObjects(nX,nY,true,...)`（`Envirnoment`）在 Rust 的等价扫描（现有 `search_view_range` 是否够用）。

---

## 4. 与其它批次的写集冲突面（**串行提示**）

两件都落在**同一批热点文件**上，且与多条在跑的线重叠 ⇒ **必须串行或同 owner**：

| 文件 | 本件用途 | 冲突车道 |
| --- | --- | --- |
| `crates/world/src/map.rs` | 加 `MapDoor`/`DoorStatus`/门函数、（事件落格） | **L1/TransMap**（`CellType::MapRoute`/传送）、V4-01/02（`GuildMgr`/`CastleMgr` 也动 `world.rs`）、V4-03（buff）、V4-08（NPC tick）、V4-09（GameTime） |
| `crates/world/src/world.rs` | `try_step` 门控 + `World::tick` 新段 + `EventRegistry` 字段 | **V4-01/02/03/08/09**（都在 `world.rs::tick` 里抢段）、商人/AI/掉落各线 |
| `crates/world/src/magic.rs` | 火墙/困魔咒建场 | **V4-05 同族**（魔法线）、M3 技能线 |
| `bins/game-svr/src/main.rs` | `sim_loop` 门/事件 tick 段 + 帧装配 | **几乎所有出站接线线**（V2-01/V2-15 等） |
| `crates/envir/src/mapfile.rs` | 门解析**已存在**（本件只读，不改） | — |

**建议排期**（沿用 critic.json 的并行建议）：门/事件属「车道组2（地图对象与定时场）」，与组1（世界 tick 扩容：buff/NPC/GameTime）
**争 `world.rs::tick`** ⇒ **串行**；`crates/world/src/map.rs` 的门控须**等 L1/TransMap 先落 MapRoute**，否则门控不可达。

---

## 5. 结论与建议动作（给 owner / 承卡）

1. **门**：
   - **不做**真开门/真 500ms 关门（等价于 oracle 冻结 no-op）；把 `CM_OPENDOOR` 的 no-op **写进 `whitelist.md`**
     （照 T-38/T-52 体例，当前 `grep -ci opendoor whitelist.md` 待补），`SM_OPENDOOR_OK`/`SM_CLOSEDDOOR` 编码器
     **登记为 orphan**（V2-01 同类）。
   - **做**（与 L1 同批）：把 `registry.maps()[i].doors` 搬进 `MapGrid` + 补 `arround_door_opened`，
     接进 `try_step` 的 MapRoute 支。tier=**R**（时序），但**依赖 L1**。
2. **事件**：建 `EventRegistry` + `MapEvent` 家族 + 20ms（子步）驱 + 事件落格 + 走格受伤，tier=**R**（RNG+数值+时序）；
   `WorldServer.ProcessEvents`/`MagicEventList` 死代码**不实现**、登记；`SafeEvent`/`ADDMAPMAGICEVENT` 照搬 no-op 登记。
3. **对账门**：把 `recon_gates.py` 的 `LoadMapDoor`（`:138`）从 `kind="other"` 改为**可追踪卡**（否则门缺口对 content 门隐形，
   见 critic false_negatives）——**属判据修正线**，不在本件写集。

---

## 6. 证据四件套（命令 + 输出摘要 + 路径 + sha256）

**命令**（均只读；未起服务、未占端口、未跑 cargo）：

```bash
# oracle 门链
grep -rn "ClientOpenDoor" src --include=*.cs
grep -rn "\.OpenDoor(" src --include=*.cs | grep -v BotSrv
grep -rn "SendDoorStatus" src --include=*.cs
grep -rn "CellType.Door\b" src --include=*.cs
sed -n '1445,1515p' src/GameSrv/Word/WorldServer.cs          # OpenDoor/CloseDoor/SendDoorStatus/ProcessMapDoor
sed -n '363,376p'  src/M2Server/Player/PlayObject.Operate.cs  # ClientOpenDoor（守卫写反）
sed -n '1241,1254p' src/M2Server/Maps/Envirnoment.cs         # GetDoor
sed -n '1185,1203p' src/M2Server/Maps/Envirnoment.cs         # ArroundDoorOpened
sed -n '934,991p'  src/M2Server/Maps/Envirnoment.cs          # .map 门解析 + Status 合并
# oracle 事件链
grep -rn "MagicEventList" src --include=*.cs                  # 唯一填表点 MagicManager.cs:1203 被注释
grep -rni "addmapmagicevent" src --include=*.cs               # 仅 ExecutionCode.cs:586-589（无处理器）
sed -n '27,60p' src/GameSrv/Word/Threads/EventProcessor.cs
sed -n '9,152p' src/Modules/SystemModule/MagicEvent/MapEvent.cs
# 部署件
ls E:/MirServer/M2GameSvr/Envir/MapDoor.txt                   # Not found
wc -c E:/MirServer/M2GameSvr/Envir/MapEvent.txt               # 0
# Rust 现状
grep -rn "\.doors\|door_status\|MapDoor\|DoorStatus" mir2-rs/crates mir2-rs/bins --include=*.rs
sha256sum <各文件>                                             # 见下表
```

**输出摘要**：`ClientOpenDoor` 仅 2 处（Message.cs:818 调用 + Operate.cs:363 定义）；`.OpenDoor(` 生产调用者**唯一** =
`Operate.cs:373`；`SendDoorStatus` 仅 WorldServer.cs:1452/1464 调用；`CellType.Door` 仅 `Envirnoment.cs:208/213` +
`CastleDoor.cs:20` ⇒ **门格不挡路**；`MagicEventList` 唯一填表点 `MagicManager.cs:1203` 被注释 ⇒ **1s ProcessEvents 恒空**；
`MapDoor.txt` 不存在、`MapEvent.txt` 0 字节。

**路径 + sha256（oracle）**：

| 路径 | sha256 |
| --- | --- |
| `src/M2Server/Maps/Envirnoment.cs` | `2f8c2f47e209599b3ae31e74ad6e9d0a0ed25639f803e43ce20cf7f1322b1317` |
| `src/Modules/SystemModule/Data/MapDoor.cs` | `9f5809295eda61aa3d5b6745aedf862f6cc8cf8eaccc6cdc7e6e3c7531128e3b` |
| `src/OpenMir2/Data/DoorStatus.cs` | `1b7e17f262556d7f4fd456c2d6dd2a794d40843387dc50cf7557bf5ede0c1dac` |
| `src/GameSrv/Word/WorldServer.cs` | `104ad00849d550b931bc328f4632105bcdb33372d2331f62fe75d32d4bdf20ff` |
| `src/GameSrv/Maps/MapManager.cs` | `8ac763ea6649a84a3807cb642174b1e87011641d70f3b1091819d22b9ff93540` |
| `src/M2Server/Player/PlayObject.Operate.cs` | `e23b446065ff2c7995940f8dd31642de81aa1a3fae9ef51a08ee9ca0e7ea1081` |
| `src/M2Server/Player/PlayObject.Message.cs` | `a1ff3fc303b89b87440a55a1012d02186b7ca581a6c49e55ef3a402d55e01ae1` |
| `src/M2Server/Player/PlayObject.cs` | `543d59892d5f4bee0aaf9005b93fde9b3b5b3e512f0d8d7abf8a14d13f75feb1` |
| `src/GameSrv/Word/Threads/EventProcessor.cs` | `30e63f8bf1fc1700f30b935c6218152216b8d3f6a541be8a86f59a7eaa88dff1` |
| `src/M2Server/Event/EventManager.cs` | `a945e1a11a76a20144427d20310d0e485e141b2ad745484b3142f0bb5372f401` |
| `src/Modules/SystemModule/MagicEvent/MapEvent.cs` | `9a0157dbfdc4d5e73fd3fd897f0c3f1f9a1e52ebf742d3efbbb835e64411335e` |
| `src/Modules/SystemModule/MagicEvent/Events/FireBurnEvent.cs` | `e70ecc2ef8ecf49f6a957fa2a4291024ad4679699516e449de5b427ba4ff112a` |
| `src/Modules/SystemModule/MagicEvent/Events/StoneMineEvent.cs` | `34227b0482fe78f3b70d899a9b6839567e807a85bae2f235002dc47824beb6ca` |
| `src/M2Server/Magic/MagicManager.cs` | `34ad86ecd7ddf5f2bc76bafb424b2d57db96304d16146e0007fe4994b65a0d4c` |
| `src/OpenMir2/Messages.cs` | `4eb4ead899de83218984de784c7c7e1a9450c821213ba1bbedfcb189af8a8dfc` |
| `src/OpenMir2/Grobal2.cs` | `be815bb3e0b2456e671c452562fff8cc8984f7c1bde3d579882097499905c2c7` |

**路径 + sha256（Rust，`E:/tmp/doc-door/mir2-rs/`）**：

| 路径 | sha256 |
| --- | --- |
| `crates/world/src/map.rs` | `2e68cd7417f60459308c20d8592c284233a996f6223aad7a29a00406068d4db0` |
| `crates/envir/src/mapfile.rs` | `ad1f9ecdc2dbae344b300412fa4348acd5721e8c711aa4a8683264ebd99c9321` |
| `crates/envir/src/registry.rs` | `9d22ccc2673de034e9b06ea45db27e545ac8a115dd2f7a3767c967cb69ebe9a2` |
| `crates/script/src/codes.rs` | `6409970f163a5963e7723c413919c4d58150d86671304a24f5ee5cdd8f8d2fd6` |
| `bins/game-svr/src/inbound.rs` | `63db9d4450f31573f341d6412cb39c941bfc533b99673c1acd23bfc6567c1d47` |
| `bins/game-svr/src/smframe_ext.rs` | `3e6e34775cbc8d4b66a15391500caa012a13180945806beb806e9899146fe2fb` |

**原始数据**：`E:/tmp/spec/redlist2.json` = `30a5958e269dcb26e760959d7bd978cf94f6bbba9b414ee1a7ddcb80ab53d74e`；
`E:/tmp/spec/critic.json` = `dc16a19621b0096b203589511ea3abe11391df9c6f640b48393f5b688ef8c469`。

---

## 7. 残留物清单 / 登记建议

- 本件**只新增**本文件；**未改**任何 `.rs`/`.cs`/`whitelist.md`/`cards.json`/`recon_gates.py`。
- **未起**服务、**未占**端口（仅 `mysqld:3306` 在跑）、**未跑** `cargo`。
- 建议登记（`whitelist.md`，供承卡线落地，本件不写）：
  - **T-新1**：`CM_OPENDOOR` 恒 no-op（守卫写反，`Operate.cs:366`）——照搬，Rust `client_open_door` 零出帧为等价。**完整登记条文（照 T-38/T-52 体例）见 §2.2.1（草案号 T-60）**。
  - **T-新2**：`ProcessMapDoor`(500ms)/`SendDoorStatus`/`SM_OPENDOOR_OK`/`SM_CLOSEDDOOR` 不可达（无门会开）。
  - **T-新3**：`WorldServer.ProcessEvents`(1s)+`MagicEventList` 死代码（`MagicManager.cs:1203` 注释）。
  - **T-新4**：`SafeEvent`/`MakeSafePkZone` 的 `MapName` 空守卫 ⇒ 从不添加。
  - **T-新5**：`ADDMAPMAGICEVENT`/`RANDOMADDMAPMAGICEVENT` 脚本命令在 oracle 无处理器（no-op）。
- 建议**判据修正线**（`recon_gates.py:138`）：`LoadMapDoor` 从 `kind="other"` 改为**可追踪卡**（与本件写集无关）。


---

## 8. 本轮修正复核（S3-FIX，fix#1–#4；命令 + 输出）

> 由 S3-FIX 车道（worktree `E:/tmp/doc-door`）对批判者 CONFIRMED 的**两处口径不完整 + 一处证据高估**逐处复核并修订。
> 命令均**只读**（未跑 `cargo`、未起服务、未占端口）。`src/` = 主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2`。

### fix#1 —— §2.1/§2.5 B：MapRoute 三级门控补全（含批判者点名的三重条件）

```text
$ grep -n "case CellType.MapRoute\|boNEEDHOLE\|ET_DIGOUTZOMBI\|ServerIndex\|ArroundDoorOpened\|EnterAnotherMap\|ChangeSpaceMove\|DisappearA" src/M2Server/Player/PlayObject.cs
1868:                            case CellType.MapRoute:
1872:                                    if (Envir.ArroundDoorOpened(CurrX, CurrY))
1874:                                        if ((!mapRoute.Envir.Flag.boNEEDHOLE) || (SystemShare.EventMgr.GetEvent(Envir, CurrX, CurrY, Grobal2.ET_DIGOUTZOMBI) != null))
1876:                                            if (M2Share.ServerIndex == mapRoute.Envir.ServerIndex)
1878:                                                if (!EnterAnotherMap(mapRoute.Envir, mapRoute.X, mapRoute.Y))
1885:                                                DisappearA();
1887:                                                ChangeSpaceMove(mapRoute.Envir, mapRoute.X, mapRoute.Y);
```
（同命令另命中 3718/3721/3759，均为 `EnterAnotherMap` 定义与别处 `DisappearA`，非本支，已省略。）

⇒ 三级门 ①`:1872` `ArroundDoorOpened` / ②`:1874` `boNEEDHOLE` + `ET_DIGOUTZOMBI` 事件 / ③`:1876` `ServerIndex` 相等
（相等 ⇒ ④a `EnterAnotherMap`(`:1878`)；不等 ⇒ ④b `DisappearA`+`ChangeSpaceMove`(`:1885-1887`)）。
已补进 §2.5 B 片段（含三级门表）与 §2.1「通行门控」行。旁证：`boNEEDHOLE` 定义在 `src/OpenMir2/Data/MapInfoFlag.cs:54`；
`ET_DIGOUTZOMBI=1` 在 `src/OpenMir2/Grobal2.cs:41`。

### fix#2 —— §2.5 A：类型与既有 `crates/envir` 结构同源

```text
$ sed -n '83,102p' mir2-rs/crates/envir/src/mapfile.rs
/// `OpenMir2.Data.DoorStatus`（可被同组多门共享的可变状态）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoorStatus {
    pub opened: bool,
    pub open_tick: i32,
}

/// `src/Modules/SystemModule/Data/MapDoor.cs` 的一条门。
///
/// `status_id` 就是 C# `MapDoor.Status` 的对象身份的 Rust 表示：两条门 `status_id` 相同
/// ⇔ C# 里它们是**同一个** `DoorStatus` 实例。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapDoor {
    pub x: i16,
    pub y: i16,
    /// C# `n08 = btDoorIndex & 0x7F`（> 0）。
    pub n08: u8,
    /// `LoadMapData` 里恒为 0（`MapDoor.DoorId` 默认值）。
    pub door_id: i32,
    pub status_id: usize,
```

⇒ `DoorStatus.open_tick` 由 `i64` 改 **`i32`**（对齐 `mapfile.rs:87`；C# `DoorStatus.OpenTick` 是 `int`）；`MapDoor` 五字段
（`x`=`i16`/`y`=`i16`/`n08`=`u8`/`door_id`=`i32`/`status_id`=`usize`）与 `mapfile.rs:95-102` **逐字段同型**；`MapGrid` 尺寸/坐标沿用
`i32`（`map.rs:170`/`:198`）；`now: i64` 传 `clock_ms`（`world.rs:77`）不变，状态比较用 `i64::from(open_tick)`。
已加 §2.5 A 后的「同源性核对」注。

### fix#3 —— §2.4：实测 grep（原「仅 mapinfo.rs 的 flag 解析」为高估）

```text
$ grep -rni "MapRoute\|bo_needhole" mir2-rs/crates mir2-rs/bins --include=*.rs
crates/envir/src/mapinfo.rs:83:    pub bo_needhole: bool,
crates/envir/src/mapinfo.rs:149:            bo_needhole: false,
crates/envir/src/mapinfo.rs:428:            flag.bo_needhole = true;
crates/envir/tests/csharp_parity.rs:68:        "bo_needhole": f.bo_needhole,
crates/world/src/map.rs:20:    MapRoute = 4,
crates/world/src/session.rs:7://! - 切图：`TransMap`/`MapRoute` 的落地动作 = 旧图 `DeleteFromMap` → 新图 `AddMapObject`，
bins/game-svr/src/main.rs:5796:/// **登记（未做项）**：触发本事件的**玩法路径**（地图链接格 `MapRoute` / 传送技能换图支）尚未移植
```

⇒ **7 行**：`mapinfo.rs:83/149/428`（`bo_needhole` 字段/初始化/置位）+ `csharp_parity.rs:68`（测试）+ `map.rs:20`
（`CellType::MapRoute` **枚举变体，已存在**）+ `session.rs:7` 与 `main.rs:5796`（**注释**）。§2.4 行已订正为
「枚举已有、`try_step` 分支未建模」，不再写「仅 `mapinfo.rs` 的 flag 解析」。

### fix#4 —— §2.2.1：登记条文草案（本件**不改** `whitelist.md`）

见 §2.2.1（可整行粘贴的 `| T-60 | … |`）。写作时 `whitelist.md` 最大编号实测：

```text
$ grep -oE "T-[0-9]+" mir2-rs/tests/parity/whitelist.md | sed 's/T-//' | sort -n | uniq | tail -1
59
```
⇒ 草案取 **T-60**（落地时以当时最大 +1 为准；撞号按「后注册者让号」顺延）。

---

> **本轮改动范围**：仅本设计文件（`mir2-rs/tests/parity/m3/design-door-and-mapevent-20261016.md`），**未**改任何 `.rs`/`.cs`/`whitelist.md`；
> **未**跑 `cargo`、**未**起服务、**未**占端口；LF 换行。
