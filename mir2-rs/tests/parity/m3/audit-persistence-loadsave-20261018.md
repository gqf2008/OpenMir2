# 角色载入 → 运行期 → 存档字段链审计（oracle ↔ Rust）

- **卡/任务**：`audit-persistence-loadsave-20261018`（只读审计）
- **基线**：`0ebdbbf54`（worktree `/e/tmp/impl-persist`，detached）
- **审计日**：2026-10-18
- **纪律**：未改共享主检出；只读数据库**结构**（`sql/mir2_db.sql` 与 `SHOW COLUMNS` 口径），未写库、未起服务、未占端口。
- **oracle 行号**：全部逐条实测（`awk`/`grep -n` 于基线 commit 的 C# 源）。
- **Rust 引用**：只给符号/文件路径，不写 `:行号`。
- **订正（rev2，2026-10-18）**：按复核 must_fix M1–M3 订正——① `IncHealing` 改判「单边/差异」并新增 G-19（R）；
  ② 建角路径改述为 `db.rs::data_add`，G-03 修复面修正并新增 G-20；③ G-14 失败语义改述为「净效果与 oracle 相同」。
- **说明**：任务文本写的 `Query.cs`/`Save.cs` 实际文件为
  `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs` /
  `PlayDataStorage.Save.cs`（另有 `PlayDataStorage.Add.cs`）。Rust 侧 `crates/storage` 只有
  **记录类型/MemoryPack 与 trait**，真正的 MySQL 查询/写入在 `bins/db-srv/src/playdata.rs`，
  GameSvr 侧存档合成在 `bins/game-svr/src/main.rs` 的 `build_save_record` / `SavedHuman` / `SessionGround`。

---

## 1. oracle 调用链（实测行号）

### 1.1 载入

```text
Gate 登录
  -> WorldServer.ProcessHumans()                       WorldServer.cs:481-545
       -> PlayerDataService.GetPlayData()              WorldServer.cs:512
       -> ProcessHumansMakeNewHuman()                  WorldServer.cs:518
            -> GetHumData(playObject, ref humanRcd)    WorldServer.cs:1100-1229
  -> PlayerDataService.QueryCharacterData()            PlayerDataService.cs:72-88
       -> LoadRcd()                                    PlayerDataService.cs:166-181
  -> GameSrv/Word/Threads/CharacterDataProcessor.cs
       ProcessReadStorage()                            CharacterDataProcessor.cs:95-147
       LoadCharacterData()                             CharacterDataProcessor.cs:149-182
  -> DBSrv DataService.ProcessServerMsg(DB_LOADHUMANRCD) DataService.cs:129-149
       LoadHumanRcd()                                  DataService.cs:151-205
         -> _cacheStorage.Get()                        DataService.cs:174-175
         -> _playDataStorage.Get(chrName, ...)         DataService.cs:177
              -> PlayDataStorage.Get/GetRecord         Query.cs:111-126 / 153-190
```

### 1.2 存档

```text
WorldServer.SaveHumanRcd(playObject)                   WorldServer.cs:1077-1092
  -> MakeSaveRcd(playObject)                           WorldServer.cs:1231-1382
  -> FrontEngine.AddToSaveRcdList()                    WorldServer.cs:1091
调用点：
  - 周期存档（SaveHumanRcdTime，缺省 600000ms）        WorldServer.cs:698-703
  - MakeGhost/断线支                                   WorldServer.cs:713
  - 定时全量（TimedService）                           TimedService.cs:230-243
  - 停服强制保存                                       AppService.cs:133-145

CharacterDataProcessor.ProcessSaveStorage()            CharacterDataProcessor.cs:64-93
  -> PlayerDataService.SaveCharacterData()             PlayerDataService.cs:94-98 / 100-112
  -> DBSrv DataService.SaveHumanRcd()                  DataService.cs:207-257
       -> _cacheStorage.Add()                          DataService.cs:233
       -> _playDataStorage.Update(chrName, ...)        DataService.cs:234
            -> PlayDataStorage.Update/SaveRecord       Save.cs:14-80
              事务内 8 个 Save*：                       Save.cs:57-66
                SaveRecord(characters)                 Save.cs:82-137
                SaveAblity(characters_ablity)          Save.cs:139-171
                SaveItem(characters_item)              Save.cs:206-279
                SaveBagItem(characters_bagitem)        Save.cs:284-355
                SaveStorageItem(characters_storageitem)Save.cs:360-432
                SaveMagics(characters_magic)           Save.cs:434-464
                SaveBonusability(characters_bonusability) Save.cs:466-491
                SaveStatus(characters_status)          Save.cs:509-539
```

**未进存档事务**：`SaveQuest`（`Save.cs:493-507`）只 `DELETE`、**没有任何 INSERT**，且
`SaveRecord`（`Save.cs:57-66`）**从未调用它** ⇒ 任务表持久化是死代码。

### 1.3 建角（新增路径，影响“行是否存在”）

`PlayDataStorage.Add` → `AddRecord` → `CreateCharacters`（`Add.cs:90-155`）、
`CreateAblity`（`Add.cs:157-193`）、`CreateStatus`（`Add.cs:195-212`）、
`CreateUseItem`（`Add.cs:214-264`）、`CreateBagItem`（`Add.cs:266-312`）、
`CreateStorageItem`（`Add.cs:314-360`）、`CreateItemAttr`（`Add.cs:362-390`）。

**关键事实**：
- 物品三表在建角时逐槽插入 13/46/50 行（空物品 `MakeIndex=0`）；
- **`characters_bonusability` 没有任何 Create\*** ⇒ 新角色永远没有该行；
- `CreateItemAttr` 只在 `MakeIndex > 0` 时 INSERT，而建角物品全为空 ⇒ **attr 表永远不产生行**；
- schema 佐证：`sql/mir2_db.sql` 中 `characters_item_attr` 的 `AUTO_INCREMENT = 1`（从无插入）。

**Rust 对应（复核补录）**：网关正常建角路径是 `bins/db-srv/src/handler.rs`（建角消息）→
`db.rs::data_add` → `create_character`，同一事务写 `characters` + `characters_ablity` +
`characters_status` + 三张物品表（13/46/50 行），**不建 `characters_bonusability`**，与 oracle
`Add.cs` 口径一致。`playdata.rs::create_ablity_status` **不是正常建角路径**：它只服务于
`play_data_add` 兜底（收到未知角色名的存档时，对应 C# `DataService.SaveHumanRcd` 的
`_playDataStorage.Add(humanRcd)` 分支），且其中调用的 `save_ablity` 是 **UPDATE** ⇒ 该路径不会新建
`characters_ablity` 行（见 G-20）。

---

## 2. oracle 逐字段清册（表.列 → 结构字段 → 载入点 → 存档点）

图例：`✓` = 读/写；`✗` = 不读/不写；`≈` = 写了但值源错误 / 映射错位。

### 2.1 `characters`

