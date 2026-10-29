# 上线切换 runbook：现网从 C# 参照栈切到 Rust 栈（含回滚）

- 卡：`openmir2-cutover-runbook`（2026-10-29；tier=L，纯文档）
- 交付前提：交付 tag `delivery-20261029`（owner 口径 @ `ba9c22caa`）已按交付验收流程四条件通过。
- **本文件只是操作手册，不构成切换授权；切换执行本身需 owner 批准后另起窗口**。执行窗口内
  本手册为唯一操作序列：**按编号顺序做，任何一步 RED 即停，走 §4 回滚，不带病继续**。

---

## 0. 范围与红线

- **切换对象**：现网十端口业务（5000/5100/5500/5600/5700/6000/7000/7100/7200）由 C# 六角色
  （GameSrv / GameGate / LoginGate / SelGate / DBSrv / LoginSrv）切到单二进制
  `mir2-server.exe` 的同六角色（db-srv / login-srv / game-svr / login-gate / sel-gate / game-gate）。
- **不动的东西（红线）**：
  1. **mysqld（3306）全程不停、不重启**——两栈共用它；谁把它停了谁负责（见 §5 风险 1）。
  2. **客户端（Delphi mirbeta）一行不改**；对外地址/端口/服务器名（`热血传奇`）不变。
  3. **不手改任何 Envir / Castle 内容**；`E:/MirServer/M2GameSvr/Castle/List.txt`（内容 `0,1`，
     sha256 `9b38d284…`）及其余数据文件保持字节原样（比对看内容不比 mtime，原因见下文 §3.S8）。
  4. 不改配置文件：六角色 conf 两栈共用（见 §2），切换只换进程，不改文件。
  5. 窗口内不跑 `cargo`（构建会干扰在跑栈；二进制必须**窗口前**备好并核验，见 §2.G1）。
- 涉及脚本与工具（本文引用的全部为仓内已验证入口）：
  `tools/m2-acceptance/start_mir2_server.ps1`、`tools/m2-acceptance/stop_mir2_server.ps1`、
  `tools/m2-acceptance/preflight.ps1`、`E:/MirServer/port-status.ps1`、`E:/MirServer/start-all.ps1`、
  `tools/window/stop-csharp-graceful.ps1`（注意：见 §3.S1 的可用性说明）、
  `tools/dbsnap/dbsnap.py`、`tools/probe/mir2_probe.py`、
  `D:/mysql/mariadb-10.11.19-winx64/bin/mysqldump.exe`。

---

## 1. 切换目标形态（切完长什么样）

- 同一台机器、同一份部署根 `E:/MirServer`、同一组端口与配置文件；变的只有**进程**：
  C# 的六个 exe（GameSrv/GameGate/LoginGate/SelGate/DBSrv/LoginSrv）换成
  `mir2-server.exe` 以六个角色各起一个进程（db-srv/login-srv/game-svr/login-gate/sel-gate/game-gate）。
- mysqld（3306，`D:/mysql/mariadb-10.11.19-winx64`）不动；库与表不动；对客户端完全透明。
- 运维入口换成：`tools/m2-acceptance/start_mir2_server.ps1`（起，fail-closed 带反向回滚）与
  `tools/m2-acceptance/stop_mir2_server.ps1`（停，game-svr 走优雅关服管道后校验端口 DOWN）；
  运行产物在 `E:/MirServer/logs/mir2-server/`（pid json + 各角色 out/err 日志）。

---

## 2. 前置清单（G0–G6；执行窗口前逐项勾掉，缺一不开窗）

### G1 交付二进制核验（sha256 + stamp）

卡面给定交付二进制 sha256：`e49a8ede7f2510d08acb334577ee2782c672c30a3b1c5734e2682ed25c4ecd71`。
三道核验（在**将要起栈的那棵树**里跑；exe 解析只认脚本的 `$PSScriptRoot`）：

