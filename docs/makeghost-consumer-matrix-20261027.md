# MakeGhost 消费者矩阵（M-1；卡 `openmir2-makeghost-lifecycle`，2026-10-27）

> **这份文档是什么**：M-1 片的交付物 —— 母卡 A2「穷举所有读 `entities` 的路径 × 会话 ghost 时的结论」的
> **枚举与分类**，外加 A4 不变量（`stage=Left` 与 `actor_id=Some` 并存）的语义定义。**零代码改动**。
> 逐行的「每条一颗牙」属于 M-2，本片只给**每条该在哪钉**的落点。
>
> 取数口径固定在本文件写作时的 `master = ab54a2350`；行号是**该 commit** 的。

## 0. 先读这条：今天「ghost 实体仍在」**还不存在**

`World::soft_close` 与 `world.disconnect` 走的是**同一条**销毁路径
（`crates/world/src/session.rs:489-537` `leave_world_inner`）：

```
remove_from_map(actor_id)  →  entities.remove(actor_id)  →  sess.actor_id = None  →  sess.stage = Left
                                                    （soft 只多一次 soft_close_count += 1）
```

⇒ **当前**：软关之后实体**已从 `entities` 移除**、`actor_id` 已清 ⇒ 所有消费者天然看不见它。
⇒ **母卡要做的（A1）** 是把 `soft_close` 改成 oracle 的**非销毁**形态（`BaseObject.Base.cs:484-495`
`MakeGhost`：`Ghost=true` + `DisappearA()`，对象**仍留在** `PlayObjectList`）：
**实体保留 + `ghost=true` + 已摘格 + `stage=Left` + `actor_id` 保留**，直到 `disconnect`/`Run` 的移除门。

因此本文档的每一行都是**前瞻评估**：「M-3 落地后，这条路径会看到什么」。
凡是今天看是 ✅ 的行，M-3 之后都可能变成 ⚠️ ——**这正是本矩阵存在的意义**。

## 1. 取值方法与口径

- 扫描范围：`mir2-rs/crates/world/src/**`、`mir2-rs/bins/game-svr/src/**`，**排除** `tests.rs` 与 `tests/` 目录；
  再按各文件的 `mod tests {` / `#[cfg(test)]` 行号把**测试内的站点剔掉**（`lib.rs:26060`、`world.rs:7923`、
  `monai.rs:6941`、`merchant.rs:1920`、`guardlist.rs:200`、`chat.rs:51`(仅一个 `use`)、`gmcmd.rs:4733`、
  `state_log.rs:151`、`party.rs:812`、`slave.rs:2300`、`buff.rs:557`、`npc.rs:522`、`combat.rs:1784`、`aoi.rs:541`）。
- 迭代面：全文正则 `entities\s*\.\s*(iter|ids|len|values)\s*\(`（**跨行**匹配）⇒ 全仓 **33 处**，
  剔掉**测试内 8 处**（**逐个点名**：`merchant.rs:2191`、`monai.rs:7444`、`monai.rs:7470`、`guardlist.rs:281`、
  `guardlist.rs:460`、`lib.rs:26081`、`lib.rs:46322`、`lib.rs:74208`）
  ⇒ **生产迭代面 25 处**（**33 = 25 + 8**；§3 的 B 表用 **24 行**覆盖这 25 处——B20 一行含两处站点）。
- 按 id 直查：`entities\s*\.\s*(get|get_mut)\s*\(` ⇒ **2720 处 / 1038 个函数**，逐条列没有意义；
  按「**id 从哪来**」分组（§4）——安全性由**名单生产者**决定，不由查表本身决定。
- 状态码：

| 码 | 含义 |
|---|---|
| ✅ | **已滤**：该路径有 `!ghost`（或等价）判据，M-3 后仍安全；给行号 |
| ⚠️ | **需补过滤**：M-3 后 ghost 会被看见/被处理，且与 oracle 不符（或无法举证相符）；M-2 要为它补一颗牙 |
| 🧱 | **结构性不可达**：给出「为什么到不了」的行号举证；**举证不了就不许用这个码** |
| 🟡 | 只读数据 / 诊断 / 计数，**无帧面风险**（可能影响计数，需对照 oracle 口径） |

