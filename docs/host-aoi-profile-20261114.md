# 宿主段 AOI 下行细分探针与分解（2026-11-14）

卡：`openmir2-host-aoi-downlink-profile`（tier R）。
分支：`card/host-aoi-probe`，基 `176cfb609`（master，S5 `crates/world` 退役之后）。
**本批范围：只加探针 + 测量 + 报告，不做任何优化**（卡面 A3/A4 留给后续卡）。
测量时段：**2026-11-14 13:37:37–13:38:45（+0800）**，release、独立 target `E:/UserTemp/w84-rel`，
**与 14:20–14:35 的全机禁编窗口无重叠**。

> **口径声明（先读）**
>
> 1. 下表的 ms 是**本机 release 进程内**测量，不是跨机器验收值；横向结论只用
>    「同一进程、同种子、同 tick 数」的三臂对照与「µs/走位者」「ms/tick」的相对份额。
> 2. 场景与容量模型同源（200×200、`race=80` 怪物、AI/buff/npc/merchant 全开、固定种子），
>    但有两处**已登记偏差**（§2），绝对值不能直接当容量模型读数。
> 3. 本批没有起真栈；**不需要**真栈窗口。理由与代价见 §6。

## 0. 结论摘要

1. `HostAoiDownlink` 的绝对主项不是玩家 AOI，而是**怪物走位帧的收件人扇出**：
   `drain_monster_walk_downlink` 对每个走位者调一次 `broadcast_ref_msg`，后者经
   `chat::hear_targets` → `hear_targets_at` 对**全部实体**做一遍 `iter_ref()` 扫描。
   在 7,908 实体 / 0 玩家时，它占 `HostAoiDownlink` 的 **95.5%**；11,000 实体 / 1,000 bot 时占 **92.3%**。
2. 单次扇出的成本与实体数**线性**：**264 µs/走位者 @7,908**、**377 µs/走位者 @11,000**
   （实体 ×1.39，单价 ×1.43）⇒ 总成本是 **O(走位者数 × 实体数)**。
   这条与 T10b 的实测自洽：T10b 7,914 实体 / 0 会话的 567 ms（dev）按 25× 折 release ≈22.7 ms，
   ÷264 µs ≈ **86 走位者/tick**（≈1.1% 实体/tick）——正是真栈 MonGen 怪的游走量级。
3. 玩家驱动的出现/消失/编码入队**在本场景里不是瓶颈**：1,000 bot 时
   `HostAoiAppear` = 0.590 ms/tick（**0.51%**），其中帧构造 0.328、可见集选择 0.153、入队 0.021。
   1 bot 时整段只有 0.002 ms/tick。
4. 第二大项是 `HostAoiStamina`（1.1–2.1%）：它包含 `drain_stamina` 末尾的
   `drain_close_health_residual` —— 对**全部实体**做一遍 `close_health_scan_ids`（与玩家数无关，
   随实体数线性）。
5. 关闭态零开销：`off`（传关闭态 profile）与 `none`（完全不传探针）的每 tick 均值差在
   **−0.3%…+1.1%**（噪声带内）；`on` 与 `off` 差在 **−0.9%…+3.5%**（四档 ≤1.1%，11k/0 档
   +3.52% 是单档噪声，见 §5.2）。代码级口径见 §5。
6. 最大优化候选（**只是建议，本批未实现**）是把扇出从「每走位者一次全实体扫描」改成
   「每 tick 一次玩家索引/格表」：按本报告观测的扇出份额（92–97%）推算，可拿掉
   `HostAoiDownlink` 的 ~90%，即宿主段（`hostΣ`）的 ~79–83%。详见 §8。

## 1. 子探针清单（默认关闭，`MIR2_TICK_PROFILE=1` 开启）

全部是新加的**命名子探针**（`HostTickProfile::probe`）：只观测，**不进** `tick_sum_ns`，
因此不改变既有 17 段的残差对账（`HostUnaccounted`）。它们与父段 `HostAoiDownlink` 是**嵌套**关系，
下表用「父段 → 子项」呈现；子项之和可以超过父段，因为存在包含关系（已在表里标注）。

