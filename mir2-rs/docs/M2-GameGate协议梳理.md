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

## 7.5 第一次实机取证（2026-10-11，假 GameSvr + 真 C# GameGate）

做法：`E:\tmp\openmir2-gamegate-capture\capture.py`——假 GameSvr 听 5000（记录每条 20B 帧 + 回一帧），
假客户端连 7200；然后启真 `E:\MirServer\RunGate\GameGate.exe`。原始日志：
`E:\tmp\openmir2-gamegate-capture\{capture.log,gate.out.log,gate.err.log}`。

**已证实（oracle 事实，取代此前的推断）**

| 事实 | 观测值 / 证据 |
| --- | --- |
| 客户端一连上 7200，网关立刻发 `GM_OPEN`（ident=1），体是**客户端 IP 字符串**（`b'127.0.0.1'`，9 字节 = `PackLength`） | capture.log 第一帧 |
| `GM_OPEN.Socket` = **客户端 socket handle**（观测 1260，小整数）；`SessionIndex` = 0 | 同上 |
| **线上 `SessionId` 是随机 4 位数字**（观测 8857），**不是数组下标** | `ClientThread.cs:128-160`：取 `connectionId` 哈希最低 8 位，与 4 个随机字节异或后逐字节 `%10`，拼成字符串再 `ushort.TryParse` ⇒ 取值 0..9999，**且没有唯一性检查（可碰撞）** |
| 客户端断开后，网关发 `GM_CLOSE`（ident=2，Socket=该 handle） | capture.log 末两帧 |
| 网关↔服务器之间还有 `GM_CHECKCLIENT`（ident=4）健康帧 | 同上 |
| **客户端 face 的载荷是经典 `#1…!` EDCode 帧**，不是裸 body | `ClientSession.cs:166`：`destinationSpan[2..^1] //跳过#1....! 只保留消息内容`，随后 `:168 EncryptUtil.DecodeSpan` |
| 解码后长度 **< 12** 或原包长 **< `GateShare.CommandFixedLength`** ⇒ **直接关会话**（"异常消息封包，关闭会话..."） | `ClientSession.cs:155-175`（本次实验里我发的 4 字节假 body 正是这样被关的——**负结果来自夹具不合法，不是网关语义**；这条本身是可钉的行为） |
| 1 字节包 = 心跳，忽略 | `ClientSession.cs:157-160` |

**对实现的影响（切片 1 改口径）**

1. `SessionId` 随机 ⇒ **golden/差分不能按字节比 `SessionId`**；等价判据必须是"结构等价 + 同一会话前后一致（该 id 由网关生成并被两侧一致复用）"，或对齐时把该字段归一化。
2. 切片 1 的假客户端**必须发真实形态的帧**（20B 头 + `#1<EDCode(ClientMessage)>!`），最省事的来源是 M0 金标准
   `mir2-rs/tests/golden/c-line-baseline-c3-20261010.frames.ndjson` 的"进世界/移动"阶段帧——不要自己造 body。
3. 差分口径：同一组金标准帧分别打到 **C# GameGate** 与 **Rust GameGate**（背后都接同一个假 GameSvr），
   比对两侧发往 5000 的帧序列（`SessionId` 归一化后）；这比"能进图"强得多，也是 §6 切片 1 判据的前置。

## 7.6 第二次实机取证：**裸网关差分不可行**（客户端 face 有登录门禁）

做法：写差分夹具 `E:\tmp\openmir2-gamegate-capture\diff.py`——假 GameSvr(5000) + 假客户端(7200)，
把 M0 金标准里 **7200 的 39 个 c2s 帧**（`c-line-baseline-c3-*.jsonl`，内层 `#1…!` 帧，外层 20B 头按已知字段构造）
按序打给**真 C# GameGate**，dump 网关发往 5000 的帧。结果与日志：

