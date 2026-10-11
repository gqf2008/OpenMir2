# M1 / A2 —— Rust 版 LoginSrv（`bins/login-srv`）交付说明

卡：`openmir2-a2-m1-services`（owner `openmir2-worker-1`）；worktree
`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\a2m1`；分支 `worktree-a2-m1-services`。

参照实现：`src/LoginSrv/**`（**只读**，一行未改）。目标：Rust 版 LoginSrv 顶住 **5500**
（LoginGate → 本进程）与 **5600**（DBSrv/GameSvr → 本进程），客户端登录/选服行为与 C# 一致。

## 1. 交付物

| 文件 | 说明 |
|---|---|
| `bins/login-srv/{Cargo.toml,src/*}` | 新 crate：`main.rs`（启动/日志/定时器）、`config.rs`（`logsrv.conf`+`AddrTable.txt`+`ServerAddr.txt`+`UserLimit.txt`）、`wire.rs`（客户端面帧解析/构造）、`login_server.rs`（5500 监听+单消费者分发+`ClientSession` 全部 6 类消息）、`session_server.rs`（5600 + `SendServerMsg`/`IsNotUserFull`/`GetServerStatus`/排序/清理）、`state.rs`（`SessionManager`/`ClientManager`/`CertUser`/`VerifyAccountRule`）、`gbk.rs` |
| `bins/login-srv/tests/login.rs` | 进程级自测：①与 C# 金标准逐字节一致（`#[ignore]`，需本机 MySQL）；②缺配置/连不上 MySQL 非零退出；③C# 心跳帧必须被接受并回 KeepAlive |
| `tests/parity/loginsrv/oracle_capture.py` | C# LoginSrv 金标准采集器（假 LoginGate 直连真身），产出 `oracle-bytes.json` |
| `tests/parity/loginsrv/oracle-bytes.json` | C# 真身响应字节（6 个 case：认证正反例/未知账号/协议/选服/需补资料） |
| `crates/protocol/src/internal.rs` | **解析容错修复**（见 §4，A-10）：MemoryPack `-1`（null）不再当 `Truncated` |
| `tests/parity/whitelist.md` | 新增 A2/M1 登记 A-10…A-15 |

## 2. 验收判据与证据（证据四件套：命令 + 输出 + 路径 + hash）

### 判据① 混跑链：C# LoginGate(7000) → Rust LoginSrv(5500) 完整登录成功

**命令**（Rust LoginSrv 顶 5500/5600，其余保持 C#）：

```powershell
login-srv.exe E:\tmp\a2m1-loginsrv\mixed\logsrv.conf      # GateAddr=0.0.0.0:5500, ServerAddr=127.0.0.1:5600
# 依次起：DBSrv(src/DBSrv/bin/Release) → GameSvr(E:\MirServer\M2GameSvr) → LoginGate(7000) → SelGate(7100) → RunGate(7200)
python E:\tmp\a2m1-loginsrv\mixed_probe.py loadbot10 loadbot11 loadbot12 loadbot13
python E:\tmp\a2m1-loginsrv\mixed_probe2.py
```

**输出**（`E:\tmp\pi-coord\evidence\a2m1-loginsrv\mixed-chain-probe.log`）：

```text
[loadbot10] CM_IDPASSWORD   → [('!$', 529, 0, 1, '热血传奇/Idle/')]
[loadbot10] CM_SELECTSERVER → [('!$', 530, 5, 0, '127.0.0.1/7100/5')]
... loadbot11/12/13 同形（4/4 全绿，无抖动）
LOGIN  → [('!$', 529, 0, 1, '热血传奇/Idle/')]
SELECT → [('!$', 530, 14, 0, '127.0.0.1/7100/14')]
SessionID from Rust LoginSrv = 14
CM_QUERYCHR(7100) → [('!', 520, 1, 0, '*loadbot20/1/1/1/0/')]   # C# SelGate → C# DBSrv 认这个会话号，取回角色列表
```

