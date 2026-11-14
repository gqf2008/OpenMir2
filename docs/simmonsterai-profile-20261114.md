# SimMonsterAi 热点探针与结构分解（2026-11-14）

卡：`openmir2-simmonsterai-profile`（tier L：**只加默认关闭探针 + 测量 + 报告，不改 AI 逻辑**）。
分支：`card/simmonsterai-probe`，基 `f19dfb42f`（master）。测量时段
**2026-11-14 16:20–16:38（+0800）**，release、独立 target `E:/UserTemp/w84-rel`；
未占用真栈窗口（卡面 `window_required=false`），全机禁编窗口（14:20–14:35）之外。

> **口径声明（先读）**
>
> 1. 下表的 ms 是**本机 release、合成世界**里的进程内测量，不是跨机器验收值；可迁移的是
>    **份额、每实体成本、斜率与代码定位**。测量机器与其它车道共享（load 40%–70%），
>    A/B 的绝对值有 ±5%–10% 的跑间噪声，已在 §6.3 用多轮范围给出。
> 2. 本批**没有起真栈**，因此 `HostTickTotal 工作段` 的逐 tick 分位数是 **harness 口径**
>    （`World::tick` head+tail 墙钟；不含 socket/宿主下行/DB 轮询/节拍 sleep）。生产
>    `sim_loop` 的逐 tick 分位数已由本次改动**真的接线并打印**（§6.1），下一次真栈窗口
>    直接可读。
> 3. 合成场景的三处已登记偏差见 §2.3；其中「怪几乎拿不到目标」使 AI 体构成偏向
>    `Wondering`/`base.Run`，真栈还含「取目标/追打」子集（§3.3 用注入目标臂给出该子集的代价）。

## 0. 结论摘要（三个问题的直接回答）

1. **结构（A1）**：在 8,000 实体上，`SimMonsterAi` 的构成是
   **实体遍历 ≈ 8%、门判定 ≈ 8–9%、单怪 AI 体 ≈ 83%**。
   按 A6 真栈窗口的 5.73 ms 折算：**遍历+门判定 ≈ 0.96 ms（17%）、AI 体 ≈ 4.77 ms（83%）**。
   口径：`collect` = `order_ids()` + 身份门 `retain`；`gate` = 复刻门判定的**只读预扫描**
   （`access.get` + `ProcessRunCount` + `IsVisibleActive` 分支），不含真实循环里的
   `set_process_run_count` 写回；`loop` = 真实逐怪循环（门写回 + 全部 AI 体），
   **AI 体 = loop − gate**。
2. **可跳过比例（A2）**：每 tick 被扫的怪里，**66.7% 被 `ProcessMonsterInterval` 节流跳过**
   （`interval=2` 的 2/3），33.3% 真正进 `Run`；被扫怪里 `IsVisibleActive=false` 的占 ~100%
   （8 bot 只覆盖 8 只怪）；**死亡/冻结（`HolySeize`/`StoneMode`/`FixedHideMode`）在稳态为 0**；
   `TargetCret == null` 的占 100%（本仓/oracle **均无仇恨表**，`HateAmount` 不存在，
   仇恨等价于 `TargetCret`）。**这 2/3 的节流跳过已经是既有优化，不能再加**：其余"看起来无事可做"
   的怪仍要跑 `Wondering`（消费 RNG）、`base.Run`（恢复/体力点/`VerifyMapTime`/`MakeGhost` 门），
   按它们跳过会改变 RNG 流与存档/帧面行为。**可安全新增的跳过 = 0**。
