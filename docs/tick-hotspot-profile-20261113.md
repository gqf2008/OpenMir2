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
3. **宿主段（P21..P32 + 帧生成/会话输出 + DB/网络）在本卡没有覆盖**（**T10b 已补测，见 §12**）：它在
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

## 12. T10b：宿主段计时（`sim_loop`，真栈）

> 卡 `openmir2-t10b-simloop-timing`（L 级，诊断计时、默认关闭）。本节是 §8「未解释部分」
> 第 3 条的补测：把**宿主段**按与 §2 同一口径计时，与 World 探针拼成一张完整 tick 分解表。

### 12.1 工具与门控

- 代码：`bins/game-svr/src/tick_profile.rs`（计时器 + 汇总打印 + 单测），接线在
  `bins/game-svr/src/lib.rs` 的 `sim_loop`（**17 个段边界**）。
- 门控：环境变量 `MIR2_TICK_PROFILE`，**只有字面 `1`** 开启（`0`/`true`/空/缺省一律关，
  与起栈器的 fail-closed 口径同形）。开启时同时调 `world.enable_tick_profile()`，
  使 World 的 `TickStage` 20 段与宿主段同一个窗口对齐。
- 汇总窗口：**50 tick 或 10s 墙钟，先到者**。200ms 节拍下两者等价；tick 被拖慢
  （本卡实测 600ms/tick）时按墙钟臂切窗，60s 观测内仍有 6 行以上。
- 输出：`glog!("DBG", "[TICKPROF] …")`，一行一个段（`ms/tick`、占整 tick %、每 tick 次数），
  末几行是「工作段合计」与「对账：`HostWorldHead+HostWorldTail` vs `ΣWorld.*`」。
  **只读观测**：不写实体、不抽 RNG、不发帧。

### 12.2 段名表（与 `TickStage` 同一命名口径）

宿主段用 `Host` 前缀、PascalCase，与 World 的 `TickStage` 变体名（`SimDup` / `OutputAoi` /
`TailVerifyMapTime` …）**不重名**；World 段在输出里加 `World.` 前缀（名字取 `TickStage` 的
`Debug`，与 `tick_bench --profile` 逐字一致）⇒ 两套探针可拼成一张表。

| 宿主段 | 测量范围（`sim_loop` 内的代码块） |
| --- | --- |
| `HostShutdown` | 关服通道探测（`shutdown_rx.try_recv()`） |
| `HostInbound` | `while ev_rx.try_recv()`：门事件与 `CM_*` 命令派发（会话/脚本/交易/包裹…） |
| `HostDbPoll` | DBSrv 读档/存档应答轮询、踢人/小退落地、GM pending 收口 |
| `HostPeriodic` | `SystemProcessor` 的 10s 块（serverinfo / castle / guild / gates）、离世广播、MonGen regen、GameTime 与滚动公告 |
| `HostWorldHead` | `tick_world_head_with_specials`（`World::tick_head` + 专用怪产仔/ref 排水） |
| `HostPostHead` | 642/monster ref 排水、`GetStartPoint`、城堡 252 扇出 |
| `HostPlayerRun` | P21 / P22 / P25-P27 / P31 / P32 宿主段 + `flush_run_frames` |
| `HostWorldTail` | `World::tick_tail`（`Tail*` 三段 + `OutputAoi`） |
| `HostPlanes` | `planes.tick()`（G11）+ `SuperMan` WAbil 投影同步 |
| `HostRecovery` | P1/P2 恢复、ExtraAbil 到期、healthspell flush |
| `HostFrames` | 本 tick 帧面排水（动作/屠宰/骨骼/buff/53/TransMap/自投递 B2-B5/组队 `Run`） |
| `HostDealTick` | 交易自动取消（`DealCancelA`） |
| `HostAoiDownlink` | `drain_run_downlinks_with_magic`：AOI 出现帧 + 52/53/体力下行 |
| `HostPersist` | `use_items` 回写 + 周期存档（`DB_SAVEHUMANRCD`） |
| `HostSwitch` | 换服轮询 + `SM_RECONNECT` 排水 |
| `HostDiag` | 每 50 tick 的 `sim: tick` 诊断行 |
| `HostSleep` | 末尾节拍 `sleep`（凑 200ms；过载时为 0） |
| `HostTickTotal` / `HostUnaccounted` | 整 tick 墙钟 / 整 tick 减各段之和的残差 |

