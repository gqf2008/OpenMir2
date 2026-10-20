# DBSrv 连接模型与持久化证据卡设计（2026-10-21）

- **车道**：`dbdesign`（只读设计，tier=L）
- **基线**：`e9c1e865470ed9988b5fa3cedb1cbfa0307f9cde`（master）
- **输入**：`mir2-rs/tests/parity/m3/audit-dbsrv-persistence-20261020.md`（含文末「评审员更正」，以该节为准）；oracle `src/DBSrv/**`、`src/Storeages/DBSvr.Storage.MySQL/**`、`src/GameSrv/DB/**`（只读）。
- **方法**：只读 `grep/sed/nl`；未建库、未连 MySQL、未起 DBSrv/GameSvr/客户端/代理、未占端口、未改主检出。Rust 行号在 `e9c1e8654` 实测；oracle 行号在同树实测。
- **边界**：本文只定义设计与证据卡，不改代码。实现卡与窗口卡另立，tier=R。

---

## 0. 结论摘要

1. **oracle 没有应用层全局锁**。DBSrv 三个监听面（5100/6000/5700）各自独立：
   - **5100 `UserService`**：TouchSocket 回调只做接入/会话维护；`Data` 类型进 `Channel<UserGateMessage>`，由**唯一消息线程**串行消费，四个角色命令因此有**全 5100 面全序**（`UserService.cs:30`、`:76-96`、`:218-231`）。
   - **6000 `DataService`**：`Received` 回调里**同步**完成 `ProcessMessagePacket` → `LoadHumanRcd`/`SaveHumanRcd`，没有队列（`DataService.cs:53-68`、`:88`、`:134-141`）。同一连接内按回调顺序串行，跨连接并发。
   - **5700 `MarketService`**：同样是 `Received` 里直接处理，无队列（`MarketService.cs:71-95`）。
   - 存储层每个方法 `new StorageContext` / `new MySqlConnection`，**没有跨方法共享连接**，也没有显式 `lock/Monitor/Semaphore`；并发由 MySqlConnector 连接池（缺省 `MaximumPoolSize=100`、`ConnectionTimeout=15s`）与 InnoDB 行锁/元数据锁决定（`DBContext.cs:21-35`；`PlayDataStorage.Query.cs:37/131/160/573`；`PlayDataStorage.Save.cs:47/576`；`PlayDataStorage.Add.cs:53`）。
2. **Rust 现状是一把 `Arc<tokio::sync::Mutex<Db>>` 贯穿 5100/6000/5700/缓存回写**：`main.rs:79` 创建；`handler.rs:52`（5100 队列 worker 每条消息持锁到 `process_data` 完成）、`data.rs:277`（6000 取档）、`data.rs:380`（6000 存档，含整段 8 表事务）、`data.rs:540-545`（5 分钟缓存回写逐条持锁）、`market.rs:439/495/522`（5700 各存储调用）。连接池又只开 `max_connections(8)`（`db.rs:147-149`）。
3. **行为后果（LB-08 仍成立）**：一次慢存档（InnoDB 行锁等待、`innodb_lock_wait_timeout`、网络抖动、Commit 卡住）在持有全局锁期间，会阻塞：
   - 5100 的 `CM_QUERYCHR` / `CM_SELCHR` / `CM_NEWCHR` / `CM_DELCHR`；
   - 6000 的 `DB_LOADHUMANRCD` 与后续 `DB_SAVEHUMANRCD`；
   - 5700 的拍卖行查询/保存；
   - 缓存回写。
   **不是连接池耗尽**：全局锁下任一时刻最多 1 个 `Db` 使用者，`max_connections(8)` 永远到不了（评审员更正已明确）。真实差异是**队头阻塞**：oracle 的 QUERYCHR/取档走独立连接，不会被另一次慢存档按应用层串行化。
4. **候选方案**：推荐 **方案 A（保留每连接 worker + 共享连接池 + 内存索引短锁 + 建角闸门）**。方案 B（单写线程 + 读池）在朴素形态下仍会把 `CM_SELCHR` 的 `SelectID` 写、`CM_NEWCHR`/`CM_DELCHR` 和 5700 写排在慢存档之后，不能单独作为 LB-08 的收口方案；若选 B，必须细化为「角色表写 lane + 索引表写 lane」并加每连接序号/读写屏障，复杂度高于 A。
5. **LB-10 证据卡**：本文给出 `W-DB-1..7` 窗口项、`C1..C8` 判据、`M1..M8` 带牙变异和采集 schema；核心是「帧序 + DB 语句序 + 表后态 + 时间戳」四件套，全部 R 级、占窗口。
6. **已落地与本文关系**：`LB-01/02/03/05/06/07/09` 已在 `d24d24ace`、`de71cc01d`、`e50df6231` 等提交中有实现与带牙测试；本文不重开这些卡。`LB-04` 与已登记 **G4** 重叠、`LB-07` 应并入 G4 的「建角失败语义」、`LB-08` 的「并发序」部分与 **G10** 重叠；**LB-08 的「慢存档阻塞登录」延迟面仍是未登记的独立差异**，本文按新卡处理。

---

## 1. 输入校正（必须先看）

### 1.1 评审员更正（2026-10-20）中与本文相关的四条

1. **「连接池被并发占满」不可达**：6000/5100/5700/缓存回写共用同一把 `Arc<tokio::sync::Mutex<Db>>`，任一时刻最多 1 条 `Db` 使用者，`max_connections(8)` 永远到不了。真实触发是 **MySQL 停机/重启**（sqlx `test_before_acquire` + 建连失败 ⇒ `acquire_timeout` 后 Err），且 acquire 等待发生在持锁期间，会把 6000/5100/5700 一起串行卡住。
2. **LB-02 语义收窄**：oracle `GetRecord` 的 `success` 是粘性的（只在 `dr.Read()` 时置真、从不重置），所以「每条失败即跳过」只在**全部** `GetBy` 失败时成立；先成功后失败会 `humRecord=null` 后 NRE ⇒ 不回包。
3. **5100 卡判据缺协议前置**：`CM_DELCHR` 受 1000ms 限频与 `CheckSession` 门，且 `sAccount` 只在 `QueryChr` 成功时填 ⇒ 每条判据必须「同 socket 先 `CM_QUERYCHR` 成功 → 睡 >1000ms → 再发目标命令」；`DeleteMinLevel` 默认 30，测试别配 0。
4. **tier 表述**：R 级 = 占窗口（故障注入子集），不要写成「不是全量真机窗口」。

### 1.2 LB-06 原文的 oracle 事实错误（本文不沿用）

原文说「四处空子对象都只跳过各自表」不成立。按 oracle 实测：

| 缺失字段 | oracle 实际路径 | 结果 |
|---|---|---|
| `Abil == null` | `SaveRecord` 调 `SaveAblity(context, …, humanRcd.Data.Abil)`（`Save.cs:59`）；`SaveAblity` 的参数绑定 `Abil.Level` 在**自己的 try 之前**（`Save.cs:139-162`，try 从 `:163` 开始）⇒ NRE 冒到外层 `SaveRecord` catch（`Save.cs:69-74`） | **整次存档回滚**（8 表一行不改；实际回滚靠 `Dispose()` 关连接，`DBContext.cs:75-82`） |
| `HumItems == null` | `SaveItem` 第一行 `int useSize = userItems.Length;` 在 try 之外（`Save.cs:206-208`）⇒ NRE 冒到外层 | **整次存档回滚** |
| `BonusAbil == null` | `SaveBonusability` 的绑定全在 try 内（`Save.cs:466-489`） | 只跳过 `characters_bonusability`，其余照常提交 |
| `StatusTimeArr == null` | `SaveStatus` 的 `statusTimeArr[0..14]` 全在 try 内（`Save.cs:509-539`） | 只跳过 `characters_status` |
| 物品 `Desc == null` | `UpdateItemAttr` 的 `userItem.Desc[0]` 在 try 内（`Save.cs:541-571`），且被 `SaveItem` 的内层 try 包住（`Save.cs:259-270`） | 只跳过 `characters_item_attr` |

当前 master 的 `playdata.rs:788-804` 已按上表在事务开头拦截 `Abil`/`HumItems` 缺失并返回 `Ok(false)`；`playdata.rs:853-877` 对 Bonus/Status 分别走「跳过该表」。**后续派卡引用 LB-06 时必须引用本节，不得引用原文的四条并列断言。**

### 1.3 已登记项去重

