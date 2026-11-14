# 10k 容量模型 + T10 Top2–Top4 第一版（2026-11-14）

卡：`openmir2-capacity-model-10k`（L 级；性能优化 + 容量建模）。
分支：`card/capacity-model`，基 `80f97231e`（master）。
前置数据：`docs/tick-hotspot-profile-20261113.md`（T10 / T10b，分段计时与真栈宿主段）。
测量工具：`mir2-rs/crates/world/src/bin/tick_bench.rs` 的 `--profile`（release）；分段口径
= `World::tick`（`tick_head` + `tick_tail` 的 20 个 `TickStage`），**不含**宿主段
（`bins/game-svr/src/lib.rs` 的 `sim_loop`，见 T10b）。

> **口径声明（先读）**
>
> 1. 本文 `World::tick` 的绝对 ms 来自本机 release 构建（独立 target `E:/UserTemp/w83c-target`，
>    测量窗口内无其它重型编译进程，CPU 探针 ~30%），**不把绝对 ms 当跨机器验收值**；
>    横向结论一律用「同一进程、同种子、同 tick 数的 A/B」与「µs/实体」。
> 2. 端到端（World + 宿主段）**没有**本机 release 真栈测量。它由 T10b 的 dev 真栈数据
>    按「构建口径 25×」外推（该系数由 World 侧独立验证：dev 真栈 144.0µs/实体 vs
>    release bench 5.82µs/实体 ≈ 24.7×）。该外推**置信度低**，只用于给出量级与方向，
>    不作为验收值；本文 §4.5 明确列出它漏算的项。

## 0. 结论摘要

1. **本卡开工时 master 上存在一个性能回归**（P6b2 迁段引入，非本卡造成）：
   `World::tick` 每实体 **4.82µs → 8.58µs（+78%）**，11k 实体 53.0ms → **94.3ms**；
   其中 `SimMonsterAi` 从 **0.64µs/实体**（6.99ms）涨到 **4.78µs/实体**（52.6ms）。
   根因：`StageAccess::get` 逐个组件走 `StageEntities::get::<C>`，一个 `EntityRef` = 24 次
   「`TypeId → ComponentId` 解析 + archetype 解析」；AI 的 Run 链每次实体访问都付这个价。
2. 本卡修复该回归并完成 Top2/Top4 第一版：11k 实体
   **94.34ms → 13.30ms**，每实体 **8.58µs → 1.209µs（−85.9%）**（§3 表；最终窗口
   `final2.txt`，CPU 14%、0 个重型编译进程；多次窗口落在 1.20–1.25µs，±4% 噪声）。
   **按提交能独立复现**：`78942f106`（回归修复）单独 = 17.19ms，`ce87f221e`（+AOI）=
   14.13ms，`19742f17a`（+AI 中间表）= 13.30ms。
3. 10k 玩家基准场景（39.5k 实体）：
   - `World::tick` **49.2ms = 200ms 预算的 24.6%**（优化前同场景 339ms = 169%）；
   - 端到端外推 **≈187ms = 94%**（World 49.2 + 宿主段 ≈138）。
   （模型表按 1.245µs/实体计算；最终窗口实测 1.209µs ⇒ 表中数字**偏保守约 3%**。）
   ⇒ **瓶颈已从 `World::tick` 转移到宿主段**（AOI 下行等，T10b 测得宿主段占 tick 18%→38.5%）。
4. 要在**内容上限 53.5k 实体**（MonGen 39,019 + 10k 玩家 + 宝宝 + 其它）下把端到端压到
   **≤50% 预算（100ms）**，还需 **≈2.5×** 的端到端改进；`World::tick` 侧只需再 1.7× 就能
   独自给到 100ms 以内的 53.5k（当前 66.6ms），因此**下一张性能卡的主战场是宿主段，不是 world crate**。
