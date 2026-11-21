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
target/obs_in 仍 `{1002@800,1001@900}`；caster 的 `(657,1001,900)` 是 **Rust 值**（self 帧，oracle/装置为 `800`，见 §12.1「订正」）。
**装置新模型的真实值：caster `{1001@800, 1002@900}`**（旧装置为 `{1001@800, 1002@800}`；`1002` 的 800→900 差异与 §12.1「二次订正」同源）。
pilot 目标是让 **Rust 对齐契约**；`1001` 的「Rust 900 vs oracle 800」另立卡修，不属 pilot。

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
- caster `(657,1001,900)` 是 **Rust 值**（self 帧；oracle/装置为 `800`，见 §12.1「订正」——旧表述「不变」已作废）；
- target / obs_in `{1002@800, 1001@900}`。

### 12.3 钉死延迟谓词 = **严格 `<`**

「收件人 turn **严格早于**当前 turn」才延迟；`R == current` 是 **self**，**不延迟**。
- 依据：本蓝图 Q4 pilot 验收段（128–131 行：caster 的 `1001` 是「自己 turn 内产生」⇒ 同轮出）；
  以及 oracle `while (GetMessage)` 同轮处理 self（`DrainPasses=3`）。
- **若将来有人想把 `<` 改成 `<=`，必须附六元组证据**：
  `(who, param/tag, 产生者 turn, 收件人 turn, 实得 due, 期望 due)`——否则会静默把 self 帧也推到下一 tick
  （caster 的 `657/1001` 会从 900 漂到 1000，与装置/契约不符）。

### §12.1 pilot 验收的 oracle 依据 + 第二处施工偏离（pilot 交付，2026-11-21）

> base = `master 4bb9f256c`（pilot 合入 `b8fffa57c` + 诊断留档 `4bb9f256c`）。本节是 §12 的续（§12 记录「容器改动」；本节记录「验收依据 + 第二处偏离 + 三条验收纪律」）。

#### 12.1.a pilot 的 oracle 依据（5 条 `file:line`）

1. 657 的**入队/出帧点**：`bins/game-svr` 的 `b3_outcome_frames`（transparent 分支 → `push_charstatuschanged`；def 同名符号）。两次 657 是**同一 code 点各跑一次**（按 `rec.slave_self_deliveries` 每条 `transparent` 一次），不是两个调用点；`recog = d.target_id` = 该 8045 的受害者。
2. **turn 序** = `SelfDelayQueue::actor_ids()` 首次入队序：`crates/world-bevy` 的 `SelfDelayQueue::actor_ids()`（pump 内 `let actors = …`）；`SelfDelayTurn` 定义（同文件）。
3. **oracle 队列语义**：`src/M2Server/Actor/BaseObject.Message.cs:357` `GetMessage(msgTick)`（未到期队首 `Enqueue(Lowest)` + `return false`，`:364`）。
4. **oracle per-actor Run**：`src/M2Server/Player/PlayObject.Message.cs:360` 的 `while (… && GetMessage(GetMessageTick, …))` —— self 同轮处理、只有「早于自己跑过的别的 actor 入队的消息」要下一 tick 才见。
5. **产生者/收件人 turn 对照**（本 head 实测，非推断）：caster=0、target=1（首入队序）；obs_in 无自有自延迟、不在 turn 表（按「未跑」⇒ 不延迟）。

**期望表（4 行；`recog@due`）**：

| 接收者 | 657 期望 | 说明 |
| --- | --- | --- |
| caster | `(657,1002,900)`、`(657,1001,900)`、`(640,19,1000)` | `1002` 由 target(turn1) 产生、caster(turn0) 已跑 ⇒ 延迟到 900；`1001` 是 caster 自己 turn 内产生 ⇒ 900 |
| target | `(657,1002,800)`、`(657,1001,900)` | `1002` self turn ⇒ 800；`1001` 由 caster(早于 target) 产生、target 未跑 ⇒ 900 |
| obs_in | `(657,1002,800)`、`(657,1001,900)` | 同 target |
| **s5 实测（caster）** | `(657,2002,900)`、`(657,2001,900)`、`(640,19,1000)` | **pilot 实测**，证明「base+2 → 900」不是推断（s5 base=2000） |

#### 12.1.b 第二处施工偏离：`deliver_self_delay_msgs_impl`「空组不产出」→「**空组也产出 turn**」