| 列 | 结构字段 | 载入（Query.cs） | 存档（Save.cs） | 备注 |
|---|---|---|---|---|
| `Id` | 无 | 查询键 | WHERE 键 | 身份 |
| `ServerIndex` | `ServerIndex` | ✗ | ✓ `:92`（`hd.ServerIndex`，由 `WorldServer.cs:1236` 填 `M2Share.ServerIndex`） | 载入不读 |
| `LoginID` | `Account` | ✓ `:205` | ✓ `:93` | |
| `ChrName` | `ChrName` | ✓ `:206` | SQL 不含（身份列） | |
| `MapName` | `CurMap` | ✓ `:209` | ✓ `:94` | |
| `CX` / `CY` | `CurX` / `CurY` | ✓ `:211-212` | ✓ `:95-96` | |
| `Dir` | `Dir` | ✓ `:213` | ✓ `:98` | 运行期转身改 `playObject.Dir` |
| `Hair` | `Hair` | ✓ `:214` | ✓ `:99` | |
| `Sex` | `Sex` | ✓ `:215` | ✓ `:100` | |
| `Job` | `Job` | ✓ `:216` | `Job = Job`（`:85`，**无效自赋值**；`@Job` 参数 `:101` 未被 SQL 使用） | 运行期不变 |
| `Level` | 无（`GetQryChar` 读 `:593`） | ✗ | ✓ `:97`（`hd.Abil.Level`） | 真源在 `characters_ablity.Level` |
| `Gold` | `Gold` | ✓ `:217` | ✓ `:102` | |
| `GamePoint` | `GamePoint` | ✓ `:242` | ✓ `:103` | |
| `HomeMap` / `HomeX` / `HomeY` | 同 | ✓ `:220-223` | ✓ `:104-106` | |
| `PkPoint` | `PKPoint` | ✗ | ✓ `:107` | **只写不读** |
| `ReLevel` | `ReLevel` | ✓ `:238` | ✓ `:108` | |
| `AttatckMode` | `AttatckMode` | ✓ `:246` | ✓ `:109` | |
| `FightZoneDieCount` | `FightZoneDieCount` | ✓ `:250` | ✓ `:110` | |
| `BodyLuck` | `BodyLuck` | ✓ `:254` | ✓ `:111` | |
| `IncHealth` / `IncSpell` / `IncHealing` | 同 | ✓ `:247-249` | ✓ `:112-114` | |
| `CreditPoint` | `CreditPoint` | ✓ `:233` | ✓ `:115` | |
| `BonusPoint` | `BonusPoint` | ✓ `:240` | ✓ `:116` | |
| `HungerStatus` | `HungerStatus` | ✓ `:244` | ✓ `:117` | |
| `PayMentPoint` | `PayMentPoint` | ✓ `:243` | ✓ `:118` | |
| `LockLogon` | `LockLogon` | ✓ `:239` | ✓ `:119` | |
| `MarryCount` | `MarryCount` | ✗ | ✓ `:120` | **只写不读** |
| `AllowGroup` | `AllowGroup` | ✓ `:245` | ✗（UPDATE 列清单无此列，`:85-88`） | **只读不写** |
| `AllowGroupReCall` | `AllowGroupReCall` | ✓ `:252` | ≈ `:121`（`@AllowGroupReCall` 绑的是 `hd.AllowGroup`） | **列映射错位** |
| `GroupRcallTime` | `GroupRcallTime` | ✓ `:253` | ✓ `:122` | |
| `AllowGuildReCall` | `AllowGuildReCall` | ✓ `:251` | ✓ `:123` | |
| `IsMaster` / `MasterName` / `DearName` | 同 | ✓ `:226-232` | ✓ `:124-126` | |
| `StoragePwd` | `StoragePwd` | ✓ `:236` | ✓ `:127` | |
| `Deleted` | 无 | `LoadQuickList` 过滤 `Deleted=0`（`:31`） | 恒写 0（`:128`） | |
| `CREATEDATE` / `LASTUPDATE` | 无 | ✗ | 建角 `now()`；存档 `LASTUPDATE=now()`（`:88`） | |

**GameGold**：`CharacterData.GameGold` 在载入时读的是 **`Gold` 同一列**（`:241`），
存档 SQL（`:85-88`）**没有任何 GameGold 列**（表里也没有该列）⇒ 运行期 GameGold 变动永不落库。

### 2.2 `characters_ablity`

| 列 | 结构字段 | 载入（Query.cs） | 存档（Save.cs） | 备注 |
|---|---|---|---|---|
| `PlayerId` | 键 | WHERE | WHERE | |
| `Level` | `Abil.Level` | ✓ `:279` | ✓ `:145` | |
| `Hp` | `Abil.HP` + `Abil.AC` | ✓ `:280-282`（`LoWord`→HP、`HiWord`→AC） | ≈ `:151`（只写 `Abil.HP`，**不写高 16 位**） | AC 重登为 0 |
| `Mp` | `Abil.MP` + `Abil.MAC` | ✓ `:283-285`（`LoWord`→MP、`HiWord`→MAC） | ≈ `:152`（只写 `Abil.MP`） | MAC 重登为 0 |
| `Ac` | `Abil.AC` | ✗（读的是 `Hp` 高字） | ≈ `:146`（绑的是 **`Abil.Level`**） | 列值无消费者 |
| `Mac` | `Abil.MAC` | ✗（读的是 `Mp` 高字） | ✓ `:147`（但 `MakeSaveRcd` 不填 MAC） | 列值无消费者 |
| `Dc` / `Mc` / `Sc` | `Abil.DC/MC/SC` | ✓ `:286-288` | ✓ `:148-150`（`MakeSaveRcd` 新建 `Ability` 未填 ⇒ 恒 0） | 登录 `RecalcAbilitys` 重算 |
| `MaxHP` / `MAxMP` | `Abil.MaxHP/MaxMP` | ✗ | ✓ `:153-154` | **只写不读**；登录重算 |
| `Exp` / `MaxExp` | 同 | ✓ `:289-290` | ✓ `:155-156` | |
| `Weight` / `MaxWeight` / `WearWeight` / `MaxWearWeight` / `HandWeight` / `MaxHandWeight` | 同 | ✓ `:291-296` | ✓ `:157-162` | |
| `ModifyTime` | 无 | ✗ | `now()`（`:141`） | |

**掩蔽事实**：`WorldServer.GetHumData`（`:1100-1229`）**只拷** Level/HP/MP/MaxHP/MaxMP/Exp/MaxExp/Weight 族
（`:1113-1125`），**不拷 AC/MAC/DC/MC/SC**；登录随后 `RecalcLevelAbilitys()/RecalcAbilitys()` 重算
（`PlayObject.cs:1308-1310`）。因此上述列映射错位**当前无玩家可见后果**，但 DB 列会持续漂移。

### 2.3 附属表

| 表 | 列 → 字段 | 载入 | 存档 | 备注 |
|---|---|---|---|---|
| `characters_bonusability` | `AC/MAC/DC/MC/SC/HP/MP/HIT/SPEED/RESERVED` → `BonusAbil.*` | ✓ 有行时 `:320-332` | ✓ `:468-484` **仅 UPDATE** | 无行即静默 no-op；建角不建行 |
| `characters_magic` | `MagicId/Level/UseKey/CurrTrain` → `Magic[pos].MagIdx/Level/MagicKey/TranPoint` | ✓ `:343-373`（**无 ORDER BY**，按结果行序填槽） | ✓ `:434-464`（`DELETE` 后按数组序 INSERT，只写 `MagIdx>0`） | 槽位下标不落库 |
| `characters_item` | `Position/MakeIndex/StdIndex/Dura/DuraMax` → `HumItems[Position]` | ✓ `:375-401` | ✓ `:206-279` 差量 UPDATE | 头盔/项链交换在 `GetHumData :1167-1168` / `MakeSaveRcd :1302-1303` |
| `characters_bagitem` | 同上 → `BagItems[Position]` | ✓ `:403-429` | ✓ `:284-355` | 46 槽 |
| `characters_storageitem` | 同上 → `StorageItems[Position]` | ✓ `:431-458` | ✓ `:360-432` | 50 槽 |
| `characters_item_attr` | `VALUE0..13` → `Item.Desc[0..13]` | ≈ `:610-638`（`userItems[nPosition].Desc[nPosition] = VALUE{nPosition}`，**行序同时索引物品槽与 Desc**） | ✓ `:541-571` **仅 UPDATE**（无 INSERT） | 映射错位 + 永不建行 |
| `characters_status` | `Status0..14` → `StatusTimeArr[0..14]` | ✓ `:460-483` | ✓ `:509-539` | `Status15` 存在但两侧均未用 |
| `characters_quest` | `QUESTOPENINDEX/QUESTFININDEX/QUEST` → 无结构字段 | ✗ | ✗（`SaveQuest` 死代码，仅 DELETE 且未调用） | 任务数组 `QuestUnitOpen/QuestUnit/QuestFlag` 从不落库 |

