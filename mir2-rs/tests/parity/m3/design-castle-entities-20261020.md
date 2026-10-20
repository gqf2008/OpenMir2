# Lane B — 城堡实体挂图（UserCastle.Initialize 门/墙/弓手/守卫）可派工简报

- 基线：`b0e618c33`（master）。只读车道：未建 worktree、未改仓库、未起服务器/客户端、未碰 `E:/MirServer`（仅只读）。
- 输入：`mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md` #5 + D-2；oracle `src/M2Server/Castle/UserCastle.cs:108-227`、`CastleDoor.cs`、`WallStructure.cs`、`src/GameSrv/Word/WorldServer.MonGen.cs:653-661`；Rust `crates/world/src/castle.rs:175-410`、`monai.rs` 的 `GuardUnit`、`bins/game-svr/src/mongen.rs:343`、`Castle/0/SabukW.txt`。
- 结论口径：**行为等价优先**。本文把「已实现/已登记」与「本卡要做的接线」分开，不把既有缺口当新发现。

---

## 0. 摘要

本卡是「`UserCastle.Initialize` 实体挂图 + `CastleDoor`/`WallStructure` 运行时接线」，不是重写状态函数：`castle.rs` 的 `initialize`/门墙状态/`StructureEvent`/`apply_structure_events` 已实现并有单测，登记缺口就是 bins 未接线（`castle.rs:963-966`；卡 `openmir2-m3-castle-file-load` / `-door-repair`）。写集：`CastleUnit` 持 actor id、race 110/111 的 Run 与死亡路由、`World` 持城堡表、bins 启动装载与帧排水。

三条更正：
1. 不是 20 实体/4 守卫。`Guards` 长度 **12**（`UserCastle.cs:49`）；4 是雇佣上限。`Castle/0` 的 `Guard_1..4` HP=0 跳过，`Guard_5..12` 缺键默认 2000/`守卫`/(0,0)，怪物库无「守卫」⇒ 8 次失败。
2. 部署实读的不是 `Castle/0`，而是 `Castle/Castle/SabukW.txt`（`CastleMgr.Initialize` 先初始化再改 `ConfigDir="0"`，`CastleManager.cs:65-69`）；该文件全是默认名 `MainDoor/LeftWall/...` 且弓手/守卫 HP=0 ⇒ 部署应为 **0 实体、仅 4 条告警**。
3. `Castle/0` 夹具成功 **16** 个（1 门+3 墙+12 弓手）；墙名中/右**互换**，照抄不改。

RNG：成功造怪 **6 抽**（ctor 4 + `Dir` + `CoolEye`；无 `MonItems`），未知名 0 抽。插入点在 `LoadMapQuest` fold 后、`LoadMerchant` 前（`GameApp.cs:184→213→227`），复用 `create_monster_oracle` 与同一 `world.rng`。tier 建议 **R**。

---

## 1. 先决更正（卡面必须写死；评审按此判）

### 1.1 「4 守卫 / 20 实体」的来源与事实

| 说法 | 事实 | 证据 |
|---|---|---|
| `Guards` 是 4 元素 | 数组长度 **12**；4 是 `MaxCalsteGuard`（雇佣上限） | `UserCastle.cs:49` `Guards = new ArcherUnit[12]`；`CastleConf.cs:84-93` 按 `Guards.Length` 读 `Guard_{i+1}_*`；`CastleConst.cs:10-14` |
| 部署有 4 个守卫实体 | `Castle/0` 的 `Guard_1..4_HP=0` ⇒ 跳过；`Guard_5..12` 缺键 ⇒ `HP=2000`、`x=y=0`、名 `守卫`；怪物库无「守卫」⇒ 8 次 `RegenMonsterByName` 返回 null | `CastleConf.cs:90` 默认 2000；`ReadWriteUInt16` 缺键写默认（`ConfigFile.cs:75-83`）；`sql/mir2_data.sql` 无守卫行 |
| 共 20 个实体 | `Castle/0` 夹具下 16；部署实际 0；理论上限 28 | §1.2 三情景 |

> `M3⑧公会攻城现状盘点报告-20261013.md` §1.F 第 131 行与 §3 卡 6 的「12 弓手/4 守卫」是同一措辞缺陷（该表自身写 `Guards[12]`），本卡以代码为准。

### 1.2 部署到底读哪份 `SabukW.txt`

oracle 启动顺序（`GameApp.cs`，行号为实读）：

- `GameApp.Initialize`：`LoadMapInfo()` `:124` → `LoadMonsterDB()` `:131` → `LoadMonGen()` `:147` → `InitializeMonster()` `:155` → `LoadMapQuest()` `:184` → **`SystemShare.CastleMgr.Initialize()` `:213`** → `DataServer.Initialize()`。
- `GameApp.InitializeWorld`：`LoadMerchant()` `:227` → `LoadNpcs()` `:229` → … → `LoadGuardList()` `:241` → `WorldEngine.Initialize()` `:244` → `GuildMgr.LoadGuildInfo()` → **`CastleMgr.LoadCastleList()` `:246`**。
- `CastleManager.Initialize`（`CastleManager.cs:62-88`）：进程启动时 `CastleList` 为空 ⇒ `new UserCastle(SystemShare.Config.CastleDir)`；`Config.CastleDir=Castle`（部署 `Server.conf:65`）⇒ ctor 的 `ConfigDir="Castle"`（`UserCastle.cs:34`），文件 `Castle/Castle/SabukW.txt`；`castle.Initialize()`（**实体在这里造**）；**之后**才 `castle.ConfigDir="0"`（`:69`）→ `CastleList.Add` → `Save()`。
- `LoadCastleList`（`:154-176`）读 `Castle/List.txt`（部署=`0\r\n1\r\n`），**只 append、不再 Initialize** ⇒ `Castle/0`、`Castle/1` 的实体永远不在启动时刷出。

