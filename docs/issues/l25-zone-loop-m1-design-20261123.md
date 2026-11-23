# L2-5 P1 · M1 设计：区 k 成为「完整的帧产生者」（2026-11-23）

> 状态：**设计稿（docs-only，未写码）**。基线 master：M0 收口 `d911c9469c60`。
> 依据：[`l25-per-zone-host-loop-scope-20261123.md`](l25-per-zone-host-loop-scope-20261123.md) §1/§4/§12；
> [`l25-zone-loop-m0-handoff-20261123.md`](zone-loop-m0-handoff-20261123.md)；
> [`l25-zone-loop-m0-design-20261123.md`](l25-zone-loop-m0-design-20261123.md)。
> 定界：opc-27 2026-11-23——**让区 k 出本区帧**（AOI 出现/消失、战斗/技能、物品、NPC/商店），
> 收件人**只限本区**；跨区收件人仍走 ⑤ 邮箱（已落地）。**本稿不含代码**；`spawn_minimal_zone`
> 的接线留到 M1 实施。
> 标注约定：**【已核】** = 已在代码/产物中核对；**【推断】** = 由事实外推、实施时须实测。

## 0. 与 M0 的接缝（一句话）

M0 已把区 0 的 host 循环抽成 `HostLoop::tick(..., PhaseSet)`，并给区 k 留了 `HostLoop::for_zone`
与空实例用例；M1 要做的三件事是：**①给区 k 一张生产段集**（`PhaseSet` 从 `M0_MIN` 扩展）；
**②把命令从「只认移动」扩到「本区可执行 / 回区 0 执行 / 暂不支持」三类**；
**③让区 k 的 `TickRecord` 被本区 drain 消费**（今天算了就丢）。

## 1. 分期切分（每片 = 一个可跑的两区场景 + 判据）

判据口径沿用今晚：**本区收件人帧集合与顺序**（`frames.ndjson` 与 N=1 golden 比）、
`HostTickProfile` 的 `work`、窗口 `p99`（≤200ms）、`in_world` 计数、网关每线 `max_depth`。

| 片 | 范围 | 两区验收场景 | 判据（缺一不算） |
| --- | --- | --- | --- |
| **M1a 骨架出帧** | 区 k 的 `TickRecord` 由本区 drain 消费：AOI 出现/消失、怪物转身、受伤 53/657；**移动命令保持现状**（已通） | 区 0 放 bot A、区 k 放 bot B（各带 1 只怪）；两 bot 只在本区走动/被打；两区跑 60s | ① 区 k 的 B 收到与 N=1 同场景**同集合、同顺序**的 AOI 帧（golden 由 N=1 跑同脚本得到，**不从 Rust 输出反推**）；② 区 0 的 A 帧集合不因 B 的存在而变；③ 区 k `work` ≤ N=1 同实体数的 1.1×；④ p99 ≤200ms、`in_world` 两区合计 0 丢失 |
| **M1b 战斗/技能命令** | 区 k 本区处理 `handle_hit_cmd` / `handle_magic_key_cmd` / `handle_spell_cmd_with_magic`；伤害/死亡/经验帧本区出 | 区 k 的 B 攻击本区怪、放一个术法（各一条脚本） | ① 帧字节与 N=1 同场景相同（逐帧）；② 伤害数值与 golden 相同；③ `work` 不回退超 10%；④ 三条 drain（53/657/死亡）逐支变异牙各红一次 |
| **M1c 物品/NPC/商店** | 区 k 本区：捡/丢/吃/用/存/查；**商店与点击 NPC 回区 0 执行**（脚本宿主 `!Send`）；组队/说话本区扇出，跨区经 ⑤ | 区 k 的 B：捡一件地面物品、吃一个药、点一次 NPC（脚本给一句话 + 一个物品） | ① 物品帧与 N=1 相同；② NPC 脚本回复帧与 N=1 相同且**延迟差 ≤1 tick**（登记偏差或同 tick 达成，见 §2）；③ `MakeIndex` 跨区唯一（`item_number` 单点）；④ 回 0 通道的锁序用例绿（§7） |

> 分期原则：**每片只扩「段集 + 命令白名单」，不碰 `World.OutputAoi`/`AoiMonsterWalk` 算法、
> 不碰 `collect+sort` 顺序契约**。若某片发现必须动算法，停手报 opc-27（边界外）。

## 2. 命令接管表（M1 归属）

区 k 今天的入口是 `handle_zone_command`（**只认 `movewire::is_move_command`**，其余计入
`zone_cmd_unsupported`）**【已核】**。M1 的三类归属：

