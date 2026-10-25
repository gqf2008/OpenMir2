# db-srv 会话时区对齐 oracle + 时间列逐列见证（卡 `openmir2-dbsrv-session-timezone`，tier R）

- 卡：`openmir2-dbsrv-session-timezone`（issue `d1e64eec`；owner `openmir2-dev-1`；
  branch `card/dbsrv-session-timezone`；base = 本 patch 的 `--base`）。
  写集：`mir2-rs/bins/db-srv/src/db.rs`、`tools/db-parity/dbsnap_pair.py`、
  `tools/conformance/compare_runs.py`、`tools/conformance/selftest_impl.py`、
  `tools/conformance/rules.json`、本文件。
- **不改 `src/**`**（C# 一字不动）；未起栈、未跑 cargo（本期 cargo 归 dev-2）。

## 1. 问题与根因

同一时刻落库成两个不同的墙钟文本，恒差 8h（真机实测 TZ-1）：

| 列 | C#（oracle） | Rust |
|---|---|---|
| `characters.LASTUPDATE` | `2026-10-25 10:09:21` | `2026-10-25 02:10:25` |
| `characters_ablity.ModifyTime` | `2026-10-25 10:11:49` | `2026-10-25 02:12:54` |

两侧 SQL 都是 `now()`（`playdata.rs:426/486` vs `PlayDataStorage.Save.cs:88/141`），差别只在**会话时区**：

- C#（MySqlConnector）不改 `time_zone` ⇒ `SYSTEM`（部署本机 +08:00）⇒ `now()` 是本地墙钟；
- Rust 走 `sqlx 0.8`，其 `MySqlConnectOptions::timezone` **默认 `Some("+00:00")`**
  （`sqlx-mysql-0.8.6/src/options/mod.rs:367`，连接后 `SET time_zone='+00:00'`，`options/connect.rs:69`）
  ⇒ `now()` 是 UTC。

**为什么一直没被对拍抓到**：`compare_runs` 的 `db.time_mode=presence` 只比时间列**存在性**，
值差多少都看不见。任何按时间列做业务判断的功能（有效期/冷却/离线时长/拍卖邮件/排行结算）都会分叉。

## 2. 修法

### 2.1 db-srv：连接时显式 `time_zone = SYSTEM`

`Db::connect_options(dsn)` 统一构造 `MySqlConnectOptions::from_url(..).timezone("SYSTEM")`，
连接池（`connect_with`）与测试建库连接都走它。

**关于 sqlx 的「改 `timezone` 会让 `DateTime<Utc>` 偏移」警告**（`options/mod.rs:369-374`）：
本进程**不把任何 DATETIME 列解码成 Rust 时间类型**——生产路径只写 `now()`；
`SELECT *` 只出现在测试里且只取整数列（`row_ints(.., ABLITY_COLS)`）。
所以这个设置只改变**服务端 SQL `now()`** 的语义，不涉及参数/返回值解码。Rust 侧牙把这点锁住。

### 2.2 对拍：时间列**逐列登记** + `plausible` 见证

`db.time_mode` 仍是 `presence`（两次独立运行的墙钟本来就不同，不能直接比值），
但**每个时间列都必须在 `db.time_columns` 里登记**，未登记 ⇒
`UNDECIDABLE(time_witness_unregistered)`——**不许全域豁免**。

登记两种 mode：

| mode | 判据 | 用途 |
|---|---|---|
| `plausible` | **该侧该列的最大值**必须落在该侧 `run.json` 的运行窗口 `[start_wall − tol, end_wall + tol]` 内（`time_tolerance_s` 默认 600s） | 抓住「写进去的时刻不对」（会话时区差 8h、写错列、没写） |
| `presence` | 只比 NULL/非 NULL | 必须是**明确理由**的列（业务日期、本流程不写、基线旧值） |

三点口径（reviewer-20 逐条核过）：

1. **只看列的最大值**，不逐行判：快照里躺着几万行历史角色（restore 基线、2022 的时间戳），
   逐行判会把它们全判 stale。「本次运行有没有写入」等价于「本列最大值是否落在运行窗口内」。