5. 上面的线性模型是**下界**：`--bots` 边界实测（11k 怪 + 1,000 玩家）显示两项玩家相关成本——
   **视野扫描** `OutputAoi` +3.11ms（外推 10k 玩家 ≈ +31ms/tick）与**可见性效应**（可见怪绕过
   1/3 节流，AI 最坏 ≈3×，基准 25k 怪 ≈ +25…32ms/tick）。两者都不在「每实体固定成本」里，
   详见 §4.5。**把 §4.2/§4.3 当成下界读**。

## 1. 测量方法

- 命令（`mir2-rs/` 下，独立 target）：
  ```sh
  CARGO_TARGET_DIR=E:/UserTemp/w83c-target cargo build --release -p mir2-world --bin tick_bench
  E:/UserTemp/w83c-target/release/tick_bench.exe --profile 1000 100 5000 100 11000 100
  ```
  每档跑两遍（A 计时未启用 / B 计时启用，同种子、同 tick 数）⇒ 墙钟比值即计时开销；
  各档 A/B 的 ΔP50 均在 ±2.3% 内（同机噪声）。
- 场景同 T10 §2：200×200 地图、199 列逐行铺满、race=80、AI/buff/npc/merchant 全开、
  无玩家移动命令；11k 实体全在图内。
- **参照构建**：为判断回归来源，额外在**本车道 worktree**（`detached`）上构建并测量
  P6b2 之前的一个提交（`d186e2d3f`，SimMerchant 段落地那一版），同一 target、同一命令。
  该测量只用于归因，不是交付内容；测完已切回 `card/capacity-model`。

## 2. 关键发现：P6b2（`SimMonsterAi` 迁段）引入的读路径回归

| 构建 | 11k 实体 `World::tick` | 每实体 | `SimMonsterAi` | `SimDup`（全量扫描段代表） |
| --- | --- | --- | --- | --- |
| `d186e2d3f`（P6b2 之前，T10 基线） | 53.0ms | 4.82µs | 6.99ms（0.635µs/实体） | 5.78ms |
| `80f97231e`（本卡基，P6b2 之后） | **94.34ms** | **8.58µs** | **52.63ms（4.78µs/实体）** | 5.06ms |

- 同一台机器、同一 target、同一 harness ⇒ 差异不可能来自测量口径。
- `SimDup` 等**非 `EntityAccess` 路径**的扫描段没有变差（Top1 预编译白名单生效，反而略降）；
  只有经过 `EntityAccess::get` 的段（AI、结算、清表、VerifyMapTime）付了回归价。
- 机制（代码已核）：`StageAccess::get`（`mir2-rs/crates/world/src/entity.rs`）旧实现是
  `resolve` + **24 次** `ad.get::<C>`；每次 `StageEntities::get::<C>` 都做
  「`SlotTable` 解析 + 白名单表查 + `world.get::<C>`（`TypeId` 哈希 + archetype 解析）」。
  而 facade 的 `EntityStore::get` 用的是 3 个缓存 `QueryState` 的 `get_manual`。
  AI 的 Run 链每次实体访问都要构造 `EntityRef`，于是 11k 实体下多出几十毫秒。
- **这不是 T10 的 Top1 没做好**：Top1（预编译白名单）把每组件核对从 ~25 条短链降到 1 次查表，
  已生效（非 `EntityAccess` 段 4 倍加速可见）；回归来自「每组件一次 `world.get`」这一层，
  Top1 管不到。

## 3. 优化结果（同一 harness，11k 实体，100 tick）

| 提交（逐项累积） | `World::tick` ms/tick | µs/实体 | `SimMonsterAi` ms | `OutputAoi` ms |
| --- | --- | --- | --- | --- |
| 基线 `80f97231e` | 94.343 | 8.577 | 52.625 | 4.293 |
| `78942f106` 段访问器组合读视图（回归修复，§5.1） | 17.192 | 1.563 | 7.317 | 0.43 |
| `ce87f221e` + AOI 直取槽位表 / 去 HashMap / 单次快照（§5.2） | 14.125 | 1.284 | 7.708 | 0.450 |
| `19742f17a` + AI 中间表（单次分配 + `ProcessRunCounts`）（§5.3） | **13.298** | **1.209** | 6.98–7.11 | 0.45 |

