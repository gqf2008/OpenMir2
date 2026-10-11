# M1 收口（S4b）DBSrv 6000 `DataService` 面 + 5700 `MarketService` 面 + `TimedService` 缓存回写 —— 交付说明

> 卡：`openmir2-m1-dbsrv-6000-face`（issue `370e332f`）｜worktree `dbsrv6k`｜分支 `worktree-m1-dbsrv-6000`｜
> 各片 base = `refs/heads/master`（落笔时取，见 patch 条目）。
>
> | 片 | 内容 | 状态 |
> | --- | --- | --- |
> | ① S4a | `crates/storage` 记录字段 + C# oracle 字节向量 | 合入 master `1ba53292`（approve reviewer-6 `98b7978d`） |
> | ② 6000 / ④ TimedService | `DataService` 取档/存档 + 缓存回写 | 合入 master `96d03db5`（approve reviewer-6 `06097e75`） |
> | ③ 5700 `MarketService` | 拍卖行面（本片） | 本片交付待审 |
>
> 卡内 **跨实现互读 diff**（GameSvr 经 Rust DBSrv 6000 读档进图 + Rust/C# 双向互读）需真 C# GameSvr/DBSrv 对端
> ⇒ 按 owner `2026-10-11 18:55/23:55` 指令**等 M2 端口窗口**（本轮不起 C# 栈）。

---

## 1. 本片做了什么

| 文件 | 内容 |
| --- | --- |
| `bins/db-srv/src/data.rs`（新，~460 行） | 6000 监听与请求处理，逐条对齐 C# `Services/Impl/DataService.cs`：6B `ServerDataPacket` + MemoryPack `ServerRequestData`（`Message`/`Packet`/`Sign` 均 EDCode）；`MakeLong(queryId^170, packetLen)` 校验码与签名比对（`checkCode<=0` 或空 `Sign` 直接放行、不匹配关连接）；`DB_LOADHUMANRCD(100)`/`DB_SAVEHUMANRCD(101)`/`DB_SAVEHUMANRCDEX(102)`，其余 `DBR_FAIL`；取档先过 `CheckSessionLoadRcd`（会话号+账号，命中一次后置真）、成功回 `DBR_LOADHUMANRCD(1100) recog=1 series=1` + `LoadPlayerDataPacket`，失败回同 ident + 负码；存档成功回 `DBR_SAVEHUMANRCD(1102) recog=1`，失败回 `DBR_LOADHUMANRCD(1100) recog=0`（C# 原样）。另有 `cache_flush_task`（C# `TimedService` 的 5 分钟 `ProcessCacheStorage`）。 |
| `bins/db-srv/src/playdata.rs`（新，~1000 行） | 6000 面的 MySQL 存储层，1:1 移植 C# `Storeages/DBSvr.Storage.MySQL/PlayDataStorage.{Query,Save,Add}.cs`：`GetRecord`（主表 + `characters_ablity`/`bonusability`/`magic`/`item`/`bagitem`/`storageitem`/`status` 七张附属表）；`SaveRecord`（一个事务 8 个 `Save*`，各自吞错，只有 `SaveMagics` 的 `DELETE` 与 `Commit` 失败才回滚）；`Add`（完整 `CharacterData` 建角）；`Index/Get/Update`。**C# 既有 bug 逐条保留**：`SaveRecord` 的 `Job = Job`（职业不更新）、`characters_ablity.Ac = Abil.Level`、`Hp` 只写 `Abil.HP`（不写回 AC）、`@AllowGroupReCall` 收 `hd.AllowGroup`、`GetChrRecord` 不读 `PkPoint`、`GameGold` 与 `Gold` 同列、`QueryItemAttr` 的「逗号串当单参数 + 行序同时索引槽与 `Desc`」。 |
| `bins/db-srv/src/cache.rs`（新，~100 行） | C# `CacheStorageService`（`ICacheStorage`）：**`Add` 只在键已存在时替换**（既有怪行为，全仓无 `TryAdd` ⇒ 缓存恒空）、`Get/Delete/QueryCacheData`。 |
| `bins/db-srv/tests/live_data.rs`（新，进程级用例） | 伪 LoginSrv（推 `(100/账号/会话/…)`）+ 伪 GameSvr，架起真 Rust `db-srv`，对真 `mir2_db` 跑 **取档 → 改字段 → 存档 → 复读库列** 闭环；端口全部临时分配（不占 `E:/MirServer` 的 5000/5100/5500/5600/5700/6000/7000/7100/7200），临时目录按 pid。 |
| `bins/db-srv/src/{config,db,server,handler,loginsrv,main}.rs` | `DbConfig` 增 `ServerAddr/ServerPort`（缺项/`*`/0 端口 fail-fast）与 `MarketAddr/MarketPort`（仅解析）；`Db` 增 Unicode 名索引 `name_text`（C# `_NameQuickMap` 的 `OrdinalIgnoreCase`）与 `gbk_bytes`；`LoginState` 增 `load_rcd` + `CheckSessionLoadRcd`/`SetSessionSaveRcd`；`Server` 增 `cache`；`Db` 改为 `Arc<Mutex<Db>>`（5100 消息循环与 6000 面共用一个池）；`main.rs` 起 6000 监听 + 缓存回写任务，并把头部「不在本进程范围内」清单改为只剩 `MarketService`。 |

