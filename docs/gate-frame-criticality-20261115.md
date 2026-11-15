# 网关帧关键性分类（两条通道）：AOI 帧可丢可计，关键帧溢出不许关「已在玩」的会话

- 卡：`openmir2-gateway-frame-criticality-queue`（tier=R，`window_required=true`）
- 总监裁决：2026-11-15（卡片线程三条 comment：两条通道 / 计数与牙 / 关键帧溢出也不许关已进世界）
- 前序：`docs/gate-backpressure-a-b-20261115.md`（A+B 有界排队 + 溢出拒绝，第一轮 500 档：
  overflow 96,309 帧、**497 个会话被 `GM_CLOSE`**，其中 429 个已在世界）
- 代码：`mir2-rs/bins/game-svr/src/gate_criticality.rs`（分类）、`gate_backpressure.rs`（计数/日志）、
  `lib.rs`（`send_gate_msg` / `send_noncritical` / `queue_or_reject`）

## 1. 结论（一句话）

下行帧按「丢了会不会把会话废掉」分两条通道：**关键帧**进有界 FIFO，溢出时未进世界 ⇒ 拒绝登录、
已进世界 ⇒ **只降级不关会话**；**非关键 AOI 帧**完全绕开队列，收不了立即丢弃并计数。
`close_inworld`（对已进世界会话发 `GM_CLOSE`）是结构上的 0，并作为可观测不变量输出。

## 2. 分类清单与依据（逐条 oracle）

判据只看两件事：① 这帧是不是**一次性状态**（丢一次就永久不同步）；② oracle 里它是否由
**AOI 广播/动作中继**产生且会被后续帧覆盖。依据是 oracle 的 `RM_* → SM_*` 映射
（`src/M2Server/Player/PlayObject.Message.cs`）与广播点 `SendRefMsg`
（`src/M2Server/Actor/BaseObject.Message.cs`，±`SendRefMsgRange` 格内广播；口径链见
`docs/M2-S3世界下行-AOI同屏-20261011.md` §2）。

### 2.1 非关键（AOI 位置/动作/状态；`gate_criticality::NON_CRITICAL_IDENTS`）

| wire ident | SM_* | oracle 产生点 | 覆盖来源 |
| --- | --- | --- | --- |
| 4 | `SM_41` | `RM_41`，`PlayObject.Message.cs:1503-1509` | 动作序列继续推进 |
| 5 | 线上 `SM_HORSERUN` | `RM_HORSERUN`，`:1413-1422`；线上 ident 见 D-15 | 下一步位置覆盖 |
| 6 | `SM_RUSH` | `RM_RUSH`，`:1526, 1536-1541` | 冲刺动作 + 下一步位置 |
| 7 | `SM_RUSHKUNG` | `RM_RUSHKUNG`，`:1527, 1538-1541` | 同上 |
| 8 | `SM_FIREHIT` | `RM_FIREHIT`，`:1489-1495` | 攻击动作动画 |
| 9 | `SM_BACKSTEP` | `RM_PUSH`，`:1525, 1533-1535` | 后退动作 + 下一步位置 |
| 10 | `SM_TURN` | `RM_TURN`，`:1524-1560` | 每次转身/AOI 重现重发 |
| 11 | `SM_WALK` | `RM_WALK`，`:1403-1412` | 下一步位置覆盖 |
| 13 | `SM_RUN` | `RM_RUN`，`:1423-1432` | 下一步位置覆盖 |
| 14 | `SM_HIT` | `RM_HIT`，`:1433-1439` | 攻击动作动画 |
| 15 | `SM_HEAVYHIT` | `RM_HEAVYHIT`，`:1440-1446` | 攻击动作动画 |
| 16 | `SM_BIGHIT` | `RM_BIGHIT`，`:1447-1453` | 攻击动作动画 |
| 17 | `SM_SPELL` | `RM_SPELL`，`:1454-1460` | 施法动作动画 |
| 18 | `SM_POWERHIT` | `RM_SPELL2`，`:1461-1467` | 攻击动作动画 |
| 19 | `SM_LONGHIT` | `RM_LONGHIT`，`:1475-1481` | 攻击动作动画 |
| 20 | `SM_DIGUP` | `RM_DIGUP`，`:1891-1898` | 动作动画 |
| 21 | `SM_DIGDOWN` | `RM_DIGDOWN`，`:1900-1903` | 动作动画 |
| 22 | `SM_FLYAXE` | `RM_FLYAXE`，`:1904-1915` | 飞斧动作动画 |
| 23 | `SM_LIGHTING` | `RM_LIGHTING`，`:1916-1934` | 闪电动作动画 |
| 24 | `SM_WIDEHIT` | `RM_WIDEHIT`，`:1482-1488` | 攻击动作动画 |
| 25 | `SM_CRSHIT` | `RM_CRSHIT`，`:1496-1502` | 攻击动作动画 |
| 26 | `SM_TWINHIT` | `RM_TWINHIT`，`:1510-1516` | 攻击动作动画 |
| 28 | `SM_MOVEFAIL` | `RM_MOVEFAIL`，`:1468-1474` | 下一次移动帧覆盖 |
| 31 | `SM_STRUCK` | `RM_STRUCK`，`:1562-1620` | 飘字瞬时；HP 由 53 覆盖 |
| 41 | `SM_FEATURECHANGED` | `RM_TURN`，`:1552-1560`；另 `RM_FEATURECHANGED`，`:1762-1764` | 每次转身重发 |
| 43 | `SM_43` | `RM_43`，`:1517-1523` | 动作序列继续 |
| 53 | `SM_HEALTHSPELLCHANGED` | `RM_HEALTHSPELLCHANGED`，`:1736-1739`；产生点 `BaseObject.cs:700-710` | 每次 HP/MP 变化重发；`SendUpdateMsg` 去重只留最新（`BaseObject.Message.cs:283-304`） |
| 657 | `SM_CHARSTATUSCHANGED` | `RM_CHARSTATUSCHANGED`，`:1845-1847`；产生点 `BaseObject.cs:1516` / `PlayObject.cs:2876` | 每次状态变化重发；`SendUpdateMsg` 去重 |

