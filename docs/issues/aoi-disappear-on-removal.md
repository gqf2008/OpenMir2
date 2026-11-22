# `card/aoi-disappear-on-removal`：实体被删仍在他人可见集时的 RM_DISAPPEAR

> 状态：**设计页（只读 + 文档；未动代码、未构建、未 push）**。2026-11-22，从 L2-5 ②段 8 的 C10
> 红复现里剥离出来的**既有分歧**。卡面验收用例之一 = 段 8 冻结的 `step2_disappear_tick_window`。

## 1. oracle 依据

`PlayObject.cs:3206-3218` 的差集收尾：对观察者 `VisibleActors` 里仍为 `Hidden` 的项，取
`visibleBaseObject.BaseObject`——这是一个**保留引用**，**不查环境（格子/实体表）里还在不在**；
发帧守卫只有 `!FixedHideMode && !Ghost`：

```text
if (VisibleFlag == Hidden) {
    if (!baseObject.FixedHideMode && !baseObject.Ghost) SendMsg(baseObject, RM_DISAPPEAR, …);
    VisibleActors.RemoveAt(...); continue;
}
```

⇒ 只要对象在离开时**不是** Ghost/FixedHideMode，即使它已经从世界里被销毁，观察者的下一次扫描
也要收到 `RM_DISAPPEAR`。

## 2. 我们的两处实现（符号锚）

- legacy：`aoi.rs` 的 `search_view_range_with_updates` 收尾差集；
- 增量：`aoi.rs` 的 `incremental_gate` 的 `removed` 构造。

两处都写成了

```rust
let removed = removed_raw
    .into_iter()
    .filter(|t| store.get(*t).is_some_and(|e| !e.fixed_hide_mode && !e.ghost))
    .collect();
```

`store.get(t)` 为 `None`（实体已删）⇒ `is_some_and` 为假 ⇒ **被删目标永远进不了 `removed`**
⇒ 不出消失帧。这是**移植引入的既有分歧**（oracle 没有存在性检查），不是镜像带来的新语义；
镜像离带（把镜像从实体表删除）只是第一个把它变成可判据的场景。

## 3. 删除路径全量清单（谁会把实体从实体表删掉）

| # | 路径 | 删除动作 | 删除时在他人 `VisibleActors` 里？ | 当前出不出 `hides`（⇒`SM_DISAPPEAR`） | 修复后预期 |
| --- | --- | --- | --- | --- | --- |
| 1 | 玩家掉线（`World::disconnect` → `leave_world_inner(soft=false)`） | `departs` 广播采集后 `entities.remove` | 会（同图 ±`SendRefMsgRange`） | **出**——走 `drain_departs` 专用广播（`BaseObject.DisappearA` 等价物），不是 AOI `hides` | **无变化**：oracle 靠 `Ghost` 位抑制扫描侧第二次发送（`PlayObject.cs:3213` 注释「防止人物退出时发送重复的消息占用带宽」）；我们 `leave_world_inner` 先置 `ghost=true` 再摘格留实体 ⇒ 扫描侧被 `!ghost` 抑制，与 oracle 同形。**不得**再加按 tick 抑制机制 |
| 2 | 玩家小退 / 账号顶号（`soft_close` → `MakeGhost` 等价物） | 只置 `ghost=true` + 摘格，**实体保留** | 会 | **出**（`departs` 广播）；AOI 路径本来就被 `!Ghost` 守卫排除 | 无变化 |
| 3 | 尸体/幽灵 5 分钟腐烂（`mongen::process_cert_lists` → `release_entity`） | `disappear_a` + `entities.remove` | 会 | **不出**（目标 `ghost=true`） | 仍不出（oracle 守卫同样排除 Ghost）⇒ 无变化 |
| 4 | GM `CLEARMAPMON`（`clear_map_mon`） | 先 `MakeGhost`（`ghost=true`），5 分钟后走 #3 | 会 | 不出（同 #3） | 无变化 |
| 5 | 召唤物/分身消失（`slave::process_slave_ghost`，`mag_make_clone`） | **已核无物理删除点**：`slave.rs` 只置 `ghost=true`；`special_monster.rs`/`magic.rs` 无删除；`release_entity` 仅 cert_list 怪物（召唤物不进 cert_list） | — | 不适用（无删除） | **不适用**；若将来出现，走同一离场标记（U2 已核，见 §7） |
| 6 | 地图切换（`World::switch_map`） | 旧格摘除 + 新图落格，**实体保留** | 会（旧图观察者） | **出**（`store.get` 仍 Some） | 无变化 |
| 7 | 重进清旧 ghost（`enter_map_at` 的 `entities.remove(old)`） | 删旧 ghost 实体 | 会（观察者 1s 扫描还没跑时仍在 `VisibleActors`） | 不出（目标是 ghost） | **要靠离场标记抑制**：删除时记 `ghost=true`，扫描侧不发；否则修好过滤后会**补发一条**，而 oracle 因保留引用 + `ghost=true` **不发**（U3 的验收用例之一） |
| 8 | **镜像离带（L2-5 新增）** | `apply_mirrors` 陈旧清扫 → `remove_mirror` → `entities.remove` | 会 | **不出**（这正是段 8 C10 的红） | **出**——本卡首个实证需求 |
| 9 | 掉落物拾取（`VisibleItems` 路径） | 地面物品移除 | **不适用**（另一条可见路径） | 不适用 | **不在本卡范围，不得混改** |

