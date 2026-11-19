# openmir2-gate-packlen-validation

> 状态：**已合并（`b8ba0da6f`）**——C# 修复 + Rust 对齐；见文末「实现与验收」与「QA 整改」。
> 已登记（本仓库 remote 无 `refs/collab/*`，walgit 看板不可用；按 AGENTS.md §7 「tracker 未就绪时
> 先落 `docs/issues/`」过渡）。看板恢复后迁移为正式卡。
> 发现于卡 `openmir2-db-write-stall` 的 44 处 `MemoryCopy.BlockCopy` 逐条审计（E 类）。

## 现象
C# `GameGate` 的会话消息路径里，`packetLen` 直接取自**网络包头**（`packetHeader.PackLength` 的绝对值），
随后作为 `MemoryCopy.BlockCopy(data, 0, sendMsg.Buffer, 0, packetLen)` 的 count。
源 `data` 的长度没有任何显式校验；`MemoryCopy` 内部是 `Buffer.MemoryCopy`（零边界检查）。

## 风险
- 修复前：若 `packetLen` 大于 `data` 实际长度 ⇒ **越界读**；大值时跨出 GC 段 ⇒ 访问违例 ⇒
  CLR 致命错误 `0x80131506` ⇒ 卡在 `NtRaiseHardError` 报告路径（与
  `dbsrv-sendmessage-oob-root-cause` 同机理）。
- 修复后（`MemoryCopy.CheckRange` 上线）：越界变为 `ArgumentOutOfRangeException`，但抛在
  TouchSocket 的收包回调里 ⇒ **该连接停读**。对网关而言，不可信输入不应走到「抛异常」这条路上。

## 建议改动（两侧同卡）
1. C# 侧：拷贝前显式校验 `packetLen >= 0 && packetLen <= data.Length`；不满足 ⇒ 记 `WRN`（含包头字段）
   并**关闭该连接**（拒绝并断开），不进入拷贝。
2. 负例用例：`packetLen < 0`、`packetLen > data.Length`、`packetLen` 极大（如 `int.MaxValue`）各一条。
3. Rust `game-gate` 侧：C# `GameGate` 是该组件的 oracle。若 Rust 已有等价校验，登记为已有；
   若没有，同 PR 对齐；若有意不同，须登记 `D-*` 偏差并说明。
4. 审计同族：`SelGate`/`LoginGate` 的接收路径是否有同类「包头长度当 count」的写法。

## 复现
离线：构造一帧 `PackLength` 大于实际负载的会话消息（不需要真客户端），看 C# 侧行为：
修复前为 AV/卡死，修复后为异常中断接收。

## 关联
- 根因卡：`openmir2-db-write-stall`
- 同批审计：`openmir2-gate-utf8-body-length`（中文正文截断）

## 实现与验收（2026-11-19）

### 改动（C#）

- 新增 `src/OpenMir2/GatePacketLength.cs`：`TryValidate(packetLen, available, out reason)`，
  `packetLen >= 0 && packetLen <= available` 才通过；失败返回含原值的原因串。
- `src/GameGate/Services/ClientThread.cs`（`ProcessServerPacket` 的 `GM_DATA`）：分配/拷贝前校验；
  失败 ⇒ `LogService.Warn`（含 raw `PackLength`）+ `_clientSocket.Close()`，函数返回，不进入
  `new byte[]` / `MemoryCopy.BlockCopy`。
- `src/GameGate/Services/ClientSession.cs`（`ProcessSessionPacket`）：任何切片之前校验
  `BuffLen <= Data.Length`（失败 ⇒ WRN + 关会话）；HTTP/HTTPS 探测切片 `[..7]` 的守卫由
  `packetLen >= 5` 改为 `>= 7`（原写法在 `BuffLen` 为 5/6 时会抛切片越界）。
- `src/OpenMir2/DataHandlingAdapters/DataMessageFixedHeaderRequestInfo.cs`：`PackLength == int.MinValue`
  时 `-PackLength` 仍是负值 ⇒ 按 0 长度交帧，由上面的显式校验处置；负 `bodyLength` 不进入框架。

### 负例三条（原文）

`dotnet test TestProject1/GameSrvTest.csproj --filter FullyQualifiedName~GatePacketLengthTest`：

