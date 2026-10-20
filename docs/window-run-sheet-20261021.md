# 真机窗口 Run Sheet v2.3（OpenMir2，2026-10-21）

> **v2.3 修订（2026-10-21，tier=L，纯工具/文档，离线）**：补齐 C10 的 5 个**离线可做**缺口——
> ① `s2c_diff.py` 增 `--from-recog/--from-x/--from-y/--to-recog`（C10#2，C3 不再只截首对）；
> ② `check_client_s2c_frames.py` 增 52 body 的 `Weight/MaxExp` 字段级解码（C10#1）与
>    `--expect-enter-seq auto`（C10#10）；③ `account_provision.ps1` 增 `-Db/-DbHost`（C10#7）；
> ④ `check_db_paths.ps1` 增 `-Table/-AllowDb` 透传（C10#11 ①）；⑤ `dbsnap.py` 增
> `--db test|prod|DB` 预设与 snap 可选读库白名单（C10#9）。每个工具都带自检（见 §0.3b）。
> **未闭环项原样保留**（#3/#4/#5/#6/#8 与 #11 的 ②：`scoped_diff.TABLE_COLUMNS` 未登记 `mir2_test_*`）。

> **v2.2 修订（同日）**：第二轮复审 N1–N4——统一 E1-a 下 G5 字段级 diff 的不可收口径（C5/C10#11/D.3）、
> A2 影子身份文件真正落盘（`Tee-Object`）、旧影子目录 fail-closed 归档、外部 C# 全栈停机方式不再假设 pid 文件。

> **v2.1 修订（同日）**：按评审 F1–F12 修正——C# oracle 必须用**全新影子**（旧 `M2GameSvr-hook` 停在 `628d8024`）、
> whitelist 补 `body_sha256`、R8 外部 C# 全栈的停机责任、B11 测试库开号、C4 按 `recog` 过滤、
> D.3 的 E1-a/E1-b 分支、7600 影子 gate 的 pid 落盘、A5 的测试库快照组、C6 摘要键、C1 用例数、
> B3/B5 可照抄 SQL、以及 `start_rust_stack -Stop` 对 MySQL 的条件结论。

> **本文件完整取代 `docs/window-run-sheet-20261020.md`（草稿）。** 草稿的评审结论
> （`docs/window-run-sheet-review-20261020.md`：A1–A5 / B1–B12 / C1–C9 / D1–D4 / E1–E4）已逐条吸收改正；
> 凡与本文件冲突的旧写法，一律以本文件为准。

---

## 🚫 状态：未获授权前不可开窗

- **当前唯一有效授权** = owner 2026-10-19 批准的「**只开缩窗 W0-1..W0-5**」
  （`docs/window-owner-decisions-20261018.md` §5 末项）。该授权是**一次性**的、已随 2026-10-19 实跑用尽，
  **不能外推**到本文件的任何组、任何 SQL、任何停/起 MySQL 动作。
- 本文件是**计划件**，不表示已执行。`E` 节的 E1–E4 未裁决 ⇒ 对应组**不可开窗**（见 §E）。
- **2026-10-20 只读实跑 guard 的结果是 R6/R7/R8 三红**（明细 §A1）：即使 owner 现在口头授权，
  机器也不满足开窗前置。**先修 §A1 的三项，再谈开窗。**

---

## 0. 基线、口径与工具实况

### 0.1 基线

| 项 | 值 |
| --- | --- |
| 代码基线 | `origin/master` @ **`35c039c8d6fef6611866977cd2e360b35c8fe6c4`**（本文件写作时） |
| 为什么不写 `3bdfa0888` | 那是 **70 个提交之前**；`check_rust_window_preflight.py` 的 R6 以 `origin/master` 为基线，生产路径已变 66 件 ⇒ 按旧基线构建必 RED。开窗当天一律**重新解析** `origin/master`，本文件不把任何 oid 当长期合同。 |
| 构建源 | 专用镜像 `/e/tmp/m2svr-window-origin`（**不是**主检出；guard 固定这个源） |
| 构建产物 | `E:/tmp/m2svr-window-target/debug`（guard 固定这个 target） |
| C# 参照栈 | 主检出 `E:/Users/gxh/Documents/GitHub/OpenMir2` 的 `src/DBSrv/bin/Release`、`src/LoginSrv/bin/Release`；部署件 `E:/MirServer` |
| 本文件固定工作目录 | `E:/tmp/window/20261021-a`（下文所有 `<WIN>` 均指它；命令里直接写全路径） |
| 判据脚本目录 | `E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013`（下文 `<CHK>` 均指它） |

### 0.2 当前机器实况（2026-10-20，只读）

`bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard` 实测：

- **R6 RED**：镜像 `HEAD=f3675f69aa` 落后 `origin/master@35c039c8d6` 的**生产路径 66 件**。
- **R7 PREREQ_RED**：`E:/tmp/m2svr-window-target/window-provenance.json` **不存在**（target 目录也不存在）。
- **R8 RED**：九游戏端口当前**都在 LISTENING**：`5000=9440, 5100=9964, 5500=22324, 5600=22324, 5700=9964, 6000=9964, 7000=3544, 7100=3584, 7200=18464`。
  ⇒ 机器上已有别的/修复前的栈。**不得直接 `Stop-Process`**；先由编排/owner 确认 pid 归属，用其**自己的 pid 文件**停。
- 镜像另有一处未提交改动：`M docs/window-shrink-run-20261019.md`（见 §A1）。

### 0.3 工具复核（对提交 `a8f1cd674` 的逐条核对）

`a8f1cd674`（`test(parity): 窗口 run sheet 判读工具补牙 C1/C2/C4/B9/C6`）在当前树中**确实存在**，
下列命令已用 `--help` / `--selftest` 实跑核对：

| 评审项 | 工具（绝对路径） | 实跑核对结果 |
| --- | --- | --- |
| C1 | `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_sm_hit.py` | 存在；`--selftest` **SELFTEST GREEN**（15 条正反例）。参数：`--capture --expect count,order --expect-count --min-count --lead-ms(默认250) --min-order-pairs --require-each-struck --expect-hit R,X,Y,DIR --expect-recog/-x/-y/-dir --expect-recipients --conn` |
| C2 | `.../check_attack_observability.py` | `--expect {attack,damage,kill,any,incoming,none}` 已含 **`incoming`**；`--selftest` rc=0。**正式判据禁用 `--allow-unknown-self`**（工具注明仅诊断） |
| C4 | `.../check_timing.py` | 存在；`--selftest` **SELFTEST GREEN**（15 条）。读 capture 的 **`ts_ms`**（不是旧文的 `t_rel_us`）；规则 `after/interval/rate`；`--example-config` 可打印模板 |
| B9 | `.../check_client_s2c_frames.py` | 已含 **`--expect-enter-seq`**，预设 `default/equipped/delmagic/g2`；**v2.3** 增 `auto` 预设、`--expect-52-weight/--expect-52-max-exp/--expect-52-layout`（40B/58B 自动识别）；`--selftest` 28 条 rc=0 |
| C6 | `mir2-rs/tests/parity/m3/scoped_diff.py` | **注意：`diff` 子命令不接受 `--out`！**（`scoped_diff.py diff --help` 原文：`本子命令不接受 --out；需要落摘要请用可选的 --summary-out`）⇒ 评审 C6 里「补 `--out`」的写法**已过时**，正确用法是 `--summary-out` |
| C7 | `tools/m2-acceptance/s2c_diff.py` | `--whitelist` 每条必须有 `reason` + `reviewer`（缺一即 RED）；字段口径 `ident/param/tag/series/body_len/body_sha256/raw_hex`。**v2.3**：`--from-recog/--from-x/--from-y/--to-recog` 选帧过滤（C3/C10#2）；`--selftest` 覆盖过滤与 `norm_raw` |
| A4 | `mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/send_ctrlc.ps1` | 存在；`-TargetPid <pid> -Event 1`（CTRL_BREAK）；M1 证据 README 记录 `CTRL_C_EVENT(0)` 不触发、`CTRL_BREAK_EVENT(1)` 触发 `Console.CancelKeyPress` |
| A3 | `tools/m5-acceptance/start_full_rust_stack.ps1` | 存在；起 **Rust** `db-srv`(6000/5100/5700) + **Rust** `login-srv`(5500/5600) + Rust game-svr/三网关；`-Stop` **跳过 MySQL** |
| A5 | `tools/dbsnap/dbsnap.py restore` | 参数是 `--dir` + `--yes-apply`（默认 dry-run）+ 可选 `--allow-db`；**v2.3** snap/restore 增 `--db test|prod|DB` 预设，snap 可选 `--allow-db` 读库白名单；`tables-selftest` 纯表集自检 |
| B11 | `tools/client/account_provision.ps1` | `-Account/-Password/-Birthday/-Mysql/-Prefix/-Count` + **v2.3** `-Db`（默认 `mir2_account`）/`-DbHost`（别名 `-Host`，默认 `127.0.0.1`）；SQL 全部跟随 `-Db`；`-Selftest`（mock mysql，不连库） |

### 0.3b 工具自检（v2.3，离线；reviewer 可在钉住 commit 上原样重跑）

| 工具 | 命令 | 期望 |
| --- | --- | --- |
| `s2c_diff.py` | `python -I -B tools/m2-acceptance/s2c_diff.py --selftest` | 8 条过滤用例全 OK，末行 `SELFTEST GREEN`，rc=0 |
| `check_client_s2c_frames.py` | `python -I -B .../check_client_s2c_frames.py --selftest` | 28 条（含 40B/58B 解码、auto 预设），`SELFTEST GREEN`，rc=0 |
| `account_provision.ps1` | `powershell -NoProfile -ExecutionPolicy Bypass -File tools/client/account_provision.ps1 -Selftest` | 16 条（mock mysql，不连库），`SELFTEST GREEN`，rc=0 |
| `check_db_paths.ps1` | `powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/check_db_paths.ps1 -Selftest` | 9 条参数透传，`SELFTEST GREEN`，rc=0 |
| `dbsnap.py` | `python -I -B tools/dbsnap/dbsnap.py tables-selftest` | 16 条表集展开/白名单，`SELFTEST GREEN`，rc=0 |

> 这些自检**不连库、不起栈、不抓包**；它们只证明「工具判据会咬人」，不替代真机窗口证据。
> 原 `dbsnap.py selftest` / `restore-selftest` 仍需要 MySQL，本批未改其语义。

### 0.4 通用约定

