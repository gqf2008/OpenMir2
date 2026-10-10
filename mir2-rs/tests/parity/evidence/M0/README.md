# M0 静态层验收证据（A 线 · 协议）

日期：2026-10-10 ｜ 分支：worktree-m0-protocol ｜ 环境：Windows 10 / cargo 1.98.1 / dotnet 9.0.313

> **范围声明**：M0 验收①②③的**最终判据**是 C 线的金标准抓包（`tests/golden/`，尚待交付）。
> 本批证据是 oracle 层（C# 真身 `EDCode/EncryptUtil/SerializerUtil` 生成向量）的静态对拍，
> 判据逻辑、门禁工具、红检三连均已就绪并与抓包格式通用——抓包到位后执行
> `cargo run -p replay -- tests/golden/<capture>.jsonl` 即可完成最终验收。
> 本层没有自造样本当判据：全部期望字节由 `tests/parity/oracle`（引用 `src/OpenMir2` 真身编译）产出。

## 判据执行结果

| 验收 | 结果 | 证据 |
|---|---|---|
| ① decode→encode 逐字节相同 | **0 差异**（N=1143：1072 编解码 + 46 帧 + 6 条 s2c 双帧尾 + 3 纯字符串帧 + 1 内部帧头 + **15 条内部帧 MemoryPack（M1 用）**） | `green.log` |
| ② (ident,Recog,param,tag,series,body 长度,body hash) 字段对拍 | **0 差异**（SHA-256 跨语言对拍；纯字符串帧只对拍 body 长度/hash） | `green.log` |
| ③ 八阶段覆盖 + 未实现包号=0 | 八阶段全覆盖（登录3/选服2/选角2/建角2/进世界4/移动3/攻击3/小退2），未实现=0 | `green.log` |
| 红检① EDCode 密钥错 1 位 | **必红**（exit 1：字节与字段同时差异） | `redcheck1-key.log` |
| 红检② 包号 +1 | **必红**（exit 1：字段对拍差异） | `redcheck2-ident.log` |
| 红检③ body 截 1 字节 | **必红**（exit 1：解析失败/字节差异/长度差异） | `redcheck3-truncate.log` |
| 红检④ s2c 帧尾被改（login 跳去 `$` / game 跳加 `$`） | **必红**（exit 1：② 报「帧尾与 hop/tail 期望不符」2 条；① 恒 0——自洽往返对帧尾天然不可见，这正是需要 `hop` 独立期望的原因） | `redcheck4-s2c-tail.log` |

注：③在 oracle 向量上跑通证明门禁逻辑可用；抓包到位后以抓包为准重跑。

## 复现命令

```powershell
cd mir2-rs
# C# oracle 生成向量（需 dotnet 8+，引用真身 src/OpenMir2）
dotnet run -c Release --project tests/parity/oracle
# Rust 侧门禁
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                 # 11 个测试（含 1119 条向量对拍）
cargo run -p msgcodegen -- --check                     # 消息号表与 C# 源零漂移
cargo run -p replay -- tests/parity/vectors/oracle_vectors.jsonl            # GREEN, exit 0
cargo run -p replay -- tests/parity/vectors/oracle_vectors.jsonl --no-coverage --sabotage key      # RED, exit 1
cargo run -p replay -- tests/parity/vectors/oracle_vectors.jsonl --no-coverage --sabotage ident    # RED, exit 1
cargo run -p replay -- tests/parity/vectors/oracle_vectors.jsonl --no-coverage --sabotage truncate # RED, exit 1
```

## 产物哈希（hashes.txt）

- `tests/parity/vectors/oracle_vectors.jsonl`：sha256 `683e0570…21de`（C# 生成器自报一致；含 3 条纯字符串帧、6 条 s2c 双帧尾、15 条内部帧 MemoryPack 向量）
- `crates/protocol/src/messages.rs`：sha256 `54a8fd81…90531`（604 条常量 = Messages.cs 594 + Grobal2 白名单 10）

## 关键裁定（实现依据，均来自真源核实）

1. **EDCode 循环状态是 2→4→6→2**（`no = no % 6 + 2`），每第 3 字节多产 1 字符；
   编码长度 = `len + ceil(len/3)`；与 Delphi `EDcode.pas::EncodeBuf` 逐行一致。
2. **帧**：客户端→网关 `#1<编码>!`（'1' 是字面量，`CltMain.pas::SendSocket` 的 `Format('#1%s!')`）；
   网关→客户端 `#<编码>!`；心跳单字节 `'*'`。头 12B 与体**分别编码后拼接**
   （12 % 3 == 0 使拼接点位于编码状态机初态，整体解码等价）。
3. **头结构** `CommandMessage` = `Recog(i32 LE) | Ident/Param/Tag/Series(u16 LE)` 共 12B
   （MemoryPack 对无非托管字段结构体做原始序列化；oracle 已断言长度=12 且 Recog@0）。
4. **内部帧** `ServerMessage` 20B（`PackLength<0` ⇒ 载荷已预编码）、`ServerDataPacket` 6B。
5. C# 网关客户端侧**无显式分帧器**（按 TCP 段直读）；Rust 提供 `FrameSplitter`
   （粘包时 C# 会解析出垃圾，属传输层健壮性差异，非协议差异，见 crates/protocol/src/frame.rs 注释）。
