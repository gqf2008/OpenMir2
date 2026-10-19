# Lane B — DBSrv 持久化面审计（只读，可派工清单）

- **基线**：`8a621f6a8`（master，2026-10-20 快照）。所有 oracle/C# 行号均在主检出实测；Rust 行号同在 `8a621f6a8`。
- **方法**：只读 `sed/grep/cat`。未建 worktree、未改仓库、未连 MySQL、未起任何进程、未碰 `E:/MirServer`、未碰 `/e/tmp/wpl2`、`/e/tmp/wtimeout`。
- **范围**：DBSrv 取档/存档/删除链路（`QueryChr`/`GetQryChar`/`LoadHumanRcd`/`SaveHumanRcd`/`NewChrData`/`DeleteChr`/`SelectChr`、会话校验、错误应答），含 `characters` 9 张表与 `characters_indexes`。
- **排序**：按「数据丢失/不一致风险 × 生产可达」降序。`tier` 按 `AGENTS.md`「判据分级」；错误路径判不准 ⇒ 一律取 R（保守），所需证据是**故障注入 + 两侧同帧**，不是全量真机窗口。
- **纪律**：B-116/B-117、G-01…G-20、S4B-1…10、A-10、D-12、T-27、M1-EXIT-2 等已登记项一律**引用、不重复当新发现**（见 §3）。

---

## 1. 可派工卡（10 条）

### LB-01（P1，R）6000 `DB_LOADHUMANRCD` 的 DB 故障路径无应答 —— 取档永久挂起

- **oracle**：`src/DBSrv/Services/Impl/DataService.cs:163`（`CheckSessionLoadRcd`）→ `:169-186`（`Index`/`Get`，`Get` 失败置 `nCheckCode=-2`）→ `:189-204`。关键：应答分支是 `if ((nCheckCode == 1) || boFoundSession)`，而 `boFoundSession` 是**粘性真**；因此「会话已找到 + 读库失败」时 oracle 回的是 **成功帧**：`DBR_LOADHUMANRCD(1100) recog=1 series=1` + `LoadPlayerDataPacket{ChrName, HumDataInfo=null}`，**不是** -2。
  `PlayDataStorage.Query.cs:93-120`（`Get`）、`:153-190`（`GetRecord`，`Open` 失败返回 false）。
- **Rust**：`bins/db-srv/src/data.rs:280`（`else if let Some(v) = db.play_data_get(&chr_name).await?`）→ `?`；`Db::play_data_get`（`playdata.rs`，`pool.acquire().await.context("取连接失败")?`）只在**取连接**失败时返回 Err（查询失败已被吞成 `Ok(None)`，与 oracle 同）；Err 冒到 `data.rs:143`（`Err(e) => log::error(...)`）⇒ **零回包**、连接继续。
- **缺口**：oracle 在连接池/MySQL 不可用时仍发「成功 + 空体」帧；Rust 不发任何帧。GameSvr 的取档请求无应答，玩家卡在进图/读档。
- **可达性**：DBSrv 连接池 `max_connections(8)` 被 6000/5100/5700 并发占满 + MySQL 瞬断/重启时，第 9 个取档请求 acquire 失败。生产可达（DB 重启窗、保存风暴 + 取档并发）。
- **tier**：**R**（持久化面 + 需对真 MySQL 做故障注入；不是纯逻辑分支）。
- **判据草案（含牙）**：`#[ignore]` 进程级用例 `load_db_acquire_failure_answers_success_null_body`
  1. 起 rust db-srv + 伪 LoginSrv（会话已到位）；
  2. 注入 `db.pool.close()`（或停 MySQL 使新连接被拒）；
  3. 发 `DB_LOADHUMANRCD`；
  4. **必须**在 5s 内收到 `1100 recog=1 series=1`，且 `LoadPlayerDataPacket.hum_data_info == None`（oracle 同场景金标准帧，先在 C# DBSrv 抓一次再写进断言）；
  5. 反向子断言：会话未命中时仍回 `1100 recog=-1`（不得把失败帧也改成成功帧）。
  **牙**：把 `data.rs:280` 的 `?` 改为 `match { Ok(Some(v)) => Some(v), Ok(None) => { n_check_code=-2; None }, Err(e) => { log; None } }` 后绿；退回 `?` 即红。

