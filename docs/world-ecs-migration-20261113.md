# World 全量 ECS 迁移 P0：组件图 / 资源图 / tick 系统图（S3+）

> 卡片：`openmir2-world-ecs-migration`（tier R，P0 = 架构设计与组件骨架，**不改生产行为**）。
> 日期：2026-11-13。分支：`card/world-ecs-migration`，base `9f8cdf248`。
> 组件骨架落点：`mir2-rs/crates/world-bevy/src/components.rs`（P0 只编译、只做 round-trip 测试，不接生产）。

---

## 0. 结论摘要（先读这一节）

1. **这是 S3+ 的完整组件化路线**：把 `crates/world/src/entity.rs` 的 **151 字段大组件 `Entity`**
   拆成 **25 个细粒度 ECS 组件**（151/151 字段全覆盖），而不是「再加几个 tick 系统」。
   P0 交付组件布局与适配器形状；S3 起按切片把字段从 `Entity` 搬出、把段逻辑搬进系统。
2. **与旧卡的关系**：旧卡 `openmir2-world-bevy-migration` 的 S1（EcsStore + TickRunner 骨架）
   与 S2（性能调优）**已完成并合入** master `f5170a8e`（approve `e6d8de59`）；S3+ 当时按
   `docs/AGENTS.project.md` §4.2.2 与 §6.0-5 **暂缓**（避免与 M2/M3 验收争写集）。
   Owner 2026-11-13 指令要求全量迁移 ⇒ 视为**解冻**，本卡是新执行卡。**不重复 S1/S2**。
   **退役目标（旧卡 owner 裁决 `de11cdd4`，必须纳入）**：最终形态**不是**两个 crate 长期并存，
   而是 `world-bevy` 承接 entity/tick/map+aoi/combat/session 全部 API（含其余模拟模块）后
   **删除 `crates/world` 与 workspace 成员**；`tests/parity` 与 `tick_bench` 改指向 `world-bevy`。
   S3–S4 = map+aoi / combat+session 及其余模块，S5 = 切换并删除（§6.3/§6.7）。
3. **tick 系统图有一条硬结论**：现有 4 阶段 `Clock/Command/Simulate/Output` 在
   「每段一个系统、构建期歧义即失败」的目标下**需要扩展**。原因是 `TickRunner` 的歧义检测是
   `LogLevel::Error`，而所有模拟段都要 `&mut Entity`/组件 + `ResMut<Rng>` ⇒ 同阶段必冲突。
   替代方案是**一个单体 exclusive `Simulate` 系统**（一个 `fn(&mut World)` 串行跑完所有段，零歧义）。
   评审 P2-4 指出「4 阶段不可能」是**过度表述**；Leader scope_ruling 选了前者：
   S3a 做**一次 TickStage 契约变更**，把 Simulate 展开成「每段一个阶段」的线性链（见 §4）。
   两条路都能保住「显式顺序 + 构建期歧义即失败」，选择「每段一阶段」是为了后续每段独立成系统。
4. **S3a 试点切片 = `DupState`（3 字段）**：两个消费点（`World::check_dup_obj` + 宿主
   `apply_get_start_point_after_tick`）、无存档面、已有专项用例
   `crates/world/tests/check_dup_obj_vectors.rs`，是最小可独立验证的落点（见 §5.2）。
   **S3b 已落地**：`Transform`（4）+ `MapBinding`（6）从 `Entity` 删除（剩 **138** 字段），
   真值在 `world-bevy` 组件；读取经 `EntityStore::get` 的三组件只读视图 `EntityRef`，
   写入经 `transform_mut`/`map_binding_mut`；`World::maps` 在 tick 边界移入
   `mir2_world_bevy::MapsRes`，AOI 刷新是注册在 `TickStage::OutputAoi` 的系统
   （`aoi::output_aoi_system`，按 `AoiIds` 槽位顺序扫描，不迭代 `Query`）。
5. **P0 未改任何生产路径**：`crates/world` 的 `Entity` / `EntityStore` / `World` 一行未动；
   新增组件骨架与 `#[cfg(test)]` 适配器；五门全绿、golden/parity 不变。

---

## 1. 起点：S1/S2 已有什么，S3 从哪里开始

| 面 | S1/S2 已交付（`f5170a8e`） | S3+ 起点（本卡） |
| --- | --- | --- |
| 实体存储 | `World::entities: EntityStore` 已是 `mir2_world_bevy::EcsStore<Entity>` 的薄适配层；槽位顺序 / LIFO 空闲槽 / 代际号语义保持 | `EcsStore<T>` 当前**只承载单一组件 `T`**；需要扩展「同一实体上的多组件」访问（S3a 前置，见 §6.2） |
| tick 骨架 | `TickRunner`（`Schedule` + `SingleThreadedExecutor`）、`TickStage::{Clock,Command,Simulate,Output}`、`STAGE_ORDER`、构建期歧义检测 | `Command` / `Simulate` / `Output` **空**；只有 `advance_clock_system` 注册在 Clock |
| 顺序资源 | `SlotOrder`（槽位顺序句柄）、`TickOrder`（本 tick 业务 id 顺序）；`bevy_bridge::collect_tick_order_system` 已写好 | `collect_tick_order_system` **尚未注册**；`TickOrder` 目前无填入者 |
| 业务逻辑 | 全部仍在 `World` 的串行方法里（`tick_head` / `tick_tail` + 各模块函数） | 逐段迁入系统（§4/§5） |
| **退役路线** | 旧卡 owner 裁决 `de11cdd4`：**目标不是长期并存** | S1/S2 = entity+tick/存储调优；S3–S4 = map+aoi / combat+session（含其余模拟模块）；S5 = `tests/parity`/`tick_bench` 切 `world-bevy`、删 `crates/world` 与 workspace 成员（§6.3/§6.7） |
| 判据 | 三份 golden（`tests/parity/world/golden/{basic,session,combat}.txt`）逐字节不变；S2 相对门禁 P50 ≤ 10%、P99 中位数 ≤ 30%（500/1000/2000 三档） | 每切片沿用同一判据，**diff 必须为 0** |

S1/S2 的四条硬约束（沿用，不重新解释）：

1. 单线程 executor（禁 `MultiThreadedExecutor` / `bevy_tasks` 并行 API / `rayon` / `crossbeam`）；
2. 遍历顺序只来自显式槽位索引，`Query` 迭代序不是契约；
3. `TickStage` 是唯一系统顺序契约（注册入口收窄 + 歧义检测 = Error）；
4. 迁移验收 = 原判据不降级（golden/parity diff = 0、P99 不劣化）。

---

## 2. 架构边界（不得越界）

照 `docs/AGENTS.project.md` §4.2.2：

```
网关 Actor（tokio per-connection：连接/协议/会话/认证/限流）
    -> 输入 channel（玩家操作意图）
地图/副本：无头 bevy_ecs World（权威模拟：移动/战斗/AI/Buff/掉落/技能判定；固定 tick）
    -> 输出 channel（命令/事件）
同步层（自研：AOI 快照 + 带宽优先级）
全局服务 Actor（聊天/组队/公会/拍卖/邮件/DB）
```

- **ECS 只做地图/副本世界模拟**。连接/会话/认证/限流、聊天/组队/公会/拍卖/邮件、持久化与
  崩溃恢复、跨图分布式，**都不进 ECS**。
- `World::sessions: SessionRegistry` 只承载「谁在图内、离开事件」这类**模拟侧存在性**，
  不是认证/连接会话；字段在迁移中保持 facade（宿主所有）。按退役路线，`session.rs` 的**代码**
  在 S4 迁入 `world-bevy`（world 要能删），但它的边界不变：连接/认证/限流仍是 actor。
- `GroupState` 是**实体侧**组队状态（`GroupOwner`/`GroupMembers` 等 oracle 玩家字段）；
  组队协议与跨实体编排仍属全局服务 actor，不因本组件进 ECS。
- `MerchantState` 是商人实体的模拟状态；交易/补货的持久化仍在宿主/DB 侧。
- 仓库**没有** `World.cheats` 字段：作弊面是实体位（`admin_mode`/`super_man`）与宿主
  `bins/game-svr` 的 GM 命令处理器（actor 侧），不新增 ECS 资源。

---

## 3. 组件图（151 字段 → 25 组件）

### 3.1 总表

