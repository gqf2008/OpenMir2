# 工具3 设计说明 —— 灰度阶梯 + 回滚演练 + 全 Rust 起栈器

> 规格来源：上一轮工作流产物 `C:/Users/gxh/AppData/Local/Temp/wf2/m5_stage0.json` 的 `tool3` 段。
> 本文件是**草稿产物**（写在 `E:/tmp/m5-tools-draft/tool3/`），**未入库**；仓库全程只读。
> 落地位置（建议，转正时）：`tools/m5-acceptance/start_full_rust_stack.ps1`、`tools/m5-acceptance/stage_gray_rollback.ps1`。

## 0. 交付物

| 文件 | 作用 | 分级 |
|---|---|---|
| `start_full_rust_stack.ps1` | 起**含 game-svr 的全 Rust 栈（六件）**：MySQL + Rust db-srv/login-srv/game-svr + Rust 三网关 | L（脚本）+ 起真栈 R |
| `stage_gray_rollback.ps1` | 灰度阶梯（逐个 C#→Rust，每步跑 E2E）+ 回滚演练 + **计时口径** | L（脚本）+ 真演练 R |
| `parse-check.txt` | 两脚本 `[scriptblock]::Create` 语法检查输出 | 自证 |

规格里把灰度脚本叫 `gray_step.ps1`；落地名按任务取 `stage_gray_rollback.ps1`（`docs/Rust重写设计文档.md` §6 M5④ 只给了功能口径）。

**分级提醒（必读）**：脚本**编写**是 L 级（工具/夹具，可零窗口写 + 自检）；**实际演练**（起真栈 / 跑真 E2E / 回滚 / 落库）
命中 R「持久化落库」，**须独占窗口**（`docs/Rust重写设计文档.md:357/359-360`、`AGENTS.md` 判据分级节）。
本次交付**只做脚本 + 离线自检**，不起任何进程/栈/端口、不 build、不写库。

## 1. 先读仓库现状（本任务实际核对结论）

| 项 | 结论 | 证据（行号，已回仓核对） |
|---|---|---|
| `tools/m2-acceptance/start_rust_stack.ps1` | 组合 = MySQL + **C#** DBSrv(6000/5100/5700) + **C#** LoginSrv(5500/5600) + Rust game-svr(5000) + Rust 三网关 | 头注释 :4/:12-17；param :93-121；`-Stop/-Status/-Force` :166-184；组合 :349-357；就绪检查 :362-390 |
| 该脚本头注释「Rust db-srv 只有 5100 面」 | **已过时**（Rust db-srv 6000/5700 已落地） | `tools/m2-acceptance/start_rust_stack.ps1:19-23`（旧）；`mir2-rs/bins/db-srv/src/main.rs:83-87`（`data::run` 6000 + `cache_flush_task` + `market::run` 5700）；`.../db-srv/src/config.rs:136/146/170`（键 6000/5100/5700）+ `:314-326`（现网 dbsvr.conf 断言） |
| `tools/m1-acceptance/start_boundary_stack.ps1` | 存在；全 Rust 边界链，**不含 6000**（我确认其 :15-16 同款过时） | 文件存在；头注释 :15-16；影子配置生成 :115-188；端口 :50 |
| `tools/m2-acceptance/preflight.ps1` / `start_boundary_stack.ps1` / `run_e2e.ps1` | `preflight.ps1` **在 `tools/m2-acceptance/`**（不是 m1）；`start_boundary_stack.ps1` 在 **`tools/m1-acceptance/`**；`run_e2e.ps1` 在 **`tools/e2e/`**（三个不同目录） | `ls` 结果：m2-acceptance 有 preflight.ps1；m1-acceptance 有 start_boundary_stack.ps1；e2e 有 run_e2e.ps1 |
| `tools/botload/load_gate.ps1` 档位参数 | `-Tiers`（string，逗号/空格分隔；`-HoldSec/-Repeat/-RestartStackPerTier/-SelfTestRed` 等） | `tools/botload/load_gate.ps1:34-62` |
| `E:/MirServer/start-all.ps1` 单服务启停 | 组件表（DBSrv/LoginSrv/GameSvr/LoginGate/SelGate/RunGate 六件，各自 Dir+Exe+Ports）；`-Stop` 按名字序停（网关→GameSrv→LoginSrv→DBSrv→mysqld）；`-Status` 只读 | `E:\MirServer\start-all.ps1:38-45`（组件）、`:112-124`（Stop-Stack）、`:130-142`（Start-Component）、`:144-145`（Stop/Status 分发） |