```
（原卡手抄计数已删除）计数以机器产物为准：`E:/UserFleet/evidence-cardfix-20261119/packlen-test.log`（`dotnet test TestProject1/GameSrvTest.csproj --filter FullyQualifiedName~GatePacketLength`）。Rust 侧接线测试：`mir2-rs/bins/game-gate/src/lib.rs` 的 `upstream_body_bound_rejects_pathological_lengths`（`MAX_UPSTREAM_BODY` 上界与边界值）。
```

- `Rejects_NegativeLength_beforeAnyCopy`：`packetLen=-1` / `int.MinValue` ⇒ false，原因含原值；
- `Rejects_LengthGreaterThanAvailable`：`packetLen=21 > available=20` ⇒ false；
- `Rejects_IntMaxValue`：`packetLen=2147483647` ⇒ false（先于 `new byte[]`）；
- `Accepts_LengthEqualToAvailable`：`0/0`、`20/20` ⇒ true；
- 接线牙两颗：GM_DATA 的 `TryValidate` 与 `_clientSocket.Close()` 必须在 `BlockCopy` 之前；
  客户端封包的 `BuffLen > Data.Length` 检查必须在任何切片之前。

### 同族扫描（C# 三网关 + Rust 三网关）

| # | 位置（文件/符号） | 用法 | 判定 | 理由 |
|---|---|---|---|---|
| 1 | `src/GameGate/Services/ClientThread.cs` `ProcessServerPacket` GM_DATA | `|PackLength|` → `new byte[]` + `BlockCopy` | **改** | 本卡主修复点 |
| 2 | `src/GameGate/Services/ClientSession.cs` `ProcessSessionPacket` | `BuffLen` → 多处切片 | **改** | 长度契约 + `[..7]` 守卫 5→7 |
| 3 | `src/OpenMir2/DataHandlingAdapters/DataMessageFixedHeaderRequestInfo.cs` `OnParsingHeader` | `-PackLength` 得负 `bodyLength` | **改** | `int.MinValue` 变体按 0 长度交帧 |
| 4 | `src/GameGate/Services/ClientSession.cs` 其余 `MemoryCopy.BlockCopy` | count 来自解码器输出/常量/数组自身长度 | 不改 | 非包头字段 |
| 5 | `src/GameGate/Services/ServerService.cs` `ServerSocketClientRead` | count = `ByteBlock.Len`（实测长度） | 不改 | 非包头声明值 |
| 6 | `src/GameGate/Services/CloudClient.cs`（`Math.Abs(PackLength)`、`new byte[PackLength]`） | `Math.Abs(int.MinValue)` 会抛 `OverflowException` | 不改（**未接线**：全仓无实例化） | 死代码；将来接线前必须先补 `TryAbs`/长度校验（本卡不动死代码，避免"改了但没人跑"的假绿） |
| 7 | `src/LoginGate/Services/ClientThread.cs` `ClientSocketRead` | count = `ByteBlock.Len` | 不改 | 实测长度 |
| 8 | `src/LoginGate/Services/ClientThread.cs` `ProcessServerData` | `nCheckMsgLen = |PacketLen+6|`；`> nLen ⇒ break` | 不改 | `PacketLen` 是 `ushort`（无溢出）；流式重组"等后续字节"是正确语义，改关连接会误杀正常分片 |
| 9 | 同上 `Deserialize(dataBuff[FixedHeaderLen..])` | span 到缓冲尾 | 不改 | 不越界（限数组内），MemoryPack 只读所需字段；按声明长度切片属协议语义，须另卡验证 |
| 10 | `src/LoginGate` / `src/SelGate` 其余 `MemoryCopy.BlockCopy` | count = `dataBuff.Length`/数组长度 | 不改 | 非包头字段 |
| 11 | `src/SelGate/Services/ClientThread.cs` | 同 #7–#10 | 不改 | 同上 |
| 12 | `mir2-rs/bins/game-gate/src/lib.rs` `reader` | `vec![0u8; body_len]`（`body_len = |PackLength|`） | **改** | 流式 `read_exact` 不越界，但畸形头会巨量分配（`i32::MIN` ⇒ 2 GiB）；加上界 + WRN + 断链 |
| 13 | `mir2-rs/crates/protocol/src/frame.rs` `ServerMessage::body_len` | 纯函数 | 不改 | 校验放在调用点（#12） |
| 14 | `mir2-rs/bins/login-gate/src/lib.rs` body 分配 | `usize::from(u16)` | 不改 | 构造性上界 64 KiB |
| 15 | `mir2-rs/bins/sel-gate` | 无 header-driven 分配 | 不改 | 读缓冲固定 8 KiB |