## 2. A. 会话驱动（入口 = `world.sessions`，靠 `stage` 门天然隔离）

`WorldStage::InWorld` 门在 `bins/game-svr` 里出现 **10 次**（`grep -c 'WorldStage::InWorld' mir2-rs/bins/game-svr/src/lib.rs`）。
软关置 `stage=Left` 后，下列路径一律 `continue`。

| # | 路径 | 位置 | 会话 ghost 时 | 依据 |
|---|---|---|---|---|
| A1 | `drain_aoi_downlink`（AOI 出现帧下行） | `lib.rs:15064` | 🧱 跳过 | `:15073` `if sess.stage != WorldStage::InWorld { continue }`；**且**目标侧 `aoi.rs:228-232` 有 `!other.ghost` |
| A2 | `drain_depart_downlink`（离世广播） | `lib.rs:14993` | 🧱 不在名单 | 事件来自 `world.drain_departs()`，只在**离开那一刻**产生 |
| A3 | `drain_login_tail`（登录尾段排水） | `lib.rs:15575` | 🧱 无登录帧 | 只有 `login_pending` 有该会话时才有事做；ghost 不重登 |
| A4 | `drain_close_health_residual`（ShowHp 到期扫描） | `lib.rs:17001-17013` | **⚠️ 需补过滤** | 它 `world.entities.iter().map(|e| e.id)` 取**全部**实体再逐个 `drain_close_health_for`；后者只判 `e.show_hp && now - e.show_hp_tick > interval`（`lib.rs:17649`），**没有 ghost/stage 门** ⇒ M-3 后「ghost 且 `show_hp=true`」会以它为 `Recog` 向 ±12 玩家出帧，而 oracle 的 ghost 对象 `Run` 不再跑（`PlayObject.Message.cs:360` 起的 `Run` 链路）⇒ 不该出 |
| A5 | `drain_periodic_save`（周期存档） | `lib.rs:17224-17255` | **⚠️ 需补过滤（stage/ghost 门）** | 名单 = `sessions.keys()`（排序后逐个 `save_human_rcd`，`:17239-17240`），**没有 stage 门**。oracle 的**周期**存档**排除 ghost**：`WorldServer.cs:648` `if (!playObject.Ghost)` 是主干，`:698-702`（Run + 周期 `SaveHumanRcd`）在**主干内**；ghost 走 **`else`**（`:707-716`：`:709 PlayObjectList.Remove` + `Disappear()` + `:713 SaveHumanRcd` + `:714 CloseUser`）⇒ **ghost 的那次存档发生在「移除时」**（在本仓对应断线存档），**不是**周期存档。⇒ M-3 后 `drain_periodic_save` 必须按 `stage`/`ghost` 跳过；`save_human_rcd` 在 `actor_id=Some` 时读同一份实体这一点仍要复核（§5） |
| A6 | `GateEvent::Closed` 臂（断线/软关存档） | `lib.rs` 的 `GateEvent::Closed` 分支 | **母卡要修的点** | 它按「实体在世」取派生字段（`entity_derived_snapshot`），而今天 soft_close 已把实体摘掉 ⇒ 派生字段静默回落。M-3 让实体保留后这条自然正确；**M-2 应为「Closed 的存档字段与 disconnect 前一致」补一颗牙** |

## 3. B. 实体迭代（`entities.iter/ids/len/values`，生产 **25** 处 / 24 行；B20 一行两处）

排序 = 文件 + 行。结论一律回答「ghost 实体仍在 `entities` 里时，这条路径看到什么」。

