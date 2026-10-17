# 界定件：登录公告等待环、`CM_LOGINNOTICEOK`/`ClientTick` 与 `DoSpell` 版本门（A/B/C 三方案）

- **性质**：设计/界定件（scoping），**不实现任何生产代码**。
- **基线**：worktree `E:/tmp/impl-loginnotice2`，`git worktree add … 0c4fe0e5d --detach`（设计初稿于 `a9c2a4c45`；
  本件所有 oracle 行号在 `0c4fe0e5d` 复测）。
- **纪律**：只读 oracle 与 Rust 源码；本件写作用 `grep`/`sed` 实测行号；未跑 `cargo`、未起服务端/客户端、
  未占端口、未写数据库；未改任何既有文件（唯一新增 = 本 md）。
- **引用纪律**：oracle 一律 `文件:行号`（写作时实测，复核命令见 §7）；Rust 侧**只引符号/函数名**，不写 `:行号`
  （`bins/game-svr/src/*` 会随并行车道位移）。
- **判级**：本件自身 **L**（文档/夹具级）。§3 三个方案的**预判**：A=R、B=R、C=R（理由逐条见 §3；判不准按 R 的保守口径）。
- **一句话**：oracle 用 `658 → CM_LOGINNOTICEOK(1018) → 进世界` 的等待环把 `UserLogon()` 推迟到客户端确认；
  冻结客户端实际把 `1018.Recog` 填 **0**（tick 放在 `Series=46`，oracle 不读），故 oracle 侧 `ClientTick` 也恒 0，
  版本门实际只由 `SoftVersionDateEx != 0` 与 `MagicId <= 80` 两条决定。宿主当前把等待环摊平成「658 后立即连发
  51…46」并忽略 1018，e2e 仍绿。**推荐 B1**：接 1018 只记 `ClientTick`（不阻塞、不发帧）+ 接全量版本门；
  **不要**按 `RunNotice` 字面回 SysMsg——oracle 自己的 drain 循环会把该帧吞掉（§1.4，机证见 §1.8）。

---

## 0. 结论速览

| # | 命题 | 机器判据 | 结论 |
| --- | --- | --- | --- |
| P1 | oracle 在 `LoginNoticeOk=false` 时**不调** `UserLogon()`；先发 658，收到 1018 后才进世界 | `WorldServer.cs:650-659`；`PlayObject.cs:1094-1115`；真 oracle 捕获 `658 < 1018 < 51` | **成立**（§1.2/§1.8）|
| P2 | `WorldServer.cs:440-455` 的 `LoginNoticeOk = true` 在 **switch-data 支**，不是「新角色/首次登录」支 | `WorldServer.cs:246`（`switchDataInfo == null` 支只置 `BoReadyRun=false`，见 `:405`）；`:410` 起才是 else（switch-data），`:449-451` 置三个字段 | **订正任务书表述**（§1.6）|
| P3 | 冻结客户端的 1018 里 `Recog=0`、`Series=46`；oracle 取 `nParam1 = Recog` ⇒ `ClientTick = 0` | 捕获 `c3` seq15 / `run-b` seq3；`WorldServer.cs:856-858` 的 default 映射；`PlayObject.cs:1112` | **成立**（§1.8）|
| P4 | `RunNotice` 自己的 `while (GetMessage)` 会把刚 `SysMsg()` 入队的 `RM_SYSMESSAGE` 取出并丢弃 ⇒ 线上**没有** ClientTick 提示帧 | `PlayObject.cs:1106-1115` + `BaseObject.Message.cs:181/334-355`；C# 捕获 game hop 的 100 帧计数与内容 | **成立**（§1.4/§1.8）|
| P5 | 宿主未登记/未处理 1018，且无 `ClientTick`/`LoginNoticeOk` 状态 | `inbound::INBOUND_IDENTS` 不含它；`GateEvent::Command` 的 `else if` 链无它；`SessionGround` 无该字段 | **成立**（§2.2）|
| P6 | `spell_version_gate` 已实现但**无生产调用点** | `mir2_world::magic::spell_version_gate`；生产路径 `handle_spell_cmd` 只在注释里登记 | **成立**（§2.5）|
| P7 | 现有 e2e 仍绿不证明 1018 被处理：判据只查 `658…46` 前缀，且客户端不依赖服务端回帧 | `check_client_s2c_frames.py` 的 `ENTER_SEQ`；Rust 捕获/log | **成立**（§2.6）|
| P8 | 推荐方案 | B1（只记 tick + 全量门、零出帧）；A 另开 R 窗口卡；C 是偏离需白名单 | **推荐 B1**（§3.4）|

---

## 1. oracle 流程（逐行实测）

### 1.1 状态字段与初值

| 字段 | 声明 | 初值/默认 | 语义 |
| --- | --- | --- | --- |
| `IsSendNotice` | `PlayObject.cs:604` | `false`（`:912`）| 658 是否已发过 |
| `WaitLoginNoticeOkTick` | `PlayObject.cs:605` | `0`（int）| 发 658 的时刻（`GetTickCount`）|
| `LoginNoticeOk` | `PlayObject.cs:606` | `false`（`:907`）| 是否已收到 1018（或 switch-data 直接置真）|
| `BoReadyRun` | `PlayObject.cs:335` | `false`（`:885`）| 是否已 `UserLogon()`；未就绪时 `ProcessUserMessage` 在 `WorldServer.cs:860-863` 早退 |
| `ClientTick` | `PlayObject.cs:810` | `0`（short）| **只**由 1018 写（`:1112`），被 `MagicManager.cs:167` 读 |
| `SoftVersionDateEx` | `PlayObject.cs:613` | `0`（`:937`），登录时由 `LoadUser.SoftVersionDate` 赋值（`WorldServer.cs:469`）| 客户端版本；`0` 会关掉 11029/5008（`PlayObject.Base.cs:930/943`）与「双 0」版本门 |
| `BoEmergencyClose` | `PlayObject.cs:364` | `false`（`:880`）| 超时/踢人标志；下游见 §1.5 |
| `RunTime` | `PlayObject.cs:899` | `250`（ms）| `ProcessPlayObjectData` 的 per-object 节流；`RunNotice` 最快 250ms 一次 |

### 1.2 `RunNotice` 的时间线（`PlayObject.cs:1079-1123`）

```text
WorldServer.ProcessPlayObjectData (:625) 每轮对每个 PlayObject：
  :650  if (!LoginNoticeOk)
  :652      RunNotice()
  :656  else if (!BoReadyRun) { :658 BoReadyRun = true; :659 UserLogon(); }
  :697  else { … SearchViewRange/GameTimeChanged/LineNotice/Run()/周期存档 … }

RunNotice (:1079):
  :1082  if (BoEmergencyClose || BoKickFlag || BoSoftClose)
  :1084      if (BoKickFlag) SendDefMessage(SM_OUTOFCONNECTION,…)
  :1088      MakeGhost();                    // 见 §1.5
  :1094  else if (!IsSendNotice)
  :1096      SendNotice();                  // 658
  :1097      IsSendNotice = true;
  :1098      WaitLoginNoticeOkTick = now;
  :1100  else
  :1102      if (now - WaitLoginNoticeOkTick > 10*1000) BoEmergencyClose = true;
  :1106      ProcessMessage msg; while (GetMessage(ref msg))
  :1109          if (msg.wIdent == CM_LOGINNOTICEOK) {
  :1111              LoginNoticeOk = true;
  :1112              ClientTick = (short)msg.nParam1;
  :1113              SysMsg(ClientTick.ToString(), MsgColor.Red, MsgType.Notice);
  :1115          }                            // 非 1018 的消息全部丢弃（见 §1.4）
```