---

## 3. Rust 清册（符号级）

### 3.1 `crates/storage`（记录类型 / trait，不是 SQL 层）

- `mir2_storage::playerdata::{CharacterData, CharacterDataInfo, Ability, NakedAbility, MagicRcd, ServerUserItem}`：
  与 C# MemoryPack 逐字节镜像；`Ability` 22 成员、`CharacterData` 52 成员。
- `mir2_storage::traits::{PlayRecordStorage, PlayDataStorage, CacheStorage}`：镜像 C# 四接口。
- `crates/storage/src/account.rs`：只有账号面 SQL；角色面 SQL 不在本 crate。

### 3.2 `bins/db-srv/src/playdata.rs`（真正的存储层）

载入：`get_record` 组合
`get_chr_record`（`characters` 主表）、`get_abil_record`、`get_bonus_abil_record`、
`get_magic_record`、`get_item_slots`（`characters_item`/`characters_bagitem`/`characters_storageitem`）、
`query_item_attr`、`get_player_status`。

存档：`save_record` 组合
`save_characters`（`SAVE_CHARACTERS_SQL`）、`save_ablity`、`save_item_table`（三表共用）、
`comparer_user_item`、`update_item_attr`、`clear_item_attr`、`save_magics`、
`save_bonusability`、`save_status`。

建角（**网关正常路径**）：`bins/db-srv/src/handler.rs` 的建角消息 → `db.rs::data_add` →
`create_character`（同事务写 `characters` + `characters_ablity` + `characters_status` + 三张物品表；
**不建 `characters_bonusability`**）。
兜底：`playdata.rs::play_data_add` → `create_characters` + `create_ablity_status` + `create_empty_items`
（仅未知角色名的存档触发；`create_ablity_status` 调用的是 `save_ablity`＝**UPDATE**，不新建
`characters_ablity` 行，见 G-20）。

分发：`bins/db-srv/src/data.rs::{load_human_rcd, save_human_rcd, process_cache_storage, cache_flush_task}`；
缓存 `bins/db-srv/src/cache.rs::CacheStore`（`add` 只在键已存在时替换）。

### 3.3 `bins/game-svr`（会话态与存档合成）

- `SessionGround`（`main.rs`）：`state: GroundState`（`item_list`/`gold`/`wabil`）、`storage`、
  `game_gold`、`game_point`、`use_items`（13 槽）、`equip: EquipInputs`、`recovery`、
  `hunger_status`、`save_rcd_tick` 等。
- 载入映射：`use_items_from`（含 Necklace/Helmet 交换）、`bag_items_from`、
  `storage::load_from_record`、`enter::recalc_for_session`。
- 存档合成：`SavedHuman`（载入的整条 `record` + `enter_map`）、`build_save_record`、
  `human_items_for_save`、`bag_items_for_save`、`storage::storage_items_for_save`、
  `server_item_from_bag`、`default_server_item`。
- 发请求：`save_human_rcd`；周期：`drain_periodic_save` / `periodic_save_due`。
- 运行期写回记录：`handle_magic_key_cmd`（改 `saved.record.data.magic` 的 Key）、
  魔法训练写回 `tran_point`（`main.rs` 的两处 `record.data.magic` 写点）。

---

## 4. 矩阵（行 = 表.列/字段组；共 **49** 行）

> `对称性`：`一致` = oracle 与 Rust 同行为（含同 bug）；`差异` = 两侧不同。
> `状态`：`对称` / `单边` / `缺失` / `不适用`。
> `tier`：按 AGENTS.md 判据分级；凡改动会改变落库字节/字段级 diff ⇒ **R**（需真机全量对拍）。

