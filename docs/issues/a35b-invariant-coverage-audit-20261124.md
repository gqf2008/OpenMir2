# A35-b 不变量覆盖审计（2026-11-24，只读）

> 状态：只读审计（未写产品码、未改牙、未跑构建/测试；判定依据 = 逐条阅读现有牙的**断言体**与实现代码，
> 不采信注释自述）。对象 = master `217f72cdf` 已合的 A35-b（`c41a5ec5c` feat + `b80aff591` merge）；
> 设计基准 = `E:/UserTemp/a35-zone-session-population-design.md` §2/§2.1/§4/§5。
> 代码锚全部用**符号名**（高 churn 文件禁行号锚）。

## 0. 结论摘要（先看这个）

1. **实现顺序与设计顺序相反（最重要）**：设计 §2 写的是「**目标区先插**（幂等覆盖）→ 确认 → 源区删 →
   `router.assign`」；实现是「**源区先 detach**（`detach_session_for_migration`，源区锁内）→ 目标区
   `attach_session`（目标区单锁）→ 失败则 `restore_session` 回源」。**设计伪代码里的「目标先插」在代码里
   不存在**。当前能接受的实际理由是「两端都在内存、进程 panic 即整机 abort，不存在跨进程持久化窗口」，
   但这必须由 Leader 显式裁定（改设计文字记「源先摘 + 回滚」为准，或改代码为目标先插）；**审计按"实现口径"
   判定覆盖，同时把该偏差列为开 `MIR2_ZONE_SLICING` 前必须先裁决的项**。
2. **`router.assign` 失败回滚分支无行为牙**（只有代码分支 + 计数，没有任何用例把 assign 弄失败）——
   **missing**。
3. 「交易/寄售取消帧**先于**搬迁」只有**帧数/表清空**牙（T3/T3b），**没有帧序牙**——**partial**。
4. `displays`：设计说"目标区**重建**展示项"，实现是**fail-loud 不重建**（`attach_session` 里
   `displays.get(id).is_none()` ⇒ 计数 `zone_handoff_target_display_missing` + WRN）；**该偏离没有 T- 登记**，
   且「缺失路径」无正例牙（现有牙只断言 `target_display_missing == 0`）——**partial + 未登记**。
5. 负向「不搬」表（`monster_walk_seen`/`line_notice_states`/`gates`/`handoff_runtime`/`mon_gen_host`）
   **没有任何牙**证明"误搬会被抓"——**missing（低风险）**。
6. 锁毒化用 `PoisonError::into_inner` 收口、但"加锁失败/超时"（设计 §4 提到的 fail-closed 面）
   **无牙**，实现也**没有超时**（阻塞锁）——**partial**。
7. 其余核心面（exactly-one、七表搬/回滚、A6 按 chr 过滤、Q1 World 会话、A38 组关系、A40 交易条目、
   驱动窗口、四条计数的 JSON 序列化）均有行为牙且断言够硬（见 §2 表）。

## 1. 六个重点核（按 opc-27 清单）

### ① `router.assign` 失败回滚：只有计数，**没有行为牙** → `missing`
- 实现：`deliver_session_migrations` 里 `router.assign(line,socket,target)` 返回 `Err` ⇒ 用目标区单锁
  `take_session_for_migration` 撤掉刚插的会话 ⇒ `restore` 回源 ⇒ `note_zone_handoff_session_failed` +
  `mirror_metrics::note_handoff_refused` + WRN。
- 现有牙：`deliver_session_migrations_assigns_router_and_counts_moved` **只跑成功路径**
  （`router.lookup(0,901)==zone(0)`）；`cross_zone_migration_refused_rolls_back_whole_session` 的两种失败
  是「目标区同 chr 冲突」与「lookup 返回 None」，**都不经过 assign**。全仓无测试构造
  `SocketRejected`（`socket <= 0`）。
- 判定依据：没有任何用例进入 `assign` 的 `else` 分支；把该分支删成「直接计失败、不撤目标区」现有牙全绿。

