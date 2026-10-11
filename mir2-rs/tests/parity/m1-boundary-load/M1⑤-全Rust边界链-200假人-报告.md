# M1⑤ 载荷证据：全 Rust 边界链 + 200 假人登录/选角（报告）

- 卡：`openmir2-m1-boundary-load-200`（issue `d01477d5`）
- 身份：`openmir2-worker-15`；worktree `m1load`；分支 `worktree-m1-boundary-load`
- **证据采集时的 base：`f5170a8e`**；本片返工后交付分支已 **rebase 到交付时的 `refs/heads/master`（`ab26808e`，含 M2 入图帧合并）**（本片只做证据入库/计数口径/报告订正，**不重跑链路**，见 §12）
- 链路表：`mir2-rs/docs/M1-边界服务移植计划.md` §1（DBSrv 用 **5100**，不是 6000）
- 工具：`tools/m1-acceptance/**`（本卡新增；计数单一口径 = `count_m1_logs.py`）
- 证据（**全部入库**，本目录 `evidence/`；下表路径均为相对仓库根的可点相对路径）：`01…24` + `SHA256SUMS.txt`

## 0. 结论摘要（逐条）

| 判据 | 结果 |
| --- | --- |
| ① 全 Rust 边界链 + 端口所有权机判 | **GREEN** —— 7000/5500/7100/5100/5600 五端口监听者全判 `rust` 且入口与端口表一致（`01-ports-green.json`） |
| ① 内部链（16005 / 5600） | **5600（DBSrv→LoginSrv）活着**；**16005 两侧都不存在**（C# oracle 源码无 `LsvServer` 实现，见 §3.3） |
| ② 200 假人同时登录 | **成功率 100%**（BotSrv 200/200；Rust login-srv 侧 `[登录]`/`[选服]` 各 200） |
| ② 200 假人选角 | **BLOCKED** —— Rust `bins/db-srv` 从不置 `chr_queryed` ⇒ `CM_SELCHR` 恒被拒（协议探针 `select_chr 0/200`；db-srv 200 条 `Double send SELCHR`）。**已另开卡 `openmir2-m1-dbsrv-selchr-queryed` 且修复已交出（head `c95e14c8`），但尚未合入 `master`** ⇒ 本片仍判 BLOCKED，按 brief 登记“等合入后复跑”（见 §0.1 F-3 / §8） |
| ② 无 panic / 无未捕获异常 | **PASS** —— Rust 四进程 `panicked` 计数全 0；BotSrv `internal_errors=0` |
| ③ 内存曲线 + 末段斜率 | **PASS**（`load_report.py` 同一口径）：四进程末段斜率 0.000–0.001 MB/s（见 §5） |
| ④ 红检（换回 C# / 停掉 ⇒ 判据必红） | **PASS** —— 换回 C# SelGate ⇒ rc=1（`02`）；还原 ⇒ rc=0（`03`） |
| ⑤ C# 参考栈对照（报告值） | 同批 200 假人：BotSrv `login_ok` 175/200（87.5%，**工具侧漏计数**，见 §6）；协议探针四步 200/200 |
| ⑥ 四门禁 + 证据四件套 | **全绿**（fmt/clippy/test/msgcodegen rc=0；msgcodegen 604 常量） |
| ⑦ 起停栈留痕 + 端口回落 | **PASS** —— 起停日志入库：`evidence/21-start-boundary-stack-1825.txt`、`22-start-boundary-stack-1844.txt`、`23-stop-boundary-stack-1835.txt`、`24-stop-boundary-stack-1846.txt`；收工端口回落（仅共享 MySQL 3306 保留） |

> **一句话**：全 Rust 链起得来、200 假人登录 100%、曲线平、红检有牙、四门禁全绿；但**「选角」被 Rust db-srv 的一个既有缺陷挡住**，已按 brief 红线另开卡（本卡不顺手改 `bins/**`）。

### 0.1 复审返工（reviewer-2 `ec2471d4`，pinned head `3b457465`；本片 head 见 §12）

