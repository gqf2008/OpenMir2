# 证据包：M3④-实现 B 分片② —— `crates/world` MonGen 调度（分组 / regen 门 / 复活 tick / RNG 次序）

- 卡：`openmir2-m3-mongen-load-spawn`（M3④-实现 B；本片 = 三片中的第 ② 片）
- 车道：`m2-svr` / principal `openmir2-worker-11`；分支 `worktree-m2-stamina-healthtick`
- 本片写集：`mir2-rs/crates/world/**`（+ 本证据目录）；**未碰** `tests/parity/m3/**`（m3bag 写集）、
  `crates/envir/**`（第 ① 片已交）、`bins/game-svr/**`（留给第 ③ 片）
- 未起栈、未占端口、未连 MySQL（判据全在构造层）
- base = 第 ① 片被审 head `56cbd896`（第 ① 片 patch `8d101d71`）；本片只含 `crates/world` 差量

## 1. oracle 锚定（逐条在工作树 `src/` 实读，行号按 base `1b576172`，C# 源冻结）

| 语义 | oracle 位置 | Rust |
| --- | --- | --- |
| 按怪名分组（`OrdinalIgnoreCase`）+ 每名 1 次 `Random(ProcessMonsterMultiThreadLimit)` | `WorldServer.MonGen.cs:32-88` | `initialize_monster` |
| `MonsterList.Values` 各再抽 1 次 | 同文件 `:91-99` | 同上 |
| `MonsterThreadMap.Add(MonGenList[i].MonName, threadId)` —— **用 `MonGenList[i]` 下标而非 `monsterNames[i]`**（原文如此） | 同文件 `:83-86` | 同上（1:1 保留错位缺陷） |
| `MonGenList.Clear()` | 同文件 `:112` | 所有权转移进 `MonGenState::entries` |
| `GetMonstersZenTime`（`<30min` 且 `d10>0` ⇒ 夹 6 加速） | 同文件 `:168-190` | `get_monsters_zen_time` |
| regen 闸门 `(now - RegenMonstersTick) > RegenMonstersTime`（严格大于） | 同文件 `:207` | `regen_gate` |
| `CurrMonGenIdx` 轮询（越界取 0、推进） | 同文件 `:211-230` | 同上 |
| `genModCount = max(1, Round(max(1,Count)/(MonGenRate/10.0)))` | 同文件 `:232` | `gen_mod_count` |
| `canCreate = !(map==null \|\| map.Flag.boNOHUMNOMON && map.HumCount<=0)` | 同文件 `:235-243` | `regen_gate` 的 `can_create` 回调 |
| `genModCount > ActiveCount && canCreate` ⇒ `RegenMonsters`；成功才回写 `StartTick` | 同文件 `:244-252` | 同上 |
| 复活条件 `Death && CanReAlive && Invisible && MonGen!=null` 且 `(now-ReAliveTick) > GetMonstersZenTime` | 同文件 `:283-291` | `monster_due_for_revive` |
| ghost 5 分钟清理 | 同文件 `:302` | `monster_ghost_expired` |
| `RegenMonsters` 集中/普通两支的取数次序 | 同文件 `:811-874` | `regen_monsters` |
| `CreateMonster` 内 RNG（`RandomByte(8)` / 掉落 / `Random(100)` CoolEye / 地形移位） | 同文件 `:740-752`、`:778-793` | `create_monster` 回调契约 |
| `MonGenInfo.TryAdd` 的 `>`（`ThreadMonLimit=40000`） | `SystemModule/Data/MonGenInfo.cs:41-57`、`:68` | `MonGenEntry::try_add` |
| `GetMonRace` 未命中 = -1；`GetMonsterThreadId` | `WorldServer.cs:167-180` | `initialize_monster` 的 `race_of` 回调 / `monster_thread_map` |
| 配置缺省与部署值 | `GameSvrConf.cs:32/64/68/329/334/1127/1142` + `ServerConf.cs:21/63/64/66/261/636/637`；部署 `E:/MirServer/M2GameSvr/Server.conf` | `MonGenConfig::default`（部署 `ProcessMonsterMultiThreadLimit` 无该键 ⇒ 默认 1） |

原始件：`01-oracle-anchors.txt`（含 `sed -n` 原文片段与部署 conf 关键行）。

## 2. 本片交付的 Rust API（`crates/world/src/mongen.rs`，`lib.rs` 再导出）

- `MonGenConfig`（`process_monster_multi_thread_limit/mon_gen_rate/regen_monsters_time/process_monster_interval/zen_fast_step/user_full/venture_server/mon_limit/zen_limit`，缺省 = `GameSvrConf` 构造缺省）；
- `MonGenEntry`（`MonGenInfo` 的运行时列 + `cert_list/thread_id`）+ `try_add`（`>` 闸门）；
- `MonGenThread` / `MonGenState`（`MonGenInfoThreadMap` + `MonsterThreadMap`，thread id 升序遍历）；
- `initialize_monster(...)` → `(MonGenState, InitializeMonsterRecord)`；
- `get_monsters_zen_time` / `gen_mod_count` / `should_regen` / `regen_gate` / `step_threads` / `regen_monsters`；
- `monster_due_for_revive` / `monster_ghost_expired`；
- 回调类型别名 `MonsterCreator` / `MapCanCreate` / `TickClockFn`（`CreateMonster` / `canCreate` / `GetTickCount` 的注入点）。

**RNG 次序契约**：`regen_monsters` 在两次位置抽之后、下一只的位置抽之前**同步**调用
`create_monster`（见函数文档）；`CreateMonster` 内部的 Dir/掉落/CoolEye 抽因此与 oracle 的
interleaving 逐调用相同。第 ① 片的解析本身不抽数。