| # | 组件 | 字段数 | 形态 | 说明 |
| --- | --- | --- | --- | --- |
| C1 | `CoreIdentity` | 2 | 具体 | `id` / `name`；`id` 同时是存储键 |
| C2 | `Transform` | 4 | 具体 | `x` / `y` / `map_id` / `view_range` |
| C3 | `Appearance` | 5 | 具体 | `race` / `race_img` / `appr` / `light` / `name_color` |
| C4 | `Vitals` | 13 | 具体 | HP/MP/DC/AC/MAC/命中/敏捷/抗性/不死加成/HitSpeed |
| C5 | `AbilitySnapshot<B>` | 1 | 泛型载荷 | `abil: Option<BaseAbil>` |
| C6 | `Progression` | 6 | 具体 | 职业/不死系/等级/经验/杀怪经验 |
| C7 | `Flags` | 13 | 具体 | 隐身/管理员/石化/SuperMan/攻击模式等布尔位 |
| C8 | `PvpState` | 5 | 具体 | PK 点/标记/颜色时刻/配偶/攻城图死亡位 |
| C9 | `LifeCycle` | 8 | 具体 | 死亡/幽灵/时刻/可复活/已结算/直接 Die |
| C10 | `MapBinding<V,E>` | 6 | 泛型载荷 | 入图位/可见集/可见事件/活跃位/tick 标记/VerifyTick |
| C11 | `BuffState` | 14 | 具体 | 毒/状态位/`status_time`/心灵启示/魔法盾/`ExtraAbil` |
| C12 | `PlayerRunState` | 13 | 具体 | 幸运/倍率/随机号/烈火/双龙 |
| C13 | `DupState` | 3 | 具体 | 重叠推挤（**S3a 试点**） |
| C14 | `Inventory<S>` | 1 | 泛型载荷 | `use_items: [UseItemSlot; 13]` |
| C15 | `DropState<D>` | 5 | 泛型载荷 | 金币/掉落物/三个禁止掉落门 |
| C16 | `CombatAttribution` | 4 | 具体 | `LastHiter` / `ExpHitter` + 两个时刻 |
| C17 | `Recovery` | 3 | 具体 | `HealthTick` / `SpellTick` / `AutoRecoveryTick` |
| C18 | `Revival` | 4 | 具体 | 复活戒指/火球戒指粘滞位 + 复活时刻 |
| C19 | `SlaveState` | 11 | 具体 | 主人/宝宝表/克隆体/叛变与召唤等级 |
| C20 | `SpawnBinding<C,D>` | 4 | 泛型载荷 | 城堡绑定/MonGen 下标/ZenTime/ctor 抽数 |
| C21 | `MonsterClassState` | 19 | 具体 | 类专用字段（CowKing/Elf/SoccerBall…） |
| C22 | `AiState<A>` | 1 | 泛型载荷 | `AnimalState` 整块 |
| C23 | `MerchantState<M>` | 1 | 泛型载荷 | `Option<MerchantState>`（商人身份门） |
| C24 | `NpcState<N>` | 1 | 泛型载荷 | `Option<NpcState>`（固定 NPC 身份门） |
| C25 | `GroupState` | 4 | 具体 | 组队四字段 |

合计：**25 组件 / 151 字段**。泛型载荷 8 个（C5/C10/C14/C15/C20/C22/C23/C24）是
**依赖方向的迁移缝**：`world-bevy` 不能依赖 `world`，P0 不复制 100+ 字段的镜像结构
（复制会立刻产生两份真值）；S3 落点再决定把载荷类型的数据定义搬进 `world-bevy`
（方法留 `world`）还是放进新的 `mir2-world-types` 小 crate。

**退役约束**：泛型载荷只是过渡。按 `de11cdd4` 的退役路线，S3 起必须把载荷数据的**定义**
迁入 `world-bevy`（或 `mir2-world-types`），S4 收口为具体类型；S5 删除 `crates/world` 时
`components.rs` 的泛型参数、`[dev-dependencies] mir2-world` 与 `#[cfg(test)]` 适配器一并删除
（它们只为「两 crate 并存期」服务）。

### 3.2 逐组字段与读写点

> 位置一律写**文件 + 符号**（不写行号：`crates/world/**` 是高 churn 区，行号锚会腐烂）。
> 「读点/写点」列的是**主消费面**，不是穷举；迁移时以 `rg` 实况为写集依据。

#### C1 `CoreIdentity`：`id`, `name`

- 读点：`world.rs::World::tick_head`/`tick_tail`（`TickRecord.processed`）、`aoi.rs::visible_snapshot`、
  `session.rs::SessionRegistry`、`merchant.rs::merchant_ids_sorted`、`npc.rs::npc_ids_sorted`、
  `bins/game-svr` 的 downlink/feature 帧面。
- 写点：`world.rs::World::add_entity`、`mongen.rs::initialize_monster`、`merchant.rs::insert_merchant`、
  `npc.rs::insert_npc`；生成后不可变。
- 迁移序：**S4c（最后）**。`id` 是 `EcsStore` 索引键（组件与索引各一份、以索引为准），
  `name` 帧面引用广。

#### C2 `Transform`：`x`, `y`, `map_id`, `view_range`

- 读点：`aoi.rs::search_view_range_with_updates` / `search_view_range_death` / `update_visible_gay`、
  `map.rs::MapGrid` 的格子 API（`cell_valid` / `add_map_object` / `remove_map_object`）、
  `monai.rs::walk_to` / `wondering` / `goto_target_xy`、`space_move.rs::space_move_get_rand_xy`、
  `world.rs::verify_map_time`。
- 写点：`world.rs::try_step` / `enter_world` / `leave_world` / `push_now` / `rush_now`、
  `monai.rs::walk_to`、`slave.rs::recall_slave_target`、`castle.rs` 的防御单元落图。
- 迁移序：**最后（S3b）**，与 `MapBinding` + AOI 同切片。`x`/`y`/`map_id` 是格子索引键，
  字段搬出后必须与 `MapGrid` 的格子登记**原子**更新（不能出现「组件已改、格子未改」的中间态）。

#### C3 `Appearance`：`race`, `race_img`, `appr`, `light`, `name_color`

- 读点：`monai.rs`（race 门/主动寻敌）、`combat.rs::race_is_animal` / `is_attack_target_*`、
  `aoi.rs`（观察者分类）、`bins/game-svr` 的 feature/downlink（`race_img`/`appr`/`light`/`name_color`）。
- 写点：`mongen.rs::apply_monster_class_constants` / `initialize_monster`、
  `merchant.rs::insert_merchant`、`npc.rs::insert_npc`、`world.rs` 的 `name_color` 更新。
- 迁移注意：`race`（行为轴）与 `race_img`（外观轴）**不是同一字段**（商人 `race=10` /
  `race_img=50`），组件里也保持两个字段。`race` 使用面约 96 处，本组**不按「外观=低风险」
  排早**。
- 迁移序：S4b（与 `LifeCycle`/`Flags` 同波）。

#### C4 `Vitals`：`hp`, `max_hp`, `mp`, `max_mp`, `hit_speed`, `dc`, `ac`, `mac`, `hit_point`, `speed_point`, `anti_magic`, `anti_poison`, `undead_power`

- 读点：`combat.rs`（`get_attack_power*` / `attack_success` / `struck_damage`）、
  `buff.rs::damage_health`、`world.rs::clamp_overflow_abilitys`、`monai.rs` 的命中/伤害段。
- 写点：`buff.rs::damage_health`、`combat.rs`、`world.rs::tick_tail` 的 HP/MP 钳制、
  `bins/game-svr` 的 `apply_ability_to_entity`（宿主 RecalcAbilitys 等价物）。
- 存档面：HP/MP/DC/AC/MAC 等经宿主会话 `wabil` 同步进出存档（`bins/game-svr` 的
  `build_save_record` / `apply_ability_to_entity`），组件化后**同步函数仍是唯一存档入口**。
- 迁移序：S4a。

#### C5 `AbilitySnapshot<B>`：`abil`

- 读点：`slave.rs::apply_derived_slave_recalc`、`monai.rs` 的 `RecalcAbilitys` 等价段。
- 写点：生成/克隆/奴隶重算路径。
- 迁移序：与 C4 同片（懒取快照语义不能被重排）。

#### C6 `Progression`：`job`, `life_attrib`, `level`, `mon_exp`, `exp`, `max_exp`

- 读点：`combat.rs`（经验结算）、`world.rs::settle_deaths`、`bins/game-svr` 升级/经验段。
- 写点：击杀经验、升级、脚本/DB 登录。
- 存档面：`level` / `exp` 由宿主经验段写回会话记录。
- 迁移序：S4a。

#### C7 `Flags`：`invisible`, `hide_mode`, `fixed_hide_mode`, `ob_mode`, `nasty_mode`, `attack_mode`, `in_safe_zone`, `admin_mode`, `stone_mode`, `super_man`, `rush_mode`, `want_ref_msg`, `stick_mode`

- 读点：`aoi.rs`（过滤器与阻挡）、`monai.rs`（隐藏门/攻击目标门）、`combat.rs`（保护目标/攻击门）、
  `world.rs`（移动/推挤/死亡路由）。
- 写点：GM/脚本命令（宿主）、`bins/game-svr` 登录、魔法效果段。
- 迁移注意：`super_man` 的「回满」发生在 `tick_tail` 最前（oracle `BaseObject.Run` 首段），
  不能与死亡结算换序。
- 迁移序：S4b。

#### C8 `PvpState`：`dear_human`, `pk_point`, `pvp_flag`, `pvp_name_color_tick`, `die_in_fight3_zone`

- 读点：`combat.rs::is_attack_target_player*`、`world.rs::check_pk_status`、`death_drop`。
- 写点：`world.rs::set_pk_flag` 等价段、GM 命令、登录。
- 迁移序：S4a（写入面窄、无 RNG）。

#### C9 `LifeCycle`：`death`, `ghost`, `death_tick`, `ghost_tick`, `can_re_alive`, `re_alive_tick`, `death_settled`, `die_called`

- 读点：`world.rs::settle_deaths` / `tick_tail`、`aoi.rs`（死亡扫描/幽灵过滤）、
  `monai.rs`、`mongen.rs`（复活/幽灵清理门）。
- 写点：`world.rs::settle_deaths`、`monai.rs`、`mongen.rs`、`slave.rs::slave_list_cleanup_all`。
- 迁移序：S4b。`death`（约 58 处）/`ghost`（约 39 处）跨段引用多，不能早拆。

#### C10 `MapBinding<V,E>`：`add_to_mapped`, `visible_actors`, `visible_events`, `is_visible_active`, `processed_tick`, `verify_tick`

- 读点：`aoi.rs` 全部扫描/差集函数、`monai.rs::monster_self_scans`。
- 写点：`aoi.rs::update_visible_gay` / `search_view_range*`、`world.rs::verify_map_time`、
  `session.rs::enter_map_at`。
- 迁移序：S3b（与 Transform/AOI 原子切片）。

