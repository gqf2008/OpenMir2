# A2/M1：Rust 版 DBSrv（5100 面）交付报告

> 卡片：`openmir2-a2-m1-services`（owner `openmir2-worker-2`，worktree `a2-dbsrv`，分支 `worktree-a2-dbsrv`）。
> 范围：**DBSrv 的 5100 面**（SelGate ↔ DBSrv 内部链路 + 角色列表/建角/删角/选角经 sqlx 接 `mir2_db`）
> 与 **DBSrv → LoginSrv(5600) 的会话链**。**不含** 6000 `DataService` 面（GameSvr 取档）与 5700 拍卖行面，见 §5。

## 1. 交付物

| 文件 | 内容 |
| --- | --- |
| `mir2-rs/bins/db-srv/src/main.rs` | 启动编排（配置 → 配置表 → DB → 监听；缺一即明确报错退出） |
| `mir2-rs/bins/db-srv/src/config.rs` | `dbsvr.conf` 读取（字节级 INI，镜像 C# `ConfigFile` + `SettingsModel` 默认值） |
| `mir2-rs/bins/db-srv/src/gbk.rs` | GB2312/GBK **字符边界**原语（`Split`/`Length`/`ArrestStringEx`/`StrToInt` 的等价复现） |
| `mir2-rs/bins/db-srv/src/route.rs` | `DBShare` 等价物（`AddrTable.txt`/`SelectID.txt`/`ServerInfo.txt`/`MapInfo.txt`/`DenyChrName.txt`） |
| `mir2-rs/bins/db-srv/src/db.rs` | `PlayRecordStorage` + `PlayDataStorage`（5100 面用到的部分），内存索引 + sqlx/MySQL |
| `mir2-rs/bins/db-srv/src/log.rs` | 与 C# Serilog 同形的控制台日志（`ShowDebug` 同开关） |
| `mir2-rs/bins/db-srv/src/loginsrv.rs` | `ClientSession` 等价物（主动连 5600，收 `100/101/104`，发 `103` 保活与 `102` 软下线） |
| `mir2-rs/bins/db-srv/src/server.rs` | 5100 监听、槽位表（20）、内部帧收发、`Enter/Leave/KeepAlive` 就地处理 + `Data` 单消费者队列 |
| `mir2-rs/bins/db-srv/src/handler.rs` | `DeCodeUserMsg` 的四个命令（`QUERYCHR/NEWCHR/DELCHR/SELCHR`）+ 限频/会话校验/回包 |
| `mir2-rs/bins/db-srv/src/user.rs` | 5100 面**纯字节层**（抠帧、头/体切分、请求解析、应答组帧）——可单测的部分 |
| `mir2-rs/bins/db-srv/tests/live_querychr.rs` | **进程级混跑验收**：伪 LoginSrv + 伪 SelGate / C# DBSrv oracle / C# SelGate 三个场景 |
| `mir2-rs/docs/M1-边界服务移植计划.md` | 切片表更新（S4 ✅、S4b 登记为下一片） |
| `mir2-rs/tests/parity/whitelist.md` | 新增 **A-10**（两处授权偏离 + `PlayQuickList` 等价化） |

## 2. 判据与证据

### 2.1 硬门禁（本线全绿）

```powershell
cargo fmt --all -- --check                     # rc=0
cargo clippy --workspace --all-targets --offline -- -D warnings   # rc=0
cargo test --workspace --offline               # 全部 ok（db-srv 22 单测 + 3 个 #[ignore] 混跑用例）
cargo run -p msgcodegen --offline -- --check    # OK: 604 条常量与 C# 源一致
```

日志：`E:\tmp\a2-dbsrv\evidence\gate.log`（fmt/clippy/test/msgcodegen 四件套的输出）。

### 2.2 ②「跨实现互读：字段级 diff = 0」——**已取证**

做法：把 DBSrv 的 5100 面**单独**架起来（伪 LoginSrv 推会话 + 伪 SelGate 发命令），
分别对 **C# DBSrv**（`E:\tmp\a2-dbsrv\oracle-dbsrv`＝`src/DBSrv/bin/Release` 的副本）与
**Rust db-srv** 跑同一份脚本，逐字节比对同一金标准。

```powershell
cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture csharp_dbsrv_querychr_matches_csharp_golden
cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture rust_dbsrv_querychr_matches_csharp_golden
```