- 两侧串行：C# 与 Rust 不能同时占 5000/7000/7100/7200。每侧起栈前重跑摆位/构造 SQL。
- **SQL 库名规则（硬约束）**：本文件所有 SQL 默认写 **`mir2_test_db` / `mir2_test_account` / `mir2_test_data`**
  （克隆库，见 §E1）。只有 E1 裁决「生产库 + 书面授权」时才把库名替换为 `mir2_db/...`，
  并在证据里记录授权条目。**不得**只凭「有 dbsnap 备份」就写生产库。
- **侧间还原（每步必写）**：`停该侧栈 → dbsnap restore 到本趟 base → 断言行数/关键值 → 再起另一侧栈`。
  「停栈」指 §A4 的优雅停法；还原命令见 §A5。
- **R 卡收口口径**：本文件只产出「窗口证据」；是否 `closed` 由独立 principal 的 `review` 判，
  且 R 卡在 `merge_result` 后仍保持 `in-progress`（`AGENTS.md` §判据分级）。

---

## A. P0 开窗步骤（A1–A5，已按评审改正）

### A1. G0：guard 源/目标必须一致（改正「主检出构建 + 镜像 guard」的错配）

**现状（实跑）**：guard 固定 `repo=/e/tmp/m2svr-window-origin`、`tgt=E:\tmp\m2svr-window-target`；
镜像停在 `f3675f69a`（脏：`M docs/window-shrink-run-20261019.md`），target 目录不存在。按草稿「主检出构建」
必然 R6/R7 红，且 guard 检查的根本不是你构建的那棵树。

**正确步骤（全部在镜像里做）**：

```bash
# 1) 清掉镜像里唯一一处未提交改动（只动 /e/tmp 镜像，不动主检出）
git -C /e/tmp/m2svr-window-origin status --porcelain
#   期望：无输出。若有 ` M docs/window-shrink-run-20261019.md`：
git -C /e/tmp/m2svr-window-origin checkout -- docs/window-shrink-run-20261019.md

# 2) 同步并按当天基线检出（先 fetch，再按 oid 检出；不要沿用旧 oid）
git -C /e/tmp/m2svr-window-origin fetch origin
BASE=$(git -C /e/tmp/m2svr-window-origin rev-parse origin/master)
git -C /e/tmp/m2svr-window-origin checkout --detach "$BASE"
git -C /e/tmp/m2svr-window-origin log -1 --oneline

# 3) 在镜像里重编六件（CARGO_TARGET_DIR 与 guard 的 tgt 完全一致）
cd /e/tmp/m2svr-window-origin/mir2-rs && \
  CARGO_TARGET_DIR=/e/tmp/m2svr-window-target \
  cargo build --offline -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv

# 4) 录出处清单（显式 --ref；`--record-provenance` 只盖章，不校验）
cd /e/Users/gxh/Documents/GitHub/OpenMir2 && python -I -B \
  mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py \
  --repo E:/tmp/m2svr-window-origin --ref "$BASE" \
  --target-dir E:/tmp/m2svr-window-target \
  --record-provenance E:/tmp/m2svr-window-target/window-provenance.json

# 5) 跑 guard（只读、零起栈）
cd /e/Users/gxh/Documents/GitHub/OpenMir2 && \
  bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard
```

- **期望输出**：最后两行 `RESULT: GREEN` + `WINDOW_GUARD_OK`。
- **失败判据**：出现任何 `RED`/`PREREQ_RED` 即拒绝起窗。R6 红 ⇒ 基线/构建源不对；R7 红 ⇒ 没重编或没录出处；
  R8 红 ⇒ 端口被占（见下）。
- **R8 的处理（不由窗口操作者单方完成）**：当前九端口是**外部 C# 全栈**（2026-10-20 12:35 起；实测路径：
  GameSrv=`E:\MirServer\M2GameSvr\GameSrv.exe` pid 9440、DBSrv=`.../src/DBSrv/bin/Release/DBSrv.exe` pid 9964、
  LoginSrv=`.../src/LoginSrv/bin/Release/LoginSrv.exe` pid 22324、三网关=`E:\MirServer\{LoginGate,SelGate,RunGate}`
  pid 3544/3584/18464），**没有 `rust-stack.pid.json`**；`start_rust_stack.ps1 -Stop` 在这里只会 `NOTHING_TO_STOP`。
  必须由**起栈会话/coordinator 按它自己的方式**优雅停（有 pid 文件就用 pid 文件；**当前外部 C# 全栈没有可发现的 pid 文件**
  ——`start-all.ps1` 不写 pid 文件，9440/9964/22324/3544/3584/18464 也都不在任何 `rust-stack.pid.json` / `csharp-stack.pid.json` 里
  ⇒ 只能由该会话/owner 授权停）；C# GameSvr 优先用 §A4 的 `send_ctrlc.ps1`；**禁止 `start-all.ps1 -Stop`**（Force 且连带杀 `mysqld`）。
  `start_rust_stack.ps1 -Stop` 只适用于**该脚本自己起过**的栈。A1 的 R8 项不能由窗口操作者单方清掉。
- **禁止**：在 `E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs` 构建后去跑镜像 guard。要改 guard 的
  `repo=/tgt=` 属于脚本变更，不在窗口授权内。

### A2. 抓包拓扑：必须「停真 gate(7200) → 起影子 gate(7600) → 代理占 7200」

**Rust 侧**（`start_rust_stack.ps1` 会先把 `game-gate` 起在 7200/7201/7202，代理再绑 7200 必失败）：

```powershell
# 1) 起混合栈（C# DBSrv/LoginSrv + Rust game-svr/三网关；共享 MySQL 已在 3306 时不再启动/登记它）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 -LogDir E:/tmp/window/20261021-a/rust/stack

# 2) 只停 7200 那个 Rust game-gate（按 pid 文件逐项停，不碰别的进程）
$pf = Get-Content E:/tmp/window/20261021-a/rust/stack/rust-stack.pid.json -Raw | ConvertFrom-Json
($pf | Where-Object { $_.name -eq 'game-gate' }).pid | ForEach-Object { Stop-Process -Id $_ -Force }

# 3) 造影子 conf：cp936 原样，只改 Count 与 GatePort1 两个数字
$enc = [Text.Encoding]::GetEncoding(936)
New-Item -ItemType Directory -Force E:/tmp/window/20261021-a/shadow | Out-Null
Copy-Item -LiteralPath 'E:\MirServer\RunGate\config.conf' -Destination E:/tmp/window/20261021-a/shadow/game-gate.conf -Force
$t = [IO.File]::ReadAllText('E:/tmp/window/20261021-a/shadow/game-gate.conf', $enc)
$t = $t -replace '(?m)(?<=^Count=)\d+(?=\r?$)','1' -replace '(?m)(?<=^GatePort1=)\d+(?=\r?$)','7600'
[IO.File]::WriteAllText('E:/tmp/window/20261021-a/shadow/game-gate.conf', $t, $enc)
Select-String -Path 'E:/tmp/window/20261021-a/shadow/game-gate.conf' -Pattern '^Count=|^GatePort1='

# 4) 起影子 Rust game-gate(7600)：Rust 直接吃 conf 路径参数
$shadowGate = Start-Process -FilePath E:/tmp/m2svr-window-target/debug/game-gate.exe `
  -ArgumentList 'E:/tmp/window/20261021-a/shadow/game-gate.conf' `
  -WorkingDirectory E:/tmp/window/20261021-a/shadow -WindowStyle Hidden `
  -RedirectStandardOutput E:/tmp/window/20261021-a/shadow/game-gate.out.log `
  -RedirectStandardError  E:/tmp/window/20261021-a/shadow/game-gate.err.log -PassThru
$shadowGate.Id | Set-Content E:/tmp/window/20261021-a/shadow/rust-gate.pid

# 5) 断言：7200 必须 DOWN、7600 必须 UP（port-status.ps1 的十端口**不含 7600**，必须单独断言）
[System.Net.NetworkInformation.IPGlobalProperties]::GetIPGlobalProperties().GetActiveTcpListeners() |
  Where-Object { $_.Port -in 7200,7600 } | Format-Table Address,Port

# 6) 起代理占官方 7200
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/capture/mir2_proxy.py `
  --out E:/tmp/window/20261021-a/rust/proxy --map 7200=127.0.0.1:7600
```

**C# 侧**（同样先停 7200，影子必须是**整目录拷贝**：C# GameGate 按 `AppContext.BaseDirectory` 读 `config.conf`）：

```powershell
# 0) fail-closed：旧影子目录存在就归档改名，否则拒绝起窗
#    （不要复用 E:/MirServer/M2GameSvr-hook：那里 GameSrv.exe 停在 628d8024，不含 d7ead5b2a）
$shadowDir = 'E:/tmp/window/20261021-a/csharp/GameSvr-shadow'
if (Test-Path $shadowDir) {
  $bak = "$shadowDir.bak-" + (Get-Date -Format 'yyyyMMdd-HHmmss')
  Move-Item -LiteralPath $shadowDir -Destination $bak -ErrorAction Stop
  Write-Output "ARCHIVED_OLD_SHADOW $bak"
}
if (Test-Path $shadowDir) { throw 'RED: 旧影子仍存在，拒绝起窗（复用旧 GameSrv.exe 会重现 F1）' }
Test-Path $shadowDir    # 期望 False

# 1) 起 C# 影子栈：**显式给全新 -Shadow**（脚本 :96-104 只在 GameSrv.exe 不存在时才 robocopy）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/start-csharp-hook.ps1 `
  -Seed 42 -LogDir E:/tmp/window/20261021-a/csharp/stack `
  -Shadow E:/tmp/window/20261021-a/csharp/GameSvr-shadow

# 1b) 记录 oracle 影子身份：git_head 必须含 d7ead5b2a，否则停窗
$identity = 'E:/tmp/window/20261021-a/csharp/GameSvr-shadow-identity.txt'
$dep = Get-Content E:/tmp/window/20261021-a/csharp/GameSvr-shadow/!deployed-from.json -Raw | ConvertFrom-Json
"git_head=$($dep.git_head)" | Tee-Object -FilePath $identity
git -C E:/Users/gxh/Documents/GitHub/OpenMir2 merge-base --is-ancestor d7ead5b2a $dep.git_head
"merge-base rc=$LASTEXITCODE" | Tee-Object -FilePath $identity -Append
Get-FileHash E:/tmp/window/20261021-a/csharp/GameSvr-shadow/GameSrv.dll, E:/tmp/window/20261021-a/csharp/GameSvr-shadow/M2Server.dll -Algorithm SHA256 |
  Format-List Path,Hash | Out-String | Tee-Object -FilePath $identity -Append
# 期望：identity 文件里 merge-base rc=0 且含两个 hash；rc!=0 立即停窗

# 2) 只停它起的 RunGate（7200）
$pf = Get-Content E:/tmp/window/20261021-a/csharp/stack/csharp-stack.pid.json -Raw | ConvertFrom-Json
($pf | Where-Object { $_.name -eq 'RunGate' }).pid | ForEach-Object { Stop-Process -Id $_ -Force }

# 3) 整目录影子 + 换 conf（runbook 82 §4.0/§4.1 的原样做法）
Remove-Item -Recurse -Force E:/tmp/window/20261021-a/shadow/run-gate -ErrorAction SilentlyContinue
Copy-Item -Recurse -Force 'E:\MirServer\RunGate' 'E:/tmp/window/20261021-a/shadow/run-gate'
Copy-Item -Force E:/tmp/window/20261021-a/shadow/game-gate.conf E:/tmp/window/20261021-a/shadow/run-gate/config.conf

# 4) 起影子 C# GameGate(7600)
$shadowGate = Start-Process -FilePath 'E:/tmp/window/20261021-a/shadow/run-gate/GameGate.exe' `
  -WorkingDirectory 'E:/tmp/window/20261021-a/shadow/run-gate' -WindowStyle Hidden `
  -RedirectStandardOutput E:/tmp/window/20261021-a/shadow/csharp-gate.out.log `
  -RedirectStandardError  E:/tmp/window/20261021-a/shadow/csharp-gate.err.log -PassThru
