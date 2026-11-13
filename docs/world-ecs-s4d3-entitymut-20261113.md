# World ECS S4d-3：按段裁剪的 EntityMut 适配器设计

> 卡片：`openmir2-world-ecs-s4d3`（tier R）。
> 分支：`card/world-ecs-s4d3`，base `95fdbb68a`（master，S4d-1/2 已合入）。
> 本文是 **S4d-3 的前置门**：Leader 在卡片 comment 明确批准前，不动任何生产代码。
> 本批**只写设计**，不改 `.rs`、不新增依赖。

本文只使用**符号/文件锚**（指向 `TickStage`、`EntityStore::get_mut`、`SimState` 这类标识符），
不使用 `path:line` 形式——`mir2-rs/crates/world/**`、`mir2-rs/bins/game-svr/**`、
`mir2-rs/tests/parity/csharp/**` 都是 `docs_anchor_gate` 的高 churn 区，行号锚会独立判红。

---

## 0. 结论摘要（先读这一节）

1. **段系统 = 独占系统 + 段裁剪适配器**。18 个余段全部写成
   `fn(&mut bevy_ecs::world::World)` 独占系统（与已落地的 `sim_hiter_snapshot_system`
   同形），系统内部用本段的适配器取组件。适配器把「槽位顺序 + id 解析 + 段白名单」
   收成一个类型，业务段拿不到裸 `Query`、拿不到本段白名单之外的组件。
2. **结构前提（本设计新增结论）**：`EcsStore` 的**槽位表 / 空闲槽表 / `IdIndex` 必须迁进
   bevy `World` 的资源**（`SlotTable`）。理由见 §1.3：Command / NPC / 商人 / 宝宝清表
   四段会在 tick 中增删实体，快照式 id 表会让后续段漏实体或遍历已删实体，直接偏离旧行为。
   不迁槽位表，S4d-3 无法做到「行为等价」。
3. **拒绝语义 = 旧 Option 语义逐项照抄**：`id 不在槽位表` / `实体已移除` / `组件缺失`
   三种情形**一律返回 `None` 并静默跳过**，不引入 `Result`、不 panic。适配器只新增
   「误用」类失败（越白名单、缺资源、结构操作未声明），且这些在旧形态里根本表达不出来。
4. **白名单是编译期的**：`StageSpec` + `Reads<C>` / `Writes<C>` 标记 trait，
   越白名单立即编译失败。运行期另有一层 fail-closed 断言（镜像白名单 + 组件注册表），
   只用于堵住「泛型/dyn 绕过」路径，并配负例牙。
5. **panic 恢复（review F5）显式登记为「不做热恢复」**；窗口数的口径是**分阶段**的：
   **P0+P1 中间态 = 4 个 `SimState` 搬移窗口 / tick**（Clock / Snapshot / Drain / AOI），
   **P7 收口目标才是 2 个**（head 一个、tail 一个：其余段全部系统化后 head 只跑一次）。
   本批只保证哨兵覆盖全部窗口；不宣称已收敛到 2。哨兵让下一次 tick 立即 fail-fast，
   而不是在 `SimState::default()` 上空转。
6. **顺序载体不是一律 `TickEntityIds`**：每段有**旧实现自己的语义载体**（命令 FIFO、
   `npc_ids_sorted` / `merchant_ids_sorted` 的 id 升序、`SelfDelayQueue::actor_ids` 入队序、
   事件数组序）。新实现**逐段照抄旧载体**，`TickEntityIds`（槽位序）只在旧实现本来就用
   `iter_ref` / `order_index` 的段上使用。`Query` 迭代序在**任何段**都不是载体。
7. **两个决定必须先由 Leader 裁决**：D1（把槽位表迁进 bevy World 资源）与
   D2（S4d-3 写集扩到 7 个模块文件）。没有 D2，AI / NPC / 商人 / 宝宝 / buff / 自投递
   六段无法系统化（它们的实现在 `impl World` / `fn(&mut World)` 里）。

---

## 1. 输入事实与结构结论

### 1.1 现状（master `95fdbb68a`）

- `crates/world-bevy/src/tick.rs`：`TickStage` 20 段、`STAGE_ORDER` 20 项、
  `TickRunner`（`Schedule` + 显式 `SingleThreadedExecutor` + `ambiguity_detection = Error`），
  唯一注册入口 `TickRunner::add_systems(stage, 单个系统)`，方法白名单
  `{add_systems, assert_build_contract, default, new, run}`。
- `crates/world/src/world.rs`：`World { entities: EntityStore, clock_runner, tail_runner,
  drain_runner, snapshot_runner, pub sim: SimState }`，对 `SimState` 实现 `Deref`/`DerefMut`。
  `World::tick_head` / `World::tick_tail` 仍内联跑全部业务段；已系统化的只有
  `SimHiterSnapshot`（#2，`snapshot_runner`）与 `OutputDrain`（#15，`drain_runner`），
  `Clock`（#0，`clock_runner`）与 `OutputAoi`（#19，`tail_runner`）本来就是系统。
- 余 **18 段**（20 减已系统化的 `SimHiterSnapshot`/`OutputDrain`）见 §2.1。
- `EcsStore<T>`（`crates/world-bevy/src/ecs.rs`）：bevy `World` 本体 + `slots`（槽位表）+
  `free`（LIFO 空闲槽）+ `index: IdIndex`（dense 数组 + HashMap 兜底）+ 四个缓存
  `QueryState`。槽位顺序是**唯一**的处理顺序来源。
- `EntityStore`（`crates/world/src/entity.rs`）：`EcsStore<CoreIdentity>` 的薄适配层，
  另有 `view_state`/`view2_state`/`view3_state`/`mut_view_state` 四个跨组件缓存查询。
  `get`/`get_mut`/`for_each_mut`/`iter_ref`/`order_index`/`order_entities` 是段的读写面。

### 1.2 结构结论（review `39979c54` 的 A5，本文复述并推进）

段系统拿到的是 `EntityStore` **内部那个 bevy `World` 的裸 `&mut`**；`EntityStore` 的
`slots`/`index`/缓存 `QueryState` 跟着 wrapper 一起不可同时出借。所以：

- **只读段**可以用「`TickEntityIds` 槽位序 id 表 + 裸 `world.get::<C>(entity)`」，
  `SimHiterSnapshot` 已自证；
- **写实体段**需要一个 `EntityMut` 等价物：既能按 id 解析到 bevy 实体，又能按段裁剪地
  拿到 `&mut` 组件。

### 1.3 本文新增结论：槽位表必须进资源（否则行为不等价）

只读段用**快照** id 表是安全的（段内不增删实体）。但余 18 段里有四段会在 tick 中
**改变实体集合**：

| 段 | 结构变更 |
| --- | --- |
| `Command`（#1） | `enter_world` / `leave_world` / `spawn_monster_now` 触发 `EntityStore::insert` / `remove` |
| `SimSelfDeliver`（#5） / `SimSlaveCleanup`（#8） | 宝宝链路上的 `entities.insert`（克隆体） |
| `SimNpc`（#11） / `SimMerchant`（#12） | Ghost 60s 门 `entities.remove(id)` |
| `SimSettleDeaths`（#4） / `TailPlayerDeaths`（#17） | 不增删，但会改 `MapBinding` 与格子表 |