**本任务只登记上述过时注释，不改它们**（仓库只读）。

## 2. 每步的确切命令行

### 2.1 `start_full_rust_stack.ps1`（全 Rust 六件）

进程与命令行（**逐条取自** `start_rust_stack.ps1:349-357`，只把 db/login 的 exe+配置换成 Rust）：

| 序 | 进程 | 命令（argv[1] = 配置路径） | 端口 | stop_order |
|---|---|---|---|---|
| 1 | MySQL | `D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe --datadir=D:\mysql\data --port=3306 --bind-address=127.0.0.1 --console` | 3306 | 9 |
| 2 | Rust db-srv | `<RustBinDir>\db-srv.exe <StackRoot>\db-srv\dbsvr.conf` | 6000/5100/5700 | 7 |
| 3 | Rust login-srv | `<RustBinDir>\login-srv.exe <StackRoot>\login-srv\logsrv.conf` | 5500/5600 | 6 |
| 4 | Rust game-svr | `<RustBinDir>\game-svr.exe <MirServer>\M2GameSvr\Server.conf` | 5000 | 5 |
| 5 | Rust login-gate | `<RustBinDir>\login-gate.exe <StackRoot>\login-gate\config.conf` | 7000 | 1 |
| 6 | Rust sel-gate | `<RustBinDir>\sel-gate.exe <StackRoot>\sel-gate\config.conf` | 7100 | 2 |
| 7 | Rust game-gate | `<RustBinDir>\game-gate.exe <StackRoot>\game-gate\config.conf` | 7200（+7201/7202 记 pid 展示） | 3 |

- 影子配置（`<StackRoot>` 下，照 m1 `start_boundary_stack.ps1:115-188`）：`db-srv\{dbsvr.conf,AddrTable.txt,ServerInfo.txt,SelectID.txt,DenyChrName.txt}`、
  `login-srv\{logsrv.conf,AddrTable.txt,UserLimit.txt,ServerAddr.txt}`、三网关 `\{config.conf}`（**GBK 整体复制**）。
- **game-svr 例外**：用**现网** `E:\MirServer\M2GameSvr\Server.conf`（不复制到影子目录）——game-svr 的 BasePath = 该文件所在目录
  （`mir2-rs/bins/game-svr/src/main.rs:289-290`），它还要在同目录找地图 / `string.conf` 等数据文件，复制会丢数据。
- 种子注入：只对 game-svr，经 `rust_seed_plumb.ps1` 的 `Start-SeededProcess`（与本脚本口径一致，未传 ⇒ 逐字节零变化）。

### 2.2 `stage_gray_rollback.ps1`（灰度阶梯）——来自自带组合器

`-ImplMatrix` 是**目标组合**；阶梯 = 从**全 C# 基线**按依赖序（db→login→game→gate）逐个翻。每个 rung 的
**确切命令行** = 把下面每个 (svc, impl) 的命令按序拼起来（`-DryRun` 会原样打印，见 §4 实测）。