| 审计条目 | 处置 |
|---|---|
| LB-04（`CM_NEWCHR` 故障路径） | 与 `audit-db-txn-consistency-20261018.md` 的 **G4**（建角失败语义、`NewChrData` 忽略 Add 返回值）重叠；并入 G4，不另开卡。 |
| LB-07（建角附属空槽 INSERT 失败） | 与 G4 同属「建角事务边界/失败语义」；并入 G4 的验收矩阵，保留「`characters_item`/`bagitem` 回滚、`storageitem` 吞错」两条反向子断言。 |
| LB-08（全局 Mutex 串行化） | 「并发序」部分 = **G10**（非契约，单区单 GameSvr 无差异）；「慢存档阻塞 QUERYCHR/SELCHR/取档」的**延迟面**不在 G10 内，本文作为独立 R 卡。 |
| LB-10（持久化证据缺口） | 成立；本文第 6 节给出证据卡定义。 |
| `openmir2-m3-scoped-diff-ability` | M3 门禁 `characters_ablity` RED 的既有承卡；W-DB-5 必须引用它，不重复开卡。 |

---

## 2. oracle 的 DBSrv 连接/线程模型（逐行）

### 2.1 进程与监听面

`Program.Main` → `AppServer.StartAsync`（`AppServer.cs:55-60`）→ `ServerHost` 宿主 → `AppService`：

- `StartAsync`：`_userService.Initialize()`、`_dataService.Initialize()`、`_marketService.Initialize()`（`AppService.cs:37-43`）。
- `StartedAsync`：`_userService.Start()`、`_dataService.Start()`、`_marketService.Start()`、`_ = _sessionService.Start()`（`AppService.cs:45-56`）。
- 三个 `TcpService` 各自 `Setup` 自己的监听地址与帧适配器（`DataService.cs:31-39`、`UserService.cs:38-45`、`MarketService.cs:29-38`）；`LoadQuickList` 在对应 `Start()` 里、监听开始后调用（`DataService.cs:41-45`、`UserService.cs:47-52`），启动期没有并发请求。

| 面 | 端口（缺省） | TouchSocket 事件 | 是否入队 | 处理线程 | 存储访问 |
|---|---|---|---|---|---|
| 5100 `UserService` | `GatePort=5100` | `Received` → `ProcessGateData` | `Data` → `_reviceQueue`；`Enter`/`Leave`/`KeepAlive` 就地 | **单消息线程** `StartMessageThread` | 四个角色命令，全部在消息线程 |
| 6000 `DataService` | `ServerPort=6000` | `Received` → `ProcessMessagePacket` → `ProcessServerMsg` | 否 | TouchSocket 回调线程（同连接串行、跨连接并发） | `LoadHumanRcd`/`SaveHumanRcd`/`SaveHumanRcdEx` |
| 5700 `MarketService` | `MarketServerPort=5700` | `Received` → `ProcessMessagePacket` → `ProcessMarketPacket` | 否 | TouchSocket 回调线程 | 拍卖行四个 ident |
| 5600 主动链 `ClientSession` | 连 LoginSrv | `LoginSocketRead` | 字符串缓冲，按 `)` 切帧 | TouchSocket 客户端回调线程 | 无（只维护 `_globaSessionList`） |
| 定时 `TimedService` | — | `PeriodicTimer(1s)` | 否 | 宿主后台任务 | 每 300s `ProcessCacheStorage` |

### 2.2 5100：唯一消息线程 = 5100 面全序

- `Channel.CreateUnbounded<UserGateMessage>()`（`UserService.cs:30`）。
- `Received`（`UserService.cs:140-160`）→ `ProcessGateData`（`:200`）：
  - `KeepAlive` 直接 `SendKeepAlivePacket`（`:211-213`）；
  - `Enter` 直接 `OpenUser`（`:214-222`）；
  - `Leave` 直接 `CloseUser`（`:223-225`）；
  - `Data` 只做 `_reviceQueue.Writer.TryWrite(userMessage)`（`:226-231`）。
- `StartMessageThread` 单消费者：`WaitToReadAsync` → `TryRead` → `ProcessMessage`（`UserService.cs:76-96`）。
- `ProcessMessage` 按 `gateInfo.UserList` 找 `SessionId`，累积 `userInfo.sText`，按 `#...!` 抠一帧，`DeCodeUserMsg`（`UserService.cs:102-134`）。
- `DeCodeUserMsg` 四命令：`CM_QUERYCHR` 200ms 限频、`CM_NEWCHR`/`CM_DELCHR` 1000ms 限频、`CM_SELCHR` 需先 `boChrQueryed`；`NEWCHR`/`DELCHR`/`SELCHR` 还要 `CheckSession`（`UserService.cs:346-437`）。
- **串行化点**：所有 5100 数据库工作（`QueryChr`/`NewChr`/`DeleteChr`/`SelectChr`）都在这一条消息线程上，因此 5100 面内部是**全序**；一条慢 `QueryChr` 会阻塞同一 DBSrv 上所有 SelGate 的后续 5100 命令。这是 oracle 的既有行为，**不要**在 Rust 侧为了并发把它拆成每连接队列。
- **无锁点**：`_globaSessionList` 被 5600 回调线程写、被 5100/6000 读，`List<GlobaSessionInfo>` 无锁（`ClientSession.cs:13`、`:135-190`）；`_gateClients`/`UserList` 也被回调线程与消息线程并发读写（`UserService.cs:200-235`、`:102-134`）。这是 oracle 的数据竞争，不是可依赖的契约（G10 相邻问题）。

### 2.3 6000：回调内同步处理，跨连接并发

- `TcpService` 构造时挂 `Connected/Disconnected/Received`（`DataService.cs:25-29`）；`Initialize` 设监听地址与 `PlayerDataFixedHeaderDataHandlingAdapter`（`:31-39`）。
- `Received`（`DataService.cs:53-68`）反序列化 `ServerRequestData` 后**直接** `ProcessMessagePacket`；返回 `Task.CompletedTask`。
- `ProcessMessagePacket`（`:88-126`）：校验 `PacketCode`、`queryId`、`Sign`；合法后 `ProcessServerMsg`。
- `ProcessServerMsg`（`:129-149`）分派 `DB_LOADHUMANRCD` → `LoadHumanRcd`（`:151-205`）、`DB_SAVEHUMANRCD` → `SaveHumanRcd`（`:207-257`）、`DB_SAVEHUMANRCDEX` → `SaveHumanRcdEx`（`:259-268`，内部再调 `SaveHumanRcd`）。
- `SendRequest`/`SendMessage`（`:270-316`）在回调栈内写回 socket；同一连接的回复顺序 = 回调顺序。
- **串行化点**：同一 GameSvr 连接内，TouchSocket 接收循环按帧回调；跨 GameSvr 连接没有应用层串行化（并发）。同一角色的两次存档来自不同 GameSvr 连接时可并发，最终由 InnoDB 行锁/提交顺序决定（G10：oracle 本身不提供顺序契约）。
- **无全局锁**：`LoadHumanRcd` 只读 `_loginService` 的会话表 + `_playDataStorage`/`_cacheStorage`；`SaveHumanRcd` 调 `_playDataStorage.Add/Index/Update` + `_cacheStorage.Add`，没有 `lock`。

### 2.4 5700：回调内同步处理

- `Received`（`MarketService.cs:71-95`）→ `ProcessMessagePacket`（`:115`）→ `ProcessMarketPacket` 四个 ident。
- 与 6000 同形：同连接串行、跨连接并发；无应用层锁。

### 2.5 存储层：每方法一条连接、无跨方法事务

- `StorageContext`：`Open` 里 `new MySqlConnection(_storageOption.ConnectionString)` + `_connection.Open()`（`DBContext.cs:21-35`）；`BeginTransaction`（`:47-57`）；`Commit`（`:59-65`）；`RollBack` 条件写反（`:67-73`，G8，实际回滚靠 `Dispose` 关连接 `:75-82`）。
- `PlayDataStorage` 每个公开方法自己 `using StorageContext context = new StorageContext(_storageOption)`：
  - `LoadQuickList` `Query.cs:37`；`Query` `:131`；`GetRecord` `:160`；`GetQryChar` `:573`；`AddRecord` `Add.cs:53`；`SaveRecord` `Save.cs:47`；`UpdateChrRecord` `Save.cs:576`。