- **现状（本 head）**：`crates/world-bevy` 的 `deliver_self_delay_msgs_impl` 尾 **无条件** `turns.push(SelfDelayTurn { actor_id, outcomes })`（含空 `outcomes`）；旧实现是 `if !out.is_empty()` 才 push。
- **理由**：turn 边界须 = 「本 tick 被 `Run` 过的 actor」（oracle 每个 actor 每 tick 跑一次 `Run`，即便本轮零投递也占一个 turn 下标）。否则 at-due 那一步只有 target 在 turn 表、caster 会被误判「未跑」而**不延迟**（pilot 会假通过但行为错）。
- **扁平 API 不变**：`flatten_self_delay_turns` 对空组无影响 ⇒ 扁平输出逐元素同序不变。**注意：这改的是 (i) 层已写进台账的契约，必须留痕。**

#### 12.1.c 三条验收纪律（合并门暴露，属本卡）

1. **断言里的位置量必须从夹具 base 派生**：s5/s13 红的教训是 `recog` 写死 `1002/1001`。应写 `base+1`/`base+2`（s1 base=1000、s5 base=2000、s13 base=3000）——s5 实测 `(657,2002,900)` 正是 `base+2` 的证明。
2. **交付循环退出时 `pending_657` 非空 ⇒ fail-closed panic**（已实现 `6e740976c`；`bins/game-svr` 的 `fireball_parity_drive` 交付循环末的 `assert!(pending_657.is_empty(), …)`）。属 §12 的「退化防线」：延迟 657 若在窗口末仍未排空，说明 wait_ms 不足或延迟排队丢帧，必须红而非静默丢。
3. **seam 目前只由对拍 rig 调用**（`#[allow(dead_code)]`，如 `bins/game-svr` 的 `drain_b3_self_deliveries_with_pending` 上的标注）——生产 `sim_loop` 的接线在「胶水」批，**不得被读成已上线**。

> **订正（2026-11-21 深夜）**：本小节与 §12.1.a 期望表里「caster `(657,1001,900)` 保持不变 = oracle 依据」**作废**。以 `docs/issues/bigcloak-caster-1001-due-divergence-20261121.md`（正文 `29e8312ac` + Rust 侧成因 `e6af80985`）为准：
>
> - **撤回**：`1001@900` **不是** oracle 值。**oracle / 装置（旧、新两版）都是 `800`；`900` 是 Rust 的值**——pilot 前后都是 900（旧台账 `bigcloak_s1.json` mtime `2026-11-21 13:33` 原文：`caster = [(638,1001,0),(642,500,0),(657,1002,800),(657,1001,900),(640,19,1000)]`）。⇒ `1001@900` 是 **Rust vs oracle 的既有分歧**，**另立卡修**，不在 pilot 范围。
> - **根因（防止后人改谓词）**：偏差在「**`SelfDelayOutcome.transparent` 的投递步**：Rust 900 vs oracle 800」，**与 `begin_turn` / per-viewer 延迟判据无关**——producer=收件人=caster 同下标 ⇒ 不延迟 ⇒ `due` = 该 outcome 的投递步；分歧在「该 outcome 何时到点」，**不在< / <= 谓词**。
> - **装置 blob 身份**：旧装置（`master c99f7dd47` 的 `Program.cs`）= `48966e170c37363b8acaa1df8ebcad8c63cc5d9f`；新装置 = `3f34d9c04fc6bdafe92390865ca24bf0bdbfa251`。
> - **该 due 从未跨栈比过**：golden 的 `bigcloak` **无 `due_ms_rel`** ⇒ 比较器的 `sent_fields.push("due_ms_rel")` 受 `ga.first()…get("due_ms_rel").is_some()` 守卫，这条 due **从未进比较面**，故分歧一直没暴露。
> - **保留**：`1002@900` **不变**（旧装置 800 是假绿；新模型 + oracle 都是 900）。 （此句已被 §12.1「二次订正」升级：`1002@900` 为**已证实**；`1001` 以订正为准 = oracle `800`）
> - **同源影响（一并订正 s5 实测行）**：§12.1.a 期望表 s5 行把 `(657,2001,900)` 写成「实测」**不准**——`2001` 是 self 帧，**装置新旧都给 `800`**，`900` 是 Rust 值（同一 `1001` 分歧）；s5 装置新实测应为 `[(657,2001,800),(657,2002,900)]`，「`base+2→900`」说的是 `2002`（正确）。

