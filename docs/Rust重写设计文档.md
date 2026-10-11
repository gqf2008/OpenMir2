# OpenMir2 → Rust 重写设计文档（v1 / 2026-10-10）

> 读者：接手实现的 agent。本文是施工图，不是调研报告。
> 三条硬约束写在 §1，违反即作废；先做 §6 的 M0，M0 未通过不要开始写引擎。

---

## 1. 目标与硬约束

**目标**：用 Rust 重写 OpenMir2 服务端，让**现存的经典客户端与现存游戏内容原样可用**，且行为与 C# 版逐项等价。

**硬约束（不可协商）**

1. **客户端冻结**：`mirbeta/MirClient`（Delphi）不改一行。协议（CM/SM 消息号 + EDCode 编解码 + 帧格式）必须**逐字节兼容**。
   **例外（2026-10-10 补）**：`src/BotSrv/**` 属**测试工具**、不在 oracle 的七个服务进程内，允许修改，但必须在 `tests/parity/whitelist.md` 登记为 `T-*` 条目，并证明改动只影响 BotSrv 产物、不改变 oracle 行为（首例见 T-1）。
2. **内容冻结**：现存的 MySQL 三个库与运行中的 `E:\MirServer\M2GameSvr\Envir` 脚本原样可用，不改表结构、不改脚本语法、不改数据文件格式。
3. **行为等价优先于修 bug**：先 1:1 搬（**包括已知 bug**），行为对拍全绿之后再单开批次修。理由见 §8.1。

**非目标**：不改客户端 UI；不换协议；不把 Envir 脚本语言换成 Lua（除非另立里程碑）；不做"单区一万真人"这类原版从未做到过的规模（见 §8.4）。

---

## 2. 现成基线（oracle，只读不改）

这一套是**已经跑通的参照实现**，Rust 版的唯一判据来源。

| 组件 | 位置 | 说明 |
| --- | --- | --- |
| 服务端源码（C#） | `E:\Users\gxh\Documents\GitHub\OpenMir2` | 168,027 行 / 33 项目，本文的移植源 |
| 服务端部署 | `E:\MirServer` | 跑起来的那一套；`start-all.cmd` / `start-all.ps1` / `restart-gamesvr.ps1` |
| 客户端运行目录 | `D:\MirClient-run` | `run-release.cmd`（参数：`127.0.0.1:7000`、1024×768） |
| 客户端编译脚本 | `E:\MirServer\build-client-104.ps1` | Delphi 10.4 + msbuild，输出 `Release\Client` |
| 客户端源码 | `E:\Users\gxh\Documents\GitHub\MirClient` | 只读参照（协议、报文语义） |
| 游戏内容（运行中） | `E:\MirServer\M2GameSvr\Envir` | 跑着的 GameSvr 读的是这一份：**递归 639 个 `.txt`**（B 线实测 2026-10-10）+ `data.db`、`Map\`、`Castle\` 等 |
| 游戏内容（另一份副本） | `E:\MirServer\Mir200\Envir` | 同源副本（递归 634 个 `.txt`），**实现前先确认该读哪一份**，不要两处都改。**2026-10-10 血统核定（S1）**：`Mir200` 整个目录是**旧血统内容副本**（macOS 发布件、`.NETCoreApp,Version=v6.0`、程序集名还是 `GameSvr.dll`、引用已不存在的 `SystemModule.Sockets.AsyncSocketServer`），其 `GameSvr` 是 Mach-O ⇒ **本机不可运行、不是 oracle**，只当内容备份用；见 §2.1 |

**进程 / 端口表（实测监听）**

| 进程 | 端口 | 源码目录 | 职责 |
| --- | --- | --- | --- |
| mysqld | 3306 | 外部 | 三个库：`mir2_account` / `mir2_data` / `mir2_db` |
| DBSrv | 5100 / 5700 / 6000 | `src/DBSrv` + `src/Storeages` | 玩家数据读写，存储插件按名动态加载 |
| LoginSrv | 5500 / 5600 | `src/LoginSrv` | 账号校验、服务器列表 |
| GameSvr (M2) | 5000 | `src/GameSrv` + `src/M2Server` + `src/Modules` | 游戏世界 |
| LoginGate | 7000 | `src/LoginGate` | 客户端入口（登录） |
| SelGate | 7100 | `src/SelGate` | 角色网关（建/删/选角） |
| GameGate | 7200 | `src/GameGate` | 游戏网关（世界内操作） |

**测试账号**：`mir2test / mir2pass`；角色 `aaa`(战士) / `bbb`(道士)。
**已跑通链路**：登录 → 选服 → 选人 → 建角 → 进游戏 → 战斗 → Alt+X 小退 → 再进。

### 2.1 谁是 oracle：部署件契约与版本对账（S1，2026-10-10）

「哪些目录里跑的是 oracle」此前只散落在 `E:\MirServer\start-all.ps1` 的 `$Components` 里，
结果 `E:\MirServer` 下 5 个同名近似目录（`Mir200`/`CloudGate`/`DBServer`/`LoginSrv`/`MakePlay`）
被反复当成"另一份部署"——它们其实是**旧血统内容副本**（macOS 发布件 + .NET 6，本机不可运行）。
现在把契约写进文档，并给出一条可复跑的对账命令：

| 组件 | 进程实际启动自 | 说明 |
| --- | --- | --- |
| DBSrv | `src/DBSrv/bin/Release/DBSvr.exe` | 直接从源码构建输出启动，无独立部署副本 |
| LoginSrv | `src/LoginSrv/bin/Release/LoginSrv.exe` | 同上 |
| GameSvr | `E:\MirServer\M2GameSvr\GameSrv.exe` | **唯一的独立部署副本**，部署件必须与源码构建输出逐字节一致 |
| LoginGate / SelGate / RunGate | `E:\MirServer\{LoginGate,SelGate,RunGate}\*.exe` | 同上；各自目录里另有 macOS 发布残留的**无扩展名 apphost**，Windows 上不参与启动 |
| `Mir200` 等 5 个旧目录 | —— | **不是部署件**，仅内容备份（其 `GameSvr` 等 apphost 是 Mach-O，本机跑不起来） |

```powershell
# 对账：组件级判 SINGLE-BUILD / MIXED / FOREIGN / SELF-HOSTED，逐产物判 MATCH / DRIFT / MISSING / EXTRA
powershell -ExecutionPolicy Bypass -File tools/oracle/reconcile_deployment.ps1        # 红检 -SelfTestRed
# 铺部署（同一份构建整体刷新 M2GameSvr，带备份、铺后逐文件复核、写部署来源登记）
powershell -ExecutionPolicy Bypass -File tools/oracle/deploy_gamesvr.ps1             # 预览 -WhatIf
```

判据：对账脚本对契约组件给出 `SINGLE-BUILD`（受管产物全部与源码构建输出逐字节一致）才算过；
`MIXED` = 部署件不是这份源码编出来的。2026-10-10 首次对账时 GameSvr 判 `MIXED`
（`M2Server.dll`/`GameSrv.dll` 是 10-10 00:10 的构建，而 `OpenMir2.dll`/`SystemModule.dll`/
`ScriptSystem.dll`/`CommandSystem.dll`/`PlanesSystem.dll`/`GameSrv.exe` 还是 10-09 的），
已整体刷新为 SINGLE-BUILD（T-3，见 §16）。

---

## 3. 可复用资产（Rust 侧）

| 资产 | 位置 | 怎么用 |
| --- | --- | --- |
| ServerRust（174k 行，kameo actor） | `E:\Users\gxh\Documents\GitHub\Crystal\ServerRust` | **架构与工具照搬**：actor 组织、tick 循环、DB 层、`maps/loader.rs`、内存泄漏探针（`mem-probe`）、测试与门禁写法；`docs/PORT_STATUS.md` 是它的进度表 |
| SharedRust（25k 行） | `E:\Users\gxh\Documents\GitHub\Crystal\SharedRust` | packet trait / 二进制读写框架；**包号空间不同**（它用 Crystal 自定义的 `ServerPacketIds`/`ClientPacketIds`），照抄会全错 |
| Client-Bevy | `E:\Users\gxh\Documents\GitHub\Crystal\Client-Bevy` | 仅作 Rust 侧 UI/协议消费方的写法参考 |

**复用规则**：架构、工具链、测试框架可以搬；**协议与数据模型必须按 OpenMir2 重写**。搬代码前核对许可证与版权头，保留出处。

---

## 4. 架构设计

### 4.1 进程划分：与 C# 对齐

建议 **一个 cargo workspace + 多个 bin**，进程边界与 C# 完全一致（LoginGate / SelGate / GameGate / LoginSrv / DBSrv / GameSvr）。理由：

- 可以**灰度替换**：Rust 的 LoginGate 直接对着 C# 的 LoginSrv，反之亦然；
- 端口、配置、日志格式与现网一致，出问题能立刻切回；
- 不把网关与引擎塞一个进程，避免"一个 panic 全服掉"。

### 4.2 并发模型（关键决策）

- **I/O 层**：tokio；会话、账号、DB、公会、交易这类"接口清晰、彼此独立"的用 actor（kameo）。
- **世界模拟：单线程串行**。理由：经典协议的语义依赖处理顺序（移动的"回显 ACK vs 权威校正"、伤害结算顺序、掉落归属）。并行化会让"差异是我们的 bug 还是调度噪声"永远说不清，而**对拍是唯一判据**。
- **不要把每个实体做成 actor**：kameo 每个 actor 带 mailbox + task + boxed future，热循环里按实体切会把自己埋了。世界内实体用**数据导向的连续存储**（ECS 式结构体数组 / slotmap），actor 只做粗粒度分片。
- **世界分片（M5 之后再说）**：按 zone/map 切，不按实体切；跨 zone 的传送、全服广播、跨图组队交易必须走**确定顺序的交接队列**。这部分原版没有参照行为，需另行设计评审。

### 4.2.1 备选方案：同步 actor / 框架切换（kameo 扛不住时）

**先分清两件事**：世界模拟是**"确定性串行"**，不是**"可并行的 CPU 密集"**。同步 actor（每个 actor 独占线程）只解决后者；对前者，它给不了任何帮助，甚至会把顺序语义打散。所以同步 actor 的价值出现在**分 zone 之后**——那时每个 zone 是一个"独占线程 + 内部严格串行"的单元，正好是同步 actor 的形状。

按代价从低到高：

| 方案 | 形态 | 适用 | 注意 |
| --- | --- | --- | --- |
| **tokio + kameo**（首选） | 异步 actor，跑在 tokio 线程池 | I/O 层：会话、账号、DB 请求、公会、交易 | 已在 Crystal 那条线验证过；**不要**用它做世界热循环 |
| **自建 zone 线程**（推荐给世界层） | `std::thread` + 有界 channel（`crossbeam` / `flume`），每 zone 一个线程，入队项按 `(tick, seq)` 排序 | 世界模拟分片 | 不依赖任何 actor 框架，行为最可控、最好对拍；跨 zone 交接要显式规定顺序 |
| **actix 同步 actor** | `SyncArbiter`（同步 actor 专用线程池） | 纯 CPU 密集、彼此无顺序要求的任务（如批量计算、路径预计算） | **`SyncArbiter` 只存在于 actix 0.12 及以前，0.13 已移除该 API**（官方建议改用线程池 / `spawn_blocking`）。要用就得锁 `actix 0.12`，或直接自建线程池 |
| **其他独占线程的 actor 库** | ractor / coerce 等 | 同上 | 选之前先核对该版本是否真给"actor 独占线程"语义，别按名字假设 |

**换框架的判据（先测后换）**：用假人把**单线程世界**的 tick P99 曲线量出来 → 单线程到顶且分 zone 后仍不够 → 才引入更多线程或换同步 actor。没有这组数据之前换框架，等于用架构复杂度换一个还不知道存不存在的问题。

**禁忌**：① 不要把 DB / 文件 IO 放进 zone 线程（一次阻塞就掉 tick，落库走独立写线程 + 队列）；② 不要按实体切 actor；③ 不要在热循环里跨线程加锁；④ 换框架**不得改变 tick 顺序**——顺序一变，与 C# 的对拍基线就作废。

### 4.2.2 用 Bevy（0.20）承载世界模拟：**已定方案 = 只取 `bevy_ecs` + 试验田先行**（owner 2026-10-11 同意）

**结论（协调者，2026-10-10）**：**网络 I/O 用 Actor 模型——同意，且这已是 §4.2 的既定设计**（会话/账号/DB/公会/交易用 actor，网关用 tokio 每连接任务；框架取舍见 §4.2.1）。
**世界层换 Bevy——同意方向、不同意时机与粒度**：

> **owner 2026-10-11 同意本判断**，故本节的落地形态是：**先做试验田（trial），不重写现有 `crates/world`**。

| 试验田 DoD（缺一不算通过） |
| --- |
| ① 选一个 M3 系统（建议掉落或战斗），用 `bevy_ecs` 实现为"纯函数式系统：输入快照 → 输出命令"，经适配层与现有实体索引对接； |
| ② 该系统对应的对拍用例（如掉落 100 杀列表、战斗伤害序列）与 C# **逐行 diff = 0**； |
| ③ `crates/world` 的既有对拍（14 实体×10 tick、session/drop 场景）**不回归**； |
| ④ `cargo fmt/clippy/test` 全绿，且 P99 不劣化（对比 `tests/parity/world/p99-baseline*.txt`）； |
| ⑤ 试验田过程中若发现"必须依赖 Query 迭代序"或"必须开并行"，**停下上报**——那说明该 ECS 方案与 parity 目标冲突，回到 §4.2 的既有实现。 |

| 主张 | 我的判断 | 依据 |
| --- | --- | --- |
| 用 Bevy 的 ECS 承载实体/组件/系统 | **赞成，但只取 `bevy_ecs`（+ `bevy_app`/`bevy_time`），不要整个引擎** | 服务端不需要 render/winit/wgpu/asset；整引擎会把编译时间、依赖面与版本风险一起带进来 |
| 现在就重写 `crates/world` | **反对** | 骨架刚通过 C# parity（14 实体×10 tick 位置/可见集逐行一致、session/drop 场景一致），并带着两条确定性守卫（`no_parallel.rs` 依赖白名单 + `World::tick` 线程断言）。此刻重写等于把已验证的确定性推翻重来，收益不明 |
| 在 M3+ 的系统层引入 `bevy_ecs` | **赞成** | 战斗/技能/掉落/任务/状态机在 C# 里是深继承对象树（`MonsterObject`→…），ECS 的"组件组合 + 系统"比继续手搓 slotmap 更贴长期形态；而且你 Crystal 那边的 `Client-Bevy` 已有实践，客户端/服务端共享一套 ECS 心智是真实收益 |

**若决定迁移（或新写系统层），四条硬约束（缺一条 parity 就会变 flaky）**：

1. **单线程 executor**：不许用多线程执行器（`MultiThreadedExecutor`/`bevy_tasks` 并行），世界 tick 仍由我们自己的循环驱动；
2. **遍历顺序显式化**：`bevy_ecs` 的 `Query` 迭代序**不是契约**（archetype/table 顺序，官方不保证）。AOI/实体处理顺序必须来自我们自己的有序索引（地图格子 → 实体 id），**不许**依赖 Query 迭代序；
3. **确定性守卫升级**：`no_parallel.rs` 从"禁一切并行依赖"改成"**只允许 `bevy_ecs`/`bevy_app`，仍禁 `bevy_tasks` 并行/rayon/crossbeam**"，并保留 tick 线程断言；
4. **迁移验收＝原判据不降级**：14 实体×10 tick 与 session/drop 场景的对拍必须仍 **diff = 0**，P99 基线不得劣化（对比 `tests/parity/world/p99-baseline*.txt`）；对拍若变成 flaky，回退。

**另需摆上台面的代价**：Bevy 大约每 3 个月一次破坏性升级，而服务端要长期行为稳定（对拍基线就是合同）⇒ 锁版本、只在有明确收益时升级会是常态；`bevy_ecs` 的版本漂移要计入维护成本。

### 4.2.3 服务器分层（与"Bevy 做整台服务器"的分界）

结论：**Bevy/`bevy_ecs` 只做"地图内的权威模拟"，不单独当完整服务器**；Actor 仍是骨架。分层照下面的形状（§0d 的拓扑在其上）。

```
网关 Actor（tokio per-connection：连接/协议/会话/认证/限流）
    -> 输入 channel（玩家的操作意图）