$shadowGate.Id | Set-Content E:/tmp/window/20261021-a/shadow/csharp-gate.pid

# 5) 断言 7200 DOWN / 7600 UP（同 Rust 侧那条 .NET 监听表命令）
# 6) 代理占 7200（同 Rust 侧）
```

- **开窗前**：C# 抓包前置把 `--shadow-config` 指向**影子** conf，期望 `RESULT: GREEN`：
  `python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_csharp_capture_preflight.py --repo E:/Users/gxh/Documents/GitHub/OpenMir2 --mirserver E:/MirServer --shadow-config E:/tmp/window/20261021-a/shadow/run-gate/config.conf`
  > 该门只查**真部署** `E:/MirServer/M2GameSvr/!deployed-from.json`（当前 `8a621f6a`），**不查影子**；
  > 影子的 `git_head` + DLL hash 必须由 step 1b 单独落证据。
- **收窗**（草稿漏项）：先停代理，再按 pid 文件停 **7600 影子 gate**
  （`Stop-Process -Id (Get-Content E:/tmp/window/20261021-a/shadow/rust-gate.pid)`；
  C# 侧同理读 `csharp-gate.pid`），最后才 `start_rust_stack.ps1 -LogDir E:/tmp/window/20261021-a/rust/stack -Stop` /
  `start-csharp-hook.ps1 -LogDir E:/tmp/window/20261021-a/csharp/stack -Stop`。
  `port-status.ps1` 看不到 7600 ⇒ 必须用 step 5 的监听表断言 **7600 DOWN**。
- **失败判据**：代理启动即报 `address in use`（7200 没释放）；7200 仍被 gate 监听；7600 无监听；
  客户端登录后进不了世界（拓扑错）。

### A3. G5/G6 的被测栈：C# DBSrv ≠ Rust db-srv（改正「用混合栈测 Rust 持久化」）

| 段 | 被测对象 | 起栈命令 | 能收什么 |
| --- | --- | --- | --- |
| **G5a** | Rust **game-svr** 的存档/耐久/关服 | `start_rust_stack.ps1`（混合栈：C# DBSrv/LoginSrv） | game-svr 行为；D-12 的 oracle 侧关服 |
| **G5b / G6** | **Rust db-srv** 的取档/存档/故障路径（1100/1102、520/524/522/525） | `start_full_rust_stack.ps1`（全 Rust 六件） | Rust db-srv 的行为；C# 对照仍需另一趟混合栈 |

```powershell
# 全 Rust 栈（G5b/G6）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m5-acceptance/start_full_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 `
  -StackRoot E:/tmp/window/20261021-a/full -LogDir E:/tmp/window/20261021-a/full/logs
# 收窗：同一个 -LogDir；-Stop 只停本脚本起的进程，**跳过 MySQL**
powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m5-acceptance/start_full_rust_stack.ps1 `
  -LogDir E:/tmp/window/20261021-a/full/logs -Stop
```

- **provenance 口径**：G0 的六件 Rust provenance 覆盖**全 Rust 栈**；混合栈只运行其中四件，
  其 6000/5100/5500/5600 由 **C# DBSrv/LoginSrv** 提供，**不被六件 provenance 覆盖**（见 §E4）。
- **G6 的 MySQL 停/启**是本机破坏性动作，且 `mysqld` 不是 Windows 服务（`Get-Service` 查不到）——
  该动作**只**作为 §E3 的 owner 决策占位，**不得**写进可执行步骤。
- **G6 的 6000/5100 帧面**：客户端代理（7200）看不到；Rust db-srv 只打连接/错误日志，不打逐帧。
  ⇒ 帧级判据**未闭环**（§C9），G6 本窗不收。

### A4. C# 关服：必须走「优雅停机」，且绝不碰共享 MySQL

草稿用 `E:/MirServer/start-all.ps1 -Stop`：`Stop-Stack`（`start-all.ps1:112-120`）对
`LoginGate/SelGate/GameGate/GameSvr/LoginSrv/DBSrv/mysqld` 一律 `Stop-Process -Force`，**`OnShutdown/SavePlayer` 不会跑**，
还会把共享 `mysqld` 一起停掉。**本文件禁止使用该命令。**

```powershell
# C# 优雅停 GameSvr：CTRL_BREAK → Console.CancelKeyPress（Program.cs:16）→ cts.Cancel → StoppingAsync → OnShutdown
$gsvr = Get-Process GameSvr -ErrorAction Stop | Sort-Object StartTime | Select-Object -Last 1
powershell -NoProfile -ExecutionPolicy Bypass -File `
  E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/send_ctrlc.ps1 `
  -TargetPid $gsvr.Id -Event 1
$gsvr.WaitForExit(60000) | Out-Null
Get-Process -Id $gsvr.Id -ErrorAction SilentlyContinue   # 期望：无输出（已退出）
```

- **期望证据**：GameSvr 日志出现 `Ctrl+C pressed`（`Program.cs:18`）；随后 `AppService.OnShutdown`（`AppService.cs:206`）
  → `SavePlayer`；DBSrv 日志出现存档请求/应答；`dbsnap` before/after 有可判差异。
- **不得**用 `start-csharp-hook.ps1 -Stop` 停 GameSvr（该脚本 `:81` 也是 `Stop-Process -Force`）——
  它只用于 GameSvr 已经优雅退出后的网关/DBSrv/LoginSrv 清理。GameSvr 退出后**至少等 3 秒**再清理，
  让最后一次 DBSrv 存档应答/日志落完（否则会把「没等到回执」误判成「没落库」）。
- **MySQL**：`start_rust_stack.ps1` / `start_full_rust_stack.ps1` 只在 3306 **原本 DOWN** 时才启动 MySQL；
  **条件结论（不是一般结论）**：3306 开工前已 UP ⇒ 起栈器不登记 MySQL（`start_rust_stack.ps1:374-377`），
  `-Stop` 不会杀它；**若中途由起栈器拉起过 MySQL**（3306 原本 DOWN），则**禁用 `start_rust_stack.ps1 -Stop`**
  （它会按 `:193-215` 对登记项 `Stop-Process -Force`，包括 MySQL），改用 `start_full_rust_stack.ps1 -Stop`
  （其 MySQL 条目跳过不杀）或按 §E3 授权手工停。**不要**用 `start-all -Stop`。
- **D-12 的预期不是既定的**：M1 证据（`mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/README.md` §6，`M1-EXIT-2`）
  记录该 shutdown 场景两侧**都没有落库**。所以 G5a 的判据是「**玩家在世界中**时 oracle 是否真落库」，
  必须现场采，不能拿 M1 的旧结论替代。

### A5. DB 快照/还原：先由 §E1 决定「生产组」还是「测试库组」

**A5-P（E1-b：生产库 + 书面授权）**

```bash
# 快照（停栈后做，避免边写边拍）
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= snap \
  --out E:/tmp/window/20261021-a/db/base

# 还原：先 dry-run 看计划
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore \
  --dir E:/tmp/window/20261021-a/db/base
# 期望：`RESTORE PLAN: N 表 / M 行 / 目标库 [...] <- ...` + `DRY-RUN: 未执行任何写库（加 --yes-apply 才真正灌库）`

# 真正写库
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore \
  --dir E:/tmp/window/20261021-a/db/base --yes-apply
```

**A5-T（E1-a：`mir2_test_*` 克隆）**——`dbsnap` 默认表集是**生产库名**；v2.3 起可用 `--db test`
一次展开 13 张测试库表（等价于下面逐表指定）：

```bash
# 推荐（C10#9 闭环）：一条 --db 展开 mir2_test_account(2) + mir2_test_db(11)
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= snap \
  --out E:/tmp/window/20261021-a/db/base-test --db test

# 等价的手工逐表写法（保留作对照）

python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= snap \
  --out E:/tmp/window/20261021-a/db/base-test \
  --table mir2_test_account.account --table mir2_test_account.account_protection \
  --table mir2_test_db.characters --table mir2_test_db.characters_ablity \
  --table mir2_test_db.characters_bonusability --table mir2_test_db.characters_item \
  --table mir2_test_db.characters_item_attr --table mir2_test_db.characters_magic \
  --table mir2_test_db.characters_bagitem --table mir2_test_db.characters_storageitem \
  --table mir2_test_db.characters_status --table mir2_test_db.characters_quest \
  --table mir2_test_db.characters_indexes

# dry-run 也要带 --allow-db（白名单在生成计划前就校验）
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore \
  --dir E:/tmp/window/20261021-a/db/base-test \
  --allow-db mir2_test_db --allow-db mir2_test_account --allow-db mir2_test_data

python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore \
  --dir E:/tmp/window/20261021-a/db/base-test --yes-apply \
  --allow-db mir2_test_db --allow-db mir2_test_account --allow-db mir2_test_data
```

> **由 §E1 裁决选哪一组**：E1-b 用 A5-P；E1-a 用 A5-T。两组不可混用；A5-T 还要求服务端确实指向测试库（§C10#8）。


- **`--yes-apply` 语义**：没有它只打印计划、**一行都不写**（`dbsnap.py::cmd_restore`）；「收窗跑过 restore」不等于回滚成功，
  必须核对 rc 与行数。
- **`--allow-db`**：默认只允许 `mir2_db/mir2_account/mir2_data`；灌 `mir2_test_*` 必须
  `--allow-db mir2_test_db --allow-db mir2_test_account --allow-db mir2_test_data`（可重复）。
  **只跑 A5-P 时绝不加 `--allow-db`**（默认白名单就是生产三库）。