2. **锚点是运行窗口，不是「距结束 ≤600s」**：probe 的 `run.json` 字段是 **`start_wall` / `end_wall`**
   （ISO 带 `+0800`；`started_at`/`ended_at` 在真机产物里是 None，只作为 selftest 合成产物的退路）。
   长场景（例如带 620s sleep 的场景）允许在运行途中写时间列——用「距结束」会把合法值判 stale。
3. `plausible` 用的是**每一侧自己的**窗口 ⇒ 两次运行相隔多久都不影响判定，但 8h 偏移必然超窗口。

**判据的语义边界（写死，别扩读）**：`time_witness` 判的是「**这次运行有没有写**」，
**不是**「表里每一行的时间都对」。所以它只看该列的**最大值**：
- 用逐行判 ⇒ `characters` 里 restore 基线的**几万行 2022 历史角色**永远 stale ⇒ 判据退化成
  「表里只要有一行老数据就一票否决」，与「本次运行是否写入」这个目标无关（本轮实测踩到：
  r1 一把跑出几万条 stale）。
- 用最大值判 ⇒ 历史行不影响；「本次运行写过 ⇒ 最大值 ≈ 运行时刻」，没写过 ⇒ 最大值仍是基线旧值。

**范围**：只要求**被快照的 13 张表**里的 **SQL 时间列（DATETIME/TIMESTAMP）**全覆盖；
不可快照的表（`account_friend` / `tbl_connectlogs` 等）不参与。
`account_protection.Birthday` 是 **`char(11)`**（业务日期存字符串）⇒ **不是时间列**，
已从 `TABLE_SPECS` 的 `time_columns` 与登记表里一并移除（原先被当成时间列是登记口径的疏漏）。

本仓登记（`rules.json` → `db.time_columns`）：

| 表 | 列 | mode | 理由（摘要） |
|---|---|---|---|
| `characters` | `LASTUPDATE` | **plausible** | 每次存档 `now()` 刷新 ⇒ TZ-1 的探针 |
| `characters_ablity` | `ModifyTime` | **plausible** | 与 LASTUPDATE 同一次存档写入 ⇒ 第二探针 |
| `characters` | `CREATEDATE` | presence | 建角时写；本流程角色来自 restore 基线（2022 旧值） |
| `characters_indexes` | `CreateDate` / `ModifyDate` | presence | 建角/改名触发；本流程未触发 |
| `account` | `ValidFrom` / `ValidUntil` / `StopUntil` | presence | 账号管理字段，游戏流程不写 |
| `account` | `CreateTime` / `ModifyTime` / `LastLoginTime` | presence | unix 秒；本流程未观测到写入（改 plausible 需真机证据） |
（`account_protection.Birthday` 是 `char(11)`，**不是** SQL 时间列，已移除，见上。）

记录 kind：`time_witness_unregistered` / `time_witness_no_clock`（缺 run.json 墙钟）/
`time_witness_unparsable` ⇒ **判不了**（rc 2）；`time_witness_stale` ⇒ **差异**（rc 1，点名表/列/值/墙钟/差值）。

## 3. 牙

**对拍侧（`compare_runs selftest`，新增 4 条，现 326/326）**

| 牙 | 断言 | 变异实测 |
|---|---|---|
| a | rust 侧写 UTC（早 8h）⇒ rc 1 + `time_witness_stale`（点名 `characters_ablity.ModifyTime`、delta=28805s） | 「stale 不判红」⇒ 红；「摘掉见证 pass」⇒ 红 |
| b | 时间列未登记 ⇒ rc 2 `time_witness_unregistered` | 「未登记当 presence 放行」⇒ 红 |
| c | 同一列登记 `presence` ⇒ rc 0（**放行是显式登记的结果**） | —（正向对照） |
| d | `plausible` 但该侧缺 `run.json` 墙钟 ⇒ rc 2 `time_witness_no_clock` | 「摘掉见证 pass」⇒ 红 |
| e | **620s 长场景**：运行**开头**写的时间列（距结束 620s）⇒ rc 0（判据是窗口，不是距结束） | 「退回距 `end` ≤ tol」⇒ 红 |
| f | 窗口**下界**同样承重：比 `start` 早 660s ⇒ rc 1 `time_witness_stale` | — |