### ② 「取消帧先于搬迁」：**partial（有数目牙，无帧序/相对序牙）**
- 现有：`cross_zone_trade_cancel_emits_exactly_one_dealcancel`（恰 1 条 `SM_DEALCANCEL`、交易表清空）、
  `cross_zone_selloff_cancel_emits_one_selloffcancel`（恰 1 条 `SM_SELLOFFCANCEL`、book 条目取消、
  区 k 空态不产帧）；接线路径 `tick_phase_a_detaches_committed_session_into_pending` 断言
  `zone_handoff_canceled_trades==1` + 恰 1 条 `SM_DEALCANCEL` + `ground_sessions` 当场摘空。
- 缺口：上述用例**无法区分**「先发帧再 detach」与「先 detach 再发帧」——帧都进了同一 `mpsc`，断言只看
  计数与最终状态。设计 §2.1 的"帧先发，不因跨区推迟/丢"目前靠**代码顺序**保证，无牙。

### ③ `displays` 目标区"重建"：**partial + 偏离未登记**
- 设计 §2：`displays` = "**新建**（目标区注册该玩家展示项）"。
- 实现：`attach_session` **不重建**；`displays.get(m.id).is_none()` 时记
  `zone_handoff_target_display_missing` + WRN，注释写明"fail-loud，不重建"。依赖目标区镜像快照
  （halo `apply_mirrors`）已写入展示项。
- 牙：T1 只在**已预置展示项**的夹具上断言 `zone_handoff_target_display_missing == 0`；**没有**"展示项
  缺失 ⇒ 计数 +1 / 出帧缺项"的正例牙。
- 登记：`whitelist.md` 无对应 T- 行（grep `target_display_missing|不重建` 无果）⇒ 这是**未登记的
  设计偏离**（若 Leader 裁定维持 fail-loud，应补一行 T- 登记 + 一条计数正例牙）。

### ④ `pending_local` 按 chr 过滤：**covered**
- `cross_zone_session_migration_keeps_exactly_one_session` 显式插两条在飞命令（`other` 与 `mover`），
  断言：源区 `pending_local.len()==1`（**别人的留下**）、目标区 `len()==1` 且
  `all(|p| chr_name == mover)`（**只带走本 chr**）。变异：过滤条件改成「全带走/全留下」⇒ 两条断言红。

### ⑤ `login_tails` / `aoi_frames_total` / `shop_sessions`：**covered（逐表，不是粗判）**
- T1 对**每张表**分别断言：源区 `login_tails.is_empty()`、`aoi_frames_total.is_empty()`、
  `shop_sessions.is_empty()`；目标区 `contains_key(chr)`、`aoi_frames_total.get(chr)==Some(&7)`、
  `contains_key(chr)`。
- 回滚牙 `cross_zone_migration_refused_rolls_back_whole_session` 的 `check_source_intact` 再对三表
  **逐项**断言恢复（`login_tails.contains_key`、`aoi==Some(&7)`、`shop.contains_key`）。
- 结论：不是「七表和恒 1」的粗判——粗判只是额外的一层，逐表断言才是主判据。

### ⑥ 四个 `zone_handoff_*` 计数进 perf 的接线：**partial**
- 事实修正：这四列**不在 TSV 平铺列**里，而在 **`zone_metrics.handoff` JSON**（A36 契约：
  `canceled_trades → session_failed → session_moved → target_display_missing`）。TSV 中 `handoff_*`
  平铺列只有 A35-a 之前的 offered/accepted/refused/promoted/inserted_fresh 等。
- 接线牙：`zone_metrics_column_is_valid_compact_json_with_matching_columns` 手动 `note_*` 四个计数后断言
  JSON 四键各 `+1`（**序列化映射**被牙钉住）；`.handoff` 段本身无独立 JSON schema 牙。
- 路径→计数：`session_moved`/`session_failed` 由成功/回滚牙断言；`canceled_trades` 由接线牙断言；
  **`target_display_missing` 只有 `==0` 断言，无 `+1` 路径牙**（见 ③）。
- 结论：序列化 covered；路径覆盖 partial（缺 ③ 的正例）。

## 2. 设计 §2/§2.1/§4/§5 逐条对照表

判定口径：`covered` = 有行为牙且断言能区分"实现 vs 变异"；`partial` = 有牙但抓不到某关键侧；`missing` = 无牙。

