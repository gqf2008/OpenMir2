# Valid 消费点全量审计（只读静态对读）

- 审计对象：提交 `b192bd9f3`（订正 mapcell 片）+ 其父 `2ff02ac0e`（把 `.map` 格子属性灌进 `MapGrid`）。
- 核心命题：装载让 `Valid == (Attribute == Walk)` 在**真机首次生效**（此前每格 `Walk` ⇒ `get_cell_info(...).ok` / `Valid` 恒真）。
- 方法：只读 `grep`/`sed`/`git show` + 独立解 `.map` 二进制（`python -c`，不落盘、不 cargo）。
- 硬约束遵守：未写仓库任何文件（本报告在 `/e/tmp/`）、未起服务/端口、未写库、未动 `E:/MirServer/**`（仅只读）。
- 审计面：`mir2-rs/crates/world/src/**` 与 `mir2-rs/bins/game-svr/src/**` 的**全部**格子可用性读取点。

---

## 0. 结论摘要

- **消费点总数（生产路径，去测试）**：**43 处**（world 36 + game-svr 7；另 1 处 envir 侧非生产 + 1 处 game-svr 独立模型）。
  上一位复验者只抽核约 10 处，**缺口 30+ 处已在本报告补齐**。
- **装载后真机可达、且为 oracle 对齐的新行为**（非回归）：**6 类**（玩家走位碰撞、刷怪移位+RNG、NPC/商人落格、
  route 落格 1658→1649、敌人计数/AOI、cetin_near_xy）。全部与 oracle **同构**。
- **未登记差异**：**1 条**（`session.rs:189 enter_map` 的 CellRejected 处置与 oracle 不等价）——见 §3 与 §5。
- **「装载后反而恒假」**：**0 条**（逐条见 §4；`set_cell_attributes` 只写 `Attribute`、不碰 `ObjList`，不具备该机理）。
- 关键证据（独立复核）：真机 `E:\MirServer\M2GameSvr\Map\0.map`（700×700，5880052 B）的**新角色出生格**
  `(289,618)`/`(650,631)` 及其 ±2 邻域**全部 `Walk`**（`bk=fr=0`），故 §3 的「登录被拒」风险在新角色默认路径上**不可达**。

---

## 1. 消费点全集（逐条：调用者 → 语义 → 装载前 → 装载后）

> 记号：`ok` = `get_cell_info(x,y).1`（= C# `GetCellInfo` 的 `success`）；`Valid` = `Attribute==Walk`；`IsAvail` = `ObjList` 非空。
> 「装载前」= 每格 `Walk`（`ok` 界内恒真）；「装载后」= 按 `.map` 属性。

### 1A. 谓词与门本体（`crates/world/src/map.rs`）

| # | 位置 / 函数 | 语义 | 装载前 | 装载后 |
|---|---|---|---|---|
| 1 | `map.rs:116 MapCellInfo::valid` | `Attribute==Walk` | 恒真 | 按属性 |
| 2 | `map.rs:121 MapCellInfo::is_available` | `ObjList` 非空 | 不变（与属性无关） | 不变 |
| 3 | `map.rs:279 MapGrid::cell_valid` | 界内→`valid()`；**越界→true** | 界内恒真 | 按属性（越界仍 true） |
| 4 | `map.rs:331 get_cell_info` | 界内且 Valid→该格，否则→`cells[0]` + `ok=false` | 界内恒 `ok=true` | 墙/越界→`ok=false` 且**回落 `cells[0]`** |
| 5 | `map.rs:342 get_cell_info_mut` | 同上（可变） | 同上 | 同上 |
| 6 | `map.rs:307 set_map_xy_flag` | 仅**当前 Valid** 格可改（B-15） | 恒可改 | 墙格**改不动**（照搬 oracle） |
| 7 | `map.rs:526 add_cell_object`（经 `:356 add_map_object`/`:369 add_floor_item`） | 界内且 Valid 才落对象 | 界内恒成功 | 墙/越界→`false`，**不落格** |
| 8 | `map.rs:390 AddMapRoute` | 界内且 Valid 才落 route | 墙格也落 | **墙格被丢**（本片主结果，1658→1649） |
| 9 | `map.rs:560 move_object` | 目标墙→`cells[0].valid()` 门 + 摘源 | 目标恒非墙 | 见 §4 反面核对（**含 cell0 耦合，与 oracle 同形**） |
| 10 | `map.rs:517 has_map_route` | `ok && 格含 MapRoute` | 恒可达 | 墙格→`false` |

