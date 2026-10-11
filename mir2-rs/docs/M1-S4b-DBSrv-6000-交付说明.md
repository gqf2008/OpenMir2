# M1 收口（S4b）DBSrv 6000 `DataService` 面 + `TimedService` 缓存回写 —— 交付说明

> 卡：`openmir2-m1-dbsrv-6000-face`（issue `370e332f`）｜worktree `dbsrv6k`｜分支 `worktree-m1-dbsrv-6000`｜
> 本片 base = `refs/heads/master`（落笔时取，见 patch 条目）。
>
> 卡内 ①（S4a：`crates/storage` 记录字段 + C# oracle 字节向量）已在上一片合入 master `1ba53292`；
> 本片做 **② 6000 `DataService` 面**与 **④ 的 `TimedService` 缓存回写**。
> 卡内 **③ 跨实现互读 diff** 需真 C# GameSvr/DBSrv 对端 ⇒ 按 owner `2026-10-11 18:55/23:55` 指令**等 M2 端口窗口**（本轮不起 C# 栈）；
> **5700 `MarketService`** 按 ④「可拆子片」拆出，未实现，逐条登记见 §4。

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

## 3. 与 C# 的对照口径

- `DataService.cs` 逐分支对读：`ProcessMessagePacket` 的 `packetLen>=DefBlockSize && queryId>0 && Packet!=null && Sign!=null`、
  `checkCode<=0`/空 `Sign` 放行、签名不匹配 `client.Close()`、`LoadHumanRcd` 的会话校验与 `-1/-2/-3` 负码、
  `SaveHumanRcd` 的 `bo21` 语义与 `SetSessionSaveRcd` 时机、`SaveHumanRcdEx` 直通，均逐条核对。
- `PlayDataStorage.{Query,Save,Add}.cs` 的 SQL 文本、参数绑定、位运算（`HUtil32.LoWord/HiWord`）、
  `ComparerUserItem` 的四分支，均逐条核对；`live_data` 的断言即取自这些分支中的可观测点。
- `get_chr_record` **不读** `PkPoint`（C# `GetChrRecord` 读集里没有），`live_data` 断言 `pk_point == 0`。

## 4. 卡内剩余范围（逐条登记，未做）

| 项 | 内容 | 状态 / 阻塞 |
| --- | --- | --- |
| ③ | 存档字段级 diff = 0（小退/掉线/服务关闭三路径，跨实现互读） | **未做**：需真 C# GameSvr/DBSrv 做对端（独占端口窗口）。按 owner `2026-10-11 18:55/23:55` ⇒ **等 M2 验收释放端口窗口**；本轮只做不占端口的实现 + 伪对端进程级用例。 |
| ④-5700 | 5700 `MarketService`（拍卖行） | **未实现**（可拆子片，登记见 whitelist `S4B-5`）。`config.rs` 已按现网键名解析 `MarketAddr`/`MarketPort`（现网 `dbsvr.conf` 无此键，C# 默认值 = Rust 解析值）。承接：`IMarketStorage` 的 MySQL 实现 + 5 个 handler。 |

> ④ 的 `TimedService` 缓存回写**已在本片完成**（`data::cache_flush_task`，5 分钟周期 + 日志两条与 C# 一致；
> 10s 重连 LoginSrv / 7s 上报由既有 `loginsrv` 承担）。缓存恒空是 C# 既有行为，已由单测钉住。

## 5. 接口冻结线通知（§12.4：B2/D2/M2）

本片**未动** `crates/storage` / `crates/protocol`（记录类型与字节面沿用上一片已合入的 S4a）。
改动全部落在 `bins/db-srv/**` + `tests/parity/evidence/M1/**`。仍在 B2/D2/M2 线留 `comment` 说明本片范围。

## 6. 已知偏差登记（见 `tests/parity/whitelist.md` §M1 S4b）

- `S4B-1`：附属表读异常时 C# 吞掉并返回「已读部分」，Rust 整份取档判失败（正常数据不可达）。
- `S4B-2`：`QueryItemAttr` 空槽回写 C# NRE vs Rust 静默跳过（脏数据才可达）。
- `S4B-3`：`Sign` 长度 < 2 时 C# 抛异常 vs Rust 按 0（畸形帧才可达）。
- `S4B-4`：缓存键大小写折叠近似（缓存恒空 ⇒ 不可观测）。
- `S4B-5`：5700 `MarketService` 未实现。

## 7. 残留物清单

- 证据（入库）：`tests/parity/evidence/M1/s4b-slice2-{gates,livedata,redcheck}.txt`
  （上一片证据 `s4a-oracle-vectors.txt`、`s4b-gates.txt` 仍在）。
- `E:\tmp\openmir2-dbsrv6k-target`：隔离 target 目录（保留，复跑门禁用）。
- `E:\tmp\openmir2-dbsrv6k-wip\`：上一实例 WIP 备份（owner 留档，未删）。
- 已清理：live_data 用例的临时目录（用例级 `%TEMP%\a2-dbsrv-6000-*`）随用例收尾删除；
  数据库里的夹具行（含前几轮失败运行遗留的 `zz6k%`）已清空，无残留。
- 未改：`E:/MirServer`、表结构、脚本语法、数据格式；未起任何 C# 服务进程；未占用现网端口
  （`live_data` 全部临时端口）。收工端口状态见 `E:\tmp\openmir2-watch\last-dbsrv6k.md`。