#### C11 `BuffState`：`poisoning_tick`, `green_poisoning_point`, `char_status`, `char_status_ex`, `status_time`, `transparent`, `show_hp`, `show_hp_tick`, `show_hp_interval`, `magic_shield`, `un_magic_shield`, `extra_abil`, `extra_abil_times`, `extra_abil_flag`

- 读点：`buff.rs`（`BuffSystem::do_work` / `get_char_status` / `damage_health` 读毒）、
  `magic.rs` / `magic_effect.rs`（隐身/盾/心灵启示）、`world.rs::expire_extra_abil`。
- 写点：`buff.rs`、`magic.rs`、`magic_effect.rs`、`world.rs`。
- 迁移注意：buff 段**不抽 RNG**（源码注释是显式口径）；`status_time` 只有写入点、
  没有周期衰减（oracle 递减循环被注释），不要「补」衰减。
- 迁移序：S4b。

#### C12 `PlayerRunState`：`luck`, `power_rate`, `power_rate_time`, `kill_mon_exp_rate`, `kill_mon_exp_rate_time`, `exp_rate_tick`, `random_no`, `bo_power_item`, `power_item`, `fire_hit_skill`, `latest_fire_hit_tick`, `twin_hit_skill`, `latest_twin_hit_tick`

- 读点：`combat.rs`（`get_attack_power_player` 的 Luck）、`world.rs::expire_player_run_state`、
  `bins/game-svr` 的 P27 经验/攻击倍率段、脚本 `<$RANDOMNO>`（待接线）。
- 写点：`bins/game-svr` 的 P27/P32 宿主段、技能层（烈火/双龙未移植，当前恒 false）。
- 迁移序：S4b。

#### C13 `DupState`：`bo_duplication`, `dup_start_tick`, `check_dup_obj_tick`

- **消费点有两个**（P0 评审 P2-1 修正，不是「唯一消费点」）：
  1. `world.rs::check_dup_obj`（重叠推挤段：3s 门、≥2 首次观测、推挤门）；
  2. **宿主** `bins/game-svr/src/lib.rs::apply_get_start_point_after_tick` 每 tick 读取在线玩家的
     `check_dup_obj_tick`，用 `dup_tick_fired(tick == now)` 决定是否覆盖 `GetStartPoint` 的
     `HomeMap/HomeX/HomeY`。
- 写点：`world.rs::check_dup_obj`（门内落 `now`）、`EntityStore::insert`（ctor 缺省 `false/0/0`）、
  `world.rs::add_entity` / `session.rs::enter_map_at`（按 ctor 时刻重盖 `check_dup_obj_tick`）。
- 存档面：无（纯内存）。
- **S3a 已落地**：三字段从 `Entity` 删除，真值在 `world-bevy` 的 `DupState` 组件；
  读写经 `EntityStore::dup_state` / `dup_state_mut`（业务模块不持有裸 `Query`），宿主读点同步改为
  `world.entities.dup_state(actor_id)`。专项用例 `crates/world/tests/check_dup_obj_vectors.rs`。


#### C14 `Inventory<S>`：`use_items`

- 读点：`combat.rs`（`StruckDamage` 的 13 槽耐久循环）、`death_drop.rs::DropUseItems`、
  `slave.rs`（装备 Shape 判定）。
- 写点：`bins/game-svr` 的 `sync_use_items_to_entity` / `sync_use_item_slot_to_entity`、
  穿脱装备段。
- 存档面：`DuraMax` / `Desc[14]` 随槽进存档（`bins/game-svr` 的 `human_items_for_save`、
  `build_save_record`），迁移不得丢。
- 迁移序：S4c（与存档面一起最后动）。

#### C15 `DropState<D>`：`gold`, `drop_items`, `angry_ring`, `no_drop_use_item`, `no_item`

- 读点：`world.rs::settle_deaths` / `death_drop.rs`、`mongen.rs`（生成时预算）。
- 写点：`mongen.rs` / `combat.rs`（掉落预算）、GM `GIVE`。
- 迁移注意：掉落预算在**生成时**一次性消费 RNG（S1/D3 结论），不是每 tick 成本；
  迁移不得把它挪进每 tick 系统。
- **命名登记**：组件字段是 `items`，对应 `Entity.drop_items`（`Vec<DropItem>` 类型不变）；
  P0 适配器显式映射 `drop_items -> items`，这是 151 字段里**唯一**的改名。
- 迁移序：S4a。

#### C16 `CombatAttribution`：`last_hiter`, `exp_hitter`, `last_hiter_tick`, `exp_hitter_tick`

- 读点：`world.rs::expire_hitters` / `settle_deaths`、`combat.rs`、`slave.rs`、`death_drop.rs`。
- 写点：`combat.rs` 的 `set_last_hiter` 等价段。
- 迁移注意：PL-1 硬前置——玩家 `LastHiter` **快照**必须发生在 `expire_hitters` 之前
  （供宿主段 P32 读清理前的值）。系统阶段顺序必须保留这条。
- 迁移序：S4a。

#### C17 `Recovery`：`health_tick`, `spell_tick`, `auto_recovery_tick`

- 读点/写点：`stamina.rs`（恢复段）、`monai.rs`、`world.rs`。
- 迁移序：S4a。

#### C18 `Revival`：`revival`, `revival_tick`, `un_revival`, `flame_ring`

- 读点：`bins/game-svr` 的 P32 宿主段（`revival_tick` 60s 门）、`combat.rs`。
- 写点：宿主 `RecalcAbilitys` 等价段（粘滞锁存：`|=`，**没有**置 false 的写入点）。
- 迁移序：S4b（与宿主段边界一起验证）。

#### C19 `SlaveState`：`master`, `slave_relax`, `slave_list`, `special_children`, `is_slave`, `is_clone`, `slave_exp_level`, `slave_make_level`, `master_royalty_tick`, `master_tick`, `class_slave_list`

- 读点：`slave.rs`（`slave_list_cleanup_all` / `check_royalty` / `recall_slave`）、
  `monai.rs`（跟随/叛变）、`aoi.rs`（克隆体观察者分类）。
- 写点：`slave.rs`、`mongen.rs`、`special_monster.rs`。
- 迁移注意：`special_children`（BeeQueen/SpiderHouse）与 `class_slave_list`
  （ScultureKing `_mSlaveObjectList`）是**不同 C# 字段**，不得合并。
- 迁移序：S4c（AI 切片的一部分）。

#### C20 `SpawnBinding<C,D>`：`castle_binding`, `mon_gen`, `mon_gen_zen_time`, `ctor_draws`

- 读点：`monai.rs`（城堡门/墙 Run、MonGen 死亡门）、`mongen.rs`（复活/ZenTime）。
- 写点：`mongen.rs::initialize_monster` / `apply_monster_class_constants`、`castle.rs` 挂图。
- 迁移序：S4a（生成时写一次，无 RNG 重放问题）。

#### C21 `MonsterClassState`：`come_out_value`, `attack_range`, `use_magic`, `danger_level`, `n550`, `class_attack_tick`, `bo_is_first`, `dig_down_tick`, `appear_start_tick`, `guard_direction`, `siege_lock_count`, `jump_time`, `crazy_ready_mode`, `crazy_king_mode`, `crazy_count`, `crazy_ready`, `crazy_time`, `old_hit_time`, `old_walk_time`

- 读点/写点：`monai.rs`（MB 系列 `Run` 覆写）、`mongen.rs::apply_monster_class_constants`、
  `ctor_fold.rs`、`special_monster.rs`。
- 迁移序：S4b（与 AI 切片相邻，但字段全在少数类分支里，可先拆）。

#### C22 `AiState<A>`：`ai`

- 读点/写点：`monai.rs`（约 173 处，全部字段里最多）、`aoi.rs`（`search_tick` 自扫门）、
  `world.rs::tick_tail`。
- 迁移注意：**不在 P0 拆 `AnimalState` 的内部字段**——拆内部字段会产生 40+ 个新引用面，
  是另一个量级的改动。先整块搬。
- 迁移序：S4c（最后的大块之一）。

#### C23 `MerchantState<M>`：`merchant`

- 读点/写点：`merchant.rs`（`insert_merchant` / `merchant_tick` / 交易 API）、
  `monai.rs`（跳过商人）、`aoi.rs`（`entity_cell_type`）。
- 迁移注意：`Option` 的 `Some/None` 是商人**身份门**（不按 race 判定）。
- 迁移序：S4c。

#### C24 `NpcState<N>`：`npc`

- 读点/写点：`npc.rs`（`insert_npc` / `npc_tick` / `npc_run`）、`monai.rs`（显式跳过）。
- 迁移注意：`Some` ⇒ 固定 NPC；`monster_ai_tick` 与 `npc_tick` **不得双驱**。
- 迁移序：S4c。

#### C25 `GroupState`：`allow_group`, `allow_group_re_call`, `group_owner`, `group_members`

- 读点/写点：`party.rs`（`party_create_group` / `party_add_group_member` / 死亡退组）、
  `world.rs`、`bins/game-svr` 组队命令段。
- 边界：组队**服务编排**仍是全局 actor；本组件只是实体侧字段。
- 迁移序：S4b（与玩家运行态同片，字段引用面不大）。

### 3.3 字段覆盖与迁移顺序的输入

