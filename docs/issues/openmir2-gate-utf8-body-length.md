# openmir2-gate-utf8-body-length

> 状态：**实现中（2026-11-19，分支 `card/gate-utf8-body-length`）**——C# 修复与单测已完成；
> **R 级窗口对拍证据到位前不合并**（opc-07 约束）。已登记（walgit 看板不可用，按 AGENTS.md §7 过渡）。
> 发现于卡 `openmir2-db-write-stall` 的 44 处 `MemoryCopy.BlockCopy` 逐条审计（C 类）。

## 现象
C# `GameGate` 的 `SendServerMsg`：正文按**字符数**而不是**字节数**处理——
```
byte[] strBuff = HUtil32.GetBytes(data);                       // 编码后的字节
byte[] tempBuff = new byte[ServerMessage.PacketSize + data.Length];  // 按字符数开缓冲
MemoryCopy.BlockCopy(strBuff, 0, tempBuff, sendBuffer.Length, data.Length);  // 拷贝 data.Length 字节
```

## 影响
`HUtil32.GetBytes` 为非 ASCII 内容产出多于字符数的字节（中文典型 2 字节/字符）⇒
**正文被截断**（拷贝量只有实际字节数的一部分）。这是功能缺陷，不是性能问题。

## 双重疑点（同卡一并核）
同一函数里 `ServerMessage.PackLength = nLen` 由**调用方**传入；需要核 `nLen` 与
`tempBuff` 实际长度、与 `GetBytes(data).Length` 三者是否一致。若不一致，即便拷贝量修好，
包头声明的长度仍会与实际正文不符，客户端侧会再次出问题。

## 建议改动（两侧同卡，R 级 + 窗口对拍）
1. 先用**中文样本**把现状固定成红/绿记录（C# 现有行为 + Rust 现状），避免"改完不知道原来是什么"。
2. C#：缓冲与拷贝长度一律取 `strBuff.Length`；`PackLength` 与实际正文长度对齐。
3. Rust `game-gate`：C# 是 oracle。两侧同改，或登记 `D-*` 偏差（只改 C# 会让对拍红）。
4. 窗口对拍：中文角色名/中文聊天样本的逐字节对拍（R 级证据）。

## 关联
- 根因卡：`openmir2-db-write-stall`
- 同批审计：`openmir2-gate-packlen-validation`

## 实现与验收（2026-11-19）

### 现状基线（改前记录）

- **C# 旧实现**（`GameGate/Services/ClientThread.cs` 的 `SendServerMsg`）：`strBuff = HUtil32.GetBytes(data)`
  （GB2312），但缓冲 `new byte[20 + data.Length]`、拷贝量 `data.Length`、`PackLength = nLen`
  （调用方传 `data.Length`）全部是**字符数**。以 "中文角色"（4 字符 / GB2312 8 字节）为例：
  旧帧长 24B、只拷 4/8 字节、`PackLength=4`（应为 8）。
- **生产可见性**：`UserEnter` 的全部调用点传的是 `sRemoteAddress`（IP，ASCII，字符数==字节数）
  ⇒ 今天**不可观测**；这是潜在缺陷（任何非 ASCII 控制正文都会截断/声明错）。
- **Rust 现状**：`game-gate` 的 `GM_OPEN` 用 `pack_length = ip_bytes.len()`（字节口径）；
  客户端→服务端方向的 `SendLoginMessage` 用 `PackLength = encodeLen + 3`（编码后字节数）
  ⇒ **Rust 已对齐，无需改动**（登记为"已有（字节口径）"）。

### 改动（C#）

- `SendServerMsg` 抽出**纯函数** `ClientThread.BuildServerFrame(command, sessionId, nSocket, userIndex, data)`：
  `PackLength`、缓冲长度、拷贝量一律取 `strBuff.Length`（GB2312 字节数）；空正文 ⇒ 只发 20B 头。
- 私有 `SendServerMsg` 去掉 `nLen` 参数（长度不再由调用方按字符数传入），4 个调用点同步更新。
- 公开 `UserEnter(sessionId, socketId, data)` 签名不变。

### 牙（4 条，原文）

```
dotnet test --filter FullyQualifiedName~GateUtf8BodyLengthTest
已通过! - 失败: 0，通过: 4，已跳过: 0，总计: 4
```
1. `EmptyBody_IsHeaderOnly`：空正文 ⇒ 20B、`PackLength=0`；
2. `AsciiBody_MatchesByteLengthBaseline`：ASCII（IP 形态）帧长/`PackLength`/正文与字节口径一致
   （证明修复不破坏现网行为）；
3. `ChineseBody_IsNotTruncated_AndPackLengthIsByteCount`：「中文角色」帧长 = 20+8、正文逐字节等于
   GB2312、`PackLength=8`；**负例对照**断言"不等于 20+字符数 / 不等于字符数"（换回 `data.Length` 即红）；
4. `FrameBuilder_UsesByteLengthNotCharLength`：接线牙，`BuildServerFrame` 区域内出现 `data.Length`
   当长度 ⇒ 红。
   （测试进程需 `Encoding.RegisterProvider(CodePagesEncodingProvider.Instance)`，与各服务 `Program.cs` 同口径。）

### 待办（R 级证据）

- 窗口对拍：中文角色名/中文聊天样本的**逐字节**对拍（C# 与 Rust 同源构建）。
- **证据到位前不合并**（opc-07 约束：先补票后上车）。

### 范围核查

- 同族：`SendLoginMessage` 已按编码后字节数（`encodeLen + 3`）；Rust 侧字节口径；未发现第三组件。