三情景（均以部署文件只读复算）：

| 情景 | 读的文件 | Regen 调用 | 成功实体 | 告警 |
|---|---|---|---|---|
| **A 部署实际（parity 目标）** | `Castle/Castle/SabukW.txt` | 4（门+3 墙；弓手/守卫 HP 全 0 被跳过） | **0**（4 个默认名怪物库不存在） | 4（主门英文文案 + 三墙 copy-paste 名，见 §2.1） |
| **B 夹具（用 `Castle/0` 作 ConfigDir）** | `Castle/0/SabukW.txt` | 4 + 12 弓手 + 8 守卫（5..12）= **24** | **16**（门/墙/弓手） | 8（`守卫` 无模板） |
| **C 假想（12 守卫全部有名有 HP）** | 自造 | 28 | 28 | 0 |

> 判据请以 **A** 钉 parity、以 **B** 钉实体机制；两者都要有。**不要**为了凑 20 把守卫写进部署内容（内容冻结）。

### 1.3 `Castle/0/SabukW.txt` 的实际数值（B 情景夹具）

| 角色 | 名字（GBK） | 坐标 | 配置 HP | DB 模板 HP / race |
|---|---|---|---|---|
| MainDoor | 沙巴克城门 | (672,330) | 9999 | 10000 / 110 |
| LeftWall | 沙巴克左城墙 | (624,278) | 4999 | 5000 / 111 |
| CenterWall | **沙巴克右城墙**（互换） | (627,278) | 4999 | 5000 / 111 |
| RightWall | **沙巴克中城墙**（互换） | (634,271) | 4999 | 5000 / 111 |
| Archer 1..12 | 弓箭手 | (662,333)(664,331)(666,329)(676,319)(678,317)(681,314)(628,271)(632,267)(670,335)(671,334)(675,330)(676,329) | 2000 | 2000 / 112 |
| Guard 1..4 | 守卫 | (0,0)（缺 X/Y） | 0 | —（跳过） |
| Guard 5..12 | 守卫（缺键默认） | (0,0) | 2000（缺键默认） | —（模板不存在） |

DB 锚：`sql/mir2_data.sql:366-369`（race 110 HP 10000；race 111 HP 5000）、`:360`（弓箭手 race 112 HP 2000）。race 110/111 不随 `MonsterPowerRate` 缩放已由数据层实现（`loaders.rs:259`，报告 §1.F 已登记）。

---

## 2. oracle 行为（逐行）

### 2.1 `UserCastle.Initialize` 调用序（`UserCastle.cs:108-227`）

1. `LoadConfig()`（`:113`）：`castleConf.LoadConfig(this)` → `MasterGuild = GuildMgr.FindGuild(OwnGuild)`；随后 `LoadAttackSabukWall()`。
2. 归属门：`GetMapOfServerIndex(MapName) == M2Share.ServerIndex`（`:113`）；部署 `MapInfo` 的 `[3 盟重省]` 无 server index ⇒ 0 = 本服，通过。
3. 皇宫/密道 `FindMap` 失败只 `Warn`（`:115-125`），不阻断。
4. `CastleEnvir = FindMap(MapName)`；null ⇒ 告警并**整体跳过**（`:127-128`）。
5. 主门：`RegenMonsterByName(MapName, x, y, MainDoor.sName)`（`:130`）→ 成功则 `WAbil.HP = nHP`（`:133`）、`Castle = this`（`:134`）、`if (MainDoor.nStatus) ((CastleDoor)BaseObject).Open()`（`:135-138`）；失败告警 `[Error] UserCastle.Initialize MainDoor.UnitObj = nil`。
6. 左/中/右墙（`:144-173`）：各自 `Regen` → `HP` → `Castle=this`；失败告警**引用上一单元名字**（copy-paste 怪癖，`:152` 报 `MainDoor.sName`、`:162` 报 `LeftWall.sName`、`:172` 报 `CenterWall.sName`）——逐字照抄。
7. 弓手 `i=0..11`（`:174-193`）：`if (nHP <= 0) continue`；成功 ⇒ `HP`、`Castle=this`、`((GuardUnit)BaseObject).GuardDirection = 3`。
8. 守卫 `i=0..11`（`:194-211`）：`if (nHP <= 0) continue`；成功 ⇒ **只**写 `HP`（**不设 `Castle`**，也不设 `GuardDirection`）。

本函数已在 Rust `castle.rs:967-1100` 逐行实现；`CastleEvent::SpawnDefenseUnit` 已带 `castle_owner`/`guard_direction` 区分。**缺的是 `CastleInitializeEnv` 的真实宿主与 actor id 绑定**（`DefenseCreature` 目前只返回 `max_hp`，见 §3）。

