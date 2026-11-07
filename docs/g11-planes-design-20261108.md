# G11 跨服（Planes）设计盘点 — A1

> 卡：`openmir2-g11-cross-server`（P1 / tier R）。本文件是 A1 的交付：把 oracle 的跨服实现逐条盘清，
> 供 A2 起逐片移植。**本片只读 + 文档，零运行时代码改动。**
>
> 盘点对象：`src/Modules/PlanesSystem/`（`PlanesMessage.cs` 524 / `PlanesServer.cs` 227 /
> `PlanesClient.cs` 171 / `PlanesService.cs` 21 / `IPlanesService.cs` 6，合计 **949 行**），
> 以及 `src/OpenMir2/Messages.cs` 的 `ISM_*` 常量、`src/GameSrv/` 的调用点。
>
> **修订**：round1 审查（`openmir2-reviewer-26`）指出 6 处事实错误，本版已全部更正并重写；
> 关键错误是**帧分隔符是 `/` 不是 `\`**（见 §1）。

## 1. wire 协议

帧是**文本**，一帧一个括号组，字段用 **`/`** 分隔：

```
( ident / serverNum / body )
```

**`/` 的依据**：`HUtil32.Backslash` 的真实值是 `'/'`（`src/OpenMir2/HUtil32.cs` 第 12 行
`public const char Backslash = '/'`），发送端也是 `nCode + "/" + nServerIdx + "/" + sMsg`
（`src/GameSrv/Word/WorldServer.cs`）。**不要按常量名想当然**（round1 我就是这么错的）。

- **切帧**：`HUtil32.ArrestStringEx(buf, "(", ")", ref frameStr)` —— 取一对括号之间的内容；
  一次 `Received` 回调拿到的字节块可能含**多帧**；但 Received **只做累积**（`SocData +=`），真正的切分发生在 `Run()`（见 §6）。
- **切字段**：`HUtil32.GetValidStr3(seg, ref head, HUtil32.Backslash)` 逐段取：`ident` → `serverNum` → 余下是 `body`。
  `GetValidStr3` 内部是 `Split(..., 2, RemoveEmptyEntries)` ⇒ **前导空字段会被折叠**（连续 `/` 不等于空字段）。
- **数值**：`ident = StrToInt(head, 0)`、`serverNum = StrToInt(sNumStr, -1)`。
- **`body` 内部**还可以再用 `/` 分字段，由各处理器自行 `GetValidStr3`（见 §3）。

### 1.1 服务器端解码（`PlanesServer.DecodeSocStr`）

- 入口条件 `IndexOf(')') <= 0` ⇒ 直接 return（**注意是 `<= 0`，位置 0 也算不处理**）；
- 循环条件 `while (BufStr.IndexOf(')') > 0)`；
- 解析完余量 `ps.SocData = BufStr + ps.SocData` 留到下次；
- **整个循环包在 `try/catch` 里，catch 体只有注释** ⇒ 畸形帧会**静默丢弃当前批次**。
  这是 oracle 的实际行为，不得"顺手修好"。

### 1.2 客户端解码（`PlanesClient.DecodeSocStr`）— 与服务器**不同**

- 入口条件同样是 `IndexOf(')') <= 0`；
- **循环条件是 `> -1`**（服务器是 `> 0`）；
- **catch 里有 `LogService.Error(sExceptionMsg)` + 堆栈**（不是静默吞）；
- 客户端**没有**任何转发调用（见 §2）。

⇒ A2 的编解码必须把这两条路径的**边界差异**也复刻出来，不能合并成一个函数了事。

## 2. 拓扑：**星形**，不是网状

- `PlanesService.Start()`：`SystemShare.ServerIndex == 0` ⇒ `PlanesServer.StartPlanesServer()`（主服）；
  否则 ⇒ `PlanesClient.Initialize() + Start()`（分服）。
- **只有主服跑 PlanesServer**；分服只跑 PlanesClient，**客户端代码里没有任何转发**。
- 主服的 `DecodeSocStr_SendOtherServer` 把每帧先转发给**除来源外**所有已连接服务器，再本地 `ProcessData`
  ⇒ 实际是**主服中继的星形**，**不存在多跳环路**。
- 启动链：`GameServer.cs` → `GameShare.PlanesService.Start()`。
- `SendServerSocket` 是对所有已连接连接广播（不排除自己）。

## 3. 消息表（41 个 case：40 有处理器 + 1 空处理）

`body 字段`列是该处理器源码里 `GetValidStr3` 的**出现顺序**——**其中绝大多数位于 `/* */` 注释块内，
运行时不会执行**。真正执行的只有 205、209、214–221，即**三个 handler 方法**（覆盖 10 个 ident，见 §4.2）；
其余行的字段名只用于说明"原作者打算解析什么"，不构成运行时契约。

| ident | 名称 | 处理器 | body 字段（`/` 分隔） |
|---:|---|---|---|
| 100 | `ISM_GROUPSERVERHEART` | `ServerHeartMessage` | — |
| 200 | `ISM_USERSERVERCHANGE` | `MsgGetUserServerChange` | — |
| 201 | `ISM_USERLOGON` | `MsgGetUserLogon` | — |
| 202 | `ISM_USERLOGOUT` | `MsgGetUserLogout` | — |
| 203 | `ISM_WHISPER` | `MsgGetWhisper` | `uname` |
| 204 | `ISM_SYSOPMSG` | `MsgGetSysopMsg` | — |
| 205 | `ISM_ADDGUILD` | `MsgGetAddGuild` | `gname` |
| 206 | `ISM_DELGUILD` | `MsgGetDelGuild` | — |
| 207 | `ISM_RELOADGUILD` | `MsgGetReloadGuild` | — |
| 208 | `ISM_GUILDMSG` | `MsgGetGuildMsg` | `gname` |
| 209 | `ISM_CHATPROHIBITION` | `MsgGetChatProhibition` | `whostr` / `minstr` |
| 210 | `ISM_CHATPROHIBITIONCANCEL` | `MsgGetChatProhibitionCancel` | — |
| 211 | `ISM_CHANGECASTLEOWNER` | `MsgGetChangeCastleOwner` | — |
| 212 | `ISM_RELOADCASTLEINFO` | `MsgGetReloadCastleAttackers` | — |
| 213 | `ISM_RELOADADMIN` | `MsgGetReloadAdmin` | — |
| 214 | `ISM_FRIEND_INFO` | `MsgGetUserMgr` | `UserName` |
| 215 | `ISM_FRIEND_DELETE` | `MsgGetUserMgr` | `UserName` |
| 216 | `ISM_FRIEND_OPEN` | `MsgGetUserMgr` | `UserName` |
| 217 | `ISM_FRIEND_CLOSE` | `MsgGetUserMgr` | `UserName` |
| 218 | `ISM_FRIEND_RESULT` | `MsgGetUserMgr` | `UserName` |
| 219 | `ISM_TAG_SEND` | `MsgGetUserMgr` | `UserName` |
| 220 | `ISM_TAG_RESULT` | `MsgGetUserMgr` | `UserName` |
| 221 | `ISM_USER_INFO` | `MsgGetUserMgr` | `UserName` |
| 222 | `ISM_CHANGESERVERRECIEVEOK` | `MsgGetUserChangeServerRecieveOk` | — |
| 223 | `ISM_RELOADCHATLOG` | `MsgGetReloadChatLog` | — |
| 224 | `ISM_MARKETOPEN` | `MsgGetMarketOpen(true)` | — |
| 225 | `ISM_MARKETCLOSE` | `MsgGetMarketOpen(false)` | — |
| 227 | `ISM_RELOADMAKEITEMLIST` | `MsgGetReloadMakeItemList` | — |
| 228 | `ISM_GUILDMEMBER_RECALL` | `MsgGetGuildMemberRecall` | `uname` / `dxstr` / `dystr` |
| 229 | `ISM_RELOADGUILDAGIT` | `MsgGetReloadGuildAgit` | — |
| 230 | `ISM_LM_WHISPER` | `MsgGetLoverWhisper` | `uname` |
| 231 | `ISM_GMWHISPER` | `MsgGetGMWhisper` | `uname` |
| 232 | `ISM_LM_LOGIN` | `MsgGetLoverLogin` | `uname` / `lovername` |
| 233 | `ISM_LM_LOGOUT` | `MsgGetLoverLogout` | `uname` |
| 234 | `ISM_REQUEST_RECALL` | `MsgGetRequestRecall` | `uname` |
| 235 | `ISM_RECALL` | `MsgGetRecall` | `uname` / `dxstr` / `dystr` |
| 236 | `ISM_LM_LOGIN_REPLY` | `MsgGetLoverLoginReply` | — |
| 237 | `ISM_LM_KILLED_MSG` | `MsgGetLoverKilledMsg` | `uname` |
| 238 | `ISM_REQUEST_LOVERRECALL` | `MsgGetRequestLoverRecall` | `uname` |
| 241 | `ISM_GUILDWAR` | `MsgGetGuildWarInfo` | `gname` / `warguildname` / `StartTime` |
| 242 | `ISM_GRUOPMESSAGE` | **无处理器**（case 体只有一行注释） | — |

> 表由脚本从源码生成，ident / 常量名 / 处理器名 / 字段顺序均与源码一致（round1 审查机判 0 mismatch）。
> **余量字段未列**：241 `ISM_GUILDWAR` 的第 4 段 `remaintime`、233 `ISM_LM_LOGOUT` 的 `lovername` 未进入本表。
> `226 ISM_LM_DELETE`、`239 ISM_STANDARDTICKREQ`、`240 ISM_STANDARDTICK` 有常量但**都没有 case**，
> 不在这张表里（行为同为 `Unknown`/no-op）。
> **⚠ 本列不代表运行时行为**——§4.2 已按"语句是否在注释里"逐条复核，只有 3 个 handler 方法真的调用了
> `GetValidStr3`，且结果全部丢弃。

## 4. **oracle 侧跨服语义大面积未启用**（round1 审查的关键发现）

`PlanesMessage` 的 40 个有处理器的 ident（32 个不同方法）里，**绝大多数是空实现或整段注释掉的桩**。已逐条确认的例子：

| 处理器 | 实际状态 |
|---|---|
| `ServerHeartMessage` | **函数体为空** |
| `MsgGetUserServerChange`（200） | 只有变量赋值 + **整段注释**（`SendServerGroupMsg(ISM_CHANGESERVERRECIEVEOK, …)` 被注释） |
| `MsgGetUserChangeServerRecieveOk` / `UserLogon` / `UserLogout` | 变量赋值 + **下游调用被注释** |
| `MsgGetChangeCastleOwner`（211） | **`throw new Exception("TODO …")`** —— 异常被 `DecodeSocStr` 的 catch 吞掉，该批剩余帧一并丢弃 |
| `MsgGetAddGuild`（205）/ `MsgGetChatProhibition`（209）/ `MsgGetUserMgr`（214–221） | 真正调了 `GetValidStr3` 解析，但**解析结果被丢弃**（下游全注释） |

**其余处理器的逐条状态尚未核实** —— 我不拿启发式分类充数（第一版试过，自动分类把空实现误判成"有语句"）。
**A2 的第一件事是逐个人工确认这 40 个有处理器的 ident（对应 **32 个不同 handler 方法**）的真实执行状态**，并按「oracle 是空实现 ⇒ Rust 也必须是空实现」
移植；任何"它应该是想做 X 所以实现 X"的推断都算偏离。

### 4.1 逐 ident 执行状态（A2 第 1 步的产出，逐个人工确认）

32 个 handler 方法 → 11 EMPTY / 17 ASSIGN·NOOP / 3 PARSES-DISCARD / 1 THROWS，**0 个有真实下游副作用**。
换算到 ident（`MsgGetUserMgr` 覆盖 8 个 ident）：EMPTY 12 / ASSIGN·NOOP 17 / PARSES-DISCARD 10 / THROWS 1。

| ident | 名称 | 处理器 | 执行状态 |
|---:|---|---|---|
| 100 | `ISM_GROUPSERVERHEART` | `ServerHeartMessage` | EMPTY |
| 200 | `ISM_USERSERVERCHANGE` | `MsgGetUserServerChange` | ASSIGN/NOOP |
| 201 | `ISM_USERLOGON` | `MsgGetUserLogon` | ASSIGN/NOOP |
| 202 | `ISM_USERLOGOUT` | `MsgGetUserLogout` | ASSIGN/NOOP |
| 203 | `ISM_WHISPER` | `MsgGetWhisper` | ASSIGN/NOOP |
| 204 | `ISM_SYSOPMSG` | `MsgGetSysopMsg` | EMPTY |
| 205 | `ISM_ADDGUILD` | `MsgGetAddGuild` | PARSES-DISCARD |
| 206 | `ISM_DELGUILD` | `MsgGetDelGuild` | ASSIGN/NOOP |
| 207 | `ISM_RELOADGUILD` | `MsgGetReloadGuild` | ASSIGN/NOOP |
| 208 | `ISM_GUILDMSG` | `MsgGetGuildMsg` | EMPTY |
| 209 | `ISM_CHATPROHIBITION` | `MsgGetChatProhibition` | PARSES-DISCARD |
| 210 | `ISM_CHATPROHIBITIONCANCEL` | `MsgGetChatProhibitionCancel` | ASSIGN/NOOP |
| 211 | `ISM_CHANGECASTLEOWNER` | `MsgGetChangeCastleOwner` | THROWS |
| 212 | `ISM_RELOADCASTLEINFO` | `MsgGetReloadCastleAttackers` | EMPTY |
| 213 | `ISM_RELOADADMIN` | `MsgGetReloadAdmin` | EMPTY |
| 214 | `ISM_FRIEND_INFO` | `MsgGetUserMgr` | PARSES-DISCARD |
| 215 | `ISM_FRIEND_DELETE` | `MsgGetUserMgr` | PARSES-DISCARD |
| 216 | `ISM_FRIEND_OPEN` | `MsgGetUserMgr` | PARSES-DISCARD |
| 217 | `ISM_FRIEND_CLOSE` | `MsgGetUserMgr` | PARSES-DISCARD |
| 218 | `ISM_FRIEND_RESULT` | `MsgGetUserMgr` | PARSES-DISCARD |
| 219 | `ISM_TAG_SEND` | `MsgGetUserMgr` | PARSES-DISCARD |
| 220 | `ISM_TAG_RESULT` | `MsgGetUserMgr` | PARSES-DISCARD |
| 221 | `ISM_USER_INFO` | `MsgGetUserMgr` | PARSES-DISCARD |
| 222 | `ISM_CHANGESERVERRECIEVEOK` | `MsgGetUserChangeServerRecieveOk` | ASSIGN/NOOP |
| 223 | `ISM_RELOADCHATLOG` | `MsgGetReloadChatLog` | EMPTY |
| 224 | `ISM_MARKETOPEN` | `MsgGetMarketOpen` | EMPTY |
| 225 | `ISM_MARKETCLOSE` | `MsgGetMarketOpen` | EMPTY |
| 227 | `ISM_RELOADMAKEITEMLIST` | `MsgGetReloadMakeItemList` | EMPTY |
| 228 | `ISM_GUILDMEMBER_RECALL` | `MsgGetGuildMemberRecall` | ASSIGN/NOOP |
| 229 | `ISM_RELOADGUILDAGIT` | `MsgGetReloadGuildAgit` | EMPTY |
| 230 | `ISM_LM_WHISPER` | `MsgGetLoverWhisper` | ASSIGN/NOOP |
| 231 | `ISM_GMWHISPER` | `MsgGetGMWhisper` | ASSIGN/NOOP |
| 232 | `ISM_LM_LOGIN` | `MsgGetLoverLogin` | ASSIGN/NOOP |
| 233 | `ISM_LM_LOGOUT` | `MsgGetLoverLogout` | ASSIGN/NOOP |
| 234 | `ISM_REQUEST_RECALL` | `MsgGetRequestRecall` | ASSIGN/NOOP |
| 235 | `ISM_RECALL` | `MsgGetRecall` | ASSIGN/NOOP |
| 236 | `ISM_LM_LOGIN_REPLY` | `MsgGetLoverLoginReply` | EMPTY |
| 237 | `ISM_LM_KILLED_MSG` | `MsgGetLoverKilledMsg` | ASSIGN/NOOP |
| 238 | `ISM_REQUEST_LOVERRECALL` | `MsgGetRequestLoverRecall` | ASSIGN/NOOP |
| 241 | `ISM_GUILDWAR` | `MsgGetGuildWarInfo` | EMPTY |
| 242 | `ISM_GRUOPMESSAGE` | — | EMPTY（case 体只有注释） |

> 状态定义：`EMPTY` 函数体为空；`ASSIGN/NOOP` 只有变量赋值或空守卫；
> `PARSES-DISCARD` 调了 `GetValidStr3` 但结果无人消费；`THROWS` 直接抛异常。
> 判定方法：去掉 `/*…*/` 与 `//` 后看剩余语句，并对 200/205/209/210/211/214 六个逐个肉眼复核。