### 字节面口径（关键）

`message_bytes / packet / sign` 都先 `EDCode.DecodeBuff` 再 MemoryPack 反序列化；响应按 C# `SendRequest`/`SendRequest<T>`
设 `QueryId`、补空 `Packet`、算 checkCode、`Sign = EncodeBuffer(BitConverter.GetBytes(checkCode))`，
最后套 6B `ServerDataPacket`。`EDCode.EncodeBuffer` 在 `data.Length >= 10000` 时返回空数组（C# `BufferSize`），已 1:1 复刻。

### 片 3（本片）：5700 `MarketService`

| 文件 | 内容 |
| --- | --- |
| `bins/db-srv/src/market.rs`（新） | 5700 监听与请求处理，逐条对齐 C# `Services/Impl/MarketService.cs`：同 6B `ServerDataPacket` + MemoryPack `ServerRequestData` 信封，但**校验码语义分叉**（6000 对 `checkCode<=0`/空 `Sign` 放行，5700 回 `DBR_FAIL(2000)`）；`DB_LOADMARKET(100)`/`DB_SAVEMARKET(101)`/`DB_SEARCHMARKET(102)`/`DB_LOADUSERMARKET(109)` 四个 ident，其余不回包；`SendSuccessMessage` 的查询号**恒为 1**（C# 原样）。内嵌只服务本面的最小 MemoryPack 编解码（不动 `crates/storage` 冻结线）。 |
| `bins/db-srv/src/marketdata.rs`（新） | `IMarketStorage` 的 MySQL 实现（C# `Storeages/DBSvr.Storage.MySQL/MarketStoageService.cs`）：`QueryMarketItems` 的 `select a.*,b.* … left JOIN …`（**`if (dr.Read())` 只取第一行**、`Value{i}→Desc[i]`、`Desc[13]` 不写）、`QueryMarketItemsCount` **恒 0**、`SaveMarketItem` 的 INSERT（列名/参数逐字）、`Search*` 四个空桩。 |
| `bins/db-srv/tests/live_market.rs`（新，进程级用例） | 伪 LoginSrv + 伪 GameSvr，架起真 Rust `db-srv`，对**独立临时库**（`mir2_dbsrv6k_mktlive_<pid>`，跑完 DROP；**不碰 `mir2_db`**）验证四个 ident 的应答/不回包/关连接。 |
| `tests/parity/oracle/Program.cs` + `vectors/oracle_vectors.jsonl` | 新增 3 条 `kind:market` C# 真身向量（`MarketRegisterMessage`/`MarketSearchMessage`/`MarkerUserLoadMessage`）；`crates/protocol/tests/oracle_vectors.rs`、`tools/replay/src/main.rs` 的 skip 分支同步加 `"market"`（未知 kind 仍必红）。 |