| 子探针 | 测量范围（代码结构） | 嵌套于 |
| --- | --- | --- |
| `HostAoiKillExp` | `drain_kill_exp_chain_with_magic` | `HostAoiDownlink` |
| `HostAoiNpcTurn` | `drain_npc_merchant_turns` | `HostAoiDownlink` |
| `HostAoiDisappear` | `drain_aoi_disappear`（整段） | `HostAoiDownlink` |
| ├ `HostAoiLeaveKind` | 每条 hide 的 `hide_observer_kind` 线性查找 | `HostAoiDisappear` |
| ├ `HostAoiLeaveBuild` | `downlink::disappear_frame` 帧构造 | `HostAoiLeaveEnqueue` |
| └ `HostAoiLeaveEnqueue` | `route_to_gate` + `tx.send`（含上项） | `HostAoiDisappear` |
| `HostAoiAppear` | `drain_aoi_downlink`（整段） | `HostAoiDownlink` |
| ├ `HostAoiAppearSelect` | 在线坐标按名查找 + `aoi_show_targets` + `aoi_appearance_slots` + 出队序 | `HostAoiAppear` |
| ├ `HostAoiAppearBuild` | 展示表查找 + 朝向覆盖 + `downlink::appear_frames` | `HostAoiAppear` |
| └ `HostAoiAppearEnqueue` | `route_to_gate` + 逐帧 `tx.send` | `HostAoiAppear` |
| `HostAoiEvent` | `drain_aoi_events`（804/805） | `HostAoiDownlink` |
| `HostAoiMonsterWalk` | `drain_monster_walk_downlink`（整段） | `HostAoiDownlink` |
| └ `HostAoiWalkFanout` | 每个走位者的 `broadcast_ref_msg`（含 `hear_targets` 全实体扫描） | `HostAoiMonsterWalk` |
| `HostAoiNpcHit` | `drain_npc_merchant_hits` | `HostAoiDownlink` |
| `HostAoiMerchantLeave` | `drain_merchant_disappears` | `HostAoiDownlink` |
| `HostAoiLoginTail` | `drain_login_tail`（654/42/100 补发） | `HostAoiDownlink` |
| `HostAoiStamina` | `drain_stamina`（含 `drain_close_health_residual` 全实体扫描） | `HostAoiDownlink` |

- 「走位扫描 + 帧构造」= `HostAoiMonsterWalk − HostAoiWalkFanout`（本报告单列，因为扇出是子集）。
- 「消失帧构造」= `HostAoiLeaveBuild` 是 `HostAoiLeaveEnqueue` 的嵌套子项（保持原实现的
  「先路由、后构造」次序：帧构造原本就在 `tx.send` 实参里）。
- 开启态输出位置：`tick_profile::log_window` 在既有 `[TICKPROF]` 汇总表后新增一节
  `宿主命名子探针（默认关闭；MIR2_TICK_PROFILE=1 时每窗口输出）`。

## 2. 测量方法与场景

工具：`mir2-rs/bins/game-svr/src/host_aoi_bench.rs`（`#[cfg(test)]` 模块内、`#[ignore]` 的
**测量 harness，不是判据**；`MIR2_HOST_AOI_BENCH=1` 二道门）。
命令：

```text
CARGO_TARGET_DIR=E:/UserTemp/w84-rel cargo test --release -p game-svr --lib \
    -- --ignored --nocapture host_aoi_bench
```

每次测量走**三臂**（同种子、同世界构造、同 tick 数）：

- `none`：不传探针句柄；
- `off`：传关闭态 `HostTickProfile`（生产缺省形态）；
- `on`：传开启态 profile，收集子探针。

每臂 = 12 tick 预热 + 30–40 tick 测量；每 tick 只对 `drain_run_downlinks_with_magic` 计时，
世界 tick（`tick_head` + `tick_tail`）与 gate 通道排空都在计时窗外。