| # | 表.列 | 结构字段 | oracle 载入 | oracle 存档 | Rust 载入 | Rust 存档 | 对称性 | 状态 | 可观测后果 | tier | 判据 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `characters.ServerIndex` | `ServerIndex` | ✗ | ✓ `:92`（`M2Share.ServerIndex`） | ✗ | ✓ 但恒 0 | 差异 | 单边 | 多机配置下该列被写 0 | R | 存档后 `SELECT ServerIndex`；oracle 侧配 `[Server] ServerIndex>0` |
| 2 | `characters.LoginID` | `Account` | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | — | L | 列值比对 |
| 3 | `characters.ChrName` | `ChrName` | ✓ | ✗（身份列） | ✓ | ✗ | 一致 | 对称 | 改名不落库（两侧无改名路径） | L | 静态 SQL 审计 |
| 4 | `characters.MapName` | `CurMap` | ✓ | ✓ | ✓ | ✓（覆盖当前图） | 一致 | 对称 | — | R | 换图后存档查列 |
| 5 | `characters.CX/CY` | `CurX/CurY` | ✓ | ✓ | ✓ | ✓（覆盖实体坐标） | 一致 | 对称 | — | R | 移动后存档查列 |
| 6 | `characters.Dir` | `Dir` | ✓ | ✓（运行期当前朝向） | ✓ | ✓ 但值=载入值 | 差异 | 单边 | 下线朝向不落库，重登回上次登录朝向 | R | 转身→存档→查列→重登 |
| 7 | `characters.Hair/Sex/Job` | 同 | ✓ | Hair/Sex ✓；Job=`Job` 无效 | ✓ | 同 | 一致 | 对称 | Job 运行期不变 ⇒ 无差 | L | 静态 SQL 审计 |
| 8 | `characters.Level` | 无（`GetQryChar` 用） | ✗ | ✓ | ✗ | ✓ | 一致 | 对称 | 真源在 ablity.Level | L | 两表 Level 比对 |
| 9 | `characters.Gold` | `Gold` | ✓ | ✓ | ✓ | ✓（`sg.state.gold`） | 一致 | 对称 | — | R | 交易/捡钱后存档 |
| 10 | `characters.Gold(→GameGold)` | `GameGold` | ✓ 同列 | ✗（无列/无参数） | ✓ 同列 | ✗ | 一致 | 单边 | 元宝变动（寄售）重登丢失 | R | 改 `game_gold`→存档→复读 |
| 11 | `characters.GamePoint` | `GamePoint` | ✓ | ✓ | ✓ | ✓ 但未 overlay 会话值 | 一致 | 对称 | 潜在：将来运行期改 GamePoint 不落库 | R | 改会话 game_point→存档 |
| 12 | `characters.PayMentPoint` | `PayMentPoint` | ✓ | ✓ | ✓ | ✓（保留记录） | 一致 | 对称 | 无运行期写 | L | 列值比对 |
| 13 | `characters.HomeMap/X/Y` | 同 | ✓ | ✓ | ✓ | ✓（保留；建角写） | 一致 | 对称 | 运行期 `GetStartPoint` 可改写；Rust 未建模（链对称） | L | 建角/重登比对 |
| 14 | `characters.PkPoint` | `PKPoint` | ✗ | ✓（运行期有加/减点） | ✗ | ✓ | 一致 | 单边 | 杀红名/自然衰减改的值重登归零（DB 写出后载入不读） | R | 杀红名→存档→重登 |
| 15 | `characters.ReLevel` | `ReLevel` | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | 运行期命令/脚本有写点（`ReNewLevelCommand`、`ExecutionProcessingSys`）；Rust 未建模，但持久化链本身对称 | L | 列值比对 |
| 16 | `characters.AttatckMode` | `AttatckMode` | ✓ | ✓（`@攻击模式` 可改） | ✓ | ✓ 但保留记录（命令未建模/未写回） | 差异 | 单边 | 攻击模式切换重登回旧值 | R | `@攻击模式`→存档→重登 |
| 17 | `characters.FightZoneDieCount` | 同 | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | 运行期 `PlayObject.Message.cs:532` 有自增；Rust 未建模（非持久化链缺口） | L | 列值比对 |
| 18 | `characters.BodyLuck` | `BodyLuck` | ✓ | ✓ | ✓ | ✓（`sg.equip.body_luck`） | 一致 | 对称 | — | R | 涨经验→存档查列 |
| 19 | `characters.IncHealth/IncSpell` | 同 | ✓ | ✓ | ✓ | ✓（recovery，&0xFF） | 一致 | 对称 | — | R | 恢复 tick→存档查列 |
| 20 | `characters.IncHealing` | `IncHealing` | ✓ | ✓（`RM_MAGHEALING` 累加 / 恢复期递减 / 死亡清零后写 `(byte)`） | ✓ | ✗（保留载入值；`sg.recovery.inc_healing` 未写回，登录也不恢复） | 差异 | 单边 | 治疗术累积的 `IncHealing` 存档丢失、登录不恢复 | R | 治疗→存档查列→重登；比对 `sg.recovery.inc_healing` |
| 21 | `characters.CreditPoint` | `CreditPoint` | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | 运行期命令/师徒奖励有写点；Rust 未建模（链对称） | L | 列值比对 |
| 22 | `characters.BonusPoint` | `BonusPoint` | ✓ | ✓ | ✓ | ✓（保留；调点净变化 0） | 一致 | 对称 | 洗点命令未建模 | L | 列值比对 |
| 23 | `characters.HungerStatus` | `HungerStatus` | ✓ | ✓ | ✓ | ✓（`sg.hunger_status`） | 一致 | 对称 | — | R | 吃东西→存档查列 |
| 24 | `characters.LockLogon` | `LockLogon` | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | — | L | 列值比对 |
| 25 | `characters.MarryCount` | `MarryCount` | ✗ | ✓ | ✗ | ✓ | 一致 | 单边 | 结婚次数重登归零 | R | 脚本结婚→存档→重登 |
| 26 | `characters.AllowGroup` | `AllowGroup` | ✓ | ✗（UPDATE 无此列） | ✓ | ✗ | 一致 | 单边 | `@允许组队` 重登回旧值 | R | 命令切换→存档→重登 |
| 27 | `characters.AllowGroupReCall` | `AllowGroupReCall` | ✓ | ≈ 绑 `hd.AllowGroup` | ✓ | ≈ 同错绑 | 一致 | 列映射错位 | 组队传送开关被组队开关覆盖；两开关都不可靠 | R | 两命令分别切换→存档→查列 |
| 28 | `characters.GroupRcallTime` | 同 | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | 运行期计时器未建模 | L | 列值比对 |
| 29 | `characters.AllowGuildReCall` | 同 | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | — | L | 列值比对 |
| 30 | `characters.IsMaster/MasterName/DearName` | 同 | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | 拜师/结婚脚本未建模 | L | 列值比对 |
| 31 | `characters.StoragePwd` | `StoragePwd` | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | — | L | 列值比对 |
| 32 | `characters.Deleted/CREATEDATE/LASTUPDATE` | 无 | Deleted 过滤 | ✓ 0/now() | 同 | 同 | 一致 | 对称 | — | L | 软删后 LoadQuickList |
| 33 | `characters_ablity.Level` | `Abil.Level` | ✓ | ✓ | ✓ | ✓ | 一致 | 对称 | — | R | 升级→存档查列 |
| 34 | `characters_ablity.Hp/Mp`（低字） | `Abil.HP/MP` | ✓ LoWord | ✓ 只写低字 | ✓ | ✓ | 一致 | 对称 | HP/MP 本身往返正常 | R | 受伤→存档→重登 |
| 35 | `characters_ablity.Hp/Mp`（高字=AC/MAC） | `Abil.AC/MAC` | ✓ HiWord | ≈ AC/MAC 不写高字 | ✓ | ≈ | 一致 | 列映射错位 | AC/MAC 重登清零（登录 recalc 掩盖） | R | 存档前后 `HiWord(Hp)` |
| 36 | `characters_ablity.Ac/Mac` | `Abil.AC/MAC` | ✗ | ≈ `Ac=Level` | ✗ | ≈ | 一致 | 列映射错位 | 列值无消费者 | R | 存档后查 `Ac/Mac` |
| 37 | `characters_ablity.Dc/Mc/Sc` | `Abil.DC/MC/SC` | ✓ | ✓ 恒 0 | ✓ | ✓ 恒 0 | 一致 | 单边 | DB 列恒 0；登录 recalc 掩盖 | R | 存档后查三列 |
| 38 | `characters_ablity.MaxHP/MAxMP` | `Abil.MaxHP/MaxMP` | ✗ | ✓ | ✗ | ✓ | 一致 | 单边 | 重登 0，recalc 掩盖 | R | 存档后查列→重登 |
| 39 | `characters_ablity.Exp/MaxExp` | 同 | ✓ | ✓ | ✓ | ✓（MaxExp 由 `need_exps` 重算） | 一致 | 对称 | MaxExp 重算覆盖 | R | 打怪→存档查列 |
| 40 | `characters_ablity.Weight` 族 6 列 | 同 | ✓ | ✓ | ✓ | ✓（wabil） | 一致 | 对称 | — | R | 捡物→存档查列 |
| 41 | `characters_ablity.ModifyTime` | 无 | ✗ | now() | ✗ | now() | 一致 | 对称 | — | L | 列值比对 |
| 42 | `characters_bonusability` 全 10 列 | `BonusAbil.*` | ✓ 有行时 | ✓ 仅 UPDATE | ✓ | ✓ | 一致 | 对称（但无行 ⇒ no-op） | 新角色奖励属性永不落库 | R | 建角→查行；调点→存档 |
| 43 | `characters_magic` 全 4 列 | `Magic[pos].*` | ✓ 无 ORDER BY | ✓ DELETE+INSERT | ✓ 同行序 | ✓ | 一致 | 对称（槽序不落库） | 槽位/按键依赖插入序；>20 截断 | L | 多技能存档→重登比对 |
| 44 | `characters_item`（13 槽） | `HumItems` | ✓ | ✓ 差量 UPDATE | ✓ | ✓ | 一致 | 对称 | 头盔/项链交换；Dura 往返 | R | 穿脱/磨损→存档→重登 |
| 45 | `characters_bagitem`（46 槽） | `BagItems` | ✓ | ✓ 差量 UPDATE | ✓ | ✓ | 一致 | 对称 | — | R | 捡/丢→存档→重登 |
| 46 | `characters_storageitem`（50 槽） | `StorageItems` | ✓ | ✓ 差量 UPDATE | ✓ | ✓ | 一致 | 对称 | — | R | 存/取→存档→重登 |
| 47 | `characters_item_attr.VALUE0..13` | `Item.Desc[14]` | ≈ 行序错位 | ✓ 仅 UPDATE | ≈ 同 | ✓ 同 | 一致 | 缺失 | 升级属性永不落库；已有行也串位 | R | 造 attr 行→存档→重登 |
| 48 | `characters_status.Status0..14` | `StatusTimeArr[15]` | ✓ | ✓ | ✓ | ✓ | 一致 | 对称（Status15 未用） | buff/毒状态未建模 ⇒ 恒为载入值 | L | 中毒→存档查列 |
| 49 | `characters_quest` 全表 | `QuestUnitOpen/QuestUnit/QuestFlag` | ✗ | ✗（死代码） | ✗ | ✗ | 一致 | 缺失 | 任务进度/标志重登全丢 | R | 接任务→存档→重登 |

