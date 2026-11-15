# 网关背压策略 A+B：有界排队 + 溢出拒绝（卡 `openmir2-gateway-backpressure-frame-drop`）

> 总监裁决（2026-11-15）：基础 **A 有界排队（256KiB/gate，可配）**，溢出落 **B（拒绝 + 计数 + 明确失败）**。
> 与 oracle「`SendChecked>0` 直接丢」的偏离登记为 **D-20**（`mir2-rs/tests/parity/whitelist.md`）；
> 本文件记录设计、配置、失败语义、回退与验证口径。

## 1. 背景

oracle `ChannelMessageHandler.ProcessBufferSend`（`src/M2Server/Net/ChannelMessageHandler.cs`）在
`SendChecked > 0`（等网关回 `GM_RECEIVE_OK`）时**无条件 `return`**：帧直接丢、不排队、不计入
`SendCount/SendBlockCount`、无任何日志。2026-11-15 03:00 窗口实测 716 条 `SM_LOGON` 里
330 Send / **386 Hold**，bot 侧 `in_world=330` 与 Send 数完全相等——386 个会话因背压永远进不了世界。
这与 D-18（SessionId 撞号）同类：oracle 在低负载下正确、高负载下错误。

## 2. 行为

| 场景 | oracle | Rust（本批） |
| --- | --- | --- |
| `SendChecked > 0`（未超时） | 帧丢弃 | **A**：按 FIFO 入 per-gate 有界队列，等清零点补发 |
| `SendChecked > 0` 且已超 50ms | 清零后**这一帧仍丢** | 入队后**立即**按序补发（超时也计入 `timeout_flushes`） |
| 队列已满（字节或帧数） | 不存在（无队列） | **B**：拒绝该帧 + 计数 + 日志 + 对该会话发 `GM_CLOSE`（明确失败） |
| 单包就超 `CheckBlock*10` | 丢弃 | **保持 oracle**：丢弃，但计数 + 日志（这种帧在流控前提下发不出去） |
| 累积撞 `CheckBlock*10` | 先发 `GM_RECEIVE_OK` 再发正文 | 同 oracle（补发批次每批最多撞一次，然后等回显） |

### 顺序硬约束

**flush 只在 `SendChecked` 清零后、任何新帧直发之前完成。** 否则共享 FIFO 会让同一会话的
`SM_SERVERUSERINDEX → SM_LOGON` 乱序——乱序比丢帧更难查，因为不报错。落地方式：

- `Hold` 帧先入队；若 `send_decision` 已因超时清零，立即补发（本帧排在积压之后）；
- `Send`/`CheckFirst` 分支若队列非空，**新帧也先入队**，再由 `flush_pending` 统一按 FIFO 写出；
  只有队列为空时才走直发路径；
- `dispatch` 的 `GM_RECEIVE_OK`（清零点）与 `GM_OPEN`（直发 `GM_SERVERUSERINDEX` 前）先尝试 flush；
- `gate_conn` 的 `select!` 带一个超时分支：队列非空且等回显时，最迟在
  `SendCheckTick + SocCheckTimeOut` 主动清零并补发（oracle 只在「下一次发送尝试」清零，
  洪峰末尾的最后一帧会无限期滞留）。

## 3. 配置（全部可选，默认零副作用）

| 环境变量 | 默认 | 含义 |
| --- | --- | --- |
| `MIR2_GATE_SEND_QUEUE` | 开 | `0`/`false`/`off`/`no` ⇒ 关闭队列，回退 oracle「直接丢」（仍计数 + 日志） |
| `MIR2_GATE_QUEUE_BYTES` | `262144`（256 KiB/gate） | per-gate 队列字节上限；`0` ⇒ 等价关闭 |
| `MIR2_GATE_QUEUE_FRAMES` | `4096` | per-gate 队列帧数上限；`0` ⇒ 等价关闭 |
| `MIR2_GATE_STATS_OUT` | 未设 | 设为目录 ⇒ 追加写 `<dir>/gate_stats.ndjson`（累计值，含绝对 `wall_ms`） |

非法值回落默认、不 panic。解析在 `mir2-rs/bins/game-svr/src/gate_queue.rs` 的 `from_env` /
`parse_flag` / `parse_usize`。

## 4. 内存上界与失败语义

