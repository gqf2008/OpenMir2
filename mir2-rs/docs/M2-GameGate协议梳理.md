# M2 入口协议梳理：Rust GameGate/RunGate（7200）

卡：`openmir2-m2-gamegate`（owner `openmir2-worker-10`，分支 `worktree-m2-gamegate`）。
本文是该卡的**第一步交付**（照 M0/C2 的做法：先把协议面钉住，再写实现），所有结论都带 oracle 行号；
**未取到的点单独列在 §7，不许猜**。

## 1. 拓扑与端口（取证）

| 事实 | 证据 |
| --- | --- |
| GameGate 对外监听 **7200**（`GatePort1`），向下连 **5000**（`ServerPort1`），`Count=3`（另有 7201/7202） | `E:\MirServer\RunGate\config.conf` 的 `[GameGate]` 段（卡内已取证） |
| LoginSrv 侧把玩家导向 7200：`RunAddr=127.0.0.1:7200` | `mir2-rs/docs/M1-边界服务移植计划.md:38` |
| C# 参考实现在 `src/GameGate/**`（**不在** `src/RunGate/**`） | `Get-ChildItem src\GameGate` |
| 与 LoginGate/SelGate **不是同一套内部协议** | 见 §3/§5 |

## 2. 客户端面帧（7200 上，客户端 ↔ 网关）

- C# 用固定头适配器：`PacketFixedHeaderDataHandlingAdapter`，其 `HeaderLength => 20`，头类型是
  `ServerMessage`，body 长度由头里的 `BodyLength` 给出。
  证据：`src/OpenMir2/DataHandlingAdapters/DataMessageFixedHeaderRequestInfo.cs:7`（适配器）、`:12`（`HeaderLength => 20`）、
  `:32`（`private ServerMessage _header`）、`:31`（`BodyLength`）。
- ⇒ **客户端面不是"原始 EDCode 字节流"**，而是 `20 字节 ServerMessage 头 + body`。
- **20 字节头的确切布局**（`[StructLayout(LayoutKind.Sequential, Pack = 1)]`，`PacketSize = 20`）：

  | 偏移 | 类型 | 字段 | 含义 |
  | --- | --- | --- | --- |
  | 0 | u32 | `PacketCode` | 封包标识码 |
  | 4 | i32 | `Socket` | SocketID |
  | 8 | u16 | `SessionId` | 会话 ID |
  | 10 | u16 | `Ident` | 消息号（客户端消息号表） |
  | 12 | i32 | `SessionIndex` | 会话下标（服务端侧用） |
  | 16 | i32 | `PackLength` | 包长（**可为负**，见 §3） |

  证据：`src/OpenMir2/Packets/ServerPackets/ServerMessage.cs:12-27`。
- ⚠️ **网关不是透明中继**：客户端 face 的 body 会被**解码**（`src/GameGate/Services/ClientSession.cs:168`
  `EncryptUtil.DecodeSpan(...)`），网关处理后再**重新编码**发回（`:533`/`:653`/`:917`/`:1206`/`:1274`
  `EncryptUtil.Encode(...)`；聊天的 `EDCode.DeCodeString` 见 `:673`）。
  ⇒ "把客户端字节原样搬进 `GM_DATA`"（切片 1 最初设想）**不足以等价**，必须先把
  "解码→处理→重编码"这一步的口径与**字节恒等性**（同 key 同偏移下 re-encode 是否逐字节相同）实测清楚，
  见 §7.2 与 §8。
- 网关侧客户端连接处理入口：`src/GameGate/Services/ServerService.cs:117`（`ServerSocketClientConnect`）——
  新玩家连上来时：分配 `SessionInfo`（`SessionArray[nIdx]`，`:131-147`）、`ushort sessionId = clientThread.GetSessionId(clientId)`（`:151`）、
  然后 `clientThread.UserEnter(sessionId, userSession.SckHandle, sRemoteAddress)`（`:152`，"通知 GameSvr 有新玩家进入游戏"）。
