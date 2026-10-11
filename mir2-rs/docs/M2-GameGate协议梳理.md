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

- ⚠️ **本节 2026-10-11 修正（原来的说法是错的，不要再照抄）**：代码库里**确实有**
  `PacketFixedHeaderDataHandlingAdapter`（`HeaderLength => 20`，头是 `ServerMessage`；
  `src/OpenMir2/DataHandlingAdapters/DataMessageFixedHeaderRequestInfo.cs:7/:12/:31/:32`），
  **但 GameGate 的客户端 face 并没有挂它**：`ServerService.Initialize()`（`:38-50`）的
  `_serverSocket.Setup(new TouchSocketConfig().SetListenIPHosts(...))` 里**没有** `SetDataHandlingAdapter`
  ⇒ TouchSocket 按"一次 read 一帧"把原始字节交给 `ProcessSessionPacket`（`ClientSession.cs:98-`）。
- **实测**（真 C# GameGate，副本跑 5010/7210；输入 = M0 金标准里真 Delphi 客户端发的 7200 第一帧）：
  - 加 20B `ServerMessage` 头 ⇒ 网关日志 `[ClientLogin] Kicked 1: ?h讣h浖…**loadbot1/lb1a/…`** ⇒ 剔除
    （头被当成帧内垃圾前缀；`forensic.py` 用 `EncryptUtil.Decode` 的 Python 逐行转写复现了这段乱码）；
  - **不加头（纯 `#1…!`）⇒ 登录通过**，网关在 5000 侧发出合成登录包（98B）。
  ⇒ **客户端 face = 裸 `#…!` 帧**（登录帧 `#1…!`），一次 send/read 就是一帧。
  证据：`E:\tmp\openmir2-gamegate-imp\oracle-login-cap\`（`harness.py` / `out-csharp-raw.json` / `FINDINGS.md`）。
- 顺带解开一个旧谜：§7.6 那次"裸网关重放被剔除"的**真实原因是我们自己按假设加了 20B 头**，不是"必须带整栈"。
- 关于"一次 read 一帧"的边界：把两帧写进**一次 send** 时，oracle 会把它们当**一帧**（实测：合成
  `GM_DATA` 的 `PackLength=40`、body 里能看到 `!`/`#1` 混在一起），**不拆帧**。⇒ Rust 侧照此
  （`bins/game-gate` 的读循环）也就是"一次 read 一帧"；真实客户端每帧一次 send，故一致。

## 7.12 客户端面三处口径的实机复测（2026-10-11，推翻 §2 的旧说法）

夹具：`E:\tmp\openmir2-gamegate-imp\oracle-login-cap\`（`harness.py` 把 `E:\MirServer\RunGate` 整目录副本的
`config.conf` 只改到 `ServerPort1=5010 / GatePort1=7210`，**不占现网 5000/7200**；假 GameSvr 在 5010，
假客户端在 7210；输入是 M0 金标准 7200 的第一条 c2s 帧）。

| 面 | 实测（真 C# 网关） | 本切片改前 | 改后 |
| --- | --- | --- | --- |
| 客户端→网关 framing | 裸 `#1…!`（**无头**；加 20B 头 ⇒ `Kicked 1`） | 读 20B 头（**真客户端必被剔除**） | 一次 read 一帧 |
| 登录字段分隔符 | **`/`**（`HUtil32.Backslash='/';`，`HUtil32.cs:12`） | `'\\'`（**account 会超长 ⇒ 必被剔除**） | `FIELD_DIVIDER='/'` |
| 网关→客户端（`PackLength>0`） | `#` + `Encode(body[..12])` + `body[12..]` **原样** + `!` | 原样写 body | 同 oracle |
| 网关→客户端（`PackLength<0`，"小包"） | `#` + body **原样** + `!` | 原样写 body（缺 `#`/`!`） | 同 oracle |
| 合成登录包 | `#0` + `Encode(GBK(text))` + `!`，`PackLength=encodeLen+3`；`hardware` = **30 个 `0`** | 同（但 `hardware` 曾误写 32 个） | 由 oracle 字节钉住 |
| 超过 `NomClientPacketSize=400` 的帧 | 记日志**丢帧、不关会话**（`:196-199`） | 未实现 | 已实现 |

**判据（可复跑，不需要整栈窗口）**：`cargo test -p game-gate` 里的
`oracle_golden_login_frame_yields_oracle_synthetic_packet`（**真客户端帧 ⇒ 必须逐字节等于真网关吐出的那条**）、
`header_prefixed_login_is_kicked`（framing 红检）、`downlink_frames_match_oracle_bytes`（下行两种形态，
期望值就是本节实测的字节）、`oversize_frame_is_dropped_without_kick`。