```sh
# ① 裸 sha（Git Bash）
sha256sum mir2-rs/target/release/mir2-server.exe
```

```powershell
# ② WhatIf 三行 + 强制 Release（缺 release 二进制时 RED，不会静默退到 debug）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root E:\MirServer -WhatIf -RequireRelease
# 期望：SERVER_EXE_SHA256 = e49a8ede…（与卡面值全等）；SERVER_EXE_BUILD = Release
```

```sh
# ③ build stamp 交叉（rebuild-server.ps1 / cargo build 产出的自述 stamp）
python -I -c "import json;d=json.load(open('mir2-rs/target/release/mir2-server.build.json'));print(d.get('sha256'),d.get('git_head'),d.get('build'))"
# 期望：sha256 与 ①/② 全等；git_head = owner 指定的交付提交（delivery-20261029 口径）
```

- 若现场重构建而非用现成二进制：重构建后 sha256 **必须仍等于卡面值**（同 tag 同构可复现）；
  不相等即停，找 owner 对齐后再开窗。
- stamp 缺失时以 ①+② 为准（两者一致即可放行），并在窗口日志登记「stamp 缺、以 sha 双源核验」。

### G2 数据库备份命令（本清单只做**预演**；正式全量备份在 §3.S3 停机后执行）

生产库实况（2026-10-29 只读盘点）：`mir2_db`（角色/物品等，`characters` 7338 行量级）、
`mir2_account`、`mir2_data`，另有 DBSrv 6000 面分片库族 `mir2_dbsrv6k_mkt_*`
（盘点时存在 `_1352_0`、`_29292_0` 两个；**分片会随玩法增长，执行时以现场枚举为准**）。

```sh
# 预演①：mysqldump 可用性 + 权限（只导表结构，秒级）
D:/mysql/mariadb-10.11.19-winx64/bin/mysqldump.exe -h 127.0.0.1 -P 3306 -u root \
  --no-data --databases mir2_db > "$TEMP/backup-drill-schema.sql" && echo DRILL_OK
# 预演②：dbsnap 工具自检（离线，不连库）
python -I -B tools/dbsnap/dbsnap.py selftest
# 预演③：备份目录就位（窗口证据根）
mkdir -p E:/tmp/window/cutover-<date>/backup
```

正式备份命令见 §3.S3（停机后执行，含分片族枚举）。恢复/回灌命令见 §4.R2。

### G3 端口基线抓取（切前存档，切后逐站复验）

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
```

- 期望缺失列表为空，十端口 = 3306,5000,5100,5500,5600,5700,6000,7000,7100,7200。
- 已知既有偏差：5700（DBSrv 声明端口之一）历史上出现过未监听（真机窗口 runbook §0.6 有记录）。
  **切前如 5700 本就 DOWN，照实记进基线**；切换后以「与基线一致」为判据，不要求凭空补齐。
- 把完整输出存 `E:/tmp/window/cutover-<date>/port-baseline-before.txt`。

### G4 数据/冻结件基线（只读取证，供 §4 回滚决策）

```sh
# 角色存档水位（回滚边界判据之一；LASTUPDATE 是 characters 的存档时间列）
D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h 127.0.0.1 -P 3306 -u root -N \
  -e "SELECT MAX(LASTUPDATE) FROM mir2_db.characters;"
