# Lane B — 网关会话生命周期审计（只读）

- 仓库/基线：`E:/Users/gxh/Documents/GitHub/OpenMir2` @ `acb017ed9`（master）
- 范围：LoginGate / SelGate / GameGate / LoginSrv / DBSrv 的**连接建立、登录绑定、空闲/心跳、断线、重连、顶号、会话表释放（槽位回收）、异常关闭**
- 方式：只读源码对读（C# oracle ↔ Rust `mir2-rs/bins/*`）；未建 worktree、未改仓库、未起进程、未碰 `E:/MirServer`、未设 `CARGO_TARGET_DIR`
- 口径：行为等价优先；Rust 现状按"已实现 / 部分实现 / 无"标注，不把"Rust 更健壮"当成等价

## 摘要（<1000 字）

按「客户端可见 × 生产可达」排序：① **SelGate 下行断线不重连**（oracle 每 10s 重试，Rust 只连一次）⇒ DBSrv 重启/闪断后建删选角全失败，直到网关重启；② **LoginGate/SelGate 客户端 60s 超时未建模**（LoginGate 实为"连接年龄"超时；两处 SM_OUTOFCONNECTION 帧在 oracle 都是畸形的，1:1 要逐字节复刻）；③ **GameGate 会话计数 used 只增不减**（Rust-only，累计 1e4 次连接后全拒）；④ **GameGate 缺 GM_CHECKCLIENT/GM_RECEIVE_OK 回显/GM_STOP**（缺回显使 GameSvr 每 ~40KB 暂停 50ms）；⑤ **LoginGate/SelGate 槽位上限与回收未建模**（oracle 正常断开漏槽，上限 1e4/2e3，SelGate 满后串号）；⑥ **DBSrv 在 LoginSrv 链路断开时清空会话表**（C# 保留）；⑦ **LoginSrv 网关条目回写竞态**（Rust-only）。次要：写失败早退泄漏会话、顶号裸 kick 无重同步、GateCount>1 未建模。tier：①④⑥⑦及裸 kick=R；③⑤=L；②=R。

## 1. 排序总表

