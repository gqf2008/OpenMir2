# loader-fields 残余接线：game-svr 消费新出生点加载器（2026-10-25）

- 卡：`openmir2-loader-fields`（既有 **R** 卡残余；owner `openmir2-dev-1`；worktree `.worktrees/dev-1`；分支 `card/loader-fields`）。
- 基线：本地 `master` `99156c2c6`（含 `d0b380ab4` 的 `crates/envir/src/startpoint.rs`）。
- 写集：`mir2-rs/bins/game-svr/src/homemap.rs`、`mir2-rs/bins/game-svr/src/lib.rs`（调用点）、本文件。
- 红线：未起栈、未占端口、未连库；未改场景/规则表。

## 0. 一句话

`bins/game-svr` 不再自带第二份「只取三列」的 StartPoint 解析：`homemap.rs` 改为
`pub use mir2_envir::startpoint::StartPoint`（测试内直接 `use mir2_envir::startpoint::parse_start_points`），文件半改调
`mir2_envir::startpoint::load_start_points_if_exists`。现网三列部署件上 `map_name/x/y` **逐位不变**；
新增承载 oracle 的 5 列（`NotAllowSay/Range/Type/PkZone/PkFire`），并把「读取失败」从静默空表改为 fail-closed。

## 1. oracle 证据（行号按本仓库 `src/` 实读）

| 项 | oracle 行为 | 证据 |
|---|---|---|
| 起点/地图/坐标 | `LoadStartPoint`：`File.Exists` 门；`Trim()` 后空行/`;` 跳过；8 次 `GetValidStr3(TextSplitConst)`；`MapName/CX/CY` 非空才入库；`CurrX/CurrY = StrToInt16(列, 0)` | `src/GameSrv/DB/LocalDB.cs:822-867`（尤其 `:842-843`、`:845-859`、`:860`、`:862`） |
| 缺列默认值 | 其余 5 列 `StrToInt(列, 0)`；`NotAllowSay = Convert.ToBoolean(StrToInt(allSay, 0))`（**非 0 即 true，负数也为 true**）；`Shape` 无赋值点 ⇒ 恒 0 | `LocalDB.cs:862-863`；`src/OpenMir2/Data/StartPoint.cs` |
| 出生点选择 | `StartPointList.Count > StartPointSize` ⇒ 1×`Random(StartPointSize)`，否则索引 0；空表 ⇒ `Config.HomeMap` + 照抄 `nX = Config.HomeY` 笔误 | `src/GameSrv/Word/WorldServer.cs:1405-1428`；调用点 `:248-260` |
| 朝向 | **不在 `StartPoint`**（结构体无该字段）；新角色朝向来自 `humData.Dir` | `WorldServer.cs:1108` `playObject.Dir = humData.Dir`；`StartPoint.cs` 字段表 |
| 读取失败 | `LoadFromFile` 抛出，调用点无 try/catch ⇒ 启动期 fail-closed | `LocalDB.cs:834`；`GameApp.cs:105-108` 调用无 catch |

## 2. 接线改动

| 文件 | 改动 |
|---|---|
| `bins/game-svr/src/homemap.rs` | 删除私有 `StartPoint`（3 字段）+ `parse_start_points` + `str_to_i16` + `TEXT_SPLIT`；`pub use mir2_envir::startpoint::StartPoint`（测试内 `use mir2_envir::startpoint::parse_start_points`）；`load_start_points` 改为 thin adapter（`start_point_path` + `load_start_points_if_exists`），返回 `Result<_, EnvirError>` |
| `bins/game-svr/src/lib.rs` | 启动调用点 `homemap::load_start_points(&base, &cfg.envir_dir)?`（读失败 fail-closed） |
| `crates/envir/src/startpoint.rs` | **未改**（既有加载器即事实源） |

`HomeFallback.start_points` 的类型随 re-export 变为 envir 的 8 字段 `StartPoint`；`get_home_info`/`new_human_spawn*`
只读 `map_name/x/y`，RNG 抽数次序与次数不变。

