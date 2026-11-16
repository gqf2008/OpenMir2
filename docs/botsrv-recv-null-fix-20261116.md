# BotSrv recv_packet_null 残余：MySelf==null 世界分支门控（2026-11-16）

卡：`openmir2-botsrv-recv-null-residual`（tier=R，保持 open，等下一轮真机窗口复验）。
写集：`src/BotSrv/**` + `docs/**`。**未改动** `recv_packet_null` 的 census 类别定义与白名单。

## 1. 触发链（与静态阶段一致）

1. 软关/离开世界后 `PlayScene.ClearActors()` 把本 bot 的 `RobotPlayer.MyActor` 置空；
2. `ClientManager.ProcessOnePacket` 仍按包设置 `MShare.MySelf = client.MyActor`（= null），
   处理完再恢复；
3. 本 bot 的 `_inWorld` 只在收到 `SM_LOGON` 时置 true、全仓无置 false 的点；软关后残留的
   排队世界帧照常进入 `DecodeMessagePacket` 的世界分支；
4. 动作分支在比较 `msg.Recog != MShare.MySelf.RecogId` 时对 null 解引用，抛
   `NullReferenceException`，被 `ClientManager.RunPacketGuard` 计成 `recv_packet_null`。

## 2. 修复（最小、可证等价）

在四个动作分支的 `RecogId` 比较前加显式 null 门控：

```csharp
if (MShare.MySelf != null && msg.Recog != MShare.MySelf.RecogId)
```

| 分支 | 源码锚（修后行号） | 窗口 ident 计数（真机 10:42 轮） |
| --- | --- | --- |
| `SM_WALK` / `SM_RUN` / `SM_HORSERUN` | `src/BotSrv/Player/RobotPlayer.Operate.cs:497` | 11=257,592；13=1,295；5010=0 |
| `SM_BUTCH` | `src/BotSrv/Player/RobotPlayer.Operate.cs:525` | 637=0 |
| `SM_SITDOWN` | `src/BotSrv/Player/RobotPlayer.Operate.cs:536` | 12=0 |
| `SM_HIT` 族（8 个 ident） | `src/BotSrv/Player/RobotPlayer.Operate.cs:553` | 14=51,433；15=2,255；其余=0 |

语义等价：`MySelf == null` 时旧代码在本行抛异常、整帧被 `RunPacketGuard` 丢弃；
门控后跳过该分支的 `SendMsg`，等价于「丢弃本帧」。`MySelf != null` 时条件与旧代码逐字节相同。

## 3. 离线证据（`--selftest-recv-null-context`）

入口：`src/BotSrv/Program.cs` 的 `--selftest-recv-null-context`；
用例：`src/BotSrv/RecvNullContextSelfCheck.cs`（不启动服务/不连端口）。
构造 `_inWorld=true` + `MyActor==null` 的 per-packet 上下文，经
`ClientManager.ProcessOnePacket` 注入真实编码的帧（16 字符头 + 加密 `CharDesc` body）。

### 3.1 红（修复前，`86f6820b6`）

```
SELFTEST_RC=1
RECVNULL FAIL null-walk-no-recv-error  delta null=1
RECVNULL FAIL null-walk-direct-no-throw  NullReferenceException @ .../src/BotSrv/Player/RobotPlayer.Operate.cs:line 494
RECVNULL FAIL null-run-no-recv-error  delta null=1
RECVNULL FAIL null-hit-no-recv-error  delta null=1
RECVNULL FAIL null-hit-direct-no-throw  NullReferenceException @ .../src/BotSrv/Player/RobotPlayer.Operate.cs:line 550
RECVNULL FAIL null-butch-direct-no-throw  NullReferenceException @ .../src/BotSrv/Player/RobotPlayer.Operate.cs:line 522
RECVNULL FAIL null-sitdown-direct-no-throw  NullReferenceException @ .../src/BotSrv/Player/RobotPlayer.Operate.cs:line 533
RECVNULL SELFTEST FAIL (10)
```

**实测栈行 ↔ 源码行 ↔ 触发分支（离线、同源构建）**：
walk/run → `src/BotSrv/Player/RobotPlayer.Operate.cs:494`，hit → `:550`，butch → `:522`，sitdown → `:533`，
均为对应分支的 `MShare.MySelf.RecogId` 解引用行。

### 3.2 绿（修复后，`074197854`）