### 1B. 世界模拟消费点（`crates/world/src/**`）

| # | 调用者（file:line 函数） | 语义 | 装载前 | 装载后 |
|---|---|---|---|---|
| 11 | `world.rs:1214 enter_world` | `add_map_object` 实体落图 | 恒成功 | 落在墙格→`false`（怪/实体落图失败） |
| 12 | `world.rs:1232 leave_world` | `get_cell_info_mut` 摘除 | `success` 恒真 | 实体坐标是墙格→`remove_from_map` **静默 no-op**（本来也无登记，见 §4） |
| 13 | `world.rs:1785 add_map_routes` | 逐条 `add_map_route` | 全落 | 9 条落墙被丢（`added=1649`） |
| 14 | `world.rs:1906-1911 step_with_route` | `cell_match`+`get_cell_info`+`is_available` 读落点 route | 恒进入 | 落点墙格→`Skipped`（但落点已由 `cell_free` 保证非墙，实际不变） |
| 15 | `world.rs:1960 step_with_route` | `cell_valid(dst)` 门 | 恒真 | 目标格墙→`failed=true`（不换图） |
| 16 | `world.rs:2012 rollback_move` | `add_map_object`（**返回值忽略**） | 恒成功 | 源格本就可走 ⇒ 不变（oracle `:685-688` 同样忽略返回值） |
| 17 | `world.rs:2055 try_step_now` | `has_map_route` 落点非玩家挡路 | 可达 | 墙格 route 已被丢 ⇒ 无影响 |
| 18 | `world.rs:2068-2075 cell_free` | `cell_match`+`ok`+`valid/is_available`+活体 | 墙格恒可走 | **墙格不可走**（玩家走位碰撞，本片最重） |
| 19 | `world.rs:2098-2102 try_step` | `cell_match`+`get_cell_info` 收 blocker | 同 18 | 同 18（仅对非墙目标调用） |
| 20 | `world.rs:2188 can_walk_ignore_objects` | `get_cell_info().1`（`CanWalk(...,true)`） | 恒真 | 墙格→`false`（冲刺被挡帧 `RushKung` 分支可达） |
| 21 | `world.rs:2270 floor_items_sorted` | 全图扫 `CellType::Item`（**对拍用**） | 扫全图 | 扫过墙格但跳过（无物品，输出不变） |
| 22 | `world.rs:2297 floor_items_view_sorted` | 同 21（**生产只读视图**） | 扫全图 | 同上，输出不变（生产掉落从不落格） |
| 23 | `session.rs:189 enter_map` | 进图 `add_map_object` | 恒成功 | **墙格→`CellRejected`（进图失败）** ← §3/§5 的关注点 |
| 24 | `session.rs:263 switch_map_inner` | 切图 `add_map_object` | 恒成功 | 目标格墙→`CellRejected`（生产仅经 route 路径，已前置 `cell_valid`；见 §4） |
| 25 | `session.rs:357 remove_from_map` | `get_cell_info_mut` 摘除 | 恒真 | 坐标是墙格→静默 no-op（同 12） |
| 26 | `aoi.rs:91-136 search_view_range` | 视距窗内 `get_cell_info` 门 | 窗内恒进入 | **墙格跳过**（无对象，`visible_actors` 不变） |
| 27 | `aoi.rs:211-247 search_view_range_death` | 同上（死亡清理） | 同上 | 同上 |
| 28 | `magic.rs:993 can_walk_ignore_objects` | `get_cell_info().1` | 恒真 | 墙→`false` |
| 29 | `magic.rs:1018 char_pushed` | `get_cell_info().1` 先判 | 恒真 | 墙→`break` |
| 30 | `magic.rs:1178 space_move_get_rand_xy` | 经 28 | 201 抽内恒首轮命中 | 墙→进入移位/RNG 支 |
| 31 | `magic.rs:1261 mag_make_clone` | `add_map_object` 结果→`created` | 恒 `true` | 施法者格墙→`false`（不可达：施法者必在 Valid 格） |
| 32 | `merchant.rs:323-324` 商人落格 | `cell_match && cell_valid` + `add_map_object` | 恒成功 | **配置格墙→`add_to_map_success`⇒若非 hide 被移除** |
| 33 | `npc.rs:195-197` NPC 落格 | 同 32 | 恒成功 | 同 32 |
| 34 | `monai.rs:107 count_at_cell` | `get_cell_info`+`is_available` 数活体 | 恒可数 | 墙格→0（同 `GetXYObjCount`，与属性解耦） |
| 35 | `slave.rs:657 recall_slave` | `SpaceMoveGetRandXY` 的 `can_walk` 闭包 | 恒真 | 墙→继续移位/RNG |
| 36 | `castle.rs:591 apply_structure_events` | `set_map_xy_flag` 运行期城门掩码 | 恒可改 | 墙格改不动（**无生产调用方**，零影响） |