> 每行是该提交**检出后单独构建**的一次 11k 测量（同一 harness、同一 target；不同窗口，
> ±10% 读作噪声）。审查者独立复测的 base = 93.27ms，与本表的 94.34 同量级。
> 更早一次窗口在 `78942f106` 上测到 21.60ms（同内容、机器更忙），故 §5.1 同时给出两个值。

三档缩放（最终构建 `final.txt`）：1k = 1.195µs/实体、5k = 1.191µs/实体、11k = 1.245µs/实体
⇒ 仍然**线性**，每实体成本跨三档漂移 <5%，没有新的超线性项。

最终 11k 分段（ms/tick；占比 = 该次测量段合计的百分比）。下表是 **T10c-2 之后、T10c-3 之前**
的一次完整测量（12 个段一次跑完）；T10c-3 只改 AI 内部与本表外的三处小项，其余段不受影响，
T10c-3 之后：`SimMonsterAi` **6.98–7.11ms**（低负载窗口）、`World::tick`
**13.69ms / 1.245µs/实体**（最终窗口 `final.txt`）。

| 段 | ms/tick | 占比 | µs/实体 | 优化前 ms（同段） |
| --- | --- | --- | --- | --- |
| `SimMonsterAi` | 7.504 | 53.78% | 0.682 | 52.63 |
| `SimNpc` | 0.682 | 4.89% | 0.062 | 5.32 |
| `TailPlayerDeaths` | 0.659 | 4.72% | 0.060 | 5.11 |
| `SimDup` | 0.654 | 4.69% | 0.059 | 5.06 |
| `TailVerifyMapTime` | 0.632 | 4.53% | 0.057 | 5.08 |
| `SimMerchant` | 0.631 | 4.52% | 0.057 | 5.16 |
| `SimSettleDeaths` | 0.625 | 4.48% | 0.057 | 5.12 |
| `SimSlaveCleanup` | 0.624 | 4.48% | 0.057 | 5.03 |
| `SimExpireHitters` | 0.477 | 3.42% | 0.043 | 0.47 |
| `OutputAoi` | 0.428 | 3.07% | 0.039 | 4.29 |
| `TailSuperManRefill` | 0.406 | 2.91% | 0.037 | 0.41 |
| 其余 9 段合计 | 0.609 | 4.39% | — | — |
| **段合计** | **13.931** | 100% | 1.267 | **94.30** |

> 现在 `SimMonsterAi` 是唯一的大项（53%）。它已经回到 P6b2 之前的水平
> （0.635µs/实体 vs 0.635µs/实体），其成本主体是「1/3 怪物每 tick 真跑 Run 链」
> （`ProcessMonsterInterval=2`），不是中间表分配 —— 再往下压需要改 Run 链本身的访问形态，
> 属另一张卡。

## 4. 10k 并发玩家的实体预算模型

### 4.1 实体估算（假设与依据）

| 项 | 基准 | 低 | 高 | 依据 / 说明 |
| --- | --- | --- | --- | --- |
| 玩家本体 | 10,000 | 10,000 | 10,000 | 卡目标：10k 并发会话 ⇒ 10k 玩家实体（每个会话 1 个） |
| 宠物/宝宝 | 2,500 | 1,500 | 5,000 | 职业构成假设：道士 25%、其中 60% 带 1 只，另有骷髅/神兽；基准 = 0.25 只/玩家 |
| 地图怪 | 25,000 | 8,000 | 39,019 | 基准 = 2.5 只/玩家；上限 = MonGen 内容上限 39,019（207 图）；低端 = 演示栈实测自然稳态 ~7.9k 的量级 |
| 其它（NPC/商人/守卫/城堡结构） | 2,000 | 500 | 5,000 | `Envir/Npcs.txt` + 各图 Merchant/GuardList + 城堡门/墙；非会话驱动 |
| **合计** | **39,500** | **20,000** | **53,519**（内容上限口径） |

