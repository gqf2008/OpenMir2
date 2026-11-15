# BotSrv 收包线程 NRE 残余：静态归因与窗口验证方案

> 卡：`openmir2-botsrv-recv-null-residual`（tier=R，`window_required`）。
> 本文件是**静态阶段**的交付物：候选归因清单 + 可判的验证方案 + 最小加固建议。
> **本轮未开真机窗口、未取到 `type`/`stack_head`**，因此卡面验收第 1 条（类型+堆栈首行指到行）
> 仍未满足，卡片不能收口。**本轮不改 `recv_packet_null` 的 census 类别定义，不改白名单，未改代码。**
>
> **卡片状态：open（needs-review）。按总监口径，无 `type`/`stack_head` 不许 closed；**
> **本文件是静态阶段交付，不是「已修复」结论。**

## 1. 结论先行

1. 共享态修复（master `7e8108572`）把 `MShare.MySelf` 从跨 bot 全局改为 **per-bot、per-thread 上下文**；
   残余 NRE 不再是「读到别人/被并发改坏」，而是 **上下文诚实地为 `null` 时，世界帧分支没有门控**。
2. 产生 `null` 上下文的**唯一活跃路径**是 `AppLogout()` → `PlayScene.ClearActors()` → 本 bot 的
   `MyActor = null`；`_inWorld` 仍为 `true`，`ProcessOnePacket` 之后每个包都会把
   `MShare.MySelf` 置为 `null`。
3. 归档证据链已闭合到「软关（CM_SOFTCLOSE）之后仍有排队帧被投递」这一形态：
   失败轮 3 次 `CM_SOFTCLOSE` ↔ 3 个 bot 日志流停止；唯一 NRE 落在其中两次软关之间（+0.85s）；
   干净轮 **0 次软关、0 次 NRE**。**NRE 的具体行号仍需下一窗口的堆栈确认**，但触发类（软关后的
   残留世界帧）已经可以用服务器侧日志判出来。
4. 由此得到验证口径的关键一条：**`recv_packet_null=0` 本身不能区分「已修」与「未触发」**——
   先要证明窗口里出现了「软关 + 残留帧」这一暴露类（下一窗口要求 ≥1 次 `CM_SOFTCLOSE`，
   或用建议的暴露计数），否则 0 次只说明没测到。

## 2. 机制

收包线程每个包（`src/BotSrv/ClientManager.cs:74`）：

```
THumActor prev = MShare.MySelf;
MShare.MySelf = client.MyActor;      // ← per-bot 上下文；MyActor 可为 null
try { client.ProcessPacket(...); } finally { MShare.MySelf = prev; }
```

`client.MyActor` 的写入点只有三处：

| 写入 | 值 | 位置 |
| --- | --- | --- |
| `SM_LOGON` 进世界 | `(THumActor)actor` | `src/BotSrv/Scenes/Scene/PlayScene.cs:1315` |
| `ClearActors()` | `null` | `src/BotSrv/Scenes/Scene/PlayScene.cs:1142` |
| 构造默认 | `null` | `src/BotSrv/Player/RobotPlayer.cs:313` |

`ClearActors()` 的活跃调用者只有 `AppLogout()`（`src/BotSrv/Player/RobotPlayer.cs:2768`）；
`ChangeServerClearGameVariables()`（`src/BotSrv/Player/RobotPlayer.cs:4641`）的触发标志
`g_boServerChanging` 在本仓**没有任何置 true 的存活写点**，不算活跃路径。

`AppLogout()` 的活跃调用者中，收包线程可达的是 `SM_STRUCK` 的「自己被打 + 自动挂机」小退分支
（`src/BotSrv/Player/RobotPlayer.Operate.cs:706` 的条件 → `:730` 的 `AppLogout()`）。

此后：`AppLogout` 只发 `CM_SOFTCLOSE`（**静默**，无日志），服务器侧「离开世界」但**不立刻断 TCP**
（失败轮全程 0 条「掉线或断开」/「退出游戏」）；bot 的 `_inWorld` 仍是 `true`、`MyActor` 已是 `null`。
服务器/网关的排队帧（失败轮 `gate_frame_queued` 出现 360 次，单次 `hold_frames=58180`）陆续投递到
该 bot 的 socket；`DecodeMessagePacket` 里 `if (!_inWorld) { … }` 之后**没有 return**，世界分支照常执行，
于是命中未加 null 门控的 `MShare.MySelf.*` 解引用 → `NullReferenceException` → `RunPacketGuard`
计数 `recv_packet_null`。

