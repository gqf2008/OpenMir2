# L2-5 后续：每区完整 host 循环——可行路径 / 工作量 / 分期（2026-11-23）

> **状态**：评估稿（只写文档，不写代码、不编译、不合入）。
> **座标系**：所有 `lib.rs:NNNN` 指本 worktree `card/l25-zone-loop-scope`，
> 基线 **`03b103c25`**（= 已推 `master`）= HEAD。行号只作**证据锚**，不写进契约；
> 定稿后如需在别处引用，用符号名（函数/结构体）而非行号。
> **推理约定**：每处结论标 **【已核】**（读代码/文档确认）或 **【推断】**（由已核事实外推，
> 实施时要复核）。本稿不改 ④b-1 的读数结论、不碰 ⑤（`card/l25-step5`）、不碰 `a7`
> （`zone-mirror-step2`）。

---

## 0. 一句话结论

【已核】今天区 k 只跑 `region_loop::run_zone_tick`（`tick_phase1 → 快照/barrier1/读邻居 →
apply_mirrors → tick_phase2`）＋移动类命令 + 本区 MonGen；它**产出了 `TickRecord` 但没有
任何下行排水**，命令也只认移动 ⇒ 区 k 玩家「会走不会打、不动不出门」。

要让每区跑**完整 host 循环**，本质上不是重写游戏逻辑——`sim_loop` 主循环体（`lib.rs:34094`
起约 2,760 行）里绝大多数的 `handle_*` / `drain_*` 已经是**对 `(world, displays, online,
ground_sessions, gates, 配置)` 的纯函数**——而是**把这段循环体抽取成「每区一个实例」，
把极少数进程级依赖（DB、行会、城堡、脚本宿主 `!Send`）留在区 0**。

**推荐路径 A（进程内每区一个循环实例，按 C 的顺序增量落地）**；
【推断】从中位约 **95 工程日**、乐观约 **60–70**、悲观 **140+**；
最大不确定性 = **跨区帧序等价证明**与**脚本宿主 `!Send` 逼出的「区 0 串行执行」**。

---

## 1. 七问①：`sim_loop` 子系统清单（进程级 / 区级 / 需共享通道）

`fn sim_loop` = `lib.rs:32504–36857`（**启动段** `32504–34093`，**主循环** `34094–36856`）。
主循环已由 `host_prof` 标出段名（`MIR2_TICK_PROFILE=1` 时打印），下表「证据锚」用段名 + 行号。

