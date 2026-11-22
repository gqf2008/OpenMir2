# L2-5 ⑤ 设计：跨区收件人帧合成（2026-11-22，设计稿）

> 状态：**设计稿，未写代码**。测量窗口期内只读代码、写文档、提交文档。
> 基线：段 3 六面计数（`cross_recipient_missing`），近边界负载实测 **10.004%**（分母口径见计划页 §12.4A）。
> 目标：镜像不再只是可见性副本——**看得见就收得到帧**；`cross_recipient_missing` 归零且残差可解释。

## 0. 事实基线（引用，不重查）

- 六面：`refmsg` / `run_frames` / `npc_merchant` / `feature_changed` / `mountride` /
  `merchant_disappear`；计数点 = 收件人 id 来自 World 扫描/缓存、本地会话查表未命中、World 里
  `mirror == true`（计划页 §11）。
- 缺口成因：`apply_mirrors` 构造 `MirrorSpec` 时**丢弃了 `HaloEntity.route`** ⇒ 镜像无 route ⇒
  收件人区无法把帧路由给跨区会话（"看得见但收不到"）。
- `halo_route`：仅 `race == Play` 且**在线登记命中**时给 `Some(HaloRoute(line,socket))`；
  非玩家/未登记 ⇒ `None`。
- `CrossZoneOutbox`（`region_loop`）是 ② 的**测试 rig**（`EmitterCriterion::RecipientZone`），
  **生产未接线**；`save_outbox` 是存档请求，另一回事。⑤ 需要的是生产形态的跨区帧邮箱。

## 1. route 从哪来、生命周期跟谁走

**决策：镜像带 route**（最小新增状态，与镜像同生命周期）；**不**在收件区自建第二张路由表
（避免双真相源与漂移）。

- `MirrorSpec` 增 `route: Option<HaloRoute>`；`apply_mirrors` 从 `HaloEntity.route` 填入。
- World 侧镜像实体保存 route（新增 `Route` 组件/字段），由 `upsert_mirror` 与
  `update_mirror_frame_fields` **每 tick 覆盖**（与外观字段同口径）。
- **清理 = 镜像本身**：陈旧镜像由 `apply_mirrors` 的 `seen` 集同 tick 清扫 ⇒ route 随镜像消失，
  不引入独立生命周期、不需要单独的过期计时器。
- **时新性不变量（写死）**：route 与镜像**共用 barrier1 后的同一份邻区本 tick 快照** ⇒
  不存在「镜像新、route 旧」或「route 新、镜像旧」的窗口；收件人要么整条镜像（含 route）在，
  要么整条不在。
- 非玩家/未登记 ⇒ `route=None` ⇒ 仍是「不可投递」，进残差桶（§5），**不是**缺陷。

## 2. 谁投递、哪个相位、队列所有权与上界

- **源区产生帧**（它拥有实体与事件来源）；**收件区是唯一发射者**（计划页 §7 口径不变）。
- 形态：源区把「帧体 + 目标收件人 route」投进**目标区邮箱**（与 `HandoffMailbox` 同形的
  `Arc<Mutex<VecDeque<CrossZoneFrame>>>`，每区一个）；目标区在 **`tick_top`（phase1 之前）**
  排水并写给自己在线会话。
  - 选 `tick_top` 的理由：①在 barrier 之前、**不持 `ZoneState` 锁**；②跨区帧先于本 tick 的
    本地 phase2 帧进入收件人队列，帧序可按 §3 全序定义；③与 handoff 的 offer/ack 排水同相位，
    一个区只在一个位置消费跨区输入。
- **有界**（无界不许）：每区邮箱上限 `MAX_CROSS_FRAMES_PER_ZONE`（建议按 §12.4A 的
  `refmsg_targets / ticks` 峰值定，初值 4096/tick）。溢出 ⇒ **拒新**（丢新来的，保留已排入的
  序） + 新计数 `cross_frame_queue_overflow` + 限流 WRN；被丢的投递同时计入
  `cross_recipient_missing`（残差可解释，不静默）。
- 所有权：邮箱 `Arc` 归 `ZoneHost`、由各区运行时绑定（同 `HandoffMailbox` 模式）；**只在
  `slicing=1 && mirror_enabled` 时构造**（N=1 零参与，§4）。

## 3. 同一收件人帧相对顺序（可检查表述）

**定义**：对收件人 R，一帧的序键 = `(tick, phase_rank, seq)`：
- `tick` = 产生该帧的源 tick（= 源区本 tick 快照 tick）；
- `phase_rank`：跨区帧 = **0**（`tick_top` 排水），本地 AOI/phase2 帧 = **1**；
- `seq` = 源区在该 `(tick, 源区)` 内的单调发送序；同一源区对同一 R 保序。

**可检查表述**：收件人 R 观察到的帧序 = 按 `(tick, phase_rank, seq)` 的全序；对可确定的用例，
R 的有序帧流必须与 N=1 基线逐字节相等（沿用 `l2_two_zone` 的"同一收件人有序序列 == 基线"口径）；
对随机事件（战斗）用"按 `(tick, seq)` 分组后，**该收件人**的组内多重集相等"。