### LB-02（P1，R）`CM_QUERYCHR` 的 DB 故障路径无应答 —— 角色列表卡死

- **oracle**：`src/DBSrv/Services/Impl/UserService.cs:439-500`；`PlayRecordStorage.cs:126-171`（`Get`→`GetRecord`：`Open` 失败返回 `default` 且 `success=false`）、`:196-208`（`FindByAccount`）、`:210-228`（`ChrCountOfAccount`）；`PlayDataStorage.Query.cs:566-608`（`GetQryChar` 内部 `catch`→false）。
- **Rust**：`bins/db-srv/src/handler.rs:283-324`；具体 `:290`（`db.record_get_by(q.index).await?`）、`:310`（`db.get_query_char(index).await?`）。
- **缺口**：MySQL 不可用（连接被拒）时，oracle 仍从内存 `_quickIdList` 拿到账号角色，`GetBy` 每条失败即跳过，最后照常发 `SM_QUERYCHR(520) recog=nChrCount`（通常 0）；`GetQryChar` 的 SQL 失败也被 catch 成「跳过该角色」。Rust 两处 `?` → `queue_loop`（`handler.rs:46-57`）只记日志 ⇒ **客户端无回包**，角色列表界面永久转圈。
- **可达性**：MySQL 重启/主从切换/连接数打满；每次登录必经。
- **tier**：**R**（故障注入 + 帧级对拍）。
- **判据草案（含牙）**：`querychr_db_down_answers_empty_list`
  1. 起 db-srv + 伪 LoginSrv；2. 建 1 个角色后 `pool.close()`；3. 发 `CM_QUERYCHR`；
  4. 断言收到 `SM_QUERYCHR(520)`，头部与空体逐字节等于 C# DBSrv 同故障下抓到的金标准（`recog=0`，尾为空）；
  5. 两条子断言分别把 `:290` / `:310` 的 Err 改成 `continue`（oracle 的 skip 语义），**只改一处必须仍红**。
  **牙**：任一 `?` 回退即超时无帧；把 Err 映射成 `SM_QUERYCHR_FAIL` 也红（oracle 是成功 ident + 0 个角色，不是 527）。

### LB-03（P1，R）`CM_DELCHR` 的 DB 故障路径无应答 —— 删角无反馈

- **oracle**：`UserService.cs:528-560`；`PlayRecordStorage.cs:126-171`（`Get`：`Open` 失败 → `default` + `success=false`）、`:382-397`（`Update`→`UpdateRecord` 内部 catch→false）；`UserService.cs:502-526`（`DelChrSnameToLevel`→`GetQryChar`）。
- **Rust**：`handler.rs:448-483`；`:458`（`record_get_by`）、`:461`（`del_chr_sname_to_level`→`:490` `get_query_char`）、`:465`（`record_update`）。
- **缺口**：oracle `boCheck` 保持 false ⇒ 回 `SM_DELCHR_FAIL(524)`；Rust 任一 `?` ⇒ 无回包。
- **可达性**：同 LB-02；删角是低频但玩家会重复点击/重连。
- **tier**：**R**。
- **判据草案（含牙）**：`delchr_db_down_answers_fail`：停 MySQL 后发 `CM_DELCHR("名")` ⇒ 断言 `SM_DELCHR_FAIL(524) recog=0`；随后恢复 MySQL、同一连接再发一次 ⇒ 必须成功（证明连接未被错误关闭）。**牙**：把 `record_update` 的 Err 当成 `boCheck=true` 即红（会回成功但没有 `IsDeleted=1`）。

### LB-04（P1，R）`CM_NEWCHR` 的 DB 故障路径无应答 —— 建角无反馈