3. **超线性（A3）**：5k/8k/11k 三档 `SimMonsterAi` 每实体 **≈0.69 / 0.71 / 0.75 µs**，
   5k→11k 上升 **+8.6%**（五轮范围 +8…+13%），与二次项应有的 +120% 相差一个量级
   ⇒ **主循环是 O(N)，没有随 N 平方增长的子块**。
   但代码里确有**一处 O(dead × N)**：`monai.rs` 的 `base_object_run_monster_impl` 死亡支每执行一次
   就 `access.order_ids()` 全量扫一遍数 `race == PLAY`（`play_object_count`）。
   实测每个**执行中的死亡怪**每 tick 多花 **≈0.61 ms @11k 实体**（= 每个被扫实体 ≈55 ns）；
   `dead=0/8/32/64` 时为 8.22 / 9.99 / 14.95 / 21.21 ms/tick。**死亡怪越多越糟，是潜在 O(N²)**。
4. **分位数（A4，opc 特别要求）**：逐 tick 样本，**不是 50-tick 桶近似**（§6 表；n=200/100）。
   A6 同构档（8,000 实体 + 8 bot）：`SimMonsterAi` p50 **5.53** / p95 **7.60** / p99 **9.53** /
   max **10.06** ms；`HostTickTotal 工作段`（harness 口径）p50 **10.24** / p95 **13.55** /
   p99 **16.07** / max **17.28** ms。11,000 实体档：`SimMonsterAi` p50 7.80 / p95 10.55 /
   p99 12.17 / max 13.36 ms。
5. **零开销（A5）**：探针默认关（沿用 `MIR2_TICK_PROFILE`）；关闭态**不插入探针资源**，
   段内走无探针路径，逐怪循环逐字节不变（代码级见 §7.1）；`off vs none` 多轮范围
   −10.6%…+8.1%、中位 **+0.4%**、方向不一致 ⇒ 在噪声带内（§7.2）。

## 1. 探针清单与切分口径（默认关闭，`MIR2_TICK_PROFILE=1` 开启）

| 探针 / 字段 | 测量范围（代码结构） | 说明 |
| --- | --- | --- |
| `MonaiProbe.collect_ns` | `order_ids()` + 身份门 `retain` | 实体遍历 |
| `MonaiProbe.gate_ns` | 只读预扫描：`access.get` + `ProcessRunCount` + `IsVisibleActive` 分支 | 门判定**模型**；不含写回 |
| `MonaiProbe.loop_ns` | 真实逐怪循环整段（门写回 + 全部 AI 体） | AI 体 = `loop − gate` |
| `counts`（`scanned` / `skipped_throttle` / `ran` / `ran_dead` / `ran_frozen` / `ran_no_target` / `ran_idle_wander` / `ran_walk_due` / `ran_target` / `scanned_inactive`） | 同一只读预扫描 | §4 的分子/分母 |
| `TickProfile.sim_ai_tick_ns` | 每 tick 一条 `SimMonsterAi` 段 ns | tick 级分位数 |
| `HostTickProfile::work_samples` / `HostWindow::work_samples` | 每 tick 一条 `HostTickTotal − HostSleep` | tick 级分位数（生产已接线） |
| `[TICKPROF]` 新增输出行 | `HostTickTotal 工作段` 分位行、`World.SimMonsterAi` 分位行、结构分解与 counts 行 | 每窗口打印（§6.1） |

**为什么 gate 用"模型"而不是逐怪计时**：逐怪包 `Instant` 对 8k 怪 ≈ 1.6 万次
`QueryPerformanceCounter`，会把被测量对象本身抬高数个百分点。这里改用一次**只读预扫描**：
它在真实循环之前执行、只读不写、不抽 RNG，逐 id 复刻门判定的读取与分支，然后用
`AI 体 = loop − gate` 还原。代价是：① 真实门里的 `set_process_run_count` 写回计入 AI 体；
② 预扫描本身在开启态增加约 0.44–0.64 ms/tick 的只读开销（§7.2 的 `on_vs_off`），
所以**份额按 `collect + loop` 归一**，不把预扫描计入分母。

## 2. 测量方法与场景

### 2.1 harness

