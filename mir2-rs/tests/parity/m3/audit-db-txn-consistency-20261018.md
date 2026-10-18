# M3 存档事务边界与失败路径审计（oracle vs Rust）

- 卡：`impl-txnaudit`（只读审计；基准提交 `69892d40b`）
- 日期：2026-10-18
- 范围：oracle `Save.cs` / `WorldServer.cs` / `DataService` / `DBSrv` 存档链路 ↔ Rust
  `bins/db-srv`（`data.rs` / `db.rs` / `playdata.rs` / `cache.rs` / `handler.rs`）与
  `bins/game-svr`（`main.rs` / `load.rs` / `dbclient.rs`）。
- 纪律：oracle 行号逐条实测（`sed -n` + `cat -n` 对位）；**Rust 侧只引符号/文件路径，不写 `:行号`**。
  只读库结构，不写库、不起服务、不占端口。
- 判级：本审计触及持久化落库与存档时序，矩阵各行按 `AGENTS.md`「判据分级」一律 **R**（真机可达的场景必须真机对拍；
  纯文档/死代码项在缺口清单里单独标 L）。

---

## 0. 结论摘要

1. **Rust 主存档路径（`playdata.rs::save_record`）的事务边界与 oracle 逐条等价**：
   单事务 8 个 `Save*`、逐句吞错、只有 `SaveMagics` 的 `DELETE` 与 `Commit` 失败会滚回；
   `Commit` 失败仍回 `DBR_SAVEHUMANRCD recog=1`（oracle 忽略 `Update` 返回值）。
2. **oracle 的 `StorageContext.RollBack()` 条件写反**（`_transaction == null` 才回滚）⇒ 显式回滚**从不执行**；
   实际回滚靠 `Dispose()` 关连接由 MySQL 服务器完成。Rust 用 `tx.rollback()` / `Transaction` drop，净效果等价，
   但这是**必须登记**的 oracle 既有 bug，不能反过来“修 oracle”。
3. **缓存链路两侧同为“恒空缓存”**：`CacheStorageService.Add` 只在键已存在时替换（`CacheStorageService.cs:12-18`），
   而没有任何路径向空缓存插入 ⇒ **“缓存先更新、库写失败”的顺序问题在现网没有可观测后果**；
   顺序本身两侧一致（先 `cache.add` 后 `play_data_update`），是潜在风险而非现有缺口。
4. **Rust 独有缺陷 7 条**（关服不存档、DB 连接/事务开启错误不回包、6000 自动建角错误不回包、
   周期存档不跳过 in-save、`play_data_add` 不更新 GBK 索引、物品表读失败整表放弃、换服未移植），
   其中 **关服不存档** 与 **错误路径不回包导致 in-save 永久卡死** 为 P0。
5. **1 条“没有照抄 oracle bug”**：建角 `NewChrData` 忽略 `_playDataStorage.Add` 的返回值、恒返回 `true`
   （`UserService.cs:278-279`），Rust 改成了失败分支（软删 + 失败码/无回包）。按“行为等价优先于修 bug”应登记并回退为 oracle 口径。

---

## 1. oracle 清册（逐条实测行号）

### 1.1 事务原语 `StorageContext`（`src/Storeages/DBSvr.Storage.MySQL/DBContext.cs`）

| 原语 | 行号 | 实测语义 |
|---|---|---|
| `Open` | `21-35` | `MySqlConnection.Open()` 失败只打日志、`success=false`；连接对象仍存在但未打开 |
| `BeginTransaction` | `47-57` | 仅在 `_transaction == null && State == Open` 时开启；否则 **只打警告并继续**（后续语句走 autocommit） |
| `Commit` | `59-65` | `_transaction == null` 时静默跳过；`Commit()` 抛异常则冒泡 |
| `RollBack` | `67-73` | **条件写反**：`if (_transaction == null) { _transaction.Rollback(); }` ⇒ 有事务时 no-op，无事务时 NRE |
| `Dispose` | `75-82` | `Close()+Dispose()`；未提交事务由 MySQL 在连接断开时回滚（这是实际生效的回滚路径） |

> 结论：oracle 的“回滚”只有两条真实路径：`Dispose()` 关连接（服务器回滚）与进程崩溃（连接断开回滚）；
> `RollBack()` 本身在任何正常路径上都不产生效果。

### 1.2 `PlayDataStorage.SaveRecord` 与 8 个 `Save*`（`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs`）

| 成员 | 行号 | 事务/异常语义 | 失败后已生效的内容 |
|---|---|---|---|
| `Update` | `14-25` | 名字不在 `_NameQuickMap` 直接 false；否则 `SaveRecord` | — |
| `SaveRecord(playerId, ref)` | `45-80` | `BeginTransaction` `:57`；8 个 `Save*`；`Commit` `:66`；catch `:69-74` 设 `result=false` + `RollBack` `:72`（no-op）；finally `Dispose` `:77` | 未提交 ⇒ 关连接回滚；返回 false |
| `SaveRecord(context, playerId, hd)`（characters UPDATE） | `82-137` | `try { ExecuteNonQuery } catch { log }`（`:129-136`）**吞** | 该行未更新，事务继续 |
| `SaveAblity` | `139-171` | `catch { log }`（`:167-170`）**吞** | 该行未更新，事务继续 |
| `SaveItem` | `206-279` | 外层 `try` `:216` + `catch` `:274-278` **吞**；删除循环 `:220-233`、更新循环 `:247-262` 都在同一 try；属性清理/更新各有内层 catch `:238-242`/`:267-271` | 循环中断点之前的语句在事务内未提交；attr 失败不回滚 |
| `SaveBagItem` | `284-355` | 整体 `try` `:286` + `catch` `:351-354` **吞**（含删除/更新/attr） | 同上 |
| `SaveStorageItem` | `360-432` | 整体 `try` `:362` + `catch` `:428-431` **吞** | 同上 |
| `SaveMagics` | `434-464` | `DELETE` `:436-439` **在 try 之外 ⇒ 抛**；随后的多行 INSERT 在 try 内 `:440-463` **吞** | DELETE 抛 ⇒ 冒泡到 `SaveRecord` catch ⇒ 整个事务随关连接回滚；INSERT 失败 ⇒ 只丢该批魔法，事务继续 |
| `SaveBonusability` | `466-491` | `catch { log }` `:486-490` **吞** | 该行未更新，事务继续 |
| `SaveStatus` | `509-539` | `catch { log }` `:534-538` **吞** | 该行未更新，事务继续 |
| `UpdateItemAttr` / `ClearItemAttr` | `541-571` / `626-649` | 各自 `catch { log }` **吞** | attr 不更新，事务继续 |
| `SaveQuest` | `493-507` | `DELETE characters_quest`（`:495`）；`catch` `:503-506` **吞**；**全仓库无调用者**（死代码，见 G13） | 无（从未被调用） |
| `DeleteItemAttr` | `609-624` | `DELETE characters_item_attr`（`:614`）；`catch` `:619-623` **吞**；**全仓库无调用者**（死代码，见 G13） | 无（从未被调用） |
| `UpdateChrRecord`（`UpdateQryChar`） | `573-607` | 独立连接、**无事务**；`catch` `:596-600` 返回 false | 单条 UPDATE 的 autocommit 语义 |

