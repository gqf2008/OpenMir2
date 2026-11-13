# T10：每实体 tick 热点剖析（2026-11-13）

卡：`openmir2-t10-tick-hotspot-profile`（L 级，测量 + 报告，不改生产行为）。
工具：`mir2-rs/crates/world/src/bin/tick_bench.rs` 的 `--profile` / `--bots` 模式；
分段计时：`mir2-rs/crates/world/src/world.rs` 的 `TickProfile` / `World::enable_tick_profile`。

> **口径（最显眼处，先读这一段）**
>
> 本文所有绝对值都是 **`World::tick`（= `tick_head` + `tick_tail`）** 的墙钟，
> **不含**生产宿主在两者之间跑的 P21..P32 段、帧生成/会话输出、以及 DB/网络 I/O
> （`bins/game-svr/src/lib.rs` 的 `sim_loop`）。演示栈实测的「tick ≈ 2000ms」是
> **sim 循环的端到端口径**，与本文的 61–64ms **不可直接相减**；差集见 §8「未解释部分」。
> 另：测量窗口内机器上还有别的车道的 `rustc/cargo`（20:07 起，CPU 88–100%），
> 绝对值按偏慢/偏快两侧都有噪声理解；**结论以相对占比与每实体成本为主**。

## 1. 测量环境声明

- 机器：Windows 10，release 构建（`cargo build --release -p mir2-world --bin tick_bench`）。
- 演示栈：本线未起、未停任何栈；测量期间**没有** game-svr/DB 由本线拉起。窗口内观察到
  两个外部 `cargo`（20:07:39 起）与一个 `rustc`（20:08:06 起）常驻，CPU 探针 88–100%
  （`tick_bench` 自带的负载探针连续报 `CPU 100% / 重型进程 4 个`）。
- 因为机器不空闲，本文**不把绝对 ms 当验收值**；所有横向结论都来自：
  ① 同一进程、同种子、同 tick 数的 A/B（§7）；② 每实体成本（µs/实体/tick）与三档缩放（§4/§5）。

## 2. 工具用法（可复现）

```sh
cd mir2-rs
cargo build --release -p mir2-world --bin tick_bench
./target/release/tick_bench --profile 1000 100
./target/release/tick_bench --profile 5000 100
./target/release/tick_bench --profile 11000 100
./target/release/tick_bench --profile --bots 10 11000 100
```

- `--profile`：开启分段计时 + 生产接线（`enable_monster_ai` / `enable_buff_tick` /
  `enable_npc_tick` / `enable_merchant_tick`），无玩家移动命令（移动负载见 §8 对照）。
- `--bots N`：追加 N 个假玩家（`race=0`，均布；无会话、无出站帧）。
- 每档**跑两遍**：A 遍计时未启用、B 遍计时启用；两遍用同一构造/同种子/同 tick 数，
  计时不写实体、不抽 RNG ⇒ 两遍做逐字节相同的工作，墙钟比值即计时开销（§7）。
- 计时**缺省关闭**：不调 `enable_tick_profile` 时 `tick_profile` 为 `None`，
  段边界只做一次 `Option` 判别（每 tick **约 45–55 次**：20 段 × `begin`+`end` = 40，
  另加 4 类命名探针 6–14 次（`buff.pass` 4 个 pass；探针随 AI/buff 开关变化），
  不是个位数），不取 `Instant`、不分配、不进入热循环。

场景说明（与 `tick_bench` 默认门禁场景的差别）：

| 项 | 默认门禁 `run_tier` | 本文 `--profile` |
| --- | --- | --- |
| 实体 race | 0（不进入 AI） | 80（进入 `monster_ai_tick`） |
| AI/buff/npc/merchant | 全关 | 全开（= 生产宿主接线） |
| 每 tick 移动命令 | 1/4 实体 `walk` | 无（AI 自行游走） |
| 铺位 | 100 列、`y=1+2*(i/100)` | 199 列逐行铺满（11k=56 行，全在图内） |

> 为什么换铺位：默认公式在 `i > 9900` 时 `y` 超过 200 行（出图），会让 11k 档的每实体成本被
> 「图外实体」污染。默认门禁场景未改（`run_tier` 原样），基线可比性不受影响。

