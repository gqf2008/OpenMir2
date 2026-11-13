# game-gate 客户端 face「一次 read 一帧」协议假设登记

- 卡片：`openmir2-game-gate-one-read-one-frame`（L 级，非阻塞）
- 判级理由：只登记协议假设与未来偏离边界，不改生产行为
- 来源：`opc-49` 2026-11-13 跨项目反馈；`mirclientzig-49` 现场（headless 客户端同拍 say+walk 连写被一次 read 合并）
- 范围：本文只登记事实与边界；不改 game-gate 生产代码、不改冻结客户端、不改协议
- 后续登记：卡 `openmir2-game-gate-defencecc-registration` 追加 A4「已知偏差」（`DefenceCCPacket` 缺口）

## 结论摘要

game-gate 的**客户端 face 没有分帧器**：oracle 的 `ServerService.Initialize()` 只把
`TcpService` 配好监听与容器，**不挂** `DataHandlingAdapter`，接收回调把一次交付的字节块整体
入队，`ProcessSessionPacket` 直接消费这份字节。因此「一次 read = 一帧」是照抄 oracle 的
**协议假设**，不是实现疏漏；帧被 TCP 拆开或合并的后果也与 oracle 一致（见 A1 后果一节）。
Rust 侧 `mir2-rs/bins/game-gate/src/lib.rs` 按同一假设实现，模块文档已把它写成显式事实。

## A1 协议假设与 oracle 证据

### A1.1 oracle：客户端 face 无分帧器

- `src/GameGate/Services/ServerService.cs` 的 `ServerService.Initialize()`：字段 `_serverSocket`
  是裸 `new TcpService()`，`Setup(...)` 只设置 `SetListenIPHosts` 与 `ConfigureContainer`，
  **没有调用 `SetTcpDataHandlingAdapter`**。对比同仓库其它 face（例如
  `src/GameGate/Services/ClientThread.cs` 的 `ClientThread.Initialize()` 对网关→GameSvr 的
  `TcpClient` 调用 `SetTcpDataHandlingAdapter(() => new PacketFixedHeaderDataHandlingAdapter())`），
  可见「客户端 face 无 adapter」是 oracle 的**局部事实**，不是全局能力缺失。
- 接收回调 `ServerSocketClientRead`：把 `e.ByteBlock.Len` 指明的**整块字节**拷进 `buff`，
  再构造 `ClientPacketMessage(serviceId, sessionId, buff, (ushort)buff.Length)` 入队。
  `ClientPacketMessage` 定义在 `src/GameGate/GateShare.cs`，其 `BuffLen` 就是这次交付的字节数。
  没有 adapter 时，这次交付就是一次 socket read 的结果（严格说是「一次 Received 事件的
  ByteBlock」，不保证等价于一个 TCP 段，见 A2.1）。
- 消费端 `ClientSession.ProcessSessionPacket`（`src/GameGate/Services/ClientSession.cs`）：
  `Span<byte> destinationSpan = messagePacket.Data.AsSpan()`、`packetLen = messagePacket.BuffLen`；
  `#…!` 剥壳写作 `destinationSpan[2..^1]`，随后把**整段剩余字节**交给
  `EncryptUtil.DecodeSpan(tempBuff, tempBuff.Length, ...)`。全程没有「按长度切下一帧」的循环、
  没有残留缓冲、也没有「本帧应有多长」的头部字段。
- 具体后果：`CM_SAY` 分支把 `decodeBuff[12..]` 当聊天正文，转发长度取 `deCodeLen`
  （= 这次 read 整段解码后的长度）。若一次 read 里合并了 `say` 与 `walk` 两帧，第二帧的
  `#1…` 头与 EDCode 字节会被并入 SAY 正文/转发尾部，GameSvr 侧看到**二进制尾巴**；
  同时 `walk` 帧不会再被单独分类，速度/间隔规则对它不生效。

### A1.2 Rust：同一假设的等价实现

`mir2-rs/bins/game-gate/src/lib.rs`：