- `PlayRecordStorage` 每个方法 `Open(ref connSuccess)` 新建 `MySqlConnection`（`PlayRecordStorage.cs:100-110`；`LoadQuickList` `:36-96`、`GetRecord` `:126-171`、`UpdateRecord` `:273-318`、`Add` `:230-263`、`Update` `:382-397`、`Delete` `:323-348`）。
- **没有跨方法的连接/事务复用**：索引表写（`characters_indexes`）全部 autocommit、无事务；存档 `SaveRecord` 单独开事务。
- `SaveRecord` 的 8 段（`Save.cs:55-67`）：`SaveRecord(context, playerId, Data)` → `SaveAblity` → `SaveItem` → `SaveBagItem` → `SaveStorageItem` → `SaveMagics` → `SaveBonusability` → `SaveStatus`，最后 `Commit`。`SaveItem` 内部的 `GetItemRecord(playerId, context, …)` 用同一个 context/connection，处于同一事务内（`Save.cs:206-211`）。
- `AddRecord` 的建角事务（`Add.cs:51-88`）：`BeginTransaction` → `CreateCharacters` → `CreateAblity`/`CreateStatus`/`CreateUseItem`/`CreateBagItem`/`CreateStorageItem` → `Commit`；`CreateStorageItem` 在 try 内吞错，`CreateUseItem`/`CreateBagItem` 在 try 外（LB-07）。

### 2.6 定时缓存回写

- `TimedService.ExecuteAsync` 1s `PeriodicTimer`；`currentTick - syncSaveTick > 300000` 时 `ProcessCacheStorage`（`TimedService.cs:39-64`）。
- `ProcessCacheStorage` 遍历 `_cacheStorage.QueryCacheData()`，逐条 `_playDataStorage.Update(play.Header.Name, play)`，再 `Delete`（`TimedService.cs:73-96`）。
- 与 6000/5100 并发；`CacheStorageService` 的字典无锁（`CacheStorageService.cs:12-18`），oracle 存在数据竞争。Rust 用 `Mutex<CacheStore>` 串行化，是可观测行为不变的加固（缓存实际恒空，见 `cache.rs` 注释）。

### 2.7 oracle 的串行化点汇总

| 范围 | 串行化机制 | 是否全局 |
|---|---|---|
| 5100 四个角色命令 | `Channel` + 单消息线程 | 5100 面全局 |
| 6000 同一 GameSvr 连接 | TouchSocket 接收回调顺序 | 每连接 |
| 6000 跨 GameSvr 连接 | 无应用层串行化；InnoDB 行锁/元数据锁 | 否 |
| 5700 同一 GameSvr 连接 | TouchSocket 接收回调顺序 | 每连接 |
| 一次存档 | 一个 `StorageContext` + 一个事务，8 段顺序执行 | 请求内 |
| 一次建角 | 一个 `StorageContext` + 一个事务，5 张附属表顺序执行 | 请求内 |
| 索引表写 | 每方法一条连接、autocommit | 否 |
| 缓存回写 | 单 `TimedService` 线程逐条 | 定时任务内 |
| MySQL 连接获取 | MySqlConnector 连接池内部同步（缺省 max 100） | 连接池 |
| 共享内存结构 | **无锁**（`_globaSessionList`、`_quickList`、`_NameQuickMap`、`_cacheStorage`） | 数据竞争，非契约 |

### 2.8 TouchSocket 2.0.0-beta.273 的边界

仓库内没有 TouchSocket 源码；本项目只用其 `TcpService` + `Received` 事件。设计按以下事实建模：

- `TouchSocketConfig.SetTcpDataHandlingAdapter(...)` 只配帧适配器，没有 `SetThreadCount`/并发开关（`DataService.cs:31-39`、`UserService.cs:38-45`、`MarketService.cs:38-44`）。
- `Received` 返回 `Task`，本仓库所有回调都同步做完再返回 `Task.CompletedTask`（`DataService.cs:53-68`、`MarketService.cs:71-95`）。
- 因此可依赖的契约是：**同一连接的回调不并发**、**不同连接可并发**。跨连接是否真正并行、同连接是否可能因线程池重入而重叠，作为 **W-DB-7** 用帧序/时间戳证据确认；在设计上不依赖 TouchSocket 的内部线程数，只依赖「同连接顺序 + 跨连接可并发」。

---

## 3. Rust 现状与差异

### 3.1 拓扑

- `main.rs:74-90`：`Db::connect` → `load_quick_lists` → `Arc<tokio::sync::Mutex<Db>>`；`Server::new`；spawn `data::run`（6000）、`data::cache_flush_task`、`market::run`（5700）；最后 `srv.run(rx, db)`（5100）。
- `server.rs:140-160`：5100 `TcpListener::accept` 后每连接 `tokio::spawn(gate_conn)`；`tokio::spawn(handler::queue_loop(...))` 单消费者。
- `data.rs:65-90`：6000 每连接 `tokio::spawn(conn_task)`；`conn_task` 读一帧、`process(...).await`、再读下一帧（`data.rs:105-180`），**同连接串行**。
- `market.rs:231-300`：5700 同形。
- `cache_flush_task`：1s tick，300s 一次 `process_cache_storage`（`data.rs:519-549`）。

### 3.2 全局锁清单（`Arc<Mutex<Db>>`）

| 位置 | 持锁范围 | 跨 await？ | 阻塞面 |
|---|---|---|---|
| `main.rs:79` | 创建 | — | — |
| `handler.rs:51-53` | 每条 5100 `Data` 消息从 `process_data` 开始到结束（含会话查找、索引读、全部 SQL、回包 `tx.send`） | 是 | 5100 全部命令 |
| `data.rs:277-300` | `LoadHumanRcd` 的索引判断 + `cache.get` + `play_data_get` | 是 | 6000 全部取档 |
| `data.rs:380-401` | `store_human_rcd` 的索引判断 + 可能的 `play_data_add` + `cache.add` + `play_data_update`（整段 `save_record` 事务） | 是 | 6000 全部存档、5100/5700/缓存 |
| `data.rs:540-545` | 缓存回写每条 `play_data_update` | 是 | 同上 |
| `market.rs:439/495/522` | 5700 各存储调用（查询/保存/搜索） | 是 | 5700 |

补充：
- `db.rs:147-149` `max_connections(8)`；在全局锁下不会同时用到 8 条（评审员更正）。
- `db.rs` 的 `Db` 方法本身不取锁，锁由调用方持有；因此 `Db` 的 `&mut self` 方法（`record_add`、`record_soft_delete`、`data_add`、`play_data_add`）实际被外层锁保护。
- `server.cache` 另有一把 `Mutex<CacheStore>`；`store_human_rcd` 在持 `Db` 锁时再取 `cache` 锁（`data.rs:380` → `:395`），当前锁序是 `Db -> cache`；`queue_loop` 是 `Db -> gates/login`（`handler.rs:52` → `:64`/`:247`）。改造时必须保持「不出现反向嵌套」。

### 3.3 一次慢存档的具体阻塞路径

设 GameSvr 发来一条 `DB_SAVEHUMANRCD`，其 `characters` 行被外部事务 `SELECT ... FOR UPDATE` 或 `UPDATE ... SLEEP(3)` 挡住：

1. `data::conn_task`（`data.rs:105-180`）读到帧，调用 `process` → `process_server_msg` → `save_human_rcd`（`data.rs:328-365`）。
2. `save_human_rcd` → `store_human_rcd`（`data.rs:374-403`）→ `db.lock().await`（`:380`）成功，成为全局锁持有者。
3. `play_data_update`（`playdata.rs:1098-1105`）→ `pool.acquire`（当前 8 条池，空闲时立即可得）→ `save_record`（`playdata.rs:777-890`）：
   - `conn.begin()`（`:805`）；
   - 8 段 `Save*`，其中 `UPDATE characters` 等语句在 InnoDB 行锁上等待（缺省 `innodb_lock_wait_timeout=50s`）；
   - `tx.commit()`（`:878`）。
   在等待期间，`db` 锁一直不释放。
4. 此时：
   - **5100**：`handler::queue_loop` 的下一条 `Data` 在 `db.lock().await`（`handler.rs:52`）排队，连 `find_by_account` 这种纯内存查询都进不去；`CM_QUERYCHR` 不回包，SelGate/客户端表现为角色列表转圈；`CM_SELCHR` 同样排队，即使它只需要写 `characters_indexes` 的 `SelectID` 和读 `characters`。
   - **6000**：另一 GameSvr 的 `DB_LOADHUMANRCD` 在 `data.rs:277` 排队；另一条 `DB_SAVEHUMANRCD` 在 `data.rs:380` 排队。
   - **5700**：`market.rs:439/495/522` 排队。
   - **缓存回写**：`data.rs:540` 排队。
   - **不受影响**：5100 的 KeepAlive/Enter/Leave（在 `gate_conn` 回调里、不取 `Db` 锁）、LoginSrv 5600 链、每连接 writer task 的 socket 写。
