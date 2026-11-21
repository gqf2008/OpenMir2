# Tick 并行分组设计（(C) 只读设计，2026-11-21）

> **只读设计，不改产品代码。** 基于 step2 实测（MultiThreadedExecutor 零加速 ±2%）+ 结构勘察。
> **红线**：本卡只给资源矩阵与**估算**上界，**不承诺"能显著提速"**——让数字说话。

## 0. 核心结论（先看）

**当前无任何可用并行面，且是结构性的、不是偶然。** 两条硬根因：
1. **11 个候选 system 的签名全是 `fn(world: &mut World)`（bevy 独占系统）** ⇒ bevy 本就无法让独占系统并发（互斥借用整个 World）。
2. 即便去掉独占，它们**全部读写同一个 `SimState` 资源**（`world.resource_mut::<SimState>()`）；而 `SimState` 是**单一巨石结构：78 个字段**（entities/maps/sessions/clock/rng/buffs/merchant/npc/self_delay/… 全在一处）⇒ **两两写冲突**，无任何可并行组。

⇒ **真并行 = 先拆分 `SimState` 成细粒度资源 + 系统改 `Res/ResMut`（不再 `&mut World`）**——这是大改造。
这正是 step2「MultiThreaded 零加速（±2%）」的结构原因，不是实现没调好。

## 1. 资源访问矩阵（当前代码）

每个 stage-system 的实际签名与访问（勘察 `world.rs`，11 支）：

| system | 签名 | 写 `SimState` | 写 `StageOut` | 其它突出访问 |
|---|---|---|---|---|
| `sim_buff_stage_system` (SimBuff) | `(&mut World)` | 是(`resource_mut`) | 是(`insert_resource`/`resource_mut`) | `resource::<SimState>` |
| `sim_dup_stage_system` (SimDup) | `(&mut World)` | 是 | 是 | — |
| `sim_merchant_stage_system` (SimMerchant) | `(&mut World)` | 是 | 是 | merchant_position/pass_tick |
| `sim_npc_stage_system` (SimNpc) | `(&mut World)` | 是 | 是 | npc_position/pass_tick |
| `expire_hitters_stage_system` (SimExpireHitters) | `(&mut World)` | 读/写 | — | `get_mut::<CombatAttribution>` |
| `expire_extra_abil_stage_system` (SimExtraAbil) | `(&mut World)` | 是 | 是 | `get_mut::<BuffState>` |
| `sim_hiter_snapshot_system` (SimHiterSnapshot) | `(&mut World)` | 是 | — | `resource::<TickEntityIds>` |
| `sim_map_events_stage_system` (SimMapEvents) | `(&mut World)` | 是 | — | — |
| `sim_self_deliver_stage_system` (SimSelfDeliver) | `(&mut World)` | 是 | 是 | self_delay_msgs |
| `sim_slave_cleanup_stage_system` (SimSlaveCleanup) | `(&mut World)` | 是(委派) | — | — |
| `tail_player_deaths_stage_system` (TailPlayerDeaths) | `(&mut World)` | 是(委派) | — | — |

**两两冲突**：因**共享写 `SimState`** ⇒ **全部 C(11,2)=55 对都冲突**。冲突资源 = `SimState`（写）、`StageOut`（写）；外加独占签名使 bevy 本就串行。

## 2. 可并行分组（**须先拆 `SimState` 才谈得上**）

只有在「把 `SimState` 拆成不相交子资源 + 系统改 `Res/ResMut`」之后，才可能出现同阶段并行组。**候选假设**（**落地前必须逐支实测资源足迹，见 §4**）：

- **候选组 G1 = {SimMerchant, SimNpc, SimBuff}**：
  - 理由（**待证伪**）：三者各写自己的子状态（`merchant_position/pass_tick`、`npc_position/pass_tick`、`buffs/buff_pass_tick`），**若不遍历 `entities`/`maps` 的同一张表**则不冲突。