| # | 设计条目（§2 表 / 不变量） | 现有牙（牙名）与实际断言 | 判定 |
| --- | --- | --- | --- |
| 1 | `ground_sessions` 搬 | T1：源区空、目标区有；回滚牙：源区恢复 | covered |
| 2 | `online` 搬 | T1：`src.online.len()+dst.online.len()==1`、`find_by_chr` 指向正确 | covered |
| 3 | `saves` 搬 | T1 + 回滚牙 `check_source_intact` | covered |
| 4 | `pending_local` 按 chr | T1：别人的留源区、只带本 chr（见 ④） | covered |
| 5 | `login_tails` 搬 | T1 两侧 + 回滚牙（见 ⑤） | covered |
| 6 | `aoi_frames_total` 搬 | T1 两侧值 7 + 回滚牙 | covered |
| 7 | `shop_sessions` 搬 | T1 两侧 + 回滚牙 | covered |
| 8 | `trade_sessions` 先取消不搬 | T3 恰 1 帧 + 表清空；接线牙计数 1 | **partial**（无帧序牙，②） |
| 9 | `sell_off_sessions` 先取消不搬 | T3b 恰 1 帧 + book 清 + 区 k 无帧 | **partial**（同上） |
| 10 | `monster_walk_seen` / `line_notice_states` / `gates` / `handoff_runtime` / `mon_gen_host` **不搬** | **无任何牙**；T1 夹具甚至未 seed 这些表 | **missing（低风险）** |
| 11 | `displays` 目标区**新建** | T1 仅 `missing==0`（预置展示）；实现 fail-loud 不重建 | **partial + 偏离未登记**（③） |
| 12 | World 实体（`HandoffOffer`/`promote_mirror_to_real`） | A35-a 既有牙（`handoff_tick_top_returns_committed_ids_from_accepting_zone` 等）+ T1 `world.session` stage；实体搬迁本身非本片 | covered（本片范围外） |
| 13 | `router.assign` 搬后必调（成功） | `deliver_session_migrations_assigns_router_and_counts_moved`：`lookup==target`、moved+1、refused 0 | covered |
| 14 | `router.assign` **失败** ⇒ 撤目标项 + 回滚 | 无 | **missing**（①） |
| 15 | 不变量：同 chr 恰在一个区 | T1：两区 ground/online 计数和恒 1；回滚牙目标区不被改动 | covered |
| 16 | 不变量：**禁同时持两把区锁**（A9） | 仅驱动窗口静态牙 `session_migration_drivers_sit_between_phase_a_and_barrier1`（锁外窗口）；锁本身无双持牙 | **partial**（结构靠 code review） |
| 17 | 顺序：目标先插 → 确认 → 源删 → assign | **2026-11-24 裁决：实现口径保留（源先摘 → attach → assign；失败整包回滚），设计 §2「目标先插」不适用**——理由：双存在（两区各一份）比短暂零份更危险，两端内存态、panic 即 abort。登记 **T-299**；验收面 = `cross_zone_session_migration_keeps_exactly_one_session` + `a35b_router_assign_failure_rolls_back_whole_session` | **已裁决（T-299）** |
| 18 | 回滚顺序：router 留源 → 删目标 → 源保留 | 回滚牙：`router.len()==0`、目标区原会话不被改动、源区七表逐项恢复 | covered（除 router 失败支，①） |
| 19 | §4 fail-closed：目标不可用/poison ⇒ 不搬不 assign + 计数 | 回滚牙覆盖"lookup None"与"同 chr 冲突"；**poison/超时无牙**，实现无锁超时（阻塞锁） | **partial** |
| 20 | §5 搬迁面读数（源无/目标有/router 指目标/refused==0/两区和 1） | 同上 T1 + 成功牙 | covered |
| 21 | §5 登录面读数（`zone_login_landed` 区 k>0 等） | 属 A35-a/登录片；`zone_metrics` 序列化牙覆盖映射，**无生产值牙** | 不在本片（登记） |
| 22 | A32 `save_outbox` 在飞请求 | 本片未涉及；A32 已合（`HostLoop.zone_save_outbox` 由区 0 `drain_zone_saves` 合并） | 本片范围外（A32 covered） |