- C# LoginGate 侧日志「账号服务器[127.0.0.1:5500]链接成功.」（`heartbeat-csharp-gate-frames.log` 同目录 `mixed-chain-logingate.out.log`）；
- Rust 侧日志 `mixed-chain-rustloginsrv.log`：`账号登陆服务[0.0.0.0:5500]已启动.` / `账号数据服务[127.0.0.1:5600]已启动.` / `账号数据读取成功...[7151]` + 逐条 `[登录]`/`[选服]`（与 C# 同形日志，便于时间线对齐）；
- 假人 BotSrv 侧（`mixed-chain-bots.log`）：`开始登陆 → 帐号登录成功！ → 选择服务器：热血传奇 → 连接角色网关:[127.0.0.1:7100] → 查询角色. → 选择角色 loadbot0 → 准备进入游戏 → 成功进入游戏`。

> BotSrv 自带的 `login_ok` 计数本轮是 2/3（与上一轮 C# LoginSrv 下的 2/3 完全相同，属 C6 线
> 已登记的「探针武装/计数」侧偶发）。判据不采信该计数：同一轮里 Rust 侧日志逐条收到
> `[登录] loadbot0/1/2`，且 4/4 确定性协议探针 + loadbot0 走完进游戏全链。

### 判据② 认证正反例：返回码/文案与 C# 一致（**逐字节**）

**命令**：`python tests/parity/loginsrv/oracle_capture.py --src <C# LoginSrv 构建目录> --out tests/parity/loginsrv/oracle-bytes.json`（采 C# 真身）
→ `cargo test -p login-srv --test login --offline -- --ignored --nocapture`（同一序列打 Rust 版，逐帧比 `raw_hex`）

**输出**（`oracle-parity.log`）：`test result: ok. 1 passed`。六个 case 全部**逐字节相同**（帧尾 `!$`、12B 头字段、体字节）：

| case | 请求 | C# 与 Rust 的响应（同一字面量） |
|---|---|---|
| `protocol` | `CM_PROTOCOL` recog=20011006 | `236464686b6450696a6464686b6464686b2124`（ident=500） |
| `login_ok` | `loadbot1/loadbot1` | `...ident=529 recog=0 series=1 body=热血传奇/0/` |
| `select_server` | 上条 + `CM_SELECTSERVER 热血传奇` | `...ident=530 recog=<会话号> body=127.0.0.1/7100/<会话号>` |
| `login_bad_password` | `loadbot1/wrong-password` | `2353534f4c5353695e6464686b6464686b2124`（ident=503 **recog=-1**） |
| `login_unknown_account` | `nosuchacct/x` | `...ident=503 recog=0`（账号不存在 ⇒ nCode=0） |
| `login_needupdate` | `mir2test/mir2pass` | `...ident=529`（`UserName`/`Quiz2` 非空 ⇒ 不走 531 分支；DB 实况决定） |

> 会话号/在线状态是状态量：`select_server` 的 `recog` 与 `body` 尾段按同一调用序列复现；
> `login_ok` 的 `serverlist` 状态位在无 GameSvr 时是 `0`（有 GameSvr 时是 `Idle`——
> 与 `tests/golden/c-line-baseline-c2-*.jsonl` 的真实抓包一致：`热血传奇/Idle/`）。

### 判据③ 失败要响（不静默空转）

```text
cargo test -p login-srv --test login --offline        # 2 passed：missing_config_exits_nonzero / mysql_unreachable_exits_nonzero
```

- 缺配置（`GatePort=0`）⇒ 非零退出 + `端口为 0 非法：GatePort=0 ServerPort=…`；
- 缺 `AddrTable.txt` / `UserLimit.txt` ⇒ 非零退出 + 指明文件名（A-11，M1④ 授权）；
- DSN 指向不通端口 ⇒ 非零退出 + `[错误] SQL 连接失败!请检查SQL设置...`（A-6 授权偏离）；
- 账号不存在/密码错/被锁 ⇒ 明确回 `SM_PASSWD_FAIL`（recog=0/-1/-2），与 C# 同码。

### 判据④ 自测（进程级，照 `bins/login-gate/tests` 风格）

`bins/login-srv/tests/login.rs`：拉起**本测试编译出的二进制**，用假 LoginGate 走真协议
（6B 头 + MemoryPack `ServerDataMessage`）、真客户端帧 `#1…!`；断言点=与 C# 金标准逐字节一致、
心跳帧被接受并回 KeepAlive、坏配置非零退出。另加 18 条单元测试（配置 INI 语义、AddrTable 解析、
`WriteAsciiString` 布局、`UserEntry` 148B、`ToPacket<UserAccountPacket>`、`SortServerList`、
`VerifyAccountRule`、响应帧字面量）。