- **其它系统暂不入组**：`SimSelfDeliver`/`SimExpireHitters`/`SimHiterSnapshot`/`SimMapEvents`/`SimSlaveCleanup`/`TailPlayerDeaths`/`SimDup` 都触达**共享实体存储 `entities`**（`world.get_mut`/遍历）⇒ 两两冲突，即便拆 `SimState` 也难并行。

> **重要**：§2 是"**假设候选**"，不是"已确认可并行"。要让某组成立，必须实测每支的**最小资源足迹**（它到底读/写哪些子资源、是否遍历 entities）。

## 3. 不可并行的（当前）

- **全部 11 系统两两不可并行**：共享写 `SimState`（78 字段巨石）+ 独占签名（`&mut World`）。冲突资源 = `SimState`、`StageOut`。
- 多数系统还共享写**实体存储 `entities`** ⇒ 即便拆 `SimState`，**凡碰 entities 的两两仍冲突**（限缩并行组到极小的子集）。

## 4. 契约影响 + 如何证明"同阶段内顺序不可观测"

- 现状契约：**阶段序 = 唯一顺序维**（`STAGE_ORDER` + `.chain()` 严格串行）。
- 同阶段并行后，**同阶段内 system 的相对先后不得被观测**（否则并行不安全/非确定）。
- **证明方案（三件齐）**：
  1. **8× 重放**：`tests/no_parallel.rs::replay_determinism_8x`（已建）——同固定输入 8 次逐字节一致；并行组启用后仍须绿。
  2. **全量对拍**：`-Mode Both`（Driver/Comparator 全绿）——并行不改变帧序/值。
  3. **红证构造**（"若把并发顺序写进输出则必红"）：在**测试夹具**里造一个 schedule，两个系统**各自把执行先后写进同一输出**（A 先 push 则序列 `[A,B]`，B 先则 `[B,A]`）⇒ **8× 重放必红**（顺序可观测 ⇒ "顺序无关"判据不成立）；或让组内两系统藏一条 read-after-write（B 读 A 写的值）⇒ 重放红。**这条把"顺序无关"钉成可证伪判据，不是口号。**

## 5. 收益上界（**估算，非承诺**）

- step2 实测：MultiThreaded **当前零加速**（±2%，无并行面）。
- **理论上界（Amdahl）**：每 tick 可省 ≤ `Σ(组内各 system 耗时) − max(组内单 system 耗时)`；且被"共享 entities"进一步限缩。
- 用本次 default-arm 段占比作**粗代理**（*非 client 档逐系统实测*）：`SimMonsterAi` 在 8000 怪臂最大（~364/616ms），其余多在个位~几十 ms。
  - **client 档 (a) 500 = 21ms、1000 = 43ms**：system 都是小件，**乐观上界**≈把少数互不相交的小 system（若 §2 G1 成立）并行掉，**或省几 ms 量级**；
  - **(b) 2000-mix 由 `SimMonsterAi` 主导**（182/236ms 里大头是怪 AI），**并行其余省不了多少**。
- **明确**：(1) 当前 **0 可兑现**；(2) 前提是拆 `SimState`；(3) 拆后上界仍须逐系统实测才能定——**本卡只给"不显著"的预估，不承诺提速**。

## 6. 建议的落地顺序（若做 (C)）

1. **第一步不是并行，是拆 `SimState`**（78 字段 → `entities`/`self_delay`/`clock`/`rng`/`buffs`/`sessions`/`merchant`/`npc`/… 细粒度资源）+ 系统改 `Res/ResMut`（去 `&mut World`）。
2. 逐支实测**资源足迹**（哪些 system 真只碰不相交子资源、不遍历 entities）。
3. 只对**实测确认不相交**的组开同阶段并行 + §4 三件证明。
4. 在那之前，保持 (A) `MultiThreadedExecutor`（安全、确定性、零加速）。