**教益（写进手记的理由）**：这三处里有两处是"照源码推断"必然踩的坑——① 代码库里有 20B 头的适配器
≠ 这条链路挂了它；② 常量名叫 `Backslash` 但值是 `'/'`。**凡是"端到端会不会通"的问题，判据必须是
把真字节喂进真实现**，公式对照与语义单测都会漏。
- ~~20 字节头的确切布局（客户端 face）~~ ——该布局只适用于**网关↔GameSvr（§3）**与**网关→客户端**的帧；
  客户端→网关方向没有头。表留作 §3 参考：
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

**仍未做**（登记，**清单以 §7.11 结尾的"仍未做"为准**）：速度规则的**非聊天部分**
（`SessionSpeedRule` 的 move/attack/spell 预算——现网 `OverSpeedPunishMethod=3`（DelaySend）下它们照转，
只在异常/持续超速时才可见，见 §7.10.1）、`Filters/*`、OTP（oracle 侧相关代码整段被注释掉）、
`GM_CHECKCLIENT` 周期帧、`ClientTimeOutTime` 空闲回收、`MaxClientPacketCount` 等上限计数。
卡内验收 #1/#2 仍需整栈窗口。

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

### 7.10.1 **自我修正（同轮复核源码得出）**：在现网这套配置下，超速帧**不会被丢**

上面那段结论只看了"用墙钟算间隔"，没看**惩罚分支怎么处理**。补读之后要改口径：

| 事实 | 证据 |
| --- | --- |
| 规则**确实全开** | 现网 `config.conf` 的 `[Switch]` 段：`MoveInterval=1`、`SpellInterval=1`、`AttackInterval=1`、`PickupInterval=1`、`EatInterval=1`、`TurnInterval=1`、`ChatInterval=1`…（`GateConfig.cs:259-261` 的默认值也全 `true`） |
| 惩罚方式是 **DelaySend** | `[Method] OverSpeedPunishMethod=3`（= `PunishMethod.DelaySend`，与 `GateConfig.cs:286` 默认一致）；`[Float] PunishIntervalRate=1`、`[Integer] PunishBaseInterval=20` |
| **DelaySend 下超速帧照转**（不丢） | `ClientSession.cs:240-261`：`delayMsgCount == 0` 时只把 `MoveTick = currentTick + dwDelay`（预算后推），**没有 `return`**，函数继续走到 `:262 break` ⇒ 该帧仍被转发；只有 `delayMsgCount != 0` 那条才会 `return`（并可能在 `>= 2` 时踢人） |
| 另一条会丢/踢的是别的开关 | `[Switch] KickOverSpeed=0`（关）、`DoMotaeboSpeedCheck=1`（`SpeedHackWarnMethod` 相关）、`DenyPresend=0`（关）、`KickOverPacketSize=1`（只对超大包，`:196-201`） |

⇒ **口径修正**：在现网配置（DelaySend + 延迟队列为空）下，**正常客户端流量的"网关→5000 帧集合"与客户端帧集合一致**
⇒ **字节级差分不需要虚拟钟也能做**；墙钟只影响三处：① 预算后推（不改变当帧）；② 持续超速触发 `SendKickMsg`/踢人；
③ 延迟队列/motaebo/CC 这些**异常**路径。⇒ 文档 §7.10 的原结论（"必须上虚拟钟"）**降级**为：
**差分优先做"正常流量 + DelaySend"；异常/惩罚路径单开场景**（那里才需要虚拟钟或专门的 pacing 驱动）。
这个修正也把切片 3 的优先级摆正：**先补"预算/踢人"这些会影响会话的规则**（它们能改变**帧集合**），
再补只影响时机的部分。

`PunishMethod` 枚举已核：`TurnPack=0, DropPack=1, NullPack=2, DelaySend=3`（`GateShare.cs:198-216`）
⇒ 现网 `OverSpeedPunishMethod=3` **就是 DelaySend**，"超速照转、只后推预算"的结论成立。

### 7.10.2 再补一刀：**聊天（say）那条间隔是"直接 return"** ⇒ 它会**丢帧**

7.10.1 的"正常流量不受影响"**只对 move/attack 一类的 DelaySend 分支成立**。补读聊天分支（`ClientSession.cs:488-546`）：

```
if (!msgContent.StartsWith(_commandFlagSpan)) {          // 非 @命令
    currentTick = GetTickCount();
    if (currentTick - gameSpeed.SayMsgTick < Config.ChatInterval) {
        return;                                          // :494 —— 直接丢，不转发！
    }
    gameSpeed.SayMsgTick = currentTick;
}
```

