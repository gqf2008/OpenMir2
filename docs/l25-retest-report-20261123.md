# L2-5 新 master 复测报告（2026-11-23）

基线身份：`3ac846e00771f47786268a0327739156f18d9679`（跨区帧六面 `e69274b6` + 索引迁移 `72a55d9d` + 日志闸/IDS 闸 `3ac846e0`）。本文把日志闸与索引迁移**分开记账**：日志闸只负责默认输出不变，索引迁移只负责 `chr_linear → 0`。

## 0. 二进制与构建状态

| 用途 | 二进制 | sha256 | 构建状态 |
| --- | --- | --- | --- |
| N=1 基线（`git archive 3ac846e0…` + 独立 target） | `game-svr.exe` | `974845786af1767a8287458e4fdc8fac9aba3c4d8dc1389d4831a9f04136a8c2` | 归档树构建，`--version rev=unknown dirty=unknown`（无 `.git`） |
| N=1 特性（本树同 master） | `game-svr.exe` | `50396d7192543435a095ce833d4afb30673acebbacd65e5f808f543ce4709f00` | 构建前 `git status --porcelain` 为空；`rev=3ac846e00771f47786268a0327739156f18d9679 dirty=0` |
| 2a/2b/步骤 3/4k 全部臂 | `mir2-server.exe` | `ec5b7b18ee4e903437dfa8dc94e0461d922a00a2b3077c898ed5f25f9d0b1505` | 构建前工作树为空；`rev=3ac846e0… dirty=0`；各臂 `stack-start.log` 的 `SERVER_EXE_SHA256` 同值 |

负载口径（除注明）：`MIR2_LOG_LEVEL=INFO`、`MIR2_TICK_PROFILE=1` + `MIR2_TICK_PROFILE_SLOW_MS=40`、1 区、`MIR2_BOT_BOUNDARY_WALK=1`、假人动作探针默认 1000ms。

## 1. 步骤 1：N=1 两树默认输出对拍

- 两树：`git archive 3ac846e0…` 独立 CARGO_TARGET_DIR 构建 vs 本树同 master 构建；`env` 全清（不设 `MIR2_LOG_LEVEL` 等），`Server.n1-1b.conf`、端口 5077、20s。
- 结果 **GREEN**：86/86 行、11980/11980 字节；两条 sha 与既有 golden 精确相等：
  - `order_sha = b78156a766b010332297c9475b08998a0918f27c97d2cecb639640121f690d24`
  - `multiset_sha = bd4b13bad3a1c1076cf5a62023e48c322caa41b2f73e75d603c39d96402c29fd`
- 复跑坑（必须记住）：conf 路径**分隔符原样进日志**（`config=`/`base=` 行）。golden 的历史口径是**正斜杠**；用反斜杠会得到另一对 sha（两树互相仍一致）。任何复跑必须 `-Conf E:/MirServer/...` 同形。
- 证据：`E:/UserTemp/l25-slice/n1-master-equiv-20261123-0301-3ac846e0/equiv_evidence.json`。

## 2. 步骤 3：1k/2k 120s 单线复测对账

| 指标 | 新 1k | 旧 1k | 新 2k* | 旧 2k* |
| --- | ---: | ---: | ---: | ---: |
| p50/p90/p99 (ms) | 47/78/109（GREEN） | 47/79/125 | 78/201/201（RED） | 63/172/201 |
| work（全窗 / 末 4 窗，ms/tick） | 28.79 / 41.25 | 34.54 / 48.88 | 38.68 / 68.98 | 45.55 / 87.03 |
| HostInbound（末 4 窗，ms/tick） | 13.04 | 19.79 | 22.29 | 35.39 |
| inbound wall（ms/tick） | 14.96 | 21.25 | 23.26 | 36.81 |
| `inbound_carry` 均值 | 0.05 | 0.06 | 0.27 | 0.57 |
| `chr_linear → chr_indexed`（末 200 tick） | **0 → 752,157** | 751,969 → 0 | **0 → 1,093,415** | 1,082,498 → 0 |