| 观测 | 证据 |
| --- | --- |
| 网关只发了 1 帧到 5000（`GM_OPEN`，与 §7.5 同形），**没有任何 `GM_DATA`** | `out-csharp.json` |
| 客户端在第 2 帧后被**断开**（WinError 10054） | diff.py 输出 |
| 原因在网关日志里写得很直白：它把**第一帧**当成 `ClientLogin` 来解，读出 `loadbot1/lb1a/2/120020522/...` 之后判 **`客户端登陆消息处理失败，剔除链接`** | `gg-csharp.log`：`[ClientLogin] Kicked 1: …loadbot1/lb1a/2/…` + `客户端登陆消息处理失败，剔除链接` + `断开链接` |

**结论与计划修正**

1. **裸网关（只有假 GameSvr）跑不到"进游戏"帧**：客户端 face 的第一步是**登录帧**，网关要校验它
   （`ClientSession.ClientGetSelectServer`/`ClientLogin` 一带），失败即剔除。⇒ 差分夹具要跑到 `GM_DATA`
   这一级，**必须带整栈**（LoginSrv/SelGate/DBSrv 把会话注册好，客户端拿到的 key 才有效）。
2. **切片 1 的"原样转发"比 oracle 宽**：真网关会对未通过登录的客户端**剔除**，我的实现会照转 `GM_DATA`。
   这不是"等价"，是**登记在案的差异**，必须进切片 2（客户端登录帧的解析与失败剔除）——已同步到卡内 comment，
   不要在合并时把它当成等价。
3. 差分夹具本身还有价值（`GM_OPEN`/`GM_CLOSE` 的形状比对已可做，且它是整栈 E2E 的前置脚手架），
   但"逐帧差分"要等整栈窗口 + 切片 2 的登录门禁落地。

## 7.7 客户端登录帧（切片 2 的规格，已从 oracle 源码钉住）

§7.6 的"裸网关被剔除"暴露了客户端 face 的**第一步**：第一帧不是普通命令帧，而是 **`ClientLogin`**。
`ClientSession.ProcessSessionPacket` 的分派（`ClientSession.cs:655-680`）：

```
if (/* 内层是 CommandMessage 形态 */) { …正常命令：解码→（速度规则）→ 拼 ServerMessage 头 + GM_DATA… }
else {                                  // :671-680
    string tempStr = EDCode.DeCodeString(destinationSpan[2..(packetLen - 1)]);   // :673
    ClientLogin(tempStr, packetLen, "", ref success);                            // :674
    if (!success) { LogService.Info("客户端登陆消息处理失败，剔除链接"); Kick(1); }   // :677-678
}
```

`ClientLogin` 的校验链（`ClientSession.cs:1058-…`）——**这是切片 2 必须照抄的部分**：

| 步骤 | 规则 | 失败动作 |
| --- | --- | --- |
| 0 | `nLen`（外层 PackLength）必须 `> 15 && < 254` | 落进 `else` 之外的路径（不处理） |
| 1 | 解码后的文本**前两位必须是 `**`**（`loginData[0]=='*' && [1]=='*'`） | `Kicked 1` → `success=false` → :677 `Kick(1)` |
| 2 | 用**反斜杠**切成 `account \ charName \ cert \ clientVerNo \ code \ hardwareId`（`HUtil32.GetValidStr3`） | — |
| 3 | `4 ≤ len(account) ≤ 12` 且 `2 < len(charName) < 15` | `success=false`（无日志） |
| 4 | `0 < len(cert) ≤ 8`；`len(clientVerNo) ≥ 8`；`len(code) == 10` | 无日志 / `Kicked 2` / 无日志 |
| 5 | `Config.IsProcClientHardwareID` 为真时校验 `hardwareId`（长度/奇偶 + `ProClientHardwareKey` 变换） | `Kicked 3` → `SendKickMsg(4)` |
| 6 | 之后还有 `Kicked 4..7`（账号/角色的进一步核对、封禁名单等） | 各自 `SendKickMsg` |

