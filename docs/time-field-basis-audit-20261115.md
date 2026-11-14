# 时间/计数类字段的基底与位宽普查（2026-11-15）

> 卡 `openmir2-time-field-basis-width-audit`（P1，tier=L）。本报告**只做普查与对照表**，不改服务端代码；
> 发现偏离按严重度另开修复卡（见 §6）。
>
> 基线：`walgit/master` `b8fdc192c`（2026-11-15）。行号是基线上的，会漂；`mir2-rs/bins/game-svr/**`
> 与 `mir2-rs/crates/world-bevy/**` 属文档锚策略里的**高 churn** 文件，本文按策略写**符号锚 + L 行号**，
> 不写 `` `path:line` `` 锚；低 churn 文件写可机判的 `` `path:line` `` 锚。

## 0. 结论摘要

1. **客户端可直接解析的时间值只有 3 类**：`+GD/<t>` 短包、`SM_SELECTSERVER_OK(530).Recog`、
   `SM_MOVEMESSAGE(99)` 的时长参数；另有 `SM_DAYCHANGING(46).Series`（本地小时桶，非时间戳）。
   其中**客户端可见字段里唯一基底/位宽不一致的是 `+GD`**：Rust 13 位 Unix 毫秒、C# 9–10 位开机毫秒（P1，已有在飞修复卡）。
2. **没有发现第二处「13 位 epoch 被塞进出站帧」**。Rust 侧 epoch→i32 截断有 3 处
   （`player.tick`、`kick_off_line_tick` 的脚本基准、login-srv 的 `tick_as_i32`）：前两处只做同侧
   wrapping 差值、不出帧；**第三处会落库**——`tick_as_i32()` 写进 `account.PassFailTime`（int32 列），
   而 oracle 写的是开机毫秒 `GetTickCount`，基底不同（**P1，§6 D2**，新卡
   `openmir2-account-passfailtime-basis`）。
3. **日志时间戳基底不一致**：Rust 的 game-svr / game-gate / login-gate / sel-gate / login-srv / ids
   六处日志前缀用 **UTC**，C# Serilog 用**本地时间**；只有 db-srv 硬编码 +8。跨栈日志时间线因此差 8 小时
   （P2，§6 D3；有今日归档实据）。
4. 世界时钟（`World::clock_ms`，0 起、+200ms/tick）与 oracle 墙钟的差异属**已登记族**
   （whitelist T-42/T-64/T-66/T-68/T-78/T-85/T-104/T-130）；本报告复核了该族的边界，未新增未登记偏离。
5. 大量「时间消息」在 oracle 侧就是死码：`+FL`、`SM_TIMECHECK_MSG`、`CM_CHECKTIME`、
   `ISM_STANDARDTICK*`、`SS_LOGINCOST`、`ISM_GAMETIMEOFTIMECARDUSER`、账户到期查询（后者是**传递性死码**，见 O20）；
   对照表逐条给出「无发送点 / 无活触发者」证据（§3/§4）。

## 1. 方法（可复跑）

### 1.1 源侧 grep 口径

Rust 与 C# 用同一条符号清单（`mir2-rs` + `src`）：

```sh
grep -rn "now_ms\|clock_ms\|UNIX_EPOCH\|SystemTime\|Instant\|Environment.TickCount\|GetTickCount\|DateTime.Now\|DateTime.UtcNow\|as_secs\|as_millis\|as_micros" \
  mir2-rs src --include=*.rs --include=*.cs
```

基线命中 **3413** 行。按符号分布（同日实测）：

| 符号 | 命中 | 主要含义 |
| --- | --- | --- |
| `GetTickCount` | 1691 | C# 开机毫秒（`Environment.TickCount`） |
| `clock_ms` | 1002 | Rust 世界虚拟时钟（0 起、+200ms/tick） |
| `now_ms` | 306 | Rust 宿主毫秒（game-svr 是 epoch，db-srv 是进程 uptime） |
| `Instant` | 213 | Rust 单调时钟（间隔判定） |
| `DateTime.Now` | 109 | C# 本地墙钟 |
| `SystemTime` | 40 | Rust 墙钟 |
| `UNIX_EPOCH` | 35 | Rust epoch 起点 |
| `as_millis` | 35 | 毫秒换算 |
| `Environment.TickCount` | 23 | C# 直接读开机毫秒 |
| `as_secs` | 11 | 秒换算 |
| `DateTime.UtcNow` | 4 | C# UTC（仅 TOTP/统计） |
| `as_micros` | 2 | 微秒换算 |

再按域收窄：`SendSocket(` / `SendDefMessage(` / `MakeMessage(` / `SendMsg(` / `MakeLong` /
`MakeWord` / `INSERT`/`UPDATE` 的时间列。出站字符串短包的穷举口径是 C# `SendSocket(` 的 122 处调用点
（`src/M2Server`、`src/GameSrv`、`src/Modules`）；其中唯一由时间派生的就是 `M2Share.GetGoodTick`。

### 1.2 帧/表字段清单来源

- 帧号：`src/OpenMir2/Messages.cs`（oracle）与 `mir2-rs/crates/protocol/src/messages.rs`（Rust，由 msgcodegen 对齐）。
- 帧头：`src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs`（`CommandMessage`）与
  `mir2-rs/crates/protocol/src/frame.rs:50`（`CommandMessage`）。线上头恒 12B：`Recog(i32) | Ident(u16) | Param(u16) | Tag(u16) | Series(u16)`。
- 帧体：`smframe`/`smframe_ext`/`downlink`/`run_frames`/`attackwire`/`movewire`（Rust）与对应 C# 发送点。
- 跨进程：`mir2-rs/crates/planes/src`、`src/Modules/PlanesSystem`、`src/GameSrv/Services/AuthenticationService.cs`。
- 表列：`sql/mir2_db.sql`、`sql/mir2_account.sql`、`sql/mir2_data.sql`（冻结 schema）。
- 文件格式：`E:/MirServer/M2GameSvr/Castle/`、`GuildBase/` 的部署件（只读）。