> **二次订正（2026-11-21 深夜，入队级 trace 实测）**：`3b` 修正一处**仪器 bug** 后，入队级 trace **实测**钉死了 `1002@900` 的 per-recipient 延迟（**时点 / 写入者 / 延迟语义三项齐全**）。上文第 215 行「`1002@900` 不变」据此**升级为「已证实」**（不再是「数值巧合」）。同时记录仪器坑与一处覆盖结论更新：
>
> - **trace 原文（s13，base 3000）**：`{"who":"caster","ident":657,"recog":3002,"enqueue_step":800,"run_step":900,"due":900}`；广播入队点 = `post_all_target`（3b 入队级 trace 的广播点）。⇒ caster 在 800 步**已 Run**，其队列在 800 步被写入 `recog=3002` 的 657，**900 步才消费**——正是 per-recipient 语义（收件人本步已跑 ⇒ 下一拍才见）。
> - **写入者已证实**：caster 在 800 步广播（`recog=caster`）写进 **target / obs_in** 队列；**caster 自己那条（`recog=target=3002`）由 target 在 800 步写入**——与 §12.2 的 byWho / producer 推断一致。
> - **仪器坑（防重踩）**：曾有一版审计因 `SendMessage` 是 `record struct`、却用**装箱对象做键**（引用比较恒 miss）⇒ `enqueue_step` 恒等于当前步 ⇒ 得出「641 行审计、`enqueue_step < run_step` 恒 0、**无跨步入队**」的**错误结论**；修正为**按值比较**后该结论被推翻。**今后入队级审计必须按值建键，不得用装箱对象做键。**
> - **R3 覆盖结论更新**：**bigcloak 自身即有 per-recipient 延迟样本**（上面 JSON 即证）；此前「60 支不可覆盖」的结论**作废**（仪器 bug 产物）。

### §12.2 producer 归属表（656 / 31 / 53 / 100）——per-viewer 延迟的 producer 口径

> 本表是后续 656/31/53/100 **扩面实现的判据**。**方法论**：oracle 里 31/53/656 是「**一个入队点 + N 个 per-receiver 出帧点**」，归属必须分「**入队 turn**」与「**sender / 出帧 actor**」两栏写，否则会把 per-receiver 出帧误算到受击者头上。

| 帧类 | Rust 产生点（`bins/game-svr/src/lib.rs`） | oracle 触发上下文 | **入队 turn** | **sender / 出帧 actor** |
| --- | --- | --- | --- | --- |
| **656**（多产生点） | 受击者名色 `drain_mag_struck_b2` 的名色支；攻击方名色 `flush_hitter_name_colors`→`push_name_color_self`；helpers `push_name_color`、`push_name_color_self` | (a) 受击者自身：`AnimalObject.cs:125`→`:330-334`→`BaseObject.cs:730-733 RefNameColor`；(b) 攻击方：`PlayObject.Message.cs:1572 SetPkFlag(hitter)`→`PlayObject.cs:4326-4338`→同一 `RefNameColor` | (a) **受击者**；(b) 也在**受击者 Run 内**（SetPkFlag 由受击者调） | (a) sender=**受击者**；(b) sender=**攻击方**（`targetObject.RefNameColor()`），但由受击者 Run 触发 |
| **31 SM_STRUCK** | `drain_mag_struck_b2` 的 `push_broadcast_cached(...,P_STRUCKMAG)` | 受击者在自己 Run 里 `BaseObject.Base.cs:629 SendRefMsg(RM_STRUCK_MAG)`（入队给可见者）；**每个接收方在自己 Run 里** `PlayObject.Message.cs:1563→1601→1614 SendSocket(self)`（发给**自己**） | **受击者** | **出帧 actor = 各接收方自己（per-receiver）**；sender(RM)=受击者 |
| **53 SM_HEALTHSPELLCHANGED** | 同函数 `push_actor(target_id,P_HEALTH)`（self）+ `push_broadcast_cached(...,P_HEALTH)`（ShowHp） | `BaseObject.cs:700-710 HealthSpellChanged()`（self `SendUpdateMsg` + `SendRefMsg` if ShowHp），在受击者 `BaseObject.Base.cs:628 RM_MAGSTRUCK` Operate 里调 | **受击者** | self-53 = 受击者自己；ShowHp-53 = 各接收方下一 Run 出帧 |
| **100 SM_SYSMESSAGE(YouPoisoned)** | `drain_b4_self_deliveries` 的 `push_actor(online, p.target_id, RM_SYSMESSAGE, body)`（**只发 self**） | `BaseObject.Base.cs:740-757 RM_POISON → MakePosion(this)`；`BaseObject.cs:1970 SysMsg(YouPoisoned)` ⇒ `SendMsg(this)`（self，非 SendRefMsg） | **中毒目标自己（RM_POISON 接收方）的 Run** | **producer = 中毒目标自己，即时 self** |