> 行号说明：矩阵中 `:NNN` 默认 oracle 文件 —— 第 1-32 行为 `PlayDataStorage.Query.cs` /
> `PlayDataStorage.Save.cs`（同一字段载入/存档行号已分别标注）；第 33-41 行为 Query/Save 的 ablity 段；
> 第 42-49 行为对应附属表的 Query/Save 方法（详见 §2 与 §8 速查表）。

**矩阵行数：49 行**。

---

## 5. 重点核查名单逐项结论

| 名单项 | 结论 |
|---|---|
| **`GameGold` 与 `Gold` 共用 `characters.Gold` 列** | 确认。载入 `Query.cs:241` 把 `Gold` 列同时写入 `Gold` 与 `GameGold`；存档 `Save.cs:85-88` 只写 `Gold`，表无 `GameGold` 列。运行期 `GameGold` 变更（元宝寄售 `selloff`）**永不落库**；两侧同 bug。 |
| **`HungerStatus`** | 对称 ✓。`Query.cs:244` ↔ `Save.cs:117`；Rust `get_chr_record`/`save_characters` 同名映射，`build_save_record` 从 `sg.hunger_status` 覆盖。`CM_EAT` 改动的值能落库。 |
| **`UseItems` 13 槽（含耐久）** | 槽位/物品/耐久**列**往返通（`Query.cs:375-401` ↔ `Save.cs:206-279`；Rust `get_item_slots`/`save_item_table`），头盔/项链交换两侧一致。**但** Rust 世界实体 `Entity.use_items` 从未由 `sg.use_items` 装载（`Entity::new` 全零，宿主无同步写点），而耐久磨损读的是 `Entity.use_items`（`crates/world/src/combat.rs::struck_damage_durability`）⇒ ① 装备耐久永不下降，存档 Dura 恒为载入值；② 每个 `Index>0` 槽位的 `Random(8)` 抽数不发生，与 oracle 的 RNG 流不一致。**这是 R 级缺口**。 |
| **`IncHealth/IncSpell/IncHealing`** | **不对称：`IncHealing` 单边**。oracle 三者均有运行期写点：`IncHealing` 由 `RM_MAGHEALING` 累加（上限 300，`PlayObject.Message.cs:1619-1628`）、恢复期递减（`PlayObject.Message.cs:113-121`）、死亡清零（`PlayObject.cs:1971-1972`）；存档 `WorldServer.cs:1281`、登录恢复 `WorldServer.cs:1151`。Rust 运行期也建模（`main.rs` 写 `sg.recovery.inc_healing`、`recover.rs` 消费），但**会话初始化只拷 `inc_health`/`inc_spell`**（`inc_healing` 落默认 0），**`build_save_record` 也只覆盖前两项** ⇒ 治疗术累积值存档丢失、登录不恢复（G-19）。前两项截断口径 `&0xFF` 与 `(byte)` 一致。 |
| **`PvpPoint/PkPoint`** | 本 oracle **没有 PvpPoint**（无列、无字段、无代码）。`PkPoint` 列存在：`Save.cs:107` 写、`Query.cs:192-254` **不读**；`GetHumData` 读 `humData.PKPoint`（`:1145`）但该字段恒为 0 ⇒ 重登归零。运行期确有加减点：杀红名 `IncPkPoint`（`PlayObject.Base.cs:1246`、`PlayObject.cs:1730/4366-4385`）、自然衰减（`PlayObject.Message.cs:249-254`）、命令/脚本（`IncPkPointCommand.cs:34`、`FreePenaltyCommand.cs:32`、`ExecutionProcessingSys.cs:2077-2100/2965-2974`）。Rust 未建模 PkPoint 会话态，但两侧载入都忽略该列 ⇒ 一致。 |
| **`HomeMap/HomeX/HomeY`** | 对称 ✓。载入 `Query.cs:220-223` ↔ 存档 `Save.cs:104-106`；Rust 建角时写、其余保留记录。脚本改回城点的运行期写点未建模（脚本面缺口，不在持久化层）。 |
| **技能/魔法列表** | 列往返通（`Query.cs:343-373` ↔ `Save.cs:434-464`；Rust `get_magic_record`/`save_magics`）。**槽位下标不落库**：载入 `SELECT *` 无 `ORDER BY`，按行序填 `Magic[position]`；存档按数组序 DELETE+INSERT。运行期 `CM_MAGICKEYCHANGE` 与训练点写回 `saved.record.data.magic` 能落库。>20 行截断两侧同。 |
| **`BonusPoint`** | 对称 ✓。载入 `Query.cs:240` ↔ 存档 `Save.cs:116`；Rust 保留记录值。`CM_ADJUST_BONUS` 在 oracle 里是 BUG-REPLICA（`BonusAbil.* += 0`、`BonusPoint = nPoint`，净变化 0），Rust 也只判定不写回 ⇒ 无持久化差异。 |
| **`AllowGroup/AllowGroupReCall`** | 列映射错位：`Save.cs:121` 把 `hd.AllowGroup` 绑到 `@AllowGroupReCall`；`AllowGroup` 列根本不在 UPDATE 清单；`hd.AllowGroupReCall` 无参数。结果两开关都不可靠。Rust 逐字照搬。 |
| **`characters_item_attr`** | **无 INSERT 路径**：`Add.cs::CreateItemAttr` 只对建角时的空物品（`MakeIndex=0`）调用 ⇒ 无行；`Save.cs:541-571` 只 UPDATE ⇒ 后续获得/升级的物品永远无 attr 行。且 `Query.cs:628` 用行序同时索引物品槽与 `Desc` 下标。Rust 逐字照搬。升级属性（`GameItemSystem.cs:187-244` 写 `Desc[0..7]`）因此**只活在内存**。 |
| **`characters_bonusability`** | 建角不建行（`Add.cs` 无 CreateBonusability），`Save.cs:466-491` 只 UPDATE ⇒ 新角色奖励属性永不落库。Rust `create_*` 同样不建行。 |
| **`characters_quest` / 任务数组** | 表存在但无载入；`SaveQuest`（`Save.cs:493-507`）只 DELETE 且从未被 `SaveRecord` 调用。`QuestUnitOpen/QuestUnit/QuestFlag` 在 packet 里有、DB 无映射 ⇒ 重登全丢。Rust 无任何 quest 落库代码。 |

---

## 6. 缺口清单（按「真机可达 × 影响」排序）

> 影响标签：`数据丢失` / `数值偏差` / `回档` / `配置错误`。
> 最小修复面只给文件/符号；是否允许偏离 oracle 需按 AGENTS.md 走 `T-*`/`B-*`/`D-*` 白名单。
> **复验补录**：G-19、G-20 为订正新增，编号追加在末尾（既有 G-01…G-18 编号保持不变）。