- **658 在 `UserLogon()` 之前**：这是为什么金标准里 658 是进图帧序的第一条（`c3` seq14）。
- **`WaitLoginNoticeOkTick` 起算**在 658 发出之后；10s 判定在**下一次** `RunNotice`（≥250ms 后）。
- **同一 tick 不会既发 658 又进世界**：`if/else` 互斥；真 oracle 捕获的 `658 < 1018 < 51` 间隔见 §1.8。

### 1.3 `SendNotice()` 与 658（`PlayObject.cs:1063-1077`）

1. `M2Share.NoticeMgr.GetNoticeMsg("Notice", loadList)`（`:1067`）：
   - `NoticeManager.GetNoticeMsg`（`src/M2Server/Notices/NoticeManager.cs:50-101`）先按名字 `Notice` 找缓存槽；
     没有则读 `M2Share.GetNoticeFilePath("Notice.txt")`（`M2Share.cs:1988-1995` = `<BasePath>/<NoticeDir>/Notice.txt`；
     `NoticeDir` 缺省 `Notice`，`GameSvrConf.cs:1297`），按 `StringList.LoadFromFile` 逐行读入（GBK 原文）。
2. 逐行拼 `line + "\x20\x1B\x1B"`（`:1073`；源码字面量里第 3 个字节是**裸 ESC**，见 `xxd` 见证），
   **最后一行后也跟一个分隔符**；随后 `sNoticeMsg.Replace("/r/n/r/n ", "")`——**字面正斜杠**的 oracle 笔误。
3. `SendDefMessage(SM_SENDNOTICE=658, 2000, 0, 0, 0, sNoticeMsg)`（`:1076`）：
   - `Recog` 恒 **2000**，`Param/Tag/Series` 全 0；
   - 尾 = `EDCode.EncodeString(sNoticeMsg)`（`PlayObject.Socket.cs:27-43`，`EDcode.cs:175-185`）；
   - 空文本仍发 **header-only 658**（`SendDefMessage` 的空尾支走 `SendSocket(CommandMessage)`，
     `PlayObject.Socket.cs:37-42/77-99`）。
4. Rust 侧已逐字节实现：`game_svr::notice::{load_notice, NOTICE_SEP, NOTICE_REPLACE_FROM}` 与
   `game_svr::smframe::sm_sendnotice`（本件只引符号）。

### 1.4 `SysMsg` 的帧编码/颜色——以及它为什么到不了线上

`SysMsg(sMsg, MsgColor.Red, MsgType.Notice)`（`BaseObject.cs:1588-1683`）：

1. `MsgType.Notice` 支（`:1590`）；`switch (sMsg[0])`（`:1596`）对 `"0"` 走 `default`（`:1633`）。
2. `MsgColor.Red`（`:1636-1642`）：若 `ShowPreFixMsg`（缺省 `false`，`GameSvrConf.cs:1751`）则前置
   `LineNoticePreFix`（缺省 `〖公告〗`，`GameSvrConf.cs:1841`）；随后
   `SendMsg(RM_SYSMESSAGE=11004, 0, RedMsgFColor=0xFF, RedMsgBColor=0x38, 0, sMsg)`
   （颜色缺省 `GameSvrConf.cs:1540-1541`；`BaseObject.Message.cs:34` 的 `SendMsg` 形参序）。
3. `RM_SYSMESSAGE` 的客户端化（`PlayObject.Message.cs:1696/1714-1716`）：
   `MakeMessage(SM_SYSMESSAGE=100, ActorId, MakeWord(0xFF,0x38), 0, 1)` + 尾 `EncodeString(sMsg)` ⇒
   `Param = 0x38FF = 14591`（低字节=前景、高字节=背景）、`Series=1`、`Recog=ActorId`。
4. **关键实证（P4）**：`SysMsg` 的 `SendMsg` 只是把 `RM_SYSMESSAGE` **入队**
   （`BaseObject.Message.cs:181`，优先级 = `(byte)wIdent` = 252）。但 `RunNotice` 的
   `while (GetMessage(ref msg))`（`PlayObject.cs:1106-1115`）**继续出队**；刚入队的 252 会在下一轮被取出，
   由于 `wIdent != CM_LOGINNOTICEOK` 被直接丢弃（`GetMessage(ref)` 语义见 `BaseObject.Message.cs:334-355`）。
   ⇒ **oracle 正常登录不产生这条 100 帧**。三份 C# 捕获在 **game hop（7200 s2c）** 上的 100 帧各 4 条，
   全是 `@Login` 脚本的绿色 `Param=65499` 文本，无 `Param=14591`、无正文 `"0"`（§1.8 表）。
   （`c3` 另有 3 条 `port=7100`/`hop=sel` 的 **c2s** `ident=100`，属另一协议面，不是 `SM_SYSMESSAGE`。）
   - 唯一能让它存活的理论分支：队列前端恰有一条 `DeliveryTime > now` 的延时消息，使 `GetMessage` 返回
     `false`、循环提前退出（`BaseObject.Message.cs:339-343`）。登录瞬间没有这种消息。
5. 同一循环还会**丢弃等待期内收到的所有非 1018 消息**（移动/说话等）；客户端在收到 658 前尚未进世界，
   实际不会发这些消息，故正常路径不可见——但方案 A 若要 1:1，必须连这个「丢弃」一起搬（见 §3.1/§5）。

### 1.5 10s 超时与 `BoEmergencyClose` 的下游

- `> 10*1000`（严格大于）⇒ `BoEmergencyClose = true`（`PlayObject.cs:1102-1105`），**同一次**仍继续 drain。
- **竞态（1018 与超时同批）**：超时置 `BoEmergencyClose` 后，若 1018 在**同一次** `RunNotice` 的 drain 中到达，
  它仍会置 `LoginNoticeOk`。下一轮 `ProcessPlayObjectData` 因 `LoginNoticeOk=true` 先走 `UserLogon()`（`:650-659`，
  **仍发 51…46**）；`MakeGhost()` **不是**在「下一次 `RunNotice` 的首行 `:1082`」执行（那条路径已不可达），
  而是由随后 `PlayObject.Run()` 的 `BoEmergencyClose` 支（`PlayObject.Message.cs:367-390`）执行，
  之后才轮到 `ProcessPlayObjectData` 的 Ghost 清理支（`WorldServer.cs:707-717`）。
- `MakeGhost()`：`PlayObject.MakeGhost`（`:3422-3529`）→ `base.MakeGhost`（`BaseObject.Base.cs:484-496`）
  置 `Ghost=true`（或 `CanReAlive` 时 `Invisible`）、记 `GhostTick`、`DisappearA()`（从视野消失）。