| 命令族 | M1 归属 | 理由 | 通道 / 延迟 |
| --- | --- | --- | --- |
| 移动（`CM_TURN/WALK/RUN/HORSERUN`） | **本区**（现状） | 纯 World 操作，④a 已证明可复用同一 handler | 无 |
| 攻击（`handle_hit_cmd`） | **本区** | 只动本区 World（命中/伤害/死亡都在 TickRecord） | 无 |
| 技能/术法（`handle_magic_key_cmd`/`handle_spell_cmd_with_magic`） | **本区** | 只动本区 World；魔法表是进程级只读 `Arc` | 无 |
| 背包/地面/吃/用/存/查询 | **本区** | 纯区级容器（`ground_sessions`/`HostLoop` 内表）；`ground_item_catalog` 只读 | 无 |
| **商店 / 点击 NPC（脚本）** | **回区 0 执行** | `npc_host`/`merchant_book` 含 `Rc` ⇒ `!Send`，物理上只能在区 0 线程跑 | **控制邮箱**：区 k 收命令 → 投 `ZoneHost` 的 per-zone 控制 inbox → 区 0 在**下一 tick** 的 `HostInbound` 段首 drain，**持锁**操作区 k 的 `HostLoop` 并用区 0 的 `gates` 发帧；**延迟恒为 T+1（200ms）**，见 §2.1 |
| 说话（`handle_say_with_saves`） | **本区**扇出；跨区/全服/行会经 ⑤ | 本区收件人可本地算；跨区收件人已由 ⑤ 邮箱合成 | 跨区 = ⑤ 既有语义（T+1） |
| 组队（`handle_party_cmd`/`world.party_run_tick`） | **本区**；跨区成员通知经 ⑤ | `party_run_tick` 在 World 内，区 k 已有 | 跨区 = ⑤ |
| 行会 / 城堡 / GM / 换服 / 改图 handoff | **回区 0**（或暂不支持 + 计数） | 进程级单点（文件写/存档/全局状态机） | 控制邮箱；行会/城堡有 10s 门，T+1 可接受 |
| 说话里的 **GM `@` 支**（`@` 命令体触 guild/castle/gm_*） | **回区 0** | GM 命令体要读写进程级单点（行会/城堡/GM 状态） | 控制邮箱（T+1）；纯聊天文本仍本区扇出 |
| 交易 `handle_deal_cmd` | **同区本区；跨区走 ⑤ 或 defer** | 双人可跨区：同区两人本区可完成；跨区须两边状态（暂不接） | 同区无；跨区 **defer + 计数**（M1c 评估） |
| 寄售 `handle_selloff_cmd` | **回区 0** | `sell_off_book` 是进程级单点（全服共享报价，分区会分裂） | 控制邮箱（T+1） |
| 其余未列命令 | **暂不支持 + `zone_cmd_unsupported` 计数**（逐条点名见下） | 未验证的命令不许静默丢；计数保证可观测 | 无 |

**「暂不支持」逐条点名（M1 首批）**：`SS_*`/`ISM_*` 服务端到服务端消息、跨服/换服、改图 handoff、
攻城相关的 GM 手动指令、以及任何**未在本表列举**的 `cmd.ident`（一律计 `zone_cmd_unsupported`，
**不落入静默**）。

- **控制邮箱形态（【推断】，实施时定稿）**：`ZoneHost` 增 `Vec<ControlInbox>`（每区一个
  `mpsc::Sender/Receiver`，与既有 `handoff_boxes`/`cross_mailboxes` 同构）；消息 =
  `{ chr_name, socket, cmd, src_tick }`；区 0 在 `HostInbound` **段首**drain（在普通事件之前，
  保证「脚本命令不排在后面」）。
- **锁序（硬约束）**：区 0 drain 控制邮箱时**只短暂持锁**区 k 的 `HostLoop`，且**不得**在持锁
  期间再取区 0 自己的锁（`db` 等）；区 k 线程侧保持「barrier 之间才持锁」的既有纪律。
  已知死锁面见 §7-②。
### 2.1 happens-before 论证：回 0 命令**只能是 T+1**（opc-27 2026-11-23 裁决后修订）

区 k 的命令入队发生在它自己 tick 的两处之一：**（A）tick 顶的 `rx.try_recv`**（在它 phase1
之前、也就在它 `barrier1(T)` 之前）；**（B）phase2 内**（在 `barrier1(T)` 之后、`barrier2(T)` 之前）。

- 区 0 的 `HostInbound` 属于**它自己 tick T 的 barrier1 之前**（17 段顺序：
  `HostInbound → … → HostWorldHead(phase1) → barrier1`）。它与区 k 的入队**只共享「都在各自
  barrier1 之前」这一点，彼此无先后保证** ⇒ 情形 A 不能保证同 tick 被排到（区 0 的 inbound 可能
  早已跑完）。
- **可证的界**：区 k 的入队（A 或 B 都在 `barrier2(T)` 之前）**先于** `barrier2(T)`（屏障会合）；
  而区 0 的 T+1 `HostInbound` 在 `barrier2(T)` 之后 ⇒ **区 k 在 tick T 入队的任何命令，必然在
  区 0 的 T+1 `HostInbound` 被看到**。