| # | 服务·类别 | oracle 入口（文件:行） | Rust 对应 | 缺口 | 可达性 | tier |
|---|---|---|---|---|---|---|
| 1 | SelGate·重连/心跳 | `src/SelGate/TimedService.cs:109-152`、`Services/ClientThread.cs:112-133,244-250`、`GateShare.cs:19-20` | `bins/sel-gate/src/main.rs:72-91,227-269` | 不连 KeepAlive、下行 EOF 后不回连 | 高：DBSrv 重启/闪断即整跳永久死 | R |
| 2 | LoginGate/SelGate·空闲/超时 | LoginGate `Services/ClientSession.cs:128-150` + `TimedService.cs:43-85` + `Conf/GateConfig.cs:63`；SelGate `Services/ClientSession.cs:92-115` + `TimedService.cs:38-76` + `Conf/GateConfig.cs:116` | 无（两 bin 无任何 timeout 状态） | 60s 超时、SM_OUTOFCONNECTION 帧、超时后的收尾全缺 | 高：每个空闲 60s 的会话 | R |
| 3 | GameGate·会话表释放 | `src/GameGate/Services/ServerService.cs:166-209`（断线清槽）、`Services/ClientThread.cs:21` | `bins/game-gate/src/main.rs:204-224`（`used` 205/209/215） | `used` 只 insert 不 remove，计数=累计连接数 | 高：累计 1e4 次连接后全拒；长时间运行必达 | L |
| 4 | GameGate·心跳/背压 | `TimedService.cs:69-88`、`ClientThread.cs:265-288`（GM_STOP/CHECKSERVER/RECEIVE_OK）、`GateShare.cs:22` | `bins/game-gate/src/main.rs:527-596`（569 空操作；591 落 other） | 无 GM_CHECKCLIENT、不回显 GM_RECEIVE_OK、GM_STOP 只打日志 | 中高：>40KB 下行周期/维护窗口 | R |
| 5 | LoginGate/SelGate·槽位上限/回收 | LoginGate `GateShare.cs:10`、`ClientManager.cs:130-149`、`ServerService.cs:82-105,113-127`、`ClientThread.cs:84-96`、`TimedService.cs:80`；SelGate `ClientThread.cs:28,231-242`、`ServerService.cs:99-124,126-150`、`TimedService.cs:68-72` | `bins/login-gate/src/main.rs:51,168,217`、`bins/sel-gate/src/main.rs:51,168,218` | 无 1e4/2e3 上限；不复制"正常断开漏槽→满后串号" | 高：登录/选角 churn 累积 | L |
| 6 | DBSrv·断线 | `src/DBSrv/Services/Impl/ClientSession.cs:64-68`（只打日志）、`AppService.cs:58-63`（Stop 仅进程退出）、`TimedService.cs:40-50` | `bins/db-srv/src/loginsrv.rs:286-291` | 链路断开即 `sessions.clear()`，C# 保留 | 中：LoginSrv↔DBSrv 闪断时老玩家会话立即失效 | R |
| 7 | LoginSrv·网关表竞态 | 无等价（`LoginServer.cs:115` 每条现查，删除 `:73-79`） | `bins/login-srv/src/login_server.rs:146-180`（clone→insert）vs `:97` remove | 断开与在途消息并发时把已删条目写回 | 中：网关断开瞬间 | R |
| 8 | LoginGate/SelGate/GameGate·异常关闭 | C# 发送前查 `IsConnected`（`LoginGate/ClientThread.cs:238-246` 等） | login-gate `main.rs:179` vs 清理 `:216-221`；sel-gate `:180` vs `:217-221`；game-gate `:266` vs `:306-318` | 上行写失败 `?` 早退，跳过 `sessions.remove` 与 Leave | 中：对端不可用期间每个客户端 | L |
| 9 | 顶号·裸 kick 帧流 | `LoginSrv/Services/ClientSession.cs:767-771,875-879`；`LoginGate/Services/ClientThread.cs:162-223`（逐字节重同步） | login-srv `login_server.rs:51-56`（裸写，同 C#）；login-gate `main.rs:226-268`（无重同步） | 裸 `%+-sid$` 后 Rust 按 6B 吞，可能永久失步 | 低中：已登录会话再发 CM_IDPASSWORD | R |
| 10 | 三网关·多实例 | LoginGate `ServerManager.cs:106-120`、`ClientManager.cs:25-36`；SelGate `ClientManager.cs:28-41`；GameGate `ServerManager.cs:36-44` | 各 `config.rs` 只读 `*0`，单 listener | GateCount>1 / 多下游实例未建模 | 低：现网据文档为 1；配置>1 即不可用 | L |
| 11 | GameGate·服务端 GM_CLOSE | oracle 不处理（`ClientThread.cs:263-289` 无该 case） | `main.rs:564-567` 删会话 | Rust 多做了（今日 GameSvr 从不发 ⇒ 不可达） | 不可达 | —（仅登记） |

## 2. 逐类明细（按"客户端可见 + 生产可达"排序）

### 2.1 SelGate 下行不重连（P0，客户端可见）
- oracle：`ClientThread.ClientSocketDisconnect` 置 `CheckServerFail=true`（`src/SelGate/Services/ClientThread.cs:124-133`）；`TimedService.CheckSessionStatus` 每 10s 判一次，失败即 `Start()` 重连（`TimedService.cs:130-152`）；连通时每 10s 发 `ServerDataMessage{Type=KeepAlive}`（`ClientThread.cs:244-250`）。
- Rust：`main` 只 `TcpStream::connect` 一次（`bins/sel-gate/src/main.rs:72-91`）；`downstream_loop` 读失败即 `return`（`:227-232`），进程继续 accept，但所有上行 `send_internal` 会失败；不发 KeepAlive。
- 缺口：重连节拍、失败计数日志（`链接服务器[...] 失败次数[N]`）、恢复后继续承载会话全部缺失。
- 可达性：DBSrv 重启、网络闪断、DBSrv 侧主动关连接。客户端表现：连接 7100 成功但任何建/删/选角都无响应（写失败后 Rust 侧关闭该客户端）。
- tier R：重试节拍与断线瞬间表现属时序；且"断线时会话是否保留"与 oracle 的 `SessionArray` 语义绑定。

