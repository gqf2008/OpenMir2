# B4 结构性批次：per-actor 队列 + per-viewer 帧延迟（施工图，2026-11-22）

> 状态：**设计蓝图（只读分析产物，未改产品代码）**。作者：openmir2-1e。承接 B17 装置对照
> （`docs/issues/b17-victim-frame-order-diagnosis.md`）与本卡 `openmir2-b4-multivictim-strike-order.md`。
> 实验装置改动在 `card/b4-device-drain`（`bcc232572`，**不单独合并**）。

## 0. 一句话

step 模型缺了**一整层 per-actor 队列语义**：oracle 每个 actor 一条 `MsgQueue`，`Run()` 各自消耗；
`Operate` 期间入队给别人的消息，对方要**下一 tick 的 `Run`** 才看到。Rust 现在是「每 tick 把全部 actor 的
`SelfDelayOutcome` 批量处理、帧同步广播给所有 viewer、同一刻 flush」——没有「谁先跑 / 跑之前看不见什么」。

## 1. 两层（缺一不可）

- **(i) 泵的 per-actor 回合序**：`deliver_self_delay_msgs_impl` 一次返回全部 actor 的扁平 outcome，丢 per-actor 处理序。
- **(ii) 帧广播的 per-viewer 延迟**：`SpellFrameBuf::flush` 把本 tick 收集的所有帧同一刻发给所有收件人；
  没有「该 viewer 的 Run 是否已跑过」。**本卡现有措辞只覆盖 (i) 的 strike 队列；657 案例证明 (ii) 也必须改。**

## 2. oracle 锚（符号，不用 path:line）

`ActorEntity.MsgQueue`（per-actor `PriorityQueue<SendMessage,byte>`）、`BaseObject.Message.GetMessage`
（未到期队首 `Enqueue(Lowest)`+`return false`）、`PlayObject.Message.Run`（每 tick 逐条 `GetMessage`）、
`StatusChanged`→`SendRefMsg`→逐可见者 `SendMsg` 入各自 `MsgQueue`。

## 3. 影响面（装置 OLD vs NEW，同生成器、只差 DrainOnce；50 case / 153 case-label）

- Sent 变化 **60 case-label**：DUE-SHIFT 14 / ORDER 55 / FIELD-COUNT 62（按 case,who 事件）。
- `DrainPasses` 变化 **44 case-label**；**无 HitCap 翻转**。
- **不是多受害者专属**：同一「2 元素等优先级 churn（延迟消息 + 640 同在一条队列）」在**单 victim** stem 也成立——
  fireball5 / lightning11 / lightning45 / fireball-nonzero / firecharm / hbfireball 的 edge/hit：延迟受击 600→700 且 656/31 换序；
  fireboom / fireboom47 / lightflower(-nonzero) / snowwind / grouplightening / mabmabe / skill10 / fire 同类；groupdeding 不变。
- 原始件：`E:/UserTemp/b17x/cmp/{old,new}`；表：`E:/UserTemp/b17x/impact_summary.txt`。

## 4. 施工图（7–9 编辑点）

