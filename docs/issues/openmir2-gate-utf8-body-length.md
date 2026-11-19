# openmir2-gate-utf8-body-length

> 状态：**实现完成（2026-11-19，分支 `card/gate-utf8-body-length`）**——证据为**跨栈 golden**
> （opc-07 裁定够当本卡证据；真机只能覆盖 ASCII IP，证明不了非 ASCII 缺陷，故不做）。
> **本卡不改变现网行为（调用点全为 ASCII IP），属防御性正确化。**
> 已登记（walgit 看板不可用，按 AGENTS.md §7 过渡）。
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
（原卡手抄计数已删除）计数以机器产物为准：`E:/UserFleet/evidence-cardfix-20261119/utf8-test.log`（`dotnet test TestProject1/GameSrvTest.csproj --filter FullyQualifiedName~GateUtf8BodyLength`）。Rust 侧：`mir2-rs/bins/game-gate/src/lib.rs` 的 `control_frame_golden_matches_csharp_oracle`（`include_str!` 读同一批 golden，逐字节断言）。
```
1. `EmptyBody_IsHeaderOnly`：空正文 ⇒ 20B、`PackLength=0`；
2. `AsciiBody_MatchesByteLengthBaseline`：ASCII（IP 形态）帧长/`PackLength`/正文与字节口径一致
   （证明修复不破坏现网行为）；
3. `ChineseBody_IsNotTruncated_AndPackLengthIsByteCount`：「中文角色」帧长 = 20+8、正文逐字节等于
   GB2312、`PackLength=8`；**负例对照**断言"不等于 20+字符数 / 不等于字符数"（换回 `data.Length` 即红）；
4. `FrameBuilder_UsesByteLengthNotCharLength`：接线牙，`BuildServerFrame` 区域内出现 `data.Length`
   当长度 ⇒ 红。
   （测试进程需 `Encoding.RegisterProvider(CodePagesEncodingProvider.Instance)`，与各服务 `Program.cs` 同口径。）

### 待办（R 级证据）——先回答"证据从哪里取"

**判断（先写死口径）**：Rust 侧本来就是字节口径，这份证据的价值是
**"证明 C# 修完后与 Rust 逐字节一致"**，**不是**"证明 Rust 有 bug"。后来人不得把它读成 Rust 有问题。

**现实约束**：m1-stack 跑的是 **Rust 三网关**，C# `GameGate` 不在该栈里；且生产今天唯一进入
`SendServerMsg` 的非空正文是 `UserEnter` 的 **IP（ASCII）**。因此：

1. **桌面跨栈 golden（已实现，无需任何栈/窗口）**：
   - C#：`TestProject1` 用**生产函数** `ClientThread.BuildServerFrame` 对样本表
     `[空, "127.0.0.1", "中文角色", "热血传奇", 混合串]` 产出十六进制帧 → golden
     （C# 是 oracle，golden 由它生成）；
   - Rust：`game-gate` 测试用 `ServerMessage::to_bytes` + `gbk::encode`（模块注释：逐字节等于
     .NET cp936）拼同构帧，`include_str!` 读同一批 golden，逐字节断言相等；
   - 进程清单/端口/MySQL/客户端：**都不需要**（纯函数对拍）。
   - **已落地**：`TestProject1/GateUtf8FrameGolden`（C# 产/断言 golden）+
     `game-gate` 的 `control_frame_golden_matches_csharp_oracle`（Rust 逐字节断言同一份 golden）；
     golden 文件 `mir2-rs/tests/parity/golden/gate_utf8_frame/gate_control_frames.txt`，
     5 个样本（空 / IP / 中文 / 中文2 / 混合），含中文样本的 GB2312 字节。原文：
```
dotnet test --filter GateUtf8FrameGolden → 通过 1 / 失败 0
cargo test -p game-gate → 89 passed / 0 failed（含 golden）；
  另一 test binary 26 passed / 0 failed
