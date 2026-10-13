# evidence-m3-castle-file-load-20261013 —— M3⑧-实现 6（`openmir2-m3-castle-file-load`）第①片

**卡**：`openmir2-m3-castle-file-load`（M3⑧-实现 6；盘点 §1.D/§1.F/§1.J）。
**作者**：`openmir2-worker-5`（worktree `m3cs`）。**不 merge、不自签**。

## 1. 本片 scope（第①片）

按 oracle 逐行落 **攻城文件加载**：

- `Castle/List.txt` 读取 / 落盘（`CastleManager.LoadCastleList`/`SaveCastleList`）；
- `Castle/<N>/SabukW.txt` 的 `[Setup]` + `[Defense]` **全键 + 默认值**（`CastleConf.LoadConfig`）；
- `Castle/<N>/AttackSabukWall.txt` 读取 / 落盘（`UserCastle.LoadAttackSabukWall`/`SaveAttackSabukWall`）；
- **防御单元规格**（门 / 左中右墙 / 12 弓手 / 12 守卫；name/x/y/hp）——即 `UserCastle.Initialize`
  要 `RegenMonsterByName` 的那批实体，HP 取配置值（race 110/111 不缩放已在 `crates/data/src/loaders.rs:259`）。

写集：`mir2-rs/crates/envir/src/castle.rs`（新）+ `lib.rs`/`error.rs` + `crates/envir/examples/dump_castle_files.rs`（新）
+ 本证据目录。**零 `src/`、零 `sql/`、零 `MirClient`、零 `crates/protocol`、零 `crates/storage`、零 `crates/world`。**

## 2. 什么**不在**本片（承接去向）

| 未做项 | 归属 | 原因 |
| --- | --- | --- |
| 门/墙/弓手/守卫**实体挂图**（`RegenMonsterByName` + `WAbil.HP`） | ⑧-7 / ⑧-8（城堡实体类型） | `crates/world` 尚无 `CastleDoor`/`WallStructure`/`ArcherGuard`（盘点 §2 #7/#22）。本片已把规格产出，实体化是纯接线。 |
| `SabukW.txt` 落盘（`CastleConf.SaveConfig`） | ⑧-7 / ⑧-8 状态机 | 写条件依赖 `BaseObject`/`MapName` 归属/`UnderWar` 等运行态。 |
| `AttackSabukWall` 的 `DateTime` 归一（oracle `DateTime.Parse`→`ToString()` culture 相关） | 本片**登记为刻意偏差** | 确定性用例无法复现 culture；本片保留引号内原始串 + `attack_date_parsed` 标志。 |
| C# 真身探针（`castle-file-probe`） | 可选 follow-up | 本片用**部署件原始字节**（`fixtures/SabukW-0.hex`，sha256 `6021523b…`）+ 源码行号锚定；未跑 oracle 探针（`CastleConf` 是纯 INI 读侧，字段可由源码+部署件双向核对）。 |

## 3. 判据与原始件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 全部命令（含隔离 `CARGO_TARGET_DIR`） |
| `01-oracle-anchors.txt` | oracle `文件:行` 锚（10 组） |
| `02-deployed-data.txt` | 部署件 `List.txt`/`0/SabukW.txt`/`0/AttackSabukWall.txt` 的 sha256+size+编码 |
| `03-rust-vectors.txt` | `dump_castle_files` 对部署件 `0/SabukW.txt` 的逐字段读数（原始输出） |
| `04-redcheck.txt` | **改坏必红**：`GUARD_ARRAY_LEN` 12→4 ⇒ 牙齿测试 RED；还原 ⇒ GREEN 12/12（还原后源码 sha256 与备份相同） |
| `05-gate-fmt.txt` / `06-gate-clippy.txt` / `07-gate-test.txt` / `08-gate-msgcodegen.txt` | 四门禁原始输出 |
| `09-boundary.txt` | 边界（`src/sql/MirClient`、`protocol/storage` 空） |
| `10-vectors-inventory.txt` | 12 条用例清单 + 未覆盖项 |
| `fixtures/SabukW-0.hex` | 部署件 `0/SabukW.txt` 的**逐字节 hex**（1748 B，GBK+CRLF），避免 `.txt` 被 git 归一 |

## 4. 判据结论

- **构造级判据**：12 条离线用例（`cargo test -p mir2-envir --lib castle`）逐字段/逐字节核对部署件与 oracle 默认值。
- **改坏必红**：`04-redcheck.txt`（牙齿 `guard_array_len_is_twelve_not_hire_cap` 咬住「12 vs 雇佣上限 4」）。
- **四门禁**：fmt 0 / clippy 0 / `cargo test --workspace --no-fail-fast` 0（998 passed / 0 failed）/ msgcodegen 604 OK。
- **边界**：`src sql MirClient` 与 `crates/protocol` `crates/storage` 空。
- **证据 hash**：`python -I tools/evidence-hash/verify_sha256sums.py --check-extra <本目录>` ⇒ `GREEN … OK 12/12`（仓库字节 / LF 归一口径）；`eolcheck <本目录>` ⇒ `GREEN 13/13`。

## 5. 残留物清单

- 本目录（证据包，含 `fixtures/SabukW-0.hex`）为**本片专属**，随 patch 入库。
- `/e/tmp/castle.rs.bak`（改坏必红前的草稿备份）——改坏必红已完，**可删**；不入库。
- 无起栈 / 无端口占用（全程离线）。
- `/e/tmp/m3cs-claim.json`（认领 body 草稿）——可删；不入库。
