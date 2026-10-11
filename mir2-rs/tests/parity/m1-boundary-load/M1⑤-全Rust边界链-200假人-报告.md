# M1⑤ 载荷证据：全 Rust 边界链 + 200 假人登录/选角（报告）

- 卡：`openmir2-m1-boundary-load-200`（issue `d01477d5`）
- 身份：`openmir2-worker-15`；worktree `m1load`；分支 `worktree-m1-boundary-load`
- **本片 base：交付时 `refs/heads/master` = `b3a40814`**（含 selchr 修复 `c95e14c8`，由 `5db28d8d` 合入）。证据所跑的四条边界 bin 的源码在 `60095f65..b3a40814` 之间**零改动**（`git diff --stat 60095f65..b3a40814 -- mir2-rs/bins/{db-srv,login-srv,login-gate,sel-gate} mir2-rs/crates` 为空）
- 链路表：`mir2-rs/docs/M1-边界服务移植计划.md` §1（DBSrv 用 **5100**，不是 6000）
- 工具：`tools/m1-acceptance/**`（本卡新增；计数单一口径 = `count_m1_logs.py`）
- 证据（**全部入库**，本目录 `evidence/`；下表路径均为相对仓库根的可点相对路径）：`01…26` + `SHA256SUMS.txt`（26 件，`sha256sum -c` 全 OK）；`evidence/.gitattributes` = `* -text` **冻结字节**，保证清单在任意检出上都成立

## 0. 结论摘要（逐条）

| 判据 | 结果 |
| --- | --- |
| ① 全 Rust 边界链 + 端口所有权机判 | **GREEN** —— 7000/5500/7100/5100/5600 五端口监听者全判 `rust` 且入口与端口表一致（`01-ports-green.json`） |
| ① 内部链（16005 / 5600） | **5600（DBSrv→LoginSrv）活着**；**16005 两侧都不存在**（C# oracle 源码无 `LsvServer` 实现，见 §3.3） |
| ② 200 假人同时登录 | **成功率 100%**（BotSrv `login_ok` 200/200；Rust login-srv 侧 `[登录]`/`[选服]` 各 200） |
| ② 200 假人选角 | **GREEN** —— 协议探针 `select_chr=200/200`、`ident_hist` 含 `525`；Rust db-srv 侧 `玩家获取游戏网关信息`（SM_STARTPLAY）=200（`05`/`07`/`13`）。selchr 缺陷已修复并合入 master |
| ② 无 panic / 无未捕获异常 | **PASS** —— Rust 四进程 `panicked` 计数全 0；BotSrv `internal_errors=0` |
| ③ 内存曲线 + 末段斜率 | **PASS**（`load_report.py` 同一口径）：四进程末段斜率 -0.011–+0.002 MB/s（见 §5） |
| ④ 红检（换回 C# / 停掉 ⇒ 判据必红） | **PASS** —— 换回 C# SelGate ⇒ rc=1（`02`）；还原 ⇒ rc=0（`03`） |
| ⑤ C# 参考栈对照（报告值） | 同批 200 假人：BotSrv `login_ok` 175/200（87.5%，**工具侧漏计数**，见 §6）；协议探针四步 200/200。**本片未重跑 C# 栈，沿用上一片入库件 `04/06/09`** |
| ⑥ 四门禁 + 证据四件套 | **全绿**（fmt/clippy/test/msgcodegen rc=0；msgcodegen 604 常量） |
| ⑦ 起停栈留痕 + 端口回落 | **PASS** —— 起停日志入库：`evidence/21-start-boundary-stack.txt`、`22-start-boundary-stack-probe.txt`、`23-start-boundary-stack-restored.txt`、`24-stop-boundary-stack.txt`、`25-stop-previous-stack.txt`、`26-postwork-ports.txt`；收工端口回落（仅共享 MySQL 3306 保留） |

> **一句话**：全 Rust 链起得来、200 假人登录 100%、**选角 200/200（selchr 修复后复跑）**、曲线平、红检有牙、四门禁全绿。

### 0.1 复审返工汇总

本片是两轮复审的合并返工：**reviewer-2 `ec2471d4`**（F-1/F-2 Important + F-3 blocker）**+ reviewer-5 `56e673ec`**（R-1 Important + F-3 blocker + M-1 Minor）。全部收口如下：

