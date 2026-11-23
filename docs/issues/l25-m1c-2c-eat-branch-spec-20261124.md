# M1c-2c 可执行规格：`CM_EAT` 分支门 + 类型化控制邮箱（2026-11-24，只读规格）

> 基线 `9a3c432c2`（2b 合并后）。**只定实施规格、不写码**。前置：2a（统一计数器）与 2b
> （ground/bag/querybag/storage 接线）均已合并。
> 上游裁决：`docs/issues/l25-m1c-2-plan-20261123.md` §7 的 2c 六条 + §12.1 族表；
> `docs/issues/l25-m1b-plan-20261123.md` 的 A20（`!` 本片不接、`ControlRequest` 只加 `EatProcess`）。
>
> **锚点口径**：本文**不写行号**——正文全部用符号锚（函数/枚举/常量名）。`lib.rs` 仍在被
> 多车道改动，行号在合并瞬间即漂（2b §8.5 已因此删除）；需要行号时按目标 head 现算。

## 0. 范围与一句话

- **范围内**：`CM_EAT(1006)` 的区 k 分支门（脚本 / 彩票 / 城堡三类**不本地执行**、解包类本地执行）
  + `ControlRequest` 枚举化（只新增 `EatProcess`）+ `EatProcess` 参与 A6 stop-and-wait
  + mailbox 满 / 区状态不在的计数与 WRN + 守 `!` 仍 `Unsupported`（A20）。
- **一句话**：分类器对 `CM_EAT` 走 **`Process` 兜底**；真正的"哪支本地执行"在**执行侧**由纯函数
  `eat_branch` 决定——`Unpack`/`OtherLocal`/`Dead` 覆写为 `Local` 就地执行，其余（`StdModeFunScript`/
  `Lottery`/`MoveFamily`）保持 `Process`，经**类型化控制邮箱**的 `EatProcess` 交区 0 执行。
- **不在范围**：脚本宿主跨线程化；彩票/城堡的本地执行；`!` 邮箱变体（A20 作废）；2c 的 2K 证据
  场景（`C1'`）本规格只给判据口径，场景装置另卡。

## 1. `CM_EAT` 现有分支枚举（`handle_eat_cmd`，符号锚）

`handle_eat_cmd` 是本片唯一入口；按执行序，分支与**进程态依赖**：