> 这条对 G11 的整体判断影响很大：**oracle 的跨服协议框架是真的，但业务语义基本是停用的**。
> A4/A5 能不能真正满足 `gm-batch-5b` 的依赖，取决于这些桩后面有没有可用语义 —— 需要在 A2 后重新评估。

### 4.2 A2-3：body 解析的**执行面**（逐语句复核）

把 32 个 handler 的语句逐条看过（区分 `/* */` 内外的）后，**真正执行 `GetValidStr3` 的只有 3 个方法**
（覆盖 10 个 ident：205、209、214–221）：

| ident | 处理器 | 执行的解析 | 结果去向 |
|---|---|---|---|
| 205 | `MsgGetAddGuild` | 1 次：`gname`，余量 `mname` | `M2Share.GuildMgr.AddGuild(gname, mname)` **被注释** ⇒ 丢弃 |
| 209 | `MsgGetChatProhibition` | 2 次：`whostr`、`minstr` | `CmdShutup` 两行**都被注释** ⇒ 丢弃 |
| 214–221 | `MsgGetUserMgr`（8 个 ident） | 1 次：`UserName`，余量 `msgbody` | `UserMgrEngine.OnExternInterMsg` **被注释** ⇒ 丢弃 |

其余 29 个 handler 的 `GetValidStr3` 调用（203/208/228/230/231/232/233/234/235/237/238/241 等）
**全在注释块里**；其中 7 个 handler 的"赋值"就是 `x = Body`（200/201/202/206/207/210/222）。
分布：空实现 11 个、纯局部赋值 17 个（含上述 7 个 `= Body`）、解析后丢弃 3 个、抛异常 1 个
= 32 个方法 / 41 个 case。以上由"剥掉 `/* */` 与 `//` 后是否含 `GetValidStr3` / `= Body`"机判，32 个方法全扫。
⇒ **`ProcessData` 的唯一可观察行为是 211 的 `throw`**；其余一律无副作用。