- 结论：**按「被 drain 的 tick」计，延迟恒为 T+1（200ms）**；"同 tick 靠 barriers 窗口"是**错误
  表述**，已删。**到达→被处理的最坏情形 ≈ 2 个 barrier 间隔**（命令在区 k 的 loop 顶 drain 之后
  才到时，要等下一 tick 的 loop 顶才入队，再等区 0 的 T+1 inbound）——验收按「被 drain 的 tick」
  计 T+1，最坏时延在窗口读数里单独看。
- **结构不变式 + 牙**：控制邮箱的**入队只允许发生在 `barrier1` 之前**（区 k 的 loop 顶 → 入队 →
  其后的 phase1/barrier1）。为此**禁止未来新增 post-`WorldTail`/post-barrier2 的收包点**；
  用例：在 `HostWorldTail` 之后向控制邮箱入队必须被断言拒绝（红），防止有人加「晚到也能补发」的旁路。

### 2.2 偏差条目（具体条目，不是"待立"）

| 编号（暂定） | 内容 | 影响面 | 验收口径 |
| --- | --- | --- | --- |
| **D-M1-CMD-T1** | 回区 0 执行的命令（商店/点击 NPC；行会/城堡/GM 请求）及其结果帧比 oracle 晚 **1 tick（200ms）** | ①NPC 对话文本与随附物品帧整体晚 200ms；②商店买卖的结果帧晚 200ms；③依赖「同 tick 内先脚本后 AOI」的极端场景可见差异 | 两区对拍的期望帧序按 **+1 tick 平移**后与 N=1 golden **逐帧相同**（平移规则写进场景脚本，不在断言里放宽） |

- 正式编号在 **M1 卡**建立时挂到 T-* 台账；本卡（M0）不承载 M1 偏差。
- **该偏差同时写进 M1 卡的验收口径**：「NPC 对话/商店帧晚 200ms」是**已知偏差**，不属于回归。
- 旧表述（"T-* 待立"）删除：以下为原始影响描述——oracle 单线程下脚本交互是**同 tick 同步**；
  两区模式下回 0 命令与其结果帧比 oracle 晚 1 tick。
- **可见影响**：NPC 对话/商店买卖的**帧相对顺序**（对话文本与随附物品帧仍彼此同序，但整体晚 200ms）；
  依赖「同一 tick 内先脚本后 AOI」的极端用例可能观察到差异。
- **不可见**：静态/周期行为（怪物 AI、刷新、AOI 扫描）不经过该通道；纯本区命令（移动/攻击/技能/
  背包）不走该通道。
- **验收口径**：两区对拍把「回 0 命令」场景的期望帧序按 **+1 tick 平移**后比对（golden 仍来自
  N=1，平移规则写进场景脚本），并显式断言「平移 1 tick 后与 N=1 逐帧相同」。

## 3. 状态与锁

- **`ZoneState` → `HostLoop`**（M1 第一步，纯结构）：`ZoneState { host: HostLoop, save_outbox,
  alive }`；`handle_zone_closed`/`run_zone_tick` 改取 `st.host.*`（现 `st.*` 访问点 11 处）。
  这样区 k 与区 0 跑**同一份** `HostLoop::tick`，差异只剩 `PhaseSet` 与 `HostServices` 的可达性。
- **`HostServices` 在区 k 的形态**：`!Send` 的成员（`npc_host`/`merchant_book`/`guilds`/
  `castle_*`/`db`/`engine`）**不进区 k**——区 k 通过控制邮箱把需要它们的命令交回区 0。
  区 k 的 `HostServices` 只填：`enter_cfg`（只读）、`registry`、`shard_runtime`、`gates`（本区表）、
  `magic_table_arc`、`item_number`（**必须共享同一计数器**，见 §7-⑤）、`perf`（原子共享）。
- **`gates`**：每区一份、demux 广播填充 **【已核】**——M1 直接复用；区 k 发帧只查本区表。
  需要新增的是**广播缺口检查**：区 k 上线时若 `gates` 为空而本区已有会话 ⇒ 报 WRN（防「帧发不出去了却没人知道」）。
- **先纠正一个表述（0c 2026-11-23）**：区 0 的 `db`/`guilds`/`castle`/`engine` **不是锁，是
  `sim_loop` 线程局部的 `&mut`**（`HostInbound` 的 AdminGm 支已直接 `&mut guilds`；
  `finish_login` 也在**持有区 k 锁的同时**借用 `&mut guilds`——既有代码即证明该模式安全：同一线程、
  借用检查器保证无别名）。真正的死锁面只有两条：
  **(a) 同一区 k 的 `std::sync::Mutex` 重入（自死锁）；(b) 跨区 `HostLoop` 锁的 ABBA 序**。
  约束写成：**持区 k 锁期间不得再取任何 `HostLoop` 锁（含同一把）**；进程级 `&mut` 借用不受限
  （同线程、编译期保证）。