| finding | 级别 | 处置 | 依据（入库件） |
| --- | --- | --- | --- |
| **F-1**：报告点名的 `12..15-*.log` 被 `.gitignore:92:*.log` 吞掉（入库只有 01..11） | Important | **已收口**：把要入库的证据全部改成**不被忽略的 `.txt`**（`*.log` 仍被忽略），并把报告里点名的证据全部落库、路径写成可点相对路径 | `evidence/12-rust-login-srv.out.txt` … `15-rust-login-gate.out.txt`；另补 `17..24-*.txt`（红检/起停栈/BotSrv bots）；清单 `evidence/SHA256SUMS.txt`。复核：`git ls-files mir2-rs/tests/parity/m1-boundary-load/evidence/` = 24 个 + 清单 |
| **F-2a**：报告 §4 的 `db-srv [QueryChr]=200` 与入库件不符（旧 `07` 里 `query_chr=1000`，是 5 行/次查询的累计） | Important | **已收口**：计数单一口径改到 `count_m1_logs.py`，把“每次查询 1 行”（`query_chr`=`[QueryChr] 候选`）与“全部行”（`query_chr_lines`=`[QueryChr]`）**分成两个键**，pattern 原样写进 JSON 的 `patterns` 字段；报告数字按重算后的入库件 `07` 改（不再手填） | 新 `evidence/07-rust-side-counts.json`：`query_chr=200`、`query_chr_lines=1000`；`patterns` 可机读 |
| **F-2b**：`rust_login_gate.connects` 用了 `客户端连接`（真实输出是 `开始连接:`）⇒ 恒 0 的死指标 | Important | **已收口**：匹配串改为真实输出 `开始连接:`；`count_m1_logs.py --selftest` 把这个反例钉住 | 新 `07`：`rust_login_gate.connects=200`（旧=0）；`selftest` 反例：`客户端连接` 不得再出现在 `patterns` 里 |
| **F-3**：②「选角」未达（DoD ②） | 阻塞 | **未收口（如实）**：缺陷卡分支 head `c95e14c8` **尚未合入 `master`**（`git merge-base --is-ancestor c95e14c8 refs/heads/master` = false）⇒ 本片不重跑选角（按 brief 不占端口、不改 `bins/**`）；已在卡上登记“等 `c95e14c8` 合入”，合入后复跑即收 | `evidence/05-probe-rust-200.json`（`select_chr=0/200`）；卡 comment（本片） |
| 另：报告数字改由脚本再生成，不手改 | — | **已做**：重跑 `tools/botload/load_report.py summarize`（当前口径，带 `--allow-missing-tick`）=> `08-report-rust-clean.json`；重跑 `count_m1_logs.py` => `07` | `evidence/08-report-rust-clean.json`（`verdict=GREEN`、`login_success_pct=100.0`、`tick_missing_acknowledged=true`） |

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

- `evidence/13-rust-db-srv.out.txt`：`账号服务器[127.0.0.1:5600]链接成功.`
- `evidence/12-rust-login-srv.out.txt`：`LegendOfMir2/99/0/DBServer/Online:0/正常`（每 20s 一次，`ServerIndex=99` 即 DBSrv）

### 3.2 SelGate ↔ DBSrv（5100）

- `evidence/14-rust-sel-gate.out.txt`：`数据服务[127.0.0.1:5100]链接成功.` / `角色网关[0.0.0.0:7100]已启动...`
- `evidence/13-rust-db-srv.out.txt`：`角色网关[1](127.0.0.1:61774)已打开...` / `玩家数据网关服务[0.0.0.0:5100]已启动.等待链接...`

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

**证据出处（两段，均在入库件里）**：下半“BotSrv 侧”来自 **BotSrv 200 假人同登那一轮**
（`evidence/20-botsrv-bots-200.txt` = `…\rust-200\tier-200\bots.log`，18:30）；
“Rust 服务侧”来自**同批 200 并发的协议探针复跑**（`evidence/12..15-*.out.txt`，18:45；
因服务 stdout 日志在栈重起时被覆盖，这四份就是 18:44 起的那条链的 stdout）。
两侧计数由**同一个脚本**从上述入库件重算：`evidence/07-rust-side-counts.json`。

BotSrv 侧（`evidence/07-rust-side-counts.json` → `bots`；原始件 `evidence/20-botsrv-bots-200.txt`）：

| 计数 | 值 |
| --- | --- |
| 开始登陆 | 200 |
| 帐号登录成功（`LoadMetrics.LoginOk`） | **200 / 200 = 100%** |
| 查询角色. | 200 |
| 选择角色（发出 `CM_SELCHR`） | 200 |
| 准备进入游戏（收到 `SM_STARTPLAY(525)`） | **0** ← 选角被拒（见 §8） |
| `load_report.py` verdict | **GREEN**（`login_success_pct=100.0`、`conn_lost_steady=0`、ramp 5016ms） |
| 假人侧 `internal_errors` / `conn_refused` / `conn_lost` | **0 / 0 / 0** |

