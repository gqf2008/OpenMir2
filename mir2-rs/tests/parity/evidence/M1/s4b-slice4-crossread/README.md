# M1 S4b ③ 跨实现互读 —— 证据与口径（2026-10-12，worktree `dbsrv6k`）

卡：`openmir2-m1-dbsrv-6000-face`（issue `370e332f`）｜owner `openmir2-worker-17`｜
分支 `worktree-m1-dbsrv-6000`（base = `refs/heads/master`，本片 ff 到 `ce18a133`）。

> 本片是卡内 **③** —— 「C# GameSvr 经 Rust DBSrv **6000** 能读档进图」+ **Rust↔C# 双向互读** +
> **存档字段级 diff = 0**。② 6000 `DataService` / ④ `TimedService` 已合入 master `96d03db5`，
> ④ 5700 `MarketService` 已合入 master `ca11a1a8`；本片只补 ③ 的真机对拍（无 Rust 代码改动，纯证据）。

**窗口**：M2 lane（`openmir2-m2-gamesvr-host`，owner `openmir2-worker-11`）在卡上写
`comment c365f0af`（“窗口已释放：… 5000/5100/5500/5600/5700/6000/7000/7100/7200 全空 … dbsrv6k 可起 M1③ 栈”）
⇒ 本片据此起栈；收工已停栈（见下 §6）。

---

## 1. 隔离口径（为什么这个 diff 归因到 DBSrv）

两侧**固定同一套 oracle 对端**，只把 DBSrv 换掉：

| 进程 | C# DBSrv 侧 | Rust DBSrv 侧 |
| --- | --- | --- |
| GameSvr(5000) | **C#** `E:\MirServer\M2GameSvr\GameSrv.exe` | **C#** 同左（同一 exe，deployed-from `628d8024`） |
| DBSrv(5100/6000/5700) | **C#** `src\DBSrv\bin\Release\DBSrv.exe` | **Rust** `E:\tmp\openmir2-dbsrv6k-target\debug\db-srv.exe` |
| LoginSrv(5500/5600) | C# `src\LoginSrv\bin\Release\LoginSrv.exe` | 同左 |
| LoginGate(7000)/SelGate(7100)/RunGate(7200) | C# `E:\MirServer\*` | 同左 |
| MySQL(3306) | MariaDB `D:\mysql\mariadb-10.11.19-winx64` | 同左 |
| 客户端 | headless `src/BotSrv`（完整客户端协议栈） | 同左 |

⇒ 两侧**唯一变量 = DBSrv 实现**（GameSvr 同 exe/同配置 `.conf`：`DBAddr=127.0.0.1`、`DBPort=6000`）。
运行态归属原始件见 `09-runtime-impl-ownership.txt`（C# 侧 `runtime_procs` 记到 `DBSrv.exe` 绝对路径）、
`10-ports-csharp-dbsrv-side.txt` / `11-ports-rust-dbsrv-side.txt`（端口 → pid → 进程名逐条）。

## 2. 判据①：「C# GameSvr 经 Rust DBSrv 6000 读档进图」

- 命令：`run_bots.ps1 -Count 1 -Prefix loadbot -RequireWorld`（账号/角色 = `loadbot0`，已存在）
- BotSrv 侧（`run_bots.ps1` 的结构化汇总原始件 `01-botsrv-loadbot0.summary.json`）：
  `{"count":1,"spawned":1,"login_ok":1,"in_world":1,"success_rate_pct":100,"world_rate_pct":100,"require_world":true}`
  ⇒ **真进世界**（本会话收到自己的 `SM_LOGON`，`in_world == count == 1`），不是本地切场景。
  注：`run_bots.ps1` 控制台打印的 `PROGRESS … in_world=1/1` / `GREEN: 1/1 登录成功 (100%)；in_world=1/1`
  **未落文件**；随片的 `01-botsrv-loadbot0.txt` 是 BotSrv 进程日志（止于进图后的打怪行），不含该汇总行。