**逐面核对产生点（必须做，不许含糊）**：若某面到 phase2 之后才发现收件人（例如 `run_frames` 的
`SendRefMsg(53)`），它的跨区帧会晚于本地帧、破坏全序 ⇒ 该面要么把产生点前移到 phase2 之前，
要么把它的跨区投递**顺延到下个 tick** 并在此登记来源面与相位。**每个面必须显式写一条**
"产生相位 → 排水相位"的对照，作为验收材料。

## 4. N=1 逐字节不变

- 准入闸：新路径只在 `slicing == 1 && mirror_enabled` 下存在；`slicing=0` ⇒ 不构造邮箱、
  `MirrorSpec.route` 恒 `None`、计数点直接 no-op（沿用段 3 的 `SLICING_ENABLED` 闸）。
- 断言位置（钉住，不能只在文档里说）：
  1. 构造入口：`cross_zone_mailbox_not_constructed_without_slicing`（与
     `walk_without_zone_bounds_never_sets_pending` 同级）；
  2. 计数：段 3 的 `slicing_off_counts_nothing` 扩展到覆盖新列（`delivered`/`overflow`）；
  3. 端到端：`n1_stdout_equivalence` 门（两棵树归一化 stdout sha 相等）+ `l2_two_zone` 全绿。

## 5. 计数怎么变（按 §11.1 的伏笔分桶）

`cross_recipient_missing` 拆成三列（新增列须同步 `perf_tsv_summary.py` 与表头）：

| 列 | 含义 | 期望 |
| --- | --- | --- |
| `cross_recipient_delivered` | route=Some ⇒ 经邮箱投递成功 | 边界负载下 **>0** |
| `cross_recipient_missing` | route=None：玩家镜像但会话未登记 / 非玩家不参与广播 / **同 tick 下线**残差 | 正常负载 → **0 或仅同 tick 下线一类** |
| `cross_frame_queue_overflow` | 有界队列溢出被拒的条数 | **0**（非 0 即容量问题，要查） |

- 残差口径写死：`missing` 里**允许**的只有「同一 tick 内下线」（≤1 tick，投递点不可辨，见 §11.1）；
  出现其它来源（例如 route 传播被删、镜像过期未清）即缺陷。
- 验收（建议阈值，窗口实测后由总监定）：`delivered > 0`、`missing / (delivered + missing) < 1%`、
  `overflow == 0`；同时报分母（§12.4A 同一口径）。

## 6. 牙（先红后绿 + 变异清单）

每条用例先证明在**未实现**时红，且红的原因是断言而非编译错误；实现后绿；随后逐条列出变异。

- **T1 route 丢失**：删 `apply_mirrors` 的 route 传递 ⇒ 跨区帧不投递、`delivered==0`、
  `missing` 仍非 0 ⇒「跨区收件人收到该帧」用例红。
- **T2 镜像过期**：邻区快照清掉该 id ⇒ 镜像（含 route）同 tick 被清扫 ⇒ 不投递、无 panic、
  无 dup（断言 `delivered` 不增、`missing` 按口径处理）。
- **T3 收件人同 tick 下线**：`SM_CLOSED` 与帧同 tick ⇒ 投递失败计 `missing`，不 panic、不重试、不 dup。
- **T4 重复投递**：跨区帧与本地帧在同 tick 覆盖同一事件 ⇒ 去重键
  `(tick, source_zone, actor_id, recipient_route, ident, body_sha)` 保证**恰一条**；用例断言该
  收件人该事件恰 1 帧。
- **T5 handoff 提升中途**：镜像在源 tick 是 route=Some 的镜像、目标 tick 被提升为真实体（本地
  路径接管）⇒ 已入队跨区帧与本地帧重复由去重键拦下；用例：提升发生在窗口中间时恰一帧。
- **T6 有界队列**：注入超过上限 ⇒ `overflow` 按"拒新" +1、丢最旧（或拒新，**二选一并写死**）、
  不无界增长；用例断言上限处行为与计数。
- **变异清单**：删 route 传递 / 删去重键 / 把排水挪到 phase2 之后 / 去掉上限 / 把 N=1 闸拆掉 ——
  每条都要有一条用例转红（恢复后 sha 回合）。

## 7. 交付顺序建议（第一步先跑通单事件）

1. route 传递（`MirrorSpec`/World 存 route）+ 三桶计数；
2. 有界邮箱 + `tick_top` 排水 + 逐面接线（按 §3 的"产生相位 → 排水相位"表逐个核）；
3. 去重键 + ordering 判据用例（先红后绿）；
4. 牙 T1–T6 + N=1 复跑（三件套 + 两树 sha）；
5. 2 区边界负载窗口（与 ④a 的命令接线联调），按 §5 口径出交付读数。

## 8. 最大的两个技术风险

1. **帧序契约没有天然仲裁者**：双线程屏障下"同一收件人的本地帧 vs 跨区帧"全序只能靠设计承诺
   （`tick_top` 排水 + `(tick, phase_rank, seq)`）；任何一面在 phase2 之后才发现收件人都会破坏
   全序。缓解：逐面写"产生相位 → 排水相位"表，并把晚产生的面顺延到下 tick（登记）。
2. **提升/下线竞态**：镜像在下个 tick 变真实体、或收件人同 tick 下线，都会让邮箱里出现"过期
   收件人"；**route、镜像、去重键必须同源于 barrier1 后的本 tick 快照**，否则会出现重复或丢帧
   ——这与段 2 的迟到重发事故同族，必须把「帧身份」与「互斥窗口」写死并用 T4/T5 钉住。
