# G11 跨服（Planes）设计盘点 — A1

> 卡：`openmir2-g11-cross-server`（P1 / tier R）。本文件是 A1 的交付：把 oracle 的跨服实现逐条盘清，
> 供 A2 起逐片移植。**本片只读 + 文档，零运行时代码改动。**
>
> 盘点对象：`src/Modules/PlanesSystem/`（`PlanesMessage.cs` 524 行 / `PlanesServer.cs` 227 行 /
> `PlanesClient.cs` 171 行 / `PlanesService.cs` 21 行 / `IPlanesService.cs` 6 行，合计 **949 行**），
> 以及与 `src/OpenMir2/Messages.cs` 的 `ISM_*` 常量、`src/GameSrv/` 的调用点。

## 1. wire 协议（`PlanesServer.DecodeSocStr`）

帧是**文本**，一帧一个括号组：

```
( ident \ serverNum \ body )
```

- **切帧**：`HUtil32.ArrestStringEx(buf, "(", ")", ref frameStr)` —— 取一对括号之间的内容；
  一次 `Received` 里可能有 **多帧**，循环处理直到不再出现 `)`。
- **切字段**：`HUtil32.GetValidStr3(seg, ref head, HUtil32.Backslash)` 逐段取：
  先取 `ident`，再取 `serverNum`，余下整体是 `body`。
  即 `body` **内部还可以再用 `\` 分字段**（见 §3 表），由各处理器自行 `GetValidStr3`。
- **流式语义（必须 1:1 复刻）**：
  - `SocData` 为空或**不含 `)`** ⇒ 直接 `return`，什么都不解析；
  - 解析完剩余的 `BufStr` 会 `ps.SocData = BufStr + ps.SocData` **留到下次**；
  - 整个循环包在 `try/catch` 里，**异常被吞掉**（catch 体是注释）。⇒ 畸形帧会静默丢弃当前批次，
    这是 oracle 的实际行为，不得"顺手修好"。
- **数值解析**：`ident = StrToInt(head, 0)`、`serverNum = StrToInt(sNumStr, -1)`（默认值分别为 0 / -1）。

## 2. 中继语义（`DecodeSocStr_SendOtherServer`）

**每解析出一帧，先转发给"除来源连接外的所有已连接服务器"，然后才本地 `ProcessData`。**

- 转发条件是 `serverMsgInfo.Socket != null && SocketId != 来源.SocketId`；
- 即拓扑是**网状全连接**：A→B 的帧会被 B 再转给 C、D……（oracle 未做环路抑制，靠消息语义幂等）；
- `SendServerSocket(msgstr)` 则是对**所有**已连接服务器广播（不排除自己）。

## 3. 消息表（40 条有处理器的 + 1 条空处理）

`body` 字段列来自各处理器内部的 `GetValidStr3(..., ref X, HUtil32.Backslash)` 顺序；
`—` 表示处理器不按 `\` 拆字段（要么整段用，要么该消息只用 `serverNum`）。

| ident | 名称 | 处理器 | body 字段（`\` 分隔） |
|---:|---|---|---|
| 100 | `ISM_GROUPSERVERHEART` | `ServerHeartMessage` | — |
| 200 | `ISM_USERSERVERCHANGE` | `MsgGetUserServerChange` | — |
| 201 | `ISM_USERLOGON` | `MsgGetUserLogon` | — |
| 202 | `ISM_USERLOGOUT` | `MsgGetUserLogout` | — |
| 203 | `ISM_WHISPER` | `MsgGetWhisper` | uname |
| 204 | `ISM_SYSOPMSG` | `MsgGetSysopMsg` | — |
| 205 | `ISM_ADDGUILD` | `MsgGetAddGuild` | gname |
| 206 | `ISM_DELGUILD` | `MsgGetDelGuild` | — |
| 207 | `ISM_RELOADGUILD` | `MsgGetReloadGuild` | — |
| 208 | `ISM_GUILDMSG` | `MsgGetGuildMsg` | gname |
| 209 | `ISM_CHATPROHIBITION` | `MsgGetChatProhibition` | whostr | minstr |
| 210 | `ISM_CHATPROHIBITIONCANCEL` | `MsgGetChatProhibitionCancel` | — |
| 211 | `ISM_CHANGECASTLEOWNER` | `MsgGetChangeCastleOwner` | — |
| 212 | `ISM_RELOADCASTLEINFO` | `MsgGetReloadCastleAttackers` | — |
| 213 | `ISM_RELOADADMIN` | `MsgGetReloadAdmin` | — |
| 214 | `ISM_FRIEND_INFO` | `MsgGetUserMgr` | UserName |
| 215 | `ISM_FRIEND_DELETE` | `MsgGetUserMgr` | UserName |
| 216 | `ISM_FRIEND_OPEN` | `MsgGetUserMgr` | UserName |
| 217 | `ISM_FRIEND_CLOSE` | `MsgGetUserMgr` | UserName |
| 218 | `ISM_FRIEND_RESULT` | `MsgGetUserMgr` | UserName |
| 219 | `ISM_TAG_SEND` | `MsgGetUserMgr` | UserName |
| 220 | `ISM_TAG_RESULT` | `MsgGetUserMgr` | UserName |
| 221 | `ISM_USER_INFO` | `MsgGetUserMgr` | UserName |
| 222 | `ISM_CHANGESERVERRECIEVEOK` | `MsgGetUserChangeServerRecieveOk` | — |
| 223 | `ISM_RELOADCHATLOG` | `MsgGetReloadChatLog` | — |
| 224 | `ISM_MARKETOPEN` | `MsgGetMarketOpen` | — |
| 225 | `ISM_MARKETCLOSE` | `MsgGetMarketOpen` | — |
| 227 | `ISM_RELOADMAKEITEMLIST` | `MsgGetReloadMakeItemList` | — |
| 228 | `ISM_GUILDMEMBER_RECALL` | `MsgGetGuildMemberRecall` | uname | dxstr | dystr |
| 229 | `ISM_RELOADGUILDAGIT` | `MsgGetReloadGuildAgit` | — |
| 230 | `ISM_LM_WHISPER` | `MsgGetLoverWhisper` | uname |
| 231 | `ISM_GMWHISPER` | `MsgGetGMWhisper` | uname |
| 232 | `ISM_LM_LOGIN` | `MsgGetLoverLogin` | uname | lovername |
| 233 | `ISM_LM_LOGOUT` | `MsgGetLoverLogout` | uname |
| 234 | `ISM_REQUEST_RECALL` | `MsgGetRequestRecall` | uname |
| 235 | `ISM_RECALL` | `MsgGetRecall` | uname | dxstr | dystr |
| 236 | `ISM_LM_LOGIN_REPLY` | `MsgGetLoverLoginReply` | — |
| 237 | `ISM_LM_KILLED_MSG` | `MsgGetLoverKilledMsg` | uname |
| 238 | `ISM_REQUEST_LOVERRECALL` | `MsgGetRequestLoverRecall` | uname |
| 241 | `ISM_GUILDWAR` | `MsgGetGuildWarInfo` | gname | warguildname | StartTime |

> **空处理**：`ISM_GRUOPMESSAGE = 242` 在 `ProcessData` 里只有一行注释（`//LogService.Info("跨服消息")`），
> **不做任何事**——移植时也要保留这个空分支，否则行为不等价。

