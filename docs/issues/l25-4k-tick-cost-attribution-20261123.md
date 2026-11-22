# 4k 臂 `work=169.47ms` 的段成本归因（代码侧只读定因，2026-11-23）

> **状态**：只读定因（不改产品代码、未起栈）。行号对提交 **`3ac846e0`**（= 4k 臂二进制 `ec5b7b18…` 的时代）。
> **产出**：逐段回答"这 ms 花在哪、有没有结构性浪费、能否在不改语义的前提下降"；**不下"该优化谁"的判断**。
> 证据分**【已核】**（读代码确认）/**【推断】**（需测）。

## 0. 背景限定（复述）

- 臂：`4k-single-heap8-20261123-0413`，**1 区、单线 7200、420s**、`-BotHeapLimitGB 8`、`-ActionProbeMs 1000`。
- 二进制 `ec5b7b18…` = 提交 `3ac846e0` 时代；`in_world` 最高 **3739/4000（93.47%），且到开跑后 ~501s 才达** ⇒ **无 4000 满员稳态**。本表"稳态" = 最后 4 个 50-tick 窗口（193 tick，≈3739）。
- 1k 对照是**旧二进制、120s**（不可与 4k 直接等口径比趋势）。
- 段值来源：`E:/UserTemp/l25-slice/aoi-4k-decompose-20261123.md` 的产品提取（`[TICKPROF] 窗口 N tick` / `[TICKPROF] SLOW`）。

## 1. 逐段表

> 记号：n = `in_world`；E = 世界内实体数（玩家+怪+NPC）。"结构性浪费"= 同一便宜事被重复做/在无需时也做。

| 段 (ms/tick, 占 work) | 执行点（`3ac846e0` 行号） | 复杂度阶 | 结构性浪费? | 只读结论 | 等级 |
|---|---|---|---|---|---|
| **HostPlayerRun 45.72 (27.0%)** | `sim_loop` 内 `host_prof` 段 `HostPlayerRun`（`begin()` → `end("HostPlayerRun")` 之间）；调用 8 个 per-player：`drain_player_need_recheck_with_magic`(29035) / `game_gold`(29771) / `map_hp`(29354) / `hunger`(29420) / `run_rates`(29524) / `auto_get_exp`(29686) / `revival`(29970) / `flush_run_frames`(29589) | 每个 drain = `sessions.keys().cloned().collect()+sort` 再 `for name { 到期门; find_by_chr(name) }` ⇒ **O(n log n)+O(n)，×7 个 drain** | **是**：同一 `ground_sessions` 每 tick 被 **7 个 drain 各 collect+sort 一遍**（7×O(n log n) 排序 + 7n 次 String 分配），per-name 体多被"到期门"挡掉 | 可改为**每 tick 一次** collect+sort（或按注册序迭代）后**一趟**跑完全部 per-name 逻辑；顺序（按名序确定性）不变。**降幅需测**。 | 已核（7 drain 均含 collect+sort）；降幅推断 |
| **HostAoiDownlink 33.49 (19.8%)** | `drain_run_downlinks_with_magic` `23277`（段 `36920-36951`） | 见四子项 | MonsterWalk 否；Stamina 是又一处 collect+sort+loop | 见下 | — |
| ├ HostAoiMonsterWalk 17.64 | `drain_monster_walk_downlink` `20992`（`collect_monster_walk_batch` + 每走者 `broadcast_ref_msg_from_walk_candidates`） | **O(走位怪数 × 收件人集)** | 否（已两阶段；注释明写原"每走者全实体 `hear_targets`"已优化） | 高是因 **4k 同图怪多、每 tick 走位者多 × 观察者扇出** = 真实出帧成本。"每小时每怪帧量"未记 ⇒ 需测。 | 已核 |
| ├ HostAoiStamina 6.39 | `drain_stamina` `22988` | **每会话全量扫 O(n log n)+O(n)** | **是**（每会话每 tick 全量） | 与 PlayerRun 同款 collect+sort+loop；可与 PlayerRun 的扫描合并。 | 已核 |
| ├ HostAoiAppear 4.66 | `drain_aoi_downlink` `20700`（select 1.15 + build 1.86） | `for sess`（search_tick 门 ~n/5）读 `visible_actors` 出帧 O(n/5×visible) | 否（读 OutputAoi 已算的 visible_actors，不重扫空间） | 真实出帧成本。 | 已核 |
| └ HostAoiLoginTail 3.70 | `drain_login_tail` `21444`（处理 `login_tails` 的 pending） | O(pending) | 否 | 本臂 `in_world` 仅 3739、末段仍有 ~261 登录未完成 ⇒ **登录尾部/爬坡，非满员稳态**。 | 已核 |
| **World.OutputAoi 25.96 (15.3%)** | `crates/world-bevy/src/world.rs:2244 output_aoi_stage_system`（`tick_phase2`，HostWorldTail 内） | **O(in_world/entities × view-range 格扫描)** + 事件扫描 | 否（空间扫描只此一次） | 与 HostAoiDownlink 是 **"一个算（填 `visible_actors`/`hides`）、一个发（读 `visible_actors` 出帧）"**，Host 侧不重扫空间，**非重复计算**；随 in_world 上升（1k 4.18 → 4k 25.96）。降本属 AOI 算法/增量级，非顺手。 | 已核 |
| **HostWorldHead 18.57 + Tail 29.53 = 48.09 (28%)** | Head = `tick_world_head_with_specials` `30300`（sim `36502-36519`）；Tail = `tick_tail_before_aoi`+`tick_phase2`（`36673-36753`） | World sim 段；`SimMonsterAi` 逐槽序 O(entities) | 否 | World 探针合计 47.85 = OutputAoi 25.96 + SimMonsterAi 9.06 + SimDup 2.35 + TailVerifyMapTime 1.43 + TailPlayerDeaths 1.29 + SimSettleDeaths 1.11 + SimNpc 1.05 + SimMerchant 1.05（=43.30）+ **未命名 ~4.55**。**更正**："剩余 10.7ms"= 48.09 −(25.96+9.06+2.35) = 10.72，其中 **5.93 是那 5 个小命名探针**、**~4.79 是未命名 sim 段**（Command FIFO / SimBuff / TailSuperManRefill / clock 等），**不是"去向不明"**。`SimMonsterAi` [推断] 线性于在场怪物数，**产物未记怪物数 ⇒ 需测**。 | 已核（探针构成）；SimMonsterAi 线性=推断 |
| **HostDiag 7.30 (4.3%)** | `sim_loop` 中 `ticks.is_multiple_of(50)` 分支调用 `world_aoi_diag(...)` 并 `glog!(DBG,…)`（符号锚：`sim_loop` + `world_aoi_diag`） | **O(n × E)**：先 `entities.iter_ref().collect()`（O(E) 分配），再**每会话 `for o in &entities` 全扫**算最近可见 | **是，纯诊断**（构造 `SessionAoiDiag` 供一行 DBG；无帧/无状态） | **每 50 tick 一次 ⇒ 7.30 是摊薄值，单次≈365ms 尖峰（每 ~10s）**，直接抬 p99 / 造成 201ms 溢出桶。**生产可关且语义零风险**：给 `ticks%50` 分支加运行期开关（env，默认关）或 `cfg!(debug_assertions)` 裁掉；代价 = 丢这行 DBG 摘要。**各段里最干净的降点（可降到 ~0：省摊薄 4.3% + 消除 365ms 周期尖峰）。** | 已核 |
| **HostPeriodic 9.90 (5.8%)** | `36177-36502`：`castle_gates.tick`(36213，10s/10min)、行会块(36239-36255)、`drain_depart_downlink`(36392)、`mon_gen`(36396)、IDS serverinfo(36202) | 城堡/行会 step 内部 10s/10min 门 | **是**：`guild_ban`(36239 `ground_sessions.iter().map(...).collect()` **O(n) 分配**) + `guild_recipients`(36243) **每 tick 无条件构建**，而 `guild_runtime::step` 才 10s gate | 把 guild_ban/recipients 挪进门内/缓存（同一 10s tick 仍用当 tick 快照，语义不变；需确认 gate 可外提）。`depart`/`mon_gen` 每 tick。 | 已核 |
| **HostPersist 4.52 (2.7%)** | `36952-36973`；`sync_use_items_to_session` 每 tick `for sg in ground_sessions.values_mut()`(36957)；`drain_periodic_save`(23460) | sync O(n)/tick；periodic_save **每 tick `keys().cloned().collect()+sort`(23475-76)** 后逐名查 due | **是**：collect+sort 每 tick 做，而实际存档按 `SaveHumanRcdTime`（默认 600s）才一次 | 可由"全局下一次到期"门在非到期 tick **整段跳过**（含 collect+sort），语义不变。 | 已核（collect+sort 每 tick）；降幅推断 |
| **HostDbPoll 4.35 (2.6%)** | `35680-36176`：`engine.drain_read_storage`(35687) + 应答表 take 循环；`drain_zone_saves`(35672，1 区 `zone_host=None` 不触发) | 每 tick；O(outstanding/inflight) | 否（证据不足） | 本臂末段仍有 ~261 登录未完成 ⇒ load list 非空、每 tick 处理登录残余 ⇒ **4.35 属爬坡/登录残余，非满员稳态**；[推断] 满员后应显著降，需测。 | 推断 |

## 2. 一句话结论（有没有"3 倍级"可降空间）

**没有任何一段存在可断言的"3 倍级"降空间**；唯一**语义零风险、可降到 ~0** 的是 **`HostDiag`**（诊断 O(n×E)、摊薄 4.3% + 365ms 周期尖峰）。其余是"重复做同一件便宜事"的结构性浪费——**PlayerRun 7× collect+sort、HostPersist 每 tick collect+sort、HostPeriodic 每 tick guild_ban/recipients**——**降幅需测，不下 3x 结论**。

## 3. 跨段注记（非本表段，供判读留意）

`HostInbound` 1k 19.79 → 2k 44.02 → **4k 5.68** 反常下降。**未深挖**；疑与 4k 帧长 169ms 下 bot 到达/事件预算/爬坡时机有关，读 4k 号时需带此不确定性。

## 4. 关联卡（仅记数、待 HostDiag 落地复测后再排优先级；均"降幅未知、须先测"）

- `player-run-seven-collect-sorts`（PlayerRun 7 个 drain 各 collect+sort 一遍 → 合并成一次、按名序不变）。
- `persist-collect-sort-every-tick`（HostPersist 每 tick collect+sort 而存档 600s 一次 → 提前门）。
- `periodic-guild-unconditional`（HostPeriodic 的 `guild_ban`/`guild_recipients` 每 tick 无条件构建而 step 有 10s 门）。

> 三张卡与 `HostDiag`（另卡/另改）不实施排序，等 `HostDiag` 落地并复测、确认 p99 真实形状后再定。