### 1C. 宿主消费点（`bins/game-svr/src/**`）

| # | 调用者（file:line 函数） | 语义 | 装载前 | 装载后 |
|---|---|---|---|---|
| 37 | `mongen.rs:645-695 cell_walkable`（经 `:499 cell_walkable_ignore_actors` / `:513 can_walk`） | `CanWalk(true/false)` 门，驱动 31 次移位环 | `ignore_actors` **恒真** ⇒ 环**从不进入** | 出生格墙→**进入移位环**（`Random(Width/2)`/`Random(Height/2)` 抽数）⇒ 刷怪位置 + RNG 流水变（向下收敛） |
| 38 | `queryuser.rs:35 cetin_near_xy` | 3×3 逐格 `ok && is_available` | 恒进入 | 被墙掩码格**跳过**（目标本在 Valid 格 ⇒ 结果不变） |
| 39 | `main.rs:2672 / :2768 / :4995`（`spell_snap_target` 等） | 消费 38 | 同上 | 同上 |
| 40 | `main.rs:7953-7990`（注册表支） | `apply_map_cells` **装载本身** + `add_map_routes` | 全 Walk | 灌属性；`added=1649`（判据行） |
| 41 | `main.rs:1857 floor_items_view_sorted` | 消费 22 | 不变 | 不变 |
| 42 | `main.rs:4650 step_with_route`（玩家走位） | 消费 18 | 墙可穿 | **墙阻挡**（与 oracle `CanWalk` 同构） |
| 43 | `ground.rs:259 put_item` / `:216 cell_match` | **独立地面模型**（`GroundMap`，`cells: Vec<Vec<GroundItem>>`，**无 attribute 字段**） | 不受装载影响 | 不受装载影响（装载只写 `world.maps`，与该模型无共享） |

**另有 2 处（非生产/旁支，列出以证明穷尽）**：
- `crates/envir/src/mapfile.rs:193 MapFile::to_grid`：用 `get_cell_info_mut` 建**新**网格；装载**不许**走它（会覆盖宿主 routes/doors），已在 `mapwire.rs` 模块头登记。
- `crates/world/src/bin/tick_bench.rs`：无 `Valid` 消费。

---

## 2. 与 oracle 对读（同构 / 差异 / 无对应）