## 3. 每个 partial/missing 的牙设计（只设计，不实现）

**D1（对应 ①，优先级最高）`router.assign` 失败整车回滚**
- 造法：把源区 `online` 条目改成 `socket = 0`（`seed_full_session` 后直接取 `online` 改 socket；或手工
  `set` 一条 `socket=0` 的 entry）⇒ `ZoneRouter::assign` 必返 `SocketRejected`。
- 断言：`zone_handoff_session_failed +1`、`handoff_refused +1`、**源区七表逐项恢复**（复用回滚牙的
  `check_source_intact`）、**目标区无该 chr**（`take_session_for_migration` 已撤）、`router.len()==0`。
- 变异：删 `router` 失败分支的 `take_session_for_migration`（目标区留半份）⇒ "目标区无该 chr" 红；
  删 `restore` ⇒ "源区恢复" 红。

**D2（对应 ②）取消帧先于搬迁**
- 造法：`tick_phase_a` 接线路径（同 `tick_phase_a_detaches_committed_session_into_pending` 夹具）下，
  在 `gates` 的 `mpsc` 之外加一个**序号探针**：把 `detach_session_for_migration` 之后 `ground_sessions`
  的摘空时刻与收到 `SM_DEALCANCEL` 的时刻记录到同一 `Vec<&'static str>`（测试内以 `Rc<RefCell>` 注入
  或改测试专用 hook，不改产品语义）。
- 断言：序 = `["cancel_frame", "detach"]`。
- 变异：把取消调用挪到 detach 之后 ⇒ 序反 ⇒ 红。（若不愿加 hook，可退化为：取消帧在
  `pending_session_migrations.push` 之前已进入 `g_rx`——用 `g_rx.try_iter()` 在 detach 前后各读一次。）

**D3（对应 ③）展示项缺失的 fail-loud 正例**
- 造法：T1 夹具**不要**预置目标区 `displays.set(id, …)`。
- 断言：`zone_handoff_target_display_missing +1`、`attach_session` 仍成功（会话七表在）、无 panic。
- 变异：把 `note_zone_handoff_target_display_missing()` 删掉 ⇒ 计数断言红。
- 另：若 Leader 裁定"应重建"⇒ 改为断言 `displays.get(id).is_some()`；并把现实现的 fail-loud 记 T- 行。

**D4（对应 ⑩"不搬"表）负向夹具**
- 造法：T1 前向源区 `monster_walk_seen/line_notice_states` 各插一条（gates/handoff_runtime 已有）。
- 断言：迁移后源区这两张表**原样仍在**、目标区**没有**这些键。
- 变异：把这两张表误加进 `detach` 的取走列表 ⇒ 源区空/目标区有 ⇒ 红。

**D5（对应 ⑯ A9）无双持锁**
- 造法：静态源码扫描太弱（易绕）；建议**行为哨兵**：测试里给两区 `ZoneState` 各加一把测试专用
  `Mutex` 哨兵（`try_lock` 模拟第二把区锁），或直接在 `deliver_session_migrations` 关键路径插入
  `debug_assert!(!another_zone_lock_held())` 的测试钩子。
- 断言：一次 `deliver` 期间第二把锁 `try_lock` 始终成功（即未被持有）。
- 变异：在目标锁内再 `restore` 源区（需源锁）⇒ 第二把锁忙 ⇒ 红。
- （若认为成本 > 收益，可接受：由驱动窗口静态牙 + code review 兜底，并在 T- 行登记。）

**D6（对应 ⑲ poison/超时）**
- poison：测试里把目标区 `Mutex` 人为 poison（持锁时 `panic!` 并 `catch_unwind`），断言 `into_inner`
  路径仍能完成搬迁/回滚且计数正确。
- 超时：**当前实现无锁超时**（阻塞 `lock()`）；若 Leader 要求 fail-closed 超时，必须先加超时机制
  （产品改动），本审计不设计产品码；否则在 T- 登记"无超时，阻塞等待"。
