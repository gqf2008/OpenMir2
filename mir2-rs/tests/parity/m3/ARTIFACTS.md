# M3 产物与临时物登记（D2 线）

按设计文档 §14.3（每批收尾清单第 5 条）：本线产生的快照/临时目录/临时库逐个登记位置与去向。
**结论：无长期临时库；临时目录只有"影子副本"，由基线命令自建于会话目录内并被自动清理；
快照 TSV 体积大、不入库但登记位置（可再生）。**

## 一、入库的产物（证据，随 patch 提交）

| 路径 | 内容 | 大小 |
| --- | --- | --- |
| `m3/sessions/csharp-*/baseline.json` | 单侧基线：13 表快照 sha256、作用域行数、改动清单、**runtime_procs**（产出该基线的 7 个进程实际 exe 路径与启动时间）、git 提交 | ~12KB |
| `m3/sessions/csharp-*/snap_{before,after}.manifest.json` | 快照逐表行数 + sha256（快照本体不入库，见下） | ~4KB |
| `m3/sessions/csharp-*/scoped_{before,after,after_norm}.jsonl` | 作用域内的行（按前缀抽取；行数少，含全部判据数据） | <4KB |
| `m3/sessions/csharp-*/{delta,ops,provision,extract_*,dbsnap_*}.json/log` | 判据的原始输出（命令 + 输出） | <20KB |
| `m3/sessions/csharp-*/bot.log` | 假人日志（操作序列判据的原始依据） | <20KB |
| `m3/sessions/m3rng-gate-*/product.json` | RNG 基线：种子、两次运行的取数条数与**确定性前缀 sha256**、公共前缀长度、hook/pack 指纹、掉落与经验序列 sha256、`pack_unchanged` | ~2KB |
| `m3/sessions/m3rng-gate-*/rng-run{1,2}.tsv` | 真实进程的 RNG 取数流（含调用点） | **不入库**（~1.5MB×2，可被 `rng-baseline.ps1` 再生；判据本体是 product.json 里的 `prefix_sha256`） |
| `m3/sessions/m3rng-gate-*/{drop_100kills.txt,exp_table.txt}` | 掉落/经验序列（C# oracle 公式，同种子） | <100KB |
| `m3/sessions/M3-gate-*.json` | 门禁报告：入口实现识别、各步骤退出码、失败项清单、pass | ~4KB |
| `m3/fixtures/rng_stream_seed42_*.tsv` | RNG 记录流夹具（回放门禁用） | ~350KB |

## 二、不入库但需登记位置的产物

| 路径 | 为什么不入库 | 去向 |
| --- | --- | --- |
| `m3/sessions/*/snap_before/`、`snap_after/` | 活库全表导出（单次 ~36MB，含 30 万行 storageitem） | `.gitignore` 已忽略；**可随时用 `run_pair.ps1` 重生成**；manifest 里的 sha256 是判据本体 |
| `m3/sessions/*/shadow-run*/` | 影子副本（217MB/次，含 Map/Envir） | 基线脚本结束时自动 `Remove-Item`（`-KeepShadow` 才留） |

## 三、无临时库

本线**不建临时库**：比对全部落在既有 `mir2_db`/`mir2_account` 上，用"每次运行全新前缀 +
作用域抽取"隔离（详见 `README.md`「为什么同一初始状态不靠破坏性还原」）。
测试账号/角色是**新增行**，清理 SQL 在 `README.md` 末尾（不自动执行）。

## 四、本轮（2026-10-10，worktree worktree-d2-m3-fixtures）实际残留清单