### 2.2 实体字段/属性来源

- 模板来自 `M2Share.WorldEngine.MonsterList[sMonName]`（`RegenMonsterByName` → `GetMonRace`，`WorldServer.cs:167-174`；未知名返回 `-1` → `CreateMonster` switch 不命中 → `cert==null`，**在任何抽数之前**）。
- `ApplyMonsterAbility`（`MonGen.cs:882+`）写 `Race/RaceImg/Appr/Level/Exp/HP/MaxHP/AC/MAC/DC/…`；随后 `cert.WAbil = cert.Abil`（`:741`）。
- `Initialize()`（`BaseObject.Base.cs:14-23`）执行 `AbilCopyToWAbil()`、`AddtoMapSuccess`、`CanWalk(x,y,true) && AddToMap()`、`CharStatus`；**无 RNG**。`WallStructure.Initialize` 额外 `Dir=0`（`WallStructure.cs:17-21`）。
- Castle 覆盖：只改 `WAbil.HP`（配置值），**不改 `MaxHP`**。因此 `9999/10000`、`4999/5000` 的比值决定 `Dir`（见 §2.4）。
- ctor 常量（`CastleDoor.cs:14-21` / `WallStructure.cs:9-15` / `ArcherGuard.cs:12-19`）：
  - 110 `CastleDoor`：`Animal=false`、`StickMode=true`、`AntiPoison=200`、`CellType=Door`、`IsOpened=false`、`HoldPlace=false`。
  - 111 `WallStructure`：`Animal=false`、`StickMode=true`、`AntiPoison=200`、`SetMapFlaged=false`、`Initialize` 后 `Dir=0`。
  - 112 `ArcherGuard`：`ViewRange=12`、`WantRefMsg=true`、`GuardDirection=-1`、`Castle=null`（随后 `Initialize` 绑 `Castle=this`、`GuardDirection=3`）。
  - `GuardUnit` ctor `Race=ActorRace.Guard(11)`，110/111/112 由 `ApplyMonsterAbility` 覆写。

### 2.3 受击 → `Die` → `SetMapXyFlag(2)` 路由

- 受击路径：`StruckDamage` 扣 `WAbil.HP`；**没有** `CastleDoor.Struck` 覆写。`GuardUnit.Struck`（`GuardUnit.cs:24-33`）只在 `Castle != null` 时置 `CrimeforCastle=true`/`CrimeforCastleTime`（对门/墙同样执行，但该标志只被 `GuardUnit.IsProperTarget` 读）。
- 死亡判定：`BaseObject.Run` 的 `if (WAbil.HP == 0) Die();`（`BaseObject.Base.cs:70-73`）。多态分派到：
  - `CastleDoor.Die()`（`CastleDoor.cs:78-82`）：`base.Die()`（`Death=true`、`DeathTick=now`、`KillFunc`、`DelObjectCount`、`RM_DEATH`）→ `SetMapXyFlag(2)`。
  - `WallStructure.Die()`（`WallStructure.cs:42-45`）：仅 `base.Die()`；格子由下一 tick 的 `Run` 处理（§2.4）。
- `SetMapXyFlag(nFlag)`（`CastleDoor.cs:23-44`）的**有序** 12/15 格操作已由 `castle.rs::door_map_xy_flag_ops` 复刻：
  - `nFlag=0`（开）：先 3 格 `true`，再 9 格 `true`，最后 3 格 `false`（15 项；重复格顺序敏感）；
  - `nFlag=1`（关）：先 3 格 `true`，再 9 格 `false`（12 项）；
  - `nFlag=2`（破）：先 3 格 `true`，再 9 格 `true`（12 项）。
- **B-15 怪行为**：`Envir.SetMapXyFlag` 只在格子 `Valid`（= `Walk`）时改属性（`Envirnoment.cs:1362-1369`；Rust `map.rs:344-360` 同口径）⇒ 已被置 `LowWall` 的格无法被后续 `walk=true` 恢复。后果：已开的门再被打破时，`nFlag=2` 无法恢复那 3 个门板格；墙死亡后 `Run` 的恢复同样无效。**照抄，不修**。
- 需要出帧：`RM_DEATH` 是通用死亡帧；`SetMapXyFlag` 本身无帧，只改地图通行。宿主必须把 `StructureEvent::SetMapXyFlag` 按序作用到 `MapGrid`。

### 2.4 `CastleDoor.Run` / `WallStructure.Run` / `RefStatus` 的事件

`CastleDoor.Run`（`CastleDoor.cs:84-104`）：
1. `Death && Castle != null` ⇒ `DeathTick = GetTickCount()`（每 tick 刷新，防 5 分钟 Ghost）；否则 `HealthTick = 0`（防回血）。
2. `!IsOpened` ⇒ `n08 = 3 - Round(HP/MaxHP*3)`；`Dir != n08 && n08 < 3` ⇒ `Dir=n08` + `SendRefMsg(RM_TURN)`。
3. `base.Run()` = `GuardUnit`→`NormNpc.Run`→`AnimalObject.Run`→`BaseObject.Run`（Rust 等价：`animal_run` + `base_object_run_monster`）。