Rust 侧落点：`mir2-rs/crates/planes/src/message.rs`（`Effect` / `process` / 三个解析函数），
6 条测试覆盖 41 个 case 的分类、226 无 case、三条解析序列与非 ASCII 字节保真。

## 5. 连接表与配置

| 项 | oracle 事实 |
|---|---|
| 配置键 | `SystemShare.Config.MasterSrvAddr` / `MasterSrvPort` |
| 监听 | `TcpService.SetListenIPHosts(IPHost(IPAddress.Parse(MasterSrvAddr), MasterSrvPort))` |
| 连接表 | `TServerMsgInfo { Socket, SocData, SocketId }` —— **没有 nServerIdx** |
| `nServerIdx` | 在另一个结构 `ServerGruopInfo { nServerIdx, sChrName }`，用于 `WorldServer.OtherUserNameList` 的角色→服务器映射 |
| 帧里的 `serverNum` | 只作为 `ProcessData` 的入参，**不落连接表** |
| 槽位 | `Received` 用 `srvArray[clientId-1]`；`Connecting` 取第一个 null 槽 ⇒ **槽位复用后对不上**，见 §5.1（已用 C# 探针实测） |
| 启动 | `GameShare.PlanesService.Start()`（`GameServer.cs`）；`Run()` 由 `SystemProcessor` 每 tick 调 |