\* 2k 120s 臂 `in_world=1612`（ramp 未完）且 `window_valid=False`（HoldSec=120 < 360），**只作参考，不作稳态对比**；稳态 2k 结论以第 3 节的 420s 臂为准。

**索引迁移的交付证据（守恒）**：`chr_linear` 归 0，`chr_indexed` 补齐且量级一一对应（1k 751,969 ≈ 752,157；2k 1,082,498 ≈ 1,093,415）。此账记索引迁移，不记日志闸。

## 3. 步骤 2：双线臂（2k、420s、同二进制）

### 3.1 两行表

| 指标 | 单线 2a | 双线 2b gate0（7200） | 双线 2b gate1（7201） |
| --- | ---: | ---: | ---: |
| flushed 中位 / p90 / max / 窗口均值 / 整段均值（帧/s） | 844.2 / 2,209.9 / 2,419.6 / 1,096.1 / 1,059.4 | 422.2 / 665.4 / 1,001.1 / 436.6 / 419.8 | 362.2 / 586.8 / 691.6 / 367.0 / 353.1 |
| sent 中位 / p90 / max / 均值（帧/s） | 47,169 / 48,418 / 48,645 / 42,307 | 35,504 / 41,147 / 45,726 / 33,439 | 31,709 / 37,506 / 45,491 / 29,999 |
| drop 中位 / p90 / max / 均值（帧/s） | 1,103,743 / 1,150,042 / 1,341,189 / 884,763 | 424,058 / 556,289 / 639,762 / 370,452 | 381,245 / 498,950 / 568,454 / 330,302 |
| overflow / degrade / degraded_sessions | 33,679 / 4 / 1,849 | **0 / 0 / 0** | **0 / 0 / 0** |
| max_depth（上限 4096） | 4,096（撞顶） | 3,976 | 2,147 |
| timeout_flushes | 5,017 | 1,176 | 797 |
| 放行率 sent/(sent+drop) | 4.57% | 8.28% | 8.33% |
| p50/p90/p99（端到端，ms） | 201/201/201（RED） | **63/109/156（GREEN）** | 同左 |
| work / HostInbound / `inbound_carry` 均值 / wall | 115.31 / 44.02 / 0.85 / 42.52ms | 122.16 / 31.05 / 0.13 / 31.65ms | — |
| 进程 CPU | 110.1% | 115.2% | — |

产物：`E:/UserTemp/l25-slice/singleline-master-20261123-0324`（2a）、`E:/UserTemp/l25-slice/twogate-20261123-0315`（2b）。

### 3.2 归因与判读

- **归因干净**：同二进制单线 2a 完整复现旧基线（flushed 中位 844.2 vs 851.6、overflow 33,679、p50 落 201 桶）⇒ **索引迁移与日志闸都不是 2b 改善的来源，唯一变量是线数**。
- **成立**：双线各自独立且健康——两线都有量（分流 53/47），`overflow/degrade/degraded_sessions` 全 0，max_depth 4,096→3,976/2,147，p50 201→63。双线买到了**延迟与队头余量**。
- **不成立（负结果）**：双线**吞吐未翻倍**——各自 flushed 中位 422/362，**合计 784 ≈ 单线 844**；放行率从 4.57% 升到 8.28%/8.33% 只是把同一批量分给了两条线。
- 因此「天花板 = 每线放行速率、1 万路 = 加线/加进程」**仍是待验假设**，不得作为验收结论。
- **假设（非结论）**：关键帧产出速率可能由假人探针/回执需求决定，而非网关供给；用第 6 节实验一验。

### 3.3 配置三态留证（`ServerInfo.txt`）