| # | 位置 | 消费者 | ghost 时 | 依据 / 说明 |
|---|---|---|---|---|
| B1 | `crates/world/src/world.rs:1976` `mag_make_defence_area` | 魔法防御区（按格找对象） | ✅ **已滤（门在循环体内，不在 filter 里）** | 收集循环的 filter **不含** ghost（只判 `map_id`/`x`/`y`/`race`），但门在**循环体内**：`world.rs:1991` `if obj.ghost { continue; }`（函数 docblock `:1949-1955` 也写明「`Ghost` 目标跳过（oracle `!baseObject.Ghost`）」）⇒ ghost 不会被命中。**写法提醒**：这一行读 filter 会误判成「没门」 |
| B2 | `world.rs:5203` `tick_head`（`player_hiter_snapshot`） | 每玩家 hiter 快照（数据） | 🟡 / **⚠️ 低风险** | 只按 `race==PLAY` 入快照，无 ghost 门。快照只被 P32 消费，而 P32 在会话驱动侧受 stage 门（A1）；但**快照内容会含 ghost** |
| B3 | `world.rs:5385` `tick_tail`（by_map 刷新） | AOI 状态刷新（viewer 循环） | ✅ **已滤（收集时不判、但自扫门判了）** | 收集 `(id, map_id)` 时确实**不判** ghost；但**自扫门**在 `world.rs:5435`：`Some(e) if e.race == PLAY => !e.ghost && …`（怪物支同：`Some(e) if !e.ghost && monster_self_scans(e)`）⇒ ghost **不会** `SearchViewRange`、不改 `visible_actors`。target 侧另由 `aoi.rs:228-232` `!other.ghost` 过滤。**这一行值得留在矩阵里的原因**正是「收集处没门、自扫处有门」这个结构 |
| B4 | `world.rs:5644` `expire_player_run_state` | 玩家 RunState 过期 | ✅ | filter：`race==PLAY && !e.ghost` |
| B5 | `world.rs:5709` `check_dup_obj` | 顶号检测 | ✅ | filter：`race==PLAY && !e.ghost`（顶号检测不拿 ghost 当「在线」） |
| B6 | `world.rs:5923` `verify_players_map_time` | 玩家图时效 | ✅ | filter：`race==PLAY && !e.ghost` |
| B7 | `world.rs:6542` `expire_hitters` | hiter 过期清理 | ✅（有意） | filter：`e.death || e.ghost` —— ghost 被当作「已消失」，用来清别人的 `last_hiter` |
| B8 | `world.rs:6755` `settle_deaths` | 死亡结算 | **⚠️ 需补过滤** | filter 只有 `(hp==0 || die_called) && !death_settled && race 匹配`，**无 ghost 门** ⇒ M-3 后「ghost 且 `die_called`」会被结算（掉落/经验/退组），而 oracle 的 ghost 不 `Run` ⇒ 不会走到 `Die()` |
| B9 | `crates/world/src/monai.rs:4979` `base_object_run_monster`（`PlayObjectCount`） | MonGen 门计数 | ✅（有意不滤） | `race==PLAY` 计数。代码注释（`lib.rs:24762-24767`、`:24965-24967`）明写：oracle `OnlinePlayObject`/`PlayObjectCount` = `PlayObjectList.Count`，**`MakeGhost` 后仍在表里** ⇒ 必须同口径 |
| B10 | `monai.rs:5249` `monster_ai_tick` | 怪物 AI | ✅ | filter：`race > PLAY && merchant.is_none() && npc.is_none() && !e.ghost` |
| B11 | `crates/world/src/merchant.rs:464` `merchant_ids_sorted` | 商人 tick | 🧱 | 只取 `merchant.is_some()` —— 玩家永不带 `merchant` 态（`insert_merchant` 是唯一写点，`merchant.rs:357`） |
| B12 | `crates/world/src/npc.rs:316` `npc_ids_sorted` | 固定 NPC tick | 🧱 | 只取 `npc.is_some()` —— 玩家永不带 `npc` 态（`npc.rs:253` 唯一写点） |
| B13 | `crates/world/src/party.rs:284` `party_play_object_by_name` | 队伍按名找人 | **⚠️ 需补过滤** | filter 只有 `race==PLAY && name==…`。M-3 后 ghost 的名字仍能被组队/邀请命中。oracle 侧需对照 `GetPlayObject`/`GetPlayObjectByName` 是否排除 ghost —— 本片**给不出行号举证 ⇒ 按「需补过滤」办**（卡面要求：举证不了就按需补） |
| B14 | `crates/world/src/slave.rs:868` `slave_list_cleanup_all` | 宝宝表清理（全实体） | **⚠️ 需补过滤** | filter 只有 `!e.slave_list.is_empty()`。oracle 在**主人自己的 `Run()`** 里清表（`BaseObject.Base.cs:208-217`），ghost 不 `Run` ⇒ ghost 主人的宝宝表不该在这种集中扫描里被清（时序/条件都可能不同） |
| B15 | `bins/game-svr/src/chat.rs:209` `hear_targets_at` | 说话/喊话收件人 | ✅ | filter：`… && !e.ghost && …` |
| B16 | `bins/game-svr/src/chat.rs:297` `cry_targets` | 哭（`Cry`）收件人 | ✅ | filter：`… && !e.ghost && …` |
| B17 | `bins/game-svr/src/gmcmd.rs:1130` `play_by_name` | GM 按名找玩家 | ✅ | filter：`race==PLAY && !e.ghost && name==…` |
| B18 | `bins/game-svr/src/state_log.rs:98` `record` | 状态日志（诊断） | 🟡 | 全实体 dump，仅日志；无帧面 |
| B19 | `bins/game-svr/src/lib.rs:10561` `front_target` | 前端目标选择 | ✅ | filter：`… && !t.ghost && !t.death` |
| B20 | `lib.rs:14808` / `:14871` `world_aoi_diag` | AOI 诊断 | 🟡 | 只读快照；ghost 会进 `entities` 计数（诊断数值会变） |
| B21 | `lib.rs:17008` `drain_close_health_residual` | ShowHp 到期扫描 | **⚠️ 需补过滤** | 与 A4 同一处（会话侧看是「名单来自全实体」，实体侧就是这条） |
| B22 | `lib.rs:19461` `step_mon_gen`（`play_object_count`） | MonGen 门计数 | ✅（有意不滤） | 与 B9 同口径，见 `:24965-24967` 注释 |
| B23 | `lib.rs:24768` `sim_loop`（`online_play_objects`） | 在线数上报（IDS `SS_SERVERINFO`） | ✅（有意不滤） | 注释 `:24762-24767`：oracle `WorldEngine.OnlinePlayObject` = `PlayObjectList.Count`，**不能排除 ghost** |
| B24 | `lib.rs:24971` `sim_loop`（MonGen 门） | 刷怪门计数 | ✅（有意不滤） | 同上注释（两条 ZenTime 门必须同一口径） |