| # | 子系统 | 现状代码锚（【已核】） | 今天归属 | 每区是否要独立实例 | 说明 / 缺口 |
|---|---|---|---|---|---|
| 1 | 启动装载（World/静态实体/脚本） | 启动段 `32504–34093`；`production_world` `32558`；静态装载 MonGen/商人/固定 NPC/守卫 `33010–33882`；脚本宿主 `npc_host` `33217`、`merchant_book` `33220` | 进程（区 0 线程跑一遍；④b-1 已把「实体」按区装进各区 World，但脚本宿主仍单实例） | 实体：是（已完成）；脚本宿主：**否**（`Rc` ⇒ `!Send`，见 §2/§6） | ④b-1 收官（见计划页 §12.8）。残余：隐藏 NPC、城堡实体、ctor-fold per-map 站点（④b-2） |
| 2 | 登录 / 取档 / 落区 | `GateEvent::LoadDbRequest` `35369`；DB 排水+落区判定 `35587–35805`；`finish_login` `35625` / `finish_login_on_zone_state` `35709`；`decide_login_landing` `35617`；`router.assign` `35775` | **进程编排在区 0，会话落在目标区**（③ 已完成） | 编排：进程；会话状态：区级（已在 `ZoneState`） | 区 k 落区已通；会话装配仍由区 0 线程持锁写入区 k（与下面脚本宿主同一「借用手法」） |
| 3 | 会话 IO / 网关线 | `gate_conn` `36895`（独立 async 任务，进程级）；`gates` 表 `33750`；demux 广播 `RegisterGate` `1892–1949`；区 k 自己的 `gates` 表 `spawn_minimal_zone` `2052–2085` | 网关连接 = **进程级**；每区各持一份 `gates` 发送端表（广播得来） | 是（发送端表已每区一份） | `SimToGate` 的 `SendFrame/KickUser/CloseUser` 经 `gates[line]` 发出；区 k 已有表但绝大多数 drain 没被调 |
| 4 | 入站事件排水 / 命令分发 | `EventDrain`/预算 `30858`（`PRODUCTION` = 2000 条 / 50ms）；`GateEvent::Command` 大 if 链 `34224–35178`；`handle_inbound_cmd` `5904`；区 k：`handle_zone_command` `1441`（**只认移动**） | 区 0：全命令；区 k：仅移动（`movewire::is_move_command`） | 是 | **核心缺口**：`handle_hit_cmd`/`handle_magic_key_cmd`/`handle_spell_cmd_with_magic`/物品/商店/交易/组队/说话等一律只在区 0 分发 |
| 5 | 移动命令 | `handle_move_cmd` `14348`（区 0 调用 `34966`；区 k 复用 `handle_zone_command` `1441`） | **区 0 与区 k 共用同一函数**（④a 已完成） | 是（已具备） | 证明「同一 handler 在区 k 复用」路线可行；其余 handler 照此扩 |
| 6 | 战斗（攻击）| `handle_hit_cmd` 调用 `35035`；world 段 `tick_world_head_with_specials` `36324`；`drain_combat_downlink` `22219` | 区 0 | 是 | 区 k 有 World tick（含怪物 AI/受击），但**攻击命令无人分发**、`TickRecord` 无人排水 |
| 7 | 技能 / 术法 | `handle_magic_key_cmd` `35054`；`handle_spell_cmd_with_magic` `35057`；魔法下行 `drain_mag_struck_b2` `15750`、`drain_b3_self_deliveries` `36708`、`drain_b4_with_enter_cfg` `36699`、`drain_b5_self_deliveries` `36710` | 区 0 | 是 | 命令与 drain 都需在区 k 运行；魔法表 `magic_table_arc`（`32627`）是进程级只读 `Arc`，可借 |
| 8 | 物品（背包/地面/吃/存/商店/交易/寄售） | `handle_ground_cmd` `34497`、`handle_deal_cmd` `34529`、`handle_selloff_cmd` `34556`、`handle_bag_cmd_with_magic` `34588`、`handle_eat_cmd` `34636`、`handle_query_bag_cmd` `34665`、`handle_shop_cmd` `34911`（`fn handle_shop_cmd_inner`）、`handle_storage_cmd` `10305` | 区 0 | 是（多数）；**商店/点击 NPC 例外**（见 9） | 背包/地面/吃/存/查询纯区级；`handle_bag_cmd` 等的 `sell_off_book` 需是进程级或分区（见 §2） |
| 9 | NPC / 商人 | world tick 内 `npc_tick`/`merchant_tick`（`crates/world-bevy/src/world.rs` 段系统，缺省由宿主 `enable_*`）；`drain_npc_merchant_turns` `21208`、`hits` `21259`、`merchant_disappears` `21339`；交互：`handle_click_npc_cmd`（签名带 `merchant_book`/`npc_vars`）、`handle_shop_cmd`（带 `merchant_book`） | tick：每区各自的 World（已能跑）；**脚本交互：进程级**（`merchant_book`/`npc_host` 含 `Rc` ⇒ `!Send`） | tick：是；交互：**需共享通道/区 0 执行** | 这是「区 k 玩家可正常游戏」最难的一环：NPC 说话/商店脚本必须回到拥有 `merchant_book` 的线程（区 0）执行，再持锁写回区 k 的 World |
| 10 | AOI 与帧 flush | `tick_phase2`（`world.rs:9277`）；`drain_run_downlinks_with_magic` `36736`；`flush_run_frames` `36480`；`drain_action_downlink` `36641`、`drain_buff_downlink` `36644` 等约 30 个 `drain_*` | 区 0 | 是 | zone k 的 `run_zone_tick` **调用 `tick_phase2` 但丢弃 `TickRecord`** ⇒ AOI 算出可见集却没有任何 drain 发帧。这是「区 k 不出网关帧」的直接原因 |
| 11 | 聊天 | `handle_say_with_saves` `34289` | 区 0 | 是（扇出可能跨区 ⇒ 需 ⑤ 通道） | 本地会话在区中；全服/行会/广播扇出跨区 |
| 12 | 行会 | 启动 `guild_runtime::startup` `32696`；`guild_run_tick` `32703`；10s 门 `36051`；命令 `guild_cmd::handle_guild_cmd` `35087`、`guild_official::handle_select` `34818`；GM `apply_pending_gm_guild_commands` `34458` | **进程级（只在片 0/区 0 装载）** | **否**（单点） | 行会表是进程级服务（文件写/存档）。区 k 的行会命令须路由区 0 或经共享只读视图（见 §5） |
| 13 | 城堡 | 宿主 `CastleHostGates` `32707`、`CastleStore` `32708`、`castle_war_states` `32711`；10s/10min 门 `36028`；`CastleMgr::run` `36072`；GM `apply_pending_gm_castle_commands` `34445` | **进程级（只在片 0/区 0 装载）** | **否**（单点） | 10min 落盘 / 战况 / 税；城堡实体今天只在区 0 World（④b-2 归属） |
| 14 | World tick 仿真本体 | `tick_phase1`（world.rs:9268）= `tick_head`（命令 FIFO/死亡/buff/怪 AI/NPC/商人）+ `tick_tail_before_aoi`（world.rs:9144，SuperMan/玩家死亡/VerifyMapTime）；`tick_phase2`（world.rs:9277）= AOI | **区 k 已在跑** | 是（已完成） | `World` 有 `owner_thread` 断言（`world.rs:9146/9204`）：一个 World 只能在其线程 tick ⇒ 任何「把区 k 的 World 借给别的线程 tick」都会 panic |
| 15 | 存档（周期/断线/关服） | 周期 `drain_periodic_save` `36773`（`fn` `23412`，默认 `SaveHumanRcdTime=10min`，`config.rs:1595`）；区断线 `drain_zone_saves` `1485`（调用 `35487`）；关服 `shutdown_save_all` `34114` | **进程级 DB 链**（`db`/`engine` 属区 0） | 存档**数据**区级，**通道**进程级 | 区 k 已能把断线存档编码进 `save_outbox` 由区 0 发 DB；周期存档需给区 k 也接这条出口 |
| 16 | 定时器 | 城堡 10s/10min `36028`；行会 10s `36051`；滚动公告 `36244`；换服 100ms `process_switch_data` `36793`；MonGen regen（区 k `36211`） | 混：行会/城堡进程级；公告/换服/ MonGen 区级 | 区级定时器：是；进程级：否 | 定时器本身便宜，风险在「全局门」与区级门混淆 |
| 17 | 看门狗 | `tick_watchdog::ZoneWatch`（`ZoneHost.watch` `985`）；`FailFastBarrier`（`region_loop.rs:424`）；区 panic ⇒ `abort` 整机（`spawn_minimal_zone` catch_unwind） | 进程级（守全机的屏障活性） | 复用（每区已参与心跳/屏障） | 已有每区心跳 `heartbeats`；扩循环体后仍受同一看门狗保护 |
| 18 | 计数 / 日志 / 探针 | `perf_counters`（TSV 尾 `write_tick` `36855`）；`tick_profile::HostTickProfile` `34015`；`glog!`；入口三段计数 `telemetry` | 进程级（原子累加；只区 0 打汇总行 `32548`） | 计数原子共享；但**分区诊断**（每区 `af=`/`zone_cmd_unsupported`）需按区分列，否则读数不可引用 | ⑤ 已在扩 `perf_counters`（跨区三桶），本项要避免与其抢列 |
| 19 | 换服 / 跨服 | `process_switch_data` `36793`；`switch_data_send_change_server` `36805`；`drain_reconnects` `36819` | 进程级（G11 大多未移植，生产恒 false） | 否（单点） | 依赖 `server_table`（进程级只读） |
| 20 | 组队 | `handle_party_cmd` `35013`；`world.party_run_tick()` `36713`；`party_cfg`（进程级只读） | 会话表区级；`party_run_tick` 在 World 内 ⇒ 区 k 已有 | 是 | 组队的跨区成员通知需经 ⑤ 通道（与聊天/广播同类） |

**读法**：凡是「区级」的行，今天的函数签名已经把状态都放在 `(world, displays, online,
ground_sessions, gates, 配置)` 里——这正是 ④a 能直接复用 `handle_move_cmd` 的原因。
真正需要**共享通道**的只有：DB（存档）、行会、城堡、脚本宿主（NPC/商店）、跨区广播/帧。

---

## 2. 七问②：每区必须复制的状态清单

「谁拥有」= 今天（基线）在哪个作用域；「跨不跨区」= 该状态是否会被邻区读到/写；
「同步」= 是否需要每 tick 的跨区通道。