### 2.2 LoginGate/SelGate 客户端超时（P0，客户端可见）
- LoginGate oracle：`m_dwClientTimeOutTick` 只在构造时初始化（`Services/ClientSession.cs:20`），`ProcessClientPacket` 内的刷新全部被注释（`:77-117`）⇒ **60s 连接年龄超时**，不是空闲超时；检查周期 1s（`TimedService.cs:14-24,43-85`）；默认/下限 60s/10s（`Conf/GateConfig.cs:63`、`Conf/ConfigManager.cs:26-30`）。
  - 触发：`SendDefMessage(SM_OUTOFCONNECTION,0,0,0,0)`（`:131-141`）→ `success=true` → TimedService 先 `_sessionManager.CloseSession`（移除并关 TCP，`SessionManager.cs:28-34`）再清槽（`TimedService.cs:79-80`）；随后的 Disconnected 查不到会话 ⇒ **不发 UserLeave**（`ServerService.cs:113-127`），LoginSrv 侧 UserInfo/会话残留。
  - 帧是畸形的：`SendDefMessage` 用 `sendBuf[0]='#'` + `Encode(..., dstOffset=1)`，返回的是 `dstPos-dstOffset`，最后按 `SendMessage(id, sendBuf, iLen)` 发送 ⇒ **丢掉最后一个编码字符且不带 `!`**（`ClientSession.cs:167-199`）。
- SelGate oracle：`_clientTimeOutTick` 只在 CM_QUERYCHR/NEWCHR/DELCHR/SELCHR 刷新（`Services/ClientSession.cs:67-78`）；扫描周期 20s（`TimedService.cs:38-76`）；默认/下限 60s/10s（`Conf/GateConfig.cs:116`、`Conf/ConfigManager.cs:33-41`）。
  - 触发：`SendDefMessage(SM_OUTOFCONNECTION)`；该函数先写 `SendBuf[0]='#'`，随后 `Encode(..., dstBuf)` 从 offset 0 写覆盖 `#`，发送长度 `iLen+2` 还包含下标 `iLen` 的未初始化字节（`ClientSession.cs:131-161`）。
  - 收尾：`success=true` ⇒ 仅从 `_sessionManager` 移除 + 清槽（`TimedService.cs:68-72`），**不关客户端 socket**；此后该客户端封包因 `GetSession==null` 被静默丢弃（`ServerService.cs:55-68`）⇒ 僵尸连接；最终 TCP 断开也不发 UserLeave。
- Rust：两 bin 均无超时/无 SM_OUTOFCONNECTION/无对应收尾。
- 可达性：任何在登录界面/选角界面停留超过 60s 的真实客户端；LoginGate 对**所有**连接（含活跃）都按 60s 关。
- tier R：时序 + 客户端可见字节；并且要复刻两处 oracle 畸形帧。

### 2.3 GameGate 会话计数 `used` 泄漏（P0，Rust-only）
- oracle：连接建立按 `SessionArray[nIdx]` 找空槽（`Services/ServerService.cs:131-147`），断开时清空该槽（`:181-197`）⇒ `MaxSession`=10000 是**并发**上限（`GateShare.cs:14`）。
- Rust：`accept_loop` 的 `used: HashSet<u16>` 在接入时 `insert`（`main.rs:205-215`），`client_loop` 结束**从不 remove**（全文件仅 3 处 `used` 引用）⇒ `used.len()` = 进程生命周期累计接入数。到 10000 后 `会话已满(10000)，拒绝` 并关闭所有新连接。
- 可达性：任何运行足够久、累计 >1e4 连接的 GameGate（含假人/重连 churn）。客户端表现：能连上 7200 但立即被断开，整个区进不去。
- tier L：纯计数逻辑，可用注入小容量/进程级 churn 测试判定；不涉 RNG/时序/落库/战斗数值。