## 4. 修法（提案，待实现窗口）

两处过滤器改成 oracle 语义：

```rust
.filter(|t| match store.get(*t) {
    None => true, // 已删：oracle 用保留引用 ⇒ 照发（除非其离开时是 Ghost/FixedHideMode）
    Some(e) => !e.fixed_hide_mode && !e.ghost,
})
```

注意「已删 ⇒ 照发」与「Ghost ⇒ 不发」的区别：**被删的 ghost**（#3/#7）在 oracle 里也不会发
（守卫用的是对象自己的 Ghost 位）；我们删了实体就读不到 Ghost 位 ⇒ **需要把"删时的 Ghost 位"
保留到扫描时刻（**离场标记**：删除时记 `(ghost, fixed_hide)`；到期做「仍被持有 ⇒ 续期」检查，
无人持有才丢弃；硬上限 30s + 计数。注意这与 L2-5 裁决「不走墓碑」不冲突：那是**镜像专属**机制，
这里是 AOI 侧对**任意被删实体**的统一口径）。这是本卡的主要实现决策点（U3）。

## 5. 影响面与待判未知项

**影响面声明**：改的是**所有「被删除且删除时仍在他人 `VisibleActors` 里」的实体**的消失帧行为，
**legacy 与增量两条 AOI 路径都变**，属 R 级对拍范围（真实 oracle 窗口 + 现有 golden 回归）。

- **U1（玩家掉线双发）——已裁决，不再是未知项**：oracle 的 `PlayObject.cs:3213` 注释明说守卫是为了
  「防止人物退出时发送重复的消息占用带宽」——**oracle 就是靠 `Ghost` 位抑制「离开广播 + 下一次扫描」
  两次发送**。我们 `leave_world_inner` 也先置 `ghost=true` 再摘格、保留实体（`session.rs` 的 A2
  不变量）⇒ 掉线本来就只发一次（`departs` 那一次），扫描侧被 `!ghost` 抑制，与 oracle 同形。
  **不要**新增「按 tick 抑制已被 `departs` 覆盖的 id」这类移植侧新机制。
- **U2（召唤物/克隆的删除是否经过 ghost）——已核（命令与原始输出见 §7）**：`slave.rs` 无
  `entities.remove`/`release_entity`（只置 `ghost=true`）；`special_monster.rs`/`magic.rs` 无删除点；
  `release_entity` 的唯一调用者是 `mongen::process_cert_lists`（ghost 5min 清理），而
  `cert_list.push` 只在 `mongen.rs` 的 MonGen 登记处 ⇒ **召唤物/分身不进 cert_list，本卡无此类删除
  用例**。若将来出现，走同一离场标记。
- **U3（AOI 通用「离场标记」）——已采纳，措辞按裁决**：要做的是 **AOI 通用的「离场标记」**——
  实体被删除时记下 `(ghost, fixed_hide)`；**不是镜像专属墓碑**。**生命周期（Leader 2026-11-22
  二次裁决）**：不是「首个消费即删」，也不是墙钟 TTL 删除——两者都会让**扫描被推迟**的观察者漏发
  （oracle 的保留引用不会漏）。做法：到期（`DEPARTURE_MARK_TTL_MS = 2×玩家扫描周期`）时做一次
  **「还有没有观察者持有」**检查（遍历 `visible_actors`；无反向索引，标记很少、到期才扫）——
  仍被持有 ⇒ **续期**；无人持有 ⇒ 丢弃；另有**硬上限 30s**（超限仍被持有 ⇒ 强制丢弃并计数
  `departure_mark_held_past_cap`，正常恒 0，真触发 = 观察者冻结 >30s 的已登记差异）。
  区别：它服务「任意被删实体仍在他人可见集里」这一**通用**情形（对应 oracle 的保留引用），
  **对单 World 也成立**；镜像只是它的众多使用者之一。
  实现要点：在 **`release_entity` / `disconnect` 的移除门 / `remove_mirror`** 三处删除入口统一
  记录；`incremental_gate` 与 legacy 两条路径同时消费并丢弃。
  **验收用例（不只掉线）**：`enter_map_at` 重进时清旧 ghost（`session.rs:598` 那类
  `entities.remove(old)`）——若观察者的 1s 扫描还没跑，`VisibleActors` 里仍有这条；修好过滤后若
  不记 `ghost=true` 会补发一条，而 oracle 不发 ⇒ 用离场标记抑制，并写一条正反用例（ghost 被删
  ⇒ 不发；非 ghost 被删 ⇒ 发）。
