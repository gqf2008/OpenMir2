# AOI 卫士可见性审计（卫士 `(281,661)` 无出现帧 + `vis=0` 采样）

- 审计日期：2026-10-18
- 基线：`429c714a4`（master，detached worktree `/e/tmp/impl-aoiaudit`）
- 触发：缩窗实拍（2026-10-18）
  - 玩家摆位 `(291,666)`；现场观察“实际落 `(286,652)`”。
  - capture 中**没有**卫士 `(281,661)` 的坐标帧（`SM_TURN(10)`/`SM_WALK(11)` 的 `param=281,tag=661` 均 0 条）。
  - `SM_FEATURECHANGED(41)` 有 5 个 actor。
  - 服务端 AOI 诊断行出现 `vr=12 vis=0 new=0 af=28 same_map=1748 nearest=10/5`（窗口内却不可见）。
  - `check_aoi_visibility_diag.py --expect visible --min-vis 1` 整体 GREEN（max vis=8）。
- 方法：只读源码实读 + 现场日志/抓包统计 + 独立解析 `MapInfo.txt` / `Map/0.map` 二进制；**未改 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`、`E:/MirServer/**`；未起服务、未占端口、未写库**。
- 引用纪律：**oracle 行号逐条实测**；**Rust 侧只引符号/文件路径，不写 `:行号`**。

---

## 0. 结论摘要

| 返回字段 | 结论 |
|---|---|
| `guard_placed` | **是**。`Envir/GuardList.txt` 49 条全部放置成功（`世界内守卫 49，跳过 0`），地图 `0` = `map_id 26`，坐标 `(281,661)`，`Dir=5`，格子可走 ⇒ **未进地形移位环**。 |
| `guard_frames_expected` | **应发 1 对**：`SM_TURN(10)`（`recog=guard_id`，`param=281`，`tag=661`，`series=(dir|light<<8)`，本例 `dir=5`）**+** `SM_FEATURECHANGED(41)`（同 `recog`）。**不是 `SM_WALK(11)`**——卫士静止，出现帧只有 `SM_TURN`；`SM_WALK` 只在真正走步时发。 |
| `vis_zero_verdict` | **混合，且含真缺陷**：`tick #2600/#2650/#2700`（`nearest=10/10/5`）是**真缺陷**（卫士实体仍在窗口内，但其格子项在 `tick #300` 已被 60s 过期逐出；`nearest` 读 `world.entities` 故仍报距离）；`tick #1900`（`nearest=1`）是**玩家已死亡**（`SM_NOWDEATH (605,588)`，`search_view_range_death` 清空 `visible_actors`）属预期；`map=98` 的 `nearest=52..63` 属**窗口外**（`nearest` 统计全部同图实体，不过滤死亡/隐身/已过期）。 |
| `defect` | **true**。根因：Rust **未建模 oracle `BaseObject.Run()` 的 30s `VerifyMapTime` 刷新**；并且玩家侧 AOI 扫描额外执行了 oracle `PlayObject.SearchViewRange` 里**已被注释掉**的 60s 过期。静止 actor（49 卫士 / 131 商人 / 3 固定 NPC / 停走怪）在 60s 后被第一个覆盖其格子的扫描从 `ObjList` 删除，永远进不了 `VisibleActors` ⇒ 无出现帧；卫士与 131 个商人的挂图都在首个 tick 前、`add_time=0`，故 `tick #300`（`clock=60000ms`）即中招。 |

一句话链：**卫士已放置且在视距内 → 它的 `CellObject.AddTime` 停在装载时刻（0）→ 60s 后任何扫描把它从格子表删除 → `visible_actors` 永远不含它 → `drain_aoi_downlink` 没有 `Show` 可发 → 客户端 0 条 `SM_TURN(281,661)`**。`nearest` 读的是 `world.entities`（不是格子表），所以仍报 10/5。

---

## 1. 现象与现场证据

### 1.1 服务端日志（`/e/tmp/window/cliente2e/rust/stack/game-svr.out.log`）

装载与放置（时间戳为准）：

```text
22:35:00.238 [INF] Envir/GuardList.txt 已载入：49 条守卫配置
22:35:01.828 [INF] 守卫列表放置完成：49 条配置，世界内守卫 49，跳过 0（未知地图/未知模板名/移位失败 ⇒ oracle null 支）
```

`map_id 26` 会话的 AOI 诊断采样（节选）：

```text
22:43:37.186 [INF] 进世界 ChrName:aaa CurMap:0 (291,666) Dir:4 entity=8569
22:43:42.866 [DBG] sim: tick #2600 clock=520000ms ... [aaa(8569) map=26 vr=12 vis=0 new=0 af=28 same_map=1748 nearest=10]
22:43:52.893 [DBG] sim: tick #2650 clock=530000ms ... [aaa(8569) map=26 vr=12 vis=0 new=0 af=28 same_map=1748 nearest=10]
22:44:02.916 [DBG] sim: tick #2700 clock=540000ms ... [aaa(8569) map=26 vr=12 vis=0 new=0 af=28 same_map=1748 nearest=5]
22:44:12.942 [DBG] sim: tick #2750 clock=550000ms ... [aaa(8569) map=26 vr=12 vis=3 new=0 af=40 same_map=1748 nearest=2]
```

启动早期实体/展示表计数（用于佐证 49 只卫士确有展示表项）：

```text
22:35:11.651 [DBG] sim: tick #50 clock=10000ms ... 实体内 204 展示表 70 在世界会话 0
```

算术：`131 商人 + 3 固定 NPC + 49 卫士 = 183`，`204 - 183 = 21` 为已刷怪；`70 - 49 = 21` 与之一致 ⇒ **旁证** 49 只卫士各有一条展示表项（不依赖商人/NPC 是否登记展示表）。

**会话编号口径**：日志 `会话准入登记 ... SessionId:N` 的 N 是 **SessionId**（本 run = 2/3/4），不是登录次序；登录第 1/2/3 次分别对应 SessionId 2/3/4、`entity=6788/8164/8569`。下文一律写 `SessionId N`（必要时附登录次序与 entity），不再混用“会话 N”。

### 1.2 抓包统计（`/e/tmp/window/cliente2e/rust/s2cC/capture.jsonl`，对应 SessionId 4 / 登录第 3 次 / `entity=8569`）

| ident | 名称 | 条数 | 说明 |
|---|---|---|---|
| 10 | `SM_TURN` | 6 | recog = `845/382/403/403/389/845`，坐标 `(269,653)/(269,666)/(262,653)/(261,647)/(268,652)`，**无 `(281,661)`** |
| 11 | `SM_WALK` | 0 | 本会话无任何走步帧 |
| 41 | `SM_FEATURECHANGED` | 7 | 5 个 actor：`8569`（自己）、`845`、`382`、`403`、`389` |
| 30 | `SM_DISAPPEAR` | 3 | 与出现帧成对 |

卫士坐标过滤：`SM_TURN ∪ SM_WALK` 中 `param==281 && tag==661` 的条数 = **0**（`s2cA`/`s2cB`/`s2cC` 三份最新抓包均为 0）。`SM_TURN` 只覆盖 4 个怪物 actor，说明**“出现帧编码/下发通路本身可用”**（怪物能出 `SM_TURN`+`SM_FEATURECHANGED`），缺口只发生在“卫士进不了 `Show`”。

### 1.3 坐标口径

- 服务端进世界日志是**移位环之后**的坐标（Rust `main` 的进世界块先调 `mapwire::shift_enter_position`，再打印 `进世界 ... (x,y)`）。本会话 `(291,666)` 的格子经独立解析为 `Walk`（见 §2.3），因此**移位环 0 抽、0 位移**，服务端记录坐标就是 `(291,666)`。
- 现场观察的 `(286,652)` 与服务端记录不一致；但从 `(286,652)` 到卫士 `(281,661)` 的 Chebyshev 距离是 `max(5,9)=9 ≤ 12`，**仍然在视距内**。⇒ 坐标口径的“5/14 格差”不改变本审计结论。
- 会话内 `CM_WALK`（c2s ident `3013`/`3011`，`recog=(y<<16)|x`）的走位时间线（相对登录）：首次走步在 **+18.96s**，到 `tick #2600`（+5.68s）与 `#2650`（+15.71s）时**尚未走位**，玩家仍在 `(291,666)`；到 `tick #2700`（+25.73s）时最近一次走步是 `(283,656)`。这正好解释 `nearest=10 → 10 → 5`。
- SessionId 2（登录第 1 次，`entity=6788`）22:40:48 进世界于 `(605,581)`；`s2cA` 在 22:41:15.3 记录 `SM_NOWDEATH ident=34 recog=6788 param=605 tag=588` ⇒ 玩家已走到 `(605,588)` 并**死亡**。该会话在 `tick #1900`（22:41:22.5）仍是 `在世界会话 1`，22:41:23 才断开。⇒ `tick #1900` 的 `vis=0` 是死亡后的正常清空（`search_view_range_death`），不是 AOI 缺陷（见 §4.2/§5.2）。

---

## 2. 问题 1：GuardList 是否真的把卫士放到 map 26？

### 2.1 oracle 链（行号实测）

| 环节 | oracle 位置 | 语义 |
|---|---|---|
| 加载门 | `src/GameSrv/GameApp.cs:239` `if (!SystemShare.Config.VentureServer)`；`:241` `LocalDb.LoadGuardList();`；`:238` 先 `InitializationMonsterThread()`；`:244` 后 `WorldEngine.Initialize()` | 非 Venture 模式才读；装载在首个 regen tick 之前 |
| 解析 | `src/GameSrv/DB/LocalDB.cs:100` `LoadGuardList()`；`:119` `GetValidStrCap(..., ' ')`；`:128` 入库门（name/map/dir 非空）；`:130` `RegenMonsterByName(...)`；`:133` `guard.Dir = (byte)StrToInt(direction, 0)` | 逐行解析/放置（**实测订正**：Rust `crates/envir/src/guardlist.rs` 模块头引用的 oracle 行号 `117/123/134` 与本基线实测差 1–5 行，以本表为准） |
| 放置 | `src/GameSrv/Word/WorldServer.MonGen.cs:382` `RegenMonsterByName` | `GetMonRace` → `CreateMonster` → `GetMonsterThreadId` → 合成 `MonGenInfo`（`Race` 不回填，`:819` `if (monGen.Race > 0)` 早退） |
| 时机 | 首次 `WorldEngine.Initialize()` 之前 | 先于首个 AOI tick |

### 2.2 Rust 对应（符号/文件）

- `mir2-rs/crates/envir/src/guardlist.rs`：`parse_guard_line` / `load_guard_list`（`LocalDB.cs:100-144` 的逐行 1:1）。
- `mir2-rs/bins/game-svr/src/guardlist.rs`：`load_guard_list_file`、`place_guards`、`regen_monster_by_name`、`GuardPlacementReport`。
- `mir2-rs/bins/game-svr/src/main.rs`：主线程装载块（`mon_gen_config.venture_server` 门 + `load_guard_list_file`）；sim 线程放置块（`place_guards`）。
- `mir2-rs/crates/envir/src/registry.rs`：`MapRegistry::from_entries`（`map_id` = 按 `MapInfo.txt` 顺序**加载成功**的下标）。

### 2.3 放置证据（实测）

1. **日志**：`GuardList.txt 已载入：49`、`世界内守卫 49，跳过 0`（§1.1）。
2. **地图名 → map_id**：独立解析 `E:/MirServer/M2GameSvr/Envir/MapInfo.txt`（GBK）：322 个 section、全部 `Map/*.map` 存在；按 section 顺序去重后，**`0` 的下标 = 26**。与 `mir2-rs/bins/game-svr/src/mapwire.rs` 模块注释“`0.map`=id26”一致。⇒ 会话 `map=26` 就是地图名 `0`（比奇）。
3. **卫士行**：部署件 `GuardList.txt` 第 28 行实测为 `卫士   0        281, 661    : 5`（GBK）⇒ `x=281,y=661,dir=5`。
4. **未移位**：独立解析 `E:/MirServer/M2GameSvr/Map/0.map`（700×700，5880052 B，索引 `k=x*height+y`，属性 = `fr>>15` 先于 `bk>>15`）：`(281,661)=Walk`。⇒ `create_monster_oracle` 的 `cell_walkable_ignore_actors` 首查即真，**不进 31 次移位环**，落点仍是 `(281,661)`。
5. **展示层**：`place_guards` 成功支同时写 `Entity.ai.dir = g.dir` 与 `DisplayTable` 的 `dir`；`tick #50` 的 `展示表 70` 比 49 卫士多 21（= `204 - 131 - 3 - 49`，已刷怪数）⇒ 旁证 49 只卫士均有展示项。

**结论 1**：GuardList **确实**把 49 只卫士放到了 `map_id 26`（地图名 `0`）的配置坐标上；`(281,661)` 那只存在、可走、未移位、有展示项。⇒ **“未放置”不是原因**。

---

## 3. 问题 2：进入视野的 actor 如何下发位置

### 3.1 oracle 的发送时机（行号实测）

1. **玩家搜索节拍**：`src/GameSrv/Word/WorldServer.cs:666` 调 `playObject.SearchViewRange()`；门在 `:663` `(GetTickCount - SearchTick) > SearchTime`；`PlayObject.cs:900` `SearchTime = 1000`（≈1s 一次）。
2. **搜索/打标**：`src/M2Server/Player/PlayObject.cs:3073` `override SearchViewRange()`。先把 `VisibleActors[i].VisibleFlag = Hidden`；扫 `(CurrX±ViewRange, CurrY±ViewRange)` 的格子；命中过滤器后 `UpdateVisibleGay`。
3. **过滤器**：`src/M2Server/Actor/BaseObject.ViewRange.cs:108-115`：
   `!Death && !Invisible && !Ghost && !FixedHideMode && !ObMode` 且
   `(Master != null || NastyMode || WantRefMsg || (对方 Master != null 且距离 ≤3) || 对方 Race == Play)`。
   玩家 `WantRefMsg = true`（`PlayObject.cs:889`）⇒ **普通怪/卫士都通过**。
4. **`Show` 的判定**：`BaseObject.ViewRange.cs:12-40` `UpdateVisibleGay`：已在列表 ⇒ 标 `Invisible`；否则 `VisibleFlag = Show` 追加（`:36-39`）。
5. **出帧**：同一 `SearchViewRange` 尾段（`PlayObject.cs:3210-3245`）：
   - `Hidden` ⇒ `:3216` `SendMsg(baseObject, RM_DISAPPEAR, ...)`；
   - `Show` ⇒ `:3240` `SendMsg(baseObject, RM_TURN, baseObject.Dir, baseObject.CurrX, baseObject.CurrY, 0, baseObject.GetShowName())`（死亡走 `:3227-3237` 的 `RM_DEATH`/`RM_SKELETON`）。
6. **`RM_TURN` → 线上帧**：`PlayObject.Message.cs:1524` `case RM_TURN`；`:1542` `MakeMessage(SM_TURN, ActorId, nParam1=x, nParam2=y, MakeWord(dir, Light))`；`:1558` 无条件再发 `SM_FEATURECHANGED(ActorId, LoWord(feature), HiWord(feature), FeatureEx)`。⇒ 每个“新出现 actor”**恰好 1 条 `SM_TURN` + 1 条 `SM_FEATURECHANGED`**。
7. **静态 actor**：上述路径只按 `VisibleFlag::Show` 发帧，与 actor 是否移动无关。**静止 actor 进入视野同样发位置帧**。
8. **怪物/卫士自己的搜索**：`WorldServer.MonGen.cs:300` 只对 `IsSlave || Race ∈ {Guard, ArcherGuard, SlaveMonster}` 调 `:311 monster.SearchViewRange()`；`:315` 每 tick `monster.Run()`。卫士的 `GuardUnit.SearchViewRange`（`GuardUnit.cs:127`）是另一套（只对 6 格内目标 `UpdateVisibleGay`，且 `CanPassiveAttack` 过滤），**不负责给玩家下发位置**。

### 3.2 Rust 对应路径（符号/文件）

- `mir2-rs/crates/world/src/aoi.rs`：`search_view_range`（打标/扫描/过滤器/`update_visible_gay`）、`CELL_OBJECT_EXPIRE_MS`。
- `mir2-rs/crates/world/src/world.rs`：`World::tick` 的 AOI 刷新段（对每个实体调 `search_view_range`）。
- `mir2-rs/bins/game-svr/src/main.rs`：`drain_aoi_downlink`（遍历 `observer.visible_actors` 的 `Show`）。
- `mir2-rs/bins/game-svr/src/downlink.rs`：`appear_frames` → `turn_pair` = `SM_TURN` + `SM_FEATURECHANGED`；怪物出现帧用 `target.ai.dir` 覆盖展示层方向。

**Rust 对静止 actor 是否发位置帧？—— 是。** `drain_aoi_downlink` 只看 `VisibleFlag::Show` 与“有展示表”，不看目标是否移动；`turn_pair` 用实体的当前 `x/y` 与 `ai.dir`。卫士若进入 `VisibleActors` 的 `Show`，就会得到 `SM_TURN(281,661)` + `SM_FEATURECHANGED`。

**结论 2**：下发链路不是原因；卫士从未进入 `Show`（§4 说明为什么）。

---

## 4. 问题 3：`nearest` 与 `vis` 的口径

### 4.1 定义（Rust `world_aoi_diag` / `SessionAoiDiag`）

- `vis` = 观察者 `Entity.visible_actors` 里 `target != self && flag != Hidden` 的条数（`Show | Invisible`）。
- `nearest` = 遍历 `world.entities`，对 `id != self && map_id == self.map_id` 取 `max(|dx|,|dy|)` 的最小值。
- `nearest` **不过滤** `death`/`ghost`/`invisible`/`fixed_hide_mode`/`ob_mode`，**也不检查该实体是否还在格子表 `ObjList` 里**。
- `af` = `aoi_frames_total`（按 `chr_name` 键）的累计值；该表在 sim 线程**循环外**创建，**不随新会话清零**。

### 4.2 逐样本对照

| 采样 | 玩家服务端坐标（依据） | 到卫士 `(281,661)` 的 Chebyshev | `nearest` | 判读 |
|---|---|---|---|---|
| `tick #2600` | `(291,666)`（进世界坐标；此时尚未走位） | `max(10,5)=10` | **10** | `nearest` **推断**为卫士（诊断只输出 Chebyshev；距离精确匹配；见下方口径说明）；`vis=0` ⇒ 卫士在 `world.entities` 但不在 `visible_actors` ⇒ **真缺陷症状** |
| `tick #2650` | 同上 | `10` | **10** | 同上（`nearest` 仍**推断**为卫士） |
| `tick #2700` | `(283,656)`（`CM_WALK` 在 +25.46s；+25.73s 为采样时间） | `max(2,5)=5` | **5** | `nearest` **推断**仍为卫士（10→5 随玩家走位；见下方口径说明）；`vis=0` ⇒ **同一缺陷** |
| `tick #2750` | `(265,652)`（+33.87s） | `max(16,9)=16` | **2** | `nearest=2` 是别的实体；`vis=3`、`af 28→40`（+12 = 6 对出现帧）⇒ **新刷/新进视野的怪能正常出现** |
| `tick #1900`（SessionId 2，登录第 1 次，`entity=6788`） | `(605,588)`（22:41:15.3 `SM_NOWDEATH recog=6788 param=605 tag=588`：玩家已走到此处并死亡） | — | **1** | **不是缺陷**：玩家已死亡，`search_view_range_death` 把 `visible_actors` 整体清空 ⇒ `vis=0` 属预期；`tick #1900`（22:41:22.5）时该会话仍在 `在世界会话 1`，22:41:23 才断开。`nearest=1` 是死亡后仍留在 `world.entities` 的相邻实体（含尸体/其它 actor；`nearest` 不过滤） |
| `map=98` 的 4 条 | SessionId 3（登录第 2 次，`entity=8164`）回落 `HomeMap=3 (839,668)` | — | **52..63 > vr=12** | **窗口外，自洽** |

**`nearest` 身份口径**：诊断只输出 Chebyshev 距离、**不输出实体 id**；`nearest` 遍历 `world.entities`，不排除 `death`/隐身/已过期，也不排除 `add_time=0` 的商人与固定 NPC。因此把 `nearest=10/10/5` 认作卫士只是**基于距离的推断**（10/10/5 与卫士 `(281,661)` 精确匹配）。补充论证：卫士在 `tick #300` 已被逐出格子表（§4.3），但它仍在 `world.entities` 中；本审计掌握的地图实体清单里没有其它已知同距实体可解释这三个读数。即使身份推断有误，也只影响“最近实体是谁”，不影响 `vis=0` 的根因结论。

### 4.3 为什么卫士 `vis=0`——根因链（静态实读）

1. **卫士装载时刻 `add_time=0`**：`place_guards` 在首个 tick 前执行，`World.clock_ms` 初值 0；`spawn_monster_precomputed` → `enter_world` → `add_map_object(..., now=clock_ms)` 写入 `CellObject.add_time = 0`。
2. **AOI 扫描有 60s 过期分支**：`mir2-rs/crates/world/src/aoi.rs::search_view_range` 对 `obj.actor_object && now_ms - obj.add_time >= CELL_OBJECT_EXPIRE_MS` 的格子项**先 `remove_at`/`clear`，再 `continue/break`**——该实体**不会**进入本轮 `candidates`。这与 oracle `BaseObject.ViewRange.cs:95-103` 逐行一致。
3. **oracle 有 30s 刷新，Rust 没有**：oracle `BaseObject.Base.cs:223-228` 每 `30s` 调 `Envir.VerifyMapTime(CurrX, CurrY, this)`；`Envirnoment.cs:858-897` 的 `VerifyMapTime` **先正序遍历本格 `ObjList` 并把 `now - AddTime >= 60s` 的过期项删除**（`:870-880` 的 `Remove`/`Clear`），命中自己的 `CellObject` 时才 `cellInfo.Update(i, ref cellObject)` 刷新 `AddTime`（`MapCellInfo.cs:37-41`），循环结束仍 `!boVerify` 才 `AddMapObject`（`:894`）。所有 actor（玩家 `WorldServer.cs:697` `playObject.Run()`、怪/卫士 `:315` `monster.Run()`、NPC `NormNpc.Run` → `base.Run()`）都会跑到这段。
4. **Rust 明确登记“未建模 `VerifyTick`”**：`mir2-rs/crates/world/src/monai.rs` 模块头把 `ShowHp`/`VerifyTick` 列为未建模；`base_object_run_monster` 只做恢复/体力点/`TargetCret` 清理，**没有刷新格子时间**。
5. **卫士不移动 ⇒ 永远不会重新 `AddMapObject`**：`monster_ai_tick` 对 `is_guard_race`（含 race 11）只走 `base_object_run_monster`，不游走；`Entity` 也没有 `verify_tick` 字段。**同一机制不只波及卫士**：131 个商人也在首个 tick 前由 `spawn_merchants` → `attach_merchant_to_map` 落格（`add_time` 同为 0），商人（静止不动）同样不会刷新格子项；3 个固定 NPC 亦然。⇒ 静止 actor 的波及面 = 49 卫士 + 131 商人 + 3 固定 NPC（以及停走怪），且都在 `tick #300` 的首次扫描中中招。
6. **谁先删它——卫士自己的扫描，且早于玩家进图**：`World::tick` 的 AOI 刷新段对**每个非死亡实体**每 tick 都调 `search_view_range`，卫士也不例外；卫士的 `ViewRange = 5`（`MonsterObject` 构造），其扫描覆盖包括自己所在格在内的 `(x±5, y±5)`。到 `tick #300`（`clock=60000ms`）时 `60000-0 >= 60000` 首次成立，过期分支当场把卫士自己的 `CellObject` 从 `(281,661)` 格删除（同一 tick 内若有别的 actor 先扫到该格，逐出机制相同）。玩家（SessionId 2）首次进图在 `tick #1700` 与 `#1750` 之间（`进世界` 日志 22:40:48.209）；`#1750` 是首条诊断采样，首轮玩家 AOI 扫描为其后的第一个 tick（约 `#1730`），**远晚于 tick #300**。⇒ 到 `tick #2600` 时卫士已从格子表消失约 460s，玩家的 `search_view_range` 从未有机会在格子里看到它 ⇒ `vis=0`、无 `SM_TURN`；而 `nearest` 读 `world.entities`，仍报 10。
7. **为什么 `tick #2750` 又 `vis=3`**：新刷/新进视野的怪 `add_time` 新鲜，没过期 ⇒ 正常 `Show`、正常出帧。这与“静止旧实体消失、活跃新实体可见”的过期模型完全吻合。

**另外一处与 oracle 的差异**：oracle `PlayObject.SearchViewRange` 里同样的 60s 过期块是**被注释掉的**（`PlayObject.cs:3114-3123`），玩家扫描**不删格子项**；Rust 的 `search_view_range` 对玩家也执行过期。即使补了 `VerifyMapTime`，这处仍应按 oracle 分流（见 §5.3）。

**结论 3**：`nearest<=vr` 却 `vis=0` **不全是口径差**：
- 对 `tick #1900` ⇒ 是**玩家已死亡**后的正常清空（`search_view_range_death`；非缺陷）；对 `map=98` ⇒ 是**窗口外**（`nearest` 统计全部实体，含死亡/隐身/已过期；或本来就在窗口外）；
- 对 `tick #2600/#2650/#2700` ⇒ 是**真缺陷**（最近实体**推断**为卫士，距离精确匹配 10/10/5；其格子项早在 `tick #300` 已被 60s 过期逐出）。

### 4.4 诊断口径自身的问题（影响判据可信度）

1. **`af` 跨会话串账**：`aoi_frames_total` 按 `chr_name` 累计、循环外创建、从不清零。证据：SessionId 2（登录第 1 次，`entity=6788`）结束时 `af=28`；SessionId 4（登录第 3 次，`entity=8569`）登录后 +5.68s 的第一条采样就是 `af=28`；而 s2cC 抓包里本会话的怪物出现对只有 6 对（12 帧），`af` 在 `tick #2750` 恰好 `28→40`（+12）。⇒ **`af=28` 是上一会话继承的**，`check_aoi_visibility_diag.py` 的 `max af >= 2` 可被陈旧数据满足。
2. **判据③是全会话聚合**：`max vis=8` 来自 SessionId 2（登录第 1 次，`entity=6788`），与最新的 SessionId 4（`entity=8569`）无关；判据⑤对“`vis==0 && nearest<=vr`”只打印 **INFO**，不判红。⇒ 整体 `GREEN` **不能**证明“最新会话同屏可见”，本 run 的 GREEN 正是这两个口径造成的假绿。
3. `nearest` 的口径（不排除死亡/隐身/过期）会让“窗口内却不可见”既可能是缺陷、也可能是尸体/过期实体，必须再核对目标身份；诊断**不输出实体 id**，本审计只能用卫士坐标+距离精确匹配把身份缩为**推断**（见 §4.2 口径说明），结论不依赖该身份。

---

## 5. 问题 4：结论与最小修法建议（不实现）

### 5.1 a) 卫士无帧属于哪一类

**不是**“未放置”：49/49 已放置、`map_id 26`、`(281,661)`、`Dir=5`、格子可走、展示表已登记。
**不是**“未下发”：`SM_TURN`/`SM_FEATURECHANGED` 编码与下发通路对怪物实测可用（s2cC 有 6 条 `SM_TURN`、7 条 `SM_FEATURECHANGED`）。
**不是**“定点不在视距”：`(291,666)` → `(281,661)` 距离 10 ≤ `vr=12`；即使按现场 `(286,652)` 也是 9 ≤ 12。
**是第四类**：**“已放置且在视距内，但 AOI 格子项在 60s 过期后被逐出，从未进入 `VisibleActors`”**。根因是 Rust 缺 oracle `BaseObject.Run` 的 30s `VerifyMapTime` 刷新（叠加玩家侧扫描执行了 oracle 已注释的过期分支）。**波及面不止卫士**：131 个商人的 `spawn_merchants` 也在首个 tick 前 `attach_merchant_to_map`（`add_time` 同为 0），3 个固定 NPC 同样如此；凡静止/未移动且 `add_time` 停在 0 的 actor（商人、固定 NPC、停走怪）都在 `tick #300` 被同一机制逐出 ⇒ 缺陷影响面更大。

### 5.2 b) `vis=0` 采样是缺陷还是口径

- `tick #2600/#2650/#2700`：**缺陷症状**（见 §4.2/§4.3）。
- `tick #1900`：**不是缺陷**——玩家已于 22:41:15 死亡（`SM_NOWDEATH (605,588)`），`search_view_range_death` 清空 `visible_actors`，`vis=0` 属预期；该会话在 `tick #1900` 时仍在 `在世界会话 1`，22:41:23 才断开。
- `map=98` 四条：**窗口外**（`nearest > vr`）。
- 检查脚本的 `GREEN`：**判据覆盖不足导致的假绿**（全会话聚合 + `af` 串账 + 窗口内 `vis=0` 只 INFO）。

### 5.3 c) 最小修法建议

1. **补 `VerifyMapTime`（最小、必须）**
   - 在 `Entity` 增加 `verify_tick`（初始化 = 生成时刻 `clock_ms`，对应 `BaseObject.cs:444`）。
   - 在 `world` 增加 `verify_map_time(actor_id, now)`：`now - verify_tick > 30_000` 时刷新 `verify_tick`；若 `!deny_ref_status`，按 oracle `Envirnoment.cs:858-897` 的语义处理实体所在格：
     - `boVerify = false`；取 `(CurrX, CurrY)` 的 `CellInfo`（须 `cellSuccess && IsAvailable`），**正序遍历该格 `ObjList`**；
     - 若当前项 `now - AddTime >= 60_000` ⇒ `boVerify = true`、`Remove(i)`（`Count > 0` 时 `continue`；**注意 C# `for` 的 `continue` 仍执行 `i++`** ⇒ 被前移进下标 `i` 的元素**本趟被跳过**，单趟并不会删尽本格全部过期项；否则 `Clear()` 后 `break`）——**与 `PlayObject.cs:3114-3123` / `BaseObject.ViewRange.cs:95-103` 的 `while (true)`（`continue` 不递增）不同**；按 oracle 连 bug 一起搬；
     - 否则若该项 `ActorObject && CellObjId == actor_id` ⇒ `Update(i)`（`MapCellInfo.cs:37-41` 把 `AddTime` 刷成当前 tick）、`boVerify = true`、`break`；
     - 循环结束后若 `!boVerify` ⇒ `AddMapObject(CurrX, CurrY, CellType, ActorId, this)` 重新挂回。注意 oracle 是 **`!boVerify` 才补挂**，不是“找不到自己就无条件补挂”：过期分支已把 `boVerify` 置真时不会补挂。
   - 调用点：`base_object_run_monster`（覆盖普通怪/守卫/NPC 的 `base.Run()` 段）、玩家的 `BaseObject.Run` 等价段、`npc_tick`、`merchant_tick`。
2. **玩家扫描不做过期（oracle 同形）**
   - 给 `search_view_range` 增加“观察者是否玩家”的分流：玩家 viewer 跳过 `CELL_OBJECT_EXPIRE_MS` 分支（`PlayObject.cs:3114-3123` 注释块），守卫/下属/怪物 viewer 保留（`BaseObject.ViewRange.cs:95` / `GuardUnit.cs:155`）。
3. **诊断口径修正（建议同批或另开 L 级小片）**
   - `nearest` 排除 `death|ghost|invisible|fixed_hide_mode|ob_mode`，并优先只统计“仍在格子表内”的实体；或另加 `nearest_cell`/`nearest_alive` 两个读数。
   - `aoi_frames_total` 按**会话**（`actor_id` 或登录事件）清零，避免 `af` 跨会话继承。
   - `check_aoi_visibility_diag.py`：按会话分组评；把“`vis==0 && nearest<=vr && 最近实体 alive 且在格子表”升级为 RED（或至少 `--strict-window` 失败模式）。
4. **`SearchViewRange` 节拍差异（顺带登记，不属本次判据）**
   - oracle 玩家搜索 ≈1s 一次（`WorldServer.cs:666` + `PlayObject.cs:900`），Rust 每 200ms tick 扫一次；oracle 普通怪完全不扫（只有 `Guard/ArcherGuard/SlaveMonster` 扫，`WorldServer.MonGen.cs:300-311`）。这会影响出现帧的**时延/顺序**，建议单独立卡评估。

### 5.4 验收判据（修法落地后）

1. **确定性单测（无 RNG）**：构造 `World`（地图 `0` 的 700×700 或小图），静态卫士 race 11 于 `(281,661)`，玩家于 `(291,666)`；推进 `> 60s`（如 400 tick × 200ms）：
   - 卫士的 `CellObject` 仍在 `(281,661)` 格内（或玩家 `vis >= 1`）；
   - `drain_aoi_downlink`/`appear_frames` 恰好产出 1 对 `SM_TURN(recog=guard, param=281, tag=661, series=dir|light<<8)` + `SM_FEATURECHANGED(recog=guard)`；
   - RNG 前后相等（`VerifyMapTime` 不抽数）⇒ 既有 golden 逐字节不变。
   - 红检：去掉 `verify_map_time` 调用 ⇒ 60s 后格子项被删、`vis=0`、无帧 ⇒ 用例必红。
2. **真机窗口（R 级）**：同一缩窗场景重跑，日志等价采样必须 `vis >= 1` 且 `nearest <= vr`；capture 必须出现 `SM_TURN(10)` `param=281,tag=661` + 配对 `SM_FEATURECHANGED(41)`；`SM_WALK(11)` 仍应为 0（卫士未移动）。
3. **判据分级**：修法触及“出帧时序/可见集” ⇒ 按 **R**（真机窗口 A/B、四门禁全绿）；若只改诊断读数（`nearest`/`af`/脚本）⇒ **L**（等价 + 抽样冒烟）。补 `VerifyMapTime` 不改 RNG/落库/战斗数值，但会改变帧时机，仍按 R 保守处理。

---

## 6. 证据索引

### 6.1 oracle 行号（本次逐条实测）

复核口径：下表所有 oracle 行号（含区间端点）均在基线 `429c714a4` 的工作树上用 `sed -n`/`awk` 逐条实测；Rust 侧只引符号/文件路径，**不写行号**。

| 行号 | 内容 |
|---|---|
| `src/GameSrv/GameApp.cs:238` / `:239` / `:241` / `:244` | `InitializationMonsterThread()` / `if (!Config.VentureServer)` / `LocalDb.LoadGuardList()` / `WorldEngine.Initialize()` |
| `src/GameSrv/DB/LocalDB.cs:100` / `:119` / `:128` / `:130` / `:133` | `LoadGuardList` / `GetValidStrCap` / 入库门 / `RegenMonsterByName` / `guard.Dir = ...` |
| `src/GameSrv/Word/WorldServer.MonGen.cs:382` / `:819` / `:300` / `:311` / `:315` | `RegenMonsterByName` / `if (monGen.Race > 0)` / 守卫与下属才搜索 / `monster.SearchViewRange()` / `monster.Run()` |
| `src/GameSrv/Word/WorldServer.cs:373-386` / `:388-395` | 进图前地形移位环 `-3+Random(6)`（≤5 轮）/ HomeMap 回落 |
| `src/GameSrv/Word/WorldServer.cs:666` / `:697` | `playObject.SearchViewRange()` / `playObject.Run()` |
| `src/M2Server/Player/PlayObject.cs:3073` | `override SearchViewRange()` |
| `src/M2Server/Player/PlayObject.cs:3114-3123` | 60s 过期块**被注释掉**（玩家扫描不删格子项） |
| `src/M2Server/Player/PlayObject.cs:3211` / `:3216` / `:3222` / `:3240` | `Hidden` / `RM_DISAPPEAR` / `Show` / `RM_TURN(dir,x,y,0,name)` |
| `src/M2Server/Player/PlayObject.cs:889` / `:900` | `WantRefMsg = true` / `SearchTime = 1000` |
| `src/M2Server/Player/PlayObject.Message.cs:1524` / `:1542` / `:1554` / `:1558` | `case RM_TURN` / `MakeMessage(SM_TURN, ...)` / `SendSocket` / `SM_FEATURECHANGED` |
| `src/M2Server/Actor/BaseObject.ViewRange.cs:12-40` / `:95` / `:108-115` | `UpdateVisibleGay`（`Show` 在 `:36-39`）/ 60s 过期 / 可见过滤器 |
| `src/M2Server/Actor/BaseObject.Base.cs:223` / `:225` / `:226` / `:228` | 30s 门 / 刷新 `VerifyTick` / `!DenyRefStatus` / `Envir.VerifyMapTime` |
| `src/M2Server/Maps/Envirnoment.cs:858` / `:870` / `:885` / `:894` | `VerifyMapTime` / 过期项移除 / `cellInfo.Update` / 找不到自己则 `AddMapObject` |
| `src/Modules/SystemModule/Data/MapCellInfo.cs:37-41` | `Update` 把 `AddTime = GetTickCount()` |
| `src/M2Server/Monster/Monsters/GuardUnit.cs:127` / `:155` / `:170` / `:178` | 卫士自己的 `SearchViewRange` / 60s 过期 / `CanPassiveAttack` 过滤 / `UpdateVisibleGay` |

### 6.2 Rust 符号/文件（不写行号）

- `mir2-rs/crates/envir/src/guardlist.rs`：`GuardInfo`、`parse_guard_line`、`load_guard_list`。
- `mir2-rs/bins/game-svr/src/guardlist.rs`：`load_guard_list_file`、`place_guards`、`regen_monster_by_name`、`GuardPlacementReport`。
- `mir2-rs/bins/game-svr/src/main.rs`：`world_aoi_diag`、`SessionAoiDiag`、`drain_aoi_downlink`、`aoi_frames_total`、守卫装载/放置块。
- `mir2-rs/crates/world/src/aoi.rs`：`search_view_range`、`update_visible_gay`、`CELL_OBJECT_EXPIRE_MS`。
- `mir2-rs/crates/world/src/world.rs`：`World::tick`（AOI 刷新）、`spawn_monster_precomputed`、`enter_world`、`TICK_INTERVAL_MS`。
- `mir2-rs/crates/world/src/map.rs`：`add_map_object`、`add_cell_object`、`move_object`、`CellObject.add_time`。
- `mir2-rs/crates/world/src/monai.rs`：`monster_ai_tick`、`base_object_run_monster`、`is_guard_race`；模块头登记 `VerifyTick` 未建模。
- `mir2-rs/bins/game-svr/src/downlink.rs`：`appear_frames`、`turn_pair`。
- `mir2-rs/bins/game-svr/src/mongen.rs`：`WorldMonsterSink::spawn_monster`、`create_monster_oracle`。
- `mir2-rs/crates/envir/src/registry.rs`：`MapRegistry::from_entries`。

### 6.3 日志/抓包

- 日志：`E:/tmp/window/cliente2e/rust/stack/game-svr.out.log`（timestamp `22:35:00`–`22:44:12`；关键行见 §1.1）。
- 抓包：`E:/tmp/window/cliente2e/rust/s2cC/capture.jsonl`（SessionId 4，登录第 3 次，`entity=8569`，`06:43:04` 起）；对照 `s2cA`（SessionId 2，登录第 1 次，`entity=6788`）、`s2cB`（SessionId 3，登录第 2 次，`entity=8164`）。
- 判据脚本：`mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_aoi_visibility_diag.py`；实跑 `--expect visible --min-vis 1` ⇒ `OK ③ max vis=8, max af=40` / `RESULT: GREEN`。

### 6.4 关键复现命令（只读）

```sh
# 1) 放置日志
grep -n "守卫列表放置完成\|GuardList.txt 已载入" /e/tmp/window/cliente2e/rust/stack/game-svr.out.log

# 2) AOI 采样
grep -n "vr=12" /e/tmp/window/cliente2e/rust/stack/game-svr.out.log

# 3) 判据脚本
python -I mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_aoi_visibility_diag.py \
  --log /e/tmp/window/cliente2e/rust/stack/game-svr.out.log --expect visible --min-vis 1

# 4) 卫士坐标是否出现在任何出现帧（0 条即缺）
python -I - <<'PY'
import json
p=r'E:/tmp/window/cliente2e/rust/s2cC/capture.jsonl'
rows=[json.loads(l) for l in open(p,encoding='utf-8')]
hits=[r for r in rows if r.get('dir')=='s2c' and r.get('ident') in (10,11)
      and r.get('param')==281 and r.get('tag')==661]
print('guard frames', len(hits))
PY

# 5) 地图名 0 的 map_id（应 26）与 (281,661) 可走性
python -I - <<'PY'
import re,os,struct
txt=open(r'E:/MirServer/M2GameSvr/Envir/MapInfo.txt','rb').read().decode('gbk','replace')
seen=set(); ids=[]
for l in re.split(r'[\r\n]',txt):
    m=re.match(r'\[([^\]]*)\]',l)
    if not m: continue
    tok=m.group(1).split()
    if not tok: continue
    n=f=tok[0]
    if '|' in tok[0]: n,f=tok[0].split('|',1)
    if not n or n.lower() in seen: continue
    if not os.path.exists(r'E:/MirServer/M2GameSvr/Map/'+f+'.map'): continue
    seen.add(n.lower()); ids.append(n)
print('id(0)=', ids.index('0'))
d=open(r'E:/MirServer/M2GameSvr/Map/0.map','rb').read()
w,h=struct.unpack_from('<hh',d,0)
off=52+(281*h+661)*12
bk,mid,fr=struct.unpack_from('<HHH',d,off)
print('(281,661)', 'LowWall' if fr&0x8000 else ('HighWall' if bk&0x8000 else 'Walk'))
PY
```

---

## 7. 残留物清单

- 本审计只新增本文件；worktree 内无其它改动。
- 临时脚本仅落在 `/e/tmp/aoiaudit-scratch/`（`mapid.py` / `cell.py` / `cap.py` / `cap2.py` / `c2s.py` / `c2smove.py` / `stats.py`），为只读解析辅助，可随时删除。
- 未起服务、未占端口、未写库；`E:/MirServer/**` 仅只读。
- `/e/tmp/aoiaudit.patch` 为本文件的新增 diff（见交付）。