| 状态 | 谁拥有（今天，【已核】） | 跨区? | 需同步? | 证据 / 备注 |
|---|---|---|---|---|
| `World`（实体/地图格/命令 FIFO/RNG） | 区级：区 0 是 `sim_loop` 局部 `let mut world`；区 k 是 `ZoneState.world` | 是（镜像/边界） | 是（halo 快照 + mirror） | `ZoneState` `1005`；`World` 有 `owner_thread` 断言 |
| 地图格（`maps`）| **每区各一份全图 grid**：`ZoneState::new` → `World::with_maps(map_sizes.to_vec())` `3910` | 否（只读，应可共享） | 否 | **【推断】内存风险**：N 区 = N 份全图 grid。单图 10k 场景只 1 张图 ⇒ 可接受；多图 N 份需评估（见 §3 失败模式） |
| `displays`（`DisplayTable`）| 区级：区 0 `33095`；区 k `ZoneState.displays` | 是（镜像展示项经快照） | 是（`apply_mirrors` 写收件区展示表） | `downlink.rs:288`；`apply_mirrors` 带 `displays` |
| `online`（`OnlineRegistry`）| 区级：区 0 `33745`；区 k `ZoneState.online` | 是（快照 `route` = 在线登记） | 是（⑤ 用 `HaloEntity::route` 合成跨区收件人） | ⑤ 设计 `apply_mirrors` 写/清 route |
| `ground_sessions` | 区级 | 否（会话只属落区） | 否 | `33969`；`ZoneState.ground_sessions` |
| `saves`（`SavedHuman`）| 区级 | 否 | 否 | `33996`；`ZoneState.saves` |
| `login_tails` / `aoi_frames_total` | 区级 | 否 | 否 | `34043`/`34038`；`ZoneState.login_tails`/`aoi_frames_total` |
| `shop_sessions` / `trade_sessions` / `sell_off_sessions` | 区级（区 k 目前空表，保留共用清理） | 否 | 否 | `ZoneState.shop_sessions`/`sell_off_sessions`；区 0 `33972/33976/33983` |
| `sell_off_book`（元宝寄售全局表）| **进程级**（`M2Share.SellOffItemList` 等价物）`33980` | 是（全服共享） | 需共享/单点 | 若每区各一份会分裂报价；**【推断】应留单点** |
| `gates`（`SimToGate` 发送端表）| 每区一份（demux 广播 `RegisterGate`） | 是（一个网关线服务多区） | 广播（已实现） | `2052–2085`；`spawn_zone_demux` `1892` |
| `perf_counters`（计数器）| **进程级原子**（`32548`）；只区 0 打汇总 | 是 | 共享原子（已有） | 分区读数需加列（与 ⑤ 协调，勿抢列） |
| `RNG`（`world.rng`）| 区级（`seed ⊕ zone`，`ZoneState::new` `1044`）；非 0 区关流水 `32543`/`2037` | 否 | 否 | **有意** per-zone 流（计划页 §12.7-2）；N=1 等价、多区不等价（④d 才解） |
| `MonGenHost` | **每区一份**（④b-1，`ZoneState.mon_gen`） | 否 | 否 | `33031`；区 k `step_mon_gen` `36211` |
| `npc_host` / `merchant_book` / `npc_vars` | **进程级单实例**（`!Send`：`Rc<NpcDef>`） | 是 | **需共享通道** | `33217`/`33220`；④b-1 偏差（计划页 §12.8） |
| `guilds`（行会管理器 + 会表）| **进程级**（只片 0 装载 `32693`） | 是 | 单点 | 文件写/存档 |
| `castle_*`（门/墙/弓手/战况/税）| **进程级**（只片 0 `33101`） | 是 | 单点 | 10min 落盘 |
| `db` / `engine`（DB 链 + 读档引擎）| **进程级**（区 0） | 是（所有区存档） | 单点 + 出口通道 | 区 k `save_outbox` → `drain_zone_saves` `1485` |
| `planes`（G11 跨服）| 进程级 | 是 | 单点 | `32527`；生产基本 inert |
| `win_lottery` | **进程级**（`SystemShare.Config` 等价） | 是 | 单点/共享 | `32531` |
| `shard_runtime` | 片级（多区与多片互斥，区模式下 `shard_count=1`） | 否 | 否 | `32509`/`32566` 断言 |
| `pwd_sessions` / `reconnect_pending` / `kick_pending` / `soft_close_pending` | 进程级（区 0 局部，登录/踢/换服同源借用（`ZoneHost` 注释）） | 是（登录/踢/换服跨区） | 借用/单点 | `33756`/`32645`/`33746`/`33748` |
| `line_notice_states` / `monster_walk_seen` | 区级（区 k 尚无滚动公告路径） | 否 | 否 | `33802`/`34048` |
| `handoff_runtime` | 区级（每区一个邮箱，`spawn_minimal_zone`） | 是 | 跨区邮箱（已实现） | `34086`；`handoff` 模块 |
| `item_number` / `item_number_ex`（MakeIndex 计数器）| **进程级**（`M2Share.GetItemNumber` 等价，`33953/33958`） | 是（跨区新件编号） | **单点/共享** | 区 k 若造新件（商店 `GIVE`）必须共用同一计数器，否则 MakeIndex 撞车 |
| `magic_table_arc` | 进程级只读 `Arc`（`32627`） | 是 | 只读共享 | 每区 clone `Arc` 即可 |

**要点**：区 k 的 `ZoneState` 已经复制了 `world/displays/online/ground_sessions/saves/
login_tails/aoi_frames_total/shop_sessions/sell_off_sessions/save_outbox`（`1005–1024`）。
**缺少的**是「每 tick 去消费它们的那段循环」——也就是 §1 表里的 4/6/7/8/9/10/11 行。

---

## 3. 七问③：三条候选路径对比

> 三条路径的**终点相同**（每区一个能自出帧的完整循环）；差别是**边界放在进程内还是进程间**、
> **搬多少代码**。工程日 = 工程人日（可并行），含实现 + 单测 + 变异牙；不含真机窗口排队。

### 3.1 总表

| 维度 | **A：进程内每区一个循环实例** | **B：每区一个独立进程** | **C：只抽「区级 tick 体」，进程服务留区 0** |
|---|---|---|---|
| 改动面 | 把 `sim_loop` 主循环体抽成 `HostLoop`/`ZoneLoopCtx`，参数化 zone 局部态 + 借用进程服务；启动段参数化 | 新建 IPC 层（halo 快照、帧、handoff、登录路由、DB/行会/城堡归属）+ 进程监督 + 配置 | 先抽「phase1→快照→mirror→phase2→drain 序列」为区级函数；区 0 保留全部服务 |
| N=1 逐字节风险 | 中（必须用 `slicing=0`/N=1 运行期门把新路径整段跳过；风险=阶段序被无意改动） | 低（每进程就是 N=1 原样）+ 高（跨区全新） | 低-中（同 A 的门控，但首步只动 tick 体） |
| 与 ⑤ 冲突面 | **高**（同改 `spawn_minimal_zone`、`sim_loop` 主循环、`run_zone_tick`/`apply_mirrors`、`ZoneHost`、`perf_counters`、`world.rs`） | **高**（⑤ 的进程内邮箱被 IPC 取代） | 中（共享 barrier1 后排水的插入点） |
| 工程日（乐观/悲观） | **60–70 / 140+**（中位 ~95） | **110 / 160+** | **55 / 110**（中位 ~80；但收敛到 A） |
| 失败模式 | ①脚本宿主 `!Send` ⇒ 区 0 串行热点；②`World.owner_thread` 误跨线程 panic；③帧序跨区漂移；④N 份地图 grid 内存；⑤drain 隐藏在进程级/全局序依赖 | ①每 tick 跨进程屏障延迟；②跨进程帧序难保；③单点服务（DB/行会/城堡）归属拆分；④运维/部署复杂度 | ①区 0 成为脚本/全局广播串行点；②命令邮箱往返延迟；③边界在「哪些命令回区 0」上难划 | 
| 回滚代价 | **低**：`slicing=0`/N=1 走原路径，旧路径逐字节保留 | **高**：独立部署/进程拓扑，回滚=退回单进程 | 低（同 A） |

