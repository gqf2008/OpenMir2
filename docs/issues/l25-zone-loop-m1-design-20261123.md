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
| 其余未列命令 | **暂不支持 + `zone_cmd_unsupported` 计数** | 未验证的命令不许静默丢；计数保证可观测 | 无 |

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
- 结论：**回 0 命令的延迟恒为 T+1（200ms）**；"同 tick 靠 barriers 窗口"是**错误表述**，已删。

**偏差登记（T-* 待立）**：oracle 单线程下脚本交互是**同 tick 同步**；两区模式下回 0 命令与其
结果帧比 oracle 晚 1 tick。
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
- **持锁路径与锁序（逐路径，opc-27 2026-11-23 要求具体到「取哪把锁、何时取、持多久」）**：
  | 路径 | 取的锁 | 何时取 | 持多久 | 期间允许/禁止 |
  | --- | --- | --- | --- | --- |
  | 登录落区（既有 1a） | 区 k `HostLoop` 锁 | 区 0 的 `HostInbound` 段内 | 单次登录处理 | 允许：建会话/写 world；禁止：取 `db`/`guilds` 锁（DB 请求走 `req_tx`） |
  | 断线（既有 1c） | 区 k `HostLoop` 锁 | 区 0 的 `HostDbPoll`/控制段内 | 单条 `Closed` | 同上；存档入 `save_outbox` |
  | 商店/点击 NPC（新） | 区 k `HostLoop` 锁 | 区 0 `HostInbound` 段首（drain 控制邮箱） | **每条命令一次**，不跨命令持有 | 允许：脚本执行 + world/会话修改 + 发帧；**禁止**：取区 0 的 `db`/`guilds`/`castle` 锁、调用会阻塞的 DB 操作 |
  | 行会/城堡/GM（新） | **只持区 0 自己的锁**（不取区 k） | 区 0 `HostPeriodic` 的 10s 门内 | 一个门周期 | 结果帧发给区 k 玩家时只查 `gates`，不动区 k 状态 |
  | 换服/改图 handoff | 既有 handoff 邮箱（无锁） | 既有相位 | — | 不改 |
- **背压**：控制邮箱**有界**（每区 `sync_channel(K)` 或 `try_send` + 上限计数）。满了 ⇒ **拒绝入队
  并计入 `zone_control_overflow`（新 perf 列，标 additive）**；区 k 侧对该命令回一条可见失败
  （`SysMsg` 与区 0 普通背压同款文案），**不静默丢**。上限 K 由窗口实测定（先取 1024）。

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
- **判据**：N=1 两树对拍（a7）；N=2 时区 0 单跑与 N=1 的帧集合相同（多区不改变本区语义）。

## 5. 与 ⑤ 的接缝

- **区 k 成为发送者**：`cross_zone_frames::install` 目前只在区 0 安装（`ZoneHost.cross_mailboxes`+
  `router`）。M1 让每个区线程在启动时 `install(Arc::clone(mailboxes), Arc::clone(router), zone_id)`；
  `note_mirror_recipient` 的 `src_zone/seq` 由 `begin_tick(tick_no)`（区 k 也是 tick_no）自动成立。
- **分槽语义不变**：每 `(dst,src)` 槽单写者（区 k 只写自己的 src）；`src_tick < current tick` 的
  T+1 排空不变；`MAX_FRAMES_PER_SLOT` 拒绝新增不变。