## 3. 门禁（本地全绿，原始输出在证据目录）

```text
cargo fmt --all -- --check                                   exit=0   (gate-fmt.log)
cargo clippy --workspace --all-targets --offline -- -D warnings  exit=0 (gate-clippy.log)
cargo test --workspace --offline                             exit=0   (gate-test.log，0 failed；
                                                                     仅 DB 依赖用例 ignored)
```

## 4. 本轮发现并修掉的互操作缺陷（混跑实测，非推测）

**现象**：混跑 30s 后 C# LoginGate 打印「账号服务器长时间没有响应，断开链接。」并断开
（`ConnectState=false; Stop()`，**网关不自动重连** ⇒ 整跳永久死掉，客户端再也登录不了）。

**根因**：网关每 10s 发心跳 `ServerDataMessage{Type=KeepAlive, SocketId=""}`，其 `Data` 是
**null** ⇒ MemoryPack 长度字段 `-1`。`crates/protocol::internal` 原把 `-1` 当 `Truncated`
拒绝 ⇒ Rust LoginSrv 丢弃心跳、不回帧 ⇒ 网关判死。真身帧（独立抓取，2026-10-11）：

```text
9aaa9aaa0c00 | 04 03 00000000 0000 ffffffff      # SocketId=""(0)、Data=null(-1)
```

**修法**：解析侧把 `-1` 视作空（`""`/`[]`），其它负值仍报错；重编码侧一律写 0（空）
（接收方只读 `Type`，行为等价）。**接口未变**（无签名/字段改动），但属 `crates/protocol` 改动
⇒ 需通知 `bins/login-gate`、`bins/sel-gate`（两者只发不收，已确认不受影响）。已登记 **A-10**。

**对照实验**（`heartbeat-control.log`，证明因果而非巧合）：

```text
对照组：python silent_peer（accept，从不回心跳）  → 09:31:40.980 链接成功 → 09:32:11.213「长时间没有响应」（30.2s，RED）
实验组：Rust bins/login-srv（修复后）             → 09:23:21.693 链接成功 → 之后 8 分钟多次心跳，无判死行（GREEN）
```

## 5. 已知偏离

全部登记在 `tests/parity/whitelist.md`（A2/M1 段）：A-10 协议解析容错（心跳 null）、
A-11 缺配置即报错退出（M1④ 授权）、A-12 `ServerAddr.txt` 行过滤笔误口径、
A-13 畸形帧只跳过该条（C# 侧 `return` 会让消费循环**永久停摆**）、A-14 gb2312 用 GBK 近似、
A-15 存储层 `AccountRecord` 字段面（C# `GetAccount` 读集之外的列属 D2 批次）。

**刻意 1:1 保留的 C# 怪行为**（不改，供后续修复批次索引）：`Index()` 返回 Id 而非位置、
`Add` 的 `Index(s)>0` 判定、`Update` 的 `Count <= nIndex` 越界判定、`ReceiveOpenUser` 先更新再无条件
Add、`SocketClientRead` 把 `ReceiveMsg` 写给所有条目、`KickUser` 直接把裸串 `%+-<sock>$` 写进内部链路、
`GetSelGateInfo` 从 `nSelIdx+1` 起轮询、`GetBackPassword` 的 `"\09"` 字符串分隔符（'\\0' 与 '9'）。

## 6. 残留物清单

- 证据目录（仓外，保留）：`E:\tmp\pi-coord\evidence\a2m1-loginsrv\`（含 `SHA256SUMS.txt`）；
  临时实验脚本：`E:\tmp\a2m1-loginsrv\{mixed_probe.py,mixed_probe2.py,capture.py,silent_peer.py}`（副本已进证据目录）；
- 本线起的进程收尾：Rust `login-srv`、C# `DBSrv/GameSvr/LoginGate/SelGate/RunGate`、
  `BotSrv`、`mysqld` 全部结束（十端口回到开工前的全 DOWN 状态）；
- `E:\MirServer\logs\*` 的 C# 运行日志被本轮覆盖（运行时产物，非仓库文件；关键切片已复制进证据目录）；
- 仓库内未跟踪产物：`tests/botload/bots-*.log`（已删除，样本已复制进证据目录）；
  `src/BotSrv/bin/`、`mir2-rs/target/` 为构建产物（构建目录）。
