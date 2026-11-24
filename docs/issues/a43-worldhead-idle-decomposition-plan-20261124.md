# A43 第二段：`HostWorldHead` 5.3 ms 拆解与优化候选（只读）

> **状态**：只读分析（2026-11-24，`card/a43-worldhead-plan`，base master `9e467102b`）。不改产品码。数据来源：a7 的 360s instrumented 档（Tree `99a622933`/master `9e467102b`）与其 50-tick 窗口行；括号内给出可核对的原始行位置（日志文件路径见 §0）。
> **符号锚**：`lib.rs` / `tick_profile.rs` = `mir2-rs/bins/game-svr/src/`；`world.rs` / `monai.rs` / `slave.rs` / `tick.rs` = `mir2-rs/crates/world-bevy/src/`。

## 0. 数据（两个窗口，排名一致）

a7 给的稳态：固定成本 10.99 ms/tick，其中 `HostWorldHead` **5.31 ms（48.31%）**、`SimMonsterAi` 3.27 ms。

我按同一 run 的另一 50-tick 窗口对账（`E:/UserTemp/l25-slice/zone-slice-20261124-222123/game-svr.out.log`，`14:27:28.921` 行起）：

```
HostTickTotal 200.405；HostSleep 184.418；工作段 15.99
HostWorldHead 7.831 / HostWorldTail 3.745 / HostPeriodic 1.737 / HostAoiDownlink 1.630 / HostFrames 1.010
World.SimMonsterAi 5.055；counts/tick：slot 6736，scanned 6600（inactive 6600），
  throttle-skip 4403，ran 2197（dead 0 / frozen 159 / no-target 2191 / idle-wander 2191 / walk-due 907 / target 7）
对账：Head+Tail = 11.576；ΣWorld.* = 9.606；宿主附加 = 1.970
```

两个窗口的绝对值随刷怪爬坡而变（Head 5.31 → 7.83），但**结构排名不变**：`SimMonsterAi` 独占 52–64%，其余每段 0.1–0.5 ms。下文用 22:27 窗口做算术（它同时给出了全部 World 段）。

## 1. `HostWorldHead` 包含什么（按代码顺序）

`lib.rs` `tick_world_head_with_specials` = **三段**：

1. **`world.tick_head()`**（`world.rs`）——15 个 World 段（`tick.rs` `STAGE_ORDER` 的前 15 个，按序）：
   `Clock` → `Command` → `SimHiterSnapshot` → `SimExpireHitters` → `SimSettleDeaths` → `SimSelfDeliver` → `SimExtraAbil` → `SimMapEvents` → `SimSlaveCleanup` → `SimBuff` → `SimMonsterAi` → `SimNpc` → `SimMerchant` → `SimPlayerRunExpiry` → `SimDup` → `OutputDrain`。
2. **`world.flush_special_spawns(...)` + `world.flush_sculture_spawns(...)`**：专用怪产仔（`BeeQueen`/`ScultureKing` 的 `RegenMonsterByName`），只处理本 tick 待产列表。
3. **`drain_special_ref_msgs(world, online, gates)`**：专用怪 `Ref*` 帧排水。

**与 World 探针的关系：不是重复计算。** `HostWorldHead` 是本段**墙钟**；`World.*` 每个段计时器是它内部的嵌套计时。算术核对（22:27 窗口）：把 15 个 head 段相加 = **7.810 ms**，而 `HostWorldHead` = 7.831 ms ⇒ **head 侧宿主附加（专用怪产仔/ref 排水）≈ 0.021 ms**。也就是说：**`HostWorldHead` 基本就是它内部 15 个 World 段之和**；`对账` 行里那 1.970 ms「宿主附加」几乎全在 **tail 侧**（我们上一轮加的 7 个子探针不是 World 段，落在差额里）。⇒ **本轮不需要再为 head 新加仪表**。

另注：`World.SimMonsterAi` 的分解行 `gate 0.437 ms/tick` 是**探针自己的只读预扫描**（`monai.rs` `monster_ai_tick_impl_probed` 的 `scan_gate`），只在 profiling 开启时存在；生产成本 ≈ `collect + loop`（本窗口 0.355 + 4.250）。

## 2. 6.6k 实体/6.6k AI slot 下的「O(实体) 扫描」与「真在做活」

| 段 | 本窗口 ms | 扫描形态（代码事实） | 空载时在做什么 |
| --- | --- | --- | --- |
| `SimMonsterAi` | **5.055** | `order_ids()` + `retain` 身份门（collect 0.355）；逐怪循环 4.25 | 6600 怪：4403 个被 `ProcessMonsterInterval` 节流跳过（只读/写 `run_count` 后 continue）；2197 个真跑 AI 体（**oracle 语义**），其中 2191 无目标（Wondering），907 走到点，7 有目标 |
| `SimMerchant` | 0.465 | 扫全表找 `merchant.is_some()` | 世界只有 1 个商人；扫描成本 ≫ 业务 |
| `SimNpc` | 0.381 | 扫全表找 `npc.is_some()` | 固定 NPC 0–3 个 |
| `SimSettleDeaths` | 0.369 | 扫全表找 `death` 的非玩家 | dead=0 |
| `SimDup` | 0.356 | 扫全表筛 `Race==Play && !Ghost`（`world.rs` `check_dup_obj_core`） | **0 玩家** ⇒ 筛出空集，纯浪费 |
| `SimSlaveCleanup` | 0.354 | 扫全表找 slave 归属 | 空载 0 slave |
| `SimExpireHitters` | 0.305 | **两趟**全表：先收集 `death||ghost` 的 `gone`，再逐实体清 `last_hitter/exp_hitter`；第二趟每实体还做 `gone.contains` 线性查找 | 空载无 hitter ⇒ 两趟都白跑 |
| `SimExtraAbil` | 0.168 | 扫全表 ×7 槽 | 无到期项 |
| `SimPlayerRunExpiry` | 0.135 | 扫全表找 `player_run` 到期 | 0 玩家 |
| `SimHiterSnapshot` | 0.071 | 扫全表 | 无 hitter |
| 其余 head 段 | ≤0.05 各 | Clock/Command/SelfDeliver/MapEvents/Buff/OutputDrain | 与实体数弱相关 |