- Rust db-srv 侧（`01-rust-dbsrv-6000-loadbot0-load-save.txt`）：
  ```
  服务器连接: 127.0.0.1                         # C# GameSvr 接入 6000
  [QueryChr] 候选 account=loadbot0 …             # 5100 面（选角）
  玩家获取游戏网关信息 RunGame:127.0.0.1/7200 …    # 5100 → SelGate：给出 7200
  获取玩家[loadbot0]数据成功                       # ★ 6000 DB_LOADHUMANRCD 取档（GameSvr → Rust DBSrv）
  会话关闭 socket_id=1 存活=11902ms
  保存角色[loadbot0]数据成功                       # ★ 6000 DB_SAVEHUMANRCD 存档（掉线路径）
  ```
  ⇒ 6000 `DataService` 的**取档**与**存档**都由真 C# GameSvr 触发、由 Rust db-srv 服务成功。

## 3. 判据②：存档字段级 diff = 0（跨实现，同一确定性序列）

同一命令序列、两侧各自**全新命名空间**（`run_pair.ps1` 的 fresh-prefix 口径），
固定建角参数避免建角随机列当判据：

```powershell
# C# DBSrv 侧
powershell -File mir2-rs/tests/parity/m3/run_pair.ps1 -Side csharp -Prefix m1c021402 -Bots 1 `
  -FixedNewChr -NewChrJob 1 -NewChrHair 2 -NewChrSex 0
# 换 Rust DBSrv（其余进程不变）后再跑
powershell -File mir2-rs/tests/parity/m3/run_pair.ps1 -Side rust   -Prefix m1r021839 -Bots 1 `
  -FixedNewChr -NewChrJob 1 -NewChrHair 2 -NewChrSex 0
# 作用域字段级对拍（M3 判据本体）
python mir2-rs/tests/parity/m3/scoped_diff.py diff --canonical --fixed-newchr `
  …/csharp-20261012-021403/scoped_after_norm.jsonl  …/rust-20261012-021839/scoped_after_norm.jsonl