```
SELFTEST_RC=0
RECVNULL PASS fixture-chardesc-roundtrip raw=8 enc_len=11 rt=(7,9)
RECVNULL PASS null-walk-no-recv-error  delta null=0 other=0 index=0
RECVNULL PASS null-walk-direct-no-throw  no-exception
RECVNULL PASS nonull-walk-other-sends  actor=True msg=1 ident=11 want=11
RECVNULL PASS nonull-walk-self-skips  selfActor=False targetMsgs=1->1
...（run/hit/butch/sitdown 同形）
RECVNULL SELFTEST PASS
```

非 null 路径等价牙：同一帧 `MyActor != null` 时，`Recog` 不等 ⇒ 目标 actor 恰好收到 1 条
`SendMsg`（ident 与分支一致：walk=11、run=13、hit=14、butch/sitdown=12）；`Recog` 相等 ⇒
不建自己 actor、不追加消息。证明门控只关 null 路径。

### 3.3 反向牙

删除四处门控（sed 还原为旧条件）⇒ 重建后 `SELFTEST_RC=1`、10 条 FAIL（同上）；
恢复文件后 `sha256` 相等、重建后 `SELFTEST_RC=0`。变异只在本人 worktree 内执行。

## 4. 与真机行号的对齐（重要）

真机 10:42 窗口的 4 条 NRE 行号为 `src/BotSrv/Player/RobotPlayer.Operate.cs:493`（3 条）与 `:538`（1 条）。
用运行时 `BotSrv.dll`（sha256 `c5a75d05…`，`binaries.txt` 记录）的内嵌 PDB + IL 反汇编核对：

- `ClientManager.cs:77/93` 与源码完全一致 ⇒ PDB 映射链路本身可信；
- 但 `:493` 是 `desc = EDCode.DecodeBuffer<CharDesc>(body)`，`:538` 是 `SM_SITDOWN` 分支的
  `Actor.SendMsg(...)`；修复前该文件的 `MShare.MySelf` 解引用在 `:494` / `:550`（`SM_SITDOWN`
  解引用在 `:533`；修后因加注释整体后移 3 行 = `:497` / `:553` / `:536`），且窗口
  `ident_counts` 中 `SM_SITDOWN(12)`、`SM_BUTCH(637)` 均为 **0**
  ⇒ `:538` 所指分支在窗口内**不可能执行**。

结论：**真机 493/538 不能按行号直接归因到 MySelf**，与离线复现实测的 494/550 对不上。
离线证据只证明「MySelf==null 时这四类帧必 NRE」这一可证事实；窗口那 4 条是否全部属于该
类别，需下一轮窗口以「修复后 `recv_packet_null=0`」判定。行号漂移的最可能解释是该轮二进制
的调试行表与源码不完全同源（增量构建/旧 PDB），未取得现场 PDB 无法进一步证明。

## 5. 同族排查：`DecodeMessagePacket` 内全部 `MShare.MySelf.` 解引用

窗口 ident 计数取归档 `E:/UserFleet/load-360-p1-verify-20261116/tier-500/load_stats.ndjson`
末行。**加粗** = 本轮已加门控；其余**未改**（按卡面要求只修可证同一根因的）。