5. 若 MySQL 停机而非行锁：`pool.acquire` 在 `test_before_acquire` 下反复失败，直到 `acquire_timeout`（sqlx 0.8 缺省 30s）返回 Err；因为锁是串行的，N 个请求的总延迟近似 **N × 30s**。oracle 每个请求各自 `Open`，N 个请求的延迟近似 **max(15s, 并发)**，不会叠加。

### 3.4 已修与未修

- **已修并有带牙测试**：
  - LB-01：`data.rs` `load_db_acquire_failure_answers_degraded_success_frame`（`data.rs:920`）及反向子断言（`:1037`）；
  - LB-02：`handler.rs` `querychr_db_down_answers_empty_list`（`:851`）、`querychr_chr_table_failure_skips_character`（`:879`）；
  - LB-03：`handler.rs:910`；
  - LB-04/G4：`handler.rs:938`、`new_chr_data_swallows_add_failure_like_oracle`（`:734`）；
  - LB-05：`handler.rs:970`；
  - LB-06 重写：`playdata.rs:1820/1872/1920/1968`；
  - LB-07：`handler.rs:1000`、`data.rs:1069/1095`；
  - LB-09：`db.rs:973`。
- **未修**：LB-08（全局锁的延迟面）。本文第 4/5 节给方案，第 6 节给证据卡。
- **不得重开**：LB-06 原文四条并列断言（见 §1.2）；LB-04/LB-07 独立新卡（并入 G4）。

---

## 4. 候选方案 A：每连接 worker + 共享连接池 + 索引短锁

### 4.1 结构

```rust
pub struct Db {
    pub(crate) pool: MySqlPool,          // 共享、Clone 即共享
    idx: tokio::sync::RwLock<Indexes>,   // 只保护内存索引；不在持锁时 await SQL
    create_gate: tokio::sync::Mutex<()>, // 只串行化“建角/自动建档”的检查+INSERT+索引更新
}

struct Indexes {
    quick: HashMap<Vec<u8>, i64>,
    by_account: HashMap<Vec<u8>, Vec<Quick>>,
    index_ids: HashSet<i64>,
    deleted_ids: Vec<i64>,
    char_ids: HashSet<i64>,
    name_quick: HashMap<Vec<u8>, i64>,
    name_text: HashMap<String, i64>,
}
```

- `main.rs` 改成 `let db = Arc::new(db::Db::connect(&dsn).await?);`（`Db` 内部池/RwLock 都是 `Send+Sync`），不再包 `tokio::sync::Mutex`。
- `handler::queue_loop` 签名改为 `db: Arc<Db>`；`process_data`/`dispatch`/四个命令改为 `&Db`（`new_chr`/`delete_chr`/`select_chr` 不再需要 `&mut Db`）。
- `data::run`/`conn_task`/`load_human_rcd`/`save_human_rcd`/`store_human_rcd` 改为 `&Arc<Db>`/`&Db`。
- `market::run`/`conn_task`/各 handler 改为 `&Arc<Db>`/`&Db`。
- `cache_flush_task` 改为 `&Arc<Db>`，逐条 `play_data_update`。
- **不改**每连接循环：6000/5700 仍是「读一帧 → await 处理 → 读下一帧」；5100 仍是单消费者 `queue_loop`。不按请求 `tokio::spawn`，否则回包顺序会变。

### 4.2 连接池参数（对齐 oracle）

MySqlConnector 2.3.5 未在连接串覆盖的缺省值：`Maximum Pool Size=100`、`Minimum Pool Size=0`、`Connection Timeout=15s`、`Connection Idle Timeout=180s`（官方 connection options；本仓库 `SettingsModel.cs:66` 的连接串也未覆盖）。sqlx 0.8 的 `PoolOptions` 缺省：`max_connections=10`、`acquire_timeout=30s`、`idle_timeout=600s`、`max_lifetime=1800s`、`test_before_acquire=true`（`sqlx-core-0.8.6/src/pool/options.rs:151-162`）。

建议显式设置：

```rust
MySqlPoolOptions::new()
    .max_connections(100)          // 对齐 MySqlConnector 缺省
    .min_connections(0)
    .acquire_timeout(Duration::from_secs(15)) // 对齐 ConnectionTimeout
    .idle_timeout(Some(Duration::from_secs(180)))
    .max_lifetime(None)
    .test_before_acquire(true)
    .connect(...)
```

- `max_connections(100)` 是 oracle 的连接池上限；不改的话，A 方案在「10 个并发取档 + 1 个慢存档」时会先在 8 条池上排队，仍可能被误判为 LB-08 未修。
- `acquire_timeout(15s)` 与 oracle `ConnectionTimeout` 对齐；**这会改变现有故障测试的等待上限**（当前 sqlx 缺省 30s），实现卡必须同步调整测试读超时并在 `review` 里写明。
- `test_before_acquire(true)` 保留 sqlx 缺省；MySQL 停机时它会丢弃坏连接并重试，直到 `acquire_timeout`。oracle 的 `Open` 在连接池取出坏连接时也可能重试/失败；两者「最终返回失败」一致，**具体耗时**由 W-DB-2 采证，不在本文假设完全相等。
- 若 owner 认为 100 条连接资源不可接受，可保留 8 但必须在 `review` 登记 `D-*` 并承认「并发取档 >8 时与 oracle 不同」；不能既保留 8 又宣称 LB-08 全绿。

### 4.3 内存索引的并发规则

| 操作 | 当前方法 | 改造后 |
|---|---|---|
| 读名字→Id | `data_index`（`db.rs:433`） | `idx.read().await` 取快照后立即释放 |
| 读账号→角色列表 | `find_by_account`（`db.rs:279`） | 克隆 `Vec<Quick>` 后释放 |
| 读索引行/角色字段 | `record_get_by`/`get_query_char`/`query_name_and_map` | 先在短读锁里取 `id`，再释放锁做 SQL |
| 插入索引 | `record_add`（`db.rs:316-363`） | `create_gate` 内：检查 → SQL INSERT → `idx.write()` 更新 `quick/index_ids/by_account` |
| 软删索引 | `record_soft_delete`（`db.rs:379-393`） | SQL 后 `idx.write()` 移除 `index_ids`；保留 `quick`（oracle 不删，见方法注释） |
| 更新索引行 | `record_update`/`record_update_by`（`db.rs:365-397`） | 只写 DB，不动内存索引（oracle 同样） |
| 6000 自动建档 | `data_add`（`db.rs:506`） | `create_gate` 内：检查 `name_quick` → 事务 → `idx.write()` 更新 `name_quick/char_ids` |
| 6000 `play_data_add` | `playdata.rs:1110` | `create_gate` 内：检查 `name_text` → 事务 → `idx.write()` 更新 `name_text/name_quick/char_ids` |

- **不在持 `idx` 锁时 await SQL**：所有 `read`/`write` guard 都在一个同步块里取快照/改完即 drop，然后才 `.await` 数据库。这样 `tokio::sync::RwLock` 不会因慢 SQL 阻塞其他索引读。
- **建角闸门**：5100 `CM_NEWCHR` 与 6000 自动建档可能并发；oracle 的 `_quickList` 是普通 `Dictionary`，并发同名会各自 INSERT、第二个 `Dictionary.Add` 抛异常（G10 非契约）。Rust 用 `create_gate` 把「检查 + INSERT + 内存索引更新」串行化，保证最终只有一行/一个索引。这是**比 oracle 更严格**的线性化；若 owner 要求 bug-for-bug，必须另立 `T-*` 并放弃该保证，本文建议登记 `D-*`（可观测收益：不产生孤儿/重复行）。
- **跨面顺序**：A 方案下 5100 队列、6000 每连接、5700 每连接仍各自有序；跨面没有顺序契约，与 oracle 一致（G10）。不要为了「全局顺序」再加回一把大锁。

### 4.4 回包顺序

- 6000：`conn_task` 的读循环 `read_exact(head) → read_exact(body) → process(...).await`（`data.rs:105-180`），同连接天然串行；回复走 `tx`（每连接 writer task），`tx.send` 顺序 = 请求处理顺序。
- 5700：同形。
- 5100：`queue_loop` 单消费者，`process_data` 内 `send_user` 先入 `tx`，再回写会话状态（`handler.rs:51-57`、`:231-248`）；同连接（同一 `Gate.tx`）回复顺序 = 队列顺序。
- 跨连接：oracle 不保证，A 也不保证；证据卡只要求**每连接**顺序一致（C2）。

### 4.5 事务边界