要点：
- **读旧值在同一事务内**：`SaveItem/SaveBagItem/SaveStorageItem` 通过 `GetItemRecord/GetBagItemRecord/GetStorageRecord`
  （`PlayDataStorage.Query.cs:375-458`）用**同一个 `StorageContext`** 读旧行，因此看到本事务已写但未提交的状态；
  Rust `save_item_table` 的 `get_item_slots(conn, …)` 接的是同一个 `Transaction`，等价。
- **`SaveRecord` 的返回值不代表落库成功**：只有 `Open` 失败（`:50-53`，尚未进入 `try`，直接 `return false`）或 `Commit` 抛异常（`:66` → catch `:69-74`）才会 `result=false`；
  所有 `Save*` 吞掉的失败仍返回 `true`。`DataService` 又忽略该返回值（见 1.7），最终对 GameSvr 回“成功”。

### 1.3 建角事务（`PlayDataStorage.Add.cs`）

| 成员 | 行号 | 语义 |
|---|---|---|
| `Add` | `34-49` | 名字已存在即 false；`AddRecord` 成功后才更新 `_NameQuickMap` / `_IndexQuickIdMap` |
| `AddRecord` | `51-88` | `BeginTransaction` `:63` → `CreateCharacters` `:64`（返回 `playerId<=0` 则直接 `return false`，finally 关连接回滚）→ 五张附属表 → `Commit` `:76`；catch `:78-82` `RollBack` `:80`（no-op） |
| `CreateCharacters` | `90-155` | INSERT `characters`；catch `:148-154` 记日志 + `RollBack` `:150`（no-op）→ 返回 0 |
| `CreateAblity/Status/UseItem/BagItem/StorageItem` | `157-360` | 各自 `catch { log }` **吞**；附属表失败仍 `Commit` |
| `CreateItemAttr` | `362-390` | `catch { log }` **吞** |

结论：建角是“**先索引（无事务）+ 后存档（单事务）**”；存档事务只保证 `characters` 主行与已成功附属行的原子性，
**不保证** `characters_indexes` 与 `characters` 的原子性。

### 1.4 `PlayRecordStorage`（索引表；`src/Storeages/DBSvr.Storage.MySQL/PlayRecordStorage.cs`）

| 成员 | 行号 | 语义 |
|---|---|---|
| `Add` | `230-265` | 先查 `_quickList`；`INSERT characters_indexes` `:267`/`:290`（`:285` 仅 `CommandText` 赋值，`ExecuteNonQuery` 在 `:290`）；成功后更新内存 `_quickList`/`_quickIdList` |
| `UpdateRecord` | `270-321` | `boNew=false` 分支 `UPDATE characters_indexes` `:299-307`（含 `IsDeleted = HumRecord.Deleted`） |
| `DeleteRecord` | `351-380` | `UPDATE characters_indexes SET IsDeleted=2` `:353`/`:365`（软删） |
| `Update` | `382-397` | 先查 `_indexQuickList` 存在性（`:388`），再 `UpdateRecord` |

所有索引写都是**独立连接、无事务、autocommit**。`Delete(sName)`（`:323-348`）的实际顺序是：`DeleteRecord` 先写
`characters_indexes.IsDeleted=2`（`:353`/`:365`）→ 摘 `_indexQuickList`（`:336`）→ 再调 `_quickIdList.DelRecord(n14, sChrName)`（`:345`）。
但 `PlayQuickList.DelRecord`（`PlayQuickList.cs:160-181`）用 `m_List[sChrName]` 取值，而 `m_List` 的键只有 **account**
（`PlayQuickList.cs:35`/`:42`/`:55` 均为 `Add(sAccount, …)`）⇒ 角色名通常不是 key，正常情况抛 `KeyNotFoundException` 并从 `Delete()` 冒泡
（仅当角色名恰等于某账号串，或 `(m_List.Count-1) < n14` 提前返回时例外）。`_quickList` 本身始终不删 ⇒ 运行期内同名不可重建。

### 1.5 GameSvr 存档触发点

| 触发 | 行号 | 语义 |
|---|---|---|
| 单角色周期存档 | `WorldServer.cs:698-703` | `!FrontEngine.IsFull()`（恒 false，`FrnEngn.cs:120-136`）且 `now-SaveRcdTick > SaveHumanRcdTime` ⇒ **先**重置 tick `:700` → `DealCancelA()` `:701` → `SaveHumanRcd()` `:702` |
| **全量周期存档** | `GameSrv/TimedService.cs:90-94` → `TimingSaveData` `:222-243` | 每 10 分钟（`60*10000` ms）遍历所有 `IPlayerActor`；`:234-237` **跳过 `FrontEngine.InSaveRcdList(play.ChrName)`**；`:238` `SaveHumanRcd` |
| 默认周期值 | `GameSvrConf.cs:423` / `:1484`、`ServerConf.cs:566` | `SaveHumanRcdTime = 10*60*1000` ms（可由 `[Setup]` 覆盖） |
| 小退/软关 | `PlayObject.Message.cs:367-390` | `BoSoftClose/BoKickFlag/BoEmergencyClose` ⇒ `MakeGhost()` `:375`（异常支 `:396` 也 `MakeGhost`） |
| 断线 | `M2Server/Net/TCP/TCPNetChannel.cs:268-269`、`:340`；`PlayObject.Message.cs:50` | 置 `BoEmergencyClose`（部分同时 `BoSoftClose`）⇒ 下一轮 `MakeGhost()` |
| 踢旧/顶号 | `WorldServer.cs:958-975`（`KickPlayObjectEx` 置 `BoEmergencyClose`）、`:1052-1062`（`KickOnlineUser` 置 `BoKickFlag`）、`:200-215`（`ProcessHumansIsLogined` 含 `InSaveRcdList` `:203`） | 下一轮 `RunNotice` 落地 `MakeGhost` |
| Ghost 落地存档 | `WorldServer.cs:707-717` | 从 `PlayObjectList` 摘除 → `Disappear()` → `AddToHumanFreeList` → `DealCancelA()` `:712` → `SaveHumanRcd()` `:713` → `CloseUser` |
| **死亡** | `M2Server/Player/PlayObject.cs:1964-1997` | `Die()` **不调用** `SaveHumanRcd`；`BaseObject.Base.cs:484-496` 在 `CanReAlive` 时也不置 `Ghost` ⇒ **死亡不是存档触发点** |
| 换服 | `WorldServer.cs:592-610` | 仅 `SwitchData && playObject.RcdSaved` 才 `SendSwitchData`；`RcdSaved` 由 `CharacterDataProcessor.cs:85-88` 在存档请求发出后置 true |
| 服务器关闭 | `GameSrv/AppService.cs:206-210` → `SavePlayer` `:133-145` | `OnShutdown` ⇒ 遍历所有在线角色 `SaveHumanRcd`（`:141`） |