- 下一轮 `ProcessPlayObjectData` 走 `else`（`WorldServer.cs:707-717`）：
  `PlayObjectList.Remove` → `Disappear()` → `AddToHumanFreeList`（`:1094-1098`，`GhostTick=now`）→
  `SaveHumanRcd`（`:713`）→ **`M2Share.NetChannel.CloseUser(GateIdx, SocketId)`**（`:714`）→ `ISM_USERLOGOUT`。
- `PlayObjectFreeList` 在 `GhostTick + HumanFreeDelayTime`（**5 分钟**，`GameSvrConf.cs:1485`）后才移除
  （`WorldServer.cs:586-590`）。
- **不发** `SM_OUTOFCONNECTION(528)`：`RunNotice` 只在 `BoKickFlag` 支发（`:1084-1087`）；
  `PlayObject.Run` 的同类分支也在 `BoKickFlag` 才发（`PlayObject.Message.cs:367-390`）。故超时是**静默踢出世界 + 关连接**。
- 另有一条别路：`KickPlayObjectEx`（`WorldServer.cs:958-970`）也只是置 `BoEmergencyClose=true`，没有立即帧。

### 1.6 `WorldServer` 的两条进世界路径与主循环

`ProcessHumansMakeNewHuman`（`WorldServer.cs:221`）在 `ProcessHumans`（`:481`，调用点 `:518`）里执行：

| 路径 | 判据 | 对 `LoginNoticeOk` 的处理 | 结果 |
| --- | --- | --- | --- |
| **首次登录 / 普通登录** | `switchDataInfo == null`（`:246`；`GetSwitchData` 只在 `ChangeServerList` 命中时非 null，见 `WorldServer.Switch.cs:7-19`）| **不置真**；只 `BoReadyRun=false`（`:405`）| 走 658→1018 等待环 |
| **跨服 switch-data** | `switchDataInfo != null`（`:410` 起）| `BoReadyRun=false; LoginNoticeOk=true; TryPlayMode=true`（`:449-451`）| **不发 658**（`RunNotice` 根本不被调用），下一轮直接 `UserLogon()` |

> **对任务书的订正**：`WorldServer.cs:440-455` 的 `LoginNoticeOk = true` 属**跨服 switch-data 支**，
> 不是「新角色路径」。首次/普通登录（`switchDataInfo == null`）恰恰**必须**等 1018；金标准里的普通登录
> 收到 658 且随后才 51…46（§1.8）。

主循环（`WorldServer.cs:625-737`）的相关点：
- `:650-659` 的 `LoginNoticeOk → BoReadyRun → UserLogon()`；
- `UserLogon()`（`PlayObject.cs:1152`）先 `SendPriorityMsg(RM_LOGON)`（`:1176`），由 `RM_LOGON` 处理器
  （`PlayObject.Message.cs:1680-1691`）发 `51/50/41/213/11029/42/766/54/5008/1106`；然后 `UserLogon` 后段
  `:1385-1388` 发 `52/752/811/46`。
- `:860-863`：`!BoReadyRun` 时 `ProcessUserMessage` 在**入队之后**早退（消息留待 `Run()` 处理）；
- `:697` 才进入 `Run()`（`PlayObject.Message.cs:19` 的 override）与 `:698-703` 的周期存档。
- `LoginNoticeOk && BoReadyRun` 是周期存档所在最内层 `else` 的前置（`:656-703`）；A 方案会改变这段计时起点。

### 1.7 `DoSpell` 版本门与调用方回帧

`MagicManager.DoSpell`（`MagicManager.cs:145`）的语句序：

| 行 | 语句 | 失败后果 |
| --- | --- | --- |
| `:151-153` | `if (IsWarrSkill(MagIdx)) return false;` | 调用方回 `RM_MAGICFIREFAIL` |
| `:155-157` | 距离门 `abs(Δx)>MagicAttackRage \|\| abs(Δy)>… ⇒ return false` | 调用方回一帧 |
| `:159` | `SendRefMsg(RM_SPELL, Effect, nTargetX, nTargetY, MagicId, "")` | **无条件、先于版本门** |
| `:160-163` | 死亡目标归一（`targetObject = null`）| 无帧 |
| **`:167`** | `if (playObject.SoftVersionDateEx == 0 && playObject.ClientTick == 0 \|\| userMagic.Magic.MagicId > 80) return false;` | **调用方回一帧** |
| `:172-643` | `switch (MagicId)` 效果体（含 RNG/落库）| — |
| `:644-670` | 共享尾：`RM_MAGICFIRE`/训练尾/`return true` | — |

调用方（`PlayObject.Attack.cs` 的 `default` 支）：`:1272-1277` 先 snap 目标坐标，`:1278` 调 `DoSpell`，
`:1279-1281` `if (!DoSpell(...)) SendRefMsg(RM_MAGICFIREFAIL, 0, 0, 0, 0, "")`。
`RM_MAGICFIREFAIL`（`Messages.cs:583`=10121）的处理器（`PlayObject.Message.cs:1794-1796`）**无**「自己不发」守卫，
且 `SendRefMsg` 把该消息发给同图所有 `Play`（含施法者自己）⇒ **施法者与同屏观察者各收到一条**
`SM_MAGICFIRE_FAIL(639, Messages.cs:301)`（`Recog` = 施法者）。观察者另收到 `:159` 先发的 `SM_SPELL(17)`
（`RM_SPELL` 处理器在 `PlayObject.Message.cs:1454-1459` 有 `processMsg.ActorId != ActorId` 的 self 守卫，故 17 **不**回给施法者）。

**门的实际可观测性**（本件的关键推理）：
- 冻结客户端 1018 的 `Recog=0` ⇒ `ClientTick=0`（P3）；
- 真 oracle 捕获里 `11029/5008` 存在 ⇒ `SoftVersionDateEx != 0`（二帧的门在 `PlayObject.Base.cs:930/943`）；
- 因此「双 0」子句在该客户端上**恒不成立**，`:167` 实际只剩 `MagicId > 80` 一条会拒。
- `ClientTick` 全仓唯一读点就是 `:167`；`SoftVersionDateEx` 另有两个帧门（`:930/943`）与一条提示（`PlayObject.cs:1332`）。

### 1.8 线上证据（真 oracle / 真客户端）

**帧序（同一条 conn）**：

| 捕获 | 658 | 1018（c2s）| 51 | 结论 |
| --- | --- | --- | --- | --- |
| `mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl` | seq14 `t=…503870` | seq15 `t=…504853`（+983ms）| seq16 `t=…505681`（+828ms）| **658 < 1018 < 51** |
| `…/client-e2e-enter-gaps-20261013/run-20261015-b-csharp-capture.jsonl` | seq2 `t=…097562` | seq3 `t=…098950`（+1388ms）| seq4 `t=…099671`（+721ms）| **658 < 1018 < 51** |
| `…/run-20261015-a-csharp-capture.jsonl` | seq2 `t=1792040424202` | seq3 `t=1792040425464`（+1262ms）| seq4 `t=1792040426257`（+793ms）| **658 < 1018 < 51** |
| `…/run-20261015-a-rust-capture.jsonl`（宿主）| seq2 `t=…323665` | seq22 `t=…324152`（+487ms）| seq3 `t=…323665`（**同 ms**）| 658 与 51…46 同批，1018 后到 |