**C# 5700 面大半不可达（真身探针已证，非本实现遗漏）**：`MarketItem` 未注册 MemoryPack，
`Serialize`/`Deserialize` 只要碰到它就抛 `MemoryPackSerializationException`。因此：
- `DB_LOADMARKET` 查得非空 ⇒ C# 构造 `MarketDataMessage` 时抛 ⇒ **不回包**（Rust 同）；
- `DB_SAVEMARKET` 的 `Deserialize<MarketSaveDataItem>` 只要成员数 ≥ 1 就抛（仅空对象头 `0x00` 可解，全默认 ⇒ `GroupId==0` 早退）⇒ **恒不回包**（Rust 同）；
- `DB_SEARCHMARKET` 的 `Search*` 全空桩 ⇒ 恒 `DB_SRARCHMARKETFAIL(104)`；
- `DB_LOADUSERMARKET`（`GroupId!=0`）⇒ `QueryMarketItemsCount` 恒 0 ⇒ `DB_LOADUSERMARKETSUCCESS(110)` + `MarkerUserLoadMessage`。

`MarkerUserLoadMessage` 是**全非托管 struct**，MemoryPack 按**原始内存布局**序列化（无成员数对象头，含 3 字节尾部填充，共 12B：`SellCount(i32) MarketNPC(i32) IsBusy(u8) pad(3)`）—— 这点与含字符串的 `MarketRegisterMessage`/`MarketSearchMessage`（有对象头）不同，
由真身探针 + `kind:market` 向量钉住。

## 2. 判据与证据（四件套：命令 + 输出 + 路径 + hash）

- **进程级闭环（真 MySQL）**：`cargo test -p db-srv --test live_data --offline -- --ignored --nocapture` ⇒
  `1 passed`；取档 `DBR_LOADHUMANRCD(1100) recog=1 series=1`、存档 `DBR_SAVEHUMANRCD(1102) recog=1`，
  并断言库列按 C# 既有 bug 落库（`Job` 不变、`AllowGroupReCall` 收到 `AllowGroup`、`Ac` 收到 `Level`、`Hp` 只写 HP）。
  原始输出：`tests/parity/evidence/M1/s4b-slice2-livedata.txt`。
- **四门禁（隔离 target `E:\tmp\openmir2-dbsrv6k-target`，`--offline`）**：`tests/parity/evidence/M1/s4b-slice2-gates.txt`
  - `cargo fmt --all -- --check` → exit 0
  - `cargo clippy --workspace --all-targets --offline -- -D warnings` → exit 0
  - `cargo test --workspace --offline` **3 连跑** → 每次 **69 suites / 494 passed / 0 failed / 14 ignored**
    （相对本片 base 复核 = 68 suites / 483 passed / 0 failed / 13 ignored；本片 **+1 suite（live_data）/ +11 用例**：
    `data.rs` 内联 5 + `playdata.rs` 内联 3 + `cache.rs` 内联 3；另 `config.rs` 用例同步扩充）
  - `cargo run -p msgcodegen --offline -- --check` → 604 常量（`Messages.cs` 594 + Grobal2 白名单 10）
- **红检（新牙齿）**：`tests/parity/evidence/M1/s4b-slice2-redcheck.txt`
  - 把 `save_characters` 的 `Job = Job` 后**误绑的 `.bind(d.job)` 放回**（这会整体错位、`WHERE ID` 错绑成 0 ⇒ 静默 0 行更新）
    ⇒ live_data `FAILED`：`assertion left 5555 right 7777`（存档应答仍 `recog=1`，正是「静默不落库」的形态）；
  - 去掉该误绑 ⇒ 同一用例 `ok`。这条红检是本片最有价值的牙齿：它证明用例能抓住「信封绿、落库假绿」。
- **文件 hash**（sha256，交付树）：
  - `data.rs = 49c5c3ff…bd99`
  - `playdata.rs = 572183ff…bddd`（含红检后还原 + 与 C# 一致的 `保存角色[…]数据成功` Debug 行）
  - `cache.rs = 554a12f6…800c`
  - `live_data.rs = 53f9514e…8b59`
  - `db.rs = c1ac43b0…0213`、`config.rs = 5a420f68…c7a`、`server.rs = 2574afbe…cfa`、
    `handler.rs = 0e12e736…a805`、`loginsrv.rs = 759e9b5f…b135`、`main.rs = ac1e7d43…4dfb`
  - `s4b-slice2-gates.txt = 13a3a092…84a9`、`s4b-slice2-redcheck.txt = 735974a7…f7a3`、`s4b-slice2-livedata.txt = 05d20f4e…af9e`

