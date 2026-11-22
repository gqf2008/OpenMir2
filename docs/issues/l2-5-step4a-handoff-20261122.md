# L2-5 ④a 交接：区 k 命令处理（2026-11-22）

> **交接边界**：本文档**只交接、不动代码**。撰写会话不建 ④a 分支、不改产品源码。
> 新会话从 **master `341f7e401`** 起分支 **`card/l25-step4a-zone-cmds`**（worktree 由总监指定；
> 若沿用本车道，用现有 `zone-mirror-step2` 身份，不要复制密钥）。
> 本车道现状：分支 `card/l25-s4-capacity-smoke` tip（见卡尾 patch），栈已停、进程 0、工作树干净。

## 0. 为什么现在是 ④a

段 4 真机结论（计划页 §12.3）：**真客户端跨界 handoff 目前无法端到端触发**——两处叠加：
①切片模式下**非 0 区线程只处理 `Closed`，其余 `GateEvent` 一律丢弃**；
②区 0 多 span（321 张未提交图默认归区 0）⇒ `handoff::bounds_for(区 0)` 返回 `None`，区 0 不设边界。
区 k 玩家因此**连走位命令都到不了世界**（夹具已验证：`MIR2_MOVE` 在区 0 侧 3928 条 `moved=true`，
区 1 侧 0 条）。④a 只做 ①；②留给 ④c。

## 1. 缺口与入口（全符号锚，不写行号）

1. **缺口**：`spawn_minimal_zone` 的区 k 线程循环把 `Closed` 收进 `closed_events`，其余事件丢弃——
   原话是「其余命令/网关线事件仍丢弃（命令处理属段 2）」。`Command` 必须在**同一套 tick 结构**
   内由区 k 线程处理。
2. **区 0 的处理入口（照抄对象）**：`sim_loop` 的 `GateEvent::Command` 派发 → `handle_move_cmd`
   （`CM_TURN`/`CM_WALK`/`CM_RUN`/`CM_HORSERUN`；`nX/nY = LoWord/HiWord(Recog)` = **目的地**、
   `Tag` = 方向）以及同一派发里的其它命令分支。区 k 要复用**同一份函数**，不要抄第二份。
3. **句柄归属（计划页 §8 写死）**：`world`/`displays`/`online`/`ground_sessions`/`saves`/
   `login_tails`/`aoi_frames_total` 属**落地区**（`ZoneState`，区线程持锁）；
   `gates`/`engine`/`guilds`/`npc_host`/`pwd_sessions`/`reconnect_pending`/`kick_pending`
   属**进程**、由区 0 线程借用。**唯一先例** = 登录落区：区 0 线程持 `ZoneState` 锁 +
   `finish_login_on_zone_state`。④a 的难点就是把这个先例推广到「区 k 线程处理 Command」。
4. **`slicing=0` 逐字节不变**：现有 demux 只在切片时路由；N=1 路径**不得**引入任何新分支/新锁。
   判据 = `n1_stdout_equivalence`（两棵树 stdout sha 相等）+ `cargo test --workspace` +
   `l2_two_zone` + 工具门全绿。
5. **观测缝**：`MIR2_LOG_MOVES=1`（`test_seams::move_log_enabled`）⇒ 每收一条走位命令打印
   `MIR2_MOVE ident=… dir=… req=(x,y) from=(x,y) moved=… rolled_back=… after=…`。这是 ④a
   最直接的验收读数；段 4 的基线是「区 0 侧 3928 条 moved=true / 区 1 侧 0 条」。

## 2. ④a 完成定义（可机器判）

- **A1 区 k 真处理命令**：2 区 + 边界走位夹具（`MIR2_BOT_BOUNDARY_WALK=1`）下，`MIR2_MOVE` 行
  出现 `from=(x ≥ 区 1 分界)` 且 `moved=true`（即区 k 会话的走位命令到达世界）。
- **A2 N=1 不变**：`python -I -B tools/hygiene/n1_stdout_equivalence.ps1 -Selftest` 之外，
  实跑两棵树 stdout 归一化 sha **相等**；`cargo test --workspace`、`l2_two_zone`、工具门 rc=0。