| svc / impl | 命令（argv[1] = 配置路径） | 端口 | stop_order |
|---|---|---|---|
| db / rust | `<RustBinDir>\db-srv.exe <StackRoot>\db-srv\dbsvr.conf` | 6000,5100,5700 | 7 |
| db / csharp | `<CsharpRepo>\src\DBSrv\bin\Release\DBSrv.exe`（工作目录=同目录） | 6000,5100,5700 | 7 |
| login / rust | `<RustBinDir>\login-srv.exe <StackRoot>\login-srv\logsrv.conf` | 5500,5600 | 6 |
| login / csharp | `<CsharpRepo>\src\LoginSrv\bin\Release\LoginSrv.exe` | 5500,5600 | 6 |
| game / rust | `<RustBinDir>\game-svr.exe <GameSvrConf>`（默认 `E:\MirServer\M2GameSvr\Server.conf`；注入种子） | 5000 | 5 |
| game / csharp | `E:\MirServer\M2GameSvr\GameSrv.exe` | 5000 | 5 |
| gate / rust | `<RustBinDir>\{login-gate,sel-gate,game-gate}.exe` + 影子 `config.conf` | 7000 / 7100 / 7200(+7201/7202) | 1 / 2 / 3 |
| gate / csharp | `E:\MirServer\{LoginGate\LoginGate.exe,SelGate\SelGate.exe,RunGate\GameGate.exe}` | 7000 / 7100 / 7200 | 1 / 2 / 3 |

C# 三件的 Dir/Exe/Ports **取自** `E:\MirServer\start-all.ps1:38-45`；Rust 件取自 `start_rust_stack.ps1:353-357` 与
`start_boundary_stack.ps1:215-218`；C# 件用无参启动（start-all 即如此，各自 `bin\Release` 工作目录）。

**举例**（`-ImplMatrix "db=rust,login=rust,game=rust,gate=rust"`，forward）：
- rung1 = db 翻 rust ⇒ 组合 `db=rust,login=csharp,game=csharp,gate=csharp`：`db-srv.exe …` + `LoginSrv.exe` + `GameSrv.exe` + `LoginGate.exe/SelGate.exe/GameGate.exe`；
- rung2 ⇒ `db=rust,login=rust,game=csharp,gate=csharp`（再加 `login-srv.exe …`）；
- rung3 ⇒ `db=rust,login=rust,game=rust,gate=csharp`（`GameSrv.exe`→`game-svr.exe …`）；
- rung4 ⇒ 全 Rust（三 C# 网关→三 Rust 网关）。

**回滚**（`-ImplMatrix "db=rust,login=rust,game=rust,gate=rust" -Rollback`）：按**反序** gate→game→login→db 翻回 C#，
末步（rung4）= 全 C# 基线；末步 `run_e2e` 必须全绿。

## 3. 与既有脚本的复用关系

| 复用点 | 来自 | 用法 |
|---|---|---|
| 每个 rung 的 E2E 判据 | `tools/e2e/run_e2e.ps1` | 子进程调用（`Invoke-E2E`，含 stack_e2e 的 stderr 陷阱处理 :72-82），GREEN/RED/SKIP + `-SelfTestRed`/`-FailOnSkip` 透传 |
| 种子/状态/rng/隔离/布局注入 | `tools/m2-acceptance/rust_seed_plumb.ps1` | `stage_gray_rollback.ps1` dot-source 其 helper（`Resolve-Mir2Seed`/`Start-SeededProcess`），只注入 **Rust game-svr**，与 `start_rust_stack.ps1` **同口径** |
| pid 文件 schema | `start_rust_stack.ps1:359` / `start_boundary_stack.ps1:220` | 同形 `{name,pid,ports,stop_order,exe}`，`-Stop` 逆 stop_order 停 |
| 影子配置生成 | `start_boundary_stack.ps1:115-188` | `New-ShadowConfigs` 逐条对齐（GBK 整体复制 + dbsvr.conf 现网键） |
| Test-Port/Get-ListenPids/Show-Status | `start_rust_stack.ps1` / `start_boundary_stack.ps1` | 逐条复制（**不用** `Get-NetTCPConnection`，该 cmdlet 本机偶发挂死） |
| 全 Rust 终点组合 | 本批 `start_full_rust_stack.ps1` | 灰度到「全 Rust」rung 时，命令行与它同源；它也是独立可用的快路径 |

**刻意的重复**：`stage_gray_rollback.ps1` 的 `New-ShadowConfigs` 与 `start_full_rust_stack.ps1` 同源（约 40 行）。
转正时建议抽成共用 `tools/m5-acceptance/m5_stack_common.ps1`（本草稿不抽，保持两个自足 `.ps1`）。**这是 reuse_map 的第一条技术债。**