`SaveHumanRcd`（`WorldServer.cs:1077-1091`）：`IsRobot` 早返回 `:1079-1082`；构 `SavePlayerRcd` `:1083-1089`；
`MakeSaveRcd` **立即**取快照 `:1090`；入 `m_SaveRcdList` `:1091`。

### 1.6 存档队列与应答（GameSvr）

| 成员 | 行号 | 语义 |
|---|---|---|
| `FrontEngine.AddToSaveRcdList` | `FrnEngn.cs:195-206` | 加锁追加 `m_SaveRcdList` |
| `FrontEngine.InSaveRcdList` | `:156-176` | 按 `ChrName` **OrdinalIgnoreCase** 线性查 |
| `FrontEngine.RemoveSaveList` | `:56-74` | 按 `QueryId` 删除 |
| `FrontEngine.IsFull` | `:120-136` | `Count >= int.MaxValue` ⇒ 恒 false |
| `CharacterDataProcessor.ExecuteInternal` | `CharacterDataProcessor.cs:30-62` | 每 500ms；`:37-48` **DBSvr 断链时直接 `ClearSaveList`（丢档）** |
| `ProcessSaveStorage` | `:64-93` | 拷 `m_SaveRcdTempList`；`:74-78` `IsSaveing` 跳过并置 true；`:79-89` 发送；**成功后置 `RcdSaved=true`**（`:87`）；`:91` 清 temp；`:92` `ProcessSaveQueue` |
| `PlayerDataService.SaveRcd` | `PlayerDataService.cs:100-112` | `SendRequest` 失败返回 false；成功入 `SaveProcessList` |
| `ProcessSaveQueue` | `:114-134` | 仅 `DBR_SAVEHUMANRCD && recog==1` 才 `RemoveSaveList`（`:124-129`）；其他应答只出队 |

**两条 oracle 既有 bug（必须照抄）**：
1. `IsSaveing` 一旦置 true **永不复位**（`:78`，全仓库只有 `:74/:78` 两处读写）⇒ 发送失败/超时的记录永久卡在保存名单；
2. 失败应答是 `DBR_LOADHUMANRCD(1100) recog=0`（`DataService.cs:248`），`ProcessSaveQueue` 只出队、不 `RemoveSaveList`
   ⇒ 角色**永久 in-save**，之后每次进世界都被 `ProcessHumansIsLogined` 判“正在保存”而踢新会话（`:200-215`）。

### 1.7 DBSrv `DataService`：缓存与库的更新时序

| 步骤 | 行号 | 语义 |
|---|---|---|
| `LoadHumanRcd` 取缓存 | `DataService.cs:151-205` | `:174` 先 `_cacheStorage.Get`；未命中 `:177` 读库 |
| `SaveHumanRcd` 入口 | `:207-257` | 反序列化失败 `:212-216` 直接返回（不回包） |
| 名字索引 | `:225-230` | `Index<0` 时 `Add(humanRcd)` `:228`（**忽略返回值**）→ 再 `Index` `:229` |
| **缓存先写** | `:233` | `_cacheStorage.Add(sChrName, humanRcd);` |
| **库后写** | `:234` | `_playDataStorage.Update(sChrName, humanRcd);`（**忽略 bool**） |
| 会话标记 | `:237` | `SetSessionSaveRcd(sUserId)` |
| 成功应答 | `:242` | 只要 `nIndex >= 0`（`bo21=false`）就回 `DBR_SAVEHUMANRCD recog=1`，**与库写是否成功无关** |
| 失败应答 | `:248` | `DBR_LOADHUMANRCD recog=0` |
| 兜底 | `:253-256` | 整个函数 `catch` 只记日志，**不回包** |
| `SaveHumanRcdEx` | `:259-268` | 反序列化后直接复用 `SaveHumanRcd` |

`CacheStorageService`（`src/Storeages/DBSrv.Storage/Impl/CacheStorageService.cs`）：
- `Add` `:12-18`：**仅当 `_cacheMap.ContainsKey` 时替换**，否则 no-op；
- `Get` `:20-29`；`Delete` `:31-37`；`QueryCacheData` `:39-42`。
- 全仓库只有 `DataService.cs:233` 一处 `Add` ⇒ 缓存从空开始永远为空。

### 1.8 DBSrv 缓存回写

`DBSrv/TimedService.cs`：`:55-59` 每 300000ms（5 分钟）→ `ProcessCacheStorage` `:73-96`；
`:77` 枚举 `QueryCacheData`；`:85` `_playDataStorage.Update`；**无论成功失败** `:93` `_cacheStorage.Delete`。

### 1.9 账号侧（参考，非本次主链）

`src/LoginSrv/Storage/AccountStorage.cs`：`CreateAccount` `:263-324` 是本仓库**唯一写法正确的显式事务**——
`:272` `BeginTransaction`、`:311` `Commit`、`:315` `Rollback`（`catch` 内真的调用）。

---

## 2. Rust 清册（只引符号/文件路径）

### 2.1 `bins/db-srv`