- **覆盖**：25 组件合计 2+4+5+13+1+6+13+5+8+6+14+13+3+1+5+4+3+4+11+4+19+1+1+1+4 = **151**。
  P0 的 round-trip 用例（`components::tests::split_and_apply_round_trip_all_151_fields`）
  对每个字段赋非缺省值后做 `Entity -> 25 组件 -> Entity`，用全字段 `Debug` 相等作判据。
  **S3a/S3b/S4a/S4b 起**：`DupState` 的 3 个字段（S3a）、`Transform` 的 4 个与 `MapBinding` 的 6 个
  字段（S3b）、S4a 的 41 个字段（Vitals 13 + AbilitySnapshot 1 + Progression 6 +
  SpawnBinding 4 + DropState 5 + CombatAttribution 4 + Recovery 3 + PvpState 5）、
  S4b 的 80 个字段（Flags 13 + LifeCycle 8 + Appearance 5 + MonsterClassState 19 +
  PlayerRunState 13 + GroupState 4 + Revival 4 + BuffState 14）已从 `Entity`
  删除（真值在组件），`Entity` 剩 **17** 字段（S4c 待迁的 CoreIdentity 2 + AiState 1 +
  SlaveState 11 + MerchantState 1 + NpcState 1 + Inventory 1）；用例更名为
  `split_and_apply_round_trip_all_remaining_fields`（只比较 `Entity` 主组件），
  对应适配器随字段删除（改用 `EntityRef` 只读视图 / `EntityMut` 可写视图 / `EntitySpawn`
  生成期载体）。
- **使用量**（Leader 统计，作为拆分顺序输入）：`ai` 173、`x` 101、`race` 96、`y` 95、
  `map_id` 88、`hp` 62、`death` 58、`ghost` 39。
  ⇒ **热点字段最后拆**；叶子组先拆。这和 §5 的波次一致。
- **迁移波次**：
  1. **S3a 试点**：`DupState` + 系统化骨架（TickStage 扩展）。
  2. **S3b 地图/AOI**：`Transform` + `MapBinding` + AOI + `MapsRes`（世界脊柱）。
  3. **S4a 叶子/生成期/属性**：`SpawnBinding`、`DropState`、`CombatAttribution`、`Recovery`、
     `PvpState`、`AbilitySnapshot` + `Vitals` + `Progression`。
  4. **S4b 玩家/Buff/生命周期**：`PlayerRunState`、`GroupState`、`Revival`、`BuffState`、
     `LifeCycle`、`Flags`、`Appearance`、`MonsterClassState`。
  5. **S4c AI/宝宝/商人/NPC/物品**：`AiState` + `SlaveState`（含 `process_run_counts` 旁挂表
     → 组件）、`MerchantState`、`NpcState`、`Inventory`、`CoreIdentity`。
  6. **S5 切换并删除**：parity/tick_bench/bins 改指 `world-bevy`；删 `crates/world`。

### 3.4 组件 `Default` 与 `EntitySpawn::new` 的非零缺省差异（迁移防漂移）

P0 组件大多 `derive(Default)`，但 `EntitySpawn::new`（各 `BaseObject` ctor 等价物）有**非零 ctor 缺省**。
**任何 S3+ 路径都不得用 `Component::default()` 直接造实体**，必须走统一 bundle 构造器（复刻
`EntitySpawn::new` 的值），否则行为会静默漂移。逐项差异（评审 P2-5）：

| 组件 | 字段 | `EntitySpawn::new` 缺省 | 派生 `Default` |
| --- | --- | --- | --- |
| `Transform` | `view_range` | 5 | 0 |
| `Appearance` | `name_color` | 255 | 0 |
| `Vitals` | `hp` / `max_hp` | 100 / 100 | 0 / 0 |
| `Progression` | `level` | 1 | 0 |
| `PlayerRunState` | `power_rate` / `kill_mon_exp_rate` | 100 / 100 | 0 / 0 |
| `BuffState` | `poisoning_tick` | `BUFF_EPOCH_MS` | 0 |

其余组件（含 S3a 的 `DupState`：`false/0/0`）的派生 `Default` 与 ctor 缺省逐字段相同。
`DupState` 的缺省由 `EntityStore::insert` 统一插入；落图时刻的重盖由 `add_entity`/`enter_map_at` 负责。

> **S4a 落实（2026-11-13）**：非零 ctor 缺省由 `EntitySpawn` 载体给出（`CTOR_HP`/`CTOR_MAX_HP`/
> `CTOR_LEVEL`），`EntityStore::insert` 消费 `SpawnParts` 整 bundle 挂 8 个 S4a 组件（缺任一即
> panic）。组件 `Default` 仍是 0/0/0；契约由 `s4a_component_ctor_defaults_match_entity_new`
> 与 `insert_uses_spawn_ctor_bundle_not_component_defaults` 两条用例钉住（`Transform::default()`
> 误用会触发后者）。

---

## 4. 资源图（`World` 字段 → Resource / facade）

### 4.1 分类

| 类别 | `World` 字段（代表） | S3+ 归宿 | 理由 |
| --- | --- | --- | --- |
| 时钟 | `clock_ms`, `tick_seq`, **`game_time`** | `clock_ms`/`tick_seq` 已是 `TickClock` 资源；`game_time`（昼夜桶，`World::refresh_game_time` 刷新）随 tick 边界同步，S3+ 可并入 `WorldClockRes` | 现在两处各一份（facade 写进资源、跑完读回），必须收敛；`game_time` 是可变的模拟时钟派生值，**不是**只读配置 |
| 随机源 | `rng`, `rng_attack` | `RngRes` / `RngAttackRes`（已存在 `RngRes`） | RNG 消费者次序是契约；资源只能有**一个**所有者 |
| 世界数据 | `maps` | `MapsRes`（S3b） | 格子表与 AOI 同切片；`SessionRegistry` 仍 facade |
| 会话存在性 | `sessions` | **facade**（模块代码 S4 迁 `world-bevy`） | 连接/认证属 actor；`SessionRegistry` 只承载进/出图与 depart 事件 |
| buff 系统 | `buffs` | `BuffRes`（S4b） | `BuffSystem` 是实体外的旁挂状态 |
| 城堡/行会 | `castles`, `actor_guilds`, `guild_allies`, `guard_castle_crime`, `castle_name_colors` | `CastleRes` 等（城堡切片） | 与实体切片解耦；公会**服务**仍 actor |
| 只读配置 | `item_catalog`, `mon_random_add_value`, `need_exps`, `combat_config`, `party_cfg`, `b4_cfg`, `b5_cfg`, `pet_slave_cfg`, 各 `*_time`/`*_rate`/`*_level`；名字表 `bee_name`/`spider_name`/`zuma_names` | 不可变 `WorldConfig` 资源包 | 宿主 tick 前灌入；系统只读（名字表是启动期装载后只读） |
| 命令输入 | `pending: VecDeque<Command>` | `CommandQueue` 资源（S3a） | Command 阶段的唯一输入面 |
| 输出/事件累积 | `pending_*`（AI 攻击、武器磨损、城堡结构、RefMsg、特殊生成…）、`fire_burn_events`, `dig_out_events` | 分阶段输出资源 + `TickRecord`（S3a+） | 每段产出、Output 阶段统一转抄 |
| 段内旁挂表 | `process_run_counts`, `attack_poison`, `attack_weapon_strong` | S4c/S4a 改成**实体组件**（按 id 查的 HashMap 是组件化的直接候选） | 旁挂表可消除哈希随机性/生命周期同步问题 |
| 段内游标 | `merchant_position`, `merchant_pass_tick`, `npc_position`, `npc_pass_tick`, `lamp_tick`, `pk_check_tick`, `spirit_mutiny_tick`, `event_sight`, `event_registered` | 随段迁入对应资源/组件 | 现在都是 facade 字段 |
| 段内初始化位 | `buff_pass_tick`, `lamp_tick_initialized`, `pk_check_tick_initialized` | 随所属段迁入对应游标资源 | 只是「首次运行」标记，不是模拟状态；与对应 tick 字段同所有者 |
| 诊断 | `last_tick`, `tail_phase_log`, `player_hiter_snapshot`, `by_map_scratch` | facade / scratch 资源 | `by_map_scratch` 是 AOI 复用缓冲，不能跨阶段共享 |
| 开关 | `ai_enabled`, `merchant_enabled`, `npc_enabled`, `buff_enabled`, `isolate_attack`, `slave_self_delay_enabled`, `refmessage_delay_enabled` | **facade** | 宿主起世界时设一次；不是模拟状态 |
| 驱动器 | `owner_thread`, `clock_runner` | `TickOwnerThread` 资源 + runner | 已部分在资源里 |
| 存储本体 | `entities: EntityStore` | facade（内含 bevy `World`） | 对外 API 不变；S3a 扩展多组件访问 |

> 本表是**类别级**分类（`World` 共 84 字段），不是逐字段枚举；归类规则是：可变模拟状态 →
> 资源/组件，只读启动配置 → `WorldConfig`，诊断/开关 → facade，存储/驱动器 → 现有资源或 facade。
> 评审 P2-3 指出的 `game_time`、`*_initialized`、名字表已显式列入上表。

### 4.2 所有权规则（避免双真值）

1. 每个字段**同一时刻只有一个所有者**：要么 facade 字段，要么资源/组件；不允许长期双写。
2. 过渡期的合法双写只有「**tick 边界一次性同步**」：进入 schedule 前 facade → 资源，
   跑完资源 → facade（`clock_ms`/`tick_seq` 现在就是这个形态）。同步点必须集中在一处，
   且 `cfg(debug_assertions)` 断言两侧相等。
3. 段内**禁止**跨所有权读写：系统读资源、facade 读字段会形成两条真值。
4. 写集层面：S3 起每个切片在「搬字段」的同一个提交里完成「搬真值」。

### 4.3 关于 `cheats`

`World` 没有 `cheats` 字段。GM/作弊面 = 实体位（`admin_mode`/`super_man`）+ 宿主
`bins/game-svr` 的 `gmcmd`（actor 侧）。本迁移**不**把 GM 命令塞进 ECS。

---

## 5. tick 系统图

### 5.1 现状段序（`World::tick` = `tick_head` + 宿主段 + `tick_tail`）

`tick_head` 内部顺序（源码注释与实现一致，这里按 `World::tick_head` / `World::tick_tail` 的符号列）：