- `save_record` 仍是一个 `pool.acquire()` 得到的连接 + `begin/commit`，8 段顺序不变；`SaveItem` 内部 `GetItemRecord` 仍用同一 `&mut MySqlConnection`，处于同一事务。
- `data_add` 仍是一个 `pool.begin()` 事务，5 张附属表顺序不变。
- 索引表 `record_add/update/delete` 仍是 autocommit、无事务；不要把它们包进存档事务。
- 连接从池里按请求借出，**不能**把同一个 `MySqlConnection` 跨任务共享；`sqlx::Transaction` 的生命周期必须限制在单个请求内。
- 关闭/回滚：`save_record` 的 `SaveMagics` 失败显式 `tx.rollback()`，其余靠 `Transaction` drop；与 oracle 的 `Dispose` 关连接回滚净效果一致（G8）。

### 4.6 错误路径

- `pool.acquire` 失败：`play_data_get` 当前 `?` 上抛，`data.rs` 映射为 `n_check_code=-2`（降级成功帧）；`play_data_update` 的 Err 被 `store_human_rcd` 吞掉并回成功帧。A 方案不改这些映射。
- `record_get_by`/`get_query_char` 的 Err：`handler.rs` 按 oracle 的 `Open` 失败语义分别 `continue`/`bail`/返回 0；A 方案不改。
- **关键**：去掉全局锁后，每个请求的 acquire 失败/超时各自独立发生；oracle 的 N 个并发请求各自 `Open` 也各自失败。这样才满足 W-DB-2 的「并行超时」判据。
- 不要在 `Db` 外层加 `anyhow` 兜底把 Err 变成 `Ok(None)`；否则会破坏 LB-01/02/03/05 的已钉语义。

### 4.7 改动文件（D2 实现卡）

| 文件 | 改动 |
|---|---|
| `mir2-rs/bins/db-srv/src/main.rs` | `Arc<Mutex<Db>>` → `Arc<Db>`；spawn 参数 |
| `mir2-rs/bins/db-srv/src/db.rs` | `Db` 结构、`Indexes`、RwLock、`create_gate`、池参数；`record_*`/`data_*` 签名 `&self` |
| `mir2-rs/bins/db-srv/src/playdata.rs` | `play_data_get/update/add` 改 `&self`；`data_add` 内部索引更新走 `idx.write()` |
| `mir2-rs/bins/db-srv/src/data.rs` | 去 `Mutex` 参数/锁；`load_human_rcd`/`store_human_rcd`/缓存回写 |
| `mir2-rs/bins/db-srv/src/handler.rs` | `queue_loop` 去锁；`process_data`/`dispatch` 四命令 `&Db`；`new_chr`/`delete_chr`/`select_chr` 签名 |
| `mir2-rs/bins/db-srv/src/market.rs` | 去 `Mutex`；各 handler `&Db` |
| `mir2-rs/bins/db-srv/src/testsupport.rs` | 测试建 `Arc<Db>` 的辅助 |
| 同上 `#[cfg(test)]` 模块 | 现有 LB 测试适配新签名；新增 LB-08 测试 |

**不碰**：`mir2-rs/crates/{protocol,storage}`、`src/**`、`sql/**`、`MirClient/`、`E:/MirServer/**`、`bins/game-svr/src/{chat,main}.rs`、`tools/**`、`docs/window-run-sheet*`、攻城设计文档。

---

## 5. 候选方案 B：单写线程 + 读池（等价性分析）

### 5.1 结构

- 一个 `writer` 任务持有专用 `MySqlConnection`（或一个单连接事务执行器），所有写请求经 `mpsc` 排队；
- 读请求直接走共享 `MySqlPool`；
- 内存索引仍需 `RwLock`（与 A 同）；
- 每个请求带 `conn_id + seq`，完成时回传给该连接的 writer task，保证每连接回包顺序。

### 5.2 等价性矩阵

| 维度 | oracle | A | B（朴素：单写队列 + 读池） | B（细化：角色写 lane + 索引写 lane） |
|---|---|---|---|---|
| 5100 QUERYCHR | 单消息线程，读独立连接 | 同 | 读池可并发，**更快** | 同 |
| 5100 SELCHR | 写 `characters_indexes`（autocommit）+ 读 `characters`，单线程顺序 | 同 | `SelectID` 写排慢存档后 ⇒ **被阻塞**，STARTPLAY 延迟 | 索引写 lane 不被角色表行锁阻塞 ⇒ 可过；仍需读后写屏障 |
| 5100 NEWCHR/DELCHR | 单消息线程，写索引/角色表 | 同 | 排在慢存档后 ⇒ **被阻塞** | 索引/建角走索引 lane，可过；建角事务含 `characters`，仍可能被角色表 lane 阻塞 |
| 6000 LOAD | 每连接独立连接 | 同 | 读池可并发 | 同 |
| 6000 SAVE | 每连接独立连接、行锁级并发 | 同 | 所有写串行，**慢存档阻塞其他角色存档** | 仍串行于同一 lane；不同角色也串行 |
| 5700 | 每连接独立连接 | 同 | 写排队；查询读池 | 需再分 lane，复杂度高 |
| 缓存回写 | 单定时线程、独立连接 | 同 | 写排队 | 写排队 |
| 每连接回包顺序 | 同连接回调顺序 | 读循环顺序 | 读/写分派后可能**乱序**，需 seq 屏障 | 同左 |
| 事务边界 | 一次存档一个连接/事务 | 同 | writer 连接可保证；但 `SaveItem` 内部 SELECT 若走读池会**读不到事务内未提交行**，必须强制走 writer 连接 | 同左 |
| 错误路径 | 每请求 `Open` 失败各自映射 | 同 | writer 任务死亡/队列满时请求会悬挂；需超时/监督 | 同左 |
| 全局写顺序 | 无契约（G10） | 无 | **强加全局写序**，可能改变多 GameSvr 同角色最终行 | 同左 |
| 关服 | 不 flush（T-27） | 不 flush | 队列中未执行写被丢弃/或需 drain，需显式选择 | 同左 |

### 5.3 结论

- **B 朴素形态不满足 LB-08 的「选角」子判据**：`CM_SELCHR` 的 `SelectID` 写是写操作，会排在慢存档后面；而 oracle 的 `SelectID` 写是独立的 `characters_indexes` autocommit 连接，不会被 `characters` 行锁阻塞。因此 B 若只做「单写线程」，只是把 6000 取档和 5100 查询救出来，**没有救出选角/建角/删角**。
- **B 细化形态**（角色表写 lane + 索引表写 lane + 每连接 seq + 读后写屏障 + writer 连接内事务）在理论上可接近 A，但：
  1. 需要维护两套队列、每连接序号、读后写屏障、writer 监督/重启、超时映射；
  2. 全局写序仍是 oracle 没有的强约束（G10），多 GameSvr 同角色时最终行可能不同；
  3. 事务内 SELECT 必须走 writer 连接，否则隔离级别改变；
  4. 代码面比 A 大，回归风险高。
- **推荐 A**。只有当 owner 明确要求「限制 MySQL 并发连接数/写放大」并接受 `D-*` 时，才考虑 B 的细化形态；即使选 B，LB-10 的窗口判据不变，仍要采「帧序 + DB 语句序 + 表后态」。

---

## 6. LB-10 证据卡定义

### 6.1 卡面

- **卡号**：`LB-10-E1`（DBSrv 持久化面真机证据卡）
- **tier**：**R**（持久化落库/时序/故障注入 ⇒ 占窗口）
- **目标**：为「9 表 + `characters_indexes`」的正常路径、错误路径、并发延迟路径留下**可复算**的跨实现证据；不是再写一份代码审计。
- **非目标**：不修代码；不替代 `openmir2-m3-scoped-diff-ability`；不覆盖 5700 拍卖行的业务语义（部署未加载 `MarketSystem.dll`，S4B-5..10 已登记）。
- **前置**：M3 现有门禁全绿；`characters_ablity` RED 的承卡状态明确（W-DB-5）。

### 6.2 采集物（每次 run 一个目录）

目录：`/e/tmp/<window>/db-evidence/<run_id>/`（本车道不创建；窗口卡创建）