**为什么共享态修复后仍可能发生**：修复前 `MShare.MySelf` 是全局静态，别的 bot 的 actor 会「兜住」
空值——同一个残留帧读到的是**别人的 actor**（错误但非 null，不会 NRE）。修复后 per-bot 上下文
**正确地变成 null**，同一个时序窗就从「静默错读」变成「显式 NRE」。这就是「降低但未归零」的
机制解释：共享态修复消掉的是别的竞争（6072→0），留下的是这一条从未被门控的 null 上下文窗口。

## 3. 归档证据（只读，未新开窗口）

| run | verdict | `recv_packet_null` | 服务器侧 `CM_SOFTCLOSE` | 触发类 ident（14/15/30/31/34） | 说明 |
| --- | --- | --- | --- | --- | --- |
| `E:/UserFleet/load-actorpool500-20261115-rust-fix360b/` | **RED** | **1** | **3**（`game-svr.out.log` 67232 / 70171 / 70859） | 73577 / 2554 / 3580 / 30195 / 2035 | 唯一 NRE：`tier-500/bots.log:47251`，20:38:37.120 |
| `E:/UserFleet/load-360-errorstack-20261115-2116/` | GREEN | 0 | **0** | 28400 / **0** / 0 / 3758 / **0** | 360s、500/500 进世界、`typed_stacked_blocks=0`（不适用） |

失败轮的三次软关与三个「日志流中途停止且不再恢复」的 bot **一一对应**：

| bot | 最后一条 bot 日志 | 服务器侧软关（同一时钟，偏移 8h） |
| --- | --- | --- |
| loadbot81 | 20:38:30.417 | 12:38:31.263 |
| loadbot44 | 20:38:35.490 | 12:38:36.270 |
| loadbot73 | 20:38:36.318 | 12:38:37.271 |

唯一 NRE 20:38:37.120 落在 loadbot44 软关 **+0.85s**、loadbot73 软关 **-0.15s** 之间；同秒
`game-svr.out.log:70216` 记录 `gate_frame_queued … hold_frames=58180`（网关背压、帧被持有后延迟投递）。
软关后 bot 侧没有任何断开/退出日志（全程 0 条），与「离开世界但连接未关」一致。
干净轮既 0 次软关、也 0 次高烈度战斗类 ident（15/34 为 0），属**触发类缺席**，不是「已修」的证据。

局限（必须同时读）：① 三个 bot 的日志流停止也可能只是进入「追击」这种静默挂机态，不能单凭
日志停止证明软关与 NRE 的因果；本表的因果证据是**服务器侧 `CM_SOFTCLOSE` 时间与 NRE 时间窗**，
加上干净的 0/0 对照。② 无法确定是哪一个 bot 的会话抛的 NRE（缺 per-bot 帧归属）。
③ NRE 的具体解引用行仍未取得（该轮二进制不含 errorstack 修复）。

## 4. 候选清单（静态，按证据强度排序）

观察到的 ident 直方图（失败轮）里，真正被执行的世界帧只有：
9/10/11/14/15/30/31/34/41/42/46/50/51/52/100/654/656/657/658/752/811/1106 及登录期 520/525/529/530。
下列 P0 是**这些帧里仅有的、未加 null 门控且会解引用 `MShare.MySelf` 的站点**。