- **`PhaseSet` 在区 k 生产模式**：`M1_MIN = M0_MIN | INBOUND | FRAMES | AOI_DOWNLINK | DEAL_TICK |
  RECOVERY | PERSIST(仅写 save_outbox)`。**每位一句「区 k 不跑它会坏什么」**：
  - `WORLD_HEAD`：不跑 ⇒ 本区无 tick head（怪物 AI/命令 FIFO/死亡结算全停）。
  - `WORLD_TAIL`：不跑 ⇒ 无 phase2（AOI 扫描）/死亡结算尾部，玩家永远看不到别人。
  - `INBOUND`：不跑 ⇒ 本区玩家的命令（移动/攻击/技能）无人执行。
  - `FRAMES`：不跑 ⇒ 本区产生的事件（动作/屠宰/骨骼/buff）不出帧。
  - `AOI_DOWNLINK`：不跑 ⇒ AOI 出现/消失与 52/53/体力帧不出（M1a 的核心）。
  - `DEAL_TICK`：不跑 ⇒ 交易自动取消不生效（会话卡在 Dealing）。
  - `RECOVERY`：不跑 ⇒ 恢复/体力/血量回复停摆。
  - `PERSIST`（仅 `save_outbox` 分支）：不跑 ⇒ 区 k 断线存档不进队列（数据丢失）。
  - **不含**：`DB_POLL`（进程级 DB 链在区 0）、`PERIODIC`（行会/城堡/公告是单点；**滚动公告**若
    需要在区 k 显示，M1c 再单独评估，届时加 `LINE_NOTICE` 子段而不是整段 `PERIODIC`）、`SWITCH`
    （换服单点）、`DIAG`（观测仅区 0）、`SLEEP`（区 k 节奏由 barriers 定，不 sleep）。
  逐片扩展：M1a `+AOI_DOWNLINK +FRAMES`（`M0_MIN` 先加这两段），M1b `+INBOUND`（战斗/技能命令），
  M1c `+DEAL_TICK +PERSIST`；每扩一位都要更新 `RAN_SEGMENTS` 的精确相等断言。
- **段集见证**：`RAN_SEGMENTS` 用例继续用；区 k 生产模式的段集 = 上述位集的**精确相等**断言。

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
2. **脚本宿主回 0 的锁序**：区 0 持区 k 锁 → 调用脚本 → 脚本可能递归调用世界操作 → 若再次取区 0
   的锁（`db`/`guilds`）可能死锁。规则：**持区 k 锁期间只做世界/会话修改与发帧，不取进程级锁**；
   需要 DB 的命令（存档类）走 `save_outbox` 的异步出口。用例：两线程互锁场景 `try_lock` 超时即红。
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

## 8. 实施顺序与可审的提交序列（opc-27 2026-11-23 定稿）

`M1-0`（结构）→ `M1a`（出帧）→ `M1b`（战斗/技能）→ `M1c`（物品/NPC）→ `收口`。
每片：一提交序列 + 全量门 + **N=1 两树对拍（最终树）**。

### 8.1 M1-0 的三个独立提交（缺一不可、顺序不可换）

| 提交 | 内容 | 判据 |
| --- | --- | --- |
| **M1-0a 纯格式**（单独提交，**零语义**） | 移除 `#[rustfmt::skip]` + `cargo fmt --all`（一次性格式化 `HostLoop::tick` 与守卫块缩进） | 提交前后 `git diff --ignore-all-space <parent> <this>` **为空**；FMT/CLIPPY/TEST 绿；**不改任何字面行内容** |
| **M1-0b 方法化 + 逐字字面量同步**（单独提交） | 段体拆成 `fn phase_*`；同时把被逐字扫描的字面行列出并同步给对应牙（`world.tick_tail(&mut rec);`、`drain_pk_flag_downlink(&mut world, &online, &gates);`、`let guild_run_tick = world.clock_ms as i32;`、`*_in_sim_loop`/`*_is_wired_in_sim_loop` 那批…完整清单在该提交信息里逐条列） | **不许 `#[ignore]`、不许把「精确匹配」改成「包含即可」、不许删牙**；每条牙改前先跑一次**旧断言**证明其仍能红（负例），改后全量绿；两条 `allow` 仅在字面量已改后移除，并在提交信息点名 |
| **M1-0c 结构** | `ZoneState { host: HostLoop, save_outbox, alive }` + `HostServices` 区位形态（零行为变化） | 全量门 + N=1 两树对拍 |

- **方法化不会截断扫描窗口**（方法行是缩进行、不匹配 `starts_with("fn ")`），但**会改逐字行**；
  这两件事在 M1-0a/M1-0b 里分开审。
- **M1-0 完成前不得开始 M1a**（opc-27 2026-11-23 放行条件）。