| # | 本仓位置 | oracle 位置 | 判定 |
|---|---|---|---|
| 1 | `map.rs:116 valid` | `src/Modules/SystemModule/Data/MapCellInfo.cs:19` | **同构**（`Attribute == CellAttribute.Walk`） |
| 2 | `map.rs:121 is_available` | `MapCellInfo.cs:30` | **同构** |
| 3 | `map.rs:279 cell_valid` | `src/M2Server/Maps/Envirnoment.cs:307-314` | **同构**（含「越界→true」方向） |
| 4 | `map.rs:331/342 get_cell_info(_mut)` | `Envirnoment.cs:322-335` | **同构**（失败回落 `CellArray[0]`） |
| 5 | `map.rs:307 set_map_xy_flag` | `Envirnoment.cs:1362-1369` | **同构**（照搬「非 Valid 不可恢复」怪行为，白名单 **B-15**） |
| 6 | `map.rs:526 add_cell_object` | `Envirnoment.cs:252-299 AddMapObject` | **同构**（`cellSuccess && Valid` 门；懒分配 `ObjList` 等价） |
| 7 | `map.rs:390 add_map_route` | `Envirnoment.cs:169-193 AddMapRoute` | **同构** |
| 8 | `map.rs:560 move_object` | `Envirnoment.cs:337-440 MoveToMovingObject` | **同构**（含 `cellInfo.Valid` 改用 cell0 的耦合，见 §4） |
| 9 | `map.rs:517 has_map_route` | `BaseObject.cs:1548`（`CellType.MapRoute` 支） | **同构** |
| 10 | `world.rs:2068-2075 cell_free` | `Envirnoment.cs:442-486 CanWalk` | **同构** |
| 11 | `world.rs:2188` / `magic.rs:993` `can_walk_ignore_objects` | `Envirnoment.cs:442`（`boFlag=true` 支） | **同构** |
| 12 | `world.rs:1906-1960 step_with_route` | `PlayObject.cs:1849-1892 Walk` + `:3718 EnterAnotherMap`（`!CellValid` → false） | **同构**（门序、`Skipped/Failed` 契约一致） |
| 13 | `world.rs:2012 rollback_move` | `BaseObject.cs:685-688 WalkTo` 回滚支 | **同构**（均**忽略** `AddMapObject` 返回值） |
| 14 | `world.rs:1214 enter_world` | `BaseObject.cs:1366-1375 AddToMap`→`:252 AddMapObject` | **同构**（单点落格）；**但**玩家进入整链见 §5 |
| 15 | `aoi.rs:91/211 search_view_range(_death)` | `BaseObject.ViewRange.cs:57-82 / :159-187` | **同构**（`cellSuccess && IsAvailable` 门；窗 `CurrX±ViewRange`） |
| 16 | `magic.rs:1178 space_move_get_rand_xy` | `BaseObject.cs:1040-1084` | **同构** |
| 17 | `slave.rs:657 recall_slave` 的 `SpaceMoveGetRandXY` | `BaseObject.cs:1072/1128` | **同构** |
| 18 | `magic.rs:1018 char_pushed` | `BaseObject.cs:854-901 CharPushed`（`CanWalk(nx,ny,false)` 先判） | **同构** |
| 19 | `magic.rs:1261 mag_make_clone` | `MagicManager.cs:1337-1341 MagMakeClone` | 结果值语义**同构**（`add_map_object` 的 `created`）；**注**：oracle 该方法本身**不校验**返回值，本仓把「格子能否放下」落成 `created` —— 差异仅在施法者格非 Valid 时可见（不可达） |
| 20 | `merchant.rs:323 / npc.rs:195` | `BaseObject.Base.cs:13-21 Initialize`（`CanWalk(x,y,true) && AddToMap()`）+ `WorldServer.Merchant.cs:18/47`（`AddtoMapSuccess && !IsHide` → 移除） | **同构** |
| 21 | `monai.rs:107 count_at_cell` | `Envirnoment.cs:1062-1093 GetXYObjCount` | **同构** |
| 22 | `mongen.rs:645-695` + `:480-540` 移位环 | `WorldServer.MonGen.cs:730-800`（`:751 AddtoMapSuccess`、`:765 !CanWalk(...,false)`、`:787 AddMapObject`） | **同构**（含「`CanWalk(true) && AddToMap()` 短路 ⇒ 进环 ⟺ `!CanWalk(true)`」的推理，见 §4） |
| 23 | `queryuser.rs:35 cetin_near_xy` | `PlayObject.Base.cs:984-1011 CretInNearXy` | **同构** |
| 24 | `main.rs:7953-7990` 装载次序 | `GameSrv/Maps/MapManager.cs:96 LoadMapData`（在 `:84-96 AddMapInfo` 内）**早于** `GameSrv/Maps/Maps.cs:379 AddMapRoute` | **同构**（顺序硬约束成立） |
| 25 | `ground.rs:259 put_item` | `Envirnoment.cs:88-165 AddItemToMap` | **差异**（oracle 有 `cellSuccess && cellInfo.Valid` 门，本仓独立模型**无 attribute 字段**、不建模该门）。**但**：① 该模型与 `world.maps` 无共享 ⇒ **装载不改变它**；② 生产 `MapItem.ItemId` 恒 0 ⇒ oracle 首行即 `false` ⇒ 该门不可观测；③ 金币合并/容量上限分支亦标未建模（`ground.rs:106`）。⇒ **装载引入的差异 = 0**，属既有简化（**已在 `ground.rs:105-106` 登记**）。 |
| 26 | `castle.rs:591 apply_structure_events` | `Envirnoment.cs:1362 SetMapXyFlag`（运行期城门） | **同构**，但**无生产调用方**（归卡 `M3⑧`，`mapwire.rs:661-666` 已登记） |
| 27 | `envir/mapfile.rs:193 to_grid` | oracle 无对应 gameplay 路径 | **oracle 无对应**（本仓自造；登记「生产不得使用」，`mapwire.rs` 模块头） |