`mir2-rs/bins/game-svr/src/simmonsterai_bench.rs`（`#[cfg(test)]`、`#[ignore]`、`MIR2_SIMMON_BENCH=1`
二道门；**测量 harness，不是判据**）。每档三臂（同构造、同种子、同 tick 数）：

- `none`：不调用宿主探针、不启用世界探针；
- `off`：调用宿主探针但关闭态（生产缺省形态）；
- `on`：宿主探针 + `world.enable_tick_profile()`（收集逐 tick 样本与 `MonaiProbe`）。

场景与 `tick_bench --profile` / `docs/capacity-model-20261114.md` 同源：200×200、199 列逐行铺满、
`race=80` 怪物、AI/buff/npc/merchant 全开、固定种子 42；8 tick 预热后测 200 tick。

### 2.2 本次新增的两处场景校准（重要）

1. **`ProcessRunCount` 相位去同步**：合成世界的怪同一刻创建 ⇒ 计数同步 ⇒ **每 3 tick 全体同时
   `Run`**，逐 tick 分位数会出现 1×/18× 的假锯齿（首轮测量即如此）。真栈的怪由 MonGen 分批生成，
   相位天然错开。harness 按 `i % (interval+1)` 预置 `ProcessRunCount` 初值。校准后分位数
   从「p50 1.3 / p95 17.9」收敛到「p50 5.4 / p95 7.1」。
2. **目标注入臂**：合成世界的 bot 没有会话，`SearchViewRange` 不把怪标成 `IsVisibleActive`，
   怪也拿不到 `TargetCret`（见 §2.3 偏差 2）。索敌臂（`race=81` `ATMonster`）给「离某个 bot ≤5 格」
   的怪直接置 `IsVisibleActive + TargetCret + TargetFocusTick`（≈5.1% 的怪），把
   「取目标 → `GotoTargetXY`/`AttackTarget`」路径纳入 AI 体构成。

### 2.3 已登记偏差（**绝对值不可直接当真栈读数**）

1. **无 MonGen/无真实 spawn 取数**：怪用 `EntitySpawn` 缺省 AI 计时器（`WalkSpeed=1400` 等），
   没有 `seed_from_spawn` 的 ctor 抽数与 `CoolEye`。`race=80` 不在 `is_active_seek_race` ⇒
   全部怪走 `Wondering`，不索敌。
2. **bot 无会话**：AOI 不扫描它们的视野 ⇒ 仅 8 只怪 `IsVisibleActive=true`，与真栈
   「8 bot × 11×11 视野内约数百只怪可见激活」不同；索敌臂用注入近似。
3. **单图、无守卫/宝宝/城堡结构**：`SimMonsterAi` 的类覆写支（守卫/城门/专用怪）未被覆盖。

## 3. 结构分解（A1）

### 3.1 三档 × 两种 bot 配置（`on` 臂，ms/tick；份额按 `collect + loop` 归一）

| 档 | `collect` 遍历 | `gate` 门判定（模型） | `loop`（门+AI） | AI 体（loop−gate） | 遍历 | 门 | AI 体 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 5,000 / 0 | 0.272 | 0.274 | 2.906 | 2.632 | 8.6% | 8.6% | 82.8% |
| 8,000 / 0 | 0.437 | 0.443 | 4.823 | 4.380 | 8.3% | 8.4% | 83.3% |
| 8,000 / 8 | 0.448 | 0.472 | 4.985 | 4.514 | 8.2% | 8.7% | 83.1% |
| 11,000 / 0 | 0.607 | 0.641 | 7.002 | 6.361 | 8.0% | 8.4% | 83.6% |
| 11,000 / 8 | 0.629 | 0.630 | 7.003 | 6.372 | 8.2% | 8.3% | 83.5% |

**回答**：`SimMonsterAi` 约 **1/6 是"扫描 + 门"，5/6 是单怪 AI 体**。扫描与门判定都已做过
T10 优化（`order_ids` 单次收集 + 原地 `retain`、`ProcessRunCounts` 密集数组），进一步压缩的空间
只在 AI 体。