### 片 3（5700）证据

- **C# 真身探针**（`MarketItem` 不可序列化 / 全非托管 struct 布局 / 空对象头）：
  `tests/parity/evidence/M1/s4b-slice3-market-probe.txt`（含 Program.cs 与原始输出）。
- **进程级 5700 行为**：`cargo test -p db-srv --test live_market --offline -- --ignored --nocapture` ⇒ `1 passed`，
  断言：`DB_SEARCHMARKET` gid=0 ⇒ 104 recog=0 param=0、gid=2 ⇒ 104 param=1；`DB_LOADUSERMARKET` gid=2 ⇒ 110 **查询号 1** recog=99 + 12B 体；`DB_LOADMARKET` 空表 ⇒ 106 param=1、有行/非法 Register ⇒ 不回包；`DB_SAVEMARKET` ⇒ 不回包；`Sign` 缺失 ⇒ `DBR_FAIL(2000)`；签名不匹配 ⇒ 关连接。
  输出：`tests/parity/evidence/M1/s4b-slice3-market.txt`。
- **存储层（独立临时库，不碰 `mir2_db`）**：`cargo test -p db-srv --offline --bin db-srv -- --ignored marketdata::` ⇒ `1 passed`，
  断言 `QueryMarketItems` **只回第一行**、`SaveMarketItem` INSERT 落库、`QueryMarketItemsCount` 恒 0、`SearchMarketItems` 恒空。
- **四门禁（隔离 target）**：`tests/parity/evidence/M1/s4b-slice3-gates.txt`
  - `cargo fmt --all -- --check` → exit 0
  - `cargo clippy --workspace --all-targets --offline -- -D warnings` → exit 0
  - `cargo test --workspace --offline` **3 连跑** → 每次 **70 suites / 506 passed / 0 failed / 16 ignored**
    （相对片 2 合并树 = 69/502/0/14；本片 **+1 suite（live_market）/ +4 用例（market 单测）/ +2 ignored**）
  - `cargo run -p msgcodegen --offline -- --check` → 604 常量
  - `cargo run -p replay --offline -- tests/parity/vectors/oracle_vectors.jsonl` → `RESULT: GREEN`（外部 kind 向量 17 = 14 playerdatainfo + 3 market）
- **红检**（同文件）：把 `MarkerUserLoadMessage::to_bytes` 错误地补上成员数对象头 ⇒
  `marker_user_load_true_bytes` 与 `live_market` 的 12B 体断言都红；把 `QueryMarketItems` 改成返回所有行 ⇒
  `marketdata` 只取第一行断言红（left 2 right 1）。还原后均绿。
- **文件 hash**（sha256，交付树）：`market.rs = 0069aec5…5453`、`marketdata.rs = 8110ce3a…7965`、`live_market.rs = e4ddc58e…e8cd`；
  向量 `oracle_vectors.jsonl = 42db7bc4…5eba`（LF 归一）、`Program.cs = 0acc6c52…9f3b`、
  `crates/protocol/tests/oracle_vectors.rs = ed67a93f…98b8`、`tools/replay/src/main.rs = 677d7d0c…d49bc`、
  `data.rs = d95ff73f…ac95`、`main.rs = 4470570d…ce97`。

## 3. 与 C# 的对照口径

- `DataService.cs` 逐分支对读：`ProcessMessagePacket` 的 `packetLen>=DefBlockSize && queryId>0 && Packet!=null && Sign!=null`、
  `checkCode<=0`/空 `Sign` 放行、签名不匹配 `client.Close()`、`LoadHumanRcd` 的会话校验与 `-1/-2/-3` 负码、
  `SaveHumanRcd` 的 `bo21` 语义与 `SetSessionSaveRcd` 时机、`SaveHumanRcdEx` 直通，均逐条核对。