### 2.4 GameGate 下行健康/背压帧（P1，客户端可见）
- oracle：
  - 每 10s `GM_CHECKCLIENT`（`TimedService.cs:69-88` → `ClientThread.CheckConnectedState` `:112-142`）；GameSvr 收到后置 `SendKeepAlive`，由 `TCPNetChannel.Run` 下发 `GM_CHECKSERVER`（`src/M2Server/Net/ChannelMessageHandler.cs:163-164`、`TCP/TCPNetChannel.cs:435-438`）。
  - 收到 `GM_RECEIVE_OK` 必须**回发**一条（`ClientThread.cs:279-280`）；否则 GameSvr 的 `SendChecked` 只能等 50ms 超时复位（`ChannelMessageHandler.cs:59-64`、`M2Share.cs:180`），阈值为 `CheckBlock*10`（默认 4069*10 ≈ 40KB，`Modules/SystemModule/Conf/Model/GameSvrConf.cs:1260`）。
  - `GM_STOP` → `RunningState.Stop`（`ClientThread.cs:265-267`），`ServerManager.GetClientThread` 过滤非 Runing（`:169`），新客户端不再拿到 `GM_OPEN`（socket 留在服务端、不主动关，`ServerService.cs:123-129`）。
- Rust：`server_loop` 只处理 `GM_DATA`/`GM_CLOSE`/`GM_SERVERUSERINDEX`；`GM_CHECKSERVER` 空操作（`:569`），`GM_RECEIVE_OK`/`GM_STOP` 落 `other` 仅打日志（`:584-592`）；从不发 `GM_CHECKCLIENT`。
- 可达性：GM_RECEIVE_OK 在大流量下每 ~40KB 触发一次 ⇒ 周期性 50ms 下行停顿；GM_STOP 期间 Rust 仍接受并给已停机实例发 GM_OPEN。
- tier R：时序/协议交互，需真机对 C# GameSvr 取证。

### 2.5 LoginGate/SelGate 槽位上限与回收（P1，决策卡）
- LoginGate oracle：上限 10000（`GateShare.cs:10`）；`GetClientThread` 用 `SessionIsFull()` 判满（`ClientManager.cs:130-149`），满则连接分支只打日志、**不关客户端也不建会话**（`ServerService.cs:65-70`）。槽位清除只发生在：① 下游连接断开（`ClientThread.cs:116-135` 把槽置 null）；② 60s 超时（`TimedService.cs:80`）。**正常客户端断开不清槽**（`ServerService.cs:113-127`）⇒ 1e4 次正常断开后（且下游未重连过）整网关失能。
- SelGate oracle：上限 2000（`Services/ClientThread.cs:28`）；分配循环满时不会新建，而是保留**最后一个非空槽**，随后 `AddClientThread(staleId)` / `UserEnter()` / `AddSession(staleId)`（`ServerService.cs:99-124`）⇒ 新客户端与会话串到旧 id；断开不回收（`:126-150`），`RestSessionArray` 也不置 null（`ClientThread.cs:231-242`）⇒ 2e3 次正常断开后必然发生。
- Rust：两 bin 都用 `HashMap<u16, _>`，断开即 remove，无上限、无串号；`AtomicU16` 到 65535 回绕后可能覆盖活跃会话（`main.rs:108-112`）。
- 可达性：SelGate 每次选角后客户端都会断开 7100 ⇒ 正常 churn 就能触发；LoginGate 每 60s 只回收超时会话，正常断开仍漏。
- tier L：纯计数/分支，可用注入上限的进程测试判定。**但"复刻 oracle bug"还是"登记授权偏离"需 owner 裁决**（见 B-3 卡）。

### 2.6 DBSrv 对 LoginSrv 链路断开的会话表语义（P2）
- oracle：`LoginSocketDisconnected` 只打日志（`src/DBSrv/Services/Impl/ClientSession.cs:64-68`）；清表只在 `Stop()`（`AppService.cs:58-63`，进程退出）。重连由 `UseReconnection` + `TimedService.cs:40-50` 每 10s `Start()` 完成，**列表跨断线保留**。
- Rust：`login_link_task` 断开后 `st.sessions.clear()`（`bins/db-srv/src/loginsrv.rs:286-291`）⇒ 闪断后老玩家的 `CheckSession` 立即变假，后续查角/选角失败。
- 可达性：LoginSrv 未重启、仅链路闪断/半开时；客户端可见。
- tier R：跨服务状态 + 断线时序；判不准按 R。