两侧输出（`evidence\oracle-csharp-run4.log`、`evidence\rust-dbsrv-run2.log`）**逐条相同**：

```text
[querychr]           head={recog:2, ident:520, param:0, tag:1, series:0} body="*aaa/0/1/1/0/bbb/2/1/1/0/"
                     raw=236664686b64646a696464696b6464686b5e6d49736d5f58685f613f5f615f58685f6e4a6f6e5f5a685f613f5f615f58685f3c21
[newchr ab]          head={recog:0, ident:522, ...}     # SM_NEWCHR_FAIL(0)
[newchr aaa]         head={recog:2, ident:522, ...}     # 重名 → 2
[newchr zzz]         head={recog:3, ident:522, ...}     # 账号已满 2 个 → 3
[delchr gm01]        head={recog:0, ident:523, ...}     # SM_DELCHR_SUCCESS（账号不匹配，不写库）
[querychr-bad-session] head={recog:0, ident:527, ...}   # 会话校验失败 → SM_QUERYCHR_FAIL
```

金标准（`GOLDEN_QUERYCHR_FRAME_HEX`）**先在 C# DBSrv 上捕获**再写进测试，因此这条判据不是自洽往返：
改 Rust 侧任一字段（职业/发型/等级/性别/顺序/`*` 前缀/头字段）都会立刻变红。
`raw=` 的十六进制即两侧的**下行原始字节**。

### 2.3 ③「失败要响」——**已取证**（退出码 + 明确报错）

| 场景 | 命令 | 退出码 | 报错（末行） |
| --- | --- | --- | --- |
| 缺配置文件 | `db-srv.exe <不存在.conf>` | 1 | `读取配置文件失败：…（M1④：缺配置必须明确报错退出）` |
| 存储插件非 MySQL | `[DataBase] Storeage=Sqlite` | 1 | `存储策略 Sqlite 未实现（C# 找不到 DBSrv.Storage.Sqlite.dll 即启动失败…）` |
| 连不上库（库不存在） | `database=no_such_db_xyz` | 1 | `1049 (42000): Unknown database 'no_such_db_xyz'` |
| 表缺失/缺列 | `database=mir2_account` | 1 | `1146 (42S02): Table 'mir2_account.characters_indexes' doesn't exist` |
| 端口被占 | `GatePort=5100`（被占时） | 1 | `10048 … 每个套接字地址只允许使用一次` |

日志：`evidence\fail-{missing-conf,storeage,baddb,missing-table,port-busy}.log`。
注：`ShowDebug` 与 C# 同开关（现网 bin 版 conf `ShowDebug=0` ⇒ 不打 Debug）；C# 侧另有
`appsettings.json` 的 Serilog `MinimumLevel=Information` 把 Debug 也滤掉，两侧日志行集因此可比（见 §2.5 附带证据）。

### 2.4 ④「进程级查询自测」——**已取证**

见 §2.2：`rust_dbsrv_querychr_matches_csharp_golden` 会真的起 `db-srv` 进程（伪 LoginSrv 推会话、
伪 SelGate 走 `Enter → QUERYCHR/NEWCHR/DELCHR`），断言四个命令的**字节级**应答。
同一条用例对 C# DBSrv 也能跑通（`csharp_dbsrv_querychr_matches_csharp_golden`）。

### 2.5 ①「C# SelGate(7100) → Rust DBSrv(5100)」——**已跑通**

做法：伪 LoginSrv 推会话给 Rust db-srv(5100)（因此不需要真 LoginGate/客户端），
**C# SelGate 副本**（`E:\tmp\a2-dbsrv\oracle-selgate`＝`E:\MirServer\SelGate` 的副本，配置指向 5100）
监听 7100，脚本客户端直接连 7100 发 `CM_QUERYCHR`：

```powershell
$env:MIR2_ORACLE_SELGATE_DIR='E:\tmp\a2-dbsrv\oracle-selgate'
cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture csharp_selgate_to_rust_dbsrv
# test result: ok. 1 passed ... finished in 8.56s
```

两侧日志时间线（`evidence\mixed-selgate-run5.log` 同一条证据里）：