| 级别 | 站点 | 解引用 | 触发条件 | 本窗口是否出现 |
| --- | --- | --- | --- | --- |
| **P0** | `src/BotSrv/Player/RobotPlayer.Operate.cs:494` | SM_WALK/RUN/HORSERUN：`msg.Recog != MShare.MySelf.RecogId` | `MyActor == null` 且收到走/跑帧（含**别人**的走跑帧） | ident 11 = 213,427 |
| **P0** | `src/BotSrv/Player/RobotPlayer.Operate.cs:550` | SM_HIT/HEAVYHIT/…：`msg.Recog != MShare.MySelf.RecogId` | 同上，收到攻击帧 | 14 = 73,577；15 = 2,554 |
| **P0** | `src/BotSrv/Player/RobotPlayer.Operate.cs:843` | SM_HIDE/GHOST/DISAPPEAR：`MShare.MySelf.RecogId != msg.Recog` | 同上，收到隐身/消失帧 | 30 = 3,580 |
| P1 | `src/BotSrv/Player/RobotPlayer.Operate.cs:384`（`:387`/`:397` 同分支） | SM_TURN 带尾部名字：`MShare.MySelf.UserName` / `SlaveObject` | `body.Length > 11`（带 name 后缀）且 `MyActor == null` | ident 10 极高频，带尾部名字的比例未知 |
| P2（潜在） | `src/BotSrv/Player/RobotPlayer.Operate.cs:436` / `:459` / `:477` / `:522` / `:533` | SM_SPACEMOVE_HIDE/SHOW、SM_RUSH、SM_BUTCH、SM_SITDOWN 的 `MySelf.RecogId` | 同上 | ident 6/7/12/637/800/801/806/807 均未出现 |
| P2（潜在） | `src/BotSrv/Player/RobotPlayer.Operate.cs:629`、`655`、`662` | SM_ABILITY/WINEXP/LEVELUP 的 `MySelf.*` | 仅当这些帧先于本 bot 的 `SM_LOGON` 或软关后到达 | 52 = 500（均在 LOGON 之后）；44/45 未出现 |
| P2（潜在） | `src/BotSrv/Player/RobotPlayer.cs:5224` / `:5261`（`ActionFailed` / `RecalcAutoMovePath`） | SM_MOVEFAIL 无门控解引用 `MySelf` | 软关后收到 SM_MOVEFAIL | ident 28 未出现 |
| P2（潜在） | `src/BotSrv/Player/RobotPlayer.cs:5335`（`ClientGetGameGoldName`） | SM_GAMEGOLDNAME 解引用 `MySelf` | 如题 | ident 5008 未出现 |
| P2（潜在） | `src/BotSrv/Player/RobotPlayer.Operate.cs:507` | `MShare.UseItems[13].Item.Name`（SM_LAMPCHANGEDURA） | `UseItems` 数组元素是 null | ident 655 未出现；`SM_SENDUSEITEMS`（621）也未出现 |
| P2（潜在） | `src/BotSrv/Player/RobotPlayer.cs:5389`、`:5396`、`:5402`（UPDATEITEM）、`:5430`（DELITEM）、`:5470`（DELITEMS）、`:983`（EAT_OK） | `UseItems[i]` / `ItemArr[i]` / `g_TIItems[i]` / `g_SellDlgItem` 无 null 检查 | 对应帧到达且槽位为空 | 202/203/635/709 未出现；两轮都 0 次背包帧（201/621），数组全空 |
| P2（真 bug，非 NRE） | `src/BotSrv/Player/RobotPlayer.Operate.cs:412`、`:448` | `body.Substring(n, body.Length)`，`n = GetCodeMsgSize(8*4/3) = 11` | `body.Length > 11` ⇒ **必抛** `ArgumentOutOfRangeException`（计 `recv_packet_index`，不是 null 类） | 未出现；`src/BotSrv/Player/RobotPlayer.Operate.cs:1579` 同写法但 `n` 未赋值（=0）故安全 |
| P3 | `src/BotSrv/Player/RobotPlayer.Operate.cs:855`（SM_DIGUP）；`src/BotSrv/Scenes/Scene/PlayScene.cs:1304`/`:1355` | `FindActor==null` 后 `NewActor` 可能返回 null（`src/BotSrv/Scenes/Scene/PlayScene.cs:700` 的换脸分支） | `BotHelper.IsChangingFace(chrid)` 为真 | ident 20 / 1104 未出现 |
| P3 | `EDCode.DecodeBuffer<T>(body)` / `DeCodeString(body)` 空体 | 抛 `ArgumentNullException` | 带体帧的 body 为空 | 计 `recv_packet_other`/index，**不产生 NRE**；仅用于口径区分 |

**已排除**（避免重复调查）：

- `DecodeMessagePacket` 的 `datablock[0]`：调用前已 `!string.IsNullOrEmpty(data)` 门控；头部切片
  `datablock[..Messages.DefBlockSize]` 有长度判断。
- `PlayScene.FindActor` / `SnapshotActors` / `NewActor` 的 ActorList 并发：A6 已加 `_actorSync` +
  快照；列表没有写入 null 的路径；并发缩表在 `BeginScene` 侧另有计数（run 线程）。
- `TActor.SendMsg` / `UpdateMsg`：只把 `TChrMsg` 入队，自己的 `this == MShare.MySelf` 是**比较**不是解引用。
- `AbilityCodec.Decode` / `ClientItemCodec.Decode`：非法长度走 `ArgumentException` /
  `InvalidDataException`（other/index 类），不会伪装成 NRE。