### 1.3 判据模板

每个字段给：**语义 → 基底（开机毫秒 / Unix 毫秒 / 秒 / tick 序号 / 本地日期文本 / 其它）→ 位宽（32/64/16/文本）
→ oracle 对应（文件:行或符号）→ 是否一致 → 证据（代码/实测/归档）**。

### 1.4 运行时抽样环境

- 抽样时端口检查（`Get-NetTCPConnection -State Listen`）：仅 `127.0.0.1:3306`（mysqld）在听；
  5000/5500/6000/7000/7100/7200 **无进程** ⇒ 无其它车道占栈，也**没有起栈**（只用今日归档 + 只读 DB）。
- 归档：`E:/UserFleet/archive/botsrv-probe-turn-20261115a/`（C# 栈 `ab-csharp/`，Rust 栈 `rust200-final/`、
  `rust200-fixed/`、`run3/`；同一 BotSrv 夹具）。
- 由于 `+GD` 的修复车道（`openmir2-gd-ack-boot-millis`，worker-84）在同一时间窗工作，本卡不抢端口、不抢构建。

## 2. 时间源清单

### 2.1 Rust 侧

| # | 源 | 位置（锚） | 基底 | 位宽 | 说明 |
| --- | --- | --- | --- | --- | --- |
| R1 | game-svr 注入时钟 | `mir2-rs/bins/game-svr/src/clock.rs` `now_ms`（L135） | Unix 纪元毫秒；`MIR2_CLOCK_MS` 时 = origin + 单调经过 | i64 | 未注入 = 墙钟，行为逐字节不变 |
| R2 | game-svr 生产入口 | `mir2-rs/bins/game-svr/src/lib.rs` `fn now_ms`（L29884） | 同 R1 | i64 | 帧面/日志/脚本共用 |
| R3 | 世界虚拟时钟 | `mir2-rs/crates/world-bevy/src/world.rs` `clock_ms` / `TICK_INTERVAL_MS`（L990 / L41） | 进程内 0 起、每 tick +200ms | i64 | 模拟时钟；负荷下会落后墙钟 |
| R4 | 共享 epoch 工具 | `mir2-rs/crates/shared/src/hutil32.rs:724` `get_timestamp` | Unix 毫秒 | i64 | 对应 C# `GetTimestamp` |
| R5 | db-srv 间隔时钟 | `mir2-rs/bins/db-srv/src/handler.rs:34` `now_ms` | **进程启动起毫秒**（`Instant`） | u64 | 只做间隔判定，语义同 C# `GetTickCount` 的差值 |
| R6 | game-gate 墙钟 | `mir2-rs/bins/game-gate/src/lib.rs:1720` `now_ms` | Unix 毫秒 | i64 | 速度/空闲门 |
| R7 | login-srv tick | `mir2-rs/bins/login-srv/src/state.rs:20` `tick` | Unix 毫秒截 u32（mod 2^32） | u32 | 差值；值经 R8 落库（D2） |
| R8 | login-srv i32 tick | `mir2-rs/bins/login-srv/src/login_server.rs:39` `tick_as_i32` | 同 R7 as i32 | i32 | 密码错误计数窗口（60s/180s）；值写 `account.PassFailTime`（D2） |
| R9 | game-svr 日志前缀 | `mir2-rs/bins/game-svr/src/lib.rs` `glog`（L132-143） | Unix 秒拆时分秒（**UTC**） | 文本 | 与 C# 本地时间不同（D3） |
| R10 | ids 日志前缀 | `mir2-rs/bins/game-svr/src/ids.rs` `glog_ids`（L27-45） | **UTC** | 文本 | 同上 |
| R11 | 网关/登录服务日志前缀 | `mir2-rs/bins/game-gate/src/lib.rs:119`、`mir2-rs/bins/login-gate/src/lib.rs:48`、`mir2-rs/bins/sel-gate/src/lib.rs:54`、`mir2-rs/bins/login-srv/src/lib.rs:42` | **UTC** | 文本 | 同上 |
| R12 | db-srv 日志前缀 | `mir2-rs/bins/db-srv/src/log.rs:14` `stamp` | Unix 秒 **硬编码 +8h** | 文本 | 本机部署等价本地；换时区/DST 即错 |
| R13 | world-bevy 内部计时 | `mir2-rs/crates/world-bevy/src/aoi.rs` / `buff.rs` 的 `now_ms` 形参 | 世界虚拟时钟（R3） | i64 | AI/搜索/buff 到期；见 §6 D4 |
| R14 | res-svr / mir2-mcp | `mir2-rs/bins/res-svr/src/index.rs`、`mir2-rs/bins/mir2-mcp/src/conf.rs:776` | `SystemTime`/`Instant` | u64/Instant | 无 oracle 对应，不在游戏协议面 |

### 2.2 C# oracle 侧

| # | 源 | 位置（锚） | 基底 | 位宽 | 说明 |
| --- | --- | --- | --- | --- | --- |
| C1 | `HUtil32.GetTickCount` | `src/OpenMir2/HUtil32.cs:36` `GetTickCount` | **开机毫秒**（`Environment.TickCount`） | int32（24.9 天后符号翻转） | `+GD`、几乎所有内部 tick 门 |
| C2 | `HUtil32.GetTimestamp` | `src/OpenMir2/HUtil32.cs:41` | Unix 毫秒 | int64 | 内建机器人 `RobotObject` 的 RunTick |
| C3 | `HUtil32.GetTimestamp(TimeSpan)` | `src/OpenMir2/HUtil32.cs:46` | Unix 毫秒 + 偏移 | int64 | 同上族 |
| C4 | `DateTime.Now` | 多处（`src/M2Server/Npc/NormNpc.cs:357`、`src/Modules/ScriptEngine/ProcessingBase.cs:148`） | 本地墙钟 | DateTime | 脚本日期变量、城堡日期 |
| C5 | `DateTime.UtcNow` | `src/GameGate/Auth/DynamicAuthenticator.cs:115` | UTC | DateTime | 仅 TOTP/统计，不进游戏协议 |
| C6 | `DateTimeOffset.Now` | `src/LoginSrv/Storage/AccountStorage.cs:283-286` | 带偏移的本地时间 | DateTimeOffset | 账户有效期列 |
| C7 | 直接 `Environment.TickCount` | `src/BotSrv/LoadMetrics.cs:151`、`src/BotSrv/Player/RobotPlayer.cs:6951` | 开机毫秒 | int32 | 压测夹具的测量域 |