| finding | 级别 | 处置 | 依据（入库件） |
| --- | --- | --- | --- |
| **F-1**（rev-2）：点名证据 `12..15-*.log` 被 `.gitignore:92:*.log` 吞掉 | Important | **已收口**：要入库的证据全用**不被忽略的 `.txt`**（`*.log` 仍被忽略），报告点名的每一件证据写成可点相对路径；`git check-ignore` 对所有入库件无命中 | `evidence/12..15-*.txt`、`17..20-*.txt`、`21..26-*.txt`；清单 `evidence/SHA256SUMS.txt`（**26 件**） |
| **F-2a**（rev-2）：报告 §4 的 `db-srv [QueryChr]=200` 与入库件不符（旧 `07` 是 `1000`，5 行/次查询的累计） | Important | **已收口**：计数单一口径在 `count_m1_logs.py`，把「每次查询 1 行」（`query_chr`=`[QueryChr] 候选`）与「全部行」（`query_chr_lines`=`[QueryChr]`）**分成两个键**，pattern 原样写进 JSON 的 `patterns` 字段 | `evidence/07-rust-side-counts.json`：`query_chr=200`、`query_chr_lines=1000`（口径 = 每次查询 5 行） |
| **F-2b**（rev-2）：`rust_login_gate.connects` 用 `客户端连接`（真实输出是 `开始连接:`）⇒ 恒 0 死指标 | Important | **已收口**：匹配串改真实输出 `开始连接:`；`--selftest` 把这个反例钉住 | `evidence/07`：`rust_login_gate.connects=200`（旧=0）；`16-four-gates.txt` 里 `COUNT_SELFTEST_RC=0` |
| **F-2c**（rev-2 派生）：「run 时一套、复算时又一套」口径漂移 | — | **已收口**：`run_m1_load.ps1` 调 `count_m1_logs.py`；归算 `report-rust` 显式带 `--allow-missing-tick`（与 `load_gate.ps1 -AllowMissingTick` 对齐，避免同一轮 load_gate 判 GREEN 而 report 反判 RED） | `evidence/08-report-rust-clean.json`：`verdict=GREEN`、`tick_missing_acknowledged=true` |
| **R-1**（rev-5）：`SHA256SUMS.txt` 里 `16-four-gates.txt` 的 sha256 是**临时 `.log`** 的哈希（差 1 字节 CRLF→LF）⇒ `sha256sum -c` 直接 FAILED | Important | **已收口**：本片重采全部证据后，用**入库后的 evidence 目录**重算清单（不再对拷入前的临时件算哈希）；并给 `evidence/` 加 `.gitattributes: * -text` **冻结字节**（`*.ndjson` 未在 `mir2-rs/.gitattributes` 的 `eol=lf` 名单里，`core.autocrlf=true` 会让它在检出时变 CRLF ⇒ 哈希漂移；`23` 原本是 UTF-16），从根上消除「工作树/入库件不同字节」 | 复核命令：`cd evidence && sha256sum -c SHA256SUMS.txt` ⇒ **26/26 OK、rc=0**；`git ls-files --eol evidence/` 全为 `i/lf w/lf` |
| **F-3**（rev-2/rev-5）：DoD②「选角」未达 | blocker | **已收口**：selchr 修复已合入 master（`git merge-base --is-ancestor c95e14c8 refs/heads/master` = true）；本片 rebase 到 `60095f65` 后**全栈重跑** 200 假人 + 协议探针 | `evidence/05-probe-rust-200.json`：四步各 200、`ident_hist={520:200,525:200,529:200,530:200}`；`13-rust-db-srv.out.txt`：`玩家获取游戏网关信息`=200 |
| **M-1**（rev-5）：`count_m1_logs.py` docstring 示例引用不存在的 `22-botsrv-bots.txt` | Minor | **已收口**：改成实际入库路径 `20-botsrv-bots-200.txt`（README 同处一并改） | `tools/m1-acceptance/count_m1_logs.py:36`、`tools/m1-acceptance/README.md:73` |

## 1. 起链（影子配置，不动 `E:\MirServer`）

```powershell
cd mir2-rs; $env:CARGO_TARGET_DIR='E:\tmp\openmir2-m1load-target'
cargo build --offline -p login-gate -p login-srv -p sel-gate -p db-srv       # rc=0（源未变，命中缓存）
powershell -File tools/m1-acceptance/start_boundary_stack.ps1               # BOUNDARY_STACK_OK
```