- **内存上界**：256 KiB × 20 条 gate 线 = **5 MiB**（`GATE_SLOT_CAPACITY` = 20）；帧数上限
  4096/gate 防止「大量小帧」只按字节统计撑爆计数。最坏情况由两个上限同时约束。
- **失败语义（B）**：被拒帧**永不投递**（不重试、不假装成功）；该会话收到一次 `GM_CLOSE`
  （同一连接内按 `SessionId` 去重），网关摘除会话路由、关闭写端 ⇒ **客户端看到连接被服务端关闭**，
  不是静默丢帧。写 `GM_CLOSE` 失败也计数（`close=failed`），不静默。
- **可观测**：`event=gate_frame_queued` / `event=gate_frame_overflow` / `event=gate_frame_drop`
  各自限速（≤1 条/秒/gate）；5s 周期汇总 `event=gate_backpressure_summary` 含入队/补发/溢出/
  单包超阈值计数、`max_depth_frames/bytes`、队列容量口径与 gate/ident/session top 列表；
  NDJSON 与 BotSrv `load_stats.ndjson` 同口径（相邻两行差分 = 窗口增量）。
- **回退**：`MIR2_GATE_SEND_QUEUE=0` 或任一容量设 0 ⇒ 队列禁用、`Hold` 帧按 oracle 直接丢，
  但仍保留计数/日志；代码可整体 revert（无数据迁移、无协议变化）。

## 5. 验证（牙与门禁）

| 判据 | 用例 |
| --- | --- |
| ①顺序：先 INDEX 后 LOGON，断言到达顺序 | `backpressure_flush_preserves_index_then_logon_order`（走 `dispatch(GM_OPEN)` → `send_gate_msg` → `dispatch(GM_RECEIVE_OK)` 生产路径）；补强：`new_frame_is_queued_behind_pending_instead_of_direct_sent` |
| ②溢出：队列满 ⇒ 计数 +1、网关侧收到 `GM_CLOSE`（不是静默） | `queue_overflow_counts_and_sends_explicit_gm_close`（真实 socket 对；含同会话去重） |
| ③删掉排队逻辑 ⇒ 必红 | 变异重放：删 `queue_or_reject` 入队分支 / 删 overflow 的 `GM_CLOSE` / 删撞阈值的 `GM_RECEIVE_OK` / 把「队列非空先入队」改成直发 |
| 队列边界 | `gate_queue` 模块 6 条（FIFO、字节闭区间、帧数上限、禁用、0 字节、env 解析） |
| 统计/日志 | `gate_backpressure` 模块 12 条（入队/溢出/回退丢/单包超阈、三类独立限速、直方图上限、NDJSON 形态、无事件无终值） |
| 超时 liveness | `stale_send_checked_flushes_queued_frame_on_timeout` |
| 补发批次流控 | `flush_stops_at_threshold_and_waits_for_echo` |

门禁：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
`cargo test --workspace --no-fail-fast`、`cargo run -p msgcodegen -- --check`、
`python -I -B tools/hygiene/docs_anchor_gate.py --prose --base <合并目标 ref>`，
外加 whitelist 锚门 / registry audit / `t_reservation_gate check`。

## 6. 真机对照（R 级窗口，2026-11-15）

两轮 500 档（同一四缝二进制、同一 BotSrv 构建；栈 `MIR2_LOG_MISDELIVER=1` + `MIR2_GATE_STATS_OUT`）：

| 指标 | 基线（旧实现） | run1（默认 4096 帧） | run2（env `MIR2_GATE_QUEUE_FRAMES=16384`） |
| --- | --- | --- | --- |
| login / no_resp / internal_errors / OOM | 100% / 0 / 0 / 0 | 100% / 0 / 0 / 0 | 100% / 0 / **25** / 0 |
| in_world / 未进世界 | 283/500（217） | **432/500（68）** | **427/500（73）** |
| SM_LOGON 静默 Hold 丢（`event=gate_frame_drop`） | 123 | **0** | **0** |
| queued / flushed | — | 260,839 / 260,838 | 304,575 / 304,574 |
| overflow / rejected_sessions | — | 96,309 / 497 | 112,419 / 500 |
| max_depth | — | 4,096 帧 / 177,908B | 6,942 帧 / **262,144B**（字节上限绑定） |
| tick 稳态 p50 / ≥201ms 占比 / max | 0 / — / — | 201 / 51.8% / 13,266ms | **31** / 47.8% / 8,641ms |
| GM_CLOSE：已进世界后被关 / 从未进世界 | — | 429 / **68** | 427 / **73** |