`CastleDoor.RefStatus`（`:106-115`）：`n08 = 3 - Round(HP/MaxHP*3)`；`n08-3>=0 ⇒ 0`；`Dir=n08` + `RM_ALIVE`。调用点只在修门（`UserCastle.cs:933/944`，属 `castle-door-repair` 卡），启动不调用。

`WallStructure.Run`（`WallStructure.cs:47-82`）：
1. `Death` ⇒ `DeathTick=now`；若 `SetMapFlaged` ⇒ `SetMapXyFlag(x,y,true)` 并清标志。
2. 非死 ⇒ `HealthTick=0`；若 `!SetMapFlaged` ⇒ `SetMapXyFlag(x,y,false)`（置 `LowWall`）并置标志。
3. `n08 = HP>0 ? 3-Round(HP/MaxHP*3) : 4`；`Dir != n08 && n08 < 5` ⇒ `Dir=n08` + `RM_DIGUP`。
4. `base.Run()`。
   - 注意：步骤 1 的 `walk=true` 因 B-15 对 `LowWall` 格**恒无效**——事件要发、格子不变。

`WallStructure.RefStatus`（`:23-40`）：`HP>0 ? 3-Round(...) : 4`；`n08>=5 ⇒ 0`；`Dir=n08` + `RM_ALIVE`（修墙调用点 `UserCastle.cs:981/991`）。

启动后首 tick（部署 B 夹具）：墙 HP 4999/5000 ⇒ `n08=0`，`Dir` 已是 0 ⇒ **不发 `RM_DIGUP`**，但会发一次 `SetMapXyFlag(false)`（置墙格 `LowWall`）。门 `MainDoorOpen=0` ⇒ 不开；`n08=0`，首 tick 若 ctor 随机 `Dir!=0` ⇒ 发一次 `RM_TURN(0)`。

### 2.5 RNG 账本（与 `mongen.rs` 现有站点的关系）

`CreateMonster` 对 110/111/112 的取数（`MonGen.cs:498-810`）：

| # | 站点 | 抽数 |
|---|---|---|
| 1 | `BaseObject` ctor `RunTick = GetTickCount() + Random(1500)`（`BaseObject.cs:439`） | 1 |
| 2 | `BaseObject` ctor `SearchTime = Random(2000) + 2000`（`:441`） | 1 |
| 3 | `AnimalObject` ctor `AttackTick = now - Random(3000)`（`AnimalObject.cs:76`） | 1 |
| 4 | `AnimalObject` ctor `WalkTick = now - Random(3000)`（`:77`） | 1 |
| — | switch 110/111/112 的 ctor **无额外抽** | 0 |
| 5 | `cert.Dir = RandomByte(8)`（`MonGen.cs:740`） | 1 |
| — | `MonGetRandomItems`：`MonItems/<名>.txt` 不存在 ⇒ `ItemList` 空 ⇒ 0 抽（`LocalDB.cs:596-650`；部署 `Envir/MonItems` 无 4 个城堡名） | 0 |
| 6 | `Random(100) < cert.CoolEyeCode`（`MonGen.cs:745-747`；DB `CoolEye=0`，但仍抽） | 1 |

- **成功造怪 = 6 抽**；跳过（`nHP<=0`）= 0；未知名字 = 0（`race=-1` 在 switch 前返回）。
- Rust 已有 `draw_monster_construct`（4 抽，`mongen.rs:290-296`，race 110/111/112 走 `{}` 分支）+ `create_monster_oracle`（`mongen.rs:480-559`：`dir`、`budget_drops`、`cool_eye`、地形移位）**完全覆盖**上述序列。本卡**不得**新写抽数，必须复用。
- **地形移位**：若 `CanWalk(x,y,true)` 为假，`CreateMonster` 会进入最多 31 轮移位循环，其中可能再抽 `Random(width/2)`/`Random(height/2)`（`MonGen.cs:755-795`）。因此判据必须先钉「部署地图 3 的 16 个坐标 `CanWalk(...,true)` 全为真 ⇒ 0 额外抽」；若为假，按 oracle 的实际移位结果记录，不得跳过。
- 启动位置：oracle `InitializeMonster`（`:155`）→ `LoadMapQuest`（`:184`）→ **城堡造怪（`:213`）** → `LoadMerchant`（`:227`）→ `LoadGuardList`（`:241`）。Rust 现有 `fold_ctor_draws_load_map_quest` 正对 `:184`，因此城堡块必须插在它之后、`load_merchant_spawns` 之前（§5）。
- `GetMonsterThreadId` 在造怪之后（`:388`）：`MonsterThreadMap` 由 `InitializeMonsterThread` 按 `MonsterList.Values` 全量建表（`MonGen.cs:91-98`）⇒ 任何 DB 名字恒有 threadId，不构成额外分支/抽数。

---

## 3. Rust 现状（引用既有登记，不算新发现）