### 3.2 逐条说明

**A（推荐）——进程内每区一个循环实例。**
- 复用已有 `barriers`/`HaloExchange`/`apply_mirrors`/`handoff`/id 段（全部进程内，零 IPC）。
- 与今天「登录在区 0 执行、持锁写区 k」的手法一致（`35709 finish_login_on_zone_state`），
  脚本宿主 `!Send` 不构成阻断，只是逼出「NPC/商店命令回区 0 执行」。
- ⑤ 的跨区邮箱天然可复用：区 k 自己成为发送者后，跨区收件人仍走同一 `CrossZoneSink`。
- N=1 逐字节：所有新路径都在 `slicing`/`N>1` 运行期门后，未开则整段不进入。

**B——每区一个独立进程。**
- 优点：每个进程就是未改动的 N=1 循环，「单区正确性」最稳。
- 代价：把今天进程内的 halo/屏障/handoff/帧全部改成 IPC；单点服务（DB/行会/城堡）跨进程
  归属需要重新设计；每 tick 跨进程屏障在 200ms tick 下可接受但帧序更难证。
- 【推断】本项对本项目不划算：现有 ③④ 全部建在「进程内多线程 + 共享内存屏障」上，
  换 B 等于推翻这些成果的收益前提。

**C——只抽「区级 tick 体」，进程服务留区 0。**
- 最小首步：把 `run_zone_tick` 扩成「phase1 → drain 序列 → 快照/barrier → mirror → phase2 →
  drain」，先把「区 k 出帧」打通（局部收件人），命令仍逐步扩。
- 它是 A 的**第一阶段**；A = C 做完后继续把命令分发/会话生命周期也搬进区实例。
- 单独看 C 有个隐含天花板：只要脚本/全局广播还在区 0 串行，10k 的 NPC 交互/全服广播仍受限；
  但**高频路径（战斗/物品/移动/AOI）已并行**，通常够用。

**推荐组合**：**目标 A，实施按 C 的顺序增量**（先 tick 体出帧，再扩命令，最后并 ⑤）。

---

## 4. 七问④：分期里程碑（每期一个可验收结果 + 验证装置）

> 每期都带「N=1 不回归」硬门：`slicing=0`/N=1 两树 stdout sha 与 RNG 流水前缀必须与基线同一对
> （沿用计划页 §12.8 的 `tools/hygiene/n1_stdout_equivalence.ps1` 口径）。
> R 级判据按卡面；窗口读数按 `capacity-curve-release-20261121.md` 的区间口径（不拿单点当确定值）。

| 里程碑 | 可验收结果（区 k） | 主验证装置 | 依赖 |
|---|---|---|---|
| **M0 骨架** | 把主循环体抽成「每区可实例化」的函数/结构，`slicing=0`/N=1 逐字节不变；区 k 空实例跑通一个 tick 不 panic | N=1 两树 stdout sha + RNG 流水前缀；`tick_phases` 等价用例；单区 `cargo test --workspace` | — |
| **M1 tick 体出帧** | 区 k 的 `TickRecord` 被本区 drain 消费：**本区收件人**能看到 AOI 出现/消失、怪物转身、受伤 53/657 | 两区对拍场景（区 k 放 bot + 怪）：`frames.ndjson` 与 oracle 同格序逐帧比；drain 逐支变异牙 | M0 |
| **M2 战斗/技能命令** | 区 k 玩家能攻击本区怪、施法、看到命中/伤害/死亡/经验帧 | 两区对拍场景（攻击/术法各一），帧字节相同；`passed+failed≥1` 变异 | M1 |
| **M3 物品/地面/商店/交易/组队/说话** | 区 k 玩家能捡/丢/吃/用/交易/组队/说话；**商店与点击 NPC** 经区 0 执行脚本后写回区 k | 逐命令两区对拍；NPC 脚本由区 0 持锁写区 k 的锁序用例；`item_number` 全局唯一用例 | M2 |
| **M4 跨区缝合（与 ⑤）** | 区 k 作为**发送者**：本区产生的帧给邻区收件人经 ⑤ 邮箱在 T+1 送达；区 k 的行会/城堡/DB 命令路由到单点并回帧 | ⑤ 的 `refmsg` 跨区单事件用例扩到「区 k→区 0」；跨区三桶恒等式；锁序/死锁用例 | ⑤（建议先落）+ M1–M3 |
| **M5 容量与收口** | 单图 10k：区 k 玩家可正常游戏；tick p99 回到预算内；无跨区丢帧/重复 | 1k 两区窗口 → 10k 多区窗口（`load_gate`）；`cross_recipient_missing` 不变差；`mirror_shadowed_static==0`；`ΔRSS` 方向 | M4 + 窗口资源 |

**每期的「装置」落地**：单测（进程内合成夹具）、两区对拍（真机真实栈）、1k/10k 窗口读数。
【已核】现有装置可复用：`l2_two_zone.rs`、`n1_stdout_equivalence.ps1`、
`tools/botload/perf_tsv_summary.py`、`region_loop` 单测族。

---

## 5. 七问⑤：必须留在单点的部分 + 1 万时是否成新瓶颈（粗算）

### 5.1 必须单点（为什么）

| 服务 | 为什么必须单点 | 今天落点 |
|---|---|---|
| DB 链（读档/存档）| 单一 DBSvr 连接 + query id 空间 + `save_outstanding` 记账；多写会乱序/撞 qid | `db`/`engine`（区 0）`35501`/`35852` |
| 行会 | 文件写 + 会表是进程级单实例；多区各写会撕裂存档 | `guilds`（只片 0）`32693` |
| 城堡 | 10min 落盘 / 战况 / 税是全局状态机 | `castle_*`（只片 0）`33101` |
| 脚本宿主（NPC/商店）| `Rc` ⇒ `!Send`，**物理上**不能跨线程 | `npc_host`/`merchant_book` `33217/33220` |
| 元宝寄售全局表 | 全服共享报价，分区会分裂 | `sell_off_book` `33980` |
| 换服/跨服 | 依赖进程级 `server_table` | `process_switch_data` `36793` |
| MakeIndex 计数器 | 全局唯一编号，分区会撞号 | `item_number` `33953` |