**1018 的字段**（c3 seq15 / run-b seq3，两者 `raw_hex` 相同）：
`recog=0, ident=1018, param=0, tag=0, series=46, body_len=0`。c2s 线缆是 `#` + `1` + EDCode(12B 头) + `!`；
把首字符后的 16 个编码字符解码得 `(0, 1018, 0, 0, 46)`（本件手工复核，与捕获工具解析一致）。
按 `WorldServer.cs:856-858` 的 default 映射 `nParam1 = Recog` ⇒ oracle `ClientTick = (short)0 = 0`。

**100 帧计数**（证明 P4）：在 **game hop（`port=7200`，`dir=s2c`）** 上，三份 C# 捕获（`c3`、
`run-20261015-a-csharp-capture.jsonl`、`run-20261015-b-csharp-capture.jsonl`）的 `ident==100` 各 4 条，
`param` 全为 `65499`（`MakeWord(0xDB,0xFF)`，`@Login` 脚本绿色文本），正文为公告/攻击模式提示；
**无** `param=14591`（`MakeWord(0xFF,0x38)`），也**无**正文 `"0"`。`c3` 另有 3 条 `port=7100`/`hop=sel` 的 c2s `ident=100`
（不同协议面），不计入。
所以 `SysMsg(ClientTick.ToString(), Red, Notice)` 在 oracle 线上不可见。

> **证据卫生**：`capture-client-default-oracle40.jsonl` 的 658 与 51 同 ms、1018 后到，与宿主行为一致；
> 其来源是 `README.md` §⑤ 的 **Rust 侧 Run A**（文件名里的 `oracle40` 指 40B 能力布局，不是 oracle 服务端），
> **不得**拿它当 oracle 帧序见证。oracle 帧序只认 `c3` / `run-a`(csharp) / `run-b`（三者均 `658 < 1018 < 51`）。

---

## 2. Rust 现状（只引符号/函数名）

### 2.1 进世界路径

- `bins/game-svr/src/main.rs::sim_loop` 的 `GateEvent::LoadDb`/认证完成路径：读档成功 ⇒
  `world.open_session` → `new_player_entity` → `world.enter_map` → `online_set` → 装配
  `smframe::EnterWorldInputs`（`enter::apply` 补世界/配置/记录层输入）→ 建 `SessionGround` →
  `route_to_gate` 后**一次性**发送 `inputs.frames()`，再 `SimToGate::MarkInWorld`。
- `smframe::EnterWorldInputs::frames` 的序为 `658, 51, 50, 41, 213, 11029, 42, 766, 54, 5008, 1106, 52, 752, 811, 46`；
  第一条恒为 `smframe::sm_sendnotice`（空公告也发 header-only）。
- **没有** `LoginNoticeOk`/`IsSendNotice`/`WaitLoginNoticeOkTick`/`ClientTick`/`BoReadyRun` 这些登录等待状态；
  `SessionGround` 里只有 `bo_soft_close`/`bo_emergency_close`/`save_rcd_tick` 等运行期字段。
  `ground_sessions.insert` 与 `MarkInWorld` 在帧发完之后同步完成 ⇒ 宿主语义等价于「一次摊平的 UserLogon」。
- 结果：宿主 658 与 51…46 同批（`run-a` 捕获同 ms），远早于客户端 1018（+487ms）。

### 2.2 `CM_LOGINNOTICEOK(1018)` 的入站派发现状

- 常量存在：`mir2_protocol::messages::CM_LOGINNOTICEOK`（值 1018）。
- **未登记**：`game_svr::inbound::INBOUND_IDENTS` 是 10 条数组，**不含** 1018；
  `sim_loop` 的 `else if` 派发链（`chat::CM_SAY` → `ground::CM_PICKUP/DROPITEM` → `CM_DEAL*` → … →
  `inbound::INBOUND_IDENTS.contains(&cmd.ident)`）没有 1018 的臂。
- 于是 1018 只命中 `GateEvent::Command` 开头那条通用 `DBG` 日志，然后**静默落空**（不是显式 ignore，
  也不是显式登记）。机证：`run-20261015-a-rust-game-svr.out.txt` 与 `run-20261015-b-rust-game-svr.out.txt`
  都有 `sim: index=1 命令 Recog=0 Ident=1018 Param=0 Tag=0 Series=46 tail=""`，前后无 WRN/ERR、无出帧。
- 若要把 1018 接进 `handle_inbound_cmd`，注意它目前还兼任「`online` 登记表查找」的早退门；
  1018 到达时宿主早已 `online_set`，正常路径可用。建议**不要**改 `INBOUND_IDENTS` 的语义（其文档写死
  「R1 余 10 条」），而是在 `sim_loop` 里加一条**独立** `else if cmd.ident == CM_LOGINNOTICEOK`，
  逻辑落到 `inbound` 的一个纯函数（便于单测），见 §3.2。

### 2.3 `SoftVersionDateEx` 在宿主是否可得

**可得**，且已在生产路径：
- `game_svr::cert::CertParse::client_version`（认证串的第 4 段，`StrToInt`；测试断言 120020522）→
  `load::LoadDbInfo::soft_version_date` → `main.rs::sim_loop` 的 `soft_version_date` 局部量。
- 进世界时同时喂给：
  - `enter::WorldFrameInputs::soft_version_date`（决定 11029/5008 是否发，`PlayObject.Base.cs:930/943` 等价物）；
  - `SessionGround::soft_version_date`（运行期门，元宝寄售等复用）。
- 因此版本门所需的第一个入参**现成**，不需要新查询。
- 注意区分：`bins/login-srv/src/state.rs::ClientUserInfo.client_tick` 是**登录服自己的时间戳**，
  与 `PlayObject.ClientTick` 无关，也从不在 game-svr 里出现。

### 2.4 `SysMsg` / 639 的出站编码器

- `SM_SYSMESSAGE(100)`：`game_svr::smframe::sm_sysmessage(recog, f_color, b_color, msg)` 已存在
  （header: `MakeWord(f,b)`、`series=1`、尾 `edcode::encode`），生产上已被账号级顶号使用。
- `MsgType.Notice` 的完整编码分支：`game_svr::line_notice::build_frame` / `sys_msg_notice`
  （`[`/`<`/`{` 三分支与 default 支），可作为 §1.4 的等价参照。
- 颜色：`GameSvrConf::red_msg_f_color`/`red_msg_b_color`（缺省 0xFF/0x38）。
- 639：`game_svr::smframe_ext::sm_magicfire_fail(actor_id)`（header-only）；
  现有距离门失败用 `broadcast_ref_msg_with_self` 发它（`handle_spell_cmd`）。

### 2.5 `spell_version_gate` 的状态

- 纯函数已实现：`mir2_world::magic::spell_version_gate(soft_version_date_ex: i32, client_tick: i16, magic_id: i32) -> bool`，
  语义 `!((soft==0 && tick==0) || magic_id > 80)`。