**db-srv 侧（`cargo test -p db-srv --bins session_time_zone_and_local_wall_clock -- --ignored`）**

- 牙 a：生产连接 `SELECT @@session.time_zone` == `SYSTEM`；
- 牙 b：经生产连接写 `LASTUPDATE = now()` 后，值与 SYSTEM 参考会话的 `NOW()` 相差 ≤ 120s；
- 牙 c（负向对照）：**不带**本选项的 sqlx 默认连接确实是 `+00:00` ⇒ 证明 a/b 承重。

**已跑（2026-10-25，`mir2-rs/` 下，独占窗口）**：

- `cargo fmt --all -- --check` ⇒ rc 0
- `cargo clippy --workspace --all-targets -- -D warnings` ⇒ rc 0
- `cargo test --workspace` ⇒ rc 0（全绿）
- `cargo test -p db-srv --lib session_time_zone_and_local_wall_clock -- --ignored --nocapture`
  ⇒ **1 passed**（会话时区 == `SYSTEM` / 写入值贴近 SYSTEM 参考 `NOW()` / 负向对照 `+00:00`）
- `target/debug/msgcodegen.exe --check` ⇒ `OK: 604 条常量…与 C# 源一致`

一处实现细节（编译期发现）：`.timezone(...)` 与 `from_url(&Url)` 必须走 URL 解析路径
（`connect_options` 用 `url::Url::parse(&dsn.to_sqlx_url())` 再 `from_url`），
**不要**用 `MySqlConnectOptions::new()` 逐字段拼——空口令在两种构造下语义不同，
逐字段拼会让 `root` 空口令连接被拒（实测 `Access denied … (using password: YES)`）。

## 3b. 反向对照（离线，未起栈）：新判据拿旧证据自证

用 harness 真机自证留下的快照直接跑当前代码（只读文件，不起栈）：

```
python -I -B tools/conformance/compare_runs.py   --oracle E:/tmp/window/harness-proof/r1/csharp   --rust   E:/tmp/window/harness-proof/r1/rust --out /tmp/tz-reverse-r1.json
```

- **r1** ⇒ `rc 1`，`time_witness_stale` **只出现在 rust 侧**：`LASTUPDATE = 2026-10-25 02:10:25`，
  运行窗口 `[10:10:23, 10:10:25]`，`delta_s = 28198`（≈8h 减去窗口余量）；**csharp 侧通过**
  （最大值 `10:09:21` 落在窗口 `[10:09:17, 10:09:21]`）。
- **r2** ⇒ 同样的两条 rust stale（`02:12:54`，`delta_s = 28197`），整轮 `rc 2` 只因 ident 14 pending。

⇒ 判据**真的会咬**「写进去的是 UTC」（**修前红**）；csharp 那半同时证明**不误伤**本地墙钟。
「修后绿」需真机复跑（§4）。

**设计修正（本轮实测发现）**：最初按「逐行判」写，r1 一把跑出**几万条** stale——`characters`
表里躺着 restore 基线的历史角色（2022 年时间戳）。改成**只看该列最大值**（= 「本次运行有没有
写入该列」的等价判据）后，才得到上面这份干净的反向对照。

## 4. 未做 / window_items

1. **真机复拍**（硬验收）：双栈各跑一次产生时间列写入的场景（`login_only` 即可），证明
   两侧 `characters.LASTUPDATE` / `characters_ablity.ModifyTime` **相等**（不再差 8h），
   且 `compare_runs` 在 `plausible` 见证下 **rc 0**（§3b 已证「修前红」，这里要证「修后绿」）。
2. `account` 的 unix 秒列是否也由游戏流程写入（`LastLoginTime`/`ModifyTime`）：需真机证据，
   确认后再决定是否从 `presence` 收紧到 `plausible`。

## 5. 门禁（本期已跑）

`compare_runs selftest` 328/328 GREEN；`dbsnap_pair selftest` 30/30、`check-spec` GREEN；
7 项卫生门 + `git diff --check <base> <head>` + `merge_tree_helper` 见 patch body。
**未跑 cargo（见 §4）**、未起栈、未占端口。
