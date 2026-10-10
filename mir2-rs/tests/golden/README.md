# tests/golden —— 金标准抓包（C 线产出，A 线 M0 验收①②③的输入）

**状态：C 线抓包链路已通（影子网关 + 抓包代理），待交付 JSONL。** 本目录一旦入库抓包文件，
A 线的 replay 门禁即可对真实流量执行 M0 验收。

## 抓包要求（设计文档 §11.2）

真实客户端（`D:\MirClient-run\run-release.cmd`，冻结客户端逐字节不改）+ 现网 C# 服务端，
从进程启动到「登录 → 选服 → 选人 → 建角 → 进游戏 → 战斗 → Alt+X 小退 → 再进」全程：

- 客户端↔网关（7000/7100/7200）双向**线上原始字节**（含 `#`/`!` 定界符与 `*` 心跳）；
- 每个包记录 C# 侧同一时刻观测到的字段日志（网关 `ClientSession` 解出的
  ident/recog/param/tag/series 与 body）；
- 时间戳（毫秒，单调即可），用于与 `E:\MirServer\logs\*.out.log` 对时间线。

### 抓包前置条件（C 线实测，2026-10-10）

1. **GameSvr 必须是全新启动的进程**。C# GameSvr 的网关表经不起 GameGate 连接抖动：
   网关断开会把槽位 `UserList` 置 null（`M2Server/Net/TCP/TCPNetChannel.cs::CloseGate`），
   此后进世界的玩家在 `SetGateUserList` 上 NRE 死循环（`NewHumanList` 清不掉），客户端黑屏，
   且**此后每次登录都失败直到 GameSvr 重启**。
   ⇒ 若回放报告里「进世界」阶段为空，**先查抓包那次 GameSvr 是否为全新进程**，再怀疑回放。
2. **探活一律用 `netstat` 文本解析，不要 TCP 连上去探**：连上去会造出幻影用户并触发同一 NRE。
3. 抓包不得改动 `E:\MirServer` 与 `D:\MirClient-run`（影子网关 + 代理方式已验证可行）。

## 文件格式（replay 工具直接消费）

`*.jsonl`，每行一条，UTF-8：

```json
{"kind":"frame","dir":"c2s","seq":1,"ts_ms":12345,"raw_hex":"2331..21",
 "ident":2001,"recog":0,"param":0,"tag":0,"series":0,"body_len":19,
 "body_sha256":"<sha256 of 解码后明文body>"}
```

字段口径：

| 字段 | 含义 |
|---|---|
| `dir` | `c2s` = 客户端→网关（帧形 `#1...!`）；`s2c` = 网关→客户端（`#...!`） |
| `seq` | 单调递增序号（全局，不分方向） |
| `ts_ms` | 捕获时刻（毫秒） |
| `raw_hex` | **线上完整帧字节**（含首尾定界符）的小写 hex；**末字节必须是 `!`(0x21)** |
| `ident/recog/param/tag/series` | C# 侧同一时刻解出的头字段（12B 头） |
| `body_len` / `body_sha256` | 解码后明文体（GB2312 原始字节，不转码）的长度与 sha256 |
| `string_frame` | 见下：无 12B 头的纯字符串帧必须标 `true` |
| `kind:"edcode"` | （可选）裸编解码向量：`plain_hex` / `encoded_hex` |

### 纯字符串帧（`"string_frame": true`）

网关在建立阶段收到的**首个上行**不是 12 字节 `CommandMessage` 头，而是裸字符串
（GameGate 走 `EDCode.DeCodeString(destinationSpan[2..packetLen-1])`，实测载荷形如
`**账号/角色/证书/版本/校验码/机器码/服务号`；LoginGate/SelGate 存在同类形态）。
这类记录加 `"string_frame": true` 并省略 `ident..series`，replay 只对它做：

- ① 逐字节回放（`#1` 或 `#` + 编码体 + `!` 与 `raw_hex` 完全相同）；
- ② `body_len` / `body_sha256` 对拍；

不参与头字段对拍，也不计入包号覆盖（`--sabotage ident` 对这类帧不适用）。
C# oracle 已含 3 条此类向量（`tests/parity/vectors/oracle_vectors.jsonl`），
`cargo test -p mir2-protocol` 与 `cargo run -p replay` 都会覆盖这条路径。

### 帧尾有两种形态（现网并存，抓包必须原样记录）

C 线实测 + C# 源核对（2026-10-10）：

| 跳 | 帧尾 | 来源 |
|---|---|---|
| 7000（LoginGate，**转发 LoginSrv 的帧**） | `!$` | `LoginSrv/Services/ClientSession.cs:745` 的 `"#" + sMsg + "!$"`，LoginGate 原样转发 |
| 7000（**LoginGate 自身**产生的帧，如超时踢人 `SM_OUTOFCONNECTION`） | `!` | `LoginGate/Services/ClientSession.cs:181/197` |
| 7100（SelGate）/ 7200（GameGate） | `!` | 各自网关构造 |

⇒ `crates/protocol` 用 `ServerFrameTail`（`Bang` / `Bang$`）区分，回放按原样复现。

**记录要求：s2c 帧请带 `"hop"`**（`login` / `sel` / `game`）。它是帧尾的**独立期望**：
`login` ⇒ `!$`，`sel`/`game` ⇒ `!`。原因是逐字节回放对帧尾自洽（解码得尾、重编码还原尾），
**单独改尾不会被 ① 发现** —— 该盲区由红检实测暴露（`evidence/M0/redcheck4-s2c-tail.log`），
所以帧尾必须由 `hop`（或显式 `"tail":"!"` / `"tail":"!$"`，用于同跳两种形态的情形）给出外部期望。
缺这两个字段时帧尾不参与判据（仅自洽往返）。

无法对应到头字段的包（如 1 字节心跳 `2a`）可记 `{"kind":"heartbeat","dir":"s2c","seq":N,"ts_ms":N}`，
replay 跳过非 frame 记录以外的校验（心跳不参与①②③计数）。

## 验收（A 线执行）

```powershell
cargo run -p replay -- tests/golden/<capture>.jsonl
# GREEN 需要：①字节差异=0 ②字段差异=0 ③八阶段全覆盖且未识别包号=0
```

## 防伪约定

抓包必须来自真客户端+真服务端；**禁止**用 replay 自己编码的产物回填当判据
（自造样本自证 = 恒绿假门禁）。每份抓包在下方登记：来源机器、客户端版本、时间、sha256。

| 文件 | 来源 | 时间 | sha256 |
|---|---|---|---|
| （待登记） | | | |