反例（可观测差异，不是理论洁癖）：

- 命令 `SpawnMonster` 在 #1 新建一只 `hp = 0` 的怪；旧实现的 #4 `settle_deaths` 用
  `iter_ref()` **看得到它**并结算死亡。若 #4 用 tick 入口的静态 id 快照，就**看不到**它
  ⇒ 少一条 `deaths` 行（golden 立刻红，但更坏的是只在特定场景红）。
- #11/#12 移除 ghost NPC 后，#13/#14 的静态 id 快照仍含该 id。旧实现 `iter_ref()` 根本
  不遍历它；适配器对已删 id 返回 `None`（跳过）——**遍历计数**类副作用（`for_each_mut`
  的调用次数、`order_index` 的长度）在两者间不等。

因此：**槽位表必须是 bevy `World` 里的资源（`SlotTable`），段在每次结构变更后原地更新它；
`TickEntityIds` 由它派生（每段入口取一次，或干脆按需查）**。这一条把「顺序来源」和
「id 解析」合并成同一份真值，也顺带消掉 review F2 的「第三种顺序载体」隐患。

> 备选（**不推荐**）：保留槽位表在 `EcsStore`，由 facade 在每段之间重填 id 表。
> 这会把 schedule 切碎成多段 run，facade 重新承载段边界，等于放弃「tick 只驱动 TickRunner」。
> 已否决。

---

## 2. 七项设计

### 2.1 段清单与每段写集（must_cover #1）

组件编号沿用 P0 组件图（`docs/world-ecs-migration-20261113.md` §3.1）：

`C1` CoreIdentity、`C2` Transform、`C3` Appearance、`C4` Vitals、`C5` AbilitySnapshot、
`C6` Progression、`C7` Flags、`C8` PvpState、`C9` LifeCycle、`C10` MapBinding、
`C11` BuffState、`C12` PlayerRunState、`C13` DupState、`C14` Inventory、`C15` DropState、
`C16` CombatAttribution、`C17` Recovery、`C18` Revival、`C19` SlaveState、
`C20` SpawnBinding、`C21` MonsterClassState、`C22` AiState、`C23` MerchantState、
`C24` NpcState、`C25` GroupState。

**白名单口径（重要）**：取「该段**所有可达路径**的并集」，宁宽毋缺。白名单写漏会立刻
变成**编译错误**（安全方向）；写宽是评审问题，不是正确性问题。落地时以 `rg` 实况复核，
见 §8 的核对命令。

| # | 段 | 现符号（唯一实现点） | 读 | 写 | 结构变更 |
| --- | --- | --- | --- | --- | --- |
| 0 | Clock | `advance_clock_system` | 无 | 无 | 否 |
| 1 | Command | `World::tick_head` 的 `match cmd` + `enter_world`/`leave_world`/`try_step`/`attack_once`/`spawn_monster_now`/`push_now`/`rush_now`/`butch_now`/`butch_effect_now` | C1–C25 全部 | C1–C25 全部 | **是** |
| 2 | SimHiterSnapshot | 已迁移 | C3,C9,C16 | 无 | 否 |
| 3 | SimExpireHitters | `World::expire_hitters` | C1,C9,C16 | C16 | 否 |
| 4 | SimSettleDeaths | `World::settle_deaths(only_players=false)` | C1,C2,C3,C4,C5,C6,C7,C8,C9,C10,C14,C15,C16,C19,C20,C21,C22,C23,C24,C25 | C4,C6,C7,C8,C9,C10,C15,C16,C19,C20,C25 | 否 |
| 5 | SimSelfDeliver | `World::deliver_self_delay_msgs_with` | C1,C3,C4,C6,C7,C9,C11,C16,C19,C22,C25 | C4,C7,C9,C11,C19,C22,C25 | 可能（克隆体） |
| 6 | SimExtraAbil | `World::expire_extra_abil` | C1,C11 | C11 | 否 |
| 7 | SimMapEvents | `World::run_map_events` | C1,C2,C3,C9,C10 | 无（不改实体） | 否 |
| 8 | SimSlaveCleanup | `slave::slave_list_cleanup_all` + `World::slave_list_cleanup` | C9,C19 | C4,C9,C19 | 可能（`entities.insert`） |
| 9 | SimBuff | `World::buff_tick` / `World::buff_do_work` 到 `BuffSystem::do_work` | C3,C4,C7,C9,C11,C12,C17 | C4,C7,C9,C11,C12,C17 | 否 |
| 10 | SimMonsterAi | `monai::monster_ai_tick` | C1,C2,C3,C4,C5,C6,C7,C8,C9,C10,C11,C12,C13,C14,C15,C16,C17,C18,C19,C20,C21,C22,C23,C24,C25 | C2,C4,C5,C6,C7,C9,C10,C11,C14,C15,C16,C17,C19,C20,C21,C22 | 否 |
| 11 | SimNpc | `World::npc_tick` | C1,C2,C3,C4,C7,C9,C11,C16,C22,C24 | C2,C4,C7,C9,C10,C22,C24 | **是**（`remove`） |
| 12 | SimMerchant | `World::merchant_tick` | C1,C2,C3,C4,C7,C9,C11,C16,C22,C23 | C2,C4,C7,C9,C10,C22,C23 | **是**（`remove`） |
| 13 | SimPlayerRunExpiry | `World::expire_player_run_state` | C1,C3,C9,C12 | C12 | 否 |
| 14 | SimDup | `World::check_dup_obj` | C1,C2,C3,C9,C13 | C2,C13 | 否 |
| 15 | OutputDrain | 已迁移 | 无 | 无 | 否 |
| 16 | TailSuperManRefill | `EntityStore::for_each_mut` 的玩家循环 + `EntityMut::run_super_man_refill` | C3,C7 | C4 | 否 |
| 17 | TailPlayerDeaths | `World::settle_deaths(only_players=true)` | 同 #4 | 同 #4 | 否 |
| 18 | TailVerifyMapTime | `World::verify_players_map_time` 到 `World::verify_map_time` + `World::clamp_overflow_abilitys` | C2,C3,C4,C7,C10,C11 | C4,C10 | 否（改 `SimState::maps` 格子表） |
| 19 | OutputAoi | `aoi::output_aoi_system` | C1,C2,C3,C7,C9,C10 | C10 | 否 |

**`SimState` 字段写集（每段）**