## 3. 出站字段（客户端可见 / 跨进程）

| # | 字段/帧 | 语义 | Rust 基底/位宽 | oracle 对应 | 一致？ | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| O1 | `+GD/<t>` 短包 | 动作受理 ack 的时间戳 | Unix 毫秒 / i64，十进制 13 位 | 开机毫秒 / int32，9–10 位 | **否（P1）** | Rust：`movewire.rs` `good_tick_payload`（L197）+ `lib.rs` `send_good_tick`（L15897，调用 L15910）；C#：`src/M2Server/M2Share.cs:205` `GetGoodTick` + `src/OpenMir2/HUtil32.cs:36` `GetTickCount`。实测见 §7 S1。修复卡 `openmir2-gd-ack-boot-millis`（在飞） |
| O2 | `+FL/<t>` 短包 | 动作失败 ack | 无实现 | 仅格式常量，无发送点 | 双侧死码 | `src/Modules/SystemModule/MessageSettings.cs:5` `sSTATUS_FAIL`；全仓 `sSTATUS_FAIL` 只有定义，无 `SendSocket` 使用 |
| O3 | `SM_SELECTSERVER_OK(530).Recog` | 付费模式下的剩余游戏秒数 | 秒 / i32（`user.seconds`） | 秒 / int32（`(int)Math.Round(...TotalSeconds,1)`） | 是 | `src/LoginSrv/Services/ClientSession.cs:516` `MakeMessage`；Rust `mir2-rs/bins/login-srv/src/login_server.rs:691-696` |
| O4 | `SM_MOVEMESSAGE(99).Tag` | 居中公告显示时长（脚本 `{}` 第三参） | u16（写在头字段 Tag） | u16（`StrToUInt16` → `MakeMessage` 的 tag 参） | 是 | `src/M2Server/Actor/BaseObject.cs:1630` `SendMsg`；C# 编码 `src/M2Server/Player/PlayObject.Message.cs:1727`；Rust `smframe_ext.rs` `sm_movemessage` + `line_notice.rs` |
| O5 | `SM_DAYCHANGING(46).Series` | 本地小时桶 0/1/2/3 | u8 桶值 | byte 桶值 | 是（相位 ≤1 tick，T-66） | `src/GameSrv/Word/Threads/SystemProcessor.cs:98-134`；Rust `world.rs` `refresh_game_time`（L7111） |
| O6 | `SM_TIMECHECK_MSG(810).Recog` | 校时 tick | 无发送点 | 有常量与 BotSrv 消费，无 GameSrv 发送点 | 双侧死码 | `src/OpenMir2/Messages.cs:393`；`src/BotSrv/Player/RobotPlayer.Operate.cs:335` 只有 `case` |
| O7 | 帧头 `Param/Tag/Series` | 通用 16 位参数 | u16 截断（`CommandMessage::make`） | ushort 截断（`MakeMessage`） | 是 | `mir2-rs/crates/protocol/src/frame.rs:71` `make`；`src/GameSrv/Word/WorldServer.cs:856-857` 的 default 映射 |
| O8 | `SM_WINEXP(44).Recog` | 当前经验总量 | i32 | int（i32） | 是（计数，非时间） | `src/M2Server/Player/PlayObject.Message.cs:1667`；Rust `lib.rs` 的 `write_back_player_abil_exp` 族 |
| O9 | `SM_GOLDCHANGED(653).Recog` | 金币 | i32 | int（i32） | 是（计数） | `src/M2Server/Player/PlayObject.Message.cs:1834`；Rust `gamegold.rs` |
| O10 | 物品 `MakeIndex`（帧/DB） | 物品唯一 ID | 纯计数器 | 计数器 + `GetTickCount()` | 否，已登记 D-1 | `src/M2Server/M2Share.cs:547-569` `GetItemNumber`/`GetItemNumberEx`；whitelist D-1（DB 列已掩码） |
| O11 | 日志行时间前缀 | 跨栈时间线 | 6 个 Rust 二进制 **UTC** | Serilog 本地时间 | **否（P2）** | 实测 §7 S2；C# 模板 `src/BotSrv/AppServer.cs:48` |
| O12 | db-srv 日志前缀 | 同上 | **UTC+8 硬编码** | 本地时间 | 本机等价；换时区错 | `mir2-rs/bins/db-srv/src/log.rs:14` `stamp` |
| O13 | NPC 说词 `<$DATETIME>` | 本地日期时间文本 | `chrono::Local` `%Y-%m-%d %H:%M:%S`，但**只在 merchant 漏斗注入** | `DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss")` | 部分（P3，现网不可达） | Rust：`lib.rs` `run_merchant_goto`（L9074）内 L9146 注入 `wall_clock`，`npc.rs` `base_variable_text`（L1253）展开；C#：`src/M2Server/Npc/NormNpc.cs:357` |
| O14 | NPC 说词 `<$DATE>` | 本地日期 | 未接线 | `DateTime.Now.ToString("yyyy-MM-dd")` | 部署件 0 次 | `src/Modules/ScriptEngine/ProcessingBase.cs:153`；`E:/MirServer/M2GameSvr/Envir` 全树 `<$DATE>` = 0 文件 |
| O15 | `<$MACRUNTIME>` / `<$SERVERRUNTIME>` | 开机天数 / 开服时间 | 未接线 | `GetTickCount()/86400000`；`StartTime` epoch | 部署件 0 次 | `src/Modules/ScriptEngine/ProcessingBase.cs:138-147`；部署 grep 均 0 文件 |
| O16 | `%logontime` / `%logonint` | 登录时间 / 在线分钟 | 未实现（`high_*_info` 恒 `None`） | `LogonTime`（Unix 毫秒）+ `GetTickCount()-LogonTick` | oracle 侧亦不可达 | `src/M2Server/Player/PlayObject.Base.cs:3127-3128`；`HighLevelHuman` 赋值点在 `src/GameSrv/TimedService.cs:102-138` 全部注释 ⇒ 恒 0 ⇒ `????` |
| O17 | 城堡说词 `$CASTLEWARDATE` / `$CASTLECHANGEDATE` | 攻城区间/占领日期文本 | 本地日期文本（`chrono::Local` 注入） | `DateTime` 解析 + `DateTime.Now` | 是（形状偏差已登记） | `mir2-rs/bins/game-svr/src/castle.rs` `change_date_text`（L1427）；C# `src/M2Server/Castle/UserCastle.cs` |
| O18 | `SS_LOGINCOST(30002)` 跨服 | 登录耗时秒 | 无发送点 | 发送方法存在，调用点被注释 | 双侧死码 | `src/GameSrv/Services/AuthenticationService.cs:107-110`；调用点 `src/M2Server/Player/PlayObject.Base.cs:388` 已注释 |
| O19 | `ISM_GAMETIMEOFTIMECARDUSER(112)` 跨服 | 剩余游戏秒 | 无发送点 | 发送方法存在，调用点被注释 | 双侧死码 | `src/GameSrv/Services/AuthenticationService.cs:116-122`；调用点 `src/M2Server/Player/PlayObject.Message.cs:42` 已注释 |
| O20 | `ISM_QUERYPLAYTIME(116)` / `ISM_QUERYACCOUNTEXPIRETIME(115)` / `ISM_CHECKTIMEACCOUNT(116)` | 账户剩余游戏秒 | 无处理 | **有活发送点与活接收分支**：LoginSrv `src/LoginSrv/Services/SessionServer.cs:213`/`:264` 发、GameSrv `src/GameSrv/Services/AuthenticationService.cs:181` 收；但上游触发 `ISM_GAMETIMEOFTIMECARDUSER(112)`/`ISM_QUERYACCOUNTEXPIRETIME(115)` 在 GameSrv 无活发送（`src/M2Server/Player/PlayObject.Base.cs:388`、`src/M2Server/Player/PlayObject.Message.cs:42` 已注释） | **传递性死码 + 潜在风险** | `src/LoginSrv/Services/SessionServer.cs:213`、`src/LoginSrv/Services/SessionServer.cs:264`、`src/GameSrv/Services/AuthenticationService.cs:181`；常量 `src/OpenMir2/Messages.cs:447-448`（注意 116 同时被 `ISM_QUERYPLAYTIME`/`ISM_CHECKTIMEACCOUNT` 使用）。**112 一旦启用，这条秒级跨进程帧即活**，其 `seconds` 是时长（i64 文本），不是时间戳 |