**小结**：装载后真机可达的**行为变化面 6 类**（消费点 18/42、37、32/33、8/#13、26/27、23/38）**全部 oracle 同构**；
**唯一未登记差异**是消费点 23 的**失败处置**（见 §3、§5）。

---

## 3. 风险排序（真机可达性 × 后果严重度）

| 排序 | 消费点 | 真机可达 | 后果面 | 判定 | 建议动作 |
|---|---|---|---|---|---|
| **1** | **`mongen.rs:645-695` 刷怪移位环** | **是**（每 tick 刷怪） | **RNG 流水 + 刷怪位置**（R 级） | oracle 同构（`WorldServer.MonGen.cs:730-800`） | **补牙已有**（M2/M3/M7）；**窗口必核**「刷怪位置 + RNG 流水」（已在 `b192bd9f3` 判据钉住：2827 条里 1072 条墙格、40 条首轮进边界抽） |
| **2** | **`world.rs:2068-2075 cell_free` 玩家走位碰撞** | **是**（100% 走位） | **帧面**（走位/推/冲刺）、几何 | oracle 同构 | **立窗口必核项**（`mapwire.rs` 模块头的「其余同族消费点」已列，但权重应提升为与刷怪同级；建议补一条 walk-block 进程级对拍） |
| **3** | **`merchant.rs:323` / `npc.rs:195` NPC·商人落格** | **是**（启动加载 NPC/商人表） | **内容面**（配置格是墙的 NPC 被**静默移除**） | oracle 同构 | **立卡/登记**：统计真机 `Envir/Npc.txt`、`Merchant.txt` 中落在墙格的条数；若 >0 则列入窗口「被移除 NPC 清单」 |
| **4** | **`map.rs:390`/`world.rs:1785` route 落格 1658→1649** | **是** | **几何/换图面**（9 条连接点消失：7 HighWall + 2 LowWall） | oracle 同构 | **已登记**（`b192bd9f3` 正文 + `mapwire.rs`）——无需新动作，窗口核 `added=1649` |
| **5** | **`session.rs:189 enter_map` CellRejected** | **边缘**（新角色默认出生格经独立复核为 `Walk`；仅当存档 `CurX/CurY` 指向墙格时可达） | **登录被拒**（角色进不了世界） | **差异**（oracle 保留角色、只是不落图） | **立卡**（或先在 `mapwire.rs` 登记）；见 §5 |