- `client_loop` 的读循环每次 `read(&mut buf)` 得到 `n`，直接交给
  `handle_frame(&buf[..n], ...)`，没有跨 read 的重组缓冲。
- `classify_frame` 与 oracle 同构 —— 限定为**修掉登录前 3 字节偏差之后**（卡
  `openmir2-game-gate-3byte-prelogin`：登录前 `frame.len() <= 3` 一律 `Ignore`，与下面 A1.3 第 1
  条**末尾的登录前段落**一致——1–2 字节切片越界、3 字节空 span 抛 `ArgumentNullException`，
  两条异常都被吞 ⇒ 丢帧、会话存活；≥4 字节才进 `ClientLogin`。修复前的门是 `< 3`，3 字节会被
  当登录帧、被 `packetLen <= 15` 判失败而**关会话**，与 oracle 的「丢帧、会话存活」不等价）：
  `payload = &frame[2..frame.len() - 1]`
  剥壳后整体 `mir2_protocol::edcode::decode(payload)`；登录前的短帧、登录后的
  `< COMMAND_FIXED_LENGTH`（`GateShare.CommandFixedLength` = 16）、心跳（长度 1）等门都以
  **这一次 read 的总长度**为准。
- `rebuild_command_frame` 按 `decoded.len() > COMMAND_MESSAGE_SIZE`（= 12）决定是否带尾部，
  与 oracle 的转发口径一致。
- 注意：该文件读循环上方的注释「帧被 TCP 拆开/合并时的后果与 oracle 一致（都会被剔除）」
  里的「都会被剔除」**不准确**（合并见 A1.1；拆分见 A1.3 的三分支，其中 ≥19 字节会转发
  垃圾帧）。前一张卡（`openmir2-game-gate-one-read-one-frame`）写集只有文档；代码注释与
  3 字节偏差已由 `openmir2-game-gate-half-frame-comment` / `openmir2-game-gate-3byte-prelogin`
  两卡完成。

### A1.3 帧被拆分/合并时的 oracle 等价后果

1. **拆帧**（一个 `#…!` 跨两次 read）：**不会自动拼回**，两段各自按「一次 read 一帧」独立处理。
   半个帧的后果**由该片段自身长度决定**（`deCodeLen` 只由输入长度决定，解码对截断/非法流
   不报错、只产出垃圾字节），按长度分三支：
   - **片段 < 16 字节**（`packetLen < CommandFixedLength`，且长度 ≠ 1）：关会话
     （只有 `packetLen == 1` 才当心跳丢弃）。
   - **16–18 字节**：`tempBuff = destinationSpan[2..^1]` 长为 13–15，`deCodeLen` = 9/10/11
     `< 12` ⇒ 关会话。
   - **≥ 19 字节**：`deCodeLen ≥ 12`，两道门全过；截断的编码流被解成垃圾字节，
     `MemoryMarshal.Read<ClientMessage>` 取到垃圾 `Ident`，不命中任何 `case` 时顺序贯穿到
     switch 之后的转发代码（该 switch **没有 `default` 标签**；垃圾 `Ident` 是 16-bit 值，
     理论上有约 20/65536 的概率命中某个 `CM_*` case，届时走该分支规则，但不改变
     「未分类垃圾帧以 `GM_DATA` 转发」的结论），随后按 `deCodeLen` 重新编码并以 `GM_DATA` 垃圾帧
     **转发给 GameSvr**（`deCodeLen == 12` 转发 12 字节固定头；`> 12` 走 oracle 的 `Encode(decodeBuff, deCodeLen - 12)` + `tempBuff[16..]` 重编码分支）。
   后段（同一帧的剩余字节）会被下一次 read 当成新帧，`[2..^1]` 剥掉它自己的首 2 字节后
   再独立处理。**登录前**：片段 ≥ 4 字节走 `EDCode.DeCodeString` → `ClientLogin`，
   失败即 `Kick(1)` 剔除；≤ 3 字节的片段都到不了 `ClientLogin`（3 字节时 `[2..2]` 是空
   payload，`DeCodeString` 的 `IsEmpty` 检查抛 `ArgumentNullException`；1–2 字节时
   `[2..(packetLen-1)]` 越界），异常被 `ServerManager` 的 catch 吞掉 ⇒ 丢帧、会话存活。