- **oracle**：`UserService.cs:565-661`；`:636-639`（`Add` 失败 ⇒ `nCode=2`）、`:210-228`（`ChrCountOfAccount`，`GetBy` 失败只影响计数）；`PlayRecordStorage.cs:230-265`（`Add`→`UpdateRecord` catch→false）。
- **Rust**：`handler.rs:363-445`；`:389`（`db.chr_count_of_account(...)?`）、`:395`（`db.record_add(...)?`）。
- **缺口**：MySQL 不可用 ⇒ oracle 计数 0、`Add` 失败 ⇒ `SM_NEWCHR_FAIL(522) recog=2`；Rust 两处 `?` ⇒ 无回包。另：Rust `record_add` 失败时内存索引未被污染（正确），但玩家看不到任何结果。
- **可达性**：建角时 DB 瞬断；新建号高峰期。
- **tier**：**R**。
- **判据草案（含牙）**：`newchr_db_down_answers_fail2`：停 MySQL → `CM_NEWCHR` ⇒ 断言 `SM_NEWCHR_FAIL(522) recog=2`（**不是** 0/3/4，也不是无帧）；恢复后同名可重建（`record_add` 未 INSERT ⇒ 内存里不得有该名字）。**牙**：把 Err 映射成 0 或 4 即红；把 `record_add` 的 Err 当成功即红（会出现「回成功但 `characters_indexes` 无行」）。

### LB-05（P2，R）`CM_SELCHR` 的 `SelectID` 写失败无应答 —— 可登录被卡住

- **oracle**：`UserService.cs:668-735`（`UpdateBy` `PlayRecordStorage.cs:399-402`→`UpdateRecord` catch **吞**）；`PlayDataStorage.Query.cs:93-120`→`:192-266`（`Query` 失败回 null，随后 `chrRecord.ChrName` NRE 被消息线程 catch ⇒ 无回包）。
- **Rust**：`handler.rs:497-562`；`:515`/`:518`（`db.record_update_by(&rec).await?`）。
- **缺口**：只有「`characters_indexes` 的 UPDATE 失败、`characters` 的 SELECT 正常」这一格是差异：oracle 吞掉写失败、继续回 `SM_STARTPLAY(525)`；Rust `?` ⇒ 无回包。（若 `characters` 读也失败，oracle 同样 NRE 无回包，不是差异。）
- **可达性**：中低。写权限被收回 / 表被设只读 / 行锁超时 / 主从只读副本；但发生时玩家完全进不了游戏。
- **tier**：**R**（时序/故障注入；涉及选角）。
- **判据草案（含牙）**：`selchr_selectid_write_failure_still_startplay`：测试库给 `characters_indexes` 挂 `BEFORE UPDATE ... SIGNAL SQLSTATE '45000'` 触发器（SELECT 不受影响）→ 发 `CM_SELCHR` ⇒ 断言收到 `SM_STARTPLAY(525)` 且路由体与正常路径逐字节相同。**牙**：把 `:515/:518` 的 `?` 改成 `if let Err(e) = ... { log }` 即绿；反向子断言「`characters` 读失败仍无回包」必须保持红→绿切换正确。

### LB-06（P1，R）存档体「空子对象」被 Rust 写 0，oracle 不写 —— 数值清零（数据丢失）

- **oracle**（四处都在各自 `try` 内，null ⇒ NRE ⇒ catch 吞 ⇒ **该表一行不改**）：
  `PlayDataStorage.Save.cs:139-171`（`SaveAblity` 读 `Abil.Level`）、`:466-491`（`SaveBonusability` 读 `bonusAbil.AC`）、`:509-539`（`SaveStatus` 读 `statusTimeArr[0]`）、`:541-571`（`UpdateItemAttr` 读 `userItem.Desc[0]`）。