## 3. 判据与交叉验证

- 构造级单测 **17 条**（`mongen.rs::tests`）：分批抽数、`OrdinalIgnoreCase` 分组、`MonGenList[i]` 错位缺陷、
  `GetMonstersZenTime`（含 30min 阈值与 d10 夹 6）、`gen_mod_count`、regen 门/`ActiveCount`/`VentureServer`、
  集中支与普通支位置窗口、`race<=0` 不抽、普通支 `null ⇒ false`、ZenLimit 超时（两支）、轮询与 `StartTick`、
  复活条件与 ghost 过期。
- **独立 Python 复刻交叉验证**（`02-crosscheck-python-mongen-rng.py.txt` → `03-crosscheck-output.json`）：
  用 `DotNetRandom`（与 .NET 实机对拍过的复刻）独立重放两支位置序列，Rust 侧断言逐项相等：
  - `initialize_monster` seed=42/limit=2 ⇒ 抽数序列 `[1,0,0,1,0]`；
  - 普通支 seed=42/range=2/(166,266)/count=4 ⇒ 位置 `[(167,264),(164,265),(164,267),(166,265)]`，
    且尾部下一次 `Random(1000000) = 517451`（同时钉住 create 回调的 interleaving）；
  - 集中支 seed=42/range=3/count=3/MissionGenRate=100 ⇒ `[(163,256),(163,256),(158,263)]`。

> 脚本以 `.py.txt` 入库：`mir2-rs/.gitattributes` 只声明了 `*.txt/*.json/*.md` 为 LF（`*.py` 尚未声明），
> 用具名 `.txt` 尾巴可保证 `core.autocrlf=true` 的**干净检出**也保持 LF（否则 eolcheck 会因 `.py` 落 CRLF 判红）。
> 运行：`python -I 02-crosscheck-python-mongen-rng.py.txt`。

## 4. 改坏必红（4 条，原始件 + 还原核对）

| 编号 | 变异 | 命中用例 | 原始件 |
| --- | --- | --- | --- |
| RC-1 | 普通支 x/y 位置抽互换 | `regen_positions_match_independent_python_normal` ⇒ FAILED（2 条） | `30-redcheck-mut1-swap-xy.txt` |
| RC-2 | 去掉 `Random(100)` 集中支判定 | `regen_positions_match_independent_python_mission` ⇒ FAILED | `31-redcheck-mut2-mission-gate.txt` |
| RC-3 | `GetMonstersZenTime` 去掉 d10 夹 6 | `get_monsters_zen_time_matches_oracle` ⇒ FAILED（0 vs 24000） | `32-redcheck-mut3-zen-clamp.txt` |
| RC-4 | 复活边界 `>` 改 `>=` | `monster_due_for_revive_and_ghost_expiry` ⇒ FAILED | `33-redcheck-mut4-revive-boundary.txt` |

每次变异后从备份还原并 `diff -q` 核对 byte-identical（`mongen.rs` sha256 =
`1c43ba4b29c901c933e78b13a222b9e32ada441e7184bbb863c527d1d9041a18`），还原后复跑 GREEN。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`）

| 门 | 结果 | 原始件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates/10-fmt.txt` |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 | `gates/11-clippy.txt` |
| `cargo run -q -p msgcodegen --offline -- --check` | rc=0（604 条） | `gates/13-msgcodegen.txt` |
| `cargo test --workspace --offline` | rc=0（**86 suites / 731 passed / 0 failed / 18 ignored**） | `gates/12-workspace-test.txt` |

（731 = 第 ① 片合并 master `1b576172` 后的 714 + 本片 17 条。）

## 6. 边界

- `git diff --stat master -- src/ sql/ MirClient` = **空**（oracle/客户端/表结构/脚本/数据格式零改动，见 `20-boundary.txt`）。
- 本片改动面：`crates/world/src/mongen.rs`（新）+ `crates/world/src/lib.rs`（模块注册/再导出）+ 本证据目录。
- **未动** `tests/parity/m3/**`（m3bag 写集）、`crates/envir/**`、`bins/game-svr/**`。

## 7. 刻意保留 / 已登记的 oracle 缺陷与建模差异

1. `MonsterThreadMap.Add(MonGenList[i].MonName, …)` 用 `MonGenList[i]` 下标（`:84`）——当同名前缀错位时
   某些怪名不会写入 `MonsterThreadMap`；有专门用例
   `initialize_monster_thread_map_uses_gen_list_index_not_name_index` 钉住（1:1 保留，不"顺手修"）。
2. `MonGenInfoThreadMap` 穿插遍历（`:101-110`）在 `limit>1` 且某 thread id 未抽中时会抛
   `KeyNotFoundException`（部署 `limit=1` 不触发）；本实现按 thread id 升序遍历实际存在的线程（模块头已登记）。
3. 线程模型：oracle 每条 `ProcessMonsters` 跑在独立 `Thread` + 墙钟 + `Thread.Sleep(20)`；
   本 crate 单线程串行、显式注入时刻（`TickClockFn`），部署 `limit=1` 时两者一致。

## 8. 残留 / 下一片

- 第 ③ 片（`bins/game-svr` 接线：`load_mon_gen` → `initialize_monster` → 宿主循环 `step_threads`，
  并实现 `create_monster` 回调把 `MonsterInfo` 的能力装载 + `RandomByte(8)`/掉落预算/CoolEye/地形移位接上）
  未在本片；`CreateMonster` 的实体构造面与承接卡 `openmir2-m3-monster-ai-tick`（C）有边界，接线时按写集分工。
- 真机两侧 100 杀对拍要窗口（本卡 DoD⑤ 已登记等 owner），依赖承接卡 `openmir2-m3-monitems-attach`。