| # | 段 | 现符号 | 建议阶段 | 抽 RNG？ | 串行约束 |
| --- | --- | --- | --- | --- | --- |
| 0 | 线程断言 + 时钟 | `advance_clock_system` | `Clock` | 否 | 已注册；每 tick 只跑一次 |
| 1 | 命令 FIFO | `World::tick_head` 的 `match cmd` → `enter_world`/`leave_world`/`try_step`/`attack_once`/`spawn_monster_now`/`push_now`/`rush_now`/`butch_now`/`butch_effect_now` | `Command` | **是**（攻击/推挤/生成掉落） | 必须**逐条 FIFO**，一个系统；不得拆散/并行 |
| 2 | PL-1 玩家命中者快照 | `player_hiter_snapshot` 填充 | `SimHiterSnapshot` | 否 | 必须在 #3 之前（读清理前的值） |
| 3 | 命中者过期 | `World::expire_hitters` | `SimExpireHitters` | 否 | 在 #2 之后 |
| 4 | 非玩家死亡结算 | `World::settle_deaths(false)` | `SimSettleDeaths` | **是**（掉落/经验） | 在命令后；玩家死亡留到 tail |
| 5 | 宝宝自投递 | `World::deliver_self_delay_msgs` | `SimSelfDeliver` | 可能有（魔法/治疗） | 在宝宝清表之前 |
| 6 | `ExtraAbil` 过期 | `World::expire_extra_abil` | `SimExtraAbil` | 否 | |
| 7 | 地图事件 | `World::run_map_events` | `SimMapEvents` | 可能有（事件分支） | |
| 8 | 宝宝清表 | `slave::slave_list_cleanup_all` | `SimSlaveCleanup` | 否 | 在死亡结算之后 |
| 9 | buff | `World::buff_tick` | `SimBuff` | **否**（显式口径） | `buff_enabled` 门 |
| 10 | 怪物 AI | `monai::monster_ai_tick` | `SimMonsterAi` | **是（重）** | `ai_enabled` 门；按槽位顺序 |
| 11 | NPC | `World::npc_tick` | `SimNpc` | **是** | 必须在商人之前（oracle `ProcessNpcs` → `ProcessMerchants`） |
| 12 | 商人 | `World::merchant_tick` | `SimMerchant` | **是** | 在 NPC 之后 |
| 13 | 玩家 Run 到期 | `World::expire_player_run_state` | `SimPlayerRunExpiry` | 否 | |
| 14 | 重叠推挤 | `World::check_dup_obj` | `SimDup` | **是（有门）** | 在 Run 到期之后 |
| 15 | 输出转抄 | `record.*.append(&mut pending_*)` | `OutputDrain` | 否 | 每向量内保持发生序 |
| 16 | 玩家 SuperMan 回满 | `Entity::run_super_man_refill`（`tick_tail` 的玩家循环） | `TailSuperManRefill` | 否 | oracle `BaseObject.Run` 首段，**先于**玩家死亡结算 |
| 17 | 玩家死亡结算 | `World::settle_deaths(true)` | `TailPlayerDeaths` | **是** | 在宿主段 P21..P32 之后、SuperMan 回满之后 |
| 18 | VerifyMapTime + 钳制 | `World::verify_players_map_time` | `TailVerifyMapTime` | 否 | 在 AOI 之前 |
| 19 | AOI | `search_view_range_with_updates` / `search_view_range_death` / `search_view_range_events` | `OutputAoi` | 否 | 按地图、地图内按槽位顺序 |

宿主段（`bins/game-svr` 驱动，P21 装备复检 … P32 复活戒指）**仍在宿主**，不在 ECS；
它夹在 `tick_head` 与 `tick_tail` 之间，是 facade API 的一部分。

### 5.2 RNG 消费者次序（合同，必须逐项保持）

```
命令 FIFO
  → 命中者过期（不抽）
  → 非玩家死亡结算
  → 宝宝自投递
  → ExtraAbil 过期（不抽）
  → 地图事件
  → 宝宝清表（不抽）
  → buff（不抽）
  → 怪物 AI
  → NPC
  → 商人
  → 玩家 Run 到期（不抽）
  → 重叠推挤（有门才抽）
  → [宿主段 P21..P32]
  → 玩家死亡结算
  → VerifyMapTime（不抽）
  → AOI（不抽）
```

任何新系统只要**可能抽 RNG**，就必须插在合同位置；「不抽 RNG」的系统插入位置
不影响流，但**仍要**避免改变后续段的可见状态。

### 5.3 阶段链必须扩展（S3a 的契约变更）

现状：`TickStage::{Clock, Command, Simulate, Output}`，阶段间 `.chain()`，同阶段冲突 ⇒
构建失败（`tick_schedule_build_settings` 的 `ambiguity_detection = Error`）。

问题：上表 #2–#14 全部读写**同一批实体组件**（至少 `LifeCycle`/`Vitals`/`AiState`），
且多数需要 `ResMut<Rng>`。把它们作为**独立系统**放进同一个 `Simulate` 阶段 ⇒ 必然冲突 ⇒
schedule 构建 panic；`TickRunner` 又只接受「一个阶段 + 一个系统」，不提供 `.chain()` 入口。

**两条可行路线**（不是「只有一条」——评审 P2-4）：

1. **单体 exclusive `Simulate` 系统**：一个 `fn(&mut World)` 在内部按 oracle 段序串行调用各段，
   零歧义、4 阶段不变；代价是段与段之间没有系统级边界，后续逐段迁移时仍要再拆一次；
2. **每段一个阶段**（Leader scope_ruling 选择）：把 `TickStage` 展开为线性段链，每段一个阶段、
   每段一个系统；代价是**一次显式契约变更**（`STAGE_ORDER`、`TickRunner::new` 的 chain、
   `tick_skeleton` 守卫必须同一提交更新），收益是后续每段可独立成系统、顺序仍只由枚举决定。

S3a 采用路线 2（见 `crates/world-bevy/src/tick.rs` 的 `TickStage`/`STAGE_ORDER`）。建议新增枚举项
（顺序即 `STAGE_ORDER`；`Clock` + 19 个 oracle 业务段 = 20 项）：

```
Clock
Command
SimHiterSnapshot
SimExpireHitters
SimSettleDeaths
SimSelfDeliver
SimExtraAbil
SimMapEvents
SimSlaveCleanup
SimBuff
SimMonsterAi
SimNpc
SimMerchant
SimPlayerRunExpiry
SimDup
OutputDrain
TailSuperManRefill   // tick_tail 第 0 段（玩家 SuperMan 回满）
TailPlayerDeaths     // tick_tail 第 1 段（玩家死亡）
TailVerifyMapTime    // tick_tail 第 2 段（VerifyMapTime + 钳制）
OutputAoi            // tick_tail 第 3 段（AOI）
```

**与原四相位的映射**（语义不变，只是把 Simulate 展开）：

- `Clock` 相位 = `Clock`；
- `Command` 相位 = `Command`（命令 FIFO）；
- `Simulate` 相位 = `SimHiterSnapshot` … `SimDup` + `TailSuperManRefill`/`TailPlayerDeaths`/`TailVerifyMapTime`
  （全部世界模拟段；拆开只为显式定序，不是新语义）；
- `Output` 相位 = `OutputDrain` + `OutputAoi`（记录转抄与 AOI 刷新）。

约束与注意：

- 这是**契约变更**：`STAGE_ORDER`、`TickRunner::new` 的 `.chain()`、`tick_skeleton` 守卫
  （阶段链/ambiguity/`TickRunner` 方法白名单）必须同一提交更新；评审里单独说明。
- 仍然**不用** `.before()`/`.after()`/`.chain()` 做业务排序——顺序只由阶段枚举决定。
- 同阶段仍只放一个系统（或**确认无访问冲突**的只读系统）；不要把「反正 chain 了」当排序。
- `collect_tick_order_system`（`bevy_bridge.rs`）**S3a 未注册**（S3a 只交付了 20 阶段契约，
  业务段仍为空）：把它接到 `Command` 之前（或由 facade 直接填 `TickOrder`）留到**首次搬迁
  业务段的切片**，届时它作为「按槽位顺序处理」的显式入口。
- `tick_tail` 的玩家死亡/VerifyMapTime/AOI 三段在宿主段之后，因此排在 `OutputDrain` 之后；
  宿主段本身不进入 schedule（facade 调用）。

### 5.4 必须在同阶段 / 必须拆阶段的清单

| 需求 | 系统 | 说明 |
| --- | --- | --- |
| 必须同阶段（不可拆） | `CommandPump` | FIFO 逐条；拆开会改变命令间可见状态 |
| 必须前后相邻 | HiterSnapshot → ExpireHitters | PL-1 快照读清理前的 `LastHiter` |
| 必须前后相邻 | `SimMonsterAi` → `SimNpc` → `SimMerchant` | RNG 消费者次序 |
| 必须前后相邻 | `SimPlayerRunExpiry` → `SimDup` | oracle `PlayObject.Run` 段序 |
| 必须拆阶段 | 所有写 `&mut Entity` 组件或 `ResMut<Rng>` 的段 | 同阶段会被歧义检测判失败 |
| 可同阶段（确认无冲突） | 纯只读统计/诊断系统 | 相对顺序不是契约，不依赖彼此 |

---

## 6. 迁移切片

### 6.1 策略：单写者接管，不做长期影子组件

每片两条路径二选一，**默认选 A**：

- **A. 单写者接管（推荐）**：字段从 `Entity` 物理移除 → 成为组件；所有引用点改成
  组件访问；`From<&Entity>`/`apply_to` **只作为片边界的等价验证工具**，验证通过后删除。
  好处：没有长期双真值；坏处：写集包含该字段的全部引用点（所以先拆叶子）。