- 断连：`src/GameGate/Services/ServerService.cs:166`（`ServerSocketClientDisconnect`）。

## 3. Gate ↔ GameSvr 内部帧（5000 上）

- 消息号（**byte**）：`Grobal2.GM_OPEN=1`、`GM_CLOSE=2`、`GM_CHECKSERVER=3`、`GM_CHECKCLIENT=4`、
  `GM_DATA=5`、`GM_SERVERUSERINDEX=6`、`GM_RECEIVE_OK=7`、`GM_STOP=8`。
  证据：`src/OpenMir2/Grobal2.cs:51-58`。
- 收发都带一个 `packetHeader`（含 `SessionId` / `SessionIndex` / `PackLength` / 消息号）+
  可选 body：`src/GameGate/Services/ClientThread.cs:265-300` 的 `switch`（`GM_STOP`/`GM_CHECKSERVER`/
  `GM_SERVERUSERINDEX`/`GM_RECEIVE_OK`/`GM_DATA`）；
  `GM_DATA` 分支把 `PacketLength`（**可取负值**，代码取绝对值）与 `SessionId` 交给上层
  （`ClientThread.cs:284-296`）。
- 网关→服务器方向的构造：`UserEnter` → `SendServerMsg(GM_OPEN, sessionId, socketId, 0, data=客户端 IP 字符串, len)`；
  `UserLeave` → `SendServerMsg(GM_CLOSE, 0, socketId, 0, "", 0)`。
  证据：`src/GameGate/Services/ClientThread.cs`（`UserEnter` / `UserLeave` 两个方法体）。
- 心跳/健康：`GM_CHECKSERVER` 收到即刷新 `CheckServerTick`、清 `CheckServerFail`（`ClientThread.cs:268-271`）；
  `GM_RECEIVE_OK` 收到要**回发**一条 `GM_RECEIVE_OK`（`:276-278`）；`GM_STOP` 把该 server 实例置为
  `RunningState.Stop`（`:265-267`）。
- `GM_SERVERUSERINDEX`：把 `SessionIndex` 记到会话的 `SvrListIdx`（`:272-275`）。

## 4. 会话模型

- 每个"网关→GameSvr"连接 = 一个 `ClientThread`，各自持一个
  `SessionInfo[] SessionArray = new SessionInfo[GateShare.MaxSession]`，**`MaxSession = 10000`**。
  证据：`src/GameGate/Services/ClientThread.cs:21`、`src/GameGate/GateShare.cs:14`。
- 会话 id 是 **ushort**（数组下标语义，`SessionArray[nIdx]`），不是字符串 uuid；
  `GetSessionId(connectionId)` 负责把连接映射到下标（`ClientThread.cs:128-`）。
- 客户端与服务器双向都靠这个 `SessionId` 找对端（`ServerService.cs:151-154` 建 `ClientSession` +
  `sessionMap[clientId] = sessionId`）。

## 5. 现成 Rust 资产：**能复用结构，不能复用 wire**

| 资产 | 能否复用 | 说明 |
| --- | --- | --- |
| `bins/login-gate`、`bins/sel-gate` 的**结构**（config 严格校验、日志前缀、会话表、`fail loudly`/非零退出） | ✅ | 与 `M1` 的 GateShare 同形，可直接照搬骨架 |
| `crates/protocol::frame::ServerDataPacket`（6B 头）+ `protocol::internal::ServerDataMessage`/`ServerDataType`（Enter/Leave/Data/KeepAlive） | ❌ | 那是 **LoginGate↔LoginSrv** 的内部协议；GameGate↔GameSvr 用的是 §3 的 `ServerMessage` 头 + `GM_*` 号，**两套不同** |
| `tools/msgcodegen` 消息号表 | ⚠️ 部分 | `GM_*` 是 `Grobal2` 常量（不在 Messages.cs 的客户端消息号表里），需要按 §3 单独建模 + 各自的门禁 |

## 6. 实现计划（切片，每片都有机器可判判据）