**关键假设**：`World::tick` 的成本是「每实体固定成本 × 实体数」（§3 三档线性、T10 §3 同结论），
且玩家/NPC/宝宝与怪物付同一个固定成本。前者的证据是 `--bots` 实验（T10 §8：10 个假玩家在 11k
档的差 −0.7%，噪声级）；但**玩家视野扫描与下行不是「每实体固定成本」**，见 §4.5。

### 4.2 `World::tick` 预算占用（200ms 节拍、5 tick/s）

| 场景 | 实体数 | 优化后 World ms | 占 200ms | 优化前 World ms（8.58µs/实体） | 占 200ms |
| --- | --- | --- | --- | --- | --- |
| 低 | 20,000 | 24.9 | 12.4% | 171.6 | 85.8% |
| **基准** | **39,500** | **49.2** | **24.6%** | **338.9** | **169.4%** |
| 内容上限 | 53,519 | 66.6 | 33.3% | 459.2 | 229.6% |
| 极端（上限 + 每玩家 0.5 宠 + 5k 其它） | 59,000 | 73.5 | 36.7% | 506.2 | 253.1% |

⇒ **优化前，10k 玩家在基准场景就已经超预算 1.7 倍**（World tick 单独就超）；
优化后单独给 World tick 有 3–4 倍余量（200ms / 49.2ms = 4.1×）。
`World::tick` 单独触顶的实体数是 **≈160,600**（200ms），是内容上限的 3.0 倍。

### 4.3 端到端预算（含宿主段；外推，低置信度）

宿主段（`sim_loop` 的 P21..P32 / AOI 下行 / 帧面 / 周期任务 / 存盘，**不在** `World::tick` 内）
按 T10b dev 真栈数据外推：

- T10b 实测（7,908 实体）：整 tick 1831ms，`WorldΣ` 1139ms（62.2%），`hostΣ` 692ms（37.8%）。
- 构建口径系数 25×（World 侧验证：dev 144.0µs/实体 vs release 5.82µs/实体 ≈ 24.7×）。
- ⇒ 宿主段 release **≈ 3.5µs/实体/tick**（692ms ÷ 7,908 ÷ 25 ≈ 3.50µs）。
- 端到端每实体 = World 1.245 + 宿主 ≈ 3.50 ≈ **4.745µs/实体**。

| 场景 | 实体数 | 端到端 ms | 占 200ms | 压到 50% 预算需要的改进倍数 |
| --- | --- | --- | --- | --- |
| 低 | 20,000 | 94.9 | 47.5% | 已达标（当前每实体 4.745µs，余量 1.05×） |
| **基准** | **39,500** | **187.4** | **93.7%** | **1.87×** |
| 内容上限 | 53,519 | 253.9 | 127.0% | 2.54× |
| 极端 | 59,000 | 280.0 | 140.0% | 2.80× |

**结论**：
1. 基准场景「刚好装得下」（94%），但没有给抖动/存盘/网络留余量；内容上限场景**超 27%**。
2. 主战场是宿主段：`hostΣ` 的 86% 是 `HostAoiDownlink`（T10b：598.8ms / 692ms @7.9k）。
   World 侧再优化 1.9× 只能把端到端从 4.70 压到 4.07（−13%），**不够**；必须动宿主段。
3. 优化前（8.58µs/实体 World + 3.5 host = 12.08µs/实体）端到端容量 ≈ **16,600 实体**
   （200ms）—— 连「10k 玩家 + 8k 怪」的低场景都装不下。本卡把端到端容量拉到 ≈ **42,200 实体**。