World 段（`World.Clock` … `World.OutputAoi`）就是 §4 那 20 个 `TickStage`，名字与编号不变。

### 12.3 关闭时零开销（A1）

缺省（环境变量未设或 ≠ `1`）时 `enabled == false`：

- `begin()`：读一次 `bool` 后返回 `None` —— **不取 `Instant`**；
- `end()` / `end_tick()`：一次 `Option` 判别即 `return` —— **不写状态、不分配、不查表**；
- `due()`：`enabled` 为假时短路，恒 `false`。

每 tick 判别次数 = 段数 × 2 + 2 = **17 × 2 + 2 = 36 次 `Option`/`bool` 判别**，与 World 探针
同口径（§2：45–55 次 `Option::is_some`）。开启态每段另有 2 次 `Instant::now()`
（≈20–30ns/次，34 次 ≈ 1µs/tick，相对 200ms 节拍是 5e-6）。

牙（`tick_profile.rs` 单测，5 条全绿）：`disabled_profile_does_no_work`（关闭态跑 100 轮
`end(None)`/`end_tick(None)` 后 `rows` 必须为空 —— 删掉 `None` 短路或把缺省改 `true` 即红）、
`env_gate_accepts_only_literal_one`（`true`/`0`/空一律关）、
`enabled_profile_accumulates_and_takes_window`、`due_has_a_wall_clock_arm`、
`world_delta_is_incremental`（首窗全量、次窗增量、计数回退饱和为 0）。

### 12.4 真栈测量（1 bot，≥60s）

环境与 `E:/UserTemp/fourseam-logs/start.ps1` **逐项同形**（同 env：`MIR2_LOG_*`、
`MIR2_TEST_PERMISSION=10`、`MIR2_RNG_SEED=42`；同配置路径），只改 bin 目录
（本车道 `E:/UserTemp/w85b-target/debug`）与日志目录，并加 `MIR2_TICK_PROFILE=1`。
`game-svr.exe` sha256 = `53a1909fb097f852043dbc31ae6988956623cc833f781601ff221f4867216aa8`
（**dev profile，unoptimized** —— 与演示栈同形，不是 release）。

- 1 bot：`t10bc0`（`-DisableAutoPlay`，空闲挂机），**在世界 21:26:48 → 21:28:48（120s ≥ 60s）**；
  bot 在 21:28:48 按 `SoftCloseAfterWorldMs=120000` 小退，21:29:19 世界会话归零。
- 注意：世界实体数**不受 bot 控制**——MonGen 持续刷怪：3 分钟 270 → ~3,000，36 分钟自然涨到
  MonGen 上限 ~7,900 并趋稳（6 个窗口 1,779–1,871ms/tick）。所以下表按**实体数分档**给，
  而不是按「bot 在不在」。

窗口表（每行一个 10s 汇总窗口；`WorldΣ` = World 探针 20 段之和；`hostΣ` = 工作段合计 − `WorldΣ`）：