### 2.2 关键（默认；未知 ident 一律关键）

- **登录/会话建立**：`SM_LOGON(50)`、`SM_NEWMAP(51)`、`SM_ABILITY(52)`、`SM_SUBABILITY(752)`、
  `SM_SERVERUSERINDEX`（`GM_*` 控制帧）、`SM_VERSION_FAIL(5008)`、`SM_MAPDESCRIPTION(54)`、
  `SM_DAYCHANGING(46)`、`SM_SENDNOTICE(658)`、`SM_SERVERCONFIG(11029)`、`SM_GAMEGOLDNAME(1106)`。
- **实体存在性/持久状态跃迁**：`SM_ALIVE(27)`、`SM_HIDE(29)`、`SM_DISAPPEAR(30)`、`SM_DEATH(32)`、
  `SM_SKELETON(33)`、`SM_NOWDEATH(34)`、`SM_USERNAME(42)`、`SM_CLEAROBJECTS(633)`、
  `SM_CHANGEMAP(634)`、`SM_ITEMSHOW(610)/SM_ITEMHIDE(611)`、`SM_BUTCH(637)`、
  `SM_CHANGELIGHT(654)`、`SM_CHANGENAMECOLOR(656)`、`SM_SPACEMOVE_HIDE/SHOW(800/801, 806/807)`。
- **背包/装备/经济**：`SM_ADDITEM(200)`、`SM_BAGITEMS(201)`、`SM_DELITEM(202)`、
  `SM_DELITEMS(203 邻域)`、`SM_UPDATEITEM(203)`、`SM_SENDUSEITEMS(621)`、`SM_WEIGHTCHANGED(622)`、
  `SM_DURACHANGE(642)`、`SM_GOLDCHANGED(653)`、交易/商店/修理/仓库存取等回复帧。
- **对话/菜单/聊天/系统/行会/组队**：`SM_MENU_OK`、`SM_QUERYVALUE(215)`、`SM_MERCHANTSAY(643)`、
  `SM_HEAR(40)`、`SM_SYSMESSAGE(100)`、`SM_GROUPMESSAGE(101)`、`SM_GUILDMESSAGE(104)`、
  `SM_GROUPMEMBERS(667)` 等。

判定理由只有一条：这些帧是「**一次性状态**」，客户端没有第二次机会从后续帧自愈；丢了就是永久
不同步（或会话不可用）。所以它们进队列、按阶段分流，绝不静默丢。

### 2.3 为什么分类只认 `GM_DATA` 的内层命令头

外层 `ServerMessage.ident` 的 `GM_*` 管理号（`GM_CLOSE=2`、`GM_CHECKSERVER=3`、`GM_CHECKCLIENT=4`、
`GM_DATA=5`、`GM_SERVERUSERINDEX=6`、`GM_RECEIVE_OK=7`、`GM_STOP=8`）与 `SM_*` 前几个值**数值相撞**。
因此 `wire_identity` 增加 `inner_command` 标志：只有「`GM_DATA` 且 `PackLength >= 12`」的帧才按内层
`SM_*` 分类；控制帧/短包一律关键（否则 `GM_SERVERUSERINDEX(6)` 会被当成 `SM_RUSH(6)` 丢掉）。