**场景（与 `docs/capacity-model-20261114.md` 同源）**：200×200 单图、`race=80` 怪物、
MonGen/AI/buff/npc/merchant 全开、固定种子 42；玩家（bot）建**真会话**
（`open_session` + `enter_map`，`race=0`、`view_range=5`），并登记 `online` 与 gate 通道，
使 1000ms 首扫门、出现帧、体力段都真的跑。

**两处已登记偏差**：

1. **横向隔格布局**：容量模型用 199 列逐行铺满；本 harness 用 `x = 1 + 2*(i % 99)`。
   原因：`try_step_impl` 的目标格被活着的 actor 占据就不移动 ⇒ 铺满时任何单格走位都被挡，
   `drain_monster_walk_downlink` 恒零，测不到真栈怪的持续游走。代价：玩家 11×11 视野内
   可见实体约少一半，故出现帧量级是容量模型稠密图的下界。
2. **走位注入**：每 tick 让 `i % 40 == tick % 40` 的怪物 `world.walk(±1, 0)`（先置
   `ai.dir` 再走，方向每 4 tick 翻转）。真栈里这是怪物 AI/MonGen 的自然游走；合成世界没有
   目标可追，只能显式注入。**注入率 2.5%/tick**（≈231–263 走位者/tick @11k）≈ 真栈的 2 倍
   （§7 用 T10b 反推真栈约 1.1%）；份额不受影响，绝对 ms 按走位者数线性缩放即可。

## 3. 三档测量（11,000 实体；1 / 100 / 1,000 bot）

「占比」以该臂 `on` 的**宿主段均值 ms/tick** 为分母。`HostAoiMonsterWalk` 与
`HostAoiWalkFanout` 是包含关系，不能相加。

### 3.1 总览

| bot 数 | 宿主段均值 ms/tick | p50 | p90 | max | hides/tick | 下行帧/tick |
| --- | --- | --- | --- | --- | --- | --- |
| 0（参照） | 89.905 | 99.616 | 107.521 | 209.298 | 0.0 | 0.0 |
| 1 | 87.444 | 97.126 | 104.065 | 207.424 | 0.0 | 1.9 |
| 100 | 98.596 | 99.234 | 118.708 | 201.509 | 2.9 | 673.7 |
| 1,000 | 116.721 | 113.115 | 196.113 | 242.175 | 25.1 | 6,906.3 |

- 0/1/100 bot 三档的均值被 `HostAoiWalkFanout` 拉高且受走位碰撞随机性影响，p50 比均值更稳；
  1,000 bot 档 p90 明显抬高（扫描 tick 与走位者数叠加）。
- 1 bot 时仍只有 1.9 帧/tick（每 5 tick 一次首扫），说明 bot 的 AOI 出现帧按 1000ms 节流
  正常工作。

### 3.2 子项分解（ms/tick 与占宿主段 %）

| 子项 | 1 bot | 100 bot | 1,000 bot |
| --- | --- | --- | --- |
| `HostAoiMonsterWalk` | 86.457 (98.87%) | 97.461 (98.85%) | 113.461 (97.21%) |
| ├ `HostAoiWalkFanout` | **84.370 (96.49%)** | **94.976 (96.33%)** | **107.762 (92.32%)** |
| └ 走位扫描+帧构造（残差） | 2.087 (2.39%) | 2.484 (2.52%) | 5.699 (4.88%) |
| `HostAoiStamina` | 0.980 (1.12%) | 1.063 (1.08%) | 2.481 (2.13%) |
| `HostAoiAppear` | 0.002 (0.00%) | 0.060 (0.06%) | 0.590 (0.51%) |
| ├ `HostAoiAppearSelect` | 0.000 | 0.006 | 0.153 |
| ├ `HostAoiAppearBuild` | 0.000 | 0.041 | 0.328 |
| └ `HostAoiAppearEnqueue` | 0.000 | 0.003 | 0.021 |
| `HostAoiEvent` | 0.001 | 0.007 | 0.173 (0.15%) |
| `HostAoiDisappear` | 0.001 | 0.001 | 0.010 (0.01%) |
| ├ `HostAoiLeaveKind` | 0.000 | 0.000 | 0.001 |
| ├ `HostAoiLeaveBuild` | 0.000 | 0.000 | 0.001 |
| └ `HostAoiLeaveEnqueue` | 0.000 | 0.000 | 0.004 |
| `HostAoiKillExp` / `NpcTurn` / `NpcHit` / `MerchantLeave` / `LoginTail` | 0.000–0.002 | 同左 | 同左 |
| 走位者数/tick（`WalkFanout` 调用数） | 230.8 | 254.7 | 263.4 |
| **扇出单价（µs/走位者）** | **365.6** | **372.9** | **409.1** |