| 面 | 现状 | 证据 |
|---|---|---|
| 配置解析（`SabukW`/`List`/`AttackSabukWall`） | **已实现**；`guards` 长度 = 12（`GUARD_ARRAY_LEN`），缺键默认 2000/`守卫`/0 | `crates/envir/src/castle.rs:193-370`、`:552`、`:689-708` |
| `UserCastle::initialize` 状态投影 | **已实现**并单测（顺序、跳过、`castle_owner`、`guard_direction`） | `crates/world/src/castle.rs:967-1100`、`:2614-2680` |
| 门/墙状态函数 + 掩码 | **已实现**并单测（含 B-15） | `castle.rs:175-410`、`:2180-2235` |
| `StructureEvent` → `MapGrid` | **已实现**，但生产无调用点（仅测试） | `castle.rs:587-598`；`apply_structure_events` |
| bins 启动接线 | **未接线**（如实登记） | `castle.rs:963-966`；`grep -rn "CastleDefenseSpecs" bins` = 0 |
| 世界城堡表 | **无**（`World` 无 `castles`；`castle_tax::CastleManager` 只有税收面且未被 bin 持有） | `castle_tax.rs:168-176` |
| 110/111 的运行时分支 | **缺失且当前是错的**：`monster_ai_tick` 只把 11/12/20/112 走守卫/弓手支；110/111 落到通用 `monster_run`（会 `Think`/游走/索敌） | `monai.rs:3780-3830`、`:868-943` |
| 死亡路由 | **无**：`settle_deaths` 只置 `death/death_tick`，不调 `door_die`、不产掩码 | `world.rs:5864-5945` |
| 帧排水 | `RM_DIGUP/DIGDOWN/ALIVE` 已有；**`RM_TURN` 无怪物侧分支**；`drain_monster_ref_msgs` 只处理 HIT/DIGUP/DIGDOWN/RM_10205/FLYAXE/LIGHTING/ALIVE | `main.rs:12680-12780` |
| 实体绑定 | `CastleUnit` **无 actor id**；`DefenseCreature` 只有 `max_hp`；`Entity` 无 castle 链 | `castle.rs:177-205/690-700`；`entity.rs:423+` |

> 上述「未实现」均已在 `M3⑧公会攻城现状盘点报告-20261013.md` §2 #7/#22 与卡 `openmir2-m3-castle-file-load` / `openmir2-m3-castle-door-repair` 登记；本卡只做剩余接线。

---

## 4. 写集（按 AGENTS §0b 切分；与 `openmir2-m3-monster-ai-tick` 在 `monai.rs` 有重叠 ⇒ 串行）

| 文件 | 改动 | 备注 |
|---|---|---|
| `mir2-rs/crates/world/src/castle.rs` | `CastleUnit` 增 `entity_id: Option<i32>`、`name: String`（或分离 `CastleBinding`）；`DefenseCreature` 增 `actor_id`；`regen_and_bind` 写绑定；`CastleInitializeEnv::regen_monster_by_name` 返回句柄 | 保持 `initialize` 调用序不变；现有单测的 `FakeEnv` 需补 `actor_id` 字段（默认 0 即可） |
| `mir2-rs/crates/world/src/entity.rs` | 增 `castle_binding: Option<CastleBinding { castle_index: usize, role: DefenseRole }>`；110 的 `entity_cell_type` 返回 `CellType::Door`（oracle `CellType=Door`）；110/111 ctor 常量 `stick_mode=true`、`anti_poison=200`（若 `Entity` 无字段则按 `PoisonState` 现状登记） | 110/111 的 `animal=false` 在 Rust 无字段，按 `stick_mode`/AI 分支等价即可；如确需新增字段，单列 |
| `mir2-rs/crates/world/src/monai.rs` | ① `monster_ai_tick` 在 112/守卫支之前加 110/111 支：`door_run(now, has_castle, hp)` / `wall_run(now, hp)`，把 `StructureEvent` 入队（见下），再 `animal_run` + `base_object_run_monster`；② **死亡支**（顶部 `if e.death`）对 110/111 也调上述 Run（`CastleDoor.Run` 死亡刷新 `DeathTick`；`WallStructure.Run` 恢复格）；③ 明确 110/111 **不得**进入 `monster_run` | 顺序敏感：Run 事件与 `base.Run` 的相对位置按 §2.4 |
| `mir2-rs/crates/world/src/world.rs` | ① `pub castles: Vec<crate::castle::UserCastle>`；② `TickRecord` 增 `castle_structures: Vec<CastleStructureAt { castle_index, event }>`；③ `settle_deaths`（非玩家段）里对 `race==110` 调 `door_die` 并 `apply_structure_events` 到对应 `MapGrid`；④ 暴露 `World::spawn_castle_defense(...)` 或 `World::apply_castle_events(...)` 供 bin 使用 | 死亡段早于 `monster_ai_tick`（`world.rs:4509` vs `:4549`），卡面需注明 1 tick 相对 oracle 的等价口径与真机窗口 |
| `mir2-rs/bins/game-svr/src/castle.rs`（新） | 实现 `CastleInitializeEnv`：`map_owner_is_self`（`MapInfo.server_index`）、`find_map`、`regen_monster_by_name`（`create_monster_oracle` + `WorldMonsterSink`，**同一条 `world.rng`**，参照 `step_mon_gen` 的 `mem::replace` 手法）、`warn` | 不得在 main 线程抽数；装载只读文件 |
| `mir2-rs/bins/game-svr/src/main.rs` | ① 主线程装载 `Castle/<CastleDir>/SabukW.txt`（GBK `decode_text`）→ `EnterCfg` 新字段；② sim 起世界后在 fold 段③之后、`load_merchant_spawns` 之前调用 `UserCastle::initialize`；③ 排水 `castle_structures`：`SetMapXyFlag` → `MapGrid`，`Ref*` → `send_ref_msg`；④ 新增 `RM_TURN` 的怪物帧分支 | 插入点见 §5 |
| `mir2-rs/bins/game-svr/src/mapwire.rs` | 生产侧接 `apply_structure_events`（`mapwire.rs:837` 已留注释）；门/墙格属性变化后更新 `MapGrid` | 与 `MapGrid` 现存掩码路径一致 |
| `mir2-rs/bins/game-svr/src/smframe.rs` | 复用 `sm_digup/sm_digdown/sm_alive`；补 `sm_turn` 若现有签名不足（`downlink.rs` 的 `SM_TURN + SM_FEATURECHANGED` 两帧口径） | RM_TURN 收件人处理见 `PlayObject.Message.cs:1524-1560` |

