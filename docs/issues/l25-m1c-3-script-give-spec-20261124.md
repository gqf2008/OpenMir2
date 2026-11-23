# M1c-3 前置分析与规格草案：脚本造件族（give / 药物合成 / 开盒）跨线程方案

> **状态**：只读分析 + 规格草案（2026-11-24，`card/m1c-3-spec`）；**不写码**。
> **上游**：`docs/issues/l25-m1c-2-plan-20261123.md` §2-4（脚本造件族留 M1c-3；前置 = 脚本宿主跨线程）
> 与 §12.1 覆盖面；`docs/issues/l25-m1c-2c-eat-branch-spec-20261124.md`（`EatProcessEnv` 先例）。
> **锚点口径**：只用符号锚，不写行号（`bins/game-svr/**` 是高 churn 目录）。

## 0. 一句话结论

三族的可搬性**不一样**，建议拆两片：

- **M1c-3a（药合合成本地，低风险）**：`CM_USERMAKEDRUGITEM` 的造件体
  `game_svr::shop::make_drug_item` **本身不碰任何进程态**——输入全部区本地（本区会话 `sg.state`、
  本区世界商人货架 `World::merchant_goods_groups`、`ShopSession`、`EnterConfig` 区副本、
  共享 `item_number`）⇒ 走 **2b 同款"纯接线"**（`zone_cmd_class` 加 `Local` + `handle_zone_command_on`
  窄臂），**不需要**新邮箱；顺带成为 A25 的**第二条区 k 造件路径**（解包之外）。
- **M1c-3b（脚本 give / NPC 开盒，沿用 2c 形态）**：`CM_CLICKNPC`/`CM_MERCHANTDLGSELECT` 的脚本链
  依赖 `NpcHost`/`NpcVars`/`MerchantScriptBook`/`ScriptGmEnv`（均 `!Send` 或借进程态）⇒
  **沿用 2c 的 `Process` 回区 0 + `XxxProcessEnv` 形态**，新增一个 `ControlRequest` 变体
  （建议名 `ScriptProcess`）与区 0 执行支；**不建议**把脚本宿主搬成跨线程（见 §2(b)）。
- **开盒脚本（`StdMode 31 / AniCount != 0` 的 `@StdModeFunc`）已经在 2c**：走 `EatProcess`
  的脚本支（`EatProcessEnv.npc_host/npc_vars/gm`），**无需新机制**；只需在 M1c-3 报告里引用既有牙。

## 1. 三族入口与"是否碰进程态"（符号锚）

| 族 | 入口（触发） | handler | 一层 callee（造件点） | 进程态依赖 |
| --- | --- | --- | --- | --- |
| **GM give**（`@s`/`@give`/脚本 `GMEXECUTE` 造物） | `CM_SAY` 的 `@` 支 → A6 邮箱 `ControlRequest::GmSay` → drain `handle_say_event` | `handle_gm_command`（`gmcmd` 纯决策）→ `collect_gm_use_item_pending` / `apply_gm_use_item_write` | `BagItem::new` + `next_item_number` + `trade::send_add_item_body` | **已是 `Process`**：`ScriptGmEnv`（`GMEXECUTE` 跳线）、`gm_filter_word`/`gm_mission`（`Cell`/`RefCell`）、`pwd_sessions`；**无 NpcVars** |
| **脚本 give**（NPC/商人脚本里的 `GIVE`/`TAKEGOLD` 等） | `CM_CLICKNPC(1010)` → `handle_click_npc_cmd`；`CM_MERCHANTDLGSELECT(1011)` → `handle_shop_cmd_inner` | 二者都走 `run_merchant_goto`（NPC/商人脚本宿主） | 脚本引擎 `CodeDef{name:"GIVE"}` 动作 → `next_item_number` + `add_item_to_bag*` | **`MerchantScriptBook`（内部 `Rc<NpcDef>`）、`NpcVars`（`Rc<RefCell>`，**!Send**）、`ScriptGmEnv`、`SellOffBook`、`ScriptWorldCtx`（`MonGenHost`/`MapRegistry`/`MonsterTable`/`DisplayTable`，后四者区本地）** |
| **药物合成** | `CM_USERMAKEDRUGITEM(1034)` → `handle_shop_cmd_inner` 的对应臂 | `handle_shop_cmd_inner`（同一大 match） | `game_svr::shop::make_drug_item` | **无**：`sg.state`（区会话）、`world.merchant_goods_groups`（区世界）、`shop_sessions`（区）、`shop_context`（`EnterConfig` 区副本）、`make_items: Option<&MakeItemRegistry>`（`EnterConfig` 区副本）、共享 `item_number` |
| **开盒脚本** | `CM_EAT` 且模板 `StdMode 31 / AniCount != 0` | `handle_eat_cmd` → `run_std_mode_func_script`（`@StdModeFunc<N>`） | 脚本引擎 `GIVE` 等同 | **`NpcHost`（`defs: Vec<Rc<NpcDef>>`）、`NpcVars`（!Send）、`ScriptGmEnv`**——**2c 已收进 `EatProcessEnv`**（`Process` 回区 0） |