关键读数：

- 扇出单价在 100 bot 与 1,000 bot 间基本不变（373→409 µs），**随实体内数而不是玩家数增长**：
  它是「每走位者一次 O(全实体) 扫描」的直接代价。玩家数只通过「收件人过滤后仍要排序的少量 id」
  轻微影响。
- 玩家数从 1 增到 1,000（×1000）只把 `HostAoiAppear` 从 0.002 拉到 0.590 ms（仍然 <0.6%）；
  说明在本场景下**玩家 AOI 下行不是瓶颈**。

## 4. 7,908 实体单点（T10b 同规模）

| 项 | ms/tick | 占宿主段 |
| --- | --- | --- |
| 宿主段均值（`on`） | **45.791**（p50 50.047 / p90 54.464 / max 104.889） | 100% |
| `HostAoiMonsterWalk` | 45.075 | 98.44% |
| └ `HostAoiWalkFanout` | **43.736** | **95.51%** |
| └ 走位扫描+帧构造（残差） | 1.339 | 2.92% |
| `HostAoiStamina` | 0.711 | 1.55% |
| 其余全部子项合计 | 0.002 | 0.01% |
| 走位者数/tick | 165.9 | — |
| 扇出单价 | **263.6 µs/走位者** | — |

hides/tick = 0、下行帧/tick = 0（0 会话时玩家路径不产帧），与 T10b 窗口 C 的
「在世界会话 0」条件一致。

## 5. 关闭态零开销

### 5.1 代码级

- 门控：`MIR2_TICK_PROFILE` 只有字面 `"1"` 开启（沿用 T10b，未改）。
- 关闭时 `enabled == false`：
  - `probe_begin(&prof)` → `begin()`：一次 `bool` 判别 ⇒ `None`，**不取 `Instant`**；
  - `probe_end(&mut prof, ..)` → `probe(None)`：一次 `Option` 判别 ⇒ `return`，
    **不写 `rows`/`probes`、不分配、不查表**。
- 每 tick 的判别次数 = 桶级 9 个子探针 × 2 = 18 次，加上按迭代次数触发的：
  每个 scan 观察者 2 次（Select）、每个出现目标 4 次（Build+Enqueue）、
  每条 hide 4 次（Kind+Build+Enqueue）、每个走位者 2 次（Fanout）。
  这些判别**不改变任何循环/迭代/帧面**：没有探针时同样的分支仍要判
  （例如 `walk_seen`/`online`/`displays` 门、`route_to_gate` 结果），本批只多一次
  `Option::is_none`。
- 探针**不进入** `tick_sum_ns`，故 `HostAoiDownlink` 的父段账与 `HostUnaccounted` 逐字节不变。

### 5.2 同进程 A/B（三臂）

| 档 | `none` 均值 | `off` 均值 | `on` 均值 | `off vs none` | `on vs off` |
| --- | --- | --- | --- | --- | --- |
| 7,908 / 0 | 45.877 | 45.892 | 45.791 | +0.03% | −0.22% |
| 11,000 / 0 | 87.084 | 86.848 | 89.905 | −0.27% | +3.52% |
| 11,000 / 1 | 87.686 | 88.253 | 87.444 | +0.65% | −0.92% |
| 11,000 / 100 | 97.719 | 98.608 | 98.596 | +0.91% | −0.01% |
| 11,000 / 1,000 | 115.577 | 116.865 | 116.721 | +1.11% | −0.12% |