**结论**：head 里 **≈2.3 ms 是「按种类扫全表」的固定开销**（`SimMerchant`/`SimNpc`/`SimSettleDeaths`/`SimDup`/`SimSlaveCleanup`/`SimExpireHitters`/`SimExtraAbil`/`SimPlayerRunExpiry`/`SimHiterSnapshot` 相加 ≈ 2.3–2.4 ms），其中在空载语义下真正有活干的只有「1 商人 + 0–3 固定 NPC」；`SimMonsterAi` 的 4.6 ms 里约 0.9–1.3 ms 是 4403 条节流簿记（真工作 3.8 ms 是 oracle 语义，不能省）。

## 3. 候选（按语义风险排序）

### 候选 A（推荐）：**每 tick 一份「实体种类索引」**
- 位置：`world.rs` `tick_head` 在 `Command` 段之后（`Command` 是 head 内唯一的结构增删段）用**一次** `order_ids()+get` 建分桶：`plays`（含 live `!ghost` 复核）、`npcs`、`merchants`、`slaves`、`dead_nonplayer`、`hitter_bearing`、`run_states`、`extra_abil_bearing`。`SimNpc` / `SimMerchant` / `SimSettleDeaths` / `SimDup` / `SimSlaveCleanup` / `SimExpireHitters` / `SimPlayerRunExpiry` / `SimExtraAbil` 改迭代对应桶（**桶内仍是槽位序**）。
- 收益：省掉 8–9 趟全表扫里与业务无关的部分；按 22:27 窗口合计 ≈ **2.3 ms − 索引构建 ≈0.4 ms = 净 ~1.9 ms**（固定成本 10.99 → ~9.1）。`SimMonsterAi` 不动（它自己的 collect 与顺序敏感逻辑不在本候选内）。
- 语义风险：**低–中**。桶只决定「看谁」，每段仍在**迭代时做 live 检查**（`access.get` 存在性、`death/ghost`、3000/10000ms 门、槽位序），与逐行照旧；`Command` 之后不会新增实体（AI/产仔在 head 之后），`SimSlaveCleanup` 删除的 slave 后续桶里仍在，各段现有的 `get→None continue` 会跳。
- 牙：①每段对**合成混合夹具**（玩家/怪/NPC/商人/slave/dead/ghost/被删）断言事件与旧「全表过滤」逐项相同；②顺序牙：桶内严格槽位序；③空集正例：零玩家/零 slave/零 hitter 时各段事件为空且不再扫；④既有 RNG trace / golden / 两区牙全绿（本候选不触 RNG 与帧面）。
- 需先确认的代码事实（实现前逐条核）：`SimSettleDeaths`/`SimSlaveCleanup`/`SimPlayerRunExpiry` 的谓词是否需要 live 复核（例如 `settle` 会改 ghost/death，后续段不得用旧桶当真值）。

### 候选 B：**`SimMonsterAi` 合并 collect + 节流判定**
- 位置：`monai.rs` `monster_ai_tick_impl_probed`：现在先 `order_ids+retain`（每实体 `get`），再循环里对每个 id 再 `get` 一次拿 `visible_active/run_count`。可合并为一次遍历：一次 `get` 同时做身份门与节流判定（被跳过的继续，不重复取）。
- 收益：省 6600 次重复 `access.get` + 一次 `Vec` 过滤；预期 **0.2–0.4 ms**。
- 语义风险：**低**（判定式、次序、写回全不变）；牙：节流计数 `scanned/throttle-skip/ran` 与 RNG trace 逐 tick 相同（现有 `counts/tick` 与 golden 已可直接对账）。

### 候选 C：**`SimMonsterAi` due-bucket**（跳过 4403 条「未到点」）
- 做法：维护每怪的 `next_due` 环/桶，本 tick 只处理到点集合；`run_count` 语义按 `ProcessMonsterInterval` 复刻。
- 收益：约 **0.5–1.0 ms**（4403 条 `get/set run_count` 的簿记）。
- 语义风险：**中–高**：可见性翻转、`interval` 变化、RNG 次序（必须保持槽位序而非 due 序）都需逐项证明；需专门的对拍牙。**建议 A/B 复测后再议**。

### 不建议动
- `SimNpc`/`SimMerchant` 的**业务**本身（有实体、逐 tick 语义要求）；候选 A 只省扫描不省逻辑。
- 专用怪产仔/ref 排水：已由算术确认 head 侧 ≈0.02 ms，不值得。
- 「降频/跳 tick」类：语义偏离需新裁决，且不针对扫描根因。

## 4. 结论

**「值得做」**：先做 **候选 A（实体种类索引，预期 −1.9 ms）**，再做 **候选 B（AI 合并遍历，预期 −0.3 ms）**；两者合计把 head 从 5.31 压到 ≈3.1 ms、固定成本从 10.99 压到 ≈8.8 ms。候选 C 留到 A/B 复测后再评估。

实现前置：先把 A 涉及 8 个段的谓词逐条写下（含 live 复核点）并给出每段的等价牙，再动手；不许把「空桶跳过」做成「有实体也跳过」。