进程（`01-ports-green.json` 的命令行即证据）：

| 端口 | pid | 入口（Rust 二进制） | 配置（`E:\tmp` 影子副本） |
| --- | --- | --- | --- |
| 7000 | 15412 | `E:\tmp\openmir2-m1load-target\debug\login-gate.exe` | `…\stack\login-gate\config.conf` |
| 5500 / 5600 | 2060 | `…\login-srv.exe` | `…\stack\login-srv\logsrv.conf` |
| 7100 | 27796 | `…\sel-gate.exe` | `…\stack\sel-gate\config.conf` |
| 5100 | 19468 | `…\db-srv.exe` | `…\stack\db-srv\dbsvr.conf` |

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

- `evidence/13-rust-db-srv.out.txt`：`账号服务器[127.0.0.1:5600]链接成功.`
- `evidence/12-rust-login-srv.out.txt`：`LegendOfMir2/99/0/DBServer/Online:0/正常`（每 20s 一次，`ServerIndex=99` 即 DBSrv）

### 3.2 SelGate ↔ DBSrv（5100）

- `evidence/14-rust-sel-gate.out.txt`：`数据服务[127.0.0.1:5100]链接成功.` / `角色网关[0.0.0.0:7100]已启动...`
- `evidence/13-rust-db-srv.out.txt`：`角色网关[1](127.0.0.1:…)已打开...` / `玩家数据网关服务[0.0.0.0:5100]已启动.等待链接...`

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
# 等价（当前口径）：tools/botload/load_gate.ps1 -Tiers 200 -HoldSec 60 -Criteria login,conn -AllowMissingTick -RampTimeoutSec 120
#   M1⑤ 不看 tick，而 load_report.py 的「缺 tick 样本」是独立开关（默认判 RED）；故必须显式 -AllowMissingTick。
```

**证据出处**：BotSrv 侧来自 **BotSrv 200 假人同登那一轮**（`evidence/20-botsrv-bots-200.txt` = 本轮 `run_m1_load.ps1` 的 `tier-200/bots.log`，22:42）；Rust 服务侧来自**同批 200 并发的协议探针复跑**（`evidence/12..15-*.txt`，22:45；这四份是探针所连那条链的 stdout）。两侧计数由**同一个脚本**从上述入库件重算：`evidence/07-rust-side-counts.json`。

BotSrv 侧（`evidence/07-rust-side-counts.json` → `bots`；原始件 `evidence/20-botsrv-bots-200.txt`）：

| 计数 | 值 |
| --- | --- |
| 开始登陆 | 200 |
| 帐号登录成功（`LoadMetrics.LoginOk`） | **200 / 200 = 100%** |
| 查询角色 | 200 |
| 选择角色（发出 `CM_SELCHR`） | **200** |
| 准备进入游戏（收到 `SM_STARTPLAY(525)`） | 30 ← **工具侧漏计数**（见下） |
| `load_report.py` verdict | **GREEN**（`login_success_pct=100.0`、`conn_lost_steady=0`、ramp 15047ms） |
| 假人侧 `internal_errors` / `conn_refused` / `conn_lost` | **0 / 27 / 0** |

> **`准备进入游戏=30` 是工具侧漏计数，不是链侧**：同批协议探针四步 200/200、Rust db-srv 侧
> `玩家获取游戏网关信息`=200（即 200 条 `SM_STARTPLAY(525)` 已发出）。且本轮**全 Rust 链不含 7200**
> （GameSvr/RunGate 不在 M1⑤ 链路），假人收到 525 后连 `127.0.0.1:7200` 得 `拒绝链接` 属预期。
> BotSrv 计数漏丢是卡 `openmir2-botsrv-world-evidence` 的已知问题（登录响应被假人侧跨假人共享态静默丢弃）。

Rust 服务侧（`evidence/07-rust-side-counts.json` → `rust_*`/`panics`；原始件 `evidence/12..15-*.txt`）：

| 计数 | key（`07` 里可核） | 值 | 口径 |
| --- | --- | --- | --- |
| login-srv `[登录] 帐号=` | `rust_login_srv.login_ok` | **200** | 每次登录 1 行 |
| login-srv `[选服] account=` | `rust_login_srv.select_server` | **200** | 每次选服 1 行 |
| db-srv `[QueryChr] 候选` | `rust_db_srv.query_chr` | **200** | **每次查询 1 行** |
| db-srv `[QueryChr]` 全部行 | `rust_db_srv.query_chr_lines` | **1000** | **每次查询共 5 行**（候选/GetBy/dataIndex/GetQryChar/debug）= 200×5 |
| db-srv `玩家获取游戏网关信息`（SM_STARTPLAY） | `rust_db_srv.start_play` | **200** | 选角成功回 525 |
| sel-gate `开始连接:` | `rust_sel_gate.connects` | **200** | 接受 TCP 连接 |
| login-gate `开始连接:` | `rust_login_gate.connects` | **200** | 接受 TCP 连接 |
| 四进程 `panicked` | `panics.*` | **0 / 0 / 0 / 0** | |

### 4.1 BotSrv 无关的协议探针（判「链本身」）

`probe_m1_chain.py`（200 并发、stagger 20ms、一条独立 TCP 走完四步）：

| 步骤 | 全 Rust 链（本片复跑） | C# 参考栈（上一片入库件 `06`） |
| --- | --- | --- |
| `CM_IDPASSWORD`（7000） | **200/200** | 200/200 |
| `CM_SELECTSERVER`（7000） | **200/200** | 200/200 |
| `CM_QUERYCHR`（7100） | **200/200** | 200/200 |
| `CM_SELCHR`（7100） | **200/200** ← 本片修复 | **200/200** |
| ident 直方图 | `{520:200, 525:200, 529:200, 530:200}` | `{520,525,529,530}` |

⇒ 登录链 100%；**选角 200/200**（`evidence/05-probe-rust-200.json`）。

## 5. 内存曲线（`load_report.py` 同一口径）

`run_m1_load.ps1` 在 load 期间另采 Rust 四进程（schema 同 `load_gate`），过滤到本线 pid 后用
`load_report.py summarize` 归算（`evidence/08-report-rust-clean.json`；原始序列 `evidence/10-rust-load-stats.ndjson` + `evidence/11-rust-mem.ndjson`）：

```sh
python tools/botload/load_report.py summarize --stats <10> --mem <11> --count 200 \
  --criteria login,conn --allow-missing-tick --out <08>