- **Rust**：
  `bins/db-srv/src/playdata.rs:434`+`:775`（`d.abil.clone().unwrap_or_default()` → `save_ablity` 写 0）、`:822`（`bonus_abil...unwrap_or_default()` → 写 0）、`:833`（`status_time_arr...unwrap_or(&[])` → `:741-760` `get(i).unwrap_or(0)` 写满 15 个 0）、`:603-623`（`desc...unwrap_or_else(|| vec![0;14])` → 把 `VALUE0..13` 全写 0）。
- **缺口**：同一份缺子对象的存档体，oracle 保持库值，Rust 把等级/HP/MP/AC/DC/MC/SC/经验/负重、奖励属性、15 个状态位、物品附加属性**全部清零**。
- **可达性**：正常 C#/Rust GameSvr 都填满这些字段；**版本混跑**（GameSvr 与 DBSrv 各自升级/回滚、MemoryPack 成员数不一致）或畸形存档体时可达。判不准 ⇒ R。
- **tier**：**R**（落库字段级）。
- **判据草案（含牙）**：`save_partial_body_preserves_rows`：夹具角色把 4 张表种成非零 → 发 `Abil=None / BonusAbil=None / StatusTimeArr=None / 物品 Desc=None` 的 `DB_SAVEHUMANRCD` ⇒ 断言 `characters_ablity`/`characters_bonusability`/`characters_status`/`characters_item_attr` 逐列与存档前完全相同（oracle 金标准）。**牙**：四个子断言各自独立；只修三处，第四处必红。

### LB-07（P2，R）建角附属空槽 INSERT 失败：Rust 提交半套槽位，oracle 整次回滚

- **oracle**：`PlayDataStorage.Add.cs:51-88`（`AddRecord` 的 `catch`→`RollBack`（no-op）→`Dispose` 关连接回滚 + 返回 false）；关键在 `CreateUseItem` `:214-264` 与 `CreateBagItem` `:266-312` 的 INSERT 循环**在 try 之外**（异常冒到 `AddRecord` ⇒ 整次建角回滚，`characters` 也不留行）；`CreateStorageItem` `:314-360` 的循环**在 try 内**（吞错、继续）。
- **Rust**：`bins/db-srv/src/db.rs:528-556`（`data_add`：三张表逐行 `match { Ok=>push, Err(e)=>{ log; break } }`，之后照常 `tx.commit()`）、`bins/db-srv/src/playdata.rs:977-1010`（`create_empty_items` 同形）。
- **缺口**：`characters_item`/`characters_bagitem` 插入失败时，oracle 不产生角色行（5100 侧只剩索引孤儿 + `SM_NEWCHR_SUCCESS`；6000 侧回 `1100 recog=0`），Rust 产生「有角色行、槽位表缺行」；而 `comparer_user_item` 对 `old[i]==None` 直接 `continue`（G-13 已登记）⇒ 该角色**此后任何物品都存不进去**。
- **可达性**：低（需 INSERT 失败：磁盘满/表缺失/锁超时/连接被杀）。
- **tier**：**R**（持久化一致性）。
- **判据草案（含牙）**：`create_item_insert_failure_rolls_back_whole_character`：测试库给 `characters_item` 的第 5 条 INSERT 挂 `SIGNAL` 触发器 → 走 `CM_NEWCHR` 与 6000 自动建档两条路径 ⇒ 断言 `characters` 0 行、`characters_indexes` 残留与 oracle 相同。**牙**：把 item/bag 两表的 `Err=>break` 改成 `Err=>return Err` 即绿；`characters_storageitem` 必须保持「吞错并提交」（反向子断言，改错即红）。

### LB-08（P2，R）全局 `Mutex<Db>` 串行化：一次慢存档阻塞全部取档/选角