```text
[C# SelGate] 09:36:41 登陆网关[127.0.0.1:7100]已启动.
[C# SelGate] 09:36:41 数据库服务器[127.0.0.1:5100]链接成功.
[C# SelGate] 09:36:43 用户[127.0.0.1]分配到数据库服务器[0] Server:tcp://127.0.0.1:5100/
[Rust db-srv] 09:36:41 玩家数据网关服务[0.0.0.0:5100]已启动.等待链接...
[Rust db-srv] 09:36:43 角色网关[2](127.0.0.1:62029)已打开...            ← C# SelGate 的接入
[Rust db-srv] 09:36:44 [QueryChr] 候选 account=mir2test i=0 name=[aaa] quickIndex=12218 …
断言：[csharp-selgate→rust-dbsrv] head={recog:2, ident:520, param:0, tag:1, series:0}
      body="*aaa/0/1/1/0/bbb/2/1/1/0/"，raw 与 §2.2 的金标准**逐字节相同**
```

> 过程留痕：09:15–09:33 这两个端口被**并行 A 线 M3-Rust 首验**的 C# 组件占用
> （`5100=37084 DBSrv.exe`、`7100=32604 SelGate.exe`），不能停别人的进程；A 线实验收工后端口空出，
> 本用例随即跑通（C# SelGate 的监听端口是编译期常量 `src/SelGate/GateShare.cs:15`，影子副本也改不了，
> 所以只能等窗口——这也是它必须独占 7100 的原因）。

**附带证据：两侧 `[QueryChr]` 日志排序后逐行相同**（各 9 行，`evidence\logdiff-{csharp,rust}-querychr.txt`）——
说明不只是回包字节一致，逐角色的读库决策序列也一致。

**长连接面（心跳）**：C# SelGate 每 10s 往上游发一次 `SendKeepAlive`，其帧是
`new ServerDataMessage { Type = KeepAlive }`（`SocketId`/`Data` **均为 null**），线上字节实测为
`04 03 ffffffff 0000 ffffffff`（`evidence\mixed-selgate-keepalive.log` 里由本实现原样打出）。
MemoryPack 把 null 编码成长度 `-1`，而本分支的 `crates/protocol::internal`（M0 冻结口径）把 `-1`
当非法长度 ⇒ 该帧被丢弃、**心跳无人应答**（C# DBSrv 会应答）。修法：
`server::normalize_memorypack_nulls` 在解析前只把整字段 `0xFFFFFFFF` 写成 0（其余字节原样，
不动冻结接口；worker-1 已就同一问题在 `crates/protocol/src/internal.rs` 提出修复（同卡 patch 67577a7f），
两处可共存、互不冲突）。用例随后等到过一个心跳周期（+12s）再查一次角色列表，并断言 db-srv 日志
出现 `响应角色网关心跳...`、不出现 `解析角色网关封包出现异常`（`evidence\mixed-selgate-keepalive3.log`）。

### 2.6 判据覆盖边界（如实记录）

- **建角成功路径**（`NEWCHR` → 写 `characters` + 5 张附属表）**没有被真机判据覆盖**：
  它会在**共享的现网库** `mir2_db` 里留下角色/物品/属性行，而本仓的既有纪律是实库测试只读
  （见 `crates/storage/tests/account_live.rs` 的注释）。因此该路径只做**逐列对照 C# 源码**的核对
  （列名/值/顺序/可空性，含 C# 的 `@Ac ← hd.Abil.Level` 既有 bug；`MapName`/`HomeMap`/`MasterName`/
  `DearName`/`StoragePwd` 在 C# 绑未赋值字符串 ⇒ **NULL**，本实现同）。
  `NEWCHR` 的**三条拒绝分支**（0 名字过短 / 2 重名 / 3 账号已满 2 个）与 `DELCHR`（含
  "账号不匹配仍回 SUCCESS"的 `bocheck` 怪行为）都已与 C# 逐字节对齐（§2.2）。
- `SELCHR`（选角）**未纳入 §2.2 的字节对拍**：它会写 `characters_indexes.SelectID`（改库），
  且回包里的路由来自 `ServerInfo.txt`（现网 1 条路由 ⇒ 端口是常量）。该路径的逻辑与
  `SelectChr` 逐行对应（见 §3.9），但真机取证留给 ① 的整链（届时客户端/脚本会顺带跑到）。

## 3. 行为对齐要点（逐条对 C# 源）