---

## 4. 反面核对（有没有「装载后**反而恒假**」的条目）

**总判**：**0 条**。机理上不可能：`MapGrid::set_cell_attributes`（`map.rs:442-450`）**只写 `Attribute`**，
对 `cells[i].obj_list` 一动不动；而「依赖 `obj_list` 非空」的判据（`is_available`）与属性**正交**，装载不改其值。

逐条核查最可疑者：

1. **`move_object` 的 cell0 耦合（消费点 9）** —— **同形，非恒假**。
   `map.rs:576` 首门 `if !ignore_block && cell_success`；目标墙格时 `cell_success=false` ⇒ 跳过。
   随后 `map.rs:593-595` `let (cell,_) = get_cell_info(to_x,to_y); if !cell.valid() { return false; }` —— 该 `cell` 是
   **`cells[0]`**（因目标非 Valid）。oracle 对应处为 `Envirnoment.cs:380` `if (canMove && cellInfo.Valid)`，
   `cellInfo` 同为目标非 Valid 时的 `CellArray[0]` ⇒ **同形**。
   ⇒ 真机后果：本片后 `0.map` 的 `(0,0)` = HighWall ⇒ `cells[0].valid()==false` ⇒ `move_object(目标墙格)` 早退、
   **不摘源格**；装载前 `cells[0].valid()==true` ⇒ 会摘源格但不落目标格。**两者都与 oracle 逐字节同形**
   （oracle 对 `0.map` 也读到 `CellArray[0].Valid==false`）。**不是恒假，是 oracle 对齐**。
   注意：该同形**依赖「(0,0) 是墙」**。`b192bd9f3` 正文④已登记「cell 0：真 0.map (0,0) 即 HighWall」。
   对 **(0,0) 是 `Walk` 的图**，`move_object(目标墙格)` 会「摘源格 + 不落格」，oracle 同样如此 ⇒ 仍同构，但**更易被误读成回归**。

2. **`leave_world` / `remove_from_map`（消费点 12/25）** —— **不是恒假**。
   坐标若是墙格，`get_cell_info_mut` 返回 `success=false` ⇒ 早退。但「实体坐标 = 墙格」只可能来自
   `rollback_move`（坐标更新但落格失败的病态态）或 DB 直填；此情形下**本来也无登记可摘**，早退与逐个 `retain` 等价。

3. **`aoi.rs` 视距扫描（消费点 26/27）** —— **不是恒假**。
   墙格被跳过 ⇒ 该格 `visible_actors` 贡献为 0。而 `world` 内**对象只能落在 Valid 格**（`add_cell_object` 门），
   装载在**任何实体落图之前**（`main.rs` 注册表支，早于 `sim_loop` 建实体/刷怪）⇒ 墙格**不可能**有已注册对象。
   ⇒ `visible_actors` 与装载前**逐条相同**。

4. **`floor_items_view_sorted` / `floor_items_sorted`（消费点 21/22/41）** —— **不是恒假**。
   生产掉落从不落格（`add_floor_item` 无生产调用方），输出恒空；装载只让「空扫描」少覆盖若干墙格。输出不变。

5. **`cetin_near_xy`（消费点 38/39）** —— **不是恒假**。目标必在 Valid 格（同 3 的推理）⇒ 3×3 内被跳过的只可能是空墙格 ⇒ 命中结果不变。

6. **`has_map_route`（消费点 10/17）** —— **不是恒假**。route **只落在 Valid 格**（装载在 route 之前），
   且该格不会因装载而变墙（装载是一次性、早于 route）⇒ route 格恒 Valid ⇒ 可达。

7. **`enter_world`（消费点 11）** —— **不是恒假，但会变成「条件失败」**：怪/实体落墙格 ⇒ `None`（`spawn_monster_precomputed`
   回退 `entities.remove`）。而刷怪环已先行保证 `can_walk` 命中非墙格 ⇒ 生产不可达。

