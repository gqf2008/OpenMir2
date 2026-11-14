# World 全量 ECS 迁移 · 总验收报告（2026-11-14）

- **验收对象**：World 全量 ECS 迁移（S0–S5）+ 10k 容量模型 + 宿主段 AOI 走位扇出优化 + 联调车道（bot 走位）
- **master head**：`f19dfb42f`（AOI 扇出优化 merge）
- **报告性质**：Leader 汇总；数字逐条给出口径与出处，不做四舍五入放大
- **口径**：本文性能数字来自**本机 release 构建**，不跨机器当验收值；端到端 10k 为**外推**，置信度低（见 §5）

## 0. 结论

1. **架构目标达成**：20/20 个 tick 阶段已切成段系统（`TickStage`），`tick_head` 只剩边界代码；
   `crates/world` 已退役并删除（`176cfb609`，内容提交 `877c645cc`），全 workspace 只剩 `world-bevy` 一份 ECS 实现，
   包别名与 `mir2_world::` 引用全部清零。
2. **质量门禁达成**：head `f19dfb42f` 上 `cargo test --workspace --no-fail-fast` = **155 段 / 3888 passed / 0 failed / 70 ignored**；
   golden 6/6 逐字节相同；parity 0 failed；fmt / clippy / msgcodegen / docs anchor 全绿。
   回滚面可证：tag `world-archive-20261114` + 仓外 tar（sha256 `f12c1862…`）冷克隆 `cargo check --workspace` rc=0。
3. **World 侧性能达成**：10k 玩家基准场景（39.5k 实体）`World::tick` **49.2ms = 200ms 预算的 24.6%**（优化前同场景 339ms = 169%）；
   11k 实体 bench 每实体 **1.209µs**（回归修复前 8.58µs，−85.9%）。
4. **10k 端到端尚未验收**：端到端外推 ≈**187ms = 94%** 预算（置信度低，未做 1000 bot 真栈全量）；
   距内容上限 53.5k 实体压到 ≤50% 预算还差 ≈**2.5×**，主战场已从 `World::tick` 转到宿主段。
   这一条是「未完成」，不是「达标」。

## 1. 交付物

| 交付 | 内容 | 证据 |
| --- | --- | --- |
| ECS 迁移 S1–S5 | 151 字段 → 25 组件；20 `TickStage`；`crates/world` 退役 | `docs/world-ecs-migration-20261113.md`、`docs/world-ecs-s4d3-entitymut-20261113.md`、`docs/s5-execution-20261114.md` |
| 容量模型 | 10k 场景 39.5k 实体的分段账 + 外推模型 | `docs/capacity-model-20261114.md` |
| AOI 扇出优化 | 走位帧收件人候选集，bench 71–79×，真栈 0.35–0.45µs/走位者 | `docs/host-aoi-profile-20261114.md` §11 |
| SimMonsterAi 探针 | 段内切分 + O(dead×N) 死亡支定位（**尚未合并**，见 §7） | 卡 `openmir2-simmonsterai-profile`，head `a76958c9` |
| 联调车道 | bot 侧走位阻塞消除（地形 + actor 避让） | `E:/UserFleet/archive/movefail-annotated.csv`、`run-mf3b/`、`run-mf5/` |

里程碑提交：

- P0 组件图与骨架：`f355edfa7`
- S4d 收口（20/20 系统化）：`80f97231e`
- S5 `crates/world` 退役：`176cfb609`（内容 `877c645cc`）
- 容量模型 + 回归修复：`068061346`
- AOI 扇出优化：`f19dfb42f`

## 2. 架构结果

- **组件化**：`Entity`（原 `CoreIdentity`）+ 24 个组件；`EntitySpawn` 142 个构造字段；`SimState` 为 88 字段资源。
- **段序**：20 个 `TickStage` 按 oracle 语义线性注册（Clock / Command / … / OutputAoi），
  段注册序有牙（`tick_stage_registrations_are_wired_in_order`）；RNG 消费次序合同（§5.2）由三族 trace diff 守着。
- **适配器**：`EntityAccess` / `StructuralAccess` / `StageSpec`（编译期白名单）；结构段用 `S: Structural` 编译期门，
  写集在类型上声明，越界是编译错而不是运行时漏改。
- **确定性**：只用单线程 executor；`MultiThreadedExecutor` / `bevy_tasks` / `rayon` / `crossbeam` 由 `no_parallel` 守卫拦死
  （守卫在删 crate 前先移植到 `world-bevy/tests` 并验证「注入并行 ⇒ 红」）。