### G-01（R）`characters_item_attr` 永不 INSERT + 载入映射错位 —— **数据丢失**
- **真机可达**：任何获得或升级带 `Desc` 的装备；`Desc[0..7]` 被 `GameItemSystem.GetUpgradeStdItem` 消费（`GameItemSystem.cs:731`、`187-244`），`CustomItemSystem.cs:125` 读 `Desc[13]`。
- **后果**：升级/附加属性重登回档；即便手工造了 attr 行，`Query.cs:628` 的 `Desc[nPosition]=VALUE{nPosition}` 也会把第 2 件物品的属性写到它的第 2 个 Desc 字节。
- **最小修复面**：`bins/db-srv/src/playdata.rs::update_item_attr` 改为 upsert（缺行 INSERT）；
  `query_item_attr` 改为按 `MakeIndex` 关联物品、读全 14 个 `VALUE` 列写入该物品 `desc`；
  建角 `create_empty_items` 无需改（空物品无属性）。需白名单登记（偏离 C# 既有 bug）。

### G-02（R）`GameGold` 无落库列 —— **数据丢失**
- **真机可达**：元宝寄售/回收（Rust `selloff` 会改 `SessionGround.game_gold`）；载入把 `Gold` 当 `GameGold`。
- **后果**：元宝变动重登丢失；`Gold` 与 `GameGold` 在内存中分裂但只落一个。
- **最小修复面**：**表结构冻结**，不能加列。只能在 owner 授权 schema 变更后新增 `characters.GameGold` 列并在
  `get_chr_record`/`save_characters` 绑定；否则按「oracle 既有行为」登记，且必须在报告/看板留痕。

### G-03（R）`characters_bonusability` 无建行 —— **数据丢失**
- **真机可达**：洗点/师徒奖励修改 `BonusAbil`（`PlayObject.Base.cs:3007/3057`）。
- **后果**：只有 UPDATE，新角色永远不落库；奖励属性重登归零。
- **最小修复面**：在**网关正常建角路径** `db.rs::data_add`（`handler.rs` 调用的建角事务，
  `create_character` 之后）补 `INSERT INTO characters_bonusability (...) VALUES (?,0,…)`；
  或 `save_bonusability` 改 upsert。`playdata.rs::create_ablity_status` 不是正常建角路径
  （且其 `save_ablity` 是 UPDATE），只改它修不到真实建角（见 G-20）。

### G-04（R）`characters_quest` 完全未持久化 —— **回档**
- **真机可达**：M3⑥ 任务系统接线后；`QuestUnitOpen/QuestUnit/QuestFlag` 已被进图帧使用。
- **后果**：任务进度/标志重登全丢。
- **最小修复面**：`playdata.rs` 新增 `get_quest_record`/`save_quest`（`DELETE`+`INSERT` 或列存 128B 数组）；
  oracle `SaveQuest` 是死代码 ⇒ 属**主动修 bug**，需白名单。

### G-05（R）`AllowGroup` / `AllowGroupReCall` 列错位 + Rust 开关类命令未写回 —— **回档**
- **真机可达**：`@允许组队`（`PlayObject.Message.cs:919` / `Operate.cs:791/798/838`）、
  `AllowGroupReCall` 命令（`AllowGroupReCallCommand.cs:15`）、`@攻击模式`
  （`ChangeAttackModeCommand.cs:20/30/39`）、`@允许行会传送`（`AllowGuildRecallCommand.cs:12`）、
  组队传送计时（`GroupRecallCommand.cs:23-57`）。
- **后果**：`AllowGroup` 列永不更新；`AllowGroupReCall` 列被 `AllowGroup` 覆盖。两开关重登都不可靠。
  Rust 侧这些命令即使改了实体状态，也没有写回 `saved.record.data`，存档永远写载入值。
- **最小修复面**：`save_characters` 的 UPDATE 增加 `AllowGroup = ?`，并把 `@AllowGroupReCall` 绑
  `d.allow_group_re_call`；需白名单。Rust 还需把命令切换写回
  `saved.record.data.{allow_group,allow_group_re_call,attatck_mode,allow_guild_re_call,group_rcall_time}`。

### G-06（R）装备耐久运行期不磨损 / `Entity.use_items` 未装载 —— **数值偏差 + 数据丢失 + RNG 流**
- **真机可达**：每次被物理攻击；oracle 对每个 `Index>0` 的装备槽抽 `Random(8)` 并扣耐久。
- **后果**：Rust `struck_damage_durability` 只看 `Entity.use_items`（恒全零）⇒ ① 耐久永不下降，存档 Dura 是假值；
  ② 缺少每槽 `Random(8)` 抽数，攻击路径 RNG 流与 oracle 不一致。
- **最小修复面**：进世界/穿脱后把 `sg.use_items` 的 `(index,dura)` 同步到 `world.entities[actor_id].use_items`；
  存档前把 `Entity.use_items` 的 `dura` 回写 `sg.use_items`；`CM_TURN`/移动同步 `Entity.dir`。
  `crates/world` 只做数据承载，不引入新并行。

### G-07（R）`characters.PkPoint` 只写不读 —— **回档**
- **真机可达**：杀红名 `IncPkPoint`（`PlayObject.Base.cs:1246`、`PlayObject.cs:1730`）、
  自然衰减（`PlayObject.Message.cs:249-254`）、命令/脚本（`IncPkPointCommand.cs:34`、
  `FreePenaltyCommand.cs:32`、`ExecutionProcessingSys.cs:2077-2100/2965-2974`）。
- **后果**：本会话内 PK 值会被写出，但下一次载入完全忽略该列（`GetChrRecord` 192-266 无 `PkPoint`）
  ⇒ 每次重登归零；`PlayObject.PvpLevel()`（`PlayObject.cs:4354`）与红名判定全部回档。
- **最小修复面**：`get_chr_record` 补 `pk_point` 读取，并让 GameSvr 把会话 `PkPoint` 写回记录。
  需与 oracle 同步（oracle 也缺读取）⇒ 属主动修 bug，需白名单。

### G-08（R）`MarryCount` 只写不读 —— **回档**
- **真机可达**：结婚脚本 `ExecutionProcessingSys.cs:3641/3642/3667/3683` 自增。
- **后果**：结婚次数重登归零，`CHECKMARRYCOUNT` 条件失效。
- **最小修复面**：`get_chr_record` 补 `marry_count`；脚本面写回。需白名单（oracle 同缺）。

### G-09（R，**Rust-only**）`Dir` 不落当前朝向 —— **回档（轻）**
- **真机可达**：每次 `CM_TURN`/走步。
- **后果**：`build_save_record` 不覆盖 `rec.data.dir`，玩家实体 `dir` 也不随转身更新 ⇒ 存档写的是载入值。
- **最小修复面**：`new_player_entity`/进世界时用记录 `dir` 初始化 `Entity.dir`；`CM_TURN`/移动写 `Entity.dir`；
  `build_save_record` 从实体覆盖 `rec.data.dir`。

### G-10（R，**Rust-only**）`ServerIndex` 恒写 0 —— **配置错误**
- **真机可达**：`Server.conf [Server] ServerIndex > 0` 的多机部署。
- **后果**：`characters.ServerIndex` 被写 0；oracle 写 `M2Share.ServerIndex`（`ServerConf.cs:17` 可配）。
- **最小修复面**：GameSvr 配置解析补 `[Server] ServerIndex`；`build_save_record` 写 `rec.data.server_index`。