8. **`switch_map_inner`（消费点 24）** —— **不是恒假**。生产唯一入口 `step_with_route` 已前置 `cell_valid(dst)`（消费点 15），
   墙目标在到达 `switch_map` 前即 `failed=true` 分流。故其 `CellRejected` 臂在生产**不可达**（除 OOB dst，与装载无关）。

---

## 5. 没能判定的条目

| 条目 | 原因 | 已知/建议 |
|---|---|---|
| **本仓 `session.rs:189 enter_map` 拒绝进图 vs oracle 保留角色** | 无法在本卡约束下**实跑**登录链验证；需追溯 oracle「`AddToMap()` 失败后角色去向」的完整调用链（`WorldServer.cs` 登录支 + `PlayObject.Initialize` 调用点） | 静态对读结论：oracle `BaseObject.Base.cs:13-21` 为 `AddtoMapSuccess=true; if (CanWalk && AddToMap()) AddtoMapSuccess=false;`，**拒绝时不移除角色**；本仓 `enter_map` 失败即 `entities.remove(id)` + `Err`（`session.rs:188-192`），生产 `main.rs:9576-9578` 只记 `WRN` 不落图。**这是本仓自造的失败处置，与 oracle 不等价**。真机可达性：**新角色默认出生格经独立复核为 `Walk`（不可达）**；仅「存档 `CurX/CurY` 落在墙格」（DB 直改/旧图版本差异）可达。**未登记** |
| `magic.rs` 的 `char_pushed`/`mag_sace_move`/`mag_make_clone` 的**生产可达性** | 未逐一追 `DoSpell`/`MagMakeClone` 在 `bins/game-svr` 的接线（`slave::spawn_clone` 已确认生产用；`char_pushed` 只见 `magic.rs:1109` 内部与测试） | 门形已判**同构**；可达性属另一维度，建议由 `M3` 施法批确认 |
| `slave.rs:657 recall_slave` 的生产可达性 | `game-svr` 未直接调 `recall_slave`（可能经 `cast_slave_spell`） | 门形同构；建议由 `M3` 召唤批确认 |
| 真机 `Npc.txt`/`Merchant.txt` 中「配置格是墙」的**条数** | 需解析 `E:/MirServer/M2GameSvr/Envir/*.txt` 并逐格查 `.map`（可行但超出本次静态范围，且涉及多图） | **建议立卡**：与 `MonGen.txt` 同法统计，作为窗口「被移除 NPC 清单」 |
| `0.map` 之外各图 `(0,0)` 是否 `Walk`（决定 §4-1 的行为分支） | 未逐图解二进制 | 可一键复核：见 §6 命令 R8 |

---

## 6. 复核命令（可直接复制，含期望输出片段）

