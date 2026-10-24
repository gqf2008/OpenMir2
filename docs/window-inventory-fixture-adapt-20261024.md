# 背包夹具按基线实表适配（`openmir2-window-inventory-fixture-adapt`，tier=L）

- 卡：`openmir2-window-inventory-fixture-adapt`；分支 `card/window-inventory-fixture-adapt`；
  执行者 openmir2-worker-76；基线 `walgit/master = 133c0f6a1`。
- 写集：`tools/conformance/card_window.py`、`tools/hygiene/dynamic_count_allowlist.toml`、本文档。
- 红线遵守：**零 ALTER**（全程只读取证：DESCRIBE / SHOW CREATE TABLE / SELECT）；
  mysqld（3306，pid 2580）全程未停未启；未写 `mir2_db` 任何数据；未改 `src/**`、`rules.json`。

## 1. 取证（acceptance①，只读）

连接参数沿用仓内 conformance 口径（`tools/conformance/README.md` + runbook）：
`D:\mysql\mariadb-10.11.19-winx64\bin\mysql.exe -h127.0.0.1 -P3306 -uroot`（口令空，
`CONFORMANCE_DB_PASSWORD` 未设）。全部命令为 DESCRIBE / SHOW CREATE TABLE / SELECT。

### 1.1 `characters_bagitem`（夹具目标表）

`DESCRIBE mir2_db.characters_bagitem`（基线 C# oracle 库；`SHOW CREATE TABLE` 同步取证）：

| Field | Type | Null | Key | Default | Extra |
|---|---|---|---|---|---|
| Id | bigint(20) | NO | PRI | NULL | auto_increment |
| PlayerId | bigint(20) | NO | | NULL | |
| Position | int(10) | NO | | 0 | |
| MakeIndex | int(20) | NO | | 0 | |
| StdIndex | int(10) | NO | | 0 | |
| Dura | int(10) | NO | | 0 | |
| DuraMax | int(10) | NO | | 0 | |

`SHOW CREATE TABLE` 注释：`PlayerId`＝角色ID、`MakeIndex`＝物品唯一ID、`StdIndex`＝物品编号、
表注释「包裹」，ENGINE=InnoDB，utf8mb3_general_ci。

**两侧测试库同构**：`mir2_test_db` 与 `mir2_test_db_r` 的 `DESCRIBE` 逐列一致（同 7 列同序）。
`mir2_db.characters_bagitem` 行数 337640（两侧测试库 restore 后同为 337640）。

### 1.2 夹具涉及的其它表

- `mir2_db.characters`（42 列，节选）：`Id bigint PK auto`、`ServerIndex`、`LoginID varchar(20)`、
  `ChrName varchar(50)`、`MapName varchar(10)`、`CX/CY int`、`Level`、`HomeMap`、`Deleted tinyint`、
  `CREATEDATE/LASTUPDATE datetime`。→ `ChrName`/`LoginID` 在 **characters** 表上存在，
  夹具解析角色用这两个键没有问题；问题只在把 `ChrName`/`LoginID` 当成了 bagitem 的列。
- `mir2_data.stditems`（47 列，节选）：**主键是 `Id`，没有 `idx` 列**。`Name/StdMode/Shape/
  AniCount/DuraMax` 存在。模板行（三库一致：`mir2_data` / `mir2_test_data` / `mir2_test_data_r`）：
  `Id=207 木剑 StdMode5 Shape1 AniCount0 DuraMax4000`、`Id=3 金创药(小量) StdMode0 Shape0
  AniCount102 DuraMax1` —— 与 `bag_equip.json` / `eat_item.json` 的 inventory 声明一致
  （场景 `index` 语义 = `stditems.Id`；Rust 加载器 `load_items_db` 即按 `Id` 取，
  `mir2-rs/crates/data/src/loaders.rs:142`）。

### 1.3 判定：夹具写错，非两侧语义分歧