**不改**：`crates/envir`（解析已正确）、`crates/data`、协议常量、`src/`（C#）。

---

## 5. 插入点（顺序是判据）

### 5.1 启动（`bins/game-svr/src/main.rs`）

1. 主线程（`main()` 装载段，`mon_gen_entries` 附近 `:639-684`）：新增 `load_sabuk_w_file(loader, castle_dir, config_dir)`，读 `Castle/Castle/SabukW.txt`（oracle 默认路径）；文件不存在时按 `ConfigFile.Load` 的「建空文件 + 全默认」口径返回默认 specs（是否真实创建文件留给 #4 落盘卡）。**不读** `Castle/List.txt` 来做实体（`LoadCastleList` 只 append）。
2. sim 线程：在 `fold_ctor_draws_load_map_quest`（`:14339-14346`）之后、`load_merchant_spawns`（`:14353`）之前，构造 `CastleDefenseSpecs`（`SabukWConfig` → `main_door.status = main_door_open`，`archers/guards` 各 12 项）并调用 `UserCastle::initialize(&specs, &mut WorldCastleEnv{...})`；随后把 castle 推入 `world.castles`，把 `SpawnDefenseUnit` 绑定到实体。
3. RNG 走 `world.rng` 同一流（`create_monster_oracle(&mut rng, &mut sink, ...)`，sink 内含 `&mut World`；借用手法照抄 `step_mon_gen` 的 `mem::replace`）。
4. **不要**把城堡块放到 `place_guards`（`:14507`）之后：oracle 顺序是 CastleMgr.Initialize（`:213`）早于 LoadGuardList（`:241`）。

### 5.2 tick（`crates/world/src/world.rs`）

- `tick_head`：`settle_deaths(&mut record, false)`（`:4509`）——**在死亡结算里**对 110 执行 `door_die`（`base.Die` 已由通用路径置位；`door_die` 只补 `death=true` 与 12 格掩码；事件入 `record.castle_structures`）。顺序：通用 `Death/DeathTick` → `door_die` 掩码。
- `monster_ai_tick`（`:4549`）：110/111 分别 `door_run`/`wall_run`；事件入 `record`（或 world 的 `pending_structure_events`），由 bin 在 tick 后排水。
- `CastleUnit.hp` 与 `Entity.hp` 的权威关系：建议 **entity.hp 为权威**，每 tick 入口同步 `unit.hp = entity.hp`（或把 `hp` 作为 `door_run/wall_run` 参数），避免 `StruckDamage` 后两处不一致。加一条牙（C3b）钉死。

### 5.3 帧/地图排水（`bins/game-svr`）

- `StructureEvent::SetMapXyFlag` → `apply_structure_events(&mut grid, &[...])`（`castle.rs:587`）。
- `RefDigUp/RefDigDown/RefTurn/RefAlive` → 转成 `MonsterRefMsg { actor_id, ident, w_param=dir, n_param1=x, n_param2=y, ... }` 并复用 `drain_monster_ref_msgs`；`RM_TURN` 分支需按 `PlayObject.Message.cs:1524-1560` 发 `SM_TURN` + `SM_FEATURECHANGED` 两帧。
- 收件人集合：`SendRefMsg` 的 ±12 格、排除自己、`Race==Play && !Ghost` 快照（`MonsterRefMsg.recipients` 已有机制）。

---

## 6. 构造级判据与牙（>=6；每条都可机判）

> 命名建议直接作为测试名。除 C1 外全部在 `crates/world` + `bins/game-svr` 构造级跑；真机窗口只做抽样。

**C1 `castle_sabukw_specs_from_deployed_file`（L）**
解析 `E:/MirServer/M2GameSvr/Castle/0/SabukW.txt`（GBK）→ 断言：门 `(672,330,9999,"沙巴克城门")`；墙 `(624,278,4999,"沙巴克左城墙")`、`(627,278,4999,"沙巴克右城墙")`、`(634,271,4999,"沙巴克中城墙")`（**互换保留**）；12 弓手坐标/HP 2000；`guards.len()==12`，`guards[0..4].hp==0`，`guards[4..12].hp==2000 && x==0 && y==0 && name=="守卫"`。
**牙**：把 `GUARD_ARRAY_LEN` 改成 4 ⇒ 红；把中/右墙名按语义对调 ⇒ 红；把缺键守卫 HP 默认改为 0 ⇒ 红。