- **oracle**：`PlayDataStorage`/`PlayRecordStorage` 每个方法 new 独立 `StorageContext`（`DBContext.cs:21-35` 每方法一条连接）；TouchSocket 回调并发，无全局锁。
- **Rust**：`handler.rs:52`（5100 队列 worker 每消息持锁）、`data.rs:276`（取档持锁）、`data.rs:367`（存档持锁）、`data.rs:522-536`（缓存回写持锁）、`market.rs`（5700 同样持锁）共用 `Arc<Mutex<Db>>`；`save_record`（`playdata.rs:763`）的整个 8 段事务都在锁内。
- **缺口**：一次慢存档（行锁等待 `innodb_lock_wait_timeout`、DB 抖动、保存风暴）期间，`CM_QUERYCHR`/`CM_SELCHR`/6000 取档**全部排队**；oracle 只受各自连接与行锁影响。表现为登录超时/重试风暴（客户端侧可观测）。
- **可达性**：中（多人同时下线、DB 慢查询、批量存档）。
- **tier**：**R**（时序）。
- **判据草案（含牙）**：`slow_save_does_not_block_login`：制造一次 3s 的存档（触发器 `DO SLEEP(3)` 或持行锁），同时发 `CM_QUERYCHR` ⇒ 断言取档回包延迟 < 500ms（oracle 同场景基线）。**牙**：把 6000 保存改为 per-call 连接、不持全局锁即绿；退回全局锁即红。若 owner 裁决「单区可接受」，必须登记 `D-*` 并把判据降为延迟采样，**不得**当作已等价。

### LB-09（P3，L）启动索引重名：Rust 静默覆盖，oracle 启动失败

- **oracle**：`PlayRecordStorage.cs:67-78`（`_quickList.Add(name, id)`；`Dictionary.Add` 重名抛 `ArgumentException`；`LoadQuickList` 无 catch ⇒ 冒到宿主启动）。
- **Rust**：`bins/db-srv/src/db.rs:203`（`self.quick.insert(key, id)` **静默覆盖**）；对比 `db.rs:224-228` 的 `characters` 侧反而 `bail!`（两侧口径不一致）。
- **缺口**：`characters_indexes` 存在两条 `IsDeleted<2` 同名行（无唯一索引；历史脏数据/双写）时，oracle 拒绝启动，Rust 启动成功：「最后一行覆盖索引」但 `by_account` 同时保留两条 ⇒ `CM_QUERYCHR` 会列出两个同名角色。
- **可达性**：低（需脏索引行）；一旦出现，oracle 是「停服」，Rust 是「带病运行」。
- **tier**：**L**（启动期纯逻辑；不写库）。
- **判据草案（含牙）**：`duplicate_index_name_fails_startup`：测试库插两条同 `ChrName`、`IsDeleted=0` 的 `characters_indexes` → 起 db-srv ⇒ 断言退出码 1 且末行含「重复」（与 `characters` 的重名路径同一措辞）。**牙**：把 `insert` 改为 `if quick.insert(...).is_some() { bail! }` 即绿。

### LB-10（P2，R）持久化面真机验收缺口（证据卡，不是代码差异）

- **现状（Rust 行）**：`tests/live_data.rs:346` 是唯一 6000 取档/存档进程级用例，落库断言只覆盖 `characters`/`characters_ablity`/`characters_status`/`characters_magic`/`characters_item` 五表；`tests/live_querychr.rs:457/474/502` 只覆盖 4 条 5100 命令的**正常/会话失败**回包；`tests/quest_no_persist.rs` 只钉 `characters_quest` 零引用。`characters_bagitem`/`characters_storageitem`/`characters_bonusability`/`characters_item_attr` 的落库、`CM_NEWCHR`/`CM_DELCHR`/`CM_SELCHR` 的**库后态**（`characters_indexes.SelectID`/`IsDeleted`）没有跨实现逐字段 diff；M3 §6 门禁仍有一处 `characters_ablity` RED（已有承卡 `openmir2-m3-scoped-diff-ability`，见 `tests/parity/M3-report.md:306`）。
- **oracle**：`WorldServer.MakeSaveRcd` 的字段全集（`src/GameSrv/Word/WorldServer.cs:1231-1382`）与 `DBSrv` 两侧入口（`DataService.cs:151-257`、`UserService.cs:439-735`）。
- **缺口**：R 级判据没有可复算的「9 表 + indexes」证据；错误路径（LB-01…LB-07）也没有任何真机/故障注入记录。
- **可达性**：每次登录/存档。
- **tier**：**R**。
- **判据草案（含牙）**：
  ① `scoped_diff.py` 扩到 9 表 + `characters_indexes`，跑「建角 → 穿脱 → 仓库存取 → 软关/断线」两侧同序列，逐字段 diff=0（`SelectID`/`IsDeleted`/`ModifyDate` 按 oracle 口径列白名单）；
  ② 三条进程级用例分别断言 `CM_NEWCHR`/`CM_DELCHR`/`CM_SELCHR` 的回包字节 + 三张表后态；
  ③ 变异牙：把 `save_item_table` 的 `Position` 绑定错 1 位、把 `IsDeleted` 写 0，①或②必须红。
  当前缺 ①②③，本卡即「证据缺口」本身。