### 3.2 A6 真栈折算（**推断，不是实测**）

A6 窗口 `SimMonsterAi = 5.73 ms/tick`（7,721→7,999 实体）。按上表 8k 档份额折算：

| 项 | ms/tick | 占 SimMonsterAi |
| --- | --- | --- |
| 实体遍历 | ≈0.48 | 8.3% |
| 门判定（含 `ProcessRunCount` 写回） | ≈0.48 | 8.4% |
| **单怪 AI 体** | **≈4.77** | **83%** |

分摊到实体：5.73 ms × 1000 / 7,860 ≈ **0.73 µs/实体/tick**，与合成 8k 档实测 0.71–0.74 µs/实体
同量级——这是本报告**绝对量可信度**的交叉校验（份额结论不依赖它）。

### 3.3 AI 体内部的路径构成（计数口径）

- **稳态（race 80）**：`ran` 的怪 100% 无目标，其中 **1/3 是 `WalkTick` 到点**（`walk-due`），
  会进 `Wondering`/`WalkTo`；另 2/3 在 `monster_run` 里早退到 `CheckRoyalty`/`CrazyMode` 到期，
  再跑 `AnimalObject.Run`（`HolySeize` 到期）与 `BaseObject.Run`（恢复/体力点/`TargetCret` 清理/
  `VerifyMapTime`/`MakeGhost` 门）。也就是说 AI 体的**大量时间花在"每怪每到的 pass 都要跑的
  公共尾段"**，而不是状态机分支本身。
- **有目标子集（索敌臂，注入 5.1% 的怪有目标）**：8k/8bot 档 AI 体 **5.218 ms**（无目标臂 4.380 ms），
  即 **5.1% 的怪贡献了 AI 体的 ≈16%**——单个"追打怪"的代价约为纯游走怪的 **2×**。
  真栈混有大量追打怪时，AI 体份额只会更偏"有目标"路径，但**遍历/门的 17% 不受影响**。

## 4. 可跳过比例（A2）

### 4.1 每 tick 计数（三档一致，200 tick 平均）

| 项 | 数量/tick（8k 档） | 占 scanned | 说明 |
| --- | --- | --- | --- |
| `slot_total`（身份门之前） | 8,000（+8 bot） | — | 槽位实体总数 |
| `scanned`（身份门之后） | 8,000 | 100% | 进入怪物门的实体 |
| `scanned_inactive`（`!IsVisibleActive`） | 7,992 | 99.9% | 8 bot 只覆盖 8 只怪 |
| `skipped_throttle` | 5,333 | **66.7%** | `!visible_active && run_count < 2` |
| `ran` | 2,667 | **33.3%** | 真正进 `Run` 链 |
| ├ `dead` | 0 | 0% | 稳态无死亡怪 |
| ├ `frozen`（`HolySeize`/`StoneMode`/`FixedHideMode`） | 0 | 0% | 合成场景无 |
| ├ `no-target`（`TargetCret == null`） | 2,667 | 100% | 本仓无仇恨表 |
| └ `walk-due` | 893 | 11.2% | 1/3 的 `ran` 会进走动段 |

### 4.2 映射与"能否跳过"的逐条判定

