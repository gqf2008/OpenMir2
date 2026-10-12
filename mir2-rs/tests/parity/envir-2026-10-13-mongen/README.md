# 证据包：M3④-实现 B 分片① —— `Envir/MonGen.txt` 解析（含 `loadgen` 包含）

- 卡：`openmir2-m3-mongen-load-spawn`（M3④-实现 B；本片 = 三片中的第 ① 片）
- 车道：`m2-svr` / principal `openmir2-worker-11`；分支 `worktree-m2-stamina-healthtick`
- 本片写集：`mir2-rs/crates/envir/**`（+ 本证据目录）；**未碰** `tests/parity/m3/**`（m3bag 写集）、
  `crates/world/**`、`bins/game-svr/**`（留给同一张卡的第 ② / ③ 片）
- 未起栈、未占端口、未连 MySQL（本片判据全在构造层 + 真机文件只读）

## 1. oracle 锚定（逐条在工作树 `src/` 实读，行号按 base `1b576172`（C# 源冻结，与 `fa82187a` 逐行相同））

| 语义 | oracle 位置 |
| --- | --- |
| 主文件名 `GetEnvirFilePath("MonGen.txt")` | `src/GameSrv/DB/LocalDB.cs:508` |
| `loadgen` 子文件判定 | `src/GameSrv/DB/LocalDB.cs:520`（`HUtil32.CompareLStr("loadgen", 行)`） |
| 子文件追加 `LoadMapGen` | `src/GameSrv/DB/LocalDB.cs:476-499`（`:494` 路径 = `sFileDir + sFileName` **字符串拼接**） |
| 8 列解析（分隔符 `{ ' ', '\t' }`） | `src/GameSrv/DB/LocalDB.cs:532-557`（分隔符定义 `:12`） |
| 入库校验 | `src/GameSrv/DB/LocalDB.cs:558-570`（`MapName`/`MonName` 非空 && `ZenTime > 0` && `GetMapInfo != null`） |
| `Count` 截断 `(ushort)` / `Range` `(byte)` / `ZenTime` | `:544-550` |
| `MonGenInfo` 字段 + `ThreadMonLimit=40000` + `TryAdd`（`>` 上限） | `src/Modules/SystemModule/Data/MonGenInfo.cs:9-57` |
| 返回值 `mongenCount = MonGenCount` = **Σ Count** | `src/GameSrv/Word/WorldServer.cs:111` |
| `FindMap`/`GetMapInfo` 的 map 存在性 | `src/GameSrv/Maps/MapManager.cs:146-162` |
| `CompareLStr(src,targ)` = 以 **targ** 为前缀 | `src/OpenMir2/HUtil32.cs:550-576` |
| `GetValidStr3` / `GetValidStrCap` / `CaptureString` / `ArrestStringEx` | `src/OpenMir2/HUtil32.cs:299-473`、`:488-548` |

原始件：`01-oracle-localdb-monogen.txt`、`02-oracle-compare-lstr.txt`、`03-oracle-moninfo-and-mappmgr.txt`。

## 2. 本片交付的 Rust API（`crates/envir/src/mongen.rs`）

- `pub struct MonGenInfo`（`map_name/x/y/mon_name/range/count/active_count/zen_time/mission_gen_rate/race/start_tick`）
  + `MonGenInfo::cert_list_has_room`（`MonGenInfo.TryAdd` 的 `>` 闸门，`MonGenInfo.cs:41-51`）；
- `pub const TEXT_SPLIT_CONST = [' ', '\t']`、`pub const MON_GEN_THREAD_MON_LIMIT = 40000`；
- `parse_mon_gen_line(line, map_exists) -> Result<Option<MonGenInfo>, EnvirError>`（单行 1:1）；
- `mon_gen_count(&[MonGenInfo]) -> i32`（= `WorldServer.cs:111` 的 `Sum(Count)`）；
- `MapLoader::mon_gen_path()` / `mon_gen_dir()` / `load_mon_gen(&MapRegistry) -> Result<Option<Vec<MonGenInfo>>, EnvirError>`；
- `load_map_gen_into(dir, file_name, lines)`（`LoadMapGen` 的目录创建 + 字符串拼接路径 + 追加）。
- 新 `EnvirError::MonGenCaptureString`（对应 oracle `CaptureString` 抛出的 `IndexOutOfRangeException`，无 try/catch 兜底）。

## 3. **两条刻意保留的 oracle 缺陷**（§1 硬约束③：行为等价优先于修 bug）