- **S5 物理退役**：26 个模块 + 20 个集成测试 + `tick_bench` 并入 `world-bevy`（47/47 与 rustfmt 后源文件逐字节相等）；
  8 个 crate 根重名消歧；6 个 Cargo 点原子翻转；`mir2_world::` → `mir2_world_bevy::` 2506 处、53 个 `.rs`；
  删除 `mir2-rs/crates/world` **49 个跟踪文件**（删除前 `git status --ignored` 为空）。
- **授权链**：删除动作由 Owner 飞书「没用就删掉」（`message_id om_x100b60494b5708a0c1f4b8cfdac8f5b`，2026-11-14 11:07）授权，
  范围经总监限定为 `crates/world` 退役；以「原子分支变更 + 删除」执行，不是裸 `git rm`。

## 3. 验证证据（head `f19dfb42f`）

| 面 | 结果 | 谁跑的 |
| --- | --- | --- |
| 全量测试 | 155 段 / **3888 passed / 0 failed / 70 ignored** | reviewer-25（merge review 实跑） |
| golden | **6/6 逐字节相同** | 同左 |
| parity | **0 failed** | 同左 |
| FIFO 牙 | 绿（同 tick 入队序 = 处理序） | 同左 |
| fmt / clippy / msgcodegen | rc=0 / rc=0 / rc=0（604） | 同左 |
| docs anchor | GREEN（`--base f19dfb42f`） | 同左 |
| RNG trace | 三族 `seq/kind/max/result` 四列 diff = 0 | S4d 收口证据 |
| 回滚 | tag + tar 冷克隆 `cargo check --workspace` rc=0 | S5 执行文档 |

- 迁移期间 **每卡独立审查**（作者与审查者不同 principal），审查抓到并修复的真实问题举例：
  P6b2 读路径回归（`StageAccess::get` 每组件一次 `world.get`，11k 实体 53→94ms）；
  容量卡把回归修复误归因到 c1（拆提交重测后改归 c3）；S5 初版分支基落后 6 个提交（rebase 后按 master 语义解 2 处冲突）。
- 复审打回与返工记录全部在 `refs/collab/*` 签名线程里，可逐条回读。

## 4. 性能结果

### 4.1 `World::tick`（11k 实体，release，100 tick）

| 构建 | `World::tick` | 每实体 | `SimMonsterAi` |
| --- | --- | --- | --- |
| `d186e2d3f`（P6b2 前基线） | 53.0ms | 4.82µs | 6.99ms |
| `80f97231e`（P6b2 后，回归） | 94.34ms | 8.58µs | 52.63ms |
| 修复后（`068061346`，最终窗口） | **13.30ms** | **1.209µs** | — |

按提交独立可复现：`78942f106`（回归修复）17.19ms → `ce87f221e`（+AOI 直取槽表）14.13ms → `19742f17a`（+AI 中间表）13.30ms。
debug 与 release 的系数 ≈25×（World 侧独立验证 144.0µs / 5.82µs ≈ 24.7×）。

### 4.2 容量模型（10k 玩家基准 = 39.5k 实体）

| 项 | 优化前 | 现在 |
| --- | --- | --- |
| `World::tick` | 338.9ms（169.4% 预算） | **49.2ms（24.6%）** |
| 端到端（外推） | — | **187.4ms（93.7%）**，低置信度 |
| `World::tick` 单独触顶 | — | ≈160,600 实体（内容上限的 3.0 倍） |

### 4.3 AOI 走位扇出

bench（同 harness、同种子）：

| 档 | 改前 | 改后 run1 / run2 | 倍数 |
| --- | --- | --- | --- |
| 7,908 / 0 bot | 43.736ms | 0.018 / 0.019 | ~2,300–2,400× |
| 11,000 / 100 bot | 94.976ms | 0.096 / 0.090 | ~990–1,050× |
| **11,000 / 1,000 bot** | **107.762ms** | **1.358 / 1.521** | **71–79×** |

真栈 A6（8 bot，实体 7,721→7,999，16:00:30–16:02:00）：

- `HostAoiWalkFanout` **0.009–0.012ms/tick**（31–32 次/tick）⇒ **0.35–0.45µs/走位者**（优化前 263.6µs）
- 父段 `HostAoiMonsterWalk` **1.10–1.48ms/tick**（bench 同量级 43.7ms）
- 工作段合计 **19.41ms = 200ms 节拍的 9.68%**（`HostTickTotal` 200.6ms 含末尾节拍 sleep，不是「一 tick 花了 200ms」）
- 等价信号：五档 `frames/tick`、`hides/tick` 与改前**逐位相同**；3 颗变异（删 `!ghost` / 加 `add_to_mapped` / `<=` 改 `<`）全红