地图/副本：无头 `bevy_ecs` World（权威模拟：移动/战斗/AI/Buff/掉落/技能判定；固定 tick）
    -> 输出 channel（命令/事件）
同步层（**自研**：按 AOI 视距生成快照 + 带宽优先级；不用现成引擎的复制方案）
全局服务 Actor（聊天/组队/公会/拍卖/邮件/DB）
```

三条与本项目的口径（别人给的通用 MMO 结论里，这两点对我们**不成立或要改写**）：

1. **"ECS 并行调度更高效"——我们不用它的并行**：验收是与 C# 逐行一致的确定性，所以 `SingleThreadedExecutor` + 禁 `MultiThreadedExecutor`/`bevy_tasks`/`rayon`（§4.2.2 已写成可证伪守卫）。这里的收益来自**存储布局与批量遍历**，不是并行。
2. **"分片天然、跨图走消息"——我们本来就是单进程单 World**：经典 Mir2 的 oracle 是**单 GameSvr 单区**（`!servertable.txt` 只指一个 7200 网关），跨图靠"会话进/出世界"（M2 的 session 切片），不存在无缝大地图或万人同服；所以 Bevy 的"分片优势"在我们这里用不上，也不需要。
3. **同步层自研**：经典协议 + AOI 视距（我们 golden 里的 `vis` 列表就是它）必须逐字节复刻，Replicon/Lightyear 之类的现成复制方案带不来对拍价值。

**规模实测（用以回答"Bevy 够不够"）**：同屏是**十几到几十个实体**（AOI 视距决定的 11×9 格窗口）；M2 骨架在 500/1000/2000 实体下 tick 占预算 0.33%/0.82%/1.94%；D3 试验田的 ECS 路径代价 **+10µs / 32 只怪的掉落批次**（常驻 world P50 21µs vs 直接调用 11µs，≈1.8×；命令 `cargo run -p mir2-world-bevy --release --bin drop_bench`，基线 `mir2-rs/tests/parity/world/d3-drop-bench-rebuild.txt`）——**口径**：掉落预算在**怪物生成时**一次性预算（C# `MonGetRandomItems`），不是每 tick 一次，故不按 tick 折算成百分比。⇒ 对我们这个"单区、同屏小、要求确定性"的目标，`bevy_ecs` **绰绰有余**，瓶颈从来不是 ECS 的吞吐。

### 4.3 协议与编解码

| 关注点 | C# 参照 | 要求 |
| --- | --- | --- |
| 消息号表 | `src/OpenMir2/Messages.cs` | **以服务端为准**。已知坑：`SM_ATTACKMODE` 是 **213**（客户端分支曾把 213 给 `SM_HERODELMAGIC`，已改成 546 对齐服务端）；`SM_HERODELMAGIC` 等由该分支私有的号，若服务端不发就保持"永不触发" |
| EDCode 加解密 | `src/OpenMir2/EDcode.cs` | 逐字节兼容，含 base64 变体与密钥来源；用 C# 侧生成测试向量做对拍 |
| 帧格式 | `src/OpenMir2/Messages.cs`（`DefBlockSize=16`、`MakeMessage`）+ `ClientMsg` 结构 | 客户端发来的每个包都要能解出 `ident / Recog / param / tag / series / body`；**字段归属别猜**：`SendDefMessage(ident, nRecog, nParam, nTag, nSeries)` ⇒ 模式、ID 这类值通常在 `Recog` |
| 启动参数 | 客户端 `uEDCode.DecodeSourceData(ParamStr(1), ...)` | Release 版客户端必须带加密启动串；Rust 侧不需要生成它，但调试时要能复现 |

### 4.4 状态机

参照 C# 各 Gate 的"连接阶段"位与 `g_ConnectionStep`：`未连接 → 已连接 → 登录中 → 已选服 → 选角中 → 进世界中 → 断线/小退`。每个阶段允许的报文集合要与 C# 一致（**非法阶段收到包的处理方式也要一致**：丢弃 / 断开 / 记录"非法攻击"）。

---

## 5. 数据与内容兼容

### 5.1 MySQL

**库 / 表归属（按 C# 实测的读写点，Rust 侧照这个边界切）**

| 库 | 谁读写 | 表 |
| --- | --- | --- |
| `mir2_data`（游戏数据） | GameSvr 启动时读 | `stditems`、`monsters`、`magics`、`goldsales` |
| `mir2_account`（账号） | LoginSrv | `account`、`account_protection`（**JOIN 关系，缺 protection 行会导致"资料读取失败"**） |
| `mir2_db`（角色数据） | DBSrv → 存储插件 | `characters`、`characters_ablity`、`characters_bonusability`、`characters_item`、`characters_item_attr`、`characters_magic`、`characters_bagitem`、`characters_storageitem`、`characters_status`、`characters_quest`、`characters_indexes`、`marketitem`、`marketitems`、`Item` |

初始化脚本在 `sql/`：`mir2_account.sql`(32,560 行) / `mir2_data.sql`(2,043 行) / `mir2_db.sql`(677,715 行)。**表结构一字不改，也不需要迁移**：直接用现成 dump 建库。

**技术选型：`sqlx`（不用 ORM）**

```toml
sqlx = { version = "0.8", features = ["runtime-tokio", "mysql", "chrono", "macros", "migrate"] }
```

- **不用 diesel / sea-orm**：schema 是既有事实、不需要建模，`sqlx` 的"真实 SQL + 结构化映射"最贴合移植场景。
- **不启用任何 `tls-*` feature**：连的是本机 MySQL（3306），明文即可；sqlx 只在需要 TLS 时才要求选一个后端。
- **查询写法**：默认用运行期 `sqlx::query_as` + 行结构体（`FromRow`），好处是编译不依赖数据库、换机器/CI 无额外配置；如果希望每条 SQL 都在编译期对着真实 schema 校验，可对**关键查询**改用 `query!` 宏，条件是构建环境能连到库、或把 `cargo sqlx prepare` 生成的 `.sqlx/` 离线元数据入库（两种做法别混用小半个仓库，选一种写进 README）。
- **连接池**：`MySqlPoolOptions` 容量按"访问是定时批处理而非每请求"来定，**8~16 条足够**（DBSrv/GameSvr 都在内存里维护玩家缓存，落库是周期性 + 事件驱动）。别按 QPS 拍脑袋开大池子，MySQL 侧反而会被打爆。
- **事务**：保存一个角色 = 多表写入（`characters*` + `market*`），**必须放在同一个事务里**，与 C# 的保存语义一致；存档失败要反馈给客户端（这是上一批修过的行为，别丢）。
- **批量**：周期存档（`TimedService` 的 1 秒 PeriodicTimer 触发的那批）用批量 INSERT/REPLACE 或 `QueryBuilder` 拼多值，别一条条 round-trip。
- **类型映射**：注意 `PassWord char(21)`（明文，别顺手哈希——行为等价优先）、时间列用 `chrono`、金额/经验字段保持原宽度（**别用 i32 截断**，英雄经验曲线就踩过这个）。
- **列语义要逐列核对**：`characters_indexes.SelectID` 这类"名字像索引、实际兼作状态标记"的列，按名字猜必错（C# 用它过滤角色列表，结果把玩过的角色藏起来）。

### 5.2 Envir 脚本（最容易低估的一块）

- 运行中那份在 `E:\MirServer\M2GameSvr\Envir`：**652 个 `.txt` / 35,756 行**（另有一份同源副本 `E:\MirServer\Mir200\Envir`，666 个 / 35,787 行）。这些是**经典 Mir2 脚本语言**，不是数据。
- 参照实现：`src/Modules/ScriptEngine/`（`ScriptEngine.cs` / `ScriptParsers.cs` / `ConditionCode.cs` / `ExecutionCode.cs` / `GrobalVarCode.cs` / `Grobal2.cs` 里的宏常量）。
- 通用语言库（rhai / mlua）**不能替代**，要么照 C# 逐条移植解释器，要么挂 Lua 再写几百个命令 binding；推荐**照 C# 逐条移植**，因为对拍只需要对行为、不需要对设计。
- 验收判据：同一批脚本下，Rust 侧解析出的"语句/条件/动作"统计与 C# 侧一致；关键 NPC（商店、传送、任务、回复）逐个跑通。

### 5.3 随机数（不显眼但必踩）

- 参照 `src/OpenMir2/RandomNumber.cs`（内部是 `System.Random` 的包装 + 若干取整/取范围工具）。
- 掉落、暴击、怪物 AI、洗装备全部走同一随机源。**Rust 的 `rand` 默认算法与其序列不同** ⇒ 数值手感立刻变。
- 做法：在 Rust 里复刻同一算法与调用顺序；用"固定种子 + 同一操作序列"的对拍来验证（同一场景两边输出同一串数）。

### 5.4 存档与生命周期

参照 `src/GameSrv/Services/FrnEngn.cs`（`FrontEngine`：`m_SaveRcdList` / `m_LoadRcdList`）、`src/GameSrv/Services/PlayerDataService.cs`、`src/GameSrv/TimedService.cs`（1 秒 `PeriodicTimer` 做检查/存档/清理/排行）、`src/M2Server/Word/Threads/CharacterDataProcessor.cs`（读档）。
要求：**保存/读档的时机、重试、踢人判定**逐条对齐——这里藏着一个已知坑（见 §8.1）。

---

## 6. 里程碑与验收

### 6.0 完成定义（DoD，适用于每一个里程碑）

1. **判据必须可测**：每条验收都要能写成"跑什么命令 → 看什么数 → 阈值多少"。**不接受**"目测通过""基本一致""行为正常"这类描述。
2. **必须能证伪**：每条门禁都要给出**红检方式**（故意改坏哪一处 → 必须变红），并附红检输出。恒绿的门禁等于没有。
3. **差异要么 0，要么进白名单**：允许的差异必须写进 `tests/parity/whitelist.md`，每条写明「差异点 / 为什么无害 / 谁审的」。**没登记 = 不过**。
4. **证据四件套**：可复现的命令 + 原始输出 + 截图/日志文件路径 + 产物 hash（或提交号）。**没有证据 = 没做**。
5. **跨里程碑不许夹带**：当前里程碑未验收通过前不写下一阶段代码（尤其是 M0/M1 未过不许动引擎）。

### M0 —— 协议层 + 抓包回放门禁（预计 1 周）

- 交付：Rust `mir2-protocol` crate（消息号表 + 帧解析 + EDCode）；一份**金标准抓包**（真实客户端"登录→选人→建角→进游戏→战斗→小退→再进"的完整流量）。
- 验收：
  1. **逐字节回放**：金标准里全部 N 个包（N 必须在报告里写明）decode→encode 后与原字节 **100% 相同**；
  2. **字段对拍**：每包的 `(ident, Recog, param, tag, series, body 长度, body hash)` 与 C# 侧同一时刻日志逐项一致，**差异数 = 0**；
  3. **覆盖断言**：包号集合必须覆盖登录 / 选服 / 选角 / 建角 / 进世界 / 移动 / 攻击 / 小退八个阶段，且报告里"未实现的包号"数量为 **0**；
  4. **红检三连**：改 EDCode 密钥 1 字节 → 必须红；某包号 +1 → 必须红；body 截断 1 字节 → 必须红（三条都要留输出）。
- 证据：回放工具命令与输出、字节比对报告、包号覆盖清单、三条红检输出。

### M1 —— 边界服务（预计 2~3 周）

- 交付：Rust 版 `LoginGate` / `SelGate` / `LoginSrv` / `DBSrv`（≈2.1 万行 C# 规模），数据层按 §5.1 用 `sqlx`。
- 验收：
  1. **混跑两条链**：真客户端 → Rust `LoginGate` → **C#** `LoginSrv` 完整登录成功；**C#** `SelGate` → Rust `DBSrv` 能取到角色列表（两个方向各跑通一次，截图 + 两侧日志时间线）；
  2. **认证正反例**：Rust `LoginSrv` 用真实 `mir2_account` 校验 `mir2test/mir2pass` 通过、错密码必须失败（两个用例都要有），且返回码/文案与 C# 一致；
  3. **跨实现互读**：Rust `DBSrv` 写入 `mir2_db` 后，用 C# `DBSrv` 读同一角色，字段级 diff = 0；反向亦然；
  4. **失败要响**：缺配置 / 连不上 MySQL / 存储插件缺失时必须**明确报错退出**（不是静默空转），错误文案与 C# 同类错误可比对；
  5. **N=200 假人**同时登录+选角：成功率 100%，无 panic，活跃字节曲线平稳（判据同 §6.0-2）。
- 证据：两条混跑链的截图与日志、认证正反例输出、跨实现互读 diff 报告、200 假人压测曲线、进程退出码。

### M2 —— GameGate + 世界最小闭环（预计 3~5 周）

- 交付：Rust 版 `GameGate` + `GameSvr` 的"最小可玩世界"：进图、走路、说话、看见别人、打怪、捡物、小退再进。
- 验收：
  1. **客户端可玩证据**：进图 / 走路 / 说话 / 看见别人 / 打怪 / 捡物 / 小退再进，**各一张截图**，且小退再进不黑屏（对照 `D:\MirClient-run` 的真实客户端）；
  2. **同序列对拍**：与 C# 版并行跑**同一份 `mir2_db` 与 Envir 脚本**，同一操作序列下两侧出包差异数 = 0（或全部落在 `tests/parity/whitelist.md`）；
  3. **存档三路径**：小退 / 掉线 / 服务关闭三种退出路径后，DB 里角色数据与 C# 版**字段级 diff = 0**（含物品、技能、坐标、状态）；
  4. **不回归**：C# 版原流程与客户端 E2E 仍全绿（证明没为了新世界改坏参照实现）。
- 证据：七张截图、出包 diff 报告 + 白名单、三条退出路径的 DB diff、回归套件输出。

### M3 —— 游戏系统全量对齐

- 顺序：背包/装备 → 战斗/技能 → 掉落/经验 → NPC/商店 → 任务 → 组队/交易 → 公会/攻城 → 英雄/宠物 → 邮件/拍卖。
- **逐系统判据（每个系统都必须给"固定条件下两侧结果逐项相同"的报告，禁止目测）**：

| 系统 | 验收判据（可测） |
| --- | --- |
| 背包 / 装备 | 穿戴、脱下、负重上限、持久消耗、属性面板数值 —— 同一操作序列下数值逐项相同；边界用例（负重满、等级不够、性别不符）都要有 |
| 战斗 | 固定随机种子下 **100 次攻击的伤害序列逐项相同**；命中/躲闪统计与 C# 一致 |
| 技能 | 每个已实装技能各 ≥1 条用例：施法前置、MP 消耗、效果、冷却，逐项一致 |
| 掉落 / 经验 | 固定种子下同一怪 **100 次击杀的掉落列表逐项相同**；经验曲线（含英雄）逐级相同，**不得出现 i32 截断** |
| NPC / 商店 | 买 / 卖 / 修理 / 仓库的价格与库存变化一致；钱不够、负重满等失败分支也要一致 |
| 任务 | 用现存脚本任务跑：接 / 交 / 条件判定一致，任务状态落库一致 |
| 组队 / 交易 | 两人及以上场景：归属、分配、取消、掉线中断的结果一致 |
| 公会 / 攻城 | 建会 / 宣战（费用、时限）/ 城门状态 / 自动结束，逐项一致 |
| 英雄 / 宠物 | 召唤 / 升级 / 阵亡 / 经验分配 / 上限判定一致 |
| 邮件 / 拍卖 | 发送 / 到期 / 成交 / 退回，DB 落库字段级 diff = 0 |

- **门禁（owner 2026-10-10 决定：M3 夹具正式提升为门禁）**：`mir2-rs/tests/parity/m3/run_pair.ps1 -Side csharp|rust`
  跑同一操作序列 → `scoped_diff.py diff --canonical` 作用域内**字段级 diff = 0**；两侧快照 hash 与 RNG 种子注入结果都要入库。
  **规范化是判据的组成部分（2026-10-11，owner 裁决 A）**：`row_sha256` 覆盖 `SELECT *` 全列，而
  (a) 自增 PK/FK、墙钟 `datetime`/unix 秒**每次运行必变**，(b) `characters.Hair/Sex/Job` 是
  夹具**建角输入随机**（`src/BotSrv/Scenes/Scene/SelectChrScene.cs:154-176`，`RandomNumber` 无种子）。
  不排除这些列，**两次都 GREEN 的 C# 会话互 diff 也判红**（实测 8 行），判据无仲裁力。故 `--canonical`
  按**逐列登记（含每列理由）**的易变列白名单置 `<VOLATILE>`，其余列逐字节比，**不许整表 ignore**，
  未登记表 fail-closed；并含「改坏必红」自检（`scoped_diff.py selftest`）。
  **差异计数是多重集/计数式（2026-10-11，卡 `openmir2-m3-scopeddiff-multiset`）**：`diff`/`delta` 按
  `(表, 行 key)` 的**出现次数**比对，不用 set 差集——否则重复行 `A=[x,x]` vs `B=[x]` 会 `仅A=仅B=0`
  而 **exit 0（假绿）**；「重复行数」本身是判据的一部分（多出一行计 1，判红）。`Hair/Sex/Job` 的行为覆盖
  迁至固定参数的确定性场景，见卡 `openmir2-m3-botsrv-fixed-newchr`。
  **前置更正（2026-10-11）**：7200 的游戏侧入口是 RunGate/GameGate，M1 不实现；首验的真实前置是
  M2 的 Rust GameGate 合入 master（卡 `openmir2-m2-gamegate`），而非 M1 顶住端口。
  M1 边界服务能顶住 7000/7100/7200 之后，立即用 `-Side rust` 跑一次作为 M3 首次验收。
- 总验收：**上述十项全绿 + M3 门禁 diff=0 + 300 假人长跑 24h 无 panic、无掉线**。
- 证据：每系统一份"命令 + 输出 + 差异报告（可为 0）"，汇总成 `tests/parity/M3-report.md`。

### M4 —— 脚本引擎 + 内容兼容

- 交付：脚本解释器（§5.2）+ 全部内建命令。
- 验收：
  1. **全量加载**：运行中那份 `E:\MirServer\M2GameSvr\Envir`（**递归 639 个 `.txt`**）全部加载，并与 C# 参照**逐行结果一致**。
     **口径修订（2026-10-10，B 线实证）**：原判据"解析错误 = 0"不可达——同一份语料下 C# 参照自身也产出 **427 条**脚本错误（`TakeOn`/`GAMEGIRD`/`ReadRandomLine` 等命令在 C# 源码中不存在，属 LEGM2 等变体遗留）。判据改为"**双侧逐行多重集差 = 0**"，双方数字与错误行都要贴。
  2. **解析对拍**：语句 / 条件 / 动作 / 宏展开的条数与 C# 侧一致（给出两边的数）；
  3. **未实现命令必须显式报错**：不许静默跳过；报错清单收敛到 **0**；
  4. **关键 NPC 用例**：商店、传送、修理、仓库、行会、任务各 ≥1 个用例，副作用（给物 / 给经验 / 传送 / 扣钱）与 C# 一致。
- 证据：加载统计、解析对拍数表、未实现命令清单（应为空）、关键 NPC 用例输出。

### M5 —— 收敛、压测、灰度

- 交付：对拍差异收敛报告、压测报告（假人 N 个在线，tick P99 数据）、灰度与回滚方案。
- 验收：
  1. **长跑**：连续 **72h** 无崩溃、无未捕获 panic；日志中 ERROR 不呈上升趋势（给日志曲线）；
  2. **内存**：`mem-probe` 活跃字节在最后 1 小时的**斜率 ≈ 0**（给出具体阈值，例如峰值-均值 < 5%），RSS 曲线附图；
  3. **压测三档**：200 / 500 / 1000 假人，记录 tick **P99**（目标 ≤ 100ms）、登录成功率 100%、无掉线，附三档曲线；
  4. **灰度与回滚**：Rust 服务逐个替换后 E2E 用例集全绿；**实际演练一次回滚**（记录命令与耗时）；
  5. **数据安全**：灰度前后的 DB 快照对比无字段丢失/类型损坏。
- 证据：日志与曲线图、三档压测报告、回滚演练记录、DB 快照 diff。

---

## 7. 测试与门禁

| 手段 | 现成资产 | 用途 |
| --- | --- | --- |
| 客户端出包追踪 | 之前的会话在客户端发送器上做过 hook；`E:\tmp\mirflow2.ps1`（全流程自动跑 + 截图）、`E:\tmp\amtest.ps1`（登录 + 点击 + 截图） | 抓金标准、做 E2E 回归 |
| 服务端日志 | `E:\MirServer\logs\*.out.log` / `*.err.log`（每个进程一份） | 与客户端出包逐条对时间线 |
| 假人 | `src/BotSrv`（C#，21k 行）+ Crystal `world/robot.rs` | 多人并发压测；也是"行为对拍"的放大器 |
| 内存门禁 | Crystal `ServerRust/src/mem_probe.rs` | 区分真泄漏与分配器高水位 |

要求：门禁必须**能证伪**（改一处必红，而不是恒绿）；长跑类门禁要断言曲线而不是单点。

---

## 8. 风险与对策

### 8.1 行为等价风险（最大）

C# 服务端里有一批"行为藏在细节里"的坑，逐行移植时不会自己浮出来，只能靠对拍。已验证的两个实例：

- **小退再进黑屏**：`PlayerDataService.ProcessSaveQueue()` 里"存档回执后把角色从正在保存列表移除"的那行 `RemoveSaveList(queryId)` **被注释掉了**，叠加 `CharacterDataProcessor` 把"保存中请重试"当成失败踢人 ⇒ 小退后永久进不去。
- **登录报"获取账号资料出错"**：上游 SQL 用 `account INNER JOIN account_protection`，缺 protection 行时函数仍返回"成功但空记录"。

⇒ 策略：**先 1:1 复刻（连 bug 一起），对拍全绿后再单开批次修**；每个修复都要有"改前/改后"两侧行为记录。

### 8.2 并行化风险

见 §4.2。任何并行化上线前必须证明"输出顺序与单线程一致"（用同一序列重放 N 次，结果逐字节相同）。

### 8.3 生态缺口

Rust 生态覆盖不了的只有三块：EDCode（自研加密）、经典脚本语言、原版 RNG。前两个手写，第三个手写算法。除此之外（tokio / kameo / sqlx / byteorder / serde / tracing）都够用。

并发框架不是"选一个就定终身"：I/O 层与世界层的选型可以分开（§4.2.1），换框架的唯一硬约束是**不得改变 tick 顺序**（否则与 C# 的对拍基线作废）。

### 8.4 规模风险

原版引擎是单线程世界循环，"单区万人"是原版从未达到过的规模，既没有参照行为也没有调参经验。建议先把单线程世界压到极限（用假人量出 tick P99 曲线），再决定是否分 zone；不要在设计阶段就为万人做复杂分片。

### 8.5 运维差异

进程数、端口、配置文件（`.conf`）、日志格式尽量与现网一致，便于灰度与回滚；Rust 侧建议保留"配置项名一致"这一条，运维脚本不用改。

---

## 9. 目录与命名建议（Rust 侧）

```
mir2-rs/
  Cargo.toml            # workspace
  crates/
    protocol/           # 消息号表 + 帧 + EDCode（M0 产出）
    shared/             # 公共工具（时间/日志/配置/HUtil32 等价物）
    storage/            # MySQL 访问（对应 DBSrv + Storeages）
    script/             # Envir 脚本解释器（M4）
  bins/
    login-gate/  sel-gate/  game-gate/
    login-srv/   db-srv/    game-svr/
  tests/                # 回放 / 对拍 / E2E
  tools/                # 抓包、回放、假人驱动、截图脚本（把 E:\tmp 那几支搬进来）