**已剔除（测试内，不计入矩阵）**：`monai.rs:7444/7470`、`merchant.rs:2191`、`guardlist.rs:281/460`、
`lib.rs:26081`（`tick_until_next_player_scan`）、`lib.rs:46322`、`lib.rs:74208` —— 全部落在各自文件的
`mod tests` / `#[cfg(test)]` 之后（行号见 §1 口径）。

## 4. C. 按 id 直查（2720 处 / 1038 函数）—— 安全性看**名单生产者**

逐条列没有意义（绝大多数是 `entities.get(id)` 的工具函数内部）。真正要判的是「**这个 id 从谁的名单来**」：

| 名单来源 | 生产者 | ghost 会进名单吗 | 结论 |
|---|---|---|---|
| 会话名单 | `world.sessions.iter()`（10 处 `stage==InWorld` 门） | 不会（`stage=Left`） | ✅ |
| 周期存档名单 | `sessions.keys()`（`lib.rs:17239`，**无 stage 门**） | 会 | **⚠️ 需补 stage/ghost 门**（§2 A5；oracle `WorldServer.cs:648` 的 `if (!playGhost)` 主干把周期存档挡在 ghost 之外） |
| 商人/NPC tick 名单 | `merchant_ids_sorted` / `npc_ids_sorted` | 不会（玩家无 merchant/npc 态） | 🧱 |
| buff 的逐 actor 表 | `buff.rs:328` `operate` 遍历的 `self.actors`（**不是 `entities`**，是 buff 自己的 actor 名单） | 会（ghost 仍在 `self.actors` 里，只判实体存在） | ⚠️ 见 §6#4——与 B 表同一条判据的另一种名单形态 |
| 宝宝表 | `slave_list_cleanup_all`（B14）+ 主人自己的 `slave_list` | 会（ghost 主人） | ⚠️ 见 B14 |
| 队伍/行会成员表 | `party_*` / `guilds` | 成员表里可能有 ghost 名字 | ⚠️ 见 B13（按名找）/ M-2 需按表核 |
| 地图格 | `MapGrid` 格索引 | **不会**（soft_close 已 `remove_from_map`，`session.rs:537`） | ✅ 移动/占格/拾取/碰撞全部走格索引（`map.rs`/`space_move.rs`）⇒ ghost 不占格 |
| 展示表 | `DisplayTable`（`downlink`） | 由 AOI 下行驱动；A1 stage 门 + 移除登记 | ✅（M-2 需为「ghost 不登记/及时摘除」补牙） |
| 战斗目标 | `combat.rs:705-716` `is_attack_target` | 不会：`target.ghost \|\| target.invisible \|\| target.death` 直接 false | ✅（行号 `combat.rs:712`） |
| 怪物目标选择 | `monai.rs` 的 `FindTarget` 系（按格扫 + `is_attack_target`） | 不会（同 combat 门） | ✅ |
| 死亡掉落 | `death_drop.rs`（由死亡事件驱动 + 格扫） | 不会：ghost 不触发死亡（但 B8 今天有个缺口） | ⚠️ 依赖 B8 修好 |