## 4. 入站字段（客户端/跨服 → 服务端）

| # | 字段/帧 | 语义 | Rust 基底/位宽 | oracle 对应 | 一致？ | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| I1 | `CM_LOGINNOTICEOK(1018).Recog` → `ClientTick` | 客户端 tick（版本门用） | i32 → **i16 截断** | `msg.nParam1` = Recog → `(short)` | 是 | C# 映射：`src/GameSrv/Word/WorldServer.cs:856-857`（default：`nParam1=Recog`）；写入 `src/M2Server/Player/PlayObject.cs:1112` `ClientTick`；Rust `inbound.rs` `client_login_notice_ok`（L137） |
| I2 | `CM_CHECKTIME(15999)` | 客户端校时 | 无处理 | 无处理（只有常量） | 双侧死码 | `src/OpenMir2/Messages.cs:230`；Rust `mir2-rs/crates/protocol/src/messages.rs:122` 仅常量 |
| I3 | `ISM_STANDARDTICKREQ(239)` / `ISM_STANDARDTICK(240)` | 跨服 tick 同步 | 无 case | 无 case | 双侧死码 | Rust `mir2-rs/crates/planes/src/message.rs:251` 明确注明「有常量但没有 case」 |
| I4 | `CM_PROTOCOL` 版本日期 | 客户端版本（日期当整数） | 2001_1006 / i32 | 20011006 / int | 是 | `src/LoginSrv/LsShare.cs:12` `VersionDate`；Rust `mir2-rs/bins/login-srv/src/state.rs:28` |
| I5 | 角色选择/登录记录里的时间列 | `LASTUPDATE` 等 | 读入但不作为时间语义使用 | 同 | 是 | 见 §5 P1-P3 |
| I6 | DB 协议（GameSvr↔DBSrv） | 存档/取档 | 无时间字段 | 无时间字段 | 是（普查未见） | 消息体常量与 `playdata.rs` 字段清单 |
| I7 | 网关↔引擎内部队列 | 会话/心跳/发送 | 无时间负载 | `SendMessage.DeliveryTime` 只在进程内 | 是 | `src/M2Server/Actor/BaseObject.Message.cs:199`（自投递延时，进程内 int32） |

## 5. 跨进程/持久化字段