1. **接入**：`AddrTable.txt` 白名单（不在表里直接断开）；槽位号＝连接号-1，数组 20（第 21 条连接按
   C# 越界语义拒绝）。
2. **帧**：6B `ServerDataPacket`(`PacketCode` 校验) + MemoryPack `ServerDataMessage`；
   `DataLen` 是独立字段（越界在 C# 抛异常 ⇒ 这里丢弃该条）。
3. **抠帧**：按会话累积 `sText`，一次 `Data` 最多抠一帧；抠出的内容**丢掉首字符**（客户端帧的 `1`），
   前 16 字符＝编码头、其余＝编码体（`#1…!` 与 `!$` 帧尾都不影响本段语义）。
4. **四个命令**：`QUERYCHR` 200ms 限频、`NEWCHR/DELCHR` 1000ms、`SELCHR` 需先查过角色；
   `NEWCHR/DELCHR/SELCHR` 都要过会话校验，不过回 `SM_OUTOFCONNECTION`。
5. **角色列表**：`characters_indexes`（账号→角色，`IsDeleted<2`）逐条 `GetBy` **复核是否已删**，
   再按角色名查 `characters.Id` → `GetQryChar`（`Job/Hair/Sex/Level`），串成
   `名字/职业/发型/等级/性别/`，`SelectID==1` 前面加 `*`，**最多 2 个**。
6. **两张表的编号分开**：`characters_indexes.Id`（索引/`SelectID` 标记）与 `characters.Id`（存档）
   是两套编号（现网实测 6229 ↔ 6226），代码里没有任何混用。
7. **建角**：插 `characters_indexes`（`@SelectID` 绑的是 `PlayerRecordData.Selected`＝恒 0，
   不是网关 ID —— C# 的 `Header.SelectID` 只进内存索引）；再插 `characters` + `characters_ablity`
   + `characters_status` + `characters_item`(13) + `characters_bagitem`(46) + `characters_storageitem`(50)，
   同一事务；附属表失败 C# 吞掉，这里同样只记日志。
8. **删角**：`Index → Get → 账号比对 → 等级比对（`DELMaxLevel`）→ Update(IsDeleted=1)`；
   **`bocheck` 语义 1:1 复刻**（读到行即 true ⇒ 账号不匹配/等级超限也回 `SM_DELCHR_SUCCESS`），
   已由 C# oracle 用例证明（§2.2 第 5 行）。
9. **选角**：账号名大小写不敏感比对 → 逐角色写 `SelectID`（选中 1、其余 0）→ 取 `MapName` →
   按 `ServerInfo.txt` 路由出 `ip/port+MapIndex` → 回 `SM_STARTPLAY` 并 `SetGlobaSessionPlay`。
10. **会话链**：7s 上报 `(103/服务器名/99/在线数)`；收 `(100/账号/会话号/…/IP/秒数)` 入表、
    `(101/…)` 出表；断线**不致命**、每 10s 重连（C# `TimedService` 同）。
11. **GBK 处理**：写库/回包一律用服务端转换 `CAST(CONVERT(ChrName USING gbk) AS BINARY)`
    （等价 .NET `GetEncoding("gb2312").GetBytes`），不在 Rust 内置码表；字符边界/切分见 `gbk.rs`
    （`Split(…, 2, RemoveEmptyEntries)` 的语义用 .NET 直调实测后复现）。
12. **心跳**：收 `KeepAlive` 就回一条空的 `KeepAlive`（C# `SendKeepAlivePacket`，报文与 oracle 向量
    `kind:"internal"` seq 56/57 逐字节一致）；对端发来的 null 字段（MemoryPack `-1`）在解析前就地
    归一化（见 §2.5 长连接面）。

## 4. 已知偏离（登记在案）

见 `tests/parity/whitelist.md` **A-10**：① 连不上库/表缺列 → 明确报错退出（M1④ 授权；
C# 是"打日志带空索引继续跑"）；② 不把缺省值写回 `dbsvr.conf`（C# `ReadWriteXxx` 会写回）；
③ `PlayQuickList` 用等价 `HashMap`（C# 的两个分支会抛 `KeyNotFoundException`，可观测行为一致）。

## 5. 阻塞与后续（各带解除判据）

1. **① 的"真 C# SelGate"一跑**：阻塞条件＝5100/7100 被并行 A 线实验占用（见 §2.5）。
   **已解除**：A 线实验收工后端口空出，本用例当场跑通（§2.5）。