| 文件 | 内容 | 关键字段 |
|---|---|---|
| `frames.ndjson` | 驱动侧收发的每一帧，按 `mono_ns` 排序 | `run_id, side, baselines, conn_id, direction, mono_ns, wall_utc, query_id, ident, recog, series, len, sha256, payload_hex` |
| `db_stmt.ndjson` | MySQL 语句序 | `run_id, side, thread_id, event_id, timer_start_ps, timer_end_ps, event_time_utc, command_type, sql_text, rows_affected, error_code, txn_id` |
| `db_txn.ndjson` | 事务边界 | `run_id, side, thread_id, txn_id, state(BEGIN/COMMIT/ROLLBACK), timer_start_ps, event_time_utc` |
| `locks.ndjson` | 锁等待采样（100ms） | `run_id, side, sample_ns, waiting_thread, blocking_thread, lock_type, lock_table, lock_index, lock_data` |
| `processlist.ndjson` | `information_schema.processlist` 采样 | `sample_ns, id, user, host, db, command, time, state, info` |
| `table_dump/<table>.tsv` | 每表全量、按主键排序、NULL 统一标记 | 列名表头；`--skip-extended-insert` 等价 |
| `schema.sql` | `SHOW CREATE TABLE` 9 表 + `characters_indexes` | 用于证明表结构未漂移 |
| `manifest.sha256` | 上述文件的 sha256 | `sha256sum -c manifest.sha256` |
| `build.txt` | 两端 commit/二进制 hash | C# DLL、Rust binary、`git rev-parse` |
| `notes.md` | 注入脚本、时钟同步、异常 | 人读 |

**时间戳口径**：
- 帧：驱动进程 `CLOCK_MONOTONIC_RAW`（ns）+ UTC wall clock；同一 run 内只比较 `mono_ns`。
- DB：`performance_schema.events_statements_history_long` 的 `TIMER_START/END`（ps，MySQL 启动计时）为主；`mysql.general_log` 的 `event_time`（MySQL 8.0 `TIMESTAMP(6)`）为交叉校验。两者通过 run 开始/结束各执行一次 `SELECT 'DBEV_SYNC_<run_id>', NOW(6), SYSDATE(6)` 对齐。
- 帧与 DB 的关联：每个请求在驱动侧记录 `query_id` 与发送 `mono_ns`；DB 侧按「请求发送后、回包前」的时间窗 + SQL 文本/表名匹配。并发场景只要求**因果**（首条语句在发送后、回包在 COMMIT 后），不要求跨连接全序。
- 若 `performance_schema` 不可用，降级为 `general_log=ON, log_output=TABLE`，并在 `notes.md` 写明精度损失；R 级证据不得用「只有应用日志」替代。

### 6.3 场景矩阵

| ID | 场景 | 操作序列 | 目的 |
|---|---|---|---|
| S1 | 正常建角/选角 | 5100 `CM_QUERYCHR`（空）→ `CM_NEWCHR` → `CM_QUERYCHR`（1 角色）→ `CM_SELCHR` | 回包、`characters_indexes` 后态、`characters`/附属表建行 |
| S2 | 6000 取档/存档 | 6000 `DB_LOADHUMANRCD` → 穿脱装备/仓库存取（GameSvr 侧构造）→ `DB_SAVEHUMANRCD` → 再 `DB_LOADHUMANRCD` | 9 表逐字段、物品差量、事务边界 |
| S3 | 错误矩阵 | MySQL `stop/start`；`KILL` 连接；`characters_item` 第 5 条 INSERT 挂 `SIGNAL`；`characters_indexes` 的 UPDATE 挂 `SIGNAL`；`SELECT ... FOR UPDATE` 持行锁；`max_connections` 压到 1 | LB-01/02/03/04/05/07、G2/G3/G4 的帧与表后态 |
| S4 | 慢存档并发 | 外部事务持 `characters` 行锁/`SLEEP(3)`；同时发 6000 `LOAD`、5100 `QUERYCHR`、`SELCHR`、另一角色 `SAVE`、5700 查询 | LB-08 延迟/队头阻塞 |
| S5 | 崩溃/重启 | `DB_SAVEHUMANRCD` 事务中途 `KILL` 连接 / 杀 DBSrv；重启后 `LoadQuickList` | 回滚、索引重建、孤儿行 |
| S6 | 关服/重连 | 软关会话、DBSrv 重启、客户端重连 | `LoadRcd` 一次性、缓存/会话状态 |
| S7 | 边界名/编码 | GBK 角色名、大小写不同账号、同名脏索引 | `ChrName` 字节、`Index` 大小写折叠、LB-09 |

> S3 的 5100 命令必须先同 socket `CM_QUERYCHR` 成功、睡 >1000ms，再发 `CM_DELCHR`/`CM_NEWCHR`；`DeleteMinLevel` 用缺省 30，测试角色等级要低于/高于它各一个。

### 6.4 判据（机判）

| ID | 判据 | 通过条件 |
|---|---|---|
| C1 | 回包字节 | 每个请求的响应帧（6B 头 + MemoryPack 体）逐字节等于 oracle；`payload_hex` 与 `sha256` 一致。允许差异：无（`QueryId` 由驱动固定为同一序列）。 |
| C2 | 每连接回包顺序 | 同一 `conn_id` 的 `direction=rx` 序列中的 `query_id` 顺序 = 发送顺序；跨连接不比较。 |
| C3 | 9 表 + indexes 后态 | 按主键/业务键对齐后逐列相等；`CreateDate/ModifyDate/LASTUPDATE` 只比较「非空/变化」；`SelectID`、`IsDeleted` 精确；`Id/PlayerId` 在相同基线快照下精确，否则按 `ChrName` 映射并记录映射表。 |
| C4 | 事务分组 | 每条 `BEGIN` 与对应 `COMMIT/ROLLBACK` 之间的 `db_stmt` 属于同一 `txn_id`，且不含其他请求的语句；存档 8 段的表/语句集合与顺序 = oracle；建角 5 张附属表集合 = oracle。 |
| C5 | 错误路径 | 每个 S3 注入下，回包帧（含 `recog`/`ident`）与 oracle 相同；表后态相同（含孤儿索引、`IsDeleted=2`、回滚行数）；无「零回包超时」差异。 |
| C6 | 慢存档延迟 | S4 中，慢存档被阻塞 ≥3s 时：6000 `LOAD` P99 < 500ms；5100 `QUERYCHR` P99 < 500ms；`SELCHR` 的 `SM_STARTPLAY` P99 < 500ms；另一角色 `SAVE` 的 P99 < 1s。oracle 同场景基线同表采集。 |
| C7 | DB 停机超时 | S3 停库时，N=4 个独立 6000 连接的 `LOAD`/`SAVE` 总墙钟 < 2×单请求超时（并行失败），不是 N×；单请求耗时与 oracle 的 15s 口径差 ≤5s（或 owner 批准的阈值）。 |
| C8 | 崩溃回滚 | S5 事务中途断连后：9 表无半套行；重启后 `QUERYCHR`/`LOAD` 与 oracle 一致；`characters_indexes` 孤儿集合与 oracle 一致。 |

### 6.5 窗口项

| 窗口项 | 时长 | 前置 | 采集 | 判据 | 失败处理 |
|---|---|---|---|---|---|
| W-DB-1 正常路径 9 表 diff | 2h | 两份同基线 schema/data（`mir2_db_oracle`/`mir2_db_rust`）；S1+S2 驱动；`performance_schema=ON` | `frames/db_stmt/db_txn/table_dump/manifest` | C1/C2/C3/C4 | 任一 diff ⇒ `request_changes`，不得用「语义等价」放过 |
| W-DB-2 故障矩阵 | 2h | W-DB-1 通过；MySQL 可 stop/start、`KILL`、`SIGNAL` 触发器权限 | S3 每格独立 run | C1/C3/C5/C7 | 缺一格即 R 未闭环 |
| W-DB-3 慢存档并发 | 1h | W-DB-1 通过；外部事务/`SLEEP` 注入 | S4 ×3 轮 | C6 | P99 超标即红；若 owner 裁决接受，必须 `D-*` + 降级判据 |
| W-DB-4 崩溃/重启 | 1h | W-DB-2 通过；可杀进程/`KILL` 连接 | S5 ×3 轮 | C8 | 出现半套行即 Critical |
| W-DB-5 M3 ability RED | 0.5h | `openmir2-m3-scoped-diff-ability` 状态确认 | 现有 `scoped_diff.py` 9 表扩展 | `characters_ablity` 4 行 diff=0 或 owner 书面豁免 | 未闭环则 LB-10 不得 `closed` |
| W-DB-6 池/超时参数 | 0.5h | A 方案实现完成 | 并发 10 取档 + 停库 | C7 + `SHOW PROCESSLIST` 峰值 | 峰值 <10（max_connections 太小）即红 |
| W-DB-7 TouchSocket 跨连接并发 | 0.5h | 无 | 两个 GameSvr 连接同时发 `LOAD`，抓帧/DB 线程 | 两请求的 DB `thread_id` 不同且时间窗重叠 | 若同连接实际串行，C6/C7 的 oracle 基线需重算 |