| # | 字段/表列 | 语义 | Rust 基底/位宽 | oracle 对应 | 一致？ | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| P1 | `characters.CREATEDATE/LASTUPDATE` | 角色创建/最后保存墙钟 | MySQL `now()` → 本地 DATETIME | MySQL `now()` → 本地 DATETIME | 是 | C# `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs:97-99`、`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:88`；Rust `mir2-rs/bins/db-srv/src/db.rs:750`、`mir2-rs/bins/db-srv/src/playdata.rs:426`；实测 §7 S3 |
| P2 | `characters_ablity.ModifyTime` | 属性最后修改 | `now()` | `now()` | 是 | C# `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs:141`；Rust `mir2-rs/bins/db-srv/src/playdata.rs:486` |
| P3 | `characters_indexes.CreateDate/ModifyDate` | 角色索引时间 | `now()` | `now()` | 是 | 建角 Rust `mir2-rs/bins/db-srv/src/db.rs:431`；C# `PlayDataStorage.Add.cs` 建索引处 |
| P4 | `account.CreateTime/ModifyTime/LastLoginTime` | 注册/修改/最后登录 | `UNIX_TIMESTAMP()`（建号与 `update_login_record`）；Unix 秒 | `ToUnixTimeSeconds()`；Unix 秒 | 是（同为绝对秒；时钟源 DB vs 应用） | `sql/mir2_account.sql:36-38`；Rust `mir2-rs/crates/storage/src/account.rs:341`/`:432`；C# `src/LoginSrv/Storage/AccountStorage.cs:289-291`、`:402-407` |
| P5 | `account.PassFailTime` | 密码失败时间 | **`tick_as_i32()`：Unix 毫秒截 u32 再转 i32** | **开机毫秒** `GetTickCount` / int32 | **否（P1）** | `sql/mir2_account.sql:29`；Rust `mir2-rs/bins/login-srv/src/login_server.rs:449` 调 `update_login_record`（fn `:389`），`mir2-rs/crates/storage/src/account.rs:435` 绑 `action_tick`；`action_tick=tick_as_i32()`（`mir2-rs/bins/login-srv/src/login_server.rs:39`、`mir2-rs/bins/login-srv/src/state.rs:20-25`）；C# `src/LoginSrv/Storage/AccountStorage.cs:404` `ActionTick`。读取方按 `GetTickCount()-PassFailTime` 会得到错误量级（§6 D2） |
| P6 | `account.ValidFrom/ValidUntil/StopUntil` | 有效期 | 建号写 `NOW()`（**DB 会话时区**） | `DateTimeOffset.Now`（应用本地） | 本机形状不一致（P2） | `sql/mir2_account.sql:30-34`；Rust `mir2-rs/crates/storage/src/account.rs:341`；C# `src/LoginSrv/Storage/AccountStorage.cs:283-286`；实测行见 §7 S5（datetime 文本比同一时刻本地早 8h） |
| P7 | `goldsales.SellDateTime` | 寄售成交时间 | 读入后只展示，未实现 save | 赋值 `DateTime.Now`，但三个保存调用点全注释 | 双侧死码 | `sql/mir2_data.sql:28`；`src/M2Server/Player/PlayObject.Sale.cs:566`；保存调用点注释见 Rust `goldsales.rs` 模块头（`src/GameSrv/DB/MySqlDB.cs:293-330`） |
| P8 | `upgradeinfo.UpgradeTime/CreateTIme/ModifyTIme` | 武器升级时间 | 未实现 | Load/Save 是 `//todo` stub 恒 `-1` | 双侧死码 | `sql/mir2_data.sql:2004-2007`；`src/GameSrv/DB/MySqlDB.cs:339-350` |
| P9 | `Castle/<n>/SabukW.txt` 日期键 | 攻城/占领日期 | 本地日期文本 | `DateTime.Now` / `ReadWriteDate` | 是（culture 形状已登记） | `src/M2Server/Castle/CastleConf.cs:19-21`；Rust `castle.rs` `refresh_castle_date_texts`（L833） |
| P10 | `GuildBase/Guilds/*` 的 `WarTick/WarTime` | 行会战剩余时长 | 世界虚拟时钟（0 起） | `GetTickCount`（开机毫秒） | 差值等价（已登记族） | `mir2-rs/crates/data/src/guild.rs:497`/`:610`；C# `src/M2Server/Guild*` 加载段 |
| P11 | `SendSelfDelayMsg.DeliveryTime` | 自投递延时到点 | 世界虚拟时钟 | `GetTickCount()+dwDelay` | 差值等价（T-83/T-85 族） | `src/M2Server/Actor/BaseObject.Message.cs:199`；Rust `SelfDelayQueue`/`deliver_delay_magic` |
| P12 | 跨服消息体（planes） | 跨服传递 | **无时间字段** | **无时间字段** | 是 | `mir2-rs/crates/planes/src/message.rs`；`src/Modules/PlanesSystem/PlanesMessage.cs:138` 的 `shifttime` 是未用局部量 |
| P13 | `SS_OPENSESSION` 的 `Seconds` | 账号剩余秒数 | i64 十进制文本 | long 十进制文本 | 是 | C# `src/LoginSrv/Services/ClientSession.cs:513`；Rust `mir2-rs/bins/login-srv/src/login_server.rs:683-690` |
| P14 | 登录耗时日志列表 | 登录时间/在线秒 | 未实现 | `ArrayList` 只增不读（无消费者） | 双侧死码 | `src/M2Server/M2Share.cs:859-862`；全仓 `LogonCostLogList` 无读取点 |

## 6. 偏离项（按严重度）

### D1（P1，客户端可见 + 夹具判据失效）`+GD` 基底与位宽不同

- 现象：Rust 输出 13 位 Unix 毫秒；oracle 输出 9–10 位开机毫秒（int32）。
- 影响：客户端/夹具按 int 解析即溢出；今日 BotSrv 同夹具实测 `good_tick_frames=7765` 但
  `tick_samples=0`（C# 侧 282/282），说明该帧被消费端丢弃（`HUtil32.StrToInt` 溢出回 0，
  `rtime <= 0` 直接 return，连 `ActionLock`/`GMoveBusy` 都不释放）。
- 证据：§7 S1；发射点 9 处全部经同一编码器（`movewire.rs` `good_tick_payload`），修一处即覆盖全部路径。
- 处置：已有在飞卡 `openmir2-gd-ack-boot-millis`（worker-84，P1）；本卡不重复修。修复验收需覆盖
  「`MIR2_CLOCK_MS` 注入时 `+GD` 取值规则」与「9–10 位 + `tick_samples>0`」。

### D2（P1，持久化 + 读取方误判）`account.PassFailTime` 基底不同

