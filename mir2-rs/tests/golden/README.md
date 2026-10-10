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

**流程（不是记性）：改了口径 = 重导全部入库件 + 复验 + 重登记。**
只要导出器/协议口径变了（新增字段、改分段布局、改 act 帧口径…），就必须把**所有**入库金标准
从保留的裸 dump 重导一遍并同批更新登记 —— **不是只重导本批关心的那份**。
`"登记 hash 能对上"不能证明产物是新的`：本项目 2026-10-10 实测栽过（C4 改口径后只重导了 C3 与上批 16 帧，
C2 那份还是旧 schema，用当前 replay 跑它给 act 假红 66 条 + ② 21 条，而 README 写着 ②=0，见下"C4 补记"）。
这条靠机器守：

```powershell
python tools/capture/golden_fresh_check.py          # 全量：重导==入库件（逐字节）+ README 登记 sha 一致 + registry 双向对账
python tools/capture/golden_fresh_check.py --selftest   # 改坏必红（临时目录里做，不动入库件）
```

判据三条：① `mir2-rs/tests/golden/*.jsonl` 与 [registry.json](registry.json) 双向一致（新增金标准必须登记来源会话目录）；
② 每份入库件 = 当前导出器从登记来源裸 dump 的重导结果（逐字节）；③ README 登记行的 sha256 = 文件实际 sha256。
已接进 `tools/e2e/run_e2e.ps1`（`golden-fresh` + `golden-fresh-red` 项）。**真值始终是裸 dump**，金标准只是它的派生产物。

**这条检查"预期会红"的情形（别当故障，更别去放宽判据）**：

- **协议层布局表/口径变了、金标准还没重导**（例如 A4 落 `BodyLayout::Segmented` 表项）：
  入库件立刻不再是"当前产物"⇒ `golden-fresh` 红。**这是设计意图**：表变了，产物就该重导。
  唯一合法出口是「重导全部入库件 + 重登记」；要临时缩小范围用 `--only <子串>`，并在提交信息里写明原因。
- 推论（协作约定）：**谁改了协议布局表，就等于让 C 线的入库件立刻过期**。改表的人不必自己重导，
  但必须知道这件事并让 C 线知道（本仓做法：改表后在共享分支上喊一声，C 线重派生 + 三份全重导 + 重登记）。
- 真故障红（要查）：来源会话目录缺失 / `registry.json` 缺项 / 导出器 rc≠0 / 裸 dump 对账（`verify_golden.py`）失败。
- 派生侧形状约定（A 线冻结、C 线派生器按此抓）：`seg_len` 用 `usize` 字面量、`sep` 是 `b'/'`、
  `count` 用 `SegCount::Fixed(3)` / `SegCount::HeaderSeries`；字段一改名派生器**报错而不是猜**
  （已用 fixture 实测：`segmented_check.py` 步骤 0a/0b）。

| 文件 | 来源 | 时间 | sha256 |
|---|---|---|---|
| `c-line-baseline-20261010-092858.jsonl` | 本机真实客户端 `D:\MirClient-run`（冻结未改）+ 现网 C# 服务端 `E:\MirServer`（只读未改，网关为会话目录影子副本）；见 [C线-交付说明.md](C线-交付说明.md) （**C4 批按分段布局表重导**） | 2026-10-10 09:28:58 | `d1f02442ce96ff6029922c15d000eef5e01093936c3e903160c00e1c71d83009` |

上批抓包覆盖 **5/8 阶段**（登录/选服/选角/建角/进世界到"公告确认"），缺 移动/攻击/小退；
缺口原因与证据（当时是抓包拓扑的代价，非协议/非抓包）见 [C线-交付说明.md](C线-交付说明.md) 的"已知缺口"一节。

**C2 批（2026-10-10，客户端进程内 dump，拓扑不变）新增一份：**

| 文件 | 来源 | 时间 | sha256 |
|---|---|---|---|
| `c-line-baseline-c2-20261010.jsonl` | 同机真客户端 `D:\MirClient-run`（**二进制未改**，sha256 `c3cb8f73…c07996`）+ 现网 C# 服务端（未改）；dump 由注入客户端的 `tools/capture/clienthook` 在**进程内** hook `wsock32` 的 send/recv 落盘，客户端直连真实 7000/7100/7200。**C4 批按分段布局表重导（见下）** | 2026-10-10 15:0x | `d3af8c99d9446c8be6600c6bf792dedaf3ee7a4e84f276ad30776b8d31c29f4e` |