- `off vs none` 全在噪声带内（最大 +1.11%，且绝对差 <1.3 ms）⇒ **关闭态接线没有可测开销**。
- `on vs off` 四档在 ±1.1%；11k/0 档 +3.52% 是**单档噪声**（同档 `on` 也比 `none` 高 3.2%，
  而其它四档方向不一致）——该档绝对差 3.1 ms，不足以用「231 走位者 × 2 次 `Instant` ≈12 µs」
  解释，判为运行间漂移。其余档说明计时扰动可忽略，`on` 的份额可直接与父段相减。

## 6. 为什么本批不起真栈

真栈路线（T10b §12.4）的代价：dev profile（比 release 慢 ~25×）、MonGen 自然涨到 7.9k 需约
36 分钟、bot 数不可控（登录/小退链路）、100/1000 bot 需要大量客户端与全机窗口。而本 harness
**直接调用生产函数** `drain_run_downlinks_with_magic`，世界状态来自**真实的 `World` tick**
（可见集、hides、走位都是真实算法产出），差异只在「玩家布置/走位注入」两处（§2 已登记）。

因此本批**不需要**真栈窗口。**建议**在优化卡落地后用一次真栈 60s 窗口做验证：新探针会把
真实走位者数（`HostAoiWalkFanout` 的 calls/tick）与真实子项 ms 直接打进 `[TICKPROF]` 输出，
一次窗口即可校准本报告的注入率假设。

## 7. 与 T10b / 容量模型对账

- T10b 窗口 C（7,914 实体、`在世界会话 0`）：`HostAoiDownlink` = **567.42 ms**（dev）。
- 容量模型的构建口径系数 25×（World 侧验证 dev 144.0 µs/实体 vs release 5.82 µs/实体）
  ⇒ 该段 release 约 **22.7 ms**。
- 本报告 7,908 实体的扇出单价 **264 µs/走位者** ⇒ 反推真栈走位者 ≈ **86 个/tick**
  （≈1.1% 实体/tick），与「大部分怪不在每 tick 走、只有 AI 命中走位支才动」一致。
- 交叉校验：22.7 ms ÷ 7,914 = **2.9 µs/实体**，与容量模型 §4.3 的「AOI 占 `hostΣ` 86%」
  推出 3.5×0.86 ≈ **3.0 µs/实体**几乎相等 ⇒ **归因成立**：T10b 的 567 ms 主体就是走位扇出。
- 容量模型的 `hostΣ` 7.9k = 692 ms（dev）≈27.7 ms release；本报告 7.9k/0 宿主段 45.8 ms
  比它高 ~1.65× —— 与「本 harness 注入走位率 ≈ 真栈 2 倍」同向，份额不受影响。

## 8. 下一步优化候选（只写建议，本批未改任何优化逻辑）