```
     这是**离线可重复**的"两侧字节一致"证据。
   - **同一份文件、生产函数产出**（验收①）：C# 由 `ClientThread.BuildServerFrame` 写，
     Rust `include_str!` 读同一个路径；中间无手工转写、无第二份常量。
   - **两侧独立构造**（验收③）：C# 走 `BuildServerFrame`（`HUtil32.GetBytes` + `SerializerUtil`），
     Rust 走 `ServerMessage::to_bytes` + `gbk::encode`；**不共享编码辅助函数**。
   - **生成幂等**（验收②）：连跑两次 `GATE_UTF8_GOLDEN_WRITE=1`，文件 sha256 三次一致：
     `4f5fa84013a9178314bc010b26a7ff6ad93d7e39d920ba9b950f5e02204430df`（committed = write1 = write2）。
   - **变异牙（验收④，原文）**：把 `BuildServerFrame` 改回字符数口径（`PackLength`/缓冲/拷贝全用
     `data.Length`）——
     - 牙①（正确 golden + 变异实现）：`golden_matches_or_is_regenerated` 红，
       `But was: "...a9aaa04000000030001000000000004000000d6d0cec4..."`（PackLength=4、只拷 4/8 字节）；
     - 牙②（用变异实现重产 golden 后跑 Rust）：`control_frame_golden_matches_csharp_oracle` 红，
       `样本 chinese 与 C# oracle 不一致`（`test result: FAILED`）。
     - 还原后：源文件 sha 与 golden sha 均复原（golden=`4f5fa840…`），两侧复跑全绿。
2. **真机抽证（可选）**：m1-stack 已在跑 Rust 网关，用现成捕获（`tools/gw-capture`）在 5000 侧抓
   `GM_OPEN` 帧（ASCII IP 正文），与第 1 级同 IP 样本的 golden 逐字节比。不需要额外起
   MySQL/DBSrv，也不需要真实客户端（一个 bot/客户端连接即可触发 `UserEnter`）。
3. **非 ASCII 的"真机"证据不可得**：今天没有可达入口把中文送进 `SendServerMsg`；中文样本只能由
   第 1 级的合成输入证明。**把它写清楚，比伪造一个窗口诚实。**

- **证据到位前不合并**（opc-07 约束：先补票后上车）。

### 范围核查

- 同族：`SendLoginMessage` 已按编码后字节数（`encodeLen + 3`）；Rust 侧字节口径；未发现第三组件。

## QA 整改（2026-11-19）

> 统一口径：**卡面不再手抄测试数字**，一律引用机器产物路径；状态写"已合并（<sha>）"。

- 机器产物：`E:/UserFleet/evidence-cardfix-20261119/`（`packlen-test.log`、`utf8-test.log`、`deps-check-and-selftest.log`）；
- 实跑命令与 `git status --porcelain` 原文：见上述日志（本次采集时 `git status --porcelain` = 0 行）；
- **源码文本/`grep` 型断言**：仅作"接线提示"，**不作行为证据**（QA 章程 §12）；行为判据以
  实跑帧/JSON 产物为准。
> 来源：QA 签署摘要 `docs/issues/qa-merged-cards-signoff-20261119.md`。

### 可独立重放的篡改（M1–M3）

| 编号 | 改什么 | 怎么改 | 期望红在哪 | 还原 |
|---|---|---|---|---|
| M1 | C# `BuildServerFrame` | `PackLength = strBuff.Length` 改成 `data.Length` | golden 行为牙红（输出 `PackLength=4`、只拷 4/8 字节） | 复原后全绿 |
| M2 | golden `chinese` 行 | `PackLength` 8→9 | `panicked at lib.rs:2241: 样本 chinese 与 C# oracle 不一致` | 复原后全绿 |
| M3 | golden `mixed` 行 | 删掉该行 | `panicked at lib.rs:2244: golden 必须覆盖 5 个样本 left: 4 right: 5` | 复原后全绿 |