# 三库结构化快照（dbsnap，账号/角色全族；产物含 manifest.json + tsv）
python -I -B tools/dbsnap/dbsnap.py snap --out E:/tmp/window/cutover-<date>/snap-pre --db prod
# Castle 冻结件内容基线（比对内容，不比 mtime）
sha256sum E:/MirServer/M2GameSvr/Castle/List.txt E:/MirServer/M2GameSvr/Castle/Castle/SabukW.txt
```

### G5 探针账号核验（冒烟用）

- 冒烟用**既有探针账号**（勿用真实玩家账号）：执行前确认账号/角色在
  `mir2_account` / `mir2_db` 里存在且未被封禁；不存在由 owner 预建并登记凭据的**存放路径**
  （凭据本身不写进本手册与窗口日志）。

### G6 窗口与公告

- 停机公告（客户端登录页/群），窗口时长预估：停机段 = S1–S5（分钟级），冒烟观察段 = S7–S8。
- 确认没有其他窗口/批次在跑（真机窗口、对拍、cargo 构建都不许与本窗口重叠）。

---

## 3. 切换步骤（S0–S8；顺序执行，RED 即停）

### S0 冻结新连接

公告生效后确认无新登录（LoginGate 7000 的在线/日志不再增长）。本步无自动工具，人工判定并登记时刻。

### S1 停 C# 六角色（GameSrv 优雅停优先）

**顺序与理由**：GameSrv 先优雅停（它持有在线玩家存档），成功后其余五角色按
「网关 → 服务」停（LoginGate/SelGate/GameGate/DBSrv/LoginSrv）。

1. **GameSrv 优雅停（CTRL_C ⇒ 存档）**：`tools/window/stop-csharp-graceful.ps1`
   给 GameSrv 发 CTRL_C，走 C# 自身的 `Console.CancelKeyPress` ⇒ `OnShutdown` ⇒
   `SavePlayer()`（在线玩家逐个落库）后自然退出。退出码：`0`=GRACEFUL_EXIT；
   `3`=STILL_ALIVE（超时未退，此时才允许人工 `Stop-Process -Force` 并**在窗口日志登记未走优雅停**）；
   `2`=参数/缺件错误。
   - **可用性注意**：该脚本随卡 `openmir2-harness-stop-and-evidence` 评审（状态以线程台账现值为
     准）。执行窗口前先确认它已在 master（或按该分支取用）；若届时不可用，退化为「逐进程
     Stop-Process」并在窗口日志登记「未走优雅停」——这正是 §4 回滚边界把**停机后备份**设为
     数据基线的核心原因。
2. **其余五角色**（GameSrv 已退出后）：

   ```powershell
   # 逐进程停；禁止 E:/MirServer/start-all.ps1 -Stop（它会把 mysqld 一起停，红线）
   Get-Process GameGate,SelGate,LoginGate,DBSrv,LoginSrv -ErrorAction SilentlyContinue | Stop-Process -Force
   ```

   与 `start-all.ps1 -Stop` 的差异：`-Stop` 连 mysqld 一起停（且非优雅，会触发 crash recovery），
   切换窗口**禁止使用**；逐进程 `Stop-Process` 只停业务六角色，3306 保持不动。
   （`E:/MirServer/restart-gamesvr.ps1` 是运维重启 GameSrv 的旧脚本，强杀强起，切换窗口不适用。）
3. 停后核验（期望：除 3306 外九端口全 DOWN）：

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
   ```

### S2 存档落库确认

优雅停的**判据**是 DB 出现该次存档写入：

```sh
D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h 127.0.0.1 -P 3306 -u root -N \
  -e "SELECT MAX(LASTUPDATE) FROM mir2_db.characters;"
# 期望：≥ G4 记录的水位（在线玩家在 S1 被逐个 SaveHumanRcd 落库）
```

若 GameSrv 走的是强停（1 的退化路径），此步只照实记录水位（可能不上抬），不判 RED——
尾部未落盘的存档本来就是已知行为，回滚边界由 S3 备份兜底。

### S3 正式备份（停机后；这是回滚的数据基线）