| # | 函数（符号锚） | 现在做什么 | 要改成什么 | 层 | 影响 stem | 能否独立验证 |
|---|---|---|---|---|---|---|
| 1 | `deliver_self_delay_msgs_impl`（world-bevy `slave`） | 一次泵全部 actor，返回扁平 `Vec<SelfDelayOutcome>`，per-actor 回合序丢失 | 返回 per-actor 有序结果（`Vec<(actor_id, Vec<Outcome>)>` 或带 turn 索引），宿主据此判「消息入给已跑过/未跑过 actor」 | (i) | 所有 self-delay stem | 能：world-bevy 单测（actor 序 + per-actor 结果） |
| 2 | `SelfDelayQueue` / `SelfDelayOutcome`（同上） | 每 actor 一条四叉堆；outcome 无跨 actor 广播意图 | 评估是否携带「跨 actor 广播意图」，或仍由宿主层负责（倾向宿主层，保持 world 层纯粹） | (i)/(ii) 边界 | 同上 | 能：单测 |
| 3 | `drain_b3_self_deliveries`（bins/game-svr） | 遍历全部 outcome；transparent→`push_charstatuschanged` 同步广播 657；`buf.flush` 当 tick | 657 入 per-viewer 延迟队列，viewer 下一回合才 flush | (ii) | bigcloak(657)、18、28、16、49-54(640) | 能：golden 657 due/recog 对上 + buffer 单测 |
| 4 | `drain_b4_with_enter_cfg` → `drain_b4_self_deliveries`（bins/game-svr） | 遍历全部 outcome；毒→657/656/100、strike→31/53/656 同步广播；单次 flush | 广播帧走 per-viewer 延迟队列 | (ii) | 6/48 施毒族、9/10 穿透、fireball 族 | 能：逐 stem 台账 |
| 5 | `drain_b5_self_deliveries`（bins/game-svr） | `drain_mag_struck_b2`→31/53/656，`struck.flush` 同 tick | 同 (ii) per-viewer 延迟 | (ii) | 9/10（RM_MAGSTRUCK 600ms） | 能 |
| 6 | `push_charstatuschanged` / `push_name_color(_self)` / `push_open_health`（bins/game-svr） | 经 `ref_msg_cache::targets` + `deliver_broadcast_targets` 把帧推给全部收件人 socket（当前 buf） | 按「产生者 turn vs 收件人 turn」入延迟队列 | (ii) | 见 3/4/5 | 能：buffer 单测 |
| 7 | `SpellFrameBuf`{`push_socket`,`deliver_broadcast_targets`,`flush`}（bins/game-svr） | 收集本 tick `(socket,prio,body)`；flush 同刻发每个 socket | **核心 seam**：加 per-recipient 跨-tick pending + 当前 turn 上下文；flush 只发「收件人 turn 未过」的帧，其余留到下一 tick。**保留既有 `spell_frame_buf_tests` 的 per-recipient 四叉堆序** | (ii) | 全部出帧 | 能：跨两 tick 的 buffer 单测 |
| 8 | 生产 `sim_loop` 的 B2/B3/B4/B5 排水块（bins/game-svr） | 每 tick 各排一次，帧同刻 flush | 串 per-actor 回合序 + 跨 tick pending；B2..B5 间序也须服从 turn | (i)+(ii) 胶水 | 全部 in-world | 部分：真机窗口 / parity |
| 9 | 对拍 `fireball_parity_drive` 交付循环（bins/game-svr） | 每 100ms：`deliver_self_delay_msgs`→`drain_b4`→`drain_b3`→`rig.drain()` | 镜像新 per-actor turn 模型；不改 oracle-golden 契约 | (i)+(ii) | 全部 magic parity stem | 能：`big_cloak_parity_*` 等 |

## 5. 落地编排（必须同批）

- committed golden 里 **47 支已比 `Sent.due_ms_rel`**（fireball5 等 edge=600），bigcloak/groupdeding 在
  「无 due」的 13 支里 ⇒ **装置重出 + 产品 per-actor 改 + 比较口径复核** 必须同一批；否则 47 支必红。
- 装置实验件：`card/b4-device-drain` `bcc232572`（不单独合并）。
- 比较器的 `multi_victim_same_tick` 例外与 poison 派生帧口径，需按新语义复核（**不是**本卡装置改动）。

## 6. 两个坑（必须记住，否则会被误判为改坏）

1. **`DrainPasses` 语义变了**：新装置里 = 单 tick 单 actor 最多连续处理条数；安全上界**不得复用旧多趟上限 8**
   （正常突发可达 >8，会被误判 `DrainPassesHitCap`）⇒ 改 256；并夹 `>=1`（`golden_drain_not_capped` 断言 `>=1`，
   否则 1→0 打破）。
2. **覆盖面**：temp diff 只跑 50 个 `--case`；`warrskill`（走 `ClientSpellXYEntry.Run` 独立路径）**rc=2 未跑**，不在本表内。

## 7. oracle 语义再确认

`GetMessage` 遇未到期队首 `Enqueue(Lowest)`+`return false` 是 per-actor、每 tick 一次；等优先级多条逐 tick swap 换根
（`swap_remove` 末→根 + `enqueue` 等停）。Rust `SelfDelayQueue` 已逐字复刻该 churn。

## 8. §(ii) 接缝确认（2026-11-22 夜，基于 master `52c351128`，(i) 层已合入）

**前提变化**：(i) 层让 `deliver_self_delay_msgs_impl` 返回 `Vec<SelfDelayTurn>{actor_id, outcomes}`
（组间 = 首次入队序、组内 = 投递序），扁平 `deliver_self_delay_msgs()` 由 `flatten_self_delay_turns` 保序还原。
**但宿主现存调用点仍吃扁平版**（`TickRecord.slave_self_deliveries: Vec<SelfDelayOutcome>`，见 `world` 的
`slave_self_deliveries` 字段与 `sim_loop`/`fireball_parity_drive` 的 `let outs = deliver_self_delay_msgs()`）
⇒ (ii) 第一步要让宿主改吃 `deliver_self_delay_msgs_turns()`。

### Q1 「当前 turn actor」上下文从哪来？