Rust 服务侧（`evidence/07-rust-side-counts.json` → `rust_*`/`panics`；原始件 `evidence/12..15-*.out.txt`）：

| 计数 | key（`07` 里可核） | 值 | 口径 |
| --- | --- | --- | --- |
| login-srv `[登录] 帐号=` | `rust_login_srv.login_ok` | **200** | 每次登录 1 行 |
| login-srv `[选服] account=` | `rust_login_srv.select_server` | **200** | 每次选服 1 行 |
| db-srv `[QueryChr] 候选` | `rust_db_srv.query_chr` | **200** | **每次查询 1 行** |
| db-srv `[QueryChr]` 全部行 | `rust_db_srv.query_chr_lines` | **1000** | **每次查询共 5 行**（候选/GetBy/dataIndex/GetQryChar/debug）= 200×5 |
| db-srv `玩家获取游戏网关信息`（SM_STARTPLAY） | `rust_db_srv.start_play` | **0** | |
| sel-gate `开始连接:` | `rust_sel_gate.connects` | **200** | 接受 TCP 连接 |
| login-gate `开始连接:` | `rust_login_gate.connects` | **200** | 接受 TCP 连接（**旧版误用 `客户端连接` 恒 0，已修**） |
| 四进程 `panicked` | `panics.*` | **0 / 0 / 0 / 0** | |

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
`load_report.py summarize` 归算（`evidence/08-report-rust-clean.json`；原始序列 `evidence/10-rust-load-stats.ndjson` + `evidence/11-rust-mem.ndjson`）。
**本片已用当前 `master` 的 `load_report.py` 重跑这条命令再生成 `08`**（口径从 base 版升到含
`--allow-missing-tick` 的版本；`08` 里因此多了 `tick_missing_acknowledged=true` 与 `missing_tick_note`，
其余与旧版逐项一致，`verdict` 仍 `GREEN`）：

```sh
python tools/botload/load_report.py summarize --stats <10> --mem <11> --count 200 \
  --criteria login,conn --allow-missing-tick --out <08>
```

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

红检原始输出：`evidence/17-redcheck-0-green.txt`、`evidence/18-redcheck-1-swap-csharp-selgate.txt`、`evidence/19-redcheck-2-restored.txt`（对应 `01/02/03-ports-*.json`）。

## 8. 发现的 Rust 侧真缺陷（已另开卡）

- **卡**：`openmir2-m1-dbsrv-selchr-queryed`（issue `2e5d3b08`）
- **缺陷**：`bins/db-srv/src/handler.rs` 的 `query_chr()` 成功返回时未置 `u.chr_queryed = true`；
  C# oracle `src/DBSrv/Services/Impl/UserService.cs:359` 有这一行。⇒ `dispatch` 的 `CM_SELCHR` 分支
  `if u.chr_queryed` 恒假，恒走 `else` → `[WRN] Double send SELCHR`，从不回 `SM_STARTPLAY(525)`。
- **为什么之前没被测出**：A2 交付时 `SELCHR` 未纳入字节对拍（A2 报告 §2.6 已如实登记）。
- **本卡处置**：按 brief 红线**不改 `bins/**`**，只在本卡登记（comment `6ea9562e`）+ 另开卡。
- **影响**：本卡 ② 的「登录」100%；「选角」须待该卡修复后复跑本目录命令方能判绿。
- **修复状态（2026-10-11 本片复核）**：缺陷卡分支已交出修复
  （head `c95e14c8`：`CM_QUERYCHR` 成功即置 `u.chr_queryed = true` + 进程级 `QUERYCHR→SELCHR` 断言
  `SM_STARTPLAY(525)`），但尚未合入 `master`：
  `git merge-base --is-ancestor c95e14c8 refs/heads/master` = **false**
  ⇒ 本片按 brief **不重跑选角、不占端口**，只登记“等合入”；合入后复跑
  `run_m1_load.ps1` + `probe_m1_chain.py`（期 `select_chr 200/200`）并把新原始件入库即可收口。

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
本卡未新增 Rust 测试（工具是 `tools/m1-acceptance/**` 的 Python/PowerShell 脚本 + 证据），
故不触发「新测试 ≥3 连跑」条款；`count_m1_logs.py --selftest` 作为计数口径自检（结果见 `16`）。

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

