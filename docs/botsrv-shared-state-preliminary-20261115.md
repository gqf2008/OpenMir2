# BotSrv 共享态根因卡开工先导（严格空载 200/500 窗口复盘）

卡：`openmir2-botsrv-shared-state-locks`。
输入（只读归档，未起栈/未构建）：
- `E:/UserFleet/archive/botsrv-decode-clean-20261115/`：2026-11-15 14:00–14:30 严格空载窗口（heavy=0）。
- `E:/UserFleet/archive/gate-criticality-20261115-w83/`：criticality N=1/N=4 单档 500 对照（新栈）。

一句话：
1. **471/500 不是 29 个 bot 进世界失败**，是「最后 5s `load_stats` 采样截断 + 200→500 换档的离世广播风暴」。
2. **109ms P99 是 500 档进世界 ramp（65–75s）的尾**，不是稳态 500；稳态 500（criticality N=4）P99=187ms，真实环境只会更差 ⇒ 属「500 档余量」讨论，不是阈值舍入。
3. **200=6072 vs 500=0 不是「丢弃率把竞态压住」**：500 的 drop 率更高（47.3% vs 22.0%），但每 in-world bot-s 的已投递非关键帧更多（≈182 vs ≈128）；200 的 NRE 来自**稳态在世界的两线程重叠**（200×50s、低 CPU 29.6%），500 的 0 是**采样窗以 ramp 为主**（稳态 500 <5s、高 CPU 60.4%），且 criticality N=4 稳态 500 有 758 条 internal_errors ⇒ 500 的 0 不能当作竞态已消失。

## 1. `world_ok 471/500`（Q1）

### 1.1 到达曲线（tier-500 `load_stats.ndjson`，5s 粒度）

| uptime (s) | 0 | 40 | 45 | 50 | 55 | 60 | 65 | 70 | 75 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `in_world` | 0 | 0 | 49 | 127 | 203 | 274 | 343 | 409 | **471** |

- 进世界从 **45s** 才开始；60s 时只有 274；75s（最后一条 stats）471。
- `bots.log` 累计 `成功进入游戏` = **495**（14:01:36–14:02:52）；报告 `world_ok` 取的是**最后一条 `load_stats`（75.1s）**，不是最终累计。
- 4 个"未进"（`loadbot192/193/195/198`）在 game-svr 的 `进世界 ChrName:` 行出现在
  **06:02:52.5 UTC = 14:02:52.5**（最后 stats 之后 2.4s）；`loadbot1` 的角色名是 **`lb1a`**（日志名差异）。
  ⇒ **没有 29 个真实失败**；471 是采样截断。

### 1.2 进世界延迟（`enter_sent → in_world`）

| 轮次 | 首个 `enter_sent` | `in_world` 首/末 | 延迟 min / median / p90 / max | 末样本 in_world |
| --- | --- | --- | --- | --- |
| 本轮 500（200→500 同栈） | 14:01:36.237 | 14:02:17.069 / 14:02:52.162 | **39.4 / 49.7 / 69.0 / 73.0s** | 471（累计 495） |
| criticality N=4（单档新栈） | 11:06:09.987 | 11:06:10.241 / 11:06:46.782 | **0.25 / 11.7 / 23.5 / 26.9s** | 500/500 |

⇒ 本轮 500 的服务端进世界处理慢约 **4×**；不是 bot 没发 enter（所有 enter 都在 14:01:36–46 发出）。

### 1.3 原因归类

- 200 档在 14:01:33–35 结束，200 个 bot **同时断开**；game-svr 的 `离世广播` warning
  **62,750 条**（首条 `06:01:33.637` = 14:01:33.6，正好在 500 档 14:01:35 之前）⇒ 离世广播风暴
  占用服务端发送/世界 tick，500 的 `取角色进世界`/`进世界` 被整体推迟。
- `tools/botload/load_gate.ps1` 文档已警告：多档连跑不重启栈会拿被污染的栈测下一档；
  criticality 500/500 是**单档新栈**，没有 200 档离世风暴。
- **与 gate hold/丢弃的关系**：500 档 `hold/queued/flushed=6940/6940/6940`、
  `reject_login=0`、`close_inworld=0`、`overflow=0`；入口是关键帧，**全部补发成功**；
  `drop_noncritical=47.3%` 只影响 AOI。⇒ 不是 gate 丢入口帧，**也不是 tick P99**。
- 结论：**Q1 = 200→500 换档的离世广播风暴 + 60s 窗口/最后 5s 采样截断**；不是 29 个 bot 的真实失败。

## 2. `tick P99 109ms`（Q2）

### 2.1 分位与尾部

| 档 | samples | P50/P90/P99/max | >50ms | >100ms | >200ms |
| --- | --- | --- | --- | --- | --- |
| 200 | 11,269 | 0/16/**16**/78ms | 24（0.21%） | 0 | 0 |
| 500 | 8,091 | 0/16/**109**/141ms | 162（2.0%） | **111（1.37%）** | 0 |