- **禁止**在建栈已停 MySQL 之后执行 restore（连不上库）。
- **失败判据**：rc≠0；`RESTORE PLAN` 的表/行数与 base 清单不符；灌完断言行数不符。

---

## B. 场景构造（B1–B12，逐条改正）

> 本节所有 SQL 默认针对 `mir2_test_*`（§E1）。执行前先做 §A5 的 base 快照；
> 每步结束写「侧间还原」，否则下一侧看到的不是同一输入。

### B1. `characters_item` 基线不是「DELETE 后自然 13 空槽」，要显式 INSERT 13 行

**改正**：现库 `aaa` 的 `characters_item` 是 **0 行**（评审 B1）。DELETE 只会得到 0 行，不是 13 空槽。
表只有 7 列（`sql/mir2_db.sql:293929`：`Id/PlayerId/Position/MakeIndex/StdIndex/Dura/DuraMax`），
空槽 = `MakeIndex=0, StdIndex=0, Dura=0, DuraMax=0`。

```sql
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='aaa');
DELETE FROM mir2_test_db.characters_item WHERE PlayerId=@pid;
INSERT INTO mir2_test_db.characters_item (PlayerId,Position,MakeIndex,StdIndex,Dura,DuraMax) VALUES
(@pid,0,0,0,0,0),(@pid,1,0,0,0,0),(@pid,2,0,0,0,0),(@pid,3,0,0,0,0),
(@pid,4,0,0,0,0),(@pid,5,0,0,0,0),(@pid,6,0,0,0,0),(@pid,7,0,0,0,0),
(@pid,8,0,0,0,0),(@pid,9,0,0,0,0),(@pid,10,0,0,0,0),(@pid,11,0,0,0,0),(@pid,12,0,0,0,0);
SELECT COUNT(*) AS n, COUNT(DISTINCT Position) AS slots, MIN(Position), MAX(Position)
  FROM mir2_test_db.characters_item WHERE PlayerId=@pid;   -- 期望 13 / 13 / 0 / 12
```

- **失败判据**：`n!=13` 或 `slots!=13` 或 `MIN/MAX != 0/12` ⇒ 不得登录采数。
- **侧间还原**：停栈 → `restore --dir E:/tmp/window/20261021-a/db/base --yes-apply` → 再断言 13 行。

### B2. 新号 52 的 Weight 是 **14**，不是 10

**改正**：部署 `Server.conf:81-82` 新手件 = 蜡烛 + 金创药(小量)；再加木剑 + 布衣(男)。
`stditems` 权重：蜡烛 `1`（Id 48）、金创药小量 `1`（Id 3）、木剑 `7`（Id 207）、布衣男 `5`（Id 97）⇒ **合计 14**。
`10` 只存在于构造级单测（`design-numeric-gaps-20261019.md:68-74` 人为赋 `1/1/5/3`）。
`MaxExp=10` 仍然正确。

- 构造：账号**在 LoginSrv 启动前**建好（B11）；`mir_flow.ps1 -CreateChar -CharName g1new` 建角；
  首登采 52 的 `Weight/MaxExp`。
- **失败判据**：Weight=10 ⇒ 采到了构造级夹具而不是部署值；Weight!=14 ⇒ 新手件清单/权重与部署不符。
- **工具已补齐（v2.3，C10#1 闭环）**：`check_client_s2c_frames.py` 按 `body_len` 自动识别 40B(oracle40)/
  58B(client58) 布局并解出 `Weight/MaxExp`，直接机判 B2：
  ```bash
  python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_client_s2c_frames.py \
    --capture E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
    --expect-enter-seq auto --expect-52-layout client58 \
    --expect-52-weight 14 --expect-52-max-exp 10
  ```
  期望末行 `RESULT: GREEN`；`Weight=10`（构造级夹具）或 `!=14`（新手件清单不符）都必红。
  **必须钉 `--expect-52-layout client58`**：40B oracle 布局同样能解出 `Weight=14/MaxExp=10`，
  不钉布局则布局回退（本该 58B 却按 40B 解）不会被咬。
  （`dump_body_layout.py` 仍只切段；布局口径以 `record.rs::encode_ability_client` 为准。）
- **侧间还原**：删 `characters/characters_ablity/characters_item/characters_magic` 里该新角行（或 restore base）。

### B3. HumItems 期望是 **load/save 均 1:1**，不是「save 仍交换」

**改正**：C# save 侧已改成与 load 对齐（`src/GameSrv/Word/WorldServer.cs:1320-1324` 注释与代码：项链/头盔各取本槽）；
Rust 断言也是 1:1（卡 work 记录 `a84c81aae + d7ead5b2a`）。旧 issue 文本里的「save 仍交换」是历史漂移，**只能做负例**。

- **前置断言（F1）**：oracle 影子 `!deployed-from.json.git_head` 必须含 `d7ead5b2a`
  （A2 step 1b 已记录）；否则 `WorldServer.cs` 仍是旧的交叉交换实现 ⇒ 本项必红，**先停窗**。
- 摆位：槽 3 = 金项链 `StdIndex=298`，槽 4 = 青铜头盔 `StdIndex=192`（`ItemLocation.Necklace=3/Helmet=4`，
  `src/OpenMir2/Enums/ItemLocation.cs`）。可照抄 SQL（`@pid` 同 B1）：
  ```sql
  SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='aaa');
  UPDATE mir2_test_db.characters_item
     SET MakeIndex=900003, StdIndex=298, Dura=8000, DuraMax=8000
   WHERE PlayerId=@pid AND Position=3;
  UPDATE mir2_test_db.characters_item
     SET MakeIndex=900004, StdIndex=192, Dura=8000, DuraMax=8000
   WHERE PlayerId=@pid AND Position=4;
  ```
- 判据 SQL（登录前 / 小退后各一次）：
  ```sql
  SELECT Position,StdIndex,Dura,DuraMax FROM mir2_test_db.characters_item
   WHERE PlayerId=@pid AND Position IN (3,4) ORDER BY Position;   -- 期望 3→298、4→192，两次相同
  ```
- **失败判据**：任一时刻 3→192 或 4→298（交换）；或小退后槽位迁移。
- **侧间还原**：恢复 13 空槽（B1）。

### B4. 行会文件在 `GuildBase/`，不在 `Envir/`；且不要手写文件格式

**改正**：`E:/MirServer/M2GameSvr/Server.conf:61-62` 为 `GuildDir=GuildBase/Guilds`、
`GuildFile=GuildBase/Guildlist.txt`；`GuildBase/GuildList.txt` **已存在（0 字节）**，`GuildBase/Guilds/` 不存在。
行会文件名大小写以 conf 为准（`Guildlist.txt`）。

- **推荐构造**（避免手写格式）：用游戏内 GuildOfficial 的建会对话，让**服务端自己**写
  `GuildBase/Guildlist.txt` + `GuildBase/Guilds/<会名>.txt`（`src/M2Server/Guild/GuildManager.cs:15-32`
  → `AddGuild` → `SaveGuildList`；`GuildOfficial.cs:142`）。
- **若要预置影子包**：`start-csharp-hook.ps1:96-104` 只在 `<Shadow>/GameSrv.exe` **不存在**时才 robocopy。
  正确做法是**先准备一份带 GuildBase 内容的 `-Pack` 源目录**再起栈；不要「拷完再改 -Shadow」（删影子会丢）。
  内容改动属 §E2，未裁决前**不做**。
- **失败判据**：`750` 不是恰 1 条；或不在 `46 < 750 < 654`；或无会角色也出 750。
- **侧间还原**：停栈 → 删除/恢复影子 GuildBase 文件 → restore base。

### B5. delmagic/addmagic 必须拆成两轮 DB 状态，不能一次登录混采

**改正**（评审 B5）：`openmir2-delmagic-addmagic-chain` 要求「粘滞 / 每轮重置 / 210、212 在 658 后 51 前」，
`openmir2-login-useitems-magic` 要求登录 621/211；两者输入不同，必须分轮。

| 轮 | `characters_magic` | 戒指 | 期望 |
| --- | --- | --- | --- |
| R1 | 空 | 槽 7 = 火焰戒指 `StdIndex=354`（Shape=115），`Dura>0` | 登录出 `210 SM_ADDMAGIC`；`658` 之后、`51` 之前 |
| R2 | 空 | **无戒指**（或 `Dura=0`） | 出 `212 SM_DELMAGIC`（oracle 实测会把不可用技能删掉并落库）；粘滞对照 |
| R3 | 一条有效行（战士 `MagicId=3` 基本剑术，`Job=0`，`NeedL1=7`；角色 Level≥7） | 无 | 登录出 `211 SM_ADDMAGIC`（配合 621） |

> **改正草稿的 MagicId**：`MagicId=1` 是**火球术，Job=1（法师）**，战士 `aaa` 不可学。战士用 `MagicId=3`；
> `magics` 表：3 的 `Job=0/NeedL1=7`；14/15 是 Job=2；1/8/31/37 是 Job=1（`sql/mir2_data.sql` 的 `magics` 行）。

可照抄 SQL（每轮前先 restore base；`@pid` 同 B1）：

```sql
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='aaa');
-- R1：无技能 + 火焰戒指（Slot7）
DELETE FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;
UPDATE mir2_test_db.characters_item
   SET MakeIndex=900005, StdIndex=354, Dura=5000, DuraMax=5000
 WHERE PlayerId=@pid AND Position=7;

-- R2：无技能 + 无戒指（Slot7 清空）
DELETE FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;
UPDATE mir2_test_db.characters_item
   SET MakeIndex=0, StdIndex=0, Dura=0, DuraMax=0
 WHERE PlayerId=@pid AND Position=7;

-- R3：一行有效技能（基本剑术），无戒指；角色 Level>=7
DELETE FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;
INSERT INTO mir2_test_db.characters_magic (PlayerId,MagicId,Level,UseKey,CurrTrain)
VALUES (@pid,3,3,'F1',0);
UPDATE mir2_test_db.characters SET Level=30 WHERE ChrName='aaa';
UPDATE mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id
   SET a.Level=30 WHERE c.ChrName='aaa';
```

- **失败判据**：210/212 不在 `658` 后 `51` 前；R2 未出 212；R3 未出 211；技能行下线后未删除。
- **侧间还原**：每轮前 restore base → 摆本轮状态 → 再登录；轮间必须停栈。

### B6. 法术不能用战士 `aaa` 采：按职业/等级分别建角色