- `PlayDataStorage.{Query,Save,Add}.cs` 的 SQL 文本、参数绑定、位运算（`HUtil32.LoWord/HiWord`）、
  `ComparerUserItem` 的四分支，均逐条核对；`live_data` 的断言即取自这些分支中的可观测点。
- `get_chr_record` **不读** `PkPoint`（C# `GetChrRecord` 读集里没有），`live_data` 断言 `pk_point == 0`。

## 4. 卡内剩余范围

| 项 | 内容 | 状态 |
| --- | --- | --- |
| ① S4a | `crates/storage` 记录字段 + C# oracle 向量 | 已合入 master `1ba53292` |
| ② 6000 `DataService` | 取档/存档面 | 已合入 master `96d03db5` |
| ④ `TimedService` | 缓存回写（5 分钟） | 已合入 master `96d03db5` |
| ④ 5700 `MarketService` | 拍卖行面（监听 + 四个 ident + `IMarketStorage`） | **本片实现**（`market.rs`/`marketdata.rs` + 进程级用例）；C# 侧的不可达分支已逐条登记（whitelist `S4B-5`…`S4B-9`） |
| ③ 跨实现互读 diff | 小退/掉线/服务关闭三路径，Rust↔C# 双向互读 | **部分完成（掉线路径）**："C# GameSvr 经 Rust DBSrv 6000 读档进图"已证；跨实现存档字段级 diff = **0**（`disconnect` 路径，两侧同 C# GameSvr、只换 DBSrv）；Rust↔C# 双向互读已证。`softclose`/`shutdown` 两路径未覆盖（登记）。证据 `tests/parity/evidence/M1/s4b-slice4-crossread/`（2026-10-12 M2 释放窗口后）。 |

> ④ 的 `TimedService` 缓存回写已在片 2 完成（`data::cache_flush_task`，5 分钟周期 + 日志两条与 C# 一致；
> 10s 重连 LoginSrv / 7s 上报由既有 `loginsrv` 承担）。缓存恒空是 C# 既有行为，已由单测钉住。
> C# `TimedService` 里的 `PushMarketData` 调用**被注释**（`//_marketService.PushMarketData();`）⇒ 不实现周期推送。

## 5. 接口冻结线通知（§12.4：B2/D2/M2）

- 片 2 **未动** `crates/storage` / `crates/protocol`（记录类型与字节面沿用 S4a）。
- 片 3 5700 面 **亦未动** `crates/storage`（market 的 MemoryPack 编解码最小实现内嵌在 `bins/db-srv/src/market.rs`，
  避免触碰冻结线）；仅共用向量文件新增 `kind:market`，因此同步改了 `crates/protocol/tests/oracle_vectors.rs`（**测试**）
  与 `tools/replay/src/main.rs`（**工具**）的 skip 分支（未知 kind 仍必 panic/bail）。已向 B2/D2/M2 留签名 `comment` 说明。

## 6. 已知偏差登记（见 `tests/parity/whitelist.md` §M1 S4b）

- `S4B-1`：附属表读异常时 C# 吞掉并返回「已读部分」，Rust 整份取档判失败（正常数据不可达）。
- `S4B-2`：`QueryItemAttr` 空槽回写 C# NRE vs Rust 静默跳过（脏数据才可达）。
- `S4B-3`：`Sign` 长度 < 2 时 C# 抛异常 vs Rust 按 0（畸形帧才可达；6000/5700 面共用）。
- `S4B-4`：缓存键大小写折叠近似（缓存恒空 ⇒ 不可观测）。
- `S4B-5`…`S4B-9`：5700 面的不可达/空桩/怪行为（详见 `tests/parity/whitelist.md`）。

## 7. 残留物清单

- 证据（入库）：`tests/parity/evidence/M1/s4b-slice2-{gates,livedata,redcheck}.txt`、
  `s4b-slice3-{gates,market,market-probe}.txt`（上一片证据 `s4a-oracle-vectors.txt`、`s4b-gates.txt` 仍在）。