> 【已核】这七项今天**已经**是进程级/单点；本项不需要「把它们变单点」，只需保证区 k 的
> 请求能路由到它们并把结果帧送回。

### 5.2 1 万时会不会成为新瓶颈（粗算）

- **周期存档**：默认 `SaveHumanRcdTime=600s`（`config.rs:1595`）。稳态 = 10000 / 600 ≈ **16.7 次/s**；
  每条记录约 1–4KB ⇒ **< 100KB/s**。单 DB 链绰绰有余。【推断】稳态不是瓶颈。
- **冷启动登录风暴**：10k 登录集中在数分钟 ⇒ 这才是 DB 读档的真正峰值（不是稳态）。
  今天已用 `EventDrain` 预算 + 入口三段计数节流；【推断】需按登录速率限流，与「每区 host 循环」正交。
- **关服存档**：`shutdown_save_all` 有界等待（`SHUTDOWN_SAVE_TIMEOUT` `34114`）。若串行 10k × ~5ms
  ≈ **50s**，可能超时。**这是已知风险**，需要批量/并发或提高超时；不在本项范围但必须登记。
- **行会 10s 门**：O(会数×成员)，500 会 × 20 人 = 10k 操作 / 10s = **1k/s**，且只对交互/在线者——
  【推断】非瓶颈；但若所有区的行会命令都汇聚到区 0 单线程，交互峰值需实测。
- **全局广播扇出（真瓶颈）**：2000 档 `refmsg_targets` 均值 **88.5k/tick**（容量曲线 §4-4），
  这是 AOI 扇出，不是单事件。10k 若全靠区 0 扇出必爆（986 档 p99 47ms → 2000 档 p99 201ms，
  超线性）；**分区并行的正是这部分**。单点服务（DB/行会/城堡）本身低频低量，不会替代扇出成为瓶颈；
  真正的风险是**「跨区帧/全服广播被汇到一条单点队列」**——所以 ⑤ 的「发到收件区、由收件区并行发」
  是关键，本项不得绕开它再造一条单点队列。

**结论**：单点服务在 10k **不是**吞吐瓶颈（除非把跨区/全服广播汇成单点队列、或关服存档不批量化）。
真正的吞吐瓶颈是**扇出**，它必须留在各区并行——这正是本项与 ⑤ 共同要守住的边界。

---

## 6. 七问⑥：与 ⑤ 的先后与接缝

### 6.1 两者是什么

- **⑤（`card/l25-step5`，未上基线）**：**区 0 发送者 → 区 k 收件人**的跨区帧合成。设计见
  `docs/issues/l2-5-step5-cross-recipient-frames-design-20261122.md`（V2：源区写目标区邮箱，
  目标区在**下一 tick `barrier1` 之后、`phase2` 之前**按 `src_tick < 当前 tick` 过滤排空）。
  它只改了 `send_ref_msg_to_targets` 的「本地会话未命中 ⇒ 收件人是镜像」这一分类点 + 邮箱 + 三桶计数。
- **本项**：**区 k 自己成为发送者**——区 k 的玩家/怪物产生帧，区 k 自己 drain 并发给收件人。

### 6.2 谁先

**建议 ⑤ 先落（或至少先落其邮箱/收件区排水点 + `CrossZoneSink` API）。** 理由：
1. 本项的区 k 一旦自己发帧，就**必然**有「收件人在邻区」的情况；若 ⑤ 未落地，本项会被迫自造
   第二条跨区队列 ⇒ 两条路径帧序/去重各自为政，之后合并不可能。
2. ⑤ 的「恰好 1 tick 延迟 + `src_tick` 过滤」是**收件区排水点**的唯一定义；本项应在**同一排水点**
   之后追加区 k 自己的本地 drain（或复用同一 drain 序列），保证顺序一致。
3. ⑤ 改动面很小（单事件最小切片 + 计数），先落不会长期占住主循环。

**允许并行**：本项的「**局部收件人**」部分（M1–M3）不依赖 ⑤（同区帧不经邮箱）；可以并行开发，
只把「跨区收件人」留到 M4 接 ⑤。这样两条车道不互相堵。

### 6.3 会撞的文件/函数（【已核】⑤ 当前 diff）

| 文件/函数 | ⑤ 改什么 | 本项会改什么 | 切分办法 |
|---|---|---|---|
| `bins/game-svr/src/lib.rs` `ZoneHost` | 加跨区邮箱字段 | 不碰该结构（或加不同字段） | ⑤ 独占 `ZoneHost` 的邮箱字段 |
| `spawn_minimal_zone` | 安装 sink、barrier1 后排空 | 扩 tick 体、区 k drain 序列 | ⑤ 的排空点先加；本项在其**后**追加本地 drain 段（不同 stage 标签） |
| `sim_loop` 主循环 | 装区 0 sink、记 `src_tick`、barrier1 后排空 | 抽 `HostLoop`/区实例 | ⑤ 只加少量插入点；本项做结构抽取——**本项在 ⑤ 合并后 rebase** |
| `region_loop::run_zone_tick` / `apply_mirrors` | `HaloEntity::route` 生命周期 | 扩 drain 序列 | ⑤ 独占 route；本项别碰 route 字段 |
| `perf_counters` | 加跨区三桶 + canary 列 | 加分区读数（`af=`/`zone_cmd_unsupported` 等） | **列清单先对齐**：⑤ 的列已定，本项在其后追加，不改其列义 |
| `crates/world-bevy/src/world.rs` | `MirrorRoute` + `set_mirror_route` | 不改 | ⑤ 独占 |
| `send_ref_msg_to_targets` | 分类 + 入邮箱 | 本项的 drain 会调用它 | ⑤ 独占该函数体；本项只改**调用点**（从区 k drain 调） |

### 6.4 指令集切分（避免互相踩）

- **⑤ 只管「一帧发不出去、收件人是镜像」时的去向**（分类 + 邮箱 + 计数），不改命令分发。
- **本项只管「命令如何到达区 k、drain 如何被区 k 调用」**，所有跨区发送一律**经 ⑤ 的 sink**，不自造队列。
- **交集只有一个**：`sim_loop`/`spawn_minimal_zone` 主循环的「barrier1 之后的排水点」。
  约定：⑤ 先钉该点并实现「跨区帧排空」；本项在同一位置**排在本区 drain 之前**或**之后**（按帧序
  证据定，建议：先本地 drain，再跨区排空，或反之——**由 M1 的两区对拍帧序决定并写死**）。