**C2 `castle_initialize_call_order_and_skip_counts`（L）**
`UserCastle::initialize` + 记录式 `CastleInitializeEnv`，喂 C1 的 specs、怪物库含 4 个城门/墙名与 `弓箭手`、**不含** `守卫` → 断言 Regen 调用序 = `[MainDoor, LeftWall, CenterWall, RightWall, Archer×12, Guard5..Guard12×8]`，共 **24** 次；`SpawnDefenseUnit` = **16**；`warn` = 8 条（守卫失败文案）。
**牙**：去掉 `hp==0` 跳过 ⇒ 28/红；把守卫默认 2000 改 0 ⇒ 16/红；调换墙序 ⇒ 红。

**C3 `castle_binding_hp_maxhp_and_castle_flag`（L）**
用真实 DB 模板（race/HP）：成功实体断言 `race`=110/111/112、`entity.hp`=9999/4999/2000、`entity.max_hp`=10000/5000/2000；门/墙/弓手 `castle_binding.is_some()`，守卫 `None`；弓手 `guard_direction==3`；门 `opened==false`（`MainDoorOpen=0`）、`stone_mode==false`。
**牙**：把配置 HP 写进 `max_hp` ⇒ 红；给守卫绑 `Castle` ⇒ 红；不给弓手设 3 ⇒ 红。

**C3b `castle_unit_hp_tracks_entity_after_struck`（L）**
对门调用 `apply_struck_damage` 扣到 9000，跑一 tick：`CastleUnit.hp == entity.hp == 9000`，后续 `door_run` 的 `Dir` 用 9000/10000（`3-round(2.7)=0`）。
**牙**：把 `unit.hp` 固定为生成值 ⇒ 红。

**C4 `castle_spawn_rng_six_draws_per_unit`（R）**
固定种子；对 16 个成功实体逐只断言 6 抽及用途顺序（`run_tick_delta`、`base_search_time`、`attack_tick_delta`、`walk_tick_delta`、`dir`、`cool_eye`）；对 8 个失败守卫断言 **0 抽**；整段 16×6=96 抽，且总流水与 `draw_monster_construct` + `create_monster_oracle` 的既有站点一致（`MIR2_RNG_LOG` site/order）。
**牙**：改用 `draw_monster_object`（多 1 抽）⇒ 红；把 `CoolEye` 抽放到 drop 之前/之后错位 ⇒ 红；失败名字也抽 ⇒ 红。

**C5 `door_die_sets_flag2_after_death`（R）**
门 HP 置 0 → 一 tick → 断言 `death==true`、`DeathTick==now`、`record.deaths` 含门、`castle_structures` 中 `SetMapXyFlag` **12 项且顺序 = `door_map_xy_flag_ops(2)`**；在「已关门（`Close()` 置 9 格 LowWall）」与「已开门」两种前置下分别断言 B-15：开门后 3 个门板格仍 `LowWall`（破门不能恢复）。
**牙**：删 `SetMapXyFlag(2)` 或把 `nFlag=2` 改成 1 ⇒ 红；在 `base.Die` 之前发掩码 ⇒ 事件序红。

**C6 `wall_run_toggles_cell_and_digup`（R）**
生成墙（HP 4999/5000）→ 首 tick：`SetMapXyFlag(false)` 1 项、`SetMapFlaged=true`、`Dir` 保持 0 且**无** `RM_DIGUP`；把 HP 打到 2000/5000（`n08=2`）→ 下一 tick `RM_DIGUP(dir=2)`；HP=0 → 下一 tick `SetMapXyFlag(true)` 事件 + `SetMapFlaged=false` + `Dir=4`，且因 B-15 格仍 `LowWall`（断言 `apply_structure_events` 返回 0 changed）。
**牙**：删死亡恢复事件 ⇒ 红；把 `<5` 改成 `<3` ⇒ HP=0 的 `Dir=4` 帧丢失 ⇒ 红。

**C7 `door_run_turn_and_death_tick`（R）**
门不开、`Dir` 初值随机为 5 → 一 tick：`n08=0`、`Dir=0`、`RM_TURN(0)` 1 帧；门死亡后连续 2 tick：`DeathTick` 每次刷新为当前 `now`（`Castle != null`）；无城堡绑定时不刷新。
**牙**：每 tick 无条件发 `RM_TURN` ⇒ 红；死亡不刷新 `DeathTick` ⇒ 红。

**C8 `castle_structures_do_not_wander`（R）**
生成 16 实体 + 100 tick（无玩家）：110/111 的 `x/y` 不变、`ai.target_creature` 不变、无 `Think` 抽数、RNG 流水只在首 tick 有 `door_run` 的转向修正（若有）；112 走 `archer_guard_run`（不进入 `monster_run`）。
**牙**：让 110/111 落到 `monster_run` ⇒ 位置/RNG 流水红（这是当前 bug 的回归牙）。