- 现象：Rust 在密码失败时写 `PassFailTime = rec.action_tick`，而 `action_tick = tick_as_i32()` ——
  把 **Unix 纪元毫秒**截 u32 再转 i32（`login_server.rs` L39、`state.rs` L20-25）；
  oracle 写 `accountRecord.ActionTick` = `HUtil32.GetTickCount()` —— **开机毫秒 int32**。
- 写入点是活路径：`account_login` 失败分支置 `rec.action_tick` 后调 `update_login_record`
  （`login_server.rs` L449，fn L389），存储层把 `action_tick` 绑到 `PassFailTime`
  （`mir2-rs/crates/storage/src/account.rs:435`）；`change_password` 路径同样写。
- 失败场景：任何按 oracle 语义读该列的消费者用 `GetTickCount() - PassFailTime` 算锁定窗口，
  会得到**错误量级**。以今日归档时刻为例：Rust 写入的 int32 = **-621,000,619**
  （`1,794,675,329,109 mod 2^32 = 3,673,966,677` → i32 溢出为负），C# `GetTickCount()` = 664,724,750
  ⇒ 差 = **1,285,725,369 ms ≈ 14.88 天**，60s/180s 锁窗被判成「早已过期」⇒ 锁定被绕过；
  符号相反时则可能长期锁定。
- 影响面：oracle 兼容读取（C# 栈读同一 `mir2_account.account`、运维查询、跨栈 A/B），以及任何
  后续把该列当「开机毫秒」用的代码。
- 处置：新卡 **`openmir2-account-passfailtime-basis`**（复审方已建）修基底；本卡只登记与取证，不给修法。

### D3（P2，运维/日志判据）日志时间前缀 UTC vs 本地

- 现象：Rust 的 game-svr/game-gate/login-gate/sel-gate/login-srv/ids 六处日志前缀是 **UTC**；
  C# Serilog 是本地时间；只有 db-srv 硬编码 `+8h`。同一台机器同一分钟的两栈日志相差 8 小时。
- 证据：§7 S2。代码：`game-svr` `glog`（L132-143，`(secs/3600)%24` 未加时区）；C# 模板
  `src/BotSrv/AppServer.cs:48`（`{Timestamp:HH:mm:ss.fff}` = 本地）。
- 影响：跨栈时间线对齐、用日志前缀做窗口判据的脚本会错位；db-srv 的 +8 硬编码在换时区/DST 机器上也错。
- 建议：另开修复卡，统一用 `chrono::Local`（或显式时区配置），并把 db-srv 的硬编码 +8 一起收敛。

### D4（P2，已登记族，需 Leader 确认口径）世界虚拟时钟 vs 墙钟

- 现象：oracle 的时间门读 `Environment.TickCount`（真实开机毫秒）；Rust 世界侧大量门读
  `World::clock_ms`（0 起、+200ms/tick）。两者在**同侧差值**语义上等价，但世界时钟在负荷下会落后墙钟
  （`mir2-rs/bins/game-svr/src/ids.rs` `keepalive_clock_ms` 注释记录：59 分钟内 `clock_ms` 走 2950s、墙钟 3547s）。
- 影响面：buff/毒/掉落保留、城堡战时限、行会战剩余、离线挂机、AI 搜索等所有用世界时钟的时长门，
  绝对到期时刻可能被拉长（实测比例约 1.2×）；相位量化另有 ≤200ms。
- 现状：该族已在 whitelist 登记（T-42/T-64/T-66/T-68/T-78/T-85/T-104/T-130），本报告**不新增结论**；
  列出是为了让后续卡在「跨栈绝对时刻」类判据上不要误用墙钟期望值。

### D5（P2，持久化形状）`account.ValidFrom/ValidUntil/StopUntil` 的 UTC/本地文本偏差

- 现象：Rust 建号用 SQL `NOW()` 写这三列（`mir2-rs/crates/storage/src/account.rs:341`），
  但 login-srv 的账户连接池**没有钉 `session.time_zone`**（`mir2-rs/crates/storage/src/account.rs:193-195` 直接
  `MySqlPoolOptions::connect`），sqlx 缺省会话时区 `+00:00` ⇒ `NOW()` 返回 **UTC 墙钟文本**；
  oracle 用应用侧 `DateTimeOffset.Now`（本地）写（`src/LoginSrv/Storage/AccountStorage.cs:283-286`）。
- 实测（§7 S5）：同一行 `CreateTime`（Unix 秒）换算成本地 21:28:25，而 `ValidFrom/StopUntil`
  存的是 13:28:25 —— 恰好早 8 小时；2026-10-13 的 `m3w*`（Rust M3 车道账号）行同样早 8h。
- 影响：本仓未见这三列的读取方（oracle 查询未取用），但任何按本地语义读该列的
  运维/跨栈消费者都会把有效期/停机截止看成提前 8 小时；与 db-srv 已显式钉 `SYSTEM` 的做法不一致。
- 处置：建议归入账户写入面（`openmir2-account-passfailtime-basis` 或另开小卡），本卡只登记实测。

### D6（P3，覆盖缺口，现网不可达）`$DATETIME` 只在 merchant 漏斗注入

- 现象：Rust 只在 `run_merchant_goto` 内注入 `wall_clock`；`run_party_goto_label`、`run_std_mode_func_script`
  等其他脚本漏斗建 `NpcPlayer` 时 `wall_clock` 为空 ⇒ `<$DATETIME>` 展开为空串。
- 现网可达性：部署件 4 个文件（含 1 份副本）用 `<$DATETIME>`，其中 `老人-3.txt`、`沙巴克城堡管理-0150.txt` 走 NPC 点击
  （merchant 漏斗，正常）；`QFunction-0.txt:86` 在 `[@StdModeFunc11]`，而冻结 `stditems` 中 **无 `AniCount=11`
  的物品** ⇒ 当前内容不可达。
- 建议：随脚本漏斗统一化另开小卡（或在该卡登记），不单独修。

### D7（P3，内部，delta-only）Rust 侧 epoch→i32 截断 3 处