**结论（给实现）**：
- **31 / 53 / 656(a) 受击者名色**：入队确在**受击者** Run ⇒ 按「谁 Run 里入队、收件人下一 turn 才见」的 deferral 口径，用**受击者 turn** 是正确的。
- **656(b) 攻击方名色**：**sender=攻击方**，但入队/触发在**受击者 Run** —— 两栏都要写，别只写一个。
- **100 是 RM_POISON 的自投递、即时、self**，**不参与 struck turn 批次**；不许与 31/53/656 混批按「受击者 turn」延迟。
- **本表是后续 656/31/53/100 扩面实现的判据；实现与表不符时，以本表为准。**

### §12.3 tick 序契约（2026-11-21，扩面暴露的设计缺口）

> **先给第一句前提**（将来任何人重做扩面都先看这条）：**装置没有 per-recipient 队列延迟模型**（见 12.3.b）——所以装置 golden 对 31/53/100/656 的 `due` 不是权威。本节记录「tick 序」契约口径、装置模型边界、本批判定，以及 order 契约为何没落地。

#### 12.3.a tick 序契约（只写已核实）

- **已核实**：装置 `byWho` 恒为 **4 个具名 actor**：`caster → target → obs_in → obs_out`（`MagicGolden` 的 `byWho`，两处定义一致）。这是 rig 侧 `order` 的唯一确定来源。
- **怪物 / 其它 actor 的 per-tick 序位 = `OPEN：怪物 tick 序未验证`**：
  - 装置怪物是 `MonsterSpec` / `RecordingMonster`「被动、不 Operate」⇒ 不占 turn、不出帧；
  - Rust pump 的 turn 集 = `SelfDelayQueue::actor_ids()`（仅**有自延迟消息**的 actor），没有「全 actor 序」枚举；
  - 生产 `sim_loop` 走 `world.tick()`（bevy ECS），实体迭代序 = archetype/table 序，**非契约**（`AGENTS.project.md` §4.2.2 明禁把迭代序当契约）。
  - **影响面**：含 `MonsterSpec` 且「跨 actor 31/656/657 依赖怪物序」的 stem（`turnundead` / `skeleton` / `angel` / `sinsu` / `fireball-on-monster` / 以及任何带 `MonsterSpec` 的支）——这些要么不判怪物序，要么须先在 rig 里建立「全 Run 序」才能判。
- 生产 `sim_loop` 的 actor 枚举「须遵守同一契约」，但**当前未接线**（胶水批）+ **怪物序 OPEN**；**不得读成已成立**。

#### 12.3.b 装置模型边界（重做扩面时的第一句前提）

- **装置没有 per-recipient 队列延迟模型**：其 SM `due` 记的是**生产/直发时刻**（`Operate` 执行那一步），**不是**「该收件人下一 Run 才见」的延迟时刻。
- **657 能对上只是间接**：装置在 800/900 步处理 `RM_CHARSTATUSCHANGED` 的时刻恰好对齐 per-viewer 结果。
- **31 / 53 / 100 / 656 对不上**：装置 golden 对这些帧的 `due` **不是权威**。要判这些帧，需二选一：**装置补 per-recipient 模型**，或**真机 oracle 证据**。

#### 12.3.c 本批判定

- **只保留 657 pilot**（已入 master）。
- **扩面撤出**：656/31/53/100 的扩面实现与判据另立 `docs/issues/b4-expand-player-events.md`（已入 master，以该卡为准）。**本批 golden 零改动。**

#### 12.3.d 「order 契约」为何没进本批

- order（「本 tick 全部被 Run 的 actor 序」）契约**在本批未落地**——它在 `B3Rig` 单测上**非中性**：试验时 **4 条 `spell_p3b3_16_*` / `spell_p3b3_28_*`** 因**怪物 producer** 触发元断言 panic（`producer 不在 tick 序契约中：4`）。
- 这正是 12.3.a「怪物序 OPEN」在单测上的体现：order 契约一旦强制，不在 4-actor 序里的怪物 producer 就 panic。故 **order 契约未落地，不得读成已成立**；其落地依赖先解决「怪物 tick 序」（12.3.a OPEN + `b4-expand-player-events`）。