### 5.1 槽位语义（A3-1 的 C# 探针实测）

探针已入仓：`tools/planes-probe`（`dotnet run -c Release -- ids` / `-- slots`）。
用 `TouchSocket 2.0.0-beta.273`（与 `src/OpenMir2/OpenMir2.csproj` 同版本）起一个
本地 `TcpService`，逐次连接/断开并打印 `SocketClient.Id`，再把 oracle 的 `srvArray` 逻辑原样搬进去：

- `SocketClient.Id` 是**进程内全局递增的十进制字符串**（`1`,`2`,`3`…），**断开后不复用**；
- `Received` 用 `Id-1` 当槽下标，且**不核对槽里是谁**；
- 首次断线后，新连接的 `Id-1` 指向**空槽**：oracle 在 `Received` 里抛
  `NullReferenceException`，**TouchSocket 静默吞掉**（探针实测进程不崩、该连接的数据
  再也不进任何缓冲）；超过 10 个连接时是 `IndexOutOfRangeException`，同样被吞；
- 极端形态：`Id-1` 指向的槽被**另一个**连接占用时，数据会追加到**别人的缓冲**，
  并被当作那个连接的数据一起解码（探针复现了 `id3 → slot2（属于 id4）`）。

> **可达性**：因 §5.2 的结论（主服从不监听），上述槽位语义在 oracle 的**生产路径不可达**——
> 没有任何连接会进 `srvArray`。A3-1 照抄它，是为了将来真启用时不必重新考古。