## 5. D. A4 不变量：`stage=Left` 与 `actor_id=Some` **并存**

M-3 之后会出现一个**今天不存在**的状态组合：

```
sess.stage == WorldStage::Left   AND   sess.actor_id == Some(id)   （且 entities 里仍有该 id、ghost=true、已摘格）
```

**含义（必须写进代码注释）**：

1. `stage` 的语义是「**这个会话还接不接收/产不产生帧**」，`actor_id` 的语义是「**世界里的实体还在不在**」。
   两者**解耦**：Left 表示前者为否，`actor_id=Some` 只表示后者为是。
2. 进世界守卫（`stage != InWorld` 一律拒绝）**维持不变**；`actor_id.is_some()` 不能再被当作「在世界上」的判据。
3. 因此所有「`world.session(..).actor_id` 的读者」必须逐个核一遍：**名单/守卫用 `stage`，取实体内容用 `actor_id`**。

读者清单（按 `actor_id` 出现的生产者归类，M-2 逐一确认）：

| 读者 | 位置 | 今天的语义 | M-3 后要确认什么 |
|---|---|---|---|
| 进世界/命令派发守卫 | `lib.rs` 各 `dispatch_*` 入口 | `stage==InWorld` 门 | 不变 |
| 存档记录构造 | `lib.rs:9404-9530` `build_save_record` | 按 `session.actor_id` 查实体，缺则回落 | ghost 时实体**在** ⇒ 走「在」的分支；与 A5/A6 的存档口径一致 |
| AOI/登录尾段/离世 | A1–A3 | stage 门 | 不变 |
| GM/聊天按名找 | `gmcmd.rs:1130` / `chat.rs` | 有 ghost 门 | 不变 |
| 软关自身（`soft_close`） | `session.rs:477` | 今天直接销毁 | M-3 改为非销毁；`soft_close_count` 语义不变 |

**构造牙（M-2 落点）**：造一个「`stage=Left` + `actor_id=Some` + `ghost=true`」的世界，断言
① 进世界守卫仍拒；② `build_save_record` 读到的是同一份实体；③ `drain_aoi_downlink` 不为它出帧。

## 6. 结论：M-3 落地前必须补的过滤点（供 M-2）

**⚠️ 八处（按优先级）**（reviewer-21 复审后：去掉两处已滤的 `mag_make_defence_area` / `tick_tail` viewer 侧，
新增 `drain_periodic_save`）：

1. `drain_close_health_residual` / `drain_close_health_for`（`lib.rs:17001`、`:17649`）—— 加 `!ghost`（或 `stage==InWorld`）；
2. `settle_deaths`（`world.rs:6755`）—— 加 `!e.ghost`；
3. **`drain_periodic_save`（`lib.rs:17224`）**—— 按 `stage`/`ghost` 跳过：oracle `WorldServer.cs:648` 的
   `if (!playObject.Ghost)` 主干把**周期**存档挡在 ghost 之外，ghost 的存档发生在**移除时**（`:707-716`）⇒
   本仓对应断线存档，不是周期存档；
