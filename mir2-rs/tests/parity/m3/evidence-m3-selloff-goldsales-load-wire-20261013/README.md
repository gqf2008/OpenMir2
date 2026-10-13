# evidence-m3-selloff-goldsales-load-wire-20261013

卡 `openmir2-m3-selloff-goldsales-load-wire`（M3⑩-实现 2）的证据包 —— **`goldsales` 只读加载接线**。

> 分支 `worktree-m3-selloff-goldsales-load-wire`；base（rebase 片 r2）= **⑩-1 前缀 head `515540139`**
> （= master `ddaca961c` + ⑩-1 世界侧寄售 + ⑩-1 r2 证据，提供 `game_svr::selloff::SellOffBook`）；patch 交付 base =
> `refs/heads/master`（交付时 origin/master = `78cf74d46`）。**本片与 ⑩-1 的依赖关系见下「串行说明」。**

## 本片做什么

oracle `GameApp.cs:30` 建空 `M2Share.SellOffItemList`，`:181-182` 日志 + 调
`GameShare.DataSource.LoadSellOffItemList()`（`MySqlDB.cs:251-291`）。本片把这条**只读**加载接到
Rust 生产启动路径：

- 新增 `bins/game-svr/src/goldsales.rs`：`load_goldsales`（连接串走 `LegacyDsn`）+ `entry_from_loaded`
  （`MySqlDB.cs:271-280` 逐字段桥接，含 `JsonSerializer.Deserialize<UserItem[]>` 的 `.NET` 形态）
  + `seed_book`（`MySqlDB.cs:279` 的 `IList.Add` 顺序尾追）；
- `main.rs`：启动时 `加载物品寄售系统...` → 加载 → `读取物品寄售列表成功...[N]`
  （`MySqlDB.cs:282` 同文），种子经 `EnterConfig.sell_off_seed` 交给独占 `World` 的 sim 线程，
  初始化 `SellOffBook`。

**缺陷照搬（§1 硬约束③）**：加载器读错列名 `DealChrName`/`BuyChrName`（DDL 为
`DealCharName`/`BuyCharName`，`sql/mir2_data.sql:26-27`）⇒ **表里有数据也恒 0 行**（白名单
`D-5`/`B-5`）；`SaveSellOffItemList` 三个调用点在 oracle **全被注释** ⇒ **不实现 save**。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-oracle-anchors.txt` | `GameApp.cs` 建表/调用点、`MySqlDB.cs:251-291`、DDL、`DealOffInfo.cs`、`save` 注释点、`UserItem` JSON 形态 |
| `01-rust-status-greps.txt` | 改动前生产路径 no-match；加载器本体（D 线）；唯一既有消费者（ignored 用例）；改动后接线 grep；白名单 `D-5`/`B-5` |
| `02-change-summary.txt` | 变更清单 + 行为等价口径 + 未做/登记 |
| `03-tests.txt` | 构造级 + 真身判据读数 + 改坏必红 |
| `20-boundary.txt` | 边界（`src sql MirClient` / `protocol` / `storage`） |
| `gates/10-fmt.txt` … `gates/13-workspace-test.txt` | 四门禁读数 |
| `redchecks/RC-A-*` | RC-A：把加载器列名改成 DDL 正确名 ⇒ 真身判据必红；还原后 rc=0 |
| `redchecks/RC-B-*` | RC-B：删掉 `seed_book` 的 `push`（`Add` 等价）⇒ 构造级必红；还原后 4 passed |
| `SHA256SUMS.txt` | 上述件的 hash（**仓库字节 = LF 归一 / git blob 口径**） |

## 判据（≥1 条 + 改坏必红）

1. **真身级（本机 MySQL，会话级 `TEMPORARY TABLE goldsales` 遮蔽真表、零持久写入）**：
   `cargo test -p game-svr --offline --test goldsales_load -- --ignored --nocapture`
   ⇒ 会话里 `SELECT COUNT(*) = 1`（表里确有数据），生产加载器仍返回 **0 行**（`D-5`/`B-5`）。
2. **构造级**：`cargo test -p game-svr --offline --bin game-svr goldsales`
   ⇒ 4 passed（字段桥接 + `null` 空槽保留 + `seed_book` 顺序尾追 + 空表保持空）。
3. **改坏必红**：
   - RC-A（列名修正 ⇒ 真身判据 FAILED，rc=101；`crates/data/src/loaders.rs` 还原后 rc=0）；
   - RC-B（删 `push` ⇒ `seed_book_appends_in_row_order` FAILED left:0/right:2，rc=101；还原后 4 passed）。

## 门禁读数（本片树；隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m3hp-target`，`--offline`）