## 3. 两条通道的实现口径

- **非关键通道**（`send_noncritical`）：
  - 关键帧队列非空 ⇒ **丢弃**（`why=CriticalPending`）。理由：直发会越过积压、破坏同会话顺序
    （`SM_LOGON` 还在队列里时直发同会话的 `SM_TURN`）；等价于「收不了」。
  - `SendChecked > 0` 且未超 50ms ⇒ **丢弃**（`why=SendChecked>0`）。
  - 已超 50ms ⇒ 按 oracle 清零（这一包不再被压住；能发就发）。
  - 否则直发（沿用 oracle 的 `CheckBlock*10` 阈值与 `SendBlockCount` 记账；单包超阈值仍按
    oracle `:76-79` 丢弃并计数）。
  - **永不入队、永不关会话**；`drop_noncritical` / `noncritical_sent` 分别计数。
- **关键通道**（`send_gate_msg` → `queue_or_reject`）：
  - 队列未满 ⇒ FIFO 入队，`GM_RECEIVE_OK`/50ms 超时后按序补发（A+B 原行为不变）。
  - 队列满且**未进世界** ⇒ `reject_login` 计数 + 对该会话发 `GM_CLOSE`（明确拒绝，客户端可见断开；
    同一连接内按会话去重）。
  - 队列满且**已进世界** ⇒ `degrade_inworld` 计数 + 每会话降级标记；**不关会话**、不扩容、不清空
    队列，后续靠既有回显/超时路径继续补发。丢弃的是溢出那一刻的关键帧（有界策略：队列容量仍是
    唯一内存上界，降级标记集合上限 4096 会话）。
  - **`close_inworld` 目标 0**：`GM_CLOSE` 只在上面「未进世界」分支写；`note_close_sent(in_world)`
    在真正写出 `GM_CLOSE` 时记账，若有人删掉降级分支、让已进世界会话走到关闭路径，这个计数就会涨。
- **会话释放清残留（总监 P3 F2，与 D-18 SessionId 撞号同病）**：队列存的是完整 wire（外层头带
  `SessionId`），补发按 FIFO 不按会话；网关的 `SessionId` 是 4 位随机数，在 ≤50ms 队列窗口内**可能
  被新会话复用**。不清 ⇒ 旧会话的残留帧投给新会话（玩家收到别人数据、无法察觉、日志无异常）。
  落地：`GM_CLOSE`（按 socket 关，释放前读该槽的 `SessionId`）、认证失败、世界侧 `KickUser` 三条
  释放路径都调用 `purge_session_residue`（`PendingQueue::retain` 按外层 `session_id` 过滤、字节计数
  重算）。**不用「禁用 id 复用」**：复用是网关侧行为（`GM_OPEN` 头里带 id），服务端无权阻止。

## 4. 计数、日志与回退

`event=gate_backpressure_summary`（≤1 条/5s/gate）与 `gate_stats.ndjson` 新增：

| 字段 | 含义 |
| --- | --- |
| `drop_noncritical_frames` / `drop_noncritical_bytes` | 非关键帧收不了而丢弃（**永不关会话**） |
| `noncritical_sent_frames` | 非关键帧直发成功（`drop + sent` = 非关键通道收到总数） |
| `reject_login_frames` | 关键帧溢出且**未进世界** ⇒ 拒绝登录 |
| `degrade_inworld_frames` / `degraded_sessions` | 关键帧溢出且**已进世界** ⇒ 降级（帧数 / 会话数） |
| `close_inworld_sessions` | 对已进世界会话真正发过 `GM_CLOSE` 的次数（**不变量：0**） |
| `overflow_frames` | 关键帧溢出总数（= `reject_login + degrade_inworld`） |

逐帧日志：`event=gate_frame_drop reason=NonCritical why=…`、`event=gate_frame_overflow stage=login
close=…`、`event=gate_frame_degrade stage=in_world close=none`，各自独立限速（≤1 条/秒/gate）。
失败面直方图 `ident/session` 同时覆盖 reject/degrade/oversize/drop_noncritical（每类 48 键上限）。

**通道开关与回退**：`MIR2_GATE_SEND_QUEUE=0`（或任一容量 0）⇒ 队列禁用，关键帧按 oracle「直接丢」
（仍计数/日志）。非关键通道在队列禁用时行为不变（它本来就不排队）。整卡可整体 revert（无协议变化、
无数据迁移）。**内存上界口径（写清）**：`256KiB × 20 条 gate 线 = 5MiB` 是 **wire 字节口径**——队列
存完整 `ServerMessage` wire，`PendingQueue.bytes` 累加的是 `wire.len()`（**含每帧 20B 头 + body**）；
按 payload 口径算会略小（每帧少 20B），所以 5MiB 是偏保守的上界。另加每会话降级标记（≤4096 个
`u16` ≈ 8KiB/gate），与 A+B 同量级。