## 3. 合同测试（逐位）与差异解释

| 测试 | 钉什么 |
|---|---|
| `wiring_matches_legacy_three_column_view` | 现网 8 条三列行：`map_name/x/y` 与旧三列解析**逐位相同**（旧值以字面量固化）；5 个新列显式为默认值；`shape=0` |
| `wiring_carries_columns_beyond_legacy_three` | 8 列行：`NotAllowSay/Range/Type/PkZone/PkFire` 逐字段承载（旧三列解析会丢弃） |
| `wiring_gate_and_illegal_values` | 缺列丢弃；非法数值 ⇒ `StrToInt16` 取默认 0 但行仍入库；`allSay=-1` ⇒ `true` |
| `parse_real_head_matches_oracle_tokens`（既有，更新） | 部署件 8 条 + 三列默认值 |

**与旧解析的差异（可解释）**：

1. `map_name/x/y`：三列部署件上**零差异**（合同测试固化旧值）。
2. 5 个新列：旧解析**丢弃**，新加载器按 oracle 承载；本接缝尚未消费（窗口项 ② 的未来接线）。
3. 读取失败：旧实现 `Err(_) => Vec::new()` 静默空表 → 新实现 `Err` 上抛。oracle `LoadFromFile` 异常无 catch
   ⇒ fail-closed；这是**修正旧偏差**，不是新偏离（已在本文件登记）。
4. 行切分：旧 `split('\n')` + 手工剥 `\r`；新 `str::lines()`。CRLF/LF 等价；单独 `\r` 不可达（与 `crates/data::lists` 同款登记）。

## 4. 牙（≥2，实测「改坏红 + 还原绿」）

在生产解析器 `crates/envir/src/startpoint.rs` 上做单点变异，只跑 `cargo test -p game-svr --lib wiring`：

| 变异 | 期望 | 结果 |
|---|---|---|
| 去掉 `c_y` 非空门（缺列被收录） | FAILED → 还原 ok | 红 / 绿 |
| `y` 默认值 `0 → 1` | FAILED → 还原 ok | 红 / 绿 |
| `NotAllowSay` 口径 `!= 0 → > 0`（负数漏判） | FAILED → 还原 ok | 红 / 绿 |
| `range` 恒 0（旧三列语义：丢弃第 5 列） | FAILED → 还原 ok | 红 / 绿 |

4/4 全红后还原绿；变异文件已逐字节还原（`git status` 仅本卡的两个源文件 + 本报告）。

## 5. 门禁

fmt / clippy / `cargo test --workspace` / msgcodegen 全 rc=0；卫生门禁（conflict-marker / stale-pointer /
dynamic-count / board-coverage / t_reservation / whitelist-anchor / m3-registry）全 rc=0；`git diff --check` rc=0；
`git merge-tree` 无冲突。接线后 `grep -rn "envir::startpoint" mir2-rs/bins mir2-rs/crates/world` = 4 行（均为本接缝）。

## 6. R 级窗口项（合并后保持 in-progress）

- **W-LF-1 真机**：`StartPoint.txt` / `Server.conf` 出生点 11 键的取值与落点（新角色进图的 map/x/y/朝向）。朝向按
  `WorldServer.cs:1108` 从 `humData.Dir` 取，**不经** StartPoint；真机同时观察首包地图/坐标是否与 oracle 一致。
- **W-LF-2 接线后消费**：本片只承载 5 个新列，尚无消费点（`MakeSafePkZone`/`InSafeZone` 属后续片）；
  真机验证这些字段在安全区判定接入后的行为。窗口未跑 ⇒ 卡保持 `in-progress` + `window_required=true`。

## 7. 判不准 / 残余

- `StartPoint.Shape` 恒 0 的结论沿用加载器注释（`LoadStartPoint` 无赋值点）；本片未再逐行复核 C# 其它写入点。
- 「朝向」不在加载器范围内（结构体无该字段）；本报告只给 oracle 行号，未改朝向路径。
- 残留：`%TEMP%` 下变异备份/脚本已删；未起栈、未占端口、未连库。