### 2.7 LoginSrv 网关条目"复活"竞态（P2，Rust-only）
- Rust：`process_message` 先 `get(connection_id).cloned()`（`bins/login-srv/src/login_server.rs:150-153`），处理完无条件 `insert` 回去（`:177-180`）；网关读任务在 EOF 时 `remove`（`:97`）。两者并发时，已删除的 `LoginGateInfo`（含 `user_list`、`tx`）会被写回，形成永久幽灵条目（`tx` 的写端已无读者，后续 KeepAlive 静默失败）。
- oracle 无等价物：每条消息现查 `_clientManager.GetSession`（`LoginServer.cs:115`），断开时 `_clientManager.Delete`（`:73-79`）。
- 可达性：网关断开瞬间恰有在途帧；客户端本身不可见，但表泄漏与后续同 id 逻辑受影响。
- tier R：竞态（策略明列）。

### 2.8 上行写失败早退（P2，Rust-only）
- Rust 三个网关都在"发 Enter/GM_OPEN"处用 `?`：失败直接返回，跳过后面的 `sessions.remove` + Leave/GM_CLOSE（login-gate `main.rs:179` vs `:216-221`；sel-gate `:180` vs `:217-221`；game-gate `:266` vs `:306-318`）。
- oracle 发送前查 `IsConnected`，失败只是不写（`LoginGate/ClientThread.cs:238-246`、`SelGate/ClientThread.cs:257-264`；GameGate `ClientThread.cs:373-380`），会话仍由断开回调正常清理。
- 可达性：对端不可用窗口内每个新客户端都会留下一条永不释放的会话表项（LoginGate 无重连 ⇒ 永久）。
- tier L：确定性控制流；建议随 P0 卡一起修。

### 2.9 顶号/裸 kick 帧流（P2）
- oracle：LoginSrv 对"已绑定账号再发 CM_IDPASSWORD"走 `KickUser` → `SendGateKickMsg` 直接 `socket.Send("%+-<sock>$")`，**绕过 6B ServerDataPacket 帧**（`ClientSession.cs:767-771,875-879`）。LoginGate 的解析器遇到错误 PacketCode 会逐字节前移重同步（`ClientThread.cs:162-223`）。
- Rust：LoginSrv 同样裸写（`login_server.rs:51-56`，等价）；但 login-gate `downstream_loop` 读 6 字节头、`from_bytes` 失败就整 6 字节丢弃（`main.rs:226-247`），没有逐字节重同步 ⇒ 可能永久失步（此后所有下行被错位解析）。
- 可达性：已登录会话再次提交登录（客户端双击/异常重试）。正常单人流程不可达。
- tier R：只会在真机字节流上暴露，判不准按 R。

### 2.10 多实例 / 异常关闭（登记）
- GateCount>1：oracle 每个 GateCount 一套 `ClientThread`+`SessionArray`+监听（LoginGate `ServerManager.cs:106-120`、SelGate `ClientManager.cs:28-41`、GameGate `ServerManager.cs:36-44`）；Rust 只读 `*0` 一组。现网据 `mir2-rs/docs/M2-GameGate协议梳理.md` §4 为单实例 ⇒ 当前不可达；若配置>1 则 Rust 完全不可用。tier L（配置面）。
- 半开连接：LoginGate oracle 每 10s KeepAlive、30s 无回判死并 `Stop()`（`ClientManager.cs:155-190`、`GateShare.cs:20`），但**不重连**；Rust 既不检测也不重连。对客户端的最终观感接近（都失去服务），差异在 C# 会主动标记断开。tier L/R 边界，建议并入 B-1/B-2 的测试矩阵。
- GameGate 服务端 `GM_CLOSE`：oracle 不处理，Rust 删会话（`main.rs:564-567`）；GameSvr 今日从不发该帧 ⇒ 不可达，仅登记。