| # | 候选 | 目标子项（本场景占比） | 做法（行为等价） | 预期收益 | 风险 |
| --- | --- | --- | --- | --- | --- |
| A | **走位扇出批量化** | `HostAoiWalkFanout`（92.3–96.7%） | 每 tick 只扫一次实体，产出「同图、`race==PLAY`、`!ghost`」的坐标索引（或直接复用 `maps` 的格表）；每个走位者只查 ±`SendRefMsgRange` 的索引桶。收件人集合与排序口径不变 | 把 O(W×N) 降到 O(N + W×k)。本报告 11k 档：108 ms → 约 1–2 ms；桶整体 117 ms → **≈9 ms**（余下是扫描/体力/出现） | 收件人集合必须与 `hear_targets_at` 逐项一致（含 `ob_mode`/`fixed_hide_mode` 退化、`out.sort_unstable` 次序）；需要 golden/真机抽样 |
| B | `drain_close_health_residual` 门控 | `HostAoiStamina`（1.1–2.1%） | 维护「有 ShowHp 实体」计数/名单；为 0 时跳过全实体 `close_health_scan_ids` | 0.7–2.4 ms/tick | ShowHp 的置位/清零点必须全覆盖（已有 `drain_close_health_for` 门） |
| C | 在线表按名索引 | `HostAoiAppearSelect`（0.13% @1k bot） | `online.iter().find(|e| e.chr_name == ...)` 改成 `HashMap<&str, idx>`（每 tick 或增删时维护） | 1,000 bot 0.15 ms；外推 10k 玩家 ≈15 ms | 插入/删除/改名必须同步；`online` 是 `Vec` 且顺序被别的路径用 |
| D | 走位扫描瘦身 | 走位扫描+帧构造（2.3–4.8%） | 复用 C 的玩家索引替代 `online.iter().any`；`ids_sorted()` 的排序可缓存（槽位序 + 稳定排序只在结构变更时重算）；`seen` 用代际数组替代 HashMap | 1.3–5.6 ms/tick | 确定性契约：`ids_sorted` 是「不让 ECS 存储序参与帧叙」的硬约束，替换必须保持逐 id 次序 |
| E | hide 类别查找去 O(H²) | `HostAoiLeaveKind`（当前 0.001 ms） | `hide_observer_kinds` 与 `hides` **同序同长**（`output_aoi_system` 一处 push）⇒ 用下标/打包结构替代逐条线性 `find_map` | 当前噪声级；**10k 移动玩家**时 H 可能上万，届时是 10⁸ 级比较 | 同序前提要写进注释并被测试钉住 |
| F | 地图事件下行 | `HostAoiEvent`（0.15% @1k bot） | 非当前瓶颈，不建议现在动 | — | — |

**容量含义（推算，非实测）**：按容量模型 `hostΣ` 的 86% 是 `HostAoiDownlink`、本报告其中
92–97% 是扇出 ⇒ 只做 A 就能拿掉 `hostΣ` 的约 **79–83%**。

- 基准 39.5k：端到端 187 ms → **≈73–78 ms（36–39% 预算）**；
- 内容上限 53.5k：254 ms → **≈98–106 ms（49–53% 预算）**，正好卡在 50% 线附近；
  要稳定进 50% 仍建议叠加 World 侧或 B–D 项。

（推算链条：`hostΣ_7.9k` 692 ms dev × 25⁻¹ ≈27.7 ms；AOI 占 86% ≈23.8 ms；扇出占 95% ≈22.6 ms。
按每实体线性外推到 39.5k/53.5k。）

## 9. 边界（本报告**没有**证明什么）

1. **绝对 ms 不是容量模型**：走位注入率是假设值（真栈约 1.1%/tick 是反推），绝对 ms 应按
   走位者数线性缩放；**份额**与**单价**才是可迁移结论。
2. 玩家位置**静态**：真栈玩家会移动、登录期有成批出现帧；本报告的出现帧量级是稳态下界。
3. 单图、无守卫/宝宝（`race=80` 只有 MonGen 怪）⇒ **hide 路径被低估**（守卫/宝宝自扫会产 hide）；
   `HostAoiLeaveKind` 的当前读数不能代表 10k 玩家场景。
4. **不含 socket 写与背压**：`SimToGate` 是 unbounded 通道，本报告量的是「帧生成 + 入队」，
   与 T10b §12.6 同一刀口。
5. 最多 1,000 bot（卡面三档）；10k 玩家的出现/选择成本是外推，未实测。
6. 单机单进程；缓存局部性、NUMA、GC（无）未涉及。

## 10. 复现与证据

```text
# 构建（独立 target，避免污染其他车道）
cd mir2-rs
CARGO_TARGET_DIR=E:/UserTemp/w84-rel cargo test --release -p game-svr --lib --no-run --offline

# 测量（三臂 × 5 档，约 68s）
MIR2_HOST_AOI_BENCH=1 CARGO_TARGET_DIR=E:/UserTemp/w84-rel \
  cargo test --release -p game-svr --lib --offline -- --ignored --nocapture host_aoi_bench \
  > E:/UserTemp/w84-measure/bench-final.txt 2>&1
```