- **持锁路径与锁序（逐路径，opc-27 2026-11-23 要求具体到「取哪把锁、何时取、持多久」）**：
  | 路径 | 取的锁 | 何时取 | 持多久 | 期间允许/禁止 |
  | --- | --- | --- | --- | --- |
  | 登录落区（既有 1a） | 区 k `HostLoop` 锁 | 区 0 的 `HostInbound` 段内 | 单次登录处理 | 允许：建会话/写 world，**并借用进程级 `&mut`（`finish_login` 已借 `&mut guilds`）**；禁止：调用会阻塞的 DB 操作（DB 请求走 `req_tx`） |
  | 断线（既有 1c） | 区 k `HostLoop` 锁 | 区 0 的 `HostDbPoll`/控制段内 | 单条 `Closed` | 同上；存档入 `save_outbox` |
  | 商店/点击 NPC（新） | 区 k `HostLoop` 锁 | 区 0 `HostInbound` 段首（drain 控制邮箱） | **每条命令一次**，不跨命令持有 | 允许：脚本执行 + world/会话修改 + 发帧（进程级 `&mut` 同线程、编译期借用保证）；**禁止**：再取任何 `HostLoop` 锁、调用会阻塞的 DB 操作 |
  | 行会/城堡/GM（新） | **只持区 0 自己的锁**（不取区 k） | 区 0 `HostPeriodic` 的 10s 门内 | 一个门周期 | 结果帧发给区 k 玩家时只查 `gates`，不动区 k 状态 |
  | 换服/改图 handoff | 既有 handoff 邮箱（无锁） | 既有相位 | — | 不改 |
- **背压策略 = 停排，不丢、不造帧**（opc-27 2026-11-23 裁决；"同款文案"没有既有对应物——
  区 0 普通背压 `INBOUND_BUDGET_HIT` 只把事件留到下一轮、不发帧；gate 背压溢出是 `GM_CLOSE` 断线，
  两者都不适合）：
  - 控制邮箱**有界**（初值 K=**1024**，以窗口实测峰值为准**上调**，长期 >0 的溢出/背压即上限不合理）；
  - 满 ⇒ 区 k **不再从自己的入站排下一条控制命令**（命令留在 carry/通道里，与 `EventDrain` 的
    carry 同款语义），计 **`zone_control_backpressure`**（新 perf 列，标 additive），并打一行
    `WRN`（zone / 命令 ident / 队列长度）；
  - **不丢命令、不发任何新帧、不新造文案**（保逐字节对拍）；
  - **只许延后「控制类」命令，不许牵连「本地类」**（0c/opc-27 2026-11-23 硬条件）：
    - 控制类（回 0：商店/点击 NPC/行会/城堡/GM/换服/handoff）⇒ 邮箱满时留在 **carry**（有界且可观测）
      并计 `zone_control_backpressure`；
    - 本地类（移动/攻击/技能/背包/地面/本区说话扇出/组队）⇒ **必须继续流动**；**一个满的控制箱
      不许把区 k 的整条入站 stall 掉**（那等于给本地命令加隐藏背压）；
    - 已转发的控制命令**保 FIFO**（回退 carry 不得乱序）。
  - 用例（两条，缺一不算）：(a) 满 ⇒ 控制命令留 carry + 计数 +1 + **无新帧**；
    (b) **同 tick 的下一条本地命令仍被处理**（防「过度回压」的负例，必须能红）。

## 4. N=1 与 N≥2（区 0 的「双角色」）

- **`slicing=0` 逐字节不变**：M0 已证（N=1 两树对拍）；M1 的新代码路径**全部**挂在
  `slicing_layout.is_some()` 之下，默认不构造任何区实例、不建控制邮箱、不改 `PhaseSet`。
- **N≥2 时区 0 与今天的差异清单**（最容易出错的地方，实施时逐条核）：
  1. **区 0 仍是宿主**：登录编排、DB、行会、城堡、换服、`drain_zone_saves`、控制邮箱 drain 全在区 0；
  2. **区 0 同时是自己的区**：它的实体/会话在 `sim_loop` 的 `HostLoop`，不在 `ZoneState`；
     **不得**把区 0 也做成一个 `Arc<Mutex<ZoneState>>`（那会引入锁而改变热路径）；
  3. **`gates` 两份**：区 0 用栈上表、区 k 用各自表，同样由 demux 广播填充（既有事实）；
  4. **`host_prof` 每区一份**：M1 必须给 `[TICKPROF]` 加区标签或仅区 0 打（M0 交接 §3-⑤）；
  5. **`perf_counters`**：原子共享；区 k 的计数若是新列，必须标 **additive**（旧基线只对旧列）；
  6. **`World.owner_thread`**：每个 World 只能在自己的线程 tick——控制邮箱执行脚本时
     **不得** tick 区 k 的 World（只做命令级修改 + 发帧）。