**推荐 ②：`SpellFrameBuf` 存「当前 turn」上下文，由宿主在 turn 边界切换**——加
`begin_turn(actor_id)`，内部 `current_turn: Option<{actor_id, turn_index}>`，
`turn_index` = 本 tick `Vec<SelfDelayTurn>` 的下标快照（`HashMap<actor_id, usize>`），默认 `None`。
- 判据：帧要发给收件人 R 时，若 `turn_index[R] <= current_turn.turn_index` ⇒ R 本 tick **已 Run 过** ⇒
  入 R 的**延迟队列**（下一 tick 的 turn 再出）；否则进本 tick 缓冲（R 还没跑）。
- 为何不选 ①（每次显式传 `&TurnCtx`）：要把该参数串进 `push_socket` / `deliver_broadcast_targets` /
  `push_broadcast_cached(_at)` / `push_actor` / `push_charstatuschanged` / `push_name_color(_self)` /
  `push_open_health` + `drain_b3/b4/b5` 十来个签名与调用点。
- **对 `spell_frame_buf_tests` 破坏最小的是 ②**：三个既有测试都不调用 `begin_turn` ⇒ `current_turn=None`
  ⇒ 与今天逐字节相同（**0 改**）；①则每个签名都要改。
- **坑**：默认 `None` = 不延迟 ⇒ 忘记 set 会静默回退旧行为。必须配**元断言/牙**：宿主本 tick 处理了 ≥1 个
  turn 却没 `begin_turn`（或全程延迟队列恒空）⇒ 红。

### Q2 既有 per-recipient 四叉堆序 / 延迟插入后会不会变 / 同 tick 立即+延迟歧义

- 现状（符号锚）：`SpellFrameBuf::flush` 对**每个收件人**起一个**新的** `NetPriorityQueue`
  （=`mir2_world_bevy::msgqueue`，.NET 8 四叉最小堆），键 = 下行优先级 `u8`（`(byte)ident`）；
  **按 priority 出队、同 priority 按堆数组布局**（不稳定；3 条 `[0,2,1]`、4 条 `[0,3,2,1]`）。
  收件人分组序 = 首次入队序；跨收件人不混堆（`deliver_broadcast_targets` 逐收件人 `push_socket`）。
- **延迟插入后序会不会变**：只要延迟帧**并入同一 per-recipient 堆**（不是另起排序再拼接），
  同 priority 的堆布局语义不变。
- **同 tick「立即 + 延迟」歧义（真存在）**：收件人 R 本 tick 可能同时有 (a) 本 tick 产生、R 还没跑的立即帧，
  (b) 上一 tick 产生、R 本 turn 才到点的延迟帧。**建议改用「per-recipient 持久 `NetPriorityQueue`（跨 tick
  存活）」**：延迟帧 = 上一 tick 已入 R 队列、本 tick 仍未出的项；新帧本 tick append——与 oracle
  `ActorEntity.MsgQueue` 的堆数组布局同构，出队仍只按 priority + 堆布局。**不要**加「先延迟后立即」或按
  tick/插入序的显式 tie-break（会偏离 oracle 的不稳定堆）。⇒ 忠实做法是把 `SpellFrameBuf` 从「每调用一个
  `Vec`」升级为「每收件人一个跨 tick 持久堆」；pilot 若不持久化，退而求其次要把延迟帧**先**入本 tick 堆
  （近似，但同 priority 交错时不保证与 oracle 同序）。

### Q3 `spell_frame_buf_tests` 现状 / 哪些必改 / 哪些必须不变

- 3 条测试（模块 `spell_frame_buf_tests`）：
  1. `same_priority_frames_follow_dotnet_heap_order`：同 priority（167）3/4 条 → `[0,2,1]`/`[0,3,2,1]`（牙=稳定排序会红）。
  2. `recipients_get_independent_heaps`：A/B 不混堆（A=`[0,2]`、B=`[1,3]`）。
  3. `caster_queue_without_rm_spell_keeps_push_order`：混合优先级堆布局 + 「caster 不入 RM_SPELL」语义。
- **必须保持不变（=「没改坏」的证据）**：以上 3 条 + `msgqueue::tests::ties_dequeue_in_four_ary_heap_order`
  + `msgqueue::tests::protocol_constants_match_world_literals`。它们只经 `push_socket`/`flush`、不设 turn 上下文
  ⇒ **默认 no-deferral 路径下应逐字节不变**。