**已知口径（写清、有界）**：已进世界的降级是「丢这一帧」而不是「把队列撑大」——若关键帧持续溢出，
同一会话会持续丢关键帧；这是有意的有界策略（踢人换成丢帧），计数与日志逐帧可见。

## 5. 牙与变异

| 牙 | 用例 | 断言 |
| --- | --- | --- |
| ①非关键拥塞直接丢 + 计数 | `noncritical_frames_bypass_queue_and_never_close` | 5 个 AOI ident 全 `NonCriticalDrop`；`drop_noncritical=5`；`pending.len()=0`；`queued=0`；网关侧静默（无 `GM_CLOSE`）；拥塞解除后同类帧直发 |
| ②关键溢出 + 未进世界 | `critical_overflow_login_stage_rejects_explicitly` | `reject_login=1`、`rejected_sessions=1`、网关收到 `GM_CLOSE(session=43)`、`close_inworld=0`、同会话去重 |
| ③关键溢出 + 已进世界 | `critical_overflow_inworld_degrades_without_close` | `degrade_inworld=1`、`degraded_sessions=1`、网关静默（无 `GM_CLOSE`）、队列深度不增；回显后积压帧照常补发、同会话下一帧仍直发（会话存活） |
| ④删通道分支必红 | 变异重放（见下） | 删非关键分支 ⇒ ①③红；删已进世界降级分支 ⇒ ③红且 `close_inworld` 涨 |
| ⑤下一 tick 覆盖 | `noncritical_drop_is_superseded_by_next_tick_frame` | 丢弃一帧后，拥塞解除的下一帧位置更新正常送达 |
| 顺序守卫 | `noncritical_dropped_when_critical_pending` | 关键帧积压时 AOI 帧不插队（`why=CriticalPending`），积压 `SM_LOGON` 仍先到 |
| **SessionId 复用误投**（总监 P3 F2） | `released_session_residue_is_purged_before_id_reuse` | A 断（`GM_CLOSE`）⇒ 队列里 A 的残留被清；B 复用同一 `SessionId` ⇒ 客户端第一帧是 B 的 `SM_ADDITEM`，A 的 `SM_ABILITY` 绝不再出现 |
| 分类集合 | `gate_criticality::tests::*` | 非关键集合逐值相等 + 升序；持久/登录/控制帧恒关键；`GM_*` 撞号不入非关键 |

变异重放（本人 worktree，逐条还原并核 sha 一致 + 无残留）：

（交付时补：删 `send_gate_msg` 的非关键分支 / 删 `queue_or_reject` 的 `in_world` 降级分支 /
删非关键的 `pending.is_empty()` 顺序守卫 —— 每条对应哪个用例红。）

## 6. 真机 500 档（R 级窗口）

四组必报：`close_inworld`（目标 0）、非关键丢弃计数、`in_world`、延迟分位（延迟与丢弃必须同时报，
只报一个会得出相反结论）。

（交付时补：窗口排期、栈版本、run 目录、四组数字、与 A+B run1/run2 的对照。）

## 7. 残留 / 已知限制

- **超时补发臂的紧循环风险（总监 P3 F2 附带项，本批登记、未修）**：`gate_conn` 的 `select!` 超时臂
  在 `send_checked==0 && 队列非空` 时把 `flush_deadline` 算成「立即」，若 `flush_pending` 的写 socket
  失败（链路已坏），它只打 WRN 就 `continue`，下一次 select 的 deadline 仍是 now ⇒ 可能短时紧循环
  （烧 CPU 直到读错误收掉连接）。后续修法：写失败时置一个退避标志（如 `flush_backoff_until =
  now + 50ms`）或给连续失败计数上限，超过就按连接错误退出。**本批不改**，因为改动落在 A+B 的
  超时补发语义上，需单独卡评审。
- 非关键通道的「收不了」把「关键帧积压」也算进去（顺序守卫）；因此在关键帧洪峰期间 AOI 帧会被
  系统性丢弃——这是有意的（优先保关键帧的顺序与完整）。
- 分类是**按 ident 的静态白名单**，未知 ident 默认关键；未来新增 AOI 类帧需要同步登记
  （模块文档 + 集合测试会逼着改）。
- `SM_STRUCK(31)` 的自身 HP 显示依赖 53 的后续刷新；53 与 31 都丢时 HP 条会短暂陈旧（不崩、不误判死亡）。
- 直方图 top 列表仍是「先到先得」的有界键集合（不是 top-k），逐帧日志才是归因通道。