## 3. 三档总表（`World::tick` 口径）

| 实体数 | tick 数 | ms/tick | µs/实体/tick | 相对 1k |
| --- | --- | --- | --- | --- |
| 1,000 | 100 | 5.57 | 5.57 | 1.0× |
| 5,000 | 100 | 28.58 | 5.72 | 5.1× |
| 11,000 | 100 | 63.97 | 5.82 | 11.5× |
| 11,000（+10 假玩家） | 100 | 63.51 | 5.77 | 11.4× |

**缩放关系：严格线性**（1k→5k 实体 ×5、耗时 ×5.13；5k→11k 实体 ×2.2、耗时 ×2.24；
每实体成本 5.57→5.82µs，跨三档漂移 <5%）。⇒ 当前 tick 的成本结构是
**「每实体固定成本 × 实体数」**，没有明显的超线性项（没有 O(N²) 命中）。

## 4. 11k 分段明细（ms/tick，占比 = 段合计的百分比）

| 段 | ms/tick | 占比 | µs/实体 | 说明 |
| --- | --- | --- | --- | --- |
| `SimDup` | 10.02 | 15.7% | 0.91 | `check_dup_obj`（重叠推挤邻居计数） |
| `SimSlaveCleanup` | 9.74 | 15.2% | 0.89 | 宝宝清表（全量扫） |
| `TailPlayerDeaths` | 9.73 | 15.2% | 0.88 | 玩家死亡结算（全量扫） |
| `TailVerifyMapTime` | 9.71 | 15.2% | 0.88 | `VerifyMapTime` + 钳制（全量扫） |
| `SimSettleDeaths` | 9.69 | 15.1% | 0.88 | 非玩家死亡结算（全量扫） |
| `SimMonsterAi` | 6.88 | 10.8% | 0.63 | `monster_ai_tick`（含 0.47ms 全量 collect） |
| `OutputAoi` | 4.67 | 7.3% | 0.42 | AOI 快照（含 11k 项 HashMap + ids 克隆） |
| `SimExpireHitters` | 0.85 | 1.3% | 0.077 | |
| `TailSuperManRefill` | 0.80 | 1.2% | 0.072 | |
| `SimExtraAbil` | 0.46 | 0.7% | 0.042 | |
| `SimNpc` | 0.43 | 0.7% | 0.039 | 实体里无 NPC ⇒ 只付全量扫描 |
| `SimPlayerRunExpiry` | 0.42 | 0.7% | 0.038 | |
| `SimMerchant` | 0.41 | 0.6% | 0.037 | |
| `SimHiterSnapshot` | 0.10 | 0.2% | 0.009 | |
| 其余（Clock/Command/MapEvents/Buff/SelfDeliver/OutputDrain） | 0.02 | 0.03% | ~0 | |
| **段合计** | **63.92** | 100% | 5.81 | |
| 未计入（记录构造/段边界搬移） | 0.05 | — | — | 可忽略 |

**最显眼的一条：五个全量扫描段（`SimDup` + 4 个死亡/清表/时间校验段）合计 48.9ms = 76.5%
的 tick**，且每段每实体都落在同一个数量级（0.88–0.91µs）。它们共享同一个成本来源（§6 Top1）。

## 5. 总监点名的 5 项（11k，以及缩放）

| # | 候选 | 1k ms/tick | 5k ms/tick | 11k ms/tick | 11k 占比 | 缩放 | 每实体 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `entities.slot_order()` 全量 `Vec` 分配（tick 内 2 处） | 0.003 | 0.019 | 0.040 | 0.06% | 线性 | 3.6ns |
| 2 | `monster_ai_tick` 全量扫描（含 collect） | 0.57 | 3.07 | 6.88 | 10.8% | 线性 | 0.63µs |
| 3 | AOI 全量刷新（`OutputAoi`） | 0.36 | 1.92 | 4.67 | 7.3% | 线性 | 0.42µs |
| 4 | buff 多 pass（4 × 50ms） | 0.0004 | 0.0011 | 0.0013 | 0.002% | **与实体数无关** | ~0 |
| 5 | `check_dup_obj` 邻居计数（`SimDup`） | 0.89 | 4.55 | 10.02 | 15.7% | 线性 | 0.91µs |
| | **5 项合计** | 1.83 | 9.56 | 21.61 | **33.8%** | | 1.97µs |