## 4. 拓扑与配置

| 项 | oracle 事实 |
|---|---|
| 配置键 | `SystemShare.Config.MasterSrvAddr` / `MasterSrvPort`（`PlanesServer` 与 `PlanesClient` 都用同一对） |
| 服务端监听 | `TcpService.SetListenIPHosts(IPHost(MasterSrvAddr, MasterSrvPort))`，`Received` 事件进 `DecodeSocStr` |
| 连接表 | `srvArray[i]`，每项 `TServerMsgInfo{Socket, SocketId, SocData, nServerIdx}`；`nServerIdx` 由对端帧里的 `serverNum` 标识 |
| 广播 | `SendServerSocket` 遍历 `srvArray` 发给所有 `Connected` 连接 |
| 中继 | 见 §2，按 `SocketId` 排除来源 |
| 启动 | `PlanesServer.Instance.Run()`（`src/GameSrv/Word/Threads/SystemProcessor.cs` 的 `PlanesServer.Instance.Run()`） |

## 5. 客户端（`PlanesClient`）

- `TcpClient` 连 `MasterSrvAddr:MasterSrvPort`；`Connected`/`Disconnected` 事件挂 `MsgClientConnect` / `MsgClientDisconnected`；
- `IsConnect()` 判定，`SendSocket` 在未连接时**直接丢弃**（不排队、不重试）；
- `CheckConnected()` 由外部定时器驱动做重连；
- 连接失败/超时有 `LogService.Error("链接主分区服务器[addr:port]失败/超时")` 两种可观察日志。

## 6. 心跳

- `ISM_GROUPSERVERHEART = 100`，处理器 `ServerHeartMessage(serverNum, body)`；
- **本片未在 `src/` 里找到发送该心跳的调用点**（`git grep ISM_GROUPSERVERHEART` 只命中定义与 case）。
  ⇒ A2/A3 需要再确认：是"只处理不发"，还是发送点在别处（如独立进程/客户端）。**不许假设**。

## 7. 与 `openmir2-gm-batch-5b-cross-server` 的关系

该卡 6 条 GM 命令的 B 片依赖"可写行会态 / 跨服"能力。本卡 A4/A5 完成后，
它的依赖点才可逐条满足；**在那之前它保持 blocked**（其 issue 明确写了不许造假通道）。

## 8. 分片计划

| 片 | 内容 | 判级 |
|---|---|---|
| A1（本片） | 盘点 + 本设计文档 | L |
| A2 | Rust 侧帧编解码（切帧/切字段/流式缓冲/数值默认值）+ 40 条消息的 body 字段解码；逐字段对照表 | L（纯编解码、可构造级对拍） |
| A3 | PlanesServer/Client + 配置加载 + game-svr 接线 + 重连/心跳（含 §6 的发送点确认） | R（连接时序） |
| A4 | guild / castle / market 跨服状态同步落地 | R（时序 + 落库） |
| A5 | 解除 `gm-batch-5b` blocked，逐条核对 6 条依赖 | — |

## 9. 待确认 / 风险（A2 前必须回答）

1. **心跳发送点**：§6 的未决项。
2. **`body` 内字段的编码**：oracle 用 GB2312 字节流承载文本（本仓协议层约定），
   跨服 body 里是否也含中文（行会名等）——A2 必须按字节处理，不得先转 UTF-8。
3. **中继环路**：oracle 无环路抑制，靠消息幂等；Rust 移植**不得**自作主张加抑制。
4. **`SocketId` 的语义**：TouchSocket 的内部 id，移植时需要一个稳定映射（建议用对端 `serverNum` + 连接序号）。
5. **`srvArray` 容量与并发**：oracle 是固定数组 + 单线程事件；Rust 侧要与现有 GameSvr 的 actor 模型对齐。

---

**证据**：本片全部结论来自对上述 5 个文件的逐段阅读与 `git grep`，命令与行号已内联；
未运行任何代码，未起栈，未改 `src/**`。
