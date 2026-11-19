# openmir2-gateway-degraded-sessions-cap

> 状态：**只读定性完成（2026-11-19，分支 `card/gateway-degraded-sessions-cap`）**——未改任何代码/参数。
> 触发：10k-ab2 四轮实测 `degraded_sessions` 全部打满 4,096 且"长期不恢复"。
> 本文回答 opc-07 的四问；结论供决定"调容量 / 改准入策略 / 改观测口径"。

## 重要更正（opc-07 先前误读，后续不得重复）

**"打满 4,096 且长期不恢复"不是产品级缺陷，而是两个单调计数器 + 上限截断造成的「指标假象」。**
- `degraded_sessions` 是**去重集合的容量上限截断**（`MAX_CLOSED_SESSIONS=4096`，从不清理）；
- `max_depth_frames` 是**队列帧上限的历史高水位**（只升不降）。
两者共同表现为"打满不回落"，与真实的会话/队列健康度无关；判断降级是否恢复需要新增的当前值 gauge
（见"次要观察"与后续观测卡）。

## 一句话结论

**两个 4,096 都是"上限常量"，不是负载饱和的即时读数**：
- `degraded_sessions=4096` 是**去重会话集合被自己的容量上限截断**（`MAX_CLOSED_SESSIONS=4096`，
  `mir2-rs/bins/game-svr/src/gate_backpressure.rs` 的 `claim_session_degrade`），该集合**从不清理** ⇒ 一旦有 4096 个不同会话
  各降级过一次，计数就**永久钉在 4096**；
- `max_depth_frames=4096` 是**队列帧数上限**（`mir2-rs/bins/game-svr/src/gate_queue.rs`（第 49 行）的 `DEFAULT_QUEUE_FRAMES`，配套
  同文件（第 46 行）的 `DEFAULT_QUEUE_BYTES=256 KiB`），字段是**历史高水位**（`max(...)` 累计），只升不降。

所以四轮"全部打满、长期不恢复"是**指标口径**的必然结果；真实行为是**队列在约 60–70 s 内被填满，
之后一直以饱和吞吐运行**（`queued≈flushed`，末值差 ≤27 帧），溢出持续发生。

## 四问逐条

### 1) `degraded_sessions` 的语义

- 定义：`DropStats.degraded_sessions: HashSet<u16>`；对外只暴露 `len()`
  （`mir2-rs/bins/game-svr/src/gate_backpressure.rs` 的 `degraded_sessions()`）。是**"累计进入过降级的会话数（去重）"**，
  **不是**"当前处于降级的会话数"（当前值没有暴露，也没有清理点——全文件无 `remove/clear/retain`）。
- 容量：`claim_session_degrade` 里 `if len() < MAX_CLOSED_SESSIONS(4096) { insert }` ⇒ 第 4096 个之后
  **不再记录**，但**仍照常降级**（丢该帧、会话继续存活）；日志里 `degraded_sessions=4096` 不会再变。
- 小瑕疵：`claim_session_degrade` 在集合已满、实际未插入时**仍返回 `true`** ⇒ 逐帧日志的
  `degraded_session=1` 可能声称"首次标记"而实际没记上。属观测口径问题，不影响丢弃/存活行为。
- 到 4,096 后的**新会话**：其行为与之前一致——关键帧队列满时，未进世界 ⇒ 拒绝登录（计入
  `rejected_sessions`/`reject_login_frames`，发 `GM_CLOSE`）；已进世界 ⇒ 丢帧 + 降级（不关会话）。
  它们只是不再进入这个已满的集合。

### 2) `max_depth_frames=4096` 的单位与出处

- **单位 = 队列中的帧数**（per gate 线）。字段是**历史高水位**：每个 note 点
  `max_depth_frames = max(max_depth_frames, depth_frames)`，只增不减。
- 上限常量：`DEFAULT_QUEUE_FRAMES = 4096`、`DEFAULT_QUEUE_BYTES = 256 * 1024`
  （`mir2-rs/bins/game-svr/src/gate_queue.rs:46,49`）；`GATE_SLOT_CAPACITY=20` ⇒ 进程级上界
  约 4096×20 帧 / 5 MiB。模块头注释给出的**唯一书面理由**是"256 KiB/gate ≥ 触发阈值
  `CheckBlock*10 ≈ 120 KB` 的 2 倍"；**帧数上限没有独立推导**，只是与字节上限配对的第二道界
  （256KiB/4096 ≈ 64 B/帧量级）。

### 3) 打满后是否恢复

- **指标本身不可能回落**：`degraded_sessions`（去重集合）与 `max_depth_frames`（高水位）都是单调量。
  四轮日志时间序列（`logs/game-svr.out.log` 的 `event=gate_backpressure_summary`，每 5 s 一条，
  87–94 条/轮）程序化验证：两者**全程单调**，`degraded_sessions` 在约 60–70 s 时到 4096 后直到
  结束（约 500 s）不再变化。