4. `buff::operate`（`buff.rs:328`，`self.actors` 遍历，**只判实体存在、不判 ghost**）—— ghost 的 buff 会继续 tick（毒/回血/到期可能出 sysmsg/hear）⇒ 需按 oracle `ActorBuffProcessor` 名单核对后补门；
5. `slave_list_cleanup_all`（`slave.rs:868`）—— ghost 主人的宝宝表；
6. `party_play_object_by_name`（`party.rs:284`）—— 按名找人不排除 ghost；
7. `tick_head` 的 `player_hiter_snapshot`（`world.rs:5203`）—— 低风险，但要么补门要么在注释里写明「含 ghost 是有意的」；
8. `DisplayTable` 的登记/摘除（`npcs.rs`/`downlink`）—— M-3 后 ghost 不应再被 AOI 下行登记（A1 已 stage 门，需补一颗牙证）。

**✅ 复审后确认已滤的两处（不进 M-2）**：`mag_make_defence_area`（门在**循环体内** `world.rs:1991`）、
`tick_tail` viewer 侧（**自扫门** `world.rs:5435` `!e.ghost`）——两处的共同坑是「读 filter / 读收集循环会误判成没门」。

**🟡 需复核但本来就是同口径的两处**：`online_play_objects`（B23）、MonGen 计数（B9/B22）——
它们都按 `PlayObjectList.Count` 的 oracle 口径**含 ghost**，M-3 后**不要**顺手加 ghost 门，否则反而偏离 oracle。

## 7. 与 reviewer-21 `854d0e9a`（`fd08ff3a9` 基线）的差异

| 项 | reviewer-21 | 本篇 | 理由 |
|---|---|---|---|
| 迭代面计数 | 「共 31 处 `entities.iter()`」 | **生产 24 处**（全仓 33，测试内 9） | 本篇按各文件 `mod tests` / `#[cfg(test)]` 行号逐个剔除；reviewer 那版把 `monai.rs:7444/7470`、`merchant.rs:2191`、`guardlist.rs:281/460`、`lib.rs:26081/46322/74208` 记为 ❓/待判，本篇给出剔除依据 |
| `settle_deaths` | ❓ 未逐行确认 | **⚠️ 无 ghost 门**（`world.rs:6755`） | 逐行读了 filter：只有 `hp==0 \|\| die_called` + `!death_settled` + race，无 ghost |
| `mag_make_defence_area` | 未列 | **✅ 已滤**（门在循环体内 `world.rs:1991`） | reviewer-21 C1 订正：我先前只读收集循环的 filter、漏了循环体内的 `if obj.ghost { continue; }` |
| `monai.rs:7444/7470`、`lib.rs:46322/74208` | ❓ | **剔除（测试内）** | 行号在 `mod tests` 之后 |
| `guardlist.rs:281/460` | ⚠️ | **剔除（测试内，`guardlist.rs:200` 起）** | 测试辅助计数，不是生产路径 |
| `party.rs:284` | ⚠️ 无 ghost 门 | ⚠️ 保留 + **明确「给不出 oracle 行号 ⇒ 按需补」** | 卡面要求：举证不了就按需补过滤办 |
| `drain_periodic_save` | ⚠️ 需确认 | **⚠️ 确认需补门**（reviewer-21 C3） | 我先前只看到 `:698` 在 `PlayObjectList` 遍历里就下结论；reviewer 读了**主干**：`:648 if (!playObject.Ghost)` 才有周期存档，ghost 走 `else`（`:707-716`）在**移除时**存档 |
| 行号 | `fd08ff3a9` | **`ab54a2350`** | 代码已前进，本文档所有行号以当前 master 为准 |

## 8. 本片**没有**做的事（明确边界）

- 没有改任何代码（M-2/M-3 的范围）；
- 没有给「每条一颗牙」（M-2）；
- **没有真机证据**：A5（`softclose → disconnect` 窗口内两侧帧面）留给窗口；本片的结论全部来自源码与行号；
- `party.rs` 一条**没有给出 oracle 行号举证**，按卡面规则一律按「需补过滤」处理，不冒充 🧱。