1. **`loadgen` 包含实参写反 ⇒ 子文件包含实际不可达**。
   `CompareLStr(src, targ)` 以 `targ` 为前缀（`HUtil32.cs:550-576`，`compn = targ.Length`，
   且要求 `src.Length >= compn`）。`LocalDB.cs:520` 传的是 `CompareLStr("loadgen", 行)` ⇒
   `compn = 行长`、要求 `"loadgen".Length(7) >= 行长` ⇒ 只有行长 ≤ 7 才可能为真；
   而那种行经 `GetValidStr3` 取到的「子文件名」必为空串 ⇒ `if (!string.IsNullOrEmpty(sMapGenFile))` 不成立
   ⇒ **永远不会展开任何子文件**。部署件 `Envir/MonGen.txt` 也**没有任何 `loadgen` 行**（实测 grep 0 命中）。
   本片保留判定与展开代码路径（1:1），并在 `tests/mongen.rs::loader_does_not_expand_loadgen_include` 上牙：
   把实参"修对" ⇒ 必红（见 `32-redcheck-comparelstr-reversed.txt`）。
2. **子文件路径是字符串拼接**：`sFilePatchName = sFileDir + sFileName`（`LocalDB.cs:494`，非 `Path.Combine`），
   照抄为 `format!("{}{}", file_dir.display(), file_name)`。

## 4. 判据与实读

- 构造级单测 8 条（`mongen.rs::tests`）：部署件两行形态、注释/空行、校验丢弃、`(byte)/(ushort)/(short)` 截断（含负值）、
  引号名走 `CaptureString` 的 oracle 偏差、`CompareLStr` 实参方向、`LoadMapGen` 拼接、加载器缺文件/存在两态。
- 集成判据 1 条（`tests/mongen.rs::loader_does_not_expand_loadgen_include`）。
- **真机部署件实读**（`11-real-deployment-reading.txt`，只读仓外 `E:\MirServer`）：
  `MonGen entries=2827 sum_count=39019 maps_loaded=322`
  （`MIR2_MONGEN_DIR='E:\MirServer\M2GameSvr' cargo test -p mir2-envir --test mongen -- --ignored`）。
- **独立 Python 复刻交叉核对**（`12-crosscheck-python.txt`）：
  `maps_in_registry=322 / entries=2827 / sum_count=39019` —— 与 Rust 逐项一致。

> 说明：C# 侧 `LoadMonGen` 的 `mongenCount` 直接探针尚未跑（需编译 C# 或两侧窗口）；本片以
> 「Rust 1:1 行号 + 独立 Python 复刻」两组证据交叉，C# 探针登记给本卡后续分片/真机窗口。

## 5. 改坏必红（3 条，原始件 + 还原核对）

| 编号 | 变异 | 命中用例 | 原始件 |
| --- | --- | --- | --- |
| RC-1 | 删掉 `zen_time <= 0` 校验 | `drops_rows_failing_oracle_validation` ⇒ FAILED | `30-redcheck-zen-time-gate.txt` |
| RC-2 | `GetValidStrCap` 换成 `GetValidStr3`（丢引号语义） | `quoted_name_keeps_oracle_capture_string_quirk` ⇒ FAILED | `31-redcheck-getvalidstrcap.txt` |
| RC-3 | `CompareLStr("loadgen", 行)` 反转为 `CompareLStr(行, "loadgen")` | `loader_does_not_expand_loadgen_include` ⇒ FAILED（子文件被展开，多出 `B` 行） | `32-redcheck-comparelstr-reversed.txt` |

每次变异后用备份还原并 `diff -q` 核对 byte-identical；还原后复跑 GREEN。

## 6. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`）

| 门 | 结果 | 原始件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates/10-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates/11-clippy.txt` |
| `cargo run -p msgcodegen -- --check` | rc=0（604 条） | `gates/12-msgcodegen.txt` |
| `cargo test --workspace --offline` | rc=0（**714 passed / 0 failed / 18 ignored**，合并 master `1b576172` 后重跑） | `gates/13-test-workspace.txt` |

（`msgcodegen` 首次因共享 target 里的旧二进制内嵌了失效的 `CARGO_MANIFEST_DIR` 而 panic；`touch` 源码重建后 rc=0，见原始件。）

## 7. 边界

- `git diff --stat -- src/ sql/ MirClient` = 空（oracle/客户端/表结构/脚本/数据格式零改动）。
- 改动面：`crates/envir/src/{mongen.rs(新),error.rs,lib.rs}`、`crates/envir/tests/mongen.rs(新)`、本证据目录。
- **未动** `tests/parity/m3/**`（m3bag 写集）、`crates/world/**`、`bins/game-svr/**`。

## 8. 残留 / 下一片

- 第 ② 片（`crates/world`：`InitializeMonster` 的按怪名分片 + `ProcessMonsterMultiThreadLimit` +
  `RegenMonsters`/复活 tick 的 RNG 取数次序）、第 ③ 片（`bins/game-svr` 接线）未在本片；
  真机两侧 100 杀对拍要窗口（本卡 DoD⑤ 已登记等 owner），且依赖承接卡 `openmir2-m3-monitems-attach`。
- `loadgen` 包含的 oracle 缺陷若 owner 要「修 bug」，须单开 `B-*` 白名单条目 + 审批，本片不做。