- **主观测同向**：静默 Hold 丢 123 → 0；进世界 283 → 432/427。A+B 生效。
- **tick 口径**：`+GD` 盖戳之后的**投递延迟**（不是 tick 间隔/工作耗时）。两轮稳态均双峰：约一半
  在 0–16ms，约一半落进「≥201ms」封顶桶（桶内含至 8.6–13.3s 的尾部）。把帧上限放到字节上限
  之后，中位 201→31ms，但 ≥201ms 尾部仍在 ~48%、overflow 反而上升 ⇒ 4096 帧上限是「中位被压进
  201 桶」的放大器，不是尾部根因；**A+B 是「用延迟换不丢」**。
- **B 的代价**：两轮 overflow 帧 ≥98% 是 AOI/广播帧（41/10/657/11/14），`SM_LOGON` ≤0.5%；
  run2 等于 500/500 会话都被服务端 `GM_CLOSE` 过至少一次，73 个新人被挡在门外。run2 的 25 条
  `internal_errors` 是 BotSrv 在服务端 `GM_CLOSE` 后的 teardown `NullReferenceException`。
  ⇒ 后续方向：`openmir2-gateway-frame-criticality-queue`（关键帧/广播帧分桶），并重裁「非关键帧
  溢出是否应关闭会话」。
- run2/run3 只做 env 实验；**代码默认值保持 4096**，是否改默认由总监看结果另定。
- **run3 对照（`MIR2_GATE_SEND_QUEUE=0`，同二进制）**：tick p99 **31ms**、max 188ms、稳态 ≥201ms 占比 **0%**，
  但 SM_LOGON Hold 210 帧全部丢弃、全部 Hold 帧丢弃 4,300,320、in_world 回到 **290/500**（≈基线 283）。
  ⇒ **201ms 尾部是队列造成的**；A+B 是「用延迟换不丢」。run3 的 `internal_errors=772` 集中在单个 bot
  （BotSrv 兜住未崩），与 run2 的 25 条同族（teardown/丢帧触发的 NRE）。
- 单网关吞吐上界（1 个在途 SendChecked / 50ms 超时 / 每周期 122,880B）≈ **2.46MB/s ≈ 62k 帧/s**；
  run3 实测 offered 1.43MB/s / 38.1k 帧/s（58–62% 上界）。按每进世界会话 ~131 帧/s 外推：500 档已≈上界，
  1000 档 2.1×、10k 档 21× ⇒ 单网关不足，需流水线化/分网关（协议级决策，本卡只给数）。
- 1000 档对照（当前修复前进世界 425/1000）等大内存窗口另批。
- 归档：`E:/UserFleet/archive/gate-ab-20261115-w83/`（run1 根目录、run2 `run2-frames16384/`）。
- 窗口纪律：内存 60s 一记、可用 <3GB 停栈、不并发 cargo、收尾端口全 DOWN、mysqld 不动。

## 7. 已知限制（不阻塞本批）

- 直方图 top 列表是「先到先得」的有界键集合，不是 top-k；**逐帧日志才是逐帧归因通道**，
  top 列表只给趋势（reviewer-26 F2）。
- `gate_stats.ndjson` 无轮转/大小上限，仅建议窗口期开启（reviewer-26 F5）。
- 计数生命周期 = 单条 gate 连接（重连归零，按 gate 线身份口径）。
- 被拒会话的 `GM_CLOSE` 依赖网关既有 `GM_CLOSE` 分支摘除路由；oracle 网关对该帧不做处理
  （本批 R 级偏离的一部分，已在 D-20 写明）。
- 某会话溢出被关闭时，它在队列里**更早的帧不会从队列删除**：补发时网关按 `SessionId` 找不到
  会话会丢弃并打 WRN（不重开、不投递），不会造成帧序问题，只是多一次无主下发。
- **后续裁决（2026-11-15，总监）**：关键帧溢出也**不许**关闭「已进世界」的会话；未进世界 ⇒
  明确拒绝登录 + 计数，已进世界 ⇒ 不关、保留队列/降级。本批 B 仍按原裁决（溢出即关闭）交付，
  该失败语义的变更由 `openmir2-gateway-frame-criticality-queue` 落地；真机 run2 的
  `rejected_sessions 500/500` 即该裁决的反例。