- **提交切分**：⑤ 的提交（sink/邮箱/计数）与本项的提交（抽取/drain/命令）**不同分支**，
  本项在 ⑤ 合并到 `master` 后 rebase，主循环的插入点只解一次冲突。

---

## 7. 七问⑦：一句话回答 Owner

> 从今天的 `master`（`03b103c25`）出发，到「单图 1 万在线且区 k 玩家可正常游戏」，
> 剩**中位约 95 工程日**（乐观 60–70，悲观 140+；含实现/单测/变异/对拍，不含真机窗口排队；
> 3–4 条车道并行时日历约 **7–10 周**，串行约束见 §4/§6）。
> **最大不确定性 = 跨区帧序等价证明 + 脚本宿主 `!Send` 逼出的「NPC/商店回区 0 串行执行」是否在
> 10k 下成为热点**（前者决定要不要加全局屏障、后者决定要不要先做 `Send` 重构）。

---

## 8. 已核事实 vs 推断（给复核者）

**已核（读代码/文档确认）**
1. `sim_loop` = `lib.rs:32504–36857`；主循环 `34094–36856`；启动段 `32504–34093`。
2. 区 k `run_zone_tick`（`region_loop.rs:726`）调用 `tick_phase1/phase2` 但**不消费 `TickRecord`**
   ⇒ 区 k 无下行 drain ⇒ 不出网关帧。
3. 区 k 命令只认移动（`handle_zone_command` `1441`，`movewire::is_move_command` 之外一律
   `note_zone_cmd_unsupported`）。
4. `ZoneState` 已复制 `world/displays/online/ground_sessions/saves/login_tails/aoi_frames_total/
   shop_sessions/sell_off_sessions/save_outbox`（`1005–1024`）。
5. 脚本宿主 `npc_host`/`merchant_book` 含 `Rc` ⇒ `!Send`（计划页 §12.8 偏差 + ④b-1 提交信息）；
   `handle_click_npc_cmd`/`handle_shop_cmd` 签名确带 `merchant_book`。
6. 约 30 个 `drain_*` 是 `(rec, world, displays, online, sessions, gates, 配置)` 的纯函数
   （抽样 `drain_npc_merchant_turns` `21208`、`drain_run_downlinks_with_magic` `23229`、
   `drain_kill_exp_chain_with_magic` `22639`）。
7. `World` 有 `owner_thread` 断言（`world.rs:9146/9204`）。
8. `ZoneState::new` 对每区 `World::with_maps(map_sizes.to_vec())`（`1043`）⇒ N 份全图 grid。
9. 默认 `SaveHumanRcdTime=600s`（`config.rs:1595`）。
10. 容量：986 档 p99 47ms GREEN；2000 档 p99 201ms RED、`refmsg_targets` 均值 88.5k/tick
    （`docs/issues/capacity-curve-release-20261121.md`）。
11. ⑤ 设计 + 最小 diff 在 `card/l25-step5`，**未在基线**（基线无该文档；diff 触
    `send_ref_msg_to_targets`/`ZoneHost`/`spawn_minimal_zone`/`sim_loop`/`perf_counters`/
    `apply_mirrors`/`world.rs`）。
12. `EventDrainBudget::PRODUCTION` = 2000 条 / 50ms。

**推断（实施前须复核）**
1. 「多数 `handle_*`/`drain_*` 可直接在区 k 复用」——已抽样，但**未逐个**核对是否隐含进程级/全局序依赖。
2. 95 工程日中位与 60–70 乐观 / 140+ 悲观——基于上述工作分解，非测量。
3. 单点服务在 10k 非瓶颈的粗算（存档 16.7/s、行会 1k/s、扇出才是瓶颈）。
4. 关服 10k 存档可能超时（串行 ~50s 估）。
5. 内存：N 份全图 grid 在单图场景可接受、多图需评估。
6. ⑤ 先落能让本项少造一条队列——依赖 ⑤ 设计不再变（v3 已定 V2）。

---

## 9. 参考

- `docs/issues/l2-5-step3-zone-slicing-plan-20261122.md`（③④a④b-1 实施与残余，§12.7–12.9）。
- `docs/issues/l2-5-step5-cross-recipient-frames-design-20261122.md`（⑤ 设计 v3，在 `card/l25-step5`）。
- `docs/issues/capacity-curve-release-20261121.md`（986/2000 实测与扇出读数）。
- `docs/AGENTS.project.md`（判据分级 R/L、五门禁、收尾 DoD）。

---

## 10. 热路径 O(n) 扫描清单（只读扫描；未编译 / 未起栈 / 未测；2026-11-23）

> **任务口径**（opc-27 限定单）：扫 `sim_loop` 主 tick 与 `tick_phase1/phase2` 内**每事件路径**
> （移动 / 攻击 / 物品 / NPC 商人 / 广播收件人选择），以及查询接口 `OnlineRegistry` /
> `DisplayTable` / `ground_sessions`(=`HashMap<String,SessionGround>`) / `World.entities`。
> 本节**只给事实与排序**，不评估工程日（§0–§9 的评估冻结不动），也**不给改法**。
> 行号 = 本 worktree `03b103c25` 的 `lib.rs`（未特别标注时）。

### 10.1 口径

- **n** = 每区在线玩家数：`OnlineRegistry.entries` 只装玩家（`by_actor`/`by_chr` 是它的索引）。
- **E** = 区内 `want_ref_msg` 实体数：`iter_send_ref_targets()` 的扫描面（玩家 + 可被看见的怪）。
- **「每 tick × 每事件」**：该调用是否在 tick 内按事件/按会话重复。
- 扫描规模是**静态估**（按 n/E 的符号复杂度），没有真实时长；标「需测」的是无法静态定量的。
- 未编译、未起栈、未测。

### 10.2 顶层事实：这是**一类**缺陷，不是一处

1. **O(1) 索引已存在，热路径未用**：
   - `OnlineRegistry` 同时持有 `Vec<OnlineEntry>`（插入序，唯一真源）与 `by_actor:HashMap<i64,usize>`、
     `by_chr:HashMap<String,usize>`；`find_by_actor`(24792)/`find_by_chr`(24805) 是 **O(1)**。
     但非测试代码仍有 **44 处 `find_linear_chr`**（字符串线性扫描）+ **64 处 `find_linear_actor`**
     （id 线性扫描）。`find_linear_*` 是**显式探针**（注释 24817：「只包一个计数，不得顺手改成索引」，
     计数键 `CHR_LINEAR`/`ACTOR_LINEAR`）。
   - `SessionRegistry` 有 `get`(`crates/world-bevy/src/session.rs:236`)/`get_mut`(:241)（O(1)，卡
     `openmir2-p1-hostinbound-index` 已接 `open`），但 `drain_login_tail` 仍 `sessions.iter().find(name)`(21414)。