### 偏差登记

- **D-gate-packlen-upstream-cap**（Rust）：`MAX_UPSTREAM_BODY = 1 MiB`。C# oracle 不设上界；
  本实现拒绝畸形帧（`PackLength` 的绝对值超 1 MiB），正常 `GM_DATA` 体远小于此，行为等价面不变。
  理由：oracle 的"无上界"在 Rust 里表现为尝试分配同等内存（`vec![0u8; n]`，失败即 abort），
  与 C# 的 OOM 同族；防御上界是安全侧收敛。

### 两层语义：帧层 vs 判定层（复审 ①）

- **帧层**（`DataMessageFixedHeaderRequestInfo.OnParsingHeader`）只能交帧——TouchSocket 的固定头
  适配器没有"拒绝并关连接"的出口；它把 `int.MinValue` 降级为 **0 字节交帧**，**不是放行**。
- **判定层**（`ClientThread.TryBuildServerSessionMessage`，GM_DATA 的唯一判定点）负责放行/拒绝；
  失败 ⇒ `WRN` + 关闭上游连接、不投递。两层共用 `GatePacketLength.TryMagnitude`（单一真源），
  不会对同一字段得出相反结论。
- 端到端断言（驱动**生产判定函数**本身）：`EngineFrame_MinValue_IsRejectedByTheProcessor`、
  `EngineFrame_Oversize_IsRejectedByTheProcessor`、`EngineFrame_Valid_IsAcceptedAndCopiedWhole`；
  接线牙 `GameGate_EngineFrame_UsesDecisionPointAndClosesOnFailure`（删判定、挪关闭、或分支内再出现
  裸 `new byte[`/`MemoryCopy.BlockCopy` ⇒ 红）。
- 协议语义：`PackLength` **负值合法**（"小包/已编码"，绝对值是长度）；"负 count"只可能来自
  `int.MinValue`（`-MinValue` 在 unchecked 下仍是负数），这正是要拦的那一例。

### Rust 上界依据与断链理由（复审 ②）

- oracle（C#）**没有定义协议上限**；从代码能推出最硬的界：
  - LoginGate/SelGate ↔ 服务端链路：`ServerDataPacket.PacketLen: ushort` ⇒ **64 KiB**
    （`crates/protocol/src/frame.rs` / `src/OpenMir2/Packets/ServerPackets/ServerDataMessage.cs`）；
  - 客户端→网关：`MaxClientPacketSize` 默认 **10,240 B**（`src/GameGate/Conf/GateConfig.cs`）；
  - 最大合法服务端消息的构成量级：`MaxBagItem = 46`（`src/OpenMir2/Grobal2.cs`）等小常量。
  - 取 **1 MiB = 64 KiB × 16**：高于最硬上界 16 倍、高于客户端上限 100 倍，合法帧不可能接近。
- **为什么断链而不是丢帧**：声明长度超过上界后，要跳过该帧必须按这个**不可信长度**读/丢弃字节
  （最多 2 GiB），读完后也无法确认下一个头的位置 ⇒ 流无法安全再对齐。断链让监督循环重建干净
  链路，是唯一能保证"后续帧不被错位解析"的处置。WRN 行内带累计拒绝次数（`upstream_len_reject_count`）。

### CloudClient 检索（复审 ③，不是 grep 猜）

```
grep -rn "new CloudClient" src --include=*.cs                                   → 无输出（rc=1）
grep -rn "CloudClient" src --include=*.cs | grep -v Services/CloudClient.cs     → 无输出（rc=1）
grep -rn "Activator.CreateInstance|GetType(|AddSingleton|AddTransient|RegisterType" src/GameGate --include=*.cs → 无输出（rc=1）
```
⇒ 无直接构造、无引用、无反射/DI 注册；判定"不改（未接线）"，将来接线前必须先补 `TryAbs`/校验。

### D-* 落点

- 本仓没有 D-* 的登记门（`tools/whitelist` 只覆盖 T-*）；按 opc-07 指示**只落卡面**
  「偏差登记」小节，并注明"待 whitelist 支持 D-* 后迁移"。不另造新机制。

### 合并前门禁