| 源码锚（修后） | 分支 | 窗口计数>0 | 处置 |
| --- | --- | --- | --- |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:384` | `SM_TURN` 带名尾部（`MySelf.UserName`） | 10=5,038,528 | 未改：需 `data` 非空 + Actor 命中，列清单 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:387` | `SM_TURN` 带名尾部（`MySelf.SlaveObject`） | 10=5,038,528 | 同上 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:389` | `SM_TURN` 带名尾部（`MySelf.SlaveObject[i]`） | 10=5,038,528 | 同上 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:397` | `SM_TURN` 带名尾部（`MySelf.SlaveObject.Add`） | 10=5,038,528 | 同上 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:436` | `SM_SPACEMOVE_HIDE/HIDE2` | 800/806=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:459` | `SM_SPACEMOVE_SHOW/SHOW2` | 801/807=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:477` | `SM_RUSH/RUSHKUNG` | 6/7=0 | 未改（本轮未命中） |
| **`src/BotSrv/Player/RobotPlayer.Operate.cs:497`** | **`SM_WALK/RUN/HORSERUN`** | **11=257,592** | **已加门控** |
| **`src/BotSrv/Player/RobotPlayer.Operate.cs:525`** | **`SM_BUTCH`** | 637=0 | **已加门控**（与真机 :538 同名族） |
| **`src/BotSrv/Player/RobotPlayer.Operate.cs:536`** | **`SM_SITDOWN`** | 12=0 | **已加门控**（真机 :538 的 PDB 指向） |
| **`src/BotSrv/Player/RobotPlayer.Operate.cs:553`** | **`SM_HIT` 族** | **14=51,433 / 15=2,255** | **已加门控** |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:632` | `SM_SKELETON/ALIVE/ABILITY`（`m_nGold`） | 52=500 | 未改：同一 null 机制，可证但未在真机栈出现 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:633` | 同上（`Job`） | 52=500 | 未改 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:634` | 同上（`m_nIPowerLvl`） | 52=500 | 未改 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:635` | 同上（`m_nGameGold`） | 52=500 | 未改 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:637` | 同上（`Abil`） | 52=500 | 未改 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:658` | `SM_DAYCHANGING/WINEXP`（`Abil.Exp`） | 46=500 | 未改 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:665` | `SM_LEVELUP`（`Abil.Level`） | 45=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:846` | `SM_HIDE/GHOST/DISAPPEAR` | 30=6,279 | 未改：同一 null 机制，可证但未在真机栈出现 |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:912` | `SM_TAKEON_OK`（`m_nFeature`） | 615=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:913` | `SM_TAKEON_OK`（`FeatureChanged`） | 615=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:931` | `SM_TAKEOFF_OK` | 619=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:932` | `SM_TAKEOFF_OK` | 619=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:950` | `SM_WEIGHTCHANGED` | 622=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:951` | `SM_WEIGHTCHANGED` | 622=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:952` | `SM_WEIGHTCHANGED` | 622=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:955` | `SM_GOLDCHANGED` | 653=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:957` | `SM_GOLDCHANGED` | 653=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:959` | `SM_GOLDCHANGED` | 653=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:960` | `SM_GOLDCHANGED` | 653=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:1104` | `SM_USERSELLITEM_OK` | 648=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:1135` | `SM_USERREPAIRITEM_OK` | 669=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:1204` | `SM_BUYITEM_SUCCESS` | 650=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:1224` | `SM_MAKEDRUG_SUCCESS` | 713=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:1366` | 组队/交易菜单族 | 666/667/673/674/681…=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:1413` | `SM_DEALCHGGOLD_OK` 族 | 682/683/684=0 | 未改（本轮未命中） |
| `src/BotSrv/Player/RobotPlayer.Operate.cs:1418` | `SM_DEALCHGGOLD_FAIL` | 685=0 | 未改（本轮未命中） |

**下一轮窗口仍需观察的两类（本轮窗口有流量、未加门控）**：`SM_TURN` 带名尾部（10）、
`SM_DISAPPEAR`（30）、`SM_ABILITY`（52）、`SM_DAYCHANGING`（46）。若复验仍有
`recv_packet_null`，优先按这四处复核（同一 null 机制，只是本轮真机栈未体现）。

## 6. 其余可疑点（未改，只登记）

- `src/BotSrv/Player/RobotPlayer.Operate.cs:493`（真机栈指向处）：`EDCode.DecodeBuffer<CharDesc>(body)`。
  离线 MySelf==null 用例证明**有效 body 下它不抛**；只有 body 非空但解码结果不足/畸形时才可能
  在 MemoryPack 反序列化里失败，异常类型与具体 body 字节相关。归档只有日志、没有现场帧字节，
  无法判成 NRE，故**不改**；若要闭死，需要下一轮窗口带 `partial_frames`/body 长度采样。
- `src/BotSrv/Player/RobotPlayer.Operate.cs:538`（真机栈指向处）：`SM_SITDOWN` 的 `Actor.SendMsg(...)`。
  `Actor` 已在 `:535` 做过非 null 判断，`TActor.SendMsg` 只做 `m_MsgList.Add`，`m_MsgList`
  在 `TActor` 构造器初始化且全仓无置 null 点 ⇒ 未找到 null 解引用路径；且该分支 ident 12
  在窗口计数为 0。故**不改**。

## 7. 未做项

1. 未跑真机窗口：`recv_packet_null` 是否归零需下一轮 500 档 ≥360s 复验；
2. 未改 `SM_TURN`/`SM_DISAPPEAR`/`SM_ABILITY`/`SM_DAYCHANGING` 等本轮有流量但真机栈未体现的
   同族解引用（按卡面「只修可证同一根因」）；
3. 未改 census 类别与白名单；未删任何归档；
4. 未提交 `bin/`/`obj/`（构建产物不进仓）。

## 8. 写集与门禁

- 提交：`86f6820b6`（test：selftest + 入口）、`074197854`（fix：四处门控）；
- `git diff --numstat 83e214ba8..074197854`：仅 `src/BotSrv/**`（见 patch entry）；
- `dotnet build src/BotSrv -c Release`：0 error；`--selftest-recv-null-context`：rc=0；
- `docs_anchor_gate --prose --base master`：见 patch entry。