**改正**（评审 B6）：`magics` 表 Job/NeedL1：`aaa` 是 `Job=0`、`Level=1`，31/37 连学习门都过不了，G4 采不到成功路径。

| 用途 | 角色要求 | 覆盖 |
| --- | --- | --- |
| 法师技能 | `characters.Job=1`、`characters.Level>=50`（`characters_ablity.Level` 同步） | 1(NeedL1=7)、8(12)、31(31)、37(50) |
| 道士技能 | `Job=2`、`Level>=25` | 2(7)、14(22)、15(25) |

```sql
-- 两处 Level 都要改：characters.Level 与 characters_ablity.Level（后者驱动 RecalcAbilitys）
UPDATE mir2_test_db.characters SET Job=1, Level=50 WHERE ChrName='g4mage';
UPDATE mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id
  SET a.Level=50 WHERE c.ChrName='g4mage';
-- 道士同形：Job=2, Level=25（g4tao）
```

- 证据里必须写角色 `Job/Level`。
- **仍不可收**：没有法术按键驱动（`run_attack_pair` 只发 `CM_HIT`；BotSrv `AutoSpell` 属 W9），
  本窗只能把「可施放的前置」摆对，**不能**采逐抽/帧序 ⇒ G4 标 `not_collectible`（§D）。
- **侧间还原**：restore base。

### B7. G3 拆成 G3a/G3b/G3c；没有构造的卡不进窗口

**改正**（评审 B7）：草稿单组 60 分钟/侧装不下 26 张卡。拆分：

- **G3a（单客户端，约 50 分钟/侧）**：`(605,578)` 攻击计数、`(291,666)` GuardList、`(311,666)` 反向点、
  入站受击、`Dir` 变化、AOI 可见性（`check_aoi_visibility_diag`）。
- **G3b（多客户端/多账号，2–3 个客户端）**：3 人同格（`run-gap-checkdupobj`）、±12 观察者
  （`disappear-recipients-gridscan`）、玩家被玩家/宝宝击杀（`player-drop-useitems`/T-81）。**本窗不排**。
- **G3c（长生命周期，30–60 分钟等待）**：3 分钟变尸/5 分钟移除（`corpse-ghost-revive`）、
  BeeQueen/SpiderHouse（`monster-specials`）、宝宝跟随/异图（`monster-master-follow`）。**本窗不排**。
- **从窗口移除（标 `not_collectible`）**：`movetomovingobject-quirk`（多对象同格属 G3b）、
  `run-gap-pr1-head-blocking`（500 场景差分 0 已构造级覆盖）、`monster-state-expiry`（FireHit/TwinHit
  写入点依赖未移植技能层）。
- **摆位 SQL**（起栈前重跑；两侧同值）：
  ```sql
  UPDATE mir2_test_db.characters SET MapName='0', CX=605, CY=578 WHERE ChrName='aaa';
  UPDATE mir2_test_db.characters SET Level=30 WHERE ChrName='aaa';
  UPDATE mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id
    SET a.Level=30 WHERE c.ChrName='aaa';
  -- 压血：Hp=MaxHP-2（不要直接改 MaxHP，会被 RecalcAbilitys 覆盖）
  UPDATE mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id
    SET a.Hp=a.MaxHP-2 WHERE c.ChrName='aaa' AND a.MaxHP>=3;
  ```
  **两侧起手 HP 必须相同**（`monster-attack-cadence-lead` 的「HP 未控」整改）。
- **侧间还原**：停栈 → restore base → 对另一侧重跑同一 SQL → 断言 `MapName/CX/CY/Level/Hp/MaxHP` 两侧一致。

### B8. 物品/NPC 卡没有触发动作：要么补动作，要么不计入可收

**改正**（评审 B8）：`-SkipTalk` 只是**跳过**聊天，不会触发 NPC 对话；登录本身不穿/卸/拾取/买卖。

| 卡 | 需要的动作 | 现有可执行路径 | 本窗结论 |
| --- | --- | --- | --- |
| `openmir2-sm-merchantsay-recog` | NPC 对话（`CM_MERCHANTSAY`） | 真客户端右键点 NPC（`mir_click.ps1 -RightClick x,y`）后选对话；或 `run_npc_shop_pair.ps1 -Side ... -Go`（假人序列） | **部分/需 E1+独立编排** |
| `openmir2-sm-updateitem` | 穿/换装 | `run_bag_equip_pair.ps1 -Side ... -Go`（BotSrv 固定序列 `CM_TAKEONITEM/CM_TAKEOFFITEM/...`） | **部分/需 E1** |
| `openmir2-sm-additem-tail` | 卸下/换装/商店/地面拾取 | 同上 | **部分/需 E1** |
| `openmir2-login-useitems-magic` | 登录带装备/技能 | B5 R3 | 可采（见 B5） |

- **不许**把「只登录」的 capture 当这四张卡的证据。`mir_flow.ps1 -SkipTalk` 保留（它本来就不做 NPC 对话）。
- **侧间还原**：每次动作序列后 restore base（BotSrv 会写库/写背包）。

### B9. 入图序判据必须用 `--expect-enter-seq`（G2 的 621/211 场景）

**改正**（评审 B9）：`check_client_s2c_frames.py:28` 的默认 15 帧不含 621/211；有装备/技能时必红。现已支持：

```bash
# G1（无装备/无技能）
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_client_s2c_frames.py \
  --capture E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --expect-enter-seq default --expect-52-body-len 58
# G2（有武器/技能：621 + 可选 211）
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_client_s2c_frames.py \
  --capture E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --expect-enter-seq equipped --expect-52-body-len 58
# G2 delmagic 轮（可选 212）：equipped 不收 212，用 delmagic 或 g2
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_client_s2c_frames.py \
  --capture E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --expect-enter-seq delmagic --expect-52-body-len 58
```

- C# 侧同命令，`--expect-52-body-len 40`（oracle 布局）。
- **失败判据**：`RESULT: RED`（序列不符/52 缺失/52 尾长不符）；**不得**拿默认预设去判 G2。

### B10. `TestServer=TRUE` 的部署收不全「非 TestServer」余量

**改正**（评审 B10）：`E:/MirServer/M2GameSvr/Server.conf:11-12` 是 `TestServer=TRUE/TestLevel=1`；
`openmir2-login-frame-order-52-752` 的余量还含非 TestServer 部署、发光装备、`IsSpirit` 槽位变化。

- 本窗**只**能收 TestServer 部署的样本 ⇒ 该卡标 **部分收**。
- 非 TestServer 影子需要改内容/起影子 GameSvr 树（`start-csharp-hook.ps1 -Pack` 可以换源目录，
  但整棵 Envir 与 `Server.conf` 都要影子化）⇒ 属 §E2，未裁决前不做。

### B11. 新账号必须在 LoginSrv **启动前**建好（或重启 LoginSrv）

**改正**（评审 B11）：`mir2-rs/tests/parity/m3/README.md:505`：`AccountStorage.Index()` 查的是启动时的内存账号表；
`account_provision.ps1` 直接写 `mir2_account`，之后新建的号看不到。

- **首选（E1-a，测试库）**：`account_provision.ps1` 自 v2.3 起支持 `-Db/-DbHost`（C10#7 闭环）；
  起任何栈之前可直接：
  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/account_provision.ps1 `
    -Account g1new -Password g1new -Db mir2_test_account -DbHost 127.0.0.1
  ```
  若不想用脚本，仍可执行下面的等价 SQL（两条都必须落测试库）：
  ```sql
  INSERT INTO mir2_test_account.account
    (Account,PassWord,PayMode,Seconds,State,CreateTime,ModifyTime,LastLoginTime)
  VALUES ('g1new','g1new',0,0,0,UNIX_TIMESTAMP(),UNIX_TIMESTAMP(),0);
  INSERT INTO mir2_test_account.account_protection
    (AccountId,UserName,Birthday,Quiz1,Answer1,Quiz2,Answer2)
  SELECT Id,Account,'1986/06/06','q1','a1','q2','a2'
    FROM mir2_test_account.account WHERE Account='g1new';
  -- 断言行齐：SELECT COUNT(*) ... WHERE Account='g1new';  -- 期望 1
  ```
  前提：LoginSrv 已指向测试库（§C10#8；否则它读的仍是生产 `mir2_account`）。
- **仅 E1-b（生产书面授权）**：才用
  `powershell -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/client/account_provision.ps1 -Account g1new -Password g1new`。
  不论哪种，账号都要在 LoginSrv 启动前建好 ⇒ 无需重启。
- **若中途开号**：优雅停 LoginSrv 再起（不要 Force）：
  ```powershell
  $pf = Get-Content E:/tmp/window/20261021-a/rust/stack/rust-stack.pid.json -Raw | ConvertFrom-Json
  $ls = ($pf | Where-Object { $_.name -eq 'LoginSrv' })
  powershell -NoProfile -ExecutionPolicy Bypass -File E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/evidence/M1/s4b-slice5-exitpaths/send_ctrlc.ps1 -TargetPid $ls.pid -Event 1
  Start-Sleep -Seconds 3
  Start-Process -FilePath 'E:/Users/gxh/Documents/GitHub/OpenMir2/src/LoginSrv/bin/Release/LoginSrv.exe' `
    -WorkingDirectory 'E:/Users/gxh/Documents/GitHub/OpenMir2/src/LoginSrv/bin/Release' -WindowStyle Hidden `
    -RedirectStandardOutput E:/tmp/window/20261021-a/rust/stack/LoginSrv.out.log `
    -RedirectStandardError  E:/tmp/window/20261021-a/rust/stack/LoginSrv.err.log
  ```
  断言 5500/5600 重新监听后再登录。
- **账号/角色名 ≤10 字符**（客户端登录框截断，实测）。C#/Rust 两侧用不同名（B12）。
- **失败判据**：登录报「此帐号不存在」⇒ 账号没进内存表。

### B12. G5 两侧共用 DB：必须侧间还原 + 各自角色名

**改正**（评审 B12）：C# 小退/关服会写回 `characters/characters_ablity/characters_item`，Rust 侧看到的已不是同一输入。

- C# 角色 `g5c`、Rust 角色 `g5r`（各 ≤10 字符）；两侧都从**同一 base 快照**起。
- 每侧登录前记录字段快照：
  ```sql
  SELECT c.ChrName,c.MapName,c.CX,c.CY,c.Level,a.Hp,a.MaxHP,a.Exp,a.MaxExp
    FROM mir2_test_db.characters c JOIN mir2_test_db.characters_ablity a ON a.PlayerId=c.Id
   WHERE c.ChrName IN ('g5c','g5r');
  SELECT PlayerId,Position,MakeIndex,StdIndex,Dura,DuraMax
    FROM mir2_test_db.characters_item
   WHERE PlayerId IN (SELECT Id FROM mir2_test_db.characters WHERE ChrName IN ('g5c','g5r'))
   ORDER BY PlayerId,Position;
  ```
- **侧间还原**：C# 侧跑完 → 停栈 → restore base → 再起 Rust 侧（或反之）。**不许**串行共用未还原的库。

---

## C. 判据与工具（C1–C9，确切命令 / 期望 / 失败）

> 统一输出口径：判据脚本退出码 `0=GREEN / 1=RED / 2=PREREQ`；文本末行 `RESULT: GREEN|RED|PREREQ_RED`。
> 所有 capture 均由
> `powershell -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/export_s2c.ps1 -ProxyDir E:/tmp/window/20261021-a/rust/proxy -OutDir E:/tmp/window/20261021-a/rust/s2c`
> 产出 `capture.jsonl`（字段含 `ts_ms/ident/dir/conn/port/hop`）。

### C1. `SM_HIT(14)` 帧级：头字段/收件人/250ms 前导（已闭环）

```bash
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_sm_hit.py \
  --capture E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --expect count,order --min-count 1 --lead-ms 250 \
  --expect-recipients count=1