| 实体内 | tick / 窗口 | ms/tick | 工作段 ms | `WorldΣ` ms | `hostΣ` ms | host 占比 | `HostAoiDownlink` | `HostPeriodic` | `HostFrames` | `HostSleep` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 280 | 50 | 200.4 | 48.9 | 40.0 | 8.9 | 18.2% | 3.9 | 2.9 | 1.8 | 151.5 |
| 744 | 50 | 200.5 | 84.0 | 66.0 | 18.0 | 21.4% | 9.4 | 5.3 | 2.9 | 116.5 |
| 1,396 | 42 | 241.2 | 236.6 | 179.3 | 57.3 | 24.2% | 36.9 | 12.2 | 7.8 | 4.6 |
| 1,715 | 32 | 322.4 | 322.4 | 238.3 | 84.0 | 26.1% | 57.4 | 15.5 | 10.6 | 0.0 |
| 2,411 | 21 | 479.0 | 479.0 | 344.6 | 134.4 | 28.1% | 96.5 | 21.5 | 15.9 | 0.0 |
| 2,927 | 17 | 595.0 | 595.0 | 419.3 | 175.8 | 29.5% | 131.2 | 25.0 | 18.7 | 0.0 |
| 3,449 | 10 | 1090.0 | 1090.0 | 747.9 | 342.1 | 31.4% | 260.8 | 44.8 | 35.1 | 0.0 |
| 3,993 | 13 | 821.4 | 821.4 | 568.1 | 253.3 | 30.8% | 194.4 | 33.6 | 25.0 | 0.0 |
| 4,467 | 11 | 937.2 | 937.2 | 641.9 | 295.3 | 31.5% | 226.8 | 38.6 | 29.5 | 0.0 |
| 4,969 | 10 | 1043.7 | 1043.7 | 706.2 | 337.4 | 32.3% | 264.2 | 41.2 | 31.6 | 0.0 |
| 5,475 | 9 | 1155.6 | 1155.6 | 773.4 | 382.2 | 33.1% | 300.8 | 45.4 | 35.5 | 0.0 |
| 5,990 | 8 | 1299.5 | 1299.5 | 873.3 | 426.2 | 32.8% | 337.0 | 51.0 | 37.7 | 0.0 |
| 6,498 | 7 | 1479.2 | 1479.2 | 929.4 | 549.8 | 37.2% | 454.0 | 53.4 | 41.9 | 0.0 |
| 6,936 | 7 | 1571.3 | 1571.3 | 1035.9 | 535.4 | 34.1% | 426.8 | 61.1 | 47.0 | 0.0 |
| 7,496 | 6 | 1729.3 | 1729.3 | 1079.3 | 650.0 | 37.6% | 541.5 | 62.3 | 45.7 | 0.0 |
| 7,914 | 6 | 1870.5 | 1870.5 | 1150.3 | 720.2 | 38.5% | 598.8 | 66.3 | 54.7 | 0.0 |

三个完整窗口（原样摘录，`E:/tmp/t10b-w85/logs/game-svr.out.log`）：

**A. 实体内 270（bot 刚进世界，未过载）**：整 tick 200.45ms（sleep 153.6 = 76.6%），
工作段 46.9ms。宿主侧：`HostWorldHead` 23.7 + `HostWorldTail` 14.4（= `World::tick` 38.1）
+ `HostAoiDownlink` 3.8 + `HostPeriodic` 2.9 + `HostFrames` 1.8 + `HostPlayerRun` 0.10。
`HostTickTotal` 200.447 / `HostUnaccounted` 0.012（0.006%）。

**B. 实体内 2,839（bot 仍在世界）**：整 tick 590.70ms（sleep 0，已超 200ms 预算）。

| 段 | ms/tick | 占 tick |
| --- | --- | --- |
| `HostWorldHead`（`World::tick_head` + 专用怪） | 264.98 | 44.86% |
| `HostWorldTail`（`World::tick_tail`） | 154.11 | 26.09% |
| `HostAoiDownlink`（AOI 出现帧 + 52/53/体力下行） | 127.48 | **21.58%** |
| `HostPeriodic`（SystemProcessor 10s 块/MonGen/公告） | 25.20 | 4.27% |
| `HostFrames`（本 tick 其余帧面排水） | 18.60 | 3.15% |
| `HostPlayerRun`（P21..P32） | 0.14 | 0.02% |
| 其余 11 段合计 | 0.19 | 0.03% |
| **`WorldΣ`（20 段）** | **418.96** | **70.9%** |
| **`hostΣ`（World 之外）** | **171.7** | **29.1%** |