### G-11（R）`characters_ablity` AC/MAC/MaxHP 映射 —— **数值偏差（被登录重算掩盖）**
- **真机可达**：每次存档/重登。
- **后果**：`Ac` 列收 `Level`；`HiWord(Hp/Mp)` 恒 0 ⇒ AC/MAC 重登 0；`MaxHP/MAxMP` 只写不读；`Dc/Mc/Sc` 恒 0。
  登录 `RecalcLevelAbilitys/RecalcAbilitys` 重算，玩家无可见差异，但 DB 字段级 diff 永远非 0。
- **最小修复面**：要么按 oracle 保留（现状）；要么在 `save_ablity` 正确打包 `MakeLong(AC,HP)`/`MakeLong(MAC,MP)`
  并在 `get_abil_record` 读 `MaxHP/MAxMP`。属行为偏离，需 owner 裁决（R 级真机对拍）。

### G-12（L）`characters_magic` 槽序不落库
- **真机可达**：多技能角色的每次存档。
- **后果**：`SELECT *` 无 `ORDER BY`，槽位↔行序靠插入序；索引重建/优化器改序时技能槽位漂移。
- **最小修复面**：`get_magic_record` 加 `ORDER BY Id`（确定性且与插入序一致）。

### G-13（M）物品行缺失时新物品不落库
- **真机可达**：老账号/手工迁入的角色若缺某槽位行。
- **后果**：`comparer_user_item` 对 `old[i] == None` 直接 `continue`；`save_item_table` 只 UPDATE 不 INSERT
  ⇒ 该槽位新物品永久丢失。
- **最小修复面**：`save_item_table` 对缺行槽位补 INSERT（或建角/登录时补齐 13/46/50 行）。

### G-14（L）`save_magics` 失败语义：净效果与 oracle 相同（仅批次/往返不同）
- **真机可达**：仅错误路径（非法 `UseKey`、重复键、SQL 失败）。
- **后果**：`Save.cs:455-457` 虽把多条 INSERT 拼成一个 `CommandText`，MySQL 仍**逐条顺序执行**；
  后续语句报错时，已执行的前几条已生效。`SaveMagics` 在 `catch` 中吞掉异常（只打日志），
  `SaveRecord` 随后照常 `Commit`（`Save.cs:66`）⇒ oracle 同样是「部分插入」。
  Rust 逐条执行、逐条吞错，**净效果相同**，差异只在批次/往返次数（无字段级差异）。
- **最小修复面**：无需改；若将来追求单语句原子性，需两侧同时改并重跑对拍。

### G-15（L，**Rust-only latent**）`GamePoint` 未从会话覆盖
- **真机可达**：今日 GameSvr 无运行期写点；接元宝/点数系统后可达。
- **后果**：将来 `sg.game_point` 变动不会出现在存档。
- **最小修复面**：`build_save_record` 补 `rec.data.game_point = sg.game_point`。

### G-16（L）`characters.Level` 不载入记录
- **真机可达**：每次取档。
- **后果**：无（真源是 `characters_ablity.Level`，`GetQryChar` 单独读主表 Level）。
- **最小修复面**：无需改；保持 oracle 口径。

### G-17（L）`characters_status.Status15` 未用 / buff 未建模
- **真机可达**：中毒/麻痹等状态效果未移植时无影响。
- **后果**：buff/状态时长重登保留的是载入值；`Status15` 永远 0。
- **最小修复面**：状态系统移植时统一 15 槽契约；不需为 `Status15` 加映射。

### G-18（L）DBSrv 缓存层空转
- **真机可达**：每次存档。
- **后果**：`CacheStore::add` 只在键已存在时替换（C# 同），首次存档不填充缓存 ⇒ 5 分钟 flush 空转；
  本次审计未发现由此产生的数据错误（读路径回落到 DB）。
- **最小修复面**：无需改（C# 同构）；若将来要启用缓存，需先改 `add` 语义并全量对拍。

### G-19（R，**Rust-only**）`IncHealing` 运行期值不落库、登录不恢复 —— **数据丢失**
- **真机可达**：治疗术 `RM_MAGHEALING`（`PlayObject.Message.cs:1619-1628` 累加，上限 300）；
  恢复 tick 消费递减（`PlayObject.Message.cs:113-121`）；死亡清零（`PlayObject.cs:1971-1972`）。
- **后果**：oracle 存档 `IncHealing = (byte)playObject.IncHealing`（`WorldServer.cs:1281`）、
  登录 `GetHumData` 恢复（`WorldServer.cs:1151`）。Rust 运行期已建模（`main.rs` 写
  `sg.recovery.inc_healing`、`recover.rs` 消费），但**会话初始化只拷 `inc_health`/`inc_spell`**
  （`inc_healing` 落默认 0），**`build_save_record` 也只覆盖前两项** ⇒ 治疗术累积值存档丢失、
  重登不恢复，与 oracle 存在字段级落库差异。
- **最小修复面**：① 会话初始化补 `inc_healing`（由 `record.data.inc_healing` 装载）；
  ② `build_save_record` 补 `rec.data.inc_healing`（`sg.recovery.inc_healing & 0x00FF`）。
  涉及落库字节 ⇒ R 级真机对拍。

### G-20（R，**Rust-only defect**）`play_data_add` 兜底建角不建 `characters_ablity` 行 —— **数据丢失**
- **真机可达**：DBSrv 收到未知角色名的存档（`data.rs::save_human_rcd` 在索引缺失时调用
  `play_data_add`；C# `DataService.SaveHumanRcd` 同分支调 `_playDataStorage.Add(humanRcd)`）。
- **后果**：`play_data_add` → `create_ablity_status` 调用的是 `save_ablity`（**UPDATE**），
  但新角色尚无 `characters_ablity` 行 ⇒ 影响 0 行；此后每次存档仍只有 UPDATE ⇒ 该角色的
  等级/经验/HP/MP/负重等整表字段永不落库。C# 的 `Add → CreateAblity`（`Add.cs:157-193`）是 INSERT，
  因此该缺陷是 Rust-only。
- **最小修复面**：`create_ablity_status` 改为 INSERT（或 `save_ablity` 改 upsert），与
  `db.rs::data_add` 的直接 INSERT 对齐。

---

## 7. 未覆盖 / 边界（登记）

1. **`characters_indexes`**（选角/建删角色索引）不属本次「角色载入→存档字段链」，未逐列展开；
   Rust 侧在 `bins/db-srv/src/db.rs`（`UPDATE characters_indexes`、`SELECT ChrName, Job, Hair, Sex, Level`）有独立实现。
2. **`characters_item_attr` 的 `Id` 主键**：`ClearItemAttr` 把 `MakeIndex` 置 0 而不删行，旧行会残留（泄漏，不影响读）。
3. **数据库真值**：本审计只读 schema 与样例 `INSERT`（`sql/mir2_db.sql`），未连现网库；`characters_item_attr.AUTO_INCREMENT=1`
   是「从未插入」的强证据，但不排除人工造行。
4. **oracle `GetQryChar`** 读 `characters.Level`（`Query.cs:593`）服务于选角列表，不进入 `CharacterData`；
   本报告按「载入链」单列一行说明。
5. **Rust `crates/storage` 不含角色 SQL**：任务文本的「crates/storage 查询/写入」实际落在 `bins/db-srv`；
   本报告已按真实代码位置审计。

---

## 8. 计数与交付