**C9 `castle_spawn_startup_order_before_merchant`（R）**
在 `MIR2_RNG_LOG` 流水上断言：城堡 96 抽位于 `fold_ctor_draws_load_map_quest` 之后、`load_merchant_spawns` 的第一抽之前；把城堡块移到 `place_guards` 之后 ⇒ 流水顺序红。
**牙**：插入点移到 `load_merchant_spawns` 之后 ⇒ 红。

**C10（抽样帧，L）`castle_structure_frames_wire`**
对 `RefDigUp/RefDigDown/RefTurn/RefAlive` 各造一事件，断言下行：`SM_DIGUP(20)` 头 + `Param1=feature/Param2=CharStatus`、`SM_DIGDOWN(21)` 头、`SM_TURN(10001)+SM_FEATURECHANGED` 两帧、`SM_ALIVE(27)` + 8 字节 `CharDesc`。
**牙**：`RM_TURN` 不补 `SM_FEATURECHANGED` ⇒ 红；`SM_DIGUP` 的 Tag1 不置 0 ⇒ 红（oracle `:1890-1899` 先赋值后清零的怪行为）。

---

## 7. tier / 窗口 / 风险

- **tier=R**（建议，审查者定）：触及 ① 怪物 ctor RNG 流水；② 死亡/掩码时序（`settle_deaths` 早于 `monster_ai_tick`）；③ 地图通行掩码属战斗/移动可见面。C1/C2/C3/C3b/C10 是 L 子项，但整卡按 R 收口。
- **真机窗口**：部署 A 情景下 oracle 应 0 实体 + 4 条告警；若窗口内看到任何门/墙实体，说明读到的文件与本文不同，先停下复算 `CastleDir/ConfigDir`。B 情景（16 实体）建议用夹具目录单独跑，不占生产部署。
- **风险**：
  1. `Castle/Castle` 与 `Castle/0` 双文件并存，极易把夹具当生产；必须以启动日志/告警条数钉死读的是哪份。
  2. `settle_deaths` 在 AI 之前 ⇒ 门死亡的 `DeathTick` 刷新可能比 oracle 早 1 tick；现有骨架已接受同类近似，但本卡必须用「首 tick 不发/发哪些帧」的 golden 钉住，真机窗口复核。
  3. `Entity.hp` 与 `CastleUnit.hp` 双份状态；C3b 是防漂移牙。
  4. 110/111 当前落入 `monster_run` 是**已有 bug**，一旦造出实体就会立刻表现为游走/抽数偏移；C8 必须先红后绿。
  5. 地图 3 的 `CanWalk` 若与 C# 有差，会多出 `Random(width/2)` 抽数；C4 的 96 抽断言会红，属预期报警，不是测试脆弱。

---

## 8. 复算命令（只读）

```bash
cd E:/Users/gxh/Documents/GitHub/OpenMir2
git rev-parse HEAD                                   # b0e618c33...
grep -n "Guards = new ArcherUnit" src/M2Server/Castle/UserCastle.cs          # :49 -> 12
grep -n "MaxCalsteGuard" src/M2Server/Castle/CastleConst.cs                   # 4 = 雇佣上限
grep -n 'ConfigDir = "0"\|public void Initialize' src/M2Server/Castle/CastleManager.cs
grep -n "CastleMgr.Initialize\|CastleMgr.LoadCastleList" src/GameSrv/GameApp.cs
grep -n "沙巴克城门\|沙巴克左城墙\|沙巴克右城墙\|沙巴克中城墙" sql/mir2_data.sql  # :366-369
grep -n "MainDoorName\|Guard_5_Name\|Archer_1_HP" E:/MirServer/M2GameSvr/Castle/Castle/SabukW.txt
grep -n "MainDoorName\|Guard_1_HP\|Archer_1_HP" E:/MirServer/M2GameSvr/Castle/0/SabukW.txt
grep -rn "CastleDefenseSpecs" mir2-rs/bins --include=*.rs     # 0 -> 未接线
grep -n "is_guard_race\|matches!(race, 11 | 12 | 20 | 112)" mir2-rs/crates/world/src/monai.rs
grep -n "settle_deaths(&mut record, false)\|monster_ai_tick" mir2-rs/crates/world/src/world.rs
```

> 全部为只读命令；未写入仓库、未改动 `E:/MirServer`。

---

## 9. 残留物清单

- 本简报：`E:/tmp/laneB-castle-entities.md`（唯一产出，保留给派工/评审）。
- 未建 worktree、未改仓库、未起进程、未改 `E:/MirServer`；无临时库/抓包/日志。


---

## 评审员更正（2026-10-20，**以本节为准**）

- 不是「20 实体 / 4 守卫」：`Guards` 长度 **12**（`UserCastle.cs:49`），4 是雇佣上限。
- 部署实读的是 `Castle/Castle/SabukW.txt`（CastleMgr.Initialize 先初始化再改 `ConfigDir="0"`）⇒ 全是默认名且弓手/守卫 HP=0
  ⇒ 部署应为 **0 实体、仅 4 条告警**；`Castle/0` 夹具成功 **16 个**（1 门+3 墙+12 弓手），墙名中/右互换照抄。
- RNG：成功造怪 6 抽（ctor 4 + Dir + CoolEye）；未知名 0 抽。插入点 `LoadMapQuest` 之后、`LoadMerchant` 之前，
  复用 `create_monster_oracle` 与同一 `world.rng`。tier=R。