# 已知右键朝向时，可再断言具体攻方/方向：
#   --expect-hit <attackerActorId>,<x>,<y>,<dir>   或 --expect-dir <0..7>
```

- **期望输出**：`sm_hit=N sm_struck=M order_pairs>=1 recipients=[...]`；末行 `RESULT: GREEN`。
- **失败判据**：`sm_hit=0`；`order_pairs=0`；任一 `SM_HIT` 的 Series 越界（0..7）；收件人数与期望不符；
  `RESULT: RED`。
- **边界**：`--require-each-struck` 默认关——怪物未命中/多点攻击常无前导，开了会假红；
  只有「每条受击都应有同源命中」的场景才开。
- 状态：**已闭环**（工具 `--selftest` 15 条用例全绿；末行另打 `SELFTEST GREEN`）。

### C2. 入站 `sm_struck>=1` 用 `--expect incoming`，不用 `any`

```bash
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_attack_observability.py \
  --capture E:/tmp/window/20261021-a/rust/s2c/capture.jsonl --expect incoming --min-struck 1
```

- **期望**：`facts["sm_struck_incoming"] >= 1`；末行 `RESULT: GREEN`。
- **失败判据**：`--expect incoming` 下 rc=2（解析不到 self）或 rc=1（空转）；**不得**加 `--allow-unknown-self`
  （工具注明仅诊断，正式判据禁用）。
- 双两侧对照（只作信息）：`--left <rust> --right <csharp> --expect incoming`。
- 状态：**已闭环**。

### C3. GuardList 帧一致：`SM_TURN(281,661)+SM_FEATURECHANGED` 的特定位点

```bash
# 主判据（两侧各跑一次；C# capture 为 oracle）
# **跨侧只能用坐标定位**：两侧 ActorId 命名空间不同——C# 的 recog 来自 HUtil32.Sequence()
# （Guid 取随机正数，实测 SM_TURN recog=255025409），Rust 的来自 EcsStore::alloc_id
# （从 1 自增，实测 actor recog=275）。同一个 --from-recog 值不可能同时命中两侧
# （实测带 --from-recog 255025409 对 Rust capture 直接 rc=1）。
# 且 recog 归一化发生在**选帧之后**：s2c_diff.py:104-118 先按 capture 的 recog 字段选帧，
# norm_raw 只改 raw_hex 头——归一化**不能**弥补跨侧 recog 差异，不能拿它当「同一组过滤值」。
# x/y 由摆位保证两侧同点，故跨侧只给 --from-x/--from-y；选中后各侧自动用
# 「该侧起始帧的 recog」匹配截止 41，不会配到旁人的 41。
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/s2c_diff.py \
  --rust E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --golden E:/tmp/window/20261021-a/csharp/s2c/capture.jsonl \
  --start-ident 10 --upto-ident 41 \
  --from-x <GUARD_X> --from-y <GUARD_Y> \
  --out E:/tmp/window/20261021-a/judge/g3-guardlist-diff.json
# 跨侧只给坐标；--from-recog/--to-recog 仅限**单侧**独立诊断（两侧取值不同，禁止共用）。
# --from-x/--from-y 必须成对，找不到匹配帧按 PREREQ/RED fail-closed。
# 辅助：出现对数
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/aoi_visibility_pair.py \
  --left E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --right E:/tmp/window/20261021-a/csharp/s2c/capture.jsonl --expect visible --min-pairs 1
```

- **期望**：`s2c_diff` 对**坐标选中的** 10→41 对的字段逐字节等价（recog 只在选中**之后**的字节比较里归一化，不参与跨侧选帧）；`GREEN: 同序列逐字节 diff = 0`。
  `aoi_visibility_pair` 末行 `RESULT: GREEN`。
- **失败判据**：选中对 10/41 的 `param/tag/series/body_sha256` 不同；出现对 <1；过滤值在任一 capture 找不到匹配帧。
- **C10#2 已闭环（v2.3）**：`s2c_diff.py` 支持 `--from-recog/--from-x/--from-y`（及 `--to-recog`），
  不再只能截首对；**跨侧 C3 只用坐标**（recog 两侧命名空间不同，见上），`--from-recog/--to-recog`
  只用于单侧独立诊断；截止 41 在该侧默认按起始帧的同一 recog 匹配，避免配到旁人的 41。选帧后仍逐字节比，
  **不降低任何口径**。真机窗口里 GuardList 卡可按「卫士那一对」直接判；若窗口抓不到该对则 fail-closed。

### C4. 时间戳判据：`check_timing.py`（已闭环，但规则要按场景写）

```bash
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_timing.py \
  --capture E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --config E:/tmp/window/20261021-a/judge/timing.json
```

先取**自己的 ActorId**（`s2c SM_LOGON(50)` 的 `recog` 即本人；`SM_HIT/SM_STRUCK` 与 `check_timing` 的过滤都用它）：

```bash
python -I -B -c "import json;rows=[json.loads(l) for l in open('E:/tmp/window/20261021-a/rust/s2c/capture.jsonl',encoding='utf-8') if l.strip()];print(next(r['recog'] for r in rows if r.get('dir')=='s2c' and r.get('ident')==50))"
```

规则 JSON（示例，**只保留本场景要判的**；`--example-config` 里的 `3014/34/35/36` 是占位，不能原样用；
`<SELF>` 换成上一条命令的输出数字）：

```json
{"rules":[
  {"name":"sm_hit-leads-sm_struck-250ms","type":"after",
   "from":{"dir":"s2c","ident":14},"to":{"dir":"s2c","ident":31},
   "min_ms":0,"max_ms":250,"allowed_missing":0},
  {"name":"monster-struck-incoming-cadence-min3s","type":"interval",
   "match":{"dir":"s2c","ident":31,"recog":<SELF>},"min_ms":3000,"max_ms":600000,"min_pairs":2},
  {"name":"send-rate-50-per-s","type":"rate",
   "match":{"dir":"s2c"},"window_ms":1000,"max_count":50}
]}
```

> **为什么必须带 `recog`（F5）**：`ident=31` 同时含两类 `SM_STRUCK`——入站（victim=自己，`recog=SELF`）与
> 出站（victim=怪/别人，`recog=对方`）。`check_timing` 只按帧字段精确过滤 ⇒ 不带 `recog` 会把两类混进
> 同一 interval 序列，`min_ms=3000` 可能假红也可能假绿。**没有 self 过滤就不许用它判 cadence。**

- **期望**：每条规则 `[PASS]`；末行 `RESULT: GREEN`。
- **失败判据**：任一 `[FAIL]`；帧缺 `ts_ms` ⇒ rc=2；`from` 帧 0 条 ⇒ fail-closed RED。
- **可判的卡**：`monster-attack-cadence-lead`（同 HP 下 ident 31 间隔，min 3s）、
  `monster-attack-hit-frame` 的 200ms 前导（C1 更细）、发送速率上限。
- **不可判（观测物不在 capture 里）**：玩家 1.2s AOI 扫描节拍（`player-searchviewrange-cadence`）、
  30s `VerifyMapTime`（`aoi-verify-maptime` 时间半）、600/1200ms 延迟法术（`rm-delaymagic-*`）、
  3/5 分钟尸体（`corpse-ghost-revive`）——需要**服务端日志时间**或专门注入 ⇒ §C10 未闭环。

### C5. 三路径存档：`softclose|disconnect|shutdown` 各自 before/after

```powershell
# 每侧、每条路径、每个 phase 各一次（SnapDir 必须分开）
powershell -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/check_db_paths.ps1 -Mode snap `
  -Path softclose -Phase before -Prefix g5 `
  -SnapDir E:/tmp/window/20261021-a/db/g5-csharp-softclose-before -OutDir E:/tmp/window/20261021-a/db/g5-csharp
#   动作（softclose=小退；disconnect=断线；shutdown=§A4 优雅关服）→ after
powershell -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/check_db_paths.ps1 -Mode snap `
  -Path softclose -Phase after -Prefix g5 `
  -SnapDir E:/tmp/window/20261021-a/db/g5-csharp-softclose-after -OutDir E:/tmp/window/20261021-a/db/g5-csharp
# Rust 侧同三套（目录换 g5-rust）
# 最后：三路径 diff
powershell -File E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/check_db_paths.ps1 -Mode diff `
  -CsharpDir E:/tmp/window/20261021-a/db/g5-csharp -RustDir E:/tmp/window/20261021-a/db/g5-rust -Canonical
```

- **期望**：每条路径 `GREEN`；末行 `EVIDENCE <...>.json`，其 `pass=true`、`failed=[]`。
- **失败判据**：`PREREQ_MISSING`（缺某条 `<path>-after.jsonl`，说明 snap 没跑全）；`RED`（字段级 diff≠0）。
- **注意**：`-Mode diff` 只读 `<dir>/<path>-after.jsonl`；三条路径都必须先 snap，否则 PREREQ（评审 F8）。
- **E1-a（测试库）下 G5 字段级 diff 不可收（N1，与 D.3/C10#11 同一口径）**：两条独立阻塞——
  ① （**v2.3 已闭环**）`check_db_paths.ps1` 增 `-Table/-AllowDb` 并透传给 `dbsnap snap`，测试库可快照；
     但 E1-a 的字段级 diff 仍被 ② 挡住，所以结论不变：E1-a 只做行数/关键值断言；
  ② 即使手动跑 §A5-T，`dbsnap` manifest 里的表名是 `mir2_test_db.*`，而 `scoped_diff.py` 的
     `TABLE_COLUMNS`（`:47-106`）只登记 `mir2_db.*`/`mir2_account.*`，`extract` 对未登记表
     `sys.exit(4)`（`:294-299`）。
  ⇒ E1-a 下**不做**字段级 diff，改做行数/关键值断言（`COUNT(*)` + 登录/关服前后 `SELECT`）；
     softclose/disconnect/shutdown 三条路径全部标**部分收**；`save-on-shutdown` 仅 E1-b 全收。