关于第 1 项：`slot_order()` 探针（`slot_order.hiter_snapshot` + `slot_order.aoi`）合计
**0.040ms/tick = 0.06%**，每实体 3.6ns（两个探针：`slot_order.hiter_snapshot` 1.915ms +
`slot_order.aoi` 2.126ms，除以 100 tick）。**它不构成热点**——11k 项 `(i32, Entity)` 表的分配/释放
在这个量级下可忽略。真正贵的全量分配在 AOI 段内部（`AoiIds` 克隆 + 11k 项 `HashMap`），见 Top2。

关于第 4 项：本场景没有 buff（`BuffSystem.actors` 为空，4 个 pass 空转 1.3µs/tick）。
`buff_do_work` 只遍历有 buff 的 `actors` 列表，**不随世界实体数增长**；它的真实成本必须带 buff
在真栈上量（本 harness 覆盖不到「有 buff 的怪物」，见 §8）。

## 6. 优化清单

> 判级：本卡不改任何生产行为，故下列均为**建议**。判级按 `docs/AGENTS.project.md`「判据分级」：
> 触及 RNG 抽数序列 / 时序 / 落库 / 战斗数值 ⇒ R；纯访问层、纯数据结构、文档与工具 ⇒ L。
> 「是否影响 golden/parity」= 是否可能改变对拍输出（状态/帧/RNG 序），与「是否需要窗口」分开看。

### Top1（根因）：段适配器的运行期白名单镜像 —— 每组件访问的固定开销

- **观测**：5 个全量扫描段各 0.88–0.91µs/实体，且**几乎相等**；`SimHiterSnapshot`
  （同样全量扫、但每实体只取少量组件）只有 0.009µs/实体 ⇒ 成本随「每实体取组件次数」线性增长。
- **根因**（代码已核）：`crates/world-bevy/src/tick.rs` 的 `StageEntities::get` / `get_mut`：
  每次取组件 = `resolve(id)`（1 次 `SlotTable` 资源查找）+ `assert_stage_component::<C>`
  （1 次 `ComponentRegistry` 资源查找 + 1 次 `TypeId` HashMap 查 + `S::reads(id)` 的
  **最多 ~25 条 TypeId 短链比较**）+ `world.get::<C>(entity)`（archetype 查）。
  一个实体在扫描段里取 20+ 个组件 ⇒ ~20 × (2 资源查 + 哈希 + 25 比较) ≈ 0.9µs。
- **改动面**：`StageEntities::new` 时按本段 `reads()/writes()` 预编译成 `HashSet<TypeId>`
  （或每类型一个 bool 缓存 / 位图），把逐次链比较降为 1 次哈希；更激进的是「每实体一次
  `view_ref`（组合视图）取齐全部组件」——白名单核对从「每组件一次」变「每实体一次」。
- **预期收益**：5 个扫描段合计 48.9ms（76.5%），按「核对开销占大头」估可省 **30–60% ⇒
  15–29ms/tick（11k）**，即每实体 1.3–2.6µs。**估算依据**：每实体每段 20+ 次核对 × 约 40ns
  ≈ 0.85µs（与实测 0.88–0.91 同量级）；换成 1 次哈希 ≈ 10ns，20 次共 0.2µs ⇒ 每段省 ~0.65µs。
- **风险**：中。必须保住 fail-closed 语义（越白名单仍要 panic）与既有负例牙；
  预编译表必须在 `ComponentRegistry` 就绪后构建、泛型实例按 `TypeId` 各自成项。
- **是否需要 R 级窗口**：**否（L）**。纯访问层，不碰 RNG/时序/落库/数值。
- **是否影响 golden/parity**：**否**（同一组件同一值，只改查表方式；顺序仍由槽位表给出）。
- **归属**：已另开卡 `openmir2-ecs-stage-whitelist-precompile`（worker-84），本卡只提供改前基线。

### Top2：`monster_ai_tick` / AOI 的 O(N) 中间表 + AOI 的 HashMap 索引

