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