- 变异：把 `unwrap_or_else(PoisonError::into_inner)` 改成 `expect` ⇒ poison 用例 panic ⇒ 红。

## 4. 开 `MIR2_ZONE_SLICING` 前的结论

**必须先补（阻断项）**
1. **顺序偏差裁决（§0-1）**：设计"目标先插"与实现"源先摘 + 回滚"不一致。虽然当前窗口只在内存、panic
   即 abort，但这是**契约级偏差**，不裁决就不能开闸（要么改设计文字，要么改实现）。
2. **D1 `router.assign` 失败回滚牙**：这是唯一"失败链最后一步"的路径，现有牙全绿也不能证明它不半搬。
3. **D3 展示项 fail-loud 的登记/牙**：`displays` 不重建是**设计偏离**，至少要有 T- 登记 + 正例计数牙。

**可带已知窗口上线（不阻断，但要登记）**
4. D2 取消帧序牙（靠代码顺序 + 现有帧数牙；上线前用一次真机窗口确认 `SM_DEALCANCEL` 在出现帧之前即可）。
5. D4 "不搬"表负向牙（误搬概率低，且症状显性：怪物走位/公告状态跨区串）。
6. D5/D6 锁双持与 poison/超时（D5 有驱动窗口静态牙兜底；D6 现状无超时——若真机上出现"锁等待"需另开卡）。

**范围外（本审计不覆盖，登记）**
- §5 登录面读数（`zone_login_landed` 区 k > 0 等）属 A35-a 登录片；
- `A32 save_outbox` 在飞请求的搬迁行为（A32 已合、独立牙覆盖周期/关服存档）。

## 5. 审计方法与可复现

- 只读：阅读 `bins/game-svr/src/lib.rs` 的 `detach_session_for_migration`/`cancel_cross_zone_trade_and_selloff`/
  `deliver_session_migrations`/`HostLoop::attach_session|restore_session|take_session_for_migration`/
  `adopt_world_session` 与 A35-b 牙块；阅读 `handoff.rs` 的 `CommittedHandoff`/`HandoffAck`；
  `zone_router::SocketRejected`；`perf_counters` 的 handoff 四计数与 `zone_metrics_json` 的 `handoff` 段。
- 未改任何文件（除本审计文档）、未跑构建/测试。

## 6. 裁决落实（2026-11-24 补牙后）

- **裁决一（顺序偏差）**：保留实现、改文字 + 登记 **T-299**（见 §2 第 17 行与 whitelist）。理由=双存在窗口比短暂零份更危险。
- **D1 已补**：`a35b_router_assign_failure_rolls_back_whole_session`——`socket<=0` 造 `SocketRejected` 进 `deliver_session_migrations` 的 `else` 支；断言目标区该 chr 已撤、源区七表逐项恢复、`zone_handoff_session_failed`/`handoff_refused` 各 +1、`zone_router_rejected_socket` +1、router 不登记。变异：else 支改「只计数不撤」⇒ 红。
- **D2 已补**：`a35b_cancel_frames_precede_session_detach`——用 `cfg(test)` 测试缝 `a35b_order` 在 `tick_phase_a` 的两个真实调用点记录 `"cancel"`/`"detach"` 相对序，断言 `["cancel","detach"]` 且 tick 返回时 `SM_DEALCANCEL` 恰一条已在 `gates`。变异：取消调用挪到 detach 之后 ⇒ 序反 ⇒ 红。
- **D3 已裁决并补**：目标区展示项由镜像路径保证（`region_loop::apply_mirrors` 同步 displays；`promote_mirror_to_real` 不写）⇒ `attach_session` 的 `is_none()` 定性为**防御性检查**：缺失 ⇒ `zone_handoff_target_display_missing` +1 + WRN + **拒收整包回滚**（已实现；检查前移到 `adopt_world_session` 之前，Err 路径零副作用）。登记 **T-300**；牙 `a35b_missing_target_display_rejects_and_rolls_back`；变异：改回「计数后继续搬」⇒ 红。
- **仍带窗口**：D4（"不搬"表负向牙）、D5（A9 双持静态牙）、D6（无锁超时——**另立卡**，不在本卡修）。