## 4. 离线自检（本任务实际跑的，无进程/无端口）

```
# 语法检查（PARSE OK）——见 parse-check.txt
[scriptblock]::Create((Get-Content -Raw stage_gray_rollback.ps1))   # OK, 27944→(改后) bytes
[scriptblock]::Create((Get-Content -Raw start_full_rust_stack.ps1)) # OK, 18165 bytes

# 阶梯展开（正向，全 Rust 目标）——只打印计划，不起栈
stage_gray_rollback.ps1 -ImplMatrix "db=rust,login=rust,game=rust,gate=rust" -DryRun -OutDir <draft>/_selfverify/forward
#  → DRYRUN mode=forward rungs=4；rung1..4 = db→login→game→gate；末组合=all-rust；rc=0

# 阶梯展开（回滚）
stage_gray_rollback.ps1 -ImplMatrix "db=rust,login=rust,game=rust,gate=rust" -Rollback -DryRun -OutDir <draft>/_selfverify/rollback
#  → DRYRUN mode=rollback rungs=4；rung1..4 = gate→game→login→db；末组合=all-C#；rc=0

# 两向牙（planning/validation 层，改坏必红）
-ImplMatrix "foo=rust"    → rc=1（未知服务 'foo'）
-ImplMatrix "db=banana"   → rc=1（实现 'banana' 认不出）
-ImplMatrix "dbrust"      → rc=1（不是 svc=impl 形式）
-ImplMatrix "db=rust,login=rust,game=rust,gate=rust" → rc=0（正面过）
```

**未在本次自检覆盖（须 R 窗口）**：真正起栈后的每 rung `run_e2e` 判 GREEN、单步 RED 的整轮 RED + 非零 rc、
`-Rollback` 末步全绿、`-SelfTestRed` 透传 `run_e2e -SelfTestRed -SkipFlow` 后首步必红。这些都要真栈 ⇒ R 级独占窗口。

**`-SelfTestRed` 牙齿设计**（改坏必红）：透传 `-SelfTestRed` 给 `run_e2e`（并在未给 `-SkipFlow` 时补 `-SkipFlow` 保确定性），
`run_e2e.ps1:209-214` 在找不到 flow-e2e 项时必 rc≠0 ⇒ 首步必 RED ⇒ 阶梯 RED + 非零 rc；本脚本断言「确实变红」才 `SELFTEST_RED GREEN` exit 0，
否则 `SELFTEST_RED RED` exit 1（假红则判据失效）。

## 5. 计时口径（`[System.Diagnostics.Stopwatch]`）

每 rung 记 4 个量（ms，落 `gray_step.ndjson` / `gray_step.timing.json` / `GRAY.md`）：

- `stop_ms`  = 停当前栈（含 2s settle；与各起栈器 `-Stop` 一致）
- `start_ms` = 起目标组合栈 **+ 等到全部目标端口就绪**（`Wait-Ports`，缺一即 rung RED）
- `e2e_ms`   = `run_e2e.ps1` 子进程 wall time
- `total_ms` = `stop_ms + start_ms + e2e_ms`

回滚（`-RollbackOnRed`，默认开）额外记一条 `kind=rollback_of_rung` 的记录（只回滚该步，`total_ms = stop+start`）。

## 6. fail-closed / 退出码

- `start_full_rust_stack.ps1`：`-Stop` 缺 pid ⇒ `NOTHING_TO_STOP`(0)；`-Status` 只读；缺 exe/配置 ⇒ `PREREQ_MISSING`(2)；
  端口已占 ⇒ exit 1；端口未就绪 ⇒ `RED` exit 1；成功 ⇒ `FULL_RUST_STACK_OK`(0)。**只起 Rust 件，绝不静默回落 C#。**
- `stage_gray_rollback.ps1`：非法 `-ImplMatrix` ⇒ 抛错 exit 1；端口已占 ⇒ exit 1；缺 exe/配置 ⇒ `PREREQ_MISSING`(2)；
  任一步 RED ⇒ 停 + 整轮 RED + exit 1（并默认只回滚该步）；全绿 ⇒ exit 0；`-DryRun` 零副作用。
  与规格「单步 RED ⇒ 只回滚该步」+「停 + 整轮 RED + 非零 rc」**两者都满足**（回滚该步后再停）。

