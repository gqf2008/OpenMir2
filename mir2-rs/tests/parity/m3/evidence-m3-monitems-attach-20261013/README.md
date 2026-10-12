# evidence-m3-monitems-attach-20261013

> 卡 `openmir2-m3-monitems-attach`（M3④-实现 A：MonItems 挂接，issue `0b247c59`）。
> 本包 = 该实现片的判据原始件：oracle 锚点 + 构造级判据 + 坏表 fail-closed + 真数据覆盖 +
> 两条改坏必红 + 四门禁。
> 仓库基线 `fa82187a`；分支 `worktree-m3-monitems-attach`；写集
> `mir2-rs/crates/data/**` + `mir2-rs/bins/game-svr/**`。

## 实现落点 ↔ oracle 锚点

| oracle（方法 + 文件:行） | Rust 落点 |
| --- | --- |
| `LoadMonsterDB` `src/GameSrv/DB/MySqlDB.cs:168`（`select * from monsters` 行循环） | `crates/data/src/loaders.rs` 的 `DataLoaders::load_monster_db`（既有；本片只补 `item_list: None`） |
| `monster.ItemList = null;` `MySqlDB.cs:225` + `GameShare.LocalDb.LoadMonitems(monster.Name, ref monster.ItemList)` `:226` | `crates/data/src/monitems.rs::attach_monitems`（先置空，再按存在性赋值） |
| `LoadMonitems` `src/GameSrv/DB/LocalDB.cs:596`；路径 `GetEnvirFilePath("MonItems", $"{monName}.txt")` `:599`；`File.Exists(monFileName)` `:600` | `crates/data/src/monitems.rs::load_monitems_if_exists`（`is_file` 口径 = `File.Exists`）+ `parse_monitems_gbk`（既有） |
| `M2Share.GetEnvirFilePath(dirPath, filePath)` `src/M2Server/M2Share.cs:1979` | `envir_dir.join("MonItems").join("<name>.txt")`（`MONITEMS_DIR`） |
| `MonsterInfo.ItemList`（`IList<MonsterDropItem>`） `src/OpenMir2/Data/MonsterInfo.cs:74` | `crates/data/src/models.rs` 的 `MonsterInfo.item_list: Option<Vec<MonsterDropItem>>` |
| `GameApp.Initialize` 调 `LoadMonsterDB` `src/GameSvr/GameApp.cs:131` | `bins/game-svr/src/monitems.rs::load_monsters` / `with_monitems`（生产装载入口，供卡 B/C/D 消费） |

> 状态映射：oracle `null` ↔ `None`（表不存在）；oracle `new List<MonsterDropItem>()` ↔ `Some(vec![])`
> （表存在但解析出 0 条）。二者是**两种状态**，判据不允许混同。

## 文件与退出码

| 文件 | 命令（`CARGO_TARGET_DIR=E:/tmp/openmir2-m3bag-target`，在 `mir2-rs/`） | rc |
| --- | --- | --- |
| `00-oracle-anchors.txt` | oracle 锚点 grep（`MySqlDB.cs` / `LocalDB.cs` / `GameApp.cs` / `M2Share.cs` / `MonsterInfo.cs`） | — |
| `01-criterion-construction.txt` | `cargo test --offline -p mir2-data monitems` + `cargo test --offline -p game-svr --test monitems_attach` | 0 / 0 |
| `02-criterion-real-data.txt` | `cargo test --offline -p game-svr --test monitems_attach -- --ignored --nocapture` | 0 |
| `03-mutation-red-A-reset-removed.txt` | 删掉 `attach_monitems` 的 null-before-load 先置空后跑 `-p game-svr --test monitems_attach`（改坏必红 A） | 101 |
| `04-mutation-red-B-missing-as-empty.txt` | 把「缺文件 ⇒ `None`」错改成「⇒ `Some(空)`」后跑 `mir2-data` + `game-svr` 用例（改坏必红 B） | 101 / 101 |
| `05-gate-fmt.txt` | `cargo fmt --all -- --check` | 0 |
| `06-gate-clippy.txt` | `cargo clippy --offline --workspace --all-targets -- -D warnings` | 0 |
| `07-gate-test.txt` | `cargo test --offline --workspace`（85 个 suite 全绿） | 0 |
| `08-gate-msgcodegen.txt` | `cargo run --offline -p msgcodegen -- --check`（604 常量一致） | 0 |