- **要改的**：**0 条**（若保留 stateless 模式）。做法：把延迟做成 `SpellFrameBuf` 的**新状态模式**
  （`current_turn` + 持久 per-recipient 堆），`SpellFrameBuf::new()` 仍是今天行为；**新增** 3 条测试：
  ① R 已跑过 ⇒ 帧落下一 tick；② R 未跑 ⇒ 同 tick 出；③ 延迟项与同 priority 新项在同一持久堆的 tie 序 = 四叉堆布局。

### Q4 最小可验证切法：只让 657 走 per-viewer 延迟

**能，且是理想 pilot。** `push_charstatuschanged` 是 657 唯一发射点（经 `ref_msg_cache::targets` +
`deliver_broadcast_targets`）。pilot：
1. 宿主 `drain_b3_self_deliveries` 吃 `Vec<SelfDelayTurn>`（先切成 `_turns`），逐 turn `begin_turn(actor)`；
2. **仅** `push_charstatuschanged` 走「R 已跑 ⇒ 落 R 的跨 tick 待发；R 未跑 ⇒ 立即」；其余广播
   （656/31/53/100/640/1100…）**原样不动**；
3. 下一 tick `drain_b3` 开头先 flush 各 R 的待发 657。

**pilot 验收（bigcloak s1，不动其他广播）**：caster 的 657 应从今天 Rust 的 `(657,1002,800)` 变成
`(657,1002,900)`（caster 在 turn 序里先于 target，target 的 recog1002 到 caster 时 caster 本 tick 已跑 ⇒ 落下一 tick）；
target/obs_in 仍 `{1002@800,1001@900}`；caster 的 `(657,1001,900)` 不变（它是 caster 自己 turn 内产生）。
**这与装置的 per-viewer 结果（caster `{1002@900,1001@900}`）一致** —— pilot 目标是让 **Rust 对齐装置/契约**，
不是让装置迁就 Rust。

### 落地顺序（按 opc-19 预置）

1. (i) 宿主改吃 `_turns`（纯搬，零翻转：`flatten_self_delay_turns` 已保证与今天扁平同序）；
2. pilot：657-only 延迟 + 3 条新单测（旧 3 条不变）；
3. 扩到 656/31/53/100（`push_name_color`/`push_open_health`/b4/b5）；
4. 胶水 + 装置 + golden 同批 + 全量门逐支点名。

## §12 施工偏离（2026-11-21，pilot 实施中）

> 由 `89` 实施 657 pilot 时发现，经 opc-19 批准。**偏离不改变判据**，只改容器落点。

### 12.1 偏离点：容器从 `SpellFrameBuf` 改为 **rig 级 `pending` 穿透**

§3 表第 3 行原写「657 入 per-viewer 延迟队列，viewer 下一回合才 flush」，隐含以 `SpellFrameBuf`
为延迟容器。**实测该切面承载不了判据维度**：`SpellFrameBuf` **每次调用都 `new()` + `flush(self)` 消耗**
（全仓 44 个 `new` / 52 个 `flush`），buf 是**短命对象**，延迟帧无法在 buf 内跨 tick 存活 ⇒
「只动 `SpellFrameBuf` + `push_charstatuschanged`」**不足以产生 800→900**。

**批准的替换切面**：
- 新增 `drain_b3_self_deliveries_with_pending(rec, …, pending, preload)`：按
  `rec.slave_self_delivery_turns` 逐 turn `begin_turn`、`flush_with_deferred` 回写 `pending`；
- `pending` 只挂在 `SpellFrameRig` / `B3Rig` 各**一个字段**；
- 旧 `drain_b3_self_deliveries` 与**其余调用点一字不动**（新增函数是并列入口，不是改旧函数）。

即：延迟容器从「短命 buf」上移到「跨 tick 存活的 rig 字段」，由新入口显式穿透，旧路径零改。

### 12.2 偏离不改变判据

验收仍为（bigcloak s1）：
- caster `(657,1002,**900**)`（延迟项，落下一 tick）；
- caster `(657,1001,900)` **不变**（self turn 内产生）；
- target / obs_in `{1002@800, 1001@900}`。

### 12.3 钉死延迟谓词 = **严格 `<`**

「收件人 turn **严格早于**当前 turn」才延迟；`R == current` 是 **self**，**不延迟**。
- 依据：本蓝图 Q4 pilot 验收段（128–131 行：caster 的 `1001` 是「自己 turn 内产生」⇒ 同轮出）；
  以及 oracle `while (GetMessage)` 同轮处理 self（`DrainPasses=3`）。
- **若将来有人想把 `<` 改成 `<=`，必须附六元组证据**：
  `(who, param/tag, 产生者 turn, 收件人 turn, 实得 due, 期望 due)`——否则会静默把 self 帧也推到下一 tick
  （caster 的 `657/1001` 会从 900 漂到 1000，与装置/契约不符）。