- **U4（增量路径的 `pending` 生命周期）——已核（命令与原始输出见 §7）**：`pending` 只在
  `incremental_gate`（`state.pending.remove(&viewer_id)`）被消费、在 `register`（紧跟一次全量扫）
  与 `unregister` 时清除；`begin_tick` 每 tick 往对应观察者的 `pending` 里累积 ⇒ 被删 id 会一直
  留到该观察者下一次扫描才被消费，**不会因窗口 diff 提前清空**。仍建议在实现窗口用 l2 多线程夹具 +
  玩家 1s 扫描门做一次实证（§6 第 2 条）。

## 6. 验收条件（本卡 Definition of Done 增量）

1. 段 8 的 `step2_disappear_tick_window` 去掉 `#[ignore]` 后**转绿**（它是本卡验收用例之一）；
   其反方向变异（`apply_mirrors` 挪到 `tick_phase2` 之后）必须打红。
2. 新用例：删除一个**非 ghost**、仍在他人 `VisibleActors` 里的实体 ⇒ 下一次扫描出
   `hides`（legacy 与增量各一条；假 viewer 用玩家 1s 门与非玩家每 tick 两类）。
3. 反向用例：删除一个 **ghost** 实体 ⇒ 不出（与 oracle 守卫一致）。
3b. **扫描被推迟到 > TTL**（如 2.5s）仍必须**恰收一条**消失（续期语义的牙；删续期必红）；
3c. **硬上限**：超 30s 仍被持有 ⇒ 强制丢弃 + `departure_mark_held_past_cap` 计数。
4. U1 已裁决：**不加**按 tick 抑制机制；用例钉住「掉线只发一次（`departs`）、扫描侧被 `!ghost`
   抑制」与「重进清旧 ghost 被离场标记抑制、不补发」。
5. `VisibleItems` 路径零改动（源码牙或测试断言）。
6. 合并门：五门 + `-Mode Both`（R 级窗口证据按判级要求）。

## 7. U2/U4 核验记录（命令 + 原始输出摘要）

```text
$ grep -n "entities.remove|release_entity|make_ghost|ghost = true" mir2-rs/crates/world-bevy/src/slave.rs
1263:            s.life_cycle.ghost = true;          # process_slave_ghost：只置 ghost
（无 entities.remove / release_entity）

$ grep -n "entities.remove|release_entity|make_ghost|ghost = true" special_monster.rs magic.rs
（无输出）

$ grep -rn "release_entity(" mir2-rs --include=*.rs | grep -v "fn release_entity|test"
mir2-rs/crates/world-bevy/src/mongen.rs:1106:  world.release_entity(id);   # 唯一调用者

$ grep -rn "cert_list.push|cert_list.extend" ...
mir2-rs/crates/world-bevy/src/mongen.rs:443:  self.cert_list.push(actor_id);   # 仅 MonGen 登记
（另有 bins/game-svr/src/lib.rs:106916 一处测试）

$ grep -n "pending.entry(|pending.remove(|清空其待复评集合" mir2-rs/crates/world-bevy/src/aoi.rs
1732: self.pending.entry(*viewer).or_default().insert(id);   # begin_tick：每 tick 累积
2024: state.pending.entry(viewer_id).or_default().extend(extra);  # window_diff
1792/1802: self.pending.remove(&viewer);                  # register / unregister
2113: let touched = state.pending.remove(&viewer_id)...     # incremental_gate：扫描时消费
```

结论：U2 = 核过，无召唤物/分身删除点；U4 = 核过，`pending` 保留到观察者下一次扫描（建议实现窗口
再以 l2 多线程夹具实证一次）。

## 8. 已知差异 / 假设（独立复核记录）

- **标记是「删除瞬间快照」，oracle 是「扫描时读保留对象」**：我们记下删除瞬间的
  `(ghost, fixed_hide_mode)`；oracle 在观察者扫描时读那个仍被引用的对象。两者等价的前提是
  **删除后不再有人改那个对象的状态**（`Ghost`/`FixedHideMode` 等）——**该前提未核**（未逐路径
  核 C# 是否还有持有引用改状态的代码）。若将来发现删除后仍有写入，需要把「删除后状态」也纳入
  标记（或在删除时机上做保证）。
- **id 复用**：今天不存在业务 id 复用（id 单调分配、槽位复用不换 id）⇒ 标记只走
  `store.get == None` 分支。将来若引入显式 id 复用，已删 id 可能被新实体占用：`Some` 分支会读
  **新实体**的位，而 oracle 读**旧对象** ⇒ 那一刻才分歧；届时需按「代际」区分标记与新实体。
- **复核补口（2026-11-22）**：`leave_world_impl` 的 `access.remove` 之前也补记离场标记——今天
  `Command::Leave` 的调用点只在测试/bench（`World::leave`），不是当下分歧，但它是同类入口；
  附行为用例 `departure_mark_via_command_leave_emits_hide`（不是源码文本牙）。