**判据（读码所得，落地前需 D-1 级复核）**：

- `make_drug_item(state, goods, s_item_name, catalog, make_items, item_number, ctx)` 的实参里
  **没有任何 `NpcHost`/`NpcVars`/`ScriptGmEnv`**；`goods` 取 `World::merchant_goods_groups(merchant_id)`
  （区 k 商人由区 k 静态装载 `spawn_merchants_with_scripts` 建在本区世界），`shop_sessions`/`sell_off_sessions`
  是 `HostLoop`（区本地）字段。
- `make_items` 是 `EnterConfig` 的区副本：`sim_enter_cfg` 字面量含 `make_items`，各区 k 拿 `sim_enter_cfg.clone()`
  ⇒ 区 k **已有**该表（`sim_loop` 里的 `enter_cfg.make_items.take()` 只动它自己那份）。
  **建议**（M1c-3a 的 P1）：像 `unbind_list` 一样提升为 **`Arc<MakeItemRegistry>` 进 `SharedServices`**
  （只读、单份装载），避免 N 份拷贝与"哪份过期"的问题。
- `CM_CLICKNPC`/`CM_MERCHANTDLGSELECT`/`CM_USERMAKEDRUGITEM` 当前都落 `zone_cmd_class` 的
  **表外 `Unsupported`**（`zone_k_unwired_families_still_count_unsupported` 已列前两个）。
- `CM_EAT` 的脚本支先例：分类 `Process` → `ControlRequest::EatProcess` → drain 用**区 k** 玩家态 +
  `EatProcessEnv`（进程态：`npc_host`/`npc_vars`/`gm`/`lottery`/`castle_war_states`）执行
  `handle_eat_cmd`；区 k 本地臂 `proc=None` 只放行 `Dead/Unpack/OtherLocal`。

## 2. 跨线程方案二选一

### (a) 沿用 2c：`Process` 回区 0 + `XxxProcessEnv`（**建议**）

- 形态：`ControlRequest` 增 **`ScriptProcess { zone, chr_name, ident, recog, param, tag, series,
  tail, at_tick, epoch }`**（与 `GmSay`/`EatProcess` 同一 drain、同一 `CONTROL_MAILBOX_CAP`、
  同一 `control_pending`/`first_tick`/超时/关服清理；**不开第二机制**）。
- 区 k：`zone_cmd_class` 把 `CM_CLICKNPC`/`CM_MERCHANTDLGSELECT` 归 **`Process`**；
  `handle_zone_command_on` 不加执行臂（执意本地 = 桩）。
- 区 0 drain 执行支：`ScriptProcessEnv<'a>` 承载区 k **拿不到**的项——
  `npc_host: Option<&NpcHost>`、`npc_vars: &NpcVars`、`merchant_book: &MerchantScriptBook`、
  `sell_off_book: &mut SellOffBook`、`gm: Option<&ScriptGmEnv>`、`script_world: Option<&mut ScriptWorldCtx>`
  ——玩家态取 **区 k** 的 `ZoneState.host`（`world`/`sessions`/`shop_sessions`/`sell_off_sessions`/
  `mon_gen_host`/`displays`），与 2c 的 `EatProcess` drain 同构。