- **A3 第一次真跨界**：2 区 1k + 边界走位（`HoldSec ≥ 360`）⇒ 段 4 的 perf TSV
  `handoff: offered>0、accepted>0、promoted+inserted_fresh>0`（方向区 1→区 0），
  `refused==0`、`dup_dropped` 量级可报；tick p50/p99 与 `region_barrier_us` 一并报。
- **A4 分母口径更新**：`cross_recipient_missing` 摘要必须带本轮窗口（ticks/时长）与
  `refmsg_targets` 分母（口径见计划页 §12.4A），并与 ④a 前的基线对照。
- **A5 门**：`cargo fmt --all -- --check` / `clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace` / `cargo test -p game-svr --test l2_two_zone` /
  `msgcodegen -- --check` / `docs_anchor_gate --prose --base <合并目标>` / 工具门
  `--only perf_tsv_summary` 全 rc=0。

## 3. 先查什么（按可能性排序）

1. **命令处理的时序位置**：区 k 的 `tick_top`/phase1/barrier1/phase2/barrier2 与命令处理该放哪、
   与区 0 `sim_loop` 的派发点对齐（会不会影响 AOI/快照定序与零闪烁判据）。
2. **进程级句柄的借法**：逐项核对 §8 的归属表；`save_outbox`/踢人/DB 请求等副作用哪些留在区 0
   （登录落区先例：回执完成后才 `ZoneRouter::assign`）。
3. **锁纪律**：`ZoneState` 锁与进程句柄的既有锁的获取顺序；**屏障等待期间不得持锁**（§8 明文），
   加任何新锁前先画锁序图，避免跨区死锁。
4. **`slicing=0` 的零参与**：demux 的路由开关、`Command` 分支的准入条件；N=1 走原路径。
5. **夹具可复现**：本车道已备：`tools/botload/run_l25_slice.ps1`（起栈/压测/摘要）、
   `MIR2_BOT_BOUNDARY_WALK*`（真 `CM_WALK`，`BoundaryWalkTick`）、108 个 `0.map` 已验证可走格
   （`E:/UserTemp/walkable_cells.txt`）、DB 铺位与回滚依据 `E:/UserTemp/l25-slice/db_chars_before.tsv`。

## 4. 已知会踩的坑

- **不要为取句柄新增全局锁**：会与 `ZoneState` 锁形成跨区死锁；沿用区 0 的持有者模型。
- **屏障等待期不持锁**（`await_neighbors`/`after_phase2`）。
- **`slicing=0` 不引入新分支**：N=1 判据会当场抓。
- **命令处理复用同一函数**（`handle_move_cmd` 等），不要抄第二份——否则行为分叉。
- **perf TSV 合计列**曾因二次 `swap` 恒写 0（已修）；改 writer 时别回退。
- **走位夹具默认关**；跨界的窗口前提三条（计划页 §12.3 末尾）：range 覆盖分界、源区允许发起
  handoff、该区线程处理 Command。

## 5. 证据与产物（本车道留给新会话）

| 项 | 路径 |
| --- | --- |
| 段 4 容量轮（含近边界三臂） | `E:/UserTemp/l25-slice/{1z-20261122-175230,2z-20261122-175527,2z-20261122-182056,2z-20261122-183911,2z-20261122-185934,2z-20261122-190833}/` |
| 可走格清单 | `E:/UserTemp/walkable_cells.txt`（108 格，含 `x/y`） |
| DB 改前快照（唯一回滚依据） | `E:/UserTemp/l25-slice/db_chars_before.tsv`（1973 行 × 10 列） |
| 段 4 记录 | 计划页 §12.1–§12.4 |
| 本车道分支 | `card/l25-s4-capacity-smoke`（tip 见卡尾 patch） |

## 6. 交接边界

- 撰写会话**不动代码、不建 ④a 分支**；④a 由新会话从 `341f7e401` 起 `card/l25-step4a-zone-cmds`。
- 段 4 卡保持待审/已合状态；④a 的验收在 ④a 自己的卡上写，不沿用本卡。