- `E:\tmp\openmir2-dbsrv6k-target`：隔离 target 目录（保留，复跑门禁用）。
- `E:\tmp\openmir2-dbsrv6k-wip\`：上一实例 WIP 备份（owner 留档，未删）。
- `E:\tmp\market-probe\`：本轮 C# 真身探针临时工程（源已入 `s4b-slice3-market-probe.txt`；可删）。
- 已清理：live_data / live_market 用例的临时目录（`%TEMP%\a2-dbsrv-6000-*` / `a2-dbsrv-5700-*`）随用例收尾删除；
  临时库 `mir2_dbsrv6k_mkt*`（marketdata 单测 + live_market）在用例内 `DROP DATABASE`；`mir2_db` 未被写入（差异：片 2 的 live_data 夹具行已清）。
- 未改：`E:/MirServer`、表结构、脚本语法、数据格式；未起任何 C# 服务进程；未占用现网端口
  （`live_data`/`live_market` 全部临时端口）。收工端口状态见 `E:\tmp\openmir2-watch\last-dbsrv6k.md`。

---

## 8. 片 4（③ 跨实现互读，2026-10-12）

M2 lane 在卡上写 `comment c365f0af`「窗口已释放」（5000/5100/5500/5600/5700/6000/7000/7100/7200 全空）后，
本片起真机栈做 ③，**无 Rust 代码改动**（纯证据；四门禁 70/507/0/16 + 604）。

栈固定「**同一 C# GameSvr + 同一 C# LoginSrv/网关**，只换 DBSrv（C# ↔ Rust）」⇒ diff 唯一归因到 DBSrv。

| 判据 | 结果 | 证据（`tests/parity/evidence/M1/s4b-slice4-crossread/`） |
| --- | --- | --- |
| C# GameSvr 经 Rust DBSrv **6000** 读档进图 | ✅ BotSrv 结构化汇总 `in_world=1`；Rust 日志 `获取玩家[loadbot0]数据成功` + `保存角色[loadbot0]数据成功` | `01-botsrv-loadbot0.summary.json`、`01-rust-dbsrv-6000-loadbot0-load-save.txt` |
| 存档字段级 diff = 0（`disconnect` 路径，跨实现） | ✅ `scoped_diff.py diff --canonical --fixed-newchr` ⇒ `GREEN: 作用域内 9 张表逐行一致`；两侧各 115 行 / 9 表分布逐表相等 | `02-*`、`03-*`、`04-*`、`09-*` |
| C# 写 → Rust 读 | ✅ BotSrv 结构化汇总 `in_world=1`（`m1c0214020`）；6000 归属 = Rust db-srv（端口归属推断，见下注） | `07-botsrv-csharp-writes-rust-reads.summary.json`、`07-*.txt`、`06-*`、`10-*` |
| Rust 写 → C# 读 | ✅ BotSrv 结构化汇总 `in_world=1`（`m1r0218390`）；C# DBSrv 侧原始件 02:20:06 读到 `m1r0218390` | `08-botsrv-rust-writes-csharp-reads.summary.json`、`08-*.txt`、`06-csharp-dbsrv-stdout.txt`、`05-*` |

> **F-1（reviewer-3 `39d41840`，request_changes）已处置**：07 方向此前引用的「Rust 日志 `获取玩家[m1c0214020]数据成功`」
> **未随片且不可复得**（服务该次读取的 Rust db-srv 的 live 日志 `DBSrv.out.log` 在 02:19:34 C# 栈启动时被截断）。
> 已补入三份 `run_bots.ps1` **结构化汇总原始件**（`01/07/08-*.summary.json`，均 `in_world=1`），
> 并把 07 方向改为「BotSrv 汇总 `in_world=1` + 端口归属推断（`06-*` 显示 C# DBSrv 02:19:34 才启动，
> 晚于 07 的 02:19:11–02:19:26；`10-*` 显示交接点停掉的是 Rust `db-srv pid=25948`）」；
> 「Rust 6000 侧对 `m1c0214020` 的取档日志未随片」登记 `not_verified`（见同目录 `README.md` §5）。

未覆盖（登记）：`softclose`（需 GUI 客户端 `Alt+X` 模态确认框）与 `shutdown`（需 bot 在世界同刻优雅停 GameSvr）
两条退出路径的 DB diff —— 本片窗口内未做，见同目录 `README.md` §5。收工已停栈、端口回落。