### 4.4 SimMonsterAi 探针（待合）

- 段内份额：遍历 8.5% / 门 8.4% / AI 体 83.3%；**可安全新增跳过 = 0**（无仇恨表、无休眠；无目标怪仍抽 RNG 并跑 `base.Run`）。
- 主循环 O(N)（5k→11k 每实体 −1.0%），但**死亡支 O(dead×N)**：`base_object_run_monster_impl` 对每只死怪调 `order_ids()` 数 `race==PLAY`，
  ≈0.58–0.66ms/只；dead=0/8/32/64 @11k → 9.05/11.02/15.66/**22.50ms**（复审独立复跑值）。
- 复审结论 request_changes，仅 docs 两处事实（F1 倍数口径、F2 测量时段），代码与 A1–A6 通过。

## 5. 10k 容量判定

| 目标 | 现状 | 判定 |
| --- | --- | --- |
| 10k 玩家（39.5k 实体）World 侧 ≤ 50% 预算 | 24.6% | **达成**（4.1× 余量） |
| 内容上限 53.5k 实体端到端 ≤ 50% 预算 | 外推约 187ms→更高，估算还需 ≈2.5× | **未达成** |
| 端到端真栈实测（1000 bot） | **未做** | **未验收** |

结论：**World 侧不再是瓶颈**；下一张性能卡的主战场是宿主段（AOI 下行 / 收包 / 帧构造）。
本报告不把外推值当验收值；1000 bot 真栈窗口是 10k 端到端验收的前置条件。

## 6. 联调车道（bot 走位）

| 指标 | 地形修复前 | 修复后 / 避让后 |
| --- | --- | --- |
| 服务端 MOVEFAIL 记录 | 131（地形 79 = 60.3% / actor 阻塞 52 = 39.7%） | 地形修复后 61 → 避让后 **0 条**（mf5 日志） |
| bot 走位成功率（ok / walk 行） | mf3b 267/453 = **58.9%** | mf5 329/435 = **75.6%** |
| 总步数（8 bot 合计） | mf3b 1455 | mf5 **1833（+26%）** |

说明：客户端（`mirbeta/MirClient` Delphi）冻结未动；这里改的是**联调用 bot 客户端**与服务端日志/判据，
MOVEFAIL 分类来自 `E:/UserFleet/archive/movefail-annotated.csv`（131 行逐条标注 `terrain_ok`）。

## 7. 未完成项与已知红

**未完成（有卡，按序推进）**

1. `openmir2-simmonsterai-profile`：复审 request_changes，作者补 docs（F1/F2）后合并。
2. `openmir2-monai-deathbranch-count`：死亡支改一次计数；判据用**结构断言**（旧 N 次 → 新 1 次），墙钟只作观察值。
3. `openmir2-s5-stale-path-refs`：195 处注释 + 5 处断言/文本串仍写 `crates/world` 旧路径。
4. 容量文档勘误：`docs/capacity-model-20261114.md` §3 c1 行 `OutputAoi` 0.43 → 4.207（增益归 c2）。
5. 1000 bot 真栈窗口：10k 端到端验收的前置。

**已知红（既有，非本次引入，S5 交付时逐条记录在 `docs/s5-execution-20261114.md` §10）**

- `butch_ledger` selftest 的 k/n（工具函数区间在 S5 之前已漂移）
- 三个 RNG 账本脚本 selftest 行号漂移（`attack_rng_ledger` 6/70、`rng_ledger_diff` 5/57、`check_anchors` 20/21，master 上同样红）
- `dynamic_count_stamp_gate` 1 条缺戳、6 条 `SHA256SUMS` mismatch（S5 时点记录）

## 8. 证据索引

- 设计/执行：`docs/world-ecs-migration-20261113.md`、`docs/world-ecs-s4d3-entitymut-20261113.md`、
  `docs/s5-prep-20261114.md`、`docs/s5-execution-20261114.md`、`docs/tick-hotspot-profile-20261113.md`、
  `docs/capacity-model-20261114.md`、`docs/host-aoi-profile-20261114.md`
- 归档：tag `world-archive-20261114`；`E:/UserFleet/archive/world-crate-20261114.tar.gz`（sha256 `f12c1862…`）；
  `E:/UserFleet/archive/hostaoi-verify-20261114/`；`E:/UserFleet/archive/run-mf*/`、`E:/UserFleet/run-mf5/`；
  `E:/UserFleet/s5-final-tests.log`
- 线程：`refs/collab/*`（每卡的 issue / patch / review / merge_result 逐条签名可回读）
