# M1⑤ 载荷证据：全 Rust 边界链 + 200 假人登录/选角（报告）

- 卡：`openmir2-m1-boundary-load-200`（issue `d01477d5`）
- 身份：`openmir2-worker-15`；worktree `m1load`；分支 `worktree-m1-boundary-load`；base `f5170a8e`
- 链路表：`mir2-rs/docs/M1-边界服务移植计划.md` §1（DBSrv 用 **5100**，不是 6000）
- 工具：`tools/m1-acceptance/**`（本卡新增）
- 证据（入库，本目录 `evidence/`）：01…15；大部头原始输出在 `E:\tmp\openmir2-m1load\`（路径在各条下方）

## 0. 结论摘要（逐条）

| 判据 | 结果 |
| --- | --- |
| ① 全 Rust 边界链 + 端口所有权机判 | **GREEN** —— 7000/5500/7100/5100/5600 五端口监听者全判 `rust` 且入口与端口表一致（`01-ports-green.json`） |
| ① 内部链（16005 / 5600） | **5600（DBSrv→LoginSrv）活着**；**16005 两侧都不存在**（C# oracle 源码无 `LsvServer` 实现，见 §3.3） |
| ② 200 假人同时登录 | **成功率 100%**（BotSrv 200/200；Rust login-srv 侧 `[登录]`/`[选服]` 各 200） |
| ② 200 假人选角 | **BLOCKED** —— Rust `bins/db-srv` 从不置 `chr_queryed` ⇒ `CM_SELCHR` 恒被拒（协议探针 `select_chr 0/200`；db-srv 200 条 `Double send SELCHR`）。**已另开卡 `openmir2-m1-dbsrv-selchr-queryed`**，本卡不改 `bins/**` |
| ② 无 panic / 无未捕获异常 | **PASS** —— Rust 四进程 `panicked` 计数全 0；BotSrv `internal_errors=0` |
| ③ 内存曲线 + 末段斜率 | **PASS**（`load_report.py` 同一口径）：四进程末段斜率 0.000–0.001 MB/s（见 §5） |
| ④ 红检（换回 C# / 停掉 ⇒ 判据必红） | **PASS** —— 换回 C# SelGate ⇒ rc=1（`02`）；还原 ⇒ rc=0（`03`） |
| ⑤ C# 参考栈对照（报告值） | 同批 200 假人：BotSrv `login_ok` 175/200（87.5%，**工具侧漏计数**，见 §6）；协议探针四步 200/200 |
| ⑥ 四门禁 + 证据四件套 | **全绿**（fmt/clippy/test/msgcodegen rc=0；msgcodegen 604 常量） |
| ⑦ 起停栈留痕 + 端口回落 | **PASS** —— 起停日志 `start-boundary-stack*.log`/`stop-boundary-stack*.log`；收工十端口回落（仅共享 MySQL 3306 保留） |

> **一句话**：全 Rust 链起得来、200 假人登录 100%、曲线平、红检有牙、四门禁全绿；但**「选角」被 Rust db-srv 的一个既有缺陷挡住**，已按 brief 红线另开卡（本卡不顺手改 `bins/**`）。

## 1. 起链（影子配置，不动 `E:\MirServer`）

```powershell
cd mir2-rs; $env:CARGO_TARGET_DIR='E:\tmp\openmir2-m1load-target'
cargo build --offline -p login-gate -p login-srv -p sel-gate -p db-srv       # rc=0
powershell -File tools/m1-acceptance/start_boundary_stack.ps1               # BOUNDARY_STACK_OK
```

进程（`01-ports-green.json` 的命令行即证据）：

| 端口 | pid | 入口（Rust 二进制） | 配置（`E:\tmp` 影子副本） |
| --- | --- | --- | --- |
| 7000 | 29796 | `E:\tmp\openmir2-m1load-target\debug\login-gate.exe` | `…\stack\login-gate\config.conf` |
| 5500 / 5600 | 56028 | `…\login-srv.exe` | `…\stack\login-srv\logsrv.conf` |
| 7100 | 54060 | `…\sel-gate.exe` | `…\stack\sel-gate\config.conf` |
| 5100 | 46760 | `…\db-srv.exe` | `…\stack\db-srv\dbsvr.conf` |

影子配置：`LoginGate/SelGate` 的 `config.conf` 与 LoginSrv 的 `AddrTable.txt`（GB2312，含 `127.0.0.1:7100` 路由）+ `UserLimit.txt` **整体复制**；`logsrv.conf`/`dbsvr.conf`/`ServerAddr.txt`/`ServerInfo.txt`/`SelectID.txt`/`AddrTable.txt` 由脚本生成。
DBSrv 用 **5100** 面；**不照 6000 起栈**（6000 是 GameSvr 取档面，Rust 未实现）。

## 2. 端口所有权判据（机判，带牙齿）

`check_boundary_ports.ps1`：复用 `tools/m2-acceptance/portclassify.ps1` 的实现归属分类（`rust`/`csharp`/`unknown`），
另加「端口 → 期望 Rust 入口」第二层牙齿（防接错线）。

```
GREEN: 五端口（7000/5500/7100/5100/5600）的所有权均为期望的 Rust 入口（验收①可判）   # rc=0
```

入口逐条：`7000=login-gate`、`5500=login-srv`、`7100=sel-gate`、`5100=db-srv`、`5600=login-srv`，`verdict=ok`。

## 3. 链路活着（内部链）

### 3.1 DBSrv → LoginSrv（5600，DBSrv 主动连）

- `13-rust-db-srv.log`：`账号服务器[127.0.0.1:5600]链接成功.`
- `12-rust-login-srv.log`：`LegendOfMir2/99/0/DBServer/Online:0/正常`（每 20s 一次，`ServerIndex=99` 即 DBSrv）

### 3.2 SelGate ↔ DBSrv（5100）

- `14-rust-sel-gate.log`：`数据服务[127.0.0.1:5100]链接成功.` / `角色网关[0.0.0.0:7100]已启动...`
- `13-rust-db-srv.log`：`角色网关[1](127.0.0.1:61774)已打开...` / `玩家数据网关服务[0.0.0.0:5100]已启动.等待链接...`

### 3.3 SelGate ↔ LoginSrv（16005）——两侧都不存在（登记）

按 brief 要求核了 16005：`E:\MirServer\SelGate\config.conf` 有 `LsvServer=127.0.0.1` / `LsvServerPort=16005`，
但 **C# oracle 源码里没有任何 `LsvServer` 实现**：`grep -rin "lsv" src/SelGate src/LoginSrv` 为空，
`grep -rln "16005" src/` 为空；运行中的 C# 参考栈也不监听 16005（`netstat` 五个边界端口之外无 16005）。
⇒ 它是从原版 Mir2 带过来的**死配置**（`M1-边界服务移植计划.md` 表里本就标 `TODO(核实)`），
**不是 Rust 的缺口**，M1⑤ 不据此判红。**登记为「待 owner 确认是否从链路表删除」**。

## 4. 200 假人（BotSrv 载荷，登录 100%）

命令：

```powershell
powershell -File tools/m1-acceptance/run_m1_load.ps1 -Tier 200 -HoldSec 60 -SkipBuild
# 等价：tools/botload/load_gate.ps1 -Tiers 200 -HoldSec 60 -Criteria login,conn -RampTimeoutSec 120
```

BotSrv 侧（`07-rust-side-counts.json`：`E:\tmp\openmir2-m1load\evidence\rust-200\tier-200\bots.log`）：

| 计数 | 值 |
| --- | --- |
| 开始登陆 | 200 |
| 帐号登录成功（`LoadMetrics.LoginOk`） | **200 / 200 = 100%** |
| 查询角色. | 200 |
| 选择角色（发出 `CM_SELCHR`） | 200 |
| 准备进入游戏（收到 `SM_STARTPLAY(525)`） | **0** ← 选角被拒（见 §7） |
| `load_report.py` verdict | **GREEN**（`login_success_pct=100.0`、`conn_lost_steady=0`、ramp 5016ms） |
| 假人侧 `internal_errors` / `conn_refused` / `conn_lost` | **0 / 0 / 0** |

Rust 服务侧（`12-rust-login-srv.log`，探针复跑同批）：

| 计数 | 值 |
| --- | --- |
| login-srv `[登录] 帐号=` | **200** |
| login-srv `[选服] account=` | **200** |
| db-srv `[QueryChr] 候选` | **200** |
| db-srv `玩家获取游戏网关信息`（SM_STARTPLAY） | **0** |
| 四进程 `panicked` | **0 / 0 / 0 / 0** |

### 4.1 BotSrv 无关的协议探针（判「链本身」）

`probe_m1_chain.py`（200 并发、stagger 20ms、一条独立 TCP 走完四步）：

| 步骤 | 全 Rust 链 | C# 参考栈 |
| --- | --- | --- |
| `CM_IDPASSWORD`（7000） | 200/200 | 200/200 |
| `CM_SELECTSERVER`（7000） | 200/200 | 200/200 |
| `CM_QUERYCHR`（7100） | 200/200 | 200/200 |
| `CM_SELCHR`（7100） | **0/200** | **200/200** |
| ident 直方图 | `{520,529,530}`（无 525） | `{520,525,529,530}` |

⇒ 登录链 100%；C# 侧四步全绿证明**探针/序列正确**，Rust 侧选角 0 是**实现缺陷**。

## 5. 内存曲线（`load_report.py` 同一口径）

`run_m1_load.ps1` 在 load 期间另采 Rust 四进程（schema 同 `load_gate`），过滤到本线 pid 后用
`load_report.py summarize` 归算（`08-report-rust-clean.json`；原始序列 `11-rust-mem.ndjson`）：

| 进程 | ws_peak | ws_mean | priv_peak | **末段斜率** |
| --- | --- | --- | --- | --- |
| db-srv | 14.4 MB | 14.4 MB | 6.9 MB | **+0.001 MB/s** |
| login-gate | 8.9 MB | 8.8 MB | 4.2 MB | **0.000 MB/s** |
| login-srv | 12.2 MB | 12.1 MB | 4.2 MB | **+0.001 MB/s** |
| sel-gate | 9.0 MB | 8.8 MB | 4.2 MB | **0.000 MB/s** |

窗口 67.4s。⇒ 末段斜率 ≈ 0（M5「泄漏 vs 高水位」判据要的就是这份口径）。
（注：采样期机上有另一条车道的一次性 `login-srv` 测试进程，已按本线 pid 过滤，见 `11-rust-mem.ndjson` 与 `mem-rust-clean.ndjson` 的差别。）

## 6. C# 参考栈对照（报告值，非判据）

同一批 `loadbot0..199`、同一序列、同一 stagger=20ms，在 `E:\MirServer\start-all.ps1`（C# 部署栈：DBSrv/LoginSrv=仓库构建、GameSvr/三网关=`E:\MirServer`）上：

| 驱动 | 结果 |
| --- | --- |
| BotSrv | `login_ok` **175/200 = 87.5%**，verdict RED（爬坡未完成）；查询角色 175 / 选择角色 121 / 准备进入游戏 121；`internal_errors=1953`；ramp 峰值同样在 5016ms |
| BotSrv（另一轮，旧栈） | 170/200 = 85.0%，`internal_errors=9809` |
| 协议探针 | **四步 200/200**（含选角） |
| 端口所有权 | RED（7000/7100 判 csharp；5500/5100/5600 判 unknown——C# 构建输出不在 `E:\MirServer` 根下） |

**关键结论**：`BotSrv.LoadMetrics.login_ok` 在两个栈上都会**漏计数**（C# 175/200；这是卡
`openmir2-botsrv-world-evidence` 的已知问题：登录响应被假人侧跨假人共享态静默丢弃），
但**协议探针在同一台 C# 栈上 200/200** ⇒ 漏计数是**工具侧**、不是链侧。
本轮全 Rust 链上 BotSrv 恰好 200/200（`internal_errors=0`），比 C# 栈这一轮更好。

## 7. 红检（④）——判据不是摆设

`redcheck_boundary.ps1 -Mode swap-csharp-selgate`：

1. 基线：`GREEN` rc=0（`01`）
2. **只把 7100 一环换回 C#**（停 Rust `sel-gate`，起 `E:\MirServer\SelGate\SelGate.exe`）：其余四端口仍 Rust，
   `02-ports-red-swapped-csharp-selgate.json`：`7100 [csharp] … verdict=unknown` ⇒ **`RED` rc=1**
3. 停 C#、全栈重起：`03-ports-restored-green.json` ⇒ **`GREEN` rc=0**

（另：C# 参考栈在跑时同一判据也 `RED`——`04-ports-red-csharp-reference-stack.json`。）

红检原始输出：`E:\tmp\openmir2-m1load\evidence\redcheck-0-green.log`、`redcheck-1-swap-csharp-selgate.log`、`redcheck-2-restored.log`。

## 8. 发现的 Rust 侧真缺陷（已另开卡）

- **卡**：`openmir2-m1-dbsrv-selchr-queryed`（issue `2e5d3b08`）
- **缺陷**：`bins/db-srv/src/handler.rs` 的 `query_chr()` 成功返回时未置 `u.chr_queryed = true`；
  C# oracle `src/DBSrv/Services/Impl/UserService.cs:359` 有这一行。⇒ `dispatch` 的 `CM_SELCHR` 分支
  `if u.chr_queryed` 恒假，恒走 `else` → `[WRN] Double send SELCHR`，从不回 `SM_STARTPLAY(525)`。
- **为什么之前没被测出**：A2 交付时 `SELCHR` 未纳入字节对拍（A2 报告 §2.6 已如实登记）。
- **本卡处置**：按 brief 红线**不改 `bins/**`**，只在本卡登记（comment `6ea9562e`）+ 另开卡。
- **影响**：本卡 ② 的「登录」100%；「选角」须待该卡修复后复跑本目录命令方能判绿。

## 9. 四门禁（工作区根在 `mir2-rs/`，隔离 target）

```powershell
cd mir2-rs; $env:CARGO_TARGET_DIR='E:\tmp\openmir2-m1load-target'
cargo fmt --all -- --check                                   # rc=0
cargo clippy --workspace --all-targets --offline -- -D warnings   # rc=0
cargo test --workspace --offline                             # rc=0（全 ok）
cargo run -p msgcodegen --offline -- --check                 # rc=0：604 条常量与 C# 源一致
```

原始输出：`E:\tmp\openmir2-m1load\evidence\gates.log`（含 `*_RC=0` 与 `msgcodegen --check OK: 604 条`）。
本卡未新增 Rust 测试（只加 `tools/m1-acceptance/**` 脚本 + 证据），故不触发「新测试 ≥3 连跑」条款。

## 10. 复现命令汇总

```powershell
cd mir2-rs; $env:CARGO_TARGET_DIR='E:\tmp\openmir2-m1load-target'
cargo build --offline -p login-gate -p login-srv -p sel-gate -p db-srv
powershell -File tools/m1-acceptance/start_boundary_stack.ps1
powershell -File tools/m1-acceptance/check_boundary_ports.ps1                    # 期 GREEN rc=0
powershell -File tools/m1-acceptance/run_m1_load.ps1 -Tier 200 -HoldSec 60 -SkipBuild
python tools/m1-acceptance/probe_m1_chain.py --start 0 --count 200 --stagger-ms 20 # 期四步全 200
powershell -File tools/m1-acceptance/redcheck_boundary.ps1 -Mode swap-csharp-selgate
powershell -File tools/m1-acceptance/start_boundary_stack.ps1 -Stop
```

## 11. 残留物清单

| 路径 | 是什么 | 处置 |
| --- | --- | --- |
| `E:\tmp\openmir2-m1load\stack\` | 影子配置 + 四服务 stdout/err + pid 文件 | **保留**（报告引用；下次复跑直接复用/覆盖） |
| `E:\tmp\openmir2-m1load\evidence\` | 本轮全部原始输出（起停栈、端口实拍、红检、探针、内存、门禁、C# 对照） | **保留**（证据；入库副本见 `mir2-rs/tests/parity/m1-boundary-load/evidence/`） |
| `E:\tmp\openmir2-m1load-target\` | 隔离 cargo target（四个 bin） | 保留（复跑需要；删除会重编） |
| `E:\tmp\openmir2-m1load\probe200.py` | 过程件：先做的「只打登录」探针（诊断 BotSrv 漏计数用） | 保留；正式件是 `tools/m1-acceptance/probe_m1_chain.py` |
| `E:\MirServer\logs\` | C# 参考栈的 stdout（start-all 覆盖式写） | 非本线脚本产物；`E:\MirServer` 未改 |
| `E:\tmp\openmir2-m1load\evidence\rust-200\`、`csharp-200*\` | BotSrv 侧 `load_stats`/`bots.log`/`report.json`/内存 | 保留（报告引用） |
| 进程 | 全 Rust 链已 `-Stop`；C# 参考栈已 `start-all.ps1 -Stop`；MySQL 3306 保留（共享基础设施，开工前即在跑） | 十端口实况：3306 UP，其余 5100/5500/5600/5700/6000/7000/7100/7200/5000 全 DOWN |