```bash
# R1 消费点全集（world 生产面，排除 map.rs 与测试）
grep -rn "get_cell_info\|cell_valid\|cell_attribute\|\.valid()\|is_available\|add_map_object\|add_floor_item\|add_map_route\|has_map_route\|set_map_xy_flag\|\.cell(\|cell_match" \
  mir2-rs/crates/world/src/*.rs | grep -v "map.rs:"
# 期望：aoi.rs 9 行 / magic.rs 4 行 / merchant.rs / npc.rs / monai.rs / session.rs 3 行 / slave.rs / world.rs 20+ 行

# R2 消费点全集（game-svr 生产面）
grep -rn "cell_walkable\|can_walk\|get_cell_info\|cetin_near_xy\|add_map_object\|apply_map_cells" mir2-rs/bins/game-svr/src/*.rs | grep -v "^[^:]*:[0-9]*: *//"
# 期望：mongen.rs:231/499/513/645/652/678/685/695 ；queryuser.rs:35 ；main.rs:2672/2768/4995/7953-7990

# R3 谓词本体（同构锚）
sed -n '110,125p;279,300p;331,350p' mir2-rs/crates/world/src/map.rs
# 期望：valid()=>Attribute==Walk ；cell_valid 越界 return true ；get_cell_info 失败 return (&self.cells[0], false)

# R4 oracle 门（对读锚）
sed -n '19p;30p' src/Modules/SystemModule/Data/MapCellInfo.cs
sed -n '307,335p;337,440p;442,486p' src/M2Server/Maps/Envirnoment.cs
# 期望：Valid=>Attribute==CellAttribute.Walk ；CellValid 越界 return true ；MoveToMovingObject 的 if (canMove && cellInfo.Valid)

# R5 刷怪环 oracle 锚
sed -n '751,800p' src/GameSrv/Word/WorldServer.MonGen.cs
# 期望：if (cert.AddtoMapSuccess){ ... while(true){ if(!CanWalk(...false)){...Random(Width/2)...} else { outofrange=AddMapObject(...); break; } ... } }

# R6 NPC/商人 oracle 锚
sed -n '13,21p' src/M2Server/Actor/BaseObject.Base.cs && sed -n '16,20p;45,50p' src/GameSrv/Word/WorldServer.Merchant.cs
# 期望：AddtoMapSuccess=true; if (Envir.CanWalk(CurrX,CurrY,true) && AddToMap()) AddtoMapSuccess=false; / if (merchant.AddtoMapSuccess && !merchant.IsHide)

# R7 独立复核新角色出生格（真机 0.map，只读）
python -I -c "
import struct
d=open(r'E:\MirServer\M2GameSvr\Map\0.map','rb').read(); W,H=struct.unpack_from('<hh',d,0)
def a(x,y):
    o=52+(x*H+y)*12; bk,mid,fr=struct.unpack_from('<HHH',d,o)
    return 'LowWall' if fr&0x8000 else ('HighWall' if bk&0x8000 else 'Walk')
print([(p,a(*p)) for p in [(289,618),(650,631),(287,616),(291,620),(648,629),(652,633)]])"
# 期望（2026-10-17 实测）：全部 'Walk'（⇒ 默认新角色登录不被拒；§3 第 5 条风险仅存档坐标可达）

# R8 各图 (0,0) 属性（决定 §4-1 行为分支）
python -I -c "
import struct,glob,os
for p in sorted(glob.glob(r'E:\MirServer\M2GameSvr\Map\*.map'))[:5]:
    d=open(p,'rb').read(); W,H=struct.unpack_from('<hh',d,0)
    bk=struct.unpack_from('<H',d,52)[0]; fr=struct.unpack_from('<H',d,52+4)[0]
    print(os.path.basename(p),W,H,'cell0=','LowWall' if fr&0x8000 else ('HighWall' if bk&0x8000 else 'Walk'))"
# 期望：0.map → cell0=HighWall（本片正文④的锚）

# R9 「装载只写 Attribute、不碰 ObjList」（§4 总判）
sed -n '442,452p' mir2-rs/crates/world/src/map.rs && grep -n "fn add_cell_object" -A 20 mir2-rs/crates/world/src/map.rs
# 期望：set_cell_attributes 体内只有 cell.set_attribute(*attr)；obj_list 不在其中

# R10 未登记差异点
sed -n '180,195p' mir2-rs/crates/world/src/session.rs && sed -n '9570,9580p' mir2-rs/bins/game-svr/src/main.rs
# 期望：if !..add_map_object(..) { self.entities.remove(id); return Err(SessionError::CellRejected); } / Err(e) => glog!("WRN","进世界失败 …")
```

---

## 7. 残留物清单

- 本报告：`/e/tmp/valid-consumers-audit.md`（唯一写盘物）。
- 仓库：**零改动**（`git status` 干净；未建 worktree、未 `git add`/`commit`）。
- 未起任何服务/客户端/假人；未占端口；未写库；未动 `E:/MirServer/**`（仅**只读** `0.map` 与 `StartPoint.txt`/`Server.conf`）。
- 未跑 cargo。