### 4.4 敏感性

对「怪物/玩家」这一个最不确定的参数做敏感（玩家 10k、宝宝 2.5k、其它 2k 固定）：

| 怪物/玩家 | 怪物数 | 实体数 | World ms | 端到端 ms | 占 200ms |
| --- | --- | --- | --- | --- | --- |
| 0.8 | 8,000 | 22,500 | 28.0 | 106.8 | 53.4% |
| 1.5 | 15,000 | 29,500 | 36.7 | 140.0 | 70.0% |
| **2.5（基准）** | **25,000** | **39,500** | **49.2** | **187.4** | **93.7%** |
| 3.0 | 30,000 | 41,500 | 51.7 | 196.9 | 98.5% |
| 3.9（= MonGen 内容上限） | 39,019 | 53,519 | 66.6 | 253.9 | 127.0% |

- 端到端对「怪物/玩家」的成本弹性 ≈ 4.745µs/实体，即**每增加 1,000 只怪 ≈ +4.7ms/tick（2.37%）**。
- 宝宝同理：每玩家多 0.25 只宠 ≈ +2,500 实体 ≈ +11.9ms 端到端（5.9%）。
- **结论**：只要每玩家活跃怪数 > ~2.5，端到端就吃掉 90% 以上预算；`World::tick` 则要到
  每玩家 ~14 只怪才独自触顶。⇒ 容量规划应以**端到端**为准。

### 4.5 本模型**没有**覆盖的项（不许硬摊）

1. **玩家视野扫描（`OutputAoi` 内的 `search_view_range`）**：玩家 `SearchTime = 1000ms`
   ⇒ 200ms tick 下每 6 tick 扫一次；10k 玩家 × (2×5+1)² = 121 格 / 6 tick ≈ **202k 格访问/tick**。
   本卡补了一次 `--bots` 边界测量（`final-bots1000.txt`：11k 怪 + 1,000 假玩家、铺满的稠密图）：

   | 指标 | 无玩家 | +1,000 玩家 | 差 |
   | --- | --- | --- | --- |
   | `World::tick` | 13.69ms | 20.54ms | +6.85ms |
   | `OutputAoi` | 0.43ms | 3.54ms | **+3.11ms** |
   | `SimMonsterAi` | ≈7.0ms | 9.73ms | +2.7ms |

   - **玩家视野扫描 ≈ 3.1µs/玩家/tick（稠密图上界）**；线性外推到 10k 玩家 ≈ **+31ms/tick**。
     真实地图的格子密度低于本 bench（每格都有怪），所以这是**上界**；量级仍足以吃掉 15% 预算。
     必须用真实地图 + 稀疏怪补一次专用基准。
   - `SimMonsterAi` 的 +2.7ms 不是玩家扫描，而是 **可见性效应**：玩家把附近怪标成
     `visible_active` ⇒ 这些怪**每 tick** 跑 Run（跳过 `ProcessMonsterInterval` 的 1/3 节流）。
     1,000 玩家在稠密图上只让 AI +39%；若 10k 玩家让大部分怪可见，AI 最坏可到线性模型的
     **≈3×**（每实体 0.635→≈1.6–1.9µs）。按基准 25k 怪算 = **+25…32ms/tick**，
     会把基准场景的 `World::tick` 从 49ms 推到 ≈75–81ms（37–41% 预算）。
     ⇒ **§4.2/§4.3 的线性模型对 AI 是下界**；真实值取决于「可见怪比例」，需要真栈/地图级基准。
2. **真实下行帧面**：T10b 量的是「帧生成 + 入队」，不含 gate 线程的 socket 写与背压；
   10k 玩家的下行量级与 1 bot 场景不可比。
