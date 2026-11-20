# openmir2-refmsg-cache-residual-sites（残余卡：`SendRefMsg` 缓存未接入的广播点）

> 卡：`openmir2-refmsg-cache-residual-sites`（由 `openmir2-visiblehumanlist-cache`（T-287）的 C2 拆分）。
> 背景：T-287 的 500ms `VisibleHumanList` 缓存已接入 `broadcast_ref_msg_opt`（含
> `broadcast_ref_msg` / `broadcast_ref_msg_with_self` / `..._with_self_at` 的内部转调）与
> `drain_merchant_disappears`；**其余按 `chat::hear_targets` 自算收件人的广播点仍未经过缓存**，
> 属明确残余（不是"已消解"）。

## 1. 口径

- oracle 侧：**所有** `SendRefMsg(ident,…)` 都走同一份缓存（`src/M2Server/Actor/BaseObject.Message.cs`
  的 `SendRefMsg`，`>=500` 重扫 `±SendRefMsgRange` 闭区间；否则复用快照 + `|Δ|<11` 严格小于）。
- Rust 侧：只有经 `broadcast_ref_msg_opt` / `drain_merchant_disappears` 的路径命中缓存；
  下列站点直接调 `chat::hear_targets` 或 `send_ref_msg_to_targets` ⇒ **每次重算、无缓存**。
- 可观测差异（共同形态）：同一发送者在 500ms 内连发两帧时，**恰在 `SendRefMsgRange`(=12) 边界**
  的观察者（以及窗口内新进范围的玩家）会**多收** oracle 不会发的帧；11 格内与范围外两侧一致。

## 2. 残余站点（逐条）