| 位置 | 符号 | 事务/失败语义 |
|---|---|---|
| `src/playdata.rs` | `save_record` | `conn.begin()` → 逐项 `save_characters` / `save_ablity` / `save_item_table`×3 / `save_magics` / `save_bonusability` / `save_status`；**逐项 `if let Err` 记日志并继续**；`save_magics` 返回 Err 时 `tx.rollback()` 并 `Ok(false)`；`tx.commit()` 失败记日志返回 `Ok(false)` |
| 同上 | `save_characters` / `save_ablity` / `save_bonusability` / `save_status` | 单条 UPDATE，失败以 Err 上抛给 `save_record`（由调用方吞） |
| 同上 | `save_item_table` | `get_item_slots`（同一 `Transaction`）→ `comparer_user_item` → 删除循环 → `clear_item_attr`（吞）→ 更新循环 → `update_item_attr`（吞）；删除/更新循环的 SQL 失败以 `?` 上抛 |
| 同上 | `save_magics` | `DELETE` 失败上抛（触发 rollback）；逐条 INSERT 失败只记日志 |
| 同上 | `Db::play_data_update` / `Db::play_data_add` / `play_data_get` / `play_data_index_text` | `play_data_update` 命中索引才调 `save_record`；`play_data_add` 单事务写主表+附属表并更新 `name_text`/`char_ids` |
| `src/db.rs` | `Db::data_add` / `create_character` | 建角单事务；附属表失败只 `warn` 并继续；`tx.commit()` 后更新 `name_quick`/`name_text`/`char_ids` |
| 同上 | `record_add` / `record_update` / `record_soft_delete` / `record_update_by` / `write_index_row` / `load_indexes` / `load_chars` | 索引表全部**无事务、autocommit**；`record_soft_delete` 只写 `IsDeleted=2` 并摘 `index_ids` |
| `src/data.rs` | `save_human_rcd` | 先 `cache.add` 再 `play_data_update`；**忽略** update 的 `Ok(false)`；`play_data_add`/`play_data_update` 的 `Err` 用 `?` 上抛；`n_index` 仍缺 ⇒ 回 `DBR_LOADHUMANRCD recog=0`，否则回 `DBR_SAVEHUMANRCD recog=1` |
| 同上 | `load_human_rcd` | 先 `cache.get`，未命中 `play_data_get` |
| 同上 | `cache_flush_task` / `process_cache_storage` | 5 分钟一次；`play_data_update` 后**无条件** `cache.delete` |
| 同上 | `conn_task` / `process` / `process_server_msg` | 处理返回 Err 时只 `log::error`，**不回包、连接继续** |
| `src/cache.rs` | `CacheStore::add/get/delete/query_all` | `add` 仅键已存在时替换（与 C# 同，单测 `add_of_missing_key_is_noop` 钉住） |
| `src/handler.rs` | `new_chr` / `delete_chr` | `record_add`（无事务）→ `data_add`（事务）；`data_add` 返回 `None` 时 `record_soft_delete` + code 4；`data_add` 返回 Err 则 `?` 上抛（不回包）；`delete_chr` 只 `record_update` 写 `IsDeleted=1` |

### 2.2 `bins/game-svr`

| 位置 | 符号 | 语义 |
|---|---|---|
| `src/main.rs` | `save_human_rcd` | 仅在 `DbLinkState::is_connected()` 时发；构包 → `req_tx.send` → 成功才 `engine.begin_save` + `save_outstanding.push`；失败只记日志、**不登记** |
| 同上 | `apply_save_reply` | 仅 `DBR_SAVEHUMANRCD && recog==1` 才 `end_save`（失败码保持 in-save，照抄 oracle） |
| 同上 | `drain_periodic_save` / `periodic_save_due` | 逐在线会话按名字排序；`now - save_rcd_tick > save_human_rcd_time` **严格大于**，先重置再发；**不检查 `in_save_rcd_list`** |
| 同上 | `drain_kicks` / `drain_soft_close` / 断线事件路径 | 先 `deal_cancel_between`（等价 `DealCancelA`）→ `save_human_rcd` → `world.disconnect` |
| 同上 | `DbWiring.save_outstanding` | 存档请求 `(query_id, chr_name)` 内存队列；取走时 `apply_save_reply` |
| 同上 | `SessionGround.save_rcd_tick` | 进世界时 `now_ms()` 起算（等价 `PlayObject.cs:886`） |
| 同上 | `DbLinkState`（`src/dbclient.rs`） | TCP 连上置位；`save_human_rcd` 未连接直接跳过 |
| `src/load.rs` | `FrontEngine::begin_save` / `end_save` / `in_save_rcd_list` / `is_full` / `delete_human` | 只存名字；`is_full` 恒 false（照抄） |
| `src/dbclient.rs` | `save_human_rcd_request` | `DB_SAVEHUMANRCD(101)` 信封 |
| `src/config.rs` | `save_human_rcd_time` | `[Setup] SaveHumanRcdTime`，缺省 `10*60*1000` ms |

### 2.3 未覆盖/未移植（与本审计相关）

- `bins/game-svr` **没有关服存档**（无 `SavePlayer`/`OnShutdown` 等价物）。
- **没有换服**（`SwitchData`/`RcdSaved`/`SendSwitchData` 全缺）。
- `crates/storage` 的 `PlayDataStorage::update_query_char` 只有 trait 占位；oracle 该函数（`PlayDataStorage.Save.cs:27-38`、`:573-607`）**全仓库无调用者**，属死代码。

---

## 3. 矩阵（22 行）

> 列说明：**差异** 指“Rust 与 oracle 的可观测差异”；“无”＝按现有代码逐条等价（含照抄 bug）。
> `tier` 按 `AGENTS.md` 判据分级；存档全部触持久化 ⇒ 除注明 L 的文档项外均为 **R**。