2. **6000 `DataService` 面（S4b）**：需要先补 `crates/storage` 的记录字段（S4a，会动 `crates/protocol`
   之外的接口面，须通知 B2/D2）与 `CharacterDataInfo` 等大结构的 internal 字节向量。判据与切片已写进
   `M1-边界服务移植计划.md` §5 的 S4b 行。**本轮 DBSrv 的卡片验收不含这一面**（见卡片回复条目）。
3. **5700 拍卖行面 + `TimedService` 缓存回写**：同属 S4b（`ICacheStorage` 只被 6000 面使用）。

## 6. 复现命令汇总

```powershell
# 门禁
cd mir2-rs; cargo fmt --all -- --check; cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline; cargo run -p msgcodegen --offline -- --check
# ②④（需要本机 MySQL + mir2_db 里有 mir2test/aaa/bbb）
cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture rust_dbsrv_querychr_matches_csharp_golden
$env:MIR2_ORACLE_DBSRV_DIR='E:\tmp\a2-dbsrv\oracle-dbsrv'
cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture csharp_dbsrv_querychr_matches_csharp_golden
# ①（需 5100/7100 空闲）
$env:MIR2_ORACLE_SELGATE_DIR='E:\tmp\a2-dbsrv\oracle-selgate'
cargo test -p db-srv --test live_querychr --offline -- --ignored --nocapture csharp_selgate_to_rust_dbsrv
# ③
E:\...\mir2-rs\target\debug\db-srv.exe E:\tmp\a2-dbsrv\fail-loud\dbsvr-baddb.conf   # 期望 rc=1 + 报错
```

## 7. 残留物清单

| 路径 | 是什么 | 处置 |
| --- | --- | --- |
| `E:\tmp\a2-dbsrv\evidence\*.log`、`logdiff-*.txt` | 本轮全部判据日志（门禁 ①②③④ / 失败用例 / 两侧日志行集 / 心跳抓帧） | **保留**（报告引用的证据；定稿＝`gate.log`、`oracle-csharp-run8.log`、`rust-dbsrv-run8.log`、`mixed-selgate-keepalive3.log`、`mixed-selgate-keepalive.log`（心跳 null 帧抓帧）、`fail-*.log`、`logdiff-*.txt`） |
| `E:\tmp\a2-dbsrv\evidence\{oracle-csharp,rust-dbsrv,mixed-selgate}-run1..6.log` | 同一判据的过程重跑（迭代期留下的中间件） | 保留；以 `run7`/`run5` 为定稿 |
| `E:\tmp\a2-dbsrv\oracle-dbsrv`、`oracle-selgate` | C# DBSrv / SelGate 构建产物**副本**（影子拓扑用；原目录未动） | 保留（① 复跑需要） |
| `E:\tmp\a2-dbsrv\fail-loud\*.conf` | ③ 的四个故意坏配置 | 保留（报告引用） |
| `E:\tmp\a2-dbsrv\oracle-csharp-run*.log` 等中间重跑 | 同一判据的过程重跑（run1..run4） | 保留；以 `run4`/`run2` 为定稿 |
| `%TEMP%\a2-dbsrv-*` | 测试夹具目录（含两侧子进程日志） | 已清空（本轮自审发现 11 个残件——原因是删目录时子进程仍持有日志句柄，已修成"先收子进程再删目录"，见 commit 60f2726a）；当前 `Get-ChildItem %TEMP% -Filter a2-dbsrv-*` 为空 |
| 我试起的 `LoginSrv.exe` / `LoginGate.exe`（主检出 bin/Release 与 E:\MirServer，09:17–09:18） | 为拿"登录→选服→会话"链路而试起；**两者都因端口已被并行 A 线的 Rust login-srv / C# LoginGate 占用而立即 EADDRINUSE 退出**（`evidence\csharp-{loginsrv,logingate}.{out,err}.log`）——也因此没有动过别人在跑的进程 | 已自行退出，无需处置（用户侧无残留进程：`Get-CimInstance` 按 `*a2-dbsrv-*` 与端口复核均为空） |

> `E:\MirServer` 的冻结部署**未被修改**（副本另存 `E:\tmp`）；本轮未动 `src/**`（oracle 源）。