**只用入库件复算两侧计数**（本片 F-2 的口径，不占端口）：

```sh
python tools/m1-acceptance/count_m1_logs.py --selftest
python tools/m1-acceptance/count_m1_logs.py \
  --bots      mir2-rs/tests/parity/m1-boundary-load/evidence/20-botsrv-bots-200.txt \
  --login-srv mir2-rs/tests/parity/m1-boundary-load/evidence/12-rust-login-srv.out.txt \
  --db-srv    mir2-rs/tests/parity/m1-boundary-load/evidence/13-rust-db-srv.out.txt \
  --sel-gate  mir2-rs/tests/parity/m1-boundary-load/evidence/14-rust-sel-gate.out.txt \
  --login-gate mir2-rs/tests/parity/m1-boundary-load/evidence/15-rust-login-gate.out.txt \
  --tier 200 --hold-sec 60 --criteria login,conn \
  --out       mir2-rs/tests/parity/m1-boundary-load/evidence/07-rust-side-counts.json
```

## 11. 残留物清单

| 路径 | 是什么 | 处置 |
| --- | --- | --- |
| `E:\tmp\openmir2-m1load\stack\` | 影子配置 + 四服务 stdout/err + pid 文件 | **保留**（报告引用；下次复跑直接复用/覆盖） |
| `E:\tmp\openmir2-m1load\evidence\` | 本轮全部原始输出（起停栈、端口实拍、红检、探针、内存、门禁、C# 对照） | **保留**（证据；入库副本见 `mir2-rs/tests/parity/m1-boundary-load/evidence/`） |
| `E:\tmp\openmir2-m1load-target\` | 隔离 cargo target（四个 bin） | 保留（复跑需要；删除会重编） |
| `E:\tmp\openmir2-m1load\probe200.py` | 过程件：先做的「只打登录」探针（诊断 BotSrv 漏计数用） | 保留；正式件是 `tools/m1-acceptance/probe_m1_chain.py` |
| `E:\MirServer\logs\` | C# 参考栈的 stdout（start-all 覆盖式写） | 非本线脚本产物；`E:\MirServer` 未改 |
| `E:\tmp\openmir2-m1load\evidence\rust-200\`、`csharp-200*\` | BotSrv 侧 `load_stats`/`bots.log`/`report.json`/内存 | 保留（报告引用；`bots.log` 已入库为 `evidence/20-botsrv-bots-200.txt`） |
| `E:\tmp\openmir2-m1load-skipcheck\` | 本片造的过程件：验证 `run_m1_load.ps1 -SkipLoad` → `count_m1_logs.py` 这条路（无端口） | **收工已删** |
| 进程 | 全 Rust 链已 `-Stop`；C# 参考栈已 `start-all.ps1 -Stop`；MySQL 3306 保留（共享基础设施，开工前即在跑） | 本片未起任何服务（无占端口）；十一端口实况：3306 UP，其余 5100/5500/5600/5700/6000/7000/7100/7200/5000/16005 全 DOWN |

## 12. 分支与交付（本片）

- **rebase（声明）**：上一片 head 为基于 `f5170a8e` 的 `3b457465`（reviewer-2 钉的 `ec2471d4`）。
  交卡要求 `patch --base refs/heads/master`，而 `master` 已前移到 `d3e5c86f`（交付前又前进一格到 `ab26808e`）；
  为免交付 diff 把 master 的 M2 变更**反向**带进去，本片把分支 **rebase 到交付时的 `refs/heads/master`（`ab26808e`）**
  （三个提交内容不变，只改 hash）后交付。**未改动被审代码逻辑**；本片只动 `tools/m1-acceptance/**` 与
  `mir2-rs/tests/parity/m1-boundary-load/**`（证据/报告），不碰 `bins/**`/`src/**`/`tools/botload/**`；
  master 新增的 M2 提交与本书写集**零交集**（`git diff --name-only master..HEAD | grep -v 'm1-acceptance\|m1-boundary-load'` 为空）。
- **本片不入库取新链路结果**：F-1/F-2 是**入库件与计数口径**的修复（用已采的原始件重算，不占端口）；
  选角（F-3）等 `c95e14c8` 合入后另跑。
- 四门禁在 rebase 后的 head 上重跑（`evidence/16-four-gates.txt`，工作区根 `mir2-rs/`）。