| # | 场景 | oracle 事务与失败语义 | Rust 现状 | 差异 | 可观测后果 | tier | 判据 |
|---|---|---|---|---|---|---|---|
| M1 | 单角色周期存档（10min） | `WorldServer.cs:698-703`；`!IsFull()`（恒 false）；先重置 tick 再 `DealCancelA`+`SaveHumanRcd`；不查 in-save | `main.rs::drain_periodic_save`/`periodic_save_due`/`save_human_rcd`；`SessionGround.save_rcd_tick` | 无（快照时点见 M19） | 无 | R | 会话级把 `SaveHumanRcdTime` 调到秒级，抓 `DB_SAVEHUMANRCD` 帧并比对落库值 |
| M2 | 全量周期存档（TimedService 10min） | `TimedService.cs:90-94`→`:222-243`；`:234-237` 跳过 `InSaveRcdList` | 无全量任务，只有 M1 的逐会话 tick | Rust 不跳过 in-save（G5） | in-save 卡死时重复发同角色存档 | R | 构造 in-save 记录，观察下一轮是否再发存档帧 |
| M3 | 小退/软关（`BoSoftClose`） | `PlayObject.Message.cs:367-390`→`MakeGhost`；`WorldServer.cs:707-717`，`:712` `DealCancelA`，`:713` `SaveHumanRcd`，`CloseUser` | `drain_soft_close`：先 `deal_cancel_between` 后 `save_human_rcd` 再 `world.disconnect` | 无（落地早约 1 tick，已登记） | 无 | R | 软关后抓存档帧 + 落库值 |
| M4 | 断线/异常掉线 | `TCPNetChannel.cs:268-269`/`:340` 置 `BoEmergencyClose`→`MakeGhost`→ghost 支存档 | 网关关闭事件：`deal_cancel_between`→`save_human_rcd`→`disconnect` | 无 | 无 | R | 拔线/关网关，抓存档帧与落库值 |
| M5 | 踢下线/顶号 | `WorldServer.cs:958-975`/`:1052-1062` 置标志；`:200-215` 用 `InSaveRcdList` `:203` 决定踢新 | `drain_kicks` + `in_save_rcd_list` 守卫 | 无（失败应答永久 in-save 照抄） | 无 | R | 同角色二次登录，验证旧会话存档与新会话被踢顺序 |
| M6 | 死亡 | `PlayObject.cs:1964-1997` `Die()` **不存档**；`CanReAlive` 时不置 `Ghost`（`BaseObject.Base.cs:484-496`） | 死亡事件不调 `save_human_rcd` | 无 | 死亡本身不产生存档；掉落/经验靠下一次触发落库 | R | 击杀后立即断线，落库应为死亡时实时态（由断线触发） |
| M7 | 换服（`SwitchData`） | `WorldServer.cs:592-610`：仅 `RcdSaved` 才 `SendSwitchData`；`RcdSaved` 在 `CharacterDataProcessor.cs:87` 置 | 未移植换服 | **功能缺口（G11）** | 多机部署时换服无存档门（单区无影响） | R | 单区：无路径；多机：换服前后落库一致性 |
| M8 | 服务器关闭 | `AppService.cs:206-210` `OnShutdown`→`SavePlayer` `:133-145`→逐角色 `SaveHumanRcd`（`:141`） | **无关服存档** | **Rust 独有缺陷（G1）** | 优雅停服回档到上次周期存档（最多 10 分钟进度） | R | 停服前改 Gold/背包，停服重启后读库比对 |
| M9 | 建角（`CM_NEWCHR`） | `UserService.cs:565-662`：`PlayRecordStorage.Add` `:635`（无事务）→`NewChrData` `:259-280`（`Add` 返回值被忽略、恒 true `:278-279`）→失败才 `Delete` 软删 `:645`；`NewChrData` 自己几乎不返回 false | `handler.rs::new_chr`：`record_add`→`data_add`（事务）→`None` 才 `record_soft_delete` + code 4；Err 则无回包 | **未照抄 oracle bug（G4）** | oracle：库写失败仍回 `SM_NEWCHR_SUCCESS` + 孤儿索引；Rust：失败码/无回包 | R | 注入 `characters` INSERT 失败，比对回包码与 `characters_indexes` 残留 |
| M10 | 删除角色（`CM_DELCHR`） | `UserService.cs:528-560`→`:545` `PlayRecordStorage.Update`→`:270-321` 写 `IsDeleted=1`；不删 `characters`、不清缓存、无事务 | `handler.rs::delete_chr`→`record_update`→`write_index_row` 写 `IsDeleted=1` | 无（含“只软删索引”的怪行为） | 角色数据保留、缓存不动 | R | 删角后查三张表与缓存 |
| M11 | 存档时自动建角（6000 索引缺失） | `DataService.cs:225-230`：`Add`（忽略结果）→再 `Index`；`:233` 缓存→`:234` `Update`（忽略结果）→`:242` 回成功 | `data.rs::save_human_rcd`：`play_data_add`→`play_data_update`（忽略 `Ok(false)`）→回成功 | Err 路径不回包（G3）；`play_data_add` 不更新 `name_quick`（G6） | 卡 in-save；新角色对 5100 面不可见直到 DBSrv 重启 | R | 构造 `characters_indexes` 有行、`characters` 无行的孤儿，发存档帧 |
| M12 | 缓存回写（DBSrv 5min） | `DBSrv/TimedService.cs:55-59`→`:73-96`；`:85` `Update`；`:93` 无条件 `Delete` | `data.rs::cache_flush_task`/`process_cache_storage`；`cache.rs::CacheStore` | 无（缓存恒空） | 无 | R | 单测已钉 `add` 对缺键 no-op；进程级观察两条日志 |
| M13 | 单个 `Save*` 语句失败 | 7 个 `Save*` 吞错；只有 `SaveMagics` DELETE `:436-439` 抛（回滚），其 INSERT 吞 | `save_record` 逐项 `if let Err` 记日志；`save_magics` DELETE Err 显式 rollback | 无（净效果等价） | 单表回档/该表不更新，其余表照常提交 | R | 逐表注入失败（唯一索引/权限/超长），比对最终表内容 |
| M14 | `Commit` 失败 | `PlayDataStorage.Save.cs:66` 抛→`:69-74` `result=false` + `RollBack` `:72`（no-op）→`Dispose` 关连接回滚；`DataService` 忽略 false 回成功 | `save_record` commit Err→`Ok(false)`；事务 drop 回滚；`save_human_rcd` 忽略 false 回成功 | 无 | 整次存档静默丢失、GameSvr 认为成功 | R | 提交阶段杀连接（或只读事务），比对落库与回包 |
| M15 | DBSrv/MySQL 不可用（Open/acquire 失败） | `StorageContext.Open` `:21-35` 失败→`SaveRecord` false→`DataService` 忽略→**回成功**（静默丢档） | `pool.acquire()`/`begin()` Err→`?`→**不回包** | **G2（Rust 独有）** | oracle 玩家可继续玩（丢一次存档）；Rust 角色永久 in-save，重登被踢 | R | 停 MySQL/断 DBSrv 链路，发存档帧后观察回包与重登 |
| M16 | GameSvr→DBSvr 发送失败 | `PlayerDataService.cs:100-112` false；记录留队且 `IsSaveing=true` 永不复位；DB 断链时 `CharacterDataProcessor.cs:37-48` 直接清空保存队列 | `save_human_rcd` send Err 不 `begin_save`、不记 outstanding；未连接直接跳过 | 重试语义不同 | oracle 卡死或清空（丢档）；Rust 下一周期自然重试 | R | 拔 DBSvr 链路，观察后续周期是否重发 |
| M17 | 失败/丢失应答 | 成功 `1102 recog=1` 才 `RemoveSaveList`（`PlayerDataService.cs:124-129`）；失败 `1100 recog=0` 只出队、保持 in-save | `apply_save_reply` 同口径；`save_outstanding` 无应答则永久保留 | 无（bug 照抄） | 角色永久 in-save，重登被“正在保存”踢 | R | 让 DBSrv 回失败码/吞应答，验证重登被踢 |
| M18 | GameSvr 崩溃 | 内存 `m_SaveRcdList` 丢失；未发/在途存档丢失；以最后一次成功提交为界 | `save_list`/`save_outstanding` 为内存态，进程崩溃同 | 无 | 崩溃前未落库的进度回档 | R | `kill -9` 后重启读库比对 |
| M19 | 存档体快照时点 | `SaveHumanRcd` 立即 `MakeSaveRcd`（`:1090`），但发送在 500ms 线程（`CharacterDataProcessor.cs:64-93`） | 触发即构包即发 | oracle 快照“更旧”≤500ms | 断开前 500ms 内的状态变化可能差一帧 | R | 同 tick 内改 Gold+存档，逐字节比存档体 |
| M20 | DBSrv 崩溃/MySQL 连接中断（写库中途） | 事务未提交→服务器回滚；重启 `LoadQuickList` 重建索引；未应答存档在 GameSvr 侧卡 in-save | sqlx 连接断开→服务器回滚；重启重建索引 | 无 | 整次存档回滚（无部分行） | R | 事务中途 `KILL` 连接，重启后全表比对 |
| M21 | 并发多 GameSvr 存同角色 | TouchSocket 回调并发；`PlayDataStorage` 无锁（`Dictionary` 非线程安全）；最后提交者赢 | `Arc<Mutex<Db>>` 串行，到达序即提交序 | 并发序不同（oracle 本身非确定） | 多 GameSvr 同角色时落库字段可能来自不同次序；单 GameSvr 无差异 | R | 双 GameSvr 并发写同角色，比对最终行与两侧日志时序 |
| M22 | 账号建号/更新（参考） | `AccountStorage.cs:263-324`：`:272` begin、`:311` commit、`:315` rollback（真事务） | `crates/storage::account::add`/`update_account`：真事务 + 显式 rollback | 无 | 无 | R | 注入 `account_protection` INSERT 失败，验证 `account` 行不残留 |