- **B. 影子组件 + 同步**：组件与 `Entity` 字段同时存在，tick 边界同步。只有在某片引用点
  多到无法一次改完时才用，且必须配 `cfg(debug_assertions)` 两侧相等断言 + 明确删除条件。

P0 的适配器（`crates/world-bevy/src/components.rs` 的 `adapters` 模块）就是路径 A 的
片边界工具：把切片前后的 `Entity` 各拆一次组件、比较值，证明「搬出去的字段没有语义漂移」。

### 6.2 S3a 前置：`EcsStore<T>` 的多组件访问

当前 `EcsStore<T>` 只承载一个组件 `T`（`insert(id, value)` / `get::<T>` / cached `QueryState<&T>`）。
组件化需要：

- 同一 bevy 实体上**追加/读取/移除**任意组件（`insert_component` / `get_component` / `remove_component`）；
- 结构变更后刷新缓存 `QueryState` 的 archetype 匹配集（现有 `update_archetypes` 模式可复用）；
- 槽位顺序仍只来自 `slots`，新增组件不得改变遍历来源；
- 插入/移除组件 = **archetype 迁移**，要评估热路径成本（P99 参数见 §6.6）；
- `Entity` 最终可能退化为薄标签（`CoreIdentity` + 迁移期剩余字段），`EcsStore<T>` 的
  泛型主组件届时需要重新定义（S5 收口）。

### 6.3 切片表（S0–S5，按「world 可删」推进）

> 编号对齐旧卡 owner 裁决 `de11cdd4`：S1/S2 已做 entity+tick/存储调优；
> S3 = map+aoi；S4 = combat+session（含其余模拟模块）；S5 = 切换并删除 `crates/world`。
> 每批内部再按「叶子先、热点后」拆子片（S3a/S3b…），子片各自独立可回退。

| 批/片 | 范围（组件与 API） | 写集（文件级） | 行为等价判据 | 回滚点 |
| --- | --- | --- | --- | --- |
| **S0 = P0**（本卡） | 组件骨架 + 设计文档 | `crates/world-bevy/src/components.rs`、`lib.rs`、`Cargo.toml`、本文件 | 五门 + 151 字段 round-trip（P0 即本卡） | 丢弃分支 |
| **S1/S2（已完成，`f5170a8e`）** | entity 存储 + tick 骨架 + 存储调优 | `crates/world-bevy/src/{ecs.rs,tick.rs,lib.rs}`、`crates/world/src/{entity.rs,world.rs,bevy_bridge.rs}` | 三份 golden 逐字节不变；P50 ≤ 10% / P99 中位数 ≤ 30% | 已合并，不回退 |
| **S3a 试点** | `DupState`（3 字段）+ `EcsStore` 多组件访问 + TickStage 段链（§5.3） | `crates/world-bevy/src/{ecs.rs,components.rs,tick.rs,lib.rs}`、`crates/world-bevy/tests/tick_skeleton.rs`、`crates/world/src/{entity.rs,world.rs,session.rs}`、`bins/game-svr/src/lib.rs`（宿主 `check_dup_obj_tick` 读点）、`crates/world/tests/{check_dup_obj_vectors.rs,ecs_migration.rs}` | `check_dup_obj_vectors`；ctor 基准用例；golden 三份；parity 三件；RNG trace 不变；`tick_skeleton` 20 段链/歧义守卫绿 | 按子片回退（无存档面） |
| **S3b map+aoi** | `Transform`+`MapBinding`+`MapsRes` + AOI 系统 + 世界侧 session 存在性（`SessionRegistry` 的进/出图 API） | `crates/world/src/{entity.rs,world.rs,aoi.rs,map.rs,session.rs,space_move.rs}` + `crates/world-bevy/{src/ecs.rs,src/components.rs}` | `world_aoi_parity` + `aoi_viewrange12` + `world_session_parity`；golden 三份；P99 参数记录 | 按子片回退；格子登记与组件更新必须同提交 |
| **S4a 属性/归属/叶子** | `SpawnBinding`、`DropState`、`CombatAttribution`、`Recovery`、`PvpState`、`Vitals`+`AbilitySnapshot`+`Progression`；RNG 抽数站点 `ctor_fold`/`run_gap` | `crates/world/src/{entity.rs,world.rs,combat.rs,mongen.rs,slave.rs,death_drop.rs,buff.rs,ctor_fold.rs,run_gap.rs}` + `bins/game-svr/src/{stamina.rs,lib.rs}`（`apply_ability_to_entity`） | golden 三份；`world_combat_parity`；`rng_parity`/`rng_replay_parity`；`drop_parity`/`exp_parity`/`levelabil_parity`；DB 存档对拍（HP/MP/exp/level/gold） | 按子片回退 |
| **S4b 玩家/Buff/生命周期** | `PlayerRunState`、`GroupState`、`Revival`、`BuffState`、`Flags`、`LifeCycle`、`Appearance`、`MonsterClassState`；出站事件词汇 `action`/优先级队列 `msgqueue` | `crates/world/src/{entity.rs,world.rs,party.rs,buff.rs,magic.rs,magic_effect.rs,aoi.rs,monai.rs,combat.rs,mongen.rs,action.rs,msgqueue.rs}` + `bins/game-svr` P27/P32 | golden 三份 + `aoi_viewrange12`；`party_vectors`/`party_attribution`；`special_monster_vectors`；帧面逐字节 | 按子片回退 |
| **S4c AI/商人/NPC/物品** | `AiState`+`SlaveState`+`MerchantState`+`NpcState`+`Inventory`+`CoreIdentity`（含 `process_run_counts` 旁挂表 → 组件） | `crates/world/src/{entity.rs,monai.rs,slave.rs,special_monster.rs,merchant.rs,npc.rs,world.rs}` + `bins/game-svr` 物品/存档同步 | `monster-ai-*`/`merchant-tick`/`npc` 场景；`world_combat_parity`；RNG 账本；存档/DB 对拍 | 按子片回退 |
| **S4d 城堡/行会/税收** | `castle`（攻城状态机）、`castle_tax`（税率/金库）、`guild`（会话态/成员/联盟）整模块迁入 `world-bevy`（公会**服务**仍是 actor，只搬状态机 API） | `crates/world/src/{castle.rs,castle_tax.rs,guild.rs}` + 对应 `bins/game-svr` 城堡/行会段 | 城堡/行会既有用例向量；golden 三份；`rg` 无 `mir2_world` 残留前先跑模块级 | 按子片回退 |
| **S5 切换并删除** | `tests/parity` + `tick_bench` + `bins/game-svr` + 守卫全部改指 `world-bevy`；删除 `crates/world` 与 workspace 成员 | `mir2-rs/crates/world-bevy/{Cargo.toml,src/bin,tests}`、`mir2-rs/tests/parity/{Cargo.toml,tests,src}`、`mir2-rs/bins/game-svr/{Cargo.toml,src}`、`mir2-rs/Cargo.toml`、`mir2-rs/Cargo.lock`、删除 `mir2-rs/crates/world/**` | 全 workspace 五门 + golden 三份 + parity 全套 + RNG trace + DB 窗口；`crates/world/tests/no_parallel.rs` 守卫**先移植**到 `world-bevy/tests` 并绿；`rg -n "mir2_world|mir2-world" mir2-rs/crates/world-bevy mir2-rs/bins mir2-rs/tests/parity` 无残留（除历史文档）；`cargo metadata` 无 world 成员 | 合并前整提交回退；合并后按 follow-up 卡处理 |

> **S3b 落地登记**：`Entity` 删除 `x/y/map_id/view_range` + `add_to_mapped/visible_actors/`
> `visible_events/is_visible_active/processed_tick/verify_tick`（151→138 字段）。`EntitySpawn`
> 作为生成期载体（`insert` 时落成 `Transform`/`MapBinding` 组件），`EntityRef` 为三组件只读视图；
> `MapsRes` 在 tick 边界一次性移入/移出，`TickStage::OutputAoi` 的 `aoi::output_aoi_system`
> 按 `AoiIds` 槽位顺序完成全部 AOI 刷新（算法仍是 `aoi.rs` 的同一实现，经 `AoiStore`
> trait 由 `EntityStore` 与 bevy `World` 两个实现复用）。

> **S4a 落地登记**：`Entity` 删除 S4a 的 41 个字段（151→97）。`EntitySpawn` 平铺生成期字段
> （`drop_items` 保留原名），`EntityRef` 平铺只读投影（`drop_items: &Vec<DropItem>`），
> `EntityMut` 暴露 `vitals`/`ability`/`progression`/`spawn`/`drop`/`attribution`/`recovery`/`pvp`
> 组件字段；`EntityStore::insert` 用 `SpawnParts` 整 bundle 挂组件，读路径 `get`/`iter_ref`、
> 写路径 `get_mut`/`for_each_mut`，另有 `vitals(_mut)`/`progression(_mut)`/`ability(_mut)`/
> `set_progression`/`write_back` 受控入口。`combat` 的攻击/经验结算改在生成期载体上跑，
> `slave` 的 Recalc 与宿主 `apply_ability_to_entity` 改收 `EntityMut`；战斗目标判定链改收
> `PvpView`（`base()` + `pvp_state()`），对 `EntitySpawn`/`EntityRef`/`EntityMut` 统一实现。
> **回写纪律**：`spawn_cloned → 改副本 → 回写` 必须用 `EntityStore::write_back`（整包写
> 主组件 + 8 个组件），不得只写 `*e = spawn.entity`（会丢 HP/MP/exp；回归牙
> `write_back_preserves_spawn_cloned_vitals`）。
>
> **RNG 站点（ctor_fold/run_gap）**：S4a 未改抽数次序。`ctor_fold` 的生成期 4 抽/
> 登录期第 5 抽与 `run_gap::dup_push_dir` 仍在原消费者位置；字段访问只是读取/写回组件值，
> 不新增/不移动任何 `random_*` 调用。证据：`rng_parity`/`rng_replay_parity`/golden 零 diff，
> 以及 M3 的「RNG 站点换序 ⇒ rng_parity 红」变异重放。
>
> **S3b 评审 P3 处置**：① `output_aoi_system` 的 `AoiIds`/`AoiParams`/`MapsRes` 由
> `unwrap_or_default` 改为 `expect`（缺资源是接线错误；`AoiScratch` 仍是每 tick 新建的合法
> 缺省）；② `insert_parts` 收窄为 `SpawnParts` 单一入参、唯一调用方是 `EntitySpawn::into_parts`，
> 并加 `insert_uses_spawn_ctor_bundle_not_component_defaults` 防 `Transform::default()` 误用；
> ③ `BevyAoiStore` 仍按 `World::get::<T>` 逐组件取（S4a 新增 5 次 TypeId 查表），属性能项，
> S4a 不压测、只登记（后续可改缓存 `QueryState`）。