| 态 | sha256 | 内容 |
| --- | --- | --- |
| 改前 | `7696ffb8fe0359c6f0cc87490e7aba46d7d4eca3069d07db0d5e9631001b8825` | `127.0.0.1 127.0.0.1 7200` |
| 改后 | `b053a391546e61ea3801b8e8218788fda54772e17178be9ff4e0a5617059b5d8` | `127.0.0.1 127.0.0.1 7200,7201` |
| 还原 | `7696ffb8fe0359c6f0cc87490e7aba46d7d4eca3069d07db0d5e9631001b8825`（`equals_before=True`） | `127.0.0.1 127.0.0.1 7200` |

单变量说明：本臂只改路由表到 2 线（不建第二个 game-gate 进程、不改 SelGate Count）；现网 `RunGate/config.conf` 本就 `Count=3`（一个 game-gate 进程 3 条线，7200/7201/7202，各线独立队列与独立账）。

## 4. 4k 单线：本机不可测（作废）

- 运行：4k 假人、单线 7200、420s、INFO、1 区；驱动脱离会话后台任务（分离进程），另挂内存闸每 5s 采样（阈值 2.5GB，**未触发**）。
- 结果：login 4000/4000（100%），但 **`world_ok=2928（73.2%）**；p50/p90/p99 = **201/201/201**；samples 153,981；verdict RED；`window_valid=True`。
- **BotSrv 进程内 OOM**：`internal_errors=44` 全在 steady（ramp=0），抽样 42 块全部 `System.OutOfMemoryException`（`bots.log` 714139 行起，03:56:43）；分布 `recv_packet_other=22 / run_round_other=14 / socket_handler_exception=6`；另 `socket_recv_fault=34`、`metric_flush_failure=2`。`partial_frames=8004`、`chunks_without_hash=4000`。
- 内存：系统可用最小 **4,654MB**（未到闸值）；BotSrv 工作集峰值 **4,430MB**（x64）；服务栈 6 进程 WS 峰值 4,620MB；结束后可用 13,522MB。⇒ **不是系统 OOM**。
- 判据影响：`world_ok` 73.2% + p99 全落 201 桶 ⇒ 本臂**不能用于吞吐/时延对比**；4k 双线未跑。卡：`capacity-4k-botsrv-oom`。

## 5. 缺口（本批复测暴露的前置阻塞）

1. **1 万验收的负载工具先阻塞**：BotSrv 在 4k 假人、进程 WS ≈4.4GB 时发生进程内 OOM（系统仍余 4.6GB），不是机器总量不足。负载工具不先解决，1 万验收无法开始。
2. 「加线是否加吞吐」未验证（4k 两臂未成）。

## 6. 后续实验方案（先不执行）

- **实验一：需求侧 vs 供给侧（2k + 提高探针频率）**。同一批 2000 假人，用 `load_gate.ps1 -ActionProbeMs <N>` 把动作探针间隔从默认 1000ms 调低（假人侧 env `MIR2_BOT_ACTION_PROBE_MS`；`MIR2_BOT_ACTION_PROBE=1` 默认开）——**不改产品源码**即可调。判读：若网关 flushed 明显高于 844 ⇒ 此前上限来自**需求侧**；若仍钉在 844 附近 ⇒ **供给侧**上限。此臂是**合成负载、非标准口径**，不进验收数字。
- **实验二：BotSrv 分片**（多进程各带一部分假人）作为 1 万验证工具的方向；先写进卡，不实施。

## 7. 残留物

产物目录：`E:/UserTemp/l25-slice/` 下 `n1-master-equiv-20261123-0301-3ac846e0`、`twogate-20261123-0315`、`singleline-master-20261123-0324`、`retest-1k-20261123-0333`、`retest-2k-20261123-0333`、`4k-single-20261123-0340`（作废）、`4k-single-retry-20261123-0349`（作废）。临时脚本（不在仓库）：`n1_master_equiv.ps1`、`gate_route_two.ps1`、`run_twogate_arm.ps1`、`mem_guard.ps1`、`launch_4k_retry.ps1`、`parse_gate_ledger.py`。配置已还原（见 3.3）。