- 单测已覆盖四种组合：`crates/world/tests/spell_prereq_vectors.rs`（0,0 拒 / 1,0 过 / 0,1 过 / 81 拒 / 80 过）。
- **生产路径无调用点**：`handle_spell_cmd` 在无条件 `RM_SPELL` 广播后直接进入效果体；模块注释明确登记
  「`:167` 版本门未接」。`SessionGround` 也没有 `client_tick` 可传。

### 2.6 为什么现有 e2e 仍然绿（代码证 + 机证）

1. **判据只查前缀**：`check_client_s2c_frames.py` 从首个 658 截到首个 46，要求 `ENTER_SEQ` 前缀；
   `658,51,50,…,46` 在宿主里同批且顺序不变 ⇒ 前缀成立。它不要求 1018 出现在中间，也不要求时序。
2. **客户端不依赖服务端的 ack 回帧**：真 oracle 线上本来也**没有** ClientTick 的 100 帧（§1.4/§1.8）；
   客户端收到 658 后自行发 1018，然后照常处理随后到达的 51…46。宿主只是把 51…46 提前发了。
3. **版本门不受影响**：该客户端 `ClientTick=0`，但 `SoftVersionDateEx != 0`（11029/5008 已发），
   `spell_version_gate` 的「双 0」子句恒不成立；四份 run capture（`run-a`/`run-b` 的 csharp 与 rust）中
   没有任何 `CM_SPELL`（`ident=3017`，实测均 0 条）。
4. **证据**：`run-a` 宿主捕获 `658/51/50/…/46` 同 ms、1018 后到；`run-a` 服务端日志把 1018 记一行 DBG 后无动作；
   两侧客户端均能走、能打（`check_attack_observability.py` 等既有判据）。
   ⇒ 「忽略 1018」在当前判据面**不可见**，不是「已正确处理」。

---

## 3. 建模方案 A/B/C

### 3.1 方案 A：完全复刻等待环（新增状态机 + 10s 超时踢人）

**形状**（全部按符号描述）：
- 新状态：`IsSendNotice`、`WaitLoginNoticeOkTick`、`LoginNoticeOk`、`BoReadyRun`、`ClientTick`；
  可挂在 `SessionGround` 或一个新的 `LoginGate` 会话态里。
- 时序改为：读档 → 建会话/实体 → **只发 658** → 等 1018（最多 10s）→ 再 `world.enter_map` + 发 51…46 + `MarkInWorld`；
  或者保留 `enter_map` 提前、但把 51…46 与 `MarkInWorld` 推迟到 ack 后（两者对世界可见性的差异需 owner 裁决）。
- `RunNotice` 等价物每 ≥250ms 驱动一次；`CM_LOGINNOTICEOK` 到达时置 `LoginNoticeOk` 并写 `ClientTick=(short)recog`；
  非 1018 的入站消息在等待期**丢弃**（§1.4 的副作用）。
- 10s 超时 ⇒ 之后一轮 `MakeGhost` 等价：从世界移除、关网关连接、存档、`ISM_USERLOGOUT`；
  不发 `SM_OUTOFCONNECTION`。宿主已有 `bo_emergency_close` 字段与 `drain_kicks` 类停机路径，但**不能**直接等同：
  oracle 的 Ghost/FreeList 5 分钟语义在宿主没有对应物。
- **时序竞态（必须进 A 的时序表）**：超时判定先置 `BoEmergencyClose`、随后同一次 drain 读到 1018 时，
  下一轮仍会 `UserLogon()`（51…46 照发），`MakeGhost()` 由随后 `PlayObject.Run()` 的 `BoEmergencyClose` 支
  （`PlayObject.Message.cs:367-390`）执行，**不是**「下一次 `RunNotice` 首行（`:1082`）」；J6 A2b 覆盖。

**对现有 e2e**：
- 帧**集合**不变（`658`、`51…46` 仍在），但 `658 → 51` 之间多出一次客户端 RTT（实测 0.72–1.39s）；
  现有 `check_client_s2c_frames.py` 的前缀判据仍绿；
- 任何「同 ms 连发」「进世界时刻」类时序判据会变；在线登记/存档计时/世界 tick 的起点也要跟着改。
- **判级 R**（时序 + 世界进出 + 存档/关连接 + 可能影响周期存档起点）；风险最高、改动面最大。

**结论**：**当前不做**。它是严格 parity 的最终形态，但需要单开 R 窗口卡，且必须把「等待期丢弃非 1018 消息」
与「超时静默踢出」一起搬，否则只是「延迟发送」而非等价。

### 3.2 方案 B：只接 1018 设 `ClientTick`（不阻塞进世界）

拆成两个子案：

**B1（推荐）：状态 + 全量版本门，零出帧**
- `SessionGround` 增 `client_tick: i16`（初值 0）；
- `sim_loop` 增独立 `else if cmd.ident == CM_LOGINNOTICEOK`：`client_tick = cmd.recog as i16`（C# `(short)` 截断），
  无会话 ⇒ WRN 丢弃；**不发任何帧**；
- `handle_spell_cmd` 在 `default_arm && !magic::is_warr_skill(...)` 内、**无条件 `sm_spell` 广播之后**、
  效果体之前，调 `mir2_world::magic::spell_version_gate(sg.soft_version_date, sg.client_tick, magic_id)`；
  失败 ⇒ `sm_magicfire_fail` + `broadcast_ref_msg_with_self`，**每个收件人恰一条 639**（施法者 1×639；同屏观察者
  1×17 + 1×639），并**提前返回**（跳过 14/15/31、17/30/41、20、46 等效果体）。
- **对现有 e2e**：真客户端 `soft != 0`、`tick=0`、`magic<=80` ⇒ 门恒过，帧面/序**逐字节不变**；
  只有构造场景（`soft=0 && tick=0`，或 `magic>80`）会新增 `639`。
- **非阻塞窗口（显式登记，见 §5.10）**：宿主在 1018 到达前已发完 51…46 并允许客户端行动，`client_tick` 此时仍为 0。
  对 `soft != 0` 的冻结客户端无影响（门恒过）。但 `soft == 0 && recog != 0` 的构造存在时序分叉：
  oracle 的客户端要收到 51…46 才能行动，而 1018 已先于 51…46 被处理（`ClientTick=recog != 0`，门过）；
  宿主在 1018 前收到的 `CM_SPELL` 只能看到 `client_tick=0`（与 `soft==0` 组成双 0，门拒 ⇒ 639）。
  B1 的最小口径：1018 前的 `CM_SPELL` 按当前 `client_tick=0` 照常过门（不排队、不丢弃），并登记该窗口；
  要消掉该分叉只能选 A（等 ack 再发 51…46），或在 B1 额外把 1018 前的 `CM_SPELL` 延后到 1018 之后处理（单开卡）。
- **判级 R（保守）**：它决定施法是否发生（含后续 RNG/落库），按「战斗数值/判不准 ⇒ R」处理；
  但可在 review 里用「真客户端恒过」的证明把 R 窗口缩到两条构造用例。