- **每条的可测断言（2 区模式下）**：
  1. 区 0 仍是宿主 ⇒ 断言 2 区模式下 `in_world` 计数、登录落区、`drain_zone_saves` 均与单区同码路径（用例：登录一个区 k 玩家 + 断线存档）。
  2. 区 0 不包 `ZoneState` ⇒ 源码锚用例断言区 0 无 `Arc<Mutex<ZoneState>>` 构造点（`slicing=0` 与 `N≥2` 都只有 `spawn_minimal_zone` 一处构造）。
  3. `gates` 两份 ⇒ 用例：区 k 玩家的 socket 只出现在区 k 的 `gates` 表；对区 0 会话扇出的帧不会写给区 k socket（帧收件人断言）。
  4. `host_prof` 标区 ⇒ TICKPROF 行含区号（或仅区 0 打），用例解析输出行数/区号。
  5. perf 新列 additive ⇒ 摘要脚本对旧产物 fail-closed、对新列标 `additive`（单测）。
  6. `World.owner_thread` ⇒ 用例：控制邮箱路径在**非区 k 线程**尝试 tick 区 k World 时必须 panic（`should_panic`），证明没有旁路。
- **N≥2-only 的三类行为差（0c 2026-11-23 要求显式列出并各给断言）**：
  1. **④b-1 静态实体按区过滤**：区 0 的 World 只含自己 rects 的静态实体（未覆盖图归区 0）。
     断言：2 区模式下区 0/区 k 的静态实体集合**交集为空、并集 == N=1 全量**（既有 `zone_statics_tests` 口径扩到生产接线）。
  2. **跨边界实体不是同一个 World 对象**：边界两侧看到的同一 actor 分别是本区实体与对区的
     镜像/快照（`apply_mirrors`），actor_id 相同但对象不同。断言：跨边界用例中两侧 `actor_id`
     相同、`mirror` 标志符合预期、且**不会**在两侧同时被当作本地实体 tick。
  3. **RNG 是 per-zone 流**（`seed ⊕ zone`），与 N=1 的单一流**不等价**——这是已知差异（④d 才解）。
     断言：两区模式**不**与 N=1 比 RNG 流水；只比**确定性输出**（帧/快照），并在卡上登记「RNG 流
     分区不等价」为已知偏差（不属回归）。
- **判据**：N=1 两树对拍（a7）；N=2 时区 0 单跑与 N=1 的帧集合相同（多区不改变本区语义）。

## 5. 与 ⑤ 的接缝

- **区 k 成为发送者**：`cross_zone_frames::install` 目前只在区 0 安装（`ZoneHost.cross_mailboxes`+
  `router`）。M1 让每个区线程在启动时 `install(Arc::clone(mailboxes), Arc::clone(router), zone_id)`；
  `note_mirror_recipient` 的 `src_zone/seq` 由 `begin_tick(tick_no)`（区 k 也是 tick_no）自动成立。
- **分槽语义不变**：每 `(dst,src)` 槽单写者（区 k 只写自己的 src）；`src_tick < current tick` 的
  T+1 排空不变；`MAX_FRAMES_PER_SLOT` 拒绝新增不变。