| # | 段 | 读的 SimState 字段（代表） | 写的 SimState 字段 |
| --- | --- | --- | --- |
| 0 | Clock | 无 | `TickClock.clock_ms` / `tick_seq`、`TickOwnerThread`（资源） |
| 1 | Command | `pending`(FIFO)、`rng`、`rng_attack`、`sessions`、`maps`、各类配置 | `pending`（出队）、`rng`/`rng_attack`、`sessions`、`msg_order`、`pending_*`（`pending_ai_attacks` 除外） |
| 3 | SimExpireHitters | `clock_ms` | 无 |
| 4 | SimSettleDeaths | `clock_ms`、`rng`、`maps`、`castles`、`party_cfg`、`kill_by_human_drop_use_item`、`kill_by_monst_drop_use_item`、`slave_self_delay_enabled` | `rng`、`party_del_member` 链、`send_master_die_mutiny` 链、`apply_castle_map_event` 链、`pending_*` |
| 5 | SimSelfDeliver | `clock_ms`、`self_delay_msgs`、`msg_order` | `self_delay_msgs`、`msg_order`、`pending_special_spawns`、`pending_special_ref_msgs` |
| 6 | SimExtraAbil | `clock_ms` | 无 |
| 7 | SimMapEvents | `clock_ms`、`fire_burn_events`、`dig_out_events`、`msg_order` | `fire_burn_events`、`dig_out_events`、`msg_order`、`pending_mag_struck` |
| 8 | SimSlaveCleanup | `clock_ms` | `pending_*`（经 `slave_list_cleanup` 链） |
| 9 | SimBuff | `clock_ms`、`buffs`、`buff_pass_tick` | `buffs`、`buff_pass_tick`、`pending_*` |
| 10 | SimMonsterAi | `clock_ms`、`rng`、`maps`、`process_run_counts`、`castles`、`mon_gen_*`、配置 | `rng`、`process_run_counts`、`pending_ai_attacks`、`pending_weapon_*`、`pending_castle_structures`、`pending_monster_ref_msgs` |
| 11 | SimNpc | `clock_ms`、`rng`、`maps`、`npc_position`、`npc_pass_tick` | `rng`、`npc_position`、`npc_pass_tick`、`pending_*` |
| 12 | SimMerchant | `clock_ms`、`rng`、`maps`、`merchant_position`、`merchant_pass_tick` | `rng`、`merchant_position`、`merchant_pass_tick`、`pending_*` |
| 13 | SimPlayerRunExpiry | `clock_ms` | `pending_refmessage_frames` 链 |
| 14 | SimDup | `clock_ms`、`rng`、`maps` | `rng` |
| 16 | TailSuperManRefill | 无 | 无 |
| 18 | TailVerifyMapTime | `clock_ms`、`maps`（格子表） | `maps`（`add_map_object`/格子 `AddTime`）、`pending_verify_health_changes`、`pending_show_hp_health_changes` |
| 19 | OutputAoi | `maps`、`clock_ms`、`clear_drop_on_floor_item_time`、`event_sight`、`fire_burn_events` | `fire_burn_events`、`event_sight` |

**`TickRecord` 字段白名单（每段只写自己那几栏）**

| 段 | 写的 `TickRecord` 字段 |
| --- | --- |
| Command | `processed`、`attacks`、`pushes`、`rushes`、`butches`、`skeletons` |
| SimSettleDeaths / TailPlayerDeaths | `deaths`、`death_use_item_drops`、`exp_awards`、`party_events`、`castle_structures` |
| SimSelfDeliver | `slave_self_deliveries` |
| SimExtraAbil | `extra_abil_expiries` |
| SimBuff | `buff_status_changes`、`buff_sysmsgs`、`buff_recalcs`、`buff_health_changes` |
| SimNpc | `npcs_run`、`npc_turns`、`npc_hits`、`npc_removed` |
| SimMerchant | `merchants_run`、`merchant_turns`、`merchant_hits`、`merchant_disappears`、`merchant_space_moves`、`merchant_removed` |
| SimPlayerRunExpiry | `run_state_expiries` |
| SimDup | `pushes` |
| TailVerifyMapTime | `verify_health_changes` |
| OutputDrain | 5 处 `append`（已落地） |
| OutputAoi | `aoi_order`、`hides`、`observer_kinds`、`event_hides` |
| Clock | `tick_seq`、`clock_ms`（facade 在边界填，不在段内写） |

> `TickRecord` 的字段白名单与组件白名单同属「段写集」，用同一套标记 trait 表达：
> `WritesRecord<Field>`。这样「某段顺手写了别人的记录栏」也是编译错误。

### 2.2 别名规则（must_cover #2）

**规则 A1（同实体同组件唯一 `&mut`）**：适配器的 `get_mut::<C>(id)` 借用 `&mut self`，
返回的引用生命周期不超过这次借用。想再拿第二个 `&mut`（同实体或跨实体）必须先结束前一个
借用——这与 borrow checker 同构，不需要运行期检查。

**规则 A2（跨实体怎么借）**：**按 id 逐个 `get_mut`，不并行持有两个可写视图。**
禁止「分段扫描」（拿一个 `Query<&mut C>` 迭代整个 archetype）：那会 (a) 把 `Query` 迭代序
带进处理顺序，(b) 允许两个实体同时被可变借用，绕过 A1。

同时改两个实体（攻击者/目标、推挤双方）在现有代码里本来就是「**取快照、改副本、
`EntityStore::write_back` / 受控入口回写**」：S4a 的 `spawn_cloned` + `write_back` 纪律、
`PvpView` 只读快照、`combat` 的目标判定链都是这个形态。适配器把这条纪律固化成 API
形状——一次只给一个实体视图，跨实体协作只能经快照/回写。

**规则 A3（同实体多组件同时 `&mut`）**：允许，但只能经**本段类型的组合视图**：
`StageEntities::view_mut(id) -> Option<StageViewMut<'_>>`，其中 `StageViewMut` 的字段
集合 = 本段白名单的读写并集（写字段 `&mut`，读字段 `&`）。一次组装、一次借用，
既满足 A1 又让常用写法（`e.vitals.hp -= dmg; e.flags.death = true;`）不必拆成两次调用。

**规则 A4（与 `SimState` 资源的借用重叠）**：`SimState` 在 tick 边界以
`mem::take(&mut facade.sim)` 移入 bevy `World` 的 `SimState` 资源，跑完
`remove_resource` 归还 facade；同一时刻只有一个所有者（P0 §4.2 规则 2）。段的取法：

- **独占系统形**（推荐，与 `sim_hiter_snapshot_system` 同形）：系统参数就是
  `&mut bevy::World`；系统内先 `world.resource_mut::<SimState>()` 做一小段（或
  `take` 出需要的那几个字段），**结束这次借用**，再用 `StageEntities` 取实体组件。
  `SimState` 与实体组件**不会同时被可变借用**，因为适配器的构造函数只拿
  `&mut bevy::World`，两者在时间上串行。
- 需要「实体 + `maps` 同时可变」的段（`TailVerifyMapTime`、`SimDup`、`SimMapEvents`；
  `TailSuperManRefill` 不需要）：用 `SimState` 的**字段级分借用**——
  `let SimState { maps, clock_ms, .. } = world.resource_mut::<SimState>();` 之后
  把 `maps` 单独借出来，再构造适配器。这与 S4d-1 之前 `World { entities, sim }` 的
  字段分借用同一手法，语义不变。