---

## 2. 已核对为逐条等价（本轮复核，不派卡）

- **存档 SQL 列/顺序/绑定**：`Save.cs:82-137`（含 `Job = Job`、`@AllowGroupReCall←hd.AllowGroup`、`Deleted=0`）↔ `playdata.rs:419-480`；`SaveAblity`（`Ac←Level`）↔ `:484-517`；`SaveBonusability` ↔ `:715-740`；`SaveStatus` 15 槽 ↔ `:741-761`；`SaveMagics` 先 DELETE 后逐条 INSERT ↔ `:687-713`。
- **物品差量算法**：`ComparerUserItem`（`Save.cs:173-201`）↔ `comparer_user_item`（`playdata.rs`）逐分支一致（含 `old==null ⇒ continue` 的 G-13）。
- **取档字段集**：`GetChrRecord`（`Query.cs:192-266`）↔ `get_chr_record`（`playdata.rs:80-137`）；七张附属表（`:268-483`）↔ `get_abil_record`/`get_bonus_abil_record`/`get_magic_record`/`get_item_slots`/`get_player_status`。
- **事务边界**：单事务 8 个 `Save*`、逐项吞错、只有 `SaveMagics` 的 DELETE 与 Commit 失败才回滚（`Save.cs:45-80`）↔ `save_record`（`playdata.rs:763-853`）；oracle `RollBack()` 条件写反（`DBContext.cs:67-73`）已登记 G8。
- **会话校验**：`CheckSessionLoadRcd` 只比账号 + 会话号、IP 入参被忽略（`ClientSession.cs:135-157`）↔ `loginsrv.rs:122-136`；`LoadRcd` 一次性语义一致。
- **失败应答标识**：`SaveHumanRcd` 成功 `1102 recog=1` / 失败 `1100 recog=0`，且与 `Update` 是否成功无关（`DataService.cs:234-250`）↔ `save_reply`/`store_human_rcd`（`data.rs:315-390`，G2/G3 已修并有单测）。
- **缓存**：`CacheStorageService.Add` 只在键已存在时替换（`CacheStorageService.cs:12-18`）↔ `CacheStore::add`（`cache.rs:26-31`，单测钉住）；5 分钟回写两条日志一致（`TimedService.cs:73-96` ↔ `data.rs:506-537`）。
- **索引表 autocommit**：`PlayRecordStorage` 全部无事务、`IsDeleted` 0/1/2 语义 ↔ `db.rs::record_*`（本轮逐列核对 `SelectID` 截断到 8 位、`IsDeleted != 0` 判真、`_indexQuickList` 存在性守卫，均一致）。

## 3. 已登记项（引用，不重复）