- 状态：**E1-b 下闭环；E1-a 下字段级 diff 不可收**（仅行数/关键值断言，三条路径均部分收）。

### C6. 字段级 DB diff：`scoped_diff.py diff` 用 `--summary-out`（不是 `--out`）

```bash
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/m3/scoped_diff.py diff \
  E:/tmp/window/20261021-a/db/g5-csharp/softclose-after.jsonl \
  E:/tmp/window/20261021-a/db/g5-rust/softclose-after.jsonl \
  --canonical --summary-out E:/tmp/window/20261021-a/judge/g5-softclose-diff.json
```

- **期望**：rc=0；无 `DIFF` 行；摘要 JSON 的 `"mismatch_hashes" == 0`。
  （`scoped_diff.py:378-380` 写的是 `{"summary":{...},"mismatch_hashes":N}`；**没有 `diff_count` 键**。）
- **失败判据**：rc≠0；任一 `DIFF ident=... field=...`（差异要么修到 0，要么按 §6.0-3 登记）。
- **不要再写 `--out`**：`diff` 子命令会直接 argparse 报错（`scoped_diff.py diff --help` 原文已注明）。

### C7. D-9 的 52 差异必须登记到 whitelist 文件

```jsonl
{"ident":52,"field":"body_len","reason":"D-9：oracle 40B vs Rust client-58 58B（owner 已裁决，whitelist.md D-9）","reviewer":"openmir2-reviewer-1"}
{"ident":52,"field":"body_sha256","reason":"D-9 同上：40B→58B 布局变化必然改变 body_sha256（s2c_diff:213 逐字段比）","reviewer":"openmir2-reviewer-1"}
{"ident":52,"field":"raw_hex","reason":"D-9 同上（52 body 布局差异）","reviewer":"openmir2-reviewer-1"}
```

```bash
python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/m2-acceptance/s2c_diff.py \
  --rust E:/tmp/window/20261021-a/rust/s2c/capture.jsonl \
  --golden E:/tmp/window/20261021-a/csharp/s2c/capture.jsonl \
  --start-ident 658 --upto-ident 46 \
  --whitelist E:/tmp/window/20261021-a/judge/s2c-whitelist.jsonl \
  --out E:/tmp/window/20261021-a/judge/g1-diff.json
```

- **期望**：三条 `WHITELISTED ident=52 field=body_len|body_sha256|raw_hex ...`；末行 `GREEN`。
  只登记 `body_len`/`raw_hex` 不够：`s2c_diff.py:213` 的字段元组含 `body_sha256`，D-9 下它必不同 ⇒ 仍 RED。
- **失败判据**：whitelist 条目缺 `reason/reviewer` ⇒ 工具直接红；未登记的差异 ⇒ `RED`。
- **注意**：`--golden` 必须是**同 DB 状态**的 C# 实时 capture，不要用 2026-10-10 的静态金标准（会带 A-9 假红）。

### C8. 网关判据：**未闭环**，本窗不收

- 缺口：LoginGate(7000)/SelGate(7100) 没有入库的影子网关/代理拓扑；`mir2_proxy.py` 只被验证过 7200。
  60s 空闲、0–210ms 丢帧、10ms 拍、>200ms drain、GM 10s 节拍都没有注入/计时工具（评审 C8/F6）。
- 现有只能看日志/截图，**不能**证明逐分支 ⇒ `gate-idle-timeout`/`selgate-reconnect`/`gategate-slots-gm`
  保持 `in-progress`；`gategate-speed-rules` 另因 `ItemSpeed/SpeedLimit` 未接线只能记录（评审 D2）。
- **本窗 G7 整组 `not_collectible`。**

### C9. 6000/5100 DB 协议帧：**未闭环**，G6 本窗不收

- `mir2_proxy.py` 的 `--map` 可以映射任意端口，但**没有**可起的 6000/5100 影子 `db-srv`（全 Rust 栈直接绑
  6000/5100，不接受把出口重定向到代理），也没有逐帧日志。
- Rust db-srv 只打连接/错误/同步日志（`data.rs` 的 `log::info/warn/error`），不打 `1100/520/524/522/525` 逐帧；
  game-svr 侧只打「存档请求已发（DB_SAVEHUMANRCD）ChrName... 记录 N 字节」（`main.rs:5469`）。
- ⇒ 本窗 G6 只能做「DB 行级 + 日志级」的**部分**观察，帧级判据未闭环。G6 整组 `not_collectible`。

### C10. 未闭环工具/未知项清单（汇总）

| # | 缺口 | 影响 | 目前能做到的 |
| --- | --- | --- | --- |
| 1 | **【闭环 v2.3】** 52 body 的 **Weight/MaxExp 字段级解码器** | B2 的 `Weight=14` 可机判 | `check_client_s2c_frames.py --expect-52-weight/--expect-52-max-exp`（40B/58B 自动识别） |
| 2 | **【闭环 v2.3】** 10/41 的**坐标过滤**（C3；recog 过滤仅单侧诊断） | GuardList 可挑「卫士那一对」 | `s2c_diff.py --from-x/--from-y`（跨侧）；`--from-recog/--to-recog` 限单侧 |
| 3 | 1.2s/30s/600ms/3min 等**服务端节拍**读数 | cadence/aoi-maptime/delay-magic/corpse 卡 | 只能真实等待 + 日志人工看；不判 R |
| 4 | LoginGate/SelGate 影子抓包 + 网关 timer gate（C8） | G7 四卡 | 日志/截图；保持 in-progress |
| 5 | 6000/5100 代理或逐帧日志（C9） | G6 | DB 行级 + 连接日志；不判 R |
| 6 | 法术按键/`AutoSpell` 驱动 | G4 全部 | 只能摆前置（B6）；不收 |
| 7 | **【闭环 v2.3】** `account_provision.ps1` 无 `--host/--db` | 测试库开号可直接用 | `-Db mir2_test_account -DbHost 127.0.0.1`（`-Host` 别名）；默认仍 `mir2_account` |
| 8 | 栈启动器无「指向 `mir2_test_*`」参数 | E1 的测试库隔离 | **未闭环**：需手工改影子 conf / 另开工具改动 |
| 9 | **【闭环 v2.3】** `dbsnap` 默认快照只含生产库名 | 测试库快照 | `--db test`（13 张）或 `--db <单库名>`；与 `--table` 取并集 |
| 10 | **【闭环 v2.3】** `check_client_s2c_frames` 无「按角色/状态选预设」自动判 | G2 预设可自动选 | `--expect-enter-seq auto`（按 210/212/621/211 选 g2/delmagic/equipped/default，仍逐帧严判） |
| 11 | **【部分闭环 v2.3】** `check_db_paths.ps1` 缺 `--table/--allow-db`（① 已补）；`scoped_diff.TABLE_COLUMNS` 仍未登记 `mir2_test_*`（② 未做，`extract` 对未登记表 rc=4） | E1-a 下 G5 字段级 diff 仍不可收 | ① 已闭环：`-Table/-AllowDb` 透传；② **保留**：仅 E1-b 可做字段级 diff，E1-a 只做行数/关键值断言 |

---

## D. 分组（按「2 小时能跑完」重排）

> 前提：§A1 的 R6/R7/R8 已修绿，且 §E 对应项已裁决。每组 = 一个 2 小时块；**块间必须收窗/还原**。
> 预算按「2 小时含起停栈与还原」填；任何一段超时即按中止条件收窗，不借下一块的时间。
> 「全收」= 该卡的窗口项在本块内都有机判证据；「部分」= 只覆盖一部分余量，卡必须保持 `in-progress`。

### D.1 Block A（2 小时）：G0 + G1（登录/入图基线）

| 段 | 内容 | 预算 |
| --- | --- | --- |
| G0 | §A1（镜像清理/fetch/checkout/六件/provenance/guard） | 40–45 min |
| G1-C# | 起 C# 栈 + 影子 7600 + 代理；`aaa` 无装备/无技能、HP<MaxHP；登录抓包 | 25 min |
| G1-Rust | 同 DB 状态、同点；登录抓包 + 53 | 25 min |
| 收/还原 | 停栈 + restore + 侧间断言 | 10 min |

- **能全收（1 张）**：`openmir2-smframe-minor-gaps`（登录帧面；T-79/T-80 属已登记的不可达/模型项，不计入本窗余量）。
- **只能部分收**：
  - `openmir2-login-frame-order-52-752`（非 TestServer/发光装备/IsSpirit 未采，§B10）
  - `openmir2-login-numeric-gaps`（B2 的 Weight=14 已有机判，§C10#1 闭环；仍需真机采集）
  - `openmir2-healthspell-emission-model`（只覆盖 53 触发/尾长，其余窗口项不在 G1）
  - `openmir2-orphan-encoder-wiring`、`openmir2-window-collector-preflight`（种子断言/生产路径未覆盖）
  - `openmir2-outbound-encoders-red-batch`（制药抽数/MYSTATUS 不在本组）
- **不收**：`sm-merchantsay-recog`、`sm-updateitem`、`sm-additem-tail`（B8，移到需 E1 的动作块）；
  `login-useitems-magic`（需要 G2 的装备/技能状态）。

### D.2 Block B（2 小时）：G3a（单客户端战斗/AOI/GuardList/反向点）

| 段 | 内容 | 预算 |
| --- | --- | --- |
| 摆位 | B7 SQL（两侧同值、同 HP） | 10 min |
| G3a-C# | `(605,578)` 攻击 + 入站；`(291,666)` GuardList；`(311,666)` 反向 | 50 min |
| G3a-Rust | 同点同流程 | 50 min |
| 收/还原 | 停栈 + restore | 10 min |

- **能全收（3 张，单场景口径）**：`openmir2-monster-attack-hit-frame`（C1+C4 头/收件人/前导）、
  `openmir2-monster-attack-cadence-lead`（C4 interval + 同 HP + **`recog=SELF` 过滤**；缺此过滤只能部分收）、
  `openmir2-player-dir-supply`（Dir 变化可观测）。