## 7. 建议的 tools/README.md 工具表行（仓库只读，未改）

```markdown
tools/m5-acceptance/start_full_rust_stack.ps1  含 game-svr 的全 Rust 栈（六件）起栈器   -Stop/-Status/-Force；端口未就绪即 RED
tools/m5-acceptance/stage_gray_rollback.ps1     灰度阶梯+回滚演练（逐个 C#→Rust，每步 E2E，计时口径） -SelfTestRed；-DryRun；-Rollback
```

## 8. open_questions（无法核实 / 需人裁）

详见 `StructuredOutput.open_questions` 与 `reuse_map`（同一批）。要点：
1. **`-ImplMatrix` 的 `gate` 粒度**：本实现把「三网关」当一个 `gate` 组一起翻；任务措辞「全 C# **五步**」在规格里
   没给出固定服务清单的锚点（示例 matrix 只有 4 个 key：db/login/game/gate）。若「五步」指 5 个服务，可能要把
   gate 拆成 `login-gate/sel-gate/game-gate` 三个 key（则 all-rust 目标 = 6 rungs）。**须 owner 钉口径**。
2. **RustBinDir 默认 `E:\tmp\openmir2-m2svr-target\debug`**：沿用 `start_rust_stack.ps1:96` 默认；本任务不 build，
   未验证 `db-srv.exe/login-srv.exe` 等产物存在。
3. **C# 件配置发现**：假设 `DBSrv.exe`/`LoginSrv.exe` 从自身 `bin\Release` 读 `dbsvr.conf`/`logsrv.conf`
   （start-all 无参启动）；未真机确认（R 窗口）。
4. **gate 端口口径**：game-gate 记 pid 端口 `7200/7201/7202`（照 `start_rust_stack.ps1:357`），就绪只等 `7200`
   （照 `:371`）；Rust game-gate 是否真绑 7201/7202 未核。
5. **`stage_gray_rollback.ps1` 不跑基线 E2E**：默认从第一个翻步开始；若要先证「全 C# 基线本身全绿」，
   需另跑一次（`start-all.ps1` + `run_e2e.ps1`）——是否要内置成 rung0 待定。
6. **tools/README.md 工具表行**：见 §7，未改。

---

## 2026-10-15 工作流对抗验证后的修复（落库前）

三处按验证者结论修（原稿的问题 → 修法）：

1. **I1（HIGH）`-SelfTestRed` 恒真牙** → 原实现「透传 -SkipFlow ⇒ run_e2e 找不到 flow-e2e 项即 rc!=0 ⇒ 判绿」，
   任何非零 rc 都会通过 ⇒ 零判别力。修后：删除自动 `-SkipFlow`；-SelfTestRed 要求三条同时成立
   （未 -SkipFlow / 汇总 RED / 至少一个 rung 的 `e2e_rc != 0` 且 `ports_ok = true`），任一条不满足即 fail-closed（exit 1/2）。
   **真机红检（注入坏步 ⇒ E2E 判红）归 R 窗口**，本脚本不再自称「改坏必红已成立」。
2. **D1（HIGH）失败提示推荐 `start-all.ps1 -Stop`（会杀 mysqld）** → 两脚本提示改指向
   「持有该端口的栈自己的起栈器 -Stop」，并显式写「不要用 start-all.ps1 -Stop（其 targets 含 mysqld）」。
3. **D2（HIGH）灰度每档会停 MySQL** → 原 Start-Combo 在 3306 down 时起 MySQL 并登记 `stop_order=9`，
   Stop-Combo 会按序把它停掉。修后 Stop-Combo 对 `name = MySQL` 一律 SKIP-STOP（共享 DB 永不由本脚本停，
   对齐 M5 计划「保留 3306 mysqld」）。

未修（登记为待决）：gate 粒度「4 rungs vs 5 步」口径（open_questions 1）；RustBinDir 产物存在性（真机窗口前核）。