## 3. 最值得先做的卡（可派工）

> 写集约定：B-1..B-3、B-6 只动 `bins/login-gate`、`bins/sel-gate`、`bins/login-srv`（本 lane 授权面）；B-4/B-5 动 `bins/game-gate`，与 `/e/tmp/wnum`、`/e/tmp/wpl2` 的 `game-svr/main.rs` 写集不重叠，但需另派 worker 认领。所有卡都不改 `src/**`、`sql/**`、`crates/protocol`、`crates/storage`。

### B-1 [tier R] SelGate 下行重连 + 10s 心跳
- 目标：DBSrv 重启/闪断后，SelGate 自动恢复；与 oracle `TimedService.CheckSessionStatus` 同节拍。
- 写集：`mir2-rs/bins/sel-gate/src/main.rs`、`src/config.rs`（如新增可调周期键，默认 10s）、`tests/relay.rs`。
- 判据草案（机判）：
  1. 进程级：假 DBSrv 接受后立即关闭；断言 SelGate 在 ≤11s 内发起第二次 connect（记录 accept 时间戳），日志含失败次数递增。
  2. 重连后新客户端：`Enter` 体逐字节 `%K{sid}/{ip}/{ip}$` 到达第二条连接；`sessions.len()` 在断线期间不变化（oracle 保留 `SessionArray`）。
  3. 心跳：连接稳定 ≥10s 时，假 DBSrv 每 10±1s 收到一帧 `ServerDataType::KeepAlive`；wire 允许 A-10 的空字段编码（`00000000` vs `ffffffff`），断言只读 kind。
  4. `cargo test -p sel-gate`；真机窗口：重启 C# DBSrv，真客户端仍能取到角色列表。
- tier 理由：重试节拍/断线瞬间表现 = 时序（R）；R 卡合并后保持 `in-progress` 直到窗口证据。

### B-2 [tier R] LoginGate/SelGate 客户端超时会话
- 目标：1:1 复刻两处 60s 超时（含畸形帧与收尾差异）。
- 写集：`mir2-rs/bins/login-gate/{main.rs,config.rs,tests/relay.rs}`、`mir2-rs/bins/sel-gate/{main.rs,config.rs,tests/relay.rs}`。
- 判据草案（机判）：
  1. 配置：读 `[Integer] ClientTimeOutTime`，默认 60000、<10000 时钳到 10000；缺失键用默认（与 C# `ReadWriteInteger` 同口径，但不写回）。
  2. LoginGate：用可注入时钟/缩短超时，断言超时后 (a) 发出的帧逐字节等于 oracle 公式：`'#'` + `Encode(CommandMessage{Ident=SM_OUTOFCONNECTION,Recog/Param/Tag/Series=0}, dstOffset=1)[0..15]`，**无 `!`、少最后一字符**；(b) 客户端 TCP 被关闭；(c) **上游没有 Leave**（oracle 先删会话再关 socket）。
  3. SelGate：超时后 (a) 帧等于 oracle 公式：`Encode(...)[0..iLen-1]`（首字节被编码覆盖，无 `#`）+ 下标 `iLen` 的 `0x00` + `'!'`；(b) 客户端 TCP **仍连着**，后续帧不再产生上游 `Data`；(c) 无 Leave。
  4. 节拍：LoginGate 1s、SelGate 20s；用注入周期断言触发窗口 ∈ [timeout, timeout+period+100ms]。
  5. 真机：真客户端停在登录/选角界面 60s，与 oracle 抓包比帧字节与关闭时刻（±1s）。
- tier 理由：超时时序 + 客户端可见字节，且两处畸形帧必须真机钉住（R）。

