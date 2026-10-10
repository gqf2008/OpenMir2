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
| 游戏内容（另一份副本） | `E:\MirServer\Mir200\Envir` | 同源副本（递归 634 个 `.txt`），**实现前先确认该读哪一份**，不要两处都改 |

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

- 总验收：**上述十项全绿 + 300 假人长跑 24h 无 panic、无掉线**。
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