```sh
BK=E:/tmp/window/cutover-<date>/backup
# ① 三主库全量（含建库/建表语句，可整库回灌）
D:/mysql/mariadb-10.11.19-winx64/bin/mysqldump.exe -h 127.0.0.1 -P 3306 -u root \
  --databases mir2_db mir2_account mir2_data --single-transaction --flush-logs > "$BK/prod-main.sql"
# ② 分片库族（动态枚举；当前实例为 mir2_dbsrv6k_mkt_1352_0 / _29292_0，以现场为准）
for db in $("D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe" -h 127.0.0.1 -P 3306 -u root -N \
  -e "SHOW DATABASES LIKE 'mir2\_dbsrv6k\_mkt%';"); do
  D:/mysql/mariadb-10.11.19-winx64/bin/mysqldump.exe -h 127.0.0.1 -P 3306 -u root \
    --databases "$db" --single-transaction > "$BK/$db.sql"
done
# ③ 备份完整性自检（非空 + 可读尾部 + 行数记账）
wc -c "$BK"/*.sql
grep -c "Dump completed" "$BK"/*.sql
# ④ 结构化快照（与 G4 同构，供 §4 的 diff 定位「Rust 到底写了哪些表」）
python -I -B tools/dbsnap/dbsnap.py snap --out E:/tmp/window/cutover-<date>/snap-at-cutover --db prod
```

判据：三主库 dump 非空且各自含 `Dump completed`；快照目录有 `manifest.json`。任一不满足 ⇒ 停，修复后重跑 S3（此时还没起 Rust，重跑无副作用）。

### S4 启动 Rust 六角色

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_mir2_server.ps1 `
  -Root E:\MirServer -RequireRelease -Seed 42
```

- 角色与端口（脚本内固定表，与现网一致）：db-srv 6000/5100/5700 → login-srv 5500/5600 →
  game-svr 5000 → login-gate 7000 → sel-gate 7100 → game-gate 7200/7201/7202；
  启动顺序如上，任一角色未就绪 ⇒ 脚本自动**反向回滚已起角色**并 RED（fail-closed，不静默）。
- conf 全部沿用 `-Root` 下现网文件（两栈共用，见 §3.S6 核对点）；库名由各 conf 的
  ConnctionString 决定，本脚本不改写任何配置。
- `-Seed 42`：显式记档便于事后对账（脚本缺省也是 42）。说明：C# 生产此前未注入种子（每次随机），
  Rust 固定种子意味着**重启后随机序列从头复放**——这不是对拍语义（生产单栈），但要在窗口日志里
  知情登记；owner 若要求与 C# 行为一致（每次随机），改传 `-Seed` 一个当场随机值。
- 产物：`E:\MirServer\logs\mir2-server\mir2-server.pid.json`（roles[]: role/pid/ports/stop_order/exe）
  与各角色 `<role>.out.log` / `<role>.err.log`。

### S5 端口基线复验

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
```

判据：与 G3 基线**逐端口一致**（含既有偏差项）。并核对 pid json 的 roles[].ports 与固定表一致。
不一致 ⇒ 停，走 §4 回滚。

### S6 配置核对点（只读；两栈共用的同一批文件）

| 文件 | 核对点 | 为什么 |
|---|---|---|
| `E:/MirServer/SelGate/config.conf` | `ServerPort0=5100`、`GatePort0=7100` | SelGate 上游指 DBSrv 5100 面、自身服务 7100；Rust sel-gate 读同一文件 |
| `E:/MirServer/LoginSrv/AddrTable.txt` | 服务器名 `热血传奇` + `…:7100` | 客户端选服路由；切栈不改，改了等于把玩家指去别处 |
| `E:/MirServer/RunGate/config.conf` | 7200/7201/7202 | game-gate 对客户端端口 |
| `E:/MirServer/M2GameSvr/Server.conf` | 端口 5000；`CastleDir=Castle`、`CastleFile=Castle/List.txt`；ConnctionString → mir2_db | game-svr 主配置；Castle 路径决定 §S8 的 600s 重写落点 |
| `E:/MirServer/DBSrv/dbsvr.conf` | 6000/5100/5700；ConnctionString → mir2_db / 分片族 | Rust db-srv 的 6000 取档/存档面 + 5100 选角面都从这里读 |

### S7 冒烟（probe 单跑；离线判读，不连对拍库）

用 G5 的探针账号逐条跑（一次一个场景，`--out` 各自独立目录）：

```sh
P=tools/probe
python -I -B $P/mir2_probe.py --login-gate 127.0.0.1:7000 \
  --account <probe账号> --password <probe密码> --server-name 热血传奇 \
  --scenario $P/scenarios/login_only.json --out E:/tmp/window/cutover-<date>/smoke-login