### B-3 [tier L，owner 需裁决] LoginGate/SelGate 会话槽位上限与回收
- 目标：在"1:1 复刻 oracle 漏槽 bug"与"登记授权偏离（Rust 无上限、断开即回收）"之间做显式选择；无论选哪条，都要有测试钉住。
- 写集：`mir2-rs/bins/login-gate/src/main.rs`、`mir2-rs/bins/sel-gate/src/main.rs`、两 bin 的 `tests/`，以及（若选偏离）`mir2-rs/tests/parity/whitelist.md` 新增 `A-16`。
- 判据草案：
  - 选项 A（1:1）：按 `MaxSession` 10000/2000 建模；正常断开不回收；LoginGate 满 ⇒ 新连接不建会话且不关 socket；SelGate 满 ⇒ 复用最后一个非空槽（断言 `%K` 的 sid 是陈旧 id、且 `sessions` 被覆盖）。测试注入 `cap=4` churn 5 次，断言第 5 次的可观测行为与 oracle 一致。
  - 选项 B（偏离）：登记"Rust 释放槽位、无硬上限、sid 为 u16 回绕有碰撞风险"；测试：cap+1 次连接/断开后 `sessions.len()==0`，且第 cap+1 次正常登录；补一条 65535 次 sid 分配的回绕用例（断言不覆盖活跃会话，或登记为待修）。
  - 共同：`cargo test -p login-gate`、`cargo test -p sel-gate`；`whitelist.md` 的机判表仍全绿。
- tier 理由：纯计数/分支，无 RNG/时序/落库/数值（L）；但涉及"是否搬 bug"的产品裁决，必须先由 owner 定选项。

### B-4 [tier L] GameGate 会话计数与 SessionId 分配
- 目标：`used` 变为"活跃会话数"，修复 1e4 累计拒绝；同时把 SessionId 碰撞策略显式化。
- 写集：`mir2-rs/bins/game-gate/src/main.rs`、`tests/relay.rs`。
- 判据草案：
  1. 每个 `client_loop` 退出路径（正常 EOF、`Step::Close`、GM_OPEN 写失败、`?` 早退）都必须释放会话计数；用 RAII guard 或统一 cleanup。
  2. 注入 `capacity=4`：顺序 connect/disconnect 12 次全部被接受；4 个并发在连时第 5 个被拒绝；`used.len()==当前活跃数`。
  3. `cargo test -p game-gate`。
  4. SessionId：与 oracle 一致"无唯一性检查"（覆盖即覆盖）或改为碰撞重抽并登记偏离；二选一写进测试注释与报告。
- tier 理由：纯计数/分配逻辑，进程级测试可判（L）。

### B-5 [tier R] GameGate 下行健康/背压帧
- 目标：补 `GM_CHECKCLIENT`(10s)、`GM_RECEIVE_OK` 回显、`GM_STOP` 语义。
- 写集：`mir2-rs/bins/game-gate/src/main.rs`、`tests/relay.rs`、`mir2-rs/docs/M2-GameGate协议梳理.md`（去掉"仍未做"对应项）。
- 判据草案：
  1. 假 GameSvr 侧：稳定连接后每 10±1s 收到一帧 `ident=GM_CHECKCLIENT`、其余头字段全 0。
  2. 发一帧 `GM_RECEIVE_OK` 给网关，断言网关回发同 ident 的 20B 头（oracle `ClientThread.cs:279-280`）。
  3. 收 `GM_STOP` 后：不再对新客户端发 `GM_OPEN`（按 oracle 让客户端保持连接/不建会话），并保留已有会话。
  4. `cargo test -p game-gate`；真机窗口：C# GameSvr + >40KB/轮下行，网关回显计数 = 服务端 `SendCheck` 次数，客户端无 50ms 级停顿。
- tier 理由：时序/协议（R）。

### B-6 [tier R，次批] LoginSrv 网关条目回写竞态
- 写集：`mir2-rs/bins/login-srv/src/login_server.rs`、`tests/login.rs`。
- 判据：用测试钩子让 dispatch 在 clone 后暂停，另一任务触发读任务 EOF 删除；恢复后断言 `gates` 为空、`tx` 被丢弃、无幽灵 `user_list`。`cargo test -p login-srv`。
- tier R：竞态。