**C. 实体内 7,908（世界自然涨到 MonGen 上限，`在世界会话 0`，即演示栈「~2000ms」的真实规模）**：
整 tick **1,831.29ms**（sleep 0），与演示栈记录的「~2000ms」同量级 —— **复现，不再是外推**。

| 段 | ms/tick | 占 tick |
| --- | --- | --- |
| `HostWorldHead` | 721.63 | 39.41% |
| `HostAoiDownlink` | 567.42 | **30.98%** |
| `HostWorldTail` | 417.73 | 22.81% |
| `HostPeriodic` | 66.53 | 3.63% |
| `HostFrames` | 54.67 | 2.99% |
| `HostDiag`（诊断行本身） | 3.04 | 0.17% |
| 其余 12 段合计 | 0.18 | 0.01% |
| **`WorldΣ`（20 段）** | **1,139.11** | **62.2%** |
| **`hostΣ`（World 之外）** | **692.2** | **37.8%** |

同窗口 World 内部：`SimMonsterAi` 244.7ms（21.5% of World），`SimSlaveCleanup` / `SimSettleDeaths` /
`TailVerifyMapTime` / `SimDup` / `TailPlayerDeaths` / `OutputAoi` 六段各 134.5–136.4ms（每段 ~11.9%）。

World 内部同窗口（C 之前的 2,839 实体档）：`SimMonsterAi` 88.8ms（21.2% of World）、`TailPlayerDeaths` 50.5、
`SimSettleDeaths` 50.0、`TailVerifyMapTime` 50.0、`SimSlaveCleanup` 49.5、`OutputAoi` 49.3、
`SimDup` 48.8（7 段各占 ~12%），`SimMerchant` 8.8、`SimNpc` 5.9、`SimExpireHitters` 5.2。
⇒ §6 Top1 的「五个全量扫描段每实体固定成本」在**真栈同构出现**（七个 ~12% 段）。
对账：`HostWorldHead+HostWorldTail` = 419.06 vs `ΣWorld.*` = 418.96 ⇒ 宿主附加 0.04ms
（专用怪产仔/ref 排水），窗口对账误差 < 0.01%。

### 12.5 32× 差额：能解释多少、还剩什么

1. **构建口径差一个数量级（主因候选）**：演示栈与本节都是 **dev profile**（`target/debug`），
   而 §3/§4 的 bench 是 `--release`。本栈 `World::tick` 每实体 144–148µs（2,839 实体 147.6µs / 7,908 实体 144.0µs），
   bench release 是 **5.8µs/实体** ⇒ **25×**。这解释了 32× 的绝大部分来源方向：
   拿 release bench 的 64ms 去对 debug 真栈的 2000ms，本身就不是同一口径。
2. **实体数随运行时长增长（世界自己涨）**：MonGen 不停刷，3 分钟 270 → ~3,000，约 36 分钟到
   ~7,900 并趋稳。实测 tick 与实体数近似线性（0.20–0.24ms/实体，debug）：
   **7,914 实体 ⇒ 1,870.5ms/tick**，演示栈的「~2000ms」由此复现（§12.4 窗口 C）。
3. **宿主段（`World::tick` 之外）随实体数增长，占比 18% → 38.5%**：6.4ms（205 实体）→
   720ms（7,914 实体），**不是**固定小尾巴。最大单项是 `HostAoiDownlink`
   （2.5 → 598.8ms；占整 tick 21.6% → **32%**）——它是「AOI 出现帧 + 52/53/体力下行」的
   生成/入队，**不含 socket 写**（写在各 gate 线程，见 §12.6）。`HostPeriodic`（SystemProcessor
   10s 块、含 MonGen）与 `HostFrames` 也随实体数增长（2.4→66ms / 1.3→55ms）。