**B2（不推荐）：B1 + 按 `RunNotice` 字面回 SysMsg**
- 1018 到达时再发 `sm_sysmessage(actor_id, red_f, red_b, edcode::encode(ClientTick.to_string()))`。
- **问题**：oracle 自己不发这条帧（§1.4/§1.8 的 drain 吞帧）⇒ 每登录一次就多一条 `100`，
  帧面与 oracle 不等价；判级 R，且需要一个白名单/owner 裁决才能接受。
- 若 owner 坚持「源码字面」而非「线上字节」，可以选 B2，但本件**明确不推荐**。

### 3.3 方案 C：只把版本门简化成 `MagicId > 80`

- 不新增任何状态；把 `spell_version_gate` 的调用（或函数体）改成只看 `magic_id > 80`。
- **对现有 e2e**：真客户端同样恒过 ⇒ 帧面不变。
- **问题**：`soft==0 && ClientTick==0` 时 oracle 拒绝**所有**技能，C 会放行 `magic<=80` ⇒ 主动偏离
  `MagicManager.cs:167`；将来任何「旧版本客户端/版本串缺失」场景都会与 oracle 分叉（含 RNG 流水）。
- **判级 R**（有意偏离，非等价）；必须登记 `T-*`/`D-*` 白名单并写改前/改后。**不推荐**。

### 3.4 对比与推荐

| 维度 | A（完整等待环）| B1（状态+门，零出帧）| B2（B1+SysMsg）| C（只留 `>80`）|
| --- | --- | --- | --- | --- |
| 新增状态 | `IsSendNotice`/`Wait…`/`LoginNoticeOk`/`BoReadyRun`/`ClientTick` | `client_tick` | 同 B1 | 无 |
| 阻塞进世界 | 是（≤10s）| 否 | 否 | 否 |
| 等待期丢消息 | 必须 | 不需要 | 不需要 | 不需要 |
| 10s 踢人 | 必须 | 不涉及 | 不涉及 | 不涉及 |
| 现有 e2e 帧集合 | 不变（时序变）| **不变** | 每次登录多一条 `100` | 不变 |
| 现有 e2e 前缀判据 | 仍绿 | 仍绿 | 仍绿（100 在 46 之后）| 仍绿 |
| 与 oracle 字节等价 | 是（若连丢消息/踢人一起搬）| 是（真客户端路径；构造场景也照 `:167`）| **否**（多一帧）| **否**（双 0 支 fail-open）|
| 判级 | R | R（保守；可论证缩窗）| R | R（偏离需白名单）|
| 推荐 | 缓做（单独 R 窗口卡）| **推荐** | 不推荐 | 不推荐 |

**推荐 B1**，理由：以最小写集补齐「`:167` 在宿主无调用点」这一实质缺口，且对已验证的冻结客户端逐字节零变化；
A 作为后续严格 parity 卡（它需要重排进世界与在线登记，风险远大于收益）；C 违反「行为等价优先」。

---

## 4. 机判验收与改坏必红（若落地 B1；B2/A 的附加项单列）

> 以下判据在**实现卡**里执行；本件只定义口径。Rust 侧引用符号，不引行号。

### J1 — 1018 写入 `ClientTick`（B1 必选）

- **布置**：一个已进世界的会话（`ground_sessions` 有 `SessionGround`）；构造 `CommandMessage{ident: CM_LOGINNOTICEOK, recog: 1234, series: 46}`。
- **断言**：`SessionGround.client_tick == 1234`；本次派发**零出帧**（无 `SM_SYSMESSAGE`、无 639）。
- **改坏必红**：
  1. 去掉 1018 的 `else if`/match 臂 ⇒ `client_tick == 0` ⇒ RED；
  2. 误用 `cmd.series`（本例 46）⇒ `client_tick == 46` ⇒ RED（这条专门咬住真客户端的 `Series=46`）；
  3. 存成 i32 不截断 ⇒ 用 `recog = 65541` 断言 `client_tick == 5` ⇒ RED。

### J2 — C# `(short)` 截断（B1 必选）

- `recog = 65541`（0x1_0005）⇒ `client_tick == 5`；`recog = -1` ⇒ `-1`。
- 变异：改成 `u16`/饱和转换 ⇒ RED。

### J3 — SysMsg 帧（**仅 B2**，反 parity 判据）

- **断言**：1018（`recog=1234`）后恰一条 `SM_SYSMESSAGE(100)`：
  `recog = actor_id`、`param = make_word(0xFF, 0x38) = 0x38FF`、`tag=0`、`series=1`、
  尾 = `edcode::encode(b"1234")`。
- **改坏必红**：颜色改绿（0xDB/0xFF）⇒ RED；发两条 ⇒ RED；正文发 `""`/`"0"` 以外的内容 ⇒ RED。
- **同时必须写明**：oracle 捕获里没有这条帧（§1.8），所以 J3 只证明「按源码字面」，不证明「与 oracle 等价」。

### J4 — `MagicId > 80` ⇒ 恰一条 639（B1 必选）

- **布置**：会话 `soft_version_date = 120020522`、`client_tick = 0`；魔表构造一条 `magic_id = 81` 的项；
  施法目标在 `MagicAttackRage` 内（走 `default_arm`）。
- **断言（施法者 socket）**：恰一条 `SM_MAGICFIRE_FAIL(639)`，header-only；**没有** `SM_SPELL(17)` 给施法者。
- **断言（同屏观察者）**：恰一条 `SM_SPELL(17)`（oracle `:159` 在 `:167` 之前）**加**恰一条 `SM_MAGICFIRE_FAIL(639)`
  （`Recog=施法者`；`SendRefMsg(RM_MAGICFIREFAIL)` 发给同图所有 `Play`，其处理器无 self 守卫）。
- **改坏必红**：
  1. 去掉版本门 ⇒ 施法者 0 帧（无 639）⇒ RED；
  2. 把门挪到 `sm_spell` 广播之前 ⇒ 观察者 0 帧（丢 17）⇒ RED；
  3. 在距离门失败时也再发一次 639（双发）⇒ RED。

### J5 — `SoftVersionDateEx == 0 && ClientTick == 0` 子句（B1 必选；需两侧构造）

- **Rust 单测**：`spell_version_gate(0,0,1) == false`、`(0,1,1) == true`（已有 `spell_prereq_vectors` 覆盖，保留为牙）。
- **Rust 生产路径**：`soft=0, tick=0, magic=1` ⇒ 恰一条 639；随后派发 1018 `recog=1`，再施同技能 ⇒ **不再有** 639。
- **1018 前窗口（B1 必登记）**：`soft=0`，在 1018 尚未到达时先发 `CM_SPELL(magic<=80)` ⇒ 按 §5.10 的选定口径
  断言（B1 最小口径 = 恰一条 639，因为此刻 `client_tick=0` 与 `soft=0` 组成双 0）；随后发 1018 `recog=1`，再施 ⇒ 门过。
  该用例同时证明「宿主提前放行 51…46」不会把 `soft==0` 的构造洗绿。