- **观测**：AI 6.88ms（10.8%），其中 `monai.collect` 0.47ms；AOI 4.67ms（7.3%）。
- **根因**：
  - `monster_ai_tick` 每 tick 两次全量物化：`entities.iter_ref()` 先 `order_entities()` 分配
    `Vec<Entity>`，再 `filter/map(...).collect::<Vec<i32>>()`（第二次分配）。0.47ms 是这两次
    分配 + 全组件查询。
  - `aoi::output_aoi_system` 每 tick：`AoiIds.0.clone()`（11k × 8B）+ 建
    `HashMap<i32, Entity>`（11k 项哈希）+ `by_map` 分桶；随后每实体两次 `store.get`。
- **改动面**：
  - AI：`SlotTable` 本身就是槽位顺序的唯一真值（资源里已有），直接借出 id 迭代器/切片，
    不再 `collect`；过滤条件在遍历中判断。
  - AOI：用槽位数组（`id - 1` 或 `SlotTable` 的 `entity_of`）替代 `HashMap`；
    `AoiIds` 用 `std::mem::take` 借走（facade 已 `mem::take` 过 `maps`，同口径），消除克隆。
- **预期收益**：AI 省 ~0.4–0.5ms；AOI 省 ~1–2ms（HashMap 建表 + 克隆在 4.67ms 内占相当比重）。
  合计 **约 1.5–2.5ms/tick（11k）**，随实体数线性放大。
- **风险**：低。顺序仍来自槽位表；只换容器。
- **是否需要 R 级窗口**：**否（L）**。
- **是否影响 golden/parity**：**否**（AI 的过滤/处理顺序不变；AOI 仍按槽位顺序分桶）。

### Top3：把「四次全量扫描」合并成一次

- **观测**：`SimSettleDeaths` / `TailPlayerDeaths` / `TailVerifyMapTime` / `SimSlaveCleanup`
  各 9.7ms，四段合计 38.9ms（60.8%），每段都在做「全量扫 + 少量条件判断」。
- **改动面**：在**保持 oracle 段序**的前提下，把四段所需的组件一次性取齐（同一次 `view_ref`
  或一次遍历），各段只做各自分支；段边界与 RNG 消费者次序不变（本卡**不**改行为，实施另开卡）。
- **预期收益**：四个「每实体 ~0.88µs」的适配器成本可摊薄到一次遍历，粗估省 **20–30ms/tick**
  （与 Top1 有重叠，二者**不可相加**：Top1 降低每次取组件成本，Top3 减少取组件次数）。
- **风险**：中高。段序是契约（`TickStage` 的 `STAGE_ORDER`），合并会动到段边界与
  `SimState` 搬移语义；需要单独设计件与完整对拍。
- **是否需要 R 级窗口**：**建议按 R 处理**（涉及时序/段边界，且判据复杂；合并后若引入
  跨段状态可见性变化，只有真机全量对拍能证伪）。
- **是否影响 golden/parity**：**设计上不影响**（同一顺序、同一组件值），但必须以对拍证据为准。

### Top4：`slot_order()` 分配（点名项 1）—— **不建议现在做**

- **改动面**：`tick_head`（`SimHiterSnapshot`）与 `tick_tail`（`OutputAoi`）两处
  `entities.slot_order()`；可改为复用同一份槽位快照，或直接借 `SlotTable` 的只读切片。
- **预期收益**：≤0.040ms/tick（11k，0.06%）——这是实测上限，即使归零也只影响 200ms 预算的 0.02%。
- **风险**：低（只换取值方式，顺序真值仍是 `SlotTable`）。
- **是否需要 R 级窗口**：否（L）。
- **是否影响 golden/parity**：否（处理顺序不变）。
- 结论：收益/风险比不划算，列入「观察项」；若将来 tick 内调用点增多再评估。

### Top5：buff 多 pass（点名项 4）—— **本场景无成本，需真栈复核**

- **改动面**：`World::buff_tick` 的 4 个 50ms pass / `BuffSystem::do_work`（`crates/world/src/buff.rs`）。
- **预期收益**：本场景 ≈0（1.3µs/tick，0.002%）；有 buff 时与 `BuffSystem.actors` 数量成正比，
  **不随世界实体数增长**，所以它不是「每实体固定成本」项。