- C#：`dotnet build` 三网关工程 0 错误；`dotnet test TestProject1` 全量；
- Rust：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo test --workspace`（三元组）、`msgcodegen --check`、anchor gate、工具门 TOTAL（集合+总数）；
- 合并：`--no-ff`，提交信息带本卡与三条负例结果。

### 边界

不碰 AOI、不碰 DBSrv、不改背压参数（`max_depth_frames` 等属另一张卡）。

## QA 整改（2026-11-19）

> 统一口径：**卡面不再手抄测试数字**，一律引用机器产物路径；状态写"已合并（<sha>）"。

- 机器产物：`E:/UserFleet/evidence-cardfix-20261119/`（`packlen-test.log`、`utf8-test.log`、`deps-check-and-selftest.log`、`packlen-rust-test.log`、`RUNS.txt`）；
- **Rust 侧测试**：`mir2-rs/bins/game-gate/src/lib.rs` 的 `tests::upstream_body_bound_rejects_pathological_lengths`（符号锚；高 churn 文件不写行号）；**原文见实跑日志** `E:/UserFleet/evidence-cardfix-20261119/packlen-rust-test.log`（卡面不再手抄代码块）。
- **构建命令 + `git status --porcelain` 的测量命令与输出**：`E:/UserFleet/evidence-cardfix-20261119/RUNS.txt`（卡面不写行数）；
- 实跑命令与 `git status --porcelain` 的测量命令：见上述日志（卡面不写行数）；
- **源码文本/`grep` 型断言**：仅作"接线提示"，**不作行为证据**（QA 章程 §12）；行为判据以
  实跑帧/JSON 产物为准。
> 来源：QA 签署摘要 —— 公共路径 `E:/UserFleet/qa/qa-merged-cards-signoff-20261119.md`。

### 可独立重放的篡改（T1–T4）

| 编号 | 改什么 | 怎么改 | 期望红在哪 | 还原 |
|---|---|---|---|---|
| T1 | `TryValidate` 的长度判断 | `if (packetLen > available)` → `if (false && packetLen > available)` | 4 条红：`Accepts_LegalNegativePackLength` / `EngineFrame_Oversize_IsRejectedByTheProcessor` / `Rejects_IntMaxValue` / `Rejects_LengthGreaterThanAvailable` | 复原后全绿 |
| T2 | `int.MinValue` 拒绝支 | `if (rawPackLength == int.MinValue)` → `if (false && rawPackLength == int.MinValue)` | 3 条红：`EngineFrame_MinValue_…` / `Magnitude_UsesSingleSourceOfTruth` / `Rejects_NegativeMagnitude_beforeAnyCopy` | 复原后全绿 |
| T3 | GM_DATA 失败分支 | 删掉 `_clientSocket.Close()` | `GameGate_EngineFrame_UsesDecisionPointAndClosesOnFailure` 红 | 复原后全绿 |
| T4 | Rust 上界 | `len <= MAX_UPSTREAM_BODY` 改成恒真 | `panicked at bins/game-gate/src/lib.rs:2179: assertion failed: !upstream_body_allowed(MAX_UPSTREAM_BODY + 1)` | 复原后全绿 |
### 统一整改（C4–C6）

- **C4 `built_at`（裁定正文，引用不指向未合并分支）**：戳的 `built_at` 必须等于**二进制自报**的
  `built_at`（单一来源）；文件 mtime 只作元数据、**不参与比对**；闸门断言"自报 == 戳"。
  M9 四件因自报与戳相差 5 秒判 `CONDITIONAL`（身份可证、语义待统一），不算 FAIL。
- **C5 `SHA256SUMS`（已补实）**：两份冻结目录 `E:/UserFleet/release-csharp-m9-572a0b8a/` 与
  `E:/UserFleet/release-m9-572a0b8a/` 均为双清单；`sha256sum -c` 的结果与 rc **见验证日志**
  `E:/UserFleet/evidence-cardfix-20261119/sums-verify.log`（卡面不写行数/OK 数）；历史
  `SHA256SUMS.txt` 不改写；用途差异见各自 `PROVENANCE.txt` 的 `sums_files=` 字段。
- **C3 构建命令与仓库状态（已补实）**：两目录的 `PROVENANCE.txt` 均含 `build_cmd=` / `build_cwd=` /
  `toolchain=` / `git_status_porcelain_measure=` 字段（**字段值照抄，卡面不写数字**）；
  C# 侧实跑命令与仓库状态见 `E:/UserFleet/evidence-cardfix-20261119/RUNS.txt`。
- **C6 章程引用**：权威 QA 章程在 **`E:\OPC\docs\qa-charter.md`（§10–§12）**，不在本仓库；
  卡面凡引"章程 §N"均以此文件为准。