总窗口约 7.5h（含切换/重置），按一个独占窗口安排；W-DB-5 可与 W-DB-1 同段。

### 6.6 带牙变异（每条都必须让指定断言红）

| 变异 | 操作 | 期望红 |
|---|---|---|
| M1 | `save_item_table` 的 `Position` 绑定错 1 位 | C3（物品表行/位置）或 C1（回包体） |
| M2 | `CM_DELCHR` 写 `IsDeleted=0` 而非 1 | C3（`characters_indexes` 后态） |
| M3 | 恢复 `store_human_rcd` 的 `?` 上抛（LB-01/02/03 旧形） | C5（回包缺失/降级帧） |
| M4 | 加回全局 `Mutex<Db>` 或把池压到 1 | C6（QUERYCHR/SELCHR P99）与 C7（并行超时） |
| M5 | `save_record` 去掉 `begin/commit` 或每表独立提交 | C4（事务分组）、C8（崩溃回滚） |
| M6 | 跳过 `characters_bonusability` 的 `SaveBonusability` | C3（该表后态） |
| M7 | 建角 `characters_storageitem` 改为「失败即回滚」（违背 LB-07 反向子断言） | C5（建角后态/回包） |
| M8 | 6000 每请求 `tokio::spawn` 后直接回包（不等完成） | C2（同连接回包顺序）、C4 |

变异必须在钉住 commit 上执行；记录「先红 → 逐字节还原 → `sha256sum -c` 绿」三件套（命令 + 输出 + 路径 + hash）。

### 6.7 证据卡收口条件

- W-DB-1..7 全绿，或 owner 对具体项书面 `D-*` 豁免；
- `manifest.sha256` 校验通过；
- 证据目录登记到报告（路径 + run_id + 基线 + 二进制 hash）；
- `review` 写明 `tier=R`、`tier_why=持久化落库/时序/故障注入`、`window_required=true` 及本卡窗口项；
- 未闭环时卡保持 `in-progress`，不得 `closed`。

---

## 7. 写集切分、tier 与执行顺序

### 7.1 卡片切分

| 卡 | 内容 | tier | 写集 | 依赖 |
|---|---|---|---|---|
| **D1（本卡）** | 连接模型与证据卡设计 | **L** | `mir2-rs/tests/parity/m3/design-dbsrv-connection-20261021.md` | 无 |
| **D2** | 方案 A 实现：去全局 `Mutex<Db>`、共享池、索引 RwLock、建角闸门 | **R** | `mir2-rs/bins/db-srv/src/{main,db,playdata,data,handler,market,testsupport}.rs` + 同文件 `#[cfg(test)]` | D1 |
| **D3** | LB-08 延迟判据 | **R** | `mir2-rs/bins/db-srv/tests/lb08_latency.rs`（新）或 `data.rs` 的 `#[ignore]` 测试 | D2 |
| **D4** | LB-10 证据驱动/采集脚本 | **R** | `mir2-rs/tests/parity/m3/db-evidence/**`（新）；必要时 `mir2-rs/tests/live_{data,querychr}.rs`（先与 owner 确认不与他人写集冲突） | D2 |
| **D5** | LB-10 窗口执行与报告 | **R** | 报告 `mir2-rs/tests/parity/m3/window-dbsrv-evidence-*.md`；证据在 `/e/tmp/<window>/db-evidence/` | D3/D4 + 窗口 |

> D2/D3/D4 都改 `bins/db-srv` 或测试，必须**单 owner 串行**；不要与其它 db-srv 车道并行。`tools/**`、`docs/window-run-sheet*`、`bins/game-svr/src/{chat,main}.rs`、攻城设计文档一律不碰。

### 7.2 tier 理由

- **D1 = L**：纯文档、只读；不改运行时，不可能只在真机暴露差异。审查按 L 级自证「逐行引用 oracle + Rust 行号」并抽样一条（本文明细已给）。
- **D2/D3/D4/D5 = R**：触及并发/时序/持久化；R 级判据必须真机窗口（故障注入子集），不得用「语义等价/代码看起来一样」盖掉延迟与回包差异。
- `review` 必须带 `tier` + `tier_why`；D1 的 `approve` 也写 `tier=L, tier_why=纯文档只读`。

### 7.3 门禁

实现卡 D2 合并前：

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p msgcodegen -- --check   # 本卡不改消息号，仍跑
```

构建隔离：`export CARGO_TARGET_DIR=/e/tmp/<本车道>-target`，在 worktree 内跑；禁止 `--manifest-path` 指向别处。LB-08 测试用 `#[ignore]`/环境变量 `MIR2_TEST_DSN` 门控，普通 CI 不占端口、不连生产库。

---

## 8. 判据草案（D2/D3 直接可执行，全部带牙）

> 下列测试需要真实 MySQL；默认 `#[ignore]`，窗口/显式 `MIR2_TEST_DSN` 才跑。测试库必须是独立 schema，禁止 `mir2_db` 生产库。

### 8.1 `slow_save_does_not_block_querychr`（LB-08 主判据）

**步骤**：
1. 种子账号 `acc1` + 角色 `A`（等级 > `DeleteMinLevel`），启动 Rust db-srv + 伪 LoginSrv + 5100 驱动。
2. 在外部 MySQL 连接执行 `START TRANSACTION; SELECT * FROM characters WHERE ChrName='A' FOR UPDATE;`（或 `UPDATE characters SET Gold=Gold WHERE ChrName='A'; SELECT SLEEP(3);`）。
3. 通过 6000 连接发 `DB_SAVEHUMANRCD`（角色 A），在 `information_schema.innodb_trx`/`data_lock_waits` 确认其在等行锁。
4. 立即通过 5100 连接发 `CM_QUERYCHR`（先完成一次成功的 `CM_QUERYCHR` 建立 `sAccount`，再等 >200ms；`CM_QUERYCHR` 有 200ms 限频）。
5. 断言：`SM_QUERYCHR` 在 **500ms** 内到达，角色列表字节与 oracle 同场景一致。
6. 释放外部事务；断言 6000 存档在 5s 内回 `DBR_SAVEHUMANRCD recog=1`。

**牙**：把 `handler::queue_loop` 的 `db.lock()` 加回（或把池压到 1）⇒ `SM_QUERYCHR` 等到行锁释放（≥3s）⇒ 红。反向子断言：MySQL 停机时仍按 LB-02 回空列表，不得因去掉锁而变成零回包。

### 8.2 `slow_save_does_not_block_selchr`

**步骤**：同 8.1，但第 4 步改为先成功 `CM_QUERYCHR`，再 `CM_SELCHR("A")`；断言 `SM_STARTPLAY(525)` + 路由体在 **500ms** 内到达，且 `characters_indexes.SelectID` 最终写入成功（行锁释放后）。

**牙**：
- 加回全局锁 ⇒ `SELCHR` 排在 `DB_SAVEHUMANRCD` 后 ⇒ 红；
- 若实现把 `record_update_by` 也放进「单写线程」方案 B 的慢 lane ⇒ 红（这正是 B 朴素形态的问题）。

### 8.3 `slow_save_does_not_block_load`

**步骤**：同 8.1，但第 4 步改为另一 6000 连接发 `DB_LOADHUMANRCD`（角色 A）；断言 `DBR_LOADHUMANRCD recog=1` + 完整 `CharacterDataInfo` 在 **500ms** 内到达。

**牙**：加回全局锁 ⇒ 红。

### 8.4 `db_down_timeouts_run_in_parallel`

**步骤**：
1. 起 4 个独立 6000 连接；停 MySQL（或 `KILL` 全部连接 + 拒绝新连接）。
2. 4 个连接各发一条 `DB_LOADHUMANRCD`，记录每条回包时间。
3. 断言：4 条回包的总墙钟 < **2×单请求超时**（不是 4×）；每条都是 oracle 同故障的降级成功帧（`1100 recog=1 HumDataInfo=null`）。
4. 5100 不要用于本判据：oracle 5100 本身是单消息线程，N 条命令就是串行，不能要求并行。

**牙**：全局 `Mutex<Db>` 下第 2/3/4 条要等前一条 acquire_timeout 结束 ⇒ 总时间 ≈4× ⇒ 红。把 `acquire_timeout` 从 15s 改成 30s 也应让阈值变红（或至少记录）。

### 8.5 `save_transactions_do_not_interleave`

**步骤**：
1. 两个独立 6000 连接并发 `DB_SAVEHUMANRCD`（角色 A/B）。
2. 抓 `performance_schema` 语句序；按 `txn_id` 分组。
3. 断言：每个事务的 8 段语句连续，`BEGIN` 后到 `COMMIT/ROLLBACK` 前没有另一事务的语句；`characters` 的 `UPDATE` 与 `characters_ablity` 等的顺序与 oracle 一致。