该抓包 **222 帧**，覆盖 **7/8 阶段**（缺 小退，三条路都被堵住：客户端确认框合成按键打不开、
顶号只发系统提示不发 528、802 需 GM/多服触发 —— 详见 [C2-交付说明.md](C2-交付说明.md)）。
本机跑 `replay`（**A4 表落地后重导**）：**① 0、② 0**、未登记号 0、③ 7/8 ⇒ **RED 仅剩覆盖缺口这一格**
（8/8 覆盖由 C3 那份金标准给出，见下）。
（历史：C4 之前的老导出件是 ① 56、act 帧口径假红 66；C4 重导后降到 ① 2 —— 那 2 条是 `811`/`201`
的分段体，等 A4 落表；A4 落地后重导 ⇒ **① 归零**。见下「C4 补记」与「C4 续 2」。）

**C3 批（2026-10-10，同法补第 8 阶段「小退」）：**

| 文件 | 来源 | 时间 | sha256 |
|---|---|---|---|
| `c-line-baseline-c3-20261010.jsonl` | 同机真客户端（**二进制未改**）+ 现网 C# 服务端（未改）；进程内 hook dump，客户端直连真实 7000/7100/7200；小退走**客户端自带「小退」按钮**（底部条 (754,621)）→ 确认框 → `CM_SOFTCLOSE(1009)`。**C4 批按分段布局表重导（见下）** | 2026-10-10 17:1x | `f34a6d111892912436eeb0d50cf2effc7e6f2915bf6ebfbf583104d3409a7ed9` |

该抓包 **382 帧**，**八阶段全覆盖**（`replay` 的 ③ 逐阶段命中：登录 2 / 选服 2 / 选角 2 / 建角 2 /
进世界 6 / 移动 4 / 攻击 3 / **小退 1**）。配套证据：`07b_logout_confirm_dlg.png`（"确认退出到选择角色界面吗？"）
与 `08_back_charsel.png`（回到选人界面、新角色 lb1a 在列）。
本机 `replay`（**A4 表落地后重导**）：**① 0、② 0、③ 8/8 ✓、未登记包号 0 ⇒ RESULT: GREEN**
（M0 四条验收在 C3 这份上全过）。历史：C4 前是 ① 33 / ② 21（体＝两段分别编码拼接族 + act 口径），
A4 落表 + 重导后逐字节回放归零。

**C4 批（2026-10-10）：导出器按分段布局表解码（A-9），三份金标准都用保留的裸 dump 重导。**

- 布局表**由协议源码派生**（`tools/capture/dump_body_layout.py` 读 `crates/protocol/src/frame.rs` 的
  `body_layout()`，形状变了就直接报错）⇒ 不在这里抄第二份表。当前导出：
  `head_block=16`、`act_prefix='+'`、`struct_then_rest = 6/7/9/10(SM_RUSH/RUSHKUNG/BACKSTEP/TURN) struct_len=8`。
- 新字段（按 A 线契约）：`body_segments:[{len,sha256}]`（单段即单元素）、
  `frame_form:"act"`（仅 payload 首字符 `+` 的 s2c 明文动作帧，不参与头字段对拍）、
  `layout`（`single` / `struct_then_rest` / `struct_then_rest(short)` / `flat_legacy`，便于排查）。
- 重导后本机 replay（三份都实测）：**C3 ① 33→2、② 21→0；C2 ① 2、② 0（act 口径假红 66→0）；上批 16 帧 ① 0、② 0**
  ⇒ **M0 ② 归零**（A-9 销账）。余下 ① 2 条 = `ident=811`(C2 seq 28 / C3 seq 28) 与 `ident=201`(C2 seq 66 / C3 seq 75)，
  属 A-8 待补表的其它多段形态（**C2 与 C3 同源**，不是两份不同的问题）。