- **改坏必红**：删掉 `soft==0 && tick==0` 子句（= 方案 C）⇒ 第一次施法无 639 ⇒ RED。这条判据就是否决 C 的机器依据。
- **两侧构造（未完成，见 §5）**：C# 侧需要一个 `SoftVersionDateEx == 0` 的真会话（登录版本串 0）或等价夹具，
  证明 oracle 在第一次施法回 639、第二次不回。没有该窗口时 `review` 必须写 `window_required: true` 并列出缺的证据。

### J6 — 方案 A 的附加判据（若改选 A）

- A1：同一连接上 `ts(658) < ts(1018) < ts(51)`，且 `ts(51) - ts(1018) >= 0`、`ts(658) < ts(51)`；
  改坏（不等待直接发 51）⇒ RED。
- A2：不发 1018 的构造会话，>10s 后：无 51…46、世界会话被移除、网关连接被关、存档已发；
  **无** `SM_OUTOFCONNECTION(528)`；变异（发 528 或只断 socket 不落存档）⇒ RED。
- A2b（超时/1018 同批竞态）：超时判定先置 `BoEmergencyClose`，同一次 `RunNotice` drain 中 1018 到达 ⇒ 下一轮
  `UserLogon()`（51…46 仍发），`MakeGhost()` 由随后 `PlayObject.Run()` 的 `BoEmergencyClose` 支执行，
  再进入 Ghost 清理（`WorldServer.cs:707-717`）；变异（把 `MakeGhost` 固定在「下一次 `RunNotice` 首行」⇒ 因
  `LoginNoticeOk=true` 该路径不可达，`MakeGhost` 永不执行、会话卡在 `BoEmergencyClose`）⇒ RED。
- A3：等待期内发送一条非 1018 的 `CM_*` ⇒ 被丢弃（不进世界、无回帧）；变异（照常处理）⇒ RED。
- A4：switch-data 路径（宿主目前不建模跨服）不在本卡范围；若将来接入，须断言**不发 658**、直接 `UserLogon`（`WorldServer.cs:449-451`）。

### J7 — 回归/门禁（全部方案必选）

- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、
  `cargo run -p msgcodegen -- --check`（协议常量不加/不改）。
- `check_client_s2c_frames.py --capture <真机 capture> --expect-52-body-len <N> --min-appearance-pairs <M>` 仍 GREEN
  （B1 不改帧序/帧体）。
- 既有 `spell_prereq_vectors` 全绿；新增 J5 后仍全绿。
- 变异总牙：把 `client_tick` 的默认值改成 1 ⇒ J5 的 `soft=0,tick=0` 用例构造不出来 ⇒ RED（防止「默认开」把门洗绿）。

---

## 5. 未决项（不静默）

1. **`SendNotice` 的内容来源与客户端依赖**：内容 = `<BasePath>/<NoticeDir>/Notice.txt`（GBK，逐行 + 3B 分隔符）；
   客户端是否**必须**非空、是否解析分隔符、空公告是否只显示空框——无真机对照（现有捕获都有 320B 公告）。
   需要一条「空 `Notice.txt`」窗口或冻结客户端源码（本仓库无 `MirClient` 源码）。
2. **10s 超时的真机行为**：现有所有捕获的 ack 都在 0.98–1.71s，从未触发超时；oracle 代码是静默
   `MakeGhost`+`CloseUser`+存档（§1.5）。客户端在超时后的表现（断线提示/自动重连）**未证**。
3. **1018 的 `Series=46` 含义**：本件只证「oracle 不读它」。冻结客户端是否把它当 tick、以及是否有客户端会把
   tick 放进 `Recog`（那才会真的让 `ClientTick != 0`）——未证。需要 Delphi 侧 `ClMain`/`PlayScn` 的发送点人证/代码证。
4. **`SoftVersionDateEx == 0` 的生产可达性**：认证串第 4 段缺失/为 0 时 `cert.client_version=0`；
   login-srv/db-srv 是否会把这种会话放进世界——未做端到端构造。J5 的两侧判据依赖它。
5. **证据卫生**：`capture-client-default-oracle40.jsonl` 是 **Rust Run A**（README §⑤），不是 oracle；
   若后续有人拿它推 oracle 的 `658→1018` 时序，会得到相反结论。建议在证据目录加一行来源标注（非本卡写集）。
6. **A 的「等待期丢弃非 1018」**：oracle 的 `while (GetMessage)` 会把等待期所有非 1018 消息丢掉（§1.4）；
   A 必须一起搬，否则只是延迟发送。该副作用没有现成宿主结构，需单独设计。
7. **B2 的 owner 裁决**：若坚持「源码字面」回 SysMsg，需要明确接受「每次登录多一条 `100`」的帧面偏离；
   否则按 B1 不发。这是产品/判据取舍，不是技术未知。
8. **A 的进世界重排**：`enter_map`/`online_set`/`SessionGround`/`MarkInWorld` 的相对顺序与 oracle
   `ProcessHumansMakeNewHuman`→`UserLogon` 的对应关系需要一张单独时序表；本件只界定到「必须等 ack 再发 51…46」。
9. **冻结客户端源码**：`MirClient` 不在本仓库；「客户端是否等待服务端对 1018 的任何响应」目前只有
   **捕获级**证据（oracle 不回帧、客户端仍继续），缺源码级人证。
10. **B1 的 1018 前施法窗口**：见 §3.2 B1。宿主不阻塞进世界，1018 前 `client_tick` 恒 0；`soft != 0` 时无差异，
    但 `soft == 0 && recog != 0` 的构造会出现「oracle 处理施法时 `ClientTick=recog`（门过）/ 宿主 1018 前处理时
    `client_tick=0`（与 `soft==0` 双 0，门拒）」的分叉：宿主多一条 639、oracle 则走完整效果帧。落地 B1 必须三选一写进实现卡：(a) 照常按当前 tick 过门、
    接受该构造分叉（本件推荐的最小写集）；(b) 把 1018 前的 `CM_SPELL` 延后到 1018 之后处理（更接近 oracle 时序，
    但引入队列/顺序语义）；(c) 改选 A（1018 前不允许行动）。未裁决前 `review` 按 R，并要求 J5 的 1018 前窗口用例覆盖。

---

## 6. 自失效声明（落地后必须改写的句子）

| 句子/位置 | 在什么落地后失效 | 应改写为 |
| --- | --- | --- |
| §0 P5、§2.2「宿主未登记/未处理 1018、无 `ClientTick`」 | B1/B2/A 任一落地 | 「1018 已接线；`ClientTick` 存于 `<符号>`；零/一条出帧」 |
| §2.5「`spell_version_gate` 无生产调用点」 | B1/B2/C 任一落地 | 「生产调用点 = `handle_spell_cmd` 的 `<位置>`」 |
| §2.6「e2e 仍绿因为 1018 被忽略」 | B1/B2/A 落地 | 改为「已接线后的回归证据」；旧解释只作历史 |
| §3.2 B1 的「零出帧」 | B2 被选中 | 改为「1018 ⇒ 一条 `SM_SYSMESSAGE`（含颜色/正文），并注明与 oracle 捕获不等价」 |
| §1.4/§1.8「oracle 线上没有 ClientTick 提示帧」 | **不随实现失效**（oracle 事实）；但若 B2 落地，本件所有「帧面等价」措辞失效 | 保留 oracle 事实，另加「宿主有意多发一帧」的偏离登记 |
| §3.1「宿主把等待环摊平」、§2.1「同批发送 658…46」 | A 落地 | 改为「658 → 等 1018 → 51…46」的实测时序 |
| §3.4「推荐 B1」 | owner 裁决/实现卡立项 | 改为「已按 `<决定>` 落地」并链接实现卡 |
| §5.1/5.2/5.4 的「未证」 | 对应真机窗口完成 | 填读数与证据路径 |