- **禁止**把 `&mut SimState` 从资源里「长借」着去调适配器：那会在编译期失败
  （`&mut World` 与 `&mut SimState` 同时持有），这正是我们要的硬约束。

**规则 A5（`self.entities` 与 `self.sim` 的双借用在 tick_head/tick_tail 里怎么拆）**：
`tick_head`/`tick_tail` 只剩边界代码，形态是

```text
facade 侧新建 TickRecord（字段与旧实现同一非零初值）
facade 侧取 ids = order_index().zip(order_entities())
facade 侧：把 (sim, record, ids) 放进 bevy World 资源（SlotTable 若常驻则不搬）
head_runner.run(entities.world_mut())
facade 侧：取回 sim / record / ids，删掉落的一次性资源
```

边界代码不再碰 `self.entities.get_mut`，因此不再出现「`self.entities` 与 `self.sim`
同时可变借用」的组合；`SimState` 与 `EntityStore` 的借用是**时间上串行**的两次
`&mut self` 访问。

### 2.3 越界 / 未知 id 的拒绝语义（must_cover #3）

**总原则：逐项照抄旧 facade 的 `Option` 语义。** 旧实现里这三种情形**全部**落到 `None`，
调用点**静默跳过**（`let Some(e) = ... else { continue }` / `filter_map` / `is_some_and`）。
新适配器**不得**把其中任何一种升级成 panic 或 `Result`，也不得把旧的 `None` 改写成
「默认值继续跑」。

| 情形 | 旧语义（唯一真值） | 新语义 | 负例用例 |
| --- | --- | --- | --- |
| `id` 不在槽位表 | `EntityStore::get/get_mut` 的 `index.get(id)` 返回 `None`，于是 `None` | `StageEntities::resolve(id)` 返回 `None`，于是 `get/get_mut` 返回 `None` | `adapter.get_mut::<Vitals>(9999)` 断言 `None`；再断言适配器**没有**改变任何槽位/顺序 |
| 实体已移除 | 槽位 `entity = None`（`remove` 后），于是 `None` | `SlotTable::entity_of(id)` 返回 `None`，于是 `None` | `insert(7)`、`remove(7)`、`get_mut::<Vitals>(7)` 断言 `None`；再 `insert(8)` 复用槽位后断言 `get_mut(7)` 仍 `None`（**不得**因槽位复用而串到 8） |
| 组件缺失 | `get_manual`/`QueryState` 取不到该组件，于是 `None`；`iter_ref`/`for_each_mut` **跳过**该实体 | 组装的组合视图任一字段取不到，于是 `None`；遍历时跳过 | 用 `insert_component` 造半实体（只挂部分组件），于是组合视图 `None`、遍历跳过，且 `StageSkip::ComponentMissing` 计数器加一（见 §2.4） |

**唯一的失败面（新增，旧形态表达不出来）**，全部是「适配器误用」而不是业务输入：

| 误用 | 行为 | 何时暴露 |
| --- | --- | --- |
| 写本段白名单之外的组件 | **编译错误**（`S: Writes<C>` 不成立） | 编译期 |
| 非结构段调用 `insert`/`remove` | **编译错误**（`S: Structural` 不成立） | 编译期 |
| 运行期镜像白名单不放行（泛型/dyn 绕过路径） | **panic（fail-closed）** | 运行期，配负例牙 |
| `SimState` / `TickRecordRes` / `SlotTable` 资源缺失 | `expect(...)` panic（接线错误；与 S4d-1/2 现有口径一致） | 运行期，首个 tick 即暴露 |

> 为什么不是 `Result`：旧调用点全是「跳过」语义，`Result` 会强迫每个调用点重新决定
> 处置，等于给一段纯结构迁移塞进行为决策面。**拒绝语义的等价性优先于 API 洁癖。**

### 2.4 未注册组件写入（must_cover #4）

**选择：编译期类型化为主，运行期 fail-closed 为副。**

1. **编译期**：每个段一个零大小类型（`struct SimExpireHitters;`），实现
   `StageSpec`；组件白名单用标记 trait 表达：

   ```rust
   pub trait StageSpec: 'static {
       const STAGE: TickStage;
       // 结构变更门是 `Structural` **标记 trait**（只有 Command/NPC/商人/宝宝清表实现）；
       // 早期草案的 `const STRUCTURAL` 已在 P0 返工删除（review P3-4）。
   }
   pub trait Reads<C: bevy_ecs::component::Component> {}
   pub trait Writes<C: bevy_ecs::component::Component<Mutability = Mutable>> {}
   pub trait Structural {}
   ```

   `macro_rules!` 生成 `impl Reads<C> for Spec` / `impl Writes<C> for Spec`。
   适配器方法带 `where S: Writes<C>` 约束，因此**越白名单是编译错误**。

2. **运行期（副）**：`ComponentRegistry` 资源（`TypeId` 与 `&'static str` 双向表），
   在 `TickRunner` 构建期用 `&mut World` 建一次；每次 `get_mut` 用 `debug_assert!` 核对
   「该组件在本段白名单里」。它的作用是给**泛型转发**（`fn helper<C: Component>(...)`）
   与将来的 `dyn` 路径兜底；**不能**替代编译期约束，也不作为主判据。
   **交付批次**：P0+P1 只落地了第 1 条（编译期）；本条的 `ComponentRegistry` 与下面第 3 条
   登记为 **P2 首批前置**（`docs/world-ecs-migration-20261113.md` 的 S4d-3 登记表）。

3. **组件注册完整性**：`ComponentRegistry` 还负责「新组件忘了登记」的 fail-closed——
   构建期把 25 个组件全部注册，任何段声明了注册表里没有的类型名则**构建期 panic**
   （不是被静默忽略）。

**负例牙**（**P2 首批必交付**；P0+P1 未交付，见迁移文档 S4d-3 登记表）：

- **T-neg-1 越白名单**：给 `SimExpireHitters` 的适配器加一句 `get_mut::<Vitals>`，
  断言**编译失败**。落地方式：新增一条源码守卫（`crates/world-bevy/tests/tick_skeleton.rs`
  已经是「读源码 + 断言形状」的守卫），断言每个段实现块里不出现未在白名单中的
  `get_mut::<C>` 文本（剥注释）。人工变异（删白名单一项导致编译红）留档。
- **T-neg-2 未注册组件**：在测试里往 `ComponentRegistry` 注册一个假组件名，再断言
  「段声明了未注册组件导致构建期 panic」；删掉 `registry.assert_registered::<C>()`
  这一行，该测试必须红（证明断言真的接线了）。

### 2.5 段内 panic / 恢复路径（must_cover #5，review F5）

F5 的事实：段系统 panic 时 `facade.sim` 已被 `mem::take` 成 `Default`，移入的 `SimState`
留在 bevy `World`，**没有恢复路径**；旧实现在 panic 后是「半更新态」，而新形态是
「**全丢态**」，严格更坏。处置：