## 判据要点

**构造级（`01`，离线、可复现，不依赖 MySQL/Envir）**：

- `mir2-data` 6/6：`attach_resets_then_matches_file_existence`（4 只怪，预置**脏 `item_list`**；
  有表 ⇒ 与独立 `parse_monitems_gbk` 逐字段相同；缺文件 ⇒ `None`；空文件 ⇒ `Some(空)`；
  同名目录 ⇒ 视同缺文件；`AttachReport = {total:4, attached:2, entries:2}`）、
  `attach_reads_from_monitems_subdir`（路径口径 = `envir_dir/MonItems/<name>.txt`）、
  `attach_fails_closed_on_unreadable_file`（`FileShare=None` 独占句柄 ⇒ `Err`，不静默成 `None`）。
- `game-svr` 4 passed / 1 ignored：同口径 + `with_monitems`（生产入口）fail-closed + 只读 `MonItems` 子目录。

**真数据覆盖（`02`，`#[ignore]`，需本机 MySQL `mir2_data` + `E:/MirServer/M2GameSvr/Envir`）**：

- `monsters` = **705**（与 C# 启动日志 `[705]` 一致）；挂表**前**逐只 `item_list.is_none()`；
- 挂表**后**逐只核 `Some ⇔ 磁盘有 <name>.txt`，且每个 `Some` 的内容与独立解析逐字段相同；
  `report.attached` = 独立计数 = **182**，`entries` = **7427**；
- 信息列（不作判据）：磁盘上存在但 DB 无同名怪的表 15 个（`命运教主`/`宝箱0`/…/`魔龙教主2`）——
  oracle 同样不会读到它们（`MonsterList` 里没有该 name）。详见 `02` 的 stdout。

**改坏必红（`03`/`04`）**：

- A：删掉「先置空」⇒ 坏表 fail-closed 用例 FAIL（旧值残留 `Some(脏数据)` 而非 `None`），rc=101；
  还原后 `01` 全绿。
- B：缺文件错报成 `Some(空)` ⇒ `mir2-data` 2 条 FAIL、`game-svr` 3 条 FAIL，rc=101；
  还原后 `01` 全绿。

> 两条红检都**真跑**过、原始输出在本包；还原后 `git diff --stat` 只剩本片的实现改动（无残留）。

## 纪律 / 边界

- `src/`（C# oracle）、`sql/`、`MirClient` **零改动**；崩溃冻结面未动。
- **未接线到 `main.rs`**：本卡只交付可复用装载入口（`game_svr::monitems`），启动接线归
  `openmir2-m3-mongen-load-spawn`（卡 B）——§0b「同一文件一次只动一路」，且 m2-svr 车道正在改 `main.rs`。
- **未起栈、未占端口**（判据全离线；真数据覆盖只读共享 3306）。
- 真机「固定种子 100 杀」两侧对拍（卡 `openmir2-m3-drop-exp-align` 的 DoD②③）需要窗口 ⇒ 由该卡
  在 B/C/D 落地后统一申请，本卡不抢。

## 复现（干净检出）

```bash
python -I tools/evidence-hash/verify_sha256sums.py mir2-rs/tests/parity/m3/evidence-m3-monitems-attach-20261013 --check-extra --exclude README.md
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-monitems-attach-20261013
```

期望 GREEN（清单件数见 `SHA256SUMS.txt`）。清单 hash 一律按**仓库字节（Git blob / LF 归一）**记。