3. **DB / 存盘**：异步、本机 <0.03ms/tick（T10b §12.6），但 10k 玩家的存档队列未测。
4. **构建口径**：以上端到端都是 release 外推；演示栈是 dev（差 25×），不能直接对照。
5. **GC/分配压力、缓存局部性**：实体数到 4–5 万时的内存带宽影响未测。

## 5. 优化项（逐项独立、可回退）

每项都是**行为等价**改动（不碰 RNG、不碰段序、不碰协议/落库）；回退方式 = 还原该项的
文件级 diff（每项单独提交）。

### 5.1 T10c-1：段访问器组合读视图（回归修复，收益最大）

- **改了什么**：`mir2-world-bevy` 的 `StageQueries` 增加「只读视图」缓存表 +
  `StageEntities::prepare_read_view` / `read_view`；`crates/world` 侧定义
  `EntityRefData`（与 `ReadViewQuery`/`ReadView2Query`/`ReadView3Query` 同一份 24 组件）
  与实现载体 `EntityRefView`；`StageAccess::get` / `spawn_cloned` 改为**一次 `get_manual` 取齐**；
  `StageAccess::new` 负责预建表。24 处构造点统一走 `StageAccess::new`。
- **为什么**：见 §2。
- **收益**（11k，**本提交单独**）：`World::tick` 94.34 → **17.19ms**（−82%；更早一次忙窗口
  21.60ms）；`SimMonsterAi` 52.63 → 7.32–8.68ms；
  七个 `EntityAccess` 全量扫描段各降 ~8×（0.46µs/实体 → 0.057µs/实体）。
- **为什么不直接用 `&mut self` 的 `view_ref`**：`EntityAccess::get` 的签名是 `&self`
  （`count_at_cell` 等辅助函数持 `&A` 共享借用），改成 `&mut` 会级联到 113 个 `_impl` 函数、
  评审面远大于收益。`QueryState::get_manual` 只需要 `&self`，因此用「只读视图缓存表」即可。
- **等价性**：组件集合、跳过语义（任一列缺失 ⇒ `None`）、fail-closed 白名单判据
  （`assert_view_access` 逐项核对，**在预建表时就 panic**）都逐条不变；顺序仍来自 `SlotTable`。
  负例牙（`t_neg_4/5/7` 等）未动，全绿。

### 5.2 T10c-2：AOI 直取槽位表 + 去 HashMap + 单次快照（T10 Top2 的 AOI 半）

- **改了什么**（`mir2-rs/crates/world/src/aoi.rs` + facade）：
  1. 删除 `AoiIds` 资源：`OutputAoi` 段把 `SlotTable` 资源**移出后借用**
     （段内无结构变更，段尾放回），按 `SlotTable::order_iter()`（新增，零分配）迭代；
     顺带删掉 facade 每 tick 的 `slot_order()` 克隆（= T10 Top4 的 AOI 半）。
  2. 删除每 tick 重建的 11k 项 `HashMap<i32, Entity>`：`entity_of` 走 `SlotTable` 的
     O(1) 稠密索引。
  3. 分桶只读 `Transform`（只取 `map_id`），不再为拿 `map_id` 构造完整 24 组件 `EntityRef`。
  4. 主循环把 death/race/ghost/`SearchTime` 门**一次快照取齐**（旧实现最多 3 次完整 `EntityRef`）；
     事件支的 `race == Play` 复用快照（段内 race/death 不变）。
  5. `AoiScratch` 增加两个跨 tick 复用的 `QueryState`（全组件读视图 + `Transform`），
     每 tick `update_archetypes` 一次。
- **收益**（11k）：`OutputAoi` 5.14 → 0.43ms（在 T10c-1 之后的基线上 **−92%**）；
  `World::tick` 21.60 → 13.95ms（−35%）。
- **等价性**：扫描顺序 = 槽位序（同一真值）；分桶/死亡/观察者分支的判定与顺序逐条不变；
  `aoi_order` / `hides` / `event_hides` 的推入点与次序不变。