1. **显式登记为不接受范围**：段内 panic **不做热恢复**。理由：tick 是单线程串行，
   任何段 panic 意味着世界状态已不可信；项目既有口径是 fail-fast（`expect` 而非
   `unwrap_or_default`，S3b 评审 P3-1 同口径）。热恢复会制造「看起来活着、其实状态已丢」
   的服务器，风险高于停机。
2. **窗口数（返工 P3-3 订正）**：P0+P1 的**中间态是 4 个 `SimState` 搬移窗口 / tick**
   （`Clock` / `SimHiterSnapshot` / `OutputDrain` / `OutputAoi`；`Clock` 也走
   `run_head_stage`，因为本批仍与内联段交错）。**P7 收口目标才是 2 个**（head 一个、
   tail 一个：其余段全部系统化后 head 只跑一次）。本批**不**宣称已收敛到 2——那是 P7 的
   验收项，P0+P1 只保证哨兵覆盖全部 4 个窗口。head 边界一次移入 `SimState`、`TickRecordRes`、
   `TickEntityIds`（`SlotTable` 按 D1 为常驻资源，不参与搬移），tail 边界同形。
3. **fail-fast 哨兵**：facade 侧加 `sim_in_flight: bool`（不进 `SimState`，是 facade 的
   暂态位）：`mem::take` 前置 `true`，`remove_resource` 归还后置 `false`；`tick_head`
   入口断言 `!self.sim_in_flight`，失败信息写明「上一次 tick 在段内 panic 且未恢复」。
   效果：宿主要是 `catch_unwind` 了段 panic，**下一次 tick 立刻以明确断言停机**，
   而不是在 `SimState::default()`（空地图表 / 种子 42 的 RNG）上继续跑。
4. **牙**：测试里让一个探针段 panic，`catch_unwind` 后再调 `tick_head`，断言第二次调用
   panic 且信息含「未恢复」；把哨兵置位那行删掉，该测试必须红。

> 不做：`catch_unwind` 包住 `runner.run` 后尝试把资源捞回来再拼一个可用世界。
> 那需要在 unwind 边界上读 bevy `World` 的资源，而 bevy 的 `World` 不是 unwind-safe 的；
> 结论是「不可靠地恢复」比「明确停机」更危险。

### 2.6 顺序载体（must_cover #6）

**总规则**：处理顺序**只**来自段的显式载体；`Query` 迭代序在**任何段**都不是载体
（沿用 `ecs.rs` 的四条硬约束与 P0 风险 R1）。`TickEntityIds` 是「槽位序（插入序 + 空闲
槽 LIFO 复用）」的载体，**只在旧实现本来就用 `iter_ref` / `order_index` 的段**使用；
别的段保留它们自己的语义载体：

| 段 | 旧实现的顺序载体 | 新实现 |
| --- | --- | --- |
| Clock | 无 | 无 |
| Command | `SimState::pending` 的 FIFO 序 | 不变（一条一条 `pop_front`） |
| SimHiterSnapshot | `iter_ref` 槽位序 | `TickEntityIds`（已落地，S4d-3 收口到单一 runner） |
| SimExpireHitters | `iter_ref` 取 `gone` 集合 + `for_each_mut` 槽位序 | `TickEntityIds`（两趟：先收集 `gone`，再逐 id 写） |
| SimSettleDeaths | `iter_ref` 槽位序收集 `dead` | `TickEntityIds` |
| SimSelfDeliver | `SelfDelayQueue::actor_ids()`（**actor 首次入队序**） | 不变（**不是**槽位序） |
| SimExtraAbil | `order_index()` 槽位序 | `TickEntityIds` |
| SimMapEvents | `fire_burn_events` 数组（倒序下标）、`dig_out_events` 数组序 | 不变 |
| SimSlaveCleanup | `iter_ref` 槽位序 | `TickEntityIds` |
| SimBuff | `BuffSystem` 内部结构序 | 不变（`SimBuff` 段内自持） |
| SimMonsterAi | `iter_ref` 槽位序 | `TickEntityIds` |
| SimNpc | `npc_ids_sorted()`（**id 升序**） | 不变（**不是**槽位序） |
| SimMerchant | `merchant_ids_sorted()`（**id 升序**） | 不变（**不是**槽位序） |
| SimPlayerRunExpiry | `iter_ref` 槽位序 | `TickEntityIds` |
| SimDup | `iter_ref` 槽位序 | `TickEntityIds` |
| OutputDrain | 向量内部发生序 | 不变（已落地） |
| TailSuperManRefill | `for_each_mut` 槽位序 | `TickEntityIds` |
| TailPlayerDeaths | `iter_ref` 槽位序 | `TickEntityIds` |
| TailVerifyMapTime | `iter_ref` 槽位序 | `TickEntityIds` |
| OutputAoi | 按地图、地图内槽位序 | `AoiIds`（已落地） |

**顺序的牙**：review F3 指出「反转 `TickEntityIds` 次序的变异全绿」，因为当时唯一消费者
用 `find(actor_id)`，顺序不可观测。S4d-3 必须补**顺序可观测**的用例：至少一个段
（`SimExpireHitters` 或 `TailVerifyMapTime`）构造 2+ 实体，其**输出向量顺序**或
**可写序列**与槽位序绑定；反转 `TickEntityIds` 后该用例红。

**`SlotTable` 与 `TickEntityIds` 的关系**（D1 采纳后）：

- `SlotTable` 是**常驻资源**（`EcsStore::new` 插入一次），槽位/空闲槽/`IdIndex` 的唯一真值；
- `TickEntityIds` 退化成「段的入口快照」：每段开始时从 `SlotTable` 取一次；
  结构变更段（Command/NPC/商人/宝宝）直接改 `SlotTable`，后续段天然看到新集合；
- `SlotOrder` / `TickOrder` 两个历史资源在 S4d-3 收口时统一到 `SlotTable`（review F2 的
  处置）；`TickOrder`（只有 id、无填入者）没有消费者，按 D3 由 Leader 定删除还是标记。

### 2.7 测试与变异计划（must_cover #7）

#### 2.7.1 每段一颗「删注册必红」牙

统一牙（覆盖全部 18 段）+ 每段行为牙（覆盖可观测效果）：

- **统一牙 T-reg-all**：调试构建下每个段系统入口把 `TickStage` 推进 `StageTrace` 资源
  （测试专用注册层），一个用例跑一次 tick 后断言 `StageTrace` 的**精确序列**等于
  `STAGE_ORDER`。任何一段注册被删则序列缺一项，红。这比 18 个分散用例更抗
  「删了但别的测试恰好还绿」。
- **每段行为牙**（删该段注册则至少这一条红）：