⇒ **切片 2 的最小可判实现**：① 识别第一帧为登录帧（内层不是 CommandMessage）→ `EDCode.DeCodeString`
（Rust 侧用 `crates/protocol` 的 EDCode，M0 已钉住与 C# 逐字节一致）；② 实现步骤 1/3/4 的判定与
"失败即 `Kick(1)`"；③ 只在登录通过后才允许把后续帧转成 `GM_DATA`（否则与原实现一样剔除）。
步骤 5/6 属切片 3（配置相关与账号核对），登记不静默跳过。

**顺带发现的一个实验坑（留给差分夹具）**：把金标准里 7200 的帧从**中间**某帧开始当作"某会话的第一帧"重放，
解出来会是**乱码前缀**（本次实测 `?h讣h讣h讣煾窴**loadbot1/lb1a/…`）——EDCode 是**按连接推进**的，
重放必须从该连接的**第一帧**开始、且两侧状态同步。这条直接决定差分夹具怎么写（否则会得到"客户端登陆
消息处理失败"的假红）。

## 7.8 命令路径的出帧构造（切片 3 的规格，已从 oracle 源码钉住）

登录通过后（`HandleLogin=true`，`:153`），每帧按命令处理：先 `EncryptUtil.DecodeSpan`（`:168`）解出
`ClientMessage`（`:177` 读 16B 固定头），再走各命令分支（速度规则/过滤器在这一段，`:205-644`），
**最后统一重建出帧**（`:646-669`）——**不是搬字节**：

```
SendMsg.SessionIndex = SvrListIdx;                                   // :649 索引来自 GM_SERVERUSERINDEX
if (deCodeLen > CommandMessage.Size) {                               // :650 有超出固定头的载荷
    byte[] sendBuffer = new byte[messagePacket.BuffLen - CommandMessage.Size + 1];
    int tLen = EncryptUtil.Encode(decodeBuff, deCodeLen - CommandMessage.Size, sendBuffer);  // :653 尾部**重新编码**
    SendMsg.PackLength = CommandMessage.Size + tLen + 1;             // :654（+1 是编码器补的那个字节）
    sendLen = 20 + SendMsg.PackLength;
    BlockCopy(decodeBuff, 0, bodyBuffer, 20, CommandMessage.Size);   // :657 固定头用**解码后**的 16B
    BlockCopy(tempBuff, CommandFixedLength, bodyBuffer, 20 + 16, tLen); // :658 尾部直接抄**输入的编码态**字节
} else { … PackLength = CommandMessage.Size; body = decodeBuff … }   // :660-666
Serialize(SendMsg) → bodyBuffer[0..20]; ClientThread.Send(sendLen);  // :667-668
```

⇒ 切片 3 要复刻的是：① 先解码拿到 16B `ClientMessage` 固定头；② 固定头**按解码态**原样写进发往
GameSvr 的 body；③ 超出部分**重新编码**（用与解码同一个 EDCode 状态）后再放进去；④ `PackLength`
用 `16 + tLen + 1`；⑤ `SessionIndex` 用本会话的 `SvrListIdx`（**这一条本批已实现并有测试**，见下）。

切片 3 还需要补的取证（**不许猜**）：
1. `CommandMessage` 的 16 字节字段布局（`CommandMessage.Size` 的来源；本文件 `:177` 用
   `MemoryMarshal.Read<ClientMessage>`，两者字段同名不同类，要找 `ClientPackets/*Message.cs` 的定义）；
2. `EncryptUtil.Encode(src, len, dst)` 与 `EDCode` 的**状态推进**：同一连接上"解码 N 字节后再编码 M 字节"
   的索引如何走（`crates/protocol::edcode` 的 `encode/decode` 是纯函数式，需要与 C# 的有状态实现对齐）；
3. `+1` 那个字节到底是什么（编码器终止符？还是 `!`）。

**本批已落地的一条**（切片 3 的第 ⑤ 点）：`GM_SERVERUSERINDEX` 下发后，该会话后续 `GM_DATA` 的
`SessionIndex` 必须一致（`world.rs` 那侧新加 `SessionIndexes` 表 + `server_user_index_is_echoed_on_gm_data`
测试钉住）。