---

## 4. 缺口清单（按「真机可达 × 影响」排序）

### G1（P0，Rust 独有缺陷，R）关服不存档
- oracle：`AppService.cs:206-210` → `SavePlayer` `:133-145` → 每个在线角色 `SaveHumanRcd`（`:141`）。
- Rust：`bins/game-svr` 无任何关服/退出存档路径。
- 可达性：**每次优雅停服/重启**。
- 影响：自上次周期存档（缺省 10 分钟）以来所有 Gold/经验/背包/坐标变化回档；在途交易/商店状态也丢。
- 修法方向：在进程退出信号处按 `drain_periodic_save` 同口径对全部在线会话 `save_human_rcd`，并等待应答或超时。
- 判据：停服前改 Gold/背包 → 停服 → 重启 → 读库逐字段比对。

### G2（P0，Rust 独有缺陷，R）DB 连接/事务开启错误不回包，in-save 永久卡死
- oracle：`StorageContext.Open` 失败（`DBContext.cs:21-35`）在内层被 `SaveRecord` 吞成 `false`，`DataService.cs:234` 忽略它，
  `:242` 仍回 `DBR_SAVEHUMANRCD recog=1` ⇒ 玩家侧“成功”，可继续游戏/重登。
- Rust：`data.rs::save_human_rcd` 里 `db.play_data_update(...).await?` 的 `Err`（`pool.acquire()`/`begin()` 失败）
  上抛到 `conn_task`，只记日志、**不回包**；`save_outstanding` 与 `engine.begin_save` 都不清理。
- 可达性：MySQL 瞬断、连接池耗尽、死锁超时、事务开启失败。
- 影响：**存档请求永不完成**；角色永久留在 in-save，之后每次进世界都被当“正在保存”踢新会话（比 oracle 更严重：
  oracle 至少能重登）。同时这次存档静默丢失。
- 判据：`iptables`/停 MySQL 制造 acquire 失败，发 `DB_SAVEHUMANRCD`，断言 Rust 无回包 + 重登被踢；oracle 回 1102 成功。

### G3（P1，Rust 独有缺陷，R）6000 自动建角错误不回包
- oracle：`DataService.cs:228` `Add` 忽略结果；`:229` 再 `Index`；失败则 `bo21` 保持 true ⇒ `:248` 回
  `DBR_LOADHUMANRCD recog=0`（请求结束）。
- Rust：`db.play_data_add(&rcd).await?` 的 `Err`（如 `create_character` SQL 失败）上抛 ⇒ 不回包。
- 可达性：低（正常流程角色已存在；只在索引有行、`characters` 缺行的孤儿数据上触发）。
- 影响：同 G2 的 in-save 卡死 + 无应答。
- 判据：构造孤儿索引（`characters_indexes.IsDeleted=0` 且 `characters` 无对应行），发存档帧。

### G4（P1，未照抄 oracle bug，R）建角失败语义
- oracle：`UserService.cs:278-279` `_playDataStorage.Add(chrRecord); return true;` —— **忽略 Add 返回值**。
  因此建角写库失败时：
  1. 仍回 `SM_NEWCHR_SUCCESS`（`:650-653`）；
  2. 不执行 `:645` 的软删 ⇒ `characters_indexes` 留下 `IsDeleted=0` 的孤儿；
  3. 该名字之后 `NewChr` 会因 `_playRecordStorage.Add` 命中 `_quickList` 返回 false（`PlayRecordStorage.cs:233-239`）而报 code 2，
     直到 DBSrv 重启（`_quickList` 不删名）。
- **追加：`NewChrData` 返回 false 的稀有支**（`UserService.cs:261-265`，仅 `Index(sChrName) != -1` 时）：`:645` 调
  `_playRecordStorage.Delete(sChrName)`；`DeleteRecord` 先把 `characters_indexes.IsDeleted=2`（§1.4），随后 `:345` 的
  `_quickIdList.DelRecord` 因键口径错误抛 `KeyNotFoundException`，异常从 `NewChr`（调用点无 catch）冒泡 ⇒ **不回包**；
  索引此时已被软删，不留下 `IsDeleted=0` 孤儿。
- Rust：`handler.rs::new_chr` 在 `data_add` 返回 `None` 时 `record_soft_delete` + code 4；`Err` 时直接无回包；
  且 `data_add` 的 `Err` 不软删 ⇒ 也留孤儿，但客户端可见行为不同。
- 可达性：DB 写失败/磁盘满/唯一键冲突时的建角。
- 影响：客户端提示不同（成功 vs 失败/超时）；孤儿索引的善后路径不同。
- 处置：按“行为等价优先于修 bug”改回 oracle 口径并登记 `T-*`；或由 owner 显式裁决保留“修复”并登记 `D-*`。

### G5（P2，Rust 独有缺陷，R）周期存档不跳过 in-save
- oracle：`TimedService.cs:234-237` 对已在 `m_SaveRcdList` 的角色 `continue`。
- Rust：`drain_periodic_save` 只按 `save_rcd_tick` 判定，不查 `in_save_rcd_list`。
- 可达性：某次存档因 G2/G17 卡死超过 10 分钟。
- 影响：同一角色多份在途存档；落库次序取决于 DBSvr 处理序（`Mutex<Db>` 下为到达序），可能出现“旧快照后提交覆盖新快照”。
- 判据：构造 in-save 后再跑一个周期，抓 `DB_SAVEHUMANRCD` 帧数量。

### G6（P2，Rust 独有缺陷，R）`play_data_add` 不更新 GBK 索引 `name_quick`
- oracle：`PlayDataStorage` 只有一份 `_NameQuickMap`（`PlayDataStorage.Query.cs:16-23`），`Add` 成功后
  `_NameQuickMap.Add` / `_IndexQuickIdMap.Add`（`PlayDataStorage.Add.cs:42-47`）⇒ 6000 面新建的角色
  立刻对 5100 面的 `Index/Query/Select` 可见。