- 500 的尾部集中在 **65–75s**：per-5s `tick_max` = 125 / 125 / 141ms；同期 `in_world` 343→471、
  CPU 60–85%（进世界 ramp + 离世风暴后的 500 填充）。
- 200 的 per-5s `tick_max` 从 30s 起稳定 78ms。

### 2.2 来源段（推断，附可复现口径）

- `#+GD` 延迟 = 服务端 action→send（World 线程 / `HostInbound` / `HostPeriodic`）+ gate 队列 + 客户端收包；
  本机环回，客户端/网络不是主项。
- 500 的 gate `max_depth_frames=224`（200 为 72）、drop 47.3%（200 22.0%）；111 条 >100ms 与
  进世界 ramp/离世风暴同段。
- **max 141ms < 200ms 服务端 tick**，尾部仍在单 tick 内 ⇒ 来自**服务端 host/world 段 + gate 队列**，
  不是 tick 跳帧/网络。**精确的 `HostInbound` / `HostPeriodic` / `World` 切分需要 host-segment
  profiler 的窗口，本窗口未开**（登记为后续，不在本卡凭推断改阈值）。
- 与 criticality N=4 稳态 500/500 对照：P99 **187ms**、max 391ms ⇒ **109ms 是短窗/部分 500 的数，
  稳态 500 更差**；这是「500 档余量」问题，不是 9ms 超阈的舍入。真实环境只会更差 ⇒ 不能按
  「109 略超」放行。

## 3. `200 recv_packet_null=6072` vs `500=0`（Q3）

### 3.1 到达帧量与丢弃率

| 档 | frames_decoded | AOI ident 41/10 | 非关键投递 / 丢弃 | drop% | gate max_depth | in-world bot-s | scene_rounds | CPU 均/峰 | NRE |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 200 | 1,315,720 | 650,042 / 650,099 | 1,284,624 / 361,582 | **22.0%** | 72 | ≈10,000（200×50s） | 190,194 | 29.6 / 44 | **6072** |
| 500 | 1,350,898 | 639,760 / 639,572 | 1,492,040 / 1,338,337 | **47.3%** | 224 | ≈8,200（45–75s ramp） | 254,501 | 60.4 / 85 | **0** |

- 500 的 drop 率更高，但**每 in-world bot-s 的已投递非关键帧** = 200: **128** / 500: **182**
  （500 更多）；所以「丢得多 ⇒ 到达帧少 ⇒ 竞态被压住」**不被数据支持**。
- `docs/gate-frame-criticality-20261115.md` 的勘误也写明：NRE 与丢帧量的相关性已证伪；
  根因是两线程对 `ActorList` / `MShare.MySelf` 的无锁并发。

### 3.2 为什么 200 反而更多

- 200：200 bot 在 15s 进世界后**稳态运行 50s**（≈10,000 bot-s），收包线程持续
  `SendMsg/NewActor/DeleteActor`，runThread 持续 `BeginScene` 迭代；CPU 均 29.6%，
  两线程以全速交错 ⇒ 竞态窗口大。
- 500：采样窗以 **45–75s 进世界 ramp** 为主，稳态 500 阶段 <5s；CPU 均 60.4%、峰 85%
  （服务端/夹具都在忙），收包线程逐帧处理与 runThread 迭代的相对时序改变；且多数 bot 只有
  10–30s 在世界。
- **关键反证**：criticality N=4 **稳态 500/500** 有 **758 条 internal_errors**（同族
  ActorList/BeginScene 竞态）⇒ **500 的 0 是窗口相位 artifact，不是竞态修好**。
  根因卡修完后，200 与「单档新栈、稳态 500」都必须复验。

## 4. 对根因实现的直接约束

1. **复验窗口**必须每档新栈（或只跑单档）且**等 500 全部进世界后的稳态**；否则 `world_ok` 与
   `recv_packet_null` 都被 ramp/离世风暴污染。200 档是本轮最好的竞态复现器（6072）；
   500 档的 0 不作为修好的证据。
2. 修 `ActorList` / `MShare.MySelf` 的锁/快照时，必须保持 per-bot `PlayScene` 的并行度
   （不做全局场景锁）；牙要能在 200 档并发注入下复现并归零。
3. Q2 的 `HostInbound` / `World` / `HostPeriodic` 切分需另开带 profiler 的窗口；本卡不据此改
   tick 阈值。

## 5. 复现命令（只读）

```sh
# 到达曲线 / 分位 / 逐类
python -I -B tools/botload/load_report.py summarize \
  --stats E:/UserFleet/archive/botsrv-decode-clean-20261115/tier-500/load_stats.ndjson \
  --mem E:/UserFleet/archive/botsrv-decode-clean-20261115/tier-500/mem.ndjson --count 500
# bot 入场时间 / 缺失名单
grep "成功进入游戏" E:/UserFleet/archive/botsrv-decode-clean-20261115/tier-500/bots.log
# 服务端进世界与离世风暴
grep "进世界 ChrName:\|离世广播" E:/UserFleet/archive/botsrv-decode-clean-20261115/stack-logs/game-svr.out.log
```