| 卡面说法 | 本仓/oracle 的等价物 | 现状 | 跳过会改变什么（**结论：不能跳**） |
| --- | --- | --- | --- |
| 睡眠 | **不存在**（oracle `AnimalObject`/`MonsterObject` 无 sleep 状态；最接近的是 `HolySeize`/`StoneMode`/`FixedHideMode` 的"无法行动"） | `frozen` 计数 | 冻结怪仍要跑 `base.Run`（`HolySeize` 到期、恢复、`VerifyMapTime`）；跳过会漏到期与存档 |
| 无目标 | `TargetCret == null` | 100% 的 `ran` | 仍跑 `Wondering`（**消费 RNG**：`Random(20)`、可能 `Random(4)`+`RandomByte(8)`）⇒ 跳过会整体移位 RNG 流，破坏 golden/对拍 |
| 无仇恨 | **不存在**（oracle 无 `HateAmount`；仇恨 = `TargetCret`） | 同"无目标" | 同上 |
| 未激活 | `IsVisibleActive == false` | 99.9% | **已经是节流门的输入**；可见怪必须每个到点 pass 都跑（oracle `MonGen.cs` 的 `else` 支），反向跳过会改变 `ProcessRunCount` 轨迹 |
| 节流未到 | `!visible_active && ProcessRunCount < ProcessMonsterInterval` | 66.7% | **已经在跳**；这是唯一被 oracle 授权的跳过。把 `interval` 调大是**行为变更**（改变 `Run` 频率与 RNG 消费者），不是优化 |

**结论**：`SimMonsterAi` 的"无事可做"比例表面上很高（~2/3 节流跳过、100% 无目标），但
**除既有节流门外没有可安全新增的跳过**。可行的优化方向只能是"让留下的工作更便宜"（§8），
而不是"少跑一些怪"。

## 5. 超线性（A3）

### 5.1 三档缩放（bots=0，200 tick）

| 实体数 | `SimMonsterAi` ms/tick | 每实体 µs | 相对 5k |
| --- | --- | --- | --- |
| 5,000 | 3.458 | **0.6916** | — |
| 8,000 | 5.712 | **0.7140** | +3.2% |
| 11,000 | 8.266 | **0.7514** | **+8.6%** |

五轮（含首轮同步相位与后续校准轮）每实体范围：5k = 0.687–0.716、8k = 0.712–0.804、
11k = 0.716–0.846 µs；**趋势方向一致（+8…+13%），幅度远低于二次项**。
子项也同步微升（collect 0.054→0.055 µs/实体、gate 0.057→0.058、loop 0.581→0.636），
没有"某一子块单独暴涨"的形态 ⇒ 判为工作集/缓存尺寸效应，不是 O(N²)。
**结论：主循环 O(N)**。

### 5.2 唯一的 O(dead × N) 子块（代码定位 + 实测斜率）

`crates/world-bevy/src/monai.rs` 的 `base_object_run_monster_impl` 死亡支：

```text
let play_object_count = access
    .order_ids()                       // ← 每只死亡怪、每个 Run 都全量收集 id
    .into_iter()
    .filter(|id| access.get(*id).is_some_and(|x| x.race == ACTOR_RACE_PLAY))
    .count();
```

oracle 的 `GetMonstersZenTime` 读全服 `PlayObjectCount`；本仓用"扫一遍实体数玩家"复刻。
它**每执行一次死亡支就 O(N) 分配 + O(N) 次 `access.get`**；死亡怪受同一个 `ProcessMonsterInterval`
节流，所以每 tick 的执行数 ≈ `dead / (interval+1)`。实测（N=11,000、`interval=2`、100 tick）：

| 死亡怪数 | 每 tick 执行数 | `SimMonsterAi` ms/tick | 每执行死亡怪增量 |
| --- | --- | --- | --- |
| 0 | 0 | 8.223 | — |
| 8 | 3 | 9.993 | 0.590 ms |
| 32 | 11 | 14.945 | 0.619 ms（相对 8 档） |
| 64 | 21 | 21.212 | 0.627 ms（相对 32 档） |

**斜率 ≈ 0.61 ms / 执行中的死亡怪 / tick @11k**（= `0.61ms / 11,000` ≈ **55 ns / 被扫实体**），
对死亡数**线性、无饱和** ⇒ 与实体数相乘即 **O(dead × N)**。
如果一个真栈窗口里同时有十几只"死亡未 Ghost"的怪，这一项就能吃掉数 ms——**必须用真栈探针核实**
（本次改动已在 `[TICKPROF]` 输出里加了 `dead` 计数，见 §6.1）。