| # | 站点（file:符号） | 帧族 | oracle 锚 | 现行为 | 预期可观测差异 | 修法与牙 |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `bins/game-svr/src/lib.rs::push_broadcast` | 施法尾帧族（ident `53`=SM_HEALTHSPELLCHANGED、`638`=SM_MAGICFIRE、`656`=SM_CHANGENAMECOLOR、`31`=SM_STRUCK、`1103`=SM_INSTANCEHEALGUAGE、`640`=SM_MAGIC_LVEXP） | `src/M2Server/Magic/MagicManager.cs` 的 `playObject.SendRefMsg(Messages.RM_MAGICFIRE, …)` | 每帧重算 | 施法同 tick 多帧 ⇒ 12 格边界多收后续帧 | 改用 `ref_msg_cache::targets`；牙：魔法线双侧 golden（需魔法线回归，故单列 C2） |
| 2 | `lib.rs::push_charstatuschanged` | `SM_CHARSTATUSCHANGED(657)` | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_CHARSTATUSCHANGED, …)` | 每帧重算 | 同窗口两帧 ⇒ 边界多收 | 同 1；牙：golden 边界观察者帧数 |
| 3 | `lib.rs::broadcast_charstatuschanged_except_self` | 657 | 同上 | 每帧重算 | 同上 | 同 1 |
| 4 | `lib.rs::flush_healthspell_slots` | `SM_HEALTHSPELLCHANGED(53)` | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_HEALTHSPELLCHANGED, …)` | 每帧重算 | 同上 | 同 1 |
| 5 | `lib.rs::drain_stamina` | 53 族 | 同上（`RM_HEALTHSPELLCHANGED`） | 每帧重算 | 同上 | 同 1 |
| 6 | `lib.rs::flush_run_frames` | 53 族 | 同上 | 每帧重算 | 同上 | 同 1 |
| 7 | `lib.rs::handle_say_with_saves` | `RM_HEAR`（`SM_HEAR`） | `src/M2Server/Actor/BaseObject.Base.cs` 的 `SendRefMsg(Messages.RM_HEAR, …)` | 每帧重算 | 连说两句 ⇒ 边界多收第二句 | 改用缓存；牙：说话两帧边界观察者 |
| 8 | `lib.rs::drain_slave_mutiny_downlink` | 叛变提示族 | 由 `SendRefMsg` 发出（宝宝叛变） | 每帧重算 | 边界多收 | 同 7 |
| 9 | `lib.rs::drain_combat_downlink`（两处：18513/18556） | 战斗/死亡族 | `SendRefMsg(RM_STRUCK/…)` | 每帧重算 | 同 tick 多目标结算 ⇒ 边界多收 | 同 1 |
| 10 | `lib.rs::drain_weapon_durability` | 武器耐久族 | `SendRefMsg`（耐久变化） | 每帧重算 | 边界多收 | 同 1 |
| 11 | `lib.rs::apply_pending_gm_target_ops` | GM 目标操作族 | `SendRefMsg`（GM 命令触发） | 每帧重算 | 边界多收 | 同 1 |
| 12 | `lib.rs::apply_pending_gm_guild_commands` | 行会/名牌族 | `SendRefMsg(RM_USERNAME/…)` | 每帧重算 | 边界多收 | 同 1 |
| 13 | `lib.rs::fan_out_npc_hear` | `RM_HEAR`（NPC 说话扇出） | `BaseObject.Base.cs` 的 `SendRefMsg(RM_HEAR, …)` | 每帧重算 | 连说两句 ⇒ 边界多收 | 同 7 |
| 14 | `lib.rs::broadcast_ref_msg_from_walk_candidates` | 走位候选广播（`RM_PUSH` 等） | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_PUSH, …)` | 每帧重算（候选集自算） | 逐步推挤 ⇒ 边界多收 | 改用 `targets_from_scan` + 牙 |
| 15 | `castle.rs::refresh_show_name_in_area` | `SM_USERNAME`（165 族） | `src/M2Server/Actor/BaseObject.cs` 的 `SendRefMsg(Messages.RM_USERNAME, …)` | 每帧重算 | 同窗口两次刷新 ⇒ 边界多收 | 同 1 |
| 16 | `guild_cmd.rs::ref_show_name` | `SM_USERNAME` | 同上 | 每帧重算 | 同上 | 同 1 |
| 17 | `guild_official.rs::handle_select` | `SM_USERNAME`（`RefShowName`） | `RM_USERNAME` 同上 | 每帧重算 | 同上 | 同 1 |
| 18 | `guild_official.rs::war_events_to_frames` | `SM_USERNAME` | 同上 | 每帧重算 | 同上 | 同 1 |

> 说明：opc-07 点名的 13 处即 #1–#12 与 #9 的两处；本表另列 #13/#14/#15–#18 共 6 处同类
> （`hear_targets` 直算），避免"数完了其实还有"。**两个例外不属 SendRefMsg、不列**：
> 纯 `SendMsg` 点对点发送（不进 `SendRefMsg` 的扫格面）。
>
> **已覆盖对照（不占残余编号）**：`drain_npc_merchant_turns` / `drain_npc_merchant_hits` 的
> 收件人走 `broadcast_ref_msg_opt`，已在 T-287 接入缓存；此处仅列作对照，不计入"残余 19 处"。
> （残余计数 = #1–#18 共 18 行，其中 #9 含两处站点 ⇒ 19 处。）

| # | 站点（file:符号） | 帧族 | 现状 | 修法 |
| --- | --- | --- | --- | --- |
| 19 | `lib.rs::broadcast_healthspellchanged`（**新登记，7b 期间发现**） | 53 族 | **手写 `online` 循环**（`ObMode/FixedHideMode` 早退 + 同图 ±range + `!Ghost`），既不调 `chat::hear_targets` 也不走 `ref_msg_cache` ⇒ 每次全额重算；oracle 的 `HealthSpellChanged → SendRefMsg(53)` 应过缓存。**不在原 17 点清单的原因**：原清单口径是「按 `chat::hear_targets` 直算收件人」，它是同形的手写循环未被清点。 | 与 7 系列同法接 `ref_msg_cache::targets`；注意其既有边界用例 `broadcast_healthspellchanged_boundary_dx12_in_dx13_out_cross_map_out` 现钉「dx=12 收」的**未缓存**口径，接入时须按 oracle 改期望并留痕；牙：同窗两次 `HealthSpellChanged` 的 12 格边界只收首帧、隐藏 sender ⇒ 只自己（既有 `broadcast_healthspellchanged_hidden_play_source_gets_degraded_self_copy` 在接入时需按 oracle 重判）。 |
| 20 | `lib.rs::broadcast_feature_changed` / `broadcast_feature_changed_except`（**新登记，残余普查补漏**） | `RM_FEATURECHANGED`→`SM_FEATURECHANGED(41)` | **手写 `online` 循环**（同图 ±range、`skip` 过滤），不走 `ref_msg_cache`；oracle `BaseObject.FeatureChanged()` 是 `SendRefMsg(RM_FEATURECHANGED)` ⇒ 应过缓存。**不在原 17 点清单的原因**：清单口径只收了 `chat::hear_targets` 直算点。 | 同 7 系列接 `ref_msg_cache::targets`（`skip` 语义保留为循环内过滤）；牙：同窗两次 `feature_changed` 的 12 格边界只收首帧。 |

### 残余普查方法（可重跑；2026-11-20 定）
1. 全枚举 `bins/game-svr/src/` 的收件人循环：`for … in online` / `for … in online.iter()` /
   `world.entities.iter_ref()`（及 `sessions` 上的同类循环）；
2. 逐个取**所在函数**，判断它是否「把某一帧按**空间收件人**（同图/±range/`!Ghost`）广播出去」；
3. 回到 oracle 判机制：`SendRefMsg` ⇒ **旁路，应过 500ms 缓存**；`SendMsg` / `SendBroadCastMsg` /
   自队列 / 状态写 ⇒ **不属**（写明依据）；
4. 与已有清单（17 点 + #19/#20）比对：未登记 ⇒ 按上表格式补一行（符号锚 / 现状 / 修法 / 牙）。
5. **不把「手写循环」一律当旁路**——判据是 oracle 机制，不是 Rust 写法。

## 3. 修法（统一）

1. 每处把 `chat::hear_targets(world, id, range)` 换成 `crate::ref_msg_cache::targets(world, id, range)`；
   候选集形态（#14）用 `targets_from_scan(world, id, candidates)`。
2. 不同帧族若属**同一次** `SendRefMsg` 的多帧（#1 的 53/638/656/31/1103/640），只调用一次缓存判定、
   共用同一收件人列表（oracle 是同一趟 `SendRefMsg` 内 `Sendmsg` 多次，不重扫）。
3. 牙：对每处构造"同窗口两帧 + 12 格边界观察者"的 golden 或单测，断言边界只收首帧；
   §16 流程（快照/还原核 sha/复跑基线）。
4. 与魔法线交叉（#1 动 `push_broadcast`）需魔法对拍回归 ⇒ 排期由 opc-07 定。

## 4. 进展（C2 分批，逐批一交）

### 批 1（已合 M46）
- #7 `handle_say_with_saves`、#13 `fan_out_npc_hear`（RM_HEAR）→ `ref_msg_cache::targets`；
  行为用例 `npc_hear_cache_window_edge_gets_first_say_only`（同窗口连说两句：+12 只收首句、
  +5 两句、+13 零帧）。

### 批 2（53 族）
- 站点：#4 `flush_healthspell_slots`、#6 `flush_run_frames`、#5 `drain_stamina`。
- 修法：#4/#6 在 `SendRefMsg` 副本循环**内**原位换 `ref_msg_cache::targets`
  （每次 `SendRefMsg` 各判一遍缓存；`ref_copies==0` 时零次调用，不得在循环外提前刷新）。
- **#5 取舍（记录）**：旧实现把 `chat::hear_targets` 提到 `runs` 循环**外**只算一次、被
  `runs × ref_copies` 复用。本批把目标集**下沉到 copy 循环内**逐次判缓存——oracle 每次
  `SendRefMsg(53)` 各过一遍 `SendRefMsgTick` 门；若只算一次，`ref_copies>1` 时 12 格边界
  仍会收满份数（这正是下游会漏的坑，已用用例钉住）。
- 行为用例（同窗口两份观察者副本）：`flush_healthspell_cache_window_edge_gets_first_copy_only`
  / `flush_run_frames_cache_window_edge_gets_first_copy_only`
  / `drain_stamina_cache_window_edge_gets_first_copy_only`：+12 只收首份、+5 收两份、
  +13 零帧、自见 3 条（`SendUpdateMsg` 1 + 自发副本 2）。
- 牙（§16：快照 `E:/UserTemp/refmsg-b2-snap/`，`cp` 还原并核 sha 相等）：
  M1/M2/M3 = 逐站点改回 `chat::hear_targets`，各自**只**红对应用例
  （`left: Some(2) / right: Some(1)`，+12 边界）；M4 = #5 目标集上提（红，同断言）；
  M5 = 去缓存 TTL ⇒ `ref_msg_cache_expires_after_500ms` 与 `np11_aoi` 的 `fresh_scan` 同红。
- 前置（同批独立提交）：隐藏发送者早退，见主卡
  `docs/issues/openmir2-visiblehumanlist-cache.md` 第 7 节。
- 门禁结果以本批交付的 `status` 条目原文为准（不在卡面预写"全绿"）。

### 批 3（657 族）
- #2 `push_charstatuschanged`：收件人改走 `ref_msg_cache::targets`（每次调用 = 一次
  `StatusChanged()` 的 `SendRefMsg`；**含自己**；`Series` 逐收件人取各自 `HitSpeed`
  ⇒ 不能共用 body、**不走** `SpellFrameBuf::push_broadcast`）。用例：
  `push_charstatuschanged_cache_window_second_event_hits_cache`（同窗两份副本：`|Δ|=11/12`
  只收首份、`+5` 收两份、`+13` 零帧、自己两份都收）、
  `push_charstatuschanged_hidden_sender_only_self_each_event`（隐藏 ⇒ 只自己，旧快照 +12 不复活）、
  `drain_b3_two_transparent_events_share_cache_window`（真实调用面：同 tick 两条
  `RM_TRANSPARENT` 到点，第二条命中缓存）。
- #3 `broadcast_charstatuschanged_except_self`：`StatusChanged()` 的"其它玩家"支改走
  `ref_msg_cache::targets`（保留 skip self；隐藏 sender ⇒ `[self]` 被 skip ⇒ 观察者零帧）。
  用例：`broadcast_charstatuschanged_except_self_cache_window_second_event_hits_cache`、
  `broadcast_charstatuschanged_except_self_hidden_sender_has_no_observers`、
  `login_charstatus_two_events_same_tick_share_cache_window`（真实调用面「登录 657 补发」，
  同 tick 两事件共享缓存窗口）。

### 批 4（RM_PUSH / 走位扇出）
- #14 `broadcast_ref_msg_from_walk_candidates`：收件人由「候选集扫描直投」改为
  `ref_msg_cache::targets_from_scan(world, actor_id, scanned)`，`scanned` 仍由
  `hear_targets_from_walk_candidates` 给（候选集，非全实体）。
- **为什么用快照入口是安全的**（本批核心论点）：候选集来自**本 tick 实体表**
  （`collect_monster_walk_batch` 收集），位置与调用时刻一致 ⇒ 不存在「发帧时刻 vs 排水时刻」
  的过期快照。对比 `drain_merchant_disappears`：那条排水时 sender 已隐藏是**设计前提**，
  收件人必须是发帧时刻快照，所以继续用 `targets_from_scan` 且**不加**隐藏早退。
- **隐藏早退不可达的行为依据**：`collect_monster_walk_batch` 的 `ob_mode || fixed_hide_mode`
  门在收集阶段跳过隐藏走怪；用例 `hidden_walker_never_enters_walk_fanout` 断言
  `drain_monster_walk_downlink` 返回 **0 步**（扇出未被调用）+ 零帧。若该用例变红 ⇒ 必须按
  `4174de804` 同款补宿主早退。
- **单步等价（显式化）**：`monster_walk_frames_match_full_scan_reference` 的「与全扫描参照
  逐条相同」只在**每走位者一步**（首次 `SendRefMsg`，缓存必然刷新）前提下成立；代码注释与
  断言信息已改为「单步等价」。同窗第二步起走缓存，唯一证据是新用例
  `walk_cache_window_second_step_hits_cache`（走怪 x=10→11→12：`Δ=12` 首步收、`Δ=11` 次步挡、
  `Δ=5` 两拍都收、`Δ=13` 零帧）。
- **缓存无淘汰**：`ref_msg_cache` 条目按走位者 id 常驻、无淘汰，随走怪数量**线性增长**；
  当前量级数百可接受，若走怪数显著上升需重评（不新增问题：T-287 既有设计）。
- 牙：m9 = 站点回退 `hear_targets_from_walk_candidates` 直投 ⇒ 第二步 `Δ=12` 也收（红）；
  m10 = 去缓存 TTL ⇒ `ref_msg_cache_expires_after_500ms` + `np11 fresh_scan` 同红。

### 批 5（RM_USERNAME / 42 族）
- #15 `castle.rs::refresh_show_name_in_area`：内层收件人改走
  `ref_msg_cache::targets(world, target_id, ref_range)`（按被展示者键控）。
  **两个半径不许混**：外层 `range` 是 `GetMapRageHuman` 的目标筛选半径（`<=`），
  `ref_range` 才是 `SendRefMsg` 的收件人半径；缓存只用 `ref_range`。用例
  `castle_show_name_two_radii_are_independent` 钉住「外层半径变 ⇒ 收件人不变/无目标零帧」。
- #15 N1 丢帧与收件人集合分开验：`castle_show_name_n1_drop_is_color_not_recipient_set` ——
  `chat::hear_targets` 对照证明扫描面**含**攻方观察者，而它在 `query_user_name_color` 的
  N1 支零帧；同集合无会观察者照收 ⇒ 丢帧发生在颜色判定，不是缓存少给收件人。
- 同窗缓存证据：`castle_show_name_cache_window_second_refresh_hits_cache`
  （首份 Δ12 收、次份 Δ11/Δ12 挡、Δ5 两拍收、Δ13 零帧）。
- #16 `guild_cmd.rs::ref_show_name`：收件人改走
  `ref_msg_cache::targets(self.world, target_id, self.ref_range)`；同一次 `flush` 里对同一
  target 的两条 `Deferred::RefShowName` 各判一遍缓存。用例：
  `ref_show_name_cache_window_second_call_hits_cache`、
  `ref_show_name_two_deferred_in_one_flush_share_cache_window`（多副本专项：一次 flush 两条）、
  `ref_show_name_hidden_target_only_self`（隐藏 ⇒ 只自己）。
- #17 `guild_official.rs::handle_select`（建会成功支）：收件人改走
  `ref_msg_cache::targets(world, actor_id, ref_range)`；隐藏被展示者 ⇒ `[self]`（oracle 退化
  自投）。用例：`build_guild_ref_show_name_cache_window_second_build_hits_cache`（同窗两次
  建会，不同会名：首建 Δ12 收、次建 Δ11/Δ12 挡）、
  `build_guild_ref_show_name_hidden_hero_only_self`（隐藏 ⇒ 只自己）。
- #18 `guild_official.rs::war_events_to_frames`：`GuildWarEvent::RefShowName` 的收件人改走
  `ref_msg_cache::targets(world, e.actor_id, ref_range)`（按成员键控）。用例
  `war_events_two_ref_show_name_same_member_share_cache_window`（多副本专项：一次 events
  里两条同 member ⇒ 第二条命中缓存；Δ11/Δ12 只收首份、Δ5 两份、Δ13 零帧）。
- **批 5 既有 42 用例核查（四条都无需改期望，QA 可逐条抽查）**：
  1. `enter.rs::username_and_daychanging_are_wired_in_order` —— `enter.rs` 全文
     `ref_msg_cache|hear_targets` 命中 **0**；用例经 `enter::apply` + `EnterWorldInputs::frames()`
     纯编码器构造登录包（ClientQueryUserName + RefShowName 两条 42），不经缓存；
  2. `smframe::ref_show_name_emits_second_username_frame_byte_identical` ——
     `login_scenario_inputs(...).frames()` 纯编码器，同上；
  3. `query_user_name_near_username_far_ghost_missing_no_frame` —— 经
     `handle_query_user_name_cmd`（CM_QUERYUSERNAME）：单 body 直发请求者 socket，**不**走
     `hear_targets`/缓存（函数体内无这两者）；
  4. `slave_mutiny_downlink_emits_namecolor_then_username` —— 属 **批 6 的 #8**
     （`drain_slave_mutiny_downlink` 仍走 `chat::hear_targets`），单事件单次 `SendRefMsg`；
     批 5 未动该站点。

### 批 6 裁定（opc-07 2026-11-20）
- **#8 `drain_slave_mutiny_downlink` = 行为修正（不是重构）**：oracle 对每只叛变宝宝连续调用
  `RefNameColor(); RefShowName();`（`PlayObject.Base.cs` 的 `PlayObject` 叛变收尾段），即**两次**
  `SendRefMsg` ⇒ 第二次（`RM_USERNAME`/42）必然命中 500ms 缓存、走 `|Δ|<11`。现实现把一份收件人
  集合同时用于 656 与 42，会让 12 格边界同时收 656+42，**而 oracle 只收 656**。批 6 按「每帧各判
  一次缓存」落，并在用例里钉住「656 收而 42 不收」的边界差；如有既有用例随此改变，按 oracle 改
  期望并留痕（不静默）。
- **#11 `apply_pending_gm_target_ops` / #12 `apply_pending_gm_guild_commands`：要真实调用面多副本**
  （经生产排水入口在 500ms 窗口内两次事件），函数级两次直调只能作为补充；若夹具存在硬障碍，
  明说并登记「真实调用面未覆盖」，不得默认降级。

### 批 7（`push_broadcast`，按函数拆 7a/7b/7c；共 17 个调用点）
> 拆分（opc-07 裁定）：7a `drain_mag_struck_b2`（4 点）→ 7b `finish_spell_tail_with` +
> `handle_spell_cmd_with_magic`（9 点）→ 7c `push_name_color(_self)` + 1100 两处（4 点）。
> 实现：新增 `SpellFrameBuf::push_broadcast_cached`（投递段与 `push_broadcast` 逐字相同，
> 仅收件人走 `ref_msg_cache::targets`）；逐站点迁移，**旧 `push_broadcast` 在 7c 迁完后应无调用者**。

#### 7a `drain_mag_struck_b2`（4 点）
- **oracle 逐点锚与顺序**：`BaseObject.Base.cs` 的 `Operate(RM_MAGSTRUCK)` 支 =
  `StruckDamage()` → `HealthSpellChanged()`（53；`ShowHp` 时 `SendRefMsg`，无 race 门）→
  `SendRefMsg(RM_STRUCK_MAG)`（31）；随后自投 `RM_STRUCK` 由消息泵处理 →
  `BreakHolySeizeMode()`（`AnimalObject.cs`）→ `RefNameColor()`（`BaseObject.cs`）→ 656。
- **无「一次 SendRefMsg → 宿主多帧」共用判定点**：三次独立 SendRefMsg ⇒ 同 sender 下
  ① 刷新（±12 闭区间）、②③ 命中（`|Δ|<11`）。
- **两种 656 不混**：本函数 656 的 sender 是**受击者**（BreakHolySeizeMode→RefNameColor）；
  `MagStruckOutcome.hitter_name_color` 的 656 是**攻击方**（`SetPkFlag→RefNameColor`），
  **不在 7a**（由 `flush_hitter_name_colors` 另发）。
- 用例（跨帧族同窗口）：`mag_struck_player_53_then_31_share_cache_window`（边界只收 53）、
  `mag_struck_monster_53_31_656_share_cache_window`（边界只收 53，31/656 都被挡）、
  `mag_struck_hidden_player_only_self`（隐藏 ⇒ 只自己）。

#### 7b `finish_spell_tail_with` + `handle_spell_cmd_with_magic`（9 点）
- **关键事实**：每次施法在 `DoSpell` 的 `:159` 先发 `RM_SPELL(17)`（`broadcast_ref_msg`，已接缓存）
  ⇒ 7b 的 53/638/639/657/716 都是**同一 sender 的后续 SendRefMsg**，一律**命中缓存**
  （`|Δ|<11`）：12 格边界只收 17，不收这些帧。**无共用判定点**（每帧独立 SendRefMsg）。
- 逐点表：

| 站点（符号） | 帧 | oracle 锚 | 同次施法次序 | 期望 |
|---|---|---|---|---|
| `finish_spell_tail_with` 的 `tail.magic_fire` 支 | 638 | `MagicManager.cs` `:648-658`（`boSpellFire ⇒ SendRefMsg(RM_MAGICFIRE)`） | 17 之后 | 命中 |
| `handle_spell_cmd_with_magic` 护盾（31）buff 支 | 657 | `BaseObject.StatusChanged()` | 17 之后 | 命中 |
| 14/15 buff 支（`take_buff_applies` 循环） | 657 | `CharacterObject.cs:96-114` → `RecalcAbilitys/StatusChanged` | 17 之后 | 命中 |
| 32 圣言术 `invalid_cast` 支 | 639 | `PlayObject.Base.cs` `:1525-1529`（吞 `InvalidCastException`） | 17 之后 | 命中 |
| 42 群雷 638 | 638 | `MagicManager.cs` 的 `MagGroupLightening`（循环前无条件 638） | 17 之后 | 命中 |
| 42 群雷 716（icons 逐目标） | 716 | `MagicManager.cs` 的 `MagGroupLightening`（逐目标循环里的 `RM_10205`） | 638 之后、逐目标 | 命中（多次） |
| 9/10/22/36 延迟 53 | 53 | `BaseObject.cs:700-709` `HealthSpellChanged` | 17 之后 | 命中 |
| 36 无极真气 amulet 失败支 | 639 | `PlayObject.Base.cs:644-647`（`boSpellFail` 保持 true） | 17 之后 | 命中 |
| 36 无极真气 `invalid_cast` 防御臂 | 639 | `PlayObject.Base.cs:1525-1529`（同上） | — | **当前入口不可达**（登记；行为牙不可用） |
- **判据形态（写死）**：**17 刷新、7b 全命中**——12 格边界只收 `SM_SPELL(17)`；专钉用例
  `spell_boundary_only_gets_spell_frame_17`（|Δ|=11/12 收 17、不收 53/638；+13 零帧）。
- **缺口登记（36 `invalid_cast` 防御臂）**：`handle_spell_cmd_with_magic` 的 36 无极真气
  `out.invalid_cast` 支（`PlayObject.Base.cs:1525-1529` 语义镜像）**当前入口不可达**——
  `cast_uenhancer_spell` 的 `invalid_cast = 目标 race != Play`，而上游 `is_proper_friend`
  要求 PLAY 目标 ⇒ 该臂只有在「非玩家目标也满足 proper-friend」的语义出现时才可达（当前
  无此模型）。**m36 记为「该站点无可用行为牙」，既不算通过也不算失败**；其余 8 点牙必须行为化。
- **分支归属（逐点核过，2026-11-20）**：
  - 13029 638（`finish_spell_tail_with` 的 `tail.magic_fire` 支）← 具体入口如 **22 火墙** arm
    （`match req.magic_id` 的 `22 =>` 调 `finish_spell_tail`）；
  - 13739 657 ← **31 护盾**（`SKILL_SHIELD` arm）；
  - 13907 657 ← **14/15**（`SKILL_HANGMAJINBUB | SKILL_DEJIWONHO` arm）；
  - 15238 639 ← **32 圣言术**（`SKILL_KILLUNDEAD` arm 的 `out.invalid_cast`）；
  - 15719 638 / 15730 716 ← **42 群体雷电**（block `matches!(6|42|48|50)` 的 `_` arm）；
  - 15826 53 ← **9/10/22/36 block**（`matches!(9|10|22|36)`，`deferred_health` 出帧，在 match 之前）；
  - 15943/15964 639 ← **36 无极真气**（同 block 的 `36`/`_` 相关臂；15964 为不可达防御臂）。
  对应用例：22→`spell_shared_tail_53_and_638...`、42→`spell_group_lightening...`、
  31→`spell_shield_657...`、14/15→`spell_defence_area_657...`、32→`spell32_turn_undead_invalid_cast_639...`、
  36→`spell36_amulet_fail_639...`；另有 `spell_boundary_only_gets_spell_frame_17`。
- 用例：`spell_shared_tail_53_and_638_hit_cache_after_spell_frame`（53/638）、
  `spell_group_lightening_638_and_716_hit_cache`（638+716 多次）、
  `spell_shield_657_hits_cache_on_each_cast`（657）、`spell_defence_area_657_hits_cache`（657）、
  `spell36_amulet_fail_639_hits_cache`（639）、
  `spell_broadcast_cache_decides_by_call_order_not_queue_priority`（**调用序 ≠ 出队优先级序**：
  先调 657（155）后调 638（136），线上 638 在前，但缓存按调用序 ⇒ 657 收、638 挡）。

### 残余普查补漏表（2026-11-20，7b 期间；只登记不改）
> 口径：**凡是自己算空间收件人并广播的循环都算旁路**，不管调不调 `chat::hear_targets`。

| 符号 | 是否空间广播旁路 | 已登记 | 判据/处置 |
| --- | --- | --- | --- |
| `lib.rs::broadcast_healthspellchanged` | 是（53，手写 `online` 循环） | **#19（本批补登）** | oracle `HealthSpellChanged→SendRefMsg` |
| `lib.rs::broadcast_feature_changed(_except)` | 是（41，手写循环 + `skip`） | **#20（本批补登）** | oracle `BaseObject.FeatureChanged()` = `SendRefMsg` |
| `lib.rs::apply_pending_gm_mountride` | 空间循环，但帧 `SM_MOUNTRIDE(1118)` 是 **Rust 自造（D-15，oracle 骑马死）** | 不登记 | 非 oracle `SendRefMsg`，缓存语义不适用 |
| `castle.rs::fan_out_sysmessage_to_all` / `lib.rs::fan_out_world_sysmessage` | 否（SysMsg 广播，`SendBroadCastMsg` 语义） | 不登记 | 非 `SendRefMsg` |
| `lib.rs::broadcast_day_changing` | 否 | 不登记 | oracle 是 `SendMsg(RM_DAYCHANGING)` 自队列 |
| `lib.rs::drain_line_notice` | 否 | 不登记 | 行公告（`SendMsg` 系） |
| `apply_pending_gm_castle_commands`（StartContest） | 否 | 不登记 | 状态写循环（FightZoneDieCount=0），不出帧 |
| `lib.rs::drain_buff_downlink` | 否 | 不登记 | 按 actor 单播，非空间广播 |
| `lib.rs::drain_aoi_downlink` | 否 | 不登记 | 出现帧按观察者 `SendMsg`，非 `SendRefMsg` |
| `drain_npc_merchant_turns/hits`、`drain_merchant_disappears`、`fan_out_npc_hear`、`handle_say_with_saves` | 是 | 已覆盖（C1/C3/批1） | — |

#### 7c `push_name_color` / `push_name_color_self` / `drain_close_health_for` / `push_open_health`（4 点）
- **形态（与 7b 相反）**：非施法驱动、**无 `RM_SPELL(17)` 预热** ⇒ 同一 sender 的
  **首调用刷新**、同窗第二次调用才**命中**（`|Δ|<11`）。
- oracle 锚：656 ← `BaseObject.RefNameColor()` → `SendRefMsg(RM_CHANGENAMECOLOR)`；
  1100/1101 ← `BaseObject` 的 `SendRefMsg(RM_OPENHEALTH/RM_CLOSEHEALTH)`。
- `include_self` 逐点：`push_name_color` = **false**（自己那份由调用方另发；隐藏 sender 时
  `targets=[self]` 被 skip ⇒ **零帧**）；`push_name_color_self` = **true**（隐藏 ⇒ **只自己收**）；
  `drain_close_health_for` / `push_open_health` = true（隐藏 ⇒ 只自己）。
- 用例：`push_name_color_two_calls_share_cache_window`、`push_name_color_hidden_sender_has_no_frames`、
  `push_name_color_self_two_calls_share_cache_window`、`push_name_color_self_hidden_sender_only_self`、
  `close_health_two_events_share_cache_window`、`open_health_two_calls_share_cache_window`。

**7b / 7c 口径对照（防误读）**

| 批 | 形态 | 期望 |
| --- | --- | --- |
| 7b（施法尾帧） | 施法前 `RM_SPELL(17)` 已刷新缓存 | 53/638/639/657/716 **全部命中**；12 格边界**只收 17** |
| 7c（名色/血条） | 无预热 | **首调用刷新、同窗第二次命中**；隐藏按各点 `include_self` 语义 |

### 残余 #19 / #20 实现（2026-11-20；证据待 lane）
- **#19 `broadcast_healthspellchanged`**：收件人改走 `ref_msg_cache::targets(world, actor_id, range)`；
  隐藏 sender 由 `targets` 早退返回 `[self]`（源在 online ⇒ 只自己；不在 ⇒ 零帧），与 oracle 退化自投一致。
  既有用例 `broadcast_healthspellchanged_boundary_dx12_in_dx13_out_cross_map_out`（首次调用=刷新，
  dx=12 仍收）与 `broadcast_healthspellchanged_hidden_play_source_gets_degraded_self_copy`
  （隐藏=只自己）**期望不变**。
- **#20 `broadcast_feature_changed(_except)`**：同上走缓存；`skip` 保留为**循环内过滤**：
  `skip=None` ⇒ 含自己（隐藏 ⇒ 只自己）；`skip=Some(self)` ⇒ 零帧（调用方已有自见帧）。
- 用例：`healthspellbroadcast_two_calls_share_cache_window` /
  `healthspellbroadcast_hidden_sender_only_self` / `feature_changed_two_calls_share_cache_window` /
  `feature_changed_skip_semantics` / `feature_changed_hidden_sender_respects_skip`。
- 形态同 7c：**非施法驱动 ⇒ 首调用刷新、同窗第二次命中**。