- 登录/选角场景处理器（`LoginScene` / `SelectChrScene`）不读写 `MShare.MySelf`。
- `TMap.LoadMap`：失败轮 500/500 次 `SM_NEWMAP` 均通过，无 NRE。
- `SendSocket`：`ClientSocket` 在 `SocketEvents()` 里先创建后连接，收包路径上非 null。

## 5. 下一窗口验证方案（可判）

### 5.1 采什么

1. **二进制**：含 errorstack 修复的树（`4bde30f13` 之后）；核 `binary_stamp` 严格等于窗口 head。
2. `tier-500/bots.log` 全文：`[ERR]` 消息行 + 紧跟的类型行 + `   at ` 帧（`load_report.py` 的
   `summarize_internal_error_log` 会给出 `typed_blocks` / `typed_stacked_blocks` / `stack_head`）。
3. `tier-500/load_stats.ndjson`：`error_census`（逐类）+ `ident_counts`（覆盖门）。
4. `stack-logs/game-svr.out.log`：`CM_SOFTCLOSE：ChrName=…` 与会话/网关背压
   （`gate_frame_queued` / `gate_frame_drop` 的 `hold_frames`、`session`、`ident`）。
5. `tier-500/report.json`：`verdict` / `fails` / `internal_error_diag` / `inworld_gate` /
   `counters_target`（采集时 provenance，不能是 post-hoc）。
6. **per-bot 帧归属**：本轮没有 `frames.ndjson`。下一窗口至少要给「每个 session 最后一个被处理的
   ident + 时间」；有此件才能把 NRE 钉到具体 bot（否则只能靠服务器侧时间窗）。建议由
   `load_gate` 落 `per_session_last_ident`（见 5.3/5.4）。

### 5.2 覆盖率闸（补充判据；否则 0 次不可判）

**主要验收手段是 5.4 的离线确定性自检；本覆盖率闸是补充，不是替代。**
窗口必须同时满足，才允许给出「本轮未观测到」的结论：

- `in_world ≥ 450/500`（卡面验收同口径）、`window.conclusion_valid=true`；
- **≥1 次 `CM_SOFTCLOSE`**（或建议的 bot 侧软关计数 ≥1）——干净轮 0 次软关正说明
  「触发类缺席」；
- 战斗类覆盖达到失败轮量级：`ident 31 (SM_STRUCK) ≥ 30,000`、`ident 14 (SM_HIT) ≥ 70,000`、
  `ident 15 (SM_HEAVYHIT) ≥ 2,000`、`ident 30 ≥ 3,000`、`ident 11 ≥ 200,000`（单轮 500 档 360s）。

任一条不满足 ⇒ 报告写「**未触发（覆盖不足：缺 X）**」，不得写「已修/未复现」。

### 5.3 判定矩阵（类型 + 统计）

| 观测 | 判定 |
| --- | --- |
| `internal_errors>0` 且 `typed_stacked_blocks>0` | 直接可判：`stack_head[0]` 必须指到本表某一行的文件:行。命中 ⇒ 归因闭合，进入修复/容忍结论；不命中 ⇒ 新候选，按同法补清单（原变异必须重放）。 |
| `internal_errors=0` 且覆盖闸不满足 | **未触发**，不能判「已修」；补覆盖或延长窗口。 |
| `internal_errors=0` 且覆盖闸满足、暴露计数（5.4 建议）为 0 | 可写「本轮未观测到；暴露窗口未出现」——**仍不等于 NRE 已修**（窗口可能恰好没有不安全 ident 在队列里）。 |
| `internal_errors=0` 但暴露计数 >0 | 「危险状态发生过但未命中」⇒ 不能判已修；必须先修/门控，再谈窗口。 |

**统计口径（为什么不能只靠 0 次）**：观测到的事件数为 4 / 1 / 0（三轮不同树、不同战斗强度，
不可合并）。

> **「单窗口 0 次只能给出 λ≤3.0/窗口 的置信上界，统计上无法单独判定『已修』。」**

按泊松近似，一个 360s 窗口 0 事件只能给出一侧 95% 上界 λ≤3.0/窗口；要把 λ 压到
≤0.1/窗口需约 30 个干净窗口（`1-exp(-λK)=0.95 ⇒ λK≈3`）。因此**结论必须落在机制层**：
要么拿到堆栈并修（或白名单），要么用暴露计数证明「软关后残留帧不再进入世界分支」；
单纯「再多跑几个 0 次窗口」不构成证据。