## 6. 逐 tick 分位数（A4，opc 特别要求）

### 6.1 生产接线（本批改动）

- World 侧：`TickProfile::sim_ai_tick_ns` 每个 tick 追加一条 `SimMonsterAi` 段 ns
  （环形上限 `SIM_AI_SAMPLE_CAP = 1024`，防长跑诊断会话无界增长；另有单调计数
  `sim_ai_samples_total`，窗口增量按它算，截断时既不漏也不重复）；
  `MonaiProbe` 的窗口累计与计数一并由 `tick_head` 合并。
- Host 侧：`HostTickProfile::end_tick` 每个 tick 追加一条 `HostTickTotal − HostSleep`
  （`work_samples`）；`take_window` 带走。
- `log_window`（生产 `sim_loop` 的每窗口汇总）新增三行：
  `HostTickTotal 工作段 逐 tick 分位数`、`World.SimMonsterAi 逐 tick 分位数`、
  `SimMonsterAi 结构分解 + counts/tick`。分位数用**最近秩** `s[ceil(p/100·n)−1]`，
  **逐 tick 样本，不是 50-tick 桶近似**。
- 复现真栈读数：起栈时设 `MIR2_TICK_PROFILE=1`，`grep TICKPROF` 即可（下一次 A6 类窗口直接可得）。

### 6.2 harness 读数（`on` 臂，逐 tick，n=200 / 索敌臂 n=100）

| 档 | 序列 | n | p50 | p95 | p99 | max | mean |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 5,000 / 8 bot | `SimMonsterAi` | 200 | 3.335 | 4.409 | 5.496 | 5.903 | 3.568 |
| 5,000 / 8 bot | Host 工作段* | 200 | 6.355 | 8.066 | 8.948 | 9.753 | 6.577 |
| **8,000 / 8 bot** | **`SimMonsterAi`** | **200** | **5.531** | **7.596** | **9.534** | **10.063** | **5.916** |
| **8,000 / 8 bot** | **Host 工作段*** | **200** | **10.242** | **13.553** | **16.069** | **17.279** | **10.727** |
| 8,000 / 0 bot | `SimMonsterAi` | 200 | 5.383 | 7.146 | 7.579 | 7.918 | 5.712 |
| 8,000 / 0 bot | Host 工作段* | 200 | 10.015 | 11.750 | 12.861 | 14.193 | 10.321 |
| 11,000 / 8 bot | `SimMonsterAi` | 200 | 7.796 | 10.545 | 12.172 | 13.358 | 8.276 |
| 11,000 / 8 bot | Host 工作段* | 200 | 14.275 | 17.468 | 19.429 | 21.142 | 14.783 |
| 11,000 / 0 bot | `SimMonsterAi` | 200 | 7.703 | 10.395 | 12.702 | 13.220 | 8.266 |
| 11,000 / 0 bot | Host 工作段* | 200 | 14.253 | 17.421 | 19.702 | 20.362 | 14.778 |
| 索敌臂 8,000 / 8 bot | `SimMonsterAi` | 100 | 6.203 | 10.011 | 10.912 | 11.221 | 6.599 |

\* **harness 口径**：`HostTickTotal 工作段` = `World::tick` head+tail 的整墙钟（无 socket/宿主下行/
DB 轮询/节拍 sleep）。生产 `HostTickTotal` 工作段（A6 窗口均值 19.41 ms）还含宿主其它段，
不能与本列直接比；生产口径的分位数由 §6.1 的接线在下一个真栈窗口输出。

### 6.3 波动与解释