- **队列本身一直在流动**（不是死锁/卡死）：`queued_frames - flushed_frames` 的末值差
  1/2/29/147（四轮），活跃深度在 0…≈cap 间振荡；溢出是**产能 > 排放速率**的结果，不是队列停摆。
- 摘录（ctrl#1，同一轮内）：

```
08:50:50 uptime=49791  degraded_sessions=3167 max_depth_frames=4096 overflow_frames=264089
08:51:05 uptime=64847  degraded_sessions=4096 max_depth_frames=4096 overflow_frames=656883
08:55:42 uptime=341306 degraded_sessions=4096 max_depth_frames=4096 overflow_frames=3386009
08:58:19 uptime=498280 degraded_sessions=4096 max_depth_frames=4096 overflow_frames=5266441
```

### 4) 与 `rejected_sessions` / `reject_login_frames` 的因果

- **先满后退**，不是先拒后满：`max_depth_frames` 在 uptime≈50 s 已是 4096（队列饱和），
  `degraded_sessions` 在 60–70 s 到顶；`rejected_sessions` 从 0 起持续增长到登录阶段结束
  （ctrl#1 在 uptime≈220 s 停在 1,181 / `reject_login_frames=19,942`，此后不再增长）。
- 同一次"队列满"事件按**会话阶段**分流（设计如此）：未进世界 ⇒ 拒绝登录 + `GM_CLOSE`（1,181–1,790
  个会话，与客户端断开数逐轮相等）；已进世界 ⇒ 只降级。四轮 `close_inworld_sessions=0` 不变量成立。
- 量级：`degrade_inworld_frames` 4.39M–6.21M vs `reject_login_frames` 19.9k–32.7k ⇒ **99.4–99.6%
  的溢出帧来自已进世界会话**；`overflow_frames ≈ degrade + reject_login`（逐轮核对一致）。
  另：`drop_noncritical_frames` 62–80M（**AOI 非关键帧、独立通道、按设计可丢**，先于队列直接丢弃）——它不是事故，不要与关键帧溢出混读。

## 次要观察（改观测口径时一并处理）

- 两个 4096 撞值容易误读：一个是**队列帧上限**，一个是**降级会话集合上限**（`MAX_CLOSED_SESSIONS`
  也用于 `closed_sessions` 去重）。建议其中至少一个改名（`degraded_sessions_total_capped`）。
- 活跃深度（`queued−flushed`）在个别采样上出现 4097–4144（略超 4096），疑似计数时点/入队与上限判定
  的边界或跨 gate 汇总口径问题，建议改观测时一并核（不影响本轮定性）。
- 当前没有任何"当前降级会话数"的读数；要判断"是否恢复"，需要新增可清理的 gauge（会话关闭/重连时移除）。

## 结论：**不扩容**（opc-07 2026-11-19 裁定，写死）

理由（本卡证据）：
1. 帧上限与字节上限**同时打满**（`max_depth_frames=4096`、`max_depth_bytes≈262,1xx/262,144`）⇒
   单纯调高任一界，只会把饱和点推迟到更高负载，10k 下 hold 速率 ≫ flush 速率不会变；
2. 溢出的 **99.4–99.6% 来自已进世界会话**，按设计只降级、不关会话（`close_inworld_sessions=0`），
   属"非关键负载按设计丢弃"，扩容量是在为**不该保住**的帧付内存；
3. 真正的杠杆是**降低 hold 产生速率**——AOI 增量（10k 实测把单 tick AOI 成本从 3.4× 预算压到 0.7×）
   已经在做；其后是"增量成为默认路径"的评估（另议）。

保留的不变量：登录阶段明确拒绝（`GM_CLOSE`）/ 已进世界只降级、绝不关会话。

## 建议方向（定性，待 opc-07 决定；本卡未改任何东西）

1. **观测口径**（低风险）：把 `degraded_sessions` 改为可回落 gauge 或改名标 `_total_capped`；
   修 `claim_session_degrade` 满集合时的返回值；汇总行补当前队列深度与 per-stage 溢出速率。
2. **容量**（改参数，需评估内存）：当前帧上限/字节上限同时打满，说明两界都生效；单纯加权只推迟饱和
   （10k 下 hold 速率远高于 flush）。若加，需同构建两轮验证内存与 AOI 成本。
3. **准入**（策略）：登录阶段拒绝是"明确失败"，已进世界只降级——这条不变量应保留；若要减少
   `rejected_sessions`，方向是降低 hold 产生速率（AOI 增量已在做）或对登录做排队/限流，
   而不是扩容到能容纳峰值。

## 方法与边界

- 方法：读 `mir2-rs/bins/game-svr/src/gate_backpressure.rs`/`gate_queue.rs`/`lib.rs` 的计数与队列代码；解析四轮
  `logs/game-svr.out.log` 的 5 s 汇总时间序列（只读）。
- 边界：未改代码、未改参数、未起任何服务；结论只定性。