- **只能部分收**：`openmir2-aoi-verify-maptime`（30s 时间半不可判，§C10#3）、`openmir2-monster-view-throttle`、
  `openmir2-attacktarget-overrides`（82/119/130/91/107/105 特殊怪未构造）、`openmir2-active-seek-race-missing`、
  `openmir2-proper-target-unfolded`/`openmir2-search-target-clear-target`（攻击已死目标需尸体构造）、
  `openmir2-run-gap-mb1/mb3/mb4`（特殊怪/城堡门未构造）、`openmir2-guardlist-guards-loading`（C3 的 recog/坐标过滤已具备，§C10#2 闭环；仍需真机采集）。
- **不收**：G3b/G3c 全部（B7）；`disappear-recipients-gridscan`、`run-gap-checkdupobj`、
  `player-drop-useitems`/T-81、`run-gap-monster-specials`、`run-gap-monster-master-follow`、
  `run-gap-corpse-ghost-revive`、`movetomovingobject-quirk`、`run-gap-pr1-head-blocking`、
  `monster-state-expiry`。

### D.3 Block C（2 小时）：G2a（装备/技能登录态）+ G5a（优雅关服/存档）

| 段 | 内容 | 预算 |
| --- | --- | --- |
| G2a-R1 | B5 R1（戒指 354，无技能）→ 210 | 10 min/侧 |
| G2a-R2 | B5 R2（无戒指）→ 212 + 粘滞 | 10 min/侧 |
| G2a-R3 | B5 R3（`MagicId=3`）→ 211 + 621 | 10 min/侧 |
| G5a | 玩家在世界中 → §A4 优雅停 GameSvr → 快照/diff | 15 min/侧 |
| 收/还原 | 停栈 + restore | 10 min |

- **能全收（E1-b 下 3 张；E1-a 下 2 张）**：`openmir2-oracle-humitems-align`（B3 1:1）、
  `openmir2-delmagic-addmagic-chain`（B5 两轮）、`openmir2-save-on-shutdown`。
  **`save-on-shutdown` 的全收只在 E1-b 成立**：`check_db_paths.ps1` 固定 `dbsnap snap --out`、无 `--table/--allow-db`，
  E1-a 测试库取不了快照；且 `-Mode diff` 要求三条 `<path>-after.jsonl` 都在，本块只排 shutdown。
  E1-a 下**不引用** `shutdown-after.jsonl`、也不跑 `scoped_diff`（C5 的 N1 口径：测试库表名未登记，`extract` 会 rc=4）；
  只做行数/关键值断言（shutdown 前后 `COUNT(*)` + 字段 `SELECT`），`save-on-shutdown` 与 softclose/disconnect 均标**部分收**。
- **只能部分收**：`openmir2-persist-useitems-wiring`（装载半有、破零收尾不在本组）、
  `openmir2-persist-create-rows`（需 E1 测试库 + 建角）、`openmir2-save-misc-gaps`（G11/G7 面未覆盖）。
- **不收**：`openmir2-persist-item-attr-and-gold`（G-01 受 B-117、G-02 受 B-116 阻塞）、
  `openmir2-snap-cell-fidelity`（两个前置未解，评审 D2）。
- **注意**：G2 的行会半（750）需要 §E2，**本块不排**；`guild-g2-dispatch` 只做无会对照或整体不做。

### D.4 本文件不排的组（另开窗口/待工具）

| 组 | 原因 | 卡处置 |
| --- | --- | --- |
| G4 法术/延时 | 无法术驱动（C10#6）；B6 只摆前置 | 全部 `in-progress`；`not_collectible` |
| G6 DB 故障 | 需停/起 mysqld（§E3）+ 6000/5100 无抓包（C9） | `dbsrv-fault-paths`/`db-error-reply` 等 `in-progress` |
| G7 网关 | 无网关 gate（C8/F6） | 四卡 `in-progress`；`gategate-speed-rules` 只能记录 |
| G8 内容周期 | 内容全部未启用 + C# 时钟未落（O3） | `in-progress` |
| G9 整包 W0 | O1–O7 未裁决 | `in-progress` |
| G10 M3 夹具链 | fixture/PREREQ 未齐 | 另档期 |
| G11 长跑 | 独占 24h+ | 另档期 |
| `enter-map-cell-rejected-vs-oracle` | 生产无 `switch_map` 调用者（真机不可达） | 构造级；移出窗口 |
| `rng-consumer-structure-parity` | `tier=L` 已完成 | 只留痕 |
| `run-gap-player-run-quantification` | B1 证据移到 G1/G3 | 不在 G8 |

---

## E. 授权边界（E1–E4 原样保留为**待 owner 裁决**）

> 每条都写「**不裁决则本组不可开窗**」。这些不是技术选择，而是 owner 的权力边界；
> 编排侧不得自行推定、不得用「有备份」替代授权。

### E1. 写库边界：生产 `mir2_*` vs 克隆 `mir2_test_*`

- 事实：`openmir2-save-misc-gaps`、`openmir2-persist-create-rows`、`openmir2-persist-single-sided-columns`、
  `openmir2-save-on-shutdown` 的 acceptance 明确写「**不写生产库**（测试库/替身）」；而本文件的摆位/装备/技能/
  关服写回如果落在 `mir2_db` 就违反了卡的验收边界。**仅「有 dbsnap 备份」不等于满足**。
- 需要裁决：
  - **E1-a**：批准克隆 `mir2_db/mir2_account/mir2_data` → `mir2_test_*`，并授权把 DBSrv/LoginSrv/game-svr
    指向测试库（当前启动器无此参数，需 §C10#8 的临时手工配置或后续工具改动）。
  - **E1-b**：或书面授权「本窗对生产 `mir2_db/mir2_account` 的摆位/装备/关服写回」，并指定授权条目/范围/回滚责任。
- 补充事实：`check_db_paths.ps1`（C5 用的工具）没有 `--table/--allow-db` 透传，内部固定对默认生产库取快照
  ⇒ 即使 E1-a（测试库）获批，G5 的三路径快照也需要**先改工具**（脚本变更不在本窗）或改用 E1-b。
- **不裁决则 Block A/B/C 全部不可开窗**（所有构造都会写库）。

### E2. 内容改动：影子 Envir / GuildBase / MonGen / 非 TestServer

- 事实：G2 的 750 需要行会内容；G3b/G3c 的特殊怪需要影子 `MonGen.txt`；B10 的非 TestServer 需要影子
  `Server.conf` + 整棵 Envir。这些都属于**内容改动**，不得改 `E:/MirServer` 原件。
- 需要裁决：是否批准「只读复制到影子包、在影子包内改内容」；以及由谁准备影子包（`-Pack` 源）。
- **不裁决则**：G2 行会半（750）不可开窗；G3b/G3c 不可开窗；B10 的非 TestServer 余量不采。

### E3. MySQL 停/起（故障注入）

- 事实：本机 `mysqld` 是 `D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe` **进程**（不是 Windows 服务，
  `Get-Service *maria*,*mysql*` 无结果）；`start-all.ps1 -Stop` 会连带杀掉它。G6 的故障注入必须停/起它。
- 需要裁决：是否批准在本窗独占时段停/起 `mysqld`，并明确：
  - 停：`Stop-Process -Name mysqld`（**不要** `Get-Service/Stop-Service`）；
  - 起：`Start-Process D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe -ArgumentList '--datadir=D:\mysql\data','--port=3306','--bind-address=127.0.0.1','--console'`；
  - 确认 3306 独占、无其他车道、恢复后校验。
- **不裁决则 G6 不可开窗**（且 `start-all -Stop` 永禁用）。

### E4. C# DBSrv/LoginSrv 的身份不在六件 provenance 内

- 事实：混合栈的 6000/5100/5700/5500/5600 由 `src/DBSrv/bin/Release/DBSrv.exe`、`src/LoginSrv/bin/Release/LoginSrv.exe`
  提供；`check_csharp_capture_preflight.py` 只查 GameSvr 的 `!deployed-from.json` 与 RunGate conf，
  **不查** DBSrv/LoginSrv。G0 的六件 Rust provenance 不等于整栈身份。
- 需要裁决（二选一）：
  - **E4-a**：允许在 `E:/Users/gxh/Documents/GitHub/OpenMir2` 执行
    `dotnet build src/DBSrv/DBSrv.csproj -c Release` 与 `src/LoginSrv/LoginSrv.csproj -c Release`，
    并把 `DBSrv.dll/LoginSrv.dll` 的 SHA256 + 构建时 HEAD 写入 `E:/tmp/window/20261021-a/csharp-provenance.json`
    （只写 E:/tmp 证据，不提交）；
  - **E4-b**：或改用全 Rust 栈做 oracle 对照（但 G1/G2/G5a 的 C# 对照就没有 oracle 了，需另立口径）。
- **不裁决则所有依赖 C# oracle 的块（A/B/C 的 C# 侧）不可开窗**。

---

## F. 收窗清单（每趟必须，顺序不可换）

1. **停代理**（`mir2_proxy.py` 进程）；**停 7600 影子 gate**（Rust/C# 各自的 pid）。
2. **Rust 栈**：`start_rust_stack.ps1 -LogDir E:/tmp/window/20261021-a/rust/stack -Stop`（game-svr 走命名管道优雅停）或
   `start_full_rust_stack.ps1 -LogDir E:/tmp/window/20261021-a/full/logs -Stop`（跳过 MySQL）。
3. **C# 栈**：先 §A4 优雅停 GameSvr（`Ctrl+C pressed` + 进程退出），**等 3 秒**，再 `start-csharp-hook.ps1 -Stop` 清网关/DBSrv/LoginSrv。
4. **MySQL**：默认**不动**（共享 3306）；只有 §E3 授权时才停/起，且必须先完成 DB 快照。
5. **DB 还原**：MySQL 活着时执行
   `python -I -B E:/Users/gxh/Documents/GitHub/OpenMir2/tools/dbsnap/dbsnap.py --password= restore --dir E:/tmp/window/20261021-a/db/base --yes-apply`
   （测试库加 `--allow-db ...`），随后断言关键行数/值。
6. **端口**：`powershell -File E:/MirServer/port-status.ps1`（十端口）+ **单独断言 7600 DOWN**
   （`port-status.ps1` 不含 7600）。
7. **工作树**：`git status` 干净；主检出 `src/ sql/` 无改动（E4-a 构建产物在 `bin/` 不入库）。
8. **证据**：命令 + 输出 + 路径 + SHA256；capture/日志/截图落 `E:/tmp/window/20261021-a/<组>/<side>/`；
   报告末段列「残留物清单」（含 `E:/tmp/window/...` 哪些保留、为什么）。
9. **线程**：`patch(base,head)` + 独立 principal 的 `review`（写 `tier`+`tier_why`+窗口证据）→ `merge_result`；
   R 卡窗口未跑/未全绿不得 `closed`。