### 5.4 确定性自检（**主要验收手段**；替代「等自然触发」）

新增 `BotSrv.exe --selftest-recv-null-context`（**本轮不实现**，需 dotnet 构建与评审）：

- 构造 `RobotPlayer`，令 `MyActor = null`、`InWorld = true`；用现有 `EDCode.EncodeMessage`
  组帧，依次经 `ClientManager.ProcessOnePacket` 注入 `SM_WALK(11)` / `SM_HIT(14)` /
  `SM_DISAPPEAR(30)`，断言 `BotErrorCensus.Get(recv_packet_null)` 每次 +1 且异常不逃逸
  ——**离线复现当前失败类**（红→红）。
- 负对照：同一组帧、`MyActor` 非空 ⇒ 计数 0（证明不是恒真）。
- 若落地 H1 门控：同一组帧 ⇒ 不再计 `recv_packet_null`，改计新的跳过计数；把门控删掉 ⇒ 自检红。
- 这套自检解决「0 次 vs 1 次」的判别困难：它证明**路径存在且被计数**，而不依赖窗口运气。

## 6. 可选最小加固（只写建议，本轮不动代码）

| 编号 | 建议 | 行为等价性 | 处置 |
| --- | --- | --- | --- |
| H1 | 在 `DecodeMessagePacket` 的世界分支（hooks 与世界 switch 之前）加 `if (MShare.MySelf == null) { 计跳过; return; }` | **未证明**。仓内唯一旁证是 `src/BotSrv/Player/RobotPlayer.Operate.cs:1602` 的注释 `// if g_MySelf = nil then Exit;`（它在 default 分支，不是全局门控）；冻结客户端源码不在本仓，无法逐字节比对 | **本轮不做**；落地前必须拿冻结客户端同位置证据 + 自检 + 负对照 |
| H2 | 软关后在 `ClientManager.ProcessOnePacket` 丢弃该 bot 的世界帧（会话标记 closing） | 明确改变行为（现在会走世界分支并 NRE/错读） | 不做；作为 H1 的替代方案登记 |
| H3 | 把 `src/BotSrv/Player/RobotPlayer.Operate.cs:412`/`:448` 的 `Substring(n, body.Length)` 改为 `body[n..]`（与 `SM_TURN` 同写法） | 修的是**真 bug**（当前必抛 AOORE）；但会改变可观测行为（从异常变为解码使用尾部数据） | 不做；属「修 bug」而非「不改行为的防御」，另开小卡带回归测试 |
| H4 | 给 `UseItems` / `ItemArr` / `g_TIItems` / `g_spItems` / `g_SellDlgItem` 的读点加 null 检查 | 对非空槽逐字节不变；对空槽把「异常」变成「跳过」，必须同时计数，否则掩盖问题 | 不做；`ItemArr/UseItems` 已被 `botsrv-shared-state-locks` 登记为 P2 物品卡 |

判定规则（本卡约束）：**只有能证明「防御前后可观测行为逐字节相同」才动代码**；H1/H3/H4 都做不到
该证明（H1 缺 oracle 证据、H3/H4 改变异常面），因此本轮只出建议。

## 7. 本轮没有做的事（与原因）

1. **没开真机窗口**（总监规定今晚不开）⇒ 没有 `type`/`stack_head`，卡面验收第 1 条仍未满足，
   本卡维持 open/needs-review，不能 closed。
2. **没跑 `cargo`/`dotnet` 构建**（机器归他人；`dotnet` 需先问）⇒ 第五节的自检与 H1–H4 均未实现、
   未验证。
3. **没改任何代码**：H1 缺冻结客户端证据、H3/H4 改变异常面，均不满足「逐字节等价」；`census`
   类别定义与白名单一字未动（卡面验收第 3 条）。
4. **没有把 0 次窗口当「已修」**：干净轮 0 次软关、0 次高烈度战斗帧，属触发类缺席，只是
   「本轮无暴露」。
5. **没有删除/清理任何归档**：只读 `E:/UserFleet/**` 与 master 树上文件。

## 8. 写集

```
docs/botsrv-recv-null-residual-static-20261115.md   （本文件，新增）
```

无 `src/**` 改动，无协议/帧/golden 改动，无 `load_report`/census schema 改动。

## 附录：local collab ref 数量过大时的逃生路径（可复跑；**不拷贝私钥**）