- 原始输出：`E:/UserTemp/w84-measure/bench-final.txt`（sha256 见卡面 patch 条目）。
- 本次窗口内另有 `bench-run1.txt`（校准前：未置 `ai.dir`）、`bench-run3.txt`（校准后、定稿前）；
  **只有 `bench-final.txt` 是交付读数**，其余仅作调参留痕。
- 探针单元测试：`tick_profile::tests`（7 条），其中
  `disabled_probe_does_no_work` 钉关闭态不写 `probes`/`tick_sum_ns`、
  `probe_does_not_change_residual_and_window_takes_probes` 钉子探针不进残差。

## 11. 优化后复测（卡 `openmir2-host-aoi-fanout-index`）

> 本节由后续卡追加：§0–§10 是**优化前**的测量，本节是**改后同 harness 复测**。
> 测量时段 2026-11-14 14:36–14:38（+0800），release，`CARGO_TARGET_DIR=E:/UserTemp/w84-rel`，
> 命令与 §2 完全相同。原始输出：`E:/UserTemp/w84-measure/bench-fanout.txt`（run1）与
> `bench-fanout-run2.txt`（run2）。

### 11.1 改了什么（语义不变）

`drain_monster_walk_downlink` 拆成两阶段：

1. 仍按 `ids_sorted` 扫一遍实体（走位检测、`seen` 游标、告警全部原样），**顺带**收集
   `race == ACTOR_RACE_PLAY` 实体的 `(id, map_id, x, y, ghost)` 候选（`WalkFanoutCandidate`）；
2. 再对每个走位者只在这份候选上按 `chat::hear_targets_at` 的**原判据**重判
   「同图 / ±`SendRefMsgRange` / `!ghost`」；说话者 `ObMode || FixedHideMode` ⇒ 只发自身的
   支路原样保留。帧的**相对次序**（走位者按 id 升序、收件人按 id 升序）与投递代码
   （`send_ref_msg_to_targets`，与全扫描版共用同一份）都没有变。

候选集**没有用地图格子表**：① `enter_map` 在墙格/越界格上保留角色但**不入格表**；
② `MoveToMovingObject` 可能把别的 `CellObjId` 追加到目标格（`mto_quirk_*`）。
两条都会让「格表里的 actor」不等于「`hear_targets_at` 扫得到的 actor」，用它取候选会改变
收件人集合。`chat.rs` 的 `hear_targets*` 未改（其它发点仍走全扫描）。

### 11.2 门槛（A3）

| 档 | 改前 Fanout | 改后 run1 / run2 | 倍数 |
| --- | --- | --- | --- |
| 7,908 / 0 | 43.736 | 0.018 / 0.019 | ~2,300–2,400× |
| 11,000 / 0 | 86.902 | 0.024 / 0.026 | ~3,300–3,600× |
| 11,000 / 1 | 84.370 | 0.024 / 0.025 | ~3,400–3,500× |
| 11,000 / 100 | 94.976 | 0.096 / 0.090 | ~990–1,050× |
| **11,000 / 1,000** | **107.762** | **1.358 / 1.521** | **71–79×** |

- 11k/1,000 bot：**1.36–1.52 ms/tick < 10 ms**（门槛留 ≥6.5× 余量）。
- 1 bot 档：0.024–0.025 ms/tick（改前 84.370）⇒ 不退化。
- 单价：0.10–0.11 µs/走位者（0–1 bot，候选集为空）、0.36–0.38（100 bot）、5.2–5.8（1,000 bot）。
  1,000 bot 档的单价来自「每个走位者线性过滤 1,000 个候选」，与实体总数脱钩。

### 11.3 其它子项（ms/tick，`on` 臂）：没有反向变慢