2. **合帧**（两帧落进同一次 read，例如同拍连写 say+walk）：整段被当成**一帧**；剥壳只去
   首 `#1` 与尾 `!`，中间的第二帧头留在解码流里。后果见 A1.1 最后一条（SAY 吞帧头、
   后续帧绕过分类/限速）。
3. **长度门整体偏移**：`NomClientPacketSize`（默认 400）等以这次 read 的总长度为判据；
   合帧**可能**使总长越过阈值（典型 say+walk 只有数十字节，取决于合入帧的数量/大小），
   一旦越过，整段被丢（而不是只丢第二帧）。

## A2 客户端正确用法与未来偏离边界

### A2.1 当前客户端必须怎么写

- **一帧一次 write + flush**，且避免把两个逻辑帧写进同一个 TCP 段。
- 同拍多动作（例如 say+walk）之间要留间隔；现场 `mirclientzig-49` 用 **10ms** 间隔规避。
- 不要依赖 Nagle/内核合并「碰巧帮我们分帧」；也不要按固定 20B 头去读客户端 face
  （加头会被当成帧内垃圾前缀，实测 `Kicked 1`）。
- 验收观测点：服务端每个 `Received` 事件的字节块应恰好是一个 `#…!` 单帧。注意这是
  「一次交付 = 一帧」的工程近似，不是 TCP 段级保证；客户端仍应假设网络可能把两次写合并。

### A2.2 如果将来要改流式分帧（偏离 oracle）

任何在客户端 face 加 adapter、或按 `#…!` 主动切帧的改动，都属于**偏离 oracle**，
按项目规则需要偏差登记（`T-*` / `D-*`）并由 Leader 裁决；动手前至少完成以下 oracle
偏差评估与回归清单：

- **C2S 帧序列**：登录帧、心跳、命令帧的顺序与「是否被单独分类」在当前实现里由
  「一次 read 一帧」隐式决定；切帧后必须逐帧重放并比对帧序列。
- **速度规则**：`CM_WALK` / `CM_RUN`、攻击族、`CM_SPELL` 的 `tag`/间隔判定以帧边界为前提。
  当前合帧会让后续帧绕过限速；切帧后这些帧会重新受限速约束，**行为会变**，需对拍。
- **聊天/走位同拍连写**：切帧后 SAY 正文不再吞下一帧头，`ChatInterval`（现网 800ms 门）
  会对每帧单独生效，需要专门用例覆盖。
- **长度/CC 门**：`IsDefenceCCPacket`、`NomClientPacketSize` 的判据都基于一次 read 的总长；
  切帧后要重新定义「以帧长还是以 read 长判定」。
- **拆帧重组**：必须加重组缓冲与字节上限（否则是内存 DoS 面）、定义超时与丢弃策略，
  并证明不会改变现有帧语义。**现状不只是「不重组」：半包在 ≥19 字节时会被当完整帧转发给
  GameSvr（见 A1.3 第 1 条），所以重组同时是消除这条垃圾转发路径的前提。**
- **回归清单（建议）**：C2S 帧序列对拍；速度规则对拍；say+walk 同拍连写；跨 read 拆帧；
  心跳；登录失败/短帧路径。

## A3 范围与不做的事

- 本卡只新增本文件；**不改** game-gate 生产代码、**不改**冻结客户端/协议。
- 不把 headless 客户端的连写习惯当成服务端要迁就的需求：行为等价优先于修 bug；
  客户端侧的规避（逐帧 flush + 间隔）是当前唯一正确的用法。

## A4 已知偏差：Rust 未实现 oracle 的 `Config.IsDefenceCCPacket`（CC 包防御）

> 本小节即本条偏差的登记处（编号待 Leader 在偏差清单落号，建议 `D-*`）。现网
> `[Switch] DefenceCCPacket=0`，当前部署下两侧行为等价；开关一旦打开即分叉。

### oracle 侧（符号与行为）

