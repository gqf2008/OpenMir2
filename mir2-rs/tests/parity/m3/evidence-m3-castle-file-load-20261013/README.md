# evidence-m3-castle-file-load-20261013 —— M3⑧-实现 6（`openmir2-m3-castle-file-load`）第①片

**卡**：`openmir2-m3-castle-file-load`（M3⑧-实现 6；盘点 §1.D/§1.F/§1.J）。
**作者**：`openmir2-worker-5`（worktree `m3cs`）。**不 merge、不自签**。

> **2026-10-13 返工**（reviewer-6 review `92cc1c49` @head `6967640c7` 判 `request_changes`）：
> **F-1** oracle 锚行号 9/10 组漂移 ⇒ 订正 `castle.rs` 模块头 + 内联 doc + `01-oracle-anchors.txt`
> （同族漂移 grep 全仓一次改干净，**零行为变更**）；
> **F-2** 落盘序列化用 LF 而 oracle 用 CRLF ⇒ 两个 `serialize_*` 改 `\r\n` + 单测同步（见 `11-serialize-crlf.txt`）；
> **M-1** 证据计数订正（`00-commands.txt` / 本 README）+ 去掉不存在的 `error.rs` 改动声明；
> **M-2** `SabukW.txt` 只读不回写（oracle 缺键会回写默认）已登记为本片刻意偏差；
> **M-3** 畸形行（未闭合引号）新增用例钉住，前导空白细微差异已登记。

## 1. 本片 scope（第①片）

按 oracle 逐行落 **攻城文件加载**：

- `Castle/List.txt` 读取 / 落盘（`CastleManager.LoadCastleList`/`SaveCastleList`）；
- `Castle/<N>/SabukW.txt` 的 `[Setup]` + `[Defense]` **全键 + 默认值**（`CastleConf.LoadConfig`）；
- `Castle/<N>/AttackSabukWall.txt` 读取 / 落盘（`UserCastle.LoadAttackSabukWall`/`SaveAttackSabukWall`）；
- **防御单元规格**（门 / 左中右墙 / 12 弓手 / 12 守卫；name/x/y/hp）——即 `UserCastle.Initialize`
  要 `RegenMonsterByName` 的那批实体，HP 取配置值（race 110/111 不缩放已在 `crates/data/src/loaders.rs:259`）。

写集：`mir2-rs/crates/envir/src/castle.rs`（新）+ `lib.rs`（导出）+ `crates/envir/examples/dump_castle_files.rs`（新）
+ 本证据目录。**零 `src/`、零 `sql/`、零 `MirClient`、零 `crates/protocol`、零 `crates/storage`、零 `crates/world`。**

## 2. 什么**不在**本片（承接去向 / 已登记偏差）

| 未做项 | 归属 | 原因 |
| --- | --- | --- |
| 门/墙/弓手/守卫**实体挂图**（`RegenMonsterByName` + `WAbil.HP`） | ⑧-7 / ⑧-8（城堡实体类型） | `crates/world` 尚无 `CastleDoor`/`WallStructure`/`ArcherGuard`（盘点 §2 #7/#22）。本片已把规格产出，实体化是纯接线。 |
| `SabukW.txt` 落盘（`CastleConf.SaveConfig`） | ⑧-7 / ⑧-8 状态机 | 写条件依赖 `BaseObject`/`MapName` 归属/`UnderWar` 等运行态。 |
| `SabukW.txt` **只读不回写**（reviewer-6 **M-2**） | 本片**登记为刻意偏差** | oracle `ConfigFile.ReadWrite*` 缺键会回写默认值（`LoadConfig` 会改写 `SabukW.txt`）；内容冻结口径下 Rust 侧只读、不回写。若将来做文件级 diff，这会体现为文件字节差异。 |
| `AttackSabukWall` 的 `DateTime` 归一（oracle `DateTime.Parse`→`ToString()` culture 相关） | 本片**登记为刻意偏差** | 确定性用例无法复现 culture；本片保留引号内原始串 + `attack_date_parsed` 标志。 |
| 畸形行（未闭合引号）的前导空白细微差异（reviewer-6 **M-3**） | 本片**登记为刻意偏差** | oracle `ArrestStringEx` 的 `Unchanged` 分支写回 remainder（含前导分隔符串），Rust `get_valid_str3` 已吃掉前导分隔符；仅畸形行可达、只影响展示/回写串。新增用例钉住当前行为。 |
| C# 真身探针（`castle-file-probe`） | 可选 follow-up | 本片用**部署件原始字节**（`fixtures/SabukW-0.hex`，sha256 `6021523b…`）+ 源码行号锚定；未跑 oracle 探针（`CastleConf` 是纯 INI 读侧，字段可由源码+部署件双向核对）。 |