| # | 分支（判据） | 实际载体 | 进程态依赖 | 2c 归属 |
|---|---|---|---|---|
| 0 | `dead`（`world.entities.get(actor_id).death`） | `SM_EAT_FAIL(636)` | 无 | **Local** |
| 1 | `StdMode == 31 && AniCount != 0` **且** `npc_host.has_function_npc()` | `run_std_mode_func_script`（`@StdModeFunc<N>`） | **`NpcHost` / `NpcVars`（`Rc<RefCell>`，!Send）/ `ScriptGmEnv`** | **Process**（`StdModeFunScript`） |
| 2 | `StdMode == 4`（书） | `readbook::maybe_read_book` | 无（world/sg/online/gates/catalog/colors/strings） | **Local** |
| 3 | `EatUseItems` move 组（回城卷/随机传送/地牢逃脱/**行会回城**） | `eat_move::maybe_eat_move` | **`castle_war_states`**（IT-13 `InGuildWarArea`；进程级 `HashMap`） | **Process**（`MoveFamily`） |
| 4 | `StdMode == 31 && AniCount == 0`（解包） | `game_svr::useitem::use_item` 的 `31 if item_list.len() + 6 - 1 <= MAX_BAG_ITEM` 臂 ⇒ `CopyToUserItemFromName` ×6 | **无**：`unbind_list`（只读表）、`catalog`、`item_number`（2a 已共享 `Arc<AtomicI32>`） | **Local**（`Unpack`） |
| 5 | `StdMode == 3 && Shape == 11`（彩票） | `use_item` 吃 + 宿主 `win_lottery`（`WinLotteryShared = Arc<Mutex<…>>`）+ `lottery_cfg` | **`win_lottery` 进程级计数 + 抽 `world.rng`** | **Process**（`Lottery`） |
| 6 | 其余消耗（`StdMode 3 / Shape 13` 经验、`Shape 12` ExtraAbil、祝福油 `Shape 4`、修复油、禁药门等） | `use_item` 各臂 + 宿主本地回写 | 无（world/sessions/strings/colors/catalog/registry/`item_number`） | **Local**（`OtherLocal`） |

**一层 callee 与签名（摘录）**：

- `run_std_mode_func_script(ani_count, chr_name, socket, gate_idx, host: &NpcHost, world: &mut World,
  sg: &mut SessionGround, map_text: &str, catalog, npc_vars: &NpcVars, item_number: &AtomicI32,
  gates, online, …)` —— 进程态四项（host/npc_vars/gm/全服广播面）都在签名里。
- `eat_move::maybe_eat_move(world, sg, actor_id, make_index, chr_name, socket, online, gates, catalog,
  registry, dead, no_drug, castle_war_states, colors) -> bool` —— `castle_war_states` 只用于 IT-13
  行会回城支（`InGuildWarArea`）。
- `readbook::maybe_read_book(world, sg, actor_id, make_index, socket, online, gates, catalog, colors,
  strings, dead) -> bool` —— 全 zone-local。
- `game_svr::useitem::use_item(ctx, state, item_list, item_number) -> UseItemOutcome` —— 纯接缝；
  解包支的 `make_index` 走 `next_item_number(item_number)`（2a 的共享原子），**无进程态**。
- `handle_eat_cmd` 自身的进程态形参：`npc_host: Option<&NpcHost>`、`npc_vars: &NpcVars`、
  `gm: Option<&ScriptGmEnv<'_>>`、`lottery: &WinLotteryShared`、`lottery_cfg`、`castle_war_states`。
  **⇒ 区 k 不能直接调用现签名**（`NpcVars` 是 `Rc<RefCell>`、!Send；`ScriptGmEnv` 借进程表）。

**解包支为什么可以 Local**：它只读 `catalog`/`unbind_list`，唯一写者是 2a 的共享计数器
（进程级 `Arc<AtomicI32>`，区 k 已持有），产物是 `UseItemOutcome.added_items`（宿主按**本区**会话
写回）；全程不碰 `NpcHost`/`NpcVars`/`ScriptGmEnv`/`win_lottery`/`castle_war_states`。

## 2. 分类器形态

**不能**沿用"ident 级 + payload 例外"：`CM_EAT` 的类别取决于**物品模板**（`StdMode/AniCount/Shape`），
而 `zone_cmd_class(ident, payload)` 是纯 `(ident, payload)` 函数、拿不到模板。

**定案（两段式）**：

1. **分类器（纯函数）**：`CM_EAT` 归 **`Process`**（兜底；与表外 `Unsupported` 不同——它属 M1c
   命令集）。同步改 `zone_cmd_class` 的 doc 与 `zone_cmd_class_m1b_families_are_local` 的清单
   （`CM_EAT` 断言为 `Process`，**不**进 `Local` 清单）。
2. **执行侧再分（`eat_branch`，纯函数）**：

   ```text
   enum EatBranch { Dead, Unpack, OtherLocal, StdModeFunScript, Lottery, MoveFamily, Unknown }
   fn eat_branch(tpl: Option<&StdItem>, dead: bool) -> EatBranch
   ```

   - 判据只读**模板 + 本区会话**（`dead`、模板查表），**默认 `Process`**（`Unknown` 也落 `Process`，
     fail-closed；**无 `_ => Local` 兜底**）；
   - **安全默认（写死）**：`tpl == None`（模板缺/物品不在包内）、`sessions.get(chr) == None`、
     `dead` 不可判（实体不在世界）⇒ **一律 `Process`**（回区 0）；**禁止**任何"判不了就本地"的默认。
     区 0 执行时按它自己的模板/会话查表得到同一失败帧（如 `SM_EAT_FAIL(636)`），行为等价、只是跨区；
   - `MoveFamily` 的判据用**单源谓词** `eat_move::is_move_family_shape(std: &StdItem) -> bool`
     （可从 `maybe_eat_move` 的 shape 判据反向抽出，禁止两处各写一份——参照 `movewire::is_move_command`）；
   - `StdMode 31 && AniCount != 0` **一律 `StdModeFunScript`**（不查 `FunctionNPC`：那是进程态；
     区 0 执行时若 `FunctionNPC == null`，`use_item` 自然回 636，与"本地也不知道"等价）；
### 2.1 唯一决策点（符号 + 调用链 + 源码断言）

**唯一决策点 = `classify_local_event(ev: &GateEvent, sessions, catalog) -> ZoneCmdClass`**
（新增、`fn` 级、非 `#[cfg(test)]`）。**只有它**决定一条命令在区 k LOCAL 段算什么类：

```text
tick_phase_a（区 k）
  └─ execute_local_budget(..., classify = |ev| classify_local_event(ev, &sessions, &env.enter_cfg.catalog))
        └─ classify_local_event
              ├─ None / 非 Command            ⇒ zone_cmd_class 不可达 ⇒ Unsupported（保持第 3 步口径）
              ├─ ident != CM_EAT              ⇒ zone_cmd_class(ident, tail)         // 既有单源，不变
              └─ ident == CM_EAT              ⇒ eat_branch(tpl, dead)
                    { Dead, Unpack, OtherLocal }        ⇒ Local
                    { StdModeFunScript, Lottery,
                      MoveFamily, Unknown }             ⇒ Process
```

- `execute_local_budget` 不再内联 `zone_cmd_class(...)`：它接收一个 `classify: &dyn Fn(&GateEvent) ->
  ZoneCmdClass` 参数，**该参数只有一个生产构造点**（`tick_phase_a` 的 LOCAL 支）；
- `eat_branch` 的**唯一生产调用者**是 `classify_local_event`；
- **源码断言（与"区 0 不调用 classify"同型）**：production 段里 `eat_branch(` 的出现处只有
  `classify_local_event` 体内一处； 区 0 的 `INBOUND_PROCESS` 臂与 `handle_say_event`/drain 的
  `GmSay` 路径**零引用**（grep 断言 + 旁证：`classify_local_event` 的生产调用点在 LOCAL 支内）。

**为什么不会有两个决策点**：`a6_dispatch_local` 收到的 `class` 已是 `classify_local_event` 的返回值，
它只做"Process 入邮箱 / Local 执行/暂缓/溢出"的**机械分派**，不再看 `ident`、不查模板（单点决策、
多点机械处置）。

**为什么区 0 路径不受影响**：`zone_cmd_class` 的唯一生产调用点是 `execute_local_budget`（区 k LOCAL
段）；区 0 的 `CM_EAT` 仍走 `INBOUND_PROCESS` 的**原臂**（一行未动）。分类器改动只改"区 k 把这条
命令算哪类"，不改区 0 的派发结构。

## 3. `ControlRequest` 枚举化（只加 `EatProcess`）

现状：`ControlRequest { zone, chr_name, line, at_tick, epoch }`（GM `@` 专用）。

**改法**（守卫/计数/邮箱结构**不动**）：

```text
enum ControlRequest {
    GmSay { zone, chr_name, line, at_tick, epoch },     // 字段逐个搬入，语义零变化
    EatProcess { zone, chr_name, recog: i32, param: u16, tag: u16,
                 tail: Option<String>, at_tick: u64, epoch: u64 },
}
```

- **GM `@` 零变化核对点**：① `a6_enqueue_control` 的临界区（判满 → `push_back` → 置
  `control_pending`/`control_first_tick`）只把构造从 struct 改成变体；② drain 的
  `A6ControlGuard` + `handle_say_event` 路径按变体匹配（`GmSay` 分支**逐字**保留）；③ 计数列
  `zonek_control_mailbox_rejected`/`wait_timeout`/`shutdown_discarded`/`defer_overflow` 全部沿用；
  ④ `a6_control_missing_warn` 的语义/限速不变。
- **只新增 `EatProcess`**；**不实现 `!` 变体**（A20 作废 A-1b：`!` 仍 `Unsupported`，不进邮箱）。
- **A6 九条负例全绿**：`a6_no_inflight_executes_immediately` / `a6_timeout_releases` /
  `a6_clear_resumes_immediately` / `a6_mailbox_full_rejects_without_marking` /
  `a6_overflow_executes_in_place_in_arrival_order` / `a6_two_control_commands_hold_until_both_done` /
  `a6_timeout_then_stale_clear_is_ignored` / `a6_overflow_batch_occupies_budget_as_a_whole` /
  `a6_overflow_batch_postponed_when_budget_short`（+ 源码接线提示
  `a6_wiring_hints_production_calls_present`）。**受影响**：凡构造 `ControlRequest` 字面量的用例
  （`a6_mailbox_full_rejects_without_marking` 等）与接线提示牙的模式匹配，必须改成变体形式——
  这是**编译驱动**的改动，不删断言。

**drain 的新分支**：与 `GmSay` 同形——取空邮箱 → 区号升序 → `zone_state` 锁内 → `A6ControlGuard`
→ `EatProcess` 调**区 0 的 eat 执行体**（用**区 k** 的 `HostLoop` 世界/会话 + **进程** `ProcessServices`
的 `npc_host`/`npc_vars`/`gm`/`win_lottery`/`castle_war_states`；WRN/count 同口径）。守卫 drop 后按
epoch 减 `control_pending`（**2 拍超时语义沿用**，不改 `A6_TIMEOUT_TICKS`）。

## 4. stop-and-wait 接入点

- **置位**：`a6_enqueue_control`（在 `a6_dispatch_local` 的 `Process` 臂内被调用）——`EatProcess`
  构造时同样 `control_pending += 1`、`first_tick` 只在 `None` 时置。
- **暂缓**：同一玩家的**本地** `CM_EAT`（`Unpack`/`OtherLocal`）在有在飞时走既有 `Deferred`
  （`A6_DEFER_CAP`），放行时按 `seq` 与其它本地命令合流（`pending_local`），**不重新分类**。
- **清位**：drain 的 `A6ControlGuard::drop`（epoch 不匹配 ⇒ 不动计数 + `a6_stale_clear_warn`）。
- **超时/溢出**：完全沿用（`A6_TIMEOUT_TICKS` 2 拍；`A6_DEFER_CAP` 8；溢出批次整批执行/整批推迟）。
  **不新开机制**（A9 口径：一个邮箱、一套计数）。
- **次序契约**：「脚本食（Process）→ 解包食（Local）」在同一玩家上**不得倒序**：脚本食在飞期间解包食
  被 `Deferred`，区 0 执行完清位后才放行。

## 5. 牙清单（§7 六条 → 可执行用例）

| # | §7 条目 | 用例名（建议） | 断言 | 删什么会红 |
|---|---|---|---|---|
| 1 | 分支穷举 | `eat_branch_is_exhaustive_fail_closed` | 枚举 `StdMode31/AniCount0`、`31/AniCount≠0`、`3/Shape11`、move 族 shape、`3/Shape13`、书、表外模板 ⇒ 每个落 **命名**变体；**表外/未知 = `Process`**；源码牙：`eat_branch` 无 `_ => Local`（源码锚） | 删 `Unpack` 臂 ⇒ 解包支落 `Unknown`/Process ⇒ 红；把默认改 `Local` ⇒ 表外断言红 |
| 1b | **安全默认** | `eat_branch_safe_default_is_process` | 模板缺（`tpl=None`）/ 会话缺（`sessions.get==None`）/ 实体不在世界 ⇒ `classify_local_event` 返回 **`Process`**；端到端：投一件**模板不存在**的 `CM_EAT` ⇒ 控制邮箱 +1、**零出帧**、会话不变（区 0 侧再按自己的查表回 636） | 把任一 None 支改成 `Local` ⇒ 该牙红（本地会出 636 帧/动会话） |
| 1c | **唯一决策点** | `eat_branch_single_decision_point_static` | 源码锚：`eat_branch(` 的生产出现处**只有** `classify_local_event` 体内一处；`execute_local_budget` 的 `classify` 参数只有一个生产构造点；区 0 `INBOUND_PROCESS` 臂/`handle_say_event`/drain 的 `GmSay` 支零引用 | 在区 0 臂里加 `eat_branch` 调用 ⇒ 红；内联第二处 `zone_cmd_class` 决策 ⇒ `classify` 参数断言红 |
| 2 | `ControlRequest` 枚举化 | `control_request_gm_say_variant_keeps_fields` + 九条 A6 负例 | GM 变体字段逐个相等；九条负例全绿；源码牙：`ZoneCmdClass` 之外**无双邮箱** | 给 `GmSay` 少搬一个字段 ⇒ 编译/断言红 |
| 3 | stop-and-wait | `eat_process_participates_in_stop_and_wait` | 入队后 `control_pending==1`/`first_tick==Some(t)`；本地 CM_EAT 被 `Deferred`；drain+guard 后 `==0`；超时 2 拍路径同 A6 | 删 `EatProcess` 的置位 ⇒ `pending` 恒 0、暂缓牙红 |
| 4 | mailbox 满 / 区状态不在（**本片范围内**，非另卡） | `eat_process_mailbox_full_counts` / `eat_process_zone_state_missing_counts` | 满 ⇒ `zonek_control_mailbox_rejected +1` + 限速 WRN；**`zone_state` 缺失（现状静默 `continue`）** ⇒ 新列 **`zonek_control_zone_gone`** +1 + 限速 WRN；**派生合计列与断言同笔同步**（加列不许只加不加总） | 删新列/删 WRN ⇒ 红；保留静默 ⇒ 该牙红 |
| 5 | 分支门（区 k） | `zonek_eat_script_not_executed_locally` / `zonek_eat_lottery_not_executed_locally` / `zonek_eat_castle_not_executed_locally` / `zonek_eat_unpack_executes_locally` | 前三：区 k 返回 `Process`（邮箱 +1、**零帧**、会话不变）；解包：`Dispatched` + `SM_ADDITEM(200)` ×N + 背包净增 + 计数器消费 N | 删覆写（全 `Process`）⇒ 解包正面牙红；把脚本支标 `Local` ⇒ 脚本牙红（本地执行 = 脚本副作用/零帧断言破） |
| 6 | `!` 守恒（**本片范围内**） | `bang_say_stays_unsupported` | 区 k 投 `!x` 的 `CM_SAY` ⇒ `zonek_inbound_unsupported +1`、**不进 A6 邮箱**、**不置 `control_pending`**、不本地执行（零帧）；断言面 = 计数增量 + 邮箱长度 0 + `a6.control_pending == 0` + 零帧 | 把 `!` 归 `Process`/进邮箱/置位 ⇒ 红（A20 防倒退；**若将来实现 `!` 的邮箱变体，本牙按定义转红——这就是那一条提醒牙**） |

**两条现状缺口并入 2c 范围（不另开卡）**：
- 第 4 条的 `zone_state` 缺失路径**现在是静默 `continue`**（drain 里 `zh.zone_state(zone_k)` 为
  `None` ⇒ `continue`，不计数不告警）——2c **必须**补 `zonek_control_zone_gone` 列 + 限速 WRN，
  且派生合计列/断言同笔同步，才算 §7 达标；
- 第 6 条目前**无此牙**（B3 的 `zone_k_channel_say_is_not_executed_locally` 只覆盖"不执行"）——2c
  **必须**补 `bang_say_stays_unsupported` 的"不进邮箱/不置 `control_pending`"两面。

## 6. 先决与风险

**先决（P1–P6）**：

- **P1 `unbind_list` 共享形态定稿**：`SharedServices` 增 `unbind_list: Arc<UnbindList>`（只读、`Clone`；
  与 `registry`/`enter_cfg` 同性质）。区 k 本地解包必须拿到它；**不得**为区 k 造第二份装载。
- **P2 `handle_eat_cmd` 的进程态形参拆分**：抽 `handle_eat_cmd_local(...)`，实参只取 zone-local 的
  那一批：`world`/`sessions`/`online`/`gates`/`catalog`/`colors`/`strings`/`registry`/`unbind_list`/
  `item_number`/`repair_item_dec_dura`/`human_max_gold`/`oil_cfg`（这些都在 `EnterConfig` 区副本或
  2a 的共享原子里）；`handle_eat_cmd` 保留进程态入口，在脚本/彩票/移族分支调进程侧。**禁止**给区 k
  造 `NpcVars`/`NpcHost` 假实例。
- **P3 新计数列** `zonek_control_zone_gone`：加列 ⇒ 表头/值/派生合计/断言同步（派生列数变化要
  按指标口径重算，**不换口径**）。
- **P4 `ControlRequest` 变体化**：`a6_mailbox_full_rejects_without_marking` 等字面量 + 接线提示牙
  按变体改（编译驱动）。
- **P5 `zone_cmd_class` doc/牙**：`CM_EAT` 断言 `Process`；`zone_cmd_class_m1b_families_are_local`
  清单不加 `CM_EAT`。
- **P6 唯一决策点**：`classify_local_event` 是**唯一**判定函数（§2.1）；`execute_local_budget` 改收
  `classify: &dyn Fn(&GateEvent) -> ZoneCmdClass`（生产构造点唯一）；`eat_branch` 与
  `eat_move::is_move_family_shape` 单源；`a6_dispatch_local` **只做机械分派**，不查 ident/模板。

**风险**：

- **R1 RNG 流**：彩票支在区 0 drain 里用**区 k 世界**的 `world.rng` 抽（drain 持区 k 锁 ⇒ 无并发），
  与 oracle"在 actor 线程抽"次序等价；但与"若将来本地执行"不同 ⇒ 登记（2c 不本地执行彩票）。
- **R2 `registry` 副本**：`no_drug`（`boNODRUG`）读 `registry.flag_of(map_id)`；区 k 的
  `SharedServices.registry` 是**副本**（M1a 既有形态），`None` 时 `no_drug=false` ⇒ 与区 0 分叉
  ——登记；`env.registry` 有值时一致。
- **R3 `!` 边界（A20）**：`!` 仍 `Unsupported`；**不实现** `Say` 变体；频道消息不进邮箱。
  **若将来实现 `!` 的邮箱变体** ⇒ `bang_say_stays_unsupported` 按定义转红（该牙即"实现倒退"提醒），
  届时应连同 `zonek_inbound_unsupported` 计数口径一起重裁，不得静默改牙。
- **R4 WRN 频控**：沿用 `DepartWarnLimiter` 口径（每分钟 ≤1 条）；新列与 WRN 同时落，不许只有其一。
- **R5 A24 不受影响**：拾取写回是 ground 面，本片不碰；`…_CURRENT_BUG` 牙保持现状。
- **R6 段序不变量**：`M1B_ZONE_PROD` 只给 A 段、LOCAL 支不调 `proc_svc!`；`PROC_SVC_SEGMENTS ==
  PROCESS_ONLY`、`RAN_SEGMENTS == ALL` 两条既有牙必须仍绿。

## 7. 交付前自查（实施片）

- [ ] `eat_branch` 穷举牙（默认 `Process`、无 `_ => Local`）+ 删解包臂 ⇒ 正面牙红；
- [ ] **唯一决策点**：`classify_local_event` 单点（源码牙 1c）+ **安全默认**（模板缺/会话缺/实体缺 ⇒
      `Process`，牙 1b）；区 0 路径零引用 `eat_branch`；
- [ ] `CM_EAT` 分类 `Process`；覆写点只在区 k 执行侧；区 0 `INBOUND_PROCESS` 臂一行未动（报告签名）；
- [ ] `ControlRequest` 枚举化：GM 变体字段/行为零变化、A6 九条负例全绿；只新增 `EatProcess`；不实现 `!` 变体；
- [ ] `EatProcess` 参与 stop-and-wait（`control_pending`/`first_tick`/守卫减计数/2 拍超时/8 条暂缓）；
- [ ] mailbox 满 ⇒ `zonek_control_mailbox_rejected` + WRN；**`zone_state` 缺失 ⇒ `zonek_control_zone_gone` + WRN**
      （**本片范围内**；派生合计列与断言同笔同步）；
- [ ] 分支门四牙（脚本/彩票/城堡不本地执行、解包本地执行）+ `bang_say_stays_unsupported`
      （**本片范围内**：含"不进邮箱/不置 `control_pending`"两面；将来实现 `!` 变体则按定义转红）；
- [ ] `unbind_list` 进 `SharedServices`（`Arc`，只读）；区 k 不持有任何进程态假实例；
- [ ] 门禁：`cargo fmt --check` / `clippy -D warnings` / `cargo test --workspace` / `msgcodegen --check` /
      `docs_anchor_gate --prose`；R 级另需真机对拍（c2 场景另卡）；
- [ ] 报告措辞：**区 0 的 `INBOUND_PROCESS` 臂一行未动**；不写"与 oracle 值相等"（2a 的 T-72 边界照旧）。