- **矩阵行数**：49
- **缺口数**：20（G-01 … G-20）
- **R 级缺口**：13（G-01、G-02、G-03、G-04、G-05、G-06、G-07、G-08、G-09、G-10、G-11、G-19、G-20）
- **L/M 级缺口**：7（G-12 L、G-13 M、G-14 L、G-15 L、G-16 L、G-17 L、G-18 L）
- **Rust-only**：G-05 的命令写回部分、G-06（耐久同步）、G-09（Dir）、G-10（ServerIndex）、G-15（GamePoint latent）、G-19（IncHealing 落库）、G-20（兜底建角缺 ablity 行）
- **两侧同 bug（对称）**：G-01、G-02、G-03、G-04、G-05、G-07、G-08、G-11、G-12、G-13、G-16、G-17、G-18

### 源文件行号速查

- `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs`
  - `GetRecord` 153-190；`GetChrRecord` 192-266（字段 205-254，GameGold 同列 241）
  - `GetAbilGetRecord` 268-307（HP/AC 280-285）；`GetBonusAbilRecord` 309-341
  - `GetMagicRecord` 343-373；`GetItemRecord` 375-401；`GetBagItemRecord` 403-429
  - `GetStorageRecord` 431-458；`GetPlayerStatus` 460-483；`GetQryChar` 566-608
  - `QueryItemAttr` 610-638（错位在 628）
- `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs`
  - `SaveRecord` 45-80 / 82-137（SQL 85-88，参数 91-128；`Job = Job` 85；`@AllowGroupReCall`←AllowGroup 121）
  - `SaveAblity` 139-171（`Ac=Level` 146；HP/MP 151-152；MaxHP/MAxMP 153-154）
  - `ComparerUserItem` 173-201；`SaveItem` 206-279；`SaveBagItem` 284-355；`SaveStorageItem` 360-432
  - `SaveMagics` 434-464；`SaveBonusability` 466-491；`SaveQuest` 493-507（死代码）
  - `SaveStatus` 509-539；`UpdateItemAttr` 541-571；`ClearItemAttr` 626-649
- `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs`
  - `CreateCharacters` 90-155；`CreateAblity` 157-193；`CreateStatus` 195-212
  - `CreateUseItem` 214-264；`CreateBagItem` 266-312；`CreateStorageItem` 314-360；`CreateItemAttr` 362-390
- `src/GameSrv/Word/WorldServer.cs`
  - `ProcessHumans` 481-545；周期存档 698-703；断线支 713
  - `SaveHumanRcd` 1077-1092；`GetHumData` 1100-1229（头盔/项链交换 1167-1168）
  - `MakeSaveRcd` 1231-1382（ServerIndex 1236；Dir 1241；PKPoint 1265；AllowGroup 1276；AllowGroupReCall 1290；交换 1302-1303）
- `src/GameSrv/Services/PlayerDataService.cs`：`SaveCharacterData` 94-98；`SaveRcd` 100-112；`ProcessSaveQueue` 114-134；`LoadRcd` 166-181
- `src/GameSrv/Word/Threads/CharacterDataProcessor.cs`：`ProcessSaveStorage` 64-93；`LoadCharacterData` 149-182
- `src/DBSrv/Services/Impl/DataService.cs`：`ProcessServerMsg` 129-149；`LoadHumanRcd` 151-205；`SaveHumanRcd` 207-257
- `src/GameSrv/TimedService.cs`：230-243；`src/GameSrv/AppService.cs`：`SavePlayer` 133-145
- `src/M2Server/Player/PlayObject.Base.cs`：`ClientAdjustBonus` 1156-1180（`BonusPoint = nPoint` 1162；`BonusAbil.* += 0`）
- `src/OpenMir2/Enums/ItemLocation.cs`：Dress 0 / Weapon 1 / RighThand 2 / Necklace 3 / Helmet 4 / … / Charm 12

---

## 9. 实施状态（2026-10-19，卡 `openmir2-persist-single-sided-columns`）

> 落地口径 = **照抄 oracle**：单边（只读不写 / 只写不读 / 列映射错位）**保持单边并在此登记**；
> Rust-only 缺口（G-09/G-10/G-19）补齐。所有 oracle 行号为本次在 `27d22a9f3` 树上复测。

| 字段 | oracle 证据 | Rust 处理 | 判据 |
|---|---|---|---|
| `characters.AllowGroup` | 存档 UPDATE 清单无此列（`PlayDataStorage.Save.cs:85-88`）；载入 `Query.cs:245` | **保持单边（只读不写）**。`SAVE_CHARACTERS_SQL` 不含该列；`@AllowGroupReCall` 仍绑 `hd.AllowGroup`（`Save.cs:121`，错位照抄） | `playdata::single_sided_column_tests`：存档后 `AllowGroup` 保持原值 1、`AllowGroupReCall` = 0 |
| `characters.PkPoint` | 写 `Save.cs:107`；载入 `GetChrRecord`（`Query.cs:192-266`）无该列 | **保持单边（只写不读）** | 同上：落库 77、`get_record` 读回 0 |
| `characters.MarryCount` | 写 `Save.cs:120`；载入无该列 | **保持单边（只写不读）** | 同上：落库 3、读回 0 |
| `characters.Dir` | `MakeSaveRcd` `WorldServer.cs:1241` 写 `playObject.Dir`；载入 `Query.cs:213` | **补齐（G-09，Rust-only）**：进世界用记录 `dir` 初始化 `Entity.ai.dir`；`CM_TURN`/走跑成功同步实体；`build_save_record` 写 `rec.data.dir = e.dir()` | `save_record_carries_dir_server_index_and_inc_healing`、`move_cmd_dispatches_turn_walk_ack_and_aoi` |
| `characters.ServerIndex` | `MakeSaveRcd` `WorldServer.cs:1236` 写 `M2Share.ServerIndex`；配置 `ServerConf.cs:17` 的 `ReadWriteByte("Server","ServerIndex",0)`；载入不读 | **补齐（G-10，Rust-only）**：`config::load` 解析 `[Server] ServerIndex`（byte，缺省 0，非法报错）；`SessionGround.server_index` 固化；`build_save_record` 写入 | `server_index_parses_with_oracle_default`、`save_record_carries_…` |
| `characters_ablity.Ac` / `Hp` / `Mp` | `SaveAblity` `Save.cs:146`：`@Ac` 绑 `Abil.Level`；`:151-152` 只写 `Abil.HP/MP` 低 16 位；载入 `Query.cs:280-285` 用 `LoWord/HiWord` | **照抄保留（列映射错位）**：`save_ablity` 的 `Ac = Level`、`Hp/Mp` 只写低字；不在本批“修 bug” | 同上：`Ac` 列 = 7（Level），`Hp` 列 = `0x1234`，载入 `AC = HiWord = 0` |
| `characters.IncHealing` | 存档 `WorldServer.cs:1281` `(byte)playObject.IncHealing`；登录恢复 `WorldServer.cs:1151`；运行期累加 `PlayObject.Message.cs:1619-1628` | **补齐（G-19，Rust-only）**：会话初始化 `inc_healing: u16::from(d.inc_healing)`；`build_save_record` 写 `(sg.recovery.inc_healing & 0xFF)` | `save_record_carries_…`（300 ⇒ 44）、单边列往返（44） |

**未在本卡修、保持原样的 oracle 既有行为**（不属于“单边”，另行登记）：
`characters_item_attr` 无 INSERT 路径（G-01，已由 `71f4baa48` 建角门照抄）、
`characters_bonusability` 无建行（G-03，复核证伪：oracle 同样不建）、
`GameGold` 与 `Gold` 共列（G-02，`71f4baa48` 登记）。