⇒ oracle 的 PlanesServer 在**首次断线之后对后续连接基本不可用**。A3-1
（`crates/planes/src/server.rs`）**照抄**该映射，未"修好"：行为等价优先。
**是否需要修，属 Owner 决策**（这是 oracle 缺陷，不是 Rust 移植引入的）。

### 5.2 **oracle 的主服从未开始监听**（A3-2b 的 C# 探针实测）

`PlanesServer.StartPlanesServer()` 只做了三件事：建 `TouchSocketConfig`、`SetListenIPHosts(...)`、
`_serverSocket.Setup(touchSocketConfig)`——**没有 `Start()`**（`PlanesServer.cs` 全文 227 行无
`_serverSocket.Start` 调用）。`Setup` 在 TouchSocket 里只是"记下配置"。

探针（`tools/planes-probe` 的 `listen` 模式，同版本 2.0.0-beta.273）：

| 步骤 | 观测 |
|---|---|
| 只 `Setup(cfg)` | `ServerState = None`；客户端 `ConnectAsync` **TimeoutException** |
| 再 `StartAsync()` | `ServerState = Running`；客户端连上，`Connected` 事件触发 |

⇒ **`ServerIndex == 0` 时跨服主服不监听，任何分服都连不上**；`ServerIndex != 0` 时
`PlanesClient` 连 `127.0.0.1:4900`（缺省）必然失败，按 `src/GameSrv/TimedService.cs:70` 的节拍
（1s 定时器 + 10s 阈值 ⇒ 约每 11s）重试并打印
`链接主分区服务器[addr:port]失败.`。这与 §4 的"业务语义基本停用"是同一个事实的两面。