- 开关字段：`src/GameGate/Conf/GateConfig.cs` 的 `GateConfig.IsDefenceCCPacket`（`bool`，
  构造函数缺省 `false`）；`src/GameGate/Conf/ConfigManager.cs` 用
  `ReadWriteBool("Switch", "DefenceCCPacket", default)` 读取（缺键/空值 ⇒ 取默认，不报错）。
- 消费点：`src/GameGate/Services/ClientSession.cs` 的 `ClientSession.ProcessSessionPacket`
  ——取出 `packetLen` 之后、`Stat`/登录态分支**之前**：

  ```csharp
  if (Config.IsDefenceCCPacket && (packetLen >= 5))
  {
      if (destinationSpan[..7].SequenceEqual(_httpSpan)) { KickFlag = false; return; }   // 整帧丢弃
      if (destinationSpan[..7].SequenceEqual(_httpsSpan)) { KickFlag = false; }         // 不 return
  }
  ```

- `_httpSpan = HUtil32.GetBytes("http://")`（7 字节）、`_httpsSpan = HUtil32.GetBytes("https://")`
  （8 字节）。两条判定都切 `destinationSpan[..7]`（7 字节），而 `Span.SequenceEqual` 长度不等
  直接返回 `false` ⇒ **https 分支永不命中**（死代码）；oracle 实际只拦 `http://` 前缀。
  登记按此事实，不要按注释/命名推测 https 也被拦。
- `destinationSpan` 是 `messagePacket.Data.AsSpan()`，`Data` 由
  `ServerService.ServerSocketClientRead` 按 `new byte[e.ByteBlock.Len]` 精确分配 ⇒
  `Data.Length == packetLen`。于是 `packetLen == 5/6` 时 `[..7]` 切片抛
  `ArgumentOutOfRangeException`，被 `ServerManager` 消息消费线程的 `catch (Exception)` 吞掉
  ⇒ 该帧被丢、**会话存活**（既未置 `KickFlag`，也不 `Kick`）。
- 命中 `http://` 的那条：置 `KickFlag = false` 后 `return` ⇒ 整帧丢弃、会话存活，**不**踢人。

### Rust 侧（缺失点）

- `mir2-rs/bins/game-gate/src/lib.rs` 的 `classify_frame` 在 `!logged_in` 分支之前没有任何 CC
  前缀判定；入参只有 `frame` / `logged_in` / 规则 / 长度上限，没有该开关。
- `mir2-rs/bins/game-gate/src/config.rs` 的 `GateConfig` **不解析** `[Switch] DefenceCCPacket`
  （只读 `ChatInterval`、`NomClientPacketSize`、`MaxItemSpeed` 与速度规则）。
- 全仓 grep `DefenceCC` 只命中 `mir2-rs/bins/game-gate/tests/fixtures/rungate-config.conf` 里的
  `DefenceCCPacket=0`——那是配置文本夹具（`config::load` 忽略该键），不是实现。

### 现网配置值

- `E:/MirServer/RunGate/config.conf` 的 `[Switch]` 段：`DefenceCCPacket=0`（2026-11-13 实
  `grep -n '^DefenceCCPacket='` 命中该行，值为 `0`）。与 `GateConfig()` 缺省 `false` 一致 ⇒ 当前部署下
  两侧都不做 CC 前缀拦截，行为等价。
- 核对命令（窗口期用它复核现网是否仍为 0）：
  `grep -n '^DefenceCCPacket=' E:/MirServer/RunGate/config.conf`

### 开关打开后的行为差异

| 输入（一次 read） | oracle（开关=1） | Rust（现状） | 差异 |
| --- | --- | --- | --- |
| 5–6 字节 | `[..7]` 越界抛异常被吞 ⇒ 丢帧、会话存活 | 登录前 ≥4 ⇒ `Login` → `handle_login` 失败 `Step::Close`；登录后 `< COMMAND_FIXED_LENGTH` ⇒ `Close` | 会话存活 vs **关会话** |
| ≥7 字节且以 `http://` 开头 | 整帧丢弃、会话存活 | 按普通帧：登录前进 `ClientLogin`（失败 `Kick(1)`）；登录后按长度门，≥19 字节的垃圾帧被以 `GM_DATA` 转发 | 丢弃 vs 处理/转发 |
| 以 `https://` 开头 | **不拦**（7 字节切片 vs 8 字节字面量恒 false，死分支） | 同左（现状本就不拦） | 无（bug-for-bug 等价） |
| 其余帧 | 原样进入登录/命令流程 | 同左 | 无 |