## 7.9 切片 3 的三个取证点：**答案**，以及本批实现到哪

§7.8 列的 3 个"不许猜"的点，本批查清并落了实现：

| 点 | 答案（oracle 行号） |
| --- | --- |
| `CommandMessage` 的 16B 布局？ | **不是 16B，是 12B**：`Recog:int(4) + Ident/Param/Tag/Series:ushort(各 2)` = `CommandMessage.Size = 12`（`src/OpenMir2/Packets/ClientPackets/ClientMesaagePacket.cs:11-24`）。`GateShare.CommandFixedLength = 16` 是**编码态**的长度（12 解码字节 ↔ 16 编码字节，4/3 扩张）——所以 `ClientSession.cs:658` 用 16 当**编码流内**的偏移、`:657` 用 12 当**解码流内**的长度，两者不矛盾。 |
| `EncryptUtil.Encode/Decode` 是否有跨调用状态？ | **没有**：`Encode` 每次调用 `no = 2`、`remainder = 0`，是**纯块码**（每 6 输入字节 → 8 输出字节，末块按 `no != 2` 补一个字节；`src/OpenMir2/EncryptUtil.cs:33-68`）⇒ `crates/protocol::edcode` 的纯函数口径**足够**，不需要额外状态机。 |
| 长度里那个 `+1` 字节是什么？ | **是池内存里的未初始化字节**：`bodyBuffer = GateShare.BytePool.Rent(sendLen)`，`:657-658` 只写了 `12 + tLen` 字节，而 `Send(bodyBuffer[..sendLen])` 送出 `12 + tLen + 1` ⇒ **最后 1 字节从没被赋值**。⇒ 逐字节等价在这一格**不可能**（oracle 自己都不确定）；Rust 侧固定送 `0`，登记为**已知差异**（§8）。 |

**本批实现**（`bins/game-gate`，出帧构造照 `ClientSession.cs:646-669`）：

- 无尾部（解码后 ≤ 12B）：`PackLength = 12`，body = **解码态**字节（oracle 抄的是 `decodeBuff`，不是原样字节）；
- 有尾部：`PackLength = 12 + tLen + 1`（`tLen` = 尾部**重编码**后的长度），body = 解码态 12B + **输入的编码态尾部**（`payload[16..]`）+ 固定的 `0`；
- 两条都有进程级测试钉住（`relays_open_data_heartbeat_and_close` 的无尾部断言、`command_frame_with_tail_is_rebuilt` 的尾部布局断言）。

**仍未做**（登记）：速度规则/过滤器（`SessionSpeedRule`、`Filters/*`，只在非默认配置或异常客户端下可见）、
OTP、`GM_CHECKCLIENT` 周期帧、`ClientTimeOutTime` 空闲回收。卡内验收 #1/#2 仍需整栈窗口。

## 7.10 结构性发现：命令路径含**墙钟依赖的丢帧**，逐帧差分必须先给两侧上同一只虚拟钟

登录通过后的命令处理里，一批分支用**真实时钟**算间隔、**超间隔就丢帧**（`return`，不转发）：
`CM_WALK/CM_RUN` 看 `MoveInterval`（`ClientSession.cs:216-238`）、攻击/施法/拾取/吃东西同理
（`IsMoveInterval`/`IsSpellInterval`/`IsAttackInterval`/`IsPickupInterval`/`IsEatInterval`，
配置项见 `Conf/GateConfig.cs:99-172`）。而**现网部署是开着的**：`E:\MirServer\RunGate\config.conf`
里 `MoveInterval=366`、`AttackInterval=500`、`PickupInterval=330`、`EatInterval=330`、`ChatInterval=800`
等都写了值（另有 `PunishMoveInterval=150` 等惩罚项与 `MaxItemSpeed/MaxItemSpeedRate`）。

⇒ 两条直接结论：