- **为什么建议 (a)**：
  1. **没有新的并发语义**：所有 `!Send` 留在区 0；区 k 只投递 12B 字段（`ident/recog/param/tag/series` + `tail`）。
  2. **与 oracle 执行序等价**：同一玩家的脚本串行执行，跨区只多 ≤1 tick 延迟——`@` 与 2c 脚本食
     已按此口径接受；`CLICKNPC` 另有 `ClickNpcTime` 限频门，频率有界。
  3. **复用既有计数与降级面**：邮箱满/区状态不在/超时全部沿用（含 4③ 的 `zonek_control_zone_gone`）。
  4. **不引入锁**：脚本里的 `$GLOBAL`/`$HUMAN` 动态变量、`GMEXECUTE` 跳线、行会/存档视图全部仍在
     区 0 单线程访问（oracle 同形）。

### (b) 脚本宿主可跨线程（**不建议**）

- **根因（`!Send` 清单）**：
  - `NpcVars.global_dyn_vars` / `session_dyn_vars`：`Rc<RefCell<…>>`（`DynVarStore`）；`NpcVars::clone()`
    共享同一张表 ⇒ 跨线程必须 `Arc<Mutex/RwLock>`。
  - `NpcHost.defs: Vec<Rc<NpcDef>>`、`MerchantScriptBook` 的 `MerchantNpc.def: Rc<NpcDef>`
    ⇒ `Arc<NpcDef>`（AST 只读，这部分改造成本低）。
  - `ScriptGmEnv` 借进程表：`guilds: &GuildManager`、`pwd_sessions: &HashMap`、
    `host: ScriptGmHost`（`Cell<bool>`/`RefCell<GmMissionState>`/`Cell<i64>`）。
  - 脚本执行要 `&mut world`/`&mut SessionGround`（区本地）与 `NpcVars`（进程全局）**同时**可变：
    搬成跨线程 = 进程全局表被多区线程并发读写 ⇒ 必须加锁，且 `RefCell` 的借用语义（脚本内可重入
    读写）要重设计。
- **成本/风险**：动 `npc.rs`/`script` 引擎/`ScriptGmEnv` 与所有脚本调用点；每个 `$GLOBAL` 读都要过锁
  （脚本热路径）；语义从"单线程可重入"变"多线程加锁"，**与 oracle 不可等价**（计划 §2 已判）。
  收益仅是"少一次邮箱往返"，而 (a) 的延迟已在本项目 `@`/脚本食口径内被接受 ⇒ **不划算**。

**结论：M1c-3b 用 (a)**；M1c-3a 连邮箱都不需要（纯接线）。

## 3. `CM_EAT` 先例：可复用 / 不可复用

**可复用（直接照抄形态）**
- `ControlRequest` 枚举 + `a6_enqueue_control` 同一临界区（判满→push→置位）与 `A6ControlGuard` 清位。
- "分类只决定 Process/Local；执行支拿 `Option<XxxProcessEnv>`，`None` 先判后消费"的拆法。
- `Process` 入邮箱即计 `Drained`、执行体不本地调用；本地命令暂缓/溢出/超时全沿用。
- 牙的形态：投 `Xxx` 变体 + `control_pending==1` + 执行体未被调用 + drain 后清位。

**不可复用（差异点）**
- `EatProcessEnv` 是**物品分支级**（模板决定 Local/Process）；脚本命令是 **ident 级整族 `Process`**，
  不需要 `eat_branch` 式运行时再分。
- 执行体不同：脚本要 `handle_click_npc_cmd`/`handle_shop_cmd_inner` 的**整套参数**（含区本地的
  `shop_sessions`/`sell_off_sessions`/`script_world` 的 `mon_gen_host`/`displays`），不是单个
  `handle_eat_cmd`。
- 请求体要带**完整 12B 字段**（`ident/recog/param/tag/series` + `tail`），`EatProcess` 只有
  `recog/param/tag/tail`（ident 恒 1006）⇒ 变体字段不能照抄。
- `CM_MERCHANTDLGSELECT` 有 **type=1 派发门**（`merchant_dlgselect_dispatch`）——分类与执行两侧都要
  保持"只有 type=1 才走 Shop 派发"的口径，别把其它 type 塞进邮箱。