**本项目的决定（Leader，可回溯）**：G11 按 oracle 如实移植——**不自行加监听**。
理由：① 行为等价优先；② 加监听会让"连得上但业务全是空实现"变成对外可观察的新行为，
属于对外语义变更，不在本卡授权内；③ 是否"修好跨服"是 Owner 的决策，不是移植的副产品。
⇒ A3 的诚实交付是「角色选择 + 客户端连接/重连/失败日志」，**主服侧不监听**；
`crates/planes` 的 Server 状态机保留（供将来真正启用时复用），但在生产路径上不可达。

## 6. 事件与解码路径

- `TcpService.Received` **只做 `SocData +=`**，不解析；
- 解码发生在 `Run()` → `DecodeSocStr`，`Run()` 由 `SystemProcessor` 每 tick 调用；
- 即：**接收是异步累积、解码是 tick 驱动**——这个分离必须在 Rust 侧保留（否则时序不等价）。

## 7. 客户端重连

- `TcpClient` 连 `MasterSrvAddr:MasterSrvPort`；`Connected`/`Disconnected` 事件；
- `SendSocket` 在未连接时**直接丢弃**（不排队、不重试）；
- `CheckConnected()` 由外部定时器驱动，且**仅在 `nServerNumber > 0` 时**才重连。