- **`--bots` 边界**：10 假玩家在 11k 档无可测差异（T10 §8）——10 个玩家的扫描量太小。
  本卡补测了 1,000 玩家的边界（§4.5 第 1 条）：`OutputAoi` +3.11ms、AI +2.7ms（可见性效应）。

### 5.3 T10c-3：AI 中间表（T10 Top2 的 AI 半）

- **改了什么**：
  1. `StageAccess::order_ids`：从「`SlotTable::order()` 克隆 11k 项 `(id, Entity)` + 再
     `collect` 一个 `Vec<i32>`」改为「`order_index_iter` 一次收集」；
     `monster_ai_tick_impl` 的全量过滤从 `filter().collect()`（第二个等长 `Vec`）改为
     **原地 `retain`**。
  2. `SimState::process_run_counts`：`BTreeMap<i32,i32>` → `ProcessRunCounts`
     （稠密下标数组 + 范围外稀疏兜底；`0` = 无条目）。唯一读点是 AI 跳步门，
     每 tick 对**每个实体**一次 `get` + 一次 `set/remove`；11k 实体 = 22k 次 O(log n) 树操作
     （插入还分配节点）。
- **收益**（11k）：`SimMonsterAi` 7.50 → 6.98–7.11ms（约 −5…7%）；`World::tick`
  14.13 → 13.30ms（最终窗口 −6%）。**其中两处分配优化单独看落在噪声内**（±5%），可测的主要是
  `ProcessRunCounts`；两项都保留（无额外风险，且实体数越大越有利）。
- **等价性**：过滤集合与顺序不变（`retain` 保序）；`ProcessRunCounts` 只有 `get/set/remove`
  三个操作、从不遍历，语义与 `BTreeMap` 逐条相同（插入 0 = 删除这一条在调用点与容器内各兜一次）。

### 5.4 T10 Top4：`slot_order()` 分配（部分完成）

- AOI 侧的两处克隆**已随 T10c-2 消失**（facade 不再 `slot_order()`，改由段直接迭代槽位表）。
- 剩余 `slot_order.hiter_snapshot` 探针仍为 **0.016ms/tick（0.12%）**，且其返回值还要参与
  `SimHiterSnapshot` 的顺序契约；即使归零也只影响 200ms 预算的 0.008%。
  **不做**：收益/风险比不划算，按 T10 §6 Top4 的结论列入观察项。

## 6. 未做项与理由

### 6.1 T10 Top3（四个全量扫描段合并成一次）

**不做**（本卡）。理由：① 涉及时序/段边界（`TickStage` 的 `STAGE_ORDER` 是契约），
T10 判级建议按 **R**；② 与 T10c-1 收益重叠——Top3 的动机是「每实体取 24 组件的适配器成本」，
T10c-1 已把每次取组件从「24 次解析」降到「1 次解析」，四段合计从 20.34ms 降到 2.54ms，
剩余收益上限只有 ~2.5ms/tick（11k，18% 的当前 tick），而风险是段序/搬移语义。
建议另开 R 卡，先设计件后实施。

### 6.2 T10 Top5（buff 多 pass）

**不做**（本卡）。理由：① 本场景无 buff（`SimBuff` 0.006ms/tick，0.05%），先在新口径下
补「大量带 buff 的 11k」真栈测量才有意义；② 任何改 pass 时序/到点门的动作都碰 parity 面
（buff 到期/sysmsg/重算），按 R 处理。不是容量瓶颈项（成本 ∝ 有 buff 的 actor 数，不 ∝ 世界实体数）。

### 6.3 玩家视野扫描 / `HostAoiDownlink` 细分

- **玩家视野扫描**：本卡用 `--bots 1000` 给出了**稠密图上的上界**（§4.5 第 1 条：`OutputAoi`
  +3.11ms / 1,000 玩家）。它仍不是真实地图密度下的结论，也不含「玩家移动触发的可见集抖动」。