4. **P21..P32 宿主段本身可忽略**（0.10–0.14ms/tick）：这批段的昂贵部分不在宿主胶水里，
   而在 `World::tick` 内部的 `Tail*`/扫描段与 AI。

⇒ 结论修正 §8：32× = **构建口径差（release bench vs debug 真栈，~25×/实体）**
× **真栈实体数持续增长（270 → 7.9k，~29×）** 的乘积方向；2000ms 在 7.9k 实体处直接复现。
**宿主段占 18%→38.5%**（实体越多占比越高），其中 AOI 下行是最大的非 World 单项（31% tick @7.9k），
但**不是** 32× 的主体；「World::tick 之外藏着全部差额」与「宿主段可忽略」两种说法都被数据否定。

### 12.6 本节的边界（不覆盖什么）

- **不入档**：gate 线程里的 socket 写（`apply_sim_cmd_async` → `send_gate_msg`）不在 `sim_loop`，
  `SimToGate` 是 unbounded 通道 ⇒ 本节量到的只是「帧生成 + 入队」；真实网络写/背压要在 gate 侧量。
- **DB 往返**：`drain_periodic_save` 在本机 < 0.03ms/tick（异步），DB 侧耗时不在本节。
- **tick 内部细分**：`HostAoiDownlink` 的 127ms 没有进一步拆分（哪个子段贵）——需要下一张卡
  在同一接缝加名字探针（`drain_run_downlinks` 的 AOI/52/53/体力四支）。
- 计时器本身在开启态有 ~1µs/tick 的 `Instant` 成本；World 探针开启态在 T10 §7 实测 ≤2.23%
  （同机噪声内），本节不重复做 A/B。
- 日志时间戳是 **UTC**（`glog` 取 `SystemTime`），与本地 +0800 差 8h —— 引用时间线时注意。

### 12.7 复现

```sh
# 1) 构建（独立 target dir；dev profile，与演示栈同形）
cd mir2-rs && CARGO_TARGET_DIR=E:/UserTemp/w85b-target cargo build -p game-svr --bin game-svr
# 2) 起栈（与 E:/UserTemp/fourseam-logs/start.ps1 同 env/同参数；本车道脚本见 §12.8 证据路径）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/tmp/t10b-w85/start.ps1
# 3) 1 bot（空闲挂机 120s；-WriteConfigOnly 后直接起 BotSrv）
powershell -File tools/botload/run_bots.ps1 -Count 1 -Prefix t10bc -NewAccount -DisableAutoPlay \
  -SoftCloseAfterWorldMs 120000 -SkipBuild -WriteConfigOnly -LogFile E:/tmp/t10b-w85/bots/t10bc.log
$env:MIR2_BOT_OUT='E:/tmp/t10b-w85/bots/t10bc'; src/BotSrv/bin/Release/BotSrv.exe
# 4) 取数
grep -a TICKPROF E:/tmp/t10b-w85/logs/game-svr.out.log
# 5) 停栈（**先等「在世界会话 0」再杀**；本车道按 pids.txt 里的 pid 杀）
```

### 12.8 门禁与本卡证据路径

| 门 | 命令（`mir2-rs/` 下） | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | RC=0 |
| clippy | `cargo clippy -p game-svr --all-targets -- -D warnings` | RC=0（全工作区另见交付报告） |
| test | `cargo test -p game-svr --lib` | 1787 passed / 0 failed / 4 ignored（含本卡 5 条新单测） |
| msgcodegen | `cargo run -p msgcodegen -- --check` | RC=0 |
| 散文锚 | `python -I -B tools/hygiene/docs_anchor_gate.py --prose --base master` | 见交付报告 |

原始证据（未入库）：`E:/tmp/t10b-w85/logs/game-svr.out.log`（`TICKPROF` 行）、
`game-svr.err.log`、`bots/bot2.out.log`（bot 进世界/小退时间线）、`start.ps1`、
`logs/pids.txt`（本车道起的 4 个 pid）。