- 平均 > p50：11k 档 `mean 8.28 > p50 7.80`，尾部来自「`WalkTick` 到点的那 1/3 同相位聚集」与
  宿主侧其它段（AOI 首扫每 5 tick 一次）叠加；校准相位后 p95/p99 与 p50 的比值从 1×/18× 收敛到
  1.35×/1.55×，仍保留真实的周期性。
- 跑间噪声：测量机与其它车道共享（load 40%–70%）。同一档的 `SimMonsterAi` 均值在五轮间漂移
  ±10%（如 11k/0：8.27–9.30 ms）。**份额、计数、斜率、每实体趋势**在五轮间稳定。

## 7. 关闭态零开销（A5）

### 7.1 代码级

- 门控未改：仍只有 `MIR2_TICK_PROFILE` 字面 `"1"` 开启。
- **关闭态（生产缺省）**：`tick_head` 的 `monai_probe_on` 为假 ⇒ **不插入** `MonaiProbe` 资源
  ⇒ `sim_monster_ai_stage_system` 的 `remove_resource::<MonaiProbe>()` 返回 `None` ⇒ 走
  `monster_ai_tick_impl`（无探针路径）。该路径只把关闭态 `MonaiProbe::default()` 传进
  `monster_ai_tick_impl_probed`，其中：
  - `timer_begin()` 一次 `bool` 判别 ⇒ `None`（**不取 `Instant`**）；
  - `record_collect` / `record_loop` 一次 `Option` 判别 ⇒ 直接返回（**不写字段**）；
  - `scan_gate` 整段被 `if probe.enabled` 挡住 ⇒ **不执行**。
  **逐怪循环体与改动前逐字节相同**（探针没有插进循环内部）。
- Host 侧：`end`/`end_tick` 多一次 `name == "HostSleep"` 比较与一次 `saturating_sub`/`push`
  （仅 `enabled`，关闭态在 `let Some(at) = at else { return }` 处已返回）；`sim_ai_tick_ns`/
  `work_samples` 只在开启态增长。
- 行为中性牙：`monai_probe_is_behavior_neutral`（探针开/关两世界跑 8 pass，断言 RNG 流位置、
  实体坐标/朝向/目标/计时器/HP、`ProcessRunCount` 全部相同）。

### 7.2 同进程 A/B（多轮）

| 关系 | 含义 | 多轮范围 | 判定 |
| --- | --- | --- | --- |
| `off vs none` | 关闭态接线开销 | **−10.6% … +8.1%**，中位 ≈ +0.4%，符号不一致 | 噪声带内（本机与他车道共享） |
| `on vs off` | 探针自身开销（只读预扫描） | **+1.2% … +9.0%**，均为正 | 符合"每 tick 多一次 O(N) 只读扫描"的预期；只在诊断态发生 |

交付轮（`simmon-ai-final5.txt`）的 `off_vs_none`：−1.28 / −0.47 / +0.53 / +5.29 / +1.49 / −0.04 %；
`on_vs_off`：+4.39 / +3.57 / +1.55 / +3.17 / +3.78 / +3.61 %。

## 8. 优化建议（**只写建议，本批未实现任何优化**）

按"不改行为"的可行性排序：

| # | 候选 | 目标 | 做法（行为等价） | 预期收益（按本报告份额） | 风险 |
| --- | --- | --- | --- | --- | --- |
| A | **死亡支 `play_object_count` 去 O(N)** | §5.2 的 O(dead×N) | 维护"`race == PLAY` 实体计数"（宿主在登录/离场/变身时增减），或把 `play_object_count` 每 tick 算一次传给死亡支 | 每个执行中的死亡怪从 0.61 ms → ~0（@11k）。A6 若有 10 只死亡怪，可拿掉 ~6 ms/tick | 计数必须在所有增删点维护（`enter_map`/`leave`/`ghost`/变身）；与 oracle 的"全服数量"语义必须逐点等价 |
| B | **公共尾段按门短路** | AI 体 83% 里的 `base_object_run` | 恢复段已有 `health_fill_time`/`spell_fill_time` 门；检查 `VerifyMapTime`/`TargetCret` 清理是否可在"无待办"时跳过**且不改变写点** | 未知，需先按"每怪每 pass 的公共尾段"再细分探针 | 任何跳写都会改变存档/帧面；必须逐门证明写点不变 |
| C | AI 访问形态（`EntityAccess::get` 组合视图） | AI 体的每实体固定成本 | 与 T10c 同路线：减少 `EntityRef` 构造次数/批量读 | 每实体 0.7 µs 里的一部分 | 大改，且触及 `StageAccess` 口径；需独立卡 |
| D | 有目标子集的追打路径 | §3.3 的 ~2× 单价 | 先测量真栈的目标子集占比，再决定是否值得动 | 5% 的怪 ≈16% AI 体 | 攻击/走位语义强约束，风险高 |