```

| 进程 | ws_peak | ws_mean | priv_peak | **末段斜率** |
| --- | --- | --- | --- | --- |
| db-srv | 14.7 MB | 14.5 MB | 6.9 MB | **+0.002 MB/s** |
| login-gate | 9.2 MB | 8.6 MB | 4.2 MB | **-0.011 MB/s** |
| login-srv | 12.6 MB | 11.6 MB | 4.6 MB | **-0.000 MB/s** |
| sel-gate | 9.2 MB | 8.7 MB | 4.2 MB | **-0.011 MB/s** |

窗口 82.8s（末段 59.7s）。⇒ 末段斜率 ≈ 0（M5「泄漏 vs 高水位」判据要的就是这份口径）。
（注：采样期机上有另一条车道的一次性 `login-srv` 测试进程 pid=5596，已按本线四进程 pid 过滤，`11-rust-mem.ndjson` 只含本线 pid=28600/22520/19732/25104。）

## 6. C# 参考栈对照（报告值，非判据）

**本片未重跑 C# 栈**（端口窗口与时间预算留给 F-3 复跑）；沿上一片入库件 `04-ports-red-csharp-reference-stack.json`、`06-probe-csharp-200.json`、`09-report-csharp-fresh.json`。

同一批 `loadbot0..199`、同一序列、同一 stagger=20ms，在 `E:\MirServer\start-all.ps1`（C# 部署栈：DBSrv/LoginSrv=仓库构建、GameSvr/三网关=`E:\MirServer`）上：

| 驱动 | 结果 |
| --- | --- |
| BotSrv | `login_ok` **175/200 = 87.5%**，verdict RED（爬坡未完成）；查询角色 175 / 选择角色 121 / 准备进入游戏 121；`internal_errors=1953`；ramp 峰值 5016ms |
| BotSrv（另一轮，旧栈） | 170/200 = 85.0%，`internal_errors=9809` |
| 协议探针 | **四步 200/200**（含选角） |
| 端口所有权 | RED（7000/7100 判 csharp；5500/5100/5600 判 unknown——C# 构建输出不在 `E:\MirServer` 根下） |

**关键结论**：`BotSrv.LoadMetrics.login_ok` 在两个栈上都会**漏计数**（C# 175/200；卡
`openmir2-botsrv-world-evidence` 的已知问题），但**协议探针在同一台 C# 栈上 200/200** ⇒ 漏计数是**工具侧**、不是链侧。
本轮全 Rust 链上 BotSrv 登录 200/200（`internal_errors=0`），优于 C# 栈这一轮。

## 7. 红检（④）——判据不是摆设

`redcheck_boundary.ps1 -Mode swap-csharp-selgate`：

1. 基线：`GREEN` rc=0（`17-redcheck-0-green.txt`，对应 `redcheck-0-green.json`）
2. **只把 7100 一环换回 C#**（停 Rust `sel-gate`，起 `E:\MirServer\SelGate\SelGate.exe`）：其余四端口仍 Rust，
   `02-ports-red-swapped-csharp-selgate.json`：`7100 [csharp] … verdict=unknown` ⇒ **`RED` rc=1**（`18-redcheck-1-swap-csharp-selgate.txt`）
3. 停 C#、全栈重起：`03-ports-restored-green.json` ⇒ **`GREEN` rc=0**（`19-redcheck-2-restored.txt`）

（另：C# 参考栈在跑时同一判据也 `RED`——`04-ports-red-csharp-reference-stack.json`。）

**注意（复现者）**：`redcheck_boundary.ps1` 内部会把 `redcheck-0/1/2` 写到 `-EvidenceDir`；本片入库时对应改名/内容为 `17/18/19-*.txt` 与 `02/03-ports-*.json`，并额外重放了第 3 步的机判（`03`/`19`），因 redcheck 最后一步的起栈子进程会让父脚本的管道悬挂（见 `23-start-boundary-stack-restored.txt`）。

## 8. 发现的 Rust 侧真缺陷（已修复、已合入、本片复跑收口）

- **卡**：`openmir2-m1-dbsrv-selchr-queryed`（issue `2e5d3b08`）
- **缺陷**：`bins/db-srv/src/handler.rs` 的 `query_chr()` 成功返回时未置 `u.chr_queryed = true`；
  C# oracle `src/DBSrv/Services/Impl/UserService.cs:359` 有这一行 ⇒ `CM_SELCHR` 恒被拒（`[WRN] Double send SELCHR`），从不回 `SM_STARTPLAY(525)`。
- **本卡处置**：按 brief 红线**不改 `bins/**`**，登记（comment `6ea9562e`）+ 另开卡（正确）。
- **修复与合入**：head `c95e14c8`（`CM_QUERYCHR` 成功即置 `u.chr_queryed = true` + 进程级 `QUERYCHR→SELCHR` 断言 `SM_STARTPLAY(525)`），
  approve reviewer-6 `15ce7938`，由 merge commit `5db28d8d` 合入 master；本片 base `60095f65` 为其后继。
  复核：`git merge-base --is-ancestor c95e14c8 refs/heads/master` = **true**。
- **本片复跑**：全栈重起 → `run_m1_load.ps1` + `probe_m1_chain.py --count 200` ⇒ **`select_chr 200/200`、`ident_hist` 含 525**（`05`），db-srv `start_play=200`（`07`/`13`）。**F-3 收口**。

## 9. 四门禁（工作区根在 `mir2-rs/`，隔离 target）

```powershell
cd mir2-rs; $env:CARGO_TARGET_DIR='E:\tmp\openmir2-m1load-target'
cargo fmt --all -- --check                                   # rc=0
cargo clippy --workspace --all-targets --offline -- -D warnings   # rc=0
cargo test --workspace --offline                             # rc=0（全 ok）
cargo run -p msgcodegen --offline -- --check                 # rc=0：604 条常量与 C# 源一致
```

原始输出：`evidence/16-four-gates.txt`（含各门 `rc=0` 与 `msgcodegen --check OK: 604 条`；文件头部记的 `git head`
是跑门禁时的分支 tip，证据入库后提交 hash 会变，但**源码树一致**）。
本卡**未新增 Rust 测试**（工具是 `tools/m1-acceptance/**` 的 Python/PowerShell 脚本 + 证据），
故不触发「新测试 ≥3 连跑」条款；`count_m1_logs.py --selftest` 作为计数口径自检（`COUNT_SELFTEST_RC=0`，见 `16`）。

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

**只用入库件复算两侧计数**（F-2 口径，不占端口）：

```sh
python tools/m1-acceptance/count_m1_logs.py --selftest
python tools/m1-acceptance/count_m1_logs.py \
  --bots      mir2-rs/tests/parity/m1-boundary-load/evidence/20-botsrv-bots-200.txt \
  --login-srv mir2-rs/tests/parity/m1-boundary-load/evidence/12-rust-login-srv.out.txt \
  --db-srv    mir2-rs/tests/parity/m1-boundary-load/evidence/13-rust-db-srv.out.txt \
  --sel-gate  mir2-rs/tests/parity/m1-boundary-load/evidence/14-rust-sel-gate.out.txt \
  --login-gate mir2-rs/tests/parity/m1-boundary-load/evidence/15-rust-login-gate.out.txt \
  --tier 200 --hold-sec 60 --criteria login,conn --load-rc 0 \
  --out       mir2-rs/tests/parity/m1-boundary-load/evidence/07-rust-side-counts.json
cd mir2-rs/tests/parity/m1-boundary-load/evidence && sha256sum -c SHA256SUMS.txt   # 期 26/26 OK
```

## 11. 残留物清单

| 路径 | 是什么 | 处置 |
| --- | --- | --- |
| `E:\tmp\openmir2-m1load\stack\` | 影子配置 + 四服务 stdout/err + pid 文件 | **保留**（报告引用；下次复跑直接复用/覆盖）。收工时四服务已 `-Stop`，日志仍留（`12..15` 的采集源） |
| `E:\tmp\openmir2-m1load\evidence\` | 上一片原始输出（`build.log`/`gates*.log`/`csharp-200*`/`rust-200` 旧轮） | **保留**（C# 对照与历史证据；入库副本见 repo `evidence/`） |
| `E:\tmp\openmir2-m1load\evidence\rerun-f3\` | **本片**原始输出（起停栈、端口实拍、红检、探针、内存、门禁、BotSrv 轮） | **保留**（本片入库件的来源；`21..26`/`12..15` 等已拷入 repo `evidence/`） |
| `E:\tmp\openmir2-m1load-target\` | 隔离 cargo target（四个 bin） | 保留（复跑需要；删除会重编） |
| `E:\MirServer\logs\` | C# 参考栈的 stdout | 非本线脚本产物；`E:\MirServer` **未改** |
| 进程 | 全 Rust 链已 `-Stop`；C# 参考栈本片未起；MySQL 3306 保留（共享基础设施，开工前即在跑） | 收工端口回落见 `evidence/26-postwork-ports.txt`：3306 UP，其余 5000/5100/5500/5600/5700/6000/7000/7100/7200/16005 全 DOWN |

## 12. 分支与交付（本片）

- **rebase（声明）**：上一片 head 为基于 `ab26808e` 的 `e587760d`（reviewer-5 钉的 `56e673ec`）。
  交卡要求 `patch --base refs/heads/master`，而 `master` 在本片期间前移（`60095f65` → `b3a40814`）；为免交付 diff 把
  master 新增的 M3/M2 变更**反向**带进去，本片把分支 **rebase 到交付时 `refs/heads/master` = `b3a40814`**（四个提交内容不变，只改 hash）。
  证据是在 `60095f65` 的目标树上采的，但四条边界 bin 的源码在 `60095f65..b3a40814` 之间零改动（见报告头），故证据对 `b3a40814` 同样成立。
  **本片只动 `tools/m1-acceptance/**` 与 `mir2-rs/tests/parity/m1-boundary-load/**`（工具/证据/报告），
  不碰 `bins/**`/`src/**`/`tools/botload/**`**；`git diff --name-only b3a40814..HEAD | grep -v 'm1-acceptance\|m1-boundary-load'` 为空。
- **本片重跑了链路**（F-3）：全栈起停、端口机判、200 假人、协议探针、红检、四门禁；证据全量更新并重算 SHA256SUMS（R-1）。
- 四门禁在 rebase 后的 head 上重跑（`evidence/16-four-gates.txt`，工作区根 `mir2-rs/`）。