- `cargo fmt --all -- --check` → **rc=0**
- `cargo clippy --workspace --all-targets -- -D warnings` → **rc=0**
- `cargo test --workspace --no-fail-fast` → **99 suites / 1127 passed / 0 failed / 22 ignored**，rc=0
- `cargo run -p msgcodegen -- --check` → **604 条常量**，rc=0
- 边界：`git diff --name-only origin/master HEAD -- src sql MirClient` = 空；`protocol`/`storage` = 空

（rebase 片 r2 读数：base = ⑩-1 前缀 head `515540139`（已在 master `ddaca961c` 上），本片自身 delta 仅 5 文件
+419/−2；四门禁全绿。原 base `2869a234b` 的读数 97/1002、`1c0efa7aa` 的 1069 均已作废。）

## 复核命令（干净检出可复现）

```sh
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-selloff-goldsales-load-wire-20261013
python -I tools/evidence-hash/verify_sha256sums.py eolcheck \
  mir2-rs/tests/parity/m3/evidence-m3-selloff-goldsales-load-wire-20261013
```

## 串行说明（⑩-2 依赖 ⑩-1）与 not_verified

- **写集冲突**：⑩-2 的启动种子必须写进 ⑩-1 落的同一 `SellOffBook` + 同一 `main.rs` 启动序列
  ⇒ 本片以 ⑩-1 的 head 为 base（**顺序依赖，非重复实现**）。
- **rebase 片 r2 2026-10-13**（与 ⑩-1 同栈，见 `world/selloff-consignment-20261013/README.md` §2c-r2）：
  ⑩-2 head 的父链含 ⑩-1 的同栈重放 commit（前缀 tip `515540139` = master `ddaca961c` + ⑩-1）。
  做法：`git rebase origin/master`（`ddaca961c`）重放 ⑩-1 + ⑩-2；唯一冲突 `main.rs` = ⑨-2 `pet_cfg`
  块（master 侧）与 ⑩-2 `sell_off_seed` 启动块（本侧）同插入点 ⇒ **两侧并集**，不改被审语义。
  本片自身 delta（⑩-1 前缀 .. HEAD）= 5 文件 +419/−2，逐字节不变；**边界对 master 已空**
  （见 `20-boundary.txt`）。交付时 origin/master = `78cf74d46`，`git merge-tree` HEAD x master = rc=0。
  ⇒ **合并顺序应先 ⑩-1（`515540139`）后 ⑩-2**（同栈前缀 ⇒ ⑩-2 是其后代，合并三向干净）。
  （旧记：`1c0efa7aa` / `16f931f9` / `029d23eae` 的读数已作废。）
- **真机两侧对拍**（`run_selloff_pair.ps1` + `goldsales` 表快照 `diff=0`，同种子）需**独占窗口**
  ⇒ **not_verified**（本片零起栈、零游戏端口占用；仅连本机既有 MySQL 跑 ignored 判据）。
- `UseItems` 的 `UserItem[]` 解码在 D 线口径里被显式推迟（`crates/data/src/models.rs:141-144`），
  本片按 `.NET UserItem` 形态补齐桥接；因「恒 0 行」该支在生产路径**不可达**（登记，不假造读数）。
- 本片**不新增白名单条目**：只读加载 / 坏列名 / 无 save 已由 `D-5`/`B-5`/`S4B-*` 覆盖。

## 残留物清单

- 隔离编译目标目录 `E:/tmp/openmir2-m3hp-target`（保留复用）。
- 证据原始捕获 `E:/tmp/m3hp-selloff/{gates,redchecks}`（已全部入库本包，仓外副本可删）。
- 本 worktree `git status` 干净；未起栈、未占游戏端口；共享主检出未写/未编译。