2. **每 tick 有多处 O(n²) 扫描环**（A 组：4 个满 n²、2 个 n²/5；都是先收集/排序全部会话名，再对每个名字做一次 O(n) 线性查表）——
   这是结构性缺陷，随 n 平方增长。
3. **移动命令每事件 2 次 O(n) 字符串线性扫描**：入口 `handle_move_cmd`(14361) + `send_good_tick`(20108)；
   后者调用时 `entry` 已在 `handle_move_cmd` 作用域内（重复取同一条）。
4. **投递段已用 O(1)，但同类未统一**：`send_ref_msg_to_targets`(19899) 用 `find_by_actor`（O(1)）；
   而 combat/buff/aoi 的**观察者投递**仍用 `find_linear_actor`（下表 B 组）——同一项目两条路径口径不一致。
5. `DisplayTable.get`(`bins/game-svr/src/downlink.rs:315`，`by_id:HashMap<i32,usize>`) 与
   `World.entities.get`(`crates/world-bevy/src/ecs.rs:560`，dense 数组/sparse 表，按 i32) 都是 **O(1)**，
   **不在缺陷面**。

### 10.3 排序表（按预计贡献从高到低）

> 「键」：str = 字符串相等；id = i32 相等。「同款」= 与 `find_linear_chr` 同款（字符串线性扫描）。

**A 组：每 tick × O(n²)（结构性，最陡）**

| 排名 | 函数 : 行（循环 → 扫描） | 操作 | 数据结构 / 复杂度 | 每 tick × 每事件 | 键 | 平均扫描规模 | 同款 | 备注 |
|---|---|---|---|---|---|---|---|---|
| A1 | `drain_aoi_downlink` 20668(`for sess`) → 20693 | `find_linear_chr(&sess.chr_name)` | `Vec` 线性 / **O(n²)**（约 n/5 会话/tick，search 门见 20685-20689） | 每 tick（HostAoiDownlink） | str | ≈ n²/5 compares/tick（n=1e3≈2e5；1e4≈2e7） | **是** | 该处已有 `HostAoiAppearSelect` 探针包裹 |
| A3 | `drain_stamina` 22953(`for name in names`) → 23050 | `find_linear_chr(&name)` | 同上 **O(n²)** | 每 tick（HostRecovery 经 run_downlinks） | str | ≈ n² | **是** | |
| A4 | `drain_player_need_recheck_with_magic` 28877 → 28878 | `find_linear_chr(&name)` | 同上 **O(n²)** | 每 tick（HostPlayerRun） | str | ≈ n² | **是** | |
| A5 | `drain_player_game_gold` 29609 → 29610 | `find_linear_chr(&name)` | 同上 **O(n²)** | 每 tick（HostPlayerRun） | str | ≈ n² | **是** | |
| A6 | `drain_player_revival_with_magic` 29817 → 29818 | `find_linear_chr(&name)` | 同上 **O(n²)** | 每 tick（HostPlayerRun） | str | ≈ n² | **是** | |
| A7 | `drain_aoi_events` 20837(`for sess`) → 20853 | `find_linear_chr(&sess.chr_name)` | 同上 **O(n²)** | 每 tick（HostFrames 经 run_downlinks） | str | ≈ n²/5（同 A1 的 search 门） | **是** | 20819 另有 `find_linear_actor`(event_hides) |

**B 组：每 tick × 每事件 / 每帧的 O(n) 观察者投递（id 线性）**

| 排名 | 函数 : 行 | 操作 | 复杂度 | 每 tick × 每事件 | 键 | 扫描规模 | 同款 | 备注 |
|---|---|---|---|---|---|---|---|---|
| B0 | `flush_run_frames` 29426(`for actor_id in actors`) → 29432 / 29477 | `find_linear_actor`（每个本 tick 有帧的 actor + 每个 ref_msg 收件人） | O(k×n)，k=本 tick 有帧的 actor/收件人数 | 每 tick（HostPlayerRun），按**有帧 actor** | id | k×n（k 取决于本 tick 活动，**需测**） | 否（id 版） | `actors=frames.pending_actors()`，非全会话；29477 另叠加观察者×n |
| B1 | `drain_combat_downlink` 22248 / 22289 / 22333 / 22349 | `find_linear_actor`（受击者 / 观察者 / 死亡目标 / 经验者） | O(k×n)，k=该事件收件人/观察者数 | 每 tick，每战斗/死亡/经验记录 | id | k×n（k≈视野内玩家数，需测） | 否（id 版） | 2 处走 `ref_msg_cache::targets` 后逐 observer 再线性查 |
| B2 | `drain_buff_downlink` 21676 / 21721 / 21769 / 21787 | `find_linear_actor`（recalc / 657 / sysmsg / hp-mp） | O(k×n) | 每 tick，每 buff 记录 | id | k×n | 否 | |
| B3 | `drain_aoi_disappear_with_counts` 20478、`drain_depart_frames` 20547、`drain_aoi_events` 20819 | `find_linear_actor(observer)` | O(k×n) | 每 tick，每出现/消失/离世事件 | id | k×n | 否 | |
| B4 | `drain_monster_ref_msgs` 28518 区（3 处）、`drain_skill_train_events` 15115 区（2 处）、`drain_mag_struck_b2` 15750 区 | `find_linear_actor` | O(k×n) | 每 tick，对应记录非空时 | id | k×n | 否 | 怪/技能/法术受击扇出 |
| B5 | 其它 `find_linear_actor` 单点（`drain_weapon_durability`/`drain_attack_weapon_wear`/`drain_close_health_for`/`drain_death_use_item_drops`/`drain_levelup_*`/`drain_mag_healing_delivery`/`drain_party_death_events`/`drain_slave_mutiny_downlink`/`drain_yedo_events`/`drain_run_state_expiries`/`flush_healthspell_slots`/`b3_outcome_frames`/`push_charstatuschanged*`/`broadcast_*` 等，共约 40 处） | `find_linear_actor` | O(n) 每次 | 每 tick（视记录）+ 每广播 | id | n | 否 | 逐条见 §8 的函数分布 |

**C 组：每事件 O(n) 字符串（命令路径）**