- **`HostAoiDownlink` 细分**：本卡**没有**数据（T10b 只给了整段 567ms@7.9k dev）。
- 两项都需要真栈/专用 bench，见 §8。

## 7. 门禁与证据（`card/capacity-model`，交付 head 见报告）

| 门 | 命令（`mir2-rs/` 下） | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | RC=0 |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | RC=0 |
| test | `cargo test --workspace --no-fail-fast` | RC=0：**3883 passed / 0 failed / 69 ignored**（0 条 FAILED） |
| msgcodegen | `cargo run -p msgcodegen -- --check` | RC=0（604 条常量） |
| 散文锚 | `python -I -B tools/hygiene/docs_anchor_gate.py --prose --base walgit/master` | GREEN（检查 0 / 悬空 0 / 越界 0；本文不用 `path:line` 锚） |
| golden | `cargo test -p mir2-parity-tests --test world_aoi_parity` | **6/6**（3 条 golden 对拍 + 3 条 redcheck 牙） |
| parity | `cargo test -p mir2-parity-tests` | 0 failed |

原始测量输出（未入库）：`E:/UserTemp/w83c-prof/baseline.txt`（`80f97231e` 基线）、
`step1-aifast.txt`、`step2-aoi.txt`、`step3-ai-alloc.txt`、`final.txt`、`final2.txt`。

## 8. 残余风险与后续卡（建议）

1. **（P1）宿主段 `HostAoiDownlink` 细分与优化**：T10b 已确认它是宿主段的最大单项
   （31% tick @7.9k dev）。建议先加 4 个名字探针（AOI 出现帧 / 52 / 53 / 体力的四个子段），
   再按数据优化。这是 10k 容量的**真正瓶颈**。
2. **（P1）玩家视野扫描基准**：补一个「大量玩家 + 稀疏/稠密怪物」的 bench 场景，
   量 202k 格/tick 的真实成本（本卡只有稠密图上界：1,000 玩家 +3.11ms）；现在的模型对稀疏
   真实地图仍是空白。
3. **（P2）AI Run 链访问形态**：`SimMonsterAi` 仍是 World tick 的 53%；若要再降，
   需要减少每只怪每 tick 的 `EntityRef` 构造次数（例如把 `race`/`death` 等派发字段一次性
   取齐并向下传），属另一张卡。
4. **（P2）`SimHiterSnapshot` 的 `slot_order` 残余**：0.12%，观察项。
5. **模型本身的置信度**：§4.3 的宿主段是 25× 外推，**必须**用更新后的 release 真栈
   （T10b 的 `MIR2_TICK_PROFILE=1` 口径）复测一次；在此之前，10k 容量结论以「World 侧充足、
   端到端吃紧」这一定性结论为准，不要引用 94% / 127% 这两个外推数字做验收。

## 9. 残留物与复现

- **代码**：3 个独立提交（`段访问器组合读视图` / `AOI 直取槽位表` / `AI 中间表`），
  每个都能单独构建（已逐个 `cargo check -p mir2-world` 验证）与单独回退。
- **未入库的测量物**（`E:/UserTemp/w83c-prof/`）：`baseline.txt`（基线的 --profile 原始输出）、
  `step1-aifast.txt` / `step2-aoi.txt` / `step3-ai-alloc.txt`（每步的 --profile 原始输出）、
  `final*.txt`（最终构建）、`clippy-final.log` / `test-final.log`（五门原始输出）、
  `c1/c2/c3.patch`（提交拆分补丁）、`msgs/`（提交信息）。保留原因：评审复算与证据四件套。
- **构建目录**：`E:/UserTemp/w83c-target`（本车道独立 CARGO_TARGET_DIR）。
- **未起任何服务/客户端/假人进程**；未改 oracle/客户端/协议；未启用并行 executor；
  未在 `E:/MirServer` 写任何东西。