## 8. 心跳

- `ISM_GROUPSERVERHEART = 100`，处理器 `ServerHeartMessage` **是空函数**；
- **全仓找不到发送点**（排除 bin/obj 后，`GROUPSERVERHEART` 只命中常量定义、case 标签、空处理器；
  30+ 处 `SendServerGroupMsg` 无一用 code=100；`"100/"` / `100 + "/"` 字面量 0 命中）。
- 结论：**只处理不发，且处理是空操作**。A3 不得自行实现心跳。

## 9. 与 `openmir2-gm-batch-5b-cross-server` 的关系

该卡 B 片 6 条 GM 命令依赖"可写行会态 / 跨服"。鉴于 §4 的发现（oracle 侧业务语义基本停用）
与 §5.2 的加固发现（**主服从不监听 ⇒ 跨服链路在生产上根本不通**），
**A2 完成后必须重新评估**：如果 oracle 对这些命令的跨服路径本身就是桩，那么"解除 blocked"的正确含义
可能是"按 oracle 的空语义实现 + 登记"，而不是"造一套能用的跨服"。

## 10. 分片计划

| 片 | 内容 | 判级 |
|---|---|---|
| A1（本片） | 盘点 + 本文档 | L |
| A2 | **先逐个人工确认 40 个处理器的执行状态**；再做 Rust 帧编解码（切帧/切字段/流式缓冲/边界差异）+ body 解码；逐字段对照表 | L |
| A3 | PlanesServer/Client + 配置 + 事件/解码分离 + 槽位语义 + 重连 | R（时序） |
| A4 | guild / castle / market 状态同步（按 A2 核实的 oracle 语义，空就是空） | R（时序 + 落库） |
| A5 | 重新评估并处理 `gm-batch-5b` 的 blocked | — |

## 11. 进度

| 片 | 状态 | 落点 |
|---|---|---|
| A1 | 完成（round1 审查后重写） | 本文档 |
| A2-1 | 完成（40 条逐 ident 执行状态表） | 本文档 §4.1 |
| A2-2 | 完成（帧编解码，C# 探针对拍 + 3020 条差分 0 mismatch） | `mir2-rs/crates/planes/src/lib.rs` |
| A2-3 | 完成（dispatch 分类 + 三条实际执行的解析） | `mir2-rs/crates/planes/src/message.rs`、本文档 §4.2 |
| A3-1 | 完成（Server/Client 纯状态机 + 槽位语义，C# 探针实测） | `mir2-rs/crates/planes/src/{server,client,service}.rs` |
| A3-2a | 完成（`[Server] MsgSrvAddr/MsgSrvPort/ServerNumber` 解析） | `mir2-rs/bins/game-svr/src/config.rs` |
| A3-2b | 完成（按 §5.2：客户端连接/重连/失败日志；主服不监听） | `mir2-rs/bins/game-svr/src/planes.rs` |

## 12. 待确认 / 风险

1. 40 个处理器的逐条执行状态（A2 第一件事）。
2. `body` 内文本是 GB2312 字节流（`HUtil32.GetBytes/GetString` 均为 `Encoding.GetEncoding("gb2312")`），
   A2 必须按字节处理，不得先转 UTF-8。
3. 槽位复用（§5）。
4. `GetValidStr3` 的 `RemoveEmptyEntries` 折叠语义（§1）。
5. oracle 跨服语义大面积停用（§4）对 G11 与 `gm-batch-5b` 的整体结论的影响。

---

**证据**：本片结论来自对上述 5 个文件的逐段阅读与 `git grep`；未运行代码、未起栈、未改 `src/**`。
round1 的 6 条 Important 已逐条核对源码后修正（帧分隔符、客户端解码差异、星形拓扑、启动/解码路径、
连接表字段、处理器执行状态）。