| 段 | 行为牙（现有优先，缺的补） |
| --- | --- |
| Clock | golden 首行 `tick`/`clock`；`tick_skeleton` 的时钟用例 |
| Command | 命令 FIFO 用例（进入/离开/移动/攻击/刷怪/推挤/采集） |
| SimExpireHitters | `expire_hitters_uses_wall_clock_last_hiter_30s_exp_hitter_6s` |
| SimSettleDeaths | `settle_deaths_*` 三例 + `world_combat_parity` 的死亡行 |
| SimSelfDeliver | 宝宝自投递用例（`slave` 模块） |
| SimExtraAbil | `ExtraAbil` 过期用例 |
| SimMapEvents | `map_effect_vectors` + 火墙/挖出僵尸场景 |
| SimSlaveCleanup | 宝宝清表用例 |
| SimBuff | buff pass 用例（`buff_enabled` 场景） |
| SimMonsterAi | `monster_ai_tick_is_slot_ordered_and_deterministic` + `special_monster_vectors` |
| SimNpc | `npc_tick_cadence_and_run_time_gate` / `npc_tick_runs_in_slot_order` |
| SimMerchant | `merchant_tick_and_identity_do_not_depend_on_race` |
| SimPlayerRunExpiry | `run_state_expiry_vectors` |
| SimDup | `check_dup_obj_vectors` + `run_gap::dup_push_dir` 的 RNG 账本 |
| OutputDrain | `weapon_break_frame_is_one_1102_per_nearby_player`（review M1 已证） |
| TailSuperManRefill | Superman 回满用例 |
| TailPlayerDeaths | 玩家死亡用例（P32 之后结算） |
| TailVerifyMapTime | 30s 门 + 钳制用例 |
| OutputAoi | `world_aoi_parity` + `aoi_viewrange12` |

#### 2.7.2 写段另需「丢写回必红」牙

对每个写实体的段，各做一次「删一条写回」变异并记录红光：

| 段 | 变异 | 期望 |
| --- | --- | --- |
| Command | 删 `Command::Kill` 的 `die_called = true` | 死亡结算少一条 |
| SimExpireHitters | 删 `last_hiter = None` | 快照/P32 读点变化 |
| SimSettleDeaths | 删 `death_settled = true` | 二次死亡/重复掉落 |
| SimSelfDeliver | 删一条投递效果体的写回 | 对应事件缺失 |
| SimExtraAbil | 删 `extra_abil[i] = 0` | 过期事件缺失 |
| SimSlaveCleanup | 删 `slave_list` 的 retain | 宝宝残留 |
| SimBuff | 删一条 `status_changes` 写 | 状态帧/记录缺失 |
| SimMonsterAi | 删一次 walk/attack 的坐标或攻击写回 | golden/parity 红 |
| SimNpc / SimMerchant | 删 `run_tick = now` | 节奏门失效（每 tick 都跑） |
| SimPlayerRunExpiry | 删 `fire_hit_skill = false` | 到期事件缺失 |
| SimDup | 删 `check_dup_obj_tick = now` | 3s 门失效 |
| TailSuperManRefill | 删 `hp = max_hp` | 死亡判据变化 |
| TailVerifyMapTime | 删 `verify_tick = now` 或钳制赋值 | 格子刷新/钳制丢失 |

变异纪律（沿用项目口径）：只在本人 worktree 跑；恢复后断言 `git hash-object` 与 HEAD
相等、无 `MUTATION` 残留；变异证据必须对**交付 head**重放。

#### 2.7.3 适配器负例牙

- **T-adapter-oob**（越界 id）：`id 不在槽位表` / `实体已移除` 两例（§2.3 表）。
- **T-adapter-nocomp**（组件缺失）：半实体 fixture + 计数器。
- **T-adapter-unregistered**（未注册写入）：§2.4 的 T-neg-2。
- **T-adapter-alias**（别名规则）：尝试在同一作用域持有两个 `get_mut` 则**编译失败**
  （守卫用源码模式断言 + 人工变异留档）。

#### 2.7.4 golden / parity 全清单（A4）

- golden（逐行 diff = 0）：`basic.txt`、`session.txt`、`combat.txt`；
  另登记的对照面：`aoi_viewrange12.txt`、`combat_exp_hitter_ghost.txt`、`combat_exp_nofix.txt`。
- parity（0 failed）：`world_aoi_parity`、`world_combat_parity`、`world_session_parity`、
  `rng_parity`、`rng_replay_parity`、`drop_parity`、`exp_parity`、`levelabil_parity`、
  `player_ctor_rng_trace`；`table_counts` 需 DB（窗口内跑，或按 R/L 例免跑并写理由）。
- crate 内：`crates/world/tests/` 的 `check_dup_obj_vectors`、`ecs_migration`、
  `hit_dice_rng_trace`、`magic_draws_rng_trace`、`run_state_expiry_vectors`、
  `party_vectors`、`special_monster_vectors`、`no_parallel`；
  `crates/world-bevy/tests/` 的 `tick_skeleton`、`bevy_drop_parity`。
- 每个迁移提交都必须五门绿：`cargo fmt`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo test --workspace`、`cargo run -p msgcodegen -- --check`、
  `python -I -B tools/hygiene/docs_anchor_gate.py --prose --base master`
  （cargo 在 `mir2-rs/` 下跑，退出码不要取管道）。

---

## 3. 类型签名草案

> 只写形状，不写实现；落点：`crates/world-bevy/src/ecs.rs`（`SlotTable`）与
> `crates/world-bevy/src/tick.rs`（`StageSpec`、适配器、注册表）。
> 不新增依赖（`macro_rules!` 即可，不引入 proc-macro crate）。

### 3.1 `SlotTable`（D1）

```rust
#[derive(bevy_ecs::prelude::Resource, Default)]
pub struct SlotTable {
    slots: Vec<Slot>,          // generation / entity / key（与现状同形）
    free: Vec<usize>,          // LIFO
    index: IdIndex,            // dense + sparse
}

impl SlotTable {
    pub fn alloc_id(&mut self) -> i32;
    pub fn entity_of(&self, id: i32) -> Option<bevy_ecs::entity::Entity>;
    pub fn order(&self) -> Vec<(i32, bevy_ecs::entity::Entity)>;   // 槽位序
    pub fn order_entities(&self) -> Vec<bevy_ecs::entity::Entity>;
    fn bind(&mut self, id: i32, e: bevy_ecs::entity::Entity) -> usize;
    fn unbind(&mut self, id: i32) -> Option<bevy_ecs::entity::Entity>;
}
```

`EcsStore` 保留 `World` 与四个缓存 `QueryState`，`slots`/`free`/`index` 改为读资源；
对外方法签名不变（`insert`/`remove`/`get`/`get_mut`/`iter`/`iter_mut`/`order_index`/
`order_entities`/`entity_of`），因此 `EntityStore` 与业务模块**零改动**。

### 3.2 段规格与白名单

```rust
pub trait StageSpec: 'static {
    const STAGE: TickStage;
    // 结构变更门是 `Structural` 标记 trait；`const STRUCTURAL` 已在 P0 返工删除（review P3-4）。
}
pub trait Reads<C: bevy_ecs::component::Component> {}
pub trait Writes<C: bevy_ecs::component::Component<Mutability = Mutable>> {}
pub trait Structural {}