python -I -B $P/mir2_probe.py --login-gate 127.0.0.1:7000 \
  --account <probe账号> --password <probe密码> --server-name 热血传奇 \
  --scenario $P/scenarios/move_walk.json --out E:/tmp/window/cutover-<date>/smoke-walk
python -I -B $P/mir2_probe.py --login-gate 127.0.0.1:7000 \
  --account <probe账号> --password <probe密码> --server-name 热血传奇 \
  --scenario $P/scenarios/merchant_dialog.json --out E:/tmp/window/cutover-<date>/smoke-merchant
```

判读口径：

- 每个场景看 `run.json` / `frames.ndjson`：登录链 `login_only` 等 `SM_LOGON(50)/SM_ABILITY(52)/
  SM_SENDNOTICE(658)` 与延迟段帧齐；`move_walk` 的移动判据**等真实回执（GD 族），
  不等 SM_WALK**（历史教训：线上顺序 ≠ 源码顺序，判据一律看 frames.ndjson，不看代码顺序推演）；
  `merchant_dialog` 等 NPC 对话帧回齐。
- 任一场景不齐 ⇒ 记 RED，走 §4 回滚；不现场调参救。
- 提醒：本窗口不做攻击类冒烟（现网有 KickOverSpeed/OverSpeedKickCount 配置，过密攻击帧会断连
  而非被拒）；登录+走路+商人对话不触发该路径。

### S8 观察段（切后 30 分钟内）

- **Castle 600s 重写**：`E:/MirServer/M2GameSvr/Castle/List.txt` 会被 game-svr 按内存每 600s 重写
  （mtime 跳、内容仍 `0,1`、sha 仍 `9b38d284…`）——预期行为，不是事故；判据是**内容**不是 mtime。
  `Castle/<n>/SabukW.txt` 同理。见真机窗口 runbook §0.6/§0.10 的同源机制记录。
- **存档抽查**：跑一段后对照快照看 Rust 的写库面：

  ```sh
  python -I -B tools/dbsnap/dbsnap.py diff E:/tmp/window/cutover-<date>/snap-at-cutover \
    E:/tmp/window/cutover-<date>/snap-post --max-diffs 50
  # 预期差异面 = 存档族（characters/物品/技能/账号线）随在线行为增长；
  # 出现存档族之外的表 ⇒ 停下归因，必要时走 §4。
  ```

- 日志巡检：`E:\MirServer\logs\mir2-server\<role>.err.log` 无持续刷屏异常；game-svr 日志里
  seed 注入行（`RUST_SEED_INJECTED=42`）与 `SEED_PROBE` 出现且一致。
- 真实客户端（owner 或值班）手工登录一次，确认选服/选角/进图/回城四步体感正常。

---

## 4. 回滚方案（R1–R5）

**触发条件**：S4 起栈 RED、S5 端口不一致、S7 任一冒烟 RED、S8 出现存档族之外的写库面或
玩家侧恶性反馈。**触发即回滚，先回滚后归因。**

### R1 停 Rust 六角色

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/stop_mir2_server.ps1 -Root E:\MirServer
```

- 行为：读 `mir2-server.pid.json` 按停止序严格反向；game-svr 先走
  `\\.\pipe\openmir2-gamesvr-shutdown-<pid>` 优雅关服（**会存档**），8 秒未退才 Force；
  其余角色直接 Force；全部停完后校验端口 DOWN，全 DOWN 输出 `MIR2_SERVER_STACK_STOPPED` 退 0，
  否则 RED 退 1（fail-closed，有端口还 UP 就不算停干净）。