- `CM_CLICKNPC` 的 `IsCanDeal/Death/Ghost/ClickNpcTime` 四道门在 zone 0 执行支里读**区 k 世界/会话**
  即可（都是区本地量），但 `ClickNpcTime` 是会话态 ⇒ 仍在区 k 会话上（drain 持区 k 锁）。

## 4. 牙与判据草案

### M1c-3a（药物合成本地）

- **正例 `zonek_make_drug_item_creates_locally`**：区 k 玩家站在**区 k 的** `is_make_drug` 商人 ±15 格内，
  背包备齐配方件，发 `CM_USERMAKEDRUGITEM`（`tail` = 成品名）⇒
  `handle_zone_command_on` 返回 `Dispatched`；**本地**出 `SM_ADDITEM(200)`（Recog=ActorId）；
  背包 +1 件且 `make_index == 计数器+1`；共享 `item_number` +1；**不进 A6 邮箱**、`control_pending==0`。
  变异：删本地臂 ⇒ `Unsupported`、零帧 ⇒ 红。
- **负例 `zonek_make_drug_requires_merchant`**：无制药商人 / 异图 / 超视距 ⇒ `Dispatched` 但**零帧、背包不变**
  （handler 的 `mflags.is_make_drug && same_map && within_sight` 门），且仍**不投邮箱**。
- **静态面**：本地臂的实参里**不得**出现 `npc_vars`/`npc_host`/`merchant_book`/`gm`/`script_world`
  （源码牙：函数体内 `proc_svc!`/这些符号 0 命中）。

### M1c-3b（脚本 give / NPC 开盒，`Process` 回区 0）

- **负例 `script_click_routes_to_zone0`**：区 k 玩家 `CM_CLICKNPC` ⇒ 分类 `Process`；
  `a6_dispatch_local` 投 **`ControlRequest::ScriptProcess`**（`ident=1010`、`recog=npc_actor` 等字段逐个相等）；
  执行体**未被调用**、零帧、`control_pending==1`。
  变异：把分类改 `Local`/漏投邮箱 ⇒ 红。
- **正例 `script_click_executes_against_zonek_state_in_drain`**：drain 执行后 ⇒ 商人/NPC 的
  `@main` 里 `GIVE 木剑 1` 生效：`SM_ADDITEM(200)` 发到**区 k 的** socket、背包 +1、`SM_MERCHANTSAY` 链出现；
  `item_number` +1；守卫清位后 `control_pending==0`。
  **口径记录**：脚本在区 0 线程执行 ⇒ 若开 `MIR2_ITEMNUM_TRACE`，这些行标 **zone 0**（**不得**当 A25 跨区证据）。
- **同族负例 `merchant_dlgselect_type_gate_kept`**：非 type=1 的 `CM_MERCHANTDLGSELECT` ⇒ 仍按
  `merchant_dlgselect_dispatch` 归非 Shop（或 `Unsupported`），**不得**被塞进 `ScriptProcess`。
- **开盒复用证据**：引用 2c 既有牙 `zonek_eat_script_not_executed_locally` /
  `eat_process_enqueues_typed_request_not_gm_line`（脚本食 = `@StdModeFunc` 开盒），M1c-3 不重复造。

## 5. 先决与风险

**已共享（2c/4③ 已解决，不用再做）**：`unbind_list`（`Arc` 单实例、`ptr_eq` 断言）、
`item_number`（进程唯一 `Arc<AtomicI32>`）、`catalog`/`colors`/`registry`/`enter_cfg`（区副本）。

**M1c-3 仍未覆盖的进程态清单**（区 k 不可持有）：
- `NpcHost`（`Rc<NpcDef>`）、`NpcVars`（`Rc<RefCell<DynVarStore>>`）——脚本宿主根；
- `MerchantScriptBook`（`Rc<NpcDef>` 的商人脚本书）、`SellOffBook`（元宝寄售全局表）；
- `ScriptGmEnv` 与其借用的 `gm_filter_word`/`gm_mission`/`gm_spirit_mutiny_tick`（`Cell`/`RefCell`）、
  `guilds`/`guild_store`、`pwd_sessions`；