- Rust：`bins/db-srv/src/playdata.rs::Db::play_data_add` 只写 `name_text` / `char_ids`，**不写** `name_quick`；
  而 `bins/db-srv/src/db.rs::data_index`（5100 面）只读 `name_quick`。
- 可达性：很低（只在 `save_human_rcd` 发现 `name_text` 缺失时触发）。
- 影响：DBSrv 重启前，`QueryChr` 里 `data_index=None` ⇒ 角色从列表消失（`handler.rs::query_chr` 的 `continue`）；
  `CM_NEWCHR` 的同名判重也看不到它。
- 判据：构造 `characters` 有行但 `name_text` 未载入的场景（或直接调用 `play_data_add` 单测），随后发 `CM_QUERYCHR`。

### G7（P3，Rust 独有缺陷，R）物品表读失败整表放弃
- oracle：`GetItemRecord/GetBagItemRecord/GetStorageRecord`（`PlayDataStorage.Query.cs:375-458`）`catch` 吞错，
  `oldItems` 部分填充或全空，随后仍进入 `ComparerUserItem` + 后续写循环。
- Rust：`get_item_slots` 的 `fetch_all(...).await?` 失败 ⇒ `save_item_table` 返回 Err ⇒ `save_record` 记日志后
  **整张表**不再比较/写入。
- 可达性：低（读表 SQL/网络错误）。
- 影响：oracle 可能清/改部分槽位（基于残缺 oldItems），Rust 完全不写该表。极端 DB 错误下存档体不同。

### G8（P3，登记项，L）oracle `RollBack()` 条件写反
- `DBContext.cs:67-73` `if (_transaction == null) { _transaction.Rollback(); }` ⇒ 显式回滚从不执行。
- 实际回滚路径：`Dispose()` 关连接（`:75-82`）→ MySQL 回滚；进程崩溃 → 连接断 → 回滚。
- Rust：`save_magics` 失败显式 `tx.rollback()`，其余靠 `Transaction` drop。
- **净行为等价**；本条只登记，防止后续有人把 oracle 的 `RollBack` 当“有回滚”去改 Rust。

### G9（P3，时序已知，R）存档体快照时点相差 ≤500ms
- oracle：`MakeSaveRcd` 在 `SaveHumanRcd` 调用时立即执行（`WorldServer.cs:1090`），但发送在 500ms 定时线程。
- Rust：触发即构包即发。
- 影响：同一触发点下 oracle 的更“旧”不超过 500ms；逐字节对拍时需固定时钟/把 `CharacterDataProcessor` 周期视为 0。
- 处置：对拍夹具把两侧触发点对齐到同一 tick；不作为缺陷。

### G10（P3，非契约，R）并发序
- oracle：TouchSocket 回调并发，`PlayDataStorage` 无锁；`_NameQuickMap` 是普通 `Dictionary`。
- Rust：`Arc<Mutex<Db>>` 串行化所有 5100/6000 面 DB 操作。
- 影响：多 GameSvr 并发同角色时提交序不同。单区单 GameSvr 下无差异；oracle 本身不提供顺序契约。

### G11（P2，功能缺口，R）换服存档门未移植
- oracle：`WorldServer.cs:592-610` 的 `SwitchData && RcdSaved` 门；`RcdSaved` 在存档请求发出后置
  （`CharacterDataProcessor.cs:85-88`）。
- Rust：无 `RcdSaved`/`SendSwitchData`/换服逻辑。
- 影响：单区（M2 oracle 的常态）无路径；多机部署时换服不等存档完成，可能在新服读到旧档。

### G12（P4，死代码，L）`UpdateQryChar` 占位
- oracle 的 `PlayDataStorage.UpdateQryChar`（`PlayDataStorage.Save.cs:27-38`、`:573-607`）全仓库**无调用者**。
- Rust 只有 `crates/storage::traits::PlayDataStorage::update_query_char` 占位。
- 无行为差异；登记为死代码，避免后续误接。

### G13（P4，死代码，L）`SaveQuest` / `DeleteItemAttr` 无调用者
- oracle `PlayDataStorage.SaveQuest`（`PlayDataStorage.Save.cs:493-507`，只 `DELETE characters_quest` + `catch` 吞）与
  `DeleteItemAttr`（`:609-624`，只 `DELETE characters_item_attr` + `catch` 吞）**全仓库无任何调用者**（C# 侧 grep 仅命中定义自身；
  二者都不在 `SaveRecord` 的 8 个 `Save*` 调用清单里）。
- 对存档主链无行为差异；与 G12 的 `UpdateQryChar` 同口径登记为 **L 级死代码**，避免后续误接或误判为“已覆盖”。
- Rust 侧无对应实现；若将来移植，须先确认是否为 oracle 漏接，不得单方面接入主链。

---

## 5. 崩溃一致性

| 崩溃点 | oracle | Rust | 差异 |
|---|---|---|---|
| 事务中途（任何 `Save*` 已执行、未 `COMMIT`） | 连接断开 ⇒ MySQL 回滚整事务；无部分行 | 同（连接断或 `Transaction` drop） | 无 |
| `COMMIT` 已发出、结果未知（in-doubt） | 由 MySQL 决定；客户端断开不改变已提交 | 同 | 无 |
| `COMMIT` 成功后、应答前崩溃 | 数据已落库；GameSvr 收不到应答 ⇒ 记录留在 `m_SaveRcdList`（`IsSaveing=true`，永不重试） | 数据已落库；`save_outstanding` 无应答 ⇒ 永久 in-save | 无（都卡 in-save） |
| GameSvr 崩溃 | 内存 `m_SaveRcdList`/`SaveRcdTick` 丢失；以最后成功提交为界 | `save_list`/`save_outstanding`/`save_rcd_tick` 丢失；同上 | 无 |
| DBSrv 崩溃 | 在途事务回滚；重启 `LoadQuickList` 重建索引；GameSvr 侧卡 in-save | 同；重启 `load_quick_lists` 重建索引 | 无 |
| MySQL 崩溃 | 未提交回滚；已提交按 redo 恢复 | 同 | 无 |
| **建角：索引已写、存档事务未完成** | `characters_indexes` 孤儿（`IsDeleted=0`），`characters` 无行；`QueryChr` 隐藏；同名不可重建（`_quickList` 不删） | 同（`record_add` 在事务外）；`data_add` Err 时 Rust 不会软删 | 两者都留孤儿；Rust 少了 oracle“软删”路径（见 G4） |
| **缓存有数据时崩溃** | 现网不可能（缓存恒空）；若将来修 `Add` 为插入，则最多丢 5 分钟缓存且缓存可能比库新 | 同（`CacheStore` 照抄 no-op） | 无；见 W1 |