### B-7 [tier R，次批] DBSrv LoginSrv 链路断开会话表语义
- 写集：`mir2-rs/bins/db-srv/src/loginsrv.rs`、`tests/`。
- 判据：断开链路不清空 `sessions`（与 C# `ClientSession.cs:64-68` 一致）；重连不重复插入；`(100/…)` 新增正常。测试：先喂 `(100/acc/7/…)`，断开 link，断言 `CheckSession(acc,7)==true`；再重连并喂新会话。`cargo test -p db-srv`。
- tier R：跨服务状态/断线时序。

## 4. 已核对但**不是**缺口的项（避免重复立项）
- GameGate `CheckStep` 状态机（`ClientSession.cs:127-150`）：`SendSmu`/`SendFinsh` 无任何处理器，`SendKickMsg` 的发送体全被注释（`:993-1053`）⇒ 事实惰性；Rust 不实现不构成行为差异。
- GameGate `ClientTimeOutTime`（默认 15s，`Conf/GateConfig.cs:246`）：只读进配置，`src/GameGate` 无引用 ⇒ oracle 无空闲回收；Rust 不做不是差异。
- LoginGate `m_KickFlag` 分支（`ClientSession.cs:142-149,155-165`）：超时路径先 `success=true` 清掉会话，`ProcessSvrData` 的 kick 分支不可达。
- LoginSrv 上的 `Leave` 下行：oracle 的 LoginSrv/DBSrv 从不发送 `ServerDataType::Leave`（全仓 grep 仅接收）⇒ Rust 的"Leave 只摘表不关 socket"不可达。
- LoginGate 不自动重连：oracle 同样不重连（`ClientManager.cs:176-189` 只 `Stop()`，无 `Start()`；已在 `mir2-rs/docs/M1-LoginSrv-交付说明.md:104` 记录）。
- GameGate 服务端 `GM_CLOSE`：oracle 不处理，今天 GameSvr 也不发送。

## 5. 残留物清单
- 本 lane 未创建 worktree、未设 `CARGO_TARGET_DIR`、未启动/停止任何服务或客户端、未修改 `E:/MirServer`。
- 唯一产物：本文件 `E:/tmp/laneB-session-lifecycle.md`。
- 工作区状态：审计前后仓库均无本 lane 的改动（`git status` 不受影响）。
- 审计收尾观察到主检出存在**非本 lane** 的未提交改动：`src/GameSrv/Word/WorldServer.cs`（+20/-2，`HumItems` 空槽补齐与 Necklace/Helmet 交叉修正）；本 lane 全程只读、未触碰，请编排侧核实归属。


---

## 评审员更正（2026-10-20，**以本节为准**）

- **#1 SelGate 不重连**：支持。更正——缺 10s KeepAlive **本身不会**让 DBSrv 判死（DBSrv 只对收到的 KeepAlive 回包，无静默超时），
  P0 因果只有「不重连」；另需裁决「启动时 DBSrv 不在 => Rust 直接失败退出（M1④ fail-loud）」是否保留。
  日志判据应对齐 oracle 原文：`服务器[...]建立链接.失败次数:[N]` / `服务器[...]链接超时.失败次数:[N]`。
- **#2 超时**：基本成立。**判据 off-by-one**：SelGate 超时帧是 16 字节编码 + 下标 16 的 `0x00` + 下标 17 的 `'!'`（共 18B），
  不是 `[0..iLen-1]`；LoginGate 是 16B（`'#'` + 15 个编码字符，无 `!`）。另补：下游未连接时 oracle **早退不发帧但会话仍被关**；
  SelGate `HandleDelayMsg` 的 else 分支（`CloseClient`）因 TimedService 先置空槽而不可达。
- **#3 GameGate `used`**：缺口真实，**量化错误**——`used` 是 `HashSet<u16>`，键为四位十进制 sid，等 0..9999 全出现约需
  9.8×10⁴ 次连接（coupon collector），不是 1e4；且 oracle 槽满时不干净拒绝，而是返回最后一个非空槽。
- **#4 GM 帧**：事实正确；`GM_STOP` 有独立分支（不是落 other），效果同为只日志。
- 其余条目的 tier 与判据修正见评审原文；有两条机判判据按字面无法对正确实现咬绿，派卡时需重写。