而现网 `[Switch] ChatInterval=1`、`[Integer] ChatInterval=800` ⇒ **聊天帧在 800ms 内发第二条就会被丢掉**。
另一条重写路径是 `GateShare.ChatCommandFilterMap`（命中就改写成 `SM_WHISPER` 并 `return`，`:519-537`），
配置 `ChatCmdFilter=0` ⇒ 该 map 应为空（惰性）。

⇒ 对 M2 验收（含"说话"）的影响：**聊天流量的"网关→5000 帧集合"会被时间影响**，所以
① 逐帧差分若含聊天，必须 pacing（按金标准 `ts_ms`）或给两侧虚拟钟；② 或者把聊天从"字节等价"里
单列成"间隔规则 + 转发形态"两个场景（间隔规则用虚拟钟单测；转发形态用 `_commandFlagSpan` 开头的命令帧走"不判间隔"的分支来比字节）。
③ 聊天帧**被转发时**的字节形态仍走 §7.9 的通用重建（`pszChatBuffer` 那套只在命中命令过滤器时用），
所以切片 3 核心对它依旧适用。

## 7.11 本轮收口：reviewer-1 的 2 Important + 3 Minor（2026-10-11）

独立审查者 `openmir2-reviewer-1` 对 `ad40cbbe`（切片 1+2）出了 `request_changes`，逐条处置如下
（判据都在 `bins/game-gate` 的测试里，可复跑）：

| 条目 | 处置 | 证据 |
| --- | --- | --- |
| **Important #1**：登录通过后**没有**向 GameSvr 发合成登录包（oracle `ClientSession.cs:1199-1224`）⇒ GameSvr 永远等不到登录 | **已实现**：登录帧通过后立刻发一条 `GM_DATA`：body = `#0` + `Encode(GBK("**account/charName/cert/clientVer/code/{hardware}/{ServiceId}"))` + `!`，`PackLength = encodeLen + 3`。`hardware` = **30 个 `0`**（`:1202` 的字面量 `"0"×30` 就是 30 个；依据**现网配置** `[Switch] ProcClientCount=0` ⇒ `Config.IsProcClientHardwareID=false`，走该三元表达式的 false 分支。「30」是 §7.12 用 oracle 字节回归用例校正过的，先前写「32」是目测错的）；`ServiceId` = 端点线号（`Conf\ConfigManager.cs:18`：`GateList[i].ServiceId = (byte)i`） | 进程级测试 `login_success_sends_synthetic_login_packet` + **oracle 字节用例** `oracle_golden_login_frame_yields_oracle_synthetic_packet`（真客户端帧 ⇒ 逐字节等于真网关的输出）；单测 `synthetic_login_body_matches_oracle_formula` |
| **Important #2**：`get_valid_str3` 与 `HUtil32.GetValidStr3` 不等价（C# `Split(divider,2,RemoveEmptyEntries)` 的语义 ≠ `split_once + retain`）⇒ 含前导/连续分隔符时字段错位 ⇒ **假踢** | **已实现**：按实测语义重写——「**连续分隔符并作一个、首尾分隔符吃掉**」：先跳过开头所有分隔符 ⇒ `dest` = 第一个非空片段 ⇒ 再跳过紧随其后的分隔符 ⇒ 其余原样返回 | 真 .NET 9 逐例实测表 `E:\tmp\openmir2-watch\splittest\`（`Program.cs` 复刻 `GetValidStr3`，输出 `split-truth-table.txt`，21 例）⇒ 单测 `get_valid_str3_semantics` 把整张表钉进 Rust；`leading_separator_login_is_not_falsely_kicked` 钉"不再假踢"（reviewer-1 举的 `\acc\name\cert\ver\code` 正是表内第 13 例） |
| Minor #3：命令路径缺 `deCodeLen < 12` 判定 | **已实现**（`:170-175` ⇒ 关会话），且放在聊天间隔判定**之前**（oracle 的顺序） | `handle_frame`；`tests/relay.rs` 覆盖 |
| Minor #2：`relay.rs` 把心跳放在登录前、并断言"恰好 1 条 `GM_DATA`" | **已改**：心跳**登录前后各发一条**（两条 oracle 路径不同，见下），断言改为"命令帧恰好 1 条 + 合成登录包恰好 1 条（且 `PackLength` = body 长）" | `relays_open_data_heartbeat_and_close` |
| Minor #1："未登录时 1 字节帧被忽略（oracle 会踢）" | **就地核对后判为口径误判**（代码不变）：oracle 登录前对 1 字节帧走 `:673` 的 `destinationSpan[2..(packetLen-1)]` = `[2..0]` ⇒ 抛 `ArgumentOutOfRangeException`，被 `ServerManager.cs:234-241` 的 catch **只记日志**（`LogService.Error(e.Message)`）⇒ **帧被丢、会话存活**，`Kick`（= `Socket.Close()`）不会被调用。Rust 侧（丢弃 + 保持会话）与之一致，差异只在日志文本 | `ClientSession.cs:673`、`ServerManager.cs:234-241`（代码注释里逐条写明） |

