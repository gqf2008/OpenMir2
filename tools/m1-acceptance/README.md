# tools/m1-acceptance/ —— M1⑤「全 Rust 边界链 + 200 假人登录/选角」验收工具

对应设计文档 §6 **M1 验收第 5 项**（N=200 假人同时登录+选角：成功率 100%、无 panic、曲线平稳），
也是 M5「灰度」的第一步（逐服务替换后的最小可跑链）。

> 背景（为什么不是 `tools/botload` / `tools/e2e`）：C6 的载荷 gate 起的是 `E:\MirServer\start-all.ps1`
> （**C# 部署栈**），它的 tier-200 没有一个字节经过 Rust 边界服务 ⇒ M1⑤ 需要一条**全 Rust** 的链。
> 本目录只做这件事；假人编排仍**只读复用** `tools/botload/load_gate.ps1`。

## 1. 链路与端口（`mir2-rs/docs/M1-边界服务移植计划.md` §1 链路表）

| 端口 | 进程（Rust 入口） | 链路 |
| --- | --- | --- |
| 7000 | `bins/login-gate` | 客户端 ↔ LoginGate |
| 5500 | `bins/login-srv` | LoginGate ↔ LoginSrv |
| 7100 | `bins/sel-gate` | 客户端 ↔ SelGate |
| 5100 | `bins/db-srv` | SelGate ↔ DBSrv（`UserService` 面：角色列表/建删选角） |
| 5600 | `bins/login-srv` | DBSrv → LoginSrv（会话同步，监听方是 LoginSrv） |

**不含 6000**：那是 DBSrv 的 `DataService` 面（GameSvr 取档），Rust 侧尚未实现
（卡 `openmir2-m1-dbsrv-6000-face`）。也不含 5000/7200（GameSvr/RunGate，M2 线在改）。

> **16005（`LsvServer`）**：`E:\MirServer\SelGate\config.conf` 里有 `LsvServerPort=16005`，
> 但 **C# oracle 源码（`src/SelGate/**`、`src/LoginSrv/**`）里根本没有 `LsvServer` 实现**
> （`grep -rin lsv src/` 为空；运行中的 C# 栈也不监听 16005）⇒ 它是遗留死配置，两侧都没这条链。
> 本目录因此不对 16005 立项（登记在报告里）。

## 2. 脚本

| 文件 | 作用 |
| --- | --- |
| `start_boundary_stack.ps1` | 起/停全 Rust 边界链。影子配置写到 `-StackRoot`（默认 `E:\tmp\openmir2-m1load\stack`），**不碰 `E:\MirServer`**；GB2312 的 `AddrTable.txt`/`UserLimit.txt` 整体复制。`-Stop` 只停本脚本起的进程（不碰 MySQL）；缺 pid 文件 ⇒ `NOTHING_TO_STOP`。缺 exe/配置 ⇒ rc=2；端口被占 ⇒ rc=1。 |
| `check_boundary_ports.ps1` | 五个端口的所有权实拍。复用 `tools/m2-acceptance/portclassify.ps1` 的 `Get-M2PortImpl`（rust/csharp/unknown），并加**第二层牙齿**（端口 → 期望 Rust 入口）：不是 Rust 或接错线 ⇒ rc=1；端口不在听 ⇒ rc=2；全绿 rc=0。证据写 `-OutFile`。 |
| `run_m1_load.ps1` | 对全 Rust 链跑一层假人（默认 200）：只读复用 `load_gate.ps1`（`-Criteria login,conn`），期间另采 Rust 四进程 WS/私存/CPU（schema 同 load_gate），再用 `tools/botload/load_report.py summarize`（**同一口径**）归算出 `report-rust.json`（含 `slope_tail_mb_per_s`）；并统计 BotSrv/Rust 两侧日志计数与 panic。 |
| `redcheck_boundary.ps1` | 红检：`-Mode swap-csharp-selgate` 停 Rust `sel-gate` 改起 C# `SelGate` ⇒ 判据必须 rc=1；`-Mode stop-login-srv` 停 `login-srv` ⇒ 必须 rc=2。再全栈重起 ⇒ 必须复绿。 |
| `probe_m1_chain.py` | **BotSrv 无关**的协议级假人：N 并发走完整 `CM_IDPASSWORD(7000) → CM_SELECTSERVER(7000) → CM_QUERYCHR(7100) → CM_SELCHR(7100)`，逐步计数 + ident 直方图。用于把「链能不能吃住 N 并发」与「BotSrv 工具侧漏计数」分开。 |

### 2.1 为什么要 `probe_m1_chain.py`（与 BotSrv 的分工）

本工程实测：`BotSrv.LoadMetrics.login_ok` 会**系统性漏计数**（C6 卡 `openmir2-botsrv-world-evidence`：
登录响应被假人侧跨假人共享态静默丢弃）。实测数据（同一台机、同一批 `loadbot0..199`、stagger=20ms）：

| 驱动 | 栈 | BotSrv `login_ok` | 协议探针四步 |
| --- | --- | --- | --- |
| BotSrv | C# 参考栈 | 175/200（87.5%） | — |
| BotSrv | 全 Rust 链 | **200/200** | — |
| probe | C# 参考栈 | — | 200/200/200/**200** |
| probe | 全 Rust 链 | — | 200/200/200/**0** ← Rust db-srv 缺陷，见 §4 |

⇒ 判「链本身」用协议探针（确定性、无共享态）；BotSrv 的两侧计数作为**载荷**证据并列给出。

## 3. 用法

```powershell
# 0) 构建（隔离 target）
cd mir2-rs; $env:CARGO_TARGET_DIR='E:\tmp\openmir2-m1load-target'
cargo build --offline -p login-gate -p login-srv -p sel-gate -p db-srv

# 1) 起链（端口必须先空）→ 2) 端口所有权判据
powershell -File tools/m1-acceptance/start_boundary_stack.ps1
powershell -File tools/m1-acceptance/check_boundary_ports.ps1

# 3) 200 假人（BotSrv 载荷）+ Rust 侧计数 + 内存曲线
powershell -File tools/m1-acceptance/run_m1_load.ps1 -Tier 200 -HoldSec 60 -SkipBuild

# 4) 红检（弄坏必红、还原必绿）
powershell -File tools/m1-acceptance/redcheck_boundary.ps1 -Mode swap-csharp-selgate

# 5) 全栈停（端口回落）
powershell -File tools/m1-acceptance/start_boundary_stack.ps1 -Stop
```

## 4. 已知阻塞（本轮发现，另开卡）

Rust `bins/db-srv` 的 `query_chr()` 成功后**没有**置 `u.chr_queryed = true`
（C# oracle `src/DBSrv/Services/Impl/UserService.cs:359` 有这一行）⇒ `CM_SELCHR` 恒被拒
（`[WRN] Double send SELCHR`），选角**完全不通**。缺陷卡：`openmir2-m1-dbsrv-selchr-queryed`。
本目录不改 `bins/**`（写集红线）。