- **风险**：低（若只优化遍历）；但任何改变 pass 时序/到点门的改动都会碰对拍面。
- **是否需要 R 级窗口**：本轮不改 ⇒ 不适用；若实施且触及 pass 时序/到点门，按 **R**。
- **是否影响 golden/parity**：**是**（buff 的到期/sysmsg/重算是 parity 面）；当前不建议动。
- 建议：在真栈上用同一套 `--profile` 口径补一次「大量带 buff 的 11k」测量，先取数再决定。

## 7. 「关闭即无额外开销」的可证伪判据

判据设计：同进程、同构造、同种子、同 tick 数，跑两遍（A 计时未启用 / B 计时启用）。
计时语句只取 `Instant` 与累计，不写实体、不抽 RNG ⇒ 两遍做**逐字节相同**的工作，
墙钟差 = 计时开销。另加单测 `tick_profile_is_opt_in_and_behavior_neutral`：
缺省必须 `None`；开/关两遍的 `TickRecord` 必须逐字段相等；开启后 `ticks`/`stages()` 非空；
暂停期间不累计。**变异**：删掉「缺省 None」或往计时里插入任何写实体/RNG ⇒ 该用例红。

同进程两遍 A/B（n=100/100，ΔP50 正 = 计时态更慢）：

| 档 | A 未启用 P50 (µs) | B 启用 P50 (µs) | ΔP50 | ΔP99 |
| --- | --- | --- | --- | --- |
| 1k | 5,157 | 5,131 | **−0.50%** | −4.58% |
| 5k | 26,398 | 26,226 | **−0.65%** | −7.08% |
| 11k | 58,872 | 60,184 | **+2.23%** | −1.69% |
| 11k+10bot | 59,875 | 58,891 | **−1.64%** | −5.81% |

⇒ 四档符号不一致、幅度 ≤2.23%，落在同机 run-to-run 噪声内：**计时开启态本身的开销
已经在噪声以下**；关闭态（缺省，连 `Instant` 都不取）只会更小。

**master 基线门禁的尝试（失败，如实记录）**：按
`./target/release/tick_bench --baseline tests/parity/world/p99-baseline-tickbench.txt 500 2000 1000 2000 2000 1000`
跑默认路径，工具自带的负载探针报 `CPU 100% / 重型进程 4 个`（窗口内另有车道的
`cargo`×2 + `rustc`×1 常驻），三档 P50 全部相对基线劣化 700–820%、被判 `RED`。
**该结果不能归因于本卡改动**：同窗口的机器被外部编译占满，任何内存/缓存敏感的基准都会
劣化这个量级；等机器空载后需要重跑一次作为独立判据。在此之前，「无额外开销」以上面的
同进程 A/B + 单测为准。

## 8. 未解释部分（不许硬拆）

演示栈口径的 2000ms/tick 与本文 `World::tick` 的 64ms 之间差 **约 32×**，本 harness **解释不了**。
已经排除 / 已量化的部分：

1. **不是移动**：协调者给的纯移动基线 11k = P50 88.4ms / P99 178.8ms（默认门禁场景）；
   本文 `--profile` 全负载 64ms 是另一种负载，不可直接比较，但两者同量级。
2. **10 假玩家几乎不改变 `World::tick`**：63.51 vs 63.97ms（−0.7%，噪声级）。机制（已核代码）：
   ① 玩家观察者的视野扫描有到点门，`PLAYER_SEARCH_TIME = 1000` ⇒ 每 1000ms 才扫一次
   （200ms tick 下约每 5 tick 一次），不是每 tick；
   ② 即便扫到，`RM_UPDATEVIEWRANGE` 的投递有跨度门
   `|Δx| <= viewer.view_range - other.view_range`（同 `|Δy|`，
   `crates/world/src/aoi.rs` 的 `search_view_range` 内）；双方 `ViewRange` 缺省都是 5
   ⇒ **span = 0，只有同格怪才可能被投递**；再叠加怪物自己走动，实际被标 `visible_active`
   的是个位数；
   ③ **race 80 不扫视野**——`monster_self_scans` 只认真宝宝与 race 11/112/254，所以不存在
   「怪物每 tick 做 99 格视野扫描」这回事（本报告前一版的错误表述，已删）。
   ⇒ 该场景对 tick 无可测影响，且 bench 里**打不开** visible-active 路径到演示舰队的量级；
   要定量必须真栈，或补完整的会话/AOI 夹具（`WantRefMsg`/ViewRange 差异/移动 bot）。