首版夹具凭空预期 `characters_bagitem` 有 `ChrName`/`LoginID`/`Index` 列、模板按 `stditems.idx` 查——
基线库（及两侧测试库）**根本没有这些列**。两侧读包语义一致：Rust db-srv 即
`SELECT * FROM characters_bagitem WHERE PlayerId=?`，列映射 `StdIndex/MakeIndex/Position/Dura/DuraMax`
（`mir2-rs/bins/db-srv/src/playdata.rs` `get_item_slots`；建档 `INSERT (PlayerId, Position,
MakeIndex, StdIndex, Dura, DuraMax)` 见 `mir2-rs/bins/db-srv/src/db.rs:660`）；M1–M3 对拍门与
window-run-1 `login_only`（帧+库后态全对拍）钉死 C# 同语义。同仓魔法夹具（`characters_magic`）
用的就是正确的 `PlayerId` 解析模式（`_magic_char_sql`），背包夹具属实现走样。⇒ 按卡 ② 适配夹具，
不涉升级。

## 2. 夹具适配（acceptance②③，替代键方案）

**替代键方案**：`PlayerId = characters.Id`，由 profile 的 `char_name`/`account` 在 `characters`
表上解析（`ChrName+LoginID` 唯一命中、`Deleted=0`——与魔法夹具同口径；表无 `Deleted` 列时按 0）。
行键、占位检查、指纹检查、DELETE+INSERT、写后读回全部以 `PlayerId` 为键。

`tools/conformance/card_window.py` 改动：

| 位置 | 改动 |
|---|---|
| 头注释块 | 登记 DESCRIBE 实表 7 列、`ChrName/LoginID/Index` 不存在、替代键方案与两侧读包语义出处 |
| `inventory_fingerprint_sql` | 签名 `(db, chr_name, login_id, …)` → `(db, player_id, …)`；`WHERE PlayerId=%d`；skip 集 `ChrName/LoginID/Index` → `PlayerId/StdIndex`；**空指纹退化**：实表列全部属于声明列集（就这 7 列）时不再拼 `CONCAT_WS`（单参调用 MySQL 报错——真库实测踩中），退化为只比行数 |
| `_inventory_char_sql`（新增） | `SELECT COUNT(*), MIN(Id)(, Deleted) FROM characters WHERE ChrName=? AND LoginID=?`，与 `_magic_char_sql` 同口径 |
| `run_inventory` | 列门 `ChrName/LoginID/Index/MakeIndex/Position` → `PlayerId/Position/MakeIndex/StdIndex/Dura/DuraMax`；新增 `inventory.character.columns`/`inventory.character` 两步解析并校验角色（不存在/多行/已删除 ⇒ rc=2）；证据 dict 增加 `player_id`、`sql.character`；occ/DELETE/INSERT 全改 `PlayerId` 键；INSERT 列 `(PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax)`；模板查询 `stditems WHERE idx=` → `WHERE Id=` |
| `preview_inventory` | 预览命令[0] 对齐角色解析 SQL（补 `LoginID`/`Deleted`），与 `preview_magic` 同形 |
| selftest（`_selftest_inventory`） | 假 runner 的 `inventory.columns` 输出改实表 7 列、新增两个 character 桩；④ 指纹用例列清单改实表列；新增 ④b「实表无余列 ⇒ 指纹退化为行数」断言（selftest 240 → 241） |

INV 牙锚点（`_check_db_pairing` 行、`if occ_n != 0:`、`if after["count"] != before["count"]:`、
`if not tpl:`、`check_inventory_rows(items, written)` 等）全部原样保留；7 颗 INV 牙在最终代码上
重跑全红（§4）。

场景 JSON 与 `scenarios/README.md` **零改动**（声明形状不变，`index`＝`stditems.Id` 语义不变）。

## 3. acceptance④ 真库写入/读回（补前卡窗口项）