**牙**：把共享池换成「全局单连接 + 两个任务并发使用」⇒ 连接 busy/语句交错 ⇒ 红；把 `save_record` 改成每表独立提交 ⇒ `BEGIN/COMMIT` 分组消失 ⇒ 红。注意：全局锁版本也能通过本测试（串行），所以本测试只防实现退化，不是 LB-08 的主牙。

### 8.6 `per_connection_reply_order`

**步骤**：同一 6000 连接连续发 10 条交替 `LOAD`/`SAVE`（不等回包、流水化），断言回包 `query_id` 顺序 = 发送顺序；5100 同一 socket 连续发 `QUERYCHR`/`SELCHR`（遵守限频）同理。

**牙**：6000 每请求 `tokio::spawn` 后立即读下一帧 ⇒ 回包乱序 ⇒ 红；5100 把队列改成多消费者 ⇒ 乱序/状态竞争 ⇒ 红。

### 8.7 `error_path_matrix_unchanged`

**步骤**：运行现有 LB-01/02/03/05/07/09 测试 + `new_chr_data_swallows_add_failure_like_oracle` + LB-06 重写的四个子测试。

**牙**：把 `store_human_rcd` 的 Err 映射改回 `?` ⇒ `load_db_acquire_failure_answers_degraded_success_frame` 红；把 `record_update_by` 的 Err 当成失败 ⇒ `selchr_selectid_write_failure_still_startplay` 红。D2 合并前这些必须全绿。

### 8.8 `index_create_single_winner`（支撑不变量，不作 parity 判据）

**步骤**：构造 6000 自动建档与 5100 `CM_NEWCHR` 对同一名字并发；断言最终 `characters` 1 行、`characters_indexes` 1 行、`name_quick/name_text` 各 1 项；不允许 panic/重复行。

**牙**：去掉 `create_gate` 并把 `idx` 改成无锁 `UnsafeCell` ⇒ 测试出现重复行/panic ⇒ 红。由于 oracle 此路径本身是字典数据竞争（G10），本测试只作为 Rust 安全性不变量；若 owner 要求 bug-for-bug，必须 `D-*` 换掉该断言。

---

## 9. 未决问题（需要 owner/审查裁决）

1. **A vs B**：本文推荐 A。若 owner 选 B，必须先接受「单写队列会把 SELCHR/DELCHR/NEWCHR/5700 写排在慢存档后」的 `D-*`，或批准 B 的「角色表 lane + 索引表 lane」细化设计（工期和回归面更大）。
2. **连接池参数**：是否把 `max_connections(8)` 提到 100、`acquire_timeout` 从 30s 降到 15s 对齐 MySqlConnector。提到 100 会改变峰值连接数/资源占用；不提到 100 则 A 方案在 >8 并发取档时仍与 oracle 不同。需要 owner 明确。
3. **建角并发 race**：用 `create_gate` 线性化（推荐，登记 `D-*`）还是照抄 oracle 的 `Dictionary` 竞争（`T-*`，可能出现重复行/异常不回包）。本文建议前者，因为重复行是持久化污染。
4. **LB-04/LB-07 合并**：确认并入 G4 的验收矩阵，不另开卡；LB-06 必须按 §1.2 重写引用。
5. **LB-08 阈值**：`500ms`（QUERYCHR/SELCHR/LOAD P99）与 `1s`（另一角色 SAVE P99）是否 owner 接受；若 oracle 基线在窗口实测更高，按实测基线 + 固定倍率调整，并在 `review` 写明。
6. **W-DB-5**：`characters_ablity` RED 是否必须在同一窗口闭环，还是允许本卡先以「已承卡」状态通过；若后者，LB-10 不得 `closed`。
7. **W-DB-7**：TouchSocket 2.0.0-beta.273 跨连接是否真正并行；若实测同连接也会重入（improbable），则 6000 的 oracle 基线需重算，C6/C7 的并发假设要改。
8. **5700**：部署未加载 `MarketSystem.dll`，5700 当前不可达；A 方案仍建议一并改（代码一致性），但可把 5700 的窗口项降为抽样冒烟（L 级证据）并在 `review` 说明。若 owner 认为不可达面不必改，保留其全局锁会破坏「一把锁」的移除，必须显式登记。
9. **关服/缓存回写**：A 方案不改变「oracle 关服不 flush」（T-27）与缓存空转语义；不要在去锁时顺手加 flush，否则是新行为。

---

## 10. 附录：关键行号速查

### oracle

| 位置 | 内容 |
|---|---|
| `src/DBSrv/AppService.cs:37-56` | 三个监听面 Initialize/Start 顺序 |
| `src/DBSrv/Services/Impl/UserService.cs:30` | `Channel.CreateUnbounded<UserGateMessage>` |
| `src/DBSrv/Services/Impl/UserService.cs:76-96` | 单消息线程 |
| `src/DBSrv/Services/Impl/UserService.cs:200-235` | `ProcessGateData`；`Data` 入队 |
| `src/DBSrv/Services/Impl/UserService.cs:346-437` | 四命令限频/会话门 |
| `src/DBSrv/Services/Impl/UserService.cs:439-735` | `QueryChr`/`DeleteChr`/`NewChr`/`SelectChr` |
| `src/DBSrv/Services/Impl/DataService.cs:53-68` | 6000 `Received` 同步处理 |
| `src/DBSrv/Services/Impl/DataService.cs:88-149` | `ProcessMessagePacket`/`ProcessServerMsg` |
| `src/DBSrv/Services/Impl/DataService.cs:151-257` | `LoadHumanRcd`/`SaveHumanRcd` |
| `src/DBSrv/Services/Impl/MarketService.cs:71-95` | 5700 `Received` 同步处理 |
| `src/DBSrv/TimedService.cs:39-96` | 1s 定时器 / 5min 缓存回写 |
| `src/Storeages/DBSvr.Storage.MySQL/DBContext.cs:21-82` | 每 context 一条连接、事务、回滚 bug |
| `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:45-80` | 存档 8 段/事务 |
| `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:139-163` | `SaveAblity` 参数绑定在 try 外 |
| `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:206-211` | `SaveItem` 的 `userItems.Length` 在 try 外 |
| `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs:51-88` | 建角事务 |
| `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs:566-608` | `GetQryChar` |
| `src/Storeages/DBSvr.Storage.MySQL/PlayRecordStorage.cs:100-171` | 每方法 `Open` + `GetRecord` |
| `src/Storeages/DBSvr.Storage.MySQL/PlayRecordStorage.cs:230-263` | `Add`（先查 `_quickList` 再 INSERT 再 `Add`） |

### Rust（`e9c1e8654`）

| 位置 | 内容 |
|---|---|
| `main.rs:74-90` | `Arc<tokio::sync::Mutex<Db>>` 创建/spawn |
| `db.rs:77-99` | `Db`（池 + 三份索引） |
| `db.rs:146-156` | `max_connections(8)` |
| `db.rs:279/289/316/365/379/395/419/433/443/467/506` | 索引/账号/角色/建角方法 |
| `handler.rs:45-58` | `queue_loop` 每消息持全局锁 |
| `handler.rs:261-360` | `query_chr` |
| `handler.rs:405-500` | `new_chr` |
| `handler.rs:510-585` | `delete_chr`/`del_chr_sname_to_level` |
| `handler.rs:588-690` | `select_chr` |
| `data.rs:65-180` | 6000 监听/每连接读循环 |
| `data.rs:237-330` | `load_human_rcd`（`:277` 锁） |
| `data.rs:328-403` | `save_human_rcd`/`store_human_rcd`（`:380` 锁） |
| `data.rs:519-549` | 缓存回写（`:540` 锁） |
| `market.rs:231-300` | 5700 监听/每连接 |
| `market.rs:439/495/522` | 5700 存储锁 |
| `playdata.rs:777-890` | `save_record`（事务/8 段/回滚） |
| `playdata.rs:1080-1165` | `play_data_get/update/add` |

---

## 11. 残留物清单

- 本文为唯一新增文件；未建库、未起进程、未写 `/e/tmp` 之外的临时物。
- 本车道 worktree：`/e/tmp/wt-dbdesign`（分支 `db-conn-design`）；合并/删除由 coordinator 按收尾清单处理。
- 未触碰：`src/**`、`sql/**`、`MirClient/`、`mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`、`bins/game-svr/src/{chat,main}.rs`、`tools/**`、`docs/window-run-sheet*`、任何攻城设计文档。