3. **宿主段（P21..P32 + 帧生成/会话输出 + DB/网络）完全没有覆盖**：它在
   `bins/game-svr/src/lib.rs` 的 `sim_loop` 里、`tick_head` 与 `tick_tail` 之间，
   不属于 `crates/world`，也不在本卡写集内。演示栈 10 个 bot × 每 tick 的帧生成/序列化/发送、
   会话与 DB 持久化，都可能是那 32× 的主体。
4. **内容形态差异**：真栈怪物 race/属性/地图密度/可见性都不同；本文是单图 11k、race=80 的同构体，
   AI 每 3 tick 才跑一次（`ProcessMonsterInterval=2` 的生产缺省）。

⇒ 结论：**「5 项占比」只在 `World::tick` 口径内成立（33.8%）；32× 差距的主要来源在宿主段与
会话耦合路径，必须在真栈上测**（建议下一张卡：给 `sim_loop` 加同一套段计时并录 10 bot 演示窗口）。
本卡不把未解释差额摊派给任何一项。

## 9. 实体预算反推

两个口径都要给，否则会被误读：

1. **仅 `World::tick`（本文口径）**：每实体 5.8µs ⇒ 200ms 预算 /
   5.8µs ≈ **34,500 实体**。与 MonGen 内容上限 **39,019** 对照 ⇒ **当前每实体成本下，
   `World::tick` 单独在内容上限处约 226ms，已经超 200ms 预算约 13%**。
   若 Top1 + Top3 落地把每实体从 5.8µs 压到 ~2.5µs，则 39k 实体 ≈ 98ms，留有 ~2× 余量。
2. **端到端（演示栈实测 0.18ms/实体）**：200ms / 0.18ms ≈ **1,100 实体**；要承载 39k
   内容上限，需要把端到端每实体成本压到 **≤5.1µs**（约 **36×** 改进）。
   这个目标**不可能**只靠 `crates/world` 的访问层优化达成，必须同时削减宿主段与帧面成本。

## 10. 残留物 / 复现材料

- 原始输出（未入库，保留给 Top1 卡的「改前基线」对照）：
  `E:/tmp/t10-worker-85/prof-1k.txt`、`prof-5k.txt`、`prof-11k.txt`、`prof-11k-bots10.txt`。
- 补丁脚本（一次性）：`E:/tmp/t10-worker-85/patch_world.py`、`patch_more.py`。
- 本线未起任何服务/进程；未改 Envir/MonGen/golden/parity；未启用并行。
- 早先一版「逐 tick 交替 A/B」的测量作废：它把两态落在不同世界状态上，给出过 +20% 的假差异；
  本文数据全部来自「同种子两遍」设计。作废数据未采用。

## 11. 门禁与复现证据（本分支 `card/t10-hotspot`）

| 门 | 命令（`mir2-rs/` 下） | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | RC=0 |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | RC=0（首轮 4 条：3× `unnecessary_sort_by` + 1× `manual_checked_ops`，已修） |
| test | `cargo test --workspace --no-fail-fast` | RC=0（无 FAILED） |
| msgcodegen | `cargo run -p msgcodegen -- --check` | RC=0（604 条常量一致） |
| 散文锚 | `python -I -B tools/hygiene/docs_anchor_gate.py --prose --base master` | GREEN；**检查 0**（本文只用反引号文件/符号，未新增 `path:line` 锚——高 churn 目录禁行号锚，属预期） |

原始输出：`E:/tmp/t10-worker-85/clippy2.log`、`test.log`、`msgcodegen.log`。
测量原始输出：`prof-1k.txt` / `prof-5k.txt` / `prof-11k.txt` / `prof-11k-bots10.txt`（同目录）。