- `game-svr` 的 `player.tick = now_ms() as i32`（`lib.rs` L3134/L28415）→ 脚本 `ScriptPlayer::tick_count()`
  （`npc.rs` L2133），供 `DELAYCALL`/`TIMERECALL`/`SETOFFLINE` 的 `wrapping_add` 差值使用；
- `guild_runtime` 的 `now_tick = world.clock_ms as i32`（`guild_runtime.rs` L97）；
- `login-srv` 的 `tick_as_i32`（R8）。
- 现状：前两处只做同侧 `wrapping_*` 差值、不出帧；**第三处经 `update_login_record` 落
  `account.PassFailTime`**（见 D2）。三者与 oracle 的 `GetTickCount` int32 回绕在**同侧差值**上
  行为等价（回绕周期同为 2^32 ms）；但**禁止**把绝对值写进帧/DB/日志判据——D2 正是这条被违反的实例。

### D8（P3，已登记）`MakeIndex` 不含 `GetTickCount`

- C# `GetItemNumber/GetItemNumberEx` 返回 `计数器 + GetTickCount()`；Rust 只返回计数器。
- 已由 whitelist D-1 登记（C# 两次运行都不一致，不存在可对拍真值；DB 列在 `rules.json` 掩码）。
  本报告只确认：**这是有意的基底差异，不是漏改**。

## 7. 运行时抽样（A3）

抽样全部来自今日归档，未起栈（端口检查见 §1.4）。

### S1 `+GD` 双侧值（高风险，客户端可见）

| 侧 | 归档文件 | 首帧值 | 帧数 | tick 样本 |
| --- | --- | --- | --- | --- |
| C# | `E:/UserFleet/archive/botsrv-probe-turn-20261115a/ab-csharp/tier-20/load_stats.ndjson` | `+GD/664724750`（9 位） | `good_tick_frames=282` | `tick_samples=282`，`tick_max_ms=15` |
| Rust | `E:/UserFleet/archive/botsrv-probe-turn-20261115a/rust200-final/tier-200/load_stats.ndjson` | `+GD/1794675329109`（13 位） | `good_tick_frames=7765` | **`tick_samples=0`** |
| Rust（另一轮） | `E:/UserFleet/archive/botsrv-probe-turn-20261115a/rust200-fixed/tier-200/load_stats.ndjson` | `+GD/1794674996708`（13 位） | `good_tick_frames=8674` | **`tick_samples=0`** |

消费端证据：`src/BotSrv/Player/RobotPlayer.cs:6942-6950` 用 `HUtil32.StrToInt(data, 0)` 解析，
`rtime <= 0` 直接 `return`。13 位十进制超出 int32 ⇒ 0 ⇒ 样本为 0。

### S2 日志时间前缀（跨栈时间线）

| 侧 | 文件 | 首行时间 | 本地实况 |
| --- | --- | --- | --- |
| C# GameSvr | `E:/MirServer/logs/GameSvr.out.log` | `00:43:46.749` | 文件 mtime `2026-11-15 00:49:37 +0800` |
| C# BotSrv | `E:/UserFleet/archive/botsrv-probe-turn-20261115a/ab-csharp/tier-20/bots.log` | `00:46:46.403` | 归档 `generated_ms` = 本地 `00:47:24` |
| Rust game-svr | `E:/UserFleet/archive/botsrv-probe-turn-20261115a/rust200-final/stack-logs/game-svr.out.log` | `16:55:18.403` | 同一晚的 00:55 本地 |
| Rust game-gate | `E:/UserFleet/archive/botsrv-probe-turn-20261115a/rust200-final/stack-logs/game-gate.out.log` | `16:55:20.515` | 同上 |

换算：`generated_ms=1794674844440` ⇒ 本地 `2026-11-15 00:47:24` / UTC `2026-11-14 16:47:24`。
Rust 日志 = UTC，C# 日志 = 本地，**同一事件相差 8 小时**。

### S3 DB 持久化时间列（本地 DATETIME）

```
mysql> SELECT NOW(), UTC_TIMESTAMP();
2026-11-15 01:04:35 | 2026-11-14 17:04:35     -- @@system_time_zone = Asia/Shanghai

mysql> SELECT ChrName, CREATEDATE, LASTUPDATE FROM mir2_db.characters ORDER BY LASTUPDATE DESC LIMIT 3;
loadbot191 | 2026-10-10 23:08:26 | 2026-11-15 00:57:08
loadbot183 | 2026-10-10 23:08:25 | 2026-11-15 00:57:08
loadbot195 | 2026-10-10 23:08:26 | 2026-11-15 00:57:08

mysql> SELECT ChrName, CreateDate, ModifyDate FROM mir2_db.characters_indexes ORDER BY ModifyDate DESC LIMIT 2;
loadbot172 | 2026-10-10 23:08:25 | 2026-11-15 00:55:32
```

结论：两侧代码都写 SQL `now()`（DB 本地墙钟）到 DATETIME 列；实测行来自 Rust 窗口（00:53–00:57，
C# 窗口 00:46–00:47 的行已被后续 Rust 存档覆盖），值与运行窗口一致；
Rust db-srv 会话时区被显式钉成 `SYSTEM`（`mir2-rs/bins/db-srv/src/db.rs:186-205`），
与 oracle MySqlConnector 缺省一致（卡 `openmir2-dbsrv-session-timezone` 已有牙）。

### S4 未取到有效样本

- `Castle/0/SabukW.txt` 的日期键仍是 `1998-01-01`/`2021-08-23`，文件 mtime `2026-10-09` ⇒ 本次窗口未重写，
  不构成运行时样本；城堡日期刷新（跨天/开战）标 **待实测**。
- `goldsales.SellDateTime`、`upgradeinfo.UpgradeTime` 无写入路径（§5），无样本可取；
  `PassFailTime` 因无失败登录样本恒 0（列形状见 S5）。

### S5 账户时间列形状（PassFailTime 无失败样本，Valid* 有实据）