## 3. 判据与原始件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 全部命令（含隔离 `CARGO_TARGET_DIR`） |
| `01-oracle-anchors.txt` | oracle `文件:行` 锚（11 组；含返工 F-1 订正记录） |
| `02-deployed-data.txt` | 部署件 `List.txt`/`0/SabukW.txt`/`0/AttackSabukWall.txt` 的 sha256+size+编码 |
| `03-rust-vectors.txt` | `dump_castle_files` 对部署件 `0/SabukW.txt` 的逐字段读数（原始输出） |
| `04-redcheck.txt` | **改坏必红 x2**：`GUARD_ARRAY_LEN` 12→4；`serialize_castle_list` 的 CRLF→LF；均 RED 后还原 GREEN 13/13 |
| `05-gate-fmt.txt` / `06-gate-clippy.txt` / `07-gate-test.txt` / `08-gate-msgcodegen.txt` | 四门禁原始输出 |
| `09-boundary.txt` | 边界（`src/sql/MirClient`、`protocol/storage` 空） |
| `10-vectors-inventory.txt` | 用例清单（castle 12 条）+ 未覆盖项 |
| `11-serialize-crlf.txt` | **F-2 返工**：oracle CRLF 依据 + 改前/改后 + 牙齿 |
| `12-hash-check.txt` | `verify_sha256sums.py --check-extra` 与 `eolcheck` 的原始输出 |
| `fixtures/SabukW-0.hex` | 部署件 `0/SabukW.txt` 的**逐字节 hex**（1748 B，GBK+CRLF），避免 `.txt` 被 git 归一 |

## 4. 判据结论

- **构造级判据**：13 条离线用例（`cargo test -p mir2-envir --lib castle`；castle 模块 12 条 + `merchant` 1 条）逐字段/逐字节核对部署件与 oracle 默认值。
- **落盘行尾**：两个 `serialize_*` 产出 CRLF，`serialize_castle_list(1)` 逐字节 == 部署件 `List.txt`（`30 0D 0A`）。
- **改坏必红 x2**：`04-redcheck.txt`。
- **四门禁**：fmt 0 / clippy 0 / `cargo test --workspace --no-fail-fast` 0（98 suites / 1036 passed / 0 failed / 21 ignored）/ msgcodegen 604 OK。
- **边界**：`src sql MirClient` 与 `crates/protocol` `crates/storage` 空。
- **证据 hash**：`python -I tools/evidence-hash/verify_sha256sums.py --check-extra <本目录>` ⇒ `GREEN OK 14/14`（仓库字节 / LF 归一口径）；`eolcheck <本目录>` ⇒ 见 `12-hash-check.txt`（原始输出）。

## 5. 残留物清单

- 本目录（证据包，含 `fixtures/SabukW-0.hex`）为**本片专属**，随 patch 入库。
- `/tmp/castle.rs.bak`、`/tmp/castle.rs.f2bak`（改坏必红前的草稿备份）——改坏必红已完，**可删**；不入库。
- 无起栈 / 无端口占用（全程离线）。
- `/e/tmp/m3cs-scratch/`（命令捕获中间件）——可删；不入库。