> 结论：开关打开时的主要差异是「5–6 字节短帧会不会关会话」与「`http://` 前缀帧会不会被丢弃
> 或转发」。现网 `=0` 时两条路径都不可达。

### 补齐前置（未来要支持时）

1. `config.rs`：`GateConfig` 增 `defence_cc_packet: bool`，用既有 `oracle_bool` 读
   `[Switch] DefenceCCPacket`，缺省 `false`（照 `GateConfig()` 构造函数）。
2. `lib.rs`：`classify_frame` 在 `!logged_in` 分支**之前**、与 oracle 同位置加判定：
   `frame.len() >= 5` 且首 7 字节 == `b"http://"` ⇒ `Classified::Drop`；`5/6` 字节 ⇒
   `Classified::Drop`（复刻「异常被吞 ⇒ 丢帧、会话存活」，**不得**退化成 `Close`）。
3. 死代码取舍：为保持 bug-for-bug 等价，**不要**顺手实现 `https://` 拦截；若要修 oracle 的
   死分支，单独走 `D-*` 偏差登记并写明 oracle 依据。
4. 测试（`classify_frame` 是纯函数，可直接断言）：开关关（现网）⇒ 既有用例逐条不变；
   开关开 + `http://` 前缀 ⇒ `Drop`；开关开 + 5/6 字节 ⇒ `Drop`（登录前/后各一例，且都不是
   `Close`）；开关开 + 普通帧 ⇒ 与开关关一致；负例把 `https://` 前缀钉成「开关开也照常处理」，
   防止顺手实现死分支。
5. 若要在真机上打开该开关，按 R 级另开卡走真机对拍窗口；本卡只登记，不改实现、不改配置。

## 实读核验结论（issue 描述 vs 代码）

- issue 描述与代码**一致**：`ServerService.Initialize()` 的 `TcpService` 未挂
  `DataHandlingAdapter`；`ServerSocketClientRead` 以一次交付的字节块构造
  `ClientPacketMessage`；`ProcessSessionPacket` 直接消费该字节块，无分帧/重组。
- 补充事实（issue 未写明，本文已登记）：同一 oracle 中网关→GameSvr 的 face
  （`ClientThread.Initialize()`）**有** `PacketFixedHeaderDataHandlingAdapter`；
  「无分帧器」只适用于客户端 face，未来讨论时不要误推广到整条链路。
- Rust 实现与注释同 oracle 对齐：`client_loop` / `handle_frame` / `classify_frame` /
  `rebuild_command_frame`。

## 参考文件（符号锚，不写行号）

- `src/GameGate/Services/ServerService.cs`：`ServerService.Initialize`、`ServerSocketClientRead`
- `src/GameGate/Services/ClientSession.cs`：`ProcessSessionPacket`、`CM_SAY` 分支
- `src/GameGate/GateShare.cs`：`ClientPacketMessage`、`CommandFixedLength`
- `src/GameGate/Services/ClientThread.cs`：`ClientThread.Initialize`（网关→GameSvr 带 adapter）
- `mir2-rs/bins/game-gate/src/lib.rs`：`client_loop`、`handle_frame`、`classify_frame`、
  `rebuild_command_frame`、模块文档
- `src/GameGate/Conf/GateConfig.cs`：`IsDefenceCCPacket`（构造函数缺省值）
- `src/GameGate/Conf/ConfigManager.cs`：`ReadWriteBool("Switch", "DefenceCCPacket", ...)`
- `mir2-rs/bins/game-gate/src/config.rs`：`GateConfig`、`oracle_bool`