**同批落的 §7.10.2 聊天间隔**：`bins/game-gate/src/rules.rs` 的 `ChatThrottle`（纯函数 + 显式时间戳）+
`[Switch] ChatInterval` / `[Integer] ChatInterval` 配置读取；未到间隔的 `CM_SAY` 帧**直接丢**、
`@` 开头不判间隔、被丢的帧**不刷新** `SayMsgTick`。判据：`rules.rs` 3 条单测 +
进程级 `say_interval_drops_the_second_chat_frame`（hello / hello-again / `@move` ⇒ 只有 2 条命令帧到 5000）。

**仍未做**（本轮的完整登记）：`SessionSpeedRule` 的非聊天预算（DelaySend 下照转，异常路径才可见）、
`Filters/*`、OTP、`GM_CHECKCLIENT` 周期帧、`ClientTimeOutTime`、`MaxClientPacketCount`/`NomClientPacketSize`
等上限计数（`:196-`）、`SendKickMsg` 的下行回踢文案。

## 8. 头号风险


- **客户端 face 有登录门禁（§7.6）**：未通过登录帧校验的客户端会被**剔除**；切片 1 目前会照转 `GM_DATA`
  ⇒ 这是**登记在案的差异**（不是等价），必须在切片 2 补上登录帧解析 + 失败剔除。
  ——**追记**：切片 2 已做「解析 + 失败剔除 + 登录帧不转发」；本轮又补上「登录通过后向 GameSvr 发**合成登录包**」
  （§7.11 的 Important #1）。缺这条时客户端侧"能进图"、GameSvr 侧却永远等不到登录——属于**不看 5000 侧就看不见**的假绿。
- **协议面是"两套"**：把 LoginGate 的 `ServerDataMessage` 套到 GameGate 上会全错（§5）。
- ~~**客户端面是 20B 头而不是裸 EDCode 流**（§2）⇒ `bins/login-gate` 的 `FrameSplitter` 不能照搬~~
  ——**已推翻（§7.12 实测）**：GameGate 客户端 face **没有**挂固定头适配器，就是裸 `#…!` 流；
  `bins/login-gate` 那套按 `#`…`!` 的思路**可以**借鉴（但注意 oracle 是"一次 read 一帧"、
  不跨 read 拼接，见 §7.12 的边界实测）。
- **最高风险：网关是"解码→处理→重编码"的加工点，不是中继**（§2 的 ⚠️）。
  ⇒ 切片 1 的判据必须先补一条"re-encode 字节恒等性"的实验（§7.2），否则"能进图"也可能是
  "改了字节但两端都容忍"的假等价。
  ——**追记（§7.9 查清）**：`EncryptUtil.Encode` 是**纯块码（无跨调用状态）**，"re-encode 字节恒等"对它成立；
  但命令帧的出帧**本来就不是输入字节**（12B 解码头 + 编码态尾部），所以"逐字节等于客户端发的帧"这个
  预期本身是错的——要比对的是**网关发往 5000 的帧**（对 C# 与 Rust 各自采集后归一化 `SessionId`）。
- **已知不可比对的一格**（§7.9）：有尾部命令帧的最后 1 字节，oracle 是 `BytePool.Rent` 出来的**未初始化**内存
  （`ClientSession.cs:657-668` 只写 `12+tLen` 字节却按 `12+tLen+1` 发送）。Rust 侧固定送 `0`；
  逐帧差分要对这一格做**归一化**（或断言服务端不读它），否则会得到永远不等的假红。
- **墙钟依赖的规则**（§7.10 + 7.10.1 的修正）：现网 `[Switch] MoveInterval=1` 等**全开**，但惩罚方式是
  **DelaySend**（`OverSpeedPunishMethod=3`，`GateShare.cs:198-216`）⇒ 超速操作**照转**（只把预算后推、
  不丢帧）⇒ **正常流量下"网关→5000 的帧集合"与客户端帧集合一致，字节级差分不需要虚拟钟**。
  墙钟只影响：① 预算后推（当帧不变）；② 持续超速触发踢人；③ 延迟队列/motaebo/CC 等**异常**路径
  ⇒ 差分优先做"正常流量 + DelaySend"，**异常/惩罚路径单开场景**（那里才需要虚拟钟或专门 pacing 驱动，
  照 `tests/parity/world` 桩 `HUtil32.GetTickCount()` 的做法）。
- 切片 1 若通过入口护栏，只能证明"**入口=Rust、世界=C#**"，不能当成 M3 的"游戏系统对齐"证据
  （与卡内 comment 的口径一致）。