| 切片 | 内容 | 判据 |
| --- | --- | --- |
| **1（本卡第一级）** | Rust `bins/game-gate`：读 `[GameGate]` 配置（严格：缺键/非法端口即非零退出）；监听 7200；按 `Count` 向 5000 建连；实现 §3 的 `GM_*` 帧收发 + §4 的 ushort 会话表；把客户端帧（20B 头 + body）原样搬进 `GM_DATA` 并反向按 `SessionId` 回写；断连发 `GM_CLOSE` | ① 进程级自测：假 GameSvr 收到 `GM_OPEN`（体=客户端 IP）/`GM_DATA`（体=客户端原始 body，逐字节不变）/`GM_CLOSE`，反向回包按 SessionId 落到对应客户端；② `run_m3_gate.ps1` 入口护栏看到 7200=Rust（非 `E:\MirServer`）；③ 四门禁全绿 |
| 2 | C# GameGate 的**行为规则**：`Services/SessionSpeedRule.cs`（156 行速度规则）、`Services/ClientSession.cs` 的包处理面（1222 行）、`ClientThread` 的发送队列/超时 | 与 C# 同输入同输出（含"超速丢弃/回踢"分支）；golden 或表驱动对照 |
| 3 | `Filters/*`（Hardware/Abusive/ChatCommand，共约 600 行）与 `Auth/DynamicAuthenticator`（129 行，OTP/`EnableOtp`） | 逐条对照；未启用的配置项（`EnableOtp=false` 等）要证明"关掉时行为一致" |

> 切片 1 **不做**速度规则/过滤器/OTP：这三块只影响异常客户端行为，不影响"能进图"；但**必须**在卡内与本文登记，
> 不许当成"已经等价"（照 §6.0-3：没登记的差异 = 不过）。

## 7. 还需取证的点（不许猜；取到再写进本文）

1. ~~`ServerMessage` 字段布局~~ **已取证**：见 §2 的偏移表（`ServerMessage.cs:12-27`）。
2. **EDCode 的 re-encode 是否字节恒等**（切片 1 能否按"透传"实现，全看这条）：
   要做的实验——取 `mir2-rs/tests/golden/c-line-baseline-c3-*.frames.ndjson` 里的客户端帧，
   过一遍 C# 的 `EncryptUtil.DecodeSpan` → `EncryptUtil.Encode`，比对字节。
   另外还要取：客户端**首帧**是什么（连上 7200 后先发哪个 `CM_*`）、网关对哪些 `CM_*` 会**改/拦**（不是全透传）。
3. `GM_OPEN` 里 `socketId` 的语义（`SessionInfo.SckHandle`=OS socket 句柄？）与 GameSvr 侧怎么用；
   取证位置：`src/M2Server/**` 里收 `GM_OPEN` 的一侧。
4. `Count=3` 时客户端如何被分配到某个 `ClientThread`（`ServerMgr.GetClientThread` 的策略），
   以及 `GateInfo.ServiceId` 与 7200/7201/7202 的对应关系。
5. `appsettings.json` 与 `config.conf` 的双份配置口径：Rust 侧以哪一份为准（现网部署用的是哪份）。

## 8. 头号风险

- **协议面是"两套"**：把 LoginGate 的 `ServerDataMessage` 套到 GameGate 上会全错（§5）。
- **客户端面是 20B 头而不是裸 EDCode 流**（§2）⇒ `bins/login-gate` 的 `FrameSplitter`（按 `#`…`!` 拆帧）
  **不能**照搬做客户端面；需要按 `ServerMessage` 头做定长头拆分。
- **最高风险：网关是"解码→处理→重编码"的加工点，不是中继**（§2 的 ⚠️）。
  ⇒ 切片 1 的判据必须先补一条"re-encode 字节恒等性"的实验（§7.2），否则"能进图"也可能是
  "改了字节但两端都容忍"的假等价。
- 切片 1 若通过入口护栏，只能证明"**入口=Rust、世界=C#**"，不能当成 M3 的"游戏系统对齐"证据
  （与卡内 comment 的口径一致）。