1. **网关发出的帧 ≠ 客户端发出的帧**，即使不考虑 §7.9 的解码/重编码：太快的操作会被**丢掉**。
   所以"M0 金标准里客户端发了 N 帧"不能推出"服务端收到 N 帧"，逐帧差分的**基线**必须是
   **两侧各自往 5000 发的帧**（而不是拿客户端帧当期望值）。
2. **差分必须给两侧同一只虚拟钟**：真时钟下同一串帧的回放时机不同、丢帧集合就会不同（flaky）。
   本仓已有先例可照抄——世界对拍夹具把 `HUtil32.GetTickCount()` **桩成由驱动推进的时钟**
   （`tests/parity/world/README.md` 的顺序契约第 6 条）。⇒ 网关差分需要一个同型的
   **`GameGateGolden`**：从 `src/GameGate` 源码构建（而不是用 `E:\MirServer\RunGate` 的部署件）、
   把时钟桩成虚拟时钟、由驱动按金标准的 `ts_ms` 推进，然后两侧（C# 桩 / Rust）比对发往 5000 的帧。

⇒ 切片 3 的落地顺序据此调整为：**先给 Rust 侧的速度规则留出可注入时钟的接缝**（生产用真钟、
测试用虚拟钟），再实现规则本体；否则规则既不能单测、也无法进差分。

## 8. 头号风险


- **客户端 face 有登录门禁（§7.6）**：未通过登录帧校验的客户端会被**剔除**；切片 1 目前会照转 `GM_DATA`
  ⇒ 这是**登记在案的差异**（不是等价），必须在切片 2 补上登录帧解析 + 失败剔除。
- **协议面是"两套"**：把 LoginGate 的 `ServerDataMessage` 套到 GameGate 上会全错（§5）。
- **客户端面是 20B 头而不是裸 EDCode 流**（§2）⇒ `bins/login-gate` 的 `FrameSplitter`（按 `#`…`!` 拆帧）
  **不能**照搬做客户端面；需要按 `ServerMessage` 头做定长头拆分。
- **最高风险：网关是"解码→处理→重编码"的加工点，不是中继**（§2 的 ⚠️）。
  ⇒ 切片 1 的判据必须先补一条"re-encode 字节恒等性"的实验（§7.2），否则"能进图"也可能是
  "改了字节但两端都容忍"的假等价。
  ——**追记（§7.9 查清）**：`EncryptUtil.Encode` 是**纯块码（无跨调用状态）**，"re-encode 字节恒等"对它成立；
  但命令帧的出帧**本来就不是输入字节**（12B 解码头 + 编码态尾部），所以"逐字节等于客户端发的帧"这个
  预期本身是错的——要比对的是**网关发往 5000 的帧**（对 C# 与 Rust 各自采集后归一化 `SessionId`）。
- **已知不可比对的一格**（§7.9）：有尾部命令帧的最后 1 字节，oracle 是 `BytePool.Rent` 出来的**未初始化**内存
  （`ClientSession.cs:657-668` 只写 `12+tLen` 字节却按 `12+tLen+1` 发送）。Rust 侧固定送 `0`；
  逐帧差分要对这一格做**归一化**（或断言服务端不读它），否则会得到永远不等的假红。
- **墙钟依赖的丢帧**（§7.10）：现网 `MoveInterval=366`/`AttackInterval=500` 等是**开着**的，超间隔的操作会被丢弃
  ⇒ ① 差分基线只能是"两侧各自发往 5000 的帧"，不能拿客户端帧当期望；② 必须给两侧同一只**虚拟钟**
  （照 `tests/parity/world` 桩 `HUtil32.GetTickCount()` 的做法做 `GameGateGolden`），否则回放时机不同、丢帧集合不同 ⇒ flaky。
- 切片 1 若通过入口护栏，只能证明"**入口=Rust、世界=C#**"，不能当成 M3 的"游戏系统对齐"证据
  （与卡内 comment 的口径一致）。