- 已清理：`E:\tmp\m3-rng-shadow\`（上一轮的影子副本与记录，脚本化后不再需要常驻）
- 已清理：本线起的影子 `GameSrv.exe` 进程（按 `$_.Path -like '<会话目录>*'` 过滤结束，线上 oracle 未动）
- 保留并登记：`m3/sessions/m3rng-gate-20261010-164729/`（门禁 GREEN 的完整基线；快照已按默认瘦身为 manifest）
  与 `M3-gate-csharp-20261010-164729.json`（门禁 GREEN 报告）
- 保留并登记：RNG 记录流夹具 `m3/fixtures/rng_stream_seed42_*.tsv`（回放门禁的输入）
- 冻结基线校验：每次基线运行都对比 `E:\MirServer\M2GameSvr` 的 5 个关键文件 sha256
  （`pack_fingerprint` / `pack_unchanged`），本轮结果 `true`

## 五、清理命令（需要时）

```powershell
# 快照与影子（可再生）
Remove-Item -Recurse -Force mir2-rs/tests/parity/m3/sessions/*/snap_before, mir2-rs/tests/parity/m3/sessions/*/snap_after -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force mir2-rs/tests/parity/m3/sessions/*/shadow-run* -ErrorAction SilentlyContinue
# 测试账号/角色（见 README 的清理 SQL，需先人工确认命中范围）
```

## 六、本轮（2026-10-11，M3-Rust 首验）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/sessions/M3-gate-csharp-20261011-142345.json`（C# 基线 GREEN，含 RNG `-VerifyRepeat`）
  - `m3/sessions/M3-gate-rust-20261011-143420.json`（Rust 固定建角 GREEN）、
    `m3/sessions/M3-gate-rust-20261011-143444.json`（Rust 默认 GREEN）
  - `m3/sessions/M3-gate-rust-20261011-143312.json`（缺陷 2 的红报告，作修复前后对照）
  - `m3/sessions/csharp-20261011-142347/`（C# 基线会话，~53KB）、
    `m3/sessions/rust-20261011-143315|143423|143447/`（Rust 三个会话，各 ~50KB）
  - `m3/sessions/m3rng-gate-20261011-142345/`（`product.json` + `drop_100kills.txt` + `exp_table.txt`）
- **不入库（已登记，`m3/.gitignore`）**：`sessions/m3rng-gate-*/rng-run{1,2}.tsv`（~2.2MB×2，可由
  `rng-baseline.ps1` 再生）、`sessions/rust-gates/`（Rust 三网关日志）、`sessions/rust-gates.pid.json`（本机 PID）。
- **无影子副本残留**：`rng-baseline.ps1 -VerifyRepeat` 的 `shadow-run*` 已自动清理。
- **冻结基线**：`pack_unchanged=true`（`E:\MirServer\M2GameSvr` 5 个关键文件 sha256 前后一致）；
  `src/**` 一行未改。收工停栈：`start_rust_gates.ps1 -Stop` + `E:\MirServer\start-all.ps1 -Stop`。
- **测试账号/角色**：本轮新增 `m3142347 / m3143315 / m3143423 / m3143447` 四个命名空间各 4 行
  （account / account_protection / characters / characters_indexes），清理 SQL 见 `README.md` 末尾（不自动执行）。


## 七、本片（2026-10-11，worktree `m3bag`：装备/能力接缝）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/ability_vectors.json`（oracle 真身探针产出的 `WAbil`/`SubAbility`/feature 向量，
    ~19 模板 / 23 用例；Rust 用例回读逐字段核对）
  - `m3/ability-probe/`（探针源码 + csproj；不连 MySQL、不起服务、不写现网文件）
- **未入库/临时物**：探针的 `bin/`、`obj/`（构建产物，已由 `m3/.gitignore` 忽略；可用上面的命令再生）；
  隔离构建目录 `E:/tmp/openmir2-m3bag-target`（登记位置，可删）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改；表结构/脚本/数据文件格式未改。

## 八、本片（2026-10-11，worktree `m3bag`：装备/卸下 handler 核心）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/bag_vectors.json`（oracle 真身探针产出的装备/卸下向量：10 模板 / 21 用例；
    状态 `UseItems`/`ItemList` + 出帧 615/616/619/620 + `SM_ADDITEM` 存在性 + 成功分支重算 `WAbil`，
    Rust 用例回读逐字段核对）
  - `m3/bag-probe/`（探针源码 + csproj；反射调私有 `ClientTakeOnItems`/`ClientTakeOffItems`，
    替身 `INetChannel` 截获真实出帧；不连 MySQL、不起服务、不写现网文件）
  - `mir2-rs/bins/game-svr/src/bag.rs`（接缝实现 + 3 条单测/红检）
- **未入库/临时物**：探针的 `bin/`、`obj/`（构建产物，已被 `mir2-rs/.gitignore` 的 `**/bin/`、`**/obj/` 忽略；
  可用 README 的命令再生）；隔离构建目录 `E:/tmp/openmir2-m3bag-target` 与日志
  `E:/tmp/openmir2-watch/m3bag3-*.log`（登记位置，可删）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改；表结构/脚本/数据文件格式未改；
  未动 `main.rs`/`smframe.rs`（接线归 m2-svr）、未动 `crates/world/**`/`crates/envir/**`。

## 九、本片（2026-10-11，worktree `m3bag`：使用药品 handler 核心）实际残留清单

> 该片当时 `eat_vectors.json` = 8 模板 / 13 用例；后续 slices 已增为 14 模板 / 22 用例（见 §十一/§十二/§十三）。

- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/eat_vectors.json`（oracle 真身探针产出的 `CM_EAT` 向量：8 模板 / 13 用例；
    即时帧 `SM_EAT_OK/FAIL`(635/636) + 延时通知 `RM_HEALTHSPELLCHANGED`(10052)/`RM_WEIGHTCHANGED`(10115)/
    `RM_MENU_OK`(10309) + `ItemList`/`WAbil`/`IncHealth`/`IncSpell`/`UserUnLockDurg`，Rust 用例回读逐字段核对）
  - `m3/eat-probe/`（探针源码 + csproj；反射调私有 `ClientUseItems`，替身 `INetChannel` 截即时帧 +
    反射读 `MsgQueue` 取出队通知；不连 MySQL、不起服务、不写现网文件）
  - `mir2-rs/bins/game-svr/src/useitem.rs`（接缝实现 + 4 条单测/红检）
- **未入库/临时物**：探针的 `bin/`、`obj/`（构建产物，已被 `mir2-rs/.gitignore` 的 `**/bin/`、`**/obj/` 忽略；
  可用 README 的命令再生）；隔离构建目录 `E:/tmp/openmir2-m3bag-target` 与日志
  `E:/tmp/openmir2-watch/m3bag4-*.log`（登记位置，可删）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改；表结构/脚本/数据文件格式未改；
  未动 `main.rs`/`smframe.rs`（接线归 m2-svr）、未动 `crates/world/**`/`crates/envir/**`/`crates/world-bevy/**`。

## 十、本片（2026-10-11，worktree `m3bag`：拾取/丢弃 handler 核心）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/ground_vectors.json`（oracle 真身探针产出的 `CM_PICKUP`/`CM_DROPITEM` 向量：6 模板 / 23 用例；
    出帧 `SM_ADDITEM=200`/`SM_DROPITEM_SUCCESS=600`/`SM_DROPITEM_FAIL=601` + 延时通知（出队序）+
    `ItemList`/`Gold`/`WAbil` + 地图格子前后/全图物品数，Rust 用例回读逐字段核对；tick 平移到固定基点
    `1_200_000` ⇒ 探针产物逐字节可复现）
  - `m3/ground-probe/`（探针源码 + csproj；反射调 `Operate(ProcessMessage)` 的完整 `CM_PICKUP`/`CM_DROPITEM`
    分支 + 真身 `Envirnoment`（反射灌入 64×64 全 Walk 网格）+ 真身 `MapItem`/`UserItem`，替身 `INetChannel`
    截即时帧 + 反射读 `MsgQueue` 取出队通知；不连 MySQL、不起服务、不写现网文件）
  - `mir2-rs/bins/game-svr/src/ground.rs`（接缝实现 + 最小 `GroundMap` + 5 条单测/红检）
  - `mir2-rs/tests/parity/whitelist.md`（更新 T-13/T-15；新增 T-19/T-20）
- **未入库/临时物**：探针的 `bin/`、`obj/`（构建产物，已被 `mir2-rs/.gitignore` 的 `**/bin/`、`**/obj/` 忽略；
  可用 README 的命令再生）；隔离构建目录 `E:/tmp/openmir2-m3bag-target` 与日志
  `E:/tmp/openmir2-watch/m3bag5-*.log`（含 4 条红检，登记位置，可删）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改（探针只**引用** `src/M2Server` 编译，不改源）；
  表结构/脚本/数据文件格式未改；未动 `main.rs`/`smframe.rs`（接线归 m2-svr）、
  未动 `crates/world/**`/`crates/envir/**`/`crates/world-bevy/**`。

## 十一、本片（2026-10-12，worktree `m3bag`：`CM_EAT` 的 `boNODRUG` 守卫 + StdMode 2）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/eat_vectors.json`（oracle 真身探针重生成：**9 模板 / 15 用例**；新增
    `eat_ok_stdmode2_noop`、`eat_fail_nodrug_no_consume`，ctx 增 `no_drug`/`red_msg_fcolor`/`red_msg_bcolor`；
    源锚仍为 `8f668418…`（oracle 源未变））
  - `m3/eat-probe/Program.cs`（Ctx 增 `noDrug`；`po.Envir.Flag.boNODRUG` 灌入；模板 `2011`（StdMode 2）；
    两条新用例；notes 更新）
  - `mir2-rs/bins/game-svr/src/useitem.rs`（`UseItemContext` 增 `no_drug`/`red_msg_fcolor`/`red_msg_bcolor`；
    `RM_SYSMESSAGE=11004` + `QueuedNotice::sysmsg`；`EatItems` 首部禁药守卫（仅 0/1/2/3）+ StdMode 2 分支；
    新增 `stdmode2_eats_without_effect`/`nodrug_guard_does_not_consume` 两条单测/红检，另两处红检已留档）
  - `m3/README.md` / `m3/ARTIFACTS.md` / `tests/parity/whitelist.md`（口径同步：本片做 0/2 + boNODRUG）
- **未入库/临时物**：探针 `bin/`、`obj/`（`mir2-rs/.gitignore` 覆盖）；隔离构建目录 `E:/tmp/openmir2-m3bag-target`；
  红检打印留在本 lane 会话/报告（`E:/tmp/openmir2-watch/last-m3bag.md`）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改（探针只**引用** `src/M2Server` 编译，不改源）；
  表结构/脚本/数据文件格式未改；未动 `main.rs`/`smframe.rs`（接线归 m2-svr）、
  未动 `crates/world/**`/`crates/envir/**`/`crates/world-bevy/**`。

## 十二、本片（2026-10-12，worktree `m3bag`：`CM_EAT` 的 StdMode 1 食物）实际残留清单
- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/eat_vectors.json`（oracle 真身探针重生成：**12 模板 / 19 用例**；StdMode 1 四例
    `eat_ok_stdmode1_hunger_no_recalc`/`_recalc`/`_clamp`、`eat_fail_nodrug_stdmode1`；ctx/case 增
    `hunger_status_before` 与 `expect.hunger_status`；源锚仍为 `8f668418…`（oracle 源未变））
  - `m3/eat-probe/Program.cs`（`Ctx` 增 `hungerStatus`；`po.HungerStatus` 灌入；`po.Abil = CloneAbility(wabilPre)`
    ——让 `RecalcAbilitys` 保留夹具基础能力而不是用 `PlayObject` 构造里的 level-1 缺省；模板
    `2012/2013/2014`（StdMode 1）；四条新用例；`oracle_source` 增 `RefMyStatus`/`GetMyStatus` 行号）
  - `mir2-rs/bins/game-svr/src/useitem.rs`（`UseItemContext` 增 `recalc`；`UseItemActorState` 增
    `hunger_status`；`RM_CHARSTATUSCHANGED=10139`/`RM_MYSTATUS=8102`/`HUNGER_STATUS_MAX=5000`；
    `my_status()`；StdMode 1 分支；`notices` 按 `(byte)ident` 升序 = `MsgQueue` 出队序）
  - `m3/README.md` / `m3/ARTIFACTS.md` / `tests/parity/whitelist.md`（口径同步：本片做 0/1/2 + boNODRUG）
- **未入库/临时物**：探针 `bin/`、`obj/`（`mir2-rs/.gitignore` 覆盖）；隔离构建目录 `E:/tmp/openmir2-m3bag-target`；
  红检打印留在本 lane 会话/报告（`E:/tmp/openmir2-watch/last-m3bag.md`）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改（探针只**引用** `src/M2Server` 编译，不改源）；
  表结构/脚本/数据文件格式未改；未动 `main.rs`/`smframe.rs`（接线归 m2-svr）、
  未动 `crates/world/**`/`crates/envir/**`/`crates/world-bevy/**`。

## 十三、本片（2026-10-12，worktree `m3bag`：`CM_EAT` 的 StdMode 31 `UseStdModeFunItem`）实际残留清单

- **入库（证据，随 patch 提交）**：
  - `m3/fixtures/eat_vectors.json`（oracle 真身探针重生成：**14 模板 / 22 用例**；StdMode 31 三例
    `eat_ok_stdmode31_funitem`/`eat_fail_stdmode31_no_functionnpc`/`eat_fail_stdmode31_unbind_bag_full`；
    ctx 增 `function_npc_present`，模板 DTO 增 `ani_count`；源锚改为
    `d6280417163829787318a8901b60c6c3b2042c7cfe3cd57d4285fe6cdd031c75`（anchor 文件列表增
    `src/OpenMir2/Grobal2.cs`））
  - `m3/eat-probe/Program.cs`（`NpcProxy.CreateMerchant()`；`Ctx` 增 `functionNpc`；`RunCase` 按
    `ctx.functionNpc` 设/清 `SystemShare.FunctionNPC`；`Tpl` 增 `AniCount`；模板 `2015`（AniCount 5）/
    `2016`（AniCount 0）；三条新用例（满包 42 件）；`oracle_source` 增 `UseStdModeFunItem`/`MaxBagItem` 行号）
  - `mir2-rs/bins/game-svr/src/useitem.rs`（`UseItemContext` 增 `function_npc_present`；`use_item` 增
    `31 =>` 分支（`ani_count != 0 && function_npc_present ⇒ eaten`）；`template()` 增 `ani_count`；新增
    `stdmode31_funitem_eats_only_when_function_npc_present` 单测/红检）
  - `m3/README.md` / `m3/ARTIFACTS.md` / `tests/parity/whitelist.md`（口径同步：T-17 收窄到 3/4 + 31 解包）
- **未入库/临时物**：探针 `bin/`、`obj/`（`mir2-rs/.gitignore` 覆盖）；隔离构建目录 `E:/tmp/openmir2-m3bag-target`；
  红检打印留在本 lane 会话/报告（`E:/tmp/openmir2-watch/last-m3bag.md`）。
- **未起栈、未占端口**（5000/7000/7100/7200 未使用）；`E:/MirServer` 十端口状态与开工前一致。
- **冻结基线**：`src/**`（C# oracle）一行未改（探针只**引用** `src/M2Server` 编译，不改源）；
  表结构/脚本/数据文件格式未改；未动 `main.rs`/`smframe.rs`（接线归 m2-svr）、
  未动 `crates/world/**`/`crates/envir/**`/`crates/world-bevy/**`。

## 十四、本片（2026-10-12，worktree `m3bag`：T-14 双侧同序列驱动 + BotSrv 固定背包/装备序列）实际残留清单

- **入库（本片交付）**：
  - `m3/run_bag_equip_pair.ps1`（新）：T-14 的「一条命令」——入口实现识别护栏 + `-BagEquipSequence` 透传
    + `M3BAGSEQ` 整串标记判据 + 跨侧 `scoped_diff.py diff --canonical --fixed-newchr`；
    模式：`-Side csharp|rust` / `-DiffOnly` / `-SelfCheckNoStack`（无栈自检）。
  - `m3/bag_equip_sequence.json`（新）：固定序列定义（每步 `CM_*` 编码、oracle 派发锚点、出包观测面、红检牙齿、已知缺件）。
  - `m3/run_pair.ps1` / `m3/run_m3_gate.ps1`：透传 `-BagEquipSequence/-BagEquipStepMs/-BagEquipStartDelayMs`；
    `run_pair.ps1` 把 `M3BAGSEQ` 标记写进 `baseline.json.bag_equip_markers`，**缺整串即 RED**。
  - `tools/botload/run_bots.ps1`：`-BagEquipSequence/-BagEquipStepMs/-BagEquipStartDelayMs` 写进 `AppSetting.json` 的 `BotPlay` 段；
   新增 `-WriteConfigOnly`（只写配置即退，供无栈自检/红检）。
  - `src/BotSrv/{RobotOptions.cs,MShare.cs,AppService.cs,Player/RobotPlayer.cs}`（**T-22 登记的工具侧例外**，非 oracle）：
   固定背包/装备序列（收到本假人 `SM_LOGON` 后拾取→装备→卸下→丢弃→用药各发一条，参数只取本假人视图；
   每步落 `M3BAGSEQ` 标记）。**默认 `BagEquipSequence=false` ⇒ 不新增任何发送、原路径逐字节不变**。
  - `m3/README.md` / `m3/ARTIFACTS.md` / `tests/parity/whitelist.md`（口径同步：T-14 更新 + 新增 T-22）。
- **未入库/临时物**：无栈自检与红检日志 `E:/tmp/openmir2-watch/m3bag9/*.log`；
  `src/BotSrv/bin|obj`（.NET 构建产物，`.gitignore` 覆盖）；本 lane 报告 `E:/tmp/openmir2-watch/last-m3bag*.md`。
- **未起栈、未占端口**：本片全程未新建 LISTENING；未用 5000/7000/7100/7200（M2 真机验收窗口优先）。
- **边界（逐条）**：
  - **oracle 未改**：`git diff --stat <base> -- src/M2Server src/DBSrv src/LoginSrv src/GameSrv sql MirClient` = 空；
    `src/BotSrv/**` 的改动是 **T-22 登记的工具侧例外**（BotSrv 不在 oracle 七进程内，默认关时逐字节不变）。
  - 未动 `main.rs`/`smframe.rs`（接线归 m2-svr）、`crates/world/**`、`crates/envir/**`、`crates/world-bevy/**`；
    表结构/脚本/数据文件格式未改。
  - 未 merge、未签 review；未新增非 T-22 的 `T-*` 编号（落笔前核过 master 现有编号 T-13..T-22）。

## 十五、本片（2026-10-12，worktree `m3bag`：T-14 复跑口径——关挂机 + 同口径 peer；纯夹具/口径，不占端口）

- **入库（本片交付）**：
  - `m3/run_pair.ps1`：`-BagEquipSequence` 分支**一并传 `-DisableAutoPlay`**（两侧同开关），并把它写进
    `baseline.json.bag_equip_sequence`（`on step_ms=… start_delay_ms=… disable_autoplay=True`）；
    新增 `-EmitOpsArgs`（只打印 run_bots 命令组成后退出，无副作用，供无栈自检核耦合）；
    新增 `-PinStartPose`（固定起点 + 固定初始背包两相：预热建角 → SQL pin 角色初始态
    ［图/坐标/Level + `characters_bagitem` 4 件 + `characters_item` 清空 + `characters_ablity` Level=1］
    → BEFORE 快照 → 选角计分），baseline 增 `start_pose`。
  - `tools/botload/run_bots.ps1`：新增 `-RequireCharCreated`（固定起点①预热：等 N 个「创建角色」再收工；
    默认关 ⇒ 原路径不变；T-22 同族夹具开关）。
  - `m3/gate_guards.ps1`：新增 `Format-M3BagEquipScope` / `ConvertTo-M3BagEquipScope` /
    `Get-M3PeerBagEquipScopeGuard` / `Get-M3SessionBagEquipScope` —— 跨侧「同一组开关」的唯一口径。
    另新增 `Format/ConvertTo-M3StartPose` + `Get-M3PeerStartPoseGuard` + `Get-M3SessionStartPose` +
    `Format-M3StartPoseSql` —— 固定起点变体的唯一口径（两侧 `baseline.json.start_pose` 逐字节比）。
  - `m3/run_m3_gate.ps1`：peer 比对前跑 `Get-M3PeerBagEquipScopeGuard` + `Get-M3PeerStartPoseGuard`
    （场景护栏之后），不一致即跳过 diff；report 的 `bag_equip_sequence` 走同一 label，另记 `peer_start_pose`。
  - `m3/run_bag_equip_pair.ps1`：`-DiffOnly` / `-Side rust -PeerSession` 走同一 peer 口径护栏（开关 + 起点）；
    `-Side rust` 新增 **0.5 preflight**（whitelist **T-27**）：机判 `bins/game-svr` 是否已实现断线存档
    （请求常量 `DB_SAVEHUMANRCD`），未实现 ⇒ PREREQ（exit 2，`-SkipSavePreflight` 仅调试）；
    新增 `-PinStartPose` 透传；`-SelfCheckNoStack` 扩到 9+ 条（含 T-27 preflight 三态与固定起点两相）。
    `-SelfCheckNoStack` 扩到 7 条（配置透传有/无两向 + run_pair 关挂机耦合两向 + 旧挂机 peer 必红 +
    `scoped_diff.py selftest` 14/14 + 入口护栏/DiffOnly fail-closed）。
    顺带修一处**潜伏 bug**：`exit (Invoke-CrossSideDiff …)` 会把函数的 `Write-Output` 全吃进子表达式
    ⇒ 真会话的跨侧 diff 消息不打印、退出码恒 0；改为脚本作用域 `$script:CrossSideDiffRc`。
  - `m3/selftest_gate.ps1`：新增 ④/⑤ 组 13 条构造用例（同开关绿 / 旧挂机 peer 红 autoplay_on / step_ms 不同红 /
    两侧 off 绿 / 本侧 on+peer 缺字段红 / 本侧 off+peer 缺字段绿 / 坏 JSON 红不中断；起点同 pin 绿 /
    pin 不同红 / 本侧 pin+peer 缺字段红 / 两侧 none 绿 / 形态认不出红）⇒ 29/29。
  - `m3/scoped_diff.py`：新增 14/14 用例「仅 `characters.CX` 不同必红」，把「`MapName/CX/CY` 逐字节比」
    钉成断言（这就是关挂机的原因）；`_chr_line` 增 `cx/cy`。
  - `m3/README.md`：新增「T-14 复跑口径（关挂机 + 同口径 peer + T-27 断线存档 preflight + 固定起点变体）」小节；selftest 13/13 → 14/14。
  - `m3/T14-复跑口径报告-20261012.md`（新）：本片报告（判据原始输出 + 残留清单）。
  - `tests/parity/whitelist.md`：新增 **T-27**（Rust game-svr 断线/进世界存档缺失；承接卡 `openmir2-m3-gamesvr-save`）。
- **未起栈、未占端口**：本片全程未新建 LISTENING；未用 5000/7000/7100/7200/3306。
- **边界（逐条）**：oracle `src/**` / `sql/**` / 表结构 / 脚本语法 / 数据文件格式零改动；
  未动 `bins/game-svr/**`（`main.rs`/`smframe.rs` 归 m2-svr）、`crates/**`、`MirClient`；
  未 merge、未签 review。