```
mysql> SELECT Account, FROM_UNIXTIME(CreateTime), ValidFrom, StopUntil, PassFailTime
       FROM mir2_account.account ORDER BY CreateTime DESC LIMIT 3;
eb30 | 2026-11-14 21:28:25 | 2026-11-14 13:28:25 | 2026-11-14 13:28:25 | 0
eb20 | 2026-11-14 21:28:10 | 2026-11-14 13:28:10 | 2026-11-14 13:28:10 | 0
eb10 | 2026-11-14 21:27:54 | 2026-11-14 13:27:54 | 2026-11-14 13:27:54 | 0
```

结论：`CreateTime/ModifyTime/LastLoginTime` 是 Unix **秒**（绝对量，与 C# 一致）；
`ValidFrom/StopUntil` 是比同一时刻本地早 8h 的 UTC 文本（D5）；`PassFailTime` 当前全 0
（无失败登录），其基底偏离由代码路径 + D2 的算术示例证明，**失败登录实测待补**（本卡未构造错误密码）。

## 8. 覆盖边界（未覆盖的帧/表）

**已覆盖**：帧头全部字段；`+GD`/`+FL` 短包；`SM_SELECTSERVER_OK`/`SM_MOVEMESSAGE`/`SM_DAYCHANGING`/
`SM_TIMECHECK_MSG`；`SM_WINEXP`/`SM_GOLDCHANGED` 的计数字段；入站 `CM_LOGINNOTICEOK`/`CM_CHECKTIME`/
`ISM_STANDARDTICK*`/`CM_PROTOCOL`；planes 跨服消息；IDS 会话消息；`characters*`/`account`/`goldsales`/
`upgradeinfo` 时间列；城堡/行会文件格式；6 个 Rust 二进制的日志前缀。

**未覆盖 / 只做定性判断**：

1. **Rust 未实现的服务**：`ChatSrv`、`CloudGate`、`LogDataSrv`、`MapSrv`、`WebApi`（`src/` 有、`mir2-rs/bins` 无）
   ⇒ 其中 `DateTime.Now`/队列时间戳不进入本表；将来实现时需按本表模板补。
2. **客户端（Delphi）解析**：仓库内无客户端源码；`+GD` 的 int 解析用 BotSrv（同语言同 API）作代理证据，
   冻结客户端是否同样溢出未直接验证。
3. **`mir2_account` 的遗留表**：`sql/mir2_account.sql` 中 `CreateTime/ModifyTime`（datetime，行 6170-6171）、
   `LoginTime`（行 12230）、`FLD_TIME/FLD_UNTIL/...` 等众多 datetime 列不在 Rust 实现面，未逐列对照；
   注意 **`account` 主表已在 Rust 实现面**（§5 P4-P6：P5 偏离见 D2，P6 形状见 D5）。
4. **帧体逐字段全量 diff 未做**：采用「按时间符号定向搜索 + 发送点穷举」；没有时间语义的计数字段
   （背包/负重/技能点等）只抽了经验/金币两类。
5. **`SendMessage.DeliveryTime` 等进程内队列时间**：只确认不出帧，未逐路径核差值语义。
6. **待实测字段**（需真机窗口，本卡不占窗口）：城堡日期跨天刷新、`$DATETIME` 在 merchant 路径的实际文本、
   `SM_DAYCHANGING` 小时边界相位、`PassFailTime` 的失败登录实测（本卡未构造错误密码）。
7. **`+GD` 修复后的验收**：属在飞卡 `openmir2-gd-ack-boot-millis`，本卡只给基线偏离证据。

## 9. 复跑命令（无新脚本，直接复制）

```sh
# 0) 基线
git -C <repo> rev-parse walgit/master

# 1) 符号清单与计数
grep -rn "now_ms\|clock_ms\|UNIX_EPOCH\|SystemTime\|Instant\|Environment.TickCount\|GetTickCount\|DateTime.Now\|DateTime.UtcNow\|as_secs\|as_millis\|as_micros" \
  mir2-rs src --include=*.rs --include=*.cs | wc -l
for s in now_ms clock_ms UNIX_EPOCH SystemTime Instant Environment.TickCount GetTickCount DateTime.Now DateTime.UtcNow as_secs as_millis as_micros; do
  printf '%s : %s\n' "$s" "$(grep -rn "$s" mir2-rs src --include=*.rs --include=*.cs | wc -l)"
done

# 2) 出站字符串短包的穷举面（应只有 GetGoodTick 由时间派生）
grep -rn "SendSocket(" src/M2Server src/GameSrv src/Modules --include=*.cs | wc -l
grep -rn "SendSocket([^;]*\(Tick\|Date\|Time\)" src --include=*.cs

# 3) 归档 A/B 值
python -I -B -c "
import json
for p in ['ab-csharp/tier-20','rust200-final/tier-200','rust200-fixed/tier-200','run3/tier-20']:
    last=None
    for line in open(p+'/load_stats.ndjson',encoding='utf-8'):
        try: last=json.loads(line)
        except: pass
    print(p, last.get('act_first'), last.get('good_tick_frames'), last.get('tick_samples'))
"

# 4) DB 时间列
mysql -h127.0.0.1 -uroot -N -e "SELECT NOW(), UTC_TIMESTAMP(), @@system_time_zone;
  SELECT ChrName, CREATEDATE, LASTUPDATE FROM mir2_db.characters ORDER BY LASTUPDATE DESC LIMIT 3;
  SELECT ChrName, CreateDate, ModifyDate FROM mir2_db.characters_indexes ORDER BY ModifyDate DESC LIMIT 3;"

# 5) 部署脚本里的时间变量（只读）
grep -rIl '<\$DATETIME>' E:/MirServer/M2GameSvr/Envir
grep -rIl '<\$DATE>' E:/MirServer/M2GameSvr/Envir
grep -rIl '<\$MACRUNTIME>' E:/MirServer/M2GameSvr/Envir
grep -rIl '<\$SERVERRUNTIME>' E:/MirServer/M2GameSvr/Envir
```

---

**报告性质**：2026-11-15 时点的普查快照；行号会随 master 漂移，复跑以上命令可重建结论。