驱动：`E:/tmp/invfix76/acc4.py`（不入仓；SQL 逐条留痕 `E:/tmp/invfix76/run.log`）。
**专属测试角色** `invfix76`（LoginID 同名，两侧测试库 characters 各插一行，Id=13580）；
角色插入前两侧 `characters` 均无该名（role_rows=0）。

| 用例 | 库 | rc | action | written | 读回（独立 SELECT，不经夹具） |
|---|---|---|---|---|---|
| csharp × bag_equip 声明 | mir2_test_db | 0 | updated | 1 | `13580, 40, 5001, 207, 4000, 4000` ✓ |
| csharp × eat_item 声明 | mir2_test_db | 0 | updated | 1 | `13580, 41, 5002, 3, 1, 1` ✓ |
| rust × bag_equip 声明 | mir2_test_db_r | 0 | updated | 1 | `13580, 40, 5001, 207, 4000, 4000` ✓ |
| rust × eat_item 声明 | mir2_test_db_r | 0 | updated | 1 | `13580, 41, 5002, 3, 1, 1` ✓ |

前态：两侧 `characters_bagitem` 各 337640 行；写后各 337642 行（+2，只动声明槽位，指纹/行数
门全过）。**恢复现场**：跑仓内既有 `E:/tmp/conformance/restore-baseline.sh` +
`restore-baseline-r.sh`（drop+create+灌 baseline，共 6 库），复跑计数：两侧 bagitem 回 337640、
`characters` 无 `invfix76`（两侧 COUNT=0）。mysqld（3306，pid 2580）全程在线未动。

## 4. 回归与门禁（acceptance④⑤）

| 项 | 结果 |
|---|---|
| `mir2_probe.py --selftest` | OK (13 checks) |
| `card_window.py --selftest` | **GREEN 241/241**（改前 240/240，本卡 +1：④b） |
| `bag_equip --check-scenario`（`--var make_index=5001 takeon_where=1 item_name=x`） | **rc=0** |
| `eat_item --check-scenario`（`--var make_index=5002 item_name=x`） | **rc=0** |
| `card_window --dry-run`（alt profile，两场景） | rc=0，两侧各 1 行背包夹具预览 |
| INV1–INV7（变异牙，最终代码重跑） | **7/7 全红** + 逐字节还原（restored sha256 == baseline）；其余 44 牙锚点不在本卡写集，未改动其锚文本 |
| `stale_pointer_gate.py` | rc=0 |
| `dynamic_count_stamp_gate.py` | rc=0（master `133c0f6a1` 上即红 2 缺戳，见 §5） |
| `conflict_marker_gate.py` | rc=0 |
| `git diff --check` | rc=0 |
| `git merge-tree --write-tree walgit/master HEAD` | 无冲突（rc=0） |

## 5. 带外登记：dynamic_count_allowlist +2 条（历史快照类）

master `133c0f6a1` 上 `dynamic_count_stamp_gate` 本来就红（`docs/window-run-1-20261024.md`
L53/L70 的当轮计分 `PASS 1 / FAIL 5 / UNDECIDABLE 12` 缺行内 24 字符快照戳；非本卡引入）。
按该门禁「历史快照/引述」既有类（同 `docs/goal-gate-card-association-20261023.md` 条目口径）
增补 2 条 allowlist（指纹由门禁 `--fingerprint` 生成，reason+added=2026-10-24）。
未改该报告正文（改写历史计分即伪造历史）。

## 6. 残留物清单

| 路径 | 是什么 | 处置 |
|---|---|---|
| `E:/tmp/invfix76/acc4.py`、`run.log` | acceptance④ 驱动与逐条 SQL 留痕 | 保留供 review 取证 |
| `E:/tmp/invfix76/gate-*.log`、`dryrun-bag-equip.log`、`dryrun-eat-item.log` | 门禁与 dry-run 输出 | 保留供 review 取证 |
| `E:/tmp/invfix76/dryrun2/`、`dryrun3/` | dry-run 输出目录 | 可删 |
| 两侧测试库 | 已 restore 回 baseline（337640 / 无 invfix76） | 与开工前一致 |