> **S4b 落地登记（2026-11-13）**：`Entity` 删除 S4b 的 80 个字段（97→17，仅剩 S4c 的
> `CoreIdentity`/`AiState`/`SlaveState`/`MerchantState`/`NpcState`/`Inventory`）。分三笔子里程碑：
> ① S4b-1 `Flags`(13)/`LifeCycle`(8)/`Appearance`(5)/`MonsterClassState`(19)；
> ② S4b-2 `PlayerRunState`(13)/`GroupState`(4)/`Revival`(4)/`BuffState`(14)；
> ③ S4b-3 出站事件词汇登记 + S4a 评审 P3 处置。`EntitySpawn` 平铺生成期字段（ctor 缺省：
> `race=Play`、`name_color=255`、`power_rate`/`kill_mon_exp_rate=100`、
> `poisoning_tick=BUFF_EPOCH_MS`），`EntityRef` 平铺只读投影，`EntityMut` 暴露组件可变引用；
> `EntityStore::insert` 用 `SpawnParts` 整 bundle 挂组件，`write_back` 整包写回全部 16 个迁移
> 组件（S4a 8 + S4b-1 4 + S4b-2 4）。bevy `QueryData` 元组上限 15 只是实现约束：读路径用第二个
> `QueryState`（两次共享借用，不迭代求序），写路径把 `Recovery`/`PvpState` 移出 `EntityMut`
> 改 `recovery_mut`/`pvp_state_mut`（组件仍是唯一真值，无影子副本、无读写换序），语义由
> golden/parity 兜底。`PvpView` 现只对 `EntitySpawn`/`EntityRef` 实现（S4a 时曾含
> `EntityMut`），并新增 `race`/`death`/`ghost`/`invisible`/`attack_mode`/`flags`/
> `name_color`/`player_run`/`group_state`/`buff` 快照方法供 combat/monai 视图读取。
>
> **出站事件词汇（action/msgqueue）= 已迁/登记**：`action.rs`（`PushEvent`/`RushEvent`/
> `RushFrame`/`ButchEvent`/`SkeletonEvent`）与 `msgqueue.rs`（.NET 8 四叉堆出队序）自
> `openmir2-frame-drain-order-audit` 起就在 `crates/world`，**不持有任何 `Entity` 字段**
> （纯确定性事件/算法），S4b 只需登记为稳定词汇、无需字段迁移；帧编码仍在宿主 `bins/game-svr`。
>
> **S4a 评审 P3 处置**：① 注释计数过期——`Entity` 剩余字段数与 `EntitySpawn::new` 调用计数改为
> 「剩余 17 字段」与「约 700 处」（不再写死会腐烂的精确数）；② `write_back` 只断言 HP/MP/exp 的
> 盲区——新增 `write_back_covers_all_migrated_components`，把 16 组件逐字段置非缺省后
> `spawn_cloned → write_back → get` 逐组断言，漏写 attribution/pvp/recovery/drop/spawn/ability
> 任一即红；③ `insert_parts`/`into_parts` 收窄为私有、`SpawnParts` 降为 `pub(crate)`
> （唯一调用方是 `EntitySpawn::new → EntityStore::insert`）。

> **模块级收口登记（评审 P2-2）**：`crates/world` 的 28 个模块逐一有归宿——
> `entity/tick/存储`=S1/S2；`DupState`+tick 段链=S3a；`map/aoi/space_move/session`=S3b；
> `combat/mongen/slave/death_drop/stamina(宿主)/buff/ctor_fold/run_gap`=S4a；
> `party/magic/magic_effect/action/msgqueue/special_monster`=S4b；`monai/merchant/npc`=S4c；
> `castle/castle_tax/guild`=S4d；`lib.rs`（crate 根再导出）、`bevy_bridge.rs`、`bin/tick_bench.rs`
> 由 S5 随 crate 删除/迁往 `world-bevy`（不是功能缺口）。S4a 的 `stamina.rs` 路径是
> `bins/game-svr/src/stamina.rs`（`crates/world/src` 无此文件）。

> **S5 的写集越界提醒**：本卡 issue 声明的 write_set 是
> `crates/world/**`、`crates/world-bevy/**`、`bins/game-svr/src/lib.rs`、`tests/parity/**`；
> S5 还必须改 `mir2-rs/Cargo.toml`（删 workspace 成员）、`bins/game-svr/Cargo.toml`、
> `tests/parity/Cargo.toml`、`crates/world-bevy/Cargo.toml`（删 dev-dependency），并搬迁
> `crates/world/tests/**` 与 `crates/world/src/bin/tick_bench.rs`。**这些不在当前 write_set 内**，
> S5 开工前必须由 Leader 扩写集（否则违反 §12.4 写集纪律）。

> 判据里「golden」= `tests/parity/world/golden/{basic,session,combat}.txt` 逐行 diff = 0；
> 「RNG trace」= `crates/world/tests/hit_dice_rng_trace.rs`、`magic_draws_rng_trace.rs`、
> `tests/parity/tests/rng_parity.rs`、`rng_replay_parity.rs`、`player_ctor_rng_trace.rs`；
> 「DB 面」= 真机窗口内 `bins/game-svr` 存档/入库与 C# 参照对比（测试栈与生产库隔离，
> 以实例重启为边界取快照）。

### 6.4 推荐的第一片（S3a 试点）：`DupState`

选择理由：

1. **最小**：3 个字段；逻辑消费点有**两个**——`World::check_dup_obj` 与宿主
   `bins/game-svr/src/lib.rs::apply_get_start_point_after_tick`（每 tick 读 `check_dup_obj_tick`
   决定 `GetStartPoint` 覆盖，见 C13/P2-1）。写点：`EntityStore::insert`（ctor 缺省）/
   `World::add_entity` / `SessionRegistry::enter_map_at`。
2. **可独立验证**：已有 `crates/world/tests/check_dup_obj_vectors.rs` 专测「3s/10s/20s 门
   与首次观测标记」。
3. **无存档面**：纯内存字段，不触碰 DB/存档，回滚零数据风险。
4. **RNG 有门**：`run_gap::dup_push_dir` 只在门通过时抽数；迁移后调用位置不变 ⇒
   账本应逐项不变（这正好验证「系统化不改变 RNG 消费者次序」）。
5. **能验证 S3a 前置**：它同时逼出 `EcsStore` 的多组件访问模式（`get_component::<DupState>` /
   `insert_component`），是后续所有片的模板。

S3a 步骤（建议提交 2–3 笔）：

1. `EcsStore` 增加多组件读写 API + 单测（不改 `EntityStore` 对外 API）。
2. `DupState` 成为真值：`Entity` 移除 3 字段；`check_dup_obj` 改为经组件读写；
   `EntityStore::insert` 插 `DupState::default`，`add_entity`/`enter_map_at` 经
   `dup_state_mut` 按 ctor 时刻重盖；宿主读点改为 `entities.dup_state`。
3. 片边界等价：`check_dup_obj_vectors`（3s/10s/20s 门、首次观测、RNG 抽数）+
   `verify_and_dup_ticks_base_is_playobject_ctor_not_map_entry` + 三份 golden + RNG trace；
   记录 P99 参数（不压测）。旧适配器已无字段可比，删除并用上述行为判据取代。

回滚点：每个提交独立可回退；S3a 不涉及持久化，回退即恢复字段。

### 6.5 与卡面 acceptance 的对齐（一处待 Leader 裁决）

卡 `openmir2-world-ecs-migration` 的 acceptance 原文要求「**P1 实体组件拆分第一片：至少
Position/Transform、Vitals、Appearance、Flags 从大 Entity 拆出**」。按 `de11cdd4` 的退役路线，
这四项分属不同批，不能强行压成一片而不改风险：

- `Transform`：必须与 `MapBinding` + `MapGrid` 格子登记 + AOI **原子**完成（S3b）。
  单独搬字段会出现「组件已改、格子未改」的中间态；`x`/`y`/`map_id` 合计约 280 处引用。
- `Vitals`：S4a（与 `AbilitySnapshot`/`Progression` 同片）。
- `Flags`：S4b（与 `LifeCycle` 同片，`super_man` 的回满段定序相关）。
- `Appearance`：S4b；其中 `race` 是行为轴（约 96 处引用），不宜早于 AOI/战斗切片。

**建议**（供 Leader 裁决）：