- **C4 补记（同批发现并修）**：入库的 C2 金标准此前一直是 **C4 之前的旧 schema**
  （缺 `frame_form`/`layout`/`body_segments`），跑当前 `replay` 会给**假红**（act 帧口径不符 66 条 + `②` 21 条）；
  已用保留的裸 dump 按当前导出器重导覆盖：sha256 由 `bcb9e483…` → **`012a2aa5…`**，② 21→0、act 口径假红 66→0。
  （教训：**改了导出器口径就要重导并重登记，不能只改登记行**——旧 artifact 会让下游对新口径产生假红/假绿。
  已固化成流程 + 自动判据：见上面"防伪约定"的 `golden_fresh_check.py`，不再是靠记性。）
- C4 判据实测：同 dump 两次导出 sha256 相同（`c26d48b9…`）✓；把 `struct_len` 故意改成 7 后
  **② 由 0 变 52**、结果 RED ✓（改错段边界必红）；③ 仍为 C3 八阶段全覆盖、C2 有 7 阶段、上批 5 阶段。
- **第三种布局 `Segmented` 已按 A 线冻结契约实现**（`count_kind=fixed` / `header_series`，
  段边界**按表算长度逐段取**、不按分隔符盲切）：`tools/capture/segmented_check.py` 用 C3 裸 dump 的真实帧实测
  `811 → body_len 60 / [20,20,20]`、`201 → 496 / [124×4]`（= A 线给的值），
  并在 `--verify-roundtrip` 下证明**各段重新编码拼回去逐字节等于线上原体**；
  把 `seg_len` 改错一格 ⇒ `segmented(mismatch:…)` + 退出码 4（改错必红）。
  等 A4 表项落地，重跑 `dump_body_layout.py` 即可（表是派生来的，导出器不用改）⇒ 预期的 ① 2→0。

**C4 续 2（2026-10-10，A4 表落地后重导：① 全部归零）**

- **触发**：A 线落 `Segmented` 表项（`crates/protocol` 提交 `0180c16a`：`811 = {seg_len 20, b'/', false, Fixed(3)}`、
  `201 = {124, b'/', true, HeaderSeries}`），落到我这边正好触发"**协议表变了 ⇒ 入库件过期**"（预期红）。
- **做法（一条命令）**：`python tools/capture/golden_fresh_check.py --regen --update-registry`
  —— 重派生布局表 → **三份金标准全部**重导（先导临时文件、成功才落盘，带 `--verify-roundtrip`）→ 更新登记行 → 自动对账。
  落地前派生器先按设计**报错**了一次：A 线的臂语法是 `messages::X => BodyLayout::Segmented { … },`（**无外层大括号**），
  与我原来只认的 `=> { … }` 不同 —— 报错而不是静默少一档（这正是上一批加守卫的目的）；随后把两种臂语法都认（并保留守卫）。
- **重导后实测（三份全部 ① 0 / ② 0）**：

  | 金标准 | 帧数 | ① 字节回放 | ② 字段对拍 | ③ 覆盖 | 结论 |
  | --- | --- | --- | --- | --- | --- |
  | `c-line-baseline-c3-20261010.jsonl` | 382 | **0** | **0** | **8/8** | **GREEN** |
  | `c-line-baseline-c2-20261010.jsonl` | 222 | **0** | **0** | 7/8 | RED 仅剩"缺小退"覆盖格 |
  | `c-line-baseline-20261010-092858.jsonl` | 16 | 0 | 0 | 5/8 | 未变（无分段帧） |
- **sha256（重登记后）**：C2 `012a2aa5…` → **`d3af8c99d9446c8be6600c6bf792dedaf3ee7a4e84f276ad30776b8d31c29f4e`**、
  C3 `c26d48b9…` → **`f34a6d111892912436eeb0d50cf2effc7e6f2915bf6ebfbf583104d3409a7ed9`**、
  上批 `d1f02442…` 未变。两帧重导后：`811 → layout segmented / body_len 60 / 段 [20,20,20]`、
  `201 → segmented / 496 / [124,124,124,124]`（201 的 `series=4` 与段数独立吻合）。
- **意义**：`811`/`201` 的切法由 A 线表决定、重编码后与 C# 真身发出的线上字节**逐字节相同** ⇒ 表被 oracle 锚定。
  裸 dump 全程未动；`golden_fresh_check` 三份 PASS。