```

- 结果：**`GREEN: 作用域内 9 张表逐行一致`**（exit 0）——`02-cross-impl-field-diff.txt`。
- 两侧作用域行数/表分布**逐表相等**（`09-runtime-impl-ownership.txt`）：
  `characters/ablity/status/indexes +1`、`bagitem +46`、`item +13`、`storageitem +50`，
  `account/account_protection +1`（合计 115 行 / 9 表，两侧一致）。
- 语义：**同一 C# GameSvr** 把同一建角+进图+退出序列的存档记录分别交给 C# DBSrv 与 Rust DBSrv，
  落库结果**字段级逐行一致** ⇒ 存档 SQL/参数/编码 1:1（`playdata.rs` 的移植正确）。
- 退出路径：本片覆盖 **`disconnect`（掉线/进程结束 → GameSvr `DB_SAVEHUMANRCD`）**。
  `softclose`（`CM_SOFTCLOSE` 小退，需 GUI 客户端确认框/合成键）与 `shutdown`（服务端优雅停栈保存）
  本轮**未覆盖**（登记，见 §5）。

## 4. 判据③：跨实现互读（双向）

| 方向 | 写入方 | 读取方 | 证据 |
| --- | --- | --- | --- |
| **C# 写 → Rust 读** | C# DBSrv（`m1c0214020`） | Rust db-srv 6000 | BotSrv 结构化汇总 `07-botsrv-csharp-writes-rust-reads.summary.json`（`in_world=1`）+ `07-botsrv-csharp-writes-rust-reads.txt`；端口归属推断见下注 |
| **Rust 写 → C# 读** | Rust db-srv（`m1r0218390`） | C# DBSrv 6000 | BotSrv 结构化汇总 `08-botsrv-rust-writes-csharp-reads.summary.json`（`in_world=1`）+ `08-botsrv-rust-writes-csharp-reads.txt`；C# DBSrv 侧原始件 `06-csharp-dbsrv-stdout.txt`（02:20:06 读到 `m1r0218390`）；Rust 侧写入原始件 `05-rust-dbsrv-6000-m1r-load-save.txt` |

⇒ 一侧 DBSrv 写下的角色资料（`characters*` 及 8 张附属表）能被**另一实现**的 DBSrv 读回并支撑
C# GameSvr 把角色**放进世界**（收到 `SM_LOGON`）——即「跨实现互读」双向成立。

> **F-1 处置（reviewer-3 `39d41840`，request_changes）**：本片此前为 07（C# 写 → Rust 读）引用的
> 「Rust 日志 `获取玩家[m1c0214020]数据成功`」**未随片、且原日志已不可复得**：服务该次读取的 Rust db-srv
> 进程（`10-ports-csharp-dbsrv-side.txt` 中被 `STOP db-srv pid=25948` 的那一个）其 live 输出就是
> `side_stack.ps1` 的 `DBSrv.out.log`，在 02:19:34 C# 栈启动时被 `-RedirectStandardOutput` **截断**
> （现存 `05-*` 副本止于 02:19:03，不含 02:19:11–02:19:26）。故 07 方向改为两条**随片可核**的依据：
> ① BotSrv 结构化汇总 `in_world=1`（该读取端到端成立）；② **端口归属推断**——`06-csharp-dbsrv-stdout.txt`
> 显示 C# DBSrv 于 **02:19:34** 才启动（晚于 07 的 02:19:11–02:19:26），而 `10-ports-csharp-dbsrv-side.txt`
> 显示同一交接点被停掉的是 Rust `db-srv pid=25948` ⇒ 6000 上服务该次读取的是 **Rust 实现**。
> 「Rust 6000 侧对 `m1c0214020` 的取档日志未随片」已登记为 `not_verified`（见 §5）。

## 5. 未覆盖 / 登记

| 项 | 状态 | 原因 |
| --- | --- | --- |
| `softclose`（小退）路径 DB diff | **未覆盖** | 需 GUI 客户端走 `Alt+X` 的模态确认框（`run_pair`/`BotSrv` 无该动作）；本轮窗口内不冒险合成。 |
| `shutdown`（服务关闭）路径 DB diff | **未覆盖** | 需 bot 在世界的同刻优雅停 GameSvr（`Stop-Process -Force` 不触发保存）；编排未做，登记。 |
| Rust 6000 侧对 `m1c0214020` 的取档日志（02:19:11–02:19:26） | **not_verified（未随片）** | 该 Rust 进程 live 输出被 02:19:34 的 C# 栈启动截断；方向归属改由 BotSrv 汇总 `in_world=1` + 端口归属推断（§4 注 F-1）。 |
| `TimedService` 5 分钟缓存回写对拍 | 已由片 2 单测钉住 | 缓存恒空是 C# 既有行为（`cache.rs` 模块头）。 |
| 5700 `MarketService` 真机对拍 | 已由片 3 伪对端/真身探针钉住 | C# 5700 大半不可达（`MarketItem` 未注册 MemoryPack），登记 `whitelist S4B-5..10`。 |

## 5b. 红检（本片无 Rust 代码改动 ⇒ 给「字段级 diff」这一判据补牙齿）

把 Rust 侧 `scoped_after_norm.jsonl` 的 `mir2_db.characters.Level` 由 `0` 改成 `5`（**被比对列**，
非易变列）后重跑同一 `scoped_diff.py diff --canonical --fixed-newchr`：

- 结果：**`RED: 作用域内 2 行不一致`（exit 1）**，差异行打印 `A: …	0	…` vs `B: …	5	…` —— `13-redcheck-field-diff.txt`。
- 语义：判据能抓住「一侧落库与另一侧不同」，不是恒绿。

## 6. 四门禁（隔离 target `E:\tmp\openmir2-dbsrv6k-target`，`--offline`）

`12-gates.txt`：`cargo fmt` exit 0；`cargo clippy --workspace --all-targets -D warnings` exit 0（0 warn）；
`cargo test --workspace` **70 suites / 507 passed / 0 failed / 16 ignored**（与 master 合并树 507 一致，本片无新 Rust 测试 ⇒ 1 连跑）；
`cargo run -p msgcodegen -- --check` ⇒ **604**。

## 7. 复现 / 残留

- 栈编排脚本（本片临时件，随证据入库副本）：`side_stack.ps1`（起 `-Dbsrv rust|csharp` 的整栈，其余固定 C# oracle）。
- 收工已 `-Action stop` 停栈；端口回落。残留物清单见 `E:\tmp\openmir2-watch\last-dbsrv6k.md`。