macro_rules! stage_spec {
    // 形如：
    // stage_spec!(SimExpireHitters, TickStage::SimExpireHitters,
    //     read: [CoreIdentity, LifeCycle], write: [CombatAttribution]);
    ($spec:ident, $stage:expr, read: [$($r:ty),* $(,)?], write: [$($w:ty),* $(,)?]) => {
        pub struct $spec;
        impl StageSpec for $spec { const STAGE: TickStage = $stage; }
        $(impl Reads<$r> for $spec {})*
        $(impl Writes<$w> for $spec {})*
        // 另生成运行期镜像 READ_NAMES / WRITE_NAMES
    };
}
```

### 3.3 段裁剪适配器

```rust
pub struct StageEntities<'w, S: StageSpec> {
    world: &'w mut bevy_ecs::world::World,
    _spec: core::marker::PhantomData<S>,
}

impl<'w, S: StageSpec> StageEntities<'w, S> {
    pub fn new(world: &'w mut bevy_ecs::world::World) -> Self;
    /// 槽位序 id 表（每段入口取一次；结构变更后重新取）
    pub fn order(&self) -> Vec<(i32, bevy_ecs::entity::Entity)>;
    /// 三态解析（仅诊断/测试用；正式路径直接用 get/get_mut 的 Option）
    pub fn resolve(&self, id: i32) -> Option<bevy_ecs::entity::Entity>;
    pub fn get<C>(&self, id: i32) -> Option<&C>
        where C: bevy_ecs::component::Component, S: Reads<C>;
    pub fn get_mut<C>(&mut self, id: i32) -> Option<&mut C>
        where C: bevy_ecs::component::Component<Mutability = Mutable>, S: Writes<C>;
    pub fn for_each_mut<C>(&mut self, f: impl FnMut(i32, &mut C))
        where C: bevy_ecs::component::Component<Mutability = Mutable>, S: Writes<C>;
    pub fn remove(&mut self, id: i32) -> bool where S: Structural;
    // insert（结构段需要）与 refresh_archetypes 接线：P5（Command）/ P6（NPC/商人/宝宝）
    // 前必须落地，见迁移文档 S4d-3 登记表的「P0 未交付项」。
}
```

组合视图（每段一个，宏生成）：`StageViewMut<'a>` 的字段等于白名单并集，
`StageEntities::view_mut(id) -> Option<StageViewMut<'_>>`。

> **落地形态（P2 / P2b 偏差登记）**：没有生成「每段一个 `StageViewMut` 字段视图」（bevy 的
> `QueryData` 无法在编译期枚举组件，宏也不可能知道白名单并集）。实际实现：
> * 组合视图用**元组**表达（`(&A, &B)` / `(&mut A, &B)`），并要求实现 `ViewAccess`
>   （`&C` = 读、`&mut C` = 写，元组 1..=15 逐项组合）；
> * `view_mut<Q: QueryData + ViewAccess>` 与只读的 `view_ref<Q: QueryData<ReadOnly = Q> + ViewAccess>`
>   在借出视图**之前**逐项核对 `ComponentRegistry` 与本段 `reads()`/`writes()`（P2b
>   P2-VIEW-MUT-BYPASS；空名单段不可绕过）；
> * `snapshot_spawn`（owned `EntitySpawn`）与 `entity_ref`（`EntityRef`）由 **world 侧**扩展
>   trait `StageSnapshotExt` 提供——`EntitySpawn`/`EntityRef` 是本 crate 类型，world-bevy 不能
>   反向依赖；装配体与 facade 共用 `EntityStore::from_components` / `EntityRef::from_components`。

### 3.4 段宿主：`StageWorld` 与 facade 的薄包装（D2）

```rust
pub struct StageWorld<'w> {
    pub entities: StageEntities<'w>,   // 方法级泛型 S，由段函数推断
    pub sim: &'w mut SimState,
}
impl core::ops::Deref for StageWorld<'_> { type Target = SimState; /* … */ }
impl core::ops::DerefMut for StageWorld<'_> { /* … */ }
```

`StageEntities` 若要同时被多段复用，把段类型 `S` 下放到方法级泛型
（`get_mut::<C, S>`），`StageWorld` 本身不携带 `S`。

facade 侧保留对外 API 的薄包装（宿主还在用）：

```rust
impl World {
    pub fn verify_map_time(&mut self, id: i32) -> bool {
        let mut sw = StageWorld::new(&mut self.entities, &mut self.sim);
        sw.verify_map_time(id)
    }
}
```

### 3.5 注册形状

```rust
pub const HEAD_STAGES: [TickStage; 16] = [ /* Clock .. OutputDrain */ ];
pub const TAIL_STAGES: [TickStage; 4]  = [ /* TailSuperManRefill .. OutputAoi */ ];

pub fn build_head_runner() -> TickRunner {
    let mut r = TickRunner::new();
    r.add_systems(TickStage::Clock, clock_stage_system);
    r.add_systems(TickStage::Command, command_stage_system);
    // … 每段一个独占系统 …
    r
}
// tail 同形
```

不新增 `impl TickRunner` 方法，因此 `tick_skeleton` 的方法白名单**不需要改**；
两个 runner 是两个 `TickRunner` 实例（`TickRunner::new` 的 20 段 `.chain()` 不变，
空阶段无害）。

段注册完整性由**测试**守：`HEAD_STAGES.len() + TAIL_STAGES.len() == STAGE_ORDER.len()`
且两个数组按 `STAGE_ORDER` 切分；再加 §2.7.1 的 `StageTrace` 序列牙。

---

## 4. 迁移顺序（串行 / 并行）

| 批 | 内容 | 写集 | 可并行？ |
| --- | --- | --- | --- |
| P0（前置，无行为变化） | `SlotTable` 迁资源 + `StageSpec`/适配器/注册表 + `StageTrace` + `sim_in_flight` 哨兵 | `world-bevy/src/{ecs.rs,tick.rs}`、`crates/world/src/{world.rs,entity.rs}` | 否（基线，单独一片） |
| P1（收口现有系统） | Clock / OutputDrain / SimHiterSnapshot / OutputAoi 收进 `head_runner`/`tail_runner`，`tick_head`/`tick_tail` 只留边界 | `world.rs` | 否（与 P0 同文件） |
| P2（只读/轻写段） | SimExpireHitters、SimExtraAbil、SimPlayerRunExpiry、SimMapEvents、TailSuperManRefill | `world.rs`、`entity.rs` | 否（同文件，逐段一个提交） |
| P3（RNG 门段） | SimDup、SimSettleDeaths + TailPlayerDeaths（同一实现点，**必须一个提交**） | `world.rs` | 否 |
| P4（结构/格子段） | SimSlaveCleanup、TailVerifyMapTime | `world.rs`、`slave.rs` | 与 P3 的文件段重叠（都改 `world.rs`），实际串行 |
| P5（最大段） | Command | `world.rs` | 否 |
| P6（模块段，需 D2） | SimSelfDeliver、SimBuff、SimMonsterAi、SimNpc、SimMerchant | `monai.rs`、`slave.rs`、`special_monster.rs`、`space_move.rs`、`npc.rs`、`merchant.rs`、`buff.rs` | **部分可并行**：模块文件互不重叠，但注册行都在 `world.rs`，必须串行合并；若把 `build_head_runner` 拆成各模块自己的注册函数，可完全并行 |
| P7（收口） | 删除 `tick_head`/`tick_tail` 里的业务内联代码；`tick()` 只剩「head 边界 + head_runner + 宿主段 + tail 边界 + tail_runner」 | `world.rs` | 否 |

**写集重叠的硬约束**：`crates/world/src/world.rs` 是所有批次的公共写集，任何两批
**不得并行**在同一 worktree 的同一文件上做迁移。唯一可真正并行的形态是 P6 的模块文件
（`monai.rs`/`npc.rs`/`merchant.rs`/`buff.rs`/`slave.rs`/`special_monster.rs`/`space_move.rs`），
前提是 P0/P1 先合入，且注册行按段拆进模块文件。

---

## 5. 风险与回退

| # | 风险 | 处置 |
| --- | --- | --- |
| R1 | `SlotTable` 迁资源改变 `EcsStore` 内部布局，可能影响 `iter_mut`/`get_mut` 热路径 | P0 单独一片、单测双跑（`order_follows_slots_not_query_order` 等既有 5 条 EcsStore 用例）；`tick_bench` 三档记录（P0 风险 R10 口径），超预算按 S2 的 `QueryState` 缓存法回退 |
| R2 | 适配器逐次 `world.get_mut::<C>` 的 `TypeId` 查表开销（review F4 已登记同类问题） | v1 正确性优先；释放后再做「每段缓存 `QueryState<组合视图>`」优化，必须带 `update_archetypes` 刷新钩子与基准对比 |
| R3 | 段顺序/RNG 漂移 | 每批跑 golden + parity + RNG trace；§2.7.1/2.7.2 的变异 |
| R4 | 模块函数改成 `StageWorld` 时漏改某个调用点 | 逐个批次编译驱动（改成新签名后全部调用点编译红，不存在静默漏改）；每批 `rg` 核对 |
| R5 | 白名单写宽（漏掉的越界） | 白名单取并集口径 + 评审逐段核 `rg` 实况；运行期镜像断言兜底 |
| 回退 | 每批一个提交，可独立回退；P0 不改行为，回退零语义风险 | — |

---

## 6. 待 Leader 裁决

**D1：槽位表进 bevy `World` 资源（`SlotTable`）。**
不改就无法满足 §1.3 的行为等价（Command/NPC/商人/宝宝四段的结构变更）。代价：改
`crates/world-bevy/src/ecs.rs` 的内部布局（对外方法签名不变）。
**若 Leader 不接受**，S4d-3 只能做「无结构变更的只读/轻写段」，其余段留到 S4d-4，
且 `tick_head`/`tick_tail` 的业务内联代码无法删干净（A3 达不到）。

**D2：S4d-3 写集扩到 7 个模块文件。**
issue 现有 write_set 只有 `crates/world/src/{world.rs,entity.rs}`、`world-bevy` 两个文件、
`bins/game-svr` 两个文件与 `tests/parity/**`。但余 18 段里 6 段的实现在
`impl World` / `fn(&mut World)` 里，改成段系统必须动它们的接收者类型：

- `crates/world/src/monai.rs`（`monster_ai_tick` 及同文件 6 个 `&mut World` 辅助）
- `crates/world/src/special_monster.rs`（`special_monster_run` 等 4 个）
- `crates/world/src/slave.rs`（`slave_list_cleanup_all`、`process_slave_mutiny`、
  `spawn_clone` 等；`impl World` 3 块）
- `crates/world/src/space_move.rs`（1 个 `&mut World` 入口）
- `crates/world/src/npc.rs` / `crates/world/src/merchant.rs` / `crates/world/src/buff.rs`
  （`impl World` 块）

最小扩写建议：上述七个文件。
**若 Leader 不扩**：S4d-3 缩到 P0–P5（只做 world.rs/entity.rs 能覆盖的 11 段），
AI/NPC/商人/buff/自投递/宝宝清表六段另开 S4d-4。

**D3：`TickOrder` / `SlotOrder` 的删除口径。**
`TickOrder` 无填入者、`SlotOrder` 与 `SlotTable` 重复。S4d-3 收口时删除还是标 deprecated？
（全局规则把「删除」列为需授权项；这里删的是本仓私有、无消费者的资源，但请 Leader 明确。）

**D4：`TickEntityIds` 保留为「段入口快照」还是彻底并入 `SlotTable`。**
本文建议保留一层薄包装（与已落地的 `AoiIds`/`SimHiterSnapshot` 兼容，减少一次性改动面），
S5 再并。

**D5：`SimState` 的资源形态是否改为「tick 全程常驻 + facade 字段访问器」。**
本文**不建议**（会破坏全仓 `world.rng` / `world.maps` 的字段访问形态，进而撕裂大量访问点）；
建议保持 S4d-1 的边界搬移 + F5 的 fail-fast 哨兵。

---

## 7. 与卡面 acceptance 的对应

| 判据 | 本文落点 |
| --- | --- |
| A1 设计先行、七项覆盖 | §2.1–§2.7 逐项；本文提交先于任何 `.rs` 改动 |
| A2 Leader 批注 | 本文 + §6 五个待裁决决定 |
| A3 段系统化、每段独立可回退提交 | §4 的 P0–P7 分批（每批一个提交） |
| A4 行为等价（golden/RNG） | §2.7.4 全清单 + `TickRecord`/RNG 次序表（§2.1） |
| A5 变异证据 | §2.7.1（删注册）、§2.7.2（丢写回）、§2.7.3（适配器负例） |
| A6 五门绿 | §2.7.4 末尾 |
| A7 不降级（禁多线程 / 不删 world / 不放宽 golden） | §3.5 两个 `TickRunner` 实例仍用 `SingleThreadedExecutor`；本文不删任何 crate |

---

## 8. 落地时的写集核对命令

每段迁移前，用下面的命令复核白名单（`crates/world/**` 是高 churn 区，**文档不写行号**）：

```text
rg -n "entities\.(get|get_mut|iter_ref|for_each_mut|insert|remove|spawn_cloned)" <段实现所在文件>
rg -n "\.(vitals|flags|life_cycle|appearance|buff|ai|slave|player_run|attribution|recovery|pvp|drop|spawn|inventory|merchant_state|npc_state|monster_class|ability|progression|group|revival|transform|map_binding|entity)\b" <段实现所在文件>
rg -n "self\.(rng|rng_attack|pending_|fire_burn_events|dig_out_events|maps|sessions|process_run_counts|npc_position|merchant_position)" <段实现所在文件>
```

三条命令的输出与该段在 §2.1 的白名单逐项对照；**写集以命令实况为准**，本文表格是设计
基线（P0 文档同样的口径：「读点/写点列的是主消费面，不是穷举」）。