- `win_lottery`/`castle_war_states`（2c 已进 `EatProcessEnv`，脚本族若也要用就复用同字段）。

**先决（建议写进卡面）**：
- P1：`MakeItemRegistry` 提升为 `Arc` 进 `SharedServices`（单份装载、区 k 只读借用；与 `unbind_list` 同款
  启动断言 `ptr_eq`）。
- P2：`ScriptProcessEnv` 的字段面固定为 §2(a) 的六项；执行支**不得**跨区持第二把区锁（A9 边界沿用）。
- P3：邮箱与暂缓上限**沿用**（`CONTROL_MAILBOX_CAP`/`A6_DEFER_CAP`/`A6_TIMEOUT_TICKS`），不加新列；
  满/超时/区不在复用既有 `zonek_control_*`（含 4③ 的 `zonek_control_zone_gone`）。

**风险**：
- **R1 邮箱容量**：`CM_CLICKNPC` 比 `@` 频繁（但受会话 `ClickNpcTime` 限频）。多玩家同时点 NPC 可能挤占
  256 邮箱 ⇒ 沿用 `zonek_control_mailbox_rejected` 可见；若窗口实测拒收 >0，再评估是否给脚本请求独立配额
  （**本片不开第二机制**，先观测）。
- **R2 段序**：`Process` 在飞期间该玩家本地命令暂缓 ≤1 tick（与 `@` 同）——oracle 聊天/脚本不阻塞移动的
  边界维持不变（A6 既有契约）。
- **R3 trace 区号**：脚本造件在区 0 线程 ⇒ trace zone 0；**A25 的跨区证据仍由 C1' 解包与 3a 药合并承担**。
- **R4 `CM_MERCHANTDLGSELECT` type 门**：分类/执行两侧都要保留 type=1 口径，别把 dialog 里"打开商店/修理"
  等非脚本路径一并搬（它们属商店族，另判）。
- **R5 script_world 副作用**：`CLEARMAPMON`/`MONGENEX` 在 drain 里作用于**区 k 的** `mon_gen_host`/`displays`
  （`ZoneState.host` 字段），不是区 0 的；写错区 = 清错图。
- **R6 与 M1c-2c 的分界**：`@StdModeFunc` 开盒已在 2c；M1c-3 不重接，只引用既有牙，避免双重派发。

## 6. 交付切片建议（给 Leader 排卡）

1. **M1c-3a**（药合并本地）：`zone_cmd_class` 加 `CM_USERMAKEDRUGITEM`（`Local`）+ `handle_zone_command_on`
   窄臂（抽 `make_drug_zone_local(...)` 复用 `shop::make_drug_item` 与 `emit_shop_frames_into`）+ P1 共享
   `MakeItemRegistry` + 牙 3 条（正/负/静态）+ 变异（删臂红）。
2. **M1c-3b**（脚本回区 0）：`ControlRequest::ScriptProcess` + `ScriptProcessEnv` + drain 执行支 +
   `zone_cmd_class` 两 ident 归 `Process` + type 门保持 + 牙 3 条（投递/执行/type 门）+ 变异
   （分类改 Local/漏投邮箱红）。
3. **收尾**：两片各自 N=1 帧字节门 + 报告明写"区 0 `INBOUND_PROCESS` 臂保留不动 / 脚本执行仍在区 0 线程 /
   trace zone 口径"。

## 7. 待裁问题（给 Leader）

1. **3a 是否要等 P1（`MakeItemRegistry` 提升 `Arc`）**？不提升也能跑（区副本已存在），但会多 N 份拷贝、
   且"哪份是真源"不显式；建议先提升再做 3a。
2. **3b 的 `ScriptProcess` 是否要覆盖 `CM_CLICKNPC` 之外的其他脚本入口**（如任务脚本 `CM_QUEST*`、
   隐藏 `QManage` 触发）？本草案只列 click/dlgselect；其余入口建议先清点、再决定是否同片。
3. **`CM_MERCHANTDLGSELECT` 的其它 type** 归商店族（M1c-4？）还是与 3b 同片，需 Leader 定边界。