- 安全性：只杀进程名与 pid 记录一致（`mir2-server`）的进程，防 pid 复用误伤。

### R2 数据回滚决策（以 LASTUPDATE/快照为界）

先**判要不要回灌**，不要默认回灌：

```sh
# 切到当下：Rust 这段时间写了哪些表？
python -I -B tools/dbsnap/dbsnap.py diff E:/tmp/window/cutover-<date>/snap-at-cutover \
  E:/tmp/window/cutover-<date>/snap-post --max-diffs 200
# 水位对照：当前存档水位 vs S2/S3 记录值
D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h 127.0.0.1 -P 3306 -u root -N \
  -e "SELECT MAX(LASTUPDATE) FROM mir2_db.characters;"
```

- **差异为空（Rust 没写过库，如 S4 就 RED）** ⇒ 跳过回灌，直接 R3。
- **差异仅存档族（玩家/探针正常游玩产生的存档增长）** ⇒ owner 现场裁决：保留（不回灌）或回灌；
  默认建议**回灌**（回到 C# 停机时刻的干净边界，玩家重登即续）。
- **差异含存档族之外** ⇒ 必须回灌，并在窗口日志点名异常表（这是要在复盘里归因的第一现场）。

回灌（整库覆盖回 C# 停机时刻）：

```sh
BK=E:/tmp/window/cutover-<date>/backup
D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h 127.0.0.1 -P 3306 -u root < "$BK/prod-main.sql"
for f in "$BK"/mir2_dbsrv6k_mkt_*.sql; do
  D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h 127.0.0.1 -P 3306 -u root < "$f"
done
# 回灌后复验：水位回到 S3 时刻、行数对账（与备份头尾计数一致）
D:/mysql/mariadb-10.11.19-winx64/bin/mysql.exe -h 127.0.0.1 -P 3306 -u root -N \
  -e "SELECT MAX(LASTUPDATE) FROM mir2_db.characters;"
```

边界口径（务必在窗口日志写明）：

- **数据边界 = S3 备份时刻**。S1 停机后到 S3 备份完成前没有业务写入（业务已全停）⇒ 该边界内
  无损；S3 之后的 Rust 写入由本节 diff/回灌覆盖。
- C# 优雅停失败的退化路径下，强停瞬间的**尾部存档**本来就没落库——它们不在备份里，回灌不会
  「丢」它们（从未存在）；此损失在 S1 已要求登记。
- 分片库族以 S3 现场枚举为准；若 Rust 运行期间新建了分片库，回灌按备份里的集合为准，
  多出的空分片库由 owner 现场决定去留（登记）。