- **`PhaseSet` 在区 k 生产模式**（**17 位逐位表**，含排除位；opc-27/0c 2026-11-23 要求）：

  | 位 | 段 | M1 归属 | 「区 k 不跑它会坏什么」/ 为什么可以不含 |
  | --- | --- | --- | --- |
  | SHUTDOWN | `HostShutdown` | **不含** | 区 k 的退出由 `stop` 旗标负责；**关服存档是区 0 的职责**（`shutdown_save_all` 只扫区 0 会话，且有副作用）⇒ 区 k 不得进入该段；区 k 的断线存档走 `save_outbox` |
  | INBOUND | `HostInbound` | **M1b**（M1a 命令仍由区线程 loop 顶的 `handle_zone_command` 处理） | 不跑 ⇒ 本区玩家的战斗/技能命令无人执行（M1a 只做移动 + 出帧） |
  | DB_POLL | `HostDbPoll` | **不含** | DB 链是进程级单点（区 0）；区 k 的存档请求走 `save_outbox` → 区 0 `drain_zone_saves` |
  | PERIODIC | `HostPeriodic` | **不含** | 行会/城堡/滚动公告是单点；**刷怪不在这一段**（见下「mon_gen」注），不需要为刷怪开 PERIODIC |
  | WORLD_HEAD | `HostWorldHead` | **M1a**（已在 `M0_MIN`） | 不跑 ⇒ 本区无 tick head（怪物 AI/命令 FIFO/死亡结算全停） |
  | **POST_HEAD** | `HostPostHead` | **M1a（新增，必修）** | 不跑 ⇒ **战斗结算**全缺：`drain_yedo_events`（击杀计数/攻杀）、`drain_skill_train_events`（技能熟练）、`drain_attack_wear_then_monster_refs`（= `drain_attack_weapon_wear` + **`drain_monster_ref_msgs`**，怪物侧 Ref 帧；函数内注释可见 656=目标 `BreakHolySeizeMode`——**具体帧面实施时以帧字节为准，不写死**）、`apply_get_start_point_after_tick`（3s 回城点）。M1a 声称的「怪物转身」正是这一段的产出 |
  | **PLAYER_RUN** | `HostPlayerRun` | **M1c** | 不跑 ⇒ 玩家逐 tick 停摆（hunger/复活 `revival`/gold/`auto_get_exp`/`run_rates`/`flush_run_frames`）。M1a/M1b 不需要：区 k 的移动帧由 `handle_move_cmd(…, None)` **直发**（不经 `run_frames` 收集器），`flush_run_frames` 只属区 0 的 `Some(collector)` 路径 |
  | WORLD_TAIL | `HostWorldTail` | **M1a**（已在 `M0_MIN`） | 不跑 ⇒ 无 phase2（AOI 扫描）/死亡结算尾部，玩家永远看不到别人 |
  | PLANES | `HostPlanes` | **不含** | G11 跨服在两端皆 inert（卡 `g11-cross-server-inert-oracle`），区 k 无 planes 链接 |
  | RECOVERY | `HostRecovery` | **M1c** | 不跑 ⇒ 恢复/体力/血量回复停摆（与 `PLAYER_RUN` 的 hunger/revival 同批） |
  | FRAMES | `HostFrames` | **M1a** | 不跑 ⇒ 本区事件（动作/屠宰/骨骼/buff/**53/657 的 B2–B5 自投递**）不出帧——**53/657 由本段产出**（`drain_b3_self_deliveries`/`drain_b4_with_enter_cfg`/`drain_b5_self_deliveries`），**不是** POST_HEAD |
  | DEAL_TICK | `HostDealTick` | **M1c** | 不跑 ⇒ 交易自动取消不生效（会话卡在 Dealing） |
  | AOI_DOWNLINK | `HostAoiDownlink` | **M1a** | 不跑 ⇒ AOI 出现/消失与 52/53/体力帧不出（M1a 的核心） |
  | PERSIST | `HostPersist` | **M1c** | 不跑 ⇒ 区 k 断线存档不进 `save_outbox`（数据丢失）；只开 `save_outbox` 分支，不发 DB |
  | SWITCH | `HostSwitch` | **不含** | 换服/重连是单点（依赖进程级 `server_table`） |
  | DIAG | `HostDiag` | **不含** | 观测仅区 0（`host_prof` 每区一份但区 k 不打；见 §4-4） |
  | SLEEP | `HostSleep` | **不含** | 区 k 节奏由 barriers 决定，不 sleep |

  ⇒ **M1a 的段集** = `M0_MIN | POST_HEAD | FRAMES | AOI_DOWNLINK`；
  **M1b** += `INBOUND`；**M1c** += `PLAYER_RUN | RECOVERY | DEAL_TICK | PERSIST`。
  每扩一位都要更新 `RAN_SEGMENTS` 的精确相等断言。

- **mon_gen（刷怪）的位置钉死（P2-4）**：区 k 的 `step_mon_gen` 现在在**区线程 loop 顶、
  `tick_phase1` 之前**（不在 `PERIODIC` 段）。**M1 保持它在 loop 顶，不搬进 `HostLoop::tick` 的
  `PERIODIC`**——若要搬，必须新增独立 `MON_GEN` 子段并入 `M1_MIN` 并给「不跑会坏什么」。

## 6. 验证装置

- **两区对拍（真机真实栈）**：沿用评估稿 §4 的装置——`l2_two_zone.rs` 场景 + `MIR2_ZONES=2`
  + 两个 bot（各在本区）。**golden 来自 N=1 同脚本跑两次**（同 seed），**不从 Rust 输出反推**
  （卡 `parity-expectations-from-golden-not-rust`）。
- **先红后绿的用例**（每片至少一条）：片内新增的 drain（AOI 出现/消失、53/657、物品）各留一条
  「删掉接线即红」的牙；控制邮箱留「锁序」与「同 tick 延迟」两条。
- **只窗口读数的**：`work`/`p99`/`in_world`/`max_depth`——由 `perf_tsv_summary.py` +
  `load_gate` 报告判定，不写成单测。
- **N=1 硬门**：每片合入前 a7 在最终树跑两树 stdout 对拍（同 M0 口径）。

## 7. 风险清单

1. **区 0 双角色**：任何把区 0 也包成 `ZoneState` 的"对称化"冲动都会给热路径加锁 ⇒ 明令禁止（§4-2）。
2. **脚本宿主回 0 的锁序**：**真正的死锁面只有两条——(a) 同一区 k 的 `std::sync::Mutex` 重入
   （自死锁）；(b) 跨区 `HostLoop` 锁的 ABBA 序**。进程级 `db`/`guilds`/`castle`/`engine` 是
   `sim_loop` 线程局部 `&mut`，**同线程、编译期借用保证，不受锁约束**（`finish_login` 持区 k 锁
   借 `&mut guilds` 即先例）。规则：**持区 k 锁期间不得再取任何 `HostLoop` 锁**；需要 DB 的命令
   （存档类）走 `save_outbox` 的异步出口。用例：`try_lock` 重入场景与跨区 ABBA 场景各一条（超时即红）。
3. **AOI 显示表生命周期**：区 k 的 `displays` 与 `apply_mirrors` 的交互（镜像展示项写入收件区表）
   在区 k 有真会话后语义变复杂（镜像 ↔ 本地同槽）；M1a 起每个 tick 后断言「display 表与
   World 实体集合一致」（`assert_mirror_routes_consistent` 的同款手法，仅测试期）。
4. **登录/断线既有 1a/1c 路径**：登录由区 0 持锁写区 k；M1 让区 k 也处理命令后，**同一会话可能
   同时被区 0（登录）与区 k（命令）触碰** ⇒ 锁序同上；断线清理（`handle_zone_closed`）要在
   控制邮箱里**作废**该会话的未执行脚本命令（否则对已离开的会话发帧）。
5. **`item_number` 共享**：区 k 造新件（商店 GIVE、捡起生成）必须用**同一** `item_number` 计数器，
   否则 MakeIndex 撞号（进程级单点；区 k 只读借用须是 `Arc<Mutex>` 或经区 0 分配）。
6. **帧序**：跨区帧 T+1、本区帧同 tick、回 0 命令最多 T+1 ⇒ 三者混在一处时的顺序契约要用
   两区对拍钉住；不得用「重试/放宽断言」掩盖。
7. **`spawn_minimal_zone` 接线**：M1 实施才动；接线本身要保留 `slicing=0` 的零构造（M0 §3.1 的构造闸）。

## 8. 实施顺序与可审的提交序列（opc-27 2026-11-23 定稿；同日修订）

**顺序（修订）**：`M1-0a`（已完成）→ **`M1-0c'`（新前置：b-1 服务拆分 + fail-fast）** → `M1a`（出帧）→
`M1b`（战斗/技能）→ `M1c`（物品/NPC）→ `收口`。每片：一提交序列 + 全量门 + **N=1 两树对拍（最终树）**。

> **`0b`（方法化）已从「前置」移出**（opc-27 2026-11-23 裁决）：M1a 的四项能力全部经
> `HostLoop::tick(services, io, phases)` 一个入口可达，**没有「某段需要不同 `&mut` 组合」的需求**
> ⇒ 0b 是纯可读性重构、零功能收益，推到 M1a 之后；将来真出现「某段要单独调用」（唯一候选 =
> M1c 控制邮箱），**按需只拆那一段**，不整函数 17 段全拆。届时再按 8.1 的 0b 清单执行。

### 8.1 M1-0a（已完成，保留）
`M1-0a` 的成果 = 把 `HostLoop::tick` 交回 rustfmt（移除 `#[rustfmt::skip]`）+ 一次性格式化 + 修掉
3 个**既有**测试脆弱点（字节切片多字节 panic、折行/缩进字面量各一）。提交序列：
`f5a9fae7f`（移 skip）→ `b9b809ef2`（fmt）→ `4f9e7e582`（字面量同步）→ `304f190d5`（#3/#1 修复）→
`356c3ce7c`（#3 切分 fail-closed 硬化 + #1 防御性留痕）。零语义判据 = 去空白**并去掉 `,{}`** 后
0a-1↔0a-2 逐字符相同。

### 8.2 `M1-0c'` 设计（b-1：服务拆分 + 唯一收敛点 + fail-fast）

**动机（0c 实测）**：6 个进程-only 字段在 **M1a 段集内的使用 = 0**；全 tick 体共 **59 处**使用，只落在
**SHUTDOWN 3 / INBOUND 26 / DB_POLL 12 / PERIODIC 5 / PERSIST 1**（其中 39 处 reborrow 敏感）。

**形态**：`HostServices` 拆成两块，`tick` 签名变为
```rust
fn tick(self, shared: SharedServices, process: Option<ProcessServices>, io: TickIo, phases: PhaseSet) -> TickOut
```
- 区 0 传 `Some(process)`；区 k（M1 起）传 `None`。
- 入口 `let mut process = process.unwrap_or_default();`（`CastleHostGates` 另外补
  `impl Default { fn default() -> Self { Self::new(0) } }`，见下「依据」）⇒ 进程段仍 **owned 解构、0 reborrow**。

**六个字段的空构造与副作用（b-1 的前提，已核）**：
| 字段 | 类型 | 空构造 | 构造副作用 |
|---|---|---|---|
| `engine` | `load::FrontEngine` | `#[derive(Debug, Default)]` | 无（仅 Vec） |
| `guilds` | `mir2_world_bevy::GuildManager` | `#[derive(.., Default, ..)]` | 无（空表；读盘在 `startup`，不在 `new`） |
| `merchant_book` | `game_svr::npc::MerchantScriptBook` | `#[derive(Debug, Default)]` | 无（BTreeMap） |
| `castle_gates` | `game_svr::castle::CastleHostGates` | **无 `Default`** ⇒ **补 `Default = new(0)`** | 无（两个 i64） |
| `castle_war_states` | `HashMap` | `HashMap::new()` | 无 |
| `sell_off_book` | `game_svr::selloff::SellOffBook` | `#[derive(.., Default, ..)]` | 无（Vec） |

- **`CastleHostGates` 的依据（opc-27 要求写死）**：补 3 行 `impl Default`，**不**手写字段全集字面量
  （字段遗漏会编译错还好；最怕将来新增字段时 `..Default::default()` 把新字段悄悄零值化）。
- **`new(0)` 的语义注释（必须写进代码）**：`run_tick/save_tick = 0` 意味着**若有人将来给区 k 开
  PERIODIC，首次比较会立即触发**。当前的安全前提是 **区 k 不进 PERIODIC**，由下面的 fail-fast 断言兜底。

**fail-fast（release 也生效，`assert!` 不是 `debug_assert!`）**：
- `PROCESS_ONLY` 常量与 `M0_MIN`/`M1_MIN` **放在同一处**（`PhaseSet` 定义旁），掩码写死
  `SHUTDOWN | INBOUND | DB_POLL | PERIODIC | PERSIST`——与 0c 的 59 处/5 段实测**互为交叉校验**。
- `tick` 入口：
  ```rust
  if phases.0 & PhaseSet::PROCESS_ONLY != 0 {
      assert!(process.is_some(), "区 k 不得进入进程-only 段（PhaseSet 位集=0x{:05x}）", phases.0);
  }
  ```
  理由：release 下静默读默认值比 panic 更糟（会把"空壳服务"当真服务读写）。
- **两枚牙**（缺一不算）：
  1. **正向**：`M1a` 的位集与 `PROCESS_ONLY` 相交为空（`const` 断言或单测）——保证后续给区 k 加段时
     不会误开进程段；
  2. **负向**：`phases` 含任一 `PROCESS_ONLY` 位且 `process = None` ⇒ **必须 panic**，`should_panic`
     断言消息关键字（与 `RAN_SEGMENTS` 段集断言互补：那条证「跑了的段对」，这条证「没跑不该跑的段」）。
- `ZoneState { host: HostLoop, save_outbox, alive }` 的整理仍在 `M1-0c'` 内（零行为变化）。

### 8.4 测试基建现状（**已落地的前置事实**，2026-11-23）

> 由 `M1-0a` 修掉的测试脆弱点，**不要当成未结问题**。代码在分支 `card/m1-0a`，tip **`356c3ce7c`**
> （`0a-5`；其父链 `304f190d5`/`4f9e7e582`/`b9b809ef2`/`f5a9fae7f`，前者已随 master
> `5db4689086e1` 合入；`0a-5` 待 opc-27 排期合并）。

- **生产/测试切分（#3）**：`login_tail_production_call_is_wired_after_aoi` 不再裸 `find("mod tests {")`
  ——改为**行级精确匹配**（断言 `trim()=="mod tests {"` 恰一行）+ 找不到即 `expect`（fail-closed），
  并保留「全文件首个命中必须在生产区」两条断言（防注释漂移）。**这条现在的形态是防未来脆弱**，
  不是补当时的错。
- **字节切片（#1）**：`game-svr` 测试里两处窗口截取都改为字符边界安全——
  `entry_telemetry_wiring::window()` **起点与终点都回退**、`aoi_shutdown_wiring::window()` 只回退终点
  （起点来自 `find`）；其余 `SRC[i..]`/`SRC[i..next]` 的起点均来自 `find`，天然在字符边界。
  **截至 `0a-5`，本仓 `game-svr` 测试无未处理的多字节裸切片**。
- 同批修掉的既有脆弱点另两处：`g3_anchor_a5` 的折行字面量、`login_tail` 的缩进字面量（都不再钉某一种
  折行/缩进）。
- 零语义判据（0a-2）：去空白**并去掉 `,{}`** 后与 0a-1 逐字符相同（3,216,037 字符）。

### 8.3 将来若做 0b（方法化）的既有约束（保留，供按需拆段时用）
- 方法行是缩进行、不匹配 `starts_with("fn ")` ⇒ **方法化本身不截断扫描窗口**；风险只在新增**顶层**
  `fn`。但方法化会改**逐字行**（8.1 已列的 7 条 + 2 条 contains/count_seq 锚）⇒ 必须逐条同步牙。
- 三条顺序锚牙（`let mut rec = tick_world_head_with_specials(`）在方法化后需**重锚到 `tick` 内的
  `phase_*` 调用序列**（两层：段体内序 + 调用序），并按 opc-27 要求带两条变异红证。
- `#[rustfmt::skip]` 已在 M1-0a 移除；若将来拆段，格式化由 rustfmt 正常接管。