| 编号 | 内容 | 位置 |
|---|---|---|
| B-116 | `GameGold` 与 `Gold` 共用 `characters.Gold` 列，元宝重登回档 | `tests/parity/whitelist.md:52` |
| B-117 | `characters_item_attr` 无 INSERT + `QueryItemAttr` 行序双索引/逗号串单参数 | `whitelist.md:53` |
| G-01/G-02/G-03 | item_attr 无 INSERT / GameGold 无列 / bonusability 无建行（G-03 复核证伪：oracle 同样不建） | `tests/parity/m3/audit-persistence-loadsave-20261018.md:315-335` |
| G-04…G-08、G-11…G-20 | quest 未持久化、AllowGroup 列错位、耐久、PkPoint/MarryCount 单边、Dir/ServerIndex/IncHealing、magic 槽序、物品缺行、死代码、缓存空转、兜底建角缺 ablity 行 | 同上 `:336-435`、`:490-527` |
| S4B-1…S4B-10 | 附属表读异常传播、QueryItemAttr 空槽、Sign 长度、缓存键折叠、拍卖行空桩 | `whitelist.md:397-410` |
| A-10 | DBSrv 启动失败/配置写回两处授权偏离 | `whitelist.md:68` |
| D-12 / T-27 / M1-EXIT-2 | 关服存档增强、game-svr 存档已接线、oracle 关服不 flush | `whitelist.md:26`、`:213`、`:412` |
| G2/G3/G6/G7 | 旧审计的 4 条 Rust-only 缺陷 | **已在 `8a621f6a8` 修复**（`data.rs` 吞 Err 回包、`play_data_add` 写 `name_quick`、`get_item_slots` 流式吞错）；旧审计文本已过期，派卡时勿重开 |
| `openmir2-m3-scoped-diff-ability` | M3 门禁 `characters_ablity` 4 行 RED（跨侧字段 diff） | `tests/parity/M3-report.md:306` |

## 4. 限制与未覆盖

- 只读审计：未连 MySQL、未起进程、未做故障注入；上述「判据草案」是待执行方案，不是已完成证据。
- 未审 5700 拍卖行的存储语义（S4B-5…10 已覆盖，且部署未加载 `MarketSystem.dll` ⇒ 不可达）。
- 未审 `characters_indexes` 的 `PlayQuickList` 二分查找怪行为（`PlayQuickList.cs:38-155`）：单区/账号大小写一致时与 Rust `HashMap` 等价；仅在大小写不同且多账号时可能出现 oracle 漏查，当前不可达。
- 未审 DBSrv 之外（game-svr 侧 `build_save_record` 的字段全集），G-19/G-20 与 T-28 已覆盖。


---

## 评审员更正（2026-10-20，**以本节为准**）

- **LB-01/02/03/04/05/09 主要差异成立**；**LB-06 有实质 oracle 事实错误，需重写**；LB-04/LB-07/LB-08 与已登记项（G4/G10）重叠需改写；LB-10 作为证据卡成立。
- **「连接池被并发占满」不可达**：6000/5100/5700/缓存回写共用同一个 `Arc<tokio::sync::Mutex<Db>>`，任一时刻最多 1 条连接，
  max_connections(8) 永远到不了。真实触发是 **MySQL 停机/重启**（sqlx `test_before_acquire` + 建连失败 ⇒ `acquire_timeout` 后 Err）。
  且 acquire 等待发生在持锁期间，会串行卡住 6000/5100/5700（交叉引用 LB-08）。
- **LB-02 语义收窄**：oracle 的 `success` 是粘性的（`GetRecord` 只在 `dr.Read()` 时置真、从不重置）
  ⇒「每条失败即跳过」只在**全部** GetBy 失败时成立；先成功后失败会 `humRecord=null` 后 NRE ⇒ **不回包**（与 Rust 同形）。
  停库时每角色付一次连接超时（默认 15s），测试读超时要给足。
- **5100 卡判据缺协议前置**：`CM_DELCHR` 受 1000ms 限频与 `CheckSession` 门，且 `sAccount` 只在 QueryChr 成功时填
  ⇒ 每条判据必须「同 socket 先 `CM_QUERYCHR` 成功 → 睡 >1000ms → 再发目标命令」；`DeleteMinLevel` 默认 30，测试别配 0。
- **tier 表述**：R 级 = 占窗口（故障注入子集），不要写成「不是全量真机窗口」。