```

`crates/storage` 的依赖（数据层统一走 sqlx，理由与配置见 §5.1）：

```toml
[dependencies]
tokio  = { version = "1", features = ["full"] }
sqlx   = { version = "0.8", features = ["runtime-tokio", "mysql", "chrono", "macros", "migrate"] }
serde  = { version = "1", features = ["derive"] }
tracing = "0.1"
thiserror = "1"
# 备选（见 §4.2.1，按需启用，不要一上来全加）：
# kameo   = "0.20"        # I/O 层 actor
# crossbeam-channel = "0.5"   # zone 线程的有界队列
# actix   = "0.12"        # 只有确实要用 SyncArbiter 时才锁这个版本
```

---

## 10. 附录：关键文件对照表

| 关注点 | C# 文件 | Rust 侧目标 |
| --- | --- | --- |
| 消息号表 | `src/OpenMir2/Messages.cs` | `crates/protocol/messages.rs` |
| 报文结构/工具 | `src/OpenMir2/Grobal2.cs`、`SerializerUtil.cs`、`MemoryCopy.cs` | `crates/protocol/frame.rs` |
| EDCode | `src/OpenMir2/EDcode.cs`、`EncryptUtil.cs`、`MD5.cs` | `crates/protocol/edcode.rs` |
| 随机数 | `src/OpenMir2/RandomNumber.cs` | `crates/shared/rng.rs` |
| 登录服务 | `src/LoginSrv/**` | `bins/login-srv` |
| 数据库服务 | `src/DBSrv/**` + `src/Storeages/DBSvr.Storage.MySQL/**` | `bins/db-srv` + `crates/storage` |
| 网关 | `src/LoginGate`、`src/SelGate`、`src/GameGate` | `bins/*-gate` |
| 世界引擎 | `src/GameSrv/**`、`src/M2Server/**` | `bins/game-svr` |
| 脚本引擎 | `src/Modules/ScriptEngine/**` | `crates/script` |
| 系统模块 | `src/Modules/{ChatSystem,MarketSystem,UserStallSystem,...}` | `crates/systems` |
| 假人 | `src/BotSrv/**` | `tools/bot`（压测用） |

---

## 11. 开工前必须做的三件事

1. **跑一遍基线并留证**：`E:\MirServer\start-all.cmd` → 客户端 `D:\MirClient-run\run-release.cmd` → 登录 `mir2test/mir2pass` → 进游戏 → 截图存档；同时确认运行中的 GameSvr 用的是哪一份 `Envir`（默认 `E:\MirServer\M2GameSvr\Envir`）。
2. **抓一份金标准流量**：从进程启动到"进游戏 → 打怪 → 小退 → 再进"全程，客户端出包 + 服务端日志 + 时间戳，落到 `tests/golden/`。
3. **读一遍本文 §1 与 §8.1**，把"先 1:1 连 bug 一起搬"写在实现计划里。

---

## 12. 并行开工方案（多 agent）

### 12.1 先分清两种"依赖"

- **真依赖：行为真值**。在没有对拍基准（金标准流量 + 差异工具）之前，每一路写出来的东西都只能"自认为对"，无法仲裁——这才是必须串行的部分。
- **假依赖：接口**。协议类型、消息号表、storage 接口都可以**从源码直接声明出来**（`Messages.cs` / `sql/` 已经是完整规范），不必等实现验证。接口一旦冻结，下游就能并行开工。

结论：**不该按 M0→M5 无脑串行，但也不该无脑并行**——正确切法是"先冻接口、再开线，每条线自带可证伪判据"。

### 12.2 可以立刻并行的四条线（不等 M0 完成）

| 线 | 内容 | 自己的判据 | 前置 |
| --- | --- | --- | --- |
| **A｜协议**（关键路径） | `mir2-protocol` crate：消息号表 + 帧 + EDCode；回放门禁 | §6 的 M0 四条 | 无 |
| **B｜脚本引擎** | Envir 脚本解析/执行（对应 `src/Modules/ScriptEngine`），自成一体 | 634 个 txt 解析错误 = 0；语句/条件/动作条数与 C# 一致；未实现命令清单收敛到 0 | 只需 C# 参照 + 脚本样本，**不依赖协议** |
| **C｜工具与夹具** | 抓包代理、假人驱动、日志 diff、DB 快照、E2E 脚本（`E:\tmp\mirflow2.ps1`/`amtest.ps1` 搬进 `tools/`） | 工具自身有红检；能在**现有 C# 栈**上跑通 | 客户端 + C# 服务端（今天就能做） |
| **D｜数据与公式** | 物品/怪物/技能表加载、`RandomNumber.cs` 的 RNG 复刻、经验曲线（含英雄）、掉落公式 | 固定种子下与 C# 输出序列一致 | MySQL 可读即可 |

其中 **C 线产出的金标准抓包是 A 线的输入**——所以 C 线应该最先起，A 线拿到抓包后一两小时内就能验证协议层，M0 的关键路径会被压缩到很短。

### 12.3 必须等"接口冻结"的线

| 线 | 等什么 |
| --- | --- |
| M1 网关 / LoginSrv / DBSrv | A 线的协议类型 + `crates/storage`（sqlx）接口 |
| M2 世界最小闭环 | 上述两项 + 世界数据结构（实体/地图/会话）定义 |
| M3 系统对齐 | 上述全部 + 各系统与 C# 的模块边界 |

### 12.4 并行的三条硬前提

1. **接口冻结日**：第 1 天就从 `Messages.cs` / `sql/` 生成 Rust 的消息号类型、帧类型与 storage trait，打 tag 冻结；之后改接口必须走评审（改接口 = 全线返工）。
2. **一线一判据**：每条线必须有自己能"证伪"的判据，否则并行产物无法仲裁。反面教材就是 Crystal 那条线：8 月一周写 13 万行、9 月 13 天全在修对齐（`fix 68 / feat 32`）——量是对的，仲裁机制没跟上。
3. **一模块一 owner**：同一文件/模块同一时间只有一个 agent 写；跨模块改动先改接口、再加实现。世界模拟是单线程串行的（§4.2），所以 M2/M3 只能**按系统模块**切 owner，不能按实体切。

### 12.5 建议编队（3~4 个 agent）

```
C（工具/金标准抓包）──┬─→ A（协议 + M0 门禁）─→ M1 ─→ M2 ─→ M3
                      │
B（脚本引擎）─────────┴───────────────────────────→ 汇入 M3 / M4 验收
D（数据与公式）────────────────────────────────→ 汇入 M3 验收
```

节奏建议：**C 先起**（抓包 + 假人 + diff 工具），**A 拿到抓包后立刻验证协议并冻结接口**，**B/D 全程独立推进**，M1 在接口冻结后开工；M1~M3 必须串行（共享世界状态，无法按实体切）。

---

## 13. 首批（A/B/C/D）验收台账与下一批任务

### 13.1 协调者独立复核结果（2026-10-10，master `03ca1f90`）

复核者重跑了全部机械门禁，结论以此为准（不采信自述）：

| 门禁 | 复核结果 |
| --- | --- |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo test --workspace` | 全绿（含 B 批次 2 合并后复跑） |
| `cargo run -p msgcodegen -- --check` | exit 0（604 条常量与 `Messages.cs` 一致） |
| `cargo run -p replay -- tests/parity/vectors/oracle_vectors.jsonl` | **GREEN**（56 帧 + 1072 EDCode 向量，①0/②0，八阶段全覆盖） |
| `cargo run -p replay -- tests/golden/c-line-baseline-20261010-092858.jsonl` | ①0/②0，**③ 5/8 → RED**（详见 13.3） |
| `dotnet build src/BotSrv/BotSrv.csproj -c Release` | 0 error（C 线工具侧改动可编） |
| oracle 运行时二进制 | `GameSrv.dll` 2026-10-10 00:10:36、三网关 2026-10-09 01:18 —— **未被任何线动过** |

### 13.2 各线判定

| 线 | 判定 | 依据 |
| --- | --- | --- |
| A 协议 | **M0 部分通过**：①②在真实流量上闭环，③未满足 | `tests/parity/evidence/M0/金标准验收报告.md` 及 4 份红检日志；同时抓到真问题：客户端独有号 **3501**（`CM_QUERYDYNCODE`）已登记 `client_only` 并加单测钉住 |
| B 脚本 | **批次 1+2 通过**（解析层 + 求值骨架，含复刻 C# 的 5 处 bug、cp936 7032 条对照表、93 个派发目标清单）；**M4 未完成** | `tests/parity/whitelist.md` B-101~B-106；批次 2 已由协调者合并（`03ca1f90`）并复跑门禁 |
| C 工具 | **工具链通过**；金标准覆盖 5/8 阶段；**一处冻结基线违规（已按例外处置）** | `tools/`、`tests/golden/`、`mir2-rs/tests/golden/C线-交付说明.md` |
| D 数据/公式 | **四项判据全通过** + 10 条红检（7 自动 + 3 人工，附输出） | `tests/parity/D线报告.md`、`whitelist.md` D-1~D-6 |

### 13.3 待解阻塞：M0 ③ 阶段覆盖 5/8

缺 **移动 / 攻击 / 小退**。根因（C 线实证）：影子网关把 oracle 网关挪到 17000/17100/17200，而 GameSvr 按网关端口对齐进世界路由 ⇒ 世界数据下不来。

**协调者决定（2026-10-10）**：不改 oracle 端口/配置（那会污染冻结基线），改走**客户端侧 dump**（客户端 → 真实端口 7000/7100/7200 直连，客户端进程内 hook send/recv 落盘）。
依据：① 拓扑不变 ⇒ 世界路由正常；② 客户端侧改动不触碰 oracle；③ 该手法在本项目历史上已验证（客户端发送器出包追踪）。
`pktmon` 不在候选内：本机非管理员且驱动不可用（实测"无法与 PktMon 驱动程序通信"）；即便提权，回环可见性仍需另行实测。

### 13.4 协调者已处置 / 已批准事项

1. **T-1（BotSrv 例外）**：C 线改动 `src/BotSrv/**`（`LogService` 引用修正、空 body 解码守卫、新增 `SocketShim.cs`）—— 判为**工具侧例外并接受**（不在 oracle 七进程内、可编过、不影响 oracle 行为），要求在 `whitelist.md` 登记 `T-1`；此后任何 `src/**` 改动必须先申报。
2. **口径修订**：M4 验收① 由"解析错误=0"改为"双侧逐行一致"（C# 自身 427 条，见 §6 M4）；Envir 文件数按实测 **639**（`M2GameSvr\Envir` 递归）/ 634（`Mir200\Envir`）。
3. **白名单批准**：D-1~D-6、B-103~B-106 全部批准（均有证据、均符合"行为等价优先于修 bug"）；D-5（`goldsales` 复刻错列名）与 B-8（命令码位移）**保留复刻、不修**，修复另立批次。
4. **合并**：B 线批次 2 已并入 master（`03ca1f90`）并复跑门禁。
5. **登记未修（属产品决策）**：① GameGate 重连后 GameSvr 网关槽位置 null ⇒ 进世界 NRE 死循环直到重启；② `Mir200` 下 `GameSvr` 是 Mach-O，与源码版本漂移。

### 13.5 下一批任务（已开进 walgit 看板，5 张卡片）

> 卡片：openmir2-c2-golden-dump(worker-1) / openmir2-a2-m1-services(worker-2) / openmir2-b2-script-dispatch(worker-3) / openmir2-d2-m3-fixtures(worker-4) / openmir2-s1-oracle-defects(需要人)。
> 每条线的判据与收尾要求都写在卡片正文里；协作与收尾流程见 §14。

| 线 | 任务 | 判据（对应章节） | 前置 |
| --- | --- | --- | --- |
| **C2**（关键路径） | 客户端侧 dump 实现 + 重抓金标准，补齐 **8/8 阶段** | §6 M0 ③；重跑 `replay` 应 **GREEN** | 无（立刻可开） |
| **A2 / M1** | Rust 版 `LoginGate`/`SelGate`/`LoginSrv`/`DBSrv`（用冻结的 protocol + storage trait + sqlx） | §6 M1 五条（混跑两链、认证正反例、跨实现互读、失败要响、200 假人） | 协议已在 master 冻结；不等 C2 |
| **B2 / M4 续** | 实现 93 个派发目标语义，收敛"未实现命令清单"到 0，跑通六类关键 NPC | §6 M4 四条（口径已修订） | 批次 2 已合并 |
| **D2** | ① 为 M3 准备"同库同脚本对拍"夹具（用 C 线 DB 快照 + 假人）；② RNG 同种子注入方案设计（解决 D-3 的 M3 缺口） | §6 M3（掉落/经验逐项相同）的准备项 | 无 |

**并行前提照 §12.4**：接口只有 A 线能改（M1 期间 `crates/protocol`、`crates/storage` 的改动必须通知 B2/D2）；一线一判据；一模块一 owner。

---

## 14. walgit 协作流程与每批收尾清单

本项目已挂到本机 walgit（`http://127.0.0.1:8081/gqf2008/OpenMir2.git`，`walgit remote` 名 = `walgit`）。
**代码主源仍是 GitHub（`origin`）**，walgit 承载 D1 协作层（issue / status / patch / review / merge_result）。

### 14.1 每条线开工前（照做即可）

```bash
git -C <checkout> remote add walgit http://127.0.0.1:8081/gqf2008/OpenMir2.git   # 没有才加
walgit collab ls                                   # 看有哪些线程
walgit collab board                               # 看看板（列定义 .walgit/board.toml）
walgit collab thread <你要接的 thread-id>          # 读卡片全文（含判据与收尾要求）
# 抢卡 = 追加一条签名 status（用你自己的 principal/key，不要共用）
walgit collab entry --repo . --kind status --id <thread-id> --actor <你的 principal> \
  --parent <该线程最后一条 entry 的 oid> \
  --body '{"status":"in-progress","owner":"<你的 principal>","worktree":"<worktree 名>","branch":"worktree-<worktree 名>","work":"<一句话>"}' \
  --key ~/.walgit/keys/<你的 principal>.ed25519 --push walgit
```

> 当前 principal 池（coordinator 已注册，**一人一把钥匙，禁止共用**）：
> `openmir2-coordinator`、`openmir2-worker-1..4`、`openmir2-reviewer-1..2`，钥匙在 `~/.walgit/keys/<principal>.ed25519`。

### 14.2 一条线的标准流转

`issue` → `status: in-progress` → 在 worktree 里干活 → `patch`（`--base/--head`）→ `status: needs-review`
→ **另一 principal** 出 `review`（作者自审不算证据）→ coordinator 合并并推 `origin`+`walgit`
→ `merge_result {"merged":true,"oid":…}` → `status: closed`。

### 14.3 每批收尾清单（Cleanup DoD，缺一不算完）

1. **worktree**：`git worktree remove <路径>`（脏树先确认内容已提交/已登记），`git worktree list` 只剩主检出 + 仍在进行中的线；
2. **分支**：已合并的分支删除本地与远端（`git push origin --delete worktree-<名>`），未合并的必须写明原因与归宿；
3. **看板**：线程收口为 `status: closed`，且 `walgit collab board` 里该卡片已在【已完成】列——**只写代码不收卡片 = 没收尾**；
4. **进程**：本线起的客户端/服务端/假人/代理进程全部结束（`Get-Process | Where Path -like 'D:\MirClient-run\*'` 等要为空），`E:\MirServer` 十端口状态与开工前一致；
5. **临时物**：本线产生的临时目录、临时库、抓包中间件、日志全部清掉或登记到报告（写清路径 + 为什么保留）；`E:\tmp` 下不留本线专属残留；
6. **冻结基线**：`git status` 干净；`git diff --stat <基线> -- src/ sql/` 为空（例外只有登记过的 `T-*` 条目）；
7. **报告**：patch 里带证据四件套（命令 + 输出 + 路径 + hash），并把"残留物清单"写在报告最后一段。

### 14.4 协调者（coordinator）的检查项

- 每次巡检跑 `E:\MirServer\port-status.ps1` + `walgit collab board` + `walgit collab report`；
- 复核 patch 时**不采信自述**，自己重跑门禁；
- 批准口径修订（如 M4 验收①、Envir 文件数）与白名单（`T-*`/`D-*`/`B-*`）；
- 负责 merge 与 push，并在 `merge_result` 里写清 oid 与门禁结果。

### 14.5 本仓库遵循 host 规范（`http://walgit.localhost:8081/SKILL.md`，2026-10-11 起）

> 取该文件：`curl.exe -sS --resolve walgit.localhost:8081:127.0.0.1 http://walgit.localhost:8081/SKILL.md -o SKILL.md`
> （本机 `walgit.localhost` 不解析；服务端又重定向到这个名字，所以必须用 `--resolve` 绕过，不能改 hosts——我们非管理员。）

**必守四条（照 §0/§0a/§0b/§0d）**：

1. **一个 worktree 一个身份**：每个 worktree 跑一次
   `walgit collab join --repo <worktree> --principal <proj>-<role>-N --key <既有种子> --push origin`，
   身份写在 `<git-dir>/walgit/identity`（不入库、`git clean` 删不掉）。之后该 worktree 里的命令**省略 `--actor`/`--key`**（从 identity 解析）。
   已就位：coordinator（主检出）、worker-1（a2-m1-services）、worker-3（b-script-engine）、worker-4（m2-world）、worker-5（c-tools）、svc-1（s1-oracle-fixes）。
2. **交付要挂 `patch`**：卡片交付时必须补一条 `--kind patch --base refs/heads/main --head refs/heads/<分支>`，再 `status: needs-review`。
3. **审查必须换人**：`--kind review` 由 **`openmir2-reviewer-1/2`** 签（与作者、与合并者都不是同一个人）；作者自审不算数；`svc-*` 不能当批准人（`merge_rule_eval` 会剔除）。
4. **无人值守用 `--once` + 计划任务**（§0c）：Windows 上别用控制台程序做常驻 watcher；`collab watch --once` 交给计划任务，事件落队列、handler 只 park 不干活。

**已知偏差（记录下来，不是忽略）**：

- 种子仍集中放在 `~/.walgit/keys/`，identity 指过去（规范建议直接把种子物化进 `<git-dir>/walgit/keys/`）。
  理由：worktree 会在合并后删除，而本项目的身份要跨 worktree 复用（S/A/B/C/D 各一条线常换 worktree）；
  规范反对的是"一个目录里堆多个 principal 的私钥、agent 之间互相借用"，这一点我们**没有违反**（每条线的 identity 只指向自己那把）。
- 合并仍由协调者（我）做，审查目前也常由我做——**从本批起改为 reviewer-1/2 出 approve，我只做 merge**。

---

## 15. owner 决策记录

| 日期 | 决策 | 落地 |
| --- | --- | --- |
| 2026-10-10 | **C3 小退阶段：按"原工程一致"执行**——不加 GM 权限、不改客户端、不依赖 AutoLogin | 事实校正：`Envir/AutoLogin.txt` 在本工程**没有服务端实现**（全仓无读取点；客户端 `g_sAutoLogin` 常量也未被使用），故不采用。改走**客户端自带退出入口**：面板「小退」按钮（鼠标 PostMessage）或聊天框输入 `小退`，二者都走客户端自己的 `EatItemName('小退') → AppLogoutEx()`（`MirClient/Source/MirClient/ClMain.pas:3978`）。判据：抓包里出现 `CM_SOFTCLOSE(1009)`（或 528/802）+ 回选人界面截图，replay 八阶段 → GREEN |
| 2026-10-10 | **S1：两条 oracle 缺陷修** | 必须记录改前/改后行为、登记 `whitelist T-*`（Rust 侧默认复刻旧行为做对拍，M3 之后按需切新行为）、修后重跑 M0/M1 门禁；缺陷②（`Mir200` 下 Mach-O 部署件）走同批做版本对账。owner = `openmir2-svc-1` |
| 2026-10-10 | **D2：M3 夹具提升为正式门禁** | §6 M3 判据改为 `run_pair.ps1` + `scoped_diff.py`（作用域内字段级 diff = 0）；要求 C# 侧基线一条命令可复跑（含快照 hash 与 RNG 种子注入） |
| 2026-10-11 | **M3 判据去噪（裁决 A）：`scoped_diff.py diff` 加易变列规范化** | §6 M3 门禁判据改为 `diff --canonical`（逐列白名单 + 每列理由、未登记表 fail-closed、含「改坏必红」自检）；`characters.Hair/Sex/Job` 的行为覆盖迁至卡 `openmir2-m3-botsrv-fixed-newchr`（B，另开）。同条更正首验真实前置 = M2 Rust GameGate 合入 master（非 M1）。证据：`mir2-rs/tests/parity/m3/M3首验报告-20261011.md` §2.5 |

---

## 16. S1：oracle 两条缺陷的处置（2026-10-10，owner = `openmir2-svc-1`）

> 全文与证据：`mir2-rs/tests/parity/evidence/S1/S1-报告.md`；白名单条目：`whitelist.md` **T-2 / T-3**。

**缺陷①（已修）游戏网关重连后进世界 NRE 死循环、客户端永久黑屏。**
根因：GameSvr 的网关槽位按 **TCP accept 序**绑定（`int.Parse(SocketId)-1`），而世界侧按网关自报的
`GateIdx`（= GameGate 的 `ServiceId`）寻址；网关重连换了 SocketId ⇒ 两者错位，且 `CloseGate`
只把槽位里的 `UserList/Socket` 置 null、**不释放槽位** ⇒ 世界下发打到死槽位（静默丢弃 ⇒ 黑屏）、
`SetGateUserList` 每 200ms 抛 NRE（`NewHumanList` 清不掉 ⇒ 玩家进不去，且**此后每次登录都失败**，
直到重启 GameSvr；C 线的抓包脚本因此每次整栈重起）。
修法：`TCPNetChannel` 改为「连接 → 槽位」映射 + 断开释放槽位 + 重连按最小空闲槽重分配，
世界侧入口（`SetGateUserList`/`AddGateBuffer`/`CloseUser`）加越界与空槽守卫。
对照：确定性探针 `1 GREEN/3 RED → 4 GREEN/0 RED`；实机门禁（真实客户端 + 只重启 GameGate）
改前 `画面非黑 0% + NRE 0→156`、改后全绿。**Rust 侧默认复刻旧行为**（T-2）。

**缺陷②（已处置）部署件与源码版本漂移。**
`E:\MirServer` 是从一份 **macOS 发布包**铺出来的：各目录里无扩展名的同名文件是 osx-x64 的 apphost
（Mach-O），Windows 上真正跑的是 `*.exe`；`Mir200` 恰好**只有** Mach-O apphost 且是 .NET 6 旧血统
（程序集名 `GameSvr.dll`、依赖已不存在的 `SystemModule.Sockets.AsyncSocketServer`、调试路径
`/Volumes/Project/...`）⇒ 不是部署件，只是内容副本。同时 `M2GameSvr` 被"只刷一部分"地更新过
（自有程序集分属 2026-10-09 与 10-10 两批）⇒ 判 `MIXED`。
处置：新增 `tools/oracle/reconcile_deployment.ps1`（对账 + `-SelfTestRed`）与 `deploy_gamesvr.ps1`
（同构建整体铺 + 写 `!deployed-from.json` 来源登记），`M2GameSvr` 刷新为 **SINGLE-BUILD**；
`Mir200` 等 5 个旧目录不删，只把血统写进 §2 / §2.1（T-3）。
**注意**：.NET 把源码路径编进 MVID ⇒ "与源码逐字节一致"只对**产出它的那个检出**成立，
对账前先读 `!deployed-from.json`。

---

## 17. 里程碑状态（协调者维护）

| 里程碑 | 状态 | 证据（协调者复跑，非自述） |
| --- | --- | --- |
| **M0 协议层** | ✅ **GREEN（2026-10-10）** | `cargo run -p replay -- mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl`：382 帧 → ① 逐字节 0 差异、② 字段 0 差异、unknown idents = 0、**八阶段全覆盖**（登录/选服/选角/建角/进世界/移动/攻击/小退）；红检三连（包号 +1 / 截断 1 字节 / 密钥错 1 位）全部 RED exit 1 ⇒ 判据可证伪。旧 16 帧金标准 ①② 仍 0（无回归）；oracle 向量 1072 条仍 GREEN |
| **M1 边界服务** | 🔄 **实现部分收口（5100 面）；⑤ 与 S4b 待补** | Rust `LoginGate`/`SelGate`/`LoginSrv`（merge `e7949ced`）与 `DBSrv`（merge `4e395fd4`）两块均经 **reviewer-1 approve** 合入 master；卡 `openmir2-a2-m1-services` 已 `closed`，合并后四门禁全绿。**缺口①（owner 2026-10-11 更正）**：M1⑤「200 假人同登+选角」**至今无证据**——A2/M1 收口时记的「转 C6 tier-200 覆盖」**不成立**：C6 的 gate 起的是 `E:/MirServer/start-all.ps1`（**C# 部署栈**，`tools/e2e/stack_e2e.ps1:51` / `tools/botload/load_gate.ps1:96`）⇒ 它的 tier-200 测的是 C# 服务，**没有一个字节经过 Rust 边界服务**（更正见 A2/M1 卡 comment `8501b153`）⇒ 已开卡 **`openmir2-m1-boundary-load-200`**（全 Rust 边界链 + 200 假人 + 红检；同时是 M5 灰度第一步），配 `m1load` 车道排队。**缺口②（owner 2026-10-11 记）**：`DBSrv` 只落了 **5100 面**（`bins/db-srv/src/main.rs` 头部自登记），`DataService`(6000)/`MarketService`(5700)/`TimedService` 缓存回写**未做**（`mir2-rs/docs/M1-边界服务移植计划.md` §5 的 **S4b**；此前只登记在已 closed 卡的 patch body `e01462d8`/`058066bd` 里，**看板无卡**）⇒ 已开卡 **`openmir2-m1-dbsrv-6000-face`**（含前置 S4a：`crates/storage` 记录字段 + `crates/protocol` 向量），配 `dbsrv6k` 车道排队 —— **M5 灰度**在 DBSrv 这一步要它 |
| M2 世界引擎 | 🔄 进行中（真机验收已跑到"进图"，卡在出图/黑屏；**两条真机前置 + 会话生命周期②③ 已落地**） | Rust `GameGate`(7200，字节级 oracle 回归；3 个独立 approve `2d04e0cf`/`764ead14`/`7a103213`) 与 `GameSvr` 世界宿主（S1→S2b-4：网关链路+`GM_*`、认证链、IDS、取档控制流、DB 请求/应答、进世界、**16 条 `SM_*` 入图帧**编码与生产路径；approve `0d0da1ed`/`ac17e73c`）；`crates/envir` 地图加载器 `2a2e8d03` 与角色记录全字段 `353a28e2` 已集成进 `bins/game-svr`（替占位图 + `CurMap`→`map_id`）。**真机首跑**（approve reviewer-2 `bce5d2c2`）：①**GREEN**（5000/7000/7100/7200 全 Rust）；当场收口两条真缺陷——网关线号恒 0（**黑屏路由根因**，照 oracle `AllocGateSlot` 改按连接唯一分配+断线释放）与 `portclassify.ps1` 误判 Rust 入口。**M2 真机前置① `WorldMaps` 可选化已落地**（owner 裁决 `272582e1`「Rust 改可选、**不改现网配置**」：真 `Server.conf` 无此键也能起；无 Envir 且无 WorldMaps 仍 fail-closed）。**顺序判据②**：658 `SM_SENDNOTICE` 接入生产路径（approve reviewer-1 `4f5005f3`；金标准 #1 尾解出 320B 公告 = `Notice.txt` 逐字节）+ 其三条 findings（空公告仍发 header-only 658 / `/r/n/r/n ` 字面量 1:1 / `NoticeDir` 手册坑）与 W-1 flaky 修复随整头 `f37ca6fa` 入库（approve reviewer-3 `b6f8b581`）。**前置② 会话生命周期 ②③ 已落地**（断线清世界幂等 + 重复进世界按 oracle **踢旧(`BoKickFlag`)+关新(`CloseUser`)**，approve reviewer-3 `67932e63`；**DoD ① 夹具已交（`dd6ca066`，待审）**——**① 实测更正（2026-10-11）**：旧连接收的是 **`SM_SYSMESSAGE`**（账号级 kick 文案，**非 528**）且不立即断开；新连接「放行 vs 被 `CloseUser` 关」**由 `ProcessHumansIsLogined`（`InSaveRcdList`/`PlayObjectList`）的竞态决定** ⇒ 原『踢旧+关新、两边都不放行』**只覆盖命中支**。**按实测对齐 Rust 列为本 lane 的独立片**（含旧会话 kick 语义与竞态复刻；`brief-m2svr-next.md` §8）。**验收②③ 仍 RED 待清**：生产路径当前 `{658,51}`/16 ⇒ **下一片 = 补帧**（`50/41/10/213` 等 `m3bag` 装备层接缝：`GetFeatureToLong()` = `PlayObject.cs:3395` → `M2Share.MakeHumanFeature`、`GetFeatureEx()` = `CharacterObject.cs:146`；`766/54/46`+配置层等 `m2env`；`52/752` 等 world 触发点）；④ 依赖②未采。**里程碑级依赖**：②③ 的完整达成依赖 **M3 ① 装备层**。每笔合并的 master oid/证据见 `handover-to-pi.md` 对应轮次。 |
| M3 系统对齐 | 🔄 **首验已达成并收卡（`b218eb15`）→ 转入逐系统对齐** | Rust 顶住 7000/7100/7200：`run_m3_gate.ps1 -Side rust` 在 `--fixed-newchr` 与默认两场景 **GREEN、跨侧作用域内字段级 diff=0**；C# 基线（含 RNG `-VerifyRepeat`）GREEN，`prefix_sha256` 与 D 线 golden 逐位相同。**两个独立审查者均 approve**：reviewer-3 `4bffcf99`（自跑四门禁 351 passed）、reviewer-4 `55116105`（自己重跑固定场景 C# 基线 + 自己的 Rust 会话，并按 baseline.json 核对 peer 场景一致）。合并顺序按审查要求：先合 GB2312 修复（`677678c2`）再合首验片（`b218eb15`），并**丢弃 m3 分支的重复 hunk**（master 只留一份配置解码实现）。为跑通修掉两处前置缺陷：Rust game-gate 读现网 GBK 配置、`run_m3_gate.ps1` 第 3 步跨侧比对路径拼接。剩余 findings 转卡 `openmir2-m3-gate-script-hardening`（peer 场景机判 / `$sessionDir` 空值 fail-open / 入口护栏强化）。后续按 §6 逐系统推进。**逐系统对齐①（背包/装备）** 与 **门禁脚本加固** 已各开卡并配车道（`openmir2-m3-bag-equip-alignment` / `openmir2-m3-gate-script-hardening`，受 cap=3 排队等空槽） |
| M4 脚本引擎 | 🔄 派发面已清零；剩对拍/用例面（**无在跑车道**，按 §6 顺序让位 M2/M3） | **判据①口径已修订**（2026-10-10）："解析错误 = 0" 不可达（同一语料下 C# 参照自身也产 427 条）⇒ 改为"**双侧逐行多重集差 = 0**"。**派发目标覆盖已清零**：`crates/script-tool` 覆盖率台账 23→106（11829 次调用）**已实现、未实现 0**（`tests/parity/evidence/B2/coverage-var-after.txt`）；`未注册（C# 静默忽略）130 次` 与 C# 同口径，不算缺口。批次已合入 master `971b354e`，卡 `openmir2-b2-script-dispatch` 已收卡。**剩余**：② 语句/条件/动作/宏展开的 parsecount 两侧对拍数表；③ 6 类关键 NPC 用例（商店/传送/修理/仓库/行会/任务）；④ "双侧逐行多重集差 = 0" 门禁落地并入库。 |
| M5 收敛/压测/灰度 | ⏳ 未开始（门禁部分已收口；其余四项无卡） | **C6 载荷门禁两卡已 closed**（内容合入 master `6f2856da`；reviewer-3 `03238f5c` / `d3583dc7` 均 approve）：口径 = 登录成功率（tier-1000 = **同命令 3 连跑取最差 ≥ 95%**）+ 稳态掉线 0 + `tick P99 ≤ 100ms`，**verdict 表分列**（login / conn / tick / samples / in_world / **本机负载**）；**残余丢包方向已证 = 服务端未回**（`132401` / `162213` 两轮 rx0 + 重发 0 成功；触发条件待**空载复测**归属）；牙齿红检保留；`F9-1`（Minor：`artifacts.sha256.json` 按工作树 CRLF 算 ⇒ 全新检出假 mismatch）**待修**。**P99 口径**：C# 参考栈 1000 档实测 125/201ms——归属**未定**（同机负载 vs 上游，待空载复测）；Rust world 合成档基线 576/1195/2301µs（`tests/parity/world/p99-baseline-tickbench.txt`；绝对量按裁决 `5e1daee` 只作报告值）⇒ **§6 本条「压测三档（1000 假人在线）下 P99 ≤ 100ms」在任何栈上都尚未验证**；`in_world` 是覆盖基数短板（`tests/loadgate/README.md:127`：仅 ~10% 假人真进世界）⇒ 现有三档只能算"登录/选角压力"，**不算**"1000 假人在线"（其根因 F4 已转卡 `openmir2-botsrv-world-evidence`）。**其余四项验收（72h 长跑 / `mem-probe` 内存斜率 / 灰度-回滚演练 / 灰度前后 DB 快照）尚无卡**，按 §6.0-5 待 M2–M4 收口后开 |

---

### 17.1 待补的验收项与"测量窗口"要求（owner 2026-10-11 记，防止被当成已完成）

- **M3 总验收未做**（§6 M3 原文）：① 十项系统逐一对齐**全绿** + M3 门禁 `diff=0`；② **300 假人长跑 24h 无 panic、无掉线**；
  ③ 每系统的"命令 + 输出 + 差异报告"汇总成 `tests/parity/M3-report.md`。
  现状：只到"逐系统对齐 ①（背包/装备）"；②③ **无卡**（按 §6.0-5 待对齐推进后再开，**别记成已完成**）。
- **M5 另有四项无卡**：72h 长跑 / `mem-probe` 内存斜率 / 灰度-回滚演练 / 灰度前后 DB 快照（见 M5 行）。
- **P99 类判据需要"静默窗口"**（两条独立事实，别再各自解释）：
  1. tick_bench 的相对门禁带 `SKIP-UNDER-LOAD`（CPU≥40% 或 `rustc`/`cargo`/`cargo-clippy`/`msbuild`≥4 ⇒ SKIP、exit 0）。
     本机常态是多车道在跑（实测 CPU 常在 50–80%）⇒ 该门禁**持续 SKIP**（`last-tickbench.md` 5/5）⇒ **相对 P99 判据实际处于休眠**，
     目前只有单测 `regression_limits_are_exact` 在钉阈值。
  2. C6 的 load gate **没有本机负载字段**（`tools/botload/load_gate.ps1:231-232` 只采被测进程）⇒ tier-1000 的 P99 越界**无法归属**
     （见卡 `openmir2-c6-loadgate-idempotent` 的 owner comment `82f8e0c3`）。
  ⇒ **任何要写进验收的 P99 数字，必须在没有其它车道编译/压测的窗口里采**；采集前由 owner 收拢窗口并登记
  （`lane-status.log` + 进程实况 + 本机负载探针，三点齐才算窗口干净）。
- **M1⑤ 的"假覆盖"已更正**（见 M1 行 + 卡 `openmir2-m1-boundary-load-200`）：**判据要落到"哪个进程收的字节"**，
  别把"别处的成绩"当成自己的证据（C6 tier-200 是 C# 栈的成绩，没覆盖 Rust 边界服务）。
- **门禁必须"同一命令重复得同一结论"（flaky = Important）**：2026-10-11 reviewer-1 在 WorldMaps 片上抓到一条**实测 flaky**——两条新用例共用
  `%TEMP%/gs-mapwire-nowm-<pid>`（同 test binary、同 pid ⇒ 并行时 `remove_dir_all` 与 `std::fs::write` 相撞），`cargo test --workspace` **7 次里红 2 次**。
  ⇒ 规则：① **新增/改动的测试，临时目录必须唯一到"用例级"**（`pid + 原子计数` 或 `tempfile`；**只用 pid 不够**——已有教训
  `~/.agents/rules/LESSON_并行测试临时路径须pid加原子计数构造_时钟粒度不足.md`）；② **交付含新测试的，作者自己给 ≥3 次连续 `cargo test --workspace --offline` 全绿**
  （reviewer 可要求到 5 次）；③ reviewer 抓到 flaky 一律按 **Important** 处理（它直接破坏"门禁即合同"）。

### 17.2 本轮 owner 更新（2026-10-11 18:45，合并后复核）

- **C6/M5 假人侧证据**：卡 `openmir2-botsrv-world-evidence` 已由 owner 按被审 commit `a236e607` 合入 master
  （merge `7a878095`，approve reviewer-4 `627575c0`）并收卡 —— F3(a)（第三条假人无应答 = 打乱账号/角色共享判据）与
  F4（`RunAutoPlay` 作用在未进世界假人）收口；判据锚在"进世界证据只认该假人自己那条连接"（改后 3/3 `in_world` 且
  `autoplay_pre_world=0`，改前 legacy 对照 30；牙齿 `SUPPRESS_INWORLD=2` ⇒ `-RequireWorld` 必 RED）。合并树四门禁 67 suites / 421 passed / 0 failed / 604。
- **M2 会话生命周期**：DoD① 真机夹具片（`e0ecd25c`，approve reviewer-4 `5e19c9df`）已合入 master（merge `55fdde5e`）。
  **owner 裁决（O-2）**：①实测显示旧会话收到的是**账号级 kick 的 `SM_SYSMESSAGE`（非 528）**且不立即断开，新连接"放行 vs 被 `CloseUser` 关"
  由 `WorldServer.ProcessHumansIsLogined`（`InSaveRcdList`/`PlayObjectList`）**竞态**决定 ⇒ 已合入的 ②③ 不等价，**本卡保持 open**，
  下一片 = 按实测对齐 Rust（含把竞态本身复刻），由 m2-svr 车道承接。
- **M2 入图帧**：m2-svr 车道交付"生产路径 **8/16**"（`3dbb4707`，patch `d2648e37`）——`766/54` + `11029/1106/5008/811` 六条接进
  `bins/game-svr/src/enter.rs`（逐条对照 oracle 行号 + 6 条红检），**待独立审查**。剩余 8 条里 `50/41/10/213` 阻塞在
  `m3bag` 的 `feature.rs` 接缝。
- **M3 逐系统对齐①（背包/装备）**：片 `ef94810e` 被 reviewer-4 判 **request_changes**（`cf067560`）：F-1（Important，阻塞）=
  新 `cmd_extract` 对"快照里有未登记表"无条件 `exit 4`，而 dbsnap 固定导出 13 表、`characters_item_attr`/`characters_magic`/`characters_quest`
  未登记 ⇒ `run_pair.ps1` / `check_db_paths.ps1` 在每条真实快照上必 RED（reviewer 已做 base 对照）。作者按 F-1/O-1/O-2 + 改号返工中。
  绿面（C# 真身探针 16+8+6 向量 deep-equal、红检有牙齿、四门禁 3 连跑 430/0/13）经独立复现成立。
- **合并护栏**：两条 merge 都在合并树上重跑四门禁（`E:/tmp/openmir2-watch/merge-55fdde5e-*.log`），并逐条核"被审面未被合并改写"
  （botsrv：`git diff --name-only a236e607 7a878095 -- <被审路径>` = 空；life：`docs/Rust重写设计文档.md` 的 blob 与 master 侧版本逐字节相同）。
  **T-12 编号已被 botsrv 占用** ⇒ m3bag 需改号（已写入其 brief）。
- **C6 残留 F9-1 已开卡**：`openmir2-c6-artifacts-hash-repro`（issue `b790d24ae7413dab3e461e4fc75ab5303390026b`）——`artifacts.sha256.json` 的
  CRLF/LF 可复现性（全新检出 18/45 假 mismatch）；Minor、不是里程碑阻塞项，车道排在 `m3gs` 之后，**避免只留在散文里**。
- **M2 入图帧 8/16 已合入**（2026-10-11 18:30）：`3dbb4707`（`enter.rs`：`766/54/11029/1106/5008/811`）经 reviewer-1 `16cb2cbf`
  **独立 approve**（隔离 target 3 连跑 429/0/13 + 独立红检 3 条 + 六条帧逐条 oracle 行号锚定）后由 owner 合入 master `0106e9f4`（合并树四门禁 67/429/0/13 + 604）。
  余 8 条：`50/41/10/213`（等 `m3bag` 的 `feature.rs` 接缝，该片因 F-1 返工中）、`42 SM_USERNAME`、`52/752`、`46`（世界时钟）。
- **M2 会话生命周期"按实测对齐"片已交付待审**：`3b2e71f2` @ head `b4700ce5`（账号级 `SM_SYSMESSAGE` 顶号 + `ProcessHumansIsLogined` 竞态三支复刻，
  `payMode≠0` 因 oracle 恒传 0 而只实现一支——如实登记）；已由 owner 路由给 reviewer 车道（审 `3dbb4707..3b2e71f2` 差量）。
- **M2 验收夹具已"先验一步"**（2026-10-11 18:37）：`powershell -File tools/m2-acceptance/selftest.ps1` ⇒ **33/33 GREEN**（`SUMMARY total=33 failed=0`，
  报告 `E:/tmp/m2-acceptance/selftest-20261011-183718/selftest.json`）——手册每一步"判红/判绿"的口径都经构造输入验证（含 `PREREQ_MISSING=2` 不静默通过、
  有 RED 不许被说成前置、金标准 schema 坏必红）。⇒ 端口窗口一空就能直接跑真机验收，**不会因为夹具本身坏了白占窗口**。
- **M3 背包/装备片的 F-1 返工已交付待审**：head `156ddba1`（base `85cac081`）——13 张真实 dbsnap 表全部登记（真快照 extract `RC=4→0`，115 行/111 族表行）、
  selftest **13/13**（真实 13 表 manifest extract 必须 GREEN + 删任一登记必红）、族表跨侧 `--canonical --fixed-newchr` diff GREEN（5 表/111 行×2）、
  新增 `oracle_source_sha256` 源码锚、`T-12/T-13`→`T-13/T-14`。**M2 帧 `50/41/10/213` 要的 `feature.rs` 就在这一片**（它不碰 `main.rs`/`smframe.rs`）。
- **M4 剩余三项已开卡**：`openmir2-m4-parity-cases`（issue `03d6c58a63d889dfadc11e378bd0ea3460fc55c5`）—— parsecount 两侧对拍表 / 6 类关键 NPC 用例 /
  "双侧逐行多重集差 = 0" 门禁落地；车道排在 `m3gs` 之后（先有卡，别只留在 §17 散文里）。
- **M2 会话生命周期对齐片已合入**（2026-10-11 18:50）：`3b2e71f2` 经 reviewer-3 `5224ada8` 独立 approve（隔离 target 3 连跑每次 435/0/13；**3 条红检独立复现**；
  oracle 逐行核对 `TCPNetChannel.cs:229/252/256/262/268-269`、`AuthenticationService.cs:299-319`、`PlayObject.Message.cs:1715`、`PlayObject.cs:1082-1090`、`WorldServer.cs:200-217/503/535/568-570`；
  字节锚 = **M0 金标准 seq66（ident 100）逐字节重建** + `KICK_USER_MSG` 49B == GBK(oracle 字面量)）⇒ merge `4f77b897`，合并树四门禁 67/435/0/13 + 604。
  **本卡仍不 close**：真机夹具复跑待端口窗口、`InSaveRcdList` 生产侧未接线（S2b-3，真机"保存中"窗口恒空 ⇒ 竞态窗口比 oracle 短，登记不臆造延时）、4 条非阻塞 Minor。
- **M1⑤ 实测抓到一条真缺陷（新卡）**：全 Rust 边界链上 **登录/选服/查角色 200/200，但选角 0/200**（无 `525`，Rust db-srv 200 条 `Double send SELCHR`）——
  根因 = `bins/db-srv` 的 `query_chr()` 成功时**没置 `u.chr_queryed = true`**（C# `src/DBSrv/Services/Impl/UserService.cs:359` 有），`CM_SELCHR` 恒走 else ⇒ 从不回 `SM_STARTPLAY`。
  已开卡 `openmir2-m1-dbsrv-selchr-queryed`（issue `2e5d3b0897576dcc4a31f26194dc3fa366805a63`），**不顺手改**（写集红线）；因与 `dbsrv6k` 的 S4b 同文件 ⇒ 由同一车道在该片之后接着做（已写进其 brief §4）。
- **M2 真机第二轮：进图不再黑屏**（2026-10-11 19:19，`m2svr` 交付 head `2201b121` / patch `82b587ce` / status `df90c8b0`，**待审**）：
  ① 帧 ③ 组接线（`100cf642`）——`50 SM_LOGON` / `41 SM_FEATURECHANGED` / `213 SM_ATTACKMODE` **同组**接入（生产路径 **10/16 → 13/16**，逐行照 `PlayObject.SendLogon()`/`PlayObject.cs:1128-1147`；
  如实登记 `Dress/Weapon` 的 `StdItem.Shape` 需物品形状表（未开卡 ⇒ 带装备槽记 0 + WRN）、`CharStatus` 的 buff 未移植 ⇒ 会话初生恒 0；红检 = 对调 `human_feature` 的 dress/weapon ⇒ 用例 FAILED）。
  ② 真机第二轮：**① 端口全 Rust GREEN；② 进图不黑屏**（比奇省 291,666 真图、`进世界帧下发：共 13 条`、小退再进不黑屏、`flow.log` 以 `FLOW_OK`+`CLIENT_STOPPED` 收尾），截图**如实 4/7**（talk/see-others/pickup 未采）；
  **③ s2c diff / ④ 三路径 DB diff 未跑并如实报 PREREQ**（不伪造 GREEN）。报告 `docs/M2验收真机报告-20261011-2.md`、证据 `tools/m2-acceptance/evidence/real-run-20261011-2/`。
  **新 finding（下一片第 1 条）**：世界 tick 之后的出帧带 `SessionIndex=1 socket=0` ⇒「用户不存在」被丢——大概率是"看见别人/后续出包"上不来的根因（判据 = 进程级 + 双客户端互相看见）。
- **M1⑤ 的缺陷已修（待审）**：`dbsrv6k` 交 `openmir2-m1-dbsrv-selchr-queryed` 的修复（`c3fa48a2 → c95e14c8`）：`CM_QUERYCHR` 成功即置 `u.chr_queryed = true`（对齐 `UserService.cs:356-361`）+ 进程级 `QUERYCHR→SELCHR` 断言回 `SM_STARTPLAY(525)` 且体 `127.0.0.1/7200`；红检撤回该行 ⇒ FAILED。
  同批还交了 **S4b 的第 1 片（S4a）**：`master f5170a8e → c3fa48a2`（`crates/storage` 记录字段全量 + C# oracle 向量，重生成 LF 归一化后**逐字节一致** `4e76d755…`；共享向量文件改成 `protocol`/`replay` 可消费，未知 kind 仍报错）。
  **S4b 的 ② 6000 `DataService` 面 / ③ 跨实现互读 / ④ 5700·`TimedService` 仍未做**（卡上登记；②③ 需完整 `PlayDataStorage`/`CacheStorage` 移植 + 真 C# 对端，③ 等 M2 窗口）。
- **同批三片合入 master `bea34684`**（2026-10-11 22:00，均经 reviewer-2 独立 approve）：
  ① **装备⇒能力接缝** `090add78`（`bins/game-svr/src/ability.rs`：`StdItemCatalog`/`equipped_feature`/`recalc_abilitys`（`WAbil` 22 + `SubAbility` 7 + `AddAbil`）/`SubAbility::to_frame_bytes`）；
  ② **装备/卸下 handler 核心** `7b139963`（`bins/game-svr/src/bag.rs`：`check_user_items`/`check_take_on_items`/`take_on`/`take_off` + oracle 真身探针 10 模板/21 用例）；⇒ **`52/752/10` 的输入接缝 + 装备族 handler 已具备**，m2-svr 下一片接线。
  ③ **B-1 修复** `d5a44341`（真 `Server.conf` 的 UTF-8 BOM 只在文件首剥一个 `U+FEFF`；真机第三轮 `stditems：1000 条`，修前 WRN+0 条）。
  合并树门禁 68 suites / 471 passed / 0 failed / 13 ignored + 604。
- **⚠ 合并事故（已修复，方法论级）**：三片合并时 `cargo metadata` 报 **`duplicate key: mir2-data`**——m2-svr 与 m3bag **各自**往 `bins/game-svr/Cargo.toml` 加了同一条依赖（两片各自绿、合起来红）
  ⇒ owner 在合并提交里去掉重复行（保留一行、合并注释）；随后把"`items.rs` 的查表/feature 收敛到 `ability`"写进 m2-svr 的 brief §18。**"合并树 ≠ 被审树，门禁必须重跑"再次被实测印证。**
- **编排运行时修复**：巡检重拉用的常驻 watcher 两度静默死亡（管道 `Bad file descriptor` + 从会话起的进程随会话/作业对象被杀）⇒ 改为**计划任务** `OpenMir2-LaneTick`（每 10 分钟跑 `supervise.sh`，`schtasks /run` 可立即触发）；
  六条车道已恢复（m2svr / m1load / m3bag / rev / rev2 / rev3）。经验沉淀 `~/.agents/rules/LESSON_从agent会话起的常驻进程随会话结束被杀_长跑编排用计划任务.md`。
- **下一批的待审**：`m2svr @52d99f0f`（**S3 路由修复**：根因在 game-gate 把 20B 头 `Socket` 写死 0，oracle 是 `_session.SckHandle`）/ `m1load @e587760d`（F-1/F-2 返工；F-3 选角复跑现在可做）/ `m3bag @f6650f05`（第三格 `CM_EAT`）。
- **M1⑤ 已收口（2026-10-11 23:50，卡 `openmir2-m1-boundary-load-200` 已 `closed`）**：全 Rust 边界链上 **登录 200/200 + 选角 200/200**（`selchr` 修复后复跑；`05-probe` `ident_hist` 含 `525:200`、db-srv `start_play=200`、`Double send SELCHR=0`）、无 panic、端口所有权全 Rust、C# 对照齐、红检 GREEN→RED→GREEN、收工端口回落；reviewer-6 `cabcad50` 独立复现（`sha256sum -c` 26 OK、`git check-ignore` 干净、计数件逐字节一致）后由 owner 合入 `cc6518c6`。
  ⇒ **M1 里程碑只差 S4b**：`openmir2-m1-dbsrv-6000-face` 的 ② 6000 `DataService` 面 + ④ 5700/`TimedService`（S4a 已合入；③ 跨实现互读等机窗口）——车道 `dbsrv6k` 已复出做这块。
- **M2 真机 ② 的关键一格已实现**：S3 后半·AOI 同屏下行（`1e81591c`，approve reviewer-5 `8843af44`）⇒ `SM_TURN(10)`+`SM_FEATURECHANGED(41)` 按观察者 `[GateIdx,Socket]` 路由，合入 `db84f98d`（合并树 68/477/0/13 + 604）。⇒ `see-others` 从"上不来"变成"有实现、待真机实拍"；`pickup` 仍等 `m3bag` 第 4 格（P-1 返工中）。
- **M2 真机验收：owner 独立跑出 ① GREEN + ② 4/7（2026-10-12 00:05–00:07，提交 `23920a93`）**：用 m2-svr 车道起好的整栈（Rust 四入口 + C# `DBSrv`/`LoginSrv` + MariaDB），
  `check_ports.ps1` ⇒ 5000/7000/7100/7200 全 Rust 且入口对得上；`mir_flow.ps1 -Account mir2test -Password mir2pass … -WalkClicks 3 -AutoPlaySec 20` ⇒ `FLOW_OK`，
  **进图不黑屏**（比奇省 291,666 + 城墙壁垒 + 完整 HUD，owner 目视核过）、走路、自动挂机攻击、**小退再进不黑屏**（画面里含 658 公告原文）；
  证据入库 `tools/m2-acceptance/evidence/owner-run-20261012/`（含 README 逐项写清覆盖/未覆盖）。余下：`talk`（驱动缺聊天动作）、`see-others`（需第二角色；AOI 下行的真机判据）、`pickup`（等第 4 格）、③（`10` 口径先定死）、④ 三路径 DB。
- **M3① 的 5 个 handler 核心全部合入**（2026-10-12 00:25）：`m3bag` 第四/五格 `CM_PICKUP`/`CM_DROPITEM`（`bins/game-svr/src/ground.rs` + `ground-probe` 真身探针）经 reviewer-6 `e63bba84` approve 合入 `2a2247c5`（合并树 **68/483/0/13** + 604）；**P-1 的修法 = StdMode 40 耐久照 oracle 按 `ushort` 回绕**（不是钳 0）。⇒ 本卡剩下的是**双侧同序列驱动（T-14，要机窗口）**与**接线（归 m2-svr；接好 `pickup` 才能拍 `m2-06-pickup`）**。
- **owner 裁决：③「入图序」判据收窄到 `658→46`**（2026-10-12 00:28，写进 `brief-m2svr-next.md` §22）：金标准 `#16 ident=10` 的 `recog ≠ ActorId` + oracle `PlayObject.Message.cs:1542` 的 `if(baseObject!=this)` ⇒ **`10 SM_TURN` 是「看见另一个 actor」的 AOI 出包、不是入图自帧**；单客户端进世界时本就不该出现。
  ⇒ 判据 = `s2c_diff.py --start-ident 658 --upto-ident 46`（15 条自帧）**同序+等长+只归一化 `recog` 的逐字节 = 0 未登记差异**；`10` **单独由 `see-others` 那格判**（AOI 下行 `db84f98d` 的进程级用例 + 真机截图）。两条都齐才算 ③ 达成。
- **同批三片合入 `078ccb34`**（2026-10-12 01:05，均经独立 approve）：`m3bag` 第六格（`CM_EAT` 的 `boNODRUG` 守卫 + StdMode 2）、**M1 S4b ② 6000 `DataService` 面 + ④ `TimedService` 缓存回写**（`96d03db5`；⇒ **M1 只差 ③ 跨实现互读与 5700 `MarketService`**）、**talk 回声**（`078ccb34`：`chat.rs` + `smframe::sm_hear` + `CM_SAY` 路由 + 驱动 talk 阶段）；
  合并树 69 suites / 502 passed / 0 failed / 14 ignored + 604。作者在 S4b② 里用 live 测试抓到一条真 bug（`save_characters` 多绑参数 ⇒ bind 错位、`WHERE ID` 绑 0 ⇒ **静默 0 行更新却回 `recog=1`**）并修。
- **再两片合入 `959addb9`**（2026-10-12 01:20）：`m3bag` 第七格（`CM_EAT` StdMode 1 食物 `HungerStatus`/`RefMyStatus` 1:1，探针 12 模板/19 用例）、**M2 验收② 真机 `m2-03-talk.png`**（⇒ **② 5/7**：进图/走路/说话/攻击/小退再进；缺 `see-others`（需第二角色，AOI 真机判据）与 `pickup`（需接线 + 地面留物）——如实登记）。合并树 69/502/0/14 + 604。
- **M2 入图帧推进到 10/16 并待审**（2026-10-11 19:00）：新 head `56ac670f`（patch `c234ef9c` / status `7da05a24`）接通 **42 `SM_USERNAME`**（`GetShowName()` 1:1 + `strings.rs` 读 `String.conf [String]` + `[Setup] ShowRankLevelName`，金标准 seq21 线上字节锚定）与
  **46 `SM_DAYCHANGING`**（`Bright = M2Share.GameTime` 按**真实本地小时**算，`chrono::Local`，不是假时钟）；`52/752` 如实登记"需 `RecalcAbilitys()`（装备/buff 层未移植）"，**未假造**。
  作者还带源码证据**反驳了 reviewer-1 的 O-1**：`SendAdjustBonus` 用 2 参 `SendSocket(ClientMsg, "")`，空串**提前 return**（`PlayObject.Socket.cs:111-114`）⇒ oracle 在 `Job∉{0,1,2}` 时**什么都不发**，"跳过"本就 1:1（复审要点已写入 `brief-review.md`）。
  **`50/41/10/213`（第 ③ 组）前置已清**：`feature.rs` 随 `1df92efe` 进 master。
- **M2 真机验收的前置门已点清（owner 实跑，只读、不占端口）**：`tools/m2-acceptance/preflight.ps1 -RequireMysql` ⇒ **GREEN（ok=18 / missing=0 / red=0）**——
  工具（python/dotnet/cargo）、仓库输入（抓包/切帧/`dump_body_layout`/`GoldenExport`/`mir_flow.ps1`/`dbsnap`/`scoped_diff.py`/EDCode 复用/replay）、
  **金标准身份**（`c-line-baseline-c3-20261010.jsonl` sha256 = `f34a6d11…`，与钉住值一致）、真客户端入口（`D:\MirClient-run\run-release.cmd`）、MySQL 3306 可连 —— 全部 OK；
  证据 `E:/tmp/m2-acceptance/preflight-20261011-190522.json`。⇒ **端口窗口一空就能跑真机验收，不会因夹具/前置本身没准备好白占窗口。**

## 18. S3：B-8 命令码位移修复（2026-10-10，oracle 行为变更）

> 全文与证据：`mir2-rs/tests/parity/evidence/S3/S3-报告.md`；白名单条目：`whitelist.md` **T-4**（并给 B-8 行加了注记）。

`src/Modules/ScriptEngine/ScriptParsers.cs` **两处**由 `nCMDCode = code - 1` 改为 `nCMDCode = code`
（条件解析 + 动作解析）。`code` 是 `GetFields()` 的字段序号，而两张执行注册表按**枚举值**建键；
实测两个枚举「字段序号 == 枚举值」**0 处不等** ⇒ 改解析器是唯一有效的落点
（改执行器查表口径与现状**完全等价**；查表 +1 会弄坏本来就是对的特判命令）。
此后每条脚本命令派发到**自己**的处理器，不再是前一个枚举成员的。

**影响面 7640 行**（条件 1401/1433 + 动作 6239/6286；未变的 79 行是特判命令）——这是整条脚本派发面被纠正。
改前/改后：S2 那 120 行回归集由「抛 `IndexOutOfRangeException`」→ **0**；B 线 flow-diff 用例 3
由 `EX IndexOutOfRangeException` → 正常产出脚本效果行（C# 侧开始按脚本意图执行）。
修后 flow-diff 由 1 一致/2 不一致 变 **0/3** —— 那是因为 Rust 侧当时仍按旧位移派发（T-4 初稿的口径）；
协调者已裁定**两侧同批翻转**（见下方裁定），翻转后应回到 **3 一致 / 0 不一致**。

**回归门禁（固化，一条命令）**：
```powershell
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/script-dispatch/run_dispatch_regression.ps1
# 阳性对照（必须判红）：同一命令加 -SelfTestRed
```
语料 `dispatch-regression-120.tsv`（旧位移口径下必抛的 120 行）+ `expected.json`（sha256 + 期望计数）
+ 运行器；判据 = 现行口径 0 行抛 / 旧位移口径仍 120 行抛（语料有牙齿）/ 轴前提（字段序号 == 枚举值）。
细节与"谁负责哪一半"见该目录 README。

**协调者裁定（2026-10-10）：Rust 侧同批翻转**（`crates/script/src/parser.rs` 由 `code-1` 改 `code`），
理由：oracle 已按 owner 决策修正，Rust 侧的目标是"追上并最终替换当前 oracle"；继续复刻旧位移等于
长期维护一个刻意的偏差（还要为它养一套 shift-aware 对账器），收益为负。
配套要求：① whitelist 的 B-8 条目改为"两侧同步翻转（原为复刻）"，保留历史说明；
② 翻转后 B 线重跑 flow-diff，期望回到 **3 一致 / 0 不一致**（S2 报告里那 120 行语料当回归集）；
③ 若翻转后出现新的不一致，按 §6.0 处理（要么修，要么进白名单并给"无害"论证）。