**容量含义**：A6 的 `SimMonsterAi` 5.73 ms 里 **83% 是单怪 AI 体**；只做"少跑怪"不可行（§4），
所以优化必须落在 B/C；A 只对"有死亡怪"的窗口有效，但一旦有死亡怪，它是**超线性**的，优先级
取决于真栈 `dead` 计数（下一次真栈窗口可直接读到）。

## 9. 边界（本报告**没有**证明什么）

1. **没有真栈窗口**：`SimMonsterAi` 的份额/计数结论来自合成 harness；A6 的 5.73 ms 折算只是把
   份额乘上去（§3.2），不是实测分解。真栈的 `dead` 数、目标子集、可见激活比例都未测。
2. `HostTickTotal 工作段` 的逐 tick 分位数**在生产上已接线但未采集**：本报告给的是 harness 的
   `World::tick` 墙钟口径（§6.2 星号）。
3. 三处场景偏差（§2.3）使"AI 体构成"偏向 `Wondering`/`base.Run`；索敌臂用注入近似，注入率
   （5.1%）是假设值。
4. 单机、单进程、单图；未涉及 NUMA/GC（无）/socket 写与背压。
5. `race=80/81` 两种怪；守卫/城门/专用怪的类覆写支未覆盖。

## 10. 复现与证据

```text
# 构建（独立 target，避免污染其它车道）
cd mir2-rs
CARGO_TARGET_DIR=E:/UserTemp/w84-rel cargo test --release -p game-svr --lib --offline --no-run

# 测量（三档 × 两种 bot × 三臂 + 索敌臂 + 死亡臂；约 45s）
MIR2_SIMMON_BENCH=1 MIR2_SIMMON_TICKS=200 CARGO_TARGET_DIR=E:/UserTemp/w84-rel \
  cargo test --release -p game-svr --lib --offline -- --ignored --nocapture simmonsterai_bench \
  > E:/UserTemp/w84-measure/simmon-ai-final5.txt 2>&1
```

- 交付读数：`E:/UserTemp/w84-measure/simmon-ai-final5.txt`（sha256 见卡面条目）。
- 过程留痕：`simmon-ai-run1.txt`（同步相位，分位数假锯齿）、`run2/run3`（相位校准）、
  `final`–`final4`（注入/死臂迭代）。**只有 `final5` 是交付读数**。
- 探针单元测试（均为红→绿牙）：
  - `monai::mb89_tests::monai_probe_is_behavior_neutral`（探针不改变 RNG/状态/计数）；
  - `monai::mb89_tests::monai_probe_counts_throttle_gate`（skip/skip/ran 轨迹 + `slot_total` 口径）；
  - `world::tests::sim_monster_ai_profile_records_per_tick_samples`（每 tick 一条样本 + 资源插入/取回）；
  - `tick_profile::tests::end_tick_records_work_segment_per_tick`（工作段 = total − sleep）；
  - `tick_profile::tests::percentile_ns_uses_nearest_rank` / `world_delta_carries_per_tick_samples_without_repeating`。