| 排名 | 函数 : 行 | 操作 | 复杂度 | 每 tick × 每事件 | 键 | 扫描规模 | 同款 | 备注 |
|---|---|---|---|---|---|---|---|---|
| C1 | `handle_move_cmd` 14361 | `find_linear_chr(chr_name)` | O(n) | **每移动事件**（实测主导入站） | str | n（≈150 moves/tick×n） | **是** | peer 实测 MoveRest 30.4/42.3ms/tick 的候选面 |
| C2 | `send_good_tick` 20108 | `find_linear_chr(chr_name)` | O(n) | **每移动事件**（+GD） | str | n | **是** | 与 C1 同一事件内**第二次**取同一 `entry` |
| C3 | `handle_hit_cmd` 14877 | `find_linear_chr(chr_name)` | O(n) | 每攻击事件 | str | n | **是** | |
| C4 | `handle_inbound_cmd` 5924 及分派到的 `handle_ground_cmd` 6515 / `handle_deal_cmd` 6761 / `handle_bag_cmd_with_magic` 7091 / `handle_selloff_cmd` 8903 / `handle_shop_cmd_inner` 9154 / `handle_storage_cmd` 10321 / `handle_eat_cmd` 12555 / `handle_click_npc_cmd` 8096 / `handle_party_cmd` 5307 / `handle_query_*` | `find_linear_chr(chr_name)` | 每条命令 O(n) | 每命令事件（频次低于移动） | str | n | **是** | 每个 handler 入口各一次 |
| C5 | `handle_spell_cmd_with_magic` 16287 / `finish_spell_tail_with` 16045 | `find_linear_chr` + 3 处 `find_linear_actor` | O(k×n) | 每施法事件 | str/id | n | **是（chr 处）** | |
| C6 | `drain_login_tail` 21396 区 → 21409(chr) + 21414(`sessions.iter().find`) | `find_linear_chr` + `Vec` 线性 | O(pending×n) | 每 tick（有登录尾时） | str | pending×n（pending 小） | **是**（21414 是 sessions 名线性，且 `sessions.get` 已存在） | |

**D 组：低频 O(n)/O(E)（不在每 tick 路径，供完整对照）**

| 排名 | 函数 : 行 | 操作 | 复杂度 | 触发 | 键 | 规模 | 同款 | 备注 |
|---|---|---|---|---|---|---|---|---|
| D1 | `OnlineRegistry::set` 24757 | `entries.iter_mut().find(name)` | O(n) | 每登录/离世（`online_set`） | str | n | **是** | 随后 `rebuild()` O(n) |
| D2 | `online_find_for_kick` 24919 | `online.iter().filter` + `seen_gate` 线性 | O(n×gates) | 每 `KickAccount`（顶号/踢） | str(account,忽略大小写)/int | n | 是 | 已有 `KICK_SCANS` 计数 |
| D3 | `party_play_object_by_name`(`crates/world-bevy/src/party.rs:434-441`) → 440 | `entities.iter_ref().find(race==Play && name==name && !ghost)` | O(E) | 每次组队按名找人 | str | E | **是** | oracle 同款线性（`GetPlayObject`） |
| D4 | `guild` 名查 (`crates/world-bevy/src/guild.rs:1072/1077/1212/1222`) | `guilds.iter().find(name)` / `position(name)` | O(会数) | 每次行会命令 | str | 会数 | **是** | 低频 |
| D5 | `handle_query_user_set_cmd` 7853 | `online.iter().any(name)` | O(n) | 每次 `CM_QUERYUSERSET` | str | n | **是** | oracle 函数体为空 |
| D6 | `castle` 图名查 (`crates/world-bevy/src/castle.rs:3459`) | `maps.iter().position(name)` | O(图数) | 城堡操作 | str | 图数 | **是** | 低频 |

**E 组：字符串键 Hash（不是 O(n)，是 O(key 长度) 哈希；供完整对照）**

| 排名 | 位置 | 操作 | 复杂度 | 触发 | 键 | 备注 |
|---|---|---|---|---|---|---|
| E1 | `sessions.get`/`get_mut` / `ground_sessions.get`（约 420 处，含 `handle_move_cmd` 内 14421/14482/14516/14530） | `HashMap<String,_>` 取/改 | O(len) 哈希 + 相等 | 每事件多次 | str | 非 O(n)；移动路径每次事件多达 4 次字符串哈希 |

### 10.4 不计入「查表缺陷」的 O(n)（说明，避免误读）

- `hear_targets_at_scan`（`chat.rs:213-241`）`iter_send_ref_targets().filter(...).collect()` + `sort_unstable`
  是 **AOI 收件人选择本体**，O(E) + O(k log k)；且 `ref_msg_cache` 500ms 缓存命中时**不扫**
  （`cache_first_hit_does_not_scan` 用例）。它是「选择」不是「查表」，故不列缺陷；但它是走位扇出的候选面
  （每次缓存刷新 O(E)）。
- `drain_*` 里 `sessions.keys().cloned().collect()` + `names.sort()` 是 O(n log n) **确定性排序**（为帧序），
  不是查表缺陷；缺陷在其后的 per-name 线性查表（A3–A6）。
- `World::tick_phase1` 的怪 AI 按实体槽位序遍历 O(E) 是**仿真本体**，不是查询缺陷。
- `aoi_show_targets`(20594) 遍历观察者自身 `visible_actors`，O(可见集)，与全局 n 无关。

### 10.5 与实测的对应（事实；不下结论）与现存探针

- peer 实测（1k，120s，两臂）：入站 98.9–99.9% 在移动；MoveRest 30.4/42.3ms/tick，格占用/格子更新/帧构造
  合计 <0.15ms/tick；转身 ~255–282µs、行走 ~240–246µs；切片开关无可测差异。
  → C1+C2（每次移动 **2 次 O(n) 字符串线性扫描**）落在 MoveRest 的候选面内；静态量级 ≈ 2×n×移动数/tick
  （n=1e3、~150 移动/tick ≈ 3e5 compares/tick）。**需测**：以 `CHR_LINEAR` 计数与 `HostInbound` 探针核对。
- A1 等每 tick O(n²) 与容量曲线「986 p99 47ms → 2000 p99 201ms、超线性」方向一致（仅三点，不宣称因果）。
- 现存可机读探针/计数（无需新写即可对账）：
  - `perf_counters::CHR_LINEAR` / `ACTOR_LINEAR`（`find_linear_*` 每次 +1），`CHR_INDEXED`/`ACTOR_INDEXED`
    （`find_by_*` 每次 +1）—— 见 24793/24806/24821/24828。
  - `REFMSG_TARGETS`（19894）、`KICK_SCANS`（24914）、`World::ref_msg_scan_total`（`note_ref_msg_scan` chat.rs:203）。
  - tick_profile 段：`HostAoiAppearSelect`（20692-20711）专门计 A1 的选择段；`HostInbound` 计 C 组；
    `HostPlayerRun`/`HostAoiDownlink`/`HostRecovery` 覆盖 A1、A3–A7 与 B0。
- 本表**未**把「按名字收集再排序」的价格算进 A 组（只算其后的线性查表）；A 组的真实常数含两部分，**需测**。