1. S3a 先做 `DupState` 试点，验证 `EcsStore` 多组件访问与片边界等价工具；
2. S3b 满足卡面的 `Transform` 部分（连同 AOI，原子切片）；
3. S4a/S4b 满足 `Vitals`/`Flags`/`Appearance`；
4. 若 Leader 要求卡面四项必须在**同一片**完成，则该片只能 = S3b+S4a+S4b 合并，
   写集覆盖 map/aoi + combat + 玩家状态，风险显著更高；本文件不替 Leader 做这个合并决定。

---

### 6.6 验收命令（每片）

```bash
# 五门（待合并分支上）
cd mir2-rs && cargo fmt --all -- --check
cd mir2-rs && cargo clippy --workspace --all-targets -- -D warnings
cd mir2-rs && cargo test --workspace --no-fail-fast
cd mir2-rs && cargo run -p msgcodegen -- --check
python -I -B tools/hygiene/docs_anchor_gate.py --prose --base <合并目标 ref>

# golden / parity（Rust 侧；金标准已入库，不需要重跑 C#）
cd mir2-rs && cargo test -p mir2-parity-tests --test world_aoi_parity
cd mir2-rs && cargo test -p mir2-parity-tests --test world_session_parity
cd mir2-rs && cargo test -p mir2-parity-tests --test world_combat_parity

# RNG 账本（进程级开关，必须独占测试二进制）
cd mir2-rs && cargo test -p mir2-world --test hit_dice_rng_trace -- --nocapture
cd mir2-rs && cargo test -p mir2-world --test magic_draws_rng_trace -- --nocapture
cd mir2-rs && cargo test -p mir2-parity-tests --test rng_parity --test rng_replay_parity

# P99 参数（当前不压测；每片记录一次三档，口径同 S2）
cd mir2-rs && cargo run -p mir2-world --release --bin tick_bench -- 500 2000 1000 2000 2000 1000
```

S2 的相对门禁参数（沿用）：三档 500/1000/2000 实体，**P50 ≤ 10%、P99 中位数 ≤ 30%**；
绝对值 P99 只作报告值（200ms tick 预算内）。当前**不压测**，但每片必须记录参数与趋势。

---

## 7. 风险与处置

| # | 风险 | 表现 | 处置 / 守卫 |
| --- | --- | --- | --- |
| R1 | **`Query` 迭代序** | 组件拆分改变 archetype/table 布局，`Query` 迭代序随之变化；若用它定序，AOI/RNG 立刻漂 | 系统一律按 `SlotOrder`/`TickOrder` 定序；`Query` 只按显式句柄取组件；`ecs_migration` 与 `tick_skeleton` 守卫保持 |
| R2 | **槽位复用与组件原子性** | `free` LIFO 复用槽时，若只插部分组件，会出现「半实体」；`iter_mut` 的 `Query` 只取有 `T` 的实体，缺主组件的实体会被静默跳过 | 插入/移除必须**整 bundle**；`EcsStore::insert/remove` 扩展为 bundle 语义；加单测：复用槽后组件集合完整 |
| R3 | **151 字段引用面** | `x` 101、`race` 96、`ai` 173 处引用；一次性拆会不可审查 | 按 §3.3 波次；每片写集只含该组字段的引用；热点最后 |
| R4 | **快照/存档字段** | HP/MP/exp/level/use_items/gold 等经宿主会话 `wabil`/物品同步进出存档；组件化后同步函数若漏字段 ⇒ DB 面差异 | 存档面**永不进 ECS**；`bins/game-svr` 的 `apply_ability_to_entity`/`sync_use_items_*`/`build_save_record` 是唯一入口；每片跑真机 DB 对拍（窗口） |
| R5 | **borrow checker** | 拆组件后原「`&mut Entity` + `&mut World` 同时借」的模式不再自然；系统参数一多会冲突 | 每系统只取自己需要的组件；跨实体访问用 `Commands`/显式 id 二阶段；facade 保留 `std::mem::take` scratch 模式 |
| R6 | **双写/真值** | 过渡期 facade 字段与组件各一份，读写不一致 ⇒ 难查的漂移 | 单写者接管（路径 A）；若必须影子，`cfg(debug_assertions)` 两侧相等 + 单一 tick 边界同步 + 明确删除条件 |
| R7 | **持久化/重建** | 组件不在 `Entity` 里，登录/重生/切图时若漏插组件 ⇒ 实体缺状态 | 生成/登录/切图路径统一走一个 bundle 构造器；`mongen`/`merchant`/`npc`/`session` 各自的插入点加「组件集合完整」断言 |
| R8 | **RNG 消费者次序** | 新系统插入/搬段改变抽数次序，账本逐击漂 | §5.2 合同；每片跑 RNG trace 与 golden；系统阶段位置显式登记 |
| R9 | **TickStage 契约变更** | 扩展枚举会改 `STAGE_ORDER`/守卫；改错则歧义检测失效或顺序静默变化 | S3a 单独评审；`tick_skeleton` 阶段链/歧义/方法白名单同步更新；禁止 `.before()`/`.after()`/`.chain()` 做业务排序 |
| R10 | **P99 劣化** | 多组件 archetype 迁移、`get_component` 走查表可能抬热路径 | 每片记录 `tick_bench` 三档；必要时像 S2 一样缓存 `QueryState`；不达标按 S2 口径回退 |
| R11 | **泛型载荷的落点** | P0 用 `AiState<A>` 等泛型避免环依赖；S3 若一直不收敛，载荷类型会长期漂在两处 | 每个泛型组件在 S3 计划里写明「类型搬去哪里」；最迟 S4c 收口为具体类型（S5 删除 world 前必须完成） |
| R12 | **守卫扫描域** | `components.rs` 会被 `tick_skeleton` 全 crate 扫描（禁配置对象 token/包含指令）；`no_parallel_guard` 扫 `[dependencies]` 白名单 | 组件文件不出现 `IntoScheduleConfigs`/`ScheduleConfigs`/`IntoSystemConfigs`/`ambiguous_with`/`ignore_ambiguity`/`include!`/`#[path`/`OUT_DIR`；`mir2-world` 只放 `[dev-dependencies]` |
| R13 | **P0 适配器的覆盖边界** | round-trip 用「全字段赋非缺省值 + 全量 `Debug` 比较」；若 `Entity` 以后新增字段而没进组件/没进 `distinctive_entity`，用例不会自动报警 | 在 `components.rs` 用例注释与本文 §3.3 登记；S3 起由「字段搬出 + 编译错误」强制覆盖，不再依赖该用例 |
| R14 | **两 crate 长期并存（偏离 `de11cdd4`）** | 迁移若只加系统不删 `crates/world`，会形成两份 API/两份真值，退役永远不到 | 每批以「world 可删」为准入：S3/S4 每片把模块与测试的归宿写清；S5 设硬判据（§6.3/§8）。不允许新增 `world -> world-bevy` 之外的耦合 |
| R15 | **S5 写集越界 / 守卫随 crate 消失** | 删 workspace 成员要改 `mir2-rs/Cargo.toml`（不在卡面 write_set）；`crates/world/tests/no_parallel.rs` 若随 crate 删除，并行守卫会静默消失 | S5 前由 Leader 扩写集；先把 `no_parallel` 守卫移植到 `world-bevy/tests` 并验证“注入并行 ⇒ 红”，再删 `crates/world`；删除后跑 `cargo metadata` + `rg mir2_world` 断言 |

---

## 8. P0 交付物与判据

| 项 | 内容 |
| --- | --- |
| 文档 | 本文件（组件图 / 资源图 / tick 系统图 / 切片 / 风险） |
| 组件骨架 | `crates/world-bevy/src/components.rs`：25 组件、151 字段、8 个泛型载荷、`#[cfg(test)]` 适配器 |
| 导出 | `crates/world-bevy/src/lib.rs` 导出全部组件与槽位常量 |
| 依赖 | `crates/world-bevy/Cargo.toml` 仅新增 `[dev-dependencies] mir2-world`（测试/迁移工具用） |
| 测试 | `split_and_apply_round_trip_all_151_fields`、`debug_snapshot_detects_single_field_change`、`dup_state_adapter_carries_values_not_defaults` |
| 门禁 | fmt rc=0；clippy rc=0；test **3795 passed / 0 failed / 69 ignored** rc=0（基线 3792 + 本卡 3 个新用例）；msgcodegen rc=0；docs prose anchor GREEN（检查 0） |
| 写集 | 本文件 + `world-bevy` 的三个文件（`components.rs` / `lib.rs` / `Cargo.toml`）；`Cargo.lock` 因新增 dev-dependency 自动增加 1 行依赖边（生成物，非手改）；**未动 `crates/world`** |
| 不做 | 压测、并行 executor、冻结客户端/C#/协议、`World`/`EntityStore` API、生产路径 |
| 退役判据（S5） | `crates/world` 与其 workspace 成员删除；`tests/parity`/`tick_bench`/`bins/game-svr` 全部指向 `world-bevy`；`no_parallel` 守卫已移植且绿；`rg -n "mir2_world|mir2-world" mir2-rs/crates/world-bevy mir2-rs/bins mir2-rs/tests/parity` 无残留；golden/parity/RNG/DB 面全绿 |

**P0 的「完成」不等于迁移开始**：S3a 的前置是 `EcsStore` 多组件访问（§6.2）与 TickStage 段链契约变更（§5.3）。两者都应在 S3a 卡里显式列出，而不是在 P0 里偷跑。

> **S3a 状态注记**：上面这张表是 **P0 当时的交付快照**。S3a 已把 `DupState` 三字段搬出
> `Entity` 并把 TickStage 展开为 20 段：round-trip 用例更名为
> `split_and_apply_round_trip_all_remaining_fields`，`dup_state_adapter_carries_values_not_defaults`
> 由 `dup_state_component_default_matches_entity_ctor_defaults` + `EcsStore` 可选组件用例替代。
> 门禁计数以 S3a 卡面报告为准。