---

## 7. 复核命令（oracle 行号逐条实测）

```bash
cd <repo>   # 本件基线 0c4fe0e5d（复核时）
# 1) RunNotice / SendNotice / 状态字段
grep -n "private void SendNotice\|public void RunNotice\|public short ClientTick\|public bool IsSendNotice\|public int WaitLoginNoticeOkTick\|public bool LoginNoticeOk\|public bool BoReadyRun" src/M2Server/Player/PlayObject.cs
grep -n "CM_LOGINNOTICEOK\|SysMsg(ClientTick" src/M2Server/Player/PlayObject.cs
grep -n "IsSendNotice = false\|LoginNoticeOk = false\|BoReadyRun = false\|RunTime = 250" src/M2Server/Player/PlayObject.cs
# 2) WorldServer 两路径 + 主循环 + default 映射
grep -n "private IPlayerActor ProcessHumansMakeNewHuman\|if (switchDataInfo == null)\|LoginNoticeOk = true\|RunNotice()\|UserLogon()\|if (!playObject.BoReadyRun)" src/GameSrv/Word/WorldServer.cs
sed -n '856,863p' src/GameSrv/Word/WorldServer.cs
# 3) 版本门 + 调用方回帧
grep -n "public static bool DoSpell\|SoftVersionDateEx == 0 && playObject.ClientTick == 0\|MagicId > 80\|SendRefMsg(Messages.RM_SPELL" src/M2Server/Magic/MagicManager.cs
grep -n "if (!DoSpell\|RM_MAGICFIREFAIL" src/M2Server/Player/PlayObject.Attack.cs
# 4) SysMsg 编码 + 消息号
grep -n "public void SysMsg\|case MsgColor.Red\|SendMsg(Messages.RM_SYSMESSAGE" src/M2Server/Actor/BaseObject.cs
grep -n "case Messages.RM_SYSMESSAGE\|MakeMessage(Messages.SM_SYSMESSAGE" src/M2Server/Player/PlayObject.Message.cs
grep -n "CM_LOGINNOTICEOK =\|SM_SYSMESSAGE =\|SM_SENDNOTICE =\|SM_MAGICFIRE_FAIL =" src/OpenMir2/Messages.cs
# 5) 公告来源/分隔符
sed -n '1063,1077p' src/M2Server/Player/PlayObject.cs | xxd        # 期望 5c783230 5c783142 1b
grep -n "GetNoticeFilePath\|NoticeDir =\|RedMsgFColor =\|RedMsgBColor =\|ShowPreFixMsg =\|LineNoticePreFix =" src/M2Server/M2Share.cs src/Modules/SystemModule/Conf/Model/GameSvrConf.cs
# 6) Rust 侧只认符号（不取行号）
grep -rn "CM_LOGINNOTICEOK\|INBOUND_IDENTS\|spell_version_gate\|client_tick\|sm_sysmessage\|sm_magicfire_fail\|soft_version_date" mir2-rs/bins/game-svr/src mir2-rs/crates/world/src mir2-rs/crates/protocol/src
```

若上述 grep 与正文不符 ⇒ **源码已漂移**，以 grep 结果为准并改本件。

---

## 附录 A：oracle 引用总表

| 主题 | 引用 |
| --- | --- |
| 公告读取/缓存 | `src/M2Server/Notices/NoticeManager.cs:50-101`；`M2Share.cs:1988-1995`；`GameSvrConf.cs:1297` |
| 658 发送 | `PlayObject.cs:1063-1077`（`:1067/:1073/:1076`）|
| 等待环/超时/1018 | `PlayObject.cs:1079-1123`（`:1094-1099/:1102-1105/:1106-1115`）|
| 默认字段 | `PlayObject.cs:880/885/899/907/912/937`；属性 `:335/364/604/605/606/810/613` |
| 进世界两路径 | `WorldServer.cs:221/246/405/410-452/449-451` |
| 主循环/UserLogon | `WorldServer.cs:625/650-659/697-703/707-717/860-863`；`PlayObject.cs:1152/1176/1385-1388` |
| 跨服 switch data | `WorldServer.Switch.cs:7-19` |
| Ghost/FreeList/CloseUser | `BaseObject.Base.cs:484-496`；`WorldServer.cs:586-590/713-715/1094-1098`；`GameSvrConf.cs:1485` |
| SysMsg | `BaseObject.cs:1588-1683`（`:1590/:1596/:1636-1642`）；`PlayObject.Message.cs:1696/1714-1716` |
| 消息队列/优先级 | `BaseObject.Message.cs:34/181/334-355` |
| 帧编码 | `PlayObject.Socket.cs:17-43/77-99/101-129`；`EDcode.cs:175-185`；`ClientMesaagePacket.cs:11-24` |
| 消息号 | `Messages.cs:73/215/301/320/583/646` |
| 颜色/前缀默认 | `GameSvrConf.cs:1540-1541/1751/1841`；`MsgColor.cs:3-9`；`MsgType.cs:3-13` |
| 版本门/调用方 | `MagicManager.cs:145/151-157/159/167/172`；`PlayObject.Attack.cs:1269-1281` |
| 11029/5008 的 soft 门 | `PlayObject.Base.cs:930/943` |
| 踢人旁路 | `WorldServer.cs:958-970` |

## 附录 B：证据文件（本件引用）

- `mir2-rs/tests/golden/c-line-baseline-c3-20261010.jsonl`（seq14/15/16；1018 字段）。
- `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/run-20261015-b-csharp-capture.jsonl`
  （真 oracle：seq2/3/4；100 帧 seq40-43）。
- `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/run-20261015-a-csharp-capture.jsonl`
  （真 oracle：seq2/3/4；658 `t=1792040424202` < 1018 `t=1792040425464` < 51 `t=1792040426257`；100 帧 seq24-27）。
- `mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/run-20261015-a-rust-capture.jsonl`
  （宿主：seq2/3/16/22）。
- `…/run-20261015-a-rust-game-svr.out.txt` / `…/run-20261015-b-rust-game-svr.out.txt`
  （1018 仅 DBG 一行、无出帧）。
- `…/check_client_s2c_frames.py`（`ENTER_SEQ` 前缀判据；脚本把首个 46 硬编码为截断点，实际参数只有
  `--capture` / `--expect-52-body-len` / `--min-appearance-pairs` / `--json` / `--selftest`，**无** `--upto-ident`）。
- `…/README.md` §⑤（Run A/B 的栈与 capture 来源；`oracle40` = 40B 布局）。
- `…/85-csharp-capture-topology-rootcause.md`（capture 拓扑旧归因订正；与本件的 1018 时序无关，只作背景）。