### R3 重启 C# 栈

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/start-all.ps1
# mysqld 已在跑时自动跳过（红线保证 3306 从未停）；起齐业务六角色
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
```

判据：与 G3 基线逐端口一致。注意 `start-all.ps1` 的 `-Stop` 仍禁止（强杀 mysqld）。

### R4 回滚后冒烟

重复 S7 的 `login_only` 一条即可（C# 栈自证可登录进世界），外加一次真实客户端手工登录。

### R5 登记

窗口日志里落：回滚触发点、diff 摘要、是否回灌、恢复用时、C# 恢复时刻。复盘归因另开卡。

---

## 5. 明确不做 / 风险

**明确不做**：不动客户端；不手改 Envir/Castle；不停 mysqld；不在本卡窗口内执行切换；
不在切换窗口里跑对拍/构建；不用 `start-all.ps1 -Stop`。

**风险（按杀伤力排序）**：

1. **mysqld 被停/重启** = 生产事故级：crash recovery + 边界作废。防线：红线 §0 + 只用逐进程
   停业务角色 + 回滚 R3 用 start（非 -Stop）。
2. **C# GameSrv 优雅停不可用**（stop-csharp-graceful 届时仍未并 master）⇒ 强停尾部丢档。
   防线：S1 的登记要求 + S3 备份即边界（尾部本来就不在边界内，回灌无「二次丢失」）。
3. **Rust 写库面超出存档族**：S8/R2 的 dbsnap diff 是唯一在岗探针；差异面异常即回滚。
4. **随机种子语义**：Rust 固定种子（默认 42）重启复放随机序列；C# 生产原为每次随机。
   已在 S4 要求知情登记；对玩家可感知性未验证，owner 可选当场随机值。
5. **多端登录/顶号、外网 IP（AddrTable 指向 10.10.0.102）** 等运维面假设在 Rust 栈下未逐一
   真机回归——本手册的冒烟覆盖登录/走路/商人对话，其余玩法靠 S8 观察段兜底。
6. **Castle 600s 重写**切换后由 Rust 接管：内容应保持 `0,1`；若 sha 变化（城数增减）＝异常，
   立即归因（它影响税/多城广播等玩法面）。

---

## 6. 残余项引用（切栈前知情、切栈后继续跟）

- **G11 跨服**：未实现，属已登记豁免项（裁决记录见 `docs/needs-human-20261025.md`、
  交付报告 `docs/delivery-report-20261025-final.md`）。跨服相关玩法在 Rust 栈上不可用/未验证。
- **T-270（ident14 额外 SM_HIT）**：受控裁决 APPROVED（`docs/ident14-extra-hit-adjudication-20261029.md`，
  规则表 `tools/conformance/rules.json` 的 oracle_extra_shapes/oracle_extra_status）；
  其后 run27 的同族帧差分诊见 `docs/run27-turn-shape-diffs-20261029.md`（tier=R，
  有 window_required 项未收口）。
- **RM_* 自身守卫系列**：A1 审计表 `docs/rm-self-guard-audit-20261029.md`；A2 路径牙随卡
  `openmir2-rm-self-guard-all-idents` 并入；其余路径（A3 负例、A4 等）以真机台账
  `openmir2-real-machine-verification-master` 与该审计文档的后续批次为准。
- **真机窗口结果**：run11–run22 逐场景记录见 `docs/window-run11-22-20261029.md`
  （其中 UNDECIDABLE 项**不是通过**，切栈后这些玩法面按 §S8 观察段重点盯）。
- **已知偏差白名单**：`mir2-rs/tests/parity/whitelist.md`（T-\*/D-\*/B-\* 登记；
  正文也是锚，勿在此引用行号）。判据分级与真机窗口口径见
  `docs/delivery-acceptance-runbook-20261025.md`。
- **db-srv 6000 面**：全 Rust 取档/存档链的模块构成见 `mir2-rs/bins/db-srv/src/lib.rs`
  的模块文档（5100 选角面 + 6000 DataService 取档/存档面）；`tools/m2-acceptance/start_rust_stack.ps1`
  头注里「DBSrv/LoginSrv 仍用 C#」的说明是**旧态注释**（该脚本属混合栈时代），
  切换以 `start_mir2_server.ps1` 六角色管线为准。

---

## 7. 证据登记口径

窗口全程落 `E:/tmp/window/cutover-<date>/`：`port-baseline-before.txt`、`backup/`、
`snap-pre/`、`snap-at-cutover/`、`snap-post/`、`smoke-*/`（frames.ndjson/run.json）、
`cutover.log`（手记：每步时刻/退出码/RED 判定）。窗口结束后按仓库惯例归档进交付证据，
`E:/tmp` 残留物按仓库规约登记或清理。

> 本手册引用的卡：`openmir2-cutover-runbook`（本卡）、`openmir2-harness-stop-and-evidence`、
> `openmir2-ident14-extra-hit-adjudication`、`openmir2-rm-self-guard-all-idents`、
> `openmir2-real-machine-verification-master`。引用处的状态一律以线程台账现值为准。