**症状**：主检出本地 `refs/collab/*` 数量过大时（2026-11-15 实测 15,718 条 blob ref，其中
`refs/collab/ci-artifacts/**` 占绝大多数），`wg collab board/thread/entry` 全部挂起并以
`GitFailed` / `RegistryReadFailed` 失败；同仓 `git fetch`/`ls-remote` 正常。此时**不要把
identity/keys 拷贝到临时克隆**（硬规则：私钥永不复制；涉及密钥材料的操作先报后做）。

**规避原理**：新建一个**共享对象库**的临时克隆（refs 从 0 开始），用 `--key` 指向**规范 key 路径**
（不复制），只拉元数据与我方 inbox refs，签名/推送后把新 ref 拉回主检出。

```sh
REPO=E:/Users/gxh/Documents/GitHub/OpenMir2
WT=$REPO/.worktrees/worker-87
CLONE=$TEMP/wg-escape            # 临时目录，用后删除
KEY=$REPO/.git/worktrees/worker-87/walgit/keys/openmir2-worker-87.ed25519
PRINCIPAL=openmir2-worker-87
THREAD=<thread-id>
PARENT=<script-extracted entry oid>   # 用 collab_read.py --json thread 取值，禁键盘输入

# 1) 共享对象库的临时克隆；--no-checkout；refs 从 0 开始（不复制主仓的 15k refs）
git clone --shared --no-checkout "$REPO" "$CLONE"
git -C "$CLONE" remote add walgit http://127.0.0.1:8081/gqf2008/OpenMir2.git

# 2) 只拉签名/发布所需的 ref 面（保持 ref 数最小）
git -C "$CLONE" fetch walgit   "+refs/collab/meta/*:refs/collab/meta/*"   "+refs/collab/inbox/$PRINCIPAL/*:refs/collab/inbox/$PRINCIPAL/*"

# 3) 用 --key 指向规范 key 路径（不复制 identity/keys；身份由 --actor 显式给出）
BODY=$(python -I -c "import json,io,sys; print(json.dumps(json.load(io.open(sys.argv[1],encoding='utf-8')),ensure_ascii=False,separators=(',',':')))" body.json)
WALGIT_LOCAL_COMMANDS=1 wg collab entry --repo "$CLONE"   --kind patch --id "$THREAD" --actor "$PRINCIPAL" --key "$KEY"   --parent "$PARENT" --body "$BODY" --push walgit

# 4) 拉回主检出并读回验证（verified=true 才算数）
git -C "$REPO" fetch walgit "+refs/collab/inbox/$PRINCIPAL/*:refs/collab/inbox/$PRINCIPAL/*"
WALGIT_LOCAL_COMMANDS=1 wg collab thread "$THREAD"   # 在 $CLONE 内跑（主检出仍会挂）

# 5) 清理：只删本线临时目录（含 body 文件）；不动主仓 refs、不动规范 key
rm -rf "$CLONE" <body-file>
```

**注意**：
- `--shared` 让临时克隆通过 alternates 复用主仓对象库，parent/blob oid 可直接解析；**不复制对象**。
- 若 `wg` 不支持 `--key`，替代方案是「在本人 worktree 内限制可见 ref 数」（例如把临时克隆的
  `refs/collab/**` 保持为空、只按需 fetch 单条），**仍然不许拷贝身份/密钥**。
- 主检出的本地 collab refs 与远端逐条 sha 一致（2026-11-15 审计
  `E:/UserFleet/archive/collab-refs-audit-20261115/`，`local_only=0`）⇒ 不需要（也不获授权）删除
  本地 refs 来「修复」wg。
- 本条路径待 `wg` 恢复后作为**签名 comment** 补到卡片线程；本轮不签（签名能力受损，且涉及密钥
  材料的操作先报后做）。

**风险边界（2026-11-15 总监广播，硬规则）**：

1. **禁止手工签名 + `git update-ref` / `git hash-object` 直写 `refs/collab/*`。** 逃生路径只有
   合规版：低 ref `clone --shared` + `--key <规范密钥路径>`，**绝不复制 identity/keys**；
   **执行前先报总监**（做什么 / 在哪 / 怎么清 / 替代方案）。
2. **例外处理**：条目先隔离、报总监复核；**「事后验签通过」不是绕道的许可**。
3. **密钥材料零复制**：涉及密钥的操作先报后做；本文附录中任何「拷贝 key/identity 到临时仓库」
   的旧做法一律作废。