---

## 6. 必须照抄 vs Rust 独有缺陷

### 6.1 必须照抄（oracle 既有行为，括注证据）

1. **7 个 `Save*` 吞错、`SaveRecord` 仍返回 true**（`PlayDataStorage.Save.cs` 各 catch）——Rust 已照抄。
2. **`DataService` 忽略 `Update` 返回值，库写失败仍回 `DBR_SAVEHUMANRCD recog=1`**（`DataService.cs:234`/`:242`）——Rust 已照抄。
3. **失败应答 `DBR_LOADHUMANRCD recog=0` 不清 in-save**（`DataService.cs:248` + `PlayerDataService.cs:124-129`）——Rust 已照抄（`apply_save_reply`）。
4. **`CacheStorageService.Add` 只替换不插入（缓存恒空）**（`CacheStorageService.cs:12-18`）——Rust 已照抄并单测钉住。
5. **死亡不是存档触发点**（`PlayObject.cs:1964-1997`）——Rust 已照抄。
6. **`IsFull()` 恒 false**（`FrnEngn.cs:120-136`）——Rust 已照抄（`FrontEngine::is_full`）。
7. **建角 `NewChrData` 忽略 `Add` 返回值、恒 true**（`UserService.cs:278-279`）——**Rust 未照抄（G4）**。
8. **索引表全部无事务（autocommit）**（`PlayRecordStorage.cs` 各方法）——Rust 已照抄。
9. **`RollBack()` 条件写反 ⇒ 显式回滚无效**（`DBContext.cs:67-73`）——Rust 用显式回滚/drop，净等价；不要求逐字复制，但不得据此声称 oracle “有回滚”。

### 6.2 Rust 独有缺陷（应修）

| 编号 | 缺陷 | 优先级 |
|---|---|---|
| G1 | 关服不存档 | P0 |
| G2 | DB 连接/事务开启错误不回包 ⇒ in-save 永久卡死 | P0 |
| G3 | 6000 自动建角 Err 不回包 | P1 |
| G5 | 周期存档不跳过 in-save | P2 |
| G6 | `play_data_add` 不更新 `name_quick` | P2 |
| G7 | 物品表读失败整表放弃 | P3 |
| G11 | 换服存档门未移植 | P2 |

### 6.3 必须升级为“照抄”的偏差

| 编号 | 偏差 | 处置 |
|---|---|---|
| G4 | 建角失败语义被“修好” | 回退为 oracle 口径（成功应答 + 孤儿索引），登记 `T-*`；或 owner 裁决保留并登记 `D-*` |

---

## 7. 观察项（非缺口，但禁止单方面“修”）

- **W1（缓存先更新、库写失败的顺序）**：两侧都是 `cache.add` → `play_data_update`（`DataService.cs:233-234`；
  `data.rs::save_human_rcd`），但 `CacheStore::add` 对缺键 no-op ⇒ 缓存恒空，**当前没有脏缓存窗口**。
  一旦有人把 `add` 改成插入（看起来像修 bug），5 分钟回写 + 库写失败立刻变成“缓存比库新”的脏读源
  （`LoadHumanRcd` 先读缓存，`DataService.cs:174` / `load_human_rcd`）。**改 `add` 必须同时重审本文件 M12/M15 与 §5。**
- **W2（`IsSaveing` 永不复位）**：oracle `CharacterDataProcessor.cs:78` 置 true 后全仓库无复位点；Rust 无该字段，
  但 `engine.begin_save` 同样只在成功应答时清 ⇒ 失败路径的“永久 in-save”一致。修 G2/G3 时不要顺手把 in-save
  改成“失败也清”，否则会偏离 oracle 的失败应答语义（`openmir2-m3-gamesvr-save` 已裁决选项 a）。
- **W3（`SaveMagics` 的 DELETE 在 try 外）**：这是 oracle 唯一会触发整事务回滚的业务失败；Rust 已显式 rollback。
  若后续把魔法表改成“删+插分事务”，会改变“魔法全丢 vs 旧魔法保留”的可观测结果。

---

## 8. 复现与证据

本审计的全部行号由以下命令实测（主检出只读，未改动任何文件）：

```sh
# oracle 事务边界
sed -n '45,80p'   src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs | cat -n
sed -n '82,171p'  src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs | cat -n
sed -n '206,279p' src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs | cat -n
sed -n '434,539p' src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs | cat -n
sed -n '47,82p'   src/Storeages/DBSvr.Storage.MySQL/DBContext.cs | cat -n
sed -n '51,88p'   src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs | cat -n
# GameSvr 触发点
sed -n '690,720p' src/GameSrv/Word/WorldServer.cs | cat -n
sed -n '1077,1092p' src/GameSrv/Word/WorldServer.cs | cat -n
sed -n '222,243p' src/GameSrv/TimedService.cs | cat -n
sed -n '133,145p' src/GameSrv/AppService.cs | cat -n
# DBSrv 缓存与应答
sed -n '207,257p' src/DBSrv/Services/Impl/DataService.cs | cat -n
sed -n '12,42p'   src/Storeages/DBSrv.Storage/Impl/CacheStorageService.cs | cat -n
sed -n '73,96p'   src/DBSrv/TimedService.cs | cat -n
# Rust 侧（只按符号检索，不取行号入文）
grep -n "fn save_record\|fn play_data_add\|fn play_data_update" mir2-rs/bins/db-srv/src/playdata.rs
grep -n "fn save_human_rcd\|fn process_cache_storage" mir2-rs/bins/db-srv/src/data.rs
grep -n "fn new_chr\|fn delete_chr" mir2-rs/bins/db-srv/src/handler.rs
grep -n "fn save_human_rcd\|fn drain_periodic_save\|fn apply_save_reply" mir2-rs/bins/game-svr/src/main.rs
grep -n "fn in_save_rcd_list\|fn begin_save\|fn end_save" mir2-rs/bins/game-svr/src/load.rs
```

本地门禁（本次只加一个 md，未触代码；合并前仍应由 reviewer 重跑）：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p msgcodegen -- --check
```

---

## 9. 覆盖度与残留

- 覆盖：清册 §1/§2、矩阵 22 行、缺口 13 条（G1–G13）、崩溃一致性 8 类、必须照抄/独有缺陷/偏差各表。
- 未覆盖（明确登记）：
  - `crates/storage::account` 的完整账号事务矩阵（仅 M22 参考行）；
  - 拍卖行 `MarketService`/`market.rs` 的事务与缓存（属另一写集）；
  - 组队/行会/邮件等全局服务的落库（不在角色存档链）；
  - `BoReconnection` 换线重连路径（依赖换服 G11）。
- 残留物：仅本 md；worktree `/e/tmp/impl-txnaudit` 内一次提交（不 push）；无临时库/进程/端口占用。