| 子项 | 改前 1 / 100 / 1,000 | 改后 run1 1/100/1,000 | 改后 run2 1/100/1,000 |
| --- | --- | --- | --- |
| `HostAoiStamina` | 0.980 / 1.063 / 2.481 | 0.992 / 1.098 / 2.334 | 0.987 / 1.023 / 2.468 |
| `HostAoiAppear` | 0.002 / 0.060 / 0.590 | 0.001 / 0.057 / 0.628 | 0.001 / 0.056 / 0.602 |
| ├ `HostAoiAppearSelect` | 0.000 / 0.006 / 0.153 | 0.000 / 0.006 / 0.171 | 0.000 / 0.005 / 0.160 |
| ├ `HostAoiAppearBuild` | 0.000 / 0.041 / 0.328 | 0.000 / 0.039 / 0.343 | 0.000 / 0.039 / 0.331 |
| └ `HostAoiAppearEnqueue` | 0.000 / 0.003 / 0.021 | 0.000 / 0.002 / 0.023 | 0.000 / 0.002 / 0.023 |
| `HostAoiEvent` | 0.001 / 0.007 / 0.173 | 0.000 / 0.007 / 0.178 | 0.000 / 0.007 / 0.175 |
| 走位扫描+帧构造（残差） | 2.087 / 2.484 / 5.699 | 1.511 / 2.002 / 5.249 | 1.511 / 1.855 / 5.562 |

- `Appear` / `Event` 的分支代码**未被本卡触碰**；上表差异（≤0.04 ms）在**同进程三臂噪声带内**
  ——同一批运行里 `off vs none` 的均值差是 +0.24…+6.72%（run1）与 −1.64…+3.18%（run2）。
- 走位扫描+帧构造（残差）反而略降（5.699 → 5.25–5.56）：同一段扫描不再夹着每走位者的
  全实体遍历，缓存压力更小。

### 11.4 端到端等价信号（A1/A2 的规模面）

同档、同种子下五档的 `frames/tick` 与 `hides/tick` 与 §3 逐位相同：
`0.0/0.0`、`0.0/0.0`、`1.9/0.0`、`673.7/2.9`、`6906.3/25.1`；走位者数（Fanout 调用数）
`165.9 / 230.8 / 230.8 / 254.7 / 263.4` 也逐位相同 ⇒ 帧面（集合 + 数量）在没有逐帧比对
的规模下没有可观测变化。

### 11.5 等价性单元测试（改后新增）

| 用例 | 钉住什么 |
| --- | --- |
| `walk_fanout_targets_match_hear_targets_on_boundaries` | 候选集版 vs **未改动的** `chat::hear_targets` 逐元素相同：恰好 12 格（x/y/对角）、13 格（正负两侧）、跨图、死亡（仍收）、Ghost（不收）、非 Play 阵营（不收）、说话者 `ob_mode`/`fixed_hide_mode`（只发自己）、未知说话者、range 12/1/0 |
| `walk_fanout_candidates_cover_every_play_entity_even_if_unmapped` | 候选集 == `iter_ref()` 里全部 `Race==Play` 实体；含**墙格未落格玩家**（格子表索引会漏掉它）与 Ghost 玩家；怪物不入候选 |
| `monster_walk_frames_match_full_scan_reference` | 两个走位者同 tick：`drain_monster_walk_downlink` 的 `(socket, 走位者)` 帧序列与「全扫描 `chat::hear_targets` + 同一投递段」逐条相同；同一走位者帧体逐字节相同 |

既有走位用例（`monster_walk_emits_sm_walk_with_oracle_fields`、
`monster_walk_recipients_exclude_self_and_out_of_range_and_players`、
`monster_walk_one_frame_per_step_and_never_for_push_or_teleport`）不改一行、仍全绿。

## 12. 残留物清单

- `E:/UserTemp/w84-target`（debug 五门用）、`E:/UserTemp/w84-rel`（release 测量用）：本线专用
  target 目录，**保留**以便复核；不属于仓库内容。
- `E:/UserTemp/w84-measure/`：测量原始输出（`bench-run1/run3/final.txt`），保留作为证据。
- 无真栈进程、无 bot 进程、无临时数据库；未起栈，`E:/MirServer` 端口状态未触碰。
- 代码残留：仅新增的探针/harness/本报告；无调试打印进 stdout（探针走 `glog!("DBG", ..)`）。